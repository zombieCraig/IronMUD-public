//! Rhai bindings for vampire state on mobiles and characters.
//!
//! Two parallel surfaces — mob-side (`*_mobile_*`) and PC-side
//! (`*_pc_*`) — because Rhai doesn't see them through the same id
//! type. Mob ids are uuid strings, PC ids are connection_id strings;
//! conflating them in one polymorphic fn led to subtle bugs in the
//! summon/charm work upstream.
//!
//! Surface kept narrow on purpose: anything Rhai can do with normal
//! getters/setters on `vampire_state` belongs there. These free fns
//! exist for high-frequency, cross-cutting checks (sun tick, combat
//! tick, dialogue conditions) and the embrace lifecycle.
//!
//! Embrace itself happens here in `embrace_pc` and `embrace_mobile`
//! so quest/admin/creation paths share one definition.
//!
//! See `src/types/vampire.rs` for the underlying struct.
//!
//! ## Mob ids
//! Mob ids are passed in as uuid strings (matching the rest of the
//! mobile-script surface). Empty string / unparseable input returns
//! `false` / 0 / unit, never panics.

use crate::SharedConnections;
use crate::db::Db;
use crate::types::{CharacterData, SkillProgress, VampireState};
use rhai::Engine;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Clan acknowledgment uplift: when a thinblood claims a clan, max blood
/// pool snaps to this value and current blood refills to it.
pub const CLAN_BLOOD_POOL_MAX: i32 = 10;

/// Thinblood starting blood pool. Reduced from the kindred default so the
/// thinblood pro/con tradeoff carries weight. Used by the auto-create path.
pub const THINBLOOD_BLOOD_POOL_MAX: i32 = 6;

/// Disciplines whose `skill_required` is at or above this value are tier-3
/// powers — locked from thinbloods. Lifted on clan acknowledgment.
pub const THINBLOOD_TIER_LOCK: i32 = 5;

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// True iff the character is embraced but carries no `clan_*` trait. The
/// thinblood state is purely derived — no field on `VampireState` flags it.
pub fn is_pc_thinblood(ch: &CharacterData) -> bool {
    ch.vampire_state.is_some() && pc_clan_from_traits(ch).is_none()
}

/// Returns the clan id (e.g. "brujah") if any `clan_*` trait is present,
/// else None. Multiple clan traits would be a builder error; we just take
/// the first.
pub fn pc_clan_from_traits(ch: &CharacterData) -> Option<String> {
    ch.traits
        .iter()
        .find_map(|t| t.strip_prefix("clan_").map(str::to_string))
}

/// Look up the first preferred discipline for a clan from
/// `scripts/data/vampire_clans.json`. Returns None if the file is missing
/// or the clan id isn't listed. Used to seed the 1-dot starter discipline
/// when a thinblood is acknowledged or a vampire migrant is rolled.
pub fn first_preferred_discipline_for_clan(clan: &str) -> Option<String> {
    let raw = std::fs::read_to_string("scripts/data/vampire_clans.json").ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let entry = parsed.get(clan)?;
    let arr = entry.get("preferred_disciplines")?.as_array()?;
    arr.iter().find_map(|v| v.as_str().map(str::to_string))
}

/// Enumerate clan ids known to `scripts/data/vampire_clans.json`. Skips
/// underscore-prefixed metadata keys (e.g. `_doc`) so callers don't treat
/// them as clans. Returns the canonical core five if the file is missing
/// or unparseable so vampire migrant rolls always have something to pick.
pub fn list_clan_ids() -> Vec<String> {
    if let Ok(raw) = std::fs::read_to_string("scripts/data/vampire_clans.json") {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(obj) = parsed.as_object() {
                let ids: Vec<String> = obj
                    .keys()
                    .filter(|k| !k.starts_with('_'))
                    .cloned()
                    .collect();
                if !ids.is_empty() {
                    return ids;
                }
            }
        }
    }
    vec![
        "brujah".to_string(),
        "toreador".to_string(),
        "ventrue".to_string(),
        "nosferatu".to_string(),
        "gangrel".to_string(),
    ]
}

/// Apply the clan-acknowledgment side-effects to an already-embraced
/// character. Idempotent: the trait is added if missing, the starter
/// discipline is seeded only when not already known, sire_id is updated
/// only when an explicit sire is supplied, and blood_pool is uplifted to
/// `CLAN_BLOOD_POOL_MAX` only on the first acknowledgment.
///
/// Returns `true` if anything changed and the character should be saved
/// by the caller.
pub fn apply_clan_acknowledgment(
    ch: &mut CharacterData,
    clan: &str,
    sire: Option<String>,
) -> bool {
    let v = match ch.vampire_state.as_mut() {
        Some(v) => v,
        None => return false,
    };
    let clan_trim = clan.trim().to_lowercase();
    if clan_trim.is_empty() {
        return false;
    }
    let mut changed = false;

    let trait_id = format!("clan_{}", clan_trim);
    if !ch.traits.iter().any(|t| t == &trait_id) {
        ch.traits.push(trait_id);
        changed = true;
    }

    if v.max_blood_pool < CLAN_BLOOD_POOL_MAX {
        v.max_blood_pool = CLAN_BLOOD_POOL_MAX;
        changed = true;
    }
    if v.blood_pool < CLAN_BLOOD_POOL_MAX {
        v.blood_pool = CLAN_BLOOD_POOL_MAX;
        changed = true;
    }

    if let Some(sire_name) = sire {
        let trimmed = sire_name.trim();
        if !trimmed.is_empty() {
            // Replace the placeholder "(no sire)" or any prior sire string.
            let new_sire = trimmed.to_string();
            if v.sire_id.as_ref().map(|s| s != &new_sire).unwrap_or(true) {
                v.sire_id = Some(new_sire);
                changed = true;
            }
        }
    }

    if let Some(skill_key) = first_preferred_discipline_for_clan(&clan_trim) {
        let key = skill_key.to_lowercase();
        let entry = ch.skills.entry(key).or_insert(SkillProgress::default());
        if entry.level < 1 {
            entry.level = 1;
            changed = true;
        }
    }

    changed
}

/// Halve humanity loss for thinbloods (integer division — base=1 yields 0
/// for the newbie-vampire forgiveness window). Returns the actual amount
/// deducted. Routes all "bad-act" humanity adjustments through one place
/// so the pro is automatic across all callers.
pub fn apply_humanity_loss(ch: &mut CharacterData, base: i32) -> i32 {
    if base <= 0 {
        return 0;
    }
    let v = match ch.vampire_state.as_mut() {
        Some(v) => v,
        None => return 0,
    };
    let actual = if is_pc_thinblood_state(v, &ch.traits) {
        base / 2
    } else {
        base
    };
    if actual > 0 {
        v.change_humanity(-actual);
    }
    actual
}

/// Inner is_pc_thinblood that takes the (already-borrowed) state + traits.
/// Used inside `apply_humanity_loss` where we already hold a `&mut` on the
/// vampire state.
fn is_pc_thinblood_state(_v: &VampireState, traits: &[String]) -> bool {
    !traits.iter().any(|t| t.starts_with("clan_"))
}

pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections) {
    // ========== Mob-side ==========

    // is_mobile_vampire(mobile_id) -> bool
    let cdb = db.clone();
    engine.register_fn("is_mobile_vampire", move |mobile_id: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(m)) = cdb.get_mobile_data(&uuid) {
                return m.vampire_state.is_some() || m.flags.vampire;
            }
        }
        false
    });

    // is_mobile_masquerade_broken(mobile_id) -> bool. False for mortals.
    let cdb = db.clone();
    engine.register_fn(
        "is_mobile_masquerade_broken",
        move |mobile_id: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(m)) = cdb.get_mobile_data(&uuid) {
                    return m
                        .vampire_state
                        .as_ref()
                        .map(|v| v.masquerade_broken)
                        .unwrap_or(false);
                }
            }
            false
        },
    );

    // get_mobile_blood_pool(mobile_id) -> i64. Returns 0 for mortals.
    let cdb = db.clone();
    engine.register_fn(
        "get_mobile_blood_pool",
        move |mobile_id: String| -> i64 {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(m)) = cdb.get_mobile_data(&uuid) {
                    return m.vampire_state.as_ref().map(|v| v.blood_pool as i64).unwrap_or(0);
                }
            }
            0
        },
    );

    // set_mobile_blood_pool(mobile_id, n) -> bool
    let cdb = db.clone();
    engine.register_fn(
        "set_mobile_blood_pool",
        move |mobile_id: String, n: i64| -> bool {
            let uuid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut mobile = match cdb.get_mobile_data(&uuid) {
                Ok(Some(m)) => m,
                _ => return false,
            };
            let v = match mobile.vampire_state.as_mut() {
                Some(v) => v,
                None => return false,
            };
            v.blood_pool = n.clamp(0, v.max_blood_pool as i64) as i32;
            cdb.save_mobile_data(mobile).is_ok()
        },
    );

    // get_mobile_humanity(mobile_id) -> i64. Returns -1 for mortals so callers
    // can distinguish "not a vampire" from "humanity = 0 (the beast wins)".
    let cdb = db.clone();
    engine.register_fn(
        "get_mobile_humanity",
        move |mobile_id: String| -> i64 {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(m)) = cdb.get_mobile_data(&uuid) {
                    return m.vampire_state.as_ref().map(|v| v.humanity as i64).unwrap_or(-1);
                }
            }
            -1
        },
    );

    // set_mobile_humanity(mobile_id, n) -> bool. Clamps to [0, 10].
    let cdb = db.clone();
    engine.register_fn(
        "set_mobile_humanity",
        move |mobile_id: String, n: i64| -> bool {
            let uuid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut mobile = match cdb.get_mobile_data(&uuid) {
                Ok(Some(m)) => m,
                _ => return false,
            };
            let v = match mobile.vampire_state.as_mut() {
                Some(v) => v,
                None => return false,
            };
            v.set_humanity(n as i32);
            cdb.save_mobile_data(mobile).is_ok()
        },
    );

    // embrace_mobile(mobile_id, sire_name, clan) -> bool
    //
    // Stamps a fresh VampireState + flags.{vampire, undead, holy_vulnerable}.
    // Clan is optional (empty allowed). Mob clans aren't trait-tagged because
    // mobs don't share the player trait pool — clan is decoration on the
    // mob, mechanically expressed via flags + builder-set discipline skills.
    let cdb = db.clone();
    engine.register_fn(
        "embrace_mobile",
        move |mobile_id: String, sire: String, _clan: String| -> bool {
            let uuid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut mobile = match cdb.get_mobile_data(&uuid) {
                Ok(Some(m)) => m,
                _ => return false,
            };
            let sire_opt = if sire.trim().is_empty() {
                None
            } else {
                Some(sire)
            };
            mobile.vampire_state = Some(VampireState::newly_embraced(now_secs(), sire_opt));
            mobile.flags.vampire = true;
            mobile.flags.undead = true;
            mobile.flags.holy_vulnerable = true;
            cdb.save_mobile_data(mobile).is_ok()
        },
    );

    // revoke_mobile_vampirism(mobile_id) -> bool. Clears state + flags.
    let cdb = db.clone();
    engine.register_fn(
        "revoke_mobile_vampirism",
        move |mobile_id: String| -> bool {
            let uuid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut mobile = match cdb.get_mobile_data(&uuid) {
                Ok(Some(m)) => m,
                _ => return false,
            };
            if mobile.vampire_state.is_none() && !mobile.flags.vampire {
                return false;
            }
            mobile.vampire_state = None;
            mobile.flags.vampire = false;
            // Leave undead/holy_vulnerable alone — they're independent flags
            // a builder may want to keep on a non-vampire undead.
            cdb.save_mobile_data(mobile).is_ok()
        },
    );

    // ========== PC-side ==========

    // is_pc_vampire(connection_id) -> bool
    let conns = connections.clone();
    engine.register_fn("is_pc_vampire", move |connection_id: String| -> bool {
        let conn_id = match uuid::Uuid::parse_str(&connection_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let conns_lock = match conns.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        conns_lock
            .get(&conn_id)
            .and_then(|s| s.character.as_ref())
            .map(|c| c.vampire_state.is_some())
            .unwrap_or(false)
    });

    // get_pc_blood_pool(connection_id) -> i64
    let conns = connections.clone();
    engine.register_fn(
        "get_pc_blood_pool",
        move |connection_id: String| -> i64 {
            let conn_id = match uuid::Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return 0,
            };
            let conns_lock = match conns.lock() {
                Ok(g) => g,
                Err(_) => return 0,
            };
            conns_lock
                .get(&conn_id)
                .and_then(|s| s.character.as_ref())
                .and_then(|c| c.vampire_state.as_ref())
                .map(|v| v.blood_pool as i64)
                .unwrap_or(0)
        },
    );

    // get_pc_humanity(connection_id) -> i64. -1 for mortals.
    let conns = connections.clone();
    engine.register_fn(
        "get_pc_humanity",
        move |connection_id: String| -> i64 {
            let conn_id = match uuid::Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return -1,
            };
            let conns_lock = match conns.lock() {
                Ok(g) => g,
                Err(_) => return -1,
            };
            conns_lock
                .get(&conn_id)
                .and_then(|s| s.character.as_ref())
                .and_then(|c| c.vampire_state.as_ref())
                .map(|v| v.humanity as i64)
                .unwrap_or(-1)
        },
    );

    // change_pc_humanity(connection_id, delta) -> i64. Returns new value
    // (-1 if not a vampire). Saves the character.
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn(
        "change_pc_humanity",
        move |connection_id: String, delta: i64| -> i64 {
            let conn_id = match uuid::Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return -1,
            };
            let mut conns_lock = match conns.lock() {
                Ok(g) => g,
                Err(_) => return -1,
            };
            let session = match conns_lock.get_mut(&conn_id) {
                Some(s) => s,
                None => return -1,
            };
            let ch = match session.character.as_mut() {
                Some(c) => c,
                None => return -1,
            };
            let v = match ch.vampire_state.as_mut() {
                Some(v) => v,
                None => return -1,
            };
            let new_val = v.change_humanity(delta as i32);
            let _ = cdb.save_character_data(ch.clone());
            new_val as i64
        },
    );

    // change_pc_blood_pool(connection_id, delta) -> i64. Clamped to
    // [0, max_blood_pool]. -1 if not a vampire.
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn(
        "change_pc_blood_pool",
        move |connection_id: String, delta: i64| -> i64 {
            let conn_id = match uuid::Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return -1,
            };
            let mut conns_lock = match conns.lock() {
                Ok(g) => g,
                Err(_) => return -1,
            };
            let session = match conns_lock.get_mut(&conn_id) {
                Some(s) => s,
                None => return -1,
            };
            let ch = match session.character.as_mut() {
                Some(c) => c,
                None => return -1,
            };
            let v = match ch.vampire_state.as_mut() {
                Some(v) => v,
                None => return -1,
            };
            let new_val = (v.blood_pool as i64 + delta).clamp(0, v.max_blood_pool as i64);
            v.blood_pool = new_val as i32;
            let _ = cdb.save_character_data(ch.clone());
            new_val
        },
    );

    // embrace_pc(connection_id, sire_name, clan) -> bool
    //
    // Three-state: mortal -> thinblood, mortal -> clan-acknowledged, or
    // thinblood -> clan-acknowledged. Repeated calls with empty clan on an
    // existing thinblood return false (no-op). Empty sire is allowed.
    //
    // Mortal + clan provided: stamps VampireState with `max_blood_pool=10`,
    // applies clan acknowledgment (trait + skill seed + blood refill).
    // Mortal + clan empty: stamps VampireState with `max_blood_pool=6`
    // (thinblood). Auto-create takes this path.
    // Thinblood + clan provided: clan acknowledgment uplift in place.
    // Already-acknowledged + any clan: no-op (returns false).
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn(
        "embrace_pc",
        move |connection_id: String, sire: String, clan: String| -> bool {
            let conn_id = match uuid::Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut conns_lock = match conns.lock() {
                Ok(g) => g,
                Err(_) => return false,
            };
            let session = match conns_lock.get_mut(&conn_id) {
                Some(s) => s,
                None => return false,
            };
            let ch = match session.character.as_mut() {
                Some(c) => c,
                None => return false,
            };
            let clan_trim = clan.trim().to_lowercase();
            let sire_opt = if sire.trim().is_empty() {
                None
            } else {
                Some(sire.trim().to_string())
            };

            let mut changed = false;

            if ch.vampire_state.is_none() {
                let mut state = VampireState::newly_embraced(now_secs(), sire_opt.clone());
                if clan_trim.is_empty() {
                    state.max_blood_pool = THINBLOOD_BLOOD_POOL_MAX;
                    state.blood_pool = THINBLOOD_BLOOD_POOL_MAX;
                }
                ch.vampire_state = Some(state);
                changed = true;
            }

            // Already a vampire (just embraced above or pre-existing): if a
            // clan was supplied, apply the acknowledgment. If clan empty
            // and the character was already a vampire, return false.
            if !clan_trim.is_empty() {
                if pc_clan_from_traits(ch).is_some() {
                    // Already clan-acknowledged. Refuse silently to avoid
                    // overwriting an existing clan via repeat embrace.
                    return false;
                }
                if apply_clan_acknowledgment(ch, &clan_trim, sire_opt) {
                    changed = true;
                }
            } else if !changed {
                // Was already a thinblood, no clan provided — no-op.
                return false;
            }

            cdb.save_character_data(ch.clone()).is_ok() && changed
        },
    );

    // is_thinblood(connection_id) -> bool
    let conns = connections.clone();
    engine.register_fn("is_thinblood", move |connection_id: String| -> bool {
        let conn_id = match uuid::Uuid::parse_str(&connection_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let conns_lock = match conns.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        conns_lock
            .get(&conn_id)
            .and_then(|s| s.character.as_ref())
            .map(is_pc_thinblood)
            .unwrap_or(false)
    });

    // get_pc_clan(connection_id) -> String
    // Returns the clan id ("brujah", "toreador", ...) or empty string when
    // the player has no clan trait (mortal or thinblood).
    let conns = connections.clone();
    engine.register_fn("get_pc_clan", move |connection_id: String| -> String {
        let conn_id = match uuid::Uuid::parse_str(&connection_id) {
            Ok(u) => u,
            Err(_) => return String::new(),
        };
        let conns_lock = match conns.lock() {
            Ok(g) => g,
            Err(_) => return String::new(),
        };
        conns_lock
            .get(&conn_id)
            .and_then(|s| s.character.as_ref())
            .and_then(pc_clan_from_traits)
            .unwrap_or_default()
    });

    // claim_clan_for_pc(connection_id, sire, clan) -> bool
    // Quest-reward path: assumes the player is already embraced (typically
    // a thinblood). No-op for mortals and for vampires who already carry
    // a clan trait. Saves the character on success.
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn(
        "claim_clan_for_pc",
        move |connection_id: String, sire: String, clan: String| -> bool {
            let conn_id = match uuid::Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut conns_lock = match conns.lock() {
                Ok(g) => g,
                Err(_) => return false,
            };
            let session = match conns_lock.get_mut(&conn_id) {
                Some(s) => s,
                None => return false,
            };
            let ch = match session.character.as_mut() {
                Some(c) => c,
                None => return false,
            };
            if ch.vampire_state.is_none() || pc_clan_from_traits(ch).is_some() {
                return false;
            }
            let sire_opt = if sire.trim().is_empty() {
                None
            } else {
                Some(sire.trim().to_string())
            };
            if !apply_clan_acknowledgment(ch, &clan, sire_opt) {
                return false;
            }
            cdb.save_character_data(ch.clone()).is_ok()
        },
    );

    // set_pc_max_blood_pool(connection_id, n) -> bool
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn(
        "set_pc_max_blood_pool",
        move |connection_id: String, n: i64| -> bool {
            let conn_id = match uuid::Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut conns_lock = match conns.lock() {
                Ok(g) => g,
                Err(_) => return false,
            };
            let session = match conns_lock.get_mut(&conn_id) {
                Some(s) => s,
                None => return false,
            };
            let ch = match session.character.as_mut() {
                Some(c) => c,
                None => return false,
            };
            let v = match ch.vampire_state.as_mut() {
                Some(v) => v,
                None => return false,
            };
            v.max_blood_pool = n.max(0) as i32;
            if v.blood_pool > v.max_blood_pool {
                v.blood_pool = v.max_blood_pool;
            }
            cdb.save_character_data(ch.clone()).is_ok()
        },
    );

    // set_pc_blood_pool(connection_id, n) -> bool
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn(
        "set_pc_blood_pool",
        move |connection_id: String, n: i64| -> bool {
            let conn_id = match uuid::Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut conns_lock = match conns.lock() {
                Ok(g) => g,
                Err(_) => return false,
            };
            let session = match conns_lock.get_mut(&conn_id) {
                Some(s) => s,
                None => return false,
            };
            let ch = match session.character.as_mut() {
                Some(c) => c,
                None => return false,
            };
            let v = match ch.vampire_state.as_mut() {
                Some(v) => v,
                None => return false,
            };
            v.blood_pool = n.clamp(0, v.max_blood_pool as i64) as i32;
            cdb.save_character_data(ch.clone()).is_ok()
        },
    );

    // masquerade_reset_pc(connection_id) -> bool. Clears the masquerade_broken
    // flag without otherwise modifying the character. Used by admin tooling
    // and the future masquerade-cleanup quest.
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn(
        "masquerade_reset_pc",
        move |connection_id: String| -> bool {
            let conn_id = match uuid::Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut conns_lock = match conns.lock() {
                Ok(g) => g,
                Err(_) => return false,
            };
            let session = match conns_lock.get_mut(&conn_id) {
                Some(s) => s,
                None => return false,
            };
            let ch = match session.character.as_mut() {
                Some(c) => c,
                None => return false,
            };
            let v = match ch.vampire_state.as_mut() {
                Some(v) => v,
                None => return false,
            };
            if !v.masquerade_broken {
                return false;
            }
            v.masquerade_broken = false;
            cdb.save_character_data(ch.clone()).is_ok()
        },
    );

    // vampire_feed_on_mobile(connection_id, mobile_id) -> Map
    //
    // Vampire (PC) feeds on a mobile in their room. Atomic: rolls bite damage,
    // drains HP from target, adds blood to caster, stamps puncture wound.
    //
    // Returns a Map describing the result:
    //   success:  bool      — true when the feed actually landed
    //   damage:   i64       — HP removed from the target (0 on failure)
    //   blood:    i64       — blood pool added to caster
    //   killed:   bool      — target dropped to 0 hp from this feed
    //   masquerade_break: bool — set when this feed trips the masquerade flag
    //   humanity_loss: i64  — humanity lost (0 unless lethal-on-mortal-non-consent)
    //   error:    String    — non-empty when success=false (caller renders to player)
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn(
        "vampire_feed_on_mobile",
        move |connection_id: String, mobile_id: String| -> rhai::Map {
            use rand::Rng;
            let mut out = rhai::Map::new();
            let put = |m: &mut rhai::Map, k: &str, v: rhai::Dynamic| {
                m.insert(k.into(), v);
            };
            let fail = |reason: &str| -> rhai::Map {
                let mut m = rhai::Map::new();
                put(&mut m, "success", rhai::Dynamic::from(false));
                put(&mut m, "damage", rhai::Dynamic::from(0i64));
                put(&mut m, "blood", rhai::Dynamic::from(0i64));
                put(&mut m, "killed", rhai::Dynamic::from(false));
                put(&mut m, "masquerade_break", rhai::Dynamic::from(false));
                put(&mut m, "humanity_loss", rhai::Dynamic::from(0i64));
                put(&mut m, "error", rhai::Dynamic::from(reason.to_string()));
                m
            };

            let conn_id = match uuid::Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return fail("invalid connection id"),
            };
            let mob_uuid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return fail("invalid mobile id"),
            };

            // Pull a mutable session reference once and hold it through the
            // entire transaction so the caster's blood update can't race with
            // a discipline cast or another feed.
            let mut conns_lock = match conns.lock() {
                Ok(g) => g,
                Err(_) => return fail("session lock poisoned"),
            };
            let session = match conns_lock.get_mut(&conn_id) {
                Some(s) => s,
                None => return fail("not logged in"),
            };
            let ch = match session.character.as_mut() {
                Some(c) => c,
                None => return fail("no character"),
            };
            if ch.vampire_state.is_none() {
                return fail("you have no fangs");
            }

            // Load target.
            let mut target = match cdb.get_mobile_data(&mob_uuid) {
                Ok(Some(m)) => m,
                _ => return fail("target not found"),
            };
            if target.is_prototype {
                return fail("target is a prototype");
            }
            if Some(ch.current_room_id) != target.current_room_id {
                return fail("target is not in your room");
            }
            if target.vampire_state.is_some() || target.flags.vampire {
                return fail("vampires have no blood worth taking");
            }
            if target.flags.no_attack {
                return fail("target cannot be attacked");
            }
            if target.current_hp <= 0 {
                return fail("target is already dead");
            }

            // Roll bite damage: 1d4. Capped by target's remaining HP so we
            // can never overdrain.
            let mut rng = rand::thread_rng();
            let roll: i32 = rng.gen_range(1..=4);
            let damage = roll.min(target.current_hp);

            // Blood gained: 4 per HP drained, capped by caster's missing pool.
            let (blood_gained, post_blood) = {
                let v = ch.vampire_state.as_ref().unwrap();
                let missing = (v.max_blood_pool - v.blood_pool).max(0);
                let gained = (damage * 4).min(missing);
                (gained, v.blood_pool + gained)
            };

            target.current_hp -= damage;
            let killed = target.current_hp <= 0;

            // Wound stamping — puncture on the neck, severity scales with
            // bite outcome. We don't loop existing wounds; a fresh feed
            // pushes a new wound entry just like combat puncture hits.
            let bleeding = if killed { 2 } else { 1 };
            target.wounds.push(crate::types::Wound {
                body_part: crate::types::BodyPart::Neck,
                level: if killed {
                    crate::types::WoundLevel::Moderate
                } else {
                    crate::types::WoundLevel::Minor
                },
                wound_type: crate::types::WoundType::Puncture,
                bleeding_severity: bleeding,
            });

            // Masquerade trip: lethal feeding is uniformly visible.
            // Per-area witness ledger is a future refinement; for MVP we
            // always trip on lethal feeding regardless of who's watching.
            let mut masquerade_break = false;
            if killed {
                if let Some(v) = ch.vampire_state.as_mut() {
                    if !v.masquerade_broken {
                        v.masquerade_broken = true;
                        masquerade_break = true;
                    }
                }
            }

            // Humanity loss: lethal, mortal target who isn't bloodfeed_willing
            // and isn't aggro (vampire hunters / wild beasts excused). PC
            // bloodfeed_willing is checked by name lookup if the target is
            // a player character — for mob targets, there's no consent flag.
            // Routed through `apply_humanity_loss` so thinbloods get the
            // half-loss pro automatically (base=1 → 0 for thinbloods).
            let mut humanity_loss = 0i64;
            if killed && !target.flags.aggressive {
                humanity_loss = apply_humanity_loss(ch, 1) as i64;
            }

            // Apply blood to caster.
            if let Some(v) = ch.vampire_state.as_mut() {
                v.blood_pool = post_blood;
            }

            let _ = cdb.save_character_data(ch.clone());
            let _ = cdb.save_mobile_data(target);

            put(&mut out, "success", rhai::Dynamic::from(true));
            put(&mut out, "damage", rhai::Dynamic::from(damage as i64));
            put(&mut out, "blood", rhai::Dynamic::from(blood_gained as i64));
            put(&mut out, "killed", rhai::Dynamic::from(killed));
            put(
                &mut out,
                "masquerade_break",
                rhai::Dynamic::from(masquerade_break),
            );
            put(&mut out, "humanity_loss", rhai::Dynamic::from(humanity_loss));
            put(&mut out, "error", rhai::Dynamic::from(String::new()));
            out
        },
    );

    // revoke_pc_vampirism(connection_id) -> bool
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn(
        "revoke_pc_vampirism",
        move |connection_id: String| -> bool {
            let conn_id = match uuid::Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut conns_lock = match conns.lock() {
                Ok(g) => g,
                Err(_) => return false,
            };
            let session = match conns_lock.get_mut(&conn_id) {
                Some(s) => s,
                None => return false,
            };
            let ch = match session.character.as_mut() {
                Some(c) => c,
                None => return false,
            };
            if ch.vampire_state.is_none() {
                return false;
            }
            ch.vampire_state = None;
            cdb.save_character_data(ch.clone()).is_ok()
        },
    );
}

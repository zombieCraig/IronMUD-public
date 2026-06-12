//! Rhai bindings for cyberware state on player characters.
//!
//! PC-only surface (mobs don't carry `CyberwareState`). Connection-id keyed,
//! mirroring `src/script/replicant.rs`. Every mutator follows the
//! session-authoritative pattern: lock connections, mutate
//! `session.character`, then `db.save_character_data` — the regen tick
//! flushes session→DB, so DB-only writes would be clobbered.
//!
//! These are capabilities, not commands: install/uninstall/therapy are
//! transactional core fns exposed to BOTH player command scripts and
//! builder-authored DG/dialogue triggers, so production ripperdoc NPCs can
//! be wired with zero code changes. Gold pricing deliberately stays in the
//! calling script — the engine moves humanity, builders set prices.
//!
//! DEADLOCK NOTE: race affinity lives behind the World lock
//! (`state.race_definitions`). Every function here that needs it reads and
//! drops the World lock BEFORE touching the connections lock.

use crate::SharedConnections;
use crate::SharedState;
use crate::cyberware::{
    apply_therapy, cha_erosion_penalty, foundation_slots_total, foundation_slots_used, install_piece, max_humanity,
    uninstall_piece,
};
use crate::db::Db;
use crate::types::{CyberwareAffinity, CyberwareState, InstalledCyberware, ItemData, ItemLocation};
use rhai::Engine;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn err_map(error: &str) -> rhai::Map {
    let mut m = rhai::Map::new();
    m.insert("success".into(), rhai::Dynamic::from(false));
    m.insert("error".into(), rhai::Dynamic::from(error.to_string()));
    m
}

/// Read a race's cyberware affinity from the World. Takes and RELEASES the
/// World lock — call before locking connections.
fn race_affinity(state: &SharedState, race: &str) -> CyberwareAffinity {
    let world = match state.lock() {
        Ok(w) => w,
        Err(_) => return CyberwareAffinity::Normal,
    };
    world
        .race_definitions
        .get(&race.to_lowercase())
        .map(|r| r.cyberware_affinity)
        .unwrap_or_default()
}

/// Case-insensitive item match: exact UUID, name-contains, or
/// keyword-starts-with (standard MUD targeting).
fn item_matches(item: &ItemData, key: &str) -> bool {
    let key_lower = key.to_lowercase();
    if let Ok(u) = uuid::Uuid::parse_str(key) {
        if item.id == u {
            return true;
        }
    }
    if item.name.to_lowercase().contains(&key_lower) {
        return true;
    }
    item.keywords.iter().any(|k| k.to_lowercase().starts_with(&key_lower))
}

/// Same matching for installed snapshots: install_id, vnum, name, keyword.
fn piece_matches(piece: &InstalledCyberware, key: &str) -> bool {
    let key_lower = key.to_lowercase();
    if let Ok(u) = uuid::Uuid::parse_str(key) {
        if piece.install_id == u {
            return true;
        }
    }
    if piece
        .source_vnum
        .as_deref()
        .is_some_and(|v| v.eq_ignore_ascii_case(key))
    {
        return true;
    }
    if piece.name.to_lowercase().contains(&key_lower) {
        return true;
    }
    piece.keywords.iter().any(|k| k.to_lowercase().starts_with(&key_lower))
}

fn piece_to_map(piece: &InstalledCyberware, installed: &[InstalledCyberware]) -> rhai::Map {
    let mut m = rhai::Map::new();
    m.insert("install_id".into(), rhai::Dynamic::from(piece.install_id.to_string()));
    m.insert(
        "vnum".into(),
        rhai::Dynamic::from(piece.source_vnum.clone().unwrap_or_default()),
    );
    m.insert("name".into(), rhai::Dynamic::from(piece.name.clone()));
    m.insert("short_desc".into(), rhai::Dynamic::from(piece.short_desc.clone()));
    m.insert(
        "category".into(),
        rhai::Dynamic::from(piece.cyber_category.to_display_string().to_string()),
    );
    m.insert("foundation".into(), rhai::Dynamic::from(piece.cyber_foundation));
    m.insert("paired".into(), rhai::Dynamic::from(piece.cyber_paired));
    m.insert(
        "humanity_loss".into(),
        rhai::Dynamic::from(piece.cyber_humanity_loss as i64),
    );
    m.insert("humanity_paid".into(), rhai::Dynamic::from(piece.humanity_paid as i64));
    m.insert(
        "exclusive_tag".into(),
        rhai::Dynamic::from(piece.cyber_exclusive_tag.clone()),
    );
    m.insert("visible".into(), rhai::Dynamic::from(piece.cyber_category.is_visible()));
    if piece.cyber_foundation {
        m.insert(
            "slots_total".into(),
            rhai::Dynamic::from(foundation_slots_total(piece) as i64),
        );
        m.insert(
            "slots_used".into(),
            rhai::Dynamic::from(foundation_slots_used(installed, piece.install_id) as i64),
        );
    } else {
        m.insert("slot_cost".into(), rhai::Dynamic::from(piece.cyber_slot_cost as i64));
    }
    m
}

pub fn register_cyberware_functions(
    engine: &mut Engine,
    db: Arc<Db>,
    connections: SharedConnections,
    state: SharedState,
) {
    // is_pc_chromed(connection_id) -> bool
    let conns = connections.clone();
    engine.register_fn("is_pc_chromed", move |connection_id: String| -> bool {
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
            .map(|c| c.cyberware_state.is_some())
            .unwrap_or(false)
    });

    // init_pc_cyberware(connection_id) -> bool
    // Stamps a fresh CyberwareState (full humanity) if absent — idempotent.
    // Shared by character creation (augmented race) and admin tooling.
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn("init_pc_cyberware", move |connection_id: String| -> bool {
        let conn_id = match uuid::Uuid::parse_str(&connection_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let mut conns_lock = match conns.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        let ch = match conns_lock.get_mut(&conn_id).and_then(|s| s.character.as_mut()) {
            Some(c) => c,
            None => return false,
        };
        if ch.cyberware_state.is_none() {
            ch.cyberware_state = Some(CyberwareState::newly_chromed(ch.stat_cha, now_secs()));
        }
        cdb.save_character_data(ch.clone()).is_ok()
    });

    // get_cyberware_state(connection_id) -> Map
    //   { success, humanity, max_humanity, pct, cha_penalty, episode_kind,
    //     installed: [Map...] }
    let conns = connections.clone();
    engine.register_fn("get_cyberware_state", move |connection_id: String| -> rhai::Map {
        let conn_id = match uuid::Uuid::parse_str(&connection_id) {
            Ok(u) => u,
            Err(_) => return err_map("bad connection id"),
        };
        let conns_lock = match conns.lock() {
            Ok(g) => g,
            Err(_) => return err_map("lock"),
        };
        let ch = match conns_lock.get(&conn_id).and_then(|s| s.character.as_ref()) {
            Some(c) => c,
            None => return err_map("no character"),
        };
        let cy = match ch.cyberware_state.as_ref() {
            Some(s) => s,
            None => return err_map("not chromed"),
        };
        let max = max_humanity(ch.stat_cha, &cy.installed);
        let now = now_secs();
        let mut m = rhai::Map::new();
        m.insert("success".into(), rhai::Dynamic::from(true));
        m.insert("humanity".into(), rhai::Dynamic::from(cy.humanity as i64));
        m.insert("max_humanity".into(), rhai::Dynamic::from(max as i64));
        m.insert(
            "pct".into(),
            rhai::Dynamic::from(crate::cyberware::humanity_pct(cy.humanity, max) as i64),
        );
        m.insert(
            "cha_penalty".into(),
            rhai::Dynamic::from(cha_erosion_penalty(cy.humanity, max) as i64),
        );
        m.insert(
            "episode_kind".into(),
            rhai::Dynamic::from(if cy.is_in_episode(now) {
                cy.episode_kind.clone().unwrap_or_default()
            } else {
                String::new()
            }),
        );
        let installed: rhai::Array = cy
            .installed
            .iter()
            .map(|p| rhai::Dynamic::from(piece_to_map(p, &cy.installed)))
            .collect();
        m.insert("installed".into(), rhai::Dynamic::from(installed));
        m
    });

    // has_cyberware(connection_id, key) -> bool
    // key matches install_id, source vnum, name, or keyword of any installed
    // piece. Powers ability gates (e.g. adrenaline_surge needs the adrenal
    // booster implant).
    let conns = connections.clone();
    engine.register_fn("has_cyberware", move |connection_id: String, key: String| -> bool {
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
            .and_then(|c| c.cyberware_state.as_ref())
            .map(|cy| cy.installed.iter().any(|p| piece_matches(p, &key)))
            .unwrap_or(false)
    });

    // install_cyberware(connection_id, key) -> Map
    //   { success, error, name, humanity_paid, humanity, max_humanity }
    // Finds the cyberware item in the player's INVENTORY by keyword/id,
    // validates against race affinity + the slot model, charges humanity,
    // consumes the item. The calling script (ripperdoc trigger, admin
    // command) owns pricing and flavor.
    let conns = connections.clone();
    let cdb = db.clone();
    let cstate = state.clone();
    engine.register_fn(
        "install_cyberware",
        move |connection_id: String, key: String| -> rhai::Map {
            let conn_id = match uuid::Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return err_map("bad connection id"),
            };
            // Resolve the character name + race WITHOUT holding both locks.
            let (char_name, race) = {
                let conns_lock = match conns.lock() {
                    Ok(g) => g,
                    Err(_) => return err_map("lock"),
                };
                match conns_lock.get(&conn_id).and_then(|s| s.character.as_ref()) {
                    Some(c) => (c.name.clone(), c.race.clone()),
                    None => return err_map("no character"),
                }
            };
            let affinity = race_affinity(&cstate, &race);

            let inventory = match cdb.get_items_in_inventory(&char_name) {
                Ok(items) => items,
                Err(_) => return err_map("inventory unavailable"),
            };
            let item = match inventory.iter().find(|i| item_matches(i, &key)) {
                Some(i) => i.clone(),
                None => return err_map("no such item in inventory"),
            };

            let mut conns_lock = match conns.lock() {
                Ok(g) => g,
                Err(_) => return err_map("lock"),
            };
            let ch = match conns_lock.get_mut(&conn_id).and_then(|s| s.character.as_mut()) {
                Some(c) => c,
                None => return err_map("no character"),
            };
            let receipt = match install_piece(ch, &item, affinity, false, now_secs()) {
                Ok(r) => r,
                Err(e) => return err_map(&e.to_string()),
            };
            let _ = cdb.save_character_data(ch.clone());
            let _ = cdb.delete_item(&item.id);

            let mut m = rhai::Map::new();
            m.insert("success".into(), rhai::Dynamic::from(true));
            m.insert("name".into(), rhai::Dynamic::from(item.short_desc.clone()));
            m.insert(
                "humanity_paid".into(),
                rhai::Dynamic::from(receipt.humanity_paid as i64),
            );
            m.insert("humanity".into(), rhai::Dynamic::from(receipt.humanity as i64));
            m.insert("max_humanity".into(), rhai::Dynamic::from(receipt.max_humanity as i64));
            m
        },
    );

    // install_cyberware_free(connection_id, vnum) -> Map
    // Spawn-from-prototype + install at ZERO current-humanity cost (the max
    // reduction still applies). Born-chromed character creation and
    // admin/test tooling. Same result map as install_cyberware.
    let conns = connections.clone();
    let cdb = db.clone();
    let cstate = state.clone();
    engine.register_fn(
        "install_cyberware_free",
        move |connection_id: String, vnum: String| -> rhai::Map {
            let conn_id = match uuid::Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return err_map("bad connection id"),
            };
            let race = {
                let conns_lock = match conns.lock() {
                    Ok(g) => g,
                    Err(_) => return err_map("lock"),
                };
                match conns_lock.get(&conn_id).and_then(|s| s.character.as_ref()) {
                    Some(c) => c.race.clone(),
                    None => return err_map("no character"),
                }
            };
            let affinity = race_affinity(&cstate, &race);

            let item = match cdb.spawn_item_from_prototype(&vnum) {
                Ok(Some(i)) => i,
                Ok(None) => return err_map("no such cyberware prototype"),
                Err(_) => return err_map("prototype unavailable"),
            };

            let mut conns_lock = match conns.lock() {
                Ok(g) => g,
                Err(_) => return err_map("lock"),
            };
            let ch = match conns_lock.get_mut(&conn_id).and_then(|s| s.character.as_mut()) {
                Some(c) => c,
                None => {
                    let _ = cdb.delete_item(&item.id);
                    return err_map("no character");
                }
            };
            let receipt = match install_piece(ch, &item, affinity, true, now_secs()) {
                Ok(r) => r,
                Err(e) => {
                    let _ = cdb.delete_item(&item.id);
                    return err_map(&e.to_string());
                }
            };
            let _ = cdb.save_character_data(ch.clone());
            let _ = cdb.delete_item(&item.id);

            let mut m = rhai::Map::new();
            m.insert("success".into(), rhai::Dynamic::from(true));
            m.insert("name".into(), rhai::Dynamic::from(item.short_desc.clone()));
            m.insert(
                "humanity_paid".into(),
                rhai::Dynamic::from(receipt.humanity_paid as i64),
            );
            m.insert("humanity".into(), rhai::Dynamic::from(receipt.humanity as i64));
            m.insert("max_humanity".into(), rhai::Dynamic::from(receipt.max_humanity as i64));
            m
        },
    );

    // uninstall_cyberware(connection_id, key) -> Map
    //   { success, error, name, humanity, max_humanity }
    // Rebuilds the item from its snapshot into the player's inventory.
    // Restores the max-humanity reduction; current humanity stays spent.
    // Foundations hosting options refuse (error names the dependents).
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn(
        "uninstall_cyberware",
        move |connection_id: String, key: String| -> rhai::Map {
            let conn_id = match uuid::Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return err_map("bad connection id"),
            };
            let mut conns_lock = match conns.lock() {
                Ok(g) => g,
                Err(_) => return err_map("lock"),
            };
            let ch = match conns_lock.get_mut(&conn_id).and_then(|s| s.character.as_mut()) {
                Some(c) => c,
                None => return err_map("no character"),
            };
            let install_id = match ch
                .cyberware_state
                .as_ref()
                .and_then(|cy| cy.installed.iter().find(|p| piece_matches(p, &key)))
            {
                Some(p) => p.install_id,
                None => return err_map("no such installed cyberware"),
            };
            let mut item = match uninstall_piece(ch, install_id) {
                Ok(i) => i,
                Err(e) => return err_map(&e.to_string()),
            };
            item.location = ItemLocation::Inventory(ch.name.to_lowercase());
            let name = item.short_desc.clone();
            let max = max_humanity(
                ch.stat_cha,
                &ch.cyberware_state
                    .as_ref()
                    .map(|c| c.installed.clone())
                    .unwrap_or_default(),
            );
            let humanity = ch.cyberware_state.as_ref().map(|c| c.humanity).unwrap_or(0);
            let _ = cdb.save_character_data(ch.clone());
            let _ = cdb.save_item_data(item);

            let mut m = rhai::Map::new();
            m.insert("success".into(), rhai::Dynamic::from(true));
            m.insert("name".into(), rhai::Dynamic::from(name));
            m.insert("humanity".into(), rhai::Dynamic::from(humanity as i64));
            m.insert("max_humanity".into(), rhai::Dynamic::from(max as i64));
            m
        },
    );

    // cyberware_therapy(connection_id, points) -> Map
    //   { success, restored, humanity, max_humanity }
    // Pure humanity restore capped at max. Gold pricing stays in the calling
    // script (builder-owned: therapist NPC dialogue, clinic room trigger).
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn(
        "cyberware_therapy",
        move |connection_id: String, points: i64| -> rhai::Map {
            let conn_id = match uuid::Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return err_map("bad connection id"),
            };
            let mut conns_lock = match conns.lock() {
                Ok(g) => g,
                Err(_) => return err_map("lock"),
            };
            let ch = match conns_lock.get_mut(&conn_id).and_then(|s| s.character.as_mut()) {
                Some(c) => c,
                None => return err_map("no character"),
            };
            if ch.cyberware_state.is_none() {
                return err_map("not chromed");
            }
            let (restored, humanity, max) = apply_therapy(ch, points as i32);
            let _ = cdb.save_character_data(ch.clone());
            let mut m = rhai::Map::new();
            m.insert("success".into(), rhai::Dynamic::from(true));
            m.insert("restored".into(), rhai::Dynamic::from(restored as i64));
            m.insert("humanity".into(), rhai::Dynamic::from(humanity as i64));
            m.insert("max_humanity".into(), rhai::Dynamic::from(max as i64));
            m
        },
    );

    // get_character_visible_cyberware(char_name) -> Array of Maps
    //   [{ short_desc, category }] — externally visible chrome only
    // (cyberoptics, cyberlimbs, external body, borgware, fashionware).
    // Name-keyed for examine: prefers the online session copy, falls back
    // to the DB for offline characters.
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn(
        "get_character_visible_cyberware",
        move |char_name: String| -> rhai::Array {
            let visible_of = |cy: &CyberwareState| -> rhai::Array {
                cy.installed
                    .iter()
                    .filter(|p| p.cyber_category.is_visible())
                    .map(|p| {
                        let mut m = rhai::Map::new();
                        m.insert("short_desc".into(), rhai::Dynamic::from(p.short_desc.clone()));
                        m.insert(
                            "category".into(),
                            rhai::Dynamic::from(p.cyber_category.to_display_string().to_string()),
                        );
                        rhai::Dynamic::from(m)
                    })
                    .collect()
            };
            let name_lower = char_name.to_lowercase();
            if let Ok(conns_lock) = conns.lock() {
                for session in conns_lock.values() {
                    if let Some(ch) = session.character.as_ref() {
                        if ch.name.to_lowercase() == name_lower {
                            return ch.cyberware_state.as_ref().map(&visible_of).unwrap_or_default();
                        }
                    }
                }
            }
            cdb.get_character_data(&name_lower)
                .ok()
                .flatten()
                .and_then(|c| c.cyberware_state.as_ref().map(&visible_of))
                .unwrap_or_default()
        },
    );

    // get_race_cyberware_affinity(race_id) -> String
    // "incompatible" | "normal" | "adept" — lets scripts refuse politely
    // before quoting a price.
    let cstate = state.clone();
    engine.register_fn("get_race_cyberware_affinity", move |race_id: String| -> String {
        race_affinity(&cstate, &race_id).to_display_string().to_string()
    });
}

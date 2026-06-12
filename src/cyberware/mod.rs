//! Cyberware install/uninstall, humanity accounting, and cyberpsychosis —
//! testable lib-side. The thin tokio loop wrapper lives in
//! `src/ticks/cyberware.rs` (bin-only) and just calls these.
//!
//! Shared here (rather than in `src/script/cyberware.rs`) because the psyche
//! tick and the Rhai bindings both mutate the same humanity state and the
//! math must stay identical. Everything operates on `&mut CharacterData`
//! with injected rolls so integration tests in `tests/` can drive it
//! deterministically.
//!
//! Model summary (Cyberpunk RED adapted — see `src/types/cyberware.rs`):
//! - Max humanity = base CHA × 10, minus 2 per installed piece (4 for
//!   borgware; 0-HL fashionware reduces nothing). Never stored, always
//!   computed from base `stat_cha` — using *effective* CHA would feed the
//!   erosion debuff back into the ceiling.
//! - Installing charges current humanity (the item's tier, discounted for
//!   `Adept` races). Uninstalling restores the max reduction, never the
//!   spent points. Therapy restores spent points up to max.
//! - Every full 10 points of deficit (max − current) = −1 effective CHA via
//!   a single permanent `CharismaBoost` buff sourced `"cyberware:humanity"`.
//! - Below 30% humanity the psyche tick rolls for cyberpsychotic episodes,
//!   escalating from dissociation (Slow + Luck debuffs) to violent
//!   (Frenzy + Rage — the same combat plumbing as vampire hunger frenzy).

use crate::types::{
    ActiveBuff, CYBERPSYCHOSIS_BUFF_SOURCE, CYBERWARE_BUFF_SOURCE_PREFIX, CharacterData, CyberwareAffinity,
    CyberwareCategory, CyberwareState, DISSOCIATION_DURATION_SECS, EPISODE_COOLDOWN_SECS, EPISODE_COOLDOWN_ZERO_SECS,
    EPISODE_KIND_DISSOCIATION, EPISODE_KIND_VIOLENT, EffectType, HUMANITY_EROSION_BUFF_SOURCE, HUMANITY_PER_CHA_POINT,
    InstalledCyberware, ItemData, ItemType, PSYCHE_STABLE_PCT, PSYCHE_VIOLENT_BAND_PCT, VIOLENT_BAND_CHANCE_CAP,
    VIOLENT_DURATION_SECS, ZERO_HUMANITY_EPISODE_CHANCE, ZERO_VIOLENT_DURATION_SECS,
};
use uuid::Uuid;

pub use crate::types::PSYCHE_TICK_INTERVAL_SECS;

/// Frenzy damage bonus during a violent episode (matches vampire hunger
/// frenzy so the combat tick treats both beasts identically).
pub const VIOLENT_EPISODE_DAMAGE_BONUS: i32 = 4;

// ---------------------------------------------------------------------------
// Humanity math
// ---------------------------------------------------------------------------

/// Max humanity ceiling: base CHA × 10 minus the per-piece reductions.
/// Floors at 0 (a CHA-1 borg festival is its own punishment).
pub fn max_humanity(base_cha: i32, installed: &[InstalledCyberware]) -> i32 {
    let base = base_cha.max(1) * HUMANITY_PER_CHA_POINT;
    let reduction: i32 = installed
        .iter()
        .map(|p| p.cyber_category.max_humanity_reduction(p.cyber_humanity_loss))
        .sum();
    (base - reduction).max(0)
}

/// Current humanity as a percentage of max (0-100). A zero max counts as 0%.
pub fn humanity_pct(humanity: i32, max: i32) -> i32 {
    if max <= 0 {
        return 0;
    }
    (humanity.clamp(0, max) * 100) / max
}

/// Humanity charged for installing a piece with the given humanity-loss
/// tier. Adept races (augmented) pay 3/4, minimum 1 for any nonzero tier:
/// 2→1, 3→2, 7→5, 14→10.
pub fn install_cost(humanity_loss: i32, affinity: CyberwareAffinity) -> i32 {
    if humanity_loss <= 0 {
        return 0;
    }
    match affinity {
        CyberwareAffinity::Adept => (humanity_loss * 3 / 4).max(1),
        _ => humanity_loss,
    }
}

// ---------------------------------------------------------------------------
// Slot accounting
// ---------------------------------------------------------------------------

/// Whether pieces of this category install into a foundation (when not a
/// foundation themselves) rather than standing alone.
fn is_slotted_category(cat: CyberwareCategory) -> bool {
    cat.foundation_max() > 0
}

/// Option slots a foundation provides (item override or category default).
pub fn foundation_slots_total(piece: &InstalledCyberware) -> i32 {
    if piece.cyber_option_slots > 0 {
        piece.cyber_option_slots
    } else {
        piece.cyber_category.default_option_slots()
    }
}

/// Slots consumed in the given foundation by installed options.
pub fn foundation_slots_used(installed: &[InstalledCyberware], foundation_id: Uuid) -> i32 {
    installed
        .iter()
        .filter(|p| p.host_foundations.contains(&foundation_id))
        .map(|p| p.cyber_slot_cost.max(1))
        .sum()
}

fn foundation_free_slots(installed: &[InstalledCyberware], foundation: &InstalledCyberware) -> i32 {
    foundation_slots_total(foundation) - foundation_slots_used(installed, foundation.install_id)
}

// ---------------------------------------------------------------------------
// Install validation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallError {
    /// Race biology rejects grafts entirely.
    Incompatible,
    /// Item isn't cyberware (wrong type or no category).
    NotCyberware,
    /// Option needs a foundation of its category that isn't installed.
    NoFoundation(CyberwareCategory),
    /// Foundations of the category exist but none has enough free slots
    /// (or, for paired options, fewer than two do).
    NoFreeSlots(CyberwareCategory),
    /// Body already holds the max foundations of this category.
    FoundationLimit(CyberwareCategory),
    /// Another installed piece carries the same exclusive tag.
    ExclusiveTag(String),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::Incompatible => write!(f, "your body rejects cybernetic grafts"),
            InstallError::NotCyberware => write!(f, "that is not installable cyberware"),
            InstallError::NoFoundation(c) => {
                write!(f, "requires an installed {} foundation", c.to_display_string())
            }
            InstallError::NoFreeSlots(c) => write!(
                f,
                "no installed {} foundation has enough free option slots",
                c.to_display_string()
            ),
            InstallError::FoundationLimit(c) => {
                write!(f, "the body cannot hold another {} foundation", c.to_display_string())
            }
            InstallError::ExclusiveTag(t) => write!(f, "already fitted with {} hardware", t),
        }
    }
}

/// Where a validated piece will go: the install_ids of the foundation(s)
/// that will host it (two for paired options, empty for foundations and
/// standalone pieces).
pub struct InstallPlacement {
    pub host_foundations: Vec<Uuid>,
}

/// Validate an install against race affinity and the RED slot model.
/// Pure — does not mutate. `installed` is the character's current chrome.
pub fn validate_install(
    affinity: CyberwareAffinity,
    installed: &[InstalledCyberware],
    item: &ItemData,
) -> Result<InstallPlacement, InstallError> {
    if affinity == CyberwareAffinity::Incompatible {
        return Err(InstallError::Incompatible);
    }
    if item.item_type != ItemType::Cyberware {
        return Err(InstallError::NotCyberware);
    }
    let cat = match item.cyber_category {
        Some(c) => c,
        None => return Err(InstallError::NotCyberware),
    };

    // One piece per non-empty exclusive tag (e.g. "speedware").
    let tag = item.cyber_exclusive_tag.trim();
    if !tag.is_empty() {
        if installed
            .iter()
            .any(|p| p.cyber_exclusive_tag.eq_ignore_ascii_case(tag))
        {
            return Err(InstallError::ExclusiveTag(tag.to_string()));
        }
    }

    // Foundations: capped per category (one neural link, two eyes...).
    if item.cyber_foundation && is_slotted_category(cat) {
        let existing = installed
            .iter()
            .filter(|p| p.cyber_foundation && p.cyber_category == cat)
            .count() as i32;
        if existing >= cat.foundation_max() {
            return Err(InstallError::FoundationLimit(cat));
        }
        return Ok(InstallPlacement {
            host_foundations: Vec::new(),
        });
    }

    // Options: need hosting foundation(s) of the same category with room.
    if is_slotted_category(cat) {
        let needed = if item.cyber_paired { 2 } else { 1 };
        let cost = item.cyber_slot_cost.max(1);
        let foundations: Vec<&InstalledCyberware> = installed
            .iter()
            .filter(|p| p.cyber_foundation && p.cyber_category == cat)
            .collect();
        if foundations.is_empty() {
            return Err(InstallError::NoFoundation(cat));
        }
        // Best-fit: fill the foundations with the most free room first so
        // small options don't strand big ones.
        let mut with_room: Vec<(&InstalledCyberware, i32)> = foundations
            .iter()
            .map(|f| (*f, foundation_free_slots(installed, f)))
            .filter(|(_, free)| *free >= cost)
            .collect();
        if with_room.len() < needed {
            return Err(InstallError::NoFreeSlots(cat));
        }
        with_room.sort_by(|a, b| b.1.cmp(&a.1));
        return Ok(InstallPlacement {
            host_foundations: with_room.iter().take(needed).map(|(f, _)| f.install_id).collect(),
        });
    }

    // Standalone categories (fashionware, internal, external, borgware).
    Ok(InstallPlacement {
        host_foundations: Vec::new(),
    })
}

// ---------------------------------------------------------------------------
// Snapshot / rebuild
// ---------------------------------------------------------------------------

/// Freeze a cyberware item into the on-character snapshot. The slot cost is
/// resolved to its effective value so later accounting never re-derives it.
pub fn snapshot_from_item(
    item: &ItemData,
    host_foundations: Vec<Uuid>,
    humanity_paid: i32,
    now: i64,
) -> InstalledCyberware {
    InstalledCyberware {
        install_id: Uuid::new_v4(),
        source_vnum: item.vnum.clone(),
        name: item.name.clone(),
        short_desc: item.short_desc.clone(),
        long_desc: item.long_desc.clone(),
        keywords: item.keywords.clone(),
        categories: item.categories.clone(),
        affects: item.affects.clone(),
        cyber_category: item.cyber_category.unwrap_or(CyberwareCategory::InternalBody),
        cyber_foundation: item.cyber_foundation,
        cyber_option_slots: item.cyber_option_slots,
        cyber_slot_cost: item.cyber_slot_cost.max(1),
        cyber_humanity_loss: item.cyber_humanity_loss,
        humanity_paid,
        cyber_paired: item.cyber_paired,
        cyber_exclusive_tag: item.cyber_exclusive_tag.trim().to_string(),
        host_foundations,
        installed_at: now,
    }
}

/// Rebuild a loose item from an uninstalled snapshot (immune to prototype
/// edits/deletion — the snapshot is the truth).
pub fn rebuild_item_from_snapshot(piece: &InstalledCyberware) -> ItemData {
    let mut item = ItemData::new(piece.name.clone(), piece.short_desc.clone(), piece.long_desc.clone());
    item.item_type = ItemType::Cyberware;
    item.keywords = piece.keywords.clone();
    item.categories = piece.categories.clone();
    item.affects = piece.affects.clone();
    item.vnum = piece.source_vnum.clone();
    item.cyber_category = Some(piece.cyber_category);
    item.cyber_foundation = piece.cyber_foundation;
    item.cyber_option_slots = piece.cyber_option_slots;
    item.cyber_slot_cost = piece.cyber_slot_cost;
    item.cyber_humanity_loss = piece.cyber_humanity_loss;
    item.cyber_paired = piece.cyber_paired;
    item.cyber_exclusive_tag = piece.cyber_exclusive_tag.clone();
    item
}

// ---------------------------------------------------------------------------
// Install / uninstall / therapy
// ---------------------------------------------------------------------------

/// Receipt for a successful install, for caller-side messaging.
pub struct InstallReceipt {
    pub install_id: Uuid,
    pub humanity_paid: i32,
    pub humanity: i32,
    pub max_humanity: i32,
}

/// Install a cyberware item into `ch`: validates, lazily stamps a
/// `CyberwareState` (full humanity) on first chrome, charges humanity
/// (`free` skips the charge — born-chromed kit, admin grants — but the max
/// reduction still applies), stamps the item's affects as permanent buffs
/// sourced `"cyberware:<install_id>"`, and restamps the CHA-erosion buff.
/// Caller deletes the consumed item, saves, syncs the session copy, and
/// messages.
pub fn install_piece(
    ch: &mut CharacterData,
    item: &ItemData,
    affinity: CyberwareAffinity,
    free: bool,
    now: i64,
) -> Result<InstallReceipt, InstallError> {
    let existing: &[InstalledCyberware] = ch
        .cyberware_state
        .as_ref()
        .map(|s| s.installed.as_slice())
        .unwrap_or(&[]);
    let placement = validate_install(affinity, existing, item)?;

    let base_cha = ch.stat_cha;
    let state = ch
        .cyberware_state
        .get_or_insert_with(|| CyberwareState::newly_chromed(base_cha, now));

    let cost = if free {
        0
    } else {
        install_cost(item.cyber_humanity_loss, affinity)
    };
    let piece = snapshot_from_item(item, placement.host_foundations, cost, now);
    let install_id = piece.install_id;

    for affect in &piece.affects {
        ch.active_buffs.push(ActiveBuff {
            effect_type: affect.effect_type,
            magnitude: affect.magnitude,
            remaining_secs: -1,
            source: format!("{}{}", CYBERWARE_BUFF_SOURCE_PREFIX, install_id),
            damage_type: affect.damage_type,
            vs_effect: affect.vs_effect.clone(),
            skill_key: affect.skill_key.clone(),
        });
    }

    state.installed.push(piece);
    let max = max_humanity(base_cha, &state.installed);
    state.humanity = (state.humanity - cost).clamp(0, max);
    let humanity = state.humanity;
    recalc_humanity_cha_erosion(ch);

    Ok(InstallReceipt {
        install_id,
        humanity_paid: cost,
        humanity,
        max_humanity: max,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UninstallError {
    NotFound,
    /// Foundation still hosts these option names — remove them first.
    HostsOptions(Vec<String>),
}

impl std::fmt::Display for UninstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UninstallError::NotFound => write!(f, "no such installed cyberware"),
            UninstallError::HostsOptions(names) => {
                write!(f, "still hosts installed options: {}", names.join(", "))
            }
        }
    }
}

/// Remove an installed piece: strips its affect buffs, restores the max
/// reduction (current humanity unchanged — the soul doesn't grow back with
/// the meat), restamps erosion, and returns the rebuilt loose item. Caller
/// places the item, saves, syncs, and messages.
pub fn uninstall_piece(ch: &mut CharacterData, install_id: Uuid) -> Result<ItemData, UninstallError> {
    let base_cha = ch.stat_cha;
    let state = match ch.cyberware_state.as_mut() {
        Some(s) => s,
        None => return Err(UninstallError::NotFound),
    };
    let idx = match state.installed.iter().position(|p| p.install_id == install_id) {
        Some(i) => i,
        None => return Err(UninstallError::NotFound),
    };
    let dependents: Vec<String> = state
        .installed
        .iter()
        .filter(|p| p.host_foundations.contains(&install_id))
        .map(|p| p.short_desc.clone())
        .collect();
    if !dependents.is_empty() {
        return Err(UninstallError::HostsOptions(dependents));
    }

    let piece = state.installed.remove(idx);
    let max = max_humanity(base_cha, &state.installed);
    state.humanity = state.humanity.clamp(0, max);

    let source = format!("{}{}", CYBERWARE_BUFF_SOURCE_PREFIX, install_id);
    ch.active_buffs.retain(|b| b.source != source);
    recalc_humanity_cha_erosion(ch);

    Ok(rebuild_item_from_snapshot(&piece))
}

/// Restore humanity through therapy, capped at the computed max. Returns
/// (points_restored, new_humanity, max). Caller charges gold, saves, syncs.
pub fn apply_therapy(ch: &mut CharacterData, points: i32) -> (i32, i32, i32) {
    let base_cha = ch.stat_cha;
    let state = match ch.cyberware_state.as_mut() {
        Some(s) => s,
        None => return (0, 0, 0),
    };
    let max = max_humanity(base_cha, &state.installed);
    let before = state.humanity.clamp(0, max);
    state.humanity = (before + points.max(0)).clamp(0, max);
    let restored = state.humanity - before;
    state.lifetime_humanity_restored += restored;
    let new = state.humanity;
    recalc_humanity_cha_erosion(ch);
    (restored, new, max)
}

/// Current effective-CHA penalty from humanity deficit: 1 per full 10 lost.
pub fn cha_erosion_penalty(humanity: i32, max: i32) -> i32 {
    (max - humanity.clamp(0, max)).max(0) / HUMANITY_PER_CHA_POINT
}

/// Strip and restamp the single permanent CHA-erosion buff (source
/// `"cyberware:humanity"`). Call after every install/uninstall/therapy/
/// episode mutation; the psyche tick also calls it as a safety net so any
/// missed `stat_cha` mutation self-heals within a minute.
pub fn recalc_humanity_cha_erosion(ch: &mut CharacterData) {
    ch.active_buffs.retain(|b| b.source != HUMANITY_EROSION_BUFF_SOURCE);
    let state = match ch.cyberware_state.as_ref() {
        Some(s) => s,
        None => return,
    };
    let max = max_humanity(ch.stat_cha, &state.installed);
    let penalty = cha_erosion_penalty(state.humanity, max);
    if penalty <= 0 {
        return;
    }
    ch.active_buffs.push(ActiveBuff {
        effect_type: EffectType::CharismaBoost,
        magnitude: -penalty,
        remaining_secs: -1,
        source: HUMANITY_EROSION_BUFF_SOURCE.to_string(),
        damage_type: None,
        vs_effect: None,
        skill_key: None,
    });
}

// ---------------------------------------------------------------------------
// Cyberpsychosis
// ---------------------------------------------------------------------------

/// Outcome of a triggered cyberpsychotic episode, for caller-side messaging.
pub struct EpisodeOutcome {
    /// "dissociation" | "violent"
    pub kind: &'static str,
    pub duration_secs: i32,
    /// First-person message for the cyberpsycho.
    pub message: &'static str,
    /// Third-person room broadcast ({name} placeholder).
    pub room_message: &'static str,
}

/// Episode chance (0-100) per psyche tick at the given humanity percentage.
/// Stable at >= 30%; (30 − pct)% in the dissociation band; doubled and
/// capped at 60% below 15%; 75% flat at zero humanity.
pub fn episode_chance(pct: i32) -> i32 {
    if pct >= PSYCHE_STABLE_PCT {
        0
    } else if pct == 0 {
        ZERO_HUMANITY_EPISODE_CHANCE
    } else if pct < PSYCHE_VIOLENT_BAND_PCT {
        (2 * (PSYCHE_STABLE_PCT - pct)).min(VIOLENT_BAND_CHANCE_CAP)
    } else {
        PSYCHE_STABLE_PCT - pct
    }
}

/// Roll the cyberpsychosis check and stamp episode buffs on a failure to
/// hold together. Mirrors `vampire::maybe_hunger_frenzy`: never stacks onto
/// an active Frenzy/Rage (vampire frenzy, replicant berserk, or a prior
/// episode), respects the episode cooldown, and takes an injected
/// `roll_1d100` so tests are deterministic. `in_company` decides whether a
/// low-band episode turns violent (someone is there to hurt) or stays
/// dissociative. Returns the outcome when an episode fired; caller saves
/// and messages.
pub fn maybe_psychotic_episode(
    ch: &mut CharacterData,
    in_company: bool,
    now: i64,
    roll_1d100: i32,
) -> Option<EpisodeOutcome> {
    let base_cha = ch.stat_cha;
    let state = ch.cyberware_state.as_ref()?;
    let max = max_humanity(base_cha, &state.installed);
    let pct = humanity_pct(state.humanity, max);
    let chance = episode_chance(pct);
    if chance <= 0 {
        return None;
    }
    let cooldown = if pct == 0 {
        EPISODE_COOLDOWN_ZERO_SECS
    } else {
        EPISODE_COOLDOWN_SECS
    };
    if now < state.last_episode_at + cooldown {
        return None;
    }
    if has_buff(&ch.active_buffs, EffectType::Frenzy) || has_buff(&ch.active_buffs, EffectType::Rage) {
        return None;
    }
    if roll_1d100 > chance {
        return None;
    }

    let violent = pct == 0 || (pct < PSYCHE_VIOLENT_BAND_PCT && in_company);
    let outcome = if violent {
        let duration = if pct == 0 {
            ZERO_VIOLENT_DURATION_SECS
        } else {
            VIOLENT_DURATION_SECS
        };
        push_buff(
            &mut ch.active_buffs,
            EffectType::Frenzy,
            VIOLENT_EPISODE_DAMAGE_BONUS,
            duration,
            CYBERPSYCHOSIS_BUFF_SOURCE,
        );
        push_buff(
            &mut ch.active_buffs,
            EffectType::Rage,
            1,
            duration,
            CYBERPSYCHOSIS_BUFF_SOURCE,
        );
        EpisodeOutcome {
            kind: EPISODE_KIND_VIOLENT,
            duration_secs: duration,
            message: "The meat around your chrome stops mattering. Everything here is just parts.",
            room_message: "{name}'s pupils contract to pinpoints. The chrome is driving now.",
        }
    } else {
        push_buff(
            &mut ch.active_buffs,
            EffectType::Slow,
            1,
            DISSOCIATION_DURATION_SECS,
            CYBERPSYCHOSIS_BUFF_SOURCE,
        );
        push_buff(
            &mut ch.active_buffs,
            EffectType::Luck,
            -3,
            DISSOCIATION_DURATION_SECS,
            CYBERPSYCHOSIS_BUFF_SOURCE,
        );
        EpisodeOutcome {
            kind: EPISODE_KIND_DISSOCIATION,
            duration_secs: DISSOCIATION_DURATION_SECS,
            message: "Your hands look like someone else's hardware. You watch them from very far away.",
            room_message: "{name} stares through their own hands as if inventorying spare parts.",
        }
    };

    if let Some(state) = ch.cyberware_state.as_mut() {
        state.last_episode_at = now;
        state.episode_until = Some(now + outcome.duration_secs as i64);
        state.episode_kind = Some(outcome.kind.to_string());
    }
    Some(outcome)
}

/// Per-character psyche-tick core, separated from the session loop so
/// integration tests can drive it without a PlayerSession. Expires episode
/// bookkeeping, restamps erosion (safety net), and rolls for a new episode.
/// Returns (anything_changed, outcome_if_episode_fired,
/// episode_passed_message).
pub fn apply_psyche_tick_to_character(
    ch: &mut CharacterData,
    in_company: bool,
    now: i64,
    roll_1d100: i32,
) -> (bool, Option<EpisodeOutcome>, Option<&'static str>) {
    if ch.cyberware_state.is_none() {
        return (false, None, None);
    }

    let mut modified = false;
    let mut passed_message = None;

    // Episode expiry bookkeeping (the buffs decay on their own).
    if let Some(state) = ch.cyberware_state.as_mut() {
        if let Some(until) = state.episode_until {
            if now >= until {
                state.episode_until = None;
                state.episode_kind = None;
                modified = true;
                passed_message =
                    Some("\n\x1b[36mThe world clicks back into focus. You are — mostly — yourself.\x1b[0m\n");
            }
        }
        state.last_psyche_tick = now;
    }

    // Erosion safety net: self-heals any stat_cha mutation path that forgot
    // to call recalc.
    let before: Vec<(EffectType, i32)> = ch
        .active_buffs
        .iter()
        .filter(|b| b.source == HUMANITY_EROSION_BUFF_SOURCE)
        .map(|b| (b.effect_type, b.magnitude))
        .collect();
    recalc_humanity_cha_erosion(ch);
    let after: Vec<(EffectType, i32)> = ch
        .active_buffs
        .iter()
        .filter(|b| b.source == HUMANITY_EROSION_BUFF_SOURCE)
        .map(|b| (b.effect_type, b.magnitude))
        .collect();
    if before != after {
        modified = true;
    }

    let outcome = maybe_psychotic_episode(ch, in_company, now, roll_1d100);
    if outcome.is_some() {
        modified = true;
    }
    (modified, outcome, passed_message)
}

/// Per-minute psyche tick over every chromed online player: expire episode
/// bookkeeping, restamp CHA erosion, roll for cyberpsychotic episodes.
/// Mirrors `vampire::process_blood_tick` — mutate the session copy
/// (session is authoritative), save, broadcast room events after the lock
/// is released.
pub fn process_psyche_tick(db: &crate::db::Db, connections: &crate::SharedConnections) -> anyhow::Result<()> {
    use rand::Rng;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // (room_id, name, room_message) episodes to broadcast after the lock
    // below is released.
    let mut episode_events: Vec<(uuid::Uuid, String, &'static str)> = Vec::new();

    {
        let mut conns = connections.lock().unwrap();

        // Room occupancy by online players, for the in-company check.
        let mut room_player_counts: std::collections::HashMap<uuid::Uuid, i32> = std::collections::HashMap::new();
        for session in conns.values() {
            if let Some(ch) = session.character.as_ref() {
                if ch.creation_complete {
                    *room_player_counts.entry(ch.current_room_id).or_insert(0) += 1;
                }
            }
        }

        for (_conn_id, session) in conns.iter_mut() {
            let ch = match session.character.as_mut() {
                Some(c) => c,
                None => continue,
            };
            if !ch.creation_complete || ch.god_mode || ch.cyberware_state.is_none() {
                continue;
            }
            let in_company =
                !ch.combat.targets.is_empty() || room_player_counts.get(&ch.current_room_id).copied().unwrap_or(0) > 1;
            let roll = rand::thread_rng().gen_range(1..=100);
            let (modified, outcome, passed_message) = apply_psyche_tick_to_character(ch, in_company, now, roll);
            if let Some(msg) = passed_message {
                let _ = session.sender.send(msg.to_string());
            }
            if let Some(outcome) = outcome {
                let _ = session.sender.send(format!("\n\x1b[1;31m{}\x1b[0m\n", outcome.message));
                episode_events.push((ch.current_room_id, ch.name.clone(), outcome.room_message));
            }
            if modified {
                let _ = db.save_character_data(ch.clone());
            }
        }
    }

    for (room_id, name, room_message) in episode_events {
        broadcast_episode_to_room(connections, &room_id, &name, room_message);
    }

    Ok(())
}

/// Tell everyone else in the room that someone's chrome just took the wheel.
fn broadcast_episode_to_room(
    connections: &crate::SharedConnections,
    room_id: &uuid::Uuid,
    psycho_name: &str,
    room_message: &str,
) {
    let conns = match connections.lock() {
        Ok(c) => c,
        Err(_) => return,
    };
    let line = room_message.replace("{name}", psycho_name);
    for session in conns.values() {
        let Some(ch) = session.character.as_ref() else {
            continue;
        };
        if ch.current_room_id != *room_id || ch.name.eq_ignore_ascii_case(psycho_name) {
            continue;
        }
        let _ = session.sender.send(format!("\n\x1b[1;31m{}\x1b[0m\n", line));
    }
}

fn has_buff(buffs: &[ActiveBuff], effect_type: EffectType) -> bool {
    buffs.iter().any(|b| b.effect_type == effect_type)
}

fn push_buff(buffs: &mut Vec<ActiveBuff>, effect_type: EffectType, magnitude: i32, secs: i32, source: &str) {
    if let Some(existing) = buffs
        .iter_mut()
        .find(|b| b.effect_type == effect_type && b.source == source)
    {
        existing.magnitude = magnitude;
        existing.remaining_secs = secs;
        return;
    }
    buffs.push(ActiveBuff {
        effect_type,
        magnitude,
        remaining_secs: secs,
        source: source.to_string(),
        damage_type: None,
        vs_effect: None,
        skill_key: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cyber_item(cat: CyberwareCategory, foundation: bool, hl: i32) -> ItemData {
        let mut item = ItemData::new("chrome".into(), "a piece of chrome".into(), String::new());
        item.item_type = ItemType::Cyberware;
        item.cyber_category = Some(cat);
        item.cyber_foundation = foundation;
        item.cyber_humanity_loss = hl;
        item
    }

    fn test_char(cha: i32) -> CharacterData {
        // CharacterData has no Default; round-trip the minimal JSON the way
        // legacy saves do.
        let mut ch: CharacterData = serde_json::from_value(serde_json::json!({
            "name": "Testee",
            "password_hash": "x",
            "current_room_id": Uuid::nil(),
        }))
        .expect("minimal character JSON");
        ch.stat_cha = cha;
        ch
    }

    #[test]
    fn max_humanity_reductions() {
        assert_eq!(max_humanity(10, &[]), 100);
        let ch = test_char(10);
        drop(ch);
        let normal = snapshot_from_item(&cyber_item(CyberwareCategory::InternalBody, false, 7), vec![], 7, 0);
        let fashion = snapshot_from_item(&cyber_item(CyberwareCategory::Fashionware, false, 0), vec![], 0, 0);
        let borg = snapshot_from_item(&cyber_item(CyberwareCategory::Borgware, false, 14), vec![], 14, 0);
        assert_eq!(max_humanity(10, &[normal.clone()]), 98);
        assert_eq!(max_humanity(10, &[normal.clone(), fashion]), 98);
        assert_eq!(max_humanity(10, &[normal, borg]), 94);
    }

    #[test]
    fn install_cost_tiers_and_adept_discount() {
        for (hl, adept) in [(2, 1), (3, 2), (7, 5), (14, 10)] {
            assert_eq!(install_cost(hl, CyberwareAffinity::Normal), hl);
            assert_eq!(install_cost(hl, CyberwareAffinity::Adept), adept);
        }
        assert_eq!(install_cost(0, CyberwareAffinity::Normal), 0);
        assert_eq!(install_cost(0, CyberwareAffinity::Adept), 0);
    }

    #[test]
    fn episode_chance_bands() {
        assert_eq!(episode_chance(100), 0);
        assert_eq!(episode_chance(30), 0);
        assert_eq!(episode_chance(29), 1);
        assert_eq!(episode_chance(20), 10);
        assert_eq!(episode_chance(15), 15);
        assert_eq!(episode_chance(14), 32);
        assert_eq!(episode_chance(1), 58);
        assert_eq!(episode_chance(0), 75);
    }

    #[test]
    fn validate_option_requires_foundation_and_slots() {
        let mut ch = test_char(10);
        let link = cyber_item(CyberwareCategory::Neuralware, true, 7);
        let mut option = cyber_item(CyberwareCategory::Neuralware, false, 7);
        option.cyber_slot_cost = 5;

        // No foundation yet.
        assert_eq!(
            validate_install(CyberwareAffinity::Normal, &[], &option).err(),
            Some(InstallError::NoFoundation(CyberwareCategory::Neuralware))
        );

        install_piece(&mut ch, &link, CyberwareAffinity::Normal, false, 0).unwrap();
        let installed = ch.cyberware_state.as_ref().unwrap().installed.clone();
        // Fits exactly (5 of 5 default slots).
        assert!(validate_install(CyberwareAffinity::Normal, &installed, &option).is_ok());

        install_piece(&mut ch, &option, CyberwareAffinity::Normal, false, 0).unwrap();
        let installed = ch.cyberware_state.as_ref().unwrap().installed.clone();
        let small = cyber_item(CyberwareCategory::Neuralware, false, 3);
        assert_eq!(
            validate_install(CyberwareAffinity::Normal, &installed, &small).err(),
            Some(InstallError::NoFreeSlots(CyberwareCategory::Neuralware))
        );
    }

    #[test]
    fn paired_option_needs_two_foundations_and_blocks_host_removal() {
        let mut ch = test_char(10);
        let eye = cyber_item(CyberwareCategory::Cyberoptic, true, 7);
        let mut lowlight = cyber_item(CyberwareCategory::Cyberoptic, false, 3);
        lowlight.cyber_paired = true;

        install_piece(&mut ch, &eye, CyberwareAffinity::Normal, false, 0).unwrap();
        let installed = ch.cyberware_state.as_ref().unwrap().installed.clone();
        assert_eq!(
            validate_install(CyberwareAffinity::Normal, &installed, &lowlight).err(),
            Some(InstallError::NoFreeSlots(CyberwareCategory::Cyberoptic))
        );

        let second_eye = install_piece(&mut ch, &eye, CyberwareAffinity::Normal, false, 0).unwrap();
        let receipt = install_piece(&mut ch, &lowlight, CyberwareAffinity::Normal, false, 0).unwrap();
        let state = ch.cyberware_state.as_ref().unwrap();
        let opt = state
            .installed
            .iter()
            .find(|p| p.install_id == receipt.install_id)
            .unwrap();
        assert_eq!(opt.host_foundations.len(), 2);

        // Host eye removal blocked while the paired option references it.
        assert!(matches!(
            uninstall_piece(&mut ch, second_eye.install_id),
            Err(UninstallError::HostsOptions(_))
        ));

        // Third eye refused.
        let installed = ch.cyberware_state.as_ref().unwrap().installed.clone();
        assert_eq!(
            validate_install(CyberwareAffinity::Normal, &installed, &eye).err(),
            Some(InstallError::FoundationLimit(CyberwareCategory::Cyberoptic))
        );
    }

    #[test]
    fn exclusive_tag_single_speedware() {
        let mut ch = test_char(10);
        let link = cyber_item(CyberwareCategory::Neuralware, true, 7);
        let mut speed = cyber_item(CyberwareCategory::Neuralware, false, 14);
        speed.cyber_exclusive_tag = "speedware".into();
        install_piece(&mut ch, &link, CyberwareAffinity::Normal, false, 0).unwrap();
        install_piece(&mut ch, &speed, CyberwareAffinity::Normal, false, 0).unwrap();
        let installed = ch.cyberware_state.as_ref().unwrap().installed.clone();
        assert_eq!(
            validate_install(CyberwareAffinity::Normal, &installed, &speed).err(),
            Some(InstallError::ExclusiveTag("speedware".into()))
        );
    }

    #[test]
    fn install_charges_uninstall_restores_max_not_current() {
        let mut ch = test_char(10);
        let graft = cyber_item(CyberwareCategory::InternalBody, false, 14);
        let receipt = install_piece(&mut ch, &graft, CyberwareAffinity::Normal, false, 0).unwrap();
        assert_eq!(receipt.humanity_paid, 14);
        assert_eq!(receipt.max_humanity, 98);
        assert_eq!(receipt.humanity, 86);

        // Erosion: deficit 12 -> -1 CHA.
        let erosion = ch
            .active_buffs
            .iter()
            .find(|b| b.source == HUMANITY_EROSION_BUFF_SOURCE)
            .expect("erosion buff");
        assert_eq!(erosion.magnitude, -1);

        let item = uninstall_piece(&mut ch, receipt.install_id).unwrap();
        assert_eq!(item.cyber_humanity_loss, 14);
        let state = ch.cyberware_state.as_ref().unwrap();
        assert_eq!(max_humanity(ch.stat_cha, &state.installed), 100);
        assert_eq!(state.humanity, 86, "current humanity does not return with the meat");

        let (restored, new, max) = apply_therapy(&mut ch, 50);
        assert_eq!((restored, new, max), (14, 100, 100));
        assert!(
            !ch.active_buffs.iter().any(|b| b.source == HUMANITY_EROSION_BUFF_SOURCE),
            "erosion clears at full humanity"
        );
    }

    #[test]
    fn free_install_charges_nothing_but_reduces_max() {
        let mut ch = test_char(10);
        let graft = cyber_item(CyberwareCategory::InternalBody, false, 14);
        let receipt = install_piece(&mut ch, &graft, CyberwareAffinity::Adept, true, 0).unwrap();
        assert_eq!(receipt.humanity_paid, 0);
        assert_eq!(receipt.max_humanity, 98);
        assert_eq!(receipt.humanity, 98, "born chromed: current clamps to reduced max");
    }

    #[test]
    fn incompatible_race_refused() {
        let graft = cyber_item(CyberwareCategory::Fashionware, false, 0);
        assert_eq!(
            validate_install(CyberwareAffinity::Incompatible, &[], &graft).err(),
            Some(InstallError::Incompatible)
        );
    }

    #[test]
    fn episode_rolls_and_cooldown() {
        let mut ch = test_char(10);
        let graft = cyber_item(CyberwareCategory::InternalBody, false, 14);
        install_piece(&mut ch, &graft, CyberwareAffinity::Normal, false, 0).unwrap();
        // Drive humanity to zero.
        ch.cyberware_state.as_mut().unwrap().humanity = 0;

        // Roll over the 75% chance: nothing.
        assert!(maybe_psychotic_episode(&mut ch, true, 1_000, 76).is_none());
        // Roll under: violent at zero humanity even alone.
        let outcome = maybe_psychotic_episode(&mut ch, false, 1_000, 75).expect("episode");
        assert_eq!(outcome.kind, EPISODE_KIND_VIOLENT);
        assert!(ch.active_buffs.iter().any(|b| b.effect_type == EffectType::Frenzy));
        assert!(ch.active_buffs.iter().any(|b| b.effect_type == EffectType::Rage));

        // No re-fire while Frenzy active (and within cooldown anyway).
        assert!(maybe_psychotic_episode(&mut ch, true, 1_001, 1).is_none());

        // Clear buffs but stay within the zero-humanity cooldown: still none.
        ch.active_buffs.clear();
        assert!(maybe_psychotic_episode(&mut ch, true, 1_000 + EPISODE_COOLDOWN_ZERO_SECS - 1, 1).is_none());
        // Past cooldown: fires again.
        assert!(maybe_psychotic_episode(&mut ch, true, 1_000 + EPISODE_COOLDOWN_ZERO_SECS, 1).is_some());
    }

    #[test]
    fn dissociation_band_stamps_slow_and_luck() {
        let mut ch = test_char(10);
        let graft = cyber_item(CyberwareCategory::InternalBody, false, 7);
        install_piece(&mut ch, &graft, CyberwareAffinity::Normal, false, 0).unwrap();
        // 98 max; 20% = 19.6 -> set humanity for pct 20 (dissociation band).
        ch.cyberware_state.as_mut().unwrap().humanity = 20;
        let max = max_humanity(ch.stat_cha, &ch.cyberware_state.as_ref().unwrap().installed);
        let pct = humanity_pct(20, max);
        assert!(pct >= PSYCHE_VIOLENT_BAND_PCT && pct < PSYCHE_STABLE_PCT);

        let outcome = maybe_psychotic_episode(&mut ch, true, 10_000, 1).expect("episode");
        assert_eq!(outcome.kind, EPISODE_KIND_DISSOCIATION);
        assert!(
            ch.active_buffs
                .iter()
                .any(|b| b.effect_type == EffectType::Slow && b.source == CYBERPSYCHOSIS_BUFF_SOURCE)
        );
        assert!(
            ch.active_buffs
                .iter()
                .any(|b| b.effect_type == EffectType::Luck && b.magnitude == -3)
        );
    }
}

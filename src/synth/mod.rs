//! Synth chassis processing and the shared down-transition / repair logic —
//! testable lib-side. The thin tokio loop wrapper lives in
//! `src/ticks/synth.rs` (bin-only) and just calls these.
//!
//! Shared here (rather than in `src/script/synth.rs`) because the combat,
//! bleeding, and drowning ticks all need the same down-transition rule and
//! the repair core is used by both the `repair` command and technician NPCs.

use crate::SharedConnections;
use crate::db;
use crate::types::{
    ActiveBuff, CHASSIS_TICK_INTERVAL_SECS, CharacterData, CreatureType, EffectType, SYNTH_CRITICAL_DAMAGE_PENALTY,
    SYNTH_CRITICAL_HIT_PENALTY, SYNTH_CRITICAL_REPAIR_KITS, SYNTH_MALFUNCTION_SOURCE, SYNTH_RECENT_COMBAT_WINDOW_SECS,
    SYNTH_REPAIR_COOLDOWN_SECS, SYNTH_REPAIR_KIT_HP_PCT, SYNTH_SHUTDOWN_GRACE_SECS, SYNTH_STAGE_CRITICAL,
    SYNTH_STAGE_DEGRADED, SYNTH_STAGE_FAILING, SYNTH_STAGE_NOMINAL, SynthState, synth_stage_for_hp,
};
use anyhow::Result;
use uuid::Uuid;

pub use crate::types::CHASSIS_TICK_INTERVAL_SECS as SYNTH_CHASSIS_TICK_INTERVAL_SECS;

/// First-person line when the chassis goes critical instead of unconscious.
pub const SYNTH_CRITICAL_MESSAGE: &str = "\x1b[1;31mALERT: core integrity lost. Emergency reserve engaged — SYSTEM \
                                          SHUTDOWN in 300 seconds without repair.\x1b[0m";
/// Room broadcast for the same moment ({name} placeholder).
pub const SYNTH_CRITICAL_ROOM_MESSAGE: &str =
    "{name} staggers but does not fall. Something inside {name} grinds, sparks, and keeps running.";
/// First-person line when a second lethal blow lands while critical.
pub const SYNTH_SHUTDOWN_MESSAGE: &str = "\x1b[1;31mCATASTROPHIC FAILURE. SYSTEM SHUTDOWN.\x1b[0m";
/// Room broadcast for System Shutdown ({name} placeholder).
pub const SYNTH_SHUTDOWN_ROOM_MESSAGE: &str = "{name} freezes mid-motion, eyes dimming, and topples — servos dead.";

/// Outcome of a lethal (HP <= 0) hit on a synth.
pub enum SynthDownOutcome {
    /// First failure: HP floored at 1, malfunction debuffs stamped, shutdown
    /// countdown started. The synth keeps acting — no unconsciousness.
    Critical,
    /// Already critical: this one is fatal. Caller runs the death pipeline.
    Shutdown,
}

/// Apply the "runs broken" rule at a point where a synth's HP just hit 0 or
/// below. Mirrors the vampire SunlightBurning rescue window: the first
/// failure opens a grace period, anything lethal during it finishes the job.
/// Returns `None` when `ch` is not a synth. Caller saves + messages.
pub fn synth_down_transition(ch: &mut CharacterData, now: i64) -> Option<SynthDownOutcome> {
    let state = ch.synth_state.as_mut()?;
    if state.is_critical() {
        return Some(SynthDownOutcome::Shutdown);
    }
    state.malfunction_stage = SYNTH_STAGE_CRITICAL;
    state.shutdown_at = Some(now + SYNTH_SHUTDOWN_GRACE_SECS);
    ch.hp = 1;
    stamp_stage_buffs(&mut ch.active_buffs, SYNTH_STAGE_CRITICAL);
    Some(SynthDownOutcome::Critical)
}

/// Per-character chassis-tick core, separated from the session loop so
/// integration tests can drive it without constructing a PlayerSession.
/// Returns (anything_changed, message_for_player_if_any, shutdown_expired).
pub fn apply_chassis_tick_to_character(ch: &mut CharacterData, now: i64) -> (bool, Option<String>, bool) {
    let Some(state) = ch.synth_state.as_mut() else {
        return (false, None, false);
    };

    let mut modified = false;
    state.last_chassis_tick = now;

    if state.is_critical() {
        // Countdown to System Shutdown; re-assert the critical debuffs so
        // dispel/expiry can't shake them off. A critical chassis without a
        // countdown (legacy save) gets one armed rather than dying instantly.
        let remaining = match state.shutdown_remaining(now) {
            Some(r) => r,
            None => {
                state.shutdown_at = Some(now + SYNTH_SHUTDOWN_GRACE_SECS);
                SYNTH_SHUTDOWN_GRACE_SECS
            }
        };
        if remaining <= 0 {
            return (true, None, true);
        }
        stamp_stage_buffs(&mut ch.active_buffs, SYNTH_STAGE_CRITICAL);
        let msg = format!(
            "\x1b[1;31mWARNING: emergency reserve at {}s. Seek repair.\x1b[0m",
            remaining
        );
        return (true, Some(msg), false);
    }

    // Stages 0-2 are derived from HP each tick; CRITICAL never is.
    let derived = synth_stage_for_hp(ch.hp, ch.max_hp);
    let mut message = None;
    if derived != state.malfunction_stage {
        message = Some(stage_transition_message(state.malfunction_stage, derived).to_string());
        state.malfunction_stage = derived;
        modified = true;
    }
    match derived {
        SYNTH_STAGE_FAILING => stamp_stage_buffs(&mut ch.active_buffs, SYNTH_STAGE_FAILING),
        SYNTH_STAGE_NOMINAL | SYNTH_STAGE_DEGRADED => {
            if clear_malfunction_buffs(&mut ch.active_buffs) {
                modified = true;
            }
        }
        _ => {}
    }
    if message.is_none() && derived >= SYNTH_STAGE_DEGRADED {
        message = maybe_cosmetic_glitch(derived);
    }

    (modified, message, false)
}

/// Per-30s chassis tick over all online synths. Returns the names of synths
/// whose shutdown countdown expired this tick — the bin wrapper finishes the
/// death pipeline (corpse, respawn) since `process_player_death` lives
/// bin-side (the sun-tick pattern).
pub fn process_chassis_tick(db: &db::Db, connections: &SharedConnections) -> Result<Vec<(String, Uuid)>> {
    let now = now_secs();
    let mut shutdowns = Vec::new();

    let mut conns = connections.lock().unwrap();
    for (_conn_id, session) in conns.iter_mut() {
        let ch = match session.character.as_mut() {
            Some(c) => c,
            None => continue,
        };
        if !ch.creation_complete || ch.god_mode || ch.synth_state.is_none() {
            continue;
        }
        let (modified, message, shutdown) = apply_chassis_tick_to_character(ch, now);
        if shutdown {
            let _ = session.sender.send(format!("\n{}\n", SYNTH_SHUTDOWN_MESSAGE));
            ch.hp = 0;
            let _ = db.save_character_data(ch.clone());
            shutdowns.push((ch.name.clone(), ch.current_room_id));
            continue;
        }
        if let Some(msg) = message {
            let _ = session.sender.send(format!("\n{}\n", msg));
        }
        if modified {
            let _ = db.save_character_data(ch.clone());
        }
    }

    Ok(shutdowns)
}

/// Outcome of a repair attempt, for caller-side messaging.
pub enum RepairOutcome {
    /// (hp_restored, new_hp, new_stage)
    Repaired(i32, i32, i32),
    /// Self-repair cooldown still running (seconds remaining).
    OnCooldown(i64),
    /// CRITICAL chassis needs SYNTH_CRITICAL_REPAIR_KITS kits at once.
    NeedsMoreKits(i32),
    /// Already at full HP and NOMINAL.
    Full,
    /// Not a synth.
    NotApplicable,
}

/// Field repair with `kits` repair kits (already validated/consumed by the
/// caller). Heals SYNTH_REPAIR_KIT_HP_PCT of max HP per kit; a CRITICAL
/// chassis demands SYNTH_CRITICAL_REPAIR_KITS in one go to clear the
/// shutdown countdown. Caller saves.
pub fn apply_kit_repair(ch: &mut CharacterData, now: i64, kits: i32) -> RepairOutcome {
    let max_hp = ch.max_hp;
    let hp = ch.hp;
    let Some(state) = ch.synth_state.as_mut() else {
        return RepairOutcome::NotApplicable;
    };
    let cooldown_left = state.last_repair_time + SYNTH_REPAIR_COOLDOWN_SECS - now;
    if cooldown_left > 0 {
        return RepairOutcome::OnCooldown(cooldown_left);
    }
    if state.is_critical() && kits < SYNTH_CRITICAL_REPAIR_KITS {
        return RepairOutcome::NeedsMoreKits(SYNTH_CRITICAL_REPAIR_KITS);
    }
    if !state.is_critical() && hp >= max_hp {
        return RepairOutcome::Full;
    }
    state.last_repair_time = now;
    state.shutdown_at = None;
    let healed = ((max_hp * SYNTH_REPAIR_KIT_HP_PCT * kits.max(1)) / 100).max(1);
    ch.hp = (hp + healed).min(max_hp);
    let new_stage = synth_stage_for_hp(ch.hp, max_hp);
    if let Some(state) = ch.synth_state.as_mut() {
        state.malfunction_stage = new_stage;
    }
    if new_stage < SYNTH_STAGE_FAILING {
        clear_malfunction_buffs(&mut ch.active_buffs);
    } else {
        stamp_stage_buffs(&mut ch.active_buffs, new_stage);
    }
    RepairOutcome::Repaired(healed, ch.hp, new_stage)
}

/// Full workshop restore by a technician NPC: full HP, NOMINAL, countdown
/// cleared, debuffs gone. No cooldown — the gate is gold. Caller saves.
pub fn apply_technician_repair(ch: &mut CharacterData) -> RepairOutcome {
    let max_hp = ch.max_hp;
    let Some(state) = ch.synth_state.as_mut() else {
        return RepairOutcome::NotApplicable;
    };
    if !state.is_critical() && ch.hp >= max_hp {
        return RepairOutcome::Full;
    }
    let healed = max_hp - ch.hp;
    state.malfunction_stage = SYNTH_STAGE_NOMINAL;
    state.shutdown_at = None;
    ch.hp = max_hp;
    clear_malfunction_buffs(&mut ch.active_buffs);
    RepairOutcome::Repaired(healed, max_hp, SYNTH_STAGE_NOMINAL)
}

/// Behavioral inhibitor: a synth may not INITIATE violence against a mortal
/// going about its business. Joining a fight already in progress is fine;
/// so is running down a mortal that fled combat within the recent window.
/// Every other creature type is unrestricted.
pub fn directive_allows_attack(
    creature_type: CreatureType,
    target_in_combat: bool,
    target_last_combat_at: i64,
    now: i64,
) -> bool {
    if creature_type != CreatureType::Mortal {
        return true;
    }
    if target_in_combat {
        return true;
    }
    target_last_combat_at > 0 && now - target_last_combat_at <= SYNTH_RECENT_COMBAT_WINDOW_SECS
}

/// Scale an organic heal (spell/potion) for a synth target: 25%, min 1.
pub fn synth_scaled_heal(amount: i32) -> i32 {
    ((amount * crate::types::SYNTH_HEAL_EFFECT_PCT) / 100).max(1)
}

/// Reset chassis state after death/respawn so a revived synth doesn't come
/// back mid-countdown.
pub fn reset_synth_state_on_death(state: &mut SynthState) {
    state.malfunction_stage = SYNTH_STAGE_NOMINAL;
    state.shutdown_at = None;
}

fn stage_transition_message(old: i32, new: i32) -> &'static str {
    if new > old {
        match new {
            SYNTH_STAGE_DEGRADED => {
                "\x1b[33mDiagnostic: chassis integrity DEGRADED. Performance within tolerances.\x1b[0m"
            }
            _ => "\x1b[1;33mDiagnostic: chassis integrity FAILING. Motor functions impaired. Seek repair.\x1b[0m",
        }
    } else {
        match new {
            SYNTH_STAGE_NOMINAL => "\x1b[36mDiagnostic: chassis integrity NOMINAL.\x1b[0m",
            _ => "\x1b[36mDiagnostic: chassis integrity improving. DEGRADED.\x1b[0m",
        }
    }
}

/// Occasional cosmetic glitch line while running damaged.
fn maybe_cosmetic_glitch(stage: i32) -> Option<String> {
    use rand::Rng;
    let chance = if stage >= SYNTH_STAGE_FAILING { 35 } else { 15 };
    if rand::thread_rng().gen_range(1..=100) > chance {
        return None;
    }
    const GLITCHES: [&str; 4] = [
        "A servo in your shoulder whines and catches.",
        "Your vision rasters, drops a frame, recovers.",
        "Something under your synthetic skin clicks twice, then stops.",
        "A thin wisp of ozone rises from a seam in your chassis.",
    ];
    let i = rand::thread_rng().gen_range(0..GLITCHES.len());
    Some(format!("\x1b[90m{}\x1b[0m", GLITCHES[i]))
}

/// Stamp the malfunction debuffs for a stage (FAILING or CRITICAL); the
/// chassis tick re-asserts them so expiry/dispel can't shake them off.
fn stamp_stage_buffs(buffs: &mut Vec<ActiveBuff>, stage: i32) {
    let secs = (CHASSIS_TICK_INTERVAL_SECS as i32) * 2;
    push_buff(buffs, EffectType::Slow, 1, secs, SYNTH_MALFUNCTION_SOURCE);
    if stage >= SYNTH_STAGE_CRITICAL {
        push_buff(
            buffs,
            EffectType::HitBonus,
            SYNTH_CRITICAL_HIT_PENALTY,
            secs,
            SYNTH_MALFUNCTION_SOURCE,
        );
        push_buff(
            buffs,
            EffectType::DamageBonus,
            SYNTH_CRITICAL_DAMAGE_PENALTY,
            secs,
            SYNTH_MALFUNCTION_SOURCE,
        );
    }
}

/// Remove every malfunction-sourced buff. Returns true if any were removed.
fn clear_malfunction_buffs(buffs: &mut Vec<ActiveBuff>) -> bool {
    let before = buffs.len();
    buffs.retain(|b| b.source != SYNTH_MALFUNCTION_SOURCE);
    buffs.len() != before
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

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

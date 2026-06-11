//! Replicant tick processing and shared breakdown/retirement logic —
//! testable lib-side. The thin tokio loop wrapper lives in
//! `src/ticks/replicant.rs` (bin-only) and just calls these.
//!
//! Shared here (rather than in `src/script/replicant.rs`) because the
//! combat tick and the Rhai bindings both need to trigger breakdowns and
//! the logic must stay identical: drop to 0 Resolve from a big hit and
//! from a reckless `push` roll on the same critical-stress table.

use crate::SharedConnections;
use crate::db;
use crate::types::{
    ActiveBuff, BREAKDOWN_DURATION_SECS, BREAKDOWN_KIND_BERSERK, BREAKDOWN_KIND_LOCKUP, BREAKDOWN_KIND_PANIC,
    BREAKDOWN_RESET_RESOLVE, COMFORT_RECEIVE_COOLDOWN_SECS, COMFORT_RESOLVE_RESTORE, CharacterData, CharacterPosition,
    EffectType, RESOLVE_REGEN_SLEEPING, RETIREMENT_DEBUFF_SECS, RETIREMENT_STAT_PENALTY, RETIREMENT_TRAIT,
    roll_breakdown,
};
use anyhow::Result;

pub use crate::types::RESOLVE_TICK_INTERVAL_SECS;

/// Outcome of a triggered breakdown, for caller-side messaging.
pub struct BreakdownOutcome {
    /// "panic" | "lockup" | "berserk"
    pub kind: &'static str,
    /// First-person message for the breaking-down replicant.
    pub message: &'static str,
    /// Third-person room broadcast ({name} placeholder).
    pub room_message: &'static str,
}

/// Roll the critical-stress table and apply a breakdown to `ch`.
/// `roll_1d6` is passed in so tests are deterministic; live callers roll.
/// Caller is responsible for saving the character (and session sync if the
/// mutation didn't happen on the session copy) and for messaging.
pub fn trigger_breakdown(ch: &mut CharacterData, now: i64, roll_1d6: i32) -> BreakdownOutcome {
    let kind = roll_breakdown(roll_1d6);
    if let Some(r) = ch.replicant_state.as_mut() {
        r.breakdown_until = Some(now + BREAKDOWN_DURATION_SECS);
        r.breakdown_kind = Some(kind.to_string());
        r.breakdowns_since_baseline += 1;
        // Snap back so the player isn't chain-broken the moment it expires.
        r.set_resolve(BREAKDOWN_RESET_RESOLVE);
    }
    match kind {
        BREAKDOWN_KIND_BERSERK => {
            // Existing Frenzy plumbing: +damage in the combat tick, flee
            // blocked in flee.rhai. Rage makes it truly uncontrolled — the
            // combat tick's rage pass forces attacks on whoever is in the
            // room (mobiles anywhere outside safe zones, players in PvP
            // zones) until the breakdown passes.
            push_buff(
                &mut ch.active_buffs,
                EffectType::Frenzy,
                2,
                BREAKDOWN_DURATION_SECS as i32,
                "breakdown",
            );
            push_buff(
                &mut ch.active_buffs,
                EffectType::Rage,
                1,
                BREAKDOWN_DURATION_SECS as i32,
                "breakdown",
            );
            BreakdownOutcome {
                kind,
                message: "Your vision tunnels. Something engineered and wordless takes the controls.",
                room_message: "{name}'s eyes empty out. Something else is driving now.",
            }
        }
        BREAKDOWN_KIND_LOCKUP => {
            // In combat the existing stun gate skips their turns; out of
            // combat, go.rhai/attack.rhai check the breakdown kind.
            ch.combat.stun_rounds_remaining = ch.combat.stun_rounds_remaining.max(2);
            BreakdownOutcome {
                kind,
                message: "Your limbs refuse. You stand perfectly still, eyes open, somewhere else entirely.",
                room_message: "{name} goes rigid, eyes open, perfectly still.",
            }
        }
        _ => {
            // Panic: shaken nerves — luck penalty and slowed reactions.
            push_buff(
                &mut ch.active_buffs,
                EffectType::Luck,
                -3,
                BREAKDOWN_DURATION_SECS as i32,
                "breakdown",
            );
            push_buff(
                &mut ch.active_buffs,
                EffectType::Slow,
                1,
                BREAKDOWN_DURATION_SECS as i32,
                "breakdown",
            );
            BreakdownOutcome {
                kind: BREAKDOWN_KIND_PANIC,
                message: "Your hands will not stop shaking. Every shadow holds a blade runner.",
                room_message: "{name} flinches from nothing, breathing fast and shallow.",
            }
        }
    }
}

/// Convenience for live callers: roll 1d6 and trigger.
pub fn trigger_breakdown_rolled(ch: &mut CharacterData, now: i64) -> BreakdownOutcome {
    use rand::Rng;
    let roll = rand::thread_rng().gen_range(1..=6);
    trigger_breakdown(ch, now, roll)
}

/// Apply a retirement order ("recalibration"): 24h all-stats debuff, the
/// persistent `retirement_order` trait, strike reset. Caller saves+messages.
pub fn apply_retirement(ch: &mut CharacterData) {
    const STAT_BUFFS: [EffectType; 6] = [
        EffectType::StrengthBoost,
        EffectType::DexterityBoost,
        EffectType::ConstitutionBoost,
        EffectType::IntelligenceBoost,
        EffectType::WisdomBoost,
        EffectType::CharismaBoost,
    ];
    for effect in STAT_BUFFS {
        push_buff(
            &mut ch.active_buffs,
            effect,
            -RETIREMENT_STAT_PENALTY,
            RETIREMENT_DEBUFF_SECS as i32,
            "recalibration",
        );
    }
    if !ch.traits.iter().any(|t| t == RETIREMENT_TRAIT) {
        ch.traits.push(RETIREMENT_TRAIT.to_string());
    }
    if let Some(r) = ch.replicant_state.as_mut() {
        r.baseline_strikes = 0;
        r.breakdowns_since_baseline = 0;
        r.retirement_count += 1;
    }
}

/// Result of trying to comfort a replicant. Drives the extra messaging the
/// `comfort` social layers on top of its normal output.
pub enum ComfortOutcome {
    /// Resolve restored; carries (restored, new_resolve, max_resolve).
    Restored(i32, i32, i32),
    /// Recipient-side cooldown still running.
    TooRattled,
    /// Already at full resolve.
    Full,
    /// Target is not a replicant (or not online) — comfort is just words.
    NotApplicable,
}

/// Apply the comfort restore to an online replicant by character name.
/// Mutates the session copy (session is authoritative) and saves. Called
/// from the `comfort` social dispatcher and the Rhai binding.
pub fn comfort_replicant_by_name(db: &db::Db, connections: &SharedConnections, target_name: &str) -> ComfortOutcome {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let mut conns = match connections.lock() {
        Ok(g) => g,
        Err(_) => return ComfortOutcome::NotApplicable,
    };
    let target_lower = target_name.to_lowercase();
    let ch = match conns
        .values_mut()
        .filter_map(|s| s.character.as_mut())
        .find(|c| c.name.to_lowercase() == target_lower)
    {
        Some(c) => c,
        None => return ComfortOutcome::NotApplicable,
    };
    let r = match ch.replicant_state.as_mut() {
        Some(r) => r,
        None => return ComfortOutcome::NotApplicable,
    };
    if now < r.comfort_cooldown_until {
        return ComfortOutcome::TooRattled;
    }
    if r.resolve >= r.max_resolve {
        return ComfortOutcome::Full;
    }
    r.comfort_cooldown_until = now + COMFORT_RECEIVE_COOLDOWN_SECS;
    let new_resolve = r.change_resolve(COMFORT_RESOLVE_RESTORE);
    let max = r.max_resolve;
    let _ = db.save_character_data(ch.clone());
    ComfortOutcome::Restored(COMFORT_RESOLVE_RESTORE, new_resolve, max)
}

/// Per-character resolve-tick core, separated from the session loop so
/// integration tests can drive it without constructing a PlayerSession.
/// Returns (anything_changed, message_for_player_if_any).
pub fn apply_resolve_tick_to_character(ch: &mut CharacterData, now: i64) -> (bool, Option<&'static str>) {
    let r = match ch.replicant_state.as_mut() {
        Some(r) => r,
        None => return (false, None),
    };

    let mut modified = false;
    let mut message = None;

    // Breakdown expiry.
    if let Some(until) = r.breakdown_until {
        if now >= until {
            r.breakdown_until = None;
            r.breakdown_kind = None;
            modified = true;
            message = Some("\n\x1b[36mYour mind quiets. The episode passes.\x1b[0m\n");
        }
    }

    // Slow recovery while sleeping (not during a breakdown).
    let breaking_down = r.is_breaking_down(now);
    if !breaking_down && ch.position == CharacterPosition::Sleeping && r.resolve < r.max_resolve {
        r.change_resolve(RESOLVE_REGEN_SLEEPING);
        modified = true;
    }
    r.last_resolve_tick = now;

    // Belt-and-braces tireless invariant.
    if ch.stamina != ch.max_stamina {
        ch.stamina = ch.max_stamina;
        modified = true;
    }

    (modified, message)
}

/// Per-minute resolve tick: expire breakdowns, trickle resolve back while
/// sleeping, keep the tireless-stamina invariant pinned.
pub fn process_resolve_tick(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let mut conns = connections.lock().unwrap();
    for (_conn_id, session) in conns.iter_mut() {
        let ch = match session.character.as_mut() {
            Some(c) => c,
            None => continue,
        };
        if !ch.creation_complete || ch.god_mode || ch.replicant_state.is_none() {
            continue;
        }
        let (modified, message) = apply_resolve_tick_to_character(ch, now);
        if let Some(msg) = message {
            let _ = session.sender.send(msg.to_string());
        }
        if modified {
            let _ = db.save_character_data(ch.clone());
        }
    }

    Ok(())
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

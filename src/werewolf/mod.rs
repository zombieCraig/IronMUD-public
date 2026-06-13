//! Werewolf rage-tick processing and shared frenzy/form logic — testable
//! lib-side. The thin tokio loop wrapper lives in `src/ticks/werewolf.rs`
//! (bin-only) and just calls these.
//!
//! Shared here (rather than in `src/script/werewolf.rs`) because the combat
//! tick (rage gain on damage/kills) and the Rhai bindings (shift, build)
//! both trigger frenzy rolls and the math must stay identical.
//!
//! Frenzy reuses the vampire constants and buff pair exactly: Frenzy
//! (+damage in the combat tick, flee blocked) + Rage (forced target
//! acquisition), source `"rage"`. Tribe banes ride the same generic
//! `frenzy_dc_modifier` trait-effect extraction the clan banes use — the
//! tick wrapper's map covers `tribe_*` traits for free because it walks ALL
//! trait definitions.

use crate::SharedConnections;
use crate::db;
use crate::types::{
    ActiveBuff, CRINOS_SHIFT_COST, CharacterData, EffectType, FORM_CRINOS, FORM_HOMID, FORM_LUPUS, LUPUS_SHIFT_COST,
    RAGE_DECAY_PER_TICK, RAGE_FRENZY_THRESHOLD, WEREWOLF_FORM_SOURCE, WerewolfState,
};
use crate::vampire::{HUNGER_FRENZY_DAMAGE_BONUS, HUNGER_FRENZY_DURATION_SECS};
use anyhow::Result;
use std::collections::HashMap;

pub use crate::types::RAGE_TICK_INTERVAL_SECS;

/// Chance (0-100) that high rage tips a Garou into frenzy on a given roll.
/// Zero below the testing range; at rage 10 with no tribe modifier the wolf
/// wins 60% of rolls. `dc_modifier` is the summed `frenzy_dc_modifier`
/// trait effect (Get of Fenris −2 → 90% at rage 10; Children of Gaia +2 →
/// 30%): a negative modifier lowers the threshold, raising the chance.
pub fn rage_frenzy_chance(rage: i32, dc_modifier: i32) -> i32 {
    (((rage - 6) - dc_modifier) * 15).clamp(0, 100)
}

/// Roll the frenzy check for a rage-hot Garou and stamp the Frenzy + Rage
/// buff pair (source "rage") on a failure to resist. No-op below the
/// frenzy threshold, and never stacks onto an active frenzy/rage.
/// `roll_1d100` is injected so tests are deterministic; live callers roll.
/// Returns true when the frenzy fired. Caller saves and messages.
pub fn maybe_rage_frenzy(
    w: &mut WerewolfState,
    buffs: &mut Vec<ActiveBuff>,
    dc_modifier: i32,
    now: i64,
    roll_1d100: i32,
) -> bool {
    if w.rage < RAGE_FRENZY_THRESHOLD {
        return false;
    }
    if has_buff(buffs, EffectType::Frenzy) || has_buff(buffs, EffectType::Rage) {
        return false;
    }
    if roll_1d100 > rage_frenzy_chance(w.rage, dc_modifier) {
        return false;
    }
    push_or_refresh_buff_secs(
        buffs,
        EffectType::Frenzy,
        HUNGER_FRENZY_DAMAGE_BONUS,
        "rage",
        HUNGER_FRENZY_DURATION_SECS,
    );
    push_or_refresh_buff_secs(buffs, EffectType::Rage, 1, "rage", HUNGER_FRENZY_DURATION_SECS);
    w.frenzy_until = Some(now + HUNGER_FRENZY_DURATION_SECS as i64);
    true
}

/// Convenience for live callers: roll 1d100 and run the frenzy check.
pub fn maybe_rage_frenzy_rolled(
    w: &mut WerewolfState,
    buffs: &mut Vec<ActiveBuff>,
    dc_modifier: i32,
    now: i64,
) -> bool {
    use rand::Rng;
    let roll = rand::thread_rng().gen_range(1..=100);
    maybe_rage_frenzy(w, buffs, dc_modifier, now, roll)
}

/// Add rage to a character and, if the gain slammed into the cap, force a
/// frenzy roll immediately (the moment of overflow, not the next tick).
/// Returns (new_rage, frenzied). No-op (0, false) for non-werewolves.
/// Caller saves and messages.
pub fn gain_rage_rolled(ch: &mut CharacterData, amount: i32, dc_modifier: i32, now: i64) -> (i32, bool) {
    let Some(mut w) = ch.werewolf_state.take() else {
        return (0, false);
    };
    let hit_max = w.gain_rage(amount);
    let mut frenzied = false;
    if hit_max {
        frenzied = maybe_rage_frenzy_rolled(&mut w, &mut ch.active_buffs, dc_modifier, now);
    }
    let rage = w.rage;
    ch.werewolf_state = Some(w);
    (rage, frenzied)
}

/// The form buff table. Persistent (re-stamped each rage tick, the mutation
/// passive-reassertion pattern) so dispel/expiry can't strip a war-form.
pub fn form_buffs(form: &str) -> Vec<(EffectType, i32)> {
    match form {
        FORM_CRINOS => vec![
            (EffectType::StrengthBoost, 4),
            (EffectType::ConstitutionBoost, 2),
            (EffectType::DamageBonus, 3),
            (EffectType::CharismaBoost, -6),
        ],
        FORM_LUPUS => vec![
            (EffectType::DexterityBoost, 3),
            (EffectType::Haste, 1),
            (EffectType::Luck, 10),
        ],
        _ => Vec::new(),
    }
}

/// Rage cost to shift INTO a form (homid is free).
pub fn shift_cost(form: &str) -> i32 {
    match form {
        FORM_CRINOS => CRINOS_SHIFT_COST,
        FORM_LUPUS => LUPUS_SHIFT_COST,
        _ => 0,
    }
}

pub fn is_known_form(form: &str) -> bool {
    matches!(form, FORM_HOMID | FORM_CRINOS | FORM_LUPUS)
}

/// Replace every form-sourced buff with the set for `form`. Used by the
/// shift transaction and the per-tick reassertion.
pub fn apply_form_buffs(buffs: &mut Vec<ActiveBuff>, form: &str) {
    buffs.retain(|b| b.source != WEREWOLF_FORM_SOURCE);
    let secs = (RAGE_TICK_INTERVAL_SECS as i32) * 2;
    for (effect, magnitude) in form_buffs(form) {
        buffs.push(ActiveBuff {
            effect_type: effect,
            magnitude,
            remaining_secs: secs,
            source: WEREWOLF_FORM_SOURCE.to_string(),
            damage_type: None,
            vs_effect: None,
            skill_key: None,
        });
    }
}

/// Per-character rage-tick core, separated from the session loop so
/// integration tests can drive it without constructing a PlayerSession.
/// `roll_1d100` injected for determinism. Returns (anything_changed,
/// message_for_player_if_any, frenzied_this_tick).
pub fn apply_rage_tick_to_character(
    ch: &mut CharacterData,
    now: i64,
    dc_modifier: i32,
    roll_1d100: i32,
) -> (bool, Option<&'static str>, bool) {
    let Some(mut w) = ch.werewolf_state.take() else {
        return (false, None, false);
    };

    let mut modified = false;
    let mut message = None;

    // Frenzy expiry bookkeeping (the buffs expire on their own; this clears
    // the informational mirror).
    if let Some(until) = w.frenzy_until {
        if now >= until {
            w.frenzy_until = None;
            modified = true;
            message = Some("\n\x1b[36mThe red tide recedes. Your own eyes look out again.\x1b[0m\n");
        }
    }

    // Rage cools out of combat; in combat it only moves via gains.
    if !ch.combat.in_combat && w.rage > 0 {
        w.set_rage(w.rage - RAGE_DECAY_PER_TICK);
        modified = true;
    }

    // The wolf tests the cage while rage runs hot.
    let mut frenzied = false;
    if w.rage >= RAGE_FRENZY_THRESHOLD && maybe_rage_frenzy(&mut w, &mut ch.active_buffs, dc_modifier, now, roll_1d100)
    {
        frenzied = true;
        modified = true;
    }

    // Re-assert the current form's buffs so expiry/dispel can't strip them.
    if w.current_form != FORM_HOMID {
        apply_form_buffs(&mut ch.active_buffs, &w.current_form);
        modified = true;
    }

    w.last_rage_tick = now;
    ch.werewolf_state = Some(w);
    (modified, message, frenzied)
}

/// Per-minute rage tick over all online werewolves. `tribe_frenzy_mods`
/// maps trait name → `frenzy_dc_modifier` (extracted from trait_definitions
/// by the tick wrapper, identical to the clan-bane map — pass empty to
/// ignore banes).
pub fn process_rage_tick(
    db: &db::Db,
    connections: &SharedConnections,
    tribe_frenzy_mods: &HashMap<String, i32>,
) -> Result<()> {
    use rand::Rng;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // Collect frenzy broadcasts to send after the per-session pass so we
    // don't iterate sessions inside the mutable iteration.
    let mut frenzy_rooms: Vec<(uuid::Uuid, String)> = Vec::new();

    {
        let mut conns = connections.lock().unwrap();
        for (_conn_id, session) in conns.iter_mut() {
            let ch = match session.character.as_mut() {
                Some(c) => c,
                None => continue,
            };
            if !ch.creation_complete || ch.god_mode || ch.werewolf_state.is_none() {
                continue;
            }
            let dc_modifier: i32 = ch.traits.iter().filter_map(|t| tribe_frenzy_mods.get(t)).sum();
            let roll = rand::thread_rng().gen_range(1..=100);
            let (modified, message, frenzied) = apply_rage_tick_to_character(ch, now, dc_modifier, roll);
            if frenzied {
                let _ = session.sender.send(
                    "\n\x1b[1;31mThe Rage crests. Fur splits skin — the wolf is driving now.\x1b[0m\n".to_string(),
                );
                frenzy_rooms.push((ch.current_room_id, ch.name.clone()));
            }
            if let Some(msg) = message {
                let _ = session.sender.send(msg.to_string());
            }
            if modified {
                let _ = db.save_character_data(ch.clone());
            }
        }
    }

    for (room_id, name) in frenzy_rooms {
        broadcast_frenzy_to_room(connections, &room_id, &name);
    }

    Ok(())
}

/// Room broadcast when a Garou loses it (mirrors the vampire bloodlust
/// broadcast). Skips the frenzied werewolf themself.
fn broadcast_frenzy_to_room(connections: &SharedConnections, room_id: &uuid::Uuid, frenzied_name: &str) {
    let conns = match connections.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    for session in conns.values() {
        let Some(ch) = session.character.as_ref() else { continue };
        if ch.current_room_id != *room_id || ch.name.eq_ignore_ascii_case(frenzied_name) {
            continue;
        }
        let _ = session.sender.send(format!(
            "\n\x1b[1;31m{}'s eyes flood amber. Something with too many teeth is wearing their face.\x1b[0m\n",
            frenzied_name
        ));
    }
}

fn has_buff(buffs: &[ActiveBuff], effect_type: EffectType) -> bool {
    buffs.iter().any(|b| b.effect_type == effect_type)
}

fn push_or_refresh_buff_secs(
    buffs: &mut Vec<ActiveBuff>,
    effect_type: EffectType,
    magnitude: i32,
    source: &str,
    secs: i32,
) {
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

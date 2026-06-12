//! Mutant tick processing and shared activation/misfire logic — testable
//! lib-side. The thin tokio loop wrappers live in `src/ticks/mutant.rs`
//! (bin-only) and just call these.
//!
//! Two ticks:
//! - **Mutation tick**: re-asserts passive mutation buffs/traits that a
//!   temporary same-`EffectType` buff may have clobbered (`apply_buff`
//!   replaces same-type buffs, so a 60s `armor` spell would otherwise eat an
//!   Insectoid's permanent plating). Deliberately does NOT regenerate MP —
//!   the push economy is the whole point: power costs flesh.
//! - **Rot tick**: world plumbing. EVERY character (any race) standing in a
//!   rotted room (`RoomData.rot_level` 1-3) accumulates rot points on a
//!   level-scaled cadence; each gain rolls total-rot d6s and every 1 is a
//!   point of damage, so exposure snowballs. Clean rooms shed 1 point per
//!   slow interval — but each shed point has a small chance of becoming
//!   permanent. Mutants gain at half rate and take half damage; Rot-Eater
//!   mutants metabolize would-be gains into MP instead.

use crate::SharedConnections;
use crate::db;
use crate::types::{
    ActiveBuff, CharacterData, DEFORMITIES, EffectType, MISFIRE_KIND_DEFORMITY, MISFIRE_KIND_POWER_LOSS,
    MISFIRE_KIND_SELF_TRAUMA, MUTANT_ROT_DAMAGE_DIVISOR, MutationDefinition, ROT_DECAY_INTERVAL_SECS, ROT_LEVEL_MAX,
    ROT_PERMANENT_CHANCE_PCT, misfire_occurred, roll_misfire_kind, rot_damage_from_dice, rot_gain_interval_secs,
};
use anyhow::Result;
use std::collections::HashMap;

pub use crate::types::{MUTATION_TICK_INTERVAL_SECS, ROT_TICK_INTERVAL_SECS};

/// Buff source tag for permanent passive-mutation buffs. The mutation tick
/// re-asserts any missing buff carrying this source.
pub const MUTATION_BUFF_SOURCE: &str = "mutation";

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Everything a `mutate` activation produced, for caller-side narration.
#[derive(Debug, Default)]
pub struct ActivationOutcome {
    /// Scaled power: `base_power + power_per_mp * mp_spent`.
    pub power: i32,
    /// Scaled duration: `duration_secs + duration_per_mp * mp_spent`.
    pub duration_secs: i64,
    pub mp_left: i32,
    pub misfire: bool,
    /// One of the MISFIRE_KIND_* constants when `misfire`.
    pub misfire_kind: Option<&'static str>,
    /// Self-trauma damage dealt by the misfire (0 otherwise).
    pub misfire_damage: i32,
    /// Cosmetic deformity gained (Deformity misfire).
    pub deformity: Option<String>,
    /// Mutation id gained by an Overload misfire.
    pub new_mutation: Option<String>,
    /// Stat key ("str".."cha") permanently lost to an Overload misfire.
    pub stat_lost: Option<String>,
    /// max_mp bumped instead (Overload with every mutation already owned).
    pub max_mp_increased: bool,
}

/// Dice bundle for `activate_mutation` so tests are deterministic. Live
/// callers use `activation_dice_rolled`.
pub struct ActivationDice {
    /// One d6 per MP spent; any 1 misfires.
    pub misfire_dice: Vec<i32>,
    /// d6 severity roll on the misfire table.
    pub severity_roll: i32,
    /// d6 trauma roll (SelfTrauma misfire damage = this + mp_spent).
    pub trauma_roll: i32,
    /// Index entropy for deformity / stat / new-mutation picks.
    pub pick_roll: usize,
}

pub fn activation_dice_rolled(mp_spent: i32) -> ActivationDice {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    ActivationDice {
        misfire_dice: (0..mp_spent.max(0)).map(|_| rng.gen_range(1..=6)).collect(),
        severity_roll: rng.gen_range(1..=6),
        trauma_roll: rng.gen_range(1..=6),
        pick_roll: rng.gen_range(0..usize::MAX),
    }
}

const STAT_KEYS: [&str; 6] = ["str", "dex", "con", "int", "wis", "cha"];
/// Overload never drops a stat below this.
const OVERLOAD_STAT_FLOOR: i32 = 3;

/// Spend MP on a mutation power and resolve any misfire, mutating `ch`
/// in place (MP, HP, deformities, stats, new mutations). Caller is
/// responsible for verifying ownership/MP beforehand, for saving, and for
/// narrating from the returned outcome. `all_mutation_ids` feeds the
/// Overload new-mutation pick (pass every loaded definition id).
pub fn activate_mutation(
    ch: &mut CharacterData,
    def: &MutationDefinition,
    mp_spent: i32,
    all_mutation_ids: &[String],
    dice: &ActivationDice,
) -> ActivationOutcome {
    let mut out = ActivationOutcome {
        power: def.base_power + def.power_per_mp * mp_spent,
        duration_secs: def.duration_secs + def.duration_per_mp * mp_spent as i64,
        ..Default::default()
    };

    let Some(state) = ch.mutant_state.as_mut() else {
        return out;
    };
    state.change_mp(-mp_spent);
    out.mp_left = state.mp;

    if !misfire_occurred(&dice.misfire_dice) {
        return out;
    }
    out.misfire = true;
    let kind = roll_misfire_kind(dice.severity_roll);
    out.misfire_kind = Some(kind);

    match kind {
        MISFIRE_KIND_POWER_LOSS => {
            state.set_mp(0);
            out.mp_left = 0;
        }
        MISFIRE_KIND_SELF_TRAUMA => {
            let dmg = dice.trauma_roll + mp_spent;
            ch.hp = (ch.hp - dmg).max(1);
            out.misfire_damage = dmg;
        }
        MISFIRE_KIND_DEFORMITY => {
            // Prefer one the character doesn't already carry; repeats are
            // fine once the list is exhausted.
            let fresh: Vec<&str> = DEFORMITIES
                .iter()
                .copied()
                .filter(|d| !state.deformities.iter().any(|owned| owned == d))
                .collect();
            let pool: &[&str] = if fresh.is_empty() { &DEFORMITIES } else { &fresh };
            let picked = pool[dice.pick_roll % pool.len()].to_string();
            state.deformities.push(picked.clone());
            out.deformity = Some(picked);
        }
        _ => {
            // Overload: the body pays, the Zone gives.
            let unowned: Vec<&String> = all_mutation_ids
                .iter()
                .filter(|id| !state.mutations.iter().any(|owned| owned == *id))
                .collect();
            if unowned.is_empty() {
                state.max_mp += 1;
                out.max_mp_increased = true;
            } else {
                let picked = unowned[dice.pick_roll % unowned.len()].clone();
                state.mutations.push(picked.clone());
                out.new_mutation = Some(picked);
            }
            // Permanent attribute loss, floor 3. Pick offset by a different
            // derivation of the entropy so it doesn't correlate with the
            // mutation pick.
            let start = dice.pick_roll / 7 % STAT_KEYS.len();
            for offset in 0..STAT_KEYS.len() {
                let key = STAT_KEYS[(start + offset) % STAT_KEYS.len()];
                let stat = match key {
                    "str" => &mut ch.stat_str,
                    "dex" => &mut ch.stat_dex,
                    "con" => &mut ch.stat_con,
                    "int" => &mut ch.stat_int,
                    "wis" => &mut ch.stat_wis,
                    _ => &mut ch.stat_cha,
                };
                if *stat > OVERLOAD_STAT_FLOOR {
                    *stat -= 1;
                    out.stat_lost = Some(key.to_string());
                    break;
                }
            }
        }
    }
    out
}

/// Pick a random mutation id from `all` excluding `owned`. `pick_roll` is
/// caller entropy. None when everything is owned (or `all` is empty).
pub fn roll_random_mutation(all: &[String], owned: &[String], pick_roll: usize) -> Option<String> {
    let pool: Vec<&String> = all.iter().filter(|id| !owned.iter().any(|o| o == *id)).collect();
    if pool.is_empty() {
        return None;
    }
    Some(pool[pick_roll % pool.len()].clone())
}

/// Ensure every owned passive mutation's permanent buffs and granted traits
/// are present on the character. Returns true when anything was (re)stamped.
pub fn ensure_passive_effects(ch: &mut CharacterData, defs: &HashMap<String, MutationDefinition>) -> bool {
    let owned: Vec<String> = match ch.mutant_state.as_ref() {
        Some(s) => s.mutations.clone(),
        None => return false,
    };
    let mut modified = false;
    for id in owned {
        let Some(def) = defs.get(&id) else { continue };
        for pb in &def.passive_buffs {
            let Some(effect_type) = EffectType::from_str(&pb.effect) else {
                continue;
            };
            let held = ch
                .active_buffs
                .iter()
                .any(|b| b.effect_type == effect_type && b.source == MUTATION_BUFF_SOURCE);
            if !held {
                ch.active_buffs.push(ActiveBuff {
                    effect_type,
                    magnitude: pb.magnitude,
                    remaining_secs: -1, // permanent: expiry pass keeps these
                    source: MUTATION_BUFF_SOURCE.to_string(),
                    damage_type: None,
                    vs_effect: None,
                    skill_key: None,
                });
                modified = true;
            }
        }
        for t in &def.granted_traits {
            if !ch.traits.iter().any(|owned_t| owned_t == t) {
                ch.traits.push(t.clone());
                modified = true;
            }
        }
    }
    modified
}

/// Per-minute mutation tick: keep passive mutation effects asserted.
pub fn process_mutation_tick(
    db: &db::Db,
    connections: &SharedConnections,
    defs: &HashMap<String, MutationDefinition>,
) -> Result<()> {
    let mut conns = connections.lock().unwrap();
    for (_conn_id, session) in conns.iter_mut() {
        let ch = match session.character.as_mut() {
            Some(c) => c,
            None => continue,
        };
        if !ch.creation_complete || ch.mutant_state.is_none() {
            continue;
        }
        if ensure_passive_effects(ch, defs) {
            let _ = db.save_character_data(ch.clone());
        }
    }
    Ok(())
}

/// What the rot tick did to one character, for messaging/testing.
#[derive(Debug, PartialEq)]
pub enum RotTickOutcome {
    Nothing,
    /// Clock (re)started — first tick in a rot zone after a gap.
    ClockStarted,
    /// Gained a rot point; carries damage taken (0 = lucky roll).
    Gained(i32),
    /// Rot-Eater fed: +1 MP instead of a rot point.
    RotEaterFed,
    /// Shed a point cleanly in a rot-free room.
    Decayed,
    /// Shed a point but it scarred in: permanent_rot_points += 1.
    DecayedPermanent,
}

/// Per-character rot-tick core. `rot_level` is the character's current
/// room's level; `damage_dice`/`permanent_roll` are caller dice (damage_dice
/// must hold at least `rot_points + permanent_rot_points + 1` d6s when a
/// gain is possible; `permanent_roll` is d100). Pure: caller saves+messages.
pub fn apply_rot_tick_to_character(
    ch: &mut CharacterData,
    rot_level: i32,
    now: i64,
    damage_dice: &[i32],
    permanent_roll: i32,
) -> RotTickOutcome {
    let is_mutant = ch.mutant_state.is_some();
    let is_rot_eater = ch
        .mutant_state
        .as_ref()
        .map(|s| s.has_mutation("rot_eater"))
        .unwrap_or(false);

    let level = rot_level.clamp(0, ROT_LEVEL_MAX);
    if level >= 1 {
        let interval = match rot_gain_interval_secs(level, is_mutant) {
            Some(i) => i,
            None => return RotTickOutcome::Nothing,
        };
        // A zero or stale clock means we just (re)entered contamination:
        // start the clock instead of back-charging the player for time they
        // didn't spend here.
        if ch.last_rot_gain_time == 0 || now - ch.last_rot_gain_time > interval * 2 {
            ch.last_rot_gain_time = now;
            return RotTickOutcome::ClockStarted;
        }
        if now - ch.last_rot_gain_time < interval {
            return RotTickOutcome::Nothing;
        }
        ch.last_rot_gain_time = now;
        if is_rot_eater {
            if let Some(s) = ch.mutant_state.as_mut() {
                if s.mp < s.max_mp {
                    s.change_mp(1);
                    return RotTickOutcome::RotEaterFed;
                }
            }
            return RotTickOutcome::Nothing;
        }
        ch.rot_points += 1;
        let total = (ch.rot_points + ch.permanent_rot_points) as usize;
        let mut dmg = rot_damage_from_dice(&damage_dice[..total.min(damage_dice.len())]);
        if is_mutant {
            dmg /= MUTANT_ROT_DAMAGE_DIVISOR;
        }
        if dmg > 0 {
            ch.hp = (ch.hp - dmg).max(1);
        }
        return RotTickOutcome::Gained(dmg);
    }

    // Rot-free room: slow decontamination with a permanence risk.
    if ch.rot_points <= 0 {
        return RotTickOutcome::Nothing;
    }
    if ch.last_rot_decay_time == 0 || now - ch.last_rot_decay_time > ROT_DECAY_INTERVAL_SECS * 2 {
        ch.last_rot_decay_time = now;
        return RotTickOutcome::ClockStarted;
    }
    if now - ch.last_rot_decay_time < ROT_DECAY_INTERVAL_SECS {
        return RotTickOutcome::Nothing;
    }
    ch.last_rot_decay_time = now;
    ch.rot_points -= 1;
    if permanent_roll < ROT_PERMANENT_CHANCE_PCT {
        ch.permanent_rot_points += 1;
        return RotTickOutcome::DecayedPermanent;
    }
    RotTickOutcome::Decayed
}

/// Per-minute world rot tick: every online character, any race.
pub fn process_rot_tick(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    use rand::Rng;
    let now = unix_now();
    let mut conns = connections.lock().unwrap();
    for (_conn_id, session) in conns.iter_mut() {
        let ch = match session.character.as_mut() {
            Some(c) => c,
            None => continue,
        };
        if !ch.creation_complete || ch.god_mode {
            continue;
        }
        let rot_level = match db.get_room_data(&ch.current_room_id) {
            Ok(Some(room)) => room.rot_level,
            _ => 0,
        };
        let (damage_dice, permanent_roll) = {
            let mut rng = rand::thread_rng();
            let n = (ch.rot_points + ch.permanent_rot_points + 1).max(1) as usize;
            (
                (0..n).map(|_| rng.gen_range(1..=6)).collect::<Vec<i32>>(),
                rng.gen_range(0..100),
            )
        };
        let outcome = apply_rot_tick_to_character(ch, rot_level, now, &damage_dice, permanent_roll);
        let msg = match &outcome {
            RotTickOutcome::Gained(0) => Some("\x1b[33mThe Rot settles a little deeper into you.\x1b[0m".to_string()),
            RotTickOutcome::Gained(dmg) => Some(format!(
                "\x1b[1;33mSores weep where the Rot touches you. \x1b[31m[-{} hp]\x1b[0m",
                dmg
            )),
            RotTickOutcome::RotEaterFed => {
                Some("\x1b[32mThe Rot pools sweet on your tongue. [+1 MP]\x1b[0m".to_string())
            }
            RotTickOutcome::Decayed => Some("\x1b[36mClean air. The Rot loosens its grip a little.\x1b[0m".to_string()),
            RotTickOutcome::DecayedPermanent => Some(
                "\x1b[1;31mThe Rot recedes — but something of it stays, scarred into your flesh forever.\x1b[0m"
                    .to_string(),
            ),
            RotTickOutcome::ClockStarted | RotTickOutcome::Nothing => None,
        };
        if let Some(m) = msg {
            let _ = session.sender.send(format!("\n{}\n", m));
        }
        if !matches!(outcome, RotTickOutcome::Nothing) {
            let _ = db.save_character_data(ch.clone());
        }
    }
    Ok(())
}

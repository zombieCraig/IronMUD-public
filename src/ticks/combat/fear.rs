//! Fear-aura application and feared-player action rolls for the combat tick.
//!
//! A combatant (player or mobile) holding an `EffectType::FearAura` buff —
//! typically stamped by an equipped weapon or armor with a `fear_aura`
//! affect — rolls terror against each of their combat opponents every
//! round. Application routes through the lib-side chokepoint
//! (`ironmud::script::fear`) so immunities and StatusResistance hold; the
//! wearer is never affected by their own aura (behavior code only ever
//! checks `Feared`).
//!
//! Feared players roll `FearAction` each round in
//! `process_character_combat_round`: forced flee (~40%), freeze losing the
//! round (~30%), or fighting on shakily (~30%).

use anyhow::Result;
use rand::Rng;

use ironmud::script::fear::{FearOutcome, is_feared, try_apply_fear_to_character, try_apply_fear_to_mobile};
use ironmud::{ActiveBuff, CombatTargetType, EffectType, SharedConnections, db};

use crate::ticks::broadcast::{broadcast_to_room_awake, send_message_to_character};

/// Per-round application chance when the aura's magnitude is unset (0).
const FEAR_AURA_DEFAULT_CHANCE: i32 = 15;
/// Duration of aura-applied terror — short, but refreshed while fighting.
const FEAR_AURA_FEAR_DURATION_SECS: i32 = 15;

/// What a feared player does with their combat round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FearAction {
    Flee,
    Freeze,
    Act,
}

/// 40% forced flee / 30% freeze / 30% act.
pub(super) fn roll_player_fear_action<R: Rng + ?Sized>(rng: &mut R) -> FearAction {
    match rng.gen_range(0..100) {
        0..=39 => FearAction::Flee,
        40..=69 => FearAction::Freeze,
        _ => FearAction::Act,
    }
}

/// Per-round application chance of the strongest `FearAura` buff held, or
/// `None` when no aura is present. Magnitude is the chance in percent;
/// unset (<= 0) falls back to `FEAR_AURA_DEFAULT_CHANCE`.
fn aura_chance(buffs: &[ActiveBuff]) -> Option<i32> {
    let mag = buffs
        .iter()
        .filter(|b| b.effect_type == EffectType::FearAura)
        .map(|b| b.magnitude)
        .max()?;
    Some(if mag > 0 { mag } else { FEAR_AURA_DEFAULT_CHANCE })
}

/// Roll fear auras for every combatant before the round's attacks. Runs at
/// the top of `process_combat_round` so terror applied here reshapes the
/// victim's action in the same round.
pub(super) fn process_fear_auras(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    let mut rng = rand::thread_rng();

    for char_name in db.get_all_characters_in_combat()? {
        let Some(char) = db.get_character_data(&char_name)? else {
            continue;
        };
        if !char.combat.in_combat || char.is_unconscious {
            continue;
        }
        let Some(chance) = aura_chance(&char.active_buffs) else {
            continue;
        };
        let room_id = char.current_room_id;
        let source = format!("{}'s dreadful presence", char.name);
        for target in char.combat.targets.clone() {
            match target.target_type {
                CombatTargetType::Mobile => {
                    frighten_mobile_opponent(
                        db,
                        connections,
                        &room_id,
                        &char.name,
                        &target.target_id,
                        chance,
                        &source,
                        &mut rng,
                    )?;
                }
                CombatTargetType::Player => {
                    if let Some(victim) = target.target_name.clone() {
                        frighten_player_opponent(
                            db,
                            connections,
                            &room_id,
                            &char.name,
                            &victim,
                            chance,
                            &source,
                            &mut rng,
                        )?;
                    }
                }
            }
        }
    }

    for mobile_id in db.get_all_mobiles_in_combat()? {
        let Some(mobile) = db.get_mobile_data(&mobile_id)? else {
            continue;
        };
        if !mobile.combat.in_combat || mobile.current_hp <= 0 {
            continue;
        }
        let Some(chance) = aura_chance(&mobile.active_buffs) else {
            continue;
        };
        let Some(room_id) = mobile.current_room_id else {
            continue;
        };
        let source = format!("{}'s dreadful presence", mobile.name);
        for target in mobile.combat.targets.clone() {
            match target.target_type {
                CombatTargetType::Mobile => {
                    frighten_mobile_opponent(
                        db,
                        connections,
                        &room_id,
                        &mobile.name,
                        &target.target_id,
                        chance,
                        &source,
                        &mut rng,
                    )?;
                }
                CombatTargetType::Player => {
                    // Mobile player-targets carry a name in PvP-era records;
                    // legacy records fall back to whoever is in the room.
                    let victim = target
                        .target_name
                        .clone()
                        .or_else(|| crate::ticks::mobile::find_player_name_in_room(connections, &room_id));
                    if let Some(victim) = victim {
                        frighten_player_opponent(
                            db,
                            connections,
                            &room_id,
                            &mobile.name,
                            &victim,
                            chance,
                            &source,
                            &mut rng,
                        )?;
                    }
                }
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn frighten_mobile_opponent<R: Rng + ?Sized>(
    db: &db::Db,
    connections: &SharedConnections,
    room_id: &uuid::Uuid,
    wearer_name: &str,
    mobile_id: &uuid::Uuid,
    chance: i32,
    source: &str,
    rng: &mut R,
) -> Result<()> {
    let Some(mob) = db.get_mobile_data(mobile_id)? else {
        return Ok(());
    };
    // Already terrified — sustain quietly rather than re-broadcasting each round.
    if is_feared(&mob.active_buffs) {
        let _ = try_apply_fear_to_mobile(db, mobile_id, chance, 0, FEAR_AURA_FEAR_DURATION_SECS, source, rng);
        return Ok(());
    }
    if try_apply_fear_to_mobile(db, mobile_id, chance, 0, FEAR_AURA_FEAR_DURATION_SECS, source, rng)
        == FearOutcome::Applied
    {
        broadcast_to_room_awake(
            connections,
            room_id,
            &format!("{}'s dreadful presence strikes terror into {}!", wearer_name, mob.name),
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn frighten_player_opponent<R: Rng + ?Sized>(
    db: &db::Db,
    connections: &SharedConnections,
    room_id: &uuid::Uuid,
    wearer_name: &str,
    victim_name: &str,
    chance: i32,
    source: &str,
    rng: &mut R,
) -> Result<()> {
    let Some(victim) = db.get_character_data(victim_name)? else {
        return Ok(());
    };
    if victim.god_mode {
        return Ok(());
    }
    // Already terrified — sustain quietly rather than re-broadcasting each round.
    if is_feared(&victim.active_buffs) {
        let _ = try_apply_fear_to_character(
            db,
            connections,
            victim_name,
            chance,
            0,
            FEAR_AURA_FEAR_DURATION_SECS,
            source,
            rng,
        );
        return Ok(());
    }
    if try_apply_fear_to_character(
        db,
        connections,
        victim_name,
        chance,
        0,
        FEAR_AURA_FEAR_DURATION_SECS,
        source,
        rng,
    ) == FearOutcome::Applied
    {
        send_message_to_character(
            connections,
            victim_name,
            &format!(
                "\x1b[1;31m{}'s dreadful presence floods you with terror!\x1b[0m",
                wearer_name
            ),
        );
        broadcast_to_room_awake(
            connections,
            room_id,
            &format!(
                "{}'s dreadful presence strikes terror into {}!",
                wearer_name, victim.name
            ),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ironmud::MobileData;
    use uuid::Uuid;

    fn buff(effect_type: EffectType, magnitude: i32) -> ActiveBuff {
        ActiveBuff {
            effect_type,
            magnitude,
            remaining_secs: -1,
            source: "item:test".to_string(),
            damage_type: None,
            vs_effect: None,
            skill_key: None,
        }
    }

    #[test]
    fn aura_chance_reads_magnitude_with_default() {
        assert_eq!(aura_chance(&[]), None);
        assert_eq!(
            aura_chance(&[buff(EffectType::FearAura, 0)]),
            Some(FEAR_AURA_DEFAULT_CHANCE)
        );
        assert_eq!(aura_chance(&[buff(EffectType::FearAura, 40)]), Some(40));
        assert_eq!(
            aura_chance(&[buff(EffectType::FearAura, 10), buff(EffectType::FearAura, 25)]),
            Some(25)
        );
        // Feared on the bearer is not an aura.
        assert_eq!(aura_chance(&[buff(EffectType::Feared, 0)]), None);
    }

    #[test]
    fn aura_wearer_is_not_feared_by_own_aura() {
        // The behavior predicate ignores FearAura entirely.
        assert!(!is_feared(&[buff(EffectType::FearAura, 25)]));
    }

    #[test]
    fn fear_action_roll_covers_all_outcomes() {
        let mut rng = rand::thread_rng();
        let mut saw = [false; 3];
        for _ in 0..2000 {
            match roll_player_fear_action(&mut rng) {
                FearAction::Flee => saw[0] = true,
                FearAction::Freeze => saw[1] = true,
                FearAction::Act => saw[2] = true,
            }
        }
        assert!(saw.iter().all(|s| *s), "all three actions should occur over 2000 rolls");
    }

    #[test]
    fn aura_pass_skips_immune_opponents() {
        let temp = tempfile::tempdir().expect("tempdir");
        let db = db::Db::open(temp.path()).expect("open db");
        let room = Uuid::new_v4();

        let mut golem = MobileData::new("iron golem".to_string());
        golem.is_prototype = false;
        golem.current_room_id = Some(room);
        golem.flags.no_fear = true;
        let golem_id = golem.id;
        db.save_mobile_data(golem).expect("save");

        let mut rng = rand::rngs::mock::StepRng::new(0, 0); // always lands the roll
        let outcome = try_apply_fear_to_mobile(&db, &golem_id, 100, 0, 15, "test aura", &mut rng);
        assert_eq!(outcome, FearOutcome::Immune);
        let golem = db.get_mobile_data(&golem_id).expect("get").expect("exists");
        assert!(!is_feared(&golem.active_buffs));
    }
}

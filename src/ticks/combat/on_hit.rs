//! On-hit effect dispatcher.
//!
//! Rolls each `OnHitEffect` from the attacker's weapon (PC) or natural attack
//! profile (mob) against d100, then applies it to the target. Three branches:
//! - `bleeding`     -> wound bleeding via the `Woundable` trait
//! - elemental      -> push/extend an `OngoingEffect` (fire/poison/cold/acid/lightning)
//! - status (else)  -> upsert an `ActiveBuff`, gated by mob immunity flags
//!
//! Pure mutation: the caller saves the entity. Returns flavour lines for
//! the room broadcaster.

use ironmud::{ActiveBuff, CharacterData, EffectType, MobileData, MobileFlags, OnHitEffect, OngoingEffect};
use rand::Rng;

use super::wounds::add_wound_bleeding;

pub struct OnHitOutcome {
    pub room_messages: Vec<String>,
}

pub fn apply_on_hit_effects_to_mobile(
    on_hit: &[OnHitEffect],
    mobile: &mut MobileData,
    attacker_name: &str,
    body_part: &str,
) -> OnHitOutcome {
    let mut rng = rand::thread_rng();
    let mut room_messages = Vec::new();
    for eff in on_hit {
        if !roll_chance(&mut rng, eff.chance) {
            continue;
        }
        match eff.effect.to_lowercase().as_str() {
            "bleeding" => {
                if !mobile.bleeds() {
                    continue;
                }
                add_wound_bleeding(mobile, body_part, eff.magnitude.max(1));
                room_messages.push(format!("{}'s {} bleeds heavily!", mobile.name, body_part));
            }
            kind @ ("fire" | "poison" | "cold" | "acid" | "lightning") => {
                push_or_extend_ongoing(
                    &mut mobile.ongoing_effects,
                    kind,
                    eff.magnitude,
                    eff.duration,
                    body_part,
                );
                room_messages.push(elemental_msg(kind, &mobile.name));
            }
            other => {
                if let Some(et) = EffectType::from_str(other) {
                    if mob_immune(&mobile.flags, et) {
                        continue;
                    }
                    // Fear immunity needs more than flags (creature_type,
                    // Courage/Frenzy buffs) — use the chokepoint's predicate.
                    if et == EffectType::Feared && ironmud::script::fear::mobile_fear_immunity(mobile).is_some() {
                        continue;
                    }
                    upsert_buff(&mut mobile.active_buffs, et, eff.magnitude, eff.duration, attacker_name);
                    if let Some(msg) = buff_msg(et, &mobile.name) {
                        room_messages.push(msg);
                    }
                }
            }
        }
    }
    OnHitOutcome { room_messages }
}

pub fn apply_on_hit_effects_to_character(
    on_hit: &[OnHitEffect],
    character: &mut CharacterData,
    attacker_name: &str,
    body_part: &str,
) -> OnHitOutcome {
    let mut rng = rand::thread_rng();
    let mut room_messages = Vec::new();
    for eff in on_hit {
        if !roll_chance(&mut rng, eff.chance) {
            continue;
        }
        match eff.effect.to_lowercase().as_str() {
            "bleeding" => {
                add_wound_bleeding(character, body_part, eff.magnitude.max(1));
                room_messages.push(format!("{}'s {} bleeds heavily!", character.name, body_part));
            }
            kind @ ("fire" | "poison" | "cold" | "acid" | "lightning") => {
                push_or_extend_ongoing(
                    &mut character.ongoing_effects,
                    kind,
                    eff.magnitude,
                    eff.duration,
                    body_part,
                );
                room_messages.push(elemental_msg(kind, &character.name));
            }
            other => {
                if let Some(et) = EffectType::from_str(other) {
                    // Synths, Courage, and Frenzy holders shrug off fear.
                    if et == EffectType::Feared && ironmud::script::fear::character_fear_immunity(character).is_some() {
                        continue;
                    }
                    upsert_buff(
                        &mut character.active_buffs,
                        et,
                        eff.magnitude,
                        eff.duration,
                        attacker_name,
                    );
                    if let Some(msg) = buff_msg(et, &character.name) {
                        room_messages.push(msg);
                    }
                }
            }
        }
    }
    OnHitOutcome { room_messages }
}

fn roll_chance(rng: &mut impl Rng, chance: i32) -> bool {
    let c = chance.clamp(0, 100);
    if c == 0 {
        return false;
    }
    rng.gen_range(1..=100) <= c
}

fn push_or_extend_ongoing(
    effects: &mut Vec<OngoingEffect>,
    kind: &str,
    magnitude: i32,
    duration: i32,
    body_part: &str,
) {
    let dpr = magnitude.max(1);
    let rounds = duration.max(1);
    if let Some(existing) = effects
        .iter_mut()
        .find(|e| e.effect_type == kind && e.body_part == body_part)
    {
        existing.damage_per_round = existing.damage_per_round.max(dpr);
        existing.rounds_remaining = existing.rounds_remaining.max(rounds);
    } else {
        effects.push(OngoingEffect {
            effect_type: kind.to_string(),
            rounds_remaining: rounds,
            damage_per_round: dpr,
            body_part: body_part.to_string(),
        });
    }
}

fn upsert_buff(buffs: &mut Vec<ActiveBuff>, effect_type: EffectType, magnitude: i32, duration: i32, source: &str) {
    let dur = duration.max(0);
    if let Some(existing) = buffs.iter_mut().find(|b| b.effect_type == effect_type) {
        existing.magnitude = existing.magnitude.max(magnitude);
        existing.remaining_secs = dur;
        existing.source = source.to_string();
    } else {
        buffs.push(ActiveBuff {
            effect_type,
            magnitude,
            remaining_secs: dur,
            source: source.to_string(),
            damage_type: None,
            vs_effect: None,
            skill_key: None,
        });
    }
}

fn mob_immune(flags: &MobileFlags, effect: EffectType) -> bool {
    match effect {
        EffectType::Sleep => flags.no_sleep,
        EffectType::Blind => flags.no_blind,
        EffectType::Charmed => flags.no_charm,
        _ => false,
    }
}

fn elemental_msg(kind: &str, name: &str) -> String {
    match kind {
        "fire" => format!("{} bursts into flame!", name),
        "poison" => format!("Poison courses through {}.", name),
        "cold" => format!("Frost crawls across {}!", name),
        "acid" => format!("Acid sizzles on {}!", name),
        "lightning" => format!("Lightning crackles over {}!", name),
        _ => format!("{} suffers ongoing damage.", name),
    }
}

fn buff_msg(effect: EffectType, name: &str) -> Option<String> {
    Some(match effect {
        EffectType::Sleep => format!("{} collapses, struck by magical sleep!", name),
        EffectType::Blind => format!("{} reels, blinded!", name),
        EffectType::Slow => format!("{} slows, movements suddenly sluggish.", name),
        EffectType::Curse => format!("A creeping curse shadows {}.", name),
        EffectType::Feared => format!("Stark terror floods {}!", name),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mob() -> MobileData {
        MobileData::new("a goblin".to_string())
    }

    fn one(effect: &str, magnitude: i32, duration: i32) -> Vec<OnHitEffect> {
        vec![OnHitEffect {
            effect: effect.to_string(),
            chance: 100,
            magnitude,
            duration,
        }]
    }

    #[test]
    fn bleeding_at_100_chance_adds_wound_severity() {
        let mut m = mob();
        let _ = apply_on_hit_effects_to_mobile(&one("bleeding", 3, 0), &mut m, "alice", "torso");
        let total: i32 = m.wounds.iter().map(|w| w.bleeding_severity).sum();
        assert_eq!(total, 3, "bleeding magnitude becomes wound bleeding severity");
    }

    #[test]
    fn fire_at_100_chance_pushes_ongoing_effect() {
        let mut m = mob();
        let _ = apply_on_hit_effects_to_mobile(&one("fire", 2, 4), &mut m, "alice", "torso");
        assert_eq!(m.ongoing_effects.len(), 1);
        let e = &m.ongoing_effects[0];
        assert_eq!(e.effect_type, "fire");
        assert_eq!(e.damage_per_round, 2);
        assert_eq!(e.rounds_remaining, 4);
        assert_eq!(e.body_part, "torso");
    }

    #[test]
    fn fire_extends_existing_same_kind_same_part() {
        let mut m = mob();
        let _ = apply_on_hit_effects_to_mobile(&one("fire", 2, 3), &mut m, "alice", "torso");
        let _ = apply_on_hit_effects_to_mobile(&one("fire", 4, 1), &mut m, "alice", "torso");
        assert_eq!(m.ongoing_effects.len(), 1, "same kind+part merges");
        assert_eq!(m.ongoing_effects[0].damage_per_round, 4, "max(2, 4)");
        assert_eq!(m.ongoing_effects[0].rounds_remaining, 3, "max(3, 1)");
    }

    #[test]
    fn no_sleep_flag_blocks_sleep_buff() {
        let mut m = mob();
        m.flags.no_sleep = true;
        let _ = apply_on_hit_effects_to_mobile(&one("sleep", 0, 60), &mut m, "alice", "torso");
        assert!(
            m.active_buffs.iter().all(|b| b.effect_type != EffectType::Sleep),
            "sleep gated by no_sleep"
        );
    }

    #[test]
    fn no_blind_flag_blocks_blind_buff() {
        let mut m = mob();
        m.flags.no_blind = true;
        let _ = apply_on_hit_effects_to_mobile(&one("blind", 0, 60), &mut m, "alice", "torso");
        assert!(m.active_buffs.iter().all(|b| b.effect_type != EffectType::Blind));
    }

    #[test]
    fn unrecognized_effect_is_ignored() {
        let mut m = mob();
        let _ = apply_on_hit_effects_to_mobile(&one("totally_made_up", 9, 9), &mut m, "alice", "torso");
        assert!(m.wounds.is_empty());
        assert!(m.ongoing_effects.is_empty());
        assert!(m.active_buffs.is_empty());
    }

    #[test]
    fn zero_chance_never_fires() {
        let mut m = mob();
        let on_hit = vec![OnHitEffect {
            effect: "bleeding".to_string(),
            chance: 0,
            magnitude: 5,
            duration: 0,
        }];
        for _ in 0..50 {
            let _ = apply_on_hit_effects_to_mobile(&on_hit, &mut m, "alice", "torso");
        }
        assert!(m.wounds.is_empty(), "0 chance must not roll true");
    }
}

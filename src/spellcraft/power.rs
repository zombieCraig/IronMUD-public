//! How strong a spell is, as one number and a 1-5 tier.
//!
//! The score measures *effect* only: damage, healing, or how much a buff or
//! debuff matters and for how long, times how many it can reach. Mana and
//! cooldown are deliberately left out: for player spells they are derived
//! from power (S4), so counting them here would be circular, and a cheap
//! spell is not a weak one.
//!
//! Everything is evaluated at a fixed reference caster (magic 10, +4 INT
//! bonus) and at spell mastery level 1, i.e. the spell as first learned.
//! Drafted player spells carry no per-level scaling, so this is the fair
//! comparison point against the built-ins.
//!
//! Tier boundaries are hand-set against the shipped fantasy spells and
//! pinned by the calibration test below: every built-in that needs magic
//! 5 or less sits at tier 3 or under (the strongest of them exactly at 3),
//! and everything needing magic 6+ is tier 4 or higher. So the canon floor
//! (`spellcraft.min_power_tier`, default 3) means "at least as strong as the
//! best mid-level spell".

use crate::types::{EffectType, SpellDefinition};

/// Spell types a player may author. Everything else (utility, summon,
/// charm, ...) has bespoke cast handlers and stays builder-only.
pub const ALLOWED_PLAYER_TYPES: &[&str] = &["damage", "debuff", "buff", "heal"];

/// Reference caster the score is computed for.
const REF_MAGIC: i64 = 10;
const REF_INT_BONUS: i64 = 4;
const REF_SPELL_LEVEL: i64 = 1;

/// Lower bound of tiers 2, 3, 4 and 5.
pub const TIER_BOUNDS: [u32; 4] = [50, 80, 110, 140];

/// Healing is worth a bit less per point than damage (it can't end a fight).
const HEAL_WEIGHT_PCT: i64 = 75;

/// How many targets the spell reaches, as a percent multiplier.
fn breadth_pct(target_type: &str) -> i64 {
    match target_type {
        "room" => 250,
        "self_or_friendly" => 125,
        _ => 100,
    }
}

/// `(reference magnitude, weight)` for a buff/debuff effect. The weight is
/// the score of that effect at its reference magnitude for 120 seconds. A
/// reference magnitude of 0 marks an on/off effect whose magnitude is ignored.
fn effect_value(effect: EffectType) -> (i64, i64) {
    use EffectType::*;
    match effect {
        Haste => (2, 90),
        Slow => (1, 60),
        Sleep => (0, 100),
        Stun => (0, 110),
        Blind => (50, 70),
        Silence => (0, 70),
        Feared => (0, 60),
        Charmed | Dominated => (0, 120),
        DamageReduction => (50, 120),
        DamageResistance => (25, 70),
        StatusResistance => (25, 50),
        ArmorClassBoost => (5, 50),
        Invisibility | Obfuscate => (0, 60),
        DetectInvisible | DetectMagic => (0, 15),
        NightVision | MagicLight | WaterBreathing => (0, 10),
        Courage => (0, 35),
        Luck => (5, 30),
        Bless | Curse => (5, 45),
        StrengthBoost | DexterityBoost | ConstitutionBoost | IntelligenceBoost | WisdomBoost | CharismaBoost => (4, 40),
        HitBonus => (5, 50),
        DamageBonus => (5, 60),
        Regeneration => (5, 60),
        Poison => (5, 60),
        MaxHpBonus => (50, 60),
        MaxManaBonus => (50, 40),
        _ => (5, 40),
    }
}

/// The magnitude an effect is weighed at; 0 for on/off effects.
pub fn reference_magnitude(effect: EffectType) -> i64 {
    effect_value(effect).0
}

fn effect_score(def: &SpellDefinition) -> f64 {
    let effect = EffectType::from_str(&def.buff_effect).unwrap_or_default();
    let (ref_mag, weight) = effect_value(effect);
    let magnitude = (def.buff_magnitude as i64 + def.buff_magnitude_per_spell_level as i64 * REF_SPELL_LEVEL).abs();
    let mag_factor = if ref_mag == 0 {
        1.0
    } else {
        (magnitude as f64 / ref_mag as f64).min(3.0)
    };
    let duration = (def.buff_duration_secs as i64 + def.buff_duration_per_spell_level as i64 * REF_SPELL_LEVEL).max(0);
    // Diminishing returns on duration, capped at 4× the reference 120s.
    let dur_factor = (duration as f64 / 120.0).sqrt().min(2.0);
    weight as f64 * mag_factor * dur_factor
}

/// The spell's power at the reference caster. 0 for types this scale does
/// not cover.
pub fn power_score(def: &SpellDefinition) -> u32 {
    let raw = match def.spell_type.as_str() {
        "damage" => {
            (def.damage_base as i64
                + def.damage_per_skill as i64 * REF_MAGIC
                + def.damage_int_scaling as i64 * REF_INT_BONUS
                + def.damage_per_spell_level as i64 * REF_SPELL_LEVEL) as f64
        }
        "heal" => {
            let amount = def.heal_base as i64
                + def.heal_per_skill as i64 * REF_MAGIC
                + def.heal_int_scaling as i64 * REF_INT_BONUS
                + def.heal_per_spell_level as i64 * REF_SPELL_LEVEL;
            (amount * HEAL_WEIGHT_PCT) as f64 / 100.0
        }
        "buff" | "debuff" => effect_score(def),
        _ => return 0,
    };
    let scored = raw * breadth_pct(&def.target_type) as f64 / 100.0;
    scored.max(0.0).round() as u32
}

/// 1-5 tier for a score.
pub fn tier(score: u32) -> u8 {
    1 + TIER_BOUNDS.iter().filter(|&&b| score >= b).count() as u8
}

/// Tier of a spell, or 0 for a type outside the scale.
pub fn spell_tier(def: &SpellDefinition) -> u8 {
    if !ALLOWED_PLAYER_TYPES.contains(&def.spell_type.as_str()) {
        return 0;
    }
    tier(power_score(def))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn fantasy() -> HashMap<String, SpellDefinition> {
        let text = std::fs::read_to_string("scripts/data/spells_fantasy.json").expect("read fantasy spells");
        crate::init::data::parse_doc_tolerant_map::<SpellDefinition>(&text).expect("parse fantasy spells")
    }

    fn def(json: serde_json::Value) -> SpellDefinition {
        serde_json::from_value(json).expect("build def")
    }

    #[test]
    fn tiers_split_on_the_bounds() {
        assert_eq!(tier(0), 1);
        assert_eq!(tier(49), 1);
        assert_eq!(tier(50), 2);
        assert_eq!(tier(80), 3);
        assert_eq!(tier(139), 4);
        assert_eq!(tier(140), 5);
        assert_eq!(tier(10_000), 5);
    }

    /// The calibration the canon floor depends on. If spell data changes and
    /// this fails, re-pick `TIER_BOUNDS` so it holds again.
    #[test]
    fn calibration_against_the_fantasy_built_ins() {
        let spells = fantasy();
        let scored: Vec<_> = spells
            .values()
            .filter(|s| ALLOWED_PLAYER_TYPES.contains(&s.spell_type.as_str()))
            .map(|s| (s.id.clone(), s.skill_required, spell_tier(s), power_score(s)))
            .collect();
        assert!(scored.len() >= 10, "too few spells to calibrate: {scored:?}");

        let mid: Vec<_> = scored.iter().filter(|s| s.1 <= 5).collect();
        let high: Vec<_> = scored.iter().filter(|s| s.1 >= 6).collect();
        assert!(!mid.is_empty() && !high.is_empty());
        for s in &mid {
            assert!(s.2 <= 3, "mid-level spell above tier 3: {s:?}");
        }
        assert_eq!(
            mid.iter().map(|s| s.2).max(),
            Some(3),
            "the strongest mid-level spell should sit at tier 3: {mid:?}"
        );
        for s in &high {
            assert!(s.2 >= 4, "high-level spell below tier 4: {s:?}");
        }
    }

    #[test]
    fn lightning_bolt_is_the_canon_floor_reference() {
        // §2a: tier 3 = at least as strong as the strongest mid-level built-in.
        let spells = fantasy();
        assert_eq!(spell_tier(&spells["lightning_bolt"]), 3);
        assert!(spell_tier(&spells["magic_missile"]) < 3);
    }

    #[test]
    fn score_ignores_mana_and_cooldown() {
        let cheap = def(serde_json::json!({
            "id": "a", "name": "A", "description": "", "spell_type": "damage",
            "damage_base": 50, "mana_cost": 5, "cooldown_secs": 1, "target_type": "enemy"
        }));
        let dear = def(serde_json::json!({
            "id": "b", "name": "B", "description": "", "spell_type": "damage",
            "damage_base": 50, "mana_cost": 90, "cooldown_secs": 60, "target_type": "enemy"
        }));
        assert_eq!(power_score(&cheap), power_score(&dear));
    }

    #[test]
    fn breadth_and_duration_raise_the_score() {
        let one = def(serde_json::json!({
            "id": "a", "name": "A", "description": "", "spell_type": "damage",
            "damage_base": 60, "target_type": "enemy"
        }));
        let room = def(serde_json::json!({
            "id": "b", "name": "B", "description": "", "spell_type": "damage",
            "damage_base": 60, "target_type": "room"
        }));
        assert!(power_score(&room) > power_score(&one));

        let short = def(serde_json::json!({
            "id": "c", "name": "C", "description": "", "spell_type": "buff",
            "buff_effect": "haste", "buff_magnitude": 2, "buff_duration_secs": 60, "target_type": "self"
        }));
        let long = def(serde_json::json!({
            "id": "d", "name": "D", "description": "", "spell_type": "buff",
            "buff_effect": "haste", "buff_magnitude": 2, "buff_duration_secs": 240, "target_type": "self"
        }));
        assert!(power_score(&long) > power_score(&short));
    }

    #[test]
    fn types_outside_the_scale_have_no_tier() {
        let spells = fantasy();
        assert_eq!(spell_tier(&spells["summon"]), 0);
        assert_eq!(spell_tier(&spells["light"]), 0);
    }
}

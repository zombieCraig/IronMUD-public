//! Designing a player spell: what a god allows, and what a draft becomes.
//!
//! Everything here is pure. [`designs`] enumerates every spell the god would
//! accept (in its domain, at or above the canon floor, at or below the power
//! ceiling); the petition menus offer only values that still lead to one of
//! them, so a player can never draft themselves into a refusal. Mana and
//! cooldown are derived from power, never chosen.

use crate::types::{
    ComponentReq, DeitySpellcraft, EffectType, Price, PriceStyle, SpellDefinition, SpellDraft, TrialRule,
};

use super::Settings;
use super::power::{power_score, reference_magnitude, tier};

/// Effects a player buff may carry. Limited to what the generic buff cast
/// path (`apply_buff`) handles with no bespoke code.
pub const BUFF_EFFECTS: &[&str] = &[
    "haste",
    "strength_boost",
    "dexterity_boost",
    "constitution_boost",
    "intelligence_boost",
    "wisdom_boost",
    "charisma_boost",
    "armor_class_boost",
    "damage_reduction",
    "damage_resistance",
    "status_resistance",
    "regeneration",
    "hit_bonus",
    "damage_bonus",
    "max_hp_bonus",
    "max_mana_bonus",
    "luck",
    "bless",
    "courage",
    "invisibility",
    "detect_invisible",
    "detect_magic",
    "night_vision",
    "water_breathing",
];

/// Effects a player debuff may carry (`apply_buff_to_mobile`). Fear and charm
/// have their own spell types and chokepoints, so they are not here.
pub const DEBUFF_EFFECTS: &[&str] = &["slow", "sleep", "blind", "silence", "curse", "poison"];

/// Damage and heal amounts offered.
const AMOUNTS: std::ops::RangeInclusive<i32> = 40..=240;
const AMOUNT_STEP: usize = 10;
/// Buff/debuff durations offered, in seconds.
pub const DURATIONS: &[i32] = &[60, 120, 240, 480];
/// Magic level a drafted spell needs to cast.
pub const SKILL_REQUIRED: i32 = 10;

/// What one god will grant, with the server's floor and ceiling applied.
#[derive(Debug, Clone)]
pub struct Domain {
    pub types: Vec<String>,
    pub elements: Vec<String>,
    pub effects: Vec<String>,
    pub targets: Vec<String>,
    /// Lowest tier accepted.
    pub floor: u8,
    pub max_power: u32,
}

impl Domain {
    pub fn new(cfg: &DeitySpellcraft, settings: &Settings) -> Self {
        let floor = cfg.min_power_tier.unwrap_or(settings.min_power_tier).clamp(1, 5) as u8;
        Domain {
            types: cfg.allowed_types.clone(),
            elements: cfg.allowed_elements.clone(),
            effects: cfg.allowed_effects.clone(),
            targets: cfg.allowed_targets.clone(),
            floor,
            max_power: settings.max_power.max(0) as u32,
        }
    }
}

/// One complete mechanical design (everything but the words).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Design {
    pub spell_type: String,
    pub element: Option<String>,
    pub effect: Option<String>,
    pub target: String,
    pub magnitude: i32,
    pub duration: Option<i32>,
    pub power: u32,
}

fn targets_for(spell_type: &str) -> &'static [&'static str] {
    match spell_type {
        "damage" | "debuff" => &["enemy"],
        _ => &["self", "self_or_friendly"],
    }
}

fn magnitudes_for(effect: &str) -> Vec<i32> {
    let r = EffectType::from_str(effect).map(reference_magnitude).unwrap_or(0) as i32;
    if r == 0 { vec![1] } else { vec![r, r * 2, r * 3] }
}

/// The mechanical spell a design describes. Words and id are placeholders.
fn mechanics(d: &Design) -> SpellDefinition {
    let mut def: SpellDefinition =
        serde_json::from_value(serde_json::json!({"id": "", "name": "", "description": ""})).expect("bare spell");
    def.spell_type = d.spell_type.clone();
    def.target_type = d.target.clone();
    match d.spell_type.as_str() {
        "damage" => {
            def.damage_base = d.magnitude;
            def.damage_type = d.element.clone().unwrap_or_default();
        }
        "heal" => def.heal_base = d.magnitude,
        _ => {
            def.buff_effect = d.effect.clone().unwrap_or_default();
            def.buff_magnitude = d.magnitude;
            def.buff_duration_secs = d.duration.unwrap_or(0);
        }
    }
    def
}

fn accepted(power: u32, domain: &Domain) -> bool {
    tier(power) >= domain.floor && power <= domain.max_power
}

/// Every design the god accepts.
pub fn designs(domain: &Domain) -> Vec<Design> {
    let mut out = Vec::new();
    let mut push = |mut d: Design| {
        d.power = power_score(&mechanics(&d));
        if accepted(d.power, domain) {
            out.push(d);
        }
    };
    for t in &domain.types {
        let targets: Vec<&str> = targets_for(t)
            .iter()
            .copied()
            .filter(|x| domain.targets.iter().any(|a| a == x))
            .collect();
        match t.as_str() {
            "damage" => {
                for e in &domain.elements {
                    for target in &targets {
                        for m in AMOUNTS.step_by(AMOUNT_STEP) {
                            push(Design {
                                spell_type: t.clone(),
                                element: Some(e.clone()),
                                effect: None,
                                target: target.to_string(),
                                magnitude: m,
                                duration: None,
                                power: 0,
                            });
                        }
                    }
                }
            }
            "heal" => {
                for target in &targets {
                    for m in AMOUNTS.step_by(AMOUNT_STEP) {
                        push(Design {
                            spell_type: t.clone(),
                            element: None,
                            effect: None,
                            target: target.to_string(),
                            magnitude: m,
                            duration: None,
                            power: 0,
                        });
                    }
                }
            }
            "buff" | "debuff" => {
                let pool = if t == "buff" { BUFF_EFFECTS } else { DEBUFF_EFFECTS };
                for e in domain.effects.iter().filter(|e| pool.contains(&e.as_str())) {
                    for target in &targets {
                        for m in magnitudes_for(e) {
                            for dur in DURATIONS {
                                push(Design {
                                    spell_type: t.clone(),
                                    element: None,
                                    effect: Some(e.clone()),
                                    target: target.to_string(),
                                    magnitude: m,
                                    duration: Some(*dur),
                                    power: 0,
                                });
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Does `d` agree with every choice the draft has made so far?
fn consistent(d: &Design, draft: &SpellDraft) -> bool {
    let same = |chosen: &Option<String>, have: Option<&str>| chosen.as_deref().is_none_or(|c| Some(c) == have);
    same(&draft.spell_type, Some(&d.spell_type))
        && same(&draft.element, d.element.as_deref())
        && same(&draft.effect, d.effect.as_deref())
        && same(&draft.target, Some(&d.target))
        && draft.magnitude.is_none_or(|m| m == d.magnitude)
        && draft.duration.is_none_or(|x| Some(x) == d.duration)
}

/// The designs still open to `draft`.
pub fn open_designs<'a>(all: &'a [Design], draft: &SpellDraft) -> Vec<&'a Design> {
    all.iter().filter(|d| consistent(d, draft)).collect()
}

/// The finished design, if every mechanical choice is made and the god
/// accepts it.
pub fn chosen_design(all: &[Design], draft: &SpellDraft) -> Option<Design> {
    draft.spell_type.as_ref()?;
    draft.target.as_ref()?;
    draft.magnitude?;
    let open = open_designs(all, draft);
    // With every field set, at most one design matches.
    (open.len() == 1).then(|| open[0].clone())
}

/// Mana a drafted spell costs: a little dearer per point of power than the
/// built-ins (Lightning Bolt, power 86, costs 40; a drafted 86 costs 47).
pub fn mana_for(power: u32) -> i32 {
    ((power as i64 * 55 + 50) / 100) as i32
}

/// Cooldown, by type: blasts and heals recur often, buffs rarely.
pub fn cooldown_for(power: u32, spell_type: &str) -> i32 {
    let p = power as i32;
    match spell_type {
        "damage" | "heal" => (p / 5).max(5),
        "debuff" => p / 2 + 30,
        _ => p + 60,
    }
}

/// The complete spell a finished draft describes, under `id`.
pub fn to_definition(design: &Design, draft: &SpellDraft, id: &str) -> SpellDefinition {
    let mut def = mechanics(design);
    def.id = id.to_string();
    def.name = draft.name.clone().unwrap_or_default();
    def.description = draft.description.clone().unwrap_or_default();
    def.mana_cost = mana_for(design.power);
    def.cooldown_secs = cooldown_for(design.power, &design.spell_type);
    def.skill_required = SKILL_REQUIRED;
    def.requires_combat = design.spell_type == "damage" || design.spell_type == "debuff";
    def.scroll_only = true;
    def.codex = true;
    def
}

/// The price of a spell of `power`, as `cfg` charges it.
pub fn price(power: u32, cfg: &DeitySpellcraft, settings: &Settings) -> Price {
    let hp = ((power as i64 * settings.hp_pledge_pct.max(0)) / 100).max(1) as i32;
    let mut p = Price {
        style: cfg.price_style,
        hp,
        favor: cfg.favor_cost,
        ..Price::default()
    };
    match cfg.price_style {
        PriceStyle::Flesh => p.hp += hp / 2,
        PriceStyle::Mind => p.mana = hp,
        PriceStyle::Essence => p.attribute = 1,
        PriceStyle::Fortune => p.gold = power as i64 * 25,
        PriceStyle::Weariness => p.stamina = hp * 2,
        // Revealed only after sealing.
        PriceStyle::Fate => {}
    }
    p
}

/// Components the ritual will consume: one element essence or effect focus
/// per tier from the floor up, better the stronger the spell, and always
/// one relic of the god.
pub fn components_for(design: &Design, god_vnum: &str, floor: u8, settings: &Settings) -> Vec<ComponentReq> {
    let t = tier(design.power);
    let above = t.saturating_sub(floor) as i64;
    let category = match design.spell_type.as_str() {
        "damage" => format!("essence:{}", design.element.clone().unwrap_or_default()),
        "heal" => "focus:heal".to_string(),
        _ => format!("focus:{}", design.effect.clone().unwrap_or_default()),
    };
    let quality = (settings.component_quality_base + settings.component_quality_per_tier * above).clamp(0, 100) as i32;
    vec![
        ComponentReq {
            category,
            qty: above as u32 + 1,
            min_quality: quality,
        },
        ComponentReq {
            category: format!("relic:{god_vnum}"),
            qty: 1,
            min_quality: 0,
        },
    ]
}

fn rule_matches(rule: &TrialRule, design: &Design) -> bool {
    let m = |want: &Option<String>, have: Option<&str>| want.as_deref().is_none_or(|w| Some(w) == have);
    m(&rule.when_type, Some(&design.spell_type))
        && m(&rule.when_element, design.element.as_deref())
        && m(&rule.when_effect, design.effect.as_deref())
        && design.power >= rule.min_power
}

fn specificity(rule: &TrialRule) -> usize {
    [&rule.when_type, &rule.when_element, &rule.when_effect]
        .iter()
        .filter(|w| w.is_some())
        .count()
}

/// The god's trial for this design: the most specific matching rule, then
/// the highest `min_power`, then the first listed.
pub fn pick_trial<'a>(rules: &'a [TrialRule], design: &Design) -> Option<&'a TrialRule> {
    pick(rules, design, false)
}

/// The god's restoration trial for a faded spell (rules marked
/// `restoration`), chosen the same way.
pub fn pick_restoration_trial<'a>(rules: &'a [TrialRule], design: &Design) -> Option<&'a TrialRule> {
    pick(rules, design, true)
}

/// What a stored spell's mechanics amount to, as a design (restoration).
pub fn design_of(def: &crate::types::SpellDefinition) -> Design {
    let (element, effect, magnitude, duration) = match def.spell_type.as_str() {
        "damage" => (Some(def.damage_type.clone()), None, def.damage_base, None),
        "heal" => (None, None, def.heal_base, None),
        _ => (
            None,
            Some(def.buff_effect.clone()),
            def.buff_magnitude,
            Some(def.buff_duration_secs),
        ),
    };
    Design {
        spell_type: def.spell_type.clone(),
        element,
        effect,
        target: def.target_type.clone(),
        magnitude,
        duration,
        power: power_score(def),
    }
}

fn pick<'a>(rules: &'a [TrialRule], design: &Design, restoration: bool) -> Option<&'a TrialRule> {
    let mut best: Option<&TrialRule> = None;
    for r in rules
        .iter()
        .filter(|r| r.restoration == restoration && rule_matches(r, design))
    {
        let better = best.is_none_or(|b| (specificity(r), r.min_power) > (specificity(b), b.min_power));
        if better {
            best = Some(r);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn god() -> DeitySpellcraft {
        DeitySpellcraft {
            allowed_types: vec!["damage".into(), "buff".into()],
            allowed_elements: vec!["fire".into()],
            allowed_effects: vec!["haste".into(), "sleep".into()],
            allowed_targets: vec!["enemy".into(), "self".into()],
            ..DeitySpellcraft::default()
        }
    }

    fn domain() -> Domain {
        Domain::new(&god(), &Settings::default())
    }

    #[test]
    fn designs_stay_inside_the_domain_floor_and_ceiling() {
        let d = domain();
        let all = designs(&d);
        assert!(!all.is_empty());
        for x in &all {
            assert!(["damage", "buff"].contains(&x.spell_type.as_str()), "{x:?}");
            assert!(x.element.as_deref().is_none_or(|e| e == "fire"));
            // sleep is a debuff and debuffs aren't allowed; haste only.
            assert!(x.effect.as_deref().is_none_or(|e| e == "haste"), "{x:?}");
            assert!(["enemy", "self"].contains(&x.target.as_str()));
            assert!(tier(x.power) >= 3 && x.power <= 160, "{x:?}");
        }
        // The buff can't target an enemy, the blast can't target self.
        assert!(all.iter().all(|x| (x.spell_type == "damage") == (x.target == "enemy")));
    }

    #[test]
    fn nothing_below_the_floor_is_offered() {
        let all = designs(&domain());
        let min_damage = all
            .iter()
            .filter(|x| x.spell_type == "damage")
            .map(|x| x.magnitude)
            .min()
            .unwrap();
        assert_eq!(min_damage, 80, "80 damage is the first tier-3 blast");
        let mut weak = SpellDraft {
            spell_type: Some("damage".into()),
            element: Some("fire".into()),
            target: Some("enemy".into()),
            magnitude: Some(70),
            ..SpellDraft::default()
        };
        assert!(chosen_design(&all, &weak).is_none());
        weak.magnitude = Some(90);
        assert_eq!(chosen_design(&all, &weak).map(|d| d.power), Some(90));
    }

    #[test]
    fn a_higher_floor_and_lower_ceiling_narrow_the_menu() {
        let mut c = god();
        c.min_power_tier = Some(5);
        let s = Settings {
            max_power: 150,
            ..Settings::default()
        };
        let all = designs(&Domain::new(&c, &s));
        assert!(all.iter().all(|x| (140..=150).contains(&x.power)), "{all:?}");
    }

    #[test]
    fn an_empty_domain_offers_nothing() {
        assert!(designs(&Domain::new(&DeitySpellcraft::default(), &Settings::default())).is_empty());
    }

    #[test]
    fn definition_derives_mana_and_cooldown() {
        let all = designs(&domain());
        let draft = SpellDraft {
            spell_type: Some("damage".into()),
            element: Some("fire".into()),
            target: Some("enemy".into()),
            magnitude: Some(90),
            name: Some("Sun Lance".into()),
            ..SpellDraft::default()
        };
        let design = chosen_design(&all, &draft).unwrap();
        let def = to_definition(&design, &draft, "cx_sun_lance");
        assert_eq!(power_score(&def), design.power);
        assert_eq!(def.mana_cost, mana_for(90));
        assert!(def.mana_cost > 40, "dearer than Lightning Bolt at similar power");
        assert_eq!(def.cooldown_secs, 18);
        assert!(def.scroll_only && def.codex);
        assert_eq!(def.damage_type, "fire");
    }

    #[test]
    fn price_by_style() {
        let s = Settings::default();
        let mut c = god();
        let flesh = price(100, &c, &s);
        assert_eq!((flesh.hp, flesh.favor), (30, 250));
        c.price_style = PriceStyle::Weariness;
        let w = price(100, &c, &s);
        assert_eq!((w.hp, w.stamina), (20, 40));
        c.price_style = PriceStyle::Fortune;
        assert_eq!(price(100, &c, &s).gold, 2500);
    }

    #[test]
    fn components_scale_with_tier_above_the_floor() {
        let s = Settings::default();
        let mk = |power| Design {
            spell_type: "damage".into(),
            element: Some("fire".into()),
            effect: None,
            target: "enemy".into(),
            magnitude: power as i32,
            duration: None,
            power,
        };
        let at_floor = components_for(&mk(90), "god:sol", 3, &s);
        assert_eq!(
            at_floor[0],
            ComponentReq {
                category: "essence:fire".into(),
                qty: 1,
                min_quality: 40
            }
        );
        assert_eq!(at_floor[1].category, "relic:god:sol");
        let top = components_for(&mk(150), "god:sol", 3, &s);
        assert_eq!((top[0].qty, top[0].min_quality), (3, 70));
    }

    #[test]
    fn trial_specificity_then_power() {
        let rule = |q: &str, t: Option<&str>, e: Option<&str>, p| TrialRule {
            quest_id: q.into(),
            when_type: t.map(String::from),
            when_element: e.map(String::from),
            when_effect: None,
            min_power: p,
            restoration: false,
        };
        let rules = vec![
            rule("any", None, None, 0),
            rule("any_strong", None, None, 120),
            rule("damage", Some("damage"), None, 0),
            rule("fire", Some("damage"), Some("fire"), 0),
            rule("fire_strong", Some("damage"), Some("fire"), 140),
            rule("cold", Some("damage"), Some("cold"), 0),
        ];
        let d = |power| Design {
            spell_type: "damage".into(),
            element: Some("fire".into()),
            effect: None,
            target: "enemy".into(),
            magnitude: 0,
            duration: None,
            power,
        };
        assert_eq!(pick_trial(&rules, &d(100)).unwrap().quest_id, "fire");
        assert_eq!(pick_trial(&rules, &d(150)).unwrap().quest_id, "fire_strong");
        let mut buff = d(130);
        buff.spell_type = "buff".into();
        buff.element = None;
        assert_eq!(pick_trial(&rules, &buff).unwrap().quest_id, "any_strong");
        assert!(pick_trial(&rules[5..], &buff).is_none());
    }
}

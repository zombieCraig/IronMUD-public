//! Combat verb selection — one implementation, shared by the combat tick and
//! the Rhai command scripts.
//!
//! Ranged combat already tiered its verbs by damage severity, so a graze read
//! differently from a solid hit. Melee did not: the tick printed a flat
//! `"You hit X for N damage!"` for a 3-damage scrape and a 19-damage cleave
//! alike. That made ranged strictly juicier than melee for no design reason.
//!
//! The severity calculation had three independent copies (the tick,
//! `shoot.rhai`, `snipe.rhai`) and the ranged verb table had the same three,
//! all agreeing by hand. Both now live here once and are registered into Rhai
//! so the scripts can share them rather than re-declare them.
//!
//! Verbs are supplied in second and third person as an explicit pair rather
//! than derived by suffixing, because several are multi-word ("cleave into",
//! "run through", "smite down") and a naive `+ "s"` mangles them.

use crate::types::{CombatDistance, DamageType};

/// How hard a blow landed, as a fraction of the attack's maximum roll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitSeverity {
    /// Bottom quarter of the damage range — a scrape.
    Graze,
    /// The broad middle.
    Solid,
    /// Top quarter — a decisive blow.
    Devastating,
}

impl HitSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            HitSeverity::Graze => "graze",
            HitSeverity::Solid => "solid",
            HitSeverity::Devastating => "devastating",
        }
    }

    /// Parse the string form used across the Rhai boundary. Unknown values
    /// degrade to `Solid`, the neutral middle, so a typo in a script produces
    /// an ordinary hit line rather than an empty verb.
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "graze" | "low" => HitSeverity::Graze,
            "devastating" | "high" => HitSeverity::Devastating,
            _ => HitSeverity::Solid,
        }
    }
}

/// Band a damage roll against the attack's ceiling.
///
/// A non-positive ceiling means the caller could not determine a maximum, so
/// everything reads as `Solid` — the same fallback the three previous copies
/// used.
pub fn hit_severity(damage: i32, max_damage: i32) -> HitSeverity {
    if max_damage <= 0 {
        HitSeverity::Solid
    } else if damage <= max_damage / 4 {
        HitSeverity::Graze
    } else if damage > (max_damage * 3) / 4 {
        HitSeverity::Devastating
    } else {
        HitSeverity::Solid
    }
}

/// A verb in both persons: `second` for "You _____ the ghoul", `third` for
/// "Kaleth _____ the ghoul".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HitVerb {
    pub second: &'static str,
    pub third: &'static str,
}

const fn verb(second: &'static str, third: &'static str) -> HitVerb {
    HitVerb { second, third }
}

/// Melee hit verb for a damage type at a given severity.
///
/// The `Solid` column is the wording melee has always used, so existing hits
/// read unchanged; `Graze` and `Devastating` are what the tiering adds.
pub fn melee_hit_verb(damage_type: DamageType, severity: HitSeverity) -> HitVerb {
    use DamageType::*;
    use HitSeverity::*;
    match (damage_type, severity) {
        (Slashing, Graze) => verb("nick", "nicks"),
        (Slashing, Solid) => verb("slash", "slashes"),
        (Slashing, Devastating) => verb("cleave into", "cleaves into"),

        (Piercing, Graze) => verb("scratch", "scratches"),
        (Piercing, Solid) => verb("stab", "stabs"),
        (Piercing, Devastating) => verb("run through", "runs through"),

        (Bludgeoning, Graze) => verb("clip", "clips"),
        (Bludgeoning, Solid) => verb("hit", "hits"),
        (Bludgeoning, Devastating) => verb("crush", "crushes"),

        (Fire, Graze) => verb("singe", "singes"),
        (Fire, Solid) => verb("burn", "burns"),
        (Fire, Devastating) => verb("immolate", "immolates"),

        (Cold, Graze) => verb("chill", "chills"),
        (Cold, Solid) => verb("freeze", "freezes"),
        (Cold, Devastating) => verb("flash-freeze", "flash-freezes"),

        (Lightning, Graze) => verb("jolt", "jolts"),
        (Lightning, Solid) => verb("shock", "shocks"),
        (Lightning, Devastating) => verb("electrocute", "electrocutes"),

        (Poison, Graze) => verb("irritate", "irritates"),
        (Poison, Solid) => verb("poison", "poisons"),
        (Poison, Devastating) => verb("envenom", "envenoms"),

        (Acid, Graze) => verb("sting", "stings"),
        (Acid, Solid) => verb("corrode", "corrodes"),
        (Acid, Devastating) => verb("dissolve", "dissolves"),

        (Bite, Graze) => verb("nip", "nips"),
        (Bite, Solid) => verb("bite", "bites"),
        (Bite, Devastating) => verb("maul", "mauls"),

        (Ballistic, Graze) => verb("graze", "grazes"),
        (Ballistic, Solid) => verb("shoot", "shoots"),
        (Ballistic, Devastating) => verb("tear through", "tears through"),

        (Arcane, Graze) => verb("buffet", "buffets"),
        (Arcane, Solid) => verb("blast", "blasts"),
        (Arcane, Devastating) => verb("rupture", "ruptures"),

        (Sunlight, Graze) => verb("scorch", "scorches"),
        (Sunlight, Solid) => verb("sear", "sears"),
        (Sunlight, Devastating) => verb("incinerate", "incinerates"),

        (Holy, Graze) => verb("rebuke", "rebukes"),
        (Holy, Solid) => verb("smite", "smites"),
        (Holy, Devastating) => verb("smite down", "smites down"),
    }
}

/// Third-person miss verb ("snaps at"). Not severity-tiered — a miss has no
/// magnitude.
pub fn miss_verb(damage_type: DamageType) -> &'static str {
    use DamageType::*;
    match damage_type {
        Slashing => "slashes at",
        Piercing => "thrusts at",
        Bludgeoning => "swings at",
        Fire => "hurls flames at",
        Cold => "sends frost at",
        Lightning => "sends lightning at",
        Poison => "strikes at",
        Acid => "flings acid at",
        Bite => "snaps at",
        Ballistic => "fires at",
        Arcane => "hurls magic at",
        Sunlight => "sears with sunlight at",
        Holy => "smites at",
    }
}

/// Noun for what a ranged weapon family sends downrange.
pub fn ranged_projectile_word(family: &str) -> &'static str {
    match family {
        "bow" => "arrow",
        "crossbow" => "bolt",
        _ => "shot",
    }
}

/// Ranged hit verb. Third person only: the grammatical subject is the
/// projectile ("Your arrow lodges in the ghoul"), never the attacker, so a
/// second-person form would never be used.
pub fn ranged_hit_verb(family: &str, severity: HitSeverity) -> &'static str {
    match severity {
        HitSeverity::Graze => {
            if family == "crossbow" {
                "nicks"
            } else {
                "grazes"
            }
        }
        HitSeverity::Solid => match family {
            "bow" => "lodges in",
            "crossbow" => "punches into",
            _ => "rips into",
        },
        HitSeverity::Devastating => {
            if family == "bow" {
                "punches through"
            } else {
                "tears through"
            }
        }
    }
}

// ===========================================================================
// Critical hits
// ===========================================================================
//
// Crits had two unrelated presentations. The `attack`/`shoot` command path
// rolled a damage-type-specific effect ("Arterial Cut! Severe bleeding!") and
// applied a mechanic classified from that name. The combat tick rolled a bare
// mechanic and printed its generic name ("[CRITICAL - Bleeding!]"), and the
// PvP branch printed a third thing again, in a different colour, with no
// effect at all. Same event, three renderings, the poorest of which fired in
// the most common case.
//
// The effect table, the name -> mechanic classification, and the renderer all
// live here now, so the tick and the scripts roll from one table and print one
// sentence.

/// What a critical effect actually does to the target.
///
/// Derived from the effect key, not rolled separately — the flavour name and
/// the mechanic are two views of one thing, and letting them be chosen
/// independently is what allowed the tick to print "Bleeding" for a stun.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CritMechanic {
    /// Open a bleeding wound on the struck body part.
    Bleed,
    /// Add stun rounds.
    Stun,
    /// Escalate the struck part to a severe (disabling) wound.
    Disable,
    /// Stun and bleed together.
    StunBleed,
    /// Elemental damage over time; see [`crit_ongoing_element`].
    Ongoing,
    /// Elemental damage over time plus a stun.
    OngoingStun,
    /// Bonus damage only — no secondary effect.
    Clean,
}

struct CritDef {
    key: &'static str,
    mechanic: CritMechanic,
    /// Element name for the ongoing mechanics; empty otherwise.
    element: &'static str,
    /// Rendered body of the `[CRITICAL - ...]` tag. `{part}` is substituted
    /// with the struck body part.
    label: &'static str,
}

/// Every crit effect in the game, grouped by the damage type that rolls it.
///
/// Three effects per damage type; a fourth roll is a clean crit. The generic
/// `bleeding`/`stun`/`disable` keys at the end are the legacy names — still
/// accepted so old saved state and any script passing them renders sensibly.
const CRIT_DEFS: &[CritDef] = &[
    // Slashing
    CritDef {
        key: "deep_laceration",
        mechanic: CritMechanic::Bleed,
        element: "",
        label: "Deep Laceration! Heavy bleeding!",
    },
    CritDef {
        key: "severed_tendon",
        mechanic: CritMechanic::Disable,
        element: "",
        label: "Severed Tendon! {part} disabled!",
    },
    CritDef {
        key: "arterial_cut",
        mechanic: CritMechanic::Bleed,
        element: "",
        label: "Arterial Cut! Severe bleeding!",
    },
    // Piercing
    CritDef {
        key: "punctured_organ",
        mechanic: CritMechanic::Bleed,
        element: "",
        label: "Punctured Organ! Internal bleeding!",
    },
    CritDef {
        key: "impaled",
        mechanic: CritMechanic::StunBleed,
        element: "",
        label: "Impaled! Stunned and bleeding!",
    },
    CritDef {
        key: "nerve_strike",
        mechanic: CritMechanic::Stun,
        element: "",
        label: "Nerve Strike! Stunned!",
    },
    // Bludgeoning
    CritDef {
        key: "broken_bone",
        mechanic: CritMechanic::Disable,
        element: "",
        label: "Broken Bone! {part} disabled!",
    },
    CritDef {
        key: "concussion",
        mechanic: CritMechanic::Stun,
        element: "",
        label: "Concussion! Stunned!",
    },
    CritDef {
        key: "crushed",
        mechanic: CritMechanic::Clean,
        element: "",
        label: "Crushed! Devastating blow!",
    },
    // Fire
    CritDef {
        key: "severe_burn",
        mechanic: CritMechanic::Ongoing,
        element: "fire",
        label: "Severe Burn! Ongoing fire damage!",
    },
    CritDef {
        key: "ignited",
        mechanic: CritMechanic::OngoingStun,
        element: "fire",
        label: "Ignited! Burning and stunned!",
    },
    CritDef {
        key: "charred",
        mechanic: CritMechanic::Disable,
        element: "",
        label: "Charred! {part} disabled!",
    },
    // Cold
    CritDef {
        key: "frozen_limb",
        mechanic: CritMechanic::Disable,
        element: "",
        label: "Frozen Limb! {part} disabled!",
    },
    CritDef {
        key: "hypothermic_shock",
        mechanic: CritMechanic::Stun,
        element: "",
        label: "Hypothermic Shock! Stunned!",
    },
    CritDef {
        key: "frostbitten",
        mechanic: CritMechanic::Ongoing,
        element: "cold",
        label: "Frostbitten! Ongoing cold damage!",
    },
    // Lightning
    CritDef {
        key: "electrocuted",
        mechanic: CritMechanic::Stun,
        element: "",
        label: "Electrocuted! Stunned!",
    },
    CritDef {
        key: "nerve_damage",
        mechanic: CritMechanic::Disable,
        element: "",
        label: "Nerve Damage! {part} disabled!",
    },
    CritDef {
        key: "cardiac_shock",
        mechanic: CritMechanic::Clean,
        element: "",
        label: "Cardiac Shock! Devastating jolt!",
    },
    // Poison
    CritDef {
        key: "venom_surge",
        mechanic: CritMechanic::Ongoing,
        element: "poison",
        label: "Venom Surge! Ongoing poison damage!",
    },
    CritDef {
        key: "toxic_shock",
        mechanic: CritMechanic::Stun,
        element: "",
        label: "Toxic Shock! Stunned!",
    },
    CritDef {
        key: "paralysis",
        mechanic: CritMechanic::Disable,
        element: "",
        label: "Paralysis! {part} disabled!",
    },
    // Acid
    CritDef {
        key: "acid_burn",
        mechanic: CritMechanic::Ongoing,
        element: "acid",
        label: "Acid Burn! Ongoing acid damage!",
    },
    CritDef {
        key: "corroded_armor",
        mechanic: CritMechanic::Disable,
        element: "",
        label: "Corroded! {part} disabled!",
    },
    CritDef {
        key: "dissolved_flesh",
        mechanic: CritMechanic::Bleed,
        element: "",
        label: "Dissolved Flesh! Heavy bleeding!",
    },
    // Bite
    CritDef {
        key: "mauled",
        mechanic: CritMechanic::Bleed,
        element: "",
        label: "Mauled! Heavy bleeding!",
    },
    CritDef {
        key: "lockjaw",
        mechanic: CritMechanic::Stun,
        element: "",
        label: "Lockjaw! Clamped and stunned!",
    },
    CritDef {
        key: "severed_chunk",
        mechanic: CritMechanic::Disable,
        element: "",
        label: "Chunk Torn Away! {part} disabled!",
    },
    // Ballistic
    CritDef {
        key: "through_and_through",
        mechanic: CritMechanic::Bleed,
        element: "",
        label: "Through and Through! Heavy bleeding!",
    },
    CritDef {
        key: "shrapnel",
        mechanic: CritMechanic::Disable,
        element: "",
        label: "Shrapnel! {part} disabled!",
    },
    CritDef {
        key: "bullet_lodged",
        mechanic: CritMechanic::StunBleed,
        element: "",
        label: "Bullet Lodged! Stunned and bleeding!",
    },
    // Arcane
    CritDef {
        key: "arcane_feedback",
        mechanic: CritMechanic::Stun,
        element: "",
        label: "Arcane Feedback! Stunned!",
    },
    CritDef {
        key: "unraveled",
        mechanic: CritMechanic::Disable,
        element: "",
        label: "Unraveled! {part} disabled!",
    },
    CritDef {
        key: "ruptured_ward",
        mechanic: CritMechanic::Clean,
        element: "",
        label: "Ruptured Ward! Devastating surge!",
    },
    // Sunlight
    CritDef {
        key: "seared",
        mechanic: CritMechanic::Ongoing,
        element: "fire",
        label: "Seared! Ongoing burning!",
    },
    CritDef {
        key: "blinding_flare",
        mechanic: CritMechanic::Stun,
        element: "",
        label: "Blinding Flare! Stunned!",
    },
    CritDef {
        key: "scorched",
        mechanic: CritMechanic::Disable,
        element: "",
        label: "Scorched! {part} disabled!",
    },
    // Holy
    CritDef {
        key: "consecrated_wound",
        mechanic: CritMechanic::Bleed,
        element: "",
        label: "Consecrated Wound! Heavy bleeding!",
    },
    CritDef {
        key: "smitten",
        mechanic: CritMechanic::Stun,
        element: "",
        label: "Smitten! Stunned!",
    },
    CritDef {
        key: "hallowed_break",
        mechanic: CritMechanic::Disable,
        element: "",
        label: "Hallowed Break! {part} disabled!",
    },
    // Legacy generic keys
    CritDef {
        key: "bleeding",
        mechanic: CritMechanic::Bleed,
        element: "",
        label: "Bleeding!",
    },
    CritDef {
        key: "stun",
        mechanic: CritMechanic::Stun,
        element: "",
        label: "Stun!",
    },
    CritDef {
        key: "disable",
        mechanic: CritMechanic::Disable,
        element: "",
        label: "{part} Disabled!",
    },
    CritDef {
        key: "clean",
        mechanic: CritMechanic::Clean,
        element: "",
        label: "",
    },
];

/// The three effect keys a damage type can roll, in roll order.
pub fn crit_effect_keys(damage_type: DamageType) -> [&'static str; 3] {
    match damage_type {
        DamageType::Slashing => ["deep_laceration", "severed_tendon", "arterial_cut"],
        DamageType::Piercing => ["punctured_organ", "impaled", "nerve_strike"],
        DamageType::Bludgeoning => ["broken_bone", "concussion", "crushed"],
        DamageType::Fire => ["severe_burn", "ignited", "charred"],
        DamageType::Cold => ["frozen_limb", "hypothermic_shock", "frostbitten"],
        DamageType::Lightning => ["electrocuted", "nerve_damage", "cardiac_shock"],
        DamageType::Poison => ["venom_surge", "toxic_shock", "paralysis"],
        DamageType::Acid => ["acid_burn", "corroded_armor", "dissolved_flesh"],
        DamageType::Bite => ["mauled", "lockjaw", "severed_chunk"],
        DamageType::Ballistic => ["through_and_through", "shrapnel", "bullet_lodged"],
        DamageType::Arcane => ["arcane_feedback", "unraveled", "ruptured_ward"],
        DamageType::Sunlight => ["seared", "blinding_flare", "scorched"],
        DamageType::Holy => ["consecrated_wound", "smitten", "hallowed_break"],
    }
}

/// Pick the effect for a d4 roll. Rolls 1-3 index the damage type's table;
/// anything else (the fourth face, or an out-of-range caller) is a clean crit.
pub fn roll_crit_effect(damage_type: DamageType, roll: i32) -> &'static str {
    match roll {
        1..=3 => crit_effect_keys(damage_type)[(roll - 1) as usize],
        _ => "clean",
    }
}

fn crit_def(key: &str) -> Option<&'static CritDef> {
    CRIT_DEFS.iter().find(|d| d.key == key)
}

/// Classify an effect key. Unknown keys are treated as clean crits so a stale
/// name adds bonus damage rather than panicking or silently stunning.
pub fn crit_mechanic(key: &str) -> CritMechanic {
    crit_def(key).map_or(CritMechanic::Clean, |d| d.mechanic)
}

/// Element applied by the ongoing mechanics. Empty for every other mechanic.
pub fn crit_ongoing_element(key: &str) -> &'static str {
    crit_def(key).map_or("", |d| d.element)
}

/// Colour a crit tag by damage type. Physical types share the default yellow.
pub fn crit_color(damage_type: DamageType) -> &'static str {
    match damage_type {
        DamageType::Fire | DamageType::Bite => "\x1b[1;31m",
        DamageType::Cold => "\x1b[1;36m",
        DamageType::Poison | DamageType::Acid => "\x1b[1;32m",
        DamageType::Lightning => "\x1b[1;34m",
        DamageType::Ballistic => "\x1b[1;37m",
        _ => "\x1b[1;33m",
    }
}

/// Render the `[CRITICAL ...]` tag, including its leading space.
///
/// Returns an empty string when the hit was not a crit, so callers can append
/// unconditionally. A blocked crit reports as blocked regardless of the rolled
/// effect — the effect never landed.
pub fn crit_text(is_crit: bool, key: &str, body_part: &str, blocked: bool, damage_type: DamageType) -> String {
    if !is_crit {
        return String::new();
    }
    let color = crit_color(damage_type);
    if blocked {
        return format!(" {}[CRITICAL - Blocked!]\x1b[0m", color);
    }
    match crit_def(key).map(|d| d.label).unwrap_or("") {
        "" => format!(" {}[CRITICAL]\x1b[0m", color),
        label => format!(" {}[CRITICAL - {}]\x1b[0m", color, label.replace("{part}", body_part)),
    }
}

/// How badly hurt something is, as one band of a six-step ladder.
///
/// Four separate copies of this ladder existed before it lived here:
/// `examine.rhai` had two (one for mobs, one for players), `look.rhai` a
/// third, and `lore.rhai` a fourth that used *different thresholds* — so a
/// mob at 85% "looked healthy" to `lore` and "had a few scratches" to
/// `examine`. The prompt tag would have been a fifth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Condition {
    Dead,
    Critical,
    Bloodied,
    Wounded,
    Scratched,
    Unhurt,
}

impl Condition {
    /// Band for a health percentage. Thresholds are the ones `examine` and
    /// `look` already agreed on; `lore` was the outlier and now follows.
    pub fn from_pct(pct: i32) -> Self {
        if pct <= 0 {
            Self::Dead
        } else if pct < 25 {
            Self::Critical
        } else if pct < 50 {
            Self::Bloodied
        } else if pct < 75 {
            Self::Wounded
        } else if pct < 100 {
            Self::Scratched
        } else {
            Self::Unhurt
        }
    }

    pub fn from_hp(hp: i32, max_hp: i32) -> Self {
        if max_hp <= 0 {
            return Self::Dead;
        }
        Self::from_pct((hp * 100) / max_hp)
    }

    /// One word, for the prompt — where every character costs.
    pub fn tag(self) -> &'static str {
        match self {
            Self::Dead => "Dead",
            Self::Critical => "Critical",
            Self::Bloodied => "Bloodied",
            Self::Wounded => "Wounded",
            Self::Scratched => "Scratched",
            Self::Unhurt => "Unhurt",
        }
    }

    /// The predicate used by `look`/`examine`/`lore`, which have room for a
    /// sentence. Caller supplies the subject.
    pub fn predicate(self) -> &'static str {
        match self {
            Self::Dead => "appears to be dead.",
            Self::Critical => "is barely holding on.",
            Self::Bloodied => "is badly wounded.",
            Self::Wounded => "has some wounds.",
            Self::Scratched => "has a few scratches.",
            Self::Unhurt => "is in perfect health.",
        }
    }

    /// Prompt colour, on the same green/yellow/red logic as the HP segment.
    pub fn color(self) -> &'static str {
        match self {
            Self::Unhurt | Self::Scratched => "\x1b[32m",
            Self::Wounded => "\x1b[33m",
            Self::Bloodied | Self::Critical | Self::Dead => "\x1b[31m",
        }
    }
}

/// `"<name> has a few scratches."` — the full sentence form.
pub fn condition_sentence(name: &str, hp: i32, max_hp: i32) -> String {
    format!("{} {}", name, Condition::from_hp(hp, max_hp).predicate())
}

/// The combat prompt's target segment, including its trailing space:
/// `[a ghoul: Bloodied] ` or `[a ghoul: Unhurt | Ranged] `.
///
/// Lives here rather than inline in `build_prompt` so the formatting is
/// testable without standing up a `World`; the caller keeps the DB lookups.
pub fn target_prompt_tag(name: &str, hp: i32, max_hp: i32, distance: CombatDistance, colors: bool) -> String {
    let condition = Condition::from_hp(hp, max_hp);
    // Melee is the default and by far the common case, so it goes unlabelled
    // — spending prompt width to say "the usual" helps nobody.
    let distance_part = match distance {
        CombatDistance::Ranged => " | Ranged",
        CombatDistance::Pole => " | Pole",
        CombatDistance::Melee => "",
    };
    if colors {
        format!(
            "{}[{}: {}{}]\x1b[0m ",
            condition.color(),
            name,
            condition.tag(),
            distance_part
        )
    } else {
        format!("[{}: {}{}] ", name, condition.tag(), distance_part)
    }
}

/// Register the Rhai surface so `attack.rhai`, `shoot.rhai` and `snipe.rhai`
/// share these tables instead of carrying their own copies.
pub fn register(engine: &mut rhai::Engine) {
    // condition_sentence(name, hp, max_hp) -> "a ghoul has a few scratches."
    engine.register_fn("condition_sentence", |name: String, hp: i64, max_hp: i64| -> String {
        condition_sentence(&name, hp as i32, max_hp as i32)
    });

    // condition_tag(hp, max_hp) -> "Bloodied"
    engine.register_fn("condition_tag", |hp: i64, max_hp: i64| -> String {
        Condition::from_hp(hp as i32, max_hp as i32).tag().to_string()
    });

    // hit_severity(damage, max_damage) -> "graze" | "solid" | "devastating"
    engine.register_fn("hit_severity", |damage: i64, max_damage: i64| -> String {
        hit_severity(damage as i32, max_damage as i32).as_str().to_string()
    });

    // melee_hit_verb(damage_type, severity) -> String   (second person)
    engine.register_fn("melee_hit_verb", |damage_type: String, severity: String| -> String {
        melee_hit_verb(
            DamageType::from_str(&damage_type).unwrap_or_default(),
            HitSeverity::from_str_lossy(&severity),
        )
        .second
        .to_string()
    });

    // melee_hit_verb_third(damage_type, severity) -> String
    engine.register_fn(
        "melee_hit_verb_third",
        |damage_type: String, severity: String| -> String {
            melee_hit_verb(
                DamageType::from_str(&damage_type).unwrap_or_default(),
                HitSeverity::from_str_lossy(&severity),
            )
            .third
            .to_string()
        },
    );

    // ranged_hit_verb(family, severity) -> String
    engine.register_fn("ranged_hit_verb", |family: String, severity: String| -> String {
        ranged_hit_verb(&family, HitSeverity::from_str_lossy(&severity)).to_string()
    });

    // ranged_projectile_word(family) -> String
    engine.register_fn("ranged_projectile_word", |family: String| -> String {
        ranged_projectile_word(&family).to_string()
    });

    // crit_text(is_crit, effect, body_part, blocked, damage_type) -> String
    engine.register_fn(
        "crit_text",
        |is_crit: bool, effect: String, body_part: String, blocked: bool, damage_type: String| -> String {
            crit_text(
                is_crit,
                &effect,
                &body_part,
                blocked,
                DamageType::from_str(&damage_type).unwrap_or_default(),
            )
        },
    );

    // crit_mechanic(effect) -> "bleed" | "stun" | "disable" | "stun_bleed"
    //                        | "ongoing" | "ongoing_stun" | "clean"
    engine.register_fn("crit_mechanic", |effect: String| -> String {
        match crit_mechanic(&effect) {
            CritMechanic::Bleed => "bleed",
            CritMechanic::Stun => "stun",
            CritMechanic::Disable => "disable",
            CritMechanic::StunBleed => "stun_bleed",
            CritMechanic::Ongoing => "ongoing",
            CritMechanic::OngoingStun => "ongoing_stun",
            CritMechanic::Clean => "clean",
        }
        .to_string()
    });

    // crit_ongoing_element(effect) -> "fire" | "cold" | "poison" | "acid" | ""
    engine.register_fn("crit_ongoing_element", |effect: String| -> String {
        crit_ongoing_element(&effect).to_string()
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_bands_split_at_a_quarter_and_three_quarters() {
        assert_eq!(hit_severity(5, 20), HitSeverity::Graze);
        assert_eq!(hit_severity(6, 20), HitSeverity::Solid);
        assert_eq!(hit_severity(15, 20), HitSeverity::Solid);
        assert_eq!(hit_severity(16, 20), HitSeverity::Devastating);
    }

    #[test]
    fn an_unknown_ceiling_reads_as_solid() {
        assert_eq!(hit_severity(9, 0), HitSeverity::Solid);
        assert_eq!(hit_severity(9, -1), HitSeverity::Solid);
    }

    #[test]
    fn solid_melee_wording_is_unchanged_from_before_the_tiering() {
        // The flat verbs melee used to print, now the middle band.
        for (dt, want) in [
            (DamageType::Slashing, "slashes"),
            (DamageType::Piercing, "stabs"),
            (DamageType::Bludgeoning, "hits"),
            (DamageType::Bite, "bites"),
            (DamageType::Holy, "smites"),
        ] {
            assert_eq!(melee_hit_verb(dt, HitSeverity::Solid).third, want, "{:?}", dt);
        }
    }

    #[test]
    fn every_damage_type_has_three_distinct_bands() {
        let all = [
            DamageType::Slashing,
            DamageType::Piercing,
            DamageType::Bludgeoning,
            DamageType::Fire,
            DamageType::Cold,
            DamageType::Lightning,
            DamageType::Poison,
            DamageType::Acid,
            DamageType::Bite,
            DamageType::Ballistic,
            DamageType::Arcane,
            DamageType::Sunlight,
            DamageType::Holy,
        ];
        for dt in all {
            let g = melee_hit_verb(dt, HitSeverity::Graze);
            let s = melee_hit_verb(dt, HitSeverity::Solid);
            let d = melee_hit_verb(dt, HitSeverity::Devastating);
            assert!(
                g != s && s != d && g != d,
                "{:?} reuses a verb across severities: {:?} {:?} {:?}",
                dt,
                g,
                s,
                d
            );
            for v in [g, s, d] {
                assert!(
                    !v.second.is_empty() && !v.third.is_empty(),
                    "{:?} has an empty verb",
                    dt
                );
                assert_ne!(v.second, v.third, "{:?} second and third person collide", dt);
            }
        }
    }

    #[test]
    fn multiword_verbs_inflect_only_the_head() {
        assert_eq!(
            melee_hit_verb(DamageType::Slashing, HitSeverity::Devastating).third,
            "cleaves into"
        );
        assert_eq!(
            melee_hit_verb(DamageType::Piercing, HitSeverity::Devastating).third,
            "runs through"
        );
        assert_eq!(
            melee_hit_verb(DamageType::Holy, HitSeverity::Devastating).third,
            "smites down"
        );
    }

    #[test]
    fn ranged_table_matches_the_three_copies_it_replaces() {
        assert_eq!(ranged_hit_verb("crossbow", HitSeverity::Graze), "nicks");
        assert_eq!(ranged_hit_verb("bow", HitSeverity::Graze), "grazes");
        assert_eq!(ranged_hit_verb("bow", HitSeverity::Solid), "lodges in");
        assert_eq!(ranged_hit_verb("crossbow", HitSeverity::Solid), "punches into");
        assert_eq!(ranged_hit_verb("gun", HitSeverity::Solid), "rips into");
        assert_eq!(ranged_hit_verb("bow", HitSeverity::Devastating), "punches through");
        assert_eq!(ranged_hit_verb("gun", HitSeverity::Devastating), "tears through");
    }

    #[test]
    fn projectile_words() {
        assert_eq!(ranged_projectile_word("bow"), "arrow");
        assert_eq!(ranged_projectile_word("crossbow"), "bolt");
        assert_eq!(ranged_projectile_word("rifle"), "shot");
    }

    #[test]
    fn severity_round_trips_across_the_rhai_boundary() {
        for s in [HitSeverity::Graze, HitSeverity::Solid, HitSeverity::Devastating] {
            assert_eq!(HitSeverity::from_str_lossy(s.as_str()), s);
        }
        // The old scripts spoke low/medium/high; keep those accepted.
        assert_eq!(HitSeverity::from_str_lossy("low"), HitSeverity::Graze);
        assert_eq!(HitSeverity::from_str_lossy("high"), HitSeverity::Devastating);
        assert_eq!(HitSeverity::from_str_lossy("medium"), HitSeverity::Solid);
        assert_eq!(HitSeverity::from_str_lossy("nonsense"), HitSeverity::Solid);
    }
}

#[cfg(test)]
mod rhai_binding_tests {
    /// The scripts call these by name at runtime; a missing registration is a
    /// runtime error Rhai's compile check cannot catch, so assert the whole
    /// surface resolves and returns what the scripts expect.
    #[test]
    fn every_registered_fn_resolves_and_returns_the_right_words() {
        let mut engine = rhai::Engine::new();
        super::register(&mut engine);

        let cases: [(&str, &str); 6] = [
            (r#"hit_severity(3, 20)"#, "graze"),
            (r#"hit_severity(19, 20)"#, "devastating"),
            (r#"melee_hit_verb("slashing", hit_severity(19, 20))"#, "cleave into"),
            (
                r#"melee_hit_verb_third("slashing", hit_severity(19, 20))"#,
                "cleaves into",
            ),
            (r#"ranged_hit_verb("bow", hit_severity(3, 20))"#, "grazes"),
            (r#"ranged_projectile_word("crossbow")"#, "bolt"),
        ];
        for (src, want) in cases {
            let got: String = engine.eval(src).unwrap_or_else(|e| panic!("`{}` failed: {}", src, e));
            assert_eq!(got, want, "`{}`", src);
        }
    }

    #[test]
    fn an_unknown_damage_type_still_yields_a_usable_verb() {
        let mut engine = rhai::Engine::new();
        super::register(&mut engine);
        // Scripts pass whatever the item carries; a bad value must not blank
        // out the combat line.
        let got: String = engine.eval(r#"melee_hit_verb("nonsense", "solid")"#).unwrap();
        assert!(!got.is_empty());
    }

    #[test]
    fn the_crit_bindings_resolve_and_agree_with_the_rust_table() {
        let mut engine = rhai::Engine::new();
        super::register(&mut engine);

        let mechanic: String = engine.eval(r#"crit_mechanic("severed_tendon")"#).unwrap();
        assert_eq!(mechanic, "disable");
        let element: String = engine.eval(r#"crit_ongoing_element("frostbitten")"#).unwrap();
        assert_eq!(element, "cold");
        let tag: String = engine
            .eval(r#"crit_text(true, "severed_tendon", "left arm", false, "slashing")"#)
            .unwrap();
        assert!(tag.contains("Severed Tendon! left arm disabled!"), "{}", tag);
        let none: String = engine
            .eval(r#"crit_text(false, "severed_tendon", "left arm", false, "slashing")"#)
            .unwrap();
        assert_eq!(none, "");
    }

    #[test]
    fn the_condition_bindings_resolve_and_agree_with_the_rust_table() {
        let mut engine = rhai::Engine::new();
        super::register(&mut engine);

        let sentence: String = engine.eval(r#"condition_sentence("a ghoul", 60, 100)"#).unwrap();
        assert_eq!(sentence, "a ghoul has some wounds.");
        let tag: String = engine.eval(r#"condition_tag(30, 100)"#).unwrap();
        assert_eq!(tag, "Bloodied");
        // A prototype with no max_hp must not divide by zero — `lore` used to
        // do exactly that before it moved onto this table.
        let dead: String = engine.eval(r#"condition_tag(5, 0)"#).unwrap();
        assert_eq!(dead, "Dead");
    }
}

#[cfg(test)]
mod condition_tests {
    use super::*;

    #[test]
    fn bands_match_the_thresholds_look_and_examine_already_used() {
        assert_eq!(Condition::from_pct(100), Condition::Unhurt);
        assert_eq!(Condition::from_pct(150), Condition::Unhurt);
        assert_eq!(Condition::from_pct(99), Condition::Scratched);
        assert_eq!(Condition::from_pct(75), Condition::Scratched);
        assert_eq!(Condition::from_pct(74), Condition::Wounded);
        assert_eq!(Condition::from_pct(50), Condition::Wounded);
        assert_eq!(Condition::from_pct(49), Condition::Bloodied);
        assert_eq!(Condition::from_pct(25), Condition::Bloodied);
        assert_eq!(Condition::from_pct(24), Condition::Critical);
        assert_eq!(Condition::from_pct(1), Condition::Critical);
        assert_eq!(Condition::from_pct(0), Condition::Dead);
        assert_eq!(Condition::from_pct(-10), Condition::Dead);
    }

    #[test]
    fn a_zero_max_never_divides() {
        assert_eq!(Condition::from_hp(50, 0), Condition::Dead);
        assert_eq!(Condition::from_hp(0, 0), Condition::Dead);
    }

    #[test]
    fn every_band_has_a_distinct_tag_and_predicate() {
        let all = [
            Condition::Dead,
            Condition::Critical,
            Condition::Bloodied,
            Condition::Wounded,
            Condition::Scratched,
            Condition::Unhurt,
        ];
        let tags: std::collections::HashSet<_> = all.iter().map(|c| c.tag()).collect();
        let preds: std::collections::HashSet<_> = all.iter().map(|c| c.predicate()).collect();
        assert_eq!(tags.len(), all.len());
        assert_eq!(preds.len(), all.len());
    }

    #[test]
    fn ordering_runs_from_dead_up_to_unhurt() {
        assert!(Condition::from_pct(10) < Condition::from_pct(90));
        assert!(Condition::from_pct(0) < Condition::from_pct(1));
    }

    #[test]
    fn the_sentence_form_keeps_the_wording_the_scripts_shipped() {
        // These exact strings were in look.rhai and examine.rhai before the
        // ladder moved here; players should see no change.
        assert_eq!(condition_sentence("Bob", 100, 100), "Bob is in perfect health.");
        assert_eq!(condition_sentence("Bob", 80, 100), "Bob has a few scratches.");
        assert_eq!(condition_sentence("Bob", 60, 100), "Bob has some wounds.");
        assert_eq!(condition_sentence("Bob", 30, 100), "Bob is badly wounded.");
        assert_eq!(condition_sentence("Bob", 10, 100), "Bob is barely holding on.");
        assert_eq!(condition_sentence("Bob", 0, 100), "Bob appears to be dead.");
    }

    #[test]
    fn the_prompt_tag_leads_with_the_name_and_omits_melee() {
        // Melee is the default engagement; labelling it spends prompt width
        // to say nothing.
        assert_eq!(
            target_prompt_tag("a ghoul", 30, 100, CombatDistance::Melee, false),
            "[a ghoul: Bloodied] "
        );
        assert_eq!(
            target_prompt_tag("a ghoul", 100, 100, CombatDistance::Ranged, false),
            "[a ghoul: Unhurt | Ranged] "
        );
        assert_eq!(
            target_prompt_tag("a ghoul", 60, 100, CombatDistance::Pole, false),
            "[a ghoul: Wounded | Pole] "
        );
    }

    #[test]
    fn the_prompt_tag_colours_by_condition_and_always_resets() {
        let hurt = target_prompt_tag("a ghoul", 10, 100, CombatDistance::Melee, true);
        assert!(hurt.starts_with(Condition::Critical.color()), "{:?}", hurt);
        assert!(hurt.ends_with("\x1b[0m "), "{:?}", hurt);

        // colors off must emit no escapes at all — a client that cannot
        // render them would otherwise see raw bytes in its input line.
        let plain = target_prompt_tag("a ghoul", 10, 100, CombatDistance::Melee, false);
        assert!(!plain.contains('\x1b'), "{:?}", plain);
    }

    #[test]
    fn colour_tracks_severity() {
        assert_eq!(Condition::Unhurt.color(), Condition::Scratched.color());
        assert_ne!(Condition::Scratched.color(), Condition::Wounded.color());
        assert_ne!(Condition::Wounded.color(), Condition::Bloodied.color());
    }
}

// ---------------------------------------------------------------------------
// Kill resolution block
// ---------------------------------------------------------------------------

/// The killer's own slay line.
pub fn slay_line(mob_display: &str) -> String {
    format!("You have slain {}.", mob_display)
}

/// The slay line for someone who took part but did not land the killing blow.
///
/// Separate from [`slay_line`] rather than a boolean parameter because they are
/// different sentences with different subjects, and a party member reading
/// "You have slain" for a kill they assisted is a lie about who did what.
pub fn party_slay_line(killer_display: &str, mob_display: &str) -> String {
    format!(
        "Your group has slain {} ({} landed the blow).",
        mob_display, killer_display
    )
}

/// The corpse-contents line under a slay line. Every credited participant gets
/// it, not just the killer — they can all loot the corpse, so they should all
/// know what is in it.
pub fn corpse_contents_line(contents: &str) -> String {
    format!("  It carried: {}.", contents)
}

/// The personal kill-milestone line. Each participant crosses their own
/// milestones, so this is per-character and not part of the slay line.
pub fn kill_milestone_line(ordinal: &str) -> String {
    format!("  \x1b[1;33mThat is your {} kill.\x1b[0m", ordinal)
}

#[cfg(test)]
mod kill_block_tests {
    use super::*;

    #[test]
    fn the_killer_and_the_party_get_different_subjects() {
        assert_eq!(slay_line("a ghoul"), "You have slain a ghoul.");
        assert_eq!(
            party_slay_line("Kaleth", "a ghoul"),
            "Your group has slain a ghoul (Kaleth landed the blow)."
        );
    }

    #[test]
    fn the_milestone_line_resets_its_colour() {
        let line = kill_milestone_line("250th");
        assert!(line.contains("250th"), "{}", line);
        assert!(line.ends_with("\x1b[0m"), "{:?}", line);
    }
}

#[cfg(test)]
mod crit_tests {
    use super::*;

    #[test]
    fn every_rolled_effect_is_in_the_definition_table() {
        // A key that rolls but has no definition renders a bare `[CRITICAL]`
        // and classifies as clean — a silent downgrade, so catch it here.
        for damage_type in [
            DamageType::Bludgeoning,
            DamageType::Slashing,
            DamageType::Piercing,
            DamageType::Fire,
            DamageType::Cold,
            DamageType::Lightning,
            DamageType::Poison,
            DamageType::Acid,
            DamageType::Bite,
            DamageType::Ballistic,
            DamageType::Arcane,
            DamageType::Sunlight,
            DamageType::Holy,
        ] {
            for key in crit_effect_keys(damage_type) {
                assert!(crit_def(key).is_some(), "{:?} rolls undefined key {}", damage_type, key);
            }
        }
    }

    #[test]
    fn ongoing_effects_name_an_element_and_others_do_not() {
        for def in CRIT_DEFS {
            let needs_element = matches!(def.mechanic, CritMechanic::Ongoing | CritMechanic::OngoingStun);
            assert_eq!(
                needs_element,
                !def.element.is_empty(),
                "{} element/mechanic mismatch",
                def.key
            );
        }
    }

    #[test]
    fn disabling_effects_substitute_the_body_part() {
        // The `{part}` placeholder only renders through `crit_text`; a def that
        // spells the part into the label directly would print a literal.
        for def in CRIT_DEFS {
            if def.mechanic == CritMechanic::Disable && !def.label.is_empty() {
                assert!(def.label.contains("{part}"), "{} names no body part", def.key);
            }
        }
        let tag = crit_text(true, "broken_bone", "right leg", false, DamageType::Bludgeoning);
        assert!(tag.contains("right leg"), "{}", tag);
        assert!(!tag.contains("{part}"), "{}", tag);
    }

    #[test]
    fn a_blocked_crit_reports_blocked_whatever_was_rolled() {
        let tag = crit_text(true, "arterial_cut", "torso", true, DamageType::Slashing);
        assert!(tag.contains("Blocked!"), "{}", tag);
        assert!(!tag.contains("Arterial"), "{}", tag);
    }

    #[test]
    fn the_fourth_face_and_out_of_range_rolls_are_clean() {
        assert_eq!(roll_crit_effect(DamageType::Slashing, 4), "clean");
        assert_eq!(roll_crit_effect(DamageType::Slashing, 0), "clean");
        assert_eq!(crit_mechanic("clean"), CritMechanic::Clean);
        let tag = crit_text(true, "clean", "torso", false, DamageType::Slashing);
        assert!(tag.contains("[CRITICAL]"), "{}", tag);
    }

    #[test]
    fn an_unknown_key_degrades_to_a_clean_crit() {
        assert_eq!(crit_mechanic("no_such_effect"), CritMechanic::Clean);
        assert_eq!(crit_ongoing_element("no_such_effect"), "");
        let tag = crit_text(true, "no_such_effect", "torso", false, DamageType::Fire);
        assert!(tag.contains("[CRITICAL]"), "{}", tag);
    }

    #[test]
    fn the_legacy_generic_keys_still_classify() {
        assert_eq!(crit_mechanic("bleeding"), CritMechanic::Bleed);
        assert_eq!(crit_mechanic("stun"), CritMechanic::Stun);
        assert_eq!(crit_mechanic("disable"), CritMechanic::Disable);
    }

    #[test]
    fn keys_are_unique() {
        let mut keys: Vec<&str> = CRIT_DEFS.iter().map(|d| d.key).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(before, keys.len(), "duplicate crit key");
    }
}

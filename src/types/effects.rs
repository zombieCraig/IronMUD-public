//! Damage and status-effect types shared by combat and item systems.
//!
//! `DamageType` is consumed by combat resolution and weapon/attack
//! definitions. `EffectType` enumerates buffs/debuffs applied to mobiles
//! and players. `ItemEffect` is the prototype-time configuration on
//! consumables and equipment; `ActiveBuff` is the live, decaying instance
//! attached to a mobile / character.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DamageType {
    #[default]
    Bludgeoning,
    Slashing,
    Piercing,
    Fire,
    Cold,
    Lightning,
    Poison,
    Acid,
    Bite,
    Ballistic,
    Arcane,
    /// Direct sunlight damage. Vampires (and other `holy_vulnerable` undead) take
    /// this from the sun-exposure tick or fire-equivalent attacks tagged as
    /// sunlight (e.g. mirror-flash spells). Not blocked by physical armor.
    Sunlight,
    /// Damage from divine/blessed sources (holy water, blessed weapons).
    /// `MobileFlags.holy_vulnerable` doubles incoming Holy damage.
    Holy,
}

impl DamageType {
    pub fn from_str(s: &str) -> Option<DamageType> {
        match s.to_lowercase().as_str() {
            "bludgeoning" | "bludgeon" => Some(DamageType::Bludgeoning),
            "slashing" | "slash" => Some(DamageType::Slashing),
            "piercing" | "pierce" => Some(DamageType::Piercing),
            "fire" => Some(DamageType::Fire),
            "cold" => Some(DamageType::Cold),
            "lightning" => Some(DamageType::Lightning),
            "poison" => Some(DamageType::Poison),
            "acid" => Some(DamageType::Acid),
            "bite" => Some(DamageType::Bite),
            "ballistic" | "bullet" | "projectile" => Some(DamageType::Ballistic),
            "arcane" | "magic" => Some(DamageType::Arcane),
            "sunlight" | "sun" => Some(DamageType::Sunlight),
            "holy" | "divine" | "blessed" => Some(DamageType::Holy),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            DamageType::Bludgeoning => "bludgeoning",
            DamageType::Slashing => "slashing",
            DamageType::Piercing => "piercing",
            DamageType::Fire => "fire",
            DamageType::Cold => "cold",
            DamageType::Lightning => "lightning",
            DamageType::Poison => "poison",
            DamageType::Acid => "acid",
            DamageType::Bite => "bite",
            DamageType::Ballistic => "ballistic",
            DamageType::Arcane => "arcane",
            DamageType::Sunlight => "sunlight",
            DamageType::Holy => "holy",
        }
    }

    pub fn all() -> Vec<&'static str> {
        vec![
            "bludgeoning",
            "slashing",
            "piercing",
            "fire",
            "cold",
            "lightning",
            "poison",
            "acid",
            "bite",
            "ballistic",
            "arcane",
            "sunlight",
            "holy",
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EffectType {
    #[default]
    None,
    Heal,
    Poison,
    ManaRestore,
    StaminaRestore,
    StrengthBoost,
    DexterityBoost,
    ConstitutionBoost,
    IntelligenceBoost,
    WisdomBoost,
    CharismaBoost,
    Haste,
    Slow,
    Sleep,
    Blind,
    Invisibility,
    DetectInvisible,
    DetectMagic,
    NightVision,
    Regeneration,
    Drunk,
    Satiated,
    Quenched,
    ArmorClassBoost,
    MagicLight,
    Disguise,
    WaterBreathing,
    DamageReduction,
    Charmed,
    Curse,
    /// Combat blessing — adds `magnitude*3` to hit chance and `(magnitude+1)/2`
    /// to weapon damage. Default magnitude=1 yields +3 hit, +1 damage.
    Bless,
    /// Signed luck modifier — adds `magnitude` (1:1, percentage points) to
    /// player skill rolls (backstab/bash/pick/forage) and subtracts from
    /// incoming `roll_status_application` chance on the target. Negative
    /// magnitude flips both directions. Sourceable from items, spells, or
    /// scripts.
    Luck,
    /// Silence — caster cannot cast spells while active. Hard gate in cast.rhai.
    Silence,
    /// Damage-over-time from direct sunlight exposure. Stacks/refreshes from
    /// the sun tick on `flags.vampire` mobs and PCs with vampire_state.
    SunlightBurn,
    /// Sun-burn unconscious "rescue window" state. Held when the entity hits
    /// 0 HP from SunlightBurn DoT — they're prone, alive, and one more blow
    /// (sun or otherwise) ends them. Cleared by being moved to a sheltered
    /// room before the next sun tick.
    SunlightBurning,
    /// Berserk rage. Combat tick reads this to lock disengage and apply +str.
    /// Triggered by blood = 0 + failed humanity check, witnessed humanity
    /// violation, or fire/sunlight damage.
    Frenzy,
    /// Hard mind control — bypasses `MobileFlags.no_charm`. Source-controlled
    /// via the `order` command, mirrors Charmed semantics otherwise.
    Dominated,
    /// Discipline-grade invisibility that holds even in lit rooms (unlike the
    /// regular Invisibility buff which can be defeated by light/perception).
    Obfuscate,
    /// Typed damage resistance — reduces incoming damage of a specific
    /// `DamageType` by `magnitude` percent. Companion tag in `ItemAffect.damage_type`
    /// / `ActiveBuff.damage_type` selects which damage type. Stacks additively
    /// with racial resistance; clamped `[-100, 95]` in the consumption site.
    DamageResistance,
    /// Status-effect application resistance — when a spell/ability tries to
    /// stamp an `EffectType` on the target, this buff's `magnitude` is
    /// subtracted from the application chance. Companion tag in
    /// `ItemAffect.vs_effect` / `ActiveBuff.vs_effect` is the snake_case name
    /// of the effect being resisted, or `"*"` for "all status effects"
    /// (CircleMUD APPLY_SAVING_SPELL parity).
    StatusResistance,
    /// Flat to-hit bonus while held as an active buff (typically stamped by an
    /// equipped item with `affects: [hit_bonus mag=N]`). Aggregated in the
    /// combat tick's hit_chance computation.
    HitBonus,
    /// Flat weapon damage bonus while held as an active buff. Combat counterpart
    /// to `HitBonus`.
    DamageBonus,
    /// Flat max-HP bonus while held as an active buff. Read by character HP
    /// regen + max-HP queries; legacy `ItemData.max_hp_bonus` migrates here.
    MaxHpBonus,
    /// Flat max-mana bonus while held as an active buff. Legacy
    /// `ItemData.max_mana_bonus` migrates here.
    MaxManaBonus,
}

impl EffectType {
    pub fn from_str(s: &str) -> Option<EffectType> {
        match s.to_lowercase().as_str() {
            "none" => Some(EffectType::None),
            "heal" => Some(EffectType::Heal),
            "poison" => Some(EffectType::Poison),
            "mana_restore" | "manarestore" | "mana" => Some(EffectType::ManaRestore),
            "stamina_restore" | "staminarestore" | "stamina" | "refresh" => Some(EffectType::StaminaRestore),
            "strength_boost" | "strengthboost" | "str_boost" | "strength" => Some(EffectType::StrengthBoost),
            "dexterity_boost" | "dexterityboost" | "dex_boost" | "dexterity" => Some(EffectType::DexterityBoost),
            "constitution_boost" | "constitutionboost" | "con_boost" | "constitution" => {
                Some(EffectType::ConstitutionBoost)
            }
            "intelligence_boost" | "intelligenceboost" | "int_boost" | "intelligence" => {
                Some(EffectType::IntelligenceBoost)
            }
            "wisdom_boost" | "wisdomboost" | "wis_boost" | "wisdom" => Some(EffectType::WisdomBoost),
            "charisma_boost" | "charismaboost" | "cha_boost" | "charisma" => Some(EffectType::CharismaBoost),
            "haste" => Some(EffectType::Haste),
            "slow" => Some(EffectType::Slow),
            "sleep" => Some(EffectType::Sleep),
            "blind" | "blindness" => Some(EffectType::Blind),
            "invisibility" | "invis" => Some(EffectType::Invisibility),
            "detect_invisible" | "detectinvisible" | "detect_invis" | "detect_invisibility"
            | "true_seeing" | "trueseeing" | "true_sight" | "sense_life" | "senselife" => {
                Some(EffectType::DetectInvisible)
            }
            "detect_magic" | "detectmagic" => Some(EffectType::DetectMagic),
            "night_vision" | "nightvision" | "infravision" => Some(EffectType::NightVision),
            "regeneration" | "regen" => Some(EffectType::Regeneration),
            "drunk" => Some(EffectType::Drunk),
            "satiated" => Some(EffectType::Satiated),
            "quenched" => Some(EffectType::Quenched),
            "armor_class_boost" | "armorclassboost" | "ac_boost" | "arcane_shield" | "armor" => {
                Some(EffectType::ArmorClassBoost)
            }
            "magic_light" | "magiclight" | "light" => Some(EffectType::MagicLight),
            "disguise" => Some(EffectType::Disguise),
            "water_breathing" | "waterbreathing" | "aqua_breath" => Some(EffectType::WaterBreathing),
            "damage_reduction" | "damagereduction" | "sanctuary" | "stone_skin" | "stoneskin"
            | "protection_from_evil" | "protection_from_good" => {
                Some(EffectType::DamageReduction)
            }
            "charm" | "charmed" => Some(EffectType::Charmed),
            "curse" | "cursed" => Some(EffectType::Curse),
            "bless" | "blessed" | "blessing" => Some(EffectType::Bless),
            "luck" | "lucky" | "fortune" | "misfortune" => Some(EffectType::Luck),
            "silence" | "silenced" => Some(EffectType::Silence),
            "sunlight_burn" | "sunlightburn" | "sunburn" => Some(EffectType::SunlightBurn),
            "sunlight_burning" | "sunlightburning" | "sun_burning" => {
                Some(EffectType::SunlightBurning)
            }
            "frenzy" | "frenzied" | "berserk" => Some(EffectType::Frenzy),
            "dominated" | "dominate" => Some(EffectType::Dominated),
            "obfuscate" | "obfuscated" => Some(EffectType::Obfuscate),
            "damage_resistance" | "damageresistance" | "resist" => Some(EffectType::DamageResistance),
            "status_resistance" | "statusresistance" | "ward" => Some(EffectType::StatusResistance),
            "hit_bonus" | "hitbonus" | "hitroll" => Some(EffectType::HitBonus),
            "damage_bonus" | "damagebonus" | "damroll" => Some(EffectType::DamageBonus),
            "max_hp_bonus" | "maxhpbonus" | "max_hp" | "maxhit" => Some(EffectType::MaxHpBonus),
            "max_mana_bonus" | "maxmanabonus" | "max_mana" | "maxmana" => Some(EffectType::MaxManaBonus),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            EffectType::None => "none",
            EffectType::Heal => "heal",
            EffectType::Poison => "poison",
            EffectType::ManaRestore => "mana_restore",
            EffectType::StaminaRestore => "stamina_restore",
            EffectType::StrengthBoost => "strength_boost",
            EffectType::DexterityBoost => "dexterity_boost",
            EffectType::ConstitutionBoost => "constitution_boost",
            EffectType::IntelligenceBoost => "intelligence_boost",
            EffectType::WisdomBoost => "wisdom_boost",
            EffectType::CharismaBoost => "charisma_boost",
            EffectType::Haste => "haste",
            EffectType::Slow => "slow",
            EffectType::Sleep => "sleep",
            EffectType::Blind => "blind",
            EffectType::Invisibility => "invisibility",
            EffectType::DetectInvisible => "detect_invisible",
            EffectType::DetectMagic => "detect_magic",
            EffectType::NightVision => "night_vision",
            EffectType::Regeneration => "regeneration",
            EffectType::Drunk => "drunk",
            EffectType::Satiated => "satiated",
            EffectType::Quenched => "quenched",
            EffectType::ArmorClassBoost => "armor_class_boost",
            EffectType::MagicLight => "magic_light",
            EffectType::Disguise => "disguise",
            EffectType::WaterBreathing => "water_breathing",
            EffectType::DamageReduction => "damage_reduction",
            EffectType::Charmed => "charmed",
            EffectType::Curse => "curse",
            EffectType::Bless => "bless",
            EffectType::Luck => "luck",
            EffectType::Silence => "silence",
            EffectType::SunlightBurn => "sunlight_burn",
            EffectType::SunlightBurning => "sunlight_burning",
            EffectType::Frenzy => "frenzy",
            EffectType::Dominated => "dominated",
            EffectType::Obfuscate => "obfuscate",
            EffectType::DamageResistance => "damage_resistance",
            EffectType::StatusResistance => "status_resistance",
            EffectType::HitBonus => "hit_bonus",
            EffectType::DamageBonus => "damage_bonus",
            EffectType::MaxHpBonus => "max_hp_bonus",
            EffectType::MaxManaBonus => "max_mana_bonus",
        }
    }

    pub fn all() -> Vec<&'static str> {
        vec![
            "none",
            "heal",
            "poison",
            "mana_restore",
            "stamina_restore",
            "strength_boost",
            "dexterity_boost",
            "constitution_boost",
            "intelligence_boost",
            "wisdom_boost",
            "charisma_boost",
            "haste",
            "slow",
            "sleep",
            "blind",
            "invisibility",
            "detect_invisible",
            "detect_magic",
            "night_vision",
            "regeneration",
            "drunk",
            "satiated",
            "quenched",
            "armor_class_boost",
            "magic_light",
            "disguise",
            "water_breathing",
            "damage_reduction",
            "charmed",
            "curse",
            "bless",
            "luck",
            "silence",
            "sunlight_burn",
            "sunlight_burning",
            "frenzy",
            "dominated",
            "obfuscate",
            "damage_resistance",
            "status_resistance",
            "hit_bonus",
            "damage_bonus",
            "max_hp_bonus",
            "max_mana_bonus",
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemEffect {
    pub effect_type: EffectType,
    pub magnitude: i32,
    pub duration: i32, // seconds, 0 = instant
    #[serde(default)]
    pub script_callback: Option<String>,
}

impl Default for ItemEffect {
    fn default() -> Self {
        ItemEffect {
            effect_type: EffectType::None,
            magnitude: 0,
            duration: 0,
            script_callback: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveBuff {
    pub effect_type: EffectType,
    pub magnitude: i32,
    pub remaining_secs: i32, // -1 = permanent until dispelled
    pub source: String,      // e.g. "coffee", "healing potion", "item:<uuid>"
    /// Companion tag for `EffectType::DamageResistance`. Identifies which
    /// damage type this resistance reduces. Ignored for all other effect types.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage_type: Option<DamageType>,
    /// Companion tag for `EffectType::StatusResistance`. Snake_case name of the
    /// `EffectType` being resisted, or `"*"` for "all status effects". Ignored
    /// for all other effect types.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vs_effect: Option<String>,
}

impl Default for ActiveBuff {
    fn default() -> Self {
        ActiveBuff {
            effect_type: EffectType::None,
            magnitude: 0,
            remaining_secs: 0,
            source: String::new(),
            damage_type: None,
            vs_effect: None,
        }
    }
}

/// Equip-time effect configuration on an item prototype. When the item is
/// worn, each `ItemAffect` is stamped onto the wearer's `active_buffs` as a
/// permanent `ActiveBuff` sourced as `"item:<item-uuid>"`; on remove, all
/// buffs with that source are stripped. See `src/db.rs::equip_and_stamp_buffs`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ItemAffect {
    pub effect_type: EffectType,
    #[serde(default)]
    pub magnitude: i32,
    /// Required iff `effect_type == DamageResistance`; ignored otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage_type: Option<DamageType>,
    /// Required iff `effect_type == StatusResistance`. Snake_case name of the
    /// `EffectType` being resisted, or `"*"` for "all status effects".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vs_effect: Option<String>,
}

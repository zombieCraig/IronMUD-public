//! Combat-related types for IronMUD

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Combat zone type - determines what combat is allowed in an area/room
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CombatZoneType {
    #[default]
    Pve,   // Default: can attack mobiles, not players
    Safe,  // No combat at all
    Pvp,   // Can attack mobiles and players
}

impl CombatZoneType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "pve" | "normal" | "default" => Some(CombatZoneType::Pve),
            "safe" | "peaceful" | "no_combat" => Some(CombatZoneType::Safe),
            "pvp" | "arena" | "full" => Some(CombatZoneType::Pvp),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            CombatZoneType::Pve => "pve",
            CombatZoneType::Safe => "safe",
            CombatZoneType::Pvp => "pvp",
        }
    }

    /// Returns true if players can attack mobiles in this zone
    pub fn can_attack_mobiles(&self) -> bool {
        match self {
            CombatZoneType::Pve => true,
            CombatZoneType::Safe => false,
            CombatZoneType::Pvp => true,
        }
    }

    /// Returns true if players can attack other players in this zone
    pub fn can_attack_players(&self) -> bool {
        match self {
            CombatZoneType::Pve => false,
            CombatZoneType::Safe => false,
            CombatZoneType::Pvp => true,
        }
    }
}

/// Combat distance states for intra-room tactical positioning
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CombatDistance {
    #[default]
    Melee, // Close combat range
    Pole,  // Reach weapon range (polearms)
    Ranged, // Missile weapon range (bows, guns)
}

impl CombatDistance {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "melee" | "close" => Some(CombatDistance::Melee),
            "pole" | "reach" => Some(CombatDistance::Pole),
            "ranged" | "missile" | "far" => Some(CombatDistance::Ranged),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            CombatDistance::Melee => "melee",
            CombatDistance::Pole => "pole",
            CombatDistance::Ranged => "ranged",
        }
    }

    /// Returns the next closer distance (for advancing)
    pub fn closer(&self) -> Option<Self> {
        match self {
            CombatDistance::Ranged => Some(CombatDistance::Pole),
            CombatDistance::Pole => Some(CombatDistance::Melee),
            CombatDistance::Melee => None, // Already at closest
        }
    }

    /// Returns the next farther distance (for retreating)
    pub fn farther(&self) -> Option<Self> {
        match self {
            CombatDistance::Melee => Some(CombatDistance::Pole),
            CombatDistance::Pole => Some(CombatDistance::Ranged),
            CombatDistance::Ranged => None, // Already at farthest
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CombatTargetType {
    #[default]
    Mobile,
    Player,
}

impl CombatTargetType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "mobile" | "mob" | "npc" => Some(CombatTargetType::Mobile),
            "player" | "character" => Some(CombatTargetType::Player),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            CombatTargetType::Mobile => "mobile",
            CombatTargetType::Player => "player",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatTarget {
    pub target_type: CombatTargetType,
    pub target_id: Uuid,
}

impl Default for CombatTarget {
    fn default() -> Self {
        CombatTarget {
            target_type: CombatTargetType::default(),
            target_id: Uuid::nil(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CombatState {
    #[serde(default)]
    pub in_combat: bool,
    #[serde(default)]
    pub targets: Vec<CombatTarget>,
    #[serde(default)]
    pub stun_rounds_remaining: i32,
    /// Distance to each target (keyed by target UUID)
    #[serde(default)]
    pub distances: HashMap<Uuid, CombatDistance>,
    /// Tracks ammo depletion state: 0=normal, 1=warned (skip round), 2+=unarmed fallback
    #[serde(default, deserialize_with = "deserialize_ammo_depleted")]
    pub ammo_depleted: u8,
    /// Whether character is reloading (costs one combat turn)
    #[serde(default)]
    pub reloading: bool,
}

/// Deserialize ammo_depleted from either bool (legacy) or u8
fn deserialize_ammo_depleted<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;

    struct AmmoDepletedVisitor;

    impl<'de> de::Visitor<'de> for AmmoDepletedVisitor {
        type Value = u8;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a boolean or integer")
        }

        fn visit_bool<E: de::Error>(self, v: bool) -> Result<u8, E> {
            Ok(if v { 1 } else { 0 })
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<u8, E> {
            Ok(v as u8)
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<u8, E> {
            Ok(v as u8)
        }
    }

    deserializer.deserialize_any(AmmoDepletedVisitor)
}

/// Body parts for combat targeting and wound tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum BodyPart {
    Head,
    Neck,
    Torso,
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
    LeftHand,
    RightHand,
    LeftFoot,
    RightFoot,
    LeftEye,
    RightEye,
    LeftEar,
    RightEar,
    Jaw,
}

impl BodyPart {
    /// Returns the hit weight for random targeting (total = 100)
    pub fn hit_weight(&self) -> u32 {
        match self {
            BodyPart::Head => 3,
            BodyPart::Neck => 3,
            BodyPart::Torso => 35,
            BodyPart::LeftArm | BodyPart::RightArm => 12,
            BodyPart::LeftLeg | BodyPart::RightLeg => 12,
            BodyPart::LeftHand | BodyPart::RightHand => 4,
            BodyPart::LeftFoot | BodyPart::RightFoot => 4,
            BodyPart::LeftEye | BodyPart::RightEye => 1,
            BodyPart::LeftEar | BodyPart::RightEar => 1,
            BodyPart::Jaw => 1,
        }
    }

    /// Returns true if this is a vital body part (critical wounds cause instant KO)
    pub fn is_vital(&self) -> bool {
        matches!(self, BodyPart::Head | BodyPart::Neck | BodyPart::Torso)
    }

    /// Returns the parent body part if this is a sub-part (e.g., LeftEye -> Head)
    pub fn parent_part(&self) -> Option<Self> {
        match self {
            BodyPart::LeftEye | BodyPart::RightEye
            | BodyPart::LeftEar | BodyPart::RightEar
            | BodyPart::Jaw => Some(BodyPart::Head),
            _ => None,
        }
    }

    /// Returns true if this body part is a sub-part of the given parent
    pub fn is_sub_part_of(&self, parent: &BodyPart) -> bool {
        self.parent_part().as_ref() == Some(parent)
    }

    /// Returns all body parts
    pub fn all() -> Vec<BodyPart> {
        vec![
            BodyPart::Head,
            BodyPart::Neck,
            BodyPart::Torso,
            BodyPart::LeftArm,
            BodyPart::RightArm,
            BodyPart::LeftLeg,
            BodyPart::RightLeg,
            BodyPart::LeftHand,
            BodyPart::RightHand,
            BodyPart::LeftFoot,
            BodyPart::RightFoot,
            BodyPart::LeftEye,
            BodyPart::RightEye,
            BodyPart::LeftEar,
            BodyPart::RightEar,
            BodyPart::Jaw,
        ]
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().replace([' ', '_'], "").as_str() {
            "head" => Some(BodyPart::Head),
            "neck" => Some(BodyPart::Neck),
            "torso" | "chest" | "body" => Some(BodyPart::Torso),
            "leftarm" | "larm" => Some(BodyPart::LeftArm),
            "rightarm" | "rarm" => Some(BodyPart::RightArm),
            "leftleg" | "lleg" => Some(BodyPart::LeftLeg),
            "rightleg" | "rleg" => Some(BodyPart::RightLeg),
            "lefthand" | "lhand" => Some(BodyPart::LeftHand),
            "righthand" | "rhand" => Some(BodyPart::RightHand),
            "leftfoot" | "lfoot" => Some(BodyPart::LeftFoot),
            "rightfoot" | "rfoot" => Some(BodyPart::RightFoot),
            "lefteye" | "leye" => Some(BodyPart::LeftEye),
            "righteye" | "reye" => Some(BodyPart::RightEye),
            "leftear" | "lear" => Some(BodyPart::LeftEar),
            "rightear" | "rear" => Some(BodyPart::RightEar),
            "jaw" => Some(BodyPart::Jaw),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            BodyPart::Head => "head",
            BodyPart::Neck => "neck",
            BodyPart::Torso => "torso",
            BodyPart::LeftArm => "left arm",
            BodyPart::RightArm => "right arm",
            BodyPart::LeftLeg => "left leg",
            BodyPart::RightLeg => "right leg",
            BodyPart::LeftHand => "left hand",
            BodyPart::RightHand => "right hand",
            BodyPart::LeftFoot => "left foot",
            BodyPart::RightFoot => "right foot",
            BodyPart::LeftEye => "left eye",
            BodyPart::RightEye => "right eye",
            BodyPart::LeftEar => "left ear",
            BodyPart::RightEar => "right ear",
            BodyPart::Jaw => "jaw",
        }
    }
}

/// Wound severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WoundLevel {
    #[default]
    None,
    Minor,      // 10% penalty
    Moderate,   // 25% penalty
    Severe,     // 50% penalty
    Critical,   // 75% penalty
    Disabled,   // 100% penalty
}

impl WoundLevel {
    /// Returns the penalty percentage (0-100) for this wound level
    pub fn penalty(&self) -> i32 {
        match self {
            WoundLevel::None => 0,
            WoundLevel::Minor => 10,
            WoundLevel::Moderate => 25,
            WoundLevel::Severe => 50,
            WoundLevel::Critical => 75,
            WoundLevel::Disabled => 100,
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "none" => Some(WoundLevel::None),
            "minor" => Some(WoundLevel::Minor),
            "moderate" => Some(WoundLevel::Moderate),
            "severe" => Some(WoundLevel::Severe),
            "critical" => Some(WoundLevel::Critical),
            "disabled" => Some(WoundLevel::Disabled),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            WoundLevel::None => "none",
            WoundLevel::Minor => "minor",
            WoundLevel::Moderate => "moderate",
            WoundLevel::Severe => "severe",
            WoundLevel::Critical => "critical",
            WoundLevel::Disabled => "disabled",
        }
    }

    /// Returns the next worse wound level
    pub fn escalate(&self) -> Self {
        match self {
            WoundLevel::None => WoundLevel::Minor,
            WoundLevel::Minor => WoundLevel::Moderate,
            WoundLevel::Moderate => WoundLevel::Severe,
            WoundLevel::Severe => WoundLevel::Critical,
            WoundLevel::Critical => WoundLevel::Disabled,
            WoundLevel::Disabled => WoundLevel::Disabled,
        }
    }
}

/// Type of wound based on damage type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WoundType {
    Cut,        // Slashing damage
    Puncture,   // Piercing damage
    Bruise,     // Bludgeoning (mild)
    Fracture,   // Bludgeoning (severe)
    Burn,       // Fire damage
    Frostbite,  // Cold damage
    Poisoned,   // Poison damage
    Corroded,   // Acid damage
}

impl WoundType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "cut" | "slash" | "laceration" => Some(WoundType::Cut),
            "puncture" | "pierce" | "stab" => Some(WoundType::Puncture),
            "bruise" | "blunt" => Some(WoundType::Bruise),
            "fracture" | "break" | "crush" => Some(WoundType::Fracture),
            "burn" | "fire" | "scorch" => Some(WoundType::Burn),
            "frostbite" | "cold" | "freeze" => Some(WoundType::Frostbite),
            "poisoned" | "poison" | "toxic" => Some(WoundType::Poisoned),
            "corroded" | "acid" | "dissolve" => Some(WoundType::Corroded),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            WoundType::Cut => "cut",
            WoundType::Puncture => "puncture",
            WoundType::Bruise => "bruise",
            WoundType::Fracture => "fracture",
            WoundType::Burn => "burn",
            WoundType::Frostbite => "frostbite",
            WoundType::Poisoned => "poisoned",
            WoundType::Corroded => "corroded",
        }
    }
}

/// An ongoing damage-over-time effect (burn, poison, frostbite, acid)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OngoingEffect {
    pub effect_type: String,       // "fire", "cold", "poison", "acid"
    pub rounds_remaining: i32,
    pub damage_per_round: i32,
    pub body_part: String,         // affected body part display name
}

/// A wound on a body part
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wound {
    pub body_part: BodyPart,
    pub level: WoundLevel,
    pub wound_type: WoundType,
    #[serde(default)]
    pub bleeding_severity: i32,  // 0-5, damage per round from bleeding
}

/// Weapon skill categories for combat
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum WeaponSkill {
    ShortBlades,  // Daggers, knives, shortswords
    LongBlades,   // Swords, longswords, greatswords
    ShortBlunt,   // Clubs, maces, hammers
    LongBlunt,    // Warhammers, staves, mauls
    Polearms,     // Spears, halberds, pikes
    Unarmed,      // Fists, natural weapons
    Ranged,       // Bows, crossbows
}

impl WeaponSkill {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().replace([' ', '_', '-'], "").as_str() {
            "shortblades" | "shortblade" | "dagger" | "knife" => Some(WeaponSkill::ShortBlades),
            "longblades" | "longblade" | "sword" | "longsword" => Some(WeaponSkill::LongBlades),
            "shortblunt" | "club" | "mace" | "hammer" => Some(WeaponSkill::ShortBlunt),
            "longblunt" | "warhammer" | "staff" | "maul" => Some(WeaponSkill::LongBlunt),
            "polearms" | "polearm" | "spear" | "halberd" | "pike" => Some(WeaponSkill::Polearms),
            "unarmed" | "fists" | "fist" | "natural" => Some(WeaponSkill::Unarmed),
            "ranged" | "bow" | "crossbow" | "archery" => Some(WeaponSkill::Ranged),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            WeaponSkill::ShortBlades => "short blades",
            WeaponSkill::LongBlades => "long blades",
            WeaponSkill::ShortBlunt => "short blunt",
            WeaponSkill::LongBlunt => "long blunt",
            WeaponSkill::Polearms => "polearms",
            WeaponSkill::Unarmed => "unarmed",
            WeaponSkill::Ranged => "ranged",
        }
    }

    /// Returns the snake_case key used in the character skills map (e.g., "short_blades")
    pub fn to_skill_key(&self) -> &'static str {
        match self {
            WeaponSkill::ShortBlades => "short_blades",
            WeaponSkill::LongBlades => "long_blades",
            WeaponSkill::ShortBlunt => "short_blunt",
            WeaponSkill::LongBlunt => "long_blunt",
            WeaponSkill::Polearms => "polearms",
            WeaponSkill::Unarmed => "unarmed",
            WeaponSkill::Ranged => "ranged",
        }
    }

    /// Returns all weapon skill types
    pub fn all() -> Vec<WeaponSkill> {
        vec![
            WeaponSkill::ShortBlades,
            WeaponSkill::LongBlades,
            WeaponSkill::ShortBlunt,
            WeaponSkill::LongBlunt,
            WeaponSkill::Polearms,
            WeaponSkill::Unarmed,
            WeaponSkill::Ranged,
        ]
    }

    /// Returns the hit modifier for this weapon at the given combat distance
    /// Positive values are bonuses, negative values are penalties
    pub fn distance_modifier(&self, distance: CombatDistance) -> i32 {
        match (self, distance) {
            // Ranged weapons excel at distance, struggle in melee
            (WeaponSkill::Ranged, CombatDistance::Ranged) => 2,
            (WeaponSkill::Ranged, CombatDistance::Pole) => -2,
            (WeaponSkill::Ranged, CombatDistance::Melee) => -4,
            // Polearms have reach advantage at pole distance
            (WeaponSkill::Polearms, CombatDistance::Ranged) => -2,
            (WeaponSkill::Polearms, CombatDistance::Pole) => 1,
            (WeaponSkill::Polearms, CombatDistance::Melee) => 0,
            // Long blades work best in melee
            (WeaponSkill::LongBlades, CombatDistance::Ranged) => -4,
            (WeaponSkill::LongBlades, CombatDistance::Pole) => -1,
            (WeaponSkill::LongBlades, CombatDistance::Melee) => 0,
            // Short blades excel in close quarters
            (WeaponSkill::ShortBlades, CombatDistance::Ranged) => -6,
            (WeaponSkill::ShortBlades, CombatDistance::Pole) => -3,
            (WeaponSkill::ShortBlades, CombatDistance::Melee) => 1,
            // Short blunt weapons for close combat
            (WeaponSkill::ShortBlunt, CombatDistance::Ranged) => -6,
            (WeaponSkill::ShortBlunt, CombatDistance::Pole) => -3,
            (WeaponSkill::ShortBlunt, CombatDistance::Melee) => 0,
            // Long blunt weapons have some reach
            (WeaponSkill::LongBlunt, CombatDistance::Ranged) => -4,
            (WeaponSkill::LongBlunt, CombatDistance::Pole) => 0,
            (WeaponSkill::LongBlunt, CombatDistance::Melee) => 0,
            // Unarmed needs to be very close
            (WeaponSkill::Unarmed, CombatDistance::Ranged) => -6,
            (WeaponSkill::Unarmed, CombatDistance::Pole) => -4,
            (WeaponSkill::Unarmed, CombatDistance::Melee) => 0,
        }
    }

    /// Returns true if this weapon skill prefers melee combat
    /// (used for mob AI auto-advance behavior)
    pub fn prefers_melee(&self) -> bool {
        !matches!(self, WeaponSkill::Ranged)
    }
}

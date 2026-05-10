//! Mobile (NPC) types: position, flags, memory, and the `MobileData`
//! aggregate with its many subsystems (combat, dialogue, shop, healer,
//! routines, simulation, social, charm, pet).

use super::serde_defaults::default_stat;
use super::{
    ActiveBuff, ActivityState, Characteristics, CombatState, DamageType, DialogueTree, EffectType,
    MobileTrigger, NeedsState, OnHitEffect, OngoingEffect, Relationship, RoutineEntry,
    SimulationConfig, SocialState, TransportRoute, Wound,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MobilePosition {
    #[default]
    Standing,
    Sitting,
    Sleeping,
}

impl std::fmt::Display for MobilePosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MobilePosition::Standing => write!(f, "standing"),
            MobilePosition::Sitting => write!(f, "sitting"),
            MobilePosition::Sleeping => write!(f, "sleeping"),
        }
    }
}

impl MobilePosition {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "standing" | "stand" | "up" => Some(MobilePosition::Standing),
            "sitting" | "sit" => Some(MobilePosition::Sitting),
            "sleeping" | "sleep" | "asleep" => Some(MobilePosition::Sleeping),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MobileFlags {
    #[serde(default)]
    pub aggressive: bool, // Attacks players on sight
    #[serde(default)]
    pub sentinel: bool, // Never wanders from spawn room
    #[serde(default)]
    pub scavenger: bool, // Picks up items from ground
    #[serde(default)]
    pub shopkeeper: bool, // Can buy/sell items
    #[serde(default)]
    pub no_attack: bool, // Cannot be attacked
    #[serde(default)]
    pub healer: bool, // Can provide healing services
    #[serde(default)]
    pub leasing_agent: bool, // Can rent out property templates
    #[serde(default)]
    pub cowardly: bool, // Flees when sniped or HP < 25%
    #[serde(default)]
    pub can_open_doors: bool, // Can open/unlock doors during routine pathfinding
    #[serde(default)]
    pub guard: bool, // Enhanced perception, responds to nearby theft
    #[serde(default)]
    pub helper: bool, // Joins combat to defend faction allies (or any NPC if faction is empty) attacked by a player
    #[serde(default)]
    pub thief: bool, // Steals gold from players
    #[serde(default)]
    pub cant_swim: bool, // Cannot enter water rooms, takes damage if in water
    #[serde(default)]
    pub poisonous: bool, // Melee hits apply a poison DoT
    #[serde(default)]
    pub fiery: bool, // Melee hits apply a fire DoT
    #[serde(default)]
    pub chilling: bool, // Melee hits apply a cold DoT
    #[serde(default)]
    pub corrosive: bool, // Melee hits apply an acid DoT
    #[serde(default)]
    pub shocking: bool, // Melee hits apply a lightning DoT
    #[serde(default)]
    pub unique: bool, // Only one live (non-prototype) instance of this vnum allowed in the world
    #[serde(default)]
    pub stay_zone: bool, // Wandering / pursuit stays inside home_area_id
    #[serde(default)]
    pub aware: bool, // Sees through hidden / sneaking / invisibility
    #[serde(default)]
    pub memory: bool, // Remembers PC attackers and attacks on sight
    #[serde(default)]
    pub no_sleep: bool, // Immune to the sleep spell (CircleMUD MOB_NOSLEEP)
    #[serde(default)]
    pub no_blind: bool, // Immune to the blind spell (CircleMUD MOB_NOBLIND)
    #[serde(default)]
    pub no_bash: bool, // Immune to the bash skill's stun (CircleMUD MOB_NOBASH)
    #[serde(default)]
    pub no_summon: bool, // Immune to the summon spell (CircleMUD MOB_NOSUMMON)
    #[serde(default)]
    pub no_charm: bool, // Immune to the charm spell (CircleMUD MOB_NOCHARM)
    #[serde(default)]
    pub hostile_on_steal: bool, // Attacks the thief when a steal attempt is caught (CircleMUD shop WILL_START_FIGHT)
    #[serde(default)]
    pub tameable: bool, // Casting `charm` on this mob installs a permanent pet bond instead of a temporary buff
    #[serde(default)]
    pub undead: bool, // Generic undead marker (zombies, skeletons, vampires, ghouls). Independent of `vampire`.
    #[serde(default)]
    pub vampire: bool, // Sun-burn tick eligibility, on-hit blood drain, gates `medit vampire` subcommand
    #[serde(default)]
    pub holy_vulnerable: bool, // Doubled incoming Holy damage; covers vampires, demons, holy_vulnerable undead
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RememberedEnemy {
    pub name: String,
    #[serde(default)]
    pub expires_at_secs: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileData {
    // Identity
    pub id: Uuid,
    pub name: String,       // "a goblin guard"
    pub short_desc: String, // "A goblin guard stands here."
    pub long_desc: String,  // Full description
    #[serde(default)]
    pub keywords: Vec<String>, // ["goblin", "guard"]

    /// Owning area for sandbox / permission checks. Orphans (None) are
    /// editable by any builder; once stamped, only `can_edit_area` callers
    /// may mutate or delete the prototype.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area_id: Option<Uuid>,

    // Location
    #[serde(default)]
    pub current_room_id: Option<Uuid>,

    // Prototype system (same as items)
    #[serde(default)]
    pub is_prototype: bool,
    #[serde(default)]
    pub vnum: String, // "goblin_guard"
    // World-wide cap on live (non-prototype) instances of this vnum.
    // None = unlimited. Some(n) = refuse spawn when count >= n.
    // `flags.unique` is sugar for Some(1).
    #[serde(default)]
    pub world_max_count: Option<i32>,

    // Base stats (for future combat)
    #[serde(default)]
    pub level: i32,
    #[serde(default = "default_mobile_hp")]
    pub max_hp: i32,
    #[serde(default = "default_mobile_hp")]
    pub current_hp: i32,
    #[serde(default = "default_mobile_stamina")]
    pub max_stamina: i32,
    #[serde(default = "default_mobile_stamina")]
    pub current_stamina: i32,
    #[serde(default)]
    pub damage_dice: String, // "2d6+3"
    #[serde(default)]
    pub damage_type: DamageType,
    #[serde(default)]
    pub armor_class: i32,
    #[serde(default)]
    pub hit_modifier: i32, // Combat skill modifier based on level
    #[serde(default)]
    pub gold: i32,

    // Attributes
    #[serde(default = "default_stat")]
    pub stat_str: i32,
    #[serde(default = "default_stat")]
    pub stat_dex: i32,
    #[serde(default = "default_stat")]
    pub stat_con: i32,
    #[serde(default = "default_stat")]
    pub stat_int: i32,
    #[serde(default = "default_stat")]
    pub stat_wis: i32,
    #[serde(default = "default_stat")]
    pub stat_cha: i32,

    // AI/Behavior flags
    #[serde(default)]
    pub flags: MobileFlags,

    // Simple dialogue (keyword -> response)
    #[serde(default)]
    pub dialogue: HashMap<String, String>,

    // Branching dialogue tree (overlay; falls back to flat `dialogue` map on miss)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialogue_tree: Option<DialogueTree>,

    // Language this mob speaks. None or a lingua-franca key means everyone
    // hears them in plain text; otherwise listeners' skill in the language
    // governs how garbled the speech sounds. See src/script/lang.rs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spoken_language: Option<String>,

    // Shop system (requires shopkeeper flag)
    #[serde(default)]
    pub shop_stock: Vec<String>, // Vnums for infinite base stock
    #[serde(default)]
    pub shop_inventory: Vec<Uuid>, // Items bought from players (finite)
    #[serde(default = "default_shop_buy_rate")]
    pub shop_buy_rate: i32, // % paid when buying from player (e.g., 50)
    #[serde(default = "default_shop_sell_rate")]
    pub shop_sell_rate: i32, // % charged when selling (e.g., 150)
    #[serde(default = "default_shop_buys_types")]
    pub shop_buys_types: Vec<String>, // Item types this shop buys ("all" = any, empty = none)
    #[serde(default)]
    pub shop_buys_categories: Vec<String>, // Categories this shop buys directly
    #[serde(default)]
    pub shop_preset_vnum: String, // Preset reference (live lookup)
    #[serde(default)]
    pub shop_extra_types: Vec<String>, // Add types beyond preset
    #[serde(default)]
    pub shop_extra_categories: Vec<String>, // Add categories beyond preset
    #[serde(default)]
    pub shop_deny_types: Vec<String>, // Exclude types from preset
    #[serde(default)]
    pub shop_deny_categories: Vec<String>, // Exclude categories from preset
    #[serde(default)]
    pub shop_min_value: i32, // 0 = no minimum
    #[serde(default)]
    pub shop_max_value: i32, // 0 = no maximum

    // Healer system (requires healer flag)
    #[serde(default)]
    pub healer_type: String, // "medic", "herbalist", "cleric"
    #[serde(default)]
    pub healing_free: bool, // Free healing or charges gold?
    #[serde(default = "default_healing_cost_multiplier")]
    pub healing_cost_multiplier: i32, // 100 = base price, 200 = 2x, etc.

    // Trigger system
    #[serde(default)]
    pub triggers: Vec<MobileTrigger>,

    /// Spell IDs the mobile may cast in combat. When non-empty, each combat
    /// round rolls `combat_spell_chance` to cast a random spell from the list
    /// instead of melee. CircleMUD `magic_user` specproc analog.
    #[serde(default)]
    pub combat_spells: Vec<String>,
    /// Per-round percent chance (0-100) to cast from `combat_spells` rather
    /// than swinging. Default 50; ignored when `combat_spells` is empty.
    #[serde(default = "default_combat_spell_chance")]
    pub combat_spell_chance: u8,

    // Transport route (for NPCs that travel)
    #[serde(default)]
    pub transport_route: Option<TransportRoute>,

    // Leasing agent system (requires leasing_agent flag)
    #[serde(default)]
    pub property_templates: Vec<String>, // Vnums of available PropertyTemplates
    #[serde(default)]
    pub leasing_area_id: Option<Uuid>, // Area this agent manages

    // Combat system
    #[serde(default)]
    pub combat: CombatState,
    #[serde(default)]
    pub wounds: Vec<Wound>,
    #[serde(default)]
    pub ongoing_effects: Vec<OngoingEffect>,
    /// Effects rolled per landed natural attack (composes with the legacy
    /// `flags.poisonous/fiery/chilling/corrosive/shocking` DOT flags).
    #[serde(default)]
    pub on_hit_effects: Vec<OnHitEffect>,
    #[serde(default)]
    pub scars: HashMap<String, i32>, // body_part display name -> scar count
    // Death/unconscious state (not persisted)
    #[serde(skip)]
    pub is_unconscious: bool,
    #[serde(skip)]
    pub bleedout_rounds_remaining: i32,

    // Pursuit state (for cross-room sniping response)
    #[serde(default)]
    pub pursuit_target_name: String,
    #[serde(default)]
    pub pursuit_target_room: Option<Uuid>,
    #[serde(default)]
    pub pursuit_direction: String,
    #[serde(default)]
    pub pursuit_certain: bool,

    // Arrow recovery: projectile vnums embedded in this mobile
    #[serde(default)]
    pub embedded_projectiles: Vec<String>,

    // Daily routine system
    #[serde(default)]
    pub daily_routine: Vec<RoutineEntry>,
    #[serde(default)]
    pub schedule_visible: bool,
    #[serde(default)]
    pub current_activity: ActivityState,
    #[serde(default)]
    pub routine_destination_room: Option<Uuid>,
    // Stealth detection (0-10, builder-set)
    #[serde(default)]
    pub perception: i32,

    // NPC needs simulation system
    /// Simulation config - if Some, this mobile uses needs-based simulation instead of routines
    #[serde(default)]
    pub simulation: Option<SimulationConfig>,
    /// Runtime needs state (initialized on first simulation tick)
    #[serde(default)]
    pub needs: Option<NeedsState>,

    // Migrant / emergent population system
    /// Visual/physical characteristics (populated for generated migrants; optional for others).
    #[serde(default)]
    pub characteristics: Option<Characteristics>,
    /// Optional household grouping (reserved for future family/partner mechanics).
    #[serde(default)]
    pub household_id: Option<Uuid>,
    /// Free-form ally tag for the helper system. None/empty falls back to
    /// Circle-stock semantics (any NPC defends any other NPC against a PC).
    /// A tagged faction opts the mob *out* of that generic pool — only
    /// matching tags ally.
    #[serde(default)]
    pub faction: Option<String>,
    /// Declared relationships to other mobiles (future: families, partners, rivals).
    #[serde(default)]
    pub relationships: Vec<Relationship>,
    /// Room vnum this mobile occupies as a claimed resident. Distinct from
    /// SimulationConfig.home_room_vnum so non-simulation mobiles can be housed too.
    #[serde(default)]
    pub resident_of: Option<String>,

    /// Social preferences + happiness. Populated for simulated migrants; None for
    /// static/prototype mobiles.
    #[serde(default)]
    pub social: Option<SocialState>,
    /// Time-limited buffs/debuffs applied to this mobile (mood, etc).
    #[serde(default)]
    pub active_buffs: Vec<ActiveBuff>,
    /// True when this (juvenile) mobile has lost its last living parent and
    /// is awaiting adoption. Set by `db::delete_mobile`; cleared by the
    /// adoption pass in the aging tick. Non-juveniles are never flagged.
    #[serde(default)]
    pub adoption_pending: bool,
    /// Home zone for `MobileFlags.stay_zone`. Stamped at spawn-time from the
    /// destination room's `area_id` when first None.
    #[serde(default)]
    pub home_area_id: Option<Uuid>,
    /// PC names this mobile remembers as enemies (`MobileFlags.memory`).
    /// Capped at MEMORY_CAP, FIFO eviction; entries expire after
    /// MEMORY_DURATION_SECS.
    #[serde(default)]
    pub remembered_enemies: Vec<RememberedEnemy>,
    /// Vampirism state. None = mortal/non-kindred (default for nearly every
    /// mobile). Some = kindred, with blood pool, humanity, masquerade,
    /// frenzy timer. See `crate::types::VampireState`. Independent of
    /// `flags.vampire`/`flags.undead` — those are fast short-circuits used
    /// in hot paths; this carries the rich state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vampire_state: Option<crate::types::VampireState>,
    /// Charmed-mob "stay" override. When true the mob ignores the master
    /// follow propagation and stays put. Set by `order <mob> stay`,
    /// cleared by `order <mob> follow [...]`. Reset on charm break.
    #[serde(default)]
    pub charm_stay: bool,
    /// Charmed-mob alternative leader. When `Some(name)`, the mob follows
    /// that player instead of its charm master. None = follow master
    /// (the default). Set by `order <mob> follow <player>`. Reset on
    /// charm break.
    #[serde(default)]
    pub charm_follow_player: Option<String>,
    /// DG Scripts persistent vars set via `set <var> <value>` while
    /// `context %self.id%` is active, plus any `remote` writes from
    /// other scripts. Empty by default.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub dg_vars: HashMap<String, String>,
    /// Physical stance (independent of `current_activity`, which is the
    /// scheduler concept). Sleeping mobs skip combat turns and emit an
    /// "(asleep)" suffix in room listings. Damage transitions Sleeping →
    /// Sitting (wake-on-damage).
    #[serde(default)]
    pub position: MobilePosition,
    /// When set, this mob is a permanent pet of the named player. Survives
    /// player logout (skipped by `break_all_charms_by_player`) and rides
    /// the same charm-propagation machinery for movement. Cleared by
    /// `pet dismiss`.
    #[serde(default)]
    pub pet_owner: Option<String>,
    /// Owner-set nickname (typically by the pet master via `pet rename`).
    /// When Some, overrides `name` and `short_desc` in room listings,
    /// combat broadcasts, and order/dismiss keyword matches.
    #[serde(default)]
    pub nickname: Option<String>,
}

fn default_mobile_hp() -> i32 {
    10
}

fn default_mobile_stamina() -> i32 {
    50
}

fn default_shop_buy_rate() -> i32 {
    50
}

fn default_shop_sell_rate() -> i32 {
    150
}

fn default_shop_buys_types() -> Vec<String> {
    vec!["all".to_string()]
}

fn default_healing_cost_multiplier() -> i32 {
    100 // Base price, no markup
}

fn default_combat_spell_chance() -> u8 {
    50
}

impl MobileData {
    pub fn new(name: String) -> Self {
        MobileData {
            id: Uuid::new_v4(),
            name: name.clone(),
            short_desc: format!("{} is here.", name),
            long_desc: String::new(),
            keywords: Vec::new(),
            area_id: None,
            current_room_id: None,
            is_prototype: true,
            vnum: String::new(),
            world_max_count: None,
            level: 1,
            max_hp: 10,
            current_hp: 10,
            max_stamina: 50,
            current_stamina: 50,
            damage_dice: "1d4".to_string(),
            damage_type: DamageType::default(),
            armor_class: 10,
            hit_modifier: 0,
            stat_str: 10,
            stat_dex: 10,
            stat_con: 10,
            stat_int: 10,
            stat_wis: 10,
            stat_cha: 10,
            flags: MobileFlags::default(),
            dialogue: HashMap::new(),
            dialogue_tree: None,
            spoken_language: None,
            shop_stock: Vec::new(),
            shop_inventory: Vec::new(),
            shop_buy_rate: 50,
            shop_sell_rate: 150,
            shop_buys_types: vec!["all".to_string()],
            shop_buys_categories: Vec::new(),
            shop_preset_vnum: String::new(),
            shop_extra_types: Vec::new(),
            shop_extra_categories: Vec::new(),
            shop_deny_types: Vec::new(),
            shop_deny_categories: Vec::new(),
            shop_min_value: 0,
            shop_max_value: 0,
            healer_type: String::new(),
            healing_free: false,
            healing_cost_multiplier: 100,
            triggers: Vec::new(),
            combat_spells: Vec::new(),
            combat_spell_chance: default_combat_spell_chance(),
            transport_route: None,
            property_templates: Vec::new(),
            leasing_area_id: None,
            gold: 0,
            combat: CombatState::default(),
            wounds: Vec::new(),
            ongoing_effects: Vec::new(),
            on_hit_effects: Vec::new(),
            scars: HashMap::new(),
            // Death/unconscious state (not persisted)
            is_unconscious: false,
            bleedout_rounds_remaining: 0,
            // Pursuit state
            pursuit_target_name: String::new(),
            pursuit_target_room: None,
            pursuit_direction: String::new(),
            pursuit_certain: false,
            // Arrow recovery
            embedded_projectiles: Vec::new(),
            // Daily routine system
            daily_routine: Vec::new(),
            schedule_visible: false,
            current_activity: ActivityState::default(),
            routine_destination_room: None,
            perception: 0,
            simulation: None,
            needs: None,
            characteristics: None,
            household_id: None,
            faction: None,
            relationships: Vec::new(),
            resident_of: None,
            social: None,
            active_buffs: Vec::new(),
            adoption_pending: false,
            home_area_id: None,
            remembered_enemies: Vec::new(),
            vampire_state: None,
            charm_stay: false,
            charm_follow_player: None,
            dg_vars: HashMap::new(),
            position: MobilePosition::default(),
            pet_owner: None,
            nickname: None,
        }
    }

    /// Display name preference: nickname (if set) overrides `name`. Used by
    /// room listings, combat broadcasts, and the `mob_display_name_for`
    /// viewer-aware helper.
    pub fn display_name(&self) -> &str {
        self.nickname.as_deref().filter(|s| !s.is_empty()).unwrap_or(&self.name)
    }

    /// Player who currently controls this mob, if any. Considers both
    /// `EffectType::Charmed` (regular charm spell, blocked by `no_charm`) and
    /// `EffectType::Dominated` (vampire Dominate discipline, bypasses
    /// `no_charm`). The `order` command and follow-master propagation treat
    /// the two identically — both grant a player full control — so this
    /// helper covers both. Charmed wins ties when both buffs exist (rare
    /// edge case).
    pub fn charm_master(&self) -> Option<&str> {
        if let Some(b) = self
            .active_buffs
            .iter()
            .find(|b| b.effect_type == EffectType::Charmed)
        {
            return Some(b.source.as_str());
        }
        self.active_buffs
            .iter()
            .find(|b| b.effect_type == EffectType::Dominated)
            .map(|b| b.source.as_str())
    }

    pub fn is_charmed_by(&self, player_name: &str) -> bool {
        self.charm_master()
            .map(|m| m.eq_ignore_ascii_case(player_name))
            .unwrap_or(false)
    }

    pub fn is_charmed_by_anyone(&self) -> bool {
        self.charm_master().is_some()
    }

    /// Pronoun gender for DG Scripts. Reads `characteristics.gender` when
    /// set; otherwise resolves as "neuter".
    pub fn resolved_gender(&self) -> &str {
        self.characteristics
            .as_ref()
            .map(|c| c.gender.as_str())
            .filter(|g| !g.is_empty())
            .unwrap_or("neuter")
    }
}

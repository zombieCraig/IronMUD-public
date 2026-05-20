//! Static option lists used by the per-command completers.

/// Directions for movement
pub const DIRECTIONS: &[&str] = &[
    "north",
    "south",
    "east",
    "west",
    "up",
    "down",
    "northeast",
    "northwest",
    "southeast",
    "southwest",
];

/// Skill names for crafting/cooking
pub const SKILL_NAMES: &[&str] = &["cooking", "crafting", "fishing", "foraging", "gardening", "swimming"];

/// Medit subcommands
pub const MEDIT_SUBCOMMANDS: &[&str] = &[
    "name",
    "short",
    "long",
    "keywords",
    "level",
    "hp",
    "damage",
    "ac",
    "damtype",
    "stat",
    "flags",
    "flag",
    "prototype",
    "vnum",
    "dialogue",
    "dialogues",
    "rmdialogue",
    "spawn",
    "shop",
    "healer",
    "trigger",
    "transport",
    "leasing",
    "routine",
    "autostats",
    "gold",
    "perception",
    "simulation",
    "combat_spells",
    "spells",
];

/// Mobile transport route actions
pub const MOBILE_TRANSPORT_ACTIONS: &[&str] = &["set", "fixed", "random", "permanent", "clear"];

/// Mobile flags
pub const MOBILE_FLAGS: &[&str] = &[
    "aggressive",
    "aggro_evil",
    "aggro_good",
    "aggro_neutral",
    "aware",
    "can_open_doors",
    "cant_swim",
    "chilling",
    "corrosive",
    "cowardly",
    "fiery",
    "guard",
    "healer",
    "helper",
    "holy_vulnerable",
    "hostile_on_steal",
    "leasing_agent",
    "memory",
    "no_attack",
    "no_bash",
    "no_blind",
    "no_charm",
    "no_sleep",
    "no_summon",
    "poisonous",
    "scavenger",
    "sentinel",
    "shocking",
    "shopkeeper",
    "stay_zone",
    "tameable",
    "thief",
    "undead",
    "unique",
    "vampire",
];

/// Routine subcommands
pub const ROUTINE_SUBCOMMANDS: &[&str] = &[
    "add", "remove", "clear", "preset", "msg", "wander", "visible", "dialogue",
];

pub const SIMULATION_SUBCOMMANDS: &[&str] = &["setup", "remove", "pay", "hours", "food", "decay"];

/// Activity states for routine entries
pub const ACTIVITY_STATES: &[&str] = &["working", "sleeping", "patrolling", "off_duty", "socializing", "eating"];

/// Shop subcommands
pub const SHOP_SUBCOMMANDS: &[&str] = &[
    "stock",
    "buyrate",
    "sellrate",
    "buys",
    "categories",
    "preset",
    "minvalue",
    "maxvalue",
];

/// Shop categories actions
pub const SHOP_CATEGORIES_ACTIONS: &[&str] = &["add", "remove", "clear"];

/// Shop preset actions
pub const SHOP_PRESET_ACTIONS: &[&str] = &["set", "clear", "extra", "deny"];

/// Bpredit subcommands
pub const BPREDIT_SUBCOMMANDS: &[&str] = &[
    "list", "create", "delete", "name", "desc", "type", "category", "minvalue", "maxvalue",
];

/// Leasing agent subcommands
pub const LEASING_SUBCOMMANDS: &[&str] = &["area", "add", "remove"];

/// Shop stock actions
pub const SHOP_STOCK_ACTIONS: &[&str] = &["add", "remove"];

/// Item flags
pub const ITEM_FLAGS: &[&str] = &[
    "no_drop",
    "no_get",
    "no_remove",
    "invisible",
    "glow",
    "holy",
    "hum",
    "no_sell",
    "unique",
    "quest_item",
    "vending",
    "provides_light",
    "fishing_rod",
    "bait",
    "foraging_tool",
    "waterproof",
    "provides_warmth",
    "reduces_glare",
    "medical_tool",
    "preserves_contents",
    "death_only",
    "atm",
    "plant_pot",
    "lockpick",
    "is_skinned",
    "anti_good",
    "anti_evil",
    "anti_neutral",
];

/// Vending subcommands
pub const VENDING_SUBCOMMANDS: &[&str] = &["stock", "sellrate"];

/// Trigger actions
pub const TRIGGER_ACTIONS: &[&str] = &[
    "list", "add", "remove", "enable", "disable", "chance", "interval", "test", "view",
];

/// Combat spells subcommand actions (medit <id> combat_spells <action>)
pub const COMBAT_SPELLS_ACTIONS: &[&str] = &["add", "remove", "clear", "chance"];

/// Trigger types
pub const TRIGGER_TYPES: &[&str] = &["greet", "attack", "death", "say", "idle", "always", "flee"];

/// Mobile trigger templates
pub const MOBILE_TRIGGER_TEMPLATES: &[&str] = &["@say_greeting", "@say_random", "@emote", "@shout"];

/// Room trigger templates
pub const ROOM_TRIGGER_TEMPLATES: &[&str] = &[
    "@room_message",
    "@time_message",
    "@weather_message",
    "@season_message",
    "@random_message",
];

/// Item trigger templates
pub const ITEM_TRIGGER_TEMPLATES: &[&str] = &["@message", "@random_message", "@block_message"];

/// Oedit subcommands
pub const OEDIT_SUBCOMMANDS: &[&str] = &[
    "name",
    "short",
    "long",
    "keywords",
    "type",
    "wear",
    "ac",
    "protects",
    "weight",
    "value",
    "flags",
    "flag",
    "spawn",
    "damage",
    "damtype",
    "twohanded",
    "wskill",
    "caliber",
    "ammocount",
    "ammobonus",
    "rangedtype",
    "magsize",
    "loadedammo",
    "firemode",
    "firemodes",
    "capacity",
    "closed",
    "locked",
    "key",
    "liquid",
    "fill",
    "empty",
    "liqpoison",
    "liqeffect",
    "clearliqeffects",
    "nutrition",
    "spoil",
    "foodpoison",
    "foodeffect",
    "clearfoodeffects",
    "resetfresh",
    "preservation",
    "spoilage",
    "level",
    "affect",
    "affects",
    "insulation",
    "category",
    "teaches",
    "transport",
    "prototype",
    "vnum",
    "trigger",
    "vending",
    "quality",
    "baituses",
    "weightreduction",
    "medical",
    "noise",
    "plantproto",
    "fertduration",
    "treats",
    "teaches_spell",
    "note",
    "extra",
];

/// Item trigger actions
pub const ITEM_TRIGGER_ACTIONS: &[&str] = &["list", "add", "remove", "enable", "disable", "chance", "test", "view"];

/// Item trigger types
pub const ITEM_TRIGGER_TYPES: &[&str] =
    &["get", "drop", "use", "examine", "on_prompt", "on_wear", "on_remove", "on_wield"];

/// `oedit <vnum> affect` sub-actions.
pub const AFFECT_ACTIONS: &[&str] = &["list", "add", "rm", "clear"];

/// Common EffectType names a builder is likely to put on an item via
/// `oedit <vnum> affect add <effect> ...`. Grouped: stat boosts, combat
/// bonuses, granted abilities, protection, and the cursed-item DoTs.
pub const AFFECT_EFFECT_TYPES: &[&str] = &[
    "strength_boost",
    "dexterity_boost",
    "constitution_boost",
    "intelligence_boost",
    "wisdom_boost",
    "charisma_boost",
    "hit_bonus",
    "damage_bonus",
    "max_hp_bonus",
    "max_mana_bonus",
    "armor_class_boost",
    "night_vision",
    "detect_invisible",
    "detect_magic",
    "water_breathing",
    "regeneration",
    "damage_resistance",
    "status_resistance",
    "damage_reduction",
    "poison",
    "blind",
    "sleep",
    "curse",
    "invisibility",
];

/// Valid `vs_effect` tags after `oedit <vnum> affect add status_resistance ...`.
/// `"*"` is the wildcard meaning "all status effects" (CircleMUD APPLY_SAVING_SPELL parity).
pub const STATUS_RESISTANCE_VS_EFFECTS: &[&str] = &[
    "*",
    "sleep",
    "charmed",
    "blind",
    "curse",
    "poison",
    "silence",
    "slow",
    "frenzy",
    "dominated",
];

/// Item types
pub const ITEM_TYPES: &[&str] = &[
    "armor",
    "weapon",
    "container",
    "liquid_container",
    "food",
    "key",
    "misc",
    "ammunition",
];

/// Damage types
pub const DAMAGE_TYPES: &[&str] = &[
    "arcane",
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
];

/// Ranged weapon types
pub const RANGED_TYPES: &[&str] = &["bow", "crossbow", "firearm", "none"];

/// Fire modes
pub const FIRE_MODES: &[&str] = &["single", "burst", "auto"];

/// Noise levels
pub const NOISE_LEVELS: &[&str] = &["silent", "quiet", "normal", "loud", "clear"];

/// Redit subcommands
pub const REDIT_SUBCOMMANDS: &[&str] = &[
    "show", "title", "desc", "flags", "flag", "zone", "extra", "vnum", "area", "trigger", "door", "seasonal",
    "dynamic", "water", "catch", "capacity", "create",
];

/// Room trigger actions
pub const ROOM_TRIGGER_ACTIONS: &[&str] = &[
    "list", "add", "remove", "enable", "disable", "interval", "chance", "test", "view",
];

/// Room trigger types
pub const ROOM_TRIGGER_TYPES: &[&str] = &[
    "enter",
    "exit",
    "look",
    "periodic",
    "on_time_change",
    "on_weather_change",
    "on_season_change",
    "on_month_change",
];

/// Room flags
pub const ROOM_FLAGS: &[&str] = &[
    "dark",
    "no_mob",
    "indoors",
    "underwater",
    "climate_controlled",
    "always_hot",
    "always_cold",
    "city",
    "no_windows",
    "difficult_terrain",
    "dirt_floor",
    "property_storage",
    "post_office",
    "bank",
    "garden",
    "spawn_point",
    "shallow_water",
    "deep_water",
    "liveable",
    "private",
    "private_room",
    "tunnel",
    "death",
    "no_magic",
    "soundproof",
    "notrack",
];

/// Combat zone types
pub const COMBAT_ZONE_TYPES: &[&str] = &["pve", "safe", "pvp", "inherit"];

/// Water types for fishing
pub const WATER_TYPES: &[&str] = &["none", "freshwater", "saltwater", "magical"];

/// Door subcommands
pub const DOOR_SUBCOMMANDS: &[&str] = &[
    "add", "remove", "name", "desc", "key", "keywords", "open", "close", "lock", "unlock", "sync",
];

/// Extra description actions
pub const EXTRA_DESC_ACTIONS: &[&str] = &["list", "add", "edit", "remove"];

/// Aedit subcommands
pub const AEDIT_SUBCOMMANDS: &[&str] = &[
    "show",
    "name",
    "desc",
    "prefix",
    "theme",
    "levels",
    "owner",
    "permission",
    "trust",
    "untrust",
    "trustees",
    "forage",
    "zone",
    "flags",
    "immigration",
];

/// Permission levels
pub const PERMISSION_LEVELS: &[&str] = &["owner_only", "trusted", "all_builders"];

/// Forage table types
pub const FORAGE_TYPES: &[&str] = &["city", "wilderness", "shallow_water", "deep_water", "underwater"];

/// Forage table actions
pub const FORAGE_ACTIONS: &[&str] = &["list", "add", "remove"];

/// Area flags
pub const AREA_FLAGS: &[&str] = &["climate_controlled"];

/// Area zone types (no inherit option for areas)
pub const AREA_ZONE_TYPES: &[&str] = &["pve", "safe", "pvp"];

/// Immigration subcommands
pub const IMMIGRATION_SUBCOMMANDS: &[&str] = &[
    "on",
    "off",
    "room",
    "namepool",
    "visuals",
    "interval",
    "max",
    "workhours",
    "pay",
    "gold",
    "guardpay",
    "healerpay",
    "scavengerpay",
    "clear_sim",
    "variations",
    "list",
];

/// Spedit subcommands
pub const SPEDIT_SUBCOMMANDS: &[&str] = &[
    "list", "create", "delete", "enable", "disable", "max", "interval", "dep",
];

/// Spedit filter keywords
pub const SPEDIT_FILTERS: &[&str] = &["all", "mobs", "items", "room"];

/// Spedit dep actions
pub const SPEDIT_DEP_ACTIONS: &[&str] = &["list", "add", "remove", "clear"];

/// Spedit dep types (destination types)
pub const SPEDIT_DEP_TYPES: &[&str] = &["inv", "equip", "contain"];

/// Wear slots for equipment
pub const WEAR_SLOTS: &[&str] = &[
    "head",
    "neck",
    "shoulders",
    "torso",
    "back",
    "arms",
    "hands",
    "waist",
    "legs",
    "feet",
    "wrists",
    "ankles",
    "wielded",
    "offhand",
    "ears",
];

/// Spawn entity types
pub const SPAWN_ENTITY_TYPES: &[&str] = &["mobile", "item"];

/// Set command subcommands (available to all users)
pub const SET_SUBCOMMANDS_BASE: &[&str] = &["mxp", "color", "afk", "helpline"];

/// Set command subcommands (builder-only)
pub const SET_SUBCOMMANDS_BUILDER: &[&str] = &["roomflags", "builderdebug"];

/// Set command toggle values
pub const SET_TOGGLE_VALUES: &[&str] = &["on", "off"];

/// Rcopy categories
pub const RCOPY_CATEGORIES: &[&str] = &[
    "all", "flags", "desc", "seasonal", "triggers", "water", "catch", "doors", "extra",
];

/// Cedit (class kit editor) subcommands
pub const CEDIT_SUBCOMMANDS: &[&str] = &["show", "gold", "items"];

/// Cedit items sub-actions
pub const CEDIT_ITEMS_ACTIONS: &[&str] = &["add", "remove", "clear"];

/// Recedit subcommands
pub const RECEDIT_SUBCOMMANDS: &[&str] = &[
    "name",
    "vnum",
    "skill",
    "level",
    "autolearn",
    "difficulty",
    "xp",
    "output",
    "ingredient",
    "tool",
];

/// Recipe ingredient actions
pub const INGREDIENT_ACTIONS: &[&str] = &["list", "add", "remove"];

/// Recipe tool actions
pub const TOOL_ACTIONS: &[&str] = &["list", "add", "remove"];

/// Admin subcommands
pub const ADMIN_SUBCOMMANDS: &[&str] = &[
    "kick",
    "summon",
    "heal",
    "settime",
    "broadcast",
    "shutdown",
    "cancel",
    "god",
    "user",
    "api-key",
    "help",
];

/// Admin user sub-actions
pub const ADMIN_USER_ACTIONS: &[&str] = &[
    "list",
    "info",
    "grant-admin",
    "revoke-admin",
    "grant-builder",
    "revoke-builder",
    "password",
    "delete",
    "help",
];

/// Admin api-key sub-actions
pub const ADMIN_API_KEY_ACTIONS: &[&str] = &["list", "create", "show", "revoke", "enable", "delete", "help"];

/// Recipe tool locations
pub const TOOL_LOCATIONS: &[&str] = &["inv", "inventory", "room", "either"];

/// Recipe skills
pub const RECIPE_SKILLS: &[&str] = &["cooking", "crafting"];

/// Tedit subcommands
pub const TEDIT_SUBCOMMANDS: &[&str] = &[
    "name",
    "vnum",
    "type",
    "interior",
    "traveltime",
    "schedule",
    "stop",
    "connect",
    "disconnect",
    "delete",
    "show",
];

/// Transport types
pub const TRANSPORT_TYPES: &[&str] = &["elevator", "bus", "train", "ferry", "airship"];

/// Transport schedule types
pub const SCHEDULE_TYPES: &[&str] = &["ondemand", "gametime"];

/// Transport stop actions
pub const STOP_ACTIONS: &[&str] = &["list", "add", "remove"];

/// Press targets (at stop)
pub const PRESS_TARGETS: &[&str] = &["button"];

/// Body parts for treat command (primary names)
pub const BODY_PARTS: &[&str] = &[
    "head",
    "neck",
    "torso",
    "leftarm",
    "rightarm",
    "leftleg",
    "rightleg",
    "lefthand",
    "righthand",
    "leftfoot",
    "rightfoot",
    "lefteye",
    "righteye",
    "leftear",
    "rightear",
    "jaw",
];

/// Treatable conditions for treat command
pub const TREATABLE_CONDITIONS: &[&str] = &["illness", "hypothermia", "heat_exhaustion", "heat_stroke"];

/// Combined body parts and conditions for treat command second argument
pub const TREAT_TARGETS: &[&str] = &[
    // Body parts
    "head",
    "neck",
    "torso",
    "leftarm",
    "rightarm",
    "leftleg",
    "rightleg",
    "lefthand",
    "righthand",
    "leftfoot",
    "rightfoot",
    "lefteye",
    "righteye",
    "leftear",
    "rightear",
    "jaw",
    // Conditions
    "illness",
    "hypothermia",
    "heat_exhaustion",
    "heat_stroke",
];

/// Pedit subcommands
pub const PEDIT_SUBCOMMANDS: &[&str] = &[
    "name", "vnum", "desc", "rent", "level", "max", "entrance", "done", "show",
];

/// Property command subcommands
pub const PROPERTY_SUBCOMMANDS: &[&str] = &["access", "trust", "untrust"];

/// Property access levels
pub const PROPERTY_ACCESS_LEVELS: &[&str] = &["none", "visit", "full"];

/// Mail command subcommands
pub const MAIL_SUBCOMMANDS: &[&str] = &["check", "list", "read", "send", "compose", "delete", "reply", "help"];

/// Bank command subcommands
pub const BANK_SUBCOMMANDS: &[&str] = &["balance", "deposit", "withdraw", "help"];

/// Escrow command subcommands
pub const ESCROW_SUBCOMMANDS: &[&str] = &["retrieve"];

/// MOTD command subcommands
pub const MOTD_SUBCOMMANDS: &[&str] = &["show", "edit", "clear", "help"];

/// Bugs command subcommands
pub const BUGS_SUBCOMMANDS: &[&str] = &[
    "list", "read", "approve", "note", "status", "priority", "close", "delete", "help",
];

/// Bug status filter values (for bugs list)
pub const BUG_STATUS_FILTERS: &[&str] = &["open", "closed", "inprogress", "resolved", "all", "unapproved"];

/// Bug priority values
pub const BUG_PRIORITY_VALUES: &[&str] = &["low", "normal", "high", "critical"];

/// Bug status values (for bugs status)
pub const BUG_STATUS_VALUES: &[&str] = &["open", "inprogress", "resolved", "closed"];

/// Plantedit subcommands
pub const PLANTEDIT_SUBCOMMANDS: &[&str] = &[
    "create",
    "list",
    "show",
    "delete",
    "name",
    "seed_vnum",
    "harvest_vnum",
    "category",
    "harvest_min",
    "harvest_max",
    "skill",
    "xp",
    "pest_resistance",
    "multi_harvest",
    "indoor_only",
    "water",
    "season",
    "stage",
    "keyword",
];

/// Plant seasons
pub const PLANT_SEASONS: &[&str] = &["spring", "summer", "autumn", "winter"];

/// Plant season actions
pub const PLANT_SEASON_ACTIONS: &[&str] = &["add", "remove"];

/// Plant stage names (for stage add subcommand)
pub const PLANT_STAGES: &[&str] = &["seed", "sprout", "seedling", "growing", "mature", "flowering"];

/// Plant categories
pub const PLANT_CATEGORIES: &[&str] = &["vegetable", "herb", "flower", "fruit", "grain"];

/// Achedit subcommands
pub const ACHEDIT_SUBCOMMANDS: &[&str] = &[
    "list",
    "create",
    "show",
    "delete",
    "name",
    "desc",
    "description",
    "category",
    "hidden",
    "reward",
    "criterion",
];

/// Achievement categories
pub const ACHIEVEMENT_CATEGORIES: &[&str] = &[
    "skill",
    "combat",
    "crafting",
    "exploration",
    "social",
    "wealth",
    "builder",
];

/// Achievement reward subcommands
pub const ACHIEVEMENT_REWARD_ACTIONS: &[&str] = &["title", "gold", "item"];

/// Achievement criterion subcommands
pub const ACHIEVEMENT_CRITERION_ACTIONS: &[&str] = &["manual", "counter", "skill", "recipe", "lease", "gold"];

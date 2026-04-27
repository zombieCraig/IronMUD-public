//! Tab completion engine for IronMUD
//!
//! Provides context-aware completion for:
//! - Command names
//! - Room vnums (for rgoto, redit, link, etc.)
//! - Item vnums (for oedit, ospawn, etc.)
//! - Mobile vnums (for medit, mspawn, etc.)
//! - Area prefixes (for aedit, spedit, etc.)

use unicode_width::UnicodeWidthStr;

/// Result of a completion request
#[derive(Debug, Clone)]
pub struct CompletionResult {
    /// List of possible completions
    pub completions: Vec<String>,
    /// Common prefix shared by all completions (for auto-complete)
    pub common_prefix: String,
    /// The type of completion being offered
    pub completion_type: CompletionType,
    /// Original partial text that was completed
    pub partial: String,
}

impl CompletionResult {
    pub fn empty() -> Self {
        Self {
            completions: Vec::new(),
            common_prefix: String::new(),
            completion_type: CompletionType::None,
            partial: String::new(),
        }
    }

    pub fn new(completions: Vec<String>, partial: &str, completion_type: CompletionType) -> Self {
        let common_prefix = find_common_prefix(&completions);
        Self {
            completions,
            common_prefix,
            completion_type,
            partial: partial.to_string(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.completions.is_empty()
    }

    pub fn is_unique(&self) -> bool {
        self.completions.len() == 1
    }
}

/// Type of completion being offered
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CompletionType {
    None,
    Command,
    RoomVnum,
    ItemVnum,
    MobileVnum,
    AreaPrefix,
    Direction,
    PlayerName,
    MeditSubcommand,
    TriggerAction,
    TriggerType,
    TriggerScript,
    OeditSubcommand,
    ItemTriggerAction,
    ItemTriggerType,
    ItemType,
    ReditSubcommand,
    RoomTriggerAction,
    RoomTriggerType,
    RoomFlag,
    ExtraDescAction,
    AeditSubcommand,
    PermissionLevel,
    ForageType,
    ForageAction,
    AreaFlag,
    AreaZoneType,
    SpeditSubcommand,
    SpeditFilter,
    SpawnEntityType,
    SpeditDepAction,
    SpeditDepType,
    WearSlot,
    SetSubcommand,
    RcopyCategory,
    SkillName,
    RecipeVnum,
    ReceditSubcommand,
    IngredientAction,
    ToolAction,
    ToolLocation,
    RecipeSkill,
    AdminSubcommand,
    AdminUserAction,
    AdminApiKeyAction,
    TreatTarget,
    BodyPart,
    TreatableCondition,
    TransportVnum,
    TeditSubcommand,
    MobileFlag,
    ShopSubcommand,
    ShopStockAction,
    ItemFlag,
    VendingSubcommand,
    CombatZone,
    WaterType,
    DoorSubcommand,
    TransportType,
    StopAction,
    PressTarget,
    MobileTransportAction,
    PropertyTemplateVnum,
    PropertySubcommand,
    PropertyAccessLevel,
    PeditSubcommand,
    LeasingSubcommand,
    BpreditSubcommand,
    ShopPresetVnum,
    ShopCategoriesAction,
    ShopPresetAction,
    MailSubcommand,
    BankSubcommand,
    EscrowSubcommand,
    MotdSubcommand,
    BugsSubcommand,
    BugStatusFilter,
    BugPriorityValue,
    DamageType,
    RoutineSubcommand,
    SimulationSubcommand,
    ActivityState,
    PlantVnum,
    PlanteditSubcommand,
    PlantSeason,
    PlantStage,
    PlantCategory,
    SpellName,
    SummonTarget,
    ImmigrationSubcommand,
}

/// Context for command argument completion
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArgumentContext {
    /// No specific context - no completion available
    None,
    /// Room vnum argument
    RoomVnum,
    /// Item vnum argument
    ItemVnum,
    /// Mobile vnum argument
    MobileVnum,
    /// Area prefix argument
    AreaPrefix,
    /// Direction (north, south, etc.)
    Direction,
    /// Player name
    PlayerName,
    /// Skill name (cooking, crafting, etc.)
    SkillName,
    /// Recipe vnum argument
    RecipeVnum,
    /// Transport vnum argument
    TransportVnum,
    /// Property template vnum argument
    PropertyTemplateVnum,
    /// Shop preset vnum argument
    ShopPresetVnum,
    /// Plant prototype vnum argument
    PlantVnum,
    /// Spell name argument
    SpellName,
}

/// Get the argument context for a command
pub fn get_argument_context(command: &str) -> ArgumentContext {
    match command.to_lowercase().as_str() {
        // Room vnum commands
        "rgoto" | "redit" | "rdelete" | "link" | "unlink" | "rcopy" => ArgumentContext::RoomVnum,

        // Item vnum commands
        "oedit" | "ospawn" | "idelete" | "orefresh" => ArgumentContext::ItemVnum,

        // Mobile vnum commands
        "medit" | "mspawn" | "mdelete" | "mrefresh" => ArgumentContext::MobileVnum,

        // Area prefix commands
        "aedit" | "adelete" | "spedit" | "areset" | "acreate" => ArgumentContext::AreaPrefix,

        // Direction commands
        "go" | "dig" | "snipe" => ArgumentContext::Direction,

        // Player name commands
        "tell" | "whisper" => ArgumentContext::PlayerName,

        // Skill name commands
        "recipes" => ArgumentContext::SkillName,

        // Recipe vnum commands
        "recedit" | "recdelete" => ArgumentContext::RecipeVnum,

        // Transport vnum commands
        "tedit" => ArgumentContext::TransportVnum,

        // Property template vnum commands
        "pedit" | "pdelete" | "upgrade" | "tour" | "rent" => ArgumentContext::PropertyTemplateVnum,

        // Visit uses player names
        "visit" => ArgumentContext::PlayerName,

        // Shop preset vnum commands
        "bpredit" => ArgumentContext::ShopPresetVnum,

        // Plant vnum commands
        "plantedit" => ArgumentContext::PlantVnum,

        // Spell name commands
        "cast" => ArgumentContext::SpellName,

        _ => ArgumentContext::None,
    }
}

/// Helper: Filter static options by prefix
fn filter_static(options: &[&str], partial: &str, comp_type: CompletionType) -> CompletionResult {
    let matches: Vec<String> = options
        .iter()
        .filter(|s| s.starts_with(partial))
        .map(|s| s.to_string())
        .collect();
    CompletionResult::new(matches, partial, comp_type)
}

/// Helper: Return all static options (no filtering)
fn all_static(options: &[&str], comp_type: CompletionType) -> CompletionResult {
    CompletionResult::new(options.iter().map(|s| s.to_string()).collect(), "", comp_type)
}

/// Helper: Filter dynamic (runtime) options by prefix
fn filter_dynamic(options: &[String], partial: &str, comp_type: CompletionType) -> CompletionResult {
    let matches: Vec<String> = options
        .iter()
        .filter(|v| v.to_lowercase().starts_with(partial))
        .cloned()
        .collect();
    CompletionResult::new(matches, partial, comp_type)
}

/// Helper: Return all dynamic options (no filtering)
fn all_dynamic(options: &[String], comp_type: CompletionType) -> CompletionResult {
    CompletionResult::new(options.to_vec(), "", comp_type)
}

/// Helper: Extract partial from words array
fn get_partial(words: &[&str], completing_word: bool) -> String {
    if completing_word {
        words.last().unwrap_or(&"").to_lowercase()
    } else {
        String::new()
    }
}

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
];

/// Mobile transport route actions
pub const MOBILE_TRANSPORT_ACTIONS: &[&str] = &["set", "fixed", "random", "permanent", "clear"];

/// Mobile flags
pub const MOBILE_FLAGS: &[&str] = &[
    "aggressive",
    "sentinel",
    "scavenger",
    "shopkeeper",
    "no_attack",
    "healer",
    "leasing_agent",
    "cowardly",
    "can_open_doors",
    "guard",
    "thief",
    "cant_swim",
    "poisonous",
    "fiery",
    "chilling",
    "corrosive",
    "shocking",
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
];

/// Vending subcommands
pub const VENDING_SUBCOMMANDS: &[&str] = &["stock", "sellrate"];

/// Trigger actions
pub const TRIGGER_ACTIONS: &[&str] = &[
    "list", "add", "remove", "enable", "disable", "chance", "interval", "test", "view",
];

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
    "stat",
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
];

/// Item trigger actions
pub const ITEM_TRIGGER_ACTIONS: &[&str] = &["list", "add", "remove", "enable", "disable", "chance", "test", "view"];

/// Item trigger types
pub const ITEM_TRIGGER_TYPES: &[&str] = &["get", "drop", "use", "examine", "on_prompt"];

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

/// Complete a partial input line
pub fn complete(
    input: &str,
    cursor_pos: usize,
    available_commands: &[String],
    room_vnums: &[String],
    item_vnums: &[String],
    mobile_vnums: &[String],
    area_prefixes: &[String],
    recipe_vnums: &[String],
    transport_vnums: &[String],
    property_template_vnums: &[String],
    shop_preset_vnums: &[String],
    plant_vnums: &[String],
    spell_names: &[String],
    online_players: &[String],
    is_builder: bool,
) -> CompletionResult {
    // Get the portion of input up to cursor
    let input_to_cursor = if cursor_pos <= input.len() {
        &input[..cursor_pos]
    } else {
        input
    };

    // Split into words
    let words: Vec<&str> = input_to_cursor.split_whitespace().collect();

    // Check if we're completing a word or starting a new one
    let completing_word = !input_to_cursor.is_empty() && !input_to_cursor.ends_with(' ');

    match words.len() {
        0 => {
            // Empty input - return all commands
            CompletionResult::new(available_commands.to_vec(), "", CompletionType::Command)
        }
        1 if completing_word => {
            // Completing first word (command name)
            let partial = words[0].to_lowercase();
            let matches: Vec<String> = available_commands
                .iter()
                .filter(|cmd| cmd.to_lowercase().starts_with(&partial))
                .cloned()
                .collect();
            CompletionResult::new(matches, &partial, CompletionType::Command)
        }
        _ => {
            // Completing an argument
            let command = words[0];
            let context = get_argument_context(command);
            let partial = if completing_word {
                words.last().unwrap_or(&"").to_lowercase()
            } else {
                String::new()
            };

            match context {
                ArgumentContext::RoomVnum => {
                    // For redit, provide context-aware completion (edits current room)
                    if command.to_lowercase() == "redit" {
                        return complete_redit(&words, completing_word);
                    }
                    // For rcopy, provide vnum + category completion
                    if command.to_lowercase() == "rcopy" {
                        return complete_rcopy(&words, completing_word, room_vnums);
                    }
                    // Default room vnum completion for rgoto, rdelete, link, unlink
                    let matches: Vec<String> = room_vnums
                        .iter()
                        .filter(|v| v.to_lowercase().starts_with(&partial))
                        .cloned()
                        .collect();
                    CompletionResult::new(matches, &partial, CompletionType::RoomVnum)
                }
                ArgumentContext::ItemVnum => {
                    // For oedit, provide context-aware completion based on position
                    if command.to_lowercase() == "oedit" {
                        return complete_oedit(&words, completing_word, item_vnums, transport_vnums);
                    }
                    // Default item vnum completion for ospawn, idelete
                    let matches: Vec<String> = item_vnums
                        .iter()
                        .filter(|v| v.to_lowercase().starts_with(&partial))
                        .cloned()
                        .collect();
                    CompletionResult::new(matches, &partial, CompletionType::ItemVnum)
                }
                ArgumentContext::MobileVnum => {
                    // For medit, provide context-aware completion based on position
                    if command.to_lowercase() == "medit" {
                        return complete_medit(
                            &words,
                            completing_word,
                            mobile_vnums,
                            item_vnums,
                            transport_vnums,
                            property_template_vnums,
                            shop_preset_vnums,
                        );
                    }
                    // Default mobile vnum completion for mspawn, mdelete
                    let matches: Vec<String> = mobile_vnums
                        .iter()
                        .filter(|v| v.to_lowercase().starts_with(&partial))
                        .cloned()
                        .collect();
                    CompletionResult::new(matches, &partial, CompletionType::MobileVnum)
                }
                ArgumentContext::AreaPrefix => {
                    // For aedit, provide context-aware completion based on position
                    if command.to_lowercase() == "aedit" {
                        return complete_aedit(&words, completing_word, area_prefixes);
                    }
                    // For spedit, provide context-aware completion based on position
                    if command.to_lowercase() == "spedit" {
                        return complete_spedit(
                            &words,
                            completing_word,
                            area_prefixes,
                            room_vnums,
                            mobile_vnums,
                            item_vnums,
                        );
                    }
                    // Default area prefix completion for adelete, areset, acreate
                    let matches: Vec<String> = area_prefixes
                        .iter()
                        .filter(|v| v.to_lowercase().starts_with(&partial))
                        .cloned()
                        .collect();
                    CompletionResult::new(matches, &partial, CompletionType::AreaPrefix)
                }
                ArgumentContext::Direction => {
                    let matches: Vec<String> = DIRECTIONS
                        .iter()
                        .filter(|d| d.starts_with(&partial))
                        .map(|s| s.to_string())
                        .collect();
                    CompletionResult::new(matches, &partial, CompletionType::Direction)
                }
                ArgumentContext::PlayerName => {
                    let matches: Vec<String> = online_players
                        .iter()
                        .filter(|p| p.to_lowercase().starts_with(&partial))
                        .cloned()
                        .collect();
                    CompletionResult::new(matches, &partial, CompletionType::PlayerName)
                }
                ArgumentContext::SkillName => {
                    let matches: Vec<String> = SKILL_NAMES
                        .iter()
                        .filter(|s| s.starts_with(&partial))
                        .map(|s| s.to_string())
                        .collect();
                    CompletionResult::new(matches, &partial, CompletionType::SkillName)
                }
                ArgumentContext::RecipeVnum => {
                    // For recedit, provide context-aware completion based on position
                    if command.to_lowercase() == "recedit" {
                        return complete_recedit(&words, completing_word, recipe_vnums, item_vnums);
                    }
                    // Default recipe vnum completion for recdelete
                    let matches: Vec<String> = recipe_vnums
                        .iter()
                        .filter(|v| v.to_lowercase().starts_with(&partial))
                        .cloned()
                        .collect();
                    CompletionResult::new(matches, &partial, CompletionType::RecipeVnum)
                }
                ArgumentContext::TransportVnum => {
                    // For tedit, provide context-aware completion based on position
                    return complete_tedit(&words, completing_word, transport_vnums, room_vnums);
                }
                ArgumentContext::PropertyTemplateVnum => {
                    // For pedit, provide context-aware completion based on position
                    if command.to_lowercase() == "pedit" {
                        return complete_pedit(&words, completing_word, property_template_vnums);
                    }
                    // Default property template vnum completion for upgrade, tour, rent
                    let matches: Vec<String> = property_template_vnums
                        .iter()
                        .filter(|v| v.to_lowercase().starts_with(&partial))
                        .cloned()
                        .collect();
                    CompletionResult::new(matches, &partial, CompletionType::PropertyTemplateVnum)
                }
                ArgumentContext::ShopPresetVnum => {
                    return complete_bpredit(&words, completing_word, shop_preset_vnums);
                }
                ArgumentContext::PlantVnum => {
                    return complete_plantedit(&words, completing_word, plant_vnums, item_vnums);
                }
                ArgumentContext::SpellName => {
                    let matches: Vec<String> = spell_names
                        .iter()
                        .filter(|s| s.to_lowercase().starts_with(&partial))
                        .cloned()
                        .collect();
                    CompletionResult::new(matches, &partial, CompletionType::SpellName)
                }
                ArgumentContext::None => {
                    // Handle "set" command specially
                    if command.to_lowercase() == "set" {
                        return complete_set(&words, completing_word, is_builder);
                    }
                    // Handle reclist command
                    if command.to_lowercase() == "reclist" {
                        return complete_reclist(&words, completing_word);
                    }
                    // Handle admin command
                    if command.to_lowercase() == "admin" {
                        return complete_admin(&words, completing_word, online_players);
                    }
                    // Handle treat command
                    if command.to_lowercase() == "treat" {
                        return complete_treat(&words, completing_word, online_players);
                    }
                    // Handle press command
                    if command.to_lowercase() == "press" {
                        return complete_press(&words, completing_word);
                    }
                    // Handle property command
                    if command.to_lowercase() == "property" {
                        return complete_property(&words, completing_word, online_players);
                    }
                    // Handle mail command
                    if command.to_lowercase() == "mail" {
                        return complete_mail(&words, completing_word, online_players);
                    }
                    // Handle bank command
                    if command.to_lowercase() == "bank" {
                        return complete_bank(&words, completing_word);
                    }
                    // Handle escrow command
                    if command.to_lowercase() == "escrow" {
                        return complete_escrow(&words, completing_word);
                    }
                    // Handle motd command
                    if command.to_lowercase() == "motd" {
                        return complete_motd(&words, completing_word);
                    }
                    // Handle bugs command
                    if command.to_lowercase() == "bugs" {
                        return complete_bugs(&words, completing_word);
                    }
                    // Handle summon command
                    if command.to_lowercase() == "summon" {
                        return complete_summon(&words, completing_word, mobile_vnums, online_players, room_vnums);
                    }
                    CompletionResult::empty()
                }
            }
        }
    }
}

/// Context-aware completion for medit command
fn complete_medit(
    words: &[&str],
    completing_word: bool,
    mobile_vnums: &[String],
    item_vnums: &[String],
    transport_vnums: &[String],
    property_template_vnums: &[String],
    shop_preset_vnums: &[String],
) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // medit <partial_vnum> - complete vnum
        2 if completing_word => filter_dynamic(mobile_vnums, &partial, CompletionType::MobileVnum),
        // medit <vnum> - show all subcommands
        2 if !completing_word => all_static(MEDIT_SUBCOMMANDS, CompletionType::MeditSubcommand),
        // medit <vnum> <partial_subcmd> - complete subcommand
        3 if completing_word => filter_static(MEDIT_SUBCOMMANDS, &partial, CompletionType::MeditSubcommand),
        // medit <vnum> trigger - show all trigger actions
        3 if !completing_word && words[2].to_lowercase() == "trigger" => {
            all_static(TRIGGER_ACTIONS, CompletionType::TriggerAction)
        }
        // medit <vnum> trigger <partial_action> - complete trigger action
        4 if completing_word && words[2].to_lowercase() == "trigger" => {
            filter_static(TRIGGER_ACTIONS, &partial, CompletionType::TriggerAction)
        }
        // medit <vnum> trigger add - show all trigger types
        4 if !completing_word && words[2].to_lowercase() == "trigger" && words[3].to_lowercase() == "add" => {
            all_static(TRIGGER_TYPES, CompletionType::TriggerType)
        }
        // medit <vnum> trigger add <partial_type> - complete trigger type
        5 if completing_word && words[2].to_lowercase() == "trigger" && words[3].to_lowercase() == "add" => {
            filter_static(TRIGGER_TYPES, &partial, CompletionType::TriggerType)
        }
        // medit <vnum> trigger add <type> - show all templates
        5 if !completing_word && words[2].to_lowercase() == "trigger" && words[3].to_lowercase() == "add" => {
            all_static(MOBILE_TRIGGER_TEMPLATES, CompletionType::TriggerScript)
        }
        // medit <vnum> trigger add <type> <partial_script> - complete template/script
        6 if completing_word && words[2].to_lowercase() == "trigger" && words[3].to_lowercase() == "add" => {
            filter_static(MOBILE_TRIGGER_TEMPLATES, &partial, CompletionType::TriggerScript)
        }
        // medit <vnum> transport - show all transport actions
        3 if !completing_word && words[2].to_lowercase() == "transport" => {
            all_static(MOBILE_TRANSPORT_ACTIONS, CompletionType::MobileTransportAction)
        }
        // medit <vnum> transport <partial_action> - complete transport action
        4 if completing_word && words[2].to_lowercase() == "transport" => filter_static(
            MOBILE_TRANSPORT_ACTIONS,
            &partial,
            CompletionType::MobileTransportAction,
        ),
        // medit <vnum> transport set - show all transport vnums
        4 if !completing_word && words[2].to_lowercase() == "transport" && words[3].to_lowercase() == "set" => {
            all_dynamic(transport_vnums, CompletionType::TransportVnum)
        }
        // medit <vnum> transport set <partial_vnum> - complete transport vnum
        5 if completing_word && words[2].to_lowercase() == "transport" && words[3].to_lowercase() == "set" => {
            filter_dynamic(transport_vnums, &partial, CompletionType::TransportVnum)
        }
        // medit <vnum> flag - show all mobile flags
        3 if !completing_word && words[2].to_lowercase() == "flag" => {
            all_static(MOBILE_FLAGS, CompletionType::MobileFlag)
        }
        // medit <vnum> flag <partial_flag> - complete flag name
        4 if completing_word && words[2].to_lowercase() == "flag" => {
            filter_static(MOBILE_FLAGS, &partial, CompletionType::MobileFlag)
        }
        // medit <vnum> shop - show all shop subcommands
        3 if !completing_word && words[2].to_lowercase() == "shop" => {
            all_static(SHOP_SUBCOMMANDS, CompletionType::ShopSubcommand)
        }
        // medit <vnum> shop <partial_subcmd> - complete shop subcommand
        4 if completing_word && words[2].to_lowercase() == "shop" => {
            filter_static(SHOP_SUBCOMMANDS, &partial, CompletionType::ShopSubcommand)
        }
        // medit <vnum> shop stock - show stock actions
        4 if !completing_word && words[2].to_lowercase() == "shop" && words[3].to_lowercase() == "stock" => {
            all_static(SHOP_STOCK_ACTIONS, CompletionType::ShopStockAction)
        }
        // medit <vnum> shop stock <partial_action> - complete stock action
        5 if completing_word && words[2].to_lowercase() == "shop" && words[3].to_lowercase() == "stock" => {
            filter_static(SHOP_STOCK_ACTIONS, &partial, CompletionType::ShopStockAction)
        }
        // medit <vnum> shop stock add - show item vnums
        5 if !completing_word
            && words[2].to_lowercase() == "shop"
            && words[3].to_lowercase() == "stock"
            && words[4].to_lowercase() == "add" =>
        {
            all_dynamic(item_vnums, CompletionType::ItemVnum)
        }
        // medit <vnum> shop stock add <partial_vnum> - complete item vnum
        6 if completing_word
            && words[2].to_lowercase() == "shop"
            && words[3].to_lowercase() == "stock"
            && words[4].to_lowercase() == "add" =>
        {
            filter_dynamic(item_vnums, &partial, CompletionType::ItemVnum)
        }
        // medit <vnum> shop categories - show categories actions
        4 if !completing_word && words[2].to_lowercase() == "shop" && words[3].to_lowercase() == "categories" => {
            all_static(SHOP_CATEGORIES_ACTIONS, CompletionType::ShopCategoriesAction)
        }
        // medit <vnum> shop categories <partial_action>
        5 if completing_word && words[2].to_lowercase() == "shop" && words[3].to_lowercase() == "categories" => {
            filter_static(SHOP_CATEGORIES_ACTIONS, &partial, CompletionType::ShopCategoriesAction)
        }
        // medit <vnum> shop preset - show preset actions
        4 if !completing_word && words[2].to_lowercase() == "shop" && words[3].to_lowercase() == "preset" => {
            all_static(SHOP_PRESET_ACTIONS, CompletionType::ShopPresetAction)
        }
        // medit <vnum> shop preset <partial_action>
        5 if completing_word && words[2].to_lowercase() == "shop" && words[3].to_lowercase() == "preset" => {
            filter_static(SHOP_PRESET_ACTIONS, &partial, CompletionType::ShopPresetAction)
        }
        // medit <vnum> shop preset set - show preset vnums
        5 if !completing_word
            && words[2].to_lowercase() == "shop"
            && words[3].to_lowercase() == "preset"
            && words[4].to_lowercase() == "set" =>
        {
            all_dynamic(shop_preset_vnums, CompletionType::ShopPresetVnum)
        }
        // medit <vnum> shop preset set <partial_vnum>
        6 if completing_word
            && words[2].to_lowercase() == "shop"
            && words[3].to_lowercase() == "preset"
            && words[4].to_lowercase() == "set" =>
        {
            filter_dynamic(shop_preset_vnums, &partial, CompletionType::ShopPresetVnum)
        }
        // medit <vnum> leasing - show all leasing subcommands
        3 if !completing_word && words[2].to_lowercase() == "leasing" => {
            all_static(LEASING_SUBCOMMANDS, CompletionType::LeasingSubcommand)
        }
        // medit <vnum> leasing <partial_subcmd> - complete leasing subcommand
        4 if completing_word && words[2].to_lowercase() == "leasing" => {
            filter_static(LEASING_SUBCOMMANDS, &partial, CompletionType::LeasingSubcommand)
        }
        // medit <vnum> leasing add - show property template vnums
        4 if !completing_word && words[2].to_lowercase() == "leasing" && words[3].to_lowercase() == "add" => {
            all_dynamic(property_template_vnums, CompletionType::PropertyTemplateVnum)
        }
        // medit <vnum> leasing add <partial_vnum> - complete property template vnum
        5 if completing_word && words[2].to_lowercase() == "leasing" && words[3].to_lowercase() == "add" => {
            filter_dynamic(property_template_vnums, &partial, CompletionType::PropertyTemplateVnum)
        }
        // medit <vnum> leasing remove - show property template vnums
        4 if !completing_word && words[2].to_lowercase() == "leasing" && words[3].to_lowercase() == "remove" => {
            all_dynamic(property_template_vnums, CompletionType::PropertyTemplateVnum)
        }
        // medit <vnum> leasing remove <partial_vnum> - complete property template vnum
        5 if completing_word && words[2].to_lowercase() == "leasing" && words[3].to_lowercase() == "remove" => {
            filter_dynamic(property_template_vnums, &partial, CompletionType::PropertyTemplateVnum)
        }
        // medit <vnum> damtype - show all damage types
        3 if !completing_word && words[2].to_lowercase() == "damtype" => {
            all_static(DAMAGE_TYPES, CompletionType::DamageType)
        }
        // medit <vnum> damtype <partial_type> - complete damage type
        4 if completing_word && words[2].to_lowercase() == "damtype" => {
            filter_static(DAMAGE_TYPES, &partial, CompletionType::DamageType)
        }
        // medit <vnum> simulation - show all simulation subcommands
        3 if !completing_word && (words[2].to_lowercase() == "simulation" || words[2].to_lowercase() == "sim") => {
            all_static(SIMULATION_SUBCOMMANDS, CompletionType::SimulationSubcommand)
        }
        // medit <vnum> simulation <partial_subcmd> - complete simulation subcommand
        4 if completing_word && (words[2].to_lowercase() == "simulation" || words[2].to_lowercase() == "sim") => {
            filter_static(SIMULATION_SUBCOMMANDS, &partial, CompletionType::SimulationSubcommand)
        }
        // medit <vnum> routine - show all routine subcommands
        3 if !completing_word && words[2].to_lowercase() == "routine" => {
            all_static(ROUTINE_SUBCOMMANDS, CompletionType::RoutineSubcommand)
        }
        // medit <vnum> routine <partial_subcmd> - complete routine subcommand
        4 if completing_word && words[2].to_lowercase() == "routine" => {
            filter_static(ROUTINE_SUBCOMMANDS, &partial, CompletionType::RoutineSubcommand)
        }
        // medit <vnum> routine add <hour> - show activity states
        5 if !completing_word && words[2].to_lowercase() == "routine" && words[3].to_lowercase() == "add" => {
            all_static(ACTIVITY_STATES, CompletionType::ActivityState)
        }
        // medit <vnum> routine add <hour> <partial_activity> - complete activity state
        6 if completing_word && words[2].to_lowercase() == "routine" && words[3].to_lowercase() == "add" => {
            filter_static(ACTIVITY_STATES, &partial, CompletionType::ActivityState)
        }
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for oedit command
fn complete_oedit(
    words: &[&str],
    completing_word: bool,
    item_vnums: &[String],
    transport_vnums: &[String],
) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // oedit <partial_vnum> - complete vnum
        2 if completing_word => filter_dynamic(item_vnums, &partial, CompletionType::ItemVnum),
        // oedit <vnum> - show all subcommands
        2 if !completing_word => all_static(OEDIT_SUBCOMMANDS, CompletionType::OeditSubcommand),
        // oedit <vnum> <partial_subcmd> - complete subcommand
        3 if completing_word => filter_static(OEDIT_SUBCOMMANDS, &partial, CompletionType::OeditSubcommand),
        // oedit <vnum> type - show all item types
        3 if !completing_word && words[2].to_lowercase() == "type" => all_static(ITEM_TYPES, CompletionType::ItemType),
        // oedit <vnum> type <partial_type> - complete item type
        4 if completing_word && words[2].to_lowercase() == "type" => {
            filter_static(ITEM_TYPES, &partial, CompletionType::ItemType)
        }
        // oedit <vnum> trigger - show all trigger actions
        3 if !completing_word && words[2].to_lowercase() == "trigger" => {
            all_static(ITEM_TRIGGER_ACTIONS, CompletionType::ItemTriggerAction)
        }
        // oedit <vnum> trigger <partial_action> - complete trigger action
        4 if completing_word && words[2].to_lowercase() == "trigger" => {
            filter_static(ITEM_TRIGGER_ACTIONS, &partial, CompletionType::ItemTriggerAction)
        }
        // oedit <vnum> trigger add - show all trigger types
        4 if !completing_word && words[2].to_lowercase() == "trigger" && words[3].to_lowercase() == "add" => {
            all_static(ITEM_TRIGGER_TYPES, CompletionType::ItemTriggerType)
        }
        // oedit <vnum> trigger add <partial_type> - complete trigger type
        5 if completing_word && words[2].to_lowercase() == "trigger" && words[3].to_lowercase() == "add" => {
            filter_static(ITEM_TRIGGER_TYPES, &partial, CompletionType::ItemTriggerType)
        }
        // oedit <vnum> trigger add <type> - show all templates
        5 if !completing_word && words[2].to_lowercase() == "trigger" && words[3].to_lowercase() == "add" => {
            all_static(ITEM_TRIGGER_TEMPLATES, CompletionType::TriggerScript)
        }
        // oedit <vnum> trigger add <type> <partial_script> - complete template/script
        6 if completing_word && words[2].to_lowercase() == "trigger" && words[3].to_lowercase() == "add" => {
            filter_static(ITEM_TRIGGER_TEMPLATES, &partial, CompletionType::TriggerScript)
        }
        // oedit <vnum> transport - show all transport vnums + clear
        3 if !completing_word && words[2].to_lowercase() == "transport" => {
            let mut matches: Vec<String> = transport_vnums.to_vec();
            matches.push("clear".to_string());
            CompletionResult::new(matches, "", CompletionType::TransportVnum)
        }
        // oedit <vnum> transport <partial> - complete transport vnum + clear
        4 if completing_word && words[2].to_lowercase() == "transport" => {
            let mut matches: Vec<String> = transport_vnums
                .iter()
                .filter(|v| v.to_lowercase().starts_with(&partial))
                .cloned()
                .collect();
            if "clear".starts_with(&partial) {
                matches.push("clear".to_string());
            }
            CompletionResult::new(matches, &partial, CompletionType::TransportVnum)
        }
        // oedit <vnum> flag - show all item flags
        3 if !completing_word && words[2].to_lowercase() == "flag" => all_static(ITEM_FLAGS, CompletionType::ItemFlag),
        // oedit <vnum> flag <partial_flag> - complete flag name
        4 if completing_word && words[2].to_lowercase() == "flag" => {
            filter_static(ITEM_FLAGS, &partial, CompletionType::ItemFlag)
        }
        // oedit <vnum> vending - show vending subcommands
        3 if !completing_word && words[2].to_lowercase() == "vending" => {
            all_static(VENDING_SUBCOMMANDS, CompletionType::VendingSubcommand)
        }
        // oedit <vnum> vending <partial_subcmd> - complete vending subcommand
        4 if completing_word && words[2].to_lowercase() == "vending" => {
            filter_static(VENDING_SUBCOMMANDS, &partial, CompletionType::VendingSubcommand)
        }
        // oedit <vnum> vending stock - show stock actions
        4 if !completing_word && words[2].to_lowercase() == "vending" && words[3].to_lowercase() == "stock" => {
            all_static(SHOP_STOCK_ACTIONS, CompletionType::ShopStockAction)
        }
        // oedit <vnum> vending stock <partial_action> - complete stock action
        5 if completing_word && words[2].to_lowercase() == "vending" && words[3].to_lowercase() == "stock" => {
            filter_static(SHOP_STOCK_ACTIONS, &partial, CompletionType::ShopStockAction)
        }
        // oedit <vnum> vending stock add - show item vnums
        5 if !completing_word
            && words[2].to_lowercase() == "vending"
            && words[3].to_lowercase() == "stock"
            && words[4].to_lowercase() == "add" =>
        {
            all_dynamic(item_vnums, CompletionType::ItemVnum)
        }
        // oedit <vnum> vending stock add <partial_vnum> - complete item vnum
        6 if completing_word
            && words[2].to_lowercase() == "vending"
            && words[3].to_lowercase() == "stock"
            && words[4].to_lowercase() == "add" =>
        {
            filter_dynamic(item_vnums, &partial, CompletionType::ItemVnum)
        }
        // oedit <vnum> damtype - show all damage types
        3 if !completing_word && words[2].to_lowercase() == "damtype" => {
            all_static(DAMAGE_TYPES, CompletionType::DamageType)
        }
        // oedit <vnum> damtype <partial_type> - complete damage type
        4 if completing_word && words[2].to_lowercase() == "damtype" => {
            filter_static(DAMAGE_TYPES, &partial, CompletionType::DamageType)
        }
        // oedit <vnum> rangedtype - show all ranged types
        3 if !completing_word && (words[2].to_lowercase() == "rangedtype" || words[2].to_lowercase() == "rtype") => {
            all_static(RANGED_TYPES, CompletionType::OeditSubcommand)
        }
        // oedit <vnum> rangedtype <partial> - complete ranged type
        4 if completing_word && (words[2].to_lowercase() == "rangedtype" || words[2].to_lowercase() == "rtype") => {
            filter_static(RANGED_TYPES, &partial, CompletionType::OeditSubcommand)
        }
        // oedit <vnum> firemode - show all fire modes
        3 if !completing_word && words[2].to_lowercase() == "firemode" => {
            all_static(FIRE_MODES, CompletionType::OeditSubcommand)
        }
        // oedit <vnum> firemode <partial> - complete fire mode
        4 if completing_word && words[2].to_lowercase() == "firemode" => {
            filter_static(FIRE_MODES, &partial, CompletionType::OeditSubcommand)
        }
        // oedit <vnum> noise - show all noise levels
        3 if !completing_word && (words[2].to_lowercase() == "noise" || words[2].to_lowercase() == "noiselevel") => {
            all_static(NOISE_LEVELS, CompletionType::OeditSubcommand)
        }
        // oedit <vnum> noise <partial> - complete noise level
        4 if completing_word && (words[2].to_lowercase() == "noise" || words[2].to_lowercase() == "noiselevel") => {
            filter_static(NOISE_LEVELS, &partial, CompletionType::OeditSubcommand)
        }
        // oedit <vnum> firemodes - show all fire modes (multi-select)
        3 if !completing_word && words[2].to_lowercase() == "firemodes" => {
            all_static(FIRE_MODES, CompletionType::OeditSubcommand)
        }
        // oedit <vnum> firemodes <partial> - complete fire modes
        4.. if completing_word && words[2].to_lowercase() == "firemodes" => {
            filter_static(FIRE_MODES, &partial, CompletionType::OeditSubcommand)
        }
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for redit command (edits current room)
fn complete_redit(words: &[&str], completing_word: bool) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // redit - show all subcommands
        1 if !completing_word => all_static(REDIT_SUBCOMMANDS, CompletionType::ReditSubcommand),
        // redit <partial_subcmd> - complete subcommand
        2 if completing_word => filter_static(REDIT_SUBCOMMANDS, &partial, CompletionType::ReditSubcommand),
        // redit flag - show all flags
        2 if !completing_word && words[1].to_lowercase() == "flag" => all_static(ROOM_FLAGS, CompletionType::RoomFlag),
        // redit flag <partial_flag> - complete flag name
        3 if completing_word && words[1].to_lowercase() == "flag" => {
            filter_static(ROOM_FLAGS, &partial, CompletionType::RoomFlag)
        }
        // redit zone - show combat zone types
        2 if !completing_word && words[1].to_lowercase() == "zone" => {
            all_static(COMBAT_ZONE_TYPES, CompletionType::CombatZone)
        }
        // redit zone <partial_type> - complete zone type
        3 if completing_word && words[1].to_lowercase() == "zone" => {
            filter_static(COMBAT_ZONE_TYPES, &partial, CompletionType::CombatZone)
        }
        // redit water - show water types
        2 if !completing_word && words[1].to_lowercase() == "water" => {
            all_static(WATER_TYPES, CompletionType::WaterType)
        }
        // redit water <partial_type> - complete water type
        3 if completing_word && words[1].to_lowercase() == "water" => {
            filter_static(WATER_TYPES, &partial, CompletionType::WaterType)
        }
        // redit door - show door subcommands
        2 if !completing_word && words[1].to_lowercase() == "door" => {
            all_static(DOOR_SUBCOMMANDS, CompletionType::DoorSubcommand)
        }
        // redit door <partial_subcmd> - complete door subcommand
        3 if completing_word && words[1].to_lowercase() == "door" => {
            filter_static(DOOR_SUBCOMMANDS, &partial, CompletionType::DoorSubcommand)
        }
        // redit door <subcmd> - show directions (for subcommands that take direction)
        3 if !completing_word && words[1].to_lowercase() == "door" => all_static(DIRECTIONS, CompletionType::Direction),
        // redit door <subcmd> <partial_dir> - complete direction
        4 if completing_word && words[1].to_lowercase() == "door" => {
            filter_static(DIRECTIONS, &partial, CompletionType::Direction)
        }
        // redit extra - show extra actions
        2 if !completing_word && words[1].to_lowercase() == "extra" => {
            all_static(EXTRA_DESC_ACTIONS, CompletionType::ExtraDescAction)
        }
        // redit extra <partial_action> - complete extra action
        3 if completing_word && words[1].to_lowercase() == "extra" => {
            filter_static(EXTRA_DESC_ACTIONS, &partial, CompletionType::ExtraDescAction)
        }
        // redit trigger - show trigger actions
        2 if !completing_word && words[1].to_lowercase() == "trigger" => {
            all_static(ROOM_TRIGGER_ACTIONS, CompletionType::RoomTriggerAction)
        }
        // redit trigger <partial_action> - complete trigger action
        3 if completing_word && words[1].to_lowercase() == "trigger" => {
            filter_static(ROOM_TRIGGER_ACTIONS, &partial, CompletionType::RoomTriggerAction)
        }
        // redit trigger add - show trigger types
        3 if !completing_word && words[1].to_lowercase() == "trigger" && words[2].to_lowercase() == "add" => {
            all_static(ROOM_TRIGGER_TYPES, CompletionType::RoomTriggerType)
        }
        // redit trigger add <partial_type> - complete trigger type
        4 if completing_word && words[1].to_lowercase() == "trigger" && words[2].to_lowercase() == "add" => {
            filter_static(ROOM_TRIGGER_TYPES, &partial, CompletionType::RoomTriggerType)
        }
        // redit trigger add <type> - show all templates
        4 if !completing_word && words[1].to_lowercase() == "trigger" && words[2].to_lowercase() == "add" => {
            all_static(ROOM_TRIGGER_TEMPLATES, CompletionType::TriggerScript)
        }
        // redit trigger add <type> <partial_script> - complete template/script
        5 if completing_word && words[1].to_lowercase() == "trigger" && words[2].to_lowercase() == "add" => {
            filter_static(ROOM_TRIGGER_TEMPLATES, &partial, CompletionType::TriggerScript)
        }
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for rcopy command
fn complete_rcopy(words: &[&str], completing_word: bool, room_vnums: &[String]) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // rcopy <partial_vnum> - complete room vnum
        2 if completing_word => filter_dynamic(room_vnums, &partial, CompletionType::RoomVnum),
        // rcopy <vnum> - show categories
        2 if !completing_word => all_static(RCOPY_CATEGORIES, CompletionType::RcopyCategory),
        // rcopy <vnum> <partial_category> - complete category
        3 if completing_word => filter_static(RCOPY_CATEGORIES, &partial, CompletionType::RcopyCategory),
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for aedit command
fn complete_aedit(words: &[&str], completing_word: bool, area_prefixes: &[String]) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // aedit <partial> - complete to either an area prefix or a subcommand
        // (aedit.rhai accepts a leading subcommand and defaults area to current room)
        2 if completing_word => {
            let mut combined: Vec<String> = AEDIT_SUBCOMMANDS
                .iter()
                .filter(|s| s.starts_with(partial.as_str()))
                .map(|s| s.to_string())
                .collect();
            combined.extend(
                area_prefixes
                    .iter()
                    .filter(|p| p.to_lowercase().starts_with(partial.as_str()))
                    .cloned(),
            );
            CompletionResult::new(combined, &partial, CompletionType::AeditSubcommand)
        }
        // aedit immigration - show immigration subcommands (no area prefix; area inferred from current room)
        2 if !completing_word && words[1].to_lowercase() == "immigration" => {
            all_static(IMMIGRATION_SUBCOMMANDS, CompletionType::ImmigrationSubcommand)
        }
        // aedit <area> - show all subcommands
        2 if !completing_word => all_static(AEDIT_SUBCOMMANDS, CompletionType::AeditSubcommand),
        // aedit immigration <partial_subcmd> - complete immigration subcommand (no area)
        3 if completing_word && words[1].to_lowercase() == "immigration" => {
            filter_static(IMMIGRATION_SUBCOMMANDS, &partial, CompletionType::ImmigrationSubcommand)
        }
        // aedit <area> <partial_subcmd> - complete subcommand
        3 if completing_word => filter_static(AEDIT_SUBCOMMANDS, &partial, CompletionType::AeditSubcommand),
        // aedit <area> permission - show all permission levels
        3 if !completing_word && words[2].to_lowercase() == "permission" => {
            all_static(PERMISSION_LEVELS, CompletionType::PermissionLevel)
        }
        // aedit <area> permission <partial_level> - complete permission level
        4 if completing_word && words[2].to_lowercase() == "permission" => {
            filter_static(PERMISSION_LEVELS, &partial, CompletionType::PermissionLevel)
        }
        // aedit <area> zone - show zone types
        3 if !completing_word && words[2].to_lowercase() == "zone" => {
            all_static(AREA_ZONE_TYPES, CompletionType::AreaZoneType)
        }
        // aedit <area> zone <partial_type> - complete zone type
        4 if completing_word && words[2].to_lowercase() == "zone" => {
            filter_static(AREA_ZONE_TYPES, &partial, CompletionType::AreaZoneType)
        }
        // aedit <area> flags - show area flags
        3 if !completing_word && words[2].to_lowercase() == "flags" => all_static(AREA_FLAGS, CompletionType::AreaFlag),
        // aedit <area> flags <partial_flag> - complete area flag
        4 if completing_word && words[2].to_lowercase() == "flags" => {
            filter_static(AREA_FLAGS, &partial, CompletionType::AreaFlag)
        }
        // aedit <area> forage - show forage types
        3 if !completing_word && words[2].to_lowercase() == "forage" => {
            all_static(FORAGE_TYPES, CompletionType::ForageType)
        }
        // aedit <area> forage <partial_type> - complete forage type
        4 if completing_word && words[2].to_lowercase() == "forage" => {
            filter_static(FORAGE_TYPES, &partial, CompletionType::ForageType)
        }
        // aedit <area> forage <type> - show forage actions
        4 if !completing_word && words[2].to_lowercase() == "forage" => {
            all_static(FORAGE_ACTIONS, CompletionType::ForageAction)
        }
        // aedit <area> forage <type> <partial_action> - complete forage action
        5 if completing_word && words[2].to_lowercase() == "forage" => {
            filter_static(FORAGE_ACTIONS, &partial, CompletionType::ForageAction)
        }
        // aedit <area> immigration - show immigration subcommands
        3 if !completing_word && words[2].to_lowercase() == "immigration" => {
            all_static(IMMIGRATION_SUBCOMMANDS, CompletionType::ImmigrationSubcommand)
        }
        // aedit <area> immigration <partial_subcmd> - complete immigration subcommand
        4 if completing_word && words[2].to_lowercase() == "immigration" => {
            filter_static(IMMIGRATION_SUBCOMMANDS, &partial, CompletionType::ImmigrationSubcommand)
        }
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for spedit command
/// Syntax: spedit <area> create <room> <type> <vnum> <max> <interval>
/// Helper to check if a word is a spedit filter keyword
fn is_spedit_filter(word: &str) -> bool {
    let lower = word.to_lowercase();
    SPEDIT_FILTERS.iter().any(|&f| f == lower)
}

/// Helper to check if a word is a spedit modification command that supports filters
fn is_spedit_mod_command(word: &str) -> bool {
    let lower = word.to_lowercase();
    matches!(
        lower.as_str(),
        "delete" | "enable" | "disable" | "max" | "interval" | "dep"
    )
}

fn complete_spedit(
    words: &[&str],
    completing_word: bool,
    _area_prefixes: &[String],
    room_vnums: &[String],
    mobile_vnums: &[String],
    item_vnums: &[String],
) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // spedit - show all subcommands
        1 if !completing_word => all_static(SPEDIT_SUBCOMMANDS, CompletionType::SpeditSubcommand),
        // spedit <partial_subcmd> - complete subcommand
        2 if completing_word => filter_static(SPEDIT_SUBCOMMANDS, &partial, CompletionType::SpeditSubcommand),

        // === list command ===
        // spedit list - show filter options
        2 if !completing_word && words[1].to_lowercase() == "list" => {
            all_static(SPEDIT_FILTERS, CompletionType::SpeditFilter)
        }
        // spedit list <partial_filter> - complete filter
        3 if completing_word && words[1].to_lowercase() == "list" => {
            filter_static(SPEDIT_FILTERS, &partial, CompletionType::SpeditFilter)
        }
        // spedit list room - show room vnums
        3 if !completing_word && words[1].to_lowercase() == "list" && words[2].to_lowercase() == "room" => {
            all_dynamic(room_vnums, CompletionType::RoomVnum)
        }
        // spedit list room <partial_vnum> - complete room vnum
        4 if completing_word && words[1].to_lowercase() == "list" && words[2].to_lowercase() == "room" => {
            filter_dynamic(room_vnums, &partial, CompletionType::RoomVnum)
        }

        // === create command ===
        // spedit create - show room vnums (including "." for current room)
        2 if !completing_word && words[1].to_lowercase() == "create" => {
            let mut matches: Vec<String> = vec![".".to_string()];
            matches.extend(room_vnums.iter().cloned());
            CompletionResult::new(matches, "", CompletionType::RoomVnum)
        }
        // spedit create <partial_room> - complete room vnum
        3 if completing_word && words[1].to_lowercase() == "create" => {
            let mut matches: Vec<String> = if ".".starts_with(&partial) {
                vec![".".to_string()]
            } else {
                vec![]
            };
            matches.extend(
                room_vnums
                    .iter()
                    .filter(|v| v.to_lowercase().starts_with(&partial))
                    .cloned(),
            );
            CompletionResult::new(matches, &partial, CompletionType::RoomVnum)
        }
        // spedit create <room> - show entity types
        3 if !completing_word && words[1].to_lowercase() == "create" => {
            all_static(SPAWN_ENTITY_TYPES, CompletionType::SpawnEntityType)
        }
        // spedit create <room> <partial_type> - complete entity type
        4 if completing_word && words[1].to_lowercase() == "create" => {
            filter_static(SPAWN_ENTITY_TYPES, &partial, CompletionType::SpawnEntityType)
        }
        // spedit create <room> mobile - complete mobile vnums
        4 if !completing_word && words[1].to_lowercase() == "create" && words[3].to_lowercase() == "mobile" => {
            all_dynamic(mobile_vnums, CompletionType::MobileVnum)
        }
        // spedit create <room> mobile <partial_vnum> - complete mobile vnum
        5 if completing_word && words[1].to_lowercase() == "create" && words[3].to_lowercase() == "mobile" => {
            filter_dynamic(mobile_vnums, &partial, CompletionType::MobileVnum)
        }
        // spedit create <room> item - complete item vnums
        4 if !completing_word && words[1].to_lowercase() == "create" && words[3].to_lowercase() == "item" => {
            all_dynamic(item_vnums, CompletionType::ItemVnum)
        }
        // spedit create <room> item <partial_vnum> - complete item vnum
        5 if completing_word && words[1].to_lowercase() == "create" && words[3].to_lowercase() == "item" => {
            filter_dynamic(item_vnums, &partial, CompletionType::ItemVnum)
        }

        // === modification commands (delete, enable, disable, max, interval) ===
        // spedit <mod_cmd> - show filter options (since index can also be typed directly)
        2 if !completing_word && is_spedit_mod_command(words[1]) && words[1].to_lowercase() != "dep" => {
            all_static(SPEDIT_FILTERS, CompletionType::SpeditFilter)
        }
        // spedit <mod_cmd> <partial_filter> - complete filter (or could be index)
        3 if completing_word && is_spedit_mod_command(words[1]) && words[1].to_lowercase() != "dep" => {
            filter_static(SPEDIT_FILTERS, &partial, CompletionType::SpeditFilter)
        }
        // spedit <mod_cmd> room - show room vnums for "room <vnum>" filter
        3 if !completing_word
            && is_spedit_mod_command(words[1])
            && words[1].to_lowercase() != "dep"
            && words[2].to_lowercase() == "room" =>
        {
            all_dynamic(room_vnums, CompletionType::RoomVnum)
        }
        // spedit <mod_cmd> room <partial_vnum> - complete room vnum
        4 if completing_word
            && is_spedit_mod_command(words[1])
            && words[1].to_lowercase() != "dep"
            && words[2].to_lowercase() == "room" =>
        {
            filter_dynamic(room_vnums, &partial, CompletionType::RoomVnum)
        }

        // === dep command ===
        // spedit dep - show filter options AND dep actions combined
        2 if !completing_word && words[1].to_lowercase() == "dep" => {
            let mut combined: Vec<String> = SPEDIT_FILTERS.iter().map(|s| s.to_string()).collect();
            combined.extend(SPEDIT_DEP_ACTIONS.iter().map(|s| s.to_string()));
            CompletionResult::new(combined, "", CompletionType::SpeditDepAction)
        }
        // spedit dep <partial> - complete filter or dep action
        3 if completing_word && words[1].to_lowercase() == "dep" => {
            let mut combined: Vec<&str> = SPEDIT_FILTERS.to_vec();
            combined.extend(SPEDIT_DEP_ACTIONS);
            let matches: Vec<String> = combined
                .iter()
                .filter(|s| s.to_lowercase().starts_with(&partial))
                .map(|s| s.to_string())
                .collect();
            CompletionResult::new(matches, &partial, CompletionType::SpeditDepAction)
        }
        // spedit dep <filter> - show dep actions
        3 if !completing_word && words[1].to_lowercase() == "dep" && is_spedit_filter(words[2]) => {
            all_static(SPEDIT_DEP_ACTIONS, CompletionType::SpeditDepAction)
        }
        // spedit dep <filter> <partial_action> - complete dep action
        4 if completing_word && words[1].to_lowercase() == "dep" && is_spedit_filter(words[2]) => {
            filter_static(SPEDIT_DEP_ACTIONS, &partial, CompletionType::SpeditDepAction)
        }
        // spedit dep room - show room vnums
        3 if !completing_word && words[1].to_lowercase() == "dep" && words[2].to_lowercase() == "room" => {
            all_dynamic(room_vnums, CompletionType::RoomVnum)
        }
        // spedit dep room <partial_vnum> - complete room vnum
        4 if completing_word && words[1].to_lowercase() == "dep" && words[2].to_lowercase() == "room" => {
            filter_dynamic(room_vnums, &partial, CompletionType::RoomVnum)
        }
        // spedit dep room <vnum> - show dep actions
        4 if !completing_word && words[1].to_lowercase() == "dep" && words[2].to_lowercase() == "room" => {
            all_static(SPEDIT_DEP_ACTIONS, CompletionType::SpeditDepAction)
        }
        // spedit dep room <vnum> <partial_action> - complete dep action
        5 if completing_word && words[1].to_lowercase() == "dep" && words[2].to_lowercase() == "room" => {
            filter_static(SPEDIT_DEP_ACTIONS, &partial, CompletionType::SpeditDepAction)
        }

        // === dep add without filter (spedit dep add <index> <type> <vnum>) ===
        // spedit dep add <index> - show dep types
        4 if !completing_word && words[1].to_lowercase() == "dep" && words[2].to_lowercase() == "add" => {
            all_static(SPEDIT_DEP_TYPES, CompletionType::SpeditDepType)
        }
        // spedit dep add <index> <partial_type> - complete dep type
        5 if completing_word && words[1].to_lowercase() == "dep" && words[2].to_lowercase() == "add" => {
            filter_static(SPEDIT_DEP_TYPES, &partial, CompletionType::SpeditDepType)
        }
        // spedit dep add <index> <type> - show item vnums
        5 if !completing_word && words[1].to_lowercase() == "dep" && words[2].to_lowercase() == "add" => {
            all_dynamic(item_vnums, CompletionType::ItemVnum)
        }
        // spedit dep add <index> <type> <partial_vnum> - complete item vnum
        6 if completing_word && words[1].to_lowercase() == "dep" && words[2].to_lowercase() == "add" => {
            filter_dynamic(item_vnums, &partial, CompletionType::ItemVnum)
        }
        // spedit dep add <index> equip <vnum> - show wear slots
        6 if !completing_word
            && words[1].to_lowercase() == "dep"
            && words[2].to_lowercase() == "add"
            && words[4].to_lowercase() == "equip" =>
        {
            all_static(WEAR_SLOTS, CompletionType::WearSlot)
        }
        // spedit dep add <index> equip <vnum> <partial_slot> - complete wear slot
        7 if completing_word
            && words[1].to_lowercase() == "dep"
            && words[2].to_lowercase() == "add"
            && words[4].to_lowercase() == "equip" =>
        {
            filter_static(WEAR_SLOTS, &partial, CompletionType::WearSlot)
        }

        // === dep add with filter (spedit dep <filter> add <index> <type> <vnum>) ===
        // spedit dep <filter> add <index> - show dep types
        5 if !completing_word
            && words[1].to_lowercase() == "dep"
            && is_spedit_filter(words[2])
            && words[3].to_lowercase() == "add" =>
        {
            all_static(SPEDIT_DEP_TYPES, CompletionType::SpeditDepType)
        }
        // spedit dep <filter> add <index> <partial_type> - complete dep type
        6 if completing_word
            && words[1].to_lowercase() == "dep"
            && is_spedit_filter(words[2])
            && words[3].to_lowercase() == "add" =>
        {
            filter_static(SPEDIT_DEP_TYPES, &partial, CompletionType::SpeditDepType)
        }
        // spedit dep <filter> add <index> <type> - show item vnums
        6 if !completing_word
            && words[1].to_lowercase() == "dep"
            && is_spedit_filter(words[2])
            && words[3].to_lowercase() == "add" =>
        {
            all_dynamic(item_vnums, CompletionType::ItemVnum)
        }
        // spedit dep <filter> add <index> <type> <partial_vnum> - complete item vnum
        7 if completing_word
            && words[1].to_lowercase() == "dep"
            && is_spedit_filter(words[2])
            && words[3].to_lowercase() == "add" =>
        {
            filter_dynamic(item_vnums, &partial, CompletionType::ItemVnum)
        }
        // spedit dep <filter> add <index> equip <vnum> - show wear slots
        7 if !completing_word
            && words[1].to_lowercase() == "dep"
            && is_spedit_filter(words[2])
            && words[3].to_lowercase() == "add"
            && words[5].to_lowercase() == "equip" =>
        {
            all_static(WEAR_SLOTS, CompletionType::WearSlot)
        }
        // spedit dep <filter> add <index> equip <vnum> <partial_slot> - complete wear slot
        8 if completing_word
            && words[1].to_lowercase() == "dep"
            && is_spedit_filter(words[2])
            && words[3].to_lowercase() == "add"
            && words[5].to_lowercase() == "equip" =>
        {
            filter_static(WEAR_SLOTS, &partial, CompletionType::WearSlot)
        }

        // === dep add with "room <vnum>" filter (spedit dep room <vnum> add <index> <type> <item_vnum>) ===
        // spedit dep room <vnum> add <index> - show dep types
        6 if !completing_word
            && words[1].to_lowercase() == "dep"
            && words[2].to_lowercase() == "room"
            && words[4].to_lowercase() == "add" =>
        {
            all_static(SPEDIT_DEP_TYPES, CompletionType::SpeditDepType)
        }
        // spedit dep room <vnum> add <index> <partial_type> - complete dep type
        7 if completing_word
            && words[1].to_lowercase() == "dep"
            && words[2].to_lowercase() == "room"
            && words[4].to_lowercase() == "add" =>
        {
            filter_static(SPEDIT_DEP_TYPES, &partial, CompletionType::SpeditDepType)
        }
        // spedit dep room <vnum> add <index> <type> - show item vnums
        7 if !completing_word
            && words[1].to_lowercase() == "dep"
            && words[2].to_lowercase() == "room"
            && words[4].to_lowercase() == "add" =>
        {
            all_dynamic(item_vnums, CompletionType::ItemVnum)
        }
        // spedit dep room <vnum> add <index> <type> <partial_vnum> - complete item vnum
        8 if completing_word
            && words[1].to_lowercase() == "dep"
            && words[2].to_lowercase() == "room"
            && words[4].to_lowercase() == "add" =>
        {
            filter_dynamic(item_vnums, &partial, CompletionType::ItemVnum)
        }
        // spedit dep room <vnum> add <index> equip <vnum> - show wear slots
        8 if !completing_word
            && words[1].to_lowercase() == "dep"
            && words[2].to_lowercase() == "room"
            && words[4].to_lowercase() == "add"
            && words[6].to_lowercase() == "equip" =>
        {
            all_static(WEAR_SLOTS, CompletionType::WearSlot)
        }
        // spedit dep room <vnum> add <index> equip <vnum> <partial_slot> - complete wear slot
        9 if completing_word
            && words[1].to_lowercase() == "dep"
            && words[2].to_lowercase() == "room"
            && words[4].to_lowercase() == "add"
            && words[6].to_lowercase() == "equip" =>
        {
            filter_static(WEAR_SLOTS, &partial, CompletionType::WearSlot)
        }

        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for set command
fn complete_set(words: &[&str], completing_word: bool, is_builder: bool) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    // Build available settings based on permissions
    let mut available: Vec<&str> = SET_SUBCOMMANDS_BASE.to_vec();
    if is_builder {
        available.extend(SET_SUBCOMMANDS_BUILDER);
    }

    match words.len() {
        // set - show all available settings
        1 if !completing_word => all_static(&available, CompletionType::SetSubcommand),
        // set <partial_setting> - complete setting name
        2 if completing_word => filter_static(&available, &partial, CompletionType::SetSubcommand),
        // set <setting> - show on/off options
        2 if !completing_word => all_static(SET_TOGGLE_VALUES, CompletionType::SetSubcommand),
        // set <setting> <partial_value> - complete on/off
        3 if completing_word => filter_static(SET_TOGGLE_VALUES, &partial, CompletionType::SetSubcommand),
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for recedit command
fn complete_recedit(
    words: &[&str],
    completing_word: bool,
    recipe_vnums: &[String],
    item_vnums: &[String],
) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // recedit <partial_vnum> - complete recipe vnum
        2 if completing_word => filter_dynamic(recipe_vnums, &partial, CompletionType::RecipeVnum),
        // recedit <vnum> - show all subcommands
        2 if !completing_word => all_static(RECEDIT_SUBCOMMANDS, CompletionType::ReceditSubcommand),
        // recedit <vnum> <partial_subcmd> - complete subcommand
        3 if completing_word => filter_static(RECEDIT_SUBCOMMANDS, &partial, CompletionType::ReceditSubcommand),
        // recedit <vnum> skill - show skill types
        3 if !completing_word && words[2].to_lowercase() == "skill" => {
            all_static(RECIPE_SKILLS, CompletionType::RecipeSkill)
        }
        // recedit <vnum> output - show item vnums (hint)
        3 if !completing_word && words[2].to_lowercase() == "output" => {
            all_dynamic(item_vnums, CompletionType::ItemVnum)
        }
        // recedit <vnum> skill <partial> - complete skill
        4 if completing_word && words[2].to_lowercase() == "skill" => {
            filter_static(RECIPE_SKILLS, &partial, CompletionType::RecipeSkill)
        }
        // recedit <vnum> autolearn - show on/off
        3 if !completing_word && words[2].to_lowercase() == "autolearn" => {
            all_static(SET_TOGGLE_VALUES, CompletionType::SetSubcommand)
        }
        // recedit <vnum> autolearn <partial> - complete on/off
        4 if completing_word && words[2].to_lowercase() == "autolearn" => {
            filter_static(SET_TOGGLE_VALUES, &partial, CompletionType::SetSubcommand)
        }
        // recedit <vnum> output <partial_item_vnum> - complete item vnum
        4 if completing_word && words[2].to_lowercase() == "output" => {
            filter_dynamic(item_vnums, &partial, CompletionType::ItemVnum)
        }
        // recedit <vnum> ingredient - show actions
        3 if !completing_word && words[2].to_lowercase() == "ingredient" => {
            all_static(INGREDIENT_ACTIONS, CompletionType::IngredientAction)
        }
        // recedit <vnum> ingredient <partial> - complete action
        4 if completing_word && words[2].to_lowercase() == "ingredient" => {
            filter_static(INGREDIENT_ACTIONS, &partial, CompletionType::IngredientAction)
        }
        // recedit <vnum> ingredient add - show item vnums
        4 if !completing_word && words[2].to_lowercase() == "ingredient" && words[3].to_lowercase() == "add" => {
            all_dynamic(item_vnums, CompletionType::ItemVnum)
        }
        // recedit <vnum> ingredient add <partial_vnum> - complete item vnum
        5 if completing_word && words[2].to_lowercase() == "ingredient" && words[3].to_lowercase() == "add" => {
            filter_dynamic(item_vnums, &partial, CompletionType::ItemVnum)
        }
        // recedit <vnum> tool - show actions
        3 if !completing_word && words[2].to_lowercase() == "tool" => {
            all_static(TOOL_ACTIONS, CompletionType::ToolAction)
        }
        // recedit <vnum> tool <partial> - complete action
        4 if completing_word && words[2].to_lowercase() == "tool" => {
            filter_static(TOOL_ACTIONS, &partial, CompletionType::ToolAction)
        }
        // recedit <vnum> tool add - show item vnums
        4 if !completing_word && words[2].to_lowercase() == "tool" && words[3].to_lowercase() == "add" => {
            all_dynamic(item_vnums, CompletionType::ItemVnum)
        }
        // recedit <vnum> tool add <partial_vnum> - complete item vnum
        5 if completing_word && words[2].to_lowercase() == "tool" && words[3].to_lowercase() == "add" => {
            filter_dynamic(item_vnums, &partial, CompletionType::ItemVnum)
        }
        // recedit <vnum> tool add <spec> - show locations
        5 if !completing_word && words[2].to_lowercase() == "tool" && words[3].to_lowercase() == "add" => {
            all_static(TOOL_LOCATIONS, CompletionType::ToolLocation)
        }
        // recedit <vnum> tool add <spec> <partial_loc> - complete location
        6 if completing_word && words[2].to_lowercase() == "tool" && words[3].to_lowercase() == "add" => {
            filter_static(TOOL_LOCATIONS, &partial, CompletionType::ToolLocation)
        }
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for reclist command
fn complete_reclist(words: &[&str], completing_word: bool) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // reclist - show skill filters
        1 if !completing_word => all_static(RECIPE_SKILLS, CompletionType::RecipeSkill),
        // reclist <partial_skill> - complete skill filter
        2 if completing_word => filter_static(RECIPE_SKILLS, &partial, CompletionType::RecipeSkill),
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for admin command
fn complete_admin(words: &[&str], completing_word: bool, online_players: &[String]) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // admin - show all subcommands
        1 if !completing_word => all_static(ADMIN_SUBCOMMANDS, CompletionType::AdminSubcommand),
        // admin <partial_subcommand> - complete subcommand
        2 if completing_word => filter_static(ADMIN_SUBCOMMANDS, &partial, CompletionType::AdminSubcommand),
        // admin <subcommand> - show next level options
        2 if !completing_word => {
            let subcommand = words[1].to_lowercase();
            match subcommand.as_str() {
                "kick" | "summon" | "heal" => all_dynamic(online_players, CompletionType::PlayerName),
                "user" => all_static(ADMIN_USER_ACTIONS, CompletionType::AdminUserAction),
                "api-key" => all_static(ADMIN_API_KEY_ACTIONS, CompletionType::AdminApiKeyAction),
                _ => CompletionResult::empty(),
            }
        }
        // admin <subcommand> <partial> - complete next level
        3 if completing_word => {
            let subcommand = words[1].to_lowercase();
            match subcommand.as_str() {
                "kick" | "summon" | "heal" => filter_dynamic(online_players, &partial, CompletionType::PlayerName),
                "user" => filter_static(ADMIN_USER_ACTIONS, &partial, CompletionType::AdminUserAction),
                "api-key" => filter_static(ADMIN_API_KEY_ACTIONS, &partial, CompletionType::AdminApiKeyAction),
                _ => CompletionResult::empty(),
            }
        }
        // admin user <action> - show player names for actions that need them
        3 if !completing_word => {
            let subcommand = words[1].to_lowercase();
            if subcommand == "user" {
                let action = words[2].to_lowercase();
                match action.as_str() {
                    "info" | "grant-admin" | "revoke-admin" | "grant-builder" | "revoke-builder" | "password"
                    | "delete" => all_dynamic(online_players, CompletionType::PlayerName),
                    _ => CompletionResult::empty(),
                }
            } else {
                CompletionResult::empty()
            }
        }
        // admin user <action> <partial_player> - complete player name
        4 if completing_word => {
            let subcommand = words[1].to_lowercase();
            if subcommand == "user" {
                let action = words[2].to_lowercase();
                match action.as_str() {
                    "info" | "grant-admin" | "revoke-admin" | "grant-builder" | "revoke-builder" | "password"
                    | "delete" => filter_dynamic(online_players, &partial, CompletionType::PlayerName),
                    _ => CompletionResult::empty(),
                }
            } else {
                CompletionResult::empty()
            }
        }
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for motd command
fn complete_motd(words: &[&str], completing_word: bool) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // motd - show all subcommands
        1 if !completing_word => all_static(MOTD_SUBCOMMANDS, CompletionType::MotdSubcommand),
        // motd <partial_subcommand> - complete subcommand
        2 if completing_word => filter_static(MOTD_SUBCOMMANDS, &partial, CompletionType::MotdSubcommand),
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for bugs command
fn complete_bugs(words: &[&str], completing_word: bool) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // bugs - show subcommands
        1 if !completing_word => all_static(BUGS_SUBCOMMANDS, CompletionType::BugsSubcommand),
        // bugs <partial> - complete subcommand
        2 if completing_word => filter_static(BUGS_SUBCOMMANDS, &partial, CompletionType::BugsSubcommand),
        // bugs list - show status filters
        2 if !completing_word && words[1].to_lowercase() == "list" => {
            all_static(BUG_STATUS_FILTERS, CompletionType::BugStatusFilter)
        }
        // bugs list <partial> - complete status filter
        3 if completing_word && words[1].to_lowercase() == "list" => {
            filter_static(BUG_STATUS_FILTERS, &partial, CompletionType::BugStatusFilter)
        }
        // bugs status <#> - show status values
        3 if !completing_word && words[1].to_lowercase() == "status" => {
            all_static(BUG_STATUS_VALUES, CompletionType::BugStatusFilter)
        }
        // bugs status <#> <partial> - complete status value
        4 if completing_word && words[1].to_lowercase() == "status" => {
            filter_static(BUG_STATUS_VALUES, &partial, CompletionType::BugStatusFilter)
        }
        // bugs priority <#> - show priority values
        3 if !completing_word && words[1].to_lowercase() == "priority" => {
            all_static(BUG_PRIORITY_VALUES, CompletionType::BugPriorityValue)
        }
        // bugs priority <#> <partial> - complete priority value
        4 if completing_word && words[1].to_lowercase() == "priority" => {
            filter_static(BUG_PRIORITY_VALUES, &partial, CompletionType::BugPriorityValue)
        }
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for summon command
/// Syntax: summon mob <vnum> [room_vnum] | summon player <name> [room_vnum]
fn complete_summon(
    words: &[&str],
    completing_word: bool,
    mobile_vnums: &[String],
    online_players: &[String],
    room_vnums: &[String],
) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // summon - show type keywords
        1 if !completing_word => all_static(&["mob", "player"], CompletionType::SummonTarget),
        // summon <partial> - complete type keyword
        2 if completing_word => filter_static(&["mob", "player"], &partial, CompletionType::SummonTarget),
        // summon mob - show mobile vnums
        2 if !completing_word && words[1].to_lowercase() == "mob" => {
            all_dynamic(mobile_vnums, CompletionType::MobileVnum)
        }
        // summon player - show online players
        2 if !completing_word && words[1].to_lowercase() == "player" => {
            all_dynamic(online_players, CompletionType::PlayerName)
        }
        // summon mob <partial_vnum> - complete mobile vnum
        3 if completing_word && words[1].to_lowercase() == "mob" => {
            filter_dynamic(mobile_vnums, &partial, CompletionType::MobileVnum)
        }
        // summon player <partial_name> - complete player name
        3 if completing_word && words[1].to_lowercase() == "player" => {
            filter_dynamic(online_players, &partial, CompletionType::PlayerName)
        }
        // summon mob <vnum> - show room vnums
        3 if !completing_word => all_dynamic(room_vnums, CompletionType::RoomVnum),
        // summon mob/player <id> <partial_room> - complete room vnum
        4 if completing_word => filter_dynamic(room_vnums, &partial, CompletionType::RoomVnum),
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for treat command
/// Syntax: treat <target> [body_part or condition]
fn complete_treat(words: &[&str], completing_word: bool, online_players: &[String]) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // treat - show "self" and online players
        1 if !completing_word => {
            let mut matches = vec!["self".to_string()];
            matches.extend(online_players.iter().cloned());
            CompletionResult::new(matches, "", CompletionType::TreatTarget)
        }
        // treat <partial_target> - complete "self" or player name
        2 if completing_word => {
            let mut matches: Vec<String> = Vec::new();
            if "self".starts_with(&partial) {
                matches.push("self".to_string());
            }
            matches.extend(
                online_players
                    .iter()
                    .filter(|p| p.to_lowercase().starts_with(&partial))
                    .cloned(),
            );
            CompletionResult::new(matches, &partial, CompletionType::TreatTarget)
        }
        // treat <target> - show body parts and conditions
        2 if !completing_word => all_static(TREAT_TARGETS, CompletionType::BodyPart),
        // treat <target> <partial_part_or_condition> - complete body part or condition
        3 if completing_word => {
            let matches: Vec<String> = TREAT_TARGETS
                .iter()
                .filter(|s| s.starts_with(&partial))
                .map(|s| s.to_string())
                .collect();
            // Determine appropriate completion type based on what's matching
            let completion_type = if matches.iter().all(|m| TREATABLE_CONDITIONS.contains(&m.as_str())) {
                CompletionType::TreatableCondition
            } else if matches.iter().all(|m| BODY_PARTS.contains(&m.as_str())) {
                CompletionType::BodyPart
            } else {
                CompletionType::BodyPart // Mixed results default to BodyPart
            };
            CompletionResult::new(matches, &partial, completion_type)
        }
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for tedit command
/// Syntax: tedit <vnum> <subcommand> [args...]
/// Or: tedit create <vnum>
fn complete_tedit(
    words: &[&str],
    completing_word: bool,
    transport_vnums: &[String],
    room_vnums: &[String],
) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // tedit <partial_vnum_or_create> - complete vnum or "create"
        2 if completing_word => {
            let mut matches: Vec<String> = Vec::new();
            if "create".starts_with(&partial) {
                matches.push("create".to_string());
            }
            matches.extend(
                transport_vnums
                    .iter()
                    .filter(|v| v.to_lowercase().starts_with(&partial))
                    .cloned(),
            );
            CompletionResult::new(matches, &partial, CompletionType::TransportVnum)
        }
        // tedit - show "create" and all vnums
        1 if !completing_word => {
            let mut matches = vec!["create".to_string()];
            matches.extend(transport_vnums.iter().cloned());
            CompletionResult::new(matches, "", CompletionType::TransportVnum)
        }
        // tedit <vnum> - show all subcommands (if not "create")
        2 if !completing_word => {
            if words[1].to_lowercase() == "create" {
                CompletionResult::empty()
            } else {
                all_static(TEDIT_SUBCOMMANDS, CompletionType::TeditSubcommand)
            }
        }
        // tedit <vnum> <partial_subcmd> - complete subcommand
        3 if completing_word => filter_static(TEDIT_SUBCOMMANDS, &partial, CompletionType::TeditSubcommand),
        // tedit <vnum> type - show transport types
        3 if !completing_word && words[2].to_lowercase() == "type" => {
            all_static(TRANSPORT_TYPES, CompletionType::TransportType)
        }
        // tedit <vnum> type <partial_type> - complete transport type
        4 if completing_word && words[2].to_lowercase() == "type" => {
            filter_static(TRANSPORT_TYPES, &partial, CompletionType::TransportType)
        }
        // tedit <vnum> schedule - show schedule types
        3 if !completing_word && words[2].to_lowercase() == "schedule" => {
            all_static(SCHEDULE_TYPES, CompletionType::TeditSubcommand)
        }
        // tedit <vnum> schedule <partial_type> - complete schedule type
        4 if completing_word && words[2].to_lowercase() == "schedule" => {
            filter_static(SCHEDULE_TYPES, &partial, CompletionType::TeditSubcommand)
        }
        // tedit <vnum> stop - show stop actions
        3 if !completing_word && words[2].to_lowercase() == "stop" => {
            all_static(STOP_ACTIONS, CompletionType::StopAction)
        }
        // tedit <vnum> stop <partial_action> - complete stop action
        4 if completing_word && words[2].to_lowercase() == "stop" => {
            filter_static(STOP_ACTIONS, &partial, CompletionType::StopAction)
        }
        // tedit <vnum> stop add - show room vnums
        4 if !completing_word && words[2].to_lowercase() == "stop" && words[3].to_lowercase() == "add" => {
            all_dynamic(room_vnums, CompletionType::RoomVnum)
        }
        // tedit <vnum> stop add <partial_room> - complete room vnum
        5 if completing_word && words[2].to_lowercase() == "stop" && words[3].to_lowercase() == "add" => {
            filter_dynamic(room_vnums, &partial, CompletionType::RoomVnum)
        }
        // tedit <vnum> stop add <room> <name> - show directions (after name entered)
        6 if !completing_word && words[2].to_lowercase() == "stop" && words[3].to_lowercase() == "add" => {
            all_static(DIRECTIONS, CompletionType::Direction)
        }
        // tedit <vnum> stop add <room> <name> <partial_dir> - complete direction
        7 if completing_word && words[2].to_lowercase() == "stop" && words[3].to_lowercase() == "add" => {
            filter_static(DIRECTIONS, &partial, CompletionType::Direction)
        }
        // tedit <vnum> interior - show room vnums
        3 if !completing_word && words[2].to_lowercase() == "interior" => {
            all_dynamic(room_vnums, CompletionType::RoomVnum)
        }
        // tedit <vnum> interior <partial_room> - complete room vnum
        4 if completing_word && words[2].to_lowercase() == "interior" => {
            filter_dynamic(room_vnums, &partial, CompletionType::RoomVnum)
        }
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for press command
/// At a transport stop: press button
/// Inside a transport: press <number> or press <stop_name>
fn complete_press(words: &[&str], completing_word: bool) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // press - show "button" (stop names would need runtime data)
        1 if !completing_word => all_static(PRESS_TARGETS, CompletionType::PressTarget),
        // press <partial> - complete "button"
        2 if completing_word => filter_static(PRESS_TARGETS, &partial, CompletionType::PressTarget),
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for pedit command
fn complete_pedit(words: &[&str], completing_word: bool, property_template_vnums: &[String]) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // pedit - show all template vnums
        1 if !completing_word => all_dynamic(property_template_vnums, CompletionType::PropertyTemplateVnum),
        // pedit <partial_vnum> - complete vnum
        2 if completing_word => filter_dynamic(property_template_vnums, &partial, CompletionType::PropertyTemplateVnum),
        // pedit <vnum> - show all subcommands
        2 if !completing_word => all_static(PEDIT_SUBCOMMANDS, CompletionType::PeditSubcommand),
        // pedit <vnum> <partial_subcmd> - complete subcommand
        3 if completing_word => filter_static(PEDIT_SUBCOMMANDS, &partial, CompletionType::PeditSubcommand),
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for property command
fn complete_property(words: &[&str], completing_word: bool, online_players: &[String]) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // property - show subcommands
        1 if !completing_word => all_static(PROPERTY_SUBCOMMANDS, CompletionType::PropertySubcommand),
        // property <partial_subcmd> - complete subcommand
        2 if completing_word => filter_static(PROPERTY_SUBCOMMANDS, &partial, CompletionType::PropertySubcommand),
        // property access - show access levels
        2 if !completing_word && words[1].to_lowercase() == "access" => {
            all_static(PROPERTY_ACCESS_LEVELS, CompletionType::PropertyAccessLevel)
        }
        // property access <partial_level> - complete access level
        3 if completing_word && words[1].to_lowercase() == "access" => {
            filter_static(PROPERTY_ACCESS_LEVELS, &partial, CompletionType::PropertyAccessLevel)
        }
        // property trust/untrust - show online players
        2 if !completing_word && (words[1].to_lowercase() == "trust" || words[1].to_lowercase() == "untrust") => {
            all_dynamic(online_players, CompletionType::PlayerName)
        }
        // property trust/untrust <partial_name> - complete player name
        3 if completing_word && (words[1].to_lowercase() == "trust" || words[1].to_lowercase() == "untrust") => {
            filter_dynamic(online_players, &partial, CompletionType::PlayerName)
        }
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for mail command
fn complete_mail(words: &[&str], completing_word: bool, online_players: &[String]) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // mail - show subcommands
        1 if !completing_word => all_static(MAIL_SUBCOMMANDS, CompletionType::MailSubcommand),
        // mail <partial_subcmd> - complete subcommand
        2 if completing_word => filter_static(MAIL_SUBCOMMANDS, &partial, CompletionType::MailSubcommand),
        // mail send/compose/reply - show online players (for recipient)
        2 if !completing_word
            && (words[1].to_lowercase() == "send"
                || words[1].to_lowercase() == "compose"
                || words[1].to_lowercase() == "reply") =>
        {
            all_dynamic(online_players, CompletionType::PlayerName)
        }
        // mail send/compose <partial_name> - complete player name
        3 if completing_word && (words[1].to_lowercase() == "send" || words[1].to_lowercase() == "compose") => {
            filter_dynamic(online_players, &partial, CompletionType::PlayerName)
        }
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for bank command
fn complete_bank(words: &[&str], completing_word: bool) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // bank - show subcommands
        1 if !completing_word => all_static(BANK_SUBCOMMANDS, CompletionType::BankSubcommand),
        // bank <partial_subcmd> - complete subcommand
        2 if completing_word => filter_static(BANK_SUBCOMMANDS, &partial, CompletionType::BankSubcommand),
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for escrow command
fn complete_escrow(words: &[&str], completing_word: bool) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // escrow - show subcommands
        1 if !completing_word => all_static(ESCROW_SUBCOMMANDS, CompletionType::EscrowSubcommand),
        // escrow <partial_subcmd> - complete subcommand
        2 if completing_word => filter_static(ESCROW_SUBCOMMANDS, &partial, CompletionType::EscrowSubcommand),
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for bpredit command
fn complete_bpredit(words: &[&str], completing_word: bool, shop_preset_vnums: &[String]) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // bpredit <partial_vnum> - complete vnum (also matches "list", "create", "delete")
        2 if completing_word => {
            // Combine static subcommands and dynamic vnums
            let static_cmds = &["list", "create", "delete"];
            let mut matches: Vec<String> = static_cmds
                .iter()
                .filter(|s| s.starts_with(&partial))
                .map(|s| s.to_string())
                .collect();
            matches.extend(
                shop_preset_vnums
                    .iter()
                    .filter(|v| v.to_lowercase().starts_with(&partial))
                    .cloned(),
            );
            CompletionResult::new(matches, &partial, CompletionType::ShopPresetVnum)
        }
        // bpredit <vnum> - show subcommands
        2 if !completing_word => all_static(BPREDIT_SUBCOMMANDS, CompletionType::BpreditSubcommand),
        // bpredit <vnum> <partial_subcmd>
        3 if completing_word => filter_static(BPREDIT_SUBCOMMANDS, &partial, CompletionType::BpreditSubcommand),
        // bpredit <vnum> type - show add/remove
        3 if !completing_word && words[2].to_lowercase() == "type" => {
            all_static(SHOP_STOCK_ACTIONS, CompletionType::ShopStockAction)
        }
        // bpredit <vnum> type <partial_action>
        4 if completing_word && words[2].to_lowercase() == "type" => {
            filter_static(SHOP_STOCK_ACTIONS, &partial, CompletionType::ShopStockAction)
        }
        // bpredit <vnum> type add - show item types
        4 if !completing_word && words[2].to_lowercase() == "type" && words[3].to_lowercase() == "add" => {
            all_static(ITEM_TYPES, CompletionType::ItemType)
        }
        // bpredit <vnum> type add <partial_type>
        5 if completing_word && words[2].to_lowercase() == "type" && words[3].to_lowercase() == "add" => {
            filter_static(ITEM_TYPES, &partial, CompletionType::ItemType)
        }
        // bpredit <vnum> category - show add/remove
        3 if !completing_word && words[2].to_lowercase() == "category" => {
            all_static(SHOP_STOCK_ACTIONS, CompletionType::ShopStockAction)
        }
        // bpredit <vnum> category <partial_action>
        4 if completing_word && words[2].to_lowercase() == "category" => {
            filter_static(SHOP_STOCK_ACTIONS, &partial, CompletionType::ShopStockAction)
        }
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for plantedit command
/// Syntax: plantedit <vnum> <subcommand> [args...]
/// Or: plantedit create <vnum>
/// Or: plantedit list
fn complete_plantedit(
    words: &[&str],
    completing_word: bool,
    plant_vnums: &[String],
    item_vnums: &[String],
) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // plantedit <partial_vnum_or_cmd> - complete vnum or "create"/"list"
        2 if completing_word => {
            let static_cmds = &["create", "list"];
            let mut matches: Vec<String> = static_cmds
                .iter()
                .filter(|s| s.starts_with(&partial))
                .map(|s| s.to_string())
                .collect();
            matches.extend(
                plant_vnums
                    .iter()
                    .filter(|v| v.to_lowercase().starts_with(&partial))
                    .cloned(),
            );
            CompletionResult::new(matches, &partial, CompletionType::PlantVnum)
        }
        // plantedit - show "create", "list", and all vnums
        1 if !completing_word => {
            let mut matches = vec!["create".to_string(), "list".to_string()];
            matches.extend(plant_vnums.iter().cloned());
            CompletionResult::new(matches, "", CompletionType::PlantVnum)
        }
        // plantedit <vnum> - show all subcommands
        2 if !completing_word => {
            if words[1].to_lowercase() == "create" || words[1].to_lowercase() == "list" {
                CompletionResult::empty()
            } else {
                all_static(PLANTEDIT_SUBCOMMANDS, CompletionType::PlanteditSubcommand)
            }
        }
        // plantedit <vnum> <partial_subcmd> - complete subcommand
        3 if completing_word => filter_static(PLANTEDIT_SUBCOMMANDS, &partial, CompletionType::PlanteditSubcommand),
        // plantedit <vnum> category - show plant categories
        3 if !completing_word && words[2].to_lowercase() == "category" => {
            all_static(PLANT_CATEGORIES, CompletionType::PlantCategory)
        }
        // plantedit <vnum> category <partial> - complete category
        4 if completing_word && words[2].to_lowercase() == "category" => {
            filter_static(PLANT_CATEGORIES, &partial, CompletionType::PlantCategory)
        }
        // plantedit <vnum> season - show season actions
        3 if !completing_word && words[2].to_lowercase() == "season" => {
            all_static(PLANT_SEASON_ACTIONS, CompletionType::PlanteditSubcommand)
        }
        // plantedit <vnum> season <partial_action> - complete action
        4 if completing_word && words[2].to_lowercase() == "season" => {
            filter_static(PLANT_SEASON_ACTIONS, &partial, CompletionType::PlanteditSubcommand)
        }
        // plantedit <vnum> season add/remove - show seasons
        4 if !completing_word && words[2].to_lowercase() == "season" => {
            all_static(PLANT_SEASONS, CompletionType::PlantSeason)
        }
        // plantedit <vnum> season add/remove <partial_season> - complete season
        5 if completing_word && words[2].to_lowercase() == "season" => {
            filter_static(PLANT_SEASONS, &partial, CompletionType::PlantSeason)
        }
        // plantedit <vnum> stage - show add/remove
        3 if !completing_word && words[2].to_lowercase() == "stage" => {
            all_static(PLANT_SEASON_ACTIONS, CompletionType::PlanteditSubcommand)
        }
        // plantedit <vnum> stage <partial_action>
        4 if completing_word && words[2].to_lowercase() == "stage" => {
            filter_static(PLANT_SEASON_ACTIONS, &partial, CompletionType::PlanteditSubcommand)
        }
        // plantedit <vnum> stage add - show stage names
        4 if !completing_word && words[2].to_lowercase() == "stage" && words[3].to_lowercase() == "add" => {
            all_static(PLANT_STAGES, CompletionType::PlantStage)
        }
        // plantedit <vnum> stage add <partial_stage> - complete stage name
        5 if completing_word && words[2].to_lowercase() == "stage" && words[3].to_lowercase() == "add" => {
            filter_static(PLANT_STAGES, &partial, CompletionType::PlantStage)
        }
        // plantedit <vnum> keyword - show add/remove
        3 if !completing_word && words[2].to_lowercase() == "keyword" => {
            all_static(PLANT_SEASON_ACTIONS, CompletionType::PlanteditSubcommand)
        }
        // plantedit <vnum> keyword <partial_action>
        4 if completing_word && words[2].to_lowercase() == "keyword" => {
            filter_static(PLANT_SEASON_ACTIONS, &partial, CompletionType::PlanteditSubcommand)
        }
        // plantedit <vnum> seed_vnum - show item vnums
        3 if !completing_word && words[2].to_lowercase() == "seed_vnum" => {
            all_dynamic(item_vnums, CompletionType::ItemVnum)
        }
        // plantedit <vnum> seed_vnum <partial> - complete item vnum
        4 if completing_word && words[2].to_lowercase() == "seed_vnum" => {
            filter_dynamic(item_vnums, &partial, CompletionType::ItemVnum)
        }
        // plantedit <vnum> harvest_vnum - show item vnums
        3 if !completing_word && words[2].to_lowercase() == "harvest_vnum" => {
            all_dynamic(item_vnums, CompletionType::ItemVnum)
        }
        // plantedit <vnum> harvest_vnum <partial> - complete item vnum
        4 if completing_word && words[2].to_lowercase() == "harvest_vnum" => {
            filter_dynamic(item_vnums, &partial, CompletionType::ItemVnum)
        }
        _ => CompletionResult::empty(),
    }
}

/// Find the longest common prefix among a list of strings
fn find_common_prefix(strings: &[String]) -> String {
    if strings.is_empty() {
        return String::new();
    }
    if strings.len() == 1 {
        return strings[0].clone();
    }

    let first = &strings[0];
    let mut prefix_len = first.len();

    for s in &strings[1..] {
        let common_len = first
            .chars()
            .zip(s.chars())
            .take_while(|(a, b)| a.eq_ignore_ascii_case(b))
            .count();
        prefix_len = prefix_len.min(common_len);
    }

    first[..prefix_len].to_string()
}

/// Format completion result for display
pub fn format_completions(result: &CompletionResult, max_width: u16) -> String {
    if result.is_empty() {
        return String::new();
    }

    if result.is_unique() {
        // Single match - no need to display list
        return String::new();
    }

    // Calculate column width using display width for proper emoji/CJK handling
    let max_item_width = result.completions.iter().map(|s| s.width()).max().unwrap_or(0);
    let col_width = max_item_width + 2; // Add padding
    let cols = ((max_width as usize) / col_width).max(1);

    // Format as columns with proper padding for display width
    let mut lines = Vec::new();
    for chunk in result.completions.chunks(cols) {
        let line: Vec<String> = chunk
            .iter()
            .map(|s| {
                // Pad to col_width based on display width, not byte length
                let display_len = s.width();
                let padding = col_width.saturating_sub(display_len);
                format!("{}{}", s, " ".repeat(padding))
            })
            .collect();
        lines.push(line.join(""));
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complete_command() {
        let commands = vec![
            "look".to_string(),
            "login".to_string(),
            "logout".to_string(),
            "help".to_string(),
        ];

        let result = complete(
            "lo",
            2,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert_eq!(result.completions.len(), 3);
        assert!(result.completions.contains(&"look".to_string()));
        assert!(result.completions.contains(&"login".to_string()));
        assert!(result.completions.contains(&"logout".to_string()));
        assert_eq!(result.completion_type, CompletionType::Command);
    }

    #[test]
    fn test_complete_room_vnum() {
        let commands = vec!["rgoto".to_string()];
        let room_vnums = vec![
            "town:square".to_string(),
            "town:tavern".to_string(),
            "forest:entrance".to_string(),
        ];

        let result = complete(
            "rgoto town:",
            11,
            &commands,
            &room_vnums,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert_eq!(result.completions.len(), 2);
        assert!(result.completions.contains(&"town:square".to_string()));
        assert!(result.completions.contains(&"town:tavern".to_string()));
        assert_eq!(result.completion_type, CompletionType::RoomVnum);
    }

    #[test]
    fn test_common_prefix() {
        let strings = vec![
            "town:square".to_string(),
            "town:tavern".to_string(),
            "town:market".to_string(),
        ];
        assert_eq!(find_common_prefix(&strings), "town:");
    }

    #[test]
    fn test_empty_input() {
        let commands = vec!["look".to_string(), "help".to_string()];
        let result = complete(
            "",
            0,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert_eq!(result.completions.len(), 2);
    }

    #[test]
    fn test_direction_completion() {
        let commands = vec!["go".to_string()];
        let result = complete(
            "go nor",
            6,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert_eq!(result.completions.len(), 3); // north, northeast, northwest
        assert!(result.completions.contains(&"north".to_string()));
        assert!(result.completions.contains(&"northeast".to_string()));
        assert!(result.completions.contains(&"northwest".to_string()));
    }

    #[test]
    fn test_medit_subcommand_completion() {
        let commands = vec!["medit".to_string()];
        let mobile_vnums = vec!["town:guard".to_string()];

        // Complete subcommand after vnum
        let result = complete(
            "medit town:guard tr",
            19,
            &commands,
            &[],
            &[],
            &mobile_vnums,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"trigger".to_string()));
        assert_eq!(result.completion_type, CompletionType::MeditSubcommand);
    }

    #[test]
    fn test_medit_trigger_action_completion() {
        let commands = vec!["medit".to_string()];
        let mobile_vnums = vec!["town:guard".to_string()];

        // Complete trigger action after "trigger"
        let result = complete(
            "medit town:guard trigger a",
            26,
            &commands,
            &[],
            &[],
            &mobile_vnums,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"add".to_string()));
        assert_eq!(result.completion_type, CompletionType::TriggerAction);
    }

    #[test]
    fn test_medit_trigger_type_completion() {
        let commands = vec!["medit".to_string()];
        let mobile_vnums = vec!["town:guard".to_string()];

        // Complete trigger type after "add"
        let result = complete(
            "medit town:guard trigger add gr",
            31,
            &commands,
            &[],
            &[],
            &mobile_vnums,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"greet".to_string()));
        assert_eq!(result.completion_type, CompletionType::TriggerType);
    }

    #[test]
    fn test_medit_trigger_template_completion() {
        let commands = vec!["medit".to_string()];
        let mobile_vnums = vec!["town:guard".to_string()];

        // Complete template after trigger type
        let result = complete(
            "medit town:guard trigger add greet @say",
            39,
            &commands,
            &[],
            &[],
            &mobile_vnums,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"@say_greeting".to_string()));
        assert!(result.completions.contains(&"@say_random".to_string()));
        assert_eq!(result.completion_type, CompletionType::TriggerScript);
    }

    #[test]
    fn test_oedit_subcommand_completion() {
        let commands = vec!["oedit".to_string()];
        let item_vnums = vec!["town:sword".to_string()];

        // Complete subcommand after vnum
        let result = complete(
            "oedit town:sword ty",
            19,
            &commands,
            &[],
            &item_vnums,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"type".to_string()));
        assert_eq!(result.completion_type, CompletionType::OeditSubcommand);
    }

    #[test]
    fn test_oedit_type_completion() {
        let commands = vec!["oedit".to_string()];
        let item_vnums = vec!["town:sword".to_string()];

        // Complete item type after "type"
        let result = complete(
            "oedit town:sword type ar",
            24,
            &commands,
            &[],
            &item_vnums,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"armor".to_string()));
        assert_eq!(result.completion_type, CompletionType::ItemType);
    }

    #[test]
    fn test_oedit_trigger_action_completion() {
        let commands = vec!["oedit".to_string()];
        let item_vnums = vec!["town:sword".to_string()];

        // Complete trigger action after "trigger"
        let result = complete(
            "oedit town:sword trigger a",
            26,
            &commands,
            &[],
            &item_vnums,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"add".to_string()));
        assert_eq!(result.completion_type, CompletionType::ItemTriggerAction);
    }

    #[test]
    fn test_oedit_trigger_type_completion() {
        let commands = vec!["oedit".to_string()];
        let item_vnums = vec!["town:sword".to_string()];

        // Complete trigger type after "add"
        let result = complete(
            "oedit town:sword trigger add ge",
            31,
            &commands,
            &[],
            &item_vnums,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"get".to_string()));
        assert_eq!(result.completion_type, CompletionType::ItemTriggerType);
    }

    #[test]
    fn test_redit_subcommand_completion() {
        let commands = vec!["redit".to_string()];

        // Complete subcommand
        let result = complete(
            "redit tr",
            8,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"trigger".to_string()));
        assert_eq!(result.completion_type, CompletionType::ReditSubcommand);
    }

    #[test]
    fn test_redit_flag_completion() {
        let commands = vec!["redit".to_string()];

        // Complete flag name after "flag"
        let result = complete(
            "redit flag da",
            13,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"dark".to_string()));
        assert_eq!(result.completion_type, CompletionType::RoomFlag);
    }

    #[test]
    fn test_redit_extra_action_completion() {
        let commands = vec!["redit".to_string()];

        // Complete extra action
        let result = complete(
            "redit extra li",
            14,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"list".to_string()));
        assert_eq!(result.completion_type, CompletionType::ExtraDescAction);
    }

    #[test]
    fn test_redit_trigger_action_completion() {
        let commands = vec!["redit".to_string()];

        // Complete trigger action
        let result = complete(
            "redit trigger a",
            15,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"add".to_string()));
        assert_eq!(result.completion_type, CompletionType::RoomTriggerAction);
    }

    #[test]
    fn test_redit_trigger_type_completion() {
        let commands = vec!["redit".to_string()];

        // Complete trigger type after "add"
        let result = complete(
            "redit trigger add en",
            20,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"enter".to_string()));
        assert_eq!(result.completion_type, CompletionType::RoomTriggerType);
    }

    #[test]
    fn test_aedit_subcommand_completion() {
        let commands = vec!["aedit".to_string()];
        let area_prefixes = vec!["town".to_string()];

        // Complete subcommand after area
        let result = complete(
            "aedit town pe",
            13,
            &commands,
            &[],
            &[],
            &[],
            &area_prefixes,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"permission".to_string()));
        assert_eq!(result.completion_type, CompletionType::AeditSubcommand);
    }

    #[test]
    fn test_aedit_permission_completion() {
        let commands = vec!["aedit".to_string()];
        let area_prefixes = vec!["town".to_string()];

        // Complete permission level
        let result = complete(
            "aedit town permission ow",
            24,
            &commands,
            &[],
            &[],
            &[],
            &area_prefixes,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"owner_only".to_string()));
        assert_eq!(result.completion_type, CompletionType::PermissionLevel);
    }

    #[test]
    fn test_spedit_subcommand_completion() {
        let commands = vec!["spedit".to_string()];

        // Complete subcommand (no area prefix in new syntax)
        let result = complete(
            "spedit cr",
            9,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"create".to_string()));
        assert_eq!(result.completion_type, CompletionType::SpeditSubcommand);
    }

    #[test]
    fn test_spedit_list_filter_completion() {
        let commands = vec!["spedit".to_string()];

        // Complete filter after "list"
        let result = complete(
            "spedit list mo",
            14,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"mobs".to_string()));
        assert_eq!(result.completion_type, CompletionType::SpeditFilter);
    }

    #[test]
    fn test_spedit_room_vnum_completion() {
        let commands = vec!["spedit".to_string()];
        let room_vnums = vec!["town:plaza".to_string(), "town:market".to_string()];

        // Complete room vnum after "create"
        let result = complete(
            "spedit create town:p",
            20,
            &commands,
            &room_vnums,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"town:plaza".to_string()));
        assert_eq!(result.completion_type, CompletionType::RoomVnum);
    }

    #[test]
    fn test_spedit_entity_type_completion() {
        let commands = vec!["spedit".to_string()];
        let room_vnums = vec!["town:plaza".to_string()];

        // Complete entity type after "create <room>"
        let result = complete(
            "spedit create town:plaza mo",
            27,
            &commands,
            &room_vnums,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"mobile".to_string()));
        assert_eq!(result.completion_type, CompletionType::SpawnEntityType);
    }

    #[test]
    fn test_spedit_mobile_vnum_completion() {
        let commands = vec!["spedit".to_string()];
        let room_vnums = vec!["town:plaza".to_string()];
        let mobile_vnums = vec!["town:guard".to_string(), "town:merchant".to_string()];

        // Complete mobile vnum after "create <room> mobile"
        let result = complete(
            "spedit create town:plaza mobile town:g",
            38,
            &commands,
            &room_vnums,
            &[],
            &mobile_vnums,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"town:guard".to_string()));
        assert_eq!(result.completion_type, CompletionType::MobileVnum);
    }

    #[test]
    fn test_spedit_item_vnum_completion() {
        let commands = vec!["spedit".to_string()];
        let room_vnums = vec!["town:plaza".to_string()];
        let item_vnums = vec!["town:sword".to_string(), "town:shield".to_string()];

        // Complete item vnum after "create <room> item"
        let result = complete(
            "spedit create town:plaza item town:sw",
            37,
            &commands,
            &room_vnums,
            &item_vnums,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"town:sword".to_string()));
        assert_eq!(result.completion_type, CompletionType::ItemVnum);
    }

    #[test]
    fn test_spedit_delete_filter_completion() {
        let commands = vec!["spedit".to_string()];

        // Complete filter after "delete"
        let result = complete(
            "spedit delete mo",
            16,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"mobs".to_string()));
        assert_eq!(result.completion_type, CompletionType::SpeditFilter);
    }

    #[test]
    fn test_spedit_dep_filter_and_action_completion() {
        let commands = vec!["spedit".to_string()];

        // Complete filter OR dep action after "dep" (both should be offered)
        let result = complete(
            "spedit dep a",
            12,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"all".to_string())); // filter
        assert!(result.completions.contains(&"add".to_string())); // dep action
    }

    #[test]
    fn test_spedit_dep_with_filter_action_completion() {
        let commands = vec!["spedit".to_string()];

        // Complete dep action after "dep mobs"
        let result = complete(
            "spedit dep mobs a",
            17,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"add".to_string()));
        assert_eq!(result.completion_type, CompletionType::SpeditDepAction);
    }

    #[test]
    fn test_set_subcommand_completion_non_builder() {
        let commands = vec!["set".to_string()];

        // Non-builder should see mxp, color, and afk but NOT roomflags
        let result = complete(
            "set ",
            4,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"mxp".to_string()));
        assert!(result.completions.contains(&"color".to_string()));
        assert!(result.completions.contains(&"afk".to_string()));
        assert!(!result.completions.contains(&"roomflags".to_string()));
        assert_eq!(result.completion_type, CompletionType::SetSubcommand);
    }

    #[test]
    fn test_set_subcommand_completion_builder() {
        let commands = vec!["set".to_string()];

        // Builder should see all settings including roomflags
        let result = complete(
            "set ",
            4,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            true,
        );
        assert!(result.completions.contains(&"mxp".to_string()));
        assert!(result.completions.contains(&"color".to_string()));
        assert!(result.completions.contains(&"afk".to_string()));
        assert!(result.completions.contains(&"roomflags".to_string()));
        assert_eq!(result.completion_type, CompletionType::SetSubcommand);
    }

    #[test]
    fn test_set_partial_completion() {
        let commands = vec!["set".to_string()];

        // Complete partial setting name
        let result = complete(
            "set m",
            5,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"mxp".to_string()));
        assert!(!result.completions.contains(&"color".to_string()));
        assert_eq!(result.completion_type, CompletionType::SetSubcommand);
    }

    #[test]
    fn test_set_toggle_value_completion() {
        let commands = vec!["set".to_string()];

        // After setting name, show on/off
        let result = complete(
            "set mxp ",
            8,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"on".to_string()));
        assert!(result.completions.contains(&"off".to_string()));
        assert_eq!(result.completion_type, CompletionType::SetSubcommand);
    }

    #[test]
    fn test_set_toggle_partial_completion() {
        let commands = vec!["set".to_string()];

        // Complete partial toggle value
        let result = complete(
            "set mxp o",
            9,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            false,
        );
        assert!(result.completions.contains(&"on".to_string()));
        assert!(result.completions.contains(&"off".to_string()));
        assert_eq!(result.completion_type, CompletionType::SetSubcommand);
    }

    #[test]
    fn test_treat_target_completion() {
        let commands = vec!["treat".to_string()];
        let online_players = vec!["Alice".to_string(), "Bob".to_string()];

        // After "treat" show self and online players
        let result = complete(
            "treat ",
            6,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &online_players,
            false,
        );
        assert!(result.completions.contains(&"self".to_string()));
        assert!(result.completions.contains(&"Alice".to_string()));
        assert!(result.completions.contains(&"Bob".to_string()));
        assert_eq!(result.completion_type, CompletionType::TreatTarget);
    }

    #[test]
    fn test_treat_target_partial_completion() {
        let commands = vec!["treat".to_string()];
        let online_players = vec!["Alice".to_string(), "Bob".to_string()];

        // Complete partial target
        let result = complete(
            "treat se",
            8,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &online_players,
            false,
        );
        assert!(result.completions.contains(&"self".to_string()));
        assert!(!result.completions.contains(&"Alice".to_string()));
        assert_eq!(result.completion_type, CompletionType::TreatTarget);
    }

    #[test]
    fn test_treat_body_part_completion() {
        let commands = vec!["treat".to_string()];
        let online_players = vec!["Alice".to_string()];

        // After target, show body parts and conditions
        let result = complete(
            "treat self ",
            11,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &online_players,
            false,
        );
        assert!(result.completions.contains(&"head".to_string()));
        assert!(result.completions.contains(&"torso".to_string()));
        assert!(result.completions.contains(&"hypothermia".to_string()));
        assert!(result.completions.contains(&"illness".to_string()));
        assert_eq!(result.completion_type, CompletionType::BodyPart);
    }

    #[test]
    fn test_treat_body_part_partial_completion() {
        let commands = vec!["treat".to_string()];
        let online_players = vec!["Alice".to_string()];

        // Complete partial body part
        let result = complete(
            "treat self le",
            13,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &online_players,
            false,
        );
        assert!(result.completions.contains(&"leftarm".to_string()));
        assert!(result.completions.contains(&"leftleg".to_string()));
        assert!(result.completions.contains(&"lefthand".to_string()));
        assert!(result.completions.contains(&"leftfoot".to_string()));
        assert!(!result.completions.contains(&"head".to_string()));
        assert_eq!(result.completion_type, CompletionType::BodyPart);
    }

    #[test]
    fn test_treat_condition_partial_completion() {
        let commands = vec!["treat".to_string()];
        let online_players = vec!["Alice".to_string()];

        // Complete partial condition - should get only conditions
        let result = complete(
            "treat self heat",
            15,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &online_players,
            false,
        );
        assert!(result.completions.contains(&"heat_exhaustion".to_string()));
        assert!(result.completions.contains(&"heat_stroke".to_string()));
        assert!(!result.completions.contains(&"head".to_string()));
        assert_eq!(result.completion_type, CompletionType::TreatableCondition);
    }

    #[test]
    fn test_mail_subcommand_completion() {
        let commands = vec!["mail".to_string()];
        let online_players = vec!["Alice".to_string(), "Bob".to_string()];

        // Complete subcommand
        let result = complete(
            "mail ",
            5,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &online_players,
            false,
        );
        assert!(result.completions.contains(&"check".to_string()));
        assert!(result.completions.contains(&"list".to_string()));
        assert!(result.completions.contains(&"read".to_string()));
        assert!(result.completions.contains(&"send".to_string()));
        assert!(result.completions.contains(&"compose".to_string()));
        assert!(result.completions.contains(&"delete".to_string()));
        assert!(result.completions.contains(&"reply".to_string()));
        assert_eq!(result.completion_type, CompletionType::MailSubcommand);

        // Complete partial subcommand
        let result = complete(
            "mail se",
            7,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &online_players,
            false,
        );
        assert!(result.completions.contains(&"send".to_string()));
        assert!(!result.completions.contains(&"list".to_string()));
        assert_eq!(result.completion_type, CompletionType::MailSubcommand);

        // Complete player name for send
        let result = complete(
            "mail send ",
            10,
            &commands,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &online_players,
            false,
        );
        assert!(result.completions.contains(&"Alice".to_string()));
        assert!(result.completions.contains(&"Bob".to_string()));
        assert_eq!(result.completion_type, CompletionType::PlayerName);
    }
}

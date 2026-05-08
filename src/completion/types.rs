//! Core types for the tab completion engine.

use super::helpers::find_common_prefix;

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
    CombatSpellsAction,
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
    /// Language key/name argument
    Language,
    /// Mob keyword from the player's current room (for `talk`)
    MobInRoom,
}

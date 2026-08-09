//! Core types for the tab completion engine.

use super::helpers::find_common_prefix;

/// What kind of thing a completion candidate is, for display purposes only.
/// Never affects matching, ordering, or dispatch — only the colour the
/// candidate is painted with in the multi-match list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompletionCategory {
    #[default]
    Plain,
    /// A virtual social entry (`CommandMeta.kind == Some("social")`).
    Social,
    // Reserved for later: Builder, Admin, Contextual.
}

/// Result of a completion request
#[derive(Debug, Clone)]
pub struct CompletionResult {
    /// List of possible completions
    pub completions: Vec<String>,
    /// Display category per completion. Always the same length as
    /// `completions`; all-`Plain` unless a completer says otherwise.
    pub categories: Vec<CompletionCategory>,
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
            categories: Vec::new(),
            common_prefix: String::new(),
            completion_type: CompletionType::None,
            partial: String::new(),
        }
    }

    pub fn new(completions: Vec<String>, partial: &str, completion_type: CompletionType) -> Self {
        let categories = vec![CompletionCategory::Plain; completions.len()];
        Self::new_categorized(completions, categories, partial, completion_type)
    }

    /// Like `new`, but each candidate carries a display category.
    pub fn new_categorized(
        completions: Vec<String>,
        categories: Vec<CompletionCategory>,
        partial: &str,
        completion_type: CompletionType,
    ) -> Self {
        debug_assert_eq!(
            completions.len(),
            categories.len(),
            "completion categories must be 1:1 with completions"
        );
        let common_prefix = find_common_prefix(&completions);
        Self {
            completions,
            categories,
            common_prefix,
            completion_type,
            partial: partial.to_string(),
        }
    }

    /// Category for candidate `i`, defaulting to `Plain` if the categories
    /// vec is short (belt-and-braces for a hand-built result).
    pub fn category_at(&self, i: usize) -> CompletionCategory {
        self.categories.get(i).copied().unwrap_or_default()
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
    TriggerDgSubcommand,
    TriggerDgProtoSubcommand,
    DgMobileTriggerType,
    DgItemTriggerType,
    DgRoomTriggerType,
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
    /// `admin loadout <class|race> <id>` — a class or race id.
    LoadoutId,
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
    PromptSubcommand,
    /// `top <board>` — a leaderboard name.
    TopBoard,
    /// `build <subcommand>`.
    BuildSubcommand,
    /// `build audit <target>` — what to grade.
    BuildAuditTarget,
    /// `build audit quest <vnum>`.
    QuestVnum,
    /// `world <subcommand>`.
    WorldSubcommand,
    /// `bounty <subcommand>`.
    BountySubcommand,
    /// `standing <faction>` — a declared faction key.
    FactionKey,
    /// `locate <what>` — the things a divination can find.
    LocateTarget,
    /// `consignments <subcommand>`.
    ConsignmentsSubcommand,
    BugsSubcommand,
    BugStatusFilter,
    BugPriorityValue,
    DamageType,
    /// `medit <vnum> creature <type>` — base biology (mortal/animal/...).
    CreatureType,
    /// `oedit <vnum> affect <subcmd>` — list/add/rm/clear.
    AffectAction,
    /// `oedit <vnum> affect add <effect>` — snake_case EffectType.
    EffectType,
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
    AcheditSubcommand,
    AchievementCategory,
    AchievementRewardAction,
    AchievementCriterionAction,
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

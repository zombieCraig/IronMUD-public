//! Trigger types for rooms, items, and mobiles

use serde::{Deserialize, Serialize};

/// Which kind of host entity a DG trigger prototype attaches to.
/// Mirrors the `attach_type` byte from `.trg` headers (0=mob, 1=obj, 2=room).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DgAttachKind {
    Mob,
    Obj,
    Room,
}

/// A DG Scripts trigger prototype, looked up by vnum at runtime by the
/// `attach <vnum> <target>` command (and the builder `trigger dg attach`
/// subcommand). Imported `.trg` files seed this registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DgTriggerProto {
    pub vnum: String,
    pub name: String,
    pub attach_kind: DgAttachKind,
    /// Letter-flag string from the `.trg` header (e.g. `g` for greet,
    /// `q` for command). Resolved via `tba::trg_map` at attach time.
    pub flags: String,
    /// Numeric arg / priority — used as `chance` on the resulting trigger
    /// (or as the HP-percent threshold for `MTRIG_HITPRCNT`).
    #[serde(default = "default_proto_numeric")]
    pub numeric_arg: i32,
    /// Single-line argument string (verb keyword for COMMAND triggers,
    /// keyword list for SPEECH triggers, etc.).
    #[serde(default)]
    pub arglist: String,
    /// Raw DG body — copied into `dg_body` on the resulting trigger.
    pub body: String,
}

fn default_proto_numeric() -> i32 {
    100
}

// === Room Trigger System ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerType {
    OnEnter,
    OnExit,
    OnLook,
    Periodic,
    OnTimeChange,    // Fires when time of day changes (dawn, dusk, etc.)
    OnWeatherChange, // Fires when weather changes (rain starts, clears up, etc.)
    OnSeasonChange,  // Fires when season changes (spring, summer, autumn, winter)
    OnMonthChange,   // Fires when month changes (1-12)
    /// Fires when a player runs any command in this room. The trigger
    /// `args` first entry is matched against the command verb (DG keyword
    /// `/=` semantics). `Return(0)` cancels the host command. (DG WTRIG_COMMAND)
    OnCommand,
}

impl Default for TriggerType {
    fn default() -> Self {
        TriggerType::OnEnter
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomTrigger {
    pub trigger_type: TriggerType,
    pub script_name: String, // e.g., "forest_trap" or "@say_greeting" for templates
    pub enabled: bool,
    #[serde(default = "default_trigger_interval")]
    pub interval_secs: i64, // For periodic triggers (default 60)
    #[serde(default)]
    pub last_fired: i64, // Unix timestamp of last execution
    #[serde(default = "default_trigger_chance")]
    pub chance: i32, // 1-100 percent (100 = always)
    #[serde(default)]
    pub args: Vec<String>, // Template arguments
    /// DG Scripts source body (from imported `.trg` files). When present,
    /// the fire path routes through the DG interpreter instead of
    /// `script_name`. Builders can author either flavor.
    #[serde(default)]
    pub dg_body: Option<String>,
    /// Human-readable trigger name from the `.trg` header (e.g.
    /// "Mage Guildguard - 3024"). Display only.
    #[serde(default)]
    pub dg_name: Option<String>,
    /// Character name of the builder who authored / last edited the
    /// `dg_body`. Empty (None) means importer-seeded or system-authored.
    /// Used by the DG opcode permission gate to scope dangerous verbs
    /// (force/at/purge/load/teleport) to the author's area.
    #[serde(default)]
    pub authored_by: Option<String>,
    /// Admin override that lifts the per-author area gate on dangerous
    /// DG opcodes for this trigger. Only `is_admin` characters can flip
    /// this on (via `redit/medit/oedit trigger dg elevate`).
    #[serde(default)]
    pub elevated: bool,
    /// Backreference to the [`DgTriggerProto`] this trigger was attached
    /// from. Set by `attach <vnum>` (and the builder `trigger dg attach`
    /// subcommand); empty for host-local triggers authored directly.
    /// When present, editing this instance saves through to the proto and
    /// refreshes all sibling instances. `trigger dg detach <idx>` clears
    /// the link to allow per-instance divergence.
    #[serde(default)]
    pub source_proto_vnum: Option<String>,
}

fn default_trigger_interval() -> i64 {
    60
}

fn default_trigger_chance() -> i32 {
    100
}

impl Default for RoomTrigger {
    fn default() -> Self {
        RoomTrigger {
            trigger_type: TriggerType::OnEnter,
            script_name: String::new(),
            enabled: true,
            interval_secs: 60,
            last_fired: 0,
            chance: 100,
            args: Vec::new(),
            dg_body: None,
            dg_name: None,
            authored_by: None,
            elevated: false,
            source_proto_vnum: None,
        }
    }
}

// === Item Trigger System ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemTriggerType {
    OnGet,
    OnDrop,
    OnUse,
    OnExamine,
    /// Fires when a player `look`s at the item in the room or container.
    /// Mirror of `OnExamine` for the casual-glance verb. `Return(0)` cancels
    /// the default look output for that item.
    OnLook,
    OnPrompt, // Fires when building prompt for equipped items
    /// Fires when an item is loaded (spawned from prototype). (DG OTRIG_LOAD)
    OnLoad,
    /// Fires when a player runs any command while this item is in their
    /// inventory or equipped. `Return(0)` cancels the host command. (DG OTRIG_COMMAND)
    OnCommand,
    /// Fires when an item is equipped via `wear` (armor/clothing/jewelry) or
    /// DG mob `wear`. Buff stamping is automatic via
    /// `db::stamp_item_buffs_on_character`; this trigger is for ad-hoc
    /// script side effects (emotes, room messages, etc.). See [`OnWield`]
    /// for the weapon/held-item counterpart fired by the `wield` command.
    OnWear,
    /// Fires when an item is removed from equipped slots.
    OnRemove,
    /// Fires when an item is equipped via the `wield` command (weapons,
    /// off-hand items, fishing rods — anything with a `wielded`/`offhand`
    /// wear location). Distinct from [`OnWear`] so weapon-specific triggers
    /// (achievements, bound-weapon awakening, etc.) don't fire on armor.
    OnWield,
}

impl Default for ItemTriggerType {
    fn default() -> Self {
        ItemTriggerType::OnGet
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemTrigger {
    pub trigger_type: ItemTriggerType,
    pub script_name: String, // e.g., "cursed_item" or "@say_greeting" for templates
    pub enabled: bool,
    #[serde(default = "default_trigger_chance")]
    pub chance: i32, // 1-100 percent (100 = always)
    #[serde(default)]
    pub args: Vec<String>, // Template arguments
    #[serde(default)]
    pub dg_body: Option<String>,
    #[serde(default)]
    pub dg_name: Option<String>,
    /// See [`RoomTrigger::authored_by`].
    #[serde(default)]
    pub authored_by: Option<String>,
    /// See [`RoomTrigger::elevated`].
    #[serde(default)]
    pub elevated: bool,
    /// See [`RoomTrigger::source_proto_vnum`].
    #[serde(default)]
    pub source_proto_vnum: Option<String>,
}

impl Default for ItemTrigger {
    fn default() -> Self {
        ItemTrigger {
            trigger_type: ItemTriggerType::OnGet,
            script_name: String::new(),
            enabled: true,
            chance: 100,
            args: Vec::new(),
            dg_body: None,
            dg_name: None,
            authored_by: None,
            elevated: false,
            source_proto_vnum: None,
        }
    }
}

// === Mobile/NPC Trigger System ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MobileTriggerType {
    OnGreet,  // Player enters room with NPC
    OnAttack, // NPC is attacked
    OnDeath,  // NPC dies
    OnSay,    // Player says something in room (for advanced dialogue)
    OnIdle,   // Periodic when players present in room
    OnAlways, // Periodic regardless of player presence
    OnFlee,   // Mobile flees from combat
    /// Fires once per round while NPC is in combat. (DG MTRIG_FIGHT)
    OnFight,
    /// Fires when NPC HP drops below the threshold percent stored in
    /// `args[0]` (a number 1-99). One-shot per crossing. (DG MTRIG_HITPRCNT)
    OnHitPercent,
    /// Fires when a player gives an item to this NPC. (DG MTRIG_RECEIVE)
    OnReceive,
    /// Fires when a player gives gold to this NPC. (DG MTRIG_BRIBE)
    OnBribe,
    /// Fires when an NPC is loaded (spawned from prototype). (DG MTRIG_LOAD)
    OnLoad,
    /// Fires when a player in the same room runs any command. The trigger
    /// `args` first entry is matched against the command verb (DG keyword
    /// `/=` semantics). `Return(0)` cancels the host command. (DG MTRIG_COMMAND)
    OnCommand,
    /// Fires on the worshiped god's mob when a worshiper prays in a temple.
    /// Context vars: `action` ("pray" | "tribute"), `overdue_days`.
    /// `Return(0)` cancels the default tribute/blessing handling.
    OnPray,
    /// Fires on the god's mob when a player forms a worship pact with it.
    OnWorshipPact,
    /// Fires on the worshiped god's mob when the anger ladder smites a
    /// worshiper. Context vars: `severity` (1-4), `overdue_days`.
    /// `Return(0)` cancels the default smite.
    OnSmite,
}

impl Default for MobileTriggerType {
    fn default() -> Self {
        MobileTriggerType::OnGreet
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileTrigger {
    pub trigger_type: MobileTriggerType,
    pub script_name: String, // e.g., "guard_greet" or "@say_greeting" for templates
    pub enabled: bool,
    #[serde(default = "default_trigger_chance")]
    pub chance: i32, // 1-100 percent (100 = always)
    #[serde(default)]
    pub args: Vec<String>, // Template arguments (e.g., greeting message)
    #[serde(default = "default_trigger_interval")]
    pub interval_secs: i64, // For OnIdle triggers (default 60)
    #[serde(default)]
    pub last_fired: i64, // Unix timestamp of last execution
    #[serde(default)]
    pub dg_body: Option<String>,
    #[serde(default)]
    pub dg_name: Option<String>,
    /// See [`RoomTrigger::authored_by`].
    #[serde(default)]
    pub authored_by: Option<String>,
    /// See [`RoomTrigger::elevated`].
    #[serde(default)]
    pub elevated: bool,
    /// See [`RoomTrigger::source_proto_vnum`].
    #[serde(default)]
    pub source_proto_vnum: Option<String>,
}

impl Default for MobileTrigger {
    fn default() -> Self {
        MobileTrigger {
            trigger_type: MobileTriggerType::OnGreet,
            script_name: String::new(),
            enabled: true,
            chance: 100,
            args: Vec::new(),
            interval_secs: 60,
            last_fired: 0,
            dg_body: None,
            dg_name: None,
            authored_by: None,
            elevated: false,
            source_proto_vnum: None,
        }
    }
}

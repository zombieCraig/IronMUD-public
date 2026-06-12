//! Dialogue tree system types: trees, nodes, choices, conditions, effects,
//! and per-(player, mob) state.

use super::serde_defaults::default_qty_one;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DialogueTree {
    pub root_node: String,
    pub nodes: HashMap<String, DialogueNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueNode {
    pub text: String,
    #[serde(default)]
    pub choices: Vec<DialogueChoice>,
    /// Effects fired only on the FIRST visit to this node by a given player.
    /// Subsequent visits skip these — track via DialoguePairState.visit_counts.
    #[serde(default)]
    pub on_enter: Vec<DialogueEffect>,
    /// Effects fired on EVERY entry to this node, including the first.
    #[serde(default)]
    pub on_each_visit: Vec<DialogueEffect>,
    /// Effects fired when the player leaves this node (Goto away or Exit).
    #[serde(default)]
    pub on_exit: Vec<DialogueEffect>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueChoice {
    pub keyword: String,
    pub label: String,
    pub target: DialogueTarget,
    #[serde(default)]
    pub conditions: Vec<DialogueCondition>,
    #[serde(default)]
    pub effects: Vec<DialogueEffect>,
    /// When `conditions` fail, this string is surfaced in the menu in place of
    /// the choice line. If `None`, the choice is hidden silently (default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// After the player picks this choice, they cannot pick it again until this
    /// many seconds have elapsed. `None` means no cooldown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooldown_secs: Option<i64>,
    /// When set, this choice can only be picked once per player per mob vnum.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub once_per_player: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DialogueTarget {
    Goto { node: String },
    Exit,
    Repeat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DialogueCondition {
    FlagSet {
        name: String,
        #[serde(default)]
        scope: FlagScope,
    },
    FlagUnset {
        name: String,
        #[serde(default)]
        scope: FlagScope,
    },
    HasItem {
        vnum: String,
        #[serde(default = "default_qty_one")]
        qty: i32,
    },
    SkillAtLeast {
        key: String,
        level: i32,
    },
    CounterAtLeast {
        key: String,
        value: i32,
    },
    DgVarEquals {
        scope: DgScope,
        key: String,
        value: String,
    },
    /// True when the player has the named quest in their `active_quests` map.
    QuestActive {
        vnum: String,
    },
    /// True when the named quest is in `completed_quests`.
    QuestComplete {
        vnum: String,
    },
    /// True when the named quest is active AND every objective is satisfied
    /// (used to gate the "ready to turn in" dialogue branch).
    QuestCompletable {
        vnum: String,
    },
    /// True when the speaker is a vampire AND humanity >= threshold.
    /// Mortals fail unconditionally — there's no humanity to compare. Use
    /// for NPC reactions that should only fire for kindred above (or below,
    /// via FlagUnset gating) a certain moral standing.
    HumanityAtLeast {
        threshold: i32,
    },
    /// True when the speaker is an embraced vampire who has not yet been
    /// acknowledged by a clan (no `clan_*` trait). The classic newly-sired
    /// state — sire NPCs gate offered embrace quests on this. False for
    /// mortals and clan-acknowledged kindred alike.
    IsThinblood,
    /// Inverse: true only for embraced vampires who carry a `clan_*` trait.
    /// Useful for sire-NPC branches that should only fire after the player
    /// has completed the embrace progression.
    IsClanAcknowledged,
    /// True when the speaker has the named achievement unlocked. Reads
    /// `CharacterData.achievements_unlocked` — the same map that
    /// `QuestReward::Achievement` writes to. Lets dialogue trees gate on
    /// long-term progression milestones (e.g. "met all sires") that survive
    /// quest cleanup and clan respec.
    HasAchievement {
        key: String,
    },
    /// True when the player's `ActiveQuest.choice_vars[key]` for `quest_vnum`
    /// equals `value`. Lets dialogue branches confirm or inspect a prior
    /// choice the player made earlier in the same tree (e.g. "Casey nods —
    /// so you've chosen Potence.").
    QuestChoiceEquals {
        quest_vnum: String,
        key: String,
        value: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DialogueEffect {
    SetFlag {
        name: String,
        #[serde(default)]
        scope: FlagScope,
    },
    ClearFlag {
        name: String,
        #[serde(default)]
        scope: FlagScope,
    },
    GiveItem {
        vnum: String,
        #[serde(default = "default_qty_one")]
        qty: i32,
    },
    TakeItem {
        vnum: String,
        #[serde(default = "default_qty_one")]
        qty: i32,
    },
    AwardSkillXp {
        skill: String,
        amount: i32,
    },
    SetCounter {
        key: String,
        value: i32,
    },
    IncrementCounter {
        key: String,
        #[serde(default = "default_qty_one")]
        by: i32,
    },
    SetDgVar {
        scope: DgScope,
        key: String,
        value: String,
    },
    FireDgTrigger {
        trigger_type: String,
        #[serde(default)]
        arg: String,
    },
    /// Add the quest to the player's `active_quests` if not already active and
    /// not completed (or quest is repeatable).
    OfferQuest {
        vnum: String,
    },
    /// Try to complete the quest and grant rewards. No-op (with a feedback
    /// line) if objectives aren't met yet.
    CompleteQuest {
        vnum: String,
    },
    /// Drop the quest from `active_quests` (progress lost).
    AbandonQuest {
        vnum: String,
    },
    /// Write a value to the player's `ActiveQuest.choice_vars` map for the
    /// named quest. No-op (with a warn-log) if the quest isn't active —
    /// dialogue trees should `OfferQuest` first, then `SetQuestChoice`.
    SetQuestChoice {
        quest_vnum: String,
        key: String,
        value: String,
    },
    /// Install a cyberware item the player is carrying (matched by vnum),
    /// charging Humanity via the `install_piece` capability and consuming the
    /// item. No-op-with-message on validation failure (missing foundation, no
    /// free slots, exclusive clash, incompatible race…). The intended ripperdoc
    /// flow: the player buys the chrome from the shop, then a dialogue choice
    /// (gated on `has_item`) installs it.
    InstallCyberware {
        vnum: String,
    },
    /// Restore Humanity points (`apply_therapy`). Gold pricing lives in the
    /// shop: gate the choice on a purchasable voucher and pair this with a
    /// `take_item` on the same choice.
    CyberwareTherapy {
        #[serde(default = "default_qty_one")]
        points: i32,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FlagScope {
    #[default]
    Local,
    Global,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DgScope {
    Player,
    Mob,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DialoguePairState {
    pub current_node: Option<String>,
    #[serde(default)]
    pub last_seen_secs: i64,
    /// How many times this player has entered each named node. Used to gate
    /// `on_enter` (first-visit only) versus `on_each_visit`.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub visit_counts: HashMap<String, u32>,
    /// Per-choice cooldown timestamps (epoch seconds of last pick). Key shape
    /// is `"<node>:<keyword>"`.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub choice_cooldowns: HashMap<String, i64>,
    /// Choices that this player has already picked once and are flagged
    /// `once_per_player`. Same `"<node>:<keyword>"` key shape.
    #[serde(default, skip_serializing_if = "std::collections::HashSet::is_empty")]
    pub choices_picked_once: std::collections::HashSet<String>,
}

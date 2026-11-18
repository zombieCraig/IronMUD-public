//! Divine spellcraft records: spells that live in the `codex` sled tree
//! rather than in the preset JSON files.
//!
//! A codex spell is an ordinary `SpellDefinition` plus `CodexMeta`, which
//! says who made it, which god it belongs to and where it stands in that
//! god's canon. `load_game_data` overlays every codex `def` into
//! `World.spell_definitions` (with `codex = true`), so once loaded a codex
//! spell casts like any JSON spell. See `crate::spellcraft`.

use serde::{Deserialize, Serialize};

use super::SpellDefinition;

/// Codex spell ids carry this prefix so they can never collide with a
/// preset JSON id.
pub const CODEX_ID_PREFIX: &str = "cx_";

/// One codex record: the spell itself and its history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexEntry {
    pub def: SpellDefinition,
    #[serde(default)]
    pub meta: CodexMeta,
}

/// Where a codex spell came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SpellOrigin {
    /// Seeded world history; no living author.
    #[default]
    Ancient,
    /// Sealed by a player through a god's ritual.
    Authored,
}

/// The spell's standing in its god's canon (cached here for display; the
/// canon record per god is the source of truth).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CanonStatus {
    /// Held in the god's canon: taught at the temple.
    #[default]
    Canon,
    /// Displaced from the canon. Still known and scribed, no longer taught.
    Faded,
    /// Nobody knows it and no scroll holds it.
    Lost,
}

/// Player-written lines shown when the spell is cast. Stored escaped and
/// length-capped (validated at inscription).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CastFlavor {
    /// What the caster sees.
    #[serde(default)]
    pub caster: String,
    /// What the room sees.
    #[serde(default)]
    pub room: String,
    /// What the target sees.
    #[serde(default)]
    pub target: String,
}

/// A change of authorship in a spell's history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LineageKind {
    #[default]
    Authored,
    Restored,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineageEvent {
    #[serde(default)]
    pub kind: LineageKind,
    /// Character name.
    #[serde(default)]
    pub who: String,
    /// Absolute game day (`GameTime::absolute_day`).
    #[serde(default)]
    pub day: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexMeta {
    #[serde(default)]
    pub origin: SpellOrigin,
    /// Current author (character name). Restoring a faded spell moves
    /// authorship to the restorer. None for Ancient spells.
    #[serde(default)]
    pub author: Option<String>,
    /// Vnum of the deity mob the spell belongs to. Every codex spell has one.
    #[serde(default)]
    pub god_vnum: String,
    #[serde(default)]
    pub status: CanonStatus,
    /// Absolute game day the spell was sealed (`GameTime::absolute_day`).
    #[serde(default)]
    pub sealed_day: i64,
    /// Authored/restored history, oldest first.
    #[serde(default)]
    pub lineage: Vec<LineageEvent>,
    /// Names of those who aided the sealing ritual.
    #[serde(default)]
    pub witnesses: Vec<String>,
    /// The author renounced the spell's god ("the Faithless").
    #[serde(default)]
    pub author_disgraced: bool,
    #[serde(default)]
    pub flavor: CastFlavor,
    /// Bumped on every change to `def`.
    #[serde(default)]
    pub version: u32,
    /// Hidden from player listings by an admin (`admin codex veto`).
    /// A vetoed spell is also Faded.
    #[serde(default)]
    pub vetoed: bool,
    /// Lifetime casts by anyone but the current author: the author's fame
    /// (`spells.canon_casts`). Drained in from World by the leaderboard tick.
    #[serde(default)]
    pub casts_by_others: u64,
}

/// One god's canon: the codex spells it currently holds and teaches, plus
/// the god's own petition clock. Stored in the `canons` tree keyed by god
/// vnum and written only through `Db::update_canon` (CAS).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Canon {
    #[serde(default)]
    pub god_vnum: String,
    #[serde(default)]
    pub slots: Vec<CanonSlot>,
    /// Absolute game day the god last accepted a petition.
    #[serde(default)]
    pub last_petition_day: Option<i64>,
    /// Display/leaderboard number: the sum of slot devotion.
    #[serde(default)]
    pub influence: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CanonSlot {
    #[serde(default)]
    pub spell_id: String,
    /// Absolute game day the spell entered the canon.
    #[serde(default)]
    pub sealed_day: i64,
    /// Decaying count of faithful casts.
    #[serde(default)]
    pub devotion: f64,
    /// The author has been told this slot is the next to go.
    #[serde(default)]
    pub warned: bool,
    /// Absolute game day `devotion` was last decayed to.
    #[serde(default)]
    pub devotion_day: i64,
}

/// What a god takes beyond the base HP pledge (§6). Only displayed and
/// frozen in S4; the pledges themselves are taken at sealing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PriceStyle {
    /// A further HP pledge.
    #[default]
    Flesh,
    /// A max-mana pledge.
    Mind,
    /// One point of a base attribute, for good.
    Essence,
    /// Gold, paid at sealing.
    Fortune,
    /// A max-stamina pledge and slower recovery.
    Weariness,
    /// One of the above, revealed only after sealing.
    Fate,
}

impl PriceStyle {
    pub const ALL: [PriceStyle; 6] = [
        PriceStyle::Flesh,
        PriceStyle::Mind,
        PriceStyle::Essence,
        PriceStyle::Fortune,
        PriceStyle::Weariness,
        PriceStyle::Fate,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            PriceStyle::Flesh => "flesh",
            PriceStyle::Mind => "mind",
            PriceStyle::Essence => "essence",
            PriceStyle::Fortune => "fortune",
            PriceStyle::Weariness => "weariness",
            PriceStyle::Fate => "fate",
        }
    }

    pub fn parse(s: &str) -> Option<PriceStyle> {
        PriceStyle::ALL.into_iter().find(|p| p.as_str().eq_ignore_ascii_case(s))
    }
}

/// One of a god's trials: the quest it sets when the drafted spell matches.
/// `None` fields match anything.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrialRule {
    pub quest_id: String,
    #[serde(default)]
    pub when_type: Option<String>,
    #[serde(default)]
    pub when_element: Option<String>,
    #[serde(default)]
    pub when_effect: Option<String>,
    /// Applies only to spells at least this strong.
    #[serde(default)]
    pub min_power: u32,
    /// A restoration trial (`pray restore`), not a petition's.
    #[serde(default)]
    pub restoration: bool,
}

/// The price of a drafted spell, as quoted at the petition.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Price {
    #[serde(default)]
    pub style: PriceStyle,
    /// Max-HP pledge.
    #[serde(default)]
    pub hp: i32,
    #[serde(default)]
    pub stamina: i32,
    #[serde(default)]
    pub mana: i32,
    /// Base attribute points lost for good (Essence).
    #[serde(default)]
    pub attribute: i32,
    #[serde(default)]
    pub gold: i64,
    /// Favor spent at sealing.
    #[serde(default)]
    pub favor: i64,
}

/// `category` × `qty`, each of at least `min_quality` (0 = any).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentReq {
    pub category: String,
    #[serde(default = "one")]
    pub qty: u32,
    #[serde(default)]
    pub min_quality: i32,
}

fn one() -> u32 {
    1
}

/// Which question the petition is asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DraftStep {
    #[default]
    Type,
    Element,
    Effect,
    Target,
    Magnitude,
    Duration,
    Name,
    Description,
    FlavorCaster,
    FlavorTarget,
    FlavorRoom,
    Review,
}

/// A spell being shaped in the petition mode. Session-only until offered;
/// then frozen into [`FrozenPetition`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpellDraft {
    pub god_vnum: String,
    #[serde(default)]
    pub step: DraftStep,
    #[serde(default)]
    pub spell_type: Option<String>,
    #[serde(default)]
    pub element: Option<String>,
    #[serde(default)]
    pub effect: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    /// Damage or heal amount, or buff/debuff magnitude.
    #[serde(default)]
    pub magnitude: Option<i32>,
    /// Buff/debuff seconds.
    #[serde(default)]
    pub duration: Option<i32>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub flavor: CastFlavor,
    /// Steps the player answered, in order; `back` pops the last one.
    /// Steps with a single possible answer fill themselves and aren't here.
    #[serde(default)]
    pub history: Vec<DraftStep>,
}

/// An offered petition, frozen: the god has set a trial and the draft can no
/// longer change. The ritual (S6) seals it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrozenPetition {
    pub draft: SpellDraft,
    /// The spell the draft describes (id `cx_<slug>`).
    pub spell: SpellDefinition,
    #[serde(default)]
    pub power: u32,
    #[serde(default)]
    pub tier: u8,
    #[serde(default)]
    pub price: Price,
    #[serde(default)]
    pub components: Vec<ComponentReq>,
    pub trial_quest_id: String,
    /// Absolute game day the god accepted it.
    #[serde(default)]
    pub offered_day: i64,
    /// Set for `pray restore`: the faded codex spell being re-lit. The rite
    /// cannot be pushed, and sealing re-lights that entry instead of
    /// writing a new one.
    #[serde(default)]
    pub restoring: Option<String>,
}

/// One brand in a god's mark pool (§6): typically one boon and one bane.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkTemplate {
    pub short_desc: String,
    #[serde(default)]
    pub body_part: String,
    #[serde(default)]
    pub affects: Vec<super::ItemAffect>,
}

/// A helper's `pray aid` in one stage.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AidRecord {
    pub helper: String,
    pub stage: u8,
}

/// A sealing ritual in progress (§5). Lives on the ritualist; advanced by
/// the ritual tick and `pray push`/`seal`/`aid` (`spellcraft::ritual`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RitualState {
    pub god_vnum: String,
    pub room_id: uuid::Uuid,
    /// Stages completed.
    #[serde(default)]
    pub stage: u8,
    /// Waiting for `pray push` or `pray seal`.
    #[serde(default)]
    pub at_boundary: bool,
    #[serde(default)]
    pub power_bonus: u8,
    #[serde(default)]
    pub pushes: u8,
    /// Dice rolled on the next push.
    #[serde(default)]
    pub risk: u8,
    /// Unix seconds the current stage ends (or the boundary next drains mana).
    #[serde(default)]
    pub next_stage_at: i64,
    /// Unix seconds the tick last saw the ritualist online.
    #[serde(default)]
    pub last_seen_at: i64,
    /// A minor disturbance this stage (+1 risk at the boundary).
    #[serde(default)]
    pub disturbed_minor: bool,
    /// Extra max-HP pledge added by pushes.
    #[serde(default)]
    pub pledge_accum: i32,
    /// Extra marks from backlashes.
    #[serde(default)]
    pub extra_marks: u8,
    /// Mana given by helpers, drawn before the ritualist's own.
    #[serde(default)]
    pub mana_pool: i32,
    #[serde(default)]
    pub aid: Vec<AidRecord>,
    /// Same-faith helpers, named on the codex entry.
    #[serde(default)]
    pub witnesses: Vec<String>,
    /// The components consumed at the start, as they were.
    #[serde(default)]
    pub claimed_components: Vec<super::ItemData>,
    /// The price to be paid at sealing; `Fate` already resolved.
    #[serde(default)]
    pub price: Price,
    /// Combat/sleep already counted as a major disturbance.
    #[serde(default)]
    pub noted_combat: bool,
    #[serde(default)]
    pub noted_sleep: bool,
}

/// Which ceiling a pledge lowers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PledgeKind {
    Hp,
    Stamina,
    Mana,
    /// Replicants' stand-in for stamina (theirs is pinned).
    Resolve,
}

/// How much of each ceiling a pledge holds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PledgeAmounts {
    #[serde(default)]
    pub hp: i32,
    #[serde(default)]
    pub stamina: i32,
    #[serde(default)]
    pub mana: i32,
    #[serde(default)]
    pub resolve: i32,
}

impl PledgeAmounts {
    pub fn get(&self, kind: PledgeKind) -> i32 {
        match kind {
            PledgeKind::Hp => self.hp,
            PledgeKind::Stamina => self.stamina,
            PledgeKind::Mana => self.mana,
            PledgeKind::Resolve => self.resolve,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.hp <= 0 && self.stamina <= 0 && self.mana <= 0 && self.resolve <= 0
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum PledgeStatus {
    /// Held in full while the spell stands.
    #[default]
    Active,
    /// Released (the spell left the canon, or was renounced): grows back
    /// over `spellcraft.regrow_days` from `from_day`.
    Regrowing { from_day: i64 },
    /// Taken for good (apostasy). Held in full.
    Forfeit,
    /// Fully grown back. Kept as history.
    Repaid,
}

fn full() -> u8 {
    100
}

fn default_floor() -> u8 {
    60
}

/// Part of a character's vigour held by a god for a spell (§6). Subtracted
/// from the ceilings at read time; see `CharacterData::pledged`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DivinePledge {
    pub id: String,
    #[serde(default)]
    pub spell_id: String,
    #[serde(default)]
    pub god_vnum: String,
    #[serde(default)]
    pub amounts: PledgeAmounts,
    #[serde(default)]
    pub made_day: i64,
    #[serde(default)]
    pub status: PledgeStatus,
    /// Percent of `amounts` still held. 100 unless regrowing; the regen tick
    /// keeps it current (`refresh_pledges`).
    #[serde(default = "full")]
    pub outstanding_pct: u8,
    /// No ceiling drops below this percent of its pre-pledge value
    /// (`spellcraft.pledge_floor_pct` when the pledge was made).
    #[serde(default = "default_floor")]
    pub floor_pct: u8,
}

impl DivinePledge {
    /// The amount of `kind` this pledge holds right now.
    pub fn holding(&self, kind: PledgeKind) -> i32 {
        let pct = match self.status {
            PledgeStatus::Active | PledgeStatus::Forfeit => 100,
            PledgeStatus::Regrowing { .. } => self.outstanding_pct.min(100) as i64,
            PledgeStatus::Repaid => 0,
        };
        ((self.amounts.get(kind) as i64 * pct) / 100) as i32
    }

    pub fn is_open(&self) -> bool {
        self.status != PledgeStatus::Repaid
    }
}

/// Percent of a released pledge still held `today`: 100 on the day it was
/// released, falling linearly to 0 after `regrow_days`.
pub fn regrow_pct(from_day: i64, today: i64, regrow_days: i64) -> u8 {
    if regrow_days <= 0 {
        return 0;
    }
    let elapsed = (today - from_day).clamp(0, regrow_days);
    (100 - elapsed * 100 / regrow_days) as u8
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkState {
    /// Its affects are stamped.
    #[default]
    Live,
    /// Cooled: shown, but stamps nothing.
    Faded,
    /// Live, with a second negative affect (apostasy).
    Worsened,
}

/// A god's memory of a character's offences against it (§10). Kept outside
/// `WorshipState`, so changing faith never wipes it, and it never decays:
/// only atonement (to your own god) or an admin clears one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grudge {
    pub count: u32,
    /// Absolute game day of the latest offence.
    pub last_day: i64,
}

/// A god the character once worshipped (§11).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormerFaith {
    pub god_vnum: String,
    pub from_day: i64,
    pub to_day: i64,
    /// Codex spells authored for that god.
    #[serde(default)]
    pub authored: Vec<String>,
    /// False for an admin clear; true for a renunciation.
    #[serde(default)]
    pub renounced: bool,
}

/// A god's brand on its spell's author (§6), modelled on tattoos: its
/// affects are stamped as permanent buffs sourced `mark:<id>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DivineMark {
    pub id: String,
    #[serde(default)]
    pub god_vnum: String,
    #[serde(default)]
    pub spell_id: Option<String>,
    #[serde(default)]
    pub body_part: String,
    pub short_desc: String,
    #[serde(default)]
    pub affects: Vec<super::ItemAffect>,
    /// Added when the mark worsens.
    #[serde(default)]
    pub worse_affect: Option<super::ItemAffect>,
    #[serde(default)]
    pub state: MarkState,
}

impl DivineMark {
    pub fn buff_source(&self) -> String {
        format!("mark:{}", self.id)
    }

    /// The affects this mark stamps in its current state.
    pub fn live_affects(&self) -> Vec<super::ItemAffect> {
        match self.state {
            MarkState::Faded => Vec::new(),
            MarkState::Live => self.affects.clone(),
            MarkState::Worsened => self.affects.iter().cloned().chain(self.worse_affect.clone()).collect(),
        }
    }

    /// How it reads on `examine`.
    pub fn display(&self) -> String {
        match self.state {
            MarkState::Faded => format!("{}, long cooled", self.short_desc),
            _ => self.short_desc.clone(),
        }
    }
}

impl Canon {
    /// True when sealing one more spell needs no displacement, or some slot
    /// is past its grace period and could be displaced.
    pub fn has_room(&self, cap: u32, today: i64, grace_days: i64) -> bool {
        (self.slots.len() as u32) < cap || self.slots.iter().any(|s| today >= s.sealed_day + grace_days)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_with_only_a_def_loads_with_default_meta() {
        let entry: CodexEntry = serde_json::from_value(serde_json::json!({
            "def": { "id": "cx_ember", "name": "Ember", "description": "A spark." }
        }))
        .expect("parse");
        assert_eq!(entry.meta, CodexMeta::default());
        assert_eq!(entry.meta.status, CanonStatus::Canon);
        assert_eq!(entry.meta.origin, SpellOrigin::Ancient);
    }

    #[test]
    fn regrow_ramp_is_exact() {
        assert_eq!(regrow_pct(10, 10, 14), 100);
        assert_eq!(regrow_pct(10, 17, 14), 50);
        assert_eq!(regrow_pct(10, 24, 14), 0);
        assert_eq!(regrow_pct(10, 99, 14), 0);
        assert_eq!(
            regrow_pct(10, 5, 14),
            100,
            "a clock behind the release day holds it all"
        );
        let mut p = DivinePledge {
            id: "p".into(),
            spell_id: String::new(),
            god_vnum: String::new(),
            amounts: PledgeAmounts {
                hp: 40,
                ..Default::default()
            },
            made_day: 0,
            status: PledgeStatus::Regrowing { from_day: 10 },
            outstanding_pct: 50,
            floor_pct: 60,
        };
        assert_eq!(p.holding(PledgeKind::Hp), 20);
        p.status = PledgeStatus::Forfeit;
        assert_eq!(p.holding(PledgeKind::Hp), 40);
        p.status = PledgeStatus::Repaid;
        assert_eq!(p.holding(PledgeKind::Hp), 0);
    }

    #[test]
    fn canon_room_counts_free_slots_and_grace() {
        let slot = |day| CanonSlot {
            spell_id: "cx_a".into(),
            sealed_day: day,
            ..CanonSlot::default()
        };
        let mut canon = Canon::default();
        assert!(canon.has_room(2, 100, 30), "empty canon has room");
        canon.slots = vec![slot(90), slot(95)];
        assert!(!canon.has_room(2, 100, 30), "full and every slot in grace");
        assert!(canon.has_room(3, 100, 30), "free slot");
        assert!(canon.has_room(2, 120, 30), "the day-90 slot left grace on day 120");
    }

    #[test]
    fn meta_round_trips() {
        let meta = CodexMeta {
            origin: SpellOrigin::Authored,
            author: Some("Mira".into()),
            god_vnum: "god:sol".into(),
            status: CanonStatus::Faded,
            sealed_day: 1234,
            lineage: vec![LineageEvent {
                kind: LineageKind::Restored,
                who: "Tal".into(),
                day: 1300,
            }],
            witnesses: vec!["Ash".into()],
            author_disgraced: true,
            flavor: CastFlavor {
                caster: "You burn.".into(),
                room: String::new(),
                target: String::new(),
            },
            version: 3,
            vetoed: true,
            casts_by_others: 12,
        };
        let json = serde_json::to_string(&meta).unwrap();
        assert!(json.contains("\"faded\""), "statuses serialize snake_case: {json}");
        let back: CodexMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(back, meta);
    }
}

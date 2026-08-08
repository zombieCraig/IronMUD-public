//! The prompt, as a table of segments rather than a chain of `if`s.
//!
//! The prompt used to be ~400 lines of hardcoded branches inside
//! `build_prompt`: every species that added a pool added another `let
//! x_segment = ...` block and another slot in one growing `format!`. That
//! shape had two costs. Players could not reorder or drop anything — the
//! only choice was `simple` or `verbose` — and every new pool meant editing
//! the same function again, in two places.
//!
//! Here each thing the prompt can say is one [`PromptSegment`]: a name, an
//! optional one-character alias, a predicate for whether it applies to this
//! character, and a renderer. A prompt is then a format string over those
//! tokens, and `simple`/`verbose` are just two stored formats.
//!
//! There are deliberately two kinds of token:
//!
//! * **Group tokens** (`%{hp}`, `%{mana}`, …) render a whole bracketed,
//!   coloured, *conditional* group — `[HP:42/60] `. They render empty when
//!   they do not apply, which is what lets one default format cover a
//!   vampire, a replicant and a mutant without the format knowing which is
//!   which. [`VERBOSE_FORMAT`] is written in these, and reproduces the old
//!   hardcoded prompt byte for byte.
//! * **Value tokens** (`%h`, `%H`, `%s`, …) render a bare number with no
//!   colour and no brackets, for players who want to lay the prompt out
//!   themselves: `%h/%H hp %s/%S st >`.
//!
//! Rendering is split from parsing so callers can skip expensive work. The
//! combat target and the on-prompt item triggers each cost a lock and some
//! database reads; [`uses`] lets `build_prompt` find out whether the format
//! mentions them before paying for them. That is why the simple prompt now
//! costs nothing at all, where it used to fetch equipped items first and
//! throw them away.

use crate::types::CharacterData;
use std::collections::HashMap;

/// The stock `prompt simple` format. Not literally empty of tokens by
/// accident — a prompt with no state in it is a legitimate choice.
pub const SIMPLE_FORMAT: &str = "> ";

/// The stock `prompt verbose` format. Every group in it is conditional, so
/// this one string is correct for every species; the groups that do not
/// apply render as nothing.
pub const VERBOSE_FORMAT: &str =
    "%{hp}%{stamina}%{mana}%{blood}%{mutation}%{air}%{burning}%{breakdown}%{target}%{extra}%{build}> ";

/// Longest format we will store. A prompt is printed after every single
/// command, so an unbounded one is a self-inflicted flood.
pub const MAX_FORMAT_LEN: usize = 240;

/// Everything a segment renderer is allowed to look at.
///
/// Built once per prompt by the caller, which owns the locks. Renderers take
/// `&PromptContext` and nothing else on purpose: a renderer that could reach
/// the database could deadlock, because the prompt is built while the caller
/// may already hold the World lock.
#[derive(Clone, Debug, Default)]
pub struct PromptContext {
    pub colors: bool,
    pub name: String,
    pub hp: i32,
    pub max_hp: i32,
    pub stamina: i32,
    pub max_stamina: i32,
    pub mana: i32,
    pub max_mana: i32,
    pub mana_enabled: bool,
    pub breath: i32,
    pub max_breath: i32,
    /// `(current, max)` blood pool — vampires only.
    pub blood: Option<(i32, i32)>,
    /// `(current, max)` resolve — replicants only. Replaces stamina, since a
    /// replicant's body never tires and its mind is the gauge.
    pub resolve: Option<(i32, i32)>,
    /// `(current, max)` mutation points — mutants only.
    pub mutation: Option<(i32, i32)>,
    pub burning: bool,
    pub breakdown: bool,
    pub build_mode: bool,
    pub gold: i32,
    pub renown: i32,
    pub morality: i32,
    pub reputation: HashMap<String, i32>,
    /// Faction key to display name, for `%{standing:key}`. Populated only
    /// when the format asks for it.
    pub faction_names: HashMap<String, String>,
    /// Pre-rendered combat target tag, including its trailing space.
    pub target_tag: String,
    /// Pre-rendered `on_prompt` item-trigger contributions.
    pub extra: String,
}

impl PromptContext {
    /// Everything a prompt can read straight off the character, with no lock
    /// and no database. The three fields this leaves blank — `target_tag`,
    /// `extra` and `faction_names` — each cost a lock, so the caller fills
    /// them only when the format asks.
    pub fn from_character(ch: &CharacterData, colors: bool) -> Self {
        // A torso wound caps hit points and a head wound caps mana. The
        // prompt shows the capped maxima rather than the paper ones, because
        // the capped number is the one the player can actually reach.
        let torso = worst_wound_penalty(ch, crate::types::BodyPart::Torso);
        let max_hp = if torso > 0 {
            (ch.max_hp * (100 - torso) / 100).max(1)
        } else {
            ch.max_hp
        };
        let head = worst_wound_penalty(ch, crate::types::BodyPart::Head);
        let max_mana = if head > 0 {
            (ch.max_mana * (100 - head) / 100).max(0)
        } else {
            ch.max_mana
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        PromptContext {
            colors,
            name: ch.name.clone(),
            hp: ch.hp,
            max_hp,
            stamina: ch.stamina,
            max_stamina: ch.max_stamina,
            mana: ch.mana,
            max_mana,
            mana_enabled: ch.mana_enabled,
            breath: ch.breath,
            max_breath: ch.max_breath,
            blood: ch.vampire_state.as_ref().map(|v| (v.blood_pool, v.max_blood_pool)),
            resolve: ch.replicant_state.as_ref().map(|r| (r.resolve, r.max_resolve)),
            mutation: ch.mutant_state.as_ref().map(|m| (m.mp, m.max_mp)),
            burning: ch
                .active_buffs
                .iter()
                .any(|b| b.effect_type == crate::types::EffectType::SunlightBurning),
            breakdown: ch
                .replicant_state
                .as_ref()
                .map(|r| r.is_breaking_down(now))
                .unwrap_or(false),
            build_mode: ch.build_mode,
            gold: ch.gold,
            renown: crate::progress::renown(ch).total,
            morality: ch.morality,
            reputation: ch.reputation.clone(),
            faction_names: HashMap::new(),
            target_tag: String::new(),
            extra: String::new(),
        }
    }
}

fn worst_wound_penalty(ch: &CharacterData, part: crate::types::BodyPart) -> i32 {
    ch.wounds
        .iter()
        .filter(|w| w.body_part == part)
        .map(|w| w.level.penalty())
        .max()
        .unwrap_or(0)
}

/// One thing the prompt can say.
pub struct PromptSegment {
    /// Single-character alias, usable as `%h`. Group tokens mostly have none
    /// — there are more groups than sensible letters, and a group is the
    /// verbose default's business rather than something players type.
    pub short: Option<char>,
    /// Long name, usable as `%{name}`.
    pub name: &'static str,
    /// Whether `%{name:arg}` takes an argument (a faction key).
    pub takes_arg: bool,
    pub help: &'static str,
    /// Whether this segment has anything to say about this character. Drives
    /// the `prompt tokens` listing; renderers still check for themselves.
    pub applies: fn(&PromptContext) -> bool,
    pub render: fn(&PromptContext, Option<&str>) -> String,
}

fn pct(cur: i32, max: i32) -> i32 {
    if max > 0 { (cur * 100) / max } else { 100 }
}

fn reset(ctx: &PromptContext) -> &'static str {
    if ctx.colors { "\x1b[0m" } else { "" }
}

/// Pick from three colours by percentage, or nothing when the player has
/// colour off. The thresholds (70 / 30) are the ones every pool already
/// used.
fn band_color(ctx: &PromptContext, p: i32, high: &'static str, mid: &'static str, low: &'static str) -> &'static str {
    if !ctx.colors {
        return "";
    }
    if p >= 70 {
        high
    } else if p >= 30 {
        mid
    } else {
        low
    }
}

fn always(_: &PromptContext) -> bool {
    true
}

// ---------------------------------------------------------------------------
// Group renderers. These reproduce the pre-table prompt exactly.
// ---------------------------------------------------------------------------

fn group_hp(ctx: &PromptContext, _: Option<&str>) -> String {
    let c = band_color(ctx, pct(ctx.hp, ctx.max_hp), "\x1b[32m", "\x1b[33m", "\x1b[31m");
    format!("[{}HP:{}/{}{}] ", c, ctx.hp, ctx.max_hp, reset(ctx))
}

fn group_stamina(ctx: &PromptContext, _: Option<&str>) -> String {
    // Resolve stands in for stamina rather than sitting beside it: a
    // replicant's stamina is pinned at zero, so showing both would print a
    // permanent `[ST:0/0]` next to the gauge that actually moves.
    if let Some((res, max_res)) = ctx.resolve {
        if max_res > 0 {
            let c = band_color(ctx, pct(res, max_res), "\x1b[36m", "\x1b[33m", "\x1b[1;31m");
            return format!("[{}RES:{}/{}{}] ", c, res, max_res, reset(ctx));
        }
    }
    let c = band_color(
        ctx,
        pct(ctx.stamina, ctx.max_stamina),
        "\x1b[36m",
        "\x1b[34m",
        "\x1b[35m",
    );
    format!("[{}ST:{}/{}{}] ", c, ctx.stamina, ctx.max_stamina, reset(ctx))
}

fn group_mana(ctx: &PromptContext, _: Option<&str>) -> String {
    if !ctx.mana_enabled {
        return String::new();
    }
    let c = band_color(ctx, pct(ctx.mana, ctx.max_mana), "\x1b[94m", "\x1b[34m", "\x1b[35m");
    format!("[{}MP:{}/{}{}] ", c, ctx.mana, ctx.max_mana, reset(ctx))
}

fn group_blood(ctx: &PromptContext, _: Option<&str>) -> String {
    match ctx.blood {
        Some((bp, max_bp)) if max_bp > 0 => {
            let c = band_color(ctx, pct(bp, max_bp), "\x1b[31m", "\x1b[33m", "\x1b[1;31m");
            format!("[{}BP:{}/{}{}] ", c, bp, max_bp, reset(ctx))
        }
        _ => String::new(),
    }
}

fn group_mutation(ctx: &PromptContext, _: Option<&str>) -> String {
    match ctx.mutation {
        Some((mp, max_mp)) if max_mp > 0 => {
            let c = band_color(ctx, pct(mp, max_mp), "\x1b[32m", "\x1b[33m", "\x1b[90m");
            format!("[{}MP:{}/{}{}] ", c, mp, max_mp, reset(ctx))
        }
        _ => String::new(),
    }
}

fn group_air(ctx: &PromptContext, _: Option<&str>) -> String {
    // Only while holding your breath — a permanent full-air gauge is noise.
    if ctx.breath >= ctx.max_breath {
        return String::new();
    }
    let p = pct(ctx.breath, ctx.max_breath);
    let c = if !ctx.colors {
        ""
    } else if p >= 50 {
        "\x1b[36m"
    } else if p >= 25 {
        "\x1b[33m"
    } else {
        "\x1b[31m"
    };
    format!("[{}Air:{}/{}{}] ", c, ctx.breath, ctx.max_breath, reset(ctx))
}

fn group_burning(ctx: &PromptContext, _: Option<&str>) -> String {
    if !ctx.burning {
        return String::new();
    }
    if ctx.colors {
        "\x1b[1;31m[BURNING]\x1b[0m ".to_string()
    } else {
        "[BURNING] ".to_string()
    }
}

fn group_breakdown(ctx: &PromptContext, _: Option<&str>) -> String {
    if !ctx.breakdown {
        return String::new();
    }
    if ctx.colors {
        "\x1b[1;31m[BREAKDOWN]\x1b[0m ".to_string()
    } else {
        "[BREAKDOWN] ".to_string()
    }
}

fn group_build(ctx: &PromptContext, _: Option<&str>) -> String {
    if !ctx.build_mode {
        return String::new();
    }
    if ctx.colors {
        "\x1b[1;33m[BUILD]\x1b[0m ".to_string()
    } else {
        "[BUILD] ".to_string()
    }
}

fn group_target(ctx: &PromptContext, _: Option<&str>) -> String {
    ctx.target_tag.clone()
}

fn group_extra(ctx: &PromptContext, _: Option<&str>) -> String {
    ctx.extra.clone()
}

fn group_renown(ctx: &PromptContext, _: Option<&str>) -> String {
    format!("[Renown:{}] ", ctx.renown)
}

fn group_morality(ctx: &PromptContext, _: Option<&str>) -> String {
    let tier = crate::morality::MoralityTier::from_value(ctx.morality);
    let c = if !ctx.colors {
        ""
    } else if tier.is_good() {
        "\x1b[36m"
    } else if tier.is_evil() {
        "\x1b[31m"
    } else {
        ""
    };
    format!("[{}{}{}] ", c, tier.label(), reset(ctx))
}

/// `%{standing:town_guard}` — where you stand with one named faction.
///
/// Standing needs an argument because there is no such thing as "your
/// reputation" in the singular; the whole point of the system is that it
/// differs per faction. A player who wants two on their prompt writes the
/// token twice.
fn group_standing(ctx: &PromptContext, arg: Option<&str>) -> String {
    let key = match crate::reputation::normalize(arg) {
        Some(k) => k,
        None => return String::new(),
    };
    let value = crate::reputation::standing(&ctx.reputation, &key);
    let tier = crate::reputation::ReputationTier::from_value(value);
    let name = ctx.faction_names.get(&key).cloned().unwrap_or(key);
    let c = if !ctx.colors {
        ""
    } else if tier.is_friendly() {
        "\x1b[32m"
    } else if value < 0 {
        "\x1b[31m"
    } else {
        ""
    };
    format!("[{}{}: {}{}] ", c, name, tier.label(), reset(ctx))
}

// ---------------------------------------------------------------------------
// Value renderers — bare, uncoloured, for hand-laid-out prompts.
// ---------------------------------------------------------------------------

fn val_hp(c: &PromptContext, _: Option<&str>) -> String {
    c.hp.to_string()
}
fn val_max_hp(c: &PromptContext, _: Option<&str>) -> String {
    c.max_hp.to_string()
}
fn val_stamina(c: &PromptContext, _: Option<&str>) -> String {
    c.stamina.to_string()
}
fn val_max_stamina(c: &PromptContext, _: Option<&str>) -> String {
    c.max_stamina.to_string()
}
fn val_mana(c: &PromptContext, _: Option<&str>) -> String {
    c.mana.to_string()
}
fn val_max_mana(c: &PromptContext, _: Option<&str>) -> String {
    c.max_mana.to_string()
}
fn val_blood(c: &PromptContext, _: Option<&str>) -> String {
    c.blood.map(|(v, _)| v.to_string()).unwrap_or_default()
}
fn val_max_blood(c: &PromptContext, _: Option<&str>) -> String {
    c.blood.map(|(_, m)| m.to_string()).unwrap_or_default()
}
fn val_resolve(c: &PromptContext, _: Option<&str>) -> String {
    c.resolve.map(|(v, _)| v.to_string()).unwrap_or_default()
}
fn val_max_resolve(c: &PromptContext, _: Option<&str>) -> String {
    c.resolve.map(|(_, m)| m.to_string()).unwrap_or_default()
}
fn val_mutation(c: &PromptContext, _: Option<&str>) -> String {
    c.mutation.map(|(v, _)| v.to_string()).unwrap_or_default()
}
fn val_max_mutation(c: &PromptContext, _: Option<&str>) -> String {
    c.mutation.map(|(_, m)| m.to_string()).unwrap_or_default()
}
fn val_air(c: &PromptContext, _: Option<&str>) -> String {
    c.breath.to_string()
}
fn val_max_air(c: &PromptContext, _: Option<&str>) -> String {
    c.max_breath.to_string()
}
fn val_name(c: &PromptContext, _: Option<&str>) -> String {
    c.name.clone()
}
fn val_gold(c: &PromptContext, _: Option<&str>) -> String {
    c.gold.to_string()
}
fn val_renown(c: &PromptContext, _: Option<&str>) -> String {
    c.renown.to_string()
}
fn val_morality(c: &PromptContext, _: Option<&str>) -> String {
    crate::morality::MoralityTier::from_value(c.morality)
        .label()
        .to_string()
}
fn val_condition(c: &PromptContext, _: Option<&str>) -> String {
    crate::combat_text::Condition::from_hp(c.hp, c.max_hp).tag().to_string()
}
fn val_standing_band(c: &PromptContext, arg: Option<&str>) -> String {
    match crate::reputation::normalize(arg) {
        Some(k) => crate::reputation::tier(&c.reputation, &k).label().to_string(),
        None => String::new(),
    }
}
fn val_standing_value(c: &PromptContext, arg: Option<&str>) -> String {
    match crate::reputation::normalize(arg) {
        Some(k) => crate::reputation::standing(&c.reputation, &k).to_string(),
        None => String::new(),
    }
}

fn has_mana(c: &PromptContext) -> bool {
    c.mana_enabled
}
fn has_blood(c: &PromptContext) -> bool {
    matches!(c.blood, Some((_, m)) if m > 0)
}
fn has_resolve(c: &PromptContext) -> bool {
    matches!(c.resolve, Some((_, m)) if m > 0)
}
fn has_mutation(c: &PromptContext) -> bool {
    matches!(c.mutation, Some((_, m)) if m > 0)
}
fn no_resolve(c: &PromptContext) -> bool {
    !has_resolve(c)
}
fn is_building(c: &PromptContext) -> bool {
    c.build_mode
}
fn has_reputation(c: &PromptContext) -> bool {
    !c.reputation.is_empty()
}

/// Every token the prompt understands, in the order `prompt tokens` lists
/// them. Adding a pool to the game means adding one row here — and nothing
/// else, because the default format already asks for every group by name.
pub static SEGMENTS: &[PromptSegment] = &[
    // Groups
    PromptSegment {
        short: None,
        name: "hp",
        takes_arg: false,
        help: "[HP:x/y] with health colouring",
        applies: always,
        render: group_hp,
    },
    PromptSegment {
        short: None,
        name: "stamina",
        takes_arg: false,
        help: "[ST:x/y], or [RES:x/y] for replicants",
        applies: always,
        render: group_stamina,
    },
    PromptSegment {
        short: None,
        name: "mana",
        takes_arg: false,
        help: "[MP:x/y] when you can cast",
        applies: has_mana,
        render: group_mana,
    },
    PromptSegment {
        short: None,
        name: "blood",
        takes_arg: false,
        help: "[BP:x/y] blood pool (vampires)",
        applies: has_blood,
        render: group_blood,
    },
    PromptSegment {
        short: None,
        name: "mutation",
        takes_arg: false,
        help: "[MP:x/y] mutation points (mutants)",
        applies: has_mutation,
        render: group_mutation,
    },
    PromptSegment {
        short: None,
        name: "air",
        takes_arg: false,
        help: "[Air:x/y] while holding your breath",
        applies: always,
        render: group_air,
    },
    PromptSegment {
        short: None,
        name: "burning",
        takes_arg: false,
        help: "[BURNING] while sunlight is killing you",
        applies: always,
        render: group_burning,
    },
    PromptSegment {
        short: None,
        name: "breakdown",
        takes_arg: false,
        help: "[BREAKDOWN] during a replicant breakdown",
        applies: has_resolve,
        render: group_breakdown,
    },
    PromptSegment {
        short: Some('t'),
        name: "target",
        takes_arg: false,
        help: "[Ghoul: Bloodied] who you are fighting",
        applies: always,
        render: group_target,
    },
    PromptSegment {
        short: None,
        name: "extra",
        takes_arg: false,
        help: "anything equipped items add (a watch, a compass)",
        applies: always,
        render: group_extra,
    },
    PromptSegment {
        short: None,
        name: "build",
        takes_arg: false,
        help: "[BUILD] while build mode is on",
        applies: is_building,
        render: group_build,
    },
    PromptSegment {
        short: None,
        name: "renown",
        takes_arg: false,
        help: "[Renown:n] your breadth score",
        applies: always,
        render: group_renown,
    },
    PromptSegment {
        short: None,
        name: "morality",
        takes_arg: false,
        help: "[Virtuous] your alignment band",
        applies: always,
        render: group_morality,
    },
    PromptSegment {
        short: None,
        name: "standing",
        takes_arg: true,
        help: "[The Town Guard: Honored] — needs a faction: %{standing:town_guard}",
        applies: has_reputation,
        render: group_standing,
    },
    // Values
    PromptSegment {
        short: Some('h'),
        name: "curhp",
        takes_arg: false,
        help: "current hit points",
        applies: always,
        render: val_hp,
    },
    PromptSegment {
        short: Some('H'),
        name: "maxhp",
        takes_arg: false,
        help: "maximum hit points",
        applies: always,
        render: val_max_hp,
    },
    PromptSegment {
        short: Some('s'),
        name: "curstamina",
        takes_arg: false,
        help: "current stamina",
        applies: no_resolve,
        render: val_stamina,
    },
    PromptSegment {
        short: Some('S'),
        name: "maxstamina",
        takes_arg: false,
        help: "maximum stamina",
        applies: no_resolve,
        render: val_max_stamina,
    },
    PromptSegment {
        short: Some('m'),
        name: "curmana",
        takes_arg: false,
        help: "current mana",
        applies: has_mana,
        render: val_mana,
    },
    PromptSegment {
        short: Some('M'),
        name: "maxmana",
        takes_arg: false,
        help: "maximum mana",
        applies: has_mana,
        render: val_max_mana,
    },
    PromptSegment {
        short: Some('v'),
        name: "curblood",
        takes_arg: false,
        help: "current blood pool",
        applies: has_blood,
        render: val_blood,
    },
    PromptSegment {
        short: Some('V'),
        name: "maxblood",
        takes_arg: false,
        help: "maximum blood pool",
        applies: has_blood,
        render: val_max_blood,
    },
    PromptSegment {
        short: Some('r'),
        name: "curresolve",
        takes_arg: false,
        help: "current resolve",
        applies: has_resolve,
        render: val_resolve,
    },
    PromptSegment {
        short: Some('R'),
        name: "maxresolve",
        takes_arg: false,
        help: "maximum resolve",
        applies: has_resolve,
        render: val_max_resolve,
    },
    PromptSegment {
        short: Some('u'),
        name: "curmutation",
        takes_arg: false,
        help: "current mutation points",
        applies: has_mutation,
        render: val_mutation,
    },
    PromptSegment {
        short: Some('U'),
        name: "maxmutation",
        takes_arg: false,
        help: "maximum mutation points",
        applies: has_mutation,
        render: val_max_mutation,
    },
    PromptSegment {
        short: Some('a'),
        name: "curair",
        takes_arg: false,
        help: "current breath",
        applies: always,
        render: val_air,
    },
    PromptSegment {
        short: Some('A'),
        name: "maxair",
        takes_arg: false,
        help: "maximum breath",
        applies: always,
        render: val_max_air,
    },
    PromptSegment {
        short: Some('n'),
        name: "charname",
        takes_arg: false,
        help: "your name",
        applies: always,
        render: val_name,
    },
    PromptSegment {
        short: Some('g'),
        name: "gold",
        takes_arg: false,
        help: "gold carried",
        applies: always,
        render: val_gold,
    },
    PromptSegment {
        short: Some('x'),
        name: "renownvalue",
        takes_arg: false,
        help: "renown as a bare number",
        applies: always,
        render: val_renown,
    },
    PromptSegment {
        short: Some('l'),
        name: "moralityband",
        takes_arg: false,
        help: "alignment band name, unbracketed",
        applies: always,
        render: val_morality,
    },
    PromptSegment {
        short: Some('c'),
        name: "condition",
        takes_arg: false,
        help: "your own health word (Bloodied, Scratched, ...)",
        applies: always,
        render: val_condition,
    },
    PromptSegment {
        short: None,
        name: "standingband",
        takes_arg: true,
        help: "one faction's band name, unbracketed",
        applies: has_reputation,
        render: val_standing_band,
    },
    PromptSegment {
        short: None,
        name: "standingvalue",
        takes_arg: true,
        help: "one faction's standing as a bare number",
        applies: has_reputation,
        render: val_standing_value,
    },
];

fn by_name(name: &str) -> Option<&'static PromptSegment> {
    SEGMENTS.iter().find(|s| s.name == name)
}

fn by_short(c: char) -> Option<&'static PromptSegment> {
    SEGMENTS.iter().find(|s| s.short == Some(c))
}

/// One resolved chunk of a format string.
pub enum Piece {
    Literal(String),
    Token(&'static PromptSegment, Option<String>),
}

/// Split a format into literals and tokens.
///
/// Returns the pieces plus any tokens it did not recognise. Unknown tokens
/// survive into the output verbatim rather than vanishing, so a typo shows
/// itself in the prompt instead of silently deleting a segment. `%%` is a
/// literal percent.
pub fn parse(format: &str) -> (Vec<Piece>, Vec<String>) {
    let mut pieces: Vec<Piece> = Vec::new();
    let mut unknown: Vec<String> = Vec::new();
    let mut literal = String::new();
    let mut chars = format.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '%' {
            literal.push(ch);
            continue;
        }
        let next = match chars.next() {
            Some(c) => c,
            // A trailing bare '%' is just a percent sign.
            None => {
                literal.push('%');
                break;
            }
        };
        if next == '%' {
            literal.push('%');
            continue;
        }

        let (name, arg, raw) = if next == '{' {
            let mut body = String::new();
            let mut closed = false;
            for c in chars.by_ref() {
                if c == '}' {
                    closed = true;
                    break;
                }
                body.push(c);
            }
            if !closed {
                // Unterminated — hand it back as text so the player can see
                // where they lost the brace.
                literal.push_str("%{");
                literal.push_str(&body);
                continue;
            }
            let raw = format!("%{{{}}}", body);
            match body.split_once(':') {
                Some((n, a)) => (n.trim().to_string(), Some(a.trim().to_string()), raw),
                None => (body.trim().to_string(), None, raw),
            }
        } else {
            match by_short(next) {
                Some(seg) => (seg.name.to_string(), None, format!("%{}", next)),
                None => {
                    unknown.push(format!("%{}", next));
                    literal.push('%');
                    literal.push(next);
                    continue;
                }
            }
        };

        match by_name(&name) {
            Some(seg) => {
                if !literal.is_empty() {
                    pieces.push(Piece::Literal(std::mem::take(&mut literal)));
                }
                pieces.push(Piece::Token(seg, arg));
            }
            None => {
                unknown.push(raw.clone());
                literal.push_str(&raw);
            }
        }
    }

    if !literal.is_empty() {
        pieces.push(Piece::Literal(literal));
    }
    (pieces, unknown)
}

/// Whether a parsed format mentions a segment by name. Callers use this to
/// skip the locks and reads a segment would need.
pub fn uses(pieces: &[Piece], name: &str) -> bool {
    pieces
        .iter()
        .any(|p| matches!(p, Piece::Token(seg, _) if seg.name == name))
}

/// Every faction key a parsed format asks about, so the caller can resolve
/// display names in one pass under one lock.
pub fn requested_factions(pieces: &[Piece]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in pieces {
        if let Piece::Token(seg, Some(arg)) = p {
            if seg.takes_arg {
                if let Some(key) = crate::reputation::normalize(Some(arg)) {
                    if !out.contains(&key) {
                        out.push(key);
                    }
                }
            }
        }
    }
    out
}

pub fn render(pieces: &[Piece], ctx: &PromptContext) -> String {
    let mut out = String::new();
    for piece in pieces {
        match piece {
            Piece::Literal(s) => out.push_str(s),
            Piece::Token(seg, arg) => out.push_str(&(seg.render)(ctx, arg.as_deref())),
        }
    }
    out
}

/// The format a character's prompt should be built from.
///
/// A stored custom format wins; otherwise `prompt_mode` picks a preset. The
/// presets are the same mechanism, not a separate code path — which is what
/// keeps `simple` and `verbose` from drifting away from what the table can
/// express.
pub fn format_for(ch: &CharacterData) -> &str {
    resolve_format(&ch.prompt_format, &ch.prompt_mode)
}

/// [`format_for`] over the two raw fields, so the precedence rule can be
/// tested without building a whole character. The rule is load-bearing in
/// both directions: a custom format has to win, or `prompt <format>` does
/// nothing; and switching preset has to clear the custom one, or `prompt
/// simple` appears to do nothing. The `prompt` command owns the clearing
/// half.
pub fn resolve_format<'a>(prompt_format: &'a str, prompt_mode: &str) -> &'a str {
    if !prompt_format.is_empty() {
        prompt_format
    } else if prompt_mode == "verbose" {
        VERBOSE_FORMAT
    } else {
        SIMPLE_FORMAT
    }
}

/// Check a player-supplied format before storing it. `Ok(())` or one line
/// naming what is wrong.
pub fn validate(format: &str) -> Result<(), String> {
    if format.len() > MAX_FORMAT_LEN {
        return Err(format!(
            "That prompt is {} characters; the limit is {}.",
            format.len(),
            MAX_FORMAT_LEN
        ));
    }
    if format.contains('\n') || format.contains('\r') {
        return Err("A prompt cannot contain a line break.".to_string());
    }
    let (pieces, unknown) = parse(format);
    if !unknown.is_empty() {
        return Err(format!(
            "Unknown token(s): {}. Try 'prompt tokens' for the list.",
            unknown.join(", ")
        ));
    }
    for piece in &pieces {
        if let Piece::Token(seg, arg) = piece {
            if seg.takes_arg && crate::reputation::normalize(arg.as_deref()).is_none() {
                return Err(format!(
                    "%{{{}}} needs a faction, like %{{{}:town_guard}}.",
                    seg.name, seg.name
                ));
            }
        }
    }
    Ok(())
}

/// Lines for `prompt tokens`, marked for whether each applies to the
/// character asking. A mutant should not have to guess whether `%u` means
/// anything for them.
pub fn token_lines(ctx: &PromptContext) -> Vec<String> {
    SEGMENTS
        .iter()
        .map(|seg| {
            let token = if seg.takes_arg {
                format!("%{{{}:<faction>}}", seg.name)
            } else {
                match seg.short {
                    Some(c) => format!("%{}  %{{{}}}", c, seg.name),
                    None => format!("    %{{{}}}", seg.name),
                }
            };
            let mark = if (seg.applies)(ctx) { " " } else { "-" };
            format!("{} {:<28} {}", mark, token, seg.help)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> PromptContext {
        PromptContext {
            colors: false,
            name: "Tester".into(),
            hp: 42,
            max_hp: 60,
            stamina: 30,
            max_stamina: 100,
            mana: 5,
            max_mana: 20,
            mana_enabled: true,
            breath: 10,
            max_breath: 10,
            gold: 77,
            renown: 123,
            ..Default::default()
        }
    }

    #[test]
    fn value_tokens_render_bare_numbers() {
        let (pieces, unknown) = parse("%h/%H hp %g gold");
        assert!(unknown.is_empty());
        assert_eq!(render(&pieces, &ctx()), "42/60 hp 77 gold");
    }

    #[test]
    fn long_names_and_short_aliases_agree() {
        let short = parse("%h").0;
        let long = parse("%{curhp}").0;
        let c = ctx();
        assert_eq!(render(&short, &c), render(&long, &c));
    }

    #[test]
    fn percent_escapes_and_unknown_tokens_survive() {
        let (pieces, unknown) = parse("100%% %q done");
        assert_eq!(unknown, vec!["%q".to_string()]);
        // The typo stays visible instead of silently disappearing.
        assert_eq!(render(&pieces, &ctx()), "100% %q done");
    }

    #[test]
    fn verbose_format_matches_the_old_hardcoded_prompt() {
        // The pre-table prompt for a plain character with colours off.
        let (pieces, unknown) = parse(VERBOSE_FORMAT);
        assert!(unknown.is_empty());
        assert_eq!(render(&pieces, &ctx()), "[HP:42/60] [ST:30/100] [MP:5/20] > ");
    }

    #[test]
    fn simple_format_is_just_a_chevron() {
        let (pieces, _) = parse(SIMPLE_FORMAT);
        assert_eq!(render(&pieces, &ctx()), "> ");
    }

    #[test]
    fn air_hides_at_full_breath_and_shows_below() {
        let mut c = ctx();
        let (pieces, _) = parse("%{air}");
        assert_eq!(render(&pieces, &c), "");
        c.breath = 4;
        assert_eq!(render(&pieces, &c), "[Air:4/10] ");
    }

    #[test]
    fn resolve_replaces_stamina_rather_than_joining_it() {
        let mut c = ctx();
        c.resolve = Some((7, 10));
        let (pieces, _) = parse("%{stamina}");
        assert_eq!(render(&pieces, &c), "[RES:7/10] ");
    }

    #[test]
    fn inapplicable_groups_render_nothing() {
        let mut c = ctx();
        c.mana_enabled = false;
        let (pieces, _) = parse("%{mana}%{blood}%{mutation}%{burning}%{breakdown}%{build}");
        assert_eq!(render(&pieces, &c), "");
    }

    #[test]
    fn standing_reads_the_named_faction() {
        let mut c = ctx();
        c.reputation.insert("town_guard".into(), 250);
        c.faction_names.insert("town_guard".into(), "The Town Guard".into());
        let (pieces, unknown) = parse("%{standing:town_guard}%{standingvalue:town_guard}");
        assert!(unknown.is_empty());
        assert_eq!(render(&pieces, &c), "[The Town Guard: Honored] 250");
    }

    #[test]
    fn standing_falls_back_to_the_key_when_the_faction_is_unregistered() {
        let mut c = ctx();
        c.reputation.insert("roadmen".into(), -60);
        let (pieces, _) = parse("%{standing:roadmen}");
        assert_eq!(render(&pieces, &c), "[roadmen: Disliked] ");
    }

    #[test]
    fn requested_factions_dedupes_and_normalizes() {
        let (pieces, _) = parse("%{standing:Town_Guard}%{standingvalue: town_guard }%{standing:bandits}");
        assert_eq!(
            requested_factions(&pieces),
            vec!["town_guard".to_string(), "bandits".to_string()]
        );
    }

    #[test]
    fn uses_finds_only_the_named_segment() {
        let (pieces, _) = parse("%{hp}%{target}");
        assert!(uses(&pieces, "target"));
        assert!(!uses(&pieces, "extra"));
    }

    #[test]
    fn validate_rejects_typos_length_and_missing_faction() {
        assert!(validate("%{hp}%{target}> ").is_ok());
        assert!(validate("%q> ").is_err());
        assert!(validate("%{standing}> ").is_err());
        assert!(validate("%{standing:town_guard}> ").is_ok());
        assert!(validate(&"x".repeat(MAX_FORMAT_LEN + 1)).is_err());
        assert!(validate("a\nb").is_err());
    }

    #[test]
    fn colors_wrap_the_value_and_reset_after_it() {
        let mut c = ctx();
        c.colors = true;
        let (pieces, _) = parse("%{hp}");
        // 42/60 is 70%, the healthy band.
        assert_eq!(render(&pieces, &c), "[\x1b[32mHP:42/60\x1b[0m] ");
    }

    #[test]
    fn unterminated_brace_is_shown_not_swallowed() {
        let (pieces, _) = parse("%{hp");
        assert_eq!(render(&pieces, &ctx()), "%{hp");
    }

    #[test]
    fn every_short_alias_is_unique() {
        let mut seen: Vec<char> = Vec::new();
        for seg in SEGMENTS {
            if let Some(c) = seg.short {
                assert!(!seen.contains(&c), "duplicate short token %{}", c);
                seen.push(c);
            }
        }
    }

    #[test]
    fn every_segment_name_is_unique() {
        let mut seen: Vec<&str> = Vec::new();
        for seg in SEGMENTS {
            assert!(!seen.contains(&seg.name), "duplicate segment name {}", seg.name);
            seen.push(seg.name);
        }
    }

    /// CharacterData has no `Default`, so build it from JSON and let serde's
    /// field defaults fill the rest — the same approach `progress` and the
    /// dialogue tests use.
    fn character(json: serde_json::Value) -> CharacterData {
        let mut base = serde_json::json!({
            "name": "Tester",
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        });
        let obj = base.as_object_mut().unwrap();
        for (k, v) in json.as_object().unwrap() {
            obj.insert(k.clone(), v.clone());
        }
        serde_json::from_value(base).expect("build character")
    }

    #[test]
    fn from_character_reports_wound_capped_maxima() {
        // A torso wound caps hit points and a head wound caps mana. The
        // prompt has always shown the capped figure; this pins that it
        // survived the move out of build_prompt.
        let ch = character(serde_json::json!({
            "hp": 40, "max_hp": 100,
            "mana": 10, "max_mana": 100,
            "wounds": [
                { "body_part": "torso", "level": "severe", "wound_type": "cut" },
                { "body_part": "head", "level": "severe", "wound_type": "cut" },
            ],
        }));
        let ctx = PromptContext::from_character(&ch, false);
        assert!(ctx.max_hp < 100, "torso wound should cap max hp, got {}", ctx.max_hp);
        assert!(
            ctx.max_mana < 100,
            "head wound should cap max mana, got {}",
            ctx.max_mana
        );
    }

    #[test]
    fn from_character_leaves_the_lock_bought_fields_empty() {
        // build_prompt fills these three only when the format asks, so they
        // must not arrive pre-populated with something stale.
        let ctx = PromptContext::from_character(&character(serde_json::json!({})), false);
        assert!(ctx.target_tag.is_empty());
        assert!(ctx.extra.is_empty());
        assert!(ctx.faction_names.is_empty());
    }

    #[test]
    fn a_plain_character_has_no_species_pools() {
        let ctx = PromptContext::from_character(&character(serde_json::json!({})), false);
        assert!(ctx.blood.is_none());
        assert!(ctx.resolve.is_none());
        assert!(ctx.mutation.is_none());
        let (pieces, _) = parse(VERBOSE_FORMAT);
        // Only the two universal pools, and no mana without mana_enabled.
        assert_eq!(render(&pieces, &ctx), "[HP:100/100] [ST:100/100] > ");
    }

    #[test]
    fn format_for_reads_the_character_fields() {
        let ch = character(serde_json::json!({ "prompt_mode": "verbose" }));
        assert_eq!(format_for(&ch), VERBOSE_FORMAT);
        let ch = character(serde_json::json!({ "prompt_format": "%h> " }));
        assert_eq!(format_for(&ch), "%h> ");
    }

    #[test]
    fn a_custom_format_outranks_both_presets() {
        assert_eq!(resolve_format("", ""), SIMPLE_FORMAT);
        assert_eq!(resolve_format("", "simple"), SIMPLE_FORMAT);
        assert_eq!(resolve_format("", "verbose"), VERBOSE_FORMAT);
        assert_eq!(resolve_format("%h> ", "verbose"), "%h> ");
        // Which is why `prompt simple` has to clear the stored format.
        assert_eq!(resolve_format("%h> ", "simple"), "%h> ");
    }

    #[test]
    fn the_default_formats_only_use_known_tokens() {
        assert!(validate(VERBOSE_FORMAT).is_ok());
        assert!(validate(SIMPLE_FORMAT).is_ok());
    }
}

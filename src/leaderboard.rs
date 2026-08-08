//! Leaderboards — the last thing Tier 1 owes the player: a way to be known
//! relative to everyone else.
//!
//! Every other consequence system in the tier answers "the world noticed."
//! This one answers "and so did everybody else." It is deliberately
//! **read-only**: nothing here writes a character, gates anything, or feeds
//! back into balance. It reads what the rest of the game already stores and
//! sorts it.
//!
//! # Data-driven, not a list of boards
//!
//! There is no hand-maintained board registry. The categories are *discovered*
//! from the characters themselves — every achievement counter anyone has
//! bumped, every skill anyone has trained, every faction anyone has a standing
//! with becomes a board. Adding a counter to the engine adds a board with no
//! code here; so does a builder publishing a custom skill or tagging a new
//! faction. Only the handful of [`DERIVED`] boards, which are functions of a
//! character rather than a stored tally, are named in code.
//!
//! # Why this is computed on a tick
//!
//! Ranking needs every character, and `db::list_all_characters` deserializes
//! the whole tree. That must never happen on a player's command. The tick
//! recomputes on a cadence measured in minutes and drops the result into
//! `World.leaderboards`; the `top` command reads only that cache, so the
//! command path costs one lock and a map lookup regardless of how many
//! characters exist.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::reputation::{FactionDefinition, ReputationTier};
use crate::types::CharacterData;

/// Rows kept per board. Ten is the whole point of a leaderboard: being
/// eleventh has to mean something, or the list is a directory.
pub const BOARD_SIZE: usize = 10;

/// How often the scan runs.
///
/// Five minutes, for two reasons. It is the most expensive read the server
/// performs, and — the design one — a board that moved the instant you killed
/// something would turn the ranking into the game. A leaderboard is a record
/// of play, and a record is allowed to lag.
pub const LEADERBOARD_TICK_INTERVAL_SECS: u64 = 300;

/// Display order for board groups. A group not named here sorts last, so a
/// counter nobody has categorised still appears rather than vanishing.
pub const GROUP_ORDER: &[&str] = &[
    "Standing",
    "Combat",
    "Exploration",
    "Craft",
    "Wealth",
    "Social",
    "Skills",
    "Factions",
    "Quarry",
    "Building",
    "Other",
];

/// The one group whose boards rank builders rather than players.
///
/// Two rules differ for it, and both are consequences of the same fact — the
/// people who build a world are usually the people who run it:
///
/// * **Admins are ranked on it.** Every other board excludes them, because a
///   god with test data would top everything. A builder board that excluded
///   admins would be permanently empty.
/// * **Only builders see it.** `top` hides the group from everyone else. A
///   player has no way to compete on it and no reason to care.
pub const BUILDING_GROUP: &str = "Building";

/// Boards computed from the character rather than read off it.
///
/// `(key, label, group, descending)`. These are the only categories named in
/// code — everything else is discovered from the data.
const DERIVED: &[(&str, &str, &str, bool)] = &[
    ("renown", "Renown", "Standing", true),
    ("achievements", "Achievements Unlocked", "Standing", true),
    ("quests", "Quests Completed", "Standing", true),
    ("mastered", "Skills Mastered", "Skills", true),
    ("gold", "Gold Carried", "Wealth", true),
    ("wealth", "Richest Ever", "Wealth", true),
    ("virtue", "Most Virtuous", "Standing", true),
    ("infamy", "Most Wicked", "Standing", false),
];

/// Human labels and groups for the counters the engine ships. A counter with
/// no entry here still gets a board — it just wears a title-cased version of
/// its own key. That is the property that makes "add a counter, get a board"
/// true without an edit here.
const COUNTER_META: &[(&str, &str, &str)] = &[
    ("kills.any", "Kills", "Combat"),
    ("deaths", "Deaths", "Combat"),
    ("rooms.visited", "Rooms Explored", "Exploration"),
    ("socials.distinct", "Socials Performed", "Social"),
    ("npcs.talked_to", "Folk Spoken To", "Social"),
    ("npcs.befriended", "Friends Made", "Social"),
    ("mail.sent", "Letters Sent", "Social"),
    ("board.posts", "Notices Posted", "Social"),
    ("recipes.learned", "Recipes Known", "Craft"),
    ("recipes.discovered", "Recipes Discovered", "Craft"),
    ("items.crafted", "Items Crafted", "Craft"),
    ("meals.cooked", "Meals Cooked", "Craft"),
    ("fish.landed", "Fish Landed", "Craft"),
    ("plants.harvested", "Plants Harvested", "Craft"),
    ("spells.cast", "Spells Cast", "Combat"),
    ("gold.spent", "Gold Spent", "Wealth"),
    ("gold.earned", "Gold Earned", "Wealth"),
    ("leases.bought", "Properties Leased", "Wealth"),
    ("items.sold", "Goods Sold", "Wealth"),
    ("items.bought", "Goods Bought", "Wealth"),
    // Builder boards. Written by the build-score tick (src/build_score.rs),
    // reconciled from a world scan rather than accumulated, so they fall when
    // content is deleted.
    ("build.score", "Builder Points", BUILDING_GROUP),
    ("build.rooms", "Rooms Built", BUILDING_GROUP),
    ("build.items", "Items Built", BUILDING_GROUP),
    ("build.mobiles", "Mobiles Built", BUILDING_GROUP),
    ("build.quests", "Quests Written", BUILDING_GROUP),
    ("build.areas", "Areas Built", BUILDING_GROUP),
    ("build.excellent", "A-Grade Content", BUILDING_GROUP),
    ("build.bounties", "Bounties Filled", BUILDING_GROUP),
];

/// Counters deliberately given no board, because a [`DERIVED`] one covers the
/// same ground from better data.
///
/// Both of these are tallies bumped going forward, while the set or map they
/// shadow is the authority and is right for characters who did the thing
/// before the counter existed. Two boards disagreeing about the same fact is
/// worse than one board.
const SUPERSEDED: &[&str] = &[
    "quests.completed", // → derived "quests", from `completed_quests`
    "skills_maxed",     // → derived "mastered", from `skills`
];

/// What a board is ranking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardKind {
    /// An achievement counter, ranked on its raw tally.
    Counter,
    /// A skill, ranked on level with experience breaking ties.
    Skill,
    /// Standing with one faction, shown as a band name.
    Reputation,
    /// A function of the character rather than a stored field.
    Derived,
}

impl BoardKind {
    pub fn key(self) -> &'static str {
        match self {
            BoardKind::Counter => "counter",
            BoardKind::Skill => "skill",
            BoardKind::Reputation => "reputation",
            BoardKind::Derived => "derived",
        }
    }

    /// Prefix used to disambiguate a key two kinds both claim.
    fn prefix(self) -> &'static str {
        self.key()
    }
}

/// One row.
#[derive(Clone, Debug)]
pub struct BoardEntry {
    /// Competition rank: equal values share a rank and the next one skips.
    pub rank: i32,
    pub name: String,
    pub value: i64,
    /// The value as a human reads it — a tally, `level 7`, or a band name.
    pub display: String,
}

#[derive(Clone, Debug)]
pub struct Board {
    pub key: String,
    pub label: String,
    pub group: String,
    pub kind: BoardKind,
    /// Highest first, except boards with `descending == false` (infamy),
    /// which put the most negative first.
    pub entries: Vec<BoardEntry>,
    /// How many characters qualified before truncation to [`BOARD_SIZE`], so
    /// a display can say "of 214" rather than implying the world has ten
    /// people in it.
    pub ranked: i32,
    /// Every ranked character's placing, keyed by lowercased name — including
    /// the ones truncation dropped. A board that shows ten rows tells the
    /// eleventh player nothing; this is what lets `top` answer "you are 34th
    /// of 214" instead.
    ///
    /// Bounded by the size of the underlying data, not by boards × players: a
    /// character appears here once per key they actually have a figure for.
    pub placings: HashMap<String, i32>,
}

/// The cached result of one scan. Lives in `World`; replaced wholesale by the
/// tick rather than mutated, so a reader either sees the previous scan or the
/// new one and never a half-built board.
#[derive(Clone, Debug, Default)]
pub struct Leaderboards {
    /// Unix seconds the scan ran. 0 means "never computed" — the state the
    /// cache is in for the first few seconds after boot.
    pub generated_at: i64,
    /// Characters considered, after admins were excluded.
    pub characters_scanned: i32,
    pub boards: BTreeMap<String, Board>,
}

impl Board {
    /// Where this character placed, or `None` if they do not rank here at all.
    pub fn placing(&self, char_name: &str) -> Option<i32> {
        self.placings.get(&char_name.to_lowercase()).copied()
    }
}

impl Leaderboards {
    /// Every board a character ranks on, best placing first.
    ///
    /// This is the answer to "what am I actually good at", which no single
    /// board can give — a player who is 300th at killing may be 2nd at
    /// cooking, and only a sweep across boards will ever tell them so.
    pub fn placings_for(&self, char_name: &str) -> Vec<(&Board, i32)> {
        let name = char_name.to_lowercase();
        let mut out: Vec<(&Board, i32)> = self
            .boards
            .values()
            .filter_map(|b| b.placings.get(&name).map(|r| (b, *r)))
            .collect();
        out.sort_by(|a, b| {
            a.1.cmp(&b.1)
                .then_with(|| group_rank(&a.0.group).cmp(&group_rank(&b.0.group)))
                .then_with(|| a.0.label.cmp(&b.0.label))
        });
        out
    }

    pub fn is_empty(&self) -> bool {
        self.boards.is_empty()
    }

    /// Board keys in display order: by [`GROUP_ORDER`], then alphabetically
    /// inside a group.
    pub fn ordered_keys(&self) -> Vec<&str> {
        let mut keys: Vec<&Board> = self.boards.values().collect();
        keys.sort_by(|a, b| {
            group_rank(&a.group)
                .cmp(&group_rank(&b.group))
                .then_with(|| a.label.cmp(&b.label))
                .then_with(|| a.key.cmp(&b.key))
        });
        keys.into_iter().map(|b| b.key.as_str()).collect()
    }

    /// Resolve a player-typed word to a board.
    ///
    /// Exact key, then case-insensitive key, then exact label, then a prefix
    /// of either. The prefix pass is what makes `top kills` find `kills.any`
    /// and `top short` find `short_blades` without either being aliased.
    pub fn resolve(&self, want: &str) -> Option<&Board> {
        let want = want.trim().to_lowercase();
        if want.is_empty() {
            return None;
        }
        if let Some(b) = self.boards.get(&want) {
            return Some(b);
        }
        let ordered: Vec<&Board> = self.ordered_keys().iter().filter_map(|k| self.boards.get(*k)).collect();
        for b in &ordered {
            if b.label.to_lowercase() == want {
                return Some(b);
            }
        }
        for b in &ordered {
            if b.key.to_lowercase().starts_with(&want) {
                return Some(b);
            }
        }
        for b in &ordered {
            if b.label.to_lowercase().starts_with(&want) {
                return Some(b);
            }
        }
        None
    }
}

/// Board names worth offering to tab completion.
///
/// Completion cannot see the cache — it runs on a keystroke, before any lock
/// is worth taking — so it gets the boards that are named in code and always
/// exist once anyone has done the thing. Boards discovered from data
/// (per-mobile kills, factions, builder skills) are absent here and found via
/// `top boards`, which is the surface built for browsing them.
///
/// Derived from the same constants the scan uses, so this cannot drift into a
/// second list of board names.
pub fn completion_hints() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = vec!["boards", "me"];
    out.extend(DERIVED.iter().map(|(k, _, _, _)| *k));
    out.extend(
        COUNTER_META
            .iter()
            .map(|(k, _, _)| *k)
            .filter(|k| !SUPERSEDED.contains(k)),
    );
    out.extend(crate::progress::CORE_SKILLS.iter().copied());
    out.sort_unstable();
    out.dedup();
    out
}

fn group_rank(group: &str) -> usize {
    GROUP_ORDER
        .iter()
        .position(|g| *g == group)
        .unwrap_or(GROUP_ORDER.len())
}

/// One character's showing on one board. `sort` exists separately from
/// `value` because a skill ranks on level but ties break on experience, and
/// collapsing both into the displayed number would lie about it.
struct Sample {
    sort: i64,
    value: i64,
    display: String,
}

struct Category {
    /// The board's identity — what a player types and what the cache is keyed
    /// on. May be qualified (`skill:gold`) when two kinds want the same word.
    key: String,
    /// The key the figure is actually read under: a skill name, a counter
    /// name, a faction tag. Distinct from `key` because qualifying a board to
    /// resolve a collision must not change where its data comes from.
    source: String,
    label: String,
    group: &'static str,
    kind: BoardKind,
    descending: bool,
    /// Whether admins are ranked on this board.
    ///
    /// False everywhere except the Building group. It is a per-category
    /// predicate rather than one global filter because the two answers are
    /// genuinely different questions: "should a god's test character top the
    /// kill board" (no) and "should the person who built the world appear on
    /// the board that ranks building" (obviously).
    include_admins: bool,
}

/// Title-case a snake_case or dotted key for a board with no declared label:
/// `short_blades` → `Short Blades`, `kills.dragon` → `Kills Dragon`.
fn humanize(key: &str) -> String {
    key.split(['_', '.', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut cs = w.chars();
            match cs.next() {
                Some(c) => c.to_uppercase().collect::<String>() + cs.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Work out which boards exist, from the data rather than from a list.
fn categories(chars: &[&CharacterData], factions: &HashMap<String, FactionDefinition>) -> Vec<Category> {
    let mut out: Vec<Category> = Vec::new();
    let mut claimed: HashSet<String> = HashSet::new();

    // Derived first, so a builder who publishes a custom skill called "gold"
    // cannot displace the gold board.
    for (key, label, group, descending) in DERIVED {
        claimed.insert((*key).to_string());
        out.push(Category {
            key: (*key).to_string(),
            source: (*key).to_string(),
            label: (*label).to_string(),
            group,
            kind: BoardKind::Derived,
            descending: *descending,
            include_admins: *group == BUILDING_GROUP,
        });
    }

    let push = |out: &mut Vec<Category>, claimed: &mut HashSet<String>, cat: Category| {
        // A key two kinds both want keeps the higher-priority board at the
        // bare name and gives the loser a qualified one, so nothing is
        // silently dropped just because two systems chose the same word.
        let key = if claimed.contains(&cat.key) {
            format!("{}:{}", cat.kind.prefix(), cat.key)
        } else {
            cat.key.clone()
        };
        if claimed.contains(&key) {
            return;
        }
        claimed.insert(key.clone());
        // `source` is left alone deliberately — see the field comment.
        out.push(Category { key, ..cat });
    };

    let mut skills: HashSet<&str> = HashSet::new();
    let mut counters: HashSet<&str> = HashSet::new();
    let mut standings: HashSet<&str> = HashSet::new();
    for ch in chars {
        for k in ch.skills.keys() {
            skills.insert(k.as_str());
        }
        for k in ch.achievement_counters.keys() {
            counters.insert(k.as_str());
        }
        for k in ch.reputation.keys() {
            standings.insert(k.as_str());
        }
    }

    let mut skills: Vec<&str> = skills.into_iter().collect();
    skills.sort_unstable();
    for key in skills {
        push(
            &mut out,
            &mut claimed,
            Category {
                key: key.to_string(),
                source: key.to_string(),
                label: humanize(key),
                group: "Skills",
                kind: BoardKind::Skill,
                descending: true,
                include_admins: false,
            },
        );
    }

    let meta: HashMap<&str, (&str, &str)> = COUNTER_META.iter().map(|(k, l, g)| (*k, (*l, *g))).collect();
    let mut counters: Vec<&str> = counters.into_iter().filter(|k| !SUPERSEDED.contains(k)).collect();
    counters.sort_unstable();
    for key in counters {
        let (label, group) = match meta.get(key) {
            Some((l, g)) => ((*l).to_string(), *g),
            // Per-mobile kill tallies are generated at kill time, one key per
            // prototype, so a busy world has hundreds. They belong on their
            // own group rather than swamping Combat.
            None if key.starts_with("kills.") => (humanize(key), "Quarry"),
            None => (humanize(key), "Other"),
        };
        push(
            &mut out,
            &mut claimed,
            Category {
                key: key.to_string(),
                source: key.to_string(),
                label,
                group,
                kind: BoardKind::Counter,
                descending: true,
                include_admins: group == BUILDING_GROUP,
            },
        );
    }

    let mut standings: Vec<&str> = standings.into_iter().collect();
    standings.sort_unstable();
    for key in standings {
        let label = match factions.get(key) {
            Some(def) => def.display().to_string(),
            None => humanize(key),
        };
        push(
            &mut out,
            &mut claimed,
            Category {
                key: key.to_string(),
                source: key.to_string(),
                label: label.clone(),
                group: "Factions",
                kind: BoardKind::Reputation,
                descending: true,
                include_admins: false,
            },
        );
        // The other end of the same field, exactly as `virtue` and `infamy`
        // are one morality column read twice. Without it the "a zero is not a
        // rank" rule silently means being Hated by four factions places you
        // nowhere — the game would rank the people a faction likes and have
        // nothing to say about the people it wants dead, which is the more
        // interesting half. `compute` drops a board nobody qualifies for, so
        // this costs nothing until somebody has actually earned it.
        push(
            &mut out,
            &mut claimed,
            Category {
                key: format!("{}.wanted", key),
                source: key.to_string(),
                label: format!("Wanted by {}", label),
                group: "Factions",
                kind: BoardKind::Reputation,
                descending: false,
                include_admins: false,
            },
        );
    }

    out
}

/// One character's figure for one category, or `None` if they do not appear
/// on it at all.
fn sample(cat: &Category, ch: &CharacterData) -> Option<Sample> {
    let plain = |v: i64| {
        Some(Sample {
            sort: v,
            value: v,
            display: v.to_string(),
        })
    };

    match cat.kind {
        BoardKind::Derived => match cat.source.as_str() {
            "renown" => plain(crate::progress::renown(ch).total as i64),
            "achievements" => plain(ch.achievements_unlocked.len() as i64),
            "quests" => plain(ch.completed_quests.len() as i64),
            "mastered" => plain(crate::progress::renown(ch).core_mastered as i64),
            "gold" => plain(ch.gold as i64),
            "wealth" => plain(ch.gold_high_water as i64),
            "virtue" | "infamy" => {
                let v = ch.morality as i64;
                Some(Sample {
                    sort: v,
                    value: v,
                    display: crate::morality::MoralityTier::from_value(ch.morality)
                        .label()
                        .to_string(),
                })
            }
            _ => None,
        },
        BoardKind::Skill => {
            let sp = ch.skills.get(&cat.source)?;
            Some(Sample {
                // Level is the rank; experience only separates a tie, so it
                // occupies the low digits and can never lift one level over
                // another (the curve tops out well under a million).
                sort: sp.level as i64 * 1_000_000 + sp.experience.max(0) as i64,
                value: sp.level as i64,
                display: format!("level {}", sp.level),
            })
        }
        BoardKind::Counter => plain(*ch.achievement_counters.get(&cat.source)? as i64),
        BoardKind::Reputation => {
            let v = *ch.reputation.get(&cat.source)? as i64;
            Some(Sample {
                sort: v,
                value: v,
                display: ReputationTier::from_value(v as i32).label().to_string(),
            })
        }
    }
}

/// Rank and truncate. Ties share a rank and the following rank skips, so a
/// three-way tie for first is followed by fourth — the convention every
/// scoreboard a player has ever seen uses.
fn rank(mut rows: Vec<(String, Sample)>, descending: bool) -> (Vec<BoardEntry>, i32, HashMap<String, i32>) {
    rows.sort_by(|a, b| {
        let by_value = if descending {
            b.1.sort.cmp(&a.1.sort)
        } else {
            a.1.sort.cmp(&b.1.sort)
        };
        // Names break remaining ties so two runs over unchanged data produce
        // identical boards.
        by_value.then_with(|| a.0.cmp(&b.0))
    });

    let ranked = rows.len() as i32;
    let mut entries: Vec<BoardEntry> = Vec::with_capacity(BOARD_SIZE.min(rows.len()));
    let mut placings: HashMap<String, i32> = HashMap::with_capacity(rows.len());
    let mut last: Option<i64> = None;
    let mut current = 0;
    // Runs to the end of the field, not to BOARD_SIZE — `placings` is the
    // half that has to cover everyone.
    for (i, (name, s)) in rows.into_iter().enumerate() {
        if last != Some(s.sort) {
            current = i as i32 + 1;
            last = Some(s.sort);
        }
        placings.insert(name.to_lowercase(), current);
        if entries.len() >= BOARD_SIZE {
            continue;
        }
        entries.push(BoardEntry {
            rank: current,
            name,
            value: s.value,
            display: s.display,
        });
    }
    (entries, ranked, placings)
}

/// Build every board from a full character scan.
///
/// Pure: takes the characters and the faction registry, returns the cache.
/// The caller owns both the read and the write, which is what keeps the World
/// lock off the expensive part.
///
/// Two rules apply to every board, so that none needs a special case:
///
/// * **Admins are excluded**, unless the board says otherwise. A god with test
///   data would top everything, and a leaderboard players cannot compete on is
///   not one. The exception is [`BUILDING_GROUP`], where excluding admins
///   would leave the boards permanently empty — the people who build a world
///   are usually the people who run it.
/// * **A zero is not a rank.** An entry appears only if its value points the
///   way the board is sorted — positive on a descending board, negative on an
///   ascending one. Otherwise every board would open with a wall of people
///   who have never done the thing.
pub fn compute(
    chars: &[CharacterData],
    factions: &HashMap<String, FactionDefinition>,
    generated_at: i64,
) -> Leaderboards {
    // Discovery runs over everyone, and eligibility is decided per board
    // below. Discovering from non-admins alone would mean a counter only
    // admins carry — every `build.*` key, on most servers — never produces a
    // board at all. A category discovered from admin data that does not admit
    // admins simply ends up with no rows, and empty boards are dropped.
    let all: Vec<&CharacterData> = chars.iter().collect();
    let cats = categories(&all, factions);

    let mut boards = BTreeMap::new();
    for cat in cats {
        let mut rows: Vec<(String, Sample)> = Vec::new();
        for ch in &all {
            if ch.is_admin && !cat.include_admins {
                continue;
            }
            let Some(s) = sample(&cat, ch) else { continue };
            let counts = if cat.descending { s.value > 0 } else { s.value < 0 };
            if !counts {
                continue;
            }
            rows.push((ch.name.clone(), s));
        }
        if rows.is_empty() {
            continue;
        }
        let (entries, ranked, placings) = rank(rows, cat.descending);
        boards.insert(
            cat.key.clone(),
            Board {
                key: cat.key,
                label: cat.label,
                group: cat.group.to_string(),
                kind: cat.kind,
                entries,
                ranked,
                placings,
            },
        );
    }

    Leaderboards {
        generated_at,
        // The player population, which is what the freshness line means by
        // "of N" — builder boards rank a different, smaller field and carry
        // their own `ranked` count.
        characters_scanned: all.iter().filter(|c| !c.is_admin).count() as i32,
        boards,
    }
}

/// Rescan the world and replace the cache.
///
/// The World lock is taken twice and briefly — once to copy the faction
/// registry out, once to install the finished result. Neither the character
/// read nor the ranking happens under it, which is the whole reason this is a
/// tick rather than a command.
///
/// Lives lib-side so integration tests can drive it without a runtime; the
/// tokio wrapper is `src/ticks/leaderboard.rs`.
pub fn process_leaderboard_tick(db: &crate::db::Db, state: &crate::SharedState) -> anyhow::Result<()> {
    let factions = {
        let world = state
            .lock()
            .map_err(|_| anyhow::anyhow!("world lock poisoned during leaderboard scan"))?;
        world.faction_definitions.clone()
    };

    let chars = db.list_all_characters()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let boards = compute(&chars, &factions, now);

    let mut world = state
        .lock()
        .map_err(|_| anyhow::anyhow!("world lock poisoned installing leaderboards"))?;
    world.leaderboards = boards;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SkillProgress;

    /// `CharacterData` has no `Default`; build it from JSON and let serde's
    /// field defaults fill the rest, the same way the progress tests do.
    fn ch(name: &str) -> CharacterData {
        serde_json::from_value(serde_json::json!({
            "name": name,
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character")
    }

    fn with_counter(name: &str, key: &str, n: u32) -> CharacterData {
        let mut c = ch(name);
        c.achievement_counters.insert(key.to_string(), n);
        c
    }

    fn compute_now(chars: &[CharacterData]) -> Leaderboards {
        compute(chars, &HashMap::new(), 1000)
    }

    #[test]
    fn a_counter_nobody_named_still_gets_a_board() {
        // The whole data-driven claim: no edit to this file is needed for a
        // new counter to rank.
        let boards = compute_now(&[with_counter("Ada", "spanners.thrown", 4)]);
        let b = boards.boards.get("spanners.thrown").expect("board exists");
        assert_eq!(b.label, "Spanners Thrown");
        assert_eq!(b.entries[0].name, "Ada");
    }

    #[test]
    fn admins_are_left_off() {
        let mut god = with_counter("Zeus", "kills.any", 9999);
        god.is_admin = true;
        let boards = compute_now(&[god, with_counter("Ada", "kills.any", 3)]);
        let b = boards.boards.get("kills.any").expect("board exists");
        assert_eq!(b.entries.len(), 1);
        assert_eq!(b.entries[0].name, "Ada");
        assert_eq!(boards.characters_scanned, 1);
    }

    #[test]
    fn admins_rank_on_builder_boards_and_only_there() {
        // The people who build a world are usually the people who run it, so
        // the global admin filter would leave every builder board empty. The
        // exemption must be exactly this group and no wider.
        let mut god = with_counter("Zeus", "build.rooms", 40);
        god.is_admin = true;
        god.achievement_counters.insert("kills.any".into(), 9999);

        let boards = compute_now(&[god, with_counter("Ada", "kills.any", 3)]);

        let building = boards.boards.get("build.rooms").expect("builder board exists");
        assert_eq!(building.group, BUILDING_GROUP);
        assert_eq!(building.entries.len(), 1);
        assert_eq!(building.entries[0].name, "Zeus");

        let combat = boards.boards.get("kills.any").expect("kill board exists");
        assert_eq!(
            combat.entries.len(),
            1,
            "the admin exemption leaked outside the Building group"
        );
        assert_eq!(combat.entries[0].name, "Ada");
    }

    #[test]
    fn a_builder_counter_only_an_admin_carries_still_produces_a_board() {
        // Discovery has to run over everyone. Scanning non-admins for
        // categories would mean that on a server where only admins build, the
        // builder boards would not exist at all.
        let mut god = with_counter("Zeus", "build.score", 250);
        god.is_admin = true;
        let boards = compute_now(&[god, ch("Ada")]);
        assert!(boards.boards.contains_key("build.score"));
        assert_eq!(boards.boards["build.score"].label, "Builder Points");
    }

    #[test]
    fn every_builder_counter_has_a_label_and_lands_in_the_building_group() {
        // A `build.*` key missing from COUNTER_META would silently land in
        // "Other" — visible to every player, and outside the admin exemption
        // that makes it rankable at all.
        for (key, value) in crate::build_score::BuilderScore::default().counters() {
            let _ = value;
            let meta = COUNTER_META.iter().find(|(k, _, _)| *k == key);
            let (_, label, group) = meta.unwrap_or_else(|| panic!("{key} is not in COUNTER_META"));
            assert_eq!(*group, BUILDING_GROUP, "{key} is not a builder board");
            assert!(!label.is_empty());
        }
    }

    #[test]
    fn a_zero_is_not_a_rank() {
        let boards = compute_now(&[with_counter("Ada", "kills.any", 0), with_counter("Bo", "kills.any", 2)]);
        let b = boards.boards.get("kills.any").expect("board exists");
        assert_eq!(b.entries.len(), 1);
        assert_eq!(b.entries[0].name, "Bo");
    }

    /// A faction ranks both the people it likes and the people it wants dead.
    ///
    /// "A zero is not a rank" plus a descending board meant negative standing
    /// placed nowhere, so being Hated by four factions was invisible — the
    /// game ranked a faction's friends and had nothing to say about its
    /// enemies, which is the more interesting half. Morality already got both
    /// ends out of one column (virtue / infamy); this is the same move for
    /// factions, and it costs nothing until someone earns it, since `compute`
    /// drops a board nobody qualifies for.
    #[test]
    fn a_faction_ranks_its_enemies_as_well_as_its_friends() {
        let mut friend = ch("Ada");
        friend.reputation.insert("town_watch".to_string(), 400);
        let mut enemy = ch("Bo");
        enemy.reputation.insert("town_watch".to_string(), -600);

        let boards = compute_now(&[friend, enemy]);

        let standing = boards.boards.get("town_watch").expect("standing board");
        assert_eq!(standing.entries.len(), 1, "only the friend ranks here");
        assert_eq!(standing.entries[0].name, "Ada");

        let wanted = boards.boards.get("town_watch.wanted").expect("wanted board");
        assert_eq!(wanted.entries.len(), 1);
        assert_eq!(wanted.entries[0].name, "Bo");
        assert_eq!(
            wanted.entries[0].display, "Hated",
            "shown as a band name, never a raw integer"
        );
    }

    #[test]
    fn a_faction_with_no_enemies_gets_no_wanted_board() {
        let mut friend = ch("Ada");
        friend.reputation.insert("town_watch".to_string(), 400);
        let boards = compute_now(&[friend]);
        assert!(boards.boards.contains_key("town_watch"));
        assert!(
            !boards.boards.contains_key("town_watch.wanted"),
            "an empty board is not a board"
        );
    }

    #[test]
    fn a_board_nobody_qualifies_for_does_not_exist() {
        let boards = compute_now(&[with_counter("Ada", "kills.any", 0)]);
        assert!(boards.boards.get("kills.any").is_none());
    }

    #[test]
    fn ties_share_a_rank_and_the_next_one_skips() {
        let boards = compute_now(&[
            with_counter("Ada", "kills.any", 5),
            with_counter("Bo", "kills.any", 5),
            with_counter("Cy", "kills.any", 1),
        ]);
        let e = &boards.boards.get("kills.any").unwrap().entries;
        assert_eq!((e[0].rank, e[0].name.as_str()), (1, "Ada"));
        assert_eq!((e[1].rank, e[1].name.as_str()), (1, "Bo"));
        assert_eq!((e[2].rank, e[2].name.as_str()), (3, "Cy"));
    }

    #[test]
    fn the_board_truncates_but_still_reports_the_field_size() {
        let chars: Vec<CharacterData> = (0..25)
            .map(|i| with_counter(&format!("Player{i:02}"), "kills.any", i + 1))
            .collect();
        let b = compute_now(&chars).boards.get("kills.any").cloned().unwrap();
        assert_eq!(b.entries.len(), BOARD_SIZE);
        assert_eq!(b.ranked, 25);
        assert_eq!(b.entries[0].name, "Player24");
    }

    #[test]
    fn skills_rank_on_level_with_experience_only_breaking_ties() {
        let mut a = ch("Ada");
        a.skills.insert(
            "cooking".into(),
            SkillProgress {
                level: 4,
                experience: 10,
            },
        );
        let mut b = ch("Bo");
        b.skills.insert(
            "cooking".into(),
            SkillProgress {
                level: 4,
                experience: 900,
            },
        );
        let mut c = ch("Cy");
        c.skills.insert(
            "cooking".into(),
            SkillProgress {
                level: 5,
                experience: 0,
            },
        );

        let board = compute_now(&[a, b, c]).boards.get("cooking").cloned().unwrap();
        let names: Vec<&str> = board.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["Cy", "Bo", "Ada"]);
        // Displayed rank is the level; the experience tie-break must not leak
        // into what the player reads.
        assert_eq!(board.entries[1].display, "level 4");
        assert_eq!(board.entries[1].value, 4);
    }

    #[test]
    fn morality_produces_a_board_at_each_end_and_neutrals_appear_on_neither() {
        let mut saint = ch("Ada");
        saint.morality = 120;
        let mut monster = ch("Bo");
        monster.morality = -120;
        let middling = ch("Cy");

        let boards = compute_now(&[saint, monster, middling]);
        let virtue = boards.boards.get("virtue").unwrap();
        let infamy = boards.boards.get("infamy").unwrap();
        assert_eq!(virtue.entries.len(), 1);
        assert_eq!(virtue.entries[0].name, "Ada");
        assert_eq!(infamy.entries.len(), 1);
        assert_eq!(infamy.entries[0].name, "Bo");
        // Bands, not raw integers — the same rule `standing` and worship
        // Favor follow.
        assert_eq!(infamy.entries[0].display, "Pure Evil");
    }

    #[test]
    fn reputation_boards_use_the_faction_display_name_when_one_is_registered() {
        let mut a = ch("Ada");
        a.reputation.insert("iron_guard".into(), 300);
        let mut factions = HashMap::new();
        factions.insert(
            "iron_guard".to_string(),
            FactionDefinition {
                name: "The Iron Guard".into(),
                ..FactionDefinition::unregistered("iron_guard")
            },
        );

        let boards = compute(&[a], &factions, 1);
        let b = boards.boards.get("iron_guard").unwrap();
        assert_eq!(b.label, "The Iron Guard");
        assert_eq!(b.entries[0].display, "Honored");
    }

    #[test]
    fn an_unregistered_faction_tag_still_ranks() {
        // Ad-hoc warbands are a supported shape; a board should not need a
        // registry entry any more than aggro does.
        let mut a = ch("Ada");
        a.reputation.insert("ratcatchers".into(), 80);
        let b = compute_now(&[a]);
        assert_eq!(b.boards.get("ratcatchers").unwrap().label, "Ratcatchers");
    }

    #[test]
    fn superseded_counters_get_no_board_of_their_own() {
        let mut a = with_counter("Ada", "quests.completed", 7);
        a.achievement_counters.insert("skills_maxed".into(), 3);
        let boards = compute_now(&[a]);
        assert!(boards.boards.get("quests.completed").is_none());
        assert!(boards.boards.get("skills_maxed").is_none());
    }

    #[test]
    fn a_derived_key_keeps_the_bare_name_and_the_collider_is_qualified() {
        // A builder publishing a skill called "gold" must not take the gold
        // board's name — but must not lose their board either.
        let mut a = ch("Ada");
        a.gold = 50;
        a.skills.insert(
            "gold".into(),
            SkillProgress {
                level: 3,
                experience: 0,
            },
        );
        let boards = compute_now(&[a]);
        assert_eq!(boards.boards.get("gold").unwrap().kind, BoardKind::Derived);
        assert_eq!(boards.boards.get("skill:gold").unwrap().kind, BoardKind::Skill);
    }

    #[test]
    fn per_mobile_kill_tallies_land_in_their_own_group() {
        // Hundreds of these exist in a busy world; they must not bury the
        // handful of boards in Combat.
        let boards = compute_now(&[with_counter("Ada", "kills.dragon", 2)]);
        assert_eq!(boards.boards.get("kills.dragon").unwrap().group, "Quarry");
        let boards = compute_now(&[with_counter("Ada", "kills.any", 2)]);
        assert_eq!(boards.boards.get("kills.any").unwrap().group, "Combat");
    }

    #[test]
    fn resolve_accepts_a_prefix_of_a_key_or_a_label() {
        let mut a = with_counter("Ada", "kills.any", 3);
        a.skills.insert(
            "short_blades".into(),
            SkillProgress {
                level: 2,
                experience: 0,
            },
        );
        let boards = compute_now(&[a]);
        assert_eq!(boards.resolve("kills.any").unwrap().key, "kills.any");
        assert_eq!(boards.resolve("KILLS").unwrap().key, "kills.any");
        assert_eq!(boards.resolve("short").unwrap().key, "short_blades");
        assert_eq!(boards.resolve("Short Blades").unwrap().key, "short_blades");
        assert!(boards.resolve("nothing-like-this").is_none());
        assert!(boards.resolve("  ").is_none());
    }

    #[test]
    fn groups_order_ahead_of_alphabetical_within_them() {
        let mut a = with_counter("Ada", "kills.any", 3);
        a.skills.insert(
            "cooking".into(),
            SkillProgress {
                level: 2,
                experience: 0,
            },
        );
        a.gold = 10;
        let boards = compute_now(&[a]);
        let keys = boards.ordered_keys();
        let pos = |k: &str| keys.iter().position(|x| *x == k).expect("present");
        // Standing < Combat < Wealth < Skills, per GROUP_ORDER.
        assert!(pos("renown") < pos("kills.any"));
        assert!(pos("kills.any") < pos("gold"));
        assert!(pos("gold") < pos("cooking"));
    }

    #[test]
    fn placings_cover_the_field_past_the_visible_rows() {
        let chars: Vec<CharacterData> = (0..25)
            .map(|i| with_counter(&format!("Player{i:02}"), "kills.any", i + 1))
            .collect();
        let b = compute_now(&chars).boards.get("kills.any").cloned().unwrap();
        // Player00 has the lowest tally and is nowhere near the ten rows the
        // board shows, but still has a placing.
        assert!(b.entries.iter().all(|e| e.name != "Player00"));
        assert_eq!(b.placing("Player00"), Some(25));
        assert_eq!(b.placing("player24"), Some(1)); // name match is case-insensitive
        assert_eq!(b.placing("Nobody"), None);
    }

    #[test]
    fn placings_for_sweeps_every_board_best_first() {
        let mut ada = with_counter("Ada", "kills.any", 1);
        ada.skills.insert(
            "cooking".into(),
            SkillProgress {
                level: 9,
                experience: 0,
            },
        );
        let mut bo = with_counter("Bo", "kills.any", 99);
        bo.skills.insert(
            "cooking".into(),
            SkillProgress {
                level: 1,
                experience: 0,
            },
        );

        let boards = compute_now(&[ada, bo]);
        let mine: Vec<(&str, i32)> = boards
            .placings_for("ada")
            .into_iter()
            .map(|(b, r)| (b.key.as_str(), r))
            .collect();
        // First at cooking, second at killing — and the good news leads. The
        // renown board rides along because skill levels feed it; equal ranks
        // fall back to group order, which puts Standing ahead of Skills.
        assert_eq!(mine, vec![("renown", 1), ("cooking", 1), ("kills.any", 2)]);
        assert!(boards.placings_for("nobody").is_empty());
    }

    #[test]
    fn an_empty_world_produces_an_empty_cache_rather_than_empty_boards() {
        let boards = compute_now(&[]);
        assert!(boards.is_empty());
        assert_eq!(boards.characters_scanned, 0);
    }
}

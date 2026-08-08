//! Progress tracks: checklists that tell a builder what to do next.
//!
//! `build audit` says what is wrong with something that exists. A track says
//! what does not exist yet, which is the harder half of the blank page. The
//! shape is deliberately the one players already read — a list of steps with
//! ticks against them — because a builder who plays this game knows how a
//! quest log works and should not have to learn a second idiom.
//!
//! Two scopes ship, and they answer two different questions:
//!
//! * **Area Readiness** — is this area finished? Every step is something a
//!   player would notice the absence of.
//! * **The Builder's Path** — do you know what this engine can do? The engine
//!   has dialogue trees, DG scripts, quests, factions, transports, recipes,
//!   forage tables, traps, seasonal descriptions and contextual verbs, and the
//!   shipped world uses almost none of them. **A builder cannot learn a system
//!   they do not know exists**, so the track is a tutorial disguised as a
//!   checklist, and completing it produces content as a side effect.
//!
//! Tracks are JSON (`scripts/data/build_tracks/*.json`) so adding one is
//! content. Adding a *predicate kind* is one match arm, which is the line
//! between the two.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::audit::{AuditReport, letter_at_least};
use crate::types::ContentKind;

/// What a single step asks for.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Predicate {
    /// At least `min` entities of a kind.
    Count { of: ContentKind, min: usize },
    /// At least `ratio` (0..1) of entities of a kind grade `min_letter` or
    /// better. Vacuously false when there are none of that kind — "no rooms"
    /// is not "all rooms are good".
    GradeRatio {
        of: ContentKind,
        min_letter: char,
        ratio: f32,
    },
    /// No entity in scope emits this audit finding code.
    NoFinding { code: String },
    /// A named engine system is in use. The one predicate whose vocabulary
    /// lives in Rust, because each key reads a different field.
    HasSystem { system: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackStep {
    pub key: String,
    /// What the builder reads.
    pub label: String,
    pub predicate: Predicate,
    /// Optional one-liner on how to do it. This is the tutorial half.
    #[serde(default)]
    pub hint: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrackScope {
    /// Evaluated against one area.
    Area,
    /// Evaluated against everything one builder has authored.
    Builder,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackDef {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub scope: TrackScope,
    pub steps: Vec<TrackStep>,
}

/// Everything a predicate can ask about, gathered once.
///
/// Built from an area report or from a builder's own content, which is what
/// lets one predicate set serve both scopes.
#[derive(Debug, Default, Clone)]
pub struct TrackFacts {
    pub counts: HashMap<ContentKind, usize>,
    /// Every graded entity's letter, per kind.
    ///
    /// The letters themselves rather than a precomputed ratio, because the
    /// floor a step asks for is written in its JSON and the facts are gathered
    /// before any step is read. An earlier version stored one ratio computed at
    /// `C`, which meant `min_letter` was silently ignored — a step written
    /// `"min_letter": "B"` quietly evaluated at C.
    pub grade_letters: HashMap<ContentKind, Vec<char>>,
    /// Every audit finding code present anywhere in scope.
    pub findings: HashSet<String>,
    /// Engine systems in use. See [`systems_used`].
    pub systems: HashSet<String>,
}

impl TrackFacts {
    fn count(&self, kind: ContentKind) -> usize {
        self.counts.get(&kind).copied().unwrap_or(0)
    }

    /// Share of graded entities of `kind` at `floor` or better, or `None` when
    /// there are none to judge.
    pub fn ratio_at_least(&self, kind: ContentKind, floor: char) -> Option<f32> {
        let letters = self.grade_letters.get(&kind)?;
        if letters.is_empty() {
            return None;
        }
        let good = letters.iter().filter(|l| letter_at_least(**l, floor)).count();
        Some(good as f32 / letters.len() as f32)
    }

    pub fn satisfies(&self, p: &Predicate) -> bool {
        match p {
            Predicate::Count { of, min } => self.count(*of) >= *min,
            Predicate::GradeRatio { of, min_letter, ratio } => {
                // Empty is false, not vacuously true: "80% of rooms grade C+"
                // is not satisfied by having no rooms.
                if self.count(*of) == 0 {
                    return false;
                }
                self.ratio_at_least(*of, *min_letter).is_some_and(|r| r >= *ratio)
            }
            Predicate::NoFinding { code } => !self.findings.contains(code),
            Predicate::HasSystem { system } => self.systems.contains(system),
        }
    }

    /// Build from a finished area report.
    pub fn from_report(report: &AuditReport) -> TrackFacts {
        let mut facts = TrackFacts::default();
        for kind in ContentKind::ALL {
            let n = report.count_of(*kind);
            if n > 0 {
                facts.counts.insert(*kind, n);
            }
            let letters = report.letters_of(*kind);
            if !letters.is_empty() {
                facts.grade_letters.insert(*kind, letters);
            }
        }
        for (_, f) in report.all_findings() {
            facts.findings.insert(f.code.to_string());
        }
        facts
    }
}

/// Every engine system a set of entities puts to use.
///
/// The vocabulary of `HasSystem`. Each key is a different field on a different
/// type, which is why this is code rather than data — and why the list doubles
/// as an inventory of what the engine can do that a world usually does not.
pub fn systems_used(
    rooms: &[&crate::types::RoomData],
    items: &[&crate::types::ItemData],
    mobiles: &[&crate::types::MobileData],
    quests: &[&crate::types::QuestData],
    areas: &[&crate::types::AreaData],
) -> HashSet<String> {
    use crate::types::ItemType;
    let mut s: HashSet<String> = HashSet::new();
    let mut add = |k: &str| {
        s.insert(k.to_string());
    };

    for r in rooms {
        if !r.extra_descs.is_empty() {
            add("extra_desc");
        }
        if r.spring_desc.is_some() || r.summer_desc.is_some() || r.autumn_desc.is_some() || r.winter_desc.is_some() {
            add("seasonal_desc");
        }
        if !r.traps.is_empty() {
            add("trap");
        }
        if !r.contextual_commands.is_empty() {
            add("contextual_verb");
        }
        if !r.triggers.is_empty() {
            add("room_trigger");
        }
        if !r.doors.is_empty() {
            add("door");
        }
        if r.entry_gate.is_some() {
            add("entry_gate");
        }
        if !r.exit_delays.is_empty() {
            add("slow_exit");
        }
        if !r.catch_table.is_empty() {
            add("fishing");
        }
    }

    for i in items {
        if !i.affects.is_empty() {
            add("item_affect");
        }
        if !i.triggers.is_empty() {
            add("item_trigger");
        }
        if !i.extra_descs.is_empty() {
            add("extra_desc");
        }
        match i.item_type {
            ItemType::Container => add("container"),
            ItemType::LiquidContainer => add("liquid_container"),
            ItemType::Board => add("board"),
            ItemType::Note => add("note"),
            ItemType::Weapon => add("weapon"),
            ItemType::Armor => add("armor"),
            _ => {}
        }
    }

    for m in mobiles {
        if !m.dialogue.is_empty() {
            add("dialogue");
        }
        if m.dialogue_tree.is_some() {
            add("dialogue_tree");
        }
        if !m.daily_routine.is_empty() {
            add("routine");
        }
        if m.simulation.is_some() {
            add("simulation");
        }
        if !m.triggers.is_empty() {
            add("mobile_trigger");
        }
        if m.flags.shopkeeper && (!m.shop_stock.is_empty() || !m.shop_preset_vnum.is_empty()) {
            add("shop");
        }
        if m.flags.healer {
            add("healer");
        }
        if !m.combat_spells.is_empty() {
            add("combat_spells");
        }
        if m.faction.is_some() {
            add("faction");
        }
        if m.spoken_language.is_some() {
            add("language");
        }
        if m.alignment != 0 {
            add("alignment");
        }
    }

    for q in quests {
        if !q.objectives.is_empty() {
            add("quest");
        }
        if q.objectives.len() >= 2 {
            add("multi_step_quest");
        }
        if q.prereq_quest_vnum.is_some() {
            add("quest_chain");
        }
    }

    for a in areas {
        if !a.city_forage_table.is_empty()
            || !a.wilderness_forage_table.is_empty()
            || !a.shallow_water_forage_table.is_empty()
        {
            add("forage");
        }
        if a.immigration_enabled {
            add("immigration");
        }
        if a.level_min != 0 || a.level_max != 0 {
            add("level_range");
        }
    }

    s
}

/// Facts for one area: its audit report plus the systems its content uses.
pub fn area_facts(
    snapshot: &crate::audit::scan::WorldSnapshot,
    area: &crate::types::AreaData,
    ctx: &crate::audit::AuditCtx,
) -> TrackFacts {
    let report = crate::audit::scan::scan_area(snapshot, area, ctx);
    let mut facts = TrackFacts::from_report(&report);
    // The area itself counts as one, so `count of area min 1` means "you have
    // one" rather than never being satisfiable inside an area scope.
    facts.counts.insert(ContentKind::Area, 1);

    let contents = snapshot.area_contents(area.id);
    facts.systems = systems_used(
        &contents.rooms.iter().collect::<Vec<_>>(),
        &contents.items.iter().collect::<Vec<_>>(),
        &contents.mobiles.iter().collect::<Vec<_>>(),
        &contents.quests.iter().collect::<Vec<_>>(),
        &[area],
    );
    facts
}

/// Facts for one builder: everything they authored, wherever it is.
///
/// Deliberately not scoped to an area. The Builder's Path asks whether *you*
/// have used a system, and using it once anywhere is the answer.
pub fn builder_facts(
    snapshot: &crate::audit::scan::WorldSnapshot,
    ctx: &crate::audit::AuditCtx,
    builder: &str,
) -> TrackFacts {
    use crate::types::Authored;

    let mine = |a: Option<&str>, o: crate::types::ContentOrigin| {
        o.counts_for_score() && a.is_some_and(|n| n.eq_ignore_ascii_case(builder))
    };

    let rooms: Vec<&crate::types::RoomData> = snapshot
        .rooms
        .iter()
        .filter(|r| mine(r.authored_by(), r.origin))
        .collect();
    let items: Vec<&crate::types::ItemData> = snapshot
        .items
        .iter()
        .filter(|i| mine(i.authored_by(), i.origin))
        .collect();
    let mobiles: Vec<&crate::types::MobileData> = snapshot
        .mobiles
        .iter()
        .filter(|m| mine(m.authored_by(), m.origin))
        .collect();
    let quests: Vec<&crate::types::QuestData> = snapshot
        .quests
        .iter()
        .filter(|q| mine(q.authored_by(), q.origin))
        .collect();
    let areas: Vec<&crate::types::AreaData> = snapshot
        .areas
        .iter()
        .filter(|a| mine(a.authored_by(), a.origin))
        .collect();

    let mut facts = TrackFacts {
        systems: systems_used(&rooms, &items, &mobiles, &quests, &areas),
        ..Default::default()
    };
    facts.counts.insert(ContentKind::Room, rooms.len());
    facts.counts.insert(ContentKind::Item, items.len());
    facts.counts.insert(ContentKind::Mobile, mobiles.len());
    facts.counts.insert(ContentKind::Quest, quests.len());
    facts.counts.insert(ContentKind::Area, areas.len());

    // Grades and findings, over the builder's own content only — so a step
    // about quality asks about *your* work, not the world's.
    let tally = |kind: ContentKind, g: crate::audit::Grade, facts: &mut TrackFacts| {
        facts.grade_letters.entry(kind).or_default().push(g.letter);
        for f in &g.findings {
            facts.findings.insert(f.code.to_string());
        }
    };
    for r in &rooms {
        tally(ContentKind::Room, crate::audit::audit_room(r, ctx), &mut facts);
    }
    for i in &items {
        tally(ContentKind::Item, crate::audit::audit_item(i), &mut facts);
    }
    for m in &mobiles {
        tally(ContentKind::Mobile, crate::audit::audit_mobile(m), &mut facts);
    }
    for q in &quests {
        tally(ContentKind::Quest, crate::audit::audit_quest(q), &mut facts);
    }
    facts
}

/// One step, evaluated.
#[derive(Debug, Clone)]
pub struct StepProgress {
    pub key: String,
    pub label: String,
    pub hint: String,
    pub done: bool,
}

/// One track, evaluated.
#[derive(Debug, Clone)]
pub struct TrackProgress {
    pub key: String,
    pub name: String,
    pub description: String,
    pub scope: TrackScope,
    pub steps: Vec<StepProgress>,
}

impl TrackProgress {
    pub fn done(&self) -> usize {
        self.steps.iter().filter(|s| s.done).count()
    }

    pub fn total(&self) -> usize {
        self.steps.len()
    }

    pub fn complete(&self) -> bool {
        self.total() > 0 && self.done() == self.total()
    }

    /// The first unfinished step. What `build next` shows: one thing, not a
    /// wall — a list of everything left is the blank page again.
    pub fn next_step(&self) -> Option<&StepProgress> {
        self.steps.iter().find(|s| !s.done)
    }
}

pub fn evaluate(def: &TrackDef, facts: &TrackFacts) -> TrackProgress {
    TrackProgress {
        key: def.key.clone(),
        name: def.name.clone(),
        description: def.description.clone(),
        scope: def.scope,
        steps: def
            .steps
            .iter()
            .map(|s| StepProgress {
                key: s.key.clone(),
                label: s.label.clone(),
                hint: s.hint.clone(),
                done: facts.satisfies(&s.predicate),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 20 rooms: 17 at B, 3 at F. 85% reach C, 85% reach B, 0% reach A.
    fn facts() -> TrackFacts {
        let mut letters = vec!['B'; 17];
        letters.extend(['F'; 3]);
        let mut f = TrackFacts::default();
        f.counts.insert(ContentKind::Room, 20);
        f.grade_letters.insert(ContentKind::Room, letters);
        f.findings.insert("room.no_exits".to_string());
        f.systems.insert("dialogue_tree".to_string());
        f
    }

    #[test]
    fn count_predicates_compare_against_the_scope() {
        assert!(facts().satisfies(&Predicate::Count {
            of: ContentKind::Room,
            min: 20
        }));
        assert!(!facts().satisfies(&Predicate::Count {
            of: ContentKind::Room,
            min: 21
        }));
        assert!(!facts().satisfies(&Predicate::Count {
            of: ContentKind::Quest,
            min: 1
        }));
    }

    #[test]
    fn a_grade_ratio_over_nothing_is_false_not_vacuously_true() {
        // "No rooms" must not read as "all rooms are good", or an empty area
        // completes the readiness track.
        let f = TrackFacts::default();
        assert!(!f.satisfies(&Predicate::GradeRatio {
            of: ContentKind::Room,
            min_letter: 'C',
            ratio: 0.8,
        }));
    }

    #[test]
    fn grade_ratios_compare_as_written() {
        let f = facts();
        assert!(f.satisfies(&Predicate::GradeRatio {
            of: ContentKind::Room,
            min_letter: 'C',
            ratio: 0.8
        }));
        assert!(!f.satisfies(&Predicate::GradeRatio {
            of: ContentKind::Room,
            min_letter: 'C',
            ratio: 0.9
        }));
    }

    #[test]
    fn min_letter_is_the_floor_the_step_asked_for() {
        // It used to be ignored: every ratio was computed at C and the
        // "malformed letter" guard was `letter_at_least('A', min_letter)`,
        // which is unconditionally true. A step written "B" evaluated at C, and
        // a step written "A" passed on rooms that were nowhere near it.
        let f = facts();
        assert!(
            f.satisfies(&Predicate::GradeRatio {
                of: ContentKind::Room,
                min_letter: 'B',
                ratio: 0.8
            }),
            "17 of 20 rooms grade B"
        );
        assert!(
            !f.satisfies(&Predicate::GradeRatio {
                of: ContentKind::Room,
                min_letter: 'A',
                ratio: 0.8
            }),
            "none of them grade A, so an A-floor step must not be satisfied"
        );
    }

    #[test]
    fn no_finding_is_satisfied_by_absence() {
        let f = facts();
        assert!(!f.satisfies(&Predicate::NoFinding {
            code: "room.no_exits".into()
        }));
        assert!(f.satisfies(&Predicate::NoFinding {
            code: "room.no_desc".into()
        }));
    }

    #[test]
    fn has_system_reads_the_gathered_set() {
        let f = facts();
        assert!(f.satisfies(&Predicate::HasSystem {
            system: "dialogue_tree".into()
        }));
        assert!(!f.satisfies(&Predicate::HasSystem {
            system: "transport".into()
        }));
    }

    #[test]
    fn next_step_is_the_first_unfinished_one() {
        let p = TrackProgress {
            key: "t".into(),
            name: "T".into(),
            description: String::new(),
            scope: TrackScope::Area,
            steps: vec![
                StepProgress {
                    key: "a".into(),
                    label: "A".into(),
                    hint: String::new(),
                    done: true,
                },
                StepProgress {
                    key: "b".into(),
                    label: "B".into(),
                    hint: String::new(),
                    done: false,
                },
                StepProgress {
                    key: "c".into(),
                    label: "C".into(),
                    hint: String::new(),
                    done: false,
                },
            ],
        };
        assert_eq!(p.next_step().map(|s| s.key.as_str()), Some("b"));
        assert_eq!(p.done(), 1);
        assert!(!p.complete());
    }
}

//! Rhai bindings for the content auditor.
//!
//! Same split as `src/script/leaderboard.rs`: the judgement lives in Rust
//! (`crate::audit`), the words live in the script. Findings come back as plain
//! Rhai maps because they are inert data — a table of strings and numbers —
//! and a script that wants to lay them out differently should not have to go
//! through an API to do it.
//!
//! Unlike the leaderboard bindings these do hit the database, because there is
//! no cache yet: `build audit world` reads the room, item, mobile, quest and
//! spawn trees. That is acceptable here and only here — it is a builder
//! command, run deliberately, by someone who has just asked for a full sweep.
//! Nothing on a player path may call these.

use rhai::{Array, Dynamic, Engine, Map};
use std::sync::Arc;

use crate::SharedState;
use crate::attribution::{self, ContentRef};
use crate::audit::scan::{self, WorldSnapshot};
use crate::audit::{self, AuditReport, EntityKind, Finding, Grade};
use crate::build_score::{BuildScores, BuilderScore};
use crate::db::Db;
use crate::types::{Authored, ContentKind, Provenance};

/// How many of the worst entries an area or world report carries back. A large
/// area has hundreds of rooms and a script cannot show them all; the report
/// still counts every one, it just names the ones worth opening first.
const WORST_LIMIT: usize = 15;

fn finding_map(f: &Finding) -> Map {
    let mut m = Map::new();
    m.insert("code".into(), Dynamic::from(f.code.to_string()));
    m.insert("severity".into(), Dynamic::from(f.severity.key().to_string()));
    m.insert("label".into(), Dynamic::from(f.severity.label().to_string()));
    m.insert("message".into(), Dynamic::from(f.message.clone()));
    m
}

fn findings_array(findings: &[Finding]) -> Array {
    findings.iter().map(|f| Dynamic::from(finding_map(f))).collect()
}

/// The fields every graded thing carries, so scripts can render an entity, an
/// area and the world with one helper.
fn insert_grade(m: &mut Map, grade: &Grade) {
    m.insert("score".into(), Dynamic::from(grade.score as i64));
    m.insert("letter".into(), Dynamic::from(grade.letter.to_string()));
    m.insert(
        "blockers".into(),
        Dynamic::from(grade.count(audit::Severity::Blocker) as i64),
    );
    m.insert("warns".into(), Dynamic::from(grade.count(audit::Severity::Warn) as i64));
    m.insert(
        "polish".into(),
        Dynamic::from(grade.count(audit::Severity::Polish) as i64),
    );
    m.insert("clean".into(), Dynamic::from(grade.is_clean()));
    m.insert("findings".into(), Dynamic::from(findings_array(&grade.findings)));
}

/// The attribution fields, on any map that carries a grade.
fn insert_credit(m: &mut Map, p: &Provenance) {
    m.insert(
        "authored_by".into(),
        Dynamic::from(p.authored_by.clone().unwrap_or_default()),
    );
    m.insert(
        "last_edited_by".into(),
        Dynamic::from(p.last_edited_by.clone().unwrap_or_default()),
    );
    m.insert("origin".into(), Dynamic::from(p.origin.key().to_string()));
    m.insert("origin_label".into(), Dynamic::from(p.origin.label().to_string()));
    m.insert("counts".into(), Dynamic::from(p.origin.counts_for_score()));
}

fn not_found(key: &str) -> Map {
    let mut m = Map::new();
    m.insert("found".into(), Dynamic::from(false));
    m.insert("key".into(), Dynamic::from(key.to_string()));
    m.insert("label".into(), Dynamic::from(String::new()));
    m.insert("name".into(), Dynamic::from(String::new()));
    insert_grade(&mut m, &Grade::from_findings(Vec::new()));
    insert_credit(&mut m, &Provenance::default());
    m.insert("entries".into(), Dynamic::from(Array::new()));
    m
}

/// The worst entries of a report, already graded, as an array of maps.
fn worst_array(report: &AuditReport) -> Array {
    report
        .worst(None, WORST_LIMIT)
        .into_iter()
        .map(|e| {
            let mut m = Map::new();
            m.insert("kind".into(), Dynamic::from(e.kind.key().to_string()));
            m.insert("label".into(), Dynamic::from(e.label.clone()));
            m.insert("name".into(), Dynamic::from(e.name.clone()));
            insert_grade(&mut m, &e.grade);
            Dynamic::from(m)
        })
        .collect()
}

/// Per-kind tallies so a report can say "42 rooms, 31 of them C or better"
/// without shipping 42 rows.
fn counts_map(report: &AuditReport) -> Map {
    let mut m = Map::new();
    for kind in [
        EntityKind::Room,
        EntityKind::Item,
        EntityKind::Mobile,
        EntityKind::Quest,
        EntityKind::Area,
    ] {
        let total = report.count_of(kind);
        if total == 0 {
            continue;
        }
        let mut row = Map::new();
        row.insert("total".into(), Dynamic::from(total as i64));
        let ratio = report.ratio_at_least(kind, 'C').unwrap_or(0.0);
        row.insert("good".into(), Dynamic::from((ratio * total as f32).round() as i64));
        row.insert("ratio".into(), Dynamic::from((ratio * 100.0).round() as i64));
        m.insert(kind.key().into(), Dynamic::from(row));
    }
    m
}

fn report_map(report: &AuditReport, key: &str) -> Map {
    let mut m = Map::new();
    m.insert("found".into(), Dynamic::from(true));
    m.insert("key".into(), Dynamic::from(key.to_string()));
    m.insert("label".into(), Dynamic::from(report.label.clone()));
    m.insert("name".into(), Dynamic::from(report.label.clone()));
    // The container's composite, not its own findings' score — that is what
    // "how good is this area" means.
    m.insert("score".into(), Dynamic::from(report.score as i64));
    m.insert("letter".into(), Dynamic::from(report.letter.to_string()));
    let (b, w, p) = report.severity_counts();
    m.insert("blockers".into(), Dynamic::from(b as i64));
    m.insert("warns".into(), Dynamic::from(w as i64));
    m.insert("polish".into(), Dynamic::from(p as i64));
    m.insert("clean".into(), Dynamic::from(b == 0 && w == 0));
    // `findings` is the container's OWN findings. Children's findings are
    // summarised by `counts` and named by `worst`.
    m.insert("findings".into(), Dynamic::from(findings_array(&report.own.findings)));
    m.insert("counts".into(), Dynamic::from(counts_map(report)));
    m.insert("worst".into(), Dynamic::from(worst_array(report)));
    m.insert(
        "truncated".into(),
        Dynamic::from(report.worst(None, usize::MAX).len() > WORST_LIMIT),
    );
    m
}

/// One builder's sheet as a script sees it.
fn score_map(score: &BuilderScore, scores: &BuildScores) -> Map {
    let mut m = Map::new();
    m.insert("found".into(), Dynamic::from(true));
    m.insert("name".into(), Dynamic::from(score.name.clone()));
    m.insert("total".into(), Dynamic::from(score.total as i64));
    m.insert("content_points".into(), Dynamic::from(score.content_points as i64));
    m.insert("bounty_points".into(), Dynamic::from(score.bounty_points as i64));
    m.insert("entities".into(), Dynamic::from(score.entities() as i64));
    m.insert("good".into(), Dynamic::from(score.good() as i64));
    m.insert("excellent".into(), Dynamic::from(score.excellent() as i64));
    m.insert("broken".into(), Dynamic::from(score.broken() as i64));
    m.insert(
        "placing".into(),
        Dynamic::from(scores.placing(&score.name).unwrap_or(0) as i64),
    );
    m.insert("builders".into(), Dynamic::from(scores.builders.len() as i64));

    // One row per kind, in the weight order the score itself uses, so the
    // sheet reads top-down as "what is worth most".
    let rows: Array = [
        ContentKind::Area,
        ContentKind::Quest,
        ContentKind::Mobile,
        ContentKind::Item,
        ContentKind::Room,
    ]
    .into_iter()
    .filter_map(|kind| {
        let t = score.tally(kind);
        if t.count == 0 {
            return None;
        }
        let mut row = Map::new();
        row.insert("kind".into(), Dynamic::from(kind.key().to_string()));
        row.insert("count".into(), Dynamic::from(t.count as i64));
        row.insert("points".into(), Dynamic::from(t.points as i64));
        row.insert("good".into(), Dynamic::from(t.good as i64));
        row.insert("excellent".into(), Dynamic::from(t.excellent as i64));
        row.insert("broken".into(), Dynamic::from(t.broken as i64));
        Some(Dynamic::from(row))
    })
    .collect();
    m.insert("tallies".into(), Dynamic::from(rows));
    m
}

pub fn register(engine: &mut Engine, db: Arc<Db>, connections: crate::SharedConnections, state: SharedState) {
    // get_build_score(name) -> Map
    //
    // A builder's sheet from the cached scan. `ready` is false until the first
    // scan lands — readers must show "not scanned yet" rather than a zero,
    // because those are different facts.
    let cloned_state = state.clone();
    engine.register_fn("get_build_score", move |name: String| -> Map {
        let mut m = Map::new();
        let Ok(world) = cloned_state.lock() else {
            m.insert("found".into(), Dynamic::from(false));
            m.insert("ready".into(), Dynamic::from(false));
            return m;
        };
        let scores = &world.build_scores;
        let mut out = match scores.get(&name) {
            Some(s) => score_map(s, scores),
            None => {
                let empty = BuilderScore {
                    name: name.clone(),
                    ..Default::default()
                };
                let mut m = score_map(&empty, scores);
                m.insert("found".into(), Dynamic::from(false));
                m
            }
        };
        out.insert("ready".into(), Dynamic::from(scores.is_ready()));
        out.insert("generated_at".into(), Dynamic::from(scores.generated_at));
        out.insert("credited".into(), Dynamic::from(scores.credited_entities as i64));
        out.insert("uncredited".into(), Dynamic::from(scores.uncredited_entities as i64));
        out
    });

    // get_build_score_index() -> Array of #{rank, name, total, entities}
    //
    // Every builder, best first. Small by construction — this ranks the people
    // who build the world, not the people who play in it.
    let cloned_state = state.clone();
    engine.register_fn("get_build_score_index", move || -> Array {
        let Ok(world) = cloned_state.lock() else {
            return Array::new();
        };
        world
            .build_scores
            .ranked()
            .into_iter()
            .enumerate()
            .map(|(i, b)| {
                let mut row = Map::new();
                row.insert("rank".into(), Dynamic::from(i as i64 + 1));
                row.insert("name".into(), Dynamic::from(b.name.clone()));
                row.insert("total".into(), Dynamic::from(b.total as i64));
                row.insert("entities".into(), Dynamic::from(b.entities() as i64));
                row.insert("excellent".into(), Dynamic::from(b.excellent() as i64));
                row.insert("broken".into(), Dynamic::from(b.broken() as i64));
                Dynamic::from(row)
            })
            .collect()
    });

    // audit_entity(kind, key) -> Map
    //
    // kind is room|item|mobile|quest|area; key is a vnum, a uuid, or (for an
    // area) its prefix or name. #{found, label, name, score, letter, blockers,
    // warns, polish, clean, findings: [#{code, severity, label, message}]}.
    let cloned_db = db.clone();
    engine.register_fn("audit_entity", move |kind: String, key: String| -> Map {
        let Some(k) = EntityKind::from_key(&kind) else {
            return not_found(&key);
        };
        match scan::scan_entity(&cloned_db, k, &key) {
            Ok(Some(e)) => {
                let mut m = Map::new();
                m.insert("found".into(), Dynamic::from(true));
                m.insert("key".into(), Dynamic::from(key));
                m.insert("kind".into(), Dynamic::from(k.key().to_string()));
                m.insert("label".into(), Dynamic::from(e.label));
                m.insert("name".into(), Dynamic::from(e.name));
                insert_grade(&mut m, &e.grade);
                m.insert("entries".into(), Dynamic::from(Array::new()));
                // Who is responsible for it, alongside how good it is — the
                // first thing asked after "is this any good" is "whose is it".
                insert_credit(&mut m, &e.provenance);
                m
            }
            _ => not_found(&key),
        }
    });

    // audit_area_report(key) -> Map
    //
    // The area's own findings plus a rollup of everything in it: #{..., counts:
    // #{room: #{total, good, ratio}, ...}, worst: [entry], truncated}.
    let cloned_db = db.clone();
    engine.register_fn("audit_area_report", move |key: String| -> Map {
        let Ok(snapshot) = WorldSnapshot::load(&cloned_db) else {
            return not_found(&key);
        };
        let needle = key.trim().to_lowercase();
        let found = snapshot.areas.iter().find(|a| {
            a.id.to_string() == needle || a.prefix.to_lowercase() == needle || a.name.to_lowercase() == needle
        });
        let Some(area) = found else {
            return not_found(&key);
        };
        let ctx = snapshot.ctx();
        report_map(&scan::scan_area(&snapshot, area, ctx), &key)
    });

    // audit_world_report() -> Map
    //
    // World-level findings plus one entry per area. Expensive by construction
    // — it reads five trees. Builder command only.
    let cloned_db = db.clone();
    engine.register_fn("audit_world_report", move || -> Map {
        let Ok(snapshot) = WorldSnapshot::load(&cloned_db) else {
            return not_found("world");
        };
        let facts = snapshot.facts();
        let report = scan::scan_world(&snapshot);
        let mut m = report_map(&report, "world");

        let mut f = Map::new();
        f.insert("areas".into(), Dynamic::from(facts.area_count as i64));
        f.insert("rooms".into(), Dynamic::from(facts.room_count as i64));
        f.insert("items".into(), Dynamic::from(facts.item_count as i64));
        f.insert("mobiles".into(), Dynamic::from(facts.mobile_count as i64));
        f.insert("quests".into(), Dynamic::from(facts.quest_count as i64));
        f.insert("spawn_points".into(), Dynamic::from(facts.spawn_point_count as i64));
        f.insert("recipes".into(), Dynamic::from(facts.recipe_count as i64));
        f.insert("transports".into(), Dynamic::from(facts.transport_count as i64));
        f.insert("dialogue_trees".into(), Dynamic::from(facts.dialogue_trees as i64));
        f.insert("boards".into(), Dynamic::from(facts.board_items as i64));
        f.insert(
            "unfiled_rooms".into(),
            Dynamic::from(snapshot.orphan_room_count() as i64),
        );
        f.insert("unfiled_mobiles".into(), Dynamic::from(facts.unfiled_mobiles as i64));
        f.insert("unfiled_items".into(), Dynamic::from(facts.unfiled_items as i64));
        m.insert("facts".into(), Dynamic::from(f));

        let areas: Array = snapshot
            .areas
            .iter()
            .map(|a| {
                let r = scan::scan_area(&snapshot, a, snapshot.ctx());
                let mut row = Map::new();
                row.insert("label".into(), Dynamic::from(a.prefix.clone()));
                row.insert("name".into(), Dynamic::from(a.name.clone()));
                row.insert("score".into(), Dynamic::from(r.score as i64));
                row.insert("letter".into(), Dynamic::from(r.letter.to_string()));
                let (b, w, p) = r.severity_counts();
                row.insert("blockers".into(), Dynamic::from(b as i64));
                row.insert("warns".into(), Dynamic::from(w as i64));
                row.insert("polish".into(), Dynamic::from(p as i64));
                row.insert("rooms".into(), Dynamic::from(r.count_of(EntityKind::Room) as i64));
                Dynamic::from(row)
            })
            .collect();
        m.insert("areas".into(), Dynamic::from(areas));
        m
    });

    // stamp_content_created(kind, id, builder_name) -> bool
    //
    // A builder just created this: claim it and mark it Builder origin. Call
    // it from an OLC editor's create branch, after the entity has been saved
    // and its vnum/area stamped.
    //
    // `id` is a uuid for room/item/mobile/area and a vnum for quest. Returns
    // false when the entity does not exist or the name is blank, and never
    // raises — a failed stamp must not abort an edit the builder already
    // completed.
    let cloned_db = db.clone();
    engine.register_fn(
        "stamp_content_created",
        move |kind: String, id: String, builder: String| -> bool {
            let Some(target) = ContentRef::parse(&kind, &id) else {
                return false;
            };
            attribution::stamp_created(&cloned_db, &target, &builder).unwrap_or(false)
        },
    );

    // stamp_content_edited(kind, id, builder_name) -> bool
    //
    // Records `last_edited_by` and nothing else. Call it once per editor, at
    // the point the permission gate has passed and a mutating subcommand is
    // about to run — not per setter, of which `src/script/rooms.rs` alone has
    // thirty-five.
    let cloned_db = db.clone();
    engine.register_fn(
        "stamp_content_edited",
        move |kind: String, id: String, builder: String| -> bool {
            let Some(target) = ContentRef::parse(&kind, &id) else {
                return false;
            };
            attribution::stamp_edited(&cloned_db, &target, &builder).unwrap_or(false)
        },
    );

    // note_grade_before(kind, id, connection_id) -> bool
    //
    // Snapshot this entity's grade onto the session, so that when the command
    // finishes the engine can say what moved. Call it beside the stamp, at the
    // point a mutating subcommand is about to run.
    //
    // The drain lives in `handle_connection` (src/lib.rs) rather than in the
    // editor scripts, because that is the one place every editor's every early
    // return passes through — wiring it per-branch would miss some, and the
    // ones it missed would be silent.
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    let cloned_state = state.clone();
    engine.register_fn(
        "note_grade_before",
        move |kind: String, id: String, connection_id: String| -> bool {
            let Some(k) = EntityKind::from_key(&kind) else {
                return false;
            };
            let Ok(conn) = uuid::Uuid::parse_str(&connection_id) else {
                return false;
            };
            // Cached context, taken and released before anything else — never
            // held across the connections lock below.
            let ctx = {
                let Ok(world) = cloned_state.lock() else {
                    return false;
                };
                world.audit_ctx.clone()
            };
            let pending = scan::grade_snapshot(&cloned_db, k, &id, &ctx);
            let Ok(mut guard) = cloned_conns.lock() else {
                return false;
            };
            match guard.get_mut(&conn) {
                Some(session) => {
                    session.pending_audit = Some(pending);
                    true
                }
                None => false,
            }
        },
    );

    // get_content_credit(kind, id) -> Map
    //
    // #{found, authored_by, last_edited_by, origin, origin_label, counts}.
    // `counts` is whether this content can be credited to a builder at all,
    // which is the one question every scoring path asks.
    let cloned_db = db.clone();
    engine.register_fn("get_content_credit", move |kind: String, id: String| -> Map {
        let mut m = Map::new();
        let Some(target) = ContentRef::parse(&kind, &id) else {
            m.insert("found".into(), Dynamic::from(false));
            return m;
        };
        match attribution::read(&cloned_db, &target) {
            Ok(Some(p)) => {
                m.insert("found".into(), Dynamic::from(true));
                m.insert("authored_by".into(), Dynamic::from(p.authored_by.unwrap_or_default()));
                m.insert(
                    "last_edited_by".into(),
                    Dynamic::from(p.last_edited_by.unwrap_or_default()),
                );
                m.insert("origin".into(), Dynamic::from(p.origin.key().to_string()));
                m.insert("origin_label".into(), Dynamic::from(p.origin.label().to_string()));
                m.insert("counts".into(), Dynamic::from(p.origin.counts_for_score()));
            }
            _ => {
                m.insert("found".into(), Dynamic::from(false));
            }
        }
        m
    });

    // get_area_credits(area_key) -> Map
    //
    // #{found, area, name, total, uncredited, authors: [#{name, rooms, items,
    // mobiles, quests, total}]}. `area_key` is a prefix, name or uuid; an empty
    // string means the whole world.
    //
    // The relatedness surface. `AreaData.owner` has always been an ACL and
    // nothing in the game has ever displayed who made anything — which is the
    // one need of the three that scoring alone cannot meet, because a number
    // beside your own name is not the same as your name beside your work.
    //
    // Counted from provenance, not from the score, so content that earns
    // nothing still shows its author. `uncredited` is everything with no author
    // at all: seed, import, and anything built before attribution existed.
    let cloned_db = db.clone();
    engine.register_fn("get_area_credits", move |area_key: String| -> Map {
        let mut m = Map::new();
        let Ok(snapshot) = WorldSnapshot::load(&cloned_db) else {
            m.insert("found".into(), Dynamic::from(false));
            return m;
        };

        let key = area_key.trim().to_lowercase();
        let area = if key.is_empty() {
            None
        } else {
            let found = snapshot
                .areas
                .iter()
                .find(|a| a.prefix.to_lowercase() == key || a.name.to_lowercase() == key || a.id.to_string() == key);
            match found {
                Some(a) => Some(a),
                None => {
                    m.insert("found".into(), Dynamic::from(false));
                    return m;
                }
            }
        };
        let scope = area.map(|a| a.id);
        let in_scope = |owner: Option<uuid::Uuid>| scope.is_none() || owner == scope;

        // (rooms, items, mobiles, quests) per author name.
        let mut tally: std::collections::BTreeMap<String, [usize; 4]> = std::collections::BTreeMap::new();
        let mut uncredited = 0usize;
        let mut credit = |author: Option<&str>,
                          slot: usize,
                          tally: &mut std::collections::BTreeMap<String, [usize; 4]>| {
            match author.filter(|a| !a.trim().is_empty()) {
                Some(a) => tally.entry(a.to_string()).or_insert([0; 4])[slot] += 1,
                None => uncredited += 1,
            }
        };

        for r in snapshot.rooms.iter().filter(|r| in_scope(r.area_id)) {
            credit(r.authored_by(), 0, &mut tally);
        }
        for i in snapshot.items.iter().filter(|i| in_scope(i.area_id)) {
            credit(i.authored_by(), 1, &mut tally);
        }
        for mob in snapshot.mobiles.iter().filter(|mo| in_scope(mo.area_id)) {
            credit(mob.authored_by(), 2, &mut tally);
        }
        // A quest has no `area_id`. It belongs to whichever area its giver
        // lives in, the same association `WorldSnapshot::quests_for_area`
        // makes; a quest with no giver belongs to the world and shows up only
        // in the unscoped listing.
        let givers: std::collections::HashSet<&str> = snapshot
            .mobiles
            .iter()
            .filter(|mo| in_scope(mo.area_id))
            .map(|mo| mo.vnum.as_str())
            .collect();
        for q in snapshot
            .quests
            .iter()
            .filter(|q| scope.is_none() || q.giver_mob_vnum.as_deref().is_some_and(|v| givers.contains(v)))
        {
            credit(q.authored_by(), 3, &mut tally);
        }

        let mut rows: Vec<(String, [usize; 4])> = tally.into_iter().collect();
        rows.sort_by(|a, b| {
            let ta: usize = a.1.iter().sum();
            let tb: usize = b.1.iter().sum();
            tb.cmp(&ta).then_with(|| a.0.cmp(&b.0))
        });

        let total: usize = rows.iter().map(|(_, c)| c.iter().sum::<usize>()).sum();
        let authors: Array = rows
            .iter()
            .map(|(name, c)| {
                let mut row = Map::new();
                row.insert("name".into(), Dynamic::from(name.clone()));
                row.insert("rooms".into(), Dynamic::from(c[0] as i64));
                row.insert("items".into(), Dynamic::from(c[1] as i64));
                row.insert("mobiles".into(), Dynamic::from(c[2] as i64));
                row.insert("quests".into(), Dynamic::from(c[3] as i64));
                row.insert("total".into(), Dynamic::from(c.iter().sum::<usize>() as i64));
                Dynamic::from(row)
            })
            .collect();

        m.insert("found".into(), Dynamic::from(true));
        m.insert(
            "area".into(),
            Dynamic::from(area.map(|a| a.prefix.clone()).unwrap_or_default()),
        );
        m.insert(
            "name".into(),
            Dynamic::from(area.map(|a| a.name.clone()).unwrap_or_else(|| "the world".to_string())),
        );
        m.insert("total".into(), Dynamic::from(total as i64));
        m.insert("uncredited".into(), Dynamic::from(uncredited as i64));
        m.insert("authors".into(), Dynamic::from(authors));
        m
    });

    // get_world_rating() -> Map
    //
    // The cached world rating: #{ready, score, tier, tier_label, description,
    // next_label, next_at, capped, cap_reason, next_step, components: [...],
    // facts: #{...}}. Read-only cache; the scan is a tick.
    let cloned_state = state.clone();
    engine.register_fn("get_world_rating", move || -> Map {
        let mut m = Map::new();
        let Ok(world) = cloned_state.lock() else {
            m.insert("ready".into(), Dynamic::from(false));
            return m;
        };
        let report = &world.world_report;
        m.insert("ready".into(), Dynamic::from(report.is_ready()));
        m.insert("generated_at".into(), Dynamic::from(report.generated_at));
        m.insert("quality".into(), Dynamic::from(report.quality_pct as i64));
        let Some(rating) = report.rating.as_ref() else {
            return m;
        };

        m.insert("score".into(), Dynamic::from(rating.score as i64));
        m.insert("uncapped".into(), Dynamic::from(rating.uncapped_score as i64));
        m.insert("tier".into(), Dynamic::from(rating.tier_key.to_string()));
        m.insert("tier_label".into(), Dynamic::from(rating.tier_label.to_string()));
        m.insert("description".into(), Dynamic::from(rating.tier_description.to_string()));
        m.insert(
            "next_label".into(),
            Dynamic::from(rating.next_label.unwrap_or_default().to_string()),
        );
        m.insert("next_at".into(), Dynamic::from(rating.next_at as i64));
        m.insert("capped".into(), Dynamic::from(rating.cap.is_some()));
        m.insert(
            "cap_reason".into(),
            Dynamic::from(rating.cap.map(|c| c.reason).unwrap_or_default().to_string()),
        );
        m.insert("next_step".into(), Dynamic::from(rating.next_step()));

        let components: Array = rating
            .components
            .iter()
            .map(|c| {
                let mut row = Map::new();
                row.insert("key".into(), Dynamic::from(c.key.to_string()));
                row.insert("label".into(), Dynamic::from(c.label.to_string()));
                row.insert("score".into(), Dynamic::from(c.score as i64));
                row.insert("weight".into(), Dynamic::from(c.weight as i64));
                row.insert("hint".into(), Dynamic::from(c.hint.to_string()));
                Dynamic::from(row)
            })
            .collect();
        m.insert("components".into(), Dynamic::from(components));

        let f = &report.facts;
        let mut facts = Map::new();
        facts.insert("areas".into(), Dynamic::from(f.area_count as i64));
        facts.insert("rooms".into(), Dynamic::from(f.room_count as i64));
        facts.insert("items".into(), Dynamic::from(f.item_count as i64));
        facts.insert("mobiles".into(), Dynamic::from(f.mobile_count as i64));
        facts.insert("quests".into(), Dynamic::from(f.quest_count as i64));
        facts.insert("spawn_points".into(), Dynamic::from(f.spawn_point_count as i64));
        facts.insert("recipes".into(), Dynamic::from(f.recipe_count as i64));
        facts.insert("transports".into(), Dynamic::from(f.transport_count as i64));
        facts.insert("dialogue_trees".into(), Dynamic::from(f.dialogue_trees as i64));
        facts.insert("boards".into(), Dynamic::from(f.board_items as i64));
        m.insert("facts".into(), Dynamic::from(facts));
        m
    });

    // get_world_milestones() -> Array of
    //   #{key, name, description, unlocked, unlocked_at, contributors, have, want}
    //
    // The wall: every milestone, met or not, with progress toward the ones
    // that are not. Showing the locked ones is the whole point — a wall of
    // things already done tells an operator nothing about what to do next.
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("get_world_milestones", move || -> Array {
        let (facts, rating) = {
            let Ok(world) = cloned_state.lock() else {
                return Array::new();
            };
            let Some(rating) = world.world_report.rating.clone() else {
                return Array::new();
            };
            (world.world_report.facts.clone(), rating)
        };
        let Ok(rows) = crate::world_milestones::wall(&cloned_db, &facts, &rating) else {
            return Array::new();
        };
        rows.into_iter()
            .map(|row| {
                let mut m = Map::new();
                let def = crate::script::achievements::describe(&cloned_state, &row.key);
                m.insert("key".into(), Dynamic::from(row.key.clone()));
                m.insert("name".into(), Dynamic::from(def.unwrap_or(row.key)));
                m.insert("unlocked".into(), Dynamic::from(row.unlocked_at.is_some()));
                m.insert("unlocked_at".into(), Dynamic::from(row.unlocked_at.unwrap_or(0)));
                let contributors: Array = row.contributors.into_iter().map(Dynamic::from).collect();
                m.insert("contributors".into(), Dynamic::from(contributors));
                m.insert("have".into(), Dynamic::from(row.have));
                m.insert("want".into(), Dynamic::from(row.want));
                m.insert("progress".into(), Dynamic::from(row.progress_text));
                Dynamic::from(m)
            })
            .collect()
    });

    // get_build_tracks(scope, key) -> Array of track maps
    //
    // scope is "area" (key = an area prefix/name/uuid) or "builder" (key = a
    // builder name). Each map is #{key, name, description, scope, done, total,
    // complete, steps: [#{key, label, hint, done}]}.
    //
    // Progress is evaluated on the call rather than cached: a track is a
    // question about the content that exists right now, and caching the answer
    // would just be a second thing to keep in sync. It reads the world, so it
    // is a builder command like the rest of this module.
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("get_build_tracks", move |scope: String, key: String| -> Array {
        let defs: Vec<crate::build_tracks::TrackDef> = match cloned_state.lock() {
            Ok(world) => world.build_tracks.clone(),
            Err(_) => return Array::new(),
        };
        let want = match scope.trim().to_lowercase().as_str() {
            "area" => crate::build_tracks::TrackScope::Area,
            _ => crate::build_tracks::TrackScope::Builder,
        };
        let Ok(snapshot) = WorldSnapshot::load(&cloned_db) else {
            return Array::new();
        };
        let ctx = snapshot.ctx();

        let facts = if want == crate::build_tracks::TrackScope::Area {
            let needle = key.trim().to_lowercase();
            let Some(area) = snapshot.areas.iter().find(|a| {
                a.id.to_string() == needle || a.prefix.to_lowercase() == needle || a.name.to_lowercase() == needle
            }) else {
                return Array::new();
            };
            crate::build_tracks::area_facts(&snapshot, area, ctx)
        } else {
            crate::build_tracks::builder_facts(&snapshot, ctx, &key)
        };

        defs.iter()
            .filter(|d| d.scope == want)
            .map(|d| {
                let p = crate::build_tracks::evaluate(d, &facts);
                let mut m = Map::new();
                m.insert("key".into(), Dynamic::from(p.key.clone()));
                m.insert("name".into(), Dynamic::from(p.name.clone()));
                m.insert("description".into(), Dynamic::from(p.description.clone()));
                m.insert("scope".into(), Dynamic::from(scope.clone()));
                m.insert("done".into(), Dynamic::from(p.done() as i64));
                m.insert("total".into(), Dynamic::from(p.total() as i64));
                m.insert("complete".into(), Dynamic::from(p.complete()));
                let next = p.next_step();
                m.insert(
                    "next_label".into(),
                    Dynamic::from(next.map(|s| s.label.clone()).unwrap_or_default()),
                );
                m.insert(
                    "next_hint".into(),
                    Dynamic::from(next.map(|s| s.hint.clone()).unwrap_or_default()),
                );
                let steps: Array = p
                    .steps
                    .iter()
                    .map(|s| {
                        let mut row = Map::new();
                        row.insert("key".into(), Dynamic::from(s.key.clone()));
                        row.insert("label".into(), Dynamic::from(s.label.clone()));
                        row.insert("hint".into(), Dynamic::from(s.hint.clone()));
                        row.insert("done".into(), Dynamic::from(s.done));
                        Dynamic::from(row)
                    })
                    .collect();
                m.insert("steps".into(), Dynamic::from(steps));
                Dynamic::from(m)
            })
            .collect()
    });

    // audit_grade_bar(score) -> String
    //
    // The ten-cell bar `build audit` prints. In Rust rather than the script so
    // every surface that shows a grade draws the same bar — the score/letter
    // relationship already lives in one table and the bar belongs with it.
    engine.register_fn("audit_grade_bar", move |score: i64| -> String {
        let filled = ((score.clamp(0, 100) as f32) / 10.0).round() as usize;
        format!("[{}{}]", "#".repeat(filled), "-".repeat(10 - filled))
    });

    // audit_letter_for(score) -> String
    engine.register_fn("audit_letter_for", move |score: i64| -> String {
        audit::letter_for(score.clamp(0, 100) as i32).to_string()
    });
}

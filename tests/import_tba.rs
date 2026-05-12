//! End-to-end test for the tbaMUD importer:
//! parse → map → apply → assert via Db queries.
//!
//! Validates the format-delta surfaces relative to CircleMUD:
//! - 10-token mob action line (decimal + ascii-letter encodings)
//! - 13-token obj type/flags line + 5-token weight/cost/rent line
//! - `T <vnum>` trigger attachments on rooms / mobs / objs
//! - `.trg` body parsing tolerates `~` characters inside DG Script bodies
//! - `.qst` records become warn-only

use std::path::PathBuf;

use ironmud::db::Db;
use ironmud::import::{MappingOptions, MudEngine, Severity, engines::tba::TbaEngine, mapping, writer};

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tba")
}

fn empty_opts() -> MappingOptions {
    MappingOptions {
        circle: mapping::CircleMappingTable::load_default(),
        existing_area_prefixes: Vec::new(),
        existing_room_vnums: Vec::new(),
        existing_mobile_vnums: Vec::new(),
        existing_item_vnums: Vec::new(),
    }
}

#[test]
fn parses_tba_fixture_cleanly() {
    let (ir, parse_warnings) = TbaEngine.parse(&fixture_root()).expect("parse");
    let parse_errors: Vec<_> = parse_warnings
        .iter()
        .filter(|w| matches!(w.kind, ironmud::import::WarningKind::Parse))
        .collect();
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    assert_eq!(ir.zones.len(), 1, "fixture has one zone");
    let zone = &ir.zones[0];
    assert_eq!(zone.vnum, 30);
    assert_eq!(zone.name, "Northern Midgaard");

    // 2 rooms, each parsed via the shared circle parser.
    assert_eq!(zone.rooms.len(), 2);
    let reading = zone.rooms.iter().find(|r| r.vnum == 3000).expect("reading room");
    assert_eq!(reading.trigger_vnums, vec![555], "T 555 attached to room 3000");

    // Mobs: 2 prototypes via the 10-token tba parser.
    assert_eq!(zone.mobiles.len(), 2);
    let wizard = zone.mobiles.iter().find(|m| m.vnum == 3000).expect("wizard");
    assert_eq!(wizard.alignment, 900);
    assert_eq!(wizard.format, 'E');
    assert_eq!(wizard.mob_flag_bits, 26635, "f1 decimal MOB flags");
    assert_eq!(wizard.aff_flag_bits, 16, "f5 decimal AFF flags");
    assert_eq!(wizard.trigger_vnums, vec![666], "T 666 attached to wizard");

    let guard = zone.mobiles.iter().find(|m| m.vnum == 3001).expect("guard");
    // Ascii-letter encoding: a|b|d|f = bits 0|1|3|5
    assert_eq!(guard.mob_flag_bits, 0b101011, "guard f1 ascii-letter MOB flags");
    assert_eq!(guard.aff_flag_bits, 0b1100, "guard f5 ascii-letter AFF flags");
    assert_eq!(guard.format, 'S');

    // Items: 2 prototypes via the 13-token tba parser.
    assert_eq!(zone.items.len(), 2);
    let teleporter = zone.items.iter().find(|i| i.vnum == 3000).expect("teleporter");
    assert_eq!(teleporter.weight, 1, "tba 5-token weight line");
    assert_eq!(teleporter.cost, 10);
    assert_eq!(teleporter.trigger_vnums, vec![777, 778], "T trailers on obj");

    let sword = zone.items.iter().find(|i| i.vnum == 3001).expect("sword");
    assert_eq!(sword.item_type, 5, "WEAPON");
    // 'a' on extra1 = bit 0 (GLOW). 'abch' on wear1 = bits 0|1|2|7.
    assert_eq!(sword.extra_flag_bits, 1u64 << 0);
    let expected_wear = (1u64 << 0) | (1 << 1) | (1 << 2) | (1 << 7);
    assert_eq!(sword.wear_flag_bits, expected_wear);
    assert_eq!(sword.affects, vec![(1, 2)], "A-block 3rd field dropped");

    // DG triggers: 4 records in the .trg file.
    assert_eq!(ir.dg_triggers.len(), 4);
    let trig555 = ir.dg_triggers.iter().find(|t| t.vnum == 555).expect("trig 555");
    assert_eq!(trig555.name, "Reading Room Greeter");
    assert_eq!(trig555.attach_type_raw, 2, "wld attach");
    assert_eq!(trig555.trigger_flags, "g");

    // Quests: one .qst record. Body fields captured in slice 1.
    assert_eq!(ir.quests.len(), 1);
    let q = &ir.quests[0];
    assert_eq!(q.vnum, 30);
    assert_eq!(q.name, "The Reading Quest");
    assert_eq!(q.quest_type, 3, "fixture quest is AQ_MOB_KILL");
    assert_eq!(q.qm_vnum, 3000);
    assert_eq!(q.target_vnum, 3001);
    assert!(!q.accept_msg.is_empty());
}

#[test]
fn maps_tba_ir_to_plan() {
    let (ir, _) = TbaEngine.parse(&fixture_root()).expect("parse");
    let (plan, warnings) = mapping::ir_to_plan(&ir, &empty_opts());

    assert_eq!(plan.areas.len(), 1);
    let area = &plan.areas[0];
    assert_eq!(area.prefix, "northern_midgaard");
    assert_eq!(plan.rooms.len(), 2);
    assert_eq!(plan.mobiles.len(), 2);
    assert_eq!(plan.items.len(), 2);

    // DG trigger handling (post-runtime-interpreter, Phase 4 mapping):
    // - Room T 555 (`g` = WTRIG_ENTER) → OnEnter overlay.
    // - Obj  T 778 (`g` letter, treated as OTRIG_GET on the obj host) → OnGet.
    // - Obj  T 777 (`c` = OTRIG_COMMAND) → OnCommand overlay (Phase 4).
    // - Mob  T 666 (`q` = MTRIG_LEAVE) still surfaces as Info — no IronMUD
    //   "leave" hook yet on mobs.
    let dg_overlays = plan
        .trigger_overlays
        .iter()
        .filter(|o| o.specproc_name.starts_with("dg_trigger_"))
        .count();
    assert_eq!(dg_overlays, 3, "WTRIG_ENTER + OTRIG_GET + OTRIG_COMMAND attach");
    let dg_info_warns = warnings
        .iter()
        .filter(|w| {
            matches!(w.kind, ironmud::import::WarningKind::Info)
                && w.message.contains("DG Scripts trigger")
                && w.message.contains("not yet wired")
        })
        .count();
    assert_eq!(dg_info_warns, 1, "MTRIG_LEAVE surfaces as Info");

    // Quest translation: fixture quest #30 is AQ_MOB_KILL — translates to a
    // PlannedQuest with no warning.
    let quest_warns = warnings
        .iter()
        .filter(|w| {
            matches!(w.kind, ironmud::import::WarningKind::DeferredFeature)
                && w.message.contains("quest #")
        })
        .count();
    assert_eq!(quest_warns, 0, "AQ_MOB_KILL translates cleanly without warns");
    assert_eq!(plan.quests.len(), 1, "fixture quest produces one PlannedQuest");
    let pq = &plan.quests[0];
    assert_eq!(pq.quest_data.vnum, "qst:30");
    assert_eq!(pq.quest_data.name, "The Reading Quest");
    assert_eq!(pq.quest_data.giver_mob_vnum.as_deref(), Some("3000"));
    use ironmud::types::QuestObjective;
    if let QuestObjective::KillMob { vnum, count } = &pq.quest_data.objectives[0] {
        assert_eq!(vnum, "3001");
        assert_eq!(*count, 1);
    } else {
        panic!("expected KillMob objective");
    }

    // No Block warnings on a clean fixture.
    let blocks = warnings.iter().filter(|w| w.severity == Severity::Block).count();
    assert_eq!(blocks, 0);

    // Spawns from M/O resets: 3 (2 mobs + 1 obj). The zone-level T reset is
    // surfaced as deferred (no IronMUD analog).
    assert_eq!(plan.spawns.len(), 3);
    let dg_zone_reset = warnings.iter().any(|w| {
        matches!(w.kind, ironmud::import::WarningKind::DeferredFeature)
            && w.message.contains("DG trigger attachment")
    });
    assert!(dg_zone_reset, "zone T reset surfaced as deferred warning");
}

#[test]
fn analyzer_flags_unsupported_dg_features_during_import() {
    // Build an in-memory ImportIR with a single dg_trigger whose body uses
    // an unsupported variable accessor and an unknown command. The mapping
    // pass should surface a single Info warning summarising both.
    use ironmud::import::{ImportIR, IrDgTrigger, SourceLoc};

    let mut ir = ImportIR::default();
    // Phase 8a: actor/victim/self unknown fields are no longer flagged
    // (treated as dg_var reads). To exercise the analyzer, use an unknown
    // command and an unknown call-form on a non-entity head.
    ir.dg_triggers.push(IrDgTrigger {
        vnum: 9001,
        name: "broken trigger".to_string(),
        attach_type_raw: 0,
        trigger_flags: "g".to_string(),
        numeric_arg: 100,
        arglist: String::new(),
        body: "if %cmd.foobar(1)% == 1\n  bogus_command stuff\nend".to_string(),
        source: SourceLoc::default(),
    });

    let (_plan, warnings) = mapping::ir_to_plan(&ir, &empty_opts());
    let analyzer_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains("unsupported features"))
        .collect();
    assert_eq!(analyzer_warnings.len(), 1, "one analyzer warning per trigger; got {warnings:?}");
    let msg = &analyzer_warnings[0].message;
    assert!(msg.contains("9001"), "warning includes trigger vnum: {msg}");
    assert!(msg.contains("bogus_command"), "warning includes the unknown verb: {msg}");
    assert!(msg.contains("cmd.foobar"), "warning includes the unknown accessor: {msg}");
}

#[test]
fn analyzer_does_not_warn_on_clean_bodies() {
    // The default fixture only uses %send%/%teleport% with %actor%/%actor.class% —
    // all supported. The analyzer should emit zero warnings.
    let (ir, _) = TbaEngine.parse(&fixture_root()).expect("parse");
    let (_plan, warnings) = mapping::ir_to_plan(&ir, &empty_opts());
    let analyzer_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains("unsupported features"))
        .collect();
    assert!(
        analyzer_warnings.is_empty(),
        "no analyzer warnings expected on clean fixture; got {analyzer_warnings:?}"
    );
}

#[test]
fn applies_tba_fixture_to_tmp_db() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let db_path = temp.path().join("tba.db");
    let db = Db::open(&db_path).expect("open tmp db");

    let (ir, _) = TbaEngine.parse(&fixture_root()).expect("parse");
    let (plan, warnings) = mapping::ir_to_plan(&ir, &empty_opts());
    let summary = writer::apply(&db, &plan, &warnings).expect("apply");

    assert_eq!(summary.written_areas, 1);
    assert_eq!(summary.written_rooms, 2);
    assert_eq!(summary.written_mobiles, 2);
    assert_eq!(summary.written_items, 2);
    assert!(summary.written_spawns >= 2, "M-resets translated to spawn points");

    // Spot check: wizard prototype lands.
    let mobs = db.list_all_mobiles().expect("list mobiles");
    let wizard = mobs
        .iter()
        .find(|m| m.is_prototype && m.vnum == "northern_midgaard_3000")
        .expect("wizard prototype");
    assert_eq!(wizard.name, "the wizard");
}

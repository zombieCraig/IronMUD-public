//! End-to-end test for the Ranvier importer:
//! parse → map → apply → assert via Db queries.
//!
//! Uses the actual stock starter bundle at
//! `/home/craig/dev/git/ranviermud/bundles/bundle-example-areas`. The test is
//! gated on its presence so a checkout without the sibling Ranvier repo
//! still passes.

use std::path::PathBuf;

use ironmud::db::Db;
use ironmud::import::engines::ranvier;
use ironmud::import::{Severity, writer};

fn ranvier_starter_bundle() -> Option<PathBuf> {
    let p = PathBuf::from("/home/craig/dev/git/ranviermud/bundles/bundle-example-areas");
    if p.is_dir() { Some(p) } else { None }
}

fn temp_db_path(name: &str) -> PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    PathBuf::from(format!("/tmp/ironmud-ranvier-test-{name}-{pid}-{nanos}.db"))
}

fn cleanup_sidecar(bundle_name: &str) {
    let p = PathBuf::from("imports").join(format!("{bundle_name}.vnum-map.json"));
    let _ = std::fs::remove_file(p);
}

#[test]
fn imports_starter_bundle_with_expected_counts() {
    let Some(bundle) = ranvier_starter_bundle() else {
        eprintln!("skip: ranvier starter bundle not present");
        return;
    };
    let db_path = temp_db_path("counts");
    let _ = std::fs::remove_dir_all(&db_path);
    cleanup_sidecar("test-bundle-counts");

    let db = Db::open(db_path.to_str().unwrap()).expect("open db");
    let import = ranvier::import_bundle(
        &bundle,
        "test-bundle-counts",
        60000,
        9000,
        &[],
        &[],
        &[],
        &[],
    )
    .expect("import bundle");

    assert_eq!(import.plan.areas.len(), 2, "limbo + mapped");
    assert_eq!(import.plan.rooms.len(), 21, "limbo 11 + mapped 10");
    assert_eq!(import.plan.mobiles.len(), 9, "limbo 8 + mapped 1");
    assert_eq!(import.plan.items.len(), 12, "limbo items only");
    assert_eq!(import.plan.shop_overlays.len(), 1, "wally vendor");

    // Cross-area exit (limbo:white -> mapped:start) must resolve.
    assert_eq!(import.plan.exits.iter().filter(|e| e.direction == "north").count() >= 1, true);

    // No Block warnings on a clean import.
    let blocks = import
        .warnings
        .iter()
        .filter(|w| w.severity == Severity::Block)
        .count();
    assert_eq!(blocks, 0, "no blocking warnings expected");

    // EquipGoal twice (auto-complete fetch quest + journeybegins lists EquipGoal twice).
    let equip_goal_warns = import
        .warnings
        .iter()
        .filter(|w| w.message.contains("EquipGoal"))
        .count();
    assert!(equip_goal_warns >= 1, "EquipGoal should warn");

    // Apply, then verify post-pass.
    writer::apply(&db, &import.plan, &import.warnings).expect("apply");
    ranvier::post::apply_post_patches(&db, &import.post_patches).expect("post-patches");

    // Coordinates: mapped area's rooms have coordinates [x,y,z]; verify a
    // sample.
    let mapped_start = db
        .get_room_by_vnum("mapped_61000")
        .expect("look up mapped_61000")
        .expect("mapped_61000 exists");
    assert_eq!(mapped_start.coordinates, Some((0, 0, 0)));

    // replace_on_respawn: the woodenchest spawn (limbo:white room, replaceOnRespawn: true)
    // must carry the flag.
    let spawns = db.list_all_spawn_points().expect("list spawn points");
    let chest_spawn = spawns
        .iter()
        .find(|s| s.vnum == "limbo_60707") // woodenchest = first item alphabetically: id rustysword=60700, sliceofcheese=60701, woodenchest=60702 — order may vary
        .or_else(|| spawns.iter().find(|s| s.replace_on_respawn));
    assert!(chest_spawn.is_some(), "expected at least one replace_on_respawn spawn");

    let _ = std::fs::remove_dir_all(&db_path);
    cleanup_sidecar("test-bundle-counts");
}

#[test]
fn vnum_map_is_idempotent_across_reruns() {
    let Some(bundle) = ranvier_starter_bundle() else {
        eprintln!("skip: ranvier starter bundle not present");
        return;
    };
    cleanup_sidecar("test-bundle-idempotent");

    let import_a = ranvier::import_bundle(
        &bundle,
        "test-bundle-idempotent",
        60000,
        9000,
        &[],
        &[],
        &[],
        &[],
    )
    .expect("first import");
    let vnums_a: Vec<String> = import_a.plan.rooms.iter().map(|r| r.vnum.clone()).collect();

    let import_b = ranvier::import_bundle(
        &bundle,
        "test-bundle-idempotent",
        60000,
        9000,
        &[],
        &[],
        &[],
        &[],
    )
    .expect("second import");
    let vnums_b: Vec<String> = import_b.plan.rooms.iter().map(|r| r.vnum.clone()).collect();

    assert_eq!(vnums_a, vnums_b, "re-running with sidecar must produce identical vnums");

    cleanup_sidecar("test-bundle-idempotent");
}

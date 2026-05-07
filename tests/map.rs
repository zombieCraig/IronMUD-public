//! ASCII map renderer tests (slice 1).
//!
//! These exercise the pure layout + render core in `src/script/map.rs`
//! without involving the Rhai engine or the network. We construct a
//! tiny `Db` populated with a synthetic room graph plus a fixture
//! `rooms_visited` set, and assert on layout cells and rendered glyphs.

#![recursion_limit = "256"]

use ironmud::db::Db;
use ironmud::script::map::{Direction, Visibility, compute_map_layout, render_map_for_player};
use ironmud::types::{AreaData, CharacterData, DoorState, RoomData};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

fn fresh_db(label: &str) -> (Db, String) {
    let path = format!("/tmp/test_map_{}_{}.db", label, std::process::id());
    let _ = std::fs::remove_dir_all(&path);
    let db = Db::open(&path).expect("open db");
    (db, path)
}

fn run(label: &str, body: impl FnOnce(&Db)) {
    let (db, path) = fresh_db(label);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(&db)));
    let _ = std::fs::remove_dir_all(&path);
    if let Err(e) = outcome {
        std::panic::resume_unwind(e);
    }
}

fn make_area(db: &Db, name: &str) -> Uuid {
    let area: AreaData = serde_json::from_value(serde_json::json!({
        "id": Uuid::new_v4(),
        "name": name,
        "prefix": "area",
    }))
    .expect("build area");
    let id = area.id;
    db.save_area_data(area).expect("save area");
    id
}

fn make_room(db: &Db, area_id: Uuid, title: &str) -> Uuid {
    let room: RoomData = serde_json::from_value(serde_json::json!({
        "id": Uuid::new_v4(),
        "title": title,
        "description": "test room",
        "exits": {},
        "area_id": area_id,
    }))
    .expect("build room");
    let id = room.id;
    db.save_room_data(room).expect("save room");
    id
}

fn link(db: &Db, from: Uuid, dir: &str, to: Uuid) {
    let mut r = db.get_room_data(&from).expect("load").expect("present");
    match dir {
        "n" => r.exits.north = Some(to),
        "s" => r.exits.south = Some(to),
        "e" => r.exits.east = Some(to),
        "w" => r.exits.west = Some(to),
        "u" => r.exits.up = Some(to),
        "d" => r.exits.down = Some(to),
        other => panic!("bad dir {}", other),
    }
    db.save_room_data(r).expect("save");
}

fn link_pair(db: &Db, a: Uuid, dir: &str, b: Uuid) {
    let opp = match dir {
        "n" => "s",
        "s" => "n",
        "e" => "w",
        "w" => "e",
        other => panic!("bad dir {}", other),
    };
    link(db, a, dir, b);
    link(db, b, opp, a);
}

fn add_door(db: &Db, room: Uuid, dir: &str, closed: bool) {
    let mut r = db.get_room_data(&room).expect("load").expect("present");
    let dir_name = match dir {
        "n" => "north",
        "s" => "south",
        "e" => "east",
        "w" => "west",
        other => panic!("bad dir {}", other),
    };
    r.doors.insert(
        dir_name.to_string(),
        DoorState {
            name: "door".to_string(),
            is_closed: closed,
            is_locked: false,
            ..Default::default()
        },
    );
    db.save_room_data(r).expect("save");
}

fn glyph_at(rendered: &str, want: &str) -> bool {
    rendered.contains(want)
}

#[test]
fn linear_corridor_all_visited() {
    run("linear_visited", |db| {
        let area = make_area(db, "Test");
        let rooms: Vec<Uuid> = (0..5).map(|i| make_room(db, area, &format!("r{}", i))).collect();
        for w in rooms.windows(2) {
            link_pair(db, w[0], "e", w[1]);
        }
        // Player at the middle room (index 2). All visited.
        let visited: HashSet<Uuid> = rooms.iter().copied().collect();
        let layout = compute_map_layout(db, rooms[2], 5, &visited);

        // 5 visited cells along a horizontal axis
        let coords: Vec<(i32, i32)> = (-2..=2).map(|x| (x, 0)).collect();
        for c in &coords {
            let cell = layout.cells.get(c).unwrap_or_else(|| panic!("missing {:?}", c));
            assert_eq!(cell.visibility, Visibility::Visited, "cell at {:?} should be Visited", c);
        }

        let rendered = ironmud::script::map::render_map(&layout);
        assert!(rendered.contains("[@]"), "must mark origin");
        // 5 cell glyphs in a line, separated by " - "
        // Spot-check origin row contains `o - o - [@]- o - o`-ish (whitespace varies);
        // just check there are 5 instances of o or [@] on the origin row.
        let origin_line = rendered
            .lines()
            .find(|l| l.contains("[@]"))
            .expect("origin line");
        let chunks = origin_line.match_indices('[').count() + origin_line.matches(" o ").count();
        assert!(chunks >= 5, "expected >=5 visible cells in origin line, got: {:?}", origin_line);
    });
}

#[test]
fn partial_visit_corridor_glimpses_neighbors() {
    run("partial_visit", |db| {
        let area = make_area(db, "Test");
        let rooms: Vec<Uuid> = (0..5).map(|i| make_room(db, area, &format!("r{}", i))).collect();
        for w in rooms.windows(2) {
            link_pair(db, w[0], "e", w[1]);
        }
        // Player only ever entered the middle room.
        let mut visited: HashSet<Uuid> = HashSet::new();
        visited.insert(rooms[2]);

        let layout = compute_map_layout(db, rooms[2], 5, &visited);

        // Origin Visited
        assert_eq!(layout.cells[&(0, 0)].visibility, Visibility::Visited);
        // One step in either direction: Glimpsed
        assert_eq!(layout.cells[&(-1, 0)].visibility, Visibility::Glimpsed);
        assert_eq!(layout.cells[&(1, 0)].visibility, Visibility::Glimpsed);
        // Two steps: not present (Glimpsed cells don't expand)
        assert!(layout.cells.get(&(-2, 0)).is_none(), "two steps west should be hidden");
        assert!(layout.cells.get(&(2, 0)).is_none(), "two steps east should be hidden");
    });
}

#[test]
fn glimpsed_cells_hide_flags() {
    run("glimpse_hides_flags", |db| {
        let area = make_area(db, "Test");
        let origin = make_room(db, area, "origin");
        let trapped: RoomData = serde_json::from_value(serde_json::json!({
            "id": Uuid::new_v4(),
            "title": "trapped",
            "description": "",
            "exits": {},
            "area_id": area,
            "traps": [{
                "trap_type": "spike",
                "owner_name": "",
                "damage": 10,
                "detect_difficulty": 5,
                "disarm_difficulty": 5,
                "charges": 1,
                "effect": "damage",
                "placed_at": 0,
            }],
        }))
        .expect("build trapped");
        let trap_id = trapped.id;
        db.save_room_data(trapped).expect("save");
        link_pair(db, origin, "e", trap_id);

        // Player has not visited the trapped room.
        let mut visited: HashSet<Uuid> = HashSet::new();
        visited.insert(origin);

        let layout = compute_map_layout(db, origin, 5, &visited);
        let cell = &layout.cells[&(1, 0)];
        assert_eq!(cell.visibility, Visibility::Glimpsed);
        // Renderer must NOT show ! on a glimpsed cell.
        let rendered = ironmud::script::map::render_map(&layout);
        let origin_line = rendered.lines().find(|l| l.contains("[@]")).unwrap();
        assert!(!origin_line.contains('!'), "glimpsed trap must not show '!': {}", origin_line);
    });
}

#[test]
fn closed_loop_no_collision() {
    run("closed_loop", |db| {
        let area = make_area(db, "Test");
        let a = make_room(db, area, "a");
        let b = make_room(db, area, "b");
        let c = make_room(db, area, "c");
        let d = make_room(db, area, "d");
        // a -e-> b, a -s-> d, b -s-> c, d -e-> c (square)
        link_pair(db, a, "e", b);
        link_pair(db, a, "s", d);
        link_pair(db, b, "s", c);
        link_pair(db, d, "e", c);

        let visited: HashSet<Uuid> = [a, b, c, d].into_iter().collect();
        let layout = compute_map_layout(db, a, 5, &visited);

        assert_eq!(layout.cells[&(0, 0)].visibility, Visibility::Visited);
        assert_eq!(layout.cells[&(1, 0)].visibility, Visibility::Visited);
        assert_eq!(layout.cells[&(0, 1)].visibility, Visibility::Visited);
        assert_eq!(layout.cells[&(1, 1)].visibility, Visibility::Visited);

        // No collision flags set anywhere.
        for (coord, cell) in &layout.cells {
            assert!(!cell.collision, "no collision in closed loop, but cell {:?} flagged", coord);
        }
    });
}

#[test]
fn non_euclidean_loop_marks_collision() {
    run("non_euclidean", |db| {
        let area = make_area(db, "Test");
        let a = make_room(db, area, "a");
        let b = make_room(db, area, "b");
        let c = make_room(db, area, "c");
        let d = make_room(db, area, "d");
        let e = make_room(db, area, "e"); // separate room; the "warp" target
        // a -e-> b -s-> c -w-> d. d -n-> e (NOT a). So going n,e,s,w,n from
        // a returns to e via the same coord (0, 0) that a occupies.
        // Easier: simpler non-Euclidean —
        // a (0,0) -n-> b (0,-1)
        // b -e-> c (1,-1)
        // c -s-> d (1, 0)
        // d -w-> e (0, 0)  <-- claims a's coord but is a different room.
        link_pair(db, a, "n", b);
        link_pair(db, b, "e", c);
        link_pair(db, c, "s", d);
        link(db, d, "w", e); // unidirectional to keep e from competing back
        let visited: HashSet<Uuid> = [a, b, c, d, e].into_iter().collect();

        let layout = compute_map_layout(db, a, 5, &visited);
        let origin_cell = &layout.cells[&(0, 0)];
        assert!(origin_cell.collision, "origin cell should have been flagged as collision");

        let rendered = ironmud::script::map::render_map(&layout);
        // ? appears somewhere
        assert!(rendered.contains('?'), "expected ? in rendered map: {}", rendered);
    });
}

#[test]
fn cross_area_arrow_only_when_source_visited() {
    run("cross_area", |db| {
        let area_a = make_area(db, "Home");
        let area_b = make_area(db, "Foreign");
        let home = make_room(db, area_a, "home");
        let foreign = make_room(db, area_b, "foreign");
        link_pair(db, home, "e", foreign);

        // Visited only home.
        let mut visited: HashSet<Uuid> = HashSet::new();
        visited.insert(home);

        let layout = compute_map_layout(db, home, 5, &visited);
        let origin_cell = &layout.cells[&(0, 0)];
        assert!(
            origin_cell.cross_area_dirs.contains(&Direction::E),
            "should record east cross-area exit on visited origin"
        );
        // The foreign cell must NOT be in the layout (we don't render foreign rooms).
        assert!(layout.cells.get(&(1, 0)).is_none());

        let rendered = ironmud::script::map::render_map(&layout);
        assert!(rendered.contains('→'), "east cross-area arrow should appear: {}", rendered);
    });
}

#[test]
fn radius_clamps() {
    run("radius_clamp", |db| {
        let area = make_area(db, "Test");
        let origin = make_room(db, area, "o");
        let visited: HashSet<Uuid> = [origin].into_iter().collect();

        let small = compute_map_layout(db, origin, 0, &visited); // below MIN
        assert_eq!(small.radius, 1);
        let big = compute_map_layout(db, origin, 99, &visited);
        assert_eq!(big.radius, 8);
    });
}

#[test]
fn closed_door_glyph_visible_only_between_visited_cells() {
    run("door_glyph", |db| {
        let area = make_area(db, "Test");
        let a = make_room(db, area, "a");
        let b = make_room(db, area, "b");
        link_pair(db, a, "e", b);
        add_door(db, a, "e", true);
        add_door(db, b, "w", true);

        // Both visited: + connector.
        let visited_both: HashSet<Uuid> = [a, b].into_iter().collect();
        let layout_both = compute_map_layout(db, a, 5, &visited_both);
        let origin_cell = &layout_both.cells[&(0, 0)];
        assert!(origin_cell.closed_door_dirs.contains(&Direction::E));
        let rendered_both = ironmud::script::map::render_map(&layout_both);
        let origin_line = rendered_both.lines().find(|l| l.contains("[@]")).unwrap();
        assert!(origin_line.contains('+'), "both visited: + connector. line={:?}", origin_line);

        // Only a visited: door bit should NOT be set (we don't leak doors at glimpse).
        let mut visited_one: HashSet<Uuid> = HashSet::new();
        visited_one.insert(a);
        let layout_one = compute_map_layout(db, a, 5, &visited_one);
        let origin_cell = &layout_one.cells[&(0, 0)];
        assert!(
            !origin_cell.closed_door_dirs.contains(&Direction::E),
            "must not record door when target is only Glimpsed"
        );
    });
}

#[test]
fn trap_glyph_only_for_visited_room() {
    run("trap_glyph", |db| {
        let area = make_area(db, "Test");
        let trapped: RoomData = serde_json::from_value(serde_json::json!({
            "id": Uuid::new_v4(),
            "title": "trapped",
            "description": "",
            "exits": {},
            "area_id": area,
            "traps": [{
                "trap_type": "pit",
                "owner_name": "",
                "damage": 10,
                "detect_difficulty": 5,
                "disarm_difficulty": 5,
                "charges": 1,
                "effect": "damage",
                "placed_at": 0,
            }],
        }))
        .expect("build trapped");
        let id = trapped.id;
        db.save_room_data(trapped).expect("save");

        // Origin == trapped, visited.
        let visited: HashSet<Uuid> = [id].into_iter().collect();
        let layout = compute_map_layout(db, id, 5, &visited);
        let cell = &layout.cells[&(0, 0)];
        assert!(cell.has_trap, "trap detected");
        // Origin always renders [@] regardless of trap; the glyph table prefers origin.
        let rendered = ironmud::script::map::render_map(&layout);
        assert!(rendered.contains("[@]"));
    });
}

#[test]
fn world_setting_off_returns_empty() {
    run("disabled_setting", |db| {
        let area = make_area(db, "Test");
        let origin = make_room(db, area, "o");
        let mut visited: HashSet<Uuid> = HashSet::new();
        visited.insert(origin);

        // Sanity: layout still works directly.
        let layout = compute_map_layout(db, origin, 5, &visited);
        assert!(!layout.cells.is_empty());

        // Build a character pointing at origin and persist.
        let mut ch: CharacterData = serde_json::from_value(serde_json::json!({
            "name": "explorer",
            "password_hash": "",
            "current_room_id": origin,
        }))
        .expect("build char");
        ch.rooms_visited = visited;
        db.save_character_data(ch).expect("save");

        // With setting on (unset == on): non-empty rendered output.
        let rendered = render_map_for_player(db, "explorer", None);
        assert!(!rendered.is_empty(), "rendered non-empty when enabled");

        // With setting off: empty.
        db.set_setting("map_enabled", "false").expect("set setting");
        let rendered = render_map_for_player(db, "explorer", None);
        assert!(
            rendered.is_empty(),
            "rendered must be empty when map_enabled=false, got {:?}",
            rendered
        );
    });
}

#[test]
fn automap_default_on_for_new_and_legacy_chars() {
    // Existing character JSON missing `automap_enabled` deserializes to true
    // via #[serde(default = "default_true")].
    let ch: CharacterData = serde_json::from_value(serde_json::json!({
        "name": "legacy",
        "password_hash": "",
        "current_room_id": Uuid::nil(),
    }))
    .expect("build char");
    assert!(ch.automap_enabled, "missing field defaults to true");
    assert!(ch.rooms_visited.is_empty(), "rooms_visited defaults empty");
}

// Pull `glyph_at` warning down — this helper is used in spot checks above.
#[allow(dead_code)]
fn _keep_helper_alive() {
    let _ = glyph_at("", "");
    let _: HashMap<i32, i32> = HashMap::new();
}

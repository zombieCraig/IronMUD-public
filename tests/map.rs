//! ASCII map renderer tests.
//!
//! These exercise the pure layout + render core in `src/script/map.rs`
//! without involving the Rhai engine or the network. We construct a
//! tiny `Db` populated with a synthetic room graph plus a fixture
//! `rooms_visited` set, and assert on layout cells and rendered glyphs.

#![recursion_limit = "256"]

use ironmud::db::Db;
use ironmud::script::map::{Direction, Visibility, compute_map_layout, render_map_for_player};
use ironmud::types::{AreaData, CharacterData, DoorState, RoomData};
use std::collections::HashSet;
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

#[test]
fn linear_corridor_all_visited() {
    run("linear_visited", |db| {
        let area = make_area(db, "Test");
        let rooms: Vec<Uuid> = (0..5).map(|i| make_room(db, area, &format!("r{}", i))).collect();
        for w in rooms.windows(2) {
            link_pair(db, w[0], "e", w[1]);
        }
        let visited: HashSet<Uuid> = rooms.iter().copied().collect();
        let layout = compute_map_layout(db, rooms[2], 5, &visited);

        let coords: Vec<(i32, i32)> = (-2..=2).map(|x| (x, 0)).collect();
        for c in &coords {
            let cell = layout.cells.get(c).unwrap_or_else(|| panic!("missing {:?}", c));
            assert_eq!(cell.visibility, Visibility::Visited, "cell at {:?} should be Visited", c);
        }

        let rendered = ironmud::script::map::render_map(&layout, true, false, false);
        assert!(rendered.contains('@'), "must mark origin");
        // Origin row: `o─o─@─o─o` (5 cells, 4 horizontal connectors)
        let origin_line = rendered
            .lines()
            .find(|l| l.contains('@'))
            .expect("origin line");
        assert!(
            origin_line.contains("o─o─@─o─o"),
            "expected compact 5-cell row, got: {:?}",
            origin_line
        );
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
        let mut visited: HashSet<Uuid> = HashSet::new();
        visited.insert(rooms[2]);

        let layout = compute_map_layout(db, rooms[2], 5, &visited);

        assert_eq!(layout.cells[&(0, 0)].visibility, Visibility::Visited);
        assert_eq!(layout.cells[&(-1, 0)].visibility, Visibility::Glimpsed);
        assert_eq!(layout.cells[&(1, 0)].visibility, Visibility::Glimpsed);
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

        let mut visited: HashSet<Uuid> = HashSet::new();
        visited.insert(origin);

        let layout = compute_map_layout(db, origin, 5, &visited);
        let cell = &layout.cells[&(1, 0)];
        assert_eq!(cell.visibility, Visibility::Glimpsed);
        let rendered = ironmud::script::map::render_map(&layout, true, false, false);
        let origin_line = rendered.lines().find(|l| l.contains('@')).unwrap();
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
        let e = make_room(db, area, "e");
        link_pair(db, a, "n", b);
        link_pair(db, b, "e", c);
        link_pair(db, c, "s", d);
        link(db, d, "w", e);
        let visited: HashSet<Uuid> = [a, b, c, d, e].into_iter().collect();

        let layout = compute_map_layout(db, a, 5, &visited);
        let origin_cell = &layout.cells[&(0, 0)];
        assert!(origin_cell.collision, "origin cell should have been flagged as collision");

        let rendered = ironmud::script::map::render_map(&layout, true, false, false);
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

        let mut visited: HashSet<Uuid> = HashSet::new();
        visited.insert(home);

        let layout = compute_map_layout(db, home, 5, &visited);
        let origin_cell = &layout.cells[&(0, 0)];
        assert!(
            origin_cell.cross_area_dirs.contains(&Direction::E),
            "should record east cross-area exit on visited origin"
        );
        assert!(layout.cells.get(&(1, 0)).is_none());

        let rendered = ironmud::script::map::render_map(&layout, true, false, false);
        assert!(rendered.contains('→'), "east cross-area arrow should appear: {}", rendered);
    });
}

#[test]
fn radius_clamps() {
    run("radius_clamp", |db| {
        let area = make_area(db, "Test");
        let origin = make_room(db, area, "o");
        let visited: HashSet<Uuid> = [origin].into_iter().collect();

        let small = compute_map_layout(db, origin, 0, &visited);
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

        let visited_both: HashSet<Uuid> = [a, b].into_iter().collect();
        let layout_both = compute_map_layout(db, a, 5, &visited_both);
        let origin_cell = &layout_both.cells[&(0, 0)];
        assert!(origin_cell.closed_door_dirs.contains(&Direction::E));
        let rendered_both = ironmud::script::map::render_map(&layout_both, true, false, false);
        let origin_line = rendered_both.lines().find(|l| l.contains('@')).unwrap();
        assert!(
            origin_line.contains('┼'),
            "both visited: ┼ door connector. line={:?}",
            origin_line
        );

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

        let visited: HashSet<Uuid> = [id].into_iter().collect();
        let layout = compute_map_layout(db, id, 5, &visited);
        let cell = &layout.cells[&(0, 0)];
        assert!(cell.has_trap, "trap detected");
        let rendered = ironmud::script::map::render_map(&layout, true, false, false);
        assert!(rendered.contains('@'));
    });
}

#[test]
fn world_setting_off_returns_empty() {
    run("disabled_setting", |db| {
        let area = make_area(db, "Test");
        let origin = make_room(db, area, "o");
        let mut visited: HashSet<Uuid> = HashSet::new();
        visited.insert(origin);

        let layout = compute_map_layout(db, origin, 5, &visited);
        assert!(!layout.cells.is_empty());

        let mut ch: CharacterData = serde_json::from_value(serde_json::json!({
            "name": "explorer",
            "password_hash": "",
            "current_room_id": origin,
        }))
        .expect("build char");
        ch.rooms_visited = visited;
        db.save_character_data(ch).expect("save");

        let rendered = render_map_for_player(db, "explorer", None);
        assert!(!rendered.is_empty(), "rendered non-empty when enabled");

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
fn automap_default_off_for_legacy_chars() {
    let ch: CharacterData = serde_json::from_value(serde_json::json!({
        "name": "legacy",
        "password_hash": "",
        "current_room_id": Uuid::nil(),
    }))
    .expect("build char");
    assert!(!ch.automap_enabled, "missing field defaults to false");
    assert!(ch.rooms_visited.is_empty(), "rooms_visited defaults empty");
}

// ---- Slice 2 tests ----

fn make_shopkeeper_in_room(db: &Db, room: Uuid, name: &str) -> Uuid {
    let mob: ironmud::types::MobileData = serde_json::from_value(serde_json::json!({
        "id": Uuid::new_v4(),
        "name": name,
        "short_desc": format!("{} stands here.", name),
        "long_desc": "",
        "current_room_id": room,
        "is_prototype": false,
        "flags": {
            "shopkeeper": true,
        },
    }))
    .expect("build mob");
    let id = mob.id;
    db.save_mobile_data(mob).expect("save mob");
    id
}

#[test]
fn shop_glyph_for_visited_shop_room() {
    run("shop_visited", |db| {
        let area = make_area(db, "Test");
        let origin = make_room(db, area, "origin");
        let shop = make_room(db, area, "shop");
        link_pair(db, origin, "e", shop);
        make_shopkeeper_in_room(db, shop, "barkeep");

        let visited: HashSet<Uuid> = [origin, shop].into_iter().collect();
        let layout = compute_map_layout(db, origin, 5, &visited);
        let cell = &layout.cells[&(1, 0)];
        assert!(cell.has_shop, "visited shop cell should be flagged has_shop");

        let rendered = ironmud::script::map::render_map(&layout, true, false, false);
        let origin_line = rendered.lines().find(|l| l.contains('@')).unwrap();
        assert!(
            origin_line.contains('#'),
            "expected `#` glyph in origin row: {:?}",
            origin_line
        );
        assert!(rendered.contains("# shop"), "legend must include `# shop`: {}", rendered);
    });
}

#[test]
fn shop_glyph_suppressed_for_glimpsed_neighbor() {
    run("shop_glimpse", |db| {
        let area = make_area(db, "Test");
        let origin = make_room(db, area, "origin");
        let shop = make_room(db, area, "shop");
        link_pair(db, origin, "e", shop);
        make_shopkeeper_in_room(db, shop, "barkeep");

        let visited: HashSet<Uuid> = [origin].into_iter().collect();
        let layout = compute_map_layout(db, origin, 5, &visited);
        let cell = &layout.cells[&(1, 0)];
        assert_eq!(cell.visibility, Visibility::Glimpsed);
        assert!(!cell.has_shop, "glimpsed cell must suppress shop flag");

        let rendered = ironmud::script::map::render_map(&layout, true, false, false);
        let origin_line = rendered.lines().find(|l| l.contains('@')).unwrap();
        // The `#` glyph as a cell glyph would appear in the origin row's east cell
        // slot (col 4 of `@─o─...`). Just check that no `#` appears anywhere on the
        // origin line — connectors and legend `#` are on different lines.
        assert!(
            !origin_line.contains('#'),
            "glimpsed shop must render as plain o, no #: {:?}",
            origin_line
        );
    });
}

#[test]
fn shop_glyph_ignores_prototype_shopkeeper() {
    run("shop_prototype", |db| {
        let area = make_area(db, "Test");
        let origin = make_room(db, area, "origin");
        let mob: ironmud::types::MobileData = serde_json::from_value(serde_json::json!({
            "id": Uuid::new_v4(),
            "name": "proto shopkeeper",
            "short_desc": "A proto shopkeeper.",
            "long_desc": "",
            "current_room_id": origin,
            "is_prototype": true,
            "flags": { "shopkeeper": true },
        }))
        .expect("build mob");
        db.save_mobile_data(mob).expect("save");

        let visited: HashSet<Uuid> = [origin].into_iter().collect();
        let layout = compute_map_layout(db, origin, 5, &visited);
        assert!(!layout.cells[&(0, 0)].has_shop);
    });
}

#[test]
fn legend_can_be_suppressed() {
    run("legend_off", |db| {
        let area = make_area(db, "Test");
        let origin = make_room(db, area, "origin");
        let visited: HashSet<Uuid> = [origin].into_iter().collect();
        let layout = compute_map_layout(db, origin, 5, &visited);

        let with_legend = ironmud::script::map::render_map(&layout, true, false, false);
        let without = ironmud::script::map::render_map(&layout, false, false, false);
        assert!(with_legend.contains("Legend:"), "expected legend in default render");
        assert!(!without.contains("Legend:"), "legend must be suppressed when show_legend=false");
    });
}

#[test]
fn collision_dimmed_only_when_colors_enabled() {
    run("dim_collision", |db| {
        let area = make_area(db, "Test");
        let a = make_room(db, area, "a");
        let b = make_room(db, area, "b");
        let c = make_room(db, area, "c");
        let d = make_room(db, area, "d");
        let e = make_room(db, area, "e");
        link_pair(db, a, "n", b);
        link_pair(db, b, "e", c);
        link_pair(db, c, "s", d);
        link(db, d, "w", e);
        let visited: HashSet<Uuid> = [a, b, c, d, e].into_iter().collect();
        let layout = compute_map_layout(db, a, 5, &visited);

        let plain = ironmud::script::map::render_map(&layout, false, false, false);
        let dimmed = ironmud::script::map::render_map(&layout, false, true, false);

        assert!(plain.contains('?'), "plain output should still contain `?`");
        assert!(
            !plain.contains("\x1b[2m"),
            "plain output must not contain dim ANSI: {:?}",
            plain
        );
        assert!(
            dimmed.contains("\x1b[2m"),
            "colored output must wrap collision in dim ANSI: {:?}",
            dimmed
        );
        assert!(
            dimmed.contains("\x1b[0m"),
            "colored output must close dim ANSI: {:?}",
            dimmed
        );
    });
}

// ---- Slice 3 tests ----

#[test]
fn automap_radius_default_is_three_for_legacy_chars() {
    let ch: CharacterData = serde_json::from_value(serde_json::json!({
        "name": "legacy",
        "password_hash": "",
        "current_room_id": Uuid::nil(),
    }))
    .expect("build char");
    assert_eq!(ch.automap_radius, 3, "missing field defaults to 3 (AUTOMAP_DEFAULT_RADIUS)");
}

#[test]
fn ascii_map_default_off_for_legacy_chars() {
    let ch: CharacterData = serde_json::from_value(serde_json::json!({
        "name": "legacy",
        "password_hash": "",
        "current_room_id": Uuid::nil(),
    }))
    .expect("build char");
    assert!(!ch.ascii_map, "ascii_map missing field defaults to false (Unicode)");
}

#[test]
fn unicode_connectors_used_by_default() {
    run("unicode_connectors", |db| {
        let area = make_area(db, "Test");
        let rooms: Vec<Uuid> = (0..3).map(|i| make_room(db, area, &format!("r{}", i))).collect();
        for w in rooms.windows(2) {
            link_pair(db, w[0], "e", w[1]);
        }
        let visited: HashSet<Uuid> = rooms.iter().copied().collect();
        let layout = compute_map_layout(db, rooms[1], 5, &visited);

        let rendered = ironmud::script::map::render_map(&layout, false, false, false);
        let origin_line = rendered.lines().find(|l| l.contains('@')).unwrap();
        assert!(
            origin_line.contains("o─@─o"),
            "expected Unicode horizontal connectors: {:?}",
            origin_line
        );
        assert!(!origin_line.contains('-'), "should not contain ASCII '-': {:?}", origin_line);
    });
}

#[test]
fn closed_door_renders_unicode_cross_by_default() {
    run("door_unicode", |db| {
        let area = make_area(db, "Test");
        let a = make_room(db, area, "a");
        let b = make_room(db, area, "b");
        link_pair(db, a, "e", b);
        add_door(db, a, "e", true);
        add_door(db, b, "w", true);
        let visited: HashSet<Uuid> = [a, b].into_iter().collect();
        let layout = compute_map_layout(db, a, 5, &visited);
        let rendered = ironmud::script::map::render_map(&layout, false, false, false);
        assert!(rendered.contains('┼'), "expected ┼ door connector: {:?}", rendered);
        assert!(!rendered.contains('+'), "should not contain ASCII '+': {:?}", rendered);
    });
}

#[test]
fn ascii_only_reverts_connectors_and_arrows() {
    run("ascii_fallback", |db| {
        let area_a = make_area(db, "Home");
        let area_b = make_area(db, "Foreign");
        let home = make_room(db, area_a, "home");
        let east_room = make_room(db, area_a, "east");
        let foreign = make_room(db, area_b, "foreign");
        link_pair(db, home, "e", east_room);
        link_pair(db, east_room, "n", foreign); // cross-area to the north
        add_door(db, home, "e", true);
        add_door(db, east_room, "w", true);
        let visited: HashSet<Uuid> = [home, east_room].into_iter().collect();
        let layout = compute_map_layout(db, home, 5, &visited);

        let rendered = ironmud::script::map::render_map(&layout, false, false, true);
        // ASCII connectors and cross-area arrow.
        assert!(rendered.contains('+'), "expected ASCII '+' door: {:?}", rendered);
        assert!(rendered.contains('^'), "expected ASCII '^' arrow: {:?}", rendered);
        // No Unicode box-drawing chars.
        assert!(!rendered.contains('┼'));
        assert!(!rendered.contains('─'));
        assert!(!rendered.contains('│'));
        assert!(!rendered.contains('↑'));
    });
}

#[test]
fn origin_renders_as_at_sign_not_bracketed() {
    run("origin_at", |db| {
        let area = make_area(db, "Test");
        let origin = make_room(db, area, "o");
        let visited: HashSet<Uuid> = [origin].into_iter().collect();
        let layout = compute_map_layout(db, origin, 1, &visited);
        let rendered = ironmud::script::map::render_map(&layout, false, false, false);
        assert!(rendered.contains('@'), "expected '@' origin: {:?}", rendered);
        assert!(!rendered.contains("[@]"), "should not contain old '[@]' marker: {:?}", rendered);
    });
}

#[test]
fn origin_colorized_when_colors_enabled() {
    run("origin_color", |db| {
        let area = make_area(db, "Test");
        let origin = make_room(db, area, "o");
        let visited: HashSet<Uuid> = [origin].into_iter().collect();
        let layout = compute_map_layout(db, origin, 1, &visited);
        let rendered = ironmud::script::map::render_map(&layout, false, true, false);
        assert!(
            rendered.contains("\x1b[1;33m@\x1b[0m"),
            "expected bright-yellow @ wrapping: {:?}",
            rendered
        );
    });
}

#[test]
fn shop_water_trap_each_colorized() {
    run("flag_colors", |db| {
        let area = make_area(db, "Test");
        let origin = make_room(db, area, "origin");

        // Shop neighbor east.
        let shop = make_room(db, area, "shop");
        link_pair(db, origin, "e", shop);
        make_shopkeeper_in_room(db, shop, "barkeep");

        // Water neighbor west.
        let water: RoomData = serde_json::from_value(serde_json::json!({
            "id": Uuid::new_v4(),
            "title": "river",
            "description": "",
            "exits": {},
            "area_id": area,
            "water_type": "freshwater",
        }))
        .expect("build water");
        let water_id = water.id;
        db.save_room_data(water).expect("save water");
        link_pair(db, origin, "w", water_id);

        // Trap neighbor south.
        let trap: RoomData = serde_json::from_value(serde_json::json!({
            "id": Uuid::new_v4(),
            "title": "trap",
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
        .expect("build trap");
        let trap_id = trap.id;
        db.save_room_data(trap).expect("save trap");
        link_pair(db, origin, "s", trap_id);

        let visited: HashSet<Uuid> = [origin, shop, water_id, trap_id].into_iter().collect();
        let layout = compute_map_layout(db, origin, 5, &visited);
        let rendered = ironmud::script::map::render_map(&layout, false, true, false);

        assert!(rendered.contains("\x1b[36m#\x1b[0m"), "shop cyan: {:?}", rendered);
        assert!(rendered.contains("\x1b[34m~\x1b[0m"), "water blue: {:?}", rendered);
        assert!(rendered.contains("\x1b[31m!\x1b[0m"), "trap red: {:?}", rendered);
    });
}

#[test]
fn glimpsed_cells_dim_when_colors_enabled() {
    run("glimpsed_dim", |db| {
        let area = make_area(db, "Test");
        let origin = make_room(db, area, "o");
        let neighbor = make_room(db, area, "n");
        link_pair(db, origin, "e", neighbor);
        let visited: HashSet<Uuid> = [origin].into_iter().collect();
        let layout = compute_map_layout(db, origin, 5, &visited);
        let rendered = ironmud::script::map::render_map(&layout, false, true, false);
        // The glimpsed `o` should be wrapped in dim ANSI.
        assert!(
            rendered.contains("\x1b[2mo\x1b[0m"),
            "expected dim-wrapped glimpsed o: {:?}",
            rendered
        );
    });
}

#[test]
fn colored_legend_includes_color_codes() {
    run("legend_colors", |db| {
        let area = make_area(db, "Test");
        let origin = make_room(db, area, "o");
        let visited: HashSet<Uuid> = [origin].into_iter().collect();
        let layout = compute_map_layout(db, origin, 1, &visited);
        let with_color = ironmud::script::map::render_map(&layout, true, true, false);
        assert!(
            with_color.contains("\x1b[1;33m@\x1b[0m you"),
            "legend should colorize @ in yellow: {:?}",
            with_color
        );
        let without_color = ironmud::script::map::render_map(&layout, true, false, false);
        assert!(
            !without_color.contains("\x1b["),
            "no-color legend must not contain escape codes: {:?}",
            without_color
        );
    });
}

#[test]
fn no_color_path_is_byte_clean() {
    run("byte_clean", |db| {
        let area = make_area(db, "Test");
        let a = make_room(db, area, "a");
        let b = make_room(db, area, "b");
        let c = make_room(db, area, "c");
        let d = make_room(db, area, "d");
        let e = make_room(db, area, "e");
        // Non-Euclidean loop produces a `?` collision; verify even that doesn't
        // leak escapes when colors are off.
        link_pair(db, a, "n", b);
        link_pair(db, b, "e", c);
        link_pair(db, c, "s", d);
        link(db, d, "w", e);
        let visited: HashSet<Uuid> = [a, b, c, d, e].into_iter().collect();
        let layout = compute_map_layout(db, a, 5, &visited);
        let rendered = ironmud::script::map::render_map(&layout, true, false, false);
        assert!(
            !rendered.contains("\x1b["),
            "no-color path must not emit ANSI escapes: {:?}",
            rendered
        );
    });
}

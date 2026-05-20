//! ASCII map renderer (`map` command + `automap` integration).
//!
//! Two-step pipeline:
//!
//! 1. `compute_map_layout` walks the room exit graph from a player's
//!    current room, BFS-style, assigning `(x, y)` integer coords to
//!    cells. Visited rooms (those in `CharacterData.rooms_visited`)
//!    expand further; one-step neighbors of visited rooms are recorded
//!    as `Glimpsed` and never expand. Cross-area exits and collisions
//!    are tagged on cells without aborting the walk.
//!
//! 2. `render_map` turns the layout into an ASCII string using the
//!    glyph table documented in the slice-1 plan. Glimpsed cells render
//!    as `o` with no flags revealed; only Visited cells reveal water /
//!    trap glyphs and are eligible for closed-door (`+`) connectors.
//!
//! Slice 1 deliberately omits the shop glyph: detecting a shopkeeper
//! requires a full mobile scan per cell. We'll add it in slice 2 once
//! we have a per-area shop-room cache.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use rhai::Engine;
use uuid::Uuid;

use crate::SharedConnections;
use crate::db::Db;

pub const DEFAULT_RADIUS: i32 = 5;
pub const AUTOMAP_DEFAULT_RADIUS: i32 = 3;
pub const MIN_RADIUS: i32 = 1;
pub const MAX_RADIUS: i32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Visited,
    Glimpsed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    N,
    S,
    E,
    W,
}

impl Direction {
    fn step(self) -> (i32, i32) {
        match self {
            Direction::N => (0, -1),
            Direction::S => (0, 1),
            Direction::E => (1, 0),
            Direction::W => (-1, 0),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Direction::N => "north",
            Direction::S => "south",
            Direction::E => "east",
            Direction::W => "west",
        }
    }

    fn arrow(self) -> char {
        match self {
            Direction::N => '↑',
            Direction::S => '↓',
            Direction::E => '→',
            Direction::W => '←',
        }
    }

    fn arrow_ascii(self) -> char {
        match self {
            Direction::N => '^',
            Direction::S => 'v',
            Direction::E => '>',
            Direction::W => '<',
        }
    }

    fn opposite(self) -> Direction {
        match self {
            Direction::N => Direction::S,
            Direction::S => Direction::N,
            Direction::E => Direction::W,
            Direction::W => Direction::E,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MapCell {
    pub room_id: Uuid,
    pub visibility: Visibility,
    pub is_origin: bool,
    pub has_up: bool,
    pub has_down: bool,
    pub has_water: bool,
    pub has_trap: bool,
    pub has_shop: bool,
    /// Directions toward Visited neighbors that share a closed door with us.
    pub closed_door_dirs: HashSet<Direction>,
    /// Directions where the exit leaves the current area entirely.
    pub cross_area_dirs: HashSet<Direction>,
    /// Cardinal directions in which this cell's room has an exit leading to
    /// the cell placed at the adjacent grid coord. Used at render time to
    /// suppress phantom connectors between coincidentally-adjacent rooms.
    pub linked_dirs: HashSet<Direction>,
    /// True when another room also tried to claim this coord. Renders `?`.
    pub collision: bool,
}

#[derive(Debug, Clone)]
pub struct MapLayout {
    pub origin: (i32, i32),
    pub radius: i32,
    pub cells: HashMap<(i32, i32), MapCell>,
    pub area_name: String,
}

impl MapLayout {
    pub fn empty() -> Self {
        MapLayout {
            origin: (0, 0),
            radius: DEFAULT_RADIUS,
            cells: HashMap::new(),
            area_name: String::new(),
        }
    }
}

pub fn compute_map_layout(db: &Db, origin: Uuid, radius: i32, visited: &HashSet<Uuid>) -> MapLayout {
    let radius = radius.clamp(MIN_RADIUS, MAX_RADIUS);

    let origin_room = match db.get_room_data(&origin) {
        Ok(Some(r)) => r,
        _ => return MapLayout::empty(),
    };
    let area_id = origin_room.area_id;
    let area_name = area_id
        .and_then(|aid| db.get_area_data(&aid).ok().flatten().map(|a| a.name))
        .unwrap_or_default();

    // Build a single set of room_ids that contain a live (non-prototype)
    // shopkeeper. One full mob-table scan per render; per-cell lookup is O(1).
    // Cheaper than scanning room.mobs per cell (RoomData has no mob index).
    let shop_rooms = collect_shop_rooms(db);

    let mut cells: HashMap<(i32, i32), MapCell> = HashMap::new();

    let origin_coord = (0_i32, 0_i32);
    cells.insert(origin_coord, build_cell(&origin_room, Visibility::Visited, true, &shop_rooms));

    // BFS: only Visited cells expand. We record visibility before enqueueing.
    let mut queue: VecDeque<((i32, i32), Uuid)> = VecDeque::new();
    queue.push_back((origin_coord, origin_room.id));

    while let Some((coord, room_id)) = queue.pop_front() {
        let room = match db.get_room_data(&room_id) {
            Ok(Some(r)) => r,
            _ => continue,
        };

        for (dir, neighbor_id) in cardinals(&room) {
            let (dx, dy) = dir.step();
            let new_coord = (coord.0 + dx, coord.1 + dy);
            // Square radius (max-of-abs) — keeps the grid square.
            if new_coord.0.abs() > radius || new_coord.1.abs() > radius {
                continue;
            }

            let neighbor_room = match db.get_room_data(&neighbor_id) {
                Ok(Some(r)) => r,
                _ => continue,
            };

            // Cross-area: stop without placing the foreign cell. Stub the
            // arrow on the source — but only if source is Visited.
            if neighbor_room.area_id != area_id {
                if let Some(src) = cells.get_mut(&coord) {
                    if src.visibility == Visibility::Visited {
                        src.cross_area_dirs.insert(dir);
                    }
                }
                continue;
            }

            // Record the real exit on the source side so the renderer can
            // distinguish a true connection from coincidental grid adjacency.
            if let Some(src) = cells.get_mut(&coord) {
                src.linked_dirs.insert(dir);
            }

            let neighbor_visited_or_origin = visited.contains(&neighbor_id) || neighbor_id == origin_room.id;

            // Closed-door annotation: only marked when both source and target
            // are Visited (we don't leak door presence at glimpse range).
            let door_closed = room.doors.get(dir.name()).map(|d| d.is_closed).unwrap_or(false);
            if door_closed {
                if let Some(src) = cells.get_mut(&coord) {
                    if src.visibility == Visibility::Visited && neighbor_visited_or_origin {
                        src.closed_door_dirs.insert(dir);
                    }
                }
            }

            // Collision: another room already lives at this coord.
            if let Some(existing) = cells.get_mut(&new_coord) {
                if existing.room_id != neighbor_id {
                    existing.collision = true;
                } else {
                    // Same room re-reached via a different path: still a real
                    // connection from this side.
                    existing.linked_dirs.insert(dir.opposite());
                }
                continue;
            }

            let target_visibility = if neighbor_visited_or_origin {
                Visibility::Visited
            } else {
                Visibility::Glimpsed
            };

            let mut new_cell = build_cell(&neighbor_room, target_visibility, false, &shop_rooms);
            new_cell.linked_dirs.insert(dir.opposite());
            cells.insert(new_coord, new_cell);

            // Only Visited cells expand further; Glimpsed cells are leaves.
            if target_visibility == Visibility::Visited {
                queue.push_back((new_coord, neighbor_id));
            }
        }
    }

    MapLayout {
        origin: origin_coord,
        radius,
        cells,
        area_name,
    }
}

fn cardinals(room: &crate::RoomData) -> Vec<(Direction, Uuid)> {
    let mut out = Vec::with_capacity(4);
    if let Some(id) = room.exits.north {
        out.push((Direction::N, id));
    }
    if let Some(id) = room.exits.south {
        out.push((Direction::S, id));
    }
    if let Some(id) = room.exits.east {
        out.push((Direction::E, id));
    }
    if let Some(id) = room.exits.west {
        out.push((Direction::W, id));
    }
    out
}

fn build_cell(
    room: &crate::RoomData,
    visibility: Visibility,
    is_origin: bool,
    shop_rooms: &HashSet<Uuid>,
) -> MapCell {
    // Shop glyph only reveals on Visited cells; Glimpsed cells suppress all
    // flag glyphs per slice-1 contract.
    let has_shop = matches!(visibility, Visibility::Visited) && shop_rooms.contains(&room.id);
    MapCell {
        room_id: room.id,
        visibility,
        is_origin,
        has_up: room.exits.up.is_some(),
        has_down: room.exits.down.is_some(),
        has_water: !matches!(room.water_type, crate::WaterType::None),
        has_trap: !room.traps.is_empty(),
        has_shop,
        closed_door_dirs: HashSet::new(),
        cross_area_dirs: HashSet::new(),
        linked_dirs: HashSet::new(),
        collision: false,
    }
}

/// Single full-table mobile scan returning the set of room_ids that house a
/// live (non-prototype) shopkeeper. Used by the renderer to stamp `#` glyphs.
fn collect_shop_rooms(db: &Db) -> HashSet<Uuid> {
    let mut out = HashSet::new();
    let mobs = match db.list_all_mobiles() {
        Ok(v) => v,
        Err(_) => return out,
    };
    for m in mobs {
        if m.is_prototype {
            continue;
        }
        if !m.flags.shopkeeper {
            continue;
        }
        if let Some(rid) = m.current_room_id {
            out.insert(rid);
        }
    }
    out
}

pub fn render_map(
    layout: &MapLayout,
    show_legend: bool,
    colors_enabled: bool,
    ascii_only: bool,
) -> String {
    let r = layout.radius;
    let grid_n = (2 * r + 1) as usize;
    // Compact layout: each cell is 1 char wide and 1 row tall. Connectors
    // (1 char) live between cells. Junction slots at (odd col, odd row) stay
    // blank — every connector still joins exactly two cells.
    let total_w = (2 * grid_n).saturating_sub(1);
    let total_h = (2 * grid_n).saturating_sub(1);

    let h_conn = if ascii_only { '-' } else { '─' };
    let v_conn = if ascii_only { '|' } else { '│' };
    let door_conn = if ascii_only { '+' } else { '┼' };

    let mut grid: Vec<Vec<char>> = vec![vec![' '; total_w]; total_h];

    for cy in -r..=r {
        for cx in -r..=r {
            let coord = (cx, cy);
            let cell = layout.cells.get(&coord);

            let row = ((cy + r) * 2) as usize;
            let col = ((cx + r) * 2) as usize;

            grid[row][col] = render_cell_glyph(cell);

            // Connectors between adjacent cells. Only render when a real exit
            // actually joins the two rooms — coincidental grid adjacency (e.g.
            // two rooms reached via different BFS paths) must not be drawn.
            if let Some(c) = cell {
                if let Some(east) = layout.cells.get(&(cx + 1, cy)) {
                    let conn_col = col + 1;
                    if conn_col < total_w
                        && (c.linked_dirs.contains(&Direction::E)
                            || east.linked_dirs.contains(&Direction::W))
                    {
                        let ch = if c.closed_door_dirs.contains(&Direction::E)
                            || east.closed_door_dirs.contains(&Direction::W)
                        {
                            door_conn
                        } else {
                            h_conn
                        };
                        grid[row][conn_col] = ch;
                    }
                }
                if let Some(south) = layout.cells.get(&(cx, cy + 1)) {
                    let conn_row = row + 1;
                    if conn_row < total_h
                        && (c.linked_dirs.contains(&Direction::S)
                            || south.linked_dirs.contains(&Direction::N))
                    {
                        let ch = if c.closed_door_dirs.contains(&Direction::S)
                            || south.closed_door_dirs.contains(&Direction::N)
                        {
                            door_conn
                        } else {
                            v_conn
                        };
                        grid[conn_row][col] = ch;
                    }
                }
            }
        }
    }

    // Cross-area arrows overlay the connector slot adjacent to the source cell.
    for cy in -r..=r {
        for cx in -r..=r {
            if let Some(c) = layout.cells.get(&(cx, cy)) {
                if c.cross_area_dirs.is_empty() {
                    continue;
                }
                let row = ((cy + r) * 2) as usize;
                let col = ((cx + r) * 2) as usize;
                for d in c.cross_area_dirs.iter().copied() {
                    let arrow_ch = if ascii_only { d.arrow_ascii() } else { d.arrow() };
                    match d {
                        Direction::N => {
                            if row > 0 {
                                grid[row - 1][col] = arrow_ch;
                            }
                        }
                        Direction::S => {
                            if row + 1 < total_h {
                                grid[row + 1][col] = arrow_ch;
                            }
                        }
                        Direction::E => {
                            if col + 1 < total_w {
                                grid[row][col + 1] = arrow_ch;
                            }
                        }
                        Direction::W => {
                            if col > 0 {
                                grid[row][col - 1] = arrow_ch;
                            }
                        }
                    }
                }
            }
        }
    }

    let mut output = String::new();
    if !layout.area_name.is_empty() {
        output.push_str(&format!("Map of {}:\n", layout.area_name));
    }
    for (row_idx, row) in grid.into_iter().enumerate() {
        let is_cell_row = row_idx % 2 == 0;
        if !colors_enabled || !is_cell_row {
            let line: String = row.into_iter().collect();
            output.push_str(line.trim_end());
            output.push('\n');
            continue;
        }
        // Cell row + colors on: per-cell color via cell_color(); connectors
        // (odd cols) stay uncolored. Track current ANSI style and emit
        // transitions inline so unicode connectors don't break byte slicing.
        let cy = (row_idx as i32 / 2) - r;
        let mut line = String::new();
        let mut current_style: Option<&'static str> = None;
        for (i, ch) in row.into_iter().enumerate() {
            let in_cell_slot = i % 2 == 0;
            let cx = ((i / 2) as i32) - r;
            let want_style = if in_cell_slot {
                cell_color(layout.cells.get(&(cx, cy)))
            } else {
                None
            };
            if want_style != current_style {
                if current_style.is_some() {
                    line.push_str("\x1b[0m");
                }
                if let Some(code) = want_style {
                    line.push_str(code);
                }
                current_style = want_style;
            }
            line.push(ch);
        }
        if current_style.is_some() {
            line.push_str("\x1b[0m");
        }
        output.push_str(line.trim_end());
        output.push('\n');
    }

    if let Some(o) = layout.cells.get(&layout.origin) {
        let mut parts: Vec<&str> = Vec::new();
        if o.has_up {
            parts.push("up");
        }
        if o.has_down {
            parts.push("down");
        }
        if !parts.is_empty() {
            output.push_str(&format!("({} exits available)\n", parts.join("/")));
        }
    }

    if show_legend {
        output.push_str(&build_legend(colors_enabled, ascii_only));
    }
    output
}

fn render_cell_glyph(cell: Option<&MapCell>) -> char {
    match cell {
        None => ' ',
        Some(c) if c.collision => '?',
        Some(c) if c.is_origin => '@',
        Some(c) if c.visibility == Visibility::Glimpsed => 'o',
        Some(c) if c.has_shop => '#',
        Some(c) if c.has_trap => '!',
        Some(c) if c.has_water => '~',
        Some(_) => 'o',
    }
}

/// Per-cell ANSI color code (without trailing reset). Mirrors
/// `render_cell_glyph`'s priority list 1:1 — adding a glyph means
/// updating both fns. None means "use default terminal color".
fn cell_color(cell: Option<&MapCell>) -> Option<&'static str> {
    match cell {
        None => None,
        Some(c) if c.collision => Some("\x1b[2m"),
        Some(c) if c.is_origin => Some("\x1b[1;33m"),
        Some(c) if c.visibility == Visibility::Glimpsed => Some("\x1b[2m"),
        Some(c) if c.has_shop => Some("\x1b[36m"),
        Some(c) if c.has_trap => Some("\x1b[31m"),
        Some(c) if c.has_water => Some("\x1b[34m"),
        Some(_) => None,
    }
}

fn build_legend(colors_enabled: bool, ascii_only: bool) -> String {
    let door = if ascii_only { '+' } else { '┼' };
    if colors_enabled {
        format!(
            "Legend: \x1b[1;33m@\x1b[0m you  o room  \x1b[36m#\x1b[0m shop  \x1b[34m~\x1b[0m water  \x1b[31m!\x1b[0m trap  {} door  \x1b[2m?\x1b[0m unmapped\n",
            door
        )
    } else {
        format!(
            "Legend: @ you  o room  # shop  ~ water  ! trap  {} door  ? unmapped\n",
            door
        )
    }
}

/// Read the admin toggle. Defaults to enabled when unset.
pub fn enabled(db: &Db) -> bool {
    match db.get_setting("map_enabled") {
        Ok(Some(v)) => !matches!(v.to_lowercase().as_str(), "false" | "0" | "off" | "no"),
        _ => true,
    }
}

/// Build a map for the given player by name. Returns the rendered string,
/// or an empty string if the system is disabled / character missing / etc.
///
/// Defaults: legend shown, colors off, Unicode connectors. Session-aware
/// callers should prefer `render_map_for_player_with_options`.
pub fn render_map_for_player(db: &Db, player_name: &str, radius: Option<i32>) -> String {
    render_map_for_player_with_options(db, player_name, radius, true, false, false)
}

pub fn render_map_for_player_with_options(
    db: &Db,
    player_name: &str,
    radius: Option<i32>,
    show_legend: bool,
    colors_enabled: bool,
    ascii_only: bool,
) -> String {
    if !enabled(db) {
        return String::new();
    }
    let ch = match db.get_character_data(player_name) {
        Ok(Some(c)) => c,
        _ => return String::new(),
    };
    let radius = radius.unwrap_or(DEFAULT_RADIUS);
    let layout = compute_map_layout(db, ch.current_room_id, radius, &ch.rooms_visited);
    if layout.cells.is_empty() {
        return String::new();
    }
    render_map(&layout, show_legend, colors_enabled, ascii_only)
}

pub fn register(engine: &mut Engine, db: Arc<Db>, _connections: SharedConnections) {
    // map_enabled() -> bool
    {
        let db = db.clone();
        engine.register_fn("map_enabled", move || -> bool { enabled(&db) });
    }

    // render_map_for(player_name, radius) -> String
    {
        let db = db.clone();
        engine.register_fn("render_map_for", move |player_name: String, radius: i64| -> String {
            render_map_for_player(&db, &player_name.to_lowercase(), Some(radius as i32))
        });
    }

    // render_map_for_session(connection_id, radius) -> String
    //
    // Session-aware variant: legend is shown only on the first render of a
    // session (suppressed thereafter), and the `?` collision glyph is dimmed
    // when the session has colors enabled. After rendering, flips
    // `map_legend_shown` so subsequent renders elide the legend.
    {
        let db = db.clone();
        let connections = _connections.clone();
        engine.register_fn(
            "render_map_for_session",
            move |connection_id: String, radius: i64| -> String {
                let conn_uuid = match Uuid::parse_str(&connection_id) {
                    Ok(u) => u,
                    Err(_) => return String::new(),
                };
                let (player_name, show_legend, colors_enabled, ascii_only) = {
                    let conns = match connections.lock() {
                        Ok(g) => g,
                        Err(_) => return String::new(),
                    };
                    match conns.get(&conn_uuid) {
                        Some(s) => {
                            let (name, ascii_only) = match s.character.as_ref() {
                                Some(c) => (c.name.clone(), c.ascii_map),
                                None => return String::new(),
                            };
                            (name, !s.map_legend_shown, s.colors_enabled, ascii_only)
                        }
                        None => return String::new(),
                    }
                };
                let rendered = render_map_for_player_with_options(
                    &db,
                    &player_name.to_lowercase(),
                    Some(radius as i32),
                    show_legend,
                    colors_enabled,
                    ascii_only,
                );
                if !rendered.is_empty() && show_legend {
                    if let Ok(mut conns) = connections.lock() {
                        if let Some(session) = conns.get_mut(&conn_uuid) {
                            session.map_legend_shown = true;
                        }
                    }
                }
                rendered
            },
        );
    }

    // render_map_for_default(player_name) -> String (default radius)
    {
        let db = db.clone();
        engine.register_fn("render_map_for_default", move |player_name: String| -> String {
            render_map_for_player(&db, &player_name.to_lowercase(), None)
        });
    }

    // mark_room_visited(player_name, room_id) -> bool (true if newly inserted)
    {
        let db = db.clone();
        engine.register_fn(
            "mark_room_visited",
            move |player_name: String, room_id: String| -> bool {
                let rid = match Uuid::parse_str(&room_id) {
                    Ok(u) => u,
                    Err(_) => return false,
                };
                let mut ch = match db.get_character_data(&player_name.to_lowercase()) {
                    Ok(Some(c)) => c,
                    _ => return false,
                };
                if ch.rooms_visited.insert(rid) {
                    let _ = db.save_character_data(ch);
                    true
                } else {
                    false
                }
            },
        );
    }

    // is_automap_enabled(connection_id) -> bool
    {
        let connections = _connections.clone();
        engine.register_fn("is_automap_enabled", move |connection_id: String| -> bool {
            let conn_uuid = match Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let conns = match connections.lock() {
                Ok(g) => g,
                Err(_) => return false,
            };
            conns
                .get(&conn_uuid)
                .and_then(|s| s.character.as_ref().map(|c| c.automap_enabled))
                .unwrap_or(false)
        });
    }

    // set_automap_enabled_for(connection_id, on) -> bool (success)
    {
        let db = db.clone();
        let connections = _connections.clone();
        engine.register_fn(
            "set_automap_enabled_for",
            move |connection_id: String, value: bool| -> bool {
                let conn_uuid = match Uuid::parse_str(&connection_id) {
                    Ok(u) => u,
                    Err(_) => return false,
                };
                let player_name = {
                    let conns = match connections.lock() {
                        Ok(g) => g,
                        Err(_) => return false,
                    };
                    match conns.get(&conn_uuid).and_then(|s| s.character.as_ref().map(|c| c.name.clone())) {
                        Some(n) => n,
                        None => return false,
                    }
                };
                let mut ch = match db.get_character_data(&player_name) {
                    Ok(Some(c)) => c,
                    _ => return false,
                };
                ch.automap_enabled = value;
                if db.save_character_data(ch.clone()).is_err() {
                    return false;
                }
                if let Ok(mut conns) = connections.lock() {
                    if let Some(session) = conns.get_mut(&conn_uuid) {
                        session.character = Some(ch);
                    }
                }
                true
            },
        );
    }

    // get_automap_radius_for(connection_id) -> i64 (defaults to AUTOMAP_DEFAULT_RADIUS)
    {
        let connections = _connections.clone();
        engine.register_fn("get_automap_radius_for", move |connection_id: String| -> i64 {
            let conn_uuid = match Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return AUTOMAP_DEFAULT_RADIUS as i64,
            };
            let conns = match connections.lock() {
                Ok(g) => g,
                Err(_) => return AUTOMAP_DEFAULT_RADIUS as i64,
            };
            conns
                .get(&conn_uuid)
                .and_then(|s| s.character.as_ref().map(|c| c.automap_radius as i64))
                .unwrap_or(AUTOMAP_DEFAULT_RADIUS as i64)
        });
    }

    // set_automap_radius_for(connection_id, radius) -> bool (success).
    // Clamps to [1, 8].
    {
        let db = db.clone();
        let connections = _connections.clone();
        engine.register_fn(
            "set_automap_radius_for",
            move |connection_id: String, radius: i64| -> bool {
                let conn_uuid = match Uuid::parse_str(&connection_id) {
                    Ok(u) => u,
                    Err(_) => return false,
                };
                let player_name = {
                    let conns = match connections.lock() {
                        Ok(g) => g,
                        Err(_) => return false,
                    };
                    match conns.get(&conn_uuid).and_then(|s| s.character.as_ref().map(|c| c.name.clone())) {
                        Some(n) => n,
                        None => return false,
                    }
                };
                let clamped = radius.clamp(MIN_RADIUS as i64, MAX_RADIUS as i64) as i32;
                let mut ch = match db.get_character_data(&player_name) {
                    Ok(Some(c)) => c,
                    _ => return false,
                };
                ch.automap_radius = clamped;
                if db.save_character_data(ch.clone()).is_err() {
                    return false;
                }
                if let Ok(mut conns) = connections.lock() {
                    if let Some(session) = conns.get_mut(&conn_uuid) {
                        session.character = Some(ch);
                    }
                }
                true
            },
        );
    }

    // is_ascii_map_for(connection_id) -> bool
    {
        let connections = _connections.clone();
        engine.register_fn("is_ascii_map_for", move |connection_id: String| -> bool {
            let conn_uuid = match Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let conns = match connections.lock() {
                Ok(g) => g,
                Err(_) => return false,
            };
            conns
                .get(&conn_uuid)
                .and_then(|s| s.character.as_ref().map(|c| c.ascii_map))
                .unwrap_or(false)
        });
    }

    // set_ascii_map_for(connection_id, on) -> bool (success)
    {
        let db = db.clone();
        let connections = _connections.clone();
        engine.register_fn(
            "set_ascii_map_for",
            move |connection_id: String, value: bool| -> bool {
                let conn_uuid = match Uuid::parse_str(&connection_id) {
                    Ok(u) => u,
                    Err(_) => return false,
                };
                let player_name = {
                    let conns = match connections.lock() {
                        Ok(g) => g,
                        Err(_) => return false,
                    };
                    match conns.get(&conn_uuid).and_then(|s| s.character.as_ref().map(|c| c.name.clone())) {
                        Some(n) => n,
                        None => return false,
                    }
                };
                let mut ch = match db.get_character_data(&player_name) {
                    Ok(Some(c)) => c,
                    _ => return false,
                };
                ch.ascii_map = value;
                if db.save_character_data(ch.clone()).is_err() {
                    return false;
                }
                if let Ok(mut conns) = connections.lock() {
                    if let Some(session) = conns.get_mut(&conn_uuid) {
                        session.character = Some(ch);
                    }
                }
                true
            },
        );
    }
}

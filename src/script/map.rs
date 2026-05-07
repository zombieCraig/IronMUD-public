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
    /// Directions toward Visited neighbors that share a closed door with us.
    pub closed_door_dirs: HashSet<Direction>,
    /// Directions where the exit leaves the current area entirely.
    pub cross_area_dirs: HashSet<Direction>,
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

    let mut cells: HashMap<(i32, i32), MapCell> = HashMap::new();

    let origin_coord = (0_i32, 0_i32);
    cells.insert(origin_coord, build_cell(&origin_room, Visibility::Visited, true));

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
                }
                continue;
            }

            let target_visibility = if neighbor_visited_or_origin {
                Visibility::Visited
            } else {
                Visibility::Glimpsed
            };

            cells.insert(new_coord, build_cell(&neighbor_room, target_visibility, false));

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

fn build_cell(room: &crate::RoomData, visibility: Visibility, is_origin: bool) -> MapCell {
    MapCell {
        room_id: room.id,
        visibility,
        is_origin,
        has_up: room.exits.up.is_some(),
        has_down: room.exits.down.is_some(),
        has_water: !matches!(room.water_type, crate::WaterType::None),
        has_trap: !room.traps.is_empty(),
        closed_door_dirs: HashSet::new(),
        cross_area_dirs: HashSet::new(),
        collision: false,
    }
}

pub fn render_map(layout: &MapLayout) -> String {
    let r = layout.radius;
    let grid_n = (2 * r + 1) as usize;
    // Each cell is 3 chars wide; horizontal connector takes 1 char between cells.
    let total_w = grid_n * 3 + (grid_n.saturating_sub(1));
    // Each cell is 1 line; vertical connector takes 1 line between cell rows.
    let total_h = grid_n + grid_n.saturating_sub(1);

    let mut grid: Vec<Vec<char>> = vec![vec![' '; total_w]; total_h];

    for cy in -r..=r {
        for cx in -r..=r {
            let coord = (cx, cy);
            let cell = layout.cells.get(&coord);

            let row = ((cy + r) * 2) as usize;
            let col_start = ((cx + r) * 4) as usize;

            let glyphs = render_cell_glyphs(cell);
            for (i, ch) in glyphs.chars().enumerate() {
                if col_start + i < total_w {
                    grid[row][col_start + i] = ch;
                }
            }

            // Connectors: drawn between visible cells.
            if let Some(c) = cell {
                if let Some(east) = layout.cells.get(&(cx + 1, cy)) {
                    let conn_col = col_start + 3;
                    if conn_col < total_w {
                        let ch = if c.closed_door_dirs.contains(&Direction::E)
                            || east.closed_door_dirs.contains(&Direction::W)
                        {
                            '+'
                        } else {
                            '-'
                        };
                        grid[row][conn_col] = ch;
                    }
                }
                if let Some(south) = layout.cells.get(&(cx, cy + 1)) {
                    let conn_row = row + 1;
                    let conn_col = col_start + 1;
                    if conn_row < total_h && conn_col < total_w {
                        let ch = if c.closed_door_dirs.contains(&Direction::S)
                            || south.closed_door_dirs.contains(&Direction::N)
                        {
                            '+'
                        } else {
                            '|'
                        };
                        grid[conn_row][conn_col] = ch;
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
                let col_start = ((cx + r) * 4) as usize;
                for d in c.cross_area_dirs.iter().copied() {
                    match d {
                        Direction::N => {
                            if row > 0 {
                                grid[row - 1][col_start + 1] = d.arrow();
                            }
                        }
                        Direction::S => {
                            if row + 1 < total_h {
                                grid[row + 1][col_start + 1] = d.arrow();
                            }
                        }
                        Direction::E => {
                            let cc = col_start + 3;
                            if cc < total_w {
                                grid[row][cc] = d.arrow();
                            }
                        }
                        Direction::W => {
                            if col_start > 0 {
                                grid[row][col_start - 1] = d.arrow();
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
    for row in grid {
        let line: String = row.into_iter().collect();
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

    output.push_str("Legend: [@] you  o room  ~ water  ! trap  + door  ? unmapped\n");
    output
}

fn render_cell_glyphs(cell: Option<&MapCell>) -> &'static str {
    match cell {
        None => " * ",
        Some(c) if c.collision => " ? ",
        Some(c) if c.is_origin => "[@]",
        Some(c) if c.visibility == Visibility::Glimpsed => " o ",
        Some(c) if c.has_trap => " ! ",
        Some(c) if c.has_water => " ~ ",
        Some(_) => " o ",
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
pub fn render_map_for_player(db: &Db, player_name: &str, radius: Option<i32>) -> String {
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
    render_map(&layout)
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
}

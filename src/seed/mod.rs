//! Demo world seeding module
//!
//! Seeds a medieval fantasy demo world on first startup. Uses deterministic
//! UUIDs (v5) so rooms can reference each other before they exist in the DB.

use anyhow::Result;
use uuid::Uuid;

use crate::db::Db;

mod areas;
mod items;
mod mobiles;
mod plants;
mod properties;
mod recipes;
mod rooms;
mod spawn_points;
mod transports;

/// Namespace UUID for deterministic seed generation.
/// All seeded entities derive their UUID from this namespace + their vnum string.
const SEED_NAMESPACE: Uuid = Uuid::from_bytes([
    0x49, 0x72, 0x6f, 0x6e, // "Iron"
    0x4d, 0x55, 0x44, 0x53, // "MUDS"
    0x65, 0x65, 0x64, 0x4e, // "eedN"
    0x61, 0x6d, 0x65, 0x21, // "ame!"
]);

/// Generate a deterministic UUID from a vnum string.
/// The same vnum always produces the same UUID, allowing cross-references
/// between entities before they exist in the database.
pub fn seed_uuid(vnum: &str) -> Uuid {
    Uuid::new_v5(&SEED_NAMESPACE, vnum.as_bytes())
}

/// Seed the demo world if it doesn't already exist.
///
/// Checks for the existence of the "oakvale" area. If absent, seeds all
/// demo world content: areas, rooms, items, mobiles, spawn points,
/// recipes, plants, transports, and properties.
///
/// Returns `true` if the world was seeded, `false` if it already existed.
pub fn seed_demo_world(db: &Db) -> Result<bool> {
    // Skip seeding if the database already has any world content.
    // This prevents dumping demo data on top of an existing world.
    let stats = db.world_stats()?;
    if stats.areas > 0 || stats.rooms > 0 {
        return Ok(false);
    }

    tracing::info!("Empty world detected — seeding demo world...");

    // Seed in dependency order
    areas::seed_areas(db)?;
    rooms::seed_rooms(db)?;
    items::seed_items(db)?;
    mobiles::seed_mobiles(db)?;
    spawn_points::seed_spawn_points(db)?;
    recipes::seed_recipes(db)?;
    plants::seed_plants(db)?;
    transports::seed_transports(db)?;
    properties::seed_properties(db)?;

    tracing::info!("Demo world seeded successfully!");
    Ok(true)
}

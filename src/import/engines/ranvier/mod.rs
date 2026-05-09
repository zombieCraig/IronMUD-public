//! Ranvier (Node.js MUD engine) bundle importer.
//!
//! Ranvier authors content as YAML under
//! `<bundle>/areas/<area>/{manifest,rooms,npcs,items,quests,loot-pools}.yml`.
//! IDs are area-scoped strings (`limbo:rat`); we synthesize stable numeric
//! vnums per `(area, kind, id)` and persist the assignment to a sidecar
//! JSON so re-imports are idempotent.
//!
//! JS scripts and behavior modules under `<area>/scripts/` are skipped with
//! warnings (parallel to how the CircleMUD importer warns on opcodes it
//! can't translate). The static YAML structure is the contract.

use std::path::Path;

use anyhow::Result;

use crate::import::{Plan, Warning};

pub mod ir;
pub mod mapping;
pub mod parser;
pub mod post;
pub mod vnum_map;

/// Result of an in-memory bundle import: the plan to feed to the existing
/// writer, the accumulated warnings, and a post-apply patch list for fields
/// (`RoomData.coordinates`, `SpawnPointData.replace_on_respawn`) that
/// don't live on the CircleMUD-shaped [`Plan`].
pub struct RanvierImport {
    pub plan: Plan,
    pub warnings: Vec<Warning>,
    pub post_patches: post::PostPatches,
}

/// Top-level entry point invoked from the CLI. Parses every area in the
/// bundle, synthesizes vnums (loading and updating the sidecar map for
/// idempotency), and produces a [`Plan`] plus warnings ready for the
/// existing writer pipeline.
#[allow(clippy::too_many_arguments)]
pub fn import_bundle(
    source: &Path,
    bundle_name: &str,
    vnum_base: i32,
    quest_vnum_base: i32,
    existing_room_vnums: &[String],
    existing_mobile_vnums: &[String],
    existing_item_vnums: &[String],
    existing_area_prefixes: &[String],
) -> Result<RanvierImport> {
    let mut warnings = Vec::new();
    let bundle = parser::parse_bundle(source, &mut warnings)?;
    let mut vnum_map = vnum_map::VnumMap::load_for_bundle(bundle_name)?;
    let plan = mapping::bundle_to_plan(
        &bundle,
        bundle_name,
        vnum_base,
        quest_vnum_base,
        &mut vnum_map,
        existing_room_vnums,
        existing_mobile_vnums,
        existing_item_vnums,
        existing_area_prefixes,
        &mut warnings,
    );
    let post_patches = post::collect_post_patches(&bundle, &vnum_map);
    vnum_map.save_for_bundle(bundle_name)?;
    Ok(RanvierImport {
        plan,
        warnings,
        post_patches,
    })
}

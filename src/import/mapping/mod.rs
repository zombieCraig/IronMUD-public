//! Pure mapping layer: engine-neutral [`ImportIR`] → [`Plan`] of IronMUD writes.
//!
//! No I/O. The mapping table is loaded separately and passed in via
//! [`MappingOptions`] so this module is trivially unit-testable with synthetic
//! IR.

use std::collections::HashMap;

use anyhow::{Context, Result};
use serde::Deserialize;

use std::collections::HashSet;

use crate::import::{
    ImportIR, IrDgTrigger, MappingOptions, Plan, PlannedArea, PlannedExit, Severity, SourceLoc, Warning, WarningKind,
};
use crate::types::{ItemType, SpawnEntityType};

mod dg;
mod items;
mod mobs;
mod quests;
mod resets;
mod rooms;
mod shops;
mod triggers;

// Re-exports used by the orchestrator below and (for apply_named_room_flag) by writer.rs.
use dg::{item_index_for_dg, map_dg_triggers, mob_index_for_dg, room_index_for_dg};
use items::map_item;
use mobs::{map_mob, unique_prefix};
use quests::translate_quest;
use resets::{apply_door_override, deferred_to_warning, map_resets};
pub(crate) use rooms::apply_named_room_flag;
use rooms::map_room;
use shops::map_shop;
use triggers::map_triggers;

/// CircleMUD spell-number → IronMUD spell ID lookup, loaded lazily from
/// `scripts/data/import/circle_spell_mapping.json`. Used by ITEM_SCROLL /
/// WAND / STAFF / POTION routing to resolve Circle's numeric `v[1..3]` slots.
fn circle_spell_mapping() -> &'static HashMap<i32, String> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<HashMap<i32, String>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let path = "scripts/data/import/circle_spell_mapping.json";
        let raw = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => return HashMap::new(),
        };
        let parsed: HashMap<String, String> = serde_json::from_str(&raw).unwrap_or_default();
        parsed
            .into_iter()
            .filter_map(|(k, v)| k.parse::<i32>().ok().map(|n| (n, v)))
            .collect()
    })
}

fn lookup_circle_spell(num: i32) -> Option<&'static str> {
    circle_spell_mapping().get(&num).map(|s| s.as_str())
}

/// CircleMUD-specific mapping table. Loaded from JSON, but defaults to a
/// hard-coded copy embedded in the binary so the importer works even when
/// the JSON file is missing (e.g. running outside the repo).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct CircleMappingTable {
    #[serde(default)]
    pub sector_to_flags: HashMap<String, SectorMapping>,
    #[serde(default)]
    pub room_flag_actions: HashMap<String, FlagAction>,
    #[serde(default)]
    pub mob_flag_actions: HashMap<String, FlagAction>,
    #[serde(default)]
    pub aff_flag_actions: HashMap<String, FlagAction>,
    /// CircleMUD ITEM_* extra-bit name → action. Item-only (the SetStat /
    /// SetArmorClass actions are rejected on rooms and mobs).
    #[serde(default)]
    pub extra_flag_actions: HashMap<String, FlagAction>,
    /// CircleMUD APPLY_* (object affect) location name → action. Drives the
    /// per-affect interpretation in `map_item`.
    #[serde(default)]
    pub apply_actions: HashMap<String, FlagAction>,
    /// CircleMUD shop `buy_types` token (FOOD, LIQ CONTAINER, ...) →
    /// action. `set_flag.ironmud_flag` carries an IronMUD `ItemType`
    /// display string (`food`, `weapon`, ...). Used by `map_shop`.
    #[serde(default)]
    pub buy_type_actions: HashMap<String, FlagAction>,
    /// CircleMUD specproc name (lowercase: `cityguard`, `puff`, `bank`, ...)
    /// → action. Drives `map_triggers`. Specprocs not listed here surface
    /// as a default `warn` so the importer never silently loses a binding.
    #[serde(default)]
    pub trigger_actions: HashMap<String, TriggerAction>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SectorMapping {
    #[serde(default)]
    pub set_flags: Vec<String>,
    #[serde(default)]
    pub info: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum FlagAction {
    /// Set a single IronMUD bool flag in `RoomFlags`.
    SetFlag { ironmud_flag: String },
    /// Set the room's combat zone (e.g. PEACEFUL → safe).
    SetCombatZone { value: String },
    /// Set a numeric stat bonus on an `ItemData` (APPLY_STR..APPLY_CHA).
    /// `ironmud_stat` is the snake-case field name on `ItemData`.
    /// Item-only: rejected on rooms / mobs.
    SetStat { ironmud_stat: String },
    /// Set `ItemData.armor_class` (APPLY_ARMOR). The mapping flips the sign
    /// because Circle's AC is *negative-is-better* and IronMUD's
    /// `armor_class` is positive damage reduction. Item-only.
    SetArmorClass {
        #[serde(default)]
        info: Option<String>,
    },
    /// Add to `ItemData.hit_bonus` (APPLY_HITROLL). Modifier copied as-is
    /// (positive = better to-hit). Item-only.
    SetHitBonus,
    /// Add to `ItemData.damage_bonus` (APPLY_DAMROLL). Modifier copied as-is
    /// (positive = bonus damage). Item-only.
    SetDamageBonus,
    /// Add to `ItemData.max_hp_bonus` (APPLY_MAXHIT). Modifier copied as-is.
    /// Item-only.
    SetMaxHpBonus,
    /// Add to `ItemData.max_mana_bonus` (APPLY_MAXMANA). Modifier copied as-is.
    /// Item-only.
    SetMaxManaBonus,
    /// Emit a warning (severity = Warn).
    Warn { message: String },
    /// Silently ignore this flag (e.g. CircleMUD runtime / editor flags).
    Drop {
        #[serde(default)]
        info: Option<String>,
    },
    /// Push a permanent `ActiveBuff` onto the imported mob's `active_buffs`.
    /// Used for AFF_* affects that translate to existing IronMUD buff effects
    /// (e.g. AFF_SANCTUARY → DamageReduction). Mob-only.
    AddBuff {
        effect_type: String,
        magnitude: i32,
        #[serde(default = "default_remaining_secs")]
        remaining_secs: i32,
        #[serde(default)]
        source: String,
    },
    /// Append a single `ItemAffect` to the imported item's `affects` Vec.
    /// `magnitude` is the literal default; `magnitude_from: "value"` reads the
    /// modifier value from the source apply entry; `magnitude_scale` multiplies
    /// the source value by a fixed coefficient (used for SAVING_* heuristics).
    /// Item-only.
    AddItemAffect {
        effect_type: String,
        #[serde(default)]
        magnitude: i32,
        #[serde(default)]
        magnitude_from: Option<String>,
        #[serde(default)]
        magnitude_scale: Option<i32>,
        #[serde(default)]
        damage_type: Option<String>,
        #[serde(default)]
        vs_effect: Option<String>,
    },
    /// Append multiple `ItemAffect` entries from one source apply (used by
    /// SAVING_BREATH which splits across fire/cold/lightning/acid). Item-only.
    AddItemAffectMulti { entries: Vec<ItemAffectEntry> },
}

#[derive(Debug, Clone, Deserialize)]
pub struct ItemAffectEntry {
    pub effect_type: String,
    #[serde(default)]
    pub magnitude: i32,
    #[serde(default)]
    pub magnitude_from: Option<String>,
    #[serde(default)]
    pub magnitude_scale: Option<i32>,
    #[serde(default)]
    pub damage_type: Option<String>,
    #[serde(default)]
    pub vs_effect: Option<String>,
}

fn default_remaining_secs() -> i32 {
    -1
}

/// Action keyed by lowercase CircleMUD specproc name in
/// `circle_trigger_mapping.json`. Each binding from `spec_assign.c` is
/// translated by looking the specproc up here.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum TriggerAction {
    /// Set a single bool field on `MobileFlags` by snake_case name.
    /// Rejected for OBJ/ROOM bindings (warn instead).
    SetMobFlag { ironmud_flag: String },
    /// Set multiple bool fields on `MobileFlags` (e.g. `snake` →
    /// `["aggressive", "poisonous"]`). Each flag is fanned out to a
    /// separate `TriggerMutation::SetMobFlag` overlay at planning time, so
    /// the writer pass treats them independently and composes cleanly.
    SetMobFlags { ironmud_flags: Vec<String> },
    /// Append a `MobileTrigger` to the mob's `triggers` Vec. `args` from
    /// `fallback_args` are used unless the parser captured something
    /// specific (e.g. puff()'s do_say quotes).
    AddMobTrigger {
        trigger_type: String,
        script_name: String,
        #[serde(default)]
        chance: Option<i32>,
        #[serde(default)]
        interval_secs: Option<i64>,
        #[serde(default)]
        fallback_args: Vec<String>,
    },
    AddItemTrigger {
        trigger_type: String,
        script_name: String,
        #[serde(default)]
        chance: Option<i32>,
        #[serde(default)]
        fallback_args: Vec<String>,
    },
    AddRoomTrigger {
        trigger_type: String,
        script_name: String,
        #[serde(default)]
        chance: Option<i32>,
        #[serde(default)]
        interval_secs: Option<i64>,
        #[serde(default)]
        fallback_args: Vec<String>,
    },
    /// Replace the mob's combat-spell list (and per-round cast chance) so
    /// the combat tick rolls a random spell each round instead of swinging.
    /// CircleMUD `magic_user` specproc analog. Rejected for OBJ/ROOM
    /// bindings (warn instead).
    SetMobCombatSpells {
        spells: Vec<String>,
        #[serde(default)]
        chance: Option<u8>,
    },
    /// Mob-attached binding that fans out into one room overlay per room
    /// the mob is M-reset into, each setting `flag` (snake_case) on
    /// `RoomFlags`. CircleMUD's `postmaster` specproc uses this to stamp
    /// `post_office` on the rooms where the postmaster mob spawns —
    /// IronMUD's mail system is room-keyed, not mob-keyed. Rejected for
    /// OBJ/ROOM bindings (warn instead). Emits a Warn if no zone reset
    /// places the mob in any room.
    SetRoomFlagOnMobSpawnRooms { flag: String },
    /// Surface as a Warn so a builder can re-author the behavior in Rhai.
    /// Multiple bindings of the same specproc collapse to one dedup line.
    Warn { message: String },
    /// Silently drop the binding.
    Drop {
        #[serde(default)]
        info: Option<String>,
    },
}

const DEFAULT_ROOM_MAPPING_JSON: &str = include_str!("../../../scripts/data/import/circle_room_mapping.json");
const DEFAULT_MOB_MAPPING_JSON: &str = include_str!("../../../scripts/data/import/circle_mob_mapping.json");
const DEFAULT_OBJ_MAPPING_JSON: &str = include_str!("../../../scripts/data/import/circle_obj_mapping.json");
const DEFAULT_SHOP_MAPPING_JSON: &str = include_str!("../../../scripts/data/import/circle_shop_mapping.json");
const DEFAULT_TRIGGER_MAPPING_JSON: &str = include_str!("../../../scripts/data/import/circle_trigger_mapping.json");

impl CircleMappingTable {
    pub fn load_default() -> Self {
        // The bundled JSON is checked at compile time via `include_str!` and
        // tested below; a runtime parse failure would mean we shipped a
        // broken default. Panic so it's caught in dev.
        let mut table: Self =
            serde_json::from_str(DEFAULT_ROOM_MAPPING_JSON).expect("bundled circle_room_mapping.json must parse");
        let mob: Self =
            serde_json::from_str(DEFAULT_MOB_MAPPING_JSON).expect("bundled circle_mob_mapping.json must parse");
        let obj: Self =
            serde_json::from_str(DEFAULT_OBJ_MAPPING_JSON).expect("bundled circle_obj_mapping.json must parse");
        let shop: Self =
            serde_json::from_str(DEFAULT_SHOP_MAPPING_JSON).expect("bundled circle_shop_mapping.json must parse");
        let trig: Self =
            serde_json::from_str(DEFAULT_TRIGGER_MAPPING_JSON).expect("bundled circle_trigger_mapping.json must parse");
        // Merge: room mapping JSON owns sector + room actions; mob mapping
        // JSON owns mob + aff actions; obj mapping JSON owns extra + apply
        // actions; shop mapping JSON owns buy_type actions; trigger mapping
        // JSON owns trigger_actions. Either file is allowed to define any
        // section so a `--mapping` override can fully replace defaults.
        table.mob_flag_actions = mob.mob_flag_actions;
        table.aff_flag_actions = mob.aff_flag_actions;
        table.extra_flag_actions = obj.extra_flag_actions;
        table.apply_actions = obj.apply_actions;
        table.buy_type_actions = shop.buy_type_actions;
        table.trigger_actions = trig.trigger_actions;
        table
    }

    pub fn load_from_path(path: &std::path::Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading mapping {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing mapping {}", path.display()))
    }
}

pub fn ir_to_plan(ir: &ImportIR, opts: &MappingOptions) -> (Plan, Vec<Warning>) {
    let mut plan = Plan::default();
    let mut warnings = Vec::new();
    // Track prefixes assigned within *this* import for in-import
    // disambiguation. Existing DB prefixes are *not* in this list — those
    // become Block warnings instead of being silently suffixed, so a
    // double-apply is loud rather than a stealth no-op.
    let mut prefix_in_use: Vec<String> = Vec::new();
    // Names of E-block attributes already warned about. Stock CircleMUD
    // mostly uses `BareHandAttack`; without dedup the report would carry
    // hundreds of identical per-mob lines.
    let mut seen_extra_attrs: HashSet<String> = HashSet::new();

    for zone in &ir.zones {
        if zone.rooms.is_empty()
            && zone.mobiles.is_empty()
            && zone.items.is_empty()
            && zone.resets.is_empty()
            && zone.deferred.is_empty()
        {
            continue;
        }
        let prefix = unique_prefix(&zone.name, zone.vnum, &prefix_in_use);
        prefix_in_use.push(prefix.clone());

        if opts.existing_area_prefixes.iter().any(|p| p == &prefix) {
            warnings.push(
                Warning::new(
                    WarningKind::PrefixCollision,
                    Severity::Block,
                    zone.source.clone(),
                    format!("area prefix '{prefix}' already exists in target DB"),
                )
                .with_suggestion("rename the source zone or remove the existing area before re-running --apply"),
            );
        }

        plan.areas.push(PlannedArea {
            source_vnum: zone.vnum,
            name: zone.name.trim().to_string(),
            prefix: prefix.clone(),
            description: zone
                .description
                .clone()
                .unwrap_or_else(|| format!("Imported from CircleMUD zone #{}", zone.vnum)),
        });

        // Surface deferred features (zone resets, etc.) as warnings.
        for d in &zone.deferred {
            warnings.push(deferred_to_warning(d));
        }

        for room in &zone.rooms {
            let (room_plan, room_warnings) = map_room(zone, &prefix, room, opts);
            warnings.extend(room_warnings);
            // Build exits from the room's parsed IrExit list.
            for ex in &room.exits {
                plan.exits.push(PlannedExit {
                    from_vnum: room_plan.vnum.clone(),
                    direction: ex.direction.clone(),
                    to_source_vnum: ex.to_room_vnum,
                });
            }
            // Vnum-collision check against existing IronMUD rooms.
            if opts.existing_room_vnums.iter().any(|v| v == &room_plan.vnum) {
                warnings.push(Warning::new(
                    WarningKind::DuplicateVnum,
                    Severity::Block,
                    room_plan.source.clone(),
                    format!("room vnum '{}' already exists in target DB", room_plan.vnum),
                ));
            }
            plan.rooms.push(room_plan);
        }

        for mob in &zone.mobiles {
            let (mob_plan, mob_warnings) = map_mob(zone, &prefix, mob, opts, &mut seen_extra_attrs);
            warnings.extend(mob_warnings);
            // Mobile vnum collision: Block, mirrors room behavior.
            if opts
                .existing_mobile_vnums
                .iter()
                .any(|v| v.eq_ignore_ascii_case(&mob_plan.vnum))
            {
                warnings.push(Warning::new(
                    WarningKind::DuplicateVnum,
                    Severity::Block,
                    mob_plan.source.clone(),
                    format!("mobile vnum '{}' already exists in target DB", mob_plan.vnum),
                ));
            }
            plan.mobiles.push(mob_plan);
        }

        for item in &zone.items {
            let (item_plan, item_warnings) = map_item(zone, &prefix, item, opts);
            warnings.extend(item_warnings);
            // Item vnum collision against an existing prototype: Block.
            if opts
                .existing_item_vnums
                .iter()
                .any(|v| v.eq_ignore_ascii_case(&item_plan.vnum))
            {
                warnings.push(Warning::new(
                    WarningKind::DuplicateVnum,
                    Severity::Block,
                    item_plan.source.clone(),
                    format!("item vnum '{}' already exists in target DB", item_plan.vnum),
                ));
            }
            plan.items.push(item_plan);
        }
    }

    // Shops cross-cut zones: a `.shp` file's keeper mob may live in a
    // different zone than the shop record itself. Build global vnum
    // indices from the per-zone passes above, then resolve shop overlays
    // in a second sweep across all zones.
    let mob_index: HashMap<i32, String> = plan.mobiles.iter().map(|m| (m.source_vnum, m.vnum.clone())).collect();
    let item_index: HashMap<i32, String> = plan.items.iter().map(|i| (i.source_vnum, i.vnum.clone())).collect();

    for zone in &ir.zones {
        for shop in &zone.shops {
            let (overlay, shop_warnings) = map_shop(shop, &mob_index, &item_index, opts);
            warnings.extend(shop_warnings);
            if let Some(o) = overlay {
                plan.shop_overlays.push(o);
            }
        }
    }

    // Zone reset commands → spawn points + door overrides. Walk each zone's
    // resets in source order; needs the prefix from PlannedArea + global mob
    // and item indices. Door overrides mutate already-planned rooms in place.
    let prefix_by_source_vnum: HashMap<i32, String> =
        plan.areas.iter().map(|a| (a.source_vnum, a.prefix.clone())).collect();
    // Item-vnum → ItemType for container detection (P chains onto Containers).
    let item_type_by_source_vnum: HashMap<i32, ItemType> =
        plan.items.iter().map(|p| (p.source_vnum, p.data.item_type)).collect();

    for zone in &ir.zones {
        if zone.resets.is_empty() {
            continue;
        }
        let Some(prefix) = prefix_by_source_vnum.get(&zone.vnum).cloned() else {
            continue;
        };
        let respawn_secs = zone.default_respawn_secs.unwrap_or(300);
        let (spawns, door_overrides, reset_warnings) = map_resets(
            zone,
            &prefix,
            respawn_secs,
            &mob_index,
            &item_index,
            &item_type_by_source_vnum,
        );
        warnings.extend(reset_warnings);
        plan.spawns.extend(spawns);
        // Apply D-reset door overrides to the already-planned rooms.
        for ov in door_overrides {
            apply_door_override(&mut plan.rooms, &prefix, &ov, &mut warnings);
        }
    }

    // Walk all planned spawns to derive per-prototype world caps from
    // accumulated max(circle_max). Circle's `max` on M/O is "stop reloading
    // when the world has N already" — world-wide, not per-spawn-point — so
    // we collapse all M/O occurrences of a vnum to the largest authored cap.
    apply_world_caps_from_spawns(&mut plan);

    // Specproc / trigger overlays. spec_assign.c is one tree-wide file, so
    // these live on `ir.triggers` (not per-zone). Resolve each binding via
    // the global mob/item/room indexes and emit a `PlannedTriggerOverlay`
    // (or a Warn).
    if !ir.triggers.is_empty() {
        let room_index: HashMap<i32, String> = plan.rooms.iter().map(|r| (r.source_vnum, r.vnum.clone())).collect();
        let (overlays, trig_warnings) =
            map_triggers(&ir.triggers, &mob_index, &item_index, &room_index, &plan.spawns, opts);
        plan.trigger_overlays.extend(overlays);
        warnings.extend(trig_warnings);
    }

    // tbaMUD DG Scripts: attach each trigger as a real trigger on the
    // host entity, with `dg_body = Some(body)` so the runtime DG
    // interpreter (`src/script/dg/`) handles fire dispatch. Triggers
    // whose flag letters don't (yet) map to a native IronMUD `TriggerType`
    // surface as a single Info warning each — they're parseable but won't
    // fire until we wire the corresponding hook.
    if !ir.dg_triggers.is_empty() {
        // Seed every parsed trigger into the runtime prototype registry,
        // regardless of whether any zone T-line attaches it. This is what
        // `attach <vnum> <target>` (DG statement and builder cmd) reads
        // from at runtime.
        for t in &ir.dg_triggers {
            let attach_kind = match t.attach_type_raw {
                0 => crate::types::DgAttachKind::Mob,
                1 => crate::types::DgAttachKind::Obj,
                2 => crate::types::DgAttachKind::Room,
                _ => continue,
            };
            plan.dg_trigger_protos.push(crate::types::DgTriggerProto {
                vnum: t.vnum.to_string(),
                name: t.name.clone(),
                attach_kind,
                flags: t.trigger_flags.clone(),
                numeric_arg: t.numeric_arg,
                arglist: t.arglist.clone(),
                body: t.body.clone(),
            });
        }

        // Static-analyze each body and emit one Info warning per trigger
        // summarising distinct issues. Catches commands/variables/eval that
        // the runtime would silently no-op on, so builders see them in the
        // import report instead of having to play through the world.
        for t in &ir.dg_triggers {
            if t.body.trim().is_empty() {
                continue;
            }
            let issues = crate::script::dg::analyze::analyze(&t.body);
            if issues.is_empty() {
                continue;
            }
            let summary = crate::script::dg::analyze::summarize(&issues);
            warnings.push(Warning::new(
                WarningKind::Info,
                Severity::Info,
                t.source.clone(),
                format!(
                    "DG Scripts trigger #{} ({}) has unsupported features: {}",
                    t.vnum, t.name, summary
                ),
            ));
        }

        let trig_index: HashMap<i32, &IrDgTrigger> = ir.dg_triggers.iter().map(|t| (t.vnum, t)).collect();
        let dg_overlays_and_warnings = map_dg_triggers(
            &trig_index,
            &ir.zones,
            &room_index_for_dg(&plan),
            &mob_index_for_dg(&plan),
            &item_index_for_dg(&plan),
        );
        plan.trigger_overlays.extend(dg_overlays_and_warnings.0);
        warnings.extend(dg_overlays_and_warnings.1);

        // Surface any defined-but-unattached triggers as a single Info note
        // so the audit trail captures them. (Stock tbaMUD has plenty of
        // these — guild guards, etc., that get attached via zone resets the
        // importer doesn't translate.)
        let attached: HashSet<i32> = ir
            .zones
            .iter()
            .flat_map(|z| {
                z.rooms
                    .iter()
                    .flat_map(|r| r.trigger_vnums.iter().copied())
                    .chain(z.mobiles.iter().flat_map(|m| m.trigger_vnums.iter().copied()))
                    .chain(z.items.iter().flat_map(|i| i.trigger_vnums.iter().copied()))
            })
            .collect();
        let unattached = ir.dg_triggers.iter().filter(|t| !attached.contains(&t.vnum)).count();
        if unattached > 0 {
            warnings.push(Warning::new(
                WarningKind::Info,
                Severity::Info,
                SourceLoc::default(),
                format!(
                    "{unattached} DG Scripts trigger(s) defined but not attached to any room/mob/obj in the imported set"
                ),
            ));
        }
    }

    // Quests: translate `.qst` body fields into QuestData. Supported AQ_*
    // types translate cleanly; MOB_FIND / MOB_SAVE / ROOM_CLEAR stay as
    // warnings until those listeners exist.
    for q in &ir.quests {
        if let Some(planned) = translate_quest(q, &mut warnings) {
            plan.quests.push(planned);
        }
    }

    (plan, warnings)
}

/// Walk all PlannedSpawns, accumulate the largest authored CircleMUD `max`
/// per resolved vnum, then apply that cap to the matching planned mob/item
/// prototype. `max == 1` becomes `flags.unique = true`; larger caps populate
/// `world_max_count`. Idempotent: running again with the same spawns has
/// the same effect.
fn apply_world_caps_from_spawns(plan: &mut Plan) {
    let mut mob_caps: HashMap<String, i32> = HashMap::new();
    let mut item_caps: HashMap<String, i32> = HashMap::new();
    for sp in &plan.spawns {
        let bucket = match sp.entity_type {
            SpawnEntityType::Mobile => &mut mob_caps,
            SpawnEntityType::Item => &mut item_caps,
        };
        let entry = bucket.entry(sp.vnum.clone()).or_insert(0);
        if sp.max_count > *entry {
            *entry = sp.max_count;
        }
    }
    for m in &mut plan.mobiles {
        if let Some(&cap) = mob_caps.get(&m.vnum) {
            if cap == 1 {
                m.flags.unique = true;
            } else if cap > 1 {
                m.world_max_count = Some(cap);
            }
        }
    }
    for it in &mut plan.items {
        if let Some(&cap) = item_caps.get(&it.vnum) {
            if cap == 1 {
                it.data.flags.unique = true;
            } else if cap > 1 {
                it.data.world_max_count = Some(cap);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::mobs::{dice_max, slug, unique_prefix};
    use super::shops::synthesize_shop_routine;
    use super::*;
    use crate::import::IrShop;
    use crate::types::ActivityState;

    #[test]
    fn default_mapping_parses() {
        let _ = CircleMappingTable::load_default();
    }

    #[test]
    fn slug_basic() {
        assert_eq!(slug("Haon-Dor, Dark Forest"), "haon_dor_dark_forest");
        assert_eq!(slug("  Mid-Gaard  "), "mid_gaard");
        assert_eq!(slug(""), "");
    }

    #[test]
    fn unique_prefix_disambiguates_with_vnum() {
        let taken = vec!["forest".to_string()];
        assert_eq!(unique_prefix("Forest", 12, &taken), "forest_12");
    }

    #[test]
    fn dice_max_parses_circle_dice() {
        assert_eq!(dice_max("5d10+550"), Some(600));
        assert_eq!(dice_max("1d1+30000"), Some(30001));
        assert_eq!(dice_max("2d6"), Some(12));
        assert_eq!(dice_max("4d6-3"), Some(21));
        assert_eq!(dice_max("garbage"), None);
    }

    fn shop_with_hours(open1: i32, close1: i32, open2: i32, close2: i32) -> IrShop {
        IrShop {
            vnum: 9000,
            keeper_vnum: 0,
            producing: Vec::new(),
            profit_buy: 1.0,
            profit_sell: 1.0,
            buy_types: Vec::new(),
            unknown_buy_types: Vec::new(),
            messages: Default::default(),
            temper: 0,
            bitvector: 0,
            with_who: 0,
            rooms: Vec::new(),
            open1,
            close1,
            open2,
            close2,
            source: SourceLoc::default(),
        }
    }

    #[test]
    fn synthesize_routine_default_hours_empty() {
        let mut warnings = Vec::new();
        let routine = synthesize_shop_routine(&shop_with_hours(0, 28, 0, 0), &mut warnings);
        assert!(routine.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn synthesize_routine_single_shift() {
        let mut warnings = Vec::new();
        let routine = synthesize_shop_routine(&shop_with_hours(8, 18, 0, 0), &mut warnings);
        assert_eq!(routine.len(), 2);
        assert_eq!(routine[0].start_hour, 8);
        assert_eq!(routine[0].activity, ActivityState::Working);
        assert_eq!(routine[1].start_hour, 18);
        assert_eq!(routine[1].activity, ActivityState::OffDuty);
        assert!(warnings.is_empty());
    }

    #[test]
    fn synthesize_routine_dual_shift() {
        let mut warnings = Vec::new();
        let routine = synthesize_shop_routine(&shop_with_hours(8, 12, 14, 20), &mut warnings);
        assert_eq!(routine.len(), 4);
        let pairs: Vec<(u8, ActivityState)> = routine.iter().map(|e| (e.start_hour, e.activity.clone())).collect();
        assert_eq!(
            pairs,
            vec![
                (8, ActivityState::Working),
                (12, ActivityState::OffDuty),
                (14, ActivityState::Working),
                (20, ActivityState::OffDuty),
            ]
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn synthesize_routine_wrap_around_normalizes_mod_24() {
        // open=22, close=30 means 22:00 through 6:00 next day.
        // After mod 24: Working@22, OffDuty@6 (sorted: OffDuty@6 first).
        let mut warnings = Vec::new();
        let routine = synthesize_shop_routine(&shop_with_hours(22, 30, 0, 0), &mut warnings);
        assert_eq!(routine.len(), 2);
        assert_eq!(routine[0].start_hour, 6);
        assert_eq!(routine[0].activity, ActivityState::OffDuty);
        assert_eq!(routine[1].start_hour, 22);
        assert_eq!(routine[1].activity, ActivityState::Working);
        assert!(warnings.is_empty());
    }

    #[test]
    fn synthesize_routine_degenerate_window_warns_and_drops() {
        // open1==close1 after mod 24 — entire window dropped, but the
        // second valid shift still produces entries.
        let mut warnings = Vec::new();
        let routine = synthesize_shop_routine(&shop_with_hours(10, 10, 14, 18), &mut warnings);
        assert_eq!(routine.len(), 2, "only the second shift survives");
        assert_eq!(routine[0].start_hour, 14);
        assert_eq!(routine[1].start_hour, 18);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("open==close"));
    }

    #[test]
    fn synthesize_routine_collapses_overlapping_boundary() {
        // shift1 closes at 14, shift2 opens at 14 — same start_hour
        // collides; we keep Working so the keeper stays on duty.
        let mut warnings = Vec::new();
        let routine = synthesize_shop_routine(&shop_with_hours(8, 14, 14, 20), &mut warnings);
        assert_eq!(routine.len(), 3);
        assert_eq!(routine[0].start_hour, 8);
        assert_eq!(routine[0].activity, ActivityState::Working);
        assert_eq!(routine[1].start_hour, 14);
        assert_eq!(routine[1].activity, ActivityState::Working);
        assert_eq!(routine[2].start_hour, 20);
        assert_eq!(routine[2].activity, ActivityState::OffDuty);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].severity, Severity::Info);
    }
}

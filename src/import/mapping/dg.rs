use std::collections::HashMap;

use crate::import::{
    AttachType, IrDgTrigger,
    IrZone, Plan, PlannedTriggerOverlay, Severity, SourceLoc,
    TriggerMutation, Warning, WarningKind,
};
use crate::types::{
    ItemTrigger, MobileTrigger, RoomTrigger,
};


/// Build (source-vnum → prefixed-vnum) indexes for the planned rooms /
/// mobs / items. Used to resolve T-line trigger attachments to their
/// IronMUD targets.
pub(super) fn room_index_for_dg(plan: &Plan) -> HashMap<i32, String> {
    plan.rooms.iter().map(|r| (r.source_vnum, r.vnum.clone())).collect()
}

pub(super) fn mob_index_for_dg(plan: &Plan) -> HashMap<i32, String> {
    plan.mobiles.iter().map(|m| (m.source_vnum, m.vnum.clone())).collect()
}

pub(super) fn item_index_for_dg(plan: &Plan) -> HashMap<i32, String> {
    plan.items.iter().map(|i| (i.source_vnum, i.vnum.clone())).collect()
}

/// Translate every (zone × T-line) DG trigger reference into one or more
/// [`PlannedTriggerOverlay`]s. Each emitted overlay carries the trigger
/// body in `dg_body` so the runtime interpreter dispatches it.
///
/// Returns `(overlays, warnings)`. The warnings are Info-severity notes
/// for triggers whose flag letters don't (yet) map to a native IronMUD
/// `TriggerType` (e.g. MTRIG_FIGHT, OTRIG_GIVE — Phase-2/3 wiring).
pub(super) fn map_dg_triggers(
    trig_index: &HashMap<i32, &IrDgTrigger>,
    zones: &[IrZone],
    room_index: &HashMap<i32, String>,
    mob_index: &HashMap<i32, String>,
    item_index: &HashMap<i32, String>,
) -> (Vec<PlannedTriggerOverlay>, Vec<Warning>) {
    use crate::import::engines::tba::trg_map;

    let mut overlays = Vec::new();
    let mut warnings = Vec::new();

    for zone in zones {
        for room in &zone.rooms {
            for tv in &room.trigger_vnums {
                let Some(t) = trig_index.get(tv) else { continue };
                let target = match room_index.get(&room.vnum) {
                    Some(v) => v.clone(),
                    None => continue,
                };
                let mapped = trg_map::room_trigger_types(&t.trigger_flags);
                if mapped.is_empty() {
                    warnings.push(unsupported_dg_warning(t, "room", room.vnum, &room.source));
                    continue;
                }
                for ttype in mapped {
                    overlays.push(PlannedTriggerOverlay {
                        attach_type: AttachType::Room,
                        target_vnum: target.clone(),
                        specproc_name: format!("dg_trigger_{}", t.vnum),
                        mutation: TriggerMutation::AddRoomTrigger(RoomTrigger {
                            trigger_type: ttype,
                            script_name: String::new(),
                            enabled: true,
                            interval_secs: 60,
                            last_fired: 0,
                            chance: t.numeric_arg.clamp(1, 100),
                            args: Vec::new(),
                            dg_body: Some(t.body.clone()),
                            dg_name: Some(t.name.clone()),
                        }),
                        source: t.source.clone(),
                    });
                }
            }
        }

        for mob in &zone.mobiles {
            for tv in &mob.trigger_vnums {
                let Some(t) = trig_index.get(tv) else { continue };
                let target = match mob_index.get(&mob.vnum) {
                    Some(v) => v.clone(),
                    None => continue,
                };
                let mapped = trg_map::mobile_trigger_types(&t.trigger_flags);
                if mapped.is_empty() {
                    warnings.push(unsupported_dg_warning(t, "mob", mob.vnum, &mob.source));
                    continue;
                }
                for ttype in mapped {
                    overlays.push(PlannedTriggerOverlay {
                        attach_type: AttachType::Mob,
                        target_vnum: target.clone(),
                        specproc_name: format!("dg_trigger_{}", t.vnum),
                        mutation: TriggerMutation::AddMobTrigger(MobileTrigger {
                            trigger_type: ttype,
                            script_name: String::new(),
                            enabled: true,
                            chance: t.numeric_arg.clamp(1, 100),
                            args: Vec::new(),
                            interval_secs: 60,
                            last_fired: 0,
                            dg_body: Some(t.body.clone()),
                            dg_name: Some(t.name.clone()),
                        }),
                        source: t.source.clone(),
                    });
                }
            }
        }

        for item in &zone.items {
            for tv in &item.trigger_vnums {
                let Some(t) = trig_index.get(tv) else { continue };
                let target = match item_index.get(&item.vnum) {
                    Some(v) => v.clone(),
                    None => continue,
                };
                let mapped = trg_map::item_trigger_types(&t.trigger_flags);
                if mapped.is_empty() {
                    warnings.push(unsupported_dg_warning(t, "obj", item.vnum, &item.source));
                    continue;
                }
                for ttype in mapped {
                    overlays.push(PlannedTriggerOverlay {
                        attach_type: AttachType::Obj,
                        target_vnum: target.clone(),
                        specproc_name: format!("dg_trigger_{}", t.vnum),
                        mutation: TriggerMutation::AddItemTrigger(ItemTrigger {
                            trigger_type: ttype,
                            script_name: String::new(),
                            enabled: true,
                            chance: t.numeric_arg.clamp(1, 100),
                            args: Vec::new(),
                            dg_body: Some(t.body.clone()),
                            dg_name: Some(t.name.clone()),
                        }),
                        source: t.source.clone(),
                    });
                }
            }
        }
    }

    (overlays, warnings)
}

pub(super) fn unsupported_dg_warning(t: &IrDgTrigger, host_kind: &str, host_vnum: i32, source: &SourceLoc) -> Warning {
    Warning::new(
        WarningKind::Info,
        Severity::Info,
        source.clone(),
        format!(
            "DG Scripts trigger #{} ({}) attached to {} #{}: flag(s) `{}` not yet wired in IronMUD; body imported but trigger will not fire",
            t.vnum, t.name, host_kind, host_vnum, t.trigger_flags
        ),
    )
}

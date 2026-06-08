use std::collections::HashMap;

use crate::import::{
    AttachType, IrTrigger, MappingOptions, PlannedSpawn, PlannedTriggerOverlay, Severity, SourceLoc, TriggerMutation,
    Warning, WarningKind,
};
use crate::types::{
    ItemTrigger, ItemTriggerType, MobileTrigger, MobileTriggerType, RoomTrigger, SpawnEntityType, TriggerType,
};

use super::TriggerAction;

/// Translate `IrTrigger`s (CircleMUD specproc bindings) into
/// [`PlannedTriggerOverlay`]s + warnings. Each binding is looked up in
/// `opts.circle.trigger_actions`; bindings with no entry default to a
/// Warn so the importer never silently loses information.
///
/// To keep the report readable on real Circle imports (`magic_user` is
/// bound to 93 separate mobs in stock 3.1), `Warn`-action specprocs that
/// target ≥3 distinct vnums collapse to a single dedup line listing the
/// count + first 8 vnums. Resolution failures (vnum not in the import
/// set) collapse the same way.
pub(super) fn map_triggers(
    triggers: &[IrTrigger],
    mob_index: &HashMap<i32, String>,
    item_index: &HashMap<i32, String>,
    room_index: &HashMap<i32, String>,
    spawns: &[PlannedSpawn],
    opts: &MappingOptions,
) -> (Vec<PlannedTriggerOverlay>, Vec<Warning>) {
    let mut overlays: Vec<PlannedTriggerOverlay> = Vec::new();
    let mut warnings: Vec<Warning> = Vec::new();
    // For Warn-action collapse: specproc → (sample_message, source, vnums)
    let mut warn_buckets: HashMap<String, (String, SourceLoc, Vec<i32>)> = HashMap::new();
    // For "vnum not in import set" collapse: specproc → vnums
    let mut orphan_buckets: HashMap<String, (SourceLoc, Vec<i32>)> = HashMap::new();
    // Track (attach_type, target_vnum) → most recent overlay index so a
    // duplicate ASSIGN line for the same vnum overrides the prior one
    // (matches CircleMUD runtime behavior — last assignment wins).
    let mut last_overlay_for: HashMap<(AttachType, String), usize> = HashMap::new();

    for trig in triggers {
        let action = opts.circle.trigger_actions.get(&trig.specproc_name);
        // Resolve the target vnum first — if it's missing from the import
        // set we drop with a per-specproc collapsed warning, regardless of
        // what the action would have been.
        let target_vnum: Option<String> = match trig.attach_type {
            AttachType::Mob => mob_index.get(&trig.source_vnum).cloned(),
            AttachType::Obj => item_index.get(&trig.source_vnum).cloned(),
            AttachType::Room => room_index.get(&trig.source_vnum).cloned(),
        };
        let Some(target_vnum) = target_vnum else {
            let bucket = orphan_buckets
                .entry(trig.specproc_name.clone())
                .or_insert_with(|| (trig.source.clone(), Vec::new()));
            bucket.1.push(trig.source_vnum);
            continue;
        };

        let mutation: Option<TriggerMutation> = match action {
            Some(TriggerAction::SetMobFlag { ironmud_flag }) => {
                if trig.attach_type != AttachType::Mob {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedFlag,
                        Severity::Warn,
                        trig.source.clone(),
                        format!(
                            "specproc `{}` mapping uses set_mob_flag but binding attaches to {:?}; ignored",
                            trig.specproc_name, trig.attach_type
                        ),
                    ));
                    None
                } else {
                    Some(TriggerMutation::SetMobFlag {
                        ironmud_flag: ironmud_flag.clone(),
                    })
                }
            }
            Some(TriggerAction::SetMobFlags { ironmud_flags }) => {
                if trig.attach_type != AttachType::Mob {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedFlag,
                        Severity::Warn,
                        trig.source.clone(),
                        format!(
                            "specproc `{}` mapping uses set_mob_flags but binding attaches to {:?}; ignored",
                            trig.specproc_name, trig.attach_type
                        ),
                    ));
                } else {
                    // Fan out: one SetMobFlag overlay per flag. Bypasses the
                    // post-match last_overlay_for dedup deliberately —
                    // multiple flag overlays for the same mob from a single
                    // specproc binding all need to land. Mirrors the
                    // SetRoomFlagOnMobSpawnRooms inline-push pattern below.
                    for flag in ironmud_flags {
                        overlays.push(PlannedTriggerOverlay {
                            attach_type: AttachType::Mob,
                            target_vnum: target_vnum.clone(),
                            specproc_name: trig.specproc_name.clone(),
                            mutation: TriggerMutation::SetMobFlag {
                                ironmud_flag: flag.clone(),
                            },
                            source: trig.source.clone(),
                        });
                    }
                }
                None
            }
            Some(TriggerAction::AddMobTrigger {
                trigger_type,
                script_name,
                chance,
                interval_secs,
                fallback_args,
            }) => {
                if trig.attach_type != AttachType::Mob {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedFlag,
                        Severity::Warn,
                        trig.source.clone(),
                        format!(
                            "specproc `{}` mapping uses add_mob_trigger but binding attaches to {:?}; ignored",
                            trig.specproc_name, trig.attach_type
                        ),
                    ));
                    None
                } else if let Some(tt) = parse_mob_trigger_type(trigger_type) {
                    let mut t = MobileTrigger::default();
                    t.trigger_type = tt;
                    t.script_name = script_name.clone();
                    t.enabled = true;
                    if let Some(c) = chance {
                        t.chance = (*c).clamp(1, 100);
                    }
                    if let Some(s) = interval_secs {
                        t.interval_secs = (*s).max(1);
                    }
                    t.args = if !trig.args.is_empty() {
                        trig.args.clone()
                    } else {
                        fallback_args.clone()
                    };
                    Some(TriggerMutation::AddMobTrigger(t))
                } else {
                    warnings.push(unknown_trigger_type_warning(
                        "mobile",
                        trigger_type,
                        &trig.specproc_name,
                        &trig.source,
                    ));
                    None
                }
            }
            Some(TriggerAction::AddItemTrigger {
                trigger_type,
                script_name,
                chance,
                fallback_args,
            }) => {
                if trig.attach_type != AttachType::Obj {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedFlag,
                        Severity::Warn,
                        trig.source.clone(),
                        format!(
                            "specproc `{}` mapping uses add_item_trigger but binding attaches to {:?}; ignored",
                            trig.specproc_name, trig.attach_type
                        ),
                    ));
                    None
                } else if let Some(tt) = parse_item_trigger_type(trigger_type) {
                    let mut t = ItemTrigger::default();
                    t.trigger_type = tt;
                    t.script_name = script_name.clone();
                    t.enabled = true;
                    if let Some(c) = chance {
                        t.chance = (*c).clamp(1, 100);
                    }
                    t.args = if !trig.args.is_empty() {
                        trig.args.clone()
                    } else {
                        fallback_args.clone()
                    };
                    Some(TriggerMutation::AddItemTrigger(t))
                } else {
                    warnings.push(unknown_trigger_type_warning(
                        "item",
                        trigger_type,
                        &trig.specproc_name,
                        &trig.source,
                    ));
                    None
                }
            }
            Some(TriggerAction::AddRoomTrigger {
                trigger_type,
                script_name,
                chance,
                interval_secs,
                fallback_args,
            }) => {
                if trig.attach_type != AttachType::Room {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedFlag,
                        Severity::Warn,
                        trig.source.clone(),
                        format!(
                            "specproc `{}` mapping uses add_room_trigger but binding attaches to {:?}; ignored",
                            trig.specproc_name, trig.attach_type
                        ),
                    ));
                    None
                } else if let Some(tt) = parse_room_trigger_type(trigger_type) {
                    let mut t = RoomTrigger::default();
                    t.trigger_type = tt;
                    t.script_name = script_name.clone();
                    t.enabled = true;
                    if let Some(c) = chance {
                        t.chance = (*c).clamp(1, 100);
                    }
                    if let Some(s) = interval_secs {
                        t.interval_secs = (*s).max(1);
                    }
                    t.args = if !trig.args.is_empty() {
                        trig.args.clone()
                    } else {
                        fallback_args.clone()
                    };
                    Some(TriggerMutation::AddRoomTrigger(t))
                } else {
                    warnings.push(unknown_trigger_type_warning(
                        "room",
                        trigger_type,
                        &trig.specproc_name,
                        &trig.source,
                    ));
                    None
                }
            }
            Some(TriggerAction::SetMobCombatSpells { spells, chance }) => {
                if trig.attach_type != AttachType::Mob {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedFlag,
                        Severity::Warn,
                        trig.source.clone(),
                        format!(
                            "specproc `{}` mapping uses set_mob_combat_spells but binding attaches to {:?}; ignored",
                            trig.specproc_name, trig.attach_type
                        ),
                    ));
                    None
                } else if spells.is_empty() {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedFlag,
                        Severity::Warn,
                        trig.source.clone(),
                        format!(
                            "specproc `{}` mapping has empty `spells` list; ignored",
                            trig.specproc_name
                        ),
                    ));
                    None
                } else {
                    Some(TriggerMutation::SetMobCombatSpells {
                        spells: spells.clone(),
                        chance: chance.unwrap_or(50).min(100),
                    })
                }
            }
            Some(TriggerAction::SetRoomFlagOnMobSpawnRooms { flag }) => {
                if trig.attach_type != AttachType::Mob {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedFlag,
                        Severity::Warn,
                        trig.source.clone(),
                        format!(
                            "specproc `{}` mapping uses set_room_flag_on_mob_spawn_rooms but binding attaches to {:?}; ignored",
                            trig.specproc_name, trig.attach_type
                        ),
                    ));
                } else {
                    let mut rooms: Vec<String> = spawns
                        .iter()
                        .filter(|s| s.entity_type == SpawnEntityType::Mobile && s.vnum == target_vnum)
                        .map(|s| s.room_vnum.clone())
                        .collect();
                    rooms.sort();
                    rooms.dedup();
                    if rooms.is_empty() {
                        warnings.push(Warning::new(
                            WarningKind::UnsupportedFlag,
                            Severity::Warn,
                            trig.source.clone(),
                            format!(
                                "specproc `{}` for mob `{}` requested room flag `{}` but no zone reset places this mob in a room; flag not applied",
                                trig.specproc_name, target_vnum, flag
                            ),
                        ));
                    } else {
                        for room_vnum in rooms {
                            overlays.push(PlannedTriggerOverlay {
                                attach_type: AttachType::Room,
                                target_vnum: room_vnum,
                                specproc_name: trig.specproc_name.clone(),
                                mutation: TriggerMutation::SetRoomFlag {
                                    ironmud_flag: flag.clone(),
                                },
                                source: trig.source.clone(),
                            });
                        }
                    }
                }
                None
            }
            Some(TriggerAction::Warn { message }) => {
                let bucket = warn_buckets
                    .entry(trig.specproc_name.clone())
                    .or_insert_with(|| (message.clone(), trig.source.clone(), Vec::new()));
                bucket.2.push(trig.source_vnum);
                None
            }
            Some(TriggerAction::Drop { .. }) => None,
            None => {
                let msg = format!(
                    "no mapping for specproc `{}` — binding dropped (add an entry to circle_trigger_mapping.json)",
                    trig.specproc_name
                );
                let bucket = warn_buckets
                    .entry(trig.specproc_name.clone())
                    .or_insert_with(|| (msg, trig.source.clone(), Vec::new()));
                bucket.2.push(trig.source_vnum);
                None
            }
        };

        if let Some(mutation) = mutation {
            // Last-assignment-wins: if a prior overlay targets the same
            // entity, replace it with this one and emit an Info note.
            let key = (trig.attach_type, target_vnum.clone());
            if let Some(&prior_idx) = last_overlay_for.get(&key) {
                warnings.push(Warning::new(
                    WarningKind::Info,
                    Severity::Info,
                    trig.source.clone(),
                    format!(
                        "duplicate specproc binding for vnum {} — `{}` overrides earlier `{}`",
                        target_vnum, trig.specproc_name, overlays[prior_idx].specproc_name
                    ),
                ));
                overlays[prior_idx] = PlannedTriggerOverlay {
                    attach_type: trig.attach_type,
                    target_vnum,
                    specproc_name: trig.specproc_name.clone(),
                    mutation,
                    source: trig.source.clone(),
                };
            } else {
                overlays.push(PlannedTriggerOverlay {
                    attach_type: trig.attach_type,
                    target_vnum: target_vnum.clone(),
                    specproc_name: trig.specproc_name.clone(),
                    mutation,
                    source: trig.source.clone(),
                });
                last_overlay_for.insert(key, overlays.len() - 1);
            }
        }
    }

    // Collapse warn buckets: per-specproc count + first-8 vnum sample.
    for (name, (message, source, vnums)) in warn_buckets {
        warnings.push(Warning::new(
            WarningKind::DeferredFeature,
            Severity::Warn,
            source,
            format_specproc_warn(&name, &message, &vnums),
        ));
    }
    // Collapse orphan-vnum buckets the same way.
    for (name, (source, vnums)) in orphan_buckets {
        warnings.push(Warning::new(
            WarningKind::DanglingExit,
            Severity::Warn,
            source,
            format!(
                "specproc `{}` bound to {} vnum(s) not in the import set — binding(s) dropped: {}",
                name,
                vnums.len(),
                format_vnum_sample(&vnums)
            ),
        ));
    }

    (overlays, warnings)
}

pub(super) fn format_specproc_warn(name: &str, message: &str, vnums: &[i32]) -> String {
    if vnums.len() <= 1 {
        format!(
            "specproc `{}` (vnum {}): {}",
            name,
            vnums.first().copied().unwrap_or(0),
            message
        )
    } else {
        format!(
            "specproc `{}` ({} bindings): {} — vnums: {}",
            name,
            vnums.len(),
            message,
            format_vnum_sample(vnums)
        )
    }
}

pub(super) fn format_vnum_sample(vnums: &[i32]) -> String {
    let take = vnums
        .iter()
        .take(8)
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    if vnums.len() > 8 {
        format!("{take}, … ({} more)", vnums.len() - 8)
    } else {
        take
    }
}

pub(super) fn unknown_trigger_type_warning(
    scope: &str,
    trigger_type: &str,
    specproc: &str,
    source: &SourceLoc,
) -> Warning {
    Warning::new(
        WarningKind::UnsupportedFlag,
        Severity::Warn,
        source.clone(),
        format!(
            "specproc `{}` mapping has unknown {} trigger_type {:?}; binding dropped",
            specproc, scope, trigger_type
        ),
    )
}

pub(super) fn parse_mob_trigger_type(s: &str) -> Option<MobileTriggerType> {
    serde_json::from_str::<MobileTriggerType>(&format!("\"{}\"", s.trim())).ok()
}

pub(super) fn parse_item_trigger_type(s: &str) -> Option<ItemTriggerType> {
    serde_json::from_str::<ItemTriggerType>(&format!("\"{}\"", s.trim())).ok()
}

pub(super) fn parse_room_trigger_type(s: &str) -> Option<TriggerType> {
    serde_json::from_str::<TriggerType>(&format!("\"{}\"", s.trim())).ok()
}

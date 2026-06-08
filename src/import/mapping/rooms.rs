use crate::import::{IrRoom, IrZone, MappingOptions, PlannedDoor, PlannedRoom, Severity, Warning, WarningKind};
use crate::types::{CombatZoneType, ExtraDesc, RoomFlags};

use super::FlagAction;

pub(super) fn map_room(
    zone: &IrZone,
    area_prefix: &str,
    room: &IrRoom,
    opts: &MappingOptions,
) -> (PlannedRoom, Vec<Warning>) {
    let mut warnings = Vec::new();
    let mut flags = RoomFlags::default();
    let _ = zone; // currently unused but kept in the signature for future per-zone overrides

    // Sector → flags via the mapping table.
    let sector_name = crate::import::engines::circle::flags::sector_name(room.sector);
    match opts.circle.sector_to_flags.get(&sector_name) {
        Some(m) => {
            apply_set_flags(&mut flags, &m.set_flags, room, &mut warnings);
            if let Some(info) = &m.info {
                warnings.push(Warning::new(
                    WarningKind::Info,
                    Severity::Info,
                    room.source.clone(),
                    format!("sector {sector_name}: {info}"),
                ));
            }
        }
        None => warnings.push(Warning::new(
            WarningKind::UnsupportedSector,
            Severity::Warn,
            room.source.clone(),
            format!(
                "unknown CircleMUD sector {sector_name} ({}); no flags applied",
                room.sector
            ),
        )),
    }

    // Decode room-flag bits and apply per-flag actions.
    let (known, unknown) = crate::import::engines::circle::flags::decode_room_flags(room.flag_bits);
    for flag in known {
        match opts.circle.room_flag_actions.get(flag) {
            Some(FlagAction::SetFlag { ironmud_flag }) => {
                if !apply_named_room_flag(&mut flags, ironmud_flag) {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedFlag,
                        Severity::Warn,
                        room.source.clone(),
                        format!("mapping points {flag} → {ironmud_flag}, but no such IronMUD RoomFlag"),
                    ));
                }
            }
            Some(FlagAction::SetCombatZone { value }) => match CombatZoneType::from_str(value) {
                Some(z) => flags.combat_zone = Some(z),
                None => warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    room.source.clone(),
                    format!("unknown combat_zone value {value:?} for flag {flag}"),
                )),
            },
            Some(FlagAction::Warn { message }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    room.source.clone(),
                    format!("ROOM_{flag}: {message}"),
                ));
            }
            Some(FlagAction::Drop { .. }) => {}
            Some(FlagAction::SetStat { .. })
            | Some(FlagAction::SetArmorClass { .. })
            | Some(FlagAction::SetHitBonus)
            | Some(FlagAction::SetDamageBonus)
            | Some(FlagAction::SetMaxHpBonus)
            | Some(FlagAction::SetMaxManaBonus)
            | Some(FlagAction::AddItemAffect { .. })
            | Some(FlagAction::AddItemAffectMulti { .. }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    room.source.clone(),
                    format!("mapping uses an item-only action for ROOM_{flag}; ignored on rooms"),
                ));
            }
            Some(FlagAction::AddBuff { .. }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    room.source.clone(),
                    format!("mapping uses add_buff for ROOM_{flag}; ignored (buffs apply to mobs only)"),
                ));
            }
            None => warnings.push(Warning::new(
                WarningKind::UnknownFlag,
                Severity::Warn,
                room.source.clone(),
                format!("no mapping for ROOM_{flag}"),
            )),
        }
    }
    for u in unknown {
        warnings.push(Warning::new(
            WarningKind::UnknownFlag,
            Severity::Warn,
            room.source.clone(),
            format!("unrecognised room flag bit {u} (likely a patched flag)"),
        ));
    }

    // Doors.
    let mut doors = Vec::new();
    for ex in &room.exits {
        let (door_known, door_unknown) = crate::import::engines::circle::flags::decode_exit_flags(ex.door_flags);
        let mut is_door = false;
        let mut is_closed = false;
        let mut is_locked = false;
        let mut pickproof = false;
        for f in &door_known {
            match *f {
                "ISDOOR" => is_door = true,
                "CLOSED" => is_closed = true,
                "LOCKED" => is_locked = true,
                "PICKPROOF" => pickproof = true,
                _ => {}
            }
        }
        for u in door_unknown {
            warnings.push(Warning::new(
                WarningKind::UnsupportedDoorFlag,
                Severity::Warn,
                room.source.clone(),
                format!("exit {} unknown door flag bit {u}", ex.direction),
            ));
        }
        if !is_door && !is_closed && !is_locked && !pickproof {
            continue;
        }
        let (name, keywords) = match &ex.keyword {
            Some(kw) => {
                let mut parts: Vec<String> = kw.split_whitespace().map(str::to_string).collect();
                let primary = parts.first().cloned().unwrap_or_else(|| "door".to_string());
                let extras = if parts.len() > 1 {
                    parts.split_off(1)
                } else {
                    Vec::new()
                };
                (primary, extras)
            }
            None => ("door".to_string(), Vec::new()),
        };
        doors.push(PlannedDoor {
            direction: ex.direction.clone(),
            name,
            keywords,
            description: ex.general_description.clone(),
            is_closed,
            is_locked,
            pickproof,
            key_source_vnum: ex.key_vnum,
        });
    }

    let extra_descs: Vec<ExtraDesc> = room
        .extras
        .iter()
        .map(|e| ExtraDesc {
            keywords: e.keywords.clone(),
            description: e.description.clone(),
        })
        .collect();

    let vnum = format!("{}_{}", area_prefix, room.vnum);
    let title = room.name.trim().to_string();
    let description = room.description.clone();

    (
        PlannedRoom {
            area_prefix: area_prefix.to_string(),
            source_vnum: room.vnum,
            vnum,
            title,
            description,
            flags,
            extra_descs,
            doors,
            source: room.source.clone(),
        },
        warnings,
    )
}

pub(super) fn apply_set_flags(flags: &mut RoomFlags, names: &[String], room: &IrRoom, warnings: &mut Vec<Warning>) {
    for name in names {
        if !apply_named_room_flag(flags, name) {
            warnings.push(Warning::new(
                WarningKind::UnsupportedFlag,
                Severity::Warn,
                room.source.clone(),
                format!("mapping references unknown IronMUD RoomFlag '{name}'"),
            ));
        }
    }
}

pub(crate) fn apply_named_room_flag(flags: &mut RoomFlags, name: &str) -> bool {
    match name {
        "dark" => flags.dark = true,
        "no_mob" => flags.no_mob = true,
        "indoors" => flags.indoors = true,
        "underwater" => flags.underwater = true,
        "climate_controlled" => flags.climate_controlled = true,
        "always_hot" => flags.always_hot = true,
        "always_cold" => flags.always_cold = true,
        "city" => flags.city = true,
        "no_windows" => flags.no_windows = true,
        "difficult_terrain" => flags.difficult_terrain = true,
        "dirt_floor" => flags.dirt_floor = true,
        "property_storage" => flags.property_storage = true,
        "post_office" => flags.post_office = true,
        "bank" => flags.bank = true,
        "garden" => flags.garden = true,
        "spawn_point" => flags.spawn_point = true,
        "shallow_water" => flags.shallow_water = true,
        "deep_water" => flags.deep_water = true,
        "liveable" => flags.liveable = true,
        "private" | "private_room" => flags.private_room = true,
        "tunnel" => flags.tunnel = true,
        "death" => flags.death = true,
        "no_magic" => flags.no_magic = true,
        "soundproof" => flags.soundproof = true,
        "notrack" | "no_track" => flags.notrack = true,
        "no_recall" | "norecall" => flags.no_recall = true,
        _ => return false,
    }
    true
}

// src/script/property.rs
// Property rental system functions for IronMUD

use rhai::Engine;
use std::sync::Arc;
use crate::db::Db;
use crate::{EscrowData, LeaseData, PartyAccessLevel, PropertyTemplate, RoomData, RoomExits, RoomFlags};
use crate::SharedConnections;

/// Get character name from connection ID
fn get_character_name_from_connection(conns: &SharedConnections, connection_id: &str) -> String {
    if let Ok(uuid) = uuid::Uuid::parse_str(connection_id) {
        let conns_guard = conns.lock().unwrap();
        if let Some(session) = conns_guard.get(&uuid) {
            if let Some(ref char) = session.character {
                return char.name.clone();
            }
        }
    }
    String::new()
}

/// Register property-related functions
pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections) {
    // ========== Property Template Registration ==========

    engine.register_type_with_name::<PropertyTemplate>("PropertyTemplate")
        .register_get("id", |t: &mut PropertyTemplate| t.id.to_string())
        .register_get("vnum", |t: &mut PropertyTemplate| t.vnum.clone())
        .register_set("vnum", |t: &mut PropertyTemplate, val: String| t.vnum = val)
        .register_get("name", |t: &mut PropertyTemplate| t.name.clone())
        .register_set("name", |t: &mut PropertyTemplate, val: String| t.name = val)
        .register_get("description", |t: &mut PropertyTemplate| t.description.clone())
        .register_set("description", |t: &mut PropertyTemplate, val: String| t.description = val)
        .register_get("monthly_rent", |t: &mut PropertyTemplate| t.monthly_rent as i64)
        .register_set("monthly_rent", |t: &mut PropertyTemplate, val: i64| t.monthly_rent = val as i32)
        .register_get("entrance_room_id", |t: &mut PropertyTemplate| t.entrance_room_id.to_string())
        .register_set("entrance_room_id", |t: &mut PropertyTemplate, val: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&val) {
                t.entrance_room_id = uuid;
            }
        })
        .register_get("max_instances", |t: &mut PropertyTemplate| t.max_instances as i64)
        .register_set("max_instances", |t: &mut PropertyTemplate, val: i64| t.max_instances = val as i32)
        .register_get("level_requirement", |t: &mut PropertyTemplate| t.level_requirement as i64)
        .register_set("level_requirement", |t: &mut PropertyTemplate, val: i64| t.level_requirement = val as i32)
        .register_get("area_id", |t: &mut PropertyTemplate| {
            t.area_id.map(|u| u.to_string()).unwrap_or_default()
        })
        .register_set("area_id", |t: &mut PropertyTemplate, val: String| {
            t.area_id = if val.is_empty() {
                None
            } else {
                uuid::Uuid::parse_str(&val).ok()
            };
        });

    // ========== LeaseData Registration ==========

    engine.register_type_with_name::<LeaseData>("LeaseData")
        .register_get("id", |l: &mut LeaseData| l.id.to_string())
        .register_get("template_vnum", |l: &mut LeaseData| l.template_vnum.clone())
        .register_get("owner_name", |l: &mut LeaseData| l.owner_name.clone())
        .register_get("leasing_agent_id", |l: &mut LeaseData| l.leasing_agent_id.to_string())
        .register_get("leasing_office_room_id", |l: &mut LeaseData| l.leasing_office_room_id.to_string())
        .register_get("area_id", |l: &mut LeaseData| l.area_id.to_string())
        .register_get("instanced_rooms", |l: &mut LeaseData| {
            l.instanced_rooms.iter().map(|u| rhai::Dynamic::from(u.to_string())).collect::<Vec<_>>()
        })
        .register_get("entrance_room_id", |l: &mut LeaseData| l.entrance_room_id.to_string())
        .register_get("monthly_rent", |l: &mut LeaseData| l.monthly_rent as i64)
        .register_get("rent_paid_until", |l: &mut LeaseData| l.rent_paid_until)
        .register_get("created_at", |l: &mut LeaseData| l.created_at)
        .register_get("is_evicted", |l: &mut LeaseData| l.is_evicted)
        .register_get("party_access", |l: &mut LeaseData| {
            l.party_access.to_display_string().to_string()
        })
        .register_get("trusted_visitors", |l: &mut LeaseData| {
            l.trusted_visitors.iter().map(|s| rhai::Dynamic::from(s.clone())).collect::<Vec<_>>()
        });

    // ========== EscrowData Registration ==========

    engine.register_type_with_name::<EscrowData>("EscrowData")
        .register_get("id", |e: &mut EscrowData| e.id.to_string())
        .register_get("owner_name", |e: &mut EscrowData| e.owner_name.clone())
        .register_get("items", |e: &mut EscrowData| {
            e.items.iter().map(|u| rhai::Dynamic::from(u.to_string())).collect::<Vec<_>>()
        })
        .register_get("source_lease_id", |e: &mut EscrowData| e.source_lease_id.to_string())
        .register_get("created_at", |e: &mut EscrowData| e.created_at)
        .register_get("expires_at", |e: &mut EscrowData| e.expires_at)
        .register_get("retrieval_fee", |e: &mut EscrowData| e.retrieval_fee as i64);

    // ========== Property Template Functions ==========

    // new_property_template(vnum) -> PropertyTemplate
    engine.register_fn("new_property_template", |vnum: String| {
        PropertyTemplate::new(vnum.clone(), vnum)
    });

    // get_property_template(id_or_vnum) -> PropertyTemplate | ()
    let cloned_db = db.clone();
    engine.register_fn("get_property_template", move |id_or_vnum: String| -> rhai::Dynamic {
        // Try as UUID first
        if let Ok(uuid) = uuid::Uuid::parse_str(&id_or_vnum) {
            if let Ok(Some(template)) = cloned_db.get_property_template(&uuid) {
                return rhai::Dynamic::from(template);
            }
        }
        // Try as vnum
        if let Ok(Some(template)) = cloned_db.get_property_template_by_vnum(&id_or_vnum) {
            return rhai::Dynamic::from(template);
        }
        rhai::Dynamic::UNIT
    });

    // save_property_template(template) -> bool
    let cloned_db = db.clone();
    engine.register_fn("save_property_template", move |template: PropertyTemplate| {
        match cloned_db.save_property_template(&template) {
            Ok(()) => true,
            Err(e) => {
                tracing::error!("Failed to save property template: {}", e);
                false
            }
        }
    });

    // delete_property_template(id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("delete_property_template", move |id: String| {
        let uuid = match uuid::Uuid::parse_str(&id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        // Delete all template rooms first
        if let Ok(rooms) = cloned_db.get_rooms_by_template_id(&uuid) {
            for room in rooms {
                if let Err(e) = cloned_db.delete_room(&room.id) {
                    tracing::error!("Failed to delete template room: {}", e);
                }
            }
        }

        match cloned_db.delete_property_template(&uuid) {
            Ok(deleted) => deleted,
            Err(e) => {
                tracing::error!("Failed to delete property template: {}", e);
                false
            }
        }
    });

    // list_property_templates() -> Array
    let cloned_db = db.clone();
    engine.register_fn("list_property_templates", move || -> Vec<rhai::Dynamic> {
        match cloned_db.list_all_property_templates() {
            Ok(templates) => templates.into_iter().map(rhai::Dynamic::from).collect(),
            Err(e) => {
                tracing::error!("Failed to list property templates: {}", e);
                Vec::new()
            }
        }
    });

    // get_template_rooms(template_id) -> Array
    let cloned_db = db.clone();
    engine.register_fn("get_template_rooms", move |template_id: String| -> Vec<rhai::Dynamic> {
        let uuid = match uuid::Uuid::parse_str(&template_id) {
            Ok(u) => u,
            Err(_) => return Vec::new(),
        };
        match cloned_db.get_rooms_by_template_id(&uuid) {
            Ok(rooms) => rooms.into_iter().map(rhai::Dynamic::from).collect(),
            Err(e) => {
                tracing::error!("Failed to get template rooms: {}", e);
                Vec::new()
            }
        }
    });

    // count_template_instances(template_vnum) -> i32
    let cloned_db = db.clone();
    engine.register_fn("count_template_instances", move |template_vnum: String| -> i64 {
        match cloned_db.count_template_instances(&template_vnum) {
            Ok(count) => count as i64,
            Err(e) => {
                tracing::error!("Failed to count template instances: {}", e);
                0
            }
        }
    });

    // ========== Leasing Agent Functions ==========

    // find_leasing_agent_in_room(room_id) -> MobileData | ()
    let cloned_db = db.clone();
    engine.register_fn("find_leasing_agent_in_room", move |room_id: String| -> rhai::Dynamic {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return rhai::Dynamic::UNIT,
        };

        if let Ok(mobiles) = cloned_db.list_all_mobiles() {
            for mobile in mobiles {
                if mobile.current_room_id == Some(room_uuid) && mobile.flags.leasing_agent {
                    return rhai::Dynamic::from(mobile);
                }
            }
        }
        rhai::Dynamic::UNIT
    });

    // add_agent_property_template(mobile_id, vnum) -> bool
    let cloned_db = db.clone();
    engine.register_fn("add_agent_property_template", move |mobile_id: String, vnum: String| {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
            if !mobile.property_templates.contains(&vnum) {
                mobile.property_templates.push(vnum);
                if let Err(e) = cloned_db.save_mobile_data(mobile.clone()) {
                    tracing::error!("Failed to save mobile: {}", e);
                    return false;
                }
            }
            true
        } else {
            false
        }
    });

    // remove_agent_property_template(mobile_id, vnum) -> bool
    let cloned_db = db.clone();
    engine.register_fn("remove_agent_property_template", move |mobile_id: String, vnum: String| {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
            let vnum_lower = vnum.to_lowercase();
            mobile.property_templates.retain(|t| t.to_lowercase() != vnum_lower);
            if let Err(e) = cloned_db.save_mobile_data(mobile.clone()) {
                tracing::error!("Failed to save mobile: {}", e);
                return false;
            }
            true
        } else {
            false
        }
    });

    // set_agent_leasing_area(mobile_id, area_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_agent_leasing_area", move |mobile_id: String, area_id: String| {
        let mobile_uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        let area_uuid = if area_id.is_empty() {
            None
        } else {
            match uuid::Uuid::parse_str(&area_id) {
                Ok(u) => Some(u),
                Err(_) => return false,
            }
        };

        if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&mobile_uuid) {
            mobile.leasing_area_id = area_uuid;
            if let Err(e) = cloned_db.save_mobile_data(mobile.clone()) {
                tracing::error!("Failed to save mobile: {}", e);
                return false;
            }
            true
        } else {
            false
        }
    });

    // get_agent_templates(mobile_id) -> Array
    let cloned_db = db.clone();
    engine.register_fn("get_agent_templates", move |mobile_id: String| -> Vec<rhai::Dynamic> {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return Vec::new(),
        };

        if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
            // If spawned instance has no templates, fall back to prototype
            if mobile.property_templates.is_empty() && !mobile.is_prototype && !mobile.vnum.is_empty() {
                if let Ok(Some(prototype)) = cloned_db.get_mobile_by_vnum(&mobile.vnum) {
                    if prototype.is_prototype {
                        return prototype.property_templates.into_iter().map(rhai::Dynamic::from).collect();
                    }
                }
            }
            mobile.property_templates.into_iter().map(rhai::Dynamic::from).collect()
        } else {
            Vec::new()
        }
    });

    // get_agent_leasing_area(mobile_id) -> String
    // Returns area_id for leasing operations. Priority:
    // 1. Agent's explicit leasing_area_id (for backwards compat)
    // 2. Prototype's leasing_area_id (for spawned instances)
    // 3. First template's area_id (new simplified approach)
    let cloned_db = db.clone();
    engine.register_fn("get_agent_leasing_area", move |mobile_id: String| -> String {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return String::new(),
        };

        if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
            // Check agent's explicit leasing_area_id first
            if let Some(area_id) = mobile.leasing_area_id {
                return area_id.to_string();
            }

            // For spawned instances, check prototype's leasing_area_id
            if !mobile.is_prototype && !mobile.vnum.is_empty() {
                if let Ok(Some(prototype)) = cloned_db.get_mobile_by_vnum(&mobile.vnum) {
                    if prototype.is_prototype {
                        if let Some(area_id) = prototype.leasing_area_id {
                            return area_id.to_string();
                        }
                        // Fall through to check prototype's templates
                        if !prototype.property_templates.is_empty() {
                            if let Ok(Some(template)) = cloned_db.get_property_template_by_vnum(&prototype.property_templates[0]) {
                                if let Some(area_id) = template.area_id {
                                    return area_id.to_string();
                                }
                            }
                        }
                    }
                }
            }

            // Fall back to first template's area_id
            if !mobile.property_templates.is_empty() {
                if let Ok(Some(template)) = cloned_db.get_property_template_by_vnum(&mobile.property_templates[0]) {
                    if let Some(area_id) = template.area_id {
                        return area_id.to_string();
                    }
                }
            }

            String::new()
        } else {
            String::new()
        }
    });

    // ========== Lease Functions ==========

    // get_lease(lease_id) -> LeaseData | ()
    let cloned_db = db.clone();
    engine.register_fn("get_lease", move |lease_id: String| -> rhai::Dynamic {
        let uuid = match uuid::Uuid::parse_str(&lease_id) {
            Ok(u) => u,
            Err(_) => return rhai::Dynamic::UNIT,
        };
        match cloned_db.get_lease(&uuid) {
            Ok(Some(lease)) => rhai::Dynamic::from(lease),
            _ => rhai::Dynamic::UNIT,
        }
    });

    // get_player_lease_in_area(char_name, area_id) -> LeaseData | ()
    let cloned_db = db.clone();
    engine.register_fn("get_player_lease_in_area", move |char_name: String, area_id: String| -> rhai::Dynamic {
        let uuid = match uuid::Uuid::parse_str(&area_id) {
            Ok(u) => u,
            Err(_) => return rhai::Dynamic::UNIT,
        };
        match cloned_db.get_player_lease_in_area(&char_name, &uuid) {
            Ok(Some(lease)) => rhai::Dynamic::from(lease),
            _ => rhai::Dynamic::UNIT,
        }
    });

    // get_all_player_leases(char_name) -> Array
    let cloned_db = db.clone();
    engine.register_fn("get_all_player_leases", move |char_name: String| -> Vec<rhai::Dynamic> {
        match cloned_db.get_leases_by_owner(&char_name) {
            Ok(leases) => leases.into_iter().map(rhai::Dynamic::from).collect(),
            Err(e) => {
                tracing::error!("Failed to get player leases: {}", e);
                Vec::new()
            }
        }
    });

    // save_lease(lease) -> bool
    let cloned_db = db.clone();
    engine.register_fn("save_lease", move |lease: LeaseData| {
        match cloned_db.save_lease(&lease) {
            Ok(()) => true,
            Err(e) => {
                tracing::error!("Failed to save lease: {}", e);
                false
            }
        }
    });

    // create_lease(template_vnum, owner_name, agent_id, office_room_id) -> LeaseData | ()
    // Creates a new property instance from a template
    let cloned_db = db.clone();
    engine.register_fn("create_lease", move |template_vnum: String, owner_name: String, agent_id: String, office_room_id: String| -> rhai::Dynamic {
        // Parse UUIDs
        let agent_uuid = match uuid::Uuid::parse_str(&agent_id) {
            Ok(u) => u,
            Err(_) => {
                tracing::error!("Invalid agent_id UUID: {}", agent_id);
                return rhai::Dynamic::UNIT;
            }
        };
        let office_uuid = match uuid::Uuid::parse_str(&office_room_id) {
            Ok(u) => u,
            Err(_) => {
                tracing::error!("Invalid office_room_id UUID: {}", office_room_id);
                return rhai::Dynamic::UNIT;
            }
        };

        // Get template
        let template = match cloned_db.get_property_template_by_vnum(&template_vnum) {
            Ok(Some(t)) => t,
            Ok(None) => {
                tracing::error!("Template not found: {}", template_vnum);
                return rhai::Dynamic::UNIT;
            }
            Err(e) => {
                tracing::error!("Failed to get template: {}", e);
                return rhai::Dynamic::UNIT;
            }
        };

        // Get area_id from template or agent
        let area_id = match template.area_id {
            Some(id) => id,
            None => {
                // Try to get from agent
                match cloned_db.get_mobile_data(&agent_uuid) {
                    Ok(Some(agent)) => agent.leasing_area_id.unwrap_or(uuid::Uuid::nil()),
                    _ => uuid::Uuid::nil(),
                }
            }
        };

        if area_id.is_nil() {
            tracing::error!("No area_id for lease");
            return rhai::Dynamic::UNIT;
        }

        // Get all template rooms
        let template_rooms = match cloned_db.get_rooms_by_template_id(&template.id) {
            Ok(rooms) => rooms,
            Err(e) => {
                tracing::error!("Failed to get template rooms: {}", e);
                return rhai::Dynamic::UNIT;
            }
        };

        if template_rooms.is_empty() {
            tracing::error!("Template has no rooms: {}", template_vnum);
            return rhai::Dynamic::UNIT;
        }

        // Generate new lease ID
        let lease_id = uuid::Uuid::new_v4();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        // Create UUID mapping: template_room_id -> new_instance_room_id
        let mut room_mapping: std::collections::HashMap<uuid::Uuid, uuid::Uuid> = std::collections::HashMap::new();
        let mut instanced_rooms: Vec<uuid::Uuid> = Vec::new();
        let mut entrance_room_id: Option<uuid::Uuid> = None;

        // First pass: Create all instance rooms
        for template_room in &template_rooms {
            let instance_id = uuid::Uuid::new_v4();
            room_mapping.insert(template_room.id, instance_id);
            instanced_rooms.push(instance_id);

            let instance_room = RoomData {
                id: instance_id,
                title: format!("{}'s {}", owner_name, template_room.title),
                description: template_room.description.clone(),
                exits: RoomExits::default(), // Will be set in second pass
                flags: RoomFlags {
                    property_storage: true,
                    ..template_room.flags.clone()
                },
                extra_descs: template_room.extra_descs.clone(),
                vnum: None,
                area_id: Some(area_id),
                triggers: Vec::new(),
                doors: std::collections::HashMap::new(),
                spring_desc: template_room.spring_desc.clone(),
                summer_desc: template_room.summer_desc.clone(),
                autumn_desc: template_room.autumn_desc.clone(),
                winter_desc: template_room.winter_desc.clone(),
                dynamic_desc: None,
                water_type: template_room.water_type.clone(),
                catch_table: Vec::new(),
                is_property_template: false,
                property_template_id: None,
                is_template_entrance: false,
                property_lease_id: Some(lease_id),
                property_entrance: template_room.is_template_entrance,
                recent_departures: Vec::new(),
                blood_trails: Vec::new(),
                traps: Vec::new(),
                living_capacity: 0,
                residents: Vec::new(),
            };

            if let Err(e) = cloned_db.save_room_data(instance_room) {
                tracing::error!("Failed to save instance room: {}", e);
                // Clean up created rooms on failure
                for created_id in &instanced_rooms {
                    let _ = cloned_db.delete_room(created_id);
                }
                return rhai::Dynamic::UNIT;
            }

            if template_room.is_template_entrance {
                entrance_room_id = Some(instance_id);
            }
        }

        // Second pass: Reconnect exits using the mapping
        for template_room in &template_rooms {
            let instance_id = room_mapping[&template_room.id];
            let mut instance_room = match cloned_db.get_room_data(&instance_id) {
                Ok(Some(r)) => r,
                _ => continue,
            };

            // Map each exit to new instance room
            let exits = &template_room.exits;
            instance_room.exits.north = exits.north.and_then(|t| room_mapping.get(&t).copied());
            instance_room.exits.south = exits.south.and_then(|t| room_mapping.get(&t).copied());
            instance_room.exits.east = exits.east.and_then(|t| room_mapping.get(&t).copied());
            instance_room.exits.west = exits.west.and_then(|t| room_mapping.get(&t).copied());
            instance_room.exits.up = exits.up.and_then(|t| room_mapping.get(&t).copied());
            instance_room.exits.down = exits.down.and_then(|t| room_mapping.get(&t).copied());
            // Copy custom exits (map to new room IDs)
            for (name, target) in &exits.custom {
                if let Some(&new_target) = room_mapping.get(target) {
                    instance_room.exits.custom.insert(name.clone(), new_target);
                }
            }

            if let Err(e) = cloned_db.save_room_data(instance_room) {
                tracing::error!("Failed to update instance room exits: {}", e);
            }
        }

        // Set entrance room's "out" exit to leasing office
        if let Some(entrance_id) = entrance_room_id {
            if let Ok(Some(mut entrance)) = cloned_db.get_room_data(&entrance_id) {
                entrance.exits.out = Some(office_uuid);
                if let Err(e) = cloned_db.save_room_data(entrance) {
                    tracing::error!("Failed to set entrance out exit: {}", e);
                }
            }
        }

        // Copy amenity items (items with no_get flag) to instance rooms
        for template_room in &template_rooms {
            let instance_room_id = room_mapping[&template_room.id];
            if let Ok(items) = cloned_db.get_items_in_room(&template_room.id) {
                for item in items {
                    if item.flags.no_get {
                        // Clone the item
                        let mut cloned_item = item.clone();
                        cloned_item.id = uuid::Uuid::new_v4();
                        cloned_item.is_prototype = false;
                        if let Err(e) = cloned_db.save_item_data(cloned_item.clone()) {
                            tracing::error!("Failed to save cloned amenity: {}", e);
                            continue;
                        }
                        if let Err(e) = cloned_db.move_item_to_room(&cloned_item.id, &instance_room_id) {
                            tracing::error!("Failed to move amenity to instance room: {}", e);
                        }
                    }
                }
            }
        }

        // Rent period is configurable via settings (default: 30 game days, 900 secs per game day)
        let rent_period_days: i64 = cloned_db
            .get_setting_or_default("rent_period_game_days", "30")
            .unwrap_or_else(|_| "30".to_string())
            .parse::<i64>()
            .unwrap_or(30)
            .max(1);
        let rent_duration: i64 = rent_period_days * 900;

        // Create lease
        let lease = LeaseData::new(
            template_vnum.clone(),
            owner_name.clone(),
            agent_uuid,
            office_uuid,
            area_id,
            template.monthly_rent,
        );

        // Update lease with calculated values
        let mut final_lease = lease;
        final_lease.id = lease_id;
        final_lease.instanced_rooms = instanced_rooms;
        final_lease.entrance_room_id = entrance_room_id.unwrap_or(uuid::Uuid::nil());
        final_lease.rent_paid_until = now + rent_duration;
        final_lease.created_at = now;

        // Save lease
        if let Err(e) = cloned_db.save_lease(&final_lease) {
            tracing::error!("Failed to save lease: {}", e);
            // Clean up on failure
            for room_id in &final_lease.instanced_rooms {
                let _ = cloned_db.delete_room(room_id);
            }
            return rhai::Dynamic::UNIT;
        }

        tracing::info!("Created lease {} for {} (template: {})", lease_id, owner_name, template_vnum);
        rhai::Dynamic::from(final_lease)
    });

    // is_lease_paid(lease_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("is_lease_paid", move |lease_id: String| -> bool {
        let uuid = match uuid::Uuid::parse_str(&lease_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        match cloned_db.get_lease(&uuid) {
            Ok(Some(lease)) => !lease.is_evicted && lease.rent_paid_until > now,
            _ => false,
        }
    });

    // get_lease_days_remaining(lease_id) -> i32
    let cloned_db = db.clone();
    engine.register_fn("get_lease_days_remaining", move |lease_id: String| -> i64 {
        let uuid = match uuid::Uuid::parse_str(&lease_id) {
            Ok(u) => u,
            Err(_) => return 0,
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        match cloned_db.get_lease(&uuid) {
            Ok(Some(lease)) => {
                let secs_remaining = lease.rent_paid_until - now;
                // Game days - using 900 seconds per game day from time system
                secs_remaining / 900
            }
            _ => 0,
        }
    });

    // ========== Property Access Functions ==========

    // set_property_access(lease_id, level) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_property_access", move |lease_id: String, level: String| {
        let uuid = match uuid::Uuid::parse_str(&lease_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        let access_level = match PartyAccessLevel::from_str(&level) {
            Some(l) => l,
            None => return false,
        };

        if let Ok(Some(mut lease)) = cloned_db.get_lease(&uuid) {
            lease.party_access = access_level;
            if let Err(e) = cloned_db.save_lease(&lease) {
                tracing::error!("Failed to save lease: {}", e);
                return false;
            }
            true
        } else {
            false
        }
    });

    // add_trusted_visitor(lease_id, char_name) -> bool
    let cloned_db = db.clone();
    engine.register_fn("add_trusted_visitor", move |lease_id: String, char_name: String| {
        let uuid = match uuid::Uuid::parse_str(&lease_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        if let Ok(Some(mut lease)) = cloned_db.get_lease(&uuid) {
            let name_lower = char_name.to_lowercase();
            if !lease.trusted_visitors.iter().any(|n| n.to_lowercase() == name_lower) {
                lease.trusted_visitors.push(char_name);
                if let Err(e) = cloned_db.save_lease(&lease) {
                    tracing::error!("Failed to save lease: {}", e);
                    return false;
                }
            }
            true
        } else {
            false
        }
    });

    // remove_trusted_visitor(lease_id, char_name) -> bool
    let cloned_db = db.clone();
    engine.register_fn("remove_trusted_visitor", move |lease_id: String, char_name: String| {
        let uuid = match uuid::Uuid::parse_str(&lease_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        if let Ok(Some(mut lease)) = cloned_db.get_lease(&uuid) {
            let name_lower = char_name.to_lowercase();
            lease.trusted_visitors.retain(|n| n.to_lowercase() != name_lower);
            if let Err(e) = cloned_db.save_lease(&lease) {
                tracing::error!("Failed to save lease: {}", e);
                return false;
            }
            true
        } else {
            false
        }
    });

    // can_enter_property(lease_id, char_name) -> bool
    let cloned_db = db.clone();
    engine.register_fn("can_enter_property", move |lease_id: String, char_name: String| -> bool {
        let uuid = match uuid::Uuid::parse_str(&lease_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        match cloned_db.get_lease(&uuid) {
            Ok(Some(lease)) => {
                // Owner always has access
                if lease.owner_name.to_lowercase() == char_name.to_lowercase() {
                    return true;
                }
                // Trusted visitors always have access
                if lease.trusted_visitors.iter().any(|n| n.to_lowercase() == char_name.to_lowercase()) {
                    return true;
                }
                // Check party access level
                matches!(lease.party_access, PartyAccessLevel::VisitOnly | PartyAccessLevel::FullAccess)
            }
            _ => false,
        }
    });

    // can_use_property(lease_id, char_name) -> bool (full access for taking items, etc.)
    let cloned_db = db.clone();
    engine.register_fn("can_use_property", move |lease_id: String, char_name: String| -> bool {
        let uuid = match uuid::Uuid::parse_str(&lease_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        match cloned_db.get_lease(&uuid) {
            Ok(Some(lease)) => {
                // Owner always has full access
                if lease.owner_name.to_lowercase() == char_name.to_lowercase() {
                    return true;
                }
                // Trusted visitors have full access
                if lease.trusted_visitors.iter().any(|n| n.to_lowercase() == char_name.to_lowercase()) {
                    return true;
                }
                // Check party access level for full access
                matches!(lease.party_access, PartyAccessLevel::FullAccess)
            }
            _ => false,
        }
    });

    // ========== Property Room Functions ==========

    // is_property_room(room_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("is_property_room", move |room_id: String| -> bool {
        let uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        match cloned_db.get_room_data(&uuid) {
            Ok(Some(room)) => room.property_lease_id.is_some(),
            _ => false,
        }
    });

    // is_template_room(room_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("is_template_room", move |room_id: String| -> bool {
        let uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        match cloned_db.get_room_data(&uuid) {
            Ok(Some(room)) => room.is_property_template,
            _ => false,
        }
    });

    // get_lease_for_room(room_id) -> LeaseData | ()
    let cloned_db = db.clone();
    engine.register_fn("get_lease_for_room", move |room_id: String| -> rhai::Dynamic {
        let uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return rhai::Dynamic::UNIT,
        };
        match cloned_db.get_lease_for_room(&uuid) {
            Ok(Some(lease)) => rhai::Dynamic::from(lease),
            _ => rhai::Dynamic::UNIT,
        }
    });

    // get_property_owner(room_id) -> String
    let cloned_db = db.clone();
    engine.register_fn("get_property_owner", move |room_id: String| -> String {
        let uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return String::new(),
        };
        match cloned_db.get_lease_for_room(&uuid) {
            Ok(Some(lease)) => lease.owner_name,
            _ => String::new(),
        }
    });

    // get_property_entrance(lease_id) -> String
    let cloned_db = db.clone();
    engine.register_fn("get_property_entrance", move |lease_id: String| -> String {
        let uuid = match uuid::Uuid::parse_str(&lease_id) {
            Ok(u) => u,
            Err(_) => return String::new(),
        };
        match cloned_db.get_lease(&uuid) {
            Ok(Some(lease)) => lease.entrance_room_id.to_string(),
            _ => String::new(),
        }
    });

    // ========== Template Room Creation Functions ==========

    // create_template_room(template_id, title, description, is_entrance) -> RoomData | ()
    let cloned_db = db.clone();
    engine.register_fn("create_template_room", move |template_id: String, title: String, description: String, is_entrance: bool| -> rhai::Dynamic {
        let template_uuid = match uuid::Uuid::parse_str(&template_id) {
            Ok(u) => u,
            Err(_) => return rhai::Dynamic::UNIT,
        };

        // Get template to inherit area_id
        let area_id = match cloned_db.get_property_template(&template_uuid) {
            Ok(Some(template)) => template.area_id,
            _ => None,
        };

        let room = RoomData {
            id: uuid::Uuid::new_v4(),
            title,
            description,
            exits: RoomExits::default(),
            flags: RoomFlags::default(),
            extra_descs: Vec::new(),
            vnum: None,
            area_id,
            triggers: Vec::new(),
            doors: std::collections::HashMap::new(),
            spring_desc: None,
            summer_desc: None,
            autumn_desc: None,
            winter_desc: None,
            dynamic_desc: None,
            water_type: crate::WaterType::None,
            catch_table: Vec::new(),
            is_property_template: true,
            property_template_id: Some(template_uuid),
            is_template_entrance: is_entrance,
            property_lease_id: None,
            property_entrance: false,
            recent_departures: Vec::new(),
            blood_trails: Vec::new(),
            traps: Vec::new(),
            living_capacity: 0,
            residents: Vec::new(),
        };

        if let Err(e) = cloned_db.save_room_data(room.clone()) {
            tracing::error!("Failed to save template room: {}", e);
            return rhai::Dynamic::UNIT;
        }

        // If this is the entrance, update the template
        if is_entrance {
            if let Ok(Some(mut template)) = cloned_db.get_property_template(&template_uuid) {
                template.entrance_room_id = room.id;
                if let Err(e) = cloned_db.save_property_template(&template) {
                    tracing::error!("Failed to update template entrance: {}", e);
                }
            }
        }

        rhai::Dynamic::from(room)
    });

    // set_room_as_template_entrance(room_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_room_as_template_entrance", move |room_id: String| -> bool {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        if let Ok(Some(mut room)) = cloned_db.get_room_data(&room_uuid) {
            if !room.is_property_template {
                return false;
            }

            let template_id = match room.property_template_id {
                Some(id) => id,
                None => return false,
            };

            // Clear previous entrance
            if let Ok(rooms) = cloned_db.get_rooms_by_template_id(&template_id) {
                for mut r in rooms {
                    if r.is_template_entrance && r.id != room_uuid {
                        r.is_template_entrance = false;
                        let _ = cloned_db.save_room_data(r);
                    }
                }
            }

            // Set this room as entrance
            room.is_template_entrance = true;
            if let Err(e) = cloned_db.save_room_data(room) {
                tracing::error!("Failed to save room: {}", e);
                return false;
            }

            // Update template
            if let Ok(Some(mut template)) = cloned_db.get_property_template(&template_id) {
                template.entrance_room_id = room_uuid;
                if let Err(e) = cloned_db.save_property_template(&template) {
                    tracing::error!("Failed to update template: {}", e);
                    return false;
                }
            }

            true
        } else {
            false
        }
    });

    // mark_room_as_template(room_id, template_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("mark_room_as_template", move |room_id: String, template_id: String| -> bool {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let template_uuid = match uuid::Uuid::parse_str(&template_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        if let Ok(Some(mut room)) = cloned_db.get_room_data(&room_uuid) {
            room.is_property_template = true;
            room.property_template_id = Some(template_uuid);
            if let Err(e) = cloned_db.save_room_data(room) {
                tracing::error!("Failed to save room: {}", e);
                return false;
            }
            true
        } else {
            false
        }
    });

    // ========== Escrow Functions ==========

    // get_player_escrow(char_name) -> Array
    let cloned_db = db.clone();
    engine.register_fn("get_player_escrow", move |char_name: String| -> Vec<rhai::Dynamic> {
        match cloned_db.get_escrow_by_owner(&char_name) {
            Ok(escrows) => escrows.into_iter().map(rhai::Dynamic::from).collect(),
            Err(e) => {
                tracing::error!("Failed to get player escrow: {}", e);
                Vec::new()
            }
        }
    });

    // get_escrow(escrow_id) -> EscrowData | ()
    let cloned_db = db.clone();
    engine.register_fn("get_escrow", move |escrow_id: String| -> rhai::Dynamic {
        let uuid = match uuid::Uuid::parse_str(&escrow_id) {
            Ok(u) => u,
            Err(_) => return rhai::Dynamic::UNIT,
        };
        match cloned_db.get_escrow(&uuid) {
            Ok(Some(escrow)) => rhai::Dynamic::from(escrow),
            _ => rhai::Dynamic::UNIT,
        }
    });

    // calculate_escrow_fee(escrow_id) -> i32
    let cloned_db = db.clone();
    engine.register_fn("calculate_escrow_fee", move |escrow_id: String| -> i64 {
        let uuid = match uuid::Uuid::parse_str(&escrow_id) {
            Ok(u) => u,
            Err(_) => return 0,
        };
        match cloned_db.get_escrow(&uuid) {
            Ok(Some(escrow)) => escrow.retrieval_fee as i64,
            _ => 0,
        }
    });

    // retrieve_escrow_items(escrow_id, dest_room_id) -> bool
    // Moves all items from escrow to a destination room
    let cloned_db = db.clone();
    engine.register_fn("retrieve_escrow_items", move |escrow_id: String, dest_room_id: String| -> bool {
        let escrow_uuid = match uuid::Uuid::parse_str(&escrow_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let room_uuid = match uuid::Uuid::parse_str(&dest_room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        // Get escrow
        let escrow = match cloned_db.get_escrow(&escrow_uuid) {
            Ok(Some(e)) => e,
            _ => return false,
        };

        // Move all items to destination room
        for item_id in &escrow.items {
            if let Err(e) = cloned_db.move_item_to_room(item_id, &room_uuid) {
                tracing::error!("Failed to move item {} to room: {}", item_id, e);
            }

            // Relocate any plant growing in this pot
            if let Ok(Some(item)) = cloned_db.get_item_data(item_id) {
                if item.flags.plant_pot {
                    if let Ok(plants) = cloned_db.list_all_plants() {
                        for mut plant in plants {
                            if plant.pot_item_id == Some(*item_id) {
                                plant.room_id = room_uuid;
                                if let Err(e) = cloned_db.save_plant(plant) {
                                    tracing::error!("Failed to relocate plant for pot {}: {}", item_id, e);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Remove escrow from character's escrow_ids
        if let Ok(Some(mut char_data)) = cloned_db.get_character_data(&escrow.owner_name) {
            char_data.escrow_ids.retain(|id| *id != escrow_uuid);
            if let Err(e) = cloned_db.save_character_data(char_data) {
                tracing::error!("Failed to update character escrow_ids: {}", e);
            }
        }

        // Delete escrow entry
        if let Err(e) = cloned_db.delete_escrow(&escrow_uuid) {
            tracing::error!("Failed to delete escrow {}: {}", escrow_uuid, e);
            return false;
        }

        true
    });

    // delete_escrow(escrow_id) -> bool
    // Deletes an escrow entry and all its items
    let cloned_db = db.clone();
    engine.register_fn("delete_escrow", move |escrow_id: String| -> bool {
        let escrow_uuid = match uuid::Uuid::parse_str(&escrow_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        // Get escrow
        let escrow = match cloned_db.get_escrow(&escrow_uuid) {
            Ok(Some(e)) => e,
            _ => return false,
        };

        // Delete all items (and any contents inside containers)
        for item_id in &escrow.items {
            let _ = cloned_db.delete_item_recursive(item_id);
        }

        // Remove from character's escrow_ids
        if let Ok(Some(mut char_data)) = cloned_db.get_character_data(&escrow.owner_name) {
            char_data.escrow_ids.retain(|id| *id != escrow_uuid);
            let _ = cloned_db.save_character_data(char_data);
        }

        // Delete escrow entry
        cloned_db.delete_escrow(&escrow_uuid).is_ok()
    });

    // get_escrow_items(escrow_id) -> Array of item names
    let cloned_db = db.clone();
    engine.register_fn("get_escrow_items", move |escrow_id: String| -> Vec<rhai::Dynamic> {
        let escrow_uuid = match uuid::Uuid::parse_str(&escrow_id) {
            Ok(u) => u,
            Err(_) => return Vec::new(),
        };

        let escrow = match cloned_db.get_escrow(&escrow_uuid) {
            Ok(Some(e)) => e,
            _ => return Vec::new(),
        };

        let mut items = Vec::new();
        for item_id in &escrow.items {
            if let Ok(Some(item)) = cloned_db.get_item_data(item_id) {
                items.push(rhai::Dynamic::from(item.name));
            }
        }
        items
    });

    // ========== Tour Functions ==========

    // start_tour(connection_id, template_vnum) -> bool
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn("start_tour", move |connection_id: String, template_vnum: String| -> bool {
        let template = match cloned_db.get_property_template_by_vnum(&template_vnum) {
            Ok(Some(t)) => t,
            _ => return false,
        };

        if template.entrance_room_id.is_nil() {
            return false;
        }

        // Get current room to save as return point
        let char_name = get_character_name_from_connection(&conns, &connection_id);
        if char_name.is_empty() {
            return false;
        }

        if let Ok(Some(mut char_data)) = cloned_db.get_character_data(&char_name) {
            char_data.tour_origin_room = Some(char_data.current_room_id);
            char_data.on_tour = true;
            char_data.current_room_id = template.entrance_room_id;

            if let Err(e) = cloned_db.save_character_data(char_data.clone()) {
                tracing::error!("Failed to save character for tour: {}", e);
                return false;
            }
            true
        } else {
            false
        }
    });

    // end_tour(connection_id) -> bool
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn("end_tour", move |connection_id: String| -> bool {
        let char_name = get_character_name_from_connection(&conns, &connection_id);
        if char_name.is_empty() {
            return false;
        }

        if let Ok(Some(mut char_data)) = cloned_db.get_character_data(&char_name) {
            if !char_data.on_tour {
                return false;
            }

            if let Some(origin) = char_data.tour_origin_room {
                char_data.current_room_id = origin;
            }
            char_data.tour_origin_room = None;
            char_data.on_tour = false;

            if let Err(e) = cloned_db.save_character_data(char_data.clone()) {
                tracing::error!("Failed to save character after tour: {}", e);
                return false;
            }
            true
        } else {
            false
        }
    });

    // is_on_tour(connection_id) -> bool
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn("is_on_tour", move |connection_id: String| -> bool {
        let char_name = get_character_name_from_connection(&conns, &connection_id);
        if char_name.is_empty() {
            return false;
        }

        match cloned_db.get_character_data(&char_name) {
            Ok(Some(char_data)) => char_data.on_tour,
            _ => false,
        }
    });

    // get_template_for_room(room_id) -> PropertyTemplate | ()
    let cloned_db = db.clone();
    engine.register_fn("get_template_for_room", move |room_id: String| -> rhai::Dynamic {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return rhai::Dynamic::UNIT,
        };

        if let Ok(Some(room)) = cloned_db.get_room_data(&room_uuid) {
            if let Some(template_id) = room.property_template_id {
                if let Ok(Some(template)) = cloned_db.get_property_template(&template_id) {
                    return rhai::Dynamic::from(template);
                }
            }
        }
        rhai::Dynamic::UNIT
    });

    // end_lease(lease_id, to_escrow) -> bool
    // Ends a lease, optionally moving items to escrow
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    engine.register_fn("end_lease", move |lease_id: String, to_escrow: bool| -> bool {
        let lease_uuid = match uuid::Uuid::parse_str(&lease_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        let mut lease = match cloned_db.get_lease(&lease_uuid) {
            Ok(Some(l)) => l,
            _ => return false,
        };

        if lease.is_evicted {
            return false; // Already ended
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        // Collect player items from property rooms
        let mut item_ids: Vec<uuid::Uuid> = Vec::new();
        for room_id in &lease.instanced_rooms {
            if let Ok(items) = cloned_db.get_items_in_room(room_id) {
                for item in items {
                    if !item.flags.no_get {
                        item_ids.push(item.id);
                    }
                }
            }
        }

        // Create escrow if requested and there are items
        if to_escrow && !item_ids.is_empty() {
            let retrieval_fee = 50 + (lease.monthly_rent / 20); // Lower fee for voluntary
            let escrow_expiry_days: i64 = cloned_db
                .get_setting_or_default("escrow_expiry_real_days", "30")
                .unwrap_or_else(|_| "30".to_string())
                .parse::<i64>()
                .unwrap_or(30)
                .max(1);
            let escrow = EscrowData::new(
                lease.owner_name.clone(),
                item_ids.clone(),
                lease.id,
                escrow_expiry_days,
                retrieval_fee,
            );

            // Move items to nowhere
            for item_id in &item_ids {
                let _ = cloned_db.move_item_to_nowhere(item_id);
            }

            // Save escrow
            if let Err(e) = cloned_db.save_escrow(&escrow) {
                tracing::error!("Failed to save escrow: {}", e);
            } else {
                // Update character's escrow_ids
                if let Ok(Some(mut char_data)) = cloned_db.get_character_data(&lease.owner_name) {
                    char_data.escrow_ids.push(escrow.id);
                    let _ = cloned_db.save_character_data(char_data);
                }
            }
        } else if !to_escrow {
            // Delete items if not escrowing
            for room_id in &lease.instanced_rooms {
                if let Ok(items) = cloned_db.get_items_in_room(room_id) {
                    for item in items {
                        if !item.flags.no_get {
                            let _ = cloned_db.delete_item(&item.id);
                        }
                    }
                }
            }
        }

        // Relocate any online players out of property rooms before deletion
        if let Ok(mut conns) = cloned_conns.lock() {
            for (_conn_id, session) in conns.iter_mut() {
                if let Some(ref mut character) = session.character {
                    if lease.instanced_rooms.contains(&character.current_room_id) {
                        character.current_room_id = lease.leasing_office_room_id;
                        let _ = cloned_db.save_character_data(character.clone());
                        let _ = session.sender.send(
                            "\nThe property around you dissolves as the lease is terminated.\nYou find yourself back at the leasing office.\n".to_string()
                        );
                    }
                }
            }
        }

        // Delete property rooms and amenities
        for room_id in &lease.instanced_rooms {
            if let Ok(items) = cloned_db.get_items_in_room(room_id) {
                for item in items {
                    if item.flags.no_get {
                        let _ = cloned_db.delete_item(&item.id);
                    }
                }
            }
            let _ = cloned_db.delete_room(room_id);
        }

        // Mark lease as ended
        lease.is_evicted = true;
        lease.eviction_time = Some(now);
        lease.instanced_rooms.clear();
        let _ = cloned_db.save_lease(&lease);

        true
    });

    // count_items_in_property(lease_id) -> i64
    // Counts non-amenity items in property rooms
    let cloned_db = db.clone();
    engine.register_fn("count_items_in_property", move |lease_id: String| -> i64 {
        let lease_uuid = match uuid::Uuid::parse_str(&lease_id) {
            Ok(u) => u,
            Err(_) => return 0,
        };

        let lease = match cloned_db.get_lease(&lease_uuid) {
            Ok(Some(l)) => l,
            _ => return 0,
        };

        let mut count = 0i64;
        for room_id in &lease.instanced_rooms {
            if let Ok(items) = cloned_db.get_items_in_room(room_id) {
                for item in items {
                    if !item.flags.no_get {
                        count += 1;
                    }
                }
            }
        }
        count
    });

    // are_in_same_group(char1, char2) -> bool
    // Checks if two characters are in the same group (one follows the other, or same leader)
    let cloned_conns = connections.clone();
    engine.register_fn("are_in_same_group", move |char1: String, char2: String| -> bool {
        let char1_lower = char1.to_lowercase();
        let char2_lower = char2.to_lowercase();

        if char1_lower == char2_lower {
            return true; // Same person
        }

        let conns = cloned_conns.lock().unwrap();

        // Find both characters
        let mut char1_data: Option<&crate::CharacterData> = None;
        let mut char2_data: Option<&crate::CharacterData> = None;

        for session in conns.values() {
            if let Some(ref char) = session.character {
                if char.name.to_lowercase() == char1_lower {
                    char1_data = Some(char);
                }
                if char.name.to_lowercase() == char2_lower {
                    char2_data = Some(char);
                }
            }
        }

        let (c1, c2) = match (char1_data, char2_data) {
            (Some(a), Some(b)) => (a, b),
            _ => return false,
        };

        // Check if c1 is grouped and following c2
        if c1.is_grouped {
            if let Some(ref following) = c1.following {
                if following.to_lowercase() == char2_lower {
                    return true;
                }
            }
        }

        // Check if c2 is grouped and following c1
        if c2.is_grouped {
            if let Some(ref following) = c2.following {
                if following.to_lowercase() == char1_lower {
                    return true;
                }
            }
        }

        // Check if both follow the same leader
        if c1.is_grouped && c2.is_grouped {
            if let (Some(f1), Some(f2)) = (&c1.following, &c2.following) {
                if f1.to_lowercase() == f2.to_lowercase() {
                    return true;
                }
            }
        }

        false
    });

    // get_player_lease_by_name(owner_name, area_id) -> LeaseData | ()
    // Gets a player's lease by their name (for visit command)
    let cloned_db = db.clone();
    engine.register_fn("get_player_lease_by_name", move |owner_name: String, area_id: String| -> rhai::Dynamic {
        let area_uuid = match uuid::Uuid::parse_str(&area_id) {
            Ok(u) => u,
            Err(_) => return rhai::Dynamic::UNIT,
        };

        if let Ok(leases) = cloned_db.list_all_leases() {
            for lease in leases {
                if lease.owner_name.to_lowercase() == owner_name.to_lowercase()
                    && lease.area_id == area_uuid
                    && !lease.is_evicted
                {
                    return rhai::Dynamic::from(lease);
                }
            }
        }
        rhai::Dynamic::UNIT
    });

    // transfer_property_items(from_lease_id, to_lease_id) -> i64
    // Moves all player items from one property to another, returns count
    let cloned_db = db.clone();
    engine.register_fn("transfer_property_items", move |from_lease_id: String, to_lease_id: String| -> i64 {
        let from_uuid = match uuid::Uuid::parse_str(&from_lease_id) {
            Ok(u) => u,
            Err(_) => return 0,
        };
        let to_uuid = match uuid::Uuid::parse_str(&to_lease_id) {
            Ok(u) => u,
            Err(_) => return 0,
        };

        let from_lease = match cloned_db.get_lease(&from_uuid) {
            Ok(Some(l)) => l,
            _ => return 0,
        };
        let to_lease = match cloned_db.get_lease(&to_uuid) {
            Ok(Some(l)) => l,
            _ => return 0,
        };

        // Get destination entrance room
        let dest_room_id = to_lease.entrance_room_id;
        let mut count = 0i64;

        // Move all non-amenity items from source property
        for room_id in &from_lease.instanced_rooms {
            if let Ok(items) = cloned_db.get_items_in_room(room_id) {
                for item in items {
                    if !item.flags.no_get {
                        if cloned_db.move_item_to_room(&item.id, &dest_room_id).is_ok() {
                            count += 1;
                        }
                    }
                }
            }
        }

        count
    });

    // delete_lease_rooms(lease_id) -> bool
    // Deletes all rooms associated with a lease (for upgrade cleanup)
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    engine.register_fn("delete_lease_rooms", move |lease_id: String| -> bool {
        let lease_uuid = match uuid::Uuid::parse_str(&lease_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        let mut lease = match cloned_db.get_lease(&lease_uuid) {
            Ok(Some(l)) => l,
            _ => return false,
        };

        // Relocate any online players out of property rooms before deletion
        if let Ok(mut conns) = cloned_conns.lock() {
            for (_conn_id, session) in conns.iter_mut() {
                if let Some(ref mut character) = session.character {
                    if lease.instanced_rooms.contains(&character.current_room_id) {
                        character.current_room_id = lease.leasing_office_room_id;
                        let _ = cloned_db.save_character_data(character.clone());
                        let _ = session.sender.send(
                            "\nThe property around you dissolves as the lease is terminated.\nYou find yourself back at the leasing office.\n".to_string()
                        );
                    }
                }
            }
        }

        // Delete amenities and rooms
        for room_id in &lease.instanced_rooms {
            if let Ok(items) = cloned_db.get_items_in_room(room_id) {
                for item in items {
                    if item.flags.no_get {
                        let _ = cloned_db.delete_item(&item.id);
                    }
                }
            }
            let _ = cloned_db.delete_room(room_id);
        }

        // Clear room list and mark evicted
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        lease.instanced_rooms.clear();
        lease.is_evicted = true;
        lease.eviction_time = Some(now);
        let _ = cloned_db.save_lease(&lease);

        true
    });
}

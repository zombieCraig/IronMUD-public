//! Library-side player death helper.
//!
//! Used by Rhai-driven death paths (e.g. ROOM_DEATH triggered by `apply_room_death`).
//! The combat tick has its own copy in `src/ticks/combat/tick.rs::process_player_death`
//! reachable only from the binary side; this mirrors that flow with lib-only helpers
//! so it can be called from `src/script/*`.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use anyhow::Result;

use crate::SharedConnections;
use crate::db::Db;
use crate::session::broadcast::broadcast_to_room;
use crate::session::connection::send_client_message;
use crate::STARTING_ROOM_ID;
use crate::types::{CharacterData, EffectType, ItemData, ItemFlags, ItemLocation, ItemType, LiquidType};

/// Drop any `EffectType::Charmed` buffs sourced to `player_name` from every
/// non-prototype mobile in the world. Used on player death and quit so that
/// charmed mobs revert immediately rather than waiting for the buff to decay.
/// Also clears `charm_stay` / `charm_follow_player` on those mobs, and clears
/// dangling `charm_follow_player == player_name` overrides on mobs charmed by
/// other players (so they fall back to following their own master).
pub fn break_all_charms_by_player(db: &Db, player_name: &str) {
    if player_name.is_empty() {
        return;
    }
    let Ok(mobiles) = db.list_all_mobiles() else {
        return;
    };
    for mut mobile in mobiles {
        if mobile.is_prototype {
            continue;
        }
        let mut changed = false;
        let before = mobile.active_buffs.len();
        mobile.active_buffs.retain(|b| {
            !(b.effect_type == EffectType::Charmed && b.source.eq_ignore_ascii_case(player_name))
        });
        if mobile.active_buffs.len() != before {
            mobile.charm_stay = false;
            mobile.charm_follow_player = None;
            changed = true;
        }
        if let Some(ref name) = mobile.charm_follow_player {
            if name.eq_ignore_ascii_case(player_name) {
                mobile.charm_follow_player = None;
                changed = true;
            }
        }
        if changed {
            let _ = db.save_mobile_data(mobile);
        }
    }
}

fn build_player_corpse(name: &str, room_id: Uuid, gold: i64) -> ItemData {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    ItemData {
        id: Uuid::new_v4(),
        name: format!("corpse of {}", name),
        short_desc: format!("The corpse of {} lies here.", name),
        long_desc: format!("The lifeless body of {} lies in a crumpled heap.", name),
        keywords: vec!["corpse".to_string(), "body".to_string(), name.to_lowercase()],
        item_type: ItemType::Container,
        categories: Vec::new(),
        teaches_recipe: None,
        teaches_spell: None,
        note_content: None,
        extra_descs: Vec::new(),
        wear_locations: vec![],
        armor_class: None,
        hit_bonus: 0,
        damage_bonus: 0,
        protects: vec![],
        flags: ItemFlags {
            no_get: true,
            is_corpse: true,
            corpse_owner: name.to_string(),
            corpse_created_at: now,
            corpse_is_player: true,
            corpse_gold: gold,
            ..Default::default()
        },
        weight: 100,
        value: 0,
        location: ItemLocation::Room(room_id),
        damage_dice_count: 0,
        damage_dice_sides: 0,
        damage_type: Default::default(),
        two_handed: false,
        weapon_skill: None,
        container_contents: vec![],
        container_max_items: 1000,
        container_max_weight: 10000,
        container_closed: false,
        container_locked: false,
        container_key_vnum: None,
        weight_reduction: 0,
        liquid_type: LiquidType::default(),
        liquid_current: 0,
        liquid_max: 0,
        liquid_poisoned: false,
        liquid_effects: vec![],
        food_nutrition: 0,
        food_poisoned: false,
        food_spoil_duration: 0,
        food_created_at: None,
        food_effects: vec![],
        food_spoilage_points: 0.0,
        preservation_level: 0,
        level_requirement: 0,
        stat_str: 0,
        stat_dex: 0,
        stat_con: 0,
        stat_int: 0,
        stat_wis: 0,
        stat_cha: 0,
        insulation: 0,
        is_prototype: false,
        vnum: None,
        world_max_count: None,
        triggers: vec![],
        vending_stock: vec![],
        vending_sell_rate: 150,
        quality: 0,
        bait_uses: 0,
        holes: 0,
        medical_tier: 0,
        medical_uses: 0,
        treats_wound_types: vec![],
        max_treatable_wound: String::new(),
        transport_link: None,
        caliber: None,
        ammo_count: 0,
        ammo_damage_bonus: 0,
        ranged_type: None,
        magazine_size: 0,
        loaded_ammo: 0,
        loaded_ammo_bonus: 0,
        loaded_ammo_vnum: None,
        fire_mode: String::new(),
        supported_fire_modes: vec![],
        noise_level: String::new(),
        ammo_effect_type: String::new(),
        ammo_effect_duration: 0,
        ammo_effect_damage: 0,
        loaded_ammo_effect_type: String::new(),
        loaded_ammo_effect_duration: 0,
        loaded_ammo_effect_damage: 0,
        attachment_slot: String::new(),
        attachment_accuracy_bonus: 0,
        attachment_noise_reduction: 0,
        attachment_magazine_bonus: 0,
        attachment_compatible_types: Vec::new(),
        plant_prototype_vnum: String::new(),
        fertilizer_duration: 0,
        treats_infestation: String::new(),
    }
}

/// Kill a player at their current room, drop a corpse with their gear + gold,
/// and respawn at their bound spawn room (or STARTING_ROOM_ID).
///
/// `connection_id_str` is the calling player's connection id, used for the
/// "You have died!" private message; `room_id` is where the corpse is dropped.
pub fn kill_player_at_room(
    db: &Arc<Db>,
    connections: &SharedConnections,
    char: &mut CharacterData,
    room_id: &Uuid,
    connection_id_str: &str,
) -> Result<()> {
    let char_name = char.name.clone();

    // Release any mobiles this player had charmed.
    break_all_charms_by_player(db, &char_name);

    send_client_message(connections, connection_id_str.to_string(), "You have died!".to_string());
    broadcast_to_room(connections, *room_id, format!("{} has died!", char_name), Some(&char_name));

    let mut corpse = build_player_corpse(&char_name, *room_id, char.gold as i64);
    let corpse_id = corpse.id;

    if let Ok(inv) = db.get_items_in_inventory(&char_name) {
        for item in inv {
            let item_id = item.id;
            let mut updated = item;
            updated.location = ItemLocation::Container(corpse_id);
            let _ = db.save_item_data(updated);
            corpse.container_contents.push(item_id);
        }
    }
    if let Ok(eq) = db.get_equipped_items(&char_name) {
        for item in eq {
            let item_id = item.id;
            let mut updated = item;
            updated.location = ItemLocation::Container(corpse_id);
            let _ = db.save_item_data(updated);
            corpse.container_contents.push(item_id);
        }
    }
    db.save_item_data(corpse)?;

    char.gold = 0;

    let spawn_room = char
        .spawn_room_id
        .unwrap_or_else(|| Uuid::parse_str(STARTING_ROOM_ID).unwrap());

    char.current_room_id = spawn_room;
    char.hp = (char.max_hp / 4).max(1);
    char.is_unconscious = false;
    char.bleedout_rounds_remaining = 0;
    char.wounds.clear();
    char.ongoing_effects.clear();
    char.combat.in_combat = false;
    char.combat.targets.clear();
    char.combat.stun_rounds_remaining = 0;
    char.combat.ammo_depleted = 0;

    char.is_wet = false;
    char.wet_level = 0;
    char.cold_exposure = 0;
    char.heat_exposure = 0;
    char.illness_progress = 0;
    char.has_illness = false;
    char.has_hypothermia = false;
    char.has_frostbite.clear();
    char.has_heat_exhaustion = false;
    char.has_heat_stroke = false;
    char.food_sick = false;

    db.save_character_data(char.clone())?;

    // Mirror the saved character back into the live session so the client
    // sees the new room/HP without waiting for the next save.
    {
        let mut conns_guard = connections.lock().unwrap();
        for (_id, session) in conns_guard.iter_mut() {
            if let Some(ref mut sc) = session.character {
                if sc.name.eq_ignore_ascii_case(&char_name) {
                    *sc = char.clone();
                    break;
                }
            }
        }
    }

    send_client_message(
        connections,
        connection_id_str.to_string(),
        "You feel yourself drawn back to safety.".to_string(),
    );
    broadcast_to_room(
        connections,
        spawn_room,
        format!("{} appears in a flash, gasping for breath.", char_name),
        Some(&char_name),
    );

    Ok(())
}

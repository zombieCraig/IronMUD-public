// src/script/mod.rs
// Rhai scripting engine registration for IronMUD

mod ai;
mod api_keys;
mod areas;
mod bugs;
mod characters;
mod combat;
mod crafting;
mod fishing;
mod garden;
mod groups;
mod healers;
mod items;
#[macro_use]
pub mod macros;
mod mail;
mod medical;
mod mobiles;
mod property;
mod rooms;
mod shop_presets;
mod shops;
mod simulation;
pub mod social;
mod spawn;
mod spells;
mod stealth;
mod transport;
mod triggers;
mod utilities;

pub use ai::{set_chat_sender, set_claude_sender, set_gemini_sender};
pub use areas::check_build_mode;
pub use combat::{apply_damage_reduction, apply_mobile_on_hit_dots};
pub use crafting::build_crafted_item_from_prototype;
pub use macros::parse_uuid_or_none;
pub use mobiles::{MEMORY_CAP, MEMORY_DURATION_SECS, check_and_prune_memory, record_mob_memory};
pub use stealth::is_player_visible_to_mob;
pub use triggers::execute_room_template;
pub use triggers::fire_mobile_triggers_from_rust;

use crate::SharedConnections;
use crate::SharedState;
use crate::db::Db;
use crate::{
    AreaData, AreaFlags, AreaPermission, CharacterData, DoorState, ExtraDesc, OnlinePlayer, RoomData, RoomExits,
    RoomFlags,
};
use rhai::{Engine, EvalAltResult, Position};
use std::sync::Arc;

pub fn register_rhai_functions(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections, state: SharedState) {
    // Register CharacterData type with getters/setters
    engine
        .register_type_with_name::<CharacterData>("CharacterData")
        .register_get("name", |c: &mut CharacterData| c.name.clone())
        .register_set("name", |c: &mut CharacterData, val: String| c.name = val)
        .register_get("password_hash", |c: &mut CharacterData| c.password_hash.clone())
        .register_get("current_room_id", |c: &mut CharacterData| c.current_room_id.to_string())
        .register_set("current_room_id", |c: &mut CharacterData, val: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&val) {
                c.current_room_id = uuid;
            }
        })
        .register_get("is_builder", |c: &mut CharacterData| c.is_builder)
        .register_get("is_admin", |c: &mut CharacterData| c.is_admin)
        .register_get("god_mode", |c: &mut CharacterData| c.god_mode)
        .register_get("build_mode", |c: &mut CharacterData| c.build_mode)
        .register_get("level", |c: &mut CharacterData| c.level as i64)
        .register_set("level", |c: &mut CharacterData, val: i64| c.level = val as i32)
        .register_get("gold", |c: &mut CharacterData| c.gold as i64)
        .register_set("gold", |c: &mut CharacterData, val: i64| c.gold = val as i32)
        .register_get("bank_gold", |c: &mut CharacterData| c.bank_gold)
        // Character creation wizard field getters/setters
        .register_get("race", |c: &mut CharacterData| c.race.clone())
        .register_set("race", |c: &mut CharacterData, val: String| c.race = val)
        .register_get("short_description", |c: &mut CharacterData| c.short_description.clone())
        .register_set("short_description", |c: &mut CharacterData, val: String| {
            c.short_description = val
        })
        .register_get("class_name", |c: &mut CharacterData| c.class_name.clone())
        .register_set("class_name", |c: &mut CharacterData, val: String| c.class_name = val)
        .register_get("traits", |c: &mut CharacterData| {
            c.traits
                .iter()
                .map(|s| rhai::Dynamic::from(s.clone()))
                .collect::<Vec<_>>()
        })
        .register_set("traits", |c: &mut CharacterData, val: rhai::Array| {
            c.traits = val.into_iter().filter_map(|d| d.try_cast::<String>()).collect();
        })
        .register_get("trait_points", |c: &mut CharacterData| c.trait_points as i64)
        .register_set("trait_points", |c: &mut CharacterData, val: i64| {
            c.trait_points = val as i32
        })
        .register_get("creation_complete", |c: &mut CharacterData| c.creation_complete)
        .register_set("creation_complete", |c: &mut CharacterData, val: bool| {
            c.creation_complete = val
        })
        // Thirst system getters/setters
        .register_get("thirst", |c: &mut CharacterData| c.thirst as i64)
        .register_set("thirst", |c: &mut CharacterData, val: i64| c.thirst = val as i32)
        .register_get("max_thirst", |c: &mut CharacterData| c.max_thirst as i64)
        .register_set("max_thirst", |c: &mut CharacterData, val: i64| {
            c.max_thirst = val as i32
        })
        // Hunger system getters/setters
        .register_get("hunger", |c: &mut CharacterData| c.hunger as i64)
        .register_set("hunger", |c: &mut CharacterData, val: i64| c.hunger = val as i32)
        .register_get("max_hunger", |c: &mut CharacterData| c.max_hunger as i64)
        .register_set("max_hunger", |c: &mut CharacterData, val: i64| {
            c.max_hunger = val as i32
        })
        // HP system getters/setters
        .register_get("hp", |c: &mut CharacterData| c.hp as i64)
        .register_set("hp", |c: &mut CharacterData, val: i64| c.hp = val as i32)
        .register_get("max_hp", |c: &mut CharacterData| c.max_hp as i64)
        .register_set("max_hp", |c: &mut CharacterData, val: i64| c.max_hp = val as i32)
        // Prompt settings
        .register_get("prompt_mode", |c: &mut CharacterData| c.prompt_mode.clone())
        .register_set("prompt_mode", |c: &mut CharacterData, val: String| c.prompt_mode = val)
        // Password management
        .register_get("must_change_password", |c: &mut CharacterData| c.must_change_password)
        .register_set("must_change_password", |c: &mut CharacterData, val: bool| {
            c.must_change_password = val
        })
        // Builder mode: show room flags
        .register_get("show_room_flags", |c: &mut CharacterData| c.show_room_flags)
        .register_set("show_room_flags", |c: &mut CharacterData, val: bool| {
            c.show_room_flags = val
        })
        // Stamina system
        .register_get("stamina", |c: &mut CharacterData| c.stamina as i64)
        .register_set("stamina", |c: &mut CharacterData, val: i64| c.stamina = val as i32)
        .register_get("max_stamina", |c: &mut CharacterData| c.max_stamina as i64)
        .register_set("max_stamina", |c: &mut CharacterData, val: i64| {
            c.max_stamina = val as i32
        })
        // Mana system
        .register_get("mana", |c: &mut CharacterData| c.mana as i64)
        .register_set("mana", |c: &mut CharacterData, val: i64| c.mana = val as i32)
        .register_get("max_mana", |c: &mut CharacterData| c.max_mana as i64)
        .register_set("max_mana", |c: &mut CharacterData, val: i64| c.max_mana = val as i32)
        .register_get("mana_enabled", |c: &mut CharacterData| c.mana_enabled)
        .register_set("mana_enabled", |c: &mut CharacterData, val: bool| c.mana_enabled = val)
        // Breath/drowning system
        .register_get("breath", |c: &mut CharacterData| c.breath as i64)
        .register_set("breath", |c: &mut CharacterData, val: i64| c.breath = val as i32)
        .register_get("max_breath", |c: &mut CharacterData| c.max_breath as i64)
        .register_set("max_breath", |c: &mut CharacterData, val: i64| {
            c.max_breath = val as i32
        })
        // Character stats
        .register_get("stat_str", |c: &mut CharacterData| c.stat_str as i64)
        .register_set("stat_str", |c: &mut CharacterData, val: i64| c.stat_str = val as i32)
        .register_get("stat_dex", |c: &mut CharacterData| c.stat_dex as i64)
        .register_set("stat_dex", |c: &mut CharacterData, val: i64| c.stat_dex = val as i32)
        .register_get("stat_con", |c: &mut CharacterData| c.stat_con as i64)
        .register_set("stat_con", |c: &mut CharacterData, val: i64| c.stat_con = val as i32)
        .register_get("stat_int", |c: &mut CharacterData| c.stat_int as i64)
        .register_set("stat_int", |c: &mut CharacterData, val: i64| c.stat_int = val as i32)
        .register_get("stat_wis", |c: &mut CharacterData| c.stat_wis as i64)
        .register_set("stat_wis", |c: &mut CharacterData, val: i64| c.stat_wis = val as i32)
        .register_get("stat_cha", |c: &mut CharacterData| c.stat_cha as i64)
        .register_set("stat_cha", |c: &mut CharacterData, val: i64| c.stat_cha = val as i32)
        .register_get("position", |c: &mut CharacterData| c.position.to_string())
        .register_set("position", |c: &mut CharacterData, val: String| {
            c.position = match val.to_lowercase().as_str() {
                "sitting" => crate::CharacterPosition::Sitting,
                "sleeping" => crate::CharacterPosition::Sleeping,
                "swimming" => crate::CharacterPosition::Swimming,
                _ => crate::CharacterPosition::Standing,
            };
        })
        // Group/Party system
        .register_get("following", |c: &mut CharacterData| {
            c.following.clone().unwrap_or_default()
        })
        .register_set("following", |c: &mut CharacterData, val: String| {
            c.following = if val.is_empty() { None } else { Some(val) };
        })
        .register_get("following_mobile_id", |c: &mut CharacterData| {
            c.following_mobile_id.map(|u| u.to_string()).unwrap_or_default()
        })
        .register_set("following_mobile_id", |c: &mut CharacterData, val: String| {
            c.following_mobile_id = if val.is_empty() {
                None
            } else {
                uuid::Uuid::parse_str(&val).ok()
            };
        })
        .register_get("is_grouped", |c: &mut CharacterData| c.is_grouped)
        .register_set("is_grouped", |c: &mut CharacterData, val: bool| c.is_grouped = val)
        // Weather exposure and medical conditions
        .register_get("is_wet", |c: &mut CharacterData| c.is_wet)
        .register_set("is_wet", |c: &mut CharacterData, val: bool| c.is_wet = val)
        .register_get("wet_level", |c: &mut CharacterData| c.wet_level as i64)
        .register_set("wet_level", |c: &mut CharacterData, val: i64| c.wet_level = val as i32)
        .register_get("cold_exposure", |c: &mut CharacterData| c.cold_exposure as i64)
        .register_set("cold_exposure", |c: &mut CharacterData, val: i64| {
            c.cold_exposure = val as i32
        })
        .register_get("heat_exposure", |c: &mut CharacterData| c.heat_exposure as i64)
        .register_set("heat_exposure", |c: &mut CharacterData, val: i64| {
            c.heat_exposure = val as i32
        })
        .register_get("has_hypothermia", |c: &mut CharacterData| c.has_hypothermia)
        .register_set("has_hypothermia", |c: &mut CharacterData, val: bool| {
            c.has_hypothermia = val
        })
        .register_get("has_frostbite", |c: &mut CharacterData| {
            c.has_frostbite
                .iter()
                .map(|bp| rhai::Dynamic::from(format!("{:?}", bp)))
                .collect::<Vec<_>>()
        })
        .register_set("has_frostbite", |c: &mut CharacterData, val: rhai::Array| {
            use crate::BodyPart;
            c.has_frostbite = val
                .into_iter()
                .filter_map(|d| {
                    d.try_cast::<String>().and_then(|s| match s.as_str() {
                        "Head" => Some(BodyPart::Head),
                        "Neck" => Some(BodyPart::Neck),
                        "Torso" => Some(BodyPart::Torso),
                        "LeftArm" => Some(BodyPart::LeftArm),
                        "RightArm" => Some(BodyPart::RightArm),
                        "LeftLeg" => Some(BodyPart::LeftLeg),
                        "RightLeg" => Some(BodyPart::RightLeg),
                        "LeftHand" => Some(BodyPart::LeftHand),
                        "RightHand" => Some(BodyPart::RightHand),
                        "LeftFoot" => Some(BodyPart::LeftFoot),
                        "RightFoot" => Some(BodyPart::RightFoot),
                        _ => None,
                    })
                })
                .collect();
        })
        // Unconscious/bleedout state
        .register_get("is_unconscious", |c: &mut CharacterData| c.is_unconscious)
        .register_set("is_unconscious", |c: &mut CharacterData, val: bool| {
            c.is_unconscious = val
        })
        .register_get("bleedout_rounds_remaining", |c: &mut CharacterData| {
            c.bleedout_rounds_remaining as i64
        })
        .register_set("bleedout_rounds_remaining", |c: &mut CharacterData, val: i64| {
            c.bleedout_rounds_remaining = val as i32
        })
        .register_get("has_heat_exhaustion", |c: &mut CharacterData| c.has_heat_exhaustion)
        .register_set("has_heat_exhaustion", |c: &mut CharacterData, val: bool| {
            c.has_heat_exhaustion = val
        })
        .register_get("has_heat_stroke", |c: &mut CharacterData| c.has_heat_stroke)
        .register_set("has_heat_stroke", |c: &mut CharacterData, val: bool| {
            c.has_heat_stroke = val
        })
        .register_get("has_illness", |c: &mut CharacterData| c.has_illness)
        .register_set("has_illness", |c: &mut CharacterData, val: bool| c.has_illness = val)
        .register_get("illness_progress", |c: &mut CharacterData| c.illness_progress as i64)
        .register_set("illness_progress", |c: &mut CharacterData, val: i64| {
            c.illness_progress = val as i32
        })
        .register_get("food_sick", |c: &mut CharacterData| c.food_sick)
        .register_set("food_sick", |c: &mut CharacterData, val: bool| c.food_sick = val)
        // Property tour state
        .register_get("tour_origin_room", |c: &mut CharacterData| {
            c.tour_origin_room.map(|u| u.to_string()).unwrap_or_default()
        })
        .register_set("tour_origin_room", |c: &mut CharacterData, val: String| {
            c.tour_origin_room = if val.is_empty() {
                None
            } else {
                uuid::Uuid::parse_str(&val).ok()
            };
        })
        .register_get("on_tour", |c: &mut CharacterData| c.on_tour)
        .register_set("on_tour", |c: &mut CharacterData, val: bool| c.on_tour = val)
        // Spawn room
        .register_get("spawn_room_id", |c: &mut CharacterData| {
            c.spawn_room_id.map(|u| u.to_string()).unwrap_or_default()
        })
        .register_set("spawn_room_id", |c: &mut CharacterData, val: String| {
            c.spawn_room_id = if val.is_empty() {
                None
            } else {
                uuid::Uuid::parse_str(&val).ok()
            };
        })
        // Racial cooldowns (ability_id -> unix timestamp)
        .register_get("racial_cooldowns", |c: &mut CharacterData| -> rhai::Map {
            c.racial_cooldowns
                .iter()
                .map(|(k, v)| (k.clone().into(), rhai::Dynamic::from(*v)))
                .collect()
        })
        .register_set("racial_cooldowns", |c: &mut CharacterData, val: rhai::Map| {
            c.racial_cooldowns = val
                .into_iter()
                .filter_map(|(k, v)| v.as_int().ok().map(|i| (k.to_string(), i)))
                .collect();
        })
        // Spell system fields
        .register_get("learned_spells", |c: &mut CharacterData| -> rhai::Array {
            c.learned_spells
                .iter()
                .map(|s| rhai::Dynamic::from(s.clone()))
                .collect()
        })
        .register_set("learned_spells", |c: &mut CharacterData, val: rhai::Array| {
            c.learned_spells = val.into_iter().filter_map(|d| d.try_cast::<String>()).collect();
        })
        .register_get("spell_cooldowns", |c: &mut CharacterData| -> rhai::Map {
            c.spell_cooldowns
                .iter()
                .map(|(k, v)| (k.clone().into(), rhai::Dynamic::from(*v)))
                .collect()
        })
        .register_set("spell_cooldowns", |c: &mut CharacterData, val: rhai::Map| {
            c.spell_cooldowns = val
                .into_iter()
                .filter_map(|(k, v)| v.as_int().ok().map(|i| (k.to_string(), i)))
                .collect();
        });

    // Register CharacterData constructor
    engine.register_fn(
        "new_character",
        |name: String, password_hash: String, room_id: String| {
            CharacterData {
                name,
                password_hash,
                current_room_id: uuid::Uuid::parse_str(&room_id).unwrap_or_default(),
                aliases: std::collections::HashMap::new(),
                is_builder: false,
                is_admin: false,
                god_mode: false,
                build_mode: false,
                level: 1,
                gold: 0,
                bank_gold: 0,
                // Character creation wizard fields
                race: String::new(),
                short_description: String::new(),
                class_name: "unemployed".to_string(),
                traits: Vec::new(),
                trait_points: 10,
                creation_complete: false,
                // Thirst system fields
                thirst: 100,
                max_thirst: 100,
                last_thirst_tick: 0,
                // Hunger system fields
                hunger: 100,
                max_hunger: 100,
                last_hunger_tick: 0,
                // HP system fields
                hp: 100,
                max_hp: 100,
                // Prompt settings
                prompt_mode: String::new(),
                // Password management
                must_change_password: false,
                // Builder mode: show room flags (persisted)
                show_room_flags: false,
                // Builder debug channel
                builder_debug_enabled: false,
                // Stamina system
                stamina: 100,
                max_stamina: 100,
                position: crate::CharacterPosition::Standing,
                // Skill system
                skills: std::collections::HashMap::new(),
                // Learned recipes
                learned_recipes: std::collections::HashSet::new(),
                // Foraging cooldown
                foraged_rooms: std::collections::HashMap::new(),
                // Group/Party system
                following: None,
                following_mobile_id: None,
                is_grouped: false,
                // Character stats (default 10)
                stat_str: 10,
                stat_dex: 10,
                stat_con: 10,
                stat_int: 10,
                stat_wis: 10,
                stat_cha: 10,
                // Combat system fields
                spawn_room_id: None,
                combat: crate::CombatState::default(),
                wounds: Vec::new(),
                ongoing_effects: Vec::new(),
                scars: std::collections::HashMap::new(),
                // Death/unconscious state (not persisted)
                is_unconscious: false,
                bleedout_rounds_remaining: 0,
                // Weather exposure status (transient)
                is_wet: false,
                wet_level: 0,
                cold_exposure: 0,
                heat_exposure: 0,
                // Environmental conditions (persisted)
                illness_progress: 0,
                has_hypothermia: false,
                has_frostbite: Vec::new(),
                has_heat_exhaustion: false,
                has_heat_stroke: false,
                has_illness: false,
                food_sick: false,
                // Helpline channel subscription
                helpline_enabled: false,
                // Property rental system
                active_leases: std::collections::HashMap::new(),
                escrow_ids: Vec::new(),
                tour_origin_room: None,
                on_tour: false,
                // Buff system fields
                active_buffs: Vec::new(),
                mana: 0,
                max_mana: 0,
                mana_enabled: false,
                drunk_level: 0,
                racial_cooldowns: std::collections::HashMap::new(),
                learned_spells: Vec::new(),
                spell_cooldowns: std::collections::HashMap::new(),
                // Breath/drowning system
                breath: 100,
                max_breath: 100,
                // Stealth system fields
                is_hidden: false,
                is_sneaking: false,
                is_camouflaged: false,
                hunting_target: String::new(),
                envenomed_charges: 0,
                circle_cooldown: 0,
                theft_cooldowns: std::collections::HashMap::new(),
            }
        },
    );

    // Register RoomExits type with getters (returns empty string for None)
    engine
        .register_type_with_name::<RoomExits>("RoomExits")
        .register_get("north", |e: &mut RoomExits| {
            e.north.map(|u| u.to_string()).unwrap_or_default()
        })
        .register_get("east", |e: &mut RoomExits| {
            e.east.map(|u| u.to_string()).unwrap_or_default()
        })
        .register_get("south", |e: &mut RoomExits| {
            e.south.map(|u| u.to_string()).unwrap_or_default()
        })
        .register_get("west", |e: &mut RoomExits| {
            e.west.map(|u| u.to_string()).unwrap_or_default()
        })
        .register_get("up", |e: &mut RoomExits| {
            e.up.map(|u| u.to_string()).unwrap_or_default()
        })
        .register_get("down", |e: &mut RoomExits| {
            e.down.map(|u| u.to_string()).unwrap_or_default()
        })
        .register_get("out", |e: &mut RoomExits| {
            e.out.map(|u| u.to_string()).unwrap_or_default()
        })
        .register_get("custom", |e: &mut RoomExits| {
            // Convert HashMap<String, Uuid> to rhai::Map for script access
            let mut map = rhai::Map::new();
            for (key, value) in &e.custom {
                map.insert(key.clone().into(), value.to_string().into());
            }
            map
        });

    // Register RoomFlags type with getters
    engine
        .register_type_with_name::<RoomFlags>("RoomFlags")
        .register_get("dark", |f: &mut RoomFlags| f.dark)
        .register_get("combat_zone", |f: &mut RoomFlags| {
            f.combat_zone
                .map(|z| z.to_display_string().to_string())
                .unwrap_or_else(|| "inherit".to_string())
        })
        .register_get("no_mob", |f: &mut RoomFlags| f.no_mob)
        .register_get("indoors", |f: &mut RoomFlags| f.indoors)
        .register_get("underwater", |f: &mut RoomFlags| f.underwater)
        // Climate/weather flags
        .register_get("climate_controlled", |f: &mut RoomFlags| f.climate_controlled)
        .register_get("always_hot", |f: &mut RoomFlags| f.always_hot)
        .register_get("always_cold", |f: &mut RoomFlags| f.always_cold)
        .register_get("city", |f: &mut RoomFlags| f.city)
        .register_get("no_windows", |f: &mut RoomFlags| f.no_windows)
        // Stamina system
        .register_get("difficult_terrain", |f: &mut RoomFlags| f.difficult_terrain)
        // Foraging
        .register_get("dirt_floor", |f: &mut RoomFlags| f.dirt_floor)
        // Mail system
        .register_get("post_office", |f: &mut RoomFlags| f.post_office)
        // Banking system
        .register_get("bank", |f: &mut RoomFlags| f.bank)
        // Gardening
        .register_get("garden", |f: &mut RoomFlags| f.garden)
        // Recall system
        .register_get("spawn_point", |f: &mut RoomFlags| f.spawn_point)
        // Water system flags
        .register_get("shallow_water", |f: &mut RoomFlags| f.shallow_water)
        .register_get("deep_water", |f: &mut RoomFlags| f.deep_water)
        // Migrant housing
        .register_get("liveable", |f: &mut RoomFlags| f.liveable)
        // CircleMUD parity flags
        // (Field is `private_room`, not `private` — `private` is a Rhai 1.x
        // reserved keyword and can't be used as a property accessor.)
        .register_get("private_room", |f: &mut RoomFlags| f.private_room)
        .register_get("tunnel", |f: &mut RoomFlags| f.tunnel)
        .register_get("death", |f: &mut RoomFlags| f.death)
        .register_get("no_magic", |f: &mut RoomFlags| f.no_magic)
        .register_get("soundproof", |f: &mut RoomFlags| f.soundproof)
        .register_get("notrack", |f: &mut RoomFlags| f.notrack);

    // Register ExtraDesc type with getters
    engine
        .register_type_with_name::<ExtraDesc>("ExtraDesc")
        .register_get("keywords", |e: &mut ExtraDesc| {
            e.keywords
                .iter()
                .map(|s| rhai::Dynamic::from(s.clone()))
                .collect::<Vec<_>>()
        })
        .register_get("description", |e: &mut ExtraDesc| e.description.clone());

    // Register AreaFlags type with getters
    engine
        .register_type_with_name::<AreaFlags>("AreaFlags")
        .register_get("climate_controlled", |f: &mut AreaFlags| f.climate_controlled);

    // Register AreaData type with getters
    engine
        .register_type_with_name::<AreaData>("AreaData")
        .register_get("id", |a: &mut AreaData| a.id.to_string())
        .register_get("name", |a: &mut AreaData| a.name.clone())
        .register_get("prefix", |a: &mut AreaData| a.prefix.clone())
        .register_get("description", |a: &mut AreaData| a.description.clone())
        .register_get("level_min", |a: &mut AreaData| a.level_min as i64)
        .register_get("level_max", |a: &mut AreaData| a.level_max as i64)
        .register_get("theme", |a: &mut AreaData| a.theme.clone())
        .register_get("owner", |a: &mut AreaData| a.owner.clone().unwrap_or_default())
        .register_get("permission_level", |a: &mut AreaData| match a.permission_level {
            AreaPermission::OwnerOnly => "owner_only".to_string(),
            AreaPermission::Trusted => "trusted".to_string(),
            AreaPermission::AllBuilders => "all_builders".to_string(),
        })
        .register_get("trusted_builders", |a: &mut AreaData| {
            a.trusted_builders
                .iter()
                .map(|s| rhai::Dynamic::from(s.clone()))
                .collect::<Vec<_>>()
        })
        .register_get("city_forage_table", |a: &mut AreaData| {
            a.city_forage_table
                .iter()
                .map(|entry| {
                    let mut map = rhai::Map::new();
                    map.insert("vnum".into(), rhai::Dynamic::from(entry.vnum.clone()));
                    map.insert("min_skill".into(), rhai::Dynamic::from(entry.min_skill as i64));
                    map.insert("rarity".into(), rhai::Dynamic::from(entry.rarity.clone()));
                    rhai::Dynamic::from(map)
                })
                .collect::<Vec<_>>()
        })
        .register_get("wilderness_forage_table", |a: &mut AreaData| {
            a.wilderness_forage_table
                .iter()
                .map(|entry| {
                    let mut map = rhai::Map::new();
                    map.insert("vnum".into(), rhai::Dynamic::from(entry.vnum.clone()));
                    map.insert("min_skill".into(), rhai::Dynamic::from(entry.min_skill as i64));
                    map.insert("rarity".into(), rhai::Dynamic::from(entry.rarity.clone()));
                    rhai::Dynamic::from(map)
                })
                .collect::<Vec<_>>()
        })
        .register_get("shallow_water_forage_table", |a: &mut AreaData| {
            a.shallow_water_forage_table
                .iter()
                .map(|entry| {
                    let mut map = rhai::Map::new();
                    map.insert("vnum".into(), rhai::Dynamic::from(entry.vnum.clone()));
                    map.insert("min_skill".into(), rhai::Dynamic::from(entry.min_skill as i64));
                    map.insert("rarity".into(), rhai::Dynamic::from(entry.rarity.clone()));
                    rhai::Dynamic::from(map)
                })
                .collect::<Vec<_>>()
        })
        .register_get("deep_water_forage_table", |a: &mut AreaData| {
            a.deep_water_forage_table
                .iter()
                .map(|entry| {
                    let mut map = rhai::Map::new();
                    map.insert("vnum".into(), rhai::Dynamic::from(entry.vnum.clone()));
                    map.insert("min_skill".into(), rhai::Dynamic::from(entry.min_skill as i64));
                    map.insert("rarity".into(), rhai::Dynamic::from(entry.rarity.clone()));
                    rhai::Dynamic::from(map)
                })
                .collect::<Vec<_>>()
        })
        .register_get("underwater_forage_table", |a: &mut AreaData| {
            a.underwater_forage_table
                .iter()
                .map(|entry| {
                    let mut map = rhai::Map::new();
                    map.insert("vnum".into(), rhai::Dynamic::from(entry.vnum.clone()));
                    map.insert("min_skill".into(), rhai::Dynamic::from(entry.min_skill as i64));
                    map.insert("rarity".into(), rhai::Dynamic::from(entry.rarity.clone()));
                    rhai::Dynamic::from(map)
                })
                .collect::<Vec<_>>()
        })
        .register_get("flags", |a: &mut AreaData| a.flags.clone())
        .register_get("default_room_flags", |a: &mut AreaData| a.default_room_flags.clone())
        // Migrant immigration system
        .register_get("immigration_enabled", |a: &mut AreaData| a.immigration_enabled)
        .register_get("immigration_room_vnum", |a: &mut AreaData| {
            a.immigration_room_vnum.clone()
        })
        .register_get("immigration_name_pool", |a: &mut AreaData| {
            a.immigration_name_pool.clone()
        })
        .register_get("immigration_visual_profile", |a: &mut AreaData| {
            a.immigration_visual_profile.clone()
        })
        .register_get("migration_interval_days", |a: &mut AreaData| {
            a.migration_interval_days as i64
        })
        .register_get("migration_max_per_check", |a: &mut AreaData| {
            a.migration_max_per_check as i64
        })
        .register_get("last_migration_check_day", |a: &mut AreaData| {
            a.last_migration_check_day.unwrap_or(-1)
        })
        .register_get("immigration_guard_chance", |a: &mut AreaData| {
            a.immigration_variation_chances.guard as f64
        })
        .register_get("immigration_healer_chance", |a: &mut AreaData| {
            a.immigration_variation_chances.healer as f64
        })
        .register_get("immigration_scavenger_chance", |a: &mut AreaData| {
            a.immigration_variation_chances.scavenger as f64
        })
        .register_get("immigration_family_parent_child_chance", |a: &mut AreaData| {
            a.immigration_family_chance.parent_child as f64
        })
        .register_get("immigration_family_sibling_pair_chance", |a: &mut AreaData| {
            a.immigration_family_chance.sibling_pair as f64
        })
        .register_get("migrant_starting_gold_min", |a: &mut AreaData| {
            a.migrant_starting_gold.min as i64
        })
        .register_get("migrant_starting_gold_max", |a: &mut AreaData| {
            a.migrant_starting_gold.max as i64
        })
        .register_get("guard_wage_per_hour", |a: &mut AreaData| a.guard_wage_per_hour as i64)
        .register_get("healer_wage_per_hour", |a: &mut AreaData| a.healer_wage_per_hour as i64)
        .register_get("scavenger_wage_per_hour", |a: &mut AreaData| {
            a.scavenger_wage_per_hour as i64
        });

    // Register RoomData type with getters
    engine
        .register_type_with_name::<RoomData>("RoomData")
        .register_get("id", |r: &mut RoomData| r.id.to_string())
        .register_get("title", |r: &mut RoomData| r.title.clone())
        .register_get("description", |r: &mut RoomData| r.description.clone())
        .register_get("exits", |r: &mut RoomData| r.exits.clone())
        .register_get("flags", |r: &mut RoomData| r.flags.clone())
        .register_get("extra_descs", |r: &mut RoomData| {
            r.extra_descs
                .iter()
                .map(|e| rhai::Dynamic::from(e.clone()))
                .collect::<Vec<_>>()
        })
        .register_get("vnum", |r: &mut RoomData| r.vnum.clone().unwrap_or_default())
        .register_get("area_id", |r: &mut RoomData| {
            r.area_id.map(|u| u.to_string()).unwrap_or_default()
        })
        .register_get("spring_desc", |r: &mut RoomData| {
            r.spring_desc.clone().unwrap_or_default()
        })
        .register_get("summer_desc", |r: &mut RoomData| {
            r.summer_desc.clone().unwrap_or_default()
        })
        .register_get("autumn_desc", |r: &mut RoomData| {
            r.autumn_desc.clone().unwrap_or_default()
        })
        .register_get("winter_desc", |r: &mut RoomData| {
            r.winter_desc.clone().unwrap_or_default()
        })
        .register_get("dynamic_desc", |r: &mut RoomData| {
            r.dynamic_desc.clone().unwrap_or_default()
        })
        .register_get("is_property_template", |r: &mut RoomData| r.is_property_template)
        .register_get("property_template_id", |r: &mut RoomData| {
            r.property_template_id.map(|u| u.to_string()).unwrap_or_default()
        })
        .register_get("is_template_entrance", |r: &mut RoomData| r.is_template_entrance)
        .register_get("property_lease_id", |r: &mut RoomData| {
            r.property_lease_id.map(|u| u.to_string()).unwrap_or_default()
        })
        .register_get("property_entrance", |r: &mut RoomData| r.property_entrance)
        // Migrant housing
        .register_get("living_capacity", |r: &mut RoomData| r.living_capacity as i64)
        .register_get("resident_count", |r: &mut RoomData| r.residents.len() as i64);

    // Register RoomData constructor
    engine.register_fn("new_room", |id: String, title: String, description: String| RoomData {
        id: uuid::Uuid::parse_str(&id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
        title,
        description,
        exits: RoomExits::default(),
        flags: RoomFlags::default(),
        extra_descs: Vec::new(),
        vnum: None,
        area_id: None,
        triggers: Vec::new(),
        doors: std::collections::HashMap::new(),
        spring_desc: None,
        summer_desc: None,
        autumn_desc: None,
        winter_desc: None,
        dynamic_desc: None,
        water_type: crate::WaterType::None,
        catch_table: Vec::new(),
        is_property_template: false,
        property_template_id: None,
        is_template_entrance: false,
        property_lease_id: None,
        property_entrance: false,
        recent_departures: Vec::new(),
        blood_trails: Vec::new(),
        traps: Vec::new(),
        living_capacity: 0,
        residents: Vec::new(),
    });

    // Register get_available_exits helper
    engine.register_fn("get_available_exits", |room: RoomData| {
        let mut exits = Vec::new();
        if room.exits.north.is_some() {
            exits.push(rhai::Dynamic::from("north"));
        }
        if room.exits.east.is_some() {
            exits.push(rhai::Dynamic::from("east"));
        }
        if room.exits.south.is_some() {
            exits.push(rhai::Dynamic::from("south"));
        }
        if room.exits.west.is_some() {
            exits.push(rhai::Dynamic::from("west"));
        }
        if room.exits.up.is_some() {
            exits.push(rhai::Dynamic::from("up"));
        }
        if room.exits.down.is_some() {
            exits.push(rhai::Dynamic::from("down"));
        }
        exits
    });

    // Register DoorState type with getters
    engine
        .register_type_with_name::<DoorState>("DoorState")
        .register_get("name", |d: &mut DoorState| d.name.clone())
        .register_get("is_closed", |d: &mut DoorState| d.is_closed)
        .register_get("is_locked", |d: &mut DoorState| d.is_locked)
        .register_get("key_vnum", |d: &mut DoorState| d.key_vnum.clone().unwrap_or_default())
        .register_get("description", |d: &mut DoorState| {
            d.description.clone().unwrap_or_default()
        })
        .register_get("keywords", |d: &mut DoorState| {
            d.keywords
                .iter()
                .map(|s: &String| rhai::Dynamic::from(s.clone()))
                .collect::<Vec<_>>()
        })
        .register_get("pickproof", |d: &mut DoorState| d.pickproof);

    // Register OnlinePlayer type with getters
    engine
        .register_type_with_name::<OnlinePlayer>("OnlinePlayer")
        .register_get("name", |p: &mut OnlinePlayer| p.name.clone())
        .register_get("room_id", |p: &mut OnlinePlayer| p.room_id.to_string())
        .register_get("addr", |p: &mut OnlinePlayer| p.addr.clone());

    // Register get_online_players function
    let conns = connections.clone();
    engine.register_fn("get_online_players", move || {
        crate::get_online_players(&conns)
            .into_iter()
            .map(rhai::Dynamic::from)
            .collect::<Vec<_>>()
    });

    // Register find_player_connection_by_name function
    let conns = connections.clone();
    engine.register_fn("find_player_connection_by_name", move |name: String| {
        crate::find_player_connection_by_name(&conns, &name)
            .map(|id| id.to_string())
            .unwrap_or_default()
    });

    // Register Db functions
    let cloned_db = db.clone();
    engine.register_fn("get_character_data", move |name: String| {
        cloned_db
            .get_character_data(&name)
            .map_err(|e| {
                Box::new(EvalAltResult::ErrorRuntime(
                    rhai::Dynamic::from(format!("DB Error: {}", e)),
                    Position::NONE,
                ))
            })
            .map(|opt| opt.map(rhai::Dynamic::from))
            .map(|opt| opt.unwrap_or_else(|| rhai::Dynamic::UNIT))
    });

    let cloned_db = db.clone();
    engine.register_fn("save_character_data", move |character: CharacterData| {
        cloned_db
            .save_character_data(character)
            .map_err(|e| {
                Box::new(EvalAltResult::ErrorRuntime(
                    rhai::Dynamic::from(format!("DB Error: {}", e)),
                    Position::NONE,
                ))
            })
            .map(|_| rhai::Dynamic::UNIT)
    });

    let cloned_db = db.clone();
    engine.register_fn("hash_password", move |password: String| {
        cloned_db
            .hash_password(&password)
            .map_err(|e| {
                Box::new(EvalAltResult::ErrorRuntime(
                    rhai::Dynamic::from(format!("Hashing Error: {}", e)),
                    Position::NONE,
                ))
            })
            .map(rhai::Dynamic::from)
    });

    let cloned_db = db.clone();
    engine.register_fn("verify_password", move |password: String, hash: String| {
        cloned_db
            .verify_password(&password, &hash)
            .map_err(|e| {
                Box::new(EvalAltResult::ErrorRuntime(
                    rhai::Dynamic::from(format!("Verification Error: {}", e)),
                    Position::NONE,
                ))
            })
            .map(rhai::Dynamic::from)
    });

    // Register room database functions
    let cloned_db = db.clone();
    engine.register_fn("get_room_data", move |room_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            cloned_db
                .get_room_data(&uuid)
                .map_err(|e| {
                    Box::new(EvalAltResult::ErrorRuntime(
                        rhai::Dynamic::from(format!("DB Error: {}", e)),
                        Position::NONE,
                    ))
                })
                .map(|opt| opt.map(rhai::Dynamic::from))
                .map(|opt| opt.unwrap_or_else(|| rhai::Dynamic::UNIT))
        } else {
            Ok(rhai::Dynamic::UNIT)
        }
    });

    let cloned_db = db.clone();
    engine.register_fn("save_room_data", move |room: RoomData| {
        cloned_db
            .save_room_data(room)
            .map_err(|e| {
                Box::new(EvalAltResult::ErrorRuntime(
                    rhai::Dynamic::from(format!("DB Error: {}", e)),
                    Position::NONE,
                ))
            })
            .map(|_| rhai::Dynamic::UNIT)
    });

    // Register get_characters_in_room function
    let conns = connections.clone();
    engine.register_fn("get_characters_in_room", move |room_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            crate::get_characters_in_room(&conns, uuid)
                .into_iter()
                .map(rhai::Dynamic::from)
                .collect::<Vec<_>>()
        } else {
            vec![]
        }
    });

    // Register broadcast_to_room function
    let conns = connections.clone();
    engine.register_fn(
        "broadcast_to_room",
        move |room_id: String, message: String, exclude_name: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
                let exclude = if exclude_name.is_empty() {
                    None
                } else {
                    Some(exclude_name.as_str())
                };
                crate::broadcast_to_room(&conns, uuid, message, exclude);
            }
        },
    );

    // Register broadcast_to_room_awake function (skips sleeping players)
    let conns = connections.clone();
    engine.register_fn(
        "broadcast_to_room_awake",
        move |room_id: String, message: String, exclude_name: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
                let exclude = if exclude_name.is_empty() {
                    None
                } else {
                    Some(exclude_name.as_str())
                };
                crate::broadcast_to_room_awake(&conns, uuid, message, exclude);
            }
        },
    );

    // Register broadcast_to_room_dreaming function (different message for sleeping players)
    let conns = connections.clone();
    engine.register_fn(
        "broadcast_to_room_dreaming",
        move |room_id: String, awake_msg: String, sleeping_msg: String, exclude_name: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
                let exclude = if exclude_name.is_empty() {
                    None
                } else {
                    Some(exclude_name.as_str())
                };
                crate::broadcast_to_room_dreaming(&conns, uuid, awake_msg, sleeping_msg, exclude);
            }
        },
    );

    // Register broadcast_to_room_except function (takes connection_id instead of name)
    let conns = connections.clone();
    engine.register_fn(
        "broadcast_to_room_except",
        move |room_id: String, connection_id: String, message: String| {
            if let Ok(room_uuid) = uuid::Uuid::parse_str(&room_id) {
                if let Ok(conn_uuid) = uuid::Uuid::parse_str(&connection_id) {
                    // Look up the character name for this connection
                    let exclude_name = {
                        let conns_guard = conns.lock().unwrap();
                        conns_guard
                            .get(&conn_uuid)
                            .and_then(|session| session.character.as_ref())
                            .map(|char| char.name.clone())
                    };
                    crate::broadcast_to_room(&conns, room_uuid, message, exclude_name.as_deref());
                }
            }
        },
    );

    // broadcast_to_all(message) - Send message to all logged-in players
    let conns = connections.clone();
    engine.register_fn("broadcast_to_all", move |message: String| {
        crate::broadcast_to_all_players(&conns, &message);
    });

    // Register connection management functions (using SharedConnections, not SharedState)
    let conns = connections.clone();
    engine.register_fn(
        "set_player_character",
        move |connection_id: String, character: CharacterData| {
            crate::set_character_for_connection(&conns, connection_id, character)
        },
    );

    let conns = connections.clone();
    engine.register_fn("get_player_character", move |connection_id: String| {
        crate::get_character_for_connection(&conns, connection_id)
    });

    let conns = connections.clone();
    engine.register_fn("clear_player_character", move |connection_id: String| {
        crate::clear_player_character(&conns, connection_id)
    });

    let conns = connections.clone();
    engine.register_fn("disconnect_client", move |connection_id: String| {
        crate::disconnect_client(&conns, connection_id)
    });

    // kick_player(target_name, reason, admin_name) -> String result message
    let conns = connections.clone();
    let kick_state = state.clone();
    engine.register_fn(
        "kick_player",
        move |target_name: String, reason: String, admin_name: String| {
            // Find player by name
            let found = crate::find_player_connection_by_name(&conns, &target_name);

            if let Some(conn_id) = found {
                // Get character data and room
                let char_info = {
                    let connections = conns.lock().unwrap();
                    if let Some(session) = connections.get(&conn_id) {
                        if let Some(ref c) = session.character {
                            Some((c.clone(), c.current_room_id, session.sender.clone()))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };

                if let Some((character, room_id, sender)) = char_info {
                    let char_name = character.name.clone();

                    // Save character before kicking
                    {
                        let world = kick_state.lock().unwrap();
                        if let Err(e) = world.db.save_character_data(character) {
                            tracing::error!("Failed to save {} before kick: {}", char_name, e);
                        }
                    }

                    // Send kick message to the target
                    let _ = sender.send(format!(
                        "\n*** You have been kicked by {}: {} ***\n",
                        admin_name, reason
                    ));

                    // Broadcast to room
                    crate::broadcast_to_room(
                        &conns,
                        room_id,
                        format!("{} has been removed from the realm.", char_name),
                        Some(&char_name),
                    );

                    // Notify chat integrations (Matrix/Discord)
                    {
                        let world = kick_state.lock().unwrap();
                        if let Some(ref chat_tx) = world.chat_sender {
                            let _ = chat_tx.send(crate::chat::ChatMessage::Broadcast(format!(
                                "{} has been removed from the realm.",
                                char_name
                            )));
                        }
                    }

                    // Disconnect the player
                    let _ = crate::disconnect_client(&conns, conn_id.to_string());

                    format!("Kicked {} from the server.", target_name)
                } else {
                    format!("Player '{}' is connected but has no character.", target_name)
                }
            } else {
                format!("Player '{}' is not online.", target_name)
            }
        },
    );

    // schedule_shutdown(delay_seconds, reason, admin_name) -> bool (success)
    let shutdown_state = state.clone();
    engine.register_fn(
        "schedule_shutdown",
        move |delay_seconds: i64, reason: String, admin_name: String| {
            if delay_seconds < 0 {
                return false;
            }
            let world = shutdown_state.lock().unwrap();
            if let Some(ref sender) = world.shutdown_sender {
                let cmd = crate::ShutdownCommand {
                    delay_seconds: delay_seconds as u64,
                    reason,
                    admin_name,
                };
                sender.send(cmd).is_ok()
            } else {
                false
            }
        },
    );

    // cancel_shutdown() -> String ("cancelled", "no_shutdown_pending")
    let cancel_state = state.clone();
    engine.register_fn("cancel_shutdown", move || {
        let world = cancel_state.lock().unwrap();
        if let Some(ref sender) = world.shutdown_cancel_sender {
            // Send true to signal cancellation
            if sender.send(true).is_ok() {
                "cancelled".to_string()
            } else {
                "no_shutdown_pending".to_string()
            }
        } else {
            "no_shutdown_pending".to_string()
        }
    });

    // set_god_mode(connection_id, enabled) -> bool (success)
    // Requires the connection to be an admin
    let god_conns = connections.clone();
    let god_db = db.clone();
    engine.register_fn("set_god_mode", move |connection_id: String, enabled: bool| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns = god_conns.lock().unwrap();
            if let Some(session) = conns.get_mut(&uuid) {
                if let Some(ref mut character) = session.character {
                    // Only admins can toggle god mode
                    if !character.is_admin {
                        tracing::warn!("[SECURITY] Non-admin {} attempted set_god_mode", character.name);
                        return false;
                    }
                    character.god_mode = enabled;
                    // Save to database
                    let _ = god_db.save_character_data(character.clone());
                    return true;
                }
            }
        }
        false
    });

    // set_build_mode(connection_id, enabled) -> bool (success)
    // Requires the connection to be a builder or admin
    let build_conns = connections.clone();
    let build_db = db.clone();
    engine.register_fn("set_build_mode", move |connection_id: String, enabled: bool| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns = build_conns.lock().unwrap();
            if let Some(session) = conns.get_mut(&uuid) {
                if let Some(ref mut character) = session.character {
                    // Builders and admins can toggle build mode
                    if !character.is_builder && !character.is_admin {
                        tracing::warn!("[SECURITY] Non-builder {} attempted set_build_mode", character.name);
                        return false;
                    }
                    character.build_mode = enabled;
                    // Save to database
                    let _ = build_db.save_character_data(character.clone());
                    return true;
                }
            }
        }
        false
    });

    let conns = connections.clone();
    engine.register_fn(
        "send_client_message",
        move |connection_id_str: String, message: String| {
            crate::send_client_message(&conns, connection_id_str, message);
        },
    );

    // is_logged_in(connection_id) -> bool
    let conns = connections.clone();
    engine.register_fn("is_logged_in", move |connection_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns = conns.lock().unwrap();
            conns.get(&uuid).map(|s| s.character.is_some()).unwrap_or(false)
        } else {
            false
        }
    });

    // send_banner(connection_id) -> reads and sends assets/banner.txt
    let conns = connections.clone();
    engine.register_fn("send_banner", move |connection_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            if let Ok(banner) = std::fs::read_to_string("assets/banner.txt") {
                let conns = conns.lock().unwrap();
                if let Some(session) = conns.get(&uuid) {
                    let _ = session.sender.send(banner);
                }
            }
        }
    });

    // get_available_commands(connection_id) -> array of maps with name and description
    let conns = connections.clone();
    let cloned_state = state.clone();
    engine.register_fn("get_available_commands", move |connection_id: String| {
        // Get login status and permissions
        let (is_logged_in, is_builder, is_admin) = if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns = conns.lock().unwrap();
            conns
                .get(&uuid)
                .map(|s| {
                    let logged_in = s.character.is_some();
                    let (builder, admin) = s
                        .character
                        .as_ref()
                        .map(|c| (c.is_builder || c.is_admin, c.is_admin))
                        .unwrap_or((false, false));
                    (logged_in, builder, admin)
                })
                .unwrap_or((false, false, false))
        } else {
            (false, false, false)
        };

        let world = cloned_state.lock().unwrap();
        let mut commands: Vec<rhai::Dynamic> = Vec::new();

        for (name, meta) in &world.command_metadata {
            let accessible = match meta.access.as_str() {
                "guest" => !is_logged_in,
                "any" => true,
                "user" => is_logged_in,
                "builder" => is_builder, // Includes admins
                "admin" => is_admin,
                _ => is_logged_in,
            };

            if accessible {
                let mut map = rhai::Map::new();
                map.insert("name".into(), rhai::Dynamic::from(name.clone()));
                map.insert("description".into(), rhai::Dynamic::from(meta.description.clone()));
                map.insert("access".into(), rhai::Dynamic::from(meta.access.clone()));
                commands.push(rhai::Dynamic::from(map));
            }
        }

        // Sort by name for consistent output
        commands.sort_by(|a, b| {
            let a_name = a
                .clone()
                .try_cast::<rhai::Map>()
                .and_then(|m| m.get("name").cloned())
                .and_then(|d| d.try_cast::<String>())
                .unwrap_or_default();
            let b_name = b
                .clone()
                .try_cast::<rhai::Map>()
                .and_then(|m| m.get("name").cloned())
                .and_then(|d| d.try_cast::<String>())
                .unwrap_or_default();
            a_name.cmp(&b_name)
        });

        commands
    });

    // get_default_aliases() -> Map of default aliases
    engine.register_fn("get_default_aliases", || {
        crate::get_default_aliases()
            .into_iter()
            .map(|(k, v)| (k.into(), rhai::Dynamic::from(v)))
            .collect::<rhai::Map>()
    });

    // get_aliases(connection_id) -> Map of user's custom aliases
    let conns = connections.clone();
    engine.register_fn("get_aliases", move |connection_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns = conns.lock().unwrap();
            if let Some(session) = conns.get(&uuid) {
                if let Some(ref character) = session.character {
                    return character
                        .aliases
                        .iter()
                        .map(|(k, v)| (k.clone().into(), rhai::Dynamic::from(v.clone())))
                        .collect::<rhai::Map>();
                }
            }
        }
        rhai::Map::new()
    });

    // set_alias(connection_id, alias_name, expansion) -> bool
    let conns = connections.clone();
    let db_clone = db.clone();
    engine.register_fn(
        "set_alias",
        move |connection_id: String, alias_name: String, expansion: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
                let mut conns = conns.lock().unwrap();
                if let Some(session) = conns.get_mut(&uuid) {
                    if let Some(ref mut character) = session.character {
                        character.aliases.insert(alias_name, expansion);
                        let _ = db_clone.save_character_data(character.clone());
                        return true;
                    }
                }
            }
            false
        },
    );

    // remove_alias(connection_id, alias_name) -> bool
    let conns = connections.clone();
    let db_clone = db.clone();
    engine.register_fn("remove_alias", move |connection_id: String, alias_name: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns = conns.lock().unwrap();
            if let Some(session) = conns.get_mut(&uuid) {
                if let Some(ref mut character) = session.character {
                    let removed = character.aliases.remove(&alias_name).is_some();
                    if removed {
                        let _ = db_clone.save_character_data(character.clone());
                    }
                    return removed;
                }
            }
        }
        false
    });

    // ========== OLC Mode Functions ==========

    // set_olc_mode(connection_id, mode) -> Set OLC mode (e.g., "collecting_desc")
    let conns = connections.clone();
    engine.register_fn("set_olc_mode", move |connection_id: String, mode: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns = conns.lock().unwrap();
            if let Some(session) = conns.get_mut(&uuid) {
                session.olc_mode = if mode.is_empty() { None } else { Some(mode) };
                return true;
            }
        }
        false
    });

    // get_olc_mode(connection_id) -> Get current OLC mode (empty string if none)
    let conns = connections.clone();
    engine.register_fn("get_olc_mode", move |connection_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns = conns.lock().unwrap();
            if let Some(session) = conns.get(&uuid) {
                return session.olc_mode.clone().unwrap_or_default();
            }
        }
        String::new()
    });

    // append_olc_buffer(connection_id, line) -> Add line to OLC buffer
    let conns = connections.clone();
    engine.register_fn("append_olc_buffer", move |connection_id: String, line: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns = conns.lock().unwrap();
            if let Some(session) = conns.get_mut(&uuid) {
                session.olc_buffer.push(line);
                return true;
            }
        }
        false
    });

    // get_olc_buffer(connection_id) -> Get all lines in OLC buffer as array
    let conns = connections.clone();
    engine.register_fn("get_olc_buffer", move |connection_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns = conns.lock().unwrap();
            if let Some(session) = conns.get(&uuid) {
                return session
                    .olc_buffer
                    .iter()
                    .map(|s| rhai::Dynamic::from(s.clone()))
                    .collect::<Vec<_>>();
            }
        }
        Vec::new()
    });

    // clear_olc_buffer(connection_id) -> Clear OLC buffer
    let conns = connections.clone();
    engine.register_fn("clear_olc_buffer", move |connection_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns = conns.lock().unwrap();
            if let Some(session) = conns.get_mut(&uuid) {
                session.olc_buffer.clear();
                return true;
            }
        }
        false
    });

    // set_olc_buffer(connection_id, lines) -> Set OLC buffer contents (for pre-populating)
    let conns = connections.clone();
    engine.register_fn("set_olc_buffer", move |connection_id: String, lines: rhai::Array| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns = conns.lock().unwrap();
            if let Some(session) = conns.get_mut(&uuid) {
                session.olc_buffer = lines.into_iter().filter_map(|d| d.try_cast::<String>()).collect();
                return true;
            }
        }
        false
    });

    // set_olc_edit_room(connection_id, room_id) -> Set room being edited
    let conns = connections.clone();
    engine.register_fn("set_olc_edit_room", move |connection_id: String, room_id: String| {
        if let Ok(conn_uuid) = uuid::Uuid::parse_str(&connection_id) {
            let room_uuid = if room_id.is_empty() {
                None
            } else {
                uuid::Uuid::parse_str(&room_id).ok()
            };
            let mut conns = conns.lock().unwrap();
            if let Some(session) = conns.get_mut(&conn_uuid) {
                session.olc_edit_room = room_uuid;
                return true;
            }
        }
        false
    });

    // get_olc_edit_room(connection_id) -> Get room being edited (empty string if none)
    let conns = connections.clone();
    engine.register_fn("get_olc_edit_room", move |connection_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns = conns.lock().unwrap();
            if let Some(session) = conns.get(&uuid) {
                return session.olc_edit_room.map(|u| u.to_string()).unwrap_or_default();
            }
        }
        String::new()
    });

    // set_olc_edit_item(connection_id, item_id) -> Set item being edited (for note editor)
    let conns = connections.clone();
    engine.register_fn("set_olc_edit_item", move |connection_id: String, item_id: String| {
        if let Ok(conn_uuid) = uuid::Uuid::parse_str(&connection_id) {
            let item_uuid = if item_id.is_empty() {
                None
            } else {
                uuid::Uuid::parse_str(&item_id).ok()
            };
            let mut conns = conns.lock().unwrap();
            if let Some(session) = conns.get_mut(&conn_uuid) {
                session.olc_edit_item = item_uuid;
                return true;
            }
        }
        false
    });

    // set_olc_extra_keywords(connection_id, keywords) -> Set keywords for extra desc being collected
    let conns = connections.clone();
    engine.register_fn(
        "set_olc_extra_keywords",
        move |connection_id: String, keywords: rhai::Array| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
                let mut conns = conns.lock().unwrap();
                if let Some(session) = conns.get_mut(&uuid) {
                    session.olc_extra_keywords = keywords.into_iter().filter_map(|d| d.try_cast::<String>()).collect();
                    return true;
                }
            }
            false
        },
    );

    // get_olc_extra_keywords(connection_id) -> Get keywords stored during OLC
    let conns = connections.clone();
    engine.register_fn("get_olc_extra_keywords", move |connection_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns = conns.lock().unwrap();
            if let Some(session) = conns.get(&uuid) {
                return session
                    .olc_extra_keywords
                    .iter()
                    .map(|s| rhai::Dynamic::from(s.clone()))
                    .collect::<Vec<_>>();
            }
        }
        Vec::new()
    });

    // ============================================================
    // Item Quality and Fishing Property Functions
    // ============================================================

    // set_item_quality(item_id, quality) -> bool
    // Generic quality setter (0-100, used by fishing rods, bait, etc.)
    let cloned_db = db.clone();
    engine.register_fn("set_item_quality", move |item_id: String, quality: i64| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            match cloned_db.get_item_data(&uuid) {
                Ok(Some(mut item)) => {
                    item.quality = (quality as i32).clamp(0, 100);
                    cloned_db.save_item_data(item).is_ok()
                }
                _ => false,
            }
        } else {
            false
        }
    });

    // set_bait_uses(item_id, uses) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_bait_uses", move |item_id: String, uses: i64| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            match cloned_db.get_item_data(&uuid) {
                Ok(Some(mut item)) => {
                    item.bait_uses = (uses as i32).max(0);
                    cloned_db.save_item_data(item).is_ok()
                }
                _ => false,
            }
        } else {
            false
        }
    });

    // consume_bait(item_id) -> bool (decrements bait_uses, deletes item when uses reach 0 or if already 0)
    let cloned_db = db.clone();
    engine.register_fn("consume_bait", move |item_id: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            match cloned_db.get_item_data(&uuid) {
                Ok(Some(mut item)) => {
                    if item.bait_uses <= 1 {
                        // Last use or single-use (0 or 1), delete the item
                        let _ = cloned_db.delete_item(&uuid);
                        return true;
                    }
                    // Multiple uses remaining, decrement
                    item.bait_uses -= 1;
                    cloned_db.save_item_data(item).is_ok()
                }
                _ => false,
            }
        } else {
            false
        }
    });

    // ============================================================
    // Room Fishing (Water Source) Functions
    // ============================================================

    // set_room_water_type(room_id, type_str) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_room_water_type",
        move |room_id: String, type_str: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
                match cloned_db.get_room_data(&uuid) {
                    Ok(Some(mut room)) => {
                        if let Some(water_type) = crate::WaterType::from_str(&type_str) {
                            room.water_type = water_type;
                            return cloned_db.save_room_data(room).is_ok();
                        }
                        false
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // get_room_water_type(room_id) -> String
    let cloned_db = db.clone();
    engine.register_fn("get_room_water_type", move |room_id: String| -> String {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            match cloned_db.get_room_data(&uuid) {
                Ok(Some(room)) => room.water_type.to_display_string().to_string(),
                _ => "none".to_string(),
            }
        } else {
            "none".to_string()
        }
    });

    // has_fishable_water(room_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("has_fishable_water", move |room_id: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            match cloned_db.get_room_data(&uuid) {
                Ok(Some(room)) => room.water_type != crate::WaterType::None,
                _ => false,
            }
        } else {
            false
        }
    });

    // add_catch_entry(room_id, vnum, weight, min_skill, rarity) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "add_catch_entry",
        move |room_id: String, vnum: String, weight: i64, min_skill: i64, rarity: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
                match cloned_db.get_room_data(&uuid) {
                    Ok(Some(mut room)) => {
                        room.catch_table.push(crate::CatchEntry {
                            vnum,
                            weight: weight as i32,
                            min_skill: min_skill as i32,
                            rarity,
                        });
                        cloned_db.save_room_data(room).is_ok()
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // remove_catch_entry(room_id, vnum) -> bool
    let cloned_db = db.clone();
    engine.register_fn("remove_catch_entry", move |room_id: String, vnum: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            match cloned_db.get_room_data(&uuid) {
                Ok(Some(mut room)) => {
                    let original_len = room.catch_table.len();
                    room.catch_table.retain(|e| e.vnum != vnum);
                    if room.catch_table.len() < original_len {
                        return cloned_db.save_room_data(room).is_ok();
                    }
                    false
                }
                _ => false,
            }
        } else {
            false
        }
    });

    // get_catch_table(room_id) -> Array of Maps
    let cloned_db = db.clone();
    engine.register_fn("get_catch_table", move |room_id: String| -> Vec<rhai::Dynamic> {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            match cloned_db.get_room_data(&uuid) {
                Ok(Some(room)) => room
                    .catch_table
                    .iter()
                    .map(|entry| {
                        let mut map = rhai::Map::new();
                        map.insert("vnum".into(), rhai::Dynamic::from(entry.vnum.clone()));
                        map.insert("weight".into(), rhai::Dynamic::from(entry.weight as i64));
                        map.insert("min_skill".into(), rhai::Dynamic::from(entry.min_skill as i64));
                        map.insert("rarity".into(), rhai::Dynamic::from(entry.rarity.clone()));
                        rhai::Dynamic::from(map)
                    })
                    .collect(),
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        }
    });

    // select_catch(room_id, skill_level) -> Map with vnum and rarity, or () if nothing
    // Weighted random selection from catch table based on skill level
    let cloned_db = db.clone();
    engine.register_fn(
        "select_catch",
        move |room_id: String, skill_level: i64| -> rhai::Dynamic {
            if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
                match cloned_db.get_room_data(&uuid) {
                    Ok(Some(room)) => {
                        // Filter by skill level
                        let available: Vec<&crate::CatchEntry> = room
                            .catch_table
                            .iter()
                            .filter(|e| e.min_skill <= skill_level as i32)
                            .collect();

                        if available.is_empty() {
                            return rhai::Dynamic::UNIT;
                        }

                        // Calculate total weight
                        let total_weight: i32 = available.iter().map(|e| e.weight).sum();
                        if total_weight <= 0 {
                            return rhai::Dynamic::UNIT;
                        }

                        // Random selection
                        use std::time::{SystemTime, UNIX_EPOCH};
                        let seed = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_nanos() as u64)
                            .unwrap_or(0);
                        let mut roll = (seed % total_weight as u64) as i32;

                        for entry in available {
                            roll -= entry.weight;
                            if roll < 0 {
                                let mut result = rhai::Map::new();
                                result.insert("vnum".into(), rhai::Dynamic::from(entry.vnum.clone()));
                                result.insert("rarity".into(), rhai::Dynamic::from(entry.rarity.clone()));
                                result.insert("min_skill".into(), rhai::Dynamic::from(entry.min_skill as i64));
                                return rhai::Dynamic::from(result);
                            }
                        }
                        rhai::Dynamic::UNIT
                    }
                    _ => rhai::Dynamic::UNIT,
                }
            } else {
                rhai::Dynamic::UNIT
            }
        },
    );

    // Register submodule functions
    utilities::register(engine, db.clone(), connections.clone());
    spawn::register(engine, db.clone());
    api_keys::register(engine, db.clone());
    areas::register(engine, db.clone());
    combat::register(engine, db.clone());
    items::register(engine, db.clone());
    mobiles::register(engine, db.clone());
    rooms::register(engine, db.clone(), connections.clone());
    shops::register(engine, db.clone());
    shop_presets::register(engine, db.clone());
    transport::register_types(engine);
    transport::register(engine, db.clone());
    triggers::register(engine, db.clone(), connections.clone());
    fishing::register(engine, connections.clone());
    medical::register(engine, db.clone(), connections.clone());
    healers::register(engine, db.clone(), connections.clone());
    crafting::register(engine, db.clone(), state.clone());
    characters::register(engine, db.clone(), connections.clone(), state.clone());
    groups::register(engine, db.clone(), connections.clone());
    property::register(engine, db.clone(), connections.clone());
    mail::register(engine, db.clone(), connections.clone());
    garden::register(engine, db.clone());
    spells::register(engine, db.clone(), connections.clone(), state.clone());
    stealth::register(engine, db.clone(), connections.clone());
    bugs::register(engine, db.clone(), connections.clone());
    simulation::register(engine, db.clone());
    social::register(engine, db.clone());
}

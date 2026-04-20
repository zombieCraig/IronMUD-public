use anyhow::Result;
use uuid::Uuid;

use crate::STARTING_ROOM_ID;
use crate::db::Db;
use crate::types::*;

use super::seed_uuid;

pub fn seed_spawn_points(db: &Db) -> Result<()> {
    let mut spawns = Vec::new();

    // ── Oakvale Village NPCs ─────────────────────────────────────

    // Blacksmith in the smithy
    spawns.push(SpawnPointData {
        id: seed_uuid("spawn:blacksmith"),
        area_id: seed_uuid("area:oakvale"),
        room_id: seed_uuid("oakvale:smithy"),
        entity_type: SpawnEntityType::Mobile,
        vnum: "oakvale:blacksmith".to_string(),
        max_count: 1,
        respawn_interval_secs: 300,
        enabled: true,
        last_spawn_time: 0,
        spawned_entities: Vec::new(),
        dependencies: Vec::new(),
    });

    // Merchant in the general store
    spawns.push(SpawnPointData {
        id: seed_uuid("spawn:merchant"),
        area_id: seed_uuid("area:oakvale"),
        room_id: seed_uuid("oakvale:general_store"),
        entity_type: SpawnEntityType::Mobile,
        vnum: "oakvale:merchant".to_string(),
        max_count: 1,
        respawn_interval_secs: 300,
        enabled: true,
        last_spawn_time: 0,
        spawned_entities: Vec::new(),
        dependencies: Vec::new(),
    });

    // Priestess in the temple
    spawns.push(SpawnPointData {
        id: seed_uuid("spawn:priestess"),
        area_id: seed_uuid("area:oakvale"),
        room_id: seed_uuid("oakvale:temple"),
        entity_type: SpawnEntityType::Mobile,
        vnum: "oakvale:priestess".to_string(),
        max_count: 1,
        respawn_interval_secs: 300,
        enabled: true,
        last_spawn_time: 0,
        spawned_entities: Vec::new(),
        dependencies: Vec::new(),
    });

    // Barkeeper in the tavern
    spawns.push(SpawnPointData {
        id: seed_uuid("spawn:barkeeper"),
        area_id: seed_uuid("area:oakvale"),
        room_id: seed_uuid("oakvale:tavern"),
        entity_type: SpawnEntityType::Mobile,
        vnum: "oakvale:barkeeper".to_string(),
        max_count: 1,
        respawn_interval_secs: 300,
        enabled: true,
        last_spawn_time: 0,
        spawned_entities: Vec::new(),
        dependencies: Vec::new(),
    });

    // Postmaster in the post office
    spawns.push(SpawnPointData {
        id: seed_uuid("spawn:postmaster"),
        area_id: seed_uuid("area:oakvale"),
        room_id: seed_uuid("oakvale:post_office"),
        entity_type: SpawnEntityType::Mobile,
        vnum: "oakvale:postmaster".to_string(),
        max_count: 1,
        respawn_interval_secs: 300,
        enabled: true,
        last_spawn_time: 0,
        spawned_entities: Vec::new(),
        dependencies: Vec::new(),
    });

    // Banker in the bank
    spawns.push(SpawnPointData {
        id: seed_uuid("spawn:banker"),
        area_id: seed_uuid("area:oakvale"),
        room_id: seed_uuid("oakvale:bank"),
        entity_type: SpawnEntityType::Mobile,
        vnum: "oakvale:banker".to_string(),
        max_count: 1,
        respawn_interval_secs: 300,
        enabled: true,
        last_spawn_time: 0,
        spawned_entities: Vec::new(),
        dependencies: Vec::new(),
    });

    // Leasing agent at cottage lane
    spawns.push(SpawnPointData {
        id: seed_uuid("spawn:leasing_agent"),
        area_id: seed_uuid("area:oakvale"),
        room_id: seed_uuid("oakvale:cottage_entry"),
        entity_type: SpawnEntityType::Mobile,
        vnum: "oakvale:leasing_agent".to_string(),
        max_count: 1,
        respawn_interval_secs: 300,
        enabled: true,
        last_spawn_time: 0,
        spawned_entities: Vec::new(),
        dependencies: Vec::new(),
    });

    // Village guard — patrols, spawns at town square
    let square_id = Uuid::parse_str(STARTING_ROOM_ID).expect("valid STARTING_ROOM_ID");
    spawns.push(SpawnPointData {
        id: seed_uuid("spawn:village_guard"),
        area_id: seed_uuid("area:oakvale"),
        room_id: square_id,
        entity_type: SpawnEntityType::Mobile,
        vnum: "oakvale:village_guard".to_string(),
        max_count: 1,
        respawn_interval_secs: 300,
        enabled: true,
        last_spawn_time: 0,
        spawned_entities: Vec::new(),
        dependencies: Vec::new(),
    });

    // ── Iron Keep NPCs ───────────────────────────────────────────

    // Armorer in the armory
    spawns.push(SpawnPointData {
        id: seed_uuid("spawn:armorer"),
        area_id: seed_uuid("area:ironkeep"),
        room_id: seed_uuid("ironkeep:armory"),
        entity_type: SpawnEntityType::Mobile,
        vnum: "ironkeep:armorer".to_string(),
        max_count: 1,
        respawn_interval_secs: 300,
        enabled: true,
        last_spawn_time: 0,
        spawned_entities: Vec::new(),
        dependencies: Vec::new(),
    });

    // Keep knight at gatehouse — spawns with the gate key equipped
    spawns.push(SpawnPointData {
        id: seed_uuid("spawn:keep_knight"),
        area_id: seed_uuid("area:ironkeep"),
        room_id: seed_uuid("ironkeep:gatehouse"),
        entity_type: SpawnEntityType::Mobile,
        vnum: "ironkeep:knight".to_string(),
        max_count: 1,
        respawn_interval_secs: 300,
        enabled: true,
        last_spawn_time: 0,
        spawned_entities: Vec::new(),
        dependencies: vec![SpawnDependency {
            item_vnum: "oakvale:gate_key".to_string(),
            destination: SpawnDestination::Inventory,
            count: 1,
            chance: 100,
        }],
    });

    // ── Whispering Woods Enemies ─────────────────────────────────

    // Wolf #1 in wolf den
    spawns.push(SpawnPointData {
        id: seed_uuid("spawn:wolf_1"),
        area_id: seed_uuid("area:whisperwood"),
        room_id: seed_uuid("whisperwood:wolf_den"),
        entity_type: SpawnEntityType::Mobile,
        vnum: "whisperwood:wolf".to_string(),
        max_count: 2,
        respawn_interval_secs: 600,
        enabled: true,
        last_spawn_time: 0,
        spawned_entities: Vec::new(),
        dependencies: Vec::new(),
    });

    // Wolf #2 on the deep trail
    spawns.push(SpawnPointData {
        id: seed_uuid("spawn:wolf_2"),
        area_id: seed_uuid("area:whisperwood"),
        room_id: seed_uuid("whisperwood:deep_trail"),
        entity_type: SpawnEntityType::Mobile,
        vnum: "whisperwood:wolf".to_string(),
        max_count: 1,
        respawn_interval_secs: 600,
        enabled: true,
        last_spawn_time: 0,
        spawned_entities: Vec::new(),
        dependencies: Vec::new(),
    });

    // ── Shadowfang Caves Enemies ─────────────────────────────────

    // Goblin #1 in goblin camp
    spawns.push(SpawnPointData {
        id: seed_uuid("spawn:goblin_1"),
        area_id: seed_uuid("area:shadowfang"),
        room_id: seed_uuid("shadowfang:goblin_camp"),
        entity_type: SpawnEntityType::Mobile,
        vnum: "shadowfang:goblin".to_string(),
        max_count: 2,
        respawn_interval_secs: 300,
        enabled: true,
        last_spawn_time: 0,
        spawned_entities: Vec::new(),
        dependencies: Vec::new(),
    });

    // Goblin #2 in twisting tunnels
    spawns.push(SpawnPointData {
        id: seed_uuid("spawn:goblin_2"),
        area_id: seed_uuid("area:shadowfang"),
        room_id: seed_uuid("shadowfang:tunnel"),
        entity_type: SpawnEntityType::Mobile,
        vnum: "shadowfang:goblin".to_string(),
        max_count: 1,
        respawn_interval_secs: 300,
        enabled: true,
        last_spawn_time: 0,
        spawned_entities: Vec::new(),
        dependencies: Vec::new(),
    });

    // Cave spider in the spider nest
    spawns.push(SpawnPointData {
        id: seed_uuid("spawn:spider"),
        area_id: seed_uuid("area:shadowfang"),
        room_id: seed_uuid("shadowfang:spider_nest"),
        entity_type: SpawnEntityType::Mobile,
        vnum: "shadowfang:spider".to_string(),
        max_count: 1,
        respawn_interval_secs: 600,
        enabled: true,
        last_spawn_time: 0,
        spawned_entities: Vec::new(),
        dependencies: Vec::new(),
    });

    // Shadow Drake in the drake lair — boss, long respawn, chance to drop shadow blade
    spawns.push(SpawnPointData {
        id: seed_uuid("spawn:shadow_drake"),
        area_id: seed_uuid("area:shadowfang"),
        room_id: seed_uuid("shadowfang:drake_lair"),
        entity_type: SpawnEntityType::Mobile,
        vnum: "shadowfang:shadow_drake".to_string(),
        max_count: 1,
        respawn_interval_secs: 1800, // 30 minutes
        enabled: true,
        last_spawn_time: 0,
        spawned_entities: Vec::new(),
        dependencies: vec![
            SpawnDependency {
                item_vnum: "shadowfang:shadow_blade".to_string(),
                destination: SpawnDestination::Inventory,
                count: 1,
                chance: 25, // 25% drop chance
            },
            SpawnDependency {
                item_vnum: "shadowfang:treasure_key".to_string(),
                destination: SpawnDestination::Inventory,
                count: 1,
                chance: 100,
            },
        ],
    });

    // ── Hilltop Farm NPCs ────────────────────────────────────────

    // Farmer at the farmyard
    spawns.push(SpawnPointData {
        id: seed_uuid("spawn:farmer"),
        area_id: seed_uuid("area:hilltop"),
        room_id: seed_uuid("hilltop:farmyard"),
        entity_type: SpawnEntityType::Mobile,
        vnum: "hilltop:farmer".to_string(),
        max_count: 1,
        respawn_interval_secs: 300,
        enabled: true,
        last_spawn_time: 0,
        spawned_entities: Vec::new(),
        dependencies: Vec::new(),
    });

    // ── Static Item Spawns ───────────────────────────────────────

    // Healing potion in the temple
    spawns.push(SpawnPointData {
        id: seed_uuid("spawn:temple_potion"),
        area_id: seed_uuid("area:oakvale"),
        room_id: seed_uuid("oakvale:temple"),
        entity_type: SpawnEntityType::Item,
        vnum: "oakvale:healing_potion".to_string(),
        max_count: 1,
        respawn_interval_secs: 900, // 15 minutes
        enabled: true,
        last_spawn_time: 0,
        spawned_entities: Vec::new(),
        dependencies: Vec::new(),
    });

    // Glowing mushroom in the fungal grotto
    spawns.push(SpawnPointData {
        id: seed_uuid("spawn:mushroom"),
        area_id: seed_uuid("area:shadowfang"),
        room_id: seed_uuid("shadowfang:fungal_grotto"),
        entity_type: SpawnEntityType::Item,
        vnum: "shadowfang:mushroom".to_string(),
        max_count: 2,
        respawn_interval_secs: 600,
        enabled: true,
        last_spawn_time: 0,
        spawned_entities: Vec::new(),
        dependencies: Vec::new(),
    });

    // Torch at the cave entrance
    spawns.push(SpawnPointData {
        id: seed_uuid("spawn:cave_torch"),
        area_id: seed_uuid("area:shadowfang"),
        room_id: seed_uuid("shadowfang:entrance"),
        entity_type: SpawnEntityType::Item,
        vnum: "oakvale:torch".to_string(),
        max_count: 1,
        respawn_interval_secs: 600,
        enabled: true,
        last_spawn_time: 0,
        spawned_entities: Vec::new(),
        dependencies: Vec::new(),
    });

    for spawn in spawns {
        db.save_spawn_point(spawn)?;
    }

    tracing::info!("Seeded {} spawn points", 22);
    Ok(())
}

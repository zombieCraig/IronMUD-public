//! Corpse creation using builder pattern

use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use ironmud::{ItemData, ItemFlags, ItemLocation, ItemType, LiquidType};

/// Builder for creating corpses from dead entities
pub struct CorpseBuilder {
    name: String,
    room_id: Uuid,
    gold: i64,
    is_player: bool,
}

impl CorpseBuilder {
    /// Create a new corpse builder for a player
    pub fn for_player(name: &str, room_id: Uuid, gold: i64) -> Self {
        Self {
            name: name.to_string(),
            room_id,
            gold,
            is_player: true,
        }
    }

    /// Create a new corpse builder for a mobile
    pub fn for_mobile(name: &str, room_id: Uuid, gold: i64) -> Self {
        Self {
            name: name.to_string(),
            room_id,
            gold,
            is_player: false,
        }
    }

    /// Build the corpse ItemData
    pub fn build(self) -> ItemData {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        ItemData {
            id: Uuid::new_v4(),
            name: format!("corpse of {}", self.name),
            short_desc: format!("The corpse of {} lies here.", self.name),
            long_desc: format!("The lifeless body of {} lies in a crumpled heap.", self.name),
            keywords: vec!["corpse".to_string(), "body".to_string(), self.name.to_lowercase()],
            item_type: ItemType::Container,
            categories: Vec::new(),
            teaches_recipe: None,
            teaches_spell: None,
            note_content: None,
            wear_locations: vec![],
            armor_class: None,
            protects: vec![],
            flags: ItemFlags {
                no_get: true,
                is_corpse: true,
                corpse_owner: self.name.clone(),
                corpse_created_at: now,
                corpse_is_player: self.is_player,
                corpse_gold: self.gold,
                ..Default::default()
            },
            weight: 100,
            value: 0,
            location: ItemLocation::Room(self.room_id),
            damage_dice_count: 0,
            damage_dice_sides: 0,
            damage_type: Default::default(),
            two_handed: false,
            weapon_skill: None,
            // Container fields - corpses are containers
            container_contents: vec![],
            container_max_items: 1000,
            container_max_weight: 10000,
            container_closed: false,
            container_locked: false,
            container_key_id: None,
            weight_reduction: 0,
            // Liquid container fields
            liquid_type: LiquidType::default(),
            liquid_current: 0,
            liquid_max: 0,
            liquid_poisoned: false,
            liquid_effects: vec![],
            // Food fields
            food_nutrition: 0,
            food_poisoned: false,
            food_spoil_duration: 0,
            food_created_at: None,
            food_effects: vec![],
            food_spoilage_points: 0.0,
            preservation_level: 0,
            // Level/stats
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
}

/// Calculate gold drop with random variance for mobiles
pub fn mobile_gold_with_variance(base_gold: i64) -> i64 {
    use rand::Rng;

    if base_gold > 0 {
        let mut rng = rand::thread_rng();
        let variance = (base_gold as f64 * 0.1) as i64;
        let min = (base_gold - variance).max(0);
        let max = base_gold + variance;
        rng.gen_range(min..=max)
    } else {
        0
    }
}

//! Item CRUD endpoints

use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use super::{
    ApiState,
    auth::{AuthenticatedUser, can_read, can_write},
    error::ApiError,
    notify_builders,
};
use crate::types::{EffectType, ExtraDesc, ItemEffect, ItemTrigger, ItemTriggerType};
use crate::{DamageType, ItemData, ItemFlags, ItemLocation, ItemType, LiquidType, WeaponSkill, WearLocation};

use super::rooms::AddExtraDescRequest;

const MAX_NOTE_BYTES: usize = 32 * 1024;

/// Normalize note body line endings (\r\n → \n, lone \r → \n) and enforce the size cap.
fn normalize_note_input(raw: String) -> Result<Option<String>, ApiError> {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    if normalized.len() > MAX_NOTE_BYTES {
        return Err(ApiError::InvalidInput(format!(
            "note_content exceeds {} bytes (got {})",
            MAX_NOTE_BYTES,
            normalized.len()
        )));
    }
    Ok(if normalized.is_empty() { None } else { Some(normalized) })
}

pub fn routes() -> Router<Arc<ApiState>> {
    Router::new()
        .route("/", get(list_items).post(create_item))
        .route("/prototypes", get(list_prototypes))
        .route("/prototypes/summary", get(list_prototypes_summary))
        .route("/:id", get(get_item).put(update_item).delete(delete_item))
        .route("/by-vnum/:vnum", get(get_item_by_vnum))
        .route("/:id/triggers", post(add_trigger))
        .route("/:id/triggers/:index", delete(remove_trigger))
        .route("/:id/extra", post(add_extra_desc))
        .route("/:id/extra/:keyword", delete(remove_extra_desc))
        .route("/:vnum/spawn", post(spawn_item))
}

#[derive(Deserialize)]
pub struct ListItemsQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub item_type: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateItemRequest {
    pub name: String,
    pub short_desc: String,
    pub long_desc: String,
    pub vnum: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    pub item_type: String,
    #[serde(default)]
    pub weight: i32,
    #[serde(default)]
    pub value: i32,
    #[serde(default)]
    pub categories: Option<Vec<String>>,
    #[serde(default)]
    pub wear_location: Option<String>,
    // Weapon stats
    #[serde(default)]
    pub damage_dice_count: Option<i32>,
    #[serde(default)]
    pub damage_dice_sides: Option<i32>,
    #[serde(default)]
    pub damage_type: Option<String>,
    // Armor stats
    #[serde(default)]
    pub armor_class: Option<i32>,
    // Equipment combat bonuses (CircleMUD APPLY_HITROLL/DAMROLL parity)
    #[serde(default)]
    pub hit_bonus: Option<i32>,
    #[serde(default)]
    pub damage_bonus: Option<i32>,
    // Flags
    #[serde(default)]
    pub flags: ItemFlagsRequest,
    // Firearm fields
    #[serde(default)]
    pub caliber: Option<String>,
    #[serde(default)]
    pub ranged_type: Option<String>,
    #[serde(default)]
    pub magazine_size: Option<i32>,
    #[serde(default)]
    pub fire_mode: Option<String>,
    #[serde(default)]
    pub supported_fire_modes: Option<Vec<String>>,
    #[serde(default)]
    pub noise_level: Option<String>,
    #[serde(default)]
    pub two_handed: Option<bool>,
    // Ammo fields
    #[serde(default)]
    pub ammo_count: Option<i32>,
    #[serde(default)]
    pub ammo_damage_bonus: Option<i32>,
    // Attachment fields
    #[serde(default)]
    pub attachment_slot: Option<String>,
    #[serde(default)]
    pub attachment_accuracy_bonus: Option<i32>,
    #[serde(default)]
    pub attachment_noise_reduction: Option<i32>,
    #[serde(default)]
    pub attachment_magazine_bonus: Option<i32>,
    // Gardening fields
    #[serde(default)]
    pub plant_prototype_vnum: Option<String>,
    #[serde(default)]
    pub fertilizer_duration: Option<i64>,
    #[serde(default)]
    pub treats_infestation: Option<String>,
    #[serde(default)]
    pub weapon_skill: Option<String>,
    // Liquid container fields
    #[serde(default)]
    pub liquid_type: Option<String>,
    #[serde(default)]
    pub liquid_current: Option<i32>,
    #[serde(default)]
    pub liquid_max: Option<i32>,
    #[serde(default)]
    pub liquid_effects: Option<Vec<FoodEffectRequest>>,
    // Medical fields
    #[serde(default)]
    pub medical_tier: Option<i32>,
    #[serde(default)]
    pub medical_uses: Option<i32>,
    #[serde(default)]
    pub treats_wound_types: Option<Vec<String>>,
    // Food fields
    #[serde(default)]
    pub food_nutrition: Option<i32>,
    #[serde(default)]
    pub food_spoil_duration: Option<i64>,
    #[serde(default)]
    pub food_effects: Option<Vec<FoodEffectRequest>>,
    #[serde(default)]
    pub note_content: Option<String>,
    #[serde(default)]
    pub container_key_vnum: Option<String>,
    #[serde(default)]
    pub world_max_count: Option<i32>,
    #[serde(default)]
    pub extra_descs: Option<Vec<ExtraDescRequest>>,
}

/// Builder-facing subset of `ItemFlags`. Every field is optional so callers
/// can send only the flags they want to change; unmentioned flags are
/// preserved on update and default to `false` on create. Runtime-only
/// corpse state is intentionally not exposed here.
#[derive(Deserialize, Default)]
pub struct ItemFlagsRequest {
    #[serde(default)]
    pub no_drop: Option<bool>,
    #[serde(default)]
    pub no_get: Option<bool>,
    #[serde(default)]
    pub no_remove: Option<bool>,
    #[serde(default)]
    pub invisible: Option<bool>,
    #[serde(default)]
    pub glow: Option<bool>,
    #[serde(default)]
    pub hum: Option<bool>,
    #[serde(default)]
    pub magical: Option<bool>,
    #[serde(default)]
    pub no_sell: Option<bool>,
    #[serde(default)]
    pub unique: Option<bool>,
    #[serde(default)]
    pub quest_item: Option<bool>,
    #[serde(default)]
    pub vending: Option<bool>,
    #[serde(default)]
    pub provides_light: Option<bool>,
    #[serde(default)]
    pub night_vision: Option<bool>,
    #[serde(default)]
    pub fishing_rod: Option<bool>,
    #[serde(default)]
    pub bait: Option<bool>,
    #[serde(default)]
    pub foraging_tool: Option<bool>,
    #[serde(default)]
    pub waterproof: Option<bool>,
    #[serde(default)]
    pub provides_warmth: Option<bool>,
    #[serde(default)]
    pub reduces_glare: Option<bool>,
    #[serde(default)]
    pub medical_tool: Option<bool>,
    #[serde(default)]
    pub preserves_contents: Option<bool>,
    #[serde(default)]
    pub death_only: Option<bool>,
    #[serde(default)]
    pub atm: Option<bool>,
    #[serde(default)]
    pub broken: Option<bool>,
    #[serde(default)]
    pub plant_pot: Option<bool>,
    #[serde(default)]
    pub lockpick: Option<bool>,
    #[serde(default)]
    pub is_skinned: Option<bool>,
    #[serde(default)]
    pub boat: Option<bool>,
    #[serde(default)]
    pub buried: Option<bool>,
    #[serde(default)]
    pub can_dig: Option<bool>,
    #[serde(default)]
    pub detect_buried: Option<bool>,
}

#[derive(Deserialize)]
pub struct FoodEffectRequest {
    pub effect_type: String,
    #[serde(default)]
    pub magnitude: i32,
    #[serde(default)]
    pub duration: i32,
}

#[derive(Deserialize)]
pub struct UpdateItemRequest {
    pub name: Option<String>,
    pub short_desc: Option<String>,
    pub long_desc: Option<String>,
    pub vnum: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub item_type: Option<String>,
    pub weight: Option<i32>,
    pub value: Option<i32>,
    #[serde(default)]
    pub categories: Option<Vec<String>>,
    pub flags: Option<ItemFlagsRequest>,
    // Firearm fields
    #[serde(default)]
    pub caliber: Option<String>,
    #[serde(default)]
    pub ranged_type: Option<String>,
    #[serde(default)]
    pub magazine_size: Option<i32>,
    #[serde(default)]
    pub fire_mode: Option<String>,
    #[serde(default)]
    pub supported_fire_modes: Option<Vec<String>>,
    #[serde(default)]
    pub noise_level: Option<String>,
    #[serde(default)]
    pub two_handed: Option<bool>,
    // Ammo fields
    #[serde(default)]
    pub ammo_count: Option<i32>,
    #[serde(default)]
    pub ammo_damage_bonus: Option<i32>,
    // Attachment fields
    #[serde(default)]
    pub attachment_slot: Option<String>,
    #[serde(default)]
    pub attachment_accuracy_bonus: Option<i32>,
    #[serde(default)]
    pub attachment_noise_reduction: Option<i32>,
    #[serde(default)]
    pub attachment_magazine_bonus: Option<i32>,
    // Weapon/armor fields
    #[serde(default)]
    pub damage_dice_count: Option<i32>,
    #[serde(default)]
    pub damage_dice_sides: Option<i32>,
    #[serde(default)]
    pub damage_type: Option<String>,
    #[serde(default)]
    pub armor_class: Option<i32>,
    #[serde(default)]
    pub hit_bonus: Option<i32>,
    #[serde(default)]
    pub damage_bonus: Option<i32>,
    #[serde(default)]
    pub wear_location: Option<String>,
    #[serde(default)]
    pub weapon_skill: Option<String>,
    // Gardening fields
    #[serde(default)]
    pub plant_prototype_vnum: Option<String>,
    #[serde(default)]
    pub fertilizer_duration: Option<i64>,
    #[serde(default)]
    pub treats_infestation: Option<String>,
    // Liquid container fields
    #[serde(default)]
    pub liquid_type: Option<String>,
    #[serde(default)]
    pub liquid_current: Option<i32>,
    #[serde(default)]
    pub liquid_max: Option<i32>,
    #[serde(default)]
    pub liquid_effects: Option<Vec<FoodEffectRequest>>,
    // Medical fields
    #[serde(default)]
    pub medical_tier: Option<i32>,
    #[serde(default)]
    pub medical_uses: Option<i32>,
    #[serde(default)]
    pub treats_wound_types: Option<Vec<String>>,
    // Food fields
    #[serde(default)]
    pub food_nutrition: Option<i32>,
    #[serde(default)]
    pub food_spoil_duration: Option<i64>,
    #[serde(default)]
    pub food_effects: Option<Vec<FoodEffectRequest>>,
    #[serde(default)]
    pub note_content: Option<String>,
    #[serde(default)]
    pub container_key_vnum: Option<String>,
    #[serde(default)]
    pub world_max_count: Option<i32>,
    #[serde(default)]
    pub extra_descs: Option<Vec<ExtraDescRequest>>,
}

/// API-facing extra description payload (mirrors `ExtraDesc`).
#[derive(Deserialize)]
pub struct ExtraDescRequest {
    pub keywords: Vec<String>,
    pub description: String,
}

#[derive(Deserialize)]
pub struct SpawnItemRequest {
    pub room_id: String,
}

#[derive(Serialize)]
pub struct ItemResponse {
    pub success: bool,
    pub data: ItemData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refreshed_instances: Option<usize>,
}

#[derive(Serialize)]
pub struct ItemsListResponse {
    pub success: bool,
    pub data: Vec<ItemData>,
    pub total: usize,
}

#[derive(Serialize)]
pub struct ItemSummary {
    pub vnum: Option<String>,
    pub name: String,
    pub item_type: ItemType,
    pub weight: i32,
    pub value: i32,
    pub wear_location: Option<String>,
    pub weapon_skill: Option<String>,
    pub damage: Option<String>,
    pub armor_class: Option<i32>,
}

#[derive(Serialize)]
pub struct ItemsSummaryResponse {
    pub success: bool,
    pub data: Vec<ItemSummary>,
    pub total: usize,
}

#[derive(Deserialize)]
pub struct SummaryQuery {
    pub vnum_prefix: Option<String>,
}

impl ItemSummary {
    pub fn from_item(item: &ItemData) -> Self {
        let wear_location = item.wear_locations.first().map(|w| format!("{:?}", w).to_lowercase());
        let weapon_skill = item.weapon_skill.as_ref().map(|ws| format!("{:?}", ws).to_lowercase());
        let damage = if item.item_type == ItemType::Weapon {
            Some(format!("{}d{}", item.damage_dice_count, item.damage_dice_sides))
        } else {
            None
        };
        let armor_class = if item.item_type == ItemType::Armor {
            item.armor_class
        } else {
            None
        };

        ItemSummary {
            vnum: item.vnum.clone(),
            name: item.name.clone(),
            item_type: item.item_type.clone(),
            weight: item.weight,
            value: item.value,
            wear_location,
            weapon_skill,
            damage,
            armor_class,
        }
    }
}

fn parse_item_type(s: &str) -> Option<ItemType> {
    match s.to_lowercase().as_str() {
        "weapon" => Some(ItemType::Weapon),
        "armor" => Some(ItemType::Armor),
        "container" => Some(ItemType::Container),
        "liquid_container" | "liquidcontainer" | "drink" | "drinkcon" => Some(ItemType::LiquidContainer),
        "food" => Some(ItemType::Food),
        "key" => Some(ItemType::Key),
        "gold" | "money" => Some(ItemType::Gold),
        "misc" | "other" => Some(ItemType::Misc),
        _ => None,
    }
}

/// Refresh all spawned instances of an item prototype from the prototype's current data.
/// Returns the number of successfully refreshed instances.
fn refresh_item_instances(db: &crate::db::Db, item: &ItemData) -> usize {
    if !item.is_prototype {
        return 0;
    }
    let vnum = match &item.vnum {
        Some(v) => v.clone(),
        None => return 0,
    };
    let instances = match db.get_item_instances_by_vnum(&vnum) {
        Ok(instances) => instances,
        Err(_) => return 0,
    };
    let mut count = 0;
    for instance in &instances {
        if db.refresh_item_from_prototype(&instance.id).is_ok() {
            count += 1;
        }
    }
    count
}

/// List items with pagination
async fn list_items(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(query): Query<ListItemsQuery>,
) -> Result<Json<ItemsListResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let mut items = state
        .db
        .list_all_items()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Filter by item_type if provided
    if let Some(ref type_str) = query.item_type {
        if let Some(item_type) = parse_item_type(type_str) {
            items.retain(|i| i.item_type == item_type);
        }
    }

    let total = items.len();
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(100);

    let items: Vec<ItemData> = items.into_iter().skip(offset).take(limit).collect();

    Ok(Json(ItemsListResponse {
        success: true,
        data: items,
        total,
    }))
}

/// List prototype items only
async fn list_prototypes(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<ItemsListResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let items: Vec<ItemData> = state
        .db
        .list_all_items()
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .into_iter()
        .filter(|i| i.is_prototype)
        .collect();

    let total = items.len();

    Ok(Json(ItemsListResponse {
        success: true,
        data: items,
        total,
    }))
}

/// List prototype item summaries (compact)
async fn list_prototypes_summary(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(query): Query<SummaryQuery>,
) -> Result<Json<ItemsSummaryResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let items: Vec<ItemData> = state
        .db
        .list_all_items()
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .into_iter()
        .filter(|i| i.is_prototype)
        .collect();

    let summaries: Vec<ItemSummary> = items
        .iter()
        .filter(|i| {
            if let Some(ref prefix) = query.vnum_prefix {
                i.vnum
                    .as_ref()
                    .map_or(false, |v| v.starts_with(&format!("{}:", prefix)))
            } else {
                true
            }
        })
        .map(ItemSummary::from_item)
        .collect();

    let total = summaries.len();

    Ok(Json(ItemsSummaryResponse {
        success: true,
        data: summaries,
        total,
    }))
}

/// Get item by UUID
async fn get_item(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<ItemResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let item = state
        .db
        .get_item_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Item '{}' not found", id)))?;

    Ok(Json(ItemResponse {
        success: true,
        data: item,
        refreshed_instances: None,
    }))
}

/// Get item by vnum
async fn get_item_by_vnum(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(vnum): Path<String>,
) -> Result<Json<ItemResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let item = state
        .db
        .get_item_by_vnum(&vnum)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Item with vnum '{}' not found", vnum)))?;

    Ok(Json(ItemResponse {
        success: true,
        data: item,
        refreshed_instances: None,
    }))
}

/// Create a new item prototype
async fn create_item(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<CreateItemRequest>,
) -> Result<Json<ItemResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    // Check vnum uniqueness
    if state
        .db
        .get_item_by_vnum(&req.vnum)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .is_some()
    {
        return Err(ApiError::VnumInUse(format!("Vnum '{}' is already in use", req.vnum)));
    }

    // Parse item type
    let item_type = parse_item_type(&req.item_type).ok_or_else(|| {
        ApiError::InvalidInput(format!(
            "Invalid item type '{}'. Use: weapon, armor, container, liquid_container, food, key, gold, misc",
            req.item_type
        ))
    })?;

    // Parse wear location if provided
    let wear_locations = if let Some(ref loc_str) = req.wear_location {
        WearLocation::from_str(loc_str).map(|l| vec![l]).unwrap_or_default()
    } else {
        Vec::new()
    };

    // Parse damage type
    let damage_type = req
        .damage_type
        .as_ref()
        .and_then(|s| DamageType::from_str(s))
        .unwrap_or_default();

    let mut item = ItemData {
        id: Uuid::new_v4(),
        name: req.name,
        short_desc: req.short_desc,
        long_desc: req.long_desc,
        vnum: Some(req.vnum.clone()),
        keywords: req.keywords,
        item_type,
        categories: req.categories.clone().unwrap_or_default(),
        teaches_recipe: None,
        teaches_spell: None,
        note_content: req.note_content.map(normalize_note_input).transpose()?.flatten(),
        extra_descs: req
            .extra_descs
            .map(|v| {
                v.into_iter()
                    .map(|e| ExtraDesc {
                        keywords: e.keywords,
                        description: e.description,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        weight: req.weight,
        value: req.value,
        is_prototype: true,
        world_max_count: req.world_max_count,
        location: ItemLocation::Nowhere,
        wear_locations,
        armor_class: req.armor_class,
        hit_bonus: req.hit_bonus.unwrap_or(0),
        damage_bonus: req.damage_bonus.unwrap_or(0),
        protects: Vec::new(),
        flags: ItemFlags {
            no_drop: req.flags.no_drop.unwrap_or(false),
            no_get: req.flags.no_get.unwrap_or(false),
            no_remove: req.flags.no_remove.unwrap_or(false),
            invisible: req.flags.invisible.unwrap_or(false),
            glow: req.flags.glow.unwrap_or(false),
            hum: req.flags.hum.unwrap_or(false),
            magical: req.flags.magical.unwrap_or(false),
            no_sell: req.flags.no_sell.unwrap_or(false),
            unique: req.flags.unique.unwrap_or(false),
            quest_item: req.flags.quest_item.unwrap_or(false),
            vending: req.flags.vending.unwrap_or(false),
            provides_light: req.flags.provides_light.unwrap_or(false),
            night_vision: req.flags.night_vision.unwrap_or(false),
            fishing_rod: req.flags.fishing_rod.unwrap_or(false),
            bait: req.flags.bait.unwrap_or(false),
            foraging_tool: req.flags.foraging_tool.unwrap_or(false),
            waterproof: req.flags.waterproof.unwrap_or(false),
            provides_warmth: req.flags.provides_warmth.unwrap_or(false),
            reduces_glare: req.flags.reduces_glare.unwrap_or(false),
            medical_tool: req.flags.medical_tool.unwrap_or(false),
            preserves_contents: req.flags.preserves_contents.unwrap_or(false),
            death_only: req.flags.death_only.unwrap_or(false),
            atm: req.flags.atm.unwrap_or(false),
            broken: req.flags.broken.unwrap_or(false),
            plant_pot: req.flags.plant_pot.unwrap_or(false),
            lockpick: req.flags.lockpick.unwrap_or(false),
            is_skinned: req.flags.is_skinned.unwrap_or(false),
            boat: req.flags.boat.unwrap_or(false),
            buried: req.flags.buried.unwrap_or(false),
            can_dig: req.flags.can_dig.unwrap_or(false),
            detect_buried: req.flags.detect_buried.unwrap_or(false),
            ..Default::default()
        },
        damage_dice_count: req.damage_dice_count.unwrap_or(1),
        damage_dice_sides: req.damage_dice_sides.unwrap_or(4),
        damage_type,
        two_handed: req.two_handed.unwrap_or(false),
        weapon_skill: req.weapon_skill.as_ref().and_then(|s| WeaponSkill::from_str(s)),
        container_contents: Vec::new(),
        container_max_items: 0,
        container_max_weight: 0,
        container_closed: false,
        container_locked: false,
        container_key_vnum: req.container_key_vnum.clone(),
        weight_reduction: 0,
        liquid_type: req
            .liquid_type
            .as_ref()
            .and_then(|s| LiquidType::from_str(s))
            .unwrap_or_default(),
        liquid_current: req.liquid_current.unwrap_or(0),
        liquid_max: req.liquid_max.unwrap_or(0),
        liquid_poisoned: false,
        liquid_effects: req
            .liquid_effects
            .as_ref()
            .map(|effects| {
                effects
                    .iter()
                    .filter_map(|e| {
                        EffectType::from_str(&e.effect_type).map(|et| ItemEffect {
                            effect_type: et,
                            magnitude: e.magnitude,
                            duration: e.duration,
                            script_callback: None,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        food_nutrition: req.food_nutrition.unwrap_or(0),
        food_poisoned: false,
        food_spoil_duration: req.food_spoil_duration.unwrap_or(0),
        food_created_at: None,
        food_effects: req
            .food_effects
            .as_ref()
            .map(|effects| {
                effects
                    .iter()
                    .filter_map(|e| {
                        EffectType::from_str(&e.effect_type).map(|et| ItemEffect {
                            effect_type: et,
                            magnitude: e.magnitude,
                            duration: e.duration,
                            script_callback: None,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
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
        triggers: Vec::new(),
        vending_stock: Vec::new(),
        vending_sell_rate: 150,
        quality: 0,
        bait_uses: 0,
        holes: 0,
        medical_tier: req.medical_tier.unwrap_or(0),
        medical_uses: req.medical_uses.unwrap_or(0),
        treats_wound_types: req.treats_wound_types.unwrap_or_default(),
        max_treatable_wound: String::new(),
        transport_link: None,
        caliber: req.caliber,
        ammo_count: req.ammo_count.unwrap_or(0),
        ammo_damage_bonus: req.ammo_damage_bonus.unwrap_or(0),
        ranged_type: req.ranged_type,
        magazine_size: req.magazine_size.unwrap_or(0),
        loaded_ammo: 0,
        loaded_ammo_bonus: 0,
        loaded_ammo_vnum: None,
        fire_mode: req.fire_mode.unwrap_or_default(),
        supported_fire_modes: req.supported_fire_modes.unwrap_or_default(),
        noise_level: req.noise_level.unwrap_or_default(),
        ammo_effect_type: String::new(),
        ammo_effect_duration: 0,
        ammo_effect_damage: 0,
        loaded_ammo_effect_type: String::new(),
        loaded_ammo_effect_duration: 0,
        loaded_ammo_effect_damage: 0,
        attachment_slot: req.attachment_slot.unwrap_or_default(),
        attachment_accuracy_bonus: req.attachment_accuracy_bonus.unwrap_or(0),
        attachment_noise_reduction: req.attachment_noise_reduction.unwrap_or(0),
        attachment_magazine_bonus: req.attachment_magazine_bonus.unwrap_or(0),
        attachment_compatible_types: Vec::new(),
        plant_prototype_vnum: req.plant_prototype_vnum.unwrap_or_default(),
        fertilizer_duration: req.fertilizer_duration.unwrap_or(0),
        treats_infestation: req.treats_infestation.unwrap_or_default(),
    };

    // Auto-set default liquid effects for liquid containers
    if item.item_type == ItemType::LiquidContainer && item.liquid_effects.is_empty() {
        item.liquid_effects = item.liquid_type.default_effects();
    }

    state
        .db
        .save_item_data(item.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(
        &state.connections,
        &format!("[API] Item prototype '{}' created by {}", req.vnum, user.api_key.name),
    );

    Ok(Json(ItemResponse {
        success: true,
        data: item,
        refreshed_instances: None,
    }))
}

/// Update an existing item
async fn update_item(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(req): Json<UpdateItemRequest>,
) -> Result<Json<ItemResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut item = state
        .db
        .get_item_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Item '{}' not found", id)))?;

    // Apply updates
    if let Some(name) = req.name {
        item.name = name;
    }
    if let Some(short_desc) = req.short_desc {
        item.short_desc = short_desc;
    }
    if let Some(long_desc) = req.long_desc {
        item.long_desc = long_desc;
    }
    if let Some(note_content) = req.note_content {
        item.note_content = normalize_note_input(note_content)?;
    }
    if let Some(extras) = req.extra_descs {
        item.extra_descs = extras
            .into_iter()
            .map(|e| ExtraDesc {
                keywords: e.keywords,
                description: e.description,
            })
            .collect();
    }
    if let Some(key_vnum) = req.container_key_vnum {
        item.container_key_vnum = if key_vnum.is_empty() { None } else { Some(key_vnum) };
    }
    if let Some(world_max) = req.world_max_count {
        item.world_max_count = if world_max <= 0 { None } else { Some(world_max) };
    }
    if let Some(ref new_vnum) = req.vnum {
        // Check vnum uniqueness (allow keeping the same vnum)
        let current_vnum = item.vnum.as_deref().unwrap_or("");
        if new_vnum != current_vnum {
            if let Ok(Some(_)) = state.db.get_item_by_vnum(new_vnum) {
                return Err(ApiError::VnumInUse(format!("Vnum '{}' is already in use", new_vnum)));
            }
            item.vnum = Some(new_vnum.clone());
        }
    }
    if let Some(ref item_type_str) = req.item_type {
        if let Some(new_type) = parse_item_type(item_type_str) {
            item.item_type = new_type;
        } else {
            return Err(ApiError::InvalidInput(format!(
                "Invalid item type '{}'. Use: weapon, armor, container, liquid_container, food, key, gold, misc",
                item_type_str
            )));
        }
    }
    if let Some(keywords) = req.keywords {
        item.keywords = keywords;
    }
    if let Some(weight) = req.weight {
        item.weight = weight;
    }
    if let Some(value) = req.value {
        item.value = value;
    }
    if let Some(categories) = req.categories {
        item.categories = categories;
    }
    if let Some(flags) = req.flags {
        if let Some(v) = flags.no_drop {
            item.flags.no_drop = v;
        }
        if let Some(v) = flags.no_get {
            item.flags.no_get = v;
        }
        if let Some(v) = flags.no_remove {
            item.flags.no_remove = v;
        }
        if let Some(v) = flags.invisible {
            item.flags.invisible = v;
        }
        if let Some(v) = flags.glow {
            item.flags.glow = v;
        }
        if let Some(v) = flags.hum {
            item.flags.hum = v;
        }
        if let Some(v) = flags.magical {
            item.flags.magical = v;
        }
        if let Some(v) = flags.no_sell {
            item.flags.no_sell = v;
        }
        if let Some(v) = flags.unique {
            item.flags.unique = v;
        }
        if let Some(v) = flags.quest_item {
            item.flags.quest_item = v;
        }
        if let Some(v) = flags.vending {
            item.flags.vending = v;
        }
        if let Some(v) = flags.provides_light {
            item.flags.provides_light = v;
        }
        if let Some(v) = flags.night_vision {
            item.flags.night_vision = v;
        }
        if let Some(v) = flags.fishing_rod {
            item.flags.fishing_rod = v;
        }
        if let Some(v) = flags.bait {
            item.flags.bait = v;
        }
        if let Some(v) = flags.foraging_tool {
            item.flags.foraging_tool = v;
        }
        if let Some(v) = flags.waterproof {
            item.flags.waterproof = v;
        }
        if let Some(v) = flags.provides_warmth {
            item.flags.provides_warmth = v;
        }
        if let Some(v) = flags.reduces_glare {
            item.flags.reduces_glare = v;
        }
        if let Some(v) = flags.medical_tool {
            item.flags.medical_tool = v;
        }
        if let Some(v) = flags.preserves_contents {
            item.flags.preserves_contents = v;
        }
        if let Some(v) = flags.death_only {
            item.flags.death_only = v;
        }
        if let Some(v) = flags.atm {
            item.flags.atm = v;
        }
        if let Some(v) = flags.broken {
            item.flags.broken = v;
        }
        if let Some(v) = flags.plant_pot {
            item.flags.plant_pot = v;
        }
        if let Some(v) = flags.lockpick {
            item.flags.lockpick = v;
        }
        if let Some(v) = flags.is_skinned {
            item.flags.is_skinned = v;
        }
        if let Some(v) = flags.boat {
            item.flags.boat = v;
        }
        if let Some(v) = flags.buried {
            item.flags.buried = v;
        }
        if let Some(v) = flags.can_dig {
            item.flags.can_dig = v;
        }
        if let Some(v) = flags.detect_buried {
            item.flags.detect_buried = v;
        }
    }
    // Gardening fields
    if let Some(plant_prototype_vnum) = req.plant_prototype_vnum {
        item.plant_prototype_vnum = plant_prototype_vnum;
    }
    if let Some(fertilizer_duration) = req.fertilizer_duration {
        item.fertilizer_duration = fertilizer_duration;
    }
    if let Some(treats_infestation) = req.treats_infestation {
        item.treats_infestation = treats_infestation;
    }
    // Firearm fields
    if let Some(caliber) = req.caliber {
        item.caliber = Some(caliber);
    }
    if let Some(ranged_type) = req.ranged_type {
        item.ranged_type = Some(ranged_type);
    }
    if let Some(magazine_size) = req.magazine_size {
        item.magazine_size = magazine_size;
    }
    if let Some(fire_mode) = req.fire_mode {
        item.fire_mode = fire_mode;
    }
    if let Some(supported_fire_modes) = req.supported_fire_modes {
        item.supported_fire_modes = supported_fire_modes;
    }
    if let Some(noise_level) = req.noise_level {
        item.noise_level = noise_level;
    }
    if let Some(two_handed) = req.two_handed {
        item.two_handed = two_handed;
    }
    // Ammo fields
    if let Some(ammo_count) = req.ammo_count {
        item.ammo_count = ammo_count;
    }
    if let Some(ammo_damage_bonus) = req.ammo_damage_bonus {
        item.ammo_damage_bonus = ammo_damage_bonus;
    }
    // Attachment fields
    if let Some(attachment_slot) = req.attachment_slot {
        item.attachment_slot = attachment_slot;
    }
    if let Some(attachment_accuracy_bonus) = req.attachment_accuracy_bonus {
        item.attachment_accuracy_bonus = attachment_accuracy_bonus;
    }
    if let Some(attachment_noise_reduction) = req.attachment_noise_reduction {
        item.attachment_noise_reduction = attachment_noise_reduction;
    }
    if let Some(attachment_magazine_bonus) = req.attachment_magazine_bonus {
        item.attachment_magazine_bonus = attachment_magazine_bonus;
    }
    // Weapon/armor fields
    if let Some(dice_count) = req.damage_dice_count {
        item.damage_dice_count = dice_count;
    }
    if let Some(dice_sides) = req.damage_dice_sides {
        item.damage_dice_sides = dice_sides;
    }
    if let Some(ref dt) = req.damage_type {
        item.damage_type = DamageType::from_str(dt).unwrap_or(item.damage_type);
    }
    if let Some(ac) = req.armor_class {
        item.armor_class = Some(ac);
    }
    if let Some(hb) = req.hit_bonus {
        item.hit_bonus = hb;
    }
    if let Some(db) = req.damage_bonus {
        item.damage_bonus = db;
    }
    if let Some(ref loc_str) = req.wear_location {
        item.wear_locations = WearLocation::from_str(loc_str).map(|l| vec![l]).unwrap_or_default();
    }
    if let Some(ref ws) = req.weapon_skill {
        item.weapon_skill = WeaponSkill::from_str(ws);
    }
    // Liquid container fields
    if let Some(ref lt) = req.liquid_type {
        let new_type = LiquidType::from_str(lt).unwrap_or_default();
        let type_changed = item.liquid_type != new_type;
        item.liquid_type = new_type;
        // Always re-apply defaults when the type changes, so stale effects from the
        // previous type (or from fallback-to-water defaults) get replaced. A caller
        // that wants custom effects can pass them via `liquid_effects` in the same
        // request — that field is applied below and wins over these defaults.
        if type_changed {
            item.liquid_effects = item.liquid_type.default_effects();
        }
    }
    if let Some(lc) = req.liquid_current {
        item.liquid_current = lc;
    }
    if let Some(lm) = req.liquid_max {
        item.liquid_max = lm;
    }
    if let Some(effects) = req.liquid_effects {
        item.liquid_effects = effects
            .iter()
            .filter_map(|e| {
                EffectType::from_str(&e.effect_type).map(|et| ItemEffect {
                    effect_type: et,
                    magnitude: e.magnitude,
                    duration: e.duration,
                    script_callback: None,
                })
            })
            .collect();
    }
    // Medical fields
    if let Some(mt) = req.medical_tier {
        item.medical_tier = mt;
    }
    if let Some(mu) = req.medical_uses {
        item.medical_uses = mu;
    }
    if let Some(twt) = req.treats_wound_types {
        item.treats_wound_types = twt;
    }
    // Food fields
    if let Some(fn_val) = req.food_nutrition {
        item.food_nutrition = fn_val;
    }
    if let Some(fsd) = req.food_spoil_duration {
        item.food_spoil_duration = fsd;
    }
    if let Some(effects) = req.food_effects {
        item.food_effects = effects
            .iter()
            .filter_map(|e| {
                EffectType::from_str(&e.effect_type).map(|et| ItemEffect {
                    effect_type: et,
                    magnitude: e.magnitude,
                    duration: e.duration,
                    script_callback: None,
                })
            })
            .collect();
    }

    state
        .db
        .save_item_data(item.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let refreshed = refresh_item_instances(&state.db, &item);

    let vnum_display = item.vnum.as_ref().unwrap_or(&item.id.to_string()).clone();
    if refreshed > 0 {
        notify_builders(
            &state.connections,
            &format!(
                "[API] Item '{}' updated by {} ({} instance(s) refreshed)",
                vnum_display, user.api_key.name, refreshed
            ),
        );
    } else {
        notify_builders(
            &state.connections,
            &format!("[API] Item '{}' updated by {}", vnum_display, user.api_key.name),
        );
    }

    Ok(Json(ItemResponse {
        success: true,
        data: item,
        refreshed_instances: Some(refreshed),
    }))
}

/// Delete an item
async fn delete_item(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let item = state
        .db
        .get_item_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Item '{}' not found", id)))?;

    let item_name = item.vnum.clone().unwrap_or_else(|| item.id.to_string());

    state
        .db
        .delete_item(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(
        &state.connections,
        &format!("[API] Item '{}' deleted by {}", item_name, user.api_key.name),
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Item '{}' deleted", item_name)
    })))
}

#[derive(Deserialize)]
pub struct AddItemTriggerRequest {
    /// One of: on_get, on_drop, on_use, on_examine, on_prompt (leading "on_" optional).
    pub trigger_type: String,
    /// Script filename without extension (e.g. "smart_watch"), or "@template" form.
    pub script_name: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_item_trigger_chance")]
    pub chance: i32,
}

fn default_item_trigger_chance() -> i32 {
    100
}

fn parse_item_trigger_type(raw: &str) -> Result<ItemTriggerType, ApiError> {
    match raw.to_lowercase().as_str() {
        "get" | "on_get" => Ok(ItemTriggerType::OnGet),
        "drop" | "on_drop" => Ok(ItemTriggerType::OnDrop),
        "use" | "on_use" => Ok(ItemTriggerType::OnUse),
        "examine" | "on_examine" => Ok(ItemTriggerType::OnExamine),
        "prompt" | "on_prompt" => Ok(ItemTriggerType::OnPrompt),
        _ => Err(ApiError::InvalidInput(format!(
            "Invalid trigger type '{}'. Use: get, drop, use, examine, prompt",
            raw
        ))),
    }
}

/// Add a trigger to an item prototype
async fn add_trigger(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(req): Json<AddItemTriggerRequest>,
) -> Result<Json<ItemResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut item = state
        .db
        .get_item_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Item '{}' not found", id)))?;

    let trigger_type = parse_item_trigger_type(&req.trigger_type)?;

    item.triggers.push(ItemTrigger {
        trigger_type,
        script_name: req.script_name,
        enabled: true,
        chance: req.chance,
        args: req.args,
    });

    state
        .db
        .save_item_data(item.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let refreshed = refresh_item_instances(&state.db, &item);

    Ok(Json(ItemResponse {
        success: true,
        data: item,
        refreshed_instances: Some(refreshed),
    }))
}

/// Remove a trigger from an item prototype by index
async fn remove_trigger(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path((id, index)): Path<(String, usize)>,
) -> Result<Json<ItemResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut item = state
        .db
        .get_item_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Item '{}' not found", id)))?;

    if index >= item.triggers.len() {
        return Err(ApiError::NotFound(format!("Trigger index {} not found", index)));
    }

    item.triggers.remove(index);

    state
        .db
        .save_item_data(item.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let refreshed = refresh_item_instances(&state.db, &item);

    Ok(Json(ItemResponse {
        success: true,
        data: item,
        refreshed_instances: Some(refreshed),
    }))
}

/// Add an extra description (sub-keyword lore) to an item
async fn add_extra_desc(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(req): Json<AddExtraDescRequest>,
) -> Result<Json<ItemResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut item = state
        .db
        .get_item_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Item '{}' not found", id)))?;

    if req.keywords.is_empty() {
        return Err(ApiError::InvalidInput("keywords must not be empty".into()));
    }

    item.extra_descs.push(ExtraDesc {
        keywords: req.keywords,
        description: req.description,
    });

    state
        .db
        .save_item_data(item.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let refreshed = refresh_item_instances(&state.db, &item);

    Ok(Json(ItemResponse {
        success: true,
        data: item,
        refreshed_instances: Some(refreshed),
    }))
}

/// Remove an extra description by keyword
async fn remove_extra_desc(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path((id, keyword)): Path<(String, String)>,
) -> Result<Json<ItemResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut item = state
        .db
        .get_item_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Item '{}' not found", id)))?;

    let keyword_lower = keyword.to_lowercase();
    let original_len = item.extra_descs.len();
    item.extra_descs
        .retain(|ed| !ed.keywords.iter().any(|k| k.to_lowercase() == keyword_lower));

    if item.extra_descs.len() == original_len {
        return Err(ApiError::NotFound(format!(
            "Extra description with keyword '{}' not found",
            keyword
        )));
    }

    state
        .db
        .save_item_data(item.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let refreshed = refresh_item_instances(&state.db, &item);

    Ok(Json(ItemResponse {
        success: true,
        data: item,
        refreshed_instances: Some(refreshed),
    }))
}

/// Spawn an item instance from a prototype
async fn spawn_item(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(vnum): Path<String>,
    Json(req): Json<SpawnItemRequest>,
) -> Result<Json<ItemResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    // Get the prototype
    let prototype = state
        .db
        .get_item_by_vnum(&vnum)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Item prototype '{}' not found", vnum)))?;

    if !prototype.is_prototype {
        return Err(ApiError::InvalidInput(format!("Item '{}' is not a prototype", vnum)));
    }

    // Verify room exists
    let room_uuid =
        Uuid::parse_str(&req.room_id).map_err(|_| ApiError::InvalidInput("Invalid room_id UUID format".into()))?;

    let _room = state
        .db
        .get_room_data(&room_uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Room '{}' not found", req.room_id)))?;

    // Clone the prototype to create an instance
    let mut instance = prototype.clone();
    instance.id = Uuid::new_v4();
    instance.is_prototype = false;
    instance.location = ItemLocation::Room(room_uuid);

    state
        .db
        .save_item_data(instance.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(
        &state.connections,
        &format!("[API] Item '{}' spawned in room by {}", vnum, user.api_key.name),
    );

    Ok(Json(ItemResponse {
        success: true,
        data: instance,
        refreshed_instances: None,
    }))
}

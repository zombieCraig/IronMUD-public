//! Mobile CRUD endpoints

use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use super::{
    ApiState,
    auth::{AuthenticatedUser, can_read, can_write},
    error::ApiError,
    notify_builders,
};
use crate::{
    ActivityState, CombatState, DamageType, MobileData, MobileFlags, MobileTrigger, MobileTriggerType, NeedsState,
    RoutineEntry, SimulationConfig,
};

pub fn routes() -> Router<Arc<ApiState>> {
    Router::new()
        .route("/", get(list_mobiles).post(create_mobile))
        .route("/prototypes", get(list_prototypes))
        .route("/prototypes/summary", get(list_prototypes_summary))
        .route("/:id", get(get_mobile).put(update_mobile).delete(delete_mobile))
        .route("/by-vnum/:vnum", get(get_mobile_by_vnum))
        .route("/:id/dialogue", post(add_dialogue))
        .route("/:id/dialogue/:keyword", delete(remove_dialogue))
        .route("/:id/routine", post(add_routine_entry))
        .route("/:id/routine/:index", delete(remove_routine_entry))
        .route("/:id/triggers", post(add_trigger))
        .route("/:id/triggers/:index", delete(remove_trigger))
        .route("/:vnum/spawn", post(spawn_mobile))
}

#[derive(Deserialize)]
pub struct ListMobilesQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Deserialize)]
pub struct CreateMobileRequest {
    pub name: String,
    pub short_desc: String,
    pub long_desc: String,
    pub vnum: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default = "default_level")]
    pub level: i32,
    #[serde(default = "default_hp")]
    pub max_hp: i32,
    #[serde(default)]
    pub damage_dice: Option<String>,
    #[serde(default = "default_ac")]
    pub armor_class: i32,
    #[serde(default)]
    pub perception: i32,
    #[serde(default)]
    pub flags: MobileFlagsRequest,
    // Healer config
    #[serde(default)]
    pub healer_type: Option<String>,
    #[serde(default)]
    pub healing_free: Option<bool>,
    #[serde(default)]
    pub healing_cost_multiplier: Option<i32>,
    // Shop config
    #[serde(default)]
    pub shop_sell_rate: Option<i32>,
    #[serde(default)]
    pub shop_buy_rate: Option<i32>,
    #[serde(default)]
    pub shop_buys_types: Option<Vec<String>>,
    #[serde(default)]
    pub shop_buys_categories: Option<Vec<String>>,
    #[serde(default)]
    pub shop_min_value: Option<i32>,
    #[serde(default)]
    pub shop_max_value: Option<i32>,
    #[serde(default)]
    pub shop_extra_types: Option<Vec<String>>,
    #[serde(default)]
    pub shop_extra_categories: Option<Vec<String>>,
    #[serde(default)]
    pub shop_deny_types: Option<Vec<String>>,
    #[serde(default)]
    pub shop_deny_categories: Option<Vec<String>>,
    #[serde(default)]
    pub shop_stock: Option<Vec<String>>,
    #[serde(default)]
    pub shop_preset_vnum: Option<String>,
    // Daily routine
    #[serde(default)]
    pub daily_routine: Option<Vec<RoutineEntryRequest>>,
    // Needs simulation
    #[serde(default)]
    pub simulation: Option<SimulationConfigRequest>,
    #[serde(default)]
    pub world_max_count: Option<i32>,
    /// Helper-system faction tag. None/empty falls back to Circle-stock semantics.
    #[serde(default)]
    pub faction: Option<String>,
}

#[derive(Deserialize)]
pub struct SimulationConfigRequest {
    pub home_room_vnum: String,
    pub work_room_vnum: String,
    #[serde(default)]
    pub shop_room_vnum: String,
    #[serde(default)]
    pub preferred_food_vnum: String,
    #[serde(default = "default_work_pay")]
    pub work_pay: i32,
    #[serde(default = "default_work_start")]
    pub work_start_hour: u8,
    #[serde(default = "default_work_end")]
    pub work_end_hour: u8,
    #[serde(default)]
    pub hunger_decay_rate: i32,
    #[serde(default)]
    pub energy_decay_rate: i32,
    #[serde(default)]
    pub comfort_decay_rate: i32,
    #[serde(default = "default_low_gold_threshold")]
    pub low_gold_threshold: i32,
}

fn default_work_pay() -> i32 {
    50
}
fn default_work_start() -> u8 {
    8
}
fn default_work_end() -> u8 {
    17
}
fn default_low_gold_threshold() -> i32 {
    10
}

fn convert_simulation_config(req: SimulationConfigRequest) -> SimulationConfig {
    SimulationConfig {
        home_room_vnum: req.home_room_vnum,
        work_room_vnum: req.work_room_vnum,
        shop_room_vnum: req.shop_room_vnum,
        preferred_food_vnum: req.preferred_food_vnum,
        work_pay: req.work_pay,
        work_start_hour: req.work_start_hour,
        work_end_hour: req.work_end_hour,
        hunger_decay_rate: req.hunger_decay_rate,
        energy_decay_rate: req.energy_decay_rate,
        comfort_decay_rate: req.comfort_decay_rate,
        low_gold_threshold: req.low_gold_threshold,
    }
}

fn default_level() -> i32 {
    1
}
fn default_hp() -> i32 {
    20
}
fn default_ac() -> i32 {
    10
}

fn parse_activity(s: &str) -> ActivityState {
    match s.to_lowercase().as_str() {
        "working" => ActivityState::Working,
        "sleeping" => ActivityState::Sleeping,
        "patrolling" => ActivityState::Patrolling,
        "offduty" | "off_duty" => ActivityState::OffDuty,
        "socializing" => ActivityState::Socializing,
        "eating" => ActivityState::Eating,
        other => ActivityState::Custom(other.to_string()),
    }
}

fn convert_routine_entries(entries: Vec<RoutineEntryRequest>) -> Vec<RoutineEntry> {
    entries
        .into_iter()
        .map(|e| RoutineEntry {
            start_hour: e.start_hour,
            activity: parse_activity(&e.activity),
            destination_vnum: e.destination_vnum,
            transition_message: e.transition_message,
            suppress_wander: e.suppress_wander,
            dialogue_overrides: e.dialogue_overrides,
        })
        .collect()
}

#[derive(Deserialize, Default)]
/// Builder-facing `MobileFlags`. Every field is optional so callers can
/// send only the flags they want to change; unmentioned flags are
/// preserved on update and default to `false` on create.
pub struct MobileFlagsRequest {
    #[serde(default)]
    pub aggressive: Option<bool>,
    #[serde(default)]
    pub sentinel: Option<bool>,
    #[serde(default)]
    pub scavenger: Option<bool>,
    #[serde(default)]
    pub shopkeeper: Option<bool>,
    #[serde(default)]
    pub healer: Option<bool>,
    #[serde(default)]
    pub no_attack: Option<bool>,
    #[serde(default)]
    pub cowardly: Option<bool>,
    #[serde(default)]
    pub can_open_doors: Option<bool>,
    #[serde(default)]
    pub leasing_agent: Option<bool>,
    #[serde(default)]
    pub guard: Option<bool>,
    #[serde(default)]
    pub helper: Option<bool>,
    #[serde(default)]
    pub thief: Option<bool>,
    #[serde(default)]
    pub cant_swim: Option<bool>,
    #[serde(default)]
    pub poisonous: Option<bool>,
    #[serde(default)]
    pub fiery: Option<bool>,
    #[serde(default)]
    pub chilling: Option<bool>,
    #[serde(default)]
    pub corrosive: Option<bool>,
    #[serde(default)]
    pub shocking: Option<bool>,
    #[serde(default)]
    pub unique: Option<bool>,
    #[serde(default)]
    pub stay_zone: Option<bool>,
    #[serde(default)]
    pub aware: Option<bool>,
    #[serde(default)]
    pub memory: Option<bool>,
}

#[derive(Deserialize)]
pub struct RoutineEntryRequest {
    pub start_hour: u8,
    pub activity: String,
    #[serde(default)]
    pub destination_vnum: Option<String>,
    #[serde(default)]
    pub transition_message: Option<String>,
    #[serde(default)]
    pub suppress_wander: bool,
    #[serde(default)]
    pub dialogue_overrides: HashMap<String, String>,
}

#[derive(Deserialize)]
pub struct UpdateMobileRequest {
    pub name: Option<String>,
    pub short_desc: Option<String>,
    pub long_desc: Option<String>,
    pub vnum: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub level: Option<i32>,
    pub max_hp: Option<i32>,
    pub armor_class: Option<i32>,
    pub perception: Option<i32>,
    pub gold: Option<i32>,
    pub flags: Option<MobileFlagsRequest>,
    // Healer config
    #[serde(default)]
    pub healer_type: Option<String>,
    #[serde(default)]
    pub healing_free: Option<bool>,
    #[serde(default)]
    pub healing_cost_multiplier: Option<i32>,
    // Shop config
    #[serde(default)]
    pub shop_sell_rate: Option<i32>,
    #[serde(default)]
    pub shop_buy_rate: Option<i32>,
    #[serde(default)]
    pub shop_buys_types: Option<Vec<String>>,
    #[serde(default)]
    pub shop_buys_categories: Option<Vec<String>>,
    #[serde(default)]
    pub shop_min_value: Option<i32>,
    #[serde(default)]
    pub shop_max_value: Option<i32>,
    #[serde(default)]
    pub shop_extra_types: Option<Vec<String>>,
    #[serde(default)]
    pub shop_extra_categories: Option<Vec<String>>,
    #[serde(default)]
    pub shop_deny_types: Option<Vec<String>>,
    #[serde(default)]
    pub shop_deny_categories: Option<Vec<String>>,
    #[serde(default)]
    pub shop_stock: Option<Vec<String>>,
    #[serde(default)]
    pub shop_preset_vnum: Option<String>,
    // Daily routine
    #[serde(default)]
    pub daily_routine: Option<Vec<RoutineEntryRequest>>,
    // Needs simulation
    #[serde(default)]
    pub simulation: Option<SimulationConfigRequest>,
    /// Set to true to remove simulation config
    #[serde(default)]
    pub remove_simulation: Option<bool>,
    #[serde(default)]
    pub world_max_count: Option<i32>,
    /// Helper-system faction tag. Empty string clears to None.
    #[serde(default)]
    pub faction: Option<String>,
}

#[derive(Deserialize)]
pub struct AddDialogueRequest {
    pub keyword: String,
    pub response: String,
}

#[derive(Deserialize)]
pub struct SpawnMobileRequest {
    pub room_id: String,
}

#[derive(Deserialize)]
pub struct AddMobileTriggerRequest {
    pub trigger_type: String,
    pub script_name: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_trigger_interval")]
    pub interval_secs: i64,
    #[serde(default = "default_trigger_chance")]
    pub chance: i32,
}

fn default_trigger_interval() -> i64 {
    60
}
fn default_trigger_chance() -> i32 {
    100
}

#[derive(Serialize)]
pub struct MobileResponse {
    pub success: bool,
    pub data: MobileData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refreshed_instances: Option<usize>,
}

#[derive(Serialize)]
pub struct MobilesListResponse {
    pub success: bool,
    pub data: Vec<MobileData>,
    pub total: usize,
}

#[derive(Serialize)]
pub struct MobileSummary {
    pub vnum: String,
    pub name: String,
    pub level: i32,
    pub max_hp: i32,
    pub armor_class: i32,
    pub damage_dice: String,
    pub flags: Vec<String>,
    pub has_dialogue: bool,
    pub has_routine: bool,
    pub trigger_count: usize,
}

#[derive(Serialize)]
pub struct MobilesSummaryResponse {
    pub success: bool,
    pub data: Vec<MobileSummary>,
    pub total: usize,
}

#[derive(Deserialize)]
pub struct MobileSummaryQuery {
    pub vnum_prefix: Option<String>,
}

impl MobileSummary {
    pub fn from_mobile(mobile: &MobileData) -> Self {
        let mut flags = Vec::new();
        if mobile.flags.aggressive {
            flags.push("aggressive".to_string());
        }
        if mobile.flags.sentinel {
            flags.push("sentinel".to_string());
        }
        if mobile.flags.scavenger {
            flags.push("scavenger".to_string());
        }
        if mobile.flags.shopkeeper {
            flags.push("shopkeeper".to_string());
        }
        if mobile.flags.healer {
            flags.push("healer".to_string());
        }
        if mobile.flags.no_attack {
            flags.push("no_attack".to_string());
        }
        if mobile.flags.cowardly {
            flags.push("cowardly".to_string());
        }
        if mobile.flags.can_open_doors {
            flags.push("can_open_doors".to_string());
        }
        if mobile.flags.leasing_agent {
            flags.push("leasing_agent".to_string());
        }
        if mobile.flags.guard {
            flags.push("guard".to_string());
        }
        if mobile.flags.helper {
            flags.push("helper".to_string());
        }
        if mobile.flags.thief {
            flags.push("thief".to_string());
        }
        if mobile.flags.cant_swim {
            flags.push("cant_swim".to_string());
        }
        if mobile.flags.poisonous {
            flags.push("poisonous".to_string());
        }
        if mobile.flags.fiery {
            flags.push("fiery".to_string());
        }
        if mobile.flags.chilling {
            flags.push("chilling".to_string());
        }
        if mobile.flags.corrosive {
            flags.push("corrosive".to_string());
        }
        if mobile.flags.shocking {
            flags.push("shocking".to_string());
        }
        if mobile.flags.stay_zone {
            flags.push("stay_zone".to_string());
        }
        if mobile.flags.aware {
            flags.push("aware".to_string());
        }
        if mobile.flags.memory {
            flags.push("memory".to_string());
        }

        MobileSummary {
            vnum: mobile.vnum.clone(),
            name: mobile.name.clone(),
            level: mobile.level,
            max_hp: mobile.max_hp,
            armor_class: mobile.armor_class,
            damage_dice: mobile.damage_dice.clone(),
            flags,
            has_dialogue: !mobile.dialogue.is_empty(),
            has_routine: !mobile.daily_routine.is_empty(),
            trigger_count: mobile.triggers.len(),
        }
    }
}

/// Refresh all spawned instances of a mobile prototype from the prototype's current data.
/// Returns the number of successfully refreshed instances.
fn refresh_mobile_instances(db: &crate::db::Db, mobile: &MobileData) -> usize {
    if !mobile.is_prototype {
        return 0;
    }
    let instances = match db.get_mobile_instances_by_vnum(&mobile.vnum) {
        Ok(instances) => instances,
        Err(_) => return 0,
    };
    let mut count = 0;
    for instance in &instances {
        if db.refresh_mobile_from_prototype(&instance.id).is_ok() {
            count += 1;
        }
    }
    count
}

/// List mobiles with pagination
async fn list_mobiles(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(query): Query<ListMobilesQuery>,
) -> Result<Json<MobilesListResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let mobiles = state
        .db
        .list_all_mobiles()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let total = mobiles.len();
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(100);

    let mobiles: Vec<MobileData> = mobiles.into_iter().skip(offset).take(limit).collect();

    Ok(Json(MobilesListResponse {
        success: true,
        data: mobiles,
        total,
    }))
}

/// List prototype mobiles only
async fn list_prototypes(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<MobilesListResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let mobiles: Vec<MobileData> = state
        .db
        .list_all_mobiles()
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .into_iter()
        .filter(|m| m.is_prototype)
        .collect();

    let total = mobiles.len();

    Ok(Json(MobilesListResponse {
        success: true,
        data: mobiles,
        total,
    }))
}

/// List prototype mobile summaries (compact)
async fn list_prototypes_summary(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(query): Query<MobileSummaryQuery>,
) -> Result<Json<MobilesSummaryResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let mobiles: Vec<MobileData> = state
        .db
        .list_all_mobiles()
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .into_iter()
        .filter(|m| m.is_prototype)
        .collect();

    let summaries: Vec<MobileSummary> = mobiles
        .iter()
        .filter(|m| {
            if let Some(ref prefix) = query.vnum_prefix {
                m.vnum.starts_with(&format!("{}:", prefix))
            } else {
                true
            }
        })
        .map(MobileSummary::from_mobile)
        .collect();

    let total = summaries.len();

    Ok(Json(MobilesSummaryResponse {
        success: true,
        data: summaries,
        total,
    }))
}

/// Get mobile by UUID
async fn get_mobile(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<MobileResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mobile = state
        .db
        .get_mobile_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Mobile '{}' not found", id)))?;

    Ok(Json(MobileResponse {
        success: true,
        data: mobile,
        refreshed_instances: None,
    }))
}

/// Get mobile by vnum
async fn get_mobile_by_vnum(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(vnum): Path<String>,
) -> Result<Json<MobileResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let mobile = state
        .db
        .get_mobile_by_vnum(&vnum)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Mobile with vnum '{}' not found", vnum)))?;

    Ok(Json(MobileResponse {
        success: true,
        data: mobile,
        refreshed_instances: None,
    }))
}

/// Create a new mobile prototype
async fn create_mobile(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<CreateMobileRequest>,
) -> Result<Json<MobileResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    // Check vnum uniqueness
    if state
        .db
        .get_mobile_by_vnum(&req.vnum)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .is_some()
    {
        return Err(ApiError::VnumInUse(format!("Vnum '{}' is already in use", req.vnum)));
    }

    // Use provided damage dice string or default
    let damage_dice = req.damage_dice.clone().unwrap_or_else(|| "1d4".to_string());

    let mobile = MobileData {
        id: Uuid::new_v4(),
        name: req.name,
        short_desc: req.short_desc,
        long_desc: req.long_desc,
        vnum: req.vnum.clone(),
        keywords: req.keywords,
        is_prototype: true,
        world_max_count: req.world_max_count,
        current_room_id: None,
        max_hp: req.max_hp,
        current_hp: req.max_hp,
        max_stamina: 100,
        current_stamina: 100,
        level: req.level,
        armor_class: req.armor_class,
        hit_modifier: 0,
        damage_dice,
        damage_type: DamageType::default(),
        stat_str: 10,
        stat_dex: 10,
        stat_con: 10,
        stat_int: 10,
        stat_wis: 10,
        stat_cha: 10,
        combat: CombatState::default(),
        wounds: Vec::new(),
        ongoing_effects: Vec::new(),
        scars: HashMap::new(),
        gold: 0,
        flags: MobileFlags {
            aggressive: req.flags.aggressive.unwrap_or(false),
            sentinel: req.flags.sentinel.unwrap_or(false),
            scavenger: req.flags.scavenger.unwrap_or(false),
            shopkeeper: req.flags.shopkeeper.unwrap_or(false),
            healer: req.flags.healer.unwrap_or(false),
            no_attack: req.flags.no_attack.unwrap_or(false),
            cowardly: req.flags.cowardly.unwrap_or(false),
            can_open_doors: req.flags.can_open_doors.unwrap_or(false),
            leasing_agent: req.flags.leasing_agent.unwrap_or(false),
            guard: req.flags.guard.unwrap_or(false),
            helper: req.flags.helper.unwrap_or(false),
            thief: req.flags.thief.unwrap_or(false),
            cant_swim: req.flags.cant_swim.unwrap_or(false),
            poisonous: req.flags.poisonous.unwrap_or(false),
            fiery: req.flags.fiery.unwrap_or(false),
            chilling: req.flags.chilling.unwrap_or(false),
            corrosive: req.flags.corrosive.unwrap_or(false),
            shocking: req.flags.shocking.unwrap_or(false),
            unique: req.flags.unique.unwrap_or(false),
            stay_zone: req.flags.stay_zone.unwrap_or(false),
            aware: req.flags.aware.unwrap_or(false),
            memory: req.flags.memory.unwrap_or(false),
        },
        dialogue: HashMap::new(),
        shop_stock: req.shop_stock.unwrap_or_default(),
        shop_inventory: Vec::new(),
        shop_buys_types: req.shop_buys_types.unwrap_or_default(),
        shop_sell_rate: req.shop_sell_rate.unwrap_or(150),
        shop_buy_rate: req.shop_buy_rate.unwrap_or(50),
        healer_type: req.healer_type.unwrap_or_default(),
        healing_free: req.healing_free.unwrap_or(false),
        healing_cost_multiplier: req.healing_cost_multiplier.unwrap_or(100),
        triggers: Vec::new(),
        transport_route: None,
        property_templates: Vec::new(),
        leasing_area_id: None,
        shop_buys_categories: req.shop_buys_categories.unwrap_or_default(),
        shop_preset_vnum: req.shop_preset_vnum.unwrap_or_default(),
        shop_extra_types: req.shop_extra_types.unwrap_or_default(),
        shop_extra_categories: req.shop_extra_categories.unwrap_or_default(),
        shop_deny_types: req.shop_deny_types.unwrap_or_default(),
        shop_deny_categories: req.shop_deny_categories.unwrap_or_default(),
        shop_min_value: req.shop_min_value.unwrap_or(0),
        shop_max_value: req.shop_max_value.unwrap_or(0),
        is_unconscious: false,
        bleedout_rounds_remaining: 0,
        pursuit_target_name: String::new(),
        pursuit_target_room: None,
        pursuit_direction: String::new(),
        pursuit_certain: false,
        embedded_projectiles: Vec::new(),
        daily_routine: req.daily_routine.map(convert_routine_entries).unwrap_or_default(),
        schedule_visible: false,
        current_activity: crate::ActivityState::default(),
        routine_destination_room: None,
        perception: req.perception,
        simulation: req.simulation.map(convert_simulation_config),
        needs: None,
        characteristics: None,
        household_id: None,
        faction: req.faction.clone().filter(|s| !s.is_empty()),
        relationships: Vec::new(),
        resident_of: None,
        social: None,
        active_buffs: Vec::new(),
        adoption_pending: false,
        home_area_id: None,
        remembered_enemies: Vec::new(),
    };

    state
        .db
        .save_mobile_data(mobile.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(
        &state.connections,
        &format!("[API] Mobile prototype '{}' created by {}", req.vnum, user.api_key.name),
    );

    Ok(Json(MobileResponse {
        success: true,
        data: mobile,
        refreshed_instances: None,
    }))
}

/// Update an existing mobile
async fn update_mobile(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(req): Json<UpdateMobileRequest>,
) -> Result<Json<MobileResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut mobile = state
        .db
        .get_mobile_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Mobile '{}' not found", id)))?;

    // Apply updates
    if let Some(name) = req.name {
        mobile.name = name;
    }
    if let Some(short_desc) = req.short_desc {
        mobile.short_desc = short_desc;
    }
    if let Some(long_desc) = req.long_desc {
        mobile.long_desc = long_desc;
    }
    if let Some(ref new_vnum) = req.vnum {
        // Check vnum uniqueness (allow keeping the same vnum)
        if new_vnum != &mobile.vnum {
            if let Ok(Some(_)) = state.db.get_mobile_by_vnum(new_vnum) {
                return Err(ApiError::VnumInUse(format!("Vnum '{}' is already in use", new_vnum)));
            }
            mobile.vnum = new_vnum.clone();
        }
    }
    if let Some(keywords) = req.keywords {
        mobile.keywords = keywords;
    }
    if let Some(gold) = req.gold {
        mobile.gold = gold;
    }
    if let Some(level) = req.level {
        mobile.level = level;
    }
    if let Some(max_hp) = req.max_hp {
        mobile.max_hp = max_hp;
        if mobile.current_hp > max_hp {
            mobile.current_hp = max_hp;
        }
    }
    if let Some(armor_class) = req.armor_class {
        mobile.armor_class = armor_class;
    }
    if let Some(perception) = req.perception {
        mobile.perception = perception;
    }
    if let Some(flags) = req.flags {
        if let Some(v) = flags.aggressive {
            mobile.flags.aggressive = v;
        }
        if let Some(v) = flags.sentinel {
            mobile.flags.sentinel = v;
        }
        if let Some(v) = flags.scavenger {
            mobile.flags.scavenger = v;
        }
        if let Some(v) = flags.shopkeeper {
            mobile.flags.shopkeeper = v;
        }
        if let Some(v) = flags.healer {
            mobile.flags.healer = v;
        }
        if let Some(v) = flags.no_attack {
            mobile.flags.no_attack = v;
        }
        if let Some(v) = flags.cowardly {
            mobile.flags.cowardly = v;
        }
        if let Some(v) = flags.can_open_doors {
            mobile.flags.can_open_doors = v;
        }
        if let Some(v) = flags.leasing_agent {
            mobile.flags.leasing_agent = v;
        }
        if let Some(v) = flags.guard {
            mobile.flags.guard = v;
        }
        if let Some(v) = flags.helper {
            mobile.flags.helper = v;
        }
        if let Some(v) = flags.thief {
            mobile.flags.thief = v;
        }
        if let Some(v) = flags.cant_swim {
            mobile.flags.cant_swim = v;
        }
        if let Some(v) = flags.poisonous {
            mobile.flags.poisonous = v;
        }
        if let Some(v) = flags.fiery {
            mobile.flags.fiery = v;
        }
        if let Some(v) = flags.chilling {
            mobile.flags.chilling = v;
        }
        if let Some(v) = flags.corrosive {
            mobile.flags.corrosive = v;
        }
        if let Some(v) = flags.shocking {
            mobile.flags.shocking = v;
        }
        if let Some(v) = flags.unique {
            mobile.flags.unique = v;
        }
        if let Some(v) = flags.stay_zone {
            mobile.flags.stay_zone = v;
        }
        if let Some(v) = flags.aware {
            mobile.flags.aware = v;
        }
        if let Some(v) = flags.memory {
            mobile.flags.memory = v;
        }
    }
    if let Some(world_max) = req.world_max_count {
        mobile.world_max_count = if world_max <= 0 { None } else { Some(world_max) };
    }
    if let Some(faction) = req.faction {
        mobile.faction = if faction.is_empty() { None } else { Some(faction) };
    }
    // Healer config
    if let Some(healer_type) = req.healer_type {
        mobile.healer_type = healer_type;
    }
    if let Some(healing_free) = req.healing_free {
        mobile.healing_free = healing_free;
    }
    if let Some(healing_cost_multiplier) = req.healing_cost_multiplier {
        mobile.healing_cost_multiplier = healing_cost_multiplier;
    }
    // Shop config
    if let Some(shop_sell_rate) = req.shop_sell_rate {
        mobile.shop_sell_rate = shop_sell_rate;
    }
    if let Some(shop_buy_rate) = req.shop_buy_rate {
        mobile.shop_buy_rate = shop_buy_rate;
    }
    if let Some(shop_buys_types) = req.shop_buys_types {
        mobile.shop_buys_types = shop_buys_types;
    }
    if let Some(shop_buys_categories) = req.shop_buys_categories {
        mobile.shop_buys_categories = shop_buys_categories;
    }
    if let Some(shop_min_value) = req.shop_min_value {
        mobile.shop_min_value = shop_min_value;
    }
    if let Some(shop_max_value) = req.shop_max_value {
        mobile.shop_max_value = shop_max_value;
    }
    if let Some(shop_extra_types) = req.shop_extra_types {
        mobile.shop_extra_types = shop_extra_types;
    }
    if let Some(shop_extra_categories) = req.shop_extra_categories {
        mobile.shop_extra_categories = shop_extra_categories;
    }
    if let Some(shop_deny_types) = req.shop_deny_types {
        mobile.shop_deny_types = shop_deny_types;
    }
    if let Some(shop_deny_categories) = req.shop_deny_categories {
        mobile.shop_deny_categories = shop_deny_categories;
    }
    if let Some(shop_stock) = req.shop_stock {
        mobile.shop_stock = shop_stock;
    }
    if let Some(shop_preset_vnum) = req.shop_preset_vnum {
        mobile.shop_preset_vnum = shop_preset_vnum;
    }
    // Daily routine
    if let Some(routine) = req.daily_routine {
        mobile.daily_routine = convert_routine_entries(routine);
    }
    // Needs simulation
    if let Some(true) = req.remove_simulation {
        mobile.simulation = None;
        mobile.needs = None;
    } else if let Some(sim_config) = req.simulation {
        mobile.simulation = Some(convert_simulation_config(sim_config));
        if mobile.needs.is_none() {
            mobile.needs = Some(NeedsState::default());
        }
    }

    state
        .db
        .save_mobile_data(mobile.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let refreshed = refresh_mobile_instances(&state.db, &mobile);

    if refreshed > 0 {
        notify_builders(
            &state.connections,
            &format!(
                "[API] Mobile '{}' updated by {} ({} instance(s) refreshed)",
                mobile.vnum, user.api_key.name, refreshed
            ),
        );
    } else {
        notify_builders(
            &state.connections,
            &format!("[API] Mobile '{}' updated by {}", mobile.vnum, user.api_key.name),
        );
    }

    Ok(Json(MobileResponse {
        success: true,
        data: mobile,
        refreshed_instances: Some(refreshed),
    }))
}

/// Delete a mobile
async fn delete_mobile(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mobile = state
        .db
        .get_mobile_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Mobile '{}' not found", id)))?;

    let mobile_name = mobile.vnum.clone();

    state
        .db
        .delete_mobile(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(
        &state.connections,
        &format!("[API] Mobile '{}' deleted by {}", mobile_name, user.api_key.name),
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Mobile '{}' deleted", mobile_name)
    })))
}

/// Add dialogue to a mobile
async fn add_dialogue(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(req): Json<AddDialogueRequest>,
) -> Result<Json<MobileResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut mobile = state
        .db
        .get_mobile_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Mobile '{}' not found", id)))?;

    mobile.dialogue.insert(req.keyword.to_lowercase(), req.response);

    state
        .db
        .save_mobile_data(mobile.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let refreshed = refresh_mobile_instances(&state.db, &mobile);

    Ok(Json(MobileResponse {
        success: true,
        data: mobile,
        refreshed_instances: Some(refreshed),
    }))
}

/// Remove dialogue from a mobile
async fn remove_dialogue(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path((id, keyword)): Path<(String, String)>,
) -> Result<Json<MobileResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut mobile = state
        .db
        .get_mobile_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Mobile '{}' not found", id)))?;

    if mobile.dialogue.remove(&keyword.to_lowercase()).is_none() {
        return Err(ApiError::NotFound(format!("Dialogue keyword '{}' not found", keyword)));
    }

    state
        .db
        .save_mobile_data(mobile.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let refreshed = refresh_mobile_instances(&state.db, &mobile);

    Ok(Json(MobileResponse {
        success: true,
        data: mobile,
        refreshed_instances: Some(refreshed),
    }))
}

/// Add a routine entry to a mobile
async fn add_routine_entry(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(req): Json<RoutineEntryRequest>,
) -> Result<Json<MobileResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut mobile = state
        .db
        .get_mobile_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Mobile '{}' not found", id)))?;

    let entry = RoutineEntry {
        start_hour: req.start_hour,
        activity: parse_activity(&req.activity),
        destination_vnum: req.destination_vnum,
        transition_message: req.transition_message,
        suppress_wander: req.suppress_wander,
        dialogue_overrides: req.dialogue_overrides,
    };

    mobile.daily_routine.push(entry);
    // Sort by start_hour for correct schedule ordering
    mobile.daily_routine.sort_by_key(|e| e.start_hour);

    state
        .db
        .save_mobile_data(mobile.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let refreshed = refresh_mobile_instances(&state.db, &mobile);

    Ok(Json(MobileResponse {
        success: true,
        data: mobile,
        refreshed_instances: Some(refreshed),
    }))
}

/// Remove a routine entry by index
async fn remove_routine_entry(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path((id, index)): Path<(String, usize)>,
) -> Result<Json<MobileResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut mobile = state
        .db
        .get_mobile_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Mobile '{}' not found", id)))?;

    if index >= mobile.daily_routine.len() {
        return Err(ApiError::NotFound(format!("Routine index {} not found", index)));
    }

    mobile.daily_routine.remove(index);

    state
        .db
        .save_mobile_data(mobile.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let refreshed = refresh_mobile_instances(&state.db, &mobile);

    Ok(Json(MobileResponse {
        success: true,
        data: mobile,
        refreshed_instances: Some(refreshed),
    }))
}

/// Add a trigger to a mobile
async fn add_trigger(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(req): Json<AddMobileTriggerRequest>,
) -> Result<Json<MobileResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut mobile = state
        .db
        .get_mobile_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Mobile '{}' not found", id)))?;

    let trigger_type = match req.trigger_type.to_lowercase().as_str() {
        "greet" | "on_greet" => MobileTriggerType::OnGreet,
        "attack" | "on_attack" => MobileTriggerType::OnAttack,
        "death" | "on_death" => MobileTriggerType::OnDeath,
        "say" | "on_say" => MobileTriggerType::OnSay,
        "idle" | "on_idle" => MobileTriggerType::OnIdle,
        "always" | "on_always" => MobileTriggerType::OnAlways,
        "flee" | "on_flee" => MobileTriggerType::OnFlee,
        _ => {
            return Err(ApiError::InvalidInput(format!(
                "Invalid trigger type '{}'. Use: greet, attack, death, say, idle, always, flee",
                req.trigger_type
            )));
        }
    };

    let trigger = MobileTrigger {
        trigger_type,
        script_name: req.script_name,
        enabled: true,
        chance: req.chance,
        interval_secs: req.interval_secs,
        args: req.args,
        last_fired: 0,
    };

    mobile.triggers.push(trigger);

    state
        .db
        .save_mobile_data(mobile.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let refreshed = refresh_mobile_instances(&state.db, &mobile);

    Ok(Json(MobileResponse {
        success: true,
        data: mobile,
        refreshed_instances: Some(refreshed),
    }))
}

/// Remove a trigger from a mobile by index
async fn remove_trigger(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path((id, index)): Path<(String, usize)>,
) -> Result<Json<MobileResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut mobile = state
        .db
        .get_mobile_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Mobile '{}' not found", id)))?;

    if index >= mobile.triggers.len() {
        return Err(ApiError::NotFound(format!("Trigger index {} not found", index)));
    }

    mobile.triggers.remove(index);

    state
        .db
        .save_mobile_data(mobile.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let refreshed = refresh_mobile_instances(&state.db, &mobile);

    Ok(Json(MobileResponse {
        success: true,
        data: mobile,
        refreshed_instances: Some(refreshed),
    }))
}

/// Spawn a mobile instance from a prototype
async fn spawn_mobile(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(vnum): Path<String>,
    Json(req): Json<SpawnMobileRequest>,
) -> Result<Json<MobileResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    // Get the prototype
    let prototype = state
        .db
        .get_mobile_by_vnum(&vnum)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Mobile prototype '{}' not found", vnum)))?;

    if !prototype.is_prototype {
        return Err(ApiError::InvalidInput(format!("Mobile '{}' is not a prototype", vnum)));
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
    instance.current_room_id = Some(room_uuid);
    instance.current_hp = instance.max_hp;

    state
        .db
        .save_mobile_data(instance.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(
        &state.connections,
        &format!("[API] Mobile '{}' spawned in room by {}", vnum, user.api_key.name),
    );

    Ok(Json(MobileResponse {
        success: true,
        data: instance,
        refreshed_instances: None,
    }))
}

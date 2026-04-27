// TypeScript interfaces for IronMUD API entities

export interface Area {
  id: string;
  name: string;
  prefix: string;
  description?: string;
  level_min?: number;
  level_max?: number;
  theme?: string;
  owner?: string;
  trusted_builders: string[];
  permission: "owner_only" | "trusted" | "all_builders";
  flags?: AreaFlags;
  // Migrant immigration config
  immigration_enabled?: boolean;
  immigration_room_vnum?: string;
  immigration_name_pool?: string;
  immigration_visual_profile?: string;
  migration_interval_days?: number;
  migration_max_per_check?: number;
  last_migration_check_day?: number | null;
  immigration_guard_chance?: number;
  migrant_starting_gold?: GoldRange;
  guard_wage_per_hour?: number;
  healer_wage_per_hour?: number;
  scavenger_wage_per_hour?: number;
  // Template RoomFlags copied into every newly-created room in this area.
  default_room_flags?: RoomFlags;
}

export interface GoldRange {
  min: number;
  max: number;
}

export interface AreaFlags {
  always_climate?: boolean;
  combat_zone?: string;
}

export interface Room {
  id: string;
  title: string;
  description: string;
  vnum?: string;
  area_id?: string;
  exits: RoomExits;
  doors: Record<string, DoorState>;
  flags: RoomFlags;
  triggers: RoomTrigger[];
  extra_descs: ExtraDesc[];
  // Migrant housing
  living_capacity?: number;
  residents?: string[];
}

export interface RoomExits {
  north?: string;
  south?: string;
  east?: string;
  west?: string;
  up?: string;
  down?: string;
}

export interface DoorState {
  name: string;
  is_closed: boolean;
  is_locked: boolean;
  key_id?: string;
  keywords: string[];
  description?: string;
}

export interface RoomFlags {
  dark?: boolean;
  no_mob?: boolean;
  indoors?: boolean;
  safe?: boolean;
  private?: boolean;
  death_trap?: boolean;
  no_recall?: boolean;
  shallow_water?: boolean;
  deep_water?: boolean;
  liveable?: boolean;
}

export interface RoomTrigger {
  trigger_type: TriggerType;
  script_name: string;
  enabled: boolean;
  interval_secs?: number;
  chance?: number;
  args: string[];
}

export type TriggerType =
  | "enter"
  | "exit"
  | "look"
  | "periodic"
  | "time"
  | "weather"
  | "season"
  | "month";

export type MobileTriggerType = "greet" | "attack" | "death" | "say" | "idle" | "always" | "flee";

export interface MobileTrigger {
  trigger_type: MobileTriggerType;
  script_name: string;
  enabled: boolean;
  interval_secs?: number;
  chance?: number;
  args: string[];
}

export interface AddMobileTriggerRequest {
  trigger_type: MobileTriggerType;
  script_name: string;
  enabled?: boolean;
  interval_secs?: number;
  chance?: number;
  args?: string[];
}

export type ItemTriggerType =
  | "on_get"
  | "on_drop"
  | "on_use"
  | "on_examine"
  | "on_prompt"
  | "get"
  | "drop"
  | "use"
  | "examine"
  | "prompt";

export interface AddItemTriggerRequest {
  trigger_type: ItemTriggerType;
  script_name: string;
  chance?: number;
  args?: string[];
}

export interface ExtraDesc {
  keywords: string[];
  description: string;
}

export interface Item {
  id: string;
  name: string;
  short_desc: string;
  long_desc: string;
  vnum?: string;
  keywords: string[];
  item_type: ItemType;
  weight: number;
  value: number;
  is_prototype: boolean;
  wear_locations: WearLocation[];
  armor_class?: number;
  damage_dice_count?: number;
  damage_dice_sides?: number;
  damage_type?: DamageType;
  flags: ItemFlags;
}

export type ItemType =
  | "misc"
  | "armor"
  | "weapon"
  | "container"
  | "liquid_container"
  | "food"
  | "key"
  | "gold";

export type WearLocation =
  | "head" | "neck" | "shoulders" | "back" | "torso" | "waist" | "ears"
  | "wielded" | "offhand" | "ready"
  | "leftarm" | "rightarm" | "leftwrist" | "rightwrist"
  | "lefthand" | "righthand" | "leftfinger" | "rightfinger"
  | "leftleg" | "rightleg" | "leftankle" | "rightankle"
  | "leftfoot" | "rightfoot";

export type DamageType =
  | "bludgeoning"
  | "slashing"
  | "piercing"
  | "fire"
  | "cold"
  | "lightning"
  | "poison"
  | "acid";

export interface ItemFlags {
  no_drop?: boolean;
  no_get?: boolean;
  invisible?: boolean;
  glow?: boolean;
  hum?: boolean;
  no_sell?: boolean;
  unique?: boolean;
  plant_pot?: boolean;
  lockpick?: boolean;
  is_skinned?: boolean;
  boat?: boolean;
  medical_tool?: boolean;
}

export interface Mobile {
  id: string;
  name: string;
  short_desc: string;
  long_desc: string;
  vnum: string;
  keywords: string[];
  level: number;
  max_hp: number;
  current_hp: number;
  damage_dice: string;
  armor_class: number;
  perception: number;
  is_prototype: boolean;
  current_room_id?: string;
  flags: MobileFlags;
  dialogue: Record<string, string>;
  triggers: MobileTrigger[];
  simulation?: SimulationConfig;
  needs?: NeedsState;
}

export interface SimulationConfig {
  home_room_vnum: string;
  work_room_vnum: string;
  shop_room_vnum: string;
  preferred_food_vnum: string;
  work_pay: number;
  work_start_hour: number;
  work_end_hour: number;
  hunger_decay_rate: number;
  energy_decay_rate: number;
  comfort_decay_rate: number;
  low_gold_threshold?: number;
}

export interface NeedsState {
  hunger: number;
  energy: number;
  comfort: number;
  current_goal: string;
  paid_this_shift: boolean;
  last_tick_hour: number;
}

export interface MobileFlags {
  aggressive?: boolean;
  sentinel?: boolean;
  scavenger?: boolean;
  shopkeeper?: boolean;
  healer?: boolean;
  no_attack?: boolean;
  cowardly?: boolean;
  can_open_doors?: boolean;
  leasing_agent?: boolean;
  guard?: boolean;
  thief?: boolean;
  cant_swim?: boolean;
  poisonous?: boolean;
  fiery?: boolean;
  chilling?: boolean;
  corrosive?: boolean;
  shocking?: boolean;
}

export interface SpawnPoint {
  id: string;
  area_id: string;
  room_id: string;
  entity_type: "mobile" | "item";
  vnum: string;
  max_count: number;
  respawn_interval_secs: number;
  enabled: boolean;
  last_spawn_time: number;
  spawned_entities: string[];
  dependencies: SpawnDependency[];
}

export interface SpawnDependency {
  item_vnum: string;
  destination: "inventory" | "equipped" | "container";
  wear_location?: WearLocation;
  count: number;
}

// API Response types
export interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: ApiError;
  refreshed_instances?: number;
}

export interface ApiError {
  code: string;
  message: string;
}

export interface ListResponse<T> {
  success: boolean;
  data: T[];
  total?: number;
}

// Request types for creating entities
export interface CreateAreaRequest {
  name: string;
  prefix: string;
  description?: string;
  level_min?: number;
  level_max?: number;
  theme?: string;
}

export interface UpdateAreaRequest {
  name?: string;
  description?: string;
  level_min?: number;
  level_max?: number;
  theme?: string;
  permission_level?: "owner_only" | "trusted" | "all_builders";
  trusted_builders?: string[];
  immigration_enabled?: boolean;
  immigration_room_vnum?: string;
  immigration_name_pool?: string;
  immigration_visual_profile?: string;
  migration_interval_days?: number;
  migration_max_per_check?: number;
  immigration_guard_chance?: number;
  migrant_starting_gold?: GoldRange;
  guard_wage_per_hour?: number;
  healer_wage_per_hour?: number;
  scavenger_wage_per_hour?: number;
  default_room_flags?: RoomFlags;
}

export interface CreateRoomRequest {
  title: string;
  description: string;
  area_id?: string;
  vnum?: string;
  flags?: RoomFlags;
}

export interface CreateItemRequest {
  name: string;
  short_desc: string;
  long_desc: string;
  vnum: string;
  keywords?: string[];
  item_type: string;
  weight?: number;
  value?: number;
  categories?: string[];
  wear_location?: string;
  damage_dice_count?: number;
  damage_dice_sides?: number;
  damage_type?: string;
  armor_class?: number;
  flags?: Partial<ItemFlags>;
  // Firearm fields
  caliber?: string;
  ranged_type?: string;
  magazine_size?: number;
  fire_mode?: string;
  supported_fire_modes?: string[];
  noise_level?: string;
  two_handed?: boolean;
  weapon_skill?: string;
  // Ammo fields
  ammo_count?: number;
  ammo_damage_bonus?: number;
  // Attachment fields
  attachment_slot?: string;
  attachment_accuracy_bonus?: number;
  attachment_noise_reduction?: number;
  attachment_magazine_bonus?: number;
  // Gardening fields
  plant_prototype_vnum?: string;
  fertilizer_duration?: number;
  treats_infestation?: string;
  // Liquid container fields
  liquid_type?: string;
  liquid_current?: number;
  liquid_max?: number;
  liquid_effects?: { effect_type: string; magnitude: number; duration: number }[];
  // Medical fields
  medical_tier?: number;
  medical_uses?: number;
  treats_wound_types?: string[];
  // Food fields
  food_nutrition?: number;
  food_spoil_duration?: number;
  food_effects?: { effect_type: string; magnitude: number; duration: number }[];
  // Readable body (any item with this becomes readable via `read`)
  note_content?: string;
}

export interface CreateMobileRequest {
  name: string;
  short_desc: string;
  long_desc: string;
  vnum: string;
  keywords?: string[];
  level?: number;
  max_hp?: number;
  damage_dice?: string;
  armor_class?: number;
  perception?: number;
  flags?: Partial<MobileFlags>;
  // Healer config
  healer_type?: string;
  healing_free?: boolean;
  healing_cost_multiplier?: number;
  // Shop config
  shop_sell_rate?: number;
  shop_buy_rate?: number;
  shop_buys_types?: string[];
  shop_buys_categories?: string[];
  shop_min_value?: number;
  shop_max_value?: number;
  shop_extra_types?: string[];
  shop_extra_categories?: string[];
  shop_deny_types?: string[];
  shop_deny_categories?: string[];
  shop_stock?: string[];
  shop_preset_vnum?: string;
  // Daily routine
  daily_routine?: RoutineEntry[];
  // Needs simulation
  simulation?: Partial<SimulationConfig> & { home_room_vnum: string; work_room_vnum: string };
  remove_simulation?: boolean;
}

export interface RoutineEntry {
  start_hour: number;
  activity: string;
  destination_vnum?: string;
  transition_message?: string;
  suppress_wander?: boolean;
  dialogue_overrides?: Record<string, string>;
}

export interface CreateSpawnPointRequest {
  area_id: string;
  room_id: string;
  entity_type: "mobile" | "item";
  vnum: string;
  max_count?: number;
  respawn_interval_secs?: number;
  enabled?: boolean;
}

export interface SetExitRequest {
  target_room_id: string;
}

export interface AddDoorRequest {
  name: string;
  is_closed?: boolean;
  is_locked?: boolean;
  key_id?: string;
  keywords?: string[];
  description?: string;
}

export interface AddTriggerRequest {
  trigger_type: TriggerType;
  script_name: string;
  enabled?: boolean;
  interval_secs?: number;
  chance?: number;
  args?: string[];
}

export interface AddExtraDescRequest {
  keywords: string[];
  description: string;
}

export interface AddDialogueRequest {
  keyword: string;
  response: string;
}

export interface AddSpawnDependencyRequest {
  item_vnum: string;
  destination: string;
  wear_location?: string;
  count?: number;
}

export interface SpawnEntityRequest {
  room_id: string;
}

// Transport types

export interface TransportStop {
  room_id: string;
  name: string;
  exit_direction: string;
}

export type TransportType = "elevator" | "bus" | "train" | "ferry" | "airship";

export type TransportSchedule =
  | { on_demand: null }
  | {
      game_time: {
        frequency_hours: number;
        operating_start: number;
        operating_end: number;
        dwell_time_secs: number;
      };
    };

export interface Transport {
  id: string;
  vnum: string | null;
  name: string;
  transport_type: TransportType;
  interior_room_id: string;
  stops: TransportStop[];
  current_stop_index: number;
  state: "stopped" | "moving";
  direction: number;
  schedule: TransportSchedule;
  travel_time_secs: number;
  last_state_change: number;
}

export interface CreateTransportRequest {
  name: string;
  vnum?: string;
  transport_type: string;
  interior_room_id: string;
  travel_time_secs?: number;
  schedule_type?: string;
  frequency_hours?: number;
  operating_start?: number;
  operating_end?: number;
  dwell_time_secs?: number;
}

export interface AddTransportStopRequest {
  room_id: string;
  name: string;
  exit_direction: string;
}

export interface ConnectTransportRequest {
  stop_index: number;
}

export interface TravelTransportRequest {
  destination_index: number;
}

// Plant prototype types

export interface GrowthStageDef {
  stage: string;
  duration_game_hours: number;
  description: string;
  examine_desc: string;
}

export type PlantCategory = "vegetable" | "herb" | "flower" | "fruit" | "grain";

export interface PlantPrototype {
  id: string;
  vnum?: string;
  name: string;
  keywords: string[];
  seed_vnum: string;
  harvest_vnum: string;
  harvest_min: number;
  harvest_max: number;
  category: PlantCategory;
  stages: GrowthStageDef[];
  preferred_seasons: string[];
  forbidden_seasons: string[];
  water_consumption_per_hour: number;
  water_capacity: number;
  indoor_only: boolean;
  min_skill_to_plant: number;
  base_xp: number;
  pest_resistance: number;
  multi_harvest: boolean;
  is_prototype: boolean;
}

// Recipes (crafting / cooking)

export interface RecipeIngredient {
  vnum?: string | null;
  category?: string | null;
  quantity: number;
}

export interface RecipeTool {
  vnum?: string | null;
  category?: string | null;
  /** "Inventory" | "Room" | "Either" — sled serializes the enum variant name */
  location: string;
}

export interface Recipe {
  /** Recipe vnum (e.g. "smith:iron_sword"); canonical id. */
  id: string;
  name: string;
  /** "crafting" | "cooking" */
  skill: string;
  skill_required: number;
  auto_learn: boolean;
  ingredients: RecipeIngredient[];
  tools: RecipeTool[];
  output_vnum: string;
  output_quantity: number;
  base_xp: number;
  difficulty: number;
}

export interface RecipeIngredientRequest {
  vnum?: string;
  category?: string;
  quantity?: number;
}

export interface RecipeToolRequest {
  vnum?: string;
  category?: string;
  /** "inv" | "inventory" | "room" | "either" — default "inventory" */
  location?: string;
}

export interface CreateRecipeRequest {
  vnum: string;
  name: string;
  skill: string;
  skill_required?: number;
  auto_learn?: boolean;
  ingredients?: RecipeIngredientRequest[];
  tools?: RecipeToolRequest[];
  output_vnum: string;
  output_quantity?: number;
  base_xp?: number;
  difficulty?: number;
}

export interface UpdateRecipeRequest {
  name?: string;
  skill?: string;
  skill_required?: number;
  auto_learn?: boolean;
  ingredients?: RecipeIngredientRequest[];
  tools?: RecipeToolRequest[];
  output_vnum?: string;
  output_quantity?: number;
  base_xp?: number;
  difficulty?: number;
}

// Forage tables (per-area)

export type ForageType =
  | "city"
  | "wilderness"
  | "shallow_water"
  | "deep_water"
  | "underwater";

export interface ForageEntry {
  vnum: string;
  min_skill: number;
  /** "common" | "uncommon" | "rare" | "legendary" */
  rarity: string;
}

export interface ForageTables {
  city: ForageEntry[];
  wilderness: ForageEntry[];
  shallow_water: ForageEntry[];
  deep_water: ForageEntry[];
  underwater: ForageEntry[];
}

export interface AddForageEntryRequest {
  forage_type: ForageType;
  vnum: string;
  min_skill?: number;
  rarity: string;
}

export interface CreatePlantPrototypeRequest {
  name: string;
  vnum: string;
  keywords?: string[];
  seed_vnum?: string;
  harvest_vnum?: string;
  harvest_min?: number;
  harvest_max?: number;
  category?: string;
  stages?: GrowthStageDef[];
  preferred_seasons?: string[];
  forbidden_seasons?: string[];
  water_consumption_per_hour?: number;
  water_capacity?: number;
  indoor_only?: boolean;
  min_skill_to_plant?: number;
  base_xp?: number;
  pest_resistance?: number;
  multi_harvest?: boolean;
}

// Summary types for compact listing (reduced context window usage)

export interface ItemSummary {
  vnum: string | null;
  name: string;
  item_type: ItemType;
  weight: number;
  value: number;
  wear_location: string | null;
  weapon_skill: string | null;
  damage: string | null;
  armor_class: number | null;
}

export interface RoomSummary {
  vnum: string | null;
  title: string;
  exits: string[];
  flags: string[];
  has_doors: string[];
  trigger_count: number;
  extra_desc_count: number;
}

export interface MobileSummary {
  vnum: string;
  name: string;
  level: number;
  max_hp: number;
  armor_class: number;
  damage_dice: string;
  flags: string[];
  has_dialogue: boolean;
  has_routine: boolean;
  trigger_count: number;
}

export interface SpawnPointSummary {
  entity_type: string;
  vnum: string;
  room_vnum: string | null;
  max_count: number;
  enabled: boolean;
}

export interface AreaOverview {
  area: Area;
  rooms: RoomSummary[];
  item_prototypes: ItemSummary[];
  mobile_prototypes: MobileSummary[];
  spawn_points: SpawnPointSummary[];
}

// Description context types for AI-assisted description generation

export interface ConnectedRoom {
  direction: string;
  room_id: string;
  title: string;
  has_door: boolean;
  door_name?: string;
}

export interface RoomContext {
  room: {
    id: string;
    title: string;
    current_description: string;
    vnum?: string;
    flags: RoomFlags;
  };
  area?: {
    name: string;
    theme?: string;
    level_min?: number;
    level_max?: number;
  };
  connected_rooms: ConnectedRoom[];
  suggested_elements: string[];
  style_guide: string;
}

export interface ItemContext {
  item: {
    id: string;
    name: string;
    item_type: ItemType;
    current_short_desc: string;
    current_long_desc: string;
    flags: ItemFlags;
    weight: number;
    value: number;
    wear_locations: WearLocation[];
    damage_dice_count?: number;
    damage_dice_sides?: number;
    damage_type?: DamageType;
    armor_class?: number;
  };
  type_guidance: string;
  flag_elements: string[];
  style_guide: string;
}

export interface MobileContext {
  mobile: {
    id: string;
    name: string;
    level: number;
    current_short_desc: string;
    current_long_desc: string;
    flags: MobileFlags;
    dialogue_keywords: string[];
  };
  role: string;
  behavior_hints: string[];
  area?: {
    name: string;
    theme?: string;
  };
  style_guide: string;
}

export interface DescriptionExample {
  vnum?: string;
  name: string;
  description: string;
  short_desc?: string;
  long_desc?: string;
  flags: Record<string, boolean>;
}

export interface DescriptionExampleFilter {
  area_prefix?: string;
  item_type?: ItemType;
  has_flag?: string;
  min_length?: number;
  max_length?: number;
}

// Bug Reporting System

export type BugStatus = "Open" | "InProgress" | "Resolved" | "Closed";
export type BugPriority = "Low" | "Normal" | "High" | "Critical";

export interface BugContext {
  room_id: string;
  room_vnum: string;
  room_title: string;
  character_level: number;
  character_class: string;
  character_race: string;
  character_position: string;
  hp: number;
  max_hp: number;
  mana: number;
  max_mana: number;
  in_combat: boolean;
  game_time: string;
  season: string;
  weather: string;
  players_in_room: string[];
  mobiles_in_room: string[];
}

export interface AdminNote {
  author: string;
  message: string;
  created_at: number;
}

export interface BugReport {
  id: string;
  ticket_number: number;
  reporter: string;
  description: string;
  status: BugStatus;
  priority: BugPriority;
  approved: boolean;
  created_at: number;
  updated_at: number;
  resolved_at: number | null;
  resolved_by: string | null;
  admin_notes: AdminNote[];
  context: BugContext;
}

export interface UpdateBugReportRequest {
  status?: string;
  priority?: string;
}

export interface AddBugNoteRequest {
  author: string;
  message: string;
}

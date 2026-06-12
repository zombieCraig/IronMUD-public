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
  donation_room_vnum?: string | null;
  starting_room_vnum?: string | null;
  immigration_name_pool?: string;
  immigration_visual_profile?: string;
  migration_interval_days?: number;
  migration_max_per_check?: number;
  last_migration_check_day?: number | null;
  immigration_variation_chances?: ImmigrationVariationChances;
  migrant_starting_gold?: GoldRange;
  guard_wage_per_hour?: number;
  healer_wage_per_hour?: number;
  scavenger_wage_per_hour?: number;
  // Template RoomFlags copied into every newly-created room in this area.
  default_room_flags?: RoomFlags;
  // Per-area climate preset that filters globally-rolled weather and shifts
  // effective temperature. Defaults to "temperate" (no filtering).
  climate?: ClimateProfile;
  // Combat zone type — PvE/Safe/PvP. Defaults to "pve". Rooms inherit unless
  // overridden at the room level.
  combat_zone?: CombatZoneType;
}

export type ClimateProfile = "temperate" | "tropical" | "arid" | "tundra" | "subarctic";

export type CombatZoneType = "pve" | "safe" | "pvp";

export interface GoldRange {
  min: number;
  max: number;
}

export interface ImmigrationVariationChances {
  guard?: number;
  healer?: number;
  scavenger?: number;
  vampire?: number;
}

export interface AreaFlags {
  always_climate?: boolean;
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
  /** The Rot contamination level: 0 clean, 1 weak, 2 heavy, 3 hotspot. */
  rot_level?: number;
  /** Builder-declared verbs the room exposes (TAB completion + look hints). */
  contextual_commands?: ContextualCommand[];
  /** Conditional entry gate. Absent = no gate (anyone may enter). */
  entry_gate?: RoomEntryGate;
}

/** All conditions in `conditions` must pass for a character to enter. */
export interface RoomEntryGate {
  conditions: RoomEntryCondition[];
  /** Shown to a blocked entrant. Empty -> "You cannot pass that way." */
  block_message?: string;
}

export type RoomEntryCondition =
  | { kind: "class_is"; name: string }
  | { kind: "has_skill"; name: string; min_level: number }
  | { kind: "has_item"; vnum: string }
  | { kind: "has_tattoo"; keyword: string }
  | { kind: "dg_var_set"; key: string }
  | { kind: "dg_var_equals"; key: string; value: string };

export interface ContextualCommand {
  /** Single keyword, lowercased. Pair with a DG OnCommand trigger to wire behavior. */
  verb: string;
  /** Short flavor displayed alongside the verb in `look`. */
  hint?: string;
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
  key_vnum?: string;
  keywords: string[];
  description?: string;
  pickproof?: boolean;
}

export interface RoomFlags {
  dark?: boolean;
  no_mob?: boolean;
  indoors?: boolean;
  safe?: boolean;
  private?: boolean;
  private_room?: boolean;
  tunnel?: boolean;
  death?: boolean;
  no_magic?: boolean;
  death_trap?: boolean;
  no_recall?: boolean;
  shallow_water?: boolean;
  deep_water?: boolean;
  liveable?: boolean;
  soundproof?: boolean;
  notrack?: boolean;
  baseline_office?: boolean;
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

// ===== Dialogue Trees =====

export type FlagScope = "local" | "global";
export type DgScope = "player" | "mob";

export type DialogueTarget =
  | { kind: "goto"; node: string }
  | { kind: "exit" }
  | { kind: "repeat" };

export type DialogueCondition =
  | { kind: "flag_set"; name: string; scope?: FlagScope }
  | { kind: "flag_unset"; name: string; scope?: FlagScope }
  | { kind: "has_item"; vnum: string; qty?: number }
  | { kind: "skill_at_least"; key: string; level: number }
  | { kind: "counter_at_least"; key: string; value: number }
  | { kind: "dg_var_equals"; scope: DgScope; key: string; value: string }
  /** True for embraced vampires who carry no clan trait yet. */
  | { kind: "is_thinblood" }
  /** True for embraced vampires who carry any clan_* trait. */
  | { kind: "is_clan_acknowledged" }
  /** True when the speaker has the named achievement unlocked. */
  | { kind: "has_achievement"; key: string }
  /**
   * True when the player's `ActiveQuest.choice_vars[key]` for `quest_vnum`
   * equals `value`. Pairs with `SetQuestChoice` to gate follow-up
   * branches on a prior in-tree decision.
   */
  | { kind: "quest_choice_equals"; quest_vnum: string; key: string; value: string };

export type DialogueEffect =
  | { kind: "set_flag"; name: string; scope?: FlagScope }
  | { kind: "clear_flag"; name: string; scope?: FlagScope }
  | { kind: "give_item"; vnum: string; qty?: number }
  | { kind: "take_item"; vnum: string; qty?: number }
  | { kind: "award_skill_xp"; skill: string; amount: number }
  | { kind: "set_counter"; key: string; value: number }
  | { kind: "increment_counter"; key: string; by?: number }
  | { kind: "set_dg_var"; scope: DgScope; key: string; value: string }
  | { kind: "fire_dg_trigger"; trigger_type: string; arg?: string }
  /**
   * Record a per-quest runtime choice on the player's active quest. No-op
   * (with a warn-log) if the quest isn't active — author the tree to
   * `OfferQuest` first, then `SetQuestChoice`. Consumed by reward
   * variants like `embrace_anarch` whose payload depends on player input.
   */
  | { kind: "set_quest_choice"; quest_vnum: string; key: string; value: string };

export interface DialogueChoice {
  keyword: string;
  label: string;
  target: DialogueTarget;
  conditions?: DialogueCondition[];
  effects?: DialogueEffect[];
  hint?: string;
  cooldown_secs?: number;
  once_per_player?: boolean;
}

export interface DialogueNode {
  text: string;
  choices?: DialogueChoice[];
  on_enter?: DialogueEffect[];
  on_each_visit?: DialogueEffect[];
  on_exit?: DialogueEffect[];
}

export interface DialogueTree {
  root_node: string;
  nodes: Record<string, DialogueNode>;
}

export interface AddDialogueNodeRequest {
  name: string;
  text: string;
  on_enter?: DialogueEffect[];
  on_each_visit?: DialogueEffect[];
  on_exit?: DialogueEffect[];
}

export interface UpdateDialogueNodeRequest {
  text?: string;
  on_enter?: DialogueEffect[];
  on_each_visit?: DialogueEffect[];
  on_exit?: DialogueEffect[];
}

export interface DialogueChoiceRequest {
  keyword: string;
  label: string;
  target: DialogueTarget;
  conditions?: DialogueCondition[];
  effects?: DialogueEffect[];
  hint?: string;
  cooldown_secs?: number;
  once_per_player?: boolean;
}

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
  | "on_look"
  | "on_prompt"
  | "get"
  | "drop"
  | "use"
  | "examine"
  | "look"
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

export interface ItemAffect {
  /** Snake_case EffectType name (strength_boost, hit_bonus, damage_resistance, status_resistance, night_vision, poison, ...). */
  effect_type: string;
  /** Effect magnitude (default 1). Percent for resistances; flat bonus otherwise. */
  magnitude?: number;
  /** Required iff effect_type === 'damage_resistance'. */
  damage_type?: string;
  /** Required iff effect_type === 'status_resistance'. Snake_case effect name being warded, or '*' for all status effects. */
  vs_effect?: string;
}

export interface Item {
  id: string;
  name: string;
  short_desc: string;
  long_desc: string;
  vnum?: string;
  /** Owning area for sandbox / permission checks. None = orphan. */
  area_id?: string;
  keywords: string[];
  item_type: ItemType;
  weight: number;
  value: number;
  is_prototype: boolean;
  wear_locations: WearLocation[];
  armor_class?: number;
  affects?: ItemAffect[];
  light_hours_remaining?: number;
  cast_on_use?: CastOnUse;
  damage_dice_count?: number;
  damage_dice_sides?: number;
  damage_type?: DamageType;
  flags: ItemFlags;
  extra_descs?: ExtraDesc[];
  on_hit_effects?: OnHitEffect[];
}

export interface OnHitEffect {
  effect: string;
  chance: number;
  magnitude: number;
  duration: number;
}

export interface CastOnUse {
  spell: string;
  min_level?: number;
  charges?: number;
  max_charges?: number;
  cooldown_secs?: number;
}

export type ItemType =
  | "misc"
  | "armor"
  | "weapon"
  | "container"
  | "liquid_container"
  | "food"
  | "key"
  | "gold"
  | "ammunition"
  | "potion"
  | "wand"
  | "staff"
  | "note"
  | "pen"
  | "tool"
  | "tattoo";

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
  magical?: boolean;
  night_vision?: boolean;
  no_sell?: boolean;
  no_donate?: boolean;
  unique?: boolean;
  plant_pot?: boolean;
  lockpick?: boolean;
  is_skinned?: boolean;
  boat?: boolean;
  medical_tool?: boolean;
  buried?: boolean;
  can_dig?: boolean;
  detect_buried?: boolean;
  anti_good?: boolean;
  anti_evil?: boolean;
  anti_neutral?: boolean;
}

export interface Mobile {
  id: string;
  name: string;
  short_desc: string;
  long_desc: string;
  vnum: string;
  /** Owning area for sandbox / permission checks. None = orphan. */
  area_id?: string;
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
  faction?: string;
  spoken_language?: string;
  dialogue: Record<string, string>;
  triggers: MobileTrigger[];
  combat_spells?: string[];
  combat_spell_chance?: number;
  simulation?: SimulationConfig;
  needs?: NeedsState;
  position?: "standing" | "sitting" | "sleeping";
  creature_type?: "mortal" | "animal" | "insect" | "plant" | "construct" | "spirit";
  pet_owner?: string;
  gender?: string;
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
  helper?: boolean;
  thief?: boolean;
  cant_swim?: boolean;
  poisonous?: boolean;
  fiery?: boolean;
  chilling?: boolean;
  corrosive?: boolean;
  shocking?: boolean;
  unique?: boolean;
  stay_zone?: boolean;
  aware?: boolean;
  memory?: boolean;
  no_sleep?: boolean;
  no_blind?: boolean;
  no_bash?: boolean;
  no_summon?: boolean;
  no_charm?: boolean;
  hostile_on_steal?: boolean;
  tameable?: boolean;
  aggro_good?: boolean;
  aggro_evil?: boolean;
  aggro_neutral?: boolean;
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
  bury_on_spawn?: boolean;
  replace_on_respawn?: boolean;
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
  climate?: ClimateProfile;
  combat_zone?: CombatZoneType;
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
  immigration_healer_chance?: number;
  immigration_scavenger_chance?: number;
  immigration_vampire_chance?: number;
  migrant_starting_gold?: GoldRange;
  donation_room_vnum?: string | null;
  starting_room_vnum?: string | null;
  guard_wage_per_hour?: number;
  healer_wage_per_hour?: number;
  scavenger_wage_per_hour?: number;
  /** Soft cap on rooms attributed to this area. 0 / null = unlimited. */
  max_rooms?: number;
  max_items?: number;
  max_mobiles?: number;
  max_spawn_points?: number;
  default_room_flags?: RoomFlags;
  climate?: ClimateProfile;
  combat_zone?: CombatZoneType;
}

export interface CreateRoomRequest {
  title: string;
  description: string;
  area_id?: string;
  vnum?: string;
  flags?: RoomFlags;
  contextual_commands?: ContextualCommand[];
  entry_gate?: RoomEntryGate;
  /** Update-only: when true, removes the room's entry gate entirely. */
  clear_entry_gate?: boolean;
  /** The Rot contamination level (0 clean .. 3 hotspot); clamped server-side. */
  rot_level?: number;
}

export interface CreateItemRequest {
  name: string;
  short_desc: string;
  long_desc: string;
  vnum: string;
  /** Owning area UUID (or omit/empty for orphan). */
  area_id?: string;
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
  affects?: ItemAffect[];
  light_hours_remaining?: number;
  cast_on_use?: CastOnUse;
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
  // ItemType=board: gate `board list`/`read` to admins only.
  board_read_admin_only?: boolean;
  // ItemType=board: gate `board write` to admins only.
  board_write_admin_only?: boolean;
  // ItemType=board: post cap (0 = engine default 60).
  board_max_messages?: number;
  // World-wide cap on live (non-prototype) instances of this vnum (0 = unlimited)
  world_max_count?: number;
  // Cyberware fields (item_type "cyberware"; see docs/design/cyberware-system.md)
  cyber_category?: string;
  cyber_foundation?: boolean;
  cyber_option_slots?: number;
  cyber_slot_cost?: number;
  cyber_humanity_loss?: number;
  cyber_paired?: boolean;
  cyber_exclusive_tag?: string;
  // Sub-keyword lore revealed via `look <keyword>` against this item
  extra_descs?: ExtraDesc[];
  // Per-hit effects rolled when this weapon lands a hit
  on_hit_effects?: OnHitEffect[];
}

export interface CreateMobileRequest {
  name: string;
  short_desc: string;
  long_desc: string;
  vnum: string;
  /** Owning area UUID (or omit/empty for orphan). */
  area_id?: string;
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
  // World-wide cap on live (non-prototype) instances of this vnum (0 = unlimited)
  world_max_count?: number;
  // Branching dialogue tree (overlay; falls back to flat keyword `dialogue` map on miss).
  dialogue_tree?: DialogueTree;
  // Pass true to remove the dialogue tree. Takes precedence over `dialogue_tree`.
  clear_dialogue_tree?: boolean;
  // Helper-system ally tag. Empty = Circle-stock fallback.
  faction?: string;
  // Language the mob speaks. Empty = lingua franca / Common (no garble).
  spoken_language?: string;
  // Spell IDs the mob may cast in combat (CircleMUD `magic_user` analog)
  combat_spells?: string[];
  // Per-round percent chance to cast (0-100). Default 50.
  combat_spell_chance?: number;
  // Per-hit effects rolled on every landed natural attack
  on_hit_effects?: OnHitEffect[];
  // Default physical stance (standing | sitting | sleeping)
  position?: "standing" | "sitting" | "sleeping";
  // Base biology; drives vampire feeding. Independent of undead/vampire flags.
  creature_type?: "mortal" | "animal" | "insect" | "plant" | "construct" | "spirit";
  // Authored gender for DG pronouns. "male" | "female" | "nonbinary" | any
  // free string. Lazy-instantiates Characteristics; unrecognised values
  // resolve as neuter pronouns in DG Scripts.
  gender?: string;
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
  bury_on_spawn?: boolean;
  replace_on_respawn?: boolean;
}

export interface SetExitRequest {
  target_room_id: string;
}

export interface AddDoorRequest {
  name: string;
  is_closed?: boolean;
  is_locked?: boolean;
  key_vnum?: string;
  keywords?: string[];
  description?: string;
  pickproof?: boolean;
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

// Quests

export type QuestObjective =
  | { kind: "kill_mob"; vnum: string; count: number }
  /** Kill any of the listed prototype vnums until the shared counter hits `count`. */
  | { kind: "kill_any_mob"; vnums: string[]; count: number }
  | { kind: "bring_item"; vnum: string; qty: number; return_to_mob_vnum?: string | null }
  | { kind: "visit_room"; vnum: string }
  | { kind: "dg_flag"; var: string; value: string };

/**
 * Set-count achievement gate. Quest is offerable only when at least
 * `min_count` keys in `keys` are unlocked in the player's
 * `achievements_unlocked` map.
 */
export interface AchievementSetPrereq {
  keys: string[];
  min_count: number;
}

export type QuestReward =
  | { kind: "gold"; amount: number }
  | { kind: "item"; vnum: string; qty: number }
  | { kind: "skill_xp"; skill: string; amount: number }
  | { kind: "achievement"; key: string }
  | { kind: "learn_recipe"; recipe_id: string }
  /**
   * Grants the named clan to a thinblood vampire on quest completion.
   * Sire defaults to the quest's `giver_mob_vnum` prototype name. No-op
   * for mortals or already-acknowledged kindred. Use lowercase clan ids:
   * brujah, toreador, ventrue, nosferatu, gangrel.
   */
  | { kind: "embrace_clan"; clan: string }
  /**
   * Anarch-path uplift on quest completion. Lifts the thinblood gates
   * without claiming a clan: stamps the `anarch_unbound` trait, sets sire
   * to the sentinel "Anarch Unbound", and seeds 1 dot of the chosen
   * discipline. When `discipline` is omitted, the reward reads the
   * player's runtime choice from `ActiveQuest.choice_vars["discipline"]`
   * (set earlier in the dialogue tree via `SetQuestChoice`).
   */
  | { kind: "embrace_anarch"; discipline?: string };

export interface Quest {
  /** Quest vnum (e.g. "qst:100"); canonical id. */
  vnum: string;
  name: string;
  keywords: string[];
  summary: string;
  description: string;
  completion_text: string;
  objectives: QuestObjective[];
  rewards: QuestReward[];
  repeatable: boolean;
  giver_mob_vnum?: string | null;
  prereq_quest_vnum?: string | null;
  min_player_skill_total?: number | null;
  duration_secs?: number | null;
  achievement_set_prereq?: AchievementSetPrereq | null;
}

export interface CreateQuestRequest {
  vnum: string;
  name: string;
  keywords?: string[];
  summary?: string;
  description?: string;
  completion_text?: string;
  objectives?: QuestObjective[];
  rewards?: QuestReward[];
  repeatable?: boolean;
  giver_mob_vnum?: string;
  prereq_quest_vnum?: string;
  min_player_skill_total?: number;
  duration_secs?: number;
  achievement_set_prereq?: AchievementSetPrereq;
}

export interface UpdateQuestRequest {
  name?: string;
  keywords?: string[];
  summary?: string;
  description?: string;
  completion_text?: string;
  objectives?: QuestObjective[];
  rewards?: QuestReward[];
  repeatable?: boolean;
  giver_mob_vnum?: string;
  prereq_quest_vnum?: string;
  min_player_skill_total?: number;
  duration_secs?: number;
  /** Pass `{keys: [], min_count: 0}` (or any object with empty keys / non-positive min_count) to clear. */
  achievement_set_prereq?: AchievementSetPrereq;
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

export interface BuilderDebugResponse {
  success: boolean;
  data: string[];
}

// Achievements

export type AchievementCategory =
  | "skill"
  | "combat"
  | "crafting"
  | "exploration"
  | "social"
  | "wealth"
  | "builder";

export type AchievementCriterion =
  | { kind: "counter"; counter: string; threshold: number }
  | { kind: "skill_reached"; skill: string; level: number }
  | { kind: "recipe_learned"; recipe_key: string }
  | { kind: "owned_lease"; area_vnum?: string | null }
  | { kind: "gold_held"; amount: number }
  | { kind: "manual" };

export interface AchievementReward {
  title: string;
  item_vnum?: string | null;
  gold?: number | null;
  /** Morality shift applied at unlock. +good / -evil. Clamped into [-200, 200]. */
  morality_delta?: number;
}

export type AchievementSource =
  | { kind: "json"; file: string }
  | { kind: "db"; author: string };

export interface Achievement {
  key: string;
  name: string;
  description: string;
  category: AchievementCategory;
  criterion: AchievementCriterion;
  reward: AchievementReward;
  hidden: boolean;
  source: AchievementSource;
}

export interface AchievementSummary {
  key: string;
  name: string;
  category: AchievementCategory;
  source: AchievementSource;
  hidden: boolean;
}

export interface CreateAchievementRequest {
  key: string;
  name: string;
  category?: AchievementCategory;
}

export interface UpdateAchievementRequest {
  name?: string;
  description?: string;
  category?: AchievementCategory;
  criterion?: AchievementCriterion;
  reward?: AchievementReward;
  hidden?: boolean;
}

// === DG Scripts trigger prototypes ===

export type DgAttachKind = "mob" | "obj" | "room";

export interface DgProtoSummary {
  vnum: string;
  name: string;
  kind: DgAttachKind;
  flags: string;
}

export interface DgProto {
  vnum: string;
  name: string;
  kind: DgAttachKind;
  flags: string;
  numeric_arg: number;
  arglist: string;
  body: string;
}

export interface CreateDgProtoRequest {
  vnum: string;
  name: string;
  kind: DgAttachKind;
  flags: string;
  body?: string;
  numeric_arg?: number;
  arglist?: string;
}

export interface UpdateDgProtoRequest {
  name?: string;
  kind?: DgAttachKind;
  flags?: string;
  body?: string;
  numeric_arg?: number;
  arglist?: string;
}

/** Wrapped response shape returned by the proto save/get endpoints. */
export interface DgProtoResponse {
  success: boolean;
  data: DgProto;
  /** Non-fatal analyzer warnings — present on create/update; absent on read. */
  warnings?: string[];
  /** Live instances refreshed after a body save. */
  refreshed_instances?: number;
}

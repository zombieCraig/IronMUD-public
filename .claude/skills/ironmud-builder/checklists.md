# Building Checklists

## New Area Checklist

### Phase 1: Setup
- [ ] Decide on area theme and level range
- [ ] Choose area prefix (short, lowercase, no spaces)
- [ ] Create area with name, description, level_min, level_max
- [ ] Note the area UUID for later use

### Phase 2: Rooms
- [ ] Plan room layout on paper/diagram first
- [ ] Create all rooms with vnums following `prefix:name` pattern
- [ ] Write unique title for each room
- [ ] Write atmospheric description for each room
- [ ] Set appropriate flags (dark, indoors, safe, etc.)
- [ ] Connect all exits bidirectionally
- [ ] Add doors where needed
- [ ] Add extra descriptions for examinable objects

### Phase 3: Items
- [ ] List all items needed (weapons, armor, keys, loot)
- [ ] Create item prototypes with unique vnums
- [ ] Set appropriate item_type for each
- [ ] Set wear_location for equipment
- [ ] Set damage stats for weapons
- [ ] Set armor_class for armor
- [ ] Set weight and value appropriately
- [ ] Add keywords for targeting

### Phase 4: Mobiles
- [ ] List all NPCs/monsters needed
- [ ] Create mobile prototypes with unique vnums
- [ ] Set appropriate flags (aggressive, sentinel, shopkeeper)
- [ ] Set level and stats appropriate to area
- [ ] Set damage_dice for combat
- [ ] Add keywords for targeting
- [ ] Add dialogue for non-hostile NPCs
- [ ] Add daily routines for NPCs with schedules (shopkeepers, guards, etc.)
- [ ] Set `can_open_doors` for NPCs that path through doors

### Phase 5: Spawn Points
- [ ] Create spawn point for EVERY mobile that should persist
- [ ] Create spawn point for EVERY item that should persist
- [ ] Set reasonable respawn intervals
- [ ] Set max_count to control population
- [ ] Add spawn dependencies for mobile equipment
- [ ] Enable all spawn points

### Phase 6: Testing
- [ ] Walk through entire area
- [ ] Verify all exits work both ways
- [ ] Verify spawn points are working
- [ ] Test all locked doors have accessible keys
- [ ] Check difficulty is appropriate

---

## Water Area Checklist

### Phase 1: Room Setup
- [ ] Create shore/beach rooms with `shallow_water` flag
- [ ] Create open water rooms with `deep_water` flag
- [ ] Create submerged rooms with `underwater` flag (add `dark` for deep areas)
- [ ] Connect rooms with bidirectional exits
- [ ] Write water-themed descriptions for each depth tier

### Phase 2: Access Items
- [ ] Create boat item (misc type, `boat` flag) for deep_water access
- [ ] Create water breathing potion (`liquid_container`, `liqeffect water_breathing 1 300`)
- [ ] Create spawn points for boat and potion in accessible locations
- [ ] Optional: Create shore shopkeeper selling boat and potions

### Phase 3: Aquatic Encounters
- [ ] Create underwater mobile prototypes (prefer piercing damage type)
- [ ] Create spawn points for mobiles in water rooms
- [ ] Note: mobiles do NOT wander into deep_water/underwater rooms, so spawn them directly
- [ ] Create loot items (use `death_only` flag for corpse drops)

### Phase 4: Testing
- [ ] Verify shallow_water rooms are accessible to all
- [ ] Verify deep_water rooms block entry without boat or swimming 5+
- [ ] Verify underwater rooms trigger breath warnings without WaterBreathing buff
- [ ] Verify fire weapons deal 0 damage underwater
- [ ] Verify boat item in inventory allows deep_water entry
- [ ] Verify water breathing potion prevents drowning

---

## Adding a New Mobile Checklist

1. **Create the Prototype**
   - [ ] Unique vnum with area prefix
   - [ ] Name (with article: "a goblin", "the king")
   - [ ] Short description (room display)
   - [ ] Long description (examine text)
   - [ ] Keywords for targeting

2. **Set Combat Stats**
   - [ ] Level appropriate to area
   - [ ] max_hp based on level
   - [ ] damage_dice for attack power
   - [ ] armor_class for defense

3. **Set Behavior**
   - [ ] aggressive if attacks on sight
   - [ ] sentinel if should not wander
   - [ ] scavenger if picks up items
   - [ ] shopkeeper if sells items
   - [ ] can_open_doors if paths through doors

4. **Set Daily Routine** (if NPC should follow a schedule)
   - [ ] Apply preset or add manual entries
   - [ ] Set transition messages with `routine msg`
   - [ ] Set `routine visible on` if players should see schedule
   - [ ] Ensure destination rooms are reachable (max 20 rooms BFS)

5. **Create Spawn Point**
   - [ ] Link to correct area_id
   - [ ] Link to correct room_id
   - [ ] entity_type = "mobile"
   - [ ] Set vnum
   - [ ] Set respawn_interval_secs
   - [ ] Set max_count

6. **Add Equipment** (if needed)
   - [ ] Create item prototypes first
   - [ ] Add spawn dependencies
   - [ ] Set destination (inventory/equipped)
   - [ ] Set wear_location if equipped
   - [ ] For ranged mobs: weapon as `equipped`/`wielded`, ammo as `equipped`/`ready` (bow) or `inventory` (crossbow/firearm)

---

## Adding a New Item Checklist

1. **Create the Prototype**
   - [ ] Unique vnum with area prefix
   - [ ] Name
   - [ ] Short description (ground display)
   - [ ] Long description (examine text)
   - [ ] Keywords for targeting
   - [ ] Appropriate item_type

2. **Set Properties**
   - [ ] weight (affects encumbrance)
   - [ ] value (for shops)
   - [ ] wear_location (if wearable)
   - [ ] flags as needed

3. **Set Type-Specific Stats**
   - Weapon: damage_dice_count, damage_dice_sides, damage_type
   - Armor: armor_class, protects (body parts)
   - Container: max_items, max_weight
   - Food: nutrition, spoil_duration, foodeffect (optional)
   - Liquid Container: liquid type, current/max sips (auto-applies default effects)

4. **Create Spawn Point** (if should respawn)
   - [ ] Link to correct area_id
   - [ ] Link to correct room_id
   - [ ] entity_type = "item"
   - [ ] Set vnum
   - [ ] Set respawn_interval_secs

---

## Connecting Rooms Checklist

For each exit you want to create:

1. [ ] Note source room UUID
2. [ ] Note target room UUID
3. [ ] Call set_room_exit(source, direction, target)
4. [ ] Call set_room_exit(target, opposite_direction, source)

Direction pairs:
- north <-> south
- east <-> west
- up <-> down

---

## Locked Door Checklist

1. **Create the Key**
   - [ ] Create key item prototype
   - [ ] Note key UUID
   - [ ] Create spawn point for key in accessible location

2. **Create the Door**
   - [ ] Call add_room_door on the room
   - [ ] Set direction
   - [ ] Set name (e.g., "iron door")
   - [ ] Set is_closed = true
   - [ ] Set is_locked = true
   - [ ] Set key_id to key's UUID
   - [ ] Add keywords (["iron", "door"])

3. **Create Door on Other Side** (optional, for two-way door)
   - [ ] Repeat add_room_door on connected room
   - [ ] Use same key_id

---

## Adding a Ranged Weapon Checklist

### 1. Create the Weapon Prototype
- [ ] `item_type` = `weapon`
- [ ] `weapon_skill` = `ranged`
- [ ] `ranged_type` = `bow`, `crossbow`, or `firearm`
- [ ] `caliber` matches intended ammo (arrow, bolt, 9mm, etc.)
- [ ] `damage_dice_count` and `damage_dice_sides` set
- [ ] `two_handed` = true for longbows, rifles, heavy crossbows
- [ ] For crossbow/firearm: `magazine_size` set
- [ ] For firearm: `supported_fire_modes` set (single, burst, auto)
- [ ] Optional: `noise_level` override (default based on ranged_type)

### 2. Create the Ammo Prototype
- [ ] `item_type` = `ammunition`
- [ ] `caliber` matches weapon caliber exactly
- [ ] `ammo_count` set (stack size)
- [ ] Optional: `ammo_damage_bonus` for quality ammo
- [ ] For special ammo: `ammo_effect_type`, `ammo_effect_duration`, `ammo_effect_damage`

### 3. Equip on a Mobile (via spawn dependencies)
- [ ] Create mobile spawn point
- [ ] Add dependency: weapon as `equipped`, `wear_location: wielded`
- [ ] For bows: add dependency: ammo as `equipped`, `wear_location: ready`
- [ ] For crossbow/firearm: add dependency: ammo as `inventory`

### 4. Create Attachments (optional)
- [ ] Set `attachment_slot` (scope, suppressor, magazine, accessory)
- [ ] Set bonus fields (accuracy, noise reduction, magazine)
- [ ] Set `attachment_compatible_types` array
- [ ] One attachment per slot type per weapon

---

## Adding a Consumable/Potion Checklist

1. **Create the Prototype**
   - [ ] Unique vnum with area prefix
   - [ ] Name, short_desc, long_desc
   - [ ] Keywords for targeting

2. **Set Type and Liquid**
   - [ ] `item_type` = `liquid_container`
   - [ ] Set liquid type (auto-applies default effects)
   - [ ] Set current and max sips

3. **Customize Effects** (optional)
   - [ ] Override auto-defaults with `liqeffect` if needed
   - [ ] Add additional effects (stat boosts, haste, etc.)
   - [ ] Set appropriate magnitude and duration

4. **Create Spawn Point or Shop Stock**
   - [ ] Add to shopkeeper's `shop_stock` if sold
   - [ ] Or create item spawn point in appropriate room

---

## Troubleshooting Checklist

### "Entity doesn't respawn"
- [ ] Verify spawn point exists
- [ ] Verify spawn point is enabled
- [ ] Verify spawn point vnum matches prototype
- [ ] Check max_count allows more spawns
- [ ] Wait for respawn_interval_secs to pass

### "Can't open door"
- [ ] Verify key exists in game
- [ ] Verify key_id on door matches key UUID
- [ ] Verify player has the key

### "Exit doesn't work"
- [ ] Verify exit was set
- [ ] Check target room exists
- [ ] Verify target room UUID is correct

### "Mobile doesn't attack"
- [ ] Verify aggressive flag is set
- [ ] Check if room has safe flag

### "Shop doesn't work"
- [ ] Verify mobile has shopkeeper flag
- [ ] Check shop_stock has item vnums
- [ ] Verify shop_buy_rate and shop_sell_rate are set
- [ ] If using routines: verify mobile's activity is `working` (shops only work during working hours)

### "NPC doesn't move to destination"
- [ ] Verify routine entries exist (`medit <id> routine`)
- [ ] Verify destination room vnums are valid
- [ ] Check path is reachable within 20 rooms
- [ ] If doors block path: set `can_open_doors` flag
- [ ] If locked doors: ensure mobile has the key in inventory (via spawn dependency)
- [ ] Check mobile is not in combat

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
- [ ] Set level (1-10) and run `autostats` for balanced combat stats
- [ ] Set appropriate flags (aggressive, sentinel, shopkeeper, no_attack)
- [ ] Customize damage_type if not bludgeoning
- [ ] Add keywords for targeting
- [ ] Add dialogue for non-hostile NPCs

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

## Adding a New Mobile Checklist

1. **Create the Prototype**
   - [ ] Unique vnum with area prefix
   - [ ] Name (with article: "a goblin", "the king")
   - [ ] Short description (room display)
   - [ ] Long description (examine text)
   - [ ] Keywords for targeting

2. **Set Combat Stats**
   - [ ] Set level (1-10) appropriate to area difficulty
   - [ ] Run `autostats` to auto-set HP, AC, damage, hit_modifier, all stats
   - [ ] Optionally customize damage_dice or armor_class after autostats
   - [ ] Set damage_type if not bludgeoning

3. **Set Behavior**
   - [ ] aggressive if attacks on sight
   - [ ] sentinel if should not wander
   - [ ] scavenger if picks up items
   - [ ] shopkeeper if sells items
   - [ ] no_attack if cannot be attacked

4. **Create Spawn Point**
   - [ ] Link to correct area_id
   - [ ] Link to correct room_id
   - [ ] entity_type = "mobile"
   - [ ] Set vnum
   - [ ] Set respawn_interval_secs
   - [ ] Set max_count

5. **Add Equipment** (if needed)
   - [ ] Create item prototypes first
   - [ ] Add spawn dependencies with destination "equipped"
   - [ ] Set wear_location (e.g., "wielded" for weapons)
   - [ ] Note: Equipped weapons override mobile's base damage_dice

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
   - Food: nutrition, spoil_duration

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

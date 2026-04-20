# Spawn Point Editor (spedit)

The spawn point editor manages where and how mobiles and items respawn in the game world. Every persistent mobile and item needs a spawn point.

## Core Concepts

### Why Spawn Points Matter
Without a spawn point:
- Killed mobiles won't come back
- Picked up items won't reappear
- Your area becomes empty over time

### Spawn Point Components
- **Area** - Which area owns the spawn point
- **Room** - Where the entity spawns
- **Entity Type** - "mobile" or "item"
- **VNUM** - Which prototype to spawn
- **Max Count** - Maximum simultaneous instances
- **Respawn Interval** - Seconds between respawns
- **Enabled** - Whether the spawn point is active

### Dependencies (Mobile Equipment)
Spawn points for mobiles can include dependencies - items that spawn with the mobile:
- **Inventory** - Items in the mobile's inventory
- **Equipped** - Items the mobile is wearing/wielding
- **Container** - Items inside a container the mobile carries
- **Chance** - Each dependency can have a spawn chance (1-100%, default 100%)

### Death-Only Items
Items with the `death_only` flag are hidden from normal inventory/equipment display. When the mobile dies, all its items transfer to the corpse and the `death_only` flag is cleared, making them visible. Use this for loot like meat on animals or gems on bosses.

## Commands

### Listing and Creating
```
spedit                           - List spawn points in current area
spedit list                      - Same as above
spedit create <room|.> <mobile|item> <vnum> <max> <interval>
```
Use `.` for room to mean "current room".

### Modifying Spawn Points
```
spedit delete <index>            - Delete spawn point
spedit enable <index>            - Enable spawn point
spedit disable <index>           - Disable spawn point
spedit max <index> <count>       - Set max count
spedit interval <index> <secs>   - Set respawn interval
```

### Managing Dependencies
```
spedit dep add <index> inv <item_vnum> [count] [chance%]
spedit dep add <index> equip <item_vnum> <slot> [chance%]
spedit dep add <index> contain <item_vnum> [count] [chance%]
spedit dep remove <index> <dep_index>
spedit dep clear <index>
```
The optional `chance%` parameter (1-100, default 100) controls the probability of the item spawning.

## Examples

### Basic Mobile Spawn
```
# Create a wolf spawn in current room, max 3, respawn every 5 minutes
spedit create . mobile forest:wolf 3 300
spedit enable 0
```

### Basic Item Spawn
```
# Create a healing potion spawn, max 1, respawn every 10 minutes
spedit create . item potions:heal_minor 1 600
spedit enable 0
```

### Shopkeeper with Equipment
```
# Create shopkeeper spawn (single, long respawn)
spedit create . mobile town:blacksmith 1 900

# Add equipped items (apron, hammer)
spedit dep add 0 equip town:blacksmith_apron torso
spedit dep add 0 equip town:hammer wielded

# Add inventory (gold pouch for transactions)
spedit dep add 0 inv town:gold_pouch 1

# Enable the spawn
spedit enable 0
```

### Guard with Weapon and Armor
```
# Create guard spawn
spedit create . mobile town:guard 2 300

# Equip full gear
spedit dep add 0 equip town:guard_helm head
spedit dep add 0 equip town:guard_armor torso
spedit dep add 0 equip town:guard_sword wielded
spedit dep add 0 equip town:guard_shield offhand

# Enable
spedit enable 0
```

### Boss with Key
```
# Create boss spawn (single, 30 minute respawn)
spedit create dungeon:boss_chamber mobile dungeon:dragon 1 1800

# Boss carries the key to treasure room
spedit dep add 0 inv dungeon:treasure_key 1

# Enable
spedit enable 0
```

### Chance Loot (Rare Drops)
```
# Create a boss with guaranteed sword + 25% chance rare gem
spedit create . mobile dungeon:ogre_chief 1 1800

# Always has a sword equipped
spedit dep add 0 equip dungeon:ogre_club wielded

# 25% chance to carry a rare gem
spedit dep add 0 inv dungeon:rare_gem 1 25

# Enable
spedit enable 0
```

### Death-Only Loot (Animal Drops)
```
# Create a deer spawn
spedit create . mobile forest:deer 3 300

# Deer always carries venison (set death_only flag via oedit on the prototype)
# The venison is hidden while the deer is alive, appears in corpse on death
spedit dep add 0 inv forest:venison 1

# Enable
spedit enable 0
```
First set `death_only` on the venison prototype: `oedit forest:venison flag death_only on`

## Equipment Slots

Available slots for `spedit dep add <index> equip`:

| Slot | Description |
|------|-------------|
| `head` | Helmets, hats |
| `neck` | Necklaces, collars |
| `shoulders` | Pauldrons, mantles |
| `torso` | Armor, robes |
| `back` | Cloaks, capes |
| `arms` | Bracers, sleeves |
| `hands` | Gloves, gauntlets |
| `waist` | Belts, sashes |
| `legs` | Leggings, pants |
| `feet` | Boots, shoes |
| `wrists` | Wristguards, bracelets |
| `ankles` | Anklets |
| `wielded` | Main weapon |
| `offhand` | Shield, secondary weapon |
| `ears` | Earrings |

## Respawn Timing Guidelines

| Entity Type | Interval | Use Case |
|-------------|----------|----------|
| 60 | 1 min | Trash mobs, common items |
| 300 | 5 min | Standard mobs and items |
| 600 | 10 min | Uncommon items |
| 900 | 15 min | Named NPCs, rare items |
| 1800 | 30 min | Mini-bosses, very rare items |
| 3600 | 1 hour | Major bosses, unique items |

## Best Practices

1. **Always Enable Spawn Points**
   - Spawn points are disabled by default
   - Don't forget `spedit enable <index>`

2. **Match Prototype VNUM**
   - The vnum must exactly match the prototype
   - Check with `medit <vnum>` or `oedit <vnum>` first

3. **Sensible Max Counts**
   - Shopkeepers/guards: 1-2
   - Common mobs: 2-5
   - Rare mobs: 1
   - Items: Usually 1

4. **Prototype Requirements**
   - The mobile/item must be marked as a prototype
   - Non-prototypes cannot be spawned

## Troubleshooting

### "Entity doesn't respawn"
1. Check spawn point exists: `spedit list`
2. Verify it's enabled (shows `[ON]` in list)
3. Check max_count allows more spawns
4. Verify vnum matches prototype exactly
5. Wait for respawn_interval_secs to pass

### "Spawn point not found"
- Use index from `spedit list` (0, 1, 2, etc.)
- Or use full UUID if you have it

### "Failed to add dependency"
- For equipment: verify the slot name is valid
- Verify the item prototype exists
- Check item vnum spelling

### "Mobile spawns without equipment"
- Dependencies require the item to be a prototype
- Check that all item vnums in dependencies exist

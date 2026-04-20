# Common Building Patterns

## Pattern: Linear Path

A simple line of rooms, good for roads, corridors, or trails.

```
[Start] -- north --> [Middle] -- north --> [End]
         <-- south --         <-- south --
```

Steps:
1. Create rooms with sequential vnums (path_1, path_2, path_3)
2. Connect north/south (or east/west) exits
3. Remember to connect BOTH directions

## Pattern: Grid Layout

Classic dungeon layout with intersections.

```
    [NW]----[N]----[NE]
      |      |      |
    [W]---[Center]---[E]
      |      |      |
    [SW]----[S]----[SE]
```

Steps:
1. Create 9 rooms with grid positions in vnum (nw, n, ne, w, center, etc.)
2. Connect horizontal exits (east/west)
3. Connect vertical exits (north/south)

## Pattern: Hub and Spokes

Central room with branches, good for town squares or clearings.

```
         [North]
            |
    [West]--[Hub]--[East]
            |
         [South]
```

Steps:
1. Create hub room first
2. Create branch rooms
3. Connect all branches to hub
4. Optionally extend branches further

## Pattern: Tower (Vertical)

Multi-floor structure using up/down exits.

```
    [Top Floor]
         |
    [Middle Floor]
         |
    [Ground Floor]
```

Steps:
1. Create rooms for each floor
2. Connect with up/down exits
3. Set `indoors` flag on all rooms
4. Consider adding stairs room description

## Pattern: Boss Lair

Challenging area with guardian and treasure.

```
    [Entrance]
         |
    [Antechamber] -- door (locked)
         |
    [Boss Room] <- boss mob spawn
         |
    [Treasure Room] <- loot spawns
```

Steps:
1. Create area and rooms
2. Add locked door to boss room
3. Create key item prototype
4. Create boss mobile with high HP/damage
5. Create spawn point for boss (long respawn: 1800s)
6. Create loot items and spawn points
7. Optional: Add spawn dependency for boss to hold key

## Pattern: Shop

NPC merchant location.

```
    [Shop Interior]
         |
    [Street] -- other connections
```

Steps:
1. Create shop room with `indoors` flag
2. Create shopkeeper mobile with `shopkeeper` flag
3. Set shop rates (buy/sell percentages)
4. Add stock items to shop_stock
5. Create spawn point for shopkeeper (`sentinel` flag recommended)

## Pattern: Road Encounter

Random monsters along a path.

```
    [Town] -- [Road 1] -- [Road 2] -- [Road 3] -- [Destination]
                ^             ^
           wolf spawn   bandit spawn
```

Steps:
1. Create road rooms connecting locations
2. Create mobile prototypes (aggressive flag)
3. Create spawn points in different road sections
4. Set reasonable max_count to avoid overcrowding

## Pattern: Locked Door Puzzle

Player must find key to progress.

```
    [Key Room] -- (contains key)
         |
    [Junction]
         |
    [Locked Door] --> [Secret Area]
```

Steps:
1. Create rooms
2. Create key item prototype
3. Create door with key_id referencing key
4. Create spawn point for key
5. Hide key room off main path

## Pattern: Healer Station

Safe area with healing services.

```
    [Healer's Hut]
         |
    [Village Square]
```

Steps:
1. Create room with `safe` and `indoors` flags
2. Create healer mobile with `healer` and `sentinel` flags
3. Set healer_type and healing options
4. Create spawn point (very long respawn or immediate)
5. Add dialogue for roleplay

## Pattern: Ranged Encounter

Area designed for ranged combat with open terrain and distance engagement.

```
    [Sniper Perch]    (elevated, ranged mob)
         |
    [Open Field]      (wide open, no cover)
         |
    [Approach Road]   (player enters here)
```

Steps:
1. Create rooms with open/outdoor descriptions emphasizing sight lines
2. Create ranged weapon and ammo prototypes
3. Create mobile with `aggressive` flag (optionally `cowardly` for flee behavior)
4. Create spawn point for mobile in the perch/field room
5. Add spawn dependencies:
   - Weapon as `equipped`, `wear_location: wielded`
   - Ammo as `equipped`, `wear_location: ready` (bow) or `inventory` (crossbow/firearm)
6. Optional: Add `on_flee` trigger with `@shout` for the mob to call reinforcements
7. Optional: Place ammo loot in the area for player resupply

Design notes:
- Aggressive ranged mobs start combat at ranged distance when a player enters
- Melee players must `advance` twice (ranged -> pole -> melee) to close the gap
- Multiple ranged mobs in an open area create a challenging encounter
- `cowardly` mobs will flee at 25% HP, adding tactical variety
- Use `noise_level` on weapons to control whether sniping alerts adjacent mobs

## Pattern: Potion Shop

NPC selling consumables with effects.

```
    [Potion Shop]
         |
    [Market Square] -- other connections
```

Steps:
1. Create shop room with `indoors` and `safe` flags
2. Create liquid_container item prototypes for each potion:
   - Set liquid type (e.g., `healing_potion`, `mana_potion`)
   - Auto-effects are applied; override with `liqeffect` for custom potions
   - For custom buff potions: `liqeffect strength_boost 3 300`
3. Create shopkeeper mobile with `shopkeeper` and `sentinel` flags
4. Add potion vnums to `shop_stock`
5. Create spawn point for shopkeeper

## Pattern: Water Area

Coastal, lake, or underwater exploration area with progressive water depth.

```
    [Beach]         (safe, shallow_water)
       |
    [Shallows]      (shallow_water)
       |
    [Open Water]    (deep_water) -- requires boat or swimming 5+
       |
    [Deep Dive]     (underwater) -- requires WaterBreathing buff
       |
    [Sea Floor]     (underwater, dark)
```

Steps:
1. Create rooms with progressive water depth flags
2. Set `shallow_water` on shore/beach rooms (accessible to all)
3. Set `deep_water` on open water rooms (requires boat or swimming 5+)
4. Set `underwater` on submerged rooms (requires WaterBreathing buff)
5. Create a boat item with `boat` flag for deep_water access
6. Create a water breathing potion (`liqeffect water_breathing 1 300`) for underwater access
7. Create aquatic mobile prototypes for underwater encounters
8. Use piercing weapons for underwater mobs (piercing gets +15% underwater)
9. Spawn boat and potion items in accessible locations (shore shop, treasure)

Design notes:
- Shallow water costs +1 stamina, deep +2, underwater +3 (swimming skill reduces)
- Swimming skill trains automatically when moving through water rooms
- Mobiles will NOT wander into deep_water or underwater rooms
- Fire damage is extinguished underwater; slashing/bludgeoning reduced 25%
- Players without WaterBreathing lose breath underwater (drowning at 0 breath)
- Create a shop on shore selling boats, water breathing potions, and piercing weapons

## Scaling Guidelines

### Small Area (5-10 rooms)
- 2-3 mobile types
- 3-5 item types
- 3-5 spawn points
- Good for: outposts, small dungeons, camps

### Medium Area (10-25 rooms)
- 4-6 mobile types
- 5-10 item types
- 8-15 spawn points
- Good for: villages, dungeons, forests

### Large Area (25+ rooms)
- 6+ mobile types
- 10+ item types
- 15+ spawn points
- Good for: cities, major dungeons, wilderness

## Quality Checklist

Before finishing an area, verify:

- [ ] All rooms have titles and descriptions
- [ ] All exits connect both ways
- [ ] All mobiles have spawn points
- [ ] All persistent items have spawn points
- [ ] Doors have keys accessible somewhere
- [ ] Level-appropriate difficulty
- [ ] No dead-end rooms (unless intentional)
- [ ] Vnum naming is consistent

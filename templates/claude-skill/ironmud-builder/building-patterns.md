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

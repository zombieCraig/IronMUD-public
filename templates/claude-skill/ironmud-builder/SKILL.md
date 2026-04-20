---
name: ironmud-builder
description: Build MUD areas with rooms, items, mobiles, and spawn points. Use when creating game content, designing dungeons, or populating areas with NPCs and items.
---

# IronMUD Area Builder

You are helping build content for IronMUD, a text-based multiplayer game (MUD). This skill provides knowledge about IronMUD's building system to help you use the MCP tools effectively.

## Core Concepts

1. **Areas** group related rooms together (forest, castle, dungeon)
2. **Rooms** are connected by exits (north/south/east/west/up/down)
3. **Items** are prototypes that spawn instances (sword template -> actual sword)
4. **Mobiles** are NPCs that can be aggressive, shopkeepers, or passive
5. **Spawn Points** control respawning of mobiles/items after death/pickup

## Critical: Building Order

When building an area, you MUST follow this order:

1. **Create the area** - Establishes prefix for vnums (e.g., "forest")
2. **Create room prototypes** - Use area prefix in vnums (e.g., "forest:entrance")
3. **Connect rooms with exits** - Use set_room_exit to link rooms
4. **Create item prototypes** - Weapons, keys, loot, equipment
5. **Create mobile prototypes** - NPCs, monsters, shopkeepers
6. **Create spawn points** - Make mobiles/items respawn
7. **Add spawn dependencies** - Equipment that spawns with mobiles

## CRITICAL WARNING: Spawn Points

**Mobiles and items without spawn points will NOT respawn after death/pickup!**

Every mobile and item that should persist in the game needs a spawn point. Without one:
- Killed monsters won't come back
- Picked up items won't reappear
- Your carefully designed area becomes empty

## Vnum Naming Convention

Vnums follow the pattern `prefix:name` where:
- `prefix` is the area prefix (e.g., "forest", "castle")
- `name` describes the entity (e.g., "entrance", "wolf", "iron_sword")

Examples:
- `forest:entrance` - A room vnum
- `forest:wolf` - A mobile vnum
- `forest:old_key` - An item vnum

## Core Editors

| Editor | Command | Purpose |
|--------|---------|---------|
| Area Editor | `aedit` | Create and manage areas |
| Room Editor | `redit` | Create and edit rooms |
| Object Editor | `oedit` | Create and edit items |
| Mobile Editor | `medit` | Create and edit NPCs/monsters |

## Specialized Editors

Beyond the core building tools, IronMUD has specialized editors for complex systems:

### Transport Editor (tedit)
Create elevators, buses, trains, and other transport systems that move players between locations.
- See `transports.md` for detailed documentation

### Property Editor (pedit)
Create rental property templates that players can lease. Templates can be instantiated multiple times.
- See `properties.md` for detailed documentation

### Spawn Point Editor (spedit)
Manage spawn points for mobiles and items in an area. Add equipment dependencies for mobiles.
- See `spawn-points.md` for detailed documentation

### Recipe Editor (recedit)
Create crafting and cooking recipes with ingredients, tools, and skill requirements.
- See `recipes.md` for detailed documentation

## Room Design Tips

- Keep titles short (shown when moving)
- Write evocative descriptions that establish atmosphere
- Use room flags appropriately:
  - `dark` - Requires light source to see
  - `no_mob` - No mobiles can enter
  - `indoors` - Protected from weather
  - `safe` - No combat allowed

## Mobile Design Tips

- Set `level` (1-10) then use `autostats` to auto-set HP/AC/damage/stats
- Use `aggressive` flag for hostile monsters
- Use `sentinel` flag for NPCs that shouldn't wander
- Use `shopkeeper` flag for merchants
- Use `no_attack` flag for important NPCs players shouldn't fight
- Equip weapons via spawn dependencies for varied damage

## Item Design Tips

- Items must have a `vnum` to be prototypes
- Set `item_type` appropriately (weapon, armor, key, etc.)
- Use flags:
  - `glow` - Item provides light
  - `no_drop` - Cannot be dropped (quest items)
  - `no_get` - Cannot be picked up (furniture/fixtures)
  - `unique` - Only one can exist

## Spawn Point Best Practices

- Set reasonable `respawn_interval_secs` (300 = 5 minutes is common)
- Use `max_count` to control population density
- Add dependencies for mobile equipment
- Remember to enable spawn points after creating them

## Example: Creating a Simple Area

```
1. Create area "Small Forest" with prefix "smallforest"
2. Create 3 rooms:
   - smallforest:entrance (south)
   - smallforest:clearing (middle)
   - smallforest:depths (north)
3. Connect exits:
   - entrance.north -> clearing
   - clearing.south -> entrance
   - clearing.north -> depths
   - depths.south -> clearing
4. Create mobile prototype "smallforest:wolf"
5. Create spawn point for wolf in depths room
6. Enable the spawn point
```

## Common Mistakes to Avoid

1. **Forgetting spawn points** - Most common mistake!
2. **Not enabling spawn points** - Spawn points are disabled by default
3. **Not connecting exits both ways** - Rooms should have return exits
4. **Invalid vnums** - Use area prefix consistently
5. **Missing required fields** - Items need short_desc and long_desc
6. **Wrong entity type in spawn point** - "mobile" vs "item"

## See Also

For more detailed information:
- `mechanics.md` - Game mechanics reference (flags, triggers, damage types)
- `building-patterns.md` - Common area designs and layouts
- `checklists.md` - Step-by-step workflows
- `transports.md` - Transport editor guide
- `properties.md` - Property editor guide
- `spawn-points.md` - Spawn point editor guide
- `recipes.md` - Recipe editor guide

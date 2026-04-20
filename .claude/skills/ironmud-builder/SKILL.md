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

### Plant Prototype Editor (plantedit)
Create plant species templates for the gardening system. Define growth stages, seasons, water needs, harvest items, and skill requirements.
- See `gardening.md` for detailed documentation

## Description Generation

The MCP server provides context-gathering tools to help generate rich, thematic descriptions. Use these before writing descriptions:

### Workflow

1. **Create entity** with placeholder description
2. **Call context tool** (`get_room_context`, `get_item_context`, or `get_mobile_context`)
3. **Review context** - area theme, flags, connected elements
4. **Write description** incorporating suggested elements
5. **Update entity** with the new description

### Context Tools

| Tool | Purpose |
|------|---------|
| `get_room_context` | Area theme, connected rooms, flag-based atmosphere elements |
| `get_item_context` | Item type guidance, flag elements (glow, hum, etc.) |
| `get_mobile_context` | Role detection (merchant, guard, monster), behavior hints |
| `get_description_examples` | Find example descriptions from existing entities |

### Style Guide

See `description-style.md` for detailed guidance on:
- Length guidelines (room: 2-4 sentences, short_desc: 5-15 words)
- Perspective (second person for rooms, third for items/mobiles)
- Theme-specific sensory elements
- Flag-based description elements

## Gardening Design Tips

- Plant prototypes define species; seeds are items that reference the prototype via `plant_prototype_vnum`
- Set `dirt_floor` flag on rooms where outdoor planting is allowed
- Set `garden` flag for thematic room display
- Gardening items need the `plant_pot` flag for pots
- Any `liquid_container` with liquid can water plants (water is most efficient, other liquids vary)
- Gardening items need specific fields: `plant_prototype_vnum` (seeds), `fertilizer_duration` (fertilizer), `treats_infestation` (pest treatment)
- Create a shopkeeper selling seeds and tools for easy player access
- Use spawn points for seeds/tools in the gardening shop
- Multi-harvest plants (`multi_harvest: true`) reset to Growing after harvest, ideal for herbs and flowers

## Room Design Tips

- Keep titles short (shown when moving)
- Write evocative descriptions that establish atmosphere
- Use context tools to gather theme and flag information
- Use room flags appropriately:
  - `dark` - Requires light source to see
  - `no_mob` - No mobiles can enter
  - `indoors` - Protected from weather
  - `safe` - No combat allowed
  - `shallow_water` - Surface water (+1 stamina cost, trains swimming)
  - `deep_water` - Deep water (requires boat item or swimming skill 5+)
  - `underwater` - Submerged (requires WaterBreathing buff or player drowns)

## Mobile Design Tips

- Use `aggressive` flag for hostile monsters
- Use `sentinel` flag for NPCs that shouldn't wander (routine destinations still override this)
- Use `shopkeeper` flag for merchants
- Set appropriate `level` and `max_hp` for challenge
- Human/humanoid combat mobiles should carry gold as loot (use spawn dependency with `destination: inventory`). Scale gold to difficulty: ~5-15 gold for levels 1-3, ~15-40 for levels 4-7, ~40-100 for levels 8-12, ~100-250 for higher levels
- Use daily routines for NPCs that should move between locations on a schedule
- Apply routine presets for common patterns: `merchant_8to20`, `guard_dayshift`, `guard_nightshift`, `tavern_keeper`, `wandering_merchant`
- Set `can_open_doors` on NPCs that need to path through closed/locked doors (guards, shopkeepers going home)
- Set `routine visible on` so players can use `schedule <npc>` to see the routine

### Daily Routine Quick Reference

Routines make NPCs move between rooms and change activity states on a game-hour schedule. 1 game hour = 2 real minutes.

**Preset workflow** (fastest):
```
medit <vnum> routine preset merchant_8to20 shop=<work_room_vnum> home=<home_room_vnum>
medit <vnum> routine visible on
```

**Manual workflow:**
```
medit <vnum> routine add <hour> <activity> [destination_vnum]
medit <vnum> routine msg <hour> {name} does something.
```

Available presets and their destination keys:
- `merchant_8to20`: `shop`, `home`
- `guard_dayshift`: `post`, `barracks`
- `guard_nightshift`: `post`, `barracks`
- `tavern_keeper`: `tavern`, `home`
- `wandering_merchant`: `market`, `camp`

Activity states: `working`, `sleeping`, `patrolling`, `off_duty`, `socializing`, `eating`

Only `working` allows shop/healer services. `sleeping` NPCs show a special room description and don't respond to dialogue. Transition messages use `{name}` for the mobile's name.

## Water Area Design Tips

Building water-themed areas uses three room flag tiers:

1. **Shallow water** (`shallow_water` flag): Beaches, streams, fords. +1 stamina cost. Swimming skill trains at 5 XP/move.
2. **Deep water** (`deep_water` flag): Lakes, rivers, open sea. +2 stamina cost. Blocked unless player has a boat item or swimming skill 5+. Swimming trains at 10 XP/move.
3. **Underwater** (`underwater` flag): Submerged caves, ocean floor. +3 stamina cost. Breath depletes without WaterBreathing buff. Drowning damage at 15% max HP per tick when breath reaches 0. Swimming trains at 15 XP/move.

**Boat items**: Create a misc item with the `boat` flag. Players carrying a boat can traverse deep_water rooms without swimming skill.

**Water breathing**: Apply the `water_breathing` buff (via potions with `liqeffect water_breathing 1 300` or spells) to allow safe underwater exploration.

**Underwater combat**: Damage is modified underwater — slashing/bludgeoning -25%, piercing +15%, fire attacks are extinguished, cold +10%. Design underwater encounters with piercing weapons for best results.

**Mob wander restriction**: Mobiles will NOT wander into `deep_water` or `underwater` rooms, keeping land-based mobs from drowning.

**Typical water area layout**:
```
[Shore] --shallow_water-- [Shallows] --deep_water-- [Open Water]
                                                         |
                                              --underwater-- [Sea Floor]
```

## Item Design Tips

- Items must have a `vnum` to be prototypes
- Set `item_type` appropriately (weapon, armor, key, ammunition, etc.)
- Use flags:
  - `glow` - Item provides light
  - `no_drop` - Cannot be dropped (quest items)
  - `no_get` - Cannot be picked up (furniture/fixtures)
  - `unique` - Only one can exist
  - `death_only` - Hidden until mobile dies (loot items like meat, gems)
  - `boat` - Allows traversing deep_water rooms when carried
- Ranged weapons require `weapon_skill: ranged`, a `ranged_type` (bow/crossbow/firearm), and a `caliber` matching their ammo
- Ammunition items use `item_type: ammunition` with matching `caliber` and an `ammo_count`
- Attachments modify ranged weapons via `attachment_slot`, accuracy/noise/magazine bonuses, and compatible type lists

## Consumable Design Tips

- Setting a liquid type auto-applies default effects (e.g., `coffee` gets `stamina_restore(8)` + `quenched(70)`)
- Override auto-defaults with `liqeffect`/`clearliqeffects` for custom potions
- Use `heal` / `poison` for instant HP effects, `regeneration` for heal-over-time
- Use `mana_restore` only for mana-enabled characters (admin flag)
- Stat boost durations of 120-300 seconds are typical (2-5 real minutes)
- `haste` and `slow` significantly affect movement - use sparingly
- `invisibility` hides from `look` and `who` but not combat
- Drunk level > 30 garbles speech, > 50 causes movement stumbling
- Same-type buffs refresh rather than stack - higher magnitude wins
- Liquid containers can water plants: water=100%, tea/juice=75%, ale/beer=50%, poison=damage

## Spawn Point Best Practices

- Set reasonable `respawn_interval_secs` (300 = 5 minutes is common)
- Use `max_count` to control population density
- Add dependencies for mobile equipment
- Use chance (1-100%) on dependencies for rare loot drops
- Use `death_only` flag on items that should only appear in corpses
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
7. **Routine destination unreachable** - Destination must be within 20 rooms via connected exits. Check for missing exits or blocked doors without `can_open_doors`
8. **Shop unavailable due to routine** - Shopkeepers with routines only sell when activity is `working`. Verify the routine has a `working` entry covering business hours

## See Also

For more detailed information:
- `mechanics.md` - Game mechanics reference (flags, triggers, damage types)
- `building-patterns.md` - Common area designs and layouts
- `checklists.md` - Step-by-step workflows
- `ranged-weapons.md` - Ranged weapons, ammunition, and attachments
- `weapon-balance.md` - Weapon damage balance tables (medieval, modern, cyberpunk)
- `transports.md` - Transport editor guide
- `properties.md` - Property editor guide
- `spawn-points.md` - Spawn point editor guide
- `recipes.md` - Recipe editor guide
- `gardening.md` - Gardening system and plant prototype editor guide

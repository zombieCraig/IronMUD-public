# Area Management

This guide covers creating and managing areas using IronMUD's Online Creation (OLC) system.

## Area Commands

| Command | Usage | Description |
|---------|-------|-------------|
| `alist` | `alist` | List all areas |
| `acreate` | `acreate <prefix> <name>` | Create a new area |
| `aedit` | `aedit <area_id> [subcommand]` | Edit area properties |
| `adelete` | `adelete <area_id>` | Delete an area |

## Creating Areas

Create an area with a prefix and name:

```
> acreate forest Dark Forest
Created area: Dark Forest [forest]
Area ID: 550e8400-e29b-41d4-a716-446655440004
You are now the owner.
```

The prefix is used for room vnums (e.g., `forest:entrance`).

## aedit Subcommands

### Basic Properties

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `show` | `aedit <id>` | Display area properties |
| `name` | `aedit <id> name <text>` | Set area name |
| `desc` | `aedit <id> desc <text>` | Set area description |
| `prefix` | `aedit <id> prefix <text>` | Set area prefix |
| `theme` | `aedit <id> theme <text>` | Set area theme |
| `levels` | `aedit <id> levels <min> <max>` | Set level range |

### Permissions

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `owner` | `aedit <id> owner <name\|clear>` | Set or clear owner |
| `permission` | `aedit <id> permission <level>` | Set permission level |
| `trust` | `aedit <id> trust <name>` | Add trusted builder |
| `untrust` | `aedit <id> untrust <name>` | Remove trusted builder |
| `trustees` | `aedit <id> trustees` | List trusted builders |

### Combat Zone & Flags

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `zone` | `aedit <id> zone <pve\|safe\|pvp>` | Set area combat zone (rooms inherit unless overridden) |
| `flags` | `aedit <id> flags [climate_controlled] [on\|off]` | Toggle area flags |

| Zone | Effect |
|------|--------|
| `pve` | Players can attack mobiles only (default) |
| `safe` | No combat allowed |
| `pvp` | Players can attack mobiles and other players |

The `climate_controlled` flag causes rooms to skip weather/season triggers. Rooms inherit this unless they set their own value.

### Immigration (Migrant Spawning)

Areas can grow their own populations over time by spawning migrant NPCs who move into liveable rooms. The migration tick runs on a game-day interval and places one migrant per liveable room that still has capacity. Migrant mobiles are area-tagged (`migrant:<role>:<prefix>`) and release their residency automatically when they die.

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `immigration` | `aedit <id> immigration` | Show immigration config + current migrant counts |
| `immigration on\|off` | `aedit <id> immigration on` | Enable or disable migrant spawning |
| `immigration room` | `aedit <id> immigration room <vnum>` | Set the arrival room vnum |
| `immigration namepool` | `aedit <id> immigration namepool <name>` | Choose the name pool (e.g. `generic`, `japan`) |
| `immigration visuals` | `aedit <id> immigration visuals <name>` | Choose the visual profile (e.g. `human`) |
| `immigration interval` | `aedit <id> immigration interval <1-30>` | Game days between migration checks |
| `immigration max` | `aedit <id> immigration max <n>` | Maximum migrants spawned per check |
| `immigration workhours` | `aedit <id> immigration workhours <start> <end>` | Default work hours for new migrants |
| `immigration pay` | `aedit <id> immigration pay <gold>` | Default work pay for new migrants |
| `immigration clear_sim` | `aedit <id> immigration clear_sim` | Reset migrant sim defaults |
| `immigration variations` | `aedit <id> immigration variations <role> <0.0-1.0>` | Per-role specialization chance |
| `immigration family_chances` | `aedit <id> immigration family_chances <shape> <0.0-1.0>` | Chance that an arrival comes as a family unit |
| `immigration list` | `aedit <id> immigration list` | List every migrant currently living in the area |

#### Variations

Each immigration check rolls through known roles in order. The first chance that hits wins; otherwise the migrant is a "common" settler. Roles stack atomic flags, perception, activity, and clothing onto the base migrant.

| Role | Chance Key | Effect |
|------|-----------|--------|
| `guard` | `immigration variations guard 0.1` | `guard`/`no_attack`/`can_open_doors` flags, perception 5, patrolling, livery |
| `healer` | `immigration variations healer 0.05` | `healer`/`no_attack` flags, herbalist services, working, robes |
| `scavenger` | `immigration variations scavenger 0.08` | `scavenger`/`can_open_doors` flags, perception 4, working, patched clothes (stays attackable) |

Chances are independent — a value of `0.0` disables that role for the area.

#### Family Chances

New arrivals can come as a family unit that shares a household and seeds relationships between members. Families each consume two liveable slots.

| Shape | Meaning |
|-------|---------|
| `parent_child` | One adult + one child, linked as Parent/Child |
| `sibling_pair` | Two adult siblings sharing a household |

```
> aedit forest immigration family_chances parent_child 0.15
Immigration family 'parent_child' chance set to 15%.
```

#### Example Setup

```
> aedit town immigration on
Immigration enabled for area 'Starting Town'.

> aedit town immigration room town:gate
Immigration arrival room set to 'town:gate'.

> aedit town immigration namepool generic
Immigration name pool set to 'generic'.

> aedit town immigration visuals human
Immigration visual profile set to 'human'.

> aedit town immigration interval 7
Migration interval set to 7 game days.

> aedit town immigration max 2
Max migrants per check set to 2.

> aedit town immigration variations guard 0.10
Immigration 'guard' chance set to 10%.

> aedit town immigration family_chances parent_child 0.20
Immigration family 'parent_child' chance set to 20%.
```

Migrants need somewhere to live. Mark rooms as `liveable` and set their `capacity` — see [Rooms: Liveable Rooms](rooms.md#liveable-rooms).

## Permission Levels

| Level | Who Can Edit |
|-------|--------------|
| `owner_only` | Only the area owner |
| `trusted` | Owner and trusted builders |
| `all_builders` | Any builder (default) |

```
> aedit forest permission trusted
Permission set to: trusted

> aedit forest trust Bob
Bob added to trusted builders.

> aedit forest trustees
=== Trusted Builders ===
Bob
```

## Assigning Rooms to Areas

Use `redit area` to assign rooms:

```
> redit area forest
Room assigned to area: Dark Forest

> redit area clear
Room unassigned from area.
```

When you `dig` from a room in an area, new rooms automatically inherit the area.

## Spawn Points

Spawn points automatically respawn mobiles and items in an area.

### Spawn Point Commands

| Command | Usage | Description |
|---------|-------|-------------|
| `spedit` | `spedit [subcommand]` | Manage spawn points (uses current room's area) |
| `areset` | `areset <area_id>` | Manually trigger area reset |

### spedit Subcommands

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `list` | `spedit list [filter]` | List spawn points |
| `create` | `spedit create <room\|.> <mobile\|item> <vnum> <max> <interval>` | Create spawn point |
| `delete` | `spedit delete [filter] <index>` | Delete spawn point |
| `enable` | `spedit enable [filter] <index>` | Enable spawn point |
| `disable` | `spedit disable [filter] <index>` | Disable spawn point |
| `max` | `spedit max [filter] <index> <count>` | Set max spawned |
| `interval` | `spedit interval [filter] <index> <secs>` | Set respawn interval |

### Filters

Filters control which spawn points are shown and which indices are used:

| Filter | Description |
|--------|-------------|
| (none) | Current room only (default) |
| `all` | All spawn points in the area |
| `mobs` | Mobile spawn points only |
| `items` | Item spawn points only |
| `room <vnum>` | Spawn points in a specific room |

### Creating Spawn Points

Use `.` for the current room or specify a room vnum:

```
> spedit create . mobile town_guard 3 300
Created spawn point: town_guard (max 3, every 300s)

> spedit create forest:entrance item health_potion 5 600
Created spawn point: health_potion (max 5, every 600s)
```

This creates spawn points that:
- Maintain up to 3 Town Guards in the current room
- Respawn every 300 seconds (5 minutes)
- Maintain up to 5 Health Potions in the entrance
- Respawn every 600 seconds (10 minutes)

### Managing Spawn Points

```
> spedit list
=== Spawn Points in Forest Clearing ===
[0] [ON] mobile: town_guard
    Room: Forest Clearing  Max: 3  Interval: 300s  Active: 2/3

Showing 1 spawn(s) in this room. Use 'spedit list all' to see all 5 in area.

> spedit list all
=== Spawn Points in Dark Forest (all) ===
[0] [ON] mobile: town_guard
    Room: Forest Clearing  Max: 3  Interval: 300s  Active: 2/3
[1] [ON] item: health_potion
    Room: Forest Entrance  Max: 5  Interval: 600s  Active: 5/5
...

> spedit disable 0
Spawn point disabled.

> spedit list mobs
=== Mobile Spawns in Dark Forest ===
[0] [OFF] mobile: town_guard
    Room: Forest Clearing  Max: 3  Interval: 300s  Active: 2/3

> spedit enable mobs 0
Spawn point enabled.

> spedit max all 1 10
Spawn point 1 max count set to 10.
```

### Manual Reset

Force all spawn points to spawn immediately:

```
> areset forest
Area reset: spawned 8 entities.
```

## Area Properties

Areas have metadata for organization:

```
> aedit forest
=== Area Editor ===
Name: Dark Forest
Prefix: forest
ID: 550e8400-e29b-41d4-a716-446655440004
Description: A dense forest filled with ancient trees and mysterious creatures.
Level Range: 5 - 15
Theme: Nature

--- Permissions ---
Owner: Craig
Permission: trusted
Trusted Builders: Bob, Alice

Combat Zone: [PVE] PvE (attack mobiles only)

--- Area Flags ---
climate_controlled: [off]  (rooms inherit unless overridden)

Rooms in area: 12

--- Migrant Immigration ---
enabled:   [ON]
arrival:   town:gate
names:     generic
visuals:   human
interval:  7 game days (3 days until next check)
max/check: 2
variations: guard 10%
current:   4 (3 common, 1 guard)
```

Set these with:
```
> aedit forest theme Nature
Theme set to: Nature

> aedit forest levels 5 15
Level range set to: 5-15
```

## Listing Areas

```
> alist
=== Areas ===
[forest] Dark Forest (5-15) - Nature
  Owner: Craig | Permission: trusted
[town] Starting Town (1-5) - Urban
  Owner: (none) | Permission: all_builders
```

## Deleting Areas

Deleting an area unassigns its rooms but does not delete them:

```
> adelete forest
Area 'Dark Forest' deleted. 12 rooms unassigned.
```

## Best Practices

1. **Use meaningful prefixes** - Keep them short and descriptive (e.g., `forest`, `cave`, `town`)
2. **Set level ranges** - Helps players find appropriate content
3. **Use permissions** - Protect work in progress with `owner_only` or `trusted`
4. **Configure spawn points** - Ensure areas stay populated

## Related Documentation

- [Rooms](rooms.md) - Room creation and editing
- [Mobiles](mobiles.md) - NPC creation for spawn points
- [Items](items.md) - Item creation for spawn points
- [Builder Guide](../builder-guide.md) - Overview of building

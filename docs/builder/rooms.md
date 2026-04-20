# Room Editing

This guide covers creating and editing rooms using IronMUD's Online Creation (OLC) system.

## Room Commands

| Command | Usage | Description |
|---------|-------|-------------|
| `dig` | `dig <direction> [title]` | Create a new room with bidirectional exit |
| `link` | `link <direction> <room_id> [both]` | Link exit to existing room |
| `unlink` | `unlink <direction>` | Remove exit from current room |
| `redit` | `redit [subcommand]` | Edit current room properties |
| `rlist` | `rlist [all\|detail]` | List rooms in current area |
| `rgoto` | `rgoto <room_id\|vnum>` | Teleport to a room |
| `rdelete` | `rdelete <room_id>` | Delete a room |
| `rfind` | `rfind <keyword>` | Search rooms by title/description |

## redit Subcommands

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `show` | `redit` or `redit show` | Display room properties |
| `title` | `redit title <text>` | Set room title |
| `desc` | `redit desc [text]` | Edit description (opens line editor) |
| `flags` | `redit flags` | Show all room flags with status |
| `flag` | `redit flag <name> [on\|off]` | Toggle or set a room flag |
| `extra list` | `redit extra list` | List extra descriptions |
| `extra add` | `redit extra add <keywords>` | Add extra description |
| `extra edit` | `redit extra edit <keyword>` | Edit existing extra description |
| `extra remove` | `redit extra remove <keyword>` | Remove extra description |
| `vnum` | `redit vnum [alias\|clear]` | Set or clear room vnum |
| `area` | `redit area [id\|prefix\|clear]` | Set or clear room area |
| `zone` | `redit zone [pve\|safe\|pvp\|inherit]` | Set room combat zone |
| `capacity` | `redit capacity <n>` | Set living capacity (for `liveable` rooms) |
| `trigger` | `redit trigger` | List triggers (see [Triggers](triggers.md)) |
| `door` | `redit door` | Manage doors (see below) |
| `seasonal` | `redit seasonal` | Set seasonal descriptions |
| `dynamic` | `redit dynamic` | Set dynamic description |

## Creating Rooms with dig

The `dig` command creates a new room and connects it to your current room:

```
> dig north The Dark Forest
Created room: The Dark Forest
Room ID: 550e8400-e29b-41d4-a716-446655440000
Exit north -> The Dark Forest
Reverse exit south -> The Town Square
```

When you dig from a room that belongs to an area, the new room automatically:
1. Inherits the same area assignment
2. Gets an auto-generated vnum based on the title

**Vnum generation rules:**
- Title is converted to lowercase
- Spaces and dashes become underscores
- Special characters are removed
- Max 24 characters for the slug portion
- Duplicates get a number suffix (`_2`, `_3`, etc.)

**Examples:**
- "The Dark Forest" in area `forest` → `forest:the_dark_forest`
- "Bob's Tavern!" → `forest:bobs_tavern`

## Line Editor

When editing descriptions with `redit desc`, you enter a line-based editor:

| Command | Description |
|---------|-------------|
| `.l` | List buffer with line numbers |
| `.d <n>` | Delete line n |
| `.i <n> <text>` | Insert text before line n |
| `.r <n> <text>` | Replace line n with text |
| `.s /old/new/` | Substitute text |
| `.c` | Clear entire buffer |
| `.u` | Undo last change |
| `.p` | Preview without line numbers |
| `.h` | Show help |
| `.` | Save and exit |
| `@` | Cancel and exit |
| `<text>` | Append text as new line |

**Example Session:**

```
> redit desc
Editing description for: The Dark Forest
(empty)

> Towering ancient oaks block out the sky above.
> A narrow path winds deeper into the forest.
> .l
  1: Towering ancient oaks block out the sky above.
  2: A narrow path winds deeper into the forest.
> .r 1 Ancient oak trees form a dense canopy overhead.
Line 1 replaced.
> .
Description saved.
```

## Extra Descriptions

Extra descriptions let players examine specific objects mentioned in room descriptions:

```
> redit extra add fountain statue
Editing extra description for keywords: fountain, statue

> A weathered stone fountain stands in the center of the square.
> Water trickles from the mouth of a carved fish.
> .
Extra description saved.
```

Players can then use `look fountain` or `look statue` to see this description.

## Room Flags

Room flags control special behaviors:

| Flag | Effect |
|------|--------|
| `dark` | Room is too dark to see without light |
| `safe` | No PvP combat allowed |
| `no_mob` | NPCs cannot enter |
| `indoors` | Weather does not affect room |
| `underwater` | Requires WaterBreathing buff; breath depletes, drowning damage at 0 |
| `shallow_water` | Surface water: +1 stamina cost, trains swimming skill |
| `deep_water` | Deep water: requires boat item or swimming 5+, +2 stamina cost |
| `climate_controlled` | Skips weather/season triggers |
| `always_hot` | Shows heat message in look |
| `always_cold` | Shows cold message in look |
| `dirt_floor` | Allows planting seeds in the ground (gardening) |
| `garden` | Thematic garden room display |
| `city` | Stays lit at night (city streets) |
| `no_windows` | No day/night messages (caves, deep interiors) |
| `difficult_terrain` | Costs 2 stamina to traverse |
| `post_office` | Allows sending mail from the room |
| `bank` | Allows banking commands |
| `spawn_point` | Players can bind spawn here (inns, safe rooms) |
| `liveable` | Migrants can claim this room as their residence |

Toggle flags with:
```
> redit flag safe on
Flag 'safe' set to: ON

> redit flags
=== Room Flags ===
[ON]  safe
[OFF] dark
...
```

## Liveable Rooms

Rooms with the `liveable` flag participate in the area's [immigration system](areas.md#immigration-migrant-spawning). Migrants arriving in the area will claim vacant liveable rooms as their residence, up to the room's `capacity`.

```
> redit flag liveable on
Flag 'liveable' set to: ON

> redit capacity 2
Room living capacity set to 2.

> redit capacity
Living capacity: 2  (residents: 1)  [liveable]
Usage: redit capacity <n>
```

Room display shows current occupancy when the flag is on or any residents are attached. Residents are cleared automatically when the mobile dies (via `delete_mobile`). Pair-housing moves (affinity >= 80) can cohabit two mobiles in the same liveable room even when capacity is 1, freeing the other for new arrivals.

## Combat Zones

Rooms can override the area's combat zone via `redit zone`:

| Zone | Effect |
|------|--------|
| `inherit` (default) | Use the area's zone |
| `pve` | Players may attack mobiles only |
| `safe` | No combat allowed |
| `pvp` | Players may attack mobiles and other players |

```
> redit zone safe
Combat zone set to SAFE.
```

## Door System

Doors control movement between rooms. They support names, descriptions, keywords, and locks.

### Adding a Door

```
> redit door add north gate
Door 'gate' added to north exit.

> redit door desc north A tall iron gate with intricate scrollwork.
Door description set.

> redit door keywords north iron ornate
Door keywords set: iron, ornate

> redit door key north iron_key
Door now requires key: Iron Key (vnum: iron_key)

> redit door sync north
Door synced to connected room (south exit).
```

### Door Subcommands

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `door add` | `redit door add <dir> <name>` | Add door to exit |
| `door remove` | `redit door remove <dir>` | Remove door |
| `door name` | `redit door name <dir> <name>` | Change door name |
| `door desc` | `redit door desc <dir> [text]` | Set door description |
| `door key` | `redit door key <dir> <vnum\|clear>` | Set key requirement |
| `door keywords` | `redit door keywords <dir> <kw>...` | Set door keywords |
| `door open/close` | `redit door open <dir>` | Force door state |
| `door lock/unlock` | `redit door lock <dir>` | Force lock state |
| `door sync` | `redit door sync <dir>` | Copy to connected room |

### Door States

- **open** - Players can pass through
- **closed** - Blocks movement, must open first
- **locked** - Requires correct key to unlock

### Player Interaction

```
> exits
Obvious exits:
  North [closed gate]
  South - Town Square

> open north
The gate is locked.

> unlock gate
You unlock the gate with the Iron Key.

> open gate
You open the gate.

> north
You enter the castle courtyard...
```

## Seasonal Descriptions

Rooms can have descriptions that change with the game season:

```
> redit seasonal
=== Seasonal Descriptions ===
Spring: (none)
Summer: (none)
Autumn: (none)
Winter: (none)

Current season: spring

> redit seasonal spring The sakura trees burst with pink blossoms.
spring description set.

> redit seasonal summer The trees provide cool green shade.
> redit seasonal autumn Golden leaves carpet the ground.
> redit seasonal winter Bare branches glisten with frost.
```

With AI assistance enabled:
```
> redit seasonal spring help cherry blossom trees in full bloom
Generating spring description... please wait.
```

## Dynamic Descriptions

The dynamic description is set by triggers for temporary content:

```
> redit dynamic Rain patters on the cobblestones.
Dynamic description set.

> redit dynamic clear
Dynamic description cleared.
```

See [Triggers](triggers.md) for using dynamic descriptions with scripts.

## Display Order

When a player looks at a room:
1. Base room description
2. Seasonal description (if set for current season)
3. Dynamic description (if set)

All three combine into a single paragraph.

## Related Documentation

- [Areas](areas.md) - Area management and permissions
- [Triggers](triggers.md) - Room triggers and scripting
- [Builder Guide](../builder-guide.md) - Overview of building

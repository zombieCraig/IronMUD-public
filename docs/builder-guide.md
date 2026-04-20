# Builder Guide

This guide introduces IronMUD's Online Creation (OLC) system for building game content.

## What is OLC?

Online Creation lets you create and modify game content while connected to the MUD:
- Create rooms, items, NPCs, and areas
- Edit properties in real-time
- Test changes immediately
- No server restarts required

## Getting Builder Access

The first character created on a fresh database automatically becomes an admin with builder permissions.

For other characters, access depends on the server's `builder_mode` setting:

| Mode | How to Get Access |
|------|-------------------|
| `all` (default) | Use `setbuilder` to toggle your own access |
| `granted` | An admin uses `setbuilder <name> on` |
| `none` | Use the admin utility (see [Admin Guide](admin-guide.md)) |

## Quick Start

### 1. Enable builder mode
```
> setbuilder on
You are now a builder.
```

### 2. Create your first room
```
> dig north My First Room
Created room: My First Room
Exit north -> My First Room
Reverse exit south -> Town Square

> north
My First Room
-------------------
This room has no description yet.

Exits: south
```

### 3. Add a description
```
> redit desc
Editing description for: My First Room
(empty)

> A cozy chamber with stone walls.
> Torchlight flickers against the ceiling.
> .
Description saved.
```

### 4. Look at your work
```
> look
My First Room
-------------------
A cozy chamber with stone walls. Torchlight flickers against the ceiling.

Exits: south
```

## Building Workflow

1. **Plan** - Sketch your area layout and content
2. **Create** - Use `dig` to create rooms, `oedit create` for items, `medit create` for NPCs
3. **Configure** - Set properties, descriptions, flags, and triggers
4. **Test** - Walk through as a player would
5. **Iterate** - Refine based on testing

## OLC Editors

| Editor | Purpose | Guide |
|--------|---------|-------|
| `redit` | Edit rooms | [Room Editing](builder/rooms.md) |
| `oedit` | Edit items | [Item Editing](builder/items.md) |
| `medit` | Edit NPCs | [Mobile Editing](builder/mobiles.md) |
| `aedit` | Edit areas | [Area Management](builder/areas.md) |
| `pedit` | Edit property templates | [Property Templates](builder/properties.md) |
| `tedit` | Edit transports | [Transport Editing](builder/transports.md) |
| `recedit` | Edit recipes | [Recipe Editing](builder/recipes.md) |
| `spedit` | Edit spawn points | [Area Management](builder/areas.md#spawn-points) |

## Key Concepts

### Vnums

Vnums are human-readable identifiers for prototypes:
- `forest:entrance` - Room in the forest area
- `rusty_sword` - Item prototype
- `town_guard` - Mobile prototype

### Prototypes vs Instances

- **Prototype** - A template (never seen by players)
- **Instance** - A copy spawned from a prototype (exists in the world)

Create prototypes with `oedit create` or `medit create`, then spawn instances with `ospawn` or `mspawn`.

### Areas

Areas group rooms together and control:
- Room vnums (e.g., `forest:clearing`)
- Builder permissions
- Spawn points for respawning content

### Triggers

Triggers add scripted behaviors:
- Room triggers - Fire on enter, exit, look, or periodically
- Item triggers - Fire on get, drop, use, examine
- NPC triggers - Fire on greet, say, or idle

See [Triggers](builder/triggers.md) for details.

### Population Simulation

Areas can grow their own populations. Mark rooms as `liveable`, set an arrival room on the area, and migrants arrive over time as simulated NPCs with needs, relationships, and (optionally) families. See:

- [Area immigration](builder/areas.md#immigration-migrant-spawning) — spawn rate, roles, families
- [Liveable rooms](builder/rooms.md#liveable-rooms) — mark residences, set capacity
- [NPC simulation](builder/mobiles.md#simulation-system) — needs, work, pay
- [Social & family](builder/mobiles.md#social-system) — happiness, affinity, households
- [Pregnancy](builder/mobiles.md#pregnancy) — gestation, force-birth

## Common Tasks

### Creating a Simple Area

```
> acreate myarea My First Area
Created area: My First Area [myarea]

> redit area myarea
Room assigned to area: My First Area

> dig north The Entrance
> dig east The Hall
> dig south The Garden
```

### Creating an Item

```
> oedit create Magic Sword
> oedit magic_sword type weapon
> oedit magic_sword damage 2 8
> oedit magic_sword value 100
> ospawn magic_sword
Spawned Magic Sword into the room.
```

### Creating a Shopkeeper

```
> medit create Merchant
> medit merchant flag shopkeeper on
> medit merchant shop stock add magic_sword
> mspawn merchant
Spawned Merchant into the room.

> list
=== Merchant's Wares ===
  Magic Sword    100 gold
```

### Adding Ambient Messages

```
> redit trigger add periodic forest_ambiance
> redit trigger interval 0 60
> redit trigger chance 0 30
```

### Creating a Rentable Property

Property templates let players rent instanced housing:

```
> pedit create cottage
Created property template 'cottage'.
[Teleported to template entrance]

> redit title Cozy Cottage - Living Room
> redit desc
> A warm living room with a crackling fireplace.
> .

> dig north Cottage - Bedroom
> oedit create fireplace
> drop fireplace
> oedit fireplace flag no_get on

> pedit name Cozy Cottage
> pedit rent 50
> pedit done
Template 'cottage' saved.
```

Then assign the template to a leasing agent:

```
> medit create Landlord
> medit landlord flag leasing_agent on
> medit landlord leasing area myarea
> medit landlord leasing add cottage
> mspawn landlord
```

Players can now use `properties`, `tour cottage`, and `rent cottage` with this NPC.

## MXP Support

MXP-capable clients (Mudlet, MUSHclient) get clickable links in OLC output:

```
> mxp on
MXP enabled. Clickable links are now active.
```

Click room names to teleport, click flags to toggle, etc.

## Best Practices

1. **Use descriptive vnums** - `healing_potion` not `pot1`
2. **Set level ranges on areas** - Helps players find content
3. **Test as a player** - Walk through without builder commands
4. **Use spawn points** - Keep areas populated automatically
5. **Add extra descriptions** - Let players examine mentioned objects

## Detailed Documentation

- [Room Editing](builder/rooms.md) - Creating and editing rooms
- [Item Editing](builder/items.md) - Creating and editing items
- [Mobile Editing](builder/mobiles.md) - Creating and editing NPCs, daily routines
- [Area Management](builder/areas.md) - Managing areas and spawn points
- [Property Templates](builder/properties.md) - Creating rentable player housing
- [Triggers](builder/triggers.md) - Adding scripted behaviors
- [Recipe Editing](builder/recipes.md) - Creating crafting recipes

## Claude Code Integration

IronMUD includes a Claude Code skill for AI-assisted building. The skill provides Claude with knowledge about IronMUD's building system, entity relationships, and best practices.

### Installing the Skill

Run the install script from the project root:

```bash
./scripts/install-claude-skill.sh
```

This copies the skill files to `.claude/skills/ironmud-builder/` where Claude Code will discover them.

### What the Skill Provides

- Core building concepts (areas, rooms, items, mobiles, spawn points)
- Specialized editor documentation (tedit, pedit, spedit, recedit)
- Common building patterns and layouts
- Game mechanics reference
- Step-by-step checklists

### Using with MCP

For full integration with the REST API, see the [MCP Server documentation](../mcp-server/README.md).

## Design Principles

1. **Fun First** - Building should feel like play
2. **Immediate Feedback** - Changes take effect instantly
3. **Safety Rails** - Can't delete room you're in, can't delete starting room
4. **Progressive Disclosure** - Simple commands for common tasks
5. **Client Agnostic** - Works in plain telnet, enhanced with MXP

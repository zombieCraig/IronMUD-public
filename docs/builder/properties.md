# Property Templates

This guide covers creating property templates for player housing using IronMUD's Online Creation (OLC) system.

## Overview

The property rental system allows players to rent instanced housing with unlimited storage. Builders create **property templates** that define the rooms and amenities; when players rent, they receive their own private copy of the template.

**Key Concepts:**
- **Template** - A builder-defined blueprint of rooms and amenities
- **Lease** - An active rental agreement between a player and a property
- **Instance** - A private copy of the template created when a player rents
- **Leasing Agent** - An NPC that manages rentals in an area

## Property Commands

| Command | Usage | Description |
|---------|-------|-------------|
| `pedit create` | `pedit create <name>` | Create a new property template (multi-word names supported) |
| `pedit` | `pedit <vnum>` | Enter an existing template for editing (shortcut) |
| `pedit edit` | `pedit edit <vnum>` | Enter an existing template for editing |
| `pedit` | `pedit [subcommand]` | Edit template properties (when in template room) |
| `plist` | `plist` | List all property templates |
| `pfind` | `pfind <keyword>` | Search templates by name/vnum |
| `pdelete` | `pdelete <vnum>` | Delete a property template |

## pedit Subcommands

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `show` | `pedit` | Display template properties |
| `name` | `pedit name <text>` | Set display name |
| `vnum` | `pedit vnum <new_vnum>` | Change template vnum |
| `desc` | `pedit desc <text>` | Set description |
| `rent` | `pedit rent <amount>` | Set monthly rent in gold |
| `max` | `pedit max <count>` | Set max instances (0=unlimited) |
| `level` | `pedit level <num>` | Set level requirement to rent |
| `entrance` | `pedit entrance` | Mark current room as entrance |
| `done` | `pedit done` | Save and exit template editing |

## Creating a Property Template

### 1. Create the Template

```
> pedit create cottage

Created property template 'cottage'.
You are now in the template entrance room.
Use redit to edit rooms, dig to add rooms.
Use 'pedit done' when finished.

Template Entrance
An empty room. Use 'redit' to customize.
```

### 2. Customize the Entrance Room

```
> redit title Cozy Cottage - Living Room
Title set to: Cozy Cottage - Living Room

> redit desc
Editing description for: Cozy Cottage - Living Room
(empty)

> A warm living room with wooden floors and whitewashed walls.
> A stone fireplace dominates the eastern wall.
> .
Description saved.
```

### 3. Add Connected Rooms

```
> dig north Cottage - Bedroom
Created room: Cottage - Bedroom
Exit north -> Cottage - Bedroom
Reverse exit south -> Cozy Cottage - Living Room

> north
Cottage - Bedroom
An empty room...

> redit desc
> A cozy bedroom with a simple bed and wooden dresser.
> Sunlight streams through a small window.
> .
Description saved.
```

When using `dig` in a template room, new rooms automatically inherit the template properties.

### 4. Place Amenities

```
> south
Cozy Cottage - Living Room

> oedit create fireplace
Created item 'fireplace'.

> drop fireplace
You drop a fireplace.

> oedit fireplace flag no_get on
no_get flag set to true.
```

Items with the `no_get` flag become permanent fixtures that players cannot pick up.

### 5. Set Template Properties

```
> pedit name Cozy Cottage
Name set to 'Cozy Cottage'.

> pedit rent 50
Monthly rent set to 50 gold.

> pedit max 10
Maximum instances set to 10.

> pedit level 5
Level requirement set to 5.

> pedit entrance
Current room marked as template entrance.
```

### 6. Finish and Save

```
> pedit done

Template 'cottage' saved.
  Rooms: 2
  Amenities: 1

Returning to Town Square...
```

## Placing Amenities

Amenities are items that stay in place when players rent the property. They're created as regular items with the `no_get` flag:

```
> oedit create stove
Created item 'stove'.

> oedit stove short A cast-iron cooking stove
> oedit stove long The stove radiates warmth. Perfect for preparing meals.

> drop stove
You drop a stove.

> oedit stove flag no_get on
no_get flag set to true.
```

**Common amenities:**
- Fireplace - For warmth and ambiance
- Stove - For cooking
- Bed - For resting
- Storage chest - Decorative (players can drop items anywhere)
- Workbench - For crafting

When a player rents the property, amenities are copied to their instance with `no_get` preserved.

## Configuring Leasing Agents

A leasing agent is an NPC that manages property rentals in an area. Players interact with the agent to list, tour, and rent properties.

### 1. Create the Mobile

```
> medit create Landlord
Created mobile prototype with vnum: landlord
```

### 2. Set the Leasing Agent Flag

```
> medit landlord flag leasing_agent on
leasing_agent flag set to ON.
```

### 3. Configure the Agent

```
> medit landlord leasing

=== Leasing Agent Configuration ===

Available Property Templates:
  (none)

Commands:
  medit <id> leasing add <vnum>         - Add template
  medit <id> leasing remove <vnum>      - Remove template

> medit landlord leasing add cottage
Added 'cottage' to available templates.

> medit landlord leasing add townhouse
Added 'townhouse' to available templates.
```

Note: The agent's managed area is automatically derived from the property templates' area settings. Ensure your templates have their area configured via `pedit <template> area <area_id>`.

### Leasing Subcommands

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `leasing` | `medit <id> leasing` | Show agent configuration |
| `leasing add` | `medit <id> leasing add <vnum>` | Add property template |
| `leasing remove` | `medit <id> leasing remove <vnum>` | Remove template |

### 4. Spawn the Agent

```
> mspawn landlord
Spawned Landlord into the room.
```

Players in the same room can now use `properties`, `tour`, and `rent` commands.

## Player Commands

These commands are available to players when interacting with the property system:

| Command | Usage | Description |
|---------|-------|-------------|
| `properties` | `properties` | List available properties at leasing agent |
| `tour` | `tour <template>` | Preview a property template |
| `tour end` | `tour end` | End tour and return |
| `rent` | `rent <template>` | Rent a property (creates instance) |
| `enter` | `enter` | Enter your property from leasing office |
| `visit` | `visit <player>` | Visit a grouped player's property |
| `property` | `property` | View your property settings |
| `property access` | `property access <level>` | Set party access level |
| `property trust` | `property trust <name>` | Add trusted visitor |
| `property untrust` | `property untrust <name>` | Remove trusted visitor |
| `endlease` | `endlease` | End your lease (items go to escrow) |
| `upgrade` | `upgrade <template>` | Transfer to a new property |
| `escrow` | `escrow` | View escrowed items |
| `escrow retrieve` | `escrow retrieve <num>` | Retrieve items from escrow |

## Player Usage Examples

### Renting a Property

```
> properties

=== Riverside Realty ===

Available Properties:

  Cozy Cottage - 50 gold/month
    A warm living room with a fireplace.
    Level required: 5

  Town House - 150 gold/month
    A two-story home with kitchen and storage.
    Available: 3 of 5

Use 'tour <property>' to preview, 'rent <property>' to lease.
Your gold: 500

> tour cottage

You begin a tour of 'Cozy Cottage'...

Cozy Cottage - Living Room
A warm living room with wooden floors...

> north
Cottage - Bedroom
A cozy bedroom...

> tour end

Tour ended. Returning to Riverside Realty.

> rent cottage

Congratulations! You have rented 'Cozy Cottage'.
50 gold has been deducted. Rent is paid until Day 30.
Use 'enter' to access your new home.
```

### Entering and Using Your Property

```
> enter

You enter your property...

Craig's Cozy Cottage - Living Room
A warm living room with wooden floors...
[Exits: north out]

> drop sword
You drop a sword.

> out

Riverside Realty
The leasing office...
```

Items dropped in property rooms are safe and will persist.

### Managing Access

```
> property

=== Your Property ===

Name: Craig's Cozy Cottage
Rent: 50 gold/month
Paid until: Day 30 (15 days remaining)
Party Access: None
Trusted Visitors: (none)

> property access visit

Party access set to 'Visit Only'.
Grouped players can now enter and look around.

> property trust alice

Alice added to trusted visitors (full access).
```

### Ending a Lease

```
> endlease

=== End Lease ===

Property: Cozy Cottage
Items inside: 5

WARNING: Your items will be moved to escrow.
You will have 30 days to retrieve them for a fee.

Type 'endlease confirm' to proceed.

> endlease confirm

Your lease has been terminated.
5 items have been placed in escrow.
Use 'escrow' to view and retrieve your items.
```

## Access Control

Property owners can control who can enter their property:

### Party Access Levels

| Level | Effect |
|-------|--------|
| `none` | No party access (default) |
| `visit` | Grouped players can enter and look around |
| `full` | Grouped players can use amenities and take items |

### Trusted Visitors

Trusted visitors have full access regardless of party membership:

```
> property trust bob
Bob added to trusted visitors (full access).

> property untrust bob
Bob removed from trusted visitors.
```

## Template Management

### Listing Templates

```
> plist

=== Property Templates ===

cottage      Cozy Cottage        50g/mo   2 rooms   3 active
townhouse    Town House         150g/mo   5 rooms   1 active
mansion      Grand Mansion      500g/mo  10 rooms   0 active
```

### Searching Templates

```
> pfind cottage

Found 1 template matching 'cottage':

  cottage - Cozy Cottage
    Rent: 50 gold/month
    Rooms: 2
    Active leases: 3
```

### Editing an Existing Template

```
> pedit edit cottage

Entering template 'cottage' for editing...

Cozy Cottage - Living Room
A warm living room...
```

### Deleting a Template

```
> pdelete cottage

WARNING: This will delete the template and all its rooms.
Active leases: 3 (will NOT be affected - they have copies)

Type 'pdelete cottage confirm' to proceed.
```

## Related Documentation

- [Property Rental Reference](../reference/property-rental.md) - Technical details and data structures
- [Mobile Editing](mobiles.md) - Creating the leasing agent
- [Item Editing](items.md) - Creating amenities
- [Room Editing](rooms.md) - Customizing template rooms
- [Builder Guide](../builder-guide.md) - Overview of building

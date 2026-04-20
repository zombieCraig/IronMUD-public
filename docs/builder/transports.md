# Transport Editing

This guide covers creating elevators and scheduled transports using IronMUD's Online Creation (OLC) system.

## Overview

The transportation system provides two types of player movement:

- **Elevators** - On-demand vertical movement between floors (responds to button presses)
- **Scheduled Transport** - Buses, trains, ferries, and airships that follow timed routes

Both types use the same underlying system. Players use the `press` command to interact with transports, and dynamic exits appear when vehicles arrive at stops.

## Transport Commands

| Command | Usage | Description |
|---------|-------|-------------|
| `tedit create` | `tedit create <vnum>` | Create a new transport |
| `tedit` | `tedit <vnum>` | Edit existing transport |
| `tlist` | `tlist` | List all transports |
| `tedit delete` | `tedit delete <vnum>` | Delete a transport |

## tedit Subcommands

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `show` | `show` | Display transport configuration |
| `name` | `name <text>` | Set display name |
| `type` | `type <type>` | Set type (elevator, bus, train, ferry, airship) |
| `interior` | `interior <room_vnum>` | Set vehicle interior room |
| `stop add` | `stop add <room_vnum> <name> <exit_dir>` | Add a stop |
| `stop remove` | `stop remove <index>` | Remove a stop |
| `stop list` | `stop list` | List all stops |
| `stop reorder` | `stop reorder <from> <to>` | Reorder stops |
| `schedule` | `schedule ondemand` | Set on-demand mode (elevators) |
| `schedule` | `schedule gametime <freq> <start> <end> <dwell>` | Set scheduled mode |
| `traveltime` | `traveltime <seconds>` | Set travel time between stops |
| `done` | `done` | Save and exit editor |

## Creating an Elevator

Elevators respond instantly when players press the call button.

### 1. Create the Interior Room

First, create the elevator car room:

```
> dig up Hotel Elevator
Created room: Hotel Elevator

> up
Hotel Elevator
An empty room...

> redit desc
> A polished brass elevator car with mirrored walls.
> A panel displays the available floors.
> .
Description saved.
```

### 2. Create the Stops

Create the rooms where the elevator will stop:

```
> dig down Hotel Lobby
> down
Hotel Lobby

> redit desc
> A grand marble lobby with crystal chandeliers.
> An elevator door is set into the north wall.
> .

> dig up Guest Floor
> up
Guest Floor

> redit desc
> A quiet hallway with numbered doors.
> The elevator is to the south.
> .

> dig up Rooftop Bar
> up
Rooftop Bar

> redit desc
> A chic open-air bar with stunning city views.
> .
```

### 3. Create the Transport

```
> tedit create hotel_elevator

Created transport 'hotel_elevator'.
Entering transport editor...

> name Hotel Elevator
Name set to: Hotel Elevator

> type elevator
Type set to: Elevator

> interior hotel_elevator
Interior room set.
```

Note: Use the vnum of the elevator car room you created.

### 4. Add Stops

```
> stop add hotel_lobby Lobby elevator
Added stop: Lobby (exit: elevator)

> stop add guest_floor Guest Rooms elevator
Added stop: Guest Rooms (exit: elevator)

> stop add rooftop_bar Rooftop Bar elevator
Added stop: Rooftop Bar (exit: elevator)

> stop list
Stops:
  [1] Lobby (hotel_lobby) - exit: elevator
  [2] Guest Rooms (guest_floor) - exit: elevator
  [3] Rooftop Bar (rooftop_bar) - exit: elevator
```

The exit direction is the command players use to enter the elevator from that stop.

### 5. Configure Schedule

```
> schedule ondemand
Schedule set to: On-Demand

> traveltime 2
Travel time set to 2 seconds between floors.

> show
=== Transport: hotel_elevator ===
Name: Hotel Elevator
Type: Elevator
Interior: Hotel Elevator (hotel_elevator)
Schedule: On-Demand
Travel Time: 2 seconds

Stops:
  [1] Lobby - exit: elevator
  [2] Guest Rooms - exit: elevator
  [3] Rooftop Bar - exit: elevator

> done
Transport saved.
```

### Player Experience

```
> look
Hotel Lobby
A grand marble lobby with crystal chandeliers.
Exits: [south]

> press button
You press the call button. A soft *ding* sounds as the elevator arrives.
The elevator doors slide open.

> elevator
Hotel Elevator
A polished brass elevator car with mirrored walls.

Floors:
  [1] Lobby (current)
  [2] Guest Rooms
  [3] Rooftop Bar

Exits: [out]

> press 3
The doors slide closed.
You feel the elevator rise smoothly...
*Ding!* The elevator stops. The doors slide open to the Rooftop Bar.

> out
Rooftop Bar
A chic open-air bar with stunning city views.
```

## Creating Scheduled Transport

Scheduled transports (buses, trains, ferries) follow timed routes and only stop at stations when the schedule dictates.

### 1. Create the Vehicle Interior

```
> dig north Eastbound Express - Passenger Car
> north

> redit desc
> A comfortable train car with rows of padded seats facing large windows.
> .
```

### 2. Create the Station Rooms

```
> rgoto town_square
> dig east Central Station Platform
> east

> redit desc
> A busy train platform with passengers waiting.
> A departure sign hangs overhead.
> .

> rgoto market_district
> dig south Market Station
> south

> redit desc
> A platform near the bustling market stalls.
> .

> rgoto harbor
> dig west Harbor Station
> west

> redit desc
> A weathered platform overlooking the docks.
> .
```

### 3. Create the Transport

```
> tedit create eastbound_express

> name Eastbound Express
> type train

> interior eastbound_express_car
Interior room set.
```

### 4. Add Stops

```
> stop add central_station Central Station train
> stop add market_station Market District train
> stop add harbor_station Harbor Town train

> stop list
Stops:
  [1] Central Station - exit: train
  [2] Market District - exit: train
  [3] Harbor Town - exit: train
```

### 5. Configure Schedule

```
> schedule gametime 2 6 23 30
```

Parameters:
- `2` - Departs every 2 game hours
- `6` - First departure at 6 AM game time
- `23` - Last departure at 11 PM game time
- `30` - Waits 30 real seconds at each stop for boarding

```
> traveltime 60
Travel time set to 60 seconds between stops.

> show
=== Transport: eastbound_express ===
Name: Eastbound Express
Type: Train
Interior: Eastbound Express - Passenger Car
Schedule: Every 2 game hours (6:00-23:00), 30s dwell
Travel Time: 60 seconds

Stops:
  [1] Central Station - exit: train
  [2] Market District - exit: train
  [3] Harbor Town - exit: train

> done
Transport saved.
```

### Player Experience

```
> look
Central Station Platform
A busy train platform with passengers waiting.
Exits: [west]

[... time passes, train arrives ...]

The Eastbound Express rumbles into the station and comes to a stop.

> look
Central Station Platform
A busy train platform with passengers waiting.
The Eastbound Express is here, doors open.
Exits: [west] [train]

> train
Eastbound Express - Passenger Car
A comfortable train car with rows of padded seats.
Exits: [out]

Next stops: Market District, Harbor Town, Central Station

[... train departs ...]

The conductor calls out "All aboard! Next stop: Market District!"
The train lurches forward and picks up speed.

[... travel time passes ...]

The train slows to a stop. "Market District! Doors opening."

> out
Market Station
A platform near the bustling market stalls.
```

## Status Signs

Status signs are items that display the current location of a transport. Players use `read` to check where a transport currently is.

### Creating a Status Sign

```
> oedit create departure_sign
Created item 'departure_sign'.

> oedit departure_sign short a departure sign
> oedit departure_sign keywords sign departure board

> oedit departure_sign transport eastbound_express
Transport link set to: Eastbound Express

> drop departure_sign
You drop a departure sign.

> oedit departure_sign flag no_get on
no_get flag set to true.
```

### Player Experience

```
> read sign
You read the departure sign:
  Eastbound Express - Currently at: Market District
```

For elevators:

```
> read indicator
You read the floor indicator:
  Hotel Elevator - Currently at: Floor 3
```

## Transport Types

| Type | Typical Use | Schedule |
|------|-------------|----------|
| `elevator` | Buildings, towers | On-demand |
| `bus` | City routes | Scheduled |
| `train` | Between areas | Scheduled |
| `ferry` | Water crossings | Scheduled (longer travel) |
| `airship` | Long distances | Scheduled (very long travel) |

## Schedule Modes

### On-Demand (Elevators)

```
> schedule ondemand
```

- Responds immediately to `press button`
- Players select destination with `press <number>`
- Brief travel delay based on distance

### Game Time (Scheduled)

```
> schedule gametime <frequency> <start_hour> <end_hour> <dwell_seconds>
```

- `frequency` - Departs every N game hours
- `start_hour` - First departure (0-23)
- `end_hour` - Last departure (0-23)
- `dwell_seconds` - Real seconds waiting at each stop

**Examples:**

City bus (every 2 hours, 6 AM - midnight):
```
> schedule gametime 2 6 23 30
```

Night owl train (hourly, midnight - 6 AM):
```
> schedule gametime 1 0 5 20
```

## NPC Transport Routes

NPCs can be configured to use transports, enabling wandering merchants, commuters, and conductors.

### Configuring an NPC Route

```
> medit create Merchant
> medit merchant transport

=== Transport Route ===
Transport: (none)
Home Stop: (none)
Destination: (none)
Schedule: (none)

> medit merchant transport set eastbound_express
Transport set to: Eastbound Express

> medit merchant transport home 0
Home stop set to: Central Station (index 0)

> medit merchant transport dest 1
Destination set to: Market District (index 1)

> medit merchant transport schedule fixed 8 18
Schedule set: Departs at 8:00, returns at 18:00
```

### NPC Schedule Types

| Schedule | Usage | Description |
|----------|-------|-------------|
| `fixed <depart> <return>` | Commuters | Leave at depart hour, return at return hour |
| `random <chance>` | Wanderers | Chance per hour to travel |
| `permanent` | Conductors | NPC stays on transport permanently |

**Conductor example:**
```
> medit conductor transport schedule permanent
Schedule set: Permanent (stays on transport)
```

The conductor will board the transport and remain there, announcing stops.

## Player Commands

| Command | Where | Description |
|---------|-------|-------------|
| `press button` | At stop | Call elevator to current floor |
| `press <number>` | In elevator | Travel to floor by number |
| `press <name>` | In elevator | Travel to floor by name |
| `read <sign>` | At stop | Check transport's current location |

Note: `press button` only works for on-demand transports (elevators). Scheduled transports must be waited for.

## Route Patterns

### Ping-Pong Route

Stops: A → B → C → B → A → B → ...

The transport reverses direction at each end. This is the default behavior.

### Circular Route

Stops: A → B → C → A → B → ...

The transport loops back to the first stop after the last. Configure by having your last stop connect back to the first stop's area.

## Related Documentation

- [Transportation Design Reference](../transportation-design.md) - Technical details and data structures
- [Room Editing](rooms.md) - Creating station and vehicle rooms
- [Item Editing](items.md) - Creating status signs
- [Mobile Editing](mobiles.md) - Creating NPCs with transport routes
- [Builder Guide](../builder-guide.md) - Overview of building

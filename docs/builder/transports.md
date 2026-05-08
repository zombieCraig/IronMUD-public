# Transport Editing

This guide covers creating elevators and scheduled transports using IronMUD's Online Creation (OLC) system.

## Overview

The transportation system provides two types of player movement:

- **Elevators** - On-demand vertical movement between floors (responds to button presses)
- **Scheduled Transport** - Buses, trains, ferries, and airships that follow timed routes

Both types use the same underlying system. Players use the `press` command to interact with transports, and dynamic exits between stops and the vehicle interior are created and removed as the vehicle arrives at and leaves each stop.

## How tedit Works

`tedit` is a stateless OLC command, just like `redit`, `oedit`, and `medit`. There is **no editor mode** to enter or exit — every change is a one-shot command of the form:

```
tedit <vnum> <subcommand> [args...]
```

There is no `done` or `quit`; changes are saved as you make them.

## Transport Commands

| Command | Description |
|---------|-------------|
| `tedit create <vnum>` | Create a new transport at your current room |
| `tedit <vnum>` | Show transport configuration |
| `tedit <vnum> <subcommand> ...` | Apply a subcommand (see below) |
| `tlist` | List all transports |

## tedit Subcommands

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `show` | `tedit <vnum> show` | Display transport configuration |
| `vnum` | `tedit <vnum> vnum <new_vnum>` | Rename the transport's vnum |
| `name` | `tedit <vnum> name <text>` | Set display name |
| `type` | `tedit <vnum> type <type>` | Set type (elevator, bus, train, ferry, airship) |
| `interior` | `tedit <vnum> interior <room_vnum>` | Set vehicle interior room |
| `traveltime` | `tedit <vnum> traveltime <seconds>` | Set travel time between stops |
| `schedule` | `tedit <vnum> schedule ondemand` | Set on-demand mode (elevators) |
| `schedule` | `tedit <vnum> schedule gametime <freq> <start> <end> <dwell>` | Set scheduled mode |
| `stop add` | `tedit <vnum> stop add <room_vnum> <name> <dir>` | Add a stop |
| `stop remove` | `tedit <vnum> stop remove <index>` | Remove a stop (1-based index) |
| `stop list` | `tedit <vnum> stop list` | List all stops |
| `connect` | `tedit <vnum> connect` | Park the vehicle at its current stop (open the doors) |
| `disconnect` | `tedit <vnum> disconnect` | Close the doors / leave the current stop |
| `delete` | `tedit <vnum> delete` | Delete the transport prototype |

## Stop Direction Semantics

When you add a stop with `stop add <room> <name> <dir>`, the direction is **the direction a player walks in the stop room to board the vehicle** — not the direction they take to disembark.

For example:

```
tedit hotel_elevator stop add hotel_lobby Lobby n
```

means: in *Hotel Lobby*, the `north` exit leads into the elevator car. The opposite direction (`south`) is automatically wired on the interior side as the way back out, so the player rides down by stepping `south` from inside the car.

Valid directions: `n`, `s`, `e`, `w`, `u`, `d` (or the long forms `north`, `south`, `east`, `west`, `up`, `down`).

Note: the exit only exists while the transport is **parked** at that stop. When the elevator moves to another floor or a train pulls out of a station, the direction at the previous stop becomes a dead end again until the vehicle returns.

## Important: Don't Use `dig` for Transport Rooms

The transport system creates and removes the exits between stops and the vehicle interior **dynamically** as the vehicle arrives and departs. If you use `dig` to make these rooms, you will leave behind a permanent two-way exit that the transport system can't manage — the elevator will appear to be at that floor *forever*, even after a player rides it somewhere else.

Use `redit create <title>` to make detached rooms with no fixed exits. The transport will wire up the connections itself via `connect` and the schedule tick.

For the surrounding world rooms (lobby, station platform, etc.) that *aren't* the vehicle interior or stop room itself, `dig` is fine — just don't use it to link a stop room to the interior, or to link interior to interior.

## Creating an Elevator

Elevators respond instantly when players press the call button.

### 1. Create the Stop Rooms

Move to where the elevator's lowest floor is, then create each floor's room as a detached room. (Use `redit create`, not `dig`.)

```
> redit create Hotel Lobby
=== Created Detached Room ===
Vnum: hotel_lobby
...

> rgoto hotel_lobby
> redit desc
A grand marble lobby with crystal chandeliers.
An elevator door is set into the north wall.
.

> redit create Guest Floor
> rgoto guest_floor
> redit desc
A quiet hallway with numbered doors.
The elevator is to the south.
.

> redit create Rooftop Bar
> rgoto rooftop_bar
> redit desc
A chic open-air bar with stunning city views.
.
```

These rooms can be linked to the rest of the world however you like (via `redit exit`, `link`, or `dig` *between* the lobby and the street, for instance) — just don't dig between them and the elevator car.

### 2. Create the Elevator Interior

```
> redit create Hotel Elevator
> rgoto hotel_elevator
> redit desc
A polished brass elevator car with mirrored walls.
A panel displays the available floors.
.
```

### 3. Create the Transport

```
> tedit create hotel_elevator
Created transport 'hotel_elevator'.

> tedit hotel_elevator name Hotel Elevator
Name set to: Hotel Elevator

> tedit hotel_elevator type elevator
Type set to: elevator

> tedit hotel_elevator interior hotel_elevator
Interior room set to: Hotel Elevator
```

(The transport vnum and interior room vnum can match — `tedit` and `redit` keep separate registries.)

### 4. Add Stops

```
> tedit hotel_elevator stop add hotel_lobby Lobby n
Added stop: Lobby

> tedit hotel_elevator stop add guest_floor Guest_Rooms n
Added stop: Guest_Rooms

> tedit hotel_elevator stop add rooftop_bar Rooftop_Bar n
Added stop: Rooftop_Bar

> tedit hotel_elevator stop list
Stops:
  [1] Lobby - exit: north
      Room: Hotel Lobby
  [2] Guest_Rooms - exit: north
      Room: Guest Floor
  [3] Rooftop_Bar - exit: north
      Room: Rooftop Bar
```

The `north` here means "from the lobby, walk north to step into the elevator." The stop reads "exit: north" because that's the direction of the door in the stop room.

### 5. Configure Schedule

```
> tedit hotel_elevator schedule ondemand
Schedule set to: On-Demand

> tedit hotel_elevator traveltime 2
Travel time set to: 2 seconds
```

### 6. Park the Elevator

A freshly created transport is not yet connected to any stop — the doors are closed everywhere. Park it at its initial stop so players can board:

```
> tedit hotel_elevator connect
Connected to stop: Lobby
```

Now the lobby has a `north` exit into the elevator car, and the car has a `south` exit back to the lobby. Pressing buttons inside the car will move the elevator to other floors automatically (rewiring the exits each time).

### Player Experience

```
> look
Hotel Lobby
A grand marble lobby with crystal chandeliers.
Exits: [south] [north]

> press button
You press the call button. A soft *ding* sounds as the elevator arrives.

> north
Hotel Elevator
A polished brass elevator car with mirrored walls.
Exits: [south]

> press 3
The doors slide closed.
You feel the elevator rise smoothly...
*Ding!* The elevator stops. The doors slide open to the Rooftop Bar.

> south
Rooftop Bar
A chic open-air bar with stunning city views.
```

## Creating Scheduled Transport

Scheduled transports (buses, trains, ferries) follow timed routes and only stop at stations when the schedule dictates.

### 1. Create the Vehicle Interior

```
> redit create Eastbound Express - Passenger Car
> rgoto eastbound_express_car
> redit desc
A comfortable train car with rows of padded seats facing large windows.
.
```

### 2. Create the Station Rooms

Make each platform as a detached room (or `rgoto` an existing one). Don't dig into the train car from the platforms.

```
> redit create Central Station Platform
> rgoto central_station
> redit desc
A busy train platform with passengers waiting.
.

> redit create Market Station
> rgoto market_station
> redit desc
A platform near the bustling market stalls.
.

> redit create Harbor Station
> rgoto harbor_station
> redit desc
A weathered platform overlooking the docks.
.
```

(Connect each platform to the rest of the world with `redit exit` or `dig` as you would any other room.)

### 3. Create the Transport

```
> tedit create eastbound_express
> tedit eastbound_express name Eastbound Express
> tedit eastbound_express type train
> tedit eastbound_express interior eastbound_express_car
Interior room set to: Eastbound Express - Passenger Car
```

### 4. Add Stops

```
> tedit eastbound_express stop add central_station Central_Station e
> tedit eastbound_express stop add market_station Market_District e
> tedit eastbound_express stop add harbor_station Harbor_Town e

> tedit eastbound_express stop list
Stops:
  [1] Central_Station - exit: east
  [2] Market_District - exit: east
  [3] Harbor_Town - exit: east
```

Here, players board by walking `east` at each platform. They disembark by walking `west` from inside the car.

### 5. Configure Schedule

```
> tedit eastbound_express schedule gametime 2 6 23 30
Schedule set to: every 2h, 6:00-23:00
```

Parameters:
- `2` - Departs every 2 game hours
- `6` - First departure at 6 AM game time
- `23` - Last departure at 11 PM game time
- `30` - Waits 30 real seconds at each stop for boarding

```
> tedit eastbound_express traveltime 60
Travel time set to: 60 seconds

> tedit eastbound_express show
=== Transport Editor ===
Vnum: eastbound_express
Name: Eastbound Express
Type: train
Interior Room: ... (Eastbound Express - Passenger Car)
Travel Time: 60 seconds
Schedule: Game-Time scheduled
...
```

The schedule tick will park the train at its first stop on its first scheduled departure — you usually don't need to call `connect` manually for scheduled transports unless you're testing.

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
Exits: [west] [east]

> east
Eastbound Express - Passenger Car
A comfortable train car with rows of padded seats.
Exits: [west]

[... train departs ...]

The train lurches forward and picks up speed.

[... travel time passes ...]

The train slows to a stop at Market District.

> west
Market Station
A platform near the bustling market stalls.
```

## Connect, Disconnect, and Delete

These three subcommands manage the live state of an existing transport prototype.

### `connect`

`tedit <vnum> connect` parks the vehicle at its **current stop** (the last stop it visited, or stop 1 if it's never moved). It writes the dynamic exits in both directions:

- The stop room gets an exit in the configured direction leading into the vehicle interior.
- The interior gets an exit in the opposite direction leading back to the stop room.

Use it when:
- You've just created a transport and want it to start parked at its first stop so players can board immediately (especially elevators, which won't move until pressed).
- You're testing and want to manually park the vehicle for inspection.
- A scheduled transport got into a weird state (rare) and you want to force it back onto a stop.

For scheduled transports in normal operation you don't need to use `connect` — the schedule tick will park and depart the vehicle automatically.

### `disconnect`

`tedit <vnum> disconnect` removes the dynamic exits at the current stop, "closing the doors." The transport is no longer reachable from any room until it next connects.

Use it when:
- You're rearranging stops or rebuilding a route and want the vehicle out of the way.
- You're about to `delete` the transport (the `delete` handler does this automatically, but it's safe to call first).

### `delete`

`tedit <vnum> delete` permanently removes the transport prototype. It first disconnects from any current stop so no orphaned exits are left behind.

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
tedit <vnum> schedule ondemand
```

- Responds immediately to `press button`
- Players select destination with `press <number>`
- Brief travel delay based on `traveltime`

### Game Time (Scheduled)

```
tedit <vnum> schedule gametime <frequency> <start_hour> <end_hour> <dwell_seconds>
```

- `frequency` - Departs every N game hours
- `start_hour` - First departure (0-23)
- `end_hour` - Last departure (0-23)
- `dwell_seconds` - Real seconds waiting at each stop

**Examples:**

City bus (every 2 hours, 6 AM - midnight):
```
tedit citybus schedule gametime 2 6 23 30
```

Night owl train (hourly, midnight - 6 AM):
```
tedit nighttrain schedule gametime 1 0 5 20
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

# Transportation System Design for IronMUD

## Implementation Status

| Phase | Description | Status |
|-------|-------------|--------|
| Phase 1 | Core Infrastructure | **COMPLETE** |
| Phase 2 | Elevator Support | **COMPLETE** |
| Phase 3 | Scheduled Transport | **COMPLETE** |
| Phase 4 | Editor and Signs | **COMPLETE** |
| Phase 5 | NPC Support | **COMPLETE** |
| Phase 6 | Polish | **PARTIAL** |

### Phase 6 Details
- Tab completion for transport commands: **COMPLETE**
- Conductor NPC dialogue integration: Not implemented (optional)
- Ticket/fare system: Not implemented (optional)
- Weather effects on schedules: Not implemented (optional)
- Area-specific transport listings: Not implemented (optional)

### Implementation Notes
- NPCTravelSchedule includes an additional `Permanent` variant for NPCs that stay on the transport (conductors)
- Transport info is displayed via `read` command (for signs with transport_link) and `look` command (inside vehicle interiors)
- The `schedule` command was not implemented; use `read` and `look` instead

---

## Executive Summary

This document proposes a unified transportation system for IronMUD covering:
1. **Elevators** - Vertical movement between floors in buildings
2. **Scheduled Transport** - Buses, trains, ferries between areas/regions

Both use the same underlying `TransportData` structure with different scheduling modes.

### Key Design Principles

- **Single `press` command** - `press button` to call transport, `press <number>` to select destination
- **Dynamic exits** - Exits appear when transport arrives, disappear when it departs
- **No explicit board/leave** - Players use normal directional exits
- **TransportData-centric** - All stops stored on transport, not flagged on rooms
- **Status signs** - Items that link to TransportData and display current location

---

## Player Experience

### Elevator Example

```
> look
Hotel Lobby
A grand marble lobby with crystal chandeliers. An elevator door is set
into the north wall. A small call button glows beside it.
Exits: [south] [elevator]

> press button
You press the call button. A soft *ding* sounds as the elevator arrives.
The elevator doors slide open.

> elevator
Hotel Elevator
A polished brass elevator car with mirrored walls. A panel displays
the available floors.

Floors:
  [1] Lobby (current)
  [2] Guest Rooms
  [3] Restaurant
  [4] Rooftop Bar

Exits: [out]

> press 4
The doors slide closed.
You feel the elevator rise smoothly...
*Ding!* The elevator stops. The doors slide open to the Rooftop Bar.

> out
Rooftop Bar
A chic open-air bar with stunning city views...
```

### Train Example

```
> look
Central Station Platform
A busy train platform with passengers waiting. A departure sign hangs overhead.
Exits: [north] [south]

> examine sign
The departure sign reads:
  Eastbound Express - Currently at: Market District

[... time passes, train arrives ...]
The Eastbound Express rumbles into the station and comes to a stop.

> look
Central Station Platform
A busy train platform with passengers waiting. A departure sign hangs overhead.
The Eastbound Express is here, doors open.
Exits: [north] [south] [train]

> train
Eastbound Express - Passenger Car
A comfortable train car with rows of padded seats facing large windows.
Exits: [out]

Next stops: Harbor Town, Eastern Village, Central Station

[... time passes, train departs ...]
The conductor calls out "All aboard! Next stop: Harbor Town!"
The train lurches forward and picks up speed.
Exits: (none while moving)

[... time passes ...]
The train slows to a stop. "Harbor Town! Doors opening."
Exits: [out]

> out
Harbor Town Station
...
```

---

## Core Data Structure: TransportData

All transportation uses a single unified structure:

```rust
pub struct TransportData {
    pub id: Uuid,
    pub vnum: Option<String>,           // e.g., "hotel:elevator", "eastbound:express"
    pub name: String,                    // "Hotel Elevator", "Eastbound Express"
    pub transport_type: TransportType,   // Elevator, Bus, Train, Ferry, Airship
    pub interior_room_id: Uuid,          // The vehicle/car room
    pub stops: Vec<TransportStop>,       // Ordered list of stops
    pub current_stop_index: usize,       // Which stop we're at (or heading to)
    pub state: TransportState,           // Stopped, Moving
    pub direction: i8,                   // 1 = forward, -1 = reverse (ping-pong routes)
    pub schedule: TransportSchedule,     // How it operates
    pub travel_time_secs: i64,           // Base time between stops
    pub last_state_change: i64,          // Unix timestamp
}

pub struct TransportStop {
    pub room_id: Uuid,                   // The station/floor room
    pub name: String,                    // "Lobby", "Floor 3", "Market District"
    pub exit_direction: String,          // Direction from stop to vehicle ("elevator", "train", "east")
}

pub enum TransportType {
    Elevator,    // On-demand, instant response to calls
    Bus,         // Scheduled, stops at each station
    Train,       // Scheduled, stops at each station
    Ferry,       // Scheduled, longer travel times
    Airship,     // Scheduled, very long travel times
}

pub enum TransportState {
    Stopped,     // At a station, exits available
    Moving,      // In transit, no exits available
}

pub enum TransportSchedule {
    OnDemand,    // Responds to `press button` calls (elevators)
    GameTime {
        frequency_hours: i32,     // Departs every N game hours
        operating_start: u8,      // First departure hour (e.g., 6 for 6 AM)
        operating_end: u8,        // Last departure hour (e.g., 23 for 11 PM)
        dwell_time_secs: i64,     // How long to wait at each stop for boarding
    },
}
```

### Schedule Examples

**Elevator (On-Demand):**
```rust
TransportSchedule::OnDemand
```

**City Bus (Every 2 game hours, 6 AM - midnight):**
```rust
TransportSchedule::GameTime {
    frequency_hours: 2,
    operating_start: 6,
    operating_end: 23,
    dwell_time_secs: 30,  // 30 real seconds at each stop
}
// Departures at: 6, 8, 10, 12, 14, 16, 18, 20, 22 game hours
// 1 game hour = 2 real minutes, so train every 4 real minutes when operating
```

**Night Owl Train (Midnight - 6 AM only):**
```rust
TransportSchedule::GameTime {
    frequency_hours: 1,
    operating_start: 0,
    operating_end: 5,
    dwell_time_secs: 20,
}
```

---

## The `press` Command

A unified command for all transport interactions:

### Usage

```
press button              - Call transport to current location (at a stop)
press <number>            - Select destination by number (inside vehicle)
press <name>              - Select destination by name (inside vehicle)
```

### Behavior at a Stop (Outside Vehicle)

When player is at a room that's a stop for a transport:

1. Check if transport exists for this room (lookup by room_id in any TransportData.stops)
2. If transport is OnDemand (elevator):
   - Move transport to this stop immediately
   - Broadcast arrival message to room (awake players)
   - Create exit from stop room to vehicle interior
   - Create exit from vehicle interior to stop room
3. If transport is GameTime (bus/train):
   - `press button` does NOT work - message: "There's no button here."
   - Players must wait for vehicle to arrive on schedule
   - Use status sign item to check current location

### Behavior Inside Vehicle

When player is inside a transport vehicle room:

1. Look up TransportData by interior_room_id
2. Display numbered list of stops with names
3. If player presses a number/name:
   - For OnDemand: Execute travel immediately with delay
   - For GameTime: Show error "This transport operates on a schedule"

### Elevator Travel Sequence

```
1. Player types: press 3
2. Validate: Is this a valid stop number?
3. If same floor: "You're already at Floor 3."
4. Remove exit from current stop room
5. Remove exit from vehicle to current stop
6. Display: "The doors slide closed."
7. Calculate delay: 1 + (floor_distance * 0.5) seconds
8. Display travel message based on direction
9. Update current_stop_index
10. Create exit from new stop room to vehicle
11. Create exit from vehicle to new stop room
12. Display: "*Ding!* The elevator stops. The doors slide open to [Stop Name]."
13. Broadcast arrival to new stop room (awake players)
```

---

## Status Sign Item

Transport signs work similarly to existing readable items (like recipe books that use `teaches_recipe`). The existing `read` command shows `long_desc` for readable items - signs extend this pattern with real-time data.

### Item Properties

Add to ItemData:
```rust
pub transport_link: Option<Uuid>,  // Links to TransportData.id
```

### Integration with Existing `read` Command

The `read.rhai` command already handles readable items by showing `long_desc`. Extend it to check for `transport_link`:

```rhai
// In read.rhai, after finding the item:
let transport_id = get_item_transport_link(item.id);
if transport_id != () {
    let transport = get_transport_data(transport_id);
    if transport != () {
        let current_stop = transport.stops[transport.current_stop_index];
        send_client_message(connection_id, "You read " + item.short_desc + ":");
        send_client_message(connection_id, "  " + transport.name + " - Currently at: " + current_stop.name);
        return;
    }
}
// Fall through to existing long_desc display for regular readable items
```

### Display Behavior

When read (`read sign`):
```
You read the departure sign:
  Eastbound Express - Currently at: Market District
```

For elevators:
```
You read the floor indicator:
  Hotel Elevator - Currently at: Floor 3
```

### No New Item Flag Needed

Since `transport_link` is already sufficient to identify transport signs, no separate flag is required. An item with `transport_link` set is automatically a transport sign.

---

## Room Description Integration

### Tracking Transport Presence at Stops

Two options for indicating that a transport is currently at a stop:

**Option A: Use Existing `dynamic_desc` Field (Recommended)**

The `dynamic_desc` field on RoomData already supports temporary room description additions. When transport arrives/departs:

```rhai
// On arrival:
set_room_dynamic_desc(stop_room_id, "The elevator doors stand open to the east.");

// On departure:
clear_room_dynamic_desc(stop_room_id);
```

Pros: No schema changes, uses existing infrastructure
Cons: Conflicts if room has other dynamic content (weather events, etc.)

**Option B: Add New `transport_presence` Field**

Add to RoomData:
```rust
#[serde(default)]
pub transport_presence: Option<Uuid>,  // TransportData.id if vehicle is here
```

Then in `look.rhai`, check this field and append transport info.

Pros: Dedicated field, no conflicts
Cons: Schema change required

**Recommendation:** Start with Option A (`dynamic_desc`). If conflicts become an issue, add the dedicated field later.

### Dynamic Exit Display

The `look` command shows transport exits only when vehicle is present (exit exists):

**When elevator is at this floor (exit exists):**
```
Hotel Lobby
A grand marble lobby.
The elevator doors stand open to the east.
Exits: [south] [elevator]
```

**When elevator is elsewhere (no exit):**
```
Hotel Lobby
A grand marble lobby.
Exits: [south]
```

Note: The exit itself (`elevator`) only exists when the transport is present. The `dynamic_desc` provides the narrative flavor.

### Vehicle Interior Description

The vehicle room should show:
- Current location (if stopped)
- Next stops (for scheduled transport)
- Available floor list (for elevators)

This can be done via:
1. Dynamic room description triggered on look (use `on_look` trigger)
2. Modify `look.rhai` to check if room is a transport interior and append info

**Recommended approach:** Add an `on_look` trigger to vehicle rooms that queries TransportData and displays:
- For elevators: Floor list with current floor marked
- For scheduled: Next stops in route order

---

## Exit Management

### Key Functions Needed

```rust
fn connect_transport_to_stop(transport_id: Uuid, stop_index: usize)
fn disconnect_transport_from_stop(transport_id: Uuid, stop_index: usize)
```

### Connect Logic

1. Get TransportData by id
2. Get stop at stop_index
3. Set exit from stop.room_id in direction stop.exit_direction -> transport.interior_room_id
4. Set exit from transport.interior_room_id direction "out" -> stop.room_id
5. Update transport.current_stop_index
6. Set transport.state = Stopped

### Disconnect Logic

1. Get TransportData by id
2. Get stop at current_stop_index
3. Clear exit from stop.room_id in direction stop.exit_direction
4. Clear exit from transport.interior_room_id direction "out"
5. Set transport.state = Moving

---

## Background Transport Tick

For scheduled transports (GameTime):

```rust
async fn run_transport_tick(state: SharedState) {
    loop {
        tokio::time::sleep(Duration::from_secs(10)).await;
        process_scheduled_transports(&state).await;
    }
}

fn process_scheduled_transports(state: &SharedState) {
    let game_time = get_game_time();

    for transport in get_all_transports() {
        if let TransportSchedule::GameTime { .. } = transport.schedule {
            match transport.state {
                TransportState::Stopped => {
                    // Check if dwell time elapsed and within operating hours
                    if should_depart(&transport, &game_time) {
                        // Broadcast departure message
                        // Disconnect from current stop
                        // Set state to Moving
                        // Calculate arrival time
                    }
                }
                TransportState::Moving => {
                    // Check if travel time elapsed
                    if should_arrive(&transport) {
                        // Advance to next stop (handle direction reversal)
                        // Connect to new stop
                        // Broadcast arrival message
                        // Set state to Stopped
                    }
                }
            }
        }
    }
}
```

### Operating Hours Check

```rust
fn is_within_operating_hours(schedule: &TransportSchedule, hour: u8) -> bool {
    if let TransportSchedule::GameTime { operating_start, operating_end, .. } = schedule {
        if operating_start <= operating_end {
            // Normal range: 6 AM to 11 PM
            hour >= operating_start && hour <= operating_end
        } else {
            // Overnight range: 11 PM to 6 AM
            hour >= operating_start || hour <= operating_end
        }
    } else {
        true // OnDemand always operates
    }
}
```

---

## NPC Transportation

NPCs can use transportation with an optional route configuration:

### MobileData Addition

```rust
pub transport_route: Option<TransportRoute>,
```

```rust
pub struct TransportRoute {
    pub transport_id: Uuid,         // Which transport to use
    pub home_stop_index: usize,     // Where NPC "lives"
    pub destination_stop_index: usize, // Where NPC travels to
    pub travel_schedule: NPCTravelSchedule,
}

pub enum NPCTravelSchedule {
    FixedHours { depart_hour: u8, return_hour: u8 },
    Random { chance_per_hour: i32 },
    Permanent,  // NPC stays on transport permanently (conductors)
}
```

### Use Cases

- **Conductor NPC**: Stays on vehicle, announces stops
- **Wandering Merchant**: Travels between market stops on schedule
- **Commuter NPC**: Goes to work in morning, returns home in evening

---

## Transport Editor (tedit)

Admin command for creating and editing transports:

```
tedit <vnum>              - Edit existing transport
tedit create <vnum>       - Create new transport
tedit list                - List all transports
tedit delete <vnum>       - Delete transport

Inside editor:
  name <name>             - Set display name
  type <elevator|bus|train|ferry|airship>
  interior <room_vnum>    - Set vehicle room
  stop add <room_vnum> <name> <exit_direction>
  stop remove <index>
  stop list
  stop reorder <from> <to>
  schedule ondemand
  schedule gametime <frequency> <start_hour> <end_hour> <dwell_secs>
  traveltime <seconds>    - Time between stops
  show                    - Display current config
  done                    - Save and exit
```

---

## Item Editor Addition (oedit)

Add transport sign support:

```
oedit <item_vnum>

Inside editor:
  transport <transport_vnum>  - Link to transport
```

---

## Implementation Phases

### Phase 1: Core Infrastructure - COMPLETE

1. ~~Add `TransportData`, `TransportStop`, `TransportType`, `TransportState`, `TransportSchedule` to lib.rs~~
2. ~~Add `transports: HashMap<Uuid, TransportData>` to World state~~
3. ~~Add database persistence for transports~~
4. ~~Create `src/script/transport.rs` with basic Rhai functions~~
5. ~~Register transport functions in script/mod.rs~~

### Phase 2: Elevator Support - COMPLETE

1. ~~Implement `press` command for on-demand transport~~
2. ~~Implement exit connect/disconnect functions~~
3. ~~Implement travel delay mechanism~~
4. ~~Add transport vnum display in vehicle rooms~~
5. ~~Test with simple 3-floor elevator~~

### Phase 3: Scheduled Transport - COMPLETE

1. ~~Add `run_transport_tick()` background task~~
2. ~~Implement GameTime schedule processing~~
3. ~~Implement operating hours check~~
4. ~~Implement route direction handling (ping-pong vs circular)~~
5. ~~Add departure/arrival broadcasts~~
6. ~~Test with simple 3-stop bus route~~

### Phase 4: Editor and Signs - COMPLETE

1. ~~Create `tedit.rhai` transport editor~~
2. ~~Add `transport_link` to ItemData~~
3. ~~Update `read.rhai` to check for transport_link and show status~~
4. ~~Update `oedit.rhai` for transport_link property~~

### Phase 5: NPC Support - COMPLETE

1. ~~Add `transport_route` to MobileData~~
2. ~~Implement NPC boarding/disembarking logic~~
3. ~~Add NPCTravelSchedule processing (including Permanent variant)~~
4. ~~Update `medit.rhai` for route configuration~~

### Phase 6: Polish - PARTIAL

1. Conductor NPC dialogue integration (not implemented - optional)
2. Ticket/fare system (not implemented - optional)
3. ~~Tab completion for transport commands~~
4. Weather effects on schedules (not implemented - optional)
5. Area-specific transport listings (not implemented - optional)

---

## Files to Modify/Create

### Rust Files

| File | Changes |
|------|---------|
| `src/lib.rs` | Add TransportData, TransportStop, TransportType, TransportState, TransportSchedule |
| `src/lib.rs` | Add transport_link to ItemData |
| `src/lib.rs` | Add transport_route to MobileData (Phase 5) |
| `src/script/mod.rs` | Register transport module |
| `src/script/transport.rs` | New file: all transport Rhai functions |
| `src/script/items.rs` | Add transport_link getter/setter |
| `src/main.rs` | Add run_transport_tick() background task |
| `src/db.rs` | Add transport persistence |

### Script Files

| File | Purpose |
|------|---------|
| `scripts/commands/press.rhai` | Unified press command |
| `scripts/commands/tedit.rhai` | Transport editor |
| `scripts/commands/tlist.rhai` | List all transports |

### Modified Scripts

| File | Changes |
|------|---------|
| `scripts/commands/look.rhai` | Show transport status in vehicle rooms (or use on_look trigger) |
| `scripts/commands/read.rhai` | Show status for items with transport_link |
| `scripts/commands/oedit.rhai` | Add transport_link property support |
| `scripts/commands/medit.rhai` | Add transport_route support (Phase 5) |

---

## Verification Plan

### Elevator Testing

1. Create test building: Lobby + 3 floors + elevator shaft
2. Configure elevator with 4 stops via tedit
3. Test `press button` from each floor
4. Test `press <number>` inside elevator
5. Verify exits appear/disappear correctly
6. Test travel delay and messages
7. Test with two players (one in elevator, one on floor)
8. Verify broadcast messages to awake players only

### Bus/Train Testing

1. Create test route: Station A, B, C
2. Configure bus with GameTime schedule via tedit
3. Verify bus moves on schedule
4. Test boarding at station (exit appears when bus arrives)
5. Test riding through multiple stops
6. Test operating hours (no service overnight)
7. Test direction reversal (A->B->C->B->A pattern)
8. Test circular route (A->B->C->A pattern)

### Sign Testing

1. Create sign item with transport_link pointing to test elevator
2. Verify `read sign` shows current floor
3. Move elevator, verify sign updates on next read
4. Test with bus (shows current station)

### NPC Testing

1. Create merchant NPC with transport_route
2. Configure to travel between two markets
3. Verify NPC boards transport at scheduled time
4. Verify NPC disembarks at destination
5. Test with conductor NPC (stays on vehicle)

---

## Design Decisions Summary

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Interaction command | `press` | Generic, reusable for buttons, levers, etc. |
| Button mechanism | Command-based | `press button` works at any on-demand stop, no item required |
| Scheduled stops | No button | `press button` doesn't work; players wait for arrival |
| Entry/exit | Dynamic exits | Simpler, uses existing movement system |
| Stop storage | On TransportData | Centralized, no room flags needed |
| Elevator timing | Brief delay (1-3s) | Immersive without being tedious |
| Bus/train schedule | Game-time based | Allows overnight shutdown, feels realistic |
| NPC support | Yes | Enables wandering merchants, conductors |
| Status signs | `read` + transport_link | Extends existing readable item pattern, no new flag |
| Room presence | Use `dynamic_desc` | Existing field, no schema change needed |

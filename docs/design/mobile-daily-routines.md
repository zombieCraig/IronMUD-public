# Mobile Daily Routine System Design for IronMUD

## Executive Summary

This document proposes a daily routine system for mobile NPCs, allowing them to follow time-based schedules throughout the game day. Mobiles can move between locations, change activity states (working, sleeping, patrolling, etc.), and have their behavior modified based on their current routine entry.

### Key Design Principles

- **Game-hour driven** - Routines use game hours 0-23 (2 real minutes = 1 game hour)
- **Step movement** - NPCs walk one room at a time toward destinations via BFS pathfinding
- **Activity states affect behavior** - Shop availability, dialogue, wander suppression all respond to current state
- **Coexists with transport_route** - Separate optional systems, both can exist on the same mobile
- **Builder presets** - Generic JSON templates with placeholder substitution for common patterns
- **Player-visible schedules** - Optional `schedule` command per NPC plus passive dialogue hints

---

## Player Experience

### Shopkeeper with Business Hours

```
> look
Market Square
A bustling open-air market. Vendors line the cobblestone square.
Old Gregor the blacksmith is here, hammering at his anvil.
Exits: [north] [south] [east] [west]

> buy sword
Old Gregor shows you his wares...

[... evening arrives ...]
Old Gregor wipes his brow. "Closing up shop for the day."
Old Gregor leaves heading west.

> buy sword
Old Gregor isn't here.
```

### Guard Shift Change

```
> look
Castle Gate
The massive iron portcullis looms overhead. Guards stand watch.
Captain Aldric stands here, watching the road with sharp eyes.
Exits: [north] [south]

[... dusk arrives ...]
Captain Aldric says "Night watch, you're up. Keep alert."
Captain Aldric leaves heading north.
Sergeant Mira arrives from the south.
Sergeant Mira takes up her post, scanning the darkness.
```

### NPC Going Home at Night

```
> look
Town Square
The heart of the village. A fountain gurgles in the center.
Martha the herbalist is here, sorting dried herbs.
Exits: [north] [south] [east] [west]

[... night falls ...]
Martha the herbalist gathers her things. "Time to head home."
Martha the herbalist leaves heading east.

[... next morning ...]
Martha the herbalist arrives from the east.
Martha the herbalist begins setting out her wares.
```

### Checking an NPC's Schedule

```
> schedule gregor
Old Gregor's daily routine:
  Dawn (5:00)  - Opens his forge at the Market Square
  Dusk (17:00) - Heads home for the evening
  Night (22:00) - Sleeping

> schedule aldric
Captain Aldric doesn't share his schedule with you.
```

---

## Core Data Structures

### ActivityState Enum

Added to `src/types/mod.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityState {
    Working,
    Sleeping,
    Patrolling,
    OffDuty,
    Socializing,
    Eating,
    Custom(String),
}

impl Default for ActivityState {
    fn default() -> Self {
        ActivityState::Working
    }
}
```

### RoutineEntry

A single time slot in a mobile's daily schedule:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineEntry {
    /// Game hour this entry starts (0-23)
    pub start_hour: u8,
    /// Activity state during this period
    pub activity: ActivityState,
    /// Target room vnum (mobile walks here when this entry activates)
    #[serde(default)]
    pub destination_vnum: Option<String>,
    /// Message broadcast when this entry activates (e.g., "opens his shop")
    #[serde(default)]
    pub transition_message: Option<String>,
    /// Suppress random wandering during this entry
    #[serde(default)]
    pub suppress_wander: bool,
    /// Dialogue overrides for this period (keyword -> response)
    #[serde(default)]
    pub dialogue_overrides: HashMap<String, String>,
}
```

### DailyRoutine (on MobileData)

```rust
// Added to MobileData struct:

/// Daily routine entries (sorted by start_hour)
#[serde(default)]
pub daily_routine: Vec<RoutineEntry>,
/// Whether players can see this NPC's schedule via the `schedule` command
#[serde(default)]
pub schedule_visible: bool,
/// Current activity state (runtime, derived from routine + game hour)
#[serde(default)]
pub current_activity: ActivityState,
/// Room the mobile is currently walking toward (set by routine tick)
#[serde(default)]
pub routine_destination_room: Option<Uuid>,
```

### Relationship to Existing Fields

| Existing Field | Interaction |
|----------------|-------------|
| `flags.sentinel` | Sentinel overrides routine movement - mobile stays put but activity state still changes |
| `flags.shopkeeper` | Shop is only open when `current_activity == Working` |
| `flags.healer` | Healer only available when `current_activity == Working` |
| `transport_route` | Independent system - transport_route handles riding vehicles, routine handles walking between rooms |
| `dialogue` | `RoutineEntry.dialogue_overrides` supplements base dialogue during that time period |

---

## Routine Tick

New file: `src/ticks/routine.rs`

### Tick Design

```rust
/// Routine tick interval - once per game hour (120 real seconds)
pub const ROUTINE_TICK_INTERVAL_SECS: u64 = 120;

pub async fn run_routine_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(ROUTINE_TICK_INTERVAL_SECS));
    loop {
        ticker.tick().await;
        if let Err(e) = process_routine_tick(&db, &connections) {
            error!("Routine tick error: {}", e);
        }
    }
}
```

### Processing Logic

```rust
fn process_routine_tick(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    let game_time = db.get_game_time()?;
    let current_hour = game_time.hour;
    let mobiles = db.list_all_mobiles()?;

    for mobile in mobiles {
        // Skip prototypes and dead mobiles
        if mobile.is_prototype || mobile.current_hp <= 0 {
            continue;
        }

        // Skip mobiles without routines
        if mobile.daily_routine.is_empty() {
            continue;
        }

        // Skip mobiles in combat
        if mobile.combat.in_combat {
            continue;
        }

        // Find the active routine entry for this hour
        let active_entry = find_active_entry(&mobile.daily_routine, current_hour);

        if let Some(entry) = active_entry {
            let mut mobile = db.get_mobile_data(&mobile.id)?
                .ok_or_else(|| anyhow::anyhow!("Mobile not found"))?;

            let old_activity = mobile.current_activity.clone();
            let new_activity = entry.activity.clone();

            // Activity state changed - broadcast transition
            if old_activity != new_activity {
                mobile.current_activity = new_activity;

                // Broadcast transition message if set
                if let Some(ref msg) = entry.transition_message {
                    if let Some(room_id) = mobile.current_room_id {
                        let full_msg = format!("{} {}\n", mobile.name, msg);
                        broadcast_to_room_awake(connections, &room_id, &full_msg);
                    }
                }
            }

            // Set movement destination if entry specifies one
            if let Some(ref dest_vnum) = entry.destination_vnum {
                if let Some(dest_room) = db.get_room_by_vnum(dest_vnum)? {
                    // Only set destination if not already there
                    if mobile.current_room_id != Some(dest_room.id) {
                        mobile.routine_destination_room = Some(dest_room.id);
                    } else {
                        mobile.routine_destination_room = None;
                    }
                }
            }

            db.save_mobile_data(mobile)?;
        }
    }
    Ok(())
}

/// Find the active entry for a given hour.
/// Entries are sorted by start_hour. The active entry is the last one
/// whose start_hour <= current_hour, wrapping around midnight.
fn find_active_entry(entries: &[RoutineEntry], hour: u8) -> Option<&RoutineEntry> {
    if entries.is_empty() {
        return None;
    }

    // Find the last entry with start_hour <= current hour
    let mut active = None;
    for entry in entries {
        if entry.start_hour <= hour {
            active = Some(entry);
        }
    }

    // If none found (hour is before first entry), wrap to last entry
    if active.is_none() {
        active = entries.last();
    }

    active
}
```

### Registration in mod.rs

Add to `src/ticks/mod.rs`:
```rust
pub mod routine;
pub use routine::run_routine_tick;
```

Add to `src/main.rs` spawn block:
```rust
tokio::spawn(ticks::run_routine_tick(db.clone(), connections.clone()));
```

---

## NPC Step Movement

Mobiles with a `routine_destination_room` walk toward it one room per wander tick using BFS pathfinding.

### Integration with Wander Tick

The existing wander tick in `src/ticks/mobile.rs` (60s interval, 33% chance) is extended:

```rust
// In process_wander_tick(), before random wandering:

// Check for routine destination (step movement toward target)
if let Some(dest_room_id) = current_mobile.routine_destination_room {
    if let Some(room_id) = current_mobile.current_room_id {
        if room_id == dest_room_id {
            // Arrived at destination - clear it
            let mut mob = current_mobile.clone();
            mob.routine_destination_room = None;
            db.save_mobile_data(mob)?;
        } else {
            // BFS to find next step toward destination
            if let Some((direction, next_room_id)) = bfs_next_step(db, room_id, dest_room_id) {
                // Move one room toward destination
                if db.move_mobile_to_room(&current_mobile.id, &next_room_id).is_ok() {
                    let departure_msg = format!("{} leaves heading {}.\n",
                        current_mobile.name, direction);
                    broadcast_to_room_mobiles(connections, &room_id, &departure_msg);

                    let arrival_dir = get_opposite_direction_rust(&direction);
                    let arrival_msg = format!("{} arrives from the {}.\n",
                        current_mobile.name, arrival_dir);
                    broadcast_to_room_mobiles(connections, &next_room_id, &arrival_msg);
                }
            }
            continue; // Skip random wandering when traveling to destination
        }
    }
}

// Check if routine suppresses wandering for current entry
if should_suppress_wander(&current_mobile) {
    continue;
}

// ... existing random wander logic ...
```

### BFS Pathfinding (Rust)

```rust
/// BFS pathfinding: find the next room to move toward a destination.
/// Returns (direction, next_room_id) for the first step, or None if no path.
/// Capped at MAX_BFS_DEPTH to prevent excessive searching.
const MAX_BFS_DEPTH: usize = 20;

fn bfs_next_step(
    db: &db::Db,
    from: Uuid,
    to: Uuid,
) -> Option<(String, Uuid)> {
    use std::collections::{HashSet, VecDeque};

    if from == to {
        return None;
    }

    // BFS queue: (room_id, first_step_direction, first_step_room_id)
    let mut queue: VecDeque<(Uuid, String, Uuid)> = VecDeque::new();
    let mut visited: HashSet<Uuid> = HashSet::new();
    visited.insert(from);

    // Seed with exits from starting room
    if let Ok(Some(room)) = db.get_room_data(&from) {
        for (dir, next_id) in get_valid_wander_exits(db, &room).unwrap_or_default() {
            if !visited.contains(&next_id) {
                if next_id == to {
                    return Some((dir, next_id));
                }
                visited.insert(next_id);
                queue.push_back((next_id, dir, next_id));
            }
        }
    }

    let mut depth = 0;
    while let Some((current, first_dir, first_room)) = queue.pop_front() {
        depth += 1;
        if depth > MAX_BFS_DEPTH {
            return None; // Give up if destination is too far
        }

        if let Ok(Some(room)) = db.get_room_data(&current) {
            for (_, next_id) in get_valid_wander_exits(db, &room).unwrap_or_default() {
                if next_id == to {
                    return Some((first_dir, first_room));
                }
                if !visited.contains(&next_id) {
                    visited.insert(next_id);
                    queue.push_back((next_id, first_dir.clone(), first_room));
                }
            }
        }
    }

    None // No path found
}
```

### Movement Characteristics

| Property | Value |
|----------|-------|
| Movement rate | 1 room per wander tick (60s) |
| Pathfinding | BFS, max 20 rooms depth |
| Door handling | Respects closed doors (cannot pass) |
| No-mob rooms | Respects `no_mob` flag (avoids) |
| Combat interrupt | Stops moving if enters combat |
| Arrival at destination | Clears `routine_destination_room` |

---

## Activity State Effects

### Shop Availability

In `scripts/commands/buy.rhai` and `scripts/commands/sell.rhai`:

```rhai
// After finding the shopkeeper mobile:
let activity = get_mobile_activity(mobile_id);
if activity != "working" {
    if activity == "sleeping" {
        send(conn, mobile.name + " is sleeping and can't help you right now.\n");
    } else {
        send(conn, mobile.name + " isn't open for business right now.\n");
    }
    return;
}
```

### Healer Availability

In `scripts/commands/heal.rhai`:

```rhai
let activity = get_mobile_activity(mobile_id);
if activity != "working" {
    send(conn, mobile.name + " isn't available for healing right now.\n");
    return;
}
```

### Sleeping Description

In `scripts/commands/look.rhai`, when listing mobiles in a room:

```rhai
let activity = get_mobile_activity(mobile_id);
if activity == "sleeping" {
    send(conn, mobile.name + " is here, sleeping.\n");
} else {
    send(conn, mobile.short_desc + "\n");
}
```

### Dialogue Gating

In `scripts/commands/talk.rhai`:

```rhai
let activity = get_mobile_activity(mobile_id);

// Check for routine-specific dialogue overrides first
let override_response = get_routine_dialogue(mobile_id, keyword);
if override_response != () {
    send(conn, mobile.name + " says \"" + override_response + "\"\n");
    return;
}

// Sleeping NPCs don't respond
if activity == "sleeping" {
    send(conn, mobile.name + " is sleeping and doesn't respond.\n");
    return;
}

// Fall through to normal dialogue
```

### Wander Suppression

The wander tick checks `suppress_wander` on the mobile's active routine entry:

```rust
fn should_suppress_wander(mobile: &MobileData) -> bool {
    // If mobile has no routine, don't suppress
    if mobile.daily_routine.is_empty() {
        return false;
    }
    // Mobile is traveling to a destination - suppress random wander
    if mobile.routine_destination_room.is_some() {
        return true;
    }
    // Check the current entry's suppress_wander flag
    // (requires knowing current hour, passed from tick context)
    false
}
```

### State Effects Summary

| Activity State | Shop | Healer | Dialogue | Wander | Look Description |
|---------------|------|--------|----------|--------|-----------------|
| Working | Open | Available | Normal | Per entry flag | Normal `short_desc` |
| Sleeping | Closed | Unavailable | No response | Suppressed | "is here, sleeping." |
| Patrolling | Closed | Unavailable | Normal | Suppressed (walks route) | Normal `short_desc` |
| OffDuty | Closed | Unavailable | Normal + overrides | Per entry flag | Normal `short_desc` |
| Socializing | Closed | Unavailable | Normal + overrides | Per entry flag | Normal `short_desc` |
| Eating | Closed | Unavailable | Normal + overrides | Suppressed | Normal `short_desc` |
| Custom(str) | Closed | Unavailable | Normal + overrides | Per entry flag | Normal `short_desc` |

---

## Routine Presets

### Concept

Routine presets are generic JSON templates stored in `scripts/data/routine_presets.json`. They define common daily patterns that builders can apply to mobiles via `medit routine preset <name> <mappings>`.

This follows the same pattern as `ShopPreset` in `src/script/shop_presets.rs` and `scripts/data/` - admin-level data loaded at startup.

### Preset Format

```json
{
    "merchant_8to20": {
        "name": "Standard Merchant (8am-8pm)",
        "description": "Opens shop at dawn, closes at dusk, sleeps at night",
        "entries": [
            {
                "start_hour": 5,
                "activity": "working",
                "destination_vnum": "{shop}",
                "transition_message": "opens up shop for the day.",
                "suppress_wander": true
            },
            {
                "start_hour": 20,
                "activity": "off_duty",
                "destination_vnum": "{home}",
                "transition_message": "closes up shop for the evening.",
                "suppress_wander": false
            },
            {
                "start_hour": 22,
                "activity": "sleeping",
                "destination_vnum": "{home}",
                "transition_message": "settles in for the night.",
                "suppress_wander": true
            }
        ]
    },
    "guard_dayshift": {
        "name": "Day Guard (6am-6pm)",
        "description": "Patrols during daytime, off duty at night",
        "entries": [
            {
                "start_hour": 6,
                "activity": "patrolling",
                "destination_vnum": "{post}",
                "transition_message": "takes up their post.",
                "suppress_wander": false
            },
            {
                "start_hour": 18,
                "activity": "off_duty",
                "destination_vnum": "{barracks}",
                "transition_message": "heads off duty.",
                "suppress_wander": true
            },
            {
                "start_hour": 22,
                "activity": "sleeping",
                "destination_vnum": "{barracks}",
                "transition_message": "turns in for the night.",
                "suppress_wander": true
            }
        ]
    },
    "guard_nightshift": {
        "name": "Night Guard (6pm-6am)",
        "description": "Patrols at night, sleeps during the day",
        "entries": [
            {
                "start_hour": 6,
                "activity": "sleeping",
                "destination_vnum": "{barracks}",
                "transition_message": "heads to the barracks to rest.",
                "suppress_wander": true
            },
            {
                "start_hour": 17,
                "activity": "eating",
                "destination_vnum": "{mess}",
                "transition_message": "grabs a meal before their shift.",
                "suppress_wander": true
            },
            {
                "start_hour": 18,
                "activity": "patrolling",
                "destination_vnum": "{post}",
                "transition_message": "takes up the night watch.",
                "suppress_wander": false
            }
        ]
    },
    "tavern_keeper": {
        "name": "Tavern Keeper (10am-2am)",
        "description": "Opens late morning, closes after midnight",
        "entries": [
            {
                "start_hour": 2,
                "activity": "sleeping",
                "destination_vnum": "{home}",
                "transition_message": "finally turns in for the night.",
                "suppress_wander": true
            },
            {
                "start_hour": 10,
                "activity": "working",
                "destination_vnum": "{tavern}",
                "transition_message": "opens the tavern for business.",
                "suppress_wander": true
            }
        ]
    },
    "wandering_merchant": {
        "name": "Wandering Merchant (travels between markets)",
        "description": "Sells at one market in morning, another in afternoon",
        "entries": [
            {
                "start_hour": 7,
                "activity": "working",
                "destination_vnum": "{market1}",
                "transition_message": "sets up their stall.",
                "suppress_wander": true
            },
            {
                "start_hour": 13,
                "activity": "working",
                "destination_vnum": "{market2}",
                "transition_message": "packs up and heads to another market.",
                "suppress_wander": true
            },
            {
                "start_hour": 20,
                "activity": "off_duty",
                "destination_vnum": "{home}",
                "transition_message": "packs up their wares for the day.",
                "suppress_wander": false
            },
            {
                "start_hour": 22,
                "activity": "sleeping",
                "destination_vnum": "{home}",
                "transition_message": "settles in for the night.",
                "suppress_wander": true
            }
        ]
    }
}
```

### Placeholder Substitution

Presets use `{placeholder}` tokens for room vnums. When applying a preset, the builder provides mappings:

```
medit routine preset merchant_8to20 shop=market:gregor_forge home=village:gregor_house
```

This replaces `{shop}` with `market:gregor_forge` and `{home}` with `village:gregor_house` in all entries.

### Rust Implementation

Presets are loaded as a `HashMap<String, RoutinePreset>` from `scripts/data/routine_presets.json`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutinePreset {
    pub name: String,
    pub description: String,
    pub entries: Vec<RoutineEntry>,
}
```

A Rhai function `apply_routine_preset(mobile_id, preset_name, mappings_map)` handles substitution and applies entries to the mobile.

---

## Editor: medit routine

### Subcommands

Added to `scripts/commands/medit.rhai`:

```
medit <vnum> routine                    - Show current routine
medit <vnum> routine add <hour> <activity> [dest_vnum]
                                         - Add a routine entry
medit <vnum> routine remove <hour>      - Remove entry at hour
medit <vnum> routine clear              - Remove all entries
medit <vnum> routine preset <name> <key=vnum ...>
                                         - Apply a preset with placeholder mappings
medit <vnum> routine preset list        - List available presets
medit <vnum> routine msg <hour> <message>
                                         - Set transition message for entry
medit <vnum> routine wander <hour> on|off
                                         - Toggle wander suppression for entry
medit <vnum> routine visible on|off     - Toggle schedule visibility to players
```

### Display Format

```
medit goblin_shopkeeper routine

Daily Routine for goblin_shopkeeper:
  [05:00] Working    -> market:goblin_stall  "opens up shop for the day."  (no wander)
  [20:00] OffDuty    -> caves:goblin_home    "closes up shop."
  [22:00] Sleeping   -> caves:goblin_home    "settles in for the night."   (no wander)

Schedule visible to players: yes
```

### Tab Completion

Activity states provide tab completion:
```
working, sleeping, patrolling, off_duty, socializing, eating
```

Preset names also tab-complete from loaded preset data.

---

## Player Commands

### schedule Command

New file: `scripts/commands/schedule.rhai`

```
schedule <npc>    - View an NPC's daily routine (if visible)
```

**Behavior:**
1. Find the target mobile in the current room by keyword
2. Check `schedule_visible` flag - if false, show "doesn't share their schedule"
3. Display routine entries in human-readable format with time-of-day names

**Output format:**
```
Old Gregor's daily routine:
  Dawn (5:00)      Working    - Opens his forge at the Market Square
  Evening (20:00)  Off Duty   - Heads home for the evening
  Night (22:00)    Sleeping
```

### Passive Dialogue Hints

NPCs with routines can hint at their schedule through dialogue and transition messages, without requiring the `schedule` command:

- **Transition messages** broadcast to the room when activity changes: `Old Gregor opens up shop for the day.`
- **Dialogue overrides** per time period: asking a merchant about "hours" at night might yield "Come back in the morning, I open at dawn."
- **Signs** - builders can place sign items with schedule text (using existing `read` command)

---

## Implementation Phases

### Phase 1: Core Infrastructure

Add types and the routine tick. Movement uses teleport (instant placement) for simplicity.

| File | Changes |
|------|---------|
| `src/types/mod.rs` | Add `ActivityState`, `RoutineEntry` types |
| `src/types/mod.rs` | Add `daily_routine`, `schedule_visible`, `current_activity`, `routine_destination_room` to `MobileData` |
| `src/types/mod.rs` | Update `MobileData::new()` with defaults |
| `src/ticks/routine.rs` | New file: `run_routine_tick()`, `process_routine_tick()`, `find_active_entry()` |
| `src/ticks/mod.rs` | Register routine module and re-export |
| `src/main.rs` | Spawn `run_routine_tick()` background task |
| `src/script/mobiles.rs` | Add getters/setters for new MobileData fields |

**Verification:** Create a mobile with two routine entries (different rooms). Confirm it teleports to the correct room when game hour changes.

### Phase 2: Step Movement

Replace teleport with BFS pathfinding. Mobiles walk one room per wander tick toward their destination.

| File | Changes |
|------|---------|
| `src/ticks/mobile.rs` | Add `bfs_next_step()` function |
| `src/ticks/mobile.rs` | Integrate routine destination check into `process_wander_tick()` |
| `src/ticks/mobile.rs` | Add `should_suppress_wander()` check |

**Verification:** Place a mobile 3 rooms away from its routine destination. Confirm it walks one room per wander tick (60s) until arriving. Confirm it broadcasts departure/arrival messages.

### Phase 3: Activity State Effects

Shop, healer, dialogue, and look commands respect the current activity state.

| File | Changes |
|------|---------|
| `src/script/mobiles.rs` | Add `get_mobile_activity()` Rhai function |
| `src/script/mobiles.rs` | Add `get_routine_dialogue()` Rhai function |
| `scripts/commands/buy.rhai` | Check activity state before allowing purchase |
| `scripts/commands/sell.rhai` | Check activity state before allowing sale |
| `scripts/commands/list.rhai` | Check activity state before showing inventory |
| `scripts/commands/heal.rhai` | Check activity state before allowing healing |
| `scripts/commands/look.rhai` | Show sleeping description for sleeping mobiles |
| `scripts/commands/talk.rhai` | Check dialogue overrides, gate sleeping NPCs |

**Verification:** Set a shopkeeper's activity to `sleeping`. Confirm `buy`, `sell`, `list` show appropriate messages. Confirm `look` shows sleeping description. Confirm `talk` shows no-response message.

### Phase 4: Presets and Editor

Add preset system and medit subcommands.

| File | Changes |
|------|---------|
| `scripts/data/routine_presets.json` | New file: preset definitions |
| `src/types/mod.rs` | Add `RoutinePreset` type |
| `src/script/mobiles.rs` | Add preset loading and application functions |
| `scripts/commands/medit.rhai` | Add `routine` subcommand tree |
| `src/script/utilities.rs` | Add tab completion entries for activity states and preset names |

**Verification:** Apply `merchant_8to20` preset to a shopkeeper via medit. Confirm routine entries are created with correct room vnums after placeholder substitution.

### Phase 5: Player Info and Polish

Add the `schedule` command, passive hints, and polish.

| File | Changes |
|------|---------|
| `scripts/commands/schedule.rhai` | New file: `schedule <npc>` command |
| `scripts/commands/medit.rhai` | Add `routine visible` subcommand |
| `src/script/mobiles.rs` | Add `get_mobile_schedule_visible()` function |
| `src/ticks/routine.rs` | Polish transition message broadcasting |

**Verification:** Set `schedule_visible` on an NPC, confirm `schedule <npc>` displays correctly. Confirm it shows denial message when `schedule_visible` is false.

---

## Verification Plan

### Routine Tick Testing

1. Create a mobile with 3 routine entries (morning/evening/night)
2. Verify activity state changes at correct game hours
3. Verify transition messages broadcast to room
4. Verify routine wraps correctly at midnight (entry at hour 22 active until hour 5 entry)

### Movement Testing

1. Create a mobile with routine destination 3 rooms away
2. Verify BFS finds a valid path
3. Verify mobile moves one room per wander tick
4. Verify departure/arrival messages are broadcast
5. Verify `routine_destination_room` is cleared on arrival
6. Verify mobile stops moving if it enters combat
7. Verify mobile respects closed doors and no_mob flags
8. Verify mobile does not move if BFS depth exceeds 20 (unreachable destination)

### Activity State Testing

1. Set shopkeeper activity to `sleeping` - verify shop commands fail gracefully
2. Set healer activity to `off_duty` - verify heal command fails gracefully
3. Verify `look` shows sleeping description
4. Verify dialogue overrides work per routine entry
5. Verify wander suppression works per entry flag

### Preset Testing

1. Apply `merchant_8to20` preset with placeholder mappings
2. Verify all entries created with correct vnums
3. Verify invalid placeholder shows error
4. Test `medit routine preset list` shows all presets

### Edge Cases

1. Mobile with empty routine - no effect, behaves normally
2. Mobile with single entry - that activity state always active
3. Routine destination in unreachable area - BFS gives up at depth 20, mobile stays
4. Mobile killed mid-route - routine tick skips dead mobiles
5. Multiple mobiles on same routine - each processes independently
6. Sentinel mobile with routine - activity changes but no movement

---

## Files to Modify/Create

### Rust Files

| File | Changes |
|------|---------|
| `src/types/mod.rs` | Add `ActivityState`, `RoutineEntry`, `RoutinePreset` types; extend `MobileData` |
| `src/ticks/routine.rs` | New file: routine tick processing |
| `src/ticks/mod.rs` | Register routine module, re-export `run_routine_tick` |
| `src/ticks/mobile.rs` | Add BFS pathfinding, integrate routine destination, wander suppression |
| `src/main.rs` | Spawn `run_routine_tick()` background task |
| `src/script/mobiles.rs` | Add Rhai functions for routine fields, activity, dialogue |
| `src/script/utilities.rs` | Add tab completion for activity states and preset names |

### Script Files

| File | Purpose |
|------|---------|
| `scripts/commands/schedule.rhai` | New: player-facing schedule command |
| `scripts/data/routine_presets.json` | New: preset definitions |

### Modified Scripts

| File | Changes |
|------|---------|
| `scripts/commands/medit.rhai` | Add `routine` subcommand tree |
| `scripts/commands/buy.rhai` | Check activity state |
| `scripts/commands/sell.rhai` | Check activity state |
| `scripts/commands/list.rhai` | Check activity state |
| `scripts/commands/heal.rhai` | Check activity state |
| `scripts/commands/look.rhai` | Show sleeping description |
| `scripts/commands/talk.rhai` | Dialogue overrides, sleeping gate |

---

## Design Decisions Summary

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Relationship to transport_route | Coexist independently | Different use cases - transport_route rides vehicles, routines walk between rooms |
| NPC movement | BFS step movement (1 room/wander tick) | Visible, immersive, players see NPCs walking through the world |
| Player schedule info | Both passive hints + opt-in `schedule` command | Passive feels natural, command provides detail for interested players |
| Data model | `Vec<RoutineEntry>` on MobileData | Simple, `#[serde(default)]` for backward compatibility |
| Time basis | Game hours 0-23 | Matches existing game time system (2 real min = 1 game hour) |
| Tick interval | 120s (once per game hour) | Aligns with time tick; activity changes only need hourly granularity |
| Activity states | Enum with Custom(String) variant | Covers common cases, extensible without code changes |
| Wander integration | Per-entry `suppress_wander` flag | Fine-grained control - merchants stay put, off-duty guards can roam |
| Presets | Generic JSON with placeholder substitution | Reusable patterns, admin-level like shop presets, no per-NPC boilerplate |
| BFS depth limit | 20 rooms | Prevents excessive computation, reasonable for most area layouts |
| Sentinel interaction | Activity changes but no movement | Respects existing sentinel semantics, still useful for shop hours |
| Sleeping in look | Override short_desc display | Minimal change, clearly communicates NPC state to players |

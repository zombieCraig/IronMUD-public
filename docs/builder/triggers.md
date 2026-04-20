# Triggers and Scripting

Triggers enable scripted behaviors that fire when specific events occur. Each trigger points to a Rhai script in `scripts/triggers/`.

## Trigger Types Overview

### Room Triggers

| Type | Event | Can Cancel |
|------|-------|------------|
| `on_enter` | Player enters room | Yes |
| `on_exit` | Player leaves room | Yes |
| `on_look` | Player looks at room | No |
| `periodic` | Timer-based (every N seconds) | No |
| `on_time_change` | Time of day changes | No |
| `on_weather_change` | Weather changes | No |
| `on_season_change` | Season changes | No |

### Item Triggers

| Type | Event | Can Cancel |
|------|-------|------------|
| `on_get` | Player picks up item | Yes |
| `on_drop` | Player drops item | Yes |
| `on_use` | Player uses item (drink/eat) | Yes |
| `on_examine` | Player examines item | No |

### NPC Triggers

| Type | Event | Can Cancel |
|------|-------|------------|
| `on_greet` | Player enters room with NPC | No |
| `on_say` | Player says something in room | No |
| `on_idle` | Periodic when players present | No |
| `on_attack` | NPC is attacked (future) | Yes |
| `on_death` | NPC dies (future) | No |

## Managing Triggers

### Room Triggers

```
> redit trigger
=== Room Triggers ===
(none)

> redit trigger add enter trapped_room
Trigger added: enter -> trapped_room

> redit trigger add periodic forest_ambiance
Trigger added: periodic -> forest_ambiance

> redit trigger interval 1 30
Trigger 1 interval set to 30 seconds.

> redit trigger chance 1 50
Trigger 1 chance set to 50%.

> redit trigger
=== Room Triggers ===
  0. on_enter -> trapped_room [ON]
  1. periodic -> forest_ambiance [ON] (every 30s) (50% chance)
```

### Item Triggers

```
> oedit sword trigger add get cursed_item
Added get trigger: cursed_item

> oedit sword trigger chance 0 30
Trigger at index 0 now has 30% chance to fire.

> oedit sword trigger list
=== Triggers on Cursed Blade ===
0: [ON]  on_get -> cursed_item (30% chance)
```

### NPC Triggers

```
> medit guard trigger add greet guard_greet
Added greet trigger: guard_greet

> medit guard trigger chance 0 50
Trigger at index 0 now has 50% chance to fire.
```

## Trigger Properties

All triggers have these configurable properties:

| Property | Description |
|----------|-------------|
| `script_name` | Name of the script (without path/extension) |
| `enabled` | Whether the trigger is active |
| `chance` | Percentage chance to fire (1-100, default 100) |
| `interval_secs` | For periodic/idle triggers, how often to fire |

## Writing Trigger Scripts

Scripts go in `scripts/triggers/` and export a `run_trigger` function:

```rhai
fn run_trigger(entity_id, connection_id, context) {
    // entity_id: UUID of room/item/mobile
    // connection_id: UUID of player's connection
    // context: Map with event-specific data

    // Return "continue" to allow action
    // Return "cancel" to prevent action (if cancelable)
    return "continue";
}
```

### Available Functions

```rhai
// Messaging
send_client_message(connection_id, message)
broadcast_to_room(room_id, message, exclude_name)

// Data access
get_player_character(connection_id)
get_room_data(room_id)
get_item_data(item_id)
get_mobile_data(mobile_id)

// Utilities
random_int(min, max)

// Room modifications (for triggers)
set_room_dynamic_desc(room_id, description)
clear_room_dynamic_desc(room_id)
set_room_flag(room_id, flag_name, value)
```

## Room Trigger Examples

### Trapped Room (on_enter)

```rhai
fn run_trigger(room_id, connection_id, context) {
    let char = get_player_character(connection_id);
    if char == () { return "continue"; }

    let roll = random_int(1, 100);
    if roll <= 30 {
        send_client_message(connection_id, "A magical barrier repels you!");
        return "cancel";  // Block entry
    }
    return "continue";
}
```

### Forest Ambiance (periodic)

```rhai
fn run_trigger(room_id, connection_id, context) {
    let messages = [
        "An owl hoots in the distance.",
        "The wind whispers through the branches.",
        "You hear rustling nearby."
    ];
    let idx = random_int(0, messages.len() - 1);
    send_client_message(connection_id, messages[idx]);
    return "continue";
}
```

## Environmental Triggers

Environmental triggers fire when game world conditions change.

### Time Change Context

```rhai
// context contains:
// old_time, new_time: dawn, morning, noon, afternoon, dusk, evening, night
// is_dawn, is_dusk, is_night, is_day: "true" or "false"
```

### Weather Change Context

```rhai
// context contains:
// old_weather, new_weather: clear, cloudy, rain, heavy_rain, etc.
// is_raining, is_snowing, is_clear: "true" or "false"
```

### Season Change Context

```rhai
// context contains:
// old_season, new_season: spring, summer, autumn, winter
// is_spring, is_summer, is_autumn, is_winter: "true" or "false"
```

### Built-in Templates

For simple room messages, use templates instead of scripts:

| Template | Arguments | Description |
|----------|-----------|-------------|
| `@room_message` | `<message>` | Always broadcasts |
| `@random_message` | `<msg1\|msg2\|...>` | Broadcasts random message |
| `@time_message` | `<time> <message>` | On specific time |
| `@weather_message` | `<weather> <message>` | On specific weather |
| `@season_message` | `<season> <message>` | On specific season |

```
> redit trigger add on_time_change @time_message dusk "Torches flicker to life."
> redit trigger add on_weather_change @weather_message raining "Rain drums on the roof."
> redit trigger add on_season_change @season_message winter "Frost forms on windows."
> redit trigger add periodic @random_message "Wind howls through the trees.|Leaves rustle nearby.|An owl hoots in the distance."
```

### Weather Matching

The `@weather_message` template supports categories:
- `raining` - matches light_rain, rain, heavy_rain, thunderstorm
- `snowing` - matches light_snow, snow, blizzard
- `stormy` - matches thunderstorm
- `precipitation` - matches any rain or snow
- Exact names also work: `clear`, `foggy`, `thunderstorm`

### Example: Dynamic Torches

```rhai
fn run_trigger(room_id, connection_id, context) {
    let new_time = context.get("new_time");
    if new_time == () { return "continue"; }

    if new_time == "dusk" || new_time == "night" {
        broadcast_to_room(room_id, "The wall torches flicker to life.", "");
    } else if new_time == "dawn" || new_time == "morning" {
        broadcast_to_room(room_id, "The wall torches extinguish.", "");
    }
    return "continue";
}
```

## Item Built-in Templates

For simple item messages, use templates instead of scripts:

| Template | Arguments | Description |
|----------|-----------|-------------|
| `@message` | `<message>` | Sends message to player |
| `@random_message` | `<msg1\|msg2\|...>` | Sends random message |
| `@block_message` | `<message>` | Sends message AND cancels action |

```
> oedit sword trigger add get @message "The sword hums as you grasp it."
> oedit book trigger add examine @random_message "The pages flutter.|Ancient text glimmers.|You hear a whisper."
> oedit ring trigger add drop @block_message "The ring refuses to leave your finger!"
```

The `@block_message` template is particularly useful for cursed or soulbound items that should prevent pickup or drop.

## Item Trigger Examples

### Cursed Item (on_get)

```rhai
fn run_trigger(item_id, connection_id, context) {
    let roll = random_int(1, 100);
    if roll <= 30 {
        send_client_message(connection_id, "Dark energy courses through your hand!");
        return "cancel";  // Block pickup
    }
    send_client_message(connection_id, "A chill runs down your spine.");
    return "continue";
}
```

### Soulbound Item (on_drop)

```rhai
fn run_trigger(item_id, connection_id, context) {
    send_client_message(connection_id, "This item is soulbound!");
    return "cancel";  // Always prevent drop
}
```

### Ancient Tome (on_examine)

```rhai
fn run_trigger(item_id, connection_id, context) {
    let messages = [
        "The pages whisper of ancient evil.",
        "You glimpse a map before the ink fades.",
        "Strange symbols glow briefly."
    ];
    let idx = random_int(0, messages.len() - 1);
    send_client_message(connection_id, messages[idx]);
    return "continue";  // on_examine can't cancel
}
```

## NPC Trigger Examples

### Guard Greeting (on_greet)

```rhai
fn run_trigger(mobile_id, connection_id, context) {
    let mobile = get_mobile_data(mobile_id);
    let roll = random_int(1, 3);

    if roll == 1 {
        send_client_message(connection_id, mobile.name + " eyes you suspiciously.");
    } else if roll == 2 {
        send_client_message(connection_id, mobile.name + " nods curtly.");
    } else {
        send_client_message(connection_id, mobile.name + " says: \"State your business.\"");
    }
    return "continue";
}
```

### Secret Word Response (on_say)

```rhai
fn run_trigger(mobile_id, connection_id, context) {
    let message = context.get("message");
    if message == () { return "continue"; }

    let mobile = get_mobile_data(mobile_id);
    let msg_lower = message.to_lower();

    if msg_lower.contains("xyzzy") {
        send_client_message(connection_id, mobile.name + " gasps: \"You know the ancient word!\"");
    }
    return "continue";
}
```

### NPC Built-in Templates

| Template | Arguments | Behavior |
|----------|-----------|----------|
| `@say_greeting` | `<message>` | NPC says the message |
| `@say_random` | `<msg1\|msg2\|...>` | Random message |
| `@emote` | `<action>` | NPC performs action |

```
> medit shopkeeper trigger add greet @say_greeting Welcome to my shop!
> medit innkeeper trigger add greet @say_random Hello!|Welcome!|Make yourself at home.
> medit guard trigger add greet @emote eyes you suspiciously.
```

### Idle Triggers

Fire periodically when players are in the room:

```
> medit guard trigger add idle @say_random Nice weather.|*yawns*
> medit guard trigger interval 0 30
> medit guard trigger chance 0 50
```

## Testing Triggers

Test triggers manually for debugging:

```
> redit trigger test 0
Testing trigger 0...
Trigger executed successfully!
Result: continue

> medit guard trigger test 0
Testing trigger 0...
Trigger test failed: Script not found: scripts/triggers/missing.rhai
```

## Script File Structure

```
scripts/triggers/
├── trapped_room.rhai       # Room: on_enter
├── forest_ambiance.rhai    # Room: periodic
├── cursed_item.rhai        # Item: on_get
├── soulbound_drop.rhai     # Item: on_drop
├── ancient_tome.rhai       # Item: on_examine
├── guard_greet.rhai        # NPC: on_greet
└── secret_word.rhai        # NPC: on_say
```

## Context Variables Reference

### Room Triggers

| Trigger | Context |
|---------|---------|
| on_enter | `direction`, `source_room` |
| on_exit | `direction`, `target_room` |
| on_look | (none) |
| periodic | `trigger_type: "periodic"` |
| on_time_change | `old_time`, `new_time`, `is_dawn`, `is_dusk`, `is_night`, `is_day` |
| on_weather_change | `old_weather`, `new_weather`, `is_raining`, `is_snowing`, `is_clear` |
| on_season_change | `old_season`, `new_season`, `is_spring`, `is_summer`, `is_autumn`, `is_winter` |

### Item Triggers

| Trigger | Context |
|---------|---------|
| on_get | `char_name`, `room_id`, `from_container`, `container_id?`, `container_name?` |
| on_drop | `char_name`, `room_id` |
| on_use | `char_name`, `room_id`, `use_type: "drink"\|"eat"` |
| on_examine | `char_name`, `room_id` |

### NPC Triggers

| Trigger | Context |
|---------|---------|
| on_greet | `char_name`, `direction`, `source_room`, `mobile_name` |
| on_say | `char_name`, `message`, `room_id`, `mobile_name` |
| on_idle | `trigger_type: "idle"`, `mobile_name` |

## Related Documentation

- [Rooms](rooms.md) - Room creation and properties
- [Items](items.md) - Item creation and properties
- [Mobiles](mobiles.md) - NPC creation and properties
- [Builder Guide](../builder-guide.md) - Overview of building

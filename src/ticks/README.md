# Ticks

Background tick systems that run periodically to update game state.

## File Structure

```
src/ticks/
  mod.rs         # Re-exports all public tick functions
  broadcast.rs   # Shared messaging utilities (internal)
  character.rs   # Player stat ticks (thirst, hunger, regen)
  combat.rs      # Combat round processing
  environment.rs # Time, weather, and exposure systems
  mobile.rs      # Mobile wandering and effects
  rent.rs        # Property rent auto-payment
  spawn.rs       # Spawn point processing
  spoilage.rs    # Corpse decay and food spoilage
  transport.rs   # Transport movement (elevators, buses, etc.)
  triggers.rs    # Periodic and idle triggers
```

## Tick Systems

| System | Interval | File | Purpose |
|--------|----------|------|---------|
| Spawn | 30s | spawn.rs | Respawn mobiles and items at spawn points |
| Periodic Triggers | 10s | triggers.rs | Fire room/mobile scripted events |
| Time | 120s | environment.rs | Advance game time, update weather |
| Thirst | 60s | character.rs | Process player thirst and dehydration |
| Hunger | 120s | character.rs | Process player hunger and starvation |
| Regen | 10s | character.rs | Regenerate HP/stamina for resting players |
| Wander | 60s | mobile.rs | Move wandering NPCs between rooms |
| Mobile Effects | 30s | mobile.rs | Poison emotes and other periodic effects |
| Combat | 5s | combat.rs | Process combat rounds |
| Corpse Decay | 60s | spoilage.rs | Remove old corpses |
| Spoilage | 60s | spoilage.rs | Accumulate food spoilage |
| Exposure | 30s | environment.rs | Apply weather effects (cold, heat, wet) |
| Transport | 1s | transport.rs | Move elevators, buses, trains between stops |
| Rent | 300s | rent.rs | Auto-pay property rent from escrow |

## Architecture

Each tick system follows the same pattern:

- `run_*_tick()` - Async loop that calls process function on interval
- `process_*()` - Core logic for the tick (synchronous)

All tick runners are spawned from `main.rs` via `tokio::spawn()`.

### Shared Utilities

The `broadcast.rs` module provides internal messaging functions used across tick systems:

- `send_message_to_character()` - Send message to a specific player
- `sync_character_to_session()` - Update session with character changes
- `broadcast_to_room()` - Send message to all players in a room
- `broadcast_to_room_awake()` - Send only to awake players
- `broadcast_to_room_except()` - Exclude a specific player
- `broadcast_to_room_mobiles()` - Notify players about mobile activity

## Module Details

### character.rs
Handles player vital stat depletion and regeneration:
- Thirst increases over time, causing dehydration damage
- Hunger increases over time, causing starvation damage
- HP and stamina regenerate when resting (sitting, sleeping)

### combat.rs
Processes combat rounds between characters and mobiles:
- Attack resolution with hit/miss rolls
- Damage calculation with weapon and skill bonuses
- Wound system (bleeding, severe wounds, critical hits)
- Death processing and corpse creation

### environment.rs
Manages time and weather systems:
- Game time advances (2 real minutes = 1 game hour)
- Weather transitions (clear, cloudy, rain, storm, etc.)
- Exposure effects (hypothermia, heat exhaustion, wetness)

### mobile.rs
Controls mobile NPC behavior:
- Wandering through valid exits (respects no_mob flags, doors)
- Aggressive mobiles attack players on sight
- Poison emotes and other periodic visual effects

### triggers.rs
Fires scripted events:
- Room periodic triggers (ambient messages, events)
- Mobile idle triggers (NPCs doing actions when not in combat)
- Fishing bite processing

### transport.rs
Manages moving vehicles:
- Elevators, buses, trains, ferries, airships
- Scheduled routes with dwell times at stops
- NPC boarding/disembarking based on schedules

### spawn.rs
Respawns entities at spawn points:
- Mobiles respawn after death or despawn
- Items respawn in rooms and containers
- Container refilling for shops and loot

### spoilage.rs
Handles decay systems:
- Corpses decay and are removed after timeout
- Food items accumulate spoilage based on temperature

### rent.rs
Processes property ownership:
- Auto-pays rent from escrow balance
- Evicts tenants when escrow runs out
- Handles escrow expiration

# CLAUDE.md

Guidance for Claude Code when working with this repository.

## Project Overview

This is a MUD game project written primarily in Rust with Rhai scripting. Key systems include: magic/mana, spells, character classes, status/prompt display, and tab completion. When implementing features, check how similar systems (e.g., combat, skills) are already structured and follow the same patterns.

## Build & Test

After making changes to Rust source files, always run `cargo build` and `cargo test` before considering the task complete. Fix all compilation errors before committing.

## Quick Reference

```bash
cargo build                    # Build
cargo test                     # Run all tests (preferred verification)
cargo run                      # Run server (port 4000)
cargo test --test server       # Integration tests only
```

## Documentation

| Guide | Purpose |
|-------|---------|
| [Player Guide](docs/player-guide.md) | How to play |
| [Builder Guide](docs/builder-guide.md) | OLC system overview |
| [Builder Details](docs/builder/) | Detailed editor docs (rooms, items, mobiles, etc.) |
| [Admin Guide](docs/admin-guide.md) | Server administration |
| [Import Guide](docs/import-guide.md) | Importing legacy MUD content (CircleMUD, ...) |
| [Installation](docs/installation.md) | Setup and configuration |

## Architecture Overview

IronMUD separates Rust core (networking, database, scripting host) from game logic (Rhai scripts in `scripts/commands/`). This enables hot-reloading without recompilation.

### Core Components

| Component | Location | Purpose |
|-----------|----------|---------|
| Types | `src/types/mod.rs` | All data types (Character, Room, Item, Mobile, etc.) |
| Ticks | `src/ticks/mod.rs` | Background systems (spawn, combat, time, weather) |
| World State | `src/lib.rs` | Central hub: Rhai engine, database, script cache |
| SharedConnections | `src/lib.rs` | Player sessions, separated to prevent deadlock |
| Database | `src/db.rs` | Sled wrapper, JSON storage, Argon2 hashing |
| Scripting | `src/script/*.rs` | Domain-based Rhai function registration |

### Data Flow

```
TCP Input → command channel → scripts/commands/{cmd}.rhai → Rhai functions → TCP Output
```

### Key Types (in `src/types/mod.rs`)

- `CharacterData` / `MobileData` / `ItemData` / `RoomData` - Game entities
- `*Flags` structs - Boolean properties for each entity type
- `*TriggerType` enums - Event triggers for rooms, items, mobiles
- `GameTime` / `Season` / `WeatherCondition` - Time and weather system

### Session Types (in `src/lib.rs`)

- `ConnectionId` / `PlayerSession` - Client session management
- `SharedConnections`: `Arc<Mutex<HashMap<ConnectionId, PlayerSession>>>`
- `SharedState`: `Arc<Mutex<World>>`

## Critical: Deadlock Prevention

The codebase uses `std::sync::Mutex` which is **NOT reentrant**.

**The Pattern:**
1. `handle_connection` clones the AST and releases World lock BEFORE `engine.call_fn`
2. Rhai functions receive `SharedConnections` directly, not `SharedState`
3. Scripts lock connections independently of World lock

**If you add new Rhai functions that need both World and Connections:**
- Never hold both locks simultaneously
- Clone needed data, release lock, then acquire the other

## Rhai Gotchas

### Expression Complexity Limit
Deeply nested if-else chains fail with "Expression exceeds maximum complexity":

```rhai
// BAD - 8+ branches causes error
if x == "a" { ... }
else if x == "b" { ... }
else if x == "c" { ... }
// ...more branches...

// GOOD - use helper function with early returns
fn get_value(x) {
    if x == "a" { return "result_a"; }
    if x == "b" { return "result_b"; }
    if x == "c" { return "result_c"; }
    return "default";
}
```

### Other Gotchas
- `trim()` is NOT a built-in Rhai string method - use `== ""` for empty checks
- Rhai maps (`#{...}`) are NOT registered Rust types - use constructors like `new_character()`
- Register Rhai functions BEFORE `load_scripts()` or scripts won't find them

## Rust/Rhai Integration

When modifying Rhai script bindings or adding new enum variants, grep the entire codebase for all call sites and match arms to ensure nothing is missed. Common pitfalls: method name mismatches between Rust API and Rhai scripts (e.g., `send_message` vs `send_client_message`), incomplete match arms on enums like `DamageType`, and missing parameters in test helpers.

## Adding New Features

When modifying game systems, multiple files must be updated in sync. Common patterns:

### Adding a New Flag (Item/Mobile/Room)
1. `src/types/mod.rs` - Add field to flags struct
2. `src/script/*.rs` - Add to `set_*_flag()` and `get_*_flag()`
3. `scripts/commands/*edit.rhai` - Add to flags display and handler
4. Consumer commands - Implement flag behavior

### Adding a New Property
1. `src/types/mod.rs` - Add field with `#[serde(default)]`
2. `src/script/*.rs` - Register getter and setter
3. `scripts/commands/*edit.rhai` - Add subcommand handler and display
4. `scripts/commands/examine.rhai` - Add display if player-visible

### Adding a New Trigger Type
1. `src/types/mod.rs` - Add enum variant
2. `src/script/triggers.rs` - Add to type matching and fire logic
3. `scripts/commands/*edit.rhai` - Add to valid types and help
4. Implement fire point in appropriate command script

### Key File Locations

| System | Rust Types | Rhai Functions | Editor |
|--------|-----------|----------------|--------|
| Items | `types/mod.rs` ItemData/ItemFlags | `script/items.rs` | `oedit.rhai` |
| Mobiles | `types/mod.rs` MobileData/MobileFlags | `script/mobiles.rs` | `medit.rhai` |
| Rooms | `types/mod.rs` RoomData/RoomFlags | `script/rooms.rs` | `redit.rhai` |
| Areas | `types/mod.rs` AreaData | `script/areas.rs` | `aedit.rhai` |
| Triggers | `types/mod.rs` *TriggerType enums | `script/triggers.rs` | `*edit.rhai` |
| Shops | `types/mod.rs` MobileData shop fields | `script/shops.rs` | `medit.rhai` |

## State Persistence Patterns

### Combat State Not Persisting (Common Issue)
**Symptom**: State changes (combat, flags) don't persist after Rhai function calls.

**Cause**: Loading entity before state function, then saving afterward overwrites the change.

**Fix**: Reload entity after state-changing functions:
```rhai
enter_mobile_combat(target_id, "player", attacker.name);
// IMPORTANT: Reload after state change
let mobile = get_mobile_data(target_id);
// Now safe to modify and save
```

### Tick System Considerations (in `src/ticks/mod.rs`)
Multiple tick systems run concurrently (spawn, combat, wander, time, etc.). Be aware of:
- Stale data if entity loaded at iteration start vs mid-iteration changes
- Mutex poisoning if panic occurs while holding locks
- Each tick has `run_*_tick()` (async loop) and `process_*()` (core logic)

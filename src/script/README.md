# Script

Rhai engine setup and function registrations, organized by domain.

| File | Purpose |
|------|---------|
| `mod.rs` | Engine initialization, type registrations, core functions |
| `rooms.rs` | Room CRUD, exits, doors, display |
| `items.rs` | Item system, containers, liquids, food |
| `mobiles.rs` | Mobile system, dialogue |
| `areas.rs` | Area CRUD, permissions, forage tables |
| `characters.rs` | Character creation, permissions, stats |
| `triggers.rs` | All trigger systems (room, item, mobile) |
| `combat.rs` | Combat mechanics, wounds, death |
| `medical.rs` | Wound treatment, healing |
| `shops.rs` | Shop and vending functions |
| `crafting.rs` | Recipe system |
| `fishing.rs` | Fishing state |
| `spawn.rs` | Spawn points |
| `transport.rs` | Elevators, scheduled transport |
| `groups.rs` | Player groups |
| `healers.rs` | NPC healer system |
| `utilities.rs` | MXP, ANSI colors, terminal |
| `ai.rs` | Claude, Gemini, Matrix integration |

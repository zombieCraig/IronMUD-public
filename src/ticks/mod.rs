//! Tick systems for IronMUD
//!
//! This module contains all background tick systems that run periodically
//! to update game state. Each tick system is organized into its own submodule.
//!
//! # Tick Systems
//!
//! | System | Interval | Description |
//! |--------|----------|-------------|
//! | Spawn | 30s | Respawns mobiles and items at spawn points |
//! | Periodic Triggers | 10s | Fires periodic room/mobile triggers |
//! | Time | 120s | Advances game time (2 real min = 1 game hour) |
//! | Thirst | 60s | Depletes player thirst |
//! | Hunger | 120s | Depletes player hunger |
//! | Regen | 10s | Regenerates player HP and stamina |
//! | Wander | 60s | Moves non-sentinel mobiles randomly |
//! | Combat | 5s | Processes combat rounds |
//! | Corpse Decay | 60s | Removes old corpses |
//! | Spoilage | 60s | Accumulates food spoilage |
//! | Exposure | 30s | Processes weather exposure effects |
//! | Transport | 1s | Moves transports between stops |
//! | Rent | 300s | Auto-pays property rent |
//! | Mobile Effects | 30s | Processes mobile periodic effects (poison emotes) |
//! | Pursuit | 10s | Moves pursuing mobs toward snipers |
//! | Routine | 120s | Evaluates mobile daily routines (activity + destination) |
//! | Garden | 120s | Processes plant growth, water depletion, and infestations |
//! | Hunting | 5s | Auto-follows trails for players using the hunt command |
//! | Drowning | 10s | Depletes breath underwater, applies drowning damage |
//! | Bleeding | 30s | Applies wound bleeding damage in and out of combat |
//! | Simulation | 60s | NPC needs simulation (hunger, energy, comfort) |
//! | Migration | 300s | Emergent migrant population spawning per area |

// Internal broadcast utilities shared across tick systems
pub(crate) mod broadcast;

// Submodules for each tick system
pub mod aging;
pub mod bleeding;
pub mod character;
pub mod combat;
pub mod environment;
pub mod garden;
pub mod migration;
pub mod mobile;
pub mod pursuit;
pub mod rent;
pub mod routine;
pub mod simulation;
pub mod spawn;
pub mod spoilage;
pub mod transport;
pub mod triggers;

// Re-export all the public tick runner functions
pub use aging::run_aging_tick;
pub use bleeding::run_bleeding_tick;
pub use character::{run_drowning_tick, run_hunger_tick, run_hunting_tick, run_regen_tick, run_thirst_tick};
pub use combat::run_combat_tick;
pub use environment::{run_exposure_tick, run_time_tick};
pub use garden::run_garden_tick;
pub use migration::run_migration_tick;
pub use mobile::{run_mobile_effects_tick, run_wander_tick};
pub use pursuit::run_pursuit_tick;
pub use rent::run_rent_tick;
pub use routine::run_routine_tick;
pub use simulation::run_simulation_tick;
pub use spawn::run_spawn_tick;
pub use spoilage::{run_corpse_decay_tick, run_spoilage_tick};
pub use transport::run_transport_tick;
pub use triggers::run_periodic_trigger_tick;

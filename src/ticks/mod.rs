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

// Liveness registry + watchdog — surfaces ticks whose tasks have died.
pub mod heartbeat;

// Submodules for each tick system
pub mod achievements;
pub mod aging;
pub mod bleeding;
pub mod character;
pub mod combat;
pub mod cyberware;
pub mod donation;
pub mod environment;
pub mod garden;
pub mod migration;
pub mod mobile;
pub mod pursuit;
pub mod quests;
pub mod rent;
pub mod replicant;
pub mod routine;
pub mod simulation;
pub mod spawn;
pub mod spoilage;
pub mod transport;
pub mod triggers;
pub mod vampire;

// Re-export all the public tick runner functions
pub use heartbeat::run_watchdog as run_heartbeat_watchdog;

pub use aging::run_aging_tick;
pub use bleeding::run_bleeding_tick;
pub use character::{
    run_drowning_tick, run_hunger_tick, run_hunting_tick, run_regen_tick, run_slow_move_tick, run_thirst_tick,
};
pub use combat::run_combat_tick;
pub use cyberware::run_psyche_tick;
pub use donation::run_donation_decay_tick;
pub use environment::{run_exposure_tick, run_time_tick};
pub use garden::run_garden_tick;
pub use migration::run_migration_tick;
pub use mobile::{run_mobile_effects_tick, run_wander_tick};
pub use pursuit::run_pursuit_tick;
pub use quests::run_quest_tick;
pub use rent::run_rent_tick;
pub use replicant::run_resolve_tick;
pub use routine::run_routine_tick;
pub use simulation::run_simulation_tick;
pub use spawn::run_spawn_tick;
pub use spoilage::{run_corpse_decay_tick, run_spoilage_tick};
pub use transport::run_transport_tick;
pub use triggers::run_periodic_trigger_tick;
pub use vampire::{run_blood_tick, run_sun_tick};

/// Register expected intervals for every tick task with the heartbeat
/// registry. Call once at startup *before* spawning the tick tasks themselves
/// so that the watchdog never flags a tick that simply hasn't beat yet.
///
/// When you add a new tick, add it here too — the watchdog only inspects
/// registered names.
pub fn register_all_heartbeats() {
    use std::time::Duration;

    let pairs: &[(&'static str, u64)] = &[
        ("spawn", spawn::SPAWN_TICK_INTERVAL_SECS),
        ("periodic_triggers", triggers::PERIODIC_TRIGGER_INTERVAL_SECS),
        ("time", environment::TIME_TICK_INTERVAL_SECS),
        ("exposure", environment::EXPOSURE_TICK_INTERVAL_SECS),
        ("thirst", character::THIRST_TICK_INTERVAL_SECS),
        ("hunger", character::HUNGER_TICK_INTERVAL_SECS),
        ("regen", character::REGEN_TICK_INTERVAL_SECS),
        ("hunting", character::HUNTING_TICK_INTERVAL_SECS),
        ("drowning", character::DROWNING_TICK_INTERVAL_SECS),
        ("slow_move", character::SLOW_MOVE_TICK_INTERVAL_SECS),
        ("wander", mobile::WANDER_TICK_INTERVAL_SECS),
        ("mobile_effects", mobile::MOBILE_EFFECTS_TICK_INTERVAL_SECS),
        ("combat", combat::COMBAT_TICK_INTERVAL_SECS),
        ("corpse_decay", spoilage::CORPSE_DECAY_INTERVAL_SECS),
        ("spoilage", spoilage::SPOILAGE_TICK_INTERVAL_SECS),
        ("transport", transport::TRANSPORT_TICK_INTERVAL_SECS),
        ("rent", rent::RENT_TICK_INTERVAL_SECS),
        ("pursuit", pursuit::PURSUIT_TICK_INTERVAL_SECS),
        ("routine", routine::ROUTINE_TICK_INTERVAL_SECS),
        ("garden", garden::GARDEN_TICK_INTERVAL_SECS),
        ("bleeding", bleeding::BLEEDING_TICK_INTERVAL_SECS),
        ("simulation", simulation::SIMULATION_TICK_INTERVAL_SECS),
        ("migration", migration::MIGRATION_TICK_INTERVAL_SECS),
        ("aging", aging::AGING_TICK_INTERVAL_SECS),
        ("donation_decay", donation::DONATION_DECAY_INTERVAL_SECS),
        ("quest", quests::QUEST_TICK_INTERVAL_SECS),
        ("sun", ironmud::vampire::SUN_TICK_INTERVAL_SECS),
        ("blood", ironmud::vampire::BLOOD_TICK_INTERVAL_SECS),
        ("resolve", ironmud::replicant::RESOLVE_TICK_INTERVAL_SECS),
        ("psyche", ironmud::cyberware::PSYCHE_TICK_INTERVAL_SECS),
    ];

    for (name, secs) in pairs {
        heartbeat::register(name, Duration::from_secs(*secs));
    }
}

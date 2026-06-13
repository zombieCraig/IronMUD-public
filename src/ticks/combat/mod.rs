//! Combat system submodules
//!
//! This module contains:
//! - `tick` - Main combat tick processing (combat rounds, damage)
//! - `wounds` - Unified wound handling via the Woundable trait
//!
//! Corpse construction lives lib-side at `ironmud::corpse` so lib-side
//! kill paths (vampire feed, etc.) can build corpses without crossing
//! the bin boundary.

mod fear;
mod on_hit;
mod rage;
mod tick;
mod wounds;

// Re-export the main combat tick function and death processors
pub use tick::{
    COMBAT_TICK_INTERVAL_SECS, handle_synth_down, process_mobile_death, process_player_death, run_combat_tick,
};

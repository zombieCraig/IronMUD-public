//! Combat system submodules
//!
//! This module contains:
//! - `tick` - Main combat tick processing (combat rounds, damage)
//! - `wounds` - Unified wound handling via the Woundable trait
//! - `corpse` - Corpse creation builder pattern

pub(crate) mod corpse;
mod tick;
mod wounds;

// Re-export the main combat tick function and death processors
pub use tick::{process_mobile_death, process_player_death, run_combat_tick};

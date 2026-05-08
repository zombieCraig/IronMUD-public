//! Core data types for IronMUD
//!
//! This module contains all game data structures organized by domain.
//! Each submodule owns one concern; everything is re-exported here so
//! consumers can `use crate::types::*` (or import via the crate root via
//! `pub use types::*` in `lib.rs`).

// Domain-specific submodules
mod achievements;
mod api_keys;
mod area;
mod bugs;
mod characters;
mod combat;
mod definitions;
mod dialogue;
mod effects;
pub mod garden;
mod items;
mod mail_board;
mod mobiles;
mod property;
mod quests;
mod recipes;
mod room;
mod serde_defaults;
mod simulation;
mod social;
mod spawn;
mod time;
mod transport;
mod trigger;

// Re-export all types from submodules
pub use achievements::*;
pub use api_keys::*;
pub use area::*;
pub use bugs::*;
pub use characters::*;
pub use combat::*;
pub use definitions::*;
pub use dialogue::*;
pub use effects::*;
pub use garden::*;
pub use items::*;
pub use mail_board::*;
pub use mobiles::*;
pub use property::*;
pub use quests::*;
pub use recipes::*;
pub use room::*;
pub use simulation::*;
pub use social::*;
pub use spawn::*;
pub use time::*;
pub use transport::*;
pub use trigger::*;

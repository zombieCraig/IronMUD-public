//! Initialization module for game startup
//!
//! This module handles:
//! - Script loading and hot-reload
//! - Game data loading (classes, traits, races, recipes)

mod data;
mod scripts;

pub use data::*;
pub use scripts::*;

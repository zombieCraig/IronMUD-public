//! Session management module for player connections and broadcasting
//!
//! This module handles:
//! - Connection management (login, logout, messaging)
//! - Broadcasting messages to players (room, global, conditional)

pub mod broadcast;
pub mod connection;
pub mod death;

pub use broadcast::*;
pub use connection::*;
pub use death::kill_player_at_room;

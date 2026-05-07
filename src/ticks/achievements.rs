//! Tick-side notify shims for the achievement system.
//!
//! All real logic lives in `ironmud::script::achievements`. This module
//! re-exports the relevant fns under shorter names so combat/garden/etc.
//! tick code can call them without typing the full path.

#[allow(unused_imports)]
pub use ironmud::script::achievements::{
    award_core as award, enabled, notify_counter_core as notify_counter, notify_event_core as notify_event,
    notify_kill_core as notify_kill_with_state,
};

//! Routine tick system for IronMUD
//!
//! Evaluates mobile daily routines each game hour, updating activity states
//! and setting destination rooms for step movement.

use anyhow::Result;
use tokio::time::{interval, Duration};
use tracing::{debug, error, warn};

use ironmud::{db, find_active_entry, SharedConnections};

use super::broadcast::broadcast_to_room_awake;
use super::simulation::sleep_transition_message;

/// Routine tick interval in seconds (matches game hour = 120 real seconds)
pub const ROUTINE_TICK_INTERVAL_SECS: u64 = 120;

/// Background task that processes mobile daily routines periodically
pub async fn run_routine_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(ROUTINE_TICK_INTERVAL_SECS));

    loop {
        ticker.tick().await;

        if let Err(e) = process_routine_tick(&db, &connections) {
            error!("Routine tick error: {}", e);
        }
    }
}

/// Process routine updates for all mobiles with daily routines
fn process_routine_tick(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    let game_time = db.get_game_time()?;
    let current_hour = game_time.hour;

    let mobiles = db.list_all_mobiles()?;

    for mobile in mobiles {
        // Skip prototypes
        if mobile.is_prototype {
            continue;
        }

        // Skip dead mobiles
        if mobile.current_hp <= 0 {
            continue;
        }

        // Skip simulated NPCs - they use the simulation tick instead
        if mobile.simulation.is_some() {
            continue;
        }

        // Skip mobiles without routines
        if mobile.daily_routine.is_empty() {
            continue;
        }

        // Skip mobiles in combat
        if mobile.combat.in_combat {
            continue;
        }

        // Find the active routine entry for this hour
        let active_entry = match find_active_entry(&mobile.daily_routine, current_hour) {
            Some(entry) => entry,
            None => continue,
        };

        let new_activity = active_entry.activity.clone();
        let activity_changed = mobile.current_activity != new_activity;

        // Re-fetch for combat check only (we'll CAS-save any changes below).
        let current_mobile = match db.get_mobile_data(&mobile.id)? {
            Some(m) => m,
            None => continue,
        };
        if current_mobile.combat.in_combat {
            continue;
        }

        // Resolve destination vnum (if any) before entering the CAS closure
        // so the closure stays pure (no DB reads).
        let dest_room_id = if let Some(ref dest_vnum) = active_entry.destination_vnum {
            if current_mobile.flags.sentinel {
                None
            } else {
                match db.get_room_by_vnum(dest_vnum) {
                    Ok(Some(r)) => Some(Some(r.id)),
                    Ok(None) => {
                        warn!(
                            "Routine: mobile {} has invalid destination vnum '{}' in routine",
                            current_mobile.name, dest_vnum
                        );
                        None
                    }
                    Err(e) => {
                        warn!(
                            "Routine: failed to resolve vnum '{}' for {}: {}",
                            dest_vnum, current_mobile.name, e
                        );
                        None
                    }
                }
            }
        } else {
            None
        };

        let transition_msg = if activity_changed {
            debug!(
                "Routine: {} activity changed from {} to {}",
                current_mobile.name,
                current_mobile.current_activity.to_display_string(),
                new_activity.to_display_string()
            );
            // Configured message wins; otherwise fall back to a sleep-transition
            // default so wake/sleep are visible without per-entry configuration.
            active_entry.transition_message.clone().or_else(|| {
                sleep_transition_message(
                    &current_mobile.name,
                    &current_mobile.current_activity,
                    &new_activity,
                )
            })
        } else {
            None
        };

        let activity_for_closure = new_activity;
        db.update_mobile(&mobile.id, |m| {
            if activity_changed {
                m.current_activity = activity_for_closure.clone();
            }
            if let Some(maybe_room) = dest_room_id {
                // Clear if we're already at the destination, else set.
                match maybe_room {
                    Some(rid) => {
                        let already_there = m.current_room_id.map(|r| r == rid).unwrap_or(false);
                        m.routine_destination_room = if already_there { None } else { Some(rid) };
                    }
                    None => {}
                }
            }
        })?;

        // Side effect: broadcast after the save so we don't repeat it on CAS retry.
        if let Some(msg) = transition_msg {
            if !msg.is_empty() {
                if let Some(room_id) = current_mobile.current_room_id {
                    broadcast_to_room_awake(connections, &room_id, &msg);
                }
            }
        }
    }

    Ok(())
}

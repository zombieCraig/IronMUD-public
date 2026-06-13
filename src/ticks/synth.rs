//! Tokio loop wrapper for the synth chassis tick. The actual processing
//! logic lives in `ironmud::synth` (lib-side) so integration tests can
//! drive it directly without spinning up the runtime.
//!
//! The lib-side tick returns the synths whose System Shutdown countdown
//! expired; the death pipeline (corpse, respawn) runs here because
//! `process_player_death` is bin-side (the sun-tick pattern).

use tokio::time::{Duration, interval};
use tracing::error;

use ironmud::synth::{SYNTH_CHASSIS_TICK_INTERVAL_SECS, process_chassis_tick};
use ironmud::{SharedConnections, SharedState, db};

pub async fn run_chassis_tick(db: db::Db, connections: SharedConnections, state: SharedState) {
    let mut ticker = interval(Duration::from_secs(SYNTH_CHASSIS_TICK_INTERVAL_SECS));
    loop {
        ticker.tick().await;
        crate::ticks::heartbeat::beat("chassis");
        match process_chassis_tick(&db, &connections) {
            Ok(shutdowns) => {
                for (char_name, room_id) in shutdowns {
                    if let Ok(Some(mut char)) = db.get_character_data(&char_name) {
                        if let Err(e) =
                            crate::ticks::combat::process_player_death(&db, &connections, &mut char, &room_id, &state)
                        {
                            error!("Chassis-shutdown death failed for {}: {}", char_name, e);
                        }
                    }
                }
            }
            Err(e) => error!("Chassis tick error: {}", e),
        }
    }
}

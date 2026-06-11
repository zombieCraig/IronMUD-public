//! Tokio loop wrapper for the vampire ticks. The actual processing logic
//! lives in `ironmud::vampire` (lib-side) so integration tests can drive
//! it directly without spinning up the runtime.

use tokio::time::{Duration, interval};
use tracing::error;

use ironmud::vampire::{BLOOD_TICK_INTERVAL_SECS, SUN_TICK_INTERVAL_SECS, process_blood_tick, process_sun_tick};
use ironmud::{SharedConnections, SharedState, db};

pub async fn run_sun_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(SUN_TICK_INTERVAL_SECS));
    loop {
        ticker.tick().await;
        crate::ticks::heartbeat::beat("sun");
        match process_sun_tick(&db, &connections) {
            Ok(deaths) => {
                // Finish the death pipeline (corpse, inventory drop,
                // spawn-point cleanup) for any mob the sun just killed.
                // process_mobile_death lives bin-side and can't be called
                // from src/vampire/mod.rs directly.
                for mob_id in deaths {
                    if let Ok(Some(mut mob)) = db.get_mobile_data(&mob_id) {
                        if let Some(room_id) = mob.current_room_id {
                            if let Err(e) =
                                crate::ticks::combat::process_mobile_death(&db, &connections, &mut mob, &room_id)
                            {
                                error!("Sun-death cleanup failed for {}: {}", mob.name, e);
                            }
                        }
                    }
                }
            }
            Err(e) => error!("Sun tick error: {}", e),
        }
    }
}

pub async fn run_blood_tick(db: db::Db, connections: SharedConnections, state: SharedState) {
    let mut ticker = interval(Duration::from_secs(BLOOD_TICK_INTERVAL_SECS));
    loop {
        ticker.tick().await;
        crate::ticks::heartbeat::beat("blood");
        // Clan banes: trait name -> frenzy_dc_modifier, re-read each tick so
        // trait hot-reloads apply. World lock is released before the tick
        // body touches the connections lock (deadlock rule).
        let clan_frenzy_mods: std::collections::HashMap<String, i32> = {
            let world = state.lock().unwrap();
            world
                .trait_definitions
                .iter()
                .filter_map(|(name, def)| def.effects.get("frenzy_dc_modifier").map(|m| (name.clone(), *m)))
                .collect()
        };
        if let Err(e) = process_blood_tick(&db, &connections, &clan_frenzy_mods) {
            error!("Blood tick error: {}", e);
        }
    }
}

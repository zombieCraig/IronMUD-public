//! Tokio loop wrapper for the werewolf rage tick. The actual processing
//! logic lives in `ironmud::werewolf` (lib-side) so integration tests can
//! drive it directly without spinning up the runtime.

use tokio::time::{Duration, interval};
use tracing::error;

use ironmud::werewolf::{RAGE_TICK_INTERVAL_SECS, process_rage_tick};
use ironmud::{SharedConnections, SharedState, db};

pub async fn run_rage_tick(db: db::Db, connections: SharedConnections, state: SharedState) {
    let mut ticker = interval(Duration::from_secs(RAGE_TICK_INTERVAL_SECS));
    loop {
        ticker.tick().await;
        crate::ticks::heartbeat::beat("rage");
        // Tribe banes: trait name -> frenzy_dc_modifier, re-read each tick so
        // trait hot-reloads apply. The extraction walks ALL trait definitions
        // (the identical map the blood tick builds for clan banes). World
        // lock is released before the tick body touches the connections lock
        // (deadlock rule).
        let tribe_frenzy_mods: std::collections::HashMap<String, i32> = {
            let world = state.lock().unwrap();
            world
                .trait_definitions
                .iter()
                .filter_map(|(name, def)| def.effects.get("frenzy_dc_modifier").map(|m| (name.clone(), *m)))
                .collect()
        };
        if let Err(e) = process_rage_tick(&db, &connections, &tribe_frenzy_mods) {
            error!("Rage tick error: {}", e);
        }
    }
}

//! Quest expiry tick. Walks online players every minute and drops any
//! `ActiveQuest` whose `started_at + duration_secs` has elapsed. Offline
//! players are not swept here — the same logic re-runs the next tick after
//! login, so a stale expired quest sees its drop as soon as the player is
//! back online.

use anyhow::Result;
use tokio::time::{Duration, interval};
use tracing::error;

use ironmud::{SharedConnections, db};

pub const QUEST_TICK_INTERVAL_SECS: u64 = 60;

pub async fn run_quest_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(QUEST_TICK_INTERVAL_SECS));
    loop {
        ticker.tick().await;
        crate::ticks::heartbeat::beat("quest");
        if let Err(e) = process_quest_tick(&db, &connections) {
            error!("Quest tick error: {}", e);
        }
    }
}

fn process_quest_tick(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    // Snapshot the names of online characters under a brief lock — actual
    // character mutations happen via db.save_character_data, which is
    // routed through the same lock-release pattern as other ticks.
    let online_names: Vec<String> = {
        let conns = connections.lock().unwrap();
        conns
            .values()
            .filter_map(|s| s.character.as_ref().map(|c| c.name.clone()))
            .collect()
    };
    if online_names.is_empty() {
        return Ok(());
    }

    let now = ironmud::quest::now_secs_pub();

    for name in online_names {
        ironmud::quest::expire_quests_for(db, connections, &name, now);
    }
    Ok(())
}

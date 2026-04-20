use anyhow::Result;

use crate::db::Db;
use crate::types::*;

use super::seed_uuid;

pub fn seed_transports(db: &Db) -> Result<()> {
    // Iron Keep elevator — 3 stops (tower base, mid, top), on-demand, 10s travel
    let elevator = TransportData {
        id: seed_uuid("transport:keep_elevator"),
        vnum: Some("ironkeep:elevator".to_string()),
        name: "Iron Keep Elevator".to_string(),
        transport_type: TransportType::Elevator,
        interior_room_id: seed_uuid("ironkeep:elevator"),
        stops: vec![
            TransportStop {
                room_id: seed_uuid("ironkeep:tower_base"),
                name: "Tower Base".to_string(),
                exit_direction: "up".to_string(),
            },
            TransportStop {
                room_id: seed_uuid("ironkeep:tower_mid"),
                name: "Tower Gallery".to_string(),
                exit_direction: "up".to_string(),
            },
            TransportStop {
                room_id: seed_uuid("ironkeep:tower_top"),
                name: "Tower Rooftop".to_string(),
                exit_direction: "up".to_string(),
            },
        ],
        current_stop_index: 0,
        state: TransportState::Stopped,
        direction: 1,
        schedule: TransportSchedule::OnDemand,
        travel_time_secs: 10,
        last_state_change: 0,
    };

    db.save_transport(&elevator)?;

    tracing::info!("Seeded 1 transport (Iron Keep elevator)");
    Ok(())
}

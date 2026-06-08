//! Per-area entity-count quotas (F6).
//!
//! Areas can carry optional `max_rooms` / `max_items` / `max_mobiles` /
//! `max_spawn_points` ceilings. When a builder tries to create an entity
//! attributed to an area whose count is already at the cap, the create
//! is refused with `ApiError::Conflict`. Caps are *soft* — exceeding the
//! cap (because it was lowered after the fact) does not retroactively
//! delete content. The intent is to prevent runaway create loops, not
//! enforce a precise inventory.

use uuid::Uuid;

use super::error::ApiError;
use crate::db::Db;
use crate::types::AreaData;

/// Which entity type a quota check applies to. Each variant maps to one
/// `Option<i32>` field on `AreaData`.
#[derive(Debug, Clone, Copy)]
pub enum QuotaKind {
    Rooms,
    Items,
    Mobiles,
    SpawnPoints,
}

impl QuotaKind {
    fn label(self) -> &'static str {
        match self {
            QuotaKind::Rooms => "rooms",
            QuotaKind::Items => "items",
            QuotaKind::Mobiles => "mobiles",
            QuotaKind::SpawnPoints => "spawn_points",
        }
    }

    fn cap_of(self, area: &AreaData) -> Option<i32> {
        match self {
            QuotaKind::Rooms => area.max_rooms,
            QuotaKind::Items => area.max_items,
            QuotaKind::Mobiles => area.max_mobiles,
            QuotaKind::SpawnPoints => area.max_spawn_points,
        }
    }

    fn current_count(self, db: &Db, area_id: &Uuid) -> anyhow::Result<usize> {
        match self {
            QuotaKind::Rooms => db.count_rooms_in_area(area_id),
            QuotaKind::Items => db.count_item_protos_in_area(area_id),
            QuotaKind::Mobiles => db.count_mobile_protos_in_area(area_id),
            QuotaKind::SpawnPoints => db.count_spawn_points_in_area(area_id),
        }
    }
}

/// Refuse a create if the target area has a non-None cap and the current
/// count already meets or exceeds it. `None` area_id (orphan) is not
/// quota-checked — orphans don't belong to an area, so there's no cap.
pub fn check_area_quota(db: &Db, area_id: Option<Uuid>, kind: QuotaKind) -> Result<(), ApiError> {
    let Some(uuid) = area_id else { return Ok(()) };
    let area = match db.get_area_data(&uuid).map_err(|e| ApiError::Internal(e.to_string()))? {
        Some(a) => a,
        // Dangling area_id: caller already passed authorize_existing_area;
        // skip the quota check rather than block on a missing record.
        None => return Ok(()),
    };
    let Some(cap) = kind.cap_of(&area) else { return Ok(()) };
    if cap <= 0 {
        // Treat 0 / negative the same as "cap reached" — refuses any
        // new creation in this area until cleared.
        return Err(ApiError::Conflict(format!(
            "Area '{}' has {} cap set to {} — refusing to create more.",
            area.name,
            kind.label(),
            cap
        )));
    }
    let current = kind
        .current_count(db, &uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if current >= cap as usize {
        return Err(ApiError::Conflict(format!(
            "Area '{}' has reached its {} cap ({} / {}). Raise it via aedit or remove existing entries.",
            area.name,
            kind.label(),
            current,
            cap
        )));
    }
    Ok(())
}

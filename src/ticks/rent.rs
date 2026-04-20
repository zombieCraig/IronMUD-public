//! Rent tick system for IronMUD
//!
//! Handles automatic rent payments for player properties and escrow expiration.

use anyhow::Result;
use tokio::time::{Duration, interval};
use tracing::{debug, error};

use ironmud::{EscrowData, SharedConnections, db};

/// Interval for rent auto-payment tick (5 minutes)
pub const RENT_TICK_INTERVAL_SECS: u64 = 300;

/// Background task that processes rent payments periodically
pub async fn run_rent_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(RENT_TICK_INTERVAL_SECS));

    loop {
        ticker.tick().await;

        if let Err(e) = process_rent_tick(&db, &connections) {
            error!("Rent tick error: {}", e);
        }
    }
}

/// Process rent payments for all active leases
fn process_rent_tick(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let leases = db.list_all_leases()?;

    for lease in leases {
        // Skip already evicted leases
        if lease.is_evicted {
            continue;
        }

        // Skip leases that are still paid up
        if lease.rent_paid_until > now {
            continue;
        }

        // Rent is due - try auto-payment
        process_lease_payment(db, connections, lease, now)?;
    }

    // Check for expired escrow entries
    process_expired_escrow(db, connections, now)?;

    Ok(())
}

/// Process expired escrow entries - delete items and notify players
fn process_expired_escrow(db: &db::Db, connections: &SharedConnections, now: i64) -> Result<()> {
    let escrows = db.list_all_escrow()?;

    for escrow in escrows {
        if escrow.expires_at > now {
            continue; // Not expired yet
        }

        // Delete escrowed items (and any contents inside containers)
        for item_id in &escrow.items {
            let _ = db.delete_item_recursive(item_id);
        }

        // Remove from character's escrow_ids
        let escrow_id = escrow.id;
        if let Err(e) = db.update_character(&escrow.owner_name, |c| {
            c.escrow_ids.retain(|id| *id != escrow_id);
        }) {
            error!("Failed to update character escrow_ids after expiration: {}", e);
        }

        // Delete escrow entry
        if let Err(e) = db.delete_escrow(&escrow.id) {
            error!("Failed to delete expired escrow {}: {}", escrow.id, e);
        }

        // Notify player if online
        send_to_player_by_name(
            connections,
            &escrow.owner_name,
            "\n[Escrow] Your escrowed items have expired and been deleted.\n",
        );

        debug!(
            "Expired escrow {} for {} ({} items deleted)",
            escrow.id,
            escrow.owner_name,
            escrow.items.len()
        );
    }

    Ok(())
}

/// Process payment for a single lease
fn process_lease_payment(
    db: &db::Db,
    connections: &SharedConnections,
    mut lease: ironmud::LeaseData,
    now: i64,
) -> Result<()> {
    let owner_name = &lease.owner_name;
    let rent_amount = lease.monthly_rent;

    // Rent period is configurable via settings (default: 30 game days, 900 secs per game day)
    let rent_period_days: i64 = db
        .get_setting_or_default("rent_period_game_days", "30")
        .unwrap_or_else(|_| "30".to_string())
        .parse::<i64>()
        .unwrap_or(30)
        .max(1);
    let rent_duration: i64 = rent_period_days * 900;

    let escrow_expiry_days: i64 = db
        .get_setting_or_default("escrow_expiry_real_days", "30")
        .unwrap_or_else(|_| "30".to_string())
        .parse::<i64>()
        .unwrap_or(30)
        .max(1);

    // Try to get character data (may be online or offline)
    let mut char_data = match db.get_character_data(owner_name)? {
        Some(c) => c,
        None => {
            // Character doesn't exist - evict the lease
            debug!("Character {} not found for lease {}, evicting", owner_name, lease.id);
            lease.is_evicted = true;
            lease.eviction_time = Some(now);
            db.save_lease(&lease)?;
            return Ok(());
        }
    };

    // Check if character has enough gold
    if char_data.gold >= rent_amount {
        // Deduct rent via CAS so an online player's own gold changes don't
        // get clobbered.
        db.update_character(owner_name, |c| {
            c.gold = c.gold.saturating_sub(rent_amount);
        })?;
        char_data.gold -= rent_amount;

        // Extend lease
        lease.rent_paid_until = now + rent_duration;
        db.save_lease(&lease)?;

        // Notify player if online
        let msg = format!(
            "\n[Property] Rent of {} gold has been automatically deducted for your property.\n",
            rent_amount
        );
        send_to_player_by_name(connections, owner_name, &msg);

        debug!(
            "Auto-paid rent {} gold for {} (lease {})",
            rent_amount, owner_name, lease.id
        );
    } else {
        // Insufficient gold - evict
        lease.is_evicted = true;
        lease.eviction_time = Some(now);
        db.save_lease(&lease)?;

        // Notify player if online
        let msg = format!(
            "\n[Property] Your property has been EVICTED due to insufficient gold for rent ({} gold needed).\nYour belongings have been moved to storage. Visit a leasing office to retrieve them.\n",
            rent_amount
        );
        send_to_player_by_name(connections, owner_name, &msg);

        debug!(
            "Evicted {} from lease {} due to insufficient gold (needed {}, had {})",
            owner_name, lease.id, rent_amount, char_data.gold
        );

        // Collect items from all property rooms for escrow
        let mut item_ids: Vec<uuid::Uuid> = Vec::new();
        for room_id in &lease.instanced_rooms {
            if let Ok(items) = db.get_items_in_room(room_id) {
                for item in items {
                    // Skip amenities (no_get items belong to the property)
                    if !item.flags.no_get {
                        item_ids.push(item.id);
                    }
                }
            }
        }

        // Create escrow if there are items to store
        if !item_ids.is_empty() {
            let retrieval_fee = 100 + (rent_amount / 10);
            let escrow = EscrowData::new(
                owner_name.to_string(),
                item_ids.clone(),
                lease.id,
                escrow_expiry_days,
                retrieval_fee,
            );

            // Move items to Nowhere (escrow storage)
            for item_id in &item_ids {
                let _ = db.move_item_to_nowhere(item_id);
            }

            // Save escrow
            if let Err(e) = db.save_escrow(&escrow) {
                error!("Failed to save escrow for {}: {}", owner_name, e);
            } else {
                // Add escrow ID to character via CAS
                let escrow_id = escrow.id;
                if let Err(e) = db.update_character(owner_name, |c| {
                    c.escrow_ids.push(escrow_id);
                }) {
                    error!("Failed to update character escrow_ids: {}", e);
                }
                debug!(
                    "Created escrow {} with {} items for {}",
                    escrow.id,
                    item_ids.len(),
                    owner_name
                );
            }
        }

        // Relocate any online players out of property rooms before deletion
        relocate_players_from_rooms(connections, db, &lease.instanced_rooms, lease.leasing_office_room_id);

        // Delete property rooms and their amenities
        for room_id in &lease.instanced_rooms {
            // Delete amenities in the room first
            if let Ok(items) = db.get_items_in_room(room_id) {
                for item in items {
                    if item.flags.no_get {
                        let _ = db.delete_item(&item.id);
                    }
                }
            }
            let _ = db.delete_room(room_id);
        }
    }

    Ok(())
}

/// Relocate all online players out of doomed rooms before deletion.
/// Moves them to the destination room, updates their session and DB record, and notifies them.
fn relocate_players_from_rooms(
    connections: &SharedConnections,
    db: &db::Db,
    room_ids: &[uuid::Uuid],
    destination: uuid::Uuid,
) {
    if let Ok(mut conns) = connections.lock() {
        for (_conn_id, session) in conns.iter_mut() {
            if let Some(ref mut character) = session.character {
                if room_ids.contains(&character.current_room_id) {
                    character.current_room_id = destination;
                    let _ = db.update_character(&character.name, |c| {
                        c.current_room_id = destination;
                    });
                    let _ = session.sender.send(
                        "\nThe property around you dissolves as the lease is terminated.\nYou find yourself back at the leasing office.\n".to_string()
                    );
                }
            }
        }
    }
}

/// Send a message to a player by name if they're online
fn send_to_player_by_name(connections: &SharedConnections, name: &str, msg: &str) {
    if let Ok(conns) = connections.lock() {
        for (_conn_id, session) in conns.iter() {
            if let Some(ref char) = session.character {
                if char.name.to_lowercase() == name.to_lowercase() {
                    let _ = session.sender.send(msg.to_string());
                    return;
                }
            }
        }
    }
}

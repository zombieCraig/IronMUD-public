//! Player-to-player consignment listings.
//!
//! A seller leaves a real item instance with a broker at a price of their
//! choosing; any other player can buy it from that broker; the proceeds land in
//! the seller's bank whether or not they are online. This is the smallest step
//! to a player economy that does not require a new spatial system — it rides
//! the shop command surface (`list`, `buy`) that already exists.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One item on one broker's shelf.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsignmentListing {
    pub id: Uuid,
    /// A real item instance, moved out of the seller's inventory into limbo.
    /// It exists in exactly one place — this listing — until it sells, expires
    /// or is withdrawn.
    pub item_id: Uuid,
    pub seller_name: String,
    /// The broker's **prototype vnum**, not an instance id.
    ///
    /// Load-bearing: mobile instances are cloned at spawn and area resets
    /// delete them, so an id-keyed shelf would empty itself the first time the
    /// zone reset. Keying by vnum means the shop outlives its shopkeeper.
    pub broker_vnum: String,
    pub price: i64,
    pub listed_at: i64,
    pub expires_at: i64,
    /// The item's name at listing time, so a listing page costs no item reads.
    /// Display only — the sale re-reads the real item.
    pub item_name: String,
}

impl ConsignmentListing {
    pub fn new(
        item_id: Uuid,
        item_name: String,
        seller_name: String,
        broker_vnum: String,
        price: i64,
        now: i64,
        duration_secs: i64,
    ) -> Self {
        ConsignmentListing {
            id: Uuid::new_v4(),
            item_id,
            item_name,
            seller_name,
            broker_vnum,
            price,
            listed_at: now,
            expires_at: now + duration_secs,
        }
    }

    pub fn is_expired(&self, now: i64) -> bool {
        now >= self.expires_at
    }
}

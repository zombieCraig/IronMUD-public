//! Property / lease / escrow types for the rental system.

use super::PartyAccessLevel;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A builder-defined blueprint for rentable properties
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyTemplate {
    pub id: Uuid,
    pub vnum: String,           // e.g., "cottage_small"
    pub name: String,           // "Small Cottage"
    pub description: String,    // Shown when listing properties
    pub monthly_rent: i32,      // Gold per game month
    pub entrance_room_id: Uuid, // Template entrance room
    #[serde(default)]
    pub max_instances: i32, // 0 = unlimited
    #[serde(default)]
    pub level_requirement: i32, // Minimum level to rent
    #[serde(default)]
    pub area_id: Option<Uuid>, // Which area this template belongs to
}

impl PropertyTemplate {
    pub fn new(vnum: String, name: String) -> Self {
        PropertyTemplate {
            id: Uuid::new_v4(),
            vnum,
            name,
            description: String::new(),
            monthly_rent: 0,
            entrance_room_id: Uuid::nil(),
            max_instances: 0,
            level_requirement: 0,
            area_id: None,
        }
    }
}

/// An active rental agreement between a player and a property
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaseData {
    pub id: Uuid,
    pub template_vnum: String,        // Which PropertyTemplate
    pub owner_name: String,           // Character name who rented
    pub leasing_agent_id: Uuid,       // Mobile who leased this
    pub leasing_office_room_id: Uuid, // Room to return to via "out"
    pub area_id: Uuid,                // Area where lease is active
    pub instanced_rooms: Vec<Uuid>,   // Actual room UUIDs created
    pub entrance_room_id: Uuid,       // Player's entrance room
    pub monthly_rent: i32,            // Locked rent amount
    pub rent_paid_until: i64,         // Unix timestamp
    pub created_at: i64,              // When lease started
    #[serde(default)]
    pub is_evicted: bool, // Ended due to non-payment
    #[serde(default)]
    pub eviction_time: Option<i64>, // When eviction occurred
    #[serde(default)]
    pub party_access: PartyAccessLevel, // Access for grouped players
    #[serde(default)]
    pub trusted_visitors: Vec<String>, // Names with full access
}

impl LeaseData {
    pub fn new(
        template_vnum: String,
        owner_name: String,
        leasing_agent_id: Uuid,
        leasing_office_room_id: Uuid,
        area_id: Uuid,
        monthly_rent: i32,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        LeaseData {
            id: Uuid::new_v4(),
            template_vnum,
            owner_name,
            leasing_agent_id,
            leasing_office_room_id,
            area_id,
            instanced_rooms: Vec::new(),
            entrance_room_id: Uuid::nil(),
            monthly_rent,
            rent_paid_until: now,
            created_at: now,
            is_evicted: false,
            eviction_time: None,
            party_access: PartyAccessLevel::None,
            trusted_visitors: Vec::new(),
        }
    }
}

/// Storage for items from evicted or ended leases
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscrowData {
    pub id: Uuid,
    pub owner_name: String,    // Character who owned items
    pub items: Vec<Uuid>,      // Item IDs held in escrow
    pub source_lease_id: Uuid, // Original lease
    pub created_at: i64,       // When escrow started
    pub expires_at: i64,       // When items get deleted
    pub retrieval_fee: i32,    // Gold fee to retrieve
    #[serde(default)]
    pub destination_lease_id: Option<Uuid>, // Property to ship items to
}

impl EscrowData {
    pub fn new(
        owner_name: String,
        items: Vec<Uuid>,
        source_lease_id: Uuid,
        expires_days: i64,
        retrieval_fee: i32,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        EscrowData {
            id: Uuid::new_v4(),
            owner_name,
            items,
            source_lease_id,
            created_at: now,
            expires_at: now + (expires_days * 24 * 60 * 60),
            retrieval_fee,
            destination_lease_id: None,
        }
    }
}

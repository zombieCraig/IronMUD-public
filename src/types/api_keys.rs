//! REST API key types: permissions and the key record itself.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Permissions for an API key
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiPermissions {
    /// Can read data
    #[serde(default)]
    pub read: bool,
    /// Can modify data
    #[serde(default)]
    pub write: bool,
    /// Bypass area permission checks
    #[serde(default)]
    pub admin: bool,
}

/// API key for REST API authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: Uuid,
    /// Argon2 hash of the key
    pub key_hash: String,
    /// Human-readable name
    pub name: String,
    /// Character name for permission checks
    pub owner_character: String,
    /// Permissions granted to this key
    #[serde(default)]
    pub permissions: ApiPermissions,
    /// Unix timestamp when key was created
    pub created_at: i64,
    /// Unix timestamp when key was last used
    #[serde(default)]
    pub last_used_at: Option<i64>,
    /// Whether the key is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

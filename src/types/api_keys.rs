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
    /// Read player activity logs (everything players typed, including
    /// tells). Needs `admin` too. `None` on keys created before this scope
    /// existed: they keep the access their `admin` flag gave them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player_logs: Option<bool>,
}

impl ApiPermissions {
    /// Whether this key may read player logs.
    pub fn can_read_player_logs(&self) -> bool {
        self.admin && self.player_logs.unwrap_or(true)
    }
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

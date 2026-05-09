//! Account types: the persistent `AccountData` aggregate that owns auth and a
//! roster of characters. Foundation for shared bank, cross-character
//! achievements, and email-verified bans (all out of scope for the foundation
//! slice).

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountData {
    pub id: Uuid,
    /// Login name. Stored as the user typed it; lowercased form is the key.
    pub name: String,
    /// Argon2 hash. Source of truth for auth (was on `CharacterData` pre-feature).
    pub password_hash: String,
    /// Characters owned by this account, by name (CharacterData has no UUID
    /// today — `name` is the primary key in the `characters` tree). Mutated by
    /// `add_character_to_account` / `remove_character_from_account` in lockstep
    /// with character delete paths.
    #[serde(default)]
    pub character_names: Vec<String>,
    /// Reserved for the future email-verification slice. None until that lands.
    #[serde(default)]
    pub email: Option<String>,
    /// Reserved for the future ban-tooling slice. The login flow does already
    /// refuse banned accounts; what's missing is the admin UI and IP/email-graph
    /// evasion detection.
    #[serde(default)]
    pub is_banned: bool,
    #[serde(default = "default_created_at")]
    pub created_at: i64,
    #[serde(default)]
    pub last_login_at: i64,
}

fn default_created_at() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl AccountData {
    pub fn new(name: String, password_hash: String) -> Self {
        AccountData {
            id: Uuid::new_v4(),
            name,
            password_hash,
            character_names: Vec::new(),
            email: None,
            is_banned: false,
            created_at: default_created_at(),
            last_login_at: 0,
        }
    }
}

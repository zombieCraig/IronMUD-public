//! Account types: the persistent `AccountData` aggregate that owns auth and a
//! roster of characters. Foundation for shared bank, cross-character
//! achievements, and email-verified bans.

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
    /// Optional email address. Populated when verification is enabled (or when
    /// admins set one out of band).
    #[serde(default)]
    pub email: Option<String>,
    /// True once the account has confirmed control of `email`. Defaults to
    /// `true` so legacy accounts grandfather in on schema upgrade; only newly
    /// created accounts under `email_verification_required = true` start with
    /// `false`.
    #[serde(default = "default_email_verified")]
    pub email_verified: bool,
    /// 6-digit code outstanding for the current verification attempt. Cleared
    /// on successful verify. None when no code is pending.
    #[serde(default)]
    pub email_verification_code: Option<String>,
    /// Unix seconds. The pending code is only honored when `now < this`.
    #[serde(default)]
    pub email_verification_code_expires_at: i64,
    /// Unix seconds of the most recent send. Drives the 1-per-minute resend
    /// throttle.
    #[serde(default)]
    pub email_verification_last_sent_at: i64,
    /// Number of sends in the current rolling-hour window. Reset by the resend
    /// path when the window expires.
    #[serde(default)]
    pub email_verification_resend_count: i32,
    /// Unix seconds anchoring the rolling-hour window for the resend cap.
    #[serde(default)]
    pub email_verification_resend_window_started_at: i64,
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

fn default_email_verified() -> bool {
    true
}

impl AccountData {
    pub fn new(name: String, password_hash: String) -> Self {
        AccountData {
            id: Uuid::new_v4(),
            name,
            password_hash,
            character_names: Vec::new(),
            email: None,
            email_verified: true,
            email_verification_code: None,
            email_verification_code_expires_at: 0,
            email_verification_last_sent_at: 0,
            email_verification_resend_count: 0,
            email_verification_resend_window_started_at: 0,
            is_banned: false,
            created_at: default_created_at(),
            last_login_at: 0,
        }
    }
}

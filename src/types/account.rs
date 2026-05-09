//! Account types: the persistent `AccountData` aggregate that owns auth and a
//! roster of characters. Foundation for shared bank, cross-character
//! achievements, and email-verified bans.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BanRecord {
    pub reason: String,
    pub banned_by: String,
    pub banned_at: i64,
    #[serde(default)]
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteBanRecord {
    pub ip: String,
    pub reason: String,
    pub banned_by: String,
    pub banned_at: i64,
    #[serde(default)]
    pub expires_at: Option<i64>,
}

/// Account-wide character preference defaults. Stamped onto each freshly
/// created character so the player doesn't have to re-run `set` for every alt.
/// `is_set = false` means the player hasn't opted in yet — alt creation skips
/// the stamp and the engine's per-character defaults stand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountPreferences {
    #[serde(default)]
    pub prompt_mode: String,
    #[serde(default = "default_colors_enabled")]
    pub colors_enabled: bool,
    #[serde(default)]
    pub mxp_enabled: bool,
    #[serde(default = "default_abbrev_enabled")]
    pub abbrev_enabled: bool,
    #[serde(default)]
    pub helpline_enabled: bool,
    #[serde(default)]
    pub summonable: bool,
    #[serde(default)]
    pub automap_enabled: bool,
    #[serde(default = "default_pref_automap_radius")]
    pub automap_radius: i32,
    #[serde(default)]
    pub ascii_map: bool,
    #[serde(default)]
    pub is_set: bool,
}

fn default_colors_enabled() -> bool {
    true
}

fn default_abbrev_enabled() -> bool {
    true
}

fn default_pref_automap_radius() -> i32 {
    crate::script::map::AUTOMAP_DEFAULT_RADIUS
}

impl Default for AccountPreferences {
    fn default() -> Self {
        AccountPreferences {
            prompt_mode: String::new(),
            colors_enabled: true,
            mxp_enabled: false,
            abbrev_enabled: true,
            helpline_enabled: false,
            summonable: false,
            automap_enabled: false,
            automap_radius: crate::script::map::AUTOMAP_DEFAULT_RADIUS,
            ascii_map: false,
            is_set: false,
        }
    }
}

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
    /// Fast-check flag mirrored from `ban_record.is_some()`. Kept for legacy
    /// boolean bans saved before the metadata slice — those rows are honored
    /// without a structured record (login shows the generic suspended message).
    #[serde(default)]
    pub is_banned: bool,
    /// Structured ban metadata: reason, banned_by, banned_at, optional expiry.
    /// `None` for grandfathered legacy bans (only `is_banned` is true) and for
    /// unbanned accounts. Set by the in-game `admin ban` command and the CLI
    /// `ironmud-admin account ban` flow.
    #[serde(default)]
    pub ban_record: Option<BanRecord>,
    /// Last IP this account logged in from, in canonical "a.b.c.d" form.
    /// Stamped after a successful login. Empty for accounts that haven't logged
    /// in since the ban-tooling slice landed.
    #[serde(default)]
    pub last_login_ip: String,
    /// IP the account was created from, stamped during account creation.
    /// Empty for accounts created before the ban-tooling slice.
    #[serde(default)]
    pub creation_ip: String,
    /// Canonical form of `email` for evasion detection. For Gmail / Googlemail
    /// addresses, dots and `+tags` are stripped from the local part; other
    /// domains pass through with just trim + lowercase. None when `email` is
    /// None. Backfilled once on `Db::open` for accounts that pre-date the slice.
    #[serde(default)]
    pub normalized_email: Option<String>,
    #[serde(default = "default_created_at")]
    pub created_at: i64,
    #[serde(default)]
    pub last_login_at: i64,
    /// Account-wide pile of gold accessible to any character on this account
    /// via `bank shared deposit|withdraw`. Distinct from each character's
    /// per-character `bank_gold`.
    #[serde(default)]
    pub shared_bank_gold: i64,
    /// Per-account character-creation defaults. `is_set = false` (the default)
    /// means the player hasn't saved any defaults; new alts get the engine's
    /// blank defaults. Player opts in via `set defaults save`.
    #[serde(default)]
    pub character_defaults: AccountPreferences,
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
            ban_record: None,
            last_login_ip: String::new(),
            creation_ip: String::new(),
            normalized_email: None,
            created_at: default_created_at(),
            last_login_at: 0,
            shared_bank_gold: 0,
            character_defaults: AccountPreferences::default(),
        }
    }
}

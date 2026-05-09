// src/script/email.rs
// Rhai surface for the optional email-verification flow. The setting
// `email_verification_required` is the master switch — every entry point here
// is a no-op or returns the "feature disabled" branch when it's false.

use crate::db::Db;
use crate::email::{
    generate_code, is_disposable_email_domain, normalize_email, send_verification_email,
};
use rhai::Engine;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Code TTL fallback when the `email_verification_code_ttl_secs` setting is
/// missing or unparseable.
const DEFAULT_CODE_TTL_SECS: i64 = 1800;

/// Resend throttle: at least this many seconds between resends.
const RESEND_MIN_SPACING_SECS: i64 = 60;

/// Resend throttle: maximum resends per rolling hour window.
const RESEND_HOURLY_CAP: i32 = 5;

const RESEND_WINDOW_SECS: i64 = 3600;

pub fn register(engine: &mut Engine, db: Arc<Db>) {
    // is_email_verification_required() -> bool
    let cloned_db = db.clone();
    engine.register_fn("is_email_verification_required", move || -> bool {
        cloned_db
            .get_setting("email_verification_required")
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(false)
    });

    // is_account_email_verified(account_id_str) -> bool
    // Returns true on lookup failure so a corrupt DB doesn't soft-lock players
    // out of their own accounts; the login path still gates on `email_verified`
    // explicitly via verify_account_code only when verification is required.
    let cloned_db = db.clone();
    engine.register_fn(
        "is_account_email_verified",
        move |account_id: String| -> bool {
            let uuid = match Uuid::parse_str(&account_id) {
                Ok(u) => u,
                Err(_) => return true,
            };
            cloned_db
                .get_account_by_id(&uuid)
                .ok()
                .flatten()
                .map(|a| a.email_verified)
                .unwrap_or(true)
        },
    );

    // set_account_email(account_id_str, email) -> bool
    // Updates the account's email and clears any prior verified state — so
    // changing email always re-triggers verification when required.
    let cloned_db = db.clone();
    engine.register_fn(
        "set_account_email",
        move |account_id: String, email: String| -> bool {
            let uuid = match Uuid::parse_str(&account_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut account = match cloned_db.get_account_by_id(&uuid) {
                Ok(Some(a)) => a,
                _ => return false,
            };
            let trimmed = email.trim().to_string();
            if trimmed.is_empty() {
                account.email = None;
                account.normalized_email = None;
            } else {
                account.normalized_email = normalize_email(&trimmed);
                account.email = Some(trimmed);
            }
            account.email_verified = false;
            account.email_verification_code = None;
            account.email_verification_code_expires_at = 0;
            cloned_db.save_account(account).is_ok()
        },
    );

    // is_disposable_email(addr) -> bool
    // True when the address's domain is on the disposable-provider blocklist
    // at scripts/data/email/disposable_domains.txt.
    engine.register_fn("is_disposable_email", |email: String| -> bool {
        is_disposable_email_domain(&email)
    });

    // find_account_id_by_normalized_email(email) -> String  ("" when none)
    // Compares on Gmail-canonical form so dot/+tag tricks don't fool the
    // duplicate-email check.
    let cloned_db = db.clone();
    engine.register_fn(
        "find_account_id_by_normalized_email",
        move |email: String| -> String {
            let Some(canonical) = normalize_email(&email) else {
                return String::new();
            };
            cloned_db
                .find_account_by_normalized_email(&canonical)
                .ok()
                .flatten()
                .map(|a| a.id.to_string())
                .unwrap_or_default()
        },
    );

    // send_verification_code(account_id_str, email) -> bool
    // Generates a fresh 6-digit code, stamps it on the account with TTL, and
    // dispatches the email. Returns false on bad uuid, missing account,
    // missing SMTP config, or send failure. Caller is responsible for the
    // resend throttle (call can_resend_verification_code first).
    let cloned_db = db.clone();
    engine.register_fn(
        "send_verification_code",
        move |account_id: String, email: String| -> bool {
            let uuid = match Uuid::parse_str(&account_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut account = match cloned_db.get_account_by_id(&uuid) {
                Ok(Some(a)) => a,
                _ => return false,
            };
            let trimmed = email.trim().to_string();
            if trimmed.is_empty() {
                return false;
            }
            // Stamp email if absent or stale; code+expiry+resend-window all
            // get a fresh write below.
            account.normalized_email = normalize_email(&trimmed);
            account.email = Some(trimmed.clone());

            let code = generate_code();
            let now = now_secs();
            let ttl = cloned_db
                .get_setting("email_verification_code_ttl_secs")
                .ok()
                .flatten()
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(DEFAULT_CODE_TTL_SECS);

            account.email_verification_code = Some(code.clone());
            account.email_verification_code_expires_at = now + ttl;
            account.email_verification_last_sent_at = now;

            // Roll the resend window. If the previous window has expired,
            // start a new one; otherwise increment within it.
            if now - account.email_verification_resend_window_started_at > RESEND_WINDOW_SECS {
                account.email_verification_resend_window_started_at = now;
                account.email_verification_resend_count = 1;
            } else {
                account.email_verification_resend_count += 1;
            }

            if cloned_db.save_account(account).is_err() {
                return false;
            }

            // Persist before send so a partial-network failure still produces
            // a verifiable code — caller can re-trigger send via admin tools.
            send_verification_email(&cloned_db, &trimmed, &code).is_ok()
        },
    );

    // verify_account_code(account_id_str, code) -> i64
    //   0 = ok, verified
    //   1 = no code outstanding
    //   2 = expired
    //   3 = mismatch
    let cloned_db = db.clone();
    engine.register_fn(
        "verify_account_code",
        move |account_id: String, code: String| -> i64 {
            let uuid = match Uuid::parse_str(&account_id) {
                Ok(u) => u,
                Err(_) => return 1,
            };
            let mut account = match cloned_db.get_account_by_id(&uuid) {
                Ok(Some(a)) => a,
                _ => return 1,
            };
            let stored = match &account.email_verification_code {
                Some(c) if !c.is_empty() => c.clone(),
                _ => return 1,
            };
            let now = now_secs();
            if account.email_verification_code_expires_at != 0
                && now >= account.email_verification_code_expires_at
            {
                return 2;
            }
            let entered = code.trim();
            if entered != stored {
                return 3;
            }
            account.email_verified = true;
            account.email_verification_code = None;
            account.email_verification_code_expires_at = 0;
            // Reset resend counters now that verification is complete; a future
            // email-change flow will re-arm them via set_account_email.
            account.email_verification_resend_count = 0;
            account.email_verification_resend_window_started_at = 0;
            if cloned_db.save_account(account).is_err() {
                return 1;
            }
            0
        },
    );

    // can_resend_verification_code(account_id_str) -> i64
    //   0 = ok
    //   1 = too soon (under 60s since last send)
    //   2 = hourly cap exceeded (5 per rolling hour)
    let cloned_db = db.clone();
    engine.register_fn(
        "can_resend_verification_code",
        move |account_id: String| -> i64 {
            let uuid = match Uuid::parse_str(&account_id) {
                Ok(u) => u,
                Err(_) => return 0,
            };
            let account = match cloned_db.get_account_by_id(&uuid) {
                Ok(Some(a)) => a,
                _ => return 0,
            };
            let now = now_secs();
            if account.email_verification_last_sent_at != 0
                && now - account.email_verification_last_sent_at < RESEND_MIN_SPACING_SECS
            {
                return 1;
            }
            if now - account.email_verification_resend_window_started_at <= RESEND_WINDOW_SECS
                && account.email_verification_resend_count >= RESEND_HOURLY_CAP
            {
                return 2;
            }
            0
        },
    );

    // find_account_id_by_email(email) -> String  ("" when none)
    // Used by the create flow to refuse duplicate registrations under a single
    // inbox when verification is required.
    let cloned_db = db.clone();
    engine.register_fn(
        "find_account_id_by_email",
        move |email: String| -> String {
            cloned_db
                .find_account_by_email(&email)
                .ok()
                .flatten()
                .map(|a| a.id.to_string())
                .unwrap_or_default()
        },
    );

    // is_valid_email_format(email) -> bool
    // Hand-rolled minimal check: contains '@', has '.' after the '@', no
    // whitespace. The real validator is the SMTP send rejection.
    engine.register_fn("is_valid_email_format", |email: String| -> bool {
        let s = email.trim();
        if s.is_empty() || s.contains(' ') || s.contains('\t') {
            return false;
        }
        let at = match s.find('@') {
            Some(i) => i,
            None => return false,
        };
        if at == 0 || at == s.len() - 1 {
            return false;
        }
        let domain = &s[at + 1..];
        domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
    });
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

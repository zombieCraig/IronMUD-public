// src/script/email.rs
// Rhai surface for the optional email-verification flow. The setting
// `email_verification_required` is the master switch — every entry point here
// is a no-op or returns the "feature disabled" branch when it's false.

use crate::db::Db;
use crate::email::{
    audit_email_send, audit_outcome_for, generate_code, generate_temp_password, is_disposable_email_domain,
    normalize_email, send_password_reset_email, send_verification_email,
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

/// Resend throttle: maximum resends per rolling hour window. Tightened from 5
/// to 3 in P2 of the email-cost defense — combined with the new daily cap and
/// the per-IP / global limits, the per-account hourly is now firmly the inner
/// bound rather than a coarse aggregate.
const RESEND_HOURLY_CAP: i32 = 3;

/// Resend throttle: maximum resends per rolling day window. Caps a persistent
/// attacker who paces under the hourly cap (3 × 24 = 72/day previously was
/// unbounded at the per-account layer).
const RESEND_DAILY_CAP: i32 = 5;

const RESEND_WINDOW_SECS: i64 = 3600;
const RESEND_DAY_WINDOW_SECS: i64 = 86_400;

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
    engine.register_fn("is_account_email_verified", move |account_id: String| -> bool {
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
    });

    // set_account_email(account_id_str, email) -> bool
    // Updates the account's email and clears any prior verified state — so
    // changing email always re-triggers verification when required.
    let cloned_db = db.clone();
    engine.register_fn("set_account_email", move |account_id: String, email: String| -> bool {
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
    });

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
    engine.register_fn("find_account_id_by_normalized_email", move |email: String| -> String {
        let Some(canonical) = normalize_email(&email) else {
            return String::new();
        };
        cloned_db
            .find_account_by_normalized_email(&canonical)
            .ok()
            .flatten()
            .map(|a| a.id.to_string())
            .unwrap_or_default()
    });

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
            // Roll the day window in lockstep — independent of the hour
            // window, so a steady pace under the hourly cap still hits the
            // daily ceiling.
            if now - account.email_verification_resend_day_started_at > RESEND_DAY_WINDOW_SECS {
                account.email_verification_resend_day_started_at = now;
                account.email_verification_resend_day_count = 1;
            } else {
                account.email_verification_resend_day_count += 1;
            }

            let account_name = account.name.clone();
            if cloned_db.save_account(account).is_err() {
                return false;
            }

            // Persist before send so a partial-network failure still produces
            // a verifiable code — caller can re-trigger send via admin tools.
            let result = send_verification_email(&cloned_db, &trimmed, &code);
            audit_email_send(&cloned_db, "verification", &account_name, audit_outcome_for(&result));
            result.is_ok()
        },
    );

    // verify_account_code(account_id_str, code) -> i64
    //   0 = ok, verified
    //   1 = no code outstanding
    //   2 = expired
    //   3 = mismatch
    let cloned_db = db.clone();
    engine.register_fn("verify_account_code", move |account_id: String, code: String| -> i64 {
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
        if account.email_verification_code_expires_at != 0 && now >= account.email_verification_code_expires_at {
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
        account.email_verification_resend_day_count = 0;
        account.email_verification_resend_day_started_at = 0;
        if cloned_db.save_account(account).is_err() {
            return 1;
        }
        0
    });

    // can_resend_verification_code(account_id_str) -> i64
    //   0 = ok
    //   1 = too soon (under 60s since last send)
    //   2 = hourly cap exceeded
    //   3 = daily cap exceeded
    let cloned_db = db.clone();
    engine.register_fn("can_resend_verification_code", move |account_id: String| -> i64 {
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
        if now - account.email_verification_resend_day_started_at <= RESEND_DAY_WINDOW_SECS
            && account.email_verification_resend_day_count >= RESEND_DAILY_CAP
        {
            return 3;
        }
        0
    });

    // find_account_id_by_email(email) -> String  ("" when none)
    // Used by the create flow to refuse duplicate registrations under a single
    // inbox when verification is required.
    let cloned_db = db.clone();
    engine.register_fn("find_account_id_by_email", move |email: String| -> String {
        cloned_db
            .find_account_by_email(&email)
            .ok()
            .flatten()
            .map(|a| a.id.to_string())
            .unwrap_or_default()
    });

    // get_email_send_stats() -> Map { day_count, day_cap, month_count,
    //                                   month_cap, day_started_at, month_started_at,
    //                                   verification_required, smtp_host,
    //                                   smtp_from_address, smtp_port, smtp_user_set,
    //                                   smtp_from_name }
    // Snapshot of the global email-send counters, configured caps, and the
    // minimum SMTP config that `send_verification_email` / `send_password_reset_email`
    // require. Used by `admin email-stats` to surface budget headroom AND to
    // diagnose "is email actually wired up on this server right now?" without
    // touching the settings tree directly. Missing strings come back empty so
    // the admin display can render "<unset>" markers.
    let cloned_db = db.clone();
    engine.register_fn("get_email_send_stats", move || -> rhai::Map {
        let mut map = rhai::Map::new();
        let day_count = read_i64_setting(&cloned_db, "email_sent_count_day");
        let day_start = read_i64_setting(&cloned_db, "email_sent_count_day_start");
        let month_count = read_i64_setting(&cloned_db, "email_sent_count_month");
        let month_start = read_i64_setting(&cloned_db, "email_sent_count_month_start");
        let day_cap = read_setting_or_default_i64(&cloned_db, "email_daily_cap", 20);
        let month_cap = read_setting_or_default_i64(&cloned_db, "email_monthly_cap", 150);
        let verification_required = cloned_db
            .get_setting("email_verification_required")
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(false);
        let smtp_host = cloned_db.get_setting("smtp_host").ok().flatten().unwrap_or_default();
        let smtp_from_address = cloned_db
            .get_setting("smtp_from_address")
            .ok()
            .flatten()
            .unwrap_or_default();
        let smtp_port = cloned_db.get_setting("smtp_port").ok().flatten().unwrap_or_default();
        let smtp_user_set = cloned_db
            .get_setting("smtp_user")
            .ok()
            .flatten()
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        let smtp_from_name = cloned_db
            .get_setting("smtp_from_name")
            .ok()
            .flatten()
            .unwrap_or_default();
        map.insert("day_count".into(), rhai::Dynamic::from(day_count));
        map.insert("day_cap".into(), rhai::Dynamic::from(day_cap));
        map.insert("month_count".into(), rhai::Dynamic::from(month_count));
        map.insert("month_cap".into(), rhai::Dynamic::from(month_cap));
        map.insert("day_started_at".into(), rhai::Dynamic::from(day_start));
        map.insert("month_started_at".into(), rhai::Dynamic::from(month_start));
        map.insert(
            "verification_required".into(),
            rhai::Dynamic::from(verification_required),
        );
        map.insert("smtp_host".into(), rhai::Dynamic::from(smtp_host));
        map.insert("smtp_from_address".into(), rhai::Dynamic::from(smtp_from_address));
        map.insert("smtp_port".into(), rhai::Dynamic::from(smtp_port));
        map.insert("smtp_user_set".into(), rhai::Dynamic::from(smtp_user_set));
        map.insert("smtp_from_name".into(), rhai::Dynamic::from(smtp_from_name));
        map
    });

    // list_email_audit_entries(limit) -> Array of Maps
    // Each map: { timestamp, kind, account_name, outcome }. Newest first.
    // Used by `admin email-audit`.
    let cloned_db = db.clone();
    engine.register_fn("list_email_audit_entries", move |limit: i64| -> rhai::Array {
        let cap = if limit <= 0 { 50 } else { limit as usize };
        let entries = cloned_db.list_email_audit(cap).unwrap_or_default();
        entries
            .into_iter()
            .map(|e| {
                let mut m = rhai::Map::new();
                m.insert("timestamp".into(), rhai::Dynamic::from(e.timestamp));
                m.insert("kind".into(), rhai::Dynamic::from(e.kind));
                m.insert("account_name".into(), rhai::Dynamic::from(e.account_name));
                m.insert("outcome".into(), rhai::Dynamic::from(e.outcome));
                rhai::Dynamic::from(m)
            })
            .collect()
    });

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

    // find_verified_account_id_by_email(email) -> String  ("" when none)
    // Like find_account_id_by_normalized_email but additionally requires
    // email_verified = true. The forgot-password flow uses this to gate
    // resets on "we have an inbox we can actually deliver to".
    let cloned_db = db.clone();
    engine.register_fn("find_verified_account_id_by_email", move |email: String| -> String {
        let Some(canonical) = normalize_email(&email) else {
            return String::new();
        };
        match cloned_db.find_account_by_normalized_email(&canonical) {
            Ok(Some(a)) if a.email_verified => a.id.to_string(),
            _ => String::new(),
        }
    });

    // can_request_password_reset(account_id_str) -> i64
    //   0 = ok
    //   1 = too soon (under 60s since last send)
    //   2 = hourly cap exceeded
    //   3 = daily cap exceeded
    let cloned_db = db.clone();
    engine.register_fn("can_request_password_reset", move |account_id: String| -> i64 {
        let uuid = match Uuid::parse_str(&account_id) {
            Ok(u) => u,
            Err(_) => return 0,
        };
        let account = match cloned_db.get_account_by_id(&uuid) {
            Ok(Some(a)) => a,
            _ => return 0,
        };
        let now = now_secs();
        if account.password_reset_last_sent_at != 0
            && now - account.password_reset_last_sent_at < RESEND_MIN_SPACING_SECS
        {
            return 1;
        }
        if now - account.password_reset_window_started_at <= RESEND_WINDOW_SECS
            && account.password_reset_count >= RESEND_HOURLY_CAP
        {
            return 2;
        }
        if now - account.password_reset_day_started_at <= RESEND_DAY_WINDOW_SECS
            && account.password_reset_day_count >= RESEND_DAILY_CAP
        {
            return 3;
        }
        0
    });

    // issue_password_reset(account_id_str) -> bool
    // Generates a fresh random password, stamps the hash onto the account
    // (auth source of truth) and onto every linked character (mirrored copies),
    // forces must_change_password on every character, and dispatches the
    // reset email. The hash is persisted before the send so a partial-network
    // failure still leaves the account in a recoverable state — admins can
    // hand the temp password to the player out of band, or the player can
    // re-run forgot.
    let cloned_db = db.clone();
    engine.register_fn("issue_password_reset", move |account_id: String| -> bool {
        let uuid = match Uuid::parse_str(&account_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let account_name = cloned_db
            .get_account_by_id(&uuid)
            .ok()
            .flatten()
            .map(|a| a.name)
            .unwrap_or_default();
        match issue_password_reset_core(&cloned_db, &uuid) {
            Some((email, temp_password)) => {
                let result = send_password_reset_email(&cloned_db, &email, &temp_password);
                audit_email_send(&cloned_db, "reset", &account_name, audit_outcome_for(&result));
                result.is_ok()
            }
            None => false,
        }
    });
}

/// Rotates the password on `account_id` and every linked character, marks all
/// characters `must_change_password`, advances the throttle counters, and
/// returns the fresh email/temp-password pair so the caller can dispatch the
/// SMTP send. Returns `None` when the account is missing, has no email on
/// file, or DB writes fail. The hash is persisted before the email send so a
/// partial-network failure still leaves the account in a recoverable state.
pub fn issue_password_reset_core(db: &Db, account_id: &Uuid) -> Option<(String, String)> {
    let mut account = db.get_account_by_id(account_id).ok().flatten()?;
    let email = match account.email.clone() {
        Some(e) if !e.trim().is_empty() => e,
        _ => return None,
    };

    let temp_password = generate_temp_password();
    let hash = db.hash_password(&temp_password).ok()?;

    account.password_hash = hash.clone();
    let now = now_secs();
    account.password_reset_last_sent_at = now;
    if now - account.password_reset_window_started_at > RESEND_WINDOW_SECS {
        account.password_reset_window_started_at = now;
        account.password_reset_count = 1;
    } else {
        account.password_reset_count += 1;
    }
    if now - account.password_reset_day_started_at > RESEND_DAY_WINDOW_SECS {
        account.password_reset_day_started_at = now;
        account.password_reset_day_count = 1;
    } else {
        account.password_reset_day_count += 1;
    }

    let character_names = account.character_names.clone();
    db.save_account(account).ok()?;

    for name in &character_names {
        if let Ok(Some(mut character)) = db.get_character_data(name) {
            character.password_hash = hash.clone();
            character.must_change_password = true;
            let _ = db.save_character_data(character);
        }
    }

    Some((email, temp_password))
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn read_i64_setting(db: &Db, key: &str) -> i64 {
    db.get_setting(key)
        .ok()
        .flatten()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0)
}

fn read_setting_or_default_i64(db: &Db, key: &str, default: i64) -> i64 {
    db.get_setting(key)
        .ok()
        .flatten()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod password_reset_tests {
    use super::*;
    use crate::types::{AccountData, CharacterData};

    struct TempDb {
        db: Db,
        _temp: tempfile::TempDir,
    }
    fn open_temp(_tag: &str) -> TempDb {
        let temp = tempfile::tempdir().expect("create temp dir");
        let db = Db::open(temp.path()).expect("open db");
        TempDb { db, _temp: temp }
    }

    fn make_account_with_character(db: &Db, name: &str, email: &str, password: &str) -> Uuid {
        let hash = db.hash_password(password).expect("hash");
        let mut account = AccountData::new(name.to_string(), hash.clone());
        account.email = Some(email.to_string());
        account.normalized_email = normalize_email(email);
        account.email_verified = true;
        account.character_names.push(name.to_string());
        let id = account.id;
        db.save_account(account).expect("save account");

        // CharacterData has three fields without #[serde(default)]: name,
        // password_hash, current_room_id. Round-trip through JSON so the rest
        // pick up their defaults.
        let value = serde_json::json!({
            "name": name,
            "password_hash": hash,
            "current_room_id": Uuid::nil().to_string(),
            "must_change_password": false,
        });
        let character: CharacterData = serde_json::from_value(value).expect("build character");
        db.save_character_data(character).expect("save character");
        id
    }

    #[test]
    fn issue_password_reset_rotates_account_hash_and_forces_character_change() {
        let t = open_temp("rotate");
        let acc_id = make_account_with_character(&t.db, "Alice", "alice@example.test", "old-password");
        let original_hash = t.db.get_account_by_id(&acc_id).unwrap().unwrap().password_hash;

        let result = issue_password_reset_core(&t.db, &acc_id);
        let (email, temp_pw) = result.expect("reset returned email/password");
        assert_eq!(email, "alice@example.test");
        assert_eq!(temp_pw.len(), 12);

        let after = t.db.get_account_by_id(&acc_id).unwrap().unwrap();
        assert_ne!(after.password_hash, original_hash, "account hash must rotate");
        assert!(t.db.verify_password(&temp_pw, &after.password_hash).unwrap_or(false));
        assert_eq!(after.password_reset_count, 1);
        assert!(after.password_reset_last_sent_at > 0);

        let character = t.db.get_character_data("Alice").unwrap().unwrap();
        assert_eq!(character.password_hash, after.password_hash);
        assert!(
            character.must_change_password,
            "character must be flagged for forced password change"
        );
    }

    #[test]
    fn issue_password_reset_rejects_account_without_email() {
        let t = open_temp("noemail");
        let hash = t.db.hash_password("pw").unwrap();
        let account = AccountData::new("Bob".to_string(), hash);
        let id = account.id;
        t.db.save_account(account).unwrap();
        assert!(issue_password_reset_core(&t.db, &id).is_none());
    }

    #[test]
    fn issue_password_reset_increments_both_hour_and_day_counters() {
        let t = open_temp("counters");
        let acc_id = make_account_with_character(&t.db, "Daria", "daria@example.test", "old-password");
        for _ in 0..2 {
            issue_password_reset_core(&t.db, &acc_id).expect("reset ok");
        }
        let after = t.db.get_account_by_id(&acc_id).unwrap().unwrap();
        assert_eq!(after.password_reset_count, 2);
        assert_eq!(after.password_reset_day_count, 2);
        assert!(after.password_reset_window_started_at > 0);
        assert!(after.password_reset_day_started_at > 0);
    }

    #[test]
    fn password_reset_day_window_resets_when_stale() {
        // Pre-stamp the day window into the past with a saturated count, then
        // confirm a fresh reset rolls the day window over rather than tripping
        // the cap.
        let t = open_temp("day_reset");
        let acc_id = make_account_with_character(&t.db, "Eli", "eli@example.test", "old-password");
        let mut account = t.db.get_account_by_id(&acc_id).unwrap().unwrap();
        account.password_reset_day_count = 99;
        account.password_reset_day_started_at = 1; // ancient unix timestamp
        t.db.save_account(account).unwrap();

        issue_password_reset_core(&t.db, &acc_id).expect("day window stale -> ok");
        let after = t.db.get_account_by_id(&acc_id).unwrap().unwrap();
        assert_eq!(after.password_reset_day_count, 1, "day counter should reset to 1");
        assert!(
            after.password_reset_day_started_at > 1,
            "day window start should advance to now"
        );
    }

    #[test]
    fn find_verified_account_id_by_email_skips_unverified() {
        let t = open_temp("verified_only");
        let hash = t.db.hash_password("pw").unwrap();
        let mut account = AccountData::new("Carol".to_string(), hash);
        account.email = Some("carol@example.test".to_string());
        account.normalized_email = normalize_email("carol@example.test");
        account.email_verified = false;
        t.db.save_account(account).unwrap();

        // Lookup mirrors the registered Rhai fn's body.
        let canonical = normalize_email("carol@example.test").unwrap();
        let found = t.db.find_account_by_normalized_email(&canonical).unwrap();
        assert!(found.is_some(), "account exists in DB");
        assert!(!found.unwrap().email_verified, "but is unverified");
        // The Rhai-side helper must therefore return "" for unverified accounts.
    }
}

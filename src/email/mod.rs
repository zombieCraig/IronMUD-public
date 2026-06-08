//! Email delivery for the optional email-verification flow.
//!
//! Single responsibility: given a destination address and a 6-digit code,
//! send a plain-text verification email via SMTP. SMTP credentials and
//! `From:` config are read from the settings tree on every call so admins
//! can rotate credentials without restarting the server.
//!
//! Off by default. Only invoked when `email_verification_required = true`.

use crate::chat::{ChatMessage, ChatSender};
use crate::db::{Db, EmailAuditEntry};
use lettre::{Message, SmtpTransport, Transport};
use lettre::transport::smtp::authentication::Credentials;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::warn;

#[derive(Debug)]
pub enum EmailError {
    /// SMTP host or `From:` address not configured. The verification flow is
    /// expected to fail closed and refuse account creation in this state.
    MissingConfig(&'static str),
    /// Either the recipient or sender address failed to parse.
    BadAddress(String),
    /// Lettre couldn't build the message body (rare — usually a header issue).
    BuildFailure(String),
    /// SMTP connection or relay rejected the send.
    SmtpFailure(String),
    /// The daily or monthly send budget is exhausted. The caller should treat
    /// this as a refusal — never retry, and never surface specifics to the
    /// user since that would let an attacker confirm they hit the cap.
    QuotaExceeded(&'static str),
}

impl std::fmt::Display for EmailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmailError::MissingConfig(key) => write!(f, "missing SMTP config: {}", key),
            EmailError::BadAddress(s) => write!(f, "bad email address: {}", s),
            EmailError::BuildFailure(s) => write!(f, "could not build email: {}", s),
            EmailError::SmtpFailure(s) => write!(f, "SMTP send failed: {}", s),
            EmailError::QuotaExceeded(scope) => write!(f, "{} email quota exceeded", scope),
        }
    }
}

impl std::error::Error for EmailError {}

/// Hardcoded fallback used when `scripts/data/email/verification.txt` is
/// missing. Keeps the slice deliverable on a fresh checkout that doesn't yet
/// have the template file. `{{code}}` is the only substitution.
const DEFAULT_TEMPLATE: &str = "Welcome to IronMUD!\n\nYour verification code is: {{code}}\n\nEnter this code in the game to complete account creation. The code expires shortly; if it does, type 'resend' to request a new one.\n\nIf you didn't create this account, ignore this email.\n";

const TEMPLATE_PATH: &str = "scripts/data/email/verification.txt";

const DEFAULT_PASSWORD_RESET_TEMPLATE: &str = "Hello,\n\nSomeone (hopefully you) requested a password reset for the IronMUD account linked to this email address.\n\nYour new temporary password is: {{password}}\n\nLog in with this password and you will be required to choose a new one immediately. If you didn't request this, change your password now or contact an administrator.\n";

const PASSWORD_RESET_TEMPLATE_PATH: &str = "scripts/data/email/password_reset.txt";

/// Default monthly send cap. SES free tier sits at 200/mo on a fresh account;
/// 150 leaves headroom for legitimate rebuilds + admin sends without burning
/// the whole budget under attack. Override via setting `email_monthly_cap`.
const DEFAULT_MONTHLY_CAP: u64 = 150;

/// Default daily send cap. 20 covers normal organic signup + reset volume on
/// a small server while making it expensive to attack the monthly budget in
/// any single 24-hour window. Override via setting `email_daily_cap`.
const DEFAULT_DAILY_CAP: u64 = 20;

const SECS_PER_DAY: i64 = 86_400;
const SECS_PER_MONTH: i64 = 30 * SECS_PER_DAY;

/// Channel handle used to push admin warnings (e.g. daily-cap hit) to the
/// chat bridge. Initialized once at startup by the script wiring; remains
/// `None` in test/headless setups where chat isn't configured. Reads are
/// lock-free; the `OnceLock` exists only so the email module doesn't have
/// to plumb a sender through every callsite.
static CHAT_SENDER: OnceLock<ChatSender> = OnceLock::new();

/// Wire the chat bridge into the email module. Idempotent: subsequent calls
/// after the first are silently ignored. Called from the chat-sender
/// registration path during server boot.
pub fn set_email_chat_sender(sender: ChatSender) {
    let _ = CHAT_SENDER.set(sender);
}

fn broadcast_admin_warning(message: &str) {
    if let Some(tx) = CHAT_SENDER.get() {
        let _ = tx.send(ChatMessage::Broadcast(message.to_string()));
    }
}

/// Map an EmailError variant to the audit-outcome string. "sent" is the
/// success case (returned by [`audit_outcome_for`] when the result is Ok).
pub fn audit_outcome_for(result: &Result<(), EmailError>) -> &'static str {
    match result {
        Ok(()) => "sent",
        Err(EmailError::QuotaExceeded("daily")) => "quota_daily",
        Err(EmailError::QuotaExceeded("monthly")) => "quota_monthly",
        Err(EmailError::QuotaExceeded(_)) => "quota_other",
        Err(EmailError::MissingConfig(_)) => "config_missing",
        Err(EmailError::BadAddress(_)) => "bad_address",
        Err(EmailError::BuildFailure(_)) => "build_failed",
        Err(EmailError::SmtpFailure(_)) => "smtp_failed",
    }
}

/// Append one row to the email audit ring. Failure to write the audit row
/// is intentionally non-fatal — we don't want a sled hiccup to also drop the
/// email.
pub fn audit_email_send(db: &Db, kind: &str, account_name: &str, outcome: &str) {
    let entry = EmailAuditEntry {
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
        kind: kind.to_string(),
        account_name: account_name.to_string(),
        outcome: outcome.to_string(),
    };
    if let Err(e) = db.record_email_audit(entry) {
        warn!("failed to record email audit row: {}", e);
    }
}

/// Fixed body for the admin SMTP test message. No substitution tokens — the
/// only purpose is to prove a message can actually reach an inbox.
const TEST_EMAIL_BODY: &str = "This is a test message from IronMUD.\n\nIf you are reading this, the server's SMTP configuration is working: verification codes and password-reset emails can be delivered.\n\nNo action is required.\n";

/// Read the SMTP config from the settings tree, build the message, and relay it
/// over STARTTLS. Shared by every public sender — the callers differ only in
/// quota handling, subject, and body. Reads config on every call so admins can
/// rotate credentials without restarting.
fn deliver(db: &Db, to_address: &str, subject: &str, body: String) -> Result<(), EmailError> {
    let host = read_setting_required(db, "smtp_host", "smtp_host")?;
    let port = db
        .get_setting_or_default("smtp_port", "587")
        .map_err(|e| EmailError::SmtpFailure(format!("settings read: {}", e)))?
        .parse::<u16>()
        .unwrap_or(587);
    let user = db
        .get_setting("smtp_user")
        .map_err(|e| EmailError::SmtpFailure(format!("settings read: {}", e)))?
        .unwrap_or_default();
    let pass = db
        .get_setting("smtp_pass")
        .map_err(|e| EmailError::SmtpFailure(format!("settings read: {}", e)))?
        .unwrap_or_default();
    let from_address = read_setting_required(db, "smtp_from_address", "smtp_from_address")?;
    let from_name = db
        .get_setting_or_default("smtp_from_name", "IronMUD")
        .map_err(|e| EmailError::SmtpFailure(format!("settings read: {}", e)))?;

    let from_mbox = format!("{} <{}>", from_name, from_address)
        .parse::<lettre::message::Mailbox>()
        .map_err(|e| EmailError::BadAddress(format!("from: {}", e)))?;
    let to_mbox = to_address
        .parse::<lettre::message::Mailbox>()
        .map_err(|e| EmailError::BadAddress(format!("to: {}", e)))?;

    let email = Message::builder()
        .from(from_mbox)
        .to(to_mbox)
        .subject(subject)
        .body(body)
        .map_err(|e| EmailError::BuildFailure(e.to_string()))?;

    let mut builder = SmtpTransport::starttls_relay(&host)
        .map_err(|e| EmailError::SmtpFailure(format!("relay setup: {}", e)))?
        .port(port);
    if !user.is_empty() {
        builder = builder.credentials(Credentials::new(user, pass));
    }
    let mailer = builder.build();

    mailer
        .send(&email)
        .map_err(|e| EmailError::SmtpFailure(e.to_string()))?;
    Ok(())
}

/// Send a 6-digit verification code to `to_address`. Reads SMTP and `From:`
/// configuration from the settings tree on every call.
pub fn send_verification_email(db: &Db, to_address: &str, code: &str) -> Result<(), EmailError> {
    check_and_increment_send_quota(db)?;
    let subject = db
        .get_setting_or_default("email_verification_subject", "Verify your IronMUD account")
        .map_err(|e| EmailError::SmtpFailure(format!("settings read: {}", e)))?;
    let body = load_template().replace("{{code}}", code);
    deliver(db, to_address, &subject, body)
}

/// Send a freshly generated temporary password to `to_address`. Reads SMTP and
/// `From:` configuration from the settings tree on every call. Mirrors
/// `send_verification_email` — only the subject, template, and substitution
/// token differ.
pub fn send_password_reset_email(
    db: &Db,
    to_address: &str,
    password: &str,
) -> Result<(), EmailError> {
    check_and_increment_send_quota(db)?;
    let subject = db
        .get_setting_or_default("password_reset_subject", "Your IronMUD password has been reset")
        .map_err(|e| EmailError::SmtpFailure(format!("settings read: {}", e)))?;
    let body = load_password_reset_template().replace("{{password}}", password);
    deliver(db, to_address, &subject, body)
}

/// Send a fixed diagnostic message to `to_address` to prove SMTP delivery
/// works. Counts against the daily/monthly send budget like the other senders
/// (an admin choice — keeps a single test from masking budget pressure). The
/// caller is responsible for recording the audit row.
pub fn send_test_email(db: &Db, to_address: &str) -> Result<(), EmailError> {
    check_and_increment_send_quota(db)?;
    let subject = db
        .get_setting_or_default("email_test_subject", "IronMUD SMTP test")
        .map_err(|e| EmailError::SmtpFailure(format!("settings read: {}", e)))?;
    deliver(db, to_address, &subject, TEST_EMAIL_BODY.to_string())
}

/// Bump the global daily/monthly send counters and refuse the send if
/// either cap is exceeded. The check is read-modify-write against the
/// settings tree, which under heavy concurrent send pressure can lose an
/// increment — that lets through a few extra emails, never orders of
/// magnitude. Acceptable for a cost ceiling. The counters reset when the
/// stored window-start drifts past `SECS_PER_DAY` / `SECS_PER_MONTH` so an
/// admin running `delete_setting email_sent_count_*` is not required to
/// recover from a momentary spike.
pub fn check_and_increment_send_quota(db: &Db) -> Result<(), EmailError> {
    let monthly_cap = db
        .get_setting("email_monthly_cap")
        .map_err(|e| EmailError::SmtpFailure(format!("settings read: {}", e)))?
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_MONTHLY_CAP);
    let daily_cap = db
        .get_setting("email_daily_cap")
        .map_err(|e| EmailError::SmtpFailure(format!("settings read: {}", e)))?
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_DAILY_CAP);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let mut day_start = read_counter_i64(db, "email_sent_count_day_start");
    let mut day_count = read_counter_u64(db, "email_sent_count_day");
    if day_start == 0 || now - day_start >= SECS_PER_DAY {
        day_start = now;
        day_count = 0;
    }

    let mut month_start = read_counter_i64(db, "email_sent_count_month_start");
    let mut month_count = read_counter_u64(db, "email_sent_count_month");
    if month_start == 0 || now - month_start >= SECS_PER_MONTH {
        month_start = now;
        month_count = 0;
    }

    if day_count >= daily_cap {
        // Fire a once-per-day admin warning so ops sees the cap getting
        // hit while it's happening, not via a billing alert later. Tracked
        // via a settings flag that's stamped with the day-window start, so
        // the warning re-arms when the day rolls over.
        let warn_stamp = read_counter_i64(db, "email_quota_warning_day_started_at");
        if warn_stamp != day_start {
            warn!(
                daily_cap, monthly_cap, day_count, month_count,
                "email daily cap hit — refusing further sends until tomorrow"
            );
            broadcast_admin_warning(&format!(
                "[email] Daily send cap reached ({}/{}). No more verification or password-reset emails will be sent today.",
                day_count, daily_cap
            ));
            let _ = db.set_setting(
                "email_quota_warning_day_started_at",
                &day_start.to_string(),
            );
        }
        return Err(EmailError::QuotaExceeded("daily"));
    }
    if month_count >= monthly_cap {
        let warn_stamp = read_counter_i64(db, "email_quota_warning_month_started_at");
        if warn_stamp != month_start {
            warn!(
                daily_cap, monthly_cap, day_count, month_count,
                "email monthly cap hit — refusing further sends until next window"
            );
            broadcast_admin_warning(&format!(
                "[email] Monthly send cap reached ({}/{}). Email-driven flows are paused until the window resets.",
                month_count, monthly_cap
            ));
            let _ = db.set_setting(
                "email_quota_warning_month_started_at",
                &month_start.to_string(),
            );
        }
        return Err(EmailError::QuotaExceeded("monthly"));
    }

    day_count += 1;
    month_count += 1;
    write_counter(db, "email_sent_count_day", day_count);
    write_counter(db, "email_sent_count_day_start", day_start as u64);
    write_counter(db, "email_sent_count_month", month_count);
    write_counter(db, "email_sent_count_month_start", month_start as u64);
    Ok(())
}

fn read_counter_u64(db: &Db, key: &str) -> u64 {
    db.get_setting(key)
        .ok()
        .flatten()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0)
}

fn read_counter_i64(db: &Db, key: &str) -> i64 {
    db.get_setting(key)
        .ok()
        .flatten()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0)
}

fn write_counter(db: &Db, key: &str, value: u64) {
    let _ = db.set_setting(key, &value.to_string());
}

fn read_setting_required(
    db: &Db,
    key: &str,
    label: &'static str,
) -> Result<String, EmailError> {
    let value = db
        .get_setting(key)
        .map_err(|e| EmailError::SmtpFailure(format!("settings read: {}", e)))?
        .unwrap_or_default();
    if value.trim().is_empty() {
        Err(EmailError::MissingConfig(label))
    } else {
        Ok(value)
    }
}

fn load_template() -> String {
    if Path::new(TEMPLATE_PATH).exists() {
        if let Ok(s) = fs::read_to_string(TEMPLATE_PATH) {
            return s;
        }
    }
    DEFAULT_TEMPLATE.to_string()
}

fn load_password_reset_template() -> String {
    if Path::new(PASSWORD_RESET_TEMPLATE_PATH).exists() {
        if let Ok(s) = fs::read_to_string(PASSWORD_RESET_TEMPLATE_PATH) {
            return s;
        }
    }
    DEFAULT_PASSWORD_RESET_TEMPLATE.to_string()
}

/// Generate a zero-padded 6-digit code. ~1 in 10^6 brute-force chance per
/// guess; rate limiting plus connection-level throttle covers what's left.
pub fn generate_code() -> String {
    use rand::Rng;
    let n: u32 = rand::thread_rng().gen_range(0..1_000_000);
    format!("{:06}", n)
}

/// Charset for temporary passwords. Excludes visually ambiguous glyphs
/// (`0/O`, `1/l/I`) so a player typing the password back from their email
/// client doesn't trip on font-dependent substitutions.
const TEMP_PASSWORD_CHARSET: &[u8] =
    b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";

/// Random 12-character temp password. ~71 bits of entropy from a 56-char
/// alphabet; combined with per-account throttling and immediate forced
/// rotation on first login, brute-forcing the email window is impractical.
pub fn generate_temp_password() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..12)
        .map(|_| {
            let idx = rng.gen_range(0..TEMP_PASSWORD_CHARSET.len());
            TEMP_PASSWORD_CHARSET[idx] as char
        })
        .collect()
}

// ===========================================================================
// Email normalization (Gmail-style canonical form)
// ===========================================================================

/// Providers that ignore dots and `+tag` suffixes in the local part. We only
/// canonicalize for these — anywhere else, dots are significant and stripping
/// them creates false positives. Conservative on purpose.
const KNOWN_DOT_PLUS_PROVIDERS: &[&str] = &["gmail.com", "googlemail.com"];

/// Canonicalize an email for evasion-detection comparisons. Returns `None`
/// if the input doesn't contain `@`. For Gmail/Googlemail: strip dots and
/// `+suffix`, rewrite `googlemail.com` to `gmail.com`. Other domains pass
/// through with just trim + lowercase.
pub fn normalize_email(raw: &str) -> Option<String> {
    let trimmed = raw.trim().to_lowercase();
    let (local, domain) = trimmed.split_once('@')?;
    if KNOWN_DOT_PLUS_PROVIDERS.contains(&domain) {
        let local_no_plus = local.split('+').next().unwrap_or("");
        let local_no_dots: String = local_no_plus.chars().filter(|c| *c != '.').collect();
        let canonical_domain = if domain == "googlemail.com" { "gmail.com" } else { domain };
        if local_no_dots.is_empty() {
            return None;
        }
        Some(format!("{}@{}", local_no_dots, canonical_domain))
    } else {
        if local.is_empty() || domain.is_empty() {
            return None;
        }
        Some(trimmed)
    }
}

// ===========================================================================
// Disposable-domain blocklist
// ===========================================================================

const DISPOSABLE_DOMAINS_PATH: &str = "scripts/data/email/disposable_domains.txt";

/// Cached lazy-loaded blocklist. ~3000 lines parsed once on first access; an
/// admin who edits the file needs to restart (or we add a hot-reload later —
/// not worth the complexity for a near-static list).
static DISPOSABLE_DOMAINS: OnceLock<HashSet<String>> = OnceLock::new();

fn load_disposable_domains() -> HashSet<String> {
    let mut set = HashSet::new();
    if let Ok(contents) = fs::read_to_string(DISPOSABLE_DOMAINS_PATH) {
        for line in contents.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            set.insert(trimmed.to_lowercase());
        }
    }
    set
}

fn disposable_domains() -> &'static HashSet<String> {
    DISPOSABLE_DOMAINS.get_or_init(load_disposable_domains)
}

/// True if the email's domain is on the disposable-provider blocklist.
/// Returns false for malformed input — the SMTP send path is the real
/// validator, this is just an early-reject.
pub fn is_disposable_email_domain(email: &str) -> bool {
    let trimmed = email.trim().to_lowercase();
    let Some((_, domain)) = trimmed.split_once('@') else {
        return false;
    };
    disposable_domains().contains(domain)
}

#[cfg(test)]
mod normalization_tests {
    use super::*;

    #[test]
    fn gmail_strips_dots_and_plus() {
        assert_eq!(
            normalize_email("Test.User+spam@Gmail.com").as_deref(),
            Some("testuser@gmail.com")
        );
    }

    #[test]
    fn googlemail_rewrites_to_gmail() {
        assert_eq!(
            normalize_email("foo.bar@googlemail.com").as_deref(),
            Some("foobar@gmail.com")
        );
    }

    #[test]
    fn non_gmail_passthrough() {
        assert_eq!(
            normalize_email("test.user@mail.example").as_deref(),
            Some("test.user@mail.example")
        );
    }

    #[test]
    fn missing_at_returns_none() {
        assert!(normalize_email("notanemail").is_none());
    }
}

#[cfg(test)]
mod quota_tests {
    use super::*;

    struct TempDb {
        db: Db,
        _temp: tempfile::TempDir,
    }
    fn open_temp(_tag: &str) -> TempDb {
        let temp = tempfile::tempdir().expect("create temp dir");
        let db = Db::open(temp.path()).expect("open db");
        TempDb { db, _temp: temp }
    }

    #[test]
    fn quota_under_caps_increments_and_passes() {
        let t = open_temp("under");
        // Use small custom caps so the test is stable.
        t.db.set_setting("email_daily_cap", "3").unwrap();
        t.db.set_setting("email_monthly_cap", "10").unwrap();
        for _ in 0..3 {
            check_and_increment_send_quota(&t.db).expect("under caps");
        }
        assert_eq!(read_counter_u64(&t.db, "email_sent_count_day"), 3);
        assert_eq!(read_counter_u64(&t.db, "email_sent_count_month"), 3);
    }

    #[test]
    fn quota_refuses_when_daily_cap_hit() {
        let t = open_temp("daily");
        t.db.set_setting("email_daily_cap", "2").unwrap();
        t.db.set_setting("email_monthly_cap", "100").unwrap();
        check_and_increment_send_quota(&t.db).unwrap();
        check_and_increment_send_quota(&t.db).unwrap();
        let err = check_and_increment_send_quota(&t.db);
        match err {
            Err(EmailError::QuotaExceeded("daily")) => {}
            other => panic!("expected QuotaExceeded(daily), got {:?}", other),
        }
    }

    #[test]
    fn quota_refuses_when_monthly_cap_hit() {
        let t = open_temp("monthly");
        // Daily wide open, monthly tight.
        t.db.set_setting("email_daily_cap", "100").unwrap();
        t.db.set_setting("email_monthly_cap", "2").unwrap();
        check_and_increment_send_quota(&t.db).unwrap();
        check_and_increment_send_quota(&t.db).unwrap();
        let err = check_and_increment_send_quota(&t.db);
        match err {
            Err(EmailError::QuotaExceeded("monthly")) => {}
            other => panic!("expected QuotaExceeded(monthly), got {:?}", other),
        }
    }

    #[test]
    fn quota_resets_when_window_start_is_stale() {
        let t = open_temp("reset");
        t.db.set_setting("email_daily_cap", "2").unwrap();
        t.db.set_setting("email_monthly_cap", "100").unwrap();
        // Pre-stamp a counter at the cap with a window start one day ago.
        let yesterday = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            - SECS_PER_DAY
            - 60;
        t.db.set_setting("email_sent_count_day", "2").unwrap();
        t.db.set_setting("email_sent_count_day_start", &yesterday.to_string())
            .unwrap();

        // The next check should reset the day window and succeed.
        check_and_increment_send_quota(&t.db).expect("day window should have reset");
        assert_eq!(read_counter_u64(&t.db, "email_sent_count_day"), 1);
    }
}

#[cfg(test)]
mod temp_password_tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn generate_temp_password_length_and_charset() {
        let pw = generate_temp_password();
        assert_eq!(pw.len(), 12);
        for c in pw.bytes() {
            assert!(
                TEMP_PASSWORD_CHARSET.contains(&c),
                "char {:?} not in safe charset",
                c as char
            );
        }
    }

    #[test]
    fn generate_temp_password_unlikely_collision() {
        let mut seen = HashSet::new();
        for _ in 0..100 {
            seen.insert(generate_temp_password());
        }
        assert_eq!(seen.len(), 100, "12-char passwords collided in 100 draws");
    }
}

//! Email delivery for the optional email-verification flow.
//!
//! Single responsibility: given a destination address and a 6-digit code,
//! send a plain-text verification email via SMTP. SMTP credentials and
//! `From:` config are read from the settings tree on every call so admins
//! can rotate credentials without restarting the server.
//!
//! Off by default. Only invoked when `email_verification_required = true`.

use crate::db::Db;
use lettre::{Message, SmtpTransport, Transport};
use lettre::transport::smtp::authentication::Credentials;
use std::fs;
use std::path::Path;

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
}

impl std::fmt::Display for EmailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmailError::MissingConfig(key) => write!(f, "missing SMTP config: {}", key),
            EmailError::BadAddress(s) => write!(f, "bad email address: {}", s),
            EmailError::BuildFailure(s) => write!(f, "could not build email: {}", s),
            EmailError::SmtpFailure(s) => write!(f, "SMTP send failed: {}", s),
        }
    }
}

impl std::error::Error for EmailError {}

/// Hardcoded fallback used when `scripts/data/email/verification.txt` is
/// missing. Keeps the slice deliverable on a fresh checkout that doesn't yet
/// have the template file. `{{code}}` is the only substitution.
const DEFAULT_TEMPLATE: &str = "Welcome to IronMUD!\n\nYour verification code is: {{code}}\n\nEnter this code in the game to complete account creation. The code expires shortly; if it does, type 'resend' to request a new one.\n\nIf you didn't create this account, ignore this email.\n";

const TEMPLATE_PATH: &str = "scripts/data/email/verification.txt";

/// Send a 6-digit verification code to `to_address`. Reads SMTP and `From:`
/// configuration from the settings tree on every call.
pub fn send_verification_email(db: &Db, to_address: &str, code: &str) -> Result<(), EmailError> {
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
    let subject = db
        .get_setting_or_default("email_verification_subject", "Verify your IronMUD account")
        .map_err(|e| EmailError::SmtpFailure(format!("settings read: {}", e)))?;

    let body = load_template().replace("{{code}}", code);

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

/// Generate a zero-padded 6-digit code. ~1 in 10^6 brute-force chance per
/// guess; rate limiting plus connection-level throttle covers what's left.
pub fn generate_code() -> String {
    use rand::Rng;
    let n: u32 = rand::thread_rng().gen_range(0..1_000_000);
    format!("{:06}", n)
}

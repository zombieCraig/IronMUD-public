//! Login banner rendering. The banner template at `assets/banner.txt` carries
//! two placeholders — `{{CREATE_LINE}}` and `{{FORGOT_LINE}}` — that swap
//! between two-arg and three-arg create syntax (and add a `forgot` line)
//! depending on whether email verification is required.

use crate::db::Db;

const BANNER_PATH: &str = "assets/banner.txt";

/// ANSI-formatted banner line for `create [name] [password] - New account`.
const CREATE_LINE_NO_EMAIL: &str =
    "\x1b[93m     create\x1b[0m [name] [password] \x1b[90m- New account\x1b[0m\n";

/// ANSI-formatted banner line for `create [name] [password] [email] - New account`.
const CREATE_LINE_WITH_EMAIL: &str =
    "\x1b[93m     create\x1b[0m [name] [password] [email] \x1b[90m- New account\x1b[0m\n";

/// ANSI-formatted banner line for `forgot [email] - Email a new password`.
/// Emitted only when email verification is required.
const FORGOT_LINE_WITH_EMAIL: &str =
    "\x1b[93m     forgot\x1b[0m [email]            \x1b[90m- Email a new password\x1b[0m\n";

/// Render the welcome banner. Reads `assets/banner.txt` and substitutes the
/// `{{CREATE_LINE}}` / `{{FORGOT_LINE}}` placeholders based on whether email
/// verification is required. Returns the empty string if the banner file is
/// unreadable — caller treats that as "no banner to send".
pub fn render_login_banner(db: &Db) -> String {
    let template = match std::fs::read_to_string(BANNER_PATH) {
        Ok(s) => s,
        Err(_) => return String::new(),
    };
    let email_required = db
        .get_setting("email_verification_required")
        .ok()
        .flatten()
        .map(|v| v == "true")
        .unwrap_or(false);
    let (create_line, forgot_line) = if email_required {
        (CREATE_LINE_WITH_EMAIL, FORGOT_LINE_WITH_EMAIL)
    } else {
        (CREATE_LINE_NO_EMAIL, "")
    };
    template
        .replace("{{CREATE_LINE}}", create_line.trim_end_matches('\n'))
        .replace("{{FORGOT_LINE}}", forgot_line)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDb {
        db: Db,
        path: String,
    }
    impl Drop for TempDb {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
    fn open_temp(tag: &str) -> TempDb {
        let path = format!(
            "test_banner_{}_{}_{}.db",
            tag,
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        );
        let _ = std::fs::remove_dir_all(&path);
        let db = Db::open(&path).expect("open db");
        TempDb { db, path }
    }

    #[test]
    fn banner_default_off_shows_two_arg_create_no_forgot() {
        let t = open_temp("off");
        let banner = render_login_banner(&t.db);
        // create line is the no-email variant
        assert!(banner.contains("[name] [password] \x1b[90m- New account"));
        // no email arg, no forgot line
        assert!(!banner.contains("[name] [password] [email]"));
        assert!(!banner.contains("forgot"));
        // placeholders all consumed
        assert!(!banner.contains("{{CREATE_LINE}}"));
        assert!(!banner.contains("{{FORGOT_LINE}}"));
    }

    #[test]
    fn banner_with_verification_shows_three_arg_create_and_forgot() {
        let t = open_temp("on");
        t.db.set_setting("email_verification_required", "true")
            .expect("set");
        let banner = render_login_banner(&t.db);
        assert!(banner.contains("[name] [password] [email]"));
        assert!(banner.contains("forgot"));
        assert!(banner.contains("- Email a new password"));
        assert!(!banner.contains("{{CREATE_LINE}}"));
        assert!(!banner.contains("{{FORGOT_LINE}}"));
    }
}

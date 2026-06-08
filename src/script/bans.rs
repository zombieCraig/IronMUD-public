//! Rhai surface for the ban-tooling slice. Covers account bans (with
//! structured `BanRecord` + lazy expiry), site (IP) bans gated at the TCP
//! accept loop, IP tracking for evasion detection, and the `admin alts`
//! correlation query.
//!
//! Kept separate from `script/accounts.rs` for grep-ability — when something
//! breaks in ban tooling, this is the only file with `ban_` prefix fns.

use crate::SharedConnections;
use crate::db::Db;
use crate::types::{BanRecord, SiteBanRecord};
use rhai::{Array, Dynamic, Engine, Map};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Lookback window for the `admin alts` IP correlation. Mirrors the
/// `ip_account_history` GC threshold so we don't surface entries the tree
/// has already silently dropped.
const ALTS_IP_LOOKBACK_SECS: i64 = 30 * 24 * 3600;

pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections) {
    // ban_account(account_id_str, reason, banned_by_name, expires_at_or_zero) -> bool
    //   expires_at_or_zero == 0 → permanent ban.
    let cloned_db = db.clone();
    engine.register_fn(
        "ban_account",
        move |account_id: String, reason: String, banned_by: String, expires_at_or_zero: i64| -> bool {
            let uuid = match Uuid::parse_str(&account_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut account = match cloned_db.get_account_by_id(&uuid) {
                Ok(Some(a)) => a,
                _ => return false,
            };
            let expires_at = if expires_at_or_zero <= 0 {
                None
            } else {
                Some(expires_at_or_zero)
            };
            account.is_banned = true;
            account.ban_record = Some(BanRecord {
                reason: reason.trim().to_string(),
                banned_by: banned_by.trim().to_string(),
                banned_at: now_secs(),
                expires_at,
            });
            cloned_db.save_account(account).is_ok()
        },
    );

    // unban_account(account_id_str) -> bool
    let cloned_db = db.clone();
    engine.register_fn("unban_account", move |account_id: String| -> bool {
        let uuid = match Uuid::parse_str(&account_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let mut account = match cloned_db.get_account_by_id(&uuid) {
            Ok(Some(a)) => a,
            _ => return false,
        };
        account.is_banned = false;
        account.ban_record = None;
        cloned_db.save_account(account).is_ok()
    });

    // check_account_ban(account_id_str) -> Map | ()
    //   () when not banned (or banned status was just lifted due to expiry).
    //   Otherwise: #{ reason, banned_by, banned_at, expires_at, expired:false,
    //                 has_record:bool }.
    //   A legacy boolean-only ban (`is_banned=true` but `ban_record=None`) is
    //   surfaced with `has_record=false` so callers can show the generic
    //   message without leaking that no metadata exists.
    let cloned_db = db.clone();
    engine.register_fn("check_account_ban", move |account_id: String| -> Dynamic {
        let uuid = match Uuid::parse_str(&account_id) {
            Ok(u) => u,
            Err(_) => return Dynamic::UNIT,
        };
        let mut account = match cloned_db.get_account_by_id(&uuid) {
            Ok(Some(a)) => a,
            _ => return Dynamic::UNIT,
        };
        if !account.is_banned && account.ban_record.is_none() {
            return Dynamic::UNIT;
        }
        let now = now_secs();
        if let Some(record) = account.ban_record.clone() {
            if let Some(expires) = record.expires_at {
                if now >= expires {
                    // Lazy lift on read.
                    account.is_banned = false;
                    account.ban_record = None;
                    let _ = cloned_db.save_account(account);
                    return Dynamic::UNIT;
                }
            }
            let mut m = Map::new();
            m.insert("reason".into(), Dynamic::from(record.reason));
            m.insert("banned_by".into(), Dynamic::from(record.banned_by));
            m.insert("banned_at".into(), Dynamic::from(record.banned_at));
            match record.expires_at {
                Some(e) => m.insert("expires_at".into(), Dynamic::from(e)),
                None => m.insert("expires_at".into(), Dynamic::UNIT),
            };
            m.insert("expired".into(), Dynamic::from(false));
            m.insert("has_record".into(), Dynamic::from(true));
            return Dynamic::from_map(m);
        }
        // Legacy: is_banned=true with no record.
        let mut m = Map::new();
        m.insert("reason".into(), Dynamic::from(String::new()));
        m.insert("banned_by".into(), Dynamic::from(String::new()));
        m.insert("banned_at".into(), Dynamic::from(0_i64));
        m.insert("expires_at".into(), Dynamic::UNIT);
        m.insert("expired".into(), Dynamic::from(false));
        m.insert("has_record".into(), Dynamic::from(false));
        Dynamic::from_map(m)
    });

    // format_ban_message(ban_map) -> String
    // Renders the user-facing line shown at login. Generic when the account
    // pre-dates the metadata slice (no record); detailed otherwise.
    engine.register_fn("format_ban_message", |ban_map: Map| -> String {
        let has_record = ban_map
            .get("has_record")
            .and_then(|d| d.as_bool().ok())
            .unwrap_or(false);
        if !has_record {
            return "This account is suspended. Contact an administrator.".to_string();
        }
        let reason = ban_map
            .get("reason")
            .and_then(|d| d.clone().into_string().ok())
            .unwrap_or_default();
        let expires_at = ban_map.get("expires_at").and_then(|d| d.as_int().ok());
        let mut msg = "This account is suspended".to_string();
        if !reason.is_empty() {
            msg.push_str(": ");
            msg.push_str(&reason);
            if !msg.ends_with('.') {
                msg.push('.');
            }
        } else {
            msg.push('.');
        }
        match expires_at {
            Some(t) => msg.push_str(&format!(" Lifts at {}.", format_time(t))),
            None => msg.push_str(" Permanent."),
        }
        msg
    });

    // add_site_ban(ip, reason, banned_by, expires_at_or_zero) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "add_site_ban",
        move |ip: String, reason: String, banned_by: String, expires_at_or_zero: i64| -> bool {
            let trimmed_ip = ip.trim().to_lowercase();
            if trimmed_ip.is_empty() {
                return false;
            }
            let expires_at = if expires_at_or_zero <= 0 {
                None
            } else {
                Some(expires_at_or_zero)
            };
            let record = SiteBanRecord {
                ip: trimmed_ip,
                reason: reason.trim().to_string(),
                banned_by: banned_by.trim().to_string(),
                banned_at: now_secs(),
                expires_at,
            };
            cloned_db.put_site_ban(&record).is_ok()
        },
    );

    // remove_site_ban(ip) -> bool
    let cloned_db = db.clone();
    engine.register_fn("remove_site_ban", move |ip: String| -> bool {
        cloned_db.remove_site_ban(&ip).unwrap_or(false)
    });

    // is_ip_site_banned(ip) -> bool   (quick check; lazy-clears expired rows)
    let cloned_db = db.clone();
    engine.register_fn("is_ip_site_banned", move |ip: String| -> bool {
        cloned_db.get_site_ban(&ip).ok().flatten().is_some()
    });

    // list_site_bans() -> Array of Map
    let cloned_db = db.clone();
    engine.register_fn("list_site_bans", move || -> Array {
        let mut out = Array::new();
        let bans = match cloned_db.list_site_bans() {
            Ok(v) => v,
            Err(_) => return out,
        };
        for r in bans {
            out.push(Dynamic::from_map(site_ban_to_map(&r)));
        }
        out
    });

    // get_connection_ip(connection_id) -> String  ("" when not found)
    let conns = connections.clone();
    engine.register_fn("get_connection_ip", move |connection_id: String| -> String {
        let cid = match Uuid::parse_str(&connection_id) {
            Ok(u) => u,
            Err(_) => return String::new(),
        };
        let conns_guard = conns.lock().unwrap();
        match conns_guard.get(&cid) {
            Some(s) => s.addr.ip().to_string(),
            None => String::new(),
        }
    });

    // record_account_ip(account_id_str, ip) -> bool
    // Stamps both `last_login_ip` on the account row AND a row in the
    // `ip_account_history` reverse index so `admin alts` can find it.
    let cloned_db = db.clone();
    engine.register_fn("record_account_ip", move |account_id: String, ip: String| -> bool {
        let uuid = match Uuid::parse_str(&account_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let trimmed_ip = ip.trim().to_lowercase();
        if trimmed_ip.is_empty() {
            return false;
        }
        let mut account = match cloned_db.get_account_by_id(&uuid) {
            Ok(Some(a)) => a,
            _ => return false,
        };
        account.last_login_ip = trimmed_ip.clone();
        if cloned_db.save_account(account).is_err() {
            return false;
        }
        cloned_db.record_account_ip_seen(uuid, &trimmed_ip).is_ok()
    });

    // record_creation_ip(account_id_str, ip) -> bool
    let cloned_db = db.clone();
    engine.register_fn("record_creation_ip", move |account_id: String, ip: String| -> bool {
        let uuid = match Uuid::parse_str(&account_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let trimmed_ip = ip.trim().to_lowercase();
        if trimmed_ip.is_empty() {
            return false;
        }
        let mut account = match cloned_db.get_account_by_id(&uuid) {
            Ok(Some(a)) => a,
            _ => return false,
        };
        account.creation_ip = trimmed_ip.clone();
        if cloned_db.save_account(account).is_err() {
            return false;
        }
        // Stamp into the reverse index too — creation is a meaningful
        // signal even before the account ever logs in.
        cloned_db.record_account_ip_seen(uuid, &trimmed_ip).is_ok()
    });

    // find_alts_by_account(account_id_str) -> Array of Map
    //   Returns sibling accounts that share the subject's
    //   creation_ip / last_login_ip (within 30d) or normalized_email.
    //   Each entry: #{ name, account_id, banned, match_type, match_value }.
    //   match_type: "ip" | "email".
    let cloned_db = db.clone();
    engine.register_fn("find_alts_by_account", move |account_id: String| -> Array {
        let mut out = Array::new();
        let uuid = match Uuid::parse_str(&account_id) {
            Ok(u) => u,
            Err(_) => return out,
        };
        let subject = match cloned_db.get_account_by_id(&uuid) {
            Ok(Some(a)) => a,
            _ => return out,
        };
        let now = now_secs();
        let since = now - ALTS_IP_LOOKBACK_SECS;
        let mut seen: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
        seen.insert(subject.id);

        // IP correlation: scan both creation_ip and last_login_ip.
        for ip in [&subject.creation_ip, &subject.last_login_ip] {
            if ip.is_empty() {
                continue;
            }
            let ids = match cloned_db.list_accounts_by_ip(ip, since) {
                Ok(v) => v,
                Err(_) => continue,
            };
            for other_id in ids {
                if !seen.insert(other_id) {
                    continue;
                }
                if let Ok(Some(other)) = cloned_db.get_account_by_id(&other_id) {
                    out.push(Dynamic::from_map(alt_entry_map(&other, "ip", ip)));
                }
            }
        }

        // Normalized-email correlation.
        if let Some(canonical) = &subject.normalized_email {
            if let Ok(accounts) = cloned_db.list_accounts() {
                for other in accounts {
                    if !seen.insert(other.id) {
                        continue;
                    }
                    if other.normalized_email.as_deref() == Some(canonical.as_str()) {
                        out.push(Dynamic::from_map(alt_entry_map(&other, "email", canonical)));
                    }
                }
            }
        }

        out
    });

    // now_unix_secs() -> i64
    // Convenience for admin.rhai's duration parsing — Rhai doesn't expose a
    // built-in current-time function and we want all duration math to anchor
    // on the same clock the ban records use.
    engine.register_fn("now_unix_secs", || -> i64 { now_secs() });

    // format_unix_time(unix_secs) -> String  (YYYY-MM-DD HH:MM UTC)
    engine.register_fn("format_unix_time", |t: i64| -> String { format_time(t) });

    // siteban_kick_active(ip, reason, admin_name) -> i64
    //   For each active session whose source IP matches, send a kick line
    //   and disconnect. Returns the number of sessions evicted. Used by
    //   `admin siteban` so a freshly-banned IP doesn't keep its open sockets.
    let conns = connections.clone();
    engine.register_fn(
        "siteban_kick_active",
        move |ip: String, reason: String, admin_name: String| -> i64 {
            let trimmed = ip.trim().to_lowercase();
            if trimmed.is_empty() {
                return 0;
            }
            let to_kick: Vec<(Uuid, tokio::sync::mpsc::UnboundedSender<String>)> = {
                let guard = conns.lock().unwrap();
                guard
                    .iter()
                    .filter(|(_, s)| s.addr.ip().to_string() == trimmed)
                    .map(|(id, s)| (*id, s.sender.clone()))
                    .collect()
            };
            let count = to_kick.len();
            for (id, sender) in to_kick {
                let _ = sender.send(format!(
                    "\n*** Your address has been blocked by {}: {} ***\n",
                    admin_name, reason
                ));
                let _ = crate::disconnect_client(&conns, id.to_string());
            }
            count as i64
        },
    );
}

fn site_ban_to_map(record: &SiteBanRecord) -> Map {
    let mut m = Map::new();
    m.insert("ip".into(), Dynamic::from(record.ip.clone()));
    m.insert("reason".into(), Dynamic::from(record.reason.clone()));
    m.insert("banned_by".into(), Dynamic::from(record.banned_by.clone()));
    m.insert("banned_at".into(), Dynamic::from(record.banned_at));
    match record.expires_at {
        Some(t) => m.insert("expires_at".into(), Dynamic::from(t)),
        None => m.insert("expires_at".into(), Dynamic::UNIT),
    };
    m
}

fn alt_entry_map(other: &crate::types::AccountData, match_type: &str, match_value: &str) -> Map {
    let mut m = Map::new();
    m.insert("name".into(), Dynamic::from(other.name.clone()));
    m.insert("account_id".into(), Dynamic::from(other.id.to_string()));
    m.insert("banned".into(), Dynamic::from(other.is_banned));
    m.insert("match_type".into(), Dynamic::from(match_type.to_string()));
    m.insert("match_value".into(), Dynamic::from(match_value.to_string()));
    m
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Render a unix timestamp as `YYYY-MM-DD HH:MM UTC` for the ban-message
/// "Lifts at ..." line. Hand-rolled to avoid pulling in chrono just for this.
fn format_time(unix_secs: i64) -> String {
    if unix_secs <= 0 {
        return format!("epoch+{}", unix_secs);
    }
    let secs_per_day: i64 = 86_400;
    let mut days = unix_secs / secs_per_day;
    let mut secs_of_day = unix_secs % secs_per_day;
    if secs_of_day < 0 {
        secs_of_day += secs_per_day;
        days -= 1;
    }
    let hour = (secs_of_day / 3600) as u32;
    let minute = ((secs_of_day % 3600) / 60) as u32;

    // Civil-from-days algorithm (Howard Hinnant). Epoch 1970-01-01 = day 0.
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = (y + if m <= 2 { 1 } else { 0 }) as i64;

    format!("{:04}-{:02}-{:02} {:02}:{:02} UTC", y, m, d, hour, minute)
}

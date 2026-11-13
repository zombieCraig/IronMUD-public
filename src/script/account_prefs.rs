//! Rhai surface for the account-wide bank + character-default-preferences
//! slice. Bank fns move gold between a character's pocket and the account's
//! shared pile; preferences fns snapshot a character's settings onto the
//! account so future alts inherit them.
//!
//! Kept separate from `script/accounts.rs` so the bank/prefs surface stays
//! greppable even as the account abstraction grows.

use crate::SharedConnections;
use crate::db::Db;
use crate::types::AccountPreferences;
use rhai::{Dynamic, Engine};
use std::sync::Arc;
use uuid::Uuid;

pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections, state: crate::SharedState) {
    // ========== Shared bank ==========

    // account_shared_bank_gold(account_id) -> i64
    let cloned_db = db.clone();
    engine.register_fn("account_shared_bank_gold", move |account_id: String| -> i64 {
        let uuid = match Uuid::parse_str(&account_id) {
            Ok(u) => u,
            Err(_) => return 0,
        };
        match cloned_db.get_account_by_id(&uuid) {
            Ok(Some(a)) => a.shared_bank_gold,
            _ => 0,
        }
    });

    // lookup_account_id_for_character(char_name) -> String  ("" when not found)
    let cloned_db = db.clone();
    engine.register_fn("lookup_account_id_for_character", move |char_name: String| -> String {
        let needle = char_name.trim().to_lowercase();
        match cloned_db.list_accounts() {
            Ok(accounts) => {
                for a in accounts {
                    if a.character_names.iter().any(|n| n.to_lowercase() == needle) {
                        return a.id.to_string();
                    }
                }
                String::new()
            }
            Err(_) => String::new(),
        }
    });

    // transfer_pocket_to_shared_bank(char_name, amount) -> bool
    //   Debits the character's pocket gold; credits the account's shared pile.
    //   Refuses non-positive amounts and insufficient pocket gold.
    //
    //   Both legs are atomic on their own: the pocket goes through `purse`
    //   (session-first; refuses an overdraft) and the shared pile through a
    //   CAS in `add_shared_bank_gold`. Debit first, refund if the credit fails.
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    let cloned_state = state.clone();
    engine.register_fn(
        "transfer_pocket_to_shared_bank",
        move |char_name: String, amount: i64| -> bool {
            if amount <= 0 {
                return false;
            }
            let lower = char_name.to_lowercase();
            let Some(account_id) = find_account_id(&cloned_db, &lower) else {
                return false;
            };
            if !crate::purse::add_gold(&cloned_db, &cloned_conns, &cloned_state, &lower, -amount) {
                return false;
            }
            if matches!(cloned_db.add_shared_bank_gold(&account_id, amount), Ok(Some(_))) {
                return true;
            }
            tracing::error!(
                "transfer_pocket_to_shared_bank: shared credit failed for char={} acct={}; refunding pocket",
                char_name,
                account_id
            );
            let _ = crate::purse::add_gold(&cloned_db, &cloned_conns, &cloned_state, &lower, amount);
            false
        },
    );

    // transfer_shared_bank_to_pocket(char_name, amount) -> bool
    //   Debits the shared pile (CAS; refuses an overdraft), then credits the
    //   pocket through `purse`. Refunds the shared pile if the credit fails.
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    let cloned_state = state.clone();
    engine.register_fn(
        "transfer_shared_bank_to_pocket",
        move |char_name: String, amount: i64| -> bool {
            if amount <= 0 {
                return false;
            }
            let lower = char_name.to_lowercase();
            let Some(account_id) = find_account_id(&cloned_db, &lower) else {
                return false;
            };
            if !matches!(cloned_db.add_shared_bank_gold(&account_id, -amount), Ok(Some(_))) {
                return false;
            }
            if crate::purse::add_gold(&cloned_db, &cloned_conns, &cloned_state, &lower, amount) {
                return true;
            }
            let _ = cloned_db.add_shared_bank_gold(&account_id, amount);
            false
        },
    );

    // ========== Character-default preferences ==========

    // save_account_defaults_from_connection(connection_id) -> bool
    //   Snapshots the active player's CharacterData + PlayerSession prefs onto
    //   the account's `character_defaults`. Sets is_set=true. Returns false if
    //   the connection is unauthenticated or missing a character.
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    engine.register_fn(
        "save_account_defaults_from_connection",
        move |connection_id: String| -> bool {
            let conn_uuid = match Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            // Lock-and-snapshot pattern: read everything we need from the
            // session under one short lock, then drop before touching the db.
            let snap = {
                let conns = cloned_conns.lock();
                let session = match conns.get(&conn_uuid) {
                    Some(s) => s,
                    None => return false,
                };
                let character = match session.character.as_ref() {
                    Some(c) => c,
                    None => return false,
                };
                let account_id = match session.account_id {
                    Some(id) => id,
                    None => return false,
                };
                Some(Snapshot {
                    account_id,
                    char_name: character.name.clone(),
                    colors_enabled: session.colors_enabled,
                    mxp_enabled: session.mxp_enabled,
                    abbrev_enabled: session.abbrev_enabled,
                })
            };
            let snap = match snap {
                Some(s) => s,
                None => return false,
            };
            // Reload the character from disk for canonical field values.
            let character = match cloned_db.get_character_data(&snap.char_name.to_lowercase()) {
                Ok(Some(c)) => c,
                _ => return false,
            };
            let prefs = AccountPreferences {
                prompt_mode: character.prompt_mode.clone(),
                prompt_format: character.prompt_format.clone(),
                colors_enabled: snap.colors_enabled,
                mxp_enabled: snap.mxp_enabled,
                abbrev_enabled: snap.abbrev_enabled,
                helpline_enabled: character.helpline_enabled,
                summonable: character.summonable,
                automap_enabled: character.automap_enabled,
                automap_radius: character.automap_radius,
                ascii_map: character.ascii_map,
                is_set: true,
            };
            cloned_db
                .save_account_preferences(&snap.account_id, prefs)
                .unwrap_or(false)
        },
    );

    // clear_account_defaults(account_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("clear_account_defaults", move |account_id: String| -> bool {
        let uuid = match Uuid::parse_str(&account_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        cloned_db
            .save_account_preferences(&uuid, AccountPreferences::default())
            .unwrap_or(false)
    });

    // apply_account_defaults_to_new_character(account_id, char_name) -> bool
    //   Stamps the 6 CharacterData-side fields onto a freshly-saved character.
    //   No-op (returns true) when account.is_set is false.
    let cloned_db = db.clone();
    engine.register_fn(
        "apply_account_defaults_to_new_character",
        move |account_id: String, char_name: String| -> bool {
            let uuid = match Uuid::parse_str(&account_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let account = match cloned_db.get_account_by_id(&uuid) {
                Ok(Some(a)) => a,
                _ => return false,
            };
            if !account.character_defaults.is_set {
                return true;
            }
            let lower = char_name.to_lowercase();
            let mut character = match cloned_db.get_character_data(&lower) {
                Ok(Some(c)) => c,
                _ => return false,
            };
            let d = &account.character_defaults;
            character.prompt_mode = d.prompt_mode.clone();
            character.prompt_format = d.prompt_format.clone();
            character.helpline_enabled = d.helpline_enabled;
            character.summonable = d.summonable;
            character.automap_enabled = d.automap_enabled;
            character.automap_radius = d.automap_radius;
            character.ascii_map = d.ascii_map;
            cloned_db.save_character_data(character).is_ok()
        },
    );

    // apply_account_session_defaults(connection_id, account_id) -> bool
    //   Pushes the 3 session-resident fields (colors/mxp/abbrev) onto the
    //   live PlayerSession. No-op (returns true) when account.is_set is false.
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    engine.register_fn(
        "apply_account_session_defaults",
        move |connection_id: String, account_id: String| -> bool {
            let conn_uuid = match Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let acct_uuid = match Uuid::parse_str(&account_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let account = match cloned_db.get_account_by_id(&acct_uuid) {
                Ok(Some(a)) => a,
                _ => return false,
            };
            if !account.character_defaults.is_set {
                return true;
            }
            let d = &account.character_defaults;
            {
                let mut conns = cloned_conns.lock();
                if let Some(session) = conns.get_mut(&conn_uuid) {
                    session.colors_enabled = d.colors_enabled;
                    session.mxp_enabled = d.mxp_enabled;
                    session.abbrev_enabled = d.abbrev_enabled;
                    return true;
                }
            }
            false
        },
    );

    // format_account_defaults(account_id) -> String
    let cloned_db = db.clone();
    engine.register_fn(
        "format_account_defaults",
        move |account_id: String| -> String {
            let uuid = match Uuid::parse_str(&account_id) {
                Ok(u) => u,
                Err(_) => return "(invalid account id)".to_string(),
            };
            let account = match cloned_db.get_account_by_id(&uuid) {
                Ok(Some(a)) => a,
                _ => return "(account not found)".to_string(),
            };
            let d = &account.character_defaults;
            if !d.is_set {
                return "(no defaults saved — run `set defaults save` to capture this character's preferences)".to_string();
            }
            // A stored custom format is what the alt will actually get, so
            // show that rather than the preset it overrides.
            let prompt_label = if !d.prompt_format.is_empty() {
                d.prompt_format.as_str()
            } else if d.prompt_mode.is_empty() {
                "default"
            } else {
                d.prompt_mode.as_str()
            };
            format!(
                "Account defaults (applied to new alts):\r\n  prompt: {}\r\n  colors: {}\r\n  mxp: {}\r\n  abbrev: {}\r\n  helpline: {}\r\n  summonable: {}\r\n  automap: {}  (radius {}, ascii {})",
                prompt_label,
                onoff(d.colors_enabled),
                onoff(d.mxp_enabled),
                onoff(d.abbrev_enabled),
                onoff(d.helpline_enabled),
                onoff(d.summonable),
                onoff(d.automap_enabled),
                d.automap_radius,
                onoff(d.ascii_map),
            )
        },
    );

    let _ = Dynamic::UNIT; // silence unused warning if all returns are typed
}

struct Snapshot {
    account_id: Uuid,
    char_name: String,
    colors_enabled: bool,
    mxp_enabled: bool,
    abbrev_enabled: bool,
}

fn find_account_id(db: &Db, char_name_lower: &str) -> Option<Uuid> {
    let accounts = db.list_accounts().ok()?;
    for a in accounts {
        if a.character_names.iter().any(|n| n.to_lowercase() == char_name_lower) {
            return Some(a.id);
        }
    }
    None
}

fn onoff(b: bool) -> &'static str {
    if b { "on" } else { "off" }
}

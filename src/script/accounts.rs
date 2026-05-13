// src/script/accounts.rs
// Rhai surface for the Account abstraction. Backs the account-name auth path
// in `scripts/commands/login.rhai` and the "create new character under
// existing account" branch in `scripts/commands/create.rhai`.

use crate::SharedConnections;
use crate::db::Db;
use rhai::Engine;
use std::sync::Arc;

/// Maximum characters per account. Prevents one stolen account from spawning
/// hundreds of characters; not a balance lever, just an anti-abuse cap.
pub const MAX_CHARACTERS_PER_ACCOUNT: usize = 5;

pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections) {
    // get_account_by_name(name) -> Map | ()
    // Empty/whitespace name returns ().
    let cloned_db = db.clone();
    engine.register_fn("get_account_by_name", move |name: String| -> rhai::Dynamic {
        if name.trim().is_empty() {
            return rhai::Dynamic::UNIT;
        }
        match cloned_db.get_account(&name) {
            Ok(Some(account)) => rhai::Dynamic::from_map(account_to_map(&account)),
            _ => rhai::Dynamic::UNIT,
        }
    });

    // get_account_by_id(account_id_str) -> Map | ()
    let cloned_db = db.clone();
    engine.register_fn("get_account_by_id", move |id: String| -> rhai::Dynamic {
        let uuid = match uuid::Uuid::parse_str(&id) {
            Ok(u) => u,
            Err(_) => return rhai::Dynamic::UNIT,
        };
        match cloned_db.get_account_by_id(&uuid) {
            Ok(Some(account)) => rhai::Dynamic::from_map(account_to_map(&account)),
            _ => rhai::Dynamic::UNIT,
        }
    });

    // create_account(name, password_hash) -> account_id_str | ""
    // Returns "" on failure (name conflict, empty inputs, etc.).
    let cloned_db = db.clone();
    engine.register_fn(
        "create_account",
        move |name: String, password_hash: String| -> String {
            let name = name.trim().to_string();
            if name.is_empty() || password_hash.is_empty() {
                return String::new();
            }
            // Refuse if an account already owns this lowercase name.
            if cloned_db.get_account(&name).ok().flatten().is_some() {
                return String::new();
            }
            let account = crate::types::AccountData::new(name, password_hash);
            let id_str = account.id.to_string();
            if cloned_db.save_account(account).is_err() {
                return String::new();
            }
            id_str
        },
    );

    // add_character_to_account(account_id_str, character_name) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "add_character_to_account",
        move |account_id: String, character_name: String| -> bool {
            let uuid = match uuid::Uuid::parse_str(&account_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            cloned_db
                .add_character_to_account(&uuid, &character_name)
                .unwrap_or(false)
        },
    );

    // count_account_characters(account_id_str) -> i64
    let cloned_db = db.clone();
    engine.register_fn(
        "count_account_characters",
        move |account_id: String| -> i64 {
            let uuid = match uuid::Uuid::parse_str(&account_id) {
                Ok(u) => u,
                Err(_) => return 0,
            };
            cloned_db
                .get_account_by_id(&uuid)
                .ok()
                .flatten()
                .map(|a| a.character_names.len() as i64)
                .unwrap_or(0)
        },
    );

    // get_account_character_summaries(account_id_str) -> Array<Map>
    // Each entry: #{ name, level, class_name, race, room_title }. Used by the
    // login roster screen.
    let cloned_db = db.clone();
    engine.register_fn(
        "get_account_character_summaries",
        move |account_id: String| -> rhai::Array {
            let uuid = match uuid::Uuid::parse_str(&account_id) {
                Ok(u) => u,
                Err(_) => return rhai::Array::new(),
            };
            let account = match cloned_db.get_account_by_id(&uuid) {
                Ok(Some(a)) => a,
                _ => return rhai::Array::new(),
            };
            let mut out = rhai::Array::new();
            for char_name in &account.character_names {
                if let Ok(Some(c)) = cloned_db.get_character_data(char_name) {
                    let mut entry = rhai::Map::new();
                    entry.insert("name".into(), rhai::Dynamic::from(c.name.clone()));
                    entry.insert("level".into(), rhai::Dynamic::from(c.level as i64));
                    entry.insert(
                        "class_name".into(),
                        rhai::Dynamic::from(c.class_name.clone()),
                    );
                    entry.insert("race".into(), rhai::Dynamic::from(c.race.clone()));
                    let room_title = cloned_db
                        .get_room_data(&c.current_room_id)
                        .ok()
                        .flatten()
                        .map(|r| r.title)
                        .unwrap_or_default();
                    entry.insert("room_title".into(), rhai::Dynamic::from(room_title));
                    out.push(rhai::Dynamic::from_map(entry));
                }
            }
            out
        },
    );

    // max_characters_per_account() -> i64
    engine.register_fn("max_characters_per_account", || -> i64 {
        MAX_CHARACTERS_PER_ACCOUNT as i64
    });

    // === Connection-state helpers ===

    // set_authenticated_account(connection_id, account_id_str, account_name) -> bool
    let conn_clone = connections.clone();
    engine.register_fn(
        "set_authenticated_account",
        move |connection_id: String,
              account_id: String,
              account_name: String|
              -> bool {
            let conn_uuid = match uuid::Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let acct_uuid = match uuid::Uuid::parse_str(&account_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            if let Ok(mut conns) = conn_clone.lock() {
                if let Some(session) = conns.get_mut(&conn_uuid) {
                    session.account_id = Some(acct_uuid);
                    session.account_name = Some(account_name);
                    return true;
                }
            }
            false
        },
    );

    // get_authenticated_account_id(connection_id) -> String  ("" when unset)
    let conn_clone = connections.clone();
    engine.register_fn(
        "get_authenticated_account_id",
        move |connection_id: String| -> String {
            let conn_uuid = match uuid::Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return String::new(),
            };
            conn_clone
                .lock()
                .ok()
                .and_then(|conns| {
                    conns
                        .get(&conn_uuid)
                        .and_then(|s| s.account_id.map(|u| u.to_string()))
                })
                .unwrap_or_default()
        },
    );

    // get_authenticated_account_name(connection_id) -> String  ("" when unset)
    let conn_clone = connections.clone();
    engine.register_fn(
        "get_authenticated_account_name",
        move |connection_id: String| -> String {
            let conn_uuid = match uuid::Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return String::new(),
            };
            conn_clone
                .lock()
                .ok()
                .and_then(|conns| {
                    conns
                        .get(&conn_uuid)
                        .and_then(|s| s.account_name.clone())
                })
                .unwrap_or_default()
        },
    );

    // clear_authenticated_account(connection_id) — used on logout/quit.
    let conn_clone = connections.clone();
    engine.register_fn(
        "clear_authenticated_account",
        move |connection_id: String| -> bool {
            let conn_uuid = match uuid::Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            if let Ok(mut conns) = conn_clone.lock() {
                if let Some(session) = conns.get_mut(&conn_uuid) {
                    session.account_id = None;
                    session.account_name = None;
                    return true;
                }
            }
            false
        },
    );

    // delete_account(name) — used by the email-verification cancel path to
    // roll back a half-created account so the name is freed up.
    let cloned_db = db.clone();
    engine.register_fn("delete_account", move |name: String| -> bool {
        cloned_db.delete_account(&name).is_ok()
    });

    // touch_account_login(account_id_str) — stamp last_login_at to now. Called
    // from login.rhai after a successful pick.
    let cloned_db = db.clone();
    engine.register_fn("touch_account_login", move |account_id: String| -> bool {
        let uuid = match uuid::Uuid::parse_str(&account_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let mut account = match cloned_db.get_account_by_id(&uuid) {
            Ok(Some(a)) => a,
            _ => return false,
        };
        account.last_login_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        cloned_db.save_account(account).is_ok()
    });

    // list_all_accounts() -> Array of Maps
    let cloned_db = db.clone();
    engine.register_fn("list_all_accounts", move || -> rhai::Array {
        match cloned_db.list_accounts() {
            Ok(accounts) => accounts
                .into_iter()
                .map(|a| rhai::Dynamic::from_map(account_to_map(&a)))
                .collect(),
            Err(_) => rhai::Array::new(),
        }
    });

    // verify_account(account_id_str) -> bool
    let cloned_db = db.clone();
    engine.register_fn("verify_account", move |account_id: String| -> bool {
        let uuid = match uuid::Uuid::parse_str(&account_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let mut account = match cloned_db.get_account_by_id(&uuid) {
            Ok(Some(a)) => a,
            _ => return false,
        };
        account.email_verified = true;
        account.email_verification_code = None;
        account.email_verification_code_expires_at = 0;
        cloned_db.save_account(account).is_ok()
    });

    // unverify_account(account_id_str) -> bool
    let cloned_db = db.clone();
    engine.register_fn("unverify_account", move |account_id: String| -> bool {
        let uuid = match uuid::Uuid::parse_str(&account_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let mut account = match cloned_db.get_account_by_id(&uuid) {
            Ok(Some(a)) => a,
            _ => return false,
        };
        account.email_verified = false;
        cloned_db.save_account(account).is_ok()
    });

    // add_shared_bank_gold(account_id_str, amount) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "add_shared_bank_gold",
        move |account_id: String, amount: i64| -> bool {
            let uuid = match uuid::Uuid::parse_str(&account_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            cloned_db.add_shared_bank_gold(&uuid, amount).is_ok()
        },
    );
}

fn account_to_map(account: &crate::types::AccountData) -> rhai::Map {
    let mut map = rhai::Map::new();
    map.insert("id".into(), rhai::Dynamic::from(account.id.to_string()));
    map.insert("name".into(), rhai::Dynamic::from(account.name.clone()));
    map.insert(
        "password_hash".into(),
        rhai::Dynamic::from(account.password_hash.clone()),
    );
    let names: rhai::Array = account
        .character_names
        .iter()
        .map(|n| rhai::Dynamic::from(n.clone()))
        .collect();
    map.insert("character_names".into(), rhai::Dynamic::from(names));
    map.insert("is_banned".into(), rhai::Dynamic::from(account.is_banned));

    if let Some(ref record) = account.ban_record {
        let mut ban = rhai::Map::new();
        ban.insert("reason".into(), rhai::Dynamic::from(record.reason.clone()));
        ban.insert(
            "banned_by".into(),
            rhai::Dynamic::from(record.banned_by.clone()),
        );
        ban.insert("banned_at".into(), rhai::Dynamic::from(record.banned_at));
        ban.insert(
            "expires_at".into(),
            match record.expires_at {
                Some(t) => rhai::Dynamic::from(t),
                None => rhai::Dynamic::UNIT,
            },
        );
        map.insert("ban_record".into(), rhai::Dynamic::from_map(ban));
    }

    map.insert(
        "email".into(),
        rhai::Dynamic::from(account.email.clone().unwrap_or_default()),
    );
    map.insert(
        "normalized_email".into(),
        rhai::Dynamic::from(account.normalized_email.clone().unwrap_or_default()),
    );
    map.insert(
        "email_verified".into(),
        rhai::Dynamic::from(account.email_verified),
    );
    map.insert(
        "pending_code".into(),
        rhai::Dynamic::from(account.email_verification_code.is_some()),
    );
    map.insert(
        "email_verification_code_expires_at".into(),
        rhai::Dynamic::from(account.email_verification_code_expires_at),
    );
    map.insert(
        "last_login_ip".into(),
        rhai::Dynamic::from(account.last_login_ip.clone()),
    );
    map.insert(
        "creation_ip".into(),
        rhai::Dynamic::from(account.creation_ip.clone()),
    );
    map.insert(
        "created_at".into(),
        rhai::Dynamic::from(account.created_at),
    );
    map.insert(
        "last_login_at".into(),
        rhai::Dynamic::from(account.last_login_at),
    );
    map.insert(
        "shared_bank_gold".into(),
        rhai::Dynamic::from(account.shared_bank_gold),
    );

    // Character defaults
    let d = &account.character_defaults;
    let mut prefs = rhai::Map::new();
    prefs.insert(
        "prompt_mode".into(),
        rhai::Dynamic::from(d.prompt_mode.clone()),
    );
    prefs.insert(
        "colors_enabled".into(),
        rhai::Dynamic::from(d.colors_enabled),
    );
    prefs.insert("mxp_enabled".into(), rhai::Dynamic::from(d.mxp_enabled));
    prefs.insert(
        "abbrev_enabled".into(),
        rhai::Dynamic::from(d.abbrev_enabled),
    );
    prefs.insert(
        "helpline_enabled".into(),
        rhai::Dynamic::from(d.helpline_enabled),
    );
    prefs.insert("summonable".into(), rhai::Dynamic::from(d.summonable));
    prefs.insert(
        "automap_enabled".into(),
        rhai::Dynamic::from(d.automap_enabled),
    );
    prefs.insert(
        "automap_radius".into(),
        rhai::Dynamic::from(d.automap_radius as i64),
    );
    prefs.insert("ascii_map".into(), rhai::Dynamic::from(d.ascii_map));
    prefs.insert("is_set".into(), rhai::Dynamic::from(d.is_set));
    map.insert("character_defaults".into(), rhai::Dynamic::from_map(prefs));

    map
}

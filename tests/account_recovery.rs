//! Account recovery and password change (security hardening 2.7).

use ironmud::db::Db;
use ironmud::{AccountData, CharacterData, PlayerSession, SharedConnections};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

fn character(name: &str, hash: &str) -> CharacterData {
    serde_json::from_value(serde_json::json!({
        "name": name, "password_hash": hash, "current_room_id": Uuid::nil(),
    }))
    .unwrap()
}

/// `password` must change what login checks (the account hash), cancel any
/// outstanding reset, and clear the forced-change flag on every character.
#[test]
fn password_command_updates_the_account_hash() {
    let temp = tempfile::tempdir().unwrap();
    let db = Db::open(temp.path()).unwrap();
    let old = db.hash_password("old-password").unwrap();
    let mut account = AccountData::new("alice".to_string(), old.clone());
    account.email = Some("alice@example.com".to_string());
    account.character_names = vec!["Alice".to_string(), "Alt".to_string()];
    let account_id = account.id;
    db.save_account(account).unwrap();
    for name in ["Alice", "Alt"] {
        let mut c = character(name, &old);
        c.must_change_password = true;
        db.save_character_data(c).unwrap();
    }
    let (_, temp_pw) = ironmud::script::email::issue_password_reset_core(&db, &account_id).unwrap();

    let connections: SharedConnections = Arc::new(Mutex::new(HashMap::new()));
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (tx_in, _rx_in) = tokio::sync::mpsc::channel::<ironmud::InputEvent>(1);
    let mut session = PlayerSession::new_for_test(tx, tx_in);
    session.account_id = Some(account_id);
    session.character = Some(db.get_character_data("Alice").unwrap().unwrap());
    let conn_id = Uuid::new_v4();
    connections.lock().insert(conn_id, session);

    let state = ironmud::World::minimal_shared(db.clone(), connections.clone());
    let mut engine = rhai::Engine::new();
    ironmud::script::characters::register(&mut engine, Arc::new(db.clone()), connections.clone(), state);
    let ok: bool = engine
        .eval(&format!(r#"change_password("{conn_id}", "new-password")"#))
        .unwrap();
    assert!(ok);

    let after = db.get_account_by_id(&account_id).unwrap().unwrap();
    assert!(db.verify_password("new-password", &after.password_hash).unwrap());
    assert!(after.pending_reset_hash.is_none(), "password change cancels the reset");
    assert!(!ironmud::script::email::verify_account_login(
        &db,
        &account_id,
        &temp_pw
    ));
    assert!(!ironmud::script::email::verify_account_login(
        &db,
        &account_id,
        "old-password"
    ));
    assert!(ironmud::script::email::verify_account_login(
        &db,
        &account_id,
        "new-password"
    ));
    assert!(!db.get_character_data("Alt").unwrap().unwrap().must_change_password);
    let live = connections.lock()[&conn_id].character.clone().unwrap();
    assert!(!live.must_change_password);
}

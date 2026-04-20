// src/script/api_keys.rs
// API key management functions: create, list, get, revoke, enable, delete

use crate::db::Db;
use rhai::Engine;
use std::sync::Arc;

/// Base64 encode bytes (URL-safe, no padding)
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut result = String::with_capacity((bytes.len() * 4 + 2) / 3);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        result.push(ALPHABET[b0 >> 2] as char);
        result.push(ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)] as char);
        if chunk.len() > 1 {
            result.push(ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
        }
        if chunk.len() > 2 {
            result.push(ALPHABET[b2 & 0x3f] as char);
        }
    }
    result
}

/// Register API key management functions
pub fn register(engine: &mut Engine, db: Arc<Db>) {
    // create_api_key(name, character, read, write, admin) -> Map {raw_key, key_id, success, error}
    let cloned_db = db.clone();
    engine.register_fn(
        "create_api_key",
        move |name: String, character: String, read: bool, write: bool, admin: bool| {
            let mut result = rhai::Map::new();

            // Verify character exists
            match cloned_db.get_character_data(&character) {
                Ok(Some(_)) => {}
                Ok(None) => {
                    result.insert("success".into(), rhai::Dynamic::from(false));
                    result.insert(
                        "error".into(),
                        rhai::Dynamic::from(format!("Character '{}' not found", character)),
                    );
                    return result;
                }
                Err(e) => {
                    result.insert("success".into(), rhai::Dynamic::from(false));
                    result.insert("error".into(), rhai::Dynamic::from(format!("DB error: {}", e)));
                    return result;
                }
            }

            // Generate 32 random bytes
            use rand::RngCore;
            let mut key_bytes = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut key_bytes);
            let raw_key = base64_encode(&key_bytes);

            // Hash the key for storage
            let key_hash = match cloned_db.hash_password(&raw_key) {
                Ok(h) => h,
                Err(e) => {
                    result.insert("success".into(), rhai::Dynamic::from(false));
                    result.insert("error".into(), rhai::Dynamic::from(format!("Hash error: {}", e)));
                    return result;
                }
            };

            // Build ApiKey struct
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            let api_key = crate::ApiKey {
                id: uuid::Uuid::new_v4(),
                key_hash,
                name,
                owner_character: character,
                permissions: crate::ApiPermissions { read, write, admin },
                created_at: now,
                last_used_at: None,
                enabled: true,
            };

            // Save
            if let Err(e) = cloned_db.save_api_key(&api_key) {
                result.insert("success".into(), rhai::Dynamic::from(false));
                result.insert("error".into(), rhai::Dynamic::from(format!("Save error: {}", e)));
                return result;
            }

            result.insert("success".into(), rhai::Dynamic::from(true));
            result.insert("key_id".into(), rhai::Dynamic::from(api_key.id.to_string()));
            result.insert("raw_key".into(), rhai::Dynamic::from(raw_key));
            result
        },
    );

    // list_api_keys() -> Array of Maps
    let cloned_db = db.clone();
    engine.register_fn("list_api_keys", move || -> Vec<rhai::Dynamic> {
        match cloned_db.list_all_api_keys() {
            Ok(keys) => keys
                .iter()
                .map(|key| {
                    let mut map = rhai::Map::new();
                    map.insert("id".into(), rhai::Dynamic::from(key.id.to_string()));
                    map.insert("name".into(), rhai::Dynamic::from(key.name.clone()));
                    map.insert(
                        "owner_character".into(),
                        rhai::Dynamic::from(key.owner_character.clone()),
                    );
                    map.insert("enabled".into(), rhai::Dynamic::from(key.enabled));
                    map.insert("read".into(), rhai::Dynamic::from(key.permissions.read));
                    map.insert("write".into(), rhai::Dynamic::from(key.permissions.write));
                    map.insert("admin".into(), rhai::Dynamic::from(key.permissions.admin));
                    map.insert("created_at".into(), rhai::Dynamic::from(key.created_at));
                    map.insert(
                        "last_used_at".into(),
                        key.last_used_at.map(rhai::Dynamic::from).unwrap_or(rhai::Dynamic::UNIT),
                    );
                    rhai::Dynamic::from(map)
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    });

    // get_api_key_info(id_string) -> Map or ()
    let cloned_db = db.clone();
    engine.register_fn("get_api_key_info", move |id_string: String| -> rhai::Dynamic {
        let uuid = match uuid::Uuid::parse_str(&id_string) {
            Ok(u) => u,
            Err(_) => return rhai::Dynamic::UNIT,
        };
        match cloned_db.get_api_key(&uuid) {
            Ok(Some(key)) => {
                let mut map = rhai::Map::new();
                map.insert("id".into(), rhai::Dynamic::from(key.id.to_string()));
                map.insert("name".into(), rhai::Dynamic::from(key.name.clone()));
                map.insert(
                    "owner_character".into(),
                    rhai::Dynamic::from(key.owner_character.clone()),
                );
                map.insert("enabled".into(), rhai::Dynamic::from(key.enabled));
                map.insert("read".into(), rhai::Dynamic::from(key.permissions.read));
                map.insert("write".into(), rhai::Dynamic::from(key.permissions.write));
                map.insert("admin".into(), rhai::Dynamic::from(key.permissions.admin));
                map.insert("created_at".into(), rhai::Dynamic::from(key.created_at));
                map.insert(
                    "last_used_at".into(),
                    key.last_used_at.map(rhai::Dynamic::from).unwrap_or(rhai::Dynamic::UNIT),
                );
                rhai::Dynamic::from(map)
            }
            _ => rhai::Dynamic::UNIT,
        }
    });

    // revoke_api_key(id_string) -> bool
    let cloned_db = db.clone();
    engine.register_fn("revoke_api_key", move |id_string: String| -> bool {
        let uuid = match uuid::Uuid::parse_str(&id_string) {
            Ok(u) => u,
            Err(_) => return false,
        };
        match cloned_db.get_api_key(&uuid) {
            Ok(Some(mut key)) => {
                key.enabled = false;
                cloned_db.save_api_key(&key).is_ok()
            }
            _ => false,
        }
    });

    // enable_api_key(id_string) -> bool
    let cloned_db = db.clone();
    engine.register_fn("enable_api_key", move |id_string: String| -> bool {
        let uuid = match uuid::Uuid::parse_str(&id_string) {
            Ok(u) => u,
            Err(_) => return false,
        };
        match cloned_db.get_api_key(&uuid) {
            Ok(Some(mut key)) => {
                key.enabled = true;
                cloned_db.save_api_key(&key).is_ok()
            }
            _ => false,
        }
    });

    // delete_api_key_by_id(id_string) -> bool
    let cloned_db = db.clone();
    engine.register_fn("delete_api_key_by_id", move |id_string: String| -> bool {
        let uuid = match uuid::Uuid::parse_str(&id_string) {
            Ok(u) => u,
            Err(_) => return false,
        };
        cloned_db.delete_api_key(&uuid).unwrap_or(false)
    });
}

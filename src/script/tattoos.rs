// src/script/tattoos.rs
// Permanent body-mark system. A `tattoo` item is consumed via `apply` and
// produces a `CharacterTattoo` record on the wearer. Each `ItemAffect` on the
// item is re-stamped as a permanent `ActiveBuff` sourced as
// `"tattoo:<vnum>:<location>"` so admin removal can strip them cleanly.

use crate::SharedConnections;
use crate::db::Db;
use crate::types::CharacterTattoo;
use rhai::{Dynamic, Engine, Map};
use std::sync::Arc;

pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections) {
    // ----- CharacterTattoo type registration ---------------------------------
    engine
        .register_type_with_name::<CharacterTattoo>("CharacterTattoo")
        .register_get("location", |t: &mut CharacterTattoo| {
            t.location.to_display_string().to_string()
        })
        .register_get("short_desc", |t: &mut CharacterTattoo| t.short_desc.clone())
        .register_get("long_desc", |t: &mut CharacterTattoo| t.long_desc.clone())
        .register_get("keywords", |t: &mut CharacterTattoo| {
            t.keywords.iter().cloned().map(Dynamic::from).collect::<Vec<_>>()
        })
        .register_get("source_vnum", |t: &mut CharacterTattoo| {
            t.source_vnum.clone().unwrap_or_default()
        });

    // apply_tattoo_to_self(connection_id, item_id) -> Map
    //   { ok: bool, message: string, short_desc: string, location: string }
    // On success: consumes the item, pushes a CharacterTattoo, stamps every
    // ItemAffect as a permanent ActiveBuff, saves the character, and syncs the
    // session cache. Self-only — the verb script must enforce target = self.
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    engine.register_fn(
        "apply_tattoo_to_self",
        move |connection_id: String, item_id: String| -> Map {
            let mut out = Map::new();
            out.insert("ok".into(), Dynamic::from(false));
            out.insert("message".into(), Dynamic::from(String::new()));
            out.insert("short_desc".into(), Dynamic::from(String::new()));
            out.insert("location".into(), Dynamic::from(String::new()));

            let conn_id = match uuid::Uuid::parse_str(&connection_id) {
                Ok(c) => c,
                Err(_) => {
                    out.insert("message".into(), "Invalid session.".into());
                    return out;
                }
            };

            let char_name = {
                let conns_lock = cloned_conns.lock().unwrap();
                match conns_lock.get(&conn_id) {
                    Some(session) => match session.character.as_ref() {
                        Some(c) => c.name.clone(),
                        None => {
                            out.insert("message".into(), "You must be logged in.".into());
                            return out;
                        }
                    },
                    None => {
                        out.insert("message".into(), "Session not found.".into());
                        return out;
                    }
                }
            };

            let item_uuid = match uuid::Uuid::parse_str(&item_id) {
                Ok(u) => u,
                Err(_) => {
                    out.insert("message".into(), "Invalid item id.".into());
                    return out;
                }
            };

            let (short_desc, location) = match cloned_db.apply_tattoo_to_character(&char_name, &item_uuid) {
                Ok(pair) => pair,
                Err(e) => {
                    out.insert("message".into(), format!("{}", e).into());
                    return out;
                }
            };

            // Sync session cache with the freshly-saved character.
            let key = char_name.to_lowercase();
            if let Ok(Some(updated)) = cloned_db.get_character_data(&key) {
                let mut conns_lock = cloned_conns.lock().unwrap();
                if let Some(session) = conns_lock.get_mut(&conn_id) {
                    session.character = Some(updated);
                }
            }

            out.insert("ok".into(), Dynamic::from(true));
            out.insert("short_desc".into(), Dynamic::from(short_desc));
            out.insert("location".into(), Dynamic::from(location));
            out
        },
    );

    // get_character_tattoos(char_name) -> Array<CharacterTattoo>
    let cloned_db = db.clone();
    engine.register_fn("get_character_tattoos", move |char_name: String| -> Vec<Dynamic> {
        let key = char_name.to_lowercase();
        match cloned_db.get_character_data(&key) {
            Ok(Some(c)) => c.tattoos.into_iter().map(Dynamic::from).collect(),
            _ => Vec::new(),
        }
    });

    // find_self_tattoo_by_keyword(char_name, keyword) -> CharacterTattoo or ()
    let cloned_db = db.clone();
    engine.register_fn(
        "find_self_tattoo_by_keyword",
        move |char_name: String, keyword: String| -> Dynamic {
            let kw = keyword.to_lowercase();
            if kw.is_empty() {
                return Dynamic::UNIT;
            }
            let key = char_name.to_lowercase();
            let character = match cloned_db.get_character_data(&key) {
                Ok(Some(c)) => c,
                _ => return Dynamic::UNIT,
            };
            for tattoo in character.tattoos {
                if tattoo.short_desc.to_lowercase().contains(&kw) {
                    return Dynamic::from(tattoo);
                }
                for k in &tattoo.keywords {
                    let kl = k.to_lowercase();
                    if kl == kw || kl.contains(&kw) {
                        return Dynamic::from(tattoo);
                    }
                }
            }
            Dynamic::UNIT
        },
    );

    // admin_remove_tattoo(char_name, index) -> bool
    // Strips the tattoo at `index` from the character's tattoos vec and removes
    // every ActiveBuff with the matching `tattoo:<vnum>:<location>` source.
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    engine.register_fn("admin_remove_tattoo", move |char_name: String, index: i64| -> bool {
        if index < 0 {
            return false;
        }
        let removed = match cloned_db.remove_tattoo_from_character(&char_name, index as usize) {
            Ok(b) => b,
            Err(_) => return false,
        };
        if !removed {
            return false;
        }
        // Sync any live session for this character.
        let key = char_name.to_lowercase();
        if let Ok(Some(updated)) = cloned_db.get_character_data(&key) {
            let mut conns_lock = cloned_conns.lock().unwrap();
            for session in conns_lock.values_mut() {
                if let Some(ref ch) = session.character {
                    if ch.name.eq_ignore_ascii_case(&char_name) {
                        session.character = Some(updated.clone());
                        break;
                    }
                }
            }
        }
        true
    });
}

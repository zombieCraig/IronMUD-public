// src/script/tattoos.rs
// Permanent body-mark system. A `tattoo` item is consumed via `apply` and
// produces a `CharacterTattoo` record on the wearer. Each `ItemAffect` on the
// item is re-stamped as a permanent `ActiveBuff` sourced as
// `"tattoo:<vnum>:<location>"` so admin removal can strip them cleanly.
//
// Every write goes through `achievements::apply_to_character`, so an online
// player's session copy is the one changed. A DB-only write would be reverted
// by the next regen/hunger/thirst flush, and overwriting the session with a
// fresh DB load would drop any change the session had not flushed yet.

use crate::SharedConnections;
use crate::db::Db;
use crate::script::achievements::apply_to_character;
use crate::types::{ActiveBuff, CharacterData, CharacterTattoo, ItemData, ItemType, WearLocation};
use rhai::{Dynamic, Engine, Map};
use std::sync::Arc;
use uuid::Uuid;

/// The `ActiveBuff.source` tag shared by stamping and stripping a tattoo.
fn tattoo_buff_source(vnum: Option<&str>, location: WearLocation) -> String {
    format!("tattoo:{}:{}", vnum.unwrap_or("-"), location.to_display_string())
}

/// Push the tattoo record for `item` and stamp each of its affects as a
/// permanent buff. Pure: the caller persists. `item` must already be
/// validated (a tattoo with at least one wear location).
fn stamp_tattoo(ch: &mut CharacterData, item: &ItemData) {
    let location = item.wear_locations[0];
    let source = tattoo_buff_source(item.vnum.as_deref(), location);
    ch.tattoos.push(CharacterTattoo {
        location,
        keywords: item.keywords.clone(),
        short_desc: item.short_desc.clone(),
        long_desc: item.long_desc.clone(),
        source_vnum: item.vnum.clone(),
        affects: item.affects.clone(),
    });
    stamp_affects(ch, &source, &item.affects);
}

/// Stamp `affects` as permanent buffs sourced `source`. Shared by tattoos
/// and divine marks; pure, the caller persists.
pub(crate) fn stamp_affects(ch: &mut CharacterData, source: &str, affects: &[crate::types::ItemAffect]) {
    for affect in affects {
        ch.active_buffs.push(ActiveBuff {
            effect_type: affect.effect_type,
            magnitude: affect.magnitude,
            remaining_secs: -1,
            source: source.to_string(),
            damage_type: affect.damage_type,
            vs_effect: affect.vs_effect.clone(),
            skill_key: affect.skill_key.clone(),
        });
    }
}

/// Remove the tattoo at `index` and every buff it stamped. False when the
/// index is out of range.
fn strip_tattoo(ch: &mut CharacterData, index: usize) -> bool {
    if index >= ch.tattoos.len() {
        return false;
    }
    let removed = ch.tattoos.remove(index);
    let source = tattoo_buff_source(removed.source_vnum.as_deref(), removed.location);
    ch.active_buffs.retain(|b| b.source != source);
    true
}

/// Apply a `Tattoo`-type item to a character: consume the item, push a
/// `CharacterTattoo` record, and stamp each `ItemAffect` as a permanent
/// `ActiveBuff`. Returns `(short_desc, location_display)` for the flavor line.
///
/// Claim first: the item is deleted before the character is touched, and only
/// the caller whose delete removed it gets the tattoo. If the character write
/// fails the item is put back.
pub fn apply_tattoo(
    db: &Db,
    connections: &SharedConnections,
    char_name: &str,
    item_id: &Uuid,
) -> Result<(String, String), String> {
    let item = match db.get_item_data(item_id) {
        Ok(Some(i)) => i,
        _ => return Err("item not found".to_string()),
    };
    if item.item_type != ItemType::Tattoo {
        return Err("not a tattoo".to_string());
    }
    if item.wear_locations.is_empty() {
        return Err("tattoo has no wear_location".to_string());
    }
    let location_display = item.wear_locations[0].to_display_string().to_string();

    match db.delete_item(item_id) {
        Ok(true) => {}
        _ => return Err("item not found".to_string()),
    }
    let stamped = apply_to_character(db, connections, char_name, |ch| {
        stamp_tattoo(ch, &item);
        true
    });
    if stamped.is_none() {
        let _ = db.save_item_data(item);
        return Err("character not found".to_string());
    }
    Ok((item.short_desc, location_display))
}

/// Remove the tattoo at `index` from a character, stripping its buffs.
/// Returns true if a tattoo was removed.
pub fn remove_tattoo(db: &Db, connections: &SharedConnections, char_name: &str, index: usize) -> bool {
    let mut removed = false;
    apply_to_character(db, connections, char_name, |ch| {
        removed = strip_tattoo(ch, index);
        removed
    });
    removed
}

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
    // On success: consumes the item, pushes a CharacterTattoo and stamps every
    // ItemAffect as a permanent ActiveBuff (see `apply_tattoo`). Self-only —
    // the verb script must enforce target = self.
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
                let conns_lock = cloned_conns.lock();
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

            let (short_desc, location) = match apply_tattoo(&cloned_db, &cloned_conns, &char_name, &item_uuid) {
                Ok(pair) => pair,
                Err(e) => {
                    out.insert("message".into(), e.into());
                    return out;
                }
            };

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
        remove_tattoo(&cloned_db, &cloned_conns, &char_name, index as usize)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PlayerSession;
    use crate::types::{EffectType, ItemAffect};
    use parking_lot::Mutex;

    fn temp_db() -> (Db, tempfile::TempDir) {
        let temp = tempfile::tempdir().expect("temp dir");
        let db = Db::open(temp.path()).expect("open db");
        (db, temp)
    }

    fn saved(db: &Db, name: &str) -> CharacterData {
        let ch: CharacterData = serde_json::from_value(serde_json::json!({
            "name": name,
            "password_hash": "",
            "current_room_id": Uuid::new_v4(),
        }))
        .expect("build character");
        db.save_character_data(ch.clone()).expect("save");
        ch
    }

    /// Put a character online so the session-first path is the one under test.
    fn online(ch: &CharacterData) -> SharedConnections {
        let conns: SharedConnections = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let (tx_client, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (tx_input, rx_in) = tokio::sync::mpsc::channel::<crate::InputEvent>(1);
        let mut session = PlayerSession::new_for_test(tx_client, tx_input);
        session.character = Some(ch.clone());
        conns.lock().insert(Uuid::new_v4(), session);
        std::mem::forget((rx, rx_in));
        conns
    }

    fn session_char(conns: &SharedConnections) -> CharacterData {
        conns
            .lock()
            .values()
            .find_map(|s| s.character.clone())
            .expect("character is online")
    }

    /// What the regen/hunger/thirst ticks do: write the session copy over the
    /// DB record wholesale.
    fn flush_session(db: &Db, conns: &SharedConnections) {
        db.save_character_data(session_char(conns)).expect("flush");
    }

    fn tattoo_item(db: &Db) -> Uuid {
        let mut item = ItemData::new(
            "rose".to_string(),
            "a red rose".to_string(),
            "A rose inked in red.".to_string(),
        );
        item.vnum = Some("ink:rose".to_string());
        item.item_type = ItemType::Tattoo;
        item.wear_locations = vec![WearLocation::LeftArm];
        item.affects = vec![ItemAffect {
            effect_type: EffectType::CustomSkillBoost,
            magnitude: 2,
            damage_type: None,
            vs_effect: None,
            skill_key: Some("rose_charm".to_string()),
        }];
        let id = item.id;
        db.save_item_data(item).expect("save item");
        id
    }

    const SOURCE: &str = "tattoo:ink:rose:left arm";

    #[test]
    fn apply_reaches_session_and_survives_flush() {
        let (db, _t) = temp_db();
        let ch = saved(&db, "Inked");
        let conns = online(&ch);
        // An unflushed session change the old DB reload used to throw away.
        conns
            .lock()
            .values_mut()
            .for_each(|s| s.character.as_mut().unwrap().hp = 7);

        let item = tattoo_item(&db);
        apply_tattoo(&db, &conns, "Inked", &item).expect("apply");

        let live = session_char(&conns);
        assert_eq!(live.tattoos.len(), 1);
        assert_eq!(live.active_buffs.iter().filter(|b| b.source == SOURCE).count(), 1);
        assert_eq!(live.hp, 7, "unflushed session state is kept");

        flush_session(&db, &conns);
        let stored = db.get_character_data("inked").unwrap().unwrap();
        assert_eq!(stored.tattoos.len(), 1, "a tick flush keeps the tattoo");
        assert!(db.get_item_data(&item).unwrap().is_none(), "item consumed");
    }

    #[test]
    fn item_is_claimed_once() {
        let (db, _t) = temp_db();
        let ch = saved(&db, "Twice");
        let conns = online(&ch);
        let item = tattoo_item(&db);

        apply_tattoo(&db, &conns, "Twice", &item).expect("first apply");
        assert!(apply_tattoo(&db, &conns, "Twice", &item).is_err());
        assert_eq!(session_char(&conns).tattoos.len(), 1);
    }

    #[test]
    fn missing_character_puts_item_back() {
        let (db, _t) = temp_db();
        let conns: SharedConnections = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let item = tattoo_item(&db);

        assert!(apply_tattoo(&db, &conns, "Nobody", &item).is_err());
        assert!(db.get_item_data(&item).unwrap().is_some(), "item restored");
    }

    #[test]
    fn remove_reaches_session_and_survives_flush() {
        let (db, _t) = temp_db();
        let ch = saved(&db, "Faded");
        let conns = online(&ch);
        let item = tattoo_item(&db);
        apply_tattoo(&db, &conns, "Faded", &item).expect("apply");

        assert!(remove_tattoo(&db, &conns, "Faded", 0));
        assert!(!remove_tattoo(&db, &conns, "Faded", 0), "nothing left to remove");

        let live = session_char(&conns);
        assert!(live.tattoos.is_empty());
        assert_eq!(live.active_buffs.iter().filter(|b| b.source == SOURCE).count(), 0);

        flush_session(&db, &conns);
        let stored = db.get_character_data("faded").unwrap().unwrap();
        assert!(stored.tattoos.is_empty(), "a tick flush does not bring it back");
    }
}

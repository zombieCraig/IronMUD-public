// Rhai bindings for the clan (player-organization) system.
//
// API shape:
// - Read getters return plain values (String / Array / Map) — no custom type
//   registration needed.
// - Mutations return a status string: empty = success; non-empty = surfaced
//   to the caller as an error message.
//
// Auth gating (admin checks, rank checks) lives in the calling Rhai script
// (`clan.rhai`). These functions trust their inputs and only enforce data
// invariants (tag format, last-leader-can't-be-removed, etc.).

use crate::SharedConnections;
use crate::db::Db;
use crate::{ClanData, ClanMember, ClanRank, DEFAULT_CLAN_COLOR};
use rhai::{Array, Dynamic, Engine, Map};
use std::sync::Arc;

fn current_game_day(db: &Db) -> i32 {
    db.get_game_time()
        .ok()
        .map(|gt| crate::migration::absolute_game_day(gt.year, gt.month, gt.day) as i32)
        .unwrap_or(0)
}

fn clan_to_map(clan: &ClanData) -> Map {
    let mut m = Map::new();
    m.insert("tag".into(), Dynamic::from(clan.tag.clone()));
    m.insert("name".into(), Dynamic::from(clan.name.clone()));
    m.insert("description".into(), Dynamic::from(clan.description.clone()));
    m.insert("motd".into(), Dynamic::from(clan.motd.clone()));
    m.insert("color".into(), Dynamic::from(clan.display_color().to_string()));
    m.insert("founded_day".into(), Dynamic::from(clan.founded_day as i64));
    m.insert("founder".into(), Dynamic::from(clan.founder.clone()));
    m.insert("member_count".into(), Dynamic::from(clan.members.len() as i64));
    let mut members = Array::new();
    for mem in &clan.members {
        let mut entry = Map::new();
        entry.insert("name".into(), Dynamic::from(mem.name.clone()));
        entry.insert("rank".into(), Dynamic::from(mem.rank.as_str().to_string()));
        entry.insert("joined_day".into(), Dynamic::from(mem.joined_day as i64));
        members.push(Dynamic::from(entry));
    }
    m.insert("members".into(), Dynamic::from(members));
    m
}

/// Apply the clan tag to a character (lowercased canonical name). Returns
/// true if the character existed.
fn set_character_clan_tag(db: &Db, name: &str, tag: Option<String>) -> bool {
    db.update_character(name, |c| {
        c.clan_tag = tag.clone();
    })
    .map(|opt| opt.is_some())
    .unwrap_or(false)
}

pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections) {
    // ===== Read-side =====

    // get_clan_info(tag) -> Map | ()  — full clan card for display.
    let cloned_db = db.clone();
    engine.register_fn("get_clan_info", move |tag: String| -> Dynamic {
        match cloned_db.get_clan(&tag) {
            Ok(Some(clan)) => Dynamic::from(clan_to_map(&clan)),
            _ => Dynamic::UNIT,
        }
    });

    // list_clans() -> Array<Map>
    let cloned_db = db.clone();
    engine.register_fn("list_clans", move || -> Array {
        match cloned_db.list_clans() {
            Ok(list) => list.into_iter().map(|c| Dynamic::from(clan_to_map(&c))).collect(),
            Err(_) => Array::new(),
        }
    });

    // get_clan_color(tag) -> String (defaults to bright yellow if unset/missing)
    let cloned_db = db.clone();
    engine.register_fn("get_clan_color", move |tag: String| -> String {
        match cloned_db.get_clan(&tag) {
            Ok(Some(clan)) => clan.display_color().to_string(),
            _ => DEFAULT_CLAN_COLOR.to_string(),
        }
    });

    // get_clan_name(tag) -> String ("" if no such clan)
    let cloned_db = db.clone();
    engine.register_fn("get_clan_name", move |tag: String| -> String {
        match cloned_db.get_clan(&tag) {
            Ok(Some(clan)) => clan.name,
            _ => String::new(),
        }
    });

    // get_character_clan_tag(name) -> String ("" if none). Reads the
    // mirror on CharacterData (no clan-tree scan).
    let cloned_db = db.clone();
    engine.register_fn("get_character_clan_tag", move |name: String| -> String {
        match cloned_db.get_character_data(&name) {
            Ok(Some(c)) => c.clan_tag.unwrap_or_default(),
            _ => String::new(),
        }
    });

    // get_character_clan_rank(name) -> String ("leader"|"officer"|"member"|"")
    let cloned_db = db.clone();
    engine.register_fn("get_character_clan_rank", move |name: String| -> String {
        let Ok(Some(ch)) = cloned_db.get_character_data(&name) else {
            return String::new();
        };
        let Some(tag) = ch.clan_tag.as_deref() else {
            return String::new();
        };
        match cloned_db.get_clan(tag) {
            Ok(Some(clan)) => clan
                .rank_of(&ch.name)
                .map(|r| r.as_str().to_string())
                .unwrap_or_default(),
            _ => String::new(),
        }
    });

    // get_online_clan_members(tag) -> Array<String> (canonical names)
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn("get_online_clan_members", move |tag: String| -> Array {
        let Ok(Some(clan)) = cloned_db.get_clan(&tag) else {
            return Array::new();
        };
        let online = crate::get_online_players(&conns);
        let mut out = Array::new();
        for player in online {
            if clan.members.iter().any(|m| m.name.eq_ignore_ascii_case(&player.name)) {
                out.push(Dynamic::from(player.name));
            }
        }
        out
    });

    // ===== Admin mutations (caller verifies is_admin) =====

    // create_clan(tag, name) -> String ("" on success)
    let cloned_db = db.clone();
    engine.register_fn("create_clan", move |tag: String, name: String| -> String {
        let tag_up = tag.trim().to_ascii_uppercase();
        if !ClanData::valid_tag(&tag_up) {
            return "Tag must be 2-6 letters or digits.".to_string();
        }
        if name.trim().is_empty() {
            return "Clan name cannot be blank.".to_string();
        }
        if matches!(cloned_db.get_clan(&tag_up), Ok(Some(_))) {
            return format!("A clan tagged [{}] already exists.", tag_up);
        }
        let day = current_game_day(&cloned_db);
        let mut clan = ClanData::new(&tag_up, name.trim(), day);
        clan.color = String::new(); // use default
        if cloned_db.save_clan(&clan).is_err() {
            return "Failed to save clan.".to_string();
        }
        String::new()
    });

    // delete_clan_full(tag) -> String  — also clears clan_tag on all members.
    let cloned_db = db.clone();
    engine.register_fn("delete_clan_full", move |tag: String| -> String {
        let tag_up = tag.trim().to_ascii_uppercase();
        let Ok(Some(clan)) = cloned_db.get_clan(&tag_up) else {
            return format!("No clan tagged [{}].", tag_up);
        };
        for mem in &clan.members {
            set_character_clan_tag(&cloned_db, &mem.name, None);
        }
        if cloned_db.delete_clan(&tag_up).is_err() {
            return "Failed to delete clan.".to_string();
        }
        String::new()
    });

    // clan_set_color(tag, ansi) -> String. Empty string clears (reverts to default).
    let cloned_db = db.clone();
    engine.register_fn("clan_set_color", move |tag: String, ansi: String| -> String {
        let tag_up = tag.trim().to_ascii_uppercase();
        let Ok(Some(mut clan)) = cloned_db.get_clan(&tag_up) else {
            return format!("No clan tagged [{}].", tag_up);
        };
        clan.color = ansi;
        if cloned_db.save_clan(&clan).is_err() {
            return "Failed to save clan.".to_string();
        }
        String::new()
    });

    // clan_set_motd(tag, body) -> String
    let cloned_db = db.clone();
    engine.register_fn("clan_set_motd", move |tag: String, body: String| -> String {
        let tag_up = tag.trim().to_ascii_uppercase();
        let Ok(Some(mut clan)) = cloned_db.get_clan(&tag_up) else {
            return format!("No clan tagged [{}].", tag_up);
        };
        clan.motd = body;
        if cloned_db.save_clan(&clan).is_err() {
            return "Failed to save clan.".to_string();
        }
        String::new()
    });

    // clan_set_description(tag, body) -> String
    let cloned_db = db.clone();
    engine.register_fn("clan_set_description", move |tag: String, body: String| -> String {
        let tag_up = tag.trim().to_ascii_uppercase();
        let Ok(Some(mut clan)) = cloned_db.get_clan(&tag_up) else {
            return format!("No clan tagged [{}].", tag_up);
        };
        clan.description = body;
        if cloned_db.save_clan(&clan).is_err() {
            return "Failed to save clan.".to_string();
        }
        String::new()
    });

    // ===== Roster mutations (caller verifies rank) =====

    // clan_add_member(tag, char_name, rank_str) -> String
    // Bootstraps founder when roster is empty (sets clan.founder).
    let cloned_db = db.clone();
    engine.register_fn(
        "clan_add_member",
        move |tag: String, char_name: String, rank_str: String| -> String {
            let tag_up = tag.trim().to_ascii_uppercase();
            let Ok(Some(mut clan)) = cloned_db.get_clan(&tag_up) else {
                return format!("No clan tagged [{}].", tag_up);
            };
            let Some(rank) = ClanRank::from_str(&rank_str) else {
                return format!("Unknown rank '{}'. Use leader/officer/member.", rank_str);
            };
            // Resolve to canonical character name (case-insensitive lookup).
            let Ok(Some(target)) = cloned_db.get_character_data(&char_name) else {
                return format!("No such character: {}.", char_name);
            };
            if clan.member(&target.name).is_some() {
                return format!("{} is already in [{}].", target.name, tag_up);
            }
            // Belt-and-braces: target shouldn't already belong to a different clan.
            if let Some(existing) = target.clan_tag.as_deref() {
                if !existing.eq_ignore_ascii_case(&tag_up) {
                    return format!(
                        "{} already belongs to clan [{}]; remove them first.",
                        target.name, existing
                    );
                }
            }
            let day = current_game_day(&cloned_db);
            if clan.members.is_empty() && clan.founder.is_empty() {
                clan.founder = target.name.clone();
            }
            clan.members.push(ClanMember {
                name: target.name.clone(),
                rank,
                joined_day: day,
            });
            if cloned_db.save_clan(&clan).is_err() {
                return "Failed to save clan.".to_string();
            }
            if !set_character_clan_tag(&cloned_db, &target.name, Some(tag_up.clone())) {
                return "Failed to update character clan mirror.".to_string();
            }
            String::new()
        },
    );

    // clan_remove_member(tag, char_name) -> String
    // Blocks removing the sole leader; caller must promote a successor first.
    let cloned_db = db.clone();
    engine.register_fn("clan_remove_member", move |tag: String, char_name: String| -> String {
        let tag_up = tag.trim().to_ascii_uppercase();
        let Ok(Some(mut clan)) = cloned_db.get_clan(&tag_up) else {
            return format!("No clan tagged [{}].", tag_up);
        };
        let Some(mem) = clan.member(&char_name) else {
            return format!("{} is not in [{}].", char_name, tag_up);
        };
        if mem.rank == ClanRank::Leader && clan.leader_count() <= 1 {
            return "Cannot remove the sole leader. Promote a successor first.".to_string();
        }
        let canon = mem.name.clone();
        clan.members.retain(|m| !m.name.eq_ignore_ascii_case(&canon));
        if cloned_db.save_clan(&clan).is_err() {
            return "Failed to save clan.".to_string();
        }
        // Clear the mirror only if it still points at this clan.
        let _ = cloned_db.update_character(&canon, |c| {
            if c.clan_tag
                .as_deref()
                .map(|t| t.eq_ignore_ascii_case(&tag_up))
                .unwrap_or(false)
            {
                c.clan_tag = None;
            }
        });
        String::new()
    });

    // clan_set_rank(tag, char_name, rank_str) -> String
    // Prevents demoting the sole leader (would leave clan leaderless).
    let cloned_db = db.clone();
    engine.register_fn(
        "clan_set_rank",
        move |tag: String, char_name: String, rank_str: String| -> String {
            let tag_up = tag.trim().to_ascii_uppercase();
            let Ok(Some(mut clan)) = cloned_db.get_clan(&tag_up) else {
                return format!("No clan tagged [{}].", tag_up);
            };
            let Some(new_rank) = ClanRank::from_str(&rank_str) else {
                return format!("Unknown rank '{}'. Use leader/officer/member.", rank_str);
            };
            let (old_rank, canon) = match clan.member(&char_name) {
                Some(m) => (m.rank, m.name.clone()),
                None => return format!("{} is not in [{}].", char_name, tag_up),
            };
            if old_rank == new_rank {
                return format!("{} is already {}.", canon, new_rank.as_str());
            }
            if old_rank == ClanRank::Leader && new_rank != ClanRank::Leader && clan.leader_count() <= 1 {
                return "Cannot demote the sole leader. Promote a successor first.".to_string();
            }
            if let Some(m) = clan.member_mut(&canon) {
                m.rank = new_rank;
            }
            if cloned_db.save_clan(&clan).is_err() {
                return "Failed to save clan.".to_string();
            }
            String::new()
        },
    );
}

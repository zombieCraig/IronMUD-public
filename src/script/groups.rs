// src/script/groups.rs
// Group/Party system functions for follow, group, ungroup, split, gtell

use crate::SharedConnections;
use crate::db::Db;
use rhai::Engine;
use std::sync::Arc;

/// Register group-related functions
pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections) {
    // ========== Follower/Group Query Functions ==========

    // get_followers(leader_name) -> Array of follower names (online only)
    // Returns all online players whose `following` field matches leader_name
    let conns = connections.clone();
    engine.register_fn("get_followers", move |leader_name: String| -> rhai::Array {
        let leader_lower = leader_name.to_lowercase();
        let conns_guard = conns.lock().unwrap();
        conns_guard
            .values()
            .filter_map(|session| {
                session.character.as_ref().and_then(|char| {
                    if let Some(ref following) = char.following {
                        if following.to_lowercase() == leader_lower {
                            return Some(rhai::Dynamic::from(char.name.clone()));
                        }
                    }
                    None
                })
            })
            .collect()
    });

    // get_group_members(leader_name) -> Array of grouped member names (is_grouped=true)
    // Returns all online players who are both following leader AND have is_grouped=true
    let conns = connections.clone();
    engine.register_fn("get_group_members", move |leader_name: String| -> rhai::Array {
        let leader_lower = leader_name.to_lowercase();
        let conns_guard = conns.lock().unwrap();
        conns_guard
            .values()
            .filter_map(|session| {
                session.character.as_ref().and_then(|char| {
                    if char.is_grouped {
                        if let Some(ref following) = char.following {
                            if following.to_lowercase() == leader_lower {
                                return Some(rhai::Dynamic::from(char.name.clone()));
                            }
                        }
                    }
                    None
                })
            })
            .collect()
    });

    // get_group_members_in_room(leader_name, room_id) -> Array of grouped members in same room
    //
    // Thin wrapper over `crate::group::group_members_in_room`. The kill-credit
    // path needs the same answer from Rust, and two implementations of "who is
    // in this party, here" would drift the moment one of them gained a rule.
    let conns = connections.clone();
    engine.register_fn(
        "get_group_members_in_room",
        move |leader_name: String, room_id: String| -> rhai::Array {
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(uuid) => uuid,
                Err(_) => return rhai::Array::new(),
            };
            crate::group::group_members_in_room(&conns, &leader_name, &room_uuid)
                .into_iter()
                .map(rhai::Dynamic::from)
                .collect()
        },
    );

    // get_followers_in_room(leader_name, room_id) -> Array of followers in same room
    let conns = connections.clone();
    engine.register_fn(
        "get_followers_in_room",
        move |leader_name: String, room_id: String| -> rhai::Array {
            let leader_lower = leader_name.to_lowercase();
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(uuid) => uuid,
                Err(_) => return rhai::Array::new(),
            };
            let conns_guard = conns.lock().unwrap();
            conns_guard
                .values()
                .filter_map(|session| {
                    session.character.as_ref().and_then(|char| {
                        if char.current_room_id == room_uuid {
                            if let Some(ref following) = char.following {
                                if following.to_lowercase() == leader_lower {
                                    return Some(rhai::Dynamic::from(char.name.clone()));
                                }
                            }
                        }
                        None
                    })
                })
                .collect()
        },
    );

    // get_group_leader(char_name) -> String
    // Returns the ultimate leader (follows the chain up), empty string if not following anyone.
    // Wrapper over `crate::group::group_leader` for the same reason as above.
    let conns = connections.clone();
    engine.register_fn("get_group_leader", move |char_name: String| -> String {
        crate::group::group_leader(&conns, &char_name)
    });

    // ========== Follower/Group Modification Functions ==========

    // set_following(char_name, leader_name) -> bool
    // Sets character's following field, clears is_grouped flag
    // Returns false if would create a cycle
    let conns = connections.clone();
    let db_clone = db.clone();
    engine.register_fn("set_following", move |char_name: String, leader_name: String| -> bool {
        let char_lower = char_name.to_lowercase();
        let leader_lower = leader_name.to_lowercase();

        // Prevent self-following (use clear_following for that)
        if char_lower == leader_lower {
            return false;
        }

        // Check for cycles: walk from leader up the chain
        {
            let conns_guard = conns.lock().unwrap();
            let mut current = leader_lower.clone();
            let mut visited = std::collections::HashSet::new();

            while !current.is_empty() {
                if current == char_lower {
                    // Would create a cycle
                    return false;
                }
                if visited.contains(&current) {
                    break; // Already in a cycle, but not including char_name
                }
                visited.insert(current.clone());

                // Find current's following
                let mut next_following: Option<String> = None;
                for session in conns_guard.values() {
                    if let Some(ref char) = session.character {
                        if char.name.to_lowercase() == current {
                            next_following = char.following.clone();
                            break;
                        }
                    }
                }

                current = next_following.map(|s| s.to_lowercase()).unwrap_or_default();
            }
        }

        // Set the following field
        let mut conns_guard = conns.lock().unwrap();
        for session in conns_guard.values_mut() {
            if let Some(ref mut char) = session.character {
                if char.name.to_lowercase() == char_lower {
                    char.following = Some(leader_name);
                    char.following_mobile_id = None; // Clear mobile-follow when following a player
                    char.is_grouped = false; // Clear group status when changing leader
                    let _ = db_clone.save_character_data(char.clone());
                    return true;
                }
            }
        }
        false
    });

    // clear_following(char_name) -> bool
    // Clears following field and is_grouped flag
    let conns = connections.clone();
    let db_clone = db.clone();
    engine.register_fn("clear_following", move |char_name: String| -> bool {
        let char_lower = char_name.to_lowercase();
        let mut conns_guard = conns.lock().unwrap();
        for session in conns_guard.values_mut() {
            if let Some(ref mut char) = session.character {
                if char.name.to_lowercase() == char_lower {
                    char.following = None;
                    char.following_mobile_id = None;
                    char.is_grouped = false;
                    let _ = db_clone.save_character_data(char.clone());
                    return true;
                }
            }
        }
        false
    });

    // set_following_mobile(char_name, mobile_id) -> bool
    // Sets character to follow a mobile instance. Clears player-follow and is_grouped.
    // Returns false if mobile_id fails to parse as a Uuid.
    let conns = connections.clone();
    let db_clone = db.clone();
    engine.register_fn(
        "set_following_mobile",
        move |char_name: String, mobile_id: String| -> bool {
            let parsed = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let char_lower = char_name.to_lowercase();
            let mut conns_guard = conns.lock().unwrap();
            for session in conns_guard.values_mut() {
                if let Some(ref mut char) = session.character {
                    if char.name.to_lowercase() == char_lower {
                        char.following = None;
                        char.following_mobile_id = Some(parsed);
                        char.is_grouped = false;
                        let _ = db_clone.save_character_data(char.clone());
                        return true;
                    }
                }
            }
            false
        },
    );

    // get_following_mobile_id(char_name) -> String (empty if not following a mobile)
    let conns = connections.clone();
    engine.register_fn("get_following_mobile_id", move |char_name: String| -> String {
        let char_lower = char_name.to_lowercase();
        let conns_guard = conns.lock().unwrap();
        for session in conns_guard.values() {
            if let Some(ref char) = session.character {
                if char.name.to_lowercase() == char_lower {
                    return char.following_mobile_id.map(|u| u.to_string()).unwrap_or_default();
                }
            }
        }
        String::new()
    });

    // set_grouped(char_name, is_grouped) -> bool
    // Sets the is_grouped flag for a character
    let conns = connections.clone();
    let db_clone = db.clone();
    engine.register_fn("set_grouped", move |char_name: String, is_grouped: bool| -> bool {
        let char_lower = char_name.to_lowercase();
        let mut conns_guard = conns.lock().unwrap();
        for session in conns_guard.values_mut() {
            if let Some(ref mut char) = session.character {
                if char.name.to_lowercase() == char_lower {
                    char.is_grouped = is_grouped;
                    let _ = db_clone.save_character_data(char.clone());
                    return true;
                }
            }
        }
        false
    });

    // ========== Group Communication Functions ==========

    // broadcast_to_group(leader_name, message, exclude_name) -> ()
    // Sends message to leader and all grouped members (any room)
    let conns = connections.clone();
    engine.register_fn(
        "broadcast_to_group",
        move |leader_name: String, message: String, exclude_name: String| {
            let leader_lower = leader_name.to_lowercase();
            let exclude_lower = exclude_name.to_lowercase();
            let conns_guard = conns.lock().unwrap();

            for (_conn_id, session) in conns_guard.iter() {
                if let Some(ref char) = session.character {
                    let char_lower = char.name.to_lowercase();

                    // Skip excluded player
                    if char_lower == exclude_lower {
                        continue;
                    }

                    // Check if this is the leader
                    let is_leader = char_lower == leader_lower;

                    // Check if this is a grouped member
                    let is_member = char.is_grouped
                        && char
                            .following
                            .as_ref()
                            .map(|f| f.to_lowercase() == leader_lower)
                            .unwrap_or(false);

                    if is_leader || is_member {
                        let _ = session.sender.send(message.clone());
                    }
                }
            }
        },
    );

    // ========== Gold Functions for Split ==========

    // get_character_gold(char_name) -> i64
    // Returns the gold amount for a character
    let conns = connections.clone();
    engine.register_fn("get_character_gold", move |char_name: String| -> i64 {
        let char_lower = char_name.to_lowercase();
        let conns_guard = conns.lock().unwrap();
        for session in conns_guard.values() {
            if let Some(ref char) = session.character {
                if char.name.to_lowercase() == char_lower {
                    return char.gold as i64;
                }
            }
        }
        0
    });

    // get_group_panel(char_name) -> Array of Maps, one per party member
    //
    // Fields: name, is_you, role ("leader"|"member"|"following"), here,
    // hp, max_hp, condition, condition_color, stamina, max_stamina, position,
    // fighting (Array).
    //
    // Everything but `fighting` is read from the session copies, which are
    // authoritative for anyone online and free — the panel is a status display
    // and must not cost a character deserialize per member per look.
    //
    // The asker leads the list, then the leader, then the rest; `group` renders
    // in that order so a member always finds themselves at the top.
    let conns = connections.clone();
    let db_clone = db.clone();
    engine.register_fn("get_group_panel", move |char_name: String| -> rhai::Array {
        let leader = crate::group::group_leader(&conns, &char_name);
        let leader = if leader.is_empty() { char_name.clone() } else { leader };

        // Snapshot everyone online out of the lock in one pass, so neither the
        // chain walks nor the per-room mobile reads below happen while holding
        // it. `std::sync::Mutex` is not reentrant and `group_leader` takes the
        // same lock.
        let (asker_room, online): (Option<uuid::Uuid>, Vec<crate::types::CharacterData>) = {
            let Ok(guard) = conns.lock() else {
                return rhai::Array::new();
            };
            let asker_room = guard.values().find_map(|s| {
                s.character
                    .as_ref()
                    .filter(|c| c.name.eq_ignore_ascii_case(&char_name))
                    .map(|c| c.current_room_id)
            });
            let online: Vec<crate::types::CharacterData> =
                guard.values().filter_map(|s| s.character.as_ref()).cloned().collect();
            (asker_room, online)
        };

        // Membership walks each candidate's whole follow chain rather than
        // comparing one hop. With a one-hop test, a second-level follower in
        // an A <- B <- C chain was missing from their *own* panel — the one row
        // the panel promises to put first.
        let members: Vec<crate::types::CharacterData> = online
            .into_iter()
            .filter(|c| {
                c.name.eq_ignore_ascii_case(&leader)
                    || crate::group::group_leader(&conns, &c.name).eq_ignore_ascii_case(&leader)
            })
            .collect();

        // Asker first, then the leader, then everyone else by name so the
        // ordering does not shuffle between two consecutive `group` calls.
        let mut ordered = members;
        ordered.sort_by(|a, b| {
            let rank = |c: &crate::types::CharacterData| {
                if c.name.eq_ignore_ascii_case(&char_name) {
                    0
                } else if c.name.eq_ignore_ascii_case(&leader) {
                    1
                } else {
                    2
                }
            };
            rank(a)
                .cmp(&rank(b))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        // One mobile read per distinct room, not per member: a party is
        // usually all in one room, and this is a status display that a player
        // can spam.
        let mut room_mobs: std::collections::HashMap<uuid::Uuid, std::collections::HashMap<uuid::Uuid, String>> =
            std::collections::HashMap::new();
        for c in &ordered {
            room_mobs.entry(c.current_room_id).or_insert_with(|| {
                db_clone
                    .get_mobiles_in_room(&c.current_room_id)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|mob| (mob.id, mob.name))
                    .collect()
            });
        }

        ordered
            .into_iter()
            .map(|c| {
                let mut m = rhai::Map::new();
                let is_leader = c.name.eq_ignore_ascii_case(&leader);
                m.insert("name".into(), rhai::Dynamic::from(c.name.clone()));
                m.insert(
                    "is_you".into(),
                    rhai::Dynamic::from(c.name.eq_ignore_ascii_case(&char_name)),
                );
                m.insert(
                    "role".into(),
                    rhai::Dynamic::from(
                        if is_leader {
                            "leader"
                        } else if c.is_grouped {
                            "member"
                        } else {
                            "following"
                        }
                        .to_string(),
                    ),
                );
                m.insert(
                    "here".into(),
                    rhai::Dynamic::from(asker_room.is_some_and(|r| r == c.current_room_id)),
                );
                m.insert("hp".into(), rhai::Dynamic::from(c.hp as i64));
                m.insert("max_hp".into(), rhai::Dynamic::from(c.max_hp as i64));
                // Band *and* colour come from `combat_text`, which owns every
                // health-presentation table in the game. The script gets them
                // as data and concatenates; it must never grow its own
                // thresholds or its own palette.
                let condition = crate::combat_text::Condition::from_hp(c.hp, c.max_hp);
                m.insert("condition".into(), rhai::Dynamic::from(condition.tag().to_string()));
                m.insert(
                    "condition_color".into(),
                    rhai::Dynamic::from(condition.color().to_string()),
                );
                m.insert("stamina".into(), rhai::Dynamic::from(c.stamina as i64));
                m.insert("max_stamina".into(), rhai::Dynamic::from(c.max_stamina as i64));
                m.insert("position".into(), rhai::Dynamic::from(c.position.to_string()));

                // Role legibility: what this member is swinging at. Read from
                // their own `combat.targets`, which is the only honest source —
                // a mobile's Player target carries no identity at all
                // (`target_id` is nil, `target_name` is None; the tick resolves
                // a victim by scanning the room at swing time), so a
                // "who is the ghoul focusing" column would be invented.
                //
                // What each member is attacking is still the thing a party
                // needs: it is how you see the group has split across three
                // mobs instead of focusing one.
                let names = room_mobs.get(&c.current_room_id);
                let fighting: rhai::Array = if c.combat.in_combat {
                    c.combat
                        .targets
                        .iter()
                        .filter_map(|t| match t.target_type {
                            crate::types::CombatTargetType::Player => t.target_name.clone(),
                            crate::types::CombatTargetType::Mobile => names.and_then(|n| n.get(&t.target_id)).cloned(),
                        })
                        .map(rhai::Dynamic::from)
                        .collect()
                } else {
                    rhai::Array::new()
                };
                m.insert("fighting".into(), rhai::Dynamic::from(fighting));
                rhai::Dynamic::from(m)
            })
            .collect()
    });

    // add_character_gold(char_name, amount) -> bool
    // Adds (or subtracts if negative) gold from a character
    let conns = connections.clone();
    let db_clone = db.clone();
    engine.register_fn("add_character_gold", move |char_name: String, amount: i64| -> bool {
        let char_lower = char_name.to_lowercase();
        let mut conns_guard = conns.lock().unwrap();
        for session in conns_guard.values_mut() {
            if let Some(ref mut char) = session.character {
                if char.name.to_lowercase() == char_lower {
                    char.gold = (char.gold as i64 + amount) as i32;
                    if char.gold < 0 {
                        char.gold = 0;
                    }
                    let _ = db_clone.save_character_data(char.clone());
                    return true;
                }
            }
        }
        false
    });
}

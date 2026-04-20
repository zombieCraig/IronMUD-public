// src/script/groups.rs
// Group/Party system functions for follow, group, ungroup, split, gtell

use rhai::Engine;
use crate::db::Db;
use crate::SharedConnections;
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
        conns_guard.values()
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
        conns_guard.values()
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
    let conns = connections.clone();
    engine.register_fn("get_group_members_in_room", move |leader_name: String, room_id: String| -> rhai::Array {
        let leader_lower = leader_name.to_lowercase();
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(uuid) => uuid,
            Err(_) => return rhai::Array::new(),
        };
        let conns_guard = conns.lock().unwrap();
        conns_guard.values()
            .filter_map(|session| {
                session.character.as_ref().and_then(|char| {
                    if char.is_grouped && char.current_room_id == room_uuid {
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

    // get_followers_in_room(leader_name, room_id) -> Array of followers in same room
    let conns = connections.clone();
    engine.register_fn("get_followers_in_room", move |leader_name: String, room_id: String| -> rhai::Array {
        let leader_lower = leader_name.to_lowercase();
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(uuid) => uuid,
            Err(_) => return rhai::Array::new(),
        };
        let conns_guard = conns.lock().unwrap();
        conns_guard.values()
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
    });

    // get_group_leader(char_name) -> String
    // Returns the ultimate leader (follows the chain up), empty string if not following anyone
    let conns = connections.clone();
    engine.register_fn("get_group_leader", move |char_name: String| -> String {
        let char_lower = char_name.to_lowercase();
        let conns_guard = conns.lock().unwrap();

        // Find the character's following field
        let mut current_following: Option<String> = None;
        for session in conns_guard.values() {
            if let Some(ref char) = session.character {
                if char.name.to_lowercase() == char_lower {
                    current_following = char.following.clone();
                    break;
                }
            }
        }

        // If not following anyone, return empty
        let mut leader = match current_following {
            Some(name) => name,
            None => return String::new(),
        };

        // Follow the chain up (with cycle detection)
        let mut visited = std::collections::HashSet::new();
        visited.insert(char_lower);

        loop {
            let leader_lower = leader.to_lowercase();
            if visited.contains(&leader_lower) {
                // Cycle detected, return current leader
                return leader;
            }
            visited.insert(leader_lower.clone());

            // Find leader's following field
            let mut leader_following: Option<String> = None;
            for session in conns_guard.values() {
                if let Some(ref char) = session.character {
                    if char.name.to_lowercase() == leader_lower {
                        leader_following = char.following.clone();
                        break;
                    }
                }
            }

            match leader_following {
                Some(next_leader) => {
                    // Leader is also following someone
                    leader = next_leader;
                }
                None => {
                    // This is the ultimate leader
                    return leader;
                }
            }
        }
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
    engine.register_fn("set_following_mobile", move |char_name: String, mobile_id: String| -> bool {
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
    });

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
    engine.register_fn("broadcast_to_group", move |leader_name: String, message: String, exclude_name: String| {
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
                let is_member = char.is_grouped &&
                    char.following.as_ref()
                        .map(|f| f.to_lowercase() == leader_lower)
                        .unwrap_or(false);

                if is_leader || is_member {
                    let _ = session.sender.send(message.clone());
                }
            }
        }
    });

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

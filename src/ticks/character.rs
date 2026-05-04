//! Character tick systems for IronMUD
//!
//! Handles thirst, hunger, and stamina/HP regeneration for players.

use anyhow::Result;
use tokio::time::{Duration, interval};
use tracing::error;

use ironmud::{BodyPart, CharacterData, CharacterPosition, EffectType, SharedConnections, TemperatureCategory, db};

/// Thirst tick interval in seconds (check thirst every minute)
pub const THIRST_TICK_INTERVAL_SECS: u64 = 60;

/// Hunger tick interval in seconds (check hunger every 2 minutes - slower than thirst)
pub const HUNGER_TICK_INTERVAL_SECS: u64 = 120;

/// Stamina/HP regeneration tick interval in seconds
pub const REGEN_TICK_INTERVAL_SECS: u64 = 10;

/// Background task that processes player thirst periodically
pub async fn run_thirst_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(THIRST_TICK_INTERVAL_SECS));

    loop {
        ticker.tick().await;

        if let Err(e) = process_thirst_tick(&db, &connections) {
            error!("Thirst tick error: {}", e);
        }
    }
}

/// Process thirst for all logged-in players
fn process_thirst_tick(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    let game_time = db.get_game_time()?;
    let temp_category = game_time.get_temperature_category();

    let thirst_base_rate: i32 = db
        .get_setting_or_default("thirst_base_rate", "1")
        .unwrap_or_else(|_| "1".to_string())
        .parse::<i32>()
        .unwrap_or(1)
        .max(1);

    let mut conns = connections.lock().unwrap();

    for (_conn_id, session) in conns.iter_mut() {
        if let Some(ref mut char) = session.character {
            // Skip if character creation is not complete
            if !char.creation_complete {
                continue;
            }

            // Skip thirst for god mode and build mode players
            if char.god_mode || ironmud::check_build_mode(db, &char.name, &char.current_room_id) {
                continue;
            }

            // Calculate base thirst decrease
            let mut decrease = thirst_base_rate;

            // Add temperature modifier (Hot: 1.5x, Sweltering: 2x thirst)
            // Since base is 1, we add 1 for each tier to achieve roughly the multiplier
            let temp_modifier = match temp_category {
                TemperatureCategory::Sweltering => 1, // 2x thirst depletion
                TemperatureCategory::Hot => 1,        // ~1.5x thirst depletion (rounded up)
                _ => 0,
            };

            // Check for traits that modify thirst
            let has_camel = char.traits.iter().any(|t| t == "camel");
            let has_desert_born = char.traits.iter().any(|t| t == "desert_born");
            let has_parched = char.traits.iter().any(|t| t == "parched");

            // Apply heat thirst modifier (unless desert_born)
            if !has_desert_born {
                decrease += temp_modifier;
            }

            // Calculate insulation from equipped items
            let insulation = calculate_character_insulation(char, db);
            if temp_modifier > 0 && insulation > 0 {
                // Reduce heat thirst by insulation percentage (but insulation might make you hotter...)
                // Actually, insulation keeps you warm in cold, but makes you hotter in heat
                // So we'll skip insulation reduction for heat-based thirst
            }

            // Apply trait modifiers
            if has_camel {
                // 50% reduction
                decrease = (decrease + 1) / 2;
            }
            if has_parched {
                // 50% increase
                decrease = decrease + decrease / 2;
            }

            // Position modifier - 50% reduction while sitting or sleeping
            if char.position == CharacterPosition::Sitting || char.position == CharacterPosition::Sleeping {
                decrease = (decrease + 1) / 2;
            }

            // Ensure minimum decrease of 1
            decrease = decrease.max(1);

            let old_thirst = char.thirst;
            char.thirst = (char.thirst - decrease).max(0);

            // Send thirst messages at thresholds
            if let Some(msg) = get_thirst_message(old_thirst, char.thirst, char.max_thirst) {
                let _ = session.sender.send(format!("\n{}\n", msg));
            }

            // Save character if thirst changed
            if old_thirst != char.thirst {
                let _ = db.save_character_data(char.clone());
            }
        }
    }

    Ok(())
}

/// Get thirst message when crossing a threshold
fn get_thirst_message(old: i32, new: i32, max: i32) -> Option<&'static str> {
    let old_pct = (old * 100) / max;
    let new_pct = (new * 100) / max;

    // Only send message when crossing a threshold downward
    if old_pct > 75 && new_pct <= 75 {
        Some("You're starting to feel a bit thirsty.")
    } else if old_pct > 50 && new_pct <= 50 {
        Some("You are thirsty. You should find something to drink.")
    } else if old_pct > 25 && new_pct <= 25 {
        Some("You are very thirsty! Your throat is parched.")
    } else if old_pct > 10 && new_pct <= 10 {
        Some("You are EXTREMELY thirsty! Find water immediately!")
    } else if new == 0 && old > 0 {
        Some("You are dying of thirst! You need water NOW or you will perish!")
    } else {
        None
    }
}

/// Calculate total insulation from equipped items
pub fn calculate_character_insulation(char: &CharacterData, db: &db::Db) -> i32 {
    let mut total_insulation = 0;

    // Query database for equipped items (source of truth is ItemLocation::Equipped)
    if let Ok(equipped_items) = db.get_equipped_items(&char.name) {
        for item in equipped_items {
            total_insulation += item.insulation;
        }
    }

    total_insulation.min(100) // Cap at 100
}

/// Background task that processes player hunger periodically
pub async fn run_hunger_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(HUNGER_TICK_INTERVAL_SECS));

    loop {
        ticker.tick().await;

        if let Err(e) = process_hunger_tick(&db, &connections) {
            error!("Hunger tick error: {}", e);
        }
    }
}

/// Process hunger for all logged-in players
fn process_hunger_tick(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    let hunger_base_rate: i32 = db
        .get_setting_or_default("hunger_base_rate", "1")
        .unwrap_or_else(|_| "1".to_string())
        .parse::<i32>()
        .unwrap_or(1)
        .max(1);

    let mut conns = connections.lock().unwrap();

    for (_conn_id, session) in conns.iter_mut() {
        if let Some(ref mut char) = session.character {
            // Skip if character creation is not complete
            if !char.creation_complete {
                continue;
            }

            // Skip hunger for god mode and build mode players
            if char.god_mode || ironmud::check_build_mode(db, &char.name, &char.current_room_id) {
                continue;
            }

            // Calculate base hunger decrease
            let mut decrease = hunger_base_rate;

            // Hunger rate traits
            let has_efficient = char.traits.iter().any(|t| t == "efficient_metabolism");
            let has_ravenous = char.traits.iter().any(|t| t == "ravenous");
            if has_efficient {
                decrease = (decrease + 1) / 2;
            } // 50% reduction
            if has_ravenous {
                decrease = decrease + decrease * 3 / 4;
            } // 75% increase

            // Position modifier - 50% reduction while sitting or sleeping (less activity)
            if char.position == CharacterPosition::Sitting || char.position == CharacterPosition::Sleeping {
                decrease = (decrease + 1) / 2;
            }

            // Ensure minimum decrease of 1
            decrease = decrease.max(1);

            let old_hunger = char.hunger;
            char.hunger = (char.hunger - decrease).max(0);

            // Send hunger messages at thresholds
            if let Some(msg) = get_hunger_message(old_hunger, char.hunger, char.max_hunger) {
                let _ = session.sender.send(format!("\n{}\n", msg));
            }

            // Save character if hunger changed
            if old_hunger != char.hunger {
                let _ = db.save_character_data(char.clone());
            }
        }
    }

    Ok(())
}

/// Get hunger message when crossing a threshold
fn get_hunger_message(old: i32, new: i32, max: i32) -> Option<&'static str> {
    if max <= 0 {
        return None;
    }
    let old_pct = (old * 100) / max;
    let new_pct = (new * 100) / max;

    // Only send message when crossing a threshold downward
    if old_pct > 75 && new_pct <= 75 {
        Some("Your stomach rumbles slightly.")
    } else if old_pct > 50 && new_pct <= 50 {
        Some("You are getting hungry. You should find something to eat.")
    } else if old_pct > 25 && new_pct <= 25 {
        Some("You are very hungry! Your stomach growls loudly.")
    } else if old_pct > 10 && new_pct <= 10 {
        Some("You are starving! You need to eat something soon!")
    } else if new == 0 && old > 0 {
        Some("You are famished! Your body aches from lack of food.")
    } else {
        None
    }
}

/// Background task that processes stamina and HP regeneration
pub async fn run_regen_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(REGEN_TICK_INTERVAL_SECS));

    loop {
        ticker.tick().await;

        if let Err(e) = process_regen_tick(&db, &connections) {
            error!("Regen tick error: {}", e);
        }
    }
}

/// Process stamina and HP regeneration for all logged-in players
fn process_regen_tick(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    let stamina_regen_standing: i32 = db
        .get_setting_or_default("stamina_regen_standing", "1")
        .unwrap_or_else(|_| "1".to_string())
        .parse::<i32>()
        .unwrap_or(1)
        .max(0);
    let stamina_regen_sitting: i32 = db
        .get_setting_or_default("stamina_regen_sitting", "3")
        .unwrap_or_else(|_| "3".to_string())
        .parse::<i32>()
        .unwrap_or(3)
        .max(0);
    let stamina_regen_sleeping: i32 = db
        .get_setting_or_default("stamina_regen_sleeping", "5")
        .unwrap_or_else(|_| "5".to_string())
        .parse::<i32>()
        .unwrap_or(5)
        .max(1);
    let hp_regen_sitting: i32 = db
        .get_setting_or_default("hp_regen_sitting", "1")
        .unwrap_or_else(|_| "1".to_string())
        .parse::<i32>()
        .unwrap_or(1)
        .max(0);
    let hp_regen_sleeping: i32 = db
        .get_setting_or_default("hp_regen_sleeping", "2")
        .unwrap_or_else(|_| "2".to_string())
        .parse::<i32>()
        .unwrap_or(2)
        .max(0);
    let mana_regen_standing: i32 = db
        .get_setting_or_default("mana_regen_standing", "1")
        .unwrap_or_else(|_| "1".to_string())
        .parse::<i32>()
        .unwrap_or(1)
        .max(0);
    let mana_regen_sitting: i32 = db
        .get_setting_or_default("mana_regen_sitting", "2")
        .unwrap_or_else(|_| "2".to_string())
        .parse::<i32>()
        .unwrap_or(2)
        .max(0);
    let mana_regen_sleeping: i32 = db
        .get_setting_or_default("mana_regen_sleeping", "4")
        .unwrap_or_else(|_| "4".to_string())
        .parse::<i32>()
        .unwrap_or(4)
        .max(1);

    let mut conns = connections.lock().unwrap();

    for (_conn_id, session) in conns.iter_mut() {
        if let Some(ref mut char) = session.character {
            // Skip if character creation is not complete
            if !char.creation_complete {
                continue;
            }

            let mut modified = false;
            let old_stamina = char.stamina;
            let old_position = char.position;

            // Stamina regeneration based on position
            let mut stamina_regen = match char.position {
                CharacterPosition::Standing | CharacterPosition::Swimming => stamina_regen_standing,
                CharacterPosition::Sitting => stamina_regen_sitting,
                CharacterPosition::Sleeping => stamina_regen_sleeping,
            };

            // Check for stamina regen traits
            let has_quick_recovery = char.traits.iter().any(|t| t == "quick_recovery");
            let has_slow_recovery = char.traits.iter().any(|t| t == "slow_recovery");
            let has_marathoner = char.traits.iter().any(|t| t == "marathoner");

            // Apply trait modifiers
            if has_quick_recovery {
                // 50% increase
                stamina_regen = stamina_regen + stamina_regen / 2;
            }
            if has_marathoner {
                // 25% increase
                stamina_regen = stamina_regen + stamina_regen / 4;
            }
            if has_slow_recovery {
                // 50% reduction (minimum 1)
                stamina_regen = (stamina_regen / 2).max(1);
            }

            if char.stamina < char.max_stamina {
                char.stamina = (char.stamina + stamina_regen).min(char.max_stamina);
                modified = true;
            }

            // HP regeneration based on position
            let hp_regen = match char.position {
                CharacterPosition::Standing | CharacterPosition::Swimming => 0,
                CharacterPosition::Sitting => hp_regen_sitting,
                CharacterPosition::Sleeping => hp_regen_sleeping,
            };

            // Hunger affects HP regen speed
            let hp_regen_adjusted = if hp_regen > 0 {
                let hunger_pct = if char.max_hunger > 0 {
                    (char.hunger * 100) / char.max_hunger
                } else {
                    100
                };

                if hunger_pct > 75 {
                    hp_regen * 3 / 2 // 150% - well fed bonus
                } else if hunger_pct > 50 {
                    hp_regen // 100% - normal
                } else if hunger_pct > 25 {
                    (hp_regen + 1) / 2 // ~50% - hungry penalty (min 1 if resting)
                } else {
                    // Starving: 25% but never 0 if base regen > 0
                    (hp_regen / 4).max(1)
                }
            } else {
                0
            };

            // HP regen traits
            let has_vigorous = char.traits.iter().any(|t| t == "vigorous");
            let has_frail = char.traits.iter().any(|t| t == "frail");
            let hp_regen_final = if hp_regen_adjusted > 0 {
                let mut r = hp_regen_adjusted;
                if has_vigorous {
                    r = r + r / 4;
                } // +25%
                if has_frail {
                    r = (r * 3 / 4).max(1);
                } // -25%
                r
            } else {
                0
            };

            // Max HP traits (effective cap)
            let has_tough = char.traits.iter().any(|t| t == "tough");
            let has_sickly = char.traits.iter().any(|t| t == "sickly");
            let mut effective_max_hp = char.max_hp;
            if has_vigorous {
                effective_max_hp = effective_max_hp * 115 / 100;
            } // +15%
            if has_tough {
                effective_max_hp = effective_max_hp * 120 / 100;
            } // +20%
            if has_frail {
                effective_max_hp = effective_max_hp * 80 / 100;
            } // -20%
            if has_sickly {
                effective_max_hp = effective_max_hp * 90 / 100;
            } // -10%

            // Torso wound caps max HP (broken ribs limit vitality)
            let torso_penalty = char
                .wounds
                .iter()
                .filter(|w| w.body_part == BodyPart::Torso)
                .map(|w| w.level.penalty())
                .max()
                .unwrap_or(0);
            if torso_penalty > 0 {
                effective_max_hp = (effective_max_hp * (100 - torso_penalty) / 100).max(1);
                if char.hp > effective_max_hp {
                    char.hp = effective_max_hp;
                    modified = true;
                    let _ = session
                        .sender
                        .send("\nYour cracked ribs limit your vitality.\n".to_string());
                }
            }

            if char.hp < effective_max_hp && hp_regen_final > 0 {
                char.hp = (char.hp + hp_regen_final).min(effective_max_hp);
                modified = true;
            }

            // Wake notification when reaching 10% stamina (from forced sleep)
            if char.position == CharacterPosition::Sleeping && old_stamina == 0 {
                let wake_threshold = char.max_stamina / 10;
                if char.stamina >= wake_threshold {
                    let _ = session
                        .sender
                        .send("\nYou feel rested enough to wake up. Type 'wake' to get up.\n".to_string());
                }
            }

            // Stamina warning messages (decreasing)
            if let Some(msg) = get_stamina_message(old_stamina, char.stamina, char.max_stamina) {
                let _ = session.sender.send(format!("\n{}\n", msg));
            }

            // Force sleep at 0 stamina (or drown in deep/underwater)
            if char.stamina == 0 && old_position != CharacterPosition::Sleeping {
                // Check if character is in deep water or underwater
                let in_drowning_water = if let Ok(Some(room)) = db.get_room_data(&char.current_room_id) {
                    room.flags.deep_water || room.flags.underwater
                } else {
                    false
                };

                if in_drowning_water && !char.god_mode {
                    // Drowning from exhaustion - deal 15% max HP damage
                    let drowning_damage = ((char.max_hp * 15) / 100).max(1);
                    char.hp -= drowning_damage;
                    modified = true;
                    let _ = session.sender.send(format!(
                        "\n\x1b[1;31mExhausted, you struggle to stay afloat! You take {} drowning damage!\x1b[0m\n",
                        drowning_damage
                    ));
                    if char.hp <= 0 {
                        char.hp = 0;
                        char.is_unconscious = true;
                        char.bleedout_rounds_remaining = 1;
                        let _ = session.sender.send(
                            "\n\x1b[1;31mYou lose consciousness as you slip beneath the water...\x1b[0m\n".to_string(),
                        );
                    }
                } else {
                    char.position = CharacterPosition::Sleeping;
                    modified = true;
                    let _ = session
                        .sender
                        .send("\nYou collapse from exhaustion and fall into a deep sleep!\n".to_string());
                }
            }

            // ========== Buff Processing ==========

            if !char.active_buffs.is_empty() {
                let tick_secs = REGEN_TICK_INTERVAL_SECS as i32;

                // Regeneration buff: add magnitude HP per tick (respects effective max HP)
                if let Some(regen_buff) = char
                    .active_buffs
                    .iter()
                    .find(|b| b.effect_type == EffectType::Regeneration)
                {
                    let regen_hp = regen_buff.magnitude;
                    if char.hp < effective_max_hp && regen_hp > 0 {
                        char.hp = (char.hp + regen_hp).min(effective_max_hp);
                        modified = true;
                    }
                }

                // Decrement timers on non-permanent buffs
                for buff in char.active_buffs.iter_mut() {
                    if buff.remaining_secs > 0 {
                        buff.remaining_secs -= tick_secs;
                    }
                }

                // Collect expiry messages and remove expired buffs
                let before_len = char.active_buffs.len();
                let mut expired_messages: Vec<String> = Vec::new();
                char.active_buffs.retain(|b| {
                    // Keep permanent buffs (-1) and those still ticking (> 0)
                    if b.remaining_secs == -1 || b.remaining_secs > 0 {
                        true
                    } else {
                        expired_messages.push(get_buff_expiry_message(b.effect_type));
                        false
                    }
                });

                if char.active_buffs.len() < before_len {
                    modified = true;
                    for msg in &expired_messages {
                        if !msg.is_empty() {
                            let _ = session.sender.send(format!("\n{}\n", msg));
                        }
                    }
                }
            }

            // Mana regeneration (similar to stamina, position-based)
            if char.mana_enabled {
                let mut mana_regen = match char.position {
                    CharacterPosition::Standing | CharacterPosition::Swimming => mana_regen_standing,
                    CharacterPosition::Sitting => mana_regen_sitting,
                    CharacterPosition::Sleeping => mana_regen_sleeping,
                };

                // Mana regen traits
                let has_focused_mind = char.traits.iter().any(|t| t == "focused_mind");
                let has_scattered_thoughts = char.traits.iter().any(|t| t == "scattered_thoughts");
                if has_focused_mind {
                    mana_regen = mana_regen + mana_regen / 2;
                } // +50%
                if has_scattered_thoughts {
                    mana_regen = (mana_regen / 2).max(1);
                } // -50%

                // Max mana traits (effective cap)
                let has_mana_well = char.traits.iter().any(|t| t == "mana_well");
                let has_mana_stunted = char.traits.iter().any(|t| t == "mana_stunted");
                let mut effective_max_mana = char.max_mana;
                if has_mana_well {
                    effective_max_mana = effective_max_mana * 130 / 100;
                } // +30%
                if has_mana_stunted {
                    effective_max_mana = effective_max_mana * 75 / 100;
                } // -25%

                // Head wound caps max mana (concussion effect)
                let head_penalty = char
                    .wounds
                    .iter()
                    .filter(|w| w.body_part == BodyPart::Head)
                    .map(|w| w.level.penalty())
                    .max()
                    .unwrap_or(0);
                if head_penalty > 0 {
                    effective_max_mana = (effective_max_mana * (100 - head_penalty) / 100).max(0);
                }
                if char.mana > effective_max_mana {
                    char.mana = effective_max_mana;
                    modified = true;
                }

                if char.mana < effective_max_mana {
                    char.mana = (char.mana + mana_regen).min(effective_max_mana);
                    modified = true;
                }
            }

            // Drunk decay - decrease by 1 per tick
            if char.drunk_level > 0 {
                char.drunk_level -= 1;
                modified = true;
                if char.drunk_level == 0 {
                    let _ = session.sender.send("\nYou feel sober again.\n".to_string());
                }
            }

            if modified {
                let _ = db.save_character_data(char.clone());
            }
        }
    }

    Ok(())
}

/// Get stamina message when crossing a threshold (decreasing only)
fn get_stamina_message(old: i32, new: i32, max: i32) -> Option<&'static str> {
    let old_pct = (old * 100) / max;
    let new_pct = (new * 100) / max;

    // Only send message when crossing a threshold downward
    if old_pct > 50 && new_pct <= 50 {
        Some("You're starting to feel tired.")
    } else if old_pct > 25 && new_pct <= 25 {
        Some("You are getting exhausted! Consider resting.")
    } else if old_pct > 10 && new_pct <= 10 {
        Some("You are EXTREMELY tired! You need to rest soon!")
    } else if old_pct > 5 && new_pct <= 5 {
        Some("You can barely keep your eyes open! Rest immediately or you will collapse!")
    } else {
        None
    }
}

/// Hunting tick interval in seconds (check hunting every 5 seconds)
pub const HUNTING_TICK_INTERVAL_SECS: u64 = 5;

/// Background task that processes player hunting auto-follow periodically
pub async fn run_hunting_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(HUNTING_TICK_INTERVAL_SECS));

    loop {
        ticker.tick().await;

        if let Err(e) = process_hunting_tick(&db, &connections) {
            error!("Hunting tick error: {}", e);
        }
    }
}

/// Process hunting auto-follow for all logged-in players with active hunt targets
fn process_hunting_tick(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    use super::broadcast::{broadcast_to_room_except_awake, send_message_to_character, sync_character_to_session};
    use super::mobile::get_opposite_direction_rust;

    // Collect hunters from session data (avoid holding lock during processing)
    let hunters: Vec<(String, String, uuid::Uuid)> = {
        let conns = connections.lock().unwrap();
        conns
            .iter()
            .filter_map(|(_, session)| {
                let char = session.character.as_ref()?;
                if char.hunting_target.is_empty() {
                    return None;
                }
                if char.position != CharacterPosition::Standing && char.position != CharacterPosition::Swimming {
                    return None;
                }
                if char.combat.in_combat {
                    return None;
                }
                Some((char.name.clone(), char.hunting_target.clone(), char.current_room_id))
            })
            .collect()
    };

    for (char_name, hunt_target, current_room_id) in hunters {
        // Get room data to check departure records
        let room = match db.get_room_data(&current_room_id)? {
            Some(r) => r,
            None => continue,
        };

        let target_lower = hunt_target.to_lowercase();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        // Check if target player is already in this room (check sessions)
        let target_here = {
            let conns = connections.lock().unwrap();
            conns.iter().any(|(_, session)| {
                if let Some(ref c) = session.character {
                    c.current_room_id == current_room_id
                        && c.name.to_lowercase() == target_lower
                        && c.name.to_lowercase() != char_name.to_lowercase()
                } else {
                    false
                }
            })
        };

        if target_here {
            // Target found - stop hunting
            if let Ok(Some(mut char)) = db.get_character_data(&char_name) {
                char.hunting_target.clear();
                let _ = db.save_character_data(char.clone());
                sync_character_to_session(connections, &char);
            }
            send_message_to_character(connections, &char_name, &format!("You have found {}!", hunt_target));
            continue;
        }

        // Check departure records for target trail
        let mut best_direction: Option<String> = None;
        let mut best_timestamp: i64 = 0;
        for dep in &room.recent_departures {
            if now - dep.timestamp >= 900 {
                continue;
            } // 15 min expiry
            if dep.name.to_lowercase() == target_lower || dep.name.to_lowercase().contains(&target_lower) {
                if dep.timestamp > best_timestamp {
                    best_direction = Some(dep.direction.clone());
                    best_timestamp = dep.timestamp;
                }
            }
        }

        // If no departure record, check blood trails for direction
        if best_direction.is_none() {
            let mut best_blood_ts: i64 = 0;
            for trail in &room.blood_trails {
                if now - trail.timestamp >= 300 {
                    continue;
                }
                if trail.direction.is_none() {
                    continue;
                }
                if trail.name.to_lowercase() == target_lower || trail.name.to_lowercase().contains(&target_lower) {
                    if trail.timestamp > best_blood_ts {
                        best_direction = trail.direction.clone();
                        best_blood_ts = trail.timestamp;
                    }
                }
            }
        }

        // If no departure record or blood trail, check adjacent rooms for target player presence
        if best_direction.is_none() {
            let directions: [(&str, Option<uuid::Uuid>); 6] = [
                ("north", room.exits.north),
                ("south", room.exits.south),
                ("east", room.exits.east),
                ("west", room.exits.west),
                ("up", room.exits.up),
                ("down", room.exits.down),
            ];
            for (dir, exit_opt) in &directions {
                let exit_id = match exit_opt {
                    Some(id) => *id,
                    None => continue,
                };
                // Check if target player is in adjacent room
                let found = {
                    let conns = connections.lock().unwrap();
                    conns.iter().any(|(_, session)| {
                        if let Some(ref c) = session.character {
                            c.current_room_id == exit_id && c.name.to_lowercase() == target_lower
                        } else {
                            false
                        }
                    })
                };

                if found {
                    best_direction = Some(dir.to_string());
                    break;
                }
            }
        }

        let direction = match best_direction {
            Some(d) => d,
            None => continue, // No trail found, keep waiting
        };

        // Get character data for stamina check and movement
        let mut char = match db.get_character_data(&char_name)? {
            Some(c) => c,
            None => continue,
        };

        // Check stamina
        if char.stamina <= 0 && !char.god_mode && !ironmud::check_build_mode(db, &char_name, &char.current_room_id) {
            char.hunting_target.clear();
            let _ = db.save_character_data(char.clone());
            sync_character_to_session(connections, &char);
            send_message_to_character(connections, &char_name, "You are too exhausted to continue hunting.");
            continue;
        }

        // Get target room from direction
        let target_room_id = match direction.as_str() {
            "north" => room.exits.north,
            "south" => room.exits.south,
            "east" => room.exits.east,
            "west" => room.exits.west,
            "up" => room.exits.up,
            "down" => room.exits.down,
            _ => continue,
        };
        let target_room_id = match target_room_id {
            Some(id) => id,
            None => continue,
        };

        // Check for closed doors
        if let Some(door) = room.doors.get(&direction) {
            if door.is_closed {
                continue;
            }
        }

        // Move the character
        let old_room_id = char.current_room_id;
        if !char.god_mode && !ironmud::check_build_mode(db, &char_name, &char.current_room_id) {
            char.stamina -= 1;
            if char.stamina < 0 {
                char.stamina = 0;
            }
        }
        char.current_room_id = target_room_id;
        let _ = db.save_character_data(char.clone());
        sync_character_to_session(connections, &char);

        // Send tracking message
        send_message_to_character(
            connections,
            &char_name,
            &format!("You sense {}'s trail leading {}...", hunt_target, direction),
        );

        // Broadcast departure and arrival
        broadcast_to_room_except_awake(
            connections,
            &old_room_id,
            &format!("{} heads {} following a trail.", char_name, direction),
            &char_name,
        );
        let arrival_dir = get_opposite_direction_rust(&direction);
        broadcast_to_room_except_awake(
            connections,
            &target_room_id,
            &format!("{} arrives from the {}, tracking something.", char_name, arrival_dir),
            &char_name,
        );
    }

    Ok(())
}

/// Get a message to show when a buff expires
/// Drowning tick interval in seconds (same as regen tick)
pub const DROWNING_TICK_INTERVAL_SECS: u64 = 10;

/// Background task that processes drowning for players in underwater rooms
pub async fn run_drowning_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(DROWNING_TICK_INTERVAL_SECS));

    loop {
        ticker.tick().await;

        if let Err(e) = process_drowning_tick(&db, &connections) {
            error!("Drowning tick error: {}", e);
        }
    }
}

/// Process breath/drowning for all logged-in players
fn process_drowning_tick(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    use super::broadcast::{send_message_to_character, sync_character_to_session};

    // Collect player data outside the lock to avoid holding it during db operations
    let players: Vec<(String, uuid::Uuid)> = {
        let conns = connections.lock().unwrap();
        conns
            .iter()
            .filter_map(|(_, session)| {
                let char = session.character.as_ref()?;
                if !char.creation_complete || char.god_mode {
                    return None;
                }
                Some((char.name.clone(), char.current_room_id))
            })
            .collect()
    };

    for (char_name, room_id) in players {
        // Skip build mode players in their editable areas
        if ironmud::check_build_mode(db, &char_name, &room_id) {
            continue;
        }
        let room = match db.get_room_data(&room_id)? {
            Some(r) => r,
            None => continue,
        };

        let mut char = match db.get_character_data(&char_name)? {
            Some(c) => c,
            None => continue,
        };

        if room.flags.underwater {
            // Check for WaterBreathing buff
            let has_water_breathing = char
                .active_buffs
                .iter()
                .any(|b| b.effect_type == EffectType::WaterBreathing);

            if has_water_breathing {
                // Restore breath while having water breathing underwater
                if char.breath < char.max_breath {
                    char.breath = char.max_breath;
                    db.save_character_data(char.clone())?;
                    sync_character_to_session(connections, &char);
                }
                continue;
            }

            // Deplete breath: base 10/tick, swimming skill reduces (-1 per 2 levels, min drain 3)
            let swim_level = char.skills.get("swimming").map(|s| s.level).unwrap_or(0);
            let reduction = swim_level / 2;
            let mut breath_drain = (10 - reduction).max(3);

            // Breath drain traits
            let has_deep_lungs = char.traits.iter().any(|t| t == "deep_lungs");
            let has_iron_lungs = char.traits.iter().any(|t| t == "iron_lungs");
            let has_shallow_breather = char.traits.iter().any(|t| t == "shallow_breather");
            let has_hydrophobic = char.traits.iter().any(|t| t == "hydrophobic");

            let mut drain_modifier: i32 = 0;
            if has_deep_lungs {
                drain_modifier -= 30;
            }
            if has_iron_lungs {
                drain_modifier -= 20;
            }
            if has_shallow_breather {
                drain_modifier += 30;
            }
            if has_hydrophobic {
                drain_modifier += 15;
            }
            breath_drain = (breath_drain * (100 + drain_modifier) / 100).max(1);

            // Effective max breath for threshold messages (capacity traits)
            let mut capacity_modifier: i32 = 0;
            if has_deep_lungs {
                capacity_modifier += 50;
            }
            if has_iron_lungs {
                capacity_modifier += 30;
            }
            let effective_max = char.max_breath * (100 + capacity_modifier) / 100;

            let old_breath = char.breath;
            char.breath = (char.breath - breath_drain).max(0);

            // Threshold messages (use effective_max for trait-adjusted thresholds)
            let max = effective_max;
            if old_breath > max * 75 / 100 && char.breath <= max * 75 / 100 {
                send_message_to_character(connections, &char_name, "\x1b[1;36mYour lungs begin to ache.\x1b[0m");
            } else if old_breath > max * 50 / 100 && char.breath <= max * 50 / 100 {
                send_message_to_character(
                    connections,
                    &char_name,
                    "\x1b[1;36mYou desperately need air! Your vision blurs.\x1b[0m",
                );
            } else if old_breath > max * 25 / 100 && char.breath <= max * 25 / 100 {
                send_message_to_character(
                    connections,
                    &char_name,
                    "\x1b[1;31mYou are about to drown! Get to the surface!\x1b[0m",
                );
            }

            // Drowning damage at breath 0
            if char.breath <= 0 {
                // Drowning damage traits: iron_lungs=-50%, hydrophobic=+25%
                let mut drown_dmg_mod: i32 = 0;
                if has_iron_lungs {
                    drown_dmg_mod -= 50;
                }
                if has_hydrophobic {
                    drown_dmg_mod += 25;
                }
                let drowning_damage = ((char.max_hp * 15) / 100 * (100 + drown_dmg_mod) / 100).max(1);
                char.hp -= drowning_damage;
                send_message_to_character(
                    connections,
                    &char_name,
                    &format!(
                        "\x1b[1;31mYou are drowning! Water fills your lungs for {} damage!\x1b[0m",
                        drowning_damage.max(1)
                    ),
                );

                if char.hp <= 0 {
                    char.hp = 0;
                    char.is_unconscious = true;
                    char.bleedout_rounds_remaining = 1;
                    send_message_to_character(
                        connections,
                        &char_name,
                        "\x1b[1;31mYou lose consciousness as water fills your lungs...\x1b[0m",
                    );
                }
            }

            db.save_character_data(char.clone())?;
            sync_character_to_session(connections, &char);
        } else {
            // Not underwater: recover breath
            if char.breath < char.max_breath {
                let swim_level = char.skills.get("swimming").map(|s| s.level).unwrap_or(0);
                let max_breath_with_skill = char.max_breath + swim_level * 5;

                // Breath recovery traits: aquatic_heritage=+50%, hydrophobic=-25%
                let has_aquatic_heritage = char.traits.iter().any(|t| t == "aquatic_heritage");
                let has_hydrophobic = char.traits.iter().any(|t| t == "hydrophobic");
                let mut recovery_modifier: i32 = 0;
                if has_aquatic_heritage {
                    recovery_modifier += 50;
                }
                if has_hydrophobic {
                    recovery_modifier -= 25;
                }
                let recovery = 25 * (100 + recovery_modifier) / 100;
                char.breath = (char.breath + recovery).min(max_breath_with_skill);

                if char.breath >= char.max_breath && char.breath - 25 < char.max_breath {
                    send_message_to_character(connections, &char_name, "\x1b[1;36mYou catch your breath.\x1b[0m");
                }

                db.save_character_data(char.clone())?;
                sync_character_to_session(connections, &char);
            }
        }
    }

    Ok(())
}

fn get_buff_expiry_message(effect_type: EffectType) -> String {
    match effect_type {
        EffectType::StrengthBoost => "The strength boost wears off.".to_string(),
        EffectType::DexterityBoost => "The agility boost wears off.".to_string(),
        EffectType::ConstitutionBoost => "The resilience boost wears off.".to_string(),
        EffectType::IntelligenceBoost => "The mental sharpness fades.".to_string(),
        EffectType::WisdomBoost => "The heightened perception fades.".to_string(),
        EffectType::CharismaBoost => "The charm effect wears off.".to_string(),
        EffectType::Haste => "The haste effect wears off.".to_string(),
        EffectType::Slow => "The sluggishness fades.".to_string(),
        EffectType::Invisibility => "You fade back into view.".to_string(),
        EffectType::DetectInvisible => "Your enhanced sight returns to normal.".to_string(),
        EffectType::Regeneration => "The regeneration effect wears off.".to_string(),
        EffectType::WaterBreathing => "The water breathing magic fades.".to_string(),
        EffectType::DamageReduction => "The protective aura around you fades.".to_string(),
        _ => String::new(),
    }
}

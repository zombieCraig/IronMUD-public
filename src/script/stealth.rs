// src/script/stealth.rs
// Stealth, thievery, and tracking system functions

use crate::SharedConnections;
use crate::db::Db;
use crate::{BodyPart, CharacterData, EffectType, MobileData};
use rhai::Engine;
use std::sync::Arc;

/// Threshold above which a mobile's `perception` stat pierces hidden / sneak
/// (but not invisibility — that still requires `flags.aware`).
pub const PERCEPTION_PIERCE_THRESHOLD: i32 = 5;

/// Returns true if `mob` can see `char` for the purposes of selecting an
/// aggression / memory target. Encodes the MOB_AWARE rule:
///
/// - Plain visible characters always return true.
/// - Hidden or sneaking PCs are invisible to mobs unless the mob is `aware`
///   or its `perception >= PERCEPTION_PIERCE_THRESHOLD`.
/// - Invisibility-buffed PCs are invisible to mobs unless the mob is `aware`
///   or carries a `DetectInvisible` buff (matches AFF_DETECT_INVIS parity).
///   Perception alone does not pierce magical invisibility.
pub fn is_player_visible_to_mob(character: &CharacterData, mob: &MobileData) -> bool {
    let invisible = character
        .active_buffs
        .iter()
        .any(|b| b.effect_type == EffectType::Invisibility);
    let stealthed = character.is_hidden || character.is_sneaking;

    if !invisible && !stealthed {
        return true;
    }

    if mob.flags.aware {
        return true;
    }

    if invisible {
        return mob
            .active_buffs
            .iter()
            .any(|b| b.effect_type == EffectType::DetectInvisible);
    }

    // Hidden / sneaking only — high perception pierces.
    mob.perception >= PERCEPTION_PIERCE_THRESHOLD
}

#[cfg(test)]
mod visibility_tests {
    use super::*;
    use crate::{ActiveBuff, EffectType, MobileFlags};

    fn base_char() -> CharacterData {
        serde_json::from_value(serde_json::json!({
            "name": "test",
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character")
    }

    fn base_mob() -> MobileData {
        MobileData::new("guard".to_string())
    }

    #[test]
    fn visible_baseline_passes() {
        let c = base_char();
        let m = base_mob();
        assert!(is_player_visible_to_mob(&c, &m));
    }

    #[test]
    fn hidden_blocks_normal_mob() {
        let mut c = base_char();
        c.is_hidden = true;
        let m = base_mob();
        assert!(!is_player_visible_to_mob(&c, &m));
    }

    #[test]
    fn sneaking_blocks_normal_mob() {
        let mut c = base_char();
        c.is_sneaking = true;
        let m = base_mob();
        assert!(!is_player_visible_to_mob(&c, &m));
    }

    #[test]
    fn aware_pierces_hidden() {
        let mut c = base_char();
        c.is_hidden = true;
        let mut m = base_mob();
        m.flags = MobileFlags {
            aware: true,
            ..Default::default()
        };
        assert!(is_player_visible_to_mob(&c, &m));
    }

    #[test]
    fn perception_pierces_sneak() {
        let mut c = base_char();
        c.is_sneaking = true;
        let mut m = base_mob();
        m.perception = 5;
        assert!(is_player_visible_to_mob(&c, &m));
    }

    #[test]
    fn invisibility_blocks_normal_mob() {
        let mut c = base_char();
        c.active_buffs.push(ActiveBuff {
            effect_type: EffectType::Invisibility,
            magnitude: 0,
            remaining_secs: 100,
            source: "test".to_string(),
        });
        let m = base_mob();
        assert!(!is_player_visible_to_mob(&c, &m));
    }

    #[test]
    fn aware_pierces_invisibility() {
        let mut c = base_char();
        c.active_buffs.push(ActiveBuff {
            effect_type: EffectType::Invisibility,
            magnitude: 0,
            remaining_secs: 100,
            source: "test".to_string(),
        });
        let mut m = base_mob();
        m.flags = MobileFlags {
            aware: true,
            ..Default::default()
        };
        assert!(is_player_visible_to_mob(&c, &m));
    }

    #[test]
    fn detect_invisible_buff_pierces_invisibility() {
        let mut c = base_char();
        c.active_buffs.push(ActiveBuff {
            effect_type: EffectType::Invisibility,
            magnitude: 0,
            remaining_secs: 100,
            source: "test".to_string(),
        });
        let mut m = base_mob();
        m.active_buffs.push(ActiveBuff {
            effect_type: EffectType::DetectInvisible,
            magnitude: 0,
            remaining_secs: -1,
            source: "test".to_string(),
        });
        assert!(is_player_visible_to_mob(&c, &m));
    }

    #[test]
    fn perception_does_not_pierce_invisibility() {
        let mut c = base_char();
        c.active_buffs.push(ActiveBuff {
            effect_type: EffectType::Invisibility,
            magnitude: 0,
            remaining_secs: 100,
            source: "test".to_string(),
        });
        let mut m = base_mob();
        m.perception = 10;
        assert!(!is_player_visible_to_mob(&c, &m));
    }
}

/// Register stealth-related functions
pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections) {
    // ========== Stealth State Functions ==========

    // set_hidden(char_name, value) -> bool
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    engine.register_fn("set_hidden", move |char_name: String, value: bool| -> bool {
        let key = char_name.to_lowercase();
        if let Ok(Some(mut character)) = cloned_db.get_character_data(&key) {
            character.is_hidden = value;
            if !value {
                // Breaking hidden also breaks camouflage
                character.is_camouflaged = false;
            }
            if cloned_db.save_character_data(character.clone()).is_ok() {
                // Update session state
                let mut conns = cloned_conns.lock().unwrap();
                for session in conns.values_mut() {
                    if let Some(ref mut sc) = session.character {
                        if sc.name.to_lowercase() == key {
                            sc.is_hidden = character.is_hidden;
                            sc.is_camouflaged = character.is_camouflaged;
                            break;
                        }
                    }
                }
                return true;
            }
        }
        false
    });

    // is_hidden(char_name) -> bool
    let cloned_db = db.clone();
    engine.register_fn("is_hidden", move |char_name: String| -> bool {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(character)) => character.is_hidden,
            _ => false,
        }
    });

    // set_sneaking(char_name, value) -> bool
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    engine.register_fn("set_sneaking", move |char_name: String, value: bool| -> bool {
        let key = char_name.to_lowercase();
        if let Ok(Some(mut character)) = cloned_db.get_character_data(&key) {
            character.is_sneaking = value;
            if cloned_db.save_character_data(character.clone()).is_ok() {
                let mut conns = cloned_conns.lock().unwrap();
                for session in conns.values_mut() {
                    if let Some(ref mut sc) = session.character {
                        if sc.name.to_lowercase() == key {
                            sc.is_sneaking = character.is_sneaking;
                            break;
                        }
                    }
                }
                return true;
            }
        }
        false
    });

    // is_sneaking(char_name) -> bool
    let cloned_db = db.clone();
    engine.register_fn("is_sneaking", move |char_name: String| -> bool {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(character)) => character.is_sneaking,
            _ => false,
        }
    });

    // set_camouflaged(char_name, value) -> bool
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    engine.register_fn("set_camouflaged", move |char_name: String, value: bool| -> bool {
        let key = char_name.to_lowercase();
        if let Ok(Some(mut character)) = cloned_db.get_character_data(&key) {
            character.is_camouflaged = value;
            if cloned_db.save_character_data(character.clone()).is_ok() {
                let mut conns = cloned_conns.lock().unwrap();
                for session in conns.values_mut() {
                    if let Some(ref mut sc) = session.character {
                        if sc.name.to_lowercase() == key {
                            sc.is_camouflaged = character.is_camouflaged;
                            break;
                        }
                    }
                }
                return true;
            }
        }
        false
    });

    // is_camouflaged(char_name) -> bool
    let cloned_db = db.clone();
    engine.register_fn("is_camouflaged", move |char_name: String| -> bool {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(character)) => character.is_camouflaged,
            _ => false,
        }
    });

    // break_stealth(char_name) -> bool
    // Clears hidden, sneaking, and camouflage states
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    engine.register_fn("break_stealth", move |char_name: String| -> bool {
        let key = char_name.to_lowercase();
        if let Ok(Some(mut character)) = cloned_db.get_character_data(&key) {
            let was_stealthy = character.is_hidden || character.is_sneaking || character.is_camouflaged;
            character.is_hidden = false;
            character.is_sneaking = false;
            character.is_camouflaged = false;
            if cloned_db.save_character_data(character.clone()).is_ok() {
                let mut conns = cloned_conns.lock().unwrap();
                for session in conns.values_mut() {
                    if let Some(ref mut sc) = session.character {
                        if sc.name.to_lowercase() == key {
                            sc.is_hidden = false;
                            sc.is_sneaking = false;
                            sc.is_camouflaged = false;
                            break;
                        }
                    }
                }
                return was_stealthy;
            }
        }
        false
    });

    // is_stealthy(char_name) -> bool
    // Returns true if character is in any stealth state
    let cloned_db = db.clone();
    engine.register_fn("is_stealthy", move |char_name: String| -> bool {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(character)) => character.is_hidden || character.is_sneaking || character.is_camouflaged,
            _ => false,
        }
    });

    // ========== Detection Engine ==========

    // calculate_stealth_score(char_name, room_id) -> i64
    // Full stealth score: (stealth_skill * 8) + (dex_mod * 3) + bonuses + random(-10, 10)
    let cloned_db = db.clone();
    engine.register_fn(
        "calculate_stealth_score",
        move |char_name: String, room_id: String| -> i64 {
            let key = char_name.to_lowercase();
            let character = match cloned_db.get_character_data(&key) {
                Ok(Some(c)) => c,
                _ => return 0,
            };

            let stealth_skill = character.skills.get("stealth").map(|s| s.level as i64).unwrap_or(0);
            let dex_mod = (character.stat_dex as i64 - 10) / 2;

            let mut score = (stealth_skill * 8) + (dex_mod * 3);

            // Darkness bonus
            if let Ok(room_uuid) = uuid::Uuid::parse_str(&room_id) {
                if let Ok(Some(room)) = cloned_db.get_room_data(&room_uuid) {
                    if room.flags.dark {
                        score += 20; // pitch dark
                    } else if !room.flags.indoors && !room.flags.city {
                        // Check if nighttime for outdoor rooms
                        if let Ok(game_time) = cloned_db.get_game_time() {
                            if game_time.hour >= 21 || game_time.hour <= 5 {
                                score += 10; // dim (nighttime outdoor)
                            }
                        }
                    }

                    // Terrain bonus for camouflage in wilderness
                    if character.is_camouflaged && room.flags.dirt_floor && !room.flags.city && !room.flags.indoors {
                        score += 15;
                    }
                }
            }

            // Trait bonuses
            if character.traits.iter().any(|t| t == "light_footed") {
                score += 10;
            }
            if character.traits.iter().any(|t| t == "clumsy") {
                score -= 10;
            }
            if character.traits.iter().any(|t| t == "shadow_born") {
                score += 15;
            }
            if character.traits.iter().any(|t| t == "silent_step") {
                score += 10;
            }
            if character.traits.iter().any(|t| t == "conspicuous") {
                score -= 15;
            }
            if character.traits.iter().any(|t| t == "heavy_footed") {
                score -= 10;
            }

            // Armor penalty: -5 per heavy armor piece (armor_class >= 5)
            let items = cloned_db.get_equipped_items(&key);
            if let Ok(equipped) = items {
                for item in &equipped {
                    if let Some(ac) = item.armor_class {
                        if ac >= 5 {
                            score -= 5;
                        }
                    }
                }
            }

            // Random factor (-10 to +10)
            use std::time::SystemTime;
            let seed = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos();
            let random_factor = ((seed % 21) as i64) - 10;
            score += random_factor;

            score
        },
    );

    // calculate_perception_score_player(char_name) -> i64
    // Player perception: max(stealth, tracking) * 8 + wis_mod * 3 + bonuses
    let cloned_db = db.clone();
    engine.register_fn("calculate_perception_score_player", move |char_name: String| -> i64 {
        let key = char_name.to_lowercase();
        let character = match cloned_db.get_character_data(&key) {
            Ok(Some(c)) => c,
            _ => return 0,
        };

        let stealth_skill = character.skills.get("stealth").map(|s| s.level as i64).unwrap_or(0);
        let tracking_skill = character.skills.get("tracking").map(|s| s.level as i64).unwrap_or(0);
        let perception_level = stealth_skill.max(tracking_skill);
        let wis_mod = (character.stat_wis as i64 - 10) / 2;

        let mut score = (perception_level * 8) + (wis_mod * 3);

        // detect_invisible buff bonus
        if character
            .active_buffs
            .iter()
            .any(|b| b.effect_type == EffectType::DetectInvisible)
        {
            score += 30;
        }

        // Sleeping penalty
        if character.position == crate::CharacterPosition::Sleeping {
            score -= 20;
        }

        // Ear wound perception penalty
        let ear_penalty: i64 = {
            let left = character
                .wounds
                .iter()
                .filter(|w| w.body_part == BodyPart::LeftEar)
                .map(|w| w.level.penalty() as i64)
                .max()
                .unwrap_or(0);
            let right = character
                .wounds
                .iter()
                .filter(|w| w.body_part == BodyPart::RightEar)
                .map(|w| w.level.penalty() as i64)
                .max()
                .unwrap_or(0);
            if left > 0 && right > 0 {
                ((left + right) * 50 / 200).min(50)
            } else {
                std::cmp::max(left, right) / 4
            }
        };
        score -= ear_penalty;

        // Random factor (-10 to +10)
        use std::time::SystemTime;
        let seed = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        let random_factor = ((seed % 21) as i64) - 10;
        score += random_factor;

        score
    });

    // calculate_perception_score_mobile(mobile_id) -> i64
    // Mobile perception: (level/2 + perception) * 8 + bonuses
    let cloned_db = db.clone();
    engine.register_fn("calculate_perception_score_mobile", move |mobile_id: String| -> i64 {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return 0,
        };
        let mobile = match cloned_db.get_mobile_data(&uuid) {
            Ok(Some(m)) => m,
            _ => return 0,
        };

        let perception_level = (mobile.level as i64 / 2) + mobile.perception as i64;
        let mut score = perception_level * 8;

        // Guard flag bonus
        if mobile.flags.guard {
            score += 20;
        }
        // Sentinel flag bonus
        if mobile.flags.sentinel {
            score += 10;
        }

        // Activity penalty - sleeping mobiles detect less
        if mobile.current_activity == crate::ActivityState::Sleeping {
            score -= 20;
        }

        // Random factor (-10 to +10)
        use std::time::SystemTime;
        let seed = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        let random_factor = ((seed % 21) as i64) - 10;
        score += random_factor;

        score
    });

    // stealth_check_vs_player(stealther, observer, room_id) -> bool
    // Returns true if stealther remains undetected by observer
    let cloned_db = db.clone();
    engine.register_fn(
        "stealth_check_vs_player",
        move |stealther: String, observer: String, room_id: String| -> bool {
            let stealther_key = stealther.to_lowercase();
            let observer_key = observer.to_lowercase();

            let stealther_char = match cloned_db.get_character_data(&stealther_key) {
                Ok(Some(c)) => c,
                _ => return false,
            };

            // Invisible characters auto-pass vs non-detect_invisible
            if stealther_char
                .active_buffs
                .iter()
                .any(|b| b.effect_type == EffectType::Invisibility)
            {
                if let Ok(Some(observer_char)) = cloned_db.get_character_data(&observer_key) {
                    if !observer_char
                        .active_buffs
                        .iter()
                        .any(|b| b.effect_type == EffectType::DetectInvisible)
                    {
                        return true; // Invisible and observer can't detect
                    }
                }
            }

            // Calculate scores using the same logic as the standalone functions
            let stealth_skill = stealther_char
                .skills
                .get("stealth")
                .map(|s| s.level as i64)
                .unwrap_or(0);
            let dex_mod = (stealther_char.stat_dex as i64 - 10) / 2;
            let mut stealth_score = (stealth_skill * 8) + (dex_mod * 3);

            // Room-based bonuses for stealth
            if let Ok(room_uuid) = uuid::Uuid::parse_str(&room_id) {
                if let Ok(Some(room)) = cloned_db.get_room_data(&room_uuid) {
                    if room.flags.dark {
                        stealth_score += 20;
                    } else if !room.flags.indoors && !room.flags.city {
                        if let Ok(game_time) = cloned_db.get_game_time() {
                            if game_time.hour >= 21 || game_time.hour <= 5 {
                                stealth_score += 10;
                            }
                        }
                    }
                    if stealther_char.is_camouflaged && room.flags.dirt_floor && !room.flags.city && !room.flags.indoors
                    {
                        stealth_score += 15;
                    }
                }
            }

            if stealther_char.traits.iter().any(|t| t == "light_footed") {
                stealth_score += 10;
            }
            if stealther_char.traits.iter().any(|t| t == "clumsy") {
                stealth_score -= 10;
            }
            if stealther_char.traits.iter().any(|t| t == "shadow_born") {
                stealth_score += 15;
            }
            if stealther_char.traits.iter().any(|t| t == "silent_step") {
                stealth_score += 10;
            }
            if stealther_char.traits.iter().any(|t| t == "conspicuous") {
                stealth_score -= 15;
            }
            if stealther_char.traits.iter().any(|t| t == "heavy_footed") {
                stealth_score -= 10;
            }

            // Observer perception
            let observer_char = match cloned_db.get_character_data(&observer_key) {
                Ok(Some(c)) => c,
                _ => return true, // Can't observe = stealth succeeds
            };

            let obs_stealth = observer_char.skills.get("stealth").map(|s| s.level as i64).unwrap_or(0);
            let obs_tracking = observer_char
                .skills
                .get("tracking")
                .map(|s| s.level as i64)
                .unwrap_or(0);
            let obs_perception = obs_stealth.max(obs_tracking);
            let obs_wis_mod = (observer_char.stat_wis as i64 - 10) / 2;
            let mut perception_score = (obs_perception * 8) + (obs_wis_mod * 3);

            if observer_char
                .active_buffs
                .iter()
                .any(|b| b.effect_type == EffectType::DetectInvisible)
            {
                perception_score += 30;
            }
            if observer_char.position == crate::CharacterPosition::Sleeping {
                perception_score -= 20;
            }

            // Random factors
            use std::time::SystemTime;
            let seed = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos();
            let stealth_random = ((seed % 21) as i64) - 10;
            let perception_random = (((seed / 21) % 21) as i64) - 10;

            stealth_score += stealth_random;
            perception_score += perception_random;

            stealth_score > perception_score
        },
    );

    // stealth_check_vs_mobile(stealther, mobile_id, room_id) -> bool
    // Returns true if stealther remains undetected by mobile
    let cloned_db = db.clone();
    engine.register_fn(
        "stealth_check_vs_mobile",
        move |stealther: String, mobile_id: String, room_id: String| -> bool {
            let stealther_key = stealther.to_lowercase();

            let stealther_char = match cloned_db.get_character_data(&stealther_key) {
                Ok(Some(c)) => c,
                _ => return false,
            };

            // Invisible characters auto-pass
            if stealther_char
                .active_buffs
                .iter()
                .any(|b| b.effect_type == EffectType::Invisibility)
            {
                return true; // Mobiles don't have detect_invisible (unless we add it later)
            }

            // Calculate stealth score
            let stealth_skill = stealther_char
                .skills
                .get("stealth")
                .map(|s| s.level as i64)
                .unwrap_or(0);
            let dex_mod = (stealther_char.stat_dex as i64 - 10) / 2;
            let mut stealth_score = (stealth_skill * 8) + (dex_mod * 3);

            if let Ok(room_uuid) = uuid::Uuid::parse_str(&room_id) {
                if let Ok(Some(room)) = cloned_db.get_room_data(&room_uuid) {
                    if room.flags.dark {
                        stealth_score += 20;
                    } else if !room.flags.indoors && !room.flags.city {
                        if let Ok(game_time) = cloned_db.get_game_time() {
                            if game_time.hour >= 21 || game_time.hour <= 5 {
                                stealth_score += 10;
                            }
                        }
                    }
                    if stealther_char.is_camouflaged && room.flags.dirt_floor && !room.flags.city && !room.flags.indoors
                    {
                        stealth_score += 15;
                    }
                }
            }

            if stealther_char.traits.iter().any(|t| t == "light_footed") {
                stealth_score += 10;
            }
            if stealther_char.traits.iter().any(|t| t == "clumsy") {
                stealth_score -= 10;
            }
            if stealther_char.traits.iter().any(|t| t == "shadow_born") {
                stealth_score += 15;
            }
            if stealther_char.traits.iter().any(|t| t == "silent_step") {
                stealth_score += 10;
            }
            if stealther_char.traits.iter().any(|t| t == "conspicuous") {
                stealth_score -= 15;
            }
            if stealther_char.traits.iter().any(|t| t == "heavy_footed") {
                stealth_score -= 10;
            }

            // Mobile perception
            let mobile_uuid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return true,
            };
            let mobile = match cloned_db.get_mobile_data(&mobile_uuid) {
                Ok(Some(m)) => m,
                _ => return true,
            };

            let perception_level = (mobile.level as i64 / 2) + mobile.perception as i64;
            let mut perception_score = perception_level * 8;

            if mobile.flags.guard {
                perception_score += 20;
            }
            if mobile.flags.sentinel {
                perception_score += 10;
            }
            if mobile.current_activity == crate::ActivityState::Sleeping {
                perception_score -= 20;
            }

            // Random factors
            use std::time::SystemTime;
            let seed = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos();
            let stealth_random = ((seed % 21) as i64) - 10;
            let perception_random = (((seed / 21) % 21) as i64) - 10;

            stealth_score += stealth_random;
            perception_score += perception_random;

            stealth_score > perception_score
        },
    );

    // ========== Theft Cooldown Functions ==========

    // get_theft_cooldown(char_name, target_id) -> i64
    // Returns seconds remaining on cooldown (0 = no cooldown)
    let cloned_db = db.clone();
    engine.register_fn(
        "get_theft_cooldown",
        move |char_name: String, target_id: String| -> i64 {
            let key = char_name.to_lowercase();
            match cloned_db.get_character_data(&key) {
                Ok(Some(character)) => {
                    if let Some(&timestamp) = character.theft_cooldowns.get(&target_id) {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::SystemTime::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64;
                        let remaining = timestamp - now;
                        if remaining > 0 { remaining } else { 0 }
                    } else {
                        0
                    }
                }
                _ => 0,
            }
        },
    );

    // set_theft_cooldown(char_name, target_id, duration_secs) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_theft_cooldown",
        move |char_name: String, target_id: String, duration: i64| -> bool {
            let key = char_name.to_lowercase();
            if let Ok(Some(mut character)) = cloned_db.get_character_data(&key) {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;
                character.theft_cooldowns.insert(target_id, now + duration);
                return cloned_db.save_character_data(character.clone()).is_ok();
            }
            false
        },
    );

    // ========== Hunting Target Functions ==========

    // set_hunting_target(char_name, target_name) -> bool
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    engine.register_fn(
        "set_hunting_target",
        move |char_name: String, target_name: String| -> bool {
            let key = char_name.to_lowercase();
            if let Ok(Some(mut character)) = cloned_db.get_character_data(&key) {
                character.hunting_target = target_name;
                if cloned_db.save_character_data(character.clone()).is_ok() {
                    let mut conns = cloned_conns.lock().unwrap();
                    for session in conns.values_mut() {
                        if let Some(ref mut sc) = session.character {
                            if sc.name.to_lowercase() == key {
                                sc.hunting_target = character.hunting_target.clone();
                                break;
                            }
                        }
                    }
                    return true;
                }
            }
            false
        },
    );

    // get_hunting_target(char_name) -> String
    let cloned_db = db.clone();
    engine.register_fn("get_hunting_target", move |char_name: String| -> String {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(character)) => character.hunting_target.clone(),
            _ => String::new(),
        }
    });

    // ========== Envenom Functions ==========

    // get_envenomed_charges(char_name) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_envenomed_charges", move |char_name: String| -> i64 {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(character)) => character.envenomed_charges as i64,
            _ => 0,
        }
    });

    // set_envenomed_charges(char_name, charges) -> bool
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    engine.register_fn(
        "set_envenomed_charges",
        move |char_name: String, charges: i64| -> bool {
            let key = char_name.to_lowercase();
            if let Ok(Some(mut character)) = cloned_db.get_character_data(&key) {
                character.envenomed_charges = charges as i32;
                if cloned_db.save_character_data(character.clone()).is_ok() {
                    let mut conns = cloned_conns.lock().unwrap();
                    for session in conns.values_mut() {
                        if let Some(ref mut sc) = session.character {
                            if sc.name.to_lowercase() == key {
                                sc.envenomed_charges = character.envenomed_charges;
                                break;
                            }
                        }
                    }
                    return true;
                }
            }
            false
        },
    );

    // ========== Circle Cooldown Functions ==========

    // get_circle_cooldown(char_name) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_circle_cooldown", move |char_name: String| -> i64 {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(character)) => character.circle_cooldown,
            _ => 0,
        }
    });

    // set_circle_cooldown(char_name, cooldown) -> bool
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    engine.register_fn("set_circle_cooldown", move |char_name: String, cooldown: i64| -> bool {
        let key = char_name.to_lowercase();
        if let Ok(Some(mut character)) = cloned_db.get_character_data(&key) {
            character.circle_cooldown = cooldown;
            if cloned_db.save_character_data(character.clone()).is_ok() {
                let mut conns = cloned_conns.lock().unwrap();
                for session in conns.values_mut() {
                    if let Some(ref mut sc) = session.character {
                        if sc.name.to_lowercase() == key {
                            sc.circle_cooldown = character.circle_cooldown;
                            break;
                        }
                    }
                }
                return true;
            }
        }
        false
    });

    // ========== Departure Record Functions ==========

    // record_departure(room_id, name, direction, is_sneaking) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "record_departure",
        move |room_id: String, name: String, direction: String, sneaking: bool| -> bool {
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            if let Ok(Some(mut room)) = cloned_db.get_room_data(&room_uuid) {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;

                // Expire old records (>15 real minutes = 900 seconds)
                room.recent_departures.retain(|d| now - d.timestamp < 900);

                // Add new record
                room.recent_departures.push(crate::DepartureRecord {
                    name,
                    direction,
                    timestamp: now,
                    is_sneaking: sneaking,
                });

                // Cap at 10 records
                while room.recent_departures.len() > 10 {
                    room.recent_departures.remove(0);
                }

                return cloned_db.save_room_data(room).is_ok();
            }
            false
        },
    );

    // get_recent_departures(room_id, tracker_skill) -> Array of maps
    // Returns departures visible to the tracker's skill level
    let cloned_db = db.clone();
    engine.register_fn(
        "get_recent_departures",
        move |room_id: String, tracker_skill: i64| -> rhai::Array {
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return vec![],
            };
            let room = match cloned_db.get_room_data(&room_uuid) {
                Ok(Some(r)) => r,
                _ => return vec![],
            };

            let now = std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            let mut results = rhai::Array::new();
            for dep in &room.recent_departures {
                // Expire old records
                if now - dep.timestamp >= 900 {
                    continue;
                }
                // Sneaking departures require tracking >= 5, unless blood trail exists
                if dep.is_sneaking && tracker_skill < 5 {
                    // Check if there's a blood trail for this person (bleeding negates sneaking)
                    let dep_name_lower = dep.name.to_lowercase();
                    let has_blood = room
                        .blood_trails
                        .iter()
                        .any(|t| now - t.timestamp < 300 && t.name.to_lowercase() == dep_name_lower);
                    if !has_blood {
                        continue;
                    }
                }
                let mut map = rhai::Map::new();
                map.insert("name".into(), rhai::Dynamic::from(dep.name.clone()));
                map.insert("direction".into(), rhai::Dynamic::from(dep.direction.clone()));
                map.insert("timestamp".into(), rhai::Dynamic::from(dep.timestamp));
                map.insert("is_sneaking".into(), rhai::Dynamic::from(dep.is_sneaking));
                // Freshness
                let age = now - dep.timestamp;
                let freshness = if age < 120 {
                    "fresh"
                } else if age < 480 {
                    "recent"
                } else {
                    "faint"
                };
                map.insert("freshness".into(), rhai::Dynamic::from(freshness.to_string()));
                results.push(rhai::Dynamic::from(map));
            }
            results
        },
    );

    // clear_departures_for(room_id, char_name) -> bool
    // Remove a character's trail from a room (cover tracks)
    let cloned_db = db.clone();
    engine.register_fn(
        "clear_departures_for",
        move |room_id: String, char_name: String| -> bool {
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            if let Ok(Some(mut room)) = cloned_db.get_room_data(&room_uuid) {
                let name_lower = char_name.to_lowercase();
                room.recent_departures.retain(|d| d.name.to_lowercase() != name_lower);
                return cloned_db.save_room_data(room).is_ok();
            }
            false
        },
    );

    // find_target_direction(room_id, target_name) -> String
    // Check departures + adjacent rooms for target, return direction or empty
    let cloned_db = db.clone();
    engine.register_fn(
        "find_target_direction",
        move |room_id: String, target_name: String| -> String {
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return String::new(),
            };
            let room = match cloned_db.get_room_data(&room_uuid) {
                Ok(Some(r)) => r,
                _ => return String::new(),
            };

            let target_lower = target_name.to_lowercase();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            // First check departure records
            let mut best_dep: Option<&crate::DepartureRecord> = None;
            for dep in &room.recent_departures {
                if now - dep.timestamp >= 900 {
                    continue;
                }
                if dep.name.to_lowercase() == target_lower {
                    if best_dep.is_none() || dep.timestamp > best_dep.unwrap().timestamp {
                        best_dep = Some(dep);
                    }
                }
            }
            if let Some(dep) = best_dep {
                return dep.direction.clone();
            }

            // Then check adjacent rooms for target presence
            let directions = [
                ("north", room.exits.north),
                ("south", room.exits.south),
                ("east", room.exits.east),
                ("west", room.exits.west),
                ("up", room.exits.up),
                ("down", room.exits.down),
            ];

            for (dir, exit) in &directions {
                if let Some(target_room_id) = exit {
                    // Check if target player is in adjacent room
                    if let Ok(Some(_char)) = cloned_db.get_character_data(&target_lower) {
                        if _char.current_room_id == *target_room_id {
                            return dir.to_string();
                        }
                    }
                    // Check mobiles in adjacent room
                    if let Ok(mobiles) = cloned_db.get_mobiles_in_room(target_room_id) {
                        for mob in &mobiles {
                            if mob.name.to_lowercase().contains(&target_lower) {
                                return dir.to_string();
                            }
                        }
                    }
                }
            }

            String::new()
        },
    );

    // ========== Room Trap Functions ==========

    // add_room_trap(room_id, trap_type, owner, damage, detect_diff, disarm_diff, charges, effect) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "add_room_trap",
        move |room_id: String,
              trap_type: String,
              owner: String,
              damage: i64,
              detect_diff: i64,
              disarm_diff: i64,
              charges: i64,
              effect: String|
              -> bool {
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            if let Ok(Some(mut room)) = cloned_db.get_room_data(&room_uuid) {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;
                room.traps.push(crate::RoomTrap {
                    trap_type,
                    owner_name: owner,
                    damage: damage as i32,
                    detect_difficulty: detect_diff as i32,
                    disarm_difficulty: disarm_diff as i32,
                    charges: charges as i32,
                    effect,
                    placed_at: now,
                });
                return cloned_db.save_room_data(room).is_ok();
            }
            false
        },
    );

    // get_room_traps(room_id) -> Array of maps
    let cloned_db = db.clone();
    engine.register_fn("get_room_traps", move |room_id: String| -> rhai::Array {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return vec![],
        };
        let room = match cloned_db.get_room_data(&room_uuid) {
            Ok(Some(r)) => r,
            _ => return vec![],
        };

        let mut results = rhai::Array::new();
        for (i, trap) in room.traps.iter().enumerate() {
            let mut map = rhai::Map::new();
            map.insert("index".into(), rhai::Dynamic::from(i as i64));
            map.insert("trap_type".into(), rhai::Dynamic::from(trap.trap_type.clone()));
            map.insert("owner_name".into(), rhai::Dynamic::from(trap.owner_name.clone()));
            map.insert("damage".into(), rhai::Dynamic::from(trap.damage as i64));
            map.insert(
                "detect_difficulty".into(),
                rhai::Dynamic::from(trap.detect_difficulty as i64),
            );
            map.insert(
                "disarm_difficulty".into(),
                rhai::Dynamic::from(trap.disarm_difficulty as i64),
            );
            map.insert("charges".into(), rhai::Dynamic::from(trap.charges as i64));
            map.insert("effect".into(), rhai::Dynamic::from(trap.effect.clone()));
            results.push(rhai::Dynamic::from(map));
        }
        results
    });

    // remove_room_trap(room_id, index) -> bool
    let cloned_db = db.clone();
    engine.register_fn("remove_room_trap", move |room_id: String, index: i64| -> bool {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        if let Ok(Some(mut room)) = cloned_db.get_room_data(&room_uuid) {
            let idx = index as usize;
            if idx < room.traps.len() {
                room.traps.remove(idx);
                return cloned_db.save_room_data(room).is_ok();
            }
        }
        false
    });

    // decrement_trap_charge(room_id, index) -> i64
    // Returns remaining charges after decrement, removes trap if 0
    let cloned_db = db.clone();
    engine.register_fn("decrement_trap_charge", move |room_id: String, index: i64| -> i64 {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return 0,
        };
        if let Ok(Some(mut room)) = cloned_db.get_room_data(&room_uuid) {
            let idx = index as usize;
            if idx < room.traps.len() {
                room.traps[idx].charges -= 1;
                let remaining = room.traps[idx].charges as i64;
                if remaining <= 0 {
                    room.traps.remove(idx);
                }
                let _ = cloned_db.save_room_data(room);
                return remaining;
            }
        }
        0
    });

    // ========== Mobile Perception Setter ==========

    // set_mobile_perception(mobile_id, value) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_mobile_perception", move |mobile_id: String, value: i64| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                mobile.perception = (value as i32).clamp(0, 10);
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
        }
        false
    });

    // ========== Blood Trail Functions ==========

    // add_blood_trail(room_id, name, severity, direction) -> bool
    // Upserts a blood trail with direction (used by go.rhai for directional trails)
    let cloned_db = db.clone();
    engine.register_fn(
        "add_blood_trail",
        move |room_id: String, name: String, severity: i64, direction: String| -> bool {
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            if let Ok(Some(mut room)) = cloned_db.get_room_data(&room_uuid) {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;

                // Expire old trails (>5 minutes)
                room.blood_trails.retain(|t| now - t.timestamp < 300);

                let sev = (severity as i32).clamp(1, 5);
                let dir = if direction.is_empty() { None } else { Some(direction) };

                // Upsert: find existing trail by name (case-insensitive)
                let name_lower = name.to_lowercase();
                if let Some(existing) = room
                    .blood_trails
                    .iter_mut()
                    .find(|t| t.name.to_lowercase() == name_lower)
                {
                    existing.timestamp = now;
                    existing.severity = sev;
                    existing.direction = dir;
                } else {
                    room.blood_trails.push(crate::BloodTrail {
                        name,
                        severity: sev,
                        timestamp: now,
                        direction: dir,
                    });
                }

                // Cap at 10 trails
                while room.blood_trails.len() > 10 {
                    room.blood_trails.remove(0);
                }

                return cloned_db.save_room_data(room).is_ok();
            }
            false
        },
    );

    // get_blood_trails(room_id) -> Array of maps with name/severity/timestamp/direction
    let cloned_db = db.clone();
    engine.register_fn("get_blood_trails", move |room_id: String| -> rhai::Array {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return vec![],
        };
        let room = match cloned_db.get_room_data(&room_uuid) {
            Ok(Some(r)) => r,
            _ => return vec![],
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let mut results = rhai::Array::new();
        for trail in &room.blood_trails {
            if now - trail.timestamp >= 300 {
                continue; // Expired
            }
            let mut map = rhai::Map::new();
            map.insert("name".into(), rhai::Dynamic::from(trail.name.clone()));
            map.insert("severity".into(), rhai::Dynamic::from(trail.severity as i64));
            map.insert("timestamp".into(), rhai::Dynamic::from(trail.timestamp));
            map.insert(
                "direction".into(),
                rhai::Dynamic::from(trail.direction.clone().unwrap_or_default()),
            );
            results.push(rhai::Dynamic::from(map));
        }
        results
    });
}

//! Combat tick system for IronMUD
//!
//! Handles combat rounds, damage calculation, wounds, and death processing.

use anyhow::Result;
use tokio::time::{Duration, interval};
use tracing::{debug, error};

use ironmud::{
    BodyPart, CharacterData, CharacterPosition, CombatDistance, CombatTarget, CombatTargetType, DamageType, EffectType,
    ItemLocation, ItemType, MobileData, STARTING_ROOM_ID, SharedConnections, SharedState, SkillProgress, WeaponSkill,
    WearLocation, WoundLevel, WoundType, db,
};

use super::corpse::{CorpseBuilder, mobile_gold_with_variance};
use super::wounds::{add_wound_bleeding, escalate_wound_to_severe};

use crate::ticks::broadcast::{
    broadcast_to_room_awake, broadcast_to_room_except, broadcast_to_room_except_awake, send_message_to_character,
    sync_character_to_session,
};
use crate::ticks::mobile::{find_player_name_in_room, get_opposite_direction_rust, get_valid_wander_exits};

/// Combat tick interval in seconds (5 second rounds)
pub const COMBAT_TICK_INTERVAL_SECS: u64 = 5;

/// Background task that processes combat rounds periodically (5 second rounds)
pub async fn run_combat_tick(db: db::Db, connections: SharedConnections, state: SharedState) {
    let mut ticker = interval(Duration::from_secs(COMBAT_TICK_INTERVAL_SECS));

    loop {
        ticker.tick().await;

        if let Err(e) = process_combat_round(&db, &connections, &state) {
            error!("Combat tick error: {}", e);
        }
    }
}

/// Process a combat round for all combatants
fn process_combat_round(db: &db::Db, connections: &SharedConnections, state: &SharedState) -> Result<()> {
    use std::time::Instant;
    let round_start = Instant::now();

    tracing::trace!("Combat tick: getting characters in combat");
    // Get all characters in combat
    let char_names = db.get_all_characters_in_combat()?;
    tracing::trace!("Combat tick: found {} characters", char_names.len());

    tracing::trace!("Combat tick: getting mobiles in combat");
    // Get all mobiles in combat
    let mobile_ids = db.get_all_mobiles_in_combat()?;
    tracing::trace!("Combat tick: found {} mobiles", mobile_ids.len());

    // Process character combat
    for char_name in &char_names {
        let start = Instant::now();
        tracing::trace!("Combat tick: processing character {}", char_name);
        if let Err(e) = process_character_combat_round(db, connections, char_name) {
            debug!("Error processing combat for {}: {}", char_name, e);
        }
        let elapsed = start.elapsed();
        if elapsed.as_millis() > 100 {
            tracing::warn!("Combat tick: character {} took {}ms", char_name, elapsed.as_millis());
        }
        tracing::trace!("Combat tick: done with character {}", char_name);
    }

    // Process mobile combat
    for mobile_id in &mobile_ids {
        let start = Instant::now();
        tracing::trace!("Combat tick: processing mobile {}", mobile_id);
        if let Err(e) = process_mobile_combat_round(db, connections, mobile_id, state) {
            debug!("Error processing combat for mobile {}: {}", mobile_id, e);
        }
        let elapsed = start.elapsed();
        if elapsed.as_millis() > 100 {
            tracing::warn!("Combat tick: mobile {} took {}ms", mobile_id, elapsed.as_millis());
        }
        tracing::trace!("Combat tick: done with mobile {}", mobile_id);
    }

    let round_elapsed = round_start.elapsed();
    if round_elapsed.as_millis() > 500 {
        tracing::warn!("Combat tick: total round took {}ms", round_elapsed.as_millis());
    }
    tracing::trace!("Combat tick: complete in {}ms", round_elapsed.as_millis());
    Ok(())
}

/// Process a combat round for a single character
fn process_character_combat_round(db: &db::Db, connections: &SharedConnections, char_name: &str) -> Result<()> {
    debug!("Processing combat for character {}", char_name);
    let mut char = match db.get_character_data(char_name)? {
        Some(c) => c,
        None => {
            debug!("Character {} not found", char_name);
            return Ok(());
        }
    };

    debug!(
        "Character {} in_combat={}, targets={}",
        char_name,
        char.combat.in_combat,
        char.combat.targets.len()
    );

    // Skip if not actually in combat
    if !char.combat.in_combat || char.combat.targets.is_empty() {
        debug!("Character {} skipping - not in combat or no targets", char_name);
        return Ok(());
    }

    // Get room ID for messaging
    let room_id = char.current_room_id;

    // Handle unconscious state - process bleedout timer
    if char.is_unconscious {
        char.bleedout_rounds_remaining -= 1;

        if char.bleedout_rounds_remaining <= 0 {
            // Bleedout timer expired - death!
            process_player_death(db, connections, &mut char, &room_id)?;
            return Ok(());
        }

        send_message_to_character(
            connections,
            char_name,
            &format!(
                "You are unconscious and bleeding out! {} rounds remaining...",
                char.bleedout_rounds_remaining
            ),
        );
        db.save_character_data(char)?;
        return Ok(());
    }

    // Handle stun
    if char.combat.stun_rounds_remaining > 0 {
        char.combat.stun_rounds_remaining -= 1;
        db.save_character_data(char.clone())?;

        send_message_to_character(connections, char_name, "You are stunned and cannot act!");
        broadcast_to_room_except(connections, &room_id, &format!("{} is stunned!", char.name), char_name);
        return Ok(());
    }

    // Apply ongoing effects (burn, frost, poison, acid)
    if !char.ongoing_effects.is_empty() {
        // Poison resistance traits
        let has_venom_ward = char.traits.iter().any(|t| t == "venom_ward");
        let has_toxin_tolerant = char.traits.iter().any(|t| t == "toxin_tolerant");
        let has_weak_constitution = char.traits.iter().any(|t| t == "weak_constitution");
        let has_hemophiliac_oe = char.traits.iter().any(|t| t == "hemophiliac");

        let mut poison_mod: i32 = 100;
        if has_venom_ward {
            poison_mod -= 50;
        }
        if has_toxin_tolerant {
            poison_mod -= 30;
        }
        if has_weak_constitution {
            poison_mod += 50;
        }
        if has_hemophiliac_oe {
            poison_mod += 20;
        }
        poison_mod = poison_mod.max(10); // minimum 10% damage

        let per_effect_damage: Vec<i32> = char
            .ongoing_effects
            .iter()
            .map(|e| {
                let raw = if e.effect_type == "poison" {
                    (e.damage_per_round * poison_mod / 100).max(1)
                } else {
                    e.damage_per_round
                };
                ironmud::script::apply_damage_reduction(raw, &char.active_buffs)
            })
            .collect();
        let ongoing_damage: i32 = per_effect_damage.iter().sum();
        if ongoing_damage > 0 {
            char.hp -= ongoing_damage;

            // Build message from active effects
            for (effect, &effect_dmg) in char.ongoing_effects.iter().zip(per_effect_damage.iter()) {
                let msg = match effect.effect_type.as_str() {
                    "fire" => format!("You continue to burn! ({} damage)", effect_dmg),
                    "cold" => format!("The frostbite spreads! ({} damage)", effect_dmg),
                    "poison" => format!("The poison courses through your veins! ({} damage)", effect_dmg),
                    "acid" => format!("The acid eats into your flesh! ({} damage)", effect_dmg),
                    "lightning" => format!("Static surges through your nerves! ({} damage)", effect_dmg),
                    _ => format!("You suffer ongoing damage! ({} damage)", effect_dmg),
                };
                send_message_to_character(connections, char_name, &msg);
            }
        }

        // Decrement rounds and remove expired
        for effect in char.ongoing_effects.iter_mut() {
            effect.rounds_remaining -= 1;
        }
        char.ongoing_effects.retain(|e| e.rounds_remaining > 0);
        db.save_character_data(char.clone())?;

        if char.hp <= 0 {
            char.is_unconscious = true;
            char.bleedout_rounds_remaining = 5;
            db.save_character_data(char.clone())?;
            sync_character_to_session(connections, &char);

            send_message_to_character(connections, char_name, "You collapse, unconscious!");
            broadcast_to_room_except(
                connections,
                &room_id,
                &format!("{} collapses, unconscious!", char.name),
                char_name,
            );
            return Ok(());
        }
    }

    // Illness combat miss: 25% chance to skip turn when significantly ill
    if char.has_illness && char.illness_progress > 25 {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        if rng.gen_range(0..100) < 25 {
            send_message_to_character(connections, char_name, "You double over, too sick to fight!");
            broadcast_to_room_except(
                connections,
                &room_id,
                &format!("{} doubles over, looking ill.", char.name),
                char_name,
            );
            db.save_character_data(char)?;
            return Ok(());
        }
    }

    // Poison combat miss: 25% chance to skip turn when poisoned
    if char.wounds.iter().any(|w| w.wound_type == WoundType::Poisoned) {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        if rng.gen_range(0..100) < 25 {
            send_message_to_character(connections, char_name, "You double over as poison racks your body!");
            broadcast_to_room_except(
                connections,
                &room_id,
                &format!("{} doubles over, wracked by poison.", char.name),
                char_name,
            );
            db.save_character_data(char)?;
            return Ok(());
        }
    }

    // Check reloading state - skip attack turn but finish reload
    if char.combat.reloading {
        char.combat.reloading = false;
        db.save_character_data(char.clone())?;
        sync_character_to_session(connections, &char);
        send_message_to_character(connections, char_name, "You finish reloading.");
        return Ok(());
    }

    // Check stamina for combat action
    const COMBAT_STAMINA_COST: i32 = 5;
    const MIN_STAMINA_RESTORE: i32 = 5;

    if char.stamina <= 0 {
        // Too exhausted - skip turn but restore minimum stamina
        char.stamina = MIN_STAMINA_RESTORE;
        db.save_character_data(char.clone())?;
        sync_character_to_session(connections, &char);
        send_message_to_character(
            connections,
            char_name,
            "You are too exhausted to attack! You catch your breath...",
        );
        return Ok(());
    }

    // Consume stamina for attack
    char.stamina = (char.stamina - COMBAT_STAMINA_COST).max(0);

    // Get primary target
    let target = match char.combat.targets.first() {
        Some(t) => t.clone(),
        None => {
            // No target, exit combat
            char.combat.in_combat = false;
            char.combat.targets.clear();
            char.combat.ammo_depleted = 0;
            db.save_character_data(char)?;
            return Ok(());
        }
    };

    // Process attack based on target type
    match target.target_type {
        CombatTargetType::Mobile => {
            process_character_attacks_mobile(db, connections, &mut char, &target.target_id)?;
        }
        CombatTargetType::Player => {
            process_character_attacks_player(db, connections, &mut char, &target.target_id)?;
        }
    }

    // After the swing, scan for HELPER mobiles in the room that should join
    // combat to defend a factional ally the PC is attacking.
    if matches!(target.target_type, CombatTargetType::Mobile) {
        if let Err(e) = process_helper_joins(db, connections, &char, &room_id) {
            debug!("Helper join scan error for {}: {}", char_name, e);
        }
    }

    db.save_character_data(char.clone())?;
    sync_character_to_session(connections, &char);
    Ok(())
}

/// Scan the PC's room for HELPER mobiles that should join combat against the PC
/// to defend an ally currently being attacked. Same room only.
///
/// Ally match: faction strings compared case-insensitively. Both empty/None =
/// ally (Circle-stock fallback). One side tagged and the other empty = NOT ally —
/// a tagged faction explicitly opts out of the generic pool.
fn process_helper_joins(
    db: &db::Db,
    connections: &SharedConnections,
    attacker: &CharacterData,
    room_id: &uuid::Uuid,
) -> Result<()> {
    let victim_ids: Vec<uuid::Uuid> = attacker
        .combat
        .targets
        .iter()
        .filter(|t| t.target_type == CombatTargetType::Mobile)
        .map(|t| t.target_id)
        .collect();
    if victim_ids.is_empty() {
        return Ok(());
    }

    let victims: Vec<MobileData> = victim_ids
        .iter()
        .filter_map(|id| db.get_mobile_data(id).ok().flatten())
        .filter(|m| m.current_room_id.as_ref() == Some(room_id))
        .collect();
    if victims.is_empty() {
        return Ok(());
    }

    let candidates = db.get_mobiles_in_room(room_id)?;
    for candidate in candidates {
        if !candidate.flags.helper {
            continue;
        }
        if candidate.combat.in_combat {
            continue;
        }
        if candidate.flags.no_attack {
            continue;
        }
        if candidate.current_hp <= 0 || candidate.is_unconscious {
            continue;
        }
        if victim_ids.contains(&candidate.id) {
            continue;
        }

        let Some(ally_name) = victims
            .iter()
            .find(|v| factions_match(&candidate.faction, &v.faction))
            .map(|v| v.name.clone())
        else {
            continue;
        };

        let player_target_id = uuid::Uuid::nil();
        let _ = db.update_mobile(&candidate.id, |m| {
            m.combat.in_combat = true;
            if !m
                .combat
                .targets
                .iter()
                .any(|t| t.target_type == CombatTargetType::Player)
            {
                m.combat.targets.push(CombatTarget {
                    target_type: CombatTargetType::Player,
                    target_id: player_target_id,
                });
            }
            m.combat.distances.insert(player_target_id, CombatDistance::Melee);
        });

        broadcast_to_room_awake(
            connections,
            room_id,
            &format!("{} rushes to {}'s aid!", candidate.name, ally_name),
        );
    }

    Ok(())
}

/// Helper-system ally match. Both sides empty/None = ally (Circle-stock
/// fallback). One side tagged and the other empty = NOT ally. Both tagged =
/// ally iff case-insensitive equal.
fn factions_match(a: &Option<String>, b: &Option<String>) -> bool {
    let a_empty = a.as_deref().map(str::is_empty).unwrap_or(true);
    let b_empty = b.as_deref().map(str::is_empty).unwrap_or(true);
    match (a_empty, b_empty) {
        (true, true) => true,
        (true, false) | (false, true) => false,
        (false, false) => a.as_deref().unwrap().eq_ignore_ascii_case(b.as_deref().unwrap()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ironmud::CharacterPosition;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    #[test]
    fn factions_both_none_is_ally() {
        assert!(factions_match(&None, &None));
    }

    #[test]
    fn factions_both_empty_string_is_ally() {
        assert!(factions_match(&Some(String::new()), &Some(String::new())));
        assert!(factions_match(&Some(String::new()), &None));
    }

    #[test]
    fn factions_one_tagged_one_empty_is_not_ally() {
        assert!(!factions_match(&Some("guard".into()), &None));
        assert!(!factions_match(&None, &Some("guard".into())));
        assert!(!factions_match(&Some("guard".into()), &Some(String::new())));
    }

    #[test]
    fn factions_matching_tags_are_ally_case_insensitive() {
        assert!(factions_match(&Some("Goblin_Clan".into()), &Some("goblin_clan".into())));
    }

    #[test]
    fn factions_mismatched_tags_are_not_ally() {
        assert!(!factions_match(&Some("guard".into()), &Some("goblin".into())));
    }

    fn empty_connections() -> SharedConnections {
        Arc::new(Mutex::new(HashMap::new()))
    }

    fn mk_char(name: &str, room: Uuid, victim_id: Uuid) -> CharacterData {
        let mut char: CharacterData = serde_json::from_value(serde_json::json!({
            "name": name,
            "password_hash": "",
            "current_room_id": room,
        }))
        .expect("build character");
        char.position = CharacterPosition::Standing;
        char.combat.in_combat = true;
        char.combat.targets.push(CombatTarget {
            target_type: CombatTargetType::Mobile,
            target_id: victim_id,
        });
        char
    }

    fn mk_mobile(
        db: &db::Db,
        name: &str,
        room: Uuid,
        helper: bool,
        faction: Option<&str>,
    ) -> MobileData {
        let mut m = MobileData::new(name.to_string());
        m.is_prototype = false;
        m.current_room_id = Some(room);
        m.flags.helper = helper;
        m.faction = faction.map(|s| s.to_string());
        db.save_mobile_data(m.clone()).expect("save mobile");
        m
    }

    fn fresh_db(label: &str) -> (db::Db, String) {
        let path = format!("/tmp/test_helper_{}_{}.db", label, std::process::id());
        let _ = std::fs::remove_dir_all(&path);
        let db = db::Db::open(&path).expect("open db");
        (db, path)
    }

    fn run_with_db(label: &str, body: impl FnOnce(&db::Db)) {
        let (db, path) = fresh_db(label);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(&db)));
        let _ = std::fs::remove_dir_all(&path);
        if let Err(e) = outcome {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn helper_joins_when_pc_attacks_faction_ally() {
        run_with_db("faction_ally", |db| {
            let room = Uuid::new_v4();
            let victim = mk_mobile(db, "goblin warrior", room, true, Some("goblin_clan"));
            let helper = mk_mobile(db, "goblin shaman", room, true, Some("goblin_clan"));

            let attacker = mk_char("hero", room, victim.id);
            process_helper_joins(db, &empty_connections(), &attacker, &room).expect("scan ok");

            let h = db.get_mobile_data(&helper.id).expect("load helper").expect("exists");
            assert!(h.combat.in_combat, "helper should have entered combat");
            assert!(
                h.combat
                    .targets
                    .iter()
                    .any(|t| t.target_type == CombatTargetType::Player),
                "helper should target the player"
            );
        });
    }

    #[test]
    fn helper_joins_when_both_factions_empty_circle_fallback() {
        run_with_db("both_empty", |db| {
            let room = Uuid::new_v4();
            let victim = mk_mobile(db, "stray dog", room, true, None);
            let helper = mk_mobile(db, "stray cat", room, true, None);

            let attacker = mk_char("hero", room, victim.id);
            process_helper_joins(db, &empty_connections(), &attacker, &room).expect("scan ok");

            let h = db.get_mobile_data(&helper.id).expect("load").expect("exists");
            assert!(h.combat.in_combat, "Circle-stock fallback should engage helper");
        });
    }

    #[test]
    fn helper_skips_when_factions_differ() {
        run_with_db("factions_differ", |db| {
            let room = Uuid::new_v4();
            let victim = mk_mobile(db, "goblin warrior", room, true, Some("goblin_clan"));
            let bystander = mk_mobile(db, "town guard", room, true, Some("town_guard"));

            let attacker = mk_char("hero", room, victim.id);
            process_helper_joins(db, &empty_connections(), &attacker, &room).expect("scan ok");

            let b = db.get_mobile_data(&bystander.id).expect("load").expect("exists");
            assert!(!b.combat.in_combat, "guard must not defend a goblin");
        });
    }

    #[test]
    fn helper_skips_when_one_faction_empty() {
        run_with_db("one_empty", |db| {
            let room = Uuid::new_v4();
            let victim = mk_mobile(db, "wandering hermit", room, true, None);
            let bystander = mk_mobile(db, "town guard", room, true, Some("town_guard"));

            let attacker = mk_char("hero", room, victim.id);
            process_helper_joins(db, &empty_connections(), &attacker, &room).expect("scan ok");

            let b = db.get_mobile_data(&bystander.id).expect("load").expect("exists");
            assert!(
                !b.combat.in_combat,
                "tagged faction must opt out of generic pool — guard should not defend an unfactioned hermit"
            );
        });
    }

    #[test]
    fn helper_skips_when_already_in_combat_or_dead_or_no_attack() {
        run_with_db("skip_predicates", |db| {
            let room = Uuid::new_v4();
            let victim = mk_mobile(db, "goblin warrior", room, true, Some("goblin_clan"));

            // Already in combat — skipped.
            let mut already = mk_mobile(db, "goblin elder", room, true, Some("goblin_clan"));
            already.combat.in_combat = true;
            db.save_mobile_data(already.clone()).expect("save already-fighting");

            // Dead (hp <= 0) — skipped.
            let mut dead = mk_mobile(db, "goblin corpse", room, true, Some("goblin_clan"));
            dead.current_hp = 0;
            db.save_mobile_data(dead.clone()).expect("save dead");

            // no_attack — skipped.
            let mut peaceful = mk_mobile(db, "goblin oracle", room, true, Some("goblin_clan"));
            peaceful.flags.no_attack = true;
            db.save_mobile_data(peaceful.clone()).expect("save peaceful");

            // Helper without flag — skipped.
            let no_flag = mk_mobile(db, "goblin grunt", room, false, Some("goblin_clan"));

            let attacker = mk_char("hero", room, victim.id);
            process_helper_joins(db, &empty_connections(), &attacker, &room).expect("scan ok");

            let after_no_flag = db.get_mobile_data(&no_flag.id).unwrap().unwrap();
            assert!(
                !after_no_flag.combat.in_combat,
                "mob without helper flag must not join"
            );

            let after_dead = db.get_mobile_data(&dead.id).unwrap().unwrap();
            assert!(!after_dead.combat.in_combat, "dead helper must not join");

            let after_peaceful = db.get_mobile_data(&peaceful.id).unwrap().unwrap();
            assert!(!after_peaceful.combat.in_combat, "no_attack helper must not join");
        });
    }
}

/// Process a character attacking a mobile
fn process_character_attacks_mobile(
    db: &db::Db,
    connections: &SharedConnections,
    char: &mut CharacterData,
    target_id: &uuid::Uuid,
) -> Result<()> {
    use rand::Rng;

    let mut mobile = match db.get_mobile_data(target_id)? {
        Some(m) => m,
        None => {
            // Target no longer exists, remove from combat
            char.combat.targets.retain(|t| t.target_id != *target_id);
            char.combat.distances.remove(target_id);
            if char.combat.targets.is_empty() {
                char.combat.in_combat = false;
                char.combat.distances.clear();
            }
            // Save changes to database and sync to session
            db.save_character_data(char.clone())?;
            sync_character_to_session(connections, char);
            return Ok(());
        }
    };

    let room_id = char.current_room_id;

    // Verify mobile is in the same room - if not, exit combat with this target
    if mobile.current_room_id != Some(room_id) {
        char.combat.targets.retain(|t| t.target_id != *target_id);
        char.combat.distances.remove(target_id);
        if char.combat.targets.is_empty() {
            char.combat.in_combat = false;
            char.combat.distances.clear();
        }
        db.save_character_data(char.clone())?;
        sync_character_to_session(connections, char);
        send_message_to_character(connections, &char.name, "Your target is no longer here.");
        return Ok(());
    }
    let mut rng = rand::thread_rng();

    // Get weapon skill from equipped weapon
    let (mut weapon_skill, mut dice_count, mut dice_sides, mut damage_bonus, weapon_damage_type) =
        get_character_weapon_info(db, char);

    // Check arm/jaw wound restrictions for melee attacks
    let is_bite_attack = weapon_damage_type == DamageType::Bite;

    // Jaw disabled blocks bite attacks
    if is_bite_attack {
        let jaw_disabled = char
            .wounds
            .iter()
            .any(|w| w.body_part == BodyPart::Jaw && w.level == WoundLevel::Disabled);
        if jaw_disabled {
            send_message_to_character(connections, &char.name, "Your shattered jaw prevents you from biting!");
            return Ok(());
        }
    }

    // Both arms disabled + non-bite = can't attack
    if !is_bite_attack {
        let both_arms_disabled = char.wounds.iter().any(|w| {
            matches!(w.body_part, BodyPart::RightArm | BodyPart::RightHand) && w.level == WoundLevel::Disabled
        }) && char
            .wounds
            .iter()
            .any(|w| matches!(w.body_part, BodyPart::LeftArm | BodyPart::LeftHand) && w.level == WoundLevel::Disabled);
        if both_arms_disabled {
            send_message_to_character(
                connections,
                &char.name,
                "Both your arms are disabled! You cannot attack!",
            );
            return Ok(());
        }
    }

    // Check distance - melee weapons require closing the gap
    let current_distance = char
        .combat
        .distances
        .get(target_id)
        .copied()
        .unwrap_or(CombatDistance::Melee);

    let weapon_prefers_melee = WeaponSkill::from_str(&weapon_skill)
        .map(|ws| ws.prefers_melee())
        .unwrap_or(true);

    // Track ammo bonus for ranged weapons
    let mut ammo_bonus: i32 = 0;
    let mut is_ranged_attack = false;
    // Multi-shot support for burst/auto fire modes
    let mut shots_to_fire: i32 = 1;
    let mut accuracy_penalty: i32 = 0;
    let mut ranged_miss_verb = "fires at";
    let mut weapon_ranged_type = String::new();

    if !weapon_prefers_melee {
        // Ranged weapon handling
        if current_distance == CombatDistance::Melee {
            // Ranged weapon at melee distance - revert to unarmed
            weapon_skill = "unarmed".to_string();
            dice_count = 1;
            dice_sides = 2;
            damage_bonus = 0;
            send_message_to_character(connections, &char.name, "You resort to fighting with your fists!");
        } else {
            // Ranged weapon at range - check ammo
            is_ranged_attack = true;
            let ranged_type = get_character_weapon_ranged_type(db, char);
            weapon_ranged_type = ranged_type.clone().unwrap_or_default();

            match ranged_type.as_deref() {
                Some("crossbow") | Some("firearm") => {
                    // Magazine-based weapon: consume from loaded_ammo
                    let weapon_id = get_character_wielded_weapon_id(db, char);
                    if let Some(wid) = weapon_id {
                        if let Ok(Some(weapon)) = db.get_item_data(&wid) {
                            if weapon.loaded_ammo <= 0 {
                                // Empty magazine
                                if char.combat.ammo_depleted == 0 {
                                    char.combat.ammo_depleted = 1;
                                    db.save_character_data(char.clone())?;
                                    sync_character_to_session(connections, char);
                                    send_message_to_character(
                                        connections,
                                        &char.name,
                                        "Your weapon is empty! Use `reload` to load ammunition.",
                                    );
                                    return Ok(());
                                }
                                if char.combat.ammo_depleted == 1 {
                                    char.combat.ammo_depleted = 2;
                                    send_message_to_character(
                                        connections,
                                        &char.name,
                                        "Weapon empty, you resort to fighting with your fists!",
                                    );
                                }
                                weapon_skill = "unarmed".to_string();
                                dice_count = 1;
                                dice_sides = 2;
                                damage_bonus = 0;
                                is_ranged_attack = false;
                            } else {
                                // Determine shots from fire mode
                                let loaded = weapon.loaded_ammo;
                                match weapon.fire_mode.as_str() {
                                    "burst" => {
                                        shots_to_fire = loaded.min(3);
                                        accuracy_penalty = -1;
                                    }
                                    "auto" => {
                                        shots_to_fire = loaded;
                                        accuracy_penalty = -3;
                                    }
                                    _ => {
                                        // "single" or default
                                        shots_to_fire = 1;
                                    }
                                }
                                ammo_bonus = weapon.loaded_ammo_bonus;
                                // Consume loaded ammo
                                consume_loaded_ammo(db, &wid, shots_to_fire);
                                // Clear ammo_depleted if set
                                if char.combat.ammo_depleted > 0 {
                                    char.combat.ammo_depleted = 0;
                                }
                                // Set miss verb based on ranged_type
                                if weapon.ranged_type.as_deref() == Some("crossbow") {
                                    ranged_miss_verb = "fires a bolt at";
                                } else {
                                    ranged_miss_verb = "fires at";
                                }
                            }
                        }
                    }
                }
                _ => {
                    // Bow or unset: existing ready-slot ammo consumption (unchanged)
                    ranged_miss_verb = "fires at";
                    let caliber = get_character_weapon_caliber(db, char);
                    if let Some(ref cal) = caliber {
                        match find_character_ammo(db, &char.name, cal) {
                            AmmoSearchResult::Ready(item_id, bonus) => {
                                ammo_bonus = bonus;
                                consume_ammo_from_item(db, &item_id);
                                if char.combat.ammo_depleted > 0 {
                                    char.combat.ammo_depleted = 0;
                                }
                            }
                            AmmoSearchResult::Inventory(_item_id, _bonus) => {
                                send_message_to_character(
                                    connections,
                                    &char.name,
                                    "You fumble trying to load ammunition from your pack!",
                                );
                                broadcast_to_room_except_awake(
                                    connections,
                                    &room_id,
                                    &format!("{} fumbles with ammunition.", char.name),
                                    &char.name,
                                );
                                return Ok(());
                            }
                            AmmoSearchResult::None => {
                                if char.combat.ammo_depleted == 0 {
                                    char.combat.ammo_depleted = 1;
                                    db.save_character_data(char.clone())?;
                                    sync_character_to_session(connections, char);
                                    send_message_to_character(connections, &char.name, "You're out of ammunition!");
                                    return Ok(());
                                }
                                if char.combat.ammo_depleted == 1 {
                                    char.combat.ammo_depleted = 2;
                                    send_message_to_character(
                                        connections,
                                        &char.name,
                                        "Out of ammunition, you resort to fighting with your fists!",
                                    );
                                }
                                weapon_skill = "unarmed".to_string();
                                dice_count = 1;
                                dice_sides = 2;
                                damage_bonus = 0;
                                is_ranged_attack = false;
                            }
                        }
                    }
                    // No caliber = backward compat, attack without ammo
                }
            }
        }
    } else if weapon_prefers_melee && current_distance != CombatDistance::Melee {
        // Melee weapon - advance toward target
        if let Some(closer) = current_distance.closer() {
            char.combat.distances.insert(*target_id, closer);
            let step_msg = match (current_distance, closer) {
                (CombatDistance::Ranged, CombatDistance::Pole) => "close in",
                (CombatDistance::Pole, CombatDistance::Melee) => "move to melee range",
                _ => "advance",
            };
            send_message_to_character(
                connections,
                &char.name,
                &format!("You {} toward {}.", step_msg, mobile.name),
            );
            broadcast_to_room_except_awake(
                connections,
                &room_id,
                &format!("{} closes in on {}.", char.name, mobile.name),
                &char.name,
            );

            // If still not at melee range, skip attack this round
            if closer != CombatDistance::Melee {
                db.save_character_data(char.clone())?;
                sync_character_to_session(connections, char);
                return Ok(());
            }
        }
    }
    let skill = get_skill_level_for_character(char, &weapon_skill);

    // Calculate base hit chance: 50 + skill*5 + attacker_dex - target_dex - target_ac
    let attacker_dex_mod = (char.stat_dex as i32 - 10) / 2;
    let target_dex_mod = (mobile.stat_dex as i32 - 10) / 2;
    let target_ac = mobile.armor_class;

    let mut base_hit_chance = (50 + skill * 5 + attacker_dex_mod - target_dex_mod - target_ac).clamp(5, 95);

    // Arm wound hit penalty (melee attacks only, not bite)
    if !is_bite_attack && !is_ranged_attack {
        let arm_penalty = char
            .wounds
            .iter()
            .filter(|w| matches!(w.body_part, BodyPart::RightArm | BodyPart::RightHand))
            .map(|w| w.level.penalty())
            .max()
            .unwrap_or(0);
        if arm_penalty > 0 {
            base_hit_chance = (base_hit_chance - arm_penalty).clamp(5, 95);
        }
    }

    // Broadcast gunshot noise to adjacent rooms for loud ranged weapons
    if is_ranged_attack {
        if let Some(wid) = get_character_wielded_weapon_id(db, char) {
            let noise = get_effective_weapon_noise(db, &wid);
            if noise == "loud" {
                broadcast_gunshot_noise(db, connections, &room_id);
            }
        }
    }

    // Head wound daze chance (concussion)
    {
        let head_level = char
            .wounds
            .iter()
            .filter(|w| w.body_part == BodyPart::Head)
            .map(|w| &w.level)
            .max();
        if let Some(level) = head_level {
            let daze_chance = match level {
                WoundLevel::Severe => 10,
                WoundLevel::Critical => 20,
                WoundLevel::Disabled => 35,
                _ => 0,
            };
            if daze_chance > 0 && rng.gen_range(1..=100) <= daze_chance {
                send_message_to_character(
                    connections,
                    &char.name,
                    "Your vision swims \u{2014} you stumble, dazed from your head injury!",
                );
                return Ok(());
            }
        }
    }

    // Multi-shot loop for burst/auto fire modes
    for shot_num in 0..shots_to_fire {
        // Apply accuracy penalty for burst/auto (cumulative per shot)
        let hit_chance = (base_hit_chance + accuracy_penalty * shot_num).clamp(5, 95);
        let roll = rng.gen_range(1..=100);

        if roll > hit_chance {
            // Miss
            if is_ranged_attack {
                send_message_to_character(
                    connections,
                    &char.name,
                    &format!("You fire at {} but miss!", mobile.name),
                );
                broadcast_to_room_except_awake(
                    connections,
                    &room_id,
                    &format!("{} {} {} but misses!", char.name, ranged_miss_verb, mobile.name),
                    &char.name,
                );
            } else {
                send_message_to_character(
                    connections,
                    &char.name,
                    &format!("You swing at {} but miss!", mobile.name),
                );
                broadcast_to_room_except_awake(
                    connections,
                    &room_id,
                    &format!("{} swings at {} but misses!", char.name, mobile.name),
                    &char.name,
                );
            }
            // For single shot, return after miss
            if shots_to_fire == 1 {
                return Ok(());
            }
            continue;
        }

        // Hit - calculate base damage (includes ammo bonus for ranged)
        let mut damage = roll_dice(dice_count, dice_sides) + damage_bonus + ammo_bonus;

        // Apply underwater damage type modifier
        let (modified_damage, water_msg) =
            apply_underwater_modifier(db, &char.current_room_id, damage, weapon_damage_type);
        damage = modified_damage;
        if let Some(_msg) = water_msg {
            if damage == 0 {
                send_message_to_character(
                    connections,
                    &char.name,
                    "Your fire attack is extinguished by the water!",
                );
                continue;
            }
        }

        // Check for critical hit (5% + skill% + trait bonuses)
        let has_keen_edge = char.traits.iter().any(|t| t == "keen_edge");
        let has_dulled_reflexes = char.traits.iter().any(|t| t == "dulled_reflexes");
        let mut crit_bonus: i32 = 0;
        if has_keen_edge {
            crit_bonus += 5;
        }
        if has_dulled_reflexes {
            crit_bonus -= 5;
        }
        let crit_chance = (5 + skill + crit_bonus).max(1);
        let crit_roll = rng.gen_range(1..=100);
        let is_crit = crit_roll <= crit_chance;

        // Track critical effect for messaging
        let mut crit_effect = String::new();

        if is_crit {
            // Scale damage: 2x at skill >= 5, 1.5x otherwise
            damage = if skill >= 5 { damage * 2 } else { (damage * 3) / 2 };

            // Roll for secondary crit effect (1-4)
            let effect_roll = rng.gen_range(1..=4);
            crit_effect = match effect_roll {
                1 => {
                    let severity = std::cmp::min(2 + skill / 3, 5);
                    let body_part = roll_random_body_part(&mut rng);
                    add_mobile_wound_bleeding(db, &mobile.id, &body_part, severity)?;
                    "Bleeding".to_string()
                }
                2 => {
                    let stun_rounds = if skill >= 5 { 2 } else { 1 };
                    mobile.combat.stun_rounds_remaining += stun_rounds;
                    "Stun".to_string()
                }
                3 => {
                    let body_part = roll_random_body_part(&mut rng);
                    escalate_mobile_wound_to_severe(db, &mobile.id, &body_part)?;

                    // Drop mobile's weapon on arm/hand disable
                    if matches!(
                        body_part.as_str(),
                        "right arm" | "right hand" | "left arm" | "left hand"
                    ) {
                        if let Ok(equipped) = db.get_items_equipped_on_mobile(&mobile.id) {
                            for item in equipped {
                                if item.item_type == ItemType::Weapon {
                                    let item_name = item.name.clone();
                                    let mut dropped = item;
                                    dropped.location = ItemLocation::Room(room_id);
                                    dropped.wear_locations.clear();
                                    let _ = db.save_item_data(dropped);
                                    broadcast_to_room_awake(
                                        connections,
                                        &room_id,
                                        &format!("{}'s {} clatters to the ground!", mobile.name, item_name),
                                    );
                                    break;
                                }
                            }
                        }
                    }

                    body_part_disable_message(&body_part)
                }
                _ => String::new(),
            };

            if let Some(fresh_mobile) = db.get_mobile_data(&mobile.id)? {
                mobile = fresh_mobile;
            }
        }

        // Apply damage
        damage = ironmud::script::apply_damage_reduction(damage, &mobile.active_buffs);
        mobile.current_hp -= damage;
        db.save_mobile_data(mobile.clone())?;

        // Build message with crit text (yellow/bold)
        let crit_text = if is_crit {
            if crit_effect.is_empty() {
                " \x1b[1;33m[CRITICAL]\x1b[0m".to_string()
            } else {
                format!(" \x1b[1;33m[CRITICAL - {}!]\x1b[0m", crit_effect)
            }
        } else {
            String::new()
        };

        // Send messages
        if is_ranged_attack {
            let body_part = roll_random_body_part(&mut rng);
            let max_dmg = dice_count * dice_sides;
            let projectile = ranged_projectile_word(&weapon_ranged_type);
            let verb = ranged_hit_verb_contextual(&weapon_ranged_type, damage, max_dmg);
            send_message_to_character(
                connections,
                &char.name,
                &format!(
                    "Your {} {} {}'s {} for {} damage!{}",
                    projectile, verb, mobile.name, body_part, damage, crit_text
                ),
            );
            broadcast_to_room_except_awake(
                connections,
                &room_id,
                &format!(
                    "{}'s {} {} {}'s {} for {} damage!",
                    char.name, projectile, verb, mobile.name, body_part, damage
                ),
                &char.name,
            );
        } else {
            send_message_to_character(
                connections,
                &char.name,
                &format!("You hit {} for {} damage!{}", mobile.name, damage, crit_text),
            );
            broadcast_to_room_except_awake(
                connections,
                &room_id,
                &format!("{} hits {} for {} damage!", char.name, mobile.name, damage),
                &char.name,
            );
        }

        // Award XP for successful hit (10 XP to weapon skill)
        let leveled = add_skill_experience_to_character(char, &weapon_skill, 10);
        if leveled {
            send_message_to_character(
                connections,
                &char.name,
                &format!(
                    "\x1b[1;33mYour {} skill has improved!\x1b[0m",
                    weapon_skill.replace('_', " ")
                ),
            );
        }

        // Check if target died
        if mobile.current_hp <= 0 {
            process_mobile_death(db, connections, &mut mobile, &room_id)?;

            char.combat.targets.retain(|t| t.target_id != *target_id);
            if char.combat.targets.is_empty() {
                char.combat.in_combat = false;
            }
            // Stop firing if target is dead
            return Ok(());
        }
    }

    Ok(())
}

/// Process a character attacking another player (PvP)
fn process_character_attacks_player(
    _db: &db::Db,
    _connections: &SharedConnections,
    char: &mut CharacterData,
    target_id: &uuid::Uuid,
) -> Result<()> {
    // Find target player by name stored in target_id (it's actually a string converted)
    // Actually, target_id is a UUID. We need to find the character somehow.
    // For now, skip PvP in automatic combat rounds.
    // PvP attacks are handled manually through the attack command.

    // Remove this target from combat since we can't process it automatically
    char.combat.targets.retain(|t| t.target_id != *target_id);
    if char.combat.targets.is_empty() {
        char.combat.in_combat = false;
    }

    Ok(())
}

/// Attempt to have a mobile flee from combat
/// Returns Some(true) if successfully fled, Some(false) if failed, None if couldn't attempt
fn attempt_mobile_flee(db: &db::Db, connections: &SharedConnections, mobile: &mut MobileData) -> Option<bool> {
    use rand::Rng;
    use rand::seq::SliceRandom;

    let room_id = mobile.current_room_id?;
    let room = db.get_room_data(&room_id).ok()??;

    // Build valid exit list using existing wander logic
    let exits = get_valid_wander_exits(db, &room).ok()?;
    if exits.is_empty() {
        // No escape - broadcast failure (sleeping players don't see combat)
        broadcast_to_room_awake(
            connections,
            &room_id,
            &format!("{} looks around frantically for an escape!\n", mobile.name),
        );
        return Some(false);
    }

    // 50% success rate
    let mut rng = rand::thread_rng();
    if rng.gen_range(0..100) >= 50 {
        // Failed flee attempt (sleeping players don't see combat)
        broadcast_to_room_awake(
            connections,
            &room_id,
            &format!("{} tries to flee but stumbles!\n", mobile.name),
        );
        return Some(false);
    }

    // Success - pick random exit and move
    let (direction, target_room_id) = exits.choose(&mut rng)?.clone();

    // Broadcast departure (sleeping players don't see combat)
    broadcast_to_room_awake(
        connections,
        &room_id,
        &format!("{} flees {}!\n", mobile.name, direction),
    );

    // Fire on_flee triggers before moving the mobile
    {
        let mut flee_context = std::collections::HashMap::new();
        flee_context.insert("direction".to_string(), direction.clone());
        flee_context.insert("source_room".to_string(), room_id.to_string());
        flee_context.insert("mobile_name".to_string(), mobile.name.clone());
        ironmud::script::fire_mobile_triggers_from_rust(
            db,
            connections,
            &mobile.id.to_string(),
            "on_flee",
            "",
            &flee_context,
        );
    }

    // Remove this mobile from all player combat targets before clearing our targets
    // Collect character names to update (to avoid calling sync_character_to_session while holding lock)
    let mut chars_to_sync: Vec<CharacterData> = Vec::new();

    for target in &mobile.combat.targets {
        if target.target_type == CombatTargetType::Player {
            // Target ID for players is stored as the player name
            // Find the character by searching connections
            let player_names: Vec<String> = {
                if let Ok(conns) = connections.lock() {
                    conns
                        .iter()
                        .filter_map(|(_, session)| {
                            if let Some(ref char) = session.character {
                                // Check if this character is targeting the mobile
                                if char.combat.targets.iter().any(|t| t.target_id == mobile.id) {
                                    return Some(char.name.clone());
                                }
                            }
                            None
                        })
                        .collect()
                } else {
                    Vec::new()
                }
            };

            // Update combat state for each player (lock is released now)
            for player_name in player_names {
                if let Ok(Some(mut char_data)) = db.get_character_data(&player_name) {
                    char_data.combat.targets.retain(|t| t.target_id != mobile.id);
                    if char_data.combat.targets.is_empty() {
                        char_data.combat.in_combat = false;
                    }
                    let _ = db.save_character_data(char_data.clone());
                    chars_to_sync.push(char_data);
                }
            }
        }
    }

    // Sync all updated characters to their sessions (safe - no lock held)
    for char_data in chars_to_sync {
        sync_character_to_session(connections, &char_data);
    }

    // Move mobile
    mobile.current_room_id = Some(target_room_id);

    // Exit combat
    mobile.combat.in_combat = false;
    mobile.combat.targets.clear();

    // Save mobile
    let _ = db.save_mobile_data(mobile.clone());

    // Broadcast arrival (sleeping players don't see)
    let arrival_dir = get_opposite_direction_rust(&direction);
    broadcast_to_room_awake(
        connections,
        &target_room_id,
        &format!("{} arrives from the {}, fleeing!\n", mobile.name, arrival_dir),
    );

    Some(true)
}

/// Process a combat round for a single mobile
fn process_mobile_combat_round(
    db: &db::Db,
    connections: &SharedConnections,
    mobile_id: &uuid::Uuid,
    state: &SharedState,
) -> Result<()> {
    use rand::Rng;

    let mut mobile = match db.get_mobile_data(mobile_id)? {
        Some(m) => m,
        None => {
            debug!("Mobile {} not found in database", mobile_id);
            return Ok(());
        }
    };

    debug!(
        "Processing combat for mobile {} ({}): in_combat={}, targets={}",
        mobile.name,
        mobile_id,
        mobile.combat.in_combat,
        mobile.combat.targets.len()
    );

    // Skip if not actually in combat
    if !mobile.combat.in_combat || mobile.combat.targets.is_empty() {
        debug!("Mobile {} skipping - not in combat or no targets", mobile.name);
        return Ok(());
    }

    // Get room ID for messaging
    let room_id = match mobile.current_room_id {
        Some(rid) => rid,
        None => {
            debug!("Mobile {} has no room, skipping", mobile.name);
            return Ok(());
        }
    };

    debug!("Mobile {} is in room {}", mobile.name, room_id);

    // Handle stun
    if mobile.combat.stun_rounds_remaining > 0 {
        debug!(
            "Mobile {} is stunned ({} rounds remaining)",
            mobile.name, mobile.combat.stun_rounds_remaining
        );
        mobile.combat.stun_rounds_remaining -= 1;
        let mobile_name = mobile.name.clone();
        db.save_mobile_data(mobile)?;

        broadcast_to_room_awake(
            connections,
            &room_id,
            &format!("{} is stunned and cannot act!", mobile_name),
        );
        debug!("Mobile {} stun handling complete, returning", mobile_name);
        return Ok(());
    }

    // Apply ongoing effects (burn, frost, poison, acid)
    if !mobile.ongoing_effects.is_empty() {
        let per_effect_damage: Vec<i32> = mobile
            .ongoing_effects
            .iter()
            .map(|e| ironmud::script::apply_damage_reduction(e.damage_per_round, &mobile.active_buffs))
            .collect();
        let ongoing_damage: i32 = per_effect_damage.iter().sum();
        if ongoing_damage > 0 {
            mobile.current_hp -= ongoing_damage;

            for (effect, &dmg) in mobile.ongoing_effects.iter().zip(per_effect_damage.iter()) {
                let msg = match effect.effect_type.as_str() {
                    "fire" => format!("{} continues to burn! ({} damage)", mobile.name, dmg),
                    "cold" => format!("Frostbite spreads across {}! ({} damage)", mobile.name, dmg),
                    "poison" => format!("Poison courses through {}! ({} damage)", mobile.name, dmg),
                    "acid" => format!("Acid eats into {}! ({} damage)", mobile.name, dmg),
                    "lightning" => format!("Static surges through {}! ({} damage)", mobile.name, dmg),
                    _ => format!("{} suffers ongoing damage! ({} damage)", mobile.name, dmg),
                };
                broadcast_to_room_awake(connections, &room_id, &msg);
            }
        }

        // Decrement rounds and remove expired
        for effect in mobile.ongoing_effects.iter_mut() {
            effect.rounds_remaining -= 1;
        }
        mobile.ongoing_effects.retain(|e| e.rounds_remaining > 0);
        db.save_mobile_data(mobile.clone())?;

        if mobile.current_hp <= 0 {
            debug!(
                "Mobile {} died from ongoing effects, calling process_mobile_death",
                mobile.name
            );
            process_mobile_death(db, connections, &mut mobile, &room_id)?;
            return Ok(());
        }
    }

    // Poison combat miss: 25% chance to skip turn when poisoned
    if mobile.wounds.iter().any(|w| w.wound_type == WoundType::Poisoned) {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        if rng.gen_range(0..100) < 25 {
            broadcast_to_room_awake(
                connections,
                &room_id,
                &format!("{} doubles over, wracked by poison.", mobile.name),
            );
            db.save_mobile_data(mobile)?;
            return Ok(());
        }
    }

    // Check stamina for combat action
    const MOBILE_COMBAT_STAMINA_COST: i32 = 5;
    const MOBILE_MIN_STAMINA_RESTORE: i32 = 5;

    if mobile.current_stamina <= 0 {
        // Too exhausted - skip turn but restore minimum stamina
        debug!("Mobile {} exhausted, restoring stamina", mobile.name);
        mobile.current_stamina = MOBILE_MIN_STAMINA_RESTORE;
        debug!("Mobile {} saving exhausted state", mobile.name);
        db.save_mobile_data(mobile.clone())?;
        debug!("Mobile {} broadcasting exhaustion message", mobile.name);
        broadcast_to_room_awake(
            connections,
            &room_id,
            &format!("{} pauses to catch their breath.", mobile.name),
        );
        debug!("Mobile {} exhaustion handling complete", mobile.name);
        return Ok(());
    }

    // Consume stamina for attack
    mobile.current_stamina = (mobile.current_stamina - MOBILE_COMBAT_STAMINA_COST).max(0);

    // Check if mobile should attempt to flee (HP <= 25%)
    let mut rng = rand::thread_rng();
    if mobile.current_hp > 0 && mobile.max_hp > 0 {
        let hp_percent = (mobile.current_hp * 100) / mobile.max_hp;
        if hp_percent <= 25 {
            // Cowardly mobs always flee; normal mobs have 30% chance
            let should_flee = mobile.flags.cowardly || rng.gen_range(0..100) < 30;
            if should_flee {
                if let Some(fled) = attempt_mobile_flee(db, connections, &mut mobile) {
                    if fled {
                        // Successfully fled - skip attack this round
                        return Ok(());
                    }
                    // Failed flee - continue with attack
                }
            }
        }
    }

    // Get primary target
    let target = match mobile.combat.targets.first() {
        Some(t) => t.clone(),
        None => {
            mobile.combat.in_combat = false;
            mobile.combat.targets.clear();
            db.save_mobile_data(mobile)?;
            return Ok(());
        }
    };

    // Mobile attacks player
    if target.target_type == CombatTargetType::Player {
        // Find the player character - target_id is character name stored as UUID
        // Actually we need to iterate connections to find the player
        debug!("Mobile {} finding player in room", mobile.name);
        let player_name = find_player_name_in_room(connections, &room_id);
        debug!("Mobile {} found player: {:?}", mobile.name, player_name);

        if let Some(player_name) = player_name {
            // Check if mob should auto-advance (melee-preferring mobs close distance)
            // For Stage 1, all mobs default to preferring melee
            // Future: Add MobileData.preferred_combat_style or check equipped weapon
            let mob_prefers_melee = true;

            // For player targets, use nil UUID (consistent with enter_mobile_combat)
            let player_target_id = uuid::Uuid::nil();
            let current_distance = mobile
                .combat
                .distances
                .get(&player_target_id)
                .copied()
                .unwrap_or(CombatDistance::Melee);

            // Melee mobs must close distance before attacking
            if mob_prefers_melee && current_distance != CombatDistance::Melee {
                // Advance one step closer
                if let Some(closer) = current_distance.closer() {
                    mobile.combat.distances.insert(player_target_id, closer);
                    let step_msg = match (current_distance, closer) {
                        (CombatDistance::Ranged, CombatDistance::Pole) => "closes in",
                        (CombatDistance::Pole, CombatDistance::Melee) => "moves to melee range",
                        _ => "advances",
                    };
                    broadcast_to_room_awake(
                        connections,
                        &room_id,
                        &format!("{} {} toward {}!", mobile.name, step_msg, player_name),
                    );
                    debug!(
                        "Mobile {} advanced from {:?} to {:?}",
                        mobile.name, current_distance, closer
                    );

                    // If still not at melee range, skip attack this round (spent action closing)
                    if closer != CombatDistance::Melee {
                        db.save_mobile_data(mobile.clone())?;
                        return Ok(());
                    }
                }
            }

            let mobile_name = mobile.name.clone();
            debug!("Mobile {} attacking player {}", mobile_name, player_name);
            process_mobile_attacks_player(db, connections, &mut mobile, &player_name, &room_id, state)?;
            debug!("Mobile {} attack complete, saving", mobile_name);
            db.save_mobile_data(mobile)?;
            debug!("Mobile {} save complete", mobile_name);
        } else {
            // Target not found, exit combat
            let mobile_name = mobile.name.clone();
            debug!("Mobile {} target not found, exiting combat", mobile_name);
            mobile.combat.in_combat = false;
            mobile.combat.targets.clear();
            db.save_mobile_data(mobile)?;
        }
    }

    debug!("Mobile combat round complete");
    Ok(())
}

/// Process a mobile attacking a player
fn process_mobile_attacks_player(
    db: &db::Db,
    connections: &SharedConnections,
    mobile: &mut MobileData,
    player_name: &str,
    room_id: &uuid::Uuid,
    state: &SharedState,
) -> Result<()> {
    use rand::Rng;

    let mut char = match db.get_character_data(player_name)? {
        Some(c) => c,
        None => return Ok(()),
    };

    // Verify player is still in the same room
    if char.current_room_id != *room_id {
        // Player left, exit combat
        mobile.combat.in_combat = false;
        mobile.combat.targets.clear();
        return Ok(());
    }

    // Check if player is sleeping - if so, wake them up and give automatic hit
    let was_sleeping = char.position == CharacterPosition::Sleeping;
    if was_sleeping {
        char.position = CharacterPosition::Standing;
        db.save_character_data(char.clone())?;
        sync_character_to_session(connections, &char);
        send_message_to_character(connections, player_name, "You are jolted awake by an attack!");
        broadcast_to_room_except_awake(
            connections,
            room_id,
            &format!("{} is jolted awake!", char.name),
            player_name,
        );
    }

    // Ensure player is in combat with this mobile (reactive combat)
    if !char.combat.in_combat || !char.combat.targets.iter().any(|t| t.target_id == mobile.id) {
        char.combat.in_combat = true;
        if !char.combat.targets.iter().any(|t| t.target_id == mobile.id) {
            char.combat.targets.push(CombatTarget {
                target_type: CombatTargetType::Mobile,
                target_id: mobile.id,
            });
        }
        db.save_character_data(char.clone())?;
        sync_character_to_session(connections, &char);
    }

    let mut rng = rand::thread_rng();

    // Get weapon info (needed for both miss and hit messages)
    let (count, sides, bonus, damage_type) = get_mobile_weapon_info(db, mobile);

    let is_bite_attack = damage_type == DamageType::Bite;

    // Jaw disabled blocks bite attacks for mobiles
    if is_bite_attack {
        let jaw_disabled = mobile
            .wounds
            .iter()
            .any(|w| w.body_part == BodyPart::Jaw && w.level == WoundLevel::Disabled);
        if jaw_disabled {
            return Ok(());
        }
    }

    // Both arms disabled + non-bite = mobile can't attack
    if !is_bite_attack {
        let both_arms_disabled = mobile.wounds.iter().any(|w| {
            matches!(w.body_part, BodyPart::RightArm | BodyPart::RightHand) && w.level == WoundLevel::Disabled
        }) && mobile
            .wounds
            .iter()
            .any(|w| matches!(w.body_part, BodyPart::LeftArm | BodyPart::LeftHand) && w.level == WoundLevel::Disabled);
        if both_arms_disabled {
            return Ok(());
        }
    }

    // Calculate hit chance (automatic hit if target was sleeping)
    let attacker_dex_mod = (mobile.stat_dex as i32 - 10) / 2;
    let target_dex_mod = (char.stat_dex as i32 - 10) / 2;
    // Calculate player AC from armor + ArmorClassBoost buffs
    let ac_buff_bonus: i32 = char
        .active_buffs
        .iter()
        .filter(|b| b.effect_type == EffectType::ArmorClassBoost)
        .map(|b| b.magnitude)
        .sum();
    let target_ac = ac_buff_bonus;
    let skill = mobile.hit_modifier; // Mobile skill level based on difficulty

    let mut hit_chance = (50 + skill * 5 + attacker_dex_mod - target_dex_mod - target_ac).clamp(5, 95);

    // Mobile arm wound hit penalty (non-bite attacks)
    if !is_bite_attack {
        let arm_penalty = mobile
            .wounds
            .iter()
            .filter(|w| matches!(w.body_part, BodyPart::RightArm | BodyPart::RightHand))
            .map(|w| w.level.penalty())
            .max()
            .unwrap_or(0);
        if arm_penalty > 0 {
            hit_chance = (hit_chance - arm_penalty).clamp(5, 95);
        }
    }

    let roll = rng.gen_range(1..=100);

    // Skip hit roll if target was sleeping (automatic hit)
    if !was_sleeping && roll > hit_chance {
        // Miss
        let miss_verb = get_miss_verb(damage_type);
        send_message_to_character(
            connections,
            player_name,
            &format!("{} {} you but misses!", mobile.name, miss_verb),
        );
        broadcast_to_room_except_awake(
            connections,
            room_id,
            &format!("{} {} {} but misses!", mobile.name, miss_verb, char.name),
            player_name,
        );
        return Ok(());
    }

    // Hit - calculate damage
    let mut damage = roll_dice(count, sides) + bonus;

    // Apply underwater damage type modifier
    let (modified_damage, _water_msg) = apply_underwater_modifier(db, room_id, damage, damage_type);
    damage = modified_damage;
    if damage == 0 {
        // Fire attacks extinguished by water
        broadcast_to_room_awake(
            connections,
            room_id,
            &format!("{}'s fire attack is extinguished by the water!", mobile.name),
        );
        return Ok(());
    }

    // Check for critical hit (5% base for mobiles, no skill bonus)
    let crit_chance = 5;
    let crit_roll = rng.gen_range(1..=100);
    let is_crit = crit_roll <= crit_chance;

    // Track critical effect for messaging
    let mut crit_effect = String::new();

    if is_crit {
        // Scale damage: 1.5x for mobiles (no skill)
        damage = (damage * 3) / 2;

        // Roll for secondary crit effect (1-4)
        let effect_roll = rng.gen_range(1..=4);
        crit_effect = match effect_roll {
            1 => {
                // Bleeding
                let severity = 2; // Base severity for mobiles
                let body_part = roll_random_body_part(&mut rng);
                add_character_wound_bleeding(db, player_name, &body_part, severity)?;
                "Bleeding".to_string()
            }
            2 => {
                // Stun: 1 round for mobiles
                char.combat.stun_rounds_remaining += 1;
                db.save_character_data(char.clone())?;
                "Stun".to_string()
            }
            3 => {
                // Limb disable
                let body_part = roll_random_body_part(&mut rng);
                escalate_character_wound_to_severe(db, player_name, &body_part)?;

                // Weapon drop on arm/hand disable
                match body_part.as_str() {
                    "right arm" | "right hand" => {
                        if let Some(wid) = get_character_wielded_weapon_id(db, &char) {
                            if let Ok(Some(weapon)) = db.get_item_data(&wid) {
                                let weapon_name = weapon.name.clone();
                                let mut dropped = weapon;
                                dropped.location = ItemLocation::Room(*room_id);
                                dropped.wear_locations.clear();
                                let _ = db.save_item_data(dropped);
                                send_message_to_character(
                                    connections,
                                    player_name,
                                    &format!("Your {} slips from your disabled hand!", weapon_name),
                                );
                                broadcast_to_room_except_awake(
                                    connections,
                                    room_id,
                                    &format!("{}'s {} clatters to the ground!", char.name, weapon_name),
                                    player_name,
                                );
                            }
                        }
                    }
                    "left arm" | "left hand" => {
                        // Drop offhand item
                        if let Ok(equipped) = db.get_equipped_items(player_name) {
                            for item in equipped {
                                if item.wear_locations.iter().any(|l| *l == WearLocation::OffHand) {
                                    let item_name = item.name.clone();
                                    let mut dropped = item;
                                    dropped.location = ItemLocation::Room(*room_id);
                                    dropped.wear_locations.clear();
                                    let _ = db.save_item_data(dropped);
                                    send_message_to_character(
                                        connections,
                                        player_name,
                                        &format!("Your {} slips from your disabled hand!", item_name),
                                    );
                                    broadcast_to_room_except_awake(
                                        connections,
                                        room_id,
                                        &format!("{}'s {} clatters to the ground!", char.name, item_name),
                                        player_name,
                                    );
                                    break;
                                }
                            }
                        }
                    }
                    _ => {}
                }

                body_part_disable_message(&body_part)
            }
            _ => String::new(), // Clean crit
        };

        // Reload character from DB to get any wounds that were added
        // (add_character_wound_bleeding and escalate_character_wound_to_severe save directly to DB)
        if let Some(fresh_char) = db.get_character_data(player_name)? {
            char = fresh_char;
        }
    }

    // Apply racial resistance modifier
    {
        let race_id = char.race.to_lowercase();
        let dmg_type_str = damage_type.to_display_string();
        let world = state.lock().unwrap();
        if let Some(race) = world.race_definitions.get(&race_id) {
            if let Some(&resist_pct) = race.resistances.get(dmg_type_str) {
                // Positive = resistance (reduces damage), negative = vulnerability (increases damage)
                damage = (damage * (100 - resist_pct)) / 100;
                if damage < 1 {
                    damage = 1;
                }
            }
        }
    }

    // Physical damage reduction traits (bludgeoning, slashing, piercing)
    let is_physical = matches!(
        damage_type,
        DamageType::Bludgeoning | DamageType::Slashing | DamageType::Piercing
    );
    if is_physical {
        let has_iron_hide = char.traits.iter().any(|t| t == "iron_hide");
        let has_glass_jaw = char.traits.iter().any(|t| t == "glass_jaw");
        let mut phys_mod: i32 = 0;
        if has_iron_hide {
            phys_mod += 10;
        } // 10% reduction
        if has_glass_jaw {
            phys_mod -= 15;
        } // 15% increase
        if phys_mod != 0 {
            damage = (damage * (100 - phys_mod) / 100).max(1);
        }
    }

    // Apply damage
    damage = ironmud::script::apply_damage_reduction(damage, &char.active_buffs);
    char.hp -= damage;

    // Apply on-hit DoT effects from mobile flags (poisonous, fiery, chilling, corrosive, shocking)
    ironmud::script::apply_mobile_on_hit_dots(mobile, &mut char.ongoing_effects, "body");

    db.save_character_data(char.clone())?;

    // Sync updated character to session so prompt shows correct HP
    sync_character_to_session(connections, &char);

    // Build message with crit text (yellow/bold)
    let crit_text = if is_crit {
        if crit_effect.is_empty() {
            " \x1b[1;33m[CRITICAL]\x1b[0m".to_string()
        } else {
            format!(" \x1b[1;33m[CRITICAL - {}!]\x1b[0m", crit_effect)
        }
    } else {
        String::new()
    };

    // Send messages (red for damage taken) - sleeping bystanders don't see combat
    let hit_verb = get_hit_verb(damage_type);
    send_message_to_character(
        connections,
        player_name,
        &format!(
            "\x1b[1;31m{} {} you for {} damage!{}\x1b[0m",
            mobile.name, hit_verb, damage, crit_text
        ),
    );
    broadcast_to_room_except_awake(
        connections,
        room_id,
        &format!("{} {} {} for {} damage!", mobile.name, hit_verb, char.name, damage),
        player_name,
    );

    // Check if player died or went unconscious
    if char.hp <= 0 {
        if char.is_unconscious {
            // Already unconscious and took damage - instant death!
            process_player_death(db, connections, &mut char, room_id)?;

            // Remove player from mobile's targets
            mobile
                .combat
                .targets
                .retain(|t| t.target_type != CombatTargetType::Player);
            if mobile.combat.targets.is_empty() {
                mobile.combat.in_combat = false;
            }
        } else {
            // First time reaching 0 HP - go unconscious
            char.is_unconscious = true;
            char.bleedout_rounds_remaining = 5; // 5 round bleedout timer
            let char_name_for_msg = char.name.clone();
            db.save_character_data(char.clone())?;
            sync_character_to_session(connections, &char);

            send_message_to_character(connections, player_name, "You collapse, unconscious!");
            broadcast_to_room_except_awake(
                connections,
                room_id,
                &format!("{} collapses, unconscious!", char_name_for_msg),
                player_name,
            );

            // If mobile is aggressive, they will continue attacking and kill the player
            if mobile.flags.aggressive {
                // Reload character and process instant death
                if let Ok(Some(mut char)) = db.get_character_data(player_name) {
                    process_player_death(db, connections, &mut char, room_id)?;

                    // Remove player from mobile's targets
                    mobile
                        .combat
                        .targets
                        .retain(|t| t.target_type != CombatTargetType::Player);
                    if mobile.combat.targets.is_empty() {
                        mobile.combat.in_combat = false;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Roll dice (e.g., 2d6)
pub fn roll_dice(count: i32, sides: i32) -> i32 {
    use rand::Rng;

    if count <= 0 || sides <= 0 {
        return 0;
    }

    let mut rng = rand::thread_rng();
    let mut total = 0;
    for _ in 0..count {
        total += rng.gen_range(1..=sides);
    }
    total
}

/// Parse damage dice string like "2d6" or "2d6+3" into (count, sides, bonus)
pub fn parse_damage_dice(dice_str: &str) -> (i32, i32, i32) {
    if dice_str.is_empty() {
        return (1, 4, 0); // Default to 1d4
    }

    // Parse formats: "2d6", "2d6+3", "2d6-1"
    let parts: Vec<&str> = dice_str.split('d').collect();
    if parts.len() != 2 {
        return (1, 4, 0);
    }

    let count: i32 = parts[0].parse().unwrap_or(1);

    // Check for bonus/penalty
    let sides_and_bonus = parts[1];
    if sides_and_bonus.contains('+') {
        let sp: Vec<&str> = sides_and_bonus.split('+').collect();
        let sides: i32 = sp[0].parse().unwrap_or(4);
        let bonus: i32 = sp.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        return (count, sides, bonus);
    } else if sides_and_bonus.contains('-') {
        let sp: Vec<&str> = sides_and_bonus.split('-').collect();
        let sides: i32 = sp[0].parse().unwrap_or(4);
        let penalty: i32 = sp.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        return (count, sides, -penalty);
    }

    let sides: i32 = sides_and_bonus.parse().unwrap_or(4);
    (count, sides, 0)
}

/// Get weapon info for a character (skill, dice_count, dice_sides, damage_bonus)
fn get_character_weapon_info(db: &db::Db, char: &CharacterData) -> (String, i32, i32, i32, DamageType) {
    // Default unarmed values
    let default = ("unarmed".to_string(), 1, 2, 0, DamageType::Bludgeoning);

    // Get equipped items from database
    let equipped_items = match db.get_equipped_items(&char.name) {
        Ok(items) => items,
        Err(_) => return default,
    };

    // Look through equipped items for a wielded weapon
    for item in &equipped_items {
        // Check if wielded
        for loc in &item.wear_locations {
            if *loc == WearLocation::Wielded {
                let skill = item
                    .weapon_skill
                    .as_ref()
                    .map(|s| s.to_skill_key().to_string())
                    .unwrap_or_else(|| "unarmed".to_string());
                return (
                    skill,
                    item.damage_dice_count,
                    item.damage_dice_sides,
                    0,
                    item.damage_type,
                );
            }
        }
    }

    default
}

/// Apply underwater damage type modifiers for combat in underwater rooms
fn apply_underwater_modifier(
    db: &db::Db,
    room_id: &uuid::Uuid,
    damage: i32,
    damage_type: DamageType,
) -> (i32, Option<&'static str>) {
    let room = match db.get_room_data(room_id) {
        Ok(Some(r)) if r.flags.underwater => r,
        _ => return (damage, None),
    };
    let _ = room; // used only for the flag check above
    match damage_type {
        DamageType::Slashing | DamageType::Bludgeoning => ((damage * 75) / 100, Some("underwater penalty")),
        DamageType::Piercing => ((damage * 115) / 100, Some("underwater bonus")),
        DamageType::Fire => (0, Some("extinguished by water")),
        DamageType::Cold => ((damage * 110) / 100, Some("underwater bonus")),
        _ => (damage, None),
    }
}

/// Get the caliber of a character's wielded weapon, if any
fn get_character_weapon_caliber(db: &db::Db, char: &CharacterData) -> Option<String> {
    let equipped_items = db.get_equipped_items(&char.name).unwrap_or_default();
    for item in &equipped_items {
        for loc in &item.wear_locations {
            if *loc == WearLocation::Wielded {
                return item.caliber.clone();
            }
        }
    }
    None
}

/// Result of searching for ammunition
enum AmmoSearchResult {
    /// Found in Ready slot: item_id, damage_bonus
    Ready(uuid::Uuid, i32),
    /// Found in inventory only (not readied): item_id, damage_bonus
    Inventory(uuid::Uuid, i32),
    /// No compatible ammo found
    None,
}

/// Search for compatible ammunition for a character
fn find_character_ammo(db: &db::Db, char_name: &str, caliber: &str) -> AmmoSearchResult {
    let caliber_lower = caliber.to_lowercase();

    // Check Ready slot first
    if let Ok(equipped) = db.get_equipped_items(char_name) {
        for item in &equipped {
            if item.wear_locations.iter().any(|loc| matches!(loc, WearLocation::Ready))
                && item.item_type == ItemType::Ammunition
                && item.caliber.as_ref().map(|c| c.to_lowercase()) == Some(caliber_lower.clone())
                && item.ammo_count > 0
            {
                return AmmoSearchResult::Ready(item.id, item.ammo_damage_bonus);
            }
        }
    }

    // Check inventory
    if let Ok(inventory) = db.get_items_in_inventory(char_name) {
        for item in &inventory {
            if item.item_type == ItemType::Ammunition
                && item.caliber.as_ref().map(|c| c.to_lowercase()) == Some(caliber_lower.clone())
                && item.ammo_count > 0
            {
                return AmmoSearchResult::Inventory(item.id, item.ammo_damage_bonus);
            }
        }
    }

    AmmoSearchResult::None
}

/// Consume one ammo from an item, deleting it when exhausted
fn consume_ammo_from_item(db: &db::Db, item_id: &uuid::Uuid) {
    if let Ok(Some(mut item)) = db.get_item_data(item_id) {
        item.ammo_count -= 1;
        if item.ammo_count <= 0 {
            let _ = db.delete_item(item_id);
        } else {
            let _ = db.save_item_data(item);
        }
    }
}

/// Get the ranged_type of a character's wielded weapon (e.g., "bow", "crossbow", "firearm")
fn get_character_weapon_ranged_type(db: &db::Db, char: &CharacterData) -> Option<String> {
    let equipped_items = db.get_equipped_items(&char.name).unwrap_or_default();
    for item in &equipped_items {
        for loc in &item.wear_locations {
            if *loc == WearLocation::Wielded {
                return item.ranged_type.clone();
            }
        }
    }
    None
}

/// Get the UUID of a character's wielded weapon
fn get_character_wielded_weapon_id(db: &db::Db, char: &CharacterData) -> Option<uuid::Uuid> {
    let equipped_items = db.get_equipped_items(&char.name).unwrap_or_default();
    for item in &equipped_items {
        for loc in &item.wear_locations {
            if *loc == WearLocation::Wielded {
                return Some(item.id);
            }
        }
    }
    None
}

/// Consume loaded ammo from a weapon's internal magazine
fn consume_loaded_ammo(db: &db::Db, weapon_id: &uuid::Uuid, count: i32) {
    if let Ok(Some(mut item)) = db.get_item_data(weapon_id) {
        item.loaded_ammo = (item.loaded_ammo - count).max(0);
        let _ = db.save_item_data(item);
    }
}

/// Get weapon damage info for a mobile (equipped weapon or fallback to damage_dice)
fn get_mobile_weapon_info(db: &db::Db, mobile: &MobileData) -> (i32, i32, i32, DamageType) {
    // Check equipped items for a weapon
    if let Ok(equipped) = db.get_items_equipped_on_mobile(&mobile.id) {
        for item in equipped {
            if item.item_type == ItemType::Weapon {
                return (item.damage_dice_count, item.damage_dice_sides, 0, item.damage_type);
            }
        }
    }
    // Fallback to mobile's base damage_dice and damage_type
    let (count, sides, bonus) = parse_damage_dice(&mobile.damage_dice);
    (count, sides, bonus, mobile.damage_type)
}

/// Get the miss verb for a damage type (third person, e.g. "snaps at")
fn get_miss_verb(damage_type: DamageType) -> &'static str {
    match damage_type {
        DamageType::Slashing => "slashes at",
        DamageType::Piercing => "thrusts at",
        DamageType::Bludgeoning => "swings at",
        DamageType::Fire => "hurls flames at",
        DamageType::Cold => "sends frost at",
        DamageType::Lightning => "sends lightning at",
        DamageType::Poison => "strikes at",
        DamageType::Acid => "flings acid at",
        DamageType::Bite => "snaps at",
        DamageType::Ballistic => "fires at",
        DamageType::Arcane => "hurls magic at",
    }
}

/// Get the hit verb for a damage type (third person, e.g. "bites")
fn get_hit_verb(damage_type: DamageType) -> &'static str {
    match damage_type {
        DamageType::Slashing => "slashes",
        DamageType::Piercing => "stabs",
        DamageType::Bludgeoning => "hits",
        DamageType::Fire => "burns",
        DamageType::Cold => "freezes",
        DamageType::Lightning => "shocks",
        DamageType::Poison => "poisons",
        DamageType::Acid => "corrodes",
        DamageType::Bite => "bites",
        DamageType::Ballistic => "shoots",
        DamageType::Arcane => "blasts",
    }
}

/// Get the projectile word for a ranged weapon type
fn ranged_projectile_word(ranged_type: &str) -> &'static str {
    match ranged_type {
        "bow" => "arrow",
        "crossbow" => "bolt",
        _ => "shot",
    }
}

/// Get a contextual hit verb based on ranged type and damage severity
fn ranged_hit_verb_contextual(ranged_type: &str, damage: i32, max_damage: i32) -> &'static str {
    // Determine severity: low (<=25%), medium (25-75%), high (>75%)
    let severity = if max_damage <= 0 {
        1 // medium
    } else if damage <= max_damage / 4 {
        0 // low
    } else if damage > (max_damage * 3) / 4 {
        2 // high
    } else {
        1 // medium
    };

    match severity {
        0 => {
            if ranged_type == "crossbow" {
                "nicks"
            } else {
                "grazes"
            }
        }
        1 => match ranged_type {
            "bow" => "lodges in",
            "crossbow" => "punches into",
            _ => "rips into",
        },
        _ => {
            if ranged_type == "bow" {
                "punches through"
            } else {
                "tears through"
            }
        }
    }
}

/// Get the effective noise level of a weapon, accounting for attachments
fn get_effective_weapon_noise(db: &db::Db, weapon_id: &uuid::Uuid) -> String {
    let item = match db.get_item_data(weapon_id) {
        Ok(Some(i)) => i,
        _ => return "normal".to_string(),
    };

    // Base noise level
    let base = if item.noise_level.is_empty() {
        match item.ranged_type.as_deref() {
            Some("bow") => "silent",
            Some("crossbow") => "quiet",
            Some("firearm") => "loud",
            _ => "normal",
        }
        .to_string()
    } else {
        item.noise_level.clone()
    };

    // Apply attachment noise reduction
    let mut reduction: i32 = 0;
    for content_id in &item.container_contents {
        if let Ok(Some(att)) = db.get_item_data(content_id) {
            if !att.attachment_slot.is_empty() {
                reduction += att.attachment_noise_reduction;
            }
        }
    }

    if reduction <= 0 {
        return base;
    }

    let levels = ["silent", "quiet", "normal", "loud"];
    let idx = levels.iter().position(|l| *l == base).unwrap_or(2) as i32;
    let new_idx = (idx - reduction).max(0) as usize;
    levels[new_idx].to_string()
}

/// Broadcast gunshot noise to rooms adjacent to the given room
fn broadcast_gunshot_noise(db: &db::Db, connections: &SharedConnections, room_id: &uuid::Uuid) {
    if let Ok(Some(room)) = db.get_room_data(room_id) {
        let directions: [(&str, Option<uuid::Uuid>); 6] = [
            ("north", room.exits.north),
            ("south", room.exits.south),
            ("east", room.exits.east),
            ("west", room.exits.west),
            ("up", room.exits.up),
            ("down", room.exits.down),
        ];
        for (dir, exit_opt) in &directions {
            if let Some(target_room_id) = exit_opt {
                let from_dir = get_opposite_direction_rust(dir);
                broadcast_to_room_awake(
                    connections,
                    target_room_id,
                    &format!("You hear gunfire from the {}!", from_dir),
                );
            }
        }
    }
}

/// Get skill level for a character
pub fn get_skill_level_for_character(char: &CharacterData, skill_name: &str) -> i32 {
    if let Some(skill) = char.skills.get(&skill_name.to_lowercase()) {
        return skill.level;
    }
    0
}

/// Add skill experience to a character, returns true if leveled up
pub fn add_skill_experience_to_character(char: &mut CharacterData, skill_name: &str, amount: i32) -> bool {
    // XP required per level - matches Rhai version in characters.rs
    fn xp_for_level(level: i32) -> i32 {
        match level {
            0 => 100,
            1 => 200,
            2 => 350,
            3 => 550,
            4 => 800,
            5 => 1100,
            6 => 1500,
            7 => 2000,
            8 => 2600,
            9 => 3300,
            _ => 0,
        }
    }

    let max_level = 10;
    let skill_key = skill_name.to_lowercase();

    if let Some(skill) = char.skills.get_mut(&skill_key) {
        if skill.level >= max_level {
            return false;
        }
        skill.experience += amount;

        // Check for level up (may level multiple times if XP is high)
        let mut leveled_up = false;
        loop {
            let xp_needed = xp_for_level(skill.level);
            if xp_needed == 0 || skill.experience < xp_needed || skill.level >= max_level {
                break;
            }
            skill.experience -= xp_needed;
            skill.level += 1;
            leveled_up = true;
            if skill.level >= max_level {
                skill.experience = 0;
                break;
            }
        }
        return leveled_up;
    }

    // Skill not found, create it
    char.skills.insert(
        skill_key,
        SkillProgress {
            level: 0,
            experience: amount,
        },
    );
    false
}

/// Roll a random body part for combat effects
fn body_part_disable_message(body_part: &str) -> String {
    match body_part {
        "head" => "Head Rattled".to_string(),
        "neck" => "Neck Wrenched".to_string(),
        "torso" => "Ribs Cracked".to_string(),
        "left eye" | "right eye" => {
            let side = if body_part.starts_with("left") { "Left" } else { "Right" };
            format!("{} Eye Blinded", side)
        }
        "left ear" | "right ear" => {
            let side = if body_part.starts_with("left") { "Left" } else { "Right" };
            format!("{} Ear Deafened", side)
        }
        "jaw" => "Jaw Shattered".to_string(),
        p if p.ends_with("arm") => {
            let side = if p.starts_with("left") { "Left" } else { "Right" };
            format!("{} Arm Disabled", side)
        }
        p if p.ends_with("leg") => {
            let side = if p.starts_with("left") { "Left" } else { "Right" };
            format!("{} Leg Disabled", side)
        }
        p if p.ends_with("hand") => {
            let side = if p.starts_with("left") { "Left" } else { "Right" };
            format!("{} Hand Disabled", side)
        }
        p if p.ends_with("foot") => {
            let side = if p.starts_with("left") { "Left" } else { "Right" };
            format!("{} Foot Disabled", side)
        }
        _ => format!("{} Disabled", body_part),
    }
}

fn roll_random_body_part<R: rand::Rng>(rng: &mut R) -> String {
    // Weights total 100: torso 35, arms 12x2, legs 12x2, head 3, hands 4x2,
    //   neck 3, eyes 1x2, ears 1x2, jaw 1
    let roll = rng.gen_range(1..=100);
    match roll {
        1..=35 => "torso",
        36..=47 => "left arm",
        48..=59 => "right arm",
        60..=71 => "left leg",
        72..=83 => "right leg",
        84..=86 => "head",
        87..=90 => "left hand",
        91..=94 => "right hand",
        95..=95 => "left eye",
        96..=96 => "right eye",
        97..=97 => "left ear",
        98..=98 => "right ear",
        99..=99 => "jaw",
        _ => "neck",
    }
    .to_string()
}

/// Add bleeding to a mobile's wound on a body part
fn add_mobile_wound_bleeding(db: &db::Db, mobile_id: &uuid::Uuid, body_part: &str, severity: i32) -> Result<()> {
    if let Ok(Some(mut mobile)) = db.get_mobile_data(mobile_id) {
        add_wound_bleeding(&mut mobile, body_part, severity);
        db.save_mobile_data(mobile)?;
    }
    Ok(())
}

/// Escalate a mobile's wound to Severe level (limb disable)
fn escalate_mobile_wound_to_severe(db: &db::Db, mobile_id: &uuid::Uuid, body_part: &str) -> Result<()> {
    if let Ok(Some(mut mobile)) = db.get_mobile_data(mobile_id) {
        escalate_wound_to_severe(&mut mobile, body_part);
        db.save_mobile_data(mobile)?;
    }
    Ok(())
}

/// Add bleeding to a character's wound on a body part
fn add_character_wound_bleeding(db: &db::Db, char_name: &str, body_part: &str, severity: i32) -> Result<()> {
    if let Ok(Some(mut char)) = db.get_character_data(char_name) {
        add_wound_bleeding(&mut char, body_part, severity);
        db.save_character_data(char)?;
    }
    Ok(())
}

/// Escalate a character's wound to Severe level (limb disable)
fn escalate_character_wound_to_severe(db: &db::Db, char_name: &str, body_part: &str) -> Result<()> {
    if let Ok(Some(mut char)) = db.get_character_data(char_name) {
        escalate_wound_to_severe(&mut char, body_part);
        db.save_character_data(char)?;
    }
    Ok(())
}

/// Process player death: create corpse, transfer items, respawn
pub fn process_player_death(
    db: &db::Db,
    connections: &SharedConnections,
    char: &mut CharacterData,
    room_id: &uuid::Uuid,
) -> Result<()> {
    let char_name = char.name.clone();

    // Send death messages
    send_message_to_character(connections, &char_name, "You have died!");
    broadcast_to_room_except(connections, room_id, &format!("{} has died!", char_name), &char_name);

    // Create corpse using builder
    let mut corpse = CorpseBuilder::for_player(&char_name, *room_id, char.gold as i64).build();
    let corpse_id = corpse.id;

    // Transfer inventory to corpse (source of truth is ItemLocation::Inventory)
    if let Ok(inventory_items) = db.get_items_in_inventory(&char_name) {
        for item in inventory_items {
            let item_id = item.id;
            let mut updated_item = item;
            updated_item.location = ItemLocation::Container(corpse_id);
            let _ = db.save_item_data(updated_item);
            corpse.container_contents.push(item_id);
        }
    }

    // Transfer equipment to corpse (source of truth is ItemLocation::Equipped)
    if let Ok(equipped_items) = db.get_equipped_items(&char_name) {
        for item in equipped_items {
            let item_id = item.id;
            let mut updated_item = item;
            updated_item.location = ItemLocation::Container(corpse_id);
            let _ = db.save_item_data(updated_item);
            corpse.container_contents.push(item_id);
        }
    }

    // Save corpse
    db.save_item_data(corpse)?;

    // Clear character's gold (inventory items already moved to corpse above)
    char.gold = 0;

    // Respawn character
    let spawn_room = char
        .spawn_room_id
        .unwrap_or_else(|| uuid::Uuid::parse_str(STARTING_ROOM_ID).unwrap());

    char.current_room_id = spawn_room;
    char.hp = char.max_hp / 4;
    if char.hp < 1 {
        char.hp = 1;
    }
    char.is_unconscious = false;
    char.bleedout_rounds_remaining = 0;
    char.wounds.clear();
    char.ongoing_effects.clear();
    char.combat.in_combat = false;
    char.combat.targets.clear();
    char.combat.stun_rounds_remaining = 0;
    char.combat.ammo_depleted = 0;

    // Clear environmental/illness conditions
    char.is_wet = false;
    char.wet_level = 0;
    char.cold_exposure = 0;
    char.heat_exposure = 0;
    char.illness_progress = 0;
    char.has_illness = false;
    char.has_hypothermia = false;
    char.has_frostbite.clear();
    char.has_heat_exhaustion = false;
    char.has_heat_stroke = false;
    char.food_sick = false;

    db.save_character_data(char.clone())?;
    sync_character_to_session(connections, &char);

    // Send respawn message
    send_message_to_character(connections, &char_name, "You awaken at your spawn point...");
    send_message_to_character(connections, &char_name, &format!("You have {} HP.", char.hp));

    Ok(())
}

/// Process mobile death: create corpse with items
pub fn process_mobile_death(
    db: &db::Db,
    connections: &SharedConnections,
    mobile: &mut MobileData,
    room_id: &uuid::Uuid,
) -> Result<()> {
    debug!("process_mobile_death: starting for {}", mobile.name);
    let mobile_name = mobile.name.clone();

    // Send death message (red) - sleeping bystanders don't see combat
    debug!("process_mobile_death: broadcasting death message");
    broadcast_to_room_awake(
        connections,
        room_id,
        &format!("\x1b[1;31m{} collapses to the ground, dead!\x1b[0m", mobile_name),
    );
    debug!("process_mobile_death: death message broadcast complete");

    // Create corpse using builder with random gold variance
    let gold = mobile_gold_with_variance(mobile.gold as i64);
    let corpse = CorpseBuilder::for_mobile(&mobile_name, *room_id, gold).build();

    // Save corpse initially
    debug!("process_mobile_death: saving corpse");
    let corpse_id = corpse.id;
    db.save_item_data(corpse)?;
    debug!("process_mobile_death: corpse saved");

    // Transfer mobile's inventory and equipment to corpse
    debug!("process_mobile_death: transferring mobile items to corpse");
    if let Ok(inventory_items) = db.get_items_in_mobile_inventory(&mobile.id) {
        for item in inventory_items {
            let item_id = item.id;
            let mut updated_item = item;
            updated_item.flags.death_only = false;
            updated_item.location = ItemLocation::Container(corpse_id);
            if let Ok(Some(mut corpse)) = db.get_item_data(&corpse_id) {
                corpse.container_contents.push(item_id);
                let _ = db.save_item_data(corpse);
            }
            let _ = db.save_item_data(updated_item);
        }
    }
    if let Ok(equipped_items) = db.get_items_equipped_on_mobile(&mobile.id) {
        for item in equipped_items {
            let item_id = item.id;
            let mut updated_item = item;
            updated_item.flags.death_only = false;
            updated_item.location = ItemLocation::Container(corpse_id);
            if let Ok(Some(mut corpse)) = db.get_item_data(&corpse_id) {
                corpse.container_contents.push(item_id);
                let _ = db.save_item_data(corpse);
            }
            let _ = db.save_item_data(updated_item);
        }
    }
    debug!("process_mobile_death: item transfer complete");

    // Arrow recovery: spawn recovered projectiles into corpse
    if !mobile.embedded_projectiles.is_empty() {
        process_arrow_recovery(db, mobile, &corpse_id);
    }

    // Clear mobile's combat state (not strictly necessary since we're deleting)
    mobile.combat.in_combat = false;
    mobile.combat.targets.clear();

    // Remove the dead mobile from the database.
    // Note: db.delete_mobile also releases any migrant residency claim.
    debug!("process_mobile_death: deleting mobile from database");
    db.delete_mobile(&mobile.id)?;
    debug!("process_mobile_death: mobile deleted, returning");

    Ok(())
}

/// Process arrow recovery from a dead mobile's embedded projectiles.
/// Spawns recovered projectiles into the corpse container.
/// - Bullets are excluded (not recoverable)
/// - Special ammo (with ammo_effect_type) is excluded (consumed on impact)
/// - 50% chance each projectile is recoverable
/// - 25% of recovered projectiles spawn as broken
fn process_arrow_recovery(db: &db::Db, mobile: &MobileData, corpse_id: &uuid::Uuid) {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bullet_calibers = ["9mm", "5.56mm", ".45", ".308", "12gauge"];

    for vnum in &mobile.embedded_projectiles {
        // Look up prototype to check caliber and special ammo
        let prototype = match db.get_item_by_vnum(vnum) {
            Ok(Some(p)) => p,
            _ => continue,
        };

        // Skip bullets
        if let Some(ref cal) = prototype.caliber {
            let cal_lower = cal.to_lowercase();
            if bullet_calibers.iter().any(|b| cal_lower == *b) {
                continue;
            }
        }

        // Skip special ammo (consumed on impact)
        if !prototype.ammo_effect_type.is_empty() {
            continue;
        }

        // 50% chance to be recoverable
        if rng.gen_range(0..100) >= 50 {
            continue;
        }

        // Spawn projectile from prototype
        let mut spawned = match db.spawn_item_from_prototype(vnum) {
            Ok(Some(item)) => item,
            _ => continue,
        };

        // 25% of recovered are broken
        if rng.gen_range(0..100) < 25 {
            spawned.flags.broken = true;
        }

        // Place into corpse container
        spawned.location = ItemLocation::Container(*corpse_id);
        spawned.ammo_count = 1;
        if db.save_item_data(spawned.clone()).is_ok() {
            if let Ok(Some(mut corpse)) = db.get_item_data(corpse_id) {
                corpse.container_contents.push(spawned.id);
                let _ = db.save_item_data(corpse);
            }
        }
    }
}

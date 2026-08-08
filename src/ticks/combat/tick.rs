//! Combat tick system for IronMUD
//!
//! Handles combat rounds, damage calculation, wounds, and death processing.

use anyhow::Result;
use tokio::time::{Duration, interval};
use tracing::{debug, error};

use ironmud::{
    ActiveBuff, BodyPart, CharacterData, CharacterPosition, CombatDistance, CombatTarget, CombatTargetType,
    CombatZoneType, DamageType, EffectType, ItemLocation, ItemType, MobileData, SharedConnections, SharedState,
    WeaponSkill, WearLocation, WoundLevel, WoundType, db,
};

use super::on_hit::{apply_on_hit_effects_to_character, apply_on_hit_effects_to_mobile};
use super::wounds::{add_wound_bleeding, escalate_wound_to_severe};
use ironmud::corpse::{CorpseBuilder, mobile_gold_with_variance};

use crate::ticks::broadcast::{
    broadcast_to_room_awake, broadcast_to_room_except, broadcast_to_room_except_awake,
    broadcast_to_room_except_awake_per_viewer, send_message_to_character, sync_character_to_session,
};
use crate::ticks::mobile::{
    filter_exits_by_stay_zone, find_player_name_in_room, get_opposite_direction_rust, get_valid_wander_exits,
};

/// Combat tick interval in seconds (5 second rounds)
pub const COMBAT_TICK_INTERVAL_SECS: u64 = 5;

/// Background task that processes combat rounds periodically (5 second rounds)
pub async fn run_combat_tick(db: db::Db, connections: SharedConnections, state: SharedState) {
    let mut ticker = interval(Duration::from_secs(COMBAT_TICK_INTERVAL_SECS));

    loop {
        ticker.tick().await;
        crate::ticks::heartbeat::beat("combat");

        if let Err(e) = process_combat_round(&db, &connections, &state) {
            error!("Combat tick error: {}", e);
        }
    }
}

/// Process a combat round for all combatants
fn process_combat_round(db: &db::Db, connections: &SharedConnections, state: &SharedState) -> Result<()> {
    use std::time::Instant;
    let round_start = Instant::now();

    // Raging players acquire targets before the round list is built so a
    // fresh engagement swings this round, not next.
    if let Err(e) = super::rage::process_rage_acquisitions(db, connections, state) {
        debug!("Rage acquisition error: {}", e);
    }

    // Fear auras roll terror against opponents before attacks so terror
    // applied here reshapes the victim's action this round.
    if let Err(e) = super::fear::process_fear_auras(db, connections) {
        debug!("Fear aura error: {}", e);
    }

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
        if let Err(e) = process_character_combat_round(db, connections, state, char_name) {
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
fn process_character_combat_round(
    db: &db::Db,
    connections: &SharedConnections,
    state: &SharedState,
    char_name: &str,
) -> Result<()> {
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
            process_player_death(db, connections, &mut char, &room_id, state)?;
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

    // Magical sleep — skip turn while the Sleep buff is active.
    if char
        .active_buffs
        .iter()
        .any(|b| b.effect_type == ironmud::EffectType::Sleep)
    {
        send_message_to_character(connections, char_name, "You sleep deeply and cannot act.");
        broadcast_to_room_except(
            connections,
            &room_id,
            &format!("{} sleeps peacefully.", char.name),
            char_name,
        );
        return Ok(());
    }

    // Terror reshapes the round: forced flee, frozen panic, or a shaky
    // swing. Frenzy holders never reach here (the fear chokepoint refuses
    // application while frenzying), and is_feared ignores Courage pairs.
    if ironmud::script::fear::is_feared(&char.active_buffs) {
        match super::fear::roll_player_fear_action(&mut rand::thread_rng()) {
            super::fear::FearAction::Flee => {
                send_message_to_character(connections, char_name, "\x1b[1;31mPanic seizes you — you bolt!\x1b[0m");
                // Inject the flee command so flee.rhai resolves stamina,
                // flee chance, and triggers exactly as a voluntary flee.
                if let Ok(conns) = connections.lock() {
                    for session in conns.values() {
                        if let Some(ref sc) = session.character {
                            if sc.name.eq_ignore_ascii_case(char_name) {
                                let _ = session
                                    .input_sender
                                    .try_send(ironmud::InputEvent::Line("flee".to_string()));
                                break;
                            }
                        }
                    }
                }
                return Ok(());
            }
            super::fear::FearAction::Freeze => {
                send_message_to_character(
                    connections,
                    char_name,
                    "\x1b[1;31mYou freeze, paralyzed by terror!\x1b[0m",
                );
                broadcast_to_room_except(
                    connections,
                    &room_id,
                    &format!("{} stands rooted in terror!", char.name),
                    char_name,
                );
                return Ok(());
            }
            super::fear::FearAction::Act => {
                send_message_to_character(connections, char_name, "You fight on through the panic, hands shaking.");
            }
        }
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
            ironmud::interrupt_writer_by_name(connections, state, char_name);

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
            // Synths run broken instead of collapsing (System Shutdown rule).
            if char.synth_state.is_some() {
                handle_synth_down(db, connections, &mut char, &room_id, state)?;
                return Ok(());
            }
            char.is_unconscious = true;
            char.bleedout_rounds_remaining = 5;
            db.save_character_data(char.clone())?;
            sync_character_to_session(connections, &char, state);

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
        sync_character_to_session(connections, &char, state);
        send_message_to_character(connections, char_name, "You finish reloading.");
        return Ok(());
    }

    // Check stamina for combat action
    const COMBAT_STAMINA_COST: i32 = 5;
    const MIN_STAMINA_RESTORE: i32 = 5;

    // Replicants are tireless: no exhaustion, no stamina cost.
    if char.replicant_state.is_none() {
        if char.stamina <= 0 {
            // Too exhausted - skip turn but restore minimum stamina
            char.stamina = MIN_STAMINA_RESTORE;
            db.save_character_data(char.clone())?;
            sync_character_to_session(connections, &char, state);
            send_message_to_character(
                connections,
                char_name,
                "You are too exhausted to attack! You catch your breath...",
            );
            return Ok(());
        }

        // Consume stamina for attack
        char.stamina = (char.stamina - COMBAT_STAMINA_COST).max(0);
    }

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
    let mut kill_info: Option<MobKill> = None;
    let mut skill_gain: Option<(String, ironmud::progress::XpOutcome)> = None;
    match target.target_type {
        CombatTargetType::Mobile => {
            process_character_attacks_mobile(
                db,
                connections,
                state,
                &mut char,
                &target.target_id,
                &mut kill_info,
                &mut skill_gain,
            )?;
        }
        CombatTargetType::Player => {
            process_character_attacks_player(db, connections, state, &mut char, &target, &mut skill_gain)?;
        }
    }

    // After the swing, scan for HELPER mobiles in the room that should join
    // combat to defend a factional ally the PC is attacking.
    if matches!(target.target_type, CombatTargetType::Mobile) {
        if let Err(e) = process_helper_joins(db, connections, &char, &room_id) {
            debug!("Helper join scan error for {}: {}", char_name, e);
        }
    }

    // Garou: a credited kill feeds the Rage. Runs before the save below so
    // the gain persists with the round's other writes.
    if kill_info.is_some() && char.werewolf_state.is_some() {
        apply_werewolf_kill_rage(connections, &mut char, &room_id);
    }

    db.save_character_data(char.clone())?;
    sync_character_to_session(connections, &char, state);

    // Kill-credit notifications run AFTER the char save+sync above so they are
    // the last writers. They mutate the character out-of-band (the achievement
    // unlock/counter and quest progress); running them before the save would
    // let the save clobber those writes back out of the DB and session (the
    // symptom: the first-kill achievement's banner fired but the award never
    // appeared under `achievements`/stats).
    //
    // Every consumer below runs once per *credited participant*, not once for
    // the killer. Before this, quests fanned out over `damaged_by` while the
    // kill counter, worship favor, morality and faction standing all took the
    // killing blow's name alone — so a party of four hunting town watch left
    // three of them with clean hands. `group::kill_credit` is the one place
    // that set is decided.
    //
    // Safe for the other participants for a different reason than the killer:
    // `process_combat_round` walks combatants sequentially and each round
    // re-loads its character from the DB at the top, so a participant's next
    // round picks these writes up rather than clobbering them.
    if let Some(kill) = kill_info {
        apply_kill_credit(db, connections, state, &kill, &char.name, &room_id);
    }
    // Same reasoning as the kill-credit block: a weapon skill reaching a new
    // level can unlock an achievement, which is an out-of-band write to the
    // killer's character. It has to run after the save above.
    if let Some((skill, outcome)) = skill_gain {
        ironmud::progress::notify_xp_achievements(db, connections, state, &char.name, &skill, &outcome);
    }
    Ok(())
}

/// Credit one mob kill to everyone who earned it, and tell each of them.
///
/// Split out of the combat round so it is directly testable — the alternative
/// is driving a whole round to observe a fan-out, which makes the test about
/// swing rolls rather than about who gets credit.
///
/// **Must be called after the caller has persisted its character.** Every
/// consumer here writes the character out-of-band, so an earlier wholesale save
/// would clobber them. This is the same rule the quest and achievement hooks
/// have always run under; the loop does not change it.
fn apply_kill_credit(
    db: &db::Db,
    connections: &SharedConnections,
    state: &SharedState,
    kill: &MobKill,
    killer_name: &str,
    room_id: &uuid::Uuid,
) {
    let credit = ironmud::group::kill_credit(connections, &kill.damaged_by, killer_name, room_id);
    let corpse_contents = ironmud::corpse::describe_corpse_contents(db, &kill.corpse_id);

    ironmud::quest::handle_mob_kill(db, connections, state, &credit.participants, &kill.killed_vnum);

    for name in &credit.participants {
        let kill_total =
            crate::ticks::achievements::notify_kill_with_state(db, connections, state, name, &kill.killed_vnum);
        ironmud::script::worship::handle_npc_kill_credit(db, connections, name, &kill.killed_vnum);

        // Morality. Killing something evil pushes the participant toward Good
        // and vice versa; a mob nobody assigned a moral weight to is inert.
        // This is what makes the slider move during play at all — before it,
        // only an admin or a DG script could touch it, which left the nine
        // tiers and the aggro_good / aggro_evil / aggro_neutral mob gating that
        // reads them effectively dead.
        ironmud::morality::apply_delta(db, connections, name, ironmud::morality::kill_delta(kill.alignment));

        // Faction standing. Killing a tagged mob costs standing with its
        // faction and buys standing with that faction's declared enemies, so
        // grinding one group is a choice about who else will deal with you.
        // Untagged mobs are inert here, exactly as morally-neutral ones are
        // above. Helping is not a loophole: assist in putting down a guard and
        // the watch holds it against you too.
        ironmud::reputation::apply_kill(db, connections, state, name, kill.faction.as_deref());

        // Kill resolution block. The room already saw the red "X collapses to
        // the ground, dead!" broadcast; this is each participant's own summary,
        // and it exists so the most-repeated action in the game finally reports
        // what it produced instead of requiring a `look in corpse` every time.
        //
        // Sits after the handlers above so the milestone can use the kill total
        // they just incremented. Weapon-skill XP is not repeated here — it
        // flushes from each player's own progress buffer at their next prompt.
        let slay = if *name == credit.killer {
            ironmud::combat_text::slay_line(&kill.mob_display)
        } else {
            ironmud::combat_text::party_slay_line(killer_name, &kill.mob_display)
        };
        send_message_to_character(connections, name, &slay);
        if let Some(ref contents) = corpse_contents {
            send_message_to_character(connections, name, &ironmud::combat_text::corpse_contents_line(contents));
        }
        if ironmud::corpse::is_kill_milestone(kill_total) {
            send_message_to_character(
                connections,
                name,
                &ironmud::combat_text::kill_milestone_line(&ironmud::corpse::ordinal(kill_total)),
            );
        }
    }
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
                    target_name: None,
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
        char.combat.targets.push(CombatTarget::mobile(victim_id));
        char
    }

    fn mk_mobile(db: &db::Db, name: &str, room: Uuid, helper: bool, faction: Option<&str>) -> MobileData {
        let mut m = MobileData::new(name.to_string());
        m.is_prototype = false;
        m.current_room_id = Some(room);
        m.flags.helper = helper;
        m.faction = faction.map(|s| s.to_string());
        db.save_mobile_data(m.clone()).expect("save mobile");
        m
    }

    /// Minimal `World` for the handful of tick paths that take `SharedState`.
    /// Nothing here reads a definition table; the lock is what is needed.
    fn dummy_state(db: &db::Db, conns: &SharedConnections) -> SharedState {
        ironmud::World::minimal_shared(db.clone(), conns.clone())
    }

    /// A landed PvP swing credits weapon-skill XP on the same terms as the PvE
    /// branch. This path awarded nothing at all until the Tier 0 review — PvP
    /// was the one way to swing a weapon all day and never advance the skill.
    ///
    /// The victim is asleep so the hit is automatic; the roll is otherwise
    /// random and the test would flake.
    #[test]
    fn a_landed_pvp_hit_credits_weapon_skill_xp() {
        run_with_db("pvp_xp", |db| {
            // No room row: `effective_combat_zone` falls back to Pve, and the
            // sustain check here only refuses Safe.
            let room = Uuid::new_v4();

            let mut victim = mk_char("victim", room, Uuid::nil());
            victim.combat.targets.clear();
            victim.combat.in_combat = false;
            victim.position = CharacterPosition::Sleeping;
            victim.hp = 500;
            victim.max_hp = 500;
            db.save_character_data(victim).expect("save victim");

            let mut attacker = mk_char("attacker", room, Uuid::nil());
            attacker.combat.targets.clear();
            attacker.combat.targets.push(CombatTarget::player("victim"));

            let conns = empty_connections();
            let state = dummy_state(db, &conns);
            let target = attacker.combat.targets[0].clone();
            let mut skill_gain = None;
            process_character_attacks_player(db, &conns, &state, &mut attacker, &target, &mut skill_gain)
                .expect("swing resolves");

            // Unarmed is the default with no weapon equipped.
            let banked: i32 = attacker.skills.values().map(|s| s.experience).sum();
            assert_eq!(banked, 10, "a landed PvP hit banks the same 10 XP as PvE");
            assert!(
                attacker.skills.contains_key("unarmed"),
                "credited against the weapon skill, got {:?}",
                attacker.skills.keys().collect::<Vec<_>>()
            );
        });
    }

    /// The kill snapshots the victim's moral weight, so the deferred credit
    /// block can move the killer's morality after the mobile row is gone.
    ///
    /// Read from the *instance*, not the prototype: mobiles clone at spawn, so
    /// editing a prototype must not retroactively change what killing an
    /// already-live copy of it means.
    #[test]
    fn a_kill_snapshots_the_victims_alignment_from_the_instance() {
        run_with_db("kill_alignment", |db| {
            let room = Uuid::new_v4();

            // Prototype and instance disagree. The instance is what counts.
            let mut proto = MobileData::new("ghoul".to_string());
            proto.is_prototype = true;
            proto.vnum = "test_ghoul".to_string();
            proto.alignment = 0;
            db.save_mobile_data(proto).expect("save prototype");

            let mut victim = mk_mobile(db, "ghoul", room, false, None);
            victim.vnum = "test_ghoul".to_string();
            victim.alignment = -150;
            victim.max_hp = 1;
            victim.current_hp = 1;
            // Trivially hittable so the roll cannot make this flake.
            victim.armor_class = -50;
            victim.stat_dex = 1;
            db.save_mobile_data(victim.clone()).expect("save victim");

            let mut attacker = mk_char("attacker", room, victim.id);
            let conns = empty_connections();
            let state = dummy_state(db, &conns);

            let mut kill = None;
            let mut skill_gain = None;
            for _ in 0..500 {
                process_character_attacks_mobile(
                    db,
                    &conns,
                    &state,
                    &mut attacker,
                    &victim.id,
                    &mut kill,
                    &mut skill_gain,
                )
                .expect("swing resolves");
                if kill.is_some() {
                    break;
                }
            }
            let kill = kill.expect("500 swings at a 1 HP mob must land one");
            assert_eq!(
                kill.alignment, -150,
                "snapshot the instance's alignment, not the prototype's"
            );
            assert_eq!(
                ironmud::morality::kill_delta(kill.alignment),
                3,
                "which is what the credit block feeds to morality"
            );
        });
    }

    /// Dying bumps the `deaths` counter, and the bump survives the wholesale
    /// respawn save that happens in the same function. Every death route —
    /// bleedout expiry, a hit while unconscious, a synth shutdown — funnels
    /// through `process_player_death`, so counting here counts all of them
    /// exactly once.
    #[test]
    fn dying_bumps_the_deaths_counter() {
        run_with_db("deaths", |db| {
            let room = Uuid::new_v4();
            let mut victim = mk_char("victim", room, Uuid::nil());
            victim.combat.targets.clear();
            victim.hp = 0;
            victim.max_hp = 100;
            victim.spawn_room_id = Some(room);
            db.save_character_data(victim.clone()).expect("save victim");

            let conns = empty_connections();
            let state = dummy_state(db, &conns);
            process_player_death(db, &conns, &mut victim, &room, &state).expect("death resolves");

            let count = |db: &db::Db| {
                db.get_character_data("victim")
                    .expect("read")
                    .expect("exists")
                    .achievement_counters
                    .get("deaths")
                    .copied()
                    .unwrap_or(0)
            };
            assert_eq!(count(db), 1, "the respawn save must not clobber the counter");

            // Reload before dying again. The counter bump writes out-of-band,
            // so the caller's in-memory copy is stale the moment it returns —
            // reusing it would save the pre-bump value back over the counter.
            // Every real caller either returns immediately or reloads, which
            // is what this mirrors.
            let mut victim = db.get_character_data("victim").expect("read").expect("exists");
            process_player_death(db, &conns, &mut victim, &room, &state).expect("second death");
            assert_eq!(count(db), 2, "deaths accumulate");
        });
    }

    /// Put `chars` online in one connections table so the party terms in
    /// `group::kill_credit` have sessions to read.
    fn connections_with(chars: &[CharacterData]) -> SharedConnections {
        let conns = empty_connections();
        for c in chars {
            let (tx_client, _rx) = tokio::sync::mpsc::unbounded_channel::<String>();
            let (tx_input, _rx_in) = tokio::sync::mpsc::channel::<ironmud::InputEvent>(1);
            let mut session = ironmud::PlayerSession::new_for_test(tx_client, tx_input);
            session.character = Some(c.clone());
            conns.lock().unwrap().insert(Uuid::new_v4(), session);
            // Leak the receivers: dropping them closes the channel and every
            // send after that fails, which would silence the messages under
            // test.
            std::mem::forget((_rx, _rx_in));
        }
        conns
    }

    fn a_kill(vnum: &str, alignment: i32, faction: Option<&str>, damaged_by: &[(&str, i32)]) -> MobKill {
        MobKill {
            killed_vnum: vnum.to_string(),
            damaged_by: damaged_by
                .iter()
                .map(|(n, d)| (n.to_string(), *d))
                .collect::<std::collections::HashMap<String, i32>>(),
            corpse_id: Uuid::nil(),
            mob_display: "a rust-scarred ghoul".to_string(),
            alignment,
            faction: faction.map(|s| s.to_string()),
        }
    }

    fn counter(db: &db::Db, name: &str, key: &str) -> u32 {
        db.get_character_data(name)
            .expect("read")
            .expect("exists")
            .achievement_counters
            .get(key)
            .copied()
            .unwrap_or(0)
    }

    /// The defect this slice exists for. A healer who lands no blow all fight
    /// contributes nothing to `damaged_by`, so before the shared recipient set
    /// they earned no kill counter, no morality shift and no faction standing —
    /// only quests, which were the one consumer that fanned out. Grouping and
    /// being present is what makes support playable.
    #[test]
    fn a_grouped_healer_who_dealt_no_damage_gets_the_whole_kill_credit() {
        run_with_db("party_credit", |db| {
            let room = Uuid::new_v4();

            let mut tank = mk_char("tank", room, Uuid::nil());
            tank.combat.targets.clear();
            let mut medic = mk_char("medic", room, Uuid::nil());
            medic.combat.targets.clear();
            medic.following = Some("tank".to_string());
            medic.is_grouped = true;
            db.save_character_data(tank.clone()).expect("save tank");
            db.save_character_data(medic.clone()).expect("save medic");

            let conns = connections_with(&[tank.clone(), medic.clone()]);
            let state = dummy_state(db, &conns);

            // An evil, faction-tagged mob, so morality and standing both move.
            let kill = a_kill("ghoul", -100, Some("undead"), &[("tank", 40)]);
            apply_kill_credit(db, &conns, &state, &kill, "tank", &room);

            for who in ["tank", "medic"] {
                assert_eq!(
                    counter(db, who, "kills.any"),
                    1,
                    "{who} should be credited with the kill"
                );
                assert_eq!(counter(db, who, "kills.ghoul"), 1, "{who} per-vnum tally");
                let ch = db.get_character_data(who).expect("read").expect("exists");
                assert!(
                    ch.morality > 0,
                    "{who} should have moved toward Good, got {}",
                    ch.morality
                );
                assert!(
                    ch.reputation.get("undead").copied().unwrap_or(0) < 0,
                    "{who} should have lost standing with the undead"
                );
            }
        });
    }

    /// Presence and grouping are both required. A grouped member off in another
    /// zone did not take part, and crediting them would make grouping a way to
    /// farm standing from a safe room.
    #[test]
    fn a_grouped_member_elsewhere_earns_nothing() {
        run_with_db("absent_member", |db| {
            let room = Uuid::new_v4();
            let elsewhere = Uuid::new_v4();

            let mut tank = mk_char("tank", room, Uuid::nil());
            tank.combat.targets.clear();
            let mut absent = mk_char("absent", elsewhere, Uuid::nil());
            absent.combat.targets.clear();
            absent.following = Some("tank".to_string());
            absent.is_grouped = true;
            db.save_character_data(tank.clone()).expect("save tank");
            db.save_character_data(absent.clone()).expect("save absent");

            let conns = connections_with(&[tank.clone(), absent.clone()]);
            let state = dummy_state(db, &conns);

            let kill = a_kill("ghoul", -100, Some("undead"), &[("tank", 40)]);
            apply_kill_credit(db, &conns, &state, &kill, "tank", &room);

            assert_eq!(counter(db, "tank", "kills.any"), 1);
            assert_eq!(counter(db, "absent", "kills.any"), 0, "not in the room, not credited");
            let ch = db.get_character_data("absent").expect("read").expect("exists");
            assert_eq!(ch.morality, 0, "and no morality drift either");
        });
    }

    /// The killer is in `damaged_by` as well as being the killing blow. Credit
    /// must not land twice.
    #[test]
    fn the_killer_is_credited_exactly_once() {
        run_with_db("no_double_credit", |db| {
            let room = Uuid::new_v4();
            let mut tank = mk_char("tank", room, Uuid::nil());
            tank.combat.targets.clear();
            db.save_character_data(tank.clone()).expect("save tank");

            let conns = connections_with(&[tank.clone()]);
            let state = dummy_state(db, &conns);

            let kill = a_kill("ghoul", -100, None, &[("tank", 40)]);
            apply_kill_credit(db, &conns, &state, &kill, "tank", &room);

            assert_eq!(counter(db, "tank", "kills.any"), 1, "one kill, one tally");
        });
    }

    fn run_with_db(_label: &str, body: impl FnOnce(&db::Db)) {
        let temp = tempfile::tempdir().expect("create temp dir");
        let db = db::Db::open(temp.path()).expect("open db");
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(&db)));
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
            assert!(!after_no_flag.combat.in_combat, "mob without helper flag must not join");

            let after_dead = db.get_mobile_data(&dead.id).unwrap().unwrap();
            assert!(!after_dead.combat.in_combat, "dead helper must not join");

            let after_peaceful = db.get_mobile_data(&peaceful.id).unwrap().unwrap();
            assert!(!after_peaceful.combat.in_combat, "no_attack helper must not join");
        });
    }
}

/// Process a character attacking a mobile
/// A mob kill detected during a swing, reported back to the caller so the
/// kill-credit notifications (achievements, quests) can run *after* the
/// combat round persists `char`. Running them mid-round would let the
/// caller's wholesale `char` save clobber the out-of-band achievement/quest
/// writes — that is how the first-kill achievement vanished even though its
/// unlock banner fired.
struct MobKill {
    killed_vnum: String,
    damaged_by: std::collections::HashMap<String, i32>,
    /// Corpse `process_mobile_death` created, so the deferred kill block can
    /// report what dropped. Read after the round-end save, not before.
    corpse_id: uuid::Uuid,
    /// The mob's display name, snapshotted before deletion.
    mob_display: String,
    /// The mob's moral weight, snapshotted before deletion. Read from the
    /// *instance*, not the prototype: mobiles clone at spawn, so a prototype
    /// edited after a mob went live must not retroactively change what killing
    /// that mob means.
    alignment: i32,
    /// The mob's faction tag, snapshotted from the instance for the same
    /// reason as `alignment`. `None` for the unaffiliated, which is most of
    /// the world's wildlife and keeps it out of the reputation system.
    faction: Option<String>,
}

fn process_character_attacks_mobile(
    db: &db::Db,
    connections: &SharedConnections,
    state: &SharedState,
    char: &mut CharacterData,
    target_id: &uuid::Uuid,
    kill_out: &mut Option<MobKill>,
    skill_gain_out: &mut Option<(String, ironmud::progress::XpOutcome)>,
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
            sync_character_to_session(connections, char, state);
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
        sync_character_to_session(connections, char, state);
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
                                    sync_character_to_session(connections, char, state);
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
                                    sync_character_to_session(connections, char, state);
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
                sync_character_to_session(connections, char, state);
                return Ok(());
            }
        }
    }
    let skill = get_skill_level_for_character(char, &weapon_skill);

    // Calculate base hit chance: 50 + skill*5 + attacker_dex - target_dex - target_ac
    let (eq_hit_bonus, eq_dam_bonus, eq_dex_bonus) = sum_combat_buff_bonuses(&char.active_buffs);
    let (_, _, mob_eq_dex_bonus) = sum_combat_buff_bonuses(&mobile.active_buffs);
    let attacker_dex_mod = (char.stat_dex as i32 + eq_dex_bonus - 10) / 2;
    let target_dex_mod = (mobile.stat_dex as i32 + mob_eq_dex_bonus - 10) / 2;
    let target_ac = mobile.armor_class;

    let mut base_hit_chance =
        (50 + skill * 5 + attacker_dex_mod - target_dex_mod - target_ac + eq_hit_bonus).clamp(5, 95);

    // Invisible-target penalty: -30 to-hit when the attacker can't see
    // their target (mob has Invisibility buff and the PC lacks
    // DetectInvisible / admin god-mode).
    let target_invisible = mobile
        .active_buffs
        .iter()
        .any(|b| b.effect_type == EffectType::Invisibility);
    let attacker_detects = char.is_admin
        || char
            .active_buffs
            .iter()
            .any(|b| b.effect_type == EffectType::DetectInvisible);
    if target_invisible && !attacker_detects {
        base_hit_chance = (base_hit_chance - 30).clamp(5, 95);
    }

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

    // Blind buff slashes accuracy. Magnitude is the percentage points to subtract.
    if let Some(blind) = char.active_buffs.iter().find(|b| b.effect_type == EffectType::Blind) {
        base_hit_chance = (base_hit_chance - blind.magnitude).clamp(5, 95);
    }

    if let Some(curse) = char.active_buffs.iter().find(|b| b.effect_type == EffectType::Curse) {
        base_hit_chance = (base_hit_chance - curse.magnitude).clamp(5, 95);
    }

    // Bless buff — magnitude*3 % to hit chance.
    if let Some(bless) = char.active_buffs.iter().find(|b| b.effect_type == EffectType::Bless) {
        base_hit_chance = (base_hit_chance + bless.magnitude * 3).clamp(5, 95);
    }

    // Haste / Slow — affect hit chance. Stock D&D haste is "extra attack per
    // round"; without per-attack timing we approximate as +/- hit accuracy.
    if let Some(haste) = char.active_buffs.iter().find(|b| b.effect_type == EffectType::Haste) {
        base_hit_chance = (base_hit_chance + haste.magnitude * 2).clamp(5, 95);
    }
    if let Some(slow) = char.active_buffs.iter().find(|b| b.effect_type == EffectType::Slow) {
        base_hit_chance = (base_hit_chance - slow.magnitude * 2).clamp(5, 95);
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
                broadcast_to_room_except_awake_per_viewer(connections, &room_id, &char.name, |viewer| {
                    format!(
                        "{} {} {} but misses!",
                        char.name,
                        ranged_miss_verb,
                        mob_display_name_for(viewer, &mobile, true)
                    )
                });
            } else {
                send_message_to_character(
                    connections,
                    &char.name,
                    &format!("You swing at {} but miss!", mobile.name),
                );
                broadcast_to_room_except_awake_per_viewer(connections, &room_id, &char.name, |viewer| {
                    format!(
                        "{} swings at {} but misses!",
                        char.name,
                        mob_display_name_for(viewer, &mobile, true)
                    )
                });
            }
            // For single shot, return after miss
            if shots_to_fire == 1 {
                return Ok(());
            }
            continue;
        }

        // Hit - calculate base damage (includes ammo bonus for ranged + APPLY_DAMROLL bonuses)
        let mut damage = roll_dice(dice_count, dice_sides) + damage_bonus + ammo_bonus + eq_dam_bonus;

        // Bless adds (magnitude+1)/2 to damage. Default magnitude=1 → +1.
        if let Some(bless) = char.active_buffs.iter().find(|b| b.effect_type == EffectType::Bless) {
            damage += (bless.magnitude + 1) / 2;
        }

        // Frenzy adds magnitude to damage. Default magnitude=4 → +4.
        // Berserk strength is the central reason a kindred would *want*
        // to frenzy; flee.rhai also blocks fleeing while frenzying.
        if let Some(frenzy) = char.active_buffs.iter().find(|b| b.effect_type == EffectType::Frenzy) {
            damage += frenzy.magnitude;
        }

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

        // Track the critical effect for messaging. The effect key doubles as
        // the mechanic: `combat_text::crit_mechanic` classifies it, so the tag
        // the player reads and the effect the target suffers cannot disagree.
        let mut crit_effect = String::new();
        let mut crit_body_part = String::new();

        if is_crit {
            use ironmud::combat_text::CritMechanic;

            // Scale damage: 2x at skill >= 5, 1.5x otherwise
            damage = if skill >= 5 { damage * 2 } else { (damage * 3) / 2 };

            crit_effect = ironmud::combat_text::roll_crit_effect(weapon_damage_type, rng.gen_range(1..=4)).to_string();
            crit_body_part = roll_random_body_part(&mut rng);
            let mechanic = ironmud::combat_text::crit_mechanic(&crit_effect);
            let severity = std::cmp::min(2 + skill / 3, 5);
            let stun_rounds = if skill >= 5 { 2 } else { 1 };

            // Wound helpers save the mobile themselves, so they run before the
            // reload below and their writes survive it.
            match mechanic {
                CritMechanic::Bleed => {
                    add_mobile_wound_bleeding(db, &mobile.id, &crit_body_part, severity)?;
                }
                CritMechanic::StunBleed => {
                    add_mobile_wound_bleeding(db, &mobile.id, &crit_body_part, 2)?;
                }
                CritMechanic::Disable => {
                    escalate_mobile_wound_to_severe(db, &mobile.id, &crit_body_part)?;

                    // Drop mobile's weapon on arm/hand disable
                    if matches!(
                        crit_body_part.as_str(),
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
                }
                _ => {}
            }

            if let Some(fresh_mobile) = db.get_mobile_data(&mobile.id)? {
                mobile = fresh_mobile;
            }

            // In-memory mutations go after the reload — the previous version
            // applied the stun first and the reload silently discarded it, so
            // crit stuns never landed on mobiles.
            if matches!(
                mechanic,
                CritMechanic::Stun | CritMechanic::StunBleed | CritMechanic::OngoingStun
            ) {
                mobile.combat.stun_rounds_remaining += stun_rounds;
            }
            if matches!(mechanic, CritMechanic::Ongoing | CritMechanic::OngoingStun) {
                mobile.ongoing_effects.push(ironmud::OngoingEffect {
                    effect_type: ironmud::combat_text::crit_ongoing_element(&crit_effect).to_string(),
                    rounds_remaining: 3,
                    damage_per_round: severity,
                    body_part: crit_body_part.clone(),
                });
            }
        }

        // Holy weapons double damage to holy_vulnerable mobs (vampires,
        // demons, blessed-vulnerable undead). Doubling happens before DR so
        // sanctuary/stone-skin still scale proportionally on the holy hit.
        if mobile.flags.holy_vulnerable && weapon_damage_type == DamageType::Holy {
            damage *= 2;
        }

        // Sun-burning vampires die outright on any landed blow. The rescue
        // window from the sun tick is single-event: a fresh hit (combat or
        // another sun tick) ends them.
        let is_sun_burning = mobile
            .active_buffs
            .iter()
            .any(|b| b.effect_type == ironmud::EffectType::SunlightBurning);

        // Apply damage
        damage = ironmud::script::apply_damage_reduction(damage, &mobile.active_buffs);
        if is_sun_burning && damage > 0 {
            damage = mobile.current_hp; // ensure death
        }
        mobile.current_hp -= damage;
        // Slice 3c: party kill credit. Track every player that contributed
        // damage so handle_mob_kill can credit each one's quest progress.
        if damage > 0 {
            *mobile.combat.damaged_by.entry(char.name.to_lowercase()).or_insert(0) += damage;
        }
        // Magical sleep breaks on any damage taken.
        let was_sleeping = mobile
            .active_buffs
            .iter()
            .any(|b| b.effect_type == ironmud::EffectType::Sleep);
        if was_sleeping {
            mobile
                .active_buffs
                .retain(|b| b.effect_type != ironmud::EffectType::Sleep);
        }
        // Physical stance wake-on-damage: Sleeping → Sitting (mob fights
        // from sitting next round). Resting CircleMUD imports flow through
        // here too.
        let was_position_sleeping = mobile.position == ironmud::types::MobilePosition::Sleeping;
        if was_position_sleeping {
            mobile.position = ironmud::types::MobilePosition::Sitting;
        }
        ironmud::script::record_mob_memory(&mut mobile, &char.name);

        // Roll on-hit effects from wielded weapon (bleeding/elemental DOTs/status buffs)
        let on_hit_messages =
            apply_weapon_on_hit_to_mobile(db, char, &mut mobile, roll_random_body_part(&mut rng).as_str());

        db.save_mobile_data(mobile.clone())?;
        if was_sleeping {
            broadcast_to_room_awake(connections, &room_id, &format!("{} jolts awake!", mobile.name));
        }
        if was_position_sleeping && !was_sleeping {
            broadcast_to_room_awake(
                connections,
                &room_id,
                &format!("{} startles awake from a deep sleep!", mobile.name),
            );
        }

        // Build message with crit text, coloured by damage type.
        let crit_text =
            ironmud::combat_text::crit_text(is_crit, &crit_effect, &crit_body_part, false, weapon_damage_type);

        // Send messages
        if is_ranged_attack {
            let body_part = roll_random_body_part(&mut rng);
            let max_dmg = dice_count * dice_sides;
            let projectile = ironmud::combat_text::ranged_projectile_word(&weapon_ranged_type);
            let verb = ironmud::combat_text::ranged_hit_verb(
                &weapon_ranged_type,
                ironmud::combat_text::hit_severity(damage, max_dmg),
            );
            send_message_to_character(
                connections,
                &char.name,
                &format!(
                    "Your {} {} {}'s {} for {} damage!{}",
                    projectile, verb, mobile.name, body_part, damage, crit_text
                ),
            );
            broadcast_to_room_except_awake_per_viewer(connections, &room_id, &char.name, |viewer| {
                format!(
                    "{}'s {} {} {}'s {} for {} damage!",
                    char.name,
                    projectile,
                    verb,
                    mob_display_name_for(viewer, &mobile, true),
                    body_part,
                    damage
                )
            });
        } else {
            // Melee used to print a flat "You hit X for N damage!" regardless
            // of how hard the blow landed, while ranged (above) tiered its
            // verbs. Same severity banding now drives both.
            let max_dmg = dice_count * dice_sides;
            let severity = ironmud::combat_text::hit_severity(damage, max_dmg);
            let verb = ironmud::combat_text::melee_hit_verb(weapon_damage_type, severity);
            send_message_to_character(
                connections,
                &char.name,
                &format!(
                    "You {} {} for {} damage!{}",
                    verb.second, mobile.name, damage, crit_text
                ),
            );
            broadcast_to_room_except_awake_per_viewer(connections, &room_id, &char.name, |viewer| {
                format!(
                    "{} {} {} for {} damage!",
                    char.name,
                    verb.third,
                    mob_display_name_for(viewer, &mobile, true),
                    damage
                )
            });
        }

        // Broadcast any on-hit effect lines (bleeding/elemental/status)
        for line in &on_hit_messages {
            broadcast_to_room_awake(connections, &room_id, line);
        }

        // Award XP for successful hit (10 XP to weapon skill).
        //
        // `is_language = false` is passed explicitly rather than looked up:
        // weapon skills are never languages, and the lookup would need the
        // World lock, which this tick must not take while holding its own
        // state. The client-facing line is emitted inline, but the achievement
        // hook is deferred to the caller so it lands AFTER the round-end
        // character save (see the kill-credit note there).
        let xp = ironmud::progress::award_xp_to_character(char, &weapon_skill, 10, false);
        ironmud::progress::report_xp(
            connections,
            &char.name,
            &weapon_skill,
            &xp,
            ironmud::progress::XpSource::Combat,
        );
        if xp.leveled {
            *skill_gain_out = Some((weapon_skill.clone(), xp));
        }

        // Check if target died
        if mobile.current_hp <= 0 {
            let killed_vnum = mobile.vnum.clone();
            // Snapshot the damage map BEFORE process_mobile_death so all
            // contributing players can be credited (slice 3c party credit).
            let damaged_by = mobile.combat.damaged_by.clone();
            // Snapshot the display name too — the mobile row is deleted by
            // process_mobile_death, so the deferred block can't look it up.
            let mob_display = mobile.display_name().to_string();
            let alignment = mobile.alignment;
            let faction = mobile.faction.clone();
            let corpse_id = process_mobile_death(db, connections, &mut mobile, &room_id)?;
            // Defer kill-credit notifications (achievements, quests) to the
            // caller, which runs them only after persisting `char`. Firing
            // them here would write the award out-of-band, then the caller's
            // wholesale `char` save would clobber it (vanished first-kill
            // achievement). See `MobKill`.
            *kill_out = Some(MobKill {
                killed_vnum,
                damaged_by,
                corpse_id,
                mob_display,
                alignment,
                faction,
            });

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
/// Process a character's automatic swing against another player (sustained
/// PvP). Mirrors the PvE mob-vs-player hit formula and weapon dice, then
/// routes a downed victim through the existing unconscious -> bleedout ->
/// `process_player_death` flow (the victim's own combat round runs the
/// bleedout countdown), so PvP death is identical to PvE death.
///
/// Entry is already zone-gated by attack.rhai (player targets only resolve in
/// `Pvp` zones); this path additionally drops the engagement if the room is a
/// `Safe` zone or the victim has left, so combat can't be sustained where it
/// couldn't be started.
fn process_character_attacks_player(
    db: &db::Db,
    connections: &SharedConnections,
    state: &SharedState,
    char: &mut CharacterData,
    target: &CombatTarget,
    skill_gain_out: &mut Option<(String, ironmud::progress::XpOutcome)>,
) -> Result<()> {
    use rand::Rng;

    // Players are keyed by name; a player CombatTarget without a name is
    // unusable — drop it.
    let target_name = match target.target_name.as_deref() {
        Some(n) => n.to_string(),
        None => {
            char.combat
                .targets
                .retain(|t| t.target_type != CombatTargetType::Player);
            if char.combat.targets.is_empty() {
                char.combat.in_combat = false;
            }
            return Ok(());
        }
    };

    // Drop the player engagement and exit combat if it was the last target.
    fn drop_player_target(char: &mut CharacterData, name: &str) {
        char.combat.targets.retain(|t| !t.is_player_named(name));
        if char.combat.targets.is_empty() {
            char.combat.in_combat = false;
        }
    }

    let mut victim = match db.get_character_data(&target_name)? {
        Some(c) => c,
        None => {
            // Target logged out or was deleted.
            drop_player_target(char, &target_name);
            return Ok(());
        }
    };

    let room_id = char.current_room_id;

    if victim.current_room_id != room_id {
        drop_player_target(char, &target_name);
        send_message_to_character(connections, &char.name, "Your target is no longer here.");
        return Ok(());
    }

    // Combat can't be sustained in a Safe zone (entry was already blocked there).
    if ironmud::script::effective_combat_zone(db, &room_id) == CombatZoneType::Safe {
        drop_player_target(char, &target_name);
        return Ok(());
    }

    // Victim is already down — let their own bleedout finish; don't pile on.
    if victim.is_unconscious {
        return Ok(());
    }

    // A sleeping victim is jolted awake and eats an automatic hit.
    let was_sleeping = victim.position == CharacterPosition::Sleeping;
    if was_sleeping {
        victim.position = CharacterPosition::Standing;
        send_message_to_character(connections, &target_name, "You are jolted awake by an attack!");
    }

    // Ensure the engagement is mutual so the victim's own round retaliates and
    // runs the bleedout countdown.
    if !victim.combat.targets.iter().any(|t| t.is_player_named(&char.name)) {
        victim.combat.in_combat = true;
        victim.combat.targets.push(CombatTarget::player(char.name.clone()));
    }

    let mut rng = rand::thread_rng();

    // Attacker offense.
    let (weapon_skill, count, sides, weapon_bonus, weapon_damage_type) = get_character_weapon_info(db, char);
    let skill = get_skill_level_for_character(char, &weapon_skill);
    let (atk_hit_bonus, atk_dam_bonus, atk_dex_bonus) = sum_combat_buff_bonuses(&char.active_buffs);
    let attacker_dex_mod = (char.stat_dex as i32 + atk_dex_bonus - 10) / 2;
    let str_mod = (char.stat_str as i32 - 10) / 2;

    // Victim defense (mirrors process_mobile_attacks_player: dex + AC buffs).
    let (_, _, vic_dex_bonus) = sum_combat_buff_bonuses(&victim.active_buffs);
    let target_dex_mod = (victim.stat_dex as i32 + vic_dex_bonus - 10) / 2;
    let ac_buff_bonus: i32 = victim
        .active_buffs
        .iter()
        .filter(|b| b.effect_type == EffectType::ArmorClassBoost)
        .map(|b| b.magnitude)
        .sum();

    let mut hit_chance =
        (50 + skill * 5 + attacker_dex_mod - target_dex_mod - ac_buff_bonus + atk_hit_bonus).clamp(5, 95);
    if let Some(blind) = char.active_buffs.iter().find(|b| b.effect_type == EffectType::Blind) {
        hit_chance = (hit_chance - blind.magnitude).clamp(5, 95);
    }

    let roll = rng.gen_range(1..=100);
    if !was_sleeping && roll > hit_chance {
        send_message_to_character(
            connections,
            &char.name,
            &format!("You attack {} but miss!", victim.name),
        );
        let victim_name = victim.name.clone();
        let attacker_name = char.name.clone();
        broadcast_to_room_except_awake_per_viewer(connections, &room_id, &attacker_name, |viewer| {
            if viewer.name.eq_ignore_ascii_case(&victim_name) {
                format!("{} attacks you but misses!", attacker_name)
            } else {
                format!("{} attacks {} but misses!", attacker_name, victim_name)
            }
        });
        db.save_character_data(victim.clone())?;
        sync_character_to_session(connections, &victim, state);
        return Ok(());
    }

    // Hit. Damage = weapon dice + bonuses + strength, light crit at 1.5x.
    let mut damage = (roll_dice(count, sides) + weapon_bonus + atk_dam_bonus + str_mod).max(1);
    let crit_chance = 5 + skill / 2;
    let is_crit = rng.gen_range(1..=100) <= crit_chance;
    if is_crit {
        damage = (damage * 3) / 2;
    }
    // PvP crits are damage-only — no wound or stun is rolled — so the tag
    // stays generic rather than naming an effect the victim never suffered.
    // It shares the PvE renderer so the colour and bracket form match.
    let crit_text = ironmud::combat_text::crit_text(is_crit, "clean", "", false, weapon_damage_type);

    victim.hp -= damage;

    let verb = ironmud::combat_text::melee_hit_verb(
        weapon_damage_type,
        ironmud::combat_text::hit_severity(damage, count * sides),
    );
    send_message_to_character(
        connections,
        &char.name,
        &format!(
            "You {} {} for {} damage!{}",
            verb.second, victim.name, damage, crit_text
        ),
    );
    {
        let victim_name = victim.name.clone();
        let attacker_name = char.name.clone();
        broadcast_to_room_except_awake_per_viewer(connections, &room_id, &attacker_name, |viewer| {
            if viewer.name.eq_ignore_ascii_case(&victim_name) {
                format!("{} {} you for {} damage!", attacker_name, verb.third, damage)
            } else {
                format!(
                    "{} {} {} for {} damage!",
                    attacker_name, verb.third, victim_name, damage
                )
            }
        });
    }

    // Weapon-skill XP, on the same terms as the PvE branch (10 per landed
    // hit). This path used to award nothing at all, which made PvP the one
    // way to swing a weapon all day and never advance the skill. Awarded
    // here rather than after the downed handling below because the synth
    // path returns early and would otherwise swallow the credit.
    //
    // `is_language = false` for the same reason as the PvE branch: weapon
    // skills are never languages and the lookup would need the World lock.
    let xp = ironmud::progress::award_xp_to_character(char, &weapon_skill, 10, false);
    ironmud::progress::report_xp(
        connections,
        &char.name,
        &weapon_skill,
        &xp,
        ironmud::progress::XpSource::Combat,
    );
    if xp.leveled {
        *skill_gain_out = Some((weapon_skill.clone(), xp));
    }

    // Downed: route through the shared PvE unconscious/bleedout flow.
    // Synths run broken instead of collapsing (System Shutdown rule).
    if victim.hp <= 0 && victim.synth_state.is_some() {
        if handle_synth_down(db, connections, &mut victim, &room_id, state)? {
            // Death pipeline already saved/synced the respawned victim.
            return Ok(());
        }
    } else if victim.hp <= 0 {
        victim.hp = 0;
        victim.is_unconscious = true;
        victim.bleedout_rounds_remaining = 5;
        send_message_to_character(
            connections,
            &char.name,
            &format!("{} collapses, unconscious!", victim.name),
        );
        let victim_name = victim.name.clone();
        let attacker_name = char.name.clone();
        broadcast_to_room_except_awake_per_viewer(connections, &room_id, &attacker_name, |viewer| {
            if viewer.name.eq_ignore_ascii_case(&victim_name) {
                "You collapse, unconscious!".to_string()
            } else {
                format!("{} collapses, unconscious!", victim_name)
            }
        });
    }

    // Replicants: big hits stress the engineered mind (PvP path).
    apply_replicant_combat_stress(connections, &mut victim, damage, &room_id);
    // Werewolves: meaningful hits feed the Rage (PvP path).
    apply_werewolf_combat_rage(connections, &mut victim, damage, &room_id);

    db.save_character_data(victim.clone())?;
    sync_character_to_session(connections, &victim, state);
    Ok(())
}

/// Attempt to have a mobile flee from combat
/// Returns Some(true) if successfully fled, Some(false) if failed, None if couldn't attempt
fn attempt_mobile_flee(
    db: &db::Db,
    connections: &SharedConnections,
    mobile: &mut MobileData,
    state: &SharedState,
) -> Option<bool> {
    use rand::Rng;
    use rand::seq::SliceRandom;

    let room_id = mobile.current_room_id?;
    let room = db.get_room_data(&room_id).ok()??;

    // Build valid exit list using existing wander logic (clamped to home zone)
    let exits = get_valid_wander_exits(db, &room).ok()?;
    let exits = filter_exits_by_stay_zone(db, mobile, exits);
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
        sync_character_to_session(connections, &char_data, state);
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

    // Stamp the recent-combat timestamp (synth behavioral inhibitor reads
    // this to allow pursuing a mortal that fled). Throttled to one save per
    // 10s so a long fight doesn't double every round's writes.
    {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        if now - mobile.last_combat_at >= 10 {
            mobile.last_combat_at = now;
            db.save_mobile_data(mobile.clone())?;
        }
    }

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

    // Handle magical sleep — skip turn while the Sleep buff is active.
    if mobile
        .active_buffs
        .iter()
        .any(|b| b.effect_type == ironmud::EffectType::Sleep)
    {
        let mobile_name = mobile.name.clone();
        broadcast_to_room_awake(connections, &room_id, &format!("{} sleeps peacefully.", mobile_name));
        return Ok(());
    }

    // Handle physical stance — sleeping mobs skip their turn entirely.
    // Wake-on-damage in `process_character_attacks_mobile` transitions
    // them to Sitting before this round runs again.
    if mobile.position == ironmud::types::MobilePosition::Sleeping {
        let mobile_name = mobile.name.clone();
        broadcast_to_room_awake(connections, &room_id, &format!("{} sleeps soundly.", mobile_name));
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
            // We don't track who applied each ongoing effect, so deaths
            // here can't be credited to a specific player. Skip the
            // achievement notify rather than mis-credit the killer.
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

    if mobile.tires() && mobile.current_stamina <= 0 {
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

    // Fire DG OnFight + OnHitPercent triggers (Phase 3) before action.
    // OnFight runs each round; OnHitPercent fires once when HP% crosses
    // a trigger-specified threshold and re-arms when HP% rises above it.
    fire_combat_dg_triggers(db, connections, state, &mut mobile);

    // Consume stamina for attack (bloodless mobiles never tire).
    if mobile.tires() {
        mobile.current_stamina = (mobile.current_stamina - MOBILE_COMBAT_STAMINA_COST).max(0);
    }

    // A terrified mobile spends every round trying to escape — it never
    // attacks while the Feared buff holds, whether or not the flee lands.
    if ironmud::script::fear::is_feared(&mobile.active_buffs) {
        broadcast_to_room_awake(
            connections,
            &room_id,
            &format!("{} shrieks in terror and scrambles to escape!", mobile.name),
        );
        let _ = attempt_mobile_flee(db, connections, &mut mobile, state);
        return Ok(());
    }

    // Check if mobile should attempt to flee (HP <= 25%)
    let mut rng = rand::thread_rng();
    if mobile.current_hp > 0 && mobile.max_hp > 0 {
        let hp_percent = (mobile.current_hp * 100) / mobile.max_hp;
        if hp_percent <= 25 {
            // Cowardly mobs always flee; normal mobs have 30% chance
            let should_flee = mobile.flags.cowardly || rng.gen_range(0..100) < 30;
            if should_flee {
                if let Some(fled) = attempt_mobile_flee(db, connections, &mut mobile, state) {
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

            // CircleMUD `magic_user` analog: if the mob has a combat_spells list,
            // roll combat_spell_chance to cast a random spell instead of swinging.
            // Cast happens after the close-the-distance branch above, so a mob still
            // spends its round closing if it isn't yet at melee range.
            let mut cast_this_round = false;
            if !mobile.combat_spells.is_empty() && mobile.combat_spell_chance > 0 {
                use rand::Rng;
                use rand::seq::SliceRandom;
                let mut rng = rand::thread_rng();
                let roll = rng.gen_range(1..=100);
                if roll <= mobile.combat_spell_chance as i32 {
                    if let Some(spell_id) = mobile.combat_spells.choose(&mut rng).cloned() {
                        cast_this_round = mob_cast_spell_at_player(
                            db,
                            connections,
                            &mut mobile,
                            &player_name,
                            &room_id,
                            &spell_id,
                            state,
                        )?;
                    }
                }
            }
            if !cast_this_round {
                process_mobile_attacks_player(db, connections, &mut mobile, &player_name, &room_id, state)?;
            }
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

/// Fire DG `OnFight` (every round) and `OnHitPercent` (once per
/// crossing) triggers on a combatant mobile. Mutates `mobile.triggers[i]
/// .last_fired` as a flag for HitPercent re-arm semantics.
fn fire_combat_dg_triggers(db: &db::Db, connections: &SharedConnections, state: &SharedState, mobile: &mut MobileData) {
    use ironmud::MobileTriggerType;

    let db_arc = std::sync::Arc::new(db.clone());

    // OnFight: fire all matching dg-bodied triggers every round.
    let snapshot = mobile.clone();
    ironmud::script::dg::fire_mobile_dg_triggers(
        &db_arc,
        connections,
        state,
        &snapshot,
        MobileTriggerType::OnFight,
        "",
        "",
        "",
        "",
    );

    // OnHitPercent: re-arm/fire logic. Each trigger's args[0] is a
    // numeric threshold (1-99 inclusive). last_fired==0 means armed;
    // non-zero means already-fired in the current crossing window.
    if mobile.max_hp <= 0 {
        return;
    }
    let hp_pct = (mobile.current_hp.max(0) * 100) / mobile.max_hp;
    let mut needs_save = false;
    let mut to_fire: Vec<usize> = Vec::new();
    for (i, t) in mobile.triggers.iter_mut().enumerate() {
        if t.trigger_type != MobileTriggerType::OnHitPercent || !t.enabled {
            continue;
        }
        let threshold: i32 = t.args.first().and_then(|s| s.parse().ok()).unwrap_or(0);
        if threshold <= 0 || threshold >= 100 {
            continue;
        }
        if hp_pct <= threshold {
            if t.last_fired == 0 {
                to_fire.push(i);
                t.last_fired = 1; // armed-flag (not a wall-clock ts here).
                needs_save = true;
            }
        } else if t.last_fired != 0 {
            t.last_fired = 0;
            needs_save = true;
        }
    }
    if !to_fire.is_empty() {
        let snapshot = mobile.clone();
        for &i in &to_fire {
            let t = &snapshot.triggers[i];
            if let Some(body) = t.dg_body.as_deref() {
                let _ = ironmud::script::dg::fire_mobile_dg(
                    body,
                    &snapshot,
                    "",
                    db_arc.clone(),
                    connections.clone(),
                    state.clone(),
                    t.authored_by.clone(),
                    t.elevated,
                    std::collections::HashMap::new(),
                );
            }
        }
    }
    if needs_save {
        let _ = db.save_mobile_data(mobile.clone());
    }
}

/// Replicant combat stress: a single hit at or above
/// `BIG_HIT_RESOLVE_THRESHOLD_PCT`% of max HP drains 1 Resolve; hitting 0
/// triggers a breakdown on the critical-stress table. Call after damage is
/// applied and BEFORE the caller saves the character — the mutation rides
/// the caller's save/sync. No-op for non-replicants, the already-broken,
/// and the dying (hp <= 0 hands off to the death pipeline instead).
fn apply_replicant_combat_stress(
    connections: &SharedConnections,
    char: &mut ironmud::CharacterData,
    damage: i32,
    room_id: &uuid::Uuid,
) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let threshold = (char.max_hp * ironmud::types::BIG_HIT_RESOLVE_THRESHOLD_PCT / 100).max(1);
    let stressed = match char.replicant_state.as_ref() {
        Some(r) if char.hp > 0 && damage >= threshold && !r.is_breaking_down(now) => true,
        _ => false,
    };
    if !stressed {
        return;
    }
    let new_resolve = char
        .replicant_state
        .as_mut()
        .map(|r| r.change_resolve(-1))
        .unwrap_or(-1);
    send_message_to_character(
        connections,
        &char.name,
        "\x1b[35mThe hit rattles something deep in your engineered mind. (-1 Resolve)\x1b[0m",
    );
    if new_resolve == 0 {
        let outcome = ironmud::replicant::trigger_breakdown_rolled(char, now);
        send_message_to_character(
            connections,
            &char.name,
            &format!("\x1b[1;31m{}\x1b[0m", outcome.message),
        );
        broadcast_to_room_except_awake(
            connections,
            room_id,
            &outcome.room_message.replace("{name}", &char.name),
            &char.name,
        );
    }
}

/// Werewolf rage gain from taking a meaningful hit (at least 5% of max HP —
/// chip damage doesn't feed the wolf). Overflow at the cap forces a frenzy
/// roll on the spot. Tribe banes apply on the rage tick's rolls; the
/// in-combat overflow roll runs unmodified (dc 0) to keep the World lock
/// out of the per-hit path. Caller saves the character afterward.
fn apply_werewolf_combat_rage(
    connections: &SharedConnections,
    char: &mut ironmud::CharacterData,
    damage: i32,
    room_id: &uuid::Uuid,
) {
    if char.werewolf_state.is_none() || char.hp <= 0 {
        return;
    }
    let threshold = (char.max_hp / 20).max(1);
    if damage < threshold {
        return;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (rage, frenzied) = ironmud::werewolf::gain_rage_rolled(char, ironmud::types::RAGE_GAIN_ON_DAMAGE, 0, now);
    send_message_to_character(
        connections,
        &char.name,
        &format!("\x1b[31mThe pain feeds the Rage. ({})\x1b[0m", rage),
    );
    if frenzied {
        announce_werewolf_frenzy(connections, char, room_id);
    }
}

/// Werewolf rage gain from a credited kill. Same overflow rule as the
/// damage hook. Caller saves the character afterward.
fn apply_werewolf_kill_rage(connections: &SharedConnections, char: &mut ironmud::CharacterData, room_id: &uuid::Uuid) {
    if char.werewolf_state.is_none() {
        return;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (rage, frenzied) = ironmud::werewolf::gain_rage_rolled(char, ironmud::types::RAGE_GAIN_ON_KILL, 0, now);
    send_message_to_character(
        connections,
        &char.name,
        &format!("\x1b[31mThe kill sings in your blood. Rage rises. ({})\x1b[0m", rage),
    );
    if frenzied {
        announce_werewolf_frenzy(connections, char, room_id);
    }
}

fn announce_werewolf_frenzy(connections: &SharedConnections, char: &ironmud::CharacterData, room_id: &uuid::Uuid) {
    send_message_to_character(
        connections,
        &char.name,
        "\x1b[1;31mThe Rage crests. Fur splits skin — the wolf is driving now.\x1b[0m",
    );
    broadcast_to_room_except_awake(
        connections,
        room_id,
        &format!(
            "{}'s eyes flood amber. Something with too many teeth is wearing their face.",
            char.name
        ),
        &char.name,
    );
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
        sync_character_to_session(connections, &char, state);
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
            char.combat.targets.push(CombatTarget::mobile(mobile.id));
        }
        db.save_character_data(char.clone())?;
        sync_character_to_session(connections, &char, state);
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
    let (_, _, char_eq_dex_bonus) = sum_combat_buff_bonuses(&char.active_buffs);
    let (mob_eq_hit_bonus, mob_eq_dam_bonus, mob_eq_dex_bonus) = sum_combat_buff_bonuses(&mobile.active_buffs);
    let attacker_dex_mod = (mobile.stat_dex as i32 + mob_eq_dex_bonus - 10) / 2;
    let target_dex_mod = (char.stat_dex as i32 + char_eq_dex_bonus - 10) / 2;
    // Calculate player AC from armor + ArmorClassBoost buffs
    let ac_buff_bonus: i32 = char
        .active_buffs
        .iter()
        .filter(|b| b.effect_type == EffectType::ArmorClassBoost)
        .map(|b| b.magnitude)
        .sum();
    let target_ac = ac_buff_bonus;
    let skill = mobile.hit_modifier; // Mobile skill level based on difficulty

    let mut hit_chance =
        (50 + skill * 5 + attacker_dex_mod - target_dex_mod - target_ac + mob_eq_hit_bonus).clamp(5, 95);

    // Invisible-target penalty: -30 to-hit when the mobile can't see
    // its target (PC has Invisibility and the mob lacks DetectInvisible
    // and isn't AWARE).
    let target_invisible = char
        .active_buffs
        .iter()
        .any(|b| b.effect_type == EffectType::Invisibility);
    let mob_detects = mobile.flags.aware
        || mobile
            .active_buffs
            .iter()
            .any(|b| b.effect_type == EffectType::DetectInvisible);
    if target_invisible && !mob_detects {
        hit_chance = (hit_chance - 30).clamp(5, 95);
    }

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

    // Blind buff slashes mob accuracy too. Magnitude is the percentage points to subtract.
    if let Some(blind) = mobile.active_buffs.iter().find(|b| b.effect_type == EffectType::Blind) {
        hit_chance = (hit_chance - blind.magnitude).clamp(5, 95);
    }

    if let Some(curse) = mobile.active_buffs.iter().find(|b| b.effect_type == EffectType::Curse) {
        hit_chance = (hit_chance - curse.magnitude).clamp(5, 95);
    }

    if let Some(bless) = mobile.active_buffs.iter().find(|b| b.effect_type == EffectType::Bless) {
        hit_chance = (hit_chance + bless.magnitude * 3).clamp(5, 95);
    }

    if let Some(haste) = mobile.active_buffs.iter().find(|b| b.effect_type == EffectType::Haste) {
        hit_chance = (hit_chance + haste.magnitude * 2).clamp(5, 95);
    }
    if let Some(slow) = mobile.active_buffs.iter().find(|b| b.effect_type == EffectType::Slow) {
        hit_chance = (hit_chance - slow.magnitude * 2).clamp(5, 95);
    }

    let roll = rng.gen_range(1..=100);

    // Skip hit roll if target was sleeping (automatic hit)
    if !was_sleeping && roll > hit_chance {
        // Miss
        let miss_verb = ironmud::combat_text::miss_verb(damage_type);
        let target_attacker_name = mob_display_name_for(&char, mobile, false);
        send_message_to_character(
            connections,
            player_name,
            &format!("{} {} you but misses!", target_attacker_name, miss_verb),
        );
        broadcast_to_room_except_awake_per_viewer(connections, room_id, player_name, |viewer| {
            format!(
                "{} {} {} but misses!",
                mob_display_name_for(viewer, mobile, false),
                miss_verb,
                char.name
            )
        });
        return Ok(());
    }

    // Hit - calculate damage (includes APPLY_DAMROLL bonuses from equipped items)
    let mut damage = roll_dice(count, sides) + bonus + mob_eq_dam_bonus;

    if let Some(bless) = mobile.active_buffs.iter().find(|b| b.effect_type == EffectType::Bless) {
        damage += (bless.magnitude + 1) / 2;
    }

    // Apply underwater damage type modifier
    let (modified_damage, water_msg) = apply_underwater_modifier(db, room_id, damage, damage_type);
    damage = modified_damage;
    if water_msg.is_some() && damage == 0 {
        // Fire attacks extinguished by water (underwater rooms only)
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

    // Track the critical effect for messaging. The key doubles as the
    // mechanic (see `combat_text::crit_mechanic`), so the tag the player reads
    // always matches what actually hit them.
    let mut crit_effect = String::new();
    let mut crit_body_part = String::new();

    if is_crit {
        use ironmud::combat_text::CritMechanic;

        // Scale damage: 1.5x for mobiles (no skill)
        damage = (damage * 3) / 2;

        crit_effect = ironmud::combat_text::roll_crit_effect(damage_type, rng.gen_range(1..=4)).to_string();
        crit_body_part = roll_random_body_part(&mut rng);
        let mechanic = ironmud::combat_text::crit_mechanic(&crit_effect);
        let severity = 2; // Base severity for mobiles

        // Wound helpers save the character themselves, so they run before the
        // reload below and their writes survive it.
        match mechanic {
            CritMechanic::Bleed => {
                add_character_wound_bleeding(db, player_name, &crit_body_part, severity)?;
            }
            CritMechanic::StunBleed => {
                add_character_wound_bleeding(db, player_name, &crit_body_part, 2)?;
            }
            CritMechanic::Disable => {
                escalate_character_wound_to_severe(db, player_name, &crit_body_part)?;

                // Weapon drop on arm/hand disable
                match crit_body_part.as_str() {
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
            }
            _ => {}
        }

        // Reload character from DB to get any wounds that were added
        // (add_character_wound_bleeding and escalate_character_wound_to_severe save directly to DB)
        if let Some(fresh_char) = db.get_character_data(player_name)? {
            char = fresh_char;
        }

        // In-memory mutations go after the reload so it cannot discard them.
        if matches!(
            mechanic,
            CritMechanic::Stun | CritMechanic::StunBleed | CritMechanic::OngoingStun
        ) {
            char.combat.stun_rounds_remaining += 1;
        }
        if matches!(mechanic, CritMechanic::Ongoing | CritMechanic::OngoingStun) {
            char.ongoing_effects.push(ironmud::OngoingEffect {
                effect_type: ironmud::combat_text::crit_ongoing_element(&crit_effect).to_string(),
                rounds_remaining: 3,
                damage_per_round: severity,
                body_part: crit_body_part.clone(),
            });
        }
        db.save_character_data(char.clone())?;
    }

    // Apply combined typed resistance: race + item-stamped buffs.
    // Race resistance lookup is scoped (drops lock before further work).
    {
        let race_id = char.race.to_lowercase();
        let dmg_type_str = damage_type.to_display_string();
        let race_resist = {
            let world = state.lock().unwrap();
            world
                .race_definitions
                .get(&race_id)
                .and_then(|r| r.resistances.get(dmg_type_str).copied())
                .unwrap_or(0)
        };
        let item_resist: i32 = char
            .active_buffs
            .iter()
            .filter(|b| b.effect_type == EffectType::DamageResistance && b.damage_type == Some(damage_type))
            .map(|b| b.magnitude)
            .sum();
        let total = (race_resist + item_resist).clamp(-100, 95);
        if total != 0 {
            damage = (damage * (100 - total)) / 100;
            if damage < 1 {
                damage = 1;
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
    ironmud::interrupt_writer_by_name(connections, state, &char.name);

    // Apply on-hit DoT effects from mobile flags (poisonous, fiery, chilling, corrosive, shocking)
    ironmud::script::apply_mobile_on_hit_dots(mobile, &mut char.ongoing_effects, "body");

    // Apply per-mobile `on_hit_effects` (composes with the legacy flag-based DOTs above).
    let mob_on_hit_messages = if mobile.on_hit_effects.is_empty() {
        Vec::new()
    } else {
        let body_part = roll_random_body_part(&mut rng);
        apply_on_hit_effects_to_character(&mobile.on_hit_effects, &mut char, &mobile.name, &body_part).room_messages
    };

    // Magical sleep breaks on any damage taken.
    let player_was_sleeping = char
        .active_buffs
        .iter()
        .any(|b| b.effect_type == ironmud::EffectType::Sleep);
    if player_was_sleeping {
        char.active_buffs
            .retain(|b| b.effect_type != ironmud::EffectType::Sleep);
    }

    // Replicants: big hits stress the engineered mind.
    apply_replicant_combat_stress(connections, &mut char, damage, room_id);
    // Werewolves: meaningful hits feed the Rage.
    apply_werewolf_combat_rage(connections, &mut char, damage, room_id);

    db.save_character_data(char.clone())?;
    if player_was_sleeping {
        send_message_to_character(connections, &char.name, "You jolt awake!");
    }

    // Sync updated character to session so prompt shows correct HP
    sync_character_to_session(connections, &char, state);

    // Build message with crit text, coloured by damage type.
    let crit_text = ironmud::combat_text::crit_text(is_crit, &crit_effect, &crit_body_part, false, damage_type);

    // Send messages (red for damage taken) - sleeping bystanders don't see combat
    // Tiered by severity for the same reason the player's own swings are: a
    // scrape and a maiming blow should not read identically.
    let hit_verb =
        ironmud::combat_text::melee_hit_verb(damage_type, ironmud::combat_text::hit_severity(damage, count * sides))
            .third;
    let target_attacker_name = mob_display_name_for(&char, mobile, false);
    send_message_to_character(
        connections,
        player_name,
        &format!(
            "\x1b[1;31m{} {} you for {} damage!{}\x1b[0m",
            target_attacker_name, hit_verb, damage, crit_text
        ),
    );
    broadcast_to_room_except_awake_per_viewer(connections, room_id, player_name, |viewer| {
        format!(
            "{} {} {} for {} damage!",
            mob_display_name_for(viewer, mobile, false),
            hit_verb,
            char.name,
            damage
        )
    });

    // Broadcast on-hit effect lines (bleeding/elemental/status from mobile.on_hit_effects)
    for line in &mob_on_hit_messages {
        broadcast_to_room_awake(connections, room_id, line);
    }

    // Check if player died or went unconscious
    if char.hp <= 0 {
        if char.synth_state.is_some() {
            // Synths run broken instead of collapsing: critical keeps the
            // fight going (no aggressive-mob coup de grâce on a standing
            // target); a lethal hit while critical is System Shutdown.
            if handle_synth_down(db, connections, &mut char, room_id, state)? {
                mobile
                    .combat
                    .targets
                    .retain(|t| t.target_type != CombatTargetType::Player);
                if mobile.combat.targets.is_empty() {
                    mobile.combat.in_combat = false;
                }
            }
        } else if char.is_unconscious {
            // Already unconscious and took damage - instant death!
            process_player_death(db, connections, &mut char, room_id, state)?;

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
            sync_character_to_session(connections, &char, state);

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
                    process_player_death(db, connections, &mut char, room_id, state)?;

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

/// CircleMUD `magic_user` analog. Resolve `spell_id` from the loaded spell
/// definitions and apply its effect to `player_name`. Returns `Ok(true)` when
/// a spell was actually cast (caller should skip the melee swing this round);
/// `Ok(false)` when the spell is unknown or the target left the room (caller
/// falls through to melee).
fn mob_cast_spell_at_player(
    db: &db::Db,
    connections: &SharedConnections,
    mobile: &mut MobileData,
    player_name: &str,
    room_id: &uuid::Uuid,
    spell_id: &str,
    state: &SharedState,
) -> Result<bool> {
    use ironmud::ActiveBuff;

    // Snapshot the spell definition so we don't hold the World lock past here.
    let spell = {
        let world = state.lock().unwrap();
        match world.spell_definitions.get(spell_id) {
            Some(s) => s.clone(),
            None => return Ok(false),
        }
    };

    let mut char = match db.get_character_data(player_name)? {
        Some(c) => c,
        None => return Ok(false),
    };
    if char.current_room_id != *room_id {
        mobile.combat.in_combat = false;
        mobile.combat.targets.clear();
        return Ok(false);
    }

    // Telegraph the cast (Circle's specprocs always announce the chant first).
    send_message_to_character(
        connections,
        player_name,
        &format!(
            "\x1b[1;35m{} chants softly, weaving a spell at you!\x1b[0m",
            mobile.name
        ),
    );
    broadcast_to_room_except_awake(
        connections,
        room_id,
        &format!("{} chants softly, weaving a spell at {}!", mobile.name, char.name),
        player_name,
    );

    // Reactive combat (mirror of process_mobile_attacks_player).
    if !char.combat.in_combat || !char.combat.targets.iter().any(|t| t.target_id == mobile.id) {
        char.combat.in_combat = true;
        if !char.combat.targets.iter().any(|t| t.target_id == mobile.id) {
            char.combat.targets.push(CombatTarget::mobile(mobile.id));
        }
        db.save_character_data(char.clone())?;
        sync_character_to_session(connections, &char, state);
    }

    match spell.spell_type.as_str() {
        "damage" => {
            // Damage formula mirrors cast.rhai's player-side roll, with mob.level
            // standing in for the player's magic skill rank.
            let int_bonus = (mobile.stat_int as i32 - 10).max(0);
            let mut damage =
                spell.damage_base + spell.damage_per_skill * mobile.level + spell.damage_int_scaling * int_bonus;

            // Map damage_type string onto DamageType for resistance + flavor.
            let damage_type = ironmud::DamageType::from_str(&spell.damage_type).unwrap_or(ironmud::DamageType::Arcane);

            // Apply combined typed resistance: race + item-stamped buffs.
            {
                let race_id = char.race.to_lowercase();
                let dmg_type_str = damage_type.to_display_string();
                let race_resist = {
                    let world = state.lock().unwrap();
                    world
                        .race_definitions
                        .get(&race_id)
                        .and_then(|r| r.resistances.get(dmg_type_str).copied())
                        .unwrap_or(0)
                };
                let item_resist: i32 = char
                    .active_buffs
                    .iter()
                    .filter(|b| {
                        b.effect_type == ironmud::EffectType::DamageResistance && b.damage_type == Some(damage_type)
                    })
                    .map(|b| b.magnitude)
                    .sum();
                let total = (race_resist + item_resist).clamp(-100, 95);
                if total != 0 {
                    damage = (damage * (100 - total)) / 100;
                    if damage < 1 {
                        damage = 1;
                    }
                }
            }

            // DR buffs.
            damage = ironmud::script::apply_damage_reduction(damage, &char.active_buffs);
            if damage < 1 {
                damage = 1;
            }

            char.hp -= damage;
            ironmud::interrupt_writer_by_name(connections, state, &char.name);

            // Magical sleep breaks on any damage taken.
            let player_was_sleeping = char.active_buffs.iter().any(|b| b.effect_type == EffectType::Sleep);
            if player_was_sleeping {
                char.active_buffs.retain(|b| b.effect_type != EffectType::Sleep);
            }

            // Replicants: big hits stress the engineered mind.
            apply_replicant_combat_stress(connections, &mut char, damage, room_id);
            // Werewolves: meaningful hits feed the Rage.
            apply_werewolf_combat_rage(connections, &mut char, damage, room_id);

            db.save_character_data(char.clone())?;
            if player_was_sleeping {
                send_message_to_character(connections, &char.name, "You jolt awake!");
            }
            sync_character_to_session(connections, &char, state);

            let dtype_lower = spell.damage_type.to_lowercase();
            send_message_to_character(
                connections,
                player_name,
                &format!(
                    "\x1b[1;31m{}'s {} strikes you for {} damage!\x1b[0m",
                    mobile.name, spell.name, damage
                ),
            );
            broadcast_to_room_except_awake(
                connections,
                room_id,
                &format!(
                    "{}'s {} strikes {} with {} energy!",
                    mobile.name, spell.name, char.name, dtype_lower
                ),
                player_name,
            );

            // Death handling mirrors process_mobile_attacks_player.
            if char.hp <= 0 {
                if char.synth_state.is_some() {
                    // Synths run broken instead of collapsing.
                    if handle_synth_down(db, connections, &mut char, room_id, state)? {
                        mobile
                            .combat
                            .targets
                            .retain(|t| t.target_type != CombatTargetType::Player);
                        if mobile.combat.targets.is_empty() {
                            mobile.combat.in_combat = false;
                        }
                    }
                } else if char.is_unconscious {
                    process_player_death(db, connections, &mut char, room_id, state)?;
                    mobile
                        .combat
                        .targets
                        .retain(|t| t.target_type != CombatTargetType::Player);
                    if mobile.combat.targets.is_empty() {
                        mobile.combat.in_combat = false;
                    }
                } else {
                    char.is_unconscious = true;
                    char.bleedout_rounds_remaining = 5;
                    let char_name_for_msg = char.name.clone();
                    db.save_character_data(char.clone())?;
                    sync_character_to_session(connections, &char, state);
                    send_message_to_character(connections, player_name, "You collapse, unconscious!");
                    broadcast_to_room_except_awake(
                        connections,
                        room_id,
                        &format!("{} collapses, unconscious!", char_name_for_msg),
                        player_name,
                    );
                    if mobile.flags.aggressive {
                        if let Ok(Some(mut char)) = db.get_character_data(player_name) {
                            process_player_death(db, connections, &mut char, room_id, state)?;
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
            Ok(true)
        }
        "debuff" => {
            // Stamp an ActiveBuff onto the player for the spell's effect.
            let effect_type = match EffectType::from_str(&spell.buff_effect) {
                Some(e) => e,
                None => return Ok(false),
            };

            let duration = if spell.buff_duration_secs > 0 {
                spell.buff_duration_secs
            } else {
                60
            };

            // Replace any existing buff of the same type from this caster.
            char.active_buffs
                .retain(|b| b.effect_type != effect_type || b.source != mobile.name);
            char.active_buffs.push(ActiveBuff {
                effect_type,
                magnitude: spell.buff_magnitude,
                remaining_secs: duration,
                source: mobile.name.clone(),
                damage_type: None,
                vs_effect: None,
                skill_key: None,
            });
            db.save_character_data(char.clone())?;
            sync_character_to_session(connections, &char, state);

            send_message_to_character(
                connections,
                player_name,
                &format!("\x1b[1;35m{}'s {} washes over you!\x1b[0m", mobile.name, spell.name),
            );
            broadcast_to_room_except_awake(
                connections,
                room_id,
                &format!("{}'s {} washes over {}!", mobile.name, spell.name, char.name),
                player_name,
            );
            Ok(true)
        }
        // Heal/buff/utility/etc. spells aren't meaningful as offensive picks.
        // Falling through to melee is the safe default.
        _ => Ok(false),
    }
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

/// Sum APPLY_HITROLL / APPLY_DAMROLL / APPLY_DEX bonuses from the entity's
/// `active_buffs`. Equipped items stamp these as permanent buffs at wear time
/// (see `db::stamp_item_buffs_on_character`), so reading from buffs covers
/// both gear and any spell-cast bonuses uniformly. Returns (hit, dam, dex).
fn sum_combat_buff_bonuses(buffs: &[ActiveBuff]) -> (i32, i32, i32) {
    let mut hit = 0;
    let mut dam = 0;
    let mut dex = 0;
    for b in buffs {
        match b.effect_type {
            EffectType::HitBonus => hit += b.magnitude,
            EffectType::DamageBonus => dam += b.magnitude,
            EffectType::DexterityBoost => dex += b.magnitude,
            _ => {}
        }
    }
    (hit, dam, dex)
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

/// Load the wielded weapon's `on_hit_effects` and roll them against `mobile`.
/// Empty (no-op) when the attacker has no wielded weapon. Returns flavour
/// lines for the room broadcaster.
fn apply_weapon_on_hit_to_mobile(
    db: &db::Db,
    char: &CharacterData,
    mobile: &mut MobileData,
    body_part: &str,
) -> Vec<String> {
    let Some(wid) = get_character_wielded_weapon_id(db, char) else {
        return Vec::new();
    };
    let Ok(Some(weapon)) = db.get_item_data(&wid) else {
        return Vec::new();
    };
    if weapon.on_hit_effects.is_empty() {
        return Vec::new();
    }
    apply_on_hit_effects_to_mobile(&weapon.on_hit_effects, mobile, &char.name, body_part).room_messages
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

/// Returns the name a viewer should see when a mob is referenced in
/// a combat message. If the mob has the Invisibility buff and the
/// viewer lacks DetectInvisible (and isn't admin), returns
/// "Something" or "something" depending on `lowered`. Otherwise
/// returns the mob's actual name.
fn mob_display_name_for(viewer: &CharacterData, mob: &MobileData, lowered: bool) -> String {
    let mob_invisible = mob
        .active_buffs
        .iter()
        .any(|b| b.effect_type == EffectType::Invisibility);
    if !mob_invisible {
        return mob.display_name().to_string();
    }
    let viewer_detects = viewer.is_admin
        || viewer
            .active_buffs
            .iter()
            .any(|b| b.effect_type == EffectType::DetectInvisible);
    if viewer_detects {
        mob.display_name().to_string()
    } else if lowered {
        "something".to_string()
    } else {
        "Something".to_string()
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

// `add_skill_experience_to_character` used to live here with its own private
// copy of the XP curve. It has been folded into
// `ironmud::progress::award_xp_to_character`, which is the single
// implementation for every path that awards skill XP. Note the old copy also
// silently dropped XP for a skill the character had never used (it created the
// entry with `level: 0` and then returned without checking for a level-up);
// the chokepoint handles first-use correctly.

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
/// Shared "runs broken" handler for every tick-side point where a synth's HP
/// hits 0. First failure goes CRITICAL (floored at 1 HP, debuffs, shutdown
/// countdown — the synth stays on its feet); a lethal hit while already
/// critical is System Shutdown and runs the death pipeline. Returns true if
/// the synth died. Caller must have checked `char.synth_state.is_some()`.
pub fn handle_synth_down(
    db: &db::Db,
    connections: &SharedConnections,
    char: &mut CharacterData,
    room_id: &uuid::Uuid,
    state: &SharedState,
) -> Result<bool> {
    use ironmud::synth::{
        SYNTH_CRITICAL_MESSAGE, SYNTH_CRITICAL_ROOM_MESSAGE, SYNTH_SHUTDOWN_MESSAGE, SYNTH_SHUTDOWN_ROOM_MESSAGE,
        SynthDownOutcome, synth_down_transition,
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    match synth_down_transition(char, now) {
        Some(SynthDownOutcome::Critical) => {
            db.save_character_data(char.clone())?;
            sync_character_to_session(connections, char, state);
            send_message_to_character(connections, &char.name, SYNTH_CRITICAL_MESSAGE);
            broadcast_to_room_except_awake(
                connections,
                room_id,
                &SYNTH_CRITICAL_ROOM_MESSAGE.replace("{name}", &char.name),
                &char.name,
            );
            Ok(false)
        }
        Some(SynthDownOutcome::Shutdown) => {
            char.hp = 0;
            send_message_to_character(connections, &char.name, SYNTH_SHUTDOWN_MESSAGE);
            broadcast_to_room_except_awake(
                connections,
                room_id,
                &SYNTH_SHUTDOWN_ROOM_MESSAGE.replace("{name}", &char.name),
                &char.name,
            );
            process_player_death(db, connections, char, room_id, state)?;
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Kill a player at `room_id`. Delegates to the lib-side implementation.
///
/// There used to be a full copy here, and a second near-copy in
/// `src/session/death.rs` for the ROOM_DEATH script path. They had drifted:
/// only this one bumped the `deaths` counter and credited PvP worship, so
/// walking into a death trap did not count as dying. The flow now lives in the
/// lib once; this stays as the name the tick modules already import.
pub fn process_player_death(
    db: &db::Db,
    connections: &SharedConnections,
    char: &mut CharacterData,
    room_id: &uuid::Uuid,
    state: &SharedState,
) -> Result<()> {
    ironmud::session::death::process_player_death(db, connections, state, char, room_id)
}

/// Process mobile death: create corpse with items
/// Returns the id of the corpse it created, so a caller that credited the kill
/// can report what dropped without re-scanning the room for a corpse it can't
/// reliably identify. Callers that don't need it can ignore the value.
pub fn process_mobile_death(
    db: &db::Db,
    connections: &SharedConnections,
    mobile: &mut MobileData,
    room_id: &uuid::Uuid,
) -> Result<uuid::Uuid> {
    debug!("process_mobile_death: starting for {}", mobile.name);
    let mobile_name = mobile.name.clone();
    let mobile_display = mobile.display_name().to_string();

    // Send death message (red) - sleeping bystanders don't see combat
    debug!("process_mobile_death: broadcasting death message");
    broadcast_to_room_awake(
        connections,
        room_id,
        &format!("\x1b[1;31m{} collapses to the ground, dead!\x1b[0m", mobile_display),
    );
    debug!("process_mobile_death: death message broadcast complete");

    // Pet death: notify the owner if they're online, regardless of room.
    // Live broadcast only — drops on the floor if the owner is offline.
    if let Some(ref owner) = mobile.pet_owner {
        if !owner.is_empty() {
            send_message_to_character(
                connections,
                owner,
                &format!("\x1b[1;31mYou feel a chill — {} has died.\x1b[0m", mobile_display),
            );
        }
    }

    // Create corpse using builder with random gold variance.
    // Logs base (mobile.gold) vs rolled so a "corpse has 0 gold" report can be
    // traced to either a 0-gold mob (data/sim drain) or the drop math.
    debug!("process_mobile_death: {} base gold={}", mobile.name, mobile.gold);
    let gold = mobile_gold_with_variance(mobile.gold as i64);
    let corpse = CorpseBuilder::for_mobile(&mobile_name, *room_id, gold)
        .with_source_vnum(Some(mobile.vnum.clone()))
        .build();

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

    Ok(corpse_id)
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

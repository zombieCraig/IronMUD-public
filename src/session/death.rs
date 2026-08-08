//! Player death — the single implementation.
//!
//! There used to be two. `process_player_death` lived in
//! `src/ticks/combat/tick.rs` (bin-side, reachable from the combat, bleedout
//! and synth ticks) and `kill_player_at_room` lived here (lib-side, reachable
//! from `src/script/*` for the ROOM_DEATH path). They were near-identical, and
//! they had already drifted: the lib copy bumped no `deaths` counter and
//! credited no PvP worship, so **walking into a death trap did not count as
//! dying** — the counter item 11d added, which `top` ranks and achievements
//! read, simply never moved for that route. It also carried its own
//! hand-rolled corpse literal duplicating [`crate::corpse::CorpseBuilder`].
//!
//! The flow now lives here in full and the tick calls it. That is the same
//! direction the codebase already took for other bin/lib mirrors, and it is
//! possible because `SharedState` reaches everywhere after Tier 1 item 11b.

use std::sync::Arc;

use anyhow::Result;
use uuid::Uuid;

use crate::db::Db;
use crate::session::broadcast::broadcast_to_room;
use crate::types::{CharacterData, CombatTargetType, EffectType, ItemLocation};
use crate::{SharedConnections, SharedState};

/// Drop any `EffectType::Charmed` buffs sourced to `player_name` from every
/// non-prototype mobile in the world. Used on player death and quit so that
/// charmed mobs revert immediately rather than waiting for the buff to decay.
/// Also clears `charm_stay` / `charm_follow_player` on those mobs, and clears
/// dangling `charm_follow_player == player_name` overrides on mobs charmed by
/// other players (so they fall back to following their own master).
///
/// Pets (mobs whose `pet_owner == player_name`) are intentionally skipped —
/// the bond is permanent and survives logout, so the buff and stay/follow
/// overrides are preserved untouched.
pub fn break_all_charms_by_player(db: &Db, player_name: &str) {
    if player_name.is_empty() {
        return;
    }
    let Ok(mobiles) = db.list_all_mobiles() else {
        return;
    };
    for mut mobile in mobiles {
        if mobile.is_prototype {
            continue;
        }
        let is_pet_of_player = mobile
            .pet_owner
            .as_deref()
            .map(|o| o.eq_ignore_ascii_case(player_name))
            .unwrap_or(false);
        if !is_pet_of_player {
            let mut changed = false;
            let before = mobile.active_buffs.len();
            // Drop Charmed AND Dominated — both grant the player full control
            // and should release on death/quit. Dominated is the
            // vampire Dominate discipline; semantics mirror Charmed for
            // lifecycle purposes.
            mobile.active_buffs.retain(|b| {
                !((b.effect_type == EffectType::Charmed || b.effect_type == EffectType::Dominated)
                    && b.source.eq_ignore_ascii_case(player_name))
            });
            if mobile.active_buffs.len() != before {
                mobile.charm_stay = false;
                mobile.charm_follow_player = None;
                changed = true;
            }
            if let Some(ref name) = mobile.charm_follow_player {
                if name.eq_ignore_ascii_case(player_name) {
                    mobile.charm_follow_player = None;
                    changed = true;
                }
            }
            if changed {
                let _ = db.save_mobile_data(mobile);
            }
        }
    }
}

/// Send one line to a character by name, if they are online.
fn notify(connections: &SharedConnections, char_name: &str, message: &str) {
    crate::script::achievements::send_to_player(connections, char_name, message);
}

/// Mirror the saved character into the live session so the client sees the new
/// room and HP without waiting for another tick, and keep MSDP coherent.
fn sync_to_session(connections: &SharedConnections, char: &CharacterData, state: &SharedState) {
    if let Ok(mut conns) = connections.lock() {
        for session in conns.values_mut() {
            let matches = session
                .character
                .as_ref()
                .is_some_and(|c| c.name.eq_ignore_ascii_case(&char.name));
            if matches {
                session.character = Some(char.clone());
                session.sync_msdp_vitals(state);
                return;
            }
        }
    }
}

/// Kill a player at `room_id`: drop a corpse holding their gear and gold,
/// reset their condition, and respawn them at their bound spawn room (or the
/// world's starting room).
///
/// **Every** route to a player's death funnels through here — a combat blow, a
/// bleedout expiry, a synth shutdown, a ROOM_DEATH trap — which is what makes
/// the `deaths` counter at the bottom countable exactly once.
///
/// The two writes after the respawn save (PvP worship credit and the death
/// counter) are out-of-band and must stay after it, for the standing reason:
/// the wholesale save would otherwise clobber them.
pub fn process_player_death(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    char: &mut CharacterData,
    room_id: &Uuid,
) -> Result<()> {
    let char_name = char.name.clone();

    // Snapshot engaged player attackers before combat state is cleared —
    // enemy-god worship credit for PvP kills is awarded after the respawn
    // save below.
    let pvp_attackers: Vec<String> = char
        .combat
        .targets
        .iter()
        .filter(|t| t.target_type == CombatTargetType::Player)
        .filter_map(|t| t.target_name.clone())
        .collect();

    // Release any mobiles this player had charmed.
    break_all_charms_by_player(db, &char_name);

    notify(connections, &char_name, "You have died!");
    broadcast_to_room(
        connections,
        *room_id,
        format!("{} has died!", char_name),
        Some(&char_name),
    );

    let mut corpse = crate::corpse::CorpseBuilder::for_player(&char_name, *room_id, char.gold as i64).build();
    let corpse_id = corpse.id;

    // Inventory and equipment both move into the corpse. The source of truth
    // for each is `ItemLocation`, so the move is a location rewrite.
    if let Ok(inventory_items) = db.get_items_in_inventory(&char_name) {
        for item in inventory_items {
            let item_id = item.id;
            let mut updated_item = item;
            updated_item.location = ItemLocation::Container(corpse_id);
            let _ = db.save_item_data(updated_item);
            corpse.container_contents.push(item_id);
        }
    }
    if let Ok(equipped_items) = db.get_equipped_items(&char_name) {
        for item in equipped_items {
            let item_id = item.id;
            let mut updated_item = item;
            updated_item.location = ItemLocation::Container(corpse_id);
            let _ = db.save_item_data(updated_item);
            corpse.container_contents.push(item_id);
        }
    }
    db.save_item_data(corpse)?;

    // Written down here so `locate corpse` is a lookup rather than a scan of
    // the whole item tree. This is the only place a player corpse is created,
    // and the character save below carries it.
    char.corpse_ids.push(corpse_id);

    // Gold moved onto the corpse with the items above.
    char.gold = 0;

    let spawn_room = char.spawn_room_id.unwrap_or_else(|| db.resolve_starting_room_id());
    char.current_room_id = spawn_room;
    char.hp = (char.max_hp / 4).max(1);
    char.is_unconscious = false;
    char.bleedout_rounds_remaining = 0;
    char.wounds.clear();
    char.ongoing_effects.clear();
    // A respawned synth boots clean: no shutdown countdown, no malfunction
    // flags (the chassis tick re-derives stages from the new HP).
    if let Some(s) = char.synth_state.as_mut() {
        crate::synth::reset_synth_state_on_death(s);
    }
    char.active_buffs
        .retain(|b| b.source != crate::types::SYNTH_MALFUNCTION_SOURCE);
    char.combat.in_combat = false;
    char.combat.targets.clear();
    char.combat.stun_rounds_remaining = 0;
    char.combat.ammo_depleted = 0;

    // Environmental and illness conditions all clear on respawn.
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
    sync_to_session(connections, char, state);

    notify(connections, &char_name, "You awaken at your spawn point...");
    notify(connections, &char_name, &format!("You have {} HP.", char.hp));
    broadcast_to_room(
        connections,
        spawn_room,
        format!("{} appears in a flash, gasping for breath.", char_name),
        Some(&char_name),
    );

    // Enemy-god worship credit for the killers. Runs after the victim's
    // save+sync above; the killers' own rounds load fresh state, so the favor
    // write isn't clobbered by end-of-round saves.
    for killer in pvp_attackers {
        crate::script::worship::handle_pvp_kill_credit(db, connections, &killer, &char_name);
    }

    // Death counter. Sits with the worship credit after the save+sync for the
    // same reason: the counter bump writes the character out-of-band, and the
    // wholesale save would otherwise overwrite it.
    crate::script::achievements::notify_counter_core(db, connections, state, &char_name, "deaths", 1);

    Ok(())
}

/// Backwards-compatible entry point for the ROOM_DEATH script path.
///
/// Kept as a wrapper rather than deleted because `apply_room_death` is the only
/// caller and its name says what it does at that site. The `connection_id_str`
/// parameter is gone: [`process_player_death`] messages by character name, so
/// the connection id was only ever a slower way to reach the same session.
pub fn kill_player_at_room(
    db: &Arc<Db>,
    connections: &SharedConnections,
    state: &SharedState,
    char: &mut CharacterData,
    room_id: &Uuid,
) -> Result<()> {
    process_player_death(db, connections, state, char, room_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> (Db, tempfile::TempDir) {
        let temp = tempfile::tempdir().expect("temp dir");
        let db = Db::open(temp.path()).expect("open db");
        (db, temp)
    }

    fn victim_at(db: &Db, name: &str, room: Uuid) -> CharacterData {
        let mut ch: CharacterData = serde_json::from_value(serde_json::json!({
            "name": name,
            "password_hash": "",
            "current_room_id": room,
        }))
        .expect("build character");
        ch.max_hp = 100;
        ch.hp = 0;
        ch.spawn_room_id = Some(room);
        db.save_character_data(ch.clone()).expect("save");
        ch
    }

    fn deaths(db: &Db, name: &str) -> u32 {
        db.get_character_data(name)
            .expect("read")
            .expect("exists")
            .achievement_counters
            .get("deaths")
            .copied()
            .unwrap_or(0)
    }

    /// The bug that made unifying the two death paths worth doing.
    ///
    /// `kill_player_at_room` is the ROOM_DEATH route — a death trap, a lethal
    /// room. It used to be a separate near-copy that never bumped the `deaths`
    /// counter, so a trap kill did not count as dying anywhere the counter is
    /// read: `top`, achievements, `status`.
    #[test]
    fn a_room_death_counts_as_a_death() {
        let (db, _temp) = temp_db();
        let room = Uuid::new_v4();
        let mut ch = victim_at(&db, "trapped", room);
        let conns: SharedConnections = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        let state = crate::World::minimal_shared(db.clone(), conns.clone());

        kill_player_at_room(&std::sync::Arc::new(db.clone()), &conns, &state, &mut ch, &room).expect("death resolves");

        assert_eq!(deaths(&db, "trapped"), 1, "a trap kill is a death");
    }

    /// Both entry points reach the same implementation, so both count.
    #[test]
    fn the_two_entry_points_agree() {
        let (db, _temp) = temp_db();
        let room = Uuid::new_v4();
        let conns: SharedConnections = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        let state = crate::World::minimal_shared(db.clone(), conns.clone());

        let mut a = victim_at(&db, "direct", room);
        process_player_death(&db, &conns, &state, &mut a, &room).expect("death resolves");
        assert_eq!(deaths(&db, "direct"), 1);

        let mut b = victim_at(&db, "wrapped", room);
        kill_player_at_room(&std::sync::Arc::new(db.clone()), &conns, &state, &mut b, &room).expect("death resolves");
        assert_eq!(deaths(&db, "wrapped"), 1);
    }

    /// The corpse takes the gold and the gear, and the player respawns with a
    /// quarter of their health and none of their money.
    #[test]
    fn death_moves_gold_and_gear_to_the_corpse() {
        let (db, _temp) = temp_db();
        let room = Uuid::new_v4();
        let mut ch = victim_at(&db, "looted", room);
        ch.gold = 137;
        db.save_character_data(ch.clone()).expect("save");

        let mut item = crate::types::ItemData::new("Dagger".into(), "a rusty dagger".into(), String::new());
        item.location = ItemLocation::Inventory("looted".to_string());
        let item_id = item.id;
        db.save_item_data(item).expect("save item");

        let conns: SharedConnections = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        let state = crate::World::minimal_shared(db.clone(), conns.clone());
        process_player_death(&db, &conns, &state, &mut ch, &room).expect("death resolves");

        assert_eq!(ch.gold, 0, "gold leaves with the corpse");
        assert_eq!(ch.hp, 25, "respawn at a quarter of max");

        let corpse = db
            .get_items_in_room(&room)
            .expect("read room")
            .into_iter()
            .find(|i| i.flags.is_corpse)
            .expect("a corpse was dropped");
        assert_eq!(corpse.flags.corpse_gold, 137);
        assert!(corpse.flags.corpse_is_player);
        assert!(corpse.container_contents.contains(&item_id), "the dagger went with it");
    }
}

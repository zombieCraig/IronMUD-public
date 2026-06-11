//! Rage-effect target acquisition for players.
//!
//! A player holding an `EffectType::Rage` buff who is not already fighting
//! force-engages a random target in their room each combat round: any
//! attackable mobile in non-safe zones, plus other players where PvP is
//! allowed. Rage removes the choice to fight, not the world's combat
//! gating — safe zones still suppress it entirely and players are only
//! eligible targets in PvP zones (mirrors attack.rhai's zone rules).
//!
//! Mobile-side rage lives in the wander tick's aggression scan
//! (`find_aggression_target_for_mob` treats a raging mob as aggressive).
//! Rage adds no damage bonus; pair with `Frenzy` for a full berserk.

use anyhow::Result;
use rand::Rng;

use ironmud::{
    CharacterData, CharacterPosition, CombatDistance, CombatTarget, CombatTargetType, CombatZoneType, EffectType,
    MobileData, SharedConnections, SharedState, db,
};

use crate::ticks::broadcast::{
    broadcast_to_room_except_awake, broadcast_to_room_except_awake_per_viewer, send_message_to_character,
    sync_character_to_session,
};
use crate::ticks::mobile::find_players_in_room;

/// A target the rage pass can force-engage.
enum RageTarget {
    Mobile(MobileData),
    Player(String),
}

fn has_rage_buff(char: &CharacterData) -> bool {
    char.active_buffs.iter().any(|b| b.effect_type == EffectType::Rage)
}

/// Force-engage a target for every online raging player not already in
/// combat. Runs at the top of each combat round so a fresh engagement
/// swings in the same round.
pub(super) fn process_rage_acquisitions(
    db: &db::Db,
    connections: &SharedConnections,
    state: &SharedState,
) -> Result<()> {
    // Snapshot candidates without holding the connections lock through the
    // DB work below.
    let raging: Vec<String> = {
        let conns = match connections.lock() {
            Ok(c) => c,
            Err(_) => return Ok(()),
        };
        conns
            .values()
            .filter_map(|s| s.character.as_ref())
            .filter(|c| {
                c.creation_complete
                    && !c.god_mode
                    && !c.is_unconscious
                    && c.position != CharacterPosition::Sleeping
                    && !c.combat.in_combat
                    && has_rage_buff(c)
            })
            .map(|c| c.name.clone())
            .collect()
    };

    for name in raging {
        let mut char = match db.get_character_data(&name)? {
            Some(c) => c,
            None => continue,
        };
        if char.combat.in_combat || !has_rage_buff(&char) {
            continue;
        }
        let room_id = char.current_room_id;
        if ironmud::check_build_mode(db, &name, &room_id) {
            continue;
        }
        let zone = ironmud::script::effective_combat_zone(db, &room_id);
        let players_here = find_players_in_room(connections, &room_id);
        let mut candidates = collect_rage_candidates(db, &char, zone, &players_here);
        if candidates.is_empty() {
            continue;
        }
        let idx = rand::thread_rng().gen_range(0..candidates.len());
        let target = candidates.swap_remove(idx);
        engage_rage_target(db, connections, state, &mut char, target)?;
    }
    Ok(())
}

/// Everything a raging `char` could lunge at. Safe zones yield nothing;
/// players are eligible only in PvP zones. Split out of the tick loop so
/// the eligibility rules are unit-testable without sessions.
fn collect_rage_candidates(
    db: &db::Db,
    char: &CharacterData,
    zone: CombatZoneType,
    players_in_room: &[String],
) -> Vec<RageTarget> {
    if zone == CombatZoneType::Safe {
        return Vec::new();
    }
    let room_id = char.current_room_id;
    let mut candidates: Vec<RageTarget> = Vec::new();

    if let Ok(mobs) = db.get_mobiles_in_room(&room_id) {
        for m in mobs {
            if m.is_prototype || m.current_hp <= 0 || m.is_unconscious || m.flags.no_attack {
                continue;
            }
            candidates.push(RageTarget::Mobile(m));
        }
    }

    if zone == CombatZoneType::Pvp {
        for other in players_in_room {
            if other.eq_ignore_ascii_case(&char.name) {
                continue;
            }
            let Ok(Some(oc)) = db.get_character_data(other) else {
                continue;
            };
            if oc.god_mode || oc.is_unconscious || ironmud::check_build_mode(db, other, &room_id) {
                continue;
            }
            candidates.push(RageTarget::Player(oc.name));
        }
    }

    candidates
}

fn engage_rage_target(
    db: &db::Db,
    connections: &SharedConnections,
    state: &SharedState,
    char: &mut CharacterData,
    target: RageTarget,
) -> Result<()> {
    let room_id = char.current_room_id;
    match target {
        RageTarget::Mobile(mob) => {
            char.combat.in_combat = true;
            if !char.combat.targets.iter().any(|t| t.target_id == mob.id) {
                char.combat.targets.push(CombatTarget::mobile(mob.id));
            }
            db.save_character_data(char.clone())?;
            sync_character_to_session(connections, char, state);

            // The mob fights back (same shape as the helper-join engage).
            let player_target_id = uuid::Uuid::nil();
            let _ = db.update_mobile(&mob.id, |m| {
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

            send_message_to_character(
                connections,
                &char.name,
                &format!("\x1b[1;31mRage takes you. You hurl yourself at {}!\x1b[0m", mob.name),
            );
            broadcast_to_room_except_awake(
                connections,
                &room_id,
                &format!(
                    "{} howls with mindless rage and hurls themselves at {}!",
                    char.name, mob.name
                ),
                &char.name,
            );
        }
        RageTarget::Player(victim_name) => {
            char.combat.in_combat = true;
            if !char.combat.targets.iter().any(|t| t.is_player_named(&victim_name)) {
                char.combat.targets.push(CombatTarget::player(victim_name.clone()));
            }
            db.save_character_data(char.clone())?;
            sync_character_to_session(connections, char, state);

            // Mutual engagement so the victim's own round retaliates.
            if let Ok(Some(mut victim)) = db.get_character_data(&victim_name) {
                if !victim.combat.targets.iter().any(|t| t.is_player_named(&char.name)) {
                    victim.combat.in_combat = true;
                    victim.combat.targets.push(CombatTarget::player(char.name.clone()));
                    db.save_character_data(victim.clone())?;
                    sync_character_to_session(connections, &victim, state);
                }
            }

            send_message_to_character(
                connections,
                &char.name,
                &format!("\x1b[1;31mRage takes you. You hurl yourself at {}!\x1b[0m", victim_name),
            );
            let attacker_name = char.name.clone();
            let victim_msg_name = victim_name.clone();
            broadcast_to_room_except_awake_per_viewer(connections, &room_id, &char.name, move |viewer| {
                if viewer.name.eq_ignore_ascii_case(&victim_msg_name) {
                    format!(
                        "\x1b[1;31m{} howls with mindless rage and hurls themselves at YOU!\x1b[0m",
                        attacker_name
                    )
                } else {
                    format!(
                        "{} howls with mindless rage and hurls themselves at {}!",
                        attacker_name, victim_msg_name
                    )
                }
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn mk_char(db: &db::Db, name: &str, room: Uuid) -> CharacterData {
        let mut char: CharacterData = serde_json::from_value(serde_json::json!({
            "name": name,
            "password_hash": "",
            "current_room_id": room,
        }))
        .expect("build character");
        char.position = CharacterPosition::Standing;
        db.save_character_data(char.clone()).expect("save char");
        char
    }

    fn mk_mobile(db: &db::Db, name: &str, room: Uuid) -> MobileData {
        let mut m = MobileData::new(name.to_string());
        m.is_prototype = false;
        m.current_room_id = Some(room);
        db.save_mobile_data(m.clone()).expect("save mobile");
        m
    }

    fn run_with_db(body: impl FnOnce(&db::Db)) {
        let temp = tempfile::tempdir().expect("create temp dir");
        let db = db::Db::open(temp.path()).expect("open db");
        body(&db);
    }

    #[test]
    fn rage_targets_mobile_in_pve_zone() {
        run_with_db(|db| {
            let room = Uuid::new_v4();
            let mob = mk_mobile(db, "alley rat", room);
            let char = mk_char(db, "ragetest", room);

            let candidates = collect_rage_candidates(db, &char, CombatZoneType::Pve, &[]);
            assert_eq!(candidates.len(), 1);
            assert!(matches!(&candidates[0], RageTarget::Mobile(m) if m.id == mob.id));
        });
    }

    #[test]
    fn rage_skips_no_attack_prototype_and_dead_mobiles() {
        run_with_db(|db| {
            let room = Uuid::new_v4();
            let mut protected = mk_mobile(db, "shopkeeper", room);
            protected.flags.no_attack = true;
            db.save_mobile_data(protected).expect("save");
            let mut proto = mk_mobile(db, "blueprint", room);
            proto.is_prototype = true;
            db.save_mobile_data(proto).expect("save");
            let mut dead = mk_mobile(db, "corpse-to-be", room);
            dead.current_hp = 0;
            db.save_mobile_data(dead).expect("save");
            let char = mk_char(db, "ragetest", room);

            let candidates = collect_rage_candidates(db, &char, CombatZoneType::Pve, &[]);
            assert!(candidates.is_empty(), "no eligible targets should remain");
        });
    }

    #[test]
    fn rage_yields_nothing_in_safe_zone() {
        run_with_db(|db| {
            let room = Uuid::new_v4();
            mk_mobile(db, "alley rat", room);
            let char = mk_char(db, "ragetest", room);

            let candidates = collect_rage_candidates(db, &char, CombatZoneType::Safe, &[]);
            assert!(candidates.is_empty(), "safe zones suppress rage entirely");
        });
    }

    #[test]
    fn rage_ignores_players_outside_pvp_zone() {
        run_with_db(|db| {
            let room = Uuid::new_v4();
            mk_char(db, "bystander", room);
            let char = mk_char(db, "ragetest", room);

            let candidates = collect_rage_candidates(db, &char, CombatZoneType::Pve, &["bystander".to_string()]);
            assert!(candidates.is_empty(), "players are not rage targets outside PvP zones");
        });
    }

    #[test]
    fn rage_targets_players_in_pvp_zone_but_never_self_or_gods() {
        run_with_db(|db| {
            let room = Uuid::new_v4();
            mk_char(db, "bystander", room);
            let mut god = mk_char(db, "immortal", room);
            god.god_mode = true;
            db.save_character_data(god).expect("save");
            let char = mk_char(db, "ragetest", room);

            let players = vec!["bystander".to_string(), "immortal".to_string(), "ragetest".to_string()];
            let candidates = collect_rage_candidates(db, &char, CombatZoneType::Pvp, &players);
            assert_eq!(candidates.len(), 1);
            assert!(matches!(&candidates[0], RageTarget::Player(n) if n == "bystander"));
        });
    }
}

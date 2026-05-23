//! Vampire tick processing — testable lib-side. The thin tokio loop wrapper
//! lives in `src/ticks/vampire.rs` (bin-only) and just calls these.
//!
//! Two ticks share this module because they iterate the same kindred
//! population. Both are public so integration tests in `tests/` can drive
//! them directly without spinning up the runtime loop.

use anyhow::Result;
use crate::types::ActiveBuff;
use crate::types::EffectType;
use crate::types::MobileData;
use crate::SharedConnections;
use crate::db;

/// Sun-exposure tick interval. 30s — fast enough that vampires can't sneak a
/// quick errand outdoors without consequence, slow enough that the tick scan
/// is cheap.
pub const SUN_TICK_INTERVAL_SECS: u64 = 30;

/// Blood-pool decay tick interval. Mirrors thirst's 60s cadence so a vampire
/// running disciplines feels parallel to a mortal getting thirsty.
pub const BLOOD_TICK_INTERVAL_SECS: u64 = 60;

const SUN_BURN_HP_DIVISOR: i32 = 20;
const MIN_SUN_BURN_DAMAGE: i32 = 1;
const BLOOD_DECAY_PER_TICK: i32 = 1;

/// Apply sun damage to every exposed kindred (PC + mob).
///
/// Rescue window: when a vampire would die from sun damage, they instead
/// drop to 1 HP with the SunlightBurning buff stamped — they're prone,
/// alive, and one more tick (or any combat blow) will end them. If an ally
/// drags them to a sheltered room before the next sun tick, the buff is
/// cleared and they wake injured. Subsequent ticks while still exposed and
/// already burning are lethal.
///
/// Returns the IDs of mobs whose HP reached 0 this tick. The caller is
/// expected to finish the death pipeline (corpse, inventory drop, spawn
/// cleanup) via `process_mobile_death` — which lives bin-side and can't
/// be called from here.
pub fn process_sun_tick(
    db: &db::Db,
    connections: &SharedConnections,
) -> Result<Vec<uuid::Uuid>> {
    let mut mob_deaths: Vec<uuid::Uuid> = Vec::new();
    let game_time = db.get_game_time()?;
    if !game_time.is_daytime() {
        // Vampires sheltered from the sun (but still SunlightBurning from a
        // prior daytime exposure) get the rescue benefit at nightfall too.
        clear_burning_when_safe(db, connections)?;
        return Ok(mob_deaths);
    }

    {
        let mut conns = connections.lock().unwrap();
        for (_conn_id, session) in conns.iter_mut() {
            let ch = match session.character.as_mut() {
                Some(c) => c,
                None => continue,
            };
            if !ch.creation_complete || ch.god_mode {
                continue;
            }
            if ch.vampire_state.is_none() {
                continue;
            }
            let exposed = is_room_exposed(db, &Some(ch.current_room_id));
            let already_burning = has_buff(&ch.active_buffs, EffectType::SunlightBurning);
            if !exposed {
                if already_burning {
                    remove_buff(&mut ch.active_buffs, EffectType::SunlightBurning);
                    let _ = session.sender.send(
                        "\n\x1b[1;33mYou drag yourself into shadow. The smoke fades. You will live — for now.\x1b[0m\n"
                            .to_string(),
                    );
                    let _ = db.save_character_data(ch.clone());
                }
                continue;
            }

            // Thinbloods take half sun damage (integer division — small
            // magnitudes harmlessly round to 0). Lifts on clan acknowledgment.
            let mut dmg = sun_damage_amount(ch.max_hp);
            if crate::script::vampire::is_pc_thinblood(ch) {
                dmg /= 2;
            }
            if dmg <= 0 {
                continue;
            }
            apply_sun_damage_with_rescue(
                &mut ch.hp,
                &mut ch.active_buffs,
                dmg,
                already_burning,
            );
            let now_burning = has_buff(&ch.active_buffs, EffectType::SunlightBurning);
            let msg = if ch.hp == 0 {
                format!(
                    "\n\x1b[1;31mThe sunlight finishes you. Your unliving flesh blackens, splits, ends.\x1b[0m\n"
                )
            } else if now_burning && !already_burning {
                format!(
                    "\n\x1b[1;31mThe sun ignites your dead flesh! You collapse, smoke pouring from your skin. ONE MORE MOMENT IN THE LIGHT AND YOU END. ({} dmg)\x1b[0m\n",
                    dmg
                )
            } else {
                format!(
                    "\n\x1b[33mDirect sunlight sears your unliving flesh — {} damage. Find shade!\x1b[0m\n",
                    dmg
                )
            };
            let _ = session.sender.send(msg);
            let _ = db.save_character_data(ch.clone());
        }
    }

    for mob in db.list_all_mobiles()? {
        if mob.is_prototype {
            continue;
        }
        if mob.vampire_state.is_none() && !mob.flags.vampire {
            continue;
        }
        let room_id = match mob.current_room_id {
            Some(r) => r,
            None => continue,
        };
        let exposed = is_room_exposed(db, &Some(room_id));
        let already_burning = has_buff(&mob.active_buffs, EffectType::SunlightBurning);
        let mut mob: MobileData = mob;
        if !exposed {
            if already_burning {
                remove_buff(&mut mob.active_buffs, EffectType::SunlightBurning);
                let _ = db.save_mobile_data(mob);
            }
            continue;
        }
        let dmg = sun_damage_amount(mob.max_hp);
        apply_sun_damage_with_rescue(
            &mut mob.current_hp,
            &mut mob.active_buffs,
            dmg,
            already_burning,
        );
        let died = mob.current_hp == 0;
        let mob_id = mob.id;
        let _ = db.save_mobile_data(mob);
        if died {
            mob_deaths.push(mob_id);
        }
    }

    Ok(mob_deaths)
}

/// At night (or when daytime exposure ends), clear SunlightBurning from
/// anyone still carrying it. Called once per non-daytime sun tick so a
/// vampire dragged out of the sun isn't stuck waiting for noon to recover.
fn clear_burning_when_safe(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    {
        let mut conns = connections.lock().unwrap();
        for (_conn_id, session) in conns.iter_mut() {
            let ch = match session.character.as_mut() {
                Some(c) => c,
                None => continue,
            };
            if has_buff(&ch.active_buffs, EffectType::SunlightBurning) {
                remove_buff(&mut ch.active_buffs, EffectType::SunlightBurning);
                let _ = session.sender.send(
                    "\n\x1b[1;33mNightfall. The burning fades from your skin. You survived another day.\x1b[0m\n"
                        .to_string(),
                );
                let _ = db.save_character_data(ch.clone());
            }
        }
    }
    for mob in db.list_all_mobiles()? {
        if mob.is_prototype {
            continue;
        }
        if !has_buff(&mob.active_buffs, EffectType::SunlightBurning) {
            continue;
        }
        let mut mob = mob;
        remove_buff(&mut mob.active_buffs, EffectType::SunlightBurning);
        let _ = db.save_mobile_data(mob);
    }
    Ok(())
}

/// Compute the rescue outcome when a sun-damage hit would land. Returns the
/// new HP and whether SunlightBurning should be set. Damage that wouldn't
/// drop the target to 0 leaves the rescue state alone.
fn apply_sun_damage_with_rescue(
    hp: &mut i32,
    buffs: &mut Vec<ActiveBuff>,
    dmg: i32,
    already_burning: bool,
) {
    push_or_refresh_buff(buffs, EffectType::SunlightBurn, dmg, "sunlight");

    let raw = *hp - dmg;
    if raw > 0 {
        *hp = raw;
        return;
    }
    // Damage would drop them to 0 or below.
    if already_burning {
        // Second sun tick while burning: lethal.
        *hp = 0;
        return;
    }
    // First time hitting 0: rescue window opens. Floor at 1 and stamp
    // SunlightBurning. The next tick (or any combat blow) finishes them.
    *hp = 1;
    push_or_refresh_buff(buffs, EffectType::SunlightBurning, 1, "sunlight");
}

/// Decay blood pool by 1 per tick on every kindred (PC + mob).
pub fn process_blood_tick(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    {
        let mut conns = connections.lock().unwrap();
        for (_conn_id, session) in conns.iter_mut() {
            let ch = match session.character.as_mut() {
                Some(c) => c,
                None => continue,
            };
            if !ch.creation_complete || ch.god_mode {
                continue;
            }
            let v = match ch.vampire_state.as_mut() {
                Some(v) => v,
                None => continue,
            };
            v.blood_pool = (v.blood_pool - BLOOD_DECAY_PER_TICK).max(0);
            v.last_blood_tick = now;
            let _ = db.save_character_data(ch.clone());
        }
    }

    for mob in db.list_all_mobiles()? {
        if mob.is_prototype {
            continue;
        }
        if mob.vampire_state.is_none() {
            continue;
        }
        let mut mob: MobileData = mob;
        if let Some(v) = mob.vampire_state.as_mut() {
            v.blood_pool = (v.blood_pool - BLOOD_DECAY_PER_TICK).max(0);
            v.last_blood_tick = now;
        }
        let _ = db.save_mobile_data(mob);
    }

    Ok(())
}

fn sun_damage_amount(max_hp: i32) -> i32 {
    (max_hp / SUN_BURN_HP_DIVISOR).max(MIN_SUN_BURN_DAMAGE)
}

/// True when the given room exposes a kindred to sunlight. Reuses the
/// existing `RoomFlags.indoors` flag — buildings, caves, deep interiors all
/// count as sheltered. Future refinement may add a `sheltered` flag to
/// distinguish "bright atrium" from "cellar".
fn is_room_exposed(db: &db::Db, room_id: &Option<uuid::Uuid>) -> bool {
    let room_uuid = match room_id {
        Some(r) => r,
        None => return false,
    };
    let room = match db.get_room_data(room_uuid) {
        Ok(Some(r)) => r,
        _ => return false,
    };
    !room.flags.indoors
}

fn has_buff(buffs: &[ActiveBuff], effect_type: EffectType) -> bool {
    buffs.iter().any(|b| b.effect_type == effect_type)
}

fn remove_buff(buffs: &mut Vec<ActiveBuff>, effect_type: EffectType) {
    buffs.retain(|b| b.effect_type != effect_type);
}

fn push_or_refresh_buff(
    buffs: &mut Vec<ActiveBuff>,
    effect_type: EffectType,
    magnitude: i32,
    source: &str,
) {
    if let Some(existing) = buffs.iter_mut().find(|b| b.effect_type == effect_type) {
        existing.magnitude = magnitude;
        existing.remaining_secs = (SUN_TICK_INTERVAL_SECS * 2) as i32;
        existing.source = source.to_string();
        return;
    }
    buffs.push(ActiveBuff {
        effect_type,
        magnitude,
        remaining_secs: (SUN_TICK_INTERVAL_SECS * 2) as i32,
        source: source.to_string(),
        damage_type: None,
        vs_effect: None,
        skill_key: None,
    });
}

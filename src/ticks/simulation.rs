//! NPC Needs Simulation tick system for IronMUD
//!
//! Implements a Sims/Dwarf Fortress-style needs simulation where NPCs have
//! internal needs (hunger, energy, comfort) that decay over time, driving
//! autonomous decision-making. The simulation tick sets goals and destinations;
//! the existing wander tick handles actual movement via BFS pathfinding.

use anyhow::Result;
use rand::Rng;
use tokio::time::{Duration, interval};
use tracing::{debug, error, warn};

use uuid::Uuid;

use ironmud::{
    ActivityState, ItemData, ItemType, MobileData, NeedsState, SharedConnections, SimGoal, SimulationConfig, db,
};

use super::broadcast::broadcast_to_room_awake;

/// Simulation tick interval in seconds (matches wander tick cadence)
pub const SIMULATION_TICK_INTERVAL_SECS: u64 = 60;

/// Minimum seconds between ambient emotes for a single NPC
const EMOTE_COOLDOWN_SECS: i64 = 120;

/// Background task that processes NPC needs simulation periodically
pub async fn run_simulation_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(SIMULATION_TICK_INTERVAL_SECS));

    loop {
        ticker.tick().await;

        if let Err(e) = process_simulation_tick(&db, &connections) {
            error!("Simulation tick error: {}", e);
        }
    }
}

/// Process simulation updates for all simulated mobiles
fn process_simulation_tick(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    let game_time = db.get_game_time()?;
    let current_hour = game_time.hour;
    let current_game_day = ironmud::migration::absolute_game_day(game_time.year, game_time.month, game_time.day) as i32;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let mobiles = db.list_all_mobiles()?;

    for mobile in mobiles {
        // Skip prototypes, dead mobiles, and those in combat
        if mobile.is_prototype || mobile.current_hp <= 0 || mobile.combat.in_combat {
            continue;
        }

        // Skip non-simulated mobiles
        if mobile.simulation.is_none() {
            continue;
        }

        // Re-fetch fresh data to avoid stale state
        let mut current_mobile = match db.get_mobile_data(&mobile.id)? {
            Some(m) => m,
            None => continue,
        };

        // Skip if combat state changed since listing
        if current_mobile.combat.in_combat {
            continue;
        }

        process_simulated_npc(
            db,
            connections,
            &mut current_mobile,
            current_hour,
            current_game_day,
            now,
        )?;

        // Commit via CAS so we don't clobber fields another tick owns
        // (current_room_id from wander, combat from combat tick, etc.).
        // The simulation tick legitimately owns needs / current_activity /
        // routine_destination_room / gold / current_hp — copy those computed
        // values onto whatever fresh copy is in the DB, re-running on
        // conflict. Side effects (broadcasts, item spawn/delete) already
        // ran once above; the closure below must stay pure.
        let owned_needs = current_mobile.needs.clone();
        let owned_activity = current_mobile.current_activity;
        let owned_destination = current_mobile.routine_destination_room;
        let owned_gold = current_mobile.gold;
        let owned_hp = current_mobile.current_hp;
        db.update_mobile(&current_mobile.id, |m| {
            m.needs = owned_needs.clone();
            m.current_activity = owned_activity.clone();
            m.routine_destination_room = owned_destination;
            m.gold = owned_gold;
            m.current_hp = owned_hp;
        })?;
    }

    Ok(())
}

/// Process a single simulated NPC through the full simulation cycle
fn process_simulated_npc(
    db: &db::Db,
    connections: &SharedConnections,
    mobile: &mut MobileData,
    current_hour: u8,
    current_game_day: i32,
    now: i64,
) -> Result<()> {
    let config = match mobile.simulation.as_ref() {
        Some(c) => c.clone(),
        None => return Ok(()),
    };

    // Take needs out of mobile to avoid borrow conflicts, then put it back at the end
    let mut needs = mobile.needs.take().unwrap_or_default();

    // Step 1: Decay needs
    decay_needs(&mut needs, &config, current_hour);

    // Step 2: Apply cascading consequences
    apply_consequences(mobile, &needs, connections);

    // Step 3: Run decision engine
    let new_goal = decide_goal(db, mobile, &config, &needs, current_hour, now)?;
    needs.current_goal = new_goal;

    // Step 4: Accumulate work wages for hours actually spent at work.
    // Runs after decide_goal so payment is based on this tick's goal state.
    check_work_payment(db, mobile, &config, &mut needs, current_hour, connections)?;

    // Step 5: Execute arrived-at-destination actions
    execute_arrival_actions(db, connections, mobile, &config, &mut needs, now)?;

    // Step 5a: Scavenger flavor — pick up loose items in non-shop, non-home rooms.
    maybe_scavenge_pickup(db, connections, mobile, &config)?;

    // Step 5b: Regenerate HP while resting at home
    regenerate_hp(db, mobile, &config, &needs)?;

    // Step 6: Set destination for wander tick to follow
    set_destination(db, mobile, &config, &needs)?;

    // Step 7: Update activity state for display
    update_activity_state(db, connections, mobile, &config, &needs)?;

    // Step 8: Emit ambient emotes
    emit_ambient_emotes(connections, mobile, &mut needs, now, current_game_day);

    // Step 9: Maybe converse with another simulated mobile in the same room.
    // Persists both sides' social state via CAS; the top-level CAS in
    // process_simulation_tick leaves social/active_buffs untouched so those
    // writes survive.
    maybe_converse(db, connections, mobile, now, current_game_day)?;

    needs.last_tick_hour = current_hour;

    // Put needs back into mobile
    mobile.needs = Some(needs);

    Ok(())
}

// ---------------------------------------------------------------------------
// Step 1: Need Decay
// ---------------------------------------------------------------------------

fn decay_needs(needs: &mut NeedsState, config: &SimulationConfig, _current_hour: u8) {
    let hunger_rate = if config.hunger_decay_rate > 0 {
        config.hunger_decay_rate
    } else {
        100
    };
    let energy_rate = if config.energy_decay_rate > 0 {
        config.energy_decay_rate
    } else {
        100
    };
    let comfort_rate = if config.comfort_decay_rate > 0 {
        config.comfort_decay_rate
    } else {
        100
    };

    // Base decay per tick
    let hunger_decay = (2 * hunger_rate / 100).max(1);
    let comfort_decay = (1 * comfort_rate / 100).max(1);

    needs.hunger = (needs.hunger - hunger_decay).max(0);
    needs.comfort = (needs.comfort - comfort_decay).max(0);

    // Energy: sleeping restores instead of decaying
    if needs.current_goal == SimGoal::SeekSleep {
        // Sleeping NPCs don't lose energy (restoration happens in arrival actions)
    } else {
        let energy_decay = (3 * energy_rate / 100).max(1);
        needs.energy = (needs.energy - energy_decay).max(0);
    }
}

// ---------------------------------------------------------------------------
// Step 2: Work Payment
// ---------------------------------------------------------------------------

/// Pay NPCs per game hour they're actually at work, instead of only at the
/// shift-end edge. The old edge-trigger paid 0 if the mobile wasn't at the work
/// room the exact tick the shift ended — a chronic problem for NPCs whose
/// hunger/energy loops keep them out of the workplace most of the shift.
///
/// This runs every tick; pay fires when the *game hour* rolls over (twice per
/// real minute) and only if the mobile is Working at the configured work room.
fn check_work_payment(
    db: &db::Db,
    mobile: &mut MobileData,
    config: &SimulationConfig,
    needs: &mut NeedsState,
    current_hour: u8,
    connections: &SharedConnections,
) -> Result<()> {
    let was_work = is_within_work_hours(needs.last_tick_hour, config.work_start_hour, config.work_end_hour);
    let is_work = is_within_work_hours(current_hour, config.work_start_hour, config.work_end_hour);

    // Just entered work hours: reset paid flag (kept for compatibility)
    if !was_work && is_work {
        needs.paid_this_shift = false;
    }

    // Both wage paths fire on game-hour boundaries only; otherwise migrants
    // would earn a full hour's wage every tick (twice per real minute).
    let hour_changed = current_hour != needs.last_tick_hour;
    if !hour_changed {
        return Ok(());
    }

    // Path 1: configured-workplace wage. Pays for being Working at the
    // configured `work_room_vnum` during work hours.
    let mut paid_this_tick = false;
    if is_work && needs.current_goal == SimGoal::Working && is_at_room(db, mobile, &config.work_room_vnum)? {
        let hourly = hourly_wage(config.work_pay, config.work_start_hour, config.work_end_hour);
        if hourly > 0 {
            mobile.gold += hourly;
            needs.paid_this_shift = true;
            paid_this_tick = true;
            debug!(
                "Simulation: {} earned {} gold for this hour of work (total gold now {})",
                mobile.name, hourly, mobile.gold
            );
            if let Some(room_id) = mobile.current_room_id {
                broadcast_to_room_awake(
                    connections,
                    &room_id,
                    &format!("{} pockets an hour's wages.", mobile.name),
                );
            }
        }
    }

    // Path 2: role wages from the area treasury. Lets migrant guards / healers /
    // scavengers earn even when they have no `work_room_vnum` configured. Pays
    // anywhere in their resident area (scavenger excluded at home — they have
    // to actually be out scrounging). Does not double-pay if path 1 already fired.
    if !paid_this_tick {
        if let Some(amount) = role_hourly_wage(db, mobile)? {
            if amount > 0 {
                mobile.gold += amount;
                debug!(
                    "Simulation: {} earned {} gold in role wages (total gold now {})",
                    mobile.name, amount, mobile.gold
                );
                if let Some(room_id) = mobile.current_room_id {
                    broadcast_to_room_awake(
                        connections,
                        &room_id,
                        &format!("{} tucks away a few coins from the day's labor.", mobile.name),
                    );
                }
            }
        }
    }

    Ok(())
}

/// Hourly area-treasury wage for a migrant variation, or None if the mobile
/// doesn't qualify (no role flag, not in their resident area, scavenger at
/// home, or area has the wage set to 0). Read-only — does not mutate.
fn role_hourly_wage(db: &db::Db, mobile: &MobileData) -> Result<Option<i32>> {
    let resident_vnum = match mobile.resident_of.as_deref() {
        Some(v) if !v.is_empty() => v,
        _ => return Ok(None),
    };
    let home_room = match db.get_room_by_vnum(resident_vnum)? {
        Some(r) => r,
        None => return Ok(None),
    };
    let area_id = match home_room.area_id {
        Some(a) => a,
        None => return Ok(None),
    };
    let cur_room_id = match mobile.current_room_id {
        Some(id) => id,
        None => return Ok(None),
    };
    let cur_area = match db.get_room_data(&cur_room_id)? {
        Some(r) => r.area_id,
        None => None,
    };
    if cur_area != Some(area_id) {
        return Ok(None);
    }
    let area = match db.get_area_data(&area_id)? {
        Some(a) => a,
        None => return Ok(None),
    };
    if mobile.flags.guard && area.guard_wage_per_hour > 0 {
        return Ok(Some(area.guard_wage_per_hour));
    }
    if mobile.flags.healer && area.healer_wage_per_hour > 0 {
        return Ok(Some(area.healer_wage_per_hour));
    }
    if mobile.flags.scavenger && area.scavenger_wage_per_hour > 0 {
        // Scavengers only earn while out (not parked at their home room).
        if cur_room_id != home_room.id {
            return Ok(Some(area.scavenger_wage_per_hour));
        }
    }
    Ok(None)
}

fn is_within_work_hours(hour: u8, start: u8, end: u8) -> bool {
    if start <= end {
        hour >= start && hour < end
    } else {
        // Wraps midnight (e.g., start=22, end=6)
        hour >= start || hour < end
    }
}

fn shift_length_hours(start: u8, end: u8) -> u32 {
    if start <= end {
        (end.saturating_sub(start)) as u32
    } else {
        (24 - start as u32) + end as u32
    }
}

fn hourly_wage(work_pay: i32, start: u8, end: u8) -> i32 {
    let hours = shift_length_hours(start, end).max(1) as i32;
    (work_pay / hours).max(1)
}

// ---------------------------------------------------------------------------
// Step 3: Cascading Consequences
// ---------------------------------------------------------------------------

fn apply_consequences(mobile: &mut MobileData, needs: &NeedsState, connections: &SharedConnections) {
    // Starvation: hunger=0 and energy critically low
    if needs.hunger == 0 && needs.energy <= 20 {
        mobile.current_hp = (mobile.current_hp - 1).max(1);
        debug!(
            "Simulation: {} taking starvation damage (hp={})",
            mobile.name, mobile.current_hp
        );

        if let Some(room_id) = mobile.current_room_id {
            broadcast_to_room_awake(
                connections,
                &room_id,
                &format!("{} looks dangerously weak from hunger and exhaustion.", mobile.name),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Step 4: Decision Engine
// ---------------------------------------------------------------------------

fn decide_goal(
    db: &db::Db,
    mobile: &MobileData,
    config: &SimulationConfig,
    needs: &NeedsState,
    current_hour: u8,
    now: i64,
) -> Result<SimGoal> {
    let is_work = is_within_work_hours(current_hour, config.work_start_hour, config.work_end_hour);
    let has_food = db
        .get_items_in_mobile_inventory(&mobile.id)?
        .iter()
        .any(|i| i.item_type == ItemType::Food);
    // `can_obtain_food` is only used for the "interrupt work" branch below;
    // off-shift food-seeking is no longer gated on it because the home-relief
    // fallback (`try_home_relief`) lets a broke NPC still acquire food.
    let can_obtain_food = has_food || mobile.gold > 0;
    let work_room_set = !config.work_room_vnum.is_empty();

    // Critical exhaustion: always overrides (they'll collapse otherwise)
    if needs.energy <= 10 {
        return Ok(SimGoal::SeekSleep);
    }

    // Sleep hysteresis: once an NPC has committed to sleeping, keep sleeping
    // until they're genuinely rested. Without this the cycle is:
    //   energy=10 → SeekSleep → +20 at home → energy=30 → wake next tick →
    //   walk out → decay back to 10 → head home → repeat.
    // They never actually get to work. Threshold ~75 gives a full restful
    // cycle that comfortably lasts a 10-hour shift (65 energy @ 3/tick decay
    // ≈ 10.8 game hours before tiredness returns). Starvation (below) can
    // still interrupt a sleep.
    const SLEEP_WAKE_ENERGY: i32 = 75;
    if needs.current_goal == SimGoal::SeekSleep && needs.energy < SLEEP_WAKE_ENERGY {
        return Ok(SimGoal::SeekSleep);
    }

    // Starving: always try to eat, even during work
    if needs.hunger <= 5 && can_obtain_food {
        return Ok(SimGoal::SeekFood);
    }

    // Mood gate: deeply unhappy NPCs will skip their shift. Breakdown always
    // skips work; Depressed skips ~50% of the time (still a chance to push
    // through). Caller applies mood buffs separately.
    let mood = mobile
        .social
        .as_ref()
        .map(|s| s.mood)
        .unwrap_or(ironmud::MoodState::Normal);
    let mut rng = rand::thread_rng();
    let skip_work = match mood {
        ironmud::MoodState::Breakdown => true,
        ironmud::MoodState::Depressed => rng.gen_range(0..100) < 50,
        _ => false,
    };

    // During work hours, Work is the dominant goal. Only abandon the shift for
    // food if we're actually approaching starvation (<=15) AND we don't already
    // carry something to eat — otherwise arrival-actions will eat carried food
    // at the work room without interrupting the shift.
    if is_work && work_room_set && !skip_work {
        if needs.hunger <= 15 && !has_food && can_obtain_food {
            return Ok(SimGoal::SeekFood);
        }
        let at_work = is_at_room(db, mobile, &config.work_room_vnum)?;
        return Ok(if at_work {
            SimGoal::Working
        } else {
            SimGoal::GoingToWork
        });
    }

    // --- Off-shift priorities below ---

    // Tired enough to sleep
    if needs.energy <= 20 {
        return Ok(SimGoal::SeekSleep);
    }

    // Broke + jobless: head for a bank room in the area to pick up a handout.
    // Sits above the hunger check so a migrant with empty pockets visits the
    // bank first, then the next tick the SeekFood path can route them to a
    // shop instead of forcing them home for the charity/forage fallback.
    if mobile.gold <= 0
        && now - needs.last_bank_visit_attempt >= BANK_VISIT_COOLDOWN_SECS
        && is_jobless(db, mobile, config)?
        && find_bank_room_in_area(db, mobile)?.is_some()
    {
        return Ok(SimGoal::SeekBank);
    }

    // Hungry: always try to address it. `set_destination` + `execute_arrival_actions`
    // know how to route the NPC — to a shop if they can pay, otherwise home for the
    // charity / forage fallback in `try_home_relief`. The previous "broke + hungry
    // → sleep" catch-22 trapped migrants in a hallway-pacing loop, so it's gone.
    if needs.hunger <= 30 {
        return Ok(SimGoal::SeekFood);
    }

    if needs.comfort <= 20 {
        return Ok(SimGoal::SeekComfort);
    }

    // Off-shift, all needs satisfied: idle (wander tick handles movement)
    Ok(SimGoal::Idle)
}

fn is_at_room(db: &db::Db, mobile: &MobileData, vnum: &str) -> Result<bool> {
    if vnum.is_empty() {
        return Ok(false);
    }
    match db.get_room_by_vnum(vnum)? {
        Some(room) => Ok(mobile.current_room_id.map(|r| r == room.id).unwrap_or(false)),
        None => Ok(false),
    }
}

// ---------------------------------------------------------------------------
// Step 5: Arrival Actions
// ---------------------------------------------------------------------------

fn execute_arrival_actions(
    db: &db::Db,
    connections: &SharedConnections,
    mobile: &mut MobileData,
    config: &SimulationConfig,
    needs: &mut NeedsState,
    now: i64,
) -> Result<()> {
    match needs.current_goal {
        SimGoal::SeekFood => {
            // Prefer eating food we already carry; otherwise (re)visit a shop,
            // or — if we're broke and at home — attempt charity / forage relief.
            let inventory = db.get_items_in_mobile_inventory(&mobile.id)?;
            let food = inventory.iter().find(|i| i.item_type == ItemType::Food).cloned();

            if let Some(food_item) = food {
                if let Some(room_id) = mobile.current_room_id {
                    eat_food_item(db, connections, mobile, needs, config, &food_item, room_id)?;
                }
            } else {
                let at_assigned_shop = is_at_room(db, mobile, &config.shop_room_vnum)?;
                let here_has_food_shopkeeper = match mobile.current_room_id {
                    Some(rid) => {
                        let mobiles_here = db.get_mobiles_in_room(&rid)?;
                        let mut found = false;
                        for m in &mobiles_here {
                            if m.flags.shopkeeper && !m.is_prototype && shopkeeper_sells_food(db, m)? {
                                found = true;
                                break;
                            }
                        }
                        found
                    }
                    None => false,
                };
                if at_assigned_shop || here_has_food_shopkeeper {
                    try_buy_food(db, connections, mobile, config, needs)?;
                } else if is_at_room(db, mobile, &config.home_room_vnum)? {
                    // Broke + hungry + at home: roll the charity / forage fallback.
                    try_home_relief(db, connections, mobile, config, needs, now)?;
                }
            }
        }
        SimGoal::SeekSleep => {
            if is_at_room(db, mobile, &config.home_room_vnum)? {
                // Sleep: restore energy
                needs.energy = (needs.energy + 20).min(100);
                debug!("Simulation: {} sleeping, energy now {}", mobile.name, needs.energy);
            }
        }
        SimGoal::SeekComfort => {
            if is_at_room(db, mobile, &config.home_room_vnum)? {
                // Home comfort restoration
                needs.comfort = (needs.comfort + 15).min(100);
                debug!(
                    "Simulation: {} relaxing at home, comfort now {}",
                    mobile.name, needs.comfort
                );
            }
        }
        SimGoal::Idle => {
            // At home, off-duty: slow comfort recovery
            if is_at_room(db, mobile, &config.home_room_vnum)? {
                needs.comfort = (needs.comfort + 5).min(100);
            }
        }
        SimGoal::Working => {
            // While on shift at work, if we're getting peckish and happen to
            // carry food, eat it here rather than abandoning the shift.
            if needs.hunger <= 50 && is_at_room(db, mobile, &config.work_room_vnum)? {
                let inventory = db.get_items_in_mobile_inventory(&mobile.id)?;
                if let Some(food_item) = inventory.iter().find(|i| i.item_type == ItemType::Food).cloned() {
                    if let Some(room_id) = mobile.current_room_id {
                        eat_food_item(db, connections, mobile, needs, config, &food_item, room_id)?;
                    }
                }
            }
        }
        SimGoal::SeekBank => {
            // Pay out the handout only once we've actually arrived at a bank
            // room. Stamping the cooldown here (rather than at goal-pick) means
            // a migrant who can't reach the bank — locked door, etc. — keeps
            // retrying instead of waiting out the full cooldown.
            if let Some(room_id) = mobile.current_room_id {
                if let Some(room) = db.get_room_data(&room_id)? {
                    if room.flags.bank {
                        mobile.gold += BANK_RELIEF_AMOUNT;
                        needs.last_bank_visit_attempt = now;
                        broadcast_to_room_awake(
                            connections,
                            &room_id,
                            &format!("{} visits the bank and counts out a small handout.", mobile.name),
                        );
                        debug!(
                            "Simulation: {} received {} gold at bank (total gold now {})",
                            mobile.name, BANK_RELIEF_AMOUNT, mobile.gold
                        );
                        needs.current_goal = SimGoal::Idle;
                        mobile.routine_destination_room = None;
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

/// True if the shopkeeper's stock list contains at least one food prototype.
/// Used to filter out non-food shops (florists, tailors, bookshops, ...) when
/// a hungry NPC is hunting for somewhere to eat.
fn shopkeeper_sells_food(db: &db::Db, shopkeeper: &MobileData) -> Result<bool> {
    for vnum in &shopkeeper.shop_stock {
        if let Some(proto) = db.get_item_by_vnum(vnum)? {
            if proto.item_type == ItemType::Food {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Find another shopkeeper room in the mobile's current area that hasn't been
/// tried this hunger cycle. Only considers shopkeepers that actually stock food
/// — otherwise hungry NPCs cycle through florists/tailors that can never feed
/// them, and the destination may be in an unreachable room (e.g. `no_mob`),
/// causing routine BFS to fail. Returns None if no such room exists or if the
/// mobile's current room has no area.
fn find_next_shop_room(db: &db::Db, mobile: &MobileData, tried: &[Uuid]) -> Result<Option<Uuid>> {
    let current_room_id = match mobile.current_room_id {
        Some(id) => id,
        None => return Ok(None),
    };
    let current_room = match db.get_room_data(&current_room_id)? {
        Some(r) => r,
        None => return Ok(None),
    };
    let area_id = match current_room.area_id {
        Some(a) => a,
        None => return Ok(None),
    };

    for room in db.get_rooms_in_area(&area_id)? {
        if room.id == current_room_id || tried.contains(&room.id) {
            continue;
        }
        let mobiles_here = db.get_mobiles_in_room(&room.id)?;
        for m in &mobiles_here {
            if m.flags.shopkeeper && !m.is_prototype && shopkeeper_sells_food(db, m)? {
                return Ok(Some(room.id));
            }
        }
    }
    Ok(None)
}

/// Find a `flags.bank` room in the same area as the mobile's current room.
/// Used to route broke jobless migrants to a place where they can pick up a
/// handout. Skips `no_mob` rooms so we don't send the NPC into an unreachable
/// destination. Returns the first matching room id, or None.
fn find_bank_room_in_area(db: &db::Db, mobile: &MobileData) -> Result<Option<Uuid>> {
    let current_room_id = match mobile.current_room_id {
        Some(id) => id,
        None => return Ok(None),
    };
    let current_room = match db.get_room_data(&current_room_id)? {
        Some(r) => r,
        None => return Ok(None),
    };
    let area_id = match current_room.area_id {
        Some(a) => a,
        None => return Ok(None),
    };
    for room in db.get_rooms_in_area(&area_id)? {
        if room.flags.bank && !room.flags.no_mob {
            return Ok(Some(room.id));
        }
    }
    Ok(None)
}

/// True if the mobile has no income source: no configured `work_room_vnum` AND
/// no role wage *exists* in their resident area for any role flag they carry.
/// Note we test the area's wage settings directly (not `role_hourly_wage`) so a
/// scavenger who's currently parked at home — which suppresses pay this tick —
/// isn't briefly classified as jobless.
fn is_jobless(db: &db::Db, mobile: &MobileData, config: &SimulationConfig) -> Result<bool> {
    if !config.work_room_vnum.is_empty() {
        return Ok(false);
    }
    let resident_vnum = match mobile.resident_of.as_deref() {
        Some(v) if !v.is_empty() => v,
        _ => return Ok(true),
    };
    let home_room = match db.get_room_by_vnum(resident_vnum)? {
        Some(r) => r,
        None => return Ok(true),
    };
    let area_id = match home_room.area_id {
        Some(a) => a,
        None => return Ok(true),
    };
    let area = match db.get_area_data(&area_id)? {
        Some(a) => a,
        None => return Ok(true),
    };
    if mobile.flags.guard && area.guard_wage_per_hour > 0 {
        return Ok(false);
    }
    if mobile.flags.healer && area.healer_wage_per_hour > 0 {
        return Ok(false);
    }
    if mobile.flags.scavenger && area.scavenger_wage_per_hour > 0 {
        return Ok(false);
    }
    Ok(true)
}

/// Apply the effects of eating a food item: hunger restoration, optional comfort
/// bonus when the item matches the mobile's preferred food, and cleanup. Resets
/// the failed-shop tracker since the hunger cycle has ended successfully.
fn eat_food_item(
    db: &db::Db,
    connections: &SharedConnections,
    mobile: &MobileData,
    needs: &mut NeedsState,
    config: &SimulationConfig,
    item: &ItemData,
    room_id: Uuid,
) -> Result<()> {
    let nutrition = effective_nutrition(item.food_nutrition);
    needs.hunger = (needs.hunger + nutrition).min(100);
    needs.tried_shops_this_cycle.clear();

    let is_preferred =
        !config.preferred_food_vnum.is_empty() && item.vnum.as_deref() == Some(config.preferred_food_vnum.as_str());
    if is_preferred {
        needs.comfort = (needs.comfort + 12).min(100);
        broadcast_to_room_awake(
            connections,
            &room_id,
            &format!("{} savors {} — their favorite!", mobile.name, item.name),
        );
        debug!(
            "Simulation: {} ate preferred food {}, hunger now {}, comfort now {}",
            mobile.name, item.name, needs.hunger, needs.comfort
        );
    } else {
        broadcast_to_room_awake(
            connections,
            &room_id,
            &format!("{} eats {} contentedly.", mobile.name, item.name),
        );
        debug!("Simulation: {} ate food, hunger now {}", mobile.name, needs.hunger);
    }
    db.delete_item(&item.id)?;
    Ok(())
}

fn try_buy_food(
    db: &db::Db,
    connections: &SharedConnections,
    mobile: &mut MobileData,
    config: &SimulationConfig,
    needs: &mut NeedsState,
) -> Result<()> {
    let room_id = match mobile.current_room_id {
        Some(id) => id,
        None => return Ok(()),
    };

    // Find shopkeeper in this room and snapshot the data we need so we don't hold
    // a borrow into `mobiles_here` across later mutations of `mobile`.
    let mobiles_here = db.get_mobiles_in_room(&room_id)?;
    let (sell_rate, stock) = match mobiles_here.iter().find(|m| m.flags.shopkeeper && !m.is_prototype) {
        Some(sk) => (sk.shop_sell_rate, sk.shop_stock.clone()),
        None => return Ok(()),
    };

    // Choose what to buy:
    //   1. preferred food, if the shop actually stocks it and we can afford it
    //   2. otherwise the most-nutritious food we can afford from this shop
    let preferred_affordable =
        if !config.preferred_food_vnum.is_empty() && stock.iter().any(|v| v == &config.preferred_food_vnum) {
            match db.get_item_by_vnum(&config.preferred_food_vnum)? {
                Some(p) if p.item_type == ItemType::Food && mobile.gold >= food_price(p.value, sell_rate) => true,
                _ => false,
            }
        } else {
            false
        };

    let chosen_vnum: Option<String> = if preferred_affordable {
        Some(config.preferred_food_vnum.clone())
    } else {
        find_best_affordable_food(db, &stock, sell_rate, mobile.gold)?
    };

    let food_vnum = match chosen_vnum {
        Some(v) => v,
        None => {
            // Nothing affordable here. Try the next shop in this area; only give
            // up shopping once we've exhausted every shop this hunger cycle.
            if !needs.tried_shops_this_cycle.contains(&room_id) {
                needs.tried_shops_this_cycle.push(room_id);
            }
            if let Some(other_room) = find_next_shop_room(db, mobile, &needs.tried_shops_this_cycle)? {
                mobile.routine_destination_room = Some(other_room);
                debug!(
                    "Simulation: {} can't afford anything here, walking to next shop",
                    mobile.name
                );
                return Ok(());
            }
            // All shops tried and nothing affordable. Head home so `try_home_relief`
            // can attempt charity / forage. Goal stays SeekFood so the arrival
            // handler runs the relief path; tried_shops_this_cycle resets only on
            // a successful eat (so we don't bounce back into the same loop).
            broadcast_to_room_awake(
                connections,
                &room_id,
                &format!("{} counts their coins and looks disappointed.", mobile.name),
            );
            debug!(
                "Simulation: {} can't afford food at any shop (has {} gold), heading home",
                mobile.name, mobile.gold
            );
            mobile.routine_destination_room = match db.get_room_by_vnum(&config.home_room_vnum)? {
                Some(r) => Some(r.id),
                None => None,
            };
            return Ok(());
        }
    };

    let food_proto = match db.get_item_by_vnum(&food_vnum)? {
        Some(p) => p,
        None => return Ok(()),
    };
    let price = food_price(food_proto.value, sell_rate);
    mobile.gold -= price;

    let spawned = match db.spawn_item_from_prototype(&food_vnum)? {
        Some(item) => item,
        None => return Ok(()),
    };
    db.move_item_to_mobile_inventory(&spawned.id, &mobile.id)?;
    broadcast_to_room_awake(
        connections,
        &room_id,
        &format!("{} buys {}.", mobile.name, spawned.name),
    );
    debug!(
        "Simulation: {} bought food for {} gold (has {} left)",
        mobile.name, price, mobile.gold
    );

    if needs.hunger <= 50 {
        eat_food_item(db, connections, mobile, needs, config, &spawned, room_id)?;
    }

    Ok(())
}

/// Minimum real-time seconds between consecutive home-relief attempts for one
/// NPC. Prevents the charity/forage fallback from firing every tick when the
/// mobile is parked at home with hunger still under threshold.
const HOME_RELIEF_COOLDOWN_SECS: i64 = 60;
/// Cohabitant must have at least this much gold before they'll lend a hand.
const COHABITANT_CHARITY_RESERVE: i32 = 20;
/// How much gold a willing cohabitant transfers when charity fires.
const COHABITANT_CHARITY_AMOUNT: i32 = 10;
/// Probability a forage roll yields a meal at all (avoids guaranteed free food).
const FORAGE_FALLBACK_CHANCE: f32 = 0.5;
/// Flat gold a broke jobless migrant receives when they reach a `flags.bank` room.
const BANK_RELIEF_AMOUNT: i32 = 5;
/// Real-time seconds a migrant must wait between successful bank visits — long
/// enough that the bank isn't a coin printer (~one game day at default cadence).
const BANK_VISIT_COOLDOWN_SECS: i64 = 600;

/// Charity + forage fallback fired when a hungry, broke NPC arrives home with
/// no shop options left. Tries cohabitant charity first, then a one-shot forage
/// roll against the area's city/wilderness table. On success the NPC eats
/// immediately. On failure broadcasts a hungry emote and sets goal=Idle so the
/// outer loop doesn't burn cycles re-trying every tick — `last_relief_attempt`
/// + `HOME_RELIEF_COOLDOWN_SECS` provides additional throttle.
fn try_home_relief(
    db: &db::Db,
    connections: &SharedConnections,
    mobile: &mut MobileData,
    config: &SimulationConfig,
    needs: &mut NeedsState,
    now: i64,
) -> Result<()> {
    if now - needs.last_relief_attempt < HOME_RELIEF_COOLDOWN_SECS {
        return Ok(());
    }
    needs.last_relief_attempt = now;

    let room_id = match mobile.current_room_id {
        Some(id) => id,
        None => return Ok(()),
    };

    // Step A: cohabitant charity. The cohabitant must be in this very room
    // (i.e. they're home too) so the broadcast lands somewhere believable.
    let cohab_id = mobile
        .relationships
        .iter()
        .find(|r| r.kind == ironmud::RelationshipKind::Cohabitant)
        .map(|r| r.other_id);
    if let Some(cohab_id) = cohab_id {
        if let Some(cohab) = db.get_mobile_data(&cohab_id)? {
            let here = cohab.current_room_id == Some(room_id);
            if here && !cohab.is_prototype && cohab.current_hp > 0 && cohab.gold >= COHABITANT_CHARITY_RESERVE {
                db.update_mobile(&cohab_id, |m| {
                    m.gold = (m.gold - COHABITANT_CHARITY_AMOUNT).max(0);
                })?;
                mobile.gold += COHABITANT_CHARITY_AMOUNT;
                broadcast_to_room_awake(
                    connections,
                    &room_id,
                    &format!("{} presses a few coins into {}'s hand.", cohab.name, mobile.name),
                );
                debug!(
                    "Simulation: {} received {} gold from cohabitant {}",
                    mobile.name, COHABITANT_CHARITY_AMOUNT, cohab.name
                );
                // Charity gave them coins — clear the tried-shops list so the next
                // hunger cycle can revisit shops with the new budget.
                needs.tried_shops_this_cycle.clear();
                return Ok(());
            }
        }
    }

    // Step B: forage scraps from the area's city/wilderness table. Picks one
    // entry uniformly; spawn + eat only if the entry resolves to a Food prototype.
    let mut rng = rand::thread_rng();
    if rng.r#gen::<f32>() < FORAGE_FALLBACK_CHANCE {
        if let Some(food_item) = try_forage_food(db, mobile, room_id, &mut rng)? {
            broadcast_to_room_awake(
                connections,
                &room_id,
                &format!("{} scrounges up some {}.", mobile.name, food_item.name),
            );
            eat_food_item(db, connections, mobile, needs, config, &food_item, room_id)?;
            return Ok(());
        }
    }

    // Both fallbacks failed. Surface a hungry emote and stand down so we don't
    // burn every tick re-running the relief path.
    broadcast_to_room_awake(
        connections,
        &room_id,
        &format!("{} eyes the empty cupboard with a hollow stomach.", mobile.name),
    );
    needs.current_goal = SimGoal::Idle;
    mobile.routine_destination_room = None;
    Ok(())
}

/// Roll one entry from the area's appropriate forage table (city by default,
/// wilderness if the home room is wilderness-flagged). Returns the spawned food
/// item placed in the mobile's inventory, or None if the table is empty / no
/// entry resolves to a Food prototype.
fn try_forage_food<R: Rng>(db: &db::Db, mobile: &MobileData, room_id: Uuid, rng: &mut R) -> Result<Option<ItemData>> {
    let room = match db.get_room_data(&room_id)? {
        Some(r) => r,
        None => return Ok(None),
    };
    let area_id = match room.area_id {
        Some(a) => a,
        None => return Ok(None),
    };
    let area = match db.get_area_data(&area_id)? {
        Some(a) => a,
        None => return Ok(None),
    };

    // Wilderness rooms (dirt floor, etc.) draw from the wilderness table; default
    // to the city table for everything else, including standard apartments.
    let table = if room.flags.dirt_floor && !area.wilderness_forage_table.is_empty() {
        &area.wilderness_forage_table
    } else if !area.city_forage_table.is_empty() {
        &area.city_forage_table
    } else if !area.wilderness_forage_table.is_empty() {
        &area.wilderness_forage_table
    } else {
        return Ok(None);
    };

    let entry = &table[rng.gen_range(0..table.len())];
    let proto = match db.get_item_by_vnum(&entry.vnum)? {
        Some(p) => p,
        None => return Ok(None),
    };
    if proto.item_type != ItemType::Food {
        return Ok(None);
    }
    let spawned = match db.spawn_item_from_prototype(&entry.vnum)? {
        Some(item) => item,
        None => return Ok(None),
    };
    db.move_item_to_mobile_inventory(&spawned.id, &mobile.id)?;
    Ok(Some(spawned))
}

/// Per-tick chance for a scavenger NPC to pocket one item from their current
/// room. Skips shopkeeper rooms (would be stealing) and the home room
/// (housemates' belongings). Requires `flags.scavenger`.
fn maybe_scavenge_pickup(
    db: &db::Db,
    connections: &SharedConnections,
    mobile: &mut MobileData,
    config: &SimulationConfig,
) -> Result<()> {
    if !mobile.flags.scavenger {
        return Ok(());
    }
    let room_id = match mobile.current_room_id {
        Some(id) => id,
        None => return Ok(()),
    };
    if is_at_room(db, mobile, &config.home_room_vnum)? {
        return Ok(());
    }
    let mobiles_here = db.get_mobiles_in_room(&room_id)?;
    if mobiles_here.iter().any(|m| m.flags.shopkeeper && !m.is_prototype) {
        return Ok(());
    }
    let items = db.get_items_in_room(&room_id)?;
    if items.is_empty() {
        return Ok(());
    }
    let mut rng = rand::thread_rng();
    if rng.r#gen::<f32>() > 0.25 {
        return Ok(());
    }
    let pick = &items[rng.gen_range(0..items.len())];
    db.move_item_to_mobile_inventory(&pick.id, &mobile.id)?;
    broadcast_to_room_awake(
        connections,
        &room_id,
        &format!("{} pockets {}.", mobile.name, pick.name),
    );
    debug!("Simulation: scavenger {} picked up {}", mobile.name, pick.name);
    Ok(())
}

// Floor nutrition so mis-configured food (nutrition=0) still feeds NPCs
// instead of silently being a no-op that leaves them stuck in SeekFood.
fn effective_nutrition(n: i32) -> i32 {
    if n <= 0 { 20 } else { n }
}

fn food_price(value: i32, sell_rate: i32) -> i32 {
    ((value as i64 * sell_rate as i64 / 100) as i32).max(1)
}

/// Pick the most-nutritious food in `stock` whose marked-up price fits the mobile's gold.
/// Falls back to `effective_nutrition` so misconfigured 0-nutrition food still ranks.
fn find_best_affordable_food(db: &db::Db, stock: &[String], sell_rate: i32, gold: i32) -> Result<Option<String>> {
    let mut best: Option<(String, i32)> = None;
    for vnum in stock {
        let proto = match db.get_item_by_vnum(vnum)? {
            Some(p) => p,
            None => continue,
        };
        if proto.item_type != ItemType::Food {
            continue;
        }
        if food_price(proto.value, sell_rate) > gold {
            continue;
        }
        let nutrition = effective_nutrition(proto.food_nutrition);
        match &best {
            Some((_, n)) if *n >= nutrition => {}
            _ => best = Some((vnum.clone(), nutrition)),
        }
    }
    Ok(best.map(|(vnum, _)| vnum))
}

// ---------------------------------------------------------------------------
// Step 5b: HP Regeneration
// ---------------------------------------------------------------------------

/// Simulated mobiles regenerate HP while resting at their home room, mirroring
/// the shape of the player regen in `src/ticks/character.rs`: base rate depends
/// on what they're doing (sleeping > resting > idle), and the rate is scaled
/// by hunger so starving NPCs heal slowly instead of not at all.
fn regenerate_hp(db: &db::Db, mobile: &mut MobileData, config: &SimulationConfig, needs: &NeedsState) -> Result<()> {
    if mobile.current_hp <= 0 || mobile.max_hp <= 0 || mobile.current_hp >= mobile.max_hp {
        return Ok(());
    }

    if !is_at_room(db, mobile, &config.home_room_vnum)? {
        return Ok(());
    }

    let base = match needs.current_goal {
        SimGoal::SeekSleep => 2,
        SimGoal::SeekComfort => 1,
        SimGoal::Idle => 1,
        _ => return Ok(()),
    };

    // Hunger modifier: well-fed heals faster, starving still heals (minimum 1)
    let regen = if needs.hunger > 75 {
        base * 3 / 2
    } else if needs.hunger > 50 {
        base
    } else if needs.hunger > 25 {
        ((base + 1) / 2).max(1)
    } else {
        (base / 4).max(1)
    };

    mobile.current_hp = (mobile.current_hp + regen).min(mobile.max_hp);
    debug!(
        "Simulation: {} regenerated {} hp at home (now {}/{})",
        mobile.name, regen, mobile.current_hp, mobile.max_hp
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Step 6: Set Destination
// ---------------------------------------------------------------------------

fn set_destination(db: &db::Db, mobile: &mut MobileData, config: &SimulationConfig, needs: &NeedsState) -> Result<()> {
    let dest_vnum = match needs.current_goal {
        SimGoal::SeekSleep | SimGoal::SeekComfort | SimGoal::GoingHome => &config.home_room_vnum,
        SimGoal::SeekFood => {
            let has_food = db
                .get_items_in_mobile_inventory(&mobile.id)?
                .iter()
                .any(|i| i.item_type == ItemType::Food);
            if has_food {
                &config.home_room_vnum
            } else if mobile.gold <= 0 || config.shop_room_vnum.is_empty() {
                // Broke or no assigned shop: head home for charity/forage fallback.
                // Arrival actions run `try_home_relief` once we're there.
                &config.home_room_vnum
            } else {
                // If the assigned shop has already been tried this hunger cycle,
                // route to the next untried shop in the area instead.
                let assigned_tried = match db.get_room_by_vnum(&config.shop_room_vnum)? {
                    Some(r) => needs.tried_shops_this_cycle.contains(&r.id),
                    None => false,
                };
                if assigned_tried {
                    if let Some(other) = find_next_shop_room(db, mobile, &needs.tried_shops_this_cycle)? {
                        let already_there = mobile.current_room_id == Some(other);
                        mobile.routine_destination_room = if already_there { None } else { Some(other) };
                        return Ok(());
                    }
                    // All shops in the area are tried and unaffordable. Fall back home.
                    &config.home_room_vnum
                } else {
                    &config.shop_room_vnum
                }
            }
        }
        SimGoal::GoingToWork | SimGoal::Working => &config.work_room_vnum,
        SimGoal::SeekBank => {
            // Bank rooms are looked up by id from the area, not by vnum on the
            // sim config — short-circuit the vnum resolve path below.
            match find_bank_room_in_area(db, mobile)? {
                Some(bank_id) => {
                    let already_there = mobile.current_room_id == Some(bank_id);
                    mobile.routine_destination_room = if already_there { None } else { Some(bank_id) };
                }
                None => {
                    mobile.routine_destination_room = None;
                }
            }
            return Ok(());
        }
        SimGoal::Idle => {
            mobile.routine_destination_room = None;
            return Ok(());
        }
    };

    if dest_vnum.is_empty() {
        mobile.routine_destination_room = None;
        return Ok(());
    }

    match db.get_room_by_vnum(dest_vnum)? {
        Some(room) => {
            let already_there = mobile.current_room_id.map(|r| r == room.id).unwrap_or(false);
            if already_there {
                mobile.routine_destination_room = None;
            } else {
                mobile.routine_destination_room = Some(room.id);
            }
        }
        None => {
            warn!(
                "Simulation: mobile {} has invalid vnum '{}' in simulation config",
                mobile.name, dest_vnum
            );
            mobile.routine_destination_room = None;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Step 7: Update Activity State
// ---------------------------------------------------------------------------

fn update_activity_state(
    db: &db::Db,
    connections: &SharedConnections,
    mobile: &mut MobileData,
    config: &SimulationConfig,
    needs: &NeedsState,
) -> Result<()> {
    // A mobile is only "Sleeping"/"Eating"/"Working" when it has actually
    // arrived at the relevant room — while travelling it should read OffDuty so
    // players don't see "Ayaka is sleeping here" in the middle of the street.
    let at_home = is_at_room(db, mobile, &config.home_room_vnum)?;
    let at_work = is_at_room(db, mobile, &config.work_room_vnum)?;

    let old_activity = mobile.current_activity.clone();

    let new_activity = match needs.current_goal {
        SimGoal::Working => {
            if at_work {
                ActivityState::Working
            } else {
                ActivityState::OffDuty
            }
        }
        SimGoal::GoingToWork => ActivityState::OffDuty,
        SimGoal::SeekSleep => {
            if at_home {
                ActivityState::Sleeping
            } else {
                ActivityState::OffDuty
            }
        }
        SimGoal::SeekFood => {
            // Only show "Eating" when actually at a food source (our shop or
            // any room with a food-stocking shopkeeper in it). Otherwise they're
            // just walking — and a florist or tailor doesn't count as a food source.
            let at_shop = is_at_room(db, mobile, &config.shop_room_vnum)?;
            let here_has_food_shop = match mobile.current_room_id {
                Some(rid) => {
                    let mobiles_here = db.get_mobiles_in_room(&rid)?;
                    let mut found = false;
                    for m in &mobiles_here {
                        if m.flags.shopkeeper && !m.is_prototype && shopkeeper_sells_food(db, m)? {
                            found = true;
                            break;
                        }
                    }
                    found
                }
                None => false,
            };
            if at_shop || here_has_food_shop {
                ActivityState::Eating
            } else {
                ActivityState::OffDuty
            }
        }
        SimGoal::SeekComfort | SimGoal::Idle | SimGoal::GoingHome | SimGoal::SeekBank => {
            ActivityState::OffDuty
        }
    };

    if old_activity != new_activity {
        if let Some(room_id) = mobile.current_room_id {
            let msg = sleep_transition_message(&mobile.name, &old_activity, &new_activity);
            if let Some(msg) = msg {
                broadcast_to_room_awake(connections, &room_id, &msg);
            }
        }
    }

    mobile.current_activity = new_activity;
    Ok(())
}

/// Returns a broadcast string when an activity change crosses the Sleeping
/// boundary in either direction. Returns None for transitions that don't
/// involve sleep (e.g. Working -> OffDuty), since those don't need a visible
/// announcement.
pub(crate) fn sleep_transition_message(name: &str, old: &ActivityState, new: &ActivityState) -> Option<String> {
    let was_sleeping = matches!(old, ActivityState::Sleeping);
    let is_sleeping = matches!(new, ActivityState::Sleeping);
    if was_sleeping && !is_sleeping {
        Some(format!("{} wakes up and stretches.", name))
    } else if !was_sleeping && is_sleeping {
        Some(format!("{} lies down and goes to sleep.", name))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Step 8: Ambient Emotes
// ---------------------------------------------------------------------------

fn emit_ambient_emotes(
    connections: &SharedConnections,
    mobile: &MobileData,
    needs: &mut NeedsState,
    now: i64,
    current_game_day: i32,
) {
    // Enforce cooldown between emotes
    if now - needs.last_emote_tick < EMOTE_COOLDOWN_SECS {
        return;
    }

    let room_id = match mobile.current_room_id {
        Some(id) => id,
        None => return,
    };

    let mut rng = rand::thread_rng();

    // Only 30% chance per tick to emit an emote (avoid spam)
    if rng.gen_range(0..100) >= 30 {
        return;
    }

    let mood = mobile.social.as_ref().map(|s| s.mood);
    let is_bereaved = mobile
        .social
        .as_ref()
        .and_then(|s| s.bereaved_until_day)
        .map(|d| d > current_game_day)
        .unwrap_or(false);
    let emote = pick_emote(&mobile.name, needs, mood, is_bereaved, &mut rng);

    if let Some(msg) = emote {
        broadcast_to_room_awake(connections, &room_id, &msg);
        needs.last_emote_tick = now;
    }
}

fn pick_emote(
    name: &str,
    needs: &NeedsState,
    mood: Option<ironmud::MoodState>,
    is_bereaved: bool,
    rng: &mut impl Rng,
) -> Option<String> {
    // Prioritize the most urgent need
    let mut candidates: Vec<String> = Vec::new();

    // Mood-driven emotes (sad/depressed/breakdown bias toward noticeable cues)
    match mood {
        Some(ironmud::MoodState::Breakdown) => {
            candidates.push(format!("{} mutters to themselves, staring blankly.", name));
            candidates.push(format!("{} wipes at their eyes and trembles.", name));
            candidates.push(format!("{} sits down heavily, head in their hands.", name));
        }
        Some(ironmud::MoodState::Depressed) => {
            candidates.push(format!("{} sighs heavily.", name));
            candidates.push(format!("{} stares off, looking miserable.", name));
        }
        Some(ironmud::MoodState::Sad) => {
            candidates.push(format!("{} looks a little down.", name));
        }
        _ => {}
    }

    // Bereavement emotes run on top of mood: the survivor can be in any mood
    // bucket (they're usually Sad/Depressed post-death, but happiness recovers
    // over the 14 days). Specific grief cues give the room context that a
    // generic "sighs heavily" doesn't.
    if is_bereaved {
        candidates.push(format!("{} gazes into the distance, lost in memory.", name));
        candidates.push(format!(
            "{} traces something unseen with a finger, tears welling.",
            name
        ));
        candidates.push(format!("{} whispers a name only they can hear.", name));
    }

    // Hunger emotes
    if needs.hunger <= 15 {
        candidates.push(format!("{} looks weak with hunger.", name));
        candidates.push(format!("{} sways unsteadily, clearly famished.", name));
    } else if needs.hunger <= 30 {
        candidates.push(format!("{}'s stomach growls loudly.", name));
        candidates.push(format!("{} clutches their stomach uncomfortably.", name));
    } else if needs.hunger <= 50 {
        candidates.push(format!("{}'s stomach growls quietly.", name));
        candidates.push(format!("{} glances around, looking for something to eat.", name));
    }

    // Energy emotes
    if needs.energy <= 15 {
        candidates.push(format!("{} can barely stand from exhaustion.", name));
        candidates.push(format!("{} leans against the wall, about to collapse.", name));
    } else if needs.energy <= 30 {
        candidates.push(format!("{} struggles to keep their eyes open.", name));
        candidates.push(format!("{} stumbles with exhaustion.", name));
    } else if needs.energy <= 50 {
        candidates.push(format!("{} yawns.", name));
        candidates.push(format!("{} rubs their eyes tiredly.", name));
    }

    // Comfort emotes
    if needs.comfort <= 20 {
        candidates.push(format!("{} looks miserable.", name));
        candidates.push(format!("{} mutters about wanting to go home.", name));
    } else if needs.comfort <= 40 {
        candidates.push(format!("{} shifts uncomfortably.", name));
        candidates.push(format!("{} looks restless.", name));
    }

    // Sleeping emotes (when at home sleeping)
    if needs.current_goal == SimGoal::SeekSleep && needs.energy < 80 {
        candidates.push(format!("{} sleeps peacefully.", name));
        candidates.push(format!("{} snores softly.", name));
    }

    // Positive emotes when all needs are good
    if needs.hunger > 80 && needs.energy > 80 && needs.comfort > 80 {
        candidates.push(format!("{} hums contentedly.", name));
        candidates.push(format!("{} looks happy and well-rested.", name));
    }

    if candidates.is_empty() {
        return None;
    }

    let idx = rng.gen_range(0..candidates.len());
    Some(candidates.swap_remove(idx))
}

// ---------------------------------------------------------------------------
// Step 9: Social Conversation
// ---------------------------------------------------------------------------

/// Seconds a mobile must wait between conversation attempts. Applies to both
/// the initiator and the partner — if the partner just finished talking to
/// someone else, they're unavailable to this initiator until the cooldown
/// clears on their own `last_converse_secs`.
const CONVERSE_COOLDOWN_SECS: u64 = 300;

/// Minimum existing affinity required for a bereaved mobile to confide in a
/// partner. Below this, strangers can't provide comfort and we fall back
/// to a normal topical conversation (or no conversation at all).
const GRIEF_COMFORT_MIN_AFFINITY: i32 = 30;

fn maybe_converse(
    db: &db::Db,
    connections: &SharedConnections,
    mobile: &mut MobileData,
    now: i64,
    current_game_day: i32,
) -> Result<()> {
    use rand::seq::SliceRandom;

    let now_secs = now.max(0) as u64;

    let social = match mobile.social.as_ref() {
        Some(s) => s.clone(),
        None => return Ok(()),
    };
    if now_secs < social.last_converse_secs.saturating_add(CONVERSE_COOLDOWN_SECS) {
        return Ok(());
    }
    if social.likes.is_empty() {
        return Ok(());
    }

    let room_id = match mobile.current_room_id {
        Some(id) => id,
        None => return Ok(()),
    };

    let others = db.get_mobiles_in_room(&room_id)?;
    let mut rng = rand::thread_rng();
    let candidates: Vec<MobileData> = others
        .into_iter()
        .filter(|m| {
            m.id != mobile.id
                && !m.is_prototype
                && m.current_hp > 0
                && !m.combat.in_combat
                && m.social.as_ref().map_or(false, |s| {
                    now_secs >= s.last_converse_secs.saturating_add(CONVERSE_COOLDOWN_SECS)
                })
        })
        .collect();
    if candidates.is_empty() {
        return Ok(());
    }

    let partner = match candidates.choose(&mut rng) {
        Some(p) => p.clone(),
        None => return Ok(()),
    };

    let partner_social = partner.social.as_ref().unwrap();
    let partner_id = partner.id;
    let initiator_id = mobile.id;
    let game_day = current_game_day;

    // Grief path: when either side is bereaved and already has a warm
    // relationship with this partner, the conversation becomes an act of
    // comfort rather than a topical chat. No topic is chosen (nothing gets
    // pushed to recent_topics) and the happiness boost for the grieving
    // side is significantly larger than any topic match could produce, so
    // friends help a widow/widower recover faster than the passive 14-day
    // drift from bereaved_until_day.
    let init_bereaved = is_bereaved(&social, current_game_day);
    let part_bereaved = is_bereaved(partner_social, current_game_day);
    let existing_affinity = mobile
        .relationships
        .iter()
        .find(|r| r.other_id == partner_id)
        .map(|r| r.affinity)
        .unwrap_or(0);

    if (init_bereaved || part_bereaved) && existing_affinity >= GRIEF_COMFORT_MIN_AFFINITY {
        let (msg, dh_init, dh_part) = if init_bereaved {
            (
                format!(
                    "{} leans on {} and speaks quietly of their loss.",
                    mobile.name, partner.name
                ),
                6,
                2,
            )
        } else {
            (
                format!(
                    "{} listens as {} grieves, offering a steady shoulder.",
                    mobile.name, partner.name
                ),
                2,
                6,
            )
        };
        broadcast_to_room_awake(connections, &room_id, &msg);
        db.update_mobile(&initiator_id, |m| {
            if let Some(s) = m.social.as_mut() {
                s.happiness = (s.happiness + dh_init).clamp(0, 100);
                s.last_converse_secs = now_secs;
            }
            bump_affinity(m, partner_id, 2, game_day);
            ironmud::social::apply_mood(m);
        })?;
        db.update_mobile(&partner_id, |m| {
            if let Some(s) = m.social.as_mut() {
                s.happiness = (s.happiness + dh_part).clamp(0, 100);
                s.last_converse_secs = now_secs;
            }
            bump_affinity(m, initiator_id, 2, game_day);
            ironmud::social::apply_mood(m);
        })?;
        if let Ok(Some(fresh)) = db.get_mobile_data(&initiator_id) {
            *mobile = fresh;
        }
        return Ok(());
    }

    let topic = match social.likes.choose(&mut rng) {
        Some(t) => t.clone(),
        None => return Ok(()),
    };

    let disposition = if partner_social.likes.iter().any(|t| t == &topic) {
        Disposition::Match
    } else if partner_social.dislikes.iter().any(|t| t == &topic) {
        Disposition::Dislike
    } else {
        Disposition::Neutral
    };

    let (base_dh_init, base_dh_part, base_da_init, base_da_part, msg) = match disposition {
        Disposition::Match => (
            3,
            3,
            5,
            5,
            format!("{} and {} trade stories about {}.", mobile.name, partner.name, topic),
        ),
        Disposition::Neutral => (
            1,
            1,
            0,
            0,
            format!("{} chats with {} about {}.", mobile.name, partner.name, topic),
        ),
        Disposition::Dislike => (
            -3,
            -3,
            -5,
            -5,
            format!("{} scowls as {} brings up {}.", partner.name, mobile.name, topic),
        ),
    };

    let init_fatigued = has_recent_topic(mobile, partner_id, &topic);
    let part_fatigued = has_recent_topic(&partner, initiator_id, &topic);
    let dh_init = apply_fatigue(base_dh_init, init_fatigued);
    let dh_part = apply_fatigue(base_dh_part, part_fatigued);
    let da_init = apply_fatigue(base_da_init, init_fatigued);
    let da_part = apply_fatigue(base_da_part, part_fatigued);

    broadcast_to_room_awake(connections, &room_id, &msg);

    let topic_init = topic.clone();
    db.update_mobile(&initiator_id, |m| {
        if let Some(s) = m.social.as_mut() {
            s.happiness = (s.happiness + dh_init).clamp(0, 100);
            s.last_converse_secs = now_secs;
        }
        upsert_relationship(m, partner_id, da_init, game_day, &topic_init);
        ironmud::social::apply_mood(m);
    })?;

    let topic_part = topic.clone();
    db.update_mobile(&partner_id, |m| {
        if let Some(s) = m.social.as_mut() {
            s.happiness = (s.happiness + dh_part).clamp(0, 100);
            s.last_converse_secs = now_secs;
        }
        upsert_relationship(m, initiator_id, da_part, game_day, &topic_part);
        ironmud::social::apply_mood(m);
    })?;

    // Refresh in-memory copy so subsequent steps (if any) see fresh state.
    if let Ok(Some(fresh)) = db.get_mobile_data(&initiator_id) {
        *mobile = fresh;
    }

    Ok(())
}

enum Disposition {
    Match,
    Neutral,
    Dislike,
}

/// Add or update a `Relationship` entry on `mobile` pointing at `other_id`.
/// New entries start as `Friend`; affinity accumulates and clamps; already-set
/// kinds (Cohabitant, Partner, etc.) are preserved so conversation doesn't
/// demote a cohabiting pair back to Friend. The `topic` covered is pushed to
/// the front of `recent_topics` so future conversations on the same subject
/// trigger the fatigue penalty.
fn upsert_relationship(mobile: &mut MobileData, other_id: Uuid, affinity_delta: i32, day: i32, topic: &str) {
    use ironmud::{Relationship, RelationshipKind, TOPIC_FATIGUE_WINDOW};
    if let Some(rel) = mobile.relationships.iter_mut().find(|r| r.other_id == other_id) {
        // Family kinds (Partner/Parent/Child/Sibling) dampen negative deltas
        // and amplify positives — kin are slower to dislike each other.
        let biased = ironmud::social::apply_family_bias(affinity_delta, rel.kind);
        rel.affinity = (rel.affinity + biased).clamp(-100, 100);
        rel.last_interaction_day = day;
        push_recent_topic(&mut rel.recent_topics, topic);
    } else {
        let mut recent_topics = Vec::with_capacity(TOPIC_FATIGUE_WINDOW);
        recent_topics.push(topic.to_string());
        mobile.relationships.push(Relationship {
            other_id,
            kind: RelationshipKind::Friend,
            affinity: affinity_delta.clamp(-100, 100),
            last_interaction_day: day,
            recent_topics,
        });
    }
}

/// True if this SocialState has a bereavement timestamp that hasn't yet
/// elapsed. `bereaved_until_day` is set in `db::delete_mobile` when a
/// Cohabitant dies (today + 14 game days) and tested here on every
/// conversation/emote.
fn is_bereaved(social: &ironmud::SocialState, current_game_day: i32) -> bool {
    social.bereaved_until_day.map(|d| d > current_game_day).unwrap_or(false)
}

/// Nudge affinity toward `other_id` without touching `recent_topics`. Used
/// by the grief-comfort path, which isn't a topical conversation. New
/// entries default to `Friend`.
fn bump_affinity(mobile: &mut MobileData, other_id: Uuid, delta: i32, day: i32) {
    use ironmud::{Relationship, RelationshipKind};
    if let Some(rel) = mobile.relationships.iter_mut().find(|r| r.other_id == other_id) {
        let biased = ironmud::social::apply_family_bias(delta, rel.kind);
        rel.affinity = (rel.affinity + biased).clamp(-100, 100);
        rel.last_interaction_day = day;
    } else {
        mobile.relationships.push(Relationship {
            other_id,
            kind: RelationshipKind::Friend,
            affinity: delta.clamp(-100, 100),
            last_interaction_day: day,
            recent_topics: Vec::new(),
        });
    }
}

/// True if `mobile`'s `Relationship` with `other_id` already covered `topic`
/// inside the fatigue window.
fn has_recent_topic(mobile: &MobileData, other_id: Uuid, topic: &str) -> bool {
    mobile
        .relationships
        .iter()
        .find(|r| r.other_id == other_id)
        .map(|r| r.recent_topics.iter().any(|t| t == topic))
        .unwrap_or(false)
}

/// Halve the delta (toward zero) when the topic has been discussed recently.
/// A `0` delta stays at 0; `1` rounds to 0 (neutral dispositions fizzle on
/// repetition); `-3` becomes `-1` etc.
fn apply_fatigue(delta: i32, fatigued: bool) -> i32 {
    if fatigued { delta / 2 } else { delta }
}

/// Push `topic` to the front of `recent`, dropping duplicates and trimming
/// the tail to `TOPIC_FATIGUE_WINDOW`.
fn push_recent_topic(recent: &mut Vec<String>, topic: &str) {
    use ironmud::TOPIC_FATIGUE_WINDOW;
    recent.retain(|t| t != topic);
    recent.insert(0, topic.to_string());
    if recent.len() > TOPIC_FATIGUE_WINDOW {
        recent.truncate(TOPIC_FATIGUE_WINDOW);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ironmud::{MobileData, RoomData, RoomExits, RoomFlags, WaterType};
    use std::collections::HashMap;

    struct DbGuard {
        db: db::Db,
        path: String,
    }
    impl Drop for DbGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn open_db(tag: &str) -> DbGuard {
        let path = format!(
            "test_sim_{}_{}_{}.db",
            tag,
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        );
        let _ = std::fs::remove_dir_all(&path);
        let db = db::Db::open(&path).expect("open db");
        DbGuard { db, path }
    }

    fn save_room(db: &db::Db, vnum: &str) -> RoomData {
        let r = RoomData {
            id: uuid::Uuid::new_v4(),
            title: format!("room {}", vnum),
            description: String::new(),
            exits: RoomExits::default(),
            flags: RoomFlags::default(),
            extra_descs: Vec::new(),
            vnum: Some(vnum.to_string()),
            area_id: None,
            triggers: Vec::new(),
            doors: HashMap::new(),
            spring_desc: None,
            summer_desc: None,
            autumn_desc: None,
            winter_desc: None,
            dynamic_desc: None,
            water_type: WaterType::None,
            catch_table: Vec::new(),
            is_property_template: false,
            property_template_id: None,
            is_template_entrance: false,
            property_lease_id: None,
            property_entrance: false,
            recent_departures: Vec::new(),
            blood_trails: Vec::new(),
            traps: Vec::new(),
            living_capacity: 0,
            residents: Vec::new(),
        };
        db.save_room_data(r.clone()).expect("save room");
        db.set_room_vnum(&r.id, vnum).expect("vnum");
        r
    }

    fn make_config() -> SimulationConfig {
        SimulationConfig {
            home_room_vnum: "test:home".to_string(),
            work_room_vnum: "test:work".to_string(),
            shop_room_vnum: "test:shop".to_string(),
            preferred_food_vnum: String::new(),
            work_pay: 50,
            work_start_hour: 8,
            work_end_hour: 17,
            hunger_decay_rate: 0,
            energy_decay_rate: 0,
            comfort_decay_rate: 0,
            low_gold_threshold: 10,
        }
    }

    #[test]
    fn broke_hungry_during_work_hours_goes_to_work() {
        let g = open_db("broke_work");
        let _home = save_room(&g.db, "test:home");
        let _shop = save_room(&g.db, "test:shop");
        let work = save_room(&g.db, "test:work");

        let mut mobile = MobileData::new("Ned".to_string());
        mobile.gold = 0;
        mobile.current_room_id = Some(uuid::Uuid::new_v4());
        g.db.save_mobile_data(mobile.clone()).expect("save mob");

        let needs = NeedsState {
            hunger: 12,
            energy: 60,
            comfort: 60,
            ..Default::default()
        };
        let goal = decide_goal(&g.db, &mobile, &make_config(), &needs, 10, 0).expect("decide");
        assert_eq!(goal, SimGoal::GoingToWork, "broke + hungry + work hours -> GoingToWork");

        mobile.current_room_id = Some(work.id);
        let goal = decide_goal(&g.db, &mobile, &make_config(), &needs, 10, 0).expect("decide");
        assert_eq!(goal, SimGoal::Working);
    }

    #[test]
    fn broke_off_shift_with_stamina_idles_not_sleeps() {
        let g = open_db("broke_off");
        save_room(&g.db, "test:home");
        save_room(&g.db, "test:shop");
        save_room(&g.db, "test:work");

        let mut mobile = MobileData::new("Ned".to_string());
        mobile.gold = 0;
        mobile.current_room_id = Some(uuid::Uuid::new_v4());
        g.db.save_mobile_data(mobile.clone()).expect("save mob");

        let needs = NeedsState {
            hunger: 40,
            energy: 70,
            comfort: 60,
            ..Default::default()
        };
        let goal = decide_goal(&g.db, &mobile, &make_config(), &needs, 22, 0).expect("decide");
        assert_eq!(
            goal,
            SimGoal::Idle,
            "broke off-shift with stamina should idle, not sleep"
        );
    }

    #[test]
    fn fed_off_shift_not_constantly_sleeping() {
        let g = open_db("fed_off");
        save_room(&g.db, "test:home");
        save_room(&g.db, "test:shop");
        save_room(&g.db, "test:work");

        let mut mobile = MobileData::new("Ned".to_string());
        mobile.gold = 100;
        mobile.current_room_id = Some(uuid::Uuid::new_v4());
        g.db.save_mobile_data(mobile.clone()).expect("save mob");

        // energy=25 used to trigger SeekSleep; new threshold (<=20) should let Idle through.
        let needs = NeedsState {
            hunger: 80,
            energy: 25,
            comfort: 80,
            ..Default::default()
        };
        let goal = decide_goal(&g.db, &mobile, &make_config(), &needs, 22, 0).expect("decide");
        assert_ne!(
            goal,
            SimGoal::SeekSleep,
            "energy 25 should no longer trigger sleep off-shift"
        );
    }

    #[test]
    fn broke_off_shift_now_seeks_food_for_relief() {
        // Regression: prior catch-22 sent broke+hungry off-shift NPCs to sleep,
        // trapping them in a hallway-pacing loop. Now they pick SeekFood so
        // set_destination can route them home for charity / forage relief.
        let g = open_db("broke_off_seekfood");
        save_room(&g.db, "test:home");
        save_room(&g.db, "test:shop");
        save_room(&g.db, "test:work");

        let mut mobile = MobileData::new("Matilda".to_string());
        mobile.gold = 0;
        mobile.current_room_id = Some(uuid::Uuid::new_v4());
        g.db.save_mobile_data(mobile.clone()).expect("save mob");

        let needs = NeedsState {
            hunger: 20,
            energy: 70,
            comfort: 60,
            ..Default::default()
        };
        let goal = decide_goal(&g.db, &mobile, &make_config(), &needs, 22, 0).expect("decide");
        assert_eq!(
            goal,
            SimGoal::SeekFood,
            "broke + hungry + off-shift now SeekFood (relief at home), no longer SeekSleep"
        );
    }

    #[test]
    fn role_hourly_wage_pays_guard_in_resident_area() {
        use ironmud::{
            AreaData, AreaFlags, AreaPermission, CombatZoneType, GoldRange, ImmigrationFamilyChance,
            ImmigrationVariationChances,
        };

        let g = open_db("role_wage_guard");
        // Set up an area with a guard wage configured.
        let area_id = uuid::Uuid::new_v4();
        let area = AreaData {
            id: area_id,
            name: "T".into(),
            prefix: "t".into(),
            description: String::new(),
            level_min: 0,
            level_max: 0,
            theme: String::new(),
            owner: None,
            permission_level: AreaPermission::AllBuilders,
            trusted_builders: Vec::new(),
            city_forage_table: Vec::new(),
            wilderness_forage_table: Vec::new(),
            shallow_water_forage_table: Vec::new(),
            deep_water_forage_table: Vec::new(),
            underwater_forage_table: Vec::new(),
            combat_zone: CombatZoneType::Pve,
            flags: AreaFlags::default(),
            default_room_flags: RoomFlags::default(),
            immigration_enabled: false,
            immigration_room_vnum: String::new(),
            immigration_name_pool: String::new(),
            immigration_visual_profile: String::new(),
            migration_interval_days: 0,
            migration_max_per_check: 0,
            migrant_sim_defaults: None,
            last_migration_check_day: None,
            immigration_variation_chances: ImmigrationVariationChances::default(),
            immigration_family_chance: ImmigrationFamilyChance::default(),
            migrant_starting_gold: GoldRange::default(),
            guard_wage_per_hour: 25,
            healer_wage_per_hour: 0,
            scavenger_wage_per_hour: 0,
        };
        g.db.save_area_data(area).expect("save area");

        // Home + a separate patrol room, both in the area.
        let mut home = save_room(&g.db, "t:home");
        home.area_id = Some(area_id);
        g.db.save_room_data(home.clone()).expect("re-save home");
        let mut patrol = save_room(&g.db, "t:street");
        patrol.area_id = Some(area_id);
        g.db.save_room_data(patrol.clone()).expect("re-save patrol");

        // Guard mobile patrolling away from home, in their resident area.
        let mut guard = MobileData::new("Garron".into());
        guard.flags.guard = true;
        guard.resident_of = Some("t:home".into());
        guard.current_room_id = Some(patrol.id);
        guard.gold = 0;

        let wage = role_hourly_wage(&g.db, &guard).expect("role wage").expect("paid");
        assert_eq!(wage, 25, "guard should earn area treasury wage");

        // Same guard standing in a non-resident room earns nothing.
        let outside = save_room(&g.db, "elsewhere:room");
        guard.current_room_id = Some(outside.id);
        let wage = role_hourly_wage(&g.db, &guard).expect("role wage outside");
        assert!(wage.is_none(), "guard outside resident area earns nothing");
    }

    #[test]
    fn role_hourly_wage_excludes_scavenger_at_home() {
        use ironmud::{
            AreaData, AreaFlags, AreaPermission, CombatZoneType, GoldRange, ImmigrationFamilyChance,
            ImmigrationVariationChances,
        };

        let g = open_db("role_wage_scav");
        let area_id = uuid::Uuid::new_v4();
        let area = AreaData {
            id: area_id,
            name: "T".into(),
            prefix: "t".into(),
            description: String::new(),
            level_min: 0,
            level_max: 0,
            theme: String::new(),
            owner: None,
            permission_level: AreaPermission::AllBuilders,
            trusted_builders: Vec::new(),
            city_forage_table: Vec::new(),
            wilderness_forage_table: Vec::new(),
            shallow_water_forage_table: Vec::new(),
            deep_water_forage_table: Vec::new(),
            underwater_forage_table: Vec::new(),
            combat_zone: CombatZoneType::Pve,
            flags: AreaFlags::default(),
            default_room_flags: RoomFlags::default(),
            immigration_enabled: false,
            immigration_room_vnum: String::new(),
            immigration_name_pool: String::new(),
            immigration_visual_profile: String::new(),
            migration_interval_days: 0,
            migration_max_per_check: 0,
            migrant_sim_defaults: None,
            last_migration_check_day: None,
            immigration_variation_chances: ImmigrationVariationChances::default(),
            immigration_family_chance: ImmigrationFamilyChance::default(),
            migrant_starting_gold: GoldRange::default(),
            guard_wage_per_hour: 0,
            healer_wage_per_hour: 0,
            scavenger_wage_per_hour: 7,
        };
        g.db.save_area_data(area).expect("save area");
        let mut home = save_room(&g.db, "t:home2");
        home.area_id = Some(area_id);
        g.db.save_room_data(home.clone()).expect("re-save home");

        let mut scav = MobileData::new("Sif".into());
        scav.flags.scavenger = true;
        scav.resident_of = Some("t:home2".into());
        scav.current_room_id = Some(home.id);

        let wage = role_hourly_wage(&g.db, &scav).expect("role wage at home");
        assert!(wage.is_none(), "scavenger at home does not earn");
    }

    #[test]
    fn topic_fatigue_halves_delta_and_caps_window() {
        use ironmud::TOPIC_FATIGUE_WINDOW;

        // First mention yields full delta.
        assert_eq!(apply_fatigue(5, false), 5);
        // Second mention yields half (toward zero).
        assert_eq!(apply_fatigue(5, true), 2);
        assert_eq!(apply_fatigue(-5, true), -2);
        // Neutral deltas (1) fizzle to 0 once fatigued.
        assert_eq!(apply_fatigue(1, true), 0);
        assert_eq!(apply_fatigue(0, true), 0);

        // push_recent_topic deduplicates and caps to TOPIC_FATIGUE_WINDOW.
        let mut recent: Vec<String> = Vec::new();
        for i in 0..(TOPIC_FATIGUE_WINDOW + 2) {
            push_recent_topic(&mut recent, &format!("topic_{}", i));
        }
        assert_eq!(recent.len(), TOPIC_FATIGUE_WINDOW);
        assert_eq!(recent[0], format!("topic_{}", TOPIC_FATIGUE_WINDOW + 1));

        // Re-pushing the same topic moves it to the front without growing the list.
        let before_len = recent.len();
        push_recent_topic(&mut recent, &format!("topic_{}", TOPIC_FATIGUE_WINDOW));
        assert_eq!(recent.len(), before_len);
        assert_eq!(recent[0], format!("topic_{}", TOPIC_FATIGUE_WINDOW));
    }

    #[test]
    fn is_bereaved_respects_until_day() {
        use ironmud::SocialState;
        let never = SocialState {
            bereaved_until_day: None,
            ..SocialState::default()
        };
        assert!(!is_bereaved(&never, 10));
        let active = SocialState {
            bereaved_until_day: Some(20),
            ..SocialState::default()
        };
        assert!(is_bereaved(&active, 10));
        assert!(is_bereaved(&active, 19));
        assert!(!is_bereaved(&active, 20), "bereavement ends on the until_day itself");
        assert!(!is_bereaved(&active, 30));
    }

    #[test]
    fn bump_affinity_creates_friend_and_preserves_recent_topics() {
        use ironmud::{Relationship, RelationshipKind};
        let mut m = MobileData::new("Mourner".to_string());
        let other = uuid::Uuid::new_v4();

        bump_affinity(&mut m, other, 2, 100);
        assert_eq!(m.relationships.len(), 1);
        assert_eq!(m.relationships[0].affinity, 2);
        assert!(matches!(m.relationships[0].kind, RelationshipKind::Friend));
        assert!(m.relationships[0].recent_topics.is_empty());

        // Pre-seed recent topics; bump_affinity should NOT clear them.
        m.relationships[0].recent_topics = vec!["fishing".into()];
        bump_affinity(&mut m, other, 3, 101);
        assert_eq!(m.relationships[0].affinity, 5);
        assert_eq!(m.relationships[0].last_interaction_day, 101);
        assert_eq!(m.relationships[0].recent_topics, vec!["fishing".to_string()]);

        // Existing Cohabitant kind is preserved (bump_affinity is neutral).
        let other2 = uuid::Uuid::new_v4();
        m.relationships.push(Relationship {
            other_id: other2,
            kind: RelationshipKind::Cohabitant,
            affinity: 80,
            last_interaction_day: 0,
            recent_topics: Vec::new(),
        });
        bump_affinity(&mut m, other2, 1, 102);
        assert!(matches!(
            m.relationships.iter().find(|r| r.other_id == other2).unwrap().kind,
            RelationshipKind::Cohabitant
        ));
    }

    /// Build + save an area with the given role wages, defaulting all other
    /// immigration knobs. Returns the area id so callers can stamp it on rooms.
    fn make_area_with_wages(db: &db::Db, prefix: &str, guard: i32, healer: i32, scavenger: i32) -> uuid::Uuid {
        use ironmud::{
            AreaData, AreaFlags, AreaPermission, CombatZoneType, GoldRange, ImmigrationFamilyChance,
            ImmigrationVariationChances,
        };
        let area_id = uuid::Uuid::new_v4();
        let area = AreaData {
            id: area_id,
            name: prefix.into(),
            prefix: prefix.into(),
            description: String::new(),
            level_min: 0,
            level_max: 0,
            theme: String::new(),
            owner: None,
            permission_level: AreaPermission::AllBuilders,
            trusted_builders: Vec::new(),
            city_forage_table: Vec::new(),
            wilderness_forage_table: Vec::new(),
            shallow_water_forage_table: Vec::new(),
            deep_water_forage_table: Vec::new(),
            underwater_forage_table: Vec::new(),
            combat_zone: CombatZoneType::Pve,
            flags: AreaFlags::default(),
            default_room_flags: RoomFlags::default(),
            immigration_enabled: false,
            immigration_room_vnum: String::new(),
            immigration_name_pool: String::new(),
            immigration_visual_profile: String::new(),
            migration_interval_days: 0,
            migration_max_per_check: 0,
            migrant_sim_defaults: None,
            last_migration_check_day: None,
            immigration_variation_chances: ImmigrationVariationChances::default(),
            immigration_family_chance: ImmigrationFamilyChance::default(),
            migrant_starting_gold: GoldRange::default(),
            guard_wage_per_hour: guard,
            healer_wage_per_hour: healer,
            scavenger_wage_per_hour: scavenger,
        };
        db.save_area_data(area).expect("save area");
        area_id
    }

    /// Sim config for a jobless migrant: no work room, no shop, just a home.
    fn make_jobless_config(home_vnum: &str) -> SimulationConfig {
        SimulationConfig {
            home_room_vnum: home_vnum.to_string(),
            work_room_vnum: String::new(),
            shop_room_vnum: String::new(),
            preferred_food_vnum: String::new(),
            work_pay: 0,
            work_start_hour: 8,
            work_end_hour: 17,
            hunger_decay_rate: 0,
            energy_decay_rate: 0,
            comfort_decay_rate: 0,
            low_gold_threshold: 10,
        }
    }

    fn cooldown_elapsed_now() -> i64 {
        BANK_VISIT_COOLDOWN_SECS + 1
    }

    fn satisfied_needs() -> NeedsState {
        NeedsState {
            hunger: 80,
            energy: 80,
            comfort: 80,
            ..Default::default()
        }
    }

    #[test]
    fn decide_goal_picks_seek_bank_for_broke_jobless_migrant_with_bank_in_area() {
        let g = open_db("seek_bank_picks");
        let area_id = make_area_with_wages(&g.db, "bk1", 0, 0, 0);

        let mut home = save_room(&g.db, "bk1:home");
        home.area_id = Some(area_id);
        g.db.save_room_data(home.clone()).expect("re-save home");

        let mut bank = save_room(&g.db, "bk1:bank");
        bank.area_id = Some(area_id);
        bank.flags.bank = true;
        g.db.save_room_data(bank.clone()).expect("re-save bank");

        let mut mobile = MobileData::new("Pip".into());
        mobile.gold = 0;
        mobile.resident_of = Some("bk1:home".into());
        mobile.current_room_id = Some(home.id);

        let goal = decide_goal(
            &g.db,
            &mobile,
            &make_jobless_config("bk1:home"),
            &satisfied_needs(),
            22,
            cooldown_elapsed_now(),
        )
        .expect("decide");
        assert_eq!(goal, SimGoal::SeekBank);
    }

    #[test]
    fn decide_goal_skips_seek_bank_when_cooldown_active() {
        let g = open_db("seek_bank_cooldown");
        let area_id = make_area_with_wages(&g.db, "bk2", 0, 0, 0);

        let mut home = save_room(&g.db, "bk2:home");
        home.area_id = Some(area_id);
        g.db.save_room_data(home.clone()).expect("re-save home");

        let mut bank = save_room(&g.db, "bk2:bank");
        bank.area_id = Some(area_id);
        bank.flags.bank = true;
        g.db.save_room_data(bank.clone()).expect("re-save bank");

        let mut mobile = MobileData::new("Pip".into());
        mobile.gold = 0;
        mobile.resident_of = Some("bk2:home".into());
        mobile.current_room_id = Some(home.id);

        let now = 1_000_000;
        let mut needs = satisfied_needs();
        needs.last_bank_visit_attempt = now; // just visited

        let goal = decide_goal(&g.db, &mobile, &make_jobless_config("bk2:home"), &needs, 22, now).expect("decide");
        assert_ne!(goal, SimGoal::SeekBank, "cooldown should suppress SeekBank");
    }

    #[test]
    fn decide_goal_skips_seek_bank_when_no_bank_room_exists() {
        let g = open_db("seek_bank_no_room");
        let area_id = make_area_with_wages(&g.db, "bk3", 0, 0, 0);

        let mut home = save_room(&g.db, "bk3:home");
        home.area_id = Some(area_id);
        g.db.save_room_data(home.clone()).expect("re-save home");
        // Note: no room with flags.bank in the area.

        let mut mobile = MobileData::new("Pip".into());
        mobile.gold = 0;
        mobile.resident_of = Some("bk3:home".into());
        mobile.current_room_id = Some(home.id);

        let goal = decide_goal(
            &g.db,
            &mobile,
            &make_jobless_config("bk3:home"),
            &satisfied_needs(),
            22,
            cooldown_elapsed_now(),
        )
        .expect("decide");
        assert_ne!(goal, SimGoal::SeekBank);
    }

    #[test]
    fn decide_goal_skips_seek_bank_for_wage_earning_guard() {
        let g = open_db("seek_bank_guard");
        let area_id = make_area_with_wages(&g.db, "bk4", 25, 0, 0);

        let mut home = save_room(&g.db, "bk4:home");
        home.area_id = Some(area_id);
        g.db.save_room_data(home.clone()).expect("re-save home");

        let mut bank = save_room(&g.db, "bk4:bank");
        bank.area_id = Some(area_id);
        bank.flags.bank = true;
        g.db.save_room_data(bank.clone()).expect("re-save bank");

        let mut guard = MobileData::new("Garron".into());
        guard.flags.guard = true;
        guard.gold = 0;
        guard.resident_of = Some("bk4:home".into());
        guard.current_room_id = Some(home.id);

        let goal = decide_goal(
            &g.db,
            &guard,
            &make_jobless_config("bk4:home"),
            &satisfied_needs(),
            22,
            cooldown_elapsed_now(),
        )
        .expect("decide");
        assert_ne!(
            goal,
            SimGoal::SeekBank,
            "wage-earning guard should not seek bank handouts"
        );
    }

    #[test]
    fn bank_arrival_grants_gold_and_clears_goal() {
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};

        let g = open_db("seek_bank_arrival");
        let area_id = make_area_with_wages(&g.db, "bk5", 0, 0, 0);

        let mut home = save_room(&g.db, "bk5:home");
        home.area_id = Some(area_id);
        g.db.save_room_data(home.clone()).expect("re-save home");

        let mut bank = save_room(&g.db, "bk5:bank");
        bank.area_id = Some(area_id);
        bank.flags.bank = true;
        g.db.save_room_data(bank.clone()).expect("re-save bank");

        let mut mobile = MobileData::new("Pip".into());
        mobile.gold = 0;
        mobile.resident_of = Some("bk5:home".into());
        mobile.current_room_id = Some(bank.id); // arrived at the bank

        let mut needs = satisfied_needs();
        needs.current_goal = SimGoal::SeekBank;

        let connections: SharedConnections = Arc::new(Mutex::new(HashMap::new()));
        let now = 1_234_567;
        execute_arrival_actions(&g.db, &connections, &mut mobile, &make_jobless_config("bk5:home"), &mut needs, now)
            .expect("arrival");

        assert_eq!(mobile.gold, BANK_RELIEF_AMOUNT, "bank should grant relief amount");
        assert_eq!(needs.current_goal, SimGoal::Idle, "goal cleared after handout");
        assert_eq!(needs.last_bank_visit_attempt, now, "cooldown stamp set");
        assert!(mobile.routine_destination_room.is_none(), "destination cleared");
    }

    #[test]
    fn has_recent_topic_detects_fatigue_per_partner() {
        use ironmud::{Relationship, RelationshipKind};

        let mut mobile = MobileData::new("Initiator".to_string());
        let partner_id = uuid::Uuid::new_v4();
        let other_id = uuid::Uuid::new_v4();
        mobile.relationships.push(Relationship {
            other_id: partner_id,
            kind: RelationshipKind::Friend,
            affinity: 10,
            last_interaction_day: 0,
            recent_topics: vec!["fishing".into(), "weather".into()],
        });

        assert!(has_recent_topic(&mobile, partner_id, "fishing"));
        assert!(!has_recent_topic(&mobile, partner_id, "politics"));
        // Fatigue is scoped per-partner: the same topic with a different mobile is fresh.
        assert!(!has_recent_topic(&mobile, other_id, "fishing"));
    }
}

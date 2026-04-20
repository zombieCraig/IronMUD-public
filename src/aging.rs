//! Aging system: advances `Characteristics.age` from `birth_day` once per
//! game day and rolls old-age death for Elderly mobiles. The async runner
//! lives in `src/ticks/aging.rs`; this module holds the sync core so
//! integration tests can drive it without tokio.

use anyhow::Result;
use rand::Rng;
use std::path::Path;
use tracing::{debug, info};

use crate::SharedConnections;
use crate::db::Db;
use crate::migration::{MigrationData, absolute_game_day, load_migration_data};
use crate::types::{GAME_DAYS_PER_YEAR, LifeStage, MobileData, age_label_for_stage, life_stage_for_age};

/// Setting key for the last absolute game day the aging tick processed.
pub const AGING_LAST_CHECK_KEY: &str = "aging_last_check_day";

/// Setting key for the per-day conception probability. Default 0.005.
pub const CONCEPTION_CHANCE_KEY: &str = "conception_chance_per_day";
/// Default conception chance if the setting is unset.
pub const DEFAULT_CONCEPTION_CHANCE: f32 = 0.005;

/// Setting key for the per-day adoption probability. Default 0.10 —
/// generous because stale orphans are worse than over-placement.
pub const ADOPTION_CHANCE_KEY: &str = "adoption_chance_per_day";
pub const DEFAULT_ADOPTION_CHANCE: f32 = 0.10;

/// Synchronous core. Uses `rand::thread_rng()` for natural-death rolls and
/// loads migration data from `scripts/data` (required by the birth pass —
/// newborns sample from the mother's area name/visual pools).
pub fn process_aging_tick(db: &Db, connections: &SharedConnections) -> Result<()> {
    let data = load_migration_data(Path::new("scripts/data"))?;
    let mut rng = rand::thread_rng();
    process_aging_tick_with_rng(db, connections, &data, &mut rng)
}

/// Core with injectable RNG and MigrationData for deterministic tests.
pub fn process_aging_tick_with_rng<R: Rng>(
    db: &Db,
    _connections: &SharedConnections,
    data: &MigrationData,
    rng: &mut R,
) -> Result<()> {
    let game_time = db.get_game_time()?;
    let today = absolute_game_day(game_time.year, game_time.month, game_time.day);

    let last_check = read_last_check_day(db)?;
    if let Some(prev) = last_check {
        if today <= prev {
            return Ok(());
        }
    }

    let mobiles = db.list_all_mobiles()?;
    let mut aged = 0usize;
    let mut deaths = 0usize;

    for mobile in mobiles {
        if mobile.is_prototype {
            continue;
        }
        if mobile.characteristics.is_none() {
            continue;
        }
        if mobile.current_hp <= 0 || mobile.combat.in_combat {
            continue;
        }

        let mobile_id = mobile.id;
        let today_i32 = today as i32;
        let snapshot = match db.update_mobile(&mobile_id, |m| {
            apply_aging(m, today);
            crate::social::prune_bereavement_notes(m, today_i32);
        })? {
            Some(m) => m,
            None => continue,
        };

        let chars = match snapshot.characteristics.as_ref() {
            Some(c) => c,
            None => continue,
        };

        if chars.birth_day != 0 {
            aged += 1;
        }

        if matches!(life_stage_for_age(chars.age), LifeStage::Elderly) && roll_natural_death(chars.age, rng) {
            if db.delete_mobile(&mobile_id).unwrap_or(false) {
                deaths += 1;
                info!("Aging: {} (age {}) died of natural causes", snapshot.name, chars.age);
            }
        }
    }

    // Conception + birth passes run AFTER the aging/death loop so a mother
    // who just gave birth today doesn't also get a death roll on the child's
    // first day (newborn is 0 years old — can't die of old age anyway, but
    // keeps the ordering intuitive).
    let births = process_pregnancy_passes(db, data, today, rng)?;
    let adoptions = process_adoption_pass(db, today, rng)?;

    write_last_check_day(db, today)?;
    debug!(
        "Aging tick: day {} processed ({} mobiles, {} deaths, {} births, {} adoptions)",
        today, aged, deaths, births, adoptions
    );
    Ok(())
}

/// Conception + birth passes. Returns the number of births that occurred.
fn process_pregnancy_passes<R: Rng>(db: &Db, data: &MigrationData, today: i64, rng: &mut R) -> Result<usize> {
    let today_i32 = today as i32;
    let conception_chance = read_conception_chance(db);

    // Conception pass FIRST: a currently-pregnant mother fails
    // `is_fertile_female` (pregnant_until_day.is_some()), so she's skipped
    // here and her due-today birth fires below. If we ran birth first and
    // conception second, a mother who just gave birth today would
    // immediately re-conceive.
    for mobile in db.list_all_mobiles()? {
        if !crate::social::is_fertile_female(&mobile, today_i32) {
            continue;
        }
        let father_id = match crate::social::eligible_mate(db, &mobile) {
            Some(id) => id,
            None => continue,
        };
        let father = match db.get_mobile_data(&father_id)? {
            Some(f) => f,
            None => continue,
        };
        if !crate::social::is_fertile_male(&father) {
            continue;
        }
        if rng.r#gen::<f32>() >= conception_chance {
            continue;
        }
        let due = today_i32 + PREGNANCY_GESTATION_DAYS;
        let _ = db.update_mobile(&mobile.id, |m| {
            if let Some(s) = m.social.as_mut() {
                s.pregnant_until_day = Some(due);
                s.pregnant_by = Some(father_id);
            }
        });
        info!("Conception: {} is expecting (due day {})", mobile.name, due);
    }

    // Birth pass: any pregnancy whose due day has arrived.
    let mut births = 0usize;
    for mobile in db.list_all_mobiles()? {
        let Some(social) = mobile.social.as_ref() else {
            continue;
        };
        let Some(due) = social.pregnant_until_day else {
            continue;
        };
        if due > today_i32 {
            continue;
        }
        let mother_id = mobile.id;
        let father_id = social.pregnant_by;
        match crate::migration::spawn_child(db, data, mother_id, father_id) {
            Ok(_) => {
                births += 1;
                info!("Birth: {} delivered a newborn (day {})", mobile.name, today);
            }
            Err(e) => {
                tracing::warn!("spawn_child failed for {}: {}", mobile.name, e);
            }
        }
    }

    Ok(births)
}

/// Game-day gestation length. 60 days = 2 game months.
pub const PREGNANCY_GESTATION_DAYS: i32 = 60;

fn read_conception_chance(db: &Db) -> f32 {
    match db.get_setting(CONCEPTION_CHANCE_KEY).ok().flatten() {
        Some(s) => s.parse::<f32>().unwrap_or(DEFAULT_CONCEPTION_CHANCE),
        None => DEFAULT_CONCEPTION_CHANCE,
    }
}

fn read_adoption_chance(db: &Db) -> f32 {
    match db.get_setting(ADOPTION_CHANCE_KEY).ok().flatten() {
        Some(s) => s.parse::<f32>().unwrap_or(DEFAULT_ADOPTION_CHANCE),
        None => DEFAULT_ADOPTION_CHANCE,
    }
}

/// Iterate orphans (`adoption_pending == true`) and attempt placement with a
/// weighted roll over eligible adopters. Returns the number of adoptions
/// that succeeded this tick.
fn process_adoption_pass<R: Rng>(db: &Db, _today: i64, rng: &mut R) -> Result<usize> {
    let base_chance = read_adoption_chance(db);
    if base_chance <= 0.0 {
        return Ok(0);
    }
    let mobiles = db.list_all_mobiles()?;
    let orphans: Vec<MobileData> = mobiles
        .iter()
        .filter(|m| m.adoption_pending && m.current_hp > 0 && !m.is_prototype)
        .cloned()
        .collect();
    if orphans.is_empty() {
        return Ok(0);
    }

    let mut adoptions = 0usize;
    for orphan in orphans {
        // Daily base roll — does ANY adopter get a shot at this orphan today?
        if rng.r#gen::<f32>() >= base_chance {
            continue;
        }
        if let Some(adopter_id) = crate::social::pick_adopter(db, &orphan, &mobiles, rng) {
            match crate::social::wire_adoption(db, adopter_id, orphan.id) {
                Ok(()) => {
                    adoptions += 1;
                    info!("Adoption: {} adopted by mobile id {}", orphan.name, adopter_id);
                }
                Err(e) => {
                    tracing::warn!("wire_adoption failed for orphan {}: {}", orphan.name, e);
                }
            }
        }
    }
    Ok(adoptions)
}

/// Refresh `age`, `age_label`, and back-fill `birth_day` for legacy saves.
/// Pure mutation on the mobile — callers drive persistence via `update_mobile`.
pub fn apply_aging(mobile: &mut MobileData, today: i64) {
    let chars = match mobile.characteristics.as_mut() {
        Some(c) => c,
        None => return,
    };

    if chars.birth_day == 0 && chars.age > 0 {
        // Legacy save: back-compute birth_day from stored age so future ticks
        // advance naturally from here.
        chars.birth_day = today - (chars.age as i64) * GAME_DAYS_PER_YEAR;
    } else if chars.birth_day == 0 && chars.age == 0 {
        // Brand-new 0-year-old (Phase D newborn): birthday = today.
        chars.birth_day = today;
    }

    let derived_age = ((today - chars.birth_day) / GAME_DAYS_PER_YEAR).max(0) as i32;
    if derived_age != chars.age {
        chars.age = derived_age;
    }
    let derived_label = age_label_for_stage(life_stage_for_age(chars.age)).to_string();
    if derived_label != chars.age_label {
        chars.age_label = derived_label;
    }
}

/// Per-game-day probability an Elderly mobile dies of natural causes. Zero
/// under 70, linear to ~0.83%/day at 95, jumps to 5%/day at 100+.
pub fn death_probability_per_game_day(age: i32) -> f32 {
    if age < 70 {
        return 0.0;
    }
    if age >= 100 {
        return 0.05;
    }
    ((age - 70) as f32 / 3000.0).clamp(0.0, 0.02)
}

pub fn roll_natural_death<R: Rng>(age: i32, rng: &mut R) -> bool {
    let p = death_probability_per_game_day(age);
    if p <= 0.0 {
        return false;
    }
    rng.r#gen::<f32>() < p
}

fn read_last_check_day(db: &Db) -> Result<Option<i64>> {
    match db.get_setting(AGING_LAST_CHECK_KEY)? {
        Some(s) => Ok(s.parse::<i64>().ok()),
        None => Ok(None),
    }
}

fn write_last_check_day(db: &Db, day: i64) -> Result<()> {
    db.set_setting(AGING_LAST_CHECK_KEY, &day.to_string())
}

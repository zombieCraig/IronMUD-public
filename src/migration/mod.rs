//! Migrant / emergent population system.
//!
//! Loads name pools and visual profiles from `scripts/data/names/` and
//! `scripts/data/visuals/`, and provides helpers for generating migrant
//! mobiles and releasing residency on mobile death. The actual migration
//! tick lives in `crate::ticks::migration`.

use anyhow::{Context, Result};
use rand::Rng;
use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::SharedConnections;
use crate::db::Db;
use crate::session::{broadcast_to_builders, broadcast_to_room_awake};
use crate::types::{
    AreaData, Characteristics, MobileData, MobileFlags, NeedsState, Relationship, RelationshipKind, RoomData,
    SimulationConfig, SocialState,
};

pub mod variations;

/// A pool of first and last names for generated migrants.
#[derive(Debug, Clone, Deserialize)]
pub struct NamePool {
    #[serde(default)]
    pub male_first: Vec<String>,
    #[serde(default)]
    pub female_first: Vec<String>,
    #[serde(default)]
    pub last: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgeRange {
    pub label: String,
    pub min: i32,
    pub max: i32,
}

/// A visual profile: pools of allowed values for generated characteristics.
#[derive(Debug, Clone, Deserialize)]
pub struct VisualProfile {
    #[serde(default)]
    pub hair_colors: Vec<String>,
    #[serde(default)]
    pub hair_styles: Vec<String>,
    #[serde(default)]
    pub eye_colors: Vec<String>,
    #[serde(default)]
    pub skin_tones: Vec<String>,
    #[serde(default)]
    pub heights: Vec<String>,
    #[serde(default)]
    pub builds: Vec<String>,
    #[serde(default)]
    pub age_ranges: Vec<AgeRange>,
    #[serde(default)]
    pub marks: Vec<String>,
    #[serde(default)]
    pub mark_chance: f32,
}

/// Cached name/visual data loaded from `scripts/data/`.
#[derive(Debug, Default, Clone)]
pub struct MigrationData {
    pub name_pools: HashMap<String, NamePool>,
    pub visual_profiles: HashMap<String, VisualProfile>,
    /// Global pool of conversation topics shared by all simulated mobiles.
    pub topics: Vec<String>,
}

impl MigrationData {
    pub fn name_pool(&self, key: &str) -> Option<&NamePool> {
        self.name_pools.get(key)
    }

    pub fn visual_profile(&self, key: &str) -> Option<&VisualProfile> {
        self.visual_profiles.get(key)
    }

    pub fn topics(&self) -> &[String] {
        &self.topics
    }
}

#[derive(Debug, Clone, Deserialize)]
struct TopicPoolFile {
    #[serde(default)]
    topics: Vec<String>,
}

/// Load all `*.json` name pools from `<data_dir>/names/`.
pub fn load_name_pools(data_dir: &Path) -> Result<HashMap<String, NamePool>> {
    load_json_dir(&data_dir.join("names"))
}

/// Load all `*.json` visual profiles from `<data_dir>/visuals/`.
pub fn load_visual_profiles(data_dir: &Path) -> Result<HashMap<String, VisualProfile>> {
    load_json_dir(&data_dir.join("visuals"))
}

fn load_json_dir<T: for<'de> Deserialize<'de>>(dir: &Path) -> Result<HashMap<String, T>> {
    let mut out = HashMap::new();
    if !dir.exists() {
        return Ok(out);
    }
    let entries = std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let key = match path.file_stem().and_then(|s| s.to_str()) {
            Some(k) => k.to_string(),
            None => continue,
        };
        let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let parsed: T = serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        out.insert(key, parsed);
    }
    Ok(out)
}

/// Load the global conversation topic pool from `<data_dir>/social/topics.json`.
/// Missing file is not an error — topics just remain empty (simulation treats
/// that as "no conversations").
pub fn load_topic_pool(data_dir: &Path) -> Result<Vec<String>> {
    let path = data_dir.join("social").join("topics.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let parsed: TopicPoolFile = serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    Ok(parsed.topics)
}

/// Load both pools and profiles from the canonical `scripts/data/` directory.
pub fn load_migration_data(data_dir: &Path) -> Result<MigrationData> {
    Ok(MigrationData {
        name_pools: load_name_pools(data_dir)?,
        visual_profiles: load_visual_profiles(data_dir)?,
        topics: load_topic_pool(data_dir)?,
    })
}

// ---------------------------------------------------------------------------
// Generation helpers
// ---------------------------------------------------------------------------

/// Generate random visual characteristics from a profile.
/// Returns the struct plus a templated natural-language description.
pub fn generate_characteristics<R: Rng>(
    profile: &VisualProfile,
    gender: &str,
    rng: &mut R,
) -> (Characteristics, String) {
    let hair_color = pick(&profile.hair_colors, rng).unwrap_or("dark");
    let hair_style = pick(&profile.hair_styles, rng).unwrap_or("neat");
    let eye_color = pick(&profile.eye_colors, rng).unwrap_or("brown");
    let skin_tone = pick(&profile.skin_tones, rng).unwrap_or("fair");
    let height = pick(&profile.heights, rng).unwrap_or("average");
    let build = pick(&profile.builds, rng).unwrap_or("average");

    // Migrants are always adults — juvenile `age_ranges` entries exist so
    // `build_migrant_family` (parent+child) and spawn_child (Phase D) can
    // draw from the same profile, but immigration never spawns a baby/child/
    // adolescent as a standalone migrant. Filter by the stage of the range's
    // minimum age.
    let adult_ranges: Vec<&AgeRange> = profile.age_ranges.iter().filter(|r| !is_juvenile_range(r)).collect();
    let (age, age_label) = if adult_ranges.is_empty() {
        (30, "adult".to_string())
    } else {
        let range = adult_ranges.choose(rng).unwrap();
        let min = range.min.min(range.max);
        let max = range.max.max(range.min);
        let age = if max > min { rng.gen_range(min..=max) } else { min };
        (age, range.label.clone())
    };

    let mark = if !profile.marks.is_empty() && rng.r#gen::<f32>() < profile.mark_chance {
        profile.marks.choose(rng).map(|s| s.to_string())
    } else {
        None
    };

    let chars = Characteristics {
        gender: gender.to_string(),
        age,
        age_label: age_label.clone(),
        birth_day: 0, // populated by build_migrant when the mobile is placed
        height: height.to_string(),
        build: build.to_string(),
        hair_color: hair_color.to_string(),
        hair_style: hair_style.to_string(),
        eye_color: eye_color.to_string(),
        skin_tone: skin_tone.to_string(),
        distinguishing_mark: mark.clone(),
    };

    let pronoun_subj = if gender == "female" { "She" } else { "He" };
    let gender_noun = if gender == "female" { "woman" } else { "man" };

    let mark_sentence = match &mark {
        Some(m) => format!(" Notable: {}.", m),
        None => String::new(),
    };

    let description = format!(
        "A {height} {build} {gender_noun}, {age_label}, with {skin_tone} skin. \
         {pronoun} has {hair_style} {hair_color} hair and {eye_color} eyes.{mark}",
        height = height,
        build = build,
        gender_noun = gender_noun,
        age_label = age_label,
        skin_tone = skin_tone,
        pronoun = pronoun_subj,
        hair_style = hair_style,
        hair_color = hair_color,
        eye_color = eye_color,
        mark = mark_sentence,
    );

    (chars, description)
}

fn pick<'a, R: Rng>(pool: &'a [String], rng: &mut R) -> Option<&'a str> {
    pool.choose(rng).map(|s| s.as_str())
}

/// True if the given age range represents a juvenile life stage. Used to
/// separate child-only generation from the default adult-only migration path.
pub fn is_juvenile_range(range: &AgeRange) -> bool {
    matches!(
        crate::types::life_stage_for_age(range.min.min(range.max)),
        crate::types::LifeStage::Baby | crate::types::LifeStage::Child | crate::types::LifeStage::Adolescent
    )
}

/// Generate a juvenile (Baby/Child/Adolescent) set of characteristics.
/// Mirrors `generate_characteristics` but samples only from juvenile age
/// ranges. Returns `None` if the profile has no juvenile ranges configured.
pub fn generate_child_characteristics<R: Rng>(
    profile: &VisualProfile,
    gender: &str,
    rng: &mut R,
) -> Option<(Characteristics, String)> {
    let juvenile_ranges: Vec<&AgeRange> = profile.age_ranges.iter().filter(|r| is_juvenile_range(r)).collect();
    if juvenile_ranges.is_empty() {
        return None;
    }
    let hair_color = pick(&profile.hair_colors, rng).unwrap_or("dark");
    let hair_style = pick(&profile.hair_styles, rng).unwrap_or("neat");
    let eye_color = pick(&profile.eye_colors, rng).unwrap_or("brown");
    let skin_tone = pick(&profile.skin_tones, rng).unwrap_or("fair");
    let height = pick(&profile.heights, rng).unwrap_or("average");
    let build = pick(&profile.builds, rng).unwrap_or("average");

    let range = juvenile_ranges.choose(rng).unwrap();
    let min = range.min.min(range.max);
    let max = range.max.max(range.min);
    let age = if max > min { rng.gen_range(min..=max) } else { min };
    let age_label = range.label.clone();

    let chars = Characteristics {
        gender: gender.to_string(),
        age,
        age_label: age_label.clone(),
        birth_day: 0,
        height: height.to_string(),
        build: build.to_string(),
        hair_color: hair_color.to_string(),
        hair_style: hair_style.to_string(),
        eye_color: eye_color.to_string(),
        skin_tone: skin_tone.to_string(),
        distinguishing_mark: None,
    };

    let pronoun_subj = if gender == "female" { "She" } else { "He" };
    let gender_noun = if gender == "female" { "girl" } else { "boy" };
    let description = format!(
        "A {height} {gender_noun}, {age_label}, with {skin_tone} skin. \
         {pronoun} has {hair_style} {hair_color} hair and {eye_color} eyes.",
        height = height,
        gender_noun = gender_noun,
        age_label = age_label,
        skin_tone = skin_tone,
        pronoun = pronoun_subj,
        hair_style = hair_style,
        hair_color = hair_color,
        eye_color = eye_color,
    );

    Some((chars, description))
}

/// Generate a first + last name from a pool, best-effort avoiding duplicates
/// already in `existing_names`. Returns the full "First Last" string.
pub fn generate_name<R: Rng>(pool: &NamePool, gender: &str, existing_names: &HashSet<String>, rng: &mut R) -> String {
    let first_pool: &[String] = if gender == "female" {
        &pool.female_first
    } else {
        &pool.male_first
    };

    for _ in 0..20 {
        let first = match first_pool.choose(rng) {
            Some(f) => f,
            None => "Stranger",
        };
        let last = match pool.last.choose(rng) {
            Some(l) => l,
            None => "",
        };
        let full = if last.is_empty() {
            first.to_string()
        } else {
            format!("{} {}", first, last)
        };
        if !existing_names.contains(&full) {
            return full;
        }
    }

    // Fallback: accept duplicate after retries
    let first = first_pool.choose(rng).map(|s| s.as_str()).unwrap_or("Stranger");
    let last = pool.last.choose(rng).map(|s| s.as_str()).unwrap_or("");
    if last.is_empty() {
        first.to_string()
    } else {
        format!("{} {}", first, last)
    }
}

/// Pick a gender ("male"/"female") with 50/50 odds.
pub fn random_gender<R: Rng>(rng: &mut R) -> &'static str {
    if rng.gen_bool(0.5) { "female" } else { "male" }
}

/// Build a fully-populated migrant `MobileData` instance ready to save.
/// Does not persist or place the mobile — callers handle that.
pub fn build_migrant<R: Rng>(
    area: &AreaData,
    home_room_vnum: &str,
    data: &MigrationData,
    existing_names: &HashSet<String>,
    rng: &mut R,
) -> Result<MobileData> {
    let name_pool = data
        .name_pool(&area.immigration_name_pool)
        .with_context(|| format!("unknown name pool '{}'", area.immigration_name_pool))?;
    let visual_profile = data
        .visual_profile(&area.immigration_visual_profile)
        .with_context(|| format!("unknown visual profile '{}'", area.immigration_visual_profile))?;

    let gender = random_gender(rng);
    let full_name = generate_name(name_pool, gender, existing_names, rng);
    let (chars, description) = generate_characteristics(visual_profile, gender, rng);

    let keywords: Vec<String> = full_name.split_whitespace().map(|s| s.to_lowercase()).collect();

    // Short desc: "Akio Tanaka, a young adult man, is here."
    let short_desc = format!(
        "{} is here, a {} {}.",
        full_name,
        chars.age_label,
        if gender == "female" { "woman" } else { "man" }
    );

    // Simulation: inherit area defaults if set, override home_room_vnum.
    let simulation = area
        .migrant_sim_defaults
        .as_ref()
        .map(|defaults| {
            let mut sim = defaults.clone();
            sim.home_room_vnum = home_room_vnum.to_string();
            sim
        })
        .or_else(|| {
            Some(SimulationConfig {
                home_room_vnum: home_room_vnum.to_string(),
                work_room_vnum: String::new(),
                shop_room_vnum: String::new(),
                preferred_food_vnum: String::new(),
                work_pay: 50,
                work_start_hour: 8,
                work_end_hour: 17,
                hunger_decay_rate: 0,
                energy_decay_rate: 0,
                comfort_decay_rate: 0,
                low_gold_threshold: 10,
            })
        });

    let mut mobile = MobileData::new(full_name.clone());
    mobile.is_prototype = false;
    // Migrants don't share a prototype vnum; use a synthetic per-instance marker
    // so builders can grep for "migrant:<area-prefix>" in tooling.
    mobile.vnum = format!("migrant:{}", area.prefix);
    mobile.short_desc = short_desc;
    mobile.long_desc = description;
    mobile.keywords = keywords;
    mobile.characteristics = Some(chars);
    mobile.simulation = simulation;
    mobile.needs = Some(NeedsState::default());
    mobile.resident_of = Some(home_room_vnum.to_string());
    mobile.flags = MobileFlags::default();

    let variation = variations::pick_variation(area, rng);
    if let Some(tag) = variation.vnum_tag() {
        mobile.vnum = format!("migrant:{}:{}", tag, area.prefix);
    }
    variations::apply_variation(&mut mobile, variation, rng);

    let mut social = roll_social_state(data.topics(), rng);
    variations::bias_social_for_variation(&mut social, variation, data.topics(), rng);
    mobile.social = Some(social);

    Ok(mobile)
}

/// Roll likes, dislikes, and starting happiness for a new migrant.
/// Samples without replacement from the global topic pool so a mobile never
/// both likes and dislikes the same topic. Caller gets a default state (empty
/// lists, happiness 50) when the pool is empty.
pub fn roll_social_state<R: Rng>(topics: &[String], rng: &mut R) -> SocialState {
    let mut state = SocialState::default();
    if topics.is_empty() {
        return state;
    }

    let mut shuffled: Vec<String> = topics.to_vec();
    shuffled.shuffle(rng);

    let like_count = rng.gen_range(2..=4).min(shuffled.len());
    let dislike_count = rng.gen_range(1..=3).min(shuffled.len().saturating_sub(like_count));

    state.likes = shuffled.drain(..like_count).collect();
    state.dislikes = shuffled.drain(..dislike_count).collect();
    state
}

// ---------------------------------------------------------------------------
// Residency lifecycle
// ---------------------------------------------------------------------------

/// Remove a mobile from the residents list of their `resident_of` room, if any.
/// Called on mobile death or deletion. Safe to call on mobiles with no residency.
pub fn release_residency(db: &Db, mobile: &MobileData) -> Result<()> {
    let vnum = match &mobile.resident_of {
        Some(v) if !v.is_empty() => v,
        _ => return Ok(()),
    };
    if let Some(mut room) = db.get_room_by_vnum(vnum)? {
        let before = room.residents.len();
        room.residents.retain(|id| *id != mobile.id);
        if room.residents.len() != before {
            db.save_room_data(room)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Game-time helpers
// ---------------------------------------------------------------------------

/// Absolute game-day counter: increases monotonically as in-game time advances.
/// Uses a 30-day month / 12-month year calendar (matches GameTime fields).
pub fn absolute_game_day(year: u32, month: u8, day: u8) -> i64 {
    let y = year as i64;
    let m = month.max(1) as i64;
    let d = day.max(1) as i64;
    y * 360 + (m - 1) * 30 + (d - 1)
}

// ---------------------------------------------------------------------------
// Migration tick core logic (sync; the async runner lives in src/ticks/)
// ---------------------------------------------------------------------------

/// Synchronous core of the migration tick. Exposed so integration tests can
/// drive it without spinning up the tokio runner.
pub fn process_migration_tick(db: &Db, connections: &SharedConnections, data: &MigrationData) -> Result<()> {
    let game_time = db.get_game_time()?;
    let current_day = absolute_game_day(game_time.year, game_time.month, game_time.day);

    let areas = db.list_all_areas()?;
    for mut area in areas {
        if !area.immigration_enabled {
            continue;
        }
        if area.migration_interval_days == 0 {
            continue;
        }
        let due = match area.last_migration_check_day {
            Some(prev) => current_day - prev >= area.migration_interval_days as i64,
            None => true,
        };
        if !due {
            continue;
        }

        let spawned = run_area_migration(db, connections, &area, data)?;
        area.last_migration_check_day = Some(current_day);
        db.save_area_data(area.clone())?;

        if spawned > 0 {
            info!(
                "Migration: area '{}' gained {} new migrants (day {})",
                area.prefix, spawned, current_day
            );
        } else {
            debug!(
                "Migration: area '{}' checked (day {}) — no migrants spawned",
                area.prefix, current_day
            );
        }
    }

    // Pair bonding / breakup / rehousing passes — not gated per-area because
    // relationships accumulate every simulation tick even when no migration is
    // due.
    if let Err(e) = process_pair_housing(db) {
        warn!("Pair-housing pass failed: {}", e);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Pair housing: cohabitation, breakup, homeless rehousing
// ---------------------------------------------------------------------------

/// Affinity threshold at which two mobiles want to live together.
pub const COHAB_AFFINITY_THRESHOLD: i32 = 80;
/// Affinity threshold below which a Cohabitant pair breaks up.
pub const BREAKUP_AFFINITY_THRESHOLD: i32 = -60;

/// Run a single housing pass: promote high-affinity pairs into cohabitation,
/// split Cohabitant pairs whose affinity has cratered, and place homeless
/// mobiles into any liveable room that has opened up.
///
/// This is a separate pub fn so tests can drive it directly without running
/// the full migration tick.
pub fn process_pair_housing(db: &Db) -> Result<()> {
    let mobiles = db.list_all_mobiles()?;

    // Snapshot the world once; all mutations go through db.save_* calls below.
    let simulated: Vec<MobileData> = mobiles
        .into_iter()
        .filter(|m| !m.is_prototype && m.current_hp > 0 && m.social.is_some())
        .collect();

    process_breakups(db, &simulated)?;
    process_cohabitations(db, &simulated)?;
    process_homeless_rehouse(db, &simulated)?;

    Ok(())
}

fn process_cohabitations(db: &Db, mobiles: &[MobileData]) -> Result<()> {
    use std::collections::HashSet;
    let mut handled_pairs: HashSet<(Uuid, Uuid)> = HashSet::new();

    for mobile in mobiles {
        let Some(my_vnum) = mobile.resident_of.clone().filter(|v| !v.is_empty()) else {
            continue;
        };
        if bereaved(mobile) {
            continue;
        }

        for rel in &mobile.relationships {
            if rel.affinity < COHAB_AFFINITY_THRESHOLD {
                continue;
            }
            if matches!(rel.kind, RelationshipKind::Cohabitant) {
                continue;
            }

            let key = ordered_pair(mobile.id, rel.other_id);
            if !handled_pairs.insert(key) {
                continue;
            }

            let Some(other) = mobiles.iter().find(|m| m.id == rel.other_id) else {
                continue;
            };
            if bereaved(other) {
                continue;
            }
            let Some(other_vnum) = other.resident_of.clone().filter(|v| !v.is_empty()) else {
                continue;
            };
            if other_vnum == my_vnum {
                continue; // already sharing
            }
            if !other
                .relationships
                .iter()
                .any(|r| r.other_id == mobile.id && r.affinity >= COHAB_AFFINITY_THRESHOLD)
            {
                continue; // feeling isn't mutual yet
            }

            attempt_cohabitation_merge(db, mobile, &my_vnum, other, &other_vnum)?;
        }
    }
    Ok(())
}

fn attempt_cohabitation_merge(db: &Db, a: &MobileData, a_vnum: &str, b: &MobileData, b_vnum: &str) -> Result<()> {
    let Some(room_a) = db.get_room_by_vnum(a_vnum)? else {
        return Ok(());
    };
    let Some(room_b) = db.get_room_by_vnum(b_vnum)? else {
        return Ok(());
    };

    let free_a = (room_a.living_capacity as usize).saturating_sub(room_a.residents.len());
    let free_b = (room_b.living_capacity as usize).saturating_sub(room_b.residents.len());

    // Pick the room with more headroom as the shared home; the other partner
    // moves in. If neither has a spare slot beyond its current occupant, skip.
    let (mover, mover_old_vnum, stay_vnum) = if free_a >= 1 && free_a >= free_b {
        (b, b_vnum.to_string(), a_vnum.to_string())
    } else if free_b >= 1 {
        (a, a_vnum.to_string(), b_vnum.to_string())
    } else {
        return Ok(());
    };

    // Remove mover from old room residents, add to new room residents.
    if let Some(mut old) = db.get_room_by_vnum(&mover_old_vnum)? {
        old.residents.retain(|id| *id != mover.id);
        db.save_room_data(old)?;
    }
    if let Some(mut shared) = db.get_room_by_vnum(&stay_vnum)? {
        if !shared.residents.contains(&mover.id) {
            shared.residents.push(mover.id);
        }
        db.save_room_data(shared)?;
    }

    // Decide the shared household_id BEFORE the two updates so both sides
    // converge on the same Uuid. Preference order: whichever side already
    // has a household keeps theirs; otherwise mint fresh. Enables pregnancy
    // on Cohabitant pairs (requires shared household).
    let household_id = a.household_id.or(b.household_id).unwrap_or_else(Uuid::new_v4);

    // Promote both sides' relationship to Cohabitant.
    let a_id = a.id;
    let b_id = b.id;
    let stay_for_mover = stay_vnum.clone();
    db.update_mobile(&mover.id, |m| {
        m.resident_of = Some(stay_for_mover.clone());
        m.household_id = Some(household_id);
        promote_to_cohabitant(m, if mover.id == a_id { b_id } else { a_id });
    })?;
    let stayer_id = if mover.id == a_id { b_id } else { a_id };
    db.update_mobile(&stayer_id, |m| {
        m.household_id = Some(household_id);
        promote_to_cohabitant(m, mover.id);
    })?;

    Ok(())
}

fn process_breakups(db: &Db, mobiles: &[MobileData]) -> Result<()> {
    use std::collections::HashSet;
    let mut handled: HashSet<(Uuid, Uuid)> = HashSet::new();

    for mobile in mobiles {
        for rel in &mobile.relationships {
            if !matches!(rel.kind, RelationshipKind::Cohabitant) {
                continue;
            }
            if rel.affinity > BREAKUP_AFFINITY_THRESHOLD {
                continue;
            }
            let key = ordered_pair(mobile.id, rel.other_id);
            if !handled.insert(key) {
                continue;
            }

            let Some(other) = mobiles.iter().find(|m| m.id == rel.other_id) else {
                continue;
            };

            // Loser = lower-affinity-toward-the-other side, falls back to `mobile`.
            let other_affinity_for_mobile = other
                .relationships
                .iter()
                .find(|r| r.other_id == mobile.id)
                .map(|r| r.affinity)
                .unwrap_or(0);
            let mover = if rel.affinity <= other_affinity_for_mobile {
                mobile
            } else {
                other
            };
            let stayer = if mover.id == mobile.id { other } else { mobile };

            breakup(db, mover, stayer)?;
        }
    }
    Ok(())
}

fn breakup(db: &Db, mover: &MobileData, stayer: &MobileData) -> Result<()> {
    let shared_vnum = match mover.resident_of.clone().filter(|v| !v.is_empty()) {
        Some(v) => v,
        None => return Ok(()),
    };

    // Try to find a free liveable room elsewhere.
    let new_home = find_free_liveable_room(db, &shared_vnum)?;

    // Remove mover from the shared room.
    if let Some(mut shared) = db.get_room_by_vnum(&shared_vnum)? {
        shared.residents.retain(|id| *id != mover.id);
        db.save_room_data(shared)?;
    }

    let new_resident_of = match new_home {
        Some(mut room) => {
            room.residents.push(mover.id);
            let vnum = room.vnum.clone();
            db.save_room_data(room)?;
            vnum
        }
        None => None, // homeless
    };

    let mover_id = mover.id;
    let stayer_id = stayer.id;
    db.update_mobile(&mover_id, |m| {
        m.resident_of = new_resident_of.clone();
        demote_from_cohabitant(m, stayer_id);
    })?;
    db.update_mobile(&stayer_id, |m| {
        demote_from_cohabitant(m, mover_id);
    })?;
    Ok(())
}

fn process_homeless_rehouse(db: &Db, mobiles: &[MobileData]) -> Result<()> {
    for mobile in mobiles {
        if mobile.resident_of.as_deref().map_or(false, |v| !v.is_empty()) {
            continue;
        }
        let Some(mut room) = find_free_liveable_room(db, "")? else {
            return Ok(()); // no rooms at all; done for this pass
        };
        room.residents.push(mobile.id);
        let assigned_vnum = room.vnum.clone();
        db.save_room_data(room)?;
        let id = mobile.id;
        db.update_mobile(&id, |m| {
            m.resident_of = assigned_vnum.clone();
        })?;
    }
    Ok(())
}

fn find_free_liveable_room(db: &Db, exclude_vnum: &str) -> Result<Option<RoomData>> {
    let areas = db.list_all_areas()?;
    for area in areas {
        let rooms = db.get_rooms_in_area(&area.id)?;
        for room in rooms {
            if !room.flags.liveable || room.living_capacity <= 0 {
                continue;
            }
            if (room.living_capacity as usize) <= room.residents.len() {
                continue;
            }
            if let Some(vnum) = &room.vnum {
                if vnum == exclude_vnum {
                    continue;
                }
            }
            return Ok(Some(room));
        }
    }
    Ok(None)
}

fn bereaved(mobile: &MobileData) -> bool {
    mobile.social.as_ref().and_then(|s| s.bereaved_until_day).is_some()
}

fn ordered_pair(a: Uuid, b: Uuid) -> (Uuid, Uuid) {
    if a < b { (a, b) } else { (b, a) }
}

fn promote_to_cohabitant(mobile: &mut MobileData, other_id: Uuid) {
    if let Some(rel) = mobile.relationships.iter_mut().find(|r| r.other_id == other_id) {
        rel.kind = RelationshipKind::Cohabitant;
    } else {
        mobile.relationships.push(Relationship {
            other_id,
            kind: RelationshipKind::Cohabitant,
            affinity: COHAB_AFFINITY_THRESHOLD,
            last_interaction_day: 0,
            recent_topics: Vec::new(),
        });
    }
}

fn demote_from_cohabitant(mobile: &mut MobileData, other_id: Uuid) {
    if let Some(rel) = mobile.relationships.iter_mut().find(|r| r.other_id == other_id) {
        if matches!(rel.kind, RelationshipKind::Cohabitant) {
            rel.kind = RelationshipKind::Friend;
        }
    }
}

/// Area lookup: resolve the area this mobile's `resident_of` room belongs to.
/// Returns `None` if resident_of is unset or the room doesn't have an area.
pub fn area_for_resident(db: &Db, mobile: &MobileData) -> Option<AreaData> {
    let vnum = mobile.resident_of.as_deref().filter(|v| !v.is_empty())?;
    let room = db.get_room_by_vnum(vnum).ok().flatten()?;
    let area_id = room.area_id?;
    db.get_area_data(&area_id).ok().flatten()
}

/// Spawn a newborn child for `mother_id`, pulling visual/name pools from the
/// mother's resident area. Wires reciprocal Parent/Child links to the mother
/// and (if supplied and alive) `father_id`, plus Sibling links to any existing
/// living children of the mother. Caller is responsible for clearing the
/// mother's `pregnant_until_day` + `pregnant_by` after a successful return.
pub fn spawn_child(db: &Db, data: &MigrationData, mother_id: Uuid, father_id: Option<Uuid>) -> Result<Uuid> {
    let mother = db
        .get_mobile_data(&mother_id)?
        .context("spawn_child: mother not found")?;
    let area = area_for_resident(db, &mother).context("spawn_child: mother has no resolvable area")?;
    let profile = data.visual_profile(&area.immigration_visual_profile).with_context(|| {
        format!(
            "spawn_child: unknown visual profile '{}' for area '{}'",
            area.immigration_visual_profile, area.prefix
        )
    })?;
    let name_pool = data.name_pool(&area.immigration_name_pool).with_context(|| {
        format!(
            "spawn_child: unknown name pool '{}' for area '{}'",
            area.immigration_name_pool, area.prefix
        )
    })?;

    let mut rng = thread_rng();
    let gender = random_gender(&mut rng);
    let (mut chars, description) = generate_child_characteristics(profile, gender, &mut rng)
        .context("spawn_child: visual profile has no juvenile age_ranges")?;

    // Newborn: override to age 0 regardless of profile juvenile range, so the
    // aging tick sees a well-defined birth_day.
    let today = {
        let gt = db.get_game_time()?;
        absolute_game_day(gt.year, gt.month, gt.day)
    };
    chars.age = 0;
    chars.age_label = crate::types::age_label_for_stage(crate::types::LifeStage::Baby).to_string();
    chars.birth_day = today;

    // Name: mother's last name + fresh first.
    let last_name = mother
        .name
        .split_once(' ')
        .map(|(_, l)| l.to_string())
        .unwrap_or_default();
    let first_pool: &[String] = if gender == "female" {
        &name_pool.female_first
    } else {
        &name_pool.male_first
    };
    let first_name = first_pool
        .choose(&mut rng)
        .cloned()
        .unwrap_or_else(|| "Mika".to_string());
    let full_name = if last_name.is_empty() {
        first_name.clone()
    } else {
        format!("{} {}", first_name, last_name)
    };

    let mut child = MobileData::new(full_name.clone());
    child.is_prototype = false;
    child.vnum = format!("migrant:newborn:{}", area.prefix);
    child.short_desc = format!(
        "{} is here, a {} {}.",
        full_name,
        chars.age_label,
        if gender == "female" { "girl" } else { "boy" }
    );
    child.long_desc = description;
    child.keywords = full_name.split_whitespace().map(|s| s.to_lowercase()).collect();
    child.characteristics = Some(chars);
    child.flags = MobileFlags::default();
    child.household_id = mother.household_id;
    // Child inherits mother's residency claim — dependent, no new slot.
    child.resident_of = None;
    child.social = None;
    child.simulation = None;
    child.needs = None;

    // Parent/Child link: mother → Child of, newborn → Parent of.
    child.relationships.push(Relationship {
        other_id: mother_id,
        kind: RelationshipKind::Parent,
        affinity: 70,
        last_interaction_day: 0,
        recent_topics: Vec::new(),
    });
    // Father: include only if alive at birth time. Posthumous paternity
    // (plan: skip link if father gone) is handled by the get_mobile_data check.
    if let Some(fid) = father_id {
        if let Ok(Some(father)) = db.get_mobile_data(&fid) {
            if father.current_hp > 0 {
                child.relationships.push(Relationship {
                    other_id: fid,
                    kind: RelationshipKind::Parent,
                    affinity: 70,
                    last_interaction_day: 0,
                    recent_topics: Vec::new(),
                });
            }
        }
    }

    // Sibling links: every living Child of the mother becomes a Sibling of
    // the newborn. Symmetric: push to both sides.
    let mother_children: Vec<Uuid> = mother
        .relationships
        .iter()
        .filter(|r| matches!(r.kind, RelationshipKind::Child))
        .map(|r| r.other_id)
        .collect();
    for sibling_id in &mother_children {
        if let Ok(Some(sib)) = db.get_mobile_data(sibling_id) {
            if sib.current_hp > 0 {
                child.relationships.push(Relationship {
                    other_id: *sibling_id,
                    kind: RelationshipKind::Sibling,
                    affinity: 50,
                    last_interaction_day: 0,
                    recent_topics: Vec::new(),
                });
            }
        }
    }

    let child_id = child.id;
    let birth_room_id = mother.current_room_id;
    db.save_mobile_data(child)?;

    // Persist reciprocal links on mother, father, and siblings.
    let father_link_id = father_id.and_then(|fid| {
        db.get_mobile_data(&fid)
            .ok()
            .flatten()
            .filter(|f| f.current_hp > 0)
            .map(|_| fid)
    });
    db.update_mobile(&mother_id, |m| {
        m.relationships.push(Relationship {
            other_id: child_id,
            kind: RelationshipKind::Child,
            affinity: 70,
            last_interaction_day: 0,
            recent_topics: Vec::new(),
        });
        if let Some(s) = m.social.as_mut() {
            s.pregnant_until_day = None;
            s.pregnant_by = None;
        }
    })?;
    if let Some(fid) = father_link_id {
        let _ = db.update_mobile(&fid, |m| {
            m.relationships.push(Relationship {
                other_id: child_id,
                kind: RelationshipKind::Child,
                affinity: 70,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
        });
    }
    for sib_id in mother_children {
        let _ = db.update_mobile(&sib_id, |m| {
            m.relationships.push(Relationship {
                other_id: child_id,
                kind: RelationshipKind::Sibling,
                affinity: 50,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
        });
    }

    // Place the newborn in the mother's current room (or leave room-less if
    // she has none — aging tick owner can fix up later).
    if let Some(room_id) = birth_room_id {
        let _ = db.move_mobile_to_room(&child_id, &room_id);
    }

    Ok(child_id)
}

/// Describes what a spawn slot resolved to before placement.
pub enum MigrantGroup {
    Single(MobileData),
    ParentChild { parent: MobileData, child: MobileData },
    SiblingPair(MobileData, MobileData),
}

/// Build a pre-linked migrant family (parent+child or sibling pair). The
/// caller handles room placement, residency, and persistence. Returns the
/// built group — never fails if the underlying `build_migrant` succeeds.
pub fn build_migrant_family<R: Rng>(
    area: &AreaData,
    home_room_vnum: &str,
    data: &MigrationData,
    existing_names: &HashSet<String>,
    rng: &mut R,
    shape: FamilyShape,
) -> Result<MigrantGroup> {
    match shape {
        FamilyShape::ParentChild => {
            let mut parent = build_migrant(area, home_room_vnum, data, existing_names, rng)?;
            let household_id = Uuid::new_v4();
            parent.household_id = Some(household_id);

            // Child uses the same visual profile but samples from juvenile
            // ranges. If the profile has no juvenile entries, fall back to
            // spawning a plain single — surface this to the caller as a
            // "just the parent" shape by dropping in a no-op sibling. We
            // consider it rare enough that the simpler fallback is fine.
            let profile = data
                .visual_profile(&area.immigration_visual_profile)
                .expect("profile validated before build");
            let child_gender = random_gender(rng);
            let (mut child_chars, child_desc) = match generate_child_characteristics(profile, child_gender, rng) {
                Some(pair) => pair,
                None => {
                    // Profile lacks juvenile ranges — treat the slot as a
                    // plain single to avoid stalling migration.
                    return Ok(MigrantGroup::Single(parent));
                }
            };
            child_chars.gender = child_gender.to_string();

            // Share the parent's last name (token after the first space) so
            // the family reads as a unit in examine output.
            let last_name = parent
                .name
                .split_once(' ')
                .map(|(_, l)| l.to_string())
                .unwrap_or_default();
            let name_pool = data
                .name_pool(&area.immigration_name_pool)
                .expect("pool validated before build");
            let first_pool: &[String] = if child_gender == "female" {
                &name_pool.female_first
            } else {
                &name_pool.male_first
            };
            let child_first = first_pool
                .choose(rng)
                .map(|s| s.clone())
                .unwrap_or_else(|| "Mika".to_string());
            let child_full = if last_name.is_empty() {
                child_first.clone()
            } else {
                format!("{} {}", child_first, last_name)
            };

            let mut child = MobileData::new(child_full.clone());
            child.is_prototype = false;
            child.vnum = format!("migrant:child:{}", area.prefix);
            child.short_desc = format!(
                "{} is here, a {} {}.",
                child_full,
                child_chars.age_label,
                if child_gender == "female" { "girl" } else { "boy" }
            );
            child.long_desc = child_desc;
            child.keywords = child_full.split_whitespace().map(|s| s.to_lowercase()).collect();
            child.characteristics = Some(child_chars);
            child.flags = MobileFlags::default();
            child.household_id = Some(household_id);
            // Child does NOT claim a separate liveable slot — they're a
            // dependent of the parent's residency.
            child.resident_of = None;
            // Children aren't simulated (no jobs, no conversation) for now.
            child.social = None;
            child.simulation = None;
            child.needs = None;

            // Reciprocal Parent/Child with starting affinity 70.
            parent.relationships.push(Relationship {
                other_id: child.id,
                kind: RelationshipKind::Child,
                affinity: 70,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
            child.relationships.push(Relationship {
                other_id: parent.id,
                kind: RelationshipKind::Parent,
                affinity: 70,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });

            Ok(MigrantGroup::ParentChild { parent, child })
        }
        FamilyShape::SiblingPair => {
            let mut sib_a = build_migrant(area, home_room_vnum, data, existing_names, rng)?;
            let mut seen = existing_names.clone();
            seen.insert(sib_a.name.clone());
            let mut sib_b = build_migrant(area, home_room_vnum, data, &seen, rng)?;

            // Force sib_b to share sib_a's last name for family feel.
            if let Some((first, _)) = sib_b.name.split_once(' ') {
                if let Some((_, last_a)) = sib_a.name.split_once(' ') {
                    let rebuilt = format!("{} {}", first, last_a);
                    sib_b.name = rebuilt.clone();
                    sib_b.keywords = rebuilt.split_whitespace().map(|s| s.to_lowercase()).collect();
                    if let Some(chars) = sib_b.characteristics.as_ref() {
                        let gnoun = if chars.gender == "female" { "woman" } else { "man" };
                        sib_b.short_desc = format!("{} is here, a {} {}.", rebuilt, chars.age_label, gnoun);
                    }
                }
            }

            let household_id = Uuid::new_v4();
            sib_a.household_id = Some(household_id);
            sib_b.household_id = Some(household_id);

            sib_a.relationships.push(Relationship {
                other_id: sib_b.id,
                kind: RelationshipKind::Sibling,
                affinity: 50,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
            sib_b.relationships.push(Relationship {
                other_id: sib_a.id,
                kind: RelationshipKind::Sibling,
                affinity: 50,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });

            Ok(MigrantGroup::SiblingPair(sib_a, sib_b))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FamilyShape {
    ParentChild,
    SiblingPair,
}

/// Roll the family shape for a spawn slot, or `None` for a plain single.
pub fn roll_family_shape<R: Rng>(chances: &crate::types::ImmigrationFamilyChance, rng: &mut R) -> Option<FamilyShape> {
    let roll: f32 = rng.r#gen();
    if roll < chances.parent_child {
        return Some(FamilyShape::ParentChild);
    }
    // Sibling-pair chance rolled independently against a fresh uniform.
    let roll: f32 = rng.r#gen();
    if roll < chances.sibling_pair {
        return Some(FamilyShape::SiblingPair);
    }
    None
}

/// Run migration for a single area. Returns number of migrants spawned.
fn run_area_migration(
    db: &Db,
    connections: &SharedConnections,
    area: &AreaData,
    data: &MigrationData,
) -> Result<usize> {
    if area.immigration_room_vnum.is_empty() {
        let msg = format!("Migration: area '{}' has no immigration_room_vnum set", area.prefix);
        warn!("{}", msg);
        broadcast_to_builders(connections, &msg);
        return Ok(0);
    }
    if area.immigration_name_pool.is_empty() || data.name_pool(&area.immigration_name_pool).is_none() {
        let msg = format!(
            "Migration: area '{}' references unknown name pool '{}'",
            area.prefix, area.immigration_name_pool
        );
        warn!("{}", msg);
        broadcast_to_builders(connections, &msg);
        return Ok(0);
    }
    if area.immigration_visual_profile.is_empty() || data.visual_profile(&area.immigration_visual_profile).is_none() {
        let msg = format!(
            "Migration: area '{}' references unknown visual profile '{}'",
            area.prefix, area.immigration_visual_profile
        );
        warn!("{}", msg);
        broadcast_to_builders(connections, &msg);
        return Ok(0);
    }

    let arrival_room = match db.get_room_by_vnum(&area.immigration_room_vnum)? {
        Some(r) => r,
        None => {
            let msg = format!(
                "Migration: area '{}' immigration_room_vnum '{}' does not resolve to any room",
                area.prefix, area.immigration_room_vnum
            );
            warn!("{}", msg);
            broadcast_to_builders(connections, &msg);
            return Ok(0);
        }
    };

    let rooms = db.get_rooms_in_area(&area.id)?;
    let mut slots: Vec<RoomData> = rooms
        .into_iter()
        .filter(|r| r.flags.liveable && r.living_capacity > 0)
        .filter(|r| (r.living_capacity as usize) > r.residents.len())
        .collect();

    if slots.is_empty() {
        return Ok(0);
    }

    let total_free: usize = slots
        .iter()
        .map(|r| (r.living_capacity as usize).saturating_sub(r.residents.len()))
        .sum();
    let cap = area.migration_max_per_check as usize;
    let n_to_spawn = total_free.min(if cap == 0 { total_free } else { cap });
    if n_to_spawn == 0 {
        return Ok(0);
    }

    let mut existing_names: HashSet<String> = db
        .list_all_mobiles()?
        .into_iter()
        .filter(|m| !m.is_prototype)
        .map(|m| m.name)
        .collect();

    let mut rng = thread_rng();
    let mut spawned = 0usize;
    for _ in 0..n_to_spawn {
        slots.shuffle(&mut rng);
        slots.sort_by(|a, b| {
            let a_free = (a.living_capacity as usize).saturating_sub(a.residents.len());
            let b_free = (b.living_capacity as usize).saturating_sub(b.residents.len());
            b_free.cmp(&a_free)
        });
        let room = match slots.first_mut() {
            Some(r) => r,
            None => break,
        };
        let free_here = (room.living_capacity as usize).saturating_sub(room.residents.len());
        let home_vnum = match &room.vnum {
            Some(v) => v.clone(),
            None => {
                warn!("Migration: liveable room {} has no vnum; skipping", room.id);
                slots.remove(0);
                continue;
            }
        };

        let shape = roll_family_shape(&area.immigration_family_chance, &mut rng);
        // Sibling pairs need 2 free slots; parent+child only needs 1 (child
        // is a dependent). Fall back to single if the chosen room can't hold
        // the group.
        let effective_shape = match shape {
            Some(FamilyShape::SiblingPair) if free_here < 2 => None,
            other => other,
        };

        let group = match effective_shape {
            Some(s) => build_migrant_family(area, &home_vnum, data, &existing_names, &mut rng, s),
            None => build_migrant(area, &home_vnum, data, &existing_names, &mut rng).map(MigrantGroup::Single),
        };
        let group = match group {
            Ok(g) => g,
            Err(e) => {
                warn!("Migration: failed to build migrant for area '{}': {}", area.prefix, e);
                break;
            }
        };

        match group {
            MigrantGroup::Single(migrant) => {
                let id = migrant.id;
                let name = migrant.name.clone();
                db.save_mobile_data(migrant)?;
                db.move_mobile_to_room(&id, &arrival_room.id)?;
                let mut persisted_room = match db.get_room_data(&room.id)? {
                    Some(r) => r,
                    None => continue,
                };
                persisted_room.residents.push(id);
                db.save_room_data(persisted_room.clone())?;
                room.residents.push(id);
                existing_names.insert(name);
                broadcast_to_room_awake(
                    connections,
                    arrival_room.id,
                    "A weary traveler arrives.".to_string(),
                    None,
                );
                spawned += 1;
            }
            MigrantGroup::ParentChild { parent, child } => {
                let parent_id = parent.id;
                let child_id = child.id;
                let parent_name = parent.name.clone();
                let child_name = child.name.clone();
                db.save_mobile_data(parent)?;
                db.save_mobile_data(child)?;
                db.move_mobile_to_room(&parent_id, &arrival_room.id)?;
                db.move_mobile_to_room(&child_id, &arrival_room.id)?;
                let mut persisted_room = match db.get_room_data(&room.id)? {
                    Some(r) => r,
                    None => continue,
                };
                persisted_room.residents.push(parent_id);
                db.save_room_data(persisted_room.clone())?;
                room.residents.push(parent_id);
                existing_names.insert(parent_name);
                existing_names.insert(child_name);
                broadcast_to_room_awake(
                    connections,
                    arrival_room.id,
                    "A weary parent arrives, a child in tow.".to_string(),
                    None,
                );
                spawned += 2;
            }
            MigrantGroup::SiblingPair(a, b) => {
                let a_id = a.id;
                let b_id = b.id;
                let a_name = a.name.clone();
                let b_name = b.name.clone();
                db.save_mobile_data(a)?;
                db.save_mobile_data(b)?;
                db.move_mobile_to_room(&a_id, &arrival_room.id)?;
                db.move_mobile_to_room(&b_id, &arrival_room.id)?;
                let mut persisted_room = match db.get_room_data(&room.id)? {
                    Some(r) => r,
                    None => continue,
                };
                persisted_room.residents.push(a_id);
                persisted_room.residents.push(b_id);
                db.save_room_data(persisted_room.clone())?;
                room.residents.push(a_id);
                room.residents.push(b_id);
                existing_names.insert(a_name);
                existing_names.insert(b_name);
                broadcast_to_room_awake(
                    connections,
                    arrival_room.id,
                    "Two weary siblings arrive.".to_string(),
                    None,
                );
                spawned += 2;
            }
        }
    }

    Ok(spawned)
}

#[cfg(test)]
mod pair_housing_tests {
    use super::*;
    use crate::types::{
        AreaData, AreaFlags, AreaPermission, CombatZoneType, ImmigrationVariationChances, MobileData, RoomData,
        RoomExits, RoomFlags, WaterType,
    };

    struct DbGuard {
        db: Db,
        path: String,
    }
    impl Drop for DbGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn open_db(tag: &str) -> DbGuard {
        let path = format!(
            "test_pair_{}_{}_{}.db",
            tag,
            std::process::id(),
            Uuid::new_v4().simple()
        );
        let _ = std::fs::remove_dir_all(&path);
        let db = Db::open(&path).expect("open db");
        DbGuard { db, path }
    }

    fn mk_area(db: &Db) -> AreaData {
        let area = AreaData {
            id: Uuid::new_v4(),
            name: "Test Area".to_string(),
            prefix: "test".to_string(),
            description: String::new(),
            level_min: 1,
            level_max: 10,
            theme: String::new(),
            owner: None,
            permission_level: AreaPermission::AllBuilders,
            trusted_builders: Vec::new(),
            city_forage_table: Vec::new(),
            wilderness_forage_table: Vec::new(),
            shallow_water_forage_table: Vec::new(),
            deep_water_forage_table: Vec::new(),
            underwater_forage_table: Vec::new(),
            combat_zone: CombatZoneType::default(),
            flags: AreaFlags::default(),
            immigration_enabled: false,
            immigration_room_vnum: String::new(),
            immigration_name_pool: String::new(),
            immigration_visual_profile: String::new(),
            migration_interval_days: 0,
            migration_max_per_check: 0,
            migrant_sim_defaults: None,
            last_migration_check_day: None,
            immigration_variation_chances: ImmigrationVariationChances::default(),
            immigration_family_chance: crate::types::ImmigrationFamilyChance::default(),
        };
        db.save_area_data(area.clone()).expect("save area");
        area
    }

    fn mk_room(db: &Db, area_id: Uuid, vnum: &str, capacity: i32) -> RoomData {
        let mut flags = RoomFlags::default();
        flags.liveable = capacity > 0;
        let room = RoomData {
            id: Uuid::new_v4(),
            title: format!("room {}", vnum),
            description: String::new(),
            exits: RoomExits::default(),
            flags,
            extra_descs: Vec::new(),
            vnum: Some(vnum.to_string()),
            area_id: Some(area_id),
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
            living_capacity: capacity,
            residents: Vec::new(),
        };
        db.save_room_data(room.clone()).expect("save room");
        db.set_room_vnum(&room.id, vnum).expect("vnum index");
        room
    }

    fn mk_simulated_mobile(db: &Db, name: &str, resident_of: Option<String>) -> MobileData {
        let mut m = MobileData::new(name.to_string());
        m.is_prototype = false;
        m.resident_of = resident_of;
        m.social = Some(SocialState::default());
        db.save_mobile_data(m.clone()).expect("save mobile");
        m
    }

    fn link_resident(db: &Db, vnum: &str, mobile_id: Uuid) {
        let mut room = db.get_room_by_vnum(vnum).unwrap().unwrap();
        room.residents.push(mobile_id);
        db.save_room_data(room).unwrap();
    }

    fn set_affinity(db: &Db, a: Uuid, b: Uuid, value: i32, kind: RelationshipKind) {
        db.update_mobile(&a, |m| {
            m.relationships.push(Relationship {
                other_id: b,
                kind,
                affinity: value,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
        })
        .unwrap();
    }

    #[test]
    fn cohabitation_merges_high_affinity_pair() {
        let guard = open_db("cohab_merge");
        let db = &guard.db;
        let area = mk_area(db);
        mk_room(db, area.id, "r1", 2);
        mk_room(db, area.id, "r2", 1);

        let alice = mk_simulated_mobile(db, "alice", Some("r1".to_string()));
        let bob = mk_simulated_mobile(db, "bob", Some("r2".to_string()));
        link_resident(db, "r1", alice.id);
        link_resident(db, "r2", bob.id);

        set_affinity(db, alice.id, bob.id, 90, RelationshipKind::Friend);
        set_affinity(db, bob.id, alice.id, 90, RelationshipKind::Friend);

        process_pair_housing(db).unwrap();

        // Bob should now share Alice's room (r1 has capacity 2).
        let bob_after = db.get_mobile_data(&bob.id).unwrap().unwrap();
        assert_eq!(bob_after.resident_of.as_deref(), Some("r1"));

        let r1 = db.get_room_by_vnum("r1").unwrap().unwrap();
        assert!(r1.residents.contains(&alice.id));
        assert!(r1.residents.contains(&bob.id));

        let r2 = db.get_room_by_vnum("r2").unwrap().unwrap();
        assert!(!r2.residents.contains(&bob.id));

        // Both sides promoted to Cohabitant.
        let alice_after = db.get_mobile_data(&alice.id).unwrap().unwrap();
        assert!(
            alice_after
                .relationships
                .iter()
                .any(|r| r.other_id == bob.id && matches!(r.kind, RelationshipKind::Cohabitant))
        );
        assert!(
            bob_after
                .relationships
                .iter()
                .any(|r| r.other_id == alice.id && matches!(r.kind, RelationshipKind::Cohabitant))
        );
    }

    #[test]
    fn breakup_rehouses_mover_when_room_is_free() {
        let guard = open_db("breakup_rehouse");
        let db = &guard.db;
        let area = mk_area(db);
        mk_room(db, area.id, "shared", 2);
        mk_room(db, area.id, "free", 1);

        let alice = mk_simulated_mobile(db, "alice", Some("shared".to_string()));
        let bob = mk_simulated_mobile(db, "bob", Some("shared".to_string()));
        link_resident(db, "shared", alice.id);
        link_resident(db, "shared", bob.id);

        // Deeply negative cohab pair; bob has worse affinity so he moves out.
        set_affinity(db, alice.id, bob.id, -65, RelationshipKind::Cohabitant);
        set_affinity(db, bob.id, alice.id, -80, RelationshipKind::Cohabitant);

        process_pair_housing(db).unwrap();

        let bob_after = db.get_mobile_data(&bob.id).unwrap().unwrap();
        assert_eq!(bob_after.resident_of.as_deref(), Some("free"));
        assert!(
            bob_after
                .relationships
                .iter()
                .find(|r| r.other_id == alice.id)
                .map_or(false, |r| matches!(r.kind, RelationshipKind::Friend))
        );
    }

    #[test]
    fn breakup_without_free_room_becomes_homeless() {
        let guard = open_db("breakup_homeless");
        let db = &guard.db;
        let area = mk_area(db);
        mk_room(db, area.id, "shared", 2);
        // No other liveable room anywhere.

        let alice = mk_simulated_mobile(db, "alice", Some("shared".to_string()));
        let bob = mk_simulated_mobile(db, "bob", Some("shared".to_string()));
        link_resident(db, "shared", alice.id);
        link_resident(db, "shared", bob.id);

        set_affinity(db, alice.id, bob.id, -90, RelationshipKind::Cohabitant);
        set_affinity(db, bob.id, alice.id, -90, RelationshipKind::Cohabitant);

        process_pair_housing(db).unwrap();

        // Exactly one of them is homeless now.
        let alice_after = db.get_mobile_data(&alice.id).unwrap().unwrap();
        let bob_after = db.get_mobile_data(&bob.id).unwrap().unwrap();
        let homeless = alice_after.resident_of.as_deref().unwrap_or("").is_empty() as usize
            + bob_after.resident_of.as_deref().unwrap_or("").is_empty() as usize;
        assert_eq!(homeless, 1);
    }

    #[test]
    fn homeless_is_rehoused_when_room_frees() {
        let guard = open_db("homeless_rehouse");
        let db = &guard.db;
        let area = mk_area(db);
        mk_room(db, area.id, "empty", 1);

        let drifter = mk_simulated_mobile(db, "drifter", None);
        process_pair_housing(db).unwrap();

        let after = db.get_mobile_data(&drifter.id).unwrap().unwrap();
        assert_eq!(after.resident_of.as_deref(), Some("empty"));
        let empty = db.get_room_by_vnum("empty").unwrap().unwrap();
        assert!(empty.residents.contains(&drifter.id));
    }

    #[test]
    fn bereavement_on_delete_crashes_survivor_happiness() {
        let guard = open_db("bereavement");
        let db = &guard.db;
        let area = mk_area(db);
        mk_room(db, area.id, "shared", 2);

        let alice = mk_simulated_mobile(db, "alice", Some("shared".to_string()));
        let bob = mk_simulated_mobile(db, "bob", Some("shared".to_string()));
        link_resident(db, "shared", alice.id);
        link_resident(db, "shared", bob.id);
        set_affinity(db, alice.id, bob.id, 90, RelationshipKind::Cohabitant);
        set_affinity(db, bob.id, alice.id, 90, RelationshipKind::Cohabitant);

        // Give alice high happiness to prove the crash happens.
        db.update_mobile(&alice.id, |m| {
            m.social.as_mut().unwrap().happiness = 80;
        })
        .unwrap();

        db.delete_mobile(&bob.id).unwrap();

        let alice_after = db.get_mobile_data(&alice.id).unwrap().unwrap();
        let social = alice_after.social.as_ref().unwrap();
        assert_eq!(social.happiness, 40); // 80 - 40
        assert!(social.bereaved_until_day.is_some());
        // Cohabitant relationship demoted to Friend.
        assert!(
            alice_after
                .relationships
                .iter()
                .find(|r| r.other_id == bob.id)
                .map_or(false, |r| matches!(r.kind, RelationshipKind::Friend))
        );
        // Shared room no longer lists bob.
        let shared = db.get_room_by_vnum("shared").unwrap().unwrap();
        assert!(!shared.residents.contains(&bob.id));
    }

    #[test]
    fn roll_social_state_produces_disjoint_lists() {
        let topics: Vec<String> = (0..20).map(|i| format!("topic{}", i)).collect();
        let mut rng = rand::thread_rng();
        for _ in 0..50 {
            let state = roll_social_state(&topics, &mut rng);
            for like in &state.likes {
                assert!(
                    !state.dislikes.contains(like),
                    "topic {} appeared in both likes and dislikes",
                    like
                );
            }
            assert!((2..=4).contains(&state.likes.len()));
            assert!((1..=3).contains(&state.dislikes.len()));
        }
    }
}

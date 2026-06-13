// src/script/fear.rs
// Fear status-effect application chokepoint. Every fear source — the fear
// spell, fear-laced consumables, equipped fear auras — must route through
// `try_apply_fear_to_character` / `try_apply_fear_to_mobile` so hard
// immunities (synths; no_fear / construct / undead mobiles; Courage and
// Frenzy holders) and `StatusResistance` hold uniformly.

use crate::db::Db;
use crate::script::combat::roll_status_application;
use crate::{ActiveBuff, CharacterData, CreatureType, EffectType, MobileData, SharedConnections};
use rand::Rng;
use rhai::Engine;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FearOutcome {
    Applied,
    Immune,
    Resisted,
    NotFound,
}

impl FearOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            FearOutcome::Applied => "applied",
            FearOutcome::Immune => "immune",
            FearOutcome::Resisted => "resisted",
            FearOutcome::NotFound => "not_found",
        }
    }
}

/// Hard fear immunity for a character, or `None` if fear can apply.
/// Returns the reason key for messaging/diagnostics.
pub fn character_fear_immunity(character: &CharacterData) -> Option<&'static str> {
    if character.synth_state.is_some() || character.race.eq_ignore_ascii_case("synth") {
        return Some("synth");
    }
    if character
        .active_buffs
        .iter()
        .any(|b| b.effect_type == EffectType::Courage)
    {
        return Some("courage");
    }
    // Frenzy wins over fear — the beast knows no terror.
    if character
        .active_buffs
        .iter()
        .any(|b| b.effect_type == EffectType::Frenzy)
    {
        return Some("frenzy");
    }
    None
}

/// Hard fear immunity for a mobile, or `None` if fear can apply.
pub fn mobile_fear_immunity(mobile: &MobileData) -> Option<&'static str> {
    if mobile.flags.no_fear {
        return Some("no_fear");
    }
    if mobile.creature_type == CreatureType::Construct {
        return Some("construct");
    }
    if mobile.flags.undead {
        return Some("undead");
    }
    if mobile.active_buffs.iter().any(|b| b.effect_type == EffectType::Courage) {
        return Some("courage");
    }
    if mobile.active_buffs.iter().any(|b| b.effect_type == EffectType::Frenzy) {
        return Some("frenzy");
    }
    None
}

/// True iff the buff list holds `Feared` and no `Courage` (belt-and-braces:
/// Courage strips Feared on application, but a stale pair must never panic
/// the bearer). This is THE predicate behavior code checks — never `FearAura`.
pub fn is_feared(buffs: &[ActiveBuff]) -> bool {
    buffs.iter().any(|b| b.effect_type == EffectType::Feared)
        && !buffs.iter().any(|b| b.effect_type == EffectType::Courage)
}

/// Add or refresh a `Feared` buff. Magnitude collapses via max (apply_buff
/// convention); duration via max so a short aura refresh can't shorten a
/// longer spell-applied terror.
fn stamp_feared(buffs: &mut Vec<ActiveBuff>, magnitude: i32, duration_secs: i32, source: &str) {
    if let Some(existing) = buffs.iter_mut().find(|b| b.effect_type == EffectType::Feared) {
        existing.magnitude = existing.magnitude.max(magnitude);
        existing.remaining_secs = existing.remaining_secs.max(duration_secs);
        existing.source = source.to_string();
    } else {
        buffs.push(ActiveBuff {
            effect_type: EffectType::Feared,
            magnitude,
            remaining_secs: duration_secs,
            source: source.to_string(),
            damage_type: None,
            vs_effect: None,
            skill_key: None,
        });
    }
}

/// Try to frighten a character. Checks hard immunities, then graded
/// resistance via `roll_status_application`, then stamps `Feared` and syncs
/// the session copy (session is authoritative for online players).
#[allow(clippy::too_many_arguments)]
pub fn try_apply_fear_to_character<R: Rng + ?Sized>(
    db: &Db,
    connections: &SharedConnections,
    char_name: &str,
    base_chance: i32,
    magnitude: i32,
    duration_secs: i32,
    source: &str,
    rng: &mut R,
) -> FearOutcome {
    let name_lower = char_name.to_lowercase();
    let mut character = match db.get_character_data(&name_lower) {
        Ok(Some(c)) => c,
        _ => return FearOutcome::NotFound,
    };
    if character_fear_immunity(&character).is_some() {
        return FearOutcome::Immune;
    }
    if !roll_status_application(&character.active_buffs, EffectType::Feared, base_chance, rng) {
        return FearOutcome::Resisted;
    }
    stamp_feared(&mut character.active_buffs, magnitude, duration_secs, source);
    if db.save_character_data(character.clone()).is_err() {
        return FearOutcome::NotFound;
    }
    let mut conns_guard = connections.lock().unwrap();
    for (_id, session) in conns_guard.iter_mut() {
        if let Some(ref mut sc) = session.character {
            if sc.name.eq_ignore_ascii_case(char_name) {
                sc.active_buffs = character.active_buffs.clone();
                break;
            }
        }
    }
    FearOutcome::Applied
}

/// Try to frighten a mobile instance. Same gate order as the character path.
pub fn try_apply_fear_to_mobile<R: Rng + ?Sized>(
    db: &Db,
    mobile_id: &Uuid,
    base_chance: i32,
    magnitude: i32,
    duration_secs: i32,
    source: &str,
    rng: &mut R,
) -> FearOutcome {
    let mut mobile = match db.get_mobile_data(mobile_id) {
        Ok(Some(m)) => m,
        _ => return FearOutcome::NotFound,
    };
    if mobile_fear_immunity(&mobile).is_some() {
        return FearOutcome::Immune;
    }
    if !roll_status_application(&mobile.active_buffs, EffectType::Feared, base_chance, rng) {
        return FearOutcome::Resisted;
    }
    stamp_feared(&mut mobile.active_buffs, magnitude, duration_secs, source);
    if db.save_mobile_data(mobile).is_err() {
        return FearOutcome::NotFound;
    }
    FearOutcome::Applied
}

pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections) {
    // try_apply_fear(target_name_or_id, base_chance, magnitude, duration_secs, source)
    //   -> "applied" | "immune" | "resisted" | "not_found"
    // Resolves the target as a mobile UUID first, else a character name
    // (same convention as roll_status_application). The ONLY way scripts
    // should ever stamp a Feared buff.
    engine.register_fn(
        "try_apply_fear",
        move |target: String, base_chance: i64, magnitude: i64, duration_secs: i64, source: String| -> String {
            let mut rng = rand::thread_rng();
            let outcome = if let Ok(uuid) = Uuid::parse_str(&target) {
                try_apply_fear_to_mobile(
                    &db,
                    &uuid,
                    base_chance as i32,
                    magnitude as i32,
                    duration_secs as i32,
                    &source,
                    &mut rng,
                )
            } else {
                try_apply_fear_to_character(
                    &db,
                    &connections,
                    &target,
                    base_chance as i32,
                    magnitude as i32,
                    duration_secs as i32,
                    &source,
                    &mut rng,
                )
            };
            outcome.as_str().to_string()
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    fn base_char() -> CharacterData {
        serde_json::from_value(serde_json::json!({
            "name": "test",
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character")
    }

    fn buff(effect_type: EffectType) -> ActiveBuff {
        ActiveBuff {
            effect_type,
            magnitude: 0,
            remaining_secs: 60,
            source: "test".to_string(),
            damage_type: None,
            vs_effect: None,
            skill_key: None,
        }
    }

    fn open_temp() -> (Db, tempfile::TempDir) {
        let temp = tempfile::tempdir().expect("tempdir");
        let db = Db::open(temp.path()).expect("open db");
        (db, temp)
    }

    fn empty_connections() -> SharedConnections {
        Arc::new(Mutex::new(HashMap::new()))
    }

    #[test]
    fn synth_character_is_immune() {
        let mut c = base_char();
        c.race = "synth".to_string();
        assert_eq!(character_fear_immunity(&c), Some("synth"));
        c.race = "SYNTH".to_string();
        assert_eq!(character_fear_immunity(&c), Some("synth"));
    }

    #[test]
    fn courage_and_frenzy_holders_are_immune() {
        let mut c = base_char();
        c.active_buffs.push(buff(EffectType::Courage));
        assert_eq!(character_fear_immunity(&c), Some("courage"));

        let mut c = base_char();
        c.active_buffs.push(buff(EffectType::Frenzy));
        assert_eq!(character_fear_immunity(&c), Some("frenzy"));

        let mut m = MobileData::new("wolf".to_string());
        m.active_buffs.push(buff(EffectType::Courage));
        assert_eq!(mobile_fear_immunity(&m), Some("courage"));
    }

    #[test]
    fn no_fear_construct_and_undead_mobiles_are_immune() {
        let mut m = MobileData::new("golem".to_string());
        assert_eq!(mobile_fear_immunity(&m), None);
        m.flags.no_fear = true;
        assert_eq!(mobile_fear_immunity(&m), Some("no_fear"));

        let mut m = MobileData::new("statue".to_string());
        m.creature_type = CreatureType::Construct;
        assert_eq!(mobile_fear_immunity(&m), Some("construct"));

        let mut m = MobileData::new("zombie".to_string());
        m.flags.undead = true;
        assert_eq!(mobile_fear_immunity(&m), Some("undead"));
    }

    #[test]
    fn is_feared_requires_feared_without_courage() {
        assert!(!is_feared(&[]));
        assert!(is_feared(&[buff(EffectType::Feared)]));
        assert!(!is_feared(&[buff(EffectType::Feared), buff(EffectType::Courage)]));
        // FearAura on the bearer never frightens the bearer.
        assert!(!is_feared(&[buff(EffectType::FearAura)]));
    }

    #[test]
    fn fear_applies_to_plain_mobile_and_refreshes_duration() {
        let (db, _temp) = open_temp();
        let mut m = MobileData::new("rabbit".to_string());
        m.is_prototype = false;
        let id = m.id;
        db.save_mobile_data(m).expect("save");

        let mut rng = rand::rngs::mock::StepRng::new(0, 0); // gen_range -> minimum -> always lands
        let outcome = try_apply_fear_to_mobile(&db, &id, 100, 0, 30, "fear spell", &mut rng);
        assert_eq!(outcome, FearOutcome::Applied);
        let m = db.get_mobile_data(&id).expect("get").expect("exists");
        let fear = m
            .active_buffs
            .iter()
            .find(|b| b.effect_type == EffectType::Feared)
            .expect("feared");
        assert_eq!(fear.remaining_secs, 30);

        // Shorter re-application must not shorten the existing terror.
        let outcome = try_apply_fear_to_mobile(&db, &id, 100, 0, 15, "fear aura", &mut rng);
        assert_eq!(outcome, FearOutcome::Applied);
        let m = db.get_mobile_data(&id).expect("get").expect("exists");
        let fear = m
            .active_buffs
            .iter()
            .find(|b| b.effect_type == EffectType::Feared)
            .expect("feared");
        assert_eq!(fear.remaining_secs, 30);
    }

    #[test]
    fn fear_respects_mobile_immunity_through_db_path() {
        let (db, _temp) = open_temp();
        let mut m = MobileData::new("golem".to_string());
        m.is_prototype = false;
        m.flags.no_fear = true;
        let id = m.id;
        db.save_mobile_data(m).expect("save");

        let mut rng = rand::rngs::mock::StepRng::new(0, 0);
        let outcome = try_apply_fear_to_mobile(&db, &id, 100, 0, 30, "fear spell", &mut rng);
        assert_eq!(outcome, FearOutcome::Immune);
    }

    #[test]
    fn fear_applies_to_character_and_synth_is_immune() {
        let (db, _temp) = open_temp();
        let conns = empty_connections();
        let mut c = base_char();
        c.name = "victim".to_string();
        db.save_character_data(c).expect("save");

        let mut rng = rand::rngs::mock::StepRng::new(0, 0);
        let outcome = try_apply_fear_to_character(&db, &conns, "victim", 100, 0, 30, "fear spell", &mut rng);
        assert_eq!(outcome, FearOutcome::Applied);
        let c = db.get_character_data("victim").expect("get").expect("exists");
        assert!(is_feared(&c.active_buffs));

        let mut s = base_char();
        s.name = "android".to_string();
        s.race = "synth".to_string();
        db.save_character_data(s).expect("save");
        let outcome = try_apply_fear_to_character(&db, &conns, "android", 100, 0, 30, "fear spell", &mut rng);
        assert_eq!(outcome, FearOutcome::Immune);
    }
}

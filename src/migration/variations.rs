//! Immigrant variation dispatcher.
//!
//! Applied at the end of `build_migrant`: rolls the area's per-role chances
//! and, if a role wins, mutates the freshly-built civilian `MobileData` into
//! that variation. The common case (no role) is a no-op so existing
//! behavior is unchanged.
//!
//! Adding a new role: extend the enum, append a roll in `pick_variation`,
//! add a match arm in `apply_variation`, and write an `apply_<role>`
//! function. Schema work is one new field on `ImmigrationVariationChances`
//! plus one match arm in `set_area_immigration_variation_chance`.

use rand::Rng;

use crate::types::{ActivityState, AreaData, MobileData, SocialState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrantVariation {
    Common,
    Guard,
    Healer,
    Scavenger,
}

impl MigrantVariation {
    /// Suffix used in the synthetic migrant vnum so builders can grep for
    /// specialized roles: `migrant:<role>:<area-prefix>`.
    pub fn vnum_tag(self) -> Option<&'static str> {
        match self {
            MigrantVariation::Common => None,
            MigrantVariation::Guard => Some("guard"),
            MigrantVariation::Healer => Some("healer"),
            MigrantVariation::Scavenger => Some("scavenger"),
        }
    }
}

pub fn pick_variation<R: Rng>(area: &AreaData, rng: &mut R) -> MigrantVariation {
    let chances = &area.immigration_variation_chances;
    if roll(chances.guard, rng) {
        return MigrantVariation::Guard;
    }
    if roll(chances.healer, rng) {
        return MigrantVariation::Healer;
    }
    if roll(chances.scavenger, rng) {
        return MigrantVariation::Scavenger;
    }
    MigrantVariation::Common
}

fn roll<R: Rng>(chance: f32, rng: &mut R) -> bool {
    chance > 0.0 && rng.r#gen::<f32>() < chance.clamp(0.0, 1.0)
}

pub fn apply_variation<R: Rng>(
    mobile: &mut MobileData,
    variation: MigrantVariation,
    rng: &mut R,
) {
    match variation {
        MigrantVariation::Common => {}
        MigrantVariation::Guard => apply_guard(mobile, rng),
        MigrantVariation::Healer => apply_healer(mobile, rng),
        MigrantVariation::Scavenger => apply_scavenger(mobile, rng),
    }
}

/// Thematic topic preferences per role. Entries must be drawn from the
/// migration topic pool (`scripts/data/social/topics.json`) — topics not
/// present in the pool are silently skipped at bias time. Keeping these
/// lists short (≤4 likes, ≤1 dislike) preserves the flavor bias without
/// making role NPCs homogeneous.
fn preferred_topics(variation: MigrantVariation) -> (&'static [&'static str], &'static [&'static str]) {
    match variation {
        MigrantVariation::Common => (&[], &[]),
        MigrantVariation::Guard => (&["the road", "the mayor", "politics", "rumors"], &[]),
        MigrantVariation::Healer => (&["old stories", "children", "craft"], &["drinking"]),
        MigrantVariation::Scavenger => (&["trade", "rumors", "gossip", "the road"], &[]),
    }
}

/// Probability that any given role-preferred topic is swapped into the
/// mobile's rolled social state. A miss leaves the generic roll intact, so
/// not every guard obsesses over the road.
const BIAS_PROBABILITY: f32 = 0.7;

/// Nudge a freshly-rolled `SocialState` toward role-typical topics. For each
/// preferred like/dislike, with probability `BIAS_PROBABILITY` swap out a
/// random generic entry in the matching list. Never introduces a contradiction
/// (topic already on the opposite list is skipped) and preserves the rolled
/// list lengths. Call this after `roll_social_state` and after the base
/// variation has been applied so keyword/flag state is finalized.
pub fn bias_social_for_variation<R: Rng>(
    social: &mut SocialState,
    variation: MigrantVariation,
    topic_pool: &[String],
    rng: &mut R,
) {
    let (pref_likes, pref_dislikes) = preferred_topics(variation);
    apply_bias_list(&mut social.likes, &social.dislikes, pref_likes, topic_pool, rng);
    apply_bias_list(&mut social.dislikes, &social.likes, pref_dislikes, topic_pool, rng);
}

fn apply_bias_list<R: Rng>(
    target: &mut Vec<String>,
    opposite: &[String],
    preferred: &[&'static str],
    topic_pool: &[String],
    rng: &mut R,
) {
    if target.is_empty() {
        return;
    }
    for &topic in preferred {
        if !topic_pool.iter().any(|t| t == topic) {
            continue;
        }
        if target.iter().any(|t| t == topic) {
            continue;
        }
        if opposite.iter().any(|t| t == topic) {
            continue;
        }
        if rng.r#gen::<f32>() > BIAS_PROBABILITY {
            continue;
        }
        let idx = rng.gen_range(0..target.len());
        target[idx] = topic.to_string();
    }
}

fn apply_guard<R: Rng>(mobile: &mut MobileData, _rng: &mut R) {
    mobile.flags.guard = true;
    mobile.flags.no_attack = true;
    mobile.flags.can_open_doors = true;
    mobile.flags.sentinel = false;
    mobile.perception = 5;
    mobile.current_activity = ActivityState::Patrolling;

    if !mobile.keywords.iter().any(|k| k == "guard") {
        mobile.keywords.push("guard".to_string());
    }

    let chars = mobile.characteristics.as_ref();
    let age_label = chars.map(|c| c.age_label.as_str()).unwrap_or("adult");
    let gender = chars.map(|c| c.gender.as_str()).unwrap_or("man");
    let gender_noun = if gender == "female" { "woman" } else { "man" };
    let pronoun = if gender == "female" { "She" } else { "He" };

    mobile.short_desc = format!(
        "{}, a {} {} in a guard's livery, is on patrol here.",
        mobile.name, age_label, gender_noun
    );

    mobile.long_desc.push_str(&format!(
        " {} wears the insignia of a local guard, hand never far from the hilt of a blade.",
        pronoun
    ));
}

fn apply_healer<R: Rng>(mobile: &mut MobileData, _rng: &mut R) {
    mobile.flags.healer = true;
    mobile.flags.no_attack = true;
    mobile.flags.sentinel = false;
    mobile.healer_type = "herbalist".to_string();
    mobile.current_activity = ActivityState::Working;

    if !mobile.keywords.iter().any(|k| k == "healer") {
        mobile.keywords.push("healer".to_string());
    }

    let chars = mobile.characteristics.as_ref();
    let age_label = chars.map(|c| c.age_label.as_str()).unwrap_or("adult");
    let gender = chars.map(|c| c.gender.as_str()).unwrap_or("man");
    let gender_noun = if gender == "female" { "woman" } else { "man" };
    let pronoun = if gender == "female" { "She" } else { "He" };

    mobile.short_desc = format!(
        "{}, a {} {} in a healer's robes, tends a small satchel of herbs here.",
        mobile.name, age_label, gender_noun
    );

    mobile.long_desc.push_str(&format!(
        " {} carries the quiet bearing of someone used to tending the wounded.",
        pronoun
    ));
}

fn apply_scavenger<R: Rng>(mobile: &mut MobileData, _rng: &mut R) {
    mobile.flags.scavenger = true;
    mobile.flags.can_open_doors = true;
    mobile.flags.sentinel = false;
    mobile.perception = 4;
    mobile.current_activity = ActivityState::Working;

    if !mobile.keywords.iter().any(|k| k == "scavenger") {
        mobile.keywords.push("scavenger".to_string());
    }

    let chars = mobile.characteristics.as_ref();
    let age_label = chars.map(|c| c.age_label.as_str()).unwrap_or("adult");
    let gender = chars.map(|c| c.gender.as_str()).unwrap_or("man");
    let gender_noun = if gender == "female" { "woman" } else { "man" };
    let pronoun = if gender == "female" { "She" } else { "He" };

    mobile.short_desc = format!(
        "{}, a {} {} in patched traveling clothes, sifts through the surroundings here.",
        mobile.name, age_label, gender_noun
    );

    mobile.long_desc.push_str(&format!(
        " {} eyes the ground and corners with the practiced squint of one who finds value where others don't.",
        pronoun
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Characteristics, MobileFlags};
    use rand::thread_rng;

    fn fresh_mobile() -> MobileData {
        let mut m = MobileData::new("Test Migrant".to_string());
        m.keywords = vec!["test".to_string(), "migrant".to_string()];
        m.short_desc = "Test Migrant is here.".to_string();
        m.long_desc = "A plain civilian.".to_string();
        m.characteristics = Some(Characteristics {
            gender: "female".to_string(),
            age: 28,
            age_label: "young adult".to_string(),
            birth_day: 0,
            height: "tall".to_string(),
            build: "lean".to_string(),
            hair_color: "black".to_string(),
            hair_style: "braided".to_string(),
            eye_color: "brown".to_string(),
            skin_tone: "olive".to_string(),
            distinguishing_mark: None,
        });
        m.flags = MobileFlags::default();
        m
    }

    #[test]
    fn apply_guard_sets_flags_and_text() {
        let mut m = fresh_mobile();
        let mut rng = thread_rng();
        apply_variation(&mut m, MigrantVariation::Guard, &mut rng);
        assert!(m.flags.guard);
        assert!(m.flags.no_attack);
        assert!(m.flags.can_open_doors);
        assert!(!m.flags.sentinel);
        assert_eq!(m.current_activity, ActivityState::Patrolling);
        assert!(m.perception > 0);
        assert!(m.keywords.iter().any(|k| k == "guard"));
        assert!(m.short_desc.contains("guard's livery"));
        assert!(m.long_desc.contains("insignia of a local guard"));
        assert!(m.long_desc.contains("She"));
    }

    #[test]
    fn apply_healer_sets_flags_and_text() {
        let mut m = fresh_mobile();
        let mut rng = thread_rng();
        apply_variation(&mut m, MigrantVariation::Healer, &mut rng);
        assert!(m.flags.healer);
        assert!(m.flags.no_attack);
        assert!(!m.flags.sentinel);
        assert_eq!(m.healer_type, "herbalist");
        assert_eq!(m.current_activity, ActivityState::Working);
        assert!(m.keywords.iter().any(|k| k == "healer"));
        assert!(m.short_desc.contains("healer's robes"));
        assert!(m.long_desc.contains("tending the wounded"));
        assert!(m.long_desc.contains("She"));
    }

    #[test]
    fn apply_scavenger_sets_flags_and_text() {
        let mut m = fresh_mobile();
        let mut rng = thread_rng();
        apply_variation(&mut m, MigrantVariation::Scavenger, &mut rng);
        assert!(m.flags.scavenger);
        assert!(m.flags.can_open_doors);
        assert!(!m.flags.sentinel);
        assert!(!m.flags.no_attack);
        assert_eq!(m.current_activity, ActivityState::Working);
        assert!(m.perception > 0);
        assert!(m.keywords.iter().any(|k| k == "scavenger"));
        assert!(m.short_desc.contains("patched traveling clothes"));
        assert!(m.long_desc.contains("practiced squint"));
        assert!(m.long_desc.contains("She"));
    }

    #[test]
    fn guard_bias_keeps_list_lengths_and_prefers_role_topics() {
        use crate::types::SocialState;
        use rand::rngs::StdRng;
        use rand::SeedableRng;

        let pool: Vec<String> = vec![
            "fishing", "cooking", "gossip", "religion", "music", "drinking", "hunting",
            "farming", "politics", "the weather", "the harvest", "old stories", "travel",
            "children", "craft", "trade", "the road", "the mayor", "rumors", "the sea",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        // Seeded RNG: every preferred topic's 0.7 roll will land the same way,
        // making the test deterministic.
        let mut rng = StdRng::seed_from_u64(42);
        let mut social = SocialState {
            likes: vec!["cooking".into(), "music".into(), "fishing".into()],
            dislikes: vec!["farming".into()],
            happiness: 50,
            ..SocialState::default()
        };

        let before_likes = social.likes.len();
        let before_dislikes = social.dislikes.len();

        bias_social_for_variation(&mut social, MigrantVariation::Guard, &pool, &mut rng);

        assert_eq!(social.likes.len(), before_likes);
        assert_eq!(social.dislikes.len(), before_dislikes);

        let preferred = ["the road", "the mayor", "politics", "rumors"];
        assert!(
            social.likes.iter().any(|t| preferred.contains(&t.as_str())),
            "guard bias should inject at least one preferred topic, got likes={:?}",
            social.likes
        );
        // Likes and dislikes must remain disjoint.
        for like in &social.likes {
            assert!(!social.dislikes.contains(like));
        }
    }

    #[test]
    fn common_bias_is_noop() {
        use crate::types::SocialState;
        use rand::thread_rng;

        let pool: Vec<String> = vec!["fishing".to_string(), "cooking".to_string()];
        let mut rng = thread_rng();
        let mut social = SocialState {
            likes: vec!["fishing".into()],
            dislikes: vec!["cooking".into()],
            happiness: 50,
            ..SocialState::default()
        };
        let snapshot = (social.likes.clone(), social.dislikes.clone());
        bias_social_for_variation(&mut social, MigrantVariation::Common, &pool, &mut rng);
        assert_eq!(social.likes, snapshot.0);
        assert_eq!(social.dislikes, snapshot.1);
    }

    #[test]
    fn bias_does_not_overwrite_opposite_list() {
        use crate::types::SocialState;
        use rand::rngs::StdRng;
        use rand::SeedableRng;

        let pool: Vec<String> = vec!["drinking".to_string(), "children".to_string()];
        let mut rng = StdRng::seed_from_u64(1);
        // Healer prefers "drinking" as a dislike, but we've got it as a like —
        // the bias must skip it rather than introduce a contradiction.
        let mut social = SocialState {
            likes: vec!["drinking".into()],
            dislikes: vec!["children".into()],
            happiness: 50,
            ..SocialState::default()
        };
        bias_social_for_variation(&mut social, MigrantVariation::Healer, &pool, &mut rng);
        assert!(social.likes.contains(&"drinking".to_string()));
        for like in &social.likes {
            assert!(!social.dislikes.contains(like));
        }
    }

    #[test]
    fn apply_common_is_noop() {
        let mut m = fresh_mobile();
        let snapshot = (
            m.flags.guard,
            m.flags.no_attack,
            m.short_desc.clone(),
            m.long_desc.clone(),
            m.keywords.clone(),
        );
        let mut rng = thread_rng();
        apply_variation(&mut m, MigrantVariation::Common, &mut rng);
        assert_eq!(m.flags.guard, snapshot.0);
        assert_eq!(m.flags.no_attack, snapshot.1);
        assert_eq!(m.short_desc, snapshot.2);
        assert_eq!(m.long_desc, snapshot.3);
        assert_eq!(m.keywords, snapshot.4);
    }
}

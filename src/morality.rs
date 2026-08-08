//! Morality tiers and "feel" line text. Centralized so Rust, Rhai, and DG Scripts
//! all see one source of truth for the slider bounds, tier thresholds, and flavor strings.
//!
//! The slider is stored on `CharacterData.morality` as `i32` and clamped to
//! `[MORALITY_MIN, MORALITY_MAX]` (i.e. -200..=+200). Tier classification, however,
//! anchors at `±EVIL_PURE_THRESHOLD` / `±GOOD_PURE_THRESHOLD` (i.e. ±100). This gap is
//! deliberate: it gives extreme alignment a "reputation buffer" so a single contrary
//! deed can't immediately flip an entrenched Pure Evil player back to a milder tier.
//!
//! Most of this module is pure. The exception is [`apply_delta`], which is the
//! single place morality moves for callers that don't already hold the
//! character — kill credit and quest rewards both go through it, so the write,
//! the session sync and the tier announcement can't drift apart. Callers that
//! own a `&mut CharacterData` (the dialogue effect layer) use the pure
//! [`adjust`] and let their own save carry it.

pub const MORALITY_MIN: i32 = -200;
pub const MORALITY_MAX: i32 = 200;
pub const EVIL_PURE_THRESHOLD: i32 = -100;
pub const GOOD_PURE_THRESHOLD: i32 = 100;
pub const NEUTRAL_BAND: i32 = 24;

/// The nine bands, ascending. Thresholds and flavour text live here and
/// nowhere else; [`MoralityTier`] derives both from this by index.
///
/// The `description` column doubles as the "feel" line surfaced on `status`,
/// which is why Neutral's is empty — standing at the middle of the slider is
/// the absence of a moral character, not a mild one, and [`feel_message`]
/// returns `None` for it.
pub const LADDER: crate::tiers::TierLadder = crate::tiers::TierLadder {
    tiers: &[
        crate::tiers::Tier {
            key: "evil_pure",
            label: "Pure Evil",
            description: "You feel pure evil radiating from your very soul.",
            floor: i32::MIN,
        },
        crate::tiers::Tier {
            key: "evil_3",
            label: "Wicked",
            description: "Darkness has taken root within you.",
            floor: -99,
        },
        crate::tiers::Tier {
            key: "evil_2",
            label: "Cruel",
            description: "Cruelty comes naturally to you now.",
            floor: -74,
        },
        crate::tiers::Tier {
            key: "evil_1",
            label: "Unkind",
            description: "You feel wickedness creeping into your bones.",
            floor: -49,
        },
        crate::tiers::Tier {
            key: "neutral",
            label: "Neutral",
            description: "",
            floor: -NEUTRAL_BAND,
        },
        crate::tiers::Tier {
            key: "good_1",
            label: "Kind",
            description: "You feel a quiet warmth in your heart.",
            floor: NEUTRAL_BAND + 1,
        },
        crate::tiers::Tier {
            key: "good_2",
            label: "Compassionate",
            description: "Compassion comes easily to you.",
            floor: 50,
        },
        crate::tiers::Tier {
            key: "good_3",
            label: "Virtuous",
            description: "A bright virtue shines from within you.",
            floor: 75,
        },
        crate::tiers::Tier {
            key: "good_pure",
            label: "Pure",
            description: "You feel utterly pure of spirit.",
            floor: GOOD_PURE_THRESHOLD,
        },
    ],
};

/// Variant order must match [`LADDER`] exactly — `MoralityTier as usize`
/// indexes straight into it. Pinned by `enum_order_matches_the_ladder`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MoralityTier {
    EvilPure,
    Evil3,
    Evil2,
    Evil1,
    Neutral,
    Good1,
    Good2,
    Good3,
    GoodPure,
}

const MORALITY_TIERS: [MoralityTier; 9] = [
    MoralityTier::EvilPure,
    MoralityTier::Evil3,
    MoralityTier::Evil2,
    MoralityTier::Evil1,
    MoralityTier::Neutral,
    MoralityTier::Good1,
    MoralityTier::Good2,
    MoralityTier::Good3,
    MoralityTier::GoodPure,
];

impl MoralityTier {
    pub fn from_value(v: i32) -> Self {
        MORALITY_TIERS[LADDER.index_of(v)]
    }

    pub fn key(self) -> &'static str {
        LADDER.tiers[self as usize].key
    }

    /// Player-facing name, for surfaces that show a standing rather than a
    /// feeling — `status`, `standing`, leaderboards.
    pub fn label(self) -> &'static str {
        LADDER.tiers[self as usize].label
    }

    pub fn is_good(self) -> bool {
        matches!(self, Self::Good1 | Self::Good2 | Self::Good3 | Self::GoodPure)
    }

    pub fn is_evil(self) -> bool {
        matches!(self, Self::Evil1 | Self::Evil2 | Self::Evil3 | Self::EvilPure)
    }

    pub fn is_neutral(self) -> bool {
        matches!(self, Self::Neutral)
    }
}

/// Returns the status-line "feel" message, or None for the Neutral tier —
/// which has no line, because the middle of the slider is the absence of a
/// moral character rather than a mild one.
pub fn feel_message(v: i32) -> Option<&'static str> {
    let d = LADDER.tier_of(v).description;
    if d.is_empty() { None } else { Some(d) }
}

/// Clamp a value into the legal morality range.
pub fn clamp(v: i32) -> i32 {
    v.clamp(MORALITY_MIN, MORALITY_MAX)
}

/// Add `delta` to `current` morality, clamped into the legal range.
/// Returns the new value. Pure — caller is responsible for writing it
/// back onto the character and persisting.
pub fn adjust(current: i32, delta: i32) -> i32 {
    clamp(current.saturating_add(delta))
}

/// Returns the tier-shift announcement line for a morality move from
/// `before` to `after`, or `None` if the move didn't cross a tier
/// boundary. Used to surface dramatic shifts (e.g. crossing into Good3
/// after a virtuous achievement) without spamming on small nudges.
pub fn tier_shift_message(before: i32, after: i32) -> Option<&'static str> {
    LADDER.crossed(before, after)?;
    feel_message(after)
}

/// How far a kill moves the killer, given the victim's `alignment`.
///
/// The sign inverts: killing something evil pushes you toward Good, killing
/// something good pushes you toward Evil. Magnitude scales with how strongly
/// the victim was aligned, so a crusade against pure evil counts for more per
/// head than culling mildly unpleasant bandits.
///
/// A victim inside the neutral band carries no moral charge at all and returns
/// zero — that is the honest reading of a mobile nobody assigned a value to,
/// and it is also the default, so an unconsidered mob stays inert rather than
/// silently drifting every player who meets it.
///
/// Divided by 50 with a floor of 1, so the strongest possible victim moves you
/// 4 and it takes roughly 25 such kills to cross from Neutral into a Pure
/// tier. Slow enough that alignment reads as a career rather than an errand.
pub fn kill_delta(victim_alignment: i32) -> i32 {
    if victim_alignment.abs() <= NEUTRAL_BAND {
        return 0;
    }
    let magnitude = (victim_alignment.abs() / 50).max(1);
    if victim_alignment > 0 { -magnitude } else { magnitude }
}

/// Move a character's morality by `delta`, persist it, keep the live session
/// copy coherent, and announce the move if it crossed a tier boundary.
///
/// The one place morality changes for callers that do *not* already hold the
/// character — kill credit and quest rewards. Callers that own a
/// `&mut CharacterData` and persist it themselves (the dialogue effect layer)
/// must use the pure [`adjust`] instead: this function's write is out-of-band
/// and their later save would overwrite it.
///
/// Returns `(before, after)`, or `None` if nothing happened.
pub fn apply_delta(
    db: &crate::db::Db,
    connections: &crate::SharedConnections,
    char_name: &str,
    delta: i32,
) -> Option<(i32, i32)> {
    if delta == 0 {
        return None;
    }
    let mut ch = db.get_character_data(&char_name.to_lowercase()).ok().flatten()?;
    let before = ch.morality;
    let after = adjust(before, delta);
    if after == before {
        return None; // already pinned at a bound
    }
    ch.morality = after;
    db.save_character_data(ch).ok()?;

    if let Ok(mut conns) = connections.lock() {
        for (_, session) in conns.iter_mut() {
            let matches = session
                .character
                .as_ref()
                .map(|c| c.name.eq_ignore_ascii_case(char_name))
                .unwrap_or(false);
            if matches {
                if let Some(c) = session.character.as_mut() {
                    c.morality = after;
                }
                break;
            }
        }
    }

    if let Some(msg) = tier_shift_message(before, after) {
        crate::script::achievements::send_to_player(connections, char_name, msg);
    }
    Some((before, after))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_boundaries() {
        assert_eq!(MoralityTier::from_value(0), MoralityTier::Neutral);
        assert_eq!(MoralityTier::from_value(24), MoralityTier::Neutral);
        assert_eq!(MoralityTier::from_value(-24), MoralityTier::Neutral);
        assert_eq!(MoralityTier::from_value(25), MoralityTier::Good1);
        assert_eq!(MoralityTier::from_value(-25), MoralityTier::Evil1);
        assert_eq!(MoralityTier::from_value(49), MoralityTier::Good1);
        assert_eq!(MoralityTier::from_value(50), MoralityTier::Good2);
        assert_eq!(MoralityTier::from_value(74), MoralityTier::Good2);
        assert_eq!(MoralityTier::from_value(75), MoralityTier::Good3);
        assert_eq!(MoralityTier::from_value(99), MoralityTier::Good3);
        assert_eq!(MoralityTier::from_value(100), MoralityTier::GoodPure);
        assert_eq!(MoralityTier::from_value(200), MoralityTier::GoodPure);
        assert_eq!(MoralityTier::from_value(-100), MoralityTier::EvilPure);
        assert_eq!(MoralityTier::from_value(-150), MoralityTier::EvilPure);
        assert_eq!(MoralityTier::from_value(-99), MoralityTier::Evil3);
        assert_eq!(MoralityTier::from_value(-50), MoralityTier::Evil2);
        assert_eq!(MoralityTier::from_value(-49), MoralityTier::Evil1);
    }

    #[test]
    fn enum_order_matches_the_ladder() {
        // `MoralityTier as usize` indexes straight into LADDER, so a variant
        // inserted out of order would silently relabel every tier above it.
        assert_eq!(MORALITY_TIERS.len(), LADDER.tiers.len());
        for (i, t) in MORALITY_TIERS.iter().enumerate() {
            assert_eq!(*t as usize, i, "variant {:?} is out of position", t);
            assert_eq!(t.key(), LADDER.tiers[i].key);
        }
    }

    #[test]
    fn neutral_has_no_feel_message() {
        assert!(feel_message(0).is_none());
        assert!(feel_message(24).is_none());
        assert!(feel_message(-24).is_none());
    }

    #[test]
    fn extremes_get_pure_feel() {
        assert!(feel_message(100).unwrap().contains("pure"));
        assert!(feel_message(-100).unwrap().contains("pure evil"));
    }

    #[test]
    fn clamp_respects_bounds() {
        assert_eq!(clamp(500), 200);
        assert_eq!(clamp(-500), -200);
        assert_eq!(clamp(50), 50);
        assert_eq!(clamp(-50), -50);
    }

    #[test]
    fn a_morally_inert_victim_moves_nothing() {
        // 0 is the default, so every mobile nobody has thought about stays
        // out of the morality system entirely rather than quietly dragging
        // whoever kills it.
        assert_eq!(kill_delta(0), 0);
        // ...as does anything inside the neutral band.
        assert_eq!(kill_delta(NEUTRAL_BAND), 0);
        assert_eq!(kill_delta(-NEUTRAL_BAND), 0);
    }

    #[test]
    fn killing_evil_pushes_good_and_killing_good_pushes_evil() {
        assert!(kill_delta(-200) > 0, "killing pure evil is a virtuous act");
        assert!(kill_delta(200) < 0, "killing pure good is not");
        assert_eq!(kill_delta(-200), 4);
        assert_eq!(kill_delta(200), -4);
    }

    #[test]
    fn kill_magnitude_scales_with_how_aligned_the_victim_was() {
        // Just past the neutral band still counts for something...
        assert_eq!(kill_delta(-25), 1);
        assert_eq!(kill_delta(-49), 1, "the floor keeps small weights from rounding away");
        // ...and a worse monster counts for more.
        assert_eq!(kill_delta(-100), 2);
        assert_eq!(kill_delta(-150), 3);
    }

    #[test]
    fn a_pure_tier_is_roughly_a_campaign_not_an_errand() {
        // Twenty-five kills of the worst thing in the world to go from
        // Neutral to Pure Good. Alignment should read as a career.
        let per_kill = kill_delta(-200);
        let kills_to_pure = GOOD_PURE_THRESHOLD / per_kill;
        assert_eq!(kills_to_pure, 25);
    }

    fn temp_db() -> (crate::db::Db, tempfile::TempDir) {
        let temp = tempfile::tempdir().expect("temp dir");
        let db = crate::db::Db::open(temp.path()).expect("open db");
        (db, temp)
    }

    fn char_at(db: &crate::db::Db, name: &str, morality: i32) {
        let mut ch: crate::types::CharacterData = serde_json::from_value(serde_json::json!({
            "name": name,
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character");
        ch.morality = morality;
        db.save_character_data(ch).expect("save");
    }

    fn online(
        ch_name: &str,
        db: &crate::db::Db,
    ) -> (crate::SharedConnections, tokio::sync::mpsc::UnboundedReceiver<String>) {
        let (tx_client, rx_client) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (tx_input, _rx_input) = tokio::sync::mpsc::channel::<crate::InputEvent>(1);
        let mut session = crate::PlayerSession::new_for_test(tx_client, tx_input);
        session.character = db.get_character_data(&ch_name.to_lowercase()).expect("read");
        let conns: crate::SharedConnections =
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        conns.lock().unwrap().insert(uuid::Uuid::new_v4(), session);
        (conns, rx_client)
    }

    fn drain(rx: &mut tokio::sync::mpsc::UnboundedReceiver<String>) -> String {
        let mut out = String::new();
        while let Ok(m) = rx.try_recv() {
            out.push_str(&m);
        }
        out
    }

    #[test]
    fn apply_delta_persists_syncs_the_session_and_announces_a_tier_crossing() {
        let (db, _t) = temp_db();
        char_at(&db, "saint", 24); // top of Neutral
        let (conns, mut rx) = online("saint", &db);

        let (before, after) = apply_delta(&db, &conns, "saint", 4).expect("moved");
        assert_eq!((before, after), (24, 28));

        assert_eq!(db.get_character_data("saint").unwrap().unwrap().morality, 28);
        let session_value = conns
            .lock()
            .unwrap()
            .values()
            .next()
            .and_then(|s| s.character.as_ref().map(|c| c.morality));
        assert_eq!(
            session_value,
            Some(28),
            "the live copy must not go stale, or the regen tick flushes it back"
        );
        assert!(drain(&mut rx).contains("quiet warmth"), "crossing into Good1 announces");
    }

    #[test]
    fn apply_delta_is_silent_within_a_tier() {
        let (db, _t) = temp_db();
        char_at(&db, "saint", 30); // already Good1
        let (conns, mut rx) = online("saint", &db);

        assert_eq!(apply_delta(&db, &conns, "saint", 4), Some((30, 34)));
        assert_eq!(drain(&mut rx), "", "sub-tier nudges must not spam");
    }

    #[test]
    fn apply_delta_does_nothing_at_the_bound_or_on_a_zero_delta() {
        let (db, _t) = temp_db();
        char_at(&db, "saint", MORALITY_MAX);
        let (conns, _rx) = online("saint", &db);

        assert_eq!(apply_delta(&db, &conns, "saint", 10), None, "already pinned");
        assert_eq!(apply_delta(&db, &conns, "saint", 0), None);
        assert_eq!(db.get_character_data("saint").unwrap().unwrap().morality, MORALITY_MAX);
    }

    #[test]
    fn sticky_extreme_buffer() {
        // -150 is Pure Evil; +30 leaves us at -120 = still Pure Evil.
        assert_eq!(MoralityTier::from_value(-150 + 30), MoralityTier::EvilPure);
        // Another +30 puts us at -90 = Evil3.
        assert_eq!(MoralityTier::from_value(-90), MoralityTier::Evil3);
    }
}

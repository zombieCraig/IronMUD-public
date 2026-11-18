//! The late-game unlock: may this character petition this god?
//!
//! [`petition_status`] is the single source: the `pray petition` checklist
//! renders it line by line, and [`can_petition`] is just "every line met",
//! so the two can never disagree.

use crate::db::Db;
use crate::types::{CharacterData, DeityConfig, DeitySpellcraft};

use super::Settings;

/// Counter bumped once per spell the first time it reaches mastery 10.
pub const SPELLS_MASTERED_COUNTER: &str = "spells.mastered";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    /// Stable key, for tests and scripts.
    pub key: &'static str,
    pub label: String,
    pub met: bool,
    /// "have / need" or a short parenthetical.
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PetitionStatus {
    pub god_vnum: String,
    pub god_name: String,
    /// The god grants player spells at all (`DeityConfig.spellcraft`).
    pub god_grants: bool,
    pub requirements: Vec<Requirement>,
}

impl PetitionStatus {
    pub fn can_petition(&self) -> bool {
        self.god_grants && self.requirements.iter().all(|r| r.met)
    }
}

/// Spells this character has mastered: the lifetime counter, or the current
/// mastery-10 entries for characters who mastered spells before the counter
/// existed (evolution removes the old entry, so the counter is the better
/// record going forward).
pub fn spells_mastered(ch: &CharacterData) -> i64 {
    let current = ch.spell_progress.values().filter(|p| p.level >= 10).count() as i64;
    let counted = ch
        .achievement_counters
        .get(SPELLS_MASTERED_COUNTER)
        .copied()
        .unwrap_or(0) as i64;
    current.max(counted)
}

fn skill(ch: &CharacterData, key: &str) -> i64 {
    ch.skills.get(key).map(|s| s.level as i64).unwrap_or(0)
}

fn at_least(key: &'static str, label: &str, have: i64, need: i64) -> Requirement {
    Requirement {
        key,
        label: label.to_string(),
        met: have >= need,
        detail: format!("{have} / {need}"),
    }
}

/// Everything time-dependent, passed in so tests can pin it.
#[derive(Debug, Clone, Copy)]
pub struct Clock {
    /// Absolute game day (`GameTime::absolute_day`).
    pub today: i64,
    /// Unix seconds.
    pub now_secs: i64,
}

impl Clock {
    pub fn now(db: &Db) -> Self {
        Clock {
            today: crate::script::worship::current_absolute_day(db),
            now_secs: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
        }
    }
}

/// The full checklist for `ch` petitioning the god `god_vnum`. Pure apart
/// from DB reads (deity prototype, quest name, canon record).
pub fn petition_status(
    db: &Db,
    ch: &CharacterData,
    god_vnum: &str,
    settings: &Settings,
    clock: Clock,
) -> PetitionStatus {
    let god_name = {
        let n = crate::script::worship::god_display_name(db, god_vnum);
        if n.is_empty() { god_vnum.to_string() } else { n }
    };
    let deity: Option<DeityConfig> = crate::script::worship::deity_config_by_vnum(db, god_vnum);
    let craft: Option<DeitySpellcraft> = deity.as_ref().and_then(|d| d.spellcraft.clone());
    let mut reqs = Vec::new();

    let worship = ch.worship.as_ref().filter(|w| w.god_vnum == god_vnum);
    reqs.push(Requirement {
        key: "worship",
        label: format!("Worship {god_name}"),
        met: worship.is_some(),
        detail: if worship.is_some() {
            "(you do)".into()
        } else {
            "(you do not)".into()
        },
    });

    reqs.push(at_least(
        "magic",
        "Magic mastered",
        skill(ch, "magic"),
        settings.min_magic,
    ));
    reqs.push(at_least(
        "spellcraft",
        "Spellcraft",
        skill(ch, "spellcraft"),
        settings.min_spellcraft,
    ));
    reqs.push(at_least(
        "mastered_spells",
        "Spells mastered",
        spells_mastered(ch),
        settings.min_mastered_spells,
    ));

    let min_favor = craft.as_ref().and_then(|c| c.min_favor).unwrap_or(settings.min_favor);
    let favor = worship.map(|w| w.favor as i64).unwrap_or(0);
    let tier = crate::worship::favor::FavorTier::from_value(min_favor as i32);
    reqs.push(at_least("favor", &format!("Favor: {}", tier.label()), favor, min_favor));

    if let Some(quest_id) = craft.as_ref().and_then(|c| c.unlock_quest_id.clone()) {
        let name = db
            .get_quest_data(&quest_id)
            .ok()
            .flatten()
            .map(|q| q.name)
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| quest_id.clone());
        let done = ch.completed_quests.contains(&quest_id);
        let detail = if done {
            "(complete)"
        } else if ch.active_quests.contains_key(&quest_id) {
            "(in progress)"
        } else {
            "(not begun)"
        };
        reqs.push(Requirement {
            key: "unlock_quest",
            label: name,
            met: done,
            detail: detail.into(),
        });
    }

    let interval = deity.as_ref().map(|d| d.tribute_interval_days).unwrap_or(3);
    let calm = worship.is_some_and(|w| {
        w.anger_stage == 0 && crate::worship::stage_for_overdue(w.overdue_days(clock.today, interval)) == 0
    });
    reqs.push(Requirement {
        key: "calm",
        label: format!("{god_name} is calm with you"),
        met: calm,
        detail: if calm {
            String::new()
        } else {
            "(your tribute is owed)".into()
        },
    });

    let canon = db.get_canon(god_vnum).unwrap_or_default();
    let cap = craft.as_ref().map(|c| c.canon_cap).unwrap_or(0);
    let room = canon.has_room(cap, clock.today, settings.canon_grace_days);
    reqs.push(Requirement {
        key: "canon_room",
        label: format!("{god_name}'s canon has room"),
        met: room,
        detail: if room {
            format!("({} of {} held)", canon.slots.len(), cap)
        } else {
            format!("(all {cap} slots within their grace period)")
        },
    });

    let god_cd = craft
        .as_ref()
        .and_then(|c| c.petition_cooldown_days)
        .unwrap_or(settings.god_petition_cooldown_days);
    let god_ready_day = canon.last_petition_day.map(|d| d + god_cd);
    let god_ready = god_ready_day.is_none_or(|d| clock.today >= d);
    reqs.push(Requirement {
        key: "god_cooldown",
        label: format!("{god_name} will hear a petition"),
        met: god_ready,
        detail: match god_ready_day {
            Some(d) if !god_ready => format!("(in {} game day(s))", d - clock.today),
            _ => String::new(),
        },
    });

    let char_ready_at = ch.last_petition_at.map(|t| t + settings.char_cooldown_days * 86_400);
    let char_ready = char_ready_at.is_none_or(|t| clock.now_secs >= t);
    reqs.push(Requirement {
        key: "char_cooldown",
        label: "Your last petition".into(),
        met: char_ready,
        detail: match (ch.last_petition_at, char_ready_at) {
            (None, _) => "(never)".into(),
            (Some(_), Some(t)) if !char_ready => {
                let days = (t - clock.now_secs + 86_399) / 86_400;
                format!("(you may ask again in {days} day(s))")
            }
            _ => "(long enough ago)".into(),
        },
    });

    PetitionStatus {
        god_vnum: god_vnum.to_string(),
        god_name,
        god_grants: craft.is_some(),
        requirements: reqs,
    }
}

/// True when every requirement is met. Never computed any other way.
pub fn can_petition(db: &Db, ch: &CharacterData, god_vnum: &str, settings: &Settings, clock: Clock) -> bool {
    petition_status(db, ch, god_vnum, settings, clock).can_petition()
}


use crate::import::{
    IrQuest, PlannedQuest, Severity, Warning, WarningKind,
};
use crate::types::{
    QuestData, QuestObjective,
    QuestReward,
};


/// Translate a tbamud `.qst` IR record into a [`PlannedQuest`]. Returns
/// `None` for quest types we can't model yet (MOB_FIND / MOB_SAVE /
/// ROOM_CLEAR) — those produce a warn and no row.
pub(super) fn translate_quest(q: &IrQuest, warnings: &mut Vec<Warning>) -> Option<PlannedQuest> {
    let vnum = format!("qst:{}", q.vnum);
    let name = if q.name.is_empty() {
        format!("Quest #{}", q.vnum)
    } else {
        q.name.clone()
    };
    let keywords: Vec<String> = q
        .keywords
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();

    let summary = first_line_compact(&q.accept_msg, 80);
    let description = clean_msg(&q.accept_msg);
    let completion_text = clean_msg(&q.complete_msg);

    let objective = match q.quest_type {
        // AQ_OBJ_FIND
        0 => Some(QuestObjective::BringItem {
            vnum: q.target_vnum.to_string(),
            qty: q.quantity.max(1),
            return_to_mob_vnum: None,
        }),
        // AQ_ROOM_FIND — listener defers to slice 2.
        1 => Some(QuestObjective::VisitRoom {
            vnum: q.target_vnum.to_string(),
        }),
        // AQ_MOB_FIND — semantics ("find this mob alive") need a tracker
        // we don't have; skip for slice 1.
        2 => {
            warnings.push(
                Warning::new(
                    WarningKind::DeferredFeature,
                    Severity::Warn,
                    q.source.clone(),
                    format!(
                        "quest #{} ({}): AQ_MOB_FIND not modelled; skipped",
                        q.vnum, name
                    ),
                )
                .with_suggestion(
                    "re-author as a kill / fetch quest, or wait for slice 3 'find alive' tracker",
                ),
            );
            return None;
        }
        // AQ_MOB_KILL
        3 => Some(QuestObjective::KillMob {
            vnum: q.target_vnum.to_string(),
            count: q.quantity.max(1),
        }),
        // AQ_MOB_SAVE — rescue mechanic doesn't exist.
        4 => {
            warnings.push(
                Warning::new(
                    WarningKind::DeferredFeature,
                    Severity::Warn,
                    q.source.clone(),
                    format!("quest #{} ({}): AQ_MOB_SAVE not modelled; skipped", q.vnum, name),
                )
                .with_suggestion("rescue mechanic deferred"),
            );
            return None;
        }
        // AQ_OBJ_RETURN
        5 => Some(QuestObjective::BringItem {
            vnum: q.target_vnum.to_string(),
            qty: q.quantity.max(1),
            return_to_mob_vnum: if q.qm_vnum > 0 {
                Some(q.qm_vnum.to_string())
            } else {
                None
            },
        }),
        // AQ_ROOM_CLEAR
        6 => {
            warnings.push(
                Warning::new(
                    WarningKind::DeferredFeature,
                    Severity::Warn,
                    q.source.clone(),
                    format!(
                        "quest #{} ({}): AQ_ROOM_CLEAR not modelled; skipped",
                        q.vnum, name
                    ),
                )
                .with_suggestion("clear-all-mobs-from-room mechanic deferred"),
            );
            return None;
        }
        other => {
            warnings.push(Warning::new(
                WarningKind::DeferredFeature,
                Severity::Warn,
                q.source.clone(),
                format!(
                    "quest #{} ({}): unknown AQ type {} — skipped",
                    q.vnum, name, other
                ),
            ));
            return None;
        }
    };

    let mut objectives = Vec::new();
    if let Some(o) = objective {
        objectives.push(o);
    }

    let mut rewards = Vec::new();
    if q.gold_reward > 0 {
        rewards.push(QuestReward::Gold { amount: q.gold_reward });
    }
    if q.obj_reward_vnum > 0 {
        rewards.push(QuestReward::Item {
            vnum: q.obj_reward_vnum.to_string(),
            qty: 1,
        });
    }

    let repeatable = (q.flags & 0x1) != 0;

    let giver_mob_vnum = if q.qm_vnum > 0 {
        Some(q.qm_vnum.to_string())
    } else {
        None
    };

    let quest_data = QuestData {
        vnum,
        name,
        keywords,
        summary,
        description,
        completion_text,
        objectives,
        rewards,
        repeatable,
        giver_mob_vnum,
        prereq_quest_vnum: None,
        min_player_skill_total: None,
        duration_secs: None,
        achievement_set_prereq: None,
    };

    Some(PlannedQuest {
        quest_data,
        source: q.source.clone(),
    })
}

/// Strip trailing tilde-line, collapse trailing whitespace, return as-is.
pub(super) fn clean_msg(s: &str) -> String {
    s.trim_end_matches('\n')
        .trim_end_matches('~')
        .trim_end_matches('\n')
        .to_string()
}

/// Pick a one-line summary from the start of a multi-line accept message.
pub(super) fn first_line_compact(s: &str, max_len: usize) -> String {
    let line = s.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    if line.len() > max_len {
        let mut truncated: String = line.chars().take(max_len.saturating_sub(1)).collect();
        truncated.push('…');
        truncated
    } else {
        line.to_string()
    }
}

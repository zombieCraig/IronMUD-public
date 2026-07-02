//! Worship anger-tick processing — testable lib-side. The thin tokio loop
//! wrapper lives in `src/ticks/worship.rs` (bin-only) and just calls these.
//!
//! Anger is derived, not stored: overdue days come from
//! `WorshipState.last_tribute_day` vs the current absolute game day, and
//! `anger_stage` records the highest ladder stage already fired so each
//! stage triggers exactly once per lapse. Paying tribute (or atoning)
//! resets the stage to 0 and the ladder re-arms.

use crate::SharedConnections;
use crate::db::Db;
use crate::script::worship::{
    current_absolute_day, deity_config_by_vnum, god_display_name, send_to_player, set_worship_anger_stage,
    smite_worshiper,
};
use crate::types::MobileTriggerType;
use anyhow::Result;
use std::sync::Arc;

/// Anger-ladder scan cadence. One game day is 48 real minutes, so a 60s
/// tick catches day rollovers promptly without meaningful scan cost.
pub const WORSHIP_TICK_INTERVAL_SECS: u64 = 60;

/// Ladder stage implied by days overdue. Stage 0 = content.
pub fn stage_for_overdue(overdue_days: i64) -> i32 {
    match overdue_days {
        d if d >= 10 => 4,
        d if d >= 6 => 3,
        d if d >= 3 => 2,
        d if d >= 1 => 1,
        _ => 0,
    }
}

/// Scan online worshipers and escalate any whose tribute has lapsed.
/// Offline players escalate on their first tick back online — the day
/// math is absolute, so nothing is missed, only deferred.
pub fn process_worship_tick(db: &Db, connections: &SharedConnections) -> Result<()> {
    let today = current_absolute_day(db);
    // Snapshot names under the lock, then release it — the capability fns
    // called during escalation take the connections lock themselves.
    let worshipers: Vec<String> = {
        let conns = connections.lock().unwrap();
        conns
            .values()
            .filter_map(|s| s.character.as_ref())
            .filter(|c| c.creation_complete && !c.god_mode && c.worship.is_some())
            .map(|c| c.name.clone())
            .collect()
    };
    for name in worshipers {
        process_worshiper(db, connections, &name, today);
    }
    Ok(())
}

/// Escalate one worshiper if their overdue days imply a higher ladder stage
/// than the one already fired. Public so integration tests can drive it
/// directly (works for offline/DB-only characters too).
pub fn process_worshiper(db: &Db, connections: &SharedConnections, char_name: &str, today: i64) {
    let character = match db
        .get_character_data(&char_name.to_lowercase())
        .ok()
        .flatten()
        .or_else(|| snapshot_online(connections, char_name))
    {
        Some(c) => c,
        None => return,
    };
    let worship = match character.worship {
        Some(ref w) => w.clone(),
        None => return,
    };
    let config = match deity_config_by_vnum(db, &worship.god_vnum) {
        Some(c) => c,
        None => return,
    };
    let overdue = worship.overdue_days(today, config.tribute_interval_days);
    let target_stage = stage_for_overdue(overdue);
    if target_stage <= worship.anger_stage {
        return;
    }
    // Stamp the stage before acting so a mid-escalation failure can't
    // re-fire the same stage every tick.
    set_worship_anger_stage(db, connections, char_name, target_stage);

    match target_stage {
        1 => {
            let god_name = {
                let n = god_display_name(db, &worship.god_vnum);
                if n.is_empty() { "your god".to_string() } else { n }
            };
            send_to_player(
                connections,
                char_name,
                &format!(
                    "\x1b[33mA cold weight settles on your thoughts: {} awaits tribute, {} day{} overdue. Pray at a temple.\x1b[0m",
                    god_name,
                    overdue,
                    if overdue == 1 { "" } else { "s" },
                ),
            );
        }
        2 => {
            smite_worshiper(db, connections, char_name, 2);
        }
        stage => {
            // Stages 3-4: give the god's OnSmite DG trigger first refusal.
            // Return(0) from the trigger cancels the default smite.
            if !fire_on_smite(db, connections, char_name, &worship.god_vnum, stage, overdue) {
                smite_worshiper(db, connections, char_name, stage);
            }
        }
    }
}

/// Fire the god's OnSmite DG triggers (live instance preferred, prototype
/// fallback). Returns true when a trigger cancelled the default smite.
fn fire_on_smite(
    db: &Db,
    connections: &SharedConnections,
    char_name: &str,
    god_vnum: &str,
    severity: i32,
    overdue_days: i64,
) -> bool {
    let god = db
        .get_mobile_instances_by_vnum(god_vnum)
        .ok()
        .and_then(|v| v.into_iter().find(|m| m.current_hp > 0))
        .or_else(|| db.get_mobile_by_vnum(god_vnum).ok().flatten());
    let god = match god {
        Some(g)
            if g.triggers
                .iter()
                .any(|t| t.trigger_type == MobileTriggerType::OnSmite && t.enabled) =>
        {
            g
        }
        _ => return false,
    };
    let connection_id = connection_id_for(connections, char_name);
    let mut context = std::collections::HashMap::new();
    context.insert("severity".to_string(), severity.to_string());
    context.insert("overdue_days".to_string(), overdue_days.to_string());
    let db_arc = Arc::new(db.clone());
    crate::script::dg::fire_mobile_dg_triggers_with_context(
        &db_arc,
        connections,
        &god,
        MobileTriggerType::OnSmite,
        &connection_id,
        "",
        "",
        "",
        context,
    )
}

fn connection_id_for(connections: &SharedConnections, char_name: &str) -> String {
    let conns = connections.lock().unwrap();
    for (id, session) in conns.iter() {
        if let Some(ref ch) = session.character {
            if ch.name.eq_ignore_ascii_case(char_name) {
                return id.to_string();
            }
        }
    }
    String::new()
}

fn snapshot_online(connections: &SharedConnections, char_name: &str) -> Option<crate::CharacterData> {
    let conns = connections.lock().unwrap();
    for session in conns.values() {
        if let Some(ref ch) = session.character {
            if ch.name.eq_ignore_ascii_case(char_name) {
                return Some(ch.clone());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DeityConfig, EffectType, GameTime, MobileData, WorshipState};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    fn open_temp() -> (Db, tempfile::TempDir) {
        let temp = tempfile::tempdir().expect("tempdir");
        let db = Db::open(temp.path()).expect("open db");
        (db, temp)
    }

    fn empty_connections() -> SharedConnections {
        Arc::new(Mutex::new(HashMap::new()))
    }

    fn base_char(name: &str) -> crate::CharacterData {
        let mut c: crate::CharacterData = serde_json::from_value(serde_json::json!({
            "name": name,
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character");
        c.name = name.to_string();
        c.hp = 100;
        c.max_hp = 100;
        c
    }

    fn god_proto(db: &Db, vnum: &str) {
        let mut m = MobileData::new(format!("god-{}", vnum));
        m.vnum = vnum.to_string();
        m.is_prototype = true;
        m.deity = Some(DeityConfig::default()); // interval 3 days
        db.save_mobile_data(m).expect("save god");
    }

    #[test]
    fn stage_thresholds() {
        assert_eq!(stage_for_overdue(0), 0);
        assert_eq!(stage_for_overdue(1), 1);
        assert_eq!(stage_for_overdue(2), 1);
        assert_eq!(stage_for_overdue(3), 2);
        assert_eq!(stage_for_overdue(5), 2);
        assert_eq!(stage_for_overdue(6), 3);
        assert_eq!(stage_for_overdue(9), 3);
        assert_eq!(stage_for_overdue(10), 4);
        assert_eq!(stage_for_overdue(100), 4);
    }

    #[test]
    fn absolute_day_boundary_math() {
        let mut t = GameTime::default();
        t.day = 1;
        t.month = 1;
        t.year = 1;
        assert_eq!(t.absolute_day(), 0);
        t.day = 30;
        assert_eq!(t.absolute_day(), 29);
        t.day = 1;
        t.month = 2;
        assert_eq!(t.absolute_day(), 30);
        t.month = 1;
        t.year = 2;
        assert_eq!(t.absolute_day(), 360);
    }

    #[test]
    fn ladder_fires_each_stage_once_and_tribute_rearms() {
        let (db, _t) = open_temp();
        let conns = empty_connections();
        god_proto(&db, "pantheon:stern");
        let mut c = base_char("lapsed");
        c.worship = Some(WorshipState::new("pantheon:stern", 0));
        db.save_character_data(c).unwrap();

        // Day 4: 1 day overdue (interval 3) -> stage 1, warning only.
        process_worshiper(&db, &conns, "lapsed", 4);
        let c = db.get_character_data("lapsed").unwrap().unwrap();
        assert_eq!(c.worship.as_ref().unwrap().anger_stage, 1);
        assert!(c.active_buffs.is_empty());

        // Same day again: no re-fire.
        process_worshiper(&db, &conns, "lapsed", 4);
        let c = db.get_character_data("lapsed").unwrap().unwrap();
        assert_eq!(c.worship.as_ref().unwrap().anger_stage, 1);

        // Day 6: 3 days overdue -> stage 2 (forsaken: curse).
        process_worshiper(&db, &conns, "lapsed", 6);
        let c = db.get_character_data("lapsed").unwrap().unwrap();
        assert_eq!(c.worship.as_ref().unwrap().anger_stage, 2);
        assert!(c.active_buffs.iter().any(|b| b.effect_type == EffectType::Curse));

        // Day 9: 6 days overdue -> stage 3 smite (blind + hp damage).
        process_worshiper(&db, &conns, "lapsed", 9);
        let c = db.get_character_data("lapsed").unwrap().unwrap();
        assert_eq!(c.worship.as_ref().unwrap().anger_stage, 3);
        assert!(c.active_buffs.iter().any(|b| b.effect_type == EffectType::Blind));
        assert!(c.hp < 100);

        // Day 13: 10 days overdue -> stage 4, but permanent smite is off by
        // default so it lands as a repeat severity-3 smite.
        process_worshiper(&db, &conns, "lapsed", 13);
        let c = db.get_character_data("lapsed").unwrap().unwrap();
        assert_eq!(c.worship.as_ref().unwrap().anger_stage, 4);
        let blind = c
            .active_buffs
            .iter()
            .find(|b| b.effect_type == EffectType::Blind)
            .expect("blind");
        assert_ne!(blind.remaining_secs, -1);

        // Tribute re-arms the ladder from zero.
        crate::script::worship::record_tribute(&db, &conns, "lapsed");
        let c = db.get_character_data("lapsed").unwrap().unwrap();
        assert_eq!(c.worship.as_ref().unwrap().anger_stage, 0);
    }

    #[test]
    fn offline_jump_lands_on_highest_stage_only() {
        let (db, _t) = open_temp();
        let conns = empty_connections();
        god_proto(&db, "pantheon:stern");
        let mut c = base_char("absent");
        c.worship = Some(WorshipState::new("pantheon:stern", 0));
        db.save_character_data(c).unwrap();

        // Long absence: straight to stage 4 in one pass, no intermediate spam.
        process_worshiper(&db, &conns, "absent", 50);
        let c = db.get_character_data("absent").unwrap().unwrap();
        assert_eq!(c.worship.as_ref().unwrap().anger_stage, 4);
    }
}

//! Necromancy capability — raise a controllable undead minion from a corpse.
//!
//! This is the *capability* layer: a single shared function that performs the
//! whole rite against the database, with no dependency on a particular
//! scripting layer. It is exposed two ways, by thin adapters that add no logic:
//!   - the DG verb `raise_dead` (`src/script/dg/cmds.rs::cmd_raise_dead`), and
//!   - the Rhai binding `raise_dead_from_corpse` (`src/script/mobiles.rs`).
//!
//! Per-entity config (which item grants it, the spoken word, the tuning) lives
//! at the builder layer — e.g. an item's `OnCommand` DG trigger calling
//! `raise_dead %arg% <mana> <morality> <mastery_xp>`. Possession is enforced
//! for free: OnCommand only fires for an item the actor is carrying, so no vnum
//! lives in core.
//!
//! Mechanics:
//!   - Only non-player corpses with a known source prototype can be raised.
//!   - The corpse's creature level must be ≤ the caster's `necromancy` mastery.
//!   - Success scales with the caster's `magic` skill (works at 0 — low odds).
//!   - The number of bound dead scales with mastery (`1 + mastery/3`).
//!   - A successful rite costs mana, stains morality, and improves `necromancy`
//!     (raising the level cap over time). A botched rite still costs mana.

use rand::Rng;
use uuid::Uuid;

use crate::db::Db;

/// Builder-tunable costs for one rite (passed from the item's DG trigger /
/// the Rhai caller, with the adapters supplying defaults).
pub struct RaiseParams {
    pub mana_cost: i32,
    pub morality_cost: i32,
    pub mastery_xp: i32,
}

/// Result of an attempt. The capability performs all DB mutations itself;
/// the adapters only relay the messages to the relevant scripting layer's I/O.
/// `caster_msg` / `room_msg` already include their trailing newline.
pub struct RaiseOutcome {
    /// True only when a minion was actually raised.
    pub success: bool,
    /// Line for the caster. Empty when there is no one to tell (no caster).
    pub caster_msg: String,
    /// Line broadcast to the rest of the room (excluding the caster).
    pub room_msg: Option<String>,
    /// Room the rite resolved in (for the room broadcast).
    pub room_id: Option<Uuid>,
    /// The raised minion's instance id, on success.
    pub minion_id: Option<Uuid>,
}

/// Skill key that gates the corpse level and improves with use.
const MASTERY_SKILL: &str = "necromancy";
/// Skill key whose level drives the success roll ("magic ability").
const MAGIC_SKILL: &str = "magic";

fn refuse(msg: &str) -> RaiseOutcome {
    RaiseOutcome {
        success: false,
        caster_msg: if msg.is_empty() {
            String::new()
        } else {
            format!("{msg}\n")
        },
        room_msg: None,
        room_id: None,
        minion_id: None,
    }
}

/// Perform the raise-dead rite for `caster_name` on a corpse matching
/// `corpse_keyword` in the caster's current room. See module docs.
pub fn raise_dead_from_corpse(db: &Db, caster_name: &str, corpse_keyword: &str, p: RaiseParams) -> RaiseOutcome {
    if corpse_keyword.trim().is_empty() {
        return refuse("Raise which corpse?");
    }
    let Ok(Some(mut ch)) = db.get_character_data(caster_name) else {
        return refuse("");
    };
    let room_id = ch.current_room_id;

    // Find a (non-player) corpse in the room by keyword / name.
    let Ok(items) = db.get_items_in_room(&room_id) else {
        return refuse("There is no corpse here by that name.");
    };
    let kw = corpse_keyword.trim().to_ascii_lowercase();
    let corpse = items.into_iter().find(|it| {
        it.flags.is_corpse
            && (it.keywords.iter().any(|k| k.eq_ignore_ascii_case(&kw)) || it.name.to_ascii_lowercase().contains(&kw))
    });
    let Some(corpse) = corpse else {
        return refuse("There is no corpse here by that name.");
    };
    if corpse.flags.corpse_is_player {
        return refuse("You cannot raise the body of a fallen adventurer.");
    }
    let Some(source_vnum) = corpse.flags.corpse_source_vnum.clone() else {
        return refuse("This corpse is too far gone to raise.");
    };

    // Prototype tells us the creature's level for the cap check.
    let Ok(Some(proto)) = db.get_mobile_by_vnum(&source_vnum) else {
        return refuse("This corpse is too far gone to raise.");
    };
    let corpse_level = proto.level;

    let mastery = ch.skills.get(MASTERY_SKILL).map(|s| s.level).unwrap_or(0);
    let magic = ch.skills.get(MAGIC_SKILL).map(|s| s.level).unwrap_or(0);

    if corpse_level > mastery {
        return refuse(&format!(
            "This soul is too strong for your mastery (it was level {corpse_level}, your necromancy is {mastery}). You need more practice."
        ));
    }
    if ch.mana < p.mana_cost {
        return refuse(&format!(
            "You lack the mana to attempt this (need {}, have {}).",
            p.mana_cost, ch.mana
        ));
    }

    // Minion cap scales with mastery.
    let cap = 1 + mastery / 3;
    let current = count_charmed_minions(db, caster_name);
    if current >= cap {
        return refuse(&format!(
            "Your will cannot bind another — you already command {current} of the dead (limit {cap})."
        ));
    }

    // Success roll, driven by magic ability and tempered by corpse level.
    let chance = (40 + magic * 4 - corpse_level * 3).clamp(5, 95);
    let roll = rand::thread_rng().gen_range(1..=100);
    if roll > chance {
        // Botched rite: mana is spent, no minion, no morality hit.
        ch.mana = (ch.mana - p.mana_cost).max(0);
        let _ = db.save_character_data(ch);
        return RaiseOutcome {
            success: false,
            caster_msg: "The corpse twitches and goes still. Your call was not answered.\n".to_string(),
            room_msg: Some(format!("{caster_name} intones over a corpse, but nothing answers.\n")),
            room_id: Some(room_id),
            minion_id: None,
        };
    }

    // Success: consume the corpse and raise its source mob.
    let _ = db.delete_item(&corpse.id);
    let Ok(Some(mut pet)) = db.spawn_mobile_from_prototype(&source_vnum) else {
        // Spawn refused (e.g. world-max cap). Still costs mana.
        ch.mana = (ch.mana - p.mana_cost).max(0);
        let _ = db.save_character_data(ch);
        return RaiseOutcome {
            success: false,
            caster_msg: "The dark magic fizzles — the body refuses to rise.\n".to_string(),
            room_msg: None,
            room_id: Some(room_id),
            minion_id: None,
        };
    };

    // Stamp it as a weakened, charmed undead at the caster's feet.
    pet.current_room_id = Some(room_id);
    let weakened = (pet.max_hp * 60 / 100).max(1);
    pet.max_hp = weakened;
    pet.current_hp = weakened;
    pet.short_desc = format!("the risen corpse of {}", pet.short_desc);
    pet.active_buffs.push(crate::ActiveBuff {
        effect_type: crate::EffectType::Charmed,
        magnitude: 0,
        remaining_secs: -1, // permanent; released by break_all_charms_by_player on death/quit
        source: caster_name.to_string(),
        damage_type: None,
        vs_effect: None,
        skill_key: None,
    });
    let minion_id = pet.id;
    let risen_desc = pet.short_desc.clone();
    let _ = db.save_mobile_data(pet);

    // Costs + progression on the caster.
    ch.mana = (ch.mana - p.mana_cost).max(0);
    ch.morality = (ch.morality as i64).saturating_sub(p.morality_cost as i64).clamp(
        crate::morality::MORALITY_MIN as i64,
        crate::morality::MORALITY_MAX as i64,
    ) as i32;
    let _ = crate::script::dialogue::award_skill_xp(&mut ch, MASTERY_SKILL, p.mastery_xp);
    let _ = db.save_character_data(ch);

    RaiseOutcome {
        success: true,
        caster_msg: format!(
            "\u{1b}[1;31mYou intone the words; {risen_desc} claws upright, bound to your will. The act stains your soul.\u{1b}[0m\n"
        ),
        room_msg: Some(format!(
            "{caster_name} intones dark words, and {risen_desc} claws its way upright.\n"
        )),
        room_id: Some(room_id),
        minion_id: Some(minion_id),
    }
}

/// Count live (non-prototype) mobiles currently bound to `caster_name` via a
/// Charmed/Dominated buff. Mirrors the enumeration in
/// [`crate::break_all_charms_by_player`].
fn count_charmed_minions(db: &Db, caster_name: &str) -> i32 {
    let Ok(mobiles) = db.list_all_mobiles() else {
        return 0;
    };
    mobiles
        .iter()
        .filter(|m| {
            !m.is_prototype
                && m.active_buffs.iter().any(|b| {
                    (b.effect_type == crate::EffectType::Charmed || b.effect_type == crate::EffectType::Dominated)
                        && b.source.eq_ignore_ascii_case(caster_name)
                })
        })
        .count() as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpse::CorpseBuilder;
    use crate::db::Db;
    use crate::types::{CharacterData, MobileData};

    fn open_temp() -> (Db, tempfile::TempDir) {
        let temp = tempfile::tempdir().expect("tempdir");
        let db = Db::open(temp.path()).expect("open db");
        (db, temp)
    }

    fn make_char(db: &Db, name: &str, room: Uuid, mana: i32, necro: i32, magic: i32) {
        let ch: CharacterData = serde_json::from_value(serde_json::json!({
            "name": name,
            "password_hash": "",
            "current_room_id": room,
            "mana": mana,
            "morality": 0,
            "skills": {
                "necromancy": {"level": necro, "experience": 0},
                "magic": {"level": magic, "experience": 0},
            },
        }))
        .expect("build character");
        db.save_character_data(ch).expect("save char");
    }

    fn save_proto(db: &Db, vnum: &str, level: i32, max_hp: i32) {
        let mut m = MobileData::new("a rotting zombie".to_string());
        m.vnum = vnum.to_string();
        m.is_prototype = true;
        m.level = level;
        m.max_hp = max_hp;
        m.current_hp = max_hp;
        m.short_desc = "a rotting zombie".to_string();
        db.save_mobile_data(m).expect("save proto");
    }

    fn place_corpse(db: &Db, room: Uuid, vnum: &str) {
        let corpse = CorpseBuilder::for_mobile("a rotting zombie", room, 0)
            .with_source_vnum(Some(vnum.to_string()))
            .build();
        db.save_item_data(corpse).expect("save corpse");
    }

    fn params() -> RaiseParams {
        RaiseParams {
            mana_cost: 70,
            morality_cost: 3,
            mastery_xp: 25,
        }
    }

    #[test]
    fn refuses_when_corpse_outranks_mastery() {
        let (db, _t) = open_temp();
        let room = Uuid::new_v4();
        save_proto(&db, "test:zombie", 5, 30);
        place_corpse(&db, room, "test:zombie");
        make_char(&db, "Necro", room, 100, 0, 10); // necromancy 0 < corpse level 5

        let out = raise_dead_from_corpse(&db, "Necro", "corpse", params());
        assert!(!out.success);
        assert!(out.minion_id.is_none());
        // Pre-cast refusal spends nothing.
        let ch = db.get_character_data("Necro").unwrap().unwrap();
        assert_eq!(ch.mana, 100);
        assert_eq!(ch.morality, 0);
    }

    #[test]
    fn refuses_player_corpse() {
        let (db, _t) = open_temp();
        let room = Uuid::new_v4();
        db.save_item_data(CorpseBuilder::for_player("Hero", room, 0).build())
            .unwrap();
        make_char(&db, "Necro", room, 100, 10, 10);

        let out = raise_dead_from_corpse(&db, "Necro", "corpse", params());
        assert!(!out.success);
        assert!(out.caster_msg.contains("fallen adventurer"));
    }

    #[test]
    fn refuses_when_minion_cap_reached() {
        let (db, _t) = open_temp();
        let room = Uuid::new_v4();
        save_proto(&db, "test:zombie", 0, 30);
        place_corpse(&db, room, "test:zombie");
        make_char(&db, "Necro", room, 100, 0, 10); // mastery 0 -> cap = 1

        // One existing charmed minion already bound to Necro.
        let mut existing = MobileData::new("an old skeleton".to_string());
        existing.is_prototype = false;
        existing.active_buffs.push(crate::ActiveBuff {
            effect_type: crate::EffectType::Charmed,
            magnitude: 0,
            remaining_secs: -1,
            source: "Necro".to_string(),
            damage_type: None,
            vs_effect: None,
            skill_key: None,
        });
        db.save_mobile_data(existing).unwrap();

        let out = raise_dead_from_corpse(&db, "Necro", "corpse", params());
        assert!(!out.success);
        assert!(out.caster_msg.contains("cannot bind another"));
    }

    #[test]
    fn success_binds_minion_and_applies_costs() {
        // Success is a ~95% roll (magic 20, corpse level 1 → clamps to 95).
        // Retry on a fresh world each attempt (success consumes the corpse);
        // the chance of 40 consecutive failures is ~1e-52.
        for _ in 0..40 {
            let (db, _t) = open_temp();
            let room = Uuid::new_v4();
            save_proto(&db, "test:zombie", 1, 50);
            place_corpse(&db, room, "test:zombie");
            make_char(&db, "Necro", room, 100, 5, 20);

            let out = raise_dead_from_corpse(&db, "Necro", "corpse", params());
            if !out.success {
                continue;
            }

            let mid = out.minion_id.expect("minion id on success");
            let pet = db.get_mobile_data(&mid).unwrap().unwrap();
            assert!(
                pet.active_buffs
                    .iter()
                    .any(|b| b.effect_type == crate::EffectType::Charmed && b.source == "Necro")
            );
            assert_eq!(pet.current_hp, 30, "weakened to 60% of 50");
            assert_eq!(pet.max_hp, 30);
            assert!(pet.short_desc.starts_with("the risen corpse of"));
            assert_eq!(pet.current_room_id, Some(room));

            let ch = db.get_character_data("Necro").unwrap().unwrap();
            assert_eq!(ch.mana, 30, "100 - 70 mana cost");
            assert_eq!(ch.morality, -3, "morality cost applied on success");
            assert_eq!(ch.skills.get("necromancy").unwrap().experience, 25);

            // Corpse consumed.
            assert!(
                db.get_items_in_room(&room)
                    .unwrap()
                    .iter()
                    .all(|it| !it.flags.is_corpse)
            );
            return;
        }
        panic!("raise never succeeded in 40 attempts at ~95% — RNG or logic broken");
    }
}

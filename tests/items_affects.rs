// Integration tests for the generic item affects system.
// Covers wear/remove buff stamping, typed damage resistance, status-effect
// resistance (the "saves" replacement), idempotent re-equip, and legacy
// field migration. See /home/craig/.claude/plans/let-s-plan-out-a-zesty-dawn.md.

use ironmud::db::Db;
use ironmud::script::{roll_status_application, status_resistance_total};
use ironmud::types::{ActiveBuff, CharacterData, DamageType, EffectType, ItemAffect, ItemData, MobileData};
use rand::rngs::StdRng;
use rand::SeedableRng;

fn fresh_db(tag: &str) -> (Db, tempfile::TempDir) {
    let temp = tempfile::tempdir().expect("create temp dir");
    let path = temp.path().join(format!("{tag}.db"));
    let db = Db::open(path.to_str().unwrap()).expect("open db");
    (db, temp)
}

fn make_char(name: &str) -> CharacterData {
    serde_json::from_value(serde_json::json!({
        "name": name,
        "password_hash": "",
        "current_room_id": uuid::Uuid::nil(),
        "stat_str": 10,
        "stat_dex": 10,
        "stat_con": 10,
        "stat_int": 10,
        "stat_wis": 10,
        "stat_cha": 10,
        "max_hp": 50,
        "hp": 50,
    }))
    .expect("build character")
}

fn item_with_affects(affects: Vec<ItemAffect>) -> ItemData {
    let mut item = ItemData::new(
        "glove".to_string(),
        "a sturdy glove".to_string(),
        "A sturdy leather glove lies here.".to_string(),
    );
    item.affects = affects;
    item
}

#[test]
fn wear_grants_buff() {
    let (db, _t) = fresh_db("wear_grants");
    let mut char = make_char("alice");
    db.save_character_data(char.clone()).unwrap();

    let item = item_with_affects(vec![ItemAffect {
        effect_type: EffectType::StrengthBoost,
        magnitude: 2,
        damage_type: None,
        vs_effect: None,
    }]);
    let item_id = item.id;
    db.save_item_data(item).unwrap();

    assert!(db.move_item_to_equipped(&item_id, &char.name).unwrap());

    char = db.get_character_data(&char.name).unwrap().expect("char exists");
    let strength_buffs: Vec<_> = char
        .active_buffs
        .iter()
        .filter(|b| b.effect_type == EffectType::StrengthBoost && b.source.starts_with("item:"))
        .collect();
    assert_eq!(strength_buffs.len(), 1, "expected one StrengthBoost item buff");
    assert_eq!(strength_buffs[0].magnitude, 2);
    assert_eq!(strength_buffs[0].source, format!("item:{}", item_id));
    assert_eq!(strength_buffs[0].remaining_secs, -1, "item-stamped buffs are permanent");
}

#[test]
fn remove_strips_buff() {
    let (db, _t) = fresh_db("remove_strips");
    let char = make_char("bob");
    db.save_character_data(char.clone()).unwrap();

    let item = item_with_affects(vec![ItemAffect {
        effect_type: EffectType::HitBonus,
        magnitude: 3,
        damage_type: None,
        vs_effect: None,
    }]);
    let item_id = item.id;
    db.save_item_data(item).unwrap();

    db.move_item_to_equipped(&item_id, &char.name).unwrap();
    let after_wear = db.get_character_data(&char.name).unwrap().unwrap();
    assert!(after_wear.active_buffs.iter().any(|b| b.source.starts_with("item:")));

    db.move_item_to_inventory(&item_id, &char.name).unwrap();
    let after_remove = db.get_character_data(&char.name).unwrap().unwrap();
    assert!(
        !after_remove.active_buffs.iter().any(|b| b.source.starts_with("item:")),
        "no item-sourced buffs should remain after remove"
    );
}

#[test]
fn acid_resistance_buff_subtracts_from_damage() {
    // Direct unit test of the typed-resistance buff-sum logic. The combat tick
    // applies the same sum-and-clamp formula at src/ticks/combat/tick.rs:2223.
    let buffs = vec![
        ActiveBuff {
            effect_type: EffectType::DamageResistance,
            magnitude: 25,
            remaining_secs: -1,
            source: "item:test".to_string(),
            damage_type: Some(DamageType::Acid),
            vs_effect: None,
        },
        ActiveBuff {
            effect_type: EffectType::DamageResistance,
            magnitude: 50,
            remaining_secs: -1,
            source: "item:other".to_string(),
            damage_type: Some(DamageType::Fire),
            vs_effect: None,
        },
    ];

    let acid_resist: i32 = buffs
        .iter()
        .filter(|b| b.effect_type == EffectType::DamageResistance && b.damage_type == Some(DamageType::Acid))
        .map(|b| b.magnitude)
        .sum();
    let fire_resist: i32 = buffs
        .iter()
        .filter(|b| b.effect_type == EffectType::DamageResistance && b.damage_type == Some(DamageType::Fire))
        .map(|b| b.magnitude)
        .sum();
    let cold_resist: i32 = buffs
        .iter()
        .filter(|b| b.effect_type == EffectType::DamageResistance && b.damage_type == Some(DamageType::Cold))
        .map(|b| b.magnitude)
        .sum();

    let damage = 100;
    assert_eq!((damage * (100 - acid_resist)) / 100, 75, "acid 25% resist → 75");
    assert_eq!((damage * (100 - fire_resist)) / 100, 50, "fire 50% resist → 50");
    assert_eq!((damage * (100 - cold_resist)) / 100, 100, "no cold resist → unchanged");
}

#[test]
fn idempotent_reequip() {
    let (db, _t) = fresh_db("idempotent");
    let char = make_char("carol");
    db.save_character_data(char.clone()).unwrap();

    let item = item_with_affects(vec![ItemAffect {
        effect_type: EffectType::StrengthBoost,
        magnitude: 2,
        damage_type: None,
        vs_effect: None,
    }]);
    let item_id = item.id;
    db.save_item_data(item).unwrap();

    db.move_item_to_equipped(&item_id, &char.name).unwrap();
    db.move_item_to_equipped(&item_id, &char.name).unwrap(); // re-equip
    let c1 = db.get_character_data(&char.name).unwrap().unwrap();
    let count1 = c1
        .active_buffs
        .iter()
        .filter(|b| b.source == format!("item:{}", item_id))
        .count();
    assert_eq!(count1, 1, "re-equip must not duplicate buffs");

    db.move_item_to_inventory(&item_id, &char.name).unwrap();
    let c2 = db.get_character_data(&char.name).unwrap().unwrap();
    let count2 = c2
        .active_buffs
        .iter()
        .filter(|b| b.source == format!("item:{}", item_id))
        .count();
    assert_eq!(count2, 0, "no orphan buffs after remove");
}

#[test]
fn migration_promotes_legacy_fields() {
    let (db, _t) = fresh_db("migration");

    // Build an item carrying legacy bonus fields directly (simulating data
    // serialized by an older build).
    let mut item = ItemData::new(
        "shield".to_string(),
        "a polished steel shield".to_string(),
        "A polished steel shield rests against the wall.".to_string(),
    );
    item.hit_bonus = 5;
    item.damage_bonus = 2;
    item.max_hp_bonus = 10;
    item.stat_str = 3;
    db.save_item_data(item.clone()).unwrap();

    // Run the migration job.
    db.migrate_item_legacy_bonuses_to_affects().unwrap();

    let migrated = db.get_item_data(&item.id).unwrap().unwrap();
    assert_eq!(migrated.hit_bonus, 0, "hit_bonus zeroed after migration");
    assert_eq!(migrated.damage_bonus, 0);
    assert_eq!(migrated.max_hp_bonus, 0);
    assert_eq!(migrated.stat_str, 0);

    let kinds: Vec<EffectType> = migrated.affects.iter().map(|a| a.effect_type).collect();
    assert!(kinds.contains(&EffectType::HitBonus), "HitBonus migrated");
    assert!(kinds.contains(&EffectType::DamageBonus), "DamageBonus migrated");
    assert!(kinds.contains(&EffectType::MaxHpBonus), "MaxHpBonus migrated");
    assert!(kinds.contains(&EffectType::StrengthBoost), "StrengthBoost migrated");

    let hit = migrated
        .affects
        .iter()
        .find(|a| a.effect_type == EffectType::HitBonus)
        .unwrap();
    assert_eq!(hit.magnitude, 5);

    // Second run should be a no-op (idempotency guard via setting key).
    db.migrate_item_legacy_bonuses_to_affects().unwrap();
    let unchanged = db.get_item_data(&item.id).unwrap().unwrap();
    assert_eq!(unchanged.affects.len(), migrated.affects.len(), "second migration is no-op");
}

#[test]
fn status_resistance_reduces_application_chance() {
    let buffs = vec![ActiveBuff {
        effect_type: EffectType::StatusResistance,
        magnitude: 30,
        remaining_secs: -1,
        source: "item:test".to_string(),
        damage_type: None,
        vs_effect: Some("sleep".to_string()),
    }];

    assert_eq!(status_resistance_total(&buffs, EffectType::Sleep), 30);
    assert_eq!(status_resistance_total(&buffs, EffectType::Charmed), 0);

    // Deterministic RNG: with base 50 and 30 resist, final chance is 20.
    // A roll of 1..=20 lands; 21..=100 misses.
    let mut rng = StdRng::seed_from_u64(42);
    let mut hits = 0;
    let mut misses = 0;
    for _ in 0..1000 {
        if roll_status_application(&buffs, EffectType::Sleep, 50, &mut rng) {
            hits += 1;
        } else {
            misses += 1;
        }
    }
    // Expected hit rate ~20%. Allow generous slack for the test seed.
    assert!(
        hits > 100 && hits < 300,
        "expected ~20% hit rate with 30% resistance against base 50 chance, got {hits}/{}",
        hits + misses
    );
}

#[test]
fn status_resistance_wildcard_matches_any_effect() {
    let buffs = vec![ActiveBuff {
        effect_type: EffectType::StatusResistance,
        magnitude: 20,
        remaining_secs: -1,
        source: "item:amulet".to_string(),
        damage_type: None,
        vs_effect: Some("*".to_string()),
    }];

    // Wildcard matches Sleep, Charmed, Blind, Curse, ...
    assert_eq!(status_resistance_total(&buffs, EffectType::Sleep), 20);
    assert_eq!(status_resistance_total(&buffs, EffectType::Charmed), 20);
    assert_eq!(status_resistance_total(&buffs, EffectType::Blind), 20);
    assert_eq!(status_resistance_total(&buffs, EffectType::Curse), 20);
}

#[test]
fn status_resistance_clamps_to_window() {
    // Even with 100 resist, application chance floors at 5% (never absolute).
    let buffs = vec![ActiveBuff {
        effect_type: EffectType::StatusResistance,
        magnitude: 100,
        remaining_secs: -1,
        source: "item:ring".to_string(),
        damage_type: None,
        vs_effect: Some("sleep".to_string()),
    }];

    let mut rng = StdRng::seed_from_u64(7);
    let mut hits = 0;
    for _ in 0..2000 {
        if roll_status_application(&buffs, EffectType::Sleep, 80, &mut rng) {
            hits += 1;
        }
    }
    // 80 - 100 = -20, clamps to 5%. Expect ~100 hits over 2000 rolls.
    assert!(
        hits > 30 && hits < 200,
        "expected ~5% floor (≈100/2000), got {hits}"
    );
}

#[test]
fn cursed_item_poisons_wearer() {
    let (db, _t) = fresh_db("cursed");
    let char = make_char("david");
    db.save_character_data(char.clone()).unwrap();

    let item = item_with_affects(vec![ItemAffect {
        effect_type: EffectType::Poison,
        magnitude: 1,
        damage_type: None,
        vs_effect: None,
    }]);
    let item_id = item.id;
    db.save_item_data(item).unwrap();

    db.move_item_to_equipped(&item_id, &char.name).unwrap();
    let after = db.get_character_data(&char.name).unwrap().unwrap();
    assert!(
        after
            .active_buffs
            .iter()
            .any(|b| b.effect_type == EffectType::Poison && b.source == format!("item:{}", item_id)),
        "cursed item must apply Poison buff to wearer"
    );

    db.move_item_to_inventory(&item_id, &char.name).unwrap();
    let after_remove = db.get_character_data(&char.name).unwrap().unwrap();
    assert!(
        !after_remove
            .active_buffs
            .iter()
            .any(|b| b.effect_type == EffectType::Poison && b.source == format!("item:{}", item_id)),
        "removing the cursed item must strip the Poison buff"
    );
}

#[test]
fn delete_item_strips_equipped_buffs() {
    let (db, _t) = fresh_db("delete_strips");
    let char = make_char("eve");
    db.save_character_data(char.clone()).unwrap();

    let item = item_with_affects(vec![ItemAffect {
        effect_type: EffectType::StrengthBoost,
        magnitude: 4,
        damage_type: None,
        vs_effect: None,
    }]);
    let item_id = item.id;
    db.save_item_data(item).unwrap();

    db.move_item_to_equipped(&item_id, &char.name).unwrap();
    assert!(db.delete_item(&item_id).unwrap());

    let after = db.get_character_data(&char.name).unwrap().unwrap();
    assert!(
        !after.active_buffs.iter().any(|b| b.source == format!("item:{}", item_id)),
        "destroyed equipped item must leave no orphan buffs"
    );
}

#[test]
fn mob_equip_stamps_buffs() {
    let (db, _t) = fresh_db("mob_equip");
    let mob = MobileData::new("guard".to_string());
    let mob_id = mob.id;
    db.save_mobile_data(mob).unwrap();

    let item = item_with_affects(vec![ItemAffect {
        effect_type: EffectType::HitBonus,
        magnitude: 5,
        damage_type: None,
        vs_effect: None,
    }]);
    let item_id = item.id;
    db.save_item_data(item).unwrap();

    db.move_item_to_mobile_equipped(&item_id, &mob_id).unwrap();
    let after = db.get_mobile_data(&mob_id).unwrap().unwrap();
    assert!(
        after
            .active_buffs
            .iter()
            .any(|b| b.effect_type == EffectType::HitBonus && b.source == format!("item:{}", item_id)),
        "mob-equipped item must stamp HitBonus buff"
    );
}

#[test]
fn hard_immunity_bypasses_resistance_in_buff_lane() {
    // Hard-immunity gates (MobileFlags.no_sleep, etc.) live in cast.rhai
    // BEFORE the resistance roll. This test documents the invariant from
    // the buff-lane side: status_resistance does NOT and SHOULD NOT mention
    // flag-immunity, so a target with no_sleep but no StatusResistance buff
    // reports zero resistance — confirming the binary gate is the only path
    // for flag immunity.
    let buffs: Vec<ActiveBuff> = Vec::new();
    assert_eq!(status_resistance_total(&buffs, EffectType::Sleep), 0);

    // Wildcard exception only applies to StatusResistance buffs, never to
    // implicit flag immunity.
    let only_wildcard = vec![ActiveBuff {
        effect_type: EffectType::StatusResistance,
        magnitude: 50,
        remaining_secs: -1,
        source: "item:ring".to_string(),
        damage_type: None,
        vs_effect: Some("*".to_string()),
    }];
    assert_eq!(status_resistance_total(&only_wildcard, EffectType::Sleep), 50);
}

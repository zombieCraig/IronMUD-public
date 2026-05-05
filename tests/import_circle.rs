//! End-to-end test for the CircleMUD importer:
//! parse → map → apply → assert via Db queries.

use std::path::PathBuf;

use ironmud::db::Db;
use ironmud::import::{MappingOptions, MudEngine, Severity, engines::circle::CircleEngine, mapping, writer};
use ironmud::types::DoorState;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/circle")
}

#[test]
fn parses_fixture_into_plan() {
    let (ir, parse_warnings) = CircleEngine.parse(&fixture_root()).expect("parse");
    assert_eq!(parse_warnings.len(), 0, "fixture should parse cleanly");
    assert_eq!(ir.zones.len(), 1, "exactly one zone in fixture");
    let zone = &ir.zones[0];
    assert_eq!(zone.vnum, 90);
    assert_eq!(zone.rooms.len(), 3);

    let opts = MappingOptions {
        circle: mapping::CircleMappingTable::load_default(),
        existing_area_prefixes: Vec::new(),
        existing_room_vnums: Vec::new(),
        existing_mobile_vnums: Vec::new(),
        existing_item_vnums: Vec::new(),
    };
    let (plan, warnings) = mapping::ir_to_plan(&ir, &opts);
    assert_eq!(plan.areas.len(), 1);
    assert_eq!(plan.rooms.len(), 3);
    // 4 exits in the fixture (9001 N→9002, 9001 E→9003, 9002 S→9001, 9003 W→9001)
    assert_eq!(plan.exits.len(), 4);

    let area = &plan.areas[0];
    assert_eq!(area.prefix, "test_fixture_village");

    let square = plan.rooms.iter().find(|r| r.source_vnum == 9001).expect("test square");
    assert_eq!(square.vnum, "test_fixture_village_9001");
    assert!(square.flags.city, "CITY sector should set city flag");
    assert_eq!(square.extra_descs.len(), 1, "fixture has one extra desc on the square");

    // The east door on 9001 should be ISDOOR + CLOSED (3) — keyed by vnum 9100.
    let east_door = square.doors.iter().find(|d| d.direction == "east").expect("east door");
    assert!(east_door.is_closed);
    assert!(!east_door.is_locked);
    assert_eq!(east_door.key_source_vnum, Some(9100));
    assert_eq!(east_door.name, "gate");
    assert_eq!(east_door.keywords, vec!["wooden".to_string()]);

    // Courtyard 9003 has flag 'a' (DARK) + 'd' (INDOORS), sector INSIDE.
    let courtyard = plan.rooms.iter().find(|r| r.source_vnum == 9003).expect("courtyard");
    assert!(courtyard.flags.dark);
    assert!(courtyard.flags.indoors);

    // The zone's M reset now translates to a spawn point; remaining
    // DeferredFeature warnings come from item-side gaps (extra-descs on
    // objects, etc.).
    let deferred = warnings
        .iter()
        .filter(|w| matches!(w.kind, ironmud::import::WarningKind::DeferredFeature))
        .count();
    assert!(deferred >= 1, "expected at least one deferred-feature warning");
    // The fixture's lone M reset should produce exactly one mob spawn point.
    assert_eq!(plan.spawns.len(), 1, "M reset translated to spawn point");
    assert_eq!(plan.spawns[0].vnum, "test_fixture_village_9001");
    assert_eq!(plan.spawns[0].room_vnum, "test_fixture_village_9001");
    assert_eq!(plan.spawns[0].respawn_interval_secs, 30 * 60);
    // Fixture uses no PEACEFUL/HOUSE/etc. — no Block warnings expected.
    let blocks = warnings.iter().filter(|w| w.severity == Severity::Block).count();
    assert_eq!(blocks, 0);
}

#[test]
fn applies_fixture_to_tmp_db() {
    let dir = tmpdir("ironmud-import-test");
    let db_path = dir.join("ironmud.db");
    {
        let db = Db::open(&db_path).expect("open tmp db");
        let (ir, _) = CircleEngine.parse(&fixture_root()).expect("parse");
        let opts = MappingOptions {
            circle: mapping::CircleMappingTable::load_default(),
            existing_area_prefixes: db
                .list_all_areas()
                .unwrap()
                .into_iter()
                .map(|a| a.prefix.to_lowercase())
                .collect(),
            existing_room_vnums: db
                .list_all_rooms()
                .unwrap()
                .into_iter()
                .filter_map(|r| r.vnum)
                .collect(),
            existing_mobile_vnums: Vec::new(),
            existing_item_vnums: Vec::new(),
        };
        let (plan, warnings) = mapping::ir_to_plan(&ir, &opts);
        let summary = writer::apply(&db, &plan, &warnings).expect("apply");
        assert_eq!(summary.written_areas, 1);
        assert_eq!(summary.written_rooms, 3);
        assert_eq!(summary.linked_exits, 4);
        assert_eq!(summary.dropped_exits, 0);

        // Re-query the DB to confirm rooms are persisted with their imported
        // vnums and exits.
        let square = db
            .get_room_by_vnum("test_fixture_village_9001")
            .unwrap()
            .expect("square saved");
        assert_eq!(square.title, "The Test Square");
        assert!(square.flags.city);

        // East exit on the square should resolve to the courtyard's UUID.
        let courtyard = db
            .get_room_by_vnum("test_fixture_village_9003")
            .unwrap()
            .expect("courtyard saved");
        assert_eq!(square.exits.east, Some(courtyard.id));

        // Door state survived round-trip.
        let east_door: &DoorState = square.doors.get("east").expect("east door persisted");
        assert!(east_door.is_closed);
        assert_eq!(east_door.name, "gate");
    }
    // Drop the Db (Sled holds a file lock); cleanup directory.
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn parses_mobiles_into_plan() {
    let (ir, parse_warnings) = CircleEngine.parse(&fixture_root()).expect("parse");
    assert_eq!(parse_warnings.len(), 0);
    let zone = &ir.zones[0];
    assert_eq!(zone.mobiles.len(), 3, "three fixture mobs");

    let opts = MappingOptions {
        circle: mapping::CircleMappingTable::load_default(),
        existing_area_prefixes: Vec::new(),
        existing_room_vnums: Vec::new(),
        existing_mobile_vnums: Vec::new(),
        existing_item_vnums: Vec::new(),
    };
    let (plan, warnings) = mapping::ir_to_plan(&ir, &opts);
    assert_eq!(plan.mobiles.len(), 3);

    let wanderer = plan.mobiles.iter().find(|m| m.source_vnum == 9001).expect("wanderer");
    assert_eq!(wanderer.vnum, "test_fixture_village_9001");
    assert_eq!(wanderer.name, "the test wanderer");
    assert!(wanderer.short_desc.starts_with("A test wanderer"));
    assert!(!wanderer.short_desc.ends_with('\n'));
    assert!(wanderer.long_desc.contains("simulated NPC"));
    assert_eq!(wanderer.keywords, vec!["test".to_string(), "wanderer".to_string()]);
    // befgl = SENTINEL (1) + AWARE (4) + AGGRESSIVE (5) + STAY_ZONE (6) + MEMORY (11)
    assert!(wanderer.flags.sentinel);
    assert!(wanderer.flags.aware);
    assert!(wanderer.flags.aggressive);
    assert!(wanderer.flags.stay_zone);
    assert!(wanderer.flags.memory);
    // 2d6+10 → max 22 hp.
    assert_eq!(wanderer.max_hp, 22);
    assert_eq!(wanderer.damage_dice, "1d6+1");
    assert_eq!(wanderer.level, 3);
    assert_eq!(wanderer.gold, 50);

    let beast = plan.mobiles.iter().find(|m| m.source_vnum == 9002).expect("beast");
    // 1d8+5 → max 13 hp.
    assert_eq!(beast.max_hp, 13);
    assert!(!beast.flags.sentinel);
    assert!(!beast.flags.aggressive);

    // The wisp's AFF_SANCTUARY should stamp a permanent DamageReduction buff
    // on the planned prototype (no warn).
    let wisp = plan.mobiles.iter().find(|m| m.source_vnum == 9003).expect("wisp");
    let sanctuary_buff = wisp
        .active_buffs
        .iter()
        .find(|b| b.effect_type == ironmud::types::EffectType::DamageReduction)
        .expect("sanctuary stamped a DamageReduction buff");
    assert_eq!(sanctuary_buff.magnitude, 50);
    assert_eq!(sanctuary_buff.remaining_secs, -1);
    assert!(
        !warnings.iter().any(|w| w.message.contains("AFF_SANCTUARY")),
        "AFF_SANCTUARY should no longer warn",
    );

    // The wisp also has AFF_BLIND (a), INVISIBLE (b), DETECT_INVIS (d),
    // DETECT_MAGIC (e), CURSE (j), INFRAVISION (k), SLEEP (o) — each one
    // should stamp the matching permanent buff on the prototype.
    for et in [
        ironmud::types::EffectType::Invisibility,
        ironmud::types::EffectType::DetectInvisible,
        ironmud::types::EffectType::DetectMagic,
        ironmud::types::EffectType::NightVision,
        ironmud::types::EffectType::Blind,
        ironmud::types::EffectType::Sleep,
        ironmud::types::EffectType::Curse,
    ] {
        let buff = wisp
            .active_buffs
            .iter()
            .find(|b| b.effect_type == et)
            .unwrap_or_else(|| panic!("expected {:?} buff stamped on wisp", et));
        assert_eq!(buff.remaining_secs, -1);
    }
    let blind_buff = wisp
        .active_buffs
        .iter()
        .find(|b| b.effect_type == ironmud::types::EffectType::Blind)
        .unwrap();
    assert_eq!(blind_buff.magnitude, 50);
    let curse_buff = wisp
        .active_buffs
        .iter()
        .find(|b| b.effect_type == ironmud::types::EffectType::Curse)
        .unwrap();
    assert_eq!(curse_buff.magnitude, 10);
    assert!(
        !warnings.iter().any(|w| w.message.contains("AFF_INVISIBLE")
            || w.message.contains("AFF_DETECT_INVIS")
            || w.message.contains("AFF_DETECT_MAGIC")
            || w.message.contains("AFF_INFRAVISION")
            || w.message.contains("AFF_BLIND")
            || w.message.contains("AFF_SLEEP")
            || w.message.contains("AFF_CURSE")),
        "permanent AFF buffs should no longer warn",
    );

    // The wisp's MOB flags nopqr cover NOCHARM (13), NOSUMMON (14),
    // NOSLEEP (15), NOBASH (16), NOBLIND (17). Each should land on the
    // matching IronMUD MobileFlags bool, no warns.
    assert!(wisp.flags.no_charm, "NOCHARM should map to flags.no_charm");
    assert!(wisp.flags.no_summon, "NOSUMMON should map to flags.no_summon");
    assert!(wisp.flags.no_sleep, "NOSLEEP should map to flags.no_sleep");
    assert!(wisp.flags.no_bash, "NOBASH should map to flags.no_bash");
    assert!(wisp.flags.no_blind, "NOBLIND should map to flags.no_blind");
    assert!(
        !warnings.iter().any(|w| w.message.contains("MOB_NOCHARM")
            || w.message.contains("MOB_NOSUMMON")
            || w.message.contains("MOB_NOSLEEP")
            || w.message.contains("MOB_NOBASH")
            || w.message.contains("MOB_NOBLIND")),
        "MOB_NOCHARM/NOSUMMON/NOSLEEP/NOBASH/NOBLIND should no longer warn",
    );

    // BareHandAttack should warn exactly once across the import (the beast
    // is the only carrier in this fixture, but we still exercise the
    // dedup path).
    let bare_warns = warnings
        .iter()
        .filter(|w| w.message.contains("BareHandAttack"))
        .count();
    assert_eq!(bare_warns, 1);

    // No Block warnings expected (no vnum collisions in a fresh tmp DB).
    let blocks = warnings.iter().filter(|w| w.severity == Severity::Block).count();
    assert_eq!(blocks, 0);
}

#[test]
fn applies_mobiles_to_tmp_db() {
    let dir = tmpdir("ironmud-import-mob-test");
    let db_path = dir.join("ironmud.db");
    {
        let db = Db::open(&db_path).expect("open tmp db");
        let (ir, _) = CircleEngine.parse(&fixture_root()).expect("parse");
        let opts = MappingOptions {
            circle: mapping::CircleMappingTable::load_default(),
            existing_area_prefixes: Vec::new(),
            existing_room_vnums: Vec::new(),
            existing_mobile_vnums: Vec::new(),
            existing_item_vnums: Vec::new(),
        };
        let (plan, warnings) = mapping::ir_to_plan(&ir, &opts);
        let summary = writer::apply(&db, &plan, &warnings).expect("apply");
        assert_eq!(summary.written_mobiles, 3);
        assert_eq!(summary.planned_mobiles, 3);

        let wanderer = db
            .get_mobile_by_vnum("test_fixture_village_9001")
            .unwrap()
            .expect("wanderer saved");
        assert!(wanderer.is_prototype);
        assert_eq!(wanderer.name, "the test wanderer");
        assert!(wanderer.flags.sentinel);
        assert!(wanderer.flags.aggressive);
        assert_eq!(wanderer.max_hp, 22);
        assert_eq!(wanderer.current_hp, 22);
        assert_eq!(wanderer.level, 3);
        assert_eq!(wanderer.damage_dice, "1d6+1");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn parses_items_into_plan() {
    use ironmud::types::{DamageType, ItemType, LiquidType, WearLocation};

    let (ir, parse_warnings) = CircleEngine.parse(&fixture_root()).expect("parse");
    assert_eq!(parse_warnings.len(), 0, "fixture should parse cleanly");
    let zone = &ir.zones[0];
    assert_eq!(zone.items.len(), 8, "eight fixture items in obj/9000.obj");

    let opts = MappingOptions {
        circle: mapping::CircleMappingTable::load_default(),
        existing_area_prefixes: Vec::new(),
        existing_room_vnums: Vec::new(),
        existing_mobile_vnums: Vec::new(),
        existing_item_vnums: Vec::new(),
    };
    let (plan, warnings) = mapping::ir_to_plan(&ir, &opts);
    assert_eq!(plan.items.len(), 8);

    // Sword: weapon, 1d8 slashing, GLOW (extra-bit `a` = bit 0), wearable
    // wielded; APPLY_DAMROLL +2 → warn (no item-level damroll yet).
    let sword = plan.items.iter().find(|i| i.source_vnum == 9010).expect("sword");
    assert_eq!(sword.vnum, "test_fixture_village_9010");
    assert!(sword.data.is_prototype);
    assert_eq!(sword.data.item_type, ItemType::Weapon);
    assert_eq!(sword.data.damage_dice_count, 1);
    assert_eq!(sword.data.damage_dice_sides, 8);
    assert_eq!(sword.data.damage_type, DamageType::Slashing);
    assert!(sword.data.flags.glow);
    assert!(sword.data.flags.magical);
    assert!(sword.data.wear_locations.contains(&WearLocation::Wielded));
    assert_eq!(sword.data.value, 600);
    let damroll_warn = warnings
        .iter()
        .find(|w| w.message.contains("APPLY_DAMROLL"))
        .expect("APPLY_DAMROLL surfaced");
    assert_eq!(damroll_warn.severity, Severity::Warn);

    // Armor: AC value v0=3 → armor_class +3 (no apply blocks bumping it
    // further), then APPLY_AC -3 sign-flips to +3 (added). APPLY_STR +1 →
    // stat_str +1.
    let plate = plan.items.iter().find(|i| i.source_vnum == 9011).expect("plate");
    assert_eq!(plate.data.item_type, ItemType::Armor);
    // v0=3 negated to -3 (no, wait — armor v0=3 directly maps as armor_class
    // = -v0 = -3. Then the A-block APPLY_AC -3 with sign-flip adds +3, net 0.
    assert_eq!(plate.data.armor_class, Some(0));
    assert!(plate.data.wear_locations.contains(&WearLocation::Torso));
    assert_eq!(plate.data.stat_str, 1);
    assert!(plate.data.flags.glow);

    // Container: locked (bits 4|8 = 12), key vnum rewritten to prefixed form.
    let chest = plan.items.iter().find(|i| i.source_vnum == 9012).expect("chest");
    assert_eq!(chest.data.item_type, ItemType::Container);
    assert_eq!(chest.data.container_max_weight, 50);
    assert!(chest.data.container_closed);
    assert!(chest.data.container_locked);
    assert_eq!(
        chest.data.container_key_vnum.as_deref(),
        Some("test_fixture_village_9013")
    );

    // Key: clean ItemType::Key.
    let key = plan.items.iter().find(|i| i.source_vnum == 9013).expect("key");
    assert_eq!(key.data.item_type, ItemType::Key);

    // Food: nutrition=24, poisoned=true, has an E-block which should produce
    // a single DeferredFeature warning (no extra_descs target on items).
    let food = plan.items.iter().find(|i| i.source_vnum == 9014).expect("food");
    assert_eq!(food.data.item_type, ItemType::Food);
    assert_eq!(food.data.food_nutrition, 24);
    assert!(food.data.food_poisoned);
    let extra_warn = warnings
        .iter()
        .find(|w| {
            matches!(w.kind, ironmud::import::WarningKind::DeferredFeature)
                && w.message.contains("extra description")
        })
        .expect("E-block warning surfaced");
    assert_eq!(extra_warn.severity, Severity::Warn);

    // Liquid container: ale, 50/50, not poisoned.
    let barrel = plan.items.iter().find(|i| i.source_vnum == 9015).expect("barrel");
    assert_eq!(barrel.data.item_type, ItemType::LiquidContainer);
    assert_eq!(barrel.data.liquid_max, 50);
    assert_eq!(barrel.data.liquid_current, 50);
    assert_eq!(barrel.data.liquid_type, LiquidType::Ale);
    assert!(!barrel.data.liquid_poisoned);

    // Light: provides_light + capacity-hours warning.
    let torch = plan.items.iter().find(|i| i.source_vnum == 9016).expect("torch");
    assert_eq!(torch.data.item_type, ItemType::Misc);
    assert!(torch.data.flags.provides_light);
    let light_warn = warnings
        .iter()
        .find(|w| w.message.contains("ITEM_LIGHT capacity"))
        .expect("ITEM_LIGHT capacity warn surfaced");
    assert_eq!(light_warn.severity, Severity::Warn);

    // Cursed amulet: NODROP set, ANTI_GOOD warns.
    let amulet = plan.items.iter().find(|i| i.source_vnum == 9017).expect("amulet");
    assert!(amulet.data.flags.no_drop);
    assert!(amulet.data.wear_locations.contains(&WearLocation::Neck));
    let anti_warn = warnings
        .iter()
        .find(|w| w.message.contains("ITEM_ANTI_GOOD"))
        .expect("ANTI_GOOD warn surfaced");
    assert_eq!(anti_warn.severity, Severity::Warn);

    // No Block warnings on a fresh fixture.
    let blocks = warnings.iter().filter(|w| w.severity == Severity::Block).count();
    assert_eq!(blocks, 0);
}

#[test]
fn applies_items_to_tmp_db() {
    use ironmud::types::ItemType;

    let dir = tmpdir("ironmud-import-obj-test");
    let db_path = dir.join("ironmud.db");
    {
        let db = Db::open(&db_path).expect("open tmp db");
        let (ir, _) = CircleEngine.parse(&fixture_root()).expect("parse");
        let opts = MappingOptions {
            circle: mapping::CircleMappingTable::load_default(),
            existing_area_prefixes: Vec::new(),
            existing_room_vnums: Vec::new(),
            existing_mobile_vnums: Vec::new(),
            existing_item_vnums: Vec::new(),
        };
        let (plan, warnings) = mapping::ir_to_plan(&ir, &opts);
        let summary = writer::apply(&db, &plan, &warnings).expect("apply");
        assert_eq!(summary.written_items, 8);
        assert_eq!(summary.planned_items, 8);

        let sword = db
            .get_item_by_vnum("test_fixture_village_9010")
            .unwrap()
            .expect("sword saved");
        assert!(sword.is_prototype);
        assert_eq!(sword.item_type, ItemType::Weapon);
        assert_eq!(sword.damage_dice_count, 1);
        assert_eq!(sword.damage_dice_sides, 8);
        assert!(sword.flags.glow);

        // Container survived round-trip with key vnum and locked state.
        let chest = db
            .get_item_by_vnum("test_fixture_village_9012")
            .unwrap()
            .expect("chest saved");
        assert!(chest.container_locked);
        assert_eq!(
            chest.container_key_vnum.as_deref(),
            Some("test_fixture_village_9013")
        );
        // Sibling key prototype exists at the rewritten vnum.
        let key = db
            .get_item_by_vnum("test_fixture_village_9013")
            .unwrap()
            .expect("key saved");
        assert_eq!(key.item_type, ItemType::Key);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn parses_shops_into_plan() {
    let (ir, parse_warnings) = CircleEngine.parse(&fixture_root()).expect("parse");
    assert_eq!(parse_warnings.len(), 0, "fixture should parse cleanly");
    let zone = &ir.zones[0];
    assert_eq!(zone.shops.len(), 2, "two fixture shops");

    let opts = MappingOptions {
        circle: mapping::CircleMappingTable::load_default(),
        existing_area_prefixes: Vec::new(),
        existing_room_vnums: Vec::new(),
        existing_mobile_vnums: Vec::new(),
        existing_item_vnums: Vec::new(),
    };
    let (plan, warnings) = mapping::ir_to_plan(&ir, &opts);
    assert_eq!(plan.shop_overlays.len(), 2);

    // Shop #9001 attaches to mob #9001 (the wanderer). Profit_buy 2.0 →
    // sell_rate 200; profit_sell 0.4 → buy_rate 40. Buy types map to
    // weapon + armor.
    let s1 = plan
        .shop_overlays
        .iter()
        .find(|o| o.shop_source_vnum == 9001)
        .expect("shop 9001");
    assert_eq!(s1.keeper_vnum, "test_fixture_village_9001");
    assert_eq!(s1.sell_rate, 200);
    assert_eq!(s1.buy_rate, 40);
    assert_eq!(
        s1.buys_types,
        vec!["weapon".to_string(), "armor".to_string()]
    );
    // Producing list is rewritten to prefixed item vnums.
    assert_eq!(
        s1.stock_vnums,
        vec![
            "test_fixture_village_9010".to_string(),
            "test_fixture_village_9011".to_string(),
        ]
    );

    // Shop #9002 attaches to mob #9002 (the beast). Multi-room + dual
    // shift + custom messages + non-zero temper/bitvector/with_who exercises
    // every advisory-warning path.
    let s2 = plan
        .shop_overlays
        .iter()
        .find(|o| o.shop_source_vnum == 9002)
        .expect("shop 9002");
    assert_eq!(s2.keeper_vnum, "test_fixture_village_9002");
    assert!(s2.stock_vnums.is_empty());
    assert_eq!(s2.buys_types, vec!["food".to_string()]);

    // Expected per-shop warnings.
    let shop_warns: Vec<&ironmud::import::Warning> = warnings
        .iter()
        .filter(|w| w.source.file.to_string_lossy().contains("9000.shp"))
        .collect();
    let multi_room = shop_warns.iter().any(|w| w.message.contains("operates in 2 rooms"));
    assert!(multi_room, "multi-room warn surfaced");
    let dual_shift = shop_warns.iter().any(|w| w.message.contains("hours (open1=8"));
    assert!(dual_shift, "dual-shift hours warn surfaced");
    let messages = shop_warns
        .iter()
        .filter(|w| w.message.contains("custom message string"))
        .count();
    assert_eq!(messages, 2, "every shop with non-empty messages warns once");
    let temper = shop_warns.iter().any(|w| w.message.contains("temper=2"));
    assert!(temper, "temper warn surfaced");
    let bitvec = shop_warns.iter().any(|w| w.message.contains("bitvector=3"));
    assert!(bitvec, "bitvector warn surfaced");
    let with_who = shop_warns.iter().any(|w| w.message.contains("with_who=8"));
    assert!(with_who, "with_who warn surfaced");

    // No Block warnings expected on a fresh fixture.
    let blocks = warnings.iter().filter(|w| w.severity == Severity::Block).count();
    assert_eq!(blocks, 0);
}

#[test]
fn applies_shops_to_tmp_db() {
    let dir = tmpdir("ironmud-import-shp-test");
    let db_path = dir.join("ironmud.db");
    {
        let db = Db::open(&db_path).expect("open tmp db");
        let (ir, _) = CircleEngine.parse(&fixture_root()).expect("parse");
        let opts = MappingOptions {
            circle: mapping::CircleMappingTable::load_default(),
            existing_area_prefixes: Vec::new(),
            existing_room_vnums: Vec::new(),
            existing_mobile_vnums: Vec::new(),
            existing_item_vnums: Vec::new(),
        };
        let (plan, warnings) = mapping::ir_to_plan(&ir, &opts);
        let summary = writer::apply(&db, &plan, &warnings).expect("apply");
        assert_eq!(summary.planned_shop_overlays, 2);
        assert_eq!(summary.overlaid_shops, 2);

        // Shop fields landed on the keeper mobile prototype.
        let wanderer = db
            .get_mobile_by_vnum("test_fixture_village_9001")
            .unwrap()
            .expect("wanderer saved");
        assert!(wanderer.flags.shopkeeper, "shopkeeper flag set defensively");
        assert_eq!(wanderer.shop_buy_rate, 40);
        assert_eq!(wanderer.shop_sell_rate, 200);
        assert_eq!(
            wanderer.shop_buys_types,
            vec!["weapon".to_string(), "armor".to_string()]
        );
        assert_eq!(
            wanderer.shop_stock,
            vec![
                "test_fixture_village_9010".to_string(),
                "test_fixture_village_9011".to_string(),
            ]
        );

        // The beast shop applied too, with empty stock and food-only buy filter.
        let beast = db
            .get_mobile_by_vnum("test_fixture_village_9002")
            .unwrap()
            .expect("beast saved");
        assert!(beast.flags.shopkeeper);
        assert!(beast.shop_stock.is_empty());
        assert_eq!(beast.shop_buys_types, vec!["food".to_string()]);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn translates_zon_resets() {
    use ironmud::types::{SpawnDestination, SpawnEntityType, WearLocation};

    let dir = tmpdir("ironmud-import-resets");
    let root = write_reset_fixture(&dir);

    let (ir, parse_warnings) = CircleEngine.parse(&root).expect("parse");
    // Synthetic root has no `src/` directory, so the engine emits a single
    // Info noting spec_assign.c was skipped — that's expected.
    let non_info: Vec<_> = parse_warnings
        .iter()
        .filter(|w| w.severity != Severity::Info)
        .collect();
    assert!(
        non_info.is_empty(),
        "fixture should parse without warn/block warnings; got: {non_info:?}"
    );
    let zone = &ir.zones[0];
    // Lifespan 10 → respawn cadence 600s
    assert_eq!(zone.default_respawn_secs, Some(600));
    // 17 reset commands in the .zon (M+G+E+M+E+O+P+O+P+O+P+M+G+D+D+D+R)
    assert_eq!(zone.resets.len(), 17, "all resets parsed structurally");

    let opts = MappingOptions {
        circle: mapping::CircleMappingTable::load_default(),
        existing_area_prefixes: Vec::new(),
        existing_room_vnums: Vec::new(),
        existing_mobile_vnums: Vec::new(),
        existing_item_vnums: Vec::new(),
    };
    let (plan, warnings) = mapping::ir_to_plan(&ir, &opts);

    // Spawn count: 3 mobs (M ×3) + 3 objs (O ×3) = 6 spawns.
    assert_eq!(plan.spawns.len(), 6, "one spawn per M and O reset");

    // Cadence inherited from the zone.
    for sp in &plan.spawns {
        assert_eq!(sp.respawn_interval_secs, 600);
    }

    // First M spawn — wanderer in room 8001 — should carry a wielded sword
    // (E slot 16) and an inventory sword (G).
    let first_mob = plan
        .spawns
        .iter()
        .find(|s| matches!(s.entity_type, SpawnEntityType::Mobile) && s.vnum.ends_with("_8001"))
        .expect("first wanderer spawn");
    assert_eq!(first_mob.room_vnum, "reset_test_zone_8001");
    let inv_dep = first_mob
        .dependencies
        .iter()
        .find(|d| matches!(d.destination, SpawnDestination::Inventory))
        .expect("G dependency");
    assert_eq!(inv_dep.item_vnum, "reset_test_zone_8050");
    let wield_dep = first_mob
        .dependencies
        .iter()
        .find(|d| matches!(d.destination, SpawnDestination::Equipped(WearLocation::Wielded)))
        .expect("E wielded dependency");
    assert_eq!(wield_dep.item_vnum, "reset_test_zone_8051");

    // The beast spawn (vnum 8002) followed by E slot 0 (LIGHT). Spawn
    // exists, but the E is dropped with an UnsupportedValueSemantic warn.
    let beast = plan
        .spawns
        .iter()
        .find(|s| matches!(s.entity_type, SpawnEntityType::Mobile) && s.vnum.ends_with("_8002"))
        .expect("beast spawn");
    assert!(
        beast.dependencies.is_empty(),
        "WEAR_LIGHT slot 0 should not yield an Equipped dep"
    );
    let slot_zero_warn = warnings
        .iter()
        .find(|w| {
            matches!(w.kind, ironmud::import::WarningKind::UnsupportedValueSemantic)
                && w.message.contains("wear-slot 0")
        })
        .expect("slot 0 unsupported warn");
    assert_eq!(slot_zero_warn.severity, Severity::Warn);

    // The first chest (8060) must carry a Container dep for the sword 8050.
    let chest_with_dep = plan
        .spawns
        .iter()
        .find(|s| {
            matches!(s.entity_type, SpawnEntityType::Item)
                && s.vnum.ends_with("_8060")
                && !s.dependencies.is_empty()
        })
        .expect("first chest with container dep");
    assert_eq!(chest_with_dep.dependencies.len(), 1);
    assert!(matches!(
        chest_with_dep.dependencies[0].destination,
        SpawnDestination::Container
    ));
    assert_eq!(chest_with_dep.dependencies[0].item_vnum, "reset_test_zone_8050");

    // P targeting a non-Container (orb 8061) → warn + drop. We expect
    // exactly one such warn.
    let p_no_container = warnings
        .iter()
        .filter(|w| {
            matches!(w.kind, ironmud::import::WarningKind::DeferredFeature)
                && w.message.contains("no preceding O of a Container")
        })
        .count();
    assert_eq!(p_no_container, 1);

    // P container_vnum mismatch (target 8099 ≠ prior chest 8060) → warn.
    let p_mismatch = warnings
        .iter()
        .filter(|w| {
            matches!(w.kind, ironmud::import::WarningKind::DeferredFeature)
                && w.message.contains("doesn't match prior O")
        })
        .count();
    assert_eq!(p_mismatch, 1);

    // G with if=0 (no anchor) → warn.
    let g_if_zero = warnings
        .iter()
        .filter(|w| {
            matches!(w.kind, ironmud::import::WarningKind::DeferredFeature)
                && w.message.contains("G reset with if=0")
        })
        .count();
    assert_eq!(g_if_zero, 1);

    // R reset → warn.
    let r_warn = warnings
        .iter()
        .filter(|w| {
            matches!(w.kind, ironmud::import::WarningKind::DeferredFeature) && w.message.starts_with("R reset")
        })
        .count();
    assert_eq!(r_warn, 1);

    // D resets: east door (state 1) closed, south door (state 2) closed+locked.
    let room = plan
        .rooms
        .iter()
        .find(|r| r.source_vnum == 8001)
        .expect("test room planned");
    let east = room.doors.iter().find(|d| d.direction == "east").expect("east door");
    assert!(east.is_closed);
    assert!(!east.is_locked, "D state 1 closes but does not lock");
    let south = room.doors.iter().find(|d| d.direction == "south").expect("south door");
    assert!(south.is_closed);
    assert!(south.is_locked, "D state 2 closes and locks");

    // D against west: no door defined → warn.
    let no_door_warn = warnings
        .iter()
        .filter(|w| {
            matches!(w.kind, ironmud::import::WarningKind::DeferredFeature)
                && w.message.contains("no matching door")
        })
        .count();
    assert_eq!(no_door_warn, 1);

    // No Block warnings expected.
    let blocks = warnings.iter().filter(|w| w.severity == Severity::Block).count();
    assert_eq!(blocks, 0);

    // Round-trip through the writer: spawn points land in the DB and carry
    // their dependencies.
    let db_path = dir.join("ironmud.db");
    {
        let db = Db::open(&db_path).expect("open tmp db");
        let summary = writer::apply(&db, &plan, &warnings).expect("apply");
        assert_eq!(summary.written_spawns, 6);
        let stored = db.list_all_spawn_points().unwrap();
        assert_eq!(stored.len(), 6);
        // Two wanderer spawn points exist (the second M reset is bare). The
        // first one carries G+E dependencies — find it by dep count.
        let wanderer_sp = stored
            .iter()
            .find(|sp| {
                matches!(sp.entity_type, SpawnEntityType::Mobile)
                    && sp.vnum == "reset_test_zone_8001"
                    && !sp.dependencies.is_empty()
            })
            .expect("wanderer SP with deps persisted");
        assert_eq!(wanderer_sp.dependencies.len(), 2);
        assert_eq!(wanderer_sp.respawn_interval_secs, 600);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Inline fixture builder for the reset-translation test. Each writer
/// creates the minimum file shape `CircleEngine.parse` will accept (zon +
/// wld + mob + obj under `lib/world/`).
fn write_reset_fixture(root: &PathBuf) -> PathBuf {
    let world = root.join("lib/world");
    std::fs::create_dir_all(world.join("zon")).unwrap();
    std::fs::create_dir_all(world.join("wld")).unwrap();
    std::fs::create_dir_all(world.join("mob")).unwrap();
    std::fs::create_dir_all(world.join("obj")).unwrap();

    // One zone (#80, vnums 8000..8099, lifespan 10 minutes → 600s respawn).
    std::fs::write(
        world.join("zon/80.zon"),
        "#80\n\
         Reset Test Zone~\n\
         8000 8099 10 2\n\
         * resets follow\n\
         M 0 8001 1 8001         Wanderer\n\
         G 1 8050 1                      sword in inventory\n\
         E 1 8051 1 16                   wielded\n\
         M 0 8002 1 8001         Beast (slot 0 test)\n\
         E 1 8050 1 0                    LIGHT (slot 0 — drop)\n\
         O 0 8060 1 8001         a chest\n\
         P 1 8050 1 8060                 sword inside chest\n\
         O 0 8061 1 8001         non-container item\n\
         P 1 8050 1 8061                 P after non-Container — drop\n\
         O 0 8060 1 8001         second chest spawn for mismatch test\n\
         P 1 8050 1 8099                 P container_vnum mismatch — drop\n\
         M 0 8001 1 8001         second wanderer for if=0 G test\n\
         G 0 8050 1                      G if=0 — drop\n\
         D 0 8001 1 1                    east door closed\n\
         D 0 8001 2 2                    south door closed+locked\n\
         D 0 8001 3 1                    west: no door defined → warn\n\
         R 0 8001 8050                   R command (no analogue) → warn\n\
         S\n\
         $\n",
    )
    .unwrap();

    // One room with east door (ISDOOR+CLOSED) and south door (ISDOOR+LOCKED);
    // both targets self for simplicity. Sector 1 (CITY).
    std::fs::write(
        world.join("wld/80.wld"),
        "#8001\n\
         Test Room~\n\
         A test room.\n\
         ~\n\
         80 0 1\n\
         D0\n\
         ~\n\
         ~\n\
         0 -1 8001\n\
         D1\n\
         ~\n\
         gate~\n\
         1 -1 8001\n\
         D2\n\
         ~\n\
         iron door~\n\
         3 -1 8001\n\
         S\n\
         $\n",
    )
    .unwrap();

    // Two mobs at vnums 8001 / 8002.
    std::fs::write(
        world.join("mob/80.mob"),
        "#8001\n\
         wanderer test~\n\
         a test wanderer~\n\
         A test wanderer stands here.~\n\
         A simple test mob.\n\
         ~\n\
         0 0 0 S\n\
         3 20 10 1d4+1 1d4+0\n\
         50 0\n\
         8 8 1\n\
         #8002\n\
         beast test~\n\
         a test beast~\n\
         A test beast prowls.~\n\
         A second test mob.\n\
         ~\n\
         0 0 0 S\n\
         3 20 10 1d4+1 1d4+0\n\
         50 0\n\
         8 8 1\n\
         $\n",
    )
    .unwrap();

    // Three items: a sword (weapon, vnum 8050), a wand (vnum 8051), a
    // container (vnum 8060), and a non-container misc item (vnum 8061).
    std::fs::write(
        world.join("obj/80.obj"),
        "#8050\n\
         sword test~\n\
         a test sword~\n\
         A test sword lies here.~\n\
         ~\n\
         5 0 16385\n\
         0 1 8 3\n\
         5 100 100\n\
         #8051\n\
         wand test~\n\
         a test wand~\n\
         A test wand lies here.~\n\
         ~\n\
         3 0 1\n\
         0 5 5 0\n\
         2 100 50\n\
         #8060\n\
         chest test~\n\
         a test chest~\n\
         A test chest sits here.~\n\
         ~\n\
         15 0 1\n\
         100 0 -1 0\n\
         50 100 100\n\
         #8061\n\
         orb test~\n\
         a test orb~\n\
         A test orb floats here.~\n\
         ~\n\
         12 0 1\n\
         0 0 0 0\n\
         1 50 25\n\
         $\n",
    )
    .unwrap();

    root.clone()
}

fn tmpdir(prefix: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    p.push(format!("{prefix}-{pid}-{nonce}"));
    std::fs::create_dir_all(&p).expect("create tmp dir");
    p
}

// ===== Specproc / trigger import tests =====

#[test]
fn parses_specprocs_into_ir() {
    let (ir, _) = CircleEngine.parse(&fixture_root()).expect("parse");
    // 5 from spec_assign.c (cityguard 9001, puff 9002, magic_user 9003,
    // cityguard 99999 orphan, bank 9010, dump 9001) + 1 from castle.c
    // (king_welmar 9002).
    assert_eq!(ir.triggers.len(), 7, "all literal ASSIGN/castle bindings parsed");
    let names: Vec<&str> = ir.triggers.iter().map(|t| t.specproc_name.as_str()).collect();
    assert!(names.contains(&"cityguard"));
    assert!(names.contains(&"puff"));
    assert!(names.contains(&"magic_user"));
    assert!(names.contains(&"bank"));
    assert!(names.contains(&"dump"));
    assert!(names.contains(&"king_welmar"));
    // Puff binding picked up the do_say quotes from spec_procs.c.
    let puff = ir
        .triggers
        .iter()
        .find(|t| t.specproc_name == "puff")
        .expect("puff binding");
    assert_eq!(puff.args.len(), 2);
    assert_eq!(puff.args[0], "My god!  It's full of stars!");
}

#[test]
fn applies_specprocs_to_tmp_db() {
    use ironmud::types::{ItemTriggerType, MobileTriggerType, TriggerType};

    let dir = tmpdir("ironmud-import-specs");
    let db_path = dir.join("ironmud.db");
    {
        let db = Db::open(&db_path).expect("open db");
        let (ir, _) = CircleEngine.parse(&fixture_root()).expect("parse");
        let opts = MappingOptions {
            circle: mapping::CircleMappingTable::load_default(),
            existing_area_prefixes: Vec::new(),
            existing_room_vnums: Vec::new(),
            existing_mobile_vnums: Vec::new(),
            existing_item_vnums: Vec::new(),
        };
        let (plan, warnings) = mapping::ir_to_plan(&ir, &opts);

        // 5 overlays expected (4 resolvable bindings produce flags/triggers,
        // king_welmar warns rather than overlays, magic_user warns,
        // 99999 orphan drops). cityguard → guard flag, puff → trigger,
        // bank → item trigger, dump → room trigger.
        assert_eq!(
            plan.trigger_overlays.len(),
            4,
            "cityguard, puff, bank, dump → 4 overlays; magic_user/king_welmar warn-only; 99999 orphan dropped"
        );

        // Apply.
        let summary = writer::apply(&db, &plan, &warnings).expect("apply");
        assert_eq!(summary.applied_triggers, 4);

        // Verify cityguard set the guard flag on mob 9001.
        let cityguard = db
            .get_mobile_by_vnum("test_fixture_village_9001")
            .unwrap()
            .expect("cityguard mob");
        assert!(cityguard.flags.guard, "cityguard → MobileFlags.guard");

        // Verify puff received an OnIdle trigger with extracted quotes.
        let puff = db
            .get_mobile_by_vnum("test_fixture_village_9002")
            .unwrap()
            .expect("puff mob");
        let puff_trig = puff
            .triggers
            .iter()
            .find(|t| matches!(t.trigger_type, MobileTriggerType::OnIdle))
            .expect("puff OnIdle trigger");
        assert_eq!(puff_trig.script_name, "@say_random");
        assert_eq!(puff_trig.args.len(), 2);
        assert!(puff_trig.args[0].starts_with("My god"));

        // Verify bank set an OnUse trigger on item 9010.
        let bank_item = db
            .get_item_by_vnum("test_fixture_village_9010")
            .unwrap()
            .expect("bank item");
        let bank_trig = bank_item
            .triggers
            .iter()
            .find(|t| matches!(t.trigger_type, ItemTriggerType::OnUse))
            .expect("bank OnUse trigger");
        assert_eq!(bank_trig.script_name, "@message");

        // Verify dump set a Periodic trigger on room 9001.
        let dump_room = db
            .get_room_by_vnum("test_fixture_village_9001")
            .unwrap()
            .expect("dump room");
        let dump_trig = dump_room
            .triggers
            .iter()
            .find(|t| matches!(t.trigger_type, TriggerType::Periodic))
            .expect("dump Periodic trigger");
        assert_eq!(dump_trig.script_name, "@room_message");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn magic_user_warns_dedup_with_count() {
    let (ir, _) = CircleEngine.parse(&fixture_root()).expect("parse");
    let opts = MappingOptions {
        circle: mapping::CircleMappingTable::load_default(),
        existing_area_prefixes: Vec::new(),
        existing_room_vnums: Vec::new(),
        existing_mobile_vnums: Vec::new(),
        existing_item_vnums: Vec::new(),
    };
    let (_plan, warnings) = mapping::ir_to_plan(&ir, &opts);
    let magic_warns: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains("magic_user"))
        .collect();
    // Even though there's only one magic_user binding in the fixture, the
    // dedup pipeline still emits exactly one warn per specproc.
    assert_eq!(
        magic_warns.len(),
        1,
        "magic_user collapses to a single dedup warn line"
    );
}

#[test]
fn out_of_set_vnum_drops_with_warn() {
    let (ir, _) = CircleEngine.parse(&fixture_root()).expect("parse");
    let opts = MappingOptions {
        circle: mapping::CircleMappingTable::load_default(),
        existing_area_prefixes: Vec::new(),
        existing_room_vnums: Vec::new(),
        existing_mobile_vnums: Vec::new(),
        existing_item_vnums: Vec::new(),
    };
    let (_plan, warnings) = mapping::ir_to_plan(&ir, &opts);
    // The 99999 cityguard binding doesn't match any planned mob; it
    // surfaces as an orphan-bucket warn naming the specproc + vnum.
    let orphan = warnings
        .iter()
        .find(|w| w.message.contains("99999") && w.message.contains("cityguard"));
    assert!(orphan.is_some(), "orphan binding warns: {:?}", warnings);
}

#[test]
fn castle_binding_warns() {
    let (ir, _) = CircleEngine.parse(&fixture_root()).expect("parse");
    let opts = MappingOptions {
        circle: mapping::CircleMappingTable::load_default(),
        existing_area_prefixes: Vec::new(),
        existing_room_vnums: Vec::new(),
        existing_mobile_vnums: Vec::new(),
        existing_item_vnums: Vec::new(),
    };
    let (plan, warnings) = mapping::ir_to_plan(&ir, &opts);
    // king_welmar overlays nothing — it's a bespoke C body, warn-only. But
    // it shares vnum 9002 with the puff binding (which DOES overlay).
    // Because the overlay was for puff, the king_welmar binding bucket
    // produces a warn and no overlay is added for it.
    let castle = warnings
        .iter()
        .find(|w| w.message.contains("king_welmar"));
    assert!(castle.is_some(), "castle binding warns: {:?}", warnings);
    // The puff overlay still lands.
    assert!(
        plan.trigger_overlays
            .iter()
            .any(|o| o.specproc_name == "puff"),
        "puff overlay still applied"
    );
}

#[test]
fn missing_spec_assign_is_info_only() {
    // Synthetic minimal tree with no `src/` directory.
    let dir = tmpdir("ironmud-import-no-spec");
    let world = dir.join("lib").join("world");
    std::fs::create_dir_all(world.join("wld")).unwrap();
    std::fs::create_dir_all(world.join("zon")).unwrap();
    std::fs::write(world.join("wld").join("90.wld"), "$\n").unwrap();
    std::fs::write(
        world.join("zon").join("90.zon"),
        "#90\nMin Zone~\n9000 9099 30 2\nS\n$\n",
    )
    .unwrap();

    let (ir, parse_warnings) = CircleEngine.parse(&dir).expect("parse");
    assert_eq!(ir.triggers.len(), 0, "no triggers parsed");
    let infos: Vec<_> = parse_warnings
        .iter()
        .filter(|w| w.severity == Severity::Info && w.message.contains("spec_assign"))
        .collect();
    assert_eq!(
        infos.len(),
        1,
        "missing spec_assign.c surfaces a single Info warning"
    );
    let non_info: Vec<_> = parse_warnings
        .iter()
        .filter(|w| w.severity != Severity::Info)
        .collect();
    assert!(non_info.is_empty(), "no Warn/Block from missing src/");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn maps_high_priority_room_flag_bits() {
    use ironmud::import::{ImportIR, IrRoom, IrZone, SourceLoc};

    // CircleMUD bit layout:
    //   DEATH=1, NOMAGIC=7, TUNNEL=8, PRIVATE=9
    let bits = (1u64 << 1) | (1u64 << 7) | (1u64 << 8) | (1u64 << 9);

    let zone = IrZone {
        vnum: 42,
        name: "Flag Test Zone".into(),
        description: None,
        vnum_range: Some((4200, 4299)),
        default_respawn_secs: None,
        source: SourceLoc::default(),
        rooms: vec![IrRoom {
            vnum: 4200,
            name: "Flag Test Room".into(),
            description: "A room used to verify high-priority flag bits map cleanly.".into(),
            sector: 0, // INSIDE
            flag_bits: bits,
            unknown_flag_names: Vec::new(),
            exits: Vec::new(),
            extras: Vec::new(),
            source: SourceLoc::default(),
        }],
        mobiles: Vec::new(),
        items: Vec::new(),
        shops: Vec::new(),
        resets: Vec::new(),
        deferred: Vec::new(),
    };
    let ir = ImportIR {
        zones: vec![zone],
        triggers: Vec::new(),
    };

    let opts = MappingOptions {
        circle: mapping::CircleMappingTable::load_default(),
        existing_area_prefixes: Vec::new(),
        existing_room_vnums: Vec::new(),
        existing_mobile_vnums: Vec::new(),
        existing_item_vnums: Vec::new(),
    };
    let (plan, warnings) = mapping::ir_to_plan(&ir, &opts);

    assert_eq!(plan.rooms.len(), 1);
    let room = &plan.rooms[0];
    assert!(room.flags.death, "DEATH bit must set RoomFlags.death");
    assert!(room.flags.no_magic, "NOMAGIC bit must set RoomFlags.no_magic");
    assert!(room.flags.tunnel, "TUNNEL bit must set RoomFlags.tunnel");
    assert!(room.flags.private_room, "PRIVATE bit must set RoomFlags.private_room");

    // None of the four high-priority flags should produce Warn/Block any more.
    let flag_warns: Vec<_> = warnings
        .iter()
        .filter(|w| {
            w.severity != Severity::Info
                && (w.message.contains("ROOM_DEATH")
                    || w.message.contains("ROOM_NOMAGIC")
                    || w.message.contains("ROOM_TUNNEL")
                    || w.message.contains("ROOM_PRIVATE"))
        })
        .collect();
    assert!(
        flag_warns.is_empty(),
        "high-priority flags must import silently: {:?}",
        flag_warns
    );
}

#[test]
fn maps_medium_priority_room_flag_bits() {
    use ironmud::import::{ImportIR, IrRoom, IrZone, SourceLoc};

    // CircleMUD bit layout: SOUNDPROOF=5, NOTRACK=6
    let bits = (1u64 << 5) | (1u64 << 6);

    let zone = IrZone {
        vnum: 43,
        name: "Medium Flag Zone".into(),
        description: None,
        vnum_range: Some((4300, 4399)),
        default_respawn_secs: None,
        source: SourceLoc::default(),
        rooms: vec![IrRoom {
            vnum: 4300,
            name: "Quiet Room".into(),
            description: "Verifies SOUNDPROOF and NOTRACK map cleanly.".into(),
            sector: 0,
            flag_bits: bits,
            unknown_flag_names: Vec::new(),
            exits: Vec::new(),
            extras: Vec::new(),
            source: SourceLoc::default(),
        }],
        mobiles: Vec::new(),
        items: Vec::new(),
        shops: Vec::new(),
        resets: Vec::new(),
        deferred: Vec::new(),
    };
    let ir = ImportIR {
        zones: vec![zone],
        triggers: Vec::new(),
    };

    let opts = MappingOptions {
        circle: mapping::CircleMappingTable::load_default(),
        existing_area_prefixes: Vec::new(),
        existing_room_vnums: Vec::new(),
        existing_mobile_vnums: Vec::new(),
        existing_item_vnums: Vec::new(),
    };
    let (plan, warnings) = mapping::ir_to_plan(&ir, &opts);

    assert_eq!(plan.rooms.len(), 1);
    let room = &plan.rooms[0];
    assert!(room.flags.soundproof, "SOUNDPROOF bit must set RoomFlags.soundproof");
    assert!(room.flags.notrack, "NOTRACK bit must set RoomFlags.notrack");

    let flag_warns: Vec<_> = warnings
        .iter()
        .filter(|w| {
            w.severity != ironmud::import::Severity::Info
                && (w.message.contains("ROOM_SOUNDPROOF") || w.message.contains("ROOM_NOTRACK"))
        })
        .collect();
    assert!(
        flag_warns.is_empty(),
        "medium-priority flags must import silently: {:?}",
        flag_warns
    );
}

#[test]
fn imports_pickproof_doors() {
    use ironmud::import::{ImportIR, IrExit, IrRoom, IrZone, SourceLoc};

    // EX_ISDOOR=0, EX_CLOSED=1, EX_LOCKED=2, EX_PICKPROOF=3 → bits 0|2|3 = 0xD
    let door_flags: u32 = (1 << 0) | (1 << 2) | (1 << 3);

    let zone = IrZone {
        vnum: 44,
        name: "Pickproof Zone".into(),
        description: None,
        vnum_range: Some((4400, 4499)),
        default_respawn_secs: None,
        source: SourceLoc::default(),
        rooms: vec![
            IrRoom {
                vnum: 4400,
                name: "Vault Antechamber".into(),
                description: "A heavy vault door blocks the way north.".into(),
                sector: 0,
                flag_bits: 0,
                unknown_flag_names: Vec::new(),
                exits: vec![IrExit {
                    direction: "north".into(),
                    general_description: Some("A massive iron-bound vault door.".into()),
                    keyword: Some("vault door".into()),
                    door_flags,
                    unknown_door_flags: Vec::new(),
                    key_vnum: None,
                    to_room_vnum: 4401,
                }],
                extras: Vec::new(),
                source: SourceLoc::default(),
            },
            IrRoom {
                vnum: 4401,
                name: "Vault Interior".into(),
                description: "Inside the vault.".into(),
                sector: 0,
                flag_bits: 0,
                unknown_flag_names: Vec::new(),
                exits: Vec::new(),
                extras: Vec::new(),
                source: SourceLoc::default(),
            },
        ],
        mobiles: Vec::new(),
        items: Vec::new(),
        shops: Vec::new(),
        resets: Vec::new(),
        deferred: Vec::new(),
    };
    let ir = ImportIR {
        zones: vec![zone],
        triggers: Vec::new(),
    };

    let opts = MappingOptions {
        circle: mapping::CircleMappingTable::load_default(),
        existing_area_prefixes: Vec::new(),
        existing_room_vnums: Vec::new(),
        existing_mobile_vnums: Vec::new(),
        existing_item_vnums: Vec::new(),
    };
    let (plan, warnings) = mapping::ir_to_plan(&ir, &opts);

    let antechamber = plan
        .rooms
        .iter()
        .find(|r| r.source_vnum == 4400)
        .expect("antechamber room mapped");
    assert_eq!(antechamber.doors.len(), 1, "one door expected");
    let door = &antechamber.doors[0];
    assert_eq!(door.direction, "north");
    assert!(door.is_locked, "LOCKED bit still drives is_locked");
    assert!(door.pickproof, "PICKPROOF bit must set DoorState.pickproof");

    let pickproof_warns: Vec<_> = warnings
        .iter()
        .filter(|w| {
            w.severity != ironmud::import::Severity::Info && w.message.contains("pickproof")
        })
        .collect();
    assert!(
        pickproof_warns.is_empty(),
        "PICKPROOF must import silently: {:?}",
        pickproof_warns
    );
}

//! End-to-end test for the CircleMUD importer:
//! parse → map → apply → assert via Db queries.

use std::path::PathBuf;

use ironmud::db::Db;
use ironmud::import::{MappingOptions, MudEngine, Severity, engines::circle::CircleEngine, mapping, writer};
use ironmud::types::{ActivityState, DoorState};

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
    // Fixture has two M resets: wanderer 9001 → room 9001, postmaster
    // 9004 → room 9003.
    assert_eq!(plan.spawns.len(), 2, "M resets translated to spawn points");
    let wanderer_spawn = plan
        .spawns
        .iter()
        .find(|s| s.vnum == "test_fixture_village_9001")
        .expect("wanderer spawn");
    assert_eq!(wanderer_spawn.room_vnum, "test_fixture_village_9001");
    assert_eq!(wanderer_spawn.respawn_interval_secs, 30 * 60);
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
    assert_eq!(zone.mobiles.len(), 5, "five fixture mobs");

    let opts = MappingOptions {
        circle: mapping::CircleMappingTable::load_default(),
        existing_area_prefixes: Vec::new(),
        existing_room_vnums: Vec::new(),
        existing_mobile_vnums: Vec::new(),
        existing_item_vnums: Vec::new(),
    };
    let (plan, warnings) = mapping::ir_to_plan(&ir, &opts);
    assert_eq!(plan.mobiles.len(), 5);

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
        assert_eq!(summary.written_mobiles, 5);
        assert_eq!(summary.planned_mobiles, 5);

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
        // SEX 1 in the fixture stamps a default Characteristics with male.
        let chars = wanderer
            .characteristics
            .as_ref()
            .expect("SEX 1 should install Characteristics");
        assert_eq!(chars.gender, "male", "SEX 1 → male");

        // Fixture beast (9002) has SEX 0 — no Characteristics installed.
        let beast = db
            .get_mobile_by_vnum("test_fixture_village_9002")
            .unwrap()
            .expect("beast saved");
        assert!(
            beast.characteristics.is_none(),
            "SEX 0 leaves Characteristics None (resolves as neuter in DG)"
        );

        // Importer should not surface the legacy "sex/gender not modeled"
        // warning anymore.
        assert!(
            !warnings.iter().any(|w| w.message.contains("sex/gender not modeled")),
            "legacy SEX warning must be gone now that the importer maps it"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn parses_items_into_plan() {
    use ironmud::types::{DamageType, ItemType, LiquidType, WearLocation};

    let (ir, parse_warnings) = CircleEngine.parse(&fixture_root()).expect("parse");
    assert_eq!(parse_warnings.len(), 0, "fixture should parse cleanly");
    let zone = &ir.zones[0];
    assert_eq!(zone.items.len(), 15, "fifteen fixture items in obj/9000.obj");

    let opts = MappingOptions {
        circle: mapping::CircleMappingTable::load_default(),
        existing_area_prefixes: Vec::new(),
        existing_room_vnums: Vec::new(),
        existing_mobile_vnums: Vec::new(),
        existing_item_vnums: Vec::new(),
    };
    let (plan, warnings) = mapping::ir_to_plan(&ir, &opts);
    assert_eq!(plan.items.len(), 15);

    // Sword: weapon, 1d8 slashing, GLOW (extra-bit `a` = bit 0), wearable
    // wielded; APPLY_DAMROLL +2 → damage_bonus = 2 (CircleMUD APPLY_DAMROLL parity).
    let sword = plan.items.iter().find(|i| i.source_vnum == 9010).expect("sword");
    assert_eq!(sword.vnum, "test_fixture_village_9010");
    assert!(sword.data.is_prototype);
    assert_eq!(sword.data.item_type, ItemType::Weapon);
    assert_eq!(sword.data.damage_dice_count, 1);
    assert_eq!(sword.data.damage_dice_sides, 8);
    assert_eq!(sword.data.damage_type, DamageType::Slashing);
    assert!(sword.data.flags.glow);
    assert!(sword.data.flags.magical);
    assert!(
        sword
            .data
            .categories
            .iter()
            .any(|c| c.eq_ignore_ascii_case("magical")),
        "ITEM_MAGIC bit should auto-tag categories with \"magical\""
    );
    assert!(sword.data.wear_locations.contains(&WearLocation::Wielded));
    assert_eq!(sword.data.value, 600);
    assert_eq!(sword.data.damage_bonus, 2, "APPLY_DAMROLL +2 lands on damage_bonus");
    let damroll_warn = warnings
        .iter()
        .find(|w| w.message.contains("APPLY_DAMROLL"));
    assert!(damroll_warn.is_none(), "APPLY_DAMROLL warn should be gone");

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

    // Food: nutrition=24, poisoned=true, has an E-block which now copies
    // 1:1 onto ItemData.extra_descs.
    let food = plan.items.iter().find(|i| i.source_vnum == 9014).expect("food");
    assert_eq!(food.data.item_type, ItemType::Food);
    assert_eq!(food.data.food_nutrition, 24);
    assert!(food.data.food_poisoned);
    assert_eq!(food.data.extra_descs.len(), 1, "food fixture has one E-block");
    assert!(!food.data.extra_descs[0].keywords.is_empty());
    assert!(!food.data.extra_descs[0].description.is_empty());
    // The DeferredFeature warning for item E-blocks should no longer fire.
    let extra_warn = warnings.iter().find(|w| {
        matches!(w.kind, ironmud::import::WarningKind::DeferredFeature)
            && w.message.contains("extra description")
    });
    assert!(extra_warn.is_none(), "item E-block deferred warning should be gone");

    // Liquid container: ale, 50/50, not poisoned.
    let barrel = plan.items.iter().find(|i| i.source_vnum == 9015).expect("barrel");
    assert_eq!(barrel.data.item_type, ItemType::LiquidContainer);
    assert_eq!(barrel.data.liquid_max, 50);
    assert_eq!(barrel.data.liquid_current, 50);
    assert_eq!(barrel.data.liquid_type, LiquidType::Ale);
    assert!(!barrel.data.liquid_poisoned);

    // Light: provides_light + capacity-hours mapped onto light_hours_remaining.
    let torch = plan.items.iter().find(|i| i.source_vnum == 9016).expect("torch");
    assert_eq!(torch.data.item_type, ItemType::Misc);
    assert!(torch.data.flags.provides_light);
    assert_eq!(torch.data.light_hours_remaining, 24);
    assert!(
        !warnings.iter().any(|w| w.message.contains("ITEM_LIGHT capacity")),
        "ITEM_LIGHT capacity hours should no longer warn"
    );

    // Wand: CircleMUD ITEM_WAND (type 3). v[0]=1 min level, v[1]=32 → magic_missile,
    // v[2]=5 max charges, v[3]=5 current charges. Imports as ItemType::Wand with
    // a populated cast_on_use payload — no spell-list warnings.
    let wand = plan.items.iter().find(|i| i.source_vnum == 9018).expect("wand");
    assert_eq!(wand.data.item_type, ItemType::Wand);
    let cou = wand.data.cast_on_use.as_ref().expect("wand has cast_on_use populated");
    assert_eq!(cou.spell, "magic_missile");
    assert_eq!(cou.min_level, 1);
    assert_eq!(cou.charges, 5);
    assert_eq!(cou.max_charges, 5);
    assert!(
        !warnings.iter().any(|w| w.message.contains("ITEM_WAND")),
        "ITEM_WAND should no longer warn for known spells"
    );

    // Fountain: ITEM_FOUNTAIN (type 23). Imports as a LiquidContainer with the
    // infinite sentinel (`liquid_max == -1`) so drink_from / fill skip
    // decrement and stock fountains never run dry. v[0]/v[1] from the .obj
    // are intentionally ignored.
    let fountain = plan.items.iter().find(|i| i.source_vnum == 9019).expect("fountain");
    assert_eq!(fountain.data.item_type, ItemType::LiquidContainer);
    assert_eq!(fountain.data.liquid_max, -1, "fountain is infinite");
    assert_eq!(fountain.data.liquid_current, -1, "fountain is infinite");
    assert!(
        !warnings.iter().any(|w| w.message.contains("ITEM_FOUNTAIN")),
        "ITEM_FOUNTAIN should no longer warn"
    );

    // Enchanted ring: APPLY_MAXHIT +20 / APPLY_MAXMANA +15 land on the new
    // ItemData fields (CircleMUD parity). No APPLY_MAXHIT/MAXMANA warnings.
    let ring = plan.items.iter().find(|i| i.source_vnum == 9020).expect("ring");
    assert_eq!(ring.data.max_hp_bonus, 20, "APPLY_MAXHIT +20 → max_hp_bonus");
    assert_eq!(ring.data.max_mana_bonus, 15, "APPLY_MAXMANA +15 → max_mana_bonus");
    assert!(
        !warnings.iter().any(|w| w.message.contains("APPLY_MAXHIT")
            || w.message.contains("APPLY_MAXMANA")),
        "APPLY_MAXHIT/MAXMANA should no longer warn"
    );

    // Paper / pen: ITEM_NOTE → Note, ITEM_PEN → Pen. Both used to warn
    // ("not modeled"); now they import cleanly with their dedicated types.
    let paper = plan.items.iter().find(|i| i.source_vnum == 9021).expect("paper");
    assert_eq!(paper.data.item_type, ItemType::Note);
    assert!(paper.data.note_content.is_none(), "blank paper has no note body");
    let pen = plan.items.iter().find(|i| i.source_vnum == 9022).expect("pen");
    assert_eq!(pen.data.item_type, ItemType::Pen);
    assert!(
        !warnings.iter().any(|w| w.message.contains("ITEM_NOTE") || w.message.contains("ITEM_PEN")),
        "ITEM_NOTE/PEN should no longer warn"
    );

    // Cursed trinket: NODONATE bit (extra-flag bit 3) flips to flags.no_donate.
    // Used to be silently dropped via the JSON mapping; now lands as a flag.
    let trinket = plan.items.iter().find(|i| i.source_vnum == 9023).expect("trinket");
    assert!(trinket.data.flags.no_donate, "NODONATE → flags.no_donate");
    assert!(
        !warnings.iter().any(|w| w.message.contains("ITEM_NODONATE")),
        "NODONATE should not surface as a warning"
    );

    // Cursed amulet: NODROP set, ANTI_GOOD warns.
    let amulet = plan.items.iter().find(|i| i.source_vnum == 9017).expect("amulet");
    assert!(amulet.data.flags.no_drop);
    assert!(amulet.data.wear_locations.contains(&WearLocation::Neck));
    let anti_warn = warnings
        .iter()
        .find(|w| w.message.contains("ITEM_ANTI_GOOD"))
        .expect("ANTI_GOOD warn surfaced");
    assert_eq!(anti_warn.severity, Severity::Warn);

    // ITEM_BOARD (type 24) — non-stock vnum 9024 imports as Board with
    // public defaults + an Info warning steering the builder to oedit.
    let board = plan.items.iter().find(|i| i.source_vnum == 9024).expect("board");
    assert_eq!(board.data.item_type, ItemType::Board);
    assert!(!board.data.board_read_admin_only, "non-stock board defaults to public read");
    assert!(!board.data.board_write_admin_only, "non-stock board defaults to public write");
    assert_eq!(board.data.board_max_messages, Some(60));
    let board_info = warnings
        .iter()
        .find(|w| w.message.contains("ITEM_BOARD vnum 9024"))
        .expect("non-stock board surfaces Info warn");
    assert_eq!(board_info.severity, Severity::Info);

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
        assert_eq!(summary.written_items, 15);
        assert_eq!(summary.planned_items, 15);

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
    // Shop hours are now translated rather than warned — assert no
    // residual "not translated" message slipped through.
    let stale_hours_warn = shop_warns
        .iter()
        .any(|w| w.message.contains("hours") && w.message.contains("not translated"));
    assert!(
        !stale_hours_warn,
        "shop hours should be synthesized into a daily_routine, not warned"
    );

    // Default-hours shop (#9001: 0/28/0/0) leaves the routine empty so
    // the keeper trades 24/7. Dual-shift shop (#9002: 8/12/14/20)
    // yields four entries partitioning the day into Working/OffDuty
    // windows.
    assert!(s1.daily_routine.is_empty(), "always-open shop has no routine");
    assert_eq!(s2.daily_routine.len(), 4, "dual-shift shop has 4 entries");
    let pairs: Vec<(u8, ActivityState)> = s2
        .daily_routine
        .iter()
        .map(|e| (e.start_hour, e.activity.clone()))
        .collect();
    assert_eq!(
        pairs,
        vec![
            (8, ActivityState::Working),
            (12, ActivityState::OffDuty),
            (14, ActivityState::Working),
            (20, ActivityState::OffDuty),
        ]
    );

    let messages = shop_warns
        .iter()
        .filter(|w| w.message.contains("custom message string"))
        .count();
    assert_eq!(messages, 2, "every shop with non-empty messages warns once");
    let temper = shop_warns.iter().any(|w| w.message.contains("temper=2"));
    assert!(temper, "temper warn surfaced");
    // bitvector=3 = WILL_START_FIGHT (translated) + WILL_BANK_MONEY (still warn).
    let bank_warn = shop_warns.iter().any(|w| w.message.contains("WILL_BANK_MONEY"));
    assert!(bank_warn, "WILL_BANK_MONEY warn surfaced");
    let stale_combined = shop_warns.iter().any(|w| w.message.contains("WILL_START_FIGHT/WILL_BANK_MONEY"));
    assert!(
        !stale_combined,
        "WILL_START_FIGHT should be translated, not warned"
    );
    // Shop #9002 has bitvector=3 → bit 0 → hostile_on_steal=true.
    assert!(s2.hostile_on_steal, "WILL_START_FIGHT decoded onto overlay");
    assert!(!s1.hostile_on_steal, "shop without WILL_START_FIGHT stays passive");
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
        // Synthesized hours (8-12, 14-20) landed on the keeper.
        let beast_hours: Vec<(u8, ActivityState)> = beast
            .daily_routine
            .iter()
            .map(|e| (e.start_hour, e.activity.clone()))
            .collect();
        assert_eq!(
            beast_hours,
            vec![
                (8, ActivityState::Working),
                (12, ActivityState::OffDuty),
                (14, ActivityState::Working),
                (20, ActivityState::OffDuty),
            ]
        );
        // Default-hours wanderer keeps an empty routine (always Working).
        assert!(wanderer.daily_routine.is_empty());
        // WILL_START_FIGHT (bit 0 of bitvector=3) on shop #9002 stamps
        // the keeper's hostile_on_steal flag; #9001 (bitvector=0) stays clean.
        assert!(beast.flags.hostile_on_steal);
        assert!(!wanderer.flags.hostile_on_steal);
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
    // 19 reset commands in the .zon (M+G+E+M+E+O+P+O+P+O+O+P+P+M+G+D+D+D+R)
    assert_eq!(zone.resets.len(), 19, "all resets parsed structurally");

    let opts = MappingOptions {
        circle: mapping::CircleMappingTable::load_default(),
        existing_area_prefixes: Vec::new(),
        existing_room_vnums: Vec::new(),
        existing_mobile_vnums: Vec::new(),
        existing_item_vnums: Vec::new(),
    };
    let (plan, warnings) = mapping::ir_to_plan(&ir, &opts);

    // Spawn count: 3 mobs (M ×3) + 4 objs (O ×4) = 7 spawns.
    assert_eq!(plan.spawns.len(), 7, "one spawn per M and O reset");

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

    // Two P resets target a vnum with no Container O in the zone:
    //   * P 1 8050 1 8061 — 8061 is a non-Container O (not in vnum→idx map)
    //   * P 1 8050 1 8099 — 8099 has no O at all in this zone
    // Both fall through cross-block lookup and emit the same warn.
    let p_unresolved = warnings
        .iter()
        .filter(|w| {
            matches!(w.kind, ironmud::import::WarningKind::DeferredFeature)
                && w.message.contains("no Container O for that vnum")
        })
        .count();
    assert_eq!(p_unresolved, 2);

    // Cross-block P (P 1 8052 1 8060 after an intervening non-container O)
    // resolves via the per-zone vnum→spawn-index map and emits an Info note.
    let cross_block_info = warnings
        .iter()
        .filter(|w| {
            matches!(w.kind, ironmud::import::WarningKind::Info)
                && w.message.contains("resolved cross-block")
        })
        .count();
    assert_eq!(cross_block_info, 1, "one cross-block P resolves with an Info");
    // The cross-block dep landed on the second chest (most-recent 8060).
    let chest_8060_with_8052 = plan
        .spawns
        .iter()
        .filter(|s| {
            matches!(s.entity_type, SpawnEntityType::Item) && s.vnum.ends_with("_8060")
        })
        .filter(|s| {
            s.dependencies
                .iter()
                .any(|d| d.item_vnum == "reset_test_zone_8052")
        })
        .count();
    assert_eq!(chest_8060_with_8052, 1, "cross-block sword landed in exactly one 8060 spawn");

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
        assert_eq!(summary.written_spawns, 7);
        let stored = db.list_all_spawn_points().unwrap();
        assert_eq!(stored.len(), 7);
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
         O 0 8061 1 8001         intervening non-container — clears immediate anchor\n\
         P 1 8052 1 8060                 cross-block P resolves to chest #2\n\
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

    // Items: a sword (weapon, vnum 8050), a wand (vnum 8051), a second
    // sword (vnum 8052) for the cross-block P case, a container (vnum
    // 8060), and a non-container misc item (vnum 8061).
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
         #8052\n\
         second sword test~\n\
         a second test sword~\n\
         A second test sword lies here.~\n\
         ~\n\
         5 0 16385\n\
         0 1 8 3\n\
         5 100 100\n\
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
    // 7 from spec_assign.c (cityguard 9001, puff 9002, magic_user 9003,
    // postmaster 9004, snake 9005, cityguard 99999 orphan, bank 9010,
    // dump 9001) + 1 from castle.c (king_welmar 9002).
    assert_eq!(ir.triggers.len(), 9, "all literal ASSIGN/castle bindings parsed");
    let names: Vec<&str> = ir.triggers.iter().map(|t| t.specproc_name.as_str()).collect();
    assert!(names.contains(&"cityguard"));
    assert!(names.contains(&"puff"));
    assert!(names.contains(&"magic_user"));
    assert!(names.contains(&"postmaster"));
    assert!(names.contains(&"snake"));
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

        // 8 overlays expected: cityguard → guard flag, puff → mob trigger,
        // bank → item trigger, dump → room trigger, magic_user → combat
        // spell list, postmaster → fan-out room flag overlay (one room),
        // snake → fan-out into 2 SetMobFlag overlays (aggressive +
        // poisonous). king_welmar warns rather than overlays; 99999 orphan
        // drops.
        assert_eq!(
            plan.trigger_overlays.len(),
            8,
            "cityguard, puff, bank, dump, magic_user, postmaster, snake×2 → 8 overlays; king_welmar warn-only; 99999 orphan dropped"
        );

        // Apply.
        let summary = writer::apply(&db, &plan, &warnings).expect("apply");
        assert_eq!(summary.applied_triggers, 8);

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

        // Verify magic_user set a combat-spell rotation on mob 9003.
        let mage = db
            .get_mobile_by_vnum("test_fixture_village_9003")
            .unwrap()
            .expect("magic_user mob");
        assert!(
            !mage.combat_spells.is_empty(),
            "magic_user → combat_spells populated"
        );
        assert!(mage.combat_spells.contains(&"magic_missile".to_string()));
        assert!(mage.combat_spell_chance > 0 && mage.combat_spell_chance <= 100);

        // Verify postmaster fan-out stamped post_office on the room mob
        // 9004 was M-reset into (room 9003, the courtyard).
        let post_office_room = db
            .get_room_by_vnum("test_fixture_village_9003")
            .unwrap()
            .expect("postmaster spawn room");
        assert!(
            post_office_room.flags.post_office,
            "postmaster → RoomFlags.post_office on the room hosting mob 9004"
        );

        // Verify snake fan-out stamped BOTH aggressive and poisonous on
        // mob 9005 (set_mob_flags plural action).
        let snake = db
            .get_mobile_by_vnum("test_fixture_village_9005")
            .unwrap()
            .expect("snake mob");
        assert!(
            snake.flags.aggressive,
            "snake → MobileFlags.aggressive (first flag of set_mob_flags)"
        );
        assert!(
            snake.flags.poisonous,
            "snake → MobileFlags.poisonous (second flag of set_mob_flags)"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn magic_user_imports_combat_spell_list() {
    let (ir, _) = CircleEngine.parse(&fixture_root()).expect("parse");
    let opts = MappingOptions {
        circle: mapping::CircleMappingTable::load_default(),
        existing_area_prefixes: Vec::new(),
        existing_room_vnums: Vec::new(),
        existing_mobile_vnums: Vec::new(),
        existing_item_vnums: Vec::new(),
    };
    let (plan, warnings) = mapping::ir_to_plan(&ir, &opts);

    // No more warn-only collapse for magic_user — it now translates to a
    // SetMobCombatSpells overlay so imported mobs cast in combat.
    let magic_warns: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains("magic_user"))
        .collect();
    assert!(
        magic_warns.is_empty(),
        "magic_user no longer warns: {:?}",
        magic_warns
    );

    let magic_overlays: Vec<_> = plan
        .trigger_overlays
        .iter()
        .filter(|ov| matches!(ov.mutation, ironmud::import::TriggerMutation::SetMobCombatSpells { .. }))
        .collect();
    assert_eq!(
        magic_overlays.len(),
        1,
        "exactly one SetMobCombatSpells overlay for the fixture's magic_user binding"
    );
}

#[test]
fn postmaster_imports_post_office_flag() {
    let (ir, _) = CircleEngine.parse(&fixture_root()).expect("parse");
    let opts = MappingOptions {
        circle: mapping::CircleMappingTable::load_default(),
        existing_area_prefixes: Vec::new(),
        existing_room_vnums: Vec::new(),
        existing_mobile_vnums: Vec::new(),
        existing_item_vnums: Vec::new(),
    };
    let (plan, warnings) = mapping::ir_to_plan(&ir, &opts);

    // No more warn-only collapse for postmaster — it now fans out into a
    // SetRoomFlag overlay on each room the postmaster is M-reset into.
    let postmaster_warns: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains("postmaster"))
        .collect();
    assert!(
        postmaster_warns.is_empty(),
        "postmaster no longer warns: {:?}",
        postmaster_warns
    );

    // Exactly one room overlay flipping post_office for the fixture's
    // single postmaster M-reset.
    let post_office_overlays: Vec<_> = plan
        .trigger_overlays
        .iter()
        .filter(|ov| {
            ov.specproc_name == "postmaster"
                && matches!(
                    &ov.mutation,
                    ironmud::import::TriggerMutation::SetRoomFlag { ironmud_flag } if ironmud_flag == "post_office"
                )
        })
        .collect();
    assert_eq!(
        post_office_overlays.len(),
        1,
        "exactly one SetRoomFlag(post_office) overlay for the fixture's postmaster binding"
    );
    assert_eq!(post_office_overlays[0].target_vnum, "test_fixture_village_9003");
    assert!(matches!(
        post_office_overlays[0].attach_type,
        ironmud::import::AttachType::Room
    ));
}

#[test]
fn postmaster_with_no_spawn_warns() {
    // Synthetic minimal fixture: a postmaster mob with NO M-reset placing
    // it. The fan-out should produce zero overlays and one Warn line.
    let dir = tmpdir("ironmud-import-pm-orphan");
    let world = dir.join("lib").join("world");
    std::fs::create_dir_all(world.join("wld")).unwrap();
    std::fs::create_dir_all(world.join("zon")).unwrap();
    std::fs::create_dir_all(world.join("mob")).unwrap();
    std::fs::create_dir_all(world.join("obj")).unwrap();
    std::fs::create_dir_all(world.join("shp")).unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    // Single room.
    std::fs::write(
        world.join("wld").join("90.wld"),
        "#9001\nLonely Room~\n   A lonely room.\n~\n0 0 0\nS\n$\n",
    )
    .unwrap();
    // Single mob, no M-reset for it.
    std::fs::write(
        world.join("mob").join("90.mob"),
        "#9100\norphan postmaster~\nthe orphan postmaster~\nA bespectacled clerk waits behind a counter.\n~\nUnplaced fixture postmaster.\n~\nb 0 0 S\n5 15 5 4d6+20 0d0+0\n0 0\n8 8 1\n$\n",
    )
    .unwrap();
    std::fs::write(world.join("obj").join("90.obj"), "$\n").unwrap();
    std::fs::write(world.join("shp").join("90.shp"), "$~\n").unwrap();
    // Zone with NO M-reset for mob 9100 (intentional).
    std::fs::write(
        world.join("zon").join("90.zon"),
        "#90\nOrphan Zone~\n9000 9099 30 2\nS\n$\n",
    )
    .unwrap();
    // spec_assign binding the orphan mob to postmaster.
    std::fs::write(
        dir.join("src").join("spec_assign.c"),
        "void assign_mobiles(void) {\n  ASSIGNMOB(9100, postmaster);\n}\n",
    )
    .unwrap();

    let (ir, _) = CircleEngine.parse(&dir).expect("parse");
    let opts = MappingOptions {
        circle: mapping::CircleMappingTable::load_default(),
        existing_area_prefixes: Vec::new(),
        existing_room_vnums: Vec::new(),
        existing_mobile_vnums: Vec::new(),
        existing_item_vnums: Vec::new(),
    };
    let (plan, warnings) = mapping::ir_to_plan(&ir, &opts);

    let post_office_overlays: Vec<_> = plan
        .trigger_overlays
        .iter()
        .filter(|ov| matches!(&ov.mutation, ironmud::import::TriggerMutation::SetRoomFlag { .. }))
        .collect();
    assert!(
        post_office_overlays.is_empty(),
        "no spawn → no overlay: {:?}",
        post_office_overlays.iter().map(|o| &o.target_vnum).collect::<Vec<_>>()
    );
    let warns: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains("postmaster") && w.message.contains("no zone reset"))
        .collect();
    assert_eq!(
        warns.len(),
        1,
        "postmaster with no spawn → one explanatory warn line: all warnings = {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );

    let _ = std::fs::remove_dir_all(&dir);
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
            trigger_vnums: Vec::new(),
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
        dg_triggers: Vec::new(),
        quests: Vec::new(),
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
            trigger_vnums: Vec::new(),
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
        dg_triggers: Vec::new(),
        quests: Vec::new(),
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
                trigger_vnums: Vec::new(),
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
                trigger_vnums: Vec::new(),
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
        dg_triggers: Vec::new(),
        quests: Vec::new(),
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

use anyhow::Result;
use uuid::Uuid;

use crate::db::Db;
use crate::types::*;

/// Create a base item prototype with all defaults set.
/// Callers then modify only the fields they need.
fn item(id: Uuid, vnum: &str, name: &str, short_desc: &str, long_desc: &str, item_type: ItemType) -> ItemData {
    ItemData {
        id,
        name: name.to_string(),
        short_desc: short_desc.to_string(),
        long_desc: long_desc.to_string(),
        keywords: name.to_lowercase().split_whitespace().map(|s| s.to_string()).collect(),
        item_type,
        categories: Vec::new(),
        teaches_recipe: None,
        teaches_spell: None,
        wear_locations: Vec::new(),
        armor_class: None,
        protects: Vec::new(),
        holes: 0,
        flags: ItemFlags::default(),
        weight: 0,
        value: 0,
        location: ItemLocation::Nowhere,
        // Weapon fields
        damage_dice_count: 0,
        damage_dice_sides: 0,
        damage_type: DamageType::Bludgeoning,
        two_handed: false,
        weapon_skill: None,
        // Container fields
        container_contents: Vec::new(),
        container_max_items: 0,
        container_max_weight: 0,
        container_closed: false,
        container_locked: false,
        container_key_id: None,
        weight_reduction: 0,
        // Liquid container fields
        liquid_type: LiquidType::Water,
        liquid_current: 0,
        liquid_max: 0,
        liquid_poisoned: false,
        liquid_effects: Vec::new(),
        // Food fields
        food_nutrition: 0,
        food_poisoned: false,
        food_spoil_duration: 0,
        food_created_at: None,
        food_effects: Vec::new(),
        food_spoilage_points: 0.0,
        preservation_level: 0,
        // Stat bonuses
        level_requirement: 0,
        stat_str: 0,
        stat_dex: 0,
        stat_con: 0,
        stat_int: 0,
        stat_wis: 0,
        stat_cha: 0,
        insulation: 0,
        // Prototype
        is_prototype: true,
        vnum: Some(vnum.to_string()),
        // Triggers
        triggers: Vec::new(),
        // Vending
        vending_stock: Vec::new(),
        vending_sell_rate: 150,
        // Quality / bait
        quality: 0,
        bait_uses: 0,
        // Medical
        medical_tier: 0,
        medical_uses: 0,
        treats_wound_types: Vec::new(),
        max_treatable_wound: String::new(),
        // Transport
        transport_link: None,
        // Ammunition
        caliber: None,
        ammo_count: 0,
        ammo_damage_bonus: 0,
        // Ranged weapon
        ranged_type: None,
        magazine_size: 0,
        loaded_ammo: 0,
        loaded_ammo_bonus: 0,
        loaded_ammo_vnum: None,
        fire_mode: String::new(),
        supported_fire_modes: Vec::new(),
        noise_level: String::new(),
        // Ammo effects
        ammo_effect_type: String::new(),
        ammo_effect_duration: 0,
        ammo_effect_damage: 0,
        loaded_ammo_effect_type: String::new(),
        loaded_ammo_effect_duration: 0,
        loaded_ammo_effect_damage: 0,
        // Attachment
        attachment_slot: String::new(),
        attachment_accuracy_bonus: 0,
        attachment_noise_reduction: 0,
        attachment_magazine_bonus: 0,
        attachment_compatible_types: Vec::new(),
        // Gardening
        plant_prototype_vnum: String::new(),
        fertilizer_duration: 0,
        treats_infestation: String::new(),
    }
}

pub fn seed_items(db: &Db) -> Result<()> {
    // ── WEAPONS ──────────────────────────────────────────────────────

    // Rusty Sword (1d4 slashing)
    let vnum = "oakvale:rusty_sword";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Rusty Sword",
        "A rusty sword lies here, its edge pitted with neglect.",
        "This old sword has seen better days. Rust blooms across its once-keen blade, and the leather grip is cracked and worn. It might still draw blood, but not much.",
        ItemType::Weapon,
    );
    i.damage_dice_count = 1;
    i.damage_dice_sides = 4;
    i.damage_type = DamageType::Slashing;
    i.weapon_skill = Some(WeaponSkill::LongBlades);
    i.wear_locations = vec![WearLocation::Wielded];
    i.weight = 3;
    i.value = 5;
    db.save_item_data(i)?;

    // Iron Sword (2d4 slashing)
    let vnum = "oakvale:iron_sword";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Iron Sword",
        "A sturdy iron sword rests here, its blade well-oiled.",
        "Forged from solid iron, this longsword bears the marks of a competent blacksmith. The blade holds a decent edge, and the crossguard is shaped like a pair of wings.",
        ItemType::Weapon,
    );
    i.damage_dice_count = 2;
    i.damage_dice_sides = 4;
    i.damage_type = DamageType::Slashing;
    i.weapon_skill = Some(WeaponSkill::LongBlades);
    i.wear_locations = vec![WearLocation::Wielded];
    i.weight = 4;
    i.value = 50;
    db.save_item_data(i)?;

    // Wooden Staff (1d6 bludgeoning, two-handed)
    let vnum = "oakvale:wooden_staff";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Wooden Staff",
        "A tall wooden staff leans against the wall here.",
        "Cut from a sturdy oak branch and smoothed by long use, this staff is both a walking aid and a formidable weapon. Iron bands reinforce each end.",
        ItemType::Weapon,
    );
    i.damage_dice_count = 1;
    i.damage_dice_sides = 6;
    i.damage_type = DamageType::Bludgeoning;
    i.weapon_skill = Some(WeaponSkill::LongBlunt);
    i.two_handed = true;
    i.wear_locations = vec![WearLocation::Wielded];
    i.weight = 3;
    i.value = 10;
    db.save_item_data(i)?;

    // Hunting Bow (1d6 piercing, ranged, two-handed)
    let vnum = "oakvale:hunting_bow";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Hunting Bow",
        "A hunting bow rests here, its string taut.",
        "This short recurve bow is crafted from yew wood and strung with waxed sinew. Simple but reliable, it is the weapon of choice for hunters throughout the valley.",
        ItemType::Weapon,
    );
    i.damage_dice_count = 1;
    i.damage_dice_sides = 6;
    i.damage_type = DamageType::Piercing;
    i.weapon_skill = Some(WeaponSkill::Ranged);
    i.two_handed = true;
    i.wear_locations = vec![WearLocation::Wielded];
    i.weight = 2;
    i.value = 30;
    db.save_item_data(i)?;

    // Dagger (1d4 piercing)
    let vnum = "oakvale:dagger";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Dagger",
        "A small dagger gleams on the ground here.",
        "A slim-bladed dagger with a leather-wrapped grip. The double-edged blade comes to a needle-sharp point, ideal for quick thrusts in close combat.",
        ItemType::Weapon,
    );
    i.damage_dice_count = 1;
    i.damage_dice_sides = 4;
    i.damage_type = DamageType::Piercing;
    i.weapon_skill = Some(WeaponSkill::ShortBlades);
    i.wear_locations = vec![WearLocation::Wielded];
    i.weight = 1;
    i.value = 15;
    db.save_item_data(i)?;

    // Iron Mace (2d3 bludgeoning)
    let vnum = "oakvale:iron_mace";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Iron Mace",
        "A heavy iron mace lies here.",
        "This brutal weapon features a flanged iron head mounted on a hardwood shaft. Each of the six flanges is designed to concentrate force through armor. The leather grip is stained dark with sweat.",
        ItemType::Weapon,
    );
    i.damage_dice_count = 2;
    i.damage_dice_sides = 3;
    i.damage_type = DamageType::Bludgeoning;
    i.weapon_skill = Some(WeaponSkill::ShortBlunt);
    i.wear_locations = vec![WearLocation::Wielded];
    i.weight = 4;
    i.value = 40;
    db.save_item_data(i)?;

    // Shadow Blade (3d4 slashing, glow, +1 str)
    let vnum = "shadowfang:shadow_blade";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Shadow Blade",
        "A blade of living shadow writhes on the ground, casting an eerie glow.",
        "This longsword seems forged from solidified darkness. Tendrils of shadow curl lazily along its edge, and it pulses with a faint, malevolent light. The grip is wrapped in black dragonhide that molds itself to the wielder's hand.",
        ItemType::Weapon,
    );
    i.damage_dice_count = 3;
    i.damage_dice_sides = 4;
    i.damage_type = DamageType::Slashing;
    i.weapon_skill = Some(WeaponSkill::LongBlades);
    i.wear_locations = vec![WearLocation::Wielded];
    i.weight = 3;
    i.value = 200;
    i.flags.glow = true;
    i.stat_str = 1;
    db.save_item_data(i)?;

    // Pitchfork (1d4 piercing, polearm, two-handed)
    let vnum = "hilltop:pitchfork";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Pitchfork",
        "A farmer's pitchfork has been left here.",
        "A three-tined iron pitchfork mounted on a long ash handle. Intended for moving hay, it could serve as a makeshift weapon in desperate times.",
        ItemType::Weapon,
    );
    i.damage_dice_count = 1;
    i.damage_dice_sides = 4;
    i.damage_type = DamageType::Piercing;
    i.weapon_skill = Some(WeaponSkill::Polearms);
    i.two_handed = true;
    i.wear_locations = vec![WearLocation::Wielded];
    i.weight = 3;
    i.value = 5;
    db.save_item_data(i)?;

    // ── ARMOR ────────────────────────────────────────────────────────

    // Leather Armor (AC 2, torso)
    let vnum = "oakvale:leather_armor";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Leather Armor",
        "A suit of leather armor lies in a heap here.",
        "Crafted from thick, boiled cowhide, this armor has been shaped to cover the torso. Bronze rivets reinforce the joints, and the interior is lined with soft linen for comfort.",
        ItemType::Armor,
    );
    i.armor_class = Some(2);
    i.wear_locations = vec![WearLocation::Torso];
    i.protects = vec![BodyPart::Torso];
    i.weight = 5;
    i.value = 30;
    db.save_item_data(i)?;

    // Iron Helm (AC 1, head)
    let vnum = "oakvale:iron_helm";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Iron Helm",
        "An iron helm sits here, its surface dull and scratched.",
        "A simple open-faced iron helmet with a nasal guard. The interior is padded with quilted wool. Functional rather than decorative, it offers solid protection for the head.",
        ItemType::Armor,
    );
    i.armor_class = Some(1);
    i.wear_locations = vec![WearLocation::Head];
    i.protects = vec![BodyPart::Head];
    i.weight = 3;
    i.value = 20;
    db.save_item_data(i)?;

    // Leather Boots (AC 1, feet)
    let vnum = "oakvale:leather_boots";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Leather Boots",
        "A pair of leather boots stands here.",
        "Sturdy boots made from thick, tanned leather with hardened soles. They lace up past the ankle and offer good footing on rough terrain.",
        ItemType::Armor,
    );
    i.armor_class = Some(1);
    i.wear_locations = vec![WearLocation::Feet];
    i.protects = vec![BodyPart::LeftFoot, BodyPart::RightFoot];
    i.weight = 2;
    i.value = 15;
    db.save_item_data(i)?;

    // Chain Mail (AC 4, torso)
    let vnum = "ironkeep:chain_mail";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Chain Mail",
        "A shirt of interlocking chain mail lies here in a heavy heap.",
        "Thousands of riveted iron rings interlock to form this knee-length mail shirt. It rattles softly with every movement but provides excellent protection against slashing weapons.",
        ItemType::Armor,
    );
    i.armor_class = Some(4);
    i.wear_locations = vec![WearLocation::Torso];
    i.protects = vec![BodyPart::Torso];
    i.weight = 8;
    i.value = 100;
    db.save_item_data(i)?;

    // Iron Shield (AC 2, off-hand)
    let vnum = "ironkeep:iron_shield";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Iron Shield",
        "A round iron shield lies propped against the wall.",
        "A round shield faced with riveted iron plates over a wooden core. A leather strap and iron grip allow it to be strapped firmly to the forearm. Dents and scratches speak of battles survived.",
        ItemType::Armor,
    );
    i.armor_class = Some(2);
    i.wear_locations = vec![WearLocation::OffHand];
    i.weight = 5;
    i.value = 35;
    db.save_item_data(i)?;

    // Dragon Scale Armor (AC 5, torso, +1 con)
    let vnum = "shadowfang:dragon_scale";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Dragon Scale Armor",
        "A suit of shimmering dragon scale armor lies here, each scale catching the light.",
        "This extraordinary armor is assembled from the scales of a black dragon, each one the size of a man's palm. The scales overlap like roof tiles, creating a surface that seems to drink in light. Despite the dragon's size, the armor is surprisingly light, and warmth radiates from within.",
        ItemType::Armor,
    );
    i.armor_class = Some(5);
    i.wear_locations = vec![WearLocation::Torso];
    i.protects = vec![BodyPart::Torso];
    i.weight = 6;
    i.value = 300;
    i.stat_con = 1;
    db.save_item_data(i)?;

    // ── KEYS ─────────────────────────────────────────────────────────

    // Iron Gate Key
    let vnum = "oakvale:gate_key";
    let i = item(
        super::seed_uuid(vnum), vnum,
        "Iron Gate Key",
        "A large iron key lies here.",
        "A heavy iron key with a simple ward pattern. It looks like it might fit the lock on a large gate.",
        ItemType::Key,
    );
    db.save_item_data(i)?;

    // Treasure Alcove Key
    let vnum = "shadowfang:treasure_key";
    let i = item(
        super::seed_uuid(vnum), vnum,
        "Treasure Alcove Key",
        "A small, ornate key lies here, glinting darkly.",
        "This small key is fashioned from black iron and set with a tiny garnet in its bow. Strange runes are etched along its shaft.",
        ItemType::Key,
    );
    db.save_item_data(i)?;

    // ── FOOD & DRINK ─────────────────────────────────────────────────

    // Loaf of Bread
    let vnum = "oakvale:bread";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Loaf of Bread",
        "A crusty loaf of bread sits here.",
        "A round loaf of rustic bread with a golden-brown crust. The interior is soft and still faintly warm, filling the air with a yeasty aroma.",
        ItemType::Food,
    );
    i.food_nutrition = 30;
    i.weight = 1;
    i.value = 3;
    db.save_item_data(i)?;

    // Roast Chicken
    let vnum = "oakvale:roast_chicken";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Roast Chicken",
        "A roast chicken glistens with juices here.",
        "A whole chicken roasted to a perfect golden brown, its skin crispy and lacquered with drippings. The aroma of herbs and garlic wafts from the steaming bird.",
        ItemType::Food,
    );
    i.food_nutrition = 60;
    i.weight = 2;
    i.value = 8;
    i.categories = vec!["meat".to_string()];
    db.save_item_data(i)?;

    // Mug of Ale
    let vnum = "oakvale:ale";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Mug of Ale",
        "A frothy mug of ale sits here.",
        "A thick ceramic mug brimming with dark amber ale. A generous head of foam crowns the brew, and the rich scent of hops and malt fills the air.",
        ItemType::LiquidContainer,
    );
    i.liquid_type = LiquidType::Ale;
    i.liquid_max = 3;
    i.liquid_current = 3;
    i.weight = 1;
    i.value = 5;
    i.liquid_effects = vec![ItemEffect {
        effect_type: EffectType::Drunk,
        magnitude: 10,
        duration: 0,
        script_callback: None,
    }];
    db.save_item_data(i)?;

    // Healing Potion
    let vnum = "oakvale:healing_potion";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Healing Potion",
        "A small vial of glowing red liquid sits here.",
        "A glass vial stoppered with wax, containing a luminous crimson liquid that swirls gently of its own accord. The potion radiates a faint warmth through the glass.",
        ItemType::LiquidContainer,
    );
    i.liquid_type = LiquidType::HealingPotion;
    i.liquid_max = 1;
    i.liquid_current = 1;
    i.weight = 1;
    i.value = 25;
    i.liquid_effects = vec![ItemEffect {
        effect_type: EffectType::Heal,
        magnitude: 30,
        duration: 0,
        script_callback: None,
    }];
    db.save_item_data(i)?;

    // Mana Potion
    let vnum = "oakvale:mana_potion";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Mana Potion",
        "A small vial of shimmering blue liquid sits here.",
        "A glass vial containing a deep azure liquid that sparkles with tiny motes of light, like stars reflected in a midnight pool. A faint hum emanates from within.",
        ItemType::LiquidContainer,
    );
    i.liquid_type = LiquidType::ManaPotion;
    i.liquid_max = 1;
    i.liquid_current = 1;
    i.weight = 1;
    i.value = 25;
    i.liquid_effects = vec![ItemEffect {
        effect_type: EffectType::ManaRestore,
        magnitude: 30,
        duration: 0,
        script_callback: None,
    }];
    db.save_item_data(i)?;

    // Waterskin
    let vnum = "oakvale:waterskin";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Waterskin",
        "A leather waterskin lies here.",
        "A simple waterskin made from goat leather, sealed with pitch at the seams. A wooden stopper dangles from a leather cord. It sloshes when moved.",
        ItemType::LiquidContainer,
    );
    i.liquid_type = LiquidType::Water;
    i.liquid_max = 5;
    i.liquid_current = 5;
    i.weight = 1;
    i.value = 10;
    db.save_item_data(i)?;

    // Apple
    let vnum = "oakvale:apple";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Apple",
        "A crisp red apple sits here.",
        "A firm, round apple with a deep red skin streaked with gold. It smells sweet and fresh, clearly just picked from the tree.",
        ItemType::Food,
    );
    i.food_nutrition = 15;
    i.weight = 1;
    i.value = 2;
    i.categories = vec!["fruit".to_string()];
    db.save_item_data(i)?;

    // Hearty Stew
    let vnum = "oakvale:stew";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Hearty Stew",
        "A steaming bowl of hearty stew sits here.",
        "A thick, rich stew fills this wooden bowl, packed with chunks of root vegetables, tender meat, and fragrant herbs. Steam rises in lazy curls, carrying the mouthwatering aroma of a slow-cooked meal.",
        ItemType::Food,
    );
    i.food_nutrition = 50;
    i.weight = 1;
    i.value = 12;
    i.categories = vec!["cooked".to_string()];
    db.save_item_data(i)?;

    // ── FISH ─────────────────────────────────────────────────────────

    // Fresh Trout
    let vnum = "oakvale:trout";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Fresh Trout",
        "A fresh trout lies here, its scales glistening.",
        "A speckled brown trout with silvery flanks, freshly caught from the river. Its scales still glisten with moisture, and it smells of clean water.",
        ItemType::Food,
    );
    i.food_nutrition = 25;
    i.weight = 1;
    i.value = 5;
    i.categories = vec!["fish".to_string()];
    db.save_item_data(i)?;

    // Largemouth Bass
    let vnum = "oakvale:bass";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Largemouth Bass",
        "A largemouth bass lies here.",
        "A plump largemouth bass with a distinctive dark lateral stripe along its olive-green body. This one is a decent size, enough for a solid meal.",
        ItemType::Food,
    );
    i.food_nutrition = 30;
    i.weight = 2;
    i.value = 8;
    i.categories = vec!["fish".to_string()];
    db.save_item_data(i)?;

    // Crystal Fish
    let vnum = "shadowfang:crystal_fish";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Crystal Fish",
        "A translucent fish shimmers with an inner light here.",
        "This strange fish is almost entirely transparent, its crystalline flesh revealing delicate bones that glow with a soft blue luminescence. It is found only in the deep underground pools of Shadowfang Keep.",
        ItemType::Food,
    );
    i.food_nutrition = 40;
    i.weight = 1;
    i.value = 50;
    i.categories = vec!["fish".to_string()];
    i.food_effects = vec![ItemEffect {
        effect_type: EffectType::ManaRestore,
        magnitude: 15,
        duration: 0,
        script_callback: None,
    }];
    db.save_item_data(i)?;

    // ── GARDENING ────────────────────────────────────────────────────

    // Tomato Seeds
    let vnum = "oakvale:tomato_seeds";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Tomato Seeds",
        "A small packet of tomato seeds lies here.",
        "A handful of tiny, flat seeds wrapped in a scrap of cloth. Each seed is pale yellow and slightly fuzzy, ready to be planted in fertile soil.",
        ItemType::Misc,
    );
    i.weight = 0;
    i.value = 3;
    i.plant_prototype_vnum = "plants:tomato".to_string();
    db.save_item_data(i)?;

    // Herb Seeds
    let vnum = "oakvale:herb_seeds";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Herb Seeds",
        "A small packet of herb seeds lies here.",
        "An assortment of tiny seeds in various shapes and colors, bundled together in a twist of parchment. A faint herbal fragrance clings to the packet.",
        ItemType::Misc,
    );
    i.weight = 0;
    i.value = 5;
    i.plant_prototype_vnum = "plants:herb".to_string();
    db.save_item_data(i)?;

    // Ripe Tomato
    let vnum = "oakvale:tomato";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Ripe Tomato",
        "A plump, ripe tomato sits here.",
        "A perfectly ripe tomato with smooth, deep red skin that yields slightly to the touch. The vine-fresh scent promises a burst of tangy sweetness.",
        ItemType::Food,
    );
    i.food_nutrition = 10;
    i.weight = 1;
    i.value = 4;
    i.categories = vec!["vegetable".to_string()];
    db.save_item_data(i)?;

    // Fresh Herb
    let vnum = "oakvale:herb";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Fresh Herb",
        "A sprig of fresh herbs lies here, filling the air with fragrance.",
        "A bundle of aromatic green herbs tied with a bit of twine. The leaves are bright and pungent, useful in cooking or simple remedies.",
        ItemType::Food,
    );
    i.food_nutrition = 5;
    i.weight = 0;
    i.value = 6;
    i.categories = vec!["herb".to_string()];
    db.save_item_data(i)?;

    // ── CRAFTING MATERIALS ───────────────────────────────────────────

    // Bag of Flour
    let vnum = "oakvale:flour";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Bag of Flour",
        "A small cloth bag of flour sits here.",
        "A tightly-woven linen bag filled with finely ground wheat flour. A dusting of white powder clings to the outside of the bag.",
        ItemType::Misc,
    );
    i.weight = 2;
    i.value = 5;
    i.categories = vec!["flour".to_string(), "ingredient".to_string()];
    db.save_item_data(i)?;

    // Piece of Leather
    let vnum = "oakvale:leather";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Piece of Leather",
        "A piece of tanned leather lies here.",
        "A square of thick, supple leather that has been properly tanned and cured. The surface is smooth on one side and slightly rough on the other, ready for crafting.",
        ItemType::Misc,
    );
    i.weight = 1;
    i.value = 8;
    i.categories = vec!["leather".to_string(), "ingredient".to_string()];
    db.save_item_data(i)?;

    // Bundle of Wood
    let vnum = "oakvale:wood";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Bundle of Wood",
        "A bundle of wood is stacked here.",
        "Several straight branches and split logs bound together with rough cord. The wood is dry and well-seasoned, suitable for building or fuel.",
        ItemType::Misc,
    );
    i.weight = 3;
    i.value = 3;
    i.categories = vec!["wood".to_string(), "ingredient".to_string()];
    db.save_item_data(i)?;

    // ── GENERAL ITEMS ────────────────────────────────────────────────

    // Torch
    let vnum = "oakvale:torch";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Torch",
        "A torch flickers with a warm, steady flame here.",
        "A sturdy wooden brand wrapped in oil-soaked rags at one end. The flame dances and crackles, casting warm light and long shadows.",
        ItemType::Misc,
    );
    i.weight = 1;
    i.value = 2;
    i.flags.provides_light = true;
    db.save_item_data(i)?;

    // Leather Backpack
    let vnum = "oakvale:backpack";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Leather Backpack",
        "A worn leather backpack lies here.",
        "A spacious backpack crafted from thick brown leather, with adjustable shoulder straps and a sturdy brass buckle. Multiple compartments provide ample storage for the discerning adventurer.",
        ItemType::Container,
    );
    i.container_max_items = 20;
    i.container_max_weight = 50;
    i.wear_locations = vec![WearLocation::Back];
    i.weight = 2;
    i.value = 15;
    db.save_item_data(i)?;

    // Coil of Rope
    let vnum = "oakvale:rope";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Coil of Rope",
        "A coil of sturdy rope lies here.",
        "A fifty-foot length of braided hemp rope, tightly coiled. It is thick and strong, useful for climbing, binding, or any number of practical tasks.",
        ItemType::Misc,
    );
    i.weight = 2;
    i.value = 5;
    db.save_item_data(i)?;

    // Linen Bandage
    let vnum = "oakvale:bandage";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Linen Bandage",
        "A roll of clean linen bandage lies here.",
        "A tightly rolled strip of bleached linen, clean and ready for use. It can be applied to wounds to staunch bleeding and promote healing.",
        ItemType::Misc,
    );
    i.weight = 0;
    i.value = 3;
    i.flags.medical_tool = true;
    i.medical_tier = 1;
    i.medical_uses = 1;
    i.treats_wound_types = vec!["laceration".to_string(), "abrasion".to_string()];
    i.max_treatable_wound = "moderate".to_string();
    db.save_item_data(i)?;

    // Fishing Rod
    let vnum = "oakvale:fishing_rod";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Fishing Rod",
        "A fishing rod leans against a nearby surface.",
        "A simple fishing rod fashioned from a flexible bamboo pole. A length of catgut line is attached to the tip, ending in a small iron hook. Just add bait and patience.",
        ItemType::Misc,
    );
    i.weight = 2;
    i.value = 10;
    i.flags.fishing_rod = true;
    db.save_item_data(i)?;

    // ── DUNGEON SPECIFIC ─────────────────────────────────────────────

    // Glowing Mushroom
    let vnum = "shadowfang:mushroom";
    let mut i = item(
        super::seed_uuid(vnum), vnum,
        "Glowing Mushroom",
        "A softly glowing mushroom sprouts from the damp stone here.",
        "This pale, bulbous mushroom emits a faint bioluminescent glow from its cap. Found deep underground where no sunlight reaches, it has a slightly bitter taste but is safe to eat. Alchemists prize them as ingredients.",
        ItemType::Food,
    );
    i.food_nutrition = 10;
    i.weight = 0;
    i.value = 15;
    i.categories = vec!["mushroom".to_string(), "ingredient".to_string()];
    db.save_item_data(i)?;

    Ok(())
}

use std::collections::HashMap;

use anyhow::Result;
use uuid::Uuid;

use crate::db::Db;
use crate::types::*;

use super::seed_uuid;

/// Create a base mobile prototype with all defaults set.
/// Callers then modify only the fields they need.
fn mobile(id: Uuid, vnum: &str, name: &str, short_desc: &str, long_desc: &str) -> MobileData {
    MobileData {
        id,
        name: name.to_string(),
        short_desc: short_desc.to_string(),
        long_desc: long_desc.to_string(),
        keywords: name
            .to_lowercase()
            .split_whitespace()
            .filter(|w| !["a", "an", "the"].contains(w))
            .map(|s| s.to_string())
            .collect(),
        current_room_id: None,
        is_prototype: true,
        vnum: vnum.to_string(),
        level: 1,
        max_hp: 10,
        current_hp: 10,
        max_stamina: 50,
        current_stamina: 50,
        damage_dice: "1d4".to_string(),
        damage_type: DamageType::default(),
        armor_class: 10,
        hit_modifier: 0,
        gold: 0,
        stat_str: 10,
        stat_dex: 10,
        stat_con: 10,
        stat_int: 10,
        stat_wis: 10,
        stat_cha: 10,
        flags: MobileFlags::default(),
        dialogue: HashMap::new(),
        shop_stock: Vec::new(),
        shop_inventory: Vec::new(),
        shop_buy_rate: 50,
        shop_sell_rate: 150,
        shop_buys_types: vec!["all".to_string()],
        shop_buys_categories: Vec::new(),
        shop_preset_vnum: String::new(),
        shop_extra_types: Vec::new(),
        shop_extra_categories: Vec::new(),
        shop_deny_types: Vec::new(),
        shop_deny_categories: Vec::new(),
        shop_min_value: 0,
        shop_max_value: 0,
        healer_type: String::new(),
        healing_free: false,
        healing_cost_multiplier: 100,
        triggers: Vec::new(),
        transport_route: None,
        property_templates: Vec::new(),
        leasing_area_id: None,
        combat: CombatState::default(),
        wounds: Vec::new(),
        ongoing_effects: Vec::new(),
        scars: HashMap::new(),
        is_unconscious: false,
        bleedout_rounds_remaining: 0,
        pursuit_target_name: String::new(),
        pursuit_target_room: None,
        pursuit_direction: String::new(),
        pursuit_certain: false,
        embedded_projectiles: Vec::new(),
        daily_routine: Vec::new(),
        schedule_visible: false,
        current_activity: ActivityState::default(),
        routine_destination_room: None,
        perception: 0,
        simulation: None,
        needs: None,
        characteristics: None,
        household_id: None,
        relationships: Vec::new(),
        resident_of: None,
        social: None,
        active_buffs: Vec::new(),
        adoption_pending: false,
    }
}

pub fn seed_mobiles(db: &Db) -> Result<()> {
    let mut mobiles = Vec::new();

    // ── Shopkeepers ──────────────────────────────────────────────

    // Blacksmith — weapons shop in the smithy
    let mut blacksmith = mobile(
        seed_uuid("mob:blacksmith"),
        "oakvale:blacksmith",
        "Grimjaw the Blacksmith",
        "Grimjaw the Blacksmith wipes soot from his hands, eyeing you appraisingly.",
        "A burly man with arms like tree trunks and a face weathered by years of forge work. \
         His leather apron is scorched in places, and his calloused hands never stray far from \
         his hammer. Despite his gruff appearance, his eyes hold a craftsman's pride.",
    );
    blacksmith.level = 8;
    blacksmith.max_hp = 80;
    blacksmith.current_hp = 80;
    blacksmith.flags.shopkeeper = true;
    blacksmith.flags.sentinel = true;
    blacksmith.flags.no_attack = true;
    blacksmith.shop_stock = vec![
        "oakvale:rusty_sword".to_string(),
        "oakvale:iron_sword".to_string(),
        "oakvale:dagger".to_string(),
        "oakvale:iron_mace".to_string(),
        "oakvale:wooden_staff".to_string(),
    ];
    blacksmith.shop_buys_types = vec!["weapon".to_string()];
    blacksmith.dialogue.insert(
        "hello".to_string(),
        "Well met, traveler! Looking for a fine blade? I forge the best steel in Oakvale.".to_string(),
    );
    blacksmith.dialogue.insert(
        "work".to_string(),
        "Aye, the forge never rests. Iron Keep's knights keep me busy with orders.".to_string(),
    );
    blacksmith.daily_routine = vec![
        RoutineEntry {
            start_hour: 7,
            activity: ActivityState::Working,
            destination_vnum: Some("oakvale:smithy".to_string()),
            transition_message: Some("opens the smithy shutters and stokes the forge.".to_string()),
            suppress_wander: true,
            dialogue_overrides: HashMap::new(),
        },
        RoutineEntry {
            start_hour: 12,
            activity: ActivityState::Eating,
            destination_vnum: Some("oakvale:tavern".to_string()),
            transition_message: Some("sets down his hammer and heads to the tavern for a meal.".to_string()),
            suppress_wander: true,
            dialogue_overrides: HashMap::new(),
        },
        RoutineEntry {
            start_hour: 13,
            activity: ActivityState::Working,
            destination_vnum: Some("oakvale:smithy".to_string()),
            transition_message: Some("returns to the forge after his meal.".to_string()),
            suppress_wander: true,
            dialogue_overrides: HashMap::new(),
        },
        RoutineEntry {
            start_hour: 20,
            activity: ActivityState::Sleeping,
            destination_vnum: Some("oakvale:tavern_upstairs".to_string()),
            transition_message: Some("banks the forge and retires for the night.".to_string()),
            suppress_wander: true,
            dialogue_overrides: HashMap::from([("hello".to_string(), "*snores loudly*".to_string())]),
        },
    ];
    blacksmith.schedule_visible = true;
    mobiles.push(blacksmith);

    // Merchant — general store
    let mut merchant = mobile(
        seed_uuid("mob:merchant"),
        "oakvale:merchant",
        "Elara the Merchant",
        "Elara the Merchant arranges her wares with practiced efficiency.",
        "A sharp-eyed woman in a colorful shawl, Elara seems to know the value of everything \
         and the price of nothing — at least not in your favor. Her shelves are stacked with \
         goods from across the realm, and she drives a hard but fair bargain.",
    );
    merchant.level = 5;
    merchant.max_hp = 50;
    merchant.current_hp = 50;
    merchant.flags.shopkeeper = true;
    merchant.flags.sentinel = true;
    merchant.flags.no_attack = true;
    merchant.shop_stock = vec![
        "oakvale:torch".to_string(),
        "oakvale:backpack".to_string(),
        "oakvale:rope".to_string(),
        "oakvale:bandage".to_string(),
        "oakvale:waterskin".to_string(),
        "oakvale:fishing_rod".to_string(),
    ];
    merchant.shop_buys_types = vec!["all".to_string()];
    merchant.dialogue.insert(
        "hello".to_string(),
        "Welcome, welcome! Browse to your heart's content. I have supplies for every adventure.".to_string(),
    );
    merchant.dialogue.insert(
        "work".to_string(),
        "Goods don't sell themselves! Though I must say, business has been brisk since the caves opened up."
            .to_string(),
    );
    merchant.daily_routine = vec![
        RoutineEntry {
            start_hour: 8,
            activity: ActivityState::Working,
            destination_vnum: Some("oakvale:general_store".to_string()),
            transition_message: Some("unlocks the shop door and flips the sign to OPEN.".to_string()),
            suppress_wander: true,
            dialogue_overrides: HashMap::new(),
        },
        RoutineEntry {
            start_hour: 20,
            activity: ActivityState::Sleeping,
            destination_vnum: Some("oakvale:tavern_upstairs".to_string()),
            transition_message: Some("locks up the shop and heads home for the evening.".to_string()),
            suppress_wander: true,
            dialogue_overrides: HashMap::from([(
                "hello".to_string(),
                "*mumbles in her sleep about inventory*".to_string(),
            )]),
        },
    ];
    merchant.schedule_visible = true;
    mobiles.push(merchant);

    // Armorer — armor shop at Iron Keep
    let mut armorer = mobile(
        seed_uuid("mob:armorer"),
        "ironkeep:armorer",
        "Ser Aldric the Armorer",
        "Ser Aldric the Armorer polishes a suit of chainmail with meticulous care.",
        "A retired knight who traded his sword for a workbench, Ser Aldric brings a soldier's \
         eye to his craft. Every piece of armor he sells has been tested, fitted, and blessed — \
         or so he claims. His workshop is lined with shields, helms, and suits of mail.",
    );
    armorer.level = 10;
    armorer.max_hp = 100;
    armorer.current_hp = 100;
    armorer.flags.shopkeeper = true;
    armorer.flags.sentinel = true;
    armorer.flags.no_attack = true;
    armorer.shop_stock = vec![
        "oakvale:leather_armor".to_string(),
        "oakvale:iron_helm".to_string(),
        "oakvale:leather_boots".to_string(),
        "oakvale:chain_mail".to_string(),
        "oakvale:iron_shield".to_string(),
    ];
    armorer.shop_buys_types = vec!["armor".to_string()];
    armorer.dialogue.insert(
        "hello".to_string(),
        "Looking for protection? A wise adventurer values armor over a sharp blade.".to_string(),
    );
    armorer.dialogue.insert("knight".to_string(), "Aye, I served twenty years with the Iron Keep garrison. These old bones have seen more battles than I care to remember.".to_string());
    armorer.daily_routine = vec![
        RoutineEntry {
            start_hour: 6,
            activity: ActivityState::Working,
            destination_vnum: Some("ironkeep:armory".to_string()),
            transition_message: Some("opens the armory for the day.".to_string()),
            suppress_wander: true,
            dialogue_overrides: HashMap::new(),
        },
        RoutineEntry {
            start_hour: 22,
            activity: ActivityState::Sleeping,
            destination_vnum: Some("ironkeep:barracks".to_string()),
            transition_message: Some("locks the armory and retires to the barracks.".to_string()),
            suppress_wander: true,
            dialogue_overrides: HashMap::from([(
                "hello".to_string(),
                "*snores, muttering about plate mail*".to_string(),
            )]),
        },
    ];
    armorer.schedule_visible = true;
    mobiles.push(armorer);

    // ── Service NPCs ─────────────────────────────────────────────

    // Priestess — healer at the temple
    let mut priestess = mobile(
        seed_uuid("mob:priestess"),
        "oakvale:priestess",
        "Sister Maren",
        "Sister Maren kneels in quiet prayer, a soft golden glow surrounding her hands.",
        "A serene woman in white robes with golden trim, Sister Maren serves as the village's \
         healer and spiritual guide. Her gentle voice and warm smile put even the most grievously \
         wounded at ease. A holy symbol hangs from a delicate chain around her neck.",
    );
    priestess.level = 10;
    priestess.max_hp = 60;
    priestess.current_hp = 60;
    priestess.flags.healer = true;
    priestess.flags.sentinel = true;
    priestess.flags.no_attack = true;
    priestess.healer_type = "cleric".to_string();
    priestess.healing_free = false;
    priestess.healing_cost_multiplier = 80; // Slightly cheaper than default
    priestess.dialogue.insert(
        "hello".to_string(),
        "Blessings upon you, child. Do you seek healing? Simply ask and I shall tend your wounds.".to_string(),
    );
    priestess.dialogue.insert(
        "temple".to_string(),
        "This temple has stood for three hundred years, a beacon of light in times of darkness.".to_string(),
    );
    priestess.dialogue.insert(
        "heal".to_string(),
        "I can mend wounds of body and spirit. Say 'heal' to receive my ministrations.".to_string(),
    );
    mobiles.push(priestess);

    // Barkeeper — food and drink at the tavern
    let mut barkeeper = mobile(
        seed_uuid("mob:barkeeper"),
        "oakvale:barkeeper",
        "Old Torvald",
        "Old Torvald polishes a tankard behind the bar, whistling a jaunty tune.",
        "A rotund man with a magnificent grey mustache and twinkling eyes, Old Torvald has run \
         the Rusty Tankard for longer than most villagers have been alive. He knows everyone's \
         name, everyone's drink, and everyone's secrets — though he'll never tell.",
    );
    barkeeper.level = 5;
    barkeeper.max_hp = 50;
    barkeeper.current_hp = 50;
    barkeeper.flags.shopkeeper = true;
    barkeeper.flags.sentinel = true;
    barkeeper.flags.no_attack = true;
    barkeeper.shop_stock = vec![
        "oakvale:bread".to_string(),
        "oakvale:roast_chicken".to_string(),
        "oakvale:ale".to_string(),
        "oakvale:stew".to_string(),
        "oakvale:apple".to_string(),
    ];
    barkeeper.shop_buys_types = vec!["food".to_string(), "liquid_container".to_string()];
    barkeeper.dialogue.insert(
        "hello".to_string(),
        "Welcome to the Rusty Tankard! Pull up a stool. What'll it be?".to_string(),
    );
    barkeeper.dialogue.insert(
        "rumors".to_string(),
        "They say strange noises echo from the caves beneath Iron Keep. Goblins, most like. Or worse.".to_string(),
    );
    barkeeper.dialogue.insert(
        "ale".to_string(),
        "Best ale in the valley! Brewed right here with Hilltop barley and Whisperwood hops.".to_string(),
    );
    barkeeper.daily_routine = vec![
        RoutineEntry {
            start_hour: 10,
            activity: ActivityState::Working,
            destination_vnum: Some("oakvale:tavern".to_string()),
            transition_message: Some("opens the tavern doors and lights the hearth.".to_string()),
            suppress_wander: true,
            dialogue_overrides: HashMap::new(),
        },
        RoutineEntry {
            start_hour: 2,
            activity: ActivityState::Sleeping,
            destination_vnum: Some("oakvale:tavern_upstairs".to_string()),
            transition_message: Some("wipes down the bar one last time and heads upstairs.".to_string()),
            suppress_wander: true,
            dialogue_overrides: HashMap::from([(
                "hello".to_string(),
                "*snores, one hand still clutching a tankard*".to_string(),
            )]),
        },
    ];
    barkeeper.schedule_visible = true;
    mobiles.push(barkeeper);

    // Postmaster
    let mut postmaster = mobile(
        seed_uuid("mob:postmaster"),
        "oakvale:postmaster",
        "Pip the Postmaster",
        "Pip the Postmaster sorts through a stack of letters with nimble fingers.",
        "A wiry halfling with spectacles perched on the end of his nose, Pip takes his duty \
         as postmaster with utmost seriousness. Stacks of parcels and scrolls surround his \
         tiny desk, each meticulously labeled and sorted.",
    );
    postmaster.level = 3;
    postmaster.max_hp = 30;
    postmaster.current_hp = 30;
    postmaster.flags.sentinel = true;
    postmaster.flags.no_attack = true;
    postmaster.dialogue.insert(
        "hello".to_string(),
        "Ah, a visitor! Need to send a letter? Check your mail? I handle it all!".to_string(),
    );
    postmaster.dialogue.insert(
        "mail".to_string(),
        "Just use the 'mail' command to send letters to anyone in the realm!".to_string(),
    );
    mobiles.push(postmaster);

    // Banker
    let mut banker = mobile(
        seed_uuid("mob:banker"),
        "oakvale:banker",
        "Aldwin the Banker",
        "Aldwin the Banker counts coins behind a reinforced counter.",
        "A thin man in impeccable attire, Aldwin peers at you through a monocle with an \
         expression that suggests he has already calculated your net worth — and found it \
         wanting. The vault behind him is sealed with three different locks.",
    );
    banker.level = 5;
    banker.max_hp = 40;
    banker.current_hp = 40;
    banker.flags.sentinel = true;
    banker.flags.no_attack = true;
    banker.dialogue.insert(
        "hello".to_string(),
        "Good day. The Oakvale Bank is at your service. Use 'deposit' and 'withdraw' to manage your gold.".to_string(),
    );
    banker.dialogue.insert(
        "vault".to_string(),
        "The vault? Triple-locked, magically warded, and guarded by an invisible something. Your gold is quite safe."
            .to_string(),
    );
    mobiles.push(banker);

    // ── Leasing Agent ────────────────────────────────────────────

    let mut agent = mobile(
        seed_uuid("mob:leasing_agent"),
        "oakvale:leasing_agent",
        "Fenwick the Estate Agent",
        "Fenwick the Estate Agent shuffles through a stack of property deeds.",
        "A fussy man in a slightly rumpled suit, Fenwick takes great pride in matching \
         adventurers with their perfect home. He carries a large ring of keys and a \
         well-thumbed ledger of available properties.",
    );
    agent.level = 5;
    agent.max_hp = 40;
    agent.current_hp = 40;
    agent.flags.leasing_agent = true;
    agent.flags.sentinel = true;
    agent.flags.no_attack = true;
    agent.leasing_area_id = Some(seed_uuid("area:oakvale"));
    agent.dialogue.insert("hello".to_string(), "Looking for a place to call home? I have several lovely properties available. Use 'lease list' to see options!".to_string());
    agent.dialogue.insert(
        "property".to_string(),
        "Every adventurer needs a home base! Our cottages come fully furnished with storage space.".to_string(),
    );
    mobiles.push(agent);

    // ── Guards ───────────────────────────────────────────────────

    // Village guard — patrols Oakvale
    let mut guard = mobile(
        seed_uuid("mob:village_guard"),
        "oakvale:village_guard",
        "a village guard",
        "A village guard patrols here, hand resting on the hilt of a sword.",
        "Clad in simple leather armor with the Oakvale crest on the chest, this guard keeps \
         a watchful eye on the village streets. Though not the most formidable warrior, the \
         guards are well-trained and fiercely loyal to the village.",
    );
    guard.level = 5;
    guard.max_hp = 60;
    guard.current_hp = 60;
    guard.damage_dice = "2d4+1".to_string();
    guard.damage_type = DamageType::Slashing;
    guard.armor_class = 7;
    guard.flags.sentinel = false; // Patrols
    guard.flags.no_attack = true;
    guard.dialogue.insert(
        "hello".to_string(),
        "Move along, citizen. All is well in Oakvale.".to_string(),
    );
    guard.dialogue.insert(
        "trouble".to_string(),
        "If you're looking for trouble, try the caves beneath Iron Keep. Plenty of goblins to keep you busy."
            .to_string(),
    );
    guard.daily_routine = vec![
        RoutineEntry {
            start_hour: 6,
            activity: ActivityState::Patrolling,
            destination_vnum: Some("oakvale:north_gate".to_string()),
            transition_message: Some("begins the morning patrol.".to_string()),
            suppress_wander: false,
            dialogue_overrides: HashMap::new(),
        },
        RoutineEntry {
            start_hour: 12,
            activity: ActivityState::Eating,
            destination_vnum: Some("oakvale:tavern".to_string()),
            transition_message: Some("takes a break for a midday meal.".to_string()),
            suppress_wander: true,
            dialogue_overrides: HashMap::new(),
        },
        RoutineEntry {
            start_hour: 13,
            activity: ActivityState::Patrolling,
            destination_vnum: Some("oakvale:south_path".to_string()),
            transition_message: Some("resumes patrol of the village.".to_string()),
            suppress_wander: false,
            dialogue_overrides: HashMap::new(),
        },
        RoutineEntry {
            start_hour: 22,
            activity: ActivityState::Sleeping,
            destination_vnum: Some("oakvale:tavern_upstairs".to_string()),
            transition_message: Some("ends the watch and retires for the night.".to_string()),
            suppress_wander: true,
            dialogue_overrides: HashMap::from([("hello".to_string(), "*snores, still in armor*".to_string())]),
        },
    ];
    guard.schedule_visible = true;
    mobiles.push(guard);

    // Keep knight — stationed at Iron Keep gatehouse
    let mut knight = mobile(
        seed_uuid("mob:keep_knight"),
        "ironkeep:knight",
        "a knight of the Iron Keep",
        "A knight of the Iron Keep stands at attention, armored from head to toe.",
        "Encased in polished plate armor bearing the Iron Keep's sigil — a tower wreathed in \
         iron chains — this knight is the picture of martial discipline. A longsword hangs at \
         the hip, and keen eyes scan every visitor from behind the visor of a full helm.",
    );
    knight.level = 10;
    knight.max_hp = 120;
    knight.current_hp = 120;
    knight.damage_dice = "2d6+3".to_string();
    knight.damage_type = DamageType::Slashing;
    knight.armor_class = 3;
    knight.flags.sentinel = true;
    knight.flags.no_attack = true;
    knight.flags.can_open_doors = true;
    knight.dialogue.insert(
        "hello".to_string(),
        "State your business at Iron Keep, traveler.".to_string(),
    );
    knight.dialogue.insert(
        "gate".to_string(),
        "The gate key is entrusted to the garrison. Only authorized persons may pass.".to_string(),
    );
    knight.dialogue.insert(
        "caves".to_string(),
        "The dungeons below connect to the Shadowfang Caves. Foul creatures lurk within. Enter at your own peril."
            .to_string(),
    );
    mobiles.push(knight);

    // ── Hostile Mobs ─────────────────────────────────────────────

    // Wolf — wilderness enemy
    let mut wolf = mobile(
        seed_uuid("mob:wolf"),
        "whisperwood:wolf",
        "a grey wolf",
        "A grey wolf stalks through the undergrowth, its yellow eyes gleaming.",
        "A lean, powerful predator with a thick grey pelt and bared fangs. This wolf moves \
         with the quiet confidence of an apex predator, muscles coiled and ready to spring. \
         A low growl rumbles in its throat as it watches you.",
    );
    wolf.level = 2;
    wolf.max_hp = 25;
    wolf.current_hp = 25;
    wolf.damage_dice = "1d6+1".to_string();
    wolf.damage_type = DamageType::Bite;
    wolf.armor_class = 8;
    wolf.stat_dex = 14;
    wolf.flags.aggressive = true;
    wolf.flags.cowardly = true; // Flees at low HP
    mobiles.push(wolf);

    // Goblin — cave enemy
    let mut goblin = mobile(
        seed_uuid("mob:goblin"),
        "shadowfang:goblin",
        "a goblin raider",
        "A goblin raider crouches here, clutching a rusty blade.",
        "Small, wiry, and malicious, this goblin wears a patchwork of stolen armor scraps \
         and carries a notched blade that has seen better days. Its beady red eyes dart about \
         nervously, looking for easy prey — or an escape route.",
    );
    goblin.level = 3;
    goblin.max_hp = 30;
    goblin.current_hp = 30;
    goblin.damage_dice = "1d6+2".to_string();
    goblin.damage_type = DamageType::Slashing;
    goblin.armor_class = 8;
    goblin.gold = 5;
    goblin.flags.aggressive = true;
    goblin.flags.scavenger = true;
    mobiles.push(goblin);

    // Cave spider — poison enemy
    let mut spider = mobile(
        seed_uuid("mob:spider"),
        "shadowfang:spider",
        "a cave spider",
        "A massive cave spider clings to the ceiling, dripping venom from its fangs.",
        "Easily the size of a large dog, this cave spider's chitinous body is mottled black \
         and grey. Eight gleaming eyes track your every movement, and glistening strands of \
         webbing stretch between its legs. Drops of translucent venom bead at the tips of \
         its oversized fangs.",
    );
    spider.level = 4;
    spider.max_hp = 40;
    spider.current_hp = 40;
    spider.damage_dice = "1d8+2".to_string();
    spider.damage_type = DamageType::Bite;
    spider.armor_class = 7;
    spider.stat_dex = 14;
    spider.flags.aggressive = true;
    spider.flags.poisonous = true;
    mobiles.push(spider);

    // Shadow Drake — boss mob
    let mut drake = mobile(
        seed_uuid("mob:shadow_drake"),
        "shadowfang:shadow_drake",
        "the Shadow Drake",
        "The Shadow Drake coils in the darkness, tendrils of shadow swirling around its massive form.",
        "An ancient creature of darkness and scale, the Shadow Drake is the undisputed master of \
         the Shadowfang Caves. Its obsidian scales absorb light itself, and wisps of pure shadow \
         trail from its wings like smoke. Eyes like molten amethyst burn with an alien intelligence, \
         and each breath releases a hiss of freezing, shadow-laced air. This is not a creature to \
         be trifled with.",
    );
    drake.level = 8;
    drake.max_hp = 200;
    drake.current_hp = 200;
    drake.max_stamina = 100;
    drake.current_stamina = 100;
    drake.damage_dice = "3d6+4".to_string();
    drake.damage_type = DamageType::Cold;
    drake.armor_class = 2;
    drake.hit_modifier = 3;
    drake.stat_str = 16;
    drake.stat_con = 16;
    drake.stat_dex = 12;
    drake.stat_int = 14;
    drake.flags.aggressive = true;
    drake.flags.sentinel = true; // Stays in lair
    mobiles.push(drake);

    // ── Ambient NPCs ─────────────────────────────────────────────

    // Farmer — at Hilltop Farm
    let mut farmer = mobile(
        seed_uuid("mob:farmer"),
        "hilltop:farmer",
        "Old Barley the Farmer",
        "Old Barley the Farmer leans on a pitchfork, surveying his fields.",
        "A weather-beaten man with soil permanently embedded under his fingernails and a \
         straw hat that has seen better decades. Old Barley knows every furrow of Hilltop \
         Farm and can predict the weather better than any mage. He speaks slowly, but every \
         word carries the wisdom of the earth.",
    );
    farmer.level = 3;
    farmer.max_hp = 30;
    farmer.current_hp = 30;
    farmer.flags.sentinel = false;
    farmer.flags.no_attack = true;
    farmer.dialogue.insert("hello".to_string(), "Well now, a visitor! Don't get many of those up on the hill. You look like you could use some fresh air and honest work.".to_string());
    farmer.dialogue.insert(
        "crops".to_string(),
        "The wheat's coming in nicely this season. Tomatoes are looking good too, if the frost holds off.".to_string(),
    );
    farmer.dialogue.insert(
        "weather".to_string(),
        "I can feel it in my bones — rain's coming. Good for the crops, not so good for my knees.".to_string(),
    );
    farmer.dialogue.insert(
        "garden".to_string(),
        "If you're keen on gardening, the plots out back have good soil. Plant some seeds and see what grows!"
            .to_string(),
    );
    farmer.daily_routine = vec![
        RoutineEntry {
            start_hour: 5,
            activity: ActivityState::Working,
            destination_vnum: Some("hilltop:wheat_field".to_string()),
            transition_message: Some("heads out to tend the wheat fields.".to_string()),
            suppress_wander: false,
            dialogue_overrides: HashMap::new(),
        },
        RoutineEntry {
            start_hour: 12,
            activity: ActivityState::Eating,
            destination_vnum: Some("hilltop:farmhouse".to_string()),
            transition_message: Some("comes in from the fields for lunch.".to_string()),
            suppress_wander: true,
            dialogue_overrides: HashMap::new(),
        },
        RoutineEntry {
            start_hour: 13,
            activity: ActivityState::Working,
            destination_vnum: Some("hilltop:garden_plots".to_string()),
            transition_message: Some("heads out to check on the garden.".to_string()),
            suppress_wander: false,
            dialogue_overrides: HashMap::new(),
        },
        RoutineEntry {
            start_hour: 19,
            activity: ActivityState::Sleeping,
            destination_vnum: Some("hilltop:farmhouse".to_string()),
            transition_message: Some("puts away his tools and settles in for the night.".to_string()),
            suppress_wander: true,
            dialogue_overrides: HashMap::from([(
                "hello".to_string(),
                "*snores contentedly, dreaming of bountiful harvests*".to_string(),
            )]),
        },
    ];
    farmer.schedule_visible = true;
    mobiles.push(farmer);

    for mob in mobiles {
        db.save_mobile_data(mob)?;
    }

    tracing::info!("Seeded 15 mobile prototypes");
    Ok(())
}

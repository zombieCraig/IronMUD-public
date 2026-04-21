use anyhow::Result;
use std::collections::HashMap;
use uuid::Uuid;

use super::seed_uuid;
use crate::STARTING_ROOM_ID;
use crate::db::Db;
use crate::types::{
    CatchEntry, DoorState, ExtraDesc, RoomData, RoomExits, RoomFlags, RoomTrigger, TriggerType, WaterType,
};

/// Helper to create a room with common defaults filled in
fn room(id: Uuid, area_id: Uuid, vnum: &str, title: &str, description: &str) -> RoomData {
    RoomData {
        id,
        title: title.to_string(),
        description: description.to_string(),
        exits: RoomExits::default(),
        flags: RoomFlags::default(),
        extra_descs: Vec::new(),
        vnum: Some(vnum.to_string()),
        area_id: Some(area_id),
        triggers: Vec::new(),
        doors: HashMap::new(),
        spring_desc: None,
        summer_desc: None,
        autumn_desc: None,
        winter_desc: None,
        dynamic_desc: None,
        water_type: WaterType::None,
        catch_table: Vec::new(),
        is_property_template: false,
        property_template_id: None,
        is_template_entrance: false,
        property_lease_id: None,
        property_entrance: false,
        recent_departures: Vec::new(),
        blood_trails: Vec::new(),
        traps: Vec::new(),
        living_capacity: 0,
        residents: Vec::new(),
    }
}

pub fn seed_rooms(db: &Db) -> Result<()> {
    let mut count = 0;

    count += seed_oakvale(db)?;
    count += seed_whisperwood(db)?;
    count += seed_ironkeep(db)?;
    count += seed_shadowfang(db)?;
    count += seed_hilltop(db)?;

    tracing::info!("Seeded {} rooms", count);
    Ok(())
}

// ============================================================
// Oakvale Village (~15 rooms)
// ============================================================
fn seed_oakvale(db: &Db) -> Result<usize> {
    let area = seed_uuid("area:oakvale");
    // The town square uses the hardcoded STARTING_ROOM_ID for backward compat
    let square_id = Uuid::parse_str(STARTING_ROOM_ID)?;

    // --- Town Square ---
    let square = RoomData {
        id: square_id,
        title: "Town Square".to_string(),
        description: "You stand in the heart of Oakvale Village. Cobblestones worn smooth by countless travelers spread out beneath your feet. A weathered stone fountain burbles in the center, surrounded by timber-framed buildings with flower boxes in the windows. Merchant stalls line the eastern edge, and the comforting smell of fresh bread drifts from somewhere nearby.".to_string(),
        exits: RoomExits {
            north: Some(seed_uuid("oakvale:tavern")),
            east: Some(seed_uuid("oakvale:market")),
            south: Some(seed_uuid("oakvale:south_path")),
            west: Some(seed_uuid("oakvale:temple")),
            ..Default::default()
        },
        flags: RoomFlags { city: true, ..Default::default() },
        extra_descs: vec![
            ExtraDesc {
                keywords: vec!["fountain".to_string(), "water".to_string()],
                description: "The fountain is carved from gray stone, depicting a mermaid pouring water from a conch shell. The water is cool and clear, coins glinting at the bottom from wishes made long ago.".to_string(),
            },
            ExtraDesc {
                keywords: vec!["stalls".to_string(), "merchants".to_string()],
                description: "Wooden stalls display an array of goods: dried herbs, carved trinkets, bolts of cloth, and bundles of candles. Most of the merchants have packed up for the day.".to_string(),
            },
        ],
        vnum: Some("oakvale:square".to_string()),
        area_id: Some(area),
        triggers: vec![
            RoomTrigger {
                trigger_type: TriggerType::OnEnter,
                script_name: "demo_welcome".to_string(),
                enabled: true,
                interval_secs: 60,
                last_fired: 0,
                chance: 100,
                args: Vec::new(),
            },
            RoomTrigger {
                trigger_type: TriggerType::Periodic,
                script_name: "demo_ambiance".to_string(),
                enabled: true,
                interval_secs: 60,
                last_fired: 0,
                chance: 30,
                args: vec!["A sparrow hops along the fountain rim, pecking at crumbs.|The fountain water catches the light, sending ripples of color across the stones.|A child chases a stray cat between the market stalls, laughing.|The village bell tolls the hour from the temple tower.".to_string()],
            },
        ],
        doors: HashMap::new(),
        spring_desc: None,
        summer_desc: None,
        autumn_desc: None,
        winter_desc: None,
        dynamic_desc: None,
        water_type: WaterType::None,
        catch_table: Vec::new(),
        is_property_template: false,
        property_template_id: None,
        is_template_entrance: false,
        property_lease_id: None,
        property_entrance: false,
        recent_departures: Vec::new(),
        blood_trails: Vec::new(),
        traps: Vec::new(),
        living_capacity: 0,
        residents: Vec::new(),
    };

    // --- Tavern ---
    let mut tavern = room(
        seed_uuid("oakvale:tavern"),
        area,
        "oakvale:tavern",
        "The Rusty Tankard",
        "A cozy tavern with low wooden beams and a roaring fireplace. The smell of ale and roasted meat fills the air. Wooden tables are scattered about, scarred by years of dice games and bar fights. A long oak bar runs along the back wall, its surface polished to a warm sheen.",
    );
    tavern.flags.indoors = true;
    tavern.exits.south = Some(square_id);
    tavern.exits.north = Some(seed_uuid("oakvale:north_gate"));
    tavern.exits.up = Some(seed_uuid("oakvale:tavern_upstairs"));
    tavern.extra_descs.push(ExtraDesc {
        keywords: vec!["fireplace".to_string(), "fire".to_string()],
        description: "The fireplace crackles with warm flames, casting dancing shadows across the room. Above the mantle hangs a rusty tankard — the tavern's namesake. Legend says the first drink ever poured in Oakvale came from that very tankard.".to_string(),
    });

    // --- Tavern Upstairs ---
    let mut tavern_up = room(
        seed_uuid("oakvale:tavern_upstairs"),
        area,
        "oakvale:tavern_upstairs",
        "Tavern Upper Floor",
        "A narrow hallway with creaky floorboards runs between several guest rooms. A window at the far end overlooks the town square below. The muffled sounds of revelry drift up from the tavern.",
    );
    tavern_up.flags.indoors = true;
    tavern_up.exits.down = Some(seed_uuid("oakvale:tavern"));

    // --- General Store ---
    let mut store = room(
        seed_uuid("oakvale:general_store"),
        area,
        "oakvale:general_store",
        "General Store",
        "Shelves line every wall from floor to ceiling, crammed with supplies of every description: rope coils, lanterns, travel rations, bedrolls, and tools. A wooden counter separates the shop floor from the storeroom behind.",
    );
    store.flags.indoors = true;
    store.exits.north = Some(seed_uuid("oakvale:market"));

    // --- Smithy ---
    let mut smithy = room(
        seed_uuid("oakvale:smithy"),
        area,
        "oakvale:smithy",
        "The Iron Anvil",
        "Heat radiates from a massive stone forge in the center of this open-air workshop. Weapons and armor hang from racks along the walls, and the rhythmic clang of hammer on steel echoes off the stone floor. Sparks fly with each blow.",
    );
    smithy.flags.indoors = true;
    smithy.exits.south = Some(seed_uuid("oakvale:market"));
    smithy.extra_descs.push(ExtraDesc {
        keywords: vec!["forge".to_string(), "anvil".to_string()],
        description: "The forge is built of dark stone, its belly glowing orange with banked coals. A heavy iron anvil sits beside it, its surface dimpled from countless hammer strikes.".to_string(),
    });

    // --- Temple ---
    let mut temple = room(
        seed_uuid("oakvale:temple"),
        area,
        "oakvale:temple",
        "Temple of Light",
        "Pale sunlight streams through stained glass windows, casting colored patterns across the stone floor. Rows of wooden pews face a simple altar adorned with white candles. The air is warm and still, carrying a faint scent of incense. A sense of peace washes over you.",
    );
    temple.flags.indoors = true;
    temple.exits.east = Some(square_id);
    temple.extra_descs.push(ExtraDesc {
        keywords: vec!["altar".to_string(), "candles".to_string()],
        description: "The altar is carved from a single block of white marble. Dozens of candles burn upon it, their flames steady and unwavering despite the occasional draft. A simple golden sun emblem hangs on the wall above.".to_string(),
    });

    // --- Bank ---
    let mut bank = room(
        seed_uuid("oakvale:bank"),
        area,
        "oakvale:bank",
        "Oakvale Bank",
        "A sturdy stone building with iron-barred windows. Inside, a long marble counter separates the lobby from the vault area behind. A clerk waits patiently, ledger open and quill in hand.",
    );
    bank.flags.indoors = true;
    bank.flags.bank = true;
    bank.exits.north = Some(seed_uuid("oakvale:east_road"));

    // --- Post Office ---
    let mut post = room(
        seed_uuid("oakvale:post_office"),
        area,
        "oakvale:post_office",
        "Post Office",
        "Cubbyholes and mailbags fill the wall behind a worn wooden counter. A notice board beside the door is covered with wanted posters and delivery notices. The faint smell of sealing wax hangs in the air.",
    );
    post.flags.indoors = true;
    post.flags.post_office = true;
    post.exits.south = Some(seed_uuid("oakvale:east_road"));

    // --- Garden ---
    let mut garden = room(
        seed_uuid("oakvale:garden"),
        area,
        "oakvale:garden",
        "Village Garden",
        "A peaceful garden enclosed by a low stone wall. Raised planting beds are arranged in neat rows, some bursting with vegetables and herbs, others freshly turned and waiting for seeds. A few clay pots sit along the wall, and a water barrel stands in the corner.",
    );
    garden.flags.garden = true;
    garden.flags.dirt_floor = true;
    garden.exits.north = Some(seed_uuid("oakvale:south_path"));

    // --- Cottage Lane ---
    let mut cottage_entry = room(
        seed_uuid("oakvale:cottage_entry"),
        area,
        "oakvale:cottage_entry",
        "Cottage Lane",
        "A quiet lane of small timber-and-plaster cottages with thatched roofs. Window boxes overflow with wildflowers, and smoke curls from a few chimneys. A sign near one cottage reads 'For Rent — Inquire Within'.",
    );
    cottage_entry.exits.east = Some(seed_uuid("oakvale:south_path"));

    // --- Cottage Interior (property template) ---
    let mut cottage_int = room(
        seed_uuid("oakvale:cottage_interior"),
        area,
        "oakvale:cottage_interior",
        "A Cozy Cottage",
        "A small but comfortable single-room cottage. A straw mattress rests on a wooden bed frame in the corner, and a rough-hewn table with two chairs sits near the hearth. Hooks on the wall hold a few cooking utensils, and a chest at the foot of the bed provides storage.",
    );
    cottage_int.flags.indoors = true;
    cottage_int.flags.property_storage = true;
    cottage_int.is_property_template = true;

    // --- North Gate ---
    let mut north_gate = room(
        seed_uuid("oakvale:north_gate"),
        area,
        "oakvale:north_gate",
        "North Gate",
        "The northern edge of Oakvale where cobblestone gives way to a dirt path leading into the forest. A weathered wooden archway marks the village boundary, its crossbeam carved with protective runes. The dark canopy of the Whispering Woods looms ahead.",
    );
    north_gate.exits.south = Some(seed_uuid("oakvale:tavern"));
    north_gate.exits.north = Some(seed_uuid("whisperwood:trail_entrance"));

    // --- East Road ---
    let mut east_road = room(
        seed_uuid("oakvale:east_road"),
        area,
        "oakvale:east_road",
        "East Road",
        "A well-maintained road paved with packed earth and gravel leads east toward the imposing silhouette of the Iron Keep. To the south a sturdy bank building stands, and the post office is just north. The village thins out here, giving way to open fields.",
    );
    east_road.exits.west = Some(seed_uuid("oakvale:market"));
    east_road.exits.east = Some(seed_uuid("ironkeep:approach"));
    east_road.exits.south = Some(seed_uuid("oakvale:bank"));
    east_road.exits.north = Some(seed_uuid("oakvale:post_office"));

    // --- South Path ---
    let mut south_path = room(
        seed_uuid("oakvale:south_path"),
        area,
        "oakvale:south_path",
        "South Path",
        "A gentle dirt path winds southward through meadows dotted with wildflowers. The rooftops of Oakvale recede behind you as the path climbs a gradual hillside toward the farms. A small garden is tucked away to the south.",
    );
    south_path.exits.north = Some(square_id);
    south_path.exits.south = Some(seed_uuid("oakvale:garden"));
    south_path.exits.west = Some(seed_uuid("oakvale:cottage_entry"));
    south_path.exits.east = Some(seed_uuid("hilltop:path"));

    // --- Market ---
    let mut market = room(
        seed_uuid("oakvale:market"),
        area,
        "oakvale:market",
        "Market Street",
        "A busy stretch of cobblestone lined with merchant stalls and shop fronts. Vendors hawk their wares, from fresh produce to exotic trinkets. The general store's door stands open to the south, the rhythmic clanging of the smithy echoes from the north, and a broad road runs eastward toward the distant silhouette of the Iron Keep.",
    );
    market.flags.city = true;
    market.exits.west = Some(square_id);
    market.exits.east = Some(seed_uuid("oakvale:east_road"));
    market.exits.south = Some(seed_uuid("oakvale:general_store"));
    market.exits.north = Some(seed_uuid("oakvale:smithy"));

    // Save all Oakvale rooms
    let rooms = vec![
        square,
        tavern,
        tavern_up,
        store,
        smithy,
        temple,
        bank,
        post,
        garden,
        cottage_entry,
        cottage_int,
        north_gate,
        east_road,
        south_path,
        market,
    ];
    for r in rooms {
        db.save_room_data(r)?;
    }
    Ok(15)
}

// ============================================================
// Whispering Woods (~12 rooms)
// ============================================================
fn seed_whisperwood(db: &Db) -> Result<usize> {
    let area = seed_uuid("area:whisperwood");

    let mut trail_entrance = room(
        seed_uuid("whisperwood:trail_entrance"),
        area,
        "whisperwood:trail_entrance",
        "Forest Trail Entrance",
        "The dirt path from Oakvale narrows as it enters the Whispering Woods. Ancient oaks tower overhead, their gnarled branches interlocking to form a living archway. Dappled sunlight filters through the canopy, and birdsong fills the air.",
    );
    trail_entrance.exits.south = Some(seed_uuid("oakvale:north_gate"));
    trail_entrance.exits.north = Some(seed_uuid("whisperwood:fork"));
    trail_entrance.flags.dirt_floor = true;

    let mut fork = room(
        seed_uuid("whisperwood:fork"),
        area,
        "whisperwood:fork",
        "Forest Fork",
        "The trail splits into three paths at the base of a massive ancient oak. Carved wooden signs, weathered and mossy, point the way: west toward the pond, north deeper into the woods, and east to a sun-dappled clearing.",
    );
    fork.exits.south = Some(seed_uuid("whisperwood:trail_entrance"));
    fork.exits.west = Some(seed_uuid("whisperwood:pond"));
    fork.exits.north = Some(seed_uuid("whisperwood:deep_trail"));
    fork.exits.east = Some(seed_uuid("whisperwood:clearing"));
    fork.flags.dirt_floor = true;
    fork.extra_descs.push(ExtraDesc {
        keywords: vec!["oak".to_string(), "tree".to_string(), "trunk".to_string()],
        description: "The ancient oak must be centuries old. Its trunk is as wide as a cottage, bark deeply furrowed. Initials and symbols have been carved into the lower bark by generations of travelers.".to_string(),
    });

    let mut pond = room(
        seed_uuid("whisperwood:pond"),
        area,
        "whisperwood:pond",
        "Tranquil Pond",
        "A still pond fed by a gurgling brook reflects the overhanging willows like a mirror. Lily pads float on the surface, and dragonflies dart above the water. The bank is muddy but solid enough to sit on.",
    );
    pond.exits.east = Some(seed_uuid("whisperwood:fork"));
    pond.water_type = WaterType::Freshwater;
    pond.catch_table = vec![
        CatchEntry {
            vnum: "oakvale:trout".to_string(),
            weight: 50,
            min_skill: 0,
            rarity: "common".to_string(),
        },
        CatchEntry {
            vnum: "oakvale:bass".to_string(),
            weight: 30,
            min_skill: 1,
            rarity: "uncommon".to_string(),
        },
    ];
    pond.flags.dirt_floor = true;

    let mut clearing = room(
        seed_uuid("whisperwood:clearing"),
        area,
        "whisperwood:clearing",
        "Sunlit Clearing",
        "A natural clearing where the canopy opens to the sky. Wildflowers carpet the ground in a riot of color, and butterflies drift lazily on the warm breeze. A fallen log provides a natural seat.",
    );
    clearing.exits.west = Some(seed_uuid("whisperwood:fork"));
    clearing.exits.north = Some(seed_uuid("whisperwood:herb_garden"));
    clearing.flags.dirt_floor = true;
    clearing.spring_desc = Some("The clearing is alive with the colors of spring. Crocuses and bluebells push through the warming earth, and birdsong fills the air from every direction.".to_string());
    clearing.summer_desc = Some("The summer sun blazes down into the clearing, warming the carpet of wildflowers. Bees drone heavily from blossom to blossom, and the air shimmers with heat.".to_string());
    clearing.autumn_desc = Some("Fallen leaves in gold and crimson blanket the clearing. The wildflowers have faded, replaced by a tapestry of autumn color. A cool breeze carries the scent of decay and woodsmoke.".to_string());
    clearing.winter_desc = Some("A thin blanket of frost covers the clearing. The wildflowers are long gone, and bare branches reach toward a pale gray sky. Your breath mists in the cold air.".to_string());

    let mut herb_garden = room(
        seed_uuid("whisperwood:herb_garden"),
        area,
        "whisperwood:herb_garden",
        "Herb Garden Clearing",
        "Someone has cultivated a small herb garden in a sheltered nook of the forest. Neat rows of lavender, sage, and thyme grow between stones arranged as borders. A weathered wooden sign reads 'Take only what you need.'",
    );
    herb_garden.exits.south = Some(seed_uuid("whisperwood:clearing"));
    herb_garden.flags.dirt_floor = true;
    herb_garden.flags.garden = true;

    let mut deep_trail = room(
        seed_uuid("whisperwood:deep_trail"),
        area,
        "whisperwood:deep_trail",
        "Deep Forest Trail",
        "The trail narrows between towering trees whose canopy blocks most of the light. Thick moss covers every surface, muffling your footsteps. Strange mushrooms grow in clusters at the base of rotting stumps.",
    );
    deep_trail.exits.south = Some(seed_uuid("whisperwood:fork"));
    deep_trail.exits.north = Some(seed_uuid("whisperwood:dark_hollow"));
    deep_trail.exits.east = Some(seed_uuid("whisperwood:wolf_den"));
    deep_trail.flags.dirt_floor = true;
    deep_trail.triggers.push(RoomTrigger {
        trigger_type: TriggerType::Periodic,
        script_name: "forest_ambiance".to_string(),
        enabled: true,
        interval_secs: 45,
        last_fired: 0,
        chance: 40,
        args: Vec::new(),
    });

    let mut wolf_den = room(
        seed_uuid("whisperwood:wolf_den"),
        area,
        "whisperwood:wolf_den",
        "Wolf Den",
        "A rocky outcropping forms a shallow cave, its entrance littered with gnawed bones and tufts of fur. Claw marks score the stone, and a musky animal scent hangs heavy in the air. This is clearly the territory of a predator.",
    );
    wolf_den.exits.west = Some(seed_uuid("whisperwood:deep_trail"));
    wolf_den.flags.dirt_floor = true;

    let mut dark_hollow = room(
        seed_uuid("whisperwood:dark_hollow"),
        area,
        "whisperwood:dark_hollow",
        "Dark Hollow",
        "The trees close in oppressively here, their trunks twisted into unsettling shapes. Little light reaches the forest floor, and the air feels thick and watchful. To the north, you can make out the dark mouth of a cave.",
    );
    dark_hollow.exits.south = Some(seed_uuid("whisperwood:deep_trail"));
    dark_hollow.exits.north = Some(seed_uuid("shadowfang:entrance"));
    dark_hollow.flags.dark = true;
    dark_hollow.flags.dirt_floor = true;

    let mut brook_crossing = room(
        seed_uuid("whisperwood:brook"),
        area,
        "whisperwood:brook",
        "Brook Crossing",
        "A cheerful brook tumbles over mossy rocks, spanned by a simple wooden bridge of planks and rope. Ferns line both banks, and small fish dart in the shallows.",
    );
    brook_crossing.exits.west = Some(seed_uuid("whisperwood:trail_entrance"));
    brook_crossing.exits.north = Some(seed_uuid("whisperwood:grove"));
    brook_crossing.flags.dirt_floor = true;

    // Add the brook as an alternate exit from trail_entrance
    trail_entrance.exits.east = Some(seed_uuid("whisperwood:brook"));

    let mut grove = room(
        seed_uuid("whisperwood:grove"),
        area,
        "whisperwood:grove",
        "Mossy Grove",
        "An ethereal grove where every surface is covered in thick emerald moss. Shafts of golden light pierce the canopy like pillars, illuminating motes of pollen that drift through the air. The silence here feels sacred.",
    );
    grove.exits.south = Some(seed_uuid("whisperwood:brook"));
    grove.flags.dirt_floor = true;

    let mut forager_camp = room(
        seed_uuid("whisperwood:forager_camp"),
        area,
        "whisperwood:forager_camp",
        "Forager's Camp",
        "A small campsite tucked under an overhanging rock. A ring of stones surrounds a fire pit with cold ashes, and a makeshift lean-to provides shelter. Bundles of dried herbs hang from the branches overhead.",
    );
    forager_camp.exits.east = Some(seed_uuid("whisperwood:deep_trail"));
    forager_camp.flags.dirt_floor = true;

    // Connect deep_trail west to forager_camp
    deep_trail.exits.west = Some(seed_uuid("whisperwood:forager_camp"));

    let rooms = vec![
        trail_entrance,
        fork,
        pond,
        clearing,
        herb_garden,
        deep_trail,
        wolf_den,
        dark_hollow,
        brook_crossing,
        grove,
        forager_camp,
    ];
    for r in rooms {
        db.save_room_data(r)?;
    }
    Ok(11)
}

// ============================================================
// Iron Keep (~12 rooms)
// ============================================================
fn seed_ironkeep(db: &Db) -> Result<usize> {
    let area = seed_uuid("area:ironkeep");

    let mut approach = room(
        seed_uuid("ironkeep:approach"),
        area,
        "ironkeep:approach",
        "Castle Approach",
        "A wide stone road leads up to the imposing walls of the Iron Keep. Banners bearing a crossed-swords sigil flutter from the battlements high above. The road passes between two guard towers before reaching the massive iron-banded gatehouse.",
    );
    approach.exits.west = Some(seed_uuid("oakvale:east_road"));
    approach.exits.north = Some(seed_uuid("ironkeep:gatehouse"));

    let mut gatehouse = room(
        seed_uuid("ironkeep:gatehouse"),
        area,
        "ironkeep:gatehouse",
        "Gatehouse",
        "You stand beneath the heavy stone arch of the gatehouse. Iron portcullis rails run in grooves overhead, and murder holes dot the ceiling. An iron gate bars passage to the north, but a guard post beside it suggests entry may be possible.",
    );
    gatehouse.exits.south = Some(seed_uuid("ironkeep:approach"));
    gatehouse.exits.north = Some(seed_uuid("ironkeep:courtyard"));
    gatehouse.flags.indoors = true;
    // Locked iron gate to the courtyard
    gatehouse.doors.insert(
        "north".to_string(),
        DoorState {
            name: "iron gate".to_string(),
            is_closed: true,
            is_locked: true,
            key_id: Some(seed_uuid("oakvale:gate_key")),
            description: Some(
                "A heavy iron gate reinforced with thick crossbars. It looks like it requires a key to open."
                    .to_string(),
            ),
            keywords: vec!["gate".to_string(), "iron".to_string()],
        },
    );

    let mut courtyard = room(
        seed_uuid("ironkeep:courtyard"),
        area,
        "ironkeep:courtyard",
        "Castle Courtyard",
        "An expansive courtyard paved with flagstones, bustling with the daily business of the keep. A well sits in the center, and archways lead to various wings of the castle. Guards patrol the perimeter walls above.",
    );
    courtyard.exits.south = Some(seed_uuid("ironkeep:gatehouse"));
    courtyard.exits.east = Some(seed_uuid("ironkeep:armory"));
    courtyard.exits.west = Some(seed_uuid("ironkeep:great_hall"));
    courtyard.exits.north = Some(seed_uuid("ironkeep:tower_base"));
    courtyard.exits.up = Some(seed_uuid("ironkeep:barracks"));
    courtyard.exits.down = Some(seed_uuid("ironkeep:dungeon_stairs"));
    courtyard.extra_descs.push(ExtraDesc {
        keywords: vec!["well".to_string()],
        description: "A stone well with a wooden bucket and winch. The water is cold and clear, drawn from deep underground springs.".to_string(),
    });

    let mut armory = room(
        seed_uuid("ironkeep:armory"),
        area,
        "ironkeep:armory",
        "Armory",
        "Racks of weapons line the walls: swords, spears, maces, and axes, all meticulously maintained. Suits of armor stand on wooden mannequins, polished to a mirror sheen. A grizzled quartermaster keeps inventory behind a heavy counter.",
    );
    armory.exits.west = Some(seed_uuid("ironkeep:courtyard"));
    armory.flags.indoors = true;

    let mut great_hall = room(
        seed_uuid("ironkeep:great_hall"),
        area,
        "ironkeep:great_hall",
        "Great Hall",
        "A cavernous hall with vaulted ceilings supported by thick stone columns. A long oak table dominates the center, surrounded by high-backed chairs. Tapestries depicting great battles hang from the walls, and a massive fireplace warms the room. Workbenches along the far wall hold tools for crafting.",
    );
    great_hall.exits.east = Some(seed_uuid("ironkeep:courtyard"));
    great_hall.exits.north = Some(seed_uuid("ironkeep:kitchen"));
    great_hall.flags.indoors = true;
    great_hall.extra_descs.push(ExtraDesc {
        keywords: vec!["tapestries".to_string(), "tapestry".to_string()],
        description: "The tapestries are masterfully woven, depicting the founding of the Iron Keep. One shows a knight driving back a horde of goblins. Another depicts the construction of the castle itself.".to_string(),
    });

    let mut kitchen = room(
        seed_uuid("ironkeep:kitchen"),
        area,
        "ironkeep:kitchen",
        "Castle Kitchen",
        "A vast kitchen filled with the aromas of simmering stews and baking bread. Copper pots hang from iron hooks above a massive hearth. Barrels of flour, salt, and spices line the walls, and a wooden butcher's block dominates the center.",
    );
    kitchen.exits.south = Some(seed_uuid("ironkeep:great_hall"));
    kitchen.flags.indoors = true;

    let mut tower_base = room(
        seed_uuid("ironkeep:tower_base"),
        area,
        "ironkeep:tower_base",
        "Tower Base",
        "The base of the keep's central tower. A spiral staircase winds upward, and a heavy wooden platform attached to chains and pulleys serves as a primitive elevator. A lever on the wall controls the mechanism.",
    );
    tower_base.exits.south = Some(seed_uuid("ironkeep:courtyard"));
    tower_base.flags.indoors = true;

    // Elevator interior room
    let mut elevator = room(
        seed_uuid("ironkeep:elevator"),
        area,
        "ironkeep:elevator",
        "Tower Elevator",
        "A sturdy wooden platform enclosed by iron railings, suspended by thick chains. The mechanism creaks softly as it holds position. Through the gaps in the railing, you can see the stone shaft of the tower stretching above and below.",
    );
    elevator.flags.indoors = true;

    let mut tower_mid = room(
        seed_uuid("ironkeep:tower_mid"),
        area,
        "ironkeep:tower_mid",
        "Tower Landing",
        "A stone landing halfway up the tower. Arrow slits in the walls provide narrow views of the surrounding countryside. A weapons rack holds spare crossbow bolts, and a guard's chair sits by the window.",
    );
    tower_mid.flags.indoors = true;

    let mut tower_top = room(
        seed_uuid("ironkeep:tower_top"),
        area,
        "ironkeep:tower_top",
        "Tower Summit",
        "The top of the Iron Keep's central tower. A sweeping panorama stretches in every direction: the Whispering Woods to the north, Oakvale Village to the west, rolling farmland to the south, and distant mountains to the east. The wind whips fiercely up here.",
    );
    tower_top.flags.indoors = false;

    let mut barracks = room(
        seed_uuid("ironkeep:barracks"),
        area,
        "ironkeep:barracks",
        "Barracks",
        "Rows of simple cots fill this long stone room, each with a small footlocker. Weapon racks line the walls between the beds. The room smells of leather, sweat, and sword oil. A few off-duty guards are playing dice in the corner.",
    );
    barracks.exits.down = Some(seed_uuid("ironkeep:courtyard"));
    barracks.flags.indoors = true;

    let mut dungeon_stairs = room(
        seed_uuid("ironkeep:dungeon_stairs"),
        area,
        "ironkeep:dungeon_stairs",
        "Dungeon Stairs",
        "Worn stone steps spiral downward into darkness. The air grows cold and damp with each step. Sconces on the wall hold sputtering torches that cast more shadow than light. The passage leads down into the Shadowfang Caves below the keep.",
    );
    dungeon_stairs.exits.up = Some(seed_uuid("ironkeep:courtyard"));
    dungeon_stairs.exits.down = Some(seed_uuid("shadowfang:upper_cave"));
    dungeon_stairs.flags.dark = true;

    let rooms = vec![
        approach,
        gatehouse,
        courtyard,
        armory,
        great_hall,
        kitchen,
        tower_base,
        elevator,
        tower_mid,
        tower_top,
        barracks,
        dungeon_stairs,
    ];
    for r in rooms {
        db.save_room_data(r)?;
    }
    Ok(12)
}

// ============================================================
// Shadowfang Caves (~10 rooms)
// ============================================================
fn seed_shadowfang(db: &Db) -> Result<usize> {
    let area = seed_uuid("area:shadowfang");

    let mut entrance = room(
        seed_uuid("shadowfang:entrance"),
        area,
        "shadowfang:entrance",
        "Cave Entrance",
        "A yawning cave mouth opens in a cliff face, surrounded by twisted roots and loose scree. A cold draft carries the smell of damp stone and something else — something feral. Scratch marks on the rock suggest large claws.",
    );
    entrance.exits.south = Some(seed_uuid("whisperwood:dark_hollow"));
    entrance.exits.north = Some(seed_uuid("shadowfang:tunnel"));
    entrance.flags.dark = true;
    entrance.flags.dirt_floor = true;

    let mut tunnel = room(
        seed_uuid("shadowfang:tunnel"),
        area,
        "shadowfang:tunnel",
        "Winding Tunnel",
        "A narrow tunnel that twists and turns through the rock. The walls are slick with moisture, and the ceiling drops low enough to force you to duck in places. Crude torches — little more than sticks wrapped in oily rags — are jammed into cracks in the walls.",
    );
    tunnel.exits.south = Some(seed_uuid("shadowfang:entrance"));
    tunnel.exits.north = Some(seed_uuid("shadowfang:goblin_camp"));
    tunnel.exits.east = Some(seed_uuid("shadowfang:fungal_grotto"));
    tunnel.flags.dark = true;
    tunnel.triggers.push(RoomTrigger {
        trigger_type: TriggerType::Periodic,
        script_name: "demo_ambiance".to_string(),
        enabled: true,
        interval_secs: 30,
        last_fired: 0,
        chance: 35,
        args: vec!["Water drips from the ceiling into an unseen pool.|Something skitters in the darkness ahead.|A cold draft extinguishes a nearby torch, plunging the tunnel into deeper shadow.|You hear a distant, echoing growl.".to_string()],
    });

    let mut goblin_camp = room(
        seed_uuid("shadowfang:goblin_camp"),
        area,
        "shadowfang:goblin_camp",
        "Goblin Camp",
        "A crude camp fills this wider section of the cave. Rough shelters made of bones and animal hides cluster around a smoldering fire pit. Gnawed bones, broken pottery, and other refuse litter the ground. The stench is overwhelming.",
    );
    goblin_camp.exits.south = Some(seed_uuid("shadowfang:tunnel"));
    goblin_camp.exits.north = Some(seed_uuid("shadowfang:crossroads"));
    goblin_camp.exits.up = Some(seed_uuid("shadowfang:upper_cave"));
    goblin_camp.flags.dark = true;

    let mut fungal_grotto = room(
        seed_uuid("shadowfang:fungal_grotto"),
        area,
        "shadowfang:fungal_grotto",
        "Fungal Grotto",
        "Bioluminescent fungi cover every surface, casting an eerie blue-green glow. Mushrooms of every size grow in clusters — some as tall as a person. Spores drift in the still air like underwater snow. A few of the larger specimens might be edible... or deadly.",
    );
    fungal_grotto.exits.west = Some(seed_uuid("shadowfang:tunnel"));
    fungal_grotto.flags.dirt_floor = true;

    let mut crossroads = room(
        seed_uuid("shadowfang:crossroads"),
        area,
        "shadowfang:crossroads",
        "Cave Crossroads",
        "Three passages branch from this junction in the cave system. The walls are scratched with crude markings — perhaps goblin territory markers. A tattered cloth banner hangs from an iron spike driven into the rock.",
    );
    crossroads.exits.south = Some(seed_uuid("shadowfang:goblin_camp"));
    crossroads.exits.west = Some(seed_uuid("shadowfang:spider_nest"));
    crossroads.exits.north = Some(seed_uuid("shadowfang:drake_lair"));
    crossroads.exits.east = Some(seed_uuid("shadowfang:underground_pool"));
    crossroads.flags.dark = true;

    let mut spider_nest = room(
        seed_uuid("shadowfang:spider_nest"),
        area,
        "shadowfang:spider_nest",
        "Spider Nest",
        "Thick webs fill this chamber from floor to ceiling, their sticky strands catching the faint light. Desiccated husks of previous victims hang suspended in cocoons. The webs vibrate with the movement of something large and unseen.",
    );
    spider_nest.exits.east = Some(seed_uuid("shadowfang:crossroads"));
    spider_nest.flags.dark = true;
    spider_nest.flags.difficult_terrain = true;

    let mut underground_pool = room(
        seed_uuid("shadowfang:underground_pool"),
        area,
        "shadowfang:underground_pool",
        "Underground Pool",
        "A cavern opens around a still, dark pool of water that seems to glow faintly from within. Crystal formations line the walls, refracting the strange light. The water is impossibly clear, and you can see strange pale fish drifting in the depths.",
    );
    underground_pool.exits.west = Some(seed_uuid("shadowfang:crossroads"));
    underground_pool.water_type = WaterType::Magical;
    underground_pool.catch_table = vec![CatchEntry {
        vnum: "shadowfang:crystal_fish".to_string(),
        weight: 50,
        min_skill: 2,
        rarity: "rare".to_string(),
    }];

    let mut drake_lair = room(
        seed_uuid("shadowfang:drake_lair"),
        area,
        "shadowfang:drake_lair",
        "Shadow Drake's Lair",
        "An enormous cavern, its ceiling lost in darkness far above. The floor is littered with bones, shattered armor, and glittering coins — the hoard of the cave's apex predator. Scorch marks blacken the walls, and the air shimmers with residual heat. A massive shadow shifts in the darkness.",
    );
    drake_lair.exits.south = Some(seed_uuid("shadowfang:crossroads"));
    drake_lair.exits.east = Some(seed_uuid("shadowfang:treasure_alcove"));
    drake_lair.flags.dark = true;
    drake_lair.extra_descs.push(ExtraDesc {
        keywords: vec!["hoard".to_string(), "coins".to_string(), "treasure".to_string()],
        description: "Gold coins, tarnished silver, and a few gemstones are scattered across the cave floor, mixed in with bones and debris. This is clearly the drake's accumulated hoard from years of raiding.".to_string(),
    });

    let mut treasure = room(
        seed_uuid("shadowfang:treasure_alcove"),
        area,
        "shadowfang:treasure_alcove",
        "Treasure Alcove",
        "A small alcove behind the drake's lair, sealed off by a heavy iron-bound door. The walls sparkle with embedded crystal veins, and a single stone pedestal stands in the center of the room.",
    );
    treasure.exits.west = Some(seed_uuid("shadowfang:drake_lair"));
    treasure.flags.dark = true;
    treasure.doors.insert(
        "west".to_string(),
        DoorState {
            name: "iron-bound door".to_string(),
            is_closed: true,
            is_locked: true,
            key_id: Some(seed_uuid("shadowfang:treasure_key")),
            description: Some(
                "A heavy door reinforced with iron bands. A large keyhole suggests it requires a specific key."
                    .to_string(),
            ),
            keywords: vec!["door".to_string(), "iron".to_string()],
        },
    );

    let mut upper_cave = room(
        seed_uuid("shadowfang:upper_cave"),
        area,
        "shadowfang:upper_cave",
        "Upper Caves",
        "A natural cave with a higher ceiling than the tunnels below. A stone staircase, clearly man-made, leads upward toward the Iron Keep. A steep slope descends into the depths below.",
    );
    upper_cave.exits.up = Some(seed_uuid("ironkeep:dungeon_stairs"));
    upper_cave.exits.down = Some(seed_uuid("shadowfang:goblin_camp"));
    upper_cave.flags.dark = true;

    let rooms = vec![
        entrance,
        tunnel,
        goblin_camp,
        fungal_grotto,
        crossroads,
        spider_nest,
        underground_pool,
        drake_lair,
        treasure,
        upper_cave,
    ];
    for r in rooms {
        db.save_room_data(r)?;
    }
    Ok(10)
}

// ============================================================
// Hilltop Farm (~8 rooms)
// ============================================================
fn seed_hilltop(db: &Db) -> Result<usize> {
    let area = seed_uuid("area:hilltop");

    let mut path = room(
        seed_uuid("hilltop:path"),
        area,
        "hilltop:path",
        "Hilltop Path",
        "A winding dirt path climbs the gentle hill toward the farm buildings visible at the crest. Wildflowers sway in the breeze along the edges, and stone walls mark the boundaries of nearby fields.",
    );
    path.exits.west = Some(seed_uuid("oakvale:south_path"));
    path.exits.east = Some(seed_uuid("hilltop:farmyard"));
    path.exits.south = Some(seed_uuid("hilltop:wheat_field"));
    path.flags.dirt_floor = true;

    let mut farmyard = room(
        seed_uuid("hilltop:farmyard"),
        area,
        "hilltop:farmyard",
        "Farmyard",
        "The center of the hilltop farm, a packed-earth yard surrounded by buildings. A weathered farmhouse stands to the north, a workshop to the east, and a large barn to the south. The smell of hay, animals, and fresh earth fills the air.",
    );
    farmyard.exits.west = Some(seed_uuid("hilltop:path"));
    farmyard.exits.north = Some(seed_uuid("hilltop:farmhouse"));
    farmyard.exits.east = Some(seed_uuid("hilltop:workshop"));
    farmyard.exits.south = Some(seed_uuid("hilltop:barn"));
    farmyard.flags.dirt_floor = true;

    let mut farmhouse = room(
        seed_uuid("hilltop:farmhouse"),
        area,
        "hilltop:farmhouse",
        "Farmhouse",
        "A sturdy stone farmhouse with a thatched roof. A large kitchen table dominates the main room, surrounded by mismatched chairs. Herbs hang drying from the rafters, and a cast-iron stove radiates warmth. Everything speaks of a simple but comfortable life.",
    );
    farmhouse.exits.south = Some(seed_uuid("hilltop:farmyard"));
    farmhouse.flags.indoors = true;

    let mut workshop = room(
        seed_uuid("hilltop:workshop"),
        area,
        "hilltop:workshop",
        "Farm Workshop",
        "A practical workspace with a heavy workbench, a small forge for tool repair, and shelves of supplies. Tools hang from pegs on the wall: saws, hammers, chisels, and planes. Wood shavings curl on the floor around the bench.",
    );
    workshop.exits.west = Some(seed_uuid("hilltop:farmyard"));
    workshop.flags.indoors = true;

    let mut barn = room(
        seed_uuid("hilltop:barn"),
        area,
        "hilltop:barn",
        "Barn",
        "A large timber barn smelling of hay and livestock. Bales of straw are stacked to the rafters, and a few stalls line one wall. Farming tools — pitchforks, scythes, and hoes — lean against the posts. A wooden ladder leads to a hayloft above.",
    );
    barn.exits.north = Some(seed_uuid("hilltop:farmyard"));
    barn.flags.indoors = true;
    barn.extra_descs.push(ExtraDesc {
        keywords: vec!["hayloft".to_string(), "loft".to_string(), "ladder".to_string()],
        description: "A wooden ladder leads up to a hayloft packed with golden straw bales. It looks like a good place to hide... or nap.".to_string(),
    });

    let mut garden_plots = room(
        seed_uuid("hilltop:garden_plots"),
        area,
        "hilltop:garden_plots",
        "Garden Plots",
        "Neat rows of raised beds stretch across this section of the farm. Rich dark soil has been carefully tended, and wooden stakes mark where different crops grow. A water barrel and a collection of clay pots sit at the end of the rows.",
    );
    garden_plots.exits.down = Some(seed_uuid("hilltop:farmyard"));
    garden_plots.flags.garden = true;
    garden_plots.flags.dirt_floor = true;

    // Connect farmyard north-east to garden plots
    farmyard.exits.up = Some(seed_uuid("hilltop:garden_plots"));

    let mut orchard = room(
        seed_uuid("hilltop:orchard"),
        area,
        "hilltop:orchard",
        "Apple Orchard",
        "Rows of gnarled apple trees stretch across the hillside, their branches heavy with fruit in season. The grass beneath the trees is soft and dappled with shade. A few fallen apples dot the ground.",
    );
    orchard.exits.up = Some(seed_uuid("hilltop:farmyard"));
    orchard.flags.dirt_floor = true;

    // Connect farmyard south to orchard too (via east)
    farmyard.exits.down = Some(seed_uuid("hilltop:orchard"));

    let mut wheat_field = room(
        seed_uuid("hilltop:wheat_field"),
        area,
        "hilltop:wheat_field",
        "Wheat Field",
        "A broad field of wheat stretches before you, the stalks swaying gently in the breeze. A narrow path cuts through the middle, barely wide enough to walk single file. The field extends to the horizon.",
    );
    wheat_field.exits.north = Some(seed_uuid("hilltop:path"));
    wheat_field.flags.dirt_floor = true;
    wheat_field.spring_desc = Some("Young green wheat shoots push up through the dark soil in orderly rows. The field has been freshly sown, and the earth smells of spring rain.".to_string());
    wheat_field.summer_desc = Some("The wheat stands tall and golden, nearly ready for harvest. The stalks rustle in the warm summer breeze, creating waves of gold across the hillside.".to_string());
    wheat_field.autumn_desc = Some("The wheat has been harvested, leaving short stubble in rows across the field. Bound sheaves lean against each other in neat stacks, waiting to be carted to the barn.".to_string());
    wheat_field.winter_desc = Some("The field lies fallow under a thin layer of frost. The bare earth is hard and cold, resting until spring comes again.".to_string());

    let rooms = vec![
        path,
        farmyard,
        farmhouse,
        workshop,
        barn,
        garden_plots,
        orchard,
        wheat_field,
    ];
    for r in rooms {
        db.save_room_data(r)?;
    }
    Ok(8)
}

//! Item types: prototypes and live instances.
//!
//! Covers `ItemType` discriminants, the per-item `ItemFlags`, on-use spell
//! payloads (`CastOnUse`), liquid container specifics (`LiquidType` plus
//! its default-effects table), gold-pile description helpers, and the
//! `ItemData` aggregate that ties them together.

use super::{BodyPart, DamageType, EffectType, ExtraDesc, ItemEffect, ItemTrigger, OnHitEffect, WeaponSkill, WearLocation};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Deserialize categories from either a single string (legacy), an array of strings, or null.
pub(crate) fn deserialize_categories<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de;

    struct CategoriesVisitor;

    impl<'de> de::Visitor<'de> for CategoriesVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("null, a string, or an array of strings")
        }

        fn visit_unit<E>(self) -> Result<Vec<String>, E>
        where
            E: de::Error,
        {
            Ok(Vec::new())
        }

        fn visit_none<E>(self) -> Result<Vec<String>, E>
        where
            E: de::Error,
        {
            Ok(Vec::new())
        }

        fn visit_str<E>(self, v: &str) -> Result<Vec<String>, E>
        where
            E: de::Error,
        {
            if v.is_empty() {
                Ok(Vec::new())
            } else {
                Ok(vec![v.to_string()])
            }
        }

        fn visit_string<E>(self, v: String) -> Result<Vec<String>, E>
        where
            E: de::Error,
        {
            if v.is_empty() { Ok(Vec::new()) } else { Ok(vec![v]) }
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Vec<String>, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let mut categories = Vec::new();
            while let Some(val) = seq.next_element::<String>()? {
                categories.push(val);
            }
            Ok(categories)
        }
    }

    deserializer.deserialize_any(CategoriesVisitor)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ItemType {
    #[default]
    Misc,
    Armor,
    Weapon,
    Container,
    LiquidContainer,
    Food,
    Key,
    Gold,
    Ammunition,
    Potion,
    Wand,
    Staff,
    /// Writable paper. Players with a `Pen` in their inventory can use
    /// `write <paper>` to author or revise the item's `note_content`.
    Note,
    /// Writing tool. Required (anywhere in inventory) to author a `Note`.
    Pen,
    /// Bulletin board. Players use the `board` command to list/read/write
    /// posts; access gating lives on `ItemData.board_*` fields.
    Board,
}

impl ItemType {
    pub fn from_str(s: &str) -> Option<ItemType> {
        match s.to_lowercase().as_str() {
            "misc" => Some(ItemType::Misc),
            "armor" => Some(ItemType::Armor),
            "weapon" => Some(ItemType::Weapon),
            "container" => Some(ItemType::Container),
            "liquid_container" | "liquidcontainer" | "drink" | "drinkcon" => Some(ItemType::LiquidContainer),
            "food" => Some(ItemType::Food),
            "key" => Some(ItemType::Key),
            "gold" => Some(ItemType::Gold),
            "ammunition" | "ammo" => Some(ItemType::Ammunition),
            "potion" => Some(ItemType::Potion),
            "wand" => Some(ItemType::Wand),
            "staff" => Some(ItemType::Staff),
            "note" | "paper" => Some(ItemType::Note),
            "pen" => Some(ItemType::Pen),
            "board" | "bulletin" | "bulletin_board" => Some(ItemType::Board),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            ItemType::Misc => "misc",
            ItemType::Armor => "armor",
            ItemType::Weapon => "weapon",
            ItemType::Container => "container",
            ItemType::LiquidContainer => "liquid_container",
            ItemType::Food => "food",
            ItemType::Key => "key",
            ItemType::Gold => "gold",
            ItemType::Ammunition => "ammunition",
            ItemType::Potion => "potion",
            ItemType::Wand => "wand",
            ItemType::Staff => "staff",
            ItemType::Note => "note",
            ItemType::Pen => "pen",
            ItemType::Board => "board",
        }
    }
}

/// CircleMUD POTION/WAND/STAFF on-use spell payload.
///
/// `spell` is an IronMUD spell id (matches an entry in `spells_fantasy.json`).
/// `min_level` is the magic skill level required to use the item (0 = none —
/// potions are universally usable). `charges` and `max_charges` track usage:
/// potions ignore charges (single-use, deleted on quaff), wands/staves consume
/// one per `zap` / `brandish`. When `charges == 0` the item is depleted but
/// stays in inventory.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CastOnUse {
    pub spell: String,
    #[serde(default)]
    pub min_level: i32,
    #[serde(default)]
    pub charges: i32,
    #[serde(default)]
    pub max_charges: i32,
    /// Per-item cooldown override in seconds. When `Some(n)`, item-cast firings
    /// set the character's spell cooldown to `n` instead of the spell's own
    /// `cooldown_secs`. Lets builders tune item-specific cadence (a wand of
    /// cheap heal with longer recharge, or a rare scroll with no cooldown).
    /// `None` = use the spell's default cooldown.
    #[serde(default)]
    pub cooldown_secs: Option<i32>,
}

/// Returns the tier description for a gold amount
pub fn get_gold_tier_description(amount: i32) -> &'static str {
    match amount {
        1..=10 => "a few coins",
        11..=50 => "some gold",
        51..=200 => "a pile of gold",
        201..=1000 => "a large pile of gold",
        _ if amount > 1000 => "a fortune in gold",
        _ => "some gold", // fallback for 0 or negative
    }
}

/// Returns the short description for gold (shown in room listings)
pub fn get_gold_short_desc(amount: i32) -> String {
    let tier = get_gold_tier_description(amount);
    // Capitalize first letter
    let mut chars = tier.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str() + " lies here.",
    }
}

/// Returns the long description for gold (shown when examined)
pub fn get_gold_long_desc(amount: i32) -> String {
    match amount {
        1..=10 => format!("A small scattering of {} gold coins glints in the light.", amount),
        11..=50 => format!("A modest collection of {} gold coins is piled here.", amount),
        51..=200 => format!("An enticing pile of {} gold coins awaits collection.", amount),
        201..=1000 => format!("A large heap of {} gold coins gleams invitingly.", amount),
        _ => format!(
            "An absolutely staggering fortune of {} gold coins fills the area.",
            amount
        ),
    }
}

/// Creates a new gold item with the specified amount
pub fn create_gold_item(amount: i32) -> ItemData {
    let tier = get_gold_tier_description(amount);
    let mut item = ItemData::new(
        tier.to_string(),
        get_gold_short_desc(amount),
        get_gold_long_desc(amount),
    );
    item.item_type = ItemType::Gold;
    item.value = amount;
    item.keywords = vec!["gold".to_string(), "coins".to_string(), "coin".to_string()];
    item.weight = (amount / 100).max(1);
    item
}

/// Updates gold item descriptions after the amount changes (e.g., after merging)
pub fn update_gold_descriptions(item: &mut ItemData) {
    if item.item_type == ItemType::Gold {
        item.name = get_gold_tier_description(item.value).to_string();
        item.short_desc = get_gold_short_desc(item.value);
        item.long_desc = get_gold_long_desc(item.value);
        item.weight = (item.value / 100).max(1);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LiquidType {
    #[default]
    Water,
    Ale,
    Wine,
    Beer,
    Alcohol,
    Spirits,
    Milk,
    Juice,
    Tea,
    Coffee,
    Poison,
    HealingPotion,
    ManaPotion,
    Blood,
    Oil,
}

impl LiquidType {
    pub fn from_str(s: &str) -> Option<LiquidType> {
        match s.to_lowercase().as_str() {
            "water" => Some(LiquidType::Water),
            "ale" => Some(LiquidType::Ale),
            "wine" => Some(LiquidType::Wine),
            "beer" | "mead" => Some(LiquidType::Beer),
            "alcohol" => Some(LiquidType::Alcohol),
            "spirits" | "liquor" | "cocktail" => Some(LiquidType::Spirits),
            "milk" => Some(LiquidType::Milk),
            "juice" => Some(LiquidType::Juice),
            "tea" => Some(LiquidType::Tea),
            "coffee" => Some(LiquidType::Coffee),
            "poison" => Some(LiquidType::Poison),
            "healing_potion" | "healingpotion" | "heal_potion" => Some(LiquidType::HealingPotion),
            "mana_potion" | "manapotion" => Some(LiquidType::ManaPotion),
            "blood" => Some(LiquidType::Blood),
            "oil" => Some(LiquidType::Oil),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            LiquidType::Water => "water",
            LiquidType::Ale => "ale",
            LiquidType::Wine => "wine",
            LiquidType::Beer => "beer",
            LiquidType::Alcohol => "alcohol",
            LiquidType::Spirits => "spirits",
            LiquidType::Milk => "milk",
            LiquidType::Juice => "juice",
            LiquidType::Tea => "tea",
            LiquidType::Coffee => "coffee",
            LiquidType::Poison => "poison",
            LiquidType::HealingPotion => "healing_potion",
            LiquidType::ManaPotion => "mana_potion",
            LiquidType::Blood => "blood",
            LiquidType::Oil => "oil",
        }
    }

    pub fn all() -> Vec<&'static str> {
        vec![
            "water",
            "ale",
            "wine",
            "beer",
            "alcohol",
            "spirits",
            "milk",
            "juice",
            "tea",
            "coffee",
            "poison",
            "healing_potion",
            "mana_potion",
            "blood",
            "oil",
        ]
    }

    /// Returns default liquid effects for this liquid type, mirroring oedit auto_set_liquid_defaults.
    pub fn default_effects(&self) -> Vec<ItemEffect> {
        match self {
            LiquidType::Water => vec![ItemEffect {
                effect_type: EffectType::Quenched,
                magnitude: 100,
                duration: 0,
                script_callback: None,
            }],
            LiquidType::Ale => vec![
                ItemEffect {
                    effect_type: EffectType::Drunk,
                    magnitude: 2,
                    duration: 300,
                    script_callback: None,
                },
                ItemEffect {
                    effect_type: EffectType::Quenched,
                    magnitude: 50,
                    duration: 0,
                    script_callback: None,
                },
            ],
            LiquidType::Wine => vec![
                ItemEffect {
                    effect_type: EffectType::Drunk,
                    magnitude: 4,
                    duration: 300,
                    script_callback: None,
                },
                ItemEffect {
                    effect_type: EffectType::Quenched,
                    magnitude: 30,
                    duration: 0,
                    script_callback: None,
                },
            ],
            LiquidType::Beer => vec![
                ItemEffect {
                    effect_type: EffectType::Drunk,
                    magnitude: 2,
                    duration: 300,
                    script_callback: None,
                },
                ItemEffect {
                    effect_type: EffectType::Quenched,
                    magnitude: 50,
                    duration: 0,
                    script_callback: None,
                },
                ItemEffect {
                    effect_type: EffectType::Satiated,
                    magnitude: 10,
                    duration: 0,
                    script_callback: None,
                },
            ],
            LiquidType::Alcohol => vec![
                ItemEffect {
                    effect_type: EffectType::Drunk,
                    magnitude: 6,
                    duration: 300,
                    script_callback: None,
                },
                ItemEffect {
                    effect_type: EffectType::Quenched,
                    magnitude: 20,
                    duration: 0,
                    script_callback: None,
                },
            ],
            LiquidType::Spirits => vec![
                ItemEffect {
                    effect_type: EffectType::Drunk,
                    magnitude: 5,
                    duration: 300,
                    script_callback: None,
                },
                ItemEffect {
                    effect_type: EffectType::Quenched,
                    magnitude: 25,
                    duration: 0,
                    script_callback: None,
                },
            ],
            LiquidType::Milk => vec![
                ItemEffect {
                    effect_type: EffectType::Satiated,
                    magnitude: 20,
                    duration: 0,
                    script_callback: None,
                },
                ItemEffect {
                    effect_type: EffectType::Quenched,
                    magnitude: 80,
                    duration: 0,
                    script_callback: None,
                },
            ],
            LiquidType::Juice => vec![
                ItemEffect {
                    effect_type: EffectType::StaminaRestore,
                    magnitude: 5,
                    duration: 0,
                    script_callback: None,
                },
                ItemEffect {
                    effect_type: EffectType::Quenched,
                    magnitude: 80,
                    duration: 0,
                    script_callback: None,
                },
            ],
            LiquidType::Tea => vec![
                ItemEffect {
                    effect_type: EffectType::StaminaRestore,
                    magnitude: 3,
                    duration: 0,
                    script_callback: None,
                },
                ItemEffect {
                    effect_type: EffectType::Quenched,
                    magnitude: 90,
                    duration: 0,
                    script_callback: None,
                },
            ],
            LiquidType::Coffee => vec![
                ItemEffect {
                    effect_type: EffectType::StaminaRestore,
                    magnitude: 8,
                    duration: 0,
                    script_callback: None,
                },
                ItemEffect {
                    effect_type: EffectType::Quenched,
                    magnitude: 70,
                    duration: 0,
                    script_callback: None,
                },
            ],
            LiquidType::Poison => vec![ItemEffect {
                effect_type: EffectType::Poison,
                magnitude: 10,
                duration: 0,
                script_callback: None,
            }],
            LiquidType::HealingPotion => vec![
                ItemEffect {
                    effect_type: EffectType::Heal,
                    magnitude: 20,
                    duration: 0,
                    script_callback: None,
                },
                ItemEffect {
                    effect_type: EffectType::Quenched,
                    magnitude: 30,
                    duration: 0,
                    script_callback: None,
                },
            ],
            LiquidType::ManaPotion => vec![
                ItemEffect {
                    effect_type: EffectType::ManaRestore,
                    magnitude: 20,
                    duration: 0,
                    script_callback: None,
                },
                ItemEffect {
                    effect_type: EffectType::Quenched,
                    magnitude: 30,
                    duration: 0,
                    script_callback: None,
                },
            ],
            LiquidType::Blood => vec![ItemEffect {
                effect_type: EffectType::Satiated,
                magnitude: 10,
                duration: 0,
                script_callback: None,
            }],
            LiquidType::Oil => vec![ItemEffect {
                effect_type: EffectType::Poison,
                magnitude: 3,
                duration: 0,
                script_callback: None,
            }],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ItemFlags {
    #[serde(default)]
    pub no_drop: bool,
    #[serde(default)]
    pub no_get: bool,
    #[serde(default)]
    pub no_remove: bool,
    #[serde(default)]
    pub invisible: bool,
    #[serde(default)]
    pub glow: bool,
    #[serde(default)]
    pub hum: bool,
    #[serde(default)]
    pub magical: bool, // Reveals "(magical aura)" cue when viewer has DetectMagic buff
    #[serde(default)]
    pub holy: bool, // Blessed/divine/silver — doubles damage to MobileFlags.holy_vulnerable targets
    #[serde(default)]
    pub no_sell: bool,
    #[serde(default)]
    pub no_donate: bool,
    #[serde(default)]
    pub unique: bool,
    #[serde(default)]
    pub quest_item: bool,
    #[serde(default)]
    pub vending: bool, // Functions as a vending machine
    #[serde(default)]
    pub provides_light: bool, // Provides light when equipped/wielded
    #[serde(default)]
    pub night_vision: bool, // Grants the wearer night vision while equipped
    #[serde(default)]
    pub fishing_rod: bool, // Can be used for fishing when held
    #[serde(default)]
    pub bait: bool, // Can be used as fishing bait
    #[serde(default)]
    pub foraging_tool: bool, // Can be used as foraging tool (uses quality for bonus)
    #[serde(default)]
    pub waterproof: bool, // Protects from rain/water when worn
    #[serde(default)]
    pub provides_warmth: bool, // Radiates warmth to room (campfire, fireplace)
    #[serde(default)]
    pub reduces_glare: bool, // Reduces bright light penalty (sunglasses)
    #[serde(default)]
    pub medical_tool: bool, // Can be used for medical treatment
    #[serde(default)]
    pub preserves_contents: bool, // Container preserves food inside (fridge/freezer)
    #[serde(default)]
    pub death_only: bool, // Only visible in corpse after death
    #[serde(default)]
    pub atm: bool, // Functions as an ATM for banking
    // Corpse system fields
    #[serde(default)]
    pub is_corpse: bool, // This item is a corpse container
    #[serde(default)]
    pub corpse_owner: String, // Name of the dead character/mobile
    #[serde(default)]
    pub corpse_created_at: i64, // Unix timestamp when corpse was created
    #[serde(default)]
    pub corpse_is_player: bool, // true = 1hr decay, false = 10min decay
    #[serde(default)]
    pub corpse_gold: i64, // Gold carried by the corpse
    #[serde(default)]
    pub corpse_source_vnum: Option<String>, // Source mob prototype vnum (for animate_dead). None on player/legacy corpses.
    #[serde(default)]
    pub broken: bool, // Broken arrows/bolts cannot be used as ammo
    // Gardening system flags
    #[serde(default)]
    pub plant_pot: bool, // Can be used as a planting container
    // Stealth/thievery system flags
    #[serde(default)]
    pub lockpick: bool, // Can be used to pick locks
    #[serde(default)]
    pub is_skinned: bool, // Corpse has been butchered/skinned
    // Water system flags
    #[serde(default)]
    pub boat: bool, // Allows traversing deep_water rooms when in inventory
    // Buried treasure system
    #[serde(default)]
    pub buried: bool, // Hidden in a dirt_floor room until dug up
    #[serde(default)]
    pub can_dig: bool, // Held/equipped item allows player to dig
    #[serde(default)]
    pub detect_buried: bool, // Surfaces a hint when buried items are nearby
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "type", content = "id")]
pub enum ItemLocation {
    #[default]
    Nowhere,
    Room(Uuid),
    Inventory(String),
    Equipped(String),
    Container(Uuid),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemData {
    pub id: Uuid,
    pub name: String,
    pub short_desc: String,
    pub long_desc: String,
    /// Owning area for sandbox / permission checks. Orphans (None) are
    /// editable by any builder; once stamped, only `can_edit_area` callers
    /// may mutate or delete the prototype.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area_id: Option<Uuid>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub item_type: ItemType,
    // Categories for recipe ingredient/tool matching (e.g., "flour", "stick", "bamboo")
    #[serde(default, deserialize_with = "deserialize_categories", alias = "category")]
    pub categories: Vec<String>,
    // Recipe ID this item teaches when read/used (for recipe books/scrolls)
    #[serde(default)]
    pub teaches_recipe: Option<String>,
    // Spell ID this item teaches when read (for spell scrolls)
    #[serde(default)]
    pub teaches_spell: Option<String>,
    // Long-form readable body (ascii maps, tutorials, in-world documents).
    // Authored via `oedit <id> note` multi-line editor; surfaced by `read`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_content: Option<String>,
    // Bulletin board (ItemType::Board) gating. Posts live in the `boards`
    // sled tree keyed by this item's prototype vnum.
    #[serde(default)]
    pub board_read_admin_only: bool,
    #[serde(default)]
    pub board_write_admin_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board_max_messages: Option<i32>,
    // Epoch seconds when this item was donated. Presence is the gate
    // for the donation-decay tick (see src/ticks/donation.rs); cleared
    // when a player picks the item up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub donated_at: Option<i64>,
    // Sub-keyword lore revealed via `look <keyword>` against this item.
    #[serde(default)]
    pub extra_descs: Vec<ExtraDesc>,
    #[serde(default)]
    pub wear_locations: Vec<WearLocation>,
    #[serde(default)]
    pub armor_class: Option<i32>,
    /// CircleMUD APPLY_HITROLL parity: flat to-hit bonus while equipped (any slot).
    #[serde(default)]
    pub hit_bonus: i32,
    /// CircleMUD APPLY_DAMROLL parity: flat damage bonus while equipped (any slot).
    #[serde(default)]
    pub damage_bonus: i32,
    /// CircleMUD APPLY_MAXHIT parity: bonus to max HP while equipped.
    #[serde(default)]
    pub max_hp_bonus: i32,
    /// CircleMUD APPLY_MAXMANA parity: bonus to max mana while equipped.
    #[serde(default)]
    pub max_mana_bonus: i32,
    /// CircleMUD ITEM_LIGHT capacity hours: 0 = permanent (default), N>0 = hours of
    /// burn time remaining while equipped lit. When the light tick decrements to 0,
    /// `flags.provides_light` is cleared and the holder sees a "burns out" message.
    #[serde(default)]
    pub light_hours_remaining: i32,
    /// CircleMUD POTION/WAND/STAFF parity: spell to fire when the item is used
    /// (`quaff` / `zap` / `brandish`). `None` for non-spell items.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cast_on_use: Option<CastOnUse>,
    /// Body parts this armor protects (for armor items)
    #[serde(default)]
    pub protects: Vec<BodyPart>,
    #[serde(default)]
    pub flags: ItemFlags,
    #[serde(default)]
    pub weight: i32,
    #[serde(default)]
    pub value: i32,
    #[serde(default)]
    pub location: ItemLocation,
    // Weapon fields
    #[serde(default)]
    pub damage_dice_count: i32,
    #[serde(default)]
    pub damage_dice_sides: i32,
    #[serde(default)]
    pub damage_type: DamageType,
    #[serde(default)]
    pub two_handed: bool,
    /// Weapon skill used by this weapon (for combat XP)
    #[serde(default)]
    pub weapon_skill: Option<WeaponSkill>,
    /// Effects rolled per landed hit while this weapon is wielded.
    /// See `OnHitEffect` for dispatch (bleeding -> wounds, elemental -> ongoing_effects, status -> active_buffs).
    #[serde(default)]
    pub on_hit_effects: Vec<OnHitEffect>,
    // Container fields (ItemType::Container)
    #[serde(default)]
    pub container_contents: Vec<Uuid>,
    #[serde(default)]
    pub container_max_items: i32,
    #[serde(default)]
    pub container_max_weight: i32,
    #[serde(default)]
    pub container_closed: bool,
    #[serde(default)]
    pub container_locked: bool,
    #[serde(default)]
    pub container_key_vnum: Option<String>,
    // Weight reduction when worn (0-100 percent, e.g., 50 = contents weigh 50% when worn)
    #[serde(default)]
    pub weight_reduction: i32,
    // Liquid Container fields (ItemType::LiquidContainer)
    #[serde(default)]
    pub liquid_type: LiquidType,
    #[serde(default)]
    pub liquid_current: i32,
    #[serde(default)]
    pub liquid_max: i32,
    #[serde(default)]
    pub liquid_poisoned: bool,
    #[serde(default)]
    pub liquid_effects: Vec<ItemEffect>,
    // Food fields (ItemType::Food)
    #[serde(default)]
    pub food_nutrition: i32,
    #[serde(default)]
    pub food_poisoned: bool,
    #[serde(default)]
    pub food_spoil_duration: i64,
    #[serde(default)]
    pub food_created_at: Option<i64>,
    #[serde(default)]
    pub food_effects: Vec<ItemEffect>,
    #[serde(default)]
    pub food_spoilage_points: f64, // 0.0 = fresh, 1.0 = spoiled
    #[serde(default)]
    pub preservation_level: i32, // 0=none, 1=fridge/cool, 2=freezer/frozen (for containers)
    // Level requirement and stat bonuses
    #[serde(default)]
    pub level_requirement: i32,
    #[serde(default)]
    pub stat_str: i32,
    #[serde(default)]
    pub stat_dex: i32,
    #[serde(default)]
    pub stat_con: i32,
    #[serde(default)]
    pub stat_int: i32,
    #[serde(default)]
    pub stat_wis: i32,
    #[serde(default)]
    pub stat_cha: i32,
    // Insulation for temperature/weather system
    #[serde(default)]
    pub insulation: i32, // 0-100 scale for warmth
    // Prototype fields
    #[serde(default)]
    pub is_prototype: bool,
    #[serde(default)]
    pub vnum: Option<String>,
    // World-wide cap on live (non-prototype) instances of this vnum.
    // None = unlimited. Some(n) = refuse spawn when count >= n.
    // `flags.unique` is sugar for Some(1).
    #[serde(default)]
    pub world_max_count: Option<i32>,
    // Item triggers
    #[serde(default)]
    pub triggers: Vec<ItemTrigger>,
    // Vending machine fields (requires flags.vending = true)
    #[serde(default)]
    pub vending_stock: Vec<String>, // Vnums for infinite stock
    #[serde(default = "default_vending_sell_rate")]
    pub vending_sell_rate: i32, // % charged when selling (default 150)
    // Generic quality field (0-100, used by fishing rods, bait, etc.)
    #[serde(default)]
    pub quality: i32,
    // Bait-specific fields (requires flags.bait = true)
    #[serde(default)]
    pub bait_uses: i32, // Uses remaining (0 = infinite)
    // Combat system - armor degradation
    /// Armor holes from combat damage (0-3, destroyed at 3)
    #[serde(default)]
    pub holes: i32,
    // Medical tool fields (requires flags.medical_tool = true)
    #[serde(default)]
    pub medical_tier: i32, // 1=basic, 2=intermediate, 3=advanced
    #[serde(default)]
    pub medical_uses: i32, // 0 = reusable, >0 = consumable uses
    #[serde(default)]
    pub treats_wound_types: Vec<String>, // ["cut", "puncture", "burn", etc.]
    #[serde(default)]
    pub max_treatable_wound: String, // "minor", "moderate", "severe", "critical"
    // Transport sign - links to a TransportData to show status when read
    #[serde(default)]
    pub transport_link: Option<Uuid>,
    // Ammunition fields (for both weapons with caliber and ammunition items)
    #[serde(default)]
    pub caliber: Option<String>, // "arrow", "bolt", "9mm", "5.56mm"
    #[serde(default)]
    pub ammo_count: i32, // Stack size for ammunition items
    #[serde(default)]
    pub ammo_damage_bonus: i32, // Quality bonus to damage
    // Crossbow/Firearm fields (internal magazine weapons)
    #[serde(default)]
    pub ranged_type: Option<String>, // "bow", "crossbow", "firearm"
    #[serde(default)]
    pub magazine_size: i32, // weapon capacity (crossbow=1, pistol=15, etc.)
    #[serde(default)]
    pub loaded_ammo: i32, // currently loaded rounds
    #[serde(default)]
    pub loaded_ammo_bonus: i32, // ammo_damage_bonus captured at reload time
    #[serde(default)]
    pub loaded_ammo_vnum: Option<String>, // vnum of loaded ammo prototype (for unload)
    #[serde(default)]
    pub fire_mode: String, // current: "single", "burst", "auto"
    #[serde(default)]
    pub supported_fire_modes: Vec<String>, // which modes this weapon supports
    #[serde(default)]
    pub noise_level: String, // "silent", "quiet", "normal", "loud" or "" for default
    // Special ammo effect payload (ammunition items)
    #[serde(default)]
    pub ammo_effect_type: String, // "fire", "cold", "poison", "acid", or ""
    #[serde(default)]
    pub ammo_effect_duration: i32,
    #[serde(default)]
    pub ammo_effect_damage: i32,
    // Captured at reload for magazine weapons
    #[serde(default)]
    pub loaded_ammo_effect_type: String,
    #[serde(default)]
    pub loaded_ammo_effect_duration: i32,
    #[serde(default)]
    pub loaded_ammo_effect_damage: i32,
    // Attachment properties (for attachment items)
    #[serde(default)]
    pub attachment_slot: String, // "scope", "suppressor", "magazine", "accessory"
    #[serde(default)]
    pub attachment_accuracy_bonus: i32,
    #[serde(default)]
    pub attachment_noise_reduction: i32,
    #[serde(default)]
    pub attachment_magazine_bonus: i32,
    #[serde(default)]
    pub attachment_compatible_types: Vec<String>,
    // Gardening system fields
    /// Plant prototype vnum this seed creates (for seed items)
    #[serde(default)]
    pub plant_prototype_vnum: String,
    /// Duration of fertilizer effect in game hours (for fertilizer items)
    #[serde(default)]
    pub fertilizer_duration: i64,
    /// Infestation type this item treats: "aphids", "blight", "root_rot", "frost", or "all"
    #[serde(default)]
    pub treats_infestation: String,
    /// DG Scripts persistent vars (see MobileData.dg_vars).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub dg_vars: HashMap<String, String>,
}

fn default_vending_sell_rate() -> i32 {
    150
}

impl ItemData {
    pub fn new(name: String, short_desc: String, long_desc: String) -> Self {
        ItemData {
            id: Uuid::new_v4(),
            name,
            short_desc,
            long_desc,
            area_id: None,
            keywords: Vec::new(),
            item_type: ItemType::Misc,
            categories: Vec::new(),
            teaches_recipe: None,
            teaches_spell: None,
            note_content: None,
            board_read_admin_only: false,
            board_write_admin_only: false,
            board_max_messages: None,
            donated_at: None,
            extra_descs: Vec::new(),
            wear_locations: Vec::new(),
            armor_class: None,
            hit_bonus: 0,
            damage_bonus: 0,
            max_hp_bonus: 0,
            max_mana_bonus: 0,
            light_hours_remaining: 0,
            cast_on_use: None,
            protects: Vec::new(),
            holes: 0,
            flags: ItemFlags::default(),
            weight: 0,
            value: 0,
            location: ItemLocation::Nowhere,
            damage_dice_count: 0,
            damage_dice_sides: 0,
            damage_type: DamageType::default(),
            two_handed: false,
            weapon_skill: None,
            on_hit_effects: Vec::new(),
            // Container fields
            container_contents: Vec::new(),
            container_max_items: 0,
            container_max_weight: 0,
            container_closed: false,
            container_locked: false,
            container_key_vnum: None,
            weight_reduction: 0,
            // Liquid container fields
            liquid_type: LiquidType::default(),
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
            level_requirement: 0,
            stat_str: 0,
            stat_dex: 0,
            stat_con: 0,
            stat_int: 0,
            stat_wis: 0,
            stat_cha: 0,
            insulation: 0,
            is_prototype: false,
            vnum: None,
            world_max_count: None,
            triggers: Vec::new(),
            // Vending machine fields
            vending_stock: Vec::new(),
            vending_sell_rate: 150,
            // Quality and bait fields
            quality: 0,
            bait_uses: 0,
            // Medical tool fields
            medical_tier: 0,
            medical_uses: 0,
            treats_wound_types: Vec::new(),
            max_treatable_wound: String::new(),
            // Transport sign
            transport_link: None,
            // Ammunition fields
            caliber: None,
            ammo_count: 0,
            ammo_damage_bonus: 0,
            // Crossbow/Firearm fields
            ranged_type: None,
            magazine_size: 0,
            loaded_ammo: 0,
            loaded_ammo_bonus: 0,
            loaded_ammo_vnum: None,
            fire_mode: String::new(),
            supported_fire_modes: Vec::new(),
            noise_level: String::new(),
            // Special ammo effect fields
            ammo_effect_type: String::new(),
            ammo_effect_duration: 0,
            ammo_effect_damage: 0,
            loaded_ammo_effect_type: String::new(),
            loaded_ammo_effect_duration: 0,
            loaded_ammo_effect_damage: 0,
            // Attachment fields
            attachment_slot: String::new(),
            attachment_accuracy_bonus: 0,
            attachment_noise_reduction: 0,
            attachment_magazine_bonus: 0,
            attachment_compatible_types: Vec::new(),
            // Gardening fields
            plant_prototype_vnum: String::new(),
            fertilizer_duration: 0,
            treats_infestation: String::new(),
            dg_vars: HashMap::new(),
        }
    }

    pub fn sync_flag_categories(&mut self) {
        const MAGICAL: &str = "magical";
        if self.flags.magical
            && !self
                .categories
                .iter()
                .any(|c| c.eq_ignore_ascii_case(MAGICAL))
        {
            self.categories.push(MAGICAL.to_string());
        }
    }
}

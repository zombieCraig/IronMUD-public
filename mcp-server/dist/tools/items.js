// Shared JSONSchema for builder-facing item flags. Every property is optional;
// unmentioned flags are preserved on update_item and default to false on create_item.
const itemFlagsSchema = {
    type: "object",
    description: "Builder flags. All optional — omit to leave unchanged. See ItemFlags in src/types/mod.rs for behaviour.",
    properties: {
        no_drop: { type: "boolean" },
        no_get: { type: "boolean" },
        no_remove: { type: "boolean" },
        invisible: { type: "boolean" },
        glow: { type: "boolean" },
        hum: { type: "boolean" },
        magical: { type: "boolean", description: "Reveals (magical aura) cue when viewer has detect_magic" },
        no_sell: { type: "boolean" },
        no_donate: { type: "boolean", description: "Cannot be donated via the `donate` command (CircleMUD ITEM_NODONATE)" },
        unique: { type: "boolean" },
        quest_item: { type: "boolean" },
        vending: { type: "boolean", description: "Functions as a vending machine" },
        provides_light: { type: "boolean", description: "Gives light when equipped/wielded" },
        night_vision: { type: "boolean", description: "Grants night vision when equipped (CircleMUD AFF_INFRAVISION)" },
        fishing_rod: { type: "boolean" },
        bait: { type: "boolean" },
        foraging_tool: { type: "boolean" },
        waterproof: { type: "boolean", description: "Protects from rain/water when worn" },
        provides_warmth: { type: "boolean", description: "Radiates warmth (campfire) or insulates when worn" },
        reduces_glare: { type: "boolean", description: "Reduces bright-light penalty (sunglasses)" },
        medical_tool: { type: "boolean", description: "Can be used for medical treatment" },
        preserves_contents: { type: "boolean", description: "Container preserves food inside (fridge/freezer)" },
        death_only: { type: "boolean", description: "Only visible in corpse after death" },
        atm: { type: "boolean", description: "Functions as an ATM for banking" },
        broken: { type: "boolean", description: "Broken arrows/bolts cannot be used as ammo" },
        plant_pot: { type: "boolean" },
        lockpick: { type: "boolean", description: "Can be used to pick locks" },
        is_skinned: { type: "boolean", description: "Corpse has been butchered/skinned" },
        boat: { type: "boolean", description: "Allows traversing deep_water rooms when in inventory" },
        buried: { type: "boolean", description: "Hidden in a dirt_floor room until dug up" },
        can_dig: { type: "boolean", description: "Held/equipped item lets the player dig in dirt_floor rooms" },
        detect_buried: { type: "boolean", description: "Surfaces a hint when buried items are in the room" },
        anti_good: { type: "boolean", description: "Refuses wear by anyone with morality > 24 (CircleMUD ITEM_ANTI_GOOD)" },
        anti_evil: { type: "boolean", description: "Refuses wear by anyone with morality < -24 (CircleMUD ITEM_ANTI_EVIL)" },
        anti_neutral: { type: "boolean", description: "Refuses wear by anyone with -24 <= morality <= 24 (CircleMUD ITEM_ANTI_NEUTRAL)" },
    },
};
// Per-hit effects rolled when a wielded weapon (or mob's natural attack) lands a hit.
// Dispatch:
//   `bleeding`  -> wound bleeding severity = magnitude on a random body part (duration ignored)
//   `fire` | `poison` | `cold` | `acid` | `lightning` -> push OngoingEffect
//        with damage_per_round=magnitude, rounds_remaining=duration
//   anything else -> resolved via EffectType (sleep, blind, slow, curse, …) and
//        applied as an ActiveBuff with magnitude + remaining_secs=duration.
//        Honours mob immunity flags (no_sleep, no_blind, no_charm).
// Passing this REPLACES the existing on_hit_effects list.
export const onHitEffectsSchema = {
    type: "array",
    description: "Per-hit effects rolled on every landed melee/ranged hit. Each entry is rolled independently. " +
        "Dispatch by `effect`: 'bleeding' → wound severity (duration ignored); " +
        "'fire'|'poison'|'cold'|'acid'|'lightning' → ongoing DOT (magnitude=damage/round, duration=rounds); " +
        "anything else → resolved via EffectType ('sleep','blind','slow','curse',...) and applied as an ActiveBuff " +
        "(magnitude, duration in seconds), gated by mob immunity flags. Passing this REPLACES the existing list.",
    items: {
        type: "object",
        properties: {
            effect: {
                type: "string",
                description: "Effect kind: 'bleeding', elemental ('fire'|'poison'|'cold'|'acid'|'lightning'), or buff effect type.",
            },
            chance: {
                type: "number",
                description: "Procurement chance, 1-100. Rolled per hit; <=0 disables, >100 clamped to 100.",
            },
            magnitude: {
                type: "number",
                description: "Bleeding: wound severity. Elemental: damage per round. Buff: magnitude (effect-specific).",
            },
            duration: {
                type: "number",
                description: "Bleeding: ignored. Elemental: rounds. Buff: seconds. -1 not supported here (use scripts for permanent buffs).",
            },
        },
        required: ["effect", "chance", "magnitude", "duration"],
    },
};
export const itemToolDefinitions = [
    {
        name: "list_items",
        description: "List items, optionally filtered by type",
        inputSchema: {
            type: "object",
            properties: {
                limit: { type: "number", default: 100 },
                offset: { type: "number", default: 0 },
                item_type: {
                    type: "string",
                    enum: ["misc", "armor", "weapon", "container", "liquid_container", "food", "key", "gold", "ammunition", "potion", "wand", "staff", "note", "pen", "tool", "tattoo"],
                },
            },
        },
    },
    {
        name: "list_item_prototypes",
        description: "List only item prototypes (templates). WARNING: Returns full entity data. Prefer list_item_prototypes_summary for discovery.",
        inputSchema: {
            type: "object",
            properties: {},
        },
    },
    {
        name: "list_item_prototypes_summary",
        description: "List item prototype summaries (compact: vnum, name, type, weight, value). Use for discovery, then get_item by vnum for detail.",
        inputSchema: {
            type: "object",
            properties: {
                vnum_prefix: {
                    type: "string",
                    description: "Filter by vnum prefix (e.g., 'training' to match 'training:*')",
                },
            },
        },
    },
    {
        name: "get_item",
        description: "Get an item by UUID or vnum",
        inputSchema: {
            type: "object",
            properties: {
                identifier: { type: "string", description: "Item UUID or vnum" },
            },
            required: ["identifier"],
        },
    },
    {
        name: "create_item",
        description: "Create a new item prototype",
        inputSchema: {
            type: "object",
            properties: {
                name: { type: "string", description: "Item name" },
                short_desc: { type: "string", description: "Short description when on ground" },
                long_desc: { type: "string", description: "Full description when examined" },
                vnum: { type: "string", description: "Unique vnum (e.g., 'weapons:iron_sword')" },
                area_id: { type: "string", description: "Optional owning area UUID. Caller must have edit rights on it (per AreaPermission). Omit/empty for an orphan prototype editable by any builder." },
                keywords: { type: "array", items: { type: "string" } },
                item_type: {
                    type: "string",
                    enum: ["misc", "armor", "weapon", "container", "liquid_container", "food", "key", "gold", "ammunition", "potion", "wand", "staff", "note", "pen", "tool", "tattoo"],
                },
                weight: { type: "number", default: 1 },
                value: { type: "number", default: 0 },
                categories: {
                    type: "array",
                    items: { type: "string" },
                    description: "Crafting / shop-filter categories (e.g., 'flowers', 'herbs', 'leather'). Used by shopkeepers' shop_buys_categories filter and by crafting recipes.",
                },
                wear_location: {
                    type: "string",
                    enum: ["head", "neck", "shoulders", "back", "torso", "waist", "ears",
                        "wielded", "offhand", "ready",
                        "leftarm", "rightarm", "leftwrist", "rightwrist",
                        "lefthand", "righthand", "leftfinger", "rightfinger",
                        "leftleg", "rightleg", "leftankle", "rightankle",
                        "leftfoot", "rightfoot"],
                },
                damage_dice_count: { type: "number", description: "For weapons: dice count (e.g., 2 in 2d6)" },
                damage_dice_sides: { type: "number", description: "For weapons: dice sides (e.g., 6 in 2d6)" },
                damage_type: {
                    type: "string",
                    enum: ["bludgeoning", "slashing", "piercing", "fire", "cold", "lightning", "poison", "acid"],
                },
                armor_class: { type: "number", description: "For armor: AC bonus" },
                affects: {
                    type: "array",
                    description: "Equip-time affects stamped onto the wearer's active_buffs at wear time and stripped on remove. Replaces the legacy hit_bonus / damage_bonus / max_hp_bonus / max_mana_bonus / stat_str..cha fields. Use `damage_resistance` with `damage_type` for typed resistance (e.g. acid, fire), and `status_resistance` with `vs_effect` for graded saves (e.g. sleep, charmed, or '*' for all status effects). Cursed-item idiom: add `poison` mag=1 to apply a permanent DoT to the wearer.",
                    items: {
                        type: "object",
                        properties: {
                            effect_type: { type: "string", description: "Snake_case EffectType (e.g. 'strength_boost', 'hit_bonus', 'damage_bonus', 'max_hp_bonus', 'max_mana_bonus', 'damage_resistance', 'status_resistance', 'night_vision', 'detect_invisible', 'sanctuary', 'damage_reduction', 'poison')" },
                            magnitude: { type: "number", description: "Effect magnitude (percent for resistances, flat bonus otherwise). Negatives allowed (e.g. cursed -2 strength)." },
                            damage_type: { type: "string", description: "Required iff effect_type='damage_resistance'. One of bludgeoning, slashing, piercing, fire, cold, lightning, poison, acid, bite, ballistic, arcane, sunlight, holy." },
                            vs_effect: { type: "string", description: "Required iff effect_type='status_resistance'. Snake_case EffectType being warded, or '*' for all status effects." },
                        },
                        required: ["effect_type"],
                    },
                },
                light_hours_remaining: { type: "number", description: "ITEM_LIGHT capacity hours: 0 = permanent, N>0 = remaining hours of light when equipped lit (decrements per game hour, switches off at 0)" },
                cast_on_use: {
                    type: "object",
                    description: "POTION/WAND/STAFF cast-on-use spell. Bound to item_type potion/wand/staff. Wands/staves require magic skill >= min_level; potions are universal.",
                    properties: {
                        spell: { type: "string", description: "Spell ID from spells_fantasy.json (e.g., 'magic_missile')" },
                        min_level: { type: "number", description: "Magic skill level required (0 = none, common for potions)" },
                        charges: { type: "number", description: "Current charges remaining (potions ignore; wands/staves consume one per use)" },
                        max_charges: { type: "number", description: "Maximum charges (display + future regen)" },
                        cooldown_secs: { type: "number", description: "Per-item cooldown override in seconds (overrides spell's own cooldown for item-cast firings; 0 or omitted = use spell default)" },
                    },
                    required: ["spell"],
                },
                flags: itemFlagsSchema,
                caliber: { type: "string", description: "Firearm caliber (e.g., '9mm', '5.56')" },
                ranged_type: { type: "string", description: "Ranged weapon type (e.g., 'pistol', 'rifle')" },
                magazine_size: { type: "number", description: "Magazine capacity" },
                fire_mode: { type: "string", description: "Default fire mode (e.g., 'semi', 'auto', 'burst')" },
                supported_fire_modes: { type: "array", items: { type: "string" }, description: "All supported fire modes" },
                noise_level: { type: "string", description: "Noise level (e.g., 'loud', 'quiet', 'suppressed')" },
                two_handed: { type: "boolean", description: "Whether weapon requires two hands" },
                weapon_skill: {
                    type: "string",
                    enum: ["short_blades", "long_blades", "short_blunt", "long_blunt", "polearms", "unarmed", "ranged"],
                    description: "Weapon skill category",
                },
                ammo_count: { type: "number", description: "Ammo: rounds in this ammo item" },
                ammo_damage_bonus: { type: "number", description: "Ammo: bonus damage per round" },
                attachment_slot: { type: "string", description: "Attachment: which slot this attaches to" },
                attachment_accuracy_bonus: { type: "number", description: "Attachment: accuracy bonus" },
                attachment_noise_reduction: { type: "number", description: "Attachment: noise reduction" },
                attachment_magazine_bonus: { type: "number", description: "Attachment: magazine size bonus" },
                plant_prototype_vnum: { type: "string", description: "For seeds: plant prototype vnum this seed grows into" },
                fertilizer_duration: { type: "number", description: "For fertilizer: duration in game hours" },
                treats_infestation: { type: "string", description: "For pest treatment: type treated (aphids, blight, root_rot, frost, all)" },
                liquid_type: { type: "string", description: "For liquid_container: liquid type (water, ale, wine, beer, spirits, alcohol, milk, juice, tea, coffee, etc.). Default effects auto-applied unless `liquid_effects` is also passed." },
                liquid_current: { type: "number", description: "For liquid_container: current sips" },
                liquid_max: { type: "number", description: "For liquid_container: maximum sips" },
                liquid_effects: {
                    type: "array",
                    description: "For liquid_container: explicit effects when drunk. If omitted, defaults for the liquid_type are auto-applied. If provided, overrides defaults entirely.",
                    items: {
                        type: "object",
                        properties: {
                            effect_type: { type: "string", description: "Effect type: heal, poison, stamina_restore, mana_restore, satiated, quenched, drunk, str_boost, dex_boost, con_boost, int_boost, wis_boost, cha_boost, haste, slow, invisibility, detect_invisible, regeneration" },
                            magnitude: { type: "number", description: "Effect strength" },
                            duration: { type: "number", description: "Duration in seconds (0 = instant)" },
                        },
                        required: ["effect_type", "magnitude"],
                    },
                },
                medical_tier: { type: "number", description: "Medical tier: 1=basic, 2=intermediate, 3=advanced" },
                medical_uses: { type: "number", description: "Medical uses: 0=reusable, >0=consumable" },
                treats_wound_types: { type: "array", items: { type: "string" }, description: "Wound types this item treats (e.g., 'cut', 'burn', 'fracture')" },
                food_nutrition: { type: "number", description: "For food: nutrition value (how filling)" },
                food_spoil_duration: { type: "number", description: "For food: spoilage time in game hours (0 = never spoils)" },
                food_effects: {
                    type: "array",
                    description: "For food: effects when eaten (e.g., heal, satiated, stamina_restore, mana_restore, poison, str/dex/con/int/wis/cha_boost, haste, slow, regeneration)",
                    items: {
                        type: "object",
                        properties: {
                            effect_type: { type: "string", description: "Effect type: heal, poison, stamina_restore, mana_restore, satiated, quenched, drunk, str_boost, dex_boost, con_boost, int_boost, wis_boost, cha_boost, haste, slow, invisibility, detect_invisible, regeneration" },
                            magnitude: { type: "number", description: "Effect strength" },
                            duration: { type: "number", description: "Duration in seconds (0 = instant)" },
                        },
                        required: ["effect_type", "magnitude"],
                    },
                },
                note_content: { type: "string", description: "Long-form readable body (use \\n for line breaks). Any item with this becomes readable via the `read` command; ANSI and whitespace are preserved. Max 32 KB." },
                board_read_admin_only: { type: "boolean", description: "For ItemType `board` only: when true, only admins can `board list`/`read`. Mirrors CircleMUD immortal-board access (gen_board.c)." },
                board_write_admin_only: { type: "boolean", description: "For ItemType `board` only: when true, only admins can `board write`. Mirrors CircleMUD immortal-board access." },
                board_max_messages: { type: "number", description: "For ItemType `board` only: per-board post cap (eviction-on-overflow). Omit or 0 = engine default 60. Stock CircleMUD boards use 60." },
                container_key_vnum: { type: "string", description: "For containers: vnum of the key item that unlocks it. Any spawned copy of that prototype works." },
                world_max_count: { type: "number", description: "Cap on live (non-prototype) instances of this vnum world-wide. Omit or 0 = unlimited. `flags.unique` is sugar for 1." },
                extra_descs: {
                    type: "array",
                    description: "Sub-keyword lore revealed via `look <keyword>` against this item (e.g. `look letters` on a brass lantern). Use `add_item_extra_desc` / `remove_item_extra_desc` to mutate after creation.",
                    items: {
                        type: "object",
                        properties: {
                            keywords: { type: "array", items: { type: "string" } },
                            description: { type: "string" },
                        },
                        required: ["keywords", "description"],
                    },
                },
                on_hit_effects: onHitEffectsSchema,
            },
            required: ["name", "short_desc", "long_desc", "vnum", "item_type"],
        },
    },
    {
        name: "update_item",
        description: "Update an existing item",
        inputSchema: {
            type: "object",
            properties: {
                id: { type: "string", description: "Item UUID or vnum" },
                name: { type: "string" },
                short_desc: { type: "string" },
                long_desc: { type: "string" },
                vnum: { type: "string", description: "New vnum (must be unique)" },
                area_id: { type: "string", description: "Reassign owning area. Empty string clears the assignment back to orphan. Caller must have edit rights on both current and target areas." },
                item_type: { type: "string", enum: ["misc", "armor", "weapon", "container", "liquid_container", "food", "key", "gold", "ammunition", "potion", "wand", "staff", "note", "pen", "tool", "tattoo"], description: "Change item type" },
                keywords: { type: "array", items: { type: "string" } },
                weight: { type: "number" },
                value: { type: "number" },
                categories: {
                    type: "array",
                    items: { type: "string" },
                    description: "Crafting / shop-filter categories. Passing this replaces the existing categories list.",
                },
                flags: itemFlagsSchema,
                damage_dice_count: { type: "number", description: "For weapons: dice count (e.g., 2 in 2d6)" },
                damage_dice_sides: { type: "number", description: "For weapons: dice sides (e.g., 6 in 2d6)" },
                damage_type: {
                    type: "string",
                    enum: ["bludgeoning", "slashing", "piercing", "fire", "cold", "lightning", "poison", "acid"],
                },
                armor_class: { type: "number", description: "For armor: AC bonus" },
                affects: {
                    type: "array",
                    description: "Equip-time affects stamped onto the wearer's active_buffs at wear time and stripped on remove. Replaces the legacy hit_bonus / damage_bonus / max_hp_bonus / max_mana_bonus / stat_str..cha fields. Use `damage_resistance` with `damage_type` for typed resistance (e.g. acid, fire), and `status_resistance` with `vs_effect` for graded saves (e.g. sleep, charmed, or '*' for all status effects). Cursed-item idiom: add `poison` mag=1 to apply a permanent DoT to the wearer.",
                    items: {
                        type: "object",
                        properties: {
                            effect_type: { type: "string", description: "Snake_case EffectType (e.g. 'strength_boost', 'hit_bonus', 'damage_bonus', 'max_hp_bonus', 'max_mana_bonus', 'damage_resistance', 'status_resistance', 'night_vision', 'detect_invisible', 'sanctuary', 'damage_reduction', 'poison')" },
                            magnitude: { type: "number", description: "Effect magnitude (percent for resistances, flat bonus otherwise). Negatives allowed (e.g. cursed -2 strength)." },
                            damage_type: { type: "string", description: "Required iff effect_type='damage_resistance'. One of bludgeoning, slashing, piercing, fire, cold, lightning, poison, acid, bite, ballistic, arcane, sunlight, holy." },
                            vs_effect: { type: "string", description: "Required iff effect_type='status_resistance'. Snake_case EffectType being warded, or '*' for all status effects." },
                        },
                        required: ["effect_type"],
                    },
                },
                light_hours_remaining: { type: "number", description: "ITEM_LIGHT capacity hours: 0 = permanent, N>0 = remaining hours of light when equipped lit" },
                cast_on_use: {
                    type: "object",
                    description: "POTION/WAND/STAFF cast-on-use spell. Bound to item_type potion/wand/staff. Wands/staves require magic skill >= min_level; potions are universal.",
                    properties: {
                        spell: { type: "string", description: "Spell ID from spells_fantasy.json" },
                        min_level: { type: "number", description: "Magic skill level required (0 = none)" },
                        charges: { type: "number", description: "Current charges remaining" },
                        max_charges: { type: "number", description: "Maximum charges" },
                        cooldown_secs: { type: "number", description: "Per-item cooldown override in seconds (overrides spell's own cooldown for item-cast firings; 0 or omitted = use spell default)" },
                    },
                    required: ["spell"],
                },
                wear_location: {
                    type: "string",
                    enum: ["head", "neck", "shoulders", "back", "torso", "waist", "ears",
                        "wielded", "offhand", "ready",
                        "leftarm", "rightarm", "leftwrist", "rightwrist",
                        "lefthand", "righthand", "leftfinger", "rightfinger",
                        "leftleg", "rightleg", "leftankle", "rightankle",
                        "leftfoot", "rightfoot"],
                },
                weapon_skill: {
                    type: "string",
                    enum: ["short_blades", "long_blades", "short_blunt", "long_blunt", "polearms", "unarmed", "ranged"],
                },
                caliber: { type: "string" },
                ranged_type: { type: "string" },
                magazine_size: { type: "number" },
                fire_mode: { type: "string" },
                supported_fire_modes: { type: "array", items: { type: "string" } },
                noise_level: { type: "string" },
                two_handed: { type: "boolean" },
                ammo_count: { type: "number" },
                ammo_damage_bonus: { type: "number" },
                attachment_slot: { type: "string" },
                attachment_accuracy_bonus: { type: "number" },
                attachment_noise_reduction: { type: "number" },
                attachment_magazine_bonus: { type: "number" },
                plant_prototype_vnum: { type: "string", description: "For seeds: plant prototype vnum this seed grows into" },
                fertilizer_duration: { type: "number", description: "For fertilizer: duration in game hours" },
                treats_infestation: { type: "string", description: "For pest treatment: type treated (aphids, blight, root_rot, frost, all)" },
                liquid_type: { type: "string", description: "For liquid_container: liquid type. Changing the type always re-applies that type's default effects, replacing any stale effects from the previous type. Pass `liquid_effects` alongside to override with custom effects." },
                liquid_current: { type: "number", description: "For liquid_container: current sips" },
                liquid_max: { type: "number", description: "For liquid_container: maximum sips" },
                liquid_effects: {
                    type: "array",
                    description: "For liquid_container: explicit effects, overriding any defaults. Applied AFTER liquid_type changes, so if passed together it wins over the type's defaults.",
                    items: {
                        type: "object",
                        properties: {
                            effect_type: { type: "string", description: "Effect type (e.g., drunk, quenched, stamina_restore, mana_restore, heal, poison)" },
                            magnitude: { type: "number", description: "Effect strength" },
                            duration: { type: "number", description: "Duration in seconds (0 = instant)" },
                        },
                        required: ["effect_type", "magnitude"],
                    },
                },
                medical_tier: { type: "number", description: "Medical tier: 1=basic, 2=intermediate, 3=advanced" },
                medical_uses: { type: "number", description: "Medical uses: 0=reusable, >0=consumable" },
                treats_wound_types: { type: "array", items: { type: "string" }, description: "Wound types this item treats" },
                food_nutrition: { type: "number", description: "For food: nutrition value" },
                food_spoil_duration: { type: "number", description: "For food: spoilage time in game hours (0 = never)" },
                food_effects: {
                    type: "array",
                    description: "For food: effects when eaten",
                    items: {
                        type: "object",
                        properties: {
                            effect_type: { type: "string", description: "Effect type (e.g., heal, satiated, stamina_restore, mana_restore)" },
                            magnitude: { type: "number", description: "Effect strength" },
                            duration: { type: "number", description: "Duration in seconds (0 = instant)" },
                        },
                        required: ["effect_type", "magnitude"],
                    },
                },
                note_content: { type: "string", description: "Long-form readable body (use \\n for line breaks). Any item with this becomes readable via the `read` command; ANSI and whitespace are preserved. Empty string clears. Max 32 KB." },
                board_read_admin_only: { type: "boolean", description: "For ItemType `board` only: when true, only admins can `board list`/`read`." },
                board_write_admin_only: { type: "boolean", description: "For ItemType `board` only: when true, only admins can `board write`." },
                board_max_messages: { type: "number", description: "For ItemType `board` only: per-board post cap. 0 or negative clears (engine default 60)." },
                container_key_vnum: { type: "string", description: "For containers: vnum of the key item that unlocks it. Empty string clears. Any spawned copy of that prototype works." },
                world_max_count: { type: "number", description: "Cap on live (non-prototype) instances of this vnum world-wide. 0 or negative clears the cap (unlimited). `flags.unique` is sugar for 1." },
                extra_descs: {
                    type: "array",
                    description: "Sub-keyword lore revealed via `look <keyword>` against this item. Passing this REPLACES the existing extra_descs list. Prefer `add_item_extra_desc` / `remove_item_extra_desc` for incremental edits.",
                    items: {
                        type: "object",
                        properties: {
                            keywords: { type: "array", items: { type: "string" } },
                            description: { type: "string" },
                        },
                        required: ["keywords", "description"],
                    },
                },
                on_hit_effects: onHitEffectsSchema,
            },
            required: ["id"],
        },
    },
    {
        name: "delete_item",
        description: "Delete an item",
        inputSchema: {
            type: "object",
            properties: {
                id: { type: "string", description: "Item UUID or vnum" },
            },
            required: ["id"],
        },
    },
    {
        name: "spawn_item",
        description: "Spawn an item instance from a prototype into a room",
        inputSchema: {
            type: "object",
            properties: {
                vnum: { type: "string", description: "Item prototype vnum" },
                room_id: { type: "string", description: "Target room UUID or vnum" },
            },
            required: ["vnum", "room_id"],
        },
    },
    {
        name: "add_item_trigger",
        description: "Add a trigger script to an item prototype (e.g. wire scripts/triggers/smart_watch.rhai to on_examine).",
        inputSchema: {
            type: "object",
            properties: {
                item_id: { type: "string", description: "Item UUID or vnum" },
                trigger_type: {
                    type: "string",
                    enum: ["on_get", "on_drop", "on_use", "on_examine", "on_look", "on_prompt"],
                    description: "When the trigger fires",
                },
                script_name: {
                    type: "string",
                    description: "Script filename without extension (e.g., 'smart_watch'), or '@template' form",
                },
                chance: { type: "number", description: "Trigger probability 1-100 (default 100)" },
                args: { type: "array", items: { type: "string" }, description: "Template arguments" },
            },
            required: ["item_id", "trigger_type", "script_name"],
        },
    },
    {
        name: "remove_item_trigger",
        description: "Remove a trigger from an item prototype by index (see triggers[] in get_item).",
        inputSchema: {
            type: "object",
            properties: {
                item_id: { type: "string", description: "Item UUID or vnum" },
                index: { type: "number", description: "Zero-based index of the trigger to remove" },
            },
            required: ["item_id", "index"],
        },
    },
    {
        name: "add_item_extra_desc",
        description: "Add an extra description (sub-keyword lore) to an item, revealed when a player types `look <keyword>` against the item.",
        inputSchema: {
            type: "object",
            properties: {
                item_id: { type: "string", description: "Item UUID or vnum" },
                keywords: {
                    type: "array",
                    items: { type: "string" },
                    description: "Keywords that trigger the description",
                },
                description: { type: "string" },
            },
            required: ["item_id", "keywords", "description"],
        },
    },
    {
        name: "remove_item_extra_desc",
        description: "Remove an extra description from an item by keyword.",
        inputSchema: {
            type: "object",
            properties: {
                item_id: { type: "string", description: "Item UUID or vnum" },
                keyword: { type: "string", description: "Keyword to remove" },
            },
            required: ["item_id", "keyword"],
        },
    },
];
//# sourceMappingURL=items.js.map
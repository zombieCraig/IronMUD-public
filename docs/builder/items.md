# Item Editing

This guide covers creating and editing items using IronMUD's Online Creation (OLC) system.

## Item Commands

| Command | Usage | Description |
|---------|-------|-------------|
| `oedit create` | `oedit create <name>` | Create a new item prototype |
| `oedit` | `oedit <id\|vnum> [subcommand]` | Edit item properties |
| `ilist` | `ilist` | List all items |
| `ifind` | `ifind <keyword>` | Search items by name/keywords |
| `idelete` | `idelete <item_id>` | Delete an item |
| `ospawn` | `ospawn <vnum> [room\|inv]` | Spawn item from prototype |

## Creating Items

The `oedit create` command creates a new item prototype with an auto-generated vnum:

```
> oedit create Rusty Sword
Created item prototype with vnum: rusty_sword
=== Item Properties ===
Name: Rusty Sword
ID: 550e8400-e29b-41d4-a716-446655440002
Vnum: rusty_sword
Type: Misc
...

> oedit rusty_sword type weapon
Type set to: Weapon

> oedit rusty_sword damage 2 6
Damage set to: 2d6
```

**Vnum generation rules:**
- Name is converted to lowercase
- Spaces and dashes become underscores
- Special characters are removed
- Max 24 characters
- Duplicates get a number suffix (`_2`, `_3`, etc.)

## oedit Subcommands

### Basic Properties

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `show` | `oedit <id>` | Display item properties |
| `name` | `oedit <id> name <text>` | Set item name |
| `short` | `oedit <id> short <text>` | Set short description (room view) |
| `long` | `oedit <id> long <text>` | Set long description (examine) |
| `keywords` | `oedit <id> keywords <kw>...` | Set item keywords |
| `type` | `oedit <id> type <type>` | Set item type |
| `wear` | `oedit <id> wear <loc>...` | Set wear locations |
| `weight` | `oedit <id> weight <value>` | Set item weight |
| `value` | `oedit <id> value <gold>` | Set item value |
| `level` | `oedit <id> level <value>` | Set level requirement |
| `note` | `oedit <id> note [clear]` | Edit a multi-line readable body (see [Readable Notes](#readable-notes)) |

### Combat Properties

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `ac` | `oedit <id> ac <value>` | Set armor class |
| `damage` | `oedit <id> damage <count> <sides>` | Set weapon damage (e.g., 2 6 for 2d6) |
| `damtype` | `oedit <id> damtype <type>` | Set damage type |
| `twohanded` | `oedit <id> twohanded [on\|off]` | Set two-handed weapon |
| `stat` | `oedit <id> stat <attr> <value>` | Set stat bonus (str/dex/con/int/wis/cha) |

See also: [Weapon Damage Balance](weapon-balance.md) for recommended damage dice by weapon type and setting.

### Flags and Triggers

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `flags` | `oedit <id> flags` | Show all flags |
| `flag` | `oedit <id> flag <name> [on\|off]` | Toggle item flag |
| `trigger` | `oedit <id> trigger` | Manage triggers (see [Triggers](triggers.md)) |

### Prototype Management

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `prototype` | `oedit <id> prototype [on\|off]` | Mark as prototype template |
| `vnum` | `oedit <id> vnum <name\|none>` | Set prototype vnum |
| `spawn` | `oedit <id> spawn` | Spawn copy into current room |

## Item Types

| Type | Description |
|------|-------------|
| `misc` | Generic item |
| `armor` | Wearable protection |
| `weapon` | Combat weapon |
| `container` | Holds other items |
| `liquid_container` | Holds drinks |
| `food` | Consumable food |
| `key` | Opens locks |

Set type with:
```
> oedit sword type weapon
Type set to: Weapon
```

## Wear Locations

Items can be worn in multiple slots:

| Location | Description |
|----------|-------------|
| `head` | Helmets, hats |
| `neck` | Necklaces, amulets |
| `body` | Armor, robes |
| `arms` | Arm guards |
| `hands` | Gloves |
| `waist` | Belts |
| `legs` | Leg armor |
| `feet` | Boots |
| `finger` | Rings |
| `wield` | Weapons |
| `shield` | Shields |
| `hold` | Held items |

```
> oedit ring wear finger
Wear locations set to: finger
```

## Item Flags

| Flag | Effect |
|------|--------|
| `no_drop` | Cannot be dropped |
| `no_get` | Cannot be picked up |
| `no_remove` | Cannot be removed once worn |
| `invisible` | Hidden from normal view |
| `glow` | Emits light |
| `hum` | Makes humming sound |
| `no_sell` | Cannot be sold to shops |
| `unique` | Only one can exist in world |
| `quest_item` | Special quest item |
| `death_only` | Only visible in corpse after death |
| `plant_pot` | Item serves as a plant pot (gardening) |
| `lockpick` | Can be used to pick locks (thievery skill) |
| `boat` | Allows traversing deep_water rooms when carried in inventory |
| `is_skinned` | Corpse has been butchered/skinned (set by butcher command) |

### Gardening Fields

These fields support the gardening system (see [Gardening](gardening.md)):

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `plantproto` | `oedit <id> plantproto <vnum>` | Link seed to a plant prototype |
| `fertduration` | `oedit <id> fertduration <hours>` | Set fertilizer duration in game hours |
| `treats` | `oedit <id> treats <type>` | Set infestation type this item treats (aphids, blight, root_rot, frost, all) |

```
> oedit ring flag unique on
Flag 'unique' set to: ON
```

## Damage Types

For weapons, set the damage type:

| Type | Description |
|------|-------------|
| `slash` | Slashing damage |
| `pierce` | Piercing damage |
| `blunt` | Bludgeoning damage |
| `fire` | Fire damage |
| `cold` | Cold damage |
| `lightning` | Lightning damage |
| `acid` | Acid damage |
| `poison` | Poison damage |

```
> oedit sword damtype slash
Damage type set to: slash
```

## Container Items

Containers hold other items:

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `capacity` | `oedit <id> capacity <items> <weight>` | Set limits (0=unlimited) |
| `closed` | `oedit <id> closed [on\|off]` | Set closed state |
| `locked` | `oedit <id> locked [on\|off]` | Set locked state |
| `key` | `oedit <id> key <item_id\|none>` | Set key item |

```
> oedit create Wooden Chest
> oedit wooden_chest type container
> oedit wooden_chest capacity 20 100
Max items: 20, max weight: 100
```

## Liquid Containers

Drinkable containers:

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `liquid` | `oedit <id> liquid <type> <current> <max>` | Set liquid and amount |
| `fill` | `oedit <id> fill` | Fill to maximum |
| `empty` | `oedit <id> empty` | Empty container |
| `liqpoison` | `oedit <id> liqpoison [on\|off]` | Set poisoned |
| `liqeffect` | `oedit <id> liqeffect <type> <mag> <dur>` | Add drink effect |
| `clearliqeffects` | `oedit <id> clearliqeffects` | Remove effects |

**Liquid types:** water, ale, wine, beer, alcohol, milk, juice, tea, coffee, poison, healing_potion, mana_potion, blood, oil

When you set a liquid type, **default effects are auto-applied** based on the liquid. For example, setting liquid to `coffee` auto-adds `stamina_restore(8)` and `quenched(70)`. You can override with `liqeffect`/`clearliqeffects`.

| Liquid | Auto Effects |
|--------|-------------|
| water | quenched(100) |
| ale | drunk(2), quenched(50) |
| wine | drunk(4), quenched(30) |
| beer | drunk(2), quenched(50), satiated(10) |
| alcohol | drunk(6), quenched(20) |
| milk | satiated(20), quenched(80) |
| juice | stamina_restore(5), quenched(80) |
| tea | stamina_restore(3), quenched(90) |
| coffee | stamina_restore(8), quenched(70) |
| poison | poison(10) |
| healing_potion | heal(20), quenched(30) |
| mana_potion | mana_restore(20), quenched(30) |
| blood | satiated(10) |
| oil | poison(3) |

```
> oedit create Healing Potion
> oedit healing_potion type liquid_container
> oedit healing_potion liquid healing_potion 5 5
Liquid set: healing_potion (5/5 sips) (auto-effects: heal(20), quenched(30))
```

## Food Items

Consumable food:

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `nutrition` | `oedit <id> nutrition <value>` | Set nutrition value |
| `spoil` | `oedit <id> spoil <seconds>` | Set spoilage time (0=never) |
| `foodpoison` | `oedit <id> foodpoison [on\|off]` | Set poisoned |
| `foodeffect` | `oedit <id> foodeffect <type> <mag> <dur>` | Add food effect |
| `clearfoodeffects` | `oedit <id> clearfoodeffects` | Remove effects |
| `resetfresh` | `oedit <id> resetfresh` | Reset freshness |

```
> oedit create Fresh Bread
> oedit fresh_bread type food
> oedit fresh_bread nutrition 30
> oedit fresh_bread spoil 3600
Spoils after 1 hour
```

## Effect Types

Both `liqeffect` and `foodeffect` use the same effect type system. Effects are applied when a player drinks or eats the item.

| Effect Type | Description | Magnitude | Duration |
|-------------|-------------|-----------|----------|
| `heal` | Restore HP instantly | HP amount | 0 (instant) |
| `poison` | Deal damage instantly | Damage amount | 0 (instant) |
| `stamina_restore` | Restore stamina instantly | Stamina amount | 0 (instant) |
| `mana_restore` | Restore mana instantly | Mana amount | 0 (instant) |
| `quenched` | Quench thirst | Thirst reduction | 0 (instant) |
| `satiated` | Reduce hunger | Hunger reduction | 0 (instant) |
| `drunk` | Increase inebriation | Drunk level added | Duration in secs |
| `strength_boost` | Buff strength stat | Bonus amount | Duration in secs |
| `dexterity_boost` | Buff dexterity stat | Bonus amount | Duration in secs |
| `constitution_boost` | Buff constitution stat | Bonus amount | Duration in secs |
| `intelligence_boost` | Buff intelligence stat | Bonus amount | Duration in secs |
| `wisdom_boost` | Buff wisdom stat | Bonus amount | Duration in secs |
| `charisma_boost` | Buff charisma stat | Bonus amount | Duration in secs |
| `haste` | Reduce movement stamina cost by 50% | 1 | Duration in secs |
| `slow` | Double movement stamina cost | 1 | Duration in secs |
| `invisibility` | Hide from look/who | 1 | Duration in secs |
| `detect_invisible` | See invisible players | 1 | Duration in secs |
| `regeneration` | Heal HP each regen tick | HP per tick | Duration in secs |

Timed effects (duration > 0) apply as **buffs** that tick down and expire. Same-type buffs are refreshed (higher magnitude wins). Players see a message when buffs expire.

### Examples

```
> oedit strength_elixir liqeffect strength_boost 3 300
Added effect: strength_boost (magnitude: 3, duration: 300s)

> oedit cursed_wine liqeffect slow 1 120
Added effect: slow (magnitude: 1, duration: 120s)

> oedit regen_potion liqeffect regeneration 5 60
Added effect: regeneration (magnitude: 5, duration: 60s)
```

## Prototypes and Spawning

Prototypes are templates for creating item instances:

1. **Create a prototype** with `oedit create`
2. **Configure properties** with oedit subcommands
3. **Spawn instances** with `ospawn`

```
> oedit create Health Potion
> oedit health_potion type food
> oedit health_potion nutrition 50

> ospawn health_potion inv
Spawned Health Potion into your inventory.

> ospawn health_potion room
Spawned Health Potion into the room.
```

**Prototype vs Instance:**

| Aspect | Prototype | Instance |
|--------|-----------|----------|
| `is_prototype` | `true` | `false` |
| Has vnum | Yes | No |
| Visible to players | No | Yes |
| Can be modified | Yes | Limited |
| Location | None | Room, inventory, etc. |

## Vending Machines

Items can be configured as vending machines:

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `vending stock add` | `oedit <id> vending stock add <vnum>` | Add item to stock |
| `vending stock remove` | `oedit <id> vending stock remove <vnum>` | Remove from stock |
| `vending sellrate` | `oedit <id> vending sellrate <rate>` | Set price multiplier |

See also: [Shop System](mobiles.md#shop-system) for shopkeeper NPCs.

## Readable Notes

Any item can carry a long-form readable body — ascii maps, tutorials, in-world documents, inscribed walls, detailed signs. When the body is non-empty the item becomes readable: `read <item>` prints the body verbatim (whitespace, blank lines, and ANSI are preserved), and `examine <item>` appends a `(You could read this.)` hint.

Notes compose with other readable mechanics. A note short-circuits `read` before `teaches_recipe` / `teaches_spell`, so do not put a note body on a scroll that also teaches — the note will win and the teaching path will never fire. For a "book with a preface plus a recipe," put the flavor text in `long_desc` and let `read` fire the recipe.

### Authoring

```
> oedit parchment_district_map note
Editing note for: a weathered parchment
(empty)

Commands: .h help | .l list | .c clear | .d N | .r N <text> | . save | @ cancel
Type lines to append. Blank lines, ANSI, and whitespace are preserved.

  N
 W-+-E
  S

(rough map of the district)
.save
Note saved.
```

The editor is the same multi-line buffer used for room descriptions (`redit desc`). If the note already has content, it is pre-loaded into the buffer so you can edit in place with `.l`, `.r N <text>`, `.d N`, etc. `.save` (or `.`) commits; `.cancel` (or `@`) aborts without changes. Saving with an empty buffer clears the note ("Note cleared.").

`oedit <id> note clear` wipes the body without opening the editor.

### Reading

```
> read parchment
You read a weathered parchment:
  N
 W-+-E
  S

(rough map of the district)
```

### Limits

- Bodies are capped at 32 KB. Over-cap saves are refused; trim with `.d` or `.r` and try again.
- `\r\n` and lone `\r` from API payloads are normalized to `\n`.
- Writing the body via the HTTP / MCP API: set `note_content` on `update_item` or `create_item`. Use `\n` for line breaks in the JSON string.

## Spell Scrolls

Scrolls are items that teach players a spell when read. They use the **Misc** item type with the `teaches_spell` field set to a spell ID.

### Creating a Scroll

```
> oedit create Scroll of Meteor Storm
Created item prototype with vnum: scroll_of_meteor_storm

> oedit scroll_of_meteor_storm short A charred scroll covered in arcane runes lies here.
> oedit scroll_of_meteor_storm long An ancient scroll sealed with wax. Faint traces of fire magic radiate from the parchment.
> oedit scroll_of_meteor_storm keywords scroll charred arcane
> oedit scroll_of_meteor_storm teaches_spell meteor_storm
Teaches spell set to: meteor_storm
```

The item type should remain **Misc** (the default). When a player uses `read scroll`, they learn the spell permanently and the scroll is consumed.

### Setting the Spell

Use the `teaches_spell` subcommand to assign which spell the scroll teaches:

```
> oedit <item> teaches_spell <spell_id>
```

To clear the field:
```
> oedit <item> teaches_spell none
```

### Available Spell IDs

Any spell can be placed on a scroll, though scroll-only spells like `meteor_storm` are specifically designed to be learned this way.

| Spell ID | Spell Name | Notes |
|----------|------------|-------|
| `magic_missile` | Magic Missile | |
| `light` | Light | |
| `firebolt` | Firebolt | |
| `arcane_shield` | Arcane Shield | |
| `cure_wounds` | Cure Wounds | |
| `detect_invisible` | Detect Invisible | |
| `lightning_bolt` | Lightning Bolt | |
| `invisibility` | Invisibility | |
| `dispel_magic` | Dispel Magic | |
| `haste` | Haste | |
| `greater_heal` | Greater Heal | |
| `meteor_storm` | Meteor Storm | Scroll-only (not learnable by skill) |

Scrolls for lower-level spells can be useful as shortcuts for new mages, or placed as loot in areas where players might not yet have the required magic skill.

## Related Documentation

- [Mobiles](mobiles.md) - NPC creation and shops
- [Triggers](triggers.md) - Item triggers and scripting
- [Builder Guide](../builder-guide.md) - Overview of building

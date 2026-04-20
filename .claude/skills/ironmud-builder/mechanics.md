# IronMUD Game Mechanics Reference

## Combat System

### Damage Types
- `bludgeoning` - Clubs, maces, fists
- `slashing` - Swords, axes
- `piercing` - Daggers, spears, arrows
- `ballistic` - Bullets, bolts at high velocity
- `fire` - Fire magic, flaming weapons
- `cold` - Ice magic, frost weapons
- `lightning` - Electric attacks
- `poison` - Venomous attacks
- `acid` - Corrosive attacks

### Damage Dice
Damage is expressed as "XdY" or "XdY+Z":
- `1d6` - Roll 1 six-sided die (1-6 damage)
- `2d4+2` - Roll 2 four-sided dice plus 2 (4-10 damage)

### Weapon Damage Guidelines

Weapons follow a damage scale from 1d2 (unarmed) to 2d10+2 (artifact ceiling). Ranged weapons with burst or auto fire deal their per-shot damage multiple times per round, so per-shot values must be lower than melee equivalents.

Quick reference:

| Dice | Avg | Tier |
|------|-----|------|
| 1d2 | 1.5 | Unarmed |
| 1d4 | 2.5 | Light (knife, club) |
| 1d6 | 3.5 | Standard light (shortsword, shortbow) |
| 1d8 | 4.5 | Standard martial (longsword, pistol per-shot) |
| 1d10 | 5.5 | Heavy (bastard sword, hunting rifle) |
| 1d12 | 6.5 | High-end (greataxe, magnum) |
| 2d6 | 7.0 | Two-handed (greatsword, shotgun) |
| 2d8 | 9.0 | Elite (sniper rifle) |
| 2d10 | 11.0 | Legendary (railgun) |
| 2d10+2 | 13.0 | Artifact ceiling |

See `weapon-balance.md` for full tables by setting (medieval, modern, cyberpunk) and mobile level correlation.

### Armor Class
Lower AC is better. 10 is unarmored human.
- Light armor: 12-14
- Medium armor: 14-16
- Heavy armor: 16-18+

## Room Flags

| Flag | Effect |
|------|--------|
| `dark` | Requires light source to see |
| `no_mob` | NPCs cannot enter |
| `indoors` | Protected from weather |
| `safe` | No combat allowed |
| `private` | Limited occupancy |
| `death_trap` | Instant death (use sparingly!) |
| `no_recall` | Cannot use recall spell |
| `dirt_floor` | Allows planting seeds in the ground (gardening) |
| `garden` | Thematic garden room display |
| `climate_controlled` | Skips weather/season triggers |
| `shallow_water` | Surface water: +1 stamina cost, trains swimming |
| `deep_water` | Deep water: requires boat or swimming 5+, +2 stamina cost |
| `underwater` | Submerged: requires WaterBreathing buff or drowning occurs |

## Exit Directions

Standard exits:
- `north`, `south`, `east`, `west`
- `up`, `down`

## Door Properties

| Property | Description |
|----------|-------------|
| `name` | Door name (e.g., "wooden door") |
| `is_closed` | Whether door is closed |
| `is_locked` | Whether door is locked |
| `key_id` | UUID of key that opens it |
| `keywords` | Words that target the door |

## Item Types

| Type | Use |
|------|-----|
| `misc` | General items |
| `armor` | Wearable protection |
| `weapon` | Combat weapons |
| `container` | Holds other items |
| `liquid_container` | Holds drinks |
| `food` | Consumable food |
| `ammunition` | Arrows, bolts, bullets |
| `key` | Opens locked doors |
| `gold` | Currency |

## Wear Locations

| Location | Slot |
|----------|------|
| `head` | Helmet |
| `face` | Mask |
| `neck` | Necklace |
| `body` | Armor |
| `back` | Cloak |
| `arms` | Bracers |
| `wrists` | Wristguards |
| `hands` | Gloves |
| `waist` | Belt |
| `legs` | Leggings |
| `feet` | Boots |
| `wielded` | Main weapon |
| `held` | Off-hand item |
| `shield` | Shield |
| `ready` | Quiver, ammo pouch |

## Item Flags

| Flag | Effect |
|------|--------|
| `no_drop` | Cannot be dropped |
| `no_get` | Cannot be picked up |
| `invisible` | Not visible without detect |
| `glow` | Provides light |
| `hum` | Makes humming sound |
| `no_sell` | Shopkeepers won't buy |
| `unique` | Only one can exist |
| `plant_pot` | Item serves as a plant pot (gardening) |
| `boat` | Allows traversing deep_water rooms when in inventory |

## Mobile Flags

| Flag | Effect |
|------|--------|
| `aggressive` | Attacks players on sight |
| `sentinel` | Does not wander (overridden by routine destinations) |
| `scavenger` | Picks up items from ground |
| `shopkeeper` | Can buy/sell items (only when activity is `working`) |
| `healer` | Provides healing services (only when activity is `working`) |
| `trainer` | Can train skills |
| `cowardly` | Flees combat at HP <= 25% and flees when sniped |
| `can_open_doors` | Can open/unlock doors during routine pathfinding |

## Activity States

Mobiles with daily routines cycle through activity states each game hour:

| State | Shop/Healer Available | Wander | Room Description |
|-------|-----------------------|--------|------------------|
| `working` | Yes | Suppressed by default | Normal short_desc |
| `sleeping` | No (special message) | Suppressed | "{name} is here, sleeping." |
| `patrolling` | No | Allowed | Normal short_desc |
| `off_duty` | No | Suppressed by default | Normal short_desc |
| `socializing` | No | Suppressed by default | Normal short_desc |
| `eating` | No | Suppressed by default | Normal short_desc |

## Trigger Types

### Room Triggers
| Type | When Fired |
|------|------------|
| `on_enter` | Player enters room |
| `on_exit` | Player leaves room |
| `on_command` | Player uses specific command |
| `on_say` | Player says something |
| `on_timer` | At set interval |
| `on_random` | Random chance per tick |

### Mobile Triggers
| Type | When Fired |
|------|------------|
| `on_greet` | Player enters room |
| `on_receive` | Given an item |
| `on_speech` | Player speaks |
| `on_combat_start` | Combat begins |
| `on_death` | Mobile dies |
| `on_flee` | Mobile flees combat |
| `on_timer` | At set interval |

**Template triggers:** Use `@shout` in trigger scripts to broadcast messages to adjacent rooms (e.g., a fleeing mob calling for help).

### Item Triggers
| Type | When Fired |
|------|------------|
| `on_get` | Item picked up |
| `on_drop` | Item dropped |
| `on_use` | Item used |
| `on_wear` | Item equipped |
| `on_remove` | Item unequipped |

## Spawn Point System

### Entity Types
- `mobile` - Spawn NPCs
- `item` - Spawn items

### Spawn Destinations (for dependencies)
- `inventory` - In mobile's inventory
- `equipped` - Worn/wielded by mobile
- `container` - Inside a container

### Respawn Timing
Common intervals:
- `60` - 1 minute (very fast)
- `300` - 5 minutes (standard)
- `900` - 15 minutes (slow)
- `1800` - 30 minutes (boss)
- `3600` - 1 hour (rare)

## Level Guidelines

| Level | Description | HP Range |
|-------|-------------|----------|
| 1-5 | Newbie area | 10-50 |
| 6-10 | Beginner | 40-100 |
| 11-15 | Intermediate | 80-150 |
| 16-20 | Advanced | 120-250 |
| 21+ | Expert/Boss | 200+ |

## Gardening System

### Gardening Item Fields

These fields on misc items support the gardening system:

| Field | Usage | Description |
|-------|-------|-------------|
| `plant_prototype_vnum` | Seeds | Links seed to a plant prototype vnum |
| `fertilizer_duration` | Fertilizer | Duration in game hours |
| `treats_infestation` | Pest Treatment | Type treated: `aphids`, `blight`, `root_rot`, `frost`, `all` |

### Plant Categories

`vegetable`, `herb`, `flower`, `fruit`, `grain`

### Growth Stages

`seed` → `sprout` → `seedling` → `growing` → `mature` → `flowering` → (harvest or `wilting` → `dead`)

### Infestation Types

`aphids`, `blight`, `root_rot`, `frost`

### Season Effects

- Preferred season: growth speed x1.25
- Forbidden season: growth blocked entirely
- Neutral season: normal growth rate

## Liquid Types

| Type | String | Description |
|------|--------|-------------|
| Water | `water` | Pure water |
| Ale | `ale` | Light alcoholic drink |
| Wine | `wine` | Moderate alcoholic drink |
| Beer | `beer` | Light alcoholic drink (also matches `mead`) |
| Alcohol | `alcohol` | Strong spirits |
| Milk | `milk` | Nutritious drink |
| Juice | `juice` | Fruit juice |
| Tea | `tea` | Herbal tea |
| Coffee | `coffee` | Caffeinated drink |
| Poison | `poison` | Toxic liquid |
| Healing Potion | `healing_potion` | Restores HP |
| Mana Potion | `mana_potion` | Restores mana |
| Blood | `blood` | Dark liquid |
| Oil | `oil` | Flammable liquid |

Setting a liquid type on a container auto-applies default drink effects. Builders can override with `liqeffect`/`clearliqeffects`.

## Effect Types

Used for `liqeffect` and `foodeffect` on consumable items.

| Effect | String | Description |
|--------|--------|-------------|
| Heal | `heal` | Restore HP (instant) |
| Poison | `poison` | Deal damage (instant) |
| Stamina Restore | `stamina_restore` | Restore stamina (instant) |
| Mana Restore | `mana_restore` | Restore mana (instant) |
| Quenched | `quenched` | Reduce thirst (instant) |
| Satiated | `satiated` | Reduce hunger (instant) |
| Drunk | `drunk` | Increase inebriation level |
| Strength Boost | `strength_boost` | Buff strength (timed) |
| Dexterity Boost | `dexterity_boost` | Buff dexterity (timed) |
| Constitution Boost | `constitution_boost` | Buff constitution (timed) |
| Intelligence Boost | `intelligence_boost` | Buff intelligence (timed) |
| Wisdom Boost | `wisdom_boost` | Buff wisdom (timed) |
| Charisma Boost | `charisma_boost` | Buff charisma (timed) |
| Haste | `haste` | Reduce movement stamina cost 50% (timed) |
| Slow | `slow` | Double movement stamina cost (timed) |
| Invisibility | `invisibility` | Hidden from look/who (timed) |
| Detect Invisible | `detect_invisible` | See invisible players (timed) |
| Regeneration | `regeneration` | Heal HP per regen tick (timed) |
| Water Breathing | `water_breathing` | Prevents breath depletion underwater (timed) |

## Underwater Combat Modifiers

When fighting in rooms with the `underwater` flag, damage types are modified:

| Damage Type | Modifier | Reason |
|-------------|----------|--------|
| Slashing | -25% | Water resistance slows swings |
| Bludgeoning | -25% | Water resistance reduces impact |
| Piercing | +15% | Thrusting weapons work well underwater |
| Fire | 0 (extinguished) | Fire cannot burn underwater |
| Cold | +10% | Cold conducts through water |
| Others | No change | |

## Buff System

Timed effects (duration > 0) are applied as **active buffs** on the character. Buffs:
- Tick down every 10 seconds during the regen tick
- Expire with a notification message to the player
- Same-type buffs refresh duration and take the higher magnitude
- Stat boost buffs are used via `get_effective_stat()` in combat calculations
- Haste/slow affect movement stamina costs
- Invisibility hides from `look` room listings and `who` unless viewer has `detect_invisible`

### Drunk Mechanics

The `drunk` effect increases a character's `drunk_level` (0-100). Effects:
- **drunk_level > 30**: Speech is garbled (letter substitutions, *hic* insertions)
- **drunk_level > 50**: 25% chance of stumbling into a random adjacent room when moving
- Drunk level decays by 1 per regen tick (10 seconds)

### Watering Plants with Liquids

The `water` command uses any `liquid_container` item. Liquid type affects plant watering efficiency:
- **Water**: 100% efficiency
- **Beneficial** (healing_potion, tea, juice, milk): 75% efficiency
- **Neutral** (ale, wine, beer, coffee): 50% efficiency
- **Harmful** (alcohol, poison, oil, blood): No water, deals damage to plant

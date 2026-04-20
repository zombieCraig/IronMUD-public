# IronMUD Game Mechanics Reference

## Combat System

### Damage Types
- `bludgeoning` - Clubs, maces, fists
- `slashing` - Swords, axes
- `piercing` - Daggers, spears, arrows
- `fire` - Fire magic, flaming weapons
- `cold` - Ice magic, frost weapons
- `lightning` - Electric attacks
- `poison` - Venomous attacks
- `acid` - Corrosive attacks

### Damage Dice
Damage is expressed as "XdY" or "XdY+Z":
- `1d6` - Roll 1 six-sided die (1-6 damage)
- `2d4+2` - Roll 2 four-sided dice plus 2 (4-10 damage)

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

## Mobile Flags

| Flag | Effect |
|------|--------|
| `aggressive` | Attacks players on sight |
| `sentinel` | Does not wander |
| `scavenger` | Picks up items from ground |
| `shopkeeper` | Can buy/sell items |
| `healer` | Provides healing services |
| `no_attack` | Cannot be attacked |
| `leasing_agent` | Property rental agent |

## Mobile Level System

Mobiles use a level 1-10 system that determines their combat stats. Use `medit <vnum> autostats` to automatically set appropriate stats after setting the level.

| Level | HP  | AC | Damage  | Hit Mod | Stats | Difficulty |
|-------|-----|----|---------|---------|-------|------------|
| 1     | 15  | 0  | 1d4     | -2      | 10    | Trivial    |
| 2     | 25  | 1  | 1d6     | -1      | 10    | Easy       |
| 3     | 40  | 2  | 1d6+1   | 0       | 11    | Moderate   |
| 4     | 60  | 3  | 1d8+1   | +1      | 12    | Challenging|
| 5     | 80  | 4  | 1d8+2   | +2      | 13    | Tough      |
| 6     | 100 | 5  | 2d6     | +3      | 14    | Dangerous  |
| 7     | 130 | 6  | 2d6+2   | +4      | 15    | Elite      |
| 8     | 170 | 8  | 2d8+2   | +5      | 16    | Boss       |
| 9     | 220 | 10 | 2d8+4   | +6      | 17    | Mini-boss  |
| 10    | 300 | 12 | 3d8+4   | +8      | 18    | Legendary  |

**Hit Modifier** affects the mobile's chance to hit in combat. Higher levels hit more often.

### Building a Combat Mobile

```
medit create Goblin Guard
medit goblin-guard level 3
medit goblin-guard autostats        # Sets HP=40, AC=2, damage=1d6+1, etc.
medit goblin-guard damtype slashing
medit goblin-guard flag aggressive on
```

### Mobile Equipped Weapons

Mobiles can equip weapons via spawn dependencies. When equipped, they use the weapon's damage instead of their base `damage_dice`. This allows the same mobile template to have different effective damage based on equipment.

## Combat Commands

Players can use these commands in combat:

| Command | Effect |
|---------|--------|
| `attack <target>` | Initiate combat with a target |
| `assist [player]` | Join an ally's fight (defaults to group leader) |
| `consider <target>` | Gauge difficulty before engaging |
| `flee` | Attempt to escape combat |

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
| `on_timer` | At set interval |

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

## Area Difficulty Guidelines

When designing areas, consider the mobile levels for appropriate challenge:

| Area Type | Mobile Levels | Description |
|-----------|---------------|-------------|
| Tutorial/Safe | 1-2 | New player areas, minimal danger |
| Beginner | 2-4 | Starting zones, learning combat |
| Intermediate | 4-6 | Main adventure areas |
| Advanced | 6-8 | Challenging content, group recommended |
| Expert | 8-10 | End-game areas, elite encounters |

**Tip**: Use `consider <mobile>` to see how players will perceive difficulty.

# Mobile/NPC Editing

This guide covers creating and editing mobiles (NPCs) using IronMUD's Online Creation (OLC) system.

## Mobile Commands

| Command | Usage | Description |
|---------|-------|-------------|
| `medit create` | `medit create <name>` | Create a new mobile prototype |
| `medit` | `medit <id\|vnum> [subcommand]` | Edit mobile properties |
| `mlist` | `mlist` | List all mobiles |
| `mfind` | `mfind <keyword>` | Search mobiles by name/keywords/vnum |
| `mdelete` | `mdelete <mobile_id>` | Delete a mobile |
| `mspawn` | `mspawn <vnum>` | Spawn mobile from prototype |

## Creating Mobiles

The `medit create` command creates a new mobile prototype:

```
> medit create Town Guard
Created mobile prototype with vnum: town_guard
=== Mobile Properties ===
Name: Town Guard
ID: 550e8400-e29b-41d4-a716-446655440003
Vnum: town_guard
Level: 1
...

> medit town_guard level 10
Level set to: 10

> medit town_guard flag shopkeeper on
Flag shopkeeper set to: ON
```

**Vnum generation rules:**
- Name is converted to lowercase
- Spaces and dashes become underscores
- Special characters are removed
- Max 24 characters
- Duplicates get a number suffix (`_2`, `_3`, etc.)

## medit Subcommands

### Basic Properties

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `show` | `medit <id>` | Display mobile properties |
| `name` | `medit <id> name <text>` | Set mobile name |
| `short` | `medit <id> short <text>` | Set short description (room view) |
| `long` | `medit <id> long <text>` | Set long description (examine) |
| `keywords` | `medit <id> keywords <kw>...` | Set mobile keywords |

### Combat Stats

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `level` | `medit <id> level <value>` | Set mobile level |
| `hp` | `medit <id> hp <value>` | Set max HP |
| `damage` | `medit <id> damage <dice>` | Set damage (e.g., 2d6+3) |
| `ac` | `medit <id> ac <value>` | Set armor class |
| `stat` | `medit <id> stat <attr> <value>` | Set attribute (str/dex/con/int/wis/cha) |
| `perception` | `medit <id> perception <0-10>` | Set stealth detection ability (higher = better at spotting hidden players) |

### Flags and Triggers

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `flags` | `medit <id> flags` | Show all flags |
| `flag` | `medit <id> flag <name> [on\|off]` | Toggle mobile flag |
| `trigger` | `medit <id> trigger` | Manage triggers (see [Triggers](triggers.md)) |

### Prototype Management

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `prototype` | `medit <id> prototype [on\|off]` | Mark as prototype template |
| `vnum` | `medit <id> vnum <name\|none>` | Set prototype vnum |
| `spawn` | `medit <id> spawn` | Spawn copy into current room |

## Mobile Flags

| Flag | Effect |
|------|--------|
| `aggressive` | Attacks players on sight |
| `sentinel` | Never wanders from spawn room |
| `scavenger` | Picks up items from ground |
| `shopkeeper` | Can buy/sell items |
| `no_attack` | Cannot be attacked |
| `healer` | Provides healing services |
| `leasing_agent` | Property rental agent |
| `cowardly` | Flees when sniped or HP < 25% |
| `can_open_doors` | Can open/unlock doors during routine pathfinding |
| `guard` | Enhanced perception, responds to nearby theft |
| `thief` | Steals gold from players |

Mobiles produced by the [immigration system](areas.md#immigration-migrant-spawning) carry `migrant:<role>:<prefix>` vnums and are tagged with role-specific flags (guard, healer, scavenger). They're attached to a liveable room via `resident_of` and released when they die.

```
> medit guard flag sentinel on
Flag 'sentinel' set to: ON
```

## Dialogue System

NPCs can respond to keywords when players speak:

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `dialogue` | `medit <id> dialogue <keyword> <response>` | Add dialogue |
| `dialogues` | `medit <id> dialogues` | List all dialogues |
| `rmdialogue` | `medit <id> rmdialogue <keyword>` | Remove dialogue |

```
> medit innkeeper dialogue hello Welcome to my inn, traveler!
Dialogue added for 'hello'

> medit innkeeper dialogue room Rooms are 5 gold per night.
Dialogue added for 'room'

> medit innkeeper dialogues
=== Dialogues ===
hello: Welcome to my inn, traveler!
room: Rooms are 5 gold per night.
```

Players trigger dialogue by saying the keyword:
```
> say hello
You say: hello
The Innkeeper says: Welcome to my inn, traveler!
```

## Shop System

Shopkeepers are mobiles with the `shopkeeper` flag. They can buy and sell items.

### Shop Subcommands

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `shop` | `medit <id> shop` | Show shop configuration |
| `shop stock add` | `medit <id> shop stock add <vnum>` | Add to base stock (infinite) |
| `shop stock remove` | `medit <id> shop stock remove <vnum>` | Remove from base stock |
| `shop buyrate` | `medit <id> shop buyrate <0-100>` | % paid when buying from players |
| `shop sellrate` | `medit <id> shop sellrate <50+>` | % charged when selling |
| `shop buys` | `medit <id> shop buys add\|remove <type>` | Manage item types the shop buys |
| `shop categories` | `medit <id> shop categories add\|remove\|clear <cat>` | Manage buy categories |
| `shop minvalue` | `medit <id> shop minvalue <amount\|clear>` | Set minimum item value to buy |
| `shop maxvalue` | `medit <id> shop maxvalue <amount\|clear>` | Set maximum item value to buy |
| `shop preset` | `medit <id> shop preset set\|clear <vnum>` | Apply or remove a buy preset |
| `shop preset extra` | `medit <id> shop preset extra type\|category add\|remove <val>` | Add types/categories beyond preset |
| `shop preset deny` | `medit <id> shop preset deny type\|category add\|remove <val>` | Exclude types/categories from preset |

### Setting Up a Shop

```
> medit create Blacksmith
> medit blacksmith flag shopkeeper on
Flag 'shopkeeper' set to: ON

> medit blacksmith shop stock add rusty_sword
Added rusty_sword to shop stock.

> medit blacksmith shop stock add iron_helmet
Added iron_helmet to shop stock.

> medit blacksmith shop buyrate 50
Buy rate set to 50% (shop pays half value)

> medit blacksmith shop sellrate 150
Sell rate set to 150% (shop charges 1.5x value)

> medit blacksmith shop buys add weapon
Added buy type: weapon

> medit blacksmith shop buys add armor
Added buy type: armor

> medit blacksmith shop minvalue 5
Min buy value set to 5 gold.
```

### Buy Filtering

Shopkeepers can filter what they buy using **types**, **categories**, and **value ranges**. These filters work as independent layers -- an item must pass all configured checks:

- **Types** -- Filter by item type (`weapon`, `armor`, `misc`, `food`, etc.). Use `all` to accept any type.
- **Categories** -- Filter by item crafting categories (e.g., `leather`, `herbs`, `metal`). The item must have at least one matching category.
- **Value range** -- Set `minvalue` and/or `maxvalue` to restrict by item gold value.

If no types and no categories are configured (and no preset is applied), the shop will not buy anything from players.

```
> medit herbalist shop buys add misc
Added buy type: misc

> medit herbalist shop categories add herbs
Added buy category: herbs

> medit herbalist shop maxvalue 500
Max buy value set to 500 gold.
```

In this example, the herbalist only buys misc-type items that have the "herbs" category and are worth 500 gold or less.

### Buy Presets

Buy presets are reusable buy configurations that can be shared across multiple shopkeepers. They are managed with the `bpredit` command and referenced by shops using a vnum.

When a shop has a preset applied, the effective buy filter is computed by combining:
1. **Preset** types and categories (base configuration)
2. **Extra** types/categories (shop adds on top of preset)
3. **Deny** types/categories (shop removes from preset)
4. **Direct** shop buys types and categories (always included)

#### bpredit Commands

| Command | Usage | Description |
|---------|-------|-------------|
| `bpredit list` | `bpredit list` | List all presets |
| `bpredit create` | `bpredit create <vnum> <name>` | Create a new preset |
| `bpredit` | `bpredit <vnum>` | Show preset details |
| `bpredit name` | `bpredit <vnum> name <text>` | Set preset name |
| `bpredit desc` | `bpredit <vnum> desc <text>` | Set preset description |
| `bpredit type` | `bpredit <vnum> type add\|remove <type>` | Manage buy types |
| `bpredit category` | `bpredit <vnum> category add\|remove <cat>` | Manage buy categories |
| `bpredit minvalue` | `bpredit <vnum> minvalue <amount\|clear>` | Set/clear min value |
| `bpredit maxvalue` | `bpredit <vnum> maxvalue <amount\|clear>` | Set/clear max value |
| `bpredit delete` | `bpredit delete <vnum>` | Delete a preset |

#### Preset Workflow Example

```
> bpredit create weapons_dealer Weapons Dealer
Created buy preset 'weapons_dealer' (Weapons Dealer).

> bpredit weapons_dealer type add weapon
Added type 'weapon'.

> bpredit weapons_dealer type add armor
Added type 'armor'.

> bpredit weapons_dealer minvalue 10
Min value set to 10 gold.

> medit blacksmith shop preset set weapons_dealer
Preset set to: weapons_dealer (Weapons Dealer)

> medit blacksmith shop preset extra category add enchanted
Added extra category 'enchanted'.

> medit blacksmith shop preset deny type add armor
Added deny type 'armor'.
```

In this example, the blacksmith uses the "weapons_dealer" preset (weapons + armor), but denies armor and adds an extra category filter for "enchanted" items. The effective result: buys weapons that are enchanted and worth at least 10 gold.

### Player Shop Commands

| Command | Usage | Description |
|---------|-------|-------------|
| `list` | `list` | Show items for sale |
| `buy` | `buy <item>` | Buy an item |
| `sell` | `sell <item>` | Sell an item |
| `appraise` | `appraise <item>` | Check what a shop will pay |

```
> list
=== Blacksmith's Wares ===
  Rusty Sword         15 gold
  Iron Helmet         25 gold

> buy sword
You buy Rusty Sword for 15 gold.
```

### Stock Types

- **Base stock** - Items always available (infinite supply, from `shop stock add`)
- **Inventory** - Items bought from players (limited supply)

## Daily Routine System

Mobiles can follow time-based daily schedules, moving between locations and changing activity states throughout the game day. A merchant might open shop at 8am and go home at 8pm. A guard might patrol during the day and sleep in the barracks at night.

### Game Time

IronMUD uses a 24-hour game clock where 1 game hour = 2 real minutes (a full game day = 48 real minutes). Routines are evaluated once per game hour.

| Game Hour | Time of Day |
|-----------|-------------|
| 0-4 | Late Night |
| 5-7 | Dawn |
| 8-11 | Morning |
| 12-13 | Midday |
| 14-17 | Afternoon |
| 18-20 | Evening |
| 21-23 | Night |

### Activity States

Each routine entry sets an activity state that affects NPC behavior:

| Activity | Effect |
|----------|--------|
| `working` | Shop/healer services available, normal dialogue |
| `sleeping` | No services, no dialogue, shows "is here, sleeping" in room |
| `patrolling` | Wandering enabled, normal interactions |
| `off_duty` | No services, normal dialogue |
| `socializing` | No services, normal dialogue |
| `eating` | No services, normal dialogue |

When a shopkeeper, healer, or dialogue NPC is not in the `working` state, players cannot use `buy`, `sell`, `list`, `appraise`, or get healer services. The NPC will tell the player they're unavailable (or sleeping).

### Routine Subcommands

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `routine` | `medit <id> routine` | Show current routine |
| `routine add` | `medit <id> routine add <hour> <activity> [vnum]` | Add a routine entry |
| `routine remove` | `medit <id> routine remove <hour>` | Remove entry at hour |
| `routine clear` | `medit <id> routine clear` | Remove all entries |
| `routine msg` | `medit <id> routine msg <hour> <message>` | Set transition message |
| `routine wander` | `medit <id> routine wander <hour> on\|off` | Toggle wander suppression |
| `routine visible` | `medit <id> routine visible on\|off` | Toggle player schedule visibility |
| `routine dialogue` | `medit <id> routine dialogue <hour> <kw> <response>` | Set dialogue override |
| `routine preset list` | `medit <id> routine preset list` | List available presets |
| `routine preset` | `medit <id> routine preset <name> <key=vnum ...>` | Apply a preset |

### Setting Up a Routine Manually

```
> medit blacksmith routine add 8 working market:smithy
Added routine entry at 8am: working -> market:smithy

> medit blacksmith routine add 20 off_duty market:house
Added routine entry at 8pm: off_duty -> market:house

> medit blacksmith routine add 22 sleeping market:house
Added routine entry at 10pm: sleeping -> market:house

> medit blacksmith routine msg 8 {name} opens up shop for the day.
Set transition message at 8am: {name} opens up shop for the day.

> medit blacksmith routine msg 20 {name} closes up shop for the night.
Set transition message at 8pm: {name} closes up shop for the night.

> medit blacksmith routine visible on
Schedule is now visible to players.
```

Use `{name}` in transition messages to insert the mobile's name.

### Using Presets

Presets apply common routine patterns with room vnum substitution. Each preset defines destination keys (like `shop`, `home`, `post`) that you map to actual room vnums.

```
> medit blacksmith routine preset list
=== Routine Presets ===
merchant_8to20 - Standard merchant: works 8-20, sleeps at home overnight
  Keys: shop, home
guard_dayshift - Day guard: patrols 6-18, off-duty and sleeps at barracks
  Keys: post, barracks
guard_nightshift - Night guard: patrols 18-6, sleeps during the day
  Keys: barracks, post
tavern_keeper - Tavern keeper: works 10-2am, sleeps upstairs
  Keys: home, tavern
wandering_merchant - Traveling merchant: works in market by day, wanders to camp at night
  Keys: market, camp

> medit blacksmith routine preset merchant_8to20 shop=market:smithy home=market:house
Applied routine preset 'merchant_8to20'.
```

### Available Presets

| Preset | Schedule | Destination Keys |
|--------|----------|------------------|
| `merchant_8to20` | Works 8am-8pm, sleeps at home overnight | `shop`, `home` |
| `guard_dayshift` | Patrols 6am-6pm, off-duty/sleeps at barracks | `post`, `barracks` |
| `guard_nightshift` | Patrols 6pm-6am, sleeps during the day | `post`, `barracks` |
| `tavern_keeper` | Works 10am-2am, sleeps upstairs | `tavern`, `home` |
| `wandering_merchant` | Works in market by day, camps at night | `market`, `camp` |

### Step Movement and Pathfinding

When a routine entry includes a destination room vnum, the mobile walks one room per tick toward that destination using BFS pathfinding (max 20 rooms). The mobile will:

- Move through open exits automatically
- Open closed unlocked doors if `can_open_doors` flag is set
- Unlock locked doors with keys from inventory if `can_open_doors` flag is set
- Close and re-lock doors behind them after passing through
- Skip movement if already at the destination

The `sentinel` flag is overridden by routine destinations -- a sentinel mobile with a routine will still walk to its destination, but won't randomly wander between destinations.

### Wander Suppression

By default, routine entries suppress random wandering (`suppress_wander: true`). You can enable wandering during specific time periods:

```
> medit guard routine wander 6 on
Wandering enabled at 6am.
```

This is useful for patrol schedules where you want guards to move randomly around their post area.

### Dialogue Overrides

Routine entries can override dialogue responses for specific time periods:

```
> medit shopkeeper routine dialogue 20 hello Sorry, I'm closed for the evening.
Set dialogue override at 8pm [hello]: Sorry, I'm closed for the evening.
```

During the 8pm routine period, if a player says "hello", the shopkeeper will respond with the override instead of their normal dialogue.

### Player Schedule Command

Players can view an NPC's schedule if `schedule_visible` is enabled:

```
> schedule blacksmith
=== Schedule for Old Gregor ===
Currently: working

  8am (Morning): working
  8pm (Evening): off_duty
  10pm (Night): sleeping
```

### Door-Aware Pathfinding

Set the `can_open_doors` flag on mobiles that should navigate through doors:

```
> medit guard flag can_open_doors on
Flag 'can_open_doors' set to: ON
```

The mobile can then:
- **Open** closed but unlocked doors
- **Unlock and open** locked doors if they have the matching key in their inventory
- **Close** doors behind them after passing through
- **Re-lock** doors if they were originally locked

This is useful for guards patrolling through gated areas, or shopkeepers who lock up at night.

## Simulation System

Simulated NPCs ("sim mobiles") run a Sims-style needs loop. They track hunger, energy, and comfort; pick goals from those needs; wander to food/work/home rooms; and earn/spend gold. Area immigration creates sim mobiles automatically — use `medit <id> simulation setup` to convert an existing mobile.

### Simulation Subcommands

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `simulation` | `medit <id> simulation` | Show config, needs, and current goal |
| `simulation setup` | `medit <id> simulation setup <home> <work> <shop>` | Enable simulation and set room vnums |
| `simulation remove` | `medit <id> simulation remove` | Disable simulation |
| `simulation pay` | `medit <id> simulation pay <gold>` | Gold earned per work cycle |
| `simulation hours` | `medit <id> simulation hours <start> <end>` | Game-hour work window |
| `simulation food` | `medit <id> simulation food <vnum>` | Preferred food item vnum |
| `simulation decay` | `medit <id> simulation decay <hunger\|energy\|comfort> <rate>` | Per-need decay rate (100 = normal, 200 = 2x faster) |
| `simulation lowgold` | `medit <id> simulation lowgold <threshold>` | When gold <= threshold, work takes priority over food |

```
> medit gregor simulation setup market:house market:smithy market:market
Simulation enabled. Home: market:house, Work: market:smithy, Shop: market:market

> medit gregor simulation pay 20
Work pay set to 20 gold.

> medit gregor simulation hours 8 18
Work hours set to 8:00 - 18:00.

> medit gregor simulation
=== Simulation Config ===
  Home Room:  market:house
  Work Room:  market:smithy
  Shop Room:  market:market
  Food Pref:  bread
  Work Pay:   20 gold
  Work Hours: 8:00 - 18:00
  Decay Rates: hunger=0 energy=0 comfort=0 (0=default, 100=normal)
  Low Gold:   10 (NPC seeks work when gold <= this)

=== Current Needs ===
  Hunger:  72/100
  Energy:  84/100
  Comfort: 66/100
  Goal:    Working
  Paid:    true
```

## Social System

Sim mobiles carry social state: per-mobile happiness (0-100), per-pair affinity (-100..=100), liked/disliked conversation topics, and a derived mood (`Content`, `Normal`, `Sad`, `Depressed`, `Breakdown`). Topic-matched conversations boost happiness and affinity on a cooldown; repeating the same topic with the same partner runs into topic fatigue (halved deltas after ~5 recent matches). Players can query relationships with `ask <mobile> about <other>`, and `examine` shows mood/bereavement/cohabitant cues.

### Social Subcommands

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `social` | `medit <id> social` | Show happiness, mood, likes, dislikes |
| `social happiness` | `medit <id> social happiness <0-100>` | Set happiness directly (debugging) |
| `social affinity` | `medit <id> social affinity <other_id> <value>` | Set affinity toward another mobile (-100..=100) |
| `social family list` | `medit <id> social family` | Show family relationships |
| `social family set` | `medit <id> social family set <other_id> <parent\|child\|sibling\|partner>` | Link two mobiles |
| `social family unset` | `medit <id> social family unset <other_id>` | Remove a relationship |
| `social family household new` | `medit <id> social family household new` | Mint a fresh household id |
| `social family household link` | `medit <id> social family household link <other_id>` | Share another mobile's household |
| `social family household clear` | `medit <id> social family household clear` | Drop household membership |

```
> medit gregor social
=== Social State ===
  Happiness: 74/100 (Content)
  Likes:     road, mayor, politics, rumors
  Dislikes:  drinking

> medit gregor social family set esme_001 partner
Linked gregor as partner of esme_001.
```

Likes and dislikes are not builder-editable through `medit`; they're seeded during migrant spawning and nudged by role (guard, healer, scavenger).

## Pregnancy

Simulated female partners can become pregnant. Gestation runs 60 game days by default; at term, `spawn_child` fires in the area and creates a child mobile linked to both parents' households. Use these builder overrides for testing:

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `pregnancy` | `medit <id> pregnancy` | Show pregnancy status |
| `pregnancy preg` | `medit <id> pregnancy preg <father_id> [days]` | Force-set pregnancy (0 = default 60 days) |
| `pregnancy birth` | `medit <id> pregnancy birth` | Fire `spawn_child` immediately |
| `pregnancy clear` | `medit <id> pregnancy clear` | Drop pregnancy state |

```
> medit esme pregnancy preg gregor_001 10
Pregnancy set (father gregor_001, 10 game days).

> medit esme pregnancy birth
Birth triggered.
```

Two server-wide settings tune natural pregnancy and orphan adoption (see [Admin Guide](../admin-guide.md)):

- `conception_chance_per_day` (default `0.005`)
- `adoption_chance_per_day` (default `0.10`)

## Prototypes and Spawning

Prototypes are templates for creating mobile instances:

1. **Create a prototype** with `medit create`
2. **Configure properties** with medit subcommands
3. **Spawn instances** with `mspawn`

```
> medit create Town Guard
> medit town_guard level 5
> medit town_guard hp 50

> mspawn town_guard
Spawned Town Guard into the room.
```

For automatic spawning, see [Areas](areas.md#spawn-points).

## NPC Triggers

NPCs support triggers for scripted behaviors:

| Type | Event |
|------|-------|
| `on_greet` | Player enters room with NPC |
| `on_say` | Player says something in room |
| `on_idle` | Periodic when players present |
| `on_attack` | NPC is attacked |
| `on_death` | NPC dies |

### Built-in Templates

For simple behaviors, use templates instead of scripts:

| Template | Arguments | Behavior |
|----------|-----------|----------|
| `@say_greeting` | `<message>` | NPC says the message |
| `@say_random` | `<msg1\|msg2\|...>` | Random message |
| `@emote` | `<action>` | NPC performs action |

```
> medit shopkeeper trigger add greet @say_greeting Welcome to my shop!
Added greet template: @say_greeting

> medit innkeeper trigger add greet @say_random Hello!|Welcome!|Make yourself at home.
Added greet template: @say_random

> medit guard trigger add greet @emote eyes you suspiciously.
Added greet template: @emote
```

### Idle Triggers

Idle triggers fire periodically when players are in the room:

```
> medit town_guard trigger add idle @say_random Nice weather today.|*yawns*
Added idle template: @say_random

> medit town_guard trigger interval 0 30
Trigger at index 0 now has 30 second interval.

> medit town_guard trigger chance 0 50
Trigger at index 0 now has 50% chance to fire.
```

### Testing Triggers

Test triggers manually for debugging:

```
> medit guard trigger test 0
Testing trigger 0...
Trigger executed successfully!
Result: continue
```

See [Triggers](triggers.md) for custom trigger scripts.

## Related Documentation

- [Items](items.md) - Item creation for shop stock
- [Areas](areas.md) - Spawn points for automatic respawning
- [Triggers](triggers.md) - NPC trigger scripting
- [Builder Guide](../builder-guide.md) - Overview of building

# Player Guide

Welcome to IronMUD! This guide will help you get started exploring the world.

## Connecting

Connect using any telnet client or MUD client:

```bash
telnet yourserver.com 4000
```

Popular MUD clients with enhanced features:
- [Mudlet](https://www.mudlet.org/) (Windows, Mac, Linux)
- [MUSHclient](http://www.gammon.com.au/mushclient) (Windows)
- [Blightmud](https://github.com/Blightmud/Blightmud) (Terminal)

## Creating a Character

When you first connect, create your character:

```
Welcome to IronMUD!

> create MyName mypassword
Character 'MyName' created successfully!
You find yourself in the Town Square...
```

On future connections, log in:
```
> login MyName mypassword
Welcome back, MyName!
```

## Your First 5 Minutes

### Look around
```
> look
Town Square
-------------------
A bustling marketplace surrounded by shops and stalls.
A large fountain stands in the center.

Exits: north, east, south, west

A Town Guard is here.
A Merchant is here.
```

### Check the exits
```
> exits
Obvious exits:
  North - The Market Street
  East - The Tavern
  South - The City Gates
  West - The Temple
```

### Move around
```
> north
The Market Street
-------------------
Vendors hawk their wares from colorful stalls...
```

You can also use shorthand: `n`, `s`, `e`, `w`, `u` (up), `d` (down).

### Talk to people
```
> say Hello everyone!
You say: Hello everyone!

> shout Anyone need help?
You shout: Anyone need help?
```

### Check your status
```
> status
=== MyName ===
Level: 1
Health: 100/100
Gold: 0
```

## Exploring

### Movement

Move using directions or their shortcuts:

| Command | Shortcut |
|---------|----------|
| `go north` | `north` or `n` |
| `go south` | `south` or `s` |
| `go east` | `east` or `e` |
| `go west` | `west` or `w` |
| `go up` | `up` or `u` |
| `go down` | `down` or `d` |

### Examining Things

Look at specific objects mentioned in room descriptions:

```
> look fountain
The fountain depicts a mermaid holding a shell,
water cascading from its lip.

> look guard
A stern-faced guard in polished armor.
```

### Opening Doors

Some exits have doors that must be opened:

```
> exits
Obvious exits:
  North [closed gate]
  South - Town Square

> open north
You open the gate.

> north
You pass through the gate...
```

Locked doors require keys:
```
> open north
The gate is locked.

> unlock north
You unlock the gate with the Iron Key.

> open north
You open the gate.
```

## Communication

### Talking in the Room

```
> say Hello there!
You say: Hello there!
```

Everyone in the same room sees your message.

### Private Messages

```
> tell Bob Meet me at the tavern
You tell Bob: Meet me at the tavern

> whisper Alice The password is xyzzy
You whisper to Alice: The password is xyzzy
```

`whisper` only works if the person is in the same room.

### Shouting

```
> shout Anyone want to group up?
You shout: Anyone want to group up?
```

Everyone online hears shouts.

### Emotes

```
> emote waves hello
MyName waves hello

> emote laughs
MyName laughs
```

## Items and Inventory

### Picking Up Items

```
> look
Town Square
-------------------
...
A rusty sword is here.

> get sword
You pick up the rusty sword.
```

### Your Inventory

```
> inventory
You are carrying:
  a rusty sword
  a torch
  a loaf of bread

> equipment
You are wearing:
  <head> a leather cap
  <body> a cloth shirt
```

### Using Items

```
> wear sword
You wield the rusty sword.

> remove sword
You stop wielding the rusty sword.

> drop sword
You drop the rusty sword.

> eat bread
You eat the loaf of bread.

> drink water
You drink some water from the waterskin.
```

### Examining Items

```
> examine sword
A rusty sword (weapon)
Damage: 1d6
Value: 5 gold
Weight: 3
```

### Containers

```
> look in chest
The wooden chest contains:
  a gold coin
  a healing potion

> get potion from chest
You get a healing potion from the wooden chest.

> put coin in chest
You put the gold coin in the wooden chest.
```

## NPCs and Shops

### Talking to NPCs

Many NPCs respond to keywords:

```
> say hello
You say: hello
The Innkeeper says: Welcome to my inn, traveler!

> say room
You say: room
The Innkeeper says: Rooms are 5 gold per night.
```

### Shopping

Find a shopkeeper and browse their wares:

```
> list
=== Blacksmith's Wares ===
  Iron Sword         50 gold
  Leather Armor      30 gold
  Healing Potion     10 gold

> buy sword
You buy an Iron Sword for 50 gold.

> sell cap
You sell the leather cap for 5 gold.
```

## Property Rental

You can rent your own private housing for safe item storage and a place to call home.

### Finding a Leasing Office

Look for leasing agents in towns - they manage property rentals in their area.

```
> look
Riverside Realty Office
-------------------
A tidy office with property listings on the wall.

A Leasing Agent is here.
```

### Viewing Available Properties

```
> properties
=== Riverside Realty ===

Available Properties:

  Small Cottage - 50 gold/month
    A cozy one-room cottage with basic amenities.

  Town House - 150 gold/month
    A two-story home with kitchen and storage.

Use 'tour <property>' to preview, 'rent <property>' to lease.
Your gold: 500
```

### Touring Before You Rent

Preview a property before committing:

```
> tour cottage
You begin a tour of 'Small Cottage'...

Small Cottage - Living Room
A cozy room with a fireplace and wooden floors.
[Exits: north out]

> north
Small Cottage - Bedroom
A small bedroom with a simple bed.

> tour end
Tour ended. Returning to Riverside Realty Office.
```

Note: You cannot pick up or drop items while touring.

### Renting a Property

```
> rent cottage
=== Rental Agreement ===

Property: Small Cottage
Monthly Rent: 50 gold
Required Now: 50 gold (30 game days upfront)
Your Gold: 500

Type 'rent cottage confirm' to proceed.

> rent cottage confirm
Congratulations! You have rented 'Small Cottage'.
50 gold has been deducted.
Use 'enter' to access your new home.
```

### Entering Your Property

From the leasing office where you rented:

```
> enter
You enter your property...

MyName's Small Cottage - Living Room
A cozy room with a fireplace and wooden floors.
[Exits: north out]
```

Use `out` to return to the leasing office.

### Managing Property Access

Control who can visit your property:

```
> property
=== Your Property ===

Name: MyName's Small Cottage
Rent: 50 gold/month
Paid until: Day 30 (15 days remaining)
Party Access: None
Trusted Visitors: (none)

> property access visit
Party access set to 'Visit Only'.
Grouped players can now enter and look around.

> property trust Alice
Alice added to trusted visitors (full access).
```

Access levels:
- **None** - Only you can enter
- **Visit Only** - Grouped players can enter and look
- **Full Access** - Grouped players can use amenities and take items

### Visiting Other Players' Properties

If a grouped player has granted you access:

```
> visit Alice
Alice has granted you access to their property.
You enter Alice's Small Cottage...
```

### Upgrading Your Property

Transfer to a better property in the same area:

```
> upgrade townhouse
=== Property Upgrade ===

Current: Small Cottage (50 gold/month)
New: Town House (150 gold/month)

Items to transfer: 5
Transfer fee: 30 gold
First month rent: 150 gold
Total cost: 180 gold

Type 'upgrade townhouse confirm' to proceed.
```

Your items are automatically moved to the new property.

### Ending Your Lease

Voluntarily terminate your lease:

```
> endlease
=== End Lease ===

Property: Small Cottage
Items inside: 5

WARNING: Your items will be moved to escrow.
You will have 30 days to retrieve them for a small fee.

Type 'endlease confirm' to proceed.
```

### Escrow Storage

If you're evicted (can't pay rent) or end your lease, items go to escrow:

```
> escrow
=== Your Escrow Storage ===

Escrow #1:
  Items: 5 items stored
  Retrieval Fee: 55 gold (5 gold if re-rented locally)
  Expires: 25 days remaining
  Contents: oak chest, clay plant pot, ...

Use 'escrow retrieve <number>' at a leasing office to retrieve items.
```

Visit any leasing office and use `escrow retrieve <number>` to get your items back:

- **Re-rented in the same area**: Items go to your new property at a discounted fee (10% of full price).
- **Property in a different area**: Items are shipped to your property at full fee.
- **No property**: Items are dropped at the leasing office for you to pick up.

Items inside containers (chests, etc.) and plants in pots are preserved through escrow.

Rent is automatically deducted from your gold each rent period (default: 30 game days, configurable by admins). Keep enough gold to avoid eviction!

## Consumables and Effects

### Eating and Drinking

Food and drinks can have special effects beyond satisfying hunger and thirst:

```
> drink potion
You drink some healing_potion from the healing potion.
You feel healed! (+20 HP)

> eat enchanted_bread
You eat the enchanted bread.
You feel a surge of strength! (+3 Strength for 300s)
```

### Active Buffs

Some consumables grant temporary buffs that enhance your abilities:
- **Stat boosts** - Increased strength, dexterity, etc. (affects combat)
- **Haste** - Reduced movement stamina cost
- **Regeneration** - Heal HP over time
- **Invisibility** - Hidden from other players' `look` and `who`

Buffs expire after their duration. You'll see a message when they wear off:
```
The strength boost effect wears off.
```

### Inebriation

Alcoholic drinks increase your drunk level. Effects:
- **Mild** (drunk > 30): Your speech becomes garbled when using `say`
- **Heavy** (drunk > 50): You may stumble into random rooms when moving

Drunk level decreases over time as you sober up.

## Useful Commands

### Getting Help

```
> help
=== Available Commands ===
Movement: north, south, east, west, up, down
...

> help look
look - Look at your surroundings or examine something
Usage: look [target]
```

### Who's Online

```
> who
=== Players Online ===
  MyName (Town Square)
  Bob (The Tavern)
  Alice (The Forest)
```

### Saving and Quitting

Your character saves automatically. To disconnect:

```
> quit
Goodbye! Your character has been saved.
```

To log out but stay connected:
```
> logout
You have logged out. Use 'login' to reconnect.
```

### Changing Your Password

```
> password newpassword
Password changed successfully.
```

### Creating Aliases

```
> alias heal drink potion
Alias 'heal' created.

> heal
You drink the healing potion.

> unalias heal
Alias 'heal' removed.
```

## Magic and Spells

### The Mage Class

Characters with the **Mage** class have access to the magic skill, which unlocks spellcasting. As your magic skill increases, more powerful spells become available.

### Casting Spells

Use the `cast` command to cast a spell:

```
> cast magic_missile goblin
You cast Magic Missile at the goblin!

> cast cure_wounds
You cast Cure Wounds on yourself.

> cast light
You cast Light, illuminating the area.
```

Syntax: `cast <spell> [target]`

Some spells require a target (like offensive spells), while others default to yourself or the room.

### Viewing Available Spells

Use the `spells` command to see which spells you currently have access to:

```
> spells
=== Your Spells ===
  Magic Missile     (magic 1)  - 5 mana
  Light             (magic 1)  - 3 mana
  Firebolt          (magic 2)  - 8 mana
  ...
```

### Learning Spells from Scrolls

Some spells can only be learned by reading magical scrolls. When you find a scroll, use `read` to learn the spell:

```
> read scroll
You study the scroll intently...
You have learned the spell 'Meteor Storm'!
The scroll crumbles to dust.
```

Once learned, the spell appears in your `spells` list permanently.

### Mana

Casting spells costs mana. Your current mana is shown in the `status` command and the prompt. Mana regenerates over time, with faster recovery in resting positions:

- **Standing** - Slowest regeneration
- **Sitting** - Moderate regeneration
- **Resting** - Faster regeneration
- **Sleeping** - Fastest regeneration

### Spell List

| Spell | Magic Skill | Mana | Description |
|-------|-------------|------|-------------|
| Magic Missile | 1 | 5 | Fires a bolt of arcane energy at a target |
| Light | 1 | 3 | Creates a magical light source |
| Firebolt | 2 | 8 | Hurls a bolt of fire at a target |
| Arcane Shield | 2 | 10 | Grants a temporary armor bonus |
| Cure Wounds | 3 | 12 | Heals yourself or an ally |
| Detect Invisible | 3 | 8 | Reveals invisible creatures |
| Lightning Bolt | 4 | 15 | Strikes a target with lightning |
| Invisibility | 5 | 20 | Makes yourself invisible |
| Dispel Magic | 5 | 18 | Removes magical buffs from a target |
| Haste | 6 | 25 | Increases your movement speed |
| Greater Heal | 6 | 30 | Powerful healing spell |
| Meteor Storm | 8 | 50 | Devastating area attack (scroll-only) |

Spells marked "scroll-only" cannot be learned through skill advancement alone and must be found on scrolls in the world.

## Stealth and Subterfuge

Three skill trees provide rogue-archetype gameplay: **stealth**, **thievery**, and **tracking**. Classes like Rogue, Assassin, Thief, Criminal, and Private Investigator start with points in these skills.

### Stealth Skills

| Command | Skill Required | Description |
|---------|---------------|-------------|
| `sneak` | Stealth 1 | Toggle sneaking mode — move without being seen |
| `hide` | Stealth 1 | Conceal yourself in the current room |
| `scout` | Stealth 2 | Scan adjacent rooms for occupants |
| `backstab <target>` | Stealth 3 | Devastating attack from hiding |
| `circle` | Stealth 4 | Flank your opponent mid-combat |
| `disguise <alias>` | Stealth 5 | Assume a false identity (requires disguise kit) |

**Sneaking**: While sneaking, your movements are hidden from other players and NPCs. Each room costs +1 extra stamina. NPCs with high perception may detect you.

**Hiding**: Once hidden, you are invisible to others unless they use `search`. Taking most actions (attacking, speaking, moving) breaks your concealment.

**Backstab**: Strike from hiding with a powerful multiplied attack. Requires a short blade weapon and stealth concealment. 60-second cooldown.

### Thievery Skills

| Command | Skill Required | Description |
|---------|---------------|-------------|
| `peek <target>` | Thievery 1 | View a target's inventory |
| `steal <gold\|item> from <target>` | Thievery 1+ | Steal from a target |
| `pick <direction\|container>` | Thievery 2 | Pick a lock (requires lockpick) |
| `settrap <type>` | Thievery 3 | Place a trap (requires trap kit) |
| `disarm` | Thievery 3 | Disarm a visible trap |
| `envenom` | Thievery 4 | Apply poison to your weapon (requires poison vial) |

**Stealing rules** depend on the zone:
- **Safe zones**: Stealing is blocked entirely
- **PvE zones**: Steal from NPCs only
- **PvP zones**: Steal from players and NPCs

A failed steal attempt against an NPC triggers combat. Failed theft against a player alerts them.

**Traps**: Place spike, alarm, snare, or poison dart traps. Players entering the room may trigger them unless they detect the trap first.

### Tracking Skills

| Command | Skill Required | Description |
|---------|---------------|-------------|
| `search` | Tracking 1 | Detect hidden characters and traps |
| `track <name>` | Tracking 2 | Find tracks of a target in the room |
| `lore <target>` | Tracking 2 | Study a creature's capabilities |
| `butcher <corpse>` | Tracking 2 | Harvest materials from a corpse |
| `covertracks` | Tracking 2 | Erase your tracks from the room |
| `camouflage` | Tracking 3 | Blend into wilderness terrain |
| `hunt <name>` | Tracking 4 | Automatically track and follow a target |

**Tracking passives**:
- **Pathfinding** (Tracking 3+): Reduced stamina cost in wilderness areas
- **Foraging bonus**: Tracking skill improves foraging success rate
- **Alertness** (Tracking 3+): Automatically sense hidden characters entering your room

**Camouflage vs Hide**: Camouflage only works outdoors in wilderness areas but is more effective there due to terrain bonuses. Hide works anywhere but lacks the terrain advantage.

**Hunt**: Sets your character to automatically follow a target's trail. Costs stamina per room and stops when the target is found, the trail goes cold, or you run out of stamina.

## Swimming and Water

The world contains three types of water terrain, each with increasing challenge:

### Shallow Water
Surface-level water like streams, beaches, and fords. Costs +1 extra stamina to move through. All characters can enter. Your swimming skill trains automatically as you move.

### Deep Water
Lakes, rivers, and open sea. Costs +2 extra stamina. You need either:
- A **boat** item in your inventory, OR
- **Swimming skill level 5** or higher

Without one of these, you'll be blocked from entering.

### Underwater
Submerged areas like sea floors and underwater caves. Costs +3 extra stamina. Your **breath** (shown in your prompt as `Air: X/100`) depletes every 10 seconds. When breath reaches 0, you take drowning damage (15% of your max HP per tick).

To explore underwater safely, you need the **WaterBreathing** buff — obtained from water breathing potions or spells. With this buff, breath does not deplete.

### Swimming Skill
Swimming improves automatically as you move through water:
- Shallow water: 5 XP per move
- Deep water: 10 XP per move
- Underwater: 15 XP per move

Higher swimming skill reduces stamina costs in water and extends how long you can hold your breath.

### Underwater Combat
Combat works differently underwater:
- **Slashing/bludgeoning** weapons deal 25% less damage
- **Piercing** weapons deal 15% more damage
- **Fire** attacks are extinguished (0 damage)
- **Cold** attacks deal 10% more damage

Bring a dagger or spear for underwater fights!

## Tips for New Players

1. **Use `look` often** - Room descriptions contain important clues
2. **Talk to NPCs** - Try common words like "hello", "help", "quest"
3. **Check your exits** - Use `exits` to see where you can go
4. **Explore carefully** - Some areas are more dangerous than others
5. **Use in-game help** - Type `help <command>` for details

## Advanced Features

### MXP Support

If using Mudlet or another MXP-capable client:
```
> mxp on
MXP enabled. Clickable links are now active.
```

Exit names become clickable for easy navigation.

### Time and Weather

The game world has a day/night cycle and weather:
```
> time
It is afternoon on day 15 of summer.
The weather is clear and mild.
```

Weather affects outdoor areas and visibility.

## Getting More Help

- Use `help` in-game for command reference
- Ask other players with `shout` or `tell`
- See the [Builder Guide](builder-guide.md) if you want to create content

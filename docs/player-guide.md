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
- [TinTin++](https://tintin.mudhalla.net/) (Linux, Mac, Windows/WSL)
- [Blightmud](https://github.com/Blightmud/Blightmud) (Terminal)

### TinTin++ Setup

IronMUD provides a pre-configured script for TinTin++ users that enables a split-screen status bar and real-time stat updates via MSDP.

1. **Download the script**: Locate `assets/ironmud.tin` in the IronMUD repository.
2. **Load the script**: Inside TinTin++, type:
   ```tintin
   #read assets/ironmud.tin
   ```
3. **Connect**: Type `ironmud` to connect to the server.

#### Optional: Auto-login

To automatically log in when you connect, edit your `assets/ironmud.tin` file and add the following at the bottom:

```tintin
#ACTION {^What is your account name?} {
    #SEND {login MyAccountName MyPassword};
}
```

Replace `MyAccountName` and `MyPassword` with your actual credentials.

## Creating a Character

When you first connect, create your account *and* your first character in one
step. The login name and your first character share a name.

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

### Multiple Characters Under One Account

A single login can own up to five characters. The account holds the
password; characters share a roster.

- **Single character**: login drops you straight into your character — same as
  always.
- **Multiple characters**: login shows a roster, type the number to pick one.
- **Add another character**: type `roster` to step out of the one you're
  playing, then `create <character_name>` from the roster prompt.
- **Switch characters mid-session**: `roster` saves your current character
  and returns you to the picker.

```
> roster

=== Your characters ===

  [1] MyName — level 12 fighter, in The Tavern
  [2] Sneakthief — level 4 unemployed, in Forest Path

Type a number to play that character, `new` to create another, or `quit` to log out.
```

### Email Verification

Some servers (those open to the public) require new accounts to verify an
email address. If yours does, the create command takes an extra argument:

```
> create MyName mypassword myname@example.com
A 6-digit verification code has been sent to myname@example.com.
Type the code to continue, 'resend' for a new code, or 'cancel' to abort.
> 482910
Email verified.
```

If you don't see the email, type `resend` (limited to once a minute, five per
hour). `cancel` rolls back the half-created account so you can try again with
a different name or email.

### Shared Bank Account

Each character has a personal bank balance, but every account also has a
**shared pile** that any character on the roster can deposit into or withdraw
from. Useful for funding a new alt or pooling gold from a crafting character
back into your main.

Both banks use the same bank rooms and ATMs (no extra travel):

```
> bank
=== Bank Account ===
Personal Balance: 1,250 gold
Shared (account): 4,000 gold
Carried Gold:     180 gold
Total Wealth:     5,430 gold

> bank shared deposit 200
You deposit 200 gold into the shared account.
Shared balance: 4,200 gold

> bank shared withdraw all
You withdraw 4,200 gold from the shared account.
Shared balance: 0 gold
You are now carrying 4,380 gold.
```

`bank help` lists every subcommand. The personal `bank deposit/withdraw`
flow is unchanged.

### Saving Default Settings for New Alts

If you've tuned your settings the way you like — automap on, a particular
prompt mode, color/MXP toggles, etc. — you can save them as account
defaults so every future alt inherits them at creation, and every alt's
session inherits the colors/MXP/abbreviations preferences at login:

```
> set automap on
> set color on
> set defaults save
Account defaults saved. New characters on this account will inherit your
current settings.

> set defaults show
Account defaults (applied to new alts):
  prompt: default
  colors: on
  mxp: off
  abbrev: on
  helpline: off
  summonable: off
  automap: on  (radius 3, ascii off)

> set defaults clear
Account defaults cleared. New characters will use the engine defaults.
```

Defaults are a one-shot stamp at creation/login — once an alt diverges with
its own `set X off`, that character keeps its own value. Run `set defaults
save` again any time to refresh the snapshot.

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
=== MyName the Halfling (Level 28) ===
HP: [########--] 82/100   Stamina: [##########] 100/100
...
Gold: 431

--- Progression ---
Renown: 47   (28 skill levels, 2 mastered, 4 achievements, 3 quests)
Skills:  [###-----------------] 28/180   Mastered: 2/18
Awards:  [#-------------------] 4/89 achievements
Quests:  3 completed, 1 active
Kills:   142     Explored: 87 rooms
Played:  4h 12m
```

The `status` line shows your six attribute scores (Str/Dex/Con/Int/Wis/Cha)
with a `(+N)` parenthetical when equipment or magical buffs are modifying
them — e.g. `Dex: 7 (+2)` while wearing boots that grant Dexterity.

#### Renown

**Renown** is a single number summarising everything you have accomplished.
It is not a character level: nothing is gated on it, no content requires it,
and it is never spent. It exists so a classless world has one figure you can
compare and watch move.

It is derived from:

| Contribution | Weight |
|---|---|
| Each skill level | 1 |
| Each skill mastered (level 10) | +3 |
| Each achievement unlocked | +2 |
| Each quest completed | +1 |
| Each spell mastery level | +½ |

Mastery, achievements and quests count for more than raw skill levels because
a specialist and a generalist can reach the same skill total by very different
routes — the raw sum alone cannot tell them apart.

The `Awards` denominator counts only the achievements you can see; hidden ones
are not revealed until you unlock them.

The `Skills` bar and the `Mastered` count cover the eighteen core skills, so
they always have the same denominator on every character. Languages and any
world-specific skills your world adds still count toward Renown — they are
progression too — and appear on their own `Other` line when you have any.

The `skills` command lists your core skill levels by category (Combat,
Crafting, Magic, Stealth, Utility). If a builder has granted you points in
any world-specific custom skills (e.g. `Dancing Queen`), they appear under
a `Custom (Builder-Defined)` section at the bottom.

### Skill XP feedback

Skills advance constantly and quietly, so the engine reports every award. How
loudly is up to you:

```
> set xpfeed brief
```

| Mode | What you see |
|---|---|
| `off` | Nothing, not even level-ups |
| `brief` | One batched line per skill, printed just above your next prompt (**default**) |
| `full` | A line per individual award |

In `brief` the line collapses a whole burst — a combat round's worth of swings
becomes one entry:

```
[ +40 short blades  410/550 → 4 ]
```

Reaching a new level always prints its own banner, in `brief` and `full`
alike, and replaces the batched line for that skill:

```
*** Your Short Blades skill rises to 4. ***
    [####------]  next: 800 xp
```

The number shown is what you were actually credited, after traits such as
`prodigy` or `slow_learner` have been applied.

### Your prompt

`prompt verbose` switches from a bare `>` to a status line:

```
[HP:82/100] [ST:64/100] >
```

Extra segments appear only when they apply — mana, air while underwater,
blood pool, and so on. In a fight you also get your opponent:

```
[HP:82/100] [ST:64/100] [a rust-scarred ghoul: Bloodied] >
```

The condition word is the same scale `look` and `examine` use — Unhurt,
Scratched, Wounded, Bloodied, Critical — so you can tell whether to press
the attack or run without spending a round looking. If you are not toe to
toe, the range is appended: `[a ghoul: Wounded | Ranged]`. Melee goes
unlabelled, being the usual case.

`prompt simple` puts it back.

#### Building your own

`simple` and `verbose` are just two saved formats. You can write your own out
of the same pieces:

```
> prompt %h/%H hp %s/%S st %t >
42/60 hp 30/100 st >
```

`prompt tokens` lists everything available and marks the lines that mean
nothing for your character — a mutant sees `%u` marked as applying, a human
does not.

There are two kinds of token. The short ones (`%h`, `%s`, `%g`, `%x`) print a
bare number and leave the layout to you. The long ones (`%{hp}`,
`%{mana}`, `%{target}`) print a whole coloured, bracketed group and print
nothing at all when they do not apply — which is how one format works for a
vampire, a replicant and a mutant at once. `%%` gives you a literal `%`.

A few worth knowing:

| Token | Shows |
|---|---|
| `%h` `%H` | current / maximum hit points |
| `%s` `%S` | current / maximum stamina |
| `%m` `%M` | current / maximum mana |
| `%c` | your own condition word (Bloodied, Scratched, …) |
| `%t` | who you are fighting and how they are holding up |
| `%g` | gold carried |
| `%x` | your Renown |
| `%l` | your alignment band |
| `%{standing:town_watch}` | where you stand with one named faction |
| `%{renown}` `%{morality}` | the same, as bracketed groups |

Standing needs a faction name because there is no such thing as your
reputation in the singular — put the token in twice if you want to watch two
groups at once.

`prompt default` drops a custom format and goes back to whichever preset you
were on. If you want to start from the verbose one and edit it, `prompt` on
its own prints it.

### Achievements and trait points

`achievements` lists every milestone you can see, locked and unlocked. Each
grants a title you can wear with `achievements title <key>`; some also pay
gold, an item, or **trait points**.

```
> achievements
  [X] [C] First Blood
  [ ] [C] Scourge (+2 trait pts)
  [ ] [S] The Compleat (+3 trait pts)

> achievements show compleat
The Compleat [locked]
  Master ten different skills (level 10).
  Title: the Compleat
  Rewards: 3 trait points, 10000 gold
```

Trait points are the only reward that changes what your character *is* rather
than what they carry. Spend them with the `traits` command, the same way you
spent your starting ten during creation — and note that traits are permanent
once accepted.

They are deliberately scarce. Only capstone achievements grant them: the top
rung of a long ladder, like a thousand kills or a hundred completed quests.
Achievements that grant points are marked in the list, so you can decide what
is worth chasing before you start.

### Leaderboards

`top` ranks everyone who has played. Nearly everything the game counts about
you is a board: renown, kills, deaths, rooms explored, gold earned, recipes
discovered, every skill, and your standing with every faction.

```
> top
Renown  (top 10 of 214)
   1  Ada                   412
   2  Brannock              388
   ...
  10  Sethri                203
   ...
  34  Yourself              118
Updated 2 minutes ago.
```

| Command | Shows |
|---|---|
| `top` | The renown board |
| `top boards` | Every board there is, grouped |
| `top <board>` | One board — `top cooking`, `top kills`, `top gold.earned` |
| `top me` | Every board *you* rank on, your best placing first |

A board shows ten names, but you are not competing against ten people. If you
rank anywhere at all your own placing is added below the list, so `top` always
tells you where you actually are.

Factions rank both ways. `top town_watch` is who they think best of;
`top town_watch.wanted` is who they would most like to see in a cell. A board
only exists once somebody qualifies for it, so the second one appears the first
time anyone gives that faction a reason.

**`top me` is the one worth knowing about.** Nobody is near the top of
everything, and a single board will not tell you what you are quietly good
at — being 300th at killing and 2nd at cooking looks identical from the
combat board. `top me` sweeps the lot and leads with your best.

Board names are matched loosely: `top kills` finds the kill board, `top short`
finds short blades. Admin characters are never ranked.

The boards are redrawn every few minutes rather than the instant you do
something, which is what the "Updated N ago" line is telling you. If the
number has not moved yet, it will.

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

### Socials

In addition to the free-form `emote` command, IronMUD ships ~490
predefined social commands you can use directly — `smile`, `wave`,
`bow`, `dance`, `hug`, `nod`, `grin`, `kiss`, `laugh`, `pout`, and
many more. Each renders three flavoured variants: you see one line,
the room sees another, and the target (if any) sees a third.

```
> wave alice
You wave at Alice.
(Alice sees:) MyName waves at you.
(Others see:) MyName waves at Alice.

> bow
You bow deeply.

> smile self
You smile at yourself.
```

Tab-completion lists socials alongside built-in commands. Most socials
require you to be standing — `groan` works while sitting; almost
nothing works while sleeping.

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

NPCs that have a **dialogue tree** instead of simple keywords will offer numbered choices when you initiate the conversation. Type the keyword next to a choice to follow it. Some choices are limited (one-shot per player) or have a cooldown — the NPC will let you know.

### Consignment Brokers

Some NPCs run a **consignment shelf**: you leave an item with them at a price
you choose, and any other player can buy it. You do not have to be online when
it sells.

```
> consign iron helm 120
You hand over an iron helm . Bram the Broker sets it out at 120 gold, minus 10% when it sells.

> consignments
Your consignments
   1. an iron helm                       120 gold   expires in 6d 23h
      with Bram the Broker
```

The proceeds — the price minus the broker's cut — go straight to your **bank**,
so you can be anywhere when the sale happens. Buyers use `list` to see the
shelf and `buy <number>` to take something off it.

If the broker is also a shopkeeper, or is just standing next to one, `list`
prints one page: the shop's own goods first, then the shelf, numbered straight
through. `buy 7` always means the seventh line you were shown.

Things worth knowing:

- **The broker's cut is destroyed, not paid to anyone.** That is deliberate: it
  is the only thing keeping gold from piling up forever, which is what makes
  prices mean anything.
- **You cannot price anything you like.** A listing has to be between a quarter
  of the item's value and ten times it. Anything outside that is not a sale, it
  is a hand-off with extra steps, and the broker will say so and name the
  actual limit.
- **Corpses, quest items and bound items are refused.** So is coin for coin.
- **An unsold listing expires after a week into escrow**, not into nothing. You
  can still get it back — see your `escrow`.
- Take something back with `unconsign <number>` or `consignments take <number>`
  — **at the broker holding it**. `consignments` names which broker has what,
  so you know where to go. A shelf is a shop counter, not a courier.

Two brokers in different towns keep separate shelves, so it is worth checking
more than one before deciding what something is worth.

### Asking About Other People

Simulated NPCs (townsfolk, neighbours) keep track of how they feel about each other. You can ask them:

```
> ask gregor about esme
Gregor smiles. "Esme? I'm very fond of her. We share a home."
```

The NPC must be in your room and the subject must be someone they actually know. `examine` also surfaces social cues — mood, recent bereavement, who they live with.

### NPC Schedules

Many NPCs follow a daily routine. If they've made their schedule visible, ask:

```
> schedule blacksmith
=== Schedule for Old Gregor ===
Currently: working

  8am  (Morning):  working
  8pm  (Evening):  off_duty
  10pm (Night):    sleeping
```

Shopkeepers and healers won't serve you when they're sleeping or off duty.

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

## Quests

NPCs may offer quests through their dialogue trees. When you accept one, it appears in your quest log. Objectives can be kill counts, item turn-ins, room visits, or world-state flags — they update automatically as you play.

| Command | Description |
|---------|-------------|
| `quests` | List your active and completed quests |
| `quest <id>` | Show full detail for a quest (objectives, time remaining, rewards) |
| `quest abandon <id>` | Drop an active quest (forfeits any progress) |

Some quests are **time-limited** — abandon or complete them before the timer
expires. Kill credit is shared across the party — see below for exactly who
counts.

## Grouping

Following someone puts you behind them; being **grouped** makes you part of what
they do. The leader adds you with `group <name>`, or `group all` to add every
follower at once.

| Command | Description |
|---------|-------------|
| `follow <name>` | Move when they move |
| `group` | Show the party panel |
| `group <name>` | Add or remove a follower from your group (leader only) |
| `group all` | Add every follower at once |
| `ungroup` | Leave the group |
| `gtell <message>` | Speak to the group |
| `split <amount>` | Divide gold among grouped members in the room |

`group` prints a live panel: each member's health bar and condition band,
stamina, position, and what they are currently fighting. Anyone in another room
shows as `(elsewhere)` with no vitals — they are not in your fight, so their
numbers would only mislead you.

```
Group led by Kaleth:
  Kaleth        [########--] Scratched  ST  45/60 standing (Leader) <-- You
                fighting a rust-scarred ghoul
  Medic         [###-------] Bloodied   ST  12/50 standing
  Scout         (elsewhere) (Following)
```

The `fighting` line is the one thing no other display shows you: at a glance
you can see whether the party is focusing one target or has split across three.

### Who gets credit for a kill

Everything a kill produces — the kill counter, quest progress, worship favor,
alignment, and faction standing — goes to **every credited participant**, not
only whoever landed the last blow. You are credited if you either:

- dealt any damage to the target during the fight, **or**
- were grouped with the killer and **in the room** when it died.

That second rule is deliberate. A healer or a buffer who carried the fight deals
no damage at all, and a credit rule built only on damage would tell them they
were never there. Being grouped somewhere else does not count — you have to be
in the room.

The consequences travel with the credit, in both directions. Help put down a
town watch guard and the watch holds it against you too; that is not a loophole,
it is the point.

Weapon skill XP is the exception, and it is not shared: it is earned per landed
blow, so everyone who swung is already paid for their own swings.

## Dying

Death costs you everything you were carrying. Your corpse drops where you fell,
holding your inventory, everything you were wearing, and every coin on you. You
wake at your bind point with a quarter of your health and nothing else.

Getting it back is a **corpse run**, and you are on a clock.

**Your corpse rots.** After an hour it and everything inside it is gone for
good. You will be told when it is halfway there and again when it is nearly
gone, wherever you are — the warnings reach you, so you always know how much
race you have left.

**Nobody else can touch it for the first five minutes.** For that window your
corpse only opens for you and for the people grouped with you. Following
someone does not count; they have to actually be in your group. After the
window lapses, it is open to anyone who finds it. Five minutes is not a lot,
which is the point — it is enough for a party to carry a body home, not enough
to leave it lying while you go do something else.

**If you have lost the body, ask.**

```
> locate corpse
You go still, and something older than you looks out through your eyes.

  Your body lies at The Sunken Causeway, in Riverwatch.
    It has 34m before it is gone.
```

`locate corpse` is a divination, not a map: it names the room and the area and
stops there. Getting there is still your problem.

It also requires a god who is listening. You must be worshipping someone and
have reached at least **Noticed** with them. If you serve nobody, or you have
been taking your deity for granted, the silence you get back is an answer of
its own — see [Gods and Worship](#gods-and-worship).

*(Server operators can tune every number above, including turning loot
protection off entirely. Ask on your server if the rules feel different.)*

## Alignment

Every meaningful thing you kill says something about you. Slay something evil
and you drift toward Good; cut down something good and you drift toward Evil.
Most creatures — vermin, constructs, wildlife — carry no moral charge at all
and move you nowhere.

You will never see a number. What you see is a line on `status` when your
standing settles into a new band:

```
You feel a quiet warmth in your heart.
```

There are nine bands, from *pure evil* through *neutral* to *pure of spirit*.
Crossing between them prints a line like the one above; drifting within one is
silent, so you are told when something has changed and left alone otherwise.
Reaching one of the two extremes takes something like a campaign's worth of
consistent deeds, and the extremes are *sticky* — once you are genuinely
notorious, a single contrary act will not launder it.

Quests and conversations move it too. A quest whose ending is a moral choice
will shift you by that ending, and some dialogue options carry weight of their
own — sparing someone, informing on someone.

It is not only bookkeeping. Some creatures in the world hunt by alignment: they
attack the wicked on sight, or the virtuous, or those who have committed to
neither. A reputation you earned is a reputation something else can smell.

## Standing With Factions

Alignment is what the world thinks of your character. **Standing** is what
particular groups think of you, and they keep separate books.

Kill a town guard and the Town Guard notices. Kill twenty and they will not
serve you, will not talk to you, and eventually will not wait for you to draw
first. Meanwhile the road bandits the guard exists to suppress have been
watching, and they think rather better of you than they did.

```
> standing
Where you stand:
  The Town Guard          Hostile       [###-------] -260
  The Roadmen             Accepted      [#####-----] 130
```

`standing <faction>` gives you the detail — what the band means, and who else
it costs you:

```
> standing guard
The Town Guard
Underpaid, over-extended, and the only thing standing between the settled
roads and everything that walks them.
Standing: Hostile (-260)
They consider you an enemy.
At odds with: The Roadmen, The Restless
Earning their goodwill costs you standing here, and the reverse.
```

Seven bands run from **Hated** through **Neutral** to **Revered**. A group you
have never dealt with is Neutral and does not appear in the list at all — the
list is a record of what you have done.

Four things follow from standing:

- **Prices.** A merchant who counts you a friend charges you less and pays you
  more. One you have wronged does the opposite. `list` and `appraise` quote you
  the real number, not the sticker price.
- **Aggression.** Fall far enough with a group and its members stop waiting to
  be provoked. How far is "far enough" varies — some are quicker to take
  offence than others.
- **Conversation.** Some dialogue only opens to people a faction trusts.
- **Work.** Some quests are never offered until you have proved yourself, and
  the questgiver will not even hint at them.

**Fighting can only lower standing.** It rises two ways: by killing a
faction's enemies, and by doing a faction's work. That is deliberate — if
every group liked everyone eventually, standing would not mean anything. The
kill that buys you the Guard's goodwill costs you the Roadmen's, and where you
end up is a record of what you chose rather than how long you played.

Like alignment, crossing a band tells you and moving inside one does not.

## Gods and Worship

Some beings in the world are true gods. Swear yourself to one and they will
bless you — but they will also expect tribute, and gods do not forgive
neglect.

| Command | Description |
|---------|-------------|
| `worship` | Show your god and your standing with them |
| `worship <god>` | Swear the pact (god present, or named in a temple) |
| `pray` | In a temple: offer devotion; blessed if your tribute is current |
| `pray tribute` | Pay what the god demands (usually a share of your gold) |
| `pray atone` | Surrender half your wealth to lift divine wrath |

Things to know before you kneel:

- **The pact is for life.** There is no command to leave a god or switch
  to another. Choose carefully.
- You can't worship a god just because you've heard of them — most demand
  an **artifact** (consumed in the ritual) or a **deed** (a completed quest).
- Blessings fade after a few days; pray regularly in any room that serves
  as a **temple** to keep them.
- Miss your tribute and the god's anger grows day by day: cold warnings,
  then curses, then a smite. Some gods can inflict wounds that never heal.
  `pray atone` is the expensive way back into grace.
- Harming your god's faithful — their priests, their creatures, their
  worshipers — is punished. Slaying the followers of your god's **enemies**
  earns favor.
- `examine` a god (or their servants) to learn who and what they are.

### Standing

`worship` shows where you stand, not a bare score:

```
Standing: Favored (127)
```

| Standing | Meaning |
|----------|---------|
| Anathema | Nothing you offer will be accepted. |
| Disfavored | Your god's regard has soured. |
| Unproven | Where everyone begins. |
| Noticed | Your god has taken note of you. |
| Favored | Your god looks upon you with favor. |
| Blessed | Your god's blessing rests upon you. |
| Exalted | You stand among your god's exalted. |

Your god tells you when you cross between standings, in either direction.
Smaller movements pass in silence, so a long run of kills doesn't narrate
every one.

Outside a temple, `pray <message>` still simply calls out to the
administrators.

## Bulletin Boards

Some rooms contain bulletin boards (a piece of furniture or a notice board). Use the `board` command while standing next to one:

```
> board list
=== Town Notices ===
  1. [Mayor]    Tax day reminder       (3 days ago)
  2. [Innkeeper] Lost cat              (1 day ago)

> board read 2
[Lost cat] My tabby slipped out the back door...

> board write Looking for adventurers
(opens the multi-line editor — `.save` to post, `.abort` to cancel)

> board remove 5
```

Some boards are admin-only for reading or writing — the board will tell you if you don't have access.

## Pets and Charmed Mobs

Casting `charm` on a non-immune NPC compels them to follow your orders. Most charms are **temporary** (the duration is the spell's effect length); NPCs flagged `tameable` form a **permanent pet bond** instead.

| Command | Description |
|---------|-------------|
| `order <mob> attack <target>` | Make a charmed mob attack |
| `order <mob> stay` | Tell the mob to remain in its current room |
| `order <mob> follow [<player>]` | Follow you or another (online) player |
| `order <mob> drop` | Release the charm |
| `order group <subcommand>` | Issue the same order to every charmed mob in your room |

Charmed mobs follow you between rooms by default unless told to `stay` or to follow another player. Mobs flagged `no_charm` are immune. Logging out (`quit`) breaks all charms you've cast.

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

## Crafting and Discovery

`craft <recipe>` and `cook <recipe>` make things you already know how to
make; `recipes crafting` and `recipes cooking` list those. Recipes reach you
by rising skill, by reading a book, or as a quest reward.

### Working it out yourself

`experiment` is the one that does not wait to be told.

```
> experiment flour, water
*** You have discovered how to make coarse bread! ***
You produce a loaf of coarse bread.
```

Name the materials you want to put together, separated by commas (`and` and
`with` work too). If they are exactly what some recipe needs — every
ingredient covered, nothing left over — you may work out how it is made, and
the recipe is yours from then on.

**The materials are used up either way.** That is what makes a guess a
decision. What a failure buys you is knowing how close you were:

```
> experiment flour
They begin to come together — and then stop. You are something short.
```

That is worth more than the flour. A combination that means nothing at all
says so, and teaches you nothing:

```
> experiment boot, boot
You work at the materials and get nothing out of them but waste.
```

Three things will not work no matter how right your materials are:

- **A recipe you already know.** There is nothing left to discover.
- **Anything measured out of a container** — a recipe needing water by the
  unit rather than by the flask has to be made the ordinary way.
- **Tools you do not have.** If the work needs a forge, you need a forge —
  and finding that out costs you the materials, same as any other failure.

Skill decides whether you can see what is in front of you. At the recipe's
own level you have an even chance; every level above that improves it, and
harder recipes resist. Failing with the right materials still teaches you a
little, so the skill creeps up even on a bad day — though never fast enough
to beat simply making things you already know.

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

### Managing Your Email

If the server uses email, keeping a verified address on your account lets you
recover a forgotten password yourself with `forgot` at the login screen. Manage
it any time with the `email` command:

```
> email
Account email: me@example.com  (verified)

> email set me@example.com      # sends a 6-digit code to that address
> email verify 123456           # confirm the code
> email resend                  # send a fresh code (limited to once a minute)
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
| Magic Missile | 1 | 15 | Fires a bolt of arcane energy at a target |
| Light | 1 | 10 | Creates a magical light source |
| Firebolt | 2 | 25 | Hurls a bolt of fire at a target |
| Frost Bolt | 2 | 25 | Hurls a bolt of cold at a target |
| Arcane Shield | 2 | 20 | Grants a temporary armor bonus |
| Detect Magic | 2 | 15 | Reveals magical auras on items and creatures |
| Night Vision | 2 | 15 | Lets you see in dark rooms (CircleMUD AFF_INFRAVISION) |
| Cure Wounds | 3 | 25 | Heals yourself or an ally |
| Detect Invisible | 3 | 20 | Reveals invisible creatures |
| Sleep | 3 | 25 | Puts a target to sleep — they skip turns until damaged or the buff expires |
| Blind | 3 | 25 | Reduces a target's hit chance |
| Fear | 3 | 30 | Floods a target with terror — mobs flee every round; feared players may bolt or freeze (PvP zones only) |
| Courage | 3 | 25 | Grants temporary fear immunity to yourself or an ally and cures existing fear |
| Charm | 4 | 35 | Compels an NPC to obey you (see [Pets and Charmed Mobs](#pets-and-charmed-mobs)) |
| Summon | 4 | 40 | Yanks a willing target to your room from anywhere in the world |
| Lightning Bolt | 4 | 40 | Strikes a target with lightning |
| Control Weather | 4 | 35 | Influence the weather in your area |
| Animate Dead | 5 | 60 | Raise a corpse as a temporary undead servant |
| Invisibility | 5 | 35 | Makes yourself invisible |
| Dispel Magic | 5 | 30 | Removes magical buffs from a target |
| Haste | 6 | 45 | Increases your movement speed |
| Greater Heal | 6 | 50 | Powerful healing spell |
| Sanctuary | 7 | 50 | Powerful damage reduction |
| Meteor Storm | 8 | 80 | Devastating area attack (scroll-only) |

Spells marked "scroll-only" cannot be learned through skill advancement alone and must be found on scrolls in the world.

**Resists:** mobs flagged `no_charm`, `no_summon`, `no_sleep`, `no_blind`, or `no_fear` are immune to the matching spell — the cast still consumes mana but has no effect. Players consenting to be summoned set this with `set summonable on`.

**Fear:** while terrified you cannot start fights, and each combat round you may bolt for an exit or freeze in panic. Synth characters, constructs, undead, and anyone under a `Courage` buff are immune. Fear can also ride on items: cursed food and drink frighten whoever consumes them, and weapons or armor bearing a *dread aura* terrify the wearer's enemies in combat.

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

### Combat Maneuvers

| Command | Description |
|---------|-------------|
| `bash <target>` | Charge a target for modest damage and a brief stun on hit. Standing-only; spends 15 stamina even on a miss. Mobs flagged `no_bash` resist the stun. |

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

## Cyberware and Humanity

On modern/cyberpunk worlds, most races can have **cyberware** installed by
a ripperdoc NPC (synths and revenants cannot; the augmented race is *born
chromed* and pays less for installs). Every piece of chrome costs
**Humanity** — your ceiling is your base Charisma × 10, each installed
piece lowers it, and each install spends from the pool.

```
> cyberware              (alias: chrome)
Chrome:
  Humanity (chrome): 86/86 (100%) — integrated
Installed cyberware:
 neuralware
  a coiled neural link processor [1/5 slots]
  ...
```

Run the ledger low and the chrome starts driving: below 30% Humanity you
risk dissociative episodes, and near zero you risk violent cyberpsychotic
breaks that attack whoever is nearby. Lost Humanity also erodes your
effective Charisma (−1 per 10 lost). Therapy (sold by clinic NPCs)
restores spent Humanity; removing chrome restores your ceiling but never
the points already spent.

Cyberware items can't be worn or wielded — find a ripperdoc to install or
remove them. Foundations (a neural link, cybereyes, cyberarms…) provide
option slots that smaller implants plug into.

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

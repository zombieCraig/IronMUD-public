# DG Scripts in IronMUD

DG Scripts is the trigger language used by tbamud and CircleMUD. IronMUD ships a native Rust interpreter so imported tbamud content (~1900 stock triggers) runs unmodified and so builders can author triggers in DG alongside Rhai.

This page is a builder-facing reference focused on what IronMUD actually implements. It supersedes the dgscripts.com docs for IronMUD-specific behavior.

> **Already familiar with DG?** Skim [IronMUD-specific notes](#ironmud-specific-notes) for the differences. Otherwise read top-to-bottom.

## When to pick DG vs Rhai

Both languages run on the same trigger surface (room / item / mobile, with `chance` and `interval`). They differ in feel:

| Use **DG** when... | Use **Rhai** when... |
|---|---|
| Porting tbamud / CircleMUD content | Writing new content from scratch |
| You want a tiny, line-oriented language | You want full programming with arrays, maps, fns |
| You need `wait` (cooperative pauses) | You don't need to suspend |
| You want to share idioms with the wider DG community | You need access to IronMUD-only systems Rhai exposes (recipes, transports, social/needs, etc.) |

A single trigger is *either* DG or Rhai — the body picks. You can mix DG and Rhai triggers on the same entity.

## Quick start: edit a DG trigger

DG triggers live in the same `triggers` array as Rhai triggers. The OLC editors (`medit` / `oedit` / `redit`) gain a `trigger dg <subcmd>` family.

```
> medit guard trigger dg add greet greet_player
[opens the DG editor; type body, terminate with @ on its own line]
> medit guard trigger dg list
0: [ON]  on_greet -> greet_player (DG, 100% chance)
> medit guard trigger dg view 0
> medit guard trigger dg edit 0
```

Subcommands:

| Subcommand | Effect |
|---|---|
| `add <type> [name]` | Create a new empty DG trigger of `<type>`, open the editor |
| `view <idx>` | Paginated read-only view of the body |
| `edit <idx>` | Re-open the editor on an existing DG trigger |
| `retype <idx> <type>` | Change the trigger's type without touching the body (see [Retyping triggers](#retyping-triggers)) |
| `attach <vnum>` | Attach an imported DG trigger prototype (from `.trg` import) |
| `detach <idx>` | Break the proto link on a single instance (allows per-instance divergence) |
| `makeproto <idx> <vnum>` | Promote a host-local trigger to a proto so siblings can attach |
| `protos` | List the imported DG trigger prototypes by vnum |
| `proto new <vnum> <flags> [name]` | Create empty proto, open editor |
| `proto view <vnum>` | Show proto metadata + body |
| `proto edit <vnum>` | Edit proto body; refresh sweep updates all attached instances on save |
| `proto retype <vnum> <type-or-flags>` | Change a proto's trigger-type and refresh attached siblings (see [Retyping triggers](#retyping-triggers)) |
| `proto delete <vnum>` | (admin) Orphan attached instances and delete the proto |
| `list` | Show all DG triggers on this entity |

The editor uses the same OLC mode as `oedit note` — type lines, then `@` on a line by itself to save, or `~` to cancel. 32 KB cap.

### Retyping triggers

Choosing the wrong trigger type is a common bug — a body authored for "item given to mob" needs `on_receive`, but typing it as `on_bribe` (which fires on gold gifts) makes the script silently never run. `retype` swaps the type in place, no re-paste required.

**For a host-local trigger:**

```
> medit guard trigger dg retype 1 receive
Trigger #1 retyped to 'receive'.
```

Accepts the same friendly type names as `add` (see the [Trigger types](#trigger-types) tables). Refuses if the trigger has no DG body (template/Rhai triggers) or is attached from a prototype (use proto retype below).

**For a proto-attached instance:**

```
> medit guard trigger dg proto retype mob_greet_pack receive
Proto 'mob_greet_pack' retyped (input: 'receive'). Refreshed 3 instance(s).
```

Mutates the proto's flag letters and runs the refresh sweep, so every attached sibling re-derives its type from the new flags. Accepts either:

- **Friendly type names** (recommended): `receive`, `bribe`, `greet say` (multi-type, space-separated)
- **CircleMUD letter-flag string**: `j` = OnReceive, `m` = OnBribe (bit positions, not mnemonics — `b` is OnIdle, not bribe). Use this only for parity with imported `.trg` content.

If you don't intend to change *all* attached siblings, run `trigger dg detach <idx>` on the instance you want to diverge first, then `trigger dg retype` on the now-host-local copy.

## Trigger types

DG triggers fire on the same events as Rhai triggers. The type names below are what `trigger dg add <type>` accepts.

### Mobile

| Type | When it fires | Cancel? |
|---|---|---|
| `on_greet` | Player enters mob's room | No |
| `on_say` | Player says something in mob's room | No |
| `on_idle` | Periodic, while players present | No |
| `on_attack` | Mob is attacked | No |
| `on_death` | Mob dies | No |
| `on_fight` | Each combat round (mob in combat) | No |
| `on_hit_percent <N>` | Mob's HP crosses below N% | No |
| `on_receive` | Player gives mob an item | No |
| `on_bribe` | Player gives mob gold | No |
| `on_load` | Mob spawned from prototype | No |
| `on_command <prefix>` | Player types a command matching `<prefix>` while in the mob's room | **Yes** (`return 0`) |

### Item

| Type | When | Cancel? |
|---|---|---|
| `on_get` | Player picks up | Yes |
| `on_drop` | Player drops | Yes |
| `on_use` | Player drinks/eats | Yes |
| `on_examine` | Player examines | No |
| `on_wear` | Player `wear`s the item (armor/clothing/jewelry) or DG mob `wear` | No |
| `on_wield` | Player `wield`s the item (weapons, off-hand, fishing rod) or DG mob `wield` | No |
| `on_remove` | Item is removed from equipped slots | No |
| `on_load` | Item spawned from prototype | No |
| `on_command <prefix>` | Player types matching command while item is in inventory or equipped | **Yes** |

### Room

| Type | When | Cancel? |
|---|---|---|
| `on_enter` | Player enters | Yes |
| `on_exit` | Player leaves | Yes |
| `on_look` | Player looks | No |
| `periodic` | Every `interval_secs` | No |
| `on_time_change` | Time-of-day boundary | No |
| `on_weather_change` | Weather changes | No |
| `on_season_change` | Season changes | No |
| `on_command <prefix>` | Player types matching command while in the room | **Yes** |

`on_command` keyword match is case-insensitive equality OR mutual prefix (DG's `/=` semantics) — `on_command get` matches both `get` and `g`.

## Anatomy of a trigger body

```
* Greet a player who enters the room
if %actor.level% < 5
  %send% %actor% The guard eyes your inexperience warily.
else
  emote bows respectfully to %actor.name%.
end
```

- **Comments** start with `*`.
- **Each line is a statement.** No semicolons. Blank lines OK.
- **Variable interpolation** uses `%head.field%`. `%actor%` alone resolves to actor's name.
- **Commands** are bare verbs (`emote ...`) or wrapped (`%send% ... msg`).

## Variables

DG resolves `%head.field%` against either the firing context, the active entity, or the script's local vars.

### Context heads

| Head | Bound when | Notes |
|---|---|---|
| `actor` | Player triggered the event | The PC or, for arg-as-actor, the named target |
| `victim` | Combat / damage triggers | Often the mob's current opponent |
| `self` | Always | The entity the trigger lives on |
| `arg` | `on_say`, `on_command` | Player's full input after the verb |
| `cmd` | `on_command` | Verb the player typed |
| `random` | Always | RNG / random PC / random direction |
| `time` / `weather` / `season` / `sunlight` | Always | World / area state |
| `findmob` / `findobj` | Always | World-wide vnum lookup |

### Common fields on actor / victim / self

These work on any character or mob head (`actor`, `victim`, `self`-when-mob).

| Field | Returns |
|---|---|
| `name` | Display name |
| `id` | UUID (mobs) or player name (PCs) |
| `level` | Level |
| `hp` / `maxhp` / `hitp` / `maxhitp` | Current / max HP |
| `mana` / `maxmana` | Current / max mana |
| `move` / `maxmove` | Stamina |
| `gold` | Gold |
| `vnum` | Mob prototype vnum (`-1` for PCs) |
| `class` | Class name (PCs) |
| `race` | Race |
| `is_pc` | `1` for player, `0` for mob |
| `sex` / `gender` | Resolved gender |
| `heshe` / `himher` / `hisher` / `hers` | Pronouns |
| `room` | Current room id (UUID) |
| `fighting` | Name of current opponent, or empty |
| `master` | Charm master (mobs) / following target (PCs) |
| `hunger` / `thirst` / `drunk` | PC-only meaningful values; mobs return `0` (sim mobs return real `hunger`) |
| `pos` / `position` | `"standing"` (no posture system on PCs yet) |
| `align` / `alignment` / `morality` | Morality slider (-200..=+200; tier thresholds at +/-100). All three names return the same value. Note: scale is narrower than tbamud's -1000..1000, so imported numeric thresholds need rescaling. |
| `morality_tier` | Tier key: `evil_pure`, `evil_3`, `evil_2`, `evil_1`, `neutral`, `good_1`, `good_2`, `good_3`, `good_pure`. Prefer this over numeric `align` comparisons. |
| `canbeseen` | Always `"1"` |
| `inventory` | Comma-joined inventory item names |
| `inventory(<vnum>)` | Count of items in inventory with that vnum |
| `equipped` | Comma-joined names of currently-equipped items |
| `equipped(<vnum>)` | Count of equipped items with that vnum (use for armor-set detection) |
| `has_item(<vnum>)` | `"1"` if held *or* worn, else `"0"` |
| `eq(<slot>)` | Name of the equipped item in `<slot>`, or first-equipped fallback when `<slot>` is empty / unrecognized. Slots accept `WearLocation` names: `head`, `neck`, `shoulders`, `torso`, `waist`, `left_hand`, `right_hand`, `wielded`, `offhand`, `left_finger`, `right_finger`, `left_foot`, `right_foot`, etc. |
| `varexists(<name>)` | `"1"` if `<name>` is a local or `dg_var` on the entity |
| `affect(<spell>)` | `"1"` if matching `EffectType` is in `active_buffs` |

#### Mutating accessors

These read **and modify** in one call, returning the post-change value.

```
nop %actor.gold(-50)%        * deduct 50 gold
nop %actor.hitp(20)%         * heal 20 hp (clamped to maxhp)
nop %actor.move(-10)%        * spend 10 stamina (clamped to 0)
nop %actor.drunk(5)%         * +5 drunk (clamped 0..100)
nop %actor.hunger(-20)%      * make hungrier (clamped 0..max_hunger)
nop %actor.thirst(-20)%      * thirstier
%actor.exp(100)%             * silently no-op (no PC progression yet)
```

`nop` evaluates its argument and discards the result, which is how you fire a mutating accessor purely for side effects.

### Self-as-item / self-as-room fields

When `self` is an **item**:

| Field | Returns |
|---|---|
| `name` / `vnum` / `weight` / `type` | Basics |
| `shortdesc` / `longdesc` | Descriptions |
| `cost` | Item value |
| `timer` | Decay timer (set with `otimer N`); persists in `dg_vars["timer"]` |
| `val0..val3` | Always `"0"` — IronMUD doesn't model per-slot values |
| `carried_by` / `worn_by` | Owner name when in inventory / equipped |
| `contents` | Comma-joined contents of containers |

When `self` is a **room**, or with chained `%head.room.field%`:

| Field | Returns |
|---|---|
| `name` / `title` | Room title |
| `vnum` / `id` | Identifiers |
| `description` / `desc` | Long description |
| `north` / `south` / `east` / `west` / `up` / `down` | Destination room id, or empty when no exit |
| `people` | Comma-joined list of mob+player names in the room (excluding self) |

Chained example: `%self.room.vnum%` reads the vnum of the room the mob is currently in. `%actor.room.people%` lists everyone with the actor.

#### Area roster — `%head.area%` and `%head.area.*%`

Where `%head.room.*%` reaches one room, `%head.area.*%` reaches **every room in
the same area**. `head` is `self`, `actor`, or `victim`; the area is resolved
from that entity's room. Returns are comma-joined name lists, just like
`people`. Works from item, mob, and room triggers alike — e.g. an item trigger
can ask who is anywhere in its zone, not just its own room.

| Accessor | Returns |
|---|---|
| `%head.area%` | The area's UUID (empty when the entity has no room/area) |
| `%head.area.people%` | All players **and** mobs across the area |
| `%head.area.players%` (alias `pcs`) | Players only |
| `%head.area.mobs%` | Mobs only — can be a large list area-wide |
| `%head.area.people(<f>)%` / `.players(<f>)` / `.mobs(<f>)` | Same, filtered by `<f>` |

The filter `<f>` matches case-insensitively when an occupant's **name contains**
`<f>`, or (mobs) any **keyword starts with** `<f>`, or its **vnum equals** `<f>`
— so `%self.area.mobs(rat)%` lists every rat in the zone. Count occupants by
splitting the result on commas; test presence with a substring `if` check.

```
* item OnUse: warn the wielder how many rats infest the zone
osend %actor% Rats nearby: %self.area.mobs(rat)%
```

##### Reading fields off a roster name

A **player** name pulled out of `%self.area.players%` (or `.people`/`.pcs`)
can be read for any character field — even though that player stands in a
different room of the area. Assign the name to a local with `.car`, then
access the field on the local (`%who.level%`, `%who.class%`, `%who.name%`).
The lookup matches by name across every room in self's area, and tolerates
the trailing comma that `.car` leaves on each token. It resolves **players
only** — mob names embed spaces and aren't unique, so they don't coerce.

```
* item OnExamine: a leaderboard of everyone crawling this zone
set list %self.area.players%
osend %actor% Crawlers
osend %actor% =======
while %list.strlen% > 0
  set who %list.car%
  osend %actor% %who.name% Level: %who.level%
  set list %list.cdr%
done
halt
```

`%who.name%` returns the canonical (cased) name with the comma stripped, so
the display stays clean. A name that resolves to no player in self's area
falls through to the plain text-field reader (empty for fields like `level`).

##### Iterating mobs by id — `%head.area.mob_ids%`

Mob *names* can't be iterated — they embed spaces (`a rat`) and aren't unique,
so `.car` can't pull a usable token out of `%self.area.mobs%`. Use
`%head.area.mob_ids%` instead: it returns the mobs' **UUIDs** (space-free,
unique, and filterable just like `.mobs`). A UUID held in a local resolves any
field via the remote-entity reader (`%m.level%`, `%m.hp%`, `%m.vnum%`, …), and
DG commands accept a UUID as a target — so you can both **read** and **act on**
each mob. There is no `player_ids` (PCs have no stable UUID); iterate players by
name with `.players`.

```
* item OnExamine: report every guard above level 10 in the zone, and weaken them
set list %self.area.mob_ids(guard)%
while %list.strlen% > 0
  set m %list.car%
  if %m.level% > 10
    osend %actor% %m.name% (level %m.level%) shudders as the beacon flares.
    mdamage %m% 5
  end
  set list %list.cdr%
done
halt
```

The trailing comma `.car` leaves on each id is stripped before the lookup, so
no manual cleanup is needed. `mob_ids(<f>)` takes the same filter as `.mobs`
(name-contains / keyword-starts-with / vnum-equals) — there is no level filter,
so test level (or any other field) inside the loop as shown.

#### Door state — `%self.door(<dir>, <field>)%`

Call-form accessor for inspecting the door on self's room (Mob self resolves to the mob's current room; Room self is the room). Direction accepts long names (`east`) or one-letter shortcuts (`e`, `n`, `s`, `w`, `u`, `d`).

| Field | Returns |
|---|---|
| `exists` | `"1"` if there's a door in that direction, else `"0"` |
| `open` / `closed` | `"1"`/`"0"` (mutually exclusive) |
| `locked` / `unlocked` | `"1"`/`"0"` |
| `pickproof` | `"1"`/`"0"` |
| `name` | Door name (e.g. `gate`); empty if no door |
| `key` / `key_vnum` | Key vnum string; empty if no door or no key |

Missing-door cases return `"0"` for boolean-shaped fields, so `if %self.door(east, locked)%` composes cleanly without a separate `exists` check.

Canonical guard-relock pattern — close + relock without per-tick spam:

```
if %self.has_item(3001)%
  if %self.door(east, open)%
    mdoor %self.room% east flags closed
    mecho %self.name% pushes the east door shut.
  end
  if %self.door(east, unlocked)%
    mdoor %self.room% east flags lock
    mecho %self.name% turns the heavy key in the lock.
  end
end
```

> The `%self.<dir>%` style (e.g. `%self.east%`) returns the *destination room id* on rooms, not door state — use `%self.door(<dir>, <field>)%` when you need to branch on lock/open status.

### Time, weather, season, sunlight

```
%time%                     * synonyms below
%time.hour%                * 0-23
%time.day%                 * day of month
%time.month%               * month name
%time.year%                * year
%time.season%              * spring / summer / autumn / winter
%time.period%              * dawn / morning / noon / afternoon / dusk / evening / night

%weather%                  * sky slug: clear/cloudy/rain/snow/...
%weather.sky%              * same
%weather.desc%             * full WeatherCondition Display string
%weather.temp%             * effective temperature (F)
%weather.tempcat%          * cold / cool / mild / warm / hot

%season%                   * shorthand for %time.season%
%sunlight%                 * "1" during dawn..dusk, else "0"
```

> **Area-aware**: `%weather.*%` projects the *global* rolled weather through the source room's `ClimateProfile`. A tropical area never reports snow even when the global weather rolled blizzard; an arid area's `tempcat` reflects the area's `temperature_offset`. Set climate via `aedit climate <preset>`.

### Random

```
%random.10%       * integer 1..10
%random.char%     * random PC currently in the source room (empty if none)
%random.dir%      * random non-None exit name from the source room
```

### Find by vnum

```
%findmob.230%             * UUID of first live (non-prototype) mob with vnum 230
%findmob.230(189)%        * same, with fallback vnum 189 if 230 has no instances
%findobj.82%              * UUID of first live item with vnum 82
```

Truthy in `if` blocks when at least one instance exists.

### Locals, globals, remote

```
set s "the merchant"               * local var; lives only for this trigger
global counter                     * promote 'counter' to durable per-entity store
set counter 0                      * persists on self.dg_vars across reboots
unset counter                      * clear all scopes

set zn118_state 1                  * stage value in a local
remote zn118_state %actor.id%      * write current local to actor's dg_vars
remote greeting %actor.id% Welcome back!   * (IronMUD ext.) 3-arg form: write the substituted value directly
%actor.zn118_state%                * read it back later
rdelete zn118_state %actor.id%     * delete the entity-side var

context %actor.id%                 * switch durable scope to that entity
```

Lookup order for bare `%name%`: locals → context-bound durable → entity-resolved durable.

> **`remote` 3-arg form** is an IronMUD extension. Stock tbamud's `remote VAR TARGET` always writes the current local value of `VAR`; the 3-arg `remote VAR TARGET VALUE` lets you write a substituted value without first staging it in a local of the same name. The remainder of the line (after the target token) becomes the value, so multi-word values work without quoting. Stock 2-arg triggers are unaffected.

> **Players are valid targets.** `%actor.id%` resolves to the character's name for PCs (IronMUD characters have no UUID — they're keyed by name). `remote` and `rdelete` accept either a UUID (mob/item/room) or a character name.

### `arg` — the player's text

In `on_say` / `on_command`, `arg` is the player's input *after* the verb:

```
%arg%               * full text
%arg.car%           * first whitespace-separated word
%arg.cdr%           * everything after the first word
%arg.strlen%        * character count
%arg.contains(foo)% * "1" if 'foo' appears (case-insensitive)
```

If the field isn't a text op, IronMUD tries to resolve `arg` as an actor name in self's room (Phase 8c). So `%arg.hp%` on `kick guard` looks up `guard` in the room and reads its HP.

### `cmd` — the verb that fired on_command

```
%cmd%               * verb the player typed (possibly abbreviated)
%cmd.mudcommand%    * canonical (un-abbreviated) verb name
```

## Control flow

```
if %actor.level% > 10
  ...
elseif %actor.level% > 5
  ...
else
  ...
end

while %self.gold% < 100
  wait 5 sec
  ...
done

switch %actor.class%
  case warrior
    ...
    break
  case mage
    ...
    break
  default
    ...
    break
done

halt              * stop the script entirely
return 0          * cancel the host action (where supported)
return 1          * proceed normally
```

`eval` computes arithmetic into a local:

```
eval cost %actor.level% * 50 + 25
%send% %actor% That'll cost %cost% gold.
```

Supports `+ - * / %`, parens, unary minus, and full precedence. Overflow falls back to passing the raw substituted string.

### `wait` — cooperative pauses

```
%send% %actor% You feel a strange tingle...
wait 3 sec
%send% %actor% The world snaps back into focus.
%teleport% %actor% 3001
```

When a body contains `wait`, the runtime returns `Outcome::Done` to the host *immediately* and continues asynchronously. **You forfeit cancellation** when you use `wait` — `return 0` after a wait does not roll back the host action. This matches tbamud's behavior.

`wait <N> sec` and `wait <N>` (interpreted as seconds) are both accepted.

## Commands

DG commands are dispatched on the verb (case-insensitive). The `m`/`o`/`w` prefixes (mob/obj/wld) are aliases — they all do the same thing in IronMUD; use whichever your imported source used.

### Messaging

```
%send% %actor% You hear a whisper.            * send to one PC
%echo% A bell tolls in the distance.          * broadcast to self's room
%echoaround% %actor% A bell tolls.            * broadcast to room except %actor%
zoneecho The sun rises.                       * broadcast to every room in self's area
```

### Logging

```
log Greet fired for %actor.name% at %time.hour%   * write to server tracing log
mlog state=%zn118_state% gold=%actor.gold%
```

Lines land in the server's `tracing` output at INFO level, tagged with the trigger's `self` name. Players never see them. Use for builder-side debug breadcrumbs — variable values, branch hits, anything you'd otherwise reach for `%send%` to debug. The `m`/`o`/`w` prefixes are aliases.

### Damage / heal

```
%damage% %actor% 25                           * 25 hp damage
%damage% %victim% -10                         * negative = heal
mdamage %actor% %actor.level%
```

### Teleport / move

```
%teleport% %actor% 3001                       * by vnum or UUID
mteleport all 3001                            * everyone in self's room
```

### Spawn / purge

```
%load% mob 1234                               * spawn mob vnum 1234 in self's room
%load% obj 82 %actor%                         * give item to a player
mload o 82                                    * obj alias
%purge% %victim%                              * remove mob or item
mpurge                                        * (no arg) self-purge
```

### Spells / effects

```
dg_cast 'fireball' %actor%                    * damage spells use a curated table
dg_cast 'cure_blind' %actor%                  * cure_*/remove_* strip the matching buff
dg_cast 'bless' %actor%                       * unmodeled spells become a generic buff
dg_affect %actor% poison 1 60                 * apply poison, magnitude 1, 60 sec
morality %actor% -10                            * shift player morality by delta (clamped -200..200)
morality %actor% 25                             * positive deltas push toward Good
```

Use `morality` (or its alias `dg_morality`) from kill/quest/dialogue triggers to react to player choices. Mob targets are silently ignored. Pair with `%actor.morality_tier%` to branch on the resulting tier rather than raw numbers.

Damage table covers `fireball`, `magic_missile`, `lightning_bolt`, `harm`, `cause_*`, `chill_touch`, `colour_spray`, `dispel_evil/good`, `energy_drain`, `shocking_grasp`, etc. Heal table covers `heal`, `cure_critic/serious/light`. Removal table covers `cure_blind`/`cure_poison`/`remove_curse`/`remove_sleep`. Anything else falls through to `apply_dg_effect`, which silently no-ops on unknown effect names.

`EffectType` aliases recognised: `armor` → ArmorClassBoost, `refresh` → StaminaRestore, `true_seeing` / `sense_life` → DetectInvisible, `stone_skin` / `protection_from_*` → DamageReduction, `infravision` → NightVision, `bless`, `silence`, `haste`, `slow`, `regeneration`, `sanctuary`, `poison`, `curse`, `blind`, `sleep`, `invisibility`, `detect_invisible`, `detect_magic`, etc.

### Necromancy: `raise_dead`

```
raise_dead %arg%                              * raise the corpse named in %arg%, default costs
raise_dead %arg% 70 3 25                      * raise <corpse> mana_cost=70 morality_cost=3 mastery_xp=25
```

`raise_dead <corpse_keyword> [mana_cost] [morality_cost] [mastery_xp]` raises a **charmed undead minion** from a non-player corpse in `%actor%`'s room and binds it permanently to the actor (released on the actor's death/quit, like any charm). It acts on `%actor%`, so the idiom is to fire it from an artifact's `on_command` trigger — see the worked example below. Defaults when args are omitted: `mana=70 morality=3 mastery_xp=25`.

Rules enforced by the capability (no scripting needed):

- **Non-player corpses only**, and only if the corpse carries a source prototype vnum (set automatically when a mob dies).
- **Level cap:** the corpse's creature level must be ≤ the actor's `necromancy` skill level.
- **Success roll** scales with the actor's `magic` skill: `clamp(40 + magic*4 − corpse_level*3, 5, 95)%`. Works at magic 0 (low odds) — this is an *artifact* ability, not a class spell, so non-mages can use it.
- **Minion cap** scales with mastery: `1 + necromancy/3` simultaneous bound dead.
- On **success**: the corpse is consumed, a 60%-HP "risen corpse of …" appears charmed to the actor, `mana_cost` mana is spent, morality drops by `morality_cost`, and `necromancy` gains `mastery_xp` (raising the cap over time). A **botched** roll still spends mana but raises nothing.

This is the *capability* layer in core (`src/necromancy.rs`); the same mechanic is exposed to Rhai as `raise_dead_from_corpse(name, keyword, mana, morality, xp)`. Per-artifact config — which item grants it and the spoken word — lives entirely on the item's trigger (below), so no vnum is hardcoded in core.

### Force / order

```
mforce %actor% pray
oforce %actor% drop %self.id%
force all flee
```

Injects the cmdline into the target's input queue. Mob targets are silent no-ops (no exposed mob command engine for arbitrary verbs — but see [mob world commands](#mob-world-commands) below for the verbs mobs *can* issue from a trigger directly).

### Doors

```
mdoor %self.room.vnum% north flags lock      * lock the north door
mdoor 3001 east flags open                   * open
mdoor 3001 east flags pickproof
mdoor 3001 east flags purge                  * remove the door
mdoor 3001 east description "iron-banded"
```

Field is one of `purge`, `description`, `flags`. Flags accept `open / closed / lock / unlock / pickproof / nopickproof / normal`.

### Mob memory + pursuit

```
mremember %actor%       * mob remembers this PC for 1 hour (default)
mforget %actor%
mhunt %actor%           * set pursuit target
mhunt                   * clear
```

### at / context-shifting

```
mat 3001 mecho A door slams in the inn.
oat %actor.room% %send% %actor% Foot
```

Re-runs the rest as a DG line with `self_room` rebound to the named room. The mob isn't physically moved (parity with tbamud).

### attach / detach

```
attach 5201 %self.id%       * attach trigger prototype 5201 to self
detach 5201 %actor.id%      * remove trigger named '5201' from actor
```

Trigger prototypes are imported from `.trg` files and stored in the `dg_trigger_protos` sled tree. List them with `medit ... trigger dg protos`. See [Prototypes](#prototypes) below for the conceptual model — when to promote, when to detach, and how the refresh sweep interacts with edits.

### Timer / transform

```
otimer 30                   * decay this item in 30 ticks (stored on self.dg_vars["timer"])
transform 1234              * replace self's appearance with prototype 1234's name/desc/flags
```

### Achievements

```
award_achievement %actor% first_blood        * grant a Manual-criterion achievement
award_achievement Galen quest_complete       * by name also works
```

`award_achievement <player> <key>` grants the named achievement to the player. IronMUD-specific — no stock tbamud equivalent. Silently no-ops when:

- the player token can't be resolved (UUID, name, or `actor`/`victim`);
- the key isn't a registered achievement;
- the achievement's criterion isn't `Manual` (engine-criterion keys like kill counts are rejected — those unlock through their own listeners);
- the player already has it;
- the achievement system is disabled.

Use this for narrative milestones the engine can't detect on its own — finishing a story beat, witnessing a scripted event, surviving a one-off encounter.

### Custom skills

```
%actor.skill(dancing_queen)%               * read effective value (base + buffs), as a string
skill_set %actor% dancing_queen 0          * set base
skill_add %actor% dancing_queen 1          * relative add (saturating)
```

Custom skills are builder-published integer attributes — see [custom-skills.md](custom-skills.md) for the full registry + item-affect flow. The key must exist (`lookup skill publish ...`) before any of these will write; unknown keys log a builder warning and no-op.

Read accessor returns `"0"` for absent keys or unresolvable actors, so `if %actor.skill(dancing_queen)% > 5` composes naturally without an existence guard.

### Mob world-commands

When `self` is a **mob**, these verbs work directly without `force`:

| Group | Verbs |
|---|---|
| Speech | `say`, `tell <player> <msg>`, `emote`, `gemote`, `pemote`, `shout` |
| Manipulation | `give <item> <player>`, `drop <item>`, `get <item>`, `take`, `junk`, `extract` |
| Combat | `kill / hit / attack / mkill <target>`, `flee`, accepted-but-flavor `rescue` / `disarm` / `bash` / `passdown` |
| Doors | `open` / `close` / `lock` / `unlock <dir>` |
| Equipment | `wear`, `wield`, `hold`, `remove`, `quaff <item>` |
| Posture | `stand`, `sit`, `rest`, `sleep`, `wake` (broadcast-only — no posture state on mobs) |
| Movement | `goto <room>`, directional `north`/`south`/`east`/`west`/`up`/`down`, `asound <msg>` (broadcast to one-step neighbours) |
| Grouping | `follow / fol / mfollow <target>`, `assist <target>` |
| Shop flavor | `list`, `value <item>` (real, when self has `shopkeeper` flag) |
| Consumables | `light <item>`, `eat <item>`, `drink <item>`, `use <item>` |
| Sub-dispatch | `order <charmed_mob> <command>` — re-dispatches as the charmed mob |
| Info | `consider`, `look` |
| Socials | `smile`, `nod`, `bow`, `grin`, `wave`, `cry`, `laugh`, `wink`, `frown`, `shake`, `clap`, `dance`, `sigh`, `poke`, `hug`, `chuckle`, `yawn`, `whisper`, `sing`, `kiss`, `peer`, `glare`, `slap`, `growl`, `cackle`, `pet`, `caress`, ~30 others — see `src/script/dg/mob_cmd.rs::known_verbs()` |
| Silent stubs | `sell`, `buy`, `time`, `date`, `oset`, `adjust`, `pat`, `snd` |

## Prototypes

A DG **trigger prototype** is a reusable template stored in the `dg_trigger_protos` sled tree by vnum. Distinct from a per-entity trigger:

- A **proto** is the source-of-truth template (body, name, flag letters, attach kind).
- An **attached instance** is a derived copy living on a specific mob/item/room's `triggers` list, carrying `source_proto_vnum` as a backreference to its parent.

### Why use prototypes

Use a proto when **two or more entities should share behavior**:

- Armor-set bonus: same trigger on every set piece (`set_glove_check` script on left + right glove).
- Mob behavior pack: same `OnGreet` on every village guard.
- Room ambience pack: same `Periodic` on every forest room in an area.

Single source of truth: edit once, every attached instance updates. Versus host-local triggers, which diverge over time as builders edit copies individually.

### Authoring flow

```
oedit 3010 trigger dg proto new 8100 cw set_glove_check   * create empty proto, opens editor
                                                          * cw = OnCommand+OnWear (item kind)
oedit 3010 trigger dg attach 8100                         * attach proto 8100 to item 3010
oedit 3011 trigger dg attach 8100                         * also attach to item 3011
oedit 3010 trigger dg edit 0                              * edit attached instance — saves to proto,
                                                          *   refreshes all attached siblings
```

Other entry points:

- `trigger dg makeproto <idx> <vnum>` — promote an existing host-local trigger to a proto so siblings can attach.
- `trigger dg detach <idx>` — break the proto link on a single instance (allows per-instance divergence; instance body unchanged).
- `trigger dg proto view <vnum>` — show proto metadata + body + attached-instance count.
- `trigger dg proto delete <vnum>` — (admin only) remove proto from registry. Attached instances are **orphaned** (source_proto_vnum cleared, bodies preserved). Behavior unchanged in-game until a builder edits the instance.

### Edit-through semantics

Editing any attached instance via `trigger dg edit <idx>` saves to the proto and runs a refresh sweep across all siblings. Builder sees:

```
Proto 'set_glove_check' saved (3 attached instances refreshed).
  warning: trigger uses unknown command 'foo'
```

No silent divergence: stealth differences between proto and instances are the footgun this design eliminates. If you want a one-off variant, run `trigger dg detach <idx>` first.

### Refresh sweep

- Sweep runs only on proto save (not on every fire).
- Single O(entities of matching kind) pass — typically tens of ms for ~10k entities.
- Rebuilds attached triggers totally from current proto state: body, flag-derived trigger types, name, chance, and arglist. Flag changes are structural (add/remove trigger types) and re-derive on the sweep.

### Parse-error abort

Save runs the DG analyzer first. If the body has any `ParseError`, save is refused and attached instances stay on the previous body:

```
Proto save refused: parse error at line 7: unexpected 'end'
(0 instances changed)
```

Non-fatal issues (unknown commands, unknown variables, etc.) come back as warnings, but the save proceeds — matches existing import warning semantics.

### Worked example: matching gloves armor set

Two glove items at vnum 3010, both with `wear_locations: [LeftHand, RightHand]`. Set bonus: +2 STR when both are worn.

```
oedit 3010 trigger dg proto new 8100 cw set_glove_check
```

In the editor:

```
* fires on OnCommand+OnWear; only the satisfying wear triggers application.
if %actor.equipped(3010)% >= 2
  dg_affect %actor% strength 2 -1
else
  dg_affect %actor% strength 0 0
end
```

Then on each glove:

```
oedit 3010 trigger dg attach 8100
oedit 3011 trigger dg attach 8100
```

In-game:

- Wear left glove → `OnWear` fires, `equipped(3010) == 1`, no buff.
- Wear right glove → `OnWear` fires, `equipped(3010) == 2`, STR +2 applied.
- Remove either → `OnRemove` fires, count drops below 2, buff stripped.

For "is the wielded weapon a sword?" style checks, use slot-aware `eq`:

```
if %actor.eq(wielded)% == longsword
  ...
end
```

### Importer interaction

The `ironmud-import tba` importer seeds every parsed `.trg` record into `dg_trigger_protos`, regardless of whether it was attached in the source zone. Attached instances get `source_proto_vnum` set automatically, so importing the same bundle twice is idempotent (re-attach finds the existing proto). Deleting an imported proto **does not** delete its imported instances — they orphan cleanly and continue firing.

## IronMUD-specific notes

Things that differ from stock tbamud:

- **Weather is area-aware.** `%weather.*%` projects through `AreaData.climate`. Set with `aedit climate <preset>` (Temperate / Tropical / Arid / Tundra / Subarctic).
- **Morality slider.** `%actor.morality%` (alias `%actor.align%`) is an integer in `[-200, 200]`, with tier thresholds at `+/-100`. Use `%actor.morality_tier%` for the named tier (`evil_pure`/`evil_3`/…/`good_pure`/`neutral`). Adjust from a trigger with `morality <target> <delta>` — clamps to the legal range, ignores mob targets (mobs carry no morality field).
- **No XP / progression mutators.** `%actor.exp(N)%` is a silent no-op pending PC progression.
- **No equipment slots on mobs.** `wear`/`wield`/`hold` move items from inventory → equipped without slot semantics. `remove` reverses.
- **No posture state.** `stand`/`sit`/`rest`/`sleep`/`wake` broadcast flavor only.
- **No item value slots.** `%self.val0..val3%` always return `"0"`.
- **PC ids are names.** `%actor.id%` returns the player's name (PCs have no UUID); mob ids are real UUIDs.
- **`wait` forfeits cancellation.** Use `return 0` *before* any `wait` if you need to block the host action.
- **`fly` and `waterwalk`** are dg_cast-allowed but the underlying movement effects aren't wired. Buffs land but don't gate flying-required exits or water rooms (yet).
- **`award_achievement`** is an IronMUD-specific command (see [Achievements](#achievements)). Only `Manual`-criterion keys are accepted; engine-criterion achievements (kills, gold thresholds, etc.) unlock through their own listeners.

## Importing tbamud `.trg` files

`ironmud-import tba --source <tbamud-tree>` parses every `.trg` and `.zon` file:

- Triggers attached via zone `T` lines land on the corresponding mob/item/room with full bodies preserved (`dg_body: Some(...)`).
- Unattached triggers register as **prototypes** in `dg_trigger_protos`, attachable later via `attach <vnum>`.
- The static analyzer scans each body and emits one Info warning per trigger that uses unsupported features. Roughly 94% of stock tbamud triggers (1029 → 62) parse and run; the residue is mostly typos in stock content, malformed switch/if blocks, and zone-specific custom verbs (DBZ-zone `kamehameha` etc.).

To see the analyzer's view of what's supported, the source of truth is `src/script/dg/analyze.rs`.

## Worked examples

### Greet trigger with cancel

```
* on_command 'enter'
if %cmd% != enter
  return 1
end
if %actor.level% < 10
  %send% %actor% The temple door does not budge for the unworthy.
  return 0
end
%send% %actor% The temple door swings open.
return 1
```

### Artifact that grants a spoken word (Book of the Dead)

The reusable "hold an item, speak a word, trigger an effect" pattern: put an `on_command`
trigger on the item. It only fires while the item is **carried**, so possession is enforced
for free — no vnum check needed. Here the word `raise` invokes the necromancy capability.

```
* on_command 'raise'  (on the Book of the Dead item)
if %cmd% /= raise
  raise_dead %arg% 70 3 25
  return 0
end
return 1
```

Any artifact can reuse this shape with its own word and its own capability verb. Pair with an
`on_examine` trigger for lore ("Hold it and speak the word to raise the dead — but the act
stains the soul.").

### Combat heal pulse

```
* on_hit_percent 30 (mob's HP < 30%)
if %self.varexists(panic_used)%
  return 1
end
set panic_used 1
global panic_used
%echo% %self.name% glows with healing light!
dg_cast 'heal' %self%
return 1
```

### Quest acceptance

```
* on_say
if %arg.contains(quest)% == 0
  return 1
end
remote zn118_state %actor.id% 1
say I knew you would come, %actor.name%. Find the relic and return.
```

### Periodic ambient with weather gate

```
* periodic, interval 60s
switch %weather.sky%
  case rain
    %echo% Rain drums on the canvas tents.
    break
  case snow
    %echo% Snow drifts pile against the lean-tos.
    break
  default
    if %sunlight% == 1
      %echo% A breeze stirs the prayer flags.
    else
      %echo% Embers glow in a dozen banked fires.
    end
    break
done
```

### Tax collector (mutating accessor)

```
* on_greet
if %actor.gold% < 5
  say Move along, %actor.name%, you've nothing for the tithe.
  return 1
end
say The temple thanks you for your tithe of 5 gold.
nop %actor.gold(-5)%
nop %self.gold(5)%
```

## Troubleshooting

**The trigger doesn't fire.** Check `medit ... trigger dg list` shows it `[ON]`. Check `chance` (default 100). For `on_idle` / `periodic`, check `interval_secs`. For `on_command`, check the keyword arg matches what the player typed (case-insensitive prefix or exact).

**A variable resolves to nothing.** `%head.field%` returns empty when:
- the head isn't bound (e.g. `%victim%` outside combat triggers);
- the field is unknown — IronMUD silently falls through to `entity.dg_vars[field]`, which is empty for unset vars.

**A command does nothing.** Unknown commands silently no-op. Mob world-commands (`say`, `kill`, etc.) only work when `self` is a mob — they no-op in obj/room context.

**Body doesn't save in the editor.** Bodies cap at 32 KB. Use `@` on a line by itself to save, `~` to cancel. The end-marker has to be alone on the line.

**Imported trigger emits an analyzer warning.** That's informational — the trigger still runs. The warning lists which features the runtime will silently no-op on, so you know what to rewrite if behavior matters.

## Related documentation

- [Triggers](triggers.md) — Rhai triggers and templates (the other authoring path)
- [Mobiles](mobiles.md) — `medit`, simulation, social
- [Areas](areas.md) — `aedit`, climate presets that drive `%weather%`
- [Import Guide](../import-guide.md) — how `.trg` / `.zon` parsing works

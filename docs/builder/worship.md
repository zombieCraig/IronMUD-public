# God Worship

Gods are real mobiles marked with a deity config. Players swear a lifelong
pact to one god, earn blessings by praying in temples, and owe recurring
tribute — miss it and the god's anger escalates from warnings to smites.

## Quick start: a zero-script god

```
medit <id> deity setup god
medit <id> deity epithet God of Wrath
medit <id> deity lore Aurex demands strength and repays it in kind. ...
medit <id> deity tribute interval 3
medit <id> deity tribute gold 5
medit <id> deity bless add strength_boost 2
medit <id> flag no_attack on
redit flag temple            (in the temple room)
```

That's a fully working god: worshipers pay 5% of their total gold every
3 game days via `pray tribute`, receive a +2 Strength blessing at prayer,
and suffer the default anger ladder if they lapse. DG scripts are only
needed for custom behavior.

## Ranks

| Rank | Worshipable | Purpose |
|------|-------------|---------|
| `god` | Yes | Top of a pantheon; accepts pacts |
| `demigod` | No | Lesser pantheon figures for quests/fiction |
| `ascended` | No | Same, lowest tier |

Only rank `god` accepts worship. Use `medit <id> patron <god_vnum>` to link
demigods, priests, and minions to their god — those links drive kill
favor/punishment (below).

## The pact

Players use `worship <god>` with the god in the room, or by naming the god
in a temple room. Gate it with either:

- `medit <id> deity pact item add <item_vnum>` — the artifact is **consumed**
  when the pact forms (any one listed vnum works)
- `medit <id> deity pact quest add <quest_vnum>` — any one completed quest

No gates configured = anyone can swear. There is **no un-worship command**;
only `admin religion <player> clear` removes a pact.

## Tribute and the anger ladder

Tribute deadline = `tribute interval` game days (1 game day = 48 real
minutes). `pray tribute` in a temple pays `tribute gold` percent of total
gold (on-hand drained first, then bank) and re-arms the ladder. Days
overdue drive escalation, each stage firing once:

| Overdue | Stage | Default effect |
|---------|-------|----------------|
| 1+ | 1 | Warning message |
| 3+ | 2 | Blessings stripped + Curse |
| 6+ | 3 | Smite: HP damage, timed Blind, heavy Curse |
| 10+ | 4 | Permanent Blind — only if `deity permanent_smite on` (else repeats stage 3) |

`pray atone` surrenders 50% of total gold, lifts all wrath afflictions
(including permanent ones), and resets offense counters. Requires at
least 100 total gold.

Blessing buffs last one tribute interval and carry source
`worship:<god_vnum>`; punishments carry `wrath:<god_vnum>`.

## Enemy gods and faith offenses

- `medit <id> deity enemy add <god_vnum>` — killing NPCs whose
  `patron` is an enemy god earns the worshiper +5 favor; PvP kills of
  enemy-god worshipers earn +25 (max once per victim per game day).
- Attacking a mob patroned to your **own** god, or a co-worshiper player,
  escalates: Curse, deeper Curse, then a full smite on every offense after.
  The pact never breaks.

Favor is a currency for DG scripts to spend; nothing consumes it by default.

### Standing tiers

Players never see the raw number on its own — `worship` and `examine` show a
named tier, with the score in parentheses for anyone optimising:

| Favor | Tier | |
|-------|------|--|
| `<= -100` | Anathema | No offering will be accepted. |
| `-99 … -25` | Disfavored | Regard has soured. |
| `-24 … 24` | Unproven | The starting band. |
| `25 … 99` | Noticed | |
| `100 … 249` | Favored | |
| `250 … 499` | Blessed | |
| `>= 500` | Exalted | |

The bands are asymmetric because favor starts at 0 and normally only rises;
it goes negative only through deliberate faith offences, so the negative side
is short and steep.

Crossing a boundary — in either direction, from any source including
`worship_favor` — announces itself to the player. Moves inside a band are
silent, so a run of +5 minion kills does not narrate every kill.

Favor is **not** clamped. Morality is a bounded slider by design; favor is an
earned currency with no ceiling in the fiction, and a clamp would silently
rewrite whatever existing worlds have accumulated. The ladder simply tops out
at Exalted.

## DG scripting

Three mobile trigger types fire **on the god mob** (live instance if one is
spawned, else the prototype — prototype-fired bodies have no room, so use
`%send% %actor%` rather than `%echo%`):

| Trigger | Fires when | Context vars | Return 0 cancels |
|---------|-----------|--------------|------------------|
| `on_pray` | Worshiper prays in a temple | `action` (`pray`/`tribute`), `overdue_days`, `god_vnum` | Default blessing/tribute handling |
| `on_worship_pact` | Player swears the pact | `god_vnum` | — |
| `on_smite` | Anger ladder stage 3/4 | `severity`, `overdue_days` | Default smite |

DG commands (capability bridge — these are how custom tribute works):

```
worship_tribute %actor%          * mark tribute paid (blood, sacrifice, task...)
worship_bless %actor%            * stamp the god's blessing buffs
worship_favor %actor% 10         * adjust favor (+/-)
worship_smite %actor% 3          * fire the punishment ladder (severity 1-4)
```

DG variables: `%actor.worship_god%` (vnum, empty if godless),
`%actor.worship_favor%` (raw integer), `%actor.worship_favor_tier%`
(`anathema`/`disfavored`/`unproven`/`noticed`/`favored`/`blessed`/`exalted` —
prefer this over numeric comparisons), `%actor.worship_offenses%`, `%self.patron_god%`,
`%self.is_deity%`, `%self.deity_epithet%`.

Example — a blood god that takes HP instead of gold:

```
* attach to the god, type on_pray
if %action% == tribute
  %send% %actor% Aurex takes payment in the only coin that matters.
  %damage% %actor% 20
  worship_tribute %actor%
  worship_bless %actor%
  return 0
end
```

## Lore surfacing

`deity epithet` ("God of Wrath") and `deity lore` (story paragraph) appear
on examine, in worship/pray messages, and in `worship` status — write them
once, they surface everywhere.

## API / MCP

`create_mobile`/`update_mobile` accept `deity` (same fields as medit),
`remove_deity`, and `patron_god_vnum`. Rooms accept the `temple` flag.
`add_mobile_trigger` accepts `pray`, `worship_pact`, `smite`.

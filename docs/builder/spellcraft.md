# Divine Spellcraft

Spells that belong to a god live in the **codex**, a database table separate from the preset spell
files. Players will one day seal their own through a god's ritual (under construction, behind
`spellcraft.enabled`). Today the codex holds **Ancient** spells: world history a builder or admin
seeds, whose scrolls can be placed as loot.

## What a codex spell is

An ordinary spell definition (same fields as `scripts/data/spells_fantasy.json`) plus a history:

| Field | Meaning |
|-------|---------|
| `god_vnum` | The deity mob the spell belongs to. If this world has no such god the spell reads as belonging to "a forgotten god". |
| `status` | `canon` (taught by the faithful), `faded` (still known and scribed, no longer taught) or `lost` (only rumoured; hidden from `spells codex` and `spells info`) |
| `author` | Who set it down. Free text for Ancient spells. |
| `flavor` | Optional cast lines: `caster`, `room`, `target` |

Rules the loader enforces:

- Ids start with `cx_`, so they never collide with preset spells.
- `spell_type` is `damage`, `debuff`, `buff` or `heal`.
- Codex spells are always **learn-only** (`scroll_only` is forced on): a player must read a scroll
  or be taught. Unlearned codex spells do not appear in a player's `spells` list.
- The name must not equal, or be a prefix either way of, any existing spell's name or id (`cast`
  matches spells by prefix). Letters, single spaces and apostrophes, 3-32 characters.
- Description and flavour text are stripped of colour codes, MXP-escaped and capped (240 / 160).

## Power tiers

Every damage, debuff, buff and heal spell has a power tier from 1 to 5, shown in `spells codex`
and `spells info`. It measures the effect (damage, healing, how much a buff matters and for how
long, how many it reaches) at a reference caster, ignoring mana and cooldown. Tier 3 is calibrated
to the strongest mid-level built-in (Lightning Bolt); the high-level built-ins are tier 4-5.

## Seeding Ancient spells

Add entries to `scripts/data/spells_ancient.json`. They are inserted at boot (and by
`admin codex seed`) when their id is not already in the codex. Seeding never overwrites, so edit a
live spell with the `admin codex` tools instead.

```json
{
  "_doc": "...",
  "spells": [
    {
      "def": {
        "id": "cx_dawn_hymn",
        "name": "Dawn Hymn",
        "description": "A note of morning light that sears the unholy.",
        "spell_type": "damage",
        "damage_type": "holy",
        "damage_base": 40,
        "damage_per_skill": 5,
        "mana_cost": 55,
        "cooldown_secs": 20,
        "skill_required": 7,
        "target_type": "enemy"
      },
      "meta": {
        "god_vnum": "god:sol",
        "author": "Saint Orin",
        "status": "faded",
        "flavor": { "caster": "You sing the first note of morning." }
      }
    }
  ]
}
```

Then place the scroll: make an item and set `oedit <vnum> teaches_spell cx_dawn_hymn`. A player
who reads it learns the spell, and `spells codex` shows them as knowing it.

## Petitioning a god

Players earn the right to shape a spell late. `pray petition`, in a temple,
shows the checklist; every line must be met:

| Requirement | Default | Source |
|-------------|---------|--------|
| Worship a spell-granting god | | `medit <god> deity spellcraft enable` |
| Magic skill | 10 | `spellcraft.min_magic` |
| Spellcraft skill | 8 | `spellcraft.min_spellcraft` |
| Spells mastered (level 10) | 5 | `spellcraft.min_mastered_spells` |
| Favor | 500 (Exalted) | `spellcraft.min_favor`, or the god's `favor` |
| Capstone quest | none | the god's `unlock` |
| God is calm (no anger, tribute current) | | |
| Canon has room | | the god's `cap`; slots in grace don't free up |
| God's petition cooldown | 7 game days | `spellcraft.god_petition_cooldown_days`, or the god's `cooldown` |
| Your petition cooldown | 30 real days | `spellcraft.char_cooldown_days` |

The **spellcraft** skill rises by mastering spells: each spell's first
mastery (level 10) awards 900 XP, and learning a codex spell 50. Lifetime
masteries are counted in the `spells.mastered` counter (so evolved spells
still count) and appear on the leaderboards.

## Drafting a spell

Once every line of the checklist is met, `pray petition` opens a guided
drafting mode. The god asks, step by step: shape (damage, debuff, buff, heal),
element or effect, reach, strength, duration (buffs and debuffs), then a
name, a one-line description and three optional flavour lines (what the
caster, the target and onlookers see).

- Only answers inside the god's allow-lists are offered, and only strengths
  that land between the canon floor (`spellcraft.min_power_tier`, or the
  god's `tier`) and the ceiling (`spellcraft.max_power`, default 160; the
  strongest built-in scores 150). A step with a single possible answer fills
  itself.
- Buffs may carry: haste, the six stat boosts, armor class, damage
  reduction/resistance, status resistance, regeneration, hit/damage bonus,
  max HP/mana, luck, bless, courage, invisibility, detect invisible/magic,
  night vision, water breathing. Debuffs: slow, sleep, blind, silence, curse,
  poison. Damage and debuffs reach an enemy; buffs and heals the caster or
  an ally.
- Mana and cooldown are derived from power, never chosen: mana is 0.55 ×
  power (a little dearer than the built-ins); cooldown is power/5 seconds
  for damage and heals, power/2 + 30 for debuffs, power + 60 for buffs.
- Inside the mode every line belongs to the petition: `back` changes the
  last answer, `show` shows the draft, `abort` abandons it, and `offer` (at
  the end) lays it before the god. Being attacked breaks the petition off.

The review shows the **price** and the **components**:

| | |
|-|-|
| Base price | a max-HP pledge of `spellcraft.hp_pledge_pct` (20) % of the power score |
| `flesh` (default) | +50% HP pledge |
| `mind` | a max-mana pledge equal to the HP pledge |
| `essence` | 1 base attribute point, for good |
| `fortune` | power × 25 gold |
| `weariness` | a max-stamina pledge of twice the HP pledge, and slower recovery |
| `fate` | one of the above, revealed only after sealing |
| Favor | the god's `cost` (250) |
| Components | `essence:<element>` (damage), `focus:<effect>` or `focus:heal`, ×(tier − floor + 1), quality `component_quality_base` + `_per_tier` × (tier − floor); and one `relic:<god_vnum>` |

Components are ordinary items tagged with those categories (`oedit <id>
category add essence:fire`). They are consumed by the sealing ritual, a later
stage.

## Offering and the trial

On `offer`:

1. The draft and the whole checklist are checked again (in a temple).
2. The god's `on_pray` triggers fire with `action` = `petition` and the
   context vars `spell_name`, `spell_type`, `power`, `tier`. A DG body that
   returns 0 refuses the petition; the player can change the draft.
3. The god must have a trial for the spell (below), or it has "no task for
   you yet".
4. The god's petition cooldown is claimed, and only while its canon has room
   (a free slot, or one past `canon_grace_days`). One god hears one petition
   at a time.
5. The trial quest is offered (with its gates). If the quest refuses, the
   claim is released.
6. The draft is frozen onto the character as their pending petition.

Trials are ordinary quests (make them `repeatable` so later petitioners can
take them too). The god's rules pick one by the spell's nature:

```
medit <god> deity spellcraft trial add rite_of_embers type=damage element=fire
medit <god> deity spellcraft trial add rite_of_ash power=130
medit <god> deity spellcraft trial add rite_of_dawn
medit <god> deity spellcraft trial add rite_of_return restore=yes
medit <god> deity spellcraft trial remove 2
```

A rule with `restore=yes` is only used for restorations (`pray restore`, see
below) and never for new petitions; the other filters apply to the faded
spell's mechanics as usual.

The most specific matching rule wins (`type`, `element`, `effect` each count),
then the highest `power`. A god with no rules refuses every petition. Trial
quests can use any objective, including DG steps: a DG `set` on a player's
variable advances `dg_flag` objectives.

While the trial is open, `pray petition` reports it. If the quest is
abandoned or expires, the petition lapses with it.

## The ritual

Once the trial is done, the author returns to a temple and types `pray ritual`.

- **Claim first.** The components are taken from their pack (for each one, the
  poorest item that is good enough), and a Fortune price's gold is taken too.
  If anything is missing, nothing is taken and nothing starts. A server-wide
  line announces the rite.
- **Stages.** Every `spellcraft.ritual_stage_secs` (120) the rite completes a
  stage and draws mana: half the spell's mana cost (at least 10), from what
  helpers gave first. If the author can't pay, the rite collapses. At the end of
  each stage it pauses, and the author chooses:
  - `pray seal`: inscribe the spell now.
  - `pray push`: the god deepens the gift. Each kept push adds 15% to damage
    or healing (25% to a buff's duration) at the same mana cost, adds half the
    base HP pledge, and adds a risk die. Then the author rolls their risk dice;
    any 1 is a backlash (below). At most `spellcraft.ritual_max_pushes` (3).
  A pause that is left waiting keeps drawing mana each stage.
- **Backlash** (d6): 1-2 the last push's gain is lost; 3-4 holy fire burns for a
  fifth of max HP (never below 1); 5 an extra mark at sealing; 6 collapse.
- **Disturbances.** Talking, looking at yourself and `pray` are free. Any other
  command is a *minor* disturbance, as is another player speaking, emoting or
  using a social in the room, or anyone walking in or out. Minors add at most +1
  risk per stage, paid when the stage ends. *Major* disturbances (moving,
  fleeing, recalling, attacking, sleeping, being drawn into a fight or put to
  sleep) add +2 risk and roll the backlash table at once.
- **Collapse.** Leaving the room, dying, or being away longer than
  `spellcraft.ritual_linkdead_secs` (300) also collapses the rite. The
  components are gone, half the HP pledge (base plus pushes) is taken anyway
  (it grows back like a released pledge), the petition is void, and the god's
  cooldown stays spent.
- **Aid.** Anyone in the room may `pray aid <author>` once per stage. They give
  the stage's mana; a worshipper of the same god gives it all and always cancels
  that stage's minor disturbance, anyone else gives half and cancels it on a
  coin flip. Helpers earn a little spellcraft XP and the `spells.aided` counter;
  same-faith helpers are named as witnesses on the codex entry. Aid never undoes
  a major disturbance, and aiding any god's rite is never an offence.

**Sealing** writes, in order (each step is safe to repeat if the server stops
mid-way): the codex entry (a name already taken gains a numeral, "Sun Lance
II"), the god's canon slot, and then everything on the author in one write: the
pledge (shrunk if it would breach the 60% floor), any Essence point (the highest
attribute, never below 3), the favor cost, the marks, and the spell itself. Fate
picks its style when the rite begins and reveals it at sealing. The author gets
the `spells.authored` counter. Sealing into a full canon displaces a spell
(see Canon life).

**Marks.** One mark from the god's `mark_pool` at sealing, plus one per
backlash 5. A Weariness price adds a 15% `regen_penalty` to the first. A god with
no pool brands "the sigil of <god>" on the brow, with no affects:

```
medit <god> deity spellcraft mark add hand strength_boost:2,dexterity_boost:-1 a sun-brand across the palm
medit <god> deity spellcraft mark remove 1
```

## Canon life

**Devotion.** Each cast of a canon spell by someone who worships its god counts
toward that spell's devotion. The leaderboard tick (every 5 minutes) folds the
casts into the canon: devotion halves every 15 game days without casts, and never
drops below half a point per character who knows the spell. The canon's
influence is the sum.

**Displacement.** When a spell is sealed into a full canon (`canon_cap`), the
least-devoted spell past `spellcraft.canon_grace_days` leaves it. It becomes
`faded`: people who know it keep it, but no one is taught it. Its author's pledge
starts growing back and their marks cool. The author hears this at once, or by
mail from the god if offline. A full canon whose spells are all still within
grace hears no petitions.

**Omens.** Once a canon is full, the author of its least-devoted spell is
warned, once, that the god's attention is drifting.

**Restoration.** `pray restore <spell>` in a temple asks a god to re-light one of
its faded spells. It has the same gates as a petition, and uses the god's
`restore=yes` trial and the spell's components. The pledge is half a
petition's. The rite cannot be pushed, and sealing re-lights the old entry (taking a canon
slot, displacing if full) rather than writing a new one. A restorer who is not
the author becomes the spell's author; the lineage keeps the first author and
`spells info` shows both. The `spells.restored` counter is bumped.

**Full form.** An author casting their own canon spell, in good standing, casts
its true form: 25% less mana, no reagent, and mastery up to 10. Everyone else who
learned a codex spell masters it only to 7. Observers see a codex spell as
"Sun Lance, gift of Sol". The author's cast lines (caster, target, room) are
shown on every cast.

**Temple teaching.** A dialogue effect lets a priest teach canon spells:

```
medit priest tree addfx greeting 0 teach_canon_spell - 100 25
medit priest tree addfx greeting 1 teach_canon_spell god:sol 0 50 Sun Lance
```

The arguments are `<god_vnum|-> [gold] [favor] [spell name]`. With `-`, the god
is the speaking mob if it is a god, otherwise the player's own god. With no
spell named, the priest teaches the oldest canon spell the player lacks. The
player must worship that god at Favored or better and meet the spell's magic
level. Faded spells are never taught.

## Scrolls, fame and the Lost scan

**Scribing.** `craft scroll <spell>` writes a codex spell the player knows onto a
**blank scroll**: any item with the category `blank_scroll` (sell them at a scribe).
The blank is inscribed in place: it gains `teaches_spell`, the short description "a
scroll inked with *Sun Lance*", the category `scroll:inscribed`, and loses
`blank_scroll`. No per-spell prototype is needed. It needs spellcraft
`spellcraft.scribe_min_spellcraft` (3) and costs the spell's mana. For
`spellcraft.scribe_monopoly_days` (14 game days) after a spell is sealed, only its
author may scribe it. Vetoed spells can't be scribed; Lost ones can, by anyone
who still knows them, which is how a spell is rediscovered.

**Fame.** Every cast of a codex spell by anyone other than its author is added to
the spell's `casts_by_others`. At 100, 1,000 and 10,000 the author is told (or
mailed). The leaderboard tick reconciles each character's counters from the codex:
`spells.authored` and `spells.restored` (from the lineage) and `spells.canon_casts`
(casts by others of the spells they now author). `spells.aided` and
`spells.scribed` are counted as they happen. All five have boards under **Faith**,
so achievements on them are pure JSON. Each authored spell adds 5 to Renown.
`spells codex` lists every god's **influence**, the summed devotion of its canon.

**Lost.** Once per game day, a Faded spell that no character knows and no item
(prototype or instance) teaches becomes Lost. Canon spells never go Lost. To keep
an Ancient spell findable, place a scroll prototype for it.

**Auditor.** The world audit warns when a god grants spells but the world can't
support it:
- `world.spellcraft_no_relic`: no item prototype carries `relic:<god vnum>`, which
  every ritual consumes.
- `world.spellcraft_no_component`: an allowed element (`essence:<element>`), effect
  (`focus:<effect>`) or heal (`focus:heal`) has no prototype at quality
  `spellcraft.component_quality_base` or better.
- `world.spellcraft_missing_quest`: `unlock_quest_id` or a trial rule names a quest
  that doesn't exist.

These are bounty-board material like any other world finding.

**API.** `GET /api/v1/codex`, `GET /api/v1/codex/:id` and `GET /api/v1/canons/:god`
are read-only (MCP `list_codex`, `get_codex_spell`, `get_canon`). Admin keys can
`PUT /codex/:id/status` and `PUT /codex/:id/veto` (`set_codex_status`,
`set_codex_veto`). There is no API to create a spell.

## Heresy and apostasy

Everything here is off while `spellcraft.enabled` is off. A spell whose god
does not exist on this world is godless and never heresy.

**Heresy.** Casting or learning a codex spell of a god you do not worship is
heresy. The tier depends on the gods: godless 1, neutral 2, enemies 3 (either
god lists the other as an enemy). It is one higher if you once renounced the
spell's god, so an apostate casting their own spell is at least tier 2.

- Each cast always costs a backlash of 5% of the spell's power per tier in HP.
  It never takes you below 1 HP. The spell still lands.
- Both gods' grudges grow by one: the spell's god's and, if you have one,
  your own god's.
- Your own god then rolls `10/20/35/50% by tier + 5% per grudge` (at most 95%).
  On a hit it punishes the heresy: favor falls by `10 + 5 × grudge`, then
  Curse, deeper Curse, and a full smite from the third offence on. The pact
  never breaks.
- The spell's god rolls the same chance. On a hit it smites at the tier's
  severity (at most 3), pact or no pact.

Learning a foreign spell from a scroll warns first; `read <scroll> confirm`
within a minute learns it, and that counts as one offence (with no backlash).

**Grudges** sit outside the pact, so changing faith never clears them, and they
never decay. `pray atone` clears only your own god's grudge. Admins can list or
lift one: `admin religion <player> grudge [clear <god_vnum>]`. `worship` shows a
player the gods who remember their offences.

**Apostasy.** `worship renounce` in a temple warns. Then `worship renounce
confirm` within a minute does the following:

- records a former faith and adds 5 to the god's grudge;
- the god smites at severity 2;
- every Active or Regrowing pledge to it is forfeit for good;
- every mark from it burns again with an extra Curse;
- the codex spells you authored for it stay canon but name you Faithless and
  lose the full form;
- an open petition to it is dropped.

A renunciation can't happen mid-rite. No new pact is possible for
`spellcraft.repact_cooldown_days` (7) game days. `admin religion <player>
clear` records a former faith without any of this.

## Pledges and marks

The price a god asks is paid with two primitives.

- **Pledge.** Part of the caster's maximum HP, stamina or mana, held by the god while the
  spell stands. The ceilings drop at once and current values are clamped under them. No
  ceiling ever drops below `spellcraft.pledge_floor_pct` (60%) of what it would be without
  pledges, and a pledge that would breach that floor is refused. Replicants pay stamina in
  Resolve, since their stamina never runs down. A **released** pledge grows back over
  `spellcraft.regrow_days` (14 game days); a **forfeit** one is taken for good. `status`
  lists every open pledge and the lowered ceilings.
- **Mark.** A god's brand, shown on `examine` like a tattoo and listed under `status detail`.
  Its affects are stamped as permanent buffs (source `mark:<id>`), for example
  `regen_penalty`. A mark can **fade** (still shown, "long cooled", stamps nothing) or
  **worsen** (gains a second, negative affect).

## Admin tools

`admin codex list | show <id> | seed | status <id> canon|faded|lost | veto <id> [off] | rename <id> <name>`.
A veto fades the spell and hides it from players; lifting it leaves the spell faded.

`admin pledge <player> list | add [hp=N] [stamina=N] [mana=N] [god=<vnum>] [spell=<id>] | release <id> | forfeit <id>`
and `admin mark <player> list | add <god vnum> <body part> <effect[:mag]> <short desc> | fade <id> | worsen <id> <effect[:mag]> | remove <id>`
drive the price primitives directly, for testing and support.

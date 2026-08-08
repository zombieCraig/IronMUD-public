# Factions and Reputation

A faction is a named group the world holds an opinion on behalf of. Tag a
mobile with one and three things start happening: its members defend each
other, killing one costs the player standing with the group and buys standing
with the group's enemies, and a group that has come to hate a player attacks
them on sight. Merchants belonging to a faction price by standing.

Players check where they stand with `standing`.

## Quick start: a faction in two steps

```
medit <id> faction town_watch
```

That is already enough. An undeclared tag works — killing those mobiles moves
the player's standing under the key `town_watch`, and its members defend one
another. What it does not have is a display name, any enemies, or its own
thresholds.

To give it those, add an entry to `scripts/data/factions.json`:

```json
{
  "key": "town_watch",
  "name": "The Town Watch",
  "description": "Underpaid, over-extended, and the only thing standing between the settled roads and everything that walks them.",
  "opposed": ["bandits", "undead"],
  "opposition_ratio": 50,
  "hostile_at": -200,
  "price_swing": 20
}
```

The file is read once at boot. It ships with six example factions —
`town_watch`, `bandits`, `merchants`, `camarilla`, `vampire_hunters`,
`undead` — which you can keep, edit or delete. The first five are the tags the
shipped mobile presets already use, so they work out of the box; a tag nothing
declares still works, it just has no opposites or display name.

An `opposed` entry naming a faction nothing declares is logged as a warning at
boot. It is not fatal — standing still moves under that exact key — but it is
almost always a typo, and the symptom is obscure: the player picks up standing
with a faction that has no mobiles and then sees it on `standing` and on the
leaderboards.

## Fields

| Field | Default | Meaning |
|---|---|---|
| `key` | required | Lowercase identifier. Must match what you put in `medit <id> faction`. |
| `name` | the key | Player-facing name. Used as the subject of standing announcements: *"The Town Guard now count you Accepted."* |
| `description` | empty | One line of lore, shown by `standing <faction>`. |
| `opposed` | `[]` | Keys of factions this one is at odds with. |
| `opposition_ratio` | `50` | Percentage of a gain that transfers, negated, to each opposed faction. |
| `hostile_at` | `-200` | Standing at or below which this faction's mobiles attack on sight. |
| `always_hostile` | `false` | Attacks at *any* standing. For a faction there is no point earning standing with at all. |
| `price_swing` | `20` | Widest shop price change, in percent, at the ends of the ladder. `0` opts the faction's merchants out of reputation pricing. |

## The ladder

Standing runs `-1000` to `+1000` and starts at 0 for every faction. A player
who has never dealt with a group is Neutral with it, and the group does not
appear in their `standing` list — that list is a record of what they have
done, not a directory.

| Standing | Band | |
|---|---|---|
| `500` and up | **Revered** | You are one of their own. |
| `200` … `499` | **Honored** | Your name is spoken well of among them. |
| `50` … `199` | **Accepted** | They count you a friend. |
| `-49` … `49` | **Neutral** | No particular opinion. |
| `-199` … `-50` | **Disliked** | They have not forgotten what you have done. |
| `-499` … `-200` | **Hostile** | They consider you an enemy. |
| `-500` and down | **Hated** | They kill on sight and will not trade at any price. |

Crossing a band announces itself; moves inside one are silent. That is the
point of naming bands at all — a `-5` per kill that shouted every time would
turn a fight into a ticker, and a raw number tells a player something changed
without telling them what it bought.

## Why opposition matters

**Combat can only lower standing.** Killing a faction's members costs 5 points
each; nothing you kill raises your standing with that same faction. Standing
rises two ways:

1. **Opposition.** Killing a faction's enemies raises it, at
   `opposition_ratio` percent. With the default 50, twenty dead bandits move
   you `+50` with the Town Guard — into Accepted.
2. **Quests and dialogue.** `quedit <vnum> reward add reputation <faction>
   <delta>` and the `reputation` dialogue effect.

Without opposed factions, reputation is a ratchet every player eventually
maxes out, and a number everyone has is not a standing. With them, the kill
that buys you the Guard's goodwill costs you the Roadmen's, so where a
character stands is a record of what they chose.

Opposition is applied **one hop only** and is **not assumed symmetric**. If
you want a mutual rivalry, list it on both sides. A one-way entry is a
legitimate shape: a faction can resent a rival that does not think about it at
all.

Small transfers that round to zero are skipped rather than written — an
`opposition_ratio` of 5 does nothing to a 5-point kill, which is worth knowing
before you set a low one.

## Aggro

A faction-tagged mobile with **no aggression flag at all** still checks
standing, and attacks any player at or below `hostile_at`. Untagged mobiles
skip the check entirely, which is what keeps it off the hot path.

Set `"always_hostile": true` for a faction that attacks everyone regardless of
standing — the shipped `undead` entry does this, since whatever keeps them
walking is not interested in negotiating. Set `hostile_at` very low (`-900`)
for a group that takes a great deal of provoking.

Do not try to spell always-hostile as a very high `hostile_at`. It looks like
it works, because the comparison is inclusive and the ladder tops out at
`1000` — but it is a coincidence of two constants rather than a statement of
intent, and a reader cannot tell it apart from a number somebody mistyped.

This is separate from the `aggro_good` / `aggro_evil` / `aggro_neutral` flags,
which read the player's **morality** rather than their standing. A mobile can
use both.

## Shop pricing

A shopkeeper tagged with a faction quotes prices adjusted by the buyer's
standing, scaling smoothly from `+price_swing` percent at Hated to
`-price_swing` at Revered. Neutral is exactly the listed rate, so tagging a
shopkeeper never changes prices on its own — only a player who has actually
dealt with the faction sees a different number.

The adjustment applies in both directions: a merchant who likes you charges
you less **and** pays you more. It stacks with the existing charisma
adjustment, and `buy`, `sell`, `list` and `appraise` all read the same
figure, so they cannot quote different numbers.

## Gating content on standing

**Quests.** `quedit <vnum> reputation <faction> <min_value>` blocks the offer
until the player reaches that standing, and hides the "(has a quest for you)"
cue in the meantime. A faction the player has never met reads 0, so a positive
threshold means *prove yourself first* and a negative one means *we will not
deal with someone who has wronged us this badly*.

**Dialogue.** The `reputation_at_least` condition takes the same shape:

```
medit <id> tree addcond greet 0 reputation_at_least town_watch 50
```

## Rewarding standing

```
quedit <vnum> reward add reputation town_watch 100
medit <id> tree addfx greet 0 reputation town_watch 25
```

Both propagate to opposed factions and announce band crossings. Deltas are
clamped to the ladder's bounds.

## Admin

```
admin reputation <player>                            show every standing
admin reputation <player> <faction>                  show one
admin reputation <player> <faction> +50              adjust (opposition applies)
admin reputation <player> <faction> set 250          assign exactly (no opposition)
```

The delta form goes through the same path quests and kill credit use, so
opposed factions move and the player is told. `set` is a correction rather
than an act in the world, and does neither.

## Pacing

Killing a faction member is `-5`. Reaching Revered with their enemies is on
the order of two hundred kills at the default `opposition_ratio`. That is
deliberate: a reputation should read as a campaign rather than an afternoon.
Quest rewards are how you let a story move standing faster than a grind can —
a `+100` at the end of a chapter is worth twenty fights.

## See also

- [Mobiles](mobiles.md) — the `faction` field and alignment
- [Quests](quests.md) — the reputation reward and prereq
- [Dialogue Trees](dialogue-trees.md) — the `reputation` effect and
  `reputation_at_least` condition

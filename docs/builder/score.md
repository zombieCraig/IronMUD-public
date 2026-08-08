# Builder Score

```
build            your sheet: points, output, standing
build who        every builder, best first
top build score  the board
```

Builder boards live in the `Building` group of `top`. They are the only boards
that rank admins — the people who build a world are usually the people who run
it, so excluding admins would leave them permanently empty — and the only ones
hidden from players, who have no way to compete on them.

## How points are earned

**The score is derived from a scan of what currently exists.** It is not a
tally of edits. A background tick every five minutes re-reads the world, grades
everything, and recomputes each builder's total from scratch.

That single decision is what makes the score hard to game:

| | |
|---|---|
| Re-saving the same room | earns nothing — there is no event to repeat |
| Deleting your own work | **lowers** your score |
| Building a hundred empty rooms | scores below five good ones |
| Running the importer | earns nothing at all |

Each entity contributes:

```
points = audit_grade / 100  ×  kind_weight  ×  bonuses  ×  diminishing_returns
```

**`audit_grade`** is the 0–100 score from [`build audit`](audit.md). An A-grade
room is worth ten times an F-grade one. Unfinished content is not worth zero —
you are mid-draft, not idle — but it is small enough that shipping it
deliberately is never the play.

**`kind_weight`** is a claim about cost, and it is the lever that decides what
building-for-score looks like:

| Kind | Weight | Because |
|---|---|---|
| Area | 60 | a decision |
| Quest | 40 | a day |
| Mobile | 20 | an afternoon |
| Item | 12 | minutes |
| Room | 10 | an hour |

An area is graded on its *composite* — its own findings blended with everything
in it — so an empty shell is not worth sixty points.

**Bonuses.** A mobile carrying a branching dialogue tree is worth double. It is
the most under-used system in the engine and by far the most work per mobile,
and it is not its own entity kind, so it rides as a multiplier rather than
being invisible.

**Diminishing returns** apply per *(builder, area, kind)*:
`factor = 40 / (40 + n)`. The first room in an area is worth full weight, the
fortieth half, the hundred-and-twentieth a quarter. Padding is not forbidden —
it is just a bad way to spend an afternoon, and starting a second area always
pays better than adding to a full one.

The worst content in an area takes the diminished slots first, so a builder
cannot pad an area with rubbish and have the padding claim the undiminished
weight.

## What does not count

Only content with `origin = builder` and a named author. That excludes the demo
world, everything an importer produced, and anything unattributed. See
[Attribution](attribution.md).

Bounty points are the one exception to "derived": they are stored on the
character, because a bounty pays for work whose product may be spread across
content the claimant does not own, and it stays paid if someone else later
deletes it.

## Counters and achievements

The scan reconciles seven counters onto each builder's character:

| Counter | Board |
|---|---|
| `build.score` | Builder Points |
| `build.rooms` | Rooms Built |
| `build.items` | Items Built |
| `build.mobiles` | Mobiles Built |
| `build.quests` | Quests Written |
| `build.areas` | Areas Built |
| `build.excellent` | A-Grade Content |

They are **reconciled, not incremented** — set to the scan's answer, so they
fall when content is deleted. Achievements already unlocked are never revoked;
an achievement records that you did the thing, and you did.

Because leaderboards are discovered from character counter keys, every one of
these got a board with no edit to `src/leaderboard.rs`. Achievements against
them are pure JSON in `scripts/data/achievements/builder.json`.

**Builder achievements pay in titles, never trait points.** Trait points are a
player currency that buys character power; paying them for building would let a
builder buy their way up the other ladder.

## The grade toast

When you change something, you get one line — but only if its letter grade
moved:

```
The Iron Gate now grades B (was D).  2 warning(s) left
```

Silence when the letter has not changed is the design. A line after every
setter would be a ticker, and a ticker is noise. This is the same rule
`src/tiers.rs` states for named tiers: a move inside a band says nothing, a
move across one announces itself.

A creation always reports, because "it exists now, and here is what it is" is a
change from nothing.

Implementation: `src/build_score.rs` (the scoring), `src/ticks/build_score.rs`
(the tick), `src/audit/scan.rs` (the toast).

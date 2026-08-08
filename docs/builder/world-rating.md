# World Rating

```
world             how far along the world is, and what is holding it back
world milestones  the wall: what it has crossed, and what is next
```

`build audit world` answers *what is broken*. This answers the other question:
**how far along is this?**

A room count cannot say. Four hundred rooms with no quests, no shops and no
NPCs is not further along than a hundred and twenty that hold together. So the
rating is a weighted composite, reported as a named tier:

```
Wilderness → Outpost → Hamlet → Village → Town → City → Realm → World
```

The naming is the point. "Village" tells an operator where they are; `41` does
not. Same rule `src/tiers.rs` was written for.

## The five components

| Component | Weight | Measures |
|---|---|---|
| Size | 25 | Rooms and areas |
| Density | 20 | Mobiles per room, items per area, spawn points per room |
| Depth | 25 | Quests, dialogue trees, recipes, transports |
| Quality | 20 | Share of all content grading C or better |
| Connectivity | 10 | Areas players can walk between |

Count terms scale as `sqrt(actual / target)`. Linear scaling would leave a new
world reading 3% forever and teach its builders that nothing they do matters;
log scaling would put the demo world past halfway and teach them the opposite.
Square root moves fast enough early that the first week is visible, and slowly
enough late that the last tier is genuinely a lot of world.

Full-marks targets live in `TARGETS` (`src/world_rating.rs`) — 1500 rooms, 20
areas, 60 quests, 40 dialogue trees, and so on. That is the honest place to
argue about scale.

Quality covers **all** content, not just builder-authored content: this
measures the world a player walks through, and a player does not care who wrote
the room.

## Caps

Some absences hold the rating back regardless of everything else:

| Cap | While |
|---|---|
| Hamlet (34) | there is only one area |
| Village (49) | no quests exist |
| Town (64) | no NPC has a dialogue tree |
| City (79) | there is no bulletin board |

The lowest applicable cap wins.

Curves cannot express "a world with no quests is not a Town" — averaging lets a
strong showing everywhere else buy past a hole that ought to be disqualifying.
Caps say it outright, and they double as the clearest next-step message the
rating can produce: not *build more*, but *build this*.

**A cap always outranks the weakest component in the advice line.** While one is
in force, improving anything else moves nothing, and pointing at a term that
cannot help would be actively misleading.

The world the repo ships rates **Village**, held there by having no quests. Its
uncapped score is 55 — a Town on the curves alone, which it plainly is not.

## Milestones

`world milestones` is the wall: seventeen things the world can cross, with
progress toward the ones it has not.

Every threshold sits **above what the demo world already has**, so a fresh
install unlocks nothing. A wall of milestones that were already lit before
anybody logged in is not a record of anything, and it takes the first real one
away from whoever earns it.

When one crosses:

1. it is recorded once, in the `world_milestones` sled tree, with the date;
2. every builder carrying a score at that moment is listed as a contributor;
3. all builders are told, via `broadcast_to_builders` — the only interruption
   in the whole builder tier, and it is allowed because milestones are rare by
   construction;
4. each contributor is awarded the matching achievement through the normal
   manual path, so it shows up under `achievements` with everything else.

A milestone **stays recorded even if the world shrinks back**. The world did
pass a thousand rooms; deleting them afterwards does not un-happen it. The
builder score already handles the "you no longer own this" half.

A milestone crossed with no scoring builder behind it — an imported world does
exactly this — credits nobody rather than failing. It still belongs to the
world; it just has no name on it.

## Adding a milestone

1. A `(key, WorldGoal)` row in `WORLD_GOALS` (`src/world_rating.rs`).
2. An `AchievementDef` with the same key in
   `scripts/data/achievements/world.json`, `criterion: manual`, category
   `builder`.

A test asserts the two lists match exactly — a goal with no definition would
record on the wall and then silently fail to credit anybody.

Goals are named in code rather than in data because each reads a different
field, the same reason `src/leaderboard.rs` names its derived boards in code.
The *presentation* stays in JSON, so what a milestone is called is still a
content decision.

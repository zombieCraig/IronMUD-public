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
3. all builders are told, via `announce_to_builders` — the only interruption
   in the whole builder tier, and it is allowed because milestones are rare by
   construction;
4. each contributor is awarded the matching achievement through the normal
   manual path, so it shows up under `achievements` with everything else.

A milestone **stays recorded even if the world shrinks back**. The world did
pass a thousand rooms; deleting them afterwards does not un-happen it. The
builder score already handles the "you no longer own this" half.

## Pointing this at a world that already exists

Most worlds this runs against were not built under it. Somebody imports a
CircleMUD area set, or upgrades a server that has been accumulating rooms for
years, and on the first boot the world already has 355 rooms and eleven areas.

**The first evaluation against any database adopts that world silently.**
Everything already met is recorded, with no banners, no per-builder awards, and
no contributors — the wall shows those rows as *"already true when this world
was adopted"*, which is the honest description. Every evaluation after that
announces normally.

Two failures this avoids, both of which were shipped at some point:

- Refusing to record until a builder is on the board leaves an imported world
  reporting `0 of 17` beside a line reading `355 / 100`, and it never resolves,
  because imported content credits nobody by design.
- Recording and announcing them all produces a boot-time storm of banners for
  things nobody in the room did, permanently consuming the milestones on the
  way past.

The marker is the `world_milestones_adopted` setting. There is no way to tell
adoption from a crossing by looking at the world alone — a world with 355 rooms
looks the same whether it grew that way while the server was watching or arrived
that way — so it has to be remembered rather than inferred.

The wall reads **met-ness as well as recorded-ness**, so a goal the world has
passed never appears under "Ahead" during the five-minute window before the
survey records it.

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

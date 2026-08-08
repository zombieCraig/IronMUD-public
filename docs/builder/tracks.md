# Progress Tracks

```
build next            the one thing worth doing next
build track           your path
build track <area>    an area's readiness checklist
```

`build audit` says what is wrong with something that exists. A track says what
does not exist yet — the harder half of the blank page.

Tracks render the way a quest log does, on purpose. A builder who plays this
game already knows how to read a checklist with ticks against it and should not
have to learn a second idiom.

## The two shipped tracks

### Area Readiness — *is this area finished?*

Ten steps, each something a player would notice the absence of: twenty rooms,
no exits into the void, every room reachable and described, five mobiles, ten
items, things that actually spawn, one quest, a level range, and eight rooms in
ten grading C or better.

### The Builder's Path — *do you know what this engine can do?*

Nineteen steps, each a system most worlds never touch: seasonal descriptions,
doors, contextual verbs, dialogue trees, daily routines, shops, factions,
alignment, item affects, containers, multi-step quests, forage tables.

This one matters more than it looks. The engine has dialogue trees, DG scripts,
quests, factions, transports, recipes, forage tables, traps, slow exits and
contextual verbs — and the shipped world uses almost none of them. **A builder
cannot learn a system they do not know exists.** The track is a tutorial
disguised as a checklist, and completing it produces content as a side effect.

Every step carries a hint naming the command that does it. A checklist that
names a system without saying how to reach it is a quiz.

## `build next`

One thing, not a wall. A list of everything outstanding is the blank page
again, which is what this is here to solve. It looks in order:

1. **Anything of yours that is outright broken.** A blocker beats a checklist
   step every time — it is already in the world and a player can already walk
   into it.
2. **The area you are standing in**, if its readiness track is unfinished.
3. **A system you have never used**, from the Builder's Path.
4. **Whatever is capping the world rating.**

## Scopes

| Scope | Evaluated against |
|---|---|
| `area` | One area's contents, via its audit report |
| `builder` | Everything *you* have authored, wherever it is |

Builder scope is deliberately not area-scoped: the Path asks whether *you* have
used a system, and using it once anywhere is the answer.

Only `origin = builder` content counts, so seeding or importing a world does not
complete anybody's tutorial. See [Attribution](attribution.md).

## Predicates

| Kind | Asks |
|---|---|
| `count` | At least *n* entities of a kind |
| `grade_ratio` | At least *r* of a kind grade *letter* or better |
| `no_finding` | No entity in scope emits this audit code |
| `has_system` | A named engine system is in use |

`grade_ratio` over nothing is **false**, not vacuously true: "no rooms" is not
"all rooms are good", or an empty area would complete the readiness track.

## Adding a track

Drop a JSON file in `scripts/data/build_tracks/`. That is the whole procedure —
tracks are content.

```json
{
  "key": "my_track",
  "name": "My Track",
  "scope": "area",
  "steps": [
    { "key": "rooms", "label": "Twenty rooms",
      "predicate": { "kind": "count", "of": "room", "min": 20 },
      "hint": "`dig <dir> <title>`." }
  ]
}
```

Adding a *predicate kind* is one match arm in `src/build_tracks.rs`. That is the
line between content and code.

Two tests guard the JSON, and both catch failures nothing else would:

- a `has_system` step naming a system `systems_used` never emits is a box
  nobody can ever tick;
- a `no_finding` step naming a code the auditor never emits is permanently
  satisfied, silently.

Progress is never stored. A track asks a question about the content that exists
right now, and caching the answer would just be a second thing to keep in sync.

# Attribution & Provenance

Every room, item, mobile, area and quest carries three fields:

| Field | Means |
|---|---|
| `authored_by` | The builder who created it. `None` = unclaimed. |
| `last_edited_by` | The builder who last changed it. |
| `origin` | Where it came from: `unknown`, `seed`, `import`, or `builder`. |

`build audit <kind> <vnum>` shows all three on its credit line.

## Why origin exists

Only `builder` content can be credited to a person. The other three are
deliberate exclusions:

- **`seed`** — the demo world that ships with the engine. Real content, nobody's
  achievement.
- **`import`** — produced by `ironmud-import`. One CircleMUD import is 679 items
  and 1,286 spawn points; without this, running the importer would be a cheat
  code for anything that rewards building.
- **`unknown`** — the default, and what every row written before attribution
  existed reads as.

Nothing is backfilled. An existing database cannot distinguish seed content
from builder content after the fact, and guessing is worse than admitting.

## The two rules

**Creating** content claims it: sets `authored_by`, `last_edited_by`, and
`origin = builder`.

**Editing** content sets `last_edited_by` and nothing else.

That second rule is deliberate in both directions:

- Editing a colleague's room does not take it from them.
- Rewriting a seed room does not convert it into your work.

The consequence worth knowing: a builder who takes an unattributed area and
rebuilds it from scratch gets no credit for it. That is the conservative side
of a call that cannot be made correctly from a database predating attribution.
An explicit claim surface is the honest fix if it becomes a real problem.

Read-only subcommands (`redit` with no argument, `oedit <vnum> show`,
`quedit <vnum> show`, …) never record an edit. Looking at a room is not
touching it, and if it were, `last_edited_by` would stop meaning anything.

## Where stamps happen

Not at `Db::save_room_data`. That is the true chokepoint — every write in the
engine goes through it — and it is the wrong place for two reasons:

1. **It carries no actor.** A save knows what changed, never who changed it.
2. **It is not builder-only.** The combat, weather, migration and spawn ticks
   all save rooms and mobiles. Hooking it would credit a builder for every
   wandering NPC that walked through their area.

So stamps live at the two surfaces where a builder is unambiguously the actor:

| Surface | Where |
|---|---|
| REST / MCP | `create_*` and `update_*` handlers in `src/api/`, beside the existing `notify_builders` call |
| OLC | One call per editor — `redit`, `oedit`, `medit`, `aedit`, `quedit`, `dig`, `acreate` — at the point the permission gate has passed |

One stamp per *editor*, not per setter: `src/script/rooms.rs` alone has
thirty-five save sites, and stamping each is not maintainable.

Sub-resource API handlers (`set_exit`, `add_trigger`, `add_extra_desc`, …) do
not currently move `last_edited_by`. Authorship and origin — the fields that
carry weight — are unaffected.

## From a script

```rhai
stamp_content_created(kind, id, builder_name)   // claim it
stamp_content_edited(kind, id, builder_name)    // record an edit
get_content_credit(kind, id)                    // #{found, authored_by,
                                                //   last_edited_by, origin,
                                                //   origin_label, counts}
```

`kind` is `room|item|mobile|area|quest`. `id` is a uuid for the first four and a
vnum for quests. All three are no-ops on a missing entity rather than errors — a
failed stamp must never abort an edit the builder already completed.

## Declaring an existing world

`attribution::stamp_unattributed(db, origin)` labels every currently
unattributed entity. It never overwrites content that already has an origin or
an author, so it is safe to re-run and safe over a world that already contains
hand-built rooms. The seed pass uses it, which is why adding a sixth `seed_*`
module cannot forget to stamp.

Implementation: `src/types/provenance.rs` (the rules), `src/attribution.rs`
(the I/O).

## Seeing it: `build credits`

```
build credits              who built the area you are standing in
build credits <prefix>     who built a named area
build credits world        the whole world
```

One row per builder — rooms, items, mobiles, quests, total — plus a count of
everything unattributed. Your own row is highlighted.

This is the surface the whole attribution slice exists for. `AreaData.owner` has
always been an ACL, and until this shipped nothing in the game displayed who made
anything. It is the one need scoring alone cannot meet: a number beside your own
name is not the same as your name beside your work.

Counted from provenance rather than from the score, so content that earns nothing
still shows its author. A quest belongs to whichever area its giver lives in —
quests carry no `area_id`, and inventing one would be a schema change wearing a
helper's clothes.

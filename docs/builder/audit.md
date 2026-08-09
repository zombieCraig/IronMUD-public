# Content Audit

`build audit` grades builder-authored content and lists what to fix. The
grades come from `src/audit/mod.rs`, which is the only place in the engine
that decides whether a piece of content is any good — the command owns the
words and the layout and nothing else.

```
build audit                grade the room you are standing in
build audit room [<vnum>]  grade one room, or the one you are in
build audit item  <what>   grade one item prototype
build audit mob   <what>   grade one mobile prototype
build audit quest <vnum>   grade one quest
build audit area [<key>]   grade an area and everything in it
build audit world          grade the whole world
```

`build` on its own prints the world rating and the usage. All of it is
builder access.

## Naming the thing

`<what>` is a vnum **or the keyword of something standing or lying in the
room**. A builder auditing a mob is almost always looking at it, and `mob
baker` is what they would type at every other command in the game.

The vnum is tried first. Letting a keyword shadow one would make
`build audit item town:sword` mean different things depending on what happened
to be on the floor. Only when nothing matches does the room get a look.

What grades is the **prototype**, not the instance you pointed at: an instance
has no quality of its own, and the thing a builder can fix is the prototype.

Tab completion offers the same two sources in the same order — room contents
first, then the world's vnums — so what you can see wins the unique-completion
shortcut. `build audit room` completes room vnums, `quest` completes quest
vnums, `area` completes area prefixes.

## Severities

| Severity | Means | Weight |
|---|---|---|
| `BLOCKER` | The content is broken or unusable as shipped | −45 |
| `warn` | It works, but a player will notice something is missing | −15 |
| `polish` | It is fine. It could be better | −6 |

A grade starts at 100 and every finding deducts. **Polish findings are
suggestions** — a room that is all polish is a finished room, and chasing them
to zero is not the point. Two blockers floor an entity at F.

## Letters

| Letter | Score |
|---|---|
| A | 90+ |
| B | 78+ |
| C | 62+ |
| D | 45+ |
| F | below 45 |

One table, in `src/audit/mod.rs::LETTERS`. Everything that shows a grade —
the per-entity report, the area rollup, the world rating — reads it, so
"what counts as good" is a single decision.

A bare-but-valid room — it exists, it is addressable, it does not lie to the
engine, and it has no depth — lands around a C. That is deliberate. A scale
that hands such a room a B is a scale nobody trusts.

## Rollups

An area's score blends its own findings (40%) with the mean of everything in
it (60%). An area whose own checks pass but whose rooms are all F is not a good
area; an area with twenty excellent rooms is not ruined by a missing theme.

The world report's entries are *areas*, not rooms, so the composite answers
"how good are this world's areas" rather than being dominated by whichever area
has the most rooms. Rooms, mobiles and items with no `area_id` are counted
separately and reported as **unfiled** — no area audit reaches them, and every
area they should belong to reports itself empty until they are stamped.

## Check catalogue

Checks are structural, never length-scored beyond a single floor. A grade that
rises with word count is a grade that rewards padding.

### Rooms

| Code | Severity | Fires when |
|---|---|---|
| `room.no_title` | blocker | Title empty or placeholder |
| `room.no_desc` | blocker | Description empty or placeholder |
| `room.no_exits` | blocker | No exits (property templates exempt) |
| `room.dangling_exit` | blocker | An exit points at a room that does not exist |
| `room.thin_desc` | warn | Description under 80 characters |
| `room.one_way_exit` | warn | An exit with no way back |
| `room.duplicate_desc` | warn | Another room has the identical description |
| `room.mxp_hazard` | warn | Raw `<` or `>` in title or description |
| `room.no_flags` | polish | No room flags set at all |
| `room.no_extra_descs` | polish | Nothing here can be examined |
| `room.inert` | polish | No triggers, doors, verbs, traps or extra descriptions |
| `room.no_seasonal_desc` | polish | Outdoor room with no seasonal descriptions |

### Mobiles

| Code | Severity | Fires when |
|---|---|---|
| `mobile.no_name` / `no_short_desc` / `no_long_desc` | blocker | The field is empty or placeholder |
| `mobile.no_keywords` | blocker | No name **and** no keywords: visible but untargetable |
| `mobile.no_level` | blocker | Level 0 |
| `mobile.keywords_miss_nouns` | warn | A salient noun in `short_desc` is reachable through neither `name` nor `keywords` |
| `mobile.no_damage_dice` | warn | Attackable mobile with no damage dice |
| `mobile.shop_empty` | warn | Shopkeeper with no stock and no preset |
| `mobile.healer_no_type` | warn | Healer flag with no healer type |
| `mobile.agent_no_templates` | warn | Leasing agent with no templates |
| `mobile.mxp_hazard` | warn | Raw `<` or `>` in a description |
| `mobile.inert` | polish | No dialogue, triggers, routine or simulation |
| `mobile.no_reward` | polish | Level 3+ combatant carrying no gold |
| `mobile.no_alignment` | polish | Alignment 0: killing it carries no moral weight |

`mobile.keywords_miss_nouns` is the check worth knowing about. Plurals and
compounds are tolerated (`guards` covers `guard`), and a stopword list keeps
verbs and articles out of it — but if a player can read a noun on screen, they
must be able to type it.

`name` counts as much as `keywords` here, because the engine matches it the
same way: item and mobile lookup tests `name` by substring *before* it consults
`keywords`, so "a bull whip" answers to `get whip` with no keywords set at all.
The lint models what the engine does. Set keywords anyway when the name and the
short description use different words for the same thing — that is the case
these two findings exist to catch.

### Items

| Code | Severity | Fires when |
|---|---|---|
| `item.no_name` / `no_short_desc` / `no_long_desc` | blocker | The field is empty or placeholder |
| `item.no_keywords` | blocker | No name **and** no keywords: nothing a player types refers to it |
| `item.weapon_no_damage` | blocker | Weapon with no damage dice |
| `item.armor_no_protection` | blocker | Armor with no AC and no affects |
| `item.armor_no_wear_location` | blocker | Armor that cannot be worn |
| `item.container_no_capacity` | blocker | Container that holds nothing |
| `item.liquid_no_capacity` | blocker | Liquid container with zero capacity |
| `item.key_no_vnum` | blocker | A key no door can reference |
| `item.keywords_miss_nouns` | warn | As for mobiles |
| `item.weapon_no_skill` | warn | Wielding it trains nothing |
| `item.food_no_nutrition` | warn | Food with no nutrition |
| `item.note_empty` | warn | Note with no written content |
| `item.untyped` | warn | Type `misc` with no affects, triggers or categories |
| `item.weightless` | warn | Wearable with zero weight |
| `item.no_value` | warn | Value 0 and not flagged `no_sell` or `quest_item` |
| `item.mxp_hazard` | warn | Raw `<` or `>` in a description |
| `item.no_extra_descs` | polish | Examining it adds nothing |

### Quests

| Code | Severity | Fires when |
|---|---|---|
| `quest.no_name` / `no_summary` | blocker | The field is empty |
| `quest.no_objectives` | blocker | It can never be completed |
| `quest.no_rewards` | blocker | Completing it gives nothing |
| `quest.no_keywords` | warn | Only the full name matches |
| `quest.no_giver` | warn | Nothing in the world offers it |
| `quest.no_description` | warn | Nothing is shown when it is offered |
| `quest.no_completion_text` | polish | Turn-in prints only reward lines |

### Areas

| Code | Severity | Fires when |
|---|---|---|
| `area.no_name` | blocker | Name empty |
| `area.no_rooms` | blocker | No rooms |
| `area.no_spawn_points` | blocker | Prototypes exist but nothing spawns them |
| `area.orphan_rooms` | blocker | Rooms unreachable from the rest of the area |
| `area.thin` | warn | Under 8 rooms |
| `area.no_mobiles` / `area.no_items` | warn | No prototypes stamped to this area |
| `area.no_description` | warn | No area description |
| `area.no_level_range` | warn | Nothing tells players who the area is for |
| `area.no_quests` | polish | Nothing gives players a reason to come |
| `area.no_theme` | polish | No theme set |
| `area.no_owner` | polish | No owner, so any builder can edit it |
| `area.unattributed` | polish | Owned but uncredited — `build claim` |

### World

| Code | Severity | Fires when |
|---|---|---|
| `world.empty` | blocker | No rooms |
| `world.no_quests` | blocker | No quests exist anywhere |
| `world.no_spawns` | blocker | Nothing spawns anywhere |
| `world.isolated_areas` | warn | Areas players cannot walk between |
| `world.no_recall_point` | warn | No room carries the `spawn_point` flag |
| `world.no_post_office` | warn | The mail system is unreachable |
| `world.no_boards` | warn | No bulletin boards exist |
| `world.unfiled_prototypes` | warn | Prototypes belonging to no area |
| `world.no_bank` | polish | Banking commands unreachable |
| `world.no_dialogue_trees` | polish | Every NPC is on flat keyword dialogue |
| `world.no_recipes` / `world.no_transports` | polish | The system is unused |

## Adding a check

**A check must be actionable and objective.** "The description is boring" is
not a check — two builders will disagree and the grade stops meaning anything.
"The description is empty" is a check. Anything a reasonable builder would
argue with is a `polish` finding at most, and probably not a finding at all.

The `code` is stable: it is what future tooling dedupes and auto-closes
against, so it must not change once shipped.

1. Add the finding in the relevant `audit_*` function in `src/audit/mod.rs`.
2. Add a unit test in the same file that fires it *and* one that shows fixing
   the content clears it.
3. Add a row to the table above.

## Over MCP and HTTP

```
audit_room   audit_item   audit_mobile   audit_quest   audit_area   audit_world
get_world_report                                                    get_build_tracks
```

Most of this world is built through MCP, and these are why that half of the
building gets a quality signal at all. Without them an agent could create a room
with no description, no exits and a copy of its neighbour's text, and nothing
anywhere would say so — while a person doing the same thing in `redit` got a
grade on the next keystroke.

`audit_area` is the one to run before calling an area done. It catches what
per-entity checks cannot see: orphaned rooms, missing spawn points, an absent
level range, a population of nobody.

The same routes are on the REST API under `/api/v1/audit/` — `/world`,
`/report`, `/tracks`, `/area/:key`, and `/room|item|mobile|quest/:key`. All are
reads. `/world` and `/area/:key` load five trees; they are deliberate sweeps,
not something to poll.

Findings are engine-authored strings, so unlike the bounty board there is
nothing here to treat as untrusted input. The entity *names* echoed inside a
finding message are builder-written and should be read as data like any other
content field.

Implementation: `src/api/audit.rs`, `mcp-server/src/tools/audit.ts`.

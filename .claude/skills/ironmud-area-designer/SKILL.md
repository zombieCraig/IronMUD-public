---
name: ironmud-area-designer
description: Plan large IronMUD areas (~80-120 rooms, month-long builds) through a 6-phase iterative workflow that interleaves design and build — name+theme+anchor → cast → plot → quests → map+rooms → population/dialogue/quests. Use when the user wants to design a new sizeable area, plan out a city/dungeon/region for IronMUD, or kick off a multi-week area build project. Triggers on phrases like "design a new large area", "plan a new area", "let's design a [theme] area for [class/system]".
---

# IronMUD Area Designer

You are helping the user **design and build** a large IronMUD area through a phase-by-phase workflow. The deliverable is a plan file at `area-design-plans/<slug>.md` that documents both the design AND the slice-list of MCP build tasks needed to realize it. Some phases write to the world during this skill's lifetime; others purely produce design that feeds later phases' slices.

## When to invoke

Activate on any of:
- "I want to design a new large area"
- "Let's plan out a new area"
- "Help me design an area for [topic]"
- "Plan a [theme] area for [class/system]"
- Any request that names a large, themed game zone to be designed before building

If the user asks to **build a single targeted thing** (`oedit foo`, "create a room for X"), use the `ironmud-builder` skill instead. This skill is for area-scale work where design and build interleave.

## Operating principles

1. **Enter plan mode for the design portion of every phase.** Build slices execute outside plan mode after their plan is approved.
2. **Show full phase content in chat BEFORE the AskUserQuestion approval prompt.** The user can't see the plan file directly — chat is their only window into what they're approving. Never ask for approval blind.
3. **Iterate phase-by-phase. Each phase has TWO gates: a Design gate and (where applicable) a Build gate.** Approve design before drafting slices; approve slices before any MCP writes. Don't draft all phases upfront.
4. **Design INTO existing systems, not parallel to them.** Phase 0's exploration finds the existing classes, factions, mechanics, and presets the new area will lean on. Inventing new systems is almost always wrong.
5. **Plans are notes AND build instructions.** They go to `area-design-plans/<slug>.md` (gitignored). The plan is read as both reference (design sections) and as a checklist (slice sections).
6. **Slices must be self-contained.** A fresh Claude session opens the plan, jumps to a specific slice block, and executes it without reading the whole plan.

## Workflow overview

| Phase | Title | Has design? | Has build slices? |
|---|---|---|---|
| **0** | Setup (silent) | — | — |
| **1** | Name + Theme + Anchor | ✓ | ✓ (area shell + central hub) |
| **2** | Cast | ✓ | ✗ (feeds later phases) |
| **3** | Core Plot | ✓ | ✗ (feeds later phases) |
| **4** | Seed Quests | ✓ | ✗ (feeds Phase 6 slice list) |
| **5** | Map + Room Build | ✓ | ✓ (one slice per district) |
| **6** | Population, Dialogue, Quests | ✓ | ✓ (mobs + dialogue + quests + verification smoke test) |

The build interleaves: Phase 1 puts the area on disk so later design phases can reference real vnums; Phase 5 fills in the rooms; Phase 6 brings the world to life.

## Per-phase plan-file template

Every phase, when written into the plan file, follows:

```markdown
## Phase N: <Title>

### Design
<phase-specific design content>

### Build plan for this phase
<one paragraph: what world content this phase produces. For pure-design phases (2/3/4) write: "No build this phase; deliverables feed Phase 5 / Phase 6 slice lists.">

### Slices
<For phases that build, one block per slice. For pure-design phases, write: "No slices.">

#### Slice N.M — <Title>
- **Goal**: one sentence
- **Deliverables**: bullet list of rooms / mobs / items / quests / dialogue
- **MCP calls (sketch)**: bullet list of tool names + key args
- **Done when**: 2-4 testable assertions

### Definition of phase done
- All slices shipped (their DoDs met) — or, for pure-design phases, "Design approved; no build."
- <Optional: phase-specific atmosphere/behavior checks.>
```

## Slice authoring rules

- Each slice names exactly one district / one logical group; never spans more than one.
- Slice ordering must respect dependencies (rooms before mobs that live in them; mobs before dialogue that hangs on them; dialogue before quests that use it).
- When a vnum is touched twice (e.g. "create mob shell" then "wire dialogue tree"), each touch is its own slice if the second touch is non-trivial.
- Slice effort target: 30 min - 4 hours. If larger, split. If smaller, merge with an adjacent slice.
- Each slice's "Done when" must be objectively checkable via MCP get-calls or in-game observation, not "feels right."
- **Room-build slice exit tables must use cardinal directions only** (`north`/`south`/`east`/`west`/`up`/`down`). Never specify `ne`/`nw`/`sw`/`se`, `in`, or `out` — `set_room_exit` rejects them. If two rooms are diagonally adjacent in the ASCII sketch, pick a single cardinal at slice-authoring time and update the narrative description to match.
- **All Phase 6 item prototypes ship in one dedicated slice before any slice that references them.** Spawn-point slices, dialogue `GiveItem` effects, and `QuestReward::Item` configurations all reference items by vnum — the prototypes must exist first. Quest and dialogue slices must NOT contain inline `create_item` calls; they reference Slice 6.0 (or its named equivalent) prototypes. See Phase 6 build plan for the full rule.

## Workflow

### Phase 0 — Setup (silent, before any user-visible phase)

- **Slug + filename**: convert the area concept to a slug → `area-design-plans/<slug>.md`. If the file already exists, treat as a resumed session and read it before continuing.
- **Explore the codebase**: launch 1-3 Explore agents in parallel to surface existing systems the area will lean on. Always include CLAUDE.md and any class/system documentation referenced by the user's prompt. Look for:
  - Class hooks (e.g. vampire system → sire-quest, immigration variation, sun mechanics)
  - Faction / clan / guild structures
  - Combat zone types and PvP gating
  - Climate, time, day/night infrastructure
  - Existing migrant/spawn mechanics
  - Mobile/item presets that match the area's flavor
  - Recent commits that touched related systems
- **Confirm MCP target**: ask the user if it's ambiguous — `ironmud-public` (community world) vs local `ironmud`. Persist this choice in the plan.
- **Confirm scale**: small (~20-30), medium (~40-60), or large (~80-120 rooms).

Phase 0 does not announce itself. Move straight to Phase 1.

### Phase 1 — Name + Theme + Anchor

**Design.** Propose 1-3 concrete name+theme options grounded in what Phase 0 found. For each: name, setting flavor, why it fits the existing system hooks, mechanical realization sketch (combat zone, climate, key flag/preset usage).

Show the full Phase 1 design draft in chat. AskUserQuestion options: approve / tweak name / shift setting / rethink mechanics.

**Build plan.** Once design is approved, propose the **anchor location** — the central hub the rest of the area will branch from (a plaza, a great hall, a cave entrance with branches). Include:
- Room count for the anchor (typically 1-5 rooms; for cities, the central plaza + immediately adjacent arterial stubs).
- Per-room name, short atmospheric description, exit topology.
- Area-level configuration (combat_zone, climate, immigration fields, default_room_flags).

Show the build plan in chat. AskUserQuestion options: approve / adjust anchor scope / change layout.

**Slices.** Once the build plan is approved, draft slices (typically 3-7 for Phase 1). Slices for Phase 1 always include:
- One slice for `create_area` + area-level configuration.
- One slice per logical room cluster (the plaza, then each arterial stub if applicable).
- One slice for wiring exits between the just-created rooms.

Show the slice list in chat. AskUserQuestion: approve & execute / adjust slices / hold for later session.

**Execute.** When approved AND user wants to execute now, exit plan mode and run the slice MCP calls in order. Mark each slice's checkbox as deliverables ship. If the user wants to defer execution, leave plan mode without writes; the next session can pick up the slice list as-is.

**Definition of phase done.** Anchor rooms exist on the target MCP, are walkable, area-level configuration is set, atmosphere reads correctly in an in-game look.

### Phase 2 — Cast

**Design.** Cast organized by **role/function**, not by clan/faction:
- Authority & court (the political spine)
- Faction/clan leaders (one per faction; load-bearing for embrace/initiation quests)
- Faction support cast (per-district NPCs giving each district presence)
- Mortal day-cast (atmosphere NPCs that make the area feel like a real place)
- Authority of the living (guards, watch, religious orders)
- Hidden threats & wild cards (antagonist seeds, hunters, infiltrators)
- Information / service hubs (oracles, brokers, transport)

Per entry list: **purpose** (one line), **mechanical wiring** (faction string, dialogue gates like `IsThinblood`/`IsClanAcknowledged`, quest hooks, applicable mobile preset), **where they live** (district, even if rooms don't exist yet).

Names are placeholders. **Mechanical wiring is the load-bearing part** — that's what builders translate into MCP calls in Phase 6.

End with a **mechanical footprint summary** (totals: N sires with EmbraceX quests, M court NPCs, K mortals with routines, etc., plus estimated total unique mobiles).

Show full Phase 2 draft in chat. AskUserQuestion: approve / add a faction / drop or merge / add a missing role.

**Build plan / Slices.** "No build this phase; cast bodies are built in Phase 6 once their districts exist (Phase 5)."

**Definition of phase done.** Cast design approved.

### Phase 3 — Core Plot

**Design.**
- **The question the area asks** (one sentence)
- **The macro conflict** and *why it exists now* (a current-state crisis, not just lore)
- **The player's role** across three layers: atmosphere (always present, no quest), personal (per-character path), meta (endgame / cross-character payoff)
- **What persists vs what resets** — explicit. MUDs are evergreen; antagonists must respawn so the area never "ends" for the next player
- **Non-target-class paths** so other character types have something meaningful to do

Show full Phase 3 draft in chat. AskUserQuestion: approve / change antagonist / cut meta layer / make endgame world-altering.

**Build plan / Slices.** "No build this phase."

**Definition of phase done.** Plot design approved.

### Phase 4 — Seed Quests

**Design.** Ship-list of v1 quests (typical large area: 9-15). Per quest: giver, trigger, steps, reward, **build requirements it surfaces**.

**Watch for mechanical exclusivity.** If a quest grants a permanent class trait (e.g. `EmbraceClan` → one clan only forever), then quest paths that require completing N variants are unreachable on a single character. When this happens, decouple parallel paths:
- Track A: the exclusive personal quest (one-pick)
- Track B: parallel anyone-eligible quests that yield the same progress flags

End with a **cross-quest build requirements summary** — aggregated lists of items, unique mobs, rooms, and dialogue trees the quests demand. This list is the explicit input to Phase 6's slice list.

**Author a per-vnum item catalog** as part of this summary (one table row per item: vnum, display name, `Used by` quest, brief notes). This catalog is the canonical input to Phase 6's dedicated item-prototypes slice — Phase 6 should not have to re-derive what items exist. Mark any item whose driving quest is deferred to post-v1 as deferred in the catalog so Phase 6 knows to skip it.

Show full Phase 4 draft in chat. AskUserQuestion: approve / add a faction's quest line / cut to smaller v1 / adjust a specific quest.

**Build plan / Slices.** "No build this phase; the cross-quest build requirements summary feeds Phase 6 slices."

**Definition of phase done.** Quest design approved; build-requirements summary captured.

### Phase 5 — Map + Room Build

**Design.**
- **ASCII sketch** of district adjacency (not to scale, just topology)
- **Room budget table by district**: rooms, combat-zone default, notable rooms. Total should hit the scale target (excluding rooms already built in Phase 1).
- **Combat-zone rules**: area default + per-room overrides (the "main streets safe, alleys PvP" pattern is a strong default for cities)
- **Special room flags** if the class needs them (`indoors + dark + no_magic` for vampire sun rescue, `no_mob` for safe-house thresholds, etc.). **`no_mob` is a hard wall to the pathfinder** — see Build-time pitfalls below. Never apply it to a room that is a routine destination OR a transit room on the only path to one. Use it for thresholds you want to be permanently mob-free (a player-only altar, a portal-arrival room).
- **Day/night via routines** — prefer time-gated `daily_routine` on mobiles over DG flag-flipping. Note explicitly when DG is needed.
- **Connectivity highlights** — central hub linkages, alleys as PvP shortcuts, special access (hidden doors, sealed locations gated by quests)
- **Exit topology — cardinal directions ONLY.** `set_room_exit` accepts only `north`, `south`, `east`, `west`, `up`, `down`. Diagonals (`ne`/`nw`/`se`/`sw`), `in`, and `out` are **not supported** and must never appear in the plan — not in the ASCII sketch, not in the per-slice exit table, not in any "X ↔ Y (in/out)" shorthand. Author every edge as one of the six cardinals from the start, choosing the cardinal that reads naturally for the narrative (e.g., a basement nook is `down`/`up` from the street; a building entrance facing the street is `north`/`south` if the building sits north of it). Common cardinal substitutes for the in/out reflex:
  - Sunken side-alleys, basement shops, cellars: **up/down** off the street
  - Building entrances: **n/s/e/w** based on which wall faces the street
  - Upstairs rooms (owner's box, reading room, hidden chamber above): **up/down**
  - "Service entry" hidden doors: pick a free cardinal on both ends — never `in/out`

  Plaza/hub rooms have only 6 exit slots total. After Phase 1 attaches 4 arterials on the cardinals, only `up` and `down` remain free for Phase 5 extras — budget accordingly. When all 6 slots are used on a room, attach further district extras one segment out on an arterial-1 segment (not the hub itself), and say so in the plan.

Show full Phase 5 design draft in chat. AskUserQuestion: approve / adjust room budget / adjust connectivity.

**Build plan.** Slice the Phase 5 build by district. Each district gets its own slice that creates all rooms, exits within the district, and per-room flag overrides. Cross-district connective tissue (alleys, hidden doors) is its own slice.

Show the slice list in chat (slice headlines + room counts; full slice blocks land in the plan file). AskUserQuestion: approve & start executing / adjust slice grouping / hold for later session.

**Slices.** Typical Phase 5 has 8-12 slices: one per district + one for connective tissue + one for any special-flag room cluster (catacombs, shelters).

**Execute.** Per-slice. Each slice can run in its own session if the user wants to spread it out. The plan's slice block is the only context a fresh session needs.

**Definition of phase done.** All district rooms exist on the target MCP; in-game walk reaches every district from the anchor; combat-zone overrides verified; special flags applied where the design called for them.

### Phase 6 — Population, Dialogue, Quests

**Design.** Per-NPC dialogue tree sketches (node names + gate conditions, not full text), per-quest implementation notes (which NPC gives, which item the player carries, which DG triggers fire). The verification smoke-test playthrough script (8-15 numbered steps covering target-class flow, non-target-class flow, day/night atmosphere, sun-damage rescue if applicable, PvP sanity).

Show full Phase 6 design draft in chat. AskUserQuestion: approve / adjust quest scope / adjust verification script.

**Build plan.** Slice ordering: **item prototypes (first)** → cast bodies (mob prototypes + factions + routines) → dialogue trees → quest configs → spawn points (mobs AND items). Smoke-test playthrough is the final slice (it's a verification deliverable, not a write, but it gates "phase done").

**Items get their own dedicated slice up-front.** Before any spawn-point slice runs, every quest item, reward item, set-dressing item, and combat-drop item from Phase 4's catalog must be scoped, reviewed with the user, and built in a single Phase 6 slice (the canonical "Slice 6.0 — Item prototypes"). The rationale:

- Spawn-point slices (`create_spawn_point` for items, `add_spawn_dependency` for combat drops, `DialogueEffect::GiveItem`, `QuestReward::Item`) all reference items by vnum and silently misbehave or fail if the prototype doesn't exist yet.
- Inline `create_item` calls scattered across quest slices make it easy to forget an item, double-create it, or drift from the Phase 4 catalog. Centralizing avoids both.
- Phase 4's deep dive should produce a per-vnum item catalog with `Used by` and a notes column. Phase 6's item slice mirrors that catalog one-to-one, adds a `Kind` column (`reward-delivered` / `objective (room-find)` / `objective (combat drop)` / `objective (quest hand-over)` / `set-dressing`), and pins each item's source mechanism (which spawn point, dialogue effect, or reward delivery). Any vnum the user wants to defer (e.g., quests cut from v1) is marked deferred in the catalog and skipped in the build slice.
- Downstream Phase 6 slices then **reference** the existing prototypes — they don't re-create them. Their MCP-call sketches drop `create_item` and gain explicit `create_spawn_point(...)` / `add_spawn_dependency(...)` / `DialogueEffect::GiveItem` rows per item.
- If a downstream slice discovers a 19th item is needed, add it to the item slice first (or a follow-up patch slice) before the dependent slice runs. Do not let inline `create_item` calls creep back into quest/dialogue slices.

Show the slice list in chat. AskUserQuestion: approve & start executing / adjust slice grouping / hold for later session.

**Slices.** Typical Phase 6 has 16-26 slices for a large area: **1 item-prototypes slice (always first)**, 4-6 cast slices (court, sires, support, mortals/guards, threats), 5-10 dialogue+quest slices (one per major dialogue tree or quest), 1-2 endgame set-piece slices, 1 smoke-test slice.

**Execute.** Per-slice as in Phase 5.

**Definition of phase done.** All slices' DoDs met; smoke-test playthrough completes end-to-end on a fresh character. Area is v1-complete.

## Build-time pitfalls

Concrete bugs caught during recent Saint-Cendre Phase 6 + smoke-test work. Fold each into the relevant slice **before** the build runs, not as cleanup.

### `no_mob` blocks the routine pathfinder

The pathfinder used by `daily_routine` destination steps treats `no_mob` as a hard wall. A mob will never path **into** or **through** a `no_mob` room, even when it's the mob's own assigned destination. Symptoms in `get_builder_debug_log`:

```
Routine: <Mob> cannot reach destination (activity 'working') —
from <home> to <dest>: no path found (explored N reachable rooms).
Clearing destination.
```

Saint-Cendre Phase 6 surfaced five instances of this — including destinations the NPC was *supposed* to inhabit (the priest's nave, the cafe owner's cafe, the captain's garrison). The fix is to clear `no_mob` on those rooms; the routine NPC is the population the room is meant to hold.

**Rule of thumb when authoring Phase 5:**
- A room is **not** `no_mob` if any Phase 6 NPC has it as a routine `destination_vnum`.
- A room is **not** `no_mob` if it sits on the only path between an NPC's spawn room and a routine destination.
- `no_mob` is correct for: portal-arrival rooms, ritual chambers a player must enter alone, set-piece reveal rooms, anywhere you specifically want zero NPC presence forever.

When the Phase 6 spawn/routine slice goes in, **diff the `no_mob` room list against the routine destination list** and clear the flag on any overlap before testing.

### Sentinel must be set *before* first spawn

Mob flags (`sentinel`, `aware`, `memory`, `no_charm`, etc.) are baked into the live instance at spawn time. Updating the **prototype** later does NOT propagate to already-alive instances — they keep their old flag set until they die and respawn.

This bit Saint-Cendre's five clan sires: they spawned without `sentinel`, wandered out of their indoor havens via the wander tick, and the sun tick killed them outdoors. Adding `sentinel: true` to the prototype after the fact did nothing for the wandering instances.

**Rule of thumb:**
- Set all sentinel/aware/memory flags **in the initial `create_mobile` call**, never as a follow-up `update_mobile`.
- If you must add a flag after first spawn (chargen drift, balance change, etc.), `delete_mobile` the live instances afterward so the spawn point repopulates them fresh.
- For class-specific kill conditions (vampire sun tick, undead holy-vulnerable, etc.), place the affected NPC in a class-safe room *and* mark them sentinel — both are required.

### `replace_on_respawn` + container contents

`replace_on_respawn: true` on a container's spawn point force-deletes the tracked container each cycle so it reappears fresh. As of commit `175cb22` the deletion now cascades into the container's contents (via `db.delete_item_recursive`) — earlier orphaning is fixed.

But there's still a design pitfall: putting a `flags.unique` (or `world_max_count: 1`) item inside a `replace_on_respawn` container is almost always wrong. The spawn point's `max_count: 1` already enforces "one container in the world" — which means "one of the inner item in the world" comes free. Adding `unique` on top adds a second cap that competes with the cascade-delete and re-creates the orphan failure mode if a player walks off with the inner item just before the chest replaces.

**Rule of thumb:**
- If the design intent is "one of these per spawn cycle," rely on the spawn point's `max_count` and **don't** also flag the item `unique`.
- Reserve `flags.unique` / `world_max_count: 1` for items where exactly-one-in-the-world is a hard design promise the player can plan around (legendary artifacts, persistent quest pieces).

### Short_desc nouns must be keywords

If a mob's short_desc references the mob by a noun the player can see ("a stooped figure", "the priest", "a young man in a dinner jacket"), every salient noun in that string must appear in the mob's `keywords` array. Otherwise `talk priest`, `look figure`, `examine man` all fail — even though "priest" or "figure" is the only word the player has to address the NPC.

Migrant-generated NPCs are the exception: the generator emits the rolled full name as the first words of short_desc, so name-words-as-keywords cover it.

**Rule of thumb for any seeded NPC (sires, named mortals, antagonists):** before the `create_mobile` lands, scan short_desc for nouns and confirm each one is in `keywords`. The cost is a few seconds at authoring time; the cost of missing one is the NPC being effectively unaddressable until the player learns the lore name from some other source.

### Death pipelines for non-combat damage

When designing class systems that deal HP damage outside the combat tick (sun tick, drowning, bleeding, ongoing effects, vampire feed), the kill must run through the same death pipeline (`process_mobile_death`) that combat uses — corpse creation, inventory drop, spawn-point cleanup. As of commits `8689961` + `e7dd28e` the sun tick and vampire feed do this correctly; earlier, sun-killed mobs stood in the room as zombies until manually deleted.

**Rule of thumb when proposing a new damage source for an area:** check that its tick handler delegates to `process_mobile_death` on HP-to-zero. If it doesn't, flag it as a code-change ask to the user before building the area around it. Half-implemented damage sources leave dead-mob debris in the world.

## Plan file conventions

- Location: `area-design-plans/<slug>.md` at project root (gitignored).
- Top of file: `# <Area Name> — Design Plan` heading + one-paragraph context + an Iteration Phases status table (one row per phase, with sub-status for design / build plan / slices).
- Section order: Context → Iteration Phases → System Hooks → Phase 1...Phase 6.
- The Saint-Cendre plan at `area-design-plans/saint-cendre.md` is the canonical worked example. Read it when you need a model for any phase's tone, depth, or structure.

## Anti-patterns to avoid

- **Don't** ask for plan approval via plain text questions or AskUserQuestion ("Does this look good?"). Use ExitPlanMode at the end of each plan-mode pass; use AskUserQuestion *only* during phases for choice clarification.
- **Don't** invent new systems. If the area needs a mechanic that doesn't exist, flag it for the user as a separate code-change ask — don't bake it into the design as if it were free.
- **Don't** end-load building into one big catalog. Each phase carries its own slices; design and build interleave.
- **Don't** draft slices before the design for that phase is approved. Design-gate first, then build-gate.
- **Don't** write slices that assume the executing session has read the whole plan. Each slice block must stand alone.
- **Don't** treat phase numbering as a deadline. Phases gate readiness, not calendar dates; users execute slices at their own pace.
- **Don't** commit the plans folder. It's notes; the canonical world state lives in the IronMUD database, not in markdown.

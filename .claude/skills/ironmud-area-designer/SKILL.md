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

Show full Phase 4 draft in chat. AskUserQuestion: approve / add a faction's quest line / cut to smaller v1 / adjust a specific quest.

**Build plan / Slices.** "No build this phase; the cross-quest build requirements summary feeds Phase 6 slices."

**Definition of phase done.** Quest design approved; build-requirements summary captured.

### Phase 5 — Map + Room Build

**Design.**
- **ASCII sketch** of district adjacency (not to scale, just topology)
- **Room budget table by district**: rooms, combat-zone default, notable rooms. Total should hit the scale target (excluding rooms already built in Phase 1).
- **Combat-zone rules**: area default + per-room overrides (the "main streets safe, alleys PvP" pattern is a strong default for cities)
- **Special room flags** if the class needs them (`indoors + dark + no_magic` for vampire sun rescue, `no_mob` for safe-house thresholds, etc.)
- **Day/night via routines** — prefer time-gated `daily_routine` on mobiles over DG flag-flipping. Note explicitly when DG is needed.
- **Connectivity highlights** — central hub linkages, alleys as PvP shortcuts, special access (hidden doors, sealed locations gated by quests)

Show full Phase 5 design draft in chat. AskUserQuestion: approve / adjust room budget / adjust connectivity.

**Build plan.** Slice the Phase 5 build by district. Each district gets its own slice that creates all rooms, exits within the district, and per-room flag overrides. Cross-district connective tissue (alleys, hidden doors) is its own slice.

Show the slice list in chat (slice headlines + room counts; full slice blocks land in the plan file). AskUserQuestion: approve & start executing / adjust slice grouping / hold for later session.

**Slices.** Typical Phase 5 has 8-12 slices: one per district + one for connective tissue + one for any special-flag room cluster (catacombs, shelters).

**Execute.** Per-slice. Each slice can run in its own session if the user wants to spread it out. The plan's slice block is the only context a fresh session needs.

**Definition of phase done.** All district rooms exist on the target MCP; in-game walk reaches every district from the anchor; combat-zone overrides verified; special flags applied where the design called for them.

### Phase 6 — Population, Dialogue, Quests

**Design.** Per-NPC dialogue tree sketches (node names + gate conditions, not full text), per-quest implementation notes (which NPC gives, which item the player carries, which DG triggers fire). The verification smoke-test playthrough script (8-15 numbered steps covering target-class flow, non-target-class flow, day/night atmosphere, sun-damage rescue if applicable, PvP sanity).

Show full Phase 6 design draft in chat. AskUserQuestion: approve / adjust quest scope / adjust verification script.

**Build plan.** Slice ordering: cast bodies (mob prototypes + factions + routines) → dialogue trees → quest configs → spawn points. Smoke-test playthrough is the final slice (it's a verification deliverable, not a write, but it gates "phase done").

Show the slice list in chat. AskUserQuestion: approve & start executing / adjust slice grouping / hold for later session.

**Slices.** Typical Phase 6 has 15-25 slices for a large area: 4-6 cast slices (court, sires, support, mortals/guards, threats), 5-10 dialogue+quest slices (one per major dialogue tree or quest), 1-2 endgame set-piece slices, 1 smoke-test slice.

**Execute.** Per-slice as in Phase 5.

**Definition of phase done.** All slices' DoDs met; smoke-test playthrough completes end-to-end on a fresh character. Area is v1-complete.

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

# Building for Replicants (Resolve, Baseline Offices, Retirement)

Players who pick the **replicant** race (modern theme,
`scripts/data/races_modern.json`) get a vampire-grade special state:
a tireless body — no action anywhere costs stamina — balanced by
**Resolve**, a 0–10 mental-stress pool. This page describes the plumbing
builders use to support them in the world. No sample content ships with
the engine.

## The resolve economy (for reference)

| Event | Resolve |
|---|---|
| Single hit ≥ 15% of max HP | −1 |
| `push` (player-initiated combat surge) | −2 |
| Re-attuning a signature item (grief) | −2 |
| Sleeping | +1 per minute |
| `comfort <replicant>` from anyone (5 min recipient cooldown) | +1 |
| `focus` on a bonded memento, safe zone only (10 min cooldown) | +2 |
| **Passing a baseline test** | **full restore** |

At 0 Resolve the replicant rolls a breakdown: **panic** (luck/slow
debuffs), **lockup** (frozen 2 combat rounds; can't move/attack out of
combat), or **berserk** (frenzy + rage: bonus damage, cannot flee, and
the rage effect forces them to attack whoever is in the room — any
mobile outside safe zones, other players in PvP zones) for 60s, then
Resolve snaps back to 3.

## Pieces you have

| Piece | What it does |
|---|---|
| `RoomFlags.baseline_office` | Enables the `baseline` command in that room (`redit flag baseline_office`) |
| `retirement_order` trait | Stamped on a 3rd baseline strike; persistent until removed by quest/admin |
| `is_pc_replicant(connection_id)` Rhai fn | Gate dialogue/trigger scripts on the race |
| `get_pc_resolve` / `change_pc_resolve` / `get_pc_baseline_strikes` | Read/adjust the pool from quest scripts |
| `replicant_comfort(target_name)` Rhai fn | NPC- or quest-driven comfort (same cooldown as the social) |
| `trigger_pc_breakdown(connection_id)` | Script-driven stress break (horror set-pieces) |

## Recipe: a baseline office

1. **Pick or build the room.** An LAPD-style precinct annex, a Wallace
   Corp clinic, a back-alley grey-market tester — anywhere that fits the
   area. Convention: indoors, `city`, and set the room (or area) combat
   zone to `safe` so the office doubles as a `focus` spot.

2. **Flag it:** `redit flag baseline_office` (or MCP `update_room` with
   `flags.baseline_office: true`).

3. **Optional examiner NPC.** The `baseline` command provides the
   apparatus flavor itself ("Cells. Interlinked."), so an NPC is pure
   dressing — but a mobile with dialogue branching on
   `is_pc_replicant` sells the scene.

Test mechanics: pass = full Resolve restore and one strike worked off;
fail = no restore, +1 strike, 4-hour real-time retest lockout. Success
chance scales with current Resolve and recent breakdowns — players are
most likely to fail exactly when they most need the heal. That tension
is the design; place offices a meaningful (but not punitive) travel
distance from combat zones.

## Retirement hooks

A third strike issues a **retirement order**: a 24h all-stats −2 debuff
("recalibration") plus the `retirement_order` trait. The trait is the
builder surface:

- Hunter/blade-runner mobs: aggro or dialogue triggers keyed on the
  trait (DG `has_trait` checks).
- Hostile/fearful dialogue branches in corp-controlled areas.
- A redemption quest whose reward removes the trait (the stat debuff
  expires on its own).

## Signature items

Any item can be a signature item — players `attune <item>` themselves.
Builders can lean in: quest-reward mementos (a photograph, a carved
horse, an origami unicorn) make natural attunement targets. Remember
the costs are player-side: replacing an attunement costs −2 Resolve and
the new bond takes 24 real-time hours before `focus` works.

## Verify the loop

1. Create a replicant character (modern race preset). Prompt shows
   `[RES:10/10]` and no stamina segment; movement and combat never tire.
2. `push` twice in a fight, take a big hit — watch Resolve fall; at 0 a
   breakdown fires.
3. Sleep (slow trickle), have a friend `comfort` you, `attune` + (after
   the bond) `focus` in a safe room.
4. Visit your baseline office: `baseline`. Pass → full restore. Force
   fails to 3 strikes → retirement order broadcast + debuff + trait.

## Admin notes

- `revoke_pc_replicantism` (Rhai) clears the state entirely.
- The `resolve_cost_push` setting overrides the push cost (default 2).
- Balance constants live in `src/types/replicant.rs`.

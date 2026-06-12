# Building for Mutants (Mutation Points, Misfires, the Rot)

Players who pick the **mutant** race (modern theme,
`scripts/data/races_modern.json`) get a Mutant: Year Zero-style special
state: one randomly rolled mutation power, a **Mutation Point (MP)**
pool that refills *only* through self-harm, and privileged terms with
**the Rot** — a world contamination mechanic every race is exposed to.
This page describes the plumbing builders use to support them.

## The MP economy (for reference)

| Event | MP |
|---|---|
| `push` (costs 5–15% max HP, grants a 30s combat surge) | +1 to +3 |
| Rot-Eater mutant standing in a rotted room | +1 per gain interval |
| Activating a mutation power (`mutate <power> [target] [mp]`) | −1 or more |

There is **no passive MP regeneration** — power always costs flesh.
Spending more MP makes a power stronger, but every MP spent adds a
misfire die (1-in-6 each). A misfire rolls: MP pool zeroed / self-trauma
/ a permanent cosmetic deformity / **Overload** (permanent −1 to a
random attribute *and* a new random mutation — or +1 max MP if they
already own everything). Degeneration is the design: heavy users slowly
become less human and more Zone.

## Mutations

Defined in `scripts/data/mutations.json` (11 in v1: Acid Spit, Manbeast,
Insectoid, Frog Legs, Extreme Reflexes, Luminescence, Sonar, Reptilian,
Corpse Eater, Rot-Eater, Parasite). Exactly one is rolled at character
creation; more arrive only via Overload. Passive mutations ride
permanent buffs/traits (re-asserted by the mutation tick); actives
dispatch through `scripts/commands/mutate.rhai`.

Corpse Eater deserves a builder note: beyond shrugging off spoiled and
poisoned food, the mutant can `eat <corpse>` — any mob corpse in the
room (player corpses are refused). The corpse's loot spills to the
ground first, then the body restores 40 hunger and heals 2d6. Every
battlefield doubles as a larder for them; corpses they eat never reach
the decay tick.

## The Rot (world plumbing — affects every race)

Rooms carry a `rot_level` 0–3 (`redit rot <0-3>`, or `rot_level` via
API/MCP `update_room`). Characters lingering in a rotted room gain Rot
Points on a cadence (weak: 5 min, heavy: 2 min, hotspot: 1 min); every
gain rolls one d6 per **total** rot point — each 1 is a point of damage,
so exposure snowballs. In clean rooms they shed 1 point per 10 minutes,
but each shed point has a 10% chance of becoming **permanent** (never
decays, always counts in damage rolls). Rot damage floors at 1 HP.

Mutants gain rot at half rate and take half damage; Rot-Eater mutants
gain none and convert it to MP. The `look` output warns about rotted
rooms; `status`/`score` show a Rot row for any contaminated character.

Use rot levels the way MYZ uses zone sectors: weak rot for ambient
wasteland, heavy rot for valuable scavenging interiors, hotspots as
short, high-stakes dashes guarding the best loot.

## Pieces you have

| Piece | What it does |
|---|---|
| `redit rot <0-3>` / `rot_level` on API/MCP | Contaminate a room |
| `is_pc_mutant(connection_id)` Rhai fn | Gate dialogue/trigger scripts on the race |
| `get_pc_mp` / `get_pc_max_mp` / `change_pc_mp` | Read/adjust the pool from quest scripts |
| `get_pc_mutations` / `pc_has_mutation(conn, id)` | Branch content on a specific power |
| `get_pc_deformities` | Surface misfire scars in dialogue |
| `get_pc_rot` / `get_pc_permanent_rot` | Any race — gate healers, decon services, fanatics |
| `get_mutation_list()` / `get_mutation_info(id)` | Enumerate loaded definitions |
| `init_pc_mutant` / `revoke_pc_mutant` | Admin/quest-driven (re)marking |

## Recipe ideas

- **Decon bathhouse**: a clean (`rot 0`) safe room near a rotted area —
  decay only ticks in rot-free rooms, so "somewhere safe to wait out the
  Rot" is real infrastructure, and the 10% permanence roll keeps it
  tense.
- **Rot-locked vault**: hotspot corridor (`rot 3`) in front of the
  prize. Rot-Eater mutants stroll through and snack; everyone else
  budgets HP or hires one.
- **Zone shaman**: NPC dialogue branching on `pc_has_mutation` — each
  power gets its own greeting; deformities (`get_pc_deformities`) make
  excellent fear/reverence triggers for commoner NPCs.

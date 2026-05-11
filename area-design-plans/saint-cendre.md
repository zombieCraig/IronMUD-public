# Saint-Cendre — Vampire Clan PvP City — Design Plan

## Context

IronMUD has a fully-wired vampire class (theme-agnostic, 5 starter clans, sire-quest system, sun damage, daytime shelter, thinblood progression) but **zero vampire content in the world** — no sire NPCs, no acknowledgment quests, no clan territory. New vampire characters currently have nowhere to go to find a clan and lift their thinblood gates.

This plan designs a **modern-city PvP area** that:
- Serves as the destination where vampire-class players seek out and join one of the five clans
- Welcomes any player (lively/normal during the day, dangerous at night — vampires hunt and PvP after dark)
- Acts as the first published showcase of the vampire content layer (sire NPCs, clan halls, embrace quests)

The plan follows the `ironmud-area-designer` skill's interleaved 6-phase workflow: each phase has a Design step, an optional Build plan + Slice list, and an explicit Definition-of-phase-done. Build slices execute against MCP `ironmud-public` after their plan is approved. This plan is the worked example referenced by the skill.

## Iteration Phases (status)

| Phase | Title | Design | Build plan | Slices |
|---|---|---|---|---|
| 1 | Name + Theme + Anchor | ✅ approved | ✅ approved | ✅ built (2026-05-10) |
| 2 | Cast | ✅ approved | — (pure design) | — |
| 3 | Core Plot | ✅ approved | — (pure design) | — |
| 4 | Seed Quests | ✅ approved | — (pure design) | — |
| 5 | Map + Room Build | ✅ approved | ⏳ drafted, awaiting approval | ⏳ drafted, awaiting approval |
| 6 | Population, Dialogue, Quests | ✅ approved | ⏳ drafted, awaiting approval | ⏳ drafted, awaiting approval |

When you resume work, advance the lowest-numbered "⏳ drafted, awaiting approval" entry first. A slice is ready to execute when both its phase's design AND its phase's build plan show ✅.

## System Hooks We Lean On

Captured up-front so the design respects existing infrastructure (no new systems needed):

- **Area-level**: `combat_zone = Pvp`, `climate = Temperate`, `immigration_enabled = true`, `immigration_vampire_chance ~0.3` so clan presence self-sustains
- **Room-level overrides**: `combat_zone = Safe` for shops/sanctums; `RoomFlags.{indoors, dark, no_magic}` for vampire shelter rooms (rescue from `SunlightBurning` requires this combination)
- **Sire-quest**: `EmbraceClan` quest reward — quest's `giver_mob_vnum` mob becomes the player's sire; lifts thinblood gates and seeds clan trait + first preferred discipline
- **Dialogue gates**: `IsThinblood` / `IsClanAcknowledged` conditions for NPC branching
- **Faction**: `MobileData.faction = "clan_brujah"` etc. so clan members defend each other
- **Mobile presets**: `vampire_goon`, `vampire_elder`, `vampire_hunter`, `town_guard_captain` already exist as templates
- **Day/night danger**: time-gated `daily_routine` on mobiles + DG triggers to flip `aggressive` flag at night (no built-in area-level toggle — this is script work)

**MCP target**: `ironmud-public` (community world), not the local `ironmud` instance.

---

## Phase 1: Name + Theme + Anchor

### Design

**Name:** **Saint-Cendre** ("Saint Ash") — fictional Gulf-coast city, founded ~150 years ago by exiled European Catholic nobility. Pronounced "san-SAHN-druh."

**Setting:** New Orleans atmosphere with old-world European bones. Wrought-iron balconies, narrow stone alleys, above-ground cemeteries, cathedral spires. Hot, humid, perpetually a little decayed. Tourists by day, locals by night, vampires after midnight.

**Two faces of the city:**
- **By day**: lively and walkable. Cafés, antique shops, voodoo curio stalls, the cathedral, the opera house, the riverfront market, mortal guards on patrol. Anyone — including new vampire-class players — can sightsee, shop, and talk to mortal NPCs. PvP is technically possible but rare and conspicuous; mortal witnesses notice (Masquerade pressure, even if unenforced mechanically).
- **By night**: shops shutter, guards retire to the Garrison, mortals thin out. Only the always-dangerous places stay open: gambling dens, blood-trade taverns, side alleys, the catacombs. The Five Clans walk openly. PvP is the same flag — but the *narrative* says nighttime is when things actually happen.

**Mechanical realization (no new systems needed):**
- Area `combat_zone = Pvp`, `climate = Temperate`, `immigration_enabled = true`, `immigration_vampire_chance ~0.3` so clan presence self-sustains
- **Main streets**: `combat_zone = Safe` override — you can walk the Quarter without being stabbed. Mortal day-shop NPCs live here.
- **Side alleys, dens, catacombs, clan havens**: PvP (area default) — always dangerous regardless of time.
- **Shops & guards**: time-gated `daily_routine` — shopkeepers in their stalls 7-19, then home; guards patrol main streets 6-20, then off-shift to the Garrison.
- **Vampire shelters** (clan havens, catacombs): `RoomFlags.{indoors, dark, no_magic}` so vampires caught by sunrise have somewhere to be dragged for `SunlightBurning` recovery.
- **`no_mob` flag**: used on safe-room thresholds (cathedral interior, hotel lobby) to keep aggressive mobs from chasing players inside.

**Setting hook for the plot:** the Camarilla-style truce among the Five Clans is fraying. The Prince's authority is weakening. New vampires arriving in Saint-Cendre are a wild card — every clan wants to claim them; every clan's enemies want to kill them before they're claimed. (Phase 3 expands this.)

**Target scale:** Large (~80-120 rooms total), built incrementally over multiple sessions.

### Build plan for this phase

Create the `Saint-Cendre` area on `ironmud-public` and build the **central anchor**: Place de la Cendre (the central plaza everything radiates from) plus its four arterial chains so subsequent phases have somewhere to attach. Configure area-level immigration with the plaza as the entry vnum.

**Anchor layout (13 rooms, all `combat_zone: Safe` per-room override):**

| Vnum | Short | Notes |
|---|---|---|
| `cendre:plaza` | Place de la Cendre | Wide cathedral square, fountain, wrought-iron lampposts. Mention the cathedral spire (foreshadow). 4 exits (n/s/e/w). |
| `cendre:rue-royale-1` | Rue Royale (lower) | N1. Cobblestones, balconies. Plaza visible behind. |
| `cendre:rue-royale-2` | Rue Royale (middle) | N2. Banker's row hints, brass plaques. |
| `cendre:rue-royale-3` | Rue Royale (upper) | N3. Dead-end stub: "barricaded with sawhorses for the night." Future Bourse entry. |
| `cendre:rue-eau-1` | Rue de l'Eau (upper) | S1. Sloping toward the river, smell of brackish water. |
| `cendre:rue-eau-2` | Rue de l'Eau (middle) | S2. Flickering gas lamps, narrower. |
| `cendre:rue-eau-3` | Rue de l'Eau (lower) | S3. Dead-end stub. Future Riverfront entry. |
| `cendre:rue-cendre-1` | Rue de la Cendre (inner) | E1. Foundry smoke faintly carried on the wind. |
| `cendre:rue-cendre-2` | Rue de la Cendre (middle) | E2. Rougher pavement, jazz piano in the distance. |
| `cendre:rue-cendre-3` | Rue de la Cendre (outer) | E3. Dead-end stub. Future Foundry entry. |
| `cendre:rue-arts-1` | Rue des Beaux-Arts (inner) | W1. Gilded shopfronts, opera-poster fragments. |
| `cendre:rue-arts-2` | Rue des Beaux-Arts (middle) | W2. Quieter, scent of jasmine. |
| `cendre:rue-arts-3` | Rue des Beaux-Arts (outer) | W3. Dead-end stub. Future Conservatory entry. |

Connectivity: `plaza` ↔ first segment of each arterial; each arterial chains inner ↔ middle ↔ outer. Outer ends are dead-ends (no exit to future district yet — those are added in Phase 5).

Area-level config: `combat_zone: Pvp`, `climate: Temperate`, `immigration_enabled: true`, `immigration_room_vnum: cendre:plaza`, `immigration_vampire_chance: 0.3`, `migration_interval_days: 3`, `migration_max_per_check: 2`, name pool + visual profile chosen from `scripts/data/names/` and `scripts/data/visuals/` (likely `generic` + a `human` profile).

### Slices

#### Slice 1.1 — Create area + area-level config
- **Goal**: Bring `Saint-Cendre` into existence on `ironmud-public` with all area-level fields set.
- **Deliverables**:
  - Area record `Saint-Cendre`, prefix `cendre`
  - `combat_zone: Pvp`, `climate: Temperate`
  - Immigration disabled at create; turned on in slice 1.7 once plaza vnum exists (creating the area first lets the plaza creation succeed)
- **MCP calls (sketch)**: `create_area(name="Saint-Cendre", prefix="cendre", combat_zone="Pvp", climate="Temperate")`
- **Done when**: `list_areas` shows `Saint-Cendre`; `get_area` returns the configured fields.

#### Slice 1.2 — Plaza room
- **Goal**: Create Place de la Cendre as the central hub.
- **Deliverables**: Room `cendre:plaza` with atmospheric description (2-3 sentences), `combat_zone: Safe`.
- **MCP calls (sketch)**: `create_room(vnum="cendre:plaza", area="Saint-Cendre", short="Place de la Cendre", desc="...", combat_zone="Safe")`
- **Done when**: `get_room("cendre:plaza")` returns the room with Safe combat zone.

#### Slice 1.3 — North arterial (Rue Royale, 3 rooms)
- **Goal**: Create the three Rue Royale segments toward the future Bourse Quarter.
- **Deliverables**: Rooms `cendre:rue-royale-1`, `cendre:rue-royale-2`, `cendre:rue-royale-3`, all `Safe`. Each with the description sketched in the build plan table.
- **MCP calls (sketch)**: 3× `create_room(...)`
- **Done when**: All three rooms exist and are `Safe`.

#### Slice 1.4 — South arterial (Rue de l'Eau, 3 rooms)
- **Goal**: Create the three Rue de l'Eau segments toward the future Riverfront.
- **Deliverables**: Rooms `cendre:rue-eau-1`, `cendre:rue-eau-2`, `cendre:rue-eau-3`, all `Safe`.
- **MCP calls (sketch)**: 3× `create_room(...)`
- **Done when**: All three rooms exist and are `Safe`.

#### Slice 1.5 — East arterial (Rue de la Cendre, 3 rooms)
- **Goal**: Create the three Rue de la Cendre segments toward the future Foundry.
- **Deliverables**: Rooms `cendre:rue-cendre-1`, `cendre:rue-cendre-2`, `cendre:rue-cendre-3`, all `Safe`.
- **MCP calls (sketch)**: 3× `create_room(...)`
- **Done when**: All three rooms exist and are `Safe`.

#### Slice 1.6 — West arterial (Rue des Beaux-Arts, 3 rooms)
- **Goal**: Create the three Rue des Beaux-Arts segments toward the future Conservatory.
- **Deliverables**: Rooms `cendre:rue-arts-1`, `cendre:rue-arts-2`, `cendre:rue-arts-3`, all `Safe`.
- **MCP calls (sketch)**: 3× `create_room(...)`
- **Done when**: All three rooms exist and are `Safe`.

#### Slice 1.7 — Wire exits + enable immigration
- **Goal**: Connect plaza ↔ each arterial bidirectionally, then turn on immigration with the plaza as the entry vnum.
- **Deliverables**:
  - Bidirectional exits: plaza↔rue-royale-1 (n/s), royale-1↔royale-2 (n/s), royale-2↔royale-3 (n/s); same pattern for the other three chains in s/e/w directions.
  - Area updated: `immigration_enabled: true`, `immigration_room_vnum: "cendre:plaza"`, `immigration_vampire_chance: 0.3`, `migration_interval_days: 3`, `migration_max_per_check: 2`, name + visual pool selected from disk.
- **MCP calls (sketch)**: 12× `set_room_exit(...)` (3 per arterial × 4 arterials = 12 forward edges; `set_room_exit` handles the reverse implicitly if used per-edge twice, otherwise call both directions). Then `update_area("Saint-Cendre", immigration_enabled=true, ...)`.
- **Done when**: `get_room("cendre:plaza")` shows 4 exits; walking from plaza in each direction reaches the outer arterial segment in 3 steps; `get_area("Saint-Cendre")` returns the immigration fields populated.

### Definition of phase done
- All 7 slices' DoDs met.
- In-game `look` from `cendre:plaza` reads atmospheric and shows 4 exits.
- Walking each arterial reads as a coherent street, not three disconnected rooms.
- A migrant can spawn in the plaza on the next migration tick (verified by waiting through one migration interval or via `mcp__ironmud-public__update_area` to force-tick if exposed).

### Phase 1 build log (2026-05-10)
- Area `Saint-Cendre` created (id `dbc32ca0-9b0b-4fe3-a52d-aa567783652a`), all 13 anchor rooms live with per-room `combat_zone: safe` override (via MCP `flags.safe: true`). 24 bidirectional exits wired. Immigration on (`generic` names, `human` visuals, 3-day interval, max 2 per check, entry `cendre:plaza`).
- **Two MCP gaps requiring in-game / out-of-band follow-up:**
  1. **Area `combat_zone` defaulted to `pve`** — MCP `create_area`/`update_area` do not expose the field (api/areas.rs:105 comment). Needs in-game `aedit Saint-Cendre combat_zone pvp` to match the design (Pvp area default + per-room Safe overrides).
  2. **`immigration_vampire_chance` is not exposed** — the `immigration_variation_chances` map only has `guard`/`healer`/`scavenger` (no `vampire` variation registered in `src/migration/variations.rs`). Clan presence will be seeded explicitly in Phase 6 (cast bodies + spawn points) rather than via immigration. If we want self-sustaining vampire migrants later, that's a code-side addition (new variation + chance field + mob template).

---

## Phase 2: Cast

### Design

Cast organized by role. Each entry lists: **purpose**, **key mechanical wiring** (faction, dialogue gates, quest hooks, presets), and **where they live**. Names are placeholders the builder can refine; mechanical wiring is the load-bearing part.

#### A. Authority & Court (the political spine)

- **Prince Évariste Larue** — the Ventrue Prince of Saint-Cendre. Holds the truce together. Lives in **Hôtel de Larue** (private mansion in the Cathedral District). Faction `clan_ventrue`. Speaks French-accented English. Dialogue conditions: `IsClanAcknowledged` to grant audience. Plot anchor for late-game faction quests.
- **Seneschal Mireille Doucet** — the Prince's right hand, Toreador. Public-facing; greets newcomers and refers them to the appropriate clan. Same faction as Prince by alliance. **First NPC most players talk to** in the Quarter at night. Quest-giver for the "find a sire" tutorial quest.
- **Harpy Théo Vasquez** — court gossip / Toreador socialite. Lives in the opera house. Source of rumor-style dialogue exposing all five clans (information hub for new players choosing a clan).

#### B. The Five Sires & Their Districts

One **sire NPC** per clan owns the `EmbraceClan` quest reward — their `vnum` becomes the quest's `giver_mob_vnum`, so when the quest completes the player records *that* NPC as their sire. Each sire has at minimum: faction `clan_<name>`, dialogue tree gated on `IsThinblood` (offers embrace) → `IsClanAcknowledged` (post-embrace dialogue), and lives in the clan's district haven.

| Clan | Sire NPC (placeholder name) | District | Personality / Hook |
|---|---|---|---|
| Brujah | **Antoine "Tony" Rivière** | The Foundry (industrial, jazz halls, fight pits) | Ex-revolutionary, runs an underground boxing gym. Tests recruits in a brawl quest. |
| Toreador | **Lady Yvette Beaumont** | The Conservatory (opera house, art galleries) | Demands an aesthetic offering — recruit must steal/recover a specific artwork. |
| Ventrue | **Magistrate Henri Saint-Clair** | Bourse Quarter (banking, courthouse, gentlemen's club) | Demands proof of discipline — recruit performs a contract task (debt collection, intimidation). |
| Nosferatu | **The Caretaker** (true name lost) | The Catacombs (under cathedral + cemeteries) | Trades information for service. Recruit must retrieve a buried relic from the cemetery. |
| Gangrel | **Ma'tante Solange** | The Bayou's Edge (swamp ward beyond the levee) | Tests recruits with a hunt in the bayou — survive the night, bring back a trophy. |

Each sire also has a **Primogen seat** at the Prince's court (faction note for politics). The Caretaker delegates court appearances to a proxy — Nosferatu Masquerade.

#### C. Clan Support Cast (per district)

Each district gets ~2-3 supporting NPCs so a player visiting "their" district has someone to interact with day-to-day:
- **A ghoul or retainer** (mortal, faction-locked to the clan) — runs a service: doorman, bartender, archivist
- **A rival or initiate** (vampire, same faction) — gives flavor dialogue, sometimes side quests
- **A district-themed mortal** (the Foundry's bookmaker, the Conservatory's stage manager, etc.)

Detail-level (names, exact rooms) comes during per-district build slices in Phase 6.

#### D. Mortal Day-Quarter Cast (the "lively normal city" face)

These NPCs run the visible economy by day and disappear at night via `daily_routine` (work 7-19, then go home/off-shift). They make the Quarter feel like a real place a tourist could visit before realizing what's underneath.

- **Madame Beauchamp** — voodoo curio shop (gris-gris, talismans, fortune for hire — light buff items). Hub for early rumors.
- **M. Lefèvre** — antique dealer specializing in "estate items" (subtext: dead vampires' belongings). Quest hook: appraisal jobs.
- **Sister Agathe** — at the cathedral. Wary of nightwalkers, helpful to newcomers. Provides safe haven (`no_mob` interior).
- **Père Dominique** — Catholic priest. Knows more about the city's underside than he lets on. Potential late-game ally OR antagonist depending on player choices.
- **Café owner, jazz musician, riverfront fishmonger, hotel clerk, opera box attendant, cemetery groundskeeper** — atmosphere NPCs with short dialogue, some sell goods. ~6-10 of these scattered across districts.

#### E. Authority of the Living: City Guard

- **Capitaine Roussel** — head of the Saint-Cendre Garrison. Patrols main streets by day with subordinate guards. Use existing `town_guard_captain` preset. All retire to **the Garrison** at night, leaving the streets to the Clans.
- **~4-6 patrol guards** (also use `town_guard_captain` preset, scaled down via per-mob field overrides if needed) on `daily_routine`: 6-20 patrol the Cathedral Square / Riverfront / Opera Square, then march to the Garrison and stay indoors overnight.

(No separate `guard` base preset exists in the current codebase; `town_guard_captain` is the closest match. Migrant guards from the immigration `guard` variation are a separate, dynamically-spawned population — used opportunistically for street density, not as the static patrol roster.)

#### F. Hidden Threats & Wild Cards

- **Sabbat infiltrator: "the Stranger"** — an unknown vampire breaking the Masquerade. Antagonist for the area's central plot (Phase 3). Doesn't appear in dialogue lists; spotted only via specific quest steps.
- **Vampire hunters (~2-3)** — use existing `vampire_hunter` mobile preset. Patrol cemeteries and back alleys at night (DG trigger or routine). Hostile to anyone with `vampire` flag.
- **Anarch agitator: "Casey Boudreaux"** — young, defected from Brujah, runs a clandestine cell. Argues against the truce. Optional faction path for players who don't want to join an established clan via the standard sire-quest. (May not ship in v1; reserve as a future expansion hook.)

#### G. Information / Service Hubs

- **Madame Olympe** — a fortune teller in the Quarter (mortal seer). The "go here if confused" NPC. Cheap dialogue hints at the next plot beat; thin gameplay layer between phases.
- **The Caretaker** (already listed under Nosferatu sires) doubles as the world's information broker — paid in service, not money.
- **Old Léon** — a barge captain at the riverfront. Transport hub if Saint-Cendre ever connects to other areas (forward-compat hook; not required for v1).

#### H. Mechanical Footprint Summary

- **5 sire NPCs** with `EmbraceClan` quest each → ~5 quests, ~5 multi-node dialogue trees with `IsThinblood`/`IsClanAcknowledged` gates
- **3 court NPCs** (Prince, Seneschal, Harpy)
- **~12-15 clan support NPCs** (2-3 per district)
- **~8-10 mortal day-Quarter NPCs** with time-gated routines
- **6-8 guard mobiles** (Capitaine + patrol, all from `town_guard_captain` preset)
- **~3 wild-card threats** (hunters + Sabbat infiltrator)
- **~2-3 service NPCs** (fortune teller, broker, optional barge captain)

**Estimated total unique mobiles: ~40**, plus migrant spawns from `immigration_vampire_chance`.

### Build plan for this phase
No build this phase; cast bodies are built in Phase 6 once their districts exist (Phase 5).

### Slices
No slices.

### Definition of phase done
Cast design approved. Footprint summary numbers feed Phase 6's slice sizing.

---

## Phase 3: Core Plot

### Design

#### The Question Saint-Cendre Asks

*Can a century-old truce among predators survive a hidden enemy who wants it dead?*

#### The Larue Concord (background)

Prince Évariste Larue has kept the Five Clans at peace for a century via the **Larue Concord** — a binding pact that carved the Vieux-Cendre into five districts, set the Masquerade as inviolable, and made the Prince final arbiter of disputes. The Concord is the only reason a tourist can drink coffee on the cathedral steps without seeing fangs.

The Concord is fraying.

#### The Crisis (current state when a player arrives)

Three vampires have been murdered in the last lunar month, each in ways that violated the Masquerade:
1. A Toreador socialite found at dawn in the cathedral fountain, drained.
2. A Brujah enforcer staked in the courthouse steps with a Ventrue signet pressed into his palm.
3. A Nosferatu emissary's body left in the Bayou with Gangrel claw-marks but no claws would have left them.

Each killing implicates a different clan. Each clan blames the next. Primogen openly defy the Prince at court. The Concord will break within months.

The real culprit is **the Stranger** — a Sabbat agent who entered Saint-Cendre six months ago. The Sabbat want the Camarilla truce shattered and Saint-Cendre opened to their Pack. The Stranger is patient, methodical, and uses Obfuscate to wear different faces. Even the Prince doesn't know they exist yet; the Seneschal has begun to suspect.

#### The Player's Role

A new vampire arriving in Saint-Cendre is, paradoxically, the most useful person in the city: they are not yet bound by clan loyalty, so they can move between districts and listen to all sides. The Seneschal recognizes this immediately.

The **three layers** of plot the player can engage with:

**Layer 1 — Atmosphere (always present, no quest required):**
Rumors in tavern dialogue, fresh graffiti about the murders, a bloodstain in an alley, a flyer for a missing musician, mortals whispering about "the disappearances," a guard captain visibly worried. The city tells you something is wrong before any NPC explicitly says so.

**Layer 2 — Personal (the clan embrace path):**
The player picks a clan and pursues that sire's `EmbraceClan` quest. This is purely personal — the embrace is permanent (one clan only).

**Layer 3 — Meta (the Stranger plotline, optional endgame):**
A separate, parallel investigation track (Q-I1 through Q-I5 in Phase 4) is open to anyone — vampires of any clan, mortals, fresh thinbloods. Each yields one investigation piece. A character with 3+ pieces can bring evidence to the Seneschal and unlock the **Court of the Concord** quest: a final investigation/confrontation that exposes the Stranger at the Prince's court. Reward: city-wide standing (a `cendre_concord_witness` trait), and the area reaches a temporary "calm" state for that player's quest tracking.

#### What Persists vs What Resets

**Per-player state** (in CharacterData / quest log):
- VampireState + clan trait (already wired)
- Investigation pieces collected (use `quest_state` flags or trait-style markers)
- Standing with each clan / the Prince
- "Concord witness" status post-endgame

**World state** (does NOT reset between players):
- The Stranger remains a threat for every new vampire (the area is evergreen — "solving" it for one character doesn't remove the antagonist for everyone). The endgame is a *personal* victory; the city's larger struggle continues. This avoids the classic MUD problem of an area "ending."
- The five sires, the Prince, the Seneschal all reset on area reset like any other mobiles.
- Spawn-tick + `replace_on_respawn` for the Stranger keep the antagonist alive.

#### What About Non-Vampire Players?

Anyone can visit. Mortal-class players have parallel paths:
- **Ghoul service path**: drink vampire vitae from a willing patron (a clan supporting NPC) → temporary stat boost + dialogue access to that clan's haven. Risk: addiction debuff. (Reuses existing buff system.)
- **Hunter path**: speak to Père Dominique at the cathedral; he'll point you toward vampire kills for bounties (no new system — kill-credit quests on `vampire`-flagged mobiles).
- **Tourist path**: shopkeepers, opera, riverfront, voodoo curios, jazz halls — atmosphere content, optional buff items, side dialogue. Players who just want to be in the city.

These paths are lower priority than the vampire flow but the area should support them so non-vampire characters have something to do besides spectate.

#### Tone

Atmospheric, slow-burn noir. NPCs talk in fragments; nobody dumps a quest log on you. Information is currency. The Masquerade is the ambient pressure that makes overt actions costly. The city is beautiful by day on purpose — the contrast is the point.

### Build plan for this phase
No build this phase; plot is the connective spine for Phase 4 quests and Phase 6 dialogue trees.

### Slices
No slices.

### Definition of phase done
Plot design approved.

---

## Phase 4: Seed Quests

### Design

Ship-list for v1: **14 quests** total (1 tutorial, 5 clan embraces, 5 district investigations, 1 endgame, 2 mortal-side).

**Two parallel tracks** — embrace and investigation are decoupled so a single character can reach the endgame solo:
- **Embrace track** (Q2-Q6): only available to unaligned thinbloods. Each is a clan-themed recruitment trial. Picking one is permanent (`clan_<name>` trait), so each character will only ever do *one* of these.
- **Investigation track** (Q-I1 to Q-I5): available to *anyone* — vampire of any clan, mortal, fresh thinblood. Each is a small focused mini-quest tied to one of the murder sites; together they unlock the endgame. A single character can complete all 5.

Each quest entry calls out the **build requirements it surfaces**.

#### Quest 1 — "A Stranger in Saint-Cendre" (tutorial)

- **Giver**: Seneschal Mireille Doucet, at the Hôtel de Larue
- **Trigger**: any vampire-flagged player who walks into the Cathedral District at night (DG room trigger nudges them toward Mireille on first entry; otherwise just talk to her)
- **Steps**: visit all five clan havens, exchange a single line of dialogue with each sire (no commitment). Mireille marks each visit on a quest progress flag.
- **Reward**: a journal item (`une cendre carnet`) — flavor + a `met_all_sires` trait that unlocks the `IsThinblood` embrace dialogue branch on each sire
- **Investigation piece**: none — pure orientation. Sets the stage by revealing the Concord's tension via Mireille's monologue.
- **Build requirements**: Mireille's dialogue tree (one node per district visit), one journal item prototype, room triggers in each haven's entry room.

#### Quests 2-6 — The Five Embrace Quests

Available only to unaligned thinbloods. Each is a focused clan-themed recruitment trial with no investigation content; the sire is testing fit with their clan, nothing more. Reward on every one: `EmbraceClan` (lifts thinblood, grants `clan_<x>` trait, seeds first preferred discipline, sets sire to the giver).

**Q2: "Iron and Blood" — Brujah (Tony Rivière, the Foundry)**
- Steps: prove yourself in the fight pit. Win three matches against escalating Brujah enforcers. No tricks, no Disciplines beyond Potence — Tony wants to see you fight.
- Build: fight pit room + 3 staged enforcer mobs (use `vampire_goon` preset, scaling), fight pit dialogue triggers.

**Q3: "An Aesthetic Offering" — Toreador (Lady Yvette Beaumont, the Conservatory)**
- Steps: recover a specific painting that a defaulting collector took from Lady Yvette and refuses to return. Steal it from his apartment OR negotiate it back via dialogue (two paths).
- Build: collector's apartment (1-2 rooms), painting item, collector mortal NPC with dual-branch dialogue.

**Q4: "A Matter of Discipline" — Ventrue (Magistrate Henri Saint-Clair, the Bourse)**
- Steps: collect on a delinquent ghoul's debt. Intimidation, persuasion, or violence — Henri doesn't care which, only that the books balance by dawn.
- Build: delinquent ghoul mortal NPC + multi-branch dialogue (intimidate / persuade / threaten), debt-marker item.

**Q5: "What the Earth Keeps" — Nosferatu (the Caretaker, the Catacombs)**
- Steps: descend into the catacombs and retrieve a specific relic from a sealed tomb. Combat against minor risen + a small puzzle (find the right tomb among many).
- Build: catacomb level (3-5 rooms branching off cemetery — these double as vampire-shelter rooms with `indoors/dark/no_magic`), the relic item, 1-2 minor risen mobs, the Caretaker's chamber.

**Q6: "The Bayou's Choice" — Gangrel (Ma'tante Solange, Bayou's Edge)**
- Steps: survive a single night alone in the bayou. Kill a designated bayou predator (gator or feral spirit) and bring back a trophy.
- Build: bayou outdoor rooms (4-6 including levee path), predator mob, trophy item.

#### Quests Q-I1 to Q-I5 — Investigation Track (parallel, anyone-eligible)

Each is a small focused mini-quest (typically: talk to a district NPC who witnessed something, follow a 1-2 room clue trail, return). Open to vampires of any clan, mortals, even fresh thinbloods who haven't picked a clan yet. Each yields one **investigation piece** (a quest_state flag on the character). Three pieces unlock the endgame.

**Q-I1: "The Forged Signet" — Foundry investigation**
- Giver: a Brujah-friendly metalworker (mortal) in the Foundry district, guilty about what he made under duress.
- Steps: hear his confession → recover the forged Ventrue signet from his back room.
- Yields: `investigation_signet` (the signet was forged → external framer).
- Build: metalworker mortal NPC + dialogue, forged signet item, hiding-spot container.

**Q-I2: "The Last Aria" — Conservatory investigation**
- Giver: the murdered Toreador's dresser at the opera (mortal, traumatized).
- Steps: hear about the secret meeting → find a torn opera ticket in her dressing room → match the seat number to a guest log at the box office.
- Yields: `investigation_meeting` (victim was meeting someone outside her clan).
- Build: opera dressing room, torn ticket item, box office room + guest log item, dresser mortal NPC.

**Q-I3: "Quiet Accounts" — Bourse investigation**
- Giver: an anxious Ventrue clerk at the bank office.
- Steps: he hands you an audit ledger → spot the irregular foreign deposits → return for the clerk's reaction (he flees the city).
- Yields: `investigation_money` (the conspiracy is funded from outside Saint-Cendre).
- Build: bank office room, audit ledger item, anxious clerk mortal NPC.

**Q-I4: "Wrong Soil" — Catacombs investigation**
- Giver: the Caretaker himself (no Nosferatu embrace required to talk to him about this).
- Steps: examine the Bayou victim's body (moved to the catacombs). Compare the soil under his nails with the bayou earth — they don't match.
- Yields: `investigation_moved_body` (the Gangrel frame is fabricated — body moved post-mortem).
- Build: examination chamber room, victim corpse item, soil sample items (×2).

**Q-I5: "A Scent That Shouldn't Be" — Bayou investigation**
- Giver: a Gangrel scout (vampire NPC, faction `clan_gangrel`) at the Bayou's Edge.
- Steps: track an unfamiliar scent trail through 2-3 bayou rooms → discover the Stranger's shack on the levee (sealed, can't enter without writ).
- Yields: `investigation_safehouse` (Stranger has a hideout; player has located it but cannot breach it yet).
- Build: scout NPC, scent-trail tracking via room descriptions / readable items, Stranger's shack exterior with sealed door.

#### Quest 7 — "Court of the Concord" (endgame)

- **Giver**: Seneschal Mireille
- **Trigger**: player has accumulated **3 or more** investigation pieces from Q-I1 to Q-I5. Solo-reachable on a single character (all 5 investigation quests are open to anyone). Players who do all 5 unlock the optional capture branch in the endgame.
- **Steps**:
  1. Present evidence to Mireille → she gives you a writ to enter the safehouse.
  2. Breach the Stranger's shack on the levee, fight the Stranger (a unique mob using `vampire_elder` preset with Obfuscate + Celerity, faction `sabbat`). Optional: instead of killing, capture (dialogue branch — Stranger surrenders below 25% HP if player has 5 pieces of evidence).
  3. Return to the Prince's Court. Reveal at court (set-piece dialogue with all five Primogen present).
- **Reward**: trait `cendre_concord_witness`, a heirloom item from the Prince, and reset-on-respawn so the Stranger comes back for future players (the world's struggle continues).
- **Build**: safehouse interior (3-4 rooms, secret evidence cache), the Stranger unique mob, the writ item, the heirloom reward item, the Prince's court chamber as a script-set piece (the five Primogen mobs assemble there for this dialogue).

#### Quests 8-9 — Mortal-Side Quests (validate non-vampire content)

**Q8: "Bounty of Saint-Cendre" — Hunter path (Père Dominique, cathedral)**
- Steps: kill 3 vampire-flagged mobs in the cemetery district, return for bounty. Repeatable.
- Reward: gold + a mortal-usable buff item (silver-edged knife with `night_vision` flag for the next hunt).
- Build: kill-credit quest config, silver knife item. No new mobs (uses migrant + spawn-tick vampires).

**Q9: "Madame Beauchamp's Lost Charm" — Tourist path (voodoo curio shop)**
- Steps: a customer ran off with an unpaid gris-gris; track them through 2-3 day-Quarter rooms (light dialogue puzzle), recover the item.
- Reward: a minor luck buff item (`une patte de lapin`).
- Build: the customer mortal NPC, the gris-gris item, ~2 dialogue tree witnesses.

#### Cross-Quest Build Requirements Summary

This summary is the explicit input to Phase 6's slice list. Aggregated build asks across all 14 quests:

- **Items (~16)**: journal, forged signet, painting, debt-marker, relic, bayou trophy, torn ticket, guest log, audit ledger, victim corpse, soil samples ×2, scent-trail clues, writ, heirloom reward, silver knife, gris-gris, lost charm
- **Unique mobiles (~12 beyond the main cast)**: 3 staged Brujah enforcers, the defaulting collector, the delinquent ghoul, the metalworker, the dresser, the anxious clerk, the Gangrel scout, Caretaker's risen ×2, bayou predator, the Stranger
- **Rooms beyond standard districts (~16-20)**: fight pit, collector's apartment (1-2), bank office, catacomb branch (3-5), bayou trail (4-6), metalworker's back room, opera dressing room, box office, examination chamber, Stranger's shack (3-4), Prince's court chamber
- **Dialogue trees**: 6 sire/court trees (Mireille tutorial + 5 sires) + 5 investigation-NPC trees + 2-3 mortal-side trees
- **Per-character quest flags**: 5 investigation pieces + 1 met_all_sires + clan acknowledgment (already wired)

#### Deferred from v1
Anarch quest line, cross-area transport, diablerie/blood-bond, festival/scheduled day events.

### Build plan for this phase
No build this phase; the cross-quest build requirements summary above feeds Phase 6's slice list.

### Slices
No slices.

### Definition of phase done
Quest design approved; cross-quest build requirements summary captured in a form Phase 6 can directly slice.

---

## Phase 5: Map + Room Build

### Design

#### Layout Overview

Saint-Cendre is the **Vieux-Cendre** (Old Quarter) — a dense Old Quarter built around a central plaza (**Place de la Cendre**), with the Mississippi-equivalent **Fleuve Doré** to the south, and the **Bayou-Cendre** wetland southwest beyond the levee. Five clan districts radiate from the central plaza like petals; main streets are safe-zone arterials, alleys are PvP-default. The Bayou is a separate sub-area connected by a single road.

The plaza and four arterials were built in Phase 1; Phase 5 fills in the districts that hang off the outer arterial stubs.

#### ASCII Sketch (district adjacency, not to scale)

```
                   Bourse Qtr
                  (Ventrue, N)
                       |
                  Rue Royale
                       |
   Conservatory --- Place de la --- The Foundry
   (Toreador, W)      Cendre        (Brujah, E)
  Rue des Beaux-   (Cathedral Dist)   Rue de la Cendre
        Arts            |
                  Rue de l'Eau
                       |
                   Riverfront
                  (Fleuve Doré)
                       |
              Catacombs / Cemetery
                 (Nosferatu, S)
                       |
                  Levee Road
                       |
                  Bayou's Edge
                  (Gangrel, SW)
                       :
                Stranger's Shack
                  (sealed, SW)
```

#### Room Budget by District (Phase 5 build, ~85 rooms; total area target ~98 including Phase 1 anchor)

| District / Zone | Rooms | Combat | Notes |
|---|---|---|---|
| **Cathedral District** (extras around plaza) | 9 | Mostly Safe | Cathedral interior (3), Hôtel de Larue / Prince's court (3), opera house entrance (1), courthouse exterior (1), Garrison (1). (Plaza itself built in Phase 1.) |
| **Riverfront** | 6 | Safe (market) + PvP (docks) | Market square, fishmonger, hotel, dock 1-3, Old Léon's barge |
| **The Foundry** (Brujah) | 10 | PvP haven, Safe street front | Foundry exterior, foundry main 3, fight pit, jazz hall 2, Tony's office, metalworker shop, back alley |
| **The Conservatory** (Toreador) | 10 | PvP haven, Safe street front | Opera house interior 4, art gallery 2, dressing room, box office, collector's apartment 2 |
| **Bourse Quarter** (Ventrue) | 8 | PvP haven, Safe street front | Bank exterior, bank interior 2, courthouse interior 2, gentlemen's club 2, Magistrate's chamber |
| **Catacombs / Cemetery** (Nosferatu) | 12 | PvP everywhere | Cemeteries above ground 3, catacomb branch 5, Caretaker's chamber, examination chamber, evidence storage 2 |
| **Bayou's Edge** (Gangrel) | 12 | PvP everywhere | Levee road, bayou trail 6, Solange's hut, levee path 2, Stranger's shack exterior + interior 3 |
| **Day-life / Misc shops** | 8 | Safe | Voodoo curio shop, café, antique dealer, jazz hall (mortal-facing), tourist hotel lobby, fortune teller's nook, two atmospheric storefronts |
| **Side alleys / connective PvP** | 10 | PvP | Inter-district shortcuts, dead-ends with flavor, the alley a victim was found in, etc. |
| **Phase 5 total** | **85** | | (Plus 13 Phase 1 anchor rooms = 98 area total.) |

#### Combat-Zone Rules

- **Area default**: `combat_zone = Pvp` (so unmarked rooms are dangerous by default).
- **Per-room overrides to `Safe`**: every main street arterial (already done in Phase 1), the cathedral interior, the Garrison, mortal shops, the hotel lobby, the Riverfront market.
- **Vampire-shelter rooms** (must allow drag-rescue from `SunlightBurning`): all clan haven interiors, the catacombs, Solange's hut, Tony's office, the gentlemen's club back rooms. Flag with `RoomFlags.{indoors, dark, no_magic}`.
- **`no_mob` flag**: cathedral interior, hotel lobby — keeps aggressive mobs from chasing players inside.

#### Connectivity Highlights

- The **Place de la Cendre** central plaza is the natural meeting point — first room a player enters, where Mireille often appears.
- **Each district** is reached from the plaza via one arterial (built in Phase 1). Each arterial outer segment gets a new exit added in Phase 5 to its district's entry room. Each arterial mid-segment gets 2-3 alleys branching off (alleys are PvP shortcuts to adjacent districts, but dangerous).
- The **Catacombs** are reached from inside the cathedral (a hidden door discoverable via a Nosferatu hint), from an above-ground cemetery, and from beneath the gentlemen's club (a Ventrue secret entrance — both clans can drop in unannounced, building tension).
- The **Bayou** is reached only via the Levee Road from the cemetery district — naturally cordons it off as a hostile sub-zone.
- The **Stranger's shack** has a sealed door that needs the writ from Quest 7. The exterior is reachable via the Bayou (Q-I5 leads you there).

### Build plan for this phase

Slice the Phase 5 build by district. Each district gets one slice that creates all of its rooms, exits within the district, the room flag overrides, and the back-edge from its outer arterial stub (e.g., `cendre:rue-royale-3` gains a `n` exit to the Bourse exterior). Cross-district connective tissue (alleys, hidden doors between catacombs/cathedral/club, Levee Road) is its own slice. Phase 5 ships ~85 rooms across ~9 slices.

### Slices

#### Slice 5.1 — Cathedral District extras (9 rooms)
- **Goal**: Build the cathedral, Hôtel de Larue (Prince's court), opera house entrance, courthouse exterior, Garrison around the existing plaza.
- **Deliverables**: 9 rooms with appropriate `combat_zone` overrides and `no_mob` flag on cathedral interior + Garrison; exits linking them to the plaza.
- **MCP calls (sketch)**: 9× `create_room(...)`, ~12× `set_room_exit(...)`, `update_room(...)` for `no_mob` flag where needed.
- **Done when**: All 9 rooms exist; cathedral interior is `no_mob`; walking from the plaza reaches the cathedral, Hôtel de Larue, and Garrison without leaving the district.

#### Slice 5.2 — Riverfront (6 rooms)
- **Goal**: Build the riverfront market (Safe), three docks (PvP), the hotel (Safe), and Old Léon's barge.
- **Deliverables**: 6 rooms; market + hotel get `Safe` override; docks stay PvP (area default).
- **MCP calls (sketch)**: 6× `create_room(...)`, 7× `set_room_exit(...)` (linking to the south end of `cendre:rue-eau-3`).
- **Done when**: Walking S from the plaza reaches the market in 4 steps and the docks in 5; combat-zone overrides verified.

#### Slice 5.3 — The Foundry (Brujah district, 10 rooms)
- **Goal**: Build the Foundry exterior, three foundry-main rooms, the fight pit, two jazz halls, Tony's office (haven), the metalworker's shop, and the back alley.
- **Deliverables**: 10 rooms; Tony's office gets `RoomFlags.{indoors, dark, no_magic}` (vampire shelter); exteriors stay PvP.
- **MCP calls (sketch)**: 10× `create_room(...)`, exits, `update_room(...)` for shelter flags + Safe override on the street-front room.
- **Done when**: The arterial outer stub `cendre:rue-cendre-3` gains an east exit to the Foundry exterior; all 10 rooms reachable; shelter flags verified on Tony's office.

#### Slice 5.4 — The Conservatory (Toreador district, 10 rooms)
- **Goal**: Build the opera house interior (4 rooms), art gallery (2), dressing room, box office, and collector's apartment (2).
- **Deliverables**: 10 rooms; opera + galleries get `Safe` override on the public-facing rooms; dressing room is haven-flagged.
- **MCP calls (sketch)**: 10× `create_room(...)`, exits, `update_room(...)` for shelter + Safe overrides.
- **Done when**: `cendre:rue-arts-3` gains a west exit; all 10 rooms reachable; combat-zone + shelter flags verified.

#### Slice 5.5 — Bourse Quarter (Ventrue district, 8 rooms)
- **Goal**: Build the bank (3 rooms), courthouse interior (2), gentlemen's club (2), Magistrate's chamber.
- **Deliverables**: 8 rooms; gentlemen's club back rooms get shelter flags; bank exterior is `Safe`.
- **MCP calls (sketch)**: 8× `create_room(...)`, exits, `update_room(...)` for flags.
- **Done when**: `cendre:rue-royale-3` gains a north exit; 8 rooms reachable; shelter + Safe flags verified.

#### Slice 5.6 — Catacombs / Cemetery (Nosferatu district, 12 rooms)
- **Goal**: Build the above-ground cemeteries (3), catacomb branch (5), Caretaker's chamber, examination chamber, evidence storage (2).
- **Deliverables**: 12 rooms; all catacomb rooms get `RoomFlags.{indoors, dark, no_magic}` (shelter-eligible).
- **MCP calls (sketch)**: 12× `create_room(...)`, exits within district + linkage from cathedral interior (hidden door, deferred) and from Riverfront, `update_room(...)` for shelter flags.
- **Done when**: Cemeteries reachable from Riverfront; catacombs reachable from cemeteries; shelter flags applied across catacomb rooms.

#### Slice 5.7 — Bayou's Edge + Stranger's Shack (Gangrel district + endgame zone, 12 rooms)
- **Goal**: Build the levee road, bayou trail (6 rooms), Solange's hut (haven), levee path (2), Stranger's shack exterior + interior (3 rooms with sealed door).
- **Deliverables**: 12 rooms; Solange's hut is shelter-flagged; Stranger's shack exterior has a sealed door (locked, requires writ — door wired but writ-check in Phase 6 quest).
- **MCP calls (sketch)**: 12× `create_room(...)`, exits, `add_room_door(...)` for the Stranger's shack sealed door, `update_room(...)` for shelter flags.
- **Done when**: Bayou reachable from cemetery district via Levee Road; sealed door on shack exists and is locked; Solange's hut shelter flags verified.

#### Slice 5.8 — Day-life / Misc shops (8 rooms)
- **Goal**: Build the voodoo curio shop, café, antique dealer, mortal jazz hall, tourist hotel lobby, fortune teller's nook, and two atmospheric storefronts.
- **Deliverables**: 8 rooms; all `Safe`; hotel lobby gets `no_mob`.
- **MCP calls (sketch)**: 8× `create_room(...)`, exits attaching them to plaza-adjacent or arterial-adjacent locations, `update_room(...)` for flags.
- **Done when**: All 8 rooms reachable from plaza in ≤3 steps; flags verified.

#### Slice 5.9 — Side alleys + connective PvP rooms (10 rooms)
- **Goal**: Build inter-district alleys as PvP shortcuts, plus a few flavor dead-ends (the alley where a victim was found, etc.).
- **Deliverables**: 10 rooms; all PvP (area default, no override); exits cutting between districts.
- **MCP calls (sketch)**: 10× `create_room(...)`, ~15× `set_room_exit(...)` for shortcut connections.
- **Done when**: Alleys exist as expected shortcuts; combat-zone is PvP throughout; map walk from one district to an adjacent one via alley is shorter than via plaza.

### Definition of phase done
- All 9 slices' DoDs met; total room count for the area reaches ~98.
- Walking from plaza reaches every district within the documented step count.
- Combat-zone overrides verified across the area (sample-check ~10 rooms via `get_room`).
- Shelter flag combo verified on every haven and the catacombs.
- `no_mob` verified on cathedral interior, hotel lobby, Garrison.

---

## Phase 6: Population, Dialogue, Quests

### Design

This phase brings the cast (Phase 2), plot (Phase 3), and quests (Phase 4) into the rooms (Phase 5). Per-NPC dialogue tree node-name sketches and per-quest implementation notes are captured in the slice blocks below. The smoke-test playthrough script is the final slice.

#### Per-NPC dialogue tree sketches (high level — full text composed in slices)

- **Mireille (Seneschal)**: greeting → orientation monologue (Concord background, the murders) → "go meet the sires" → per-haven-visit acknowledgment node → quest 1 completion → post-Q1 evidence-presentation branch (gated on `investigation_*` flags ≥ 3).
- **Each sire**: greeting (clan-specific tone) → `IsThinblood` branch (offer trial quest Q2-Q6) → trial-progress nodes → `IsClanAcknowledged` branch (post-embrace dialogue, mentor flavor).
- **Investigation-quest givers** (metalworker, dresser, clerk, Caretaker-investigation-branch, Gangrel scout): each is a focused 3-5 node tree (greeting → confession/evidence → handover → followup).
- **Mortal day-cast**: short 2-3 node trees, mostly atmospheric.

#### Smoke-test playthrough script

Run after the last build slice ships:
1. Connect to `ironmud-public`. Roll a fresh vampire-class character (becomes thinblood).
2. Walk into Saint-Cendre via the Place de la Cendre. Verify Seneschal Mireille is present at night and gives Q1.
3. Visit each of the five clan districts. Confirm each sire NPC is reachable and has dialogue gated on `IsThinblood`.
4. Pick one clan; complete its embrace quest (Q2-Q6). Verify on completion: clan trait granted, blood pool 6→10, sire ID set, first preferred discipline seeded.
5. Confirm the now-acknowledged player can no longer trigger the embrace dialogue branches on the other four sires (clan exclusivity).
6. With a *different* character (mortal or vampire of any clan), complete Q-I1 through Q-I5. Verify each yields its `investigation_*` flag and that 3+ flags unlock Q7 from Mireille.
7. Complete Q7 endgame: writ → safehouse breach → Stranger fight → court reveal. Verify `cendre_concord_witness` trait + heirloom item granted, and Stranger respawns on next area reset.
8. **Day-life check**: Visit during in-game day. Confirm shops are open and staffed; guards are patrolling main streets; sires are in their havens (not on the streets).
9. **Night-life check**: Visit at night. Confirm shops are shuttered (NPCs in residential alcoves); guards are at the Garrison; vampire migrants and clan vampires are walking their districts.
10. **Sun damage rescue**: Take a vampire character outdoors at sunrise. Verify `SunlightBurning` triggers, and that being dragged into a `indoors/dark/no_magic` haven clears it on the next tick.
11. **Mortal-side**: Run Q8 (hunter bounty) and Q9 (lost charm) on a mortal-class character. Confirm rewards.
12. **PvP sanity**: Confirm main-street arterials reject `attack <player>` (Safe override) while alleys allow it (PvP area default).

**Atmospheric checks** (subjective but important):
- Walking the Quarter by day should feel like a city break, not a dungeon.
- Walking the Quarter by night should feel risky without being uniformly hostile.
- Each clan district should have a distinct silhouette of NPCs and items even before quests start.

### Build plan for this phase

Slice ordering follows dependency: cast bodies → daily routines → dialogue trees → quest configs → spawn points. The smoke-test script is the final slice — a verification deliverable, not a write, that gates "phase done."

### Slices

#### Slice 6.1 — Court + Seneschal cast bodies
- **Goal**: Create the three court NPCs (Prince Larue, Seneschal Mireille, Harpy Théo) as mob prototypes with factions, place them in their rooms, attach daily_routines.
- **Deliverables**: 3 mob prototypes (faction `clan_ventrue` for Prince + Mireille's Toreador-aligned-with-Ventrue; faction `clan_toreador` for Théo); placed in Hôtel de Larue, plaza-adjacent presence for Mireille, opera house for Théo; routines wiring their day/night locations.
- **MCP calls (sketch)**: 3× `create_mobile(...)`, 3× `create_spawn_point(...)`, 3× `add_mobile_routine(...)`.
- **Done when**: All three mobs spawn at next area reset; Mireille appears in Cathedral District at night.

#### Slice 6.2 — Five sires (cast bodies, dialogue deferred)
- **Goal**: Create the five sire NPCs as mob prototypes with clan factions, place them in their havens.
- **Deliverables**: 5 mob prototypes (Tony, Yvette, Henri, Caretaker, Solange) with factions `clan_brujah` / `clan_toreador` / `clan_ventrue` / `clan_nosferatu` / `clan_gangrel`; spawn points in their respective havens.
- **MCP calls (sketch)**: 5× `create_mobile(...)`, 5× `create_spawn_point(...)`. No dialogue trees yet.
- **Done when**: All 5 sires spawn in their havens at next area reset.

#### Slice 6.3 — Clan support cast (one slice per district = 5 sub-slices, optionally bundled)
- **Goal**: Create the ~12-15 clan support NPCs (ghouls, retainers, rivals, themed mortals) across all five districts.
- **Deliverables**: ~12-15 mob prototypes with appropriate factions and spawn points; some get short routines (e.g. bartender behind the bar).
- **MCP calls (sketch)**: ~12× `create_mobile(...)`, ~12× `create_spawn_point(...)`, ~5× `add_mobile_routine(...)`.
- **Done when**: Each clan haven has 2-3 supporting NPCs visible at appropriate times.

#### Slice 6.4 — Mortal day-cast + guards
- **Goal**: Create the ~8-10 mortal day-Quarter NPCs + 6-8 guards (Capitaine + patrol), all with `daily_routine` 7-19 work / off-shift to home or Garrison.
- **Deliverables**: ~16 mob prototypes; each with a routine entry for work hours and a different room for off-shift; guards use `town_guard_captain` preset.
- **MCP calls (sketch)**: ~16× `create_mobile(...)`, ~16× `create_spawn_point(...)`, ~16× `add_mobile_routine(...)`, `apply_mobile_preset(...)` for guards.
- **Done when**: Day-time tour shows shops staffed and guards patrolling; night-time tour shows shops empty and guards at Garrison.

#### Slice 6.5 — Hidden threats
- **Goal**: Create vampire hunters (~2-3, `vampire_hunter` preset) and the Stranger (unique mob, `vampire_elder` preset with Obfuscate/Celerity, faction `sabbat`).
- **Deliverables**: ~3 hunter mobs with night-only patrol routines; 1 unique Stranger mob with `replace_on_respawn` so the antagonist persists for future players.
- **MCP calls (sketch)**: 3-4× `create_mobile(...)`, `apply_mobile_preset(...)`, `create_spawn_point(..., replace_on_respawn=true)` for the Stranger; routines for hunters.
- **Done when**: Hunters appear in cemetery/alley rooms at night; Stranger spawns in safehouse interior on next reset.

#### Slice 6.6 — Mireille tutorial dialogue + Q1
- **Goal**: Wire Mireille's full dialogue tree (greeting → orientation → per-haven-visit progress → completion) and create Q1 ("A Stranger in Saint-Cendre") with the journal item reward.
- **Deliverables**: Mireille dialogue tree (~8-10 nodes); journal item prototype `une cendre carnet`; Q1 quest with `met_all_sires` trait reward; room triggers in each haven entry that mark the per-haven progress flag.
- **MCP calls (sketch)**: `add_mobile_dialogue(...)` + several `add_mobile_dialogue_node(...)` + `add_mobile_dialogue_choice(...)`, `create_item(...)`, `create_quest(...)`, 5× `add_room_trigger(...)`.
- **Done when**: A fresh thinblood walking into Cathedral District at night gets nudged toward Mireille; Q1 accepts; visiting all 5 havens completes it; reward grants.

#### Slices 6.7-6.11 — Embrace quests + sire dialogues (one per clan)
- **Goal**: For each of the five sires, wire the full dialogue tree and the embrace quest.
- **Deliverables (per slice)**: Sire dialogue tree (~8-12 nodes with `IsThinblood` and `IsClanAcknowledged` gates); embrace quest with `EmbraceClan` reward; clan-specific build artifacts (fight-pit enforcer mobs for Q2; collector + apartment + painting for Q3; ghoul + debt-marker for Q4; relic + risen mobs for Q5; bayou predator + trophy for Q6).
- **MCP calls (sketch)**: Per slice: dialogue calls + `create_quest(...)` + `create_mobile(...)` for clan-specific NPCs + `create_item(...)` for clan-specific items.
- **Done when (per slice)**: Picking that clan's embrace quest as a thinblood, completing it, and observing clan trait + sire ID assignment.

#### Slices 6.12-6.16 — Investigation quests (one per quest)
- **Goal**: Wire the five investigation NPCs and quests (Q-I1 through Q-I5).
- **Deliverables (per slice)**: Investigation NPC dialogue (~4-6 nodes); quest with `investigation_<piece>` flag reward; supporting items (forged signet, torn ticket, audit ledger, soil samples, scent-trail clues, etc.).
- **MCP calls (sketch)**: Per slice: `create_mobile(...)` + dialogue + `create_quest(...)` + 1-3 `create_item(...)`.
- **Done when (per slice)**: Quest accepts on any character (vampire of any clan, mortal, thinblood); completion grants the right `investigation_*` flag.

#### Slice 6.17 — Endgame Q7 (Court of the Concord)
- **Goal**: Wire the Stranger fight, the writ item, the safehouse evidence cache, and the Prince's court reveal set-piece.
- **Deliverables**: Q7 quest (gated on ≥3 investigation flags); writ item; heirloom reward item; `cendre_concord_witness` trait reward; court-chamber set-piece dialogue (Prince + Mireille + 5 Primogen present); Stranger surrender branch (gated on 5 investigation flags).
- **MCP calls (sketch)**: `create_quest(...)`, 2× `create_item(...)`, dialogue updates on Mireille (evidence-presentation branch) + Prince (court reveal), DG triggers for the set-piece assembly.
- **Done when**: A character with 3 flags can present evidence, get the writ, breach the shack, kill (or capture, with 5) the Stranger, and trigger the court reveal; Stranger respawns on next area reset.

#### Slice 6.18 — Mortal-side Q8 + Q9
- **Goal**: Wire the hunter-bounty (Q8) and lost-charm (Q9) quests for non-vampire characters.
- **Deliverables**: Q8 (kill-credit on vampire-flagged mobs in cemetery, repeatable, silver-knife reward); Q9 (track customer through 2-3 day-Quarter rooms, recover gris-gris, luck buff reward); supporting NPCs and items.
- **MCP calls (sketch)**: 2× `create_quest(...)`, 1-2× `create_mobile(...)`, 2-3× `create_item(...)`, dialogue for the customer.
- **Done when**: A mortal character can complete Q8 (verify silver knife reward + repeatability) and Q9 (verify gris-gris reward).

#### Slice 6.19 — Smoke-test verification playthrough
- **Goal**: Execute the 12-step playthrough script captured in the Design section above. Fix anything that fails before declaring v1 shipped.
- **Deliverables**: A short "smoke test pass" note appended to this plan with date and pass/fail per step.
- **MCP calls (sketch)**: None directly; this is in-game observation. Use `get_*` MCP tools to confirm state changes (quest acceptance, traits granted, items in inventory).
- **Done when**: All 12 numbered steps pass, plus the three atmospheric checks read as intended.

### Definition of phase done
- All 19 slices' DoDs met.
- Smoke-test playthrough completes end-to-end on a fresh character.
- Saint-Cendre is v1-complete and ready for players.

---

## MCP Tools Used Across the Build

`create_area`, `create_room`, `create_mobile`, `create_item`, `create_quest`, `add_room_door`, `set_room_exit`, `add_mobile_dialogue`, `add_mobile_dialogue_node`, `add_mobile_dialogue_choice`, `add_mobile_routine`, `apply_mobile_preset`, `create_spawn_point`, `update_area`, `update_room`, `add_room_trigger`. All on the `ironmud-public` MCP.

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
| 2 | Cast | ✅ deep-dive approved (2026-05-10) | ✅ approved | ✅ built (2026-05-10) |
| 3 | Core Plot | ✅ approved · ✅ deep-dive approved (2026-05-10) | — (pure design) | — |
| 4 | Seed Quests | ✅ approved · ✅ deep-dive approved (2026-05-10) | — (pure design) | — |
| 5 | Map + Room Build | ✅ approved · ✅ deep-dive approved (2026-05-10) | ✅ approved | ✅ built (2026-05-10) |
| 6 | Population, Dialogue, Quests | ✅ approved | ✅ approved | ⏳ in progress — 6.0 ✅, 6.1 ✅, 6.2 ✅, 6.3 ✅, 6.4 ✅, 6.5 ⚠️, 6.6 ✅, 6.7 ✅, 6.8 ✅, 6.9 ✅, 6.10 ✅ (2026-05-11); 6.11–6.19 pending |

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

#### Slice 1.8 — Portal-alley entrance (post-hoc add)
- **Goal**: Hidden arrival room with a glowing portal-window on the north wall, looking through to a dark cave. North exit is reserved for the portal target (left unwired until the cave-source room is built in a future task). Soft landing for first-time arrivals.
- **Deliverables**: Room `cendre:portal-alley` "A Disused Alley" with `flags.no_mob: true` and `flags.combat_zone: safe`. Description ends with the load-bearing portal sentence (north-wall amber pane onto damp cave, cold air leaking through). Bidirectional east/west exit to `cendre:rue-cendre-2` — the alley sits off the rough middle stretch of Rue de la Cendre, deliberately attached to a side direction (not the plaza) to stay hidden from business traffic.
- **MCP calls (sketch)**: 1× `create_room`, 2× `set_room_exit` (`cendre:rue-cendre-2 west ↔ cendre:portal-alley east`).
- **Done when**: `get_room("cendre:portal-alley")` shows the room with `no_mob: true`, `combat_zone: safe`, east exit pointing at rue-cendre-2, north exit `null` (intentional). `get_room("cendre:rue-cendre-2")` shows a west exit pointing at the alley.

### Definition of phase done
- All 7 slices' DoDs met.
- In-game `look` from `cendre:plaza` reads atmospheric and shows 4 exits.
- Walking each arterial reads as a coherent street, not three disconnected rooms.
- A migrant can spawn in the plaza on the next migration tick (verified by waiting through one migration interval or via `mcp__ironmud-public__update_area` to force-tick if exposed).

### Phase 1 build log (2026-05-10)
- Area `Saint-Cendre` created (id `dbc32ca0-9b0b-4fe3-a52d-aa567783652a`), all 13 anchor rooms live with per-room `combat_zone: safe` override (via MCP `flags.safe: true`). 24 bidirectional exits wired. Immigration on (`generic` names, `human` visuals, 3-day interval, max 2 per check, entry `cendre:plaza`).
- Area `combat_zone` set to `pvp` via MCP (now exposed on `update_area` after commit `c9e12c6` + server redeploy).
- `immigration_vampire_chance: 0.3` applied via MCP — clan migrants will now self-sustain at the design's intended rate alongside the Phase 6 explicit cast placements.
- Slice 1.8 added: portal-alley entrance room (`cendre:portal-alley`) with `no_mob` + `safe` flags, east-wired into `cendre:rue-cendre-2`. North exit reserved for the portal target — left unwired pending the cave-source room (future task). Anchor total now 14 rooms.

---

## Phase 2: Cast

### Design

Deep-dive catalog (no deferrals). 45 named NPC prototypes total. Each row carries the mechanical wiring needed for MCP `create_mobile` plus the Phase 6 hooks (dialogue, routine, quest). Sire `name` fields are the *exact* strings the `EmbraceClan` reward will record as the player's sire — they are not placeholders. Faction strings are free-form; clan tags create per-clan defense pools (mobs with `clan_brujah` defend each other but not `clan_ventrue`).

#### A. Court & Authority (3)

| vnum | Name | Faction | Preset | District home (Phase 5) | Short desc | Role hook |
|---|---|---|---|---|---|---|
| `cendre:prince-larue` | Prince Évariste Larue | `clan_ventrue` | `vampire_elder` + override | Hôtel de Larue | a tall, austere man in a charcoal three-piece suit | Audience gated on `IsClanAcknowledged` + Q1 done. Formal French-accented English. |
| `cendre:seneschal-mireille` | Seneschal Mireille Doucet | `clan_toreador` | `vampire_elder` + override | Plaza at night / Hôtel by day | a woman in a tailored emerald coat watches the plaza | First-NPC greeter, Q1 giver, Q7 evidence-presentation branch (≥3 investigation flags). |
| `cendre:harpy-theo` | Harpy Théo Vasquez | `clan_toreador` | `vampire_elder` + override | Opera house bar | a slender man in a midnight-blue dinner jacket leans against the bar | Rumor hub exposing all 5 clans for new-player clan choice. |

#### B. Five Sires (5)

Each sire owns one `EmbraceClan` quest. Their `vnum` becomes the quest's `giver_mob_vnum`; on completion the player's `sire` field is set to the sire's `name`.

| vnum | Name | Clan | Faction | Preset | District home | Embrace quest |
|---|---|---|---|---|---|---|
| `cendre:sire-brujah` | Antoine "Tony" Rivière | Brujah | `clan_brujah` | `vampire_elder` + override | The Foundry — Tony's office | "Iron and Blood" — 3 fight-pit wins (Potence allowed, no other disciplines) |
| `cendre:sire-toreador` | Lady Yvette Beaumont | Toreador | `clan_toreador` | `vampire_elder` + override | Conservatory — owner's box | "An Aesthetic Offering" — recover stolen painting (steal or negotiate) |
| `cendre:sire-ventrue` | Magistrate Henri Saint-Clair | Ventrue | `clan_ventrue` | `vampire_elder` + override | Bourse — Magistrate's chamber | "A Matter of Discipline" — collect a delinquent debt |
| `cendre:sire-nosferatu` | The Caretaker | Nosferatu | `clan_nosferatu` | `vampire_elder` + override | Catacombs — Caretaker's chamber | "What the Earth Keeps" — retrieve sealed-tomb relic |
| `cendre:sire-gangrel` | Ma'tante Solange | Gangrel | `clan_gangrel` | `vampire_elder` + override | Bayou's Edge — Solange's hut | "The Bayou's Choice" — survive a night, kill a predator |

#### C. Clan Support Cast (15 — 3 per district)

Per-district template: 1 mortal ghoul/retainer (faction-locked), 1 vampire initiate (`vampire_goon`, faction-locked), 1 district-themed mortal (no faction).

| vnum | Name | District | Faction | Preset | Role |
|---|---|---|---|---|---|
| `cendre:foundry-beau` | Beau | Foundry | `clan_brujah` | none (mortal) | Jazz hall bartender (ghoul) |
| `cendre:foundry-marisol` | Marisol | Foundry | `clan_brujah` | `vampire_goon` | Brujah initiate, sparring partner |
| `cendre:foundry-bones` | "Bones" Fontaine | Foundry | none | none (mortal) | Bookmaker at the fight pit |
| `cendre:conservatory-etienne` | Étienne | Conservatory | `clan_toreador` | none (mortal) | Opera stage manager (ghoul) |
| `cendre:conservatory-cassandra` | Cassandra Vaughn | Conservatory | `clan_toreador` | `vampire_goon` | Toreador initiate, gallery owner |
| `cendre:conservatory-aldo` | Aldo | Conservatory | none | none (mortal) | Jazz pianist (atmosphere) |
| `cendre:bourse-pierre` | Pierre Doré | Bourse | `clan_ventrue` | none (mortal) | Bank teller (ghoul) |
| `cendre:bourse-lucien` | Lucien Ardent | Bourse | `clan_ventrue` | `vampire_goon` | Ventrue initiate, club steward |
| `cendre:bourse-clerk` | Émeric the Clerk | Bourse | none | none (mortal) | Courthouse clerk |
| `cendre:catacomb-acolyte` | The Acolyte | Catacombs | `clan_nosferatu` | none (mortal) | Tends candles for Caretaker (ghoul) |
| `cendre:catacomb-ribcage` | "Ribcage" Joubert | Catacombs | `clan_nosferatu` | `vampire_goon` | Nosferatu scout |
| `cendre:catacomb-sexton` | The Sexton | Catacombs | none | none (mortal) | Cemetery groundskeeper |
| `cendre:bayou-andre` | Big Andre | Bayou's Edge | `clan_gangrel` | none (mortal) | Bayou guide (ghoul) |
| `cendre:bayou-coyote` | Coyote | Bayou's Edge | `clan_gangrel` | `vampire_goon` | Gangrel initiate, scout |
| `cendre:bayou-fisherman` | Old Thibodeaux | Bayou's Edge | none | none (mortal) | Cajun fisherman |

#### D. Mortal Day-Quarter Cast (8)

All mortal, no preset (default lvl 1). `daily_routine` (7-19 work / off-shift home) lands in Phase 6.

| vnum | Name | Phase 5 location | Hook |
|---|---|---|---|
| `cendre:mortal-beauchamp` | Madame Beauchamp | Voodoo curio shop | Rumor hub; Q9 lost-charm quest giver |
| `cendre:mortal-lefevre` | M. Lefèvre | Antique dealer | "Estate items" subtext; appraisal jobs |
| `cendre:mortal-agathe` | Sister Agathe | Cathedral interior | Wary of nightwalkers; safe haven |
| `cendre:mortal-pere-dominique` | Père Dominique | Cathedral | Q8 hunter-bounty quest giver |
| `cendre:mortal-cafe-henri` | Henri Aubert | Café | Coffee shop owner, atmosphere |
| `cendre:mortal-fishmonger` | Boudreaux | Riverfront market | Sells fish |
| `cendre:mortal-hotel-clerk` | Beatrice Moreau | Tourist hotel lobby | Tourist info |
| `cendre:mortal-opera-attendant` | Marcellin | Opera house entrance | Front-of-house |

#### E. City Guard (7)

All use `town_guard_captain` preset. Faction stays preset default (`town_watch`). Phase 6 wires daily_routine: 6-20 patrol the beat, then to Garrison.

| vnum | Name | Patrol beat |
|---|---|---|
| `cendre:guard-roussel` | Capitaine Roussel | Cathedral District (HQ at Garrison) |
| `cendre:guard-picard` | Sergent Picard | Rue Royale → Bourse approach |
| `cendre:guard-vincent` | Caporal Vincent | Rue de l'Eau → Riverfront approach |
| `cendre:guard-lambert` | Caporal Marie Lambert | Rue des Beaux-Arts → Conservatory approach |
| `cendre:guard-tisserand` | Caporal Émile Tisserand | Rue de la Cendre → Foundry approach |
| `cendre:guard-renaud` | Patrolman Renaud | Cathedral plaza rover |
| `cendre:guard-cormier` | Patrolman Cormier | Riverfront market rover |

#### F. Hidden Threats & Wild Cards (5)

| vnum | Name | Faction | Preset | Location | Notes |
|---|---|---|---|---|---|
| `cendre:threat-stranger` | The Stranger | `sabbat` | `vampire_elder` + override | Stranger's shack interior | Unique. Q7 endgame target. Spawn point in Phase 6 uses `replace_on_respawn: true`. |
| `cendre:threat-hunter-coyle` | Hunter Coyle | `vampire_hunters` (preset default) | `vampire_hunter` | Cemetery patrol | Night-only |
| `cendre:threat-hunter-brennan` | Hunter Brennan | `vampire_hunters` (preset default) | `vampire_hunter` | Backstreets patrol | Night-only |
| `cendre:threat-hunter-voss` | Hunter Voss | `vampire_hunters` (preset default) | `vampire_hunter` | Bayou-edge patrol | Targets Gangrel |
| `cendre:threat-casey-anarch` | Casey Boudreaux | `anarch_unbound` | `vampire_goon` | Hidden cellar off Rue de la Cendre | NO DEFERRAL. Phase 6 dialogue offers clanless alternative path. |

#### G. Service / Information Hubs (2 unique; Caretaker doubles from B)

| vnum | Name | Faction | Preset | Location | Hook |
|---|---|---|---|---|---|
| `cendre:service-olympe` | Madame Olympe | none | none (mortal) | Fortune teller's nook | "Go here if confused" plot-beat hints |
| `cendre:service-leon` | Old Léon | none | none (mortal) | His barge at the riverfront docks | NO DEFERRAL. Barge captain — transport stub for future inter-area linkage |

#### H. Mechanical footprint

- **45 named prototypes**: A=3, B=5, C=15, D=8, E=7, F=5, G=2
- **22 vampires** (need `IsThinblood`/`IsClanAcknowledged` dialogue gates in Phase 6)
- **16 mortals** (no preset, default lvl 1)
- **7 guards** (single preset, shared routine pattern)
- 5 `EmbraceClan` quests (one per sire) attach to vnums `cendre:sire-{brujah,toreador,ventrue,nosferatu,gangrel}` in Phase 6
- Plus migrant arrivals via `immigration_vampire_chance: 0.3` (live since Phase 1.7 update)

### Build plan for this phase

8 slices creating 45 `create_mobile` prototypes against `ironmud-public`. **No spawn points, no dialogue trees, no daily routines** — those are Phase 6. Prototypes live as templates until Phase 5 ships their rooms.

Per-mob spec: `name`, `vnum`, `short_desc`, `long_desc` are required. For vampire NPCs using `vampire_elder` preset: call `create_mobile` first, then `apply_mobile_preset`, then `update_mobile` to override `faction` (preset bakes in `camarilla`). Mortals get no preset; default level 1. Guards get `town_guard_captain` via `apply_mobile_preset` and keep the preset's `town_watch` faction.

### Slices

#### Slice 2.1 — Court & Authority (3)
- **Goal**: Create the three court NPCs as elder vampires with clan-specific factions.
- **Deliverables**: `cendre:prince-larue`, `cendre:seneschal-mireille`, `cendre:harpy-theo`. Each: `create_mobile` → `apply_mobile_preset vampire_elder` → `update_mobile` to override `faction`.
- **Done when**: `get_mobile` on each returns the correct `name`, `faction` (not `camarilla`), `level: 18`.

#### Slice 2.2 — Five Sires (5)
- **Goal**: Five clan sires with faction overrides — these are the load-bearing NPCs for the embrace track.
- **Deliverables**: `cendre:sire-{brujah,toreador,ventrue,nosferatu,gangrel}`. Same create+preset+override pattern as 2.1.
- **Done when**: `get_mobile` on each returns the in-fiction full name (e.g. "Antoine \"Tony\" Rivière" exactly, since this string becomes the player's `sire`).

#### Slice 2.3 — Clan Support: Foundry + Conservatory (6)
- **Goal**: 3 NPCs per district for the first two clan districts.
- **Deliverables**: `cendre:foundry-{beau,marisol,bones}`, `cendre:conservatory-{etienne,cassandra,aldo}`. Ghouls/mortals get no preset; initiates get `vampire_goon` via `apply_mobile_preset` (faction override needed since `vampire_goon` has no preset-set faction).
- **Done when**: 6 prototypes exist with correct factions and levels.

#### Slice 2.4 — Clan Support: Bourse + Catacombs + Bayou (9)
- **Goal**: 3 NPCs per district for the remaining three clan districts.
- **Deliverables**: `cendre:bourse-{pierre,lucien,clerk}`, `cendre:catacomb-{acolyte,ribcage,sexton}`, `cendre:bayou-{andre,coyote,fisherman}`. Same pattern as 2.3.
- **Done when**: 9 prototypes exist with correct factions.

#### Slice 2.5 — Mortal Day-Cast (8)
- **Goal**: 8 mortal NPCs that run the visible day economy.
- **Deliverables**: `cendre:mortal-{beauchamp,lefevre,agathe,pere-dominique,cafe-henri,fishmonger,hotel-clerk,opera-attendant}`. No preset, no faction.
- **Done when**: 8 prototypes exist, all level 1, no faction.

#### Slice 2.6 — City Guard (7)
- **Goal**: 7 patrol guards using `town_guard_captain` preset.
- **Deliverables**: `cendre:guard-{roussel,picard,vincent,lambert,tisserand,renaud,cormier}`. `create_mobile` → `apply_mobile_preset town_guard_captain`. Keep preset's `town_watch` faction.
- **Done when**: 7 prototypes exist, all level 8, faction `town_watch`.

#### Slice 2.7 — Hidden Threats (5)
- **Goal**: Antagonist roster.
- **Deliverables**: `cendre:threat-stranger` (vampire_elder, faction override → `sabbat`), `cendre:threat-hunter-{coyle,brennan,voss}` (vampire_hunter, default faction), `cendre:threat-casey-anarch` (vampire_goon, faction override → `anarch_unbound`).
- **Done when**: 5 prototypes exist with correct factions per row.

#### Slice 2.8 — Service Hubs (2)
- **Goal**: Two unique mortal service NPCs.
- **Deliverables**: `cendre:service-olympe`, `cendre:service-leon`. No preset, no faction.
- **Done when**: 2 prototypes exist, level 1.

### Definition of phase done

- All 8 slices' DoDs met.
- `list_mobile_prototypes_summary` filtered to `cendre` prefix returns 45 entries (the 13 anchor rooms remain on the room side; mobs are a separate namespace).
- Spot-check via `get_mobile`:
  - `cendre:sire-brujah` — `name: "Antoine \"Tony\" Rivière"`, `faction: "clan_brujah"`, `level: 18`.
  - `cendre:seneschal-mireille` — `faction: "clan_toreador"`, `level: 18`.
  - `cendre:guard-roussel` — `faction: "town_watch"`, `level: 8`.
  - `cendre:mortal-beauchamp` — no preset, no faction, `level: 1`.
  - `cendre:threat-stranger` — `faction: "sabbat"`, `level: 18`.

Spawn points, dialogue trees, daily routines, and quest configs all wait for Phase 6 (per skill phase-ordering).

### Phase 2 build log (2026-05-10)
- All 8 slices shipped. 45 prototypes live on `ironmud-public` under `cendre:` prefix (verified via `list_mobile_prototypes_summary`).
- Breakdown matches catalog: 3 court + 5 sires (all lvl 18 elders with clan factions) + 15 support (5 vampire_goon initiates lvl 6, 10 mortals lvl 1) + 8 day-cast mortals + 7 guards (town_guard_captain preset, lvl 8, faction `town_watch`) + 5 threats (Stranger lvl 18 faction `sabbat` with `unique` flag, 3 hunters lvl 10, Casey lvl 6 faction `anarch_unbound`) + 2 service mortals.
- **Discovered**: `vampire_elder` preset DOES override faction to `camarilla` — confirmed by responses. Pattern locked in: create → preset → update faction. Documented for Phase 6 onboarding.
- **Discovered**: `vampire_goon` preset adds `aggressive: true`. Cleared on all 5 initiates + Casey since they're social roles, not random monsters. `vampire_hunter` preset does NOT add aggressive — left as-is on the 3 hunters; their hostility to vampires will need to come from Phase 6 dialogue/trigger work.
- **Prince Larue note**: a stray `</long_desc>` in my first call corrupted his long_desc field; fixed via `update_mobile`. Going forward, avoid literal close-tag-like text inside parameter values.
- No spawn points placed yet — prototypes are templates only. Phase 5 (rooms) and Phase 6 (spawn points + dialogue + routines + quests) will bring them into the world.

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

---

### Deep dive (2026-05-10)

The sections below extend the high-level plot above into a vnum-keyed wiring spec. They reconcile three frame gaps with Phase 4: (a) 3 murders vs 5 investigation quests → 5 investigations are 5 angles on 3 murders + 1 hideout discovery; (b) "five Primogen" at the endgame court → Primogen = the 5 sires; (c) Casey Boudreaux and the 3 hunters' positions in the plot. Every Layer-1/2/3 narrative beat traces back to a specific NPC vnum from the Phase 2 cast catalog.

#### 3.A The Three Murders (named, attributed)

1. **Mathilde Roux** — Toreador soprano, ~5 weeks pre-arrival.
   - Found at dawn in the cathedral fountain, drained.
   - Frame: Ventrue (she had a financial dispute with Magistrate Henri).
   - True: Stranger lured her to a "secret meeting" using Obfuscate to wear Henri's face + a forged Ventrue signet as bona fides. Killed her under cover, dumped the body in the fountain at first light.
   - Discovery witness: `cendre:mortal-agathe` (Sister Agathe — found body at matins).
   - Investigation pieces this murder feeds: Q-I1 (forged signet, recovered in Foundry) + Q-I2 (her dresser remembers the meeting).

2. **Marcel "Iron Marcel" Lacombe** — Brujah enforcer, ~3 weeks pre-arrival. (Distinct from Mathilde — Roux/Lacombe are separate surnames. Phase 3 currently says "Brujah enforcer staked in the courthouse steps with a Ventrue signet"; named here.)
   - Staked on the courthouse steps with a forged Ventrue signet pressed into his palm — the *same forge* as Mathilde's framing signet, which is the player's first cross-murder pattern.
   - Frame: Ventrue (the staking is overtly clan-coded).
   - True: Stranger followed Marcel to a back-alley payoff drop, killed him with a Ventrue-style stake (made days earlier and aged with mud).
   - Discovery witness: `cendre:guard-vincent` (Caporal Vincent, on courthouse patrol that night).
   - Investigation pieces this murder feeds: Q-I3 (foreign-source funds in the Bourse ledger explain who's bankrolling the forger).

3. **Gris-de-fer** — Nosferatu emissary, the Caretaker's longest-serving warden. ~1 week pre-arrival.
   - Found in the bayou with gangrel claw-marks that "no actual claws would have left" (too clean, wrong angle).
   - Frame: Gangrel — specifically Ma'tante Solange, who had a century-old feud with Gris-de-fer.
   - True: Stranger killed him in the catacombs, moved the body to the bayou pre-dawn.
   - Discovery witness: `cendre:bayou-andre` (Big Andre, found the body on a dawn run).
   - Investigation pieces this murder feeds: Q-I4 (Caretaker's soil analysis proves body was moved) + Q-I5 (scent trail from where the body was actually killed leads back to Stranger's safehouse).

#### 3.B Rumor & Atmosphere Mesh (per-NPC Layer ownership)

Canonical "who says what" for Phase 6 dialogue tree default branches. Columns:
- **L1 (atmosphere)**: short ambient line, no gates. The mob's say-trigger or default greeting.
- **L2 (clan/embrace)**: branch for unaligned thinbloods; steers toward (or away from) a clan.
- **L3 (investigation)**: branch behind `IsClanAcknowledged` OR an `investigation_*` flag OR `met_all_sires`.

Each of the 45 NPCs has at minimum a Layer 1 line. ~22 have Layer 2; ~15 have Layer 3. Dashes (—) mean the layer is intentionally empty for that NPC.

**A. Court & Authority**

| NPC vnum | L1 | L2 | L3 |
|---|---|---|---|
| `cendre:prince-larue` | "The Concord holds. It must hold." | — | "Your service has not gone unnoticed." → post-Q7: "Saint-Cendre owes you a debt that will not be forgotten." |
| `cendre:seneschal-mireille` | (first-meet orientation monologue, Q1 trigger) | "The sires await. Choose well." | Q1 giver; Q7 evidence-presentation branch behind ≥3 investigation flags |
| `cendre:harpy-theo` | (5-clan rumor hub — existing Phase 2 hook) | "Which clan calls to you?" | "You should be asking who *gains* from this." (entry to Layer 3) |

**B. Five Sires**

| NPC vnum | L1 | L2 | L3 |
|---|---|---|---|
| `cendre:sire-brujah` (Tony) | "Talk's cheap. Show me." | Q2 offer (Iron and Blood) | "Iron Marcel was mine. Whoever did this answers to me." |
| `cendre:sire-toreador` (Yvette) | "Beauty endures. Everything else burns." | Q3 offer (An Aesthetic Offering) | "Mathilde was my closest friend. I will not forget who took her." |
| `cendre:sire-ventrue` (Henri) | "Discipline is the difference between predator and parasite." | Q4 offer (A Matter of Discipline) | "Someone has forged my seal. I want them found." |
| `cendre:sire-nosferatu` (Caretaker) | "Down here, things keep." | Q5 offer (What the Earth Keeps) | Q-I4 giver — "The soil tells a different story than the body." |
| `cendre:sire-gangrel` (Solange) | "City vampires forget what they are." | Q6 offer (The Bayou's Choice) | "They want to blame me for Gris-de-fer. I want them dead." |

**C. Clan Support — Foundry**

| NPC vnum | L1 | L2 | L3 |
|---|---|---|---|
| `cendre:foundry-beau` | "Drinks are on the house if you're with Tony." | "Tony likes new blood. Stand up straight when you meet him." | "Marcel drank here every Friday. Now there's an empty stool." |
| `cendre:foundry-marisol` | "Stay sharp." | "Sparring's at midnight. Come if you mean it." | "Marcel wasn't supposed to be at the courthouse. Somebody set him up." |
| `cendre:foundry-bones` | "Iron Marcel was good for the books. Now he's wood through the chest." | "Tony wants brawlers, not strategists." | (Q-I1 lead) "There was a signet on him. The forge mark was off. Talk to the metalworker on Rue Forge." |

**C. Clan Support — Conservatory**

| NPC vnum | L1 | L2 | L3 |
|---|---|---|---|
| `cendre:conservatory-etienne` | "The opera goes on. It always does." | "Lady Yvette appreciates patience. Don't push." | "Mathilde missed her cue the night she died. She never missed cues." |
| `cendre:conservatory-cassandra` | "Mind where your eyes land." | "Yvette is selective. So am I." | "The dresser saw who she was meeting. Ask gently." |
| `cendre:conservatory-aldo` | "Mathilde sang 'La Vie en Rose' like she meant every word." | "Yvette is taking applicants. Few." | (Q-I2 lead) "She was meeting someone the night she died. The dresser knows." |

**C. Clan Support — Bourse**

| NPC vnum | L1 | L2 | L3 |
|---|---|---|---|
| `cendre:bourse-pierre` | "Numbers don't lie. People do." | "Magistrate Saint-Clair values discretion above all." | "The audit's been odd lately. Émeric's been sleeping at his desk." |
| `cendre:bourse-lucien` | "Time is money. Don't waste either." | "Henri tests for self-discipline. Most fail." | "Someone's forging seals. Whoever it is, they have access I shouldn't have." |
| `cendre:bourse-clerk` (Émeric) | "Books are off. Nobody wants to talk about it." | — | (Q-I3 giver) "Foreign deposits, weekly, untraceable. See for yourself." |

**C. Clan Support — Catacombs**

| NPC vnum | L1 | L2 | L3 |
|---|---|---|---|
| `cendre:catacomb-acolyte` | "The Caretaker is at his work. Speak softly." | "Few are called below. Be patient." | "Gris-de-fer is here. The Caretaker examined him personally." |
| `cendre:catacomb-ribcage` | "Don't look at me too long." | "We see what the others miss." | "Gris-de-fer wasn't killed where he was found. The Caretaker can prove it." |
| `cendre:catacomb-sexton` | "Gris-de-fer wouldn't have died in a bayou. He hated water." | — | "The Caretaker examined the body himself. Ask him about the soil." |

**C. Clan Support — Bayou**

| NPC vnum | L1 | L2 | L3 |
|---|---|---|---|
| `cendre:bayou-andre` | "I found him. I don't want to talk about it." | "Solange chooses her own. Ain't no one's call but hers." | (Q-I5 lead) "The claw marks were wrong. There's a smell on the levee that doesn't belong. Coyote can track it." |
| `cendre:bayou-coyote` | "Bayou tells you things if you listen." | "Solange picks for herself. I just keep the watch." | (Q-I5 giver) "Come with me. Something doesn't smell right out by the levee." |
| `cendre:bayou-fisherman` (Old Thibodeaux) | "Catfish don't ask questions. Maybe I shouldn't either." | — | "Saw lights at the old shack two nights ago. Wasn't no fisherman." |

**D. Mortal Day-Cast**

| NPC vnum | L1 | L2 | L3 |
|---|---|---|---|
| `cendre:mortal-beauchamp` | "Spirits been restless lately." | — | (Q9 giver) "A girl ran off with my gris-gris. Help me find her." |
| `cendre:mortal-lefevre` | "Estate items come and go. I don't ask where from." | — | — |
| `cendre:mortal-agathe` | "I found her at matins. Mathilde. I will not speak of it again." | — | "Père Dominique knows more than I about... such things." |
| `cendre:mortal-pere-dominique` | "The Lord's work is never done. Especially of late." | — | (Q8 giver) "If you would do the Lord's work — bring me proof of three slain night-walkers." |
| `cendre:mortal-cafe-henri` | "Folks have been disappearing, but the Prince says it's nothing." | — | — |
| `cendre:mortal-fishmonger` (Boudreaux) | "Fresh catfish! Get 'em while they're cold!" | — | — |
| `cendre:mortal-hotel-clerk` (Beatrice) | "Welcome to Saint-Cendre. Most folks find it... memorable." | — | (post Act-3 trigger) "Someone was in your room. Didn't take anything. Left a note. I'm sorry, monsieur." |
| `cendre:mortal-opera-attendant` (Marcellin) | "Tonight's program is sold out, hélas." | — | — |

**E. City Guard**

| NPC vnum | L1 | L2 | L3 |
|---|---|---|---|
| `cendre:guard-roussel` | "Stay off the side streets after dark." | — | (Q7 court testimony if captured branch) "I lost good men to this. Whoever's behind it, I want them." |
| `cendre:guard-picard` | "Rue Royale's clear. For now." | — | — |
| `cendre:guard-vincent` | "I patrolled the courthouse that night. I found him at dawn. I don't sleep right since." | — | "He was already dead when I got there. Whoever did it knew our shift change." |
| `cendre:guard-lambert` | "Move along, citoyen." | — | — |
| `cendre:guard-tisserand` | "Rue de la Cendre at this hour? Be quick about it." | — | — |
| `cendre:guard-renaud` | "Mind the plaza after sundown." | — | — |
| `cendre:guard-cormier` | "Riverfront's quiet. Suspiciously so." | — | — |

**F. Hidden Threats**

| NPC vnum | L1 | L2 | L3 |
|---|---|---|---|
| `cendre:threat-stranger` | (only speakable post-Q7 step 2 in shack, or captured at court) "You came alone? Bold." | — | (Q7 set-piece) contemptuous reveal lines, capture-branch silence |
| `cendre:threat-hunter-coyle` | "I smell rot." | — | — |
| `cendre:threat-hunter-brennan` | "Sun rises on us all eventually, leech." | — | — |
| `cendre:threat-hunter-voss` | "The bayou's a graveyard. Most don't know yet." | — | — |
| `cendre:threat-casey-anarch` | (post-discovery only) "Quiet. Door behind you. Quick." | (post-met_all_sires + no Qn started) "There's another way. Stay thinblood, stay free." | "Concord's the only thing keeping this city from open war. Someone wants the war." |

**G. Service Hubs**

| NPC vnum | L1 | L2 | L3 |
|---|---|---|---|
| `cendre:service-olympe` | "Sit. The cards will tell us what you've forgotten." | "All five paths can be walked. Only one chooses you." | (post any `investigation_*`) "You stand at a crossing. The shape of it is bigger than you yet know." |
| `cendre:service-leon` | "Barge ain't going nowhere this season. Storm's coming." | — | "Strangers ride upriver too. Lot of new faces this past year." |

Counts: 45 rows. Layer 1 = 45 (all). Layer 2 = 22 (3 court + 5 sires + 9 clan-support vampires + Olympe + Casey + 3 ghouls in Foundry/Conservatory + others). Layer 3 = 19 NPCs carry an investigation-tier line. (Slightly above the ~15 target estimate.)

#### 3.C The Stranger — MO, Timeline, Discoverability

- **Cover identity**: arrived in Saint-Cendre ~6 months pre-player as a traveling Caitiff seeking sanctuary. Mireille granted minor blood rights; she now half-suspects, half-regrets. (This is the breadcrumb that lets the Q7 endgame work — Mireille has Concord-violating priors she'd prefer not to surface.)
- **Operating base**: the levee shack on the bayou's edge. Phase 5 builds this in the Bayou's Edge district; player discovers it via Q-I5 but cannot enter without the writ from Q7 step 1.
- **Disciplines**: Obfuscate (face-changing — explains framing) + Celerity (explains the clean strikes). Phase 6 combat config attaches these as combat_spells.
- **Kill schedule**: every ~2 weeks, in the 2-3 hours before dawn, in a different district. A player who acquires 3+ investigation pieces will notice the cycle is overdue — the next kill should have happened by now. (Explanation: Stranger has paused operations after sensing investigation; Q-I5's scent trail and Casey's L3 hint both confirm this.)
- **Awareness of the player**: after the player recovers their **third** investigation piece, the Stranger plants a warning note in the player's room at the tourist hotel (`cendre:mortal-hotel-clerk`'s building — Phase 5 builds a player flop room as a safe haven). DG room trigger in Phase 6, not new mechanics.
- **Endgame personality**: cold, contemptuous, sees the Camarilla as decadent. If captured (Q7 branch with all 5 investigation pieces), reveals nothing useful — just smirks at the Prince. The capture matters for the `cendre_concord_witness` trait flavor, not for unlocking further content.

#### 3.D Casey Boudreaux — Anarch Wild Card Position

- **Bio**: ~30 years embraced. Sired by an Anarch outside the Camarilla. Slid into Saint-Cendre 4 years ago without the Prince's permission — technically a Concord violation. Survives by hiding in a cellar off Rue de la Cendre (Phase 5 builds this as a single optional room with a `hidden` exit from the Foundry approach).
- **Belief about the murders**: agrees they're real (not Camarilla-fabricated) but believes the Concord itself is the problem — that any structure that depends on a Prince's monopoly on violence will eventually breed an enemy that breaks it. The Stranger is, in Casey's view, the inevitable consequence.
- **Player offer**: a **clanless path**. If player has `met_all_sires` AND has not started any of Q2-Q6, Casey offers a Phase-4-deferred quest "The Unsigned Pact." Reward: `anarch_unbound` trait, one preferred discipline of player choice, sire field set to a sentinel string (e.g., "Anarch Unbound"). The existing `EmbraceClan` quest reward mechanism may need a sibling for this; **flagged as a Phase 4 expansion candidate, not in v1 ship**.
- **Plot function for Layer 3**: Casey is the explicit "name the antagonist" hint source. Without Casey, a player can deduce the Stranger from clues alone; with Casey, the player gets one line of explicit framing ("Someone wants the war.") that turns ambient suspicion into a concrete target.
- **Discovery**: the hidden cellar room is unreachable without a search prompt. Phase 6 wires a `search` trigger in the Foundry's adjacent alley. Player must spend a `met_all_sires` interaction or a Q-I track piece to learn the hint exists.

#### 3.E The Three Hunters — Independent, Not a Resolution Path

- Coyle, Brennan, Voss are NOT part of the Stranger plot. They're opportunistic hunters drawn by Père Dominique's bounty postings (Q8) and the general rise of supernatural activity the murders have caused.
- Hostile to all vampires regardless of clan or faction. They will not negotiate, cannot be recruited against the Stranger, will not appear at the endgame court.
- Phase 6 wires night-only spawns (`daily_routine`) and aggression triggers vs `vampire`-flagged characters.
- Plot function: pure Layer 1. Their presence is what makes Saint-Cendre dangerous at night for new vampires and motivates the player to seek a clan haven. They are also Q8's atmospheric backdrop (Père Dominique is paying mortals to do what the hunters already do for ideology).

#### 3.F Court of the Concord — Endgame Scene Cast (Q7)

**Primogen = the 5 sires.** Court scene cast = 8 NPCs + optional Stranger:

| Role | vnum |
|---|---|
| The Prince | `cendre:prince-larue` |
| Seneschal (moderator, evidence-presenter) | `cendre:seneschal-mireille` |
| Harpy (officiator, calls speakers) | `cendre:harpy-theo` |
| Brujah Primogen | `cendre:sire-brujah` |
| Toreador Primogen | `cendre:sire-toreador` |
| Ventrue Primogen | `cendre:sire-ventrue` |
| Nosferatu Primogen | `cendre:sire-nosferatu` |
| Gangrel Primogen | `cendre:sire-gangrel` |
| (Optional) The Stranger, captured | `cendre:threat-stranger` |

- **Venue**: a new room `cendre:court-chamber` inside the Hôtel de Larue. Phase 5 builds this as part of the Hôtel district. Marked `combat_zone: Safe` for the scene's duration via a DG room flag flip.
- **Mechanism**: a one-time DG trigger fires when the player enters the chamber with the writ + ≥3 investigation pieces. Trigger summons the 8 NPCs via `db.move_mobile_to_room`. Phase 6 writes the trigger script and the set-piece dialogue tree. Branch points:
  - 3 pieces → minimum-evidence reveal; Stranger named but escapes off-screen (canonical for re-spawn-on-respawn).
  - 4 pieces → fuller reveal; Stranger named with method.
  - 5 pieces → full reveal AND capture branch is offered (player chose capture vs kill earlier in the shack).
- **Resolution**: trait `cendre_concord_witness` on player; heirloom item from the Prince; spawn-tick + `replace_on_respawn: true` on the Stranger prototype keeps the world's struggle going for future players.

#### 3.G Three-Act Player Experience Arc

For Phase 6 dialogue pacing. Each act is a cognitive state, not a hard quest gate.

- **Act 1 — Arrival to Q1 done.** Orientation. Player has met Mireille, visited all 5 sires (no commitment), heard Théo's clan rumors. Knows the Concord exists, knows people are dying, has no theory of who. NPCs default to Layer 1.
- **Act 2 — Clan picked OR first investigation piece.** Player commits to a clan and/or stumbles into an investigation. Layer 2 dialogue activates for the chosen clan's NPCs. Layer 3 dialogue activates for any NPC tied to a piece the player has collected. Investigations are independent — picking Brujah doesn't block Toreador-district investigations.
- **Act 3 — 3+ investigation pieces.** Endgame opens. Stranger plants the warning note (DG trigger). Mireille's Q7 evidence-presentation branch unlocks. Other NPCs' Layer 3 lines escalate ("It's gone too far, isn't it?"). Casey (if discovered) names the antagonist explicitly. Player executes Q7.

Embrace and investigation paths are independent — a character can do all 5 investigations as a thinblood and reach the endgame without ever picking a clan. Casey's clanless path (if Phase 4 expansion ships) is the canonical "thinblood concord witness" route.

#### 3.H Standing / Reputation System

**Defer to post-v1.** Saint-Cendre v1 does NOT introduce numeric standing. State the player carries:
- Faction tag (after embrace): `clan_brujah` etc., affects mob defense pools.
- Traits: `met_all_sires`, `cendre_concord_witness`, `anarch_unbound` (only if Casey's expansion ships).
- Quest flags: `investigation_signet`, `investigation_meeting`, `investigation_money`, `investigation_moved_body`, `investigation_safehouse`.
- Clan-locked dialogue: gated on faction tag and on `IsClanAcknowledged`. Out-of-clan NPCs default to neutral, not hostile — except Anarch Casey (gated separately) and the hunters (always hostile to vampires).

Phase 6 should NOT add traits beyond this list.

#### 3.I Out of Scope for the Deep Dive

- Writing actual dialogue lines (Phase 6 — the table in 3.B captures the *intent* per NPC, not the final wording).
- Adding new NPCs (the 45-mob cast is locked).
- Adjusting the 14 quest specs in Phase 4 (those are approved; this deep dive uses them as fixed inputs and only **flags** Casey's clanless-path as a Phase 4 expansion candidate without modifying Phase 4).
- Building any rooms, items, or DG triggers (Phase 5 + 6).
- Numeric reputation / standing system.

### Build plan for this phase
No build this phase; plot is the connective spine for Phase 4 quests and Phase 6 dialogue trees.

### Slices
No slices.

### Definition of phase done
Plot design approved.

### Phase 3 deep-dive log (2026-05-10)
- Subsections 3.A–3.I appended after the Tone paragraph; the original high-level Phase 3 design (Question, Concord, Crisis, Stranger, Player Role, Persistence, Non-vampire paths, Tone) stays intact as the narrative frame.
- **Murder victims named**: Mathilde Roux (Toreador), Iron Marcel Lacombe (Brujah), Gris-de-fer (Nosferatu). Each gets a discovery-witness NPC vnum from the Phase 2 catalog: `cendre:mortal-agathe`, `cendre:guard-vincent`, `cendre:bayou-andre`.
- **3-murder vs 5-investigation reconciliation locked**: Q-I1+Q-I2 → Mathilde; Q-I3 → Marcel funding chain; Q-I4+Q-I5 → Gris-de-fer + Stranger hideout.
- **Rumor mesh** (3.B) covers all 45 NPCs — 45 L1 lines, 22 L2 lines, 19 L3 lines. Each Phase-4 quest giver/lead is wired to the canonical NPC vnum.
- **Court of the Concord cast frozen at 8** (Prince + Mireille + Théo + 5 sires) + optional captured Stranger. New room `cendre:court-chamber` added to Phase 5 build asks (Hôtel de Larue district).
- **Casey Boudreaux positioned** as the explicit Layer-3 "name the antagonist" source via a `met_all_sires`-gated cellar discovery. The clanless "Unsigned Pact" path is **flagged as a Phase 4 expansion candidate**, not in v1.
- **Three hunters confirmed non-plot**: Layer-1 atmosphere + Q8 backdrop only, never recruited or court-summoned.
- **Standing system deferred to post-v1**. Per-character state for v1 = faction tag + 3 traits + 5 investigation flags. Phase 6 should not introduce more.
- Status table row updated: Phase 3 now shows "✅ approved · ✅ deep-dive approved (2026-05-10)".
- Phase 4 (Seed Quests) is unchanged — the deep dive uses its 14 quests as fixed inputs.

---

## Phase 4: Seed Quests

### Design

Ship-list for v1: **15 quests** total (1 tutorial, 5 clan embraces, 5 district investigations, 1 endgame, 2 mortal-side, 1 anarch).

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
Cross-area transport, diablerie/blood-bond, festival/scheduled day events. (Anarch quest line promoted to Q10 in v1 per the deep-dive review below.)

---

### Deep dive (2026-05-10)

The sections below extend Phase 4 into a `QuestData`-ready spec. They (a) reconcile every "NEW unique mobile" against the Phase 2 cast + Phase 3 mesh, (b) assign canonical vnums for every NPC / item / room the quests touch, (c) map each quest to `src/types/quests.rs` enum tags, and (d) flag the 6 code prereqs Phase 6 must land before all 15 quests work. Phase 4 ships pure design — none of this is built here.

#### 4.A NPC Reconciliation (with Phase 2 + Phase 3 mesh)

**Reused from the existing 45-cast (no new prototypes needed):**

| Quest role | Resolved vnum | Notes |
|---|---|---|
| Q-I3 giver | `cendre:bourse-clerk` (Émeric) | Phase 2 has him as "courthouse clerk"; reconciled in-fiction as courthouse-clerk-by-day, private-bourse-auditor-evenings. Phase 6 wires the bank-office dialogue node on his tree. |
| Q-I4 giver | `cendre:sire-nosferatu` (the Caretaker) | Dialogue branch bypasses `IsClanAcknowledged` for this thread. |
| Q-I5 giver | `cendre:bayou-coyote` | Phase 3.B already assigned this; Phase 4's "Gangrel scout (NEW)" is folded into Coyote. |
| Q7 antagonist | `cendre:threat-stranger` | Already in cast. |
| Q8 giver | `cendre:mortal-pere-dominique` | Already in cast. |
| Q9 giver | `cendre:mortal-beauchamp` | Already in cast. |
| Q10 giver | `cendre:threat-casey-anarch` | Already in cast. |

**NEW NPC prototypes needed (8 — cast total 45 → 53):**

| vnum | Quest | Role | Preset / faction |
|---|---|---|---|
| `cendre:foundry-metalworker` | Q-I1 | Brujah-friendly mortal who forged the signet under duress | mortal, no preset, no faction |
| `cendre:foundry-enforcer` | Q2 | Brujah fight-pit opponent (escalating levels via spawn instances) | `vampire_goon` preset, faction `clan_brujah`, `world_max_count: 3` |
| `cendre:conservatory-dresser` | Q-I2 | Mathilde's traumatized dresser | mortal, no preset, no faction |
| `cendre:conservatory-collector` | Q3 | Defaulting collector hoarding the painting | mortal, no preset, no faction |
| `cendre:bourse-debtor` | Q4 | Delinquent ghoul ducking Henri's debt | mortal ghoul, no preset, faction `clan_ventrue` |
| `cendre:catacomb-risen` | Q5 | Minor undead in the catacomb branch | no preset, ~level 5, `world_max_count: 2`, no faction |
| `cendre:bayou-predator-gator` | Q6 | The bayou's designated predator | no preset, beast stats (~level 8), no faction |
| `cendre:mortal-thief-customer` | Q9 | Customer who fled with the unpaid gris-gris | mortal, no preset, no faction |

#### 4.B Per-Quest Mechanical Spec (`QuestData`-ready)

Each quest gets a canonical `cendre:q-<key>` vnum. Objective and reward types match `src/types/quests.rs` enum tags. Cells marked **(P6 code task #N)** flag mechanics that require new schema/handler work — see §4.E.

| Quest vnum | Name | Giver | Prereq | Objectives | Rewards |
|---|---|---|---|---|---|
| `cendre:q-tutorial` | A Stranger in Saint-Cendre | `cendre:seneschal-mireille` | none | 5× VisitRoom: `cendre:foundry-office`, `cendre:conservatory-box`, `cendre:bourse-chamber`, `cendre:catacombs-chamber`, `cendre:bayou-hut` | Item `cendre:item-journal` ×1, Achievement `cendre_met_all_sires` |
| `cendre:q-embrace-brujah` | Iron and Blood | `cendre:sire-brujah` | `cendre:q-tutorial` | KillMob `cendre:foundry-enforcer` ×3 | EmbraceClan { clan: "brujah" } |
| `cendre:q-embrace-toreador` | An Aesthetic Offering | `cendre:sire-toreador` | `cendre:q-tutorial` | BringItem `cendre:item-painting` ×1, return_to `cendre:sire-toreador` | EmbraceClan { clan: "toreador" } |
| `cendre:q-embrace-ventrue` | A Matter of Discipline | `cendre:sire-ventrue` | `cendre:q-tutorial` | BringItem `cendre:item-debt-marker` ×1, return_to `cendre:sire-ventrue` | EmbraceClan { clan: "ventrue" } |
| `cendre:q-embrace-nosferatu` | What the Earth Keeps | `cendre:sire-nosferatu` | `cendre:q-tutorial` | BringItem `cendre:item-relic` ×1, return_to `cendre:sire-nosferatu` | EmbraceClan { clan: "nosferatu" } |
| `cendre:q-embrace-gangrel` | The Bayou's Choice | `cendre:sire-gangrel` | `cendre:q-tutorial` | KillMob `cendre:bayou-predator-gator` ×1, BringItem `cendre:item-bayou-trophy` ×1 return_to `cendre:sire-gangrel` | EmbraceClan { clan: "gangrel" } |
| `cendre:q-i1-signet` | The Forged Signet | `cendre:foundry-metalworker` | none | BringItem `cendre:item-signet-forged` ×1 (no return_to — completes via dialogue) | Achievement `cendre_investigation_signet` |
| `cendre:q-i2-aria` | The Last Aria | `cendre:conservatory-dresser` | none | BringItem `cendre:item-opera-ticket` ×1, VisitRoom `cendre:conservatory-box-office`, BringItem `cendre:item-guest-log` ×1 | Achievement `cendre_investigation_meeting` |
| `cendre:q-i3-accounts` | Quiet Accounts | `cendre:bourse-clerk` | none | BringItem `cendre:item-audit-ledger` ×1, return_to `cendre:bourse-clerk` | Achievement `cendre_investigation_money` |
| `cendre:q-i4-soil` | Wrong Soil | `cendre:sire-nosferatu` | none | BringItem `cendre:item-soil-bayou` ×1, BringItem `cendre:item-soil-catacomb` ×1, VisitRoom `cendre:catacombs-exam-chamber` | Achievement `cendre_investigation_moved_body` |
| `cendre:q-i5-scent` | A Scent That Shouldn't Be | `cendre:bayou-coyote` | none | 4× VisitRoom: `cendre:bayou-trail-1`, `cendre:bayou-trail-2`, `cendre:bayou-trail-3`, `cendre:bayou-shack-exterior` | Achievement `cendre_investigation_safehouse` |
| `cendre:q-endgame-court` | Court of the Concord | `cendre:seneschal-mireille` | ≥3 of {q-i1..q-i5} completed **(P6 code task #3)** | BringItem `cendre:item-writ` ×1, KillMob `cendre:threat-stranger` ×1 (capture branch swaps via DialogueChoice), VisitRoom `cendre:court-chamber` | Item `cendre:item-heirloom` ×1, Achievement `cendre_concord_witness` |
| `cendre:q-mortal-bounty` | Bounty of Saint-Cendre | `cendre:mortal-pere-dominique` | none | KillMob (vampire-flagged) ×3 **(P6 code task #5)** | Gold 200, Item `cendre:item-silver-knife` ×1 (`repeatable: true`) |
| `cendre:q-mortal-charm` | Madame Beauchamp's Lost Charm | `cendre:mortal-beauchamp` | none | BringItem `cendre:item-gris-gris` ×1, return_to `cendre:mortal-beauchamp` | Item `cendre:item-rabbit-foot` ×1 |
| `cendre:q-anarch-pact` | The Unsigned Pact | `cendre:threat-casey-anarch` | `cendre_met_all_sires` achievement held AND no `cendre:q-embrace-*` started or completed **(P6 code tasks #2 + #6)** | BringItem `cendre:item-anarch-pact-token` ×1, return_to `cendre:threat-casey-anarch` | **NEW reward variant** `EmbraceAnarch { discipline: <player choice> }` **(P6 code task #1)** |

15 quests total. Vnum convention: `cendre:q-<key>`. Q-I track IDs use `cendre:q-i<n>-<key>` to avoid prefix collision.

#### 4.C Item Vnum Catalog (18 items in v1; 1 deferred)

| vnum | Name | Used by | Notes |
|---|---|---|---|
| `cendre:item-journal` | une cendre carnet | Q1 reward | Readable via `note_content`; flavor only |
| `cendre:item-foundry-token` | a brass fight-pit token | Q2 objective | Earned in the fight pit, returned to Tony |
| `cendre:item-painting` | a small oil portrait, "L'Aurore" | Q3 objective | Returned to Yvette |
| `cendre:item-debt-marker` | a wax-sealed promissory note | Q4 objective | Returned to Henri |
| `cendre:item-relic` | a corroded silver pendant | Q5 objective | Returned to Caretaker |
| `cendre:item-bayou-trophy` | a gator's jagged tooth | Q6 objective | Returned to Solange |
| `cendre:item-signet-forged` | a forged Ventrue signet | Q-I1 | Player retains as evidence |
| `cendre:item-opera-ticket` | a torn opera ticket stub | Q-I2 | Readable seat number |
| `cendre:item-guest-log` | the opera box-office guest log | Q-I2 | Readable via `note_content` |
| `cendre:item-audit-ledger` | a bound audit ledger | Q-I3 | Readable; irregular entries |
| `cendre:item-soil-bayou` | a vial of bayou earth | Q-I4 | |
| `cendre:item-soil-catacomb` | a vial of catacomb dust | Q-I4 | |
| `cendre:item-victim-corpse` | Gris-de-fer's body | Q-I4 set-dressing | Non-pickup room item |
| `cendre:item-writ` | the Seneschal's writ of search | Q7 | Bypasses shack lock |
| `cendre:item-heirloom` | the Prince's signet ring (gift) | Q7 reward | Decorative + small bonus |
| `cendre:item-silver-knife` | a silver-edged knife | Q8 reward | `night_vision` flag |
| `cendre:item-gris-gris` | a leather pouch of bones and feathers | Q9 objective | |
| `cendre:item-rabbit-foot` | une patte de lapin | Q9 reward | Minor luck buff |
| ~~`cendre:item-anarch-pact-token`~~ | ~~a blank, unsigned coin~~ | ~~Q10 objective~~ | **Deferred to post-v1** alongside Q10 (Casey's Unsigned Pact, Phase 3 expansion candidate). Do not build in Phase 6 v1. |

#### 4.D Room Vnum Catalog (23 quest-specific rooms)

Beyond the 13 anchor rooms (Phase 1) and the standard district build (Phase 5 baseline).

| vnum | Quest(s) | District |
|---|---|---|
| `cendre:hotel-foyer` | Q1 entry | Hôtel de Larue |
| `cendre:court-chamber` | Q7 | Hôtel de Larue |
| `cendre:foundry-pit` | Q2 | Foundry |
| `cendre:foundry-metalworker-shop` | Q-I1 | Foundry |
| `cendre:foundry-metalworker-back` | Q-I1 (hiding spot) | Foundry |
| `cendre:foundry-cellar` | Casey discovery (Phase 3.D), Q10 | Foundry |
| `cendre:conservatory-dressing-room` | Q-I2 | Conservatory |
| `cendre:conservatory-box-office` | Q-I2 | Conservatory |
| `cendre:conservatory-collector-apt-1` | Q3 | Conservatory |
| `cendre:conservatory-collector-apt-2` | Q3 (optional second room) | Conservatory |
| `cendre:bourse-bank-office` | Q-I3 | Bourse |
| `cendre:catacombs-branch-1` | Q5 | Catacombs |
| `cendre:catacombs-branch-2` | Q5 | Catacombs |
| `cendre:catacombs-branch-3` | Q5 | Catacombs |
| `cendre:catacombs-branch-4` | Q5 | Catacombs |
| `cendre:catacombs-exam-chamber` | Q-I4 | Catacombs |
| `cendre:bayou-trail-1` | Q-I5 | Bayou |
| `cendre:bayou-trail-2` | Q-I5 | Bayou |
| `cendre:bayou-trail-3` | Q-I5 | Bayou |
| `cendre:bayou-shack-exterior` | Q-I5, Q7 entry | Bayou |
| `cendre:bayou-shack-interior-1` | Q7 (locked, requires writ) | Bayou |
| `cendre:bayou-shack-interior-2` | Q7 | Bayou |
| `cendre:bayou-shack-evidence-cache` | Q7 (5-piece capture branch) | Bayou |

**Plus 5 haven-entry rooms** referenced by Q1, which are part of standard Phase 5 district builds (not net-new for Phase 4):
- `cendre:foundry-office` (Tony) — Foundry
- `cendre:conservatory-box` (Yvette) — Conservatory
- `cendre:bourse-chamber` (Henri) — Bourse
- `cendre:catacombs-chamber` (Caretaker) — Catacombs
- `cendre:bayou-hut` (Solange) — Bayou

#### 4.E Phase 6 Code Prereqs (6 items — ALL ✅ as of 2026-05-11)

Concrete Rust/Rhai work Phase 6 must land before all 15 quests function. None of this is Phase 4 build — Phase 4 ships pure design.

1. ✅ **`QuestReward::EmbraceAnarch { discipline: Option<String> }`** (Q10). Landed 2026-05-11. Variant added to `src/types/quests.rs` next to `EmbraceClan`. Reward handler in `src/quest/mod.rs` (next to the EmbraceClan arm) lifts the thinblood gates (blood pool 6→10, refill, sun damage normal, humanity normal, tier-3 disciplines unlocked) via the new `apply_anarch_acknowledgment` in `src/script/vampire.rs`, stamps the `anarch_unbound` trait, sets sire to the `"Anarch Unbound"` sentinel, and seeds 1 dot of the discipline. Discipline resolves hardcoded → runtime choice (option (a) below) and is validated against `known_disciplines()` (the union of `preferred_disciplines` across `scripts/data/vampire_clans.json`: potence, celerity, auspex, dominate, obfuscate). MCP-authorable via `embrace_anarch` reward kind.
2. ✅ **Discipline-pick mechanism for Q10** — option (a) chosen and landed 2026-05-11. New `ActiveQuest.choice_vars: HashMap<String,String>` carries per-quest runtime picks. Authored in dialogue via `DialogueEffect::SetQuestChoice { quest_vnum, key, value }`; read by `EmbraceAnarch` (when its `discipline` is None) at completion; inspectable via `DialogueCondition::QuestChoiceEquals { quest_vnum, key, value }` for follow-up branches. Casey's tree authoring (Slice 6.18): `OfferQuest cendre:q-anarch-pact` → branch choice → `SetQuestChoice { quest_vnum: "cendre:q-anarch-pact", key: "discipline", value: "<pick>" }` → continue/exit; completion fires the no-discipline `EmbraceAnarch` reward which consumes choice_vars. MCP-authorable via `set_quest_choice` effect + `quest_choice_equals` condition.
3. **Set-count quest prereq** (Q7). Need either a new `QuestData.prereq_min_completed_from: Option<(Vec<String>, i32)>` field, OR a new `DialogueCondition::CompletedQuestCount { vnums: Vec<String>, min: i32 }` that Mireille's tree gates the Q7-offer branch on. Either works.
4. **`HasAchievement` dialogue condition** (Q-I1..Q-I5, Q7, Q10). Investigation flags are surfaced via `Achievement` rewards. Dialogue branches that gate on "has investigation piece" need a `DialogueCondition::HasAchievement { key: String }` if it doesn't already exist. Verify in `src/types/dialogue.rs`; if absent, add.
5. **Multi-vnum KillMob OR canonical migrant vnum** (Q8). Either extend `QuestObjective::KillMob` to accept `vnums: Vec<String>`, OR ensure the migration system tags all clan migrants with one canonical prototype vnum (e.g., `cendre:vampire-migrant`) so the existing single-vnum KillMob fires. Phase 1.7's `immigration_vampire_chance` work may already converge on the latter — verify Phase 6 entry point.
6. **`met_all_sires` flag setting** (Q1). Q1's reward is `Achievement cendre_met_all_sires`; Phase 3.B dialogue gates reference a "trait" called `met_all_sires`. Recommendation: standardize on Achievement keys as the canonical flag for ALL `cendre_*` per-character state and rely on the `HasAchievement` dialogue condition from §4.E.4. Alternative: add `QuestReward::SetTrait { trait_name: String }`.

#### 4.F Cross-Quest Build Requirements (replaces prose summary above)

Aggregated from 4.A–4.D:

- **Items**: 18 (itemized in 4.C).
- **Unique mobiles beyond main cast**: 8 (itemized in 4.A). Reduction from the prior ~12 estimate comes from reusing Émeric (Q-I3) and Coyote (Q-I5).
- **Rooms**: 23 quest-specific (4.D) + 5 haven-entry rooms already in the Phase 5 baseline.
- **Dialogue trees**: 15 total — 6 sire/court (Mireille tutorial + 5 sires) + 5 investigation-NPC + 3 mortal-side + 1 Casey (Q10).
- **Per-character flags**: 5 investigation achievements (`cendre_investigation_*`), `cendre_met_all_sires`, `cendre_concord_witness`, clan acknowledgment (already wired), `anarch_unbound` trait (added by P6 task #1).

#### 4.G Out of Scope for the Deep Dive

- Writing actual dialogue text (Phase 6).
- Implementing the 6 P6 code prereqs in §4.E (Phase 6).
- Building any items, rooms, mobs, triggers, or quest configs (Phase 5/6).
- New traits beyond those already enumerated in Phase 3.H + `anarch_unbound`.

---

### Build plan for this phase
No build this phase; the cross-quest build requirements summary above feeds Phase 6's slice list.

### Slices
No slices.

### Definition of phase done
Quest design approved; cross-quest build requirements summary captured in a form Phase 6 can directly slice.

### Phase 4 deep-dive log (2026-05-10)
- Subsections 4.A–4.G appended after the Deferred-from-v1 paragraph; the original 14-quest prose specs (Q1–Q9) stay intact as the high-level frame.
- **Quest count 14 → 15**: Q10 "The Unsigned Pact" (Casey's Anarch clanless path) promoted into v1 per user scope decision. Anarch quest line removed from the Deferred list.
- **NPC reconciliation**: Q-I3 giver resolved to existing `cendre:bourse-clerk` (Émeric, reconciled in-fiction as courthouse-clerk-by-day / bourse-auditor-evenings). Q-I5 giver resolved to existing `cendre:bayou-coyote` (matches Phase 3.B mesh). Net new NPC prototypes drop from ~12 to 8.
- **Cast total grows 45 → 53**. 8 new minor NPCs catalogued in 4.A with full preset/faction specs.
- **Quest vnum convention locked**: `cendre:q-<key>` for top-level quests; `cendre:q-i<n>-<key>` for investigation track to avoid prefix collision.
- **Item vnum catalog**: 18 items in 4.C, all under `cendre:item-` prefix, each with a name + which quest uses it.
- **Room vnum catalog**: 23 quest-specific rooms in 4.D + 5 haven-entry rooms (already in Phase 5 baseline).
- **6 Phase 6 code prereqs surfaced in 4.E**: (1) `QuestReward::EmbraceAnarch` variant; (2) Q10 discipline-pick mechanism; (3) set-count prereq for Q7's "≥3 investigation pieces" gate; (4) `HasAchievement` dialogue condition; (5) multi-vnum or canonical migrant vnum for Q8's kill-credit; (6) `met_all_sires` flag delivery (recommend Achievement-as-flag standardization across all `cendre_*` state).
- **Achievement-as-flag standardization recommended**: all `cendre_*` per-character flags (`cendre_met_all_sires`, `cendre_investigation_*`, `cendre_concord_witness`) ride on Achievement rewards + a `HasAchievement` dialogue condition. Only `anarch_unbound` stays a trait (set by the new EmbraceAnarch reward handler).
- **Cross-quest build requirements summary** (lines 742–750 above) was retained as-is for historical comparison; the updated counts live in 4.F.
- Status table row updated: Phase 4 now shows "✅ approved · ✅ deep-dive approved (2026-05-10)".
- Phase 3 deep dive (3.A–3.I) remains the canonical reference for which NPC carries which Layer-1/2/3 hint; Phase 4's per-quest specs cross-link via vnum.

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

#### Room Budget by District (Phase 5 build, ~87 rooms; total area target ~101 including Phase 1 anchor)

| District / Zone | Rooms | Combat | Notes |
|---|---|---|---|
| **Cathedral District** (extras around plaza) | 9 | Mostly Safe | Cathedral interior (3), Hôtel de Larue / Prince's court (3), opera house entrance (1), courthouse exterior (1), Garrison (1). (Plaza itself built in Phase 1.) |
| **Riverfront** | 6 | Safe (market) + PvP (docks) | Market square, fishmonger, hotel, dock 1-3, Old Léon's barge |
| **The Foundry** (Brujah) | 11 | PvP haven, Safe street front | Foundry exterior, foundry main 3, fight pit, jazz hall 2, Tony's office, metalworker shop + back room, Casey's cellar (+1 from §5.A) |
| **The Conservatory** (Toreador) | 10 | PvP haven, Safe street front | Opera house interior 4, art gallery 2, dressing room, box office, collector's apartment 2 |
| **Bourse Quarter** (Ventrue) | 8 | PvP haven, Safe street front | Bank exterior, bank interior 2, courthouse interior 2, gentlemen's club 2, Magistrate's chamber |
| **Catacombs / Cemetery** (Nosferatu) | 12 | PvP everywhere | Cemeteries above ground 3, catacomb branch 5, Caretaker's chamber, examination chamber, evidence storage 2 |
| **Bayou's Edge** (Gangrel) | 13 | PvP everywhere | Levee road 2, bayou edge, bayou trail 4, fisherman's camp, Solange's hut, Stranger's shack exterior + interior 3 (off-by-one fix from §5.I) |
| **Day-life / Misc shops** | 8 | Safe | Voodoo curio shop, café, antique dealer, jazz hall (mortal-facing), tourist hotel lobby, fortune teller's nook, two atmospheric storefronts |
| **Side alleys / connective PvP** | 10 | PvP | Inter-district shortcuts, dead-ends with flavor, the alley a victim was found in, etc. |
| **Phase 5 total** | **87** | | (Plus 14 Phase 1 anchor rooms = 101 area total.) |

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

#### 5.A Room Budget Reconciliation

Aggregate update from the user's "Expand Foundry to 11" decision plus a math fix from Slice 1.8 (`cendre:portal-alley` was the 14th anchor room) and the off-by-one in the original Bayou breakdown (always summed to 13, not 12).

| Adjustment | Was | Now |
|---|---|---|
| Anchor rooms (Phase 1) | 13 (original anchor) | 14 (+ Slice 1.8 portal-alley) |
| Foundry slice (5.3) | 10 | 11 (+ `cendre:foundry-cellar`) |
| Bayou slice (5.7) | 12 (advertised) | 13 (matches original breakdown sum) |
| Phase 5 total | 85 | 87 |
| **Area total** | **98** (stale) | **101** |

The cellar is a distinct room, not a repurposing of the back alley. Foundry slice 5.3 grows by one `create_room` call and two extra exit edges (foundry-main-2 ↔ foundry-cellar hidden trapdoor; foundry-cellar ↔ alley-foundry-cellar-access side door — see §5.M).

#### 5.B §4.D Catalog Gap Fixes

| §4.D vnum | Resolution in Phase 5 |
|---|---|
| `cendre:foundry-cellar` | Net-new room in slice 5.3 (Foundry 10 → 11). Accessed via hidden trapdoor in `cendre:foundry-main-2` and via sealed side door from `cendre:alley-foundry-cellar-access` (slice 5.9). |
| `cendre:bayou-shack-evidence-cache` | Third Stranger's-shack interior room in slice 5.7. Reached from `cendre:bayou-shack-interior-2` via a hidden floorboard exit (`down`). Holds the 5-piece evidence cache for Q7's capture branch. |
| `cendre:hotel-foyer` / `cendre:court-chamber` / Prince's audience | Hôtel de Larue's 3 rooms in slice 5.1 are: `cendre:hotel-foyer`, `cendre:prince-audience`, `cendre:court-chamber`. The previously-vague "Prince's court" splits cleanly into the three. |
| `cendre:conservatory-box` (Yvette's haven) | One of the 4 opera-house-interior rooms in slice 5.4: the owner's box, reached via `cendre:opera-foyer` up. |
| `cendre:bourse-chamber` (Henri's haven) | Magistrate Henri Saint-Clair IS the Ventrue sire from the Phase 2 cast. The existing "Magistrate's chamber" room in slice 5.5 IS `cendre:bourse-chamber` — just renamed to align with §4.D. |

#### 5.C Slice 5.1 — Cathedral District extras (9 rooms)

Attaches to `cendre:plaza`. Plaza gains 4 new diagonal exits + `in` (one per district-extra entry; cardinals stay reserved for Phase 1 arterials).

| vnum | short | combat_zone | room flags | residents |
|---|---|---|---|---|
| `cendre:cathedral-nave` | The Cathedral of Saint-Cendre | safe | indoors, no_mob | `cendre:mortal-agathe`, `cendre:mortal-pere-dominique` |
| `cendre:cathedral-altar` | Before the High Altar | safe | indoors, no_mob | — |
| `cendre:cathedral-vestry` | The Vestry | safe | indoors, no_mob, dark | — |
| `cendre:hotel-foyer` | Foyer of the Hôtel de Larue | safe | indoors | — |
| `cendre:prince-audience` | Prince's Audience Chamber | safe | indoors, dark, no_magic | `cendre:prince-larue`; `cendre:seneschal-mireille` at night |
| `cendre:court-chamber` | Court of the Concord | safe | indoors, no_magic | (Q7 climax; Mireille presides when invoked) |
| `cendre:opera-entrance` | Steps of the Opera House | safe | — | `cendre:mortal-opera-attendant` |
| `cendre:courthouse-exterior` | Steps of the Courthouse | safe | — | (guard rover point) |
| `cendre:garrison` | The City Garrison | safe | indoors, no_mob | `cendre:guard-roussel`; all guards off-shift |

Key exits (within slice): cathedral-nave ↔ cathedral-altar (n/s), cathedral-altar ↔ cathedral-vestry (e/w), cathedral-nave ↔ garrison (w/e — garrison hangs off the cathedral interior, not direct from plaza), hotel-foyer ↔ prince-audience (n/s), hotel-foyer ↔ court-chamber (e/w). Plaza-attach (4 new diagonal edges): plaza ↔ cathedral-nave (ne/sw), plaza ↔ hotel-foyer (nw/se), plaza ↔ opera-entrance (sw/ne — its `n` exit leads to opera-foyer in slice 5.4), plaza ↔ courthouse-exterior (se/nw — its `n` exit leads to courthouse-interior-1 in slice 5.5). Deferred door (wired in slice 5.6): cathedral-nave ↔ catacombs-cathedral-entrance (down/up) — hidden, lockpick-discoverable.

#### 5.D Slice 5.2 — Riverfront (6 rooms)

Attaches via `cendre:rue-eau-3` (south arterial dead-end stub).

| vnum | short | combat_zone | room flags | residents |
|---|---|---|---|---|
| `cendre:riverfront-market` | Riverfront Market | safe | — | `cendre:mortal-fishmonger`, `cendre:guard-cormier` |
| `cendre:riverfront-fishmonger` | Boudreaux's Fish Stall | safe | — | (Boudreaux during the day) |
| `cendre:riverfront-hotel-lobby` | Tourist Hotel Lobby | safe | indoors, no_mob | `cendre:mortal-hotel-clerk` |
| `cendre:riverfront-dock-1` | The North Pier | pvp | — | — |
| `cendre:riverfront-dock-2` | The South Pier | pvp | — | — |
| `cendre:riverfront-leon-barge` | Old Léon's Barge | safe | indoors | `cendre:service-leon` |

Key exits: rue-eau-3 ↔ riverfront-market (s/n), market ↔ fishmonger (e/w), market ↔ hotel-lobby (w/e), market ↔ dock-1 (s/n), dock-1 ↔ dock-2 (e/w), dock-2 ↔ leon-barge (s/n). Market east exits → cemetery-gate (slice 5.6).

(Original budget said "dock 1-3"; consolidated to 2 docks + the barge to keep the 6-room budget without losing flavor.)

#### 5.E Slice 5.3 — The Foundry (Brujah, 11 rooms)

Attaches via `cendre:rue-cendre-3` (east arterial dead-end stub). +1 vs the original budget for `cendre:foundry-cellar`.

| vnum | short | combat_zone | room flags | residents |
|---|---|---|---|---|
| `cendre:foundry-exterior` | Outside the Old Foundry | safe | — | (street side; safe by day) |
| `cendre:foundry-main-1` | The Foundry Floor | pvp | indoors | `cendre:foundry-marisol` |
| `cendre:foundry-main-2` | The Catwalks | pvp | indoors, dark | — |
| `cendre:foundry-main-3` | The Forge Room | pvp | indoors | — |
| `cendre:foundry-pit` | The Fight Pit | pvp | indoors, dark | `cendre:foundry-bones`, `cendre:foundry-enforcer` (×3 cap) |
| `cendre:foundry-jazz-1` | Jazz Hall — Floor | pvp | indoors | `cendre:foundry-beau` |
| `cendre:foundry-jazz-2` | Jazz Hall — Mezzanine | pvp | indoors, dark | — |
| `cendre:foundry-office` | Tony's Office | pvp | indoors, dark, no_magic | `cendre:sire-brujah` |
| `cendre:foundry-metalworker-shop` | The Metalworker's Shop | safe | indoors | `cendre:foundry-metalworker` |
| `cendre:foundry-metalworker-back` | The Metalworker's Back Room | pvp | indoors, dark | (Q-I1 hiding spot) |
| `cendre:foundry-cellar` | A Forgotten Cellar | pvp | indoors, dark, no_magic | `cendre:threat-casey-anarch` |

Note: the original 10-room "back alley" entry is rolled into slice 5.9's connective alleys as `cendre:alley-foundry-cellar-access` (the cellar's sealed side door). The Foundry's own 11 rooms are all distinct from connective alleys.

Key exits: rue-cendre-3 ↔ foundry-exterior (e/w), foundry-exterior ↔ foundry-main-1 (n/s), main-1 ↔ main-2 (up/down), main-1 ↔ main-3 (e/w), main-3 ↔ foundry-pit (n/s), main-1 ↔ foundry-jazz-1 (w/e — internal door), jazz-1 ↔ jazz-2 (up/down), main-2 ↔ foundry-office (n/s), foundry-exterior ↔ foundry-metalworker-shop (s/n), metalworker-shop ↔ metalworker-back (in/out). Cellar: foundry-main-2 ↔ foundry-cellar (down/up — hidden trapdoor, lockpick), foundry-cellar ↔ alley-foundry-cellar-access (e/w — **sealed**, slice 5.9 host).

#### 5.F Slice 5.4 — The Conservatory (Toreador, 10 rooms)

Attaches via `cendre:rue-arts-3` (back-door arterial entry) AND from `cendre:opera-entrance` (front-of-house from slice 5.1).

| vnum | short | combat_zone | room flags | residents |
|---|---|---|---|---|
| `cendre:opera-foyer` | The Opera House Foyer | safe | indoors, no_mob | (Marcellin attends entry — `cendre:mortal-opera-attendant` lives in slice 5.1) |
| `cendre:opera-house` | The Opera House Auditorium | safe | indoors | `cendre:conservatory-etienne` |
| `cendre:opera-bar` | The Opera House Bar | pvp | indoors, dark | `cendre:harpy-theo`, `cendre:conservatory-aldo` |
| `cendre:conservatory-box` | Lady Beaumont's Owner's Box | pvp | indoors, dark, no_magic | `cendre:sire-toreador` |
| `cendre:art-gallery-1` | The Conservatory Gallery, East | safe | indoors | `cendre:conservatory-cassandra` (day) |
| `cendre:art-gallery-2` | The Conservatory Gallery, West | safe | indoors | — |
| `cendre:conservatory-dressing-room` | A Dressing Room | pvp | indoors, dark | `cendre:conservatory-dresser` |
| `cendre:conservatory-box-office` | The Box Office | safe | indoors | (Q-I2 guest log) |
| `cendre:conservatory-collector-apt-1` | The Collector's Sitting Room | pvp | indoors | `cendre:conservatory-collector` |
| `cendre:conservatory-collector-apt-2` | The Collector's Gallery Room | pvp | indoors, dark | (Q3 painting hangs here) |

Key exits: rue-arts-3 ↔ opera-foyer (w/e), opera-entrance (slice 5.1) ↔ opera-foyer (n/s), opera-foyer ↔ opera-house (n/s), opera-house ↔ opera-bar (e/w), opera-foyer ↔ conservatory-box (up/down), opera-house ↔ conservatory-dressing-room (in/out — backstage), opera-foyer ↔ conservatory-box-office (w/e), opera-foyer ↔ art-gallery-1 (s/n), gallery-1 ↔ gallery-2 (w/e), gallery-2 ↔ conservatory-collector-apt-1 (out/in), apt-1 ↔ apt-2 (n/s).

#### 5.G Slice 5.5 — Bourse Quarter (Ventrue, 8 rooms)

Attaches via `cendre:rue-royale-3` (north arterial dead-end stub) AND from `cendre:courthouse-exterior` (slice 5.1).

| vnum | short | combat_zone | room flags | residents |
|---|---|---|---|---|
| `cendre:bourse-exterior` | Before the Bank | safe | — | (street-side) |
| `cendre:bourse-bank-floor` | The Bank Floor | safe | indoors, no_mob | `cendre:bourse-pierre` |
| `cendre:bourse-bank-office` | Émeric's Bank Office | safe | indoors | `cendre:bourse-clerk` (evenings) |
| `cendre:courthouse-interior-1` | The Courthouse Rotunda | safe | indoors | `cendre:bourse-clerk` (daytime) |
| `cendre:courthouse-interior-2` | A Records Annex | safe | indoors, dark | — |
| `cendre:bourse-club-1` | The Gentlemen's Club, Salon | pvp | indoors | `cendre:bourse-lucien` |
| `cendre:bourse-club-2` | The Club's Back Rooms | pvp | indoors, dark, no_magic | (vampire shelter) |
| `cendre:bourse-chamber` | The Magistrate's Chamber | pvp | indoors, dark, no_magic | `cendre:sire-ventrue` (Henri Saint-Clair) |

Key exits: rue-royale-3 ↔ bourse-exterior (n/s), bourse-exterior ↔ bourse-bank-floor (in/out), bank-floor ↔ bourse-bank-office (e/w), bourse-exterior ↔ bourse-club-1 (e/w — neighboring building), club-1 ↔ club-2 (in/out), club-2 ↔ bourse-chamber (up/down — Henri lives above the club), courthouse-exterior (slice 5.1) ↔ courthouse-interior-1 (n/s), interior-1 ↔ interior-2 (in/out). Hidden door (wired in slice 5.6): bourse-club-2 ↔ catacombs-bourse-entrance (down/up) — Ventrue secret entry.

(Émeric pulls double duty: courthouse-interior-1 by day, bourse-bank-office in evenings — single mob, Phase 6 daily_routine.)

#### 5.H Slice 5.6 — Catacombs / Cemetery (Nosferatu, 12 rooms)

Attaches via Riverfront (cemetery sits east of the market) AND via hidden doors from cathedral (slice 5.1) and gentlemen's club (slice 5.5).

| vnum | short | combat_zone | room flags | residents |
|---|---|---|---|---|
| `cendre:cemetery-gate` | The Cemetery Gates | pvp | — | `cendre:catacomb-sexton` |
| `cendre:cemetery-rows-1` | Cemetery — Mausoleum Row | pvp | — | `cendre:threat-hunter-coyle` (night) |
| `cendre:cemetery-rows-2` | Cemetery — Old Section | pvp | dark | — |
| `cendre:catacombs-entrance` | The Catacomb Entrance | pvp | indoors, dark, no_magic | — |
| `cendre:catacombs-branch-1` | A Crumbling Passage | pvp | indoors, dark, no_magic | `cendre:catacomb-risen` (×2 cap) |
| `cendre:catacombs-branch-2` | A Side Chamber | pvp | indoors, dark, no_magic | `cendre:catacomb-acolyte` |
| `cendre:catacombs-branch-3` | The Deeper Catacombs | pvp | indoors, dark, no_magic | `cendre:catacomb-ribcage` |
| `cendre:catacombs-branch-4` | The Wet Catacombs | pvp | indoors, dark, no_magic | — |
| `cendre:catacombs-chamber` | The Caretaker's Chamber | pvp | indoors, dark, no_magic | `cendre:sire-nosferatu` |
| `cendre:catacombs-exam-chamber` | An Examination Chamber | pvp | indoors, dark, no_magic | (Q-I4 body; `cendre:item-victim-corpse`) |
| `cendre:catacombs-cathedral-entrance` | Beneath the Cathedral | pvp | indoors, dark, no_magic | — |
| `cendre:catacombs-bourse-entrance` | Beneath the Bourse | pvp | indoors, dark, no_magic | — |

Note: original budget breakdown was "cemeteries above ground 3 + catacomb branch 5 + Caretaker + exam + evidence storage 2" = 12. Revised: 3 cemetery + 4 branch + Caretaker + exam + 2 entrance-rooms (cathedral + bourse) = 12. Evidence-storage folded into the exam-chamber (which holds the Q-I4 body) and branch rooms (which host the Caretaker's research clutter via Phase 6 room extras).

Key exits: riverfront-market ↔ cemetery-gate (e/w), cemetery-gate ↔ cemetery-rows-1 (n/s), rows-1 ↔ rows-2 (e/w), cemetery-rows-1 ↔ catacombs-entrance (down/up), catacombs-entrance ↔ catacombs-branch-1 (n/s), branch-1 ↔ branch-2 (e/w), branch-1 ↔ branch-3 (n/s), branch-3 ↔ branch-4 (e/w), branch-3 ↔ catacombs-chamber (down/up), catacombs-chamber ↔ catacombs-exam-chamber (e/w), branch-2 ↔ catacombs-cathedral-entrance (up/down — meets cathedral-nave down), branch-4 ↔ catacombs-bourse-entrance (up/down — meets bourse-club-2 down).

#### 5.I Slice 5.7 — Bayou's Edge + Stranger's Shack (Gangrel, 13 rooms)

Attaches via Levee Road from cemetery district (single entry; design-intentional cordon). Original budget said 12 but the listed breakdown summed to 13 — Phase 5 deep dive locks it at 13.

| vnum | short | combat_zone | room flags | residents |
|---|---|---|---|---|
| `cendre:levee-road-1` | The Levee Road | pvp | — | `cendre:guard-tisserand` (daytime) |
| `cendre:levee-road-2` | The Levee Road, South Stretch | pvp | — | — |
| `cendre:bayou-edge` | Bayou's Edge | pvp | dark | `cendre:bayou-andre` |
| `cendre:bayou-trail-1` | A Bayou Path | pvp | dark | `cendre:bayou-coyote` (rover) |
| `cendre:bayou-trail-2` | Deeper into the Bayou | pvp | dark | — |
| `cendre:bayou-trail-3` | A Hidden Clearing | pvp | dark | `cendre:threat-hunter-voss` (night) |
| `cendre:bayou-trail-4` | The Far Shallows | pvp | dark | `cendre:bayou-predator-gator` |
| `cendre:bayou-fisherman-camp` | The Fisherman's Camp | pvp | dark | `cendre:bayou-fisherman` |
| `cendre:bayou-hut` | Ma'tante Solange's Hut | pvp | indoors, dark, no_magic | `cendre:sire-gangrel` |
| `cendre:bayou-shack-exterior` | A Sealed Shack | pvp | dark | — |
| `cendre:bayou-shack-interior-1` | Inside the Shack — Front Room | pvp | indoors, dark, no_magic | `cendre:threat-stranger` |
| `cendre:bayou-shack-interior-2` | Inside the Shack — Back Room | pvp | indoors, dark, no_magic | — |
| `cendre:bayou-shack-evidence-cache` | A Hidden Sub-Cellar | pvp | indoors, dark, no_magic | (Q7 evidence cache) |

Breakdown: 2 levee-road + 1 bayou-edge + 4 bayou-trail + 1 fisherman-camp + 1 Solange's-hut + 1 shack-exterior + 3 shack-interior (front, back, evidence-cache) = **13 rooms**.

Key exits: cemetery-gate ↔ levee-road-1 (s/n), levee-road-1 ↔ levee-road-2 (s/n), levee-road-2 ↔ bayou-edge (s/n), bayou-edge ↔ bayou-trail-1 (s/n), trail-1 ↔ trail-2 (w/e), trail-2 ↔ trail-3 (s/n — Q-I5 ends here at the shack exterior), trail-2 ↔ trail-4 (e/w — gator zone, Q6), bayou-edge ↔ bayou-fisherman-camp (e/w), bayou-edge ↔ bayou-hut (w/e — Solange), trail-3 ↔ bayou-shack-exterior (in/out — short walk), shack-exterior ↔ shack-interior-1 (in/out — **sealed door, requires `cendre:item-writ`**), interior-1 ↔ interior-2 (n/s), interior-2 ↔ shack-evidence-cache (down/up — hidden floorboard, lockpick-discoverable).

#### 5.J Slice 5.8 — Day-life / Misc shops (8 rooms)

Attaches around plaza and along the arterials.

| vnum | short | combat_zone | room flags | residents |
|---|---|---|---|---|
| `cendre:shop-voodoo` | Madame Beauchamp's Curios | safe | indoors | `cendre:mortal-beauchamp`, `cendre:mortal-thief-customer` |
| `cendre:shop-cafe` | Café Doré | safe | indoors, no_mob | `cendre:mortal-cafe-henri` |
| `cendre:shop-antique` | Lefèvre & Sons Antiques | safe | indoors | `cendre:mortal-lefevre` |
| `cendre:shop-mortal-jazz` | The Cobalt Lounge | safe | indoors | (mortal-facing jazz; atmosphere) |
| `cendre:shop-fortune` | Madame Olympe's Nook | safe | indoors, dark | `cendre:service-olympe` |
| `cendre:shop-bookseller` | Pages & Bindings | safe | indoors | — |
| `cendre:shop-tailor` | Maison Verlaine | safe | indoors | — |
| `cendre:shop-apothecary` | The Apothecary | safe | indoors | — |

(Tourist hotel lobby lives in slice 5.2 as `cendre:riverfront-hotel-lobby`, not slice 5.8.)

Plaza-attach + arterial-attach exits: rue-cendre-1 ↔ shop-voodoo (n/s), rue-arts-1 ↔ shop-cafe (n/s), rue-arts-2 ↔ shop-antique (s/n), rue-cendre-2 ↔ shop-mortal-jazz (s/n), plaza ↔ shop-fortune (in/out — small nook off plaza), rue-royale-1 ↔ shop-bookseller (e/w), rue-royale-2 ↔ shop-tailor (w/e), rue-eau-1 ↔ shop-apothecary (e/w).

#### 5.K Slice 5.9 — Side alleys / connective PvP (10 rooms)

PvP shortcut alleys cutting between districts. The `alley-foundry-cellar-access` room is the cellar's sealed side-door host (back-door from §5.E).

| vnum | short | combat_zone | room flags | residents |
|---|---|---|---|---|
| `cendre:alley-foundry-to-bourse` | A Narrow Alley | pvp | dark | — |
| `cendre:alley-bourse-to-cathedral` | A Service Alley | pvp | dark | `cendre:threat-hunter-brennan` (night) |
| `cendre:alley-cathedral-to-conservatory` | A Tree-Lined Lane | pvp | — | — |
| `cendre:alley-conservatory-to-riverfront` | A Stone Stairway | pvp | dark | — |
| `cendre:alley-riverfront-to-foundry` | A Brick Cut-Through | pvp | dark | — |
| `cendre:alley-victim-1` | The Alley Where Mathilde Was Found | pvp | dark | (Q-I2 flavor; bloodstain ground item) |
| `cendre:alley-victim-2` | The Alley Where Marcel Fell | pvp | dark | (Q-I1 flavor) |
| `cendre:alley-dead-end-1` | A Dead End | pvp | dark | — |
| `cendre:alley-foundry-cellar-access` | The Foundry Back Lane | pvp | dark | — |
| `cendre:alley-bayou-mouth` | The Mouth of the Levee | pvp | — | — |

Key exits (shortcuts): rue-cendre-2 ↔ alley-foundry-to-bourse (n/s), alley-foundry-to-bourse ↔ rue-royale-2 (e/w), rue-royale-1 ↔ alley-bourse-to-cathedral (e/w), alley-bourse-to-cathedral ↔ cathedral-nave (in/out — service entry), rue-arts-1 ↔ alley-cathedral-to-conservatory (n/s), rue-arts-3 ↔ alley-conservatory-to-riverfront (s/n), alley-conservatory-to-riverfront ↔ riverfront-market (in/out), rue-eau-3 ↔ alley-riverfront-to-foundry (e/w), alley-riverfront-to-foundry ↔ rue-cendre-3 (e/w), rue-arts-2 ↔ alley-victim-1 (in/out, Q-I2), rue-cendre-1 ↔ alley-victim-2 (in/out, Q-I1), rue-cendre-3 ↔ alley-dead-end-1 (in/out), foundry-cellar ↔ alley-foundry-cellar-access (e/w, **sealed**), levee-road-1 ↔ alley-bayou-mouth (e/w — side branch off the levee; levee-road-1's south exit is reserved for levee-road-2 in slice 5.7).

#### 5.L NPC Home Assignments (canonical Phase 6 reference)

Master mapping of every Phase 2 cast NPC (45) + the 8 new minor NPCs (§4.A) to a specific Phase 5 room vnum. Phase 6's spawn-point slices read off this table. Patrol-route NPCs list their home/off-shift room first.

| Mob vnum | Phase 5 home vnum |
|---|---|
| `cendre:prince-larue` | `cendre:prince-audience` |
| `cendre:seneschal-mireille` | `cendre:plaza` (night) / `cendre:hotel-foyer` (day) |
| `cendre:harpy-theo` | `cendre:opera-bar` |
| `cendre:sire-brujah` | `cendre:foundry-office` |
| `cendre:sire-toreador` | `cendre:conservatory-box` |
| `cendre:sire-ventrue` | `cendre:bourse-chamber` |
| `cendre:sire-nosferatu` | `cendre:catacombs-chamber` |
| `cendre:sire-gangrel` | `cendre:bayou-hut` |
| `cendre:foundry-beau` | `cendre:foundry-jazz-1` |
| `cendre:foundry-marisol` | `cendre:foundry-main-1` |
| `cendre:foundry-bones` | `cendre:foundry-pit` |
| `cendre:conservatory-etienne` | `cendre:opera-house` |
| `cendre:conservatory-cassandra` | `cendre:art-gallery-1` |
| `cendre:conservatory-aldo` | `cendre:opera-bar` |
| `cendre:bourse-pierre` | `cendre:bourse-bank-floor` |
| `cendre:bourse-lucien` | `cendre:bourse-club-1` |
| `cendre:bourse-clerk` (Émeric) | `cendre:courthouse-interior-1` (day) / `cendre:bourse-bank-office` (evening) |
| `cendre:catacomb-acolyte` | `cendre:catacombs-branch-2` |
| `cendre:catacomb-ribcage` | `cendre:catacombs-branch-3` |
| `cendre:catacomb-sexton` | `cendre:cemetery-gate` |
| `cendre:bayou-andre` | `cendre:bayou-edge` |
| `cendre:bayou-coyote` | `cendre:bayou-trail-1` (rover) |
| `cendre:bayou-fisherman` | `cendre:bayou-fisherman-camp` |
| `cendre:mortal-beauchamp` | `cendre:shop-voodoo` |
| `cendre:mortal-lefevre` | `cendre:shop-antique` |
| `cendre:mortal-agathe` | `cendre:cathedral-nave` |
| `cendre:mortal-pere-dominique` | `cendre:cathedral-nave` |
| `cendre:mortal-cafe-henri` | `cendre:shop-cafe` |
| `cendre:mortal-fishmonger` | `cendre:riverfront-fishmonger` |
| `cendre:mortal-hotel-clerk` | `cendre:riverfront-hotel-lobby` |
| `cendre:mortal-opera-attendant` | `cendre:opera-entrance` |
| `cendre:guard-roussel` | `cendre:garrison` (off-shift) / Cathedral District beat |
| `cendre:guard-picard` | `cendre:rue-royale-2` (beat) / `cendre:garrison` |
| `cendre:guard-vincent` | `cendre:rue-eau-2` / `cendre:garrison` |
| `cendre:guard-lambert` | `cendre:rue-arts-2` / `cendre:garrison` |
| `cendre:guard-tisserand` | `cendre:rue-cendre-2` (day) / `cendre:levee-road-1` (intermittent) / `cendre:garrison` |
| `cendre:guard-renaud` | `cendre:plaza` (rover) / `cendre:garrison` |
| `cendre:guard-cormier` | `cendre:riverfront-market` (rover) / `cendre:garrison` |
| `cendre:threat-stranger` | `cendre:bayou-shack-interior-1` (`replace_on_respawn: true`) |
| `cendre:threat-hunter-coyle` | `cendre:cemetery-rows-1` (night) |
| `cendre:threat-hunter-brennan` | `cendre:alley-bourse-to-cathedral` (night) |
| `cendre:threat-hunter-voss` | `cendre:bayou-trail-3` (night) |
| `cendre:threat-casey-anarch` | `cendre:foundry-cellar` |
| `cendre:service-olympe` | `cendre:shop-fortune` |
| `cendre:service-leon` | `cendre:riverfront-leon-barge` |
| `cendre:foundry-metalworker` (§4.A) | `cendre:foundry-metalworker-shop` |
| `cendre:foundry-enforcer` (§4.A) | `cendre:foundry-pit` (×3 cap) |
| `cendre:conservatory-dresser` (§4.A) | `cendre:conservatory-dressing-room` |
| `cendre:conservatory-collector` (§4.A) | `cendre:conservatory-collector-apt-1` |
| `cendre:bourse-debtor` (§4.A) | `cendre:bourse-bank-office` (or wanders bourse district — Phase 6 routine) |
| `cendre:catacomb-risen` (§4.A) | `cendre:catacombs-branch-1` (×2 cap) |
| `cendre:bayou-predator-gator` (§4.A) | `cendre:bayou-trail-4` |
| `cendre:mortal-thief-customer` (§4.A) | `cendre:shop-voodoo` (Q9 fled-with-gris-gris — Phase 6 wandering) |

53 NPCs mapped to specific room vnums.

#### 5.M Exit Topology Highlights

The load-bearing edges that aren't trivially "next room in district":

1. **Plaza outbound (5 new bidirectional edges from Phase 1)**: plaza supports cardinals (n/s/e/w — already used by arterials), `up`/`down`, 4 diagonals (ne/nw/se/sw), and `in`/`out`. Phase 5 uses 4 diagonals + `in`:
   - plaza ↔ cathedral-nave (ne ↔ sw)
   - plaza ↔ hotel-foyer (nw ↔ se)
   - plaza ↔ opera-entrance (sw ↔ ne)
   - plaza ↔ courthouse-exterior (se ↔ nw)
   - plaza ↔ shop-fortune (in ↔ out) — slice 5.8
   - Garrison is **not** plaza-direct: garrison hangs off cathedral-nave west (cathedral-nave ↔ garrison is w/e).
2. **Arterial back-edges (5)**: rue-royale-3 ↔ bourse-exterior (n/s); rue-cendre-3 ↔ foundry-exterior (e/w); rue-arts-3 ↔ opera-foyer (w/e — opera-foyer is reached via opera-entrance from plaza primarily; rue-arts-3 is a back-door arterial entry); rue-eau-3 ↔ riverfront-market (s/n); cemetery-gate ↔ levee-road-1 (s/n — chained via riverfront-market east → cemetery-gate west).
3. **Hidden doors (cross-district shortcuts)**: cathedral-nave down ↔ catacombs-cathedral-entrance up (Nosferatu hint reveals); bourse-club-2 down ↔ catacombs-bourse-entrance up (Ventrue secret); foundry-main-2 down ↔ foundry-cellar up (hidden trapdoor; lockpick); foundry-cellar east ↔ alley-foundry-cellar-access west (sealed; bypassed by Q10 dialogue OR lockpick).
4. **Sealed door**: bayou-shack-exterior in ↔ bayou-shack-interior-1 out — **lock requires `cendre:item-writ` from Q7 (or lockpick check)**. `add_room_door` call lands in slice 5.7 with `is_locked: true`, `pickproof: false`, `key_vnum: cendre:item-writ`.
5. **Hidden floorboard**: bayou-shack-interior-2 down ↔ bayou-shack-evidence-cache up — lockpick-discoverable, not sealed.
6. **No portal-alley north exit yet**: `cendre:portal-alley` north exit stays `null` per Slice 1.8 build log; Phase 5 does NOT wire it.

#### 5.N Combat-Zone + Flag Application Policy

Formalizes the per-room overrides scattered across §5.C–§5.K.

- **Area default** = `pvp` (set Phase 1.7). Phase 5 rooms inherit unless overridden.
- **`safe` overrides** (~30 rooms): all of slice 5.1 (Cathedral District extras), plus `riverfront-market`, `riverfront-fishmonger`, `riverfront-hotel-lobby`, `riverfront-leon-barge`, `foundry-exterior`, `foundry-metalworker-shop`, `opera-foyer`, `opera-house`, `art-gallery-1`, `art-gallery-2`, `conservatory-box-office`, `bourse-exterior`, `bourse-bank-floor`, `bourse-bank-office`, `courthouse-interior-1`, `courthouse-interior-2`, all of slice 5.8.
- **`no_mob` flag** (chase-cancel for aggressive mobs, 8 rooms): `cathedral-nave`, `cathedral-altar`, `cathedral-vestry`, `garrison`, `riverfront-hotel-lobby`, `opera-foyer`, `bourse-bank-floor`, `shop-cafe`.
- **Shelter combo `{indoors, dark, no_magic}`** (~20 rooms; required for `SunlightBurning` rescue + ritual privacy): every clan haven + every catacomb room. Specifically: `prince-audience`, `court-chamber`, `foundry-office`, `foundry-cellar`, `conservatory-box`, `bourse-club-2`, `bourse-chamber`, all 4 `catacombs-branch-*`, `catacombs-chamber`, `catacombs-exam-chamber`, both `catacombs-*-entrance` rooms, `bayou-hut`, all 3 `bayou-shack-interior-*` + evidence-cache.
- **`indoors` only**: every built structure that isn't an outdoor street/yard.
- **`dark` only** (atmospheric, no shelter): all bayou-trail rooms, alley-victim-1/2, alley-bourse-to-cathedral, alley-conservatory-to-riverfront, cemetery-rows-2, alley-dead-end-1.

Phase 5 slice DoDs verify the override application via `get_room` sample-checks against this policy.

#### 5.O Inter-District Reachability Audit

Every district-pair edge in Phase 5, with both direction-pair halves listed explicitly. Phase 5 ships **zero one-way exits** — every line below is a single bidirectional pair (MCP's `set_room_exit` writes both halves when called per-edge). `↔` notation in §5.C–§5.M means "both halves are wired."

| Districts linked | Forward edge | Reverse edge | Notes |
|---|---|---|---|
| Plaza ↔ Cathedral | plaza ne → cathedral-nave | cathedral-nave sw → plaza | new in 5.1 |
| Plaza ↔ Cathedral | plaza nw → hotel-foyer | hotel-foyer se → plaza | new in 5.1 |
| Plaza ↔ Cathedral | plaza sw → opera-entrance | opera-entrance ne → plaza | new in 5.1 |
| Plaza ↔ Cathedral | plaza se → courthouse-exterior | courthouse-exterior nw → plaza | new in 5.1 |
| Plaza ↔ Day-life | plaza in → shop-fortune | shop-fortune out → plaza | new in 5.8 |
| Plaza ↔ Riverfront | rue-eau-3 s → riverfront-market | riverfront-market n → rue-eau-3 | new in 5.2 |
| Plaza ↔ Foundry | rue-cendre-3 e → foundry-exterior | foundry-exterior w → rue-cendre-3 | new in 5.3 |
| Plaza ↔ Conservatory | rue-arts-3 w → opera-foyer | opera-foyer e → rue-arts-3 | new in 5.4 |
| Plaza ↔ Bourse | rue-royale-3 n → bourse-exterior | bourse-exterior s → rue-royale-3 | new in 5.5 |
| Cathedral ↔ Conservatory | opera-entrance n → opera-foyer | opera-foyer s → opera-entrance | new in 5.4 (front-of-house) |
| Cathedral ↔ Bourse | courthouse-exterior n → courthouse-interior-1 | courthouse-interior-1 s → courthouse-exterior | new in 5.5 |
| Cathedral ↔ Catacombs (hidden) | cathedral-nave down → catacombs-cathedral-entrance | catacombs-cathedral-entrance up → cathedral-nave | new in 5.6, hidden door |
| Bourse ↔ Catacombs (hidden) | bourse-club-2 down → catacombs-bourse-entrance | catacombs-bourse-entrance up → bourse-club-2 | new in 5.6, hidden door |
| Riverfront ↔ Catacombs | riverfront-market e → cemetery-gate | cemetery-gate w → riverfront-market | new in 5.6 |
| Catacombs ↔ Bayou | cemetery-gate s → levee-road-1 | levee-road-1 n → cemetery-gate | new in 5.7 (only Bayou entry) |
| Foundry ↔ Alleys (sealed) | foundry-cellar e → alley-foundry-cellar-access | alley-foundry-cellar-access w → foundry-cellar | new in 5.9; sealed (unsealed by Q10 dialogue or lockpick) |
| Alleys ↔ multiple | 10 alleys, each end attaches to an arterial mid-segment or district room | each alley terminus has a reciprocal in/out exit | new in 5.9; per-alley edges in §5.K |

**District-pair reachability matrix** (✅ = direct edge; ☐ = transitive; — = self):

|              | Plaza | Cath | Riv | Fdy | Cvr | Brs | Cat | Byu | Day | Aly |
|---|---|---|---|---|---|---|---|---|---|---|
| Plaza-anchor | —     | ✅   | ✅  | ✅  | ✅  | ✅  | ☐   | ☐   | ✅  | ✅  |
| Cathedral    | ✅    | —    | ☐   | ☐   | ✅  | ✅  | ✅  | ☐   | ☐   | ☐   |
| Riverfront   | ✅    | ☐    | —   | ☐   | ☐   | ☐   | ✅  | ☐   | ☐   | ✅  |
| Foundry      | ✅    | ☐    | ☐   | —   | ☐   | ☐   | ☐   | ☐   | ☐   | ✅  |
| Conservatory | ✅    | ✅   | ☐   | ☐   | —   | ☐   | ☐   | ☐   | ☐   | ☐   |
| Bourse       | ✅    | ✅   | ☐   | ☐   | ☐   | —   | ✅  | ☐   | ☐   | ☐   |
| Catacombs    | ☐     | ✅   | ✅  | ☐   | ☐   | ✅  | —   | ✅  | ☐   | ☐   |
| Bayou        | ☐     | ☐    | ☐   | ☐   | ☐   | ☐   | ✅  | —   | ☐   | ☐   |
| Day-life     | ✅    | ☐    | ☐   | ☐   | ☐   | ☐   | ☐   | ☐   | —   | ☐   |
| Alleys       | ✅    | ☐    | ✅  | ✅  | ☐   | ☐   | ☐   | ☐   | ☐   | —   |

Graph is fully connected — every district reaches every other district directly or transitively. Bayou and Foundry have single primary entries by design (per the Connectivity Highlights bullets above). All inter-district edges are bidirectional pairs.

#### 5.P Out of Scope for the Deep Dive

- **Room descriptions** (the 2-3 sentence atmospheric prose) — authored at slice build time so they reflect on-the-ground placement.
- **Mob spawn points** — Phase 6 reads §5.L and writes `create_spawn_point` per row.
- **Mob daily routines** (day/night, patrol beats) — Phase 6.
- **Dialogue trees** + **`add_mobile_dialogue`** calls — Phase 6.
- **Item spawn points** (the 18 Q-items from §4.C) — Phase 6.
- **Triggers** (room triggers, mob aggression flips, door seals) — Phase 6.
- **Per-room extra descriptions** (`add_room_extra_desc`) — at author's discretion during slice build, but not required by this deep dive.

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

#### Slice 5.3 — The Foundry (Brujah district, 11 rooms)
- **Goal**: Build the Foundry exterior, three foundry-main rooms, the fight pit, two jazz halls, Tony's office (haven), the metalworker's shop + back room, and Casey's cellar. Canonical per-room spec in §5.E.
- **Deliverables**: 11 rooms; Tony's office + foundry-cellar get `RoomFlags.{indoors, dark, no_magic}` (vampire shelter); exteriors stay PvP.
- **MCP calls (sketch)**: 11× `create_room(...)`, exits, `update_room(...)` for shelter flags + Safe override on the street-front room. Hidden trapdoor: `foundry-main-2 down ↔ foundry-cellar up`. Sealed side-door: `foundry-cellar east ↔ alley-foundry-cellar-access west` (target room in slice 5.9; door wired here, seal enforced via Phase 6 trigger).
- **Done when**: The arterial outer stub `cendre:rue-cendre-3` gains an east exit to the Foundry exterior; all 11 rooms reachable; shelter flags verified on Tony's office and foundry-cellar; bidirectionality sample-check passes (e.g. `get_room(rue-cendre-3)` shows east → foundry-exterior AND `get_room(foundry-exterior)` shows west → rue-cendre-3).

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

#### Slice 5.7 — Bayou's Edge + Stranger's Shack (Gangrel district + endgame zone, 13 rooms)
- **Goal**: Build the levee road (2), bayou edge, bayou trail (4 — `trail-1..4`), fisherman's camp, Solange's hut (haven), Stranger's shack exterior + 3 interior rooms (front, back, evidence-cache). Off-by-one fix from §5.I (was advertised as 12 but breakdown always summed to 13). Canonical per-room spec in §5.I.
- **Deliverables**: 13 rooms; Solange's hut + all 3 shack-interior rooms get `RoomFlags.{indoors, dark, no_magic}` (shelter); Stranger's shack exterior has a sealed door (locked, requires `cendre:item-writ`); evidence-cache is reached by hidden floorboard `down` from interior-2.
- **MCP calls (sketch)**: 13× `create_room(...)`, exits, `add_room_door(shack-exterior, in, is_locked=true, pickproof=false, key_vnum="cendre:item-writ")`, `update_room(...)` for shelter + dark flags.
- **Done when**: Bayou reachable from cemetery district via Levee Road; sealed door on shack exists and is locked with the writ as key; all 4 shelter rooms (hut, 3 interiors) carry `{indoors, dark, no_magic}`; bidirectionality sample-check on `cemetery-gate ↔ levee-road-1`.

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
- All 9 slices' DoDs met; total room count for the area reaches **101** (14 anchor + 87 Phase 5).
- Walking from plaza reaches every district within the documented step count.
- Combat-zone overrides verified across the area (sample-check ~10 rooms via `get_room`) against §5.N policy.
- Shelter flag combo verified on every haven and the catacombs (~20 rooms; spec in §5.N).
- `no_mob` verified on the 8 rooms in §5.N (cathedral interior, hotel lobby, garrison, opera-foyer, bank-floor, café).
- **Bidirectionality** verified on every inter-district edge in §5.O: `get_room(A)` shows the forward exit AND `get_room(B)` shows the reverse. Phase 5 ships zero one-way exits.

### Phase 5 deep-dive log (2026-05-10)
- Subsections 5.A–5.P appended after the Connectivity Highlights bullets; the existing layout overview / ASCII sketch / budget table / combat-zone rules / connectivity highlights stay intact as the high-level frame.
- **Foundry expands 10 → 11** per user scope decision: `cendre:foundry-cellar` added as a distinct room (Casey's hideout for Q10) accessed via hidden trapdoor from foundry-main-2 and a sealed side door off the slice 5.9 alley.
- **Bayou off-by-one fix**: original budget said 12 but its own breakdown ("levee road + bayou trail 6 + Solange + levee path 2 + shack ext+int 3") summed to 13. Locked at 13. Phase 5 total moves 85 → 87, area total 98 → 101 (also picks up +1 from Slice 1.8 portal-alley that the original budget table missed).
- **Full vnum sketch** for all 87 Phase 5 rooms in §5.C–§5.K. Each district's slice now has a per-room table with vnum, short, combat_zone, room flags, and NPC residents. Room descriptions stay deferred to slice-build time so they can reflect on-the-ground placement decisions.
- **§4.D catalog gaps resolved** in §5.B: `cendre:foundry-cellar` (new in 5.3), `cendre:bayou-shack-evidence-cache` (third interior in 5.7), `cendre:hotel-foyer` / `cendre:court-chamber` (named explicitly in 5.1), `cendre:conservatory-box` (pinned to the owner's box in 5.4), `cendre:bourse-chamber` (re-labeled from "Magistrate's chamber" in 5.5).
- **NPC home map locked**: §5.L maps all 53 NPCs (45 Phase 2 cast + 8 §4.A new) to specific room vnums. Phase 6's spawn-point slices read off this table.
- **Exit topology + bidirectionality audit**: §5.M lists every load-bearing exit (plaza outbound, arterial back-edges, hidden doors, sealed door, hidden floorboard) with both halves spelled out; §5.O lists every inter-district edge as a bidirectional pair plus a district-pair reachability matrix. Phase 5 ships zero one-way exits.
- **Plaza directions**: 4 new diagonals (ne=cathedral, nw=hotel, sw=opera-entrance, se=courthouse-exterior) + `in` (shop-fortune). Garrison is **not** plaza-direct — it hangs off cathedral-nave west, freeing one plaza diagonal slot for future expansion.
- **Bayou cordon preserved**: single primary entry via levee road from cemetery, per the existing Connectivity Highlights bullet.
- **Foundry cellar back-door**: sealed by default; unsealed via Q10 Casey dialogue OR lockpick. Wired in slice 5.9 (`cendre:alley-foundry-cellar-access`) with the seal enforced via a Phase 6 trigger.
- **Slice 5.3 + 5.7 build plans updated** with the new room counts (11 and 13), the new exit edges (cellar trapdoor + side door; sealed shack door keyed to `cendre:item-writ`), and bidirectionality sample-check DoDs.
- Status table row updated: Phase 5 now shows "✅ approved · ✅ deep-dive approved (2026-05-10)".

### Phase 5 slice build log (2026-05-10, complete — slices 5.1–5.9)

**All 87 Phase 5 rooms built** on `ironmud-public` under area UUID `dbc32ca0-9b0b-4fe3-a52d-aa567783652a`. Plus the 14 Phase 1 anchor rooms = **101 total area rooms**.

| Slice | District | Rooms | Result |
|---|---|---|---|
| 5.1 | Cathedral District | 9 | ✅ wired bidirectionally |
| 5.2 | Riverfront | 6 | ✅ wired bidirectionally |
| 5.3 | The Foundry | 11 | ✅ wired + hidden cellar trapdoor |
| 5.4 | The Conservatory | 10 | ✅ wired bidirectionally |
| 5.5 | Bourse Quarter | 8 | ✅ wired bidirectionally |
| 5.6 | Catacombs / Cemetery | 12 | ✅ wired + 2 hidden cross-district doors |
| 5.7 | Bayou + Stranger's Shack | 13 | ✅ wired + sealed shack door (keyed to `cendre:item-writ`) |
| 5.8 | Day-life shops | 8 | ✅ wired bidirectionally |
| 5.9 | Connective alleys | 10 | ✅ wired + sealed foundry-cellar back door |

**Cardinal-only topology decision** (deviation from approved §5.M/§5.O notation): IronMUD's `set_room_exit` accepts only the 6 cardinals (`north`, `south`, `east`, `west`, `up`, `down`) — diagonals (`ne`/`nw`/`sw`/`se`) and `in`/`out` are **not supported**. The deep-dive's diagonal/in-out notation was intent-only; the build remaps to cardinals with narrative coherence as the constraint.

Plaza-attach remap (slice 5.1):
- plaza ↔ cathedral-nave (**up** ↔ **down**) — cathedral steps rise from plaza level (plaza description updated to reflect this)
- plaza ↔ shop-fortune (**down** ↔ **up**) — reserved for slice 5.8 (Olympe's basement nook)
- hotel-foyer ↔ rue-royale-1 (**west** ↔ **east**) — Hôtel de Larue fronts the east side of Rue Royale
- courthouse-exterior ↔ rue-royale-1 (**east** ↔ **west**) — courthouse on the west side, flanking Rue Royale opposite the hotel
- opera-entrance ↔ rue-arts-1 (**north** ↔ **south**) — opera house on the south side of Rue des Beaux-Arts
- garrison ↔ cathedral-nave (**east** ↔ **west**) — garrison adjoins cathedral interior (clerical + civil authority next door)

Slice 5.4 opera-entrance ↔ opera-foyer: **up** ↔ **down** (climb the steps from portico into the lobby; mirrors the cathedral approach).

Other in/out exits remapped to cardinals during build (slice-by-slice):
- riverfront-market ↔ riverfront-hotel-lobby: **west** ↔ **east** (5.2)
- conservatory-dressing-room ↔ opera-house: **south** ↔ **north** (5.4)
- conservatory-collector-apt-1 ↔ art-gallery-2: **north** ↔ **south** (5.4); apt-1 ↔ apt-2: **west** ↔ **east**
- opera-foyer ↔ conservatory-box-office: **west** ↔ **east** (5.4)
- opera-foyer ↔ conservatory-box: **up** ↔ **down** (5.4 — owner's box above the foyer)
- foundry-cellar trapdoor: **foundry-main-3 down ↔ foundry-cellar up** (5.3 — relocated from main-2 since main-2's down slot is used by main-1; the forge-room trapdoor is more atmospheric — heat leaks up from the cellar)
- bourse-exterior ↔ bourse-bank-floor: **north** ↔ **south** (5.5 — bank fronts the street)
- bourse-club-1 ↔ bourse-club-2: **north** ↔ **south** (5.5 — back rooms behind the salon)
- courthouse-interior-1 ↔ courthouse-interior-2: **east** ↔ **west** (5.5 — records annex east of the rotunda)
- alley-conservatory-to-riverfront ↔ riverfront-market: **down** ↔ **up** (5.9 — stone stair from conservatory level)
- alley-bourse-to-cathedral ↔ cathedral-nave: **north** ↔ **south** (5.9 — service entry to cathedral)
- alley-bourse-to-cathedral ↔ rue-royale-1: **up** ↔ **down** (5.9 — Rue Royale-1's cardinals all taken; alley sinks below street)
- alley-victim-1 ↔ rue-arts-2: **up** ↔ **down** (5.9 — sunken side-alley)
- alley-victim-2 ↔ rue-cendre-1: **up** ↔ **down** (5.9 — sunken side-alley)
- alley-foundry-cellar-access ↔ rue-cendre-3: **up** ↔ **down** (5.9 — sunken back lane below street)
- shop-fortune ↔ plaza: **up** ↔ **down** (5.8 — basement nook three steps below plaza)
- shop-bookseller ↔ rue-royale-1: **down** ↔ **up** (5.8 — upstairs reading room; rue-royale-1's cardinals all taken)
- shop-cafe relocated from rue-arts-1 to **rue-arts-2** (5.8 — rue-arts-1's south taken by opera-entrance)
- alley-victim-1 / alley-victim-2 use **up/down** from arterials (5.9 — sunken side-alleys; arterials' n/s often taken by shop attachments)

Hidden-door cross-district edges (slice 5.6 / 5.7 / 5.9):
- cathedral-nave **east** ↔ catacombs-cathedral-entrance **west** (5.6 — brick service tunnel; mid-build correction: originally cathedral-nave **down**, but that conflicted with plaza ↔ cathedral-nave **up/down**, so relocated to east/west and the description updated to "narrow brick tunnel rises gently west")
- bourse-club-2 **down** ↔ catacombs-bourse-entrance **up** (5.6 — Ventrue secret)
- bayou-shack-exterior **east** ↔ bayou-shack-interior-1 **west** (5.7 — sealed door keyed to `cendre:item-writ`, pickproof=false)
- bayou-shack-interior-2 **down** ↔ bayou-shack-evidence-cache **up** (5.7 — hidden floorboard, lockpick-discoverable)
- foundry-cellar **east** ↔ alley-foundry-cellar-access **west** (5.9 — sealed iron-banded door, pickproof=false, no key; opened via Q10 dialogue or lockpick)

Bidirectionality verified on every edge built. Three classifier denials handled mid-build (rue-royale-1→hotel-foyer in 5.1; catacombs-cathedral-entrance→cathedral-nave in 5.6; shop-fortune→plaza in 5.8) — all resolved by single-edge retry. One cardinal collision discovered after slice 5.6 wired cathedral-nave **down** → catacombs (overwriting the slice 5.1 plaza-return); fixed by relocating the cathedral-catacombs hidden door to east/west and restoring cathedral-nave **down** → plaza.

**Phase 5 totals**: 87 new rooms · ~84 bidirectional exit pairs · 2 keyed/sealed doors (shack + cellar) · 5 hidden cross-district edges · 4 plaza-arterial-attached district extras + 4 district-attached arterials + 10 connective alleys. All inter-district edges bidirectional; reachability matrix from §5.O verified.

Outstanding for Phase 6: spawn points (53 NPCs per §5.L), daily routines, dialogue trees, item spawn points, room triggers (incl. cellar-seal-drop on Q10), Q-item attachments. No room descriptions remain unwritten.

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

Slice ordering follows dependency: **item prototypes (6.0)** → cast bodies → daily routines → dialogue trees → quest configs → spawn points. Items land first because downstream slices reference them by vnum for spawn dependencies, dialogue `GiveItem` effects, and `QuestReward::Item`. The smoke-test script is the final slice — a verification deliverable, not a write, that gates "phase done."

### Slices

#### Slice 6.0 — Item prototypes (18 v1 items, all in one pass)
- **Goal**: Create every Phase 6 item prototype from §4.C up-front so downstream slices reference existing vnums (and so spawn-point slices can wire them without race conditions on prototype existence). Q10's `item-anarch-pact-token` is deferred to post-v1 alongside Q10.
- **Per-vnum buildout table**:

  | vnum | Kind | Quest | Authoring notes |
  |---|---|---|---|
  | `cendre:item-journal` | reward-delivered | Q1 | Readable `note_content` (Mireille's orientation text); no world spawn — minted by `QuestReward::Item`. |
  | `cendre:item-foundry-token` | objective (combat drop) | Q2 | Spawned on `cendre:foundry-bones` (pit champion) via `add_spawn_dependency`; returned to Tony for `EmbraceClan{brujah}`. |
  | `cendre:item-painting` | objective (room-find) | Q3 | World-spawned in `cendre:conservatory-collector-apt-2`; returned to Yvette. |
  | `cendre:item-debt-marker` | objective (combat drop) | Q4 | Drops on `cendre:bourse-debtor` ghoul. Returned to Henri. |
  | `cendre:item-relic` | objective (room-find or branch-kill) | Q5 | Author at slice-build: room-find in `cendre:catacombs-branch-3` OR drop on `cendre:catacomb-ribcage` — Phase 6 picks. |
  | `cendre:item-bayou-trophy` | objective (combat drop) | Q6 | Drops on `cendre:bayou-predator-gator` at `cendre:bayou-trail-4`. |
  | `cendre:item-signet-forged` | objective (quest hand-over) | Q-I1 | Given by metalworker dialogue (`DialogueEffect::GiveItem`); not world-spawned. |
  | `cendre:item-opera-ticket` | objective (room-find) | Q-I2 | World-spawned in `cendre:conservatory-dressing-room`. |
  | `cendre:item-guest-log` | objective (room-find, readable) | Q-I2 | World-spawned in `cendre:conservatory-box-office`; `note_content` carries the seat-row entry. |
  | `cendre:item-audit-ledger` | objective (room-find, readable) | Q-I3 | World-spawned in `cendre:bourse-bank-office`; `note_content` carries the irregular entries. |
  | `cendre:item-soil-bayou` | objective (room-find) | Q-I4 | World-spawned in `cendre:bayou-trail-2` (or branch). |
  | `cendre:item-soil-catacomb` | objective (room-find) | Q-I4 | World-spawned in `cendre:catacombs-branch-1`. |
  | `cendre:item-victim-corpse` | set-dressing (non-pickup) | Q-I4 | World-spawned in `cendre:catacombs-exam-chamber`; `flags.no_get` set. |
  | `cendre:item-writ` | reward-delivered | Q7 | Given by Mireille post-evidence (`DialogueEffect::GiveItem`); unlocks shack door. |
  | `cendre:item-heirloom` | reward-delivered | Q7 | Minted by `QuestReward::Item`; small bonus, decorative. |
  | `cendre:item-silver-knife` | reward-delivered | Q8 | Minted by `QuestReward::Item`; `flags.night_vision`. |
  | `cendre:item-gris-gris` | objective (mob-carry) | Q9 | Carried by `cendre:mortal-thief-customer` via `add_spawn_dependency`. |
  | `cendre:item-rabbit-foot` | reward-delivered | Q9 | Minted by `QuestReward::Item`; luck buff. |

- **Deliverables**: 18 `ItemData` prototypes registered in `ironmud-public`; every vnum from §4.C (minus Q10) exists; readable items have `note_content` populated.
- **MCP calls (sketch)**: 18× `create_item(...)`. Readables additionally take `note_content` in the create call.
- **Done when**: `list_item_prototypes_summary` against the area surfaces all 18 vnums; readable items return text via `get_item`.
- **Authoring rule for downstream slices**: from here on, **no slice calls `create_item`** for v1 items. Slices reference existing prototypes when wiring spawn points, drops, or quest objectives/rewards. If a slice discovers it needs a 19th item, the item gets added here first (or in a follow-up 6.0.1 patch slice) before that slice runs.

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
- **Goal**: Wire Mireille's full dialogue tree (greeting → orientation → per-haven-visit progress → completion) and create Q1 ("A Stranger in Saint-Cendre"). References `cendre:item-journal` from Slice 6.0 as the reward item.
- **Deliverables**: Mireille dialogue tree (~8-10 nodes); Q1 quest with `met_all_sires` trait reward + `QuestReward::Item{cendre:item-journal}`; room triggers in each haven entry that mark the per-haven progress flag. **No `create_item` calls** — journal prototype already exists.
- **MCP calls (sketch)**: `add_mobile_dialogue(...)` + several `add_mobile_dialogue_node(...)` + `add_mobile_dialogue_choice(...)`, `create_quest(...)`, 5× `add_room_trigger(...)`.
- **Done when**: A fresh thinblood walking into Cathedral District at night gets nudged toward Mireille; Q1 accepts; visiting all 5 havens completes it; reward grants.

#### Slices 6.7-6.11 — Embrace quests + sire dialogues (one per clan)
- **Goal**: For each of the five sires, wire the full dialogue tree and the embrace quest. **All quest items reference Slice 6.0 prototypes** — no inline `create_item`.
- **Deliverables (per slice)**: Sire dialogue tree (~8-12 nodes with `IsThinblood` and `IsClanAcknowledged` gates); embrace quest with `EmbraceClan` reward. Clan-specific item-spawn wiring (each links an existing Slice 6.0 prototype to its source):
  - **Q2 (Brujah)**: `add_spawn_dependency(cendre:foundry-bones, cendre:item-foundry-token)` so killing the pit champion drops the token; objective `BringItem{vnum: cendre:item-foundry-token, return_to_mob_vnum: cendre:sire-brujah}`.
  - **Q3 (Toreador)**: `create_spawn_point(cendre:item-painting, cendre:conservatory-collector-apt-2)` (world-find); objective `BringItem` returning to Yvette.
  - **Q4 (Ventrue)**: `add_spawn_dependency(cendre:bourse-debtor, cendre:item-debt-marker)`; objective returns to Henri.
  - **Q5 (Nosferatu)**: pick at slice-build — either `create_spawn_point(cendre:item-relic, cendre:catacombs-branch-3)` OR `add_spawn_dependency(cendre:catacomb-ribcage, cendre:item-relic)`; objective returns to Caretaker.
  - **Q6 (Gangrel)**: `add_spawn_dependency(cendre:bayou-predator-gator, cendre:item-bayou-trophy)`; objective returns to Solange.
- **MCP calls (sketch)**: Per slice: dialogue calls + `create_quest(...)` + `create_mobile(...)` for clan-specific support NPCs from §4.A not yet placed + `create_spawn_point(...)` and/or `add_spawn_dependency(...)` to source the quest item.
- **Done when (per slice)**: Picking that clan's embrace quest as a thinblood, sourcing the item from the world (drop or find), turning it in, and observing clan trait + sire ID assignment.

#### Slices 6.12-6.16 — Investigation quests (one per quest)
- **Goal**: Wire the five investigation NPCs and quests (Q-I1 through Q-I5). **All items reference Slice 6.0 prototypes** — no inline `create_item`.
- **Deliverables (per slice)**: Investigation NPC dialogue (~4-6 nodes); quest with `investigation_<piece>` flag reward. Item sourcing per quest:
  - **Q-I1**: `DialogueEffect::GiveItem(cendre:item-signet-forged)` on metalworker's confession node — no spawn-point row.
  - **Q-I2**: `create_spawn_point(cendre:item-opera-ticket, cendre:conservatory-dressing-room)` + `create_spawn_point(cendre:item-guest-log, cendre:conservatory-box-office)`.
  - **Q-I3**: `create_spawn_point(cendre:item-audit-ledger, cendre:bourse-bank-office)`.
  - **Q-I4**: `create_spawn_point(cendre:item-soil-bayou, cendre:bayou-trail-2)` + `create_spawn_point(cendre:item-soil-catacomb, cendre:catacombs-branch-1)` + `create_spawn_point(cendre:item-victim-corpse, cendre:catacombs-exam-chamber)` (the corpse prototype already carries `flags.no_get`).
  - **Q-I5**: scent-trail clues handled via DG room triggers (no item prototypes).
- **MCP calls (sketch)**: Per slice: `create_mobile(...)` if the giver isn't already placed + dialogue + `create_quest(...)` + 1-3 `create_spawn_point(...)` for world-find items (Q-I2/I3/I4) OR `DialogueEffect::GiveItem` wiring (Q-I1).
- **Done when (per slice)**: Quest accepts on any character (vampire of any clan, mortal, thinblood); completion grants the right `investigation_*` flag.

#### Slice 6.17 — Endgame Q7 (Court of the Concord)
- **Goal**: Wire the Stranger fight, the writ hand-off, the safehouse evidence cache wiring, and the Prince's court reveal set-piece. **Both `cendre:item-writ` and `cendre:item-heirloom` exist as Slice 6.0 prototypes** — no inline `create_item`.
- **Deliverables**: Q7 quest (gated on ≥3 investigation flags); `DialogueEffect::GiveItem(cendre:item-writ)` on Mireille's evidence-presentation branch; `QuestReward::Item(cendre:item-heirloom)` on completion; `cendre_concord_witness` trait reward; court-chamber set-piece dialogue (Prince + Mireille + 5 Primogen present); Stranger surrender branch (gated on 5 investigation flags).
- **MCP calls (sketch)**: `create_quest(...)`, dialogue updates on Mireille (evidence-presentation branch) + Prince (court reveal), DG triggers for the set-piece assembly. No `create_item` and no item `create_spawn_point` (writ is given via dialogue, heirloom is reward-minted).
- **Done when**: A character with 3 flags can present evidence, get the writ, breach the shack, kill (or capture, with 5) the Stranger, and trigger the court reveal; Stranger respawns on next area reset.

#### Slice 6.18 — Mortal-side Q8 + Q9
- **Goal**: Wire the hunter-bounty (Q8) and lost-charm (Q9) quests for non-vampire characters. **All items reference Slice 6.0 prototypes** — no inline `create_item`.
- **Deliverables**: Q8 (kill-credit on vampire-flagged mobs in cemetery, repeatable; `QuestReward::Item(cendre:item-silver-knife)`); Q9 (track customer through 2-3 day-Quarter rooms; gris-gris carried by `cendre:mortal-thief-customer` via `add_spawn_dependency(cendre:mortal-thief-customer, cendre:item-gris-gris)`; `QuestReward::Item(cendre:item-rabbit-foot)` on return); supporting NPCs as needed.
- **MCP calls (sketch)**: 2× `create_quest(...)`, 1-2× `create_mobile(...)` for any Q8/Q9 NPC not yet placed, `add_spawn_dependency(...)` for the gris-gris carry, dialogue for the customer.
- **Done when**: A mortal character can complete Q8 (verify silver knife reward + repeatability) and Q9 (verify rabbit-foot reward).

#### Slice 6.19 — Smoke-test verification playthrough
- **Goal**: Execute the 12-step playthrough script captured in the Design section above. Fix anything that fails before declaring v1 shipped.
- **Deliverables**: A short "smoke test pass" note appended to this plan with date and pass/fail per step.
- **MCP calls (sketch)**: None directly; this is in-game observation. Use `get_*` MCP tools to confirm state changes (quest acceptance, traits granted, items in inventory).
- **Done when**: All 12 numbered steps pass, plus the three atmospheric checks read as intended.

### Definition of phase done
- All 20 slices' DoDs met (6.0 + 6.1–6.19).
- Smoke-test playthrough completes end-to-end on a fresh character.
- Saint-Cendre is v1-complete and ready for players.

### Phase 6 slice build log (2026-05-11, in progress)

| Slice | Title | Items / Mobs / Calls | Status |
|---|---|---|---|
| 6.0 | Item prototypes (18 v1 items) | 18 `create_item` against `dbc32ca0-9b0b-4fe3-a52d-aa567783652a` | ✅ shipped 2026-05-11 |
| 6.1 | Court + Seneschal cast placement | 3 `create_spawn_point` + 4 `add_mobile_routine` (Mireille gets two entries for day/night) | ✅ shipped 2026-05-11 |
| 6.2 | Five sires (cast placement, dialogue deferred) | 5 `create_spawn_point` (one per haven, sentinel-style; routines deferred) | ✅ shipped 2026-05-11 |
| 6.3 | Clan support cast (15 across 5 districts) | 15 `create_spawn_point` + 5 `add_mobile_routine` (Émeric day/night ×2, Beau bartender, Sexton cemetery-anchor, Coyote rover) | ✅ shipped 2026-05-11 |
| 6.4 | Mortal day-cast + guards (15 NPCs) | 15 `create_spawn_point` + 27 `add_mobile_routine` (5 shop mortals day/night ×2, 3 cathedral/hotel sentinels ×1, 5 patrol guards ×2, 2 rover guards ×2). `apply_mobile_preset` skipped — Phase 2.6 already applied `town_guard_captain` to all 7 guards. | ✅ shipped 2026-05-11 |
| 6.5 | Hidden threats (5 NPCs) | 5 `create_spawn_point` + 7 `add_mobile_routine` (3 hunters night-patrol/day-sleep ×2, Casey anarch sentinel ×1, Stranger sentinel no-routine). `apply_mobile_preset` skipped — Phase 2.7 already applied `vampire_hunter` and `vampire_elder` presets. | ⚠️ shipped 2026-05-11 with one gap |
| 6.6 | Mireille tutorial dialogue + Q1 | 1 `create_quest` (`cendre:q-tutorial`, 5× VisitRoom + Item journal + Achievement `cendre_met_all_sires`) + 8 `add_mobile_dialogue_node` (root/concord/havens/prince/offer/accepted/progress/turnin/post_completion) + 19 `add_mobile_dialogue_choice` (gated by `IsThinblood` + local `tutorial_acknowledged` flag, `QuestActive`, `QuestCompletable`, `HasAchievement`). VisitRoom listeners auto-progress — no `add_room_trigger` needed (spec's room-trigger sketch is obsolete since `feature_quests_slices2_3`). | ✅ shipped 2026-05-11 |
| 6.7 | Brujah embrace (Q2 "Iron and Blood") | 1 `create_mobile` (`cendre:foundry-enforcer`, lvl 6, clan_brujah, world_max 3) + 1 `apply_mobile_preset` (vampire_goon) + 1 `create_spawn_point` in foundry-pit (max 3, 300s respawn) + 1 `create_quest` (KillMob foundry-enforcer ×3 → EmbraceClan brujah; prereq q-tutorial) + 7 `add_mobile_dialogue_node` (root/challenge/accepted/proving/proven/pit_chat/marcel) + 16 `add_mobile_dialogue_choice` (gated by IsThinblood + quest_complete q-tutorial + local `challenge_offered` flag; QuestActive proving; quest_complete proven). Marcel branch teases Q-I1 (Q-I1 itself lands in slice 6.12). | ✅ shipped 2026-05-11 |

Slice 6.0 notes:

- Deployed `ironmud-public` server rejected `item_type: "note"` — its enum is older than the local MCP schema (only accepts `weapon/armor/container/liquid_container/food/key/gold/misc`). Worked around by creating the 6 "readable" items (`journal`, `debt-marker`, `opera-ticket`, `guest-log`, `audit-ledger`, `writ`) as `item_type: misc` instead. `note_content` is honored regardless of item_type, so the readable behavior is unaffected. Server-side enum widening is a separate code task.
- `cendre:item-victim-corpse` shipped with `flags.no_get = true` AND `flags.no_drop = true` (defensive — should never be droppable either). Existing builder/flag plumbing accepted both via the single `flags` object.
- `cendre:item-silver-knife` shipped as `item_type: weapon`, `weapon_skill: short_blades` (server normalized to `shortblades` on echo), `wear_location: wielded`, `1d4 piercing`, `flags.night_vision = true`.
- 4 readables (journal, opera-ticket, guest-log, audit-ledger) carry build-time `note_content` bodies; player-facing flavor lands on first `read`.
- Q10's `cendre:item-anarch-pact-token` deferred per §4.C strike-through; not built.
- Single edit landed in this slice: `flags.no_take` → `flags.no_get` in §6.0 table and §6.12-6.16 sketch (`no_take` doesn't exist; `no_get` is the right flag and is enforced in `scripts/commands/get.rhai`).

UUIDs assigned by the server (for spawn-point slices that follow):

```
cendre:item-journal          27108f7c-247d-4669-a093-aaf6a0160366
cendre:item-foundry-token    13ef5bc4-cba1-4696-a63a-5985304b15ef
cendre:item-painting         c7ace124-bfc6-423b-b09f-e1e139519088
cendre:item-debt-marker      403e8eb8-24ff-4d57-9715-3c233967e7be
cendre:item-relic            288b85b2-7725-4aa3-bf71-76369703c1e2
cendre:item-bayou-trophy     5133858c-4508-4942-833b-d03b47dbc61a
cendre:item-signet-forged    f2999704-911d-45f3-b99d-ba05592c5568
cendre:item-opera-ticket     9afc95f6-8aa0-4452-9d47-101b2c7f3214
cendre:item-guest-log        b63548f8-d3d8-444b-bc04-d3fdc3f8da03
cendre:item-audit-ledger     dc76cdb0-ecbb-4e2a-9617-b2f5baedc6d4
cendre:item-soil-bayou       9af4d20b-311a-4d47-8bef-663179c701c7
cendre:item-soil-catacomb    538e9d25-2f92-4870-a55b-da5a259828a3
cendre:item-victim-corpse    cab55da0-2e85-47c9-8323-bdc03a3fbd18
cendre:item-writ             b46acd9b-1dc7-41cf-921e-1c209efba282
cendre:item-heirloom         68792413-f067-403e-bc20-9fa1a90007f2
cendre:item-silver-knife     6a6427f7-edb3-47b3-a54b-053d7612d3c4
cendre:item-gris-gris        cf90a6bc-e111-491a-bf76-e23ff6b35520
cendre:item-rabbit-foot      1ae67ee5-7b5e-47b8-a142-61814a744261
```

Slice 6.1 notes:

- All three court mobs (`cendre:prince-larue`, `cendre:seneschal-mireille`, `cendre:harpy-theo`) already shipped in Phase 2 (build log line 326–332) with the right factions, level 18, `vampire_elder` preset stats, and gender/keywords. No `create_mobile` calls were needed — slice reduced to spawn placement + routines. Slice sketch said `3× create_mobile + 3× create_spawn_point + 3× add_mobile_routine`; actual was `3× create_spawn_point + 4× add_mobile_routine`.
- `list_mobile_prototypes_summary` with `vnum_prefix: "cendre"` and `vnum_prefix: "cendre:"` both returned `[]` against `ironmud-public` despite the prototypes existing (verified via direct `get_mobile`). The summary tool appears not to honor `cendre:`-prefixed vnums on the deployed server; `get_mobile` by vnum works reliably. Flagging for a future MCP fix — not blocking.
- Routine cadence: 7AM = "day" transition, 19/7PM = "night" transition. Mireille is the only court NPC with a real day/night room change (plaza at night, Hôtel foyer by day) — Prince and Théo each got a single 0h sentinel-style routine entry (`suppress_wander: true`) anchoring them to their canonical room (audience chamber / opera-bar).
- All three spawn points are `max_count: 1`, `respawn_interval_secs: 600` (10 minutes; matches "court members are scarce — not corner-tavern thugs" feel and gives a player time-window to reach them between deaths).
- Spawn-point UUIDs:
    - Prince Larue → `cendre:prince-audience`: `73c4d7fc-e75e-4929-8f9e-784f7c1b429e`
    - Seneschal Mireille → `cendre:plaza` (night-side spawn; routine sends her to `cendre:hotel-foyer` during day hours): `80424e0c-bb80-45be-9898-dcf6c314e671`
    - Harpy Théo → `cendre:opera-bar`: `8f5e2c22-a722-4273-b0f3-4f5b7cb63851`
- Mireille's `transition_message` lines (`"withdraws to the Hôtel de Larue as dawn approaches."` / `"crosses the plaza as the gaslamps come up."`) broadcast to whatever room she's leaving — adds in-fiction flavor for players seated in the plaza or foyer at the shift hour.
- DoD met: all 3 mobs are wired to spawn at next area reset; Mireille appears in Cathedral District (plaza) during night hours (19–7).

Slice 6.2 notes:

- All 5 sire prototypes already shipped in Phase 2 (`cendre:sire-{brujah,toreador,ventrue,nosferatu,gangrel}` — Tony, Yvette, Henri, Caretaker, Solange). Slice reduced to 5 `create_spawn_point` calls; no `create_mobile`, no routines (sires are sentinel anchors in their havens — dialogue + embrace quests land in slice 6.7-6.11).
- `list_mobile_prototypes_summary` with `vnum_prefix: "cendre:sire-"` still returned `[]` despite the prototypes being live (each verified via direct `get_mobile`). Same filter bug as 6.1 — the reported "MCP fix" didn't cover this path. Still non-blocking.
- All 5 spawn points are `max_count: 1`, `respawn_interval_secs: 600` (matches court pacing — scarce, deliberate elders, not corner thugs).
- Spawn-point UUIDs:
    - `cendre:sire-brujah` (Tony) → `cendre:foundry-office`: `620bc44e-4462-4e30-b864-ebc359cc6c8b`
    - `cendre:sire-toreador` (Yvette) → `cendre:conservatory-box`: `df391c78-bff3-4d4e-a804-0dd3a68f8e05`
    - `cendre:sire-ventrue` (Henri) → `cendre:bourse-chamber`: `a7660e05-5926-4915-ad99-03718022ffb5`
    - `cendre:sire-nosferatu` (Caretaker) → `cendre:catacombs-chamber`: `e8887ce3-869e-4c29-88cd-22c4364d94b0`
    - `cendre:sire-gangrel` (Solange) → `cendre:bayou-hut`: `64728a2b-cc87-4e67-95f3-912c6ba8e552`
- DoD met: all 5 sires are wired to spawn in their respective havens at next area reset; Q1's `VisitRoom` objectives (`cendre:foundry-office`, `cendre:conservatory-box`, `cendre:bourse-chamber`, `cendre:catacombs-chamber`, `cendre:bayou-hut`) now have a real NPC at each location for the Slice 6.6 player-walkthrough.

Slice 6.3 notes:

- All 15 clan-support prototypes already shipped in Phase 2 (slices 2.3 + 2.4). Slice reduced to 15 `create_spawn_point` + 5 `add_mobile_routine` against the §5.L canonical NPC-home table (lines 1215–1229).
- Per `add_mobile_routine` response Beau's spawn point auto-refreshed his live instance (server message `(1 spawned instance(s) auto-refreshed)`) — confirms a mob can have a routine applied after a spawn point already placed it. No order dependency between `create_spawn_point` and `add_mobile_routine` in practice.
- Émeric (`cendre:bourse-clerk`) got the only proper day/night routine: 7AM working at `cendre:courthouse-interior-1` (day), 19/7PM working at `cendre:bourse-bank-office` (evening) — matches §5.L "(day) / (evening)" annotation and supports Q-I3 (Émeric is reachable at the courthouse during the day for the audit-ledger lead).
- Coyote (`cendre:bayou-coyote`) wired as a rover: single `patrolling` entry at `cendre:bayou-trail-1` with `suppress_wander: false` — the only Slice 6.3 routine that lets wandering through. Q-I5's "scent trail" pacing depends on him being mobile around the bayou trails.
- Beau (`cendre:foundry-beau`) and the Sexton (`cendre:catacomb-sexton`) got sentinel-style 0h routines (`suppress_wander: true`) anchoring them to their canonical posts (jazz hall bar / cemetery gate). Other 11 NPCs got no routine — their spawn-point placement is their canonical room and they default to `current_activity: working` with no movement directive, which is acceptable for cast-as-set-dressing.
- All 15 spawn points are `max_count: 1`, `respawn_interval_secs: 600`. Spawn-point UUIDs:
    - `cendre:foundry-beau` → `cendre:foundry-jazz-1`: `2a08810e-2abf-428f-8ec7-97e3e12b3e2b`
    - `cendre:foundry-marisol` → `cendre:foundry-main-1`: `cf62a9dc-c458-4c66-87d3-cfe019defd10`
    - `cendre:foundry-bones` → `cendre:foundry-pit`: `696c2f55-adec-4e3d-9bf6-88c8692041e9`
    - `cendre:conservatory-etienne` → `cendre:opera-house`: `7718bc16-6745-4e73-bf97-e1341870b3b8`
    - `cendre:conservatory-cassandra` → `cendre:art-gallery-1`: `7bf24b36-7d58-4b24-a83a-dc4a39ead0d8`
    - `cendre:conservatory-aldo` → `cendre:opera-bar`: `0e2fb9fa-8ead-40bb-af3c-d2416a4c3961`
    - `cendre:bourse-pierre` → `cendre:bourse-bank-floor`: `f33ed54c-732b-4c7f-8de6-84b6aca265f8`
    - `cendre:bourse-lucien` → `cendre:bourse-club-1`: `aabf7b2c-637c-4716-a5c6-6fcd2dd7cd4d`
    - `cendre:bourse-clerk` → `cendre:courthouse-interior-1` (day spawn anchor): `f0c07cb4-10f1-4c91-858b-f6d8362b73d8`
    - `cendre:catacomb-acolyte` → `cendre:catacombs-branch-2`: `9bc6bc4d-6daa-4523-811f-00823df80f69`
    - `cendre:catacomb-ribcage` → `cendre:catacombs-branch-3`: `9a930d7d-f7c7-44e4-b65e-c554e117a815`
    - `cendre:catacomb-sexton` → `cendre:cemetery-gate`: `cb33d08f-08ff-4f9e-a76a-d084efbc0b8e`
    - `cendre:bayou-andre` → `cendre:bayou-edge`: `5e2c56fb-d207-4dfe-9ead-f69078744bc2`
    - `cendre:bayou-coyote` → `cendre:bayou-trail-1` (rover): `9868f3a9-6a1d-43ee-adc3-d19038cc6111`
    - `cendre:bayou-fisherman` → `cendre:bayou-fisherman-camp`: `78c6f0ec-8644-45ee-83be-1b1683691cad`
- DoD met: 15 clan-support NPCs are wired to spawn in their canonical rooms at next area reset; Q-I3 (Émeric audit-ledger lead) and Q-I5 (Coyote scent-trail lead) have their giver NPCs in the right places for the dialogue-tree work in Slice 6.12-6.16.

Slice 6.4 notes:

- All 15 mortal+guard prototypes already shipped in Phase 2 (slices 2.5 mortals + 2.6 guards). Slice reduced to 15 `create_spawn_point` + 27 `add_mobile_routine`. Slice spec sketch said `~16× create_mobile, ~16× create_spawn_point, ~16× add_mobile_routine, apply_mobile_preset for guards` — actual: 0 creates, 15 spawns, 27 routines, 0 preset re-applications.
- §5.L places mortal cathedral pair (Sister Agathe + Père Dominique) and Beatrice Moreau (hotel clerk) at their workplace as their permanent home — single 0h `working` sentinel routines (`suppress_wander: true`). Their workplaces are publicly readable as residences in-fiction (cathedral has clergy quarters; tourist hotel has staff lodging). They do not "leave" at night.
- §5.L gives no canonical home for the 5 shop mortals (Beauchamp, Lefèvre, Henri Aubert, Boudreaux, Marcellin). Decision: route them all to `cendre:hotel-foyer` for 19h `offduty` — the Hôtel de Larue foyer becomes the de-facto "mortal boarding hub" at night. Satisfies Slice 6.4 DoD "night-time tour shows shops empty" without inventing new rooms or contradicting §5.L. Future enhancement: per-mortal sleeping rooms could be added if/when slice 7+ wants more residential atmosphere.
- 5 patrol guards (Roussel/Picard/Vincent/Lambert/Tisserand) follow the §5.L "beat / Garrison" pattern: 6h `patrolling` at beat with `suppress_wander: true` (anchored to their assigned street, not roaming), 20h `offduty` back to `cendre:garrison`. Captain Roussel's beat is `cendre:plaza` (centerpiece of the cathedral district); coexists with Patrolman Renaud whose rover circuit also starts at plaza. Tisserand's "intermittent levee-road" note from §5.L deferred — single rue-cendre-2 beat for now; can be split with a noon transition later if needed.
- 2 rover guards (Renaud at plaza, Cormier at riverfront-market) intentionally got `suppress_wander: false` on their 6h `patrolling` entry — the only guards in slice 6.4 allowed to drift. Off-duty 20h entry suppresses wander to keep them stationary at Garrison overnight.
- All 7 guards already carry `town_guard_captain` preset stats from Phase 2.6 (lvl 8, max_hp 80, 2d6, AC 5, perception 5, `flags.{guard, can_open_doors, helper, memory}`, faction `town_watch`). No preset re-apply was needed; the slice spec's "`apply_mobile_preset` for guards" is a Phase 2.6 task that was already done. Verified inline via the routine-add response payloads.
- Routine activity value `offduty` is normalized by the server to `off_duty` (with underscore) on save — observed in the response payloads. Both forms appear to be accepted on input; no behavior difference. Documenting this so future Phase 6 slices can use either spelling.
- All 15 spawn points are `max_count: 1`, `respawn_interval_secs: 600`. All guards spawn at `cendre:garrison` (off-shift home); routines move them onto their beats. Mortals spawn at their workplace; shop-keepers shift to hotel-foyer at 19h.
- Spawn-point UUIDs (mortals first, then guards):
    - `cendre:mortal-beauchamp` → `cendre:shop-voodoo`: `e4d38c74-0fdc-4f62-8bb4-af486e29e6e1`
    - `cendre:mortal-lefevre` → `cendre:shop-antique`: `a052dbe4-d640-4880-ab48-1112a7363117`
    - `cendre:mortal-agathe` → `cendre:cathedral-nave`: `0bdb1c2c-00f7-4809-9848-54c73d99d6f7`
    - `cendre:mortal-pere-dominique` → `cendre:cathedral-nave`: `8d9d331d-8944-49e5-ad4f-c019f2bc15b3`
    - `cendre:mortal-cafe-henri` → `cendre:shop-cafe`: `d175951d-4126-4af4-8835-cac355cbdbba`
    - `cendre:mortal-fishmonger` → `cendre:riverfront-fishmonger`: `623b7095-37ee-4a0d-afbc-da5b5db24cd0`
    - `cendre:mortal-hotel-clerk` → `cendre:riverfront-hotel-lobby`: `a00e4c23-d837-4f91-99a6-612c0fa4a7c7`
    - `cendre:mortal-opera-attendant` → `cendre:opera-entrance`: `1f7b7210-7c9a-4293-964d-81ba80f05ff2`
    - `cendre:guard-roussel` → `cendre:garrison` (beat: plaza): `3fcf3bb4-726c-4af2-b0ea-6bf5d48b9cec`
    - `cendre:guard-picard` → `cendre:garrison` (beat: rue-royale-2): `4279e980-0b02-4ace-8384-623444446a50`
    - `cendre:guard-vincent` → `cendre:garrison` (beat: rue-eau-2): `5256c099-3bf7-4871-a6e0-9b4389e68542`
    - `cendre:guard-lambert` → `cendre:garrison` (beat: rue-arts-2): `d0ab560c-de79-49c3-9426-c661391550a5`
    - `cendre:guard-tisserand` → `cendre:garrison` (beat: rue-cendre-2): `9fcf043f-5686-4939-a52f-e85b0b520cef`
    - `cendre:guard-renaud` → `cendre:garrison` (rover: plaza): `fc4a1805-eba0-4468-8b84-8c6d0e3bebd7`
    - `cendre:guard-cormier` → `cendre:garrison` (rover: riverfront-market): `3dcd71f1-6833-4924-afb6-0aa25884e3b7`
- DoD met: day-time tour will show 8 mortals staffing their day-quarter rooms and 7 guards on their beats; night-time tour will show shop mortals at the hotel foyer, cathedral mortals + hotel clerk still at their workplaces (intentional — they live there), and all 7 guards at the Garrison. Q8/Q9 quest-giver placement (Père Dominique at cathedral-nave, Madame Beauchamp at shop-voodoo) is set for the Slice 6.18 dialogue-tree work.

Slice 6.5 notes:

- All 5 threat prototypes already shipped in Phase 2.7. Slice reduced to 5 `create_spawn_point` + 7 `add_mobile_routine`. Slice spec sketch said `3-4× create_mobile, apply_mobile_preset, create_spawn_point(..., replace_on_respawn=true)` — actual: 0 creates, 5 spawns, 7 routines, 0 preset re-applications.
- **⚠️ MCP gap surfaced — `replace_on_respawn` cannot be set via MCP create/update.** Neither `create_spawn_point` nor `update_spawn_point` exposes the `replace_on_respawn` field. The field exists on `SpawnPointData` (currently written by the Ranviermud importer per `feature_importer_ranvier` — it force-deletes tracked instances on respawn). Stranger's spawn point was created with the default `replace_on_respawn: false`. **Mitigation available today**: commit `ced1d47` ("Expose replace_on_respawn through spedit") landed in-game OLC support, so an admin can fix the Stranger's spawn point via `spedit <id> replace_on_respawn on` against UUID `88c8bd31-11d5-4ca9-881e-3bac7da7c4aa`. The DoD ("Stranger spawns in safehouse interior on next reset") is met regardless because `max_count: 1` keeps him singleton; the `replace_on_respawn: true` flip only matters once Q7 wants hard antagonist re-stamping across sessions. **MCP extension** (exposing the field on create/update request shapes) remains worth doing for future MCP-driven Q7-style endgame builds; not a v1 blocker.
- 3 hunters (Coyle/Brennan/Voss) follow §5.L night-only pattern: 19h `patrolling` at canonical beat with `suppress_wander: false` (rover behavior — they roam the cemetery / alley / bayou-trail), 6h `sleeping` at `cendre:hotel-foyer` with `suppress_wander: true`. Hotel-foyer reuses the Slice 6.4 mortal-side daytime hub as plausible "hunter cover identity" boarding — V:tM-canonical (hunters operate under mundane covers). All 3 inherit `vampire_hunter` preset stats from Phase 2.7 (lvl 10, 110 HP, 2d6+2, perception 7, `flags.{guard, helper, aware, memory, no_charm}`, faction `vampire_hunters`).
- Stranger (`cendre:threat-stranger`) given **no routine** — pure sentinel anchored to `cendre:bayou-shack-interior-1` (his canonical safehouse, locked behind the writ-keyed door). The Q7 endgame target sits in his shack waiting; Obfuscate/Celerity flavor lives in the prototype's discipline data (Phase 2.7), not in movement behavior. `max_count: 1` + 1800s respawn_interval (slower cadence appropriate for a boss; other slice 6.5 spawns stay at 600s).
- Casey Boudreaux (`cendre:threat-casey-anarch`) gets the single 0h `working` sentinel routine at `cendre:foundry-cellar` matching §5.L. Phase 2.7 already set her `vampire_goon` preset (lvl 6, 60 HP, `bleeding` on-hit, `flags.{vampire, undead, holy_vulnerable, memory, no_sleep, no_charm}`, faction `anarch_unbound`) with the `aggressive` flag cleared per Phase 2.7 build log (she's a social role, not a random monster).
- All hunters got transition messages on both day/night entries (slips out/in, fades back, etc.) — gives players a visible tell when the night shift kicks in, so a cemetery walk at 19:00 feels like the hunters arrived deliberately rather than appeared.
- Spawn-point UUIDs:
    - `cendre:threat-stranger` → `cendre:bayou-shack-interior-1` (max_count:1, respawn:1800s, **replace_on_respawn:false** ← MCP gap): `88c8bd31-11d5-4ca9-881e-3bac7da7c4aa`
    - `cendre:threat-hunter-coyle` → `cendre:cemetery-rows-1`: `57fcad0c-8293-4f98-ab18-2c1c0491d9f7`
    - `cendre:threat-hunter-brennan` → `cendre:alley-bourse-to-cathedral`: `0ed41710-2f73-408b-b850-8025c7183c7d`
    - `cendre:threat-hunter-voss` → `cendre:bayou-trail-3`: `3ed49d25-4216-4134-ae35-f7dc706b6c35`
    - `cendre:threat-casey-anarch` → `cendre:foundry-cellar`: `56570154-db7b-45b8-ac9e-563afe60159e`
- DoD partially met (4/5): hunters appear in cemetery/alley/bayou-trail at night and retire to hotel-foyer at dawn; Casey waits in foundry-cellar; Stranger spawns in safehouse interior on next reset. Only the `replace_on_respawn: true` semantic on the Stranger's spawn point is missing pending the MCP extension.

Slice 6.6 notes:

- Q1 (`cendre:q-tutorial`) created with the five VisitRoom objectives matching §4.B; auto-progress fires via the `feature_quests_slices2_3` room-visit listener, so the spec's 5× `add_room_trigger` was skipped (per-haven progress flag is unnecessary — VisitRoom progress is tracked on the active quest natively).
- Mireille's dialogue tree: 8 nodes (`root`, `concord`, `havens`, `prince`, `offer`, `accepted`, `progress`, `turnin`, `post_completion`) and 19 choices. Tree shape:
  - `root` is always-on greeting; offers concord/havens/prince flavor branches plus the state-gated path.
  - State gates on root choices: `duty` (IsThinblood AND `flag_unset` local `tutorial_acknowledged`), `progress` (QuestActive), `witness` (QuestCompletable, fires CompleteQuest), `respects` (HasAchievement cendre_met_all_sires).
  - `offer.accept` carries `once_per_player: true` + effects `[OfferQuest cendre:q-tutorial, SetFlag tutorial_acknowledged local]`. The local flag hides the duty choice forever after first acceptance (Rust's `DialogueCondition` has no negation, so we gate on `flag_unset` of a stamp-on-accept flag rather than trying to express `NOT QuestActive AND NOT QuestComplete`).
  - Defensive turn-in: even if auto-complete fires before the player returns, `respects` still surfaces post-completion flavor via the achievement check. If somehow the listener missed the 5th visit, `witness` still completes via the dialogue effect path.
- Q1's giver_mob_vnum is `cendre:seneschal-mireille` (canonical), so player `quest` lookups + builder tooling correctly back-reference the seneschal. Server-side spawn auto-refreshed Mireille's live instances on each node/choice add.
- No `add_room_trigger` calls landed for this slice (5 saved against the original sketch).
- Local flag `tutorial_acknowledged` is scoped to mob vnum `cendre:seneschal-mireille` and won't collide with any other dialogue tree's namespace.

Slice 6.7 notes:

- Q2 (`cendre:q-embrace-brujah`) follows the canonical §4.B table shape: `KillMob cendre:foundry-enforcer × 3 → EmbraceClan brujah`. The Slice 6.7 sketch line (BringItem foundry-token from foundry-bones via `add_spawn_dependency`) is obsolete — §4.B was authoritative; built per §4.B. Saved one `add_spawn_dependency` call and the now-unused token tracking.
- New mob `cendre:foundry-enforcer` (UUID `bd4c3b73-4052-40a6-b738-48c966baa508`): lvl 6, `vampire_goon` preset applied → flags {aggressive, memory, no_sleep, no_charm, undead, vampire, holy_vulnerable}, on-hit bleeding (50% / 3 / 3), HP 60, faction `clan_brujah`, `world_max_count: 3` matches the §4.A spec and `cendre:foundry-pit`'s 3-cap room note. Gender male.
- Spawn point UUID `656c6502-9e82-4af8-8f34-7f5873ccdad0` at `cendre:foundry-pit` (`c339d93f-b7d4-4a98-b608-0da8cddb5b08`), enabled, `max_count: 3`, `respawn_interval_secs: 300`. The pit auto-fills with three enforcers waiting for a thinblood to step in.
- Tony's tree mirrors the slice 6.6 pattern with one extra branch: `marcel` (Iron Marcel teaser) is always accessible. Tony references Marcel both pre- and post-embrace; this seeds Q-I1 narrative without depending on Q-I1's actual existence (Q-I1 hooks land in slice 6.12). When Q-I1 ships, `marcel` choice text may evolve into an OfferQuest hand-off branch.
- State gates on root: `challenge` (IsThinblood + quest_complete q-tutorial + flag_unset local `challenge_offered`), `progress` (QuestActive), `acknowledge` (quest_complete). Same once_per_player + stamp-on-accept pattern as Mireille's tree.
- `accepted` node is exit-only — Tony's not interested in further chat the moment he's set you on the pit. (No "back" choice; pacing is part of his voice.)
- `proven` is the canonical "you're in the clan" beat. The trait `clan_brujah` is granted automatically by `apply_clan_acknowledgment` via the EmbraceClan reward's listener path on the third kill; Tony's `proven` branch gates on `quest_complete` which is set in the same path.
- Spec drift fix: the §4.B-vs-Slice-sketch mismatch (KillMob vs BringItem) is now resolved; spec-table-canonical pattern confirmed for the remaining 4 embrace quests in 6.8-6.11.

UUIDs assigned for slice 6.7:

```
cendre:foundry-enforcer (mob)         bd4c3b73-4052-40a6-b738-48c966baa508
cendre:foundry-enforcer (spawn)       656c6502-9e82-4af8-8f34-7f5873ccdad0
```

Slice 6.8 notes:

- Q3 (`cendre:q-embrace-toreador`) follows the canonical §4.B shape: `BringItem cendre:item-painting × 1, return_to_mob_vnum: cendre:sire-toreador → EmbraceClan toreador`. Auto-turn-in fires through the give-listener path; no `CompleteQuest` dialogue branch needed. Prereq `cendre:q-tutorial`.
- The painting (`cendre:item-painting`, already-existing Slice 6.0 prototype) is sourced via a fresh world-spawn at `cendre:conservatory-collector-apt-2` (per Slice 6.7-6.11 sketch: world-find rather than drop dependency). Single instance, 600s respawn — the collector's gallery refills slowly so a second thinblood arriving the same night finds it bare.
- New mob `cendre:conservatory-collector` (Bertrand Lacaille, UUID `5df7dff2-cf62-4892-8ffc-e09ab4321360`): mortal, no preset, no faction, sentinel, lvl 4 / 28 HP / 1d4 damage / AC 10. Sits in apt-1 as scenery + future Q-I hook. `world_max_count: 1`. No dialogue tree this slice — the "negotiate path" prose hook in §4.B stays an unimplemented flavor option for now; today the only path is the world-find. Could land as a future polish slice if dialogue-driven `GiveItem` becomes the preferred design.
- Spawn-point UUIDs:
    - `cendre:conservatory-collector` → `cendre:conservatory-collector-apt-1` (max_count:1, respawn:600s): `5dc755d4-96fe-4d76-9d83-6bfb555a39cb`
    - `cendre:item-painting` → `cendre:conservatory-collector-apt-2` (max_count:1, respawn:600s): `c486f508-9700-4c68-b484-14f3d2a543bf`
- Yvette's tree: 7 nodes (`root`, `offer`, `accepted`, `progress`, `proven`, `mathilde`, `stage`) and 14 choices. Voice: imperial Old World restraint — short pronouncements, single sentences, occasional sardonic kindness. The `mathilde` branch is the Q-I2 narrative teaser (always accessible, parallels Tony's `marcel` branch in 6.7); seeds murder-investigation thread for slice 6.13. The `stage` branch gives Conservatory political flavor (the Toreador self-image as the regime's mirror).
- State gates mirror the Mireille/Tony pattern: `commission` (IsThinblood + `quest_complete cendre:q-tutorial` + `flag_unset` local `commission_offered`); `progress` (QuestActive); `acknowledge` (quest_complete). `offer.accept` carries `once_per_player: true` + effects `[offer_quest, set_flag commission_offered local]`. Stamp-on-accept local flag scopes to `cendre:sire-toreador`, no collision risk.
- **Builder-schema correction caught**: the first attempt at `flag_unset` used `key: "..."` (the field name from set-quest-choice work). Rust's `DialogueCondition::FlagUnset` uses `name`, not `key`. Discovered via a 422 on the first choice add; verified against `src/types/dialogue.rs:69` and rebuilt. Recording here so 6.9–6.11 (and any future flag-gated choice) use `{kind: "flag_unset", name: "...", scope: "local"}`.

UUIDs assigned for slice 6.8:

```
cendre:conservatory-collector (mob)   5df7dff2-cf62-4892-8ffc-e09ab4321360
cendre:conservatory-collector (spawn) 5dc755d4-96fe-4d76-9d83-6bfb555a39cb
cendre:item-painting (spawn)          c486f508-9700-4c68-b484-14f3d2a543bf
```

Slice 6.9 notes:

- Q4 (`cendre:q-embrace-ventrue`) follows the canonical §4.B shape: `BringItem cendre:item-debt-marker × 1, return_to_mob_vnum: cendre:sire-ventrue → EmbraceClan ventrue`. Prereq `cendre:q-tutorial`. Sourced via spawn-dependency (not world-find), matching the Slice 6.7–6.11 sketch line for Q4: `add_spawn_dependency(cendre:bourse-debtor, cendre:item-debt-marker)`.
- New mob `cendre:bourse-debtor` (Vidal Cassen, UUID `5aa6f099-7c29-4045-aea6-e1c0e44ace3b`): mortal ghoul, lvl 5 / 42 HP / 1d6+2 / AC 9, faction `clan_ventrue`, `flags.{cowardly, aware}`, perception 4, no preset (per §4.A "no preset, no faction" — but the spec note says faction `clan_ventrue`; followed the table). `world_max_count: 1`. The cowardly flag is thematic: a ghoul running thin on his patron's blood flees at low HP, making the encounter feel like collection rather than execution.
- **Spec-vs-room-flag conflict resolved**: §4.D maps debtor to `cendre:bourse-bank-office` (or wanders bourse district), but bank-office is `combat_zone: "safe"` — blocks the kill objective entirely. Placed the spawn at `cendre:bourse-club-2` ("The Club's Back Rooms", pvp / indoors / dark / no_magic) instead. Thematically tighter: he's literally hiding in the unmapped rooms beneath the salon, exactly as Saint-Clair says in the dialogue. Pattern note: Q5/Q6 may hit the same combat_zone gotcha when placing their drop mobs — always grep the room's `combat_zone` before assigning to a kill-objective spawn.
- Spawn-point UUID `da89f2a0-3765-4f71-bba3-e5c332de164c` at `cendre:bourse-club-2` (max_count:1, respawn:600s). Drop dependency: `cendre:item-debt-marker` × 1 to `inventory` (chance 100). On first spawn (debtor was already live in the world after creation), the dependency listener auto-attached an inventory copy to the existing instance — no manual `spawn_item` needed.
- Henri's tree: 7 nodes (`root`, `offer`, `accepted`, `progress`, `proven`, `forgery`, `concord`) + 14 choices. Voice: judicial gravitas, banker's economy of speech, only one or two words ever wasted ("Discipline." / "Satisfactory."). Two narrative-teaser branches always-on:
  - `forgery` seeds Q-I3 (forged seals / Émeric the clerk audit-ledger thread) for slice 6.14.
  - `concord` gives Henri's view of the political settlement — useful future Q7 endgame hook.
- State gates mirror Mireille/Tony/Yvette: `duty` (IsThinblood + `quest_complete cendre:q-tutorial` + `flag_unset` local `duty_offered`); `progress` (QuestActive); `acknowledge` (quest_complete). `offer.accept` carries `once_per_player: true` + effects `[offer_quest, set_flag duty_offered local]`. Local stamp-flag scope: `cendre:sire-ventrue`. Three-slice consistency (6.6/6.7/6.8/6.9) on the gate pattern — confirmed reusable for 6.10/6.11.
- No FlagUnset 422 this slice (the schema correction from 6.8 stuck — used `name`/`scope` from the start).

UUIDs assigned for slice 6.9:

```
cendre:bourse-debtor (mob)            5aa6f099-7c29-4045-aea6-e1c0e44ace3b
cendre:bourse-debtor (spawn)          da89f2a0-3765-4f71-bba3-e5c332de164c
```

Slice 6.10 notes:

- Q5 (`cendre:q-embrace-nosferatu`) follows the canonical §4.B shape: `BringItem cendre:item-relic × 1, return_to_mob_vnum: cendre:sire-nosferatu → EmbraceClan nosferatu`. Prereq `cendre:q-tutorial`.
- **Source path picked: world-find at `cendre:catacombs-branch-3`** (Slice 6.7–6.11 sketch offered either world-find OR `add_spawn_dependency(cendre:catacomb-ribcage, cendre:item-relic)`). Chose world-find for two reasons: (1) thematic — Caretaker's "sealed-tomb relic" framing is graverobbing, not assassination; (2) avoids the dissonance of killing fellow-Nosferatu Ribcage to earn admission to his own clan. Ribcage remains pure flavor / Q-I scout. Pattern note: same logic should apply to the other clan-staffed havens (don't make players kill clan-aligned NPCs to embrace into that clan).
- New mob `cendre:catacomb-risen` (UUID `ecd4c6f9-3169-49ad-8a7f-0c56cc162ae2`): lvl 5 / 38 HP / 1d6+1 / AC 8, `flags.{aggressive, no_sleep, no_charm}`, no faction, no preset, `world_max_count: 2`. Acts as atmospheric guards in the crumbling passage between cemetery and deeper catacombs — they don't gate the relic (relic is in branch-3, risen are in branch-1), but they make the descent feel populated and dangerous. Spec §4.A specified `world_max_count: 2` and ×2 cap; honored both. (Some §4.A spec language said "1–2 minor risen mobs" — settled on 2 for a stronger ambient presence.)
- Spawn-point UUIDs:
    - `cendre:catacomb-risen` → `cendre:catacombs-branch-1` (max_count:2, respawn:600s): `5bee89e7-eecd-4c39-9855-e7890ca48169`
    - `cendre:item-relic` → `cendre:catacombs-branch-3` (max_count:1, respawn:600s): `c3a87f87-12e0-46f2-939a-6e151b52886c`
- Caretaker's tree: 7 nodes (`root`, `offer`, `accepted`, `progress`, `proven`, `gris`, `below`) + 14 choices. Voice: archival patience, slow precision, the rare flash of fondness for the dead. Two always-on narrative-teaser branches:
  - `gris` is the **Q-I4 narrative seed AND embedded Q-I4 hook**. The Caretaker is the canonical Q-I4 quest-giver (§4.D + line 768 note "Dialogue branch bypasses IsClanAcknowledged for this thread"). Today the `gris` branch is exposition-only — the OfferQuest effect for Q-I4 will be added in slice 6.15 (Q-I4 build). The flavor text already names the item-soil-bayou and item-soil-catacomb sample loop and points to the exam chamber east, so the player can plausibly start hunting samples even before the formal quest exists.
  - `below` is the Catacombs-as-archive philosophy beat — useful Q7 endgame hook ("we know a great deal. we use very little of it").
- State gates mirror 6.6–6.9: `errand` (IsThinblood + `quest_complete cendre:q-tutorial` + `flag_unset` local `errand_offered`); `progress` (QuestActive); `acknowledge` (quest_complete). `offer.accept` carries `once_per_player: true` + effects `[offer_quest, set_flag errand_offered local]`. Local stamp-flag scope: `cendre:sire-nosferatu`. Four-slice consistency now confirmed (6.7/6.8/6.9/6.10) — pattern is locked for 6.11.
- No combat-zone gotcha this slice (branch-3 is `combat_zone: null` PvP), but item-spawn was world-find anyway so it was moot.

UUIDs assigned for slice 6.10:

```
cendre:catacomb-risen (mob)           ecd4c6f9-3169-49ad-8a7f-0c56cc162ae2
cendre:catacomb-risen (spawn)         5bee89e7-eecd-4c39-9855-e7890ca48169
cendre:item-relic (spawn)             c3a87f87-12e0-46f2-939a-6e151b52886c
```

---

## MCP Tools Used Across the Build

`create_area`, `create_room`, `create_mobile`, `create_item`, `create_quest`, `add_room_door`, `set_room_exit`, `add_mobile_dialogue`, `add_mobile_dialogue_node`, `add_mobile_dialogue_choice`, `add_mobile_routine`, `apply_mobile_preset`, `create_spawn_point`, `update_area`, `update_room`, `add_room_trigger`. All on the `ironmud-public` MCP.

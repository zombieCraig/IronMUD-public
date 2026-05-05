# Import Guide

Status: living document. Sections marked **(planned)** describe surface area
the importer framework already supports but no engine currently exercises.

`ironmud-import` is a CLI utility that translates world data from older MUD
engines into IronMUD's room/area model. It is engine-agnostic by design —
each supported source MUD plugs in via the `MudEngine` trait — but currently
ships with a single working engine: **CircleMUD 3.x rooms**.

The importer:

- Always defaults to **dry-run**. No DB writes occur unless you pass `--apply`.
- Emits a categorised list of warnings for anything in the source that
  IronMUD does not (yet) model. Warnings are advisory unless they are
  marked **Block** (e.g. a colliding area prefix), in which case `--apply`
  refuses to run.
- Writes directly to a Sled database via the same layer `ironmud-admin` uses,
  so it must run **with the IronMUD server stopped** (Sled holds an exclusive
  file lock).

## Supported engines

| Engine | Rooms | Mobiles | Objects | Zone resets | Shops | Triggers |
|---|---|---|---|---|---|---|
| **CircleMUD 3.x** | ✅ | ✅ (prototypes) | ✅ (prototypes) | ✅ (M/O/G/E/P/D; R warn-only) | ✅ (overlaid onto keeper mob) | ✅ (specproc bindings from `spec_assign.c` + `castle.c`) |
| Diku / ROM / Smaug | (planned) | (planned) | (planned) | (planned) | (planned) | (planned) |

A **(planned)** entry means the framework can hold the data but no parser
is wired up yet. Adding one is mostly a matter of writing the per-engine
parser module — see [Adding a new engine](#adding-a-new-engine) below.

## Quick start

```bash
# Dry-run: parse + map the source tree, print plan + warnings, write nothing.
ironmud-import circle --source /path/to/circle-3.1

# Same, but also write a JSON warnings report to disk.
ironmud-import circle --source /path/to/circle-3.1 --report /tmp/import.json

# Restrict to a single source zone (useful for iterating on mapping rules).
ironmud-import circle --source /path/to/circle-3.1 --zone 30

# Commit to a (stopped) IronMUD database.
ironmud-import --database ironmud.db circle --source /path/to/circle-3.1 --apply

# Use a custom mapping table.
ironmud-import circle --source /path/to/circle-3.1 \
                      --mapping ./my-circle-mapping.json
```

`--source` accepts the CircleMUD root, its `lib/` subdirectory, or `lib/world/`
itself. The importer auto-detects the `wld/` + `zon/` pair underneath.

### Exit codes

| Code | Meaning |
|---|---|
| `0` | Clean dry-run, or successful apply |
| `1` | Parse / I/O error (source files missing or malformed) |
| `2` | Dry-run finished with **Block** warnings — fix and re-run before `--apply` |
| `3` | Apply failed mid-write (partial state may exist; inspect with `ironmud-admin world info`) |

## CircleMUD coverage matrix

What lands cleanly, what becomes a warning, and what is silently dropped.

### Sectors → IronMUD `RoomFlags`

| CircleMUD sector | IronMUD result |
|---|---|
| `INSIDE` | `indoors` |
| `CITY` | `city` |
| `FIELD` | `dirt_floor` |
| `FOREST` | `dirt_floor` |
| `HILLS` | `dirt_floor`, `difficult_terrain` |
| `MOUNTAIN` | `difficult_terrain` |
| `WATER_SWIM` | `shallow_water` |
| `WATER_NOSWIM` | `deep_water` |
| `FLYING` | (no flag — emits an Info warning) |
| `UNDERWATER` | `underwater`, `deep_water` |

### Room flags

| CircleMUD `ROOM_*` | IronMUD treatment |
|---|---|
| `DARK` | sets `dark` |
| `NOMOB` | sets `no_mob` |
| `INDOORS` | sets `indoors` |
| `PEACEFUL` | sets `combat_zone = safe` |
| `DEATH` | sets `death` (instant-kill on player entry) |
| `SOUNDPROOF` | sets `soundproof` (blocks shouts from leaking in or out) |
| `NOTRACK` | sets `notrack` (defeats the track skill in this room) |
| `NOMAGIC` | sets `no_magic` (suppresses player spellcasting from the room) |
| `TUNNEL` | sets `tunnel` (caps player occupancy at 1) |
| `PRIVATE` | sets `private_room` (caps player occupancy at 2; user-facing alias `private`) |
| `GODROOM` | **Warn**: rely on builder permissions |
| `HOUSE` | **Warn**: legacy house system differs from IronMUD property |
| `ATRIUM` | **Warn**: not modeled |
| `HOUSE_CRASH`, `OLC`, `BFS_MARK` | silently dropped (runtime / editor flags) |
| Bits ≥ 16 | **Warn** (`unknown flag`): patched flag — surface for review |

### Doors

| CircleMUD `EX_*` | IronMUD treatment |
|---|---|
| `ISDOOR` | creates a `DoorState` keyed by direction |
| `CLOSED` | `is_closed = true` |
| `LOCKED` | `is_locked = true` |
| `PICKPROOF` | sets `pickproof` on the door (lockpick skill cannot defeat it) |
| Door key vnums | rewritten to the prefixed IronMUD vnum (`<area>_<src_vnum>`) |

### Extra descriptions

`E` blocks copy 1:1 to `RoomData.extra_descs`.

### Zone reset commands

`M` / `O` / `G` / `E` / `P` / `D` reset commands in `.zon` files are
**translated** into IronMUD `SpawnPointData` rows (and door overrides for
`D`). `R` and a handful of edge cases (cross-block `P` chains,
`if=0` G/E without an anchor, `E` slot 0 / dual-NECK collisions) stay
warn-only. See the [CircleMUD zone reset coverage matrix](#circlemud-zone-reset-coverage-matrix)
below for per-command details and the [Zone resets backlog](#zone-resets-backlog)
for ranked unsupported features.

Cadence: each spawn point inherits the zone's `lifespan` (minutes → seconds)
as `respawn_interval_secs` so imported worlds repopulate at roughly the
authored cadence. Zones with `lifespan = 0` fall back to a 5-minute default.

### Other features (not imported)

- Special procedures: imported. See the
  [CircleMUD trigger coverage matrix](#circlemud-trigger-coverage-matrix)
  for the per-specproc mapping and the
  [Trigger backlog](#trigger-backlog) for ranked unsupported behaviors.
- Light counters
- Per-zone `reset_mode` (when to reset: never / when nobody's there /
  always — no IronMUD analogue; `lifespan` is consumed as the spawn-point
  cadence)
- The CircleMUD house system (saves, crash recovery, atrium gating)

## CircleMUD mobile coverage matrix

Each `.mob` entry becomes a single IronMUD `MobileData` **prototype**
(`is_prototype = true`). Live spawned instances are not created — that's
the spawn point system's job, and stock CircleMUD zone resets are still
warn-only (see the [Zone reset commands](#zone-reset-commands) section).

### Identity

| CircleMUD field | IronMUD field | Notes |
|---|---|---|
| keywords (line 2) | `keywords` | whitespace-split |
| short_descr (line 3, e.g. `the wizard`) | `name` | used in attack/action messages |
| long_descr (line 4) | `short_desc` | room-listing line; trailing newline trimmed |
| description (multi-line block) | `long_desc` | look/examine body; empty is OK |

### Stats

| CircleMUD field | IronMUD result |
|---|---|
| LEVEL | `level` (clamped ≥0) |
| HP_DICE (e.g. `5d10+550`) | `max_hp` and `current_hp` set to the dice's *maximum* value (`600`). Prototypes are templates; spawned instances can be re-rolled later. |
| DAMAGE_DICE | `damage_dice` (string, copied verbatim) |
| AC | `armor_class` (copied as-is — Circle's negative-is-better convention may need rebalancing) |
| GOLD | `gold` (clamped ≥0) |
| THAC0 | not modeled; silently dropped |
| EXP | not modeled (no XP system); silently dropped |
| ALIGNMENT | not modeled; **Info** warning if non-zero |
| POSITION / DEFAULT_POSITION | not modeled; silently dropped |
| SEX | not modeled at prototype level; **Info** warning if 1/2 |
| BareHandAttack and other E-block named attrs | not imported; **Warn** once per distinct attribute name across the whole import |

### MOB_* action bits → `MobileFlags`

| CircleMUD `MOB_*` | IronMUD treatment |
|---|---|
| `SENTINEL` | sets `sentinel` |
| `SCAVENGER` | sets `scavenger` |
| `AGGRESSIVE` | sets `aggressive` |
| `WIMPY` | sets `cowardly` (close enough — Circle's wimpy is HP-threshold-driven) |
| `SPEC` | **Warn**: special procedures not modeled — replace with a Rhai trigger after import |
| `ISNPC` | silently dropped (implicit on every imported mob) |
| `AWARE` | sets `aware` (mob sees through hidden/sneak/invisibility) |
| `STAY_ZONE` | sets `stay_zone` (wander/pursuit clamps to home area; stamped at first room placement) |
| `AGGR_EVIL`, `AGGR_GOOD`, `AGGR_NEUTRAL` | **Warn**: blocked on alignment system |
| `MEMORY` | sets `memory` (remembers PC attackers for 30 min, FIFO cap 10; resets on respawn) |
| `HELPER` | sets `helper` (faction left empty → Circle-stock semantics: any NPC defends any other NPC against PCs) |
| `NOSLEEP`, `NOBASH`, `NOBLIND`, `NOSUMMON`, `NOCHARM` | sets matching `MobileFlags.no_*` (hard-immunity gate over the paired spell/skill) |
| Bits ≥ 18 | **Warn** (`unrecognised mob flag`): patched flag — surface for review |

### AFF_* affected-by bits

Stock affects are persistent buffs/debuffs on the mob. The mapping
table now supports an `add_buff` action that stamps a permanent
`ActiveBuff` onto the imported prototype's `active_buffs`; the
spawn-time clone in `db::spawn_mobile_from_prototype` carries it to
every instance for free. `AFF_SANCTUARY` uses this path
(`damage_reduction`, magnitude 50, permanent). Other AFF_* without an
existing IronMUD `EffectType` still become advisory **Warn** entries.

`AFF_GROUP` and `AFF_CHARM` are transient runtime flags (never
authored) and are silently dropped.

## CircleMUD object coverage matrix

Each `.obj` entry becomes a single IronMUD `ItemData` **prototype**
(`is_prototype = true`). Like mobiles, no live instances are spawned —
zone-reset `O`/`G`/`E`/`P` commands that would place items in rooms,
mob inventories, or containers stay warn-only (see [Zone reset
commands](#zone-reset-commands)).

### Identity

| CircleMUD field | IronMUD field | Notes |
|---|---|---|
| keywords (line 2) | `keywords` | whitespace-split |
| short_descr (line 3, e.g. "a long sword") | `name`, `short_desc` | both copied to give look/inventory text a sane default |
| long_descr (line 4) | `long_desc` | in-room sentence; trailing newline trimmed |
| action_descr (line 5) | (dropped) | CircleMUD's "use" message; **Info** when present |
| weight (line 8 col 1) | `weight` | clamped ≥0 |
| cost (line 8 col 2) | `value` | clamped ≥0 |
| rent (line 8 col 3) | (dropped) | no rent system |

### Item type (line 6 col 1) → IronMUD `ItemType`

| CircleMUD type | IronMUD result | Notes |
|---|---|---|
| `LIGHT` (1) | `Misc` + `flags.provides_light` + `light_hours_remaining = v2` | `v2 ≤ 0` (incl. `-1` permanent) → `0` (no decay); positive → game-hour countdown that flips `provides_light` off when it hits 0 |
| `SCROLL` (2) | `Misc` | spell list (`v0..v3`) **Warn** — no `cast_spells_on_use` field |
| `WAND` (3) | `Misc` | charges + spell **Warn** — same gap |
| `STAFF` (4) | `Misc` | charges + spell **Warn** — same gap |
| `WEAPON` (5) | `Weapon` | `damage_dice_count`/`_sides` from `v1`/`v2`; `damage_type` from `v3` verb (see below) |
| `FIRE_WEAPON` (6), `MISSILE` (7) | `Misc` | unimplemented in stock Circle, **Warn** |
| `TREASURE` (8) | `Misc` + `categories: ["treasure"]` | |
| `ARMOR` (9) | `Armor` | `armor_class = -v0` (sign flip — Circle is negative-better) |
| `POTION` (10) | `LiquidContainer` (capacity 1 sip, type `HealingPotion`) | spell list **Warn** |
| `WORN` (11) | `Misc` | unimplemented stock; wear locations carry the slot info |
| `OTHER` (12) | `Misc` | clean |
| `TRASH` (13) | `Misc` + `categories: ["trash"]` | |
| `TRAP` (14) | `Misc` | unimplemented stock, **Warn** |
| `CONTAINER` (15) | `Container` | `v0` → `container_max_weight`; `v1` bits → `container_closed/_locked` (PICKPROOF **Warn**); `v2` → `container_key_vnum` rewritten to prefixed form |
| `NOTE` (16) | `Misc` | blank-paper writing semantics **Warn** — not modeled |
| `DRINKCON` (17) | `LiquidContainer` | `v0`/`v1`/`v3` → `liquid_max`/`_current`/`_poisoned`; `v2` → `liquid_type` via Circle drink table |
| `KEY` (18) | `Key` | clean |
| `FOOD` (19) | `Food` | `v0` → `food_nutrition` (Circle's "hours of hunger" is close enough); `v3≠0` → `food_poisoned` |
| `MONEY` (20) | `Gold` | `value = v0` (gold coins) |
| `PEN` (21) | `Misc` | writing-tool **Warn** |
| `BOAT` (22) | `Misc` + `flags.boat = true` | clean |
| `FOUNTAIN` (23) | `LiquidContainer` | same shape as DRINKCON; infinite-fill behaviour **Warn** |

### `WEAPON` damage verb (`v3`) → `DamageType`

| Circle verb (v3) | IronMUD `DamageType` |
|---|---|
| 0 hit, 5 bludgeon, 6 crush, 7 pound, 9 maul, 10 thrash, 13 punch | Bludgeoning |
| 2 whip, 3 slash, 8 claw | Slashing |
| 1 sting, 11 pierce, 14 stab | Piercing |
| 4 bite | Bite |
| 12 blast | Lightning *(lossy — no "kinetic burst" damage type)* |

### `DRINKCON` / `FOUNTAIN` liquid index (`v2`) → `LiquidType`

| Circle (`LIQ_*`) | IronMUD result |
|---|---|
| 0 water, 15 clear water | `Water` |
| 1 beer | `Beer` |
| 2 wine | `Wine` |
| 3 ale | `Ale` |
| 4 dark ale | `Ale` (Info — no distinct dark-ale) |
| 5 whisky, 7 firebreather | `Spirits` (firebreather is Info) |
| 6 lemonade, 9 slime mold juice | `Juice` (Info) |
| 8 local speciality | `Ale` (Info) |
| 10 milk | `Milk` |
| 11 tea | `Tea` |
| 12 coffee | `Coffee` |
| 13 blood | `Blood` |
| 14 salt water | `Water` (Info) |

### Extra (`ITEM_*`) flags → `ItemFlags`

| CircleMUD `ITEM_*` | IronMUD treatment |
|---|---|
| `GLOW` | sets `glow` |
| `HUM` | sets `hum` |
| `INVISIBLE` | sets `invisible` |
| `NODROP` | sets `no_drop` (curse) |
| `NOSELL` | sets `no_sell` |
| `NORENT`, `NODONATE` | silently dropped (no rent / donation systems) |
| `NOINVIS` | **Warn**: cannot-be-made-invis not modeled |
| `MAGIC` | **Warn**: tag-only; no IronMUD field — set `categories: ["magical"]` manually if desired |
| `BLESS` | **Warn**: no blessing system |
| `ANTI_GOOD`, `ANTI_EVIL`, `ANTI_NEUTRAL` | **Warn**: alignment-restricted use not modeled |
| `ANTI_MAGE`, `ANTI_CLERIC`, `ANTI_THIEF`, `ANTI_WARRIOR` | **Warn**: class-restricted use not modeled |
| Bits ≥ 17 | **Warn** (`unrecognised extra-flag bit`): patched flag — surface for review |

### Wear bits (`ITEM_WEAR_*`) → `wear_locations`

The wear-bit → `WearLocation` mapping is hard-coded (the right-hand side is
a list, not a single flag, so it isn't customisable through the JSON):

| Circle | IronMUD |
|---|---|
| `TAKE` | (implicit; absence emits **Info** — IronMUD has no "fixed in place" notion) |
| `FINGER` | `[FingerLeft, FingerRight]` |
| `NECK` | `[Neck]` |
| `BODY` | `[Torso]` |
| `HEAD` | `[Head]` |
| `LEGS` | `[LeftLeg, RightLeg]` |
| `FEET` | `[LeftFoot, RightFoot]` |
| `HANDS` | `[LeftHand, RightHand]` |
| `ARMS` | `[LeftArm, RightArm]` |
| `SHIELD` | `[OffHand]` |
| `ABOUT` | `[Back]` |
| `WAIST` | `[Waist]` |
| `WRIST` | `[WristLeft, WristRight]` |
| `WIELD` | `[Wielded]` |
| `HOLD` | `[Ready]` |

### Affect blocks (`A`) → `APPLY_*` translations

`A`-blocks attach permanent stat bonuses to items (e.g. `+2 STR` when worn).
The mapper applies them to `ItemData` directly — they aren't `ActiveBuff`
entries.

| CircleMUD `APPLY_*` | IronMUD treatment |
|---|---|
| `STR`, `DEX`, `CON`, `INT`, `WIS`, `CHA` | adds modifier to `stat_str`/`_dex`/… |
| `ARMOR` | adds `-modifier` to `armor_class` (sign-flipped) |
| `HITROLL` | adds modifier to `hit_bonus` — flat to-hit bonus summed across all worn equipment in combat |
| `DAMROLL` | adds modifier to `damage_bonus` — flat damage bonus summed across all worn equipment in combat |
| `MAXHIT`, `MAXMANA` | **Warn**: no item-level HP / mana bonus |
| `MAXMOVE` | **Warn**: no movement stat in IronMUD |
| `AGE`, `CHAR_WEIGHT`, `CHAR_HEIGHT` | silently dropped (no aging, height/weight on chars) |
| `SAVING_*` | **Warn**: no saving-throw system |
| `CLASS`, `LEVEL`, `GOLD`, `EXP` | silently dropped (unimplemented in stock Circle) |

### Extra descriptions (`E`)

`E` blocks copy 1:1 to `ItemData.extra_descs`. Each E-block becomes an
`ExtraDesc { keywords, description }`; players reveal it via
`look <keyword>` against the item in inventory, equipment, or the room
(e.g. `look letters` on the brass lantern surfaces its usage
instructions). Mirrors the room-side handling — see [Extra
descriptions](#extra-descriptions) above.

Builders can mutate the list post-import via `oedit <id> extra
<list|add|edit|remove>` or the `add_item_extra_desc` /
`remove_item_extra_desc` MCP tools.

### Value semantics

Several CircleMUD value semantics are lossy (light burn-time,
scroll/wand spell lists, fountain infinite-fill, blank-note language) —
each surfaces as an `UnsupportedValueSemantic` warning so the dropped
data is auditable.

## CircleMUD shop coverage matrix

Each `.shp` entry becomes a [`PlannedShopOverlay`] applied onto the
matching keeper mobile prototype after Pass 3 lands. Shop data does **not**
become a separate IronMUD entity — the importer mutates the keeper's
`shop_*` fields and sets `flags.shopkeeper = true` defensively. Stock
CircleMUD 3.1 ships eight `.shp` files (zones 25, 30, 31, 33, 54, 65,
120, 150) yielding **46** overlays.

The keeper mob is resolved via the global mob vnum index built during
mapping; shops whose keeper isn't in the import set are dropped with a
warning. Shops can reference keepers in *any* imported zone, so the
`.shp` and `.mob` files don't have to share a zone.

### Identity

| CircleMUD field | IronMUD field | Notes |
|---|---|---|
| `keeper_vnum` | resolves to the matching `MobileData` | shop dropped if keeper isn't in the import |
| `producing` vnum list | `shop_stock` | each entry rewritten to `<area_prefix>_<src_vnum>`; missing items dropped per-entry with a Warn |
| `profit_buy` (float, e.g. 2.1) | `shop_sell_rate` (i32, e.g. 200) | shop's *sell-to-player* multiplier × 100, rounded |
| `profit_sell` (float, e.g. 0.5) | `shop_buy_rate` (i32, e.g. 50) | shop's *buy-from-player* multiplier × 100, rounded |
| `buy_types` token list | `shop_buys_types` | mapped via JSON; deduped; lowercase IronMUD `ItemType` strings |

### Buy-type translation (`v0`) → IronMUD `ItemType`

| Circle token | IronMUD result |
|---|---|
| `LIGHT`, `SCROLL`, `WAND`, `STAFF`, `WORN`, `OTHER`, `TRASH`, `NOTE`, `PEN`, `BOAT`, `TREASURE` | `misc` |
| `WEAPON` | `weapon` |
| `ARMOR` | `armor` |
| `CONTAINER` | `container` |
| `LIQ CONTAINER`, `POTION`, `FOUNTAIN` | `liquid_container` |
| `KEY` | `key` |
| `FOOD` | `food` |
| `MONEY` | `gold` (matched verbatim; rare in stock files) |
| `FIRE WEAPON`, `MISSILE`, `TRAP` | **Warn** — unimplemented in stock Circle; entry dropped |

### Other shop fields (warn-only)

- **`in_room` list (rooms)** — IronMUD shopkeepers travel with their
  shop, so multi-room operation surfaces a per-shop Warn. Single-room
  shops are silent.
- **`open1/close1/open2/close2`** — IronMUD gates trading via the
  keeper's `daily_routine` `ActivityState`, not per-shop hours. Any
  non-default schedule (anything other than "always open") emits a Warn
  suggesting the builder author a routine on the keeper.
- **7 message strings** (no_such_item1/2, do_not_buy, missing_cash1/2,
  message_buy, message_sell) — IronMUD has no per-shop messaging, so
  shops with any non-empty messages emit a single Warn.
- **`temper`** — Info note (no analogue).
- **`bitvector`** (WILL_START_FIGHT, WILL_BANK_MONEY) — Warn (no analogue).
- **`with_who`** (TRADE_NO* alignment/class trade gates) — Warn (no
  analogue; the imported shop will trade with anyone).
- **`bank_account`** — silently dropped (runtime-only field).

## CircleMUD trigger coverage matrix

Stock CircleMUD 3.1 ships **without DG Scripts** (no `lib/world/trg/`,
no `dg_*.c`). Its only "trigger" surface is hard-coded vnum→specproc
bindings in `src/spec_assign.c` (148 lines) and `src/castle.c` (16
lines for King Welmar's Castle NPCs). The importer auto-locates these
files relative to `--source` (siblings `<root>/src/spec_assign.c`,
`<root>/src/castle.c`, `<root>/src/spec_procs.c`) and translates each
binding via `circle_trigger_mapping.json` into either:

- a **`MobileFlags` bit** (cityguard → `guard`, fido → `scavenger`),
- a **`*Trigger` struct** appended to the entity's `triggers` Vec
  (puff → OnIdle `@say_random`, dump → Periodic `@room_message`),
- or a **Warn** for behaviours with no IronMUD analog (collapsed to
  one dedup line per specproc when ≥2 vnums are bound — `magic_user`'s
  93 bindings show up once with a vnum-list sample).

If `src/spec_assign.c` is not located (e.g. `--source` points at
`lib/world` only), the importer emits a single Info note and skips —
spec parsing is never a hard error.

### Stock specproc → IronMUD action

| Specproc | Stock count | IronMUD treatment |
|---|---|---|
| `cityguard` | 12 mobs | sets `MobileFlags.guard` |
| `fido` | 2 mobs | sets `MobileFlags.scavenger` |
| `janitor` | 3 mobs | sets `MobileFlags.scavenger` |
| `snake` | 8 mobs | sets `MobileFlags.aggressive` (venom not modeled — pair with `poisonous` once `damage_type` is right) |
| `thief` | 5 mobs | sets `MobileFlags.aggressive` + Warn ("steals gold; no IronMUD steal action") |
| `receptionist` | 3 mobs | sets `MobileFlags.leasing_agent` + Warn ("set `medit <id> leasing area <area>` to bind the agent to a leasable area") |
| `puff` | 1 mob | OnIdle `@say_random` trigger; quote `args` extracted from `puff()`'s `do_say` literals in `spec_procs.c` |
| `mayor` | 1 mob | OnAlways `@emote` trigger + Warn ("walks a fixed path; no `daily_routine` generated") |
| `gen_board` | 4 items | OnExamine `@message` placeholder + Warn ("bulletin boards need a custom item type") |
| `bank` | 2 items | OnUse `@message` placeholder + Warn ("banking not modeled") |
| `dump` | 1 room | Periodic `@room_message` flavour trigger + Warn ("auto-disposal not modeled") |
| `magic_user` | 93 mobs | **Warn** (collapsed) — "casts random offensive spells in combat — replace with custom OnAttack trigger" |
| `guild` | 4 mobs | **Warn** (collapsed) — "class-specific practice; replace with mobile dialogue" |
| `guild_guard` | 5 mobs | **Warn** (collapsed) — "blocks wrong-class players; no IronMUD analog" |
| `postmaster` | 2 mobs | **Warn** — "mail system not modeled" |
| `cryogenicist` | 1 mob | **Warn** — "long-term rent storage has no IronMUD analog" |
| `pet_shops` | 1 room | **Warn** — "pet purchase needs custom dialogue + spawn rules" |
| `puff` (any others) | — | unrecognised → default Warn ("no mapping for specproc `X`") |

### Castle (King Welmar's Castle, zone 150)

`src/castle.c` calls `castle_mob_spec(offset, fname)` where `offset` is
*relative* to the zone bot derived from `#define Z_KINGS_C 150`
(stock = 15000). The parser auto-reads the `#define` and translates
offsets to absolute vnums. All 10 castle specprocs map to **Warn**
(bespoke per-NPC C bodies surface as warnings naming the real vnum +
specproc, so a builder can re-author each as a Rhai trigger):
`king_welmar`, `training_master`, `tom`, `tim`, `peter`, `jerry`,
`james`, `cleaning`, `castleguard`, `dickndavid`.

### Apply-time composition

The trigger overlay pass runs **after** the shop overlay pass, so a
mob hit by both (e.g. `cityguard` + a hypothetical `shopkeeper`) gets
both flags set. Last-assignment-wins: a duplicate `ASSIGN*` line for
the same vnum overrides the prior overlay and emits an Info note —
matches CircleMUD's runtime behavior.

### Other features (warn-only / not imported)

- The runtime `dts_are_dumps` `for` loop in `spec_assign.c` (binds
  every `ROOM_DEATH` room to `dump` at boot) is silently ignored —
  the parser only matches literal `ASSIGN*(VNUM, fname)` calls.
- Custom mob/obj/room flag bits (e.g. `MOB_SPEC` set on a `.mob`
  prototype but not assigned via `ASSIGN*`) — silently dropped at the
  flag layer; no fallback action.

## CircleMUD zone reset coverage matrix

Each `.zon` file's reset block is walked in source order and translated
into IronMUD `SpawnPointData` rows + per-room door overrides. Stock
CircleMUD 3.1 carries 1098 M / 188 O / 328 G / 554 E / 432 D / 77 P / 80 R
across all zones; the importer translates everything except `R` and a
handful of edge cases (see the [Zone resets backlog](#zone-resets-backlog)).

Anchor model: `G` and `E` with `if=1` chain onto the most-recent translated
`M` (becoming `SpawnDependency` entries on that spawn point); `P` with
`if=1` chains onto the most-recent translated `O` *if* that `O` loaded a
Container item. The runtime "did the parent actually spawn?" check is
intentionally not modelled at import time — that's the spawn tick's job
(`src/ticks/spawn.rs`); we treat any translated parent as a live anchor.

| CircleMUD reset | IronMUD result |
|---|---|
| `M if mob max room` | New `SpawnPointData` (entity_type=Mobile). `max_count = max`, `respawn_interval_secs = zone.lifespan × 60`. Sets the G/E anchor. The largest `max` seen across all M-resets for this vnum is also rolled up into the prototype's `world_max_count` (or `flags.unique` if `max == 1`) so Circle's world-wide cap semantics carry over. |
| `O if obj max room` | New `SpawnPointData` (entity_type=Item). Same cadence + same prototype-level world cap rollup as M. If the item is a Container, sets the P anchor. |
| `G if=1 obj max` | `SpawnDependency { destination: Inventory }` on the anchor mob's spawn point. `if=0` or no anchor → **Warn** + drop. |
| `E if=1 obj max wear_loc` | `SpawnDependency { destination: Equipped(loc) }`; `wear_loc` 0..17 mapped via the table below. `if=0` or no anchor → **Warn** + drop. |
| `P if=1 obj max container_vnum` | `SpawnDependency { destination: Container }` on the anchor item's spawn point. Anchor missing or `container_vnum` mismatch → **Warn** + drop. |
| `D if room dir state` | Mutates the matching `PlannedDoor`: state 0 → open, 1 → closed, 2 → closed+locked. Missing room or door → **Warn** + drop. |
| `R if room obj` | **Skipped silently.** Circle's `R` exists to dedupe room contents across resets; IronMUD's spawn tick + area reset already cap by (room, vnum), so `R` is redundant. See [Zone resets backlog](#zone-resets-backlog). |

### CircleMUD `E` wear-slot index → `WearLocation`

| Circle slot | IronMUD `WearLocation` | Notes |
|---|---|---|
| 0 LIGHT | (none) | **Warn** (`UnsupportedValueSemantic`) — no hold-light slot |
| 1 FINGER_R / 2 FINGER_L | `FingerRight` / `FingerLeft` | |
| 3 NECK_1 / 4 NECK_2 | `Neck` | both collapse — warn-once if a single mob uses both |
| 5 BODY | `Torso` | |
| 6 HEAD | `Head` | |
| 7 LEGS / 8 FEET / 9 HANDS / 10 ARMS | `LeftLeg` / `LeftFoot` / `LeftHand` / `LeftArm` | paired-slot collapse — Info note |
| 11 SHIELD | `OffHand` | |
| 12 ABOUT | `Back` | closest cloak analogue |
| 13 WAIST | `Waist` | |
| 14 WRIST_R / 15 WRIST_L | `WristRight` / `WristLeft` | |
| 16 WIELD | `Wielded` | |
| 17 HOLD | `Ready` | |

Authoritative source: `circle-3.1/src/structs.h` (`WEAR_LIGHT`..`WEAR_HOLD`).
Hard-coded in `src/import/engines/circle/wear.rs` — not configurable via JSON
(IronMUD's `WearLocation` is a Rust enum, not a flag bit, so the mapping
table format can't represent it).

## Mapping table format

Five default mapping files ship under `scripts/data/import/`:
`circle_room_mapping.json` (sectors + room flags),
`circle_mob_mapping.json` (MOB_* + AFF_* flags),
`circle_obj_mapping.json` (ITEM_* extras + APPLY_* affects),
`circle_shop_mapping.json` (shop `buy_types` tokens), and
`circle_trigger_mapping.json` (specproc bindings from `spec_assign.c`
+ `castle.c`). All five are embedded in the binary via `include_str!`
so the importer works outside the repo. A single `--mapping <file.json>`
override may define any combination of the eight sections
(`sector_to_flags`, `room_flag_actions`, `mob_flag_actions`,
`aff_flag_actions`, `extra_flag_actions`, `apply_actions`,
`buy_type_actions`, `trigger_actions`).

Schema:

```json
{
  "sector_to_flags": {
    "<CIRCLE_SECTOR_NAME>": {
      "set_flags": ["<ironmud_room_flag_name>", ...],
      "info": "optional human-readable note"
    }
  },
  "room_flag_actions": {
    "<CIRCLE_ROOM_FLAG_NAME>": {
      "action": "set_flag" | "set_combat_zone" | "warn" | "drop",

      // when action = set_flag:
      "ironmud_flag": "<ironmud_room_flag_name>",

      // when action = set_combat_zone:
      "value": "pve" | "safe" | "pvp",

      // when action = warn:
      "message": "free text shown in the dry-run report",

      // when action = drop (silently ignore):
      "info": "optional note"
    }
  },
  "extra_flag_actions": {
    "<CIRCLE_ITEM_FLAG_NAME>": {
      "action": "set_flag" | "warn" | "drop",
      // when action = set_flag, ironmud_flag is a snake-case ItemFlags field
      "ironmud_flag": "<ironmud_item_flag_name>"
    }
  },
  "apply_actions": {
    "<CIRCLE_APPLY_NAME>": {
      "action": "set_stat" | "set_armor_class" | "warn" | "drop",
      // set_stat → snake-case ItemData stat field (stat_str/_dex/_con/_int/_wis/_cha)
      "ironmud_stat": "stat_str"
    }
  },
  "buy_type_actions": {
    "<CIRCLE_ITEM_TYPE_TOKEN>": {
      "action": "set_flag" | "warn" | "drop",
      // when action = set_flag, ironmud_flag is an IronMUD ItemType
      // display string ("misc", "weapon", "armor", "container",
      // "liquid_container", "food", "key", "gold")
      "ironmud_flag": "<ironmud_item_type>"
    }
  }
}
```

Valid `<ironmud_room_flag_name>` values match the snake-case fields on
`RoomFlags` in `src/types/mod.rs` (`dark`, `no_mob`, `indoors`,
`underwater`, `climate_controlled`, `always_hot`, `always_cold`, `city`,
`no_windows`, `difficult_terrain`, `dirt_floor`, `property_storage`,
`post_office`, `bank`, `garden`, `spawn_point`, `shallow_water`,
`deep_water`, `liveable`).

The `mob_flag_actions` and `aff_flag_actions` sections share the same
`FlagAction` shape (`set_flag`, `warn`, `drop`; `set_combat_zone` is
rejected on a mob/aff entry). For `set_flag`, `ironmud_flag` must match
a snake-case field on `MobileFlags` (e.g. `aggressive`, `sentinel`,
`scavenger`, `cowardly`, `guard`, `healer`, `poisonous`, `fiery`,
`chilling`, `corrosive`, `shocking`, ...).

The `extra_flag_actions` section uses the same shape (`set_flag`, `warn`,
`drop` are valid; `set_combat_zone`/`set_stat`/`set_armor_class` are
rejected on extras). For `set_flag`, `ironmud_flag` must match a
snake-case field on `ItemFlags` (`glow`, `hum`, `invisible`, `no_drop`,
`no_get`, `no_remove`, `no_sell`, `unique`, `quest_item`,
`provides_light`, `boat`, `waterproof`, ...).

The `apply_actions` section accepts `set_stat`, `set_armor_class`,
`warn`, and `drop`. `set_stat` requires `ironmud_stat` (one of
`stat_str`, `stat_dex`, `stat_con`, `stat_int`, `stat_wis`, `stat_cha`).
`set_armor_class` automatically sign-flips the modifier (Circle's
negative-better convention → IronMUD's positive damage reduction).

CircleMUD MOB_*, AFF_*, ITEM_* (extra), ITEM_WEAR_*, and APPLY_* flag
names match the stock constants without the prefix — see
[`src/import/engines/circle/flags.rs`](../src/import/engines/circle/flags.rs)
for the canonical bit-name tables. Flags omitted from the JSON receive
a default action: MOB_* flags surface as `no mapping for MOB_X`
(unknown), AFF_* flags surface as a default `permanent AFF_X not
modeled at prototype level` warn, ITEM_* extras surface as
`no mapping for ITEM_X` (unknown), and APPLY_* without a mapping
surface as `no mapping for APPLY_X — affect dropped`.

CircleMUD sector names are uppercase: `INSIDE`, `CITY`, `FIELD`, `FOREST`,
`HILLS`, `MOUNTAIN`, `WATER_SWIM`, `WATER_NOSWIM`, `FLYING`, `UNDERWATER`.
Room flag names match the stock CircleMUD `ROOM_*` constants without the
prefix (`DARK`, `DEATH`, `NOMOB`, `INDOORS`, `PEACEFUL`, `SOUNDPROOF`,
`NOTRACK`, `NOMAGIC`, `TUNNEL`, `PRIVATE`, `GODROOM`, `HOUSE`,
`HOUSE_CRASH`, `ATRIUM`, `OLC`, `BFS_MARK`).

## Vnum and area-prefix conventions

CircleMUD vnums are integers (e.g. `6100`); IronMUD vnums are strings
prefixed with the owning area's slug. Each imported room gets vnum
`<area_prefix>_<source_vnum>`, e.g. CircleMUD zone 61 ("Haon-Dor, Dark
Forest") → area prefix `haon_dor_dark_forest` → room 6100 → vnum
`haon_dor_dark_forest_6100`. Door key references and exit destinations are
rewritten the same way.

The area prefix is derived from the zone name:

1. Lowercase
2. Replace any non-alphanumeric run with a single underscore
3. Strip leading/trailing underscores
4. If empty, fall back to `zone_<vnum>`
5. If two zones in the same import slug to the same prefix, the later one
   gets `<slug>_<vnum>` appended

If the resulting prefix already exists in the target DB, the importer
emits a **Block** warning rather than silently disambiguating, so a
double-`--apply` is loud rather than a stealth duplication. The same
applies to mobile prototype vnums — a `<area_prefix>_<source_mob_vnum>`
collision against an existing prototype is a **Block**.

> **Re-import caveat:** every `--apply` mints fresh `Uuid`s for areas,
> rooms, and mobile prototypes. Vnums are stable, so anything keyed by
> vnum (spawn points, triggers, scripts) round-trips cleanly across
> re-imports. UUID-keyed references do not.

## Adding a new engine

The framework is engine-agnostic. To add support for, e.g., ROM 2.4:

1. **Create the parser module:** `src/import/engines/rom/mod.rs`,
   implementing the `MudEngine` trait from `src/import/mod.rs`. The trait
   has one method: `parse(source: &Path) -> Result<(ImportIR, Vec<Warning>)>`.
2. **Fill in `IrZone` / `IrRoom`** from the source format. The IR is
   intentionally permissive — anything you couldn't translate goes in
   `IrZone.deferred` (becomes a warning) or `IrRoom.unknown_flag_names`.
3. **Add a mapping JSON** under `scripts/data/import/<engine>_room_mapping.json`
   with the same schema as Circle's. The mapping layer in
   `src/import/mapping.rs` is currently CircleMUD-specific; for a fully
   generic mapping pass you'll likely want to factor `MappingOptions`
   to hold a per-engine table. For a first pass it's fine to fork
   `mapping.rs` per engine and have the binary dispatch.
4. **Register on the CLI:** add a subcommand in
   `src/bin/ironmud-import.rs`.
5. **Update this guide** — add a row to the [Supported engines](#supported-engines)
   table and a coverage matrix section.
6. **Test:** drop a small synthetic fixture under
   `tests/fixtures/<engine>/` and add a `tests/import_<engine>.rs`
   integration test that mirrors `tests/import_circle.rs`.

## Adding a new content type

Phase-1 ships rooms only. To extend the framework to mobiles / objects /
shops / triggers:

1. **Add IR types:** `IrMob`, `IrItem`, etc. on `ImportIR` (`src/import/mod.rs`).
2. **Extend `MudEngine`:** add e.g. `parse_mobs` returning the new IR. The
   trait already has room for this — the existing `parse` can be split or
   keep returning a fuller `ImportIR`.
3. **Extend the mapping layer:** new `Planned*` types and `ir_to_plan`
   branches. Reuse the `Warning` model — coverage gaps just become more
   warnings.
4. **Extend the writer:** add a write pass that goes through `db::Db`'s
   `save_mobile_data` / `save_item_data` / etc. Vnum index rebuilds for
   each entity type.
5. **Document** in the coverage matrix above.

## Unsupported features backlog

Catalogued here so future IronMUD work can pick them off. Each entry is a
gap surfaced by the importer when run against real legacy content; ranked
by how often it appears in stock Circle 3.1 and how visible the missing
behavior is to players. Update this list as features get added to IronMUD
and as new engines are wired up.

### High priority

*(All previously high-priority CircleMUD room flags now have IronMUD
equivalents — see the [Room flags](#room-flags) coverage table.
`ROOM_PRIVATE`, `ROOM_TUNNEL`, `ROOM_DEATH`, and `ROOM_NOMAGIC` map to
`private_room`, `tunnel`, `death`, and `no_magic` respectively
(`private_room` rather than `private` because Rhai 1.x reserves
`private` as a keyword). The medium-priority `ROOM_SOUNDPROOF` and
`ROOM_NOTRACK` map to `soundproof` and `notrack`, and the exit-level
`EX_PICKPROOF` maps to `DoorState.pickproof`. Caveat: PRIVATE's stock
semantic also blocks summon/teleport into the room; the IronMUD port
currently models this only via the 2-occupant cap that walking-in
already respects. Revisit if/when summon/teleport spells land.)*

### Medium priority

- **`ROOM_GODROOM`** — immortal-only access. IronMUD relies on builder
  permissions, which are coarser than per-room gating.
- **`ROOM_ATRIUM`** — gates entry to a `ROOM_HOUSE`. Tied to the house
  system below; not useful in isolation.

### House system (whole subsystem)

- **`ROOM_HOUSE`** + **`ROOM_HOUSE_CRASH`** — Circle's player-owned-room
  model with crash recovery (items dropped in a house persist across
  reboots). IronMUD's property/lease system has different semantics
  (template + instance, time-limited leases). Direct conversion isn't
  possible without a deliberate compatibility layer; in the meantime,
  stock CircleMUD houses just import as ordinary rooms with a warning.

### Sector-level

- **`SECT_FLYING`** — Circle's flying-required terrain. IronMUD has no
  flying-only flag — players just walk through. Likely shape: a
  `RoomFlags.requires_flight` consulted in the move path; depends on a
  flight buff/skill existing first.
- **`SECT_INSIDE` + `ROOM_INDOORS` overlap** — Circle stock files set
  both redundantly. Cosmetic: produces a duplicate-set Info note during
  import. Not a feature gap, just a noise source.

### Zone resets backlog

Histogram from stock CircleMUD 3.1 (across all `.zon` files): **1098 M /
188 O / 328 G / 554 E / 432 D / 77 P / 80 R**. M/O/G/E/D translate
cleanly today, P translates in the common case, R is warn-only. Gaps
ranked below.

#### Intentionally not imported

- **`R` (remove obj from room)** — 80 occurrences. CircleMUD `R` clears
  named objects from a room before each reset because Circle's loader
  has no per-(room, vnum) dedupe — without `R` you'd pile up N copies of
  the floor-mat each cycle. IronMUD's spawn tick
  (`src/ticks/spawn.rs:118-133`) and `trigger_area_reset`
  (`src/script/spawn.rs:218-238`) already cap by (room, vnum) using
  `max_count`, so `R` is redundant in steady-state. The only edge case
  is re-importing onto an already-populated DB; treat that as an
  operational concern (clean DB or run a one-off dedupe pass) rather
  than a runtime gap.

#### High priority

- **P cross-block container chaining** — ~10 of 77 `P` resets (zone 25
  is the worst offender) reference containers declared in earlier reset
  blocks rather than the immediately-prior `O`. The current importer
  only chains onto the most recent `O`-of-Container, so these get
  dropped with a warn. Likely shape: a per-area
  `last_seen_obj_vnum -> spawn_point_index` map walked in source order.

#### Medium priority

- **WEAR_LIGHT (slot 0)** — no IronMUD hold-light slot. The spawn point
  for the parent mob is still produced; the `E` is dropped with a warn.
- **NECK_1 + NECK_2 collision** — both Circle neck slots collapse to
  IronMUD's single `Neck`; the second item per mob is dropped (warn-once
  per mob).

#### Resolved

- **Max-count semantics** — Circle's `max` ("stop reloading when the
  world has N already") is now imported into IronMUD's prototype-level
  `world_max_count: Option<i32>` (or `flags.unique` when `max == 1`).
  The mapper accumulates `max(circle_max)` per resolved vnum across all
  M/O reset blocks and applies the cap to the planned mob/item
  prototype. Enforcement lives in `db.spawn_*_from_prototype`, so spawn
  tick / area reset / `ospawn` / `mspawn` / migration all share one
  chokepoint. Per-spawn-point `max_count` still controls the per-room
  cap independently.

#### Low priority

- **Paired-slot collapse** (LEGS/FEET/HANDS/ARMS → `LeftLeg`/`LeftFoot`
  /`LeftHand`/`LeftArm`) — IronMUD models each foot/hand/etc.
  independently; Circle uses a single bit covering both. Cosmetic
  Info-level note; the right-side slot stays empty after import.
- **G/E with `if=0`** — essentially absent from stock content; warn + drop
  on the rare occurrence (no anchor by definition).
- **Per-zone `lifespan` / `reset_mode`** — already used: `lifespan`
  becomes the spawn point cadence. `reset_mode` (when to reset a zone:
  never / when nobody's there / always) has no IronMUD analogue and is
  silently dropped.

### Mobile flags — Implemented (May 2026)

- **`AFF_SANCTUARY`** → permanent `EffectType::DamageReduction` buff
  (`magnitude=50`, `remaining_secs=-1`) stamped onto the imported
  prototype's `active_buffs`. The buff carries to every spawn via the
  existing prototype-clone path. Damage paths (melee, spell, DoT,
  bleeding) consult `apply_damage_reduction` and halve incoming damage;
  drowning is intentionally excluded. The same effect powers the new
  player-castable `sanctuary` spell, and an aura cue is shown in
  examine and the room mobile listing for any target with the buff
  active.

- **`MOB_STAY_ZONE`** → `MobileFlags.stay_zone`. Wander, BFS routine
  pathing, pursuit, and combat-flee all consult
  `filter_exits_by_stay_zone`, dropping any exit whose target room
  belongs to a different `area_id`. Each mobile's `home_area_id` is
  stamped on its first room placement (`db::move_mobile_to_room`) and
  never overwritten thereafter, so a STAY_ZONE mob stays bound to the
  zone it was first dropped into. Mobiles without `home_area_id`
  pre-dating the field treat the flag as a no-op (graceful upgrade).
- **`MOB_AWARE`** → `MobileFlags.aware`. Aggression-target selection
  goes through `find_aggression_target_for_mob`, which calls
  `is_player_visible_to_mob`: hidden / sneaking PCs are skipped unless
  the mob is `aware` or its `perception` stat is at least
  `PERCEPTION_PIERCE_THRESHOLD = 5`; magical invisibility is only
  pierced by `aware` (perception is not enough). Composes with
  `MOB_MEMORY`: a hidden vendetta target slips by unless the mob is
  also `aware`.
- **`MOB_MEMORY`** → `MobileFlags.memory` plus a per-instance
  `remembered_enemies: Vec<RememberedEnemy>`. Every PC→mob attack that
  reaches `enter_mobile_combat` (melee, spell, scripted) and every
  melee swing in `process_character_attacks_mobile` calls
  `record_mob_memory`, which stamps a 30-minute timer
  (`MEMORY_DURATION_SECS`) and FIFO-evicts at `MEMORY_CAP = 10`. The
  same expansion of the wander-tick aggression predicate that fires
  for `aggressive` also fires when the mob has any non-expired
  remembered name in the room — yielding the in-room "<Mob> snarls,
  '<player>! I remember you!' and attacks!" emote. A periodic sweep
  in `process_mobile_effects` prunes expired entries on dormant mobs.
  Memory is per-instance and resets on respawn (Circle parity) — the
  prototype keeps an empty Vec, and `db::spawn_mobile_from_prototype`
  clones it.

- **`AFF_INFRAVISION`** → permanent `EffectType::NightVision` buff
  stamped onto the imported prototype, mapped onto IronMUD's existing
  night-vision logic (no separate infravision system). Players gain
  night vision from any of three sources: the `night_vision` trait
  (racial / point-buy), the new `night_vision` buff (cast via the
  `night_vision` spell — skill 2, mana 15, 10 min duration, in
  `spells_fantasy.json`), or wearing an item with the new
  `ItemFlags.night_vision` flag (e.g. goggles). All three OR into the
  dark-room checks in `display_room`, `is_room_dark`, and the
  light-level penalty calculation in `src/script/characters.rs`.
  Builder surface: `oedit <id> flag night_vision [on|off]` (alias
  `infravision`); MCP/API: `night_vision` on `ItemFlags`.
- **`AFF_INVISIBLE` / `AFF_DETECT_INVIS` / `AFF_DETECT_MAGIC`** →
  permanent buffs stamped onto the imported prototype's `active_buffs`,
  riding the same path as `AFF_SANCTUARY`. `AFF_INVISIBLE` adds a
  permanent `EffectType::Invisibility` buff: room listings filter the
  mob out for viewers without `EffectType::DetectInvisible` (or admin
  god-mode), the resolver `filter_visible_mobiles` drops the mob from
  player-facing keyword lookups (kill/look/examine/cast/etc.), and
  combat broadcasts use a viewer-aware `mob_display_name_for` so
  bystanders without detect see "Something hits you for X damage!"
  instead of the mob's name. Pierced viewers see the regular name plus
  a "(invisible)" cue in look. `AFF_DETECT_INVIS` adds the matching
  buff so the mob can target invisible PCs (extends
  `is_player_visible_to_mob` alongside the existing AWARE pierce).
  `AFF_DETECT_MAGIC` introduces a new `EffectType::DetectMagic` and a
  player-castable spell of the same name (skill 2, mana 15, 5 min
  duration); items with the new `ItemFlags.magical` bit get a
  "(magical aura)" cue in room listings and a "magical" property tag
  in examine when the viewer's buff is active. `ITEM_MAGIC` now
  imports straight to `flags.magical` instead of warning.
- **`MOB_NOSLEEP` / `MOB_NOBLIND` / `MOB_NOBASH`** → `MobileFlags.no_sleep`,
  `no_blind`, `no_bash`. Each acts as a hard-immunity gate over the
  matching new ability: the player-castable `sleep` spell (skill 3,
  mana 25, 60s duration), the player-castable `blind` spell (skill 3,
  mana 25, 90s, magnitude=50 to-hit penalty), and the new `bash`
  combat skill (15 stamina, 2-round stun on hit). All three abilities
  use a new `debuff` `spell_type` (sleep/blind) or write directly to
  `CombatState.stun_rounds_remaining` (bash). Sleep applies a
  permanent-style `EffectType::Sleep` buff that makes the target skip
  combat turns and is cleared automatically when the target takes
  damage. Blind applies an `EffectType::Blind` buff that subtracts
  `magnitude` percentage points from the attacker's hit chance and,
  when applied to a player, also short-circuits room sight (mirrors
  the `blindness` trait).
- **`MOB_NOSUMMON`** → `MobileFlags.no_summon`. Hard-immunity gate
  over the new player-castable `summon` spell (skill 4, mana 40,
  60s cooldown). The spell does world-wide name lookup; on success
  it yanks the target (NPC or opt-in PC) to the caster's current
  room, breaking any combat the target was in. PCs gate on a new
  `CharacterData.summonable` consent flag (default off, opt in via
  `set summonable on` — parallels CircleMUD's `PRF_SUMMONABLE`).
  Admin / `no_attack` mobs are filtered from world-wide lookup so
  shopkeepers, healers, and guards can't be pulled out of place.
  ROOM_NOMAGIC on the caster's room blocks the cast through the
  existing source-room gate.
- **`AFF_BLIND` / `AFF_SLEEP` / `AFF_CURSE`** → permanent buffs stamped
  onto the imported prototype's `active_buffs`, riding the same
  `add_buff` path as `AFF_SANCTUARY`. `AFF_BLIND` applies the existing
  `EffectType::Blind` (magnitude 50 = −50% to-hit), reusing the
  combat-tick handling that the player-castable `blind` spell already
  exercises. `AFF_SLEEP` applies the existing `EffectType::Sleep`,
  which the combat tick honors by skipping the mob's turn and clearing
  the buff on first damage; on respawn the prototype-stamped buff is
  cloned again, so the mob is asleep until disturbed each life. New
  `EffectType::Curse` (`AFF_CURSE`) subtracts its magnitude (default
  10 = −10%) from hit chance in both player-attack and mob-attack
  paths, alongside Blind. Each carries a cosmetic examine cue
  ("fast asleep" / "eyes are clouded" / "wreathed in a faint shadow
  of ill fortune"). With this landing, only `AFF_POISON` remains as a
  warn-only AFF row — see the note below.
- **`MOB_NOCHARM`** → `MobileFlags.no_charm`. Hard-immunity gate
  over the new player-castable `charm` spell (skill 4, mana 35,
  90s cooldown, 5min duration). Charm targets an in-room NPC and
  stamps a duration-bound `EffectType::Charmed` buff whose `source`
  field stores the master's name — no separate master_id field on
  MobileData. Charmed mobs auto-follow the master across rooms via
  a new `propagate_charmed_mobs` hook in `go.rhai`, are filtered
  from `find_aggression_target_for_mob` so they never aggro the
  master, and short-circuit `should_suppress_wander` so they stop
  wandering / sim-routing while bound. The new `order <mob> attack
  <target>` command lets the master redirect the charmed mob in
  combat (refuses targeting the master themselves). Charm releases
  cleanly on master quit, master death, mob death, or buff expiry
  (`break_all_charms_by_player` runs from `quit.rhai` and both
  player-death paths). With this landing, the stock CircleMUD
  mob-flag set has zero `warn` rows.
- **`MOB_HELPER`** → `MobileFlags.helper`. The helper system scans the
  current room each combat tick and pulls any standing, alive,
  non-engaged HELPER mobile into combat against a PC who is attacking
  one of its allies. Allies are identified via the optional
  `MobileData.faction: Option<String>` tag (case-insensitive equality).
  An empty/None faction falls back to Circle-stock semantics: any
  unfactioned HELPER defends any other unfactioned NPC. A tagged
  faction explicitly opts a mob *out* of the generic pool, so
  authored factions can coexist with stock content. Engagement is
  same-room and PC-attackers only; helper-vs-NPC and adjacent-room
  rescue are deliberate non-features for now. Builders set the tag via
  `medit <id> faction <string|clear>`. Stock CircleMUD imports leave
  faction empty so existing zones work unchanged.

### Mobile flags — Medium priority

- **`MOB_AGGR_EVIL` / `AGGR_GOOD` / `AGGR_NEUTRAL`** — alignment-conditional
  aggression. Blocked on the alignment system below; degrades gracefully
  to non-aggressive in the meantime.
- **`AFF_DETECT_ALIGN`** — paired with an alignment system that doesn't
  exist yet.

### Mobile flags — Low priority

- **`AFF_POISON`** — permanent self-DoT on prototype. Deliberately not
  modeled: IronMUD's `MobileFlags.poisonous` already covers the "this
  creature is venomous on hit" case, and "spawn already poisoned with
  a self-DoT" is rare flavor that isn't worth the complexity. Importer
  warns and continues.
- **`MOB_WIMPY`** — currently maps to `cowardly`, but Circle's wimpy is
  HP-threshold-driven (≤ 30% HP) where IronMUD's `cowardly` flees on
  any sniping or low-HP. Close enough for now; revisit if behavior
  diverges noticeably in play.

### Mobile subsystems (whole-feature)

- **Special procedures (`MOB_SPEC`)** — C function pointers (cityguard,
  postmaster, fido, snake, mage-shopkeeper). Imported via
  `src/spec_assign.c`; see the
  [CircleMUD trigger coverage matrix](#circlemud-trigger-coverage-matrix)
  and the [Trigger backlog](#trigger-backlog) for ranked gaps.
- **Alignment system** — Circle's −1000..+1000 axis. Without this,
  alignment-conditional aggression and the `PROTECT_EVIL`/`PROTECT_GOOD`
  affects are meaningless.
- **XP awards** — Circle awards a per-mob `EXP` value on kill. IronMUD
  has no XP system; the field is silently dropped at import.
- **Position / default position** — sleeping / sitting / resting NPCs.
  Tied into a stance system that doesn't exist; IronMUD daily routines
  cover *some* idle behaviors but not "this mob is asleep until
  attacked".
- **Sex / gender on prototypes** — `Characteristics.gender` lives only on
  generated migrants. Stock CircleMUD authors mob gender directly on
  the prototype; would need a prototype-level `Characteristics` (or a
  `gender: Option<String>` on `MobileData`) to round-trip.
- **THAC0 → `hit_modifier` conversion** — Circle's combat math is
  different (lower THAC0 = better, intersects with target AC). A
  calibrated formula plus a balancing pass is needed before this is
  more than guesswork.
- **AC convention** — Circle's AC is *negative-is-better* (−10 is
  excellent armor); IronMUD treats `armor_class` as a positive damage
  reduction. The importer copies AC verbatim — most stock mobs will
  end up under-armored after import. Needs a mapping/sign-flip pass.
- **E-block named attributes** (`BareHandAttack`, `Str:`, `Hit:`, `Dam:`,
  `Move:`, …) — patches add many. Captured but not imported; surfaces
  warn-once per distinct name.
- **Shops (`.shp`)** — Imported. See [CircleMUD shop coverage
  matrix](#circlemud-shop-coverage-matrix) for the field-by-field
  translation and [Shop subsystems](#shop-subsystems--high-priority)
  for backlog gaps (hours, message strings, multi-room, etc.).
- **Equipment / inventory from zone resets** — `G`/`E`/`P` reset
  commands give items to mobs / equip them / put items in containers.
  Currently warn-only; not applied. See [Zone resets](#zone-resets-whole-subsystem)
  above.

### Object subsystems — High priority

Histogram numbers below are from a clean dry-run against stock CircleMUD 3.1
(30 zones, 679 imported items).

- **Spell-bearing consumables** (`SCROLL`, `POTION`, `WAND`, `STAFF`)
  — `v0..v3` carry "cast spell N at level L on use" semantics. IronMUD
  has `teaches_spell` (different concept: learning the spell) but no
  "cast on use". Likely shape: `cast_on_use: Vec<{spell, level,
  charges}>` on `ItemData` consulted by `quaff` / `recite` /
  `zap` / `brandish` commands.

### Object subsystems — Medium priority

- **`ITEM_MAGIC` tag** — 213 stock items carry the bit as a "this is a
  magical item" indicator. Currently warns. Closest equivalent:
  `categories: ["magical"]`, but the importer doesn't auto-add it
  because the bit is informational rather than functional.
- **Class / alignment restriction flags** (`ITEM_ANTI_*`) — gate equip
  by class or alignment. ~140 stock occurrences across all six bits.
  Both systems (alignment + class restrictions on items) need to land
  before this can be wired.
- **`APPLY_MAXHIT` / `APPLY_MAXMANA`** — items that bump max HP/mana
  when worn. No item-level field; once buff stamping at equip-time
  exists, these become trivial.
- **`ITEM_FOUNTAIN` infinite-fill** — currently imports as a finite
  `LiquidContainer`. Stock fountains refill themselves on use. Likely
  shape: `flags.infinite_liquid` consulted by the `drink` / `fill`
  commands.

### Object subsystems — Low priority

- **`ITEM_WAND` / `ITEM_STAFF` charge counts** (`v2`) — currently
  dropped. Likely shape: a `charges_remaining: i32` once "cast on use"
  lands.
- **`ITEM_TRAP`** (`v0` = spell, `v1` = damage) — unimplemented in
  stock Circle and uncommon. Maps to `Misc` with a warn today; closest
  IronMUD analogue is a per-room trap.
- **`ITEM_NOTE`** blank-paper writing — `note_content` already exists
  on `ItemData` but is authored by `oedit`, not by a player writing
  on a blank page mid-game.
- **`ITEM_PEN`** — needed for the note-writing system above; no
  IronMUD analogue.
- **`APPLY_SAVING_*`** — saving throws don't exist in IronMUD's
  combat system.
- **`NORENT` / `NODONATE`** — no rent / donation systems in IronMUD;
  silently dropped.

### Object subsystems (whole-feature)

- **Item affect → buff conversion** — Circle's `A`-blocks today apply
  permanent stat bumps directly to `ItemData.stat_*`. Once the buff
  system supports "apply on equip / remove on unequip", convert these
  to `ActiveBuff` stamps for cleaner state management.
- **Equipment / inventory from zone resets** — `G`/`E`/`P` reset
  commands. Already covered under [Zone resets](#zone-resets-whole-subsystem).

### Shop subsystems — High priority

Histogram numbers below are from a clean dry-run against stock CircleMUD 3.1
(8 `.shp` files yielding 46 shop overlays).

- **Shop daily-routine synthesis from `open1/close1/open2/close2`** —
  Stock shops carry open hours (e.g. shop #12034 is open 8-18 then
  18-22). IronMUD gates trading via the keeper's `daily_routine`
  `ActivityState` (Working vs. OffDuty / Sleeping). The importer warns
  per shop today; a future pass should synthesize a `daily_routine` on
  the keeper that flips the keeper to `Working` during the open
  window(s) and `OffDuty` otherwise. Dual-shift hours (open2/close2)
  complicate the transform, but most stock shops are single-shift.
  Without this, every imported shop is "always open" regardless of the
  source schedule.
- **`WILL_START_FIGHT`** — Circle's anti-theft response: shopkeepers
  attack on detected steal. ~10 of 46 stock shops set this bit. Likely
  shape: a per-mob `MobileFlags.hostile_on_steal` consulted by the
  `steal` skill path, or a `start_fight_on_theft` shop field.

### Shop subsystems — Medium priority

- **Per-shop message strings** — All 46 stock shops carry the 7 custom
  templates (`no_such_item`, `do_not_buy`, `missing_cash`,
  `message_buy`, `message_sell`). IronMUD has none. Likely shape: a
  `MobileData.shop_messages: Option<ShopMessages>` with template
  substitution in `buy.rhai` / `sell.rhai` / `list.rhai` (substitutions
  needed: `%s` for player name / item name, `%d` for gold amount).
- **Multi-room shops (`in_room[]`)** — Circle shops can operate in
  multiple rooms with the keeper free to move between them
  (e.g. shop #12016 spans 6 rooms). IronMUD shopkeepers travel with
  their shop today, so the imported keeper only trades from their
  current room. Likely shape: a `MobileData.shop_room_vnums:
  Vec<String>` consulted by `find_shopkeeper_in_room`, or pin the
  keeper to one room via routine.
- **`WILL_BANK_MONEY` + `bank_account`** — Circle shopkeepers can stash
  excess gold in a bank account (gold beyond `MAX_OUTSIDE_BANK = 15000`
  moves to the bank). IronMUD has no shop-bank link today.

### Shop subsystems — Low priority

- **`temper`** — Keeper mood when broke (0-2); cosmetic emote variation
  only. No IronMUD analogue.
- **`with_who`** (TRADE_NO* alignment/class trade gates) —
  alignment/class restrictions on trading. Blocked on the alignment
  system that's already on the mob backlog above; class-restricted
  trade is also blocked on a class-system landing first.
- **Producing items in foreign zones** — A handful of stock shops
  reference items in zones the importer didn't see (e.g. shop #5484
  references items 5524-5533 which live outside the imported tree).
  Currently per-entry Warn + drop; harmless when importing the full
  Circle tree, surface for partial imports.

### Trigger backlog

Histogram from stock CircleMUD 3.1: 148 specproc bindings in
`src/spec_assign.c` + 16 in `src/castle.c`. The importer translates
~42 cleanly (cityguard/fido/janitor/snake/thief/receptionist set flags;
puff/mayor/gen_board/bank/dump get template triggers); the remainder
warn-only. Gaps ranked below.

#### High priority

- **`magic_user` combat spell list** — 93 mobs in stock 3.1 (the most
  common specproc by a wide margin). The C body picks a random offensive
  spell each combat round (magic missile / chill touch / fireball /
  lightning bolt scaling with mob level). Without it those mobs fight
  with melee only and feel under-equipped. Likely shape: an
  `OnAttack` trigger template (`@cast_random` with a configurable
  spell list arg) plus a per-mob spell-list field, OR a generic
  `MobileFlags.casts_spells` consulted by the combat tick.
- **`mayor` daily walk path** — The only stock NPC with a hard-coded
  path (`open eastgate / move / unlock western gate / ...`). IronMUD's
  `daily_routine` system already supports paths; the importer could
  synthesise one from the literal string in `spec_procs.c::mayor()`.
  Today: warn-only with an OnAlways `@emote` placeholder.
- **`guild` class-specific practice** — 4 mobs (mage/cleric/warrior/
  thief guildmasters in Midgaard). The C body checks the player's
  class then offers `practice` to spend trains. Blocked on IronMUD
  growing class-specific guild semantics; in the meantime, builders
  can author per-mob dialogue.

#### Medium priority

- **`postmaster` mail system** — 2 mobs (Midgaard + Immortal Inn).
  Players type `mail <recipient>`, write a body, the postmaster
  charges 50 gold. No IronMUD mail subsystem exists. Likely shape:
  a `MobileFlags.postmaster` + a `mail` command that queues messages
  on `CharacterData.unread_mail: Vec<MailMessage>`.
- **`gen_board` bulletin boards** — 4 items (social/freeze/immortal/
  mortal). Players read/write/remove posts via `look board` /
  `write <subject>`. Today imports as a `Misc` item with an OnExamine
  placeholder. Likely shape: new `ItemType::Board` + a `BoardData`
  side-table for posts. Tag with `categories: ["board"]` post-import
  if the builder wants the same item to surface in board-targeting
  triggers.
- **`bank` ATM** — 2 items (atm + cashcard in Midgaard). Players
  `deposit`/`withdraw` to grow gold balances stored on the player.
  Today imports as a `Misc` item with an OnUse placeholder. Likely
  shape: a `bank_balance: i64` on `CharacterData` + a `bank` command
  gated by `flags.bank` on the room or a `bank_terminal` flag on the
  item.
- **`cryogenicist` long-term storage** — 1 mob (Midgaard). Saves a
  player's inventory across reboots in exchange for daily rent. Tied
  to the rent system that doesn't exist; receptionist's lease-style
  partial mapping is the closest analog today.
- **`receptionist` automatic area-binding** — Already maps to
  `MobileFlags.leasing_agent`, but `leasing_area_id` must be authored
  by hand via `medit <id> leasing area <area>`. Could be inferred from
  the keeper's home zone vnum range during the trigger overlay pass.

#### Low priority

- **`snake` per-attack venom** — 8 mobs. C body inflicts a poison
  affect on every successful bite. Today maps to `aggressive` only.
  Once `MobileFlags.poisonous` is wired on bite damage_type, swap the
  mapping to set both flags.
- **`thief` steal action** — 5 mobs. Steals gold from players each
  round. Blocked on a player-side `steal` skill landing first.
- **`pet_shops` purchase dialogue** — 1 room. Stock pet shops let
  players buy a pet by typing `list` / `buy <pet>`; the bought pet
  becomes a follower. No IronMUD pet/familiar system today.
- **`fido` corpse-only filter** — `scavenger` flag picks up any
  ground item; the stock fido eats only corpses. Acceptable
  approximation; revisit if scavenged items pile up oddly.
- **`puff` quote rotation** — currently fires `@say_random` with the
  4 stock quotes from `spec_procs.c`. If a builder wants new lines,
  edit Puff's trigger args via `medit puff trigger`.
- **King's Castle bespoke NPCs** (10 specprocs: `king_welmar`,
  `training_master`, `tom`, `tim`, `peter`, `jerry`, `james`,
  `cleaning`, `castleguard`, `dickndavid`) — each is several hundred
  lines of C describing pathing, dialogue, combat reactions, and
  inter-NPC interactions. No mechanical conversion possible; the
  importer surfaces one Warn per binding naming the real vnum so a
  builder can re-author each manually.

### Other engine features (not imported)

- **Per-room light counters** — Circle tracks cumulative light from
  carried sources at the room level. IronMUD models light per item.
- **Per-zone `reset_mode`** — Circle's "when does this zone reset"
  policy (never / when empty / always). IronMUD spawn points poll
  individually, so there's no direct mapping; the field is silently
  dropped. (`lifespan` *is* consumed as the per-spawn cadence; see
  [CircleMUD zone reset coverage matrix](#circlemud-zone-reset-coverage-matrix).)

## Troubleshooting

**`could not open IronMUD database at <path>: ... resource temporarily unavailable`**
— Sled is detecting an open lock from another process. Stop the running
IronMUD server before importing. Apply mode requires exclusive DB access.

**`refusing to --apply: N blocking warning(s)`** — The mapping layer found
a hard issue (e.g. an area prefix collides with an existing area). Re-run
without `--apply` to see the **BLOCK** entries, then either rename the
source zone, delete the conflicting area in IronMUD, or run against a
fresh DB.

**Encoding / weird characters in descriptions** — The parser reads files
as UTF-8 and treats CR/LF defensively, but legacy CircleMUD files can
contain Latin-1 bytes (umlauts, currency symbols). Convert with
`iconv -f latin1 -t utf-8` before importing if you see replacement-char
glyphs.

**`N exit(s) pointed to vnums outside the imported set; they were not linked.`**
— Cross-zone exits whose destination vnum lives in a zone the importer
didn't see. Either include that zone's `.wld` / `.zon` in the source tree
or accept the dropped exit (you can re-link it manually with `redit`).

**Room flag warnings on every room** — `ROOM_PRIVATE`, `ROOM_TUNNEL`,
`ROOM_DEATH`, `ROOM_NOMAGIC`, `ROOM_SOUNDPROOF`, and `ROOM_NOTRACK` all
import as first-class IronMUD flags now (silent). The remaining
warn-on-import flags are `GODROOM`, `HOUSE`, and `ATRIUM` — use the
mapping JSON to swap `"action": "warn"` for `"action": "drop"` per flag
if you don't want them in the report.

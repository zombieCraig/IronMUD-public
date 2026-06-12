# Cyberware, Humanity, and Cyberpsychosis

Cyberpunk RED–inspired chrome for IronMUD. Any character whose race accepts
grafts can have cyberware installed; every piece costs **Humanity**, and a
soul run dry invites **cyberpsychosis**. The augmented race is *born
chromed*: its racial bonuses are actual pre-installed implants.

## Player model

- **Humanity ceiling** = base CHA × 10, minus **2 per installed piece**
  (**4** for borgware; pieces with humanity loss 0, i.e. fashionware,
  reduce nothing). The ceiling is never stored — it is recomputed from base
  CHA and the installed list. Removing a piece restores its ceiling
  reduction.
- **Installing** charges current Humanity by the item's
  `cyber_humanity_loss` tier (RED tiers: 0 / 2 / 3 / 7 / 14). Races with
  `cyberware_affinity: "adept"` (augmented) pay 3/4, minimum 1
  (2→1, 3→2, 7→5, 14→10).
- **Uninstalling** restores the ceiling but *not* spent Humanity — the soul
  doesn't grow back with the meat.
- **Therapy** (`cyberware_therapy` capability) restores spent Humanity up
  to the current ceiling. Gold pricing is builder-owned.
- **CHA erosion**: every full 10 points of deficit (ceiling − current) is
  −1 effective CHA, applied as a single permanent `CharismaBoost` buff
  sourced `cyberware:humanity`.
- **Cyberpsychosis** (psyche tick, 60s): below 30% humanity the character
  rolls for episodes —
  - 15–29%: `(30 − pct)%` chance of a **dissociation** episode
    (Slow + Luck −3 for 60s);
  - 1–14%: doubled chance (capped 60%); **violent** when in combat or
    company (Frenzy +4 / Rage for 45s — same plumbing as vampire hunger
    frenzy), dissociative when alone;
  - 0%: 75% chance, always violent, 60s, shorter cooldown.
  Episodes never stack onto an active Frenzy/Rage and respect a 300s
  cooldown (120s at zero). Recovery: therapy or chrome removal.

NOTE: this Humanity (0..CHA×10) is unrelated to the vampire 0–10 humanity
scale. UI strings say "Humanity (chrome)".

## Item model (RED slot system)

Cyberware is `ItemType::Cyberware` plus `cyber_*` fields on `ItemData`:

| Field | Meaning |
|---|---|
| `cyber_category` | `fashionware, neuralware, cyberoptic, cyberaudio, cyberarm, cyberleg, internal_body, external_body, borgware` |
| `cyber_foundation` | Base unit providing option slots (neural link, cybereye, cyberaudio suite, cyberarm, cyberleg) |
| `cyber_option_slots` | Slots a foundation provides; 0 = category default (neuralware 5 / cyberoptic 3 / cyberaudio 3 / cyberarm 4 / cyberleg 3) |
| `cyber_slot_cost` | Slots an option consumes; 0 = 1 |
| `cyber_humanity_loss` | Humanity charged on install (0/2/3/7/14) |
| `cyber_paired` | Option needs a slot in BOTH foundations of its category (low-light optics → both eyes) |
| `cyber_exclusive_tag` | At most one installed piece per tag — `speedware` implements the one-speedware rule |

Foundation caps per body: neuralware 1, cyberaudio 1, cyberoptic 2,
cyberarm 2, cyberleg 2. Options require an installed foundation of their
category with free slots; foundations can't be removed while hosting
options (no cascade).

**Installed chrome is not an item.** Installing *consumes* the item and
pushes an `InstalledCyberware` snapshot onto
`CharacterData.cyberware_state` (the tattoo pattern), stamping its
`affects` as permanent buffs sourced `cyberware:<install_id>`. Chrome
never occupies wear slots, can't be dropped/stolen/looted, and survives
death. Uninstalling rebuilds a loose item from the snapshot (immune to
prototype edits/deletion). `wear`/`wield` refuse cyberware items.

## Race matrix (`RaceDefinition.cyberware_affinity`)

| Affinity | Races | Effect |
|---|---|---|
| `adept` | augmented | 3/4 humanity cost, born chromed |
| `normal` (default) | human, mutant, psychic, bioroid, replicant | full cost |
| `incompatible` | synth, revenant | cannot install |

### Augmented: born chromed

The race's old stat block became its **starting kit**, installed free at
character creation (no humanity charge, but each piece still lowers the
ceiling — a fresh augmented sits at a full-but-reduced 86/86 at CHA 10):

| Seed vnum | Piece | Carries |
|---|---|---|
| `cyb-neural-link` | Neuralware foundation | 5 slots |
| `cyb-reflex-booster` | Neuralware option, `speedware` | +1 DEX (old race dex bonus) |
| `cyb-cybereye` ×2 | Cyberoptic foundations | 3 slots each |
| `cyb-lowlight-optics` | Paired cyberoptic option | NightVision (old `dark_adapted` trait) |
| `cyb-muscle-graft` | Internal body | +1 STR and −15 lightning resistance (old race str bonus + weakness) |
| `cyb-adrenal-booster` | Internal body | Gates `racial adrenaline_surge` |

Only `wis −1` stays innate. Seeds are created idempotently on every
startup (`seed_cyberware_prototypes`, skip-existing) since creation
depends on the vnums on every world.

## Capabilities (Rhai, callable from commands AND DG/dialogue triggers)

Builders wire ripperdoc NPCs to these with zero code changes; gold pricing
stays in the calling script.

- `install_cyberware(connection_id, keyword)` — from inventory; validates,
  charges, consumes.
- `install_cyberware_free(connection_id, vnum)` — spawn + install at zero
  humanity charge (creation, admin grants).
- `uninstall_cyberware(connection_id, keyword_or_install_id)` — back to
  inventory; ceiling restored, spent humanity not.
- `cyberware_therapy(connection_id, points)` — restore up to ceiling.
- `get_cyberware_state(connection_id)` / `is_pc_chromed` /
  `has_cyberware(connection_id, key)` / `init_pc_cyberware` /
  `get_character_visible_cyberware(name)` /
  `get_race_cyberware_affinity(race)`.

## Player surface

- `cyberware` (alias `chrome`): humanity ledger + installed list. Admin
  subcommands for testing: `install <item>`, `grant <vnum>`,
  `uninstall <item>`, `therapy <points>`.
- `status`: Humanity bar when chromed.
- `examine <player>`: externally visible chrome only (cyberoptics,
  cyberlimbs, external body, borgware, fashionware — neuralware/
  cyberaudio/internal stay hidden).

## Builder surface

- `oedit <id> type cyberware`, then `oedit <id> cyber
  <category|foundation|slots|slot_cost|humanity|paired|tag> <value>`.
- API: `cyber_*` fields on item create/update. MCP: same on
  `create_item`/`update_item`.

## Key files

| Concern | File |
|---|---|
| Types/constants | `src/types/cyberware.rs`, `ItemData.cyber_*` in `src/types/items.rs` |
| Core logic + unit tests | `src/cyberware/mod.rs` |
| Psyche tick wrapper | `src/ticks/cyberware.rs` (heartbeat `psyche`) |
| Rhai bindings | `src/script/cyberware.rs` |
| Seed kit | `src/seed/items.rs::seed_cyberware_prototypes` |
| Commands | `scripts/commands/cyberware.rhai` (+ `chrome.rhai` alias) |
| Integration tests | `tests/server.rs` (cyberware section) |

# Starting Kits (`admin loadout`)

`admin loadout` edits **starting kits** — the gold and item prototypes granted to a freshly-created character. Both the chosen **class** and the chosen **race** can contribute a kit, and the two **stack**: a new character receives the union of both item lists and the sum of both gold values.

> **Admin-only.** Starting equipment is server policy, not world-building, so this is an `admin` subcommand (it replaced the old builder-facing `cedit` command). Builders cannot edit starting kits.

JSON class/race definitions remain the source of truth for skills, bonuses, abilities, and languages; `admin loadout` only overlays the kit fields (`starting_items` + `starting_gold`), persisted in the `class_loadouts` / `race_loadouts` sled trees and re-applied on startup and on `admin reload`.

## Subcommands

`<kind>` is either `class` or `race`. `<id>` is the class or race id (the same string used in `class_info` / `get_race_info`, e.g. `warrior`, `replicant`).

| Subcommand | Effect |
|---|---|
| `admin loadout list` | List every loaded class **and** race with a one-line kit summary |
| `admin loadout <kind> <id>` | Show the kit for `<id>` (alias: `… <id> show`) |
| `admin loadout <kind> <id> gold <amount>` | Set starting gold (non-negative integer) |
| `admin loadout <kind> <id> items add <vnum>` | Add an item prototype to the kit (vnum must exist) |
| `admin loadout <kind> <id> items remove <vnum>` | Remove an item from the kit |
| `admin loadout <kind> <id> items clear` | Empty the item kit |
| `admin loadout help` | Show usage banner |

Use `admin loadout list` if you're not sure of the valid class/race ids.

## When the kit applies

`scripts/commands/create.rhai` applies kits during character creation, immediately after race-based starting languages:

1. **Class kit** — `starting_gold` (if `> 0`) is set via `set_character_gold`, then each class `starting_items` vnum is spawned into inventory.
2. **Race kit** — race `starting_gold` is **added** on top via `add_character_gold`, then each race `starting_items` vnum is spawned.

So a private-investigator class that grants a `badge` plus a `replicant` race that grants a `replicant manual` yields a character holding **both**; a human PI gets only the badge.

Missing item vnums **do not block** character creation — they log a `[loadout] <kind> <id> starting_items references missing vnum: <vnum>` line to all online builders so the kit can be fixed at leisure.

## Example

```
> admin loadout list
=== Class Starting Kits ===
  private_investigator — gold=50, items=iron:badge
  warrior — gold=100, items=(none)
=== Race Starting Kits ===
  human — gold=0, items=(none)
  replicant — gold=10, items=iron:replicant_manual

> admin loadout class warrior gold 100
class warrior starting gold set to 100.

> admin loadout race replicant items add iron:replicant_manual
Added iron:replicant_manual to race replicant starting kit.

> admin loadout race replicant
Starting kit for race replicant:
  gold:  10
  items:
    - iron:replicant_manual
```

## Related

- [Items](items.md) — authoring the item prototypes referenced by a kit
- JSON class definitions in `scripts/data/classes/` and race definitions in `scripts/data/races_*.json` (source of truth for everything else about a class/race)

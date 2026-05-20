# Class Starting Kits (cedit)

`cedit` edits the per-class **starting kit** — the gold and item prototypes granted to a freshly-created character of that class. JSON class definitions remain the source of truth for skills, bonuses, and languages; `cedit` only overlays the kit fields, persisted in the `class_loadouts` sled tree.

Access: `builder` (also available to admins).

## Subcommands

| Subcommand | Effect |
|---|---|
| `cedit list` | List every loaded class with a one-line kit summary (gold + item vnums) |
| `cedit <class>` | Show the kit for `<class>` (alias: `cedit <class> show`) |
| `cedit <class> gold <amount>` | Set starting gold (non-negative integer) |
| `cedit <class> items add <vnum>` | Add an item prototype to the kit (vnum must exist) |
| `cedit <class> items remove <vnum>` | Remove an item from the kit |
| `cedit <class> items clear` | Empty the item kit |
| `cedit help` | Show usage banner |

`<class>` is the class id (the same string used in `class_info`, e.g. `warrior`, `mage`). Use `cedit list` if you're not sure of the valid ids.

## When the kit applies

`scripts/commands/create.rhai` applies the kit during character creation immediately after race-based starting languages:

1. `starting_gold` (if `> 0`) is set on the new character via `set_character_gold`.
2. Each `starting_items` vnum is spawned into the new character's inventory.

Missing item vnums **do not block** character creation — they log a `[cedit] <class> starting_items references missing vnum: <vnum>` line to all online builders so you can fix the kit at leisure.

## Example

```
> cedit list
Class Starting Kits
  warrior — gold=50, items=(none)
  mage — gold=20, items=(none)

> cedit warrior gold 100
warrior starting gold set to 100.

> cedit warrior items add rusty_sword
Added rusty_sword to warrior starting kit.

> cedit warrior items add leather_jerkin
Added leather_jerkin to warrior starting kit.

> cedit warrior
Starting kit for warrior
  gold:  100
  items:
    - rusty_sword
    - leather_jerkin
```

## Related

- [Items](items.md) — authoring the item prototypes referenced by a kit
- JSON class definitions in `scripts/data/classes/` (source of truth for everything else about a class)

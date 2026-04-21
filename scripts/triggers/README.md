# Triggers

Reusable trigger scripts for rooms, items, and mobiles. Each exports:

```rhai
fn run_trigger(entity_id, connection_id, context)
```

**Room triggers:** `forest_ambiance`, `trapped_room`, `safe_zone_exit`, `festival_bonfire`

**Item triggers:** `cursed_item`, `soulbound_drop`, `sticky_trap`, `ancient_tome`, `magic_potion`, `smart_watch`

**Mobile triggers:** `guard_greet`, `innkeeper_greet`, `hostile_greet`, `quest_giver`, `secret_word`

See [Builder Guide: Triggers](../../docs/builder/triggers.md) for usage.

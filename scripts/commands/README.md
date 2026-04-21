# Commands

Rhai scripts implementing player and builder commands. Each file exports:

```rhai
fn run_command(args, connection_id)
```

Commands are hot-reloaded when modified. Metadata (help, permissions) is in `../commands.json`.

**Categories:**
- Movement: `go`, `exits`, `recall`
- Communication: `say`, `tell`, `shout`, `emote`
- Items: `get`, `drop`, `inventory`, `wear`, `eat`, `drink`
- Combat: `attack`, `flee`, `diagnose`
- Shops: `buy`, `sell`, `list`, `appraise`
- OLC editors: `redit`, `oedit`, `medit`, `aedit`, `spedit`, `recedit`
- Admin: `setadmin`, `setbuilder`, `shutdown`

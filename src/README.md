# Source

Rust source code for the IronMUD server.

| File | Purpose |
|------|---------|
| `main.rs` | Server entry point, connection handling, command loop |
| `lib.rs` | Core types (Room, Item, Mobile, Character, etc.) |
| `db.rs` | Sled database wrapper and migrations |
| `completion.rs` | Tab completion for all commands |
| `telnet.rs` | Telnet/MXP protocol handling |
| `claude.rs` | Claude API integration |
| `gemini.rs` | Gemini API integration |
| `matrix.rs` | Matrix chat bridge |

| Directory | Purpose |
|-----------|---------|
| `bin/` | Additional binary targets |
| `script/` | Rhai function registrations by domain |

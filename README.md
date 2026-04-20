# IronMUD

A Rust-based MUD (multi-user dungeon) server with hot-reloadable Rhai
scripting, an embedded sled database, and optional Discord / AI-assisted
building integrations.

**Website:** https://ironmud.games *(coming soon)*

## Try the public test server

```
telnet play.ironmud.games 4000
```

Or connect with any MUD client (MUSHclient, Mudlet, tintin++, etc.) pointed
at `play.ironmud.games:4000`.

Create an account at the login prompt with `create <name> <password>` and
explore the seeded demo world (Oakvale Village, Whispering Woods, Iron Keep,
Shadowfang Caves, Hilltop Farm).

This is a **proof-of-concept server**. Player data has no retention guarantee
and may be wiped at any time. Telnet is unencrypted — don't reuse a password
you care about. See [PRIVACY.md](PRIVACY.md) for specifics.

## Features

- Single-binary async Rust server built on `tokio`
- Hot-reloadable `Rhai` scripts for commands and triggers — edit game content
  without restarting the server
- Embedded `sled` database — no external DB needed
- Argon2 password hashing, login lockout, idle timeouts
- Admin and builder roles with per-area ownership for content creators
- Optional Discord bridge for chat/notifications
- Optional AI assistance (Anthropic) for builders writing prose
- REST API (loopback-only by default) for remote content-management tooling
  via a companion MCP server

## Run it locally

Prerequisites: Rust 1.85+ (`rustup install stable`).

```bash
git clone https://github.com/zombieCraig/IronMUD-public.git
cd IronMUD-public
cargo build --release
./target/release/ironmud --port 4000 --database ./ironmud.db
```

Connect with `telnet localhost 4000`. The first account created becomes the
admin + builder.

### Optional environment variables

Set these before launch if you want the corresponding feature:

| Var | Effect |
|-----|--------|
| `IRONMUD_API_ENABLED=true` | Starts the REST API on `--api-port` (default 4001) |
| `IRONMUD_API_BIND=127.0.0.1` | Restricts the REST API to loopback |
| `IRONMUD_API_KEY=...` | Bearer token required for REST API calls |
| `DISCORD_TOKEN`, `DISCORD_CHANNEL_ID` | Enable Discord bridge |
| `CLAUDE_API_KEY`, `CLAUDE_MODEL` | Enable Anthropic-backed builder help |

See `src/main.rs` for the full CLI and `src/` for the integration modules.

## Documentation

Game mechanics, builder guide, and design notes live in [`docs/`](docs/).

## Contributing

Issues and PRs welcome. Please include:

- Rust: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
- A short description of the change and why it matters

CI runs format, clippy, test, and `cargo audit` on every PR — make sure those
are green.

## License

[MIT](LICENSE)

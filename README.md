# IronMUD

An asynchronous, event-driven Multi-User Dungeon (MUD) engine written in Rust.

IronMUD emphasizes hot-reloading of game logic through scripting, providing a dynamic development environment for MUD administrators and content creators. Edit game commands and behaviors without restarting the server.

**Website:** https://ironmud.games

## Try the public test server

```
telnet play.ironmud.games 4000
```

Or connect with any MUD client (MUSHclient, Mudlet, tintin++, etc.) pointed at `play.ironmud.games:4000`. Create an account at the login prompt with `create <name> <password>` and explore the seeded demo world.

This is a **proof-of-concept server**. Player data has no retention guarantee and may be wiped at any time. Telnet is unencrypted — don't reuse a password you care about.

## Quick Start

```bash
# Install Rust (if needed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/zombieCraig/IronMUD-public.git
cd IronMUD-public
cargo build --release

# Run the server
cargo run --release --bin ironmud

# Connect (in another terminal)
telnet localhost 4000
```

The first character created becomes an administrator.

## Documentation

| Guide | Description |
|-------|-------------|
| [Installation](docs/installation.md) | Server setup, configuration, and upgrading |
| [Getting Started](docs/getting-started.md) | Demo world walkthrough and first steps |
| [Player Guide](docs/player-guide.md) | How to play the game |
| [Builder Guide](docs/builder-guide.md) | Creating game content with OLC |
| [Import Guide](docs/import-guide.md) | Importing legacy MUD content (CircleMUD, tbaMUD) |
| [Admin Guide](docs/admin-guide.md) | Server administration, Matrix, and AI integration |

## Features

- **Hot-reloading** — edit Rhai scripts while the server runs
- **Online Creation (OLC)** — build rooms, items, NPCs, areas, quests, and dialogue trees in-game
- **Legacy MUD imports** — translate CircleMUD 3.x and tbaMUD worlds (rooms, mobiles, items, zone resets, shops, quests, DG triggers)
- **DG Scripts** — native Rust interpreter for tbaMUD's trigger language
- **Branching NPC dialogue** — quest hooks, conditions, effects, per-player cooldowns
- **Quests** — kill / fetch / visit / flag objectives with prereqs, time limits, party kill credit
- **NPC simulation** — Sims-style needs, relationships, daily routines, area immigration
- **Languages** — per-skill comprehension with garbled speech for non-speakers
- **Property rental** — player housing with secure storage and access controls
- **Combat system** — body-part wounds, weapon skills, charm/summon/sleep/blind spells, the bash skill
- **Embedded database** — Sled, no external setup
- **MXP support** — clickable links in compatible clients
- **Matrix integration** — optional bridge to Matrix chat rooms
- **AI assistance** — optional Claude/Gemini integration for description writing

## Technical Stack

- **Language**: Rust (2024 Edition)
- **Async Runtime**: Tokio
- **Scripting Engine**: Rhai
- **Database**: Sled (embedded)
- **Password Hashing**: Argon2

## Project Status

In active development. The core engine — networking, persistence, scripting, OLC, combat, simulation, importers — is functional and used to run live worlds. New subsystems land regularly; see [docs/builder/](docs/builder/) for current authoring surfaces.

## Running Tests

```bash
cargo test
```

## Contributing

Issues and pull requests are welcome at the project repository.

## License

See LICENSE file for details.

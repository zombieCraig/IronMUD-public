# Installation Guide

## Requirements

- **Rust**: 2024 Edition (1.85+)
- **Operating System**: Linux, macOS, or Windows

## Quick Start (Development)

1. **Install Rust** (if not already installed):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Clone the repository**:
   ```bash
   git clone <repository-url>
   cd IronMUD
   ```

3. **Build the project**:
   ```bash
   cargo build --release
   ```

4. **Run the server**:
   ```bash
   cargo run --release --bin ironmud
   ```

The server listens on TCP port 4000 by default.

## Production Installation (Ubuntu/Debian)

For production deployment, use the included installation script:

```bash
sudo ./install.sh
```

This script:
- Installs build dependencies and Rust toolchain
- Creates a dedicated `ironmud` system user
- Builds and installs to `/opt/ironmud`
- Sets up a systemd service with security hardening
- Starts the server automatically

After installation:
```bash
sudo systemctl status ironmud      # Check status
sudo journalctl -u ironmud -f      # View logs
telnet localhost 4000              # Connect
```

## Configuration

### Command-Line Options

```bash
ironmud [OPTIONS]

Options:
  -p, --port <PORT>                  Port to listen on [default: 4000]
  -d, --database <DATABASE>          Database path [default: ironmud.db]
      --control-socket <PATH>        Unix control socket path
                                     [default: <database-dir>/control.sock]
  -h, --help                         Print help
```

Example:
```bash
./ironmud --port 5000 --database /var/lib/ironmud/data.db
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `IRONMUD_DATABASE` | Database path | `ironmud.db` |
| `IRONMUD_CONTROL_SOCKET` | Unix control socket path used by `ironmud-admin broadcast` | `<database-dir>/control.sock` |

The `IRONMUD_DATABASE` environment variable sets the database path for both the server and admin tool. Precedence: CLI flag (`-d`) > environment variable > default (`ironmud.db`).

`IRONMUD_CONTROL_SOCKET` only needs to be set if you want to override the default location. Both the server and the admin tool derive the default from the database path, so a standard install requires no configuration. See the [Admin Guide](admin-guide.md#control-socket) for details.

When installed via `install.sh`, the admin wrapper at `/usr/local/bin/ironmud-admin` automatically sets this to `/opt/ironmud/data/ironmud.db`.

### Defaults

- **Telnet Port**: 4000
- **Database**: `ironmud.db` directory (created automatically)

## Upgrading

### Systemd Installation

```bash
cd /path/to/IronMUD
git pull
sudo ./install.sh
```

The install script automatically detects existing installations and:
- Stops the service gracefully (saving all active players)
- Backs up the database to `data.backup-TIMESTAMP/`
- Updates binaries and scripts (backing up modified scripts to `.backup` files)
- Preserves your systemd service configuration
- Restarts the service

#### Warning Online Players Before an Upgrade

Pass `--warn-seconds N` to broadcast a restart notice to every logged-in
player, then pause for `N` seconds before the build starts. This is only
useful for upgrades to a running server (the warning is skipped if the
service is already stopped):

```bash
sudo ./install.sh --warn-seconds 300
```

The default is `0` (no warning, no sleep). Under the hood this runs
`ironmud-admin broadcast` against the control socket — if the admin tool
isn't installed yet (first install) or the socket is unreachable, the
warning is skipped with a log message and the install proceeds.

### Manual Installation

1. **Pull latest changes**:
   ```bash
   git pull
   ```

2. **Rebuild**:
   ```bash
   cargo build --release
   ```

3. **Restart the server**. Database migrations run automatically on startup.

## Running Tests

```bash
cargo test
```

## First User

The first character created on a fresh database automatically becomes an administrator with full builder permissions.

## Next Steps

- [Getting Started](getting-started.md) - Explore the demo world and start building
- [Player Guide](player-guide.md) - How to play the game
- [Admin Guide](admin-guide.md) - Server administration
- [Builder Guide](builder-guide.md) - Creating game content

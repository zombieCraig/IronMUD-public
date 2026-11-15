# Installation Guide

> Putting a server on the internet? Start with the [Hosting guide](hosting.md):
> it compares the ways to run IronMUD (cloud-init, Terraform, Docker, Fly.io)
> and covers TLS, monitoring and backups.

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

   The Matrix and Discord chat bridges are cargo features (`matrix`, `discord`), both on by
   default. To build without them (smaller binary, no OpenSSL dependency):
   ```bash
   cargo build --release --no-default-features              # neither bridge
   cargo build --release --no-default-features -F discord   # Discord only
   ```
   A server built without a bridge logs a warning if that bridge's environment variables are set.

4. **Run the server**:
   ```bash
   cargo run --release --bin ironmud
   ```

The server listens on TCP port 4000 by default.

## Cloud Server in One Step (cloud-init)

On a fresh Ubuntu 22.04+ / Debian 12+ VPS (AWS Lightsail or EC2,
DigitalOcean, Hetzner, Vultr, Linode, ...), paste
[`deploy/cloud-init/ironmud.yaml`](../deploy/cloud-init/ironmud.yaml) into
the provider's **user data** box when creating the server. A few minutes
after boot IronMUD is installed, running and health-checked. Progress is in
`/var/log/ironmud-bootstrap.log`.

Prefer infrastructure as code? [`deploy/terraform/`](../deploy/terraform/README.md)
has Terraform/OpenTofu modules for AWS Lightsail, DigitalOcean and Hetzner
that create the server, its firewall and this same first-boot setup in one
`apply`.

Without cloud-init, SSH in and run the same bootstrap by hand:

```bash
curl -fsSL https://github.com/zombieCraig/IronMUD-public/releases/latest/download/bootstrap.sh | sudo bash
```

The bootstrap installs the latest prebuilt release (below), adds a swap
file on servers with less than 2 GB of RAM, and configures the `ufw`
firewall to allow SSH and the game port only. Settings are environment
variables or lines in `/etc/ironmud/bootstrap.env`:

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONMUD_VERSION` | `latest` | Release tag to install |
| `IRONMUD_TLS_DOMAIN` | *(empty)* | Domain for encrypted connections on port 4443 (see below) |
| `IRONMUD_TLS_EMAIL` | *(empty)* | Let's Encrypt expiry-notice email |
| `IRONMUD_TLS_ONLY` | `0` | `1` closes plaintext port 4000 once TLS is up |
| `IRONMUD_TLS_DNS_WAIT` | `0` | Seconds to wait for the TLS domain to resolve before skipping TLS |
| `IRONMUD_FIREWALL` | `1` | `0` leaves the server firewall alone |
| `IRONMUD_SWAP_MB` | `1024` | Swap file size on low-memory servers; `0` never adds one |
| `IRONMUD_RELEASE_REPO` | `zombieCraig/IronMUD-public` | GitHub repo to install releases from |

Re-running the bootstrap is safe: it upgrades to the requested release and
re-applies these settings. Also open the same ports in your cloud provider's
firewall (22 and 4000; plus 80 and 4443 with TLS).

**TLS:** without a domain, players connect with plain telnet and the
bootstrap warns that passwords cross the network unencrypted. Point a
domain's DNS (A/AAAA record) at the server *before* running the bootstrap
with `IRONMUD_TLS_DOMAIN=mud.example.com`, and it installs HAProxy, gets a
Let's Encrypt certificate (renewed automatically), and serves TLS on 4443.
Plain 4000 stays open for clients without TLS support unless
`IRONMUD_TLS_ONLY=1`. If the domain doesn't resolve yet or the certificate
can't be issued, the bootstrap skips TLS with a message instead of failing;
fix DNS and re-run it. Don't put passwords or API keys in user data: the
instance metadata service exposes it.

## Production Installation from a Prebuilt Release (Ubuntu/Debian)

The fastest way onto a server: no Rust toolchain, no compile. Prebuilt
Linux binaries for x86_64 and aarch64 (ARM, e.g. AWS Graviton, Hetzner CAX,
Oracle Ampere) are published on the
[releases page](https://github.com/zombieCraig/IronMUD-public/releases).
They need glibc 2.35+ (Ubuntu 22.04+, Debian 12+).

```bash
curl -fsSLO https://github.com/zombieCraig/IronMUD-public/releases/latest/download/ironmud-x86_64-linux.tar.gz
curl -fsSLO https://github.com/zombieCraig/IronMUD-public/releases/latest/download/SHA256SUMS
sha256sum -c --ignore-missing SHA256SUMS
tar xzf ironmud-x86_64-linux.tar.gz
cd ironmud-x86_64-linux
sudo ./install.sh
```

(Use `ironmud-aarch64-linux.tar.gz` on ARM.) `install.sh` notices it is
running from an unpacked release and installs the binaries as-is.

Or let the script fetch and verify the release itself, from any copy of
`install.sh`:

```bash
sudo ./install.sh --from-release            # latest release
sudo ./install.sh --from-release v0.2.0     # a specific tag
sudo ./install.sh --from-release ./ironmud-x86_64-linux.tar.gz   # a local tarball
```

Downloads are checked against the release's `SHA256SUMS`; a missing or
mismatched checksum aborts the install. Set `IRONMUD_RELEASE_REPO=owner/name`
(or pass `--repo owner/name`) to install from a fork's releases.

The install itself (system user, `/opt/ironmud`, systemd unit, admin
wrapper) is the same as the source install below. A copy of `install.sh`
and `ironmud-sync.sh` is kept in `/opt/ironmud/` for later upgrades. It finishes by waiting
for `ironmud-admin health` to report the server healthy.

## Production Installation from Source (Ubuntu/Debian)

To build on the server itself, run the installation script from a git checkout:

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

## Docker

Each release publishes a multi-arch (amd64/arm64) image,
`ghcr.io/zombiecraig/ironmud-public:<tag>` (also `:latest`). It runs as a
non-root user, keeps the database in a volume at `/opt/ironmud/data`, and
has a built-in health check.

```bash
docker run -d --name ironmud -p 4000:4000 \
  -v ironmud-data:/opt/ironmud/data --restart unless-stopped \
  ghcr.io/zombiecraig/ironmud-public:latest
```

or with Compose: `docker compose -f deploy/docker/compose.yaml up -d`
([`deploy/docker/compose.yaml`](../deploy/docker/compose.yaml)). Set
`IRONMUD_IMAGE` to pin a version and `IRONMUD_PORT` to change the host port.

- **One container per volume.** The database is single-writer: never run two
  containers on the same data, and never scale the service above 1.
- **Admin commands.** Commands that talk to the running server work live
  (`docker exec ironmud ironmud-admin health`, `... broadcast "hi"`).
  Commands that edit the database (`user`, `settings`, `api-key`, ...) need
  the server stopped:
  `docker stop ironmud && docker run --rm -v ironmud-data:/opt/ironmud/data --entrypoint ironmud-admin ghcr.io/zombiecraig/ironmud-public:latest user list && docker start ironmud`.
- **Configuration** uses the same environment variables as everywhere else
  (`-e IRONMUD_API_ENABLED=true`, SMTP, Discord, Matrix, ...).
- **Backups:** stop the container and copy the volume, e.g.
  `docker run --rm -v ironmud-data:/data -v "$PWD":/backup ubuntu tar czf /backup/ironmud-data.tgz -C /data .`
- **Upgrades:** pull the new image and recreate the container; the volume
  keeps the world and players.
- **TLS:** put a TLS proxy in front, as in [TLS / Encrypted
  Connections](#tls--encrypted-connections), and use the PROXY-protocol port
  (`IRONMUD_PROXY_PORT`) so players keep their real IPs.

To build the image yourself from release tarballs:
`deploy/package-release.sh --out dist && docker build -f deploy/docker/Dockerfile -t ironmud dist/`.

### Fly.io

[`deploy/fly/fly.toml`](../deploy/fly/fly.toml) runs the published image on
Fly.io with a volume, plaintext telnet on 4000, TLS on 4443 (Fly terminates
TLS and passes each player's IP with the PROXY protocol), a `/healthz`
check, and `/metrics` scraped into Fly's free managed Grafana. The steps
are at the top of the file. Raw TCP for IPv4 players needs a dedicated IPv4
address (`fly ips allocate-v4`), which Fly bills monthly.

## Configuration

### Command-Line Options

```bash
ironmud [OPTIONS]

Options:
  -p, --port <PORT>                  Port to listen on [default: 4000]
  -d, --database <DATABASE>          Database path [default: ironmud.db]
      --control-socket <PATH>        Unix control socket path
                                     [default: <database-dir>/control.sock]
      --bind <ADDR>                  Game port bind address [default: 0.0.0.0]
      --proxy-port <PORT>            Extra loopback port for a TLS proxy sending
                                     PROXY protocol v1 headers [default: off]
      --proxy-bind <ADDR>            Bind address for --proxy-port [default: 127.0.0.1]
      --ops-enabled <BOOL>           Serve /healthz and /metrics [default: true]
      --ops-port <PORT>              Ops endpoints port [default: 4002]
      --ops-bind <ADDR>              Ops endpoints bind address [default: 127.0.0.1]
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
| `IRONMUD_BIND` | Game port bind address | `0.0.0.0` |
| `IRONMUD_PROXY_PORT` / `IRONMUD_PROXY_BIND` | PROXY-protocol port for a local TLS proxy (see [TLS](#tls--encrypted-connections)) | off / `127.0.0.1` |
| `IRONMUD_OPS_ENABLED` / `IRONMUD_OPS_PORT` / `IRONMUD_OPS_BIND` | Health and metrics endpoints; see the [Admin Guide](admin-guide.md#health-and-metrics-endpoints) | `true` / `4002` / `127.0.0.1` |
| `IRONMUD_LOOPBACK_BUDGET` | Multiplier on the per-IP connection/login/creation/email limits for connections from localhost (e.g. a ttyd web gateway), which all share one budget. `0` = unlimited | `8` |

The `IRONMUD_DATABASE` environment variable sets the database path for both the server and admin tool. Precedence: CLI flag (`-d`) > environment variable > default (`ironmud.db`).

Every connection arriving from localhost shares one rate-limit budget. If you run a local web gateway such as ttyd, all of its players count as one client, so they get `IRONMUD_LOOPBACK_BUDGET` times the normal per-IP limits (64 simultaneous connections and 40 failed logins a minute at the default of 8). Raise it for a busy gateway; `0` removes the limits on localhost altogether (the pre-hardening behaviour, which lets anyone behind the gateway brute-force logins).

`IRONMUD_CONTROL_SOCKET` only needs to be set if you want to override the default location. Both the server and the admin tool derive the default from the database path, so a standard install requires no configuration. Live-server commands such as `ironmud-admin broadcast` go through it; see the [Admin Guide](admin-guide.md).

When installed via `install.sh`, the admin wrapper at `/usr/local/bin/ironmud-admin` automatically sets this to `/opt/ironmud/data/ironmud.db`.

### Defaults

- **Telnet Port**: 4000
- **Database**: `ironmud.db` directory (created automatically)

## Upgrading

### Systemd Installation

From a prebuilt release:

```bash
sudo /opt/ironmud/install.sh --from-release          # or: --from-release v0.2.0
```

From source:

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

The first character created on a fresh database automatically becomes an administrator with full builder permissions when it's created from the server itself (a loopback connection, e.g. `telnet localhost 4000`). From anywhere else, set `IRONMUD_FIRST_USER_ADMIN=true` before creating it, or grant it afterwards with `ironmud-admin user grant-admin <name>`.

## TLS / Encrypted Connections

IronMUD speaks plaintext telnet on its TCP port. This is acceptable on a LAN or a private VPN but exposes login passwords on the wire if the server is reachable across the public internet. The supported deployment pattern is to run a TLS-terminating proxy in front of the MUD process — most MUD clients do not speak TLS natively, so doing this in-process would not help typical users.

### stunnel example

`stunnel` is the simplest option. Install it from your distro's package manager, then create `/etc/stunnel/ironmud.conf`:

```ini
[ironmud-tls]
accept = 0.0.0.0:4443
connect = 127.0.0.1:4000
cert = /etc/letsencrypt/live/mud.example.com/fullchain.pem
key  = /etc/letsencrypt/live/mud.example.com/privkey.pem
```

Players who use a TLS-capable client (Mudlet, MUSHclient with the SSL plugin, telnet wrappers like `socat`) connect to `mud.example.com:4443`. To make plaintext access from outside the host impossible, bind IronMUD itself to loopback with `IRONMUD_BIND=127.0.0.1` (or `--bind`).

With stunnel, every TLS player reaches IronMUD from `127.0.0.1`, so they share one rate-limit budget and site bans can't single them out. HAProxy (below) avoids that.

### haproxy example (recommended)

HAProxy can pass each player's real address to IronMUD with the PROXY
protocol. Enable IronMUD's PROXY-protocol port (`IRONMUD_PROXY_PORT=4010`;
it binds `127.0.0.1` by default, set with `IRONMUD_PROXY_BIND`), then point
HAProxy at it with `send-proxy`:

```
frontend ironmud_tls
    bind :4443 ssl crt /etc/haproxy/certs/mud.pem
    mode tcp
    default_backend ironmud_local

backend ironmud_local
    mode tcp
    timeout server 2h
    server mud 127.0.0.1:4010 send-proxy
```

Every connection to the PROXY-protocol port must start with the header, and
whatever address the header names is trusted, so never expose that port
beyond the host. The cloud-init bootstrap above sets all of this up.

### Certificate renewal

Both options work cleanly with Let's Encrypt's `certbot` in standalone or webroot mode. Reload the proxy (`systemctl reload stunnel` / `haproxy`) after each renewal — IronMUD itself is unaffected.

## Next Steps

- [Getting Started](getting-started.md) - Explore the demo world and start building
- [Player Guide](player-guide.md) - How to play the game
- [Admin Guide](admin-guide.md) - Server administration
- [Builder Guide](builder-guide.md) - Creating game content

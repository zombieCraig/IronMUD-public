# Hosting Your Own IronMUD Server

This guide helps you pick a way to run IronMUD on the internet and points at
the detailed instructions for each. Everything here installs a prebuilt
release from
[IronMUD-public releases](https://github.com/zombieCraig/IronMUD-public/releases),
so you never need a Rust toolchain on the server.

## What IronMUD needs

- **A raw TCP port** (4000) that players' MUD clients connect to. Hosts that
  only accept HTTP traffic (most "serverless" and web-app platforms) can't
  run it.
- **A persistent disk** for the database (`/opt/ironmud/data`). It must
  survive restarts and redeploys.
- **Exactly one running copy per database.** The database is single-writer:
  never run two servers, containers or replicas on the same data.
- **Very little hardware.** An idle server with the demo world uses about
  65 MB of RAM; the smallest plans (512 MB) are plenty, and the bootstrap
  adds a swap file on them as a safety margin. x86_64 and ARM (aarch64) are
  both supported.

## Pick a path

| You want... | Use | Guide |
|---|---|---|
| A VPS, set up by pasting one file when you create it | cloud-init | [Installation: Cloud Server in One Step](installation.md#cloud-server-in-one-step-cloud-init) |
| The server, firewall and setup created from code | Terraform / OpenTofu (Lightsail, DigitalOcean, Hetzner) | [`deploy/terraform/README.md`](../deploy/terraform/README.md) |
| Containers (home server, NAS, existing Docker host) | Docker / Compose | [Installation: Docker](installation.md#docker) |
| A managed platform, no server to look after | Fly.io | [Installation: Fly.io](installation.md#flyio) |
| An existing Linux server, step by step | Release install script | [Installation: Prebuilt Release](installation.md#production-installation-from-a-prebuilt-release-ubuntudebian) |
| To modify the Rust code | Build from source | [Installation: From Source](installation.md#production-installation-from-source-ubuntudebian) |

The cloud-init, Terraform and manual paths all run the same
`bootstrap.sh`/`install.sh`, so a server set up one way is managed the same
as any other: a systemd service, files in `/opt/ironmud`, and
`ironmud-admin` on the path.

## Choosing a provider

Approximate prices for the smallest suitable plan (late 2026; check the
provider, prices move):

| Provider | Plan | Price | Notes |
|---|---|---|---|
| AWS Lightsail | Nano, 512 MB | $5/mo ($3.50 IPv6-only) | Static IP included. Terraform module. |
| DigitalOcean | Basic, 512 MB | $4/mo | Free monitoring agent with alerts. Terraform module. |
| Hetzner Cloud | CX23 (x86) / CAX11 (ARM) | ~€5/mo | Lots of RAM for the price; plan availability varies by location. Terraform module. |
| Vultr, Linode, OVH, ... | smallest | ~$3.50–5/mo | Use the cloud-init file. |
| Fly.io | shared-cpu-1x, 512 MB | usage-based | Raw TCP for IPv4 players needs a dedicated IPv4 address, billed monthly. |
| Oracle Cloud | Always Free ARM | free | Works (aarch64 release), but free capacity is often unavailable. |

Pick a region close to most of your players; MUDs are sensitive to latency.

## Encryption (TLS)

Plain telnet sends passwords unencrypted. Every path can add TLS on port
4443 with a free, auto-renewing Let's Encrypt certificate once you have a
domain name pointing at the server:

- cloud-init / bootstrap / Terraform: set `IRONMUD_TLS_DOMAIN` (or the
  `tls_domain` variable). HAProxy terminates TLS and passes each player's
  real IP to the game, so bans and per-IP limits still work.
- Fly.io: built in; Fly terminates TLS on 4443.
- Docker or your own proxy: see [TLS / Encrypted
  Connections](installation.md#tls--encrypted-connections).

Keep port 4000 open too unless all your players' clients support TLS
(Mudlet does: tick "Secure" when connecting).

## Monitoring

| What | How |
|---|---|
| Is it healthy? | `ironmud-admin health` on the server, or `GET 127.0.0.1:4002/healthz` |
| Numbers over time | `GET 127.0.0.1:4002/metrics` (Prometheus format): players, logins, commands, errors, scripts failing to load, tick health, database size |
| Graphs without running anything | DigitalOcean's monitoring agent (Terraform enables it), Fly.io's built-in metrics |
| A real dashboard | [`deploy/monitoring/`](../deploy/monitoring/): Grafana dashboard plus an Alloy config for Grafana Cloud's free tier; setup in the [admin guide](admin-guide.md#grafana-cloud-free-tier-with-alloy) |
| "Tell me when it's down" | Any external TCP monitor (e.g. UptimeRobot's free plan) on port 4000 or 4443 |

The ops port (4002) listens on the server itself only. Don't expose it: it
has no authentication.

## Backups

The database is a directory of files that the running server writes
continuously. **Don't copy it while the server runs**; the copy can be
inconsistent. Choose one:

- **Provider snapshots/backups** (Lightsail snapshots, DigitalOcean and
  Hetzner backups: `backups = true` in Terraform). No downtime, whole-server
  restore. The easiest option.
- **`ironmud-sync.sh`**: `sudo /opt/ironmud/ironmud-sync.sh backup` stops the
  server for a few seconds, archives the database and custom trigger
  scripts to `/opt/ironmud/backups/`, and starts it again. `restore <file>`
  brings one back, on the same or a new server. Copy the archives off the
  server. If you run it from a nightly timer, players get disconnected
  briefly each night.
- **Docker:** stop the container, archive the volume, start it again (the
  command is in [Installation: Docker](installation.md#docker)).

The installer also keeps a copy of the data directory before every upgrade
(`/opt/ironmud/data.backup-<timestamp>`).

## Upgrading

| Installed with | Upgrade with |
|---|---|
| cloud-init, Terraform, release install | `sudo /opt/ironmud/install.sh --from-release` (or `--from-release v0.3.0`) |
| bootstrap (also re-applies TLS/firewall settings) | `sudo bash /root/ironmud-bootstrap.sh` |
| Docker / Compose | pull the new image, recreate the container |
| Fly.io | `fly deploy --config deploy/fly/fly.toml` |

## Security checklist

- **No secrets in user data or Terraform variables.** Cloud user data can be
  read from the server's metadata service. Configure SMTP, Discord, Matrix
  and API keys after the first boot.
- **Restrict SSH** to your own address where you can (`ssh_allowed_cidrs` in
  Terraform, or your provider's firewall).
- **Keep internal ports internal.** 4002 (health/metrics) and the PROXY port
  (4010) listen on the server itself only; the bootstrap and Terraform
  firewalls never open them.
- **The REST API is off by default.** If you enable it, set
  `IRONMUD_API_BIND=127.0.0.1` unless something off the server needs it; it
  currently defaults to all interfaces.
- **Create your admin account first.** The first character becomes an
  administrator only when it's created from the server itself: SSH in and
  run `telnet localhost 4000` (or `nc localhost 4000`). Players connecting
  from outside, including you from home, get normal accounts. If you
  already made your character remotely, stop the server and run
  `ironmud-admin user grant-admin <name>`. In Docker:
  `docker stop ironmud`, then
  `docker run --rm -v ironmud-data:/opt/ironmud/data --entrypoint ironmud-admin ghcr.io/zombiecraig/ironmud-public:latest user grant-admin <name>`,
  then `docker start ironmud`. See [Installation: First User](installation.md#first-user).

## Troubleshooting

| Problem | Look at |
|---|---|
| Setup didn't finish | `/var/log/ironmud-bootstrap.log` (cloud-init: also `/var/log/cloud-init-output.log`) |
| Server won't start or keeps restarting | `journalctl -u ironmud -e` |
| Can't connect from outside | Provider firewall *and* `sudo ufw status`: both must allow 4000 (and 4443) |
| TLS never came up | DNS must point at the server; then `sudo bash /root/ironmud-bootstrap.sh` |
| Some commands missing | `ironmud_script_load_errors` in `/metrics` and `journalctl -u ironmud \| grep "Failed to compile"` |

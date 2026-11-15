#!/usr/bin/env bash
#
# IronMUD server bootstrap: turn a fresh Ubuntu/Debian VPS into a running,
# health-checked IronMUD server in one command. Used by cloud-init
# (deploy/cloud-init/ironmud.yaml) and runnable by hand:
#
#   curl -fsSL https://github.com/zombieCraig/IronMUD-public/releases/latest/download/bootstrap.sh | sudo bash
#
# Configure with environment variables, or put KEY=value lines in
# /etc/ironmud/bootstrap.env (read first; variables already set in the
# environment win):
#
#   IRONMUD_VERSION        latest | vX.Y.Z                    (default: latest)
#   IRONMUD_RELEASE_REPO   GitHub owner/name for releases     (default: zombieCraig/IronMUD-public)
#   IRONMUD_RELEASE_BASE_URL  download from this URL instead of GitHub (a mirror holding
#                          install.sh, the tarballs and SHA256SUMS)
#   IRONMUD_TLS_DOMAIN     e.g. mud.example.com; enables TLS on 4443 via HAProxy + Let's Encrypt.
#                          The domain's DNS must already point at this server.
#   IRONMUD_TLS_EMAIL      Let's Encrypt account email (optional; expiry notices)
#   IRONMUD_TLS_ONLY       1 = close plaintext port 4000 to the outside (TLS only)
#   IRONMUD_TLS_DNS_WAIT   seconds to wait for the domain to resolve before giving up on TLS
#                          (default 0; Terraform sets it so DNS can be added after apply)
#   IRONMUD_FIREWALL       1 = configure ufw (default), 0 = leave the firewall alone
#   IRONMUD_SWAP_MB        swap file size when RAM < 2 GB and no swap exists (default 1024, 0 = never)
#
# Safe to re-run: upgrades the release, refreshes the proxy config, and
# keeps an existing certificate. Secrets (SMTP, Discord, API keys) do NOT
# belong here -- set them after boot with ironmud-admin / a systemd override.

set -euo pipefail

ENV_FILE="/etc/ironmud/bootstrap.env"
LOG_FILE="/var/log/ironmud-bootstrap.log"
GAME_PORT=4000
TLS_PORT=4443
PROXY_PORT=4010   # loopback-only PROXY-protocol port HAProxy forwards to
DROPIN_DIR="/etc/systemd/system/ironmud.service.d"
HAPROXY_PEM="/etc/haproxy/certs/ironmud.pem"

log() { echo "[ironmud-bootstrap] $*" >&2; }
warn() { echo "[ironmud-bootstrap] WARNING: $*" >&2; }
die() { echo "[ironmud-bootstrap] ERROR: $*" >&2; exit 1; }

# Load KEY=value lines without letting the file override the environment.
load_env_file() {
    [[ -f "$ENV_FILE" ]] || return 0
    local line key value
    while IFS= read -r line || [[ -n "$line" ]]; do
        [[ "$line" =~ ^[[:space:]]*(#|$) ]] && continue
        [[ "$line" =~ ^[[:space:]]*([A-Z_][A-Z0-9_]*)=(.*)$ ]] || continue
        key="${BASH_REMATCH[1]}"
        value="${BASH_REMATCH[2]}"
        value="${value%\"}"; value="${value#\"}"
        value="${value%\'}"; value="${value#\'}"
        [[ "$key" == IRONMUD_* ]] || continue
        if [[ -z "${!key+x}" ]]; then
            export "$key=$value"
        fi
    done < "$ENV_FILE"
}

apply_defaults() {
    IRONMUD_VERSION="${IRONMUD_VERSION:-latest}"
    IRONMUD_RELEASE_REPO="${IRONMUD_RELEASE_REPO:-zombieCraig/IronMUD-public}"
    IRONMUD_TLS_DOMAIN="${IRONMUD_TLS_DOMAIN:-}"
    IRONMUD_TLS_EMAIL="${IRONMUD_TLS_EMAIL:-}"
    IRONMUD_TLS_ONLY="${IRONMUD_TLS_ONLY:-0}"
    IRONMUD_TLS_DNS_WAIT="${IRONMUD_TLS_DNS_WAIT:-0}"
    [[ "$IRONMUD_TLS_DNS_WAIT" =~ ^[0-9]+$ ]] || IRONMUD_TLS_DNS_WAIT=0
    IRONMUD_FIREWALL="${IRONMUD_FIREWALL:-1}"
    IRONMUD_SWAP_MB="${IRONMUD_SWAP_MB:-1024}"
    export IRONMUD_RELEASE_REPO
    TLS_ACTIVE=0
}

release_base() {
    if [[ -n "${IRONMUD_RELEASE_BASE_URL:-}" ]]; then
        echo "${IRONMUD_RELEASE_BASE_URL%/}"
    elif [[ "$IRONMUD_VERSION" == "latest" ]]; then
        echo "https://github.com/${IRONMUD_RELEASE_REPO}/releases/latest/download"
    else
        echo "https://github.com/${IRONMUD_RELEASE_REPO}/releases/download/${IRONMUD_VERSION}"
    fi
}

apt_install() {
    DEBIAN_FRONTEND=noninteractive apt-get install -y -q "$@"
}

ensure_swap() {
    [[ "$IRONMUD_SWAP_MB" =~ ^[0-9]+$ ]] || die "IRONMUD_SWAP_MB must be a number"
    [[ "$IRONMUD_SWAP_MB" -gt 0 ]] || return 0
    local mem_kb
    mem_kb=$(awk '/^MemTotal:/ {print $2}' /proc/meminfo)
    if [[ "$mem_kb" -ge 2000000 ]] || [[ -n "$(swapon --noheadings 2>/dev/null)" ]]; then
        return 0
    fi
    if [[ -f /swapfile ]]; then
        swapon /swapfile 2>/dev/null || true
        return 0
    fi
    log "Low memory ($((mem_kb / 1024)) MB, no swap): adding a ${IRONMUD_SWAP_MB} MB swap file"
    if ! fallocate -l "${IRONMUD_SWAP_MB}M" /swapfile 2>/dev/null; then
        dd if=/dev/zero of=/swapfile bs=1M count="$IRONMUD_SWAP_MB" status=none
    fi
    chmod 600 /swapfile
    mkswap /swapfile >/dev/null
    swapon /swapfile
    grep -q '^/swapfile ' /etc/fstab || echo '/swapfile none swap sw 0 0' >> /etc/fstab
}

# Download install.sh from the release and verify it against SHA256SUMS.
fetch_installer() {
    local base work
    base=$(release_base)
    work=$(mktemp -d)
    log "Fetching installer from $base"
    curl -fsSL --retry 3 -o "$work/install.sh" "$base/install.sh" ||
        die "could not download $base/install.sh (does the release exist?)"
    curl -fsSL --retry 3 -o "$work/SHA256SUMS" "$base/SHA256SUMS" ||
        die "could not download $base/SHA256SUMS"
    (cd "$work" && grep ' install\.sh$' SHA256SUMS | sha256sum -c --status -) ||
        die "install.sh failed checksum verification"
    echo "$work/install.sh"
}

release_arch() {
    case "$(uname -m)" in
        x86_64 | amd64) echo "x86_64" ;;
        aarch64 | arm64) echo "aarch64" ;;
        *) die "no prebuilt IronMUD release for $(uname -m)" ;;
    esac
}

install_ironmud() {
    local installer tarball_url
    installer=$(fetch_installer)
    # Hand install.sh the exact tarball URL; it verifies it against the
    # SHA256SUMS next to it. Run from / so install.sh never mistakes the
    # cwd for an unpacked release.
    tarball_url="$(release_base)/ironmud-$(release_arch)-linux.tar.gz"
    (cd / && bash "$installer" --from-release "$tarball_url")
}

# Server settings this bootstrap owns live in a drop-in, so install.sh
# upgrades (which preserve the main unit file) keep them.
write_dropin() {
    mkdir -p "$DROPIN_DIR"
    {
        echo "# Managed by IronMUD bootstrap.sh -- re-run it to change."
        echo "[Service]"
        if [[ "$TLS_ACTIVE" == 1 ]]; then
            echo "Environment=\"IRONMUD_PROXY_PORT=${PROXY_PORT}\""
            echo "Environment=\"IRONMUD_PROXY_BIND=127.0.0.1\""
        fi
        if [[ "$TLS_ACTIVE" == 1 && "$IRONMUD_TLS_ONLY" == 1 ]]; then
            echo "Environment=\"IRONMUD_BIND=127.0.0.1\""
        fi
    } > "$DROPIN_DIR/bootstrap.conf"
    systemctl daemon-reload
}

# --- TLS -----------------------------------------------------------------

domain_resolves() {
    getent ahosts "$IRONMUD_TLS_DOMAIN" >/dev/null 2>&1
}

# Wait up to IRONMUD_TLS_DNS_WAIT seconds for the domain to resolve. The game
# is already running on port 4000 meanwhile (install.sh started it).
wait_for_dns() {
    domain_resolves && return 0
    local waited=0
    [[ "$IRONMUD_TLS_DNS_WAIT" -gt 0 ]] || return 1
    log "Waiting up to ${IRONMUD_TLS_DNS_WAIT}s for $IRONMUD_TLS_DOMAIN to resolve (add its DNS record now)"
    while [[ "$waited" -lt "$IRONMUD_TLS_DNS_WAIT" ]]; do
        sleep 15
        waited=$((waited + 15))
        # Don't let a cached NXDOMAIN hide a record that was just added.
        resolvectl flush-caches >/dev/null 2>&1 || true
        domain_resolves && return 0
    done
    return 1
}

# Optional $1 = config path (the test harness writes elsewhere).
# shellcheck disable=SC2120
write_haproxy_config() {
    local cfg="${1:-/etc/haproxy/haproxy.cfg}"
    if [[ -f "$cfg" ]] && ! grep -q "Managed by IronMUD bootstrap" "$cfg"; then
        cp "$cfg" "$cfg.pre-ironmud"
    fi
    cat > "$cfg" <<EOF
# Managed by IronMUD bootstrap.sh -- re-run it to change.
# TLS on :${TLS_PORT} -> IronMUD's loopback PROXY-protocol port, so the
# server sees each player's real IP (rate limits, site bans, logs).
global
    log /dev/log local0
    maxconn 2000
    user haproxy
    group haproxy
    ssl-default-bind-options ssl-min-ver TLSv1.2

defaults
    mode tcp
    log global
    option tcplog
    timeout connect 5s
    # MUD sessions idle for a long time between commands.
    timeout client 2h
    timeout server 2h

frontend ironmud_tls
    bind :::${TLS_PORT} v4v6 ssl crt ${HAPROXY_PEM}
    default_backend ironmud

backend ironmud
    server game 127.0.0.1:${PROXY_PORT} send-proxy
EOF
}

# HAProxy wants certificate + key in one PEM file.
build_haproxy_pem() {
    local live="/etc/letsencrypt/live/${IRONMUD_TLS_DOMAIN}"
    mkdir -p "$(dirname "$HAPROXY_PEM")"
    (umask 077 && cat "$live/fullchain.pem" "$live/privkey.pem" > "${HAPROXY_PEM}.new")
    mv -f "${HAPROXY_PEM}.new" "$HAPROXY_PEM"
}

# certbot deploy hook: rebuild HAProxy's combined PEM and reload on renewal.
write_renewal_hook() {
    mkdir -p /etc/letsencrypt/renewal-hooks/deploy
    cat > /etc/letsencrypt/renewal-hooks/deploy/ironmud-haproxy.sh <<EOF
#!/bin/sh
# Managed by IronMUD bootstrap.sh
set -e
live="/etc/letsencrypt/live/${IRONMUD_TLS_DOMAIN}"
umask 077
mkdir -p "$(dirname "$HAPROXY_PEM")"
cat "\$live/fullchain.pem" "\$live/privkey.pem" > "${HAPROXY_PEM}.new"
mv -f "${HAPROXY_PEM}.new" "${HAPROXY_PEM}"
systemctl reload-or-restart haproxy
EOF
    chmod 755 /etc/letsencrypt/renewal-hooks/deploy/ironmud-haproxy.sh
}

# TLS was set up on an earlier run but no domain is configured now.
disable_managed_tls() {
    if [[ -f /etc/haproxy/haproxy.cfg ]] && grep -q "Managed by IronMUD bootstrap" /etc/haproxy/haproxy.cfg; then
        log "No IRONMUD_TLS_DOMAIN set: stopping the IronMUD-managed HAProxy"
        systemctl disable --now haproxy >/dev/null 2>&1 || true
        if [[ "$IRONMUD_FIREWALL" == 1 ]] && command -v ufw >/dev/null; then
            ufw delete allow "${TLS_PORT}/tcp" >/dev/null 2>&1 || true
            ufw delete allow 80/tcp >/dev/null 2>&1 || true
        fi
    fi
}

setup_tls() {
    if [[ -z "$IRONMUD_TLS_DOMAIN" ]]; then
        disable_managed_tls
        return 0
    fi
    log "Setting up TLS for $IRONMUD_TLS_DOMAIN on port $TLS_PORT"

    if ! wait_for_dns; then
        warn "$IRONMUD_TLS_DOMAIN does not resolve yet. Skipping TLS; players can use plaintext port $GAME_PORT."
        warn "Point its DNS at this server, then re-run this script to add TLS."
        return 0
    fi

    apt_install haproxy certbot

    if [[ "$IRONMUD_FIREWALL" == 1 ]] && command -v ufw >/dev/null; then
        ufw allow 80/tcp >/dev/null   # Let's Encrypt HTTP-01 challenge (certbot standalone)
    fi

    local email_args=(--register-unsafely-without-email)
    if [[ -n "$IRONMUD_TLS_EMAIL" ]]; then
        email_args=(-m "$IRONMUD_TLS_EMAIL")
    fi
    if ! certbot certonly --standalone --non-interactive --agree-tos --keep-until-expiring \
        "${email_args[@]}" -d "$IRONMUD_TLS_DOMAIN"; then
        warn "Let's Encrypt could not issue a certificate for $IRONMUD_TLS_DOMAIN."
        warn "Check that its DNS points at this server's public IP ($(getent ahosts "$IRONMUD_TLS_DOMAIN" | awk 'NR==1{print $1}')) and port 80 is reachable."
        warn "Skipping TLS; re-run this script once fixed."
        return 0
    fi

    write_renewal_hook
    # Certificate PEM and config must both exist before HAProxy (re)loads.
    build_haproxy_pem
    write_haproxy_config
    haproxy -c -f /etc/haproxy/haproxy.cfg >/dev/null || die "generated HAProxy config is invalid"
    systemctl enable haproxy >/dev/null 2>&1 || true
    systemctl restart haproxy
    TLS_ACTIVE=1
}

# --- Firewall ------------------------------------------------------------

setup_firewall() {
    [[ "$IRONMUD_FIREWALL" == 1 ]] || { log "Leaving the firewall alone (IRONMUD_FIREWALL=0)"; return 0; }
    command -v ufw >/dev/null || apt_install ufw

    # Never lock ourselves out: allow every port sshd listens on.
    local port
    for port in $(sshd -T 2>/dev/null | awk '$1=="port"{print $2}'); do
        ufw allow "${port}/tcp" >/dev/null
    done
    ufw allow OpenSSH >/dev/null 2>&1 || ufw allow 22/tcp >/dev/null

    if [[ "$TLS_ACTIVE" == 1 ]]; then
        ufw allow "${TLS_PORT}/tcp" >/dev/null
        ufw allow 80/tcp >/dev/null
    fi
    if [[ "$TLS_ACTIVE" == 1 && "$IRONMUD_TLS_ONLY" == 1 ]]; then
        ufw delete allow "${GAME_PORT}/tcp" >/dev/null 2>&1 || true
    else
        ufw allow "${GAME_PORT}/tcp" >/dev/null
    fi
    ufw default deny incoming >/dev/null
    ufw --force enable >/dev/null
    log "Firewall: $(ufw status | grep -c ALLOW) allow rules active (ops/API ports stay local)"
}

wait_healthy() {
    local _
    for _ in $(seq 1 90); do
        if ironmud-admin health -q 2>/dev/null; then
            return 0
        fi
        sleep 1
    done
    return 1
}

print_summary() {
    local host="${IRONMUD_TLS_DOMAIN:-YOUR_SERVER_IP}"
    echo ""
    echo "=================================================================="
    echo " IronMUD is running."
    if [[ "$TLS_ACTIVE" == 1 ]]; then
        echo "   Encrypted (TLS):  ${host}:${TLS_PORT}   (Mudlet: enable 'Secure')"
    fi
    if [[ "$TLS_ACTIVE" == 1 && "$IRONMUD_TLS_ONLY" == 1 ]]; then
        echo "   Plaintext port ${GAME_PORT} is closed (IRONMUD_TLS_ONLY=1)."
    else
        echo "   Plaintext telnet: ${host}:${GAME_PORT}"
    fi
    if [[ "$TLS_ACTIVE" != 1 ]]; then
        echo ""
        echo " WARNING: connections are NOT encrypted; passwords cross the"
        echo " internet in plaintext. To add TLS, point a domain at this server"
        echo " and re-run with IRONMUD_TLS_DOMAIN=mud.example.com."
    fi
    echo ""
    echo " First admin: create your character FROM THIS SERVER so it gets admin"
    echo " rights:  telnet localhost ${GAME_PORT}   (or: nc localhost ${GAME_PORT})"
    echo ""
    echo " Health:   ironmud-admin health"
    echo " Logs:     journalctl -u ironmud -f"
    echo " Upgrade:  sudo /opt/ironmud/install.sh --from-release"
    echo " Note: your cloud provider's firewall must also allow these ports."
    echo "=================================================================="
}

main() {
    [[ $EUID -eq 0 ]] || die "run as root (sudo)"
    [[ -f /etc/debian_version ]] || warn "built for Ubuntu/Debian; continuing anyway"

    load_env_file
    apply_defaults
    log "Release ${IRONMUD_VERSION} from ${IRONMUD_RELEASE_REPO}"

    apt-get update -q
    apt_install curl ca-certificates

    ensure_swap
    install_ironmud
    setup_tls
    write_dropin
    setup_firewall

    systemctl restart ironmud
    if wait_healthy; then
        log "Server is healthy"
    else
        warn "Server did not report healthy within 90s. Check: journalctl -u ironmud -e"
    fi
    print_summary
}

# Sourcing with IRONMUD_BOOTSTRAP_LIB=1 loads the functions without running
# (used by the test harness).
if [[ "${IRONMUD_BOOTSTRAP_LIB:-0}" != 1 ]]; then
    main "$@" 2>&1 | tee -a "$LOG_FILE"
    exit "${PIPESTATUS[0]}"
fi

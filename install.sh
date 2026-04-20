#!/bin/bash
#
# IronMUD Server Installation Script
# Installs IronMUD as a systemd service on Ubuntu/Debian
#

set -e

# Configuration
INSTALL_DIR="/opt/ironmud"
SERVICE_USER="ironmud"
SERVICE_GROUP="ironmud"
DEFAULT_PORT="4000"
BACKUP_DIR=""  # Set during upgrade

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Parse arguments
SCRIPTS_ONLY=false
WARN_SECONDS=0
while [[ $# -gt 0 ]]; do
    case $1 in
        --scripts-only)
            SCRIPTS_ONLY=true
            shift
            ;;
        --warn-seconds)
            if [[ -z "$2" || ! "$2" =~ ^[0-9]+$ ]]; then
                log_error "--warn-seconds requires a non-negative integer argument"
                exit 1
            fi
            WARN_SECONDS="$2"
            shift 2
            ;;
        --warn-seconds=*)
            WARN_SECONDS="${1#*=}"
            if [[ ! "$WARN_SECONDS" =~ ^[0-9]+$ ]]; then
                log_error "--warn-seconds requires a non-negative integer argument"
                exit 1
            fi
            shift
            ;;
        *)
            log_error "Unknown option: $1"
            echo "Usage: $0 [--scripts-only] [--warn-seconds N]"
            exit 1
            ;;
    esac
done

# Check if running as root
check_root() {
    if [[ $EUID -ne 0 ]]; then
        log_error "This script must be run as root (use sudo)"
        exit 1
    fi
}

# Check if this is an upgrade (existing installation detected)
is_upgrade() {
    [[ -d "$INSTALL_DIR" ]] && [[ -f "$INSTALL_DIR/bin/ironmud" ]]
}

# Warn online players and wait before disruptive actions
warn_players() {
    local seconds="$1"
    if [[ "$seconds" -le 0 ]]; then
        return
    fi

    if ! systemctl is-active --quiet ironmud 2>/dev/null; then
        log_info "Service not running; skipping player warning."
        return
    fi

    if [[ ! -x /usr/local/bin/ironmud-admin ]]; then
        log_warn "ironmud-admin not installed; cannot warn players. Skipping warning."
        return
    fi

    local msg="*** SERVER NOTICE: An update is starting. The server will restart in approximately ${seconds} seconds. ***"
    log_info "Warning online players (${seconds}s until restart)..."
    if ! /usr/local/bin/ironmud-admin broadcast "$msg"; then
        log_warn "Broadcast failed (is the control socket available?). Continuing without warning."
        # Still honor the delay so operators get the pause they asked for.
    fi
    sleep "$seconds"
}

# Stop service if running
stop_service_if_running() {
    if systemctl is-active --quiet ironmud 2>/dev/null; then
        log_info "Stopping IronMUD service..."
        systemctl stop ironmud
        # Wait for graceful shutdown
        sleep 2
    fi
}

# Backup database before upgrade
backup_data() {
    if [[ -d "$INSTALL_DIR/data" ]]; then
        BACKUP_DIR="$INSTALL_DIR/data.backup-$(date +%Y%m%d-%H%M%S)"
        log_info "Backing up database to $BACKUP_DIR..."
        cp -r "$INSTALL_DIR/data" "$BACKUP_DIR"
    fi
}

# Smart copy scripts - backup modified files before overwriting
copy_scripts_smart() {
    log_info "Updating scripts (backing up modified files)..."

    # Commands
    for src_file in scripts/commands/*.rhai; do
        filename=$(basename "$src_file")
        dest_file="$INSTALL_DIR/scripts/commands/$filename"
        if [[ -f "$dest_file" ]]; then
            if ! diff -q "$src_file" "$dest_file" > /dev/null 2>&1; then
                log_info "  Backing up modified: commands/$filename"
                cp "$dest_file" "${dest_file}.backup"
            fi
        fi
        cp "$src_file" "$dest_file"
    done

    # Triggers
    for src_file in scripts/triggers/*.rhai; do
        filename=$(basename "$src_file")
        dest_file="$INSTALL_DIR/scripts/triggers/$filename"
        if [[ -f "$dest_file" ]]; then
            if ! diff -q "$src_file" "$dest_file" > /dev/null 2>&1; then
                log_info "  Backing up modified: triggers/$filename"
                cp "$dest_file" "${dest_file}.backup"
            fi
        fi
        cp "$src_file" "$dest_file"
    done

    # Shared libraries (scripts/lib/)
    mkdir -p "$INSTALL_DIR/scripts/lib"
    for src_file in scripts/lib/*.rhai; do
        filename=$(basename "$src_file")
        dest_file="$INSTALL_DIR/scripts/lib/$filename"
        if [[ -f "$dest_file" ]]; then
            if ! diff -q "$src_file" "$dest_file" > /dev/null 2>&1; then
                log_info "  Backing up modified: lib/$filename"
                cp "$dest_file" "${dest_file}.backup"
            fi
        fi
        cp "$src_file" "$dest_file"
    done

    # commands.json
    if [[ -f "$INSTALL_DIR/scripts/commands.json" ]]; then
        if ! diff -q scripts/commands.json "$INSTALL_DIR/scripts/commands.json" > /dev/null 2>&1; then
            log_info "  Backing up modified: commands.json"
            cp "$INSTALL_DIR/scripts/commands.json" "$INSTALL_DIR/scripts/commands.json.backup"
        fi
    fi
    cp scripts/commands.json "$INSTALL_DIR/scripts/"

    # Data files (traits, classes, races) - always overwrite with new definitions
    mkdir -p "$INSTALL_DIR/scripts/data"
    cp -r scripts/data/* "$INSTALL_DIR/scripts/data/"
}

# Scripts-only update (no build, no restart)
scripts_only_update() {
    log_info "Scripts-only update mode"

    if [[ ! -d "$INSTALL_DIR" ]] || [[ ! -f "$INSTALL_DIR/bin/ironmud" ]]; then
        log_error "No existing installation found at $INSTALL_DIR"
        log_error "Run a full install first before using --scripts-only"
        exit 1
    fi

    copy_scripts_smart

    # Set permissions on scripts only
    chown -R "$SERVICE_USER:$SERVICE_GROUP" "$INSTALL_DIR/scripts"

    echo ""
    echo "=================================================="
    echo -e "${GREEN}Scripts updated!${NC}"
    echo "=================================================="
    echo ""
    echo "The server will automatically detect and reload the scripts."
    echo "No restart required."
    echo ""
}

# Print upgrade completion message
print_upgrade_completion() {
    echo ""
    echo "=================================================="
    echo -e "${GREEN}IronMUD upgrade complete!${NC}"
    echo "=================================================="
    echo ""
    if [[ -n "$BACKUP_DIR" ]]; then
        echo "Database backup: $BACKUP_DIR"
    fi
    echo ""
    echo "Check logs: sudo journalctl -u ironmud -f"
    echo ""
}

# Detect OS
check_os() {
    if [[ -f /etc/debian_version ]]; then
        log_info "Detected Debian/Ubuntu system"
    else
        log_warn "This script is designed for Debian/Ubuntu. Proceed with caution."
    fi
}

# Install build dependencies
install_dependencies() {
    log_info "Installing build dependencies..."
    apt-get update
    apt-get install -y build-essential pkg-config libssl-dev curl
}

# Install Rust toolchain
install_rust() {
    if command -v rustc &> /dev/null && command -v cargo &> /dev/null; then
        log_info "Rust is already installed: $(rustc --version)"
        return
    fi

    log_info "Installing Rust toolchain..."

    # Install rustup for the current user (will be used for building)
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

    # Source cargo env for this session
    source "$HOME/.cargo/env"

    log_info "Rust installed: $(rustc --version)"
}

# Create system user
create_user() {
    if id "$SERVICE_USER" &>/dev/null; then
        log_info "User '$SERVICE_USER' already exists"
        return
    fi

    log_info "Creating system user '$SERVICE_USER'..."
    useradd --system --no-create-home --shell /usr/sbin/nologin "$SERVICE_USER"
}

# Build the project
build_project() {
    log_info "Building IronMUD (release mode)..."

    # Ensure cargo is in PATH
    if [[ -f "$HOME/.cargo/env" ]]; then
        source "$HOME/.cargo/env"
    fi

    cargo build --release

    if [[ ! -f "target/release/ironmud" ]]; then
        log_error "Build failed: target/release/ironmud not found"
        exit 1
    fi

    log_info "Build successful"
}

# Create directory structure
create_directories() {
    log_info "Creating installation directories..."

    mkdir -p "$INSTALL_DIR/bin"
    mkdir -p "$INSTALL_DIR/scripts/commands"
    mkdir -p "$INSTALL_DIR/scripts/triggers"
    mkdir -p "$INSTALL_DIR/scripts/lib"
    mkdir -p "$INSTALL_DIR/scripts/data"
    mkdir -p "$INSTALL_DIR/assets"
    mkdir -p "$INSTALL_DIR/data"
}

# Copy files
copy_files() {
    log_info "Copying files to $INSTALL_DIR..."

    # Binaries
    cp target/release/ironmud "$INSTALL_DIR/bin/"
    cp target/release/ironmud-admin "$INSTALL_DIR/bin/"

    # Scripts
    cp -r scripts/commands/* "$INSTALL_DIR/scripts/commands/"
    cp -r scripts/triggers/* "$INSTALL_DIR/scripts/triggers/"
    cp -r scripts/lib/* "$INSTALL_DIR/scripts/lib/"
    cp scripts/commands.json "$INSTALL_DIR/scripts/"

    # Game data (traits, classes, races)
    cp -r scripts/data/* "$INSTALL_DIR/scripts/data/"

    # Assets
    if [[ -f "assets/banner.txt" ]]; then
        cp assets/banner.txt "$INSTALL_DIR/assets/"
    fi
    if [[ -f "assets/matrix_avatar.png" ]]; then
        cp assets/matrix_avatar.png "$INSTALL_DIR/assets/"
    fi
}

# Set permissions
set_permissions() {
    log_info "Setting ownership and permissions..."

    chown -R "$SERVICE_USER:$SERVICE_GROUP" "$INSTALL_DIR"
    chmod 755 "$INSTALL_DIR/bin/ironmud"
    chmod 755 "$INSTALL_DIR/bin/ironmud-admin"
}

# Install systemd service
install_service() {
    log_info "Installing systemd service..."

    cat > /etc/systemd/system/ironmud.service << 'EOF'
[Unit]
Description=IronMUD Game Server
After=network.target

[Service]
Type=simple
User=ironmud
Group=ironmud
WorkingDirectory=/opt/ironmud
Environment="IRONMUD_DATABASE=/opt/ironmud/data/ironmud.db"
ExecStart=/opt/ironmud/bin/ironmud --port 4000 --database /opt/ironmud/data/ironmud.db
Restart=on-failure
RestartSec=5
TimeoutStopSec=30

# Matrix integration (optional)
# Uncomment and configure to enable Matrix room announcements
#Environment="MATRIX_HOMESERVER=http://matrix.example.com:8008"
#Environment="MATRIX_USER=ironmud_bot"
#Environment="MATRIX_PASSWORD=your_password_here"
#Environment="MATRIX_ROOM=!roomid:example.com"

# Security hardening
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/opt/ironmud/data
PrivateTmp=yes

[Install]
WantedBy=multi-user.target
EOF

    systemctl daemon-reload
    log_info "Service installed"
}

# Install admin wrapper script
install_admin_wrapper() {
    log_info "Installing ironmud-admin wrapper to /usr/local/bin..."
    cat > /usr/local/bin/ironmud-admin << 'WRAPPER'
#!/bin/bash
# IronMUD admin tool wrapper - sets default database path
export IRONMUD_DATABASE="${IRONMUD_DATABASE:-/opt/ironmud/data/ironmud.db}"
exec /opt/ironmud/bin/ironmud-admin "$@"
WRAPPER
    chmod 755 /usr/local/bin/ironmud-admin
}

# Enable and start service
start_service() {
    log_info "Enabling and starting IronMUD service..."

    systemctl enable ironmud
    systemctl start ironmud

    sleep 2

    if systemctl is-active --quiet ironmud; then
        log_info "IronMUD service is running"
    else
        log_warn "Service may not have started correctly. Check: systemctl status ironmud"
    fi
}

# Print completion message
print_completion() {
    echo ""
    echo "=================================================="
    echo -e "${GREEN}IronMUD installation complete!${NC}"
    echo "=================================================="
    echo ""
    echo "Installation directory: $INSTALL_DIR"
    echo "Database location:      $INSTALL_DIR/data/ironmud.db"
    echo "Service user:           $SERVICE_USER"
    echo ""
    echo "Useful commands:"
    echo "  sudo systemctl status ironmud    # Check service status"
    echo "  sudo systemctl stop ironmud      # Stop the server"
    echo "  sudo systemctl start ironmud     # Start the server"
    echo "  sudo systemctl restart ironmud   # Restart the server"
    echo "  sudo journalctl -u ironmud -f    # View live logs"
    echo ""
    echo "Connect to the server:"
    echo "  telnet localhost $DEFAULT_PORT"
    echo ""
    echo "Admin tool:"
    echo "  sudo ironmud-admin --help"
    echo ""
    echo "Matrix integration (optional):"
    echo "  Edit /etc/systemd/system/ironmud.service to configure Matrix room"
    echo "  announcements. Uncomment and set the MATRIX_* environment variables."
    echo ""
    echo "To change port, database path, or Matrix settings, edit:"
    echo "  /etc/systemd/system/ironmud.service"
    echo "Then run: sudo systemctl daemon-reload && sudo systemctl restart ironmud"
    echo ""
}

# Main installation flow
main() {
    check_root
    check_os

    if [[ "$SCRIPTS_ONLY" == true ]]; then
        scripts_only_update
        return
    fi

    # Configure git hooks if running from a git checkout
    if [[ -d ".git" ]] && [[ -d ".githooks" ]]; then
        git config core.hooksPath .githooks
        log_info "Configured git hooks path to .githooks"
    fi

    if is_upgrade; then
        echo "=================================================="
        echo "IronMUD Server Upgrade"
        echo "=================================================="
        echo ""
        log_info "Existing installation detected - upgrading..."

        # Optional: warn online players before we start consuming the box with a build
        warn_players "$WARN_SECONDS"

        # Build first (while server is still running) to minimize downtime
        build_project

        stop_service_if_running
        backup_data

        # Update binaries
        log_info "Installing new binaries..."
        cp target/release/ironmud "$INSTALL_DIR/bin/"
        cp target/release/ironmud-admin "$INSTALL_DIR/bin/"

        # Smart copy scripts (backup modified ones)
        copy_scripts_smart

        # Copy assets (create dir if needed)
        mkdir -p "$INSTALL_DIR/assets"
        if [[ -f "assets/banner.txt" ]]; then
            cp assets/banner.txt "$INSTALL_DIR/assets/"
        fi
        if [[ -f "assets/matrix_avatar.png" ]]; then
            cp assets/matrix_avatar.png "$INSTALL_DIR/assets/"
        fi

        # Skip service file (preserve customizations)
        log_info "Preserving existing systemd service configuration"

        set_permissions
        install_admin_wrapper
        start_service
        print_upgrade_completion
    else
        echo "=================================================="
        echo "IronMUD Server Installation"
        echo "=================================================="
        echo ""

        install_dependencies
        install_rust
        create_user
        build_project
        create_directories
        copy_files
        set_permissions
        install_service
        install_admin_wrapper
        start_service
        print_completion
    fi
}

# Run main
main "$@"

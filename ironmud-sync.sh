#!/bin/bash
#
# IronMUD Backup/Restore Script
# Syncs database and custom triggers between servers
#
# Prerequisites: Both servers must have IronMUD installed via install.sh
#
# Usage:
#   sudo ./ironmud-sync.sh backup              # Create backup tarball
#   sudo ./ironmud-sync.sh restore <file>      # Restore from tarball
#   sudo ./ironmud-sync.sh status              # Show status and backups
#

set -e

# Configuration
INSTALL_DIR="/opt/ironmud"
BACKUP_DIR="$INSTALL_DIR/backups"
SERVICE_USER="ironmud"
SERVICE_GROUP="ironmud"
SERVICE_NAME="ironmud"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
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

log_step() {
    echo -e "${CYAN}[STEP]${NC} $1"
}

# Check if running as root
check_root() {
    if [[ $EUID -ne 0 ]]; then
        log_error "This script must be run as root (use sudo)"
        exit 1
    fi
}

# Check if IronMUD is installed
check_installation() {
    if [[ ! -d "$INSTALL_DIR" ]]; then
        log_error "IronMUD not installed at $INSTALL_DIR"
        log_error "Run install.sh first"
        exit 1
    fi
    if [[ ! -f "$INSTALL_DIR/bin/ironmud" ]]; then
        log_error "IronMUD binary not found at $INSTALL_DIR/bin/ironmud"
        log_error "Run install.sh first"
        exit 1
    fi
}

# Check if service is running
is_service_running() {
    systemctl is-active --quiet "$SERVICE_NAME" 2>/dev/null
}

# Stop service gracefully
stop_service() {
    if is_service_running; then
        log_step "Stopping IronMUD service (allowing graceful shutdown)..."
        systemctl stop "$SERVICE_NAME"

        # Wait for process to fully terminate
        local count=0
        while is_service_running && [[ $count -lt 10 ]]; do
            sleep 1
            ((count++))
        done

        if is_service_running; then
            log_warn "Service taking long to stop, waiting more..."
            sleep 5
        fi

        log_info "Service stopped"
    else
        log_info "Service was not running"
    fi
}

# Start service
start_service() {
    log_step "Starting IronMUD service..."
    systemctl start "$SERVICE_NAME"
    sleep 2

    if is_service_running; then
        log_info "Service started successfully"
    else
        log_error "Service failed to start. Check: journalctl -u $SERVICE_NAME"
        exit 1
    fi
}

# Create backup
do_backup() {
    local no_restart=false

    # Parse options
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --no-restart)
                no_restart=true
                shift
                ;;
            *)
                log_error "Unknown option: $1"
                exit 1
                ;;
        esac
    done

    check_installation

    echo ""
    echo "=================================================="
    echo "IronMUD Backup"
    echo "=================================================="
    echo ""

    # Create backup directory
    mkdir -p "$BACKUP_DIR"

    # Generate timestamp and filename
    local timestamp=$(date +%Y%m%d-%H%M%S)
    local backup_file="$BACKUP_DIR/ironmud-backup-$timestamp.tar.gz"

    # Check what we're backing up
    if [[ ! -d "$INSTALL_DIR/data/ironmud.db" ]]; then
        log_warn "Database directory not found at $INSTALL_DIR/data/ironmud.db"
        log_warn "Creating backup anyway (may be empty)"
    fi

    # Remember if service was running
    local was_running=false
    if is_service_running; then
        was_running=true
    fi

    # Stop service for consistent backup
    stop_service

    # Create tarball
    log_step "Creating backup archive..."
    cd "$INSTALL_DIR"

    # Build list of items to backup
    local items_to_backup=""

    if [[ -d "data/ironmud.db" ]]; then
        items_to_backup="data/ironmud.db"
    fi

    if [[ -d "scripts/triggers" ]]; then
        items_to_backup="$items_to_backup scripts/triggers"
    fi

    if [[ -z "$items_to_backup" ]]; then
        log_error "Nothing to backup!"
        exit 1
    fi

    tar -czvf "$backup_file" $items_to_backup

    # Set ownership so ironmud user can access backups
    chown "$SERVICE_USER:$SERVICE_GROUP" "$backup_file"

    log_info "Backup created: $backup_file"

    # Show backup size
    local size=$(du -h "$backup_file" | cut -f1)
    log_info "Backup size: $size"

    # Restart service if it was running and --no-restart not specified
    if [[ "$was_running" == true ]] && [[ "$no_restart" == false ]]; then
        start_service
    elif [[ "$no_restart" == true ]]; then
        log_info "Service left stopped (--no-restart)"
    fi

    echo ""
    echo "=================================================="
    echo -e "${GREEN}Backup complete!${NC}"
    echo "=================================================="
    echo ""
    echo "Backup file: $backup_file"
    echo ""
    echo "To transfer to another server:"
    echo "  scp $backup_file user@other-server:/tmp/"
    echo ""
}

# Restore from backup
do_restore() {
    local backup_file="$1"
    local no_start=false

    # Parse remaining options
    shift || true
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --no-start)
                no_start=true
                shift
                ;;
            *)
                log_error "Unknown option: $1"
                exit 1
                ;;
        esac
    done

    if [[ -z "$backup_file" ]]; then
        log_error "Usage: $0 restore <backup-file.tar.gz>"
        exit 1
    fi

    if [[ ! -f "$backup_file" ]]; then
        log_error "Backup file not found: $backup_file"
        exit 1
    fi

    check_installation

    echo ""
    echo "=================================================="
    echo "IronMUD Restore"
    echo "=================================================="
    echo ""

    # Verify tarball integrity
    log_step "Verifying backup archive..."
    if ! tar -tzf "$backup_file" > /dev/null 2>&1; then
        log_error "Backup file is corrupted or invalid"
        exit 1
    fi
    log_info "Archive verified"

    # Show what will be restored
    log_info "Contents to restore:"
    tar -tzf "$backup_file" | head -20
    local file_count=$(tar -tzf "$backup_file" | wc -l)
    if [[ $file_count -gt 20 ]]; then
        echo "  ... and $((file_count - 20)) more files"
    fi
    echo ""

    # Stop service
    stop_service

    # Backup existing data before restore
    local timestamp=$(date +%Y%m%d-%H%M%S)

    if [[ -d "$INSTALL_DIR/data/ironmud.db" ]]; then
        local pre_restore="$INSTALL_DIR/data/ironmud.db.pre-restore-$timestamp"
        log_step "Backing up existing database to $pre_restore..."
        mv "$INSTALL_DIR/data/ironmud.db" "$pre_restore"
        log_info "Existing database preserved"
    fi

    if [[ -d "$INSTALL_DIR/scripts/triggers" ]]; then
        local pre_restore_triggers="$INSTALL_DIR/scripts/triggers.pre-restore-$timestamp"
        log_step "Backing up existing triggers to $pre_restore_triggers..."
        cp -r "$INSTALL_DIR/scripts/triggers" "$pre_restore_triggers"
        log_info "Existing triggers preserved"
    fi

    # Extract backup
    log_step "Extracting backup..."
    cd "$INSTALL_DIR"
    tar -xzvf "$backup_file"

    # Fix permissions
    log_step "Setting permissions..."
    chown -R "$SERVICE_USER:$SERVICE_GROUP" "$INSTALL_DIR/data"
    if [[ -d "$INSTALL_DIR/scripts/triggers" ]]; then
        chown -R "$SERVICE_USER:$SERVICE_GROUP" "$INSTALL_DIR/scripts/triggers"
    fi

    log_info "Permissions set to $SERVICE_USER:$SERVICE_GROUP"

    # Start service unless --no-start
    if [[ "$no_start" == false ]]; then
        start_service
    else
        log_info "Service left stopped (--no-start)"
    fi

    echo ""
    echo "=================================================="
    echo -e "${GREEN}Restore complete!${NC}"
    echo "=================================================="
    echo ""
    echo "Restored from: $backup_file"
    if [[ -n "${pre_restore:-}" ]]; then
        echo "Previous database saved to: $pre_restore"
    fi
    echo ""
    echo "Verify with: sudo systemctl status $SERVICE_NAME"
    echo ""
}

# Show status
do_status() {
    check_installation

    echo ""
    echo "=================================================="
    echo "IronMUD Sync Status"
    echo "=================================================="
    echo ""

    # Service status
    echo -e "${CYAN}Service Status:${NC}"
    if is_service_running; then
        echo -e "  State: ${GREEN}running${NC}"
    else
        echo -e "  State: ${YELLOW}stopped${NC}"
    fi
    echo ""

    # Database info
    echo -e "${CYAN}Database:${NC}"
    if [[ -d "$INSTALL_DIR/data/ironmud.db" ]]; then
        local db_size=$(du -sh "$INSTALL_DIR/data/ironmud.db" 2>/dev/null | cut -f1)
        echo "  Location: $INSTALL_DIR/data/ironmud.db"
        echo "  Size: $db_size"
    else
        echo -e "  ${YELLOW}Not found${NC}"
    fi
    echo ""

    # Triggers info
    echo -e "${CYAN}Custom Triggers:${NC}"
    if [[ -d "$INSTALL_DIR/scripts/triggers" ]]; then
        local trigger_count=$(find "$INSTALL_DIR/scripts/triggers" -name "*.rhai" 2>/dev/null | wc -l)
        echo "  Location: $INSTALL_DIR/scripts/triggers"
        echo "  Count: $trigger_count scripts"
    else
        echo -e "  ${YELLOW}Not found${NC}"
    fi
    echo ""

    # Backups info
    echo -e "${CYAN}Local Backups:${NC}"
    if [[ -d "$BACKUP_DIR" ]]; then
        local backup_count=$(find "$BACKUP_DIR" -name "ironmud-backup-*.tar.gz" 2>/dev/null | wc -l)
        echo "  Location: $BACKUP_DIR"
        echo "  Count: $backup_count backups"

        if [[ $backup_count -gt 0 ]]; then
            echo ""
            echo "  Recent backups:"
            find "$BACKUP_DIR" -name "ironmud-backup-*.tar.gz" -printf "    %T+ %p\n" 2>/dev/null | sort -r | head -5 | while read -r line; do
                local file=$(echo "$line" | awk '{print $2}')
                local size=$(du -h "$file" 2>/dev/null | cut -f1)
                local name=$(basename "$file")
                echo "    $name ($size)"
            done
        fi
    else
        echo "  No backups yet"
    fi
    echo ""

    # Pre-restore backups
    local pre_restore_count=$(find "$INSTALL_DIR/data" -maxdepth 1 -name "ironmud.db.pre-restore-*" 2>/dev/null | wc -l)
    if [[ $pre_restore_count -gt 0 ]]; then
        echo -e "${CYAN}Pre-Restore Backups:${NC}"
        echo "  Count: $pre_restore_count"
        find "$INSTALL_DIR/data" -maxdepth 1 -name "ironmud.db.pre-restore-*" -printf "    %f\n" 2>/dev/null | head -5
        echo ""
    fi
}

# Print usage
print_usage() {
    echo "IronMUD Backup/Restore Script"
    echo ""
    echo "Usage: $0 <command> [options]"
    echo ""
    echo "Commands:"
    echo "  backup [--no-restart]     Create backup of database and triggers"
    echo "  restore <file> [--no-start]  Restore from backup file"
    echo "  status                    Show current status and backups"
    echo ""
    echo "Examples:"
    echo "  sudo $0 backup"
    echo "  sudo $0 backup --no-restart"
    echo "  sudo $0 restore /tmp/ironmud-backup-20260114-153000.tar.gz"
    echo "  sudo $0 status"
    echo ""
    echo "Workflow:"
    echo "  1. On source server:  sudo $0 backup"
    echo "  2. Transfer:          scp /opt/ironmud/backups/ironmud-backup-*.tar.gz user@dest:/tmp/"
    echo "  3. On dest server:    sudo $0 restore /tmp/ironmud-backup-*.tar.gz"
    echo ""
}

# Main
main() {
    check_root

    local command="${1:-}"
    shift || true

    case "$command" in
        backup)
            do_backup "$@"
            ;;
        restore)
            do_restore "$@"
            ;;
        status)
            do_status
            ;;
        -h|--help|help)
            print_usage
            ;;
        "")
            print_usage
            exit 1
            ;;
        *)
            log_error "Unknown command: $command"
            print_usage
            exit 1
            ;;
    esac
}

main "$@"

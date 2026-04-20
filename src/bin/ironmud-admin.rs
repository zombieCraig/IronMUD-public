//! IronMUD Admin Utility
//!
//! Command-line tool for managing IronMUD users and settings without
//! requiring the server to be running.

#![recursion_limit = "512"]

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use ironmud::control::{ControlCommand, ControlResponse, default_socket_path};
use ironmud::{ApiKey, ApiPermissions, db::Db};
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "ironmud-admin")]
#[command(about = "IronMUD administration utility")]
struct Cli {
    /// Database path (default: ironmud.db)
    #[arg(short, long, default_value = "ironmud.db", env = "IRONMUD_DATABASE")]
    database: String,

    /// Control socket path (defaults to <database-dir>/control.sock)
    #[arg(long, env = "IRONMUD_CONTROL_SOCKET")]
    control_socket: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// User management commands
    User {
        #[command(subcommand)]
        action: UserAction,
    },
    /// Settings management commands
    Settings {
        #[command(subcommand)]
        action: SettingsAction,
    },
    /// API key management commands
    ApiKey {
        #[command(subcommand)]
        action: ApiKeyAction,
    },
    /// World data management commands
    World {
        #[command(subcommand)]
        action: WorldAction,
    },
    /// Broadcast a message to all logged-in players (requires running server)
    Broadcast {
        /// Message to send
        message: String,
    },
}

#[derive(Subcommand)]
enum WorldAction {
    /// Show world statistics (entity counts)
    Info,
    /// Clear all world data (areas, rooms, items, mobiles, etc.)
    Clear,
}

#[derive(Subcommand)]
enum UserAction {
    /// Grant admin privileges to a user
    GrantAdmin {
        /// Character name
        name: String,
    },
    /// Revoke admin privileges from a user
    RevokeAdmin {
        /// Character name
        name: String,
    },
    /// Grant builder privileges to a user
    GrantBuilder {
        /// Character name
        name: String,
    },
    /// Revoke builder privileges from a user
    RevokeBuilder {
        /// Character name
        name: String,
    },
    /// List all users
    List,
    /// Change a user's password
    ChangePassword {
        /// Character name
        name: String,
    },
    /// Require user to change password on next login
    RequirePasswordChange {
        /// Character name
        name: String,
    },
    /// Delete a character from the database
    Delete {
        /// Character name
        name: String,
    },
}

#[derive(Subcommand)]
enum SettingsAction {
    /// Set a server setting
    Set {
        /// Setting key
        key: String,
        /// Setting value
        value: String,
    },
    /// Get a server setting
    Get {
        /// Setting key
        key: String,
    },
    /// List all settings
    List,
    /// Delete a setting
    Delete {
        /// Setting key
        key: String,
    },
}

#[derive(Subcommand)]
enum ApiKeyAction {
    /// Create a new API key
    Create {
        /// Human-readable name for the key
        #[arg(long)]
        name: String,
        /// Character name for permission checks
        #[arg(long)]
        character: String,
        /// Grant read permission
        #[arg(long)]
        read: bool,
        /// Grant write permission
        #[arg(long)]
        write: bool,
        /// Grant admin permission (bypass area checks)
        #[arg(long)]
        admin: bool,
    },
    /// List all API keys
    List,
    /// Show details for an API key
    Show {
        /// API key ID (UUID)
        id: String,
    },
    /// Revoke (disable) an API key
    Revoke {
        /// API key ID (UUID)
        id: String,
    },
    /// Delete an API key permanently
    Delete {
        /// API key ID (UUID)
        id: String,
    },
}

/// All known settings with their default values.
/// Displayed by `settings list` and used by `settings get` for defaults.
const KNOWN_SETTINGS: &[(&str, &str)] = &[
    // Presets
    ("class_preset", "fantasy"),
    ("race_preset", "fantasy"),
    // Administration
    ("builder_mode", "all"),
    ("motd", ""),
    ("recall_enabled", "true"),
    ("login_lockout_duration", "600"),
    ("idle_timeout_secs", "300"),
    // Regeneration
    ("stamina_regen_standing", "1"),
    ("stamina_regen_sitting", "3"),
    ("stamina_regen_sleeping", "5"),
    ("hp_regen_sitting", "1"),
    ("hp_regen_sleeping", "2"),
    ("mana_regen_standing", "1"),
    ("mana_regen_sitting", "2"),
    ("mana_regen_sleeping", "4"),
    // Stamina costs
    ("stamina_cost_move", "1"),
    ("stamina_cost_move_difficult", "2"),
    ("stamina_cost_attack", "5"),
    ("stamina_cost_recall", "50"),
    ("stamina_cost_flee", "10"),
    // Character needs
    ("thirst_base_rate", "1"),
    ("hunger_base_rate", "1"),
    // Corpse decay
    ("player_corpse_decay_secs", "3600"),
    ("mobile_corpse_decay_secs", "600"),
    // Economy & property
    ("rent_period_game_days", "30"),
    ("escrow_expiry_real_days", "30"),
    // Mail
    ("mail_stamp_price", "10"),
    ("mail_max_messages", "50"),
    ("mail_level_requirement", "5"),
    // Mob behavior
    ("wander_chance_percent", "33"),
    // Child safety
    ("min_attackable_age", "0"),
    ("conception_chance_per_day", "0.005"),
    ("adoption_chance_per_day", "0.10"),
];

fn setting_default(key: &str) -> Option<&'static str> {
    KNOWN_SETTINGS.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Broadcast talks to the live server, not the on-disk database.
    if let Commands::Broadcast { message } = &cli.command {
        let socket = resolve_socket_path(&cli);
        return send_broadcast(&socket, message);
    }

    let db = Db::open(&cli.database).context(format!("Failed to open database at '{}'", cli.database))?;

    match cli.command {
        Commands::User { action } => handle_user_command(&db, action),
        Commands::Settings { action } => handle_settings_command(&db, action),
        Commands::ApiKey { action } => handle_api_key_command(&db, action),
        Commands::World { action } => handle_world_command(&db, action),
        Commands::Broadcast { .. } => unreachable!("handled above"),
    }
}

fn resolve_socket_path(cli: &Cli) -> PathBuf {
    cli.control_socket
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(|| default_socket_path(&cli.database))
}

fn send_broadcast(socket: &PathBuf, message: &str) -> Result<()> {
    let stream = UnixStream::connect(socket).with_context(|| {
        format!(
            "Failed to connect to control socket at {}. Is the server running?",
            socket.display()
        )
    })?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(5)))?;

    let cmd = ControlCommand::Broadcast {
        message: message.to_string(),
    };
    let mut payload = serde_json::to_string(&cmd)?;
    payload.push('\n');
    (&stream).write_all(payload.as_bytes())?;

    let mut reader = BufReader::new(&stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line)?;
    if response_line.is_empty() {
        bail!("Server closed control socket without responding");
    }

    let response: ControlResponse = serde_json::from_str(response_line.trim())
        .map_err(|e| anyhow!("Malformed response from server: {} (raw: {})", e, response_line.trim()))?;
    match response {
        ControlResponse::Ok => {
            println!("Broadcast sent.");
            Ok(())
        }
        ControlResponse::Error { message } => Err(anyhow!("Server rejected broadcast: {}", message)),
    }
}

fn handle_user_command(db: &Db, action: UserAction) -> Result<()> {
    match action {
        UserAction::GrantAdmin { name } => {
            let mut char = db
                .get_character_data(&name)?
                .context(format!("Character '{}' not found", name))?;
            char.is_admin = true;
            db.save_character_data(char)?;
            println!("Granted admin privileges to '{}'", name);
        }
        UserAction::RevokeAdmin { name } => {
            let mut char = db
                .get_character_data(&name)?
                .context(format!("Character '{}' not found", name))?;
            char.is_admin = false;
            db.save_character_data(char)?;
            println!("Revoked admin privileges from '{}'", name);
        }
        UserAction::GrantBuilder { name } => {
            let mut char = db
                .get_character_data(&name)?
                .context(format!("Character '{}' not found", name))?;
            char.is_builder = true;
            db.save_character_data(char)?;
            println!("Granted builder privileges to '{}'", name);
        }
        UserAction::RevokeBuilder { name } => {
            let mut char = db
                .get_character_data(&name)?
                .context(format!("Character '{}' not found", name))?;
            char.is_builder = false;
            db.save_character_data(char)?;
            println!("Revoked builder privileges from '{}'", name);
        }
        UserAction::List => {
            let characters = db.list_all_characters()?;
            if characters.is_empty() {
                println!("No characters found.");
            } else {
                println!("{:<20} {:<10} {:<10} {:<10}", "Name", "Admin", "Builder", "PwdChange");
                println!("{}", "-".repeat(55));
                for char in &characters {
                    println!(
                        "{:<20} {:<10} {:<10} {:<10}",
                        char.name,
                        if char.is_admin { "Yes" } else { "No" },
                        if char.is_builder { "Yes" } else { "No" },
                        if char.must_change_password { "Yes" } else { "No" }
                    );
                }
                println!("\nTotal: {} character(s)", characters.len());
            }
        }
        UserAction::ChangePassword { name } => {
            // Verify character exists first
            let mut char = db
                .get_character_data(&name)?
                .context(format!("Character '{}' not found", name))?;

            // Prompt for new password using rpassword
            let password = rpassword::prompt_password("Enter new password: ").context("Failed to read password")?;

            if password.len() < 4 {
                anyhow::bail!("Password must be at least 4 characters long");
            }

            // Confirm password
            let confirm =
                rpassword::prompt_password("Confirm new password: ").context("Failed to read password confirmation")?;

            if password != confirm {
                anyhow::bail!("Passwords do not match");
            }

            // Hash and save
            let hash = db.hash_password(&password)?;
            char.password_hash = hash;
            char.must_change_password = false; // Clear flag since admin set the password
            db.save_character_data(char)?;

            println!("Password changed for '{}'", name);
        }
        UserAction::RequirePasswordChange { name } => {
            let mut char = db
                .get_character_data(&name)?
                .context(format!("Character '{}' not found", name))?;
            char.must_change_password = true;
            db.save_character_data(char)?;
            println!("Password change required for '{}' on next login", name);
        }
        UserAction::Delete { name } => {
            // Verify character exists
            let _char = db
                .get_character_data(&name)?
                .context(format!("Character '{}' not found", name))?;

            // Confirmation prompt
            println!("WARNING: This will permanently delete character '{}'", name);
            print!("Type the character name to confirm deletion: ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let input = input.trim();

            if input.to_lowercase() != name.to_lowercase() {
                anyhow::bail!("Confirmation failed - character not deleted");
            }

            db.delete_character_data(&name)?;
            println!("Character '{}' has been deleted", name);
        }
    }
    Ok(())
}

fn handle_settings_command(db: &Db, action: SettingsAction) -> Result<()> {
    match action {
        SettingsAction::Set { key, value } => {
            // Validate builder_mode values
            if key == "builder_mode" {
                match value.as_str() {
                    "all" | "granted" | "none" => {}
                    _ => {
                        anyhow::bail!(
                            "Invalid builder_mode value '{}'. Must be 'all', 'granted', or 'none'",
                            value
                        );
                    }
                }
            }
            db.set_setting(&key, &value)?;
            println!("Set '{}' = '{}'", key, value);
        }
        SettingsAction::Get { key } => match db.get_setting(&key)? {
            Some(value) => println!("{} = {}", key, value),
            None => {
                let default = setting_default(&key);
                if let Some(d) = default {
                    println!("{} = {} (default)", key, d);
                } else {
                    println!("{} is not set", key);
                }
            }
        },
        SettingsAction::List => {
            let configured: std::collections::HashMap<String, String> = db.list_all_settings()?.into_iter().collect();

            println!("{:<35} {:<20} {}", "Key", "Value", "Source");
            println!("{}", "-".repeat(70));

            for &(key, default) in KNOWN_SETTINGS {
                if let Some(value) = configured.get(key) {
                    println!("{:<35} {:<20} configured", key, value);
                } else {
                    println!("{:<35} {:<20} default", key, default);
                }
            }

            // Show any extra settings not in KNOWN_SETTINGS
            let known_keys: std::collections::HashSet<&str> = KNOWN_SETTINGS.iter().map(|(k, _)| *k).collect();
            let mut extras: Vec<_> = configured
                .iter()
                .filter(|(k, _)| !known_keys.contains(k.as_str()))
                .collect();
            extras.sort_by_key(|(k, _)| k.to_owned());
            for (key, value) in extras {
                println!("{:<35} {:<20} configured", key, value);
            }
        }
        SettingsAction::Delete { key } => {
            if db.delete_setting(&key)? {
                println!("Deleted setting '{}'", key);
            } else {
                println!("Setting '{}' was not set", key);
            }
        }
    }
    Ok(())
}

fn handle_api_key_command(db: &Db, action: ApiKeyAction) -> Result<()> {
    match action {
        ApiKeyAction::Create {
            name,
            character,
            read,
            write,
            admin,
        } => {
            if admin && !read && !write {
                anyhow::bail!(
                    "--admin alone has no effect. It only bypasses area ownership checks for --read or --write operations. Add --read and/or --write."
                );
            }

            // Verify the character exists
            let _char = db
                .get_character_data(&character)?
                .context(format!("Character '{}' not found", character))?;

            // Generate a random API key (32 bytes, base64 encoded)
            use rand::RngCore;
            let mut key_bytes = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut key_bytes);
            let raw_key = base64_encode(&key_bytes);

            // Hash the key for storage
            let key_hash = db.hash_password(&raw_key)?;

            // Create the API key
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            let api_key = ApiKey {
                id: Uuid::new_v4(),
                key_hash,
                name: name.clone(),
                owner_character: character.clone(),
                permissions: ApiPermissions { read, write, admin },
                created_at: now,
                last_used_at: None,
                enabled: true,
            };

            db.save_api_key(&api_key)?;

            println!("API key created successfully!");
            println!();
            println!("Key ID: {}", api_key.id);
            println!("Name: {}", api_key.name);
            println!("Character: {}", api_key.owner_character);
            println!("Permissions: read={}, write={}, admin={}", read, write, admin);
            println!();
            println!("=== IMPORTANT: Save this key now! It cannot be recovered! ===");
            println!();
            println!("API Key: {}", raw_key);
            println!();
        }
        ApiKeyAction::List => {
            let keys = db.list_all_api_keys()?;
            if keys.is_empty() {
                println!("No API keys found.");
            } else {
                println!(
                    "{:<36}  {:<20}  {:<15}  {:<8}  {}",
                    "ID", "Name", "Character", "Enabled", "Permissions"
                );
                println!("{}", "-".repeat(100));
                for key in &keys {
                    let perms = format!(
                        "{}{}{}",
                        if key.permissions.read { "r" } else { "-" },
                        if key.permissions.write { "w" } else { "-" },
                        if key.permissions.admin { "a" } else { "-" }
                    );
                    println!(
                        "{:<36}  {:<20}  {:<15}  {:<8}  {}",
                        key.id,
                        truncate_str(&key.name, 20),
                        truncate_str(&key.owner_character, 15),
                        if key.enabled { "Yes" } else { "No" },
                        perms
                    );
                }
                println!("\nTotal: {} key(s)", keys.len());
            }
        }
        ApiKeyAction::Show { id } => {
            let uuid = Uuid::parse_str(&id).context("Invalid UUID format")?;
            let key = db.get_api_key(&uuid)?.context(format!("API key '{}' not found", id))?;

            println!("API Key Details");
            println!("{}", "-".repeat(40));
            println!("ID: {}", key.id);
            println!("Name: {}", key.name);
            println!("Character: {}", key.owner_character);
            println!("Enabled: {}", if key.enabled { "Yes" } else { "No" });
            println!("Permissions:");
            println!("  Read: {}", if key.permissions.read { "Yes" } else { "No" });
            println!("  Write: {}", if key.permissions.write { "Yes" } else { "No" });
            println!("  Admin: {}", if key.permissions.admin { "Yes" } else { "No" });
            println!("Created: {}", format_timestamp(key.created_at));
            if let Some(last_used) = key.last_used_at {
                println!("Last Used: {}", format_timestamp(last_used));
            } else {
                println!("Last Used: Never");
            }
        }
        ApiKeyAction::Revoke { id } => {
            let uuid = Uuid::parse_str(&id).context("Invalid UUID format")?;
            let mut key = db.get_api_key(&uuid)?.context(format!("API key '{}' not found", id))?;

            if !key.enabled {
                println!("API key '{}' is already revoked", key.name);
            } else {
                key.enabled = false;
                db.save_api_key(&key)?;
                println!("Revoked API key '{}'", key.name);
            }
        }
        ApiKeyAction::Delete { id } => {
            let uuid = Uuid::parse_str(&id).context("Invalid UUID format")?;
            let key = db.get_api_key(&uuid)?.context(format!("API key '{}' not found", id))?;

            // Confirmation prompt
            println!("WARNING: This will permanently delete API key '{}'", key.name);
            print!("Type the key name to confirm deletion: ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let input = input.trim();

            if input != key.name {
                anyhow::bail!("Confirmation failed - API key not deleted");
            }

            db.delete_api_key(&uuid)?;
            println!("API key '{}' has been deleted", key.name);
        }
    }
    Ok(())
}

/// Base64 encode bytes (URL-safe, no padding)
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut result = String::with_capacity((bytes.len() * 4 + 2) / 3);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        result.push(ALPHABET[b0 >> 2] as char);
        result.push(ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)] as char);
        if chunk.len() > 1 {
            result.push(ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
        }
        if chunk.len() > 2 {
            result.push(ALPHABET[b2 & 0x3f] as char);
        }
    }
    result
}

/// Truncate a string to max_len, adding "..." if truncated
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else if max_len > 3 {
        format!("{}...", &s[..max_len - 3])
    } else {
        s[..max_len].to_string()
    }
}

/// Format a Unix timestamp as a human-readable date
fn format_timestamp(ts: i64) -> String {
    use std::time::{Duration, UNIX_EPOCH};
    let datetime = UNIX_EPOCH + Duration::from_secs(ts as u64);
    // Simple ISO-ish format without external crates
    format!("{:?}", datetime)
}

fn handle_world_command(db: &Db, action: WorldAction) -> Result<()> {
    match action {
        WorldAction::Info => {
            let stats = db.world_stats()?;
            println!("World Statistics");
            println!("{}", "-".repeat(30));
            println!("  Areas:              {:>6}", stats.areas);
            println!("  Rooms:              {:>6}", stats.rooms);
            println!("  Items:              {:>6}", stats.items);
            println!("  Mobiles:            {:>6}", stats.mobiles);
            println!("  Spawn Points:       {:>6}", stats.spawn_points);
            println!("  Recipes:            {:>6}", stats.recipes);
            println!("  Transports:         {:>6}", stats.transports);
            println!("  Property Templates: {:>6}", stats.property_templates);
            println!("  Leases:             {:>6}", stats.leases);
            println!("  Plant Prototypes:   {:>6}", stats.plant_prototypes);
            println!("  Plants:             {:>6}", stats.plants);
            println!("  Characters:         {:>6}", stats.characters);
        }
        WorldAction::Clear => {
            let stats = db.world_stats()?;
            let total = stats.areas
                + stats.rooms
                + stats.items
                + stats.mobiles
                + stats.spawn_points
                + stats.recipes
                + stats.transports
                + stats.property_templates
                + stats.leases
                + stats.plant_prototypes
                + stats.plants;

            if total == 0 {
                println!("World is already empty.");
                return Ok(());
            }

            println!("WARNING: This will permanently delete ALL world data:");
            println!(
                "  {} areas, {} rooms, {} items, {} mobiles, {} spawn points",
                stats.areas, stats.rooms, stats.items, stats.mobiles, stats.spawn_points
            );
            println!(
                "  {} recipes, {} transports, {} property templates, {} leases",
                stats.recipes, stats.transports, stats.property_templates, stats.leases
            );
            println!("  {} plant prototypes, {} plants", stats.plant_prototypes, stats.plants);
            println!();
            println!("Characters, settings, and API keys will be preserved.");
            println!("All characters will be moved to the starting room.");
            println!("The demo world will re-seed on next server start.");
            println!();
            print!("Type CONFIRM to proceed: ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            if input.trim() != "CONFIRM" {
                anyhow::bail!("Confirmation failed — world not cleared");
            }

            db.clear_world_data()?;
            println!("World data cleared. Restart the server to re-seed the demo world.");
        }
    }
    Ok(())
}

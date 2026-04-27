// matrix-sdk has deeply nested async futures that overflow clippy's default
// query depth limit. 512 is comfortable headroom.
#![recursion_limit = "512"]

use rhai::{AST, Engine};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

pub mod aging;
pub mod api;
pub mod chat;
pub mod claude;
pub mod completion;
pub mod control;
pub mod db;
pub mod discord;
pub mod game;
pub mod gemini;
pub mod init;
pub mod matrix;
pub mod migration;
pub mod script;
pub mod seed;
pub mod session;
pub mod social;
pub mod telnet;
pub mod types;

pub use types::*;

// Re-export session and init functions for backward compatibility
pub use init::{load_command_metadata, load_game_data, load_scripts, watch_scripts};
pub use script::check_build_mode;
pub use session::{
    broadcast_to_all_players, broadcast_to_builders, broadcast_to_outdoor_players, broadcast_to_room,
    broadcast_to_room_awake, broadcast_to_room_dreaming, clear_player_character, disconnect_client,
    find_player_connection_by_name, get_character_for_connection, get_characters_in_room,
    get_characters_in_room_with_positions, send_client_message, set_character_for_connection,
};

// Starting room UUID constant
pub const STARTING_ROOM_ID: &str = "00000000-0000-0000-0000-000000000001";

// Session types that depend on tokio/runtime
pub type ConnectionId = Uuid;

pub struct PlayerSession {
    pub character: Option<CharacterData>,
    pub sender: mpsc::UnboundedSender<String>,
    pub input_sender: mpsc::UnboundedSender<InputEvent>,
    pub addr: std::net::SocketAddr,
    // OLC (Online Creation) fields
    pub olc_mode: Option<String>,
    pub olc_buffer: Vec<String>,
    pub olc_edit_room: Option<Uuid>,
    pub olc_edit_item: Option<Uuid>,
    pub olc_extra_keywords: Vec<String>,
    pub olc_undo_buffer: Option<Vec<String>>,
    // Character creation wizard state (JSON-serialized)
    pub wizard_data: Option<String>,
    // MXP (MUD eXtension Protocol) support
    pub mxp_enabled: bool,
    // ANSI color support
    pub colors_enabled: bool,
    // Builder mode: show room flags/vnum in room display
    pub show_room_flags: bool,
    // Telnet protocol state for tab completion
    pub telnet_state: telnet::TelnetState,
    pub input_buffer: String,
    pub cursor_pos: usize,
    // Readline-like input handling
    pub command_history: VecDeque<String>,
    pub history_index: Option<usize>, // None = editing new line, Some(n) = viewing history[n]
    pub saved_input: String,          // Saved current input when navigating history
    pub escape_state: telnet::EscapeState, // State for multi-byte escape sequences
    // AI integration fields (Claude or Gemini)
    pub pending_ai_request: Option<Uuid>,
    pub pending_ai_response: Option<claude::AiResponse>,
    pub pending_ai_target: Option<claude::AiDescriptionTarget>,
    // Fishing state
    pub fishing_state: Option<FishingState>,
    // AFK (Away From Keyboard) status
    pub afk: bool,
    // Idle tracking (unix timestamp of last activity)
    pub last_activity_time: i64,
    // Command abbreviation matching (e.g., 'sc' -> 'scan')
    pub abbrev_enabled: bool,
}

/// Input events from the read handler
#[derive(Debug, Clone)]
pub enum InputEvent {
    /// Complete line (Enter pressed)
    Line(String),
    /// Tab key pressed for completion
    Tab,
    /// Raw bytes for fallback line mode (client doesn't support char mode)
    RawLine(String),
}

pub type SharedConnections = Arc<Mutex<HashMap<ConnectionId, PlayerSession>>>;
pub type SharedState = Arc<Mutex<World>>;

pub struct World {
    pub engine: Engine,
    pub db: crate::db::Db,
    pub connections: SharedConnections,
    pub scripts: HashMap<String, AST>,
    pub command_metadata: HashMap<String, CommandMeta>,
    // Character creation data (loaded from scripts/data/*.json)
    pub class_definitions: HashMap<String, ClassDefinition>,
    pub trait_definitions: HashMap<String, TraitDefinition>,
    pub race_suggestions: Vec<RaceSuggestion>,
    pub race_definitions: HashMap<String, RaceDefinition>,
    // Crafting/cooking recipes (created via recedit command)
    pub recipes: HashMap<String, Recipe>,
    // Spell definitions (loaded from scripts/data/spells_*.json)
    pub spell_definitions: HashMap<String, SpellDefinition>,
    // Transportation system (elevators, buses, trains, etc.)
    pub transports: HashMap<Uuid, TransportData>,
    // Chat integration sender (for disconnect notifications to Matrix/Discord)
    pub chat_sender: Option<chat::ChatSender>,
    // Shutdown command sender (for admin shutdown command)
    pub shutdown_sender: Option<tokio::sync::mpsc::UnboundedSender<ShutdownCommand>>,
    // Shutdown cancellation sender (to abort pending shutdown)
    pub shutdown_cancel_sender: Option<tokio::sync::watch::Sender<bool>>,
}

/// Command sent to trigger server shutdown
#[derive(Debug, Clone)]
pub struct ShutdownCommand {
    pub delay_seconds: u64,
    pub reason: String,
    pub admin_name: String,
}

/// Result of a shutdown cancellation attempt
#[derive(Debug, Clone, PartialEq)]
pub enum CancelShutdownResult {
    Cancelled,
    NoShutdownPending,
}

/// Get the message to broadcast when time of day changes
pub fn get_time_transition_message(tod: &TimeOfDay) -> &'static str {
    match tod {
        TimeOfDay::Dawn => "The sun begins to rise on the horizon, painting the sky in shades of orange and pink.",
        TimeOfDay::Morning => "The morning sun casts long shadows across the land.",
        TimeOfDay::Noon => "The sun reaches its peak in the sky, bathing everything in bright light.",
        TimeOfDay::Afternoon => "The afternoon sun warms the land as the day continues.",
        TimeOfDay::Dusk => "The sun begins to set, painting the sky in orange and crimson hues.",
        TimeOfDay::Evening => "Twilight settles over the land as stars begin to appear.",
        TimeOfDay::Night => "Darkness falls as night takes hold. The stars shine brightly overhead.",
    }
}

/// Get the message to broadcast when season changes
pub fn get_season_transition_message(season: &Season) -> &'static str {
    match season {
        Season::Spring => "The air grows warmer as spring arrives. Flowers begin to bloom across the land.",
        Season::Summer => "Summer has arrived! The sun beats down warmly and the days grow long.",
        Season::Autumn => {
            "The leaves begin to change color as autumn settles in. A cool breeze carries the scent of fallen leaves."
        }
        Season::Winter => "Winter descends upon the land. A chill fills the air as frost blankets the ground.",
    }
}

/// Fire environmental triggers of a given type for all rooms with players
/// Returns true if any triggers were fired
pub fn fire_environmental_triggers_impl(
    db: &db::Db,
    connections: &SharedConnections,
    trigger_type: TriggerType,
    context: &std::collections::HashMap<String, String>,
) -> bool {
    use rand::Rng;

    let rooms = match db.list_all_rooms() {
        Ok(r) => r,
        Err(_) => return false,
    };

    let mut any_fired = false;

    for room in rooms {
        // For time, weather, and season triggers, skip indoor/climate_controlled rooms
        if trigger_type == TriggerType::OnTimeChange
            || trigger_type == TriggerType::OnWeatherChange
            || trigger_type == TriggerType::OnSeasonChange
        {
            // Check for climate_controlled (room or area inherited)
            let is_climate_controlled = room.flags.climate_controlled
                || room
                    .area_id
                    .and_then(|aid| db.get_area_data(&aid).ok().flatten())
                    .map(|area| area.flags.climate_controlled)
                    .unwrap_or(false);
            if room.flags.indoors || is_climate_controlled {
                continue;
            }
        }

        // Find matching triggers
        for trigger in &room.triggers {
            if trigger.trigger_type != trigger_type || !trigger.enabled {
                continue;
            }

            // Check chance
            if trigger.chance < 100 {
                let roll: i32 = rand::thread_rng().gen_range(1..=100);
                if roll > trigger.chance {
                    continue;
                }
            }

            // Find all players in this room
            let players_in_room: Vec<(Uuid, tokio::sync::mpsc::UnboundedSender<String>)> = {
                let conns = connections.lock().unwrap();
                conns
                    .iter()
                    .filter_map(|(conn_id, session)| {
                        if let Some(ref char) = session.character {
                            if char.current_room_id == room.id {
                                return Some((*conn_id, session.sender.clone()));
                            }
                        }
                        None
                    })
                    .collect()
            };

            if players_in_room.is_empty() {
                continue;
            }

            // Handle built-in templates (script_name starts with @)
            if trigger.script_name.starts_with('@') {
                let template_name = &trigger.script_name[1..];
                execute_room_template(template_name, &trigger.args, &room.id, connections, context);
                any_fired = true;
                continue;
            }

            // Execute trigger script
            let script_path = format!("scripts/triggers/{}.rhai", trigger.script_name);
            let script_content = match std::fs::read_to_string(&script_path) {
                Ok(content) => content,
                Err(_) => continue,
            };

            // Create a minimal Rhai engine for trigger execution
            let mut trigger_engine = rhai::Engine::new();

            // Register send_client_message
            for (conn_id, sender) in &players_in_room {
                let conn_id_str = conn_id.to_string();
                let sender_clone = sender.clone();

                trigger_engine.register_fn("send_client_message", move |cid: String, message: String| {
                    if cid == conn_id_str {
                        let _ = sender_clone.send(message);
                    }
                });
            }

            // Register broadcast_to_room
            let conns_clone = connections.clone();
            let room_id = room.id;
            trigger_engine.register_fn(
                "broadcast_to_room",
                move |_rid: String, message: String, _exclude: String| {
                    let conns = conns_clone.lock().unwrap();
                    for (_, session) in conns.iter() {
                        if let Some(ref char) = session.character {
                            if char.current_room_id == room_id {
                                let _ = session.sender.send(message.clone());
                            }
                        }
                    }
                },
            );

            // Register random_int
            trigger_engine.register_fn("random_int", |min: i64, max: i64| {
                if min >= max {
                    return min;
                }
                rand::thread_rng().gen_range(min..=max)
            });

            // Compile and run trigger
            if let Ok(ast) = trigger_engine.compile(&script_content) {
                for (conn_id, _) in &players_in_room {
                    let mut scope = rhai::Scope::new();
                    let room_id_str = room.id.to_string();
                    let conn_id_str = conn_id.to_string();

                    // Build context map
                    let mut rhai_context = rhai::Map::new();
                    for (k, v) in context {
                        rhai_context.insert(k.clone().into(), v.clone().into());
                    }

                    let _ = trigger_engine.call_fn::<rhai::Dynamic>(
                        &mut scope,
                        &ast,
                        "run_trigger",
                        (room_id_str, conn_id_str, rhai_context),
                    );
                }
                any_fired = true;
            }
        }
    }

    any_fired
}

/// Execute a built-in room template for environmental triggers
fn execute_room_template(
    template_name: &str,
    args: &[String],
    room_id: &Uuid,
    connections: &SharedConnections,
    context: &std::collections::HashMap<String, String>,
) {
    // Helper to broadcast message to all players in the room
    let broadcast = |msg: &str| {
        if let Ok(conns) = connections.lock() {
            for (_, session) in conns.iter() {
                if let Some(ref char_data) = session.character {
                    if char_data.current_room_id == *room_id {
                        let _ = session.sender.send(format!("{}\n", msg));
                    }
                }
            }
        }
    };

    match template_name {
        "room_message" => {
            if let Some(message) = args.first() {
                broadcast(message);
            }
        }
        "time_message" => {
            if args.len() >= 2 {
                let target_time = &args[0].to_lowercase();
                let message = &args[1];

                if let Some(new_time) = context.get("new_time") {
                    if new_time.to_lowercase() == *target_time {
                        broadcast(message);
                    }
                }
            }
        }
        "weather_message" => {
            if args.len() >= 2 {
                let target_weather = args[0].to_lowercase();
                let message = &args[1];

                if let Some(new_weather) = context.get("new_weather") {
                    let weather_lower = new_weather.to_lowercase();
                    let matches = weather_lower == target_weather
                        || (target_weather == "raining"
                            && (weather_lower.contains("rain") || weather_lower == "thunderstorm"))
                        || (target_weather == "snowing" && weather_lower.contains("snow"))
                        || (target_weather == "stormy" && weather_lower == "thunderstorm")
                        || (target_weather == "precipitation"
                            && (weather_lower.contains("rain") || weather_lower.contains("snow")));

                    if matches {
                        broadcast(message);
                    }
                }
            }
        }
        "season_message" => {
            if args.len() >= 2 {
                let target_season = args[0].to_lowercase();
                let message = &args[1];

                if let Some(new_season) = context.get("new_season") {
                    if new_season.to_lowercase() == target_season {
                        broadcast(message);
                    }
                }
            }
        }
        _ => {}
    }
}

#[derive(Clone)]
pub struct OnlinePlayer {
    pub name: String,
    pub room_id: Uuid,
    pub addr: String,
}

pub fn get_online_players(connections: &SharedConnections) -> Vec<OnlinePlayer> {
    let conns = connections.lock().unwrap();
    conns
        .values()
        .filter_map(|session| {
            session.character.as_ref().map(|c| OnlinePlayer {
                name: c.name.clone(),
                room_id: c.current_room_id,
                addr: session.addr.to_string(),
            })
        })
        .collect()
}

/// Save all logged-in player characters to the database.
/// Called during graceful shutdown to prevent data loss.
pub fn save_all_players(connections: &SharedConnections, db: &crate::db::Db) -> usize {
    let conns = connections.lock().unwrap();
    let mut saved_count = 0;

    for (_conn_id, session) in conns.iter() {
        if let Some(ref char_data) = session.character {
            match db.save_character_data(char_data.clone()) {
                Ok(()) => {
                    info!("Saved character on shutdown: {}", char_data.name);
                    saved_count += 1;
                }
                Err(e) => {
                    error!("Failed to save {} on shutdown: {}", char_data.name, e);
                }
            }
        }
    }

    saved_count
}

pub fn get_default_aliases() -> HashMap<String, String> {
    let mut defaults = HashMap::new();
    defaults.insert("n".to_string(), "go north".to_string());
    defaults.insert("s".to_string(), "go south".to_string());
    defaults.insert("e".to_string(), "go east".to_string());
    defaults.insert("w".to_string(), "go west".to_string());
    defaults.insert("u".to_string(), "go up".to_string());
    defaults.insert("d".to_string(), "go down".to_string());
    defaults.insert("north".to_string(), "go north".to_string());
    defaults.insert("south".to_string(), "go south".to_string());
    defaults.insert("east".to_string(), "go east".to_string());
    defaults.insert("west".to_string(), "go west".to_string());
    defaults.insert("up".to_string(), "go up".to_string());
    defaults.insert("down".to_string(), "go down".to_string());
    defaults.insert("out".to_string(), "go out".to_string());
    defaults.insert("'".to_string(), "say".to_string());
    defaults.insert(":".to_string(), "emote".to_string());
    defaults.insert("hold".to_string(), "wield".to_string());
    defaults.insert("afk".to_string(), "set afk".to_string());
    defaults.insert("kill".to_string(), "attack".to_string());
    defaults.insert("con".to_string(), "consider".to_string());
    defaults.insert("score".to_string(), "status".to_string());
    defaults
}

fn resolve_alias(connections: &SharedConnections, connection_id: ConnectionId, alias: &str) -> Option<String> {
    let conns = connections.lock().unwrap();
    if let Some(session) = conns.get(&connection_id) {
        if let Some(ref character) = session.character {
            // User aliases take precedence over defaults
            if let Some(expansion) = character.aliases.get(alias) {
                return Some(expansion.clone());
            }
        }
    }
    // Fall back to default aliases
    get_default_aliases().get(alias).cloned()
}

fn expand_alias(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    command_name: &str,
    args: &str,
) -> String {
    if let Some(expansion) = resolve_alias(connections, connection_id, command_name) {
        if args.is_empty() {
            expansion
        } else {
            // Append args to each semicolon-separated command
            expansion
                .split(';')
                .map(|cmd| format!("{} {}", cmd.trim(), args))
                .collect::<Vec<_>>()
                .join(";")
        }
    } else if args.is_empty() {
        command_name.to_string()
    } else {
        format!("{} {}", command_name, args)
    }
}

// === Line Editor Shared Handler ===

/// Help text for the line editor
const EDITOR_HELP: &str = "\
=== Line Editor Commands ===
  .l            - List buffer with line numbers
  .d <n>        - Delete line n
  .i <n> <text> - Insert text before line n
  .r <n> <text> - Replace line n with text
  .s /old/new/  - Substitute: replace all 'old' with 'new'
  .c            - Clear entire buffer
  .u            - Undo last change
  .p            - Preview without line numbers
  .h            - Show this help
  .             - Save and exit
  @             - Cancel and exit
  <text>        - Append text as new line
";

/// Result of processing a line editor command
enum EditorCommandResult {
    /// Command handled, send this message to client
    Handled(String),
    /// User wants to save - buffer contents returned as joined string
    Save(String),
    /// User wants to cancel
    Cancel,
    /// Not an editor command - treat as text to append
    AppendText(String),
}

/// Check if a command is allowed while unconscious.
fn is_unconscious_allowed(cmd: &str) -> bool {
    matches!(
        cmd,
        "quit" | "logout" | "who" | "help" | "diagnose" | "status" | "tell" | "gtell"
    )
}

/// Find a command by prefix matching.
/// Returns the first alphabetically-sorted command that:
/// 1. Starts with the given prefix
/// 2. Has a script file that exists
/// 3. Has access level the user can execute
fn find_command_by_prefix(
    prefix: &str,
    command_metadata: &HashMap<String, CommandMeta>,
    is_logged_in: bool,
    is_builder: bool,
    is_admin: bool,
) -> Option<String> {
    // Collect matching commands that the user can access
    let mut matches: Vec<&String> = command_metadata
        .iter()
        .filter(|(name, meta)| {
            // Must start with prefix
            if !name.starts_with(prefix) {
                return false;
            }
            // Check access level
            match meta.access.as_str() {
                "guest" => !is_logged_in,
                "any" => true,
                "user" => is_logged_in,
                "builder" => is_builder,
                "admin" => is_admin,
                _ => is_logged_in,
            }
        })
        .map(|(name, _)| name)
        .collect();

    // Sort alphabetically and return first match
    matches.sort();
    matches.first().map(|s| (*s).clone())
}

/// Process line editor commands for any OLC mode.
/// Takes mutable references to the buffer and undo buffer.
/// Returns an EditorCommandResult indicating what action to take.
fn handle_editor_command(
    input: &str,
    buffer: &mut Vec<String>,
    undo_buffer: &mut Option<Vec<String>>,
) -> EditorCommandResult {
    let trimmed = input.trim();
    let trimmed_lower = trimmed.to_lowercase();

    // Help
    if trimmed_lower == ".h" {
        return EditorCommandResult::Handled(EDITOR_HELP.to_string());
    }

    // Substitute: .s /old/new/ - replace all occurrences
    if trimmed_lower.starts_with(".s ") {
        let rest = &trimmed[3..];
        if rest.len() < 4 {
            return EditorCommandResult::Handled("Usage: .s /old/new/ (use any delimiter)\n".to_string());
        }
        let delim = rest.chars().next().unwrap();
        let rest_after_delim = &rest[delim.len_utf8()..];
        let parts: Vec<&str> = rest_after_delim.split(delim).collect();
        if parts.len() < 2 {
            return EditorCommandResult::Handled("Usage: .s /old/new/ (use any delimiter)\n".to_string());
        }
        let old_str = parts[0];
        let new_str = parts[1];
        if old_str.is_empty() {
            return EditorCommandResult::Handled("Error: Search string cannot be empty.\n".to_string());
        }
        *undo_buffer = Some(buffer.clone());
        let mut count = 0;
        for line in buffer.iter_mut() {
            if line.contains(old_str) {
                count += line.matches(old_str).count();
                *line = line.replace(old_str, new_str);
            }
        }
        if count > 0 {
            return EditorCommandResult::Handled(format!("Replaced {} occurrence(s).\n", count));
        } else {
            return EditorCommandResult::Handled("No matches found.\n".to_string());
        }
    }

    // Save
    if trimmed == "." {
        let content = buffer.join("\n");
        return EditorCommandResult::Save(content);
    }

    // Cancel
    if trimmed == "@" {
        return EditorCommandResult::Cancel;
    }

    // List with line numbers
    if trimmed_lower == ".l" {
        if buffer.is_empty() {
            return EditorCommandResult::Handled("(empty)\n".to_string());
        }
        let mut output = String::new();
        for (i, line) in buffer.iter().enumerate() {
            output.push_str(&format!("{:3}: {}\n", i + 1, line));
        }
        return EditorCommandResult::Handled(output);
    }

    // Preview without line numbers
    if trimmed_lower == ".p" {
        if buffer.is_empty() {
            return EditorCommandResult::Handled("(empty)\n".to_string());
        }
        return EditorCommandResult::Handled(buffer.join("\n") + "\n");
    }

    // Clear buffer
    if trimmed_lower == ".c" {
        *undo_buffer = Some(buffer.clone());
        buffer.clear();
        return EditorCommandResult::Handled("Buffer cleared.\n".to_string());
    }

    // Undo last change
    if trimmed_lower == ".u" {
        if let Some(undo) = undo_buffer.take() {
            *buffer = undo;
            return EditorCommandResult::Handled("Undo successful.\n".to_string());
        } else {
            return EditorCommandResult::Handled("Nothing to undo.\n".to_string());
        }
    }

    // Delete line N
    if trimmed_lower.starts_with(".d ") {
        let line_num: Option<usize> = trimmed[3..].trim().parse().ok();
        if let Some(n) = line_num {
            if n >= 1 && n <= buffer.len() {
                *undo_buffer = Some(buffer.clone());
                buffer.remove(n - 1);
                return EditorCommandResult::Handled(format!("Line {} deleted.\n", n));
            } else {
                return EditorCommandResult::Handled(format!("Line {} out of range (1-{}).\n", n, buffer.len()));
            }
        } else {
            return EditorCommandResult::Handled("Usage: .d <line_number>\n".to_string());
        }
    }

    // Insert before line N
    if trimmed_lower.starts_with(".i ") {
        let rest = &trimmed[3..];
        let parts: Vec<&str> = rest.splitn(2, ' ').collect();
        if parts.len() >= 2 {
            let line_num: Option<usize> = parts[0].trim().parse().ok();
            let text = parts[1].to_string();
            if let Some(n) = line_num {
                if n >= 1 && n <= buffer.len() + 1 {
                    *undo_buffer = Some(buffer.clone());
                    buffer.insert(n - 1, text);
                    return EditorCommandResult::Handled(format!("Line {} inserted.\n", n));
                } else {
                    return EditorCommandResult::Handled(format!(
                        "Line {} out of range (1-{}).\n",
                        n,
                        buffer.len() + 1
                    ));
                }
            } else {
                return EditorCommandResult::Handled("Usage: .i <line_number> <text>\n".to_string());
            }
        } else {
            return EditorCommandResult::Handled("Usage: .i <line_number> <text>\n".to_string());
        }
    }

    // Replace line N
    if trimmed_lower.starts_with(".r ") {
        let rest = &trimmed[3..];
        let parts: Vec<&str> = rest.splitn(2, ' ').collect();
        if parts.len() >= 2 {
            let line_num: Option<usize> = parts[0].trim().parse().ok();
            let text = parts[1].to_string();
            if let Some(n) = line_num {
                if n >= 1 && n <= buffer.len() {
                    *undo_buffer = Some(buffer.clone());
                    buffer[n - 1] = text;
                    return EditorCommandResult::Handled(format!("Line {} replaced.\n", n));
                } else {
                    return EditorCommandResult::Handled(format!("Line {} out of range (1-{}).\n", n, buffer.len()));
                }
            } else {
                return EditorCommandResult::Handled("Usage: .r <line_number> <text>\n".to_string());
            }
        } else {
            return EditorCommandResult::Handled("Usage: .r <line_number> <text>\n".to_string());
        }
    }

    // Not an editor command - treat as text to append
    EditorCommandResult::AppendText(trimmed.to_string())
}

/// Build the prompt string for a connection, respecting prompt_mode setting.
/// Returns simple "> " for guests or when prompt_mode is "simple".
/// Returns verbose "[HP:current/max] >" with color coding when prompt_mode is "verbose".
fn build_prompt(connection_id: &ConnectionId, connections: &SharedConnections, state: &SharedState) -> String {
    let (
        prompt_mode,
        hp,
        max_hp,
        stamina,
        max_stamina,
        mana,
        max_mana,
        mana_enabled,
        breath,
        max_breath,
        colors_enabled,
        char_name,
        build_mode,
    ) = {
        let conns = connections.lock().unwrap();
        let session = match conns.get(connection_id) {
            Some(s) => s,
            None => return "> ".to_string(),
        };

        let colors = session.colors_enabled;

        match &session.character {
            Some(c) => {
                let mode = if c.prompt_mode.is_empty() {
                    "simple"
                } else {
                    &c.prompt_mode
                };
                // Apply torso wound HP cap
                let torso_penalty = c
                    .wounds
                    .iter()
                    .filter(|w| w.body_part == BodyPart::Torso)
                    .map(|w| w.level.penalty())
                    .max()
                    .unwrap_or(0);
                let effective_max_hp = if torso_penalty > 0 {
                    (c.max_hp * (100 - torso_penalty) / 100).max(1)
                } else {
                    c.max_hp
                };
                // Apply head wound mana cap
                let head_penalty = c
                    .wounds
                    .iter()
                    .filter(|w| w.body_part == BodyPart::Head)
                    .map(|w| w.level.penalty())
                    .max()
                    .unwrap_or(0);
                let effective_max_mana = if head_penalty > 0 {
                    (c.max_mana * (100 - head_penalty) / 100).max(0)
                } else {
                    c.max_mana
                };
                (
                    mode.to_string(),
                    c.hp,
                    effective_max_hp,
                    c.stamina,
                    c.max_stamina,
                    c.mana,
                    effective_max_mana,
                    c.mana_enabled,
                    c.breath,
                    c.max_breath,
                    colors,
                    c.name.clone(),
                    c.build_mode,
                )
            }
            None => return "> ".to_string(), // Not logged in = simple prompt
        }
    };

    // Get equipped items from database (source of truth is ItemLocation::Equipped)
    let equipped_items: Vec<Uuid> = {
        let world = state.lock().unwrap();
        world
            .db
            .get_equipped_items(&char_name)
            .unwrap_or_default()
            .into_iter()
            .map(|item| item.id)
            .collect()
    };

    if prompt_mode == "simple" {
        return "> ".to_string();
    }

    // Verbose prompt
    let hp_percent = if max_hp > 0 { (hp * 100) / max_hp } else { 100 };

    // Color based on health percentage
    let (hp_color, reset) = if colors_enabled {
        let color = if hp_percent >= 70 {
            "\x1b[32m" // Green: healthy
        } else if hp_percent >= 30 {
            "\x1b[33m" // Yellow: wounded
        } else {
            "\x1b[31m" // Red: critical
        };
        (color, "\x1b[0m")
    } else {
        ("", "")
    };

    // Stamina percentage and color
    let stamina_percent = if max_stamina > 0 {
        (stamina * 100) / max_stamina
    } else {
        100
    };
    let st_color = if colors_enabled {
        if stamina_percent >= 70 {
            "\x1b[36m" // Cyan: energized
        } else if stamina_percent >= 30 {
            "\x1b[34m" // Blue: tired
        } else {
            "\x1b[35m" // Magenta: exhausted
        }
    } else {
        ""
    };

    // Mana segment (only if mana_enabled)
    let mana_segment = if mana_enabled {
        let mana_percent = if max_mana > 0 { (mana * 100) / max_mana } else { 100 };
        let mp_color = if colors_enabled {
            if mana_percent >= 70 {
                "\x1b[94m" // Bright blue: full
            } else if mana_percent >= 30 {
                "\x1b[34m" // Blue: moderate
            } else {
                "\x1b[35m" // Magenta: low
            }
        } else {
            ""
        };
        format!("[{}MP:{}/{}{}] ", mp_color, mana, max_mana, reset)
    } else {
        String::new()
    };

    // Breath segment (only shown when breath < max_breath)
    let breath_segment = if breath < max_breath {
        let breath_percent = if max_breath > 0 {
            (breath * 100) / max_breath
        } else {
            100
        };
        let br_color = if colors_enabled {
            if breath_percent >= 50 {
                "\x1b[36m" // Cyan: ok
            } else if breath_percent >= 25 {
                "\x1b[33m" // Yellow: low
            } else {
                "\x1b[31m" // Red: critical
            }
        } else {
            ""
        };
        format!("[{}Air:{}/{}{}] ", br_color, breath, max_breath, reset)
    } else {
        String::new()
    };

    let base_prompt = format!(
        "[{}HP:{}/{}{}] [{}ST:{}/{}{}] {}{}",
        hp_color, hp, max_hp, reset, st_color, stamina, max_stamina, reset, mana_segment, breath_segment
    );

    // Distance indicator for combat prompt
    let distance_tag = {
        let world = state.lock().unwrap();
        if let Ok(Some(char_data)) = world.db.get_character_data(&char_name) {
            if char_data.combat.in_combat && !char_data.combat.targets.is_empty() {
                let primary = &char_data.combat.targets[0];
                let distance = char_data
                    .combat
                    .distances
                    .get(&primary.target_id)
                    .copied()
                    .unwrap_or(CombatDistance::Melee);
                match distance {
                    CombatDistance::Ranged | CombatDistance::Pole => {
                        // Resolve target name
                        let target_name = match primary.target_type {
                            CombatTargetType::Mobile => world
                                .db
                                .get_mobile_data(&primary.target_id)
                                .ok()
                                .flatten()
                                .map(|m| m.name.clone())
                                .unwrap_or_else(|| "opponent".to_string()),
                            CombatTargetType::Player => "opponent".to_string(),
                        };
                        let (tag_color, label) = if distance == CombatDistance::Ranged {
                            ("\x1b[33m", "Ranged")
                        } else {
                            ("\x1b[36m", "Pole")
                        };
                        if colors_enabled {
                            format!("{}[{}: {}]\x1b[0m ", tag_color, label, target_name)
                        } else {
                            format!("[{}: {}] ", label, target_name)
                        }
                    }
                    CombatDistance::Melee => String::new(),
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    };

    // Collect on_prompt trigger contributions from equipped items
    let extra = collect_on_prompt_contributions(connection_id, connections, state, &equipped_items);

    // Build mode indicator
    let build_tag = if build_mode {
        if colors_enabled {
            "\x1b[1;33m[BUILD]\x1b[0m ".to_string()
        } else {
            "[BUILD] ".to_string()
        }
    } else {
        String::new()
    };

    format!("{}{}{}{}> ", base_prompt, distance_tag, extra, build_tag)
}

/// Collect prompt contributions from equipped items with on_prompt triggers
fn collect_on_prompt_contributions(
    connection_id: &ConnectionId,
    _connections: &SharedConnections,
    state: &SharedState,
    equipped_item_ids: &[Uuid],
) -> String {
    let mut contributions = String::new();

    let world = state.lock().unwrap();

    for item_id in equipped_item_ids {
        let item = match world.db.get_item_data(item_id) {
            Ok(Some(i)) => i,
            _ => continue,
        };

        // Find enabled on_prompt triggers
        let prompt_triggers: Vec<_> = item
            .triggers
            .iter()
            .filter(|t| t.trigger_type == ItemTriggerType::OnPrompt && t.enabled)
            .collect();

        if prompt_triggers.is_empty() {
            continue;
        }

        for trigger in prompt_triggers {
            // Check chance
            if trigger.chance < 100 {
                use rand::Rng;
                let roll = rand::thread_rng().gen_range(1..=100);
                if roll > trigger.chance {
                    continue;
                }
            }

            // Load and execute trigger script
            let script_path = format!("scripts/triggers/{}.rhai", trigger.script_name);
            let script_content = match std::fs::read_to_string(&script_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            // Create a minimal engine for trigger execution
            let mut trigger_engine = rhai::Engine::new();

            // Register get_game_time for watch triggers
            let db_for_time = world.db.clone();
            trigger_engine.register_fn("get_game_time", move || -> rhai::Map {
                let game_time = db_for_time.get_game_time().unwrap_or_default();
                let mut map = rhai::Map::new();
                map.insert("hour".into(), rhai::Dynamic::from(game_time.hour as i64));
                map.insert("day".into(), rhai::Dynamic::from(game_time.day as i64));
                map.insert("month".into(), rhai::Dynamic::from(game_time.month as i64));
                map.insert("year".into(), rhai::Dynamic::from(game_time.year as i64));
                map.insert(
                    "season".into(),
                    rhai::Dynamic::from(game_time.get_season().to_string().to_lowercase()),
                );
                map
            });

            // Compile and run
            match trigger_engine.compile(&script_content) {
                Ok(ast) => {
                    let item_id_str = item_id.to_string();
                    let conn_id_str = connection_id.to_string();
                    let context = rhai::Map::new();

                    match trigger_engine.call_fn::<rhai::Dynamic>(
                        &mut rhai::Scope::new(),
                        &ast,
                        "run_trigger",
                        (item_id_str, conn_id_str, context),
                    ) {
                        Ok(result) => {
                            let result_str = result.to_string();
                            if !result_str.is_empty() && result_str != "()" {
                                contributions.push_str(&result_str);
                            }
                        }
                        Err(e) => {
                            tracing::error!("on_prompt trigger runtime error: {}", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("on_prompt trigger compile error: {}", e);
                }
            }
        }
    }

    contributions
}

pub async fn handle_connection(
    socket: tokio::net::TcpStream,
    addr: std::net::SocketAddr,
    state: SharedState,
    connection_id: ConnectionId,
) {
    let (reader, writer) = socket.into_split();

    let (tx_client, rx_client) = mpsc::unbounded_channel::<String>();
    let (tx_input, mut rx_input) = mpsc::unbounded_channel::<InputEvent>();
    let (tx_raw, rx_raw) = mpsc::unbounded_channel::<Vec<u8>>();

    // Get the connections map from World and store the PlayerSession
    let connections = {
        let world = state.lock().unwrap();
        world.connections.clone()
    };

    {
        let mut conns = connections.lock().unwrap();
        conns.insert(
            connection_id,
            PlayerSession {
                character: None,
                sender: tx_client.clone(),
                input_sender: tx_input.clone(),
                addr,
                olc_mode: None,
                olc_buffer: Vec::new(),
                olc_edit_room: None,
                olc_edit_item: None,
                olc_extra_keywords: Vec::new(),
                olc_undo_buffer: None,
                wizard_data: None,
                mxp_enabled: false,
                colors_enabled: true,
                show_room_flags: false,
                telnet_state: telnet::TelnetState::new(),
                input_buffer: String::new(),
                cursor_pos: 0,
                command_history: VecDeque::new(),
                history_index: None,
                saved_input: String::new(),
                escape_state: telnet::EscapeState::Normal,
                pending_ai_request: None,
                pending_ai_response: None,
                pending_ai_target: None,
                fishing_state: None,
                afk: false,
                last_activity_time: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64,
                abbrev_enabled: true,
            },
        );
    }

    // Send welcome banner
    if let Ok(banner) = std::fs::read_to_string("assets/banner.txt") {
        let _ = tx_client.send(banner);
    }

    // Spawn character-mode read task with telnet protocol support
    tokio::spawn(handle_read_char_mode(
        reader,
        addr,
        connection_id,
        tx_input,
        connections.clone(),
        tx_raw,
    ));

    // Spawn write task with raw byte support for telnet negotiation
    tokio::spawn(handle_write_with_raw(writer, addr, rx_client, rx_raw));

    // Main loop for processing client input and sending responses
    loop {
        // Check if connection still exists. If not, it means it was disconnected by a script.
        let is_connected = {
            let conns = connections.lock().unwrap();
            conns.contains_key(&connection_id)
        };
        if !is_connected {
            info!("Connection {} removed by script, terminating handle_connection.", addr);
            break;
        }

        // Only send prompt in line mode (char mode echoes as you type)
        let is_char_mode = {
            let conns = connections.lock().unwrap();
            conns
                .get(&connection_id)
                .map(|s| s.telnet_state.char_mode)
                .unwrap_or(false)
        };
        if !is_char_mode {
            let prompt = build_prompt(&connection_id, &connections, &state);
            if let Err(e) = tx_client.send(prompt) {
                error!("Failed to send prompt to socket {}: {}", addr, e);
                break;
            }
        }

        let event = match rx_input.recv().await {
            Some(event) => event,
            None => break, // Read task disconnected
        };

        // Handle Tab completion
        if matches!(event, InputEvent::Tab) {
            // Get current input buffer and cursor position
            let (current_input, cursor_pos, window_width) = {
                let conns = connections.lock().unwrap();
                conns
                    .get(&connection_id)
                    .map(|s| (s.input_buffer.clone(), s.cursor_pos, s.telnet_state.window_width))
                    .unwrap_or_default()
            };

            // Gather completion data
            let (
                available_commands,
                room_vnums,
                item_vnums,
                mobile_vnums,
                area_prefixes,
                recipe_vnums,
                transport_vnums,
                property_template_vnums,
                shop_preset_vnums,
                plant_vnums,
                spell_names,
                online_players,
                has_builder_access,
            ) = {
                let world = state.lock().unwrap();

                // Get commands the user can access
                let is_logged_in = {
                    let conns = connections.lock().unwrap();
                    conns.get(&connection_id).and_then(|s| s.character.as_ref()).is_some()
                };
                let (is_builder, is_admin) = {
                    let conns = connections.lock().unwrap();
                    conns
                        .get(&connection_id)
                        .and_then(|s| s.character.as_ref())
                        .map(|c| (c.is_builder, c.is_admin))
                        .unwrap_or((false, false))
                };

                let available_commands: Vec<String> = world
                    .command_metadata
                    .iter()
                    .filter(|(_, meta)| match meta.access.as_str() {
                        "guest" => true,
                        "user" => is_logged_in,
                        "builder" => is_builder || is_admin,
                        "admin" => is_admin,
                        _ => true,
                    })
                    .map(|(name, _)| name.clone())
                    .collect();

                // Get vnums from database
                let room_vnums: Vec<String> = world
                    .db
                    .list_all_rooms()
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|r| r.vnum.clone())
                    .collect();

                let item_vnums: Vec<String> = world
                    .db
                    .list_all_items()
                    .unwrap_or_default()
                    .iter()
                    .filter(|i| i.is_prototype)
                    .filter_map(|i| i.vnum.clone())
                    .collect();

                let mobile_vnums: Vec<String> = world
                    .db
                    .list_all_mobiles()
                    .unwrap_or_default()
                    .iter()
                    .filter(|m| m.is_prototype)
                    .filter_map(|m| Some(m.vnum.clone()))
                    .filter(|v| !v.is_empty())
                    .collect();

                let area_prefixes: Vec<String> = world
                    .db
                    .list_all_areas()
                    .unwrap_or_default()
                    .iter()
                    .map(|a| a.prefix.clone())
                    .collect();

                let recipe_vnums: Vec<String> = world.recipes.keys().cloned().collect();

                let transport_vnums: Vec<String> = world
                    .db
                    .list_all_transports()
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|t| t.vnum.clone())
                    .collect();

                let property_template_vnums: Vec<String> = world
                    .db
                    .list_all_property_templates()
                    .unwrap_or_default()
                    .iter()
                    .map(|t| t.vnum.clone())
                    .collect();

                let shop_preset_vnums: Vec<String> = world
                    .db
                    .list_all_shop_presets()
                    .unwrap_or_default()
                    .iter()
                    .map(|p| p.vnum.clone())
                    .collect();

                let plant_vnums: Vec<String> = world
                    .db
                    .list_all_plant_prototypes()
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|p| p.vnum.clone())
                    .collect();

                let spell_names: Vec<String> = world.spell_definitions.values().map(|s| s.name.clone()).collect();

                // Get online player names
                let online_players: Vec<String> = {
                    let conns = connections.lock().unwrap();
                    conns
                        .values()
                        .filter_map(|s| s.character.as_ref())
                        .map(|c| c.name.clone())
                        .collect()
                };

                let has_builder_access = is_builder || is_admin;

                (
                    available_commands,
                    room_vnums,
                    item_vnums,
                    mobile_vnums,
                    area_prefixes,
                    recipe_vnums,
                    transport_vnums,
                    property_template_vnums,
                    shop_preset_vnums,
                    plant_vnums,
                    spell_names,
                    online_players,
                    has_builder_access,
                )
            };

            // Perform completion
            let result = completion::complete(
                &current_input,
                cursor_pos,
                &available_commands,
                &room_vnums,
                &item_vnums,
                &mobile_vnums,
                &area_prefixes,
                &recipe_vnums,
                &transport_vnums,
                &property_template_vnums,
                &shop_preset_vnums,
                &plant_vnums,
                &spell_names,
                &online_players,
                has_builder_access,
            );

            if result.is_empty() {
                // No completions - just beep or do nothing
                continue;
            }

            if result.is_unique() {
                // Single match - auto-complete
                let completed = &result.completions[0];
                let to_add = &completed[result.partial.len()..];

                let new_input = {
                    let mut conns = connections.lock().unwrap();
                    if let Some(session) = conns.get_mut(&connection_id) {
                        // Find byte offset for cursor position (char-based)
                        let byte_pos = session
                            .input_buffer
                            .char_indices()
                            .nth(session.cursor_pos)
                            .map(|(i, _)| i)
                            .unwrap_or(session.input_buffer.len());

                        let at_end = byte_pos >= session.input_buffer.len();

                        // Insert completion at cursor position
                        session.input_buffer.insert_str(byte_pos, to_add);
                        session.cursor_pos += to_add.chars().count();

                        // Add trailing space only if cursor was at end of buffer
                        if at_end {
                            session.input_buffer.push(' ');
                            session.cursor_pos += 1;
                        }

                        session.input_buffer.clone()
                    } else {
                        continue;
                    }
                };

                // Redraw the full line (handles cursor positioning)
                let cursor_pos = {
                    let conns = connections.lock().unwrap();
                    conns.get(&connection_id).map(|s| s.cursor_pos).unwrap_or(0)
                };
                let redraw = telnet::redraw_input_line("> ", &new_input, cursor_pos);
                let _ = tx_client.send(String::from_utf8_lossy(&redraw).into_owned());
            } else {
                // Multiple matches - show list and complete common prefix
                let display = completion::format_completions(&result, window_width);

                // Complete to common prefix if it's longer than what we have
                let to_add = if result.common_prefix.len() > result.partial.len() {
                    &result.common_prefix[result.partial.len()..]
                } else {
                    ""
                };

                if !to_add.is_empty() {
                    let mut conns = connections.lock().unwrap();
                    if let Some(session) = conns.get_mut(&connection_id) {
                        let byte_pos = session
                            .input_buffer
                            .char_indices()
                            .nth(session.cursor_pos)
                            .map(|(i, _)| i)
                            .unwrap_or(session.input_buffer.len());

                        session.input_buffer.insert_str(byte_pos, to_add);
                        session.cursor_pos += to_add.chars().count();
                    }
                }

                // Redraw: show completions list, then redraw input line with cursor
                let (new_input, cursor_pos) = {
                    let conns = connections.lock().unwrap();
                    conns
                        .get(&connection_id)
                        .map(|s| (s.input_buffer.clone(), s.cursor_pos))
                        .unwrap_or_default()
                };
                let redraw = telnet::redraw_input_line("> ", &new_input, cursor_pos);
                let redraw_str = String::from_utf8_lossy(&redraw).into_owned();
                let output = format!("\r\n{}\r\n{}", display, redraw_str);
                let _ = tx_client.send(output);
            }
            continue;
        }

        // Extract input string from event
        let input = match event {
            InputEvent::Line(s) | InputEvent::RawLine(s) => s,
            InputEvent::Tab => continue, // Already handled above
        };

        // Update last activity time for idle tracking
        {
            let mut conns = connections.lock().unwrap();
            if let Some(session) = conns.get_mut(&connection_id) {
                session.last_activity_time = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
            }
        }

        // Check for OLC description collection mode
        let olc_mode = {
            let conns = connections.lock().unwrap();
            conns.get(&connection_id).and_then(|s| s.olc_mode.clone())
        };

        if olc_mode.as_deref() == Some("collecting_desc") {
            // Use shared editor command handler
            let result = {
                let mut conns = connections.lock().unwrap();
                if let Some(session) = conns.get_mut(&connection_id) {
                    handle_editor_command(&input, &mut session.olc_buffer, &mut session.olc_undo_buffer)
                } else {
                    EditorCommandResult::Cancel
                }
            };

            match result {
                EditorCommandResult::Handled(msg) => {
                    let _ = tx_client.send(msg);
                }
                EditorCommandResult::Save(content) => {
                    // Save buffer to room description
                    let edit_room_id = {
                        let conns = connections.lock().unwrap();
                        conns.get(&connection_id).and_then(|s| s.olc_edit_room)
                    };

                    if let Some(room_id) = edit_room_id {
                        let world = state.lock().unwrap();
                        if let Ok(Some(mut room)) = world.db.get_room_data(&room_id) {
                            room.description = content;
                            let _ = world.db.save_room_data(room);
                        }
                        drop(world);
                        let _ = tx_client.send("Description saved.\n".to_string());
                    } else {
                        let _ = tx_client.send("Error: No room being edited.\n".to_string());
                    }

                    // Clear OLC state
                    {
                        let mut conns = connections.lock().unwrap();
                        if let Some(session) = conns.get_mut(&connection_id) {
                            session.olc_mode = None;
                            session.olc_buffer.clear();
                            session.olc_edit_room = None;
                            session.olc_undo_buffer = None;
                        }
                    }
                    let prompt = build_prompt(&connection_id, &connections, &state);
                    let _ = tx_client.send(prompt);
                }
                EditorCommandResult::Cancel => {
                    // Clear OLC state
                    {
                        let mut conns = connections.lock().unwrap();
                        if let Some(session) = conns.get_mut(&connection_id) {
                            session.olc_mode = None;
                            session.olc_buffer.clear();
                            session.olc_edit_room = None;
                            session.olc_undo_buffer = None;
                        }
                    }
                    let _ = tx_client.send("Description editing cancelled.\n".to_string());
                    let prompt = build_prompt(&connection_id, &connections, &state);
                    let _ = tx_client.send(prompt);
                }
                EditorCommandResult::AppendText(text) => {
                    let mut conns = connections.lock().unwrap();
                    if let Some(session) = conns.get_mut(&connection_id) {
                        session.olc_undo_buffer = Some(session.olc_buffer.clone());
                        session.olc_buffer.push(text);
                    }
                }
            }
            continue;
        }

        // Check for OLC note collection mode (item.note_content)
        if olc_mode.as_deref() == Some("collecting_note") {
            let result = {
                let mut conns = connections.lock().unwrap();
                if let Some(session) = conns.get_mut(&connection_id) {
                    handle_editor_command(&input, &mut session.olc_buffer, &mut session.olc_undo_buffer)
                } else {
                    EditorCommandResult::Cancel
                }
            };

            match result {
                EditorCommandResult::Handled(msg) => {
                    let _ = tx_client.send(msg);
                }
                EditorCommandResult::Save(content) => {
                    // Cap at 32 KB to avoid runaway bodies. Keep mode active on over-cap.
                    const MAX_NOTE_BYTES: usize = 32 * 1024;
                    if content.len() > MAX_NOTE_BYTES {
                        let _ = tx_client.send(format!(
                            "Note too long ({} bytes, max {}). Trim with .d or .r, then .save.\n",
                            content.len(),
                            MAX_NOTE_BYTES
                        ));
                    } else {
                        let edit_item_id = {
                            let conns = connections.lock().unwrap();
                            conns.get(&connection_id).and_then(|s| s.olc_edit_item)
                        };

                        let (saved, cleared) = if let Some(item_id) = edit_item_id {
                            let world = state.lock().unwrap();
                            match world.db.get_item_data(&item_id) {
                                Ok(Some(mut item)) => {
                                    let cleared = content.is_empty();
                                    item.note_content = if cleared { None } else { Some(content) };
                                    let ok = world.db.save_item_data(item).is_ok();
                                    (ok, cleared)
                                }
                                _ => (false, false),
                            }
                        } else {
                            (false, false)
                        };

                        if saved {
                            let _ =
                                tx_client.send(if cleared { "Note cleared.\n" } else { "Note saved.\n" }.to_string());
                        } else {
                            let _ = tx_client.send("Error: could not save note.\n".to_string());
                        }

                        {
                            let mut conns = connections.lock().unwrap();
                            if let Some(session) = conns.get_mut(&connection_id) {
                                session.olc_mode = None;
                                session.olc_buffer.clear();
                                session.olc_edit_item = None;
                                session.olc_undo_buffer = None;
                            }
                        }
                        let prompt = build_prompt(&connection_id, &connections, &state);
                        let _ = tx_client.send(prompt);
                    }
                }
                EditorCommandResult::Cancel => {
                    {
                        let mut conns = connections.lock().unwrap();
                        if let Some(session) = conns.get_mut(&connection_id) {
                            session.olc_mode = None;
                            session.olc_buffer.clear();
                            session.olc_edit_item = None;
                            session.olc_undo_buffer = None;
                        }
                    }
                    let _ = tx_client.send("Note editing cancelled.\n".to_string());
                    let prompt = build_prompt(&connection_id, &connections, &state);
                    let _ = tx_client.send(prompt);
                }
                EditorCommandResult::AppendText(text) => {
                    let mut conns = connections.lock().unwrap();
                    if let Some(session) = conns.get_mut(&connection_id) {
                        session.olc_undo_buffer = Some(session.olc_buffer.clone());
                        session.olc_buffer.push(text);
                    }
                }
            }
            continue;
        }

        // Check for OLC extra description collection mode
        if olc_mode.as_deref() == Some("collecting_extra_desc") {
            // Use shared editor command handler
            let result = {
                let mut conns = connections.lock().unwrap();
                if let Some(session) = conns.get_mut(&connection_id) {
                    handle_editor_command(&input, &mut session.olc_buffer, &mut session.olc_undo_buffer)
                } else {
                    EditorCommandResult::Cancel
                }
            };

            match result {
                EditorCommandResult::Handled(msg) => {
                    let _ = tx_client.send(msg);
                }
                EditorCommandResult::Save(content) => {
                    // Mode-specific save: add extra description to room
                    let (edit_room_id, keywords) = {
                        let conns = connections.lock().unwrap();
                        if let Some(session) = conns.get(&connection_id) {
                            (session.olc_edit_room, session.olc_extra_keywords.clone())
                        } else {
                            (None, Vec::new())
                        }
                    };

                    if let Some(room_id) = edit_room_id {
                        let world = state.lock().unwrap();
                        if let Ok(Some(mut room)) = world.db.get_room_data(&room_id) {
                            room.extra_descs.push(ExtraDesc {
                                keywords,
                                description: content,
                            });
                            let _ = world.db.save_room_data(room);
                        }
                        drop(world);
                        let _ = tx_client.send("Extra description saved.\n".to_string());
                    } else {
                        let _ = tx_client.send("Error: No room being edited.\n".to_string());
                    }

                    // Clear OLC mode and buffer
                    {
                        let mut conns = connections.lock().unwrap();
                        if let Some(session) = conns.get_mut(&connection_id) {
                            session.olc_mode = None;
                            session.olc_buffer.clear();
                            session.olc_edit_room = None;
                            session.olc_extra_keywords.clear();
                            session.olc_undo_buffer = None;
                        }
                    }
                }
                EditorCommandResult::Cancel => {
                    // Clear OLC mode and buffer
                    {
                        let mut conns = connections.lock().unwrap();
                        if let Some(session) = conns.get_mut(&connection_id) {
                            session.olc_mode = None;
                            session.olc_buffer.clear();
                            session.olc_edit_room = None;
                            session.olc_extra_keywords.clear();
                            session.olc_undo_buffer = None;
                        }
                    }
                    let _ = tx_client.send("Extra description editing cancelled.\n".to_string());
                }
                EditorCommandResult::AppendText(text) => {
                    // Append line to buffer (save undo first)
                    {
                        let mut conns = connections.lock().unwrap();
                        if let Some(session) = conns.get_mut(&connection_id) {
                            session.olc_undo_buffer = Some(session.olc_buffer.clone());
                            session.olc_buffer.push(text);
                        }
                    }
                    let _ = tx_client.send("Line added.\n".to_string());
                }
            }
            continue;
        }

        // Check for OLC MOTD collection mode
        if olc_mode.as_deref() == Some("collecting_motd") {
            let result = {
                let mut conns = connections.lock().unwrap();
                if let Some(session) = conns.get_mut(&connection_id) {
                    handle_editor_command(&input, &mut session.olc_buffer, &mut session.olc_undo_buffer)
                } else {
                    EditorCommandResult::Cancel
                }
            };

            match result {
                EditorCommandResult::Handled(msg) => {
                    let _ = tx_client.send(msg);
                }
                EditorCommandResult::Save(content) => {
                    let world = state.lock().unwrap();
                    let _ = world.db.set_setting("motd", &content);
                    drop(world);
                    let _ = tx_client.send("MOTD saved.\n".to_string());

                    // Clear OLC state
                    {
                        let mut conns = connections.lock().unwrap();
                        if let Some(session) = conns.get_mut(&connection_id) {
                            session.olc_mode = None;
                            session.olc_buffer.clear();
                            session.olc_undo_buffer = None;
                        }
                    }
                    let prompt = build_prompt(&connection_id, &connections, &state);
                    let _ = tx_client.send(prompt);
                }
                EditorCommandResult::Cancel => {
                    // Clear OLC state
                    {
                        let mut conns = connections.lock().unwrap();
                        if let Some(session) = conns.get_mut(&connection_id) {
                            session.olc_mode = None;
                            session.olc_buffer.clear();
                            session.olc_undo_buffer = None;
                        }
                    }
                    let _ = tx_client.send("MOTD editing cancelled.\n".to_string());
                    let prompt = build_prompt(&connection_id, &connections, &state);
                    let _ = tx_client.send(prompt);
                }
                EditorCommandResult::AppendText(text) => {
                    let mut conns = connections.lock().unwrap();
                    if let Some(session) = conns.get_mut(&connection_id) {
                        session.olc_undo_buffer = Some(session.olc_buffer.clone());
                        session.olc_buffer.push(text);
                    }
                }
            }
            continue;
        }

        // Handle AI description confirmation mode
        if olc_mode.as_deref() == Some("ai_confirm") {
            let trimmed = input.trim().to_lowercase();

            if trimmed == "y" || trimmed == "yes" {
                // Accept the AI description
                let (response, target) = {
                    let mut conns = connections.lock().unwrap();
                    if let Some(session) = conns.get_mut(&connection_id) {
                        let response = session.pending_ai_response.take();
                        let target = session.pending_ai_target.take();
                        session.olc_mode = None;
                        session.pending_ai_request = None;
                        (response, target)
                    } else {
                        (None, None)
                    }
                };

                if let (Some(response), Some(target)) = (response, target) {
                    if response.success {
                        // Apply the description based on target type
                        let world = state.lock().unwrap();
                        let engine_ptr = &world.engine as *const Engine;
                        drop(world);

                        let engine = unsafe { &*engine_ptr };
                        let mut scope = rhai::Scope::new();
                        scope.push("connection_id", connection_id.to_string());

                        // Apply main description
                        if let Some(ref desc) = response.description {
                            match target.target_type {
                                claude::AiTargetType::Room => {
                                    scope.push("room_id", target.entity_id.to_string());
                                    scope.push("new_desc", desc.clone());
                                    let _ = engine
                                        .eval_with_scope::<()>(&mut scope, "set_room_description(room_id, new_desc);");
                                }
                                claude::AiTargetType::Mobile => {
                                    scope.push("mobile_id", target.entity_id.to_string());
                                    scope.push("new_desc", desc.clone());
                                    if target.field == "short_desc" {
                                        let _ = engine.eval_with_scope::<()>(
                                            &mut scope,
                                            "set_mobile_short_desc(mobile_id, new_desc);",
                                        );
                                    } else {
                                        let _ = engine.eval_with_scope::<()>(
                                            &mut scope,
                                            "set_mobile_long_desc(mobile_id, new_desc);",
                                        );
                                    }
                                }
                                claude::AiTargetType::Item => {
                                    scope.push("item_id", target.entity_id.to_string());
                                    scope.push("new_desc", desc.clone());
                                    if target.field == "short_desc" {
                                        let _ = engine.eval_with_scope::<()>(
                                            &mut scope,
                                            "set_item_short_desc(item_id, new_desc);",
                                        );
                                    } else {
                                        let _ = engine.eval_with_scope::<()>(
                                            &mut scope,
                                            "set_item_long_desc(item_id, new_desc);",
                                        );
                                    }
                                }
                            }
                        }

                        // Apply extra descriptions for rooms
                        let extra_count = response.extra_descs.len();
                        if matches!(target.target_type, claude::AiTargetType::Room) && extra_count > 0 {
                            for extra in &response.extra_descs {
                                let keywords_str = extra.keywords.join(" ");
                                scope.push("room_id", target.entity_id.to_string());
                                scope.push("keywords", keywords_str);
                                scope.push("extra_desc", extra.description.clone());
                                let _ = engine.eval_with_scope::<()>(
                                    &mut scope,
                                    "add_room_extra_desc(room_id, keywords, extra_desc);",
                                );
                            }
                            let _ = tx_client.send(format!(
                                "Description updated. {} extra description(s) added.\n",
                                extra_count
                            ));
                        } else {
                            let _ = tx_client.send("Description updated.\n".to_string());
                        }
                    }
                }
                continue;
            } else if trimmed == "n" || trimmed == "no" {
                // Reject the AI description
                {
                    let mut conns = connections.lock().unwrap();
                    if let Some(session) = conns.get_mut(&connection_id) {
                        session.pending_ai_response = None;
                        session.pending_ai_target = None;
                        session.pending_ai_request = None;
                        session.olc_mode = None;
                    }
                }
                let _ = tx_client.send("Description rejected.\n".to_string());
                continue;
            } else {
                let _ = tx_client.send("Please enter 'y' to accept or 'n' to reject.\n".to_string());
                continue;
            }
        }

        // Handle character wizard modes (from create.rhai, login.rhai, or traits.rhai)
        debug!("OLC mode check: {:?}", olc_mode);
        if let Some(ref mode) = olc_mode {
            let script_path = if mode.starts_with("chargen_") || mode == "migration_prompt" {
                // Check wizard_data to determine which script started the wizard
                let is_migration = {
                    let conns = connections.lock().unwrap();
                    conns
                        .get(&connection_id)
                        .and_then(|s| s.wizard_data.as_ref())
                        .map(|d| d.contains("migration=true"))
                        .unwrap_or(false)
                };
                if is_migration || mode == "migration_prompt" {
                    Some("scripts/commands/login.rhai".to_string())
                } else {
                    Some("scripts/commands/create.rhai".to_string())
                }
            } else if mode == "traits_select" {
                Some("scripts/commands/traits.rhai".to_string())
            } else {
                None
            };

            if let Some(script) = script_path {
                // Clone the AST and engine reference
                let (ast_opt, engine_ptr) = {
                    let world = state.lock().unwrap();
                    let ast = world.scripts.get(&script).cloned();
                    let engine_ptr = &world.engine as *const Engine;
                    (ast, engine_ptr)
                };

                if let Some(ast) = ast_opt {
                    let mut scope = rhai::Scope::new();
                    scope.set_value("args", input.clone());
                    scope.set_value("connection_id", connection_id.to_string());

                    let engine = unsafe { &*engine_ptr };
                    match engine.call_fn::<rhai::Dynamic>(
                        &mut scope,
                        &ast,
                        "run_command",
                        (input.clone(), connection_id.to_string()),
                    ) {
                        Ok(output) => {
                            if output.is_string() {
                                let msg = output.into_string().unwrap();
                                if !msg.is_empty() {
                                    let _ = tx_client.send(msg);
                                }
                            }
                        }
                        Err(e) => {
                            let _ = tx_client.send(format!("Error: {}\n", e));
                        }
                    }
                    // Send prompt after wizard command execution
                    let prompt = build_prompt(&connection_id, &connections, &state);
                    let _ = tx_client.send(prompt);
                }
                continue;
            }
        }

        // History recall: !! or !prefix (bash-style)
        let input = if input.starts_with('!') && input.len() > 1 {
            let recall_result = {
                let conns = connections.lock().unwrap();
                if let Some(session) = conns.get(&connection_id) {
                    if input == "!!" {
                        session.command_history.front().cloned()
                    } else {
                        let search = &input[1..];
                        session
                            .command_history
                            .iter()
                            .find(|cmd| cmd.starts_with(search))
                            .cloned()
                    }
                } else {
                    None
                }
            };
            match recall_result {
                Some(recalled) => {
                    let _ = tx_client.send(format!("{}\r\n", recalled));
                    recalled
                }
                None => {
                    let _ = tx_client.send(format!("{}: event not found\r\n", input));
                    continue;
                }
            }
        } else {
            input
        };

        // Parse initial input
        let parts: Vec<&str> = input.splitn(2, ' ').collect();
        let raw_command = parts[0];
        let raw_args = parts.get(1).map_or("", |s| *s);

        // Empty input - just redisplay prompt (useful for checking stats while resting)
        if raw_command.is_empty() {
            let prompt = build_prompt(&connection_id, &connections, &state);
            let _ = tx_client.send(prompt);
            continue;
        }

        // Expand alias (may return semicolon-separated commands)
        let expanded = expand_alias(&connections, connection_id, raw_command, raw_args);

        // Split by semicolon and execute each command
        for single_cmd in expanded.split(';') {
            let single_cmd = single_cmd.trim();
            if single_cmd.is_empty() {
                continue;
            }

            let cmd_parts: Vec<&str> = single_cmd.splitn(2, ' ').collect();
            let command_name = cmd_parts[0];
            let args = cmd_parts.get(1).map_or("", |s| *s);

            // Check if user is logged in
            let is_logged_in = {
                let conns = connections.lock().unwrap();
                conns
                    .get(&connection_id)
                    .map(|s| s.character.is_some())
                    .unwrap_or(false)
            };

            // Get player permissions for access control
            let (is_builder, is_admin, abbrev_enabled) = {
                let conns = connections.lock().unwrap();
                conns
                    .get(&connection_id)
                    .map(|s| {
                        let (b, a) = s
                            .character
                            .as_ref()
                            .map(|c| (c.is_builder || c.is_admin, c.is_admin))
                            .unwrap_or((false, false));
                        (b, a, s.abbrev_enabled)
                    })
                    .unwrap_or((false, false, true))
            };

            // Try to resolve command - exact match first, then prefix matching if enabled
            let (resolved_command, access_requirement) = {
                let world = state.lock().unwrap();
                let script_path = format!("scripts/commands/{}.rhai", command_name);

                // Try exact match first
                if world.scripts.contains_key(&script_path) {
                    let access = world.command_metadata.get(command_name).map(|m| m.access.clone());
                    (command_name.to_string(), access)
                } else if abbrev_enabled {
                    // Try prefix matching
                    if let Some(matched) = find_command_by_prefix(
                        command_name,
                        &world.command_metadata,
                        is_logged_in,
                        is_builder,
                        is_admin,
                    ) {
                        let access = world.command_metadata.get(&matched).map(|m| m.access.clone());
                        (matched, access)
                    } else {
                        (command_name.to_string(), None)
                    }
                } else {
                    (command_name.to_string(), None)
                }
            };

            // Check access control
            let access_allowed = if let Some(ref access) = access_requirement {
                match access.as_str() {
                    "guest" => !is_logged_in,
                    "any" => true,
                    "user" => is_logged_in,
                    "builder" => is_builder, // Now includes admins
                    "admin" => is_admin,     // Only admins
                    _ => is_logged_in,       // Default to requiring login
                }
            } else {
                is_logged_in // Unknown commands require login
            };

            if !access_allowed {
                let msg = match access_requirement.as_deref() {
                    Some("guest") => format!("Command '{}' is only available before logging in.\n", resolved_command),
                    Some("builder") => {
                        format!("Command '{}' requires builder access.\n", resolved_command)
                    }
                    Some("admin") => {
                        format!("Command '{}' requires admin access.\n", resolved_command)
                    }
                    _ if !is_logged_in => "You must log in first. Use 'login' or 'create'.\n".to_string(),
                    _ => format!("You don't have access to command '{}'.\n", resolved_command),
                };
                let _ = tx_client.send(msg);
                continue;
            }

            // Block commands while unconscious (allow only essential ones)
            if is_logged_in {
                let is_unconscious = {
                    let conns = connections.lock().unwrap();
                    conns
                        .get(&connection_id)
                        .and_then(|s| s.character.as_ref())
                        .map(|c| c.is_unconscious)
                        .unwrap_or(false)
                };
                if is_unconscious && !is_unconscious_allowed(&resolved_command) {
                    let _ = tx_client.send("You are unconscious and cannot do that.\n".to_string());
                    continue;
                }
            }

            let script_path = format!("scripts/commands/{}.rhai", resolved_command);

            // Clone the AST and engine reference outside the lock to avoid deadlock
            // Rhai functions will need to lock state themselves
            let (ast_opt, engine_ptr) = {
                let world = state.lock().unwrap();
                let ast = world.scripts.get(&script_path).cloned();
                // SAFETY: The engine lives as long as World, and we only use it while state exists
                let engine_ptr = &world.engine as *const Engine;
                (ast, engine_ptr)
            };

            let result_message = if let Some(ast) = ast_opt {
                let mut scope = rhai::Scope::new();
                scope.set_value("args", args.to_string());
                scope.set_value("connection_id", connection_id.to_string());

                // SAFETY: Engine pointer is valid because state (and World) still exists
                let engine = unsafe { &*engine_ptr };
                match engine.call_fn::<rhai::Dynamic>(
                    &mut scope,
                    &ast,
                    "run_command",
                    (args.to_string(), connection_id.to_string()),
                ) {
                    Ok(output) => {
                        if output.is_string() {
                            output.into_string().unwrap()
                        } else {
                            String::new()
                        }
                    }
                    Err(e) => format!("Error executing command {}: {}\n", resolved_command, e),
                }
            } else {
                format!("Unknown command: {}\n", command_name)
            };

            if !result_message.is_empty() {
                if let Err(e) = tx_client.send(result_message) {
                    error!("Failed to send error message to socket {}: {}", addr, e);
                    break;
                }
            }

            // Send prompt after command execution
            let prompt = build_prompt(&connection_id, &connections, &state);
            if let Err(e) = tx_client.send(prompt) {
                error!("Failed to send prompt to socket {}: {}", addr, e);
                break;
            }
        }
    }
    info!("Connection closed: {}", addr);

    // Clean up character on unexpected disconnect (TCP close without logout/quit)
    let cleanup_info = {
        let conns = connections.lock().unwrap();
        if let Some(session) = conns.get(&connection_id) {
            if let Some(ref character) = session.character {
                Some((character.clone(), character.current_room_id))
            } else {
                None
            }
        } else {
            None
        }
    };

    if let Some((character, room_id)) = cleanup_info {
        let char_name = character.name.clone();

        // Save character to database
        {
            let world = state.lock().unwrap();
            if let Err(e) = world.db.save_character_data(character) {
                error!("Failed to save {} on disconnect: {}", char_name, e);
            } else {
                info!("Saved character {} on disconnect", char_name);
            }
        }

        // Broadcast disconnect message to room
        broadcast_to_room(
            &connections,
            room_id,
            format!("{} has lost their connection.", char_name),
            Some(&char_name),
        );

        // Notify chat integrations (Matrix/Discord)
        {
            let world = state.lock().unwrap();
            if let Some(ref chat_tx) = world.chat_sender {
                let _ = chat_tx.send(chat::ChatMessage::Broadcast(format!(
                    "{} has lost their connection.",
                    char_name
                )));
            }
        }

        info!("Cleaned up disconnected player: {}", char_name);
    }

    // Remove connection from map
    {
        let mut conns = connections.lock().unwrap();
        conns.remove(&connection_id);
    }
}

/// Character-mode read handler with telnet protocol support
///
/// This handler processes input byte-by-byte, handles telnet protocol
/// negotiation, and supports TAB completion. Falls back to line mode
/// if the client doesn't support character mode.
async fn handle_read_char_mode(
    mut reader: tokio::net::tcp::OwnedReadHalf,
    addr: std::net::SocketAddr,
    connection_id: ConnectionId,
    tx_input: mpsc::UnboundedSender<InputEvent>,
    connections: SharedConnections,
    tx_raw: mpsc::UnboundedSender<Vec<u8>>,
) {
    use crate::telnet::*;

    let mut parser = TelnetParser::new();
    let mut buf = [0u8; 256];
    let mut line_buffer = String::new();

    // Send initial telnet negotiations
    let negotiations = build_initial_negotiations();
    let _ = tx_raw.send(negotiations);

    // Set a timeout for negotiation response (500ms for quick fallback to line mode)
    let negotiation_deadline = tokio::time::Instant::now() + tokio::time::Duration::from_millis(500);
    let mut negotiation_complete = false;
    let mut char_mode_enabled = false;

    loop {
        // Check if connection was removed by script (e.g., quit command)
        let is_connected = {
            let conns = connections.lock().unwrap();
            conns.contains_key(&connection_id)
        };
        if !is_connected {
            info!("Read task: connection {} removed, exiting.", addr);
            break;
        }

        // Use timeout for initial negotiation period, then periodic timeout to check disconnect
        let read_result = if !negotiation_complete && tokio::time::Instant::now() < negotiation_deadline {
            match tokio::time::timeout(
                negotiation_deadline - tokio::time::Instant::now(),
                reader.read(&mut buf),
            )
            .await
            {
                Ok(result) => result,
                Err(_) => {
                    // Negotiation timeout - check if we got char mode
                    negotiation_complete = true;
                    let is_char_mode = {
                        let conns = connections.lock().unwrap();
                        conns
                            .get(&connection_id)
                            .map(|s| s.telnet_state.char_mode)
                            .unwrap_or(false)
                    };
                    char_mode_enabled = is_char_mode;
                    if !char_mode_enabled {
                        info!("Client {} using line mode (no telnet negotiation response)", addr);
                    }
                    continue;
                }
            }
        } else {
            // Use timeout to periodically check if connection was removed
            match tokio::time::timeout(tokio::time::Duration::from_millis(100), reader.read(&mut buf)).await {
                Ok(result) => result,
                Err(_) => continue, // Timeout - loop back to check if still connected
            }
        };

        match read_result {
            Ok(0) => break, // Connection closed
            Ok(n) => {
                let (data, events) = parser.process_bytes(&buf[..n]);
                let had_events = !events.is_empty();

                // Handle telnet events
                for event in events {
                    match event {
                        TelnetEvent::Will(opt) => {
                            // Track negotiation state
                            let mut conns = connections.lock().unwrap();
                            if let Some(session) = conns.get_mut(&connection_id) {
                                match opt {
                                    OPT_SGA => {
                                        // Send DO response for SGA
                                        let _ = tx_raw.send(respond_to_will(opt));
                                        session.telnet_state.suppress_go_ahead = true;
                                        // Both sides agreeing to SGA enables char mode
                                        if session.telnet_state.echo {
                                            session.telnet_state.char_mode = true;
                                            char_mode_enabled = true;
                                        }
                                    }
                                    OPT_MXP => {
                                        // Client supports MXP - auto-enable it
                                        // Don't send DO MXP response - we already sent it in initial negotiation
                                        // Sending it again would cause an infinite loop
                                        if !session.telnet_state.mxp_supported {
                                            session.telnet_state.mxp_supported = true;
                                            session.mxp_enabled = true;
                                            // Send MXP activation sequence
                                            let _ = tx_raw.send(build_mxp_activation());
                                            debug!("MXP auto-enabled for connection {}", connection_id);
                                        }
                                    }
                                    OPT_TTYPE => {
                                        // Client supports TTYPE - start MTTS 3-stage negotiation
                                        // We already sent DO TTYPE in initial negotiation
                                        if session.telnet_state.ttype_stage == 0 {
                                            session.telnet_state.ttype_stage = 1;
                                            let _ = tx_raw.send(build_ttype_send());
                                        }
                                    }
                                    _ => {
                                        // Send standard response for other options
                                        let _ = tx_raw.send(respond_to_will(opt));
                                    }
                                }
                            }
                        }
                        TelnetEvent::Wont(opt) => {
                            // Client refused option
                            let _ = tx_raw.send(build_negotiation(DONT, opt));
                        }
                        TelnetEvent::Do(opt) => {
                            let response = respond_to_do(opt);
                            let _ = tx_raw.send(response);

                            let mut conns = connections.lock().unwrap();
                            if let Some(session) = conns.get_mut(&connection_id) {
                                match opt {
                                    OPT_ECHO => {
                                        session.telnet_state.echo = true;
                                        if session.telnet_state.suppress_go_ahead {
                                            session.telnet_state.char_mode = true;
                                            char_mode_enabled = true;
                                        }
                                    }
                                    OPT_SGA => {
                                        session.telnet_state.suppress_go_ahead = true;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        TelnetEvent::Dont(opt) => {
                            // Client refused our option
                            let _ = tx_raw.send(build_negotiation(WONT, opt));
                        }
                        TelnetEvent::Subnegotiation(opt, subneg_data) => {
                            match opt {
                                OPT_NAWS => {
                                    if let Some((width, height)) = parse_naws(&subneg_data) {
                                        let mut conns = connections.lock().unwrap();
                                        if let Some(session) = conns.get_mut(&connection_id) {
                                            session.telnet_state.window_width = width;
                                            session.telnet_state.window_height = height;
                                        }
                                    }
                                }
                                OPT_TTYPE => {
                                    // Handle TTYPE IS response (MTTS 3-stage negotiation)
                                    if let Some(ttype_str) = parse_ttype_is(&subneg_data) {
                                        let mut conns = connections.lock().unwrap();
                                        if let Some(session) = conns.get_mut(&connection_id) {
                                            let stage = session.telnet_state.ttype_stage;
                                            match stage {
                                                1 => {
                                                    // Stage 1: Client name (e.g., "MUDLET")
                                                    session.telnet_state.client_name = Some(ttype_str);
                                                    session.telnet_state.ttype_stage = 2;
                                                    let _ = tx_raw.send(build_ttype_send());
                                                }
                                                2 => {
                                                    // Stage 2: Terminal type (e.g., "XTERM-256COLOR")
                                                    session.telnet_state.terminal_type = Some(ttype_str);
                                                    session.telnet_state.ttype_stage = 3;
                                                    let _ = tx_raw.send(build_ttype_send());
                                                }
                                                3 => {
                                                    // Stage 3: MTTS bitvector or repeated value
                                                    if let Some(flags) = parse_mtts_flags(&ttype_str) {
                                                        session.telnet_state.mtts_flags = flags;
                                                        session.telnet_state.utf8_supported = (flags & MTTS_UTF8) != 0;
                                                    }
                                                    session.telnet_state.ttype_stage = 4; // Complete

                                                    // Log client capabilities
                                                    let ts = &session.telnet_state;
                                                    info!(
                                                        "Client {} capabilities - name: {:?}, type: {:?}, MTTS: {} (UTF-8: {}, ANSI: {}, 256color: {})",
                                                        connection_id,
                                                        ts.client_name,
                                                        ts.terminal_type,
                                                        ts.mtts_flags,
                                                        ts.utf8_supported,
                                                        (ts.mtts_flags & MTTS_ANSI) != 0,
                                                        (ts.mtts_flags & MTTS_256_COLORS) != 0,
                                                    );
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }

                // Mark negotiation complete after first data or if we got responses
                if !negotiation_complete && (!data.is_empty() || had_events) {
                    let is_char_mode = {
                        let conns = connections.lock().unwrap();
                        conns
                            .get(&connection_id)
                            .map(|s| s.telnet_state.char_mode)
                            .unwrap_or(false)
                    };
                    char_mode_enabled = is_char_mode;
                    negotiation_complete = true;
                }

                // Process data bytes
                for byte in data {
                    if char_mode_enabled {
                        // Character-by-character mode with escape sequence parsing
                        // Parse byte through escape sequence state machine
                        let key_event = {
                            let mut conns = connections.lock().unwrap();
                            if let Some(session) = conns.get_mut(&connection_id) {
                                let (new_state, event) = telnet::parse_key_byte(session.escape_state.clone(), byte);
                                session.escape_state = new_state;
                                event
                            } else {
                                None
                            }
                        };

                        // Handle the key event
                        if let Some(key) = key_event {
                            use telnet::KeyEvent;
                            match key {
                                KeyEvent::Tab => {
                                    let _ = tx_input.send(InputEvent::Tab);
                                }
                                KeyEvent::Enter => {
                                    // Submit line and add to history
                                    let line = {
                                        let mut conns = connections.lock().unwrap();
                                        if let Some(session) = conns.get_mut(&connection_id) {
                                            let line = session.input_buffer.trim().to_string();
                                            // Add to history if non-empty and different from last
                                            if !line.is_empty() {
                                                let should_add = session
                                                    .command_history
                                                    .front()
                                                    .map(|last| last != &line)
                                                    .unwrap_or(true);
                                                if should_add {
                                                    session.command_history.push_front(line.clone());
                                                    if session.command_history.len() > telnet::MAX_HISTORY_SIZE {
                                                        session.command_history.pop_back();
                                                    }
                                                }
                                            }
                                            session.input_buffer.clear();
                                            session.cursor_pos = 0;
                                            session.history_index = None;
                                            session.saved_input.clear();
                                            line
                                        } else {
                                            line_buffer.trim().to_string()
                                        }
                                    };
                                    line_buffer.clear();
                                    let _ = tx_raw.send(b"\r\n".to_vec());
                                    let _ = tx_input.send(InputEvent::Line(line));
                                }
                                KeyEvent::Backspace => {
                                    handle_readline_backspace(&connections, connection_id, &tx_raw);
                                }
                                KeyEvent::Delete => {
                                    handle_readline_delete(&connections, connection_id, &tx_raw);
                                }
                                KeyEvent::ArrowUp => {
                                    handle_readline_history_up(&connections, connection_id, &tx_raw);
                                }
                                KeyEvent::ArrowDown => {
                                    handle_readline_history_down(&connections, connection_id, &tx_raw);
                                }
                                KeyEvent::ArrowLeft => {
                                    handle_readline_cursor_left(&connections, connection_id, &tx_raw);
                                }
                                KeyEvent::ArrowRight => {
                                    handle_readline_cursor_right(&connections, connection_id, &tx_raw);
                                }
                                KeyEvent::Home | KeyEvent::CtrlA => {
                                    handle_readline_cursor_home(&connections, connection_id, &tx_raw);
                                }
                                KeyEvent::End | KeyEvent::CtrlE => {
                                    handle_readline_cursor_end(&connections, connection_id, &tx_raw);
                                }
                                KeyEvent::CtrlT => {
                                    handle_readline_transpose(&connections, connection_id, &tx_raw);
                                }
                                KeyEvent::CtrlW => {
                                    handle_readline_delete_word(&connections, connection_id, &tx_raw);
                                }
                                KeyEvent::CtrlU => {
                                    handle_readline_delete_to_start(&connections, connection_id, &tx_raw);
                                }
                                KeyEvent::CtrlK => {
                                    handle_readline_delete_to_end(&connections, connection_id, &tx_raw);
                                }
                                KeyEvent::CtrlC => {
                                    handle_readline_cancel_line(&connections, connection_id, &tx_raw);
                                }
                                KeyEvent::CtrlL => {
                                    handle_readline_redraw_screen(&connections, connection_id, &tx_raw);
                                }
                                KeyEvent::CtrlD => {
                                    // Logout on empty line
                                    let should_logout = {
                                        let conns = connections.lock().unwrap();
                                        conns
                                            .get(&connection_id)
                                            .map(|s| s.input_buffer.is_empty())
                                            .unwrap_or(false)
                                    };
                                    if should_logout {
                                        let _ = tx_input.send(InputEvent::Line("quit".to_string()));
                                    }
                                }
                                KeyEvent::Char(c) => {
                                    handle_readline_insert_char(&connections, connection_id, c, &tx_raw);
                                    line_buffer.push(c);
                                }
                                KeyEvent::Unknown => {
                                    // Ignore unknown keys
                                }
                            }
                        }
                    } else {
                        // Line mode - accumulate until newline
                        match byte {
                            CHAR_CR => {}
                            CHAR_LF => {
                                let line = line_buffer.trim().to_string();
                                line_buffer.clear();
                                let _ = tx_input.send(InputEvent::RawLine(line));
                            }
                            _ => {
                                if let Some(c) = char::from_u32(byte as u32) {
                                    line_buffer.push(c);
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to read from socket {}: {}", addr, e);
                break;
            }
        }
    }

    info!("Client {} disconnected (char mode read handler).", addr);
}

// New helper function to handle writing to client
// Accepts both String messages and raw byte arrays for telnet negotiation
async fn handle_write_with_raw(
    mut writer: tokio::net::tcp::OwnedWriteHalf,
    addr: std::net::SocketAddr,
    mut rx_client: mpsc::UnboundedReceiver<String>,
    mut rx_raw: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    loop {
        // `biased` drains rx_raw before rx_client so the \r\n emitted when the
        // user hits Enter can't lose a race to the command's response text and
        // end up written after it (which glued responses onto the echoed line).
        tokio::select! {
            biased;
            Some(raw) = rx_raw.recv() => {
                if let Err(e) = writer.write_all(&raw).await {
                    error!("Failed to write raw bytes to socket {}: {}", addr, e);
                    break;
                }
                let _ = writer.flush().await;
            }
            Some(msg) = rx_client.recv() => {
                // Convert \n to \r\n for proper telnet line endings
                // First normalize any existing \r\n to \n, then convert all \n to \r\n
                let telnet_msg = msg.replace("\r\n", "\n").replace("\n", "\r\n");
                if let Err(e) = writer.write_all(telnet_msg.as_bytes()).await {
                    error!("Failed to write to socket {}: {}", addr, e);
                    break;
                }
                let _ = writer.flush().await;
            }
            else => break,
        }
    }
    info!("Client {} disconnected (write handler).", addr);
}

// ============================================================================
// Readline-like input handling helper functions
// ============================================================================

fn handle_readline_backspace(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        if session.cursor_pos > 0 {
            let char_count = session.input_buffer.chars().count();

            if session.cursor_pos >= char_count {
                // Cursor at end - simple case, but handle variable width chars
                if let Some(ch) = session.input_buffer.pop() {
                    let char_width = telnet::display_width(&ch.to_string());
                    session.cursor_pos -= 1;
                    // Move left, overwrite with spaces, move left again
                    let mut output = Vec::new();
                    output.extend(telnet::ansi::cursor_left(char_width));
                    output.extend(vec![b' '; char_width]);
                    output.extend(telnet::ansi::cursor_left(char_width));
                    let _ = tx_raw.send(output);
                }
            } else {
                // Cursor in middle - need to redraw
                let chars: Vec<char> = session.input_buffer.chars().collect();
                let remove_idx = session.cursor_pos - 1;
                let mut new_buffer = String::new();
                for (i, c) in chars.iter().enumerate() {
                    if i != remove_idx {
                        new_buffer.push(*c);
                    }
                }
                session.input_buffer = new_buffer;
                session.cursor_pos -= 1;

                // Redraw from cursor position
                let output = telnet::redraw_input_line("> ", &session.input_buffer, session.cursor_pos);
                let _ = tx_raw.send(output);
            }
            // Reset history navigation on edit
            session.history_index = None;
        }
    }
}

fn handle_readline_delete(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        let char_count = session.input_buffer.chars().count();
        if session.cursor_pos < char_count {
            // Remove character at cursor position
            let chars: Vec<char> = session.input_buffer.chars().collect();
            let mut new_buffer = String::new();
            for (i, c) in chars.iter().enumerate() {
                if i != session.cursor_pos {
                    new_buffer.push(*c);
                }
            }
            session.input_buffer = new_buffer;

            // Redraw from cursor position
            let output = telnet::redraw_input_line("> ", &session.input_buffer, session.cursor_pos);
            let _ = tx_raw.send(output);

            // Reset history navigation on edit
            session.history_index = None;
        }
    }
}

fn handle_readline_cursor_left(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        if session.cursor_pos > 0 {
            // Get the character we're moving over and its display width
            if let Some(ch) = session.input_buffer.chars().nth(session.cursor_pos - 1) {
                let char_width = telnet::display_width(&ch.to_string());
                session.cursor_pos -= 1;
                let _ = tx_raw.send(telnet::ansi::cursor_left(char_width));
            }
        }
    }
}

fn handle_readline_cursor_right(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        let char_count = session.input_buffer.chars().count();
        if session.cursor_pos < char_count {
            // Get the character we're moving over and its display width
            if let Some(ch) = session.input_buffer.chars().nth(session.cursor_pos) {
                let char_width = telnet::display_width(&ch.to_string());
                session.cursor_pos += 1;
                let _ = tx_raw.send(telnet::ansi::cursor_right(char_width));
            }
        }
    }
}

fn handle_readline_cursor_home(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        if session.cursor_pos > 0 {
            // Calculate display width of characters from start to cursor
            let cols_to_move = telnet::display_width_up_to(&session.input_buffer, session.cursor_pos);
            let _ = tx_raw.send(telnet::ansi::cursor_left(cols_to_move));
            session.cursor_pos = 0;
        }
    }
}

fn handle_readline_cursor_end(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        let char_count = session.input_buffer.chars().count();
        if session.cursor_pos < char_count {
            // Calculate display width of characters from cursor to end
            let chars_after: String = session.input_buffer.chars().skip(session.cursor_pos).collect();
            let cols_to_move = telnet::display_width(&chars_after);
            let _ = tx_raw.send(telnet::ansi::cursor_right(cols_to_move));
            session.cursor_pos = char_count;
        }
    }
}

fn handle_readline_history_up(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        if session.command_history.is_empty() {
            return;
        }

        let new_index = match session.history_index {
            None => {
                // Save current input before navigating
                session.saved_input = session.input_buffer.clone();
                Some(0)
            }
            Some(idx) if idx + 1 < session.command_history.len() => Some(idx + 1),
            Some(_) => return, // Already at oldest
        };

        if let Some(idx) = new_index {
            session.history_index = Some(idx);
            let history_line = session.command_history[idx].clone();

            // Clear current line and display history entry
            let output = telnet::redraw_input_line("> ", &history_line, history_line.chars().count());
            session.input_buffer = history_line;
            session.cursor_pos = session.input_buffer.chars().count();
            let _ = tx_raw.send(output);
        }
    }
}

fn handle_readline_history_down(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        match session.history_index {
            None => return, // Already at newest
            Some(0) => {
                // Return to saved input
                session.history_index = None;
                let restored = session.saved_input.clone();
                let output = telnet::redraw_input_line("> ", &restored, restored.chars().count());
                session.input_buffer = restored;
                session.cursor_pos = session.input_buffer.chars().count();
                let _ = tx_raw.send(output);
            }
            Some(idx) => {
                session.history_index = Some(idx - 1);
                let history_line = session.command_history[idx - 1].clone();
                let output = telnet::redraw_input_line("> ", &history_line, history_line.chars().count());
                session.input_buffer = history_line;
                session.cursor_pos = session.input_buffer.chars().count();
                let _ = tx_raw.send(output);
            }
        }
    }
}

fn handle_readline_transpose(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        let chars: Vec<char> = session.input_buffer.chars().collect();
        let len = chars.len();

        // Need at least 2 characters, and cursor must not be at position 0
        if len < 2 || session.cursor_pos == 0 {
            let _ = tx_raw.send(telnet::ansi::bell());
            return;
        }

        let (a, b) = if session.cursor_pos >= len {
            // At end of line: swap last two characters
            (len - 2, len - 1)
        } else {
            // Mid-line: swap char before cursor with char at cursor, advance cursor
            (session.cursor_pos - 1, session.cursor_pos)
        };

        let mut new_chars = chars;
        new_chars.swap(a, b);
        session.input_buffer = new_chars.into_iter().collect();

        // Advance cursor (unless already at end)
        if session.cursor_pos < len {
            session.cursor_pos += 1;
        }

        let output = telnet::redraw_input_line("> ", &session.input_buffer, session.cursor_pos);
        let _ = tx_raw.send(output);

        // Reset history navigation on edit
        session.history_index = None;
    }
}

fn handle_readline_delete_word(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        if session.cursor_pos == 0 {
            return;
        }

        // Find word boundary (skip trailing spaces, then skip word chars)
        let chars: Vec<char> = session.input_buffer.chars().collect();
        let mut new_pos = session.cursor_pos;

        // Skip spaces backward
        while new_pos > 0 && chars[new_pos - 1].is_whitespace() {
            new_pos -= 1;
        }
        // Skip word backward
        while new_pos > 0 && !chars[new_pos - 1].is_whitespace() {
            new_pos -= 1;
        }

        if new_pos < session.cursor_pos {
            // Remove characters from new_pos to cursor_pos
            let mut new_buffer = String::new();
            for (i, c) in chars.iter().enumerate() {
                if i < new_pos || i >= session.cursor_pos {
                    new_buffer.push(*c);
                }
            }
            session.input_buffer = new_buffer;
            session.cursor_pos = new_pos;

            // Redraw line
            let output = telnet::redraw_input_line("> ", &session.input_buffer, session.cursor_pos);
            let _ = tx_raw.send(output);

            // Reset history navigation on edit
            session.history_index = None;
        }
    }
}

fn handle_readline_delete_to_start(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        if session.cursor_pos > 0 {
            let chars: Vec<char> = session.input_buffer.chars().collect();
            session.input_buffer = chars[session.cursor_pos..].iter().collect();
            session.cursor_pos = 0;

            let output = telnet::redraw_input_line("> ", &session.input_buffer, 0);
            let _ = tx_raw.send(output);

            // Reset history navigation on edit
            session.history_index = None;
        }
    }
}

fn handle_readline_delete_to_end(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        let char_count = session.input_buffer.chars().count();
        if session.cursor_pos < char_count {
            let chars: Vec<char> = session.input_buffer.chars().collect();
            session.input_buffer = chars[..session.cursor_pos].iter().collect();

            // Clear to end of line
            let _ = tx_raw.send(telnet::ansi::clear_to_eol());

            // Reset history navigation on edit
            session.history_index = None;
        }
    }
}

fn handle_readline_cancel_line(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        // Display ^C and start new line
        let _ = tx_raw.send(b"^C\r\n> ".to_vec());
        session.input_buffer.clear();
        session.cursor_pos = 0;
        session.history_index = None;
        session.saved_input.clear();
    }
}

fn handle_readline_redraw_screen(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let conns = connections.lock().unwrap();
    if let Some(session) = conns.get(&connection_id) {
        // Clear screen and redraw
        let mut output = b"\x1b[2J\x1b[H".to_vec(); // Clear screen, home cursor
        output.extend(telnet::redraw_input_line(
            "> ",
            &session.input_buffer,
            session.cursor_pos,
        ));
        let _ = tx_raw.send(output);
    }
}

fn handle_readline_insert_char(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    c: char,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        let char_count = session.input_buffer.chars().count();

        if session.cursor_pos >= char_count {
            // Append at end (simple case)
            session.input_buffer.push(c);
            session.cursor_pos += 1;
            let mut buf = [0u8; 4];
            let bytes = c.encode_utf8(&mut buf);
            let _ = tx_raw.send(bytes.as_bytes().to_vec());
        } else {
            // Insert in middle
            let chars: Vec<char> = session.input_buffer.chars().collect();
            let mut new_buffer = String::new();
            for (i, ch) in chars.iter().enumerate() {
                if i == session.cursor_pos {
                    new_buffer.push(c);
                }
                new_buffer.push(*ch);
            }
            session.input_buffer = new_buffer;
            session.cursor_pos += 1;

            // Redraw from insertion point
            let output = telnet::redraw_input_line("> ", &session.input_buffer, session.cursor_pos);
            let _ = tx_raw.send(output);
        }

        // Reset history navigation on edit
        session.history_index = None;
    }
}

pub async fn run_server(state: SharedState, listener: TcpListener, shutdown_rx: tokio::sync::oneshot::Receiver<()>) {
    tokio::select! {
        _ = async {
            loop {
                let (socket, addr) = listener.accept().await.unwrap();
                info!("New connection: {}", addr);

                // Cloud NATs (e.g. GCP VPC) silently drop idle TCP flows after ~10 min.
                // Keepalive probes every 60s keep long-lived telnet sessions alive.
                let ka = socket2::TcpKeepalive::new()
                    .with_time(std::time::Duration::from_secs(60))
                    .with_interval(std::time::Duration::from_secs(30));
                if let Err(e) = socket2::SockRef::from(&socket).set_tcp_keepalive(&ka) {
                    warn!("Failed to set TCP keepalive on {}: {}", addr, e);
                }

                let state = state.clone();
                let connection_id = Uuid::new_v4();
                tokio::spawn(async move {
                    handle_connection(socket, addr, state, connection_id).await;
                });
            }
        } => {},
        _ = shutdown_rx => {
            info!("Shutting down server...");
        }
    }
}

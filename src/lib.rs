// matrix-sdk has deeply nested async futures that overflow clippy's default
// query depth limit. 512 is comfortable headroom.
#![recursion_limit = "512"]

use parking_lot::Mutex;
use rhai::{AST, Engine};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

pub mod aging;
pub mod ai_guard;
pub mod api;
pub mod api_key_auth;
pub mod attribution;
pub mod audit;
pub mod bounty;
pub mod build_score;
pub mod build_tracks;
pub mod chat;
pub mod claude;
pub mod combat_text;
pub mod completion;
mod conn_input;
mod conn_read;
pub mod consignment;
pub mod control;
pub mod corpse;
pub mod cyberware;
pub mod db;
pub mod dialogue_edit;
#[cfg(feature = "discord")]
pub mod discord;
pub mod editor;
pub mod email;
pub mod experiment;
pub mod game;
pub mod gemini;
pub mod group;
pub mod import;
pub mod init;
pub mod leaderboard;
#[cfg(feature = "matrix")]
pub mod matrix;
pub mod migration;
pub mod morality;
pub mod mutant;
pub mod necromancy;
pub mod player_log;
pub mod progress;
pub mod prompt;
pub mod purse;
pub mod quest;
pub mod ratelimit;
pub mod readline;
pub mod replicant;
pub mod reputation;
pub mod script;
pub mod seed;
pub mod session;
pub mod settings;
pub mod social;
pub mod spawn;
pub mod synth;
pub mod telnet;
pub mod text;
pub mod throttle;
pub mod tiers;
pub mod types;
pub mod vampire;
pub mod werewolf;
pub mod world_milestones;
pub mod world_rating;
pub mod worship;

pub use types::*;

// Re-export session and init functions for backward compatibility
pub use init::{
    load_command_metadata, load_game_data, load_scripts, register_socials_in_command_metadata, watch_scripts,
};
use readline::*;
pub use script::check_build_mode;
pub use session::{
    break_all_charms_by_player, broadcast_to_all_players, broadcast_to_builders, broadcast_to_outdoor_players,
    broadcast_to_room, broadcast_to_room_awake, broadcast_to_room_dreaming, clear_player_character, disconnect_client,
    find_player_connection_by_name, get_character_for_connection, get_characters_in_room,
    get_characters_in_room_with_positions, send_client_message, set_character_for_connection,
};

// Starting room UUID constant
pub const STARTING_ROOM_ID: &str = "00000000-0000-0000-0000-000000000001";

/// Hard cap on the byte length of a single command line. The Rhai engine
/// caps strings at 1 MiB only after a line is dispatched; without a cap on
/// the inbound buffer itself, an unauthenticated client can stream gigabytes
/// of non-newline bytes and exhaust RAM before any newline arrives.
pub const MAX_INPUT_LINE: usize = 8 * 1024;

/// Bound on the per-session `InputEvent` channel. 64 in-flight commands is
/// roomy for any human typist; a scripted client that fills it triggers a
/// "slow down" message and a dropped line rather than unbounded queueing.
pub const INPUT_CHANNEL_CAPACITY: usize = 64;

/// Hard cap on description-like text fields written through Rhai-registered
/// setters (long_desc, short_desc, room description, extra descs, item note
/// content). Mirrors the existing `MAX_NOTE_BYTES` already enforced on the
/// REST and OLC text-collection paths.
pub const MAX_DESC_BYTES: usize = 32 * 1024;

/// Idle disconnect threshold for a connected session, in seconds. Updated
/// each time a command is dispatched (`session.last_activity_time`); the
/// read loop checks it on every poll and terminates inactive sessions to
/// reclaim per-session memory and file descriptors.
pub const IDLE_DISCONNECT_SECS: i64 = 1800;
/// How often an otherwise silent connection's read task wakes to run the
/// idle check.
const IDLE_CHECK_SECS: u64 = 30;

/// Hard cap on total characters in the database. Stops account-flood DoS
/// from filling sled with junk accounts. Five thousand is comfortably
/// above any realistic active player population for a single shard.
pub const MAX_CHARACTERS: i64 = 5000;

// Session types that depend on tokio/runtime
pub type ConnectionId = Uuid;

pub struct PlayerSession {
    pub character: Option<CharacterData>,
    pub sender: session::ClientSender,
    pub raw_sender: Option<mpsc::UnboundedSender<Vec<u8>>>,
    pub input_sender: mpsc::Sender<InputEvent>,
    pub addr: std::net::SocketAddr,
    // OLC (Online Creation) fields
    pub olc_mode: Option<String>,
    pub olc_buffer: Vec<String>,
    pub olc_edit_room: Option<Uuid>,
    pub olc_edit_item: Option<Uuid>,
    /// Bulletin board destination vnum while collecting a multi-line post.
    /// Set by `start_board_post` (called from `board write`); the saved
    /// post is committed via `add_board_post` on `.save`.
    pub olc_edit_board_vnum: Option<String>,
    /// Subject line for the in-progress bulletin board post.
    pub olc_board_subject: Option<String>,
    /// DG trigger editing — host mobile when editing a mob's trigger.
    /// Used together with `olc_edit_trigger_index` and `olc_edit_trigger_host`
    /// to identify which trigger to write the edited body to.
    pub olc_edit_mobile: Option<Uuid>,
    /// `"mobile" | "item" | "room"` — which entity kind owns the trigger
    /// being edited. The matching `olc_edit_*` field carries the id.
    pub olc_edit_trigger_host: Option<String>,
    /// Index into the host's `triggers` Vec.
    pub olc_edit_trigger_index: Option<usize>,
    /// DG trigger prototype vnum being edited in `collecting_dg_proto_body`
    /// mode. When set, `.save` routes the buffered body through
    /// `dg_proto_save_body` (parse-check, persist, refresh sweep) instead
    /// of the per-trigger save path.
    pub olc_edit_proto_vnum: Option<String>,
    /// When `olc_mode == "collecting_dialogue_node_text"`, names the
    /// dialogue tree node whose `text` field receives the buffered content.
    /// `olc_edit_mobile` carries the mobile id.
    pub olc_dialogue_node_name: Option<String>,
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
    /// Grade of the thing this builder is about to change, taken just before
    /// the change. Drained after the command finishes, so the editor can say
    /// what moved. See `crate::audit::scan::grade_change_line`.
    pub pending_audit: Option<crate::audit::scan::PendingAudit>,
    /// Skill XP accumulated since the last prompt render, keyed by skill.
    /// Populated by `progress::report_xp` in `brief` feed mode and drained by
    /// `build_prompt`, so a burst of small awards (a combat round awards 10
    /// XP per hit) collapses into one line instead of spamming the scroll.
    /// BTreeMap for stable ordering across renders.
    pub xp_buffer: std::collections::BTreeMap<String, progress::XpTally>,
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
    // ASCII map: legend printed once per session, suppressed thereafter.
    // Resets to false on disconnect (session reconstruction).
    pub map_legend_shown: bool,
    // Dialogue tree: sticky-mode active partner mob (transient, not persisted).
    // `Some(mob_id)` means inputs route through dialogue; `None` is normal command parsing.
    pub dialogue_partner_id: Option<Uuid>,
    // Account auth: Some after the user supplies a valid account name + password.
    // Cleared on disconnect. Until a character is selected, `character` stays
    // None — both fields are populated together once the player picks from the
    // roster (or auto-selects when only 1 character exists).
    pub account_id: Option<Uuid>,
    pub account_name: Option<String>,
    /// One-shot flag: when set, the next `go <dir>` command bypasses the
    /// exit-delay check. The slow-move tick sets this before injecting a
    /// completion command; `go.rhai` reads-and-clears it on entry. Not
    /// persisted — purely in-flight movement state.
    pub slow_move_completing: bool,
    /// Unix timestamp when the connection was lost. If `Some`, the player is
    /// in the Linkdead grace period. `None` means the connection is active.
    pub disconnected_at: Option<i64>,
    /// Unix timestamp when the player's character entered the world for the
    /// current session. `Some` while a character is in-play; `None` before
    /// character select and after `flush_play_time` runs on quit/disconnect.
    /// Drives the `CharacterData.total_seconds_played` accumulator.
    pub session_started_at: Option<i64>,
    /// Active modern multi-line editor instance. Present only while the
    /// player is composing inside the new editor; mutually exclusive with
    /// `olc_mode == Some(...)` routing through the legacy line editor
    /// (we keep `olc_mode` set during a modern session so cleanup and
    /// force-save paths still recognise the editor state).
    pub modern_editor: Option<crate::editor::EditorSession>,
    /// Dropped with the session: when anything removes this session from
    /// `SharedConnections` (quit, kick, linkdead cleanup), the sender drops
    /// and the connection's read task wakes and exits at once, instead of
    /// polling the map.
    pub removed_signal: Option<tokio::sync::oneshot::Sender<()>>,
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
    /// Modern editor user saved — content is the joined buffer to commit.
    ModernEditorSave(String),
    /// Modern editor user cancelled — discard the buffer.
    ModernEditorCancel,
    /// Read task is ending (EOF, idle, error, oversized line, or unwind).
    /// Sentinel so the main loop breaks and runs linkdead cleanup even though
    /// the session's `input_sender` clone keeps the channel otherwise open.
    Disconnect,
}

pub type SharedConnections = Arc<Mutex<HashMap<ConnectionId, PlayerSession>>>;
pub type SharedState = Arc<Mutex<World>>;

/// Minimum length of a newly chosen password (create, `password`, the
/// admin CLI). Existing shorter passwords keep working.
pub const MIN_PASSWORD_LEN: usize = 8;

pub struct World {
    /// Shared so command handlers can hold it after releasing the World
    /// lock. Mutate only during startup, through [`World::engine_mut`].
    pub engine: Arc<Engine>,
    pub db: crate::db::Db,
    pub connections: SharedConnections,
    pub scripts: HashMap<String, AST>,
    pub command_metadata: HashMap<String, CommandMeta>,
    /// CircleMUD-style social commands (`wave`, `bow`, `smile`, …). Loaded
    /// once at startup from `scripts/data/socials.json`; populated as
    /// virtual entries in `command_metadata` so prefix matching, access
    /// control, and tab completion all treat socials like normal commands.
    /// See `src/social/actions.rs`.
    pub socials: crate::social::actions::SocialRegistry,
    // Character creation data (loaded from scripts/data/*.json)
    pub class_definitions: HashMap<String, ClassDefinition>,
    pub trait_definitions: HashMap<String, TraitDefinition>,
    pub race_suggestions: Vec<RaceSuggestion>,
    pub race_definitions: HashMap<String, RaceDefinition>,
    // Mutation definitions (loaded from scripts/data/mutations.json).
    // Owned per-character via MutantState.mutations.
    pub mutation_definitions: HashMap<String, MutationDefinition>,
    // Language definitions (loaded from scripts/data/languages_*.json)
    pub language_definitions: HashMap<String, LanguageDefinition>,
    /// Faction definitions (loaded from `scripts/data/factions.json`), keyed
    /// by the same lowercase string builders put in `MobileData.faction`.
    /// A tag with no entry here still works — see
    /// `reputation::FactionDefinition::unregistered`.
    pub faction_definitions: HashMap<String, crate::reputation::FactionDefinition>,
    // Crafting/cooking recipes (created via recedit command)
    pub recipes: HashMap<String, Recipe>,
    // Spell definitions (loaded from scripts/data/spells_*.json)
    pub spell_definitions: HashMap<String, SpellDefinition>,
    // Achievement definitions (loaded from scripts/data/achievements/*.json
    // and the sled `achievements` tree). Indexed by stable achievement key.
    pub achievement_definitions: HashMap<String, AchievementDef>,
    // Index from counter key -> list of achievement keys whose criterion
    // is a Counter on that key. Built once at load; rebuilt on hot reload
    // if/when reload is wired. Used to make `notify_achievement_counter`
    // O(matching) instead of O(all defs).
    pub achievement_index_by_counter: HashMap<String, Vec<String>>,
    // Builder-published custom skill metadata (loaded from sled tree on
    // boot, mutated by `lookup skill publish/unpublish`). Per-entity values
    // live on CharacterData/MobileData, not here.
    pub custom_skill_definitions: HashMap<String, CustomSkillDefinition>,
    // Transportation system (elevators, buses, trains, etc.)
    pub transports: HashMap<Uuid, TransportData>,
    // Chat integration sender (for disconnect notifications to Matrix/Discord)
    pub chat_sender: Option<chat::ChatSender>,
    // Shutdown command sender (for admin shutdown command)
    pub shutdown_sender: Option<tokio::sync::mpsc::UnboundedSender<ShutdownCommand>>,
    // Shutdown cancellation sender (to abort pending shutdown)
    pub shutdown_cancel_sender: Option<tokio::sync::watch::Sender<bool>>,
    /// Per-IP connection / failed-auth rate limiter. Lives behind its own
    /// internal mutex so callers don't have to take the World lock to
    /// check a rate limit on every accepted socket.
    pub ip_limiter: Arc<ratelimit::IpRateLimiter>,
    /// Per-character per-command cooldown table. Used by wide-blast chat
    /// commands (`shout`, `tell`) to suppress spam without throttling
    /// the rest of the game.
    pub command_throttle: Arc<throttle::CommandThrottle>,
    /// Last leaderboard scan. Written wholesale by the leaderboard tick and
    /// read by `top`; empty until the first scan completes, which is the only
    /// state a reader has to handle. Cached rather than computed on demand
    /// because ranking needs every character in the database and that must
    /// never run on a player's command.
    pub leaderboards: crate::leaderboard::Leaderboards,
    /// Last builder-score scan. Same shape and same reasoning as
    /// `leaderboards`: a full-world read installed wholesale by a tick and
    /// read by the `build` command. `generated_at == 0` means the first scan
    /// has not landed, which readers must render as "not ready" rather than
    /// "nobody has built anything".
    pub build_scores: crate::build_score::BuildScores,
    /// Last world rating. Installed by the same tick as `build_scores`, for
    /// the same reason: computing it needs a full world scan.
    pub world_report: crate::world_rating::WorldReport,
    /// Builder progress tracks, loaded from `scripts/data/build_tracks/`.
    /// Definitions only — progress is evaluated per area or per builder on
    /// demand, because a track is a question about content, not a stored fact.
    pub build_tracks: Vec<crate::build_tracks::TrackDef>,
    /// Room-derived audit context, refreshed by the build-score tick.
    ///
    /// Exists for the OLC grade toast. Building it means deserialising every
    /// room in the world and hashing every description, and the toast fires on
    /// *every* mutating editor subcommand — twice, once for the before and once
    /// for the after. On an imported world that is the most expensive thing in
    /// the interactive path, paid at the speed a builder types.
    ///
    /// Reusing a cached one costs at most five minutes of staleness in the two
    /// checks that read it (dangling and one-way exits). That is the right
    /// trade: the toast is a nudge, and `build audit` is the authority.
    pub audit_ctx: std::sync::Arc<crate::audit::AuditCtx>,
}

impl World {
    /// Mutable access to the scripting engine, for registering functions at
    /// startup. Panics once any handler holds a clone (i.e. after the server
    /// starts taking commands): all registration must happen before that.
    pub fn engine_mut(&mut self) -> &mut Engine {
        Arc::get_mut(&mut self.engine)
            .expect("Rhai engine mutated after it was shared; register functions before serving")
    }

    /// A `World` with the two handles that matter and every definition table
    /// empty. For callers that need something to put behind a [`SharedState`]
    /// but read nothing out of it — the tick tests, and `ironmud-admin`, which
    /// drives the db directly and never loads game data.
    ///
    /// Exists so the ~25-field literal lives in one place: adding a field to
    /// `World` should not break every test module that happens to need a lock
    /// to hand to a function. Callers that *do* need a populated table should
    /// build one and assign it after construction.
    ///
    /// Deliberately not `#[cfg(test)]`: the bin crate and the integration
    /// tests compile against the lib without `cfg(test)`, so a test-gated
    /// constructor would be invisible to exactly the callers that duplicate
    /// this literal most.
    pub fn minimal(db: crate::db::Db, connections: SharedConnections) -> World {
        World {
            build_tracks: Vec::new(),
            world_report: Default::default(),
            build_scores: Default::default(),
            audit_ctx: Default::default(),
            engine: Arc::new(Engine::new()),
            db,
            connections,
            scripts: HashMap::new(),
            command_metadata: HashMap::new(),
            socials: crate::social::actions::SocialRegistry::default(),
            class_definitions: HashMap::new(),
            trait_definitions: HashMap::new(),
            race_suggestions: Vec::new(),
            race_definitions: HashMap::new(),
            mutation_definitions: HashMap::new(),
            language_definitions: HashMap::new(),
            faction_definitions: HashMap::new(),
            recipes: HashMap::new(),
            spell_definitions: HashMap::new(),
            achievement_definitions: HashMap::new(),
            achievement_index_by_counter: HashMap::new(),
            custom_skill_definitions: HashMap::new(),
            transports: HashMap::new(),
            chat_sender: None,
            shutdown_sender: None,
            shutdown_cancel_sender: None,
            ip_limiter: Arc::new(ratelimit::IpRateLimiter::new()),
            command_throttle: Arc::new(throttle::CommandThrottle::new()),
            leaderboards: crate::leaderboard::Leaderboards::default(),
        }
    }

    /// [`World::minimal`] wrapped in the `Arc<Mutex<_>>` every caller actually
    /// wants.
    pub fn minimal_shared(db: crate::db::Db, connections: SharedConnections) -> SharedState {
        Arc::new(Mutex::new(World::minimal(db, connections)))
    }
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
            let players_in_room: Vec<(Uuid, session::ClientSender)> = {
                let conns = connections.lock();
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
            let script_content = match crate::script::trigger_sandbox::read_trigger_script(&trigger.script_name) {
                Ok(content) => content,
                Err(_) => continue,
            };

            // Create a minimal Rhai engine for trigger execution
            let mut trigger_engine = crate::script::trigger_sandbox::sandboxed_engine();

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
                    let conns = conns_clone.lock();
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

                    let _ = crate::script::trigger_sandbox::guard_panic(|| {
                        trigger_engine.call_fn::<rhai::Dynamic>(
                            &mut scope,
                            &ast,
                            "run_trigger",
                            (room_id_str, conn_id_str, rhai_context),
                        )
                    });
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
        let conns = connections.lock();
        for (_, session) in conns.iter() {
            if let Some(ref char_data) = session.character {
                if char_data.current_room_id == *room_id {
                    let _ = session.sender.send(format!("{}\n", msg));
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
    let conns = connections.lock();
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
    let conns = connections.lock();
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
    defaults.insert("ct".to_string(), "clanchat".to_string());
    defaults
}

fn resolve_alias(connections: &SharedConnections, connection_id: ConnectionId, alias: &str) -> Option<String> {
    let conns = connections.lock();
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

/// True for OLC modes that own a multi-line text buffer. `ai_confirm` is an
/// olc_mode value too but represents a yes/no prompt with no buffer — it is
/// intentionally excluded so the writing tag and broadcast mute don't apply.
pub fn is_text_editor_mode(mode: &str) -> bool {
    matches!(
        mode,
        "collecting_desc"
            | "collecting_long"
            | "collecting_note"
            | "collecting_board_post"
            | "collecting_dg_body"
            | "collecting_dg_proto_body"
            | "collecting_dialogue_node_text"
            | "collecting_extra_desc"
            | "collecting_motd"
    )
}

/// True when the session is composing in the multi-line editor and should
/// therefore be deafened to ambient/room broadcasts.
pub fn session_is_writing(session: &PlayerSession) -> bool {
    if session.modern_editor.is_some() {
        return true;
    }
    session.olc_mode.as_deref().map(is_text_editor_mode).unwrap_or(false)
}

/// Persist `body` onto DG trigger proto `vnum` and refresh all attached
/// instances. Returns the user-facing message to print after the save
/// (success report with refresh count + warnings, parse-error abort, or
/// unknown-vnum). Shared by both the line-mode and modern-editor
/// `collecting_dg_proto_body` save paths.
pub fn save_dg_proto_body_msg(db: &crate::db::Db, vnum: &str, body: String, author: &str) -> String {
    let mut proto = match db.get_dg_trigger_proto(vnum) {
        Ok(Some(p)) => p,
        _ => return format!("Proto save failed: unknown vnum '{}'.\n", vnum),
    };
    if let Err(why) = crate::script::dg::check_proto_edit(db, vnum, author, false) {
        return format!("Proto save refused: {why}.\n(0 instances changed)\n");
    }
    proto.body = body;
    proto.stamp_edit(author);
    match db.save_dg_trigger_proto_with_refresh(&proto) {
        Ok((refreshed, warnings)) => {
            let mut msg = format!(
                "Proto '{}' saved ({} attached instance{} refreshed).\n",
                if proto.name.is_empty() {
                    proto.vnum.as_str()
                } else {
                    proto.name.as_str()
                },
                refreshed,
                if refreshed == 1 { "" } else { "s" },
            );
            for w in warnings {
                msg.push_str(&format!("  warning: {}\n", w));
            }
            msg
        }
        Err(e) => format!("Proto save refused: {}\n(0 instances changed)\n", e),
    }
}

/// Clear every editor-related slot on a session.
fn clear_olc_state(session: &mut PlayerSession) {
    session.olc_mode = None;
    session.olc_buffer.clear();
    session.olc_edit_room = None;
    session.olc_edit_item = None;
    session.olc_edit_mobile = None;
    session.olc_edit_board_vnum = None;
    session.olc_board_subject = None;
    session.olc_edit_trigger_host = None;
    session.olc_edit_trigger_index = None;
    session.olc_edit_proto_vnum = None;
    session.olc_dialogue_node_name = None;
    session.olc_extra_keywords.clear();
    session.olc_undo_buffer = None;
    session.modern_editor = None;
}

/// True if the session is composing via the modern editor.
pub fn session_in_modern_editor(session: &PlayerSession) -> bool {
    session.modern_editor.is_some()
}

/// Emergency-save the in-progress editor buffer to its target entity, then
/// clear all OLC state on the session. Returns `true` if a save was attempted
/// (regardless of whether the underlying DB write succeeded), `false` if the
/// session was not in a text-editor mode.
///
/// Used for combat interrupts, quit/disconnect, and other paths that need to
/// rip the player out of the editor without losing buffered work. Buffers over
/// their per-mode size cap are truncated rather than refused — the alternative
/// is silent data loss.
pub fn force_save_editor(connection_id: ConnectionId, connections: &SharedConnections, state: &SharedState) -> bool {
    struct Snapshot {
        mode: String,
        content: String,
        truncated: bool,
        edit_room: Option<Uuid>,
        edit_item: Option<Uuid>,
        edit_mobile: Option<Uuid>,
        board_vnum: Option<String>,
        board_subject: Option<String>,
        trigger_host: Option<String>,
        trigger_index: Option<usize>,
        proto_vnum: Option<String>,
        dialogue_node_name: Option<String>,
        extra_keywords: Vec<String>,
        author: String,
        author_is_admin: bool,
        sender: session::ClientSender,
    }

    let snap: Option<Snapshot> = {
        let conns = connections.lock();
        let session = match conns.get(&connection_id) {
            Some(s) => s,
            None => return false,
        };
        let mode = match session.olc_mode.as_deref() {
            Some(m) if is_text_editor_mode(m) => m.to_string(),
            _ => return false,
        };
        let raw = session.olc_buffer.join("\n");
        let cap = match mode.as_str() {
            "collecting_desc" => crate::api::validate::DESCRIPTION_MAX,
            // 32 KB matches the inline `.save` arms in handle_connection.
            _ => 32 * 1024,
        };
        // The cap is a storage limit, so it counts bytes — but it still has to
        // land between characters, or a description ending on an em dash
        // panics the save it was supposed to shorten.
        let (content, truncated) = if raw.len() > cap {
            (crate::text::truncate_bytes(&raw, cap).to_string(), true)
        } else {
            (raw, false)
        };
        let (author, author_is_admin) = session
            .character
            .as_ref()
            .map(|c| (c.name.clone(), c.is_admin))
            .unwrap_or((String::new(), false));
        Some(Snapshot {
            mode,
            content,
            truncated,
            edit_room: session.olc_edit_room,
            edit_item: session.olc_edit_item,
            edit_mobile: session.olc_edit_mobile,
            board_vnum: session.olc_edit_board_vnum.clone(),
            board_subject: session.olc_board_subject.clone(),
            trigger_host: session.olc_edit_trigger_host.clone(),
            trigger_index: session.olc_edit_trigger_index,
            proto_vnum: session.olc_edit_proto_vnum.clone(),
            dialogue_node_name: session.olc_dialogue_node_name.clone(),
            extra_keywords: session.olc_extra_keywords.clone(),
            author,
            author_is_admin,
            sender: session.sender.clone(),
        })
    };

    let snap = match snap {
        Some(s) => s,
        None => return false,
    };

    if snap.truncated {
        let _ = snap.sender.send(format!(
            "Your work was truncated to {} bytes before saving.\n",
            snap.content.len()
        ));
    }

    // Per-mode save. State lock acquired only here; never held across the
    // connections lock taken at the bottom.
    match snap.mode.as_str() {
        "collecting_desc" => {
            if let Some(room_id) = snap.edit_room {
                {
                    let world = state.lock();
                    if let Ok(Some(mut room)) = world.db.get_room_data(&room_id) {
                        room.description = snap.content;
                        let _ = world.db.save_room_data(room);
                    }
                }
            }
        }
        "collecting_note" => {
            if let Some(item_id) = snap.edit_item {
                {
                    let world = state.lock();
                    if let Ok(Some(mut item)) = world.db.get_item_data(&item_id) {
                        item.note_content = if snap.content.is_empty() {
                            None
                        } else {
                            Some(snap.content)
                        };
                        let _ = world.db.save_item_data(item);
                    }
                }
            }
        }
        "collecting_long" => {
            if let Some(item_id) = snap.edit_item {
                {
                    let world = state.lock();
                    if let Ok(Some(mut item)) = world.db.get_item_data(&item_id) {
                        item.long_desc = snap.content;
                        let _ = world.db.save_item_data(item);
                    }
                }
            }
        }
        "collecting_board_post" => {
            // Boards refuse empty posts — match the inline policy: drop silently.
            if !snap.content.is_empty() {
                if let (Some(vnum), Some(subj)) = (snap.board_vnum, snap.board_subject) {
                    if !snap.author.is_empty() {
                        {
                            let world = state.lock();
                            let max = world
                                .db
                                .get_item_by_vnum(&vnum)
                                .ok()
                                .flatten()
                                .and_then(|item| item.board_max_messages);
                            let post = BoardPost::new(vnum, snap.author, subj, snap.content);
                            let _ = world.db.store_board_post(post, max);
                        }
                    }
                }
            }
        }
        "collecting_dg_body" => {
            if let (Some(kind), Some(idx)) = (snap.trigger_host, snap.trigger_index) {
                let host_id = match kind.as_str() {
                    "mobile" => snap.edit_mobile,
                    "item" => snap.edit_item,
                    "room" => snap.edit_room,
                    _ => None,
                };
                if let Some(id) = host_id {
                    let body = if snap.content.is_empty() {
                        None
                    } else {
                        Some(snap.content)
                    };
                    let author_opt = if snap.author.is_empty() {
                        None
                    } else {
                        Some(snap.author)
                    };
                    let author_is_admin = snap.author_is_admin;
                    {
                        let world = state.lock();
                        match kind.as_str() {
                            "mobile" => {
                                let _ = world.db.update_mobile(&id, |m| {
                                    if idx < m.triggers.len() {
                                        m.triggers[idx].dg_body = body.clone();
                                        m.triggers[idx].authored_by = author_opt.clone();
                                        if !author_is_admin {
                                            m.triggers[idx].elevated = false;
                                        }
                                    }
                                });
                            }
                            "item" => {
                                let _ = world.db.update_item(&id, |i| {
                                    if idx < i.triggers.len() {
                                        i.triggers[idx].dg_body = body.clone();
                                        i.triggers[idx].authored_by = author_opt.clone();
                                        if !author_is_admin {
                                            i.triggers[idx].elevated = false;
                                        }
                                    }
                                });
                            }
                            "room" => {
                                let _ = world.db.update_room(&id, |r| {
                                    if idx < r.triggers.len() {
                                        r.triggers[idx].dg_body = body.clone();
                                        r.triggers[idx].authored_by = author_opt.clone();
                                        if !author_is_admin {
                                            r.triggers[idx].elevated = false;
                                        }
                                    }
                                });
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        "collecting_dg_proto_body" => {
            if let Some(vnum) = snap.proto_vnum.as_ref() {
                {
                    let world = state.lock();
                    let msg = save_dg_proto_body_msg(&world.db, vnum, snap.content.clone(), &snap.author);
                    let _ = snap.sender.send(msg);
                }
            }
        }
        "collecting_dialogue_node_text" => {
            if let (Some(mid), Some(name)) = (snap.edit_mobile, snap.dialogue_node_name) {
                {
                    let world = state.lock();
                    let _ = world.db.update_mobile(&mid, |m| {
                        if let Some(tree) = m.dialogue_tree.as_mut() {
                            if let Some(node) = tree.nodes.get_mut(&name) {
                                node.text = snap.content.clone();
                            }
                        }
                    });
                }
            }
        }
        "collecting_extra_desc" => {
            if let Some(item_id) = snap.edit_item {
                {
                    let world = state.lock();
                    if let Ok(Some(mut item)) = world.db.get_item_data(&item_id) {
                        item.extra_descs.push(ExtraDesc {
                            keywords: snap.extra_keywords,
                            description: snap.content,
                        });
                        let _ = world.db.save_item_data(item);
                    }
                }
            } else if let Some(room_id) = snap.edit_room {
                {
                    let world = state.lock();
                    if let Ok(Some(mut room)) = world.db.get_room_data(&room_id) {
                        room.extra_descs.push(ExtraDesc {
                            keywords: snap.extra_keywords,
                            description: snap.content,
                        });
                        let _ = world.db.save_room_data(room);
                    }
                }
            }
        }
        "collecting_motd" => {
            let world = state.lock();
            let _ = world.db.set_setting("motd", &snap.content);
        }
        _ => {}
    }

    // Clear OLC state regardless of save outcome.
    {
        let mut conns = connections.lock();
        if let Some(session) = conns.get_mut(&connection_id) {
            clear_olc_state(session);
        }
    }
    true
}

/// Modern-editor save: commit the supplied buffer content to the entity
/// targeted by the current OLC mode without tearing down the editor. The
/// caller (modern editor Ctrl-O handler) re-renders the editor afterwards
/// with `EditorSession::mark_saved`. Mirrors `force_save_editor` minus the
/// truncation broadcast and the trailing `clear_olc_state` — both happen
/// in the editor session instead.
///
/// Returns the number of bytes written on success, `None` if the session
/// is not in a text-editor OLC mode or the buffer exceeds the per-mode cap.
pub fn save_modern_editor(
    connection_id: ConnectionId,
    connections: &SharedConnections,
    state: &SharedState,
    content: String,
) -> Option<usize> {
    struct Snapshot {
        mode: String,
        content: String,
        edit_room: Option<Uuid>,
        edit_item: Option<Uuid>,
        edit_mobile: Option<Uuid>,
        board_vnum: Option<String>,
        board_subject: Option<String>,
        trigger_host: Option<String>,
        trigger_index: Option<usize>,
        proto_vnum: Option<String>,
        dialogue_node_name: Option<String>,
        extra_keywords: Vec<String>,
        author: String,
        author_is_admin: bool,
        sender: session::ClientSender,
    }

    let snap = {
        let conns = connections.lock();
        let session = conns.get(&connection_id)?;
        let mode = match session.olc_mode.as_deref() {
            Some(m) if is_text_editor_mode(m) => m.to_string(),
            _ => return None,
        };
        let cap = match mode.as_str() {
            "collecting_desc" => crate::api::validate::DESCRIPTION_MAX,
            _ => 32 * 1024,
        };
        if content.len() > cap {
            return None;
        }
        let (author, author_is_admin) = session
            .character
            .as_ref()
            .map(|c| (c.name.clone(), c.is_admin))
            .unwrap_or((String::new(), false));
        Snapshot {
            mode,
            content,
            edit_room: session.olc_edit_room,
            edit_item: session.olc_edit_item,
            edit_mobile: session.olc_edit_mobile,
            board_vnum: session.olc_edit_board_vnum.clone(),
            board_subject: session.olc_board_subject.clone(),
            trigger_host: session.olc_edit_trigger_host.clone(),
            trigger_index: session.olc_edit_trigger_index,
            proto_vnum: session.olc_edit_proto_vnum.clone(),
            dialogue_node_name: session.olc_dialogue_node_name.clone(),
            extra_keywords: session.olc_extra_keywords.clone(),
            author,
            author_is_admin,
            sender: session.sender.clone(),
        }
    };

    let written = snap.content.len();

    match snap.mode.as_str() {
        "collecting_desc" => {
            if let Some(room_id) = snap.edit_room {
                {
                    let world = state.lock();
                    if let Ok(Some(mut room)) = world.db.get_room_data(&room_id) {
                        room.description = snap.content;
                        let _ = world.db.save_room_data(room);
                    }
                }
            }
        }
        "collecting_note" => {
            if let Some(item_id) = snap.edit_item {
                {
                    let world = state.lock();
                    if let Ok(Some(mut item)) = world.db.get_item_data(&item_id) {
                        item.note_content = if snap.content.is_empty() {
                            None
                        } else {
                            Some(snap.content)
                        };
                        let _ = world.db.save_item_data(item);
                    }
                }
            }
        }
        "collecting_long" => {
            if let Some(item_id) = snap.edit_item {
                {
                    let world = state.lock();
                    if let Ok(Some(mut item)) = world.db.get_item_data(&item_id) {
                        item.long_desc = snap.content;
                        let _ = world.db.save_item_data(item);
                    }
                }
            }
        }
        "collecting_board_post" => {
            if !snap.content.is_empty() {
                if let (Some(vnum), Some(subj)) = (snap.board_vnum, snap.board_subject) {
                    if !snap.author.is_empty() {
                        {
                            let world = state.lock();
                            let max = world
                                .db
                                .get_item_by_vnum(&vnum)
                                .ok()
                                .flatten()
                                .and_then(|item| item.board_max_messages);
                            let post = BoardPost::new(vnum, snap.author, subj, snap.content);
                            let _ = world.db.store_board_post(post, max);
                        }
                    }
                }
            }
        }
        "collecting_dg_body" => {
            if let (Some(kind), Some(idx)) = (snap.trigger_host, snap.trigger_index) {
                let host_id = match kind.as_str() {
                    "mobile" => snap.edit_mobile,
                    "item" => snap.edit_item,
                    "room" => snap.edit_room,
                    _ => None,
                };
                if let Some(id) = host_id {
                    let body = if snap.content.is_empty() {
                        None
                    } else {
                        Some(snap.content)
                    };
                    let author_opt = if snap.author.is_empty() {
                        None
                    } else {
                        Some(snap.author)
                    };
                    let author_is_admin = snap.author_is_admin;
                    {
                        let world = state.lock();
                        match kind.as_str() {
                            "mobile" => {
                                let _ = world.db.update_mobile(&id, |m| {
                                    if idx < m.triggers.len() {
                                        m.triggers[idx].dg_body = body.clone();
                                        m.triggers[idx].authored_by = author_opt.clone();
                                        if !author_is_admin {
                                            m.triggers[idx].elevated = false;
                                        }
                                    }
                                });
                            }
                            "item" => {
                                let _ = world.db.update_item(&id, |i| {
                                    if idx < i.triggers.len() {
                                        i.triggers[idx].dg_body = body.clone();
                                        i.triggers[idx].authored_by = author_opt.clone();
                                        if !author_is_admin {
                                            i.triggers[idx].elevated = false;
                                        }
                                    }
                                });
                            }
                            "room" => {
                                let _ = world.db.update_room(&id, |r| {
                                    if idx < r.triggers.len() {
                                        r.triggers[idx].dg_body = body.clone();
                                        r.triggers[idx].authored_by = author_opt.clone();
                                        if !author_is_admin {
                                            r.triggers[idx].elevated = false;
                                        }
                                    }
                                });
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        "collecting_dg_proto_body" => {
            if let Some(vnum) = snap.proto_vnum.as_ref() {
                {
                    let world = state.lock();
                    let msg = save_dg_proto_body_msg(&world.db, vnum, snap.content.clone(), &snap.author);
                    let _ = snap.sender.send(msg);
                }
            }
        }
        "collecting_dialogue_node_text" => {
            if let (Some(mid), Some(name)) = (snap.edit_mobile, snap.dialogue_node_name) {
                {
                    let world = state.lock();
                    let _ = world.db.update_mobile(&mid, |m| {
                        if let Some(tree) = m.dialogue_tree.as_mut() {
                            if let Some(node) = tree.nodes.get_mut(&name) {
                                node.text = snap.content.clone();
                            }
                        }
                    });
                }
            }
        }
        "collecting_extra_desc" => {
            if let Some(item_id) = snap.edit_item {
                {
                    let world = state.lock();
                    if let Ok(Some(mut item)) = world.db.get_item_data(&item_id) {
                        item.extra_descs.push(ExtraDesc {
                            keywords: snap.extra_keywords,
                            description: snap.content,
                        });
                        let _ = world.db.save_item_data(item);
                    }
                }
            } else if let Some(room_id) = snap.edit_room {
                {
                    let world = state.lock();
                    if let Ok(Some(mut room)) = world.db.get_room_data(&room_id) {
                        room.extra_descs.push(ExtraDesc {
                            keywords: snap.extra_keywords,
                            description: snap.content,
                        });
                        let _ = world.db.save_room_data(room);
                    }
                }
            }
        }
        "collecting_motd" => {
            let world = state.lock();
            let _ = world.db.set_setting("motd", &snap.content);
        }
        _ => {}
    }

    Some(written)
}

/// If a player named `char_name` is composing in the multi-line editor when a
/// "major interruption" (combat damage, drowning, environmental harm) lands,
/// force-save their buffer, clear the writing state, and inform them. No-op
/// for players who aren't currently writing. Lives in lib.rs so combat ticks,
/// script-side combat, and environmental damage paths can all call it.
pub fn interrupt_writer_by_name(connections: &SharedConnections, state: &SharedState, char_name: &str) {
    let Some(conn_id) = find_player_connection_by_name(connections, char_name) else {
        return;
    };
    let writing = {
        let conns = connections.lock();
        conns.get(&conn_id).map(session_is_writing).unwrap_or(false)
    };
    if !writing {
        return;
    }
    force_save_editor(conn_id, connections, state);
    {
        let conns = connections.lock();
        if let Some(session) = conns.get(&conn_id) {
            let _ = session
                .sender
                .send("You're interrupted! Your work is saved.\n".to_string());
        }
    }
}

/// Build the prompt string for a connection.
///
/// What the prompt can *say* lives in [`crate::prompt`] as a table of
/// segments; this function only gathers the state those segments read. It
/// gathers as little as the player's format actually asks for: the combat
/// target, the equipped-item contributions and the faction display names
/// each cost a lock and some reads, and are skipped when no token wants
/// them. A `simple` prompt now touches nothing, where it used to fetch the
/// player's equipped items and throw them away.
fn build_prompt(connection_id: &ConnectionId, connections: &SharedConnections, state: &SharedState) -> String {
    // Flush any skill XP batched since the last prompt. This runs here rather
    // than on a tick because the prompt is rendered after every command, which
    // is exactly the cadence we want: a combat round's worth of 10-XP awards
    // collapses into one line instead of spamming the scroll, and the line
    // lands immediately above the prompt where the player is already looking.
    //
    // Scoped so the lock is released before the main read block below
    // re-acquires it.
    {
        {
            let mut conns = connections.lock();
            if let Some(session) = conns.get_mut(connection_id) {
                for line in progress::drain_xp_buffer(session) {
                    let _ = session.sender.send(format!("{}\n", line));
                }
            }
        }
    }

    // Snapshot everything the prompt can read off the session in one lock,
    // and pick the format while we are in there.
    let (format, mut ctx) = {
        let conns = connections.lock();
        let session = match conns.get(connection_id) {
            Some(s) => s,
            None => return prompt::SIMPLE_FORMAT.to_string(),
        };
        match &session.character {
            // Not logged in = simple prompt.
            None => return prompt::SIMPLE_FORMAT.to_string(),
            Some(c) => (
                prompt::format_for(c).to_string(),
                prompt::PromptContext::from_character(c, session.colors_enabled),
            ),
        }
    };

    let (pieces, _unknown) = prompt::parse(&format);

    // Combat target segment: who you are fighting and how they are holding
    // up, plus the engagement distance when it isn't melee.
    //
    // The distance half already existed but only rendered at Ranged/Pole, so
    // in the overwhelmingly common melee case the prompt said nothing about
    // the fight at all — the player had to spend a round on `look` to learn
    // whether the thing hitting them was nearly dead. MSDP has reported
    // OPPONENT_NAME/OPPONENT_HEALTH for GUI clients all along; this is the
    // same two facts for everyone else.
    if prompt::uses(&pieces, "target") {
        let world = state.lock();
        if let Ok(Some(char_data)) = world.db.get_character_data(&ctx.name) {
            if char_data.combat.in_combat && !char_data.combat.targets.is_empty() {
                let primary = &char_data.combat.targets[0];
                let (target_name, hp, max_hp) = match primary.target_type {
                    CombatTargetType::Mobile => world
                        .db
                        .get_mobile_data(&primary.target_id)
                        .ok()
                        .flatten()
                        .map(|m| (m.name.clone(), m.current_hp, m.max_hp))
                        .unwrap_or_else(|| ("opponent".to_string(), 0, 1)),
                    // `target_name` is what the PvP path records; the old
                    // distance tag hardcoded "opponent" here and lost it.
                    CombatTargetType::Player => primary
                        .target_name
                        .as_deref()
                        .and_then(|n| world.db.get_character_data(n).ok().flatten())
                        .map(|c| (c.name.clone(), c.hp, c.effective_max_hp()))
                        .unwrap_or_else(|| {
                            (
                                primary.target_name.clone().unwrap_or_else(|| "opponent".to_string()),
                                0,
                                1,
                            )
                        }),
                };
                let distance = char_data
                    .combat
                    .distances
                    .get(&primary.target_id)
                    .copied()
                    .unwrap_or(CombatDistance::Melee);
                ctx.target_tag = combat_text::target_prompt_tag(&target_name, hp, max_hp, distance, ctx.colors);
            }
        }
    }

    // Display names for any faction the format names. Resolved in one pass
    // so a prompt carrying two standings does not lock the world twice.
    let wanted = prompt::requested_factions(&pieces);
    if !wanted.is_empty() {
        let world = state.lock();
        for key in wanted {
            if let Some(def) = world.faction_definitions.get(&key) {
                ctx.faction_names.insert(key, def.display().to_string());
            }
        }
    }

    // Contributions from equipped items with on_prompt triggers — a watch
    // showing the time, a compass showing the heading. Each one runs a Rhai
    // script, so this stays behind the token that asks for it.
    if prompt::uses(&pieces, "extra") {
        let equipped: Vec<Uuid> = {
            let world = state.lock();
            world
                .db
                .get_equipped_items(&ctx.name)
                .unwrap_or_default()
                .into_iter()
                .map(|item| item.id)
                .collect()
        };
        ctx.extra = collect_on_prompt_contributions(connection_id, connections, state, &equipped);
    }

    prompt::render(&pieces, &ctx)
}

impl PlayerSession {
    /// A blank session wired to the given channels, for tests that need a
    /// player to be *online* — anything that asserts on text the engine sends
    /// via `session.sender`. Every field beyond the channels is at its
    /// logged-out default; set `character` after constructing.
    ///
    /// Exists so the ~40-field literal lives in one place. Adding a field to
    /// `PlayerSession` should not break every test module that happens to
    /// need a connected player.
    ///
    /// Deliberately not `#[cfg(test)]`, for the same reason as
    /// [`World::minimal`]: the bin crate and the integration tests compile
    /// against the lib without `cfg(test)`, so a test-gated constructor is
    /// invisible to the tick-side test modules that need it most.
    pub fn new_for_test(
        sender: mpsc::UnboundedSender<String>,
        input_sender: mpsc::Sender<InputEvent>,
    ) -> PlayerSession {
        PlayerSession {
            pending_audit: None,
            character: None,
            sender: session::ClientSender::unbounded(sender),
            raw_sender: None,
            input_sender,
            addr: "127.0.0.1:0".parse().expect("loopback addr"),
            olc_mode: None,
            olc_buffer: Vec::new(),
            olc_edit_room: None,
            olc_edit_item: None,
            olc_edit_board_vnum: None,
            olc_board_subject: None,
            olc_edit_mobile: None,
            olc_edit_trigger_host: None,
            olc_edit_trigger_index: None,
            olc_edit_proto_vnum: None,
            olc_dialogue_node_name: None,
            olc_extra_keywords: Vec::new(),
            olc_undo_buffer: None,
            wizard_data: None,
            mxp_enabled: false,
            colors_enabled: true,
            show_room_flags: false,
            xp_buffer: std::collections::BTreeMap::new(),
            telnet_state: telnet::TelnetState::new(),
            input_buffer: String::new(),
            cursor_pos: 0,
            command_history: std::collections::VecDeque::new(),
            history_index: None,
            saved_input: String::new(),
            escape_state: telnet::EscapeState::Normal,
            pending_ai_request: None,
            pending_ai_response: None,
            pending_ai_target: None,
            fishing_state: None,
            afk: false,
            last_activity_time: 0,
            abbrev_enabled: true,
            map_legend_shown: false,
            dialogue_partner_id: None,
            account_id: None,
            account_name: None,
            slow_move_completing: false,
            disconnected_at: None,
            session_started_at: None,
            modern_editor: None,
            removed_signal: None,
        }
    }

    /// Sync MSDP vitals if supported and requested.
    ///
    /// Takes a `Db` handle rather than `SharedState`: callers invoke this while
    /// holding the Connections lock, and taking the World lock here would
    /// invert the World -> Connections lock order used elsewhere (ABBA
    /// deadlock). `Db` is a cheap clone of Arc'd sled trees and needs no lock.
    pub fn sync_msdp_vitals(&self, db: &crate::db::Db) {
        if !self.telnet_state.msdp_supported
            || self.telnet_state.msdp_reported_variables.is_empty()
            || self.raw_sender.is_none()
        {
            return;
        }

        let char_data = match &self.character {
            Some(c) => c,
            None => return,
        };

        let tx_raw = self.raw_sender.as_ref().unwrap();
        let reported = &self.telnet_state.msdp_reported_variables;

        if reported.contains("HEALTH") {
            let _ = tx_raw.send(telnet::build_msdp_var("HEALTH", &char_data.hp.to_string()));
        }
        if reported.contains("HEALTH_MAX") {
            let _ = tx_raw.send(telnet::build_msdp_var(
                "HEALTH_MAX",
                &char_data.effective_max_hp().to_string(),
            ));
        }
        if reported.contains("STAMINA") {
            let _ = tx_raw.send(telnet::build_msdp_var("STAMINA", &char_data.stamina.to_string()));
        }
        if reported.contains("STAMINA_MAX") {
            let _ = tx_raw.send(telnet::build_msdp_var(
                "STAMINA_MAX",
                &char_data.max_stamina.to_string(),
            ));
        }
        if reported.contains("MANA") {
            let _ = tx_raw.send(telnet::build_msdp_var("MANA", &char_data.mana.to_string()));
        }
        if reported.contains("MANA_MAX") {
            let _ = tx_raw.send(telnet::build_msdp_var("MANA_MAX", &char_data.max_mana.to_string()));
        }
        if reported.contains("GOLD") {
            let _ = tx_raw.send(telnet::build_msdp_var("GOLD", &char_data.gold.to_string()));
        }
        if reported.contains("BANK_GOLD") {
            let _ = tx_raw.send(telnet::build_msdp_var("BANK_GOLD", &char_data.bank_gold.to_string()));
        }
        if reported.contains("LEVEL") {
            let _ = tx_raw.send(telnet::build_msdp_var("LEVEL", &char_data.level.to_string()));
        }
        if reported.contains("ROOM_NAME") {
            if let Some(ref char) = self.character {
                if let Ok(Some(room)) = db.get_room_data(&char.current_room_id) {
                    let _ = tx_raw.send(telnet::build_msdp_var("ROOM_NAME", &room.title));
                }
            }
        }
        if reported.contains("OPPONENT_HEALTH") || reported.contains("OPPONENT_NAME") {
            if let Some(ref char) = self.character {
                if char.combat.in_combat && !char.combat.targets.is_empty() {
                    let primary = &char.combat.targets[0];
                    if reported.contains("OPPONENT_NAME") {
                        let target_name = match primary.target_type {
                            CombatTargetType::Mobile => db
                                .get_mobile_data(&primary.target_id)
                                .ok()
                                .flatten()
                                .map(|m| m.name.clone())
                                .unwrap_or_else(|| "opponent".to_string()),
                            CombatTargetType::Player => primary
                                .target_name
                                .clone()
                                .unwrap_or_else(|| "another player".to_string()),
                        };
                        let _ = tx_raw.send(telnet::build_msdp_var("OPPONENT_NAME", &target_name));
                    }
                    if reported.contains("OPPONENT_HEALTH") {
                        let (hp, max_hp) = match primary.target_type {
                            CombatTargetType::Mobile => db
                                .get_mobile_data(&primary.target_id)
                                .ok()
                                .flatten()
                                .map(|m| (m.current_hp, m.max_hp))
                                .unwrap_or((0, 1)),
                            CombatTargetType::Player => primary
                                .target_name
                                .as_deref()
                                .and_then(|n| db.get_character_data(n).ok().flatten())
                                .map(|c| (c.hp, c.effective_max_hp()))
                                .unwrap_or((100, 100)),
                        };
                        let pct: i32 = (hp * 100) / max_hp.max(1);
                        let _ = tx_raw.send(telnet::build_msdp_var("OPPONENT_HEALTH", &pct.to_string()));
                    }
                } else {
                    if reported.contains("OPPONENT_NAME") {
                        let _ = tx_raw.send(telnet::build_msdp_var("OPPONENT_NAME", ""));
                    }
                    if reported.contains("OPPONENT_HEALTH") {
                        let _ = tx_raw.send(telnet::build_msdp_var("OPPONENT_HEALTH", "0"));
                    }
                }
            }
        }
    }
}

/// Collect prompt contributions from equipped items with on_prompt triggers
fn collect_on_prompt_contributions(
    connection_id: &ConnectionId,
    _connections: &SharedConnections,
    state: &SharedState,
    equipped_item_ids: &[Uuid],
) -> String {
    let mut contributions = String::new();

    // Only the Db is needed. Clone it and release World immediately: these
    // scripts run on every prompt, and holding World across them meant one
    // slow or looping on_prompt script stalled every command on the server.
    let db = state.lock().db.clone();

    for item_id in equipped_item_ids {
        let item = match db.get_item_data(item_id) {
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
            let script_content = match crate::script::trigger_sandbox::read_trigger_script(&trigger.script_name) {
                Ok(c) => c,
                Err(_) => continue,
            };

            // Create a minimal engine for trigger execution
            let mut trigger_engine = crate::script::trigger_sandbox::sandboxed_engine();

            // Register get_game_time for watch triggers
            let db_for_time = db.clone();
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

                    match crate::script::trigger_sandbox::guard_panic(|| {
                        trigger_engine.call_fn::<rhai::Dynamic>(
                            &mut rhai::Scope::new(),
                            &ast,
                            "run_trigger",
                            (item_id_str, conn_id_str, context),
                        )
                    }) {
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

/// Run a command script's `run_command`, converting a Rust panic inside any
/// registered function into a script error.
///
/// Without this, a panic in a native Rhai function (e.g. an unchecked
/// `gen_range`) unwinds out of `handle_connection`, skipping its cleanup: the
/// session stays in `SharedConnections` as a ghost and the linkdead/save path
/// never runs. The player instead sees an error and keeps their connection.
fn call_run_command_guarded(
    engine: &Engine,
    scope: &mut rhai::Scope,
    ast: &rhai::AST,
    args: String,
    connection_id: String,
    command: &str,
) -> Result<rhai::Dynamic, String> {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine.call_fn::<rhai::Dynamic>(scope, ast, "run_command", (args, connection_id))
    }));
    match outcome {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Err(e.to_string()),
        Err(panic) => {
            let detail = panic
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".to_string());
            error!("Command '{}' panicked: {}", command, detail);
            Err("an internal error occurred".to_string())
        }
    }
}

pub async fn handle_connection(
    socket: tokio::net::TcpStream,
    addr: std::net::SocketAddr,
    state: SharedState,
    connection_id: ConnectionId,
) {
    let (reader, writer) = socket.into_split();

    let (tx_client, rx_client) = mpsc::unbounded_channel::<String>();
    // Bounded — see HIGH-1 in the security audit. Try-send semantics drop
    // overflow lines and warn the client rather than letting a fast typist
    // (or a scripted attacker) inflate the queue without limit.
    let (tx_input, mut rx_input) = mpsc::channel::<InputEvent>(INPUT_CHANNEL_CAPACITY);
    let (tx_raw, rx_raw) = mpsc::unbounded_channel::<Vec<u8>>();
    // Bounded by bytes queued; overflow posts `Disconnect` to the input loop.
    let tx_client = session::ClientSender::new(tx_client, tx_input.clone());
    let drain = tx_client.drain_handle();

    // Get the connections map from World and store the PlayerSession
    // `msdp_db` lets MSDP syncs read room/opponent data while the
    // Connections lock is held, without re-taking the World lock.
    let (connections, msdp_db) = {
        let world = state.lock();
        (world.connections.clone(), world.db.clone())
    };

    // Activity log for whoever logs in on this connection (see `player_log`).
    let mut play_log = {
        let world = state.lock();
        player_log::SessionTracker::new(world.db.clone(), connection_id, addr)
    };

    let (removed_tx, removed_rx) = tokio::sync::oneshot::channel::<()>();
    {
        let mut conns = connections.lock();
        conns.insert(
            connection_id,
            PlayerSession {
                pending_audit: None,
                character: None,
                sender: tx_client.clone(),
                raw_sender: Some(tx_raw.clone()),
                input_sender: tx_input.clone(),
                addr,
                olc_mode: None,
                olc_buffer: Vec::new(),
                olc_edit_room: None,
                olc_edit_item: None,
                olc_edit_board_vnum: None,
                olc_board_subject: None,
                olc_edit_mobile: None,
                olc_edit_trigger_host: None,
                olc_edit_trigger_index: None,
                olc_edit_proto_vnum: None,
                olc_dialogue_node_name: None,
                olc_extra_keywords: Vec::new(),
                olc_undo_buffer: None,
                wizard_data: None,
                mxp_enabled: false,
                colors_enabled: true,
                show_room_flags: false,
                xp_buffer: std::collections::BTreeMap::new(),
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
                map_legend_shown: false,
                dialogue_partner_id: None,
                account_id: None,
                account_name: None,
                slow_move_completing: false,
                disconnected_at: None,
                session_started_at: None,
                modern_editor: None,
                removed_signal: Some(removed_tx),
            },
        );
    }

    // Send welcome banner. Rendered with the create / forgot lines swapped
    // for their three-arg variants when email verification is required.
    let banner = {
        let world = state.lock();
        crate::session::banner::render_login_banner(&world.db)
    };
    if !banner.is_empty() {
        let _ = tx_client.send(banner);
    }

    // Spawn character-mode read task with telnet protocol support. Hand it a
    // cloned `Db` handle (cheap, Arc-based) so the read loop can consult the
    // `idle_disconnect_enabled` setting without ever locking the World mutex.
    let read_db = { state.lock().db.clone() };
    tokio::spawn(handle_read_char_mode(
        reader,
        addr,
        connection_id,
        tx_input,
        connections.clone(),
        tx_raw,
        read_db,
        removed_rx,
    ));

    // Spawn write task with raw byte support for telnet negotiation
    tokio::spawn(handle_write_with_raw(writer, addr, rx_client, rx_raw, drain));

    let cx = conn_input::ConnCx {
        connection_id,
        connections: &connections,
        state: &state,
        tx_client: &tx_client,
    };

    // Main loop for processing client input and sending responses
    loop {
        // Check if connection still exists. If not, it means it was disconnected by a script.
        let is_connected = {
            let conns = connections.lock();
            conns.contains_key(&connection_id)
        };
        if !is_connected {
            info!("Connection {} removed by script, terminating handle_connection.", addr);
            break;
        }
        play_log.sync(&connections);

        // Only send prompt in line mode (char mode echoes as you type)
        let is_char_mode = {
            let conns = connections.lock();
            conns
                .get(&connection_id)
                .map(|s| s.telnet_state.char_mode)
                .unwrap_or(false)
        };
        if !is_char_mode {
            {
                let conns = connections.lock();
                if let Some(session) = conns.get(&connection_id) {
                    session.sync_msdp_vitals(&msdp_db);
                }
            }
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

        // Read task signalled it is ending (idle/EOF/error). Break here so the
        // linkdead-cleanup block below runs — the channel itself never closes
        // because the session's `input_sender` clone outlives the read task.
        if matches!(event, InputEvent::Disconnect) {
            break;
        }

        // Handle Tab completion
        if matches!(event, InputEvent::Tab) {
            conn_input::tab_completion(&cx);
            continue;
        }

        // Extract input string from event. Player-controlled text is
        // sanitized once here so any path that downstreams a command's
        // args into a broadcast (say/tell/shout/emote, board posts,
        // OLC descriptions) is automatically free of ANSI escapes,
        // CRLF protocol breaks, and other control chars. Server-emitted
        // output is unaffected because it never passes through this
        // boundary.
        // Modern editor signalling. The read handler emits these when the
        // player presses Ctrl-X / Ctrl-O inside the new editor; we mirror
        // the legacy save/cancel cleanup here without sanitising the
        // content (the editor already controls what enters the buffer).
        match &event {
            InputEvent::ModernEditorCancel => {
                {
                    let mut conns = connections.lock();
                    if let Some(session) = conns.get_mut(&connection_id) {
                        clear_olc_state(session);
                    }
                }
                // Clear the screen the editor was painting on so the
                // legacy prompt lands on a clean line.
                if let Some(raw) = {
                    let conns = connections.lock();
                    conns.get(&connection_id).and_then(|s| s.raw_sender.clone())
                } {
                    let _ = raw.send(b"\x1b[?2004l\x1b[?1000l\x1b[2J\x1b[H".to_vec());
                }
                let _ = tx_client.send("Editing cancelled — no changes saved.\n".to_string());
                let prompt = build_prompt(&connection_id, &connections, &state);
                let _ = tx_client.send(prompt);
                continue;
            }
            InputEvent::ModernEditorSave(content) => {
                let mode = {
                    let conns = connections.lock();
                    conns.get(&connection_id).and_then(|s| s.olc_mode.clone())
                };
                let bytes = content.len();
                let result = save_modern_editor(connection_id, &connections, &state, content.clone());
                {
                    let mut conns = connections.lock();
                    if let Some(session) = conns.get_mut(&connection_id) {
                        clear_olc_state(session);
                    }
                }
                if let Some(raw) = {
                    let conns = connections.lock();
                    conns.get(&connection_id).and_then(|s| s.raw_sender.clone())
                } {
                    let _ = raw.send(b"\x1b[?2004l\x1b[?1000l\x1b[2J\x1b[H".to_vec());
                }
                let msg = match (result, mode.as_deref()) {
                    (Some(_), Some("collecting_desc")) => "Description saved.\n".to_string(),
                    (Some(_), Some("collecting_long")) => "Long description saved.\n".to_string(),
                    (Some(_), Some("collecting_note")) => {
                        if bytes == 0 {
                            "Note cleared.\n".to_string()
                        } else {
                            "Note saved.\n".to_string()
                        }
                    }
                    (Some(_), Some("collecting_board_post")) => {
                        if bytes == 0 {
                            "Empty post discarded.\n".to_string()
                        } else {
                            "Post submitted.\n".to_string()
                        }
                    }
                    (Some(_), Some("collecting_dg_body")) => "DG trigger body saved.\n".to_string(),
                    // Proto save sends its own detailed report (refresh count + warnings)
                    // from save_modern_editor's "collecting_dg_proto_body" branch.
                    (Some(_), Some("collecting_dg_proto_body")) => String::new(),
                    (Some(_), Some("collecting_dialogue_node_text")) => "Dialogue node text saved.\n".to_string(),
                    (Some(_), Some("collecting_extra_desc")) => "Extra description saved.\n".to_string(),
                    (Some(_), Some("collecting_motd")) => "MOTD saved.\n".to_string(),
                    (Some(_), _) => "Saved.\n".to_string(),
                    (None, _) => format!(
                        "Save refused (buffer too large: {} bytes). Re-enter the editor to trim.\n",
                        bytes
                    ),
                };
                let _ = tx_client.send(msg);
                let prompt = build_prompt(&connection_id, &connections, &state);
                let _ = tx_client.send(prompt);
                continue;
            }
            _ => {}
        }

        let input = match event {
            InputEvent::Line(s) | InputEvent::RawLine(s) => session::sanitize_player_text(&s),
            InputEvent::Tab => continue, // Already handled above
            InputEvent::ModernEditorSave(_) | InputEvent::ModernEditorCancel => {
                unreachable!("modern editor events handled above")
            }
            InputEvent::Disconnect => continue, // handled above; unreachable in practice
        };

        // Update last activity time for idle tracking
        {
            let mut conns = connections.lock();
            if let Some(session) = conns.get_mut(&connection_id) {
                session.last_activity_time = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
            }
        }

        // Check for OLC description collection mode
        let olc_mode = {
            let conns = connections.lock();
            conns.get(&connection_id).and_then(|s| s.olc_mode.clone())
        };

        if olc_mode.as_deref() == Some("collecting_desc") {
            conn_input::collect_desc(&cx, &input);
            continue;
        }

        // Check for OLC note collection mode (item.note_content)
        if olc_mode.as_deref() == Some("collecting_note") {
            conn_input::collect_note(&cx, &input);
            continue;
        }

        // Check for OLC long-description collection mode (item.long_desc)
        if olc_mode.as_deref() == Some("collecting_long") {
            conn_input::collect_long(&cx, &input);
            continue;
        }

        // Check for OLC bulletin board post collection mode
        if olc_mode.as_deref() == Some("collecting_board_post") {
            conn_input::collect_board_post(&cx, &input);
            continue;
        }

        // Check for OLC DG trigger body collection mode
        if olc_mode.as_deref() == Some("collecting_dg_body") {
            conn_input::collect_dg_body(&cx, &input);
            continue;
        }

        // Line-mode editor for a DG trigger prototype body. Mirrors
        // collecting_dg_body but routes the saved content through
        // save_dg_trigger_proto_with_refresh: analyzer parse-check,
        // persist, then sweep refresh across all attached instances.
        if olc_mode.as_deref() == Some("collecting_dg_proto_body") {
            conn_input::collect_dg_proto_body(&cx, &input);
            continue;
        }

        // Multi-line text editor for a dialogue tree node's `text` field.
        if olc_mode.as_deref() == Some("collecting_dialogue_node_text") {
            conn_input::collect_dialogue_node_text(&cx, &input);
            continue;
        }

        // Check for OLC extra description collection mode
        if olc_mode.as_deref() == Some("collecting_extra_desc") {
            conn_input::collect_extra_desc(&cx, &input);
            continue;
        }

        // Check for OLC MOTD collection mode
        if olc_mode.as_deref() == Some("collecting_motd") {
            conn_input::collect_motd(&cx, &input);
            continue;
        }

        // Handle AI description confirmation mode
        if olc_mode.as_deref() == Some("ai_confirm") {
            if conn_input::ai_confirm(&cx, &input) {
                continue;
            }
        }

        // Handle character wizard modes (from create.rhai, login.rhai, or traits.rhai)
        debug!("OLC mode check: {:?}", olc_mode);
        if let Some(ref mode) = olc_mode {
            if conn_input::script_mode(&cx, &input, mode) {
                continue;
            }
        }

        // Sticky dialogue input: numeric / keyword / bye / movement intercept.
        // Runs after OLC modes so OLC takes precedence over a stray dialogue
        // partner (which is benign to leave dangling). Movement directions and
        // unhandled keywords fall through to normal command dispatch.
        if crate::script::dialogue::session_in_dialogue(&connections, &connection_id) {
            let db_clone = {
                let world = state.lock();
                world.db.clone()
            };
            let outcome =
                crate::script::dialogue::dispatch_sticky_input(&db_clone, &connections, &state, connection_id, &input);
            match outcome {
                crate::script::dialogue::DialogueDispatch::Handled {
                    actor_lines,
                    room_broadcasts,
                    speech,
                } => {
                    let actor_name = {
                        let conns = connections.lock();
                        conns
                            .get(&connection_id)
                            .and_then(|s| s.character.as_ref().map(|c| c.name.clone()))
                    };
                    if let Some(s) = speech {
                        crate::session::emit_mob_speech(&connections, &state, &s);
                    }
                    for line in actor_lines {
                        let _ = tx_client.send(format!("{}\n", line));
                    }
                    for (room_id, msg) in room_broadcasts {
                        crate::session::broadcast_to_room(&connections, room_id, msg, actor_name.as_deref());
                    }
                    let prompt = build_prompt(&connection_id, &connections, &state);
                    let _ = tx_client.send(prompt);
                    continue;
                }
                crate::script::dialogue::DialogueDispatch::ExitedFallthrough {
                    actor_lines,
                    room_broadcasts,
                    speech,
                } => {
                    let actor_name = {
                        let conns = connections.lock();
                        conns
                            .get(&connection_id)
                            .and_then(|s| s.character.as_ref().map(|c| c.name.clone()))
                    };
                    if let Some(s) = speech {
                        crate::session::emit_mob_speech(&connections, &state, &s);
                    }
                    for line in actor_lines {
                        let _ = tx_client.send(format!("{}\n", line));
                    }
                    for (room_id, msg) in room_broadcasts {
                        crate::session::broadcast_to_room(&connections, room_id, msg, actor_name.as_deref());
                    }
                    // Fall through to normal command parsing with the same input.
                }
                crate::script::dialogue::DialogueDispatch::Fallthrough => {
                    // Fall through to normal command parsing.
                }
            }
        }

        // History recall: !! or !prefix (bash-style)
        let input = if input.starts_with('!') && input.len() > 1 {
            let recall_result = {
                let conns = connections.lock();
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

        // Split by ';' first so each piece is alias-expanded on its own —
        // otherwise "west;north" is looked up as one alias, fails, then gets
        // split into raw "west"/"north" tokens that aren't commands.
        let pieces: Vec<&str> = input.split(';').map(|s| s.trim()).collect();

        // Empty input - just redisplay prompt (useful for checking stats while resting)
        if pieces.iter().all(|p| p.is_empty()) {
            let prompt = build_prompt(&connection_id, &connections, &state);
            let _ = tx_client.send(prompt);
            continue;
        }

        let mut expanded_commands: Vec<String> = Vec::new();
        for piece in &pieces {
            if piece.is_empty() {
                continue;
            }
            let parts: Vec<&str> = piece.splitn(2, ' ').collect();
            let raw_command = parts[0];
            let raw_args = parts.get(1).map_or("", |s| *s);
            let expanded = expand_alias(&connections, connection_id, raw_command, raw_args);
            for cmd in expanded.split(';') {
                let cmd = cmd.trim();
                if !cmd.is_empty() {
                    expanded_commands.push(cmd.to_string());
                }
            }
        }

        for single_cmd in &expanded_commands {
            let single_cmd = single_cmd.as_str();

            let cmd_parts: Vec<&str> = single_cmd.splitn(2, ' ').collect();
            let command_name = cmd_parts[0];
            let args = cmd_parts.get(1).map_or("", |s| *s);

            // Check if user is logged in
            let is_logged_in = {
                let conns = connections.lock();
                conns
                    .get(&connection_id)
                    .map(|s| s.character.is_some())
                    .unwrap_or(false)
            };
            let log_ctx = play_log.context(&connections);

            // Get player permissions for access control
            let (is_builder, is_admin, abbrev_enabled) = {
                let conns = connections.lock();
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
                let world = state.lock();
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
                play_log.command(&log_ctx, &resolved_command, single_cmd, player_log::outcome::DENIED);
                let _ = tx_client.send(msg);
                continue;
            }

            // Block commands while unconscious (allow only essential ones)
            if is_logged_in {
                let is_unconscious = {
                    let conns = connections.lock();
                    conns
                        .get(&connection_id)
                        .and_then(|s| s.character.as_ref())
                        .map(|c| c.is_unconscious)
                        .unwrap_or(false)
                };
                if is_unconscious && !is_unconscious_allowed(&resolved_command) {
                    play_log.command(
                        &log_ctx,
                        &resolved_command,
                        single_cmd,
                        player_log::outcome::UNCONSCIOUS,
                    );
                    let _ = tx_client.send("You are unconscious and cannot do that.\n".to_string());
                    continue;
                }
            }

            // DG OnCommand hook: fire any OnCommand triggers on the room,
            // its mobs, and the player's items before the rhai dispatch.
            // Return(0) cancels the host command. Skip while not logged in.
            if is_logged_in {
                let db_arc = {
                    let world = state.lock();
                    std::sync::Arc::new(world.db.clone())
                };
                if crate::script::dg::fire_oncommand_for_player(
                    &connection_id,
                    command_name,
                    &resolved_command,
                    args,
                    &db_arc,
                    &connections,
                    &state,
                ) {
                    // Suppressed: skip rhai dispatch and the prompt redraw
                    // path picks up cleanly on the next iteration.
                    play_log.command(
                        &log_ctx,
                        &resolved_command,
                        single_cmd,
                        player_log::outcome::DG_SUPPRESSED,
                    );
                    let prompt = build_prompt(&connection_id, &connections, &state);
                    let _ = tx_client.send(prompt);
                    continue;
                }
            }

            // CircleMUD-style socials (smile/wave/bow/…) are handled in
            // Rust — there's no per-verb .rhai for them. The registry was
            // populated at startup from `scripts/data/socials.json`; if
            // the resolved verb is in it we render and broadcast here,
            // then short-circuit past the rhai dispatch.
            if is_logged_in {
                if let crate::social::actions::DispatchOutcome::Handled = crate::social::actions::dispatch_player_social(
                    &state,
                    &connections,
                    connection_id,
                    &resolved_command,
                    args,
                ) {
                    play_log.command(&log_ctx, &resolved_command, single_cmd, player_log::outcome::SOCIAL);
                    let prompt = build_prompt(&connection_id, &connections, &state);
                    let _ = tx_client.send(prompt);
                    continue;
                }
            }

            let script_path = format!("scripts/commands/{}.rhai", resolved_command);

            // Clone the AST and engine reference outside the lock to avoid deadlock
            // Rhai functions will need to lock state themselves
            let (ast_opt, engine) = {
                let world = state.lock();
                let ast = world.scripts.get(&script_path).cloned();
                (ast, world.engine.clone())
            };

            let mut log_outcome = player_log::outcome::OK;
            let result_message = if let Some(ast) = ast_opt {
                let mut scope = rhai::Scope::new();
                scope.set_value("args", args.to_string());
                scope.set_value("connection_id", connection_id.to_string());

                match call_run_command_guarded(
                    &engine,
                    &mut scope,
                    &ast,
                    args.to_string(),
                    connection_id.to_string(),
                    &resolved_command,
                ) {
                    Ok(output) => {
                        if output.is_string() {
                            output.into_string().unwrap()
                        } else {
                            String::new()
                        }
                    }
                    Err(e) => {
                        log_outcome = player_log::outcome::SCRIPT_ERROR;
                        format!("Error executing command {}: {}\n", resolved_command, e)
                    }
                }
            } else {
                log_outcome = player_log::outcome::UNKNOWN;
                format!("Unknown command: {}\n", command_name)
            };
            play_log.command(&log_ctx, &resolved_command, single_cmd, log_outcome);

            if !result_message.is_empty() {
                if let Err(e) = tx_client.send(result_message) {
                    error!("Failed to send error message to socket {}: {}", addr, e);
                    break;
                }
            }

            // A builder who just changed something gets one line about what
            // moved — the core loop of building, and the reason `note_grade_before`
            // exists. Drained here rather than in the editor scripts because
            // this is the one place every editor's every early return passes
            // through. Silent when the letter did not change; see
            // `audit::scan::grade_change_line`.
            {
                let pending = {
                    let mut conns = connections.lock();
                    conns.get_mut(&connection_id).and_then(|s| s.pending_audit.take())
                };
                if let Some(pending) = pending {
                    // One lock, both values, released before the grading runs.
                    let (db, ctx) = {
                        let w = state.lock();
                        (w.db.clone(), w.audit_ctx.clone())
                    };
                    if let Some(line) = audit::scan::grade_change_line(&db, &pending, &ctx) {
                        let _ = tx_client.send(format!("{line}\n"));
                    }
                }
            }

            // Sync MSDP vitals before sending prompt
            {
                let conns = connections.lock();
                if let Some(session) = conns.get(&connection_id) {
                    session.sync_msdp_vitals(&msdp_db);
                }
            }

            // Send prompt after command execution — but skip when the
            // command just opened the modern editor, otherwise the prompt
            // lands on row 1 of the freshly-cleared editor screen as a
            // visible remnant until the user redraws with ^L.
            let suppress_prompt = {
                let conns = connections.lock();
                conns.get(&connection_id).map(session_in_modern_editor).unwrap_or(false)
            };
            if !suppress_prompt {
                let prompt = build_prompt(&connection_id, &connections, &state);
                if let Err(e) = tx_client.send(prompt) {
                    error!("Failed to send prompt to socket {}: {}", addr, e);
                    break;
                }
            }
        }
    }
    info!("Connection closed: {}", addr);

    // Session gone from the map = the script removed it (quit); still present
    // with a character = TCP drop, which leaves the character linkdead.
    let still_present = connections.lock().contains_key(&connection_id);
    play_log.finish(if still_present { "linkdead" } else { "quit" });

    // Best-effort save of any in-progress multi-line edit before the session
    // goes linkdead. The buffer lives only in PlayerSession, so without this
    // a TCP drop while editing would silently lose the work even though the
    // linkdead window keeps the character around for 3 minutes.
    force_save_editor(connection_id, &connections, &state);

    // Clean up character on unexpected disconnect (TCP close without logout/quit)
    // Mark as linkdead and wait 3 minutes before final cleanup.
    let character_present = {
        let mut conns = connections.lock();
        if let Some(session) = conns.get_mut(&connection_id) {
            if session.character.is_some() {
                session.disconnected_at = Some(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64,
                );
                true
            } else {
                // Unauthenticated connection: remove immediately
                conns.remove(&connection_id);
                false
            }
        } else {
            false
        }
    };

    if character_present {
        let char_name_for_linkdead = {
            let conns = connections.lock();
            conns
                .get(&connection_id)
                .and_then(|s| s.character.as_ref().map(|c| c.name.clone()))
                .unwrap_or_default()
        };

        if !char_name_for_linkdead.is_empty() {
            notify_friends_linkdead(&connections, &state, &char_name_for_linkdead);
        }

        let cleanup_connections = connections.clone();
        let cleanup_state = state.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(180)).await;

            // Lock connections and check if this session still exists and is still linkdead.
            // If it was reclaimed by a reconnect, it would have been removed from the map
            // by disconnect_existing_session in login.rhai.
            let cleanup_info = {
                let mut conns = cleanup_connections.lock();
                if let Some(session) = conns.get(&connection_id) {
                    if session.disconnected_at.is_some() {
                        if let Some(ref character) = session.character {
                            let mut character = character.clone();
                            if let Some(start) = session.session_started_at {
                                let now = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs() as i64;
                                let delta = (now - start).max(0);
                                character.total_seconds_played = character.total_seconds_played.saturating_add(delta);
                            }
                            let room_id = character.current_room_id;
                            let info = Some((character, room_id));
                            conns.remove(&connection_id);
                            info
                        } else {
                            conns.remove(&connection_id);
                            None
                        }
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
                    let world = cleanup_state.lock();
                    if let Err(e) = world.db.save_character_data(character) {
                        error!("Failed to save {} on disconnect timeout: {}", char_name, e);
                    } else {
                        info!("Saved character {} on disconnect timeout", char_name);
                    }
                }

                // Broadcast disconnect message to room
                broadcast_to_room(
                    &cleanup_connections,
                    room_id,
                    format!("{} has lost their connection.", char_name),
                    Some(&char_name),
                );

                // Notify chat integrations (Matrix/Discord)
                {
                    let world = cleanup_state.lock();
                    if let Some(ref chat_tx) = world.chat_sender {
                        let _ = chat_tx.send(chat::ChatMessage::Broadcast(format!(
                            "{} has lost their connection.",
                            char_name
                        )));
                    }
                }

                info!("Cleaned up disconnected player after timeout: {}", char_name);
            }
        });
    }
}

/// Emits `InputEvent::Disconnect` when dropped, unblocking the main connection
/// loop on every read-task exit path (idle, EOF, error, oversized line, unwind).
/// Without this the session's `input_sender` clone keeps the channel open and
/// the loop blocks on `recv()` forever, leaking the session into
/// `SharedConnections` (showing as a permanent `[Idle]` player in `who`).
struct DisconnectOnDrop(mpsc::Sender<InputEvent>);
impl Drop for DisconnectOnDrop {
    fn drop(&mut self) {
        // Bounded channel: full or closed are both fine — ignore the error.
        let _ = self.0.try_send(InputEvent::Disconnect);
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
    tx_input: mpsc::Sender<InputEvent>,
    connections: SharedConnections,
    tx_raw: mpsc::UnboundedSender<Vec<u8>>,
    db: crate::db::Db,
    mut removed: tokio::sync::oneshot::Receiver<()>,
) {
    use crate::conn_read::{self, ReadCx, ReadFlow};
    use crate::telnet::{TelnetParser, build_initial_negotiations};

    // Fires on every exit path so the main loop breaks and cleans up the session.
    let _disconnect_guard = DisconnectOnDrop(tx_input.clone());
    let cx = ReadCx {
        addr,
        connection_id,
        connections: &connections,
        tx_input: &tx_input,
        tx_raw: &tx_raw,
    };

    let mut parser = TelnetParser::new();
    let mut buf = [0u8; 256];
    // Line mode: raw bytes of the current line, decoded as UTF-8 at newline.
    let mut line_buffer: Vec<u8> = Vec::new();

    let _ = tx_raw.send(build_initial_negotiations());

    // Clients that don't answer telnet negotiation within 500ms get line mode.
    let negotiation_deadline = tokio::time::Instant::now() + tokio::time::Duration::from_millis(500);
    let mut negotiation_complete = false;
    let mut char_mode_enabled = false;

    loop {
        // Check if connection was removed by script (e.g., quit command)
        if !connections.lock().contains_key(&connection_id) {
            info!("Read task: connection {} removed, exiting.", addr);
            break;
        }
        if conn_read::idle_timed_out(&cx, &db) {
            break;
        }

        let read_result = if !negotiation_complete && tokio::time::Instant::now() < negotiation_deadline {
            match tokio::time::timeout(
                negotiation_deadline - tokio::time::Instant::now(),
                reader.read(&mut buf),
            )
            .await
            {
                Ok(result) => result,
                Err(_) => {
                    // Negotiation window over - use whatever mode was agreed.
                    negotiation_complete = true;
                    char_mode_enabled = conn_read::session_char_mode(&cx);
                    if !char_mode_enabled {
                        info!("Client {} using line mode (no telnet negotiation response)", addr);
                    }
                    continue;
                }
            }
        } else {
            // Wake on input, on the session being removed (its signal sender
            // drops), or every IDLE_CHECK_SECS to re-run the idle check above.
            // `read` on a TcpStream is cancel-safe, so losing the race to the
            // other branches drops no bytes.
            tokio::select! {
                result = reader.read(&mut buf) => result,
                _ = &mut removed => {
                    info!("Read task: connection {} removed, exiting.", addr);
                    break;
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(IDLE_CHECK_SECS)) => continue,
            }
        };

        let n = match read_result {
            Ok(0) => break, // Connection closed
            Ok(n) => n,
            Err(e) => {
                error!("Failed to read from socket {}: {}", addr, e);
                break;
            }
        };

        let (data, events) = parser.process_bytes(&buf[..n]);
        let had_events = !events.is_empty();
        for event in events {
            if conn_read::handle_telnet_event(&cx, event) {
                char_mode_enabled = true;
            }
        }

        // Mark negotiation complete after first data or if we got responses
        if !negotiation_complete && (!data.is_empty() || had_events) {
            char_mode_enabled = conn_read::session_char_mode(&cx);
            negotiation_complete = true;
        }

        for byte in data {
            let flow = if char_mode_enabled {
                match conn_read::parse_key(&cx, byte) {
                    Some(key) => conn_read::handle_key(&cx, key),
                    None => ReadFlow::Continue,
                }
            } else {
                conn_read::handle_line_byte(&cx, &mut line_buffer, byte)
            };
            if flow == ReadFlow::Disconnect {
                return;
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
    drain: session::DrainHandle,
) {
    use tokio::time::timeout;
    let write_timeout = session::outbox::WRITE_TIMEOUT;
    loop {
        if drain.is_overflowed() {
            warn!("Client {} stopped reading output; closing its socket.", addr);
            break;
        }
        // `biased` drains rx_raw before rx_client so the \r\n emitted when the
        // user hits Enter can't lose a race to the command's response text and
        // end up written after it (which glued responses onto the echoed line).
        tokio::select! {
            biased;
            Some(raw) = rx_raw.recv() => {
                match timeout(write_timeout, writer.write_all(&raw)).await {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        error!("Failed to write raw bytes to socket {}: {}", addr, e);
                        break;
                    }
                    Err(_) => {
                        warn!("Write to {} timed out; closing its socket.", addr);
                        break;
                    }
                }
                let _ = timeout(write_timeout, writer.flush()).await;
            }
            Some(msg) = rx_client.recv() => {
                drain.mark_written(msg.len());
                // Convert \n to \r\n for proper telnet line endings
                // First normalize any existing \r\n to \n, then convert all \n to \r\n
                let telnet_msg = msg.replace("\r\n", "\n").replace("\n", "\r\n");
                match timeout(write_timeout, writer.write_all(telnet_msg.as_bytes())).await {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        error!("Failed to write to socket {}: {}", addr, e);
                        break;
                    }
                    Err(_) => {
                        warn!("Write to {} timed out; closing its socket.", addr);
                        break;
                    }
                }
                let _ = timeout(write_timeout, writer.flush()).await;
            }
            else => break,
        }
    }
    info!("Client {} disconnected (write handler).", addr);
}

pub async fn run_server(state: SharedState, listener: TcpListener, shutdown_rx: tokio::sync::oneshot::Receiver<()>) {
    let limiter = {
        let world = state.lock();
        world.ip_limiter.clone()
    };
    // Clone the Db arc once for site-ban lookups in the accept loop. Keeping
    // it outside the loop avoids relocking the World mutex on every accept.
    let db_for_bans = {
        let world = state.lock();
        world.db.clone()
    };
    tokio::select! {
        _ = async {
            loop {
                // Never panic on accept: EMFILE/ENFILE (fd exhaustion) or
                // ECONNABORTED are transient, and a panic here would kill the
                // listener task for good while the rest of the server runs on.
                let (mut socket, addr) = match listener.accept().await {
                    Ok(pair) => pair,
                    Err(e) => {
                        warn!("accept() failed: {e}; retrying");
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        continue;
                    }
                };

                // Per-IP connection cap. An attacker that opens hundreds of
                // sockets from one IP would otherwise exhaust file descriptors
                // and per-session memory.
                if !limiter.try_acquire(addr.ip()) {
                    info!("Rejecting connection from {} -- IP at simultaneous-connection cap.", addr);
                    let _ = socket
                        .write_all(b"Too many connections from your address. Try again later.\r\n")
                        .await;
                    let _ = socket.shutdown().await;
                    continue;
                }

                // Site-ban gate. Refused IPs see a generic refusal — we don't
                // confirm "you are banned" so admins can quietly block scrapers
                // without painting a target. Lazy expiry inside get_site_ban
                // means a banned IP becomes unbanned automatically when the
                // expires_at passes (next connect attempt succeeds).
                if let Ok(Some(record)) = db_for_bans.get_site_ban(&addr.ip().to_string()) {
                    info!(
                        "Refusing connection from site-banned IP {} (reason: {})",
                        addr.ip(),
                        record.reason
                    );
                    let _ = socket
                        .write_all(b"Connections from your address are not permitted on this server.\r\n")
                        .await;
                    let _ = socket.shutdown().await;
                    limiter.release(addr.ip());
                    continue;
                }
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
                let limiter_for_conn = limiter.clone();
                tokio::spawn(async move {
                    // Release on drop, so the per-IP slot (paired with the
                    // try_acquire above) is freed even if handle_connection
                    // returns early or panics.
                    let _slot = ratelimit::ConnectionSlot::new(limiter_for_conn, addr.ip());
                    handle_connection(socket, addr, state, connection_id).await;
                });
            }
        } => {},
        _ = shutdown_rx => {
            info!("Shutting down server...");
        }
    }
}

/// Notify friends when a player enters the Linkdead state
fn notify_friends_linkdead(connections: &SharedConnections, state: &SharedState, char_name: &str) {
    // Clone the Db handle and release World before the full character scan,
    // and never hold World while taking Connections.
    let db = state.lock().db.clone();
    if let Ok(all_chars) = db.list_all_characters() {
        let conns = connections.lock();
        for char_data in all_chars {
            if char_data.friends.iter().any(|f| f.eq_ignore_ascii_case(char_name)) {
                if char_data.ignored.iter().all(|i| !i.eq_ignore_ascii_case(char_name)) {
                    for session in conns.values() {
                        if let Some(ref c) = session.character {
                            if c.name == char_data.name && session.disconnected_at.is_none() {
                                let _ = session.sender.send(format!(
                                    "\x1b[1;33m[Friend] {} has lost their connection.\x1b[0m\n",
                                    char_name
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod guarded_call_tests {
    use super::*;

    /// A panic inside a native Rhai function must surface as a script error,
    /// not unwind out of handle_connection (which would skip session cleanup).
    #[test]
    fn panicking_native_fn_becomes_a_script_error() {
        let mut engine = Engine::new();
        engine.register_fn("boom", || -> i64 { panic!("native fn panic") });
        let ast = engine
            .compile("fn run_command(args, connection_id) { boom() }")
            .expect("compile");
        let mut scope = rhai::Scope::new();
        let result = call_run_command_guarded(&engine, &mut scope, &ast, String::new(), String::new(), "boom");
        assert_eq!(result.unwrap_err(), "an internal error occurred");
    }
}

#[cfg(test)]
mod lock_order_tests {
    use super::*;
    use std::time::Duration;

    /// Regression: `sync_msdp_vitals` is called while the Connections lock is
    /// held. It used to take the World lock for ROOM_NAME / OPPONENT_*, which
    /// inverted the World -> Connections order used by tab completion and
    /// deadlocked the server. It must now complete while another thread holds
    /// World.
    #[test]
    fn sync_msdp_vitals_does_not_take_world_lock() {
        let temp = tempfile::tempdir().expect("temp dir");
        let db = crate::db::Db::open(temp.path()).expect("open db");
        let conns: SharedConnections = Arc::new(Mutex::new(HashMap::new()));
        let state = World::minimal_shared(db.clone(), conns.clone());

        let (tx_client, _rx) = mpsc::unbounded_channel::<String>();
        let (tx_input, _rx_in) = mpsc::channel::<InputEvent>(1);
        let (tx_raw, mut rx_raw) = mpsc::unbounded_channel::<Vec<u8>>();
        let mut session = PlayerSession::new_for_test(tx_client, tx_input);
        session.raw_sender = Some(tx_raw);
        session.telnet_state.msdp_supported = true;
        for v in ["ROOM_NAME", "OPPONENT_NAME", "OPPONENT_HEALTH", "HEALTH"] {
            session.telnet_state.msdp_reported_variables.insert(v.to_string());
        }
        let ch: CharacterData = serde_json::from_value(serde_json::json!({
            "name": "Msdp",
            "password_hash": "",
            "current_room_id": Uuid::new_v4(),
        }))
        .expect("build character");
        session.character = Some(ch);

        // Hold World for the whole call, as tab completion would.
        let _world = state.lock();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            session.sync_msdp_vitals(&db);
            let _ = done_tx.send(());
        });
        done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("sync_msdp_vitals blocked on the World lock");
        assert!(rx_raw.try_recv().is_ok(), "expected MSDP output");
    }
}

#[cfg(test)]
mod read_task_tests {
    use super::*;

    /// Removing a session from `SharedConnections` wakes its read task at
    /// once (the session's `removed_signal` sender drops), rather than on the
    /// next poll or idle tick.
    #[tokio::test]
    async fn removing_the_session_ends_the_read_task_promptly() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let _client = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (server_side, peer) = listener.accept().await.unwrap();
        let (reader, _writer) = server_side.into_split();

        let temp = tempfile::tempdir().unwrap();
        let db = crate::db::Db::open(temp.path()).unwrap();
        let connections: SharedConnections = Arc::new(Mutex::new(HashMap::new()));
        let (tx_client, _rx_client) = mpsc::unbounded_channel();
        let (tx_input, _rx_input) = mpsc::channel(8);
        let (tx_raw, _rx_raw) = mpsc::unbounded_channel();
        let (removed_tx, removed_rx) = tokio::sync::oneshot::channel();
        let id = Uuid::new_v4();
        let mut session = PlayerSession::new_for_test(tx_client, tx_input.clone());
        session.removed_signal = Some(removed_tx);
        session.last_activity_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        connections.lock().insert(id, session);

        let task = tokio::spawn(handle_read_char_mode(
            reader,
            peer,
            id,
            tx_input,
            connections.clone(),
            tx_raw,
            db,
            removed_rx,
        ));
        // Past the 500ms negotiation window, into the steady-state select.
        tokio::time::sleep(std::time::Duration::from_millis(700)).await;
        assert!(!task.is_finished(), "still reading while the session exists");

        connections.lock().remove(&id);
        tokio::time::timeout(std::time::Duration::from_secs(2), task)
            .await
            .expect("read task ended within 2s of removal (idle tick is 30s)")
            .unwrap();
    }
}

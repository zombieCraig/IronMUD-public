use anyhow::Result;
use clap::Parser;
use rhai::Engine;
use rhai::module_resolvers::FileModuleResolver;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio::time::Duration;
use tracing::{error, info};

mod ticks;

use ironmud::{
    SharedConnections, ShutdownCommand, World, broadcast_to_all_players,
    chat::ChatMessage,
    claude::{ClaudeConfig, ClaudeRequest, run_claude_task},
    db,
    discord::{DiscordConfig, run_discord_bot},
    game::Entity,
    gemini::{GeminiConfig, GeminiRequest, run_gemini_task},
    load_command_metadata, load_game_data, load_scripts,
    matrix::{MatrixConfig, run_matrix_bot},
    run_server, save_all_players, watch_scripts,
};

use ironmud::script;
use ticks::{
    run_aging_tick, run_bleeding_tick, run_combat_tick, run_corpse_decay_tick, run_drowning_tick, run_exposure_tick,
    run_garden_tick, run_hunger_tick, run_hunting_tick, run_migration_tick, run_mobile_effects_tick,
    run_periodic_trigger_tick, run_pursuit_tick, run_regen_tick, run_rent_tick, run_routine_tick, run_simulation_tick,
    run_spawn_tick, run_spoilage_tick, run_thirst_tick, run_time_tick, run_transport_tick, run_wander_tick,
};

#[derive(Parser, Debug)]
#[command(name = "ironmud")]
#[command(about = "IronMUD Game Server")]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value = "4000")]
    port: u16,

    /// Database path
    #[arg(short, long, default_value = "ironmud.db", env = "IRONMUD_DATABASE")]
    database: String,

    /// REST API port (requires IRONMUD_API_ENABLED=true)
    #[arg(long, default_value = "4001")]
    api_port: u16,

    /// REST API bind address (e.g. 127.0.0.1 to restrict to loopback)
    #[arg(long, default_value = "0.0.0.0", env = "IRONMUD_API_BIND")]
    api_bind: String,

    /// Unix control socket path (defaults to <database-dir>/control.sock)
    #[arg(long, env = "IRONMUD_CONTROL_SOCKET")]
    control_socket: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt::init();
    info!("Starting IronMUD server...");

    let mut engine = Engine::new();
    engine.set_max_expr_depths(128, 128);
    engine.set_max_operations(1_000_000);
    engine.set_max_string_size(1_000_000); // 1MB max string
    engine.set_max_array_size(10_000);
    engine.set_max_map_size(10_000);

    // Set up module resolver for shared Rhai libraries (scripts/lib/*.rhai)
    let mut resolver = FileModuleResolver::new();
    resolver.set_base_path("scripts/lib");
    engine.set_module_resolver(resolver);

    let db = crate::db::Db::open(&args.database)?;

    // Seed demo world on first startup (or after world clear)
    match ironmud::seed::seed_demo_world(&db) {
        Ok(true) => info!("Demo world seeded on first startup"),
        Ok(false) => {} // Already exists
        Err(e) => error!("Failed to seed demo world: {}", e),
    }

    // Migrate character keys to lowercase for case-insensitive lookup
    db.migrate_character_keys_to_lowercase()?;

    // Migrate characters with invalid room IDs to starting room
    db.migrate_characters_to_valid_rooms()?;

    // Rebuild vnum index from existing room data
    db.rebuild_vnum_index()?;

    let cloned_db_for_rhai = db.clone(); // Clone db for Rhai registration
    let tick_db = db.clone(); // Clone db for spawn tick
    let tick_db2 = db.clone(); // Clone db for periodic triggers
    let tick_db3 = db.clone(); // Clone db for time tick
    let tick_db4 = db.clone(); // Clone db for thirst tick
    let tick_db5 = db.clone(); // Clone db for regen tick
    let tick_db6 = db.clone(); // Clone db for wander tick
    let tick_db7 = db.clone(); // Clone db for combat tick
    let tick_db8 = db.clone(); // Clone db for corpse decay tick
    let tick_db9 = db.clone(); // Clone db for exposure tick
    let tick_db10 = db.clone(); // Clone db for transport tick
    let tick_db11 = db.clone(); // Clone db for rent tick
    let tick_db12 = db.clone(); // Clone db for hunger tick
    let tick_db13 = db.clone(); // Clone db for spoilage tick
    let tick_db14 = db.clone(); // Clone db for mobile effects tick
    let tick_db15 = db.clone(); // Clone db for pursuit tick
    let tick_db16 = db.clone(); // Clone db for routine tick
    let tick_db17 = db.clone(); // Clone db for garden tick
    let tick_db18 = db.clone(); // Clone db for hunting tick
    let tick_db19 = db.clone(); // Clone db for drowning tick
    let tick_db20 = db.clone(); // Clone db for bleeding tick
    let tick_db21 = db.clone(); // Clone db for simulation tick
    let tick_db22 = db.clone(); // Clone db for migration tick
    let tick_db23 = db.clone(); // Clone db for aging tick
    let api_db = db.clone(); // Clone db for REST API

    let connections = Arc::new(Mutex::new(HashMap::new()));
    let command_metadata = load_command_metadata()?;

    let state = Arc::new(Mutex::new(World {
        engine,
        db,
        connections: connections.clone(),
        scripts: HashMap::new(),
        command_metadata,
        class_definitions: HashMap::new(),
        trait_definitions: HashMap::new(),
        race_suggestions: Vec::new(),
        race_definitions: HashMap::new(),
        recipes: HashMap::new(),
        spell_definitions: HashMap::new(),
        transports: HashMap::new(),
        chat_sender: None,            // Set after chat bridge channel is created
        shutdown_sender: None,        // Set after shutdown channel is created
        shutdown_cancel_sender: None, // Set after shutdown channel is created
    }));

    // Register types and functions
    {
        let mut world = state.lock().unwrap();
        world
            .engine
            .register_type_with_name::<Entity>("Entity")
            .register_get("id", |entity: &mut Entity| entity.id.to_string())
            .register_get("name", |entity: &mut Entity| entity.name.clone())
            .register_get("description", |entity: &mut Entity| entity.description.clone());

        // Register Rhai functions - pass SharedConnections and SharedState
        script::register_rhai_functions(
            &mut world.engine,
            Arc::new(cloned_db_for_rhai),
            connections.clone(),
            state.clone(),
        );
    }

    // Load and watch scripts
    load_scripts(state.clone())?;
    load_game_data(state.clone())?;
    watch_scripts(state.clone());

    // Start background spawn tick
    let spawn_connections = connections.clone();
    tokio::spawn(async move {
        run_spawn_tick(tick_db, spawn_connections).await;
    });

    // Start background periodic trigger tick
    let tick_connections = connections.clone();
    tokio::spawn(async move {
        run_periodic_trigger_tick(tick_db2, tick_connections).await;
    });

    // Start background time tick (game time advancement)
    let time_connections = connections.clone();
    tokio::spawn(async move {
        run_time_tick(tick_db3, time_connections).await;
    });

    // Start background thirst tick (player thirst updates)
    let thirst_connections = connections.clone();
    tokio::spawn(async move {
        run_thirst_tick(tick_db4, thirst_connections).await;
    });

    // Start background regen tick (stamina and HP regeneration)
    let regen_connections = connections.clone();
    tokio::spawn(async move {
        run_regen_tick(tick_db5, regen_connections).await;
    });

    // Start background wander tick (mobile wandering)
    let wander_connections = connections.clone();
    tokio::spawn(async move {
        run_wander_tick(tick_db6, wander_connections).await;
    });

    // Start background combat tick (combat round processing)
    let combat_connections = connections.clone();
    let combat_state = state.clone();
    tokio::spawn(async move {
        run_combat_tick(tick_db7, combat_connections, combat_state).await;
    });

    // Start background corpse decay tick (corpse cleanup)
    let corpse_connections = connections.clone();
    tokio::spawn(async move {
        run_corpse_decay_tick(tick_db8, corpse_connections).await;
    });

    // Start background exposure tick (weather/temperature effects)
    let exposure_connections = connections.clone();
    tokio::spawn(async move {
        run_exposure_tick(tick_db9, exposure_connections).await;
    });

    // Start background transport tick (elevator/bus/train movement)
    let transport_connections = connections.clone();
    tokio::spawn(async move {
        run_transport_tick(tick_db10, transport_connections).await;
    });

    // Start background rent tick (property rent auto-payment)
    let rent_connections = connections.clone();
    tokio::spawn(async move {
        run_rent_tick(tick_db11, rent_connections).await;
    });

    // Start background hunger tick (player hunger updates)
    let hunger_connections = connections.clone();
    tokio::spawn(async move {
        run_hunger_tick(tick_db12, hunger_connections).await;
    });

    // Start background spoilage tick (food spoilage accumulation)
    let spoilage_connections = connections.clone();
    tokio::spawn(async move {
        run_spoilage_tick(tick_db13, spoilage_connections).await;
    });

    // Start background mobile effects tick (poison emotes, etc.)
    let mobile_effects_connections = connections.clone();
    tokio::spawn(async move {
        run_mobile_effects_tick(tick_db14, mobile_effects_connections).await;
    });

    // Start background pursuit tick (mob pursuit after sniping)
    let pursuit_connections = connections.clone();
    tokio::spawn(async move {
        run_pursuit_tick(tick_db15, pursuit_connections).await;
    });

    // Start background routine tick (mobile daily routines)
    let routine_connections = connections.clone();
    tokio::spawn(async move {
        run_routine_tick(tick_db16, routine_connections).await;
    });

    // Start background garden tick (plant growth and maintenance)
    let garden_connections = connections.clone();
    tokio::spawn(async move {
        run_garden_tick(tick_db17, garden_connections).await;
    });

    // Start background hunting tick (player auto-follow while hunting)
    let hunting_connections = connections.clone();
    tokio::spawn(async move {
        run_hunting_tick(tick_db18, hunting_connections).await;
    });

    // Start background drowning tick (breath depletion and drowning damage)
    let drowning_connections = connections.clone();
    tokio::spawn(async move {
        run_drowning_tick(tick_db19, drowning_connections).await;
    });

    // Start background bleeding tick (wound bleeding damage)
    let bleeding_connections = connections.clone();
    tokio::spawn(async move {
        run_bleeding_tick(tick_db20, bleeding_connections).await;
    });

    // Start background simulation tick (NPC needs simulation)
    let simulation_connections = connections.clone();
    tokio::spawn(async move {
        run_simulation_tick(tick_db21, simulation_connections).await;
    });

    // Start background migration tick (emergent migrant population)
    let migration_connections = connections.clone();
    let migration_data_dir = std::path::PathBuf::from("scripts/data");
    tokio::spawn(async move {
        run_migration_tick(tick_db22, migration_connections, migration_data_dir).await;
    });

    // Start background aging tick (advance age, old-age natural death)
    let aging_connections = connections.clone();
    tokio::spawn(async move {
        run_aging_tick(tick_db23, aging_connections).await;
    });

    // Start control socket listener for out-of-process admin commands.
    #[cfg(unix)]
    {
        let socket_path = args
            .control_socket
            .clone()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| ironmud::control::default_socket_path(&args.database));
        let control_connections = connections.clone();
        tokio::spawn(async move {
            if let Err(e) = ironmud::control::run_control_socket(socket_path, control_connections).await {
                error!("Control socket terminated: {}", e);
            }
        });
    }

    // Create a dummy entity for the look command
    // This section will need to be refactored or removed as part of character system integration
    /*
    let entity_id = Uuid::new_v4();
    let entity = Entity {
        id: entity_id,
        name: "The Void".to_string(),
        description: "A vast, empty void.".to_string(),
    };
    {
        let world = state.lock().unwrap();
        let serialized_entity = serde_json::to_vec(&entity)?;
        world.db.insert(entity_id.as_bytes(), serialized_entity)?;
    }
    */

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let state2 = state.clone();
    let addr = format!("0.0.0.0:{}", args.port);
    let telnet_server = tokio::spawn(async move {
        let listener = TcpListener::bind(&addr).await.unwrap();
        info!("IronMUD Gateway listening on {}", addr);
        run_server(state2, listener, shutdown_rx).await;
    });

    // Start REST API if enabled via environment variable
    // Disabled by default - set IRONMUD_API_ENABLED=true to enable
    if std::env::var("IRONMUD_API_ENABLED")
        .map(|v| v == "true" || v == "1" || v == "yes")
        .unwrap_or(false)
    {
        let api_connections = connections.clone();
        let api_port = args.api_port;
        let api_bind = args.api_bind.clone();
        tokio::spawn(async move {
            let api_state = std::sync::Arc::new(ironmud::api::ApiState {
                db: api_db,
                connections: api_connections,
            });
            ironmud::api::run_api_server(api_state, &api_bind, api_port).await;
        });
        info!("REST API listening on {}:{}", args.api_bind, args.api_port);
    } else {
        info!("REST API disabled (set IRONMUD_API_ENABLED=true to enable)");
    }

    // Create chat bridge channel (single channel for all chat backends)
    let (chat_tx, chat_rx) = tokio::sync::mpsc::unbounded_channel::<ChatMessage>();
    let mut backend_senders: Vec<tokio::sync::mpsc::UnboundedSender<ChatMessage>> = Vec::new();

    // Matrix backend
    if let Some(matrix_config) = MatrixConfig::from_env() {
        let (matrix_tx, matrix_rx) = tokio::sync::mpsc::unbounded_channel::<ChatMessage>();
        backend_senders.push(matrix_tx);
        let matrix_connections = connections.clone();
        tokio::spawn(async move {
            run_matrix_bot(matrix_config, matrix_connections, matrix_rx).await;
        });
        info!("Matrix bot started");
    } else {
        info!("Matrix integration disabled (MATRIX_HOMESERVER, MATRIX_USER, MATRIX_PASSWORD, MATRIX_ROOM not all set)");
    }

    // Discord backend
    if let Some(discord_config) = DiscordConfig::from_env() {
        let (discord_tx, discord_rx) = tokio::sync::mpsc::unbounded_channel::<ChatMessage>();
        backend_senders.push(discord_tx);
        let discord_connections = connections.clone();
        tokio::spawn(async move {
            run_discord_bot(discord_config, discord_connections, discord_rx).await;
        });
        info!("Discord bot started");
    } else {
        info!("Discord integration disabled (DISCORD_TOKEN, DISCORD_CHANNEL_ID not all set)");
    }

    // Start chat bridge (fans out messages to all backends)
    if !backend_senders.is_empty() {
        tokio::spawn(async move {
            ironmud::chat::run_chat_bridge(chat_rx, backend_senders).await;
        });
    } else {
        drop(chat_rx);
    }

    // Keep a clone for shutdown notification
    let shutdown_chat_tx = chat_tx.clone();

    // Store chat sender in World for disconnect notifications, and register with Rhai
    {
        let mut world = state.lock().unwrap();
        world.chat_sender = Some(chat_tx.clone());
        script::set_chat_sender(&mut world.engine, chat_tx);
    }

    // Create shutdown command channel for admin shutdown
    let (admin_shutdown_tx, mut admin_shutdown_rx) = tokio::sync::mpsc::unbounded_channel::<ShutdownCommand>();
    // Create shutdown cancellation channel (false = not cancelled, true = cancelled)
    let (shutdown_cancel_tx, shutdown_cancel_rx) = tokio::sync::watch::channel(false);
    {
        let mut world = state.lock().unwrap();
        world.shutdown_sender = Some(admin_shutdown_tx);
        world.shutdown_cancel_sender = Some(shutdown_cancel_tx);
    }

    // AI Provider selection (Claude or Gemini, but not both)
    let claude_configured = std::env::var("CLAUDE_API_KEY").is_ok();
    let gemini_configured = std::env::var("GEMINI_API_KEY").is_ok();

    if claude_configured && gemini_configured {
        // Both configured - error and disable both
        error!(
            "Both CLAUDE_API_KEY and GEMINI_API_KEY are set. Only one AI provider can be used at a time. Disabling AI integration."
        );
        // Register a dummy Claude sender that will never receive anything
        let (claude_tx, _) = tokio::sync::mpsc::unbounded_channel::<ClaudeRequest>();
        let mut world = state.lock().unwrap();
        script::set_claude_sender(&mut world.engine, claude_tx, connections.clone());
    } else if let Some(claude_config) = ClaudeConfig::from_env() {
        // Claude enabled
        let (claude_tx, claude_rx) = tokio::sync::mpsc::unbounded_channel::<ClaudeRequest>();
        let claude_connections = connections.clone();
        tokio::spawn(async move {
            run_claude_task(claude_config, claude_connections, claude_rx).await;
        });
        info!("AI integration enabled (Claude)");
        let mut world = state.lock().unwrap();
        script::set_claude_sender(&mut world.engine, claude_tx, connections.clone());
    } else if let Some(gemini_config) = GeminiConfig::from_env() {
        // Gemini enabled
        let (gemini_tx, gemini_rx) = tokio::sync::mpsc::unbounded_channel::<GeminiRequest>();
        let gemini_connections = connections.clone();
        tokio::spawn(async move {
            run_gemini_task(gemini_config, gemini_connections, gemini_rx).await;
        });
        info!("AI integration enabled (Gemini)");
        let mut world = state.lock().unwrap();
        script::set_gemini_sender(&mut world.engine, gemini_tx, connections.clone());
    } else {
        // No AI configured - register dummy sender
        info!("AI integration disabled (no API key set)");
        let (claude_tx, _) = tokio::sync::mpsc::unbounded_channel::<ClaudeRequest>();
        let mut world = state.lock().unwrap();
        script::set_claude_sender(&mut world.engine, claude_tx, connections.clone());
    }

    // Wait for shutdown signal (SIGINT, SIGTERM, or admin command)
    // Loop to handle cancelled shutdowns (allows re-waiting for signals)
    let shutdown_connections = connections.clone();
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm = signal(SignalKind::terminate()).expect("Failed to set up SIGTERM handler");
        let mut sigint = signal(SignalKind::interrupt()).expect("Failed to set up SIGINT handler");

        loop {
            tokio::select! {
                _ = sigint.recv() => {
                    info!("SIGINT received, shutting down...");
                    break;
                }
                _ = sigterm.recv() => {
                    info!("SIGTERM received, shutting down...");
                    break;
                }
                Some(cmd) = admin_shutdown_rx.recv() => {
                    info!("Admin shutdown initiated by {}: {}", cmd.admin_name, cmd.reason);
                    let was_cancelled = run_shutdown_countdown(
                        &shutdown_connections,
                        &shutdown_chat_tx,
                        cmd,
                        shutdown_cancel_rx.clone(),
                    ).await;
                    if !was_cancelled {
                        break; // Proceed with shutdown
                    }
                    // Reset the cancel flag for next shutdown attempt
                    if let Ok(world) = state.lock() {
                        if let Some(ref sender) = world.shutdown_cancel_sender {
                            let _ = sender.send(false);
                        }
                    }
                    info!("Shutdown cancelled, resuming normal operation");
                    // Continue loop to wait for next signal
                }
            }
        }
    }

    #[cfg(not(unix))]
    {
        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    info!("Ctrl-C received, shutting down...");
                    break;
                }
                Some(cmd) = admin_shutdown_rx.recv() => {
                    info!("Admin shutdown initiated by {}: {}", cmd.admin_name, cmd.reason);
                    let was_cancelled = run_shutdown_countdown(
                        &shutdown_connections,
                        &shutdown_chat_tx,
                        cmd,
                        shutdown_cancel_rx.clone(),
                    ).await;
                    if !was_cancelled {
                        break; // Proceed with shutdown
                    }
                    // Reset the cancel flag for next shutdown attempt
                    if let Ok(world) = state.lock() {
                        if let Some(ref sender) = world.shutdown_cancel_sender {
                            let _ = sender.send(false);
                        }
                    }
                    info!("Shutdown cancelled, resuming normal operation");
                    // Continue loop to wait for next signal
                }
            }
        }
    }

    // Notify all connected players about shutdown
    broadcast_to_all_players(
        &connections,
        "\n*** SERVER NOTICE: The server is shutting down. Your progress will be saved. ***\n",
    );

    // Notify chat integrations about shutdown
    let _ = shutdown_chat_tx.send(ChatMessage::Broadcast("IronMUD server is shutting down.".to_string()));

    // Give chat backends a moment to send the message
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Signal the server to stop accepting new connections
    let _ = shutdown_tx.send(());

    // Wait for server to finish current connections
    telnet_server.await?;

    // Graceful shutdown: save all active players
    info!("Saving all active players...");
    let saved = {
        let world = state.lock().unwrap();
        save_all_players(&connections, &world.db)
    };
    info!("Saved {} player(s)", saved);

    // Flush database to disk
    info!("Flushing database to disk...");
    {
        let world = state.lock().unwrap();
        if let Err(e) = world.db.flush() {
            error!("Failed to flush database: {}", e);
        }
    }

    info!("Shutdown complete");
    Ok(())
}

/// Run shutdown countdown, broadcasting warnings to all players
/// Returns true if the shutdown was cancelled, false if it should proceed
async fn run_shutdown_countdown(
    connections: &SharedConnections,
    chat_tx: &tokio::sync::mpsc::UnboundedSender<ChatMessage>,
    cmd: ShutdownCommand,
    mut cancel_rx: tokio::sync::watch::Receiver<bool>,
) -> bool {
    let mut remaining = cmd.delay_seconds;

    // Initial announcement
    let initial_msg = format!(
        "\n*** SERVER SHUTDOWN INITIATED ***\nReason: {}\nInitiated by: {}\nServer will shut down in {} seconds.\nType 'admin cancel' to abort.\n",
        cmd.reason, cmd.admin_name, remaining
    );
    broadcast_to_all_players(connections, &initial_msg);
    let _ = chat_tx.send(ChatMessage::Broadcast(format!(
        "Server shutdown initiated by {} ({}). Shutting down in {} seconds.",
        cmd.admin_name, cmd.reason, remaining
    )));

    // Countdown milestones to announce
    let milestones = [300, 180, 120, 60, 30, 15, 10, 5, 4, 3, 2, 1];

    while remaining > 0 {
        // Check if cancelled
        if *cancel_rx.borrow() {
            broadcast_to_all_players(connections, "\n*** SERVER SHUTDOWN CANCELLED ***\n");
            let _ = chat_tx.send(ChatMessage::Broadcast(
                "Server shutdown has been cancelled.".to_string(),
            ));
            return true;
        }

        // Find the next milestone
        let next_milestone = milestones
            .iter()
            .filter(|&&m| m < remaining)
            .max()
            .copied()
            .unwrap_or(0);

        // Sleep until the next milestone (or until done), but check for cancellation
        let sleep_time = remaining - next_milestone;
        if sleep_time > 0 {
            // Sleep in 1-second intervals to check for cancellation more frequently
            for _ in 0..sleep_time {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                    _ = cancel_rx.changed() => {
                        if *cancel_rx.borrow() {
                            broadcast_to_all_players(connections, "\n*** SERVER SHUTDOWN CANCELLED ***\n");
                            let _ = chat_tx.send(ChatMessage::Broadcast("Server shutdown has been cancelled.".to_string()));
                            return true;
                        }
                    }
                }
            }
            remaining = next_milestone;
        }

        // Announce if we hit a milestone
        if remaining > 0 && milestones.contains(&remaining) {
            let unit = if remaining == 1 { "second" } else { "seconds" };
            let msg = format!("\n*** SERVER SHUTDOWN in {} {} ***\n", remaining, unit);
            broadcast_to_all_players(connections, &msg);
        }

        // If we're at 0 or below a milestone, break to continue shutdown
        if remaining == 0 || !milestones.contains(&remaining) {
            break;
        }
    }

    // Final countdown completed
    broadcast_to_all_players(connections, "\n*** SERVER SHUTDOWN NOW ***\n");
    false // Not cancelled, proceed with shutdown
}

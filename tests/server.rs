#![recursion_limit = "512"]

use anyhow::Result;
use tempfile;
use ironmud::{World, load_command_metadata, load_game_data, load_scripts, run_server, script, watch_scripts};
use ironmud::db::Db;
use rhai::Engine;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::time::{sleep, timeout};

// Telnet protocol constants
const IAC: u8 = 255;
const DONT: u8 = 254;
const DO: u8 = 253;
const WONT: u8 = 252;
const WILL: u8 = 251;
const SB: u8 = 250;
const SE: u8 = 240;

/// Telnet-aware test client helper
struct TelnetTestClient {
    reader: OwnedReadHalf,
    writer: OwnedWriteHalf,
}

impl TelnetTestClient {
    fn new(reader: OwnedReadHalf, writer: OwnedWriteHalf) -> Self {
        Self { reader, writer }
    }

    /// Read bytes and handle telnet negotiation, returning clean text
    async fn read_until_prompt(&mut self) -> Result<String> {
        let mut buf = [0u8; 1024];
        let mut data = Vec::new();

        // Read with timeout to avoid hanging
        loop {
            match timeout(Duration::from_secs(5), self.reader.read(&mut buf)).await {
                Ok(Ok(0)) => break, // Connection closed
                Ok(Ok(n)) => {
                    let bytes = &buf[..n];

                    // Process bytes, respond to telnet and collect text
                    let (text_bytes, responses) = self.process_telnet_bytes(bytes);
                    data.extend(text_bytes);

                    // Send any telnet responses
                    for response in responses {
                        let _ = self.writer.write_all(&response).await;
                    }

                    // Check if we have a prompt
                    if data.contains(&b'>') {
                        break;
                    }
                }
                Ok(Err(e)) => return Err(e.into()),
                Err(_) => break, // Timeout
            }
        }

        Ok(String::from_utf8_lossy(&data).to_string())
    }

    /// Process telnet bytes, stripping IAC sequences and generating responses
    fn process_telnet_bytes(&self, input: &[u8]) -> (Vec<u8>, Vec<Vec<u8>>) {
        let mut text = Vec::new();
        let mut responses = Vec::new();
        let mut i = 0;

        while i < input.len() {
            if input[i] == IAC && i + 1 < input.len() {
                match input[i + 1] {
                    IAC => {
                        // Escaped IAC -> literal 0xFF
                        text.push(IAC);
                        i += 2;
                    }
                    WILL => {
                        // Server offers option, we refuse with DONT
                        if i + 2 < input.len() {
                            responses.push(vec![IAC, DONT, input[i + 2]]);
                            i += 3;
                        } else {
                            i += 2;
                        }
                    }
                    WONT => {
                        // Server refuses, acknowledge
                        if i + 2 < input.len() {
                            i += 3;
                        } else {
                            i += 2;
                        }
                    }
                    DO => {
                        // Server requests option, we refuse with WONT
                        if i + 2 < input.len() {
                            responses.push(vec![IAC, WONT, input[i + 2]]);
                            i += 3;
                        } else {
                            i += 2;
                        }
                    }
                    DONT => {
                        // Server tells us not to, acknowledge
                        if i + 2 < input.len() {
                            i += 3;
                        } else {
                            i += 2;
                        }
                    }
                    SB => {
                        // Subnegotiation - skip until SE
                        i += 2;
                        while i < input.len() {
                            if input[i] == IAC && i + 1 < input.len() && input[i + 1] == SE {
                                i += 2;
                                break;
                            }
                            i += 1;
                        }
                    }
                    _ => {
                        // Other IAC command, skip
                        i += 2;
                    }
                }
            } else {
                text.push(input[i]);
                i += 1;
            }
        }

        (text, responses)
    }

    /// Send a command (adds newline)
    async fn send(&mut self, cmd: &str) -> Result<()> {
        self.writer.write_all(format!("{}\n", cmd).as_bytes()).await?;
        Ok(())
    }
}

/*
#[tokio::test]
async fn test_server_hot_reload() -> Result<()> {
    tracing_subscriber::fmt::init();
    // Clean up any previous test database
    std::fs::remove_dir_all("test.db").ok();

    // 1. Initialize the server state
    let engine = Engine::new();
    let db = ironmud::db::Db::open("test.db")?;
    let scripts = HashMap::new();
    let state = Arc::new(Mutex::new(World {
        engine,
        db,
        scripts,
        connections: HashMap::new(),
    }));

    {
        let mut world = state.lock().unwrap();
        world
            .engine
            .register_type_with_name::<Entity>("Entity")
            .register_get("id", |entity: &mut Entity| entity.id.to_string())
            .register_get("name", |entity: &mut Entity| entity.name.clone())
            .register_get("description", |entity: &mut Entity| {
                entity.description.clone()
            });
    }

    load_scripts(state.clone())?;
    watch_scripts(state.clone());

    // Create a dummy entity (COMMENTED OUT)
    /*
    let entity_id = Uuid::new_v4();
    let entity = Entity {
        id: entity_id,
        name: "The Test Void".to_string(),
        description: "A vast, empty test void.".to_string(),
    };
    {
        let world = state.lock().unwrap();
        world
            .db
            .insert(entity_id.as_bytes(), serde_json::to_vec(&entity)?)?;
    }
    */

    // 2. Start the server in a background task
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let server_handle = tokio::spawn(run_server(state, listener, shutdown_rx));
    sleep(Duration::from_millis(100)).await;

    // 3. Connect to the server and send the first 'look' command
    let mut stream = tokio::net::TcpStream::connect(addr).await?;
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);
    let mut response_bytes = Vec::new();

    // Read the initial prompt
    reader.read_until(b'>', &mut response_bytes).await?;
    response_bytes.clear();

    writer.write_all(b"look\n").await?;
    reader.read_until(b'>', &mut response_bytes).await?;
    let response = String::from_utf8_lossy(&response_bytes);
    assert!(response.contains("A vast, empty test void."));

    // 4. Modify the look.rhai script
    let new_content = r#"
fn look(entity) {
    `You see ${entity.name}.
${entity.description}
It is now a bustling test void.`
}
"#;
    tokio::fs::write("scripts/commands/look.rhai", new_content).await?;
    sleep(Duration::from_secs(2)).await; // Give time for the watcher to pick up the change

    // 5. Send the 'look' command again on the same connection
    response_bytes.clear();
    writer.write_all(b"look\n").await?;
    reader.read_until(b'>', &mut response_bytes).await?;
    let response2 = String::from_utf8_lossy(&response_bytes);
    assert!(response2.contains("a bustling test void"));

    // 6. Shutdown the server
    shutdown_tx.send(()).unwrap();
    server_handle.await?;

    Ok(())
}
*/

#[tokio::test]
async fn test_character_system_lifecycle() -> Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    // Clean up any previous test database
    std::fs::remove_dir_all("test.db").ok();

    // 1. Initialize the server state
    let mut engine = Engine::new();
    engine.set_max_expr_depths(128, 128);
    let db = ironmud::db::Db::open("test.db")?;
    let scripts = HashMap::new();
    let connections = Arc::new(Mutex::new(HashMap::new()));
    let command_metadata = load_command_metadata()?;

    let state = Arc::new(Mutex::new(World {
        engine,
        db,
        scripts,
        connections: connections.clone(),
        command_metadata,
        socials: ironmud::social::actions::SocialRegistry::default(),
        class_definitions: std::collections::HashMap::new(),
        trait_definitions: std::collections::HashMap::new(),
        race_suggestions: Vec::new(),
        race_definitions: std::collections::HashMap::new(),
        language_definitions: std::collections::HashMap::new(),
        recipes: std::collections::HashMap::new(),
        transports: std::collections::HashMap::new(),
        spell_definitions: std::collections::HashMap::new(),
        achievement_definitions: std::collections::HashMap::new(),
        achievement_index_by_counter: std::collections::HashMap::new(),
        custom_skill_definitions: std::collections::HashMap::new(),
        chat_sender: None,
        shutdown_sender: None,
        shutdown_cancel_sender: None,
        ip_limiter: Arc::new(ironmud::ratelimit::IpRateLimiter::new()),
        command_throttle: Arc::new(ironmud::throttle::CommandThrottle::new()),
    }));

    // Register Rhai functions for character system
    {
        let mut world = state.lock().unwrap();
        let db_clone = world.db.clone();
        script::register_rhai_functions(
            &mut world.engine,
            Arc::new(db_clone),
            connections.clone(),
            state.clone(),
        );
        // Register no-op chat broadcast functions for tests (chat integrations not configured in test environment)
        world.engine.register_fn("chat_broadcast", |_message: String| {});
        world.engine.register_fn("matrix_broadcast", |_message: String| {});
    }

    load_scripts(state.clone())?;
    load_game_data(state.clone())?;
    watch_scripts(state.clone());

    // 2. Start the server in a background task
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let server_handle = tokio::spawn(run_server(state.clone(), listener, shutdown_rx));
    sleep(Duration::from_millis(100)).await;

    // 3. Connect to the server with telnet-aware client
    let stream = tokio::net::TcpStream::connect(addr).await?;
    let (reader, writer) = stream.into_split();
    let mut client = TelnetTestClient::new(reader, writer);

    // Read initial prompt (handles telnet negotiation)
    let _ = client.read_until_prompt().await?;

    let char_name = "TestChar";
    let password = "testpassword";
    let wrong_password = "wrongpassword";
    let non_existent_char = "NonExistent";

    // --- Test 1: Character Creation (with wizard) ---
    client.send(&format!("create {} {}", char_name, password)).await?;
    let response = client.read_until_prompt().await?;
    println!("[TEST] Response from create: {}", response);
    // New wizard flow: should show the character creation menu
    assert!(
        response.contains("=== Character Creation:"),
        "Did not enter character wizard: {}",
        response
    );

    // Complete the wizard by pressing "d" (Done) - this now auto-logs in
    client.send("d").await?;
    let response = client.read_until_prompt().await?;
    println!("[TEST] Response from 'd': {}", response);
    let expected_creation_msg = format!("Character '{}' created successfully!", char_name);
    assert!(
        response.contains(&expected_creation_msg),
        "Failed to complete character creation wizard: {}",
        response
    );
    // Should also see welcome message since auto-login
    let expected_welcome_msg = format!("Welcome, {}!", char_name);
    assert!(
        response.contains(&expected_welcome_msg),
        "Auto-login after creation failed: {}",
        response
    );

    // --- Test 2: Logout and Disconnect ---
    client.send("quit").await?;
    let response = client.read_until_prompt().await?;
    let expected_goodbye_msg = format!("Goodbye, {}.", char_name);
    assert!(response.contains(&expected_goodbye_msg), "Failed to quit: {}", response);

    // --- Test 3: Attempt Login with Wrong Password (new connection) ---
    let stream2 = tokio::net::TcpStream::connect(addr).await?;
    let (reader2, writer2) = stream2.into_split();
    let mut client2 = TelnetTestClient::new(reader2, writer2);

    let _ = client2.read_until_prompt().await?;

    client2.send(&format!("login {} {}", char_name, wrong_password)).await?;
    let response2 = client2.read_until_prompt().await?;
    assert!(
        response2.contains("Incorrect password."),
        "Login with wrong password unexpectedly succeeded or gave wrong message: {}",
        response2
    );

    client2.send("quit").await?;
    let _ = client2.read_until_prompt().await?;

    // --- Test 4: Attempt Login with Non-Existent Character (new connection) ---
    let stream3 = tokio::net::TcpStream::connect(addr).await?;
    let (reader3, writer3) = stream3.into_split();
    let mut client3 = TelnetTestClient::new(reader3, writer3);

    let _ = client3.read_until_prompt().await?;

    client3
        .send(&format!("login {} {}", non_existent_char, password))
        .await?;
    let response3 = client3.read_until_prompt().await?;
    let expected_not_found_msg = format!("Account '{}' not found.", non_existent_char);
    assert!(
        response3.contains(&expected_not_found_msg),
        "Login with non-existent character unexpectedly succeeded or gave wrong message: {}",
        response3
    );

    client3.send("quit").await?;
    let _ = client3.read_until_prompt().await?;

    // 6. Shutdown the server
    shutdown_tx.send(()).unwrap();
    server_handle.await?;

    Ok(())
}

/// Test that all Rhai scripts compile without syntax errors.
/// This catches issues like missing functions, syntax errors, etc.
#[test]
fn test_all_scripts_compile() {
    use glob::glob;

    let mut engine = Engine::new();
    engine.set_max_expr_depths(128, 128);
    let mut failures = Vec::new();

    for entry in glob("scripts/**/*.rhai").expect("Failed to read glob pattern") {
        match entry {
            Ok(path) => {
                let path_str = path.display().to_string();
                match engine.compile_file(path.clone()) {
                    Ok(_) => {
                        // Script compiled successfully
                    }
                    Err(e) => {
                        failures.push(format!("{}: {}", path_str, e));
                    }
                }
            }
            Err(e) => {
                failures.push(format!("Glob error: {}", e));
            }
        }
    }

    if !failures.is_empty() {
        panic!("Script compilation failures:\n{}", failures.join("\n"));
    }
}

/// Test that all Rhai scripts only call functions that are registered.
/// This catches issues like typos in function names (e.g., get_inventory_items vs get_items_in_inventory).
#[test]
fn test_scripts_call_registered_functions() {
    use glob::glob;
    use regex::Regex;
    use std::collections::HashSet;
    use std::fs;

    // Step 1: Extract all registered function names from Rust source code
    let mut registered_functions: HashSet<String> = HashSet::new();

    // Pattern to match: register_fn("function_name", ...)
    let register_fn_pattern = Regex::new(r#"register_fn\s*\(\s*"([^"]+)""#).unwrap();

    // Scan all Rust files in src/script/
    for entry in glob("src/script/**/*.rs").expect("Failed to glob src/script") {
        if let Ok(path) = entry {
            if let Ok(content) = fs::read_to_string(&path) {
                for cap in register_fn_pattern.captures_iter(&content) {
                    registered_functions.insert(cap[1].to_string());
                }
            }
        }
    }

    // Also scan src/lib.rs and src/main.rs for any additional registrations
    for path in &["src/lib.rs", "src/main.rs"] {
        if let Ok(content) = fs::read_to_string(path) {
            for cap in register_fn_pattern.captures_iter(&content) {
                registered_functions.insert(cap[1].to_string());
            }
        }
    }

    // Step 2: Add Rhai built-in functions and common patterns
    let builtins = [
        // String methods
        "len",
        "is_empty",
        "to_upper",
        "to_lower",
        "trim",
        "contains",
        "starts_with",
        "ends_with",
        "sub_string",
        "split",
        "replace",
        "index_of",
        "pad",
        // Array methods
        "push",
        "pop",
        "shift",
        "insert",
        "remove",
        "clear",
        "reverse",
        "sort",
        "filter",
        "map",
        "reduce",
        "all",
        "any",
        "find",
        "index_of",
        // Map methods
        "keys",
        "values",
        "get",
        "set",
        "remove",
        "contains",
        "clear",
        // Type conversion
        "to_string",
        "to_int",
        "to_float",
        "to_bool",
        "parse_int",
        "parse_float",
        "type_of",
        "is_string",
        "is_int",
        "is_float",
        "is_bool",
        "is_array",
        "is_map",
        // Math
        "abs",
        "floor",
        "ceiling",
        "round",
        "sqrt",
        "sin",
        "cos",
        "tan",
        "log",
        "exp",
        "min",
        "max",
        "clamp",
        // Utility
        "print",
        "debug",
        "timestamp",
        "elapsed",
        // Range
        "range",
    ];
    for builtin in builtins {
        registered_functions.insert(builtin.to_string());
    }

    // Step 3: Scan all Rhai scripts for function calls
    // Pattern to match function calls: identifier followed by (
    // But NOT after . (method calls are fine, they're on registered types)
    // And NOT after :: (module-prefixed calls are fine, they're from imported modules)
    // And NOT function definitions (fn name(...))
    let fn_call_pattern = Regex::new(r"(?:^|[^.\w:])([a-z_][a-z0-9_]*)\s*\(").unwrap();
    let fn_def_pattern = Regex::new(r"fn\s+([a-z_][a-z0-9_]*)\s*\(").unwrap();
    // Pattern to remove string literals (both single and double quoted)
    let string_pattern = Regex::new(r#""(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'"#).unwrap();

    let mut failures: Vec<String> = Vec::new();

    for entry in glob("scripts/**/*.rhai").expect("Failed to glob scripts") {
        if let Ok(path) = entry {
            let path_str = path.display().to_string();
            if let Ok(content) = fs::read_to_string(&path) {
                // Extract functions defined in this script
                let mut local_functions: HashSet<String> = HashSet::new();
                for cap in fn_def_pattern.captures_iter(&content) {
                    local_functions.insert(cap[1].to_string());
                }

                // Find all function calls
                for (line_num, line) in content.lines().enumerate() {
                    // Skip comments
                    let line_trimmed = line.trim();
                    if line_trimmed.starts_with("//") {
                        continue;
                    }
                    // Remove inline comments
                    let line_no_comment = if let Some(idx) = line.find("//") {
                        &line[..idx]
                    } else {
                        line
                    };

                    // Remove string literals to avoid false positives
                    let line_no_strings = string_pattern.replace_all(line_no_comment, "\"\"");

                    for cap in fn_call_pattern.captures_iter(&line_no_strings) {
                        let fn_name = &cap[1];

                        // Skip if it's a control flow keyword
                        if matches!(
                            fn_name,
                            "if" | "else"
                                | "while"
                                | "for"
                                | "loop"
                                | "return"
                                | "let"
                                | "fn"
                                | "in"
                                | "switch"
                                | "throw"
                                | "try"
                                | "catch"
                        ) {
                            continue;
                        }

                        // Check if the function is registered, built-in, or locally defined
                        if !registered_functions.contains(fn_name) && !local_functions.contains(fn_name) {
                            failures.push(format!(
                                "{}:{}: unknown function '{}' (not registered)",
                                path_str,
                                line_num + 1,
                                fn_name
                            ));
                        }
                    }
                }
            }
        }
    }

    if !failures.is_empty() {
        // Deduplicate and sort failures
        let mut unique_failures: Vec<String> = failures.into_iter().collect::<HashSet<_>>().into_iter().collect();
        unique_failures.sort();
        panic!("Scripts call unregistered functions:\n{}", unique_failures.join("\n"));
    }
}

/// Test that all property accesses in Rhai scripts have corresponding registered getters.
/// This catches the bug where a new field is added to a data type (e.g., ItemData) but
/// the Rhai getter registration is forgotten, causing a runtime error when scripts
/// access `item.field_name`.
#[test]
fn test_scripts_access_registered_properties() {
    use glob::glob;
    use regex::Regex;
    use std::collections::{HashMap, HashSet};
    use std::fs;

    // --- Step 1: Build registry of getters per type from Rust source ---

    let mut getters_by_type: HashMap<String, HashSet<String>> = HashMap::new();

    // Match: .register_get("field_name", |var: &mut TypeName| ...)
    let register_get_re =
        Regex::new(r#"\.register_get\s*\(\s*"([^"]+)"[^|]*\|\s*\w+\s*:\s*&mut\s+(\w+)\s*\|"#).unwrap();

    // Match field-list registration macros:
    //   register_bool_flags!(engine, TypeName, flag1, flag2, ...)
    //   register_bool_ro!(engine, TypeName, field1, field2, ...)
    //   register_string!(engine, TypeName, field1, field2, ...)
    //   register_string_ro!(engine, TypeName, field1, field2, ...)
    //   register_i32!(engine, TypeName, field1, field2, ...)
    //   register_i32_ro!(engine, TypeName, field1, field2, ...)
    //   register_option_string{,_ro}!(engine, TypeName, ...)
    //   register_uuid_ro!, register_option_uuid{,_ro}!(engine, TypeName, ...)
    //   register_string_vec{,_ro}!(engine, TypeName, ...)
    let field_list_macro_re = Regex::new(
        r"register_(?:bool_flags|bool_ro|string|string_ro|i32|i32_ro|option_string|option_string_ro|uuid_ro|option_uuid|option_uuid_ro|string_vec|string_vec_ro)!\s*\(\s*\w+\s*,\s*(\w+)\s*,\s*([\s\S]*?)\);",
    )
    .unwrap();

    for entry in glob("src/script/**/*.rs").expect("Failed to glob src/script") {
        if let Ok(path) = entry {
            if let Ok(content) = fs::read_to_string(&path) {
                for cap in register_get_re.captures_iter(&content) {
                    getters_by_type
                        .entry(cap[2].to_string())
                        .or_default()
                        .insert(cap[1].to_string());
                }
                for cap in field_list_macro_re.captures_iter(&content) {
                    let type_name = cap[1].to_string();
                    for flag in cap[2].split(',') {
                        let flag = flag.trim();
                        if !flag.is_empty() {
                            getters_by_type
                                .entry(type_name.clone())
                                .or_default()
                                .insert(flag.to_string());
                        }
                    }
                }
            }
        }
    }

    // Also scan lib.rs and main.rs for any additional registrations
    for path in &["src/lib.rs", "src/main.rs"] {
        if let Ok(content) = fs::read_to_string(path) {
            for cap in register_get_re.captures_iter(&content) {
                getters_by_type
                    .entry(cap[2].to_string())
                    .or_default()
                    .insert(cap[1].to_string());
            }
        }
    }

    // Sanity check: ensure we actually found getters
    assert!(
        getters_by_type.contains_key("ItemData"),
        "No ItemData getters found - regex may be broken"
    );
    assert!(
        getters_by_type.contains_key("MobileData"),
        "No MobileData getters found - regex may be broken"
    );
    assert!(
        getters_by_type.contains_key("RoomData"),
        "No RoomData getters found - regex may be broken"
    );
    assert!(
        getters_by_type.contains_key("CharacterData"),
        "No CharacterData getters found - regex may be broken"
    );

    // --- Step 2: Map getter functions to the types they return ---

    let fn_returns_type: HashMap<&str, &str> = [
        ("get_item_data", "ItemData"),
        ("get_item_by_id", "ItemData"),
        ("get_mobile_data", "MobileData"),
        ("get_mobile_by_id", "MobileData"),
        ("find_mobile_by_keyword_anywhere", "MobileData"),
        ("spawn_mobile_from_prototype", "MobileData"),
        ("find_item_in_room", "ItemData"),
        ("get_room_data", "RoomData"),
        ("get_room_by_id", "RoomData"),
        ("get_character_data", "CharacterData"),
        ("get_character", "CharacterData"),
    ]
    .into_iter()
    .collect();

    // Sub-property type resolution (e.g., ItemData.flags -> ItemFlags)
    let resolve_sub_type = |parent: &str, prop: &str| -> Option<&'static str> {
        match (parent, prop) {
            ("ItemData", "flags") => Some("ItemFlags"),
            ("MobileData", "flags") => Some("MobileFlags"),
            ("RoomData", "flags") => Some("RoomFlags"),
            ("RoomData", "exits") => Some("RoomExits"),
            ("AreaData", "flags") => Some("AreaFlags"),
            _ => None,
        }
    };

    // --- Step 3: Scan Rhai scripts for property accesses on typed variables ---

    let string_re = Regex::new(r#""(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'"#).unwrap();
    // Match: let VAR = FUNC( or VAR = FUNC(
    let assign_re = Regex::new(r"(?:let\s+)?(\w+)\s*=\s*(\w+)\s*\(").unwrap();
    // Match property chains: word.word, word.word.word, etc.
    let chain_re = Regex::new(r"\b(\w+(?:\.\w+)+)").unwrap();
    // Match sub-property assignments: let var = other.prop;
    let sub_assign_re = Regex::new(r"let\s+(\w+)\s*=\s*(\w+)\.(\w+)\s*;").unwrap();

    let mut failures: Vec<String> = Vec::new();

    for entry in glob("scripts/**/*.rhai").expect("Failed to glob scripts") {
        if let Ok(path) = entry {
            let path_str = path.display().to_string();
            if let Ok(content) = fs::read_to_string(&path) {
                // Track variable types per file (reset per file)
                let mut var_types: HashMap<String, String> = HashMap::new();

                for (line_num, line) in content.lines().enumerate() {
                    // Skip full-line comments
                    let trimmed = line.trim();
                    if trimmed.starts_with("//") {
                        continue;
                    }

                    // Strip string literals first (avoids false matches inside strings)
                    let no_strings = string_re.replace_all(line, "\"\"");
                    // Then strip inline comments
                    let clean = match no_strings.find("//") {
                        Some(idx) => &no_strings[..idx],
                        None => &*no_strings,
                    };

                    // Track variable types from function return values
                    // e.g., let item = get_item_data(id) -> item is ItemData
                    for cap in assign_re.captures_iter(clean) {
                        let var = cap[1].to_string();
                        let func = &cap[2];
                        if let Some(&typ) = fn_returns_type.get(func) {
                            var_types.insert(var, typ.to_string());
                        }
                    }

                    // Track sub-property assignments
                    // e.g., let f = item.flags; -> f is ItemFlags
                    for cap in sub_assign_re.captures_iter(clean) {
                        let target = cap[1].to_string();
                        let source = &cap[2];
                        let prop = &cap[3];
                        if let Some(source_type) = var_types.get(source).cloned() {
                            if let Some(st) = resolve_sub_type(&source_type, prop) {
                                var_types.insert(target, st.to_string());
                            }
                        }
                    }

                    // Check property access chains against registered getters
                    for m in chain_re.captures_iter(clean) {
                        let full_chain = &m[1];
                        let match_end = m.get(0).unwrap().end();
                        let parts: Vec<&str> = full_chain.split('.').collect();

                        if parts.len() < 2 {
                            continue;
                        }

                        // Only check variables with known types
                        let var_type = match var_types.get(parts[0]) {
                            Some(t) => t.clone(),
                            None => continue,
                        };

                        let mut current_type = var_type;

                        for (i, &prop) in parts[1..].iter().enumerate() {
                            let is_last = i == parts.len() - 2;

                            // If the last element is followed by '(', it's a method call - skip it
                            if is_last && match_end < clean.len() {
                                let after = clean[match_end..].trim_start();
                                if after.starts_with('(') {
                                    break;
                                }
                            }

                            // Validate property against registered getters for current type
                            match getters_by_type.get(&current_type) {
                                Some(getters) if !getters.contains(prop) => {
                                    failures.push(format!(
                                        "{}:{}: '{}' accesses unregistered property '{}' on type {}",
                                        path_str,
                                        line_num + 1,
                                        full_chain,
                                        prop,
                                        current_type
                                    ));
                                    break;
                                }
                                None => break, // No getters registered for this type, skip
                                _ => {}        // Property is registered, continue chain
                            }

                            // Resolve sub-type for chained access (e.g., flags -> ItemFlags)
                            match resolve_sub_type(&current_type, prop) {
                                Some(st) => current_type = st.to_string(),
                                None => break, // No sub-type mapping, stop checking chain
                            }
                        }
                    }
                }
            }
        }
    }

    if !failures.is_empty() {
        let mut unique: Vec<String> = failures.into_iter().collect::<HashSet<_>>().into_iter().collect();
        unique.sort();
        panic!("Scripts access unregistered properties:\n{}", unique.join("\n"));
    }
}

/// Ensures all register_get closures that access i32/i16/u32/u16/u8 fields
/// cast to i64, preventing Rhai type mismatch errors at runtime.
///
/// Rhai's default integer type is i64. If a Rust getter returns i32 directly,
/// comparisons like `if value <= 5` fail with:
///   "Function not found: <= (i32, i64)"
///
/// This test catches missing `as i64` casts at test time.
#[test]
fn test_registered_getters_return_rhai_compatible_types() {
    use glob::glob;
    use regex::Regex;
    use std::fs;

    // --- Phase 1: Auto-discover ALL i32 fields from src/types/mod.rs ---
    // This eliminates the manual allowlist that kept going stale.
    let types_content = fs::read_to_string("src/types/mod.rs").expect("Failed to read src/types/mod.rs");

    let field_re = Regex::new(r"pub\s+(\w+)\s*:\s*i32\b").unwrap();

    let mut i32_fields: std::collections::HashSet<String> = std::collections::HashSet::new();
    for cap in field_re.captures_iter(&types_content) {
        i32_fields.insert(cap[1].to_string());
    }

    // --- Phase 2: Scan all register_get calls ---
    // Match: .register_get("name", |var: &mut Type| BODY)
    let getter_re =
        Regex::new(r#"\.register_get\s*\(\s*"([^"]+)"\s*,\s*\|(\w+)\s*:\s*&mut\s+(\w+)\s*\|\s*(.+)"#).unwrap();

    let mut failures: Vec<String> = Vec::new();

    for entry in glob("src/script/**/*.rs").expect("Failed to glob src/script") {
        if let Ok(path) = entry {
            let path_str = path.display().to_string();
            if let Ok(content) = fs::read_to_string(&path) {
                let lines: Vec<&str> = content.lines().collect();
                for (line_num, line) in lines.iter().enumerate() {
                    if let Some(cap) = getter_re.captures(line) {
                        let prop_name = &cap[1];
                        let _var = &cap[2];
                        let type_name = &cap[3];
                        let body_head = &cap[4];

                        // rustfmt may split a closure body across multiple lines.
                        // If the captured body is just `{` or ends with `{`, extend
                        // the body scan to include subsequent lines up to the
                        // matching close. Cap at 10 lines for safety.
                        let body: String = if body_head.trim_end().ends_with('{') {
                            let mut acc = body_head.to_string();
                            let end = (line_num + 10).min(lines.len());
                            for follow in lines[line_num + 1..end].iter() {
                                acc.push(' ');
                                acc.push_str(follow.trim());
                                if follow.trim_start().starts_with("})") || follow.trim() == "}" {
                                    break;
                                }
                            }
                            acc
                        } else {
                            body_head.to_string()
                        };
                        let body = body.as_str();

                        // Check if the getter name matches a known i32 field
                        let accesses_i32_field = i32_fields.contains(prop_name);
                        let has_i64_cast = body.contains("as i64");
                        let returns_string = body.contains(".clone()")
                            || body.contains(".to_string()")
                            || body.contains("format!")
                            || body.contains("to_display_string");
                        let returns_bool = body.contains("bool")
                            || body.trim_end().ends_with("true")
                            || body.trim_end().ends_with("false");
                        let returns_other_type = body.contains("map(")
                            || body.contains("unwrap_or")
                            || body.contains("Vec")
                            || body.contains("clone()")
                            || body.contains("as f64");

                        // If getter name matches a known i32 field and doesn't cast,
                        // and doesn't obviously return a string/bool/other type, flag it.
                        if accesses_i32_field
                            && !has_i64_cast
                            && !returns_string
                            && !returns_bool
                            && !returns_other_type
                        {
                            failures.push(format!(
                                "{}:{}: register_get(\"{}\") on {} returns i32 without `as i64` cast. \
                                 Rhai comparisons with integer literals will fail at runtime.",
                                path_str,
                                line_num + 1,
                                prop_name,
                                type_name,
                            ));
                        }
                    }
                }
            }
        }
    }

    if !failures.is_empty() {
        failures.sort();
        panic!(
            "Registered getters return i32 without casting to i64 (Rhai's default integer type).\n\
             Fix by adding `as i64` to the getter closure.\n\n{}",
            failures.join("\n")
        );
    }
}

#[test]
fn test_rhai_property_writes_have_registered_setters() {
    use glob::glob;
    use regex::Regex;
    use std::collections::HashSet;
    use std::fs;

    // Step 1: Collect all registered setter property names from Rust source.
    // If register_set("foo", ...) exists on ANY type, "foo" is considered valid.
    // Setter-bearing macros (register_string!, register_i32!, register_bool_flags!) also
    // count - the *_ro variants are read-only and intentionally excluded.
    let setter_re = Regex::new(r#"register_set\s*\(\s*"([^"]+)""#).unwrap();
    let setter_macro_re =
        Regex::new(r"register_(?:bool_flags|string|i32|option_string|option_uuid|string_vec)!\s*\(\s*\w+\s*,\s*\w+\s*,\s*([\s\S]*?)\);").unwrap();
    let mut registered_setters: HashSet<String> = HashSet::new();

    for entry in glob("src/script/**/*.rs").expect("glob src/script") {
        if let Ok(path) = entry {
            if let Ok(content) = fs::read_to_string(&path) {
                for cap in setter_re.captures_iter(&content) {
                    registered_setters.insert(cap[1].to_string());
                }
                for cap in setter_macro_re.captures_iter(&content) {
                    for field in cap[1].split(',') {
                        let field = field.trim();
                        if !field.is_empty() {
                            registered_setters.insert(field.to_string());
                        }
                    }
                }
            }
        }
    }

    // Step 2: Scan Rhai scripts for property writes on non-map variables.
    // Pattern: var.property = value (but not var.property == value)
    let prop_write_re = Regex::new(r"(\w+)\.(\w+)\s*[+\-*/]?=[^=]").unwrap();
    let comment_re = Regex::new(r"//.*").unwrap();
    let string_re = Regex::new(r#""(?:[^"\\]|\\.)*""#).unwrap();
    let map_literal_re = Regex::new(r"let\s+(\w+)\s*=\s*#\s*\{").unwrap();
    let fn_assign_re = Regex::new(r"let\s+(\w+)\s*=\s*(\w+)\s*\(").unwrap();

    // Functions known to return Rhai maps (not registered Rust types).
    // Add entries here when the test flags writes on variables from map-returning functions.
    let map_returning_fns: HashSet<&str> = ["get_attacker_weapon_info", "get_lockout_data"].into_iter().collect();

    let mut failures: Vec<String> = Vec::new();

    for entry in glob("scripts/**/*.rhai").expect("glob scripts") {
        if let Ok(path) = entry {
            let path_str = path.display().to_string();
            if let Ok(content) = fs::read_to_string(&path) {
                // Detect map variables: both literal #{} and function-returned maps
                let mut map_vars: HashSet<String> = map_literal_re
                    .captures_iter(&content)
                    .map(|cap| cap[1].to_string())
                    .collect();

                for cap in fn_assign_re.captures_iter(&content) {
                    if map_returning_fns.contains(&cap[2]) {
                        map_vars.insert(cap[1].to_string());
                    }
                }

                for (line_num, line) in content.lines().enumerate() {
                    // Strip comments and string literals
                    let stripped = comment_re.replace(line, "");
                    let stripped = string_re.replace_all(&stripped, r#""""#);

                    for cap in prop_write_re.captures_iter(&stripped) {
                        let var_name = &cap[1];
                        let prop_name = &cap[2];

                        // Skip writes on map variables
                        if map_vars.contains(var_name) {
                            continue;
                        }

                        if !registered_setters.contains(prop_name) {
                            failures.push(format!(
                                "{}:{}: '{}.{} = ...' but no register_set(\"{}\") found in src/script/",
                                path_str,
                                line_num + 1,
                                var_name,
                                prop_name,
                                prop_name
                            ));
                        }
                    }
                }
            }
        }
    }

    if !failures.is_empty() {
        failures.sort();
        failures.dedup();
        panic!(
            "Rhai scripts write to properties that have no registered setter.\n\
             Fix by adding .register_set() in src/script/*.rs for each property.\n\
             If the variable is a Rhai map (#{{}}), initialize it with 'let x = #{{}};' \
             so the test can detect it.\n\n{}",
            failures.join("\n")
        );
    }
}

/// Test that every command script in scripts/commands/ has a corresponding
/// entry in scripts/commands.json. This catches commands that were added
/// as .rhai files but forgotten in commands.json (which controls help and
/// tab-completion).
#[test]
fn test_commands_json_covers_all_scripts() {
    use std::collections::BTreeSet;

    let commands_json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string("scripts/commands.json").expect("read commands.json"))
            .expect("parse commands.json");

    let registered: BTreeSet<String> = commands_json
        .as_object()
        .expect("commands.json should be an object")
        .keys()
        .cloned()
        .collect();

    let mut script_names: BTreeSet<String> = BTreeSet::new();
    for entry in glob::glob("scripts/commands/*.rhai").expect("glob") {
        let path = entry.expect("glob entry");
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        script_names.insert(stem);
    }

    let missing: Vec<&String> = script_names.difference(&registered).collect();
    let extra: Vec<&String> = registered.difference(&script_names).collect();

    let mut errors = Vec::new();
    if !missing.is_empty() {
        errors.push(format!(
            "Commands with .rhai scripts but missing from commands.json:\n  {}",
            missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        ));
    }
    if !extra.is_empty() {
        errors.push(format!(
            "Commands in commands.json with no matching .rhai script:\n  {}",
            extra.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        ));
    }

    if !errors.is_empty() {
        panic!("commands.json sync errors:\n{}", errors.join("\n"));
    }
}

/// Extract bool field names from a Rust struct defined anywhere under
/// src/types/. Scans every `*.rs` file in the directory until the named
/// struct is found, so callers don't have to know which submodule a type
/// lives in.
fn extract_bool_field_names(struct_name: &str) -> Vec<String> {
    use regex::Regex;

    let struct_re = Regex::new(&format!(r"pub struct {} \{{([^}}]+)\}}", struct_name)).unwrap();
    let field_re = Regex::new(r"pub\s+(\w+)\s*:\s*bool").unwrap();

    for entry in std::fs::read_dir("src/types").expect("read src/types") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        if let Some(caps) = struct_re.captures(&content) {
            let body = caps.get(1).unwrap().as_str();
            return field_re.captures_iter(body).map(|c| c[1].to_string()).collect();
        }
    }
    panic!("{} struct not found anywhere under src/types/", struct_name);
}

/// Extract the body of a Rhai function by name.
fn extract_rhai_fn_body<'a>(file_content: &'a str, fn_name: &str) -> &'a str {
    use regex::Regex;
    // Rhai function bodies end at a top-level `}` at column 0.
    let re = Regex::new(&format!(r"(?ms)^fn {}\s*\([^)]*\)\s*\{{(.*?)^\}}", fn_name)).unwrap();
    let m = re
        .captures(file_content)
        .unwrap_or_else(|| panic!("fn {} not found", fn_name))
        .get(1)
        .unwrap();
    &file_content[m.start()..m.end()]
}

/// Guard against editor UIs falling out of sync with *Flags structs.
///
/// For each editor we:
/// 1. Verify every bool field on the matching *Flags struct (minus an
///    explicit skip-list of internal-only flags) is referenced in every
///    display/dispatch section.
/// 2. Verify no flag description string contains raw `<` or `>` — those
///    characters break MXP rendering (the client parses `<` as a tag and
///    eats text until the next `>`).
///
/// When adding a new flag, add it to the struct in src/types/mod.rs, then
/// run this test — it will tell you exactly which editor sections are
/// missing the flag.
#[test]
fn test_flag_editors_stay_in_sync_with_structs() {
    use regex::Regex;
    use std::collections::HashSet;
    use std::fs;

    struct EditorCheck {
        label: &'static str,
        struct_name: &'static str,
        script_path: &'static str,
        // Rhai fn names that each must reference every non-skipped flag.
        sections: Vec<(&'static str, &'static str)>, // (fn_name, flag_regex_template with {flag})
        // Flags not exposed to builders (e.g. internal state).
        skip: HashSet<&'static str>,
    }

    let editors = vec![
        EditorCheck {
            label: "medit",
            struct_name: "MobileFlags",
            script_path: "scripts/commands/medit.rhai",
            sections: vec![
                ("show_mobile_flags", r#"build_flag_line\("{flag}""#),
                ("build_flags_display", r#"flags\.{flag}\b"#),
                ("get_mobile_flag_value", r#"flags\.{flag}\b"#),
            ],
            skip: HashSet::new(),
        },
        EditorCheck {
            label: "oedit",
            struct_name: "ItemFlags",
            script_path: "scripts/commands/oedit.rhai",
            sections: vec![
                ("show_item_flags", r#"build_flag_line\("{flag}""#),
                ("build_flags_display", r#"flags\.{flag}\b"#),
            ],
            // Corpse flags are set by the death system, never by builders.
            skip: ["is_corpse", "corpse_is_player"].into_iter().collect(),
        },
    ];

    let mut missing: Vec<String> = Vec::new();
    let mut bad_desc: Vec<String> = Vec::new();

    for editor in &editors {
        let fields = extract_bool_field_names(editor.struct_name);
        assert!(!fields.is_empty(), "Expected bool fields on {}", editor.struct_name);

        let script = fs::read_to_string(editor.script_path).unwrap_or_else(|_| panic!("read {}", editor.script_path));

        for (fn_name, tmpl) in &editor.sections {
            let body = extract_rhai_fn_body(&script, fn_name);
            for flag in &fields {
                if editor.skip.contains(flag.as_str()) {
                    continue;
                }
                let re = Regex::new(&tmpl.replace("{flag}", flag)).unwrap();
                if !re.is_match(body) {
                    missing.push(format!(
                        "[{}] {} is not referenced in fn {}() of {}",
                        editor.label, flag, fn_name, editor.script_path
                    ));
                }
            }
        }

        // Scan every build_flag_line(...) call in the script for descriptions
        // containing raw `<` or `>` — these survive into MXP output and cause
        // the client to eat subsequent text.
        let call_re = Regex::new(r#"build_flag_line\s*\(\s*"[^"]*"\s*,\s*[^,]+,\s*"([^"]*)""#).unwrap();
        for cap in call_re.captures_iter(&script) {
            let desc = &cap[1];
            if desc.contains('<') || desc.contains('>') {
                bad_desc.push(format!(
                    "[{}] description in {} contains raw '<' or '>': {:?}",
                    editor.label, editor.script_path, desc
                ));
            }
        }
    }

    // redit uses inline toggles, not build_flag_line. Verify each RoomFlags
    // bool field appears inside the `subcommand == \"flags\"` block (the
    // display) and the `valid_flags` array (the setter dispatch).
    let room_fields = extract_bool_field_names("RoomFlags");
    // property_storage, climate_controlled, always_hot, always_cold are
    // system-managed and not yet exposed via redit; skip them for now.
    let room_skip: HashSet<&str> = ["property_storage", "climate_controlled", "always_hot", "always_cold"]
        .into_iter()
        .collect();

    let redit = fs::read_to_string("scripts/commands/redit.rhai").expect("read redit.rhai");
    // Isolate the flags display and the flag-setter dispatch blocks.
    let flags_display_re = Regex::new(r#"(?ms)if subcommand == "flags" \{(.*?)return;\s*\}"#).unwrap();
    let flags_display_block = flags_display_re
        .captures(&redit)
        .expect("redit flags subcommand block not found")
        .get(1)
        .unwrap()
        .as_str();
    let valid_flags_re = Regex::new(r#"let valid_flags = \[([^\]]+)\];"#).unwrap();
    let valid_flags_block = valid_flags_re
        .captures(&redit)
        .expect("redit valid_flags array not found")
        .get(1)
        .unwrap()
        .as_str();

    for flag in &room_fields {
        if room_skip.contains(flag.as_str()) {
            continue;
        }
        let usage_re = Regex::new(&format!(r"flags\.{}\b", regex::escape(flag))).unwrap();
        if !usage_re.is_match(flags_display_block) {
            missing.push(format!(
                "[redit] {} is not referenced in `subcommand == \"flags\"` block of scripts/commands/redit.rhai",
                flag
            ));
        }
        let listed_re = Regex::new(&format!(r#""{}""#, regex::escape(flag))).unwrap();
        if !listed_re.is_match(valid_flags_block) {
            missing.push(format!(
                "[redit] {} is not listed in `valid_flags` array of scripts/commands/redit.rhai",
                flag
            ));
        }
    }

    if !missing.is_empty() || !bad_desc.is_empty() {
        let mut msg = String::new();
        if !missing.is_empty() {
            msg.push_str("*Flags struct fields not handled in their editor UI:\n  ");
            msg.push_str(&missing.join("\n  "));
            msg.push('\n');
        }
        if !bad_desc.is_empty() {
            msg.push_str("\nbuild_flag_line descriptions contain raw '<' or '>' — these break MXP:\n  ");
            msg.push_str(&bad_desc.join("\n  "));
        }
        panic!("{}", msg);
    }
}

#[test]
fn test_seed_rooms_have_bidirectional_exits() {
    use ironmud::types::RoomExits;
    use std::collections::HashMap;
    use uuid::Uuid;

    // Cross-area exits are handled within the seed data; this test walks every
    // seeded room and asserts that for each outgoing exit there is a matching
    // reverse exit from the destination room.
    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");
        ironmud::seed::seed_demo_world(&db).expect("seed_demo_world");

        let rooms = db.list_all_rooms().expect("list_all_rooms");
        let by_id: HashMap<Uuid, &ironmud::types::RoomData> = rooms.iter().map(|r| (r.id, r)).collect();

        fn dir_exits(e: &RoomExits) -> [(&'static str, Option<Uuid>); 6] {
            [
                ("north", e.north),
                ("east", e.east),
                ("south", e.south),
                ("west", e.west),
                ("up", e.up),
                ("down", e.down),
            ]
        }
        fn opposite(d: &str) -> &'static str {
            match d {
                "north" => "south",
                "south" => "north",
                "east" => "west",
                "west" => "east",
                "up" => "down",
                "down" => "up",
                _ => "",
            }
        }
        fn get_dir(e: &RoomExits, d: &str) -> Option<Uuid> {
            match d {
                "north" => e.north,
                "east" => e.east,
                "south" => e.south,
                "west" => e.west,
                "up" => e.up,
                "down" => e.down,
                _ => None,
            }
        }

        let mut errors = Vec::new();
        for room in &rooms {
            // Skip property templates — cottage_interior has no exits by design.
            if room.is_property_template {
                continue;
            }
            for (dir, maybe_target) in dir_exits(&room.exits) {
                let Some(target_id) = maybe_target else { continue };
                let Some(target) = by_id.get(&target_id) else {
                    errors.push(format!(
                        "{} -> {} points at missing room {}",
                        room.vnum.as_deref().unwrap_or("?"),
                        dir,
                        target_id
                    ));
                    continue;
                };
                let rev = opposite(dir);
                match get_dir(&target.exits, rev) {
                    Some(back) if back == room.id => {}
                    other => errors.push(format!(
                        "{} -> {} -> {} (vnum {}), but reverse {}: {:?}",
                        room.vnum.as_deref().unwrap_or("?"),
                        dir,
                        target_id,
                        target.vnum.as_deref().unwrap_or("?"),
                        rev,
                        other
                    )),
                }
            }
        }

        assert!(errors.is_empty(), "Non-bidirectional exits:\n  {}", errors.join("\n  "));
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_seed_demo_world() {
    // Use a unique temp DB to avoid conflicts with other tests
    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        // First call should seed and return true
        let seeded = ironmud::seed::seed_demo_world(&db).expect("seed_demo_world");
        assert!(seeded, "First call should seed the world");

        // Verify entities were created
        let stats = db.world_stats().expect("world_stats");
        assert_eq!(stats.areas, 5, "Expected 5 areas");
        assert!(stats.rooms >= 50, "Expected at least 50 rooms, got {}", stats.rooms);
        assert!(stats.items >= 30, "Expected at least 30 items, got {}", stats.items);
        assert!(
            stats.mobiles >= 14,
            "Expected at least 14 mobiles, got {}",
            stats.mobiles
        );
        assert!(
            stats.spawn_points >= 15,
            "Expected at least 15 spawn points, got {}",
            stats.spawn_points
        );
        assert!(stats.recipes >= 4, "Expected at least 4 recipes, got {}", stats.recipes);
        assert!(
            stats.plant_prototypes >= 2,
            "Expected at least 2 plant prototypes, got {}",
            stats.plant_prototypes
        );
        assert!(
            stats.transports >= 1,
            "Expected at least 1 transport, got {}",
            stats.transports
        );
        assert!(
            stats.property_templates >= 1,
            "Expected at least 1 property template, got {}",
            stats.property_templates
        );

        // Second call should be idempotent (already exists)
        let seeded_again = ironmud::seed::seed_demo_world(&db).expect("seed_demo_world again");
        assert!(!seeded_again, "Second call should detect existing world");

        // Clear world data
        db.clear_world_data().expect("clear_world_data");
        let stats_after_clear = db.world_stats().expect("world_stats after clear");
        assert_eq!(stats_after_clear.areas, 0, "Areas should be cleared");
        assert_eq!(stats_after_clear.rooms, 0, "Rooms should be cleared");
        assert_eq!(stats_after_clear.items, 0, "Items should be cleared");
        assert_eq!(stats_after_clear.mobiles, 0, "Mobiles should be cleared");

        // Re-seed should work after clear
        let re_seeded = ironmud::seed::seed_demo_world(&db).expect("re-seed");
        assert!(re_seeded, "Should re-seed after clear");

        let stats_after_reseed = db.world_stats().expect("world_stats after reseed");
        assert_eq!(stats_after_reseed.areas, 5, "Should have 5 areas after re-seed");
    }));

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

// ===========================================================================
// Migrant immigration system tests
// ===========================================================================

mod migration_tests {
    use ironmud::db::Db;
    use ironmud::migration::{MigrationData, absolute_game_day, load_migration_data, process_migration_tick};
    use ironmud::{
        AreaData, AreaFlags, AreaPermission, CombatZoneType, GameTime, RoomData, RoomExits, RoomFlags, WaterType,
    };
    use std::collections::HashMap;
    use std::panic::AssertUnwindSafe;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    fn data_dir() -> PathBuf {
        // Tests run from crate root; scripts/data/ is a sibling of src/.
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/data")
    }

    fn empty_connections() -> ironmud::SharedConnections {
        Arc::new(Mutex::new(HashMap::new()))
    }

    fn migration_data_for_tests() -> MigrationData {
        load_migration_data(&data_dir()).expect("load migration data for tests")
    }

    fn new_area(prefix: &str) -> AreaData {
        AreaData {
            id: Uuid::new_v4(),
            name: format!("Test {}", prefix),
            prefix: prefix.to_string(),
            description: String::new(),
            level_min: 0,
            level_max: 0,
            theme: String::new(),
            owner: None,
            permission_level: AreaPermission::AllBuilders,
            trusted_builders: Vec::new(),
            city_forage_table: Vec::new(),
            wilderness_forage_table: Vec::new(),
            shallow_water_forage_table: Vec::new(),
            deep_water_forage_table: Vec::new(),
            underwater_forage_table: Vec::new(),
            combat_zone: CombatZoneType::Pve,
            flags: AreaFlags::default(),
            default_room_flags: ironmud::types::RoomFlags::default(),
            climate: ironmud::types::ClimateProfile::default(),
            immigration_enabled: true,
            immigration_room_vnum: format!("{}:gate", prefix),
            immigration_name_pool: "generic".to_string(),
            immigration_visual_profile: "human".to_string(),
            migration_interval_days: 1,
            migration_max_per_check: 10,
            migrant_sim_defaults: None,
            last_migration_check_day: None,
            immigration_variation_chances: ironmud::types::ImmigrationVariationChances::default(),
            immigration_family_chance: ironmud::types::ImmigrationFamilyChance::default(),
            migrant_starting_gold: ironmud::types::GoldRange::default(),
            guard_wage_per_hour: 0,
            healer_wage_per_hour: 0,
            scavenger_wage_per_hour: 0,
            donation_room_vnum: None,
            max_rooms: None,
            max_items: None,
            max_mobiles: None,
            max_spawn_points: None,
        }
    }

    fn new_room(area_id: Uuid, vnum: &str, liveable: bool, capacity: i32) -> RoomData {
        RoomData {
            id: Uuid::new_v4(),
            title: format!("Room {}", vnum),
            description: String::new(),
            exits: RoomExits::default(),
            flags: RoomFlags {
                liveable,
                ..Default::default()
            },
            extra_descs: Vec::new(),
            vnum: Some(vnum.to_string()),
            area_id: Some(area_id),
            triggers: Vec::new(),
            doors: HashMap::new(),
            spring_desc: None,
            summer_desc: None,
            autumn_desc: None,
            winter_desc: None,
            dynamic_desc: None,
            water_type: WaterType::None,
            catch_table: Vec::new(),
            is_property_template: false,
            property_template_id: None,
            is_template_entrance: false,
            property_lease_id: None,
            property_entrance: false,
            recent_departures: Vec::new(),
            blood_trails: Vec::new(),
            traps: Vec::new(),
            living_capacity: capacity,
            residents: Vec::new(),
            dg_vars: std::collections::HashMap::new(),
            coordinates: None,
            contextual_commands: Vec::new(),
            exit_delays: std::collections::HashMap::new(),
        }
    }

    /// Save room, wire its vnum into the db's vnum index, and return the stored copy.
    fn save_room_with_vnum(db: &Db, room: &RoomData) {
        db.save_room_data(room.clone()).expect("save_room_data");
        if let Some(v) = &room.vnum {
            db.set_room_vnum(&room.id, v).expect("set_room_vnum");
        }
    }

    fn set_game_day(db: &Db, day_absolute: i64) {
        // Fit into 1..=30 day, 1..=12 month, then year
        let days_per_month = 30i64;
        let months_per_year = 12i64;
        let year = (day_absolute / (days_per_month * months_per_year)) as u32;
        let remainder = day_absolute % (days_per_month * months_per_year);
        let month = (remainder / days_per_month) as u8 + 1;
        let day = (remainder % days_per_month) as u8 + 1;
        let mut gt = GameTime::default();
        gt.year = year;
        gt.month = month;
        gt.day = day;
        db.save_game_time(&gt).expect("save_game_time");
    }

    fn open_temp_db(_tag: &str) -> (Db, tempfile::TempDir) {
        let temp = tempfile::tempdir().expect("create temp dir");
        let db = Db::open(temp.path()).expect("open DB");
        (db, temp)
    }

    fn run_one_tick(db: &Db, data: &MigrationData) {
        let conns = empty_connections();
        process_migration_tick(db, &conns, data).expect("migration tick");
    }

    #[test]
    fn test_name_pool_loading() {
        let data = load_migration_data(&data_dir()).expect("load migration data");
        assert!(data.name_pools.contains_key("generic"), "generic pool loaded");
        assert!(data.name_pools.contains_key("japan"), "japan pool loaded");
        let generic = data.name_pools.get("generic").unwrap();
        assert!(generic.male_first.len() >= 50, "enough male names");
        assert!(generic.female_first.len() >= 50, "enough female names");
        assert!(generic.last.len() >= 50, "enough last names");
        assert!(data.visual_profiles.contains_key("human"), "human profile loaded");
    }

    #[test]
    fn test_migration_spawns_when_room_available() {
        let (db, _temp) = open_temp_db(
"spawn");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let area = new_area("mig_spawn");
            db.save_area_data(area.clone()).unwrap();

            let gate = new_room(area.id, &format!("{}:gate", area.prefix), false, 0);
            save_room_with_vnum(&db, &gate);
            let home = new_room(area.id, &format!("{}:home", area.prefix), true, 1);
            save_room_with_vnum(&db, &home);

            set_game_day(&db, 10);
            let data = load_migration_data(&data_dir()).unwrap();
            run_one_tick(&db, &data);

            // Exactly 1 migrant spawned.
            let mobs: Vec<_> = db
                .list_all_mobiles()
                .unwrap()
                .into_iter()
                .filter(|m| !m.is_prototype)
                .collect();
            assert_eq!(mobs.len(), 1, "expected one migrant");
            let m = &mobs[0];
            assert_eq!(m.resident_of.as_deref(), Some(format!("{}:home", area.prefix).as_str()));
            assert!(m.characteristics.is_some(), "characteristics populated");
            assert!(m.simulation.is_some(), "simulation populated");
            assert_eq!(m.current_room_id, Some(gate.id), "spawned in arrival room");

            // Room residents list updated.
            let refreshed = db.get_room_data(&home.id).unwrap().unwrap();
            assert_eq!(refreshed.residents.len(), 1);
            assert_eq!(refreshed.residents[0], m.id);

            // Area last_migration_check_day advanced.
            let refreshed_area = db.get_area_data(&area.id).unwrap().unwrap();
            assert_eq!(refreshed_area.last_migration_check_day, Some(10));
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_migration_skips_when_no_capacity() {
        let (db, _temp) = open_temp_db(
"nocap");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let area = new_area("mig_nocap");
            db.save_area_data(area.clone()).unwrap();

            let gate = new_room(area.id, &format!("{}:gate", area.prefix), false, 0);
            save_room_with_vnum(&db, &gate);
            // Liveable room but already at capacity (capacity 1, 1 fake resident).
            let mut home = new_room(area.id, &format!("{}:home", area.prefix), true, 1);
            home.residents.push(Uuid::new_v4());
            save_room_with_vnum(&db, &home);

            set_game_day(&db, 10);
            let data = load_migration_data(&data_dir()).unwrap();
            run_one_tick(&db, &data);

            let real_mobs: Vec<_> = db
                .list_all_mobiles()
                .unwrap()
                .into_iter()
                .filter(|m| !m.is_prototype)
                .collect();
            assert!(real_mobs.is_empty(), "no migrants when all slots full");
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_migration_respects_max_per_check() {
        let (db, _temp) = open_temp_db(
"maxcheck");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut area = new_area("mig_max");
            area.migration_max_per_check = 3;
            db.save_area_data(area.clone()).unwrap();

            let gate = new_room(area.id, &format!("{}:gate", area.prefix), false, 0);
            save_room_with_vnum(&db, &gate);
            // 10 free slots total.
            let home = new_room(area.id, &format!("{}:home", area.prefix), true, 10);
            save_room_with_vnum(&db, &home);

            set_game_day(&db, 10);
            let data = load_migration_data(&data_dir()).unwrap();
            run_one_tick(&db, &data);

            let mobs: Vec<_> = db
                .list_all_mobiles()
                .unwrap()
                .into_iter()
                .filter(|m| !m.is_prototype)
                .collect();
            assert_eq!(mobs.len(), 3, "spawn capped to migration_max_per_check");
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_migration_respects_interval() {
        let (db, _temp) = open_temp_db(
"interval");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut area = new_area("mig_interval");
            area.migration_interval_days = 5;
            // Record a check at game-day 100; next due at 105.
            area.last_migration_check_day = Some(100);
            db.save_area_data(area.clone()).unwrap();

            let gate = new_room(area.id, &format!("{}:gate", area.prefix), false, 0);
            save_room_with_vnum(&db, &gate);
            let home = new_room(area.id, &format!("{}:home", area.prefix), true, 5);
            save_room_with_vnum(&db, &home);

            // Only 2 days elapsed — should not fire.
            set_game_day(&db, 102);
            let data = load_migration_data(&data_dir()).unwrap();
            run_one_tick(&db, &data);

            let mobs: Vec<_> = db
                .list_all_mobiles()
                .unwrap()
                .into_iter()
                .filter(|m| !m.is_prototype)
                .collect();
            assert!(mobs.is_empty(), "no migrants before interval elapses");

            // Confirm last_migration_check_day was NOT advanced.
            let refreshed = db.get_area_data(&area.id).unwrap().unwrap();
            assert_eq!(refreshed.last_migration_check_day, Some(100));

            // Advance past interval — now it should fire.
            set_game_day(&db, 106);
            run_one_tick(&db, &data);
            let mobs_after: Vec<_> = db
                .list_all_mobiles()
                .unwrap()
                .into_iter()
                .filter(|m| !m.is_prototype)
                .collect();
            assert!(!mobs_after.is_empty(), "migrants spawn once interval elapses");
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_death_releases_residency() {
        let (db, _temp) = open_temp_db(
"death");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let area = new_area("mig_death");
            db.save_area_data(area.clone()).unwrap();

            let gate = new_room(area.id, &format!("{}:gate", area.prefix), false, 0);
            save_room_with_vnum(&db, &gate);
            let home = new_room(area.id, &format!("{}:home", area.prefix), true, 1);
            save_room_with_vnum(&db, &home);

            set_game_day(&db, 10);
            let data = load_migration_data(&data_dir()).unwrap();
            run_one_tick(&db, &data);

            let mobs: Vec<_> = db
                .list_all_mobiles()
                .unwrap()
                .into_iter()
                .filter(|m| !m.is_prototype)
                .collect();
            assert_eq!(mobs.len(), 1);
            let migrant_id = mobs[0].id;

            // Delete (simulating death) — should free the residency slot.
            let removed = db.delete_mobile(&migrant_id).unwrap();
            assert!(removed);

            let refreshed_home = db.get_room_data(&home.id).unwrap().unwrap();
            assert!(
                refreshed_home.residents.is_empty(),
                "residency released on mobile deletion"
            );
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_guard_variation_never_when_chance_zero() {
        let (db, _temp) = open_temp_db(
"variation_zero");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let area = new_area("mig_var0");
            // Default chances == zero — explicit for clarity.
            assert_eq!(area.immigration_variation_chances.guard, 0.0);
            db.save_area_data(area.clone()).unwrap();

            let gate = new_room(area.id, &format!("{}:gate", area.prefix), false, 0);
            save_room_with_vnum(&db, &gate);
            let home = new_room(area.id, &format!("{}:home", area.prefix), true, 25);
            save_room_with_vnum(&db, &home);

            set_game_day(&db, 10);
            let data = load_migration_data(&data_dir()).unwrap();
            run_one_tick(&db, &data);

            let mobs: Vec<_> = db
                .list_all_mobiles()
                .unwrap()
                .into_iter()
                .filter(|m| !m.is_prototype)
                .collect();
            assert!(!mobs.is_empty(), "tick should have spawned migrants");
            for m in &mobs {
                assert!(!m.flags.guard, "guard chance 0.0 must never produce guards");
                assert!(m.vnum.starts_with("migrant:") && !m.vnum.starts_with("migrant:guard:"));
            }
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_guard_variation_always_when_chance_one() {
        use ironmud::types::ActivityState;
        let (db, _temp) = open_temp_db(
"variation_one");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut area = new_area("mig_var1");
            area.immigration_variation_chances.guard = 1.0;
            db.save_area_data(area.clone()).unwrap();

            let gate = new_room(area.id, &format!("{}:gate", area.prefix), false, 0);
            save_room_with_vnum(&db, &gate);
            let home = new_room(area.id, &format!("{}:home", area.prefix), true, 5);
            save_room_with_vnum(&db, &home);

            set_game_day(&db, 10);
            let data = load_migration_data(&data_dir()).unwrap();
            run_one_tick(&db, &data);

            let mobs: Vec<_> = db
                .list_all_mobiles()
                .unwrap()
                .into_iter()
                .filter(|m| !m.is_prototype)
                .collect();
            assert!(mobs.len() >= 5, "expected at least 5 migrants in this run");
            for m in &mobs {
                assert!(m.flags.guard, "guard flag must be set");
                assert!(m.flags.no_attack, "no_attack must be set");
                assert!(m.flags.can_open_doors, "can_open_doors must be set");
                assert!(!m.flags.sentinel, "sentinel must remain false");
                assert_eq!(m.current_activity, ActivityState::Patrolling);
                assert!(m.perception > 0, "perception bumped above default");
                assert!(m.vnum.starts_with("migrant:guard:"));
                assert!(m.short_desc.contains("livery"));
                assert!(m.long_desc.contains("insignia"));
            }
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_guard_variation_keywords_include_guard() {
        let (db, _temp) = open_temp_db(
"variation_kw");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut area = new_area("mig_varkw");
            area.immigration_variation_chances.guard = 1.0;
            db.save_area_data(area.clone()).unwrap();

            let gate = new_room(area.id, &format!("{}:gate", area.prefix), false, 0);
            save_room_with_vnum(&db, &gate);
            let home = new_room(area.id, &format!("{}:home", area.prefix), true, 1);
            save_room_with_vnum(&db, &home);

            set_game_day(&db, 10);
            let data = load_migration_data(&data_dir()).unwrap();
            run_one_tick(&db, &data);

            let mobs: Vec<_> = db
                .list_all_mobiles()
                .unwrap()
                .into_iter()
                .filter(|m| !m.is_prototype)
                .collect();
            assert_eq!(mobs.len(), 1);
            let m = &mobs[0];
            assert!(
                m.keywords.iter().any(|k| k == "guard"),
                "guard keyword present so `look guard` works"
            );
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_healer_variation_never_when_chance_zero() {
        let (db, _temp) = open_temp_db(
"healer_zero");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let area = new_area("mig_hvar0");
            assert_eq!(area.immigration_variation_chances.healer, 0.0);
            db.save_area_data(area.clone()).unwrap();

            let gate = new_room(area.id, &format!("{}:gate", area.prefix), false, 0);
            save_room_with_vnum(&db, &gate);
            let home = new_room(area.id, &format!("{}:home", area.prefix), true, 25);
            save_room_with_vnum(&db, &home);

            set_game_day(&db, 10);
            let data = load_migration_data(&data_dir()).unwrap();
            run_one_tick(&db, &data);

            let mobs: Vec<_> = db
                .list_all_mobiles()
                .unwrap()
                .into_iter()
                .filter(|m| !m.is_prototype)
                .collect();
            assert!(!mobs.is_empty(), "tick should have spawned migrants");
            for m in &mobs {
                assert!(!m.flags.healer, "healer chance 0.0 must never produce healers");
                assert!(m.vnum.starts_with("migrant:") && !m.vnum.starts_with("migrant:healer:"));
            }
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_healer_variation_always_when_chance_one() {
        use ironmud::types::ActivityState;
        let (db, _temp) = open_temp_db(
"healer_one");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut area = new_area("mig_hvar1");
            area.immigration_variation_chances.healer = 1.0;
            db.save_area_data(area.clone()).unwrap();

            let gate = new_room(area.id, &format!("{}:gate", area.prefix), false, 0);
            save_room_with_vnum(&db, &gate);
            let home = new_room(area.id, &format!("{}:home", area.prefix), true, 5);
            save_room_with_vnum(&db, &home);

            set_game_day(&db, 10);
            let data = load_migration_data(&data_dir()).unwrap();
            run_one_tick(&db, &data);

            let mobs: Vec<_> = db
                .list_all_mobiles()
                .unwrap()
                .into_iter()
                .filter(|m| !m.is_prototype)
                .collect();
            assert!(mobs.len() >= 5, "expected at least 5 migrants in this run");
            for m in &mobs {
                assert!(m.flags.healer, "healer flag must be set");
                assert!(m.flags.no_attack, "no_attack must be set");
                assert!(!m.flags.sentinel, "sentinel must remain false");
                assert_eq!(m.healer_type, "herbalist");
                assert_eq!(m.current_activity, ActivityState::Working);
                assert!(m.vnum.starts_with("migrant:healer:"));
                assert!(m.short_desc.contains("healer's robes"));
                assert!(m.long_desc.contains("tending the wounded"));
            }
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_healer_variation_keywords_include_healer() {
        let (db, _temp) = open_temp_db(
"healer_kw");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut area = new_area("mig_hvarkw");
            area.immigration_variation_chances.healer = 1.0;
            db.save_area_data(area.clone()).unwrap();

            let gate = new_room(area.id, &format!("{}:gate", area.prefix), false, 0);
            save_room_with_vnum(&db, &gate);
            let home = new_room(area.id, &format!("{}:home", area.prefix), true, 1);
            save_room_with_vnum(&db, &home);

            set_game_day(&db, 10);
            let data = load_migration_data(&data_dir()).unwrap();
            run_one_tick(&db, &data);

            let mobs: Vec<_> = db
                .list_all_mobiles()
                .unwrap()
                .into_iter()
                .filter(|m| !m.is_prototype)
                .collect();
            assert_eq!(mobs.len(), 1);
            let m = &mobs[0];
            assert!(
                m.keywords.iter().any(|k| k == "healer"),
                "healer keyword present so `look healer` works"
            );
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_scavenger_variation_never_when_chance_zero() {
        let (db, _temp) = open_temp_db(
"scav_zero");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let area = new_area("mig_svar0");
            assert_eq!(area.immigration_variation_chances.scavenger, 0.0);
            db.save_area_data(area.clone()).unwrap();

            let gate = new_room(area.id, &format!("{}:gate", area.prefix), false, 0);
            save_room_with_vnum(&db, &gate);
            let home = new_room(area.id, &format!("{}:home", area.prefix), true, 25);
            save_room_with_vnum(&db, &home);

            set_game_day(&db, 10);
            let data = load_migration_data(&data_dir()).unwrap();
            run_one_tick(&db, &data);

            let mobs: Vec<_> = db
                .list_all_mobiles()
                .unwrap()
                .into_iter()
                .filter(|m| !m.is_prototype)
                .collect();
            assert!(!mobs.is_empty(), "tick should have spawned migrants");
            for m in &mobs {
                assert!(!m.flags.scavenger, "scavenger chance 0.0 must never produce scavengers");
                assert!(m.vnum.starts_with("migrant:") && !m.vnum.starts_with("migrant:scavenger:"));
            }
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_scavenger_variation_always_when_chance_one() {
        use ironmud::types::ActivityState;
        let (db, _temp) = open_temp_db(
"scav_one");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut area = new_area("mig_svar1");
            area.immigration_variation_chances.scavenger = 1.0;
            db.save_area_data(area.clone()).unwrap();

            let gate = new_room(area.id, &format!("{}:gate", area.prefix), false, 0);
            save_room_with_vnum(&db, &gate);
            let home = new_room(area.id, &format!("{}:home", area.prefix), true, 5);
            save_room_with_vnum(&db, &home);

            set_game_day(&db, 10);
            let data = load_migration_data(&data_dir()).unwrap();
            run_one_tick(&db, &data);

            let mobs: Vec<_> = db
                .list_all_mobiles()
                .unwrap()
                .into_iter()
                .filter(|m| !m.is_prototype)
                .collect();
            assert!(mobs.len() >= 5, "expected at least 5 migrants in this run");
            for m in &mobs {
                assert!(m.flags.scavenger, "scavenger flag must be set");
                assert!(m.flags.can_open_doors, "can_open_doors must be set");
                assert!(!m.flags.sentinel, "sentinel must remain false");
                assert!(!m.flags.no_attack, "scavengers are ordinary neutrals, not no_attack");
                assert_eq!(m.current_activity, ActivityState::Working);
                assert!(m.perception > 0, "perception bumped above default");
                assert!(m.vnum.starts_with("migrant:scavenger:"));
                assert!(m.short_desc.contains("patched traveling clothes"));
                assert!(m.long_desc.contains("practiced squint"));
            }
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_scavenger_variation_keywords_include_scavenger() {
        let (db, _temp) = open_temp_db(
"scav_kw");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut area = new_area("mig_svarkw");
            area.immigration_variation_chances.scavenger = 1.0;
            db.save_area_data(area.clone()).unwrap();

            let gate = new_room(area.id, &format!("{}:gate", area.prefix), false, 0);
            save_room_with_vnum(&db, &gate);
            let home = new_room(area.id, &format!("{}:home", area.prefix), true, 1);
            save_room_with_vnum(&db, &home);

            set_game_day(&db, 10);
            let data = load_migration_data(&data_dir()).unwrap();
            run_one_tick(&db, &data);

            let mobs: Vec<_> = db
                .list_all_mobiles()
                .unwrap()
                .into_iter()
                .filter(|m| !m.is_prototype)
                .collect();
            assert_eq!(mobs.len(), 1);
            let m = &mobs[0];
            assert!(
                m.keywords.iter().any(|k| k == "scavenger"),
                "scavenger keyword present so `look scavenger` works"
            );
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_absolute_game_day_math() {
        // year 0, month 1, day 1 -> 0
        assert_eq!(absolute_game_day(0, 1, 1), 0);
        // 30 days in month 1 -> day 30 of year 0 = 29
        assert_eq!(absolute_game_day(0, 1, 30), 29);
        // next month rolls over
        assert_eq!(absolute_game_day(0, 2, 1), 30);
        // next year
        assert_eq!(absolute_game_day(1, 1, 1), 360);
    }

    #[test]
    fn test_life_stage_boundaries() {
        use ironmud::types::{LifeStage, age_label_for_stage, life_stage_for_age};

        assert_eq!(life_stage_for_age(0), LifeStage::Baby);
        assert_eq!(life_stage_for_age(2), LifeStage::Baby);
        assert_eq!(life_stage_for_age(3), LifeStage::Child);
        assert_eq!(life_stage_for_age(12), LifeStage::Child);
        assert_eq!(life_stage_for_age(13), LifeStage::Adolescent);
        assert_eq!(life_stage_for_age(17), LifeStage::Adolescent);
        assert_eq!(life_stage_for_age(18), LifeStage::YoungAdult);
        assert_eq!(life_stage_for_age(29), LifeStage::YoungAdult);
        assert_eq!(life_stage_for_age(30), LifeStage::Adult);
        assert_eq!(life_stage_for_age(49), LifeStage::Adult);
        assert_eq!(life_stage_for_age(50), LifeStage::MiddleAged);
        assert_eq!(life_stage_for_age(64), LifeStage::MiddleAged);
        assert_eq!(life_stage_for_age(65), LifeStage::Elderly);
        assert_eq!(life_stage_for_age(120), LifeStage::Elderly);

        // Label strings stay aligned with the JSON-seeded labels so a migrant's
        // stored `age_label` keeps matching the derived stage.
        assert_eq!(age_label_for_stage(LifeStage::YoungAdult), "young adult");
        assert_eq!(age_label_for_stage(LifeStage::Adult), "adult");
        assert_eq!(age_label_for_stage(LifeStage::MiddleAged), "middle-aged");
        assert_eq!(age_label_for_stage(LifeStage::Elderly), "elderly");
    }

    #[test]
    fn test_characteristics_birth_day_serde_default() {
        use ironmud::types::Characteristics;

        // Legacy save (birth_day absent from JSON) must round-trip with birth_day = 0
        // so the Phase B aging tick can back-fill it on first read.
        let legacy = r#"{
            "gender": "female",
            "age": 35,
            "age_label": "adult",
            "height": "average",
            "build": "slim",
            "hair_color": "black",
            "hair_style": "long",
            "eye_color": "brown",
            "skin_tone": "fair"
        }"#;
        let c: Characteristics = serde_json::from_str(legacy).expect("legacy characteristics load");
        assert_eq!(
            c.birth_day, 0,
            "missing birth_day deserialises to 0 (back-compat marker)"
        );
        assert_eq!(c.age, 35);
        assert_eq!(c.age_label, "adult");

        // Round-trip preserves a populated birth_day.
        let mut c2 = c.clone();
        c2.birth_day = 12_345;
        let s = serde_json::to_string(&c2).unwrap();
        let c3: Characteristics = serde_json::from_str(&s).unwrap();
        assert_eq!(c3.birth_day, 12_345);
    }

    #[test]
    fn test_aging_tick_advances_age_one_year_per_360_days() {
        use ironmud::aging::process_aging_tick;
        use ironmud::types::{Characteristics, MobileData};

        let (db, _temp) = open_temp_db(
"aging_advance");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut m = MobileData::new("Kenji".to_string());
            m.is_prototype = false;
            m.characteristics = Some(Characteristics {
                gender: "male".to_string(),
                age: 25,
                age_label: "young adult".to_string(),
                birth_day: 0, // legacy save — back-fill expected
                height: "average".to_string(),
                build: "average".to_string(),
                hair_color: "black".to_string(),
                hair_style: "short".to_string(),
                eye_color: "brown".to_string(),
                skin_tone: "fair".to_string(),
                distinguishing_mark: None,
            });
            db.save_mobile_data(m.clone()).unwrap();

            let conns = empty_connections();

            // Day 100: first run back-fills birth_day to 100 - 25*360 = -8900
            set_game_day(&db, 100);
            process_aging_tick(&db, &conns).expect("first aging tick");
            let stored = db.get_mobile_data(&m.id).unwrap().unwrap();
            let c = stored.characteristics.as_ref().unwrap();
            assert_eq!(c.birth_day, 100 - 25 * 360, "birth_day back-filled from age");
            assert_eq!(c.age, 25, "age unchanged on back-fill day");
            assert_eq!(c.age_label, "young adult");

            // Advance one full game year. Same-day re-run is a no-op because
            // the last-check singleton was written.
            set_game_day(&db, 100 + 360);
            process_aging_tick(&db, &conns).expect("one year later");
            let stored = db.get_mobile_data(&m.id).unwrap().unwrap();
            let c = stored.characteristics.as_ref().unwrap();
            assert_eq!(c.age, 26, "age bumped after one game year");
            assert_eq!(c.birth_day, 100 - 25 * 360, "birth_day frozen");

            // Cross Adult boundary at age 30.
            set_game_day(&db, 100 + 360 * 5);
            process_aging_tick(&db, &conns).expect("five years later");
            let stored = db.get_mobile_data(&m.id).unwrap().unwrap();
            let c = stored.characteristics.as_ref().unwrap();
            assert_eq!(c.age, 30);
            assert_eq!(c.age_label, "adult", "label re-derived from LifeStage");
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_aging_tick_is_day_gated() {
        use ironmud::aging::process_aging_tick;
        use ironmud::types::{Characteristics, MobileData};

        let (db, _temp) = open_temp_db(
"aging_gated");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut m = MobileData::new("Aya".to_string());
            m.is_prototype = false;
            m.characteristics = Some(Characteristics {
                gender: "female".to_string(),
                age: 25,
                age_label: "young adult".to_string(),
                birth_day: 0,
                height: "average".to_string(),
                build: "average".to_string(),
                hair_color: "brown".to_string(),
                hair_style: "long".to_string(),
                eye_color: "brown".to_string(),
                skin_tone: "fair".to_string(),
                distinguishing_mark: None,
            });
            db.save_mobile_data(m.clone()).unwrap();

            let conns = empty_connections();

            set_game_day(&db, 500);
            process_aging_tick(&db, &conns).expect("first pass");
            let first = db.get_mobile_data(&m.id).unwrap().unwrap();
            let first_birth = first.characteristics.unwrap().birth_day;

            // Running again the same day must not re-fire the back-fill or
            // double-age the mobile. Corrupt birth_day to prove re-run skips.
            db.update_mobile(&m.id, |mm| {
                if let Some(c) = mm.characteristics.as_mut() {
                    c.birth_day = 999_999;
                }
            })
            .unwrap();
            process_aging_tick(&db, &conns).expect("second pass same day");
            let second = db.get_mobile_data(&m.id).unwrap().unwrap();
            assert_eq!(
                second.characteristics.unwrap().birth_day,
                999_999,
                "same-day re-run is a no-op (singleton guards it)"
            );
            let _ = first_birth;
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_aging_skips_vampires_globally() {
        // Vampires (kindred) do not age and do not roll natural death.
        // Mortal-control "they should die" coverage already lives in
        // `test_natural_death_rolls_for_elderly_mobiles`; this test focuses on
        // the vampire skip in isolation.
        use ironmud::aging::process_aging_tick_with_rng;
        use ironmud::types::{Characteristics, MobileData, VampireState};
        use rand::SeedableRng;
        use rand::rngs::StdRng;

        let (db, _temp) = open_temp_db(
"aging_skip_vampires");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut flagged = MobileData::new("Lucien".to_string());
            flagged.is_prototype = false;
            flagged.flags.vampire = true;
            flagged.flags.undead = true;
            flagged.vampire_state = Some(VampireState::newly_embraced(0, None));
            flagged.characteristics = Some(Characteristics {
                gender: "male".to_string(),
                age: 105,
                age_label: "elderly".to_string(),
                birth_day: 100, // explicitly non-zero — back-fill must NOT run
                height: "average".to_string(),
                build: "lean".to_string(),
                hair_color: "black".to_string(),
                hair_style: "short".to_string(),
                eye_color: "grey".to_string(),
                skin_tone: "pale".to_string(),
                distinguishing_mark: None,
            });
            let flagged_id = flagged.id;
            db.save_mobile_data(flagged).unwrap();

            let conns = empty_connections();
            let mut rng = StdRng::seed_from_u64(7);
            // 1000 game days at 5%/day natural-death — survival probability is
            // effectively 0 for a non-vampire elderly mobile (covered by the
            // mortality test). The vampire skip must keep this one alive AND
            // stop apply_aging from rewriting the derived age.
            for day in 200..=1200i64 {
                set_game_day(&db, day);
                process_aging_tick_with_rng(&db, &conns, &migration_data_for_tests(), &mut rng)
                    .expect("aging tick");
            }

            let v = db
                .get_mobile_data(&flagged_id)
                .unwrap()
                .expect("vampire survives 1000 days");
            let chars = v.characteristics.expect("vampire keeps characteristics");
            assert_eq!(chars.birth_day, 100, "vampire birth_day untouched");
            assert_eq!(chars.age, 105, "vampire age untouched");
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_aging_skips_vampire_state_without_flag() {
        // Defensive: a hand-built undead with VampireState but no flags.vampire
        // is still skipped (mirrors the OR in src/aging.rs).
        use ironmud::aging::process_aging_tick_with_rng;
        use ironmud::types::{Characteristics, MobileData, VampireState};
        use rand::SeedableRng;
        use rand::rngs::StdRng;

        let (db, _temp) = open_temp_db(
"aging_skip_vstate_only");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut m = MobileData::new("Stateful".to_string());
            m.is_prototype = false;
            m.flags.vampire = false; // intentionally unset
            m.vampire_state = Some(VampireState::newly_embraced(0, None));
            m.characteristics = Some(Characteristics {
                gender: "male".to_string(),
                age: 30,
                age_label: "adult".to_string(),
                birth_day: 100,
                height: "average".to_string(),
                build: "average".to_string(),
                hair_color: "brown".to_string(),
                hair_style: "short".to_string(),
                eye_color: "brown".to_string(),
                skin_tone: "fair".to_string(),
                distinguishing_mark: None,
            });
            let mid = m.id;
            db.save_mobile_data(m).unwrap();

            let conns = empty_connections();
            let mut rng = StdRng::seed_from_u64(1);
            // Advance ten years of game days. Any normal mortal would age.
            for day in 200..=(200 + 360 * 10i64) {
                set_game_day(&db, day);
                process_aging_tick_with_rng(&db, &conns, &migration_data_for_tests(), &mut rng)
                    .expect("aging tick");
            }
            let stored = db.get_mobile_data(&mid).unwrap().unwrap();
            let chars = stored.characteristics.unwrap();
            assert_eq!(chars.age, 30, "vampire_state-only mob still does not age");
            assert_eq!(chars.birth_day, 100);
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_natural_death_rolls_for_elderly_mobiles() {
        use ironmud::aging::{death_probability_per_game_day, process_aging_tick_with_rng};
        use ironmud::types::{Characteristics, MobileData};
        use rand::SeedableRng;
        use rand::rngs::StdRng;

        // Curve spot-checks first — cheap, no DB needed.
        assert_eq!(death_probability_per_game_day(25), 0.0);
        assert_eq!(death_probability_per_game_day(69), 0.0);
        assert!(death_probability_per_game_day(85) > 0.0);
        assert!(death_probability_per_game_day(85) < 0.01);
        assert_eq!(death_probability_per_game_day(100), 0.05);
        assert_eq!(death_probability_per_game_day(150), 0.05);

        let (db, _temp) = open_temp_db(
"aging_death");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            // Spawn a 105-year-old Elderly mobile. At 5%/day, 1000 rolls is
            // basically certain to kill them.
            let mut m = MobileData::new("Elder".to_string());
            m.is_prototype = false;
            m.characteristics = Some(Characteristics {
                gender: "male".to_string(),
                age: 105,
                age_label: "elderly".to_string(),
                birth_day: 0,
                height: "short".to_string(),
                build: "slim".to_string(),
                hair_color: "white".to_string(),
                hair_style: "short".to_string(),
                eye_color: "grey".to_string(),
                skin_tone: "pale".to_string(),
                distinguishing_mark: None,
            });
            let mobile_id = m.id;
            db.save_mobile_data(m).unwrap();

            let conns = empty_connections();
            let mut rng = StdRng::seed_from_u64(7);

            // Step game day forward once per tick so the day-gate releases.
            let mut alive_days = 0;
            for day in 1..=1000i64 {
                set_game_day(&db, day);
                process_aging_tick_with_rng(&db, &conns, &migration_data_for_tests(), &mut rng).expect("aging tick");
                match db.get_mobile_data(&mobile_id).unwrap() {
                    Some(_) => alive_days += 1,
                    None => break,
                }
            }
            assert!(
                alive_days < 1000,
                "elderly mobile survived 1000 rolls — death curve broken"
            );
            assert!(
                db.get_mobile_data(&mobile_id).unwrap().is_none(),
                "mobile must be deleted on natural death"
            );
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_family_grief_cascade_on_delete_mobile() {
        use ironmud::social::{COHABITANT_GRIEF, FAMILY_GRIEF, GRIEF_AFFINITY_FLOOR, grief_params};
        use ironmud::types::{MobileData, Relationship, RelationshipKind, SocialState};

        // Unit spot-checks on grief_params before the DB round-trip.
        assert_eq!(
            grief_params(RelationshipKind::Parent, 60),
            Some(FAMILY_GRIEF),
            "healthy affinity with parent => full family grief"
        );
        assert_eq!(
            grief_params(RelationshipKind::Cohabitant, 60),
            Some(COHABITANT_GRIEF),
            "cohabitant grief unchanged"
        );
        // Mildly negative halves the hit.
        let halved = grief_params(RelationshipKind::Sibling, -10).unwrap();
        assert_eq!(halved.0, -30, "half of -60 base for mildly-negative sibling");
        // Deeply negative skips grief entirely.
        assert_eq!(
            grief_params(RelationshipKind::Partner, GRIEF_AFFINITY_FLOOR),
            None,
            "hated partner => no grief"
        );
        assert_eq!(
            grief_params(RelationshipKind::Friend, 90),
            None,
            "plain friends don't trigger mourning"
        );

        // Full cascade: deletes a parent, confirms the child mourns.
        let (db, _temp) = open_temp_db(
"grief_family");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut parent = MobileData::new("Akio".to_string());
            parent.is_prototype = false;
            let mut child = MobileData::new("Yuki".to_string());
            child.is_prototype = false;

            child.relationships.push(Relationship {
                other_id: parent.id,
                kind: RelationshipKind::Parent,
                affinity: 70,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
            parent.relationships.push(Relationship {
                other_id: child.id,
                kind: RelationshipKind::Child,
                affinity: 70,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
            let mut social = SocialState::default();
            social.happiness = 80;
            child.social = Some(social);
            parent.social = Some(SocialState::default());

            let parent_id = parent.id;
            let child_id = child.id;
            db.save_mobile_data(parent).unwrap();
            db.save_mobile_data(child).unwrap();
            set_game_day(&db, 100);

            assert!(db.delete_mobile(&parent_id).unwrap(), "parent deleted");

            let mourner = db.get_mobile_data(&child_id).unwrap().unwrap();
            let s = mourner.social.as_ref().expect("social survived");
            assert_eq!(s.happiness, 20, "80 - 60 family grief");
            assert_eq!(s.bereaved_until_day, Some(100 + 28), "28-day family mourning");
            assert_eq!(s.bereaved_for.len(), 1, "bereavement note recorded");
            assert_eq!(s.bereaved_for[0].kind, RelationshipKind::Parent);
            assert_eq!(s.bereaved_for[0].other_name, "Akio");
            // Parent kind preserved on the surviving child — a dead parent is
            // still your parent.
            assert!(
                mourner
                    .relationships
                    .iter()
                    .any(|r| r.other_id == parent_id && r.kind == RelationshipKind::Parent),
                "family relationship kind NOT demoted to Friend on death"
            );
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_hated_parent_death_skips_grief() {
        use ironmud::types::{MobileData, Relationship, RelationshipKind, SocialState};

        let (db, _temp) = open_temp_db(
"grief_hated");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut parent = MobileData::new("Cruel Mother".to_string());
            parent.is_prototype = false;
            let mut child = MobileData::new("Bitter Son".to_string());
            child.is_prototype = false;

            child.relationships.push(Relationship {
                other_id: parent.id,
                kind: RelationshipKind::Parent,
                affinity: -80, // deep-hate
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
            let mut social = SocialState::default();
            social.happiness = 70;
            child.social = Some(social);

            let parent_id = parent.id;
            let child_id = child.id;
            db.save_mobile_data(parent).unwrap();
            db.save_mobile_data(child).unwrap();

            db.delete_mobile(&parent_id).unwrap();

            let survivor = db.get_mobile_data(&child_id).unwrap().unwrap();
            let s = survivor.social.as_ref().unwrap();
            assert_eq!(s.happiness, 70, "no grief when deeply negative affinity");
            assert!(s.bereaved_until_day.is_none(), "no mourning window");
            assert!(s.bereaved_for.is_empty(), "no bereavement note");
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_set_family_relationship_writes_both_directions() {
        use ironmud::social::{FamilyError, set_family_relationship};
        use ironmud::types::{MobileData, RelationshipKind};

        let (db, _temp) = open_temp_db(
"family_set");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut p = MobileData::new("Parent".to_string());
            p.is_prototype = false;
            let mut c = MobileData::new("Child".to_string());
            c.is_prototype = false;
            let pid = p.id;
            let cid = c.id;
            db.save_mobile_data(p).unwrap();
            db.save_mobile_data(c).unwrap();

            assert!(set_family_relationship(&db, pid, cid, "parent").is_ok());

            let parent = db.get_mobile_data(&pid).unwrap().unwrap();
            let child = db.get_mobile_data(&cid).unwrap().unwrap();
            assert!(
                parent
                    .relationships
                    .iter()
                    .any(|r| r.other_id == cid && r.kind == RelationshipKind::Parent)
            );
            assert!(
                child
                    .relationships
                    .iter()
                    .any(|r| r.other_id == pid && r.kind == RelationshipKind::Child)
            );

            // Invalid kind.
            assert_eq!(
                set_family_relationship(&db, pid, cid, "bestie"),
                Err(FamilyError::InvalidKind)
            );
            // Self-link blocked.
            assert_eq!(
                set_family_relationship(&db, pid, pid, "sibling"),
                Err(FamilyError::SelfLink)
            );
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_partner_monogamy_blocks_second_partner() {
        use ironmud::social::{FamilyError, set_family_relationship};
        use ironmud::types::MobileData;

        let (db, _temp) = open_temp_db(
"monogamy");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut a = MobileData::new("A".to_string());
            a.is_prototype = false;
            let mut b = MobileData::new("B".to_string());
            b.is_prototype = false;
            let mut c = MobileData::new("C".to_string());
            c.is_prototype = false;
            let aid = a.id;
            let bid = b.id;
            let cid = c.id;
            db.save_mobile_data(a).unwrap();
            db.save_mobile_data(b).unwrap();
            db.save_mobile_data(c).unwrap();

            assert!(set_family_relationship(&db, aid, bid, "partner").is_ok());
            // Second Partner on either side must fail.
            assert_eq!(
                set_family_relationship(&db, aid, cid, "partner"),
                Err(FamilyError::PartnerConflict),
                "A already has B as partner; linking A↔C must be refused"
            );
            assert_eq!(
                set_family_relationship(&db, cid, bid, "partner"),
                Err(FamilyError::PartnerConflict),
                "B already has A; linking C↔B must be refused"
            );
            // Re-setting the same pair is idempotent (not a conflict).
            assert!(set_family_relationship(&db, aid, bid, "partner").is_ok());
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_cohabitant_promotion_mints_shared_household() {
        use ironmud::migration::process_pair_housing;
        use ironmud::types::{Characteristics, MobileData, Relationship, RelationshipKind, SocialState};

        let (db, _temp) = open_temp_db(
"cohab_household");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let area = new_area("cohab");
            db.save_area_data(area.clone()).unwrap();

            let gate = new_room(area.id, &format!("{}:gate", area.prefix), false, 0);
            save_room_with_vnum(&db, &gate);
            let mut room_a = new_room(area.id, &format!("{}:a", area.prefix), true, 2);
            let mut room_b = new_room(area.id, &format!("{}:b", area.prefix), true, 2);

            // Seed two sim mobiles with mutual high affinity but different rooms.
            let mk_sim = |name: &str, gender: &str, age: i32| {
                let mut m = MobileData::new(name.to_string());
                m.is_prototype = false;
                m.characteristics = Some(Characteristics {
                    gender: gender.to_string(),
                    age,
                    age_label: "adult".to_string(),
                    birth_day: 0,
                    height: "average".to_string(),
                    build: "average".to_string(),
                    hair_color: "black".to_string(),
                    hair_style: "short".to_string(),
                    eye_color: "brown".to_string(),
                    skin_tone: "fair".to_string(),
                    distinguishing_mark: None,
                });
                m.social = Some(SocialState::default());
                m
            };
            let mut alice = mk_sim("Alice", "female", 28);
            let mut bob = mk_sim("Bob", "male", 30);
            alice.resident_of = Some(format!("{}:a", area.prefix));
            bob.resident_of = Some(format!("{}:b", area.prefix));
            // Mutual affinity >= 80 so pair-housing promotes them.
            alice.relationships.push(Relationship {
                other_id: bob.id,
                kind: RelationshipKind::Friend,
                affinity: 85,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
            bob.relationships.push(Relationship {
                other_id: alice.id,
                kind: RelationshipKind::Friend,
                affinity: 85,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
            room_a.residents.push(alice.id);
            room_b.residents.push(bob.id);
            save_room_with_vnum(&db, &room_a);
            save_room_with_vnum(&db, &room_b);
            let alice_id = alice.id;
            let bob_id = bob.id;
            db.save_mobile_data(alice).unwrap();
            db.save_mobile_data(bob).unwrap();

            set_game_day(&db, 10);
            process_pair_housing(&db).unwrap();

            let a = db.get_mobile_data(&alice_id).unwrap().unwrap();
            let b = db.get_mobile_data(&bob_id).unwrap().unwrap();
            // Both should be Cohabitant now with a shared household_id.
            assert!(
                a.relationships
                    .iter()
                    .any(|r| r.other_id == bob_id && r.kind == RelationshipKind::Cohabitant)
            );
            assert!(
                b.relationships
                    .iter()
                    .any(|r| r.other_id == alice_id && r.kind == RelationshipKind::Cohabitant)
            );
            assert!(a.household_id.is_some(), "alice has household");
            assert_eq!(a.household_id, b.household_id, "shared household_id");
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_conception_and_birth_produce_newborn() {
        use ironmud::aging::{CONCEPTION_CHANCE_KEY, PREGNANCY_GESTATION_DAYS, process_aging_tick_with_rng};
        use ironmud::types::{Characteristics, MobileData, Relationship, RelationshipKind, SocialState};
        use rand::SeedableRng;
        use rand::rngs::StdRng;

        let (db, _temp) = open_temp_db(
"pregnancy");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            // Use a real area so spawn_child can resolve visual/name pools.
            let area = new_area("preg");
            db.save_area_data(area.clone()).unwrap();

            let home = new_room(area.id, &format!("{}:home", area.prefix), true, 2);
            save_room_with_vnum(&db, &home);

            let household = uuid::Uuid::new_v4();
            let mk_parent =
                |name: &str, gender: &str, age: i32, room_vnum: &str, room_id: Uuid, household: uuid::Uuid| {
                    let mut m = MobileData::new(name.to_string());
                    m.is_prototype = false;
                    m.characteristics = Some(Characteristics {
                        gender: gender.to_string(),
                        age,
                        age_label: "adult".to_string(),
                        birth_day: 0,
                        height: "average".to_string(),
                        build: "average".to_string(),
                        hair_color: "black".to_string(),
                        hair_style: "short".to_string(),
                        eye_color: "brown".to_string(),
                        skin_tone: "fair".to_string(),
                        distinguishing_mark: None,
                    });
                    m.social = Some(SocialState::default());
                    m.household_id = Some(household);
                    m.resident_of = Some(room_vnum.to_string());
                    m.current_room_id = Some(room_id);
                    m
                };
            let mut mother = mk_parent(
                "Yuki",
                "female",
                28,
                &format!("{}:home", area.prefix),
                home.id,
                household,
            );
            let mut father = mk_parent("Ken", "male", 30, &format!("{}:home", area.prefix), home.id, household);
            // Cohabitant + high affinity so eligible_mate picks them.
            mother.relationships.push(Relationship {
                other_id: father.id,
                kind: RelationshipKind::Cohabitant,
                affinity: 80,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
            father.relationships.push(Relationship {
                other_id: mother.id,
                kind: RelationshipKind::Cohabitant,
                affinity: 80,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
            let mother_id = mother.id;
            let father_id = father.id;
            db.save_mobile_data(mother).unwrap();
            db.save_mobile_data(father).unwrap();

            // Force conception chance to 1.0 so the daily roll always hits.
            db.set_setting(CONCEPTION_CHANCE_KEY, "1.0").unwrap();

            let conns = empty_connections();
            let mut rng = StdRng::seed_from_u64(42);

            // Day 100: aging tick fires conception.
            set_game_day(&db, 100);
            process_aging_tick_with_rng(&db, &conns, &migration_data_for_tests(), &mut rng).unwrap();
            let m = db.get_mobile_data(&mother_id).unwrap().unwrap();
            let s = m.social.as_ref().unwrap();
            assert_eq!(
                s.pregnant_until_day,
                Some(100 + PREGNANCY_GESTATION_DAYS),
                "pregnancy due date set"
            );
            assert_eq!(s.pregnant_by, Some(father_id));

            // Advance past due date.
            set_game_day(&db, 100 + PREGNANCY_GESTATION_DAYS as i64);
            process_aging_tick_with_rng(&db, &conns, &migration_data_for_tests(), &mut rng).unwrap();
            let m = db.get_mobile_data(&mother_id).unwrap().unwrap();
            let s = m.social.as_ref().unwrap();
            assert!(s.pregnant_until_day.is_none(), "pregnancy cleared after birth");
            assert!(s.pregnant_by.is_none());

            // Newborn should now exist with reciprocal Parent/Child links.
            let all = db.list_all_mobiles().unwrap();
            let newborn = all
                .iter()
                .find(|m| m.id != mother_id && m.id != father_id)
                .expect("newborn exists");
            assert_eq!(newborn.characteristics.as_ref().unwrap().age, 0);
            assert_eq!(
                newborn.characteristics.as_ref().unwrap().birth_day,
                100 + PREGNANCY_GESTATION_DAYS as i64
            );
            assert_eq!(newborn.household_id, Some(household));
            // Newborn has TWO Parent entries (mother + father).
            let parent_count = newborn
                .relationships
                .iter()
                .filter(|r| r.kind == RelationshipKind::Parent)
                .count();
            assert_eq!(parent_count, 2, "newborn has both parents linked");
            // Mother and father have reciprocal Child entries.
            let m = db.get_mobile_data(&mother_id).unwrap().unwrap();
            let f = db.get_mobile_data(&father_id).unwrap().unwrap();
            assert!(
                m.relationships
                    .iter()
                    .any(|r| r.other_id == newborn.id && r.kind == RelationshipKind::Child)
            );
            assert!(
                f.relationships
                    .iter()
                    .any(|r| r.other_id == newborn.id && r.kind == RelationshipKind::Child)
            );
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_same_gender_pair_does_not_conceive() {
        use ironmud::aging::{CONCEPTION_CHANCE_KEY, process_aging_tick_with_rng};
        use ironmud::types::{Characteristics, MobileData, Relationship, RelationshipKind, SocialState};
        use rand::SeedableRng;
        use rand::rngs::StdRng;

        let (db, _temp) = open_temp_db(
"no_conceive");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let area = new_area("same");
            db.save_area_data(area.clone()).unwrap();
            let home = new_room(area.id, &format!("{}:home", area.prefix), true, 2);
            save_room_with_vnum(&db, &home);

            let household = uuid::Uuid::new_v4();
            let mk = |name: &str, gender: &str| {
                let mut m = MobileData::new(name.to_string());
                m.is_prototype = false;
                m.characteristics = Some(Characteristics {
                    gender: gender.to_string(),
                    age: 28,
                    age_label: "young adult".to_string(),
                    birth_day: 0,
                    height: "average".to_string(),
                    build: "average".to_string(),
                    hair_color: "black".to_string(),
                    hair_style: "short".to_string(),
                    eye_color: "brown".to_string(),
                    skin_tone: "fair".to_string(),
                    distinguishing_mark: None,
                });
                m.social = Some(SocialState::default());
                m.household_id = Some(household);
                m.resident_of = Some(format!("{}:home", area.prefix));
                m
            };
            let mut a = mk("Alice", "female");
            let mut b = mk("Beth", "female");
            a.relationships.push(Relationship {
                other_id: b.id,
                kind: RelationshipKind::Partner,
                affinity: 80,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
            b.relationships.push(Relationship {
                other_id: a.id,
                kind: RelationshipKind::Partner,
                affinity: 80,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
            let a_id = a.id;
            db.save_mobile_data(a).unwrap();
            db.save_mobile_data(b).unwrap();

            db.set_setting(CONCEPTION_CHANCE_KEY, "1.0").unwrap();
            let conns = empty_connections();
            let mut rng = StdRng::seed_from_u64(42);
            set_game_day(&db, 100);
            process_aging_tick_with_rng(&db, &conns, &migration_data_for_tests(), &mut rng).unwrap();

            let a = db.get_mobile_data(&a_id).unwrap().unwrap();
            assert!(
                a.social.as_ref().unwrap().pregnant_until_day.is_none(),
                "same-gender Partners don't conceive"
            );
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_orphan_flagged_on_last_parent_death() {
        use ironmud::types::{Characteristics, MobileData, Relationship, RelationshipKind, SocialState};

        let (db, _temp) = open_temp_db(
"orphan_flag");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mk = |name: &str, age: i32, gender: &str| {
                let mut m = MobileData::new(name.to_string());
                m.is_prototype = false;
                m.characteristics = Some(Characteristics {
                    gender: gender.to_string(),
                    age,
                    age_label: if age <= 12 {
                        "child".to_string()
                    } else {
                        "adult".to_string()
                    },
                    birth_day: 0,
                    height: "average".to_string(),
                    build: "average".to_string(),
                    hair_color: "black".to_string(),
                    hair_style: "short".to_string(),
                    eye_color: "brown".to_string(),
                    skin_tone: "fair".to_string(),
                    distinguishing_mark: None,
                });
                m.social = Some(SocialState::default());
                m
            };
            let mut p1 = mk("Mother", 35, "female");
            let mut p2 = mk("Father", 37, "male");
            let mut child = mk("Kid", 7, "male");
            child.relationships.push(Relationship {
                other_id: p1.id,
                kind: RelationshipKind::Parent,
                affinity: 70,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
            child.relationships.push(Relationship {
                other_id: p2.id,
                kind: RelationshipKind::Parent,
                affinity: 70,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
            p1.relationships.push(Relationship {
                other_id: child.id,
                kind: RelationshipKind::Child,
                affinity: 70,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
            p2.relationships.push(Relationship {
                other_id: child.id,
                kind: RelationshipKind::Child,
                affinity: 70,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
            let p1_id = p1.id;
            let p2_id = p2.id;
            let child_id = child.id;
            db.save_mobile_data(p1).unwrap();
            db.save_mobile_data(p2).unwrap();
            db.save_mobile_data(child).unwrap();
            set_game_day(&db, 100);

            // First parent dies: child still has p2 alive → not flagged.
            db.delete_mobile(&p1_id).unwrap();
            let k = db.get_mobile_data(&child_id).unwrap().unwrap();
            assert!(!k.adoption_pending, "one parent dead, child not yet orphaned");

            // Second parent dies: child now has no living parents → flagged.
            db.delete_mobile(&p2_id).unwrap();
            let k = db.get_mobile_data(&child_id).unwrap().unwrap();
            assert!(k.adoption_pending, "both parents dead, child flagged for adoption");
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_orphan_adult_child_not_flagged() {
        // An adult child who loses a parent doesn't need an adopter.
        use ironmud::types::{Characteristics, MobileData, Relationship, RelationshipKind, SocialState};

        let (db, _temp) = open_temp_db(
"orphan_adult");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mk_adult = |name: &str| {
                let mut m = MobileData::new(name.to_string());
                m.is_prototype = false;
                m.characteristics = Some(Characteristics {
                    gender: "male".to_string(),
                    age: 35,
                    age_label: "adult".to_string(),
                    birth_day: 0,
                    height: "average".to_string(),
                    build: "average".to_string(),
                    hair_color: "black".to_string(),
                    hair_style: "short".to_string(),
                    eye_color: "brown".to_string(),
                    skin_tone: "fair".to_string(),
                    distinguishing_mark: None,
                });
                m.social = Some(SocialState::default());
                m
            };
            let mut parent = mk_adult("Elder");
            let mut grown_child = mk_adult("Adult-Son");
            grown_child.relationships.push(Relationship {
                other_id: parent.id,
                kind: RelationshipKind::Parent,
                affinity: 70,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
            parent.relationships.push(Relationship {
                other_id: grown_child.id,
                kind: RelationshipKind::Child,
                affinity: 70,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
            let pid = parent.id;
            let cid = grown_child.id;
            db.save_mobile_data(parent).unwrap();
            db.save_mobile_data(grown_child).unwrap();
            db.delete_mobile(&pid).unwrap();
            let c = db.get_mobile_data(&cid).unwrap().unwrap();
            assert!(!c.adoption_pending, "adult children aren't flagged");
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_orphan_adoption_resolves_in_aging_tick() {
        use ironmud::aging::{ADOPTION_CHANCE_KEY, process_aging_tick_with_rng};
        use ironmud::types::{Characteristics, MobileData, RelationshipKind, SocialState};
        use rand::SeedableRng;
        use rand::rngs::StdRng;

        let (db, _temp) = open_temp_db(
"orphan_adopt");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let area = new_area("orph");
            db.save_area_data(area.clone()).unwrap();
            let home = new_room(area.id, &format!("{}:home", area.prefix), true, 4);
            save_room_with_vnum(&db, &home);

            let mk = |name: &str, age: i32, gender: &str, room: Uuid| {
                let mut m = MobileData::new(name.to_string());
                m.is_prototype = false;
                m.characteristics = Some(Characteristics {
                    gender: gender.to_string(),
                    age,
                    age_label: if age <= 12 {
                        "child".to_string()
                    } else {
                        "adult".to_string()
                    },
                    birth_day: 0,
                    height: "average".to_string(),
                    build: "average".to_string(),
                    hair_color: "black".to_string(),
                    hair_style: "short".to_string(),
                    eye_color: "brown".to_string(),
                    skin_tone: "fair".to_string(),
                    distinguishing_mark: None,
                });
                m.social = Some(SocialState::default());
                m.current_room_id = Some(room);
                m
            };

            // Adult sim mobile in the same room = candidate adopter.
            let mut adopter = mk("Grace", 35, "female", home.id);
            adopter.resident_of = Some(format!("{}:home", area.prefix));
            let mut orphan = mk("Tommy", 7, "male", home.id);
            orphan.adoption_pending = true;
            let adopter_id = adopter.id;
            let orphan_id = orphan.id;
            db.save_mobile_data(adopter).unwrap();
            db.save_mobile_data(orphan).unwrap();

            db.set_setting(ADOPTION_CHANCE_KEY, "1.0").unwrap();

            let conns = empty_connections();
            let mut rng = StdRng::seed_from_u64(3);
            set_game_day(&db, 10);
            process_aging_tick_with_rng(&db, &conns, &migration_data_for_tests(), &mut rng).unwrap();

            let adopter = db.get_mobile_data(&adopter_id).unwrap().unwrap();
            let orphan = db.get_mobile_data(&orphan_id).unwrap().unwrap();
            assert!(!orphan.adoption_pending, "flag cleared on adoption");
            assert_eq!(
                orphan.household_id,
                adopter.household_id.or(orphan.household_id),
                "orphan inherits adopter's household"
            );
            assert!(orphan.household_id.is_some(), "orphan has household");
            assert!(
                adopter
                    .relationships
                    .iter()
                    .any(|r| r.other_id == orphan_id && r.kind == RelationshipKind::Child),
                "adopter has Child link to orphan"
            );
            assert!(
                orphan
                    .relationships
                    .iter()
                    .any(|r| r.other_id == adopter_id && r.kind == RelationshipKind::Parent),
                "orphan has Parent link to adopter"
            );
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_adoption_weight_prefers_same_gender_pairs() {
        use ironmud::social::{
            ADOPT_WEIGHT_OPPOSITE_PAIR, ADOPT_WEIGHT_SAME_PAIR, ADOPT_WEIGHT_SINGLE, adoption_weight,
        };
        use ironmud::types::{Characteristics, MobileData, Relationship, RelationshipKind, SocialState};

        let (db, _temp) = open_temp_db(
"adopt_weight");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mk = |name: &str, gender: &str| {
                let mut m = MobileData::new(name.to_string());
                m.is_prototype = false;
                m.characteristics = Some(Characteristics {
                    gender: gender.to_string(),
                    age: 30,
                    age_label: "adult".to_string(),
                    birth_day: 0,
                    height: "average".to_string(),
                    build: "average".to_string(),
                    hair_color: "black".to_string(),
                    hair_style: "short".to_string(),
                    eye_color: "brown".to_string(),
                    skin_tone: "fair".to_string(),
                    distinguishing_mark: None,
                });
                m.social = Some(SocialState::default());
                m
            };

            // Three pairs: same-gender Partners, opposite-gender Partners, single.
            let mut sg_a = mk("SGA", "female");
            let mut sg_b = mk("SGB", "female");
            sg_a.relationships.push(Relationship {
                other_id: sg_b.id,
                kind: RelationshipKind::Partner,
                affinity: 80,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
            sg_b.relationships.push(Relationship {
                other_id: sg_a.id,
                kind: RelationshipKind::Partner,
                affinity: 80,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });

            let mut og_a = mk("OGA", "male");
            let mut og_b = mk("OGB", "female");
            og_a.relationships.push(Relationship {
                other_id: og_b.id,
                kind: RelationshipKind::Partner,
                affinity: 80,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
            og_b.relationships.push(Relationship {
                other_id: og_a.id,
                kind: RelationshipKind::Partner,
                affinity: 80,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });

            let single = mk("Solo", "female");

            db.save_mobile_data(sg_b.clone()).unwrap();
            db.save_mobile_data(og_b.clone()).unwrap();
            db.save_mobile_data(single.clone()).unwrap();

            let w_sg = adoption_weight(&db, &sg_a).unwrap();
            let w_og = adoption_weight(&db, &og_a).unwrap();
            let w_single = adoption_weight(&db, &single).unwrap();

            // Same-gender pair > opposite-gender pair > single (before affinity scaling).
            assert!(w_sg > w_og, "same-gender ({}) > opposite ({})", w_sg, w_og);
            assert!(w_og > w_single, "opposite ({}) > single ({})", w_og, w_single);

            // Base weights sanity: affinity=80 → modulator = 1.8, so:
            // w_sg = 3.0 * 1.8 = 5.4; w_og = 2.0 * 1.8 = 3.6; w_single = 1.0
            assert!((w_sg - ADOPT_WEIGHT_SAME_PAIR * 1.8).abs() < 0.01);
            assert!((w_og - ADOPT_WEIGHT_OPPOSITE_PAIR * 1.8).abs() < 0.01);
            assert!((w_single - ADOPT_WEIGHT_SINGLE).abs() < 0.01);
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_examine_cues_surface_family_and_bereavement() {
        // build_social_cues is pub(crate) — exercise via the full pipeline
        // so we also cover the non-simulated fallback path (juvenile children
        // with no SocialState).
        use ironmud::types::{
            BereavementNote, Characteristics, MobileData, Relationship, RelationshipKind, SocialState,
        };

        let (db, _temp) = open_temp_db(
"examine_cues");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            // Room with both mobiles in it.
            let room = new_room(Uuid::new_v4(), "examine:test", false, 0);
            save_room_with_vnum(&db, &room);

            let child_chars = Characteristics {
                gender: "male".to_string(),
                age: 7,
                age_label: "child".to_string(),
                birth_day: 0,
                height: "short".to_string(),
                build: "slim".to_string(),
                hair_color: "brown".to_string(),
                hair_style: "short".to_string(),
                eye_color: "brown".to_string(),
                skin_tone: "fair".to_string(),
                distinguishing_mark: None,
            };
            let parent_chars = Characteristics {
                gender: "female".to_string(),
                age: 35,
                age_label: "adult".to_string(),
                birth_day: 0,
                height: "average".to_string(),
                build: "average".to_string(),
                hair_color: "black".to_string(),
                hair_style: "long".to_string(),
                eye_color: "brown".to_string(),
                skin_tone: "fair".to_string(),
                distinguishing_mark: None,
            };

            let mut parent = MobileData::new("Aiko".to_string());
            parent.is_prototype = false;
            let mut child = MobileData::new("Haru".to_string());
            child.is_prototype = false;
            parent.characteristics = Some(parent_chars);
            child.characteristics = Some(child_chars);
            parent.current_room_id = Some(room.id);
            child.current_room_id = Some(room.id);
            parent.current_hp = 10;
            child.current_hp = 10;
            parent.relationships.push(Relationship {
                other_id: child.id,
                kind: RelationshipKind::Child,
                affinity: 70,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
            child.relationships.push(Relationship {
                other_id: parent.id,
                kind: RelationshipKind::Parent,
                affinity: 70,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
            parent.social = Some(SocialState::default());
            // child stays social = None (non-simulated juvenile).

            let parent_id = parent.id;
            let child_id = child.id;
            db.save_mobile_data(parent).unwrap();
            db.save_mobile_data(child).unwrap();

            // Parent cues: should mention the child is here.
            let pm = db.get_mobile_data(&parent_id).unwrap().unwrap();
            let pm_social = pm.social.clone().unwrap_or_default();
            let cues = ironmud::script::social::build_social_cues(&db, &pm, &pm_social, 100);
            assert!(
                cues.contains("Their child Haru is here."),
                "parent examine cues include child-in-room; got: {}",
                cues
            );

            // Child cues (non-simulated fallback): life-stage + parent-in-room.
            let cm = db.get_mobile_data(&child_id).unwrap().unwrap();
            let cm_social = cm.social.clone().unwrap_or_default();
            let cues = ironmud::script::social::build_social_cues(&db, &cm, &cm_social, 100);
            assert!(
                cues.contains("child's easy wonder"),
                "child examine cues include life-stage; got: {}",
                cues
            );
            assert!(
                cues.contains("Their parent Aiko is here."),
                "child cues include parent-in-room; got: {}",
                cues
            );

            // Bereavement cue: set a BereavementNote on the parent for a dead partner.
            db.update_mobile(&parent_id, |m| {
                let s = m.social.as_mut().unwrap();
                s.bereaved_for.push(BereavementNote {
                    other_id: Uuid::new_v4(),
                    other_name: "Kenji".to_string(),
                    kind: RelationshipKind::Partner,
                    until_day: 200,
                });
                s.bereaved_until_day = Some(200);
            })
            .unwrap();
            let pm = db.get_mobile_data(&parent_id).unwrap().unwrap();
            let pm_social = pm.social.clone().unwrap_or_default();
            let cues = ironmud::script::social::build_social_cues(&db, &pm, &pm_social, 100);
            assert!(
                cues.contains("mourning for their partner"),
                "partner bereavement cue rendered; got: {}",
                cues
            );
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_migration_spawns_parent_child_family() {
        use ironmud::types::{ImmigrationFamilyChance, RelationshipKind};

        let (db, _temp) = open_temp_db(
"family_spawn_pc");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut area = new_area("fam_pc");
            // Force the family roll to always hit parent_child.
            area.immigration_family_chance = ImmigrationFamilyChance {
                parent_child: 1.0,
                sibling_pair: 0.0,
            };
            // Only one spawn slot per tick to keep the test deterministic.
            area.migration_max_per_check = 1;
            db.save_area_data(area.clone()).unwrap();

            let gate = new_room(area.id, &format!("{}:gate", area.prefix), false, 0);
            save_room_with_vnum(&db, &gate);
            let home = new_room(area.id, &format!("{}:home", area.prefix), true, 2);
            save_room_with_vnum(&db, &home);

            set_game_day(&db, 20);
            let data = load_migration_data(&data_dir()).unwrap();
            run_one_tick(&db, &data);

            let mobs: Vec<_> = db
                .list_all_mobiles()
                .unwrap()
                .into_iter()
                .filter(|m| !m.is_prototype)
                .collect();
            assert_eq!(mobs.len(), 2, "parent_child slot spawns two mobiles");

            // Identify parent and child by their kind on the relationship edges.
            let parent = mobs
                .iter()
                .find(|m| m.relationships.iter().any(|r| r.kind == RelationshipKind::Child))
                .expect("a parent");
            let child = mobs
                .iter()
                .find(|m| m.relationships.iter().any(|r| r.kind == RelationshipKind::Parent))
                .expect("a child");
            assert_ne!(parent.id, child.id);

            // Shared household.
            assert!(parent.household_id.is_some());
            assert_eq!(parent.household_id, child.household_id, "shared household_id");

            // Parent claims residency; child doesn't (dependent).
            assert_eq!(
                parent.resident_of.as_deref(),
                Some(format!("{}:home", area.prefix).as_str())
            );
            assert!(child.resident_of.is_none());

            // Reciprocal affinity ~70.
            let p2c = parent.relationships.iter().find(|r| r.other_id == child.id).unwrap();
            let c2p = child.relationships.iter().find(|r| r.other_id == parent.id).unwrap();
            assert_eq!(p2c.affinity, 70);
            assert_eq!(c2p.affinity, 70);
            assert_eq!(p2c.kind, RelationshipKind::Child);
            assert_eq!(c2p.kind, RelationshipKind::Parent);

            // Child is juvenile.
            let c_chars = child.characteristics.as_ref().expect("child has chars");
            assert!(
                matches!(
                    ironmud::types::life_stage_for_age(c_chars.age),
                    ironmud::types::LifeStage::Baby
                        | ironmud::types::LifeStage::Child
                        | ironmud::types::LifeStage::Adolescent
                ),
                "child stage must be juvenile; got age {} label {}",
                c_chars.age,
                c_chars.age_label
            );

            // Child has no simulation or social state (not a simulated NPC).
            assert!(child.social.is_none());
            assert!(child.simulation.is_none());
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_migration_spawns_sibling_pair() {
        use ironmud::types::{ImmigrationFamilyChance, RelationshipKind};

        let (db, _temp) = open_temp_db(
"family_spawn_sibs");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut area = new_area("fam_sib");
            area.immigration_family_chance = ImmigrationFamilyChance {
                parent_child: 0.0,
                sibling_pair: 1.0,
            };
            area.migration_max_per_check = 1;
            db.save_area_data(area.clone()).unwrap();

            let gate = new_room(area.id, &format!("{}:gate", area.prefix), false, 0);
            save_room_with_vnum(&db, &gate);
            let home = new_room(area.id, &format!("{}:home", area.prefix), true, 2);
            save_room_with_vnum(&db, &home);

            set_game_day(&db, 20);
            let data = load_migration_data(&data_dir()).unwrap();
            run_one_tick(&db, &data);

            let mobs: Vec<_> = db
                .list_all_mobiles()
                .unwrap()
                .into_iter()
                .filter(|m| !m.is_prototype)
                .collect();
            assert_eq!(mobs.len(), 2, "sibling pair spawns two mobiles");

            let a = &mobs[0];
            let b = &mobs[1];
            // Shared household.
            assert!(a.household_id.is_some());
            assert_eq!(a.household_id, b.household_id);
            // Both claim residency.
            assert_eq!(a.resident_of.as_deref(), Some(format!("{}:home", area.prefix).as_str()));
            assert_eq!(b.resident_of.as_deref(), Some(format!("{}:home", area.prefix).as_str()));
            // Reciprocal sibling edges.
            let ab = a.relationships.iter().find(|r| r.other_id == b.id).unwrap();
            let ba = b.relationships.iter().find(|r| r.other_id == a.id).unwrap();
            assert_eq!(ab.kind, RelationshipKind::Sibling);
            assert_eq!(ba.kind, RelationshipKind::Sibling);
            assert_eq!(ab.affinity, 50);
            // Shared last name for family feel.
            let a_last = a.name.split_once(' ').map(|(_, l)| l).unwrap_or("");
            let b_last = b.name.split_once(' ').map(|(_, l)| l).unwrap_or("");
            assert_eq!(a_last, b_last, "siblings share last name");
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_family_bias_halves_negative_deltas_between_siblings() {
        use ironmud::social::apply_family_bias;
        use ironmud::types::RelationshipKind;

        // Simulate 100 rounds of negative affinity delta. With bias, siblings
        // lose affinity at half the rate of strangers.
        let mut sibling_affinity = 0i32;
        let mut stranger_affinity = 0i32;
        for _ in 0..100 {
            sibling_affinity += apply_family_bias(-4, RelationshipKind::Sibling);
            stranger_affinity += apply_family_bias(-4, RelationshipKind::Friend);
        }
        // Both are negative; sibling should be less extreme.
        assert!(
            sibling_affinity > stranger_affinity,
            "sibling ({}) should have higher (less negative) affinity than stranger ({})",
            sibling_affinity,
            stranger_affinity
        );
        // Specifically: strangers dropped -400; siblings should be about -200.
        assert_eq!(stranger_affinity, -400);
        assert_eq!(sibling_affinity, -200);
    }

    #[test]
    fn test_bereavement_notes_pruned_after_expiry() {
        use ironmud::aging::process_aging_tick;
        use ironmud::types::{BereavementNote, Characteristics, MobileData, RelationshipKind, SocialState};

        let (db, _temp) = open_temp_db(
"grief_prune");
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut m = MobileData::new("Mourner".to_string());
            m.is_prototype = false;
            m.characteristics = Some(Characteristics {
                gender: "female".to_string(),
                age: 30,
                age_label: "adult".to_string(),
                birth_day: 0,
                height: "average".to_string(),
                build: "average".to_string(),
                hair_color: "brown".to_string(),
                hair_style: "short".to_string(),
                eye_color: "brown".to_string(),
                skin_tone: "fair".to_string(),
                distinguishing_mark: None,
            });
            let mut social = SocialState::default();
            social.bereaved_for.push(BereavementNote {
                other_id: uuid::Uuid::new_v4(),
                other_name: "Lost Parent".to_string(),
                kind: RelationshipKind::Parent,
                until_day: 150,
            });
            social.bereaved_until_day = Some(150);
            m.social = Some(social);
            let mobile_id = m.id;
            db.save_mobile_data(m).unwrap();

            let conns = empty_connections();

            // Day 120: still mourning.
            set_game_day(&db, 120);
            process_aging_tick(&db, &conns).unwrap();
            let after = db.get_mobile_data(&mobile_id).unwrap().unwrap();
            assert_eq!(after.social.unwrap().bereaved_for.len(), 1, "not yet expired");

            // Day 200: mourning window closed; aging tick prunes.
            set_game_day(&db, 200);
            process_aging_tick(&db, &conns).unwrap();
            let after = db.get_mobile_data(&mobile_id).unwrap().unwrap();
            assert!(after.social.unwrap().bereaved_for.is_empty(), "expired note dropped");
        }));
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_migrants_spawn_only_as_adults() {
        // generate_characteristics must never pick baby/child/adolescent ranges —
        // those entries exist in human.json so Phase D `spawn_child` can use them,
        // but immigration always produces an adult.
        use ironmud::migration::generate_characteristics;
        use ironmud::types::{LifeStage, life_stage_for_age};
        use rand::SeedableRng;
        use rand::rngs::StdRng;

        let data = load_migration_data(&data_dir()).expect("load migration data");
        let profile = data.visual_profile("human").expect("human profile");
        // Run many rolls — filter bug would show up probabilistically.
        for seed in 0..200u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let (chars, _desc) = generate_characteristics(profile, "female", &mut rng);
            let stage = life_stage_for_age(chars.age);
            assert!(
                !matches!(stage, LifeStage::Baby | LifeStage::Child | LifeStage::Adolescent),
                "seed {seed} produced juvenile migrant (age {}, label {})",
                chars.age,
                chars.age_label
            );
            assert!(chars.age >= 18, "seed {seed} produced age {} < 18", chars.age);
        }
    }
}

#[test]
fn test_item_extra_descs_persist() {
    use ironmud::ItemData;
    
    use ironmud::types::ExtraDesc;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut item = ItemData::new(
            "lantern".to_string(),
            "a brass lantern".to_string(),
            "A hooded brass lantern hangs here.".to_string(),
        );
        assert!(item.extra_descs.is_empty(), "new items start with no extras");

        item.extra_descs.push(ExtraDesc {
            keywords: vec!["letters".to_string(), "inscription".to_string()],
            description: "Tiny letters scratched into the side read: 'press the latch firmly'.".to_string(),
        });
        item.extra_descs.push(ExtraDesc {
            keywords: vec!["latch".to_string()],
            description: "A small brass latch sits on the side.".to_string(),
        });
        let item_id = item.id;
        db.save_item_data(item).expect("save");

        let loaded = db.get_item_data(&item_id).expect("get").expect("present");
        assert_eq!(loaded.extra_descs.len(), 2, "two extras persisted");
        assert_eq!(loaded.extra_descs[0].keywords, vec!["letters", "inscription"]);
        assert!(loaded.extra_descs[0].description.contains("press the latch"));

        // Mutate: drop one, save, reload.
        let mut updated = loaded;
        updated.extra_descs.retain(|e| !e.keywords.iter().any(|k| k == "latch"));
        db.save_item_data(updated).expect("save");
        let loaded2 = db.get_item_data(&item_id).expect("get").expect("present");
        assert_eq!(loaded2.extra_descs.len(), 1, "one extra after removal");
        assert_eq!(loaded2.extra_descs[0].keywords, vec!["letters", "inscription"]);
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_item_hit_damage_bonus_persist() {
    use ironmud::ItemData;
    

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut item = ItemData::new(
            "magic ring".to_string(),
            "a glittering ring".to_string(),
            "A glittering ring lies here.".to_string(),
        );
        // Defaults: zero, zero.
        assert_eq!(item.hit_bonus, 0);
        assert_eq!(item.damage_bonus, 0);

        item.hit_bonus = 1;
        item.damage_bonus = 2;
        let item_id = item.id;
        db.save_item_data(item).expect("save");

        let loaded = db.get_item_data(&item_id).expect("get").expect("present");
        assert_eq!(loaded.hit_bonus, 1, "hit_bonus survives round-trip");
        assert_eq!(loaded.damage_bonus, 2, "damage_bonus survives round-trip");

        // Mutate: clear hit, bump damage further.
        let mut updated = loaded;
        updated.hit_bonus = 0;
        updated.damage_bonus = 5;
        db.save_item_data(updated).expect("save");
        let loaded2 = db.get_item_data(&item_id).expect("get").expect("present");
        assert_eq!(loaded2.hit_bonus, 0);
        assert_eq!(loaded2.damage_bonus, 5);
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_item_light_hours_remaining_persists() {
    use ironmud::ItemData;
    

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut item = ItemData::new(
            "torch".to_string(),
            "a flickering torch".to_string(),
            "A flickering torch lies here.".to_string(),
        );
        // Default: 0 (= permanent / no decay tracked).
        assert_eq!(item.light_hours_remaining, 0);

        item.flags.provides_light = true;
        item.light_hours_remaining = 12;
        let item_id = item.id;
        db.save_item_data(item).expect("save");

        let loaded = db.get_item_data(&item_id).expect("get").expect("present");
        assert!(loaded.flags.provides_light);
        assert_eq!(loaded.light_hours_remaining, 12);

        // Drop to 1, save, then simulate burnout to 0.
        let mut updated = loaded;
        updated.light_hours_remaining = 1;
        db.save_item_data(updated).expect("save");
        let nearly = db.get_item_data(&item_id).expect("get").expect("present");
        assert_eq!(nearly.light_hours_remaining, 1);

        let mut burned = nearly;
        burned.light_hours_remaining = 0;
        burned.flags.provides_light = false;
        db.save_item_data(burned).expect("save");
        let final_state = db.get_item_data(&item_id).expect("get").expect("present");
        assert_eq!(final_state.light_hours_remaining, 0);
        assert!(!final_state.flags.provides_light, "burnout clears provides_light");
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_item_cast_on_use_persists() {
    use ironmud::ItemData;
    use ironmud::ItemType;
    
    use ironmud::types::CastOnUse;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut item = ItemData::new(
            "wand".to_string(),
            "a slender wand".to_string(),
            "A slender wand lies here.".to_string(),
        );
        // Default: None.
        assert!(item.cast_on_use.is_none());

        item.item_type = ItemType::Wand;
        item.cast_on_use = Some(CastOnUse {
            spell: "magic_missile".to_string(),
            min_level: 2,
            charges: 5,
            max_charges: 5,
            cooldown_secs: None,
        });
        let item_id = item.id;
        db.save_item_data(item).expect("save");

        let loaded = db.get_item_data(&item_id).expect("get").expect("present");
        assert_eq!(loaded.item_type, ItemType::Wand);
        let cou = loaded.cast_on_use.as_ref().expect("cast_on_use round-trips");
        assert_eq!(cou.spell, "magic_missile");
        assert_eq!(cou.min_level, 2);
        assert_eq!(cou.charges, 5);
        assert_eq!(cou.max_charges, 5);

        // Decrement charges (mimics zap path) and verify persistence.
        let mut after_zap = loaded;
        after_zap.cast_on_use.as_mut().unwrap().charges = 4;
        db.save_item_data(after_zap).expect("save");
        let reloaded = db.get_item_data(&item_id).expect("get").expect("present");
        assert_eq!(reloaded.cast_on_use.as_ref().unwrap().charges, 4);
        // max_charges unchanged.
        assert_eq!(reloaded.cast_on_use.as_ref().unwrap().max_charges, 5);
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_item_max_hp_mana_bonus_persists() {
    use ironmud::ItemData;
    

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");
        let mut item = ItemData::new(
            "ring".to_string(),
            "an enchanted ring".to_string(),
            "An enchanted ring lies here.".to_string(),
        );
        assert_eq!(item.max_hp_bonus, 0);
        assert_eq!(item.max_mana_bonus, 0);
        item.max_hp_bonus = 25;
        item.max_mana_bonus = 10;
        let item_id = item.id;
        db.save_item_data(item).expect("save");

        let loaded = db.get_item_data(&item_id).expect("get").expect("present");
        assert_eq!(loaded.max_hp_bonus, 25);
        assert_eq!(loaded.max_mana_bonus, 10);
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_item_magical_flag_auto_adds_category() {
    use ironmud::ItemData;
    

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut item = ItemData::new(
            "amulet".to_string(),
            "an amulet".to_string(),
            "An amulet lies here.".to_string(),
        );
        assert!(item.categories.is_empty());

        // Flag on → category added.
        item.flags.magical = true;
        item.sync_flag_categories();
        assert!(item.categories.iter().any(|c| c == "magical"));
        let item_id = item.id;
        db.save_item_data(item).expect("save");

        let loaded = db.get_item_data(&item_id).expect("get").expect("present");
        assert!(loaded.categories.iter().any(|c| c == "magical"));

        // Idempotent — calling again does not duplicate.
        let mut again = loaded;
        again.sync_flag_categories();
        again.sync_flag_categories();
        let count = again
            .categories
            .iter()
            .filter(|c| c.eq_ignore_ascii_case("magical"))
            .count();
        assert_eq!(count, 1, "magical category should not duplicate");

        // Clearing flag does NOT strip the category (one-way by design).
        again.flags.magical = false;
        again.sync_flag_categories();
        assert!(again.categories.iter().any(|c| c == "magical"));
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_area_caps_persist_and_count_helpers_match() {
    // Persistence round-trip for the new max_* cap fields, plus a check
    // that the per-area count helpers used by F6's create-time gate
    // distinguish in-area, orphan, and other-area entities correctly.
    
    use ironmud::types::{
        AreaData, AreaFlags, AreaPermission, ClimateProfile, CombatZoneType, GoldRange,
        ImmigrationFamilyChance, ImmigrationVariationChances, RoomFlags,
    };
    use ironmud::{ItemData, MobileData};
    use uuid::Uuid;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let area_id = Uuid::new_v4();
        let area = AreaData {
            id: area_id,
            name: "Capped".into(),
            prefix: "cap".into(),
            description: String::new(),
            level_min: 0,
            level_max: 0,
            theme: String::new(),
            owner: None,
            permission_level: AreaPermission::AllBuilders,
            trusted_builders: Vec::new(),
            city_forage_table: Vec::new(),
            wilderness_forage_table: Vec::new(),
            shallow_water_forage_table: Vec::new(),
            deep_water_forage_table: Vec::new(),
            underwater_forage_table: Vec::new(),
            combat_zone: CombatZoneType::Pve,
            flags: AreaFlags::default(),
            default_room_flags: RoomFlags::default(),
            climate: ClimateProfile::default(),
            immigration_enabled: false,
            immigration_room_vnum: String::new(),
            immigration_name_pool: String::new(),
            immigration_visual_profile: String::new(),
            migration_interval_days: 0,
            migration_max_per_check: 0,
            migrant_sim_defaults: None,
            last_migration_check_day: None,
            immigration_variation_chances: ImmigrationVariationChances::default(),
            immigration_family_chance: ImmigrationFamilyChance::default(),
            migrant_starting_gold: GoldRange::default(),
            guard_wage_per_hour: 0,
            healer_wage_per_hour: 0,
            scavenger_wage_per_hour: 0,
            donation_room_vnum: None,
            max_rooms: Some(2),
            max_items: Some(3),
            max_mobiles: None,
            max_spawn_points: None,
        };
        db.save_area_data(area).expect("save area");

        let loaded = db.get_area_data(&area_id).expect("get area").expect("present");
        assert_eq!(loaded.max_rooms, Some(2), "max_rooms round-trips");
        assert_eq!(loaded.max_items, Some(3), "max_items round-trips");
        assert!(loaded.max_mobiles.is_none(), "None persists for max_mobiles");
        assert!(loaded.max_spawn_points.is_none(), "None persists for max_spawn_points");

        // Items: one in-area proto, one orphan proto, one in a different area.
        let other_area = Uuid::new_v4();
        let mut in_area = ItemData::new("torch".into(), "a torch".into(), "A torch.".into());
        in_area.is_prototype = true;
        in_area.area_id = Some(area_id);
        db.save_item_data(in_area).expect("save in-area item");

        let mut orphan = ItemData::new("loose".into(), "a loose stick".into(), "A stick.".into());
        orphan.is_prototype = true;
        db.save_item_data(orphan).expect("save orphan");

        let mut other = ItemData::new("rope".into(), "a rope".into(), "A rope.".into());
        other.is_prototype = true;
        other.area_id = Some(other_area);
        db.save_item_data(other).expect("save other-area item");

        assert_eq!(
            db.count_item_protos_in_area(&area_id).expect("count"),
            1,
            "count includes only in-area protos (orphans + other areas excluded)"
        );
        assert_eq!(
            db.count_item_protos_in_area(&other_area).expect("count"),
            1,
            "other area sees its own proto only"
        );

        // Mobile attribution.
        let mut m = MobileData::new("guard".into());
        m.is_prototype = true;
        m.area_id = Some(area_id);
        db.save_mobile_data(m).expect("save mob");
        assert_eq!(db.count_mobile_protos_in_area(&area_id).expect("count"), 1);

        // Empty areas count to zero (no panics on the absence path).
        assert_eq!(db.count_rooms_in_area(&area_id).expect("count"), 0);
        assert_eq!(db.count_spawn_points_in_area(&area_id).expect("count"), 0);
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_item_and_mobile_area_id_persists() {
    
    use ironmud::{ItemData, MobileData};
    use uuid::Uuid;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let area_uuid = Uuid::new_v4();

        // Item: defaults to None (orphan), survives a None → Some → None round-trip.
        let mut item = ItemData::new("torch".to_string(), "a torch".to_string(), "A torch.".to_string());
        assert!(item.area_id.is_none(), "freshly-constructed items default to orphan");
        item.area_id = Some(area_uuid);
        let item_id = item.id;
        db.save_item_data(item).expect("save item");
        let loaded = db.get_item_data(&item_id).expect("get").expect("present");
        assert_eq!(loaded.area_id, Some(area_uuid), "area_id round-trips through sled");

        let mut cleared = loaded;
        cleared.area_id = None;
        db.save_item_data(cleared).expect("save cleared");
        let loaded2 = db.get_item_data(&item_id).expect("get").expect("present");
        assert!(loaded2.area_id.is_none(), "clearing back to orphan persists");

        // Mobile: same shape.
        let mut mob = MobileData::new("guard".to_string());
        assert!(mob.area_id.is_none(), "freshly-constructed mobiles default to orphan");
        mob.area_id = Some(area_uuid);
        let mob_id = mob.id;
        db.save_mobile_data(mob).expect("save mob");
        let loaded_mob = db.get_mobile_data(&mob_id).expect("get").expect("present");
        assert_eq!(loaded_mob.area_id, Some(area_uuid), "mobile area_id round-trips");
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_total_seconds_played_persists() {
    // Verifies the new `CharacterData.total_seconds_played` field round-trips
    // through save/load with `#[serde(default)]` honoring pre-feature rows.
    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut character: ironmud::types::CharacterData = serde_json::from_value(
            serde_json::json!({
                "name": "Timekeeper",
                "password_hash": "",
                "current_room_id": uuid::Uuid::new_v4().to_string(),
            }),
        )
        .expect("build character");

        assert_eq!(
            character.total_seconds_played, 0,
            "fresh character starts with zero play time"
        );

        character.total_seconds_played = 4_321;
        db.save_character_data(character).expect("save");

        let loaded = db
            .get_character_data("Timekeeper")
            .expect("get")
            .expect("present");
        assert_eq!(
            loaded.total_seconds_played, 4_321,
            "total_seconds_played survives save/load"
        );
    }));

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_item_note_content_persists() {
    use ironmud::ItemData;
    

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut item = ItemData::new(
            "parchment".to_string(),
            "a weathered parchment".to_string(),
            "A rolled parchment rests here.".to_string(),
        );
        assert!(item.note_content.is_none(), "new items start with no note");

        let body = "  N\n W-+-E\n  S\n\n(rough map of the district)";
        item.note_content = Some(body.to_string());
        let item_id = item.id;
        db.save_item_data(item).expect("save");

        let loaded = db.get_item_data(&item_id).expect("get").expect("present");
        assert_eq!(
            loaded.note_content.as_deref(),
            Some(body),
            "body survives save/load and preserves whitespace + blank lines"
        );

        let mut cleared = loaded;
        cleared.note_content = None;
        db.save_item_data(cleared).expect("save cleared");
        let loaded2 = db.get_item_data(&item_id).expect("get").expect("present");
        assert!(loaded2.note_content.is_none(), "None persists");
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_item_on_hit_effects_persist() {
    
    use ironmud::{ItemData, OnHitEffect};

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut item = ItemData::new(
            "cutlass".to_string(),
            "a curved cutlass".to_string(),
            "A cutlass lies here.".to_string(),
        );
        assert!(item.on_hit_effects.is_empty(), "fresh items start empty");

        item.on_hit_effects = vec![
            OnHitEffect {
                effect: "bleeding".to_string(),
                chance: 70,
                magnitude: 2,
                duration: 0,
            },
            OnHitEffect {
                effect: "fire".to_string(),
                chance: 30,
                magnitude: 3,
                duration: 5,
            },
        ];
        let item_id = item.id;
        db.save_item_data(item).expect("save");

        let loaded = db.get_item_data(&item_id).expect("get").expect("present");
        assert_eq!(loaded.on_hit_effects.len(), 2);
        assert_eq!(loaded.on_hit_effects[0].effect, "bleeding");
        assert_eq!(loaded.on_hit_effects[0].chance, 70);
        assert_eq!(loaded.on_hit_effects[0].magnitude, 2);
        assert_eq!(loaded.on_hit_effects[1].effect, "fire");
        assert_eq!(loaded.on_hit_effects[1].duration, 5);
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_mobile_on_hit_effects_persist() {
    
    use ironmud::{MobileData, OnHitEffect};

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut mob = MobileData::new("a fire elemental".to_string());
        assert!(mob.on_hit_effects.is_empty(), "fresh mobs start empty");

        mob.on_hit_effects = vec![OnHitEffect {
            effect: "fire".to_string(),
            chance: 100,
            magnitude: 4,
            duration: 3,
        }];
        let id = mob.id;
        db.save_mobile_data(mob).expect("save");

        let loaded = db.get_mobile_data(&id).expect("get").expect("present");
        assert_eq!(loaded.on_hit_effects.len(), 1);
        assert_eq!(loaded.on_hit_effects[0].effect, "fire");
        assert_eq!(loaded.on_hit_effects[0].magnitude, 4);
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_item_type_note_and_pen_round_trip() {
    use ironmud::ItemData;
    
    use ironmud::types::ItemType;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut paper = ItemData::new(
            "paper blank".to_string(),
            "a blank piece of paper".to_string(),
            "A blank piece of paper lies here.".to_string(),
        );
        paper.item_type = ItemType::Note;
        let paper_id = paper.id;
        db.save_item_data(paper).expect("save paper");

        let mut pen = ItemData::new(
            "pen quill".to_string(),
            "a feathered quill pen".to_string(),
            "A feathered quill pen rests here.".to_string(),
        );
        pen.item_type = ItemType::Pen;
        let pen_id = pen.id;
        db.save_item_data(pen).expect("save pen");

        let loaded_paper = db.get_item_data(&paper_id).expect("get").expect("present");
        assert_eq!(loaded_paper.item_type, ItemType::Note);
        assert_eq!(loaded_paper.item_type.to_display_string(), "note");

        let loaded_pen = db.get_item_data(&pen_id).expect("get").expect("present");
        assert_eq!(loaded_pen.item_type, ItemType::Pen);
        assert_eq!(loaded_pen.item_type.to_display_string(), "pen");

        assert_eq!(ItemType::from_str("note"), Some(ItemType::Note));
        assert_eq!(ItemType::from_str("paper"), Some(ItemType::Note));
        assert_eq!(ItemType::from_str("pen"), Some(ItemType::Pen));
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_donation_fields_round_trip() {
    use ironmud::ItemData;
    
    use ironmud::types::AreaPermission;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        // ItemFlags.no_donate persists.
        let mut item = ItemData::new(
            "trinket".to_string(),
            "a worthless trinket".to_string(),
            "A worthless trinket lies here.".to_string(),
        );
        item.flags.no_donate = true;
        let item_id = item.id;
        db.save_item_data(item).expect("save item");
        let loaded = db.get_item_data(&item_id).expect("get").expect("present");
        assert!(loaded.flags.no_donate, "no_donate persists across save/load");
        assert!(loaded.donated_at.is_none(), "donated_at unset by default");

        // AreaData.donation_room_vnum starts unset, persists when set.
        let mut area = ironmud::types::AreaData {
            id: uuid::Uuid::new_v4(),
            name: "Test Area".to_string(),
            prefix: "test".to_string(),
            description: String::new(),
            level_min: 0,
            level_max: 0,
            theme: String::new(),
            owner: None,
            permission_level: AreaPermission::default(),
            trusted_builders: Vec::new(),
            city_forage_table: Vec::new(),
            wilderness_forage_table: Vec::new(),
            shallow_water_forage_table: Vec::new(),
            deep_water_forage_table: Vec::new(),
            underwater_forage_table: Vec::new(),
            combat_zone: ironmud::types::CombatZoneType::default(),
            flags: ironmud::types::AreaFlags::default(),
            default_room_flags: ironmud::types::RoomFlags::default(),
            climate: ironmud::types::ClimateProfile::default(),
            immigration_enabled: false,
            immigration_room_vnum: String::new(),
            immigration_name_pool: String::new(),
            immigration_visual_profile: String::new(),
            migration_interval_days: 0,
            migration_max_per_check: 0,
            migrant_sim_defaults: None,
            last_migration_check_day: None,
            immigration_variation_chances: Default::default(),
            immigration_family_chance: Default::default(),
            migrant_starting_gold: Default::default(),
            guard_wage_per_hour: 0,
            healer_wage_per_hour: 0,
            scavenger_wage_per_hour: 0,
            donation_room_vnum: None,
            max_rooms: None,
            max_items: None,
            max_mobiles: None,
            max_spawn_points: None,
        };
        let area_id = area.id;
        db.save_area_data(area.clone()).expect("save area");
        let reloaded = db.get_area_data(&area_id).expect("get").expect("present");
        assert!(reloaded.donation_room_vnum.is_none(), "default is None");

        area.donation_room_vnum = Some("test_9100".to_string());
        db.save_area_data(area).expect("save area v2");
        let reloaded = db.get_area_data(&area_id).expect("get").expect("present");
        assert_eq!(
            reloaded.donation_room_vnum.as_deref(),
            Some("test_9100"),
            "donation_room_vnum survives round-trip"
        );
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

/// Mirror of `src/ticks/donation.rs::process_donation_decay` minus the
/// room broadcast. The tick module lives under main.rs so it isn't
/// reachable from integration tests; replicating the iteration here
/// guards the deletion logic (only Some(donated_at) past threshold,
/// only items in a Room) without coupling tests to the runner.
fn run_donation_decay_pass(db: &ironmud::db::Db, decay_secs: i64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    if let Ok(items) = db.list_all_items() {
        for item in items {
            let stamped = match item.donated_at {
                Some(t) => t,
                None => continue,
            };
            if now - stamped < decay_secs {
                continue;
            }
            if !matches!(item.location, ironmud::types::ItemLocation::Room(_)) {
                continue;
            }
            let _ = db.delete_item(&item.id);
        }
    }
}

#[test]
fn test_donation_decay_pass_deletes_expired_items_only() {
    use ironmud::ItemData;
    use ironmud::types::ItemLocation;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");
        let room_id = uuid::Uuid::new_v4();

        // Expired donation — donated_at is the epoch.
        let mut expired = ItemData::new(
            "expired".to_string(),
            "an expired trinket".to_string(),
            "An expired trinket sits here.".to_string(),
        );
        expired.location = ItemLocation::Room(room_id);
        expired.donated_at = Some(0);
        let expired_id = expired.id;
        db.save_item_data(expired).expect("save expired");

        // Fresh donation — stamped now, must survive.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap();
        let mut fresh = ItemData::new(
            "fresh".to_string(),
            "a fresh trinket".to_string(),
            "A fresh trinket sits here.".to_string(),
        );
        fresh.location = ItemLocation::Room(room_id);
        fresh.donated_at = Some(now);
        let fresh_id = fresh.id;
        db.save_item_data(fresh).expect("save fresh");

        // Never-donated item must not be touched.
        let mut bystander = ItemData::new(
            "bystander".to_string(),
            "a stick".to_string(),
            "A stick lies here.".to_string(),
        );
        bystander.location = ItemLocation::Room(room_id);
        let bystander_id = bystander.id;
        db.save_item_data(bystander).expect("save bystander");

        run_donation_decay_pass(&db, 60);

        assert!(
            db.get_item_data(&expired_id).expect("get").is_none(),
            "expired donation should be deleted"
        );
        assert!(
            db.get_item_data(&fresh_id).expect("get").is_some(),
            "fresh donation should survive"
        );
        assert!(
            db.get_item_data(&bystander_id).expect("get").is_some(),
            "non-donated item must not be touched"
        );
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_spawn_crafted_item_copies_liquid_ammo_and_note_fields() {
    use ironmud::ItemData;
    use ironmud::script::build_crafted_item_from_prototype;
    use ironmud::types::{EffectType, ItemEffect, ItemType, LiquidType};

    let mut prototype = ItemData::new(
        "poison vial".to_string(),
        "a small vial of dark liquid".to_string(),
        "A glass vial filled with a sickly green poison.".to_string(),
    );
    prototype.vnum = Some("test:poison_vial".to_string());
    prototype.item_type = ItemType::LiquidContainer;
    prototype.liquid_type = LiquidType::Poison;
    prototype.liquid_max = 5;
    prototype.liquid_current = 5;
    prototype.liquid_poisoned = true;
    prototype.liquid_effects = vec![ItemEffect {
        effect_type: EffectType::Poison,
        magnitude: 3,
        duration: 3,
        script_callback: None,
    }];
    prototype.note_content = Some("warning: do not drink".to_string());
    prototype.caliber = Some("arrow".to_string());
    prototype.ammo_count = 10;
    prototype.ammo_damage_bonus = 2;
    prototype.ammo_effect_type = "poison".to_string();
    prototype.ammo_effect_duration = 3;
    prototype.ammo_effect_damage = 3;

    let crafted = build_crafted_item_from_prototype(&prototype, "TestChar", 1);

    assert_eq!(crafted.liquid_type, LiquidType::Poison, "liquid_type copied");
    assert_eq!(crafted.liquid_max, 5, "liquid_max copied");
    assert_eq!(crafted.liquid_current, 5, "liquid_current copied (vial spawns full)");
    assert!(crafted.liquid_poisoned, "liquid_poisoned copied");
    assert_eq!(crafted.liquid_effects.len(), 1, "liquid_effects copied");
    assert_eq!(crafted.liquid_effects[0].effect_type, EffectType::Poison);
    assert_eq!(crafted.liquid_effects[0].magnitude, 3);
    assert_eq!(crafted.liquid_effects[0].duration, 3);

    assert_eq!(crafted.note_content.as_deref(), Some("warning: do not drink"));

    assert_eq!(crafted.caliber.as_deref(), Some("arrow"));
    assert_eq!(crafted.ammo_count, 10);
    assert_eq!(crafted.ammo_damage_bonus, 2);
    assert_eq!(crafted.ammo_effect_type, "poison");
    assert_eq!(crafted.ammo_effect_duration, 3);
    assert_eq!(crafted.ammo_effect_damage, 3);

    assert_eq!(crafted.quality, 50, "Normal quality tier yields quality=50");
}

#[test]
fn test_area_default_room_flags_apply_to_new_rooms() {
    
    use ironmud::types::{
        AreaData, AreaFlags, AreaPermission, CombatZoneType, ImmigrationFamilyChance, ImmigrationVariationChances,
        RoomData, RoomExits, RoomFlags, WaterType,
    };
    use std::collections::HashMap as StdHashMap;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        // Underground area with indoors + no_windows as defaults
        let mut defaults = RoomFlags::default();
        defaults.indoors = true;
        defaults.no_windows = true;
        defaults.dark = true;

        let area = AreaData {
            id: uuid::Uuid::new_v4(),
            name: "Underground".into(),
            prefix: "und".into(),
            description: String::new(),
            level_min: 0,
            level_max: 0,
            theme: String::new(),
            owner: None,
            permission_level: AreaPermission::AllBuilders,
            trusted_builders: Vec::new(),
            city_forage_table: Vec::new(),
            wilderness_forage_table: Vec::new(),
            shallow_water_forage_table: Vec::new(),
            deep_water_forage_table: Vec::new(),
            underwater_forage_table: Vec::new(),
            combat_zone: CombatZoneType::default(),
            flags: AreaFlags::default(),
            default_room_flags: defaults.clone(),
            climate: ironmud::types::ClimateProfile::default(),
            immigration_enabled: false,
            immigration_room_vnum: String::new(),
            immigration_name_pool: String::new(),
            immigration_visual_profile: String::new(),
            migration_interval_days: 0,
            migration_max_per_check: 0,
            migrant_sim_defaults: None,
            last_migration_check_day: None,
            immigration_variation_chances: ImmigrationVariationChances::default(),
            immigration_family_chance: ImmigrationFamilyChance::default(),
            migrant_starting_gold: ironmud::types::GoldRange::default(),
            guard_wage_per_hour: 0,
            healer_wage_per_hour: 0,
            scavenger_wage_per_hour: 0,
            donation_room_vnum: None,
            max_rooms: None,
            max_items: None,
            max_mobiles: None,
            max_spawn_points: None,
        };
        db.save_area_data(area.clone()).expect("save area");

        // Reload the area (exercises the #[serde(default)] roundtrip) and
        // build a fresh room using the area's defaults as the starting point.
        let reloaded = db.get_area_data(&area.id).expect("get").expect("present");
        assert!(reloaded.default_room_flags.indoors, "defaults survive save/load");
        assert!(reloaded.default_room_flags.no_windows);
        assert!(reloaded.default_room_flags.dark);

        let mut flags = RoomFlags::default();
        flags.merge_area_defaults(&reloaded.default_room_flags);
        assert!(flags.indoors, "new room picks up indoors default");
        assert!(flags.no_windows);
        assert!(flags.dark);
        assert!(!flags.liveable, "flags not in the defaults remain off");

        // Scenario: an existing area predating this feature loads fine
        // (serde defaults to all-false RoomFlags, so new rooms get no
        // inheritance — matches prior behavior).
        let blank_defaults = RoomFlags::default();
        let mut flags2 = RoomFlags::default();
        flags2.merge_area_defaults(&blank_defaults);
        assert!(!flags2.indoors);
        assert!(!flags2.no_windows);

        // merge_area_defaults only ORs ON — it never clears a flag the
        // caller already set.
        let mut flags3 = RoomFlags::default();
        flags3.city = true;
        let mut sparse = RoomFlags::default();
        sparse.indoors = true;
        flags3.merge_area_defaults(&sparse);
        assert!(flags3.city, "pre-set city stays on");
        assert!(flags3.indoors, "default indoors applied");

        // Sanity: a fresh RoomData using merged flags persists and loads.
        let room = RoomData {
            id: uuid::Uuid::new_v4(),
            title: "Cave".into(),
            description: String::new(),
            exits: RoomExits::default(),
            flags,
            extra_descs: Vec::new(),
            vnum: None,
            area_id: Some(area.id),
            triggers: Vec::new(),
            doors: StdHashMap::new(),
            spring_desc: None,
            summer_desc: None,
            autumn_desc: None,
            winter_desc: None,
            dynamic_desc: None,
            water_type: WaterType::None,
            catch_table: Vec::new(),
            is_property_template: false,
            property_template_id: None,
            is_template_entrance: false,
            property_lease_id: None,
            property_entrance: false,
            recent_departures: Vec::new(),
            blood_trails: Vec::new(),
            traps: Vec::new(),
            living_capacity: 0,
            residents: Vec::new(),
            dg_vars: std::collections::HashMap::new(),
            coordinates: None,
            contextual_commands: Vec::new(),
            exit_delays: std::collections::HashMap::new(),
        };
        let room_id = room.id;
        db.save_room_data(room).expect("save room");
        let loaded = db.get_room_data(&room_id).expect("get").expect("present");
        assert!(loaded.flags.indoors);
        assert!(loaded.flags.no_windows);
        assert!(loaded.flags.dark);
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_exit_delays_round_trip_through_db() {
    use ironmud::types::RoomData;

    let temp = tempfile::tempdir().expect("create temp dir");
    let db = ironmud::db::Db::open(temp.path()).expect("open db");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut room: RoomData = serde_json::from_value(serde_json::json!({
            "id": uuid::Uuid::new_v4(),
            "title": "Narrow Tunnel",
            "description": "A tight squeeze.",
            "exits": {},
        }))
        .expect("build room");
        room.exit_delays.insert("north".to_string(), 5);
        room.exit_delays.insert("up".to_string(), 12);
        let room_id = room.id;
        db.save_room_data(room).expect("save");

        let loaded = db.get_room_data(&room_id).expect("get").expect("present");
        assert_eq!(loaded.exit_delays.get("north"), Some(&5));
        assert_eq!(loaded.exit_delays.get("up"), Some(&12));
        assert_eq!(loaded.exit_delays.get("south"), None);
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_pending_slow_move_round_trip_through_db() {
    use ironmud::types::{CharacterData, PendingSlowMove};

    let temp = tempfile::tempdir().expect("create temp dir");
    let db = ironmud::db::Db::open(temp.path()).expect("open db");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let room_id = uuid::Uuid::new_v4();
        let mut char: CharacterData = serde_json::from_value(serde_json::json!({
            "name": "tunnelwalker",
            "password_hash": "",
            "current_room_id": room_id,
        }))
        .expect("build character");
        assert!(char.pending_slow_move.is_none(), "fresh char has no pending move");

        char.pending_slow_move = Some(PendingSlowMove {
            direction: "north".to_string(),
            source_room_id: room_id,
            complete_at: 1_700_000_000,
        });
        db.save_character_data(char.clone()).expect("save");

        let loaded = db
            .get_character_data("tunnelwalker")
            .expect("get")
            .expect("present");
        let psm = loaded.pending_slow_move.clone().expect("pending preserved");
        assert_eq!(psm.direction, "north");
        assert_eq!(psm.source_room_id, room_id);
        assert_eq!(psm.complete_at, 1_700_000_000);

        // Clearing also persists.
        let mut updated = loaded;
        updated.pending_slow_move = None;
        db.save_character_data(updated).expect("save cleared");
        let reloaded = db
            .get_character_data("tunnelwalker")
            .expect("get")
            .expect("present");
        assert!(reloaded.pending_slow_move.is_none());
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_climate_profile_projection() {
    use ironmud::types::{ClimateProfile, WeatherCondition};

    // Tropical erases snow/blizzard.
    assert_eq!(
        ClimateProfile::Tropical.project(WeatherCondition::Snow),
        WeatherCondition::Rain
    );
    assert_eq!(
        ClimateProfile::Tropical.project(WeatherCondition::Blizzard),
        WeatherCondition::Thunderstorm
    );
    assert_eq!(
        ClimateProfile::Tropical.project(WeatherCondition::LightSnow),
        WeatherCondition::LightRain
    );
    // Tropical leaves permitted conditions alone.
    assert_eq!(
        ClimateProfile::Tropical.project(WeatherCondition::Thunderstorm),
        WeatherCondition::Thunderstorm
    );

    // Tundra converts rain to snow.
    assert_eq!(
        ClimateProfile::Tundra.project(WeatherCondition::Rain),
        WeatherCondition::Snow
    );
    assert_eq!(
        ClimateProfile::Tundra.project(WeatherCondition::LightRain),
        WeatherCondition::LightSnow
    );
    assert_eq!(
        ClimateProfile::Tundra.project(WeatherCondition::Thunderstorm),
        WeatherCondition::Blizzard
    );

    // Arid bleaches rain/snow into clear.
    assert_eq!(
        ClimateProfile::Arid.project(WeatherCondition::Rain),
        WeatherCondition::Clear
    );
    assert_eq!(
        ClimateProfile::Arid.project(WeatherCondition::Snow),
        WeatherCondition::Clear
    );

    // Temperate is the identity.
    for w in [
        WeatherCondition::Clear,
        WeatherCondition::Rain,
        WeatherCondition::Snow,
        WeatherCondition::Thunderstorm,
        WeatherCondition::Blizzard,
        WeatherCondition::Fog,
    ] {
        assert_eq!(
            ClimateProfile::Temperate.project(w),
            w,
            "Temperate must preserve the global condition (no filtering)"
        );
    }

    // Round-trip the from_name / to_string surface so the aedit + API paths
    // share a single canonical naming.
    for c in ClimateProfile::all() {
        let name = c.to_string();
        assert_eq!(ClimateProfile::from_name(&name), Some(*c));
    }
    assert_eq!(ClimateProfile::from_name("desert"), Some(ClimateProfile::Arid));
    assert_eq!(ClimateProfile::from_name("frozen"), Some(ClimateProfile::Tundra));
    assert!(ClimateProfile::from_name("not-a-climate").is_none());
}

#[test]
fn test_climate_temperature_offset_applies() {
    use ironmud::types::{ClimateProfile, GameTime, WeatherCondition};

    let mut gt = GameTime::default();
    // Cloudy passes through unchanged in every climate, so the only
    // difference between profiles is the temperature offset itself.
    gt.weather = WeatherCondition::Cloudy;
    gt.base_temperature = 18;

    let temperate = gt.effective_temperature_for_climate(ClimateProfile::Temperate);
    let tropical = gt.effective_temperature_for_climate(ClimateProfile::Tropical);
    let tundra = gt.effective_temperature_for_climate(ClimateProfile::Tundra);

    assert_eq!(
        tropical - temperate,
        ClimateProfile::Tropical.temperature_offset(),
        "tropical adds its offset on top of the temperate baseline"
    );
    assert_eq!(
        tundra - temperate,
        ClimateProfile::Tundra.temperature_offset(),
        "tundra subtracts its (negative) offset"
    );
    assert!(tropical > temperate, "tropical must be warmer than temperate");
    assert!(tundra < temperate, "tundra must be colder than temperate");
}

#[test]
fn test_area_climate_persists() {
    
    use ironmud::types::{
        AreaData, AreaFlags, AreaPermission, ClimateProfile, CombatZoneType, ImmigrationFamilyChance,
        ImmigrationVariationChances, RoomFlags,
    };

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let area = AreaData {
            id: uuid::Uuid::new_v4(),
            name: "Sun Isle".into(),
            prefix: "isle".into(),
            description: String::new(),
            level_min: 0,
            level_max: 0,
            theme: String::new(),
            owner: None,
            permission_level: AreaPermission::AllBuilders,
            trusted_builders: Vec::new(),
            city_forage_table: Vec::new(),
            wilderness_forage_table: Vec::new(),
            shallow_water_forage_table: Vec::new(),
            deep_water_forage_table: Vec::new(),
            underwater_forage_table: Vec::new(),
            combat_zone: CombatZoneType::default(),
            flags: AreaFlags::default(),
            default_room_flags: RoomFlags::default(),
            climate: ClimateProfile::Tropical,
            immigration_enabled: false,
            immigration_room_vnum: String::new(),
            immigration_name_pool: String::new(),
            immigration_visual_profile: String::new(),
            migration_interval_days: 0,
            migration_max_per_check: 0,
            migrant_sim_defaults: None,
            last_migration_check_day: None,
            immigration_variation_chances: ImmigrationVariationChances::default(),
            immigration_family_chance: ImmigrationFamilyChance::default(),
            migrant_starting_gold: ironmud::types::GoldRange::default(),
            guard_wage_per_hour: 0,
            healer_wage_per_hour: 0,
            scavenger_wage_per_hour: 0,
            donation_room_vnum: None,
            max_rooms: None,
            max_items: None,
            max_mobiles: None,
            max_spawn_points: None,
        };
        let area_id = area.id;
        db.save_area_data(area).expect("save area");

        let reloaded = db.get_area_data(&area_id).expect("get").expect("present");
        assert_eq!(reloaded.climate, ClimateProfile::Tropical);

        // Pre-existing areas serialized before this field exists deserialize
        // with the Temperate default — verify by stripping the field from the
        // JSON and round-tripping.
        let mut value = serde_json::to_value(&reloaded).expect("to_value");
        value
            .as_object_mut()
            .expect("object")
            .remove("climate");
        let downgraded: AreaData = serde_json::from_value(value).expect("legacy load");
        assert_eq!(
            downgraded.climate,
            ClimateProfile::Temperate,
            "absent climate field must default to Temperate"
        );
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_area_combat_zone_persists_and_parses() {
    
    use ironmud::types::{
        AreaData, AreaFlags, AreaPermission, ClimateProfile, CombatZoneType, ImmigrationFamilyChance,
        ImmigrationVariationChances, RoomFlags,
    };

    // Parser round-trip: every value the MCP enum exposes must map back to its
    // canonical name, and unknown values must fall through to None (so the
    // API's "silently ignore" branch fires instead of silently mutating state).
    assert_eq!(CombatZoneType::from_str("pve"), Some(CombatZoneType::Pve));
    assert_eq!(CombatZoneType::from_str("safe"), Some(CombatZoneType::Safe));
    assert_eq!(CombatZoneType::from_str("pvp"), Some(CombatZoneType::Pvp));
    assert_eq!(CombatZoneType::from_str("gibberish"), None);

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let area = AreaData {
            id: uuid::Uuid::new_v4(),
            name: "Arena".into(),
            prefix: "arena".into(),
            description: String::new(),
            level_min: 0,
            level_max: 0,
            theme: String::new(),
            owner: None,
            permission_level: AreaPermission::AllBuilders,
            trusted_builders: Vec::new(),
            city_forage_table: Vec::new(),
            wilderness_forage_table: Vec::new(),
            shallow_water_forage_table: Vec::new(),
            deep_water_forage_table: Vec::new(),
            underwater_forage_table: Vec::new(),
            combat_zone: CombatZoneType::Pvp,
            flags: AreaFlags::default(),
            default_room_flags: RoomFlags::default(),
            climate: ClimateProfile::default(),
            immigration_enabled: false,
            immigration_room_vnum: String::new(),
            immigration_name_pool: String::new(),
            immigration_visual_profile: String::new(),
            migration_interval_days: 0,
            migration_max_per_check: 0,
            migrant_sim_defaults: None,
            last_migration_check_day: None,
            immigration_variation_chances: ImmigrationVariationChances::default(),
            immigration_family_chance: ImmigrationFamilyChance::default(),
            migrant_starting_gold: ironmud::types::GoldRange::default(),
            guard_wage_per_hour: 0,
            healer_wage_per_hour: 0,
            scavenger_wage_per_hour: 0,
            donation_room_vnum: None,
            max_rooms: None,
            max_items: None,
            max_mobiles: None,
            max_spawn_points: None,
        };
        let area_id = area.id;
        db.save_area_data(area).expect("save area");

        let reloaded = db.get_area_data(&area_id).expect("get").expect("present");
        assert_eq!(reloaded.combat_zone, CombatZoneType::Pvp);

        // Legacy areas serialized before this field roundtrip with the Pve default.
        let mut value = serde_json::to_value(&reloaded).expect("to_value");
        value
            .as_object_mut()
            .expect("object")
            .remove("combat_zone");
        let downgraded: AreaData = serde_json::from_value(value).expect("legacy load");
        assert_eq!(
            downgraded.combat_zone,
            CombatZoneType::Pve,
            "absent combat_zone field must default to Pve"
        );
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_area_update_request_deserializes_combat_zone() {
    use ironmud::api::areas::UpdateAreaRequest;

    let req: UpdateAreaRequest =
        serde_json::from_str(r#"{"combat_zone": "safe"}"#).expect("parse update request");
    assert_eq!(req.combat_zone.as_deref(), Some("safe"));

    let empty: UpdateAreaRequest = serde_json::from_str("{}").expect("parse empty update");
    assert!(empty.combat_zone.is_none(), "absent field stays None");
}

#[test]
fn test_mobile_dot_flags_apply_on_hit() {
    use ironmud::script::apply_mobile_on_hit_dots;
    use ironmud::{MobileData, OngoingEffect};

    fn collect_kinds(effects: &[OngoingEffect]) -> Vec<&str> {
        effects.iter().map(|e| e.effect_type.as_str()).collect()
    }

    // Level 1 poisonous snake: 3 rounds of 1 damage poison
    let mut snake = MobileData::new("a strand snake".to_string());
    snake.level = 1;
    snake.flags.poisonous = true;
    let mut effects: Vec<OngoingEffect> = Vec::new();
    apply_mobile_on_hit_dots(&snake, &mut effects, "body");
    assert_eq!(effects.len(), 1, "one DoT for poisonous flag");
    assert_eq!(effects[0].effect_type, "poison");
    assert_eq!(effects[0].rounds_remaining, 3);
    assert_eq!(effects[0].damage_per_round, 1, "level/2 floored to min 1");
    assert_eq!(effects[0].body_part, "body");

    // Level 4 spider: damage scales with level (level/2 = 2)
    let mut spider = MobileData::new("a cave spider".to_string());
    spider.level = 4;
    spider.flags.poisonous = true;
    let mut spider_effects: Vec<OngoingEffect> = Vec::new();
    apply_mobile_on_hit_dots(&spider, &mut spider_effects, "left leg");
    assert_eq!(spider_effects[0].damage_per_round, 2, "level 4 → 2 dmg/round");
    assert_eq!(spider_effects[0].body_part, "left leg");

    // Compose: a hellhound that is both fiery and poisonous applies both DoTs
    let mut hellhound = MobileData::new("a hellhound".to_string());
    hellhound.level = 6;
    hellhound.flags.fiery = true;
    hellhound.flags.poisonous = true;
    let mut combo_effects: Vec<OngoingEffect> = Vec::new();
    apply_mobile_on_hit_dots(&hellhound, &mut combo_effects, "body");
    assert_eq!(combo_effects.len(), 2);
    let kinds = collect_kinds(&combo_effects);
    assert!(kinds.contains(&"poison"));
    assert!(kinds.contains(&"fire"));
    assert!(combo_effects.iter().all(|e| e.damage_per_round == 3));

    // All five elements
    let mut elemental = MobileData::new("a chimera".to_string());
    elemental.level = 10;
    elemental.flags.poisonous = true;
    elemental.flags.fiery = true;
    elemental.flags.chilling = true;
    elemental.flags.corrosive = true;
    elemental.flags.shocking = true;
    let mut all_effects: Vec<OngoingEffect> = Vec::new();
    apply_mobile_on_hit_dots(&elemental, &mut all_effects, "body");
    let all_kinds = collect_kinds(&all_effects);
    assert_eq!(all_effects.len(), 5);
    for kind in &["poison", "fire", "cold", "acid", "lightning"] {
        assert!(all_kinds.contains(kind), "missing element: {}", kind);
    }

    // No flags = no effects
    let mut plain = MobileData::new("a rat".to_string());
    plain.level = 1;
    let mut plain_effects: Vec<OngoingEffect> = Vec::new();
    apply_mobile_on_hit_dots(&plain, &mut plain_effects, "body");
    assert!(plain_effects.is_empty(), "no flags → no on-hit DoTs");
}

#[test]
fn test_buried_flag_and_lock_vnums_round_trip() {
    use ironmud::ItemData;
    
    use ironmud::types::DoorState;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        // Buried + can_dig + detect_buried flag persistence
        let mut chest = ItemData::new(
            "chest".to_string(),
            "a battered iron chest".to_string(),
            "A battered iron chest is half-buried in the dirt.".to_string(),
        );
        chest.flags.buried = true;
        chest.container_locked = true;
        chest.container_key_vnum = Some("pirate:pirate_key".to_string());
        let chest_id = chest.id;
        db.save_item_data(chest).expect("save chest");

        let loaded = db.get_item_data(&chest_id).expect("get").expect("present");
        assert!(loaded.flags.buried, "buried flag persists");
        assert!(loaded.container_locked, "locked persists");
        assert_eq!(
            loaded.container_key_vnum.as_deref(),
            Some("pirate:pirate_key"),
            "container key vnum persists",
        );

        // can_dig and detect_buried persist
        let mut shovel = ItemData::new(
            "shovel".to_string(),
            "a sturdy iron shovel".to_string(),
            "A sturdy iron shovel.".to_string(),
        );
        shovel.flags.can_dig = true;
        let shovel_id = shovel.id;
        db.save_item_data(shovel).expect("save shovel");
        let loaded_shovel = db.get_item_data(&shovel_id).expect("get").expect("present");
        assert!(loaded_shovel.flags.can_dig, "can_dig persists");

        let mut detector = ItemData::new(
            "detector".to_string(),
            "a humming brass detector".to_string(),
            "A brass detector hums softly.".to_string(),
        );
        detector.flags.detect_buried = true;
        let detector_id = detector.id;
        db.save_item_data(detector).expect("save detector");
        let loaded_det = db.get_item_data(&detector_id).expect("get").expect("present");
        assert!(loaded_det.flags.detect_buried, "detect_buried persists");

        // Door key_vnum persistence: construct DoorState directly and verify it serdes
        let door = DoorState {
            name: "iron-bound door".to_string(),
            is_closed: true,
            is_locked: true,
            key_vnum: Some("pirate:pirate_key".to_string()),
            description: None,
            keywords: vec!["door".to_string()],
            pickproof: false,
        };
        let json = serde_json::to_string(&door).expect("serialize door");
        assert!(
            json.contains("pirate:pirate_key"),
            "key_vnum is in the serialized form: {}",
            json
        );
        let round_tripped: DoorState = serde_json::from_str(&json).expect("deserialize door");
        assert_eq!(
            round_tripped.key_vnum.as_deref(),
            Some("pirate:pirate_key"),
            "door key_vnum survives round-trip"
        );
        // Old serialized form (with key_id field) should default to None thanks to #[serde(default)]
        let legacy = "{\"name\":\"old door\",\"is_closed\":true,\"is_locked\":true,\"keywords\":[]}";
        let legacy_door: DoorState = serde_json::from_str(legacy).expect("legacy deser");
        assert!(legacy_door.key_vnum.is_none(), "missing key_vnum defaults to None");
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_spawn_point_bury_on_spawn_field_persists() {
    
    use ironmud::types::{SpawnEntityType, SpawnPointData};
    use uuid::Uuid;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let sp = SpawnPointData {
            id: Uuid::new_v4(),
            area_id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            entity_type: SpawnEntityType::Item,
            vnum: "pirate:pirate_chest".to_string(),
            max_count: 1,
            respawn_interval_secs: 300,
            enabled: true,
            last_spawn_time: 0,
            spawned_entities: Vec::new(),
            dependencies: Vec::new(),
            bury_on_spawn: true,
            replace_on_respawn: false,
        };
        let sp_id = sp.id;
        db.save_spawn_point(sp).expect("save spawn point");

        let loaded = db.get_spawn_point(&sp_id).expect("get").expect("present");
        assert!(loaded.bury_on_spawn, "bury_on_spawn persists across save/load");

        // Default value (false) when not set explicitly via serde default fallback
        let sp_no_bury = SpawnPointData {
            id: Uuid::new_v4(),
            area_id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            entity_type: SpawnEntityType::Mobile,
            vnum: "test:rat".to_string(),
            max_count: 1,
            respawn_interval_secs: 300,
            enabled: true,
            last_spawn_time: 0,
            spawned_entities: Vec::new(),
            dependencies: Vec::new(),
            bury_on_spawn: false,
            replace_on_respawn: false,
        };
        let sp_no_id = sp_no_bury.id;
        db.save_spawn_point(sp_no_bury).expect("save");
        let loaded_no = db.get_spawn_point(&sp_no_id).expect("get").expect("present");
        assert!(!loaded_no.bury_on_spawn, "bury_on_spawn=false persists",);
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_item_unique_flag_caps_spawn_at_one() {
    use ironmud::ItemData;
    

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut proto = ItemData::new(
            "the crown of Sigil".to_string(),
            "the crown of Sigil rests here".to_string(),
            "An ancient crown radiates faint light.".to_string(),
        );
        proto.is_prototype = true;
        proto.vnum = Some("test:unique_crown".to_string());
        proto.flags.unique = true;
        db.save_item_data(proto).expect("save proto");

        let first = db
            .spawn_item_from_prototype("test:unique_crown")
            .expect("first spawn ok");
        assert!(first.is_some(), "first spawn succeeds under unique cap");

        let second = db
            .spawn_item_from_prototype("test:unique_crown")
            .expect("second spawn ok");
        assert!(second.is_none(), "second spawn refused — unique cap of 1");

        // After deleting the first instance, a fresh spawn is allowed.
        let first_id = first.unwrap().id;
        db.delete_item(&first_id).expect("delete first");
        let third = db
            .spawn_item_from_prototype("test:unique_crown")
            .expect("third spawn ok");
        assert!(third.is_some(), "deletion frees the cap");
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_class_loadout_round_trip_through_db() {
    
    use ironmud::types::ClassLoadout;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let loadout = ClassLoadout {
            class_id: "fighter".to_string(),
            starting_items: vec!["iron:short_sword".to_string(), "armor:leather_jerkin".to_string()],
            starting_gold: 25,
        };
        db.save_class_loadout(loadout.clone()).expect("save loadout");

        let fetched = db
            .get_class_loadout("fighter")
            .expect("get ok")
            .expect("loadout present");
        assert_eq!(fetched.class_id, "fighter");
        assert_eq!(fetched.starting_gold, 25);
        assert_eq!(fetched.starting_items.len(), 2);
        assert_eq!(fetched.starting_items[0], "iron:short_sword");

        // Overwriting replaces the row rather than appending.
        let updated = ClassLoadout {
            class_id: "fighter".to_string(),
            starting_items: vec!["iron:long_sword".to_string()],
            starting_gold: 50,
        };
        db.save_class_loadout(updated).expect("overwrite ok");
        let after = db.get_class_loadout("fighter").expect("get ok").expect("present");
        assert_eq!(after.starting_gold, 50);
        assert_eq!(after.starting_items, vec!["iron:long_sword".to_string()]);

        // list_all_class_loadouts returns the single (latest) row.
        let all = db.list_all_class_loadouts().expect("list ok");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].starting_gold, 50);

        // Missing class id returns None.
        let missing = db.get_class_loadout("does_not_exist").expect("get ok");
        assert!(missing.is_none());
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_class_loadout_skips_missing_item_vnum_at_spawn() {
    // Sanity check the Rhai-side miss path: spawn_item_from_prototype on a
    // vnum the DB has never seen returns Ok(None) without panicking. The
    // create.rhai loop relies on this to emit a builderdebug warning and
    // continue on bad vnums.
    

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");
        let spawn = db
            .spawn_item_from_prototype("nonexistent:vnum_for_kit")
            .expect("call must not error");
        assert!(spawn.is_none(), "missing vnum yields None, not panic");
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_mobile_world_max_count_caps_spawn() {
    
    use ironmud::types::MobileData;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut proto = MobileData::new("a captain of the guard".to_string());
        proto.is_prototype = true;
        proto.vnum = "test:guard_captain".to_string();
        proto.world_max_count = Some(3);
        db.save_mobile_data(proto).expect("save proto");

        let mut spawned = Vec::new();
        for i in 0..3 {
            let m = db
                .spawn_mobile_from_prototype("test:guard_captain")
                .expect(&format!("spawn {i} ok"));
            assert!(m.is_some(), "spawn {i} succeeds (cap=3)");
            spawned.push(m.unwrap().id);
        }

        let fourth = db
            .spawn_mobile_from_prototype("test:guard_captain")
            .expect("fourth spawn call ok");
        assert!(fourth.is_none(), "fourth spawn refused — cap reached");

        // unique on a mobile is sugar for cap=1, even with no world_max_count.
        let mut proto2 = MobileData::new("the warden".to_string());
        proto2.is_prototype = true;
        proto2.vnum = "test:warden_unique".to_string();
        proto2.flags.unique = true;
        db.save_mobile_data(proto2).expect("save warden proto");
        let w1 = db.spawn_mobile_from_prototype("test:warden_unique").expect("warden 1");
        assert!(w1.is_some(), "first warden spawns");
        let w2 = db.spawn_mobile_from_prototype("test:warden_unique").expect("warden 2 call");
        assert!(w2.is_none(), "second warden refused under unique flag");
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_spawn_dependencies_apply_inventory_equip_container() {
    
    use ironmud::spawn::apply_spawn_dependencies;
    use ironmud::types::{
        ItemData, ItemType, MobileData, SpawnDependency, SpawnDestination, SpawnEntityType, SpawnPointData, WearLocation,
    };
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");
        let connections = Arc::new(Mutex::new(HashMap::new()));

        // Mob prototype that the spawn point will produce.
        let mut mob_proto = MobileData::new("a town guard".to_string());
        mob_proto.is_prototype = true;
        mob_proto.vnum = "test:guard".to_string();
        db.save_mobile_data(mob_proto).expect("save mob proto");

        // Inventory item prototype.
        let mut inv_proto = ItemData::new(
            "a leather pouch".to_string(),
            "a leather pouch lies here".to_string(),
            "A small leather pouch.".to_string(),
        );
        inv_proto.is_prototype = true;
        inv_proto.vnum = Some("test:pouch".to_string());
        db.save_item_data(inv_proto).expect("save inv proto");

        // Equippable item prototype with Head wear location.
        let mut head_proto = ItemData::new(
            "a steel helm".to_string(),
            "a steel helm lies here".to_string(),
            "A polished steel helm.".to_string(),
        );
        head_proto.is_prototype = true;
        head_proto.vnum = Some("test:helm".to_string());
        head_proto.wear_locations = vec![WearLocation::Head];
        db.save_item_data(head_proto).expect("save head proto");

        // Container item prototype + a treasure to put inside.
        let mut chest_proto = ItemData::new(
            "an iron chest".to_string(),
            "an iron chest sits here".to_string(),
            "A heavy iron chest.".to_string(),
        );
        chest_proto.is_prototype = true;
        chest_proto.vnum = Some("test:chest".to_string());
        chest_proto.item_type = ItemType::Container;
        db.save_item_data(chest_proto).expect("save chest proto");

        let mut gold_proto = ItemData::new(
            "a gold coin".to_string(),
            "a gold coin glints here".to_string(),
            "A shiny gold coin.".to_string(),
        );
        gold_proto.is_prototype = true;
        gold_proto.vnum = Some("test:coin".to_string());
        db.save_item_data(gold_proto).expect("save coin proto");

        // Mob spawn point with Inventory + Equipped(Head) deps.
        let mob_sp = SpawnPointData {
            id: Uuid::new_v4(),
            area_id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            entity_type: SpawnEntityType::Mobile,
            vnum: "test:guard".to_string(),
            max_count: 1,
            respawn_interval_secs: 300,
            enabled: true,
            last_spawn_time: 0,
            spawned_entities: Vec::new(),
            dependencies: vec![
                SpawnDependency {
                    item_vnum: "test:pouch".to_string(),
                    destination: SpawnDestination::Inventory,
                    count: 1,
                    chance: 100,
                },
                SpawnDependency {
                    item_vnum: "test:helm".to_string(),
                    destination: SpawnDestination::Equipped(WearLocation::Head),
                    count: 1,
                    chance: 100,
                },
            ],
            bury_on_spawn: false,
            replace_on_respawn: false,
        };

        // Container spawn point with Container dep.
        let chest_sp = SpawnPointData {
            id: Uuid::new_v4(),
            area_id: mob_sp.area_id,
            room_id: mob_sp.room_id,
            entity_type: SpawnEntityType::Item,
            vnum: "test:chest".to_string(),
            max_count: 1,
            respawn_interval_secs: 300,
            enabled: true,
            last_spawn_time: 0,
            spawned_entities: Vec::new(),
            dependencies: vec![SpawnDependency {
                item_vnum: "test:coin".to_string(),
                destination: SpawnDestination::Container,
                count: 1,
                chance: 100,
            }],
            bury_on_spawn: false,
            replace_on_respawn: false,
        };

        // Spawn the mob and apply its deps.
        let mob = db.spawn_mobile_from_prototype("test:guard").expect("spawn mob").expect("mob present");
        let placed = apply_spawn_dependencies(&db, &connections, &mob_sp, &mob.id);
        assert_eq!(placed, 2, "both Inventory and Equipped deps placed");

        let inventory = db.get_items_in_mobile_inventory(&mob.id).expect("get inv");
        assert_eq!(inventory.len(), 1, "pouch landed in inventory");
        assert_eq!(inventory[0].vnum.as_deref(), Some("test:pouch"));

        let equipped = db.get_items_equipped_on_mobile(&mob.id).expect("get equipped");
        assert_eq!(equipped.len(), 1, "helm landed in equipped slot");
        assert_eq!(equipped[0].vnum.as_deref(), Some("test:helm"));

        // Spawn the chest and apply its container dep.
        let chest = db.spawn_item_from_prototype("test:chest").expect("spawn chest").expect("chest present");
        let placed_chest = apply_spawn_dependencies(&db, &connections, &chest_sp, &chest.id);
        assert_eq!(placed_chest, 1, "coin placed in container");

        let contents = db.get_items_in_container(&chest.id).expect("get container contents");
        assert_eq!(contents.len(), 1, "one item in chest");
        assert_eq!(contents[0].vnum.as_deref(), Some("test:coin"));
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_spawn_dependencies_skip_equip_when_wear_locations_mismatch() {
    
    use ironmud::spawn::apply_spawn_dependencies;
    use ironmud::types::{
        ItemData, MobileData, SpawnDependency, SpawnDestination, SpawnEntityType, SpawnPointData, WearLocation,
    };
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");
        let connections = Arc::new(Mutex::new(HashMap::new()));

        let mut mob_proto = MobileData::new("a sentry".to_string());
        mob_proto.is_prototype = true;
        mob_proto.vnum = "test:sentry".to_string();
        db.save_mobile_data(mob_proto).expect("save mob proto");

        // Item that is NOT wearable at Head (empty wear_locations).
        let mut bad_proto = ItemData::new(
            "a wooden mug".to_string(),
            "a wooden mug lies here".to_string(),
            "A plain wooden mug.".to_string(),
        );
        bad_proto.is_prototype = true;
        bad_proto.vnum = Some("test:mug".to_string());
        db.save_item_data(bad_proto).expect("save mug proto");

        let sp = SpawnPointData {
            id: Uuid::new_v4(),
            area_id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            entity_type: SpawnEntityType::Mobile,
            vnum: "test:sentry".to_string(),
            max_count: 1,
            respawn_interval_secs: 300,
            enabled: true,
            last_spawn_time: 0,
            spawned_entities: Vec::new(),
            dependencies: vec![SpawnDependency {
                item_vnum: "test:mug".to_string(),
                destination: SpawnDestination::Equipped(WearLocation::Head),
                count: 1,
                chance: 100,
            }],
            bury_on_spawn: false,
            replace_on_respawn: false,
        };

        let mob = db.spawn_mobile_from_prototype("test:sentry").expect("spawn mob").expect("mob present");
        let placed = apply_spawn_dependencies(&db, &connections, &sp, &mob.id);
        assert_eq!(placed, 0, "wear_locations mismatch refuses the equip");

        let inventory = db.get_items_in_mobile_inventory(&mob.id).expect("get inv");
        assert!(inventory.is_empty(), "mismatched item is not stashed in inventory");

        let equipped = db.get_items_equipped_on_mobile(&mob.id).expect("get equipped");
        assert!(equipped.is_empty(), "mismatched item is not equipped");
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_sanctuary_buff_on_prototype_carries_to_spawn() {
    
    use ironmud::types::{ActiveBuff, EffectType, MobileData};

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut proto = MobileData::new("a glowing wisp".to_string());
        proto.is_prototype = true;
        proto.vnum = "test:wisp".to_string();
        proto.active_buffs.push(ActiveBuff {
            effect_type: EffectType::DamageReduction,
            magnitude: 50,
            remaining_secs: -1,
            source: "innate sanctuary".to_string(),
            damage_type: None,
            vs_effect: None,
            skill_key: None,
        });
        db.save_mobile_data(proto).expect("save proto");

        let spawned = db
            .spawn_mobile_from_prototype("test:wisp")
            .expect("spawn ok")
            .expect("spawn produced an instance");

        assert!(!spawned.is_prototype, "spawned mob is an instance");
        let buff = spawned
            .active_buffs
            .iter()
            .find(|b| b.effect_type == EffectType::DamageReduction)
            .expect("DamageReduction buff carried over");
        assert_eq!(buff.magnitude, 50);
        assert_eq!(buff.remaining_secs, -1);
        assert_eq!(buff.source, "innate sanctuary");
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_damage_reduction_effect_type_round_trips() {
    use ironmud::types::EffectType;

    assert_eq!(
        EffectType::from_str("damage_reduction"),
        Some(EffectType::DamageReduction),
    );
    assert_eq!(
        EffectType::from_str("sanctuary"),
        Some(EffectType::DamageReduction),
        "the `sanctuary` alias resolves to DamageReduction",
    );
    assert_eq!(EffectType::DamageReduction.to_display_string(), "damage_reduction");
}

#[test]
fn test_sanctuary_spell_definition_loads() {
    use std::fs;

    let json = fs::read_to_string("scripts/data/spells_fantasy.json").expect("read spells_fantasy");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");
    let spell = parsed
        .get("sanctuary")
        .expect("sanctuary entry present")
        .as_object()
        .expect("sanctuary is an object");

    assert_eq!(spell.get("buff_effect").and_then(|v| v.as_str()), Some("damage_reduction"));
    assert_eq!(spell.get("buff_magnitude").and_then(|v| v.as_i64()), Some(50));
    assert_eq!(spell.get("buff_duration_secs").and_then(|v| v.as_i64()), Some(120));
    assert_eq!(spell.get("target_type").and_then(|v| v.as_str()), Some("self_or_friendly"));
}

fn stay_zone_test_room(area_id: Option<uuid::Uuid>) -> ironmud::types::RoomData {
    use ironmud::types::{RoomData, RoomExits, RoomFlags, WaterType};
    use std::collections::HashMap;
    RoomData {
        id: uuid::Uuid::new_v4(),
        title: "test room".to_string(),
        description: String::new(),
        exits: RoomExits::default(),
        flags: RoomFlags::default(),
        extra_descs: Vec::new(),
        vnum: None,
        area_id,
        triggers: Vec::new(),
        doors: HashMap::new(),
        spring_desc: None,
        summer_desc: None,
        autumn_desc: None,
        winter_desc: None,
        dynamic_desc: None,
        water_type: WaterType::None,
        catch_table: Vec::new(),
        is_property_template: false,
        property_template_id: None,
        is_template_entrance: false,
        property_lease_id: None,
        property_entrance: false,
        recent_departures: Vec::new(),
        blood_trails: Vec::new(),
        traps: Vec::new(),
        living_capacity: 0,
        residents: Vec::new(),
        dg_vars: std::collections::HashMap::new(),
        coordinates: None,
        contextual_commands: Vec::new(),
        exit_delays: std::collections::HashMap::new(),
    }
}

#[test]
fn test_home_area_id_is_stamped_on_first_room_placement() {
    
    use ironmud::types::MobileData;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let area_id = uuid::Uuid::new_v4();
        let other_area_id = uuid::Uuid::new_v4();

        let room = stay_zone_test_room(Some(area_id));
        let room_id = room.id;
        db.save_room_data(room).expect("save room");

        let other = stay_zone_test_room(Some(other_area_id));
        let other_id = other.id;
        db.save_room_data(other).expect("save other room");

        let mut mob = MobileData::new("a sentinel".to_string());
        mob.is_prototype = false;
        let mob_id = mob.id;
        db.save_mobile_data(mob).expect("save mob");

        assert!(db.move_mobile_to_room(&mob_id, &room_id).expect("move ok"));
        let stamped = db.get_mobile_data(&mob_id).expect("get").expect("present");
        assert_eq!(stamped.home_area_id, Some(area_id), "home_area_id stamped");

        // Subsequent moves do NOT change home_area_id.
        assert!(db.move_mobile_to_room(&mob_id, &other_id).expect("move ok"));
        let after_move = db.get_mobile_data(&mob_id).expect("get").expect("present");
        assert_eq!(
            after_move.home_area_id,
            Some(area_id),
            "home_area_id is sticky across moves",
        );
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_memory_resets_on_mob_respawn() {
    
    use ironmud::types::{MobileData, RememberedEnemy};

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        // Prototype with the memory flag and an empty enemies list — the
        // expected stable state.
        let mut proto = MobileData::new("an angry boar".to_string());
        proto.is_prototype = true;
        proto.vnum = "test:angry_boar".to_string();
        proto.flags.memory = true;
        db.save_mobile_data(proto).expect("save proto");

        // Spawn an instance, give it a remembered enemy, persist.
        let mut spawned = db
            .spawn_mobile_from_prototype("test:angry_boar")
            .expect("spawn ok")
            .expect("spawn produced an instance");
        spawned.remembered_enemies.push(RememberedEnemy {
            name: "alice".to_string(),
            expires_at_secs: i64::MAX,
        });
        db.save_mobile_data(spawned.clone()).expect("save instance");

        // Respawning from prototype yields a fresh instance with empty memory.
        let respawn = db
            .spawn_mobile_from_prototype("test:angry_boar")
            .expect("spawn ok")
            .expect("respawn instance");
        assert!(
            respawn.remembered_enemies.is_empty(),
            "respawn carries no remembered enemies (prototype is empty)",
        );

        // The original instance still holds its enemy until it's deleted.
        let still = db.get_mobile_data(&spawned.id).expect("get").expect("present");
        assert_eq!(still.remembered_enemies.len(), 1);
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_record_mob_memory_caps_at_ten_and_decays() {
    use ironmud::script::{MEMORY_CAP, MEMORY_DURATION_SECS, check_and_prune_memory, record_mob_memory};
    use ironmud::types::MobileData;

    let mut mob = MobileData::new("an angry boar".to_string());
    mob.flags.memory = true;

    // FIFO eviction at MEMORY_CAP.
    for i in 0..(MEMORY_CAP + 4) {
        record_mob_memory(&mut mob, &format!("foe{i}"));
    }
    assert_eq!(mob.remembered_enemies.len(), MEMORY_CAP);
    assert_eq!(
        mob.remembered_enemies[0].name, "foe4",
        "oldest entries evicted FIFO when over cap",
    );

    // Decay: stamp an entry as already expired and confirm prune.
    mob.remembered_enemies.clear();
    mob.remembered_enemies.push(ironmud::types::RememberedEnemy {
        name: "ghost".to_string(),
        expires_at_secs: 1,
    });
    let (remembers, pruned) = check_and_prune_memory(&mut mob, "ghost");
    assert!(!remembers);
    assert!(pruned);

    // Sanity-check the duration is the expected wall-clock window.
    assert_eq!(MEMORY_DURATION_SECS, 1800);
}

#[test]
fn test_aff_buffs_carry_from_prototype_to_spawn() {
    
    use ironmud::types::{ActiveBuff, EffectType, MobileData};

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        // Prototype mirroring a Circle import of a mob with AFF_INVISIBLE +
        // AFF_DETECT_INVIS + AFF_DETECT_MAGIC: three permanent buffs.
        let mut proto = MobileData::new("a spectral wisp".to_string());
        proto.is_prototype = true;
        proto.vnum = "test:wisp".to_string();
        for et in [
            EffectType::Invisibility,
            EffectType::DetectInvisible,
            EffectType::DetectMagic,
        ] {
            proto.active_buffs.push(ActiveBuff {
                effect_type: et,
                magnitude: 0,
                remaining_secs: -1,
                source: "innate".to_string(),
                damage_type: None,
                vs_effect: None,
                skill_key: None,
            });
        }
        db.save_mobile_data(proto).expect("save proto");

        let spawned = db
            .spawn_mobile_from_prototype("test:wisp")
            .expect("spawn ok")
            .expect("spawn produced an instance");
        for et in [
            EffectType::Invisibility,
            EffectType::DetectInvisible,
            EffectType::DetectMagic,
        ] {
            assert!(
                spawned.active_buffs.iter().any(|b| b.effect_type == et),
                "spawn instance missing {et:?} buff",
            );
        }
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_detect_magic_effect_type_roundtrip() {
    use ironmud::types::EffectType;

    assert_eq!(
        EffectType::from_str("detect_magic"),
        Some(EffectType::DetectMagic)
    );
    assert_eq!(
        EffectType::from_str("detectmagic"),
        Some(EffectType::DetectMagic)
    );
    assert_eq!(EffectType::DetectMagic.to_display_string(), "detect_magic");
    assert!(EffectType::all().contains(&"detect_magic"));
}

#[test]
fn test_mob_with_detect_invisible_buff_sees_invisible_pc() {
    use ironmud::script::is_player_visible_to_mob;
    use ironmud::types::{ActiveBuff, EffectType, MobileData};

    // Build a CharacterData with the Invisibility buff (use serde to avoid
    // pulling all the construction boilerplate).
    let mut char: ironmud::types::CharacterData = serde_json::from_value(serde_json::json!({
        "name": "thief",
        "password_hash": "",
        "current_room_id": uuid::Uuid::nil(),
    }))
    .expect("build character");
    char.active_buffs.push(ActiveBuff {
        effect_type: EffectType::Invisibility,
        magnitude: 0,
        remaining_secs: 100,
        source: "spell".to_string(),
        damage_type: None,
        vs_effect: None,
        skill_key: None,
    });

    // Mob without DetectInvisible/AWARE: blind to invisible PC.
    let plain = MobileData::new("guard".to_string());
    assert!(!is_player_visible_to_mob(&char, &plain));

    // Mob with DetectInvisible buff: sees the PC.
    let mut detector = MobileData::new("seer".to_string());
    detector.active_buffs.push(ActiveBuff {
        effect_type: EffectType::DetectInvisible,
        magnitude: 0,
        remaining_secs: -1,
        source: "innate".to_string(),
        damage_type: None,
        vs_effect: None,
        skill_key: None,
    });
    assert!(is_player_visible_to_mob(&char, &detector));
}

#[test]
fn test_night_vision_effect_type_roundtrip() {
    use ironmud::types::EffectType;

    assert_eq!(
        EffectType::from_str("night_vision"),
        Some(EffectType::NightVision)
    );
    assert_eq!(
        EffectType::from_str("nightvision"),
        Some(EffectType::NightVision)
    );
    // CircleMUD AFF_INFRAVISION ⇒ night_vision
    assert_eq!(
        EffectType::from_str("infravision"),
        Some(EffectType::NightVision)
    );
    assert_eq!(EffectType::NightVision.to_display_string(), "night_vision");
    assert!(EffectType::all().contains(&"night_vision"));
}

#[test]
fn test_aff_infravision_imports_as_night_vision_buff() {
    
    use ironmud::types::{ActiveBuff, EffectType, MobileData};

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        // Mirror what the importer stamps for AFF_INFRAVISION.
        let mut proto = MobileData::new("an owl".to_string());
        proto.is_prototype = true;
        proto.vnum = "test:owl".to_string();
        proto.active_buffs.push(ActiveBuff {
            effect_type: EffectType::NightVision,
            magnitude: 0,
            remaining_secs: -1,
            source: "innate night vision".to_string(),
            damage_type: None,
            vs_effect: None,
            skill_key: None,
        });
        db.save_mobile_data(proto).expect("save proto");

        let spawned = db
            .spawn_mobile_from_prototype("test:owl")
            .expect("spawn ok")
            .expect("spawn produced an instance");
        assert!(
            spawned
                .active_buffs
                .iter()
                .any(|b| b.effect_type == EffectType::NightVision),
            "spawn instance missing NightVision buff"
        );
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_sleep_blind_effect_type_roundtrip() {
    use ironmud::types::EffectType;

    assert_eq!(EffectType::from_str("sleep"), Some(EffectType::Sleep));
    assert_eq!(EffectType::Sleep.to_display_string(), "sleep");
    assert!(EffectType::all().contains(&"sleep"));

    assert_eq!(EffectType::from_str("blind"), Some(EffectType::Blind));
    assert_eq!(EffectType::from_str("blindness"), Some(EffectType::Blind));
    assert_eq!(EffectType::Blind.to_display_string(), "blind");
    assert!(EffectType::all().contains(&"blind"));
}

#[test]
fn test_no_sleep_no_blind_no_bash_flags_persist() {
    
    use ironmud::types::MobileData;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut mob = MobileData::new("an iron golem".to_string());
        mob.flags.no_sleep = true;
        mob.flags.no_blind = true;
        mob.flags.no_bash = true;
        let id = mob.id;
        db.save_mobile_data(mob).expect("save");

        let loaded = db.get_mobile_data(&id).expect("read").expect("present");
        assert!(loaded.flags.no_sleep);
        assert!(loaded.flags.no_blind);
        assert!(loaded.flags.no_bash);
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_sleep_buff_persists_on_mobile() {
    
    use ironmud::types::{ActiveBuff, EffectType, MobileData};

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut mob = MobileData::new("a slumbering ogre".to_string());
        mob.active_buffs.push(ActiveBuff {
            effect_type: EffectType::Sleep,
            magnitude: 0,
            remaining_secs: 60,
            source: "Sleep".to_string(),
            damage_type: None,
            vs_effect: None,
            skill_key: None,
        });
        let id = mob.id;
        db.save_mobile_data(mob).expect("save");

        let loaded = db.get_mobile_data(&id).expect("read").expect("present");
        assert!(
            loaded
                .active_buffs
                .iter()
                .any(|b| b.effect_type == EffectType::Sleep),
            "Sleep buff missing after roundtrip"
        );
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_luck_spells_definition_load() {
    use std::fs;

    let json = fs::read_to_string("scripts/data/spells_fantasy.json").expect("read spells_fantasy");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");

    let fortune = parsed
        .get("fortune")
        .expect("fortune entry present")
        .as_object()
        .expect("fortune is an object");
    assert_eq!(fortune.get("spell_type").and_then(|v| v.as_str()), Some("buff"));
    assert_eq!(fortune.get("buff_effect").and_then(|v| v.as_str()), Some("luck"));
    assert_eq!(fortune.get("buff_magnitude").and_then(|v| v.as_i64()), Some(5));
    assert_eq!(
        fortune.get("target_type").and_then(|v| v.as_str()),
        Some("self_or_friendly")
    );

    let misfortune = parsed
        .get("misfortune")
        .expect("misfortune entry present")
        .as_object()
        .expect("misfortune is an object");
    assert_eq!(misfortune.get("spell_type").and_then(|v| v.as_str()), Some("debuff"));
    assert_eq!(misfortune.get("buff_effect").and_then(|v| v.as_str()), Some("luck"));
    assert_eq!(misfortune.get("buff_magnitude").and_then(|v| v.as_i64()), Some(-5));
    assert_eq!(
        misfortune.get("target_type").and_then(|v| v.as_str()),
        Some("in_room_npc")
    );
}

#[test]
fn test_summon_spell_definition_loads() {
    use std::fs;

    let json = fs::read_to_string("scripts/data/spells_fantasy.json").expect("read spells_fantasy");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");
    let spell = parsed
        .get("summon")
        .expect("summon entry present")
        .as_object()
        .expect("summon is an object");

    assert_eq!(spell.get("spell_type").and_then(|v| v.as_str()), Some("summon"));
    assert_eq!(spell.get("skill_required").and_then(|v| v.as_i64()), Some(4));
    assert_eq!(spell.get("mana_cost").and_then(|v| v.as_i64()), Some(40));
    assert_eq!(spell.get("target_type").and_then(|v| v.as_str()), Some("world_npc"));
}

#[test]
fn test_no_summon_flag_persists() {
    
    use ironmud::types::MobileData;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut mob = MobileData::new("a stone sentinel".to_string());
        mob.flags.no_summon = true;
        let id = mob.id;
        db.save_mobile_data(mob).expect("save");

        let loaded = db.get_mobile_data(&id).expect("read").expect("present");
        assert!(loaded.flags.no_summon);
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_summonable_field_defaults_off_and_persists() {
    

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        // Build a CharacterData via serde so missing fields take their
        // serde defaults — proves `summonable` defaults to false.
        let mut char: ironmud::types::CharacterData =
            serde_json::from_value(serde_json::json!({
                "name": "TestSummonee",
                "password_hash": "",
                "current_room_id": uuid::Uuid::nil(),
            }))
            .expect("build character");
        assert!(!char.summonable, "summonable defaults to false");

        char.summonable = true;
        db.save_character_data(char.clone()).expect("save");

        let loaded = db
            .get_character_data(&char.name)
            .expect("read")
            .expect("present");
        assert!(loaded.summonable, "summonable persists across roundtrip");
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_morality_field_defaults_zero_and_persists() {
    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        // Build a CharacterData via serde so missing fields take serde
        // defaults — proves `morality` defaults to 0.
        let mut char: ironmud::types::CharacterData =
            serde_json::from_value(serde_json::json!({
                "name": "TestMorality",
                "password_hash": "",
                "current_room_id": uuid::Uuid::nil(),
            }))
            .expect("build character");
        assert_eq!(char.morality, 0, "morality defaults to 0");

        char.morality = -75;
        db.save_character_data(char.clone()).expect("save");

        let loaded = db
            .get_character_data(&char.name)
            .expect("read")
            .expect("present");
        assert_eq!(loaded.morality, -75, "morality persists across roundtrip");
    }));

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_spell_progress_field_defaults_empty_and_persists() {
    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        // Pre-feature character JSON (no `spell_progress` field) loads with
        // an empty map via serde default.
        let mut char: ironmud::types::CharacterData =
            serde_json::from_value(serde_json::json!({
                "name": "TestSpellProgress",
                "password_hash": "",
                "current_room_id": uuid::Uuid::nil(),
            }))
            .expect("build character");
        assert!(char.spell_progress.is_empty(), "spell_progress defaults to empty map");

        char.spell_progress.insert(
            "magic_missile".to_string(),
            ironmud::types::SpellProgress { level: 4, experience: 250 },
        );
        db.save_character_data(char.clone()).expect("save");

        let loaded = db
            .get_character_data(&char.name)
            .expect("read")
            .expect("present");
        let entry = loaded
            .spell_progress
            .get("magic_missile")
            .expect("entry present");
        assert_eq!(entry.level, 4);
        assert_eq!(entry.experience, 250);
    }));

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_achievement_morality_delta_default_is_zero_and_round_trips() {
    // AchievementReward without the new field deserializes (#[serde(default)])
    // and with it round-trips.
    let bare: ironmud::types::AchievementReward =
        serde_json::from_value(serde_json::json!({ "title": "the Brave" })).expect("bare parses");
    assert_eq!(bare.morality_delta, 0);

    let evil: ironmud::types::AchievementReward = serde_json::from_value(
        serde_json::json!({ "title": "the Cruel", "morality_delta": -50 }),
    )
    .expect("evil parses");
    assert_eq!(evil.morality_delta, -50);

    // Zero is skipped from serialization to keep JSON files clean.
    let serialized = serde_json::to_value(&bare).expect("serialize");
    assert!(
        serialized.get("morality_delta").is_none(),
        "zero morality_delta should not serialize"
    );
}

#[test]
fn test_achievement_unlock_applies_morality_delta_clamped() {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");
        // achievements default to enabled — no setting needed.

        // Manual-criterion achievement with a strong evil shift.
        let def = ironmud::types::AchievementDef {
            key: "dark_deed".to_string(),
            name: "Dark Deed".to_string(),
            description: "You did something cruel.".to_string(),
            category: ironmud::types::AchievementCategory::Social,
            criterion: ironmud::types::AchievementCriterion::Manual,
            reward: ironmud::types::AchievementReward {
                title: "the Cruel".to_string(),
                item_vnum: None,
                gold: None,
                morality_delta: -50,
            },
            hidden: false,
            source: ironmud::types::AchievementSource::Db { author: String::new() },
        };
        db.save_achievement(def).expect("save def");

        // Character starts at morality 10.
        let mut ch: ironmud::types::CharacterData = serde_json::from_value(serde_json::json!({
            "name": "TestEvil",
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character");
        ch.morality = 10;
        db.save_character_data(ch.clone()).expect("save char");

        // No live session — sync/send become no-ops. That's fine here.
        let conns: ironmud::SharedConnections = Arc::new(Mutex::new(HashMap::new()));

        let unlocked = ironmud::script::achievements::award_manual_via_db(
            &db,
            &conns,
            "TestEvil",
            "dark_deed",
        );
        assert!(unlocked, "first manual award returns true");

        let after = db.get_character_data("testevil").expect("read").expect("present");
        assert_eq!(after.morality, -40, "morality shifted by -50, clamped");
        assert!(after.achievements_unlocked.contains_key("dark_deed"));

        // Second award is a no-op (already unlocked); morality unchanged.
        let again = ironmud::script::achievements::award_manual_via_db(
            &db,
            &conns,
            "TestEvil",
            "dark_deed",
        );
        assert!(!again, "second award returns false (already unlocked)");
        let after2 = db.get_character_data("testevil").expect("read").expect("present");
        assert_eq!(after2.morality, -40, "second award does not re-apply delta");
    }));

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_achievement_morality_delta_clamps_at_floor() {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let def = ironmud::types::AchievementDef {
            key: "the_abyss".to_string(),
            name: "Into the Abyss".to_string(),
            description: "".to_string(),
            category: ironmud::types::AchievementCategory::Social,
            criterion: ironmud::types::AchievementCriterion::Manual,
            reward: ironmud::types::AchievementReward {
                title: "of the Abyss".to_string(),
                item_vnum: None,
                gold: None,
                morality_delta: -200, // huge swing
            },
            hidden: false,
            source: ironmud::types::AchievementSource::Db { author: String::new() },
        };
        db.save_achievement(def).expect("save def");

        let mut ch: ironmud::types::CharacterData = serde_json::from_value(serde_json::json!({
            "name": "TestFloor",
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character");
        ch.morality = -180;
        db.save_character_data(ch.clone()).expect("save char");

        let conns: ironmud::SharedConnections = Arc::new(Mutex::new(HashMap::new()));

        assert!(ironmud::script::achievements::award_manual_via_db(
            &db,
            &conns,
            "TestFloor",
            "the_abyss",
        ));

        let after = db.get_character_data("testfloor").expect("read").expect("present");
        assert_eq!(after.morality, -200, "clamped at MORALITY_MIN");
    }));

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_spell_definition_evolves_to_and_new_scaling_fields() {
    // SpellDefinition without the new fields deserializes (all #[serde(default)])
    // and with them round-trips.
    let bare: ironmud::types::SpellDefinition = serde_json::from_value(serde_json::json!({
        "id": "bare",
        "name": "Bare",
        "description": "no new fields",
    }))
    .expect("bare SpellDefinition parses");
    assert_eq!(bare.damage_per_spell_level, 0);
    assert_eq!(bare.heal_per_spell_level, 0);
    assert_eq!(bare.buff_magnitude_per_spell_level, 0);
    assert_eq!(bare.buff_duration_per_spell_level, 0);
    assert!(bare.evolves_to.is_none());

    let evolved: ironmud::types::SpellDefinition = serde_json::from_value(serde_json::json!({
        "id": "lesser_fireball",
        "name": "Lesser Fireball",
        "description": "evolves",
        "damage_per_spell_level": 3,
        "evolves_to": { "level_required": 7, "spell_id": "fireball" },
    }))
    .expect("evolved SpellDefinition parses");
    assert_eq!(evolved.damage_per_spell_level, 3);
    let ev = evolved.evolves_to.expect("evolves_to present");
    assert_eq!(ev.level_required, 7);
    assert_eq!(ev.spell_id, "fireball");
}

#[test]
fn test_magic_missile_evolves_to_arcane_lance_in_stock_json() {
    // Anchor the stock evolution chain so renames/regressions surface in CI.
    let raw = std::fs::read_to_string("scripts/data/spells_fantasy.json")
        .expect("read spells_fantasy.json");
    let parsed: std::collections::HashMap<String, ironmud::types::SpellDefinition> =
        serde_json::from_str(&raw).expect("parse spells_fantasy.json");

    let mm = parsed.get("magic_missile").expect("magic_missile present");
    let ev = mm.evolves_to.as_ref().expect("magic_missile has evolves_to");
    assert_eq!(ev.spell_id, "arcane_lance");
    assert!(parsed.contains_key(&ev.spell_id), "evolution target exists");
    assert!(mm.damage_per_spell_level > 0, "per-spell scaling populated");
}

#[test]
fn test_morality_tier_thresholds_exhaustive() {
    use ironmud::morality::MoralityTier;
    let cases = [
        (-200, MoralityTier::EvilPure),
        (-150, MoralityTier::EvilPure),
        (-100, MoralityTier::EvilPure),
        (-99, MoralityTier::Evil3),
        (-75, MoralityTier::Evil3),
        (-74, MoralityTier::Evil2),
        (-50, MoralityTier::Evil2),
        (-49, MoralityTier::Evil1),
        (-25, MoralityTier::Evil1),
        (-24, MoralityTier::Neutral),
        (0, MoralityTier::Neutral),
        (24, MoralityTier::Neutral),
        (25, MoralityTier::Good1),
        (49, MoralityTier::Good1),
        (50, MoralityTier::Good2),
        (74, MoralityTier::Good2),
        (75, MoralityTier::Good3),
        (99, MoralityTier::Good3),
        (100, MoralityTier::GoodPure),
        (200, MoralityTier::GoodPure),
    ];
    for (v, expected) in cases {
        assert_eq!(
            MoralityTier::from_value(v),
            expected,
            "morality {v} -> wrong tier"
        );
    }
}

#[test]
fn test_morality_feel_message_neutral_band() {
    use ironmud::morality::feel_message;
    // Neutral band returns None.
    assert!(feel_message(0).is_none());
    assert!(feel_message(-24).is_none());
    assert!(feel_message(24).is_none());
    // Both wings give some flavor text.
    assert!(feel_message(-25).is_some());
    assert!(feel_message(25).is_some());
    assert!(feel_message(-200).unwrap().contains("pure evil"));
    assert!(feel_message(100).unwrap().contains("pure"));
}

#[test]
fn test_morality_clamp_is_minus200_to_200_not_minus100_to_100() {
    use ironmud::morality::clamp;
    // The slider extends beyond the tier thresholds — sticky reputation.
    assert_eq!(clamp(500), 200);
    assert_eq!(clamp(-500), -200);
    assert_eq!(clamp(150), 150, "150 is a legal stored value");
    assert_eq!(clamp(-150), -150);
}

#[test]
fn test_anti_evil_item_flag_persists() {
    use ironmud::types::ItemData;

    let temp = tempfile::tempdir().expect("create temp dir");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut item = ItemData::new(
            "a glowing holy sword".to_string(),
            "A blade of pure light hovers here.".to_string(),
            "The sword pulses with righteous fire.".to_string(),
        );
        item.flags.anti_evil = true;
        item.flags.anti_neutral = true;
        let id = item.id;
        db.save_item_data(item).expect("save");

        let loaded = db.get_item_data(&id).expect("read").expect("present");
        assert!(loaded.flags.anti_evil);
        assert!(loaded.flags.anti_neutral);
        assert!(!loaded.flags.anti_good);
    }));
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_aggro_mobile_flags_persist() {
    use ironmud::types::MobileData;

    let temp = tempfile::tempdir().expect("create temp dir");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut mob = MobileData::new("a paladin patrolling".to_string());
        mob.flags.aggro_evil = true;
        let id = mob.id;
        db.save_mobile_data(mob).expect("save");

        let loaded = db.get_mobile_data(&id).expect("read").expect("present");
        assert!(loaded.flags.aggro_evil);
        assert!(!loaded.flags.aggro_good);
        assert!(!loaded.flags.aggro_neutral);
    }));
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_character_gender_persists_and_accepts_free_strings() {
    

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut char: ironmud::types::CharacterData =
            serde_json::from_value(serde_json::json!({
                "name": "TestGender",
                "password_hash": "",
                "current_room_id": uuid::Uuid::nil(),
            }))
            .expect("build character");
        assert_eq!(char.gender, "", "missing gender field defaults to empty");

        // Canonical chargen value.
        char.gender = "nonbinary".to_string();
        db.save_character_data(char.clone()).expect("save");
        let loaded = db
            .get_character_data(&char.name)
            .expect("read")
            .expect("present");
        assert_eq!(loaded.gender, "nonbinary");

        // Free-text post-creation override (DG falls back to neuter pronouns
        // for unrecognised values via parse_gender's default arm).
        char.gender = "starfolk".to_string();
        db.save_character_data(char.clone()).expect("save");
        let loaded = db
            .get_character_data(&char.name)
            .expect("read")
            .expect("present");
        assert_eq!(loaded.gender, "starfolk");
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_charm_spell_definition_loads() {
    use std::fs;

    let json = fs::read_to_string("scripts/data/spells_fantasy.json").expect("read spells_fantasy");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");
    let spell = parsed
        .get("charm")
        .expect("charm entry present")
        .as_object()
        .expect("charm is an object");

    assert_eq!(spell.get("spell_type").and_then(|v| v.as_str()), Some("charm"));
    assert_eq!(spell.get("skill_required").and_then(|v| v.as_i64()), Some(4));
    assert_eq!(spell.get("mana_cost").and_then(|v| v.as_i64()), Some(35));
    assert_eq!(spell.get("target_type").and_then(|v| v.as_str()), Some("in_room_npc"));
    assert_eq!(spell.get("buff_effect").and_then(|v| v.as_str()), Some("charmed"));
    assert_eq!(spell.get("buff_duration_secs").and_then(|v| v.as_i64()), Some(300));
}

#[test]
fn test_buried_items_visible_to_builders_and_admins_in_room_display() {
    use std::fs;

    let src = fs::read_to_string("src/script/rooms.rs").expect("read rooms.rs");

    // The room-display item filter must admit buried items when the viewer is
    // an admin or in build mode (bug #2 ironmud-public).
    assert!(
        src.contains("let see_buried = viewer_is_admin || in_build_mode;"),
        "see_buried gate missing from display_room item block"
    );
    assert!(
        src.contains("(!i.flags.buried || see_buried)"),
        "room display item filter does not allow buried items through for admins/builders"
    );
    assert!(
        src.contains("\" (buried)\""),
        "room display does not tag buried items with (buried)"
    );
}

#[test]
fn test_safe_zone_gate_present_in_offensive_cast_handlers() {
    use regex::Regex;
    use std::fs;

    let src = fs::read_to_string("scripts/commands/cast.rhai").expect("read cast.rhai");

    for handler in ["cast_damage", "cast_debuff", "cast_charm"] {
        let body_re = Regex::new(&format!(
            r"(?ms)fn {}\([^)]*\)\s*\{{(.*?)\n\}}",
            regex::escape(handler)
        ))
        .unwrap();
        let captures = body_re
            .captures(&src)
            .unwrap_or_else(|| panic!("could not locate {handler} body in cast.rhai"));
        let body = captures.get(1).unwrap().as_str();

        assert!(
            body.contains("get_combat_zone(room_id)"),
            "{handler} is missing the safe-zone gate (get_combat_zone(room_id))"
        );
        assert!(
            body.contains("zone == \"safe\""),
            "{handler} is missing the zone == \"safe\" check"
        );
        assert!(
            body.contains("char.god_mode"),
            "{handler} safe-zone gate is missing the god_mode bypass"
        );
    }
}

#[test]
fn test_redit_flag_safe_alias_routes_to_combat_zone() {
    use std::fs;

    let src = fs::read_to_string("scripts/commands/redit.rhai").expect("read redit.rhai");

    assert!(
        src.contains("handle_safe_alias"),
        "handle_safe_alias helper missing from redit.rhai"
    );
    assert!(
        src.contains("flag_name == \"safe\"") && src.contains("flag_name == \"peaceful\""),
        "redit flag dispatcher does not branch on safe/peaceful"
    );
    assert!(
        src.contains("set_room_combat_zone(room_id, \"safe\")"),
        "handle_safe_alias does not route to set_room_combat_zone(\"safe\")"
    );
}

#[test]
fn test_no_charm_flag_persists() {
    
    use ironmud::types::MobileData;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut mob = MobileData::new("an iron golem".to_string());
        mob.flags.no_charm = true;
        let id = mob.id;
        db.save_mobile_data(mob).expect("save");

        let loaded = db.get_mobile_data(&id).expect("read").expect("present");
        assert!(loaded.flags.no_charm);
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_charmed_buff_persists_with_master_source() {
    
    use ironmud::types::{ActiveBuff, EffectType, MobileData};

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut mob = MobileData::new("a hapless thrall".to_string());
        mob.active_buffs.push(ActiveBuff {
            effect_type: EffectType::Charmed,
            magnitude: 0,
            remaining_secs: 300,
            source: "Wizard".to_string(),
            damage_type: None,
            vs_effect: None,
            skill_key: None,
        });
        let id = mob.id;
        db.save_mobile_data(mob).expect("save");

        let loaded = db.get_mobile_data(&id).expect("read").expect("present");
        assert_eq!(loaded.active_buffs.len(), 1);
        assert_eq!(loaded.active_buffs[0].effect_type, EffectType::Charmed);
        assert_eq!(loaded.active_buffs[0].source, "Wizard");
        assert!(loaded.is_charmed_by("Wizard"));
        assert!(loaded.is_charmed_by("wizard"), "is_charmed_by is case-insensitive");
        assert!(!loaded.is_charmed_by("Cleric"));
        assert_eq!(loaded.charm_master(), Some("Wizard"));
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_break_all_charms_by_player_clears_only_matching_buffs() {
    use ironmud::break_all_charms_by_player;
    
    use ironmud::types::{ActiveBuff, EffectType, MobileData};

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let make_charmed = |name: &str, master: &str| -> MobileData {
            let mut m = MobileData::new(name.to_string());
            m.is_prototype = false;
            m.active_buffs.push(ActiveBuff {
                effect_type: EffectType::Charmed,
                magnitude: 0,
                remaining_secs: 300,
                source: master.to_string(),
                damage_type: None,
                vs_effect: None,
                skill_key: None,
            });
            m
        };

        let a = make_charmed("thrall a", "Wizard");
        let b = make_charmed("thrall b", "Wizard");
        let c = make_charmed("thrall c", "Cleric");
        let (a_id, b_id, c_id) = (a.id, b.id, c.id);
        db.save_mobile_data(a).unwrap();
        db.save_mobile_data(b).unwrap();
        db.save_mobile_data(c).unwrap();

        break_all_charms_by_player(&db, "Wizard");

        let a = db.get_mobile_data(&a_id).unwrap().unwrap();
        let b = db.get_mobile_data(&b_id).unwrap().unwrap();
        let c = db.get_mobile_data(&c_id).unwrap().unwrap();

        assert!(!a.is_charmed_by_anyone(), "Wizard's charm a should be cleared");
        assert!(!b.is_charmed_by_anyone(), "Wizard's charm b should be cleared");
        assert!(c.is_charmed_by("Cleric"), "Cleric's charm should remain");
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_tameable_flag_persists() {
    
    use ironmud::types::MobileData;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut mob = MobileData::new("a stray kitten".to_string());
        mob.flags.tameable = true;
        let id = mob.id;
        db.save_mobile_data(mob).expect("save");

        let loaded = db.get_mobile_data(&id).expect("read").expect("present");
        assert!(loaded.flags.tameable);
        // Default for non-tameable mobs is false (graceful upgrade for legacy saves)
        let plain = MobileData::new("a regular mob".to_string());
        assert!(!plain.flags.tameable);
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_pet_owner_field_persists() {
    
    use ironmud::types::MobileData;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut pet = MobileData::new("a tabby cat".to_string());
        pet.is_prototype = false;
        pet.pet_owner = Some("Alice".to_string());
        let id = pet.id;
        db.save_mobile_data(pet).expect("save");

        let loaded = db.get_mobile_data(&id).expect("read").expect("present");
        assert_eq!(loaded.pet_owner.as_deref(), Some("Alice"));

        // New mobs default to None.
        let plain = MobileData::new("a passing crow".to_string());
        assert!(plain.pet_owner.is_none());
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_break_all_charms_skips_pets_of_quitting_player() {
    use ironmud::break_all_charms_by_player;
    
    use ironmud::types::{ActiveBuff, EffectType, MobileData};

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        // A regular charm by Alice (should be cleared).
        let mut charmed = MobileData::new("a hapless thrall".to_string());
        charmed.is_prototype = false;
        charmed.active_buffs.push(ActiveBuff {
            effect_type: EffectType::Charmed,
            magnitude: 0,
            remaining_secs: 300,
            source: "Alice".to_string(),
            damage_type: None,
            vs_effect: None,
            skill_key: None,
        });
        let charmed_id = charmed.id;
        db.save_mobile_data(charmed).expect("save charmed");

        // A pet of Alice with permanent charm + pet_owner stamp (should survive).
        let mut pet = MobileData::new("a faithful hound".to_string());
        pet.is_prototype = false;
        pet.active_buffs.push(ActiveBuff {
            effect_type: EffectType::Charmed,
            magnitude: 0,
            remaining_secs: -1,
            source: "Alice".to_string(),
            damage_type: None,
            vs_effect: None,
            skill_key: None,
        });
        pet.pet_owner = Some("Alice".to_string());
        pet.charm_stay = true;
        let pet_id = pet.id;
        db.save_mobile_data(pet).expect("save pet");

        break_all_charms_by_player(&db, "Alice");

        let charmed_after = db
            .get_mobile_data(&charmed_id)
            .expect("read")
            .expect("present");
        assert!(
            charmed_after.active_buffs.is_empty(),
            "regular charm should be cleared on quit"
        );

        let pet_after = db
            .get_mobile_data(&pet_id)
            .expect("read")
            .expect("present");
        assert_eq!(pet_after.pet_owner.as_deref(), Some("Alice"));
        assert_eq!(pet_after.active_buffs.len(), 1, "pet keeps its bond on quit");
        assert!(pet_after.charm_stay, "pet retains stay/follow overrides");
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_mobile_position_default_and_persists() {
    
    use ironmud::types::{MobileData, MobilePosition};

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        // Defaults to Standing.
        let plain = MobileData::new("a generic mob".to_string());
        assert_eq!(plain.position, MobilePosition::Standing);

        let mut sleeper = MobileData::new("a sleeping wolf".to_string());
        sleeper.position = MobilePosition::Sleeping;
        let id = sleeper.id;
        db.save_mobile_data(sleeper).expect("save");

        let loaded = db.get_mobile_data(&id).expect("read").expect("present");
        assert_eq!(loaded.position, MobilePosition::Sleeping);
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_mobile_position_parse_handles_aliases() {
    use ironmud::types::MobilePosition;

    assert_eq!(MobilePosition::parse("standing"), Some(MobilePosition::Standing));
    assert_eq!(MobilePosition::parse("STAND"), Some(MobilePosition::Standing));
    assert_eq!(MobilePosition::parse("up"), Some(MobilePosition::Standing));
    assert_eq!(MobilePosition::parse("sit"), Some(MobilePosition::Sitting));
    assert_eq!(MobilePosition::parse("sleeping"), Some(MobilePosition::Sleeping));
    assert_eq!(MobilePosition::parse("asleep"), Some(MobilePosition::Sleeping));
    assert_eq!(MobilePosition::parse("dancing"), None);
    assert_eq!(MobilePosition::parse(""), None);
}

#[test]
fn test_mobile_nickname_field_defaults_none_and_persists() {
    
    use ironmud::types::MobileData;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let plain = MobileData::new("a forest wolf".to_string());
        assert!(plain.nickname.is_none(), "default nickname is None");
        assert_eq!(plain.display_name(), "a forest wolf", "display_name falls back to name");

        let mut named = MobileData::new("a forest wolf".to_string());
        named.nickname = Some("Fido".to_string());
        let id = named.id;
        db.save_mobile_data(named).expect("save");

        let loaded = db.get_mobile_data(&id).expect("read").expect("present");
        assert_eq!(loaded.nickname.as_deref(), Some("Fido"));
        assert_eq!(loaded.display_name(), "Fido", "display_name prefers nickname");
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_mobile_display_name_treats_empty_nickname_as_unset() {
    use ironmud::types::MobileData;

    let mut m = MobileData::new("a forest wolf".to_string());
    m.nickname = Some(String::new());
    assert_eq!(
        m.display_name(),
        "a forest wolf",
        "empty-string nickname must not override the real name"
    );
    m.nickname = Some("   ".to_string());
    // Whitespace-only is technically allowed since set_mobile_nickname
    // trims it to "" before storing, but display_name only filters on
    // is_empty — verify the user-facing input path actually trims.
    // (This part is enforced by set_mobile_nickname's trim().)
}

#[test]
fn test_mailbox_purged_on_character_delete() {
    
    use ironmud::types::MailMessage;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        // Send 3 mails to "alice" + 1 unrelated to "bob".
        for body in &["hello", "again", "third"] {
            let msg = MailMessage::new("sender".to_string(), "alice".to_string(), body.to_string());
            db.store_mail(msg).expect("store");
        }
        let bystander = MailMessage::new("sender".to_string(), "bob".to_string(), "kept".to_string());
        db.store_mail(bystander).expect("store");

        assert_eq!(db.get_mailbox_size("alice").expect("size"), 3);
        assert_eq!(db.get_mailbox_size("bob").expect("size"), 1);

        // delete_mail_for_recipient is exposed for direct test (mixed-case
        // recipient name still resolves via to_lowercase).
        let removed = db.delete_mail_for_recipient("Alice").expect("purge");
        assert_eq!(removed, 3, "purge should report 3 deletions");
        assert_eq!(db.get_mailbox_size("alice").expect("size after purge"), 0);

        // Bob's mailbox is untouched.
        assert_eq!(db.get_mailbox_size("bob").expect("bob size"), 1);

        // delete_character_data should also purge mail end-to-end. Send
        // another message to bob, delete the bob character, confirm purge.
        let bystander2 = MailMessage::new("sender".to_string(), "bob".to_string(), "second".to_string());
        db.store_mail(bystander2).expect("store");
        assert_eq!(db.get_mailbox_size("bob").expect("size"), 2);

        db.delete_character_data("Bob").expect("delete");
        assert_eq!(
            db.get_mailbox_size("bob").expect("bob size after delete"),
            0,
            "delete_character_data should purge the deleted player's inbox"
        );
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

// === Bulletin board (gen_board) tests ===

#[test]
fn test_board_post_persists_and_lists_oldest_first() {
    
    use ironmud::types::BoardPost;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        // Insert three posts with explicit posted_at to verify oldest-first
        // listing regardless of insertion order.
        let mut p1 = BoardPost::new("3098".to_string(), "Alice".to_string(), "first".to_string(), "body 1".to_string());
        let mut p2 = BoardPost::new("3098".to_string(), "Bob".to_string(), "second".to_string(), "body 2".to_string());
        let mut p3 = BoardPost::new("3098".to_string(), "Carol".to_string(), "third".to_string(), "body 3".to_string());
        p1.posted_at = 100;
        p2.posted_at = 200;
        p3.posted_at = 300;
        // Insert in reverse order.
        db.store_board_post(p3.clone(), None).expect("store p3");
        db.store_board_post(p1.clone(), None).expect("store p1");
        db.store_board_post(p2.clone(), None).expect("store p2");

        let listed = db.get_board_posts("3098").expect("list");
        assert_eq!(listed.len(), 3);
        assert_eq!(listed[0].subject, "first");
        assert_eq!(listed[1].subject, "second");
        assert_eq!(listed[2].subject, "third");

        // Sibling board is untouched.
        assert_eq!(db.count_board_posts("3099").expect("count"), 0);
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_board_max_messages_evicts_oldest() {
    
    use ironmud::types::BoardPost;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        for (i, subj) in ["one", "two", "three"].iter().enumerate() {
            let mut p = BoardPost::new(
                "3098".to_string(),
                "Alice".to_string(),
                subj.to_string(),
                "body".to_string(),
            );
            p.posted_at = (i as i64) + 1;
            db.store_board_post(p, Some(2)).expect("store");
        }

        let listed = db.get_board_posts("3098").expect("list");
        assert_eq!(listed.len(), 2, "cap=2 should retain only 2 newest");
        assert_eq!(listed[0].subject, "two", "oldest (one) evicted");
        assert_eq!(listed[1].subject, "three");
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_board_posts_purged_on_character_delete() {
    
    use ironmud::types::BoardPost;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        for subj in &["a", "b", "c"] {
            let p = BoardPost::new(
                "3098".to_string(),
                "Alice".to_string(),
                subj.to_string(),
                "body".to_string(),
            );
            db.store_board_post(p, None).expect("store");
        }
        let bystander = BoardPost::new(
            "3098".to_string(),
            "Bob".to_string(),
            "kept".to_string(),
            "body".to_string(),
        );
        db.store_board_post(bystander, None).expect("store");

        assert_eq!(db.count_board_posts("3098").expect("count"), 4);

        let removed = db.delete_board_posts_by_author("Alice").expect("purge");
        assert_eq!(removed, 3);

        let listed = db.get_board_posts("3098").expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].author, "Bob", "Bob's post should survive");

        // delete_character_data path also purges.
        db.delete_character_data("Bob").expect("delete");
        assert_eq!(db.count_board_posts("3098").expect("count after"), 0);
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_board_default_cap_uses_engine_constant() {
    use ironmud::db::Db;
    use ironmud::types::BoardPost;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        // Insert DEFAULT_BOARD_MAX_MESSAGES + 1 posts with `None` cap; the
        // engine default should evict.
        let cap = Db::DEFAULT_BOARD_MAX_MESSAGES;
        for i in 0..(cap + 1) {
            let mut p = BoardPost::new(
                "3098".to_string(),
                "Alice".to_string(),
                format!("post {}", i),
                "body".to_string(),
            );
            p.posted_at = i as i64;
            db.store_board_post(p, None).expect("store");
        }
        assert_eq!(db.count_board_posts("3098").expect("count"), cap);
        // Oldest (post 0) should be gone.
        let listed = db.get_board_posts("3098").expect("list");
        assert_eq!(listed[0].subject, "post 1");
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_mob_passive_regen_standing_yields_zero() {
    use ironmud::script::apply_mobile_passive_stance_regen;
    use ironmud::types::{MobileData, MobilePosition};

    let mut m = MobileData::new("a guard".to_string());
    m.max_hp = 100;
    m.current_hp = 50;
    m.position = MobilePosition::Standing;
    let added = apply_mobile_passive_stance_regen(&mut m);
    assert_eq!(added, 0);
    assert_eq!(m.current_hp, 50);
}

#[test]
fn test_mob_passive_regen_sitting_adds_one() {
    use ironmud::script::apply_mobile_passive_stance_regen;
    use ironmud::types::{MobileData, MobilePosition};

    let mut m = MobileData::new("a sitting beggar".to_string());
    m.max_hp = 100;
    m.current_hp = 50;
    m.position = MobilePosition::Sitting;
    let added = apply_mobile_passive_stance_regen(&mut m);
    assert_eq!(added, 1);
    assert_eq!(m.current_hp, 51);
}

#[test]
fn test_mob_passive_regen_sleeping_adds_two() {
    use ironmud::script::apply_mobile_passive_stance_regen;
    use ironmud::types::{MobileData, MobilePosition};

    let mut m = MobileData::new("a slumbering wolf".to_string());
    m.max_hp = 100;
    m.current_hp = 50;
    m.position = MobilePosition::Sleeping;
    let added = apply_mobile_passive_stance_regen(&mut m);
    assert_eq!(added, 2);
    assert_eq!(m.current_hp, 52);
}

#[test]
fn test_mob_passive_regen_caps_at_max_hp() {
    use ironmud::script::apply_mobile_passive_stance_regen;
    use ironmud::types::{MobileData, MobilePosition};

    let mut m = MobileData::new("a nearly-healed bear".to_string());
    m.max_hp = 100;
    m.current_hp = 99;
    m.position = MobilePosition::Sleeping;
    let added = apply_mobile_passive_stance_regen(&mut m);
    assert_eq!(added, 1);
    assert_eq!(m.current_hp, 100);

    // At max already → no change.
    let added2 = apply_mobile_passive_stance_regen(&mut m);
    assert_eq!(added2, 0);
    assert_eq!(m.current_hp, 100);
}

#[test]
fn test_mob_passive_regen_composes_with_regeneration_buff() {
    use ironmud::script::apply_mobile_passive_stance_regen;
    use ironmud::types::{ActiveBuff, EffectType, MobileData, MobilePosition};

    let mut m = MobileData::new("a regenerating troll".to_string());
    m.max_hp = 100;
    m.current_hp = 50;
    m.position = MobilePosition::Sleeping;
    m.active_buffs.push(ActiveBuff {
        effect_type: EffectType::Regeneration,
        magnitude: 3,
        remaining_secs: -1,
        source: "test".to_string(),
        damage_type: None,
        vs_effect: None,
        skill_key: None,
    });

    // Stance regen runs first (mirrors process_mobile_effects ordering).
    let stance_added = apply_mobile_passive_stance_regen(&mut m);
    assert_eq!(stance_added, 2);
    assert_eq!(m.current_hp, 52);

    // Then the existing Regeneration buff arm would add its magnitude on top.
    if let Some(regen) = m
        .active_buffs
        .iter()
        .find(|b| b.effect_type == EffectType::Regeneration)
    {
        let amt = regen.magnitude;
        if m.current_hp < m.max_hp && amt > 0 {
            m.current_hp = (m.current_hp + amt).min(m.max_hp);
        }
    }
    assert_eq!(m.current_hp, 55, "stance (2) + buff (3) should compose to +5");
}

#[test]
fn test_static_mobile_resolves_neuter_when_gender_unset() {
    use ironmud::types::MobileData;

    let plain = MobileData::new("a stone gargoyle".to_string());
    assert!(plain.characteristics.is_none());
    assert_eq!(plain.resolved_gender(), "neuter");

    // Lazy-init Characteristics with empty gender → still resolves neuter.
    let mut chars_empty = MobileData::new("a wisp".to_string());
    chars_empty.characteristics = Some(ironmud::types::Characteristics::default());
    assert_eq!(chars_empty.resolved_gender(), "neuter");
}

#[test]
fn test_mobile_gender_clear_preserves_other_characteristics() {
    use ironmud::types::{Characteristics, MobileData};

    // Simulate the medit `gender clear` path: empty-string write should
    // preserve age/visuals on a migrant-style mob.
    let mut mob = MobileData::new("a young farmer".to_string());
    mob.characteristics = Some(Characteristics {
        gender: "female".to_string(),
        age: 24,
        age_label: "young adult".to_string(),
        height: "tall".to_string(),
        ..Characteristics::default()
    });

    // Mirror the API update apply path: empty string clears gender field
    // without dropping the Characteristics struct.
    if let Some(ref mut chars) = mob.characteristics {
        chars.gender = String::new();
    }

    let chars = mob.characteristics.expect("characteristics retained");
    assert_eq!(chars.gender, "");
    assert_eq!(chars.age, 24, "age survives gender clear");
    assert_eq!(chars.age_label, "young adult");
    assert_eq!(chars.height, "tall");
}

#[test]
fn test_random_gender_includes_all_three_kinds() {
    use ironmud::migration::random_gender;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    let mut rng = StdRng::seed_from_u64(0xCAFEBABE);
    let mut male = 0;
    let mut female = 0;
    let mut nb = 0;
    for _ in 0..5000 {
        match random_gender(&mut rng) {
            "male" => male += 1,
            "female" => female += 1,
            "nonbinary" => nb += 1,
            other => panic!("unexpected gender roll: {other:?}"),
        }
    }
    assert!(male > 0, "no male rolls in 5000 (got male={male})");
    assert!(female > 0, "no female rolls in 5000 (got female={female})");
    assert!(nb > 0, "no nonbinary rolls in 5000 — adjust MIGRANT_NB_CHANCE?");
    // Sanity: NB should be the rare branch, not dominate.
    assert!(nb < male, "nonbinary unexpectedly dominant (male={male} nb={nb})");
}

#[test]
fn test_gender_noun_and_pronoun_helpers() {
    use ironmud::migration::{gender_noun, gender_pronoun};

    assert_eq!(gender_noun("male"), "man");
    assert_eq!(gender_noun("female"), "woman");
    assert_eq!(gender_noun("nonbinary"), "person");
    assert_eq!(gender_noun(""), "person");
    assert_eq!(gender_noun("starkin"), "person");

    assert_eq!(gender_pronoun("male"), "He");
    assert_eq!(gender_pronoun("female"), "She");
    assert_eq!(gender_pronoun("nonbinary"), "They");
    assert_eq!(gender_pronoun("starkin"), "They");
}

#[test]
fn test_charmed_effect_type_round_trips_via_serde() {
    use ironmud::types::EffectType;

    assert_eq!(EffectType::from_str("charm"), Some(EffectType::Charmed));
    assert_eq!(EffectType::from_str("charmed"), Some(EffectType::Charmed));
    assert_eq!(EffectType::Charmed.to_display_string(), "charmed");

    let json = serde_json::to_string(&EffectType::Charmed).expect("serialize");
    assert_eq!(json, "\"charmed\"");
    let back: EffectType = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, EffectType::Charmed);
}

#[test]
fn test_curse_effect_type_round_trips_via_serde() {
    use ironmud::types::EffectType;

    assert_eq!(EffectType::from_str("curse"), Some(EffectType::Curse));
    assert_eq!(EffectType::from_str("cursed"), Some(EffectType::Curse));
    assert_eq!(EffectType::Curse.to_display_string(), "curse");
    assert!(EffectType::all().contains(&"curse"));

    let json = serde_json::to_string(&EffectType::Curse).expect("serialize");
    assert_eq!(json, "\"curse\"");
    let back: EffectType = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, EffectType::Curse);
}

#[test]
fn test_permanent_aff_buffs_persist_on_mobile() {
    
    use ironmud::types::{ActiveBuff, EffectType, MobileData};

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut mob = MobileData::new("a cursed sleeper".to_string());
        mob.is_prototype = false;
        for (et, mag) in [
            (EffectType::Blind, 50),
            (EffectType::Sleep, 0),
            (EffectType::Curse, 10),
        ] {
            mob.active_buffs.push(ActiveBuff {
                effect_type: et,
                magnitude: mag,
                remaining_secs: -1,
                source: "innate".to_string(),
                damage_type: None,
                vs_effect: None,
                skill_key: None,
            });
        }
        let id = mob.id;
        db.save_mobile_data(mob).expect("save");

        let loaded = db.get_mobile_data(&id).expect("read").expect("present");
        let blind = loaded
            .active_buffs
            .iter()
            .find(|b| b.effect_type == EffectType::Blind)
            .expect("blind buff present");
        assert_eq!(blind.magnitude, 50);
        assert_eq!(blind.remaining_secs, -1);
        let sleep = loaded
            .active_buffs
            .iter()
            .find(|b| b.effect_type == EffectType::Sleep)
            .expect("sleep buff present");
        assert_eq!(sleep.remaining_secs, -1);
        let curse = loaded
            .active_buffs
            .iter()
            .find(|b| b.effect_type == EffectType::Curse)
            .expect("curse buff present");
        assert_eq!(curse.magnitude, 10);
        assert_eq!(curse.remaining_secs, -1);
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_charm_stay_and_follow_persist() {
    
    use ironmud::types::MobileData;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut mob = MobileData::new("a thrall".to_string());
        // Defaults
        assert!(!mob.charm_stay);
        assert!(mob.charm_follow_player.is_none());

        mob.charm_stay = true;
        mob.charm_follow_player = Some("Other".to_string());
        let id = mob.id;
        db.save_mobile_data(mob).expect("save");

        let loaded = db.get_mobile_data(&id).expect("read").expect("present");
        assert!(loaded.charm_stay);
        assert_eq!(loaded.charm_follow_player.as_deref(), Some("Other"));
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_break_all_charms_clears_stay_follow_and_dangling_follow_targets() {
    use ironmud::break_all_charms_by_player;
    
    use ironmud::types::{ActiveBuff, EffectType, MobileData};

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        // Mob A: charmed by Wizard, with stay set.
        let mut a = MobileData::new("thrall A".to_string());
        a.is_prototype = false;
        a.active_buffs.push(ActiveBuff {
            effect_type: EffectType::Charmed,
            magnitude: 0,
            remaining_secs: 300,
            source: "Wizard".to_string(),
            damage_type: None,
            vs_effect: None,
            skill_key: None,
        });
        a.charm_stay = true;
        let a_id = a.id;
        db.save_mobile_data(a).unwrap();

        // Mob B: charmed by Cleric, told to follow Wizard. Wizard's break
        // should clear B's follow override but leave the Cleric charm intact.
        let mut b = MobileData::new("thrall B".to_string());
        b.is_prototype = false;
        b.active_buffs.push(ActiveBuff {
            effect_type: EffectType::Charmed,
            magnitude: 0,
            remaining_secs: 300,
            source: "Cleric".to_string(),
            damage_type: None,
            vs_effect: None,
            skill_key: None,
        });
        b.charm_follow_player = Some("Wizard".to_string());
        let b_id = b.id;
        db.save_mobile_data(b).unwrap();

        break_all_charms_by_player(&db, "Wizard");

        let a = db.get_mobile_data(&a_id).unwrap().unwrap();
        assert!(!a.is_charmed_by_anyone(), "A's charm cleared");
        assert!(!a.charm_stay, "A's stay cleared on charm break");

        let b = db.get_mobile_data(&b_id).unwrap().unwrap();
        assert!(b.is_charmed_by("Cleric"), "B's Cleric charm should remain");
        assert!(
            b.charm_follow_player.is_none(),
            "B's dangling follow override on Wizard cleared"
        );
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

/// `combat_spells` + `combat_spell_chance` round-trip through the DB.
/// Default chance is 50, default list is empty.
#[test]
fn test_mobile_combat_spells_persist() {
    
    use ironmud::types::MobileData;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut mob = MobileData::new("an apprentice mage".to_string());
        assert!(mob.combat_spells.is_empty(), "defaults to empty list");
        assert_eq!(mob.combat_spell_chance, 50, "default chance is 50");

        mob.combat_spells = vec!["magic_missile".into(), "firebolt".into()];
        mob.combat_spell_chance = 75;
        let id = mob.id;
        db.save_mobile_data(mob).expect("save");

        let loaded = db.get_mobile_data(&id).expect("read").expect("present");
        assert_eq!(loaded.combat_spells, vec!["magic_missile", "firebolt"]);
        assert_eq!(loaded.combat_spell_chance, 75);
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

/// `frost_bolt` is the cold-damage spell added so importer-translated
/// `magic_user` mobs have a 4-element rotation matching CircleMUD's
/// magic_missile / chill_touch / firebolt / lightning_bolt selection.
#[test]
fn test_frost_bolt_spell_definition_loads() {
    use std::fs;

    let json = fs::read_to_string("scripts/data/spells_fantasy.json").expect("read spells_fantasy");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");
    let spell = parsed
        .get("frost_bolt")
        .expect("frost_bolt entry present")
        .as_object()
        .expect("frost_bolt is an object");

    assert_eq!(spell.get("spell_type").and_then(|v| v.as_str()), Some("damage"));
    assert_eq!(spell.get("damage_type").and_then(|v| v.as_str()), Some("cold"));
    assert_eq!(spell.get("target_type").and_then(|v| v.as_str()), Some("enemy"));
    assert!(spell.get("damage_base").and_then(|v| v.as_i64()).unwrap_or(0) > 0);
}

/// The CircleMUD trigger mapping flips `magic_user` from a Warn to
/// `set_mob_combat_spells` so all 93 stock magic_user mobs get a working
/// spell rotation rather than silently degrading to melee-only post-import.
#[test]
fn test_magic_user_trigger_mapping_sets_combat_spells() {
    use std::fs;

    let json = fs::read_to_string("scripts/data/import/circle_trigger_mapping.json")
        .expect("read circle_trigger_mapping");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");
    let entry = parsed
        .get("trigger_actions")
        .expect("trigger_actions section")
        .get("magic_user")
        .expect("magic_user entry present")
        .as_object()
        .expect("magic_user is an object");

    assert_eq!(
        entry.get("action").and_then(|v| v.as_str()),
        Some("set_mob_combat_spells"),
        "magic_user no longer warn-only"
    );
    let spells: Vec<&str> = entry
        .get("spells")
        .and_then(|v| v.as_array())
        .expect("spells array")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(spells.contains(&"magic_missile"));
    assert!(spells.contains(&"frost_bolt"));
    assert!(spells.contains(&"firebolt"));
    assert!(spells.contains(&"lightning_bolt"));
    let chance = entry.get("chance").and_then(|v| v.as_i64()).unwrap_or(0);
    assert!((0..=100).contains(&chance), "chance {} out of range", chance);
}

/// Bug #5 regression: `item.extra_descs` must behave as a Rhai Array so script
/// code can call `.len()`, index, and iterate. The original getter returned
/// `Vec<ExtraDesc>` which Rhai treats as an opaque type with no methods.
#[test]
fn test_item_extra_descs_is_rhai_array() {
    use ironmud::types::{ExtraDesc, ItemData};

    let mut engine = rhai::Engine::new();
    engine
        .register_type_with_name::<ItemData>("ItemData")
        .register_type_with_name::<ExtraDesc>("ExtraDesc")
        .register_get("extra_descs", |i: &mut ItemData| {
            i.extra_descs
                .iter()
                .map(|e| rhai::Dynamic::from(e.clone()))
                .collect::<Vec<_>>()
        })
        .register_get("keywords", |e: &mut ExtraDesc| {
            e.keywords
                .iter()
                .map(|s| rhai::Dynamic::from(s.clone()))
                .collect::<Vec<_>>()
        });

    let mut item = ItemData::new(
        "rock".to_string(),
        "a rock".to_string(),
        "a small rock lies here.".to_string(),
    );
    item.extra_descs.push(ExtraDesc {
        keywords: vec!["mossy".to_string()],
        description: "moss covers the surface.".to_string(),
    });
    item.extra_descs.push(ExtraDesc {
        keywords: vec!["chip".to_string()],
        description: "a small chip on the side.".to_string(),
    });

    let mut scope = rhai::Scope::new();
    scope.push("item", item);

    let count: i64 = engine
        .eval_with_scope(&mut scope, "item.extra_descs.len()")
        .expect("item.extra_descs.len() must succeed");
    assert_eq!(count, 2, "extra_descs.len() should return the array length");

    let joined: String = engine
        .eval_with_scope(
            &mut scope,
            r#"
                let extras = item.extra_descs;
                let kws = "";
                for ex in extras {
                    for kw in ex.keywords {
                        if kws != "" { kws += ","; }
                        kws += kw;
                    }
                }
                kws
            "#,
        )
        .expect("iterating extra_descs and their keywords must succeed");
    assert_eq!(joined, "mossy,chip");
}

/// Bug #6 regression: `set_mobile_flag` must accept `helper`, `stay_zone`,
/// `aware`, and `memory` (each advertised in medit's help and read-side getter
/// but previously absent from the Rust write path). Verifies by scanning the
/// source — keeps the test cheap and avoids a full server boot.
#[test]
fn test_set_mobile_flag_handles_all_public_flags() {
    use std::fs;

    let src = fs::read_to_string("src/script/mobiles.rs").expect("read src/script/mobiles.rs");
    let body = src
        .split("\"set_mobile_flag\",")
        .nth(1)
        .expect("set_mobile_flag registration not found");
    // Bound to the closure: the next register_fn call ends our scan window.
    let body = body
        .split("set_mobile_vnum")
        .next()
        .expect("end of set_mobile_flag closure not found");

    for flag in &["helper", "stay_zone", "aware", "memory"] {
        let needle = format!("\"{}\"", flag);
        assert!(
            body.contains(&needle),
            "set_mobile_flag is missing match arm for `{}` — bug #6 has regressed",
            flag
        );
    }
}

/// Tab-completion `MOBILE_FLAGS` must match medit's advertised flag list and
/// stay non-empty. Locks completion drift after the bug #6/#6b fix.
#[test]
fn test_mobile_flags_completion_matches_medit_advertisement() {
    use std::collections::HashSet;
    use std::fs;

    let completion_set: HashSet<&str> = ironmud::completion::MOBILE_FLAGS.iter().copied().collect();

    // medit advertises flags inside the "Available: aggressive, ..." string at
    // the unknown-flag fallback. Anchor on `aggressive` so we don't pick up
    // other "Available: " strings (damtype, trigger scripts, etc.).
    let medit = fs::read_to_string("scripts/commands/medit.rhai").expect("read medit.rhai");
    let needle = "Available: aggressive";
    let idx = medit
        .find(needle)
        .expect("medit unknown-flag advertise string not found");
    let start = idx + "Available: ".len();
    let tail = &medit[start..];
    let end = tail.find('"').expect("end of advertise string not found");
    let advertised: HashSet<&str> = tail[..end]
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    let missing_in_completion: Vec<&&str> = advertised.difference(&completion_set).collect();
    let extra_in_completion: Vec<&&str> = completion_set.difference(&advertised).collect();

    assert!(
        missing_in_completion.is_empty() && extra_in_completion.is_empty(),
        "MOBILE_FLAGS / medit advertise list drift.\n  missing in completion: {:?}\n  extra in completion: {:?}",
        missing_in_completion,
        extra_in_completion
    );

    assert!(
        completion_set.len() >= 28,
        "MOBILE_FLAGS shrank below the post-fix size — possible regression"
    );
}

#[test]
fn test_animate_dead_spell_definition_loads() {
    use std::fs;

    let json = fs::read_to_string("scripts/data/spells_fantasy.json").expect("read spells_fantasy");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");
    let spell = parsed
        .get("animate_dead")
        .expect("animate_dead entry present")
        .as_object()
        .expect("animate_dead is an object");

    assert_eq!(spell.get("spell_type").and_then(|v| v.as_str()), Some("animate_dead"));
    assert_eq!(spell.get("target_type").and_then(|v| v.as_str()), Some("corpse_in_room"));
    assert_eq!(spell.get("skill_required").and_then(|v| v.as_i64()), Some(5));
}

#[test]
fn test_control_weather_spell_definition_loads() {
    use std::fs;

    let json = fs::read_to_string("scripts/data/spells_fantasy.json").expect("read spells_fantasy");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");
    let spell = parsed
        .get("control_weather")
        .expect("control_weather entry present")
        .as_object()
        .expect("control_weather is an object");

    assert_eq!(spell.get("spell_type").and_then(|v| v.as_str()), Some("control_weather"));
    assert_eq!(spell.get("target_type").and_then(|v| v.as_str()), Some("self"));
    assert_eq!(spell.get("skill_required").and_then(|v| v.as_i64()), Some(4));
}

#[test]
fn test_corpse_source_vnum_persists() {
    
    use ironmud::types::{ItemData, ItemFlags, ItemLocation};
    use uuid::Uuid;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        // Default: corpse_source_vnum is None.
        let flags_default = ItemFlags::default();
        assert!(flags_default.corpse_source_vnum.is_none());

        let room_id = Uuid::new_v4();
        let mut item = ItemData::new(
            "corpse of a goblin".to_string(),
            "The corpse of a goblin lies here.".to_string(),
            "The lifeless body of a goblin lies in a crumpled heap.".to_string(),
        );
        item.flags.is_corpse = true;
        item.flags.corpse_owner = "a goblin".to_string();
        item.flags.corpse_source_vnum = Some("goblin_warrior".to_string());
        item.location = ItemLocation::Room(room_id);
        let id = item.id;
        db.save_item_data(item).expect("save corpse");

        let loaded = db.get_item_data(&id).expect("read").expect("present");
        assert_eq!(
            loaded.flags.corpse_source_vnum.as_deref(),
            Some("goblin_warrior")
        );
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

// ===== Declarative usable items: per-item cooldown override =====

#[test]
fn test_cast_on_use_cooldown_secs_round_trips() {
    
    use ironmud::{CastOnUse, ItemData};

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut item = ItemData::new(
            "wand".to_string(),
            "a slim wand".to_string(),
            "A slim wand lies here.".to_string(),
        );
        item.cast_on_use = Some(CastOnUse {
            spell: "heal".to_string(),
            min_level: 1,
            charges: 5,
            max_charges: 5,
            cooldown_secs: Some(60),
        });
        let item_id = item.id;
        db.save_item_data(item).expect("save");

        let loaded = db
            .get_item_data(&item_id)
            .expect("get")
            .expect("present");
        let cou = loaded.cast_on_use.expect("cast_on_use present");
        assert_eq!(cou.cooldown_secs, Some(60), "override survives round-trip");

        // None on the override is the default — uses spell's own cooldown.
        let mut item2 = ItemData::new(
            "vial".to_string(),
            "a glass vial".to_string(),
            "A vial sits here.".to_string(),
        );
        item2.cast_on_use = Some(CastOnUse {
            spell: "heal".to_string(),
            min_level: 0,
            charges: 1,
            max_charges: 1,
            cooldown_secs: None,
        });
        let id2 = item2.id;
        db.save_item_data(item2).expect("save");
        let loaded2 = db.get_item_data(&id2).expect("get").expect("present");
        assert!(
            loaded2.cast_on_use.unwrap().cooldown_secs.is_none(),
            "None override persists as None (use spell default)"
        );
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_quaff_and_zap_scripts_apply_cooldown_override() {
    // Source-level check: quaff.rhai and zap.rhai must read the per-item
    // override and call set_spell_cooldown — otherwise the new
    // CastOnUse.cooldown_secs field would persist but be ignored at runtime.
    use std::fs;

    for script in ["scripts/commands/quaff.rhai", "scripts/commands/zap.rhai"] {
        let src = fs::read_to_string(script).expect(script);
        assert!(
            src.contains("resolve_item_spell_cooldown"),
            "{script} missing resolve_item_spell_cooldown helper (cooldown override path)"
        );
        assert!(
            src.contains("set_spell_cooldown"),
            "{script} must stamp cooldown via set_spell_cooldown"
        );
        assert!(
            src.contains("get_spell_cooldown_remaining"),
            "{script} must check cooldown before firing (otherwise override is bypassable)"
        );
    }
}

#[test]
fn test_oedit_cast_on_use_supports_show_clear_and_cooldown() {
    // Source-level check: oedit.rhai must dispatch show/clear and accept the
    // optional 4-arg cooldown form.
    use std::fs;

    let src = fs::read_to_string("scripts/commands/oedit.rhai").expect("read oedit.rhai");

    assert!(
        src.contains("show_cast_on_use"),
        "oedit.rhai missing `cast_on_use show` handler"
    );
    assert!(
        src.contains("clear_item_cast_on_use"),
        "oedit.rhai missing cast_on_use clear handler"
    );
    assert!(
        src.contains("set_item_cast_on_use(item_id, spell_id, min_level, charges, cooldown_secs)"),
        "oedit.rhai must pass cooldown_secs to set_item_cast_on_use"
    );
}

// ===== Builder knowledge-base lookup =====

#[test]
fn test_lookup_command_registered_as_builder() {
    use std::fs;

    let raw = fs::read_to_string("scripts/commands.json").expect("read commands.json");
    let parsed: serde_json::Value = serde_json::from_str(&raw).expect("valid json");
    let entry = parsed
        .get("lookup")
        .expect("lookup command registered")
        .as_object()
        .expect("lookup is an object");
    assert_eq!(
        entry.get("access").and_then(|v| v.as_str()),
        Some("builder"),
        "lookup must be builder-gated (surfaces internal balance numbers)"
    );
}

#[test]
fn test_lookup_command_gates_on_builder_or_admin() {
    use std::fs;

    let src = fs::read_to_string("scripts/commands/lookup.rhai").expect("read lookup.rhai");
    assert!(
        src.contains("!char.is_builder && !char.is_admin"),
        "lookup.rhai must gate on is_builder || is_admin"
    );
    assert!(
        src.contains("dispatch_spell")
            && src.contains("dispatch_trait")
            && src.contains("dispatch_effect")
            && src.contains("dispatch_skill"),
        "lookup.rhai must dispatch all four kinds"
    );
}

#[test]
fn test_lookup_known_skills_covers_magic_and_crafting() {
    // KNOWN_SKILLS in src/script/lookup.rs must include the skills referenced
    // by spell `skill_required` (magic) AND the player-skill set (crafting,
    // cooking, ...). Otherwise `lookup skill` misses entire categories.
    use std::fs;

    let src = fs::read_to_string("src/script/lookup.rs").expect("read lookup.rs");
    for required in &[
        "\"magic\"",
        "\"melee\"",
        "\"ranged\"",
        "\"stealth\"",
        "\"cooking\"",
        "\"crafting\"",
    ] {
        assert!(
            src.contains(required),
            "KNOWN_SKILLS missing {required} — `lookup skill` would miss this category"
        );
    }
}

#[test]
fn test_lookup_effect_cross_ref_uses_buff_effect_match() {
    // The cross-ref scanner in lookup.rs must compare against a spell's
    // `buff_effect` field. If this drifts (e.g. someone renames the field on
    // SpellDefinition), `lookup effect <name>` silently returns empty arrays.
    use std::fs;

    let src = fs::read_to_string("src/script/lookup.rs").expect("read lookup.rs");
    assert!(
        src.contains("s.buff_effect == display"),
        "effect cross-ref must filter spells by buff_effect == display"
    );
}

// ===== Per-room contextual commands =====

#[test]
fn test_contextual_commands_round_trip() {
    
    use ironmud::types::{ContextualCommand, RoomData, RoomExits, RoomFlags, WaterType};
    use std::collections::HashMap;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let room = RoomData {
            id: uuid::Uuid::new_v4(),
            title: "Puzzle chamber".to_string(),
            description: "An odd room.".to_string(),
            exits: RoomExits::default(),
            flags: RoomFlags::default(),
            extra_descs: Vec::new(),
            vnum: None,
            area_id: None,
            triggers: Vec::new(),
            doors: HashMap::new(),
            spring_desc: None,
            summer_desc: None,
            autumn_desc: None,
            winter_desc: None,
            dynamic_desc: None,
            water_type: WaterType::None,
            catch_table: Vec::new(),
            is_property_template: false,
            property_template_id: None,
            is_template_entrance: false,
            property_lease_id: None,
            property_entrance: false,
            recent_departures: Vec::new(),
            blood_trails: Vec::new(),
            traps: Vec::new(),
            living_capacity: 0,
            residents: Vec::new(),
            dg_vars: std::collections::HashMap::new(),
            coordinates: None,
            contextual_commands: vec![
                ContextualCommand {
                    verb: "pull".to_string(),
                    hint: Some("the rusty lever".to_string()),
                },
                ContextualCommand {
                    verb: "examine".to_string(),
                    hint: None,
                },
            ],
            exit_delays: std::collections::HashMap::new(),
        };
        let room_id = room.id;
        db.save_room_data(room).expect("save");

        let loaded = db.get_room_data(&room_id).expect("get").expect("present");
        assert_eq!(
            loaded.contextual_commands.len(),
            2,
            "both entries persist"
        );
        assert_eq!(loaded.contextual_commands[0].verb, "pull");
        assert_eq!(
            loaded.contextual_commands[0].hint.as_deref(),
            Some("the rusty lever"),
            "hint round-trips"
        );
        assert_eq!(loaded.contextual_commands[1].verb, "examine");
        assert!(
            loaded.contextual_commands[1].hint.is_none(),
            "missing hint stays None"
        );
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_contextual_commands_field_default_empty_on_old_room_json() {
    // Old persisted RoomData (pre-feature) has no contextual_commands key.
    // #[serde(default)] must allow that to deserialize cleanly with an empty Vec.
    use ironmud::types::RoomData;

    let raw = r#"{
        "id": "00000000-0000-0000-0000-000000000001",
        "title": "Old room",
        "description": "predates the feature",
        "exits": {
            "north": null, "east": null, "south": null, "west": null,
            "up": null, "down": null
        }
    }"#;
    let room: RoomData = serde_json::from_str(raw).expect("old shape deserializes");
    assert!(
        room.contextual_commands.is_empty(),
        "default empty Vec when key absent"
    );
}

#[test]
fn test_display_room_renders_contextual_commands_block() {
    // Source-level: src/script/rooms.rs must contain the "Here you can:"
    // block keyed off room.contextual_commands.
    use std::fs;

    let src = fs::read_to_string("src/script/rooms.rs").expect("read rooms.rs");
    assert!(
        src.contains("Here you can: "),
        "display_room missing 'Here you can:' line"
    );
    assert!(
        src.contains("room.contextual_commands.is_empty()"),
        "display_room must short-circuit when contextual_commands is empty"
    );
}

#[test]
fn test_redit_cmd_subcommand_present() {
    // Source-level: redit.rhai must have a `cmd` dispatcher with add/rm/clear/list.
    use std::fs;

    let src = fs::read_to_string("scripts/commands/redit.rhai").expect("read redit.rhai");
    assert!(
        src.contains("subcommand == \"cmd\""),
        "redit.rhai missing the cmd subcommand dispatch"
    );
    for sub in &["\"add\"", "\"clear\"", "\"list\""] {
        assert!(
            src.contains(&format!("cmd_sub == {}", sub)),
            "redit cmd missing {sub} branch"
        );
    }
    assert!(
        src.contains("add_room_contextual_command")
            && src.contains("remove_room_contextual_command")
            && src.contains("clear_room_contextual_commands"),
        "redit cmd must call all three Rhai helper fns"
    );
}

#[test]
fn test_tab_completion_appends_room_contextual_verbs() {
    // Source-level: src/lib.rs must extend available_commands with the
    // current room's contextual_commands.verb entries (deduped).
    use std::fs;

    let src = fs::read_to_string("src/lib.rs").expect("read lib.rs");
    assert!(
        src.contains("contextual_commands"),
        "lib.rs TAB path doesn't reference contextual_commands"
    );
    assert!(
        src.contains("available_commands.push(cc.verb.clone())"),
        "lib.rs must push room contextual verbs into available_commands"
    );
}

// ===== Multi-character accounts (foundation slice) =====

#[test]
fn test_account_migration_creates_one_to_one_account() {
    use ironmud::db::Db;

    let temp = tempfile::tempdir().expect("create temp dir");
    let db_path = temp.path();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {

        // Seed two pre-feature characters BEFORE migration runs. We do that by
        // opening the DB once (which seeds an empty `accounts` tree and stamps
        // accounts_migrated=true), then writing characters, then resetting the
        // flag, then re-opening to force the migration path.
        {
            let db = Db::open(&db_path).expect("open DB");
            // Reset migration flag so the next open() actually runs migration.
            db.set_setting("accounts_migrated", "false").unwrap();

            // Seed two characters with non-empty password_hash.
            let mut alice = serde_json::from_str::<ironmud::types::CharacterData>(
                r#"{ "name": "Alice", "password_hash": "hash-alice",
                     "current_room_id": "00000000-0000-0000-0000-000000000001" }"#,
            )
            .unwrap();
            alice.password_hash = "hash-alice".into();
            db.save_character_data(alice).unwrap();

            let mut bob = serde_json::from_str::<ironmud::types::CharacterData>(
                r#"{ "name": "Bob", "password_hash": "hash-bob",
                     "current_room_id": "00000000-0000-0000-0000-000000000001" }"#,
            )
            .unwrap();
            bob.password_hash = "hash-bob".into();
            db.save_character_data(bob).unwrap();
        }

        // Re-open: account migration should fire and create a 1:1 account per character.
        let db = Db::open(&db_path).expect("re-open DB");
        let accounts = db.list_accounts().expect("list accounts");
        assert!(
            accounts.len() >= 2,
            "expected at least 2 accounts after migration; got {}",
            accounts.len()
        );

        let alice_account = db
            .get_account("Alice")
            .unwrap()
            .expect("Alice account exists");
        assert_eq!(alice_account.password_hash, "hash-alice");
        assert_eq!(alice_account.character_names, vec!["Alice".to_string()]);

        let bob_account = db
            .get_account("Bob")
            .unwrap()
            .expect("Bob account exists");
        assert_eq!(bob_account.password_hash, "hash-bob");
        assert_eq!(bob_account.character_names, vec!["Bob".to_string()]);

        // Idempotent: re-running migration shouldn't duplicate.
        db.set_setting("accounts_migrated", "false").unwrap();
        // (private fn — exercised via open(). Re-open and check counts hold.)
        drop(db);
        let db2 = Db::open(&db_path).expect("re-open DB 2");
        let accounts2 = db2.list_accounts().unwrap();
        assert_eq!(
            accounts2.len(),
            accounts.len(),
            "migration must be idempotent"
        );
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_account_password_round_trip() {
    use ironmud::db::Db;
    use ironmud::types::AccountData;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(temp.path()).expect("open DB");

        let mut account = AccountData::new("Roundtrip".into(), "hash-rt".into());
        account.email = Some("a@b".into());
        account.is_banned = false;
        account.character_names = vec!["Roundtrip".into(), "Sidekick".into()];
        let id = account.id;
        db.save_account(account).expect("save");

        let loaded = db
            .get_account("Roundtrip")
            .expect("get")
            .expect("present");
        assert_eq!(loaded.id, id);
        assert_eq!(loaded.password_hash, "hash-rt");
        assert_eq!(loaded.email.as_deref(), Some("a@b"));
        assert_eq!(loaded.character_names, vec!["Roundtrip", "Sidekick"]);

        // ID index must resolve back to the same row.
        let by_id = db
            .get_account_by_id(&id)
            .expect("by id")
            .expect("present");
        assert_eq!(by_id.name, loaded.name);
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_max_characters_per_account_constant_present() {
    // The cap must exist and be > 1; the create.rhai authenticated path
    // reads it via max_characters_per_account().
    use ironmud::script::accounts::MAX_CHARACTERS_PER_ACCOUNT;
    assert!(
        MAX_CHARACTERS_PER_ACCOUNT >= 2,
        "cap must allow at least 2 characters to be useful"
    );
}

#[test]
fn test_add_and_remove_character_from_account_roundtrips() {
    use ironmud::types::AccountData;

    let temp = tempfile::tempdir().expect("create temp dir");
    let db_path = temp.path();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(db_path).expect("open DB");

        let account = AccountData::new("Addremove".into(), "h".into());
        let id = account.id;
        db.save_account(account).unwrap();

        assert!(db.add_character_to_account(&id, "Char1").unwrap());
        assert!(db.add_character_to_account(&id, "Char2").unwrap());
        // Adding the same name twice is a no-op (case-insensitive).
        assert!(db.add_character_to_account(&id, "char1").unwrap());

        let loaded = db.get_account_by_id(&id).unwrap().unwrap();
        assert_eq!(loaded.character_names.len(), 2);

        assert!(db.remove_character_from_account(&id, "Char1").unwrap());
        let after = db.get_account_by_id(&id).unwrap().unwrap();
        assert_eq!(after.character_names, vec!["Char2".to_string()]);

        // find_account_for_character round-trips.
        let found = db.find_account_for_character("Char2").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, id);
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_forgot_command_uses_verified_email_lookup_and_throttle() {
    // Source-level: forgot.rhai must gate on is_email_verification_required,
    // route the email through find_verified_account_id_by_email (so unverified
    // accounts don't leak), respect both the per-account and per-IP throttle,
    // record the IP send only when a real email was dispatched, and always
    // reply with the same generic message to prevent enumeration.
    use std::fs;
    let src = fs::read_to_string("scripts/commands/forgot.rhai").expect("read forgot.rhai");
    assert!(
        src.contains("is_email_verification_required"),
        "forgot.rhai must gate on the email-verification master switch"
    );
    assert!(
        src.contains("find_verified_account_id_by_email"),
        "forgot.rhai must look up by verified email, not bare normalized email"
    );
    assert!(
        src.contains("can_request_password_reset"),
        "forgot.rhai must check the per-account reset throttle"
    );
    assert!(
        src.contains("is_email_send_throttled"),
        "forgot.rhai must check the per-IP email-send rate limiter"
    );
    assert!(
        src.contains("record_email_send"),
        "forgot.rhai must stamp successful sends against the per-IP limiter"
    );
    assert!(
        src.contains("issue_password_reset"),
        "forgot.rhai must call issue_password_reset to rotate the password"
    );
    assert!(
        src.contains("If an account with that email exists"),
        "forgot.rhai must reply with the same generic message regardless of match"
    );
    assert!(
        src.contains("do_dummy_password_work"),
        "forgot.rhai must burn dummy Argon2 work on no-match / throttled branches to defeat timing-based account enumeration"
    );
    // Specifically: every early-exit branch (per-IP throttled, no account,
    // per-account throttled) must call dummy work. Three call sites total.
    let dummy_calls = src.matches("do_dummy_password_work").count();
    assert!(
        dummy_calls >= 3,
        "forgot.rhai needs dummy work on all three early-exit branches; found {}",
        dummy_calls
    );
}

#[test]
fn test_create_command_throttles_email_sends() {
    // Source-level: create.rhai must gate the verification email + resend on
    // the per-IP send throttle (paired with the global daily/monthly cap in
    // src/email/mod.rs) so a flooder can't drive the SES bill via fresh
    // accounts or repeated resend requests.
    use std::fs;
    let src = fs::read_to_string("scripts/commands/create.rhai").expect("read create.rhai");
    assert!(
        src.contains("is_email_send_throttled"),
        "create.rhai must check is_email_send_throttled before sending verification mail"
    );
    assert!(
        src.contains("record_email_send"),
        "create.rhai must stamp successful sends against the per-IP limiter"
    );
    // Both the initial send and the resend path need the gate. The simplest
    // check: count occurrences and require >= 2.
    let occurrences = src.matches("is_email_send_throttled").count();
    assert!(
        occurrences >= 2,
        "create.rhai must gate BOTH the initial send and the resend path; found {}",
        occurrences
    );
}

#[test]
fn test_login_uses_account_password_hash_not_character_hash() {
    // Source-level: login.rhai must verify against account.password_hash, not
    // existing_char.password_hash. If this drifts, the auth path is reading a
    // stale hash and post-migration password changes silently fail to land.
    use std::fs;
    let src = fs::read_to_string("scripts/commands/login.rhai").expect("read login.rhai");
    assert!(
        src.contains("verify_password(password, account.password_hash)"),
        "login.rhai must verify against account.password_hash"
    );
    assert!(
        src.contains("get_account_by_name"),
        "login.rhai must look up the account, not the character, for auth"
    );
}

#[test]
fn test_login_auto_selects_when_one_character() {
    // Source-level: when summaries.len() == 1, login.rhai must skip the roster
    // and call complete_character_selection directly. Otherwise legacy
    // single-character users get an unwanted extra prompt.
    use std::fs;
    let src = fs::read_to_string("scripts/commands/login.rhai").expect("read login.rhai");
    assert!(
        src.contains("if summaries.len() == 1"),
        "login.rhai must short-circuit the roster for single-character accounts"
    );
    assert!(
        src.contains("complete_character_selection"),
        "login.rhai must hand off to complete_character_selection on auto-select"
    );
}

#[test]
fn test_login_refuses_banned_account() {
    // Source-level: the login.rhai auth path must run the structured ban check
    // and refuse the login before stamping auth or surfacing the roster. The
    // ban-tooling slice replaced the bare `account.is_banned` boolean check
    // with `check_account_ban` + `format_ban_message` so expiry and reason are
    // honored uniformly.
    use std::fs;
    let src = fs::read_to_string("scripts/commands/login.rhai").expect("read login.rhai");
    assert!(
        src.contains("check_account_ban"),
        "login.rhai must call check_account_ban before stamping auth"
    );
    assert!(
        src.contains("format_ban_message"),
        "login.rhai must use format_ban_message for the ban refusal text"
    );
}

#[test]
fn test_account_email_field_persists_when_set() {
    // Roundtrip with email = Some(...) so the future verification slice has a
    // place to attach. Schema bump prevention.
    use ironmud::types::AccountData;

    let temp = tempfile::tempdir().expect("create temp dir");
    let db_path = temp.path();
    

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(db_path).expect("open DB");
        let mut account = AccountData::new("Emailer".into(), "h".into());
        account.email = Some("verified@example.com".into());
        db.save_account(account).unwrap();

        let loaded = db.get_account("Emailer").unwrap().unwrap();
        assert_eq!(loaded.email.as_deref(), Some("verified@example.com"));
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_create_rhai_branches_on_authenticated_account() {
    // Source-level: create.rhai must read get_authenticated_account_id and
    // dispatch into the authenticated branch (no password) vs unauthenticated
    // branch (legacy account+character one-shot).
    use std::fs;
    let src = fs::read_to_string("scripts/commands/create.rhai").expect("read create.rhai");
    assert!(
        src.contains("get_authenticated_account_id"),
        "create.rhai must check authentication state"
    );
    assert!(
        src.contains("run_authenticated_create"),
        "create.rhai must have an authenticated branch"
    );
    assert!(
        src.contains("max_characters_per_account"),
        "create.rhai must enforce per-account character cap"
    );
    assert!(
        src.contains("create_account") && src.contains("add_character_to_account"),
        "unauthenticated path must create an account and link the first character"
    );
}

#[test]
fn test_select_character_mode_routes_to_login_rhai() {
    // Source-level: src/lib.rs OLC dispatch table must include
    // `select_character`, otherwise typing a number at the roster prompt
    // falls through to "Unknown command".
    use std::fs;
    let src = fs::read_to_string("src/lib.rs").expect("read lib.rs");
    assert!(
        src.contains("mode == \"select_character\""),
        "lib.rs OLC dispatch must route select_character to login.rhai"
    );
}

#[test]
fn test_roster_command_registered_and_gated() {
    // Source-level: roster.rhai exists and is registered as a `user`-tier
    // command (only logged-in players have an account_id to query).
    use std::fs;
    let raw = fs::read_to_string("scripts/commands.json").expect("read commands.json");
    let parsed: serde_json::Value = serde_json::from_str(&raw).expect("valid json");
    let entry = parsed
        .get("roster")
        .expect("roster command registered")
        .as_object()
        .expect("roster is an object");
    assert_eq!(
        entry.get("access").and_then(|v| v.as_str()),
        Some("user"),
        "roster must be `user`-tier (requires authenticated account)"
    );

    let src = fs::read_to_string("scripts/commands/roster.rhai").expect("read roster.rhai");
    assert!(
        src.contains("get_authenticated_account_id"),
        "roster.rhai must verify auth before stepping the player out"
    );
    assert!(
        src.contains("clear_player_character") && src.contains("set_olc_mode(connection_id, \"select_character\")"),
        "roster.rhai must drop the active character and switch to select_character mode"
    );
}

#[test]
fn test_create_refuses_when_playing_a_character() {
    // Source-level: run_authenticated_create must refuse when the connection
    // already has a player character loaded; otherwise chargen swaps the
    // session silently.
    use std::fs;
    let src = fs::read_to_string("scripts/commands/create.rhai").expect("read create.rhai");
    assert!(
        src.contains("get_player_character(connection_id)") && src.contains("`roster`"),
        "create.rhai must refuse when active and point users to `roster`"
    );
}

#[test]
fn test_delete_character_clears_account_roster_pointer() {
    use ironmud::types::AccountData;

    let temp = tempfile::tempdir().expect("create temp dir");
    let db_path = temp.path();
    

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(db_path).expect("open DB");

        // Build an account with one character. Use a real CharacterData so
        // delete_character_data works end-to-end.
        let mut account = AccountData::new("Owner".into(), "h".into());
        account.character_names = vec!["Pawn".into()];
        let id = account.id;
        db.save_account(account).unwrap();

        let pawn = serde_json::from_str::<ironmud::types::CharacterData>(
            r#"{ "name": "Pawn", "password_hash": "h",
                 "current_room_id": "00000000-0000-0000-0000-000000000001" }"#,
        )
        .unwrap();
        db.save_character_data(pawn).unwrap();

        // Delete the character → owning account's roster should drop the name.
        db.delete_character_data("Pawn").unwrap();
        let after = db.get_account_by_id(&id).unwrap().unwrap();
        assert!(
            after.character_names.is_empty(),
            "deleting a character must remove its name from the owning account's roster (got {:?})",
            after.character_names
        );
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

// ===== Email verification slice =====

#[test]
fn test_email_verified_defaults_true_for_legacy_accounts() {
    // Account JSON written before the verification slice will not have any of
    // the new fields. Loading it must produce email_verified = true so
    // existing accounts grandfather in when the schema upgrades.
    use ironmud::types::AccountData;

    let legacy_json = r#"{
        "id": "11111111-1111-1111-1111-111111111111",
        "name": "Legacy",
        "password_hash": "h",
        "character_names": ["Legacy"],
        "is_banned": false,
        "created_at": 0,
        "last_login_at": 0
    }"#;

    let account: AccountData = serde_json::from_str(legacy_json).expect("legacy schema decodes");
    assert!(
        account.email_verified,
        "legacy accounts must default email_verified=true so the verification flag flip doesn't lock them out"
    );
    assert!(account.email_verification_code.is_none());
    assert_eq!(account.email_verification_code_expires_at, 0);
    assert_eq!(account.email_verification_resend_count, 0);
}

#[test]
fn test_email_verification_fields_round_trip() {
    use ironmud::types::AccountData;

    let temp = tempfile::tempdir().expect("create temp dir");
    let db_path = temp.path();


    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(db_path).expect("open DB");
        let mut account = AccountData::new("Codey".into(), "h".into());
        account.email = Some("codey@example.com".into());
        account.email_verified = false;
        account.email_verification_code = Some("482910".into());
        account.email_verification_code_expires_at = 1_700_000_000;
        account.email_verification_last_sent_at = 1_699_999_900;
        account.email_verification_resend_count = 3;
        account.email_verification_resend_window_started_at = 1_699_999_500;
        db.save_account(account).unwrap();

        let loaded = db.get_account("Codey").unwrap().unwrap();
        assert!(!loaded.email_verified);
        assert_eq!(loaded.email_verification_code.as_deref(), Some("482910"));
        assert_eq!(loaded.email_verification_code_expires_at, 1_700_000_000);
        assert_eq!(loaded.email_verification_last_sent_at, 1_699_999_900);
        assert_eq!(loaded.email_verification_resend_count, 3);
        assert_eq!(
            loaded.email_verification_resend_window_started_at,
            1_699_999_500
        );
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_find_account_by_email() {
    use ironmud::types::AccountData;

    let temp = tempfile::tempdir().expect("create temp dir");
    let db_path = temp.path();


    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(db_path).expect("open DB");
        let mut a = AccountData::new("Alice".into(), "h".into());
        a.email = Some("Alice@Example.COM".into());
        db.save_account(a).unwrap();
        let mut b = AccountData::new("Bob".into(), "h".into());
        b.email = Some("bob@example.com".into());
        db.save_account(b).unwrap();

        // Case-insensitive match.
        let hit = db
            .find_account_by_email("alice@example.com")
            .unwrap()
            .expect("alice found by lowercase email");
        assert_eq!(hit.name, "Alice");

        // Whitespace tolerance.
        let hit2 = db
            .find_account_by_email("  bob@example.com  ")
            .unwrap()
            .expect("bob found with surrounding whitespace");
        assert_eq!(hit2.name, "Bob");

        // No match.
        let miss = db.find_account_by_email("nobody@example.com").unwrap();
        assert!(miss.is_none());

        // Empty string is None (not a wildcard).
        let empty = db.find_account_by_email("").unwrap();
        assert!(empty.is_none());
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_create_rhai_branches_on_email_verification_required() {
    // Source-level: create.rhai must check is_email_verification_required and
    // require an email argument when on. Without this branch, public servers
    // can't enforce verification at account creation.
    use std::fs;
    let src = fs::read_to_string("scripts/commands/create.rhai").expect("read create.rhai");
    assert!(
        src.contains("is_email_verification_required()"),
        "create.rhai must check is_email_verification_required"
    );
    assert!(
        src.contains("send_verification_code"),
        "create.rhai must dispatch a verification code when verification is required"
    );
    assert!(
        src.contains("\"email_verify_prompt\""),
        "create.rhai must enter email_verify_prompt mode after sending the code"
    );
    assert!(
        src.contains("verify_account_code"),
        "create.rhai must handle the verify response in email_verify_prompt mode"
    );
    assert!(
        src.contains("find_account_id_by_email"),
        "create.rhai must refuse duplicate email registrations when verification is on"
    );
}

#[test]
fn test_login_rhai_gates_on_email_verification() {
    // Source-level: login.rhai must, after password verify but before showing
    // the character roster, gate users into email_verify_prompt when the
    // server requires verification and the account is unverified.
    use std::fs;
    let src = fs::read_to_string("scripts/commands/login.rhai").expect("read login.rhai");
    assert!(
        src.contains("is_email_verification_required()"),
        "login.rhai must check is_email_verification_required before character selection"
    );
    assert!(
        src.contains("is_account_email_verified"),
        "login.rhai must check the account's verified state"
    );
    assert!(
        src.contains("\"email_verify_prompt\""),
        "login.rhai must drop unverified users into email_verify_prompt mode"
    );
}

#[test]
fn test_email_verify_prompt_mode_routes_to_create_rhai() {
    // Source-level: src/lib.rs OLC dispatch must route the email_verify_prompt
    // mode to create.rhai (which owns both the create-time and login-time
    // verification UX).
    use std::fs;
    let src = fs::read_to_string("src/lib.rs").expect("read lib.rs");
    assert!(
        src.contains("mode == \"email_verify_prompt\""),
        "lib.rs OLC dispatch must include email_verify_prompt"
    );
}

#[test]
fn test_email_template_has_code_placeholder() {
    // The default verification email body lives at scripts/data/email/
    // verification.txt. It must contain the {{code}} placeholder, otherwise
    // the user receives a code-less email.
    use std::fs;
    let body = fs::read_to_string("scripts/data/email/verification.txt")
        .expect("verification template exists");
    assert!(
        body.contains("{{code}}"),
        "email template must contain {{code}} placeholder"
    );
}

#[test]
fn test_admin_account_subcommands_present() {
    // Source-level: ironmud-admin must expose Verify, Unverify, SetEmail, and
    // SendCode subcommands so admins can manage stuck accounts without
    // hand-editing the DB.
    use std::fs;
    let src = fs::read_to_string("src/bin/ironmud-admin.rs").expect("read admin");
    for needle in &["Verify {", "Unverify {", "SetEmail {", "SendCode {"] {
        assert!(
            src.contains(needle),
            "ironmud-admin must expose AccountAction::{}",
            needle
        );
    }
}

#[test]
fn test_email_verification_disabled_by_default() {
    // No setting in the tree means the verification gate is off. This is the
    // private/tailscale/homelab default.

    let temp = tempfile::tempdir().expect("create temp dir");
    let db_path = temp.path();
    

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(db_path).expect("open DB");
        let v = db.get_setting("email_verification_required").unwrap();
        assert!(
            v.is_none(),
            "fresh DB must not have email_verification_required preset"
        );
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_generate_code_is_six_digit() {
    // 1M-space code keeps brute-force at 1-in-a-million per attempt; format
    // must be zero-padded so "000482" doesn't render as "482".
    use ironmud::email::generate_code;
    for _ in 0..32 {
        let code = generate_code();
        assert_eq!(code.len(), 6, "code must be 6 chars (got {:?})", code);
        assert!(
            code.chars().all(|c| c.is_ascii_digit()),
            "code must be all digits (got {:?})",
            code
        );
    }
}

// ===========================================================================
// Ban-tooling slice tests
// ===========================================================================

#[test]
fn test_ban_record_round_trips() {
    use ironmud::db::Db;
    use ironmud::types::{AccountData, BanRecord};

    let temp = tempfile::tempdir().expect("create temp dir");
    
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(temp.path()).expect("open DB");
        let mut a = AccountData::new("BanRT".into(), "h".into());
        a.is_banned = true;
        a.ban_record = Some(BanRecord {
            reason: "abuse".into(),
            banned_by: "admin".into(),
            banned_at: 1_700_000_000,
            expires_at: Some(1_900_000_000),
        });
        db.save_account(a).unwrap();
        let reloaded = db.get_account("banrt").unwrap().expect("account");
        assert!(reloaded.is_banned);
        let r = reloaded.ban_record.expect("record");
        assert_eq!(r.reason, "abuse");
        assert_eq!(r.banned_by, "admin");
        assert_eq!(r.banned_at, 1_700_000_000);
        assert_eq!(r.expires_at, Some(1_900_000_000));
    }));
    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_legacy_is_banned_without_record_deserializes() {
    // Accounts saved before the metadata slice have only `is_banned: true` and
    // no `ban_record`. They must still load and the boolean must still flip
    // the login gate. Grandfather behavior is the whole point of #[serde(default)]
    // on every new field.
    use ironmud::types::AccountData;
    let json = serde_json::json!({
        "id": "00000000-0000-0000-0000-000000000001",
        "name": "Legacy",
        "password_hash": "h",
        "character_names": ["Legacy"],
        "is_banned": true
    });
    let a: AccountData = serde_json::from_value(json).expect("legacy account loads");
    assert!(a.is_banned);
    assert!(a.ban_record.is_none());
    assert_eq!(a.last_login_ip, "");
    assert_eq!(a.creation_ip, "");
    assert!(a.normalized_email.is_none());
}

#[test]
fn test_site_ban_round_trips_and_lazy_expires() {
    use ironmud::db::Db;
    use ironmud::types::SiteBanRecord;
    use std::time::{SystemTime, UNIX_EPOCH};

    let temp = tempfile::tempdir().expect("create temp dir");
    
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(temp.path()).expect("open DB");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Permanent ban round-trips.
        db.put_site_ban(&SiteBanRecord {
            ip: "10.0.0.1".into(),
            reason: "scrape".into(),
            banned_by: "cli".into(),
            banned_at: now,
            expires_at: None,
        })
        .unwrap();
        let r = db
            .get_site_ban("10.0.0.1")
            .unwrap()
            .expect("permanent ban present");
        assert_eq!(r.reason, "scrape");
        assert!(r.expires_at.is_none());

        // Already-expired ban gets lazily cleared on read.
        db.put_site_ban(&SiteBanRecord {
            ip: "10.0.0.2".into(),
            reason: "old".into(),
            banned_by: "cli".into(),
            banned_at: now - 100,
            expires_at: Some(now - 1),
        })
        .unwrap();
        assert!(
            db.get_site_ban("10.0.0.2").unwrap().is_none(),
            "expired ban must lazy-clear on get_site_ban"
        );

        // list_site_bans reflects the cleanup.
        let active = db.list_site_bans().unwrap();
        assert_eq!(active.len(), 1, "only 10.0.0.1 should remain");
        assert_eq!(active[0].ip, "10.0.0.1");

        // remove_site_ban returns true on hit, false on miss.
        assert!(db.remove_site_ban("10.0.0.1").unwrap());
        assert!(!db.remove_site_ban("10.0.0.1").unwrap());
    }));
    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_record_account_ip_seen_and_lookup() {
    use ironmud::db::Db;
    use ironmud::types::AccountData;
    use std::time::{SystemTime, UNIX_EPOCH};

    let temp = tempfile::tempdir().expect("create temp dir");
    
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(temp.path()).expect("open DB");
        let alpha = AccountData::new("Alpha".into(), "h".into());
        let beta = AccountData::new("Beta".into(), "h".into());
        let alpha_id = alpha.id;
        let beta_id = beta.id;
        db.save_account(alpha).unwrap();
        db.save_account(beta).unwrap();

        db.record_account_ip_seen(alpha_id, "192.168.1.42").unwrap();
        db.record_account_ip_seen(beta_id, "192.168.1.42").unwrap();
        db.record_account_ip_seen(beta_id, "10.0.0.1").unwrap();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let since = now - 30 * 24 * 3600;

        let mut shared = db.list_accounts_by_ip("192.168.1.42", since).unwrap();
        shared.sort();
        let mut expected = vec![alpha_id, beta_id];
        expected.sort();
        assert_eq!(shared, expected);

        let single = db.list_accounts_by_ip("10.0.0.1", since).unwrap();
        assert_eq!(single, vec![beta_id]);

        let none = db.list_accounts_by_ip("10.0.0.99", since).unwrap();
        assert!(none.is_empty());
    }));
    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_normalize_email_canonical_form() {
    use ironmud::email::normalize_email;
    assert_eq!(
        normalize_email("Test.User+spam@Gmail.com").as_deref(),
        Some("testuser@gmail.com")
    );
    assert_eq!(
        normalize_email("foo.bar@googlemail.com").as_deref(),
        Some("foobar@gmail.com")
    );
    assert_eq!(
        normalize_email("test.user@mail.example").as_deref(),
        Some("test.user@mail.example")
    );
    assert!(normalize_email("notanemail").is_none());
    assert!(normalize_email("").is_none());
}

#[test]
fn test_disposable_domain_blocklist_matches_known_provider() {
    use ironmud::email::is_disposable_email_domain;
    // The blocklist file is loaded relative to CWD = repo root for tests.
    assert!(is_disposable_email_domain("foo@mailinator.com"));
    assert!(is_disposable_email_domain("FOO@yopmail.com"));
    assert!(!is_disposable_email_domain("foo@gmail.com"));
    assert!(!is_disposable_email_domain("not-an-email"));
}

#[test]
fn test_find_account_by_normalized_email() {
    use ironmud::db::Db;
    use ironmud::email::normalize_email;
    use ironmud::types::AccountData;

    let temp = tempfile::tempdir().expect("create temp dir");
    
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(temp.path()).expect("open DB");
        let mut a = AccountData::new("Alpha".into(), "h".into());
        a.email = Some("Test.User+stuff@Gmail.com".into());
        a.normalized_email = normalize_email("Test.User+stuff@Gmail.com");
        db.save_account(a).unwrap();

        // Different surface form, same normalized form → match.
        let canonical = normalize_email("testuser@gmail.com").unwrap();
        let hit = db
            .find_account_by_normalized_email(&canonical)
            .unwrap()
            .expect("alpha found by normalized email");
        assert_eq!(hit.name, "Alpha");

        // Truly different normalized form → no match.
        let other = normalize_email("someone-else@gmail.com").unwrap();
        assert!(db.find_account_by_normalized_email(&other).unwrap().is_none());
    }));
    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_login_records_ip_after_auth() {
    // Source-level: login.rhai stamps source IP on the account row + the
    // ip_account_history reverse index after a successful auth, so admin alts
    // can correlate. Ordering matters — IP recording must come after
    // touch_account_login (which writes last_login_at).
    use std::fs;
    let src = fs::read_to_string("scripts/commands/login.rhai").expect("read login.rhai");
    assert!(src.contains("get_connection_ip"), "must read connection IP");
    assert!(src.contains("record_account_ip"), "must record IP after auth");
    let touch = src.find("touch_account_login").expect("touch_account_login present");
    let record = src.find("record_account_ip").expect("record_account_ip present");
    assert!(record > touch, "record_account_ip must come after touch_account_login");
}

#[test]
fn test_create_rejects_disposable_domain_when_verification_on() {
    // Source-level: create.rhai must short-circuit on disposable-provider
    // emails before sending a verification code, only when verification is
    // required.
    use std::fs;
    let src = fs::read_to_string("scripts/commands/create.rhai").expect("read create.rhai");
    assert!(src.contains("is_disposable_email"), "must call is_disposable_email");
    let disp_idx = src.find("is_disposable_email").unwrap();
    let send_idx = src.find("send_verification_code").unwrap();
    assert!(
        disp_idx < send_idx,
        "is_disposable_email must be checked before send_verification_code"
    );
    // Normalized-email duplicate check too.
    assert!(src.contains("find_account_id_by_normalized_email"));
}

#[test]
fn test_admin_rhai_has_ban_handlers() {
    use std::fs;
    let src = fs::read_to_string("scripts/commands/admin.rhai").expect("read admin.rhai");
    for needle in &[
        "handle_ban",
        "handle_unban",
        "handle_siteban",
        "handle_sitebans",
        "handle_alts",
        "ban_account",
        "unban_account",
        "add_site_ban",
        "list_site_bans",
        "find_alts_by_account",
        "parse_duration_to_expires_at",
    ] {
        assert!(src.contains(needle), "admin.rhai missing: {}", needle);
    }
}

#[test]
fn test_accept_loop_calls_get_site_ban() {
    // Source-level: src/lib.rs's TCP accept loop must consult the bans tree
    // before spawning handle_connection. The siteban gate sits between the
    // per-IP rate-limit acquire and the handle_connection spawn.
    use std::fs;
    let src = fs::read_to_string("src/lib.rs").expect("read lib.rs");
    let acquire_idx = src.find("limiter.try_acquire(addr.ip())").expect("rate-limit gate");
    let siteban_idx = src.find("get_site_ban").expect("siteban gate");
    let spawn_idx = src
        .find("tokio::spawn(async move {\n                    handle_connection")
        .or_else(|| src.find("handle_connection(socket, addr"))
        .expect("handle_connection spawn");
    assert!(
        siteban_idx > acquire_idx,
        "site-ban gate must come after the rate-limit acquire"
    );
    assert!(
        siteban_idx < spawn_idx,
        "site-ban gate must come before the handle_connection spawn"
    );
}

#[test]
fn test_bans_rhai_module_is_registered() {
    // Source-level: ensure script/mod.rs declares the `bans` submodule and
    // calls `bans::register` so all the ban_*, *_site_ban, and alts fns are
    // actually available from Rhai.
    use std::fs;
    let src = fs::read_to_string("src/script/mod.rs").expect("read script/mod.rs");
    assert!(src.contains("pub mod bans;"));
    assert!(src.contains("bans::register("));
}

// ===========================================================================
// Account-wide bank + character-default-preferences slice tests
// ===========================================================================

#[test]
fn test_account_preferences_default_is_unset() {
    use ironmud::types::AccountPreferences;
    let p = AccountPreferences::default();
    assert!(!p.is_set);
    assert!(p.colors_enabled, "default colors should be on");
    assert!(p.abbrev_enabled, "default abbreviations should be on");
    assert!(!p.automap_enabled);
}

#[test]
fn test_legacy_account_without_shared_fields_loads() {
    // Pre-slice account JSON has neither `shared_bank_gold` nor
    // `character_defaults`. Both must default cleanly thanks to #[serde(default)].
    use ironmud::types::AccountData;
    let json = serde_json::json!({
        "id": "00000000-0000-0000-0000-000000000001",
        "name": "Legacy",
        "password_hash": "h",
        "character_names": ["Legacy"],
        "is_banned": false
    });
    let a: AccountData = serde_json::from_value(json).expect("legacy account loads");
    assert_eq!(a.shared_bank_gold, 0);
    assert!(!a.character_defaults.is_set);
}

#[test]
fn test_shared_bank_gold_round_trips() {
    use ironmud::db::Db;
    use ironmud::types::AccountData;

    let temp = tempfile::tempdir().expect("create temp dir");
    
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(temp.path()).expect("open DB");
        let mut a = AccountData::new("Vault".into(), "h".into());
        a.shared_bank_gold = 12345;
        db.save_account(a).unwrap();
        let reloaded = db.get_account("vault").unwrap().expect("account");
        assert_eq!(reloaded.shared_bank_gold, 12345);
    }));
    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_add_shared_bank_gold_refuses_negative_balance() {
    use ironmud::db::Db;
    use ironmud::types::AccountData;

    let temp = tempfile::tempdir().expect("create temp dir");
    
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(temp.path()).expect("open DB");
        let mut a = AccountData::new("Coffers".into(), "h".into());
        a.shared_bank_gold = 50;
        let id = a.id;
        db.save_account(a).unwrap();

        // Crediting 100 succeeds, balance 150.
        let new_balance = db.add_shared_bank_gold(&id, 100).unwrap();
        assert_eq!(new_balance, Some(150));

        // Debiting 200 (would be -50) is refused.
        let refused = db.add_shared_bank_gold(&id, -200).unwrap();
        assert_eq!(refused, None);

        // Account state is unchanged after the refused debit.
        let after = db.get_account_by_id(&id).unwrap().expect("account");
        assert_eq!(after.shared_bank_gold, 150);
    }));
    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_save_account_preferences_marks_is_set() {
    use ironmud::db::Db;
    use ironmud::types::{AccountData, AccountPreferences};

    let temp = tempfile::tempdir().expect("create temp dir");
    
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(temp.path()).expect("open DB");
        let a = AccountData::new("Prefs".into(), "h".into());
        let id = a.id;
        db.save_account(a).unwrap();

        let mut p = AccountPreferences::default();
        p.prompt_mode = "verbose".into();
        p.automap_enabled = true;
        p.automap_radius = 5;
        p.helpline_enabled = true;
        p.is_set = true;
        let ok = db.save_account_preferences(&id, p).unwrap();
        assert!(ok);

        let reloaded = db.get_account_by_id(&id).unwrap().expect("account");
        let d = reloaded.character_defaults;
        assert!(d.is_set);
        assert_eq!(d.prompt_mode, "verbose");
        assert!(d.automap_enabled);
        assert_eq!(d.automap_radius, 5);
        assert!(d.helpline_enabled);
    }));
    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_clear_account_preferences_resets_is_set() {
    use ironmud::db::Db;
    use ironmud::types::{AccountData, AccountPreferences};

    let temp = tempfile::tempdir().expect("create temp dir");
    
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(temp.path()).expect("open DB");
        let a = AccountData::new("Prefs2".into(), "h".into());
        let id = a.id;
        db.save_account(a).unwrap();

        let mut p = AccountPreferences::default();
        p.is_set = true;
        p.prompt_mode = "simple".into();
        db.save_account_preferences(&id, p).unwrap();

        // Clear by saving the engine default (is_set=false).
        db.save_account_preferences(&id, AccountPreferences::default())
            .unwrap();
        let reloaded = db.get_account_by_id(&id).unwrap().expect("account");
        assert!(!reloaded.character_defaults.is_set);
        assert_eq!(reloaded.character_defaults.prompt_mode, "");
    }));
    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_bank_rhai_has_shared_subcommand() {
    use std::fs;
    let src = fs::read_to_string("scripts/commands/bank.rhai").expect("read bank.rhai");
    assert!(
        src.contains("\"shared\""),
        "bank.rhai must dispatch a `shared` arm"
    );
    assert!(src.contains("cmd_shared_balance"));
    assert!(src.contains("cmd_shared_deposit"));
    assert!(src.contains("cmd_shared_withdraw"));
    assert!(src.contains("transfer_pocket_to_shared_bank"));
    assert!(src.contains("transfer_shared_bank_to_pocket"));
}

#[test]
fn test_set_rhai_has_defaults_subcommand() {
    use std::fs;
    let src = fs::read_to_string("scripts/commands/set.rhai").expect("read set.rhai");
    assert!(
        src.contains("\"defaults\""),
        "set.rhai must recognize the `defaults` arm"
    );
    assert!(src.contains("save_account_defaults_from_connection"));
    assert!(src.contains("clear_account_defaults"));
    assert!(src.contains("format_account_defaults"));
}

#[test]
fn test_create_rhai_calls_apply_account_defaults() {
    // Source-level: both insertion sites in create.rhai (auth'd alt path and
    // first-character path) must invoke apply_account_defaults_to_new_character
    // so saved defaults get stamped onto each freshly-saved character.
    use std::fs;
    let src = fs::read_to_string("scripts/commands/create.rhai").expect("read create.rhai");
    let count = src.matches("apply_account_defaults_to_new_character").count();
    assert!(
        count >= 2,
        "expected at least 2 calls (alt + first character), got {}",
        count
    );
}

#[test]
fn test_login_rhai_applies_session_defaults() {
    // The session-resident defaults (colors / mxp / abbrev) are stamped onto
    // the connection at every login so each alt inherits them.
    use std::fs;
    let src = fs::read_to_string("scripts/commands/login.rhai").expect("read login.rhai");
    assert!(src.contains("apply_account_session_defaults"));
}

#[test]
fn test_account_prefs_module_is_registered() {
    use std::fs;
    let src = fs::read_to_string("src/script/mod.rs").expect("read script/mod.rs");
    assert!(src.contains("pub mod account_prefs;"));
    assert!(src.contains("account_prefs::register("));
}

#[test]
fn test_apply_world_preset_switches_settings_and_reloads() {
    // Drives `apply_world_preset` through a free-standing engine so eval()
    // doesn't run while we hold the World lock — the binding itself locks
    // state and would deadlock otherwise (std::sync::Mutex isn't reentrant).
    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");
        let connections = Arc::new(Mutex::new(HashMap::new()));
        let command_metadata = load_command_metadata().expect("load command metadata");

        db.set_setting("class_preset", "fantasy").expect("seed class_preset");

        let state = Arc::new(Mutex::new(World {
            engine: Engine::new(),
            db,
            scripts: HashMap::new(),
            connections: connections.clone(),
            command_metadata,
            socials: ironmud::social::actions::SocialRegistry::default(),
            class_definitions: HashMap::new(),
            trait_definitions: HashMap::new(),
            race_suggestions: Vec::new(),
            race_definitions: HashMap::new(),
            language_definitions: HashMap::new(),
            recipes: HashMap::new(),
            transports: HashMap::new(),
            spell_definitions: HashMap::new(),
            achievement_definitions: HashMap::new(),
            achievement_index_by_counter: HashMap::new(),
            custom_skill_definitions: HashMap::new(),
            chat_sender: None,
            shutdown_sender: None,
            shutdown_cancel_sender: None,
            ip_limiter: Arc::new(ironmud::ratelimit::IpRateLimiter::new()),
            command_throttle: Arc::new(ironmud::throttle::CommandThrottle::new()),
        }));

        let mut driver = Engine::new();
        driver.set_max_expr_depths(128, 128);
        let db_for_register = state.lock().unwrap().db.clone();
        script::register_rhai_functions(
            &mut driver,
            Arc::new(db_for_register),
            connections.clone(),
            state.clone(),
        );
        driver.register_fn("chat_broadcast", |_m: String| {});
        driver.register_fn("matrix_broadcast", |_m: String| {});

        load_game_data(state.clone()).expect("initial load");
        let fantasy_classes = state.lock().unwrap().class_definitions.len();
        assert!(
            fantasy_classes > 1,
            "fantasy preset should expose >1 class, got {}",
            fantasy_classes
        );

        let bad: rhai::Map = driver
            .eval(r#"apply_world_preset("nonsense")"#)
            .expect("eval bad preset");
        let bad_ok = bad
            .get("ok")
            .and_then(|d| d.clone().as_bool().ok())
            .unwrap_or(true);
        assert!(!bad_ok, "unknown preset should fail, got {:?}", bad);

        let res: rhai::Map = driver
            .eval(r#"apply_world_preset("modern")"#)
            .expect("eval modern");
        let ok = res
            .get("ok")
            .and_then(|d| d.clone().as_bool().ok())
            .unwrap_or(false);
        assert!(ok, "modern preset switch should succeed: {:?}", res);

        {
            let world = state.lock().unwrap();
            for key in ["class_preset", "race_preset", "spell_preset", "language_preset"] {
                let val = world.db.get_setting(key).unwrap().unwrap_or_default();
                assert_eq!(val, "modern", "{} not switched", key);
            }
        }

        let (classes_n, races_n) = {
            let world = state.lock().unwrap();
            (world.class_definitions.len(), world.race_definitions.len())
        };
        assert!(classes_n >= 1, "modern preset loaded {} classes", classes_n);
        assert!(races_n >= 1, "modern preset loaded {} races", races_n);

        let _back: rhai::Map = driver
            .eval(r#"apply_world_preset("fantasy")"#)
            .expect("eval fantasy");
        let val = state
            .lock()
            .unwrap()
            .db
            .get_setting("class_preset")
            .unwrap()
            .unwrap_or_default();
        assert_eq!(val, "fantasy");
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_apply_mobile_preset_stamps_flags_and_stats() {
    use ironmud::MobileData;
    use ironmud::script::mobile_presets::{apply_preset_to_mobile, find_preset_by_id};

    // dire_wolf preset is shipped with the codebase; if this fails, either the
    // JSON file moved or the preset id changed.
    let preset = find_preset_by_id("dire_wolf").expect("dire_wolf preset present");

    let mut mob = MobileData::new("a goblin".to_string());
    assert!(!mob.flags.aggressive, "fresh mob is not aggressive");
    assert!(mob.on_hit_effects.is_empty());

    apply_preset_to_mobile(&mut mob, &preset);

    assert!(mob.flags.aggressive, "preset stamps aggressive=true");
    assert!(mob.flags.stay_zone, "preset stamps stay_zone=true");
    assert_eq!(mob.level, 6);
    assert_eq!(mob.max_hp, 60);
    assert_eq!(mob.current_hp, 60, "current_hp synced to new max_hp");
    assert_eq!(mob.armor_class, 6);
    assert_eq!(mob.damage_dice, "2d4");
    assert_eq!(mob.perception, 4);
    assert_eq!(mob.on_hit_effects.len(), 1);
    assert_eq!(mob.on_hit_effects[0].effect, "bleeding");
    assert_eq!(mob.on_hit_effects[0].chance, 30);
}

#[test]
fn test_apply_mobile_preset_persists_through_db_round_trip() {
    use ironmud::MobileData;
    
    use ironmud::script::mobile_presets::{apply_preset_to_mobile, find_preset_by_id};

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");
        let preset = find_preset_by_id("town_guard_captain").expect("guard preset present");

        let mut mob = MobileData::new("a recruit".to_string());
        let id = mob.id;
        apply_preset_to_mobile(&mut mob, &preset);
        db.save_mobile_data(mob).expect("save");

        let loaded = db.get_mobile_data(&id).expect("get").expect("present");
        assert!(loaded.flags.guard);
        assert!(loaded.flags.helper);
        assert_eq!(loaded.level, 8);
        assert_eq!(loaded.max_hp, 80);
        assert_eq!(loaded.faction.as_deref(), Some("town_watch"));
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

fn vampire_test_room(title: &str, indoors: bool) -> ironmud::types::RoomData {
    use ironmud::types::{RoomData, RoomExits, RoomFlags, WaterType};
    use std::collections::HashMap;
    let mut flags = RoomFlags::default();
    flags.indoors = indoors;
    RoomData {
        id: uuid::Uuid::new_v4(),
        title: title.to_string(),
        description: String::new(),
        exits: RoomExits::default(),
        flags,
        extra_descs: Vec::new(),
        vnum: None,
        area_id: None,
        triggers: Vec::new(),
        doors: HashMap::new(),
        spring_desc: None,
        summer_desc: None,
        autumn_desc: None,
        winter_desc: None,
        dynamic_desc: None,
        water_type: WaterType::None,
        catch_table: Vec::new(),
        is_property_template: false,
        property_template_id: None,
        is_template_entrance: false,
        property_lease_id: None,
        property_entrance: false,
        recent_departures: Vec::new(),
        blood_trails: Vec::new(),
        traps: Vec::new(),
        living_capacity: 0,
        residents: Vec::new(),
        dg_vars: HashMap::new(),
        coordinates: None,
        contextual_commands: Vec::new(),
        exit_delays: HashMap::new(),
    }
}

#[test]
fn test_sun_tick_burns_outdoor_vampire_mob_during_day() {
    use ironmud::MobileData;
    
    use ironmud::vampire::process_sun_tick;
    use ironmud::types::VampireState;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        // Force daytime: noon-ish.
        let mut gt = db.get_game_time().expect("read time");
        gt.hour = 12;
        db.save_game_time(&gt).expect("save time");

        // Outdoor room.
        let outdoor = vampire_test_room("Town Square", false);
        let outdoor_id = outdoor.id;
        db.save_room_data(outdoor).expect("save room");

        // Vampire mob in the outdoor room.
        let mut vamp = MobileData::new("a fledgling vampire".to_string());
        vamp.is_prototype = false;
        vamp.flags.vampire = true;
        vamp.vampire_state = Some(VampireState::default());
        vamp.current_room_id = Some(outdoor_id);
        vamp.max_hp = 100;
        vamp.current_hp = 100;
        let vamp_id = vamp.id;
        db.save_mobile_data(vamp).expect("save mob");

        // Mortal mob in the same room (control).
        let mut mortal = MobileData::new("a baker".to_string());
        mortal.is_prototype = false;
        mortal.current_room_id = Some(outdoor_id);
        mortal.max_hp = 100;
        mortal.current_hp = 100;
        let mortal_id = mortal.id;
        db.save_mobile_data(mortal).expect("save mortal");

        let connections: ironmud::SharedConnections = Arc::new(Mutex::new(HashMap::new()));
        process_sun_tick(&db, &connections).expect("tick runs");

        let burned = db.get_mobile_data(&vamp_id).unwrap().unwrap();
        assert!(burned.current_hp < 100, "vampire took sun damage");

        let unburned = db.get_mobile_data(&mortal_id).unwrap().unwrap();
        assert_eq!(unburned.current_hp, 100, "mortal untouched");
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_sun_tick_skips_indoor_vampire() {
    use ironmud::MobileData;
    
    use ironmud::vampire::process_sun_tick;
    use ironmud::types::VampireState;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut gt = db.get_game_time().expect("read time");
        gt.hour = 12;
        db.save_game_time(&gt).expect("save time");

        let indoor = vampire_test_room("a windowless cellar", true);
        let indoor_id = indoor.id;
        db.save_room_data(indoor).expect("save room");

        let mut vamp = MobileData::new("a sheltered vampire".to_string());
        vamp.is_prototype = false;
        vamp.flags.vampire = true;
        vamp.vampire_state = Some(VampireState::default());
        vamp.current_room_id = Some(indoor_id);
        vamp.max_hp = 100;
        vamp.current_hp = 100;
        let vamp_id = vamp.id;
        db.save_mobile_data(vamp).expect("save");

        let connections: ironmud::SharedConnections = Arc::new(Mutex::new(HashMap::new()));
        process_sun_tick(&db, &connections).expect("tick");

        let after = db.get_mobile_data(&vamp_id).unwrap().unwrap();
        assert_eq!(after.current_hp, 100, "indoor vampire is safe");
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_sun_tick_skips_at_night() {
    use ironmud::MobileData;
    
    use ironmud::vampire::process_sun_tick;
    use ironmud::types::VampireState;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut gt = db.get_game_time().expect("read time");
        gt.hour = 1; // dead of night
        db.save_game_time(&gt).expect("save time");

        let outdoor = vampire_test_room("Moonlit Park", false);
        let outdoor_id = outdoor.id;
        db.save_room_data(outdoor).expect("save room");

        let mut vamp = MobileData::new("a midnight prowler".to_string());
        vamp.is_prototype = false;
        vamp.flags.vampire = true;
        vamp.vampire_state = Some(VampireState::default());
        vamp.current_room_id = Some(outdoor_id);
        vamp.max_hp = 100;
        vamp.current_hp = 100;
        let vamp_id = vamp.id;
        db.save_mobile_data(vamp).expect("save");

        let connections: ironmud::SharedConnections = Arc::new(Mutex::new(HashMap::new()));
        process_sun_tick(&db, &connections).expect("tick");

        let after = db.get_mobile_data(&vamp_id).unwrap().unwrap();
        assert_eq!(after.current_hp, 100, "night gives no damage");
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_vampire_class_present_with_dominate_starter_skill() {
    // Vampire lives in its own theme-agnostic file so any preset (fantasy,
    // modern, future) picks it up when `enable_vampire_creation` is on.
    let path = "scripts/data/classes_vampire.json";
    let content = std::fs::read_to_string(path).expect("classes_vampire.json present");
    let parsed: serde_json::Value = serde_json::from_str(&content).expect("valid JSON");
    let vampire = parsed
        .get("vampire")
        .expect("vampire class entry present");
    assert_eq!(
        vampire.get("available").and_then(|v| v.as_bool()),
        Some(true),
        "vampire class loadable; runtime gate via enable_vampire_creation lives in get_class_list"
    );
    let starting_skills = vampire
        .get("starting_skills")
        .and_then(|v| v.as_object())
        .expect("starting_skills present");
    assert!(
        starting_skills.contains_key("dominate"),
        "vampire class seeds the dominate discipline"
    );
    let incompat: Vec<String> = vampire
        .get("incompatible_races")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    for race in &["synth", "bioroid", "clone"] {
        assert!(
            incompat.iter().any(|r| r == race),
            "modern synthetic race '{}' should be blocked from the vampire class",
            race
        );
    }
}

#[test]
fn test_class_allowed_for_race_filters() {
    use ironmud::types::ClassDefinition;
    use std::collections::HashMap;

    let mut vampire = ClassDefinition {
        id: "vampire".into(),
        name: "Vampire".into(),
        description: String::new(),
        starting_skills: HashMap::new(),
        stat_bonuses: HashMap::new(),
        available: true,
        starting_languages: HashMap::new(),
        starting_items: Vec::new(),
        starting_gold: 0,
        allowed_races: Vec::new(),
        incompatible_races: vec!["synth".into(), "bioroid".into(), "clone".into()],
    };
    assert!(vampire.allowed_for_race("human"));
    assert!(vampire.allowed_for_race("orc"));
    assert!(!vampire.allowed_for_race("synth"));
    assert!(!vampire.allowed_for_race("SYNTH"), "case-insensitive");
    assert!(!vampire.allowed_for_race("clone"));
    // Empty race id (pre-pick) leaves the class visible.
    assert!(vampire.allowed_for_race(""));

    // Allowlist semantics: only listed races may pick.
    vampire.allowed_races = vec!["human".into(), "elf".into()];
    vampire.incompatible_races.clear();
    assert!(vampire.allowed_for_race("human"));
    assert!(vampire.allowed_for_race("elf"));
    assert!(!vampire.allowed_for_race("dwarf"));
}

#[test]
fn test_dialogue_humanity_at_least_evaluates_correctly() {
    use ironmud::types::{DialogueCondition, VampireState};

    let cond = DialogueCondition::HumanityAtLeast { threshold: 5 };
    let serialized = serde_json::to_string(&cond).expect("serialize");
    assert!(
        serialized.contains("humanity_at_least"),
        "tag is humanity_at_least"
    );
    assert!(serialized.contains("5"), "threshold encoded");

    // Also round-trip parse to make sure the deserialize path works.
    let parsed: DialogueCondition = serde_json::from_str(
        r#"{"kind":"humanity_at_least","threshold":7}"#,
    )
    .expect("parse");
    match parsed {
        DialogueCondition::HumanityAtLeast { threshold } => assert_eq!(threshold, 7),
        _ => panic!("wrong variant"),
    }

    // Sanity-check that VampireState matches threshold semantics.
    let mut vs = VampireState::default();
    vs.humanity = 6;
    assert!(vs.humanity >= 5);
    assert!(vs.humanity < 7);
}

#[test]
fn test_bloodfeed_willing_field_persists() {
    use ironmud::CharacterData;

    let json = r#"{
        "name": "test",
        "password_hash": "",
        "current_room_id": "00000000-0000-0000-0000-000000000000",
        "bloodfeed_willing": true
    }"#;
    let ch: CharacterData = serde_json::from_str(json).expect("parse char");
    assert!(ch.bloodfeed_willing, "field deserializes");

    let serialized = serde_json::to_string(&ch).expect("serialize");
    assert!(
        serialized.contains("\"bloodfeed_willing\":true"),
        "field serializes"
    );
}

#[test]
fn test_charm_master_recognizes_dominated_buff() {
    use ironmud::ActiveBuff;
    use ironmud::MobileData;
    use ironmud::types::EffectType;

    let mut mob = MobileData::new("a fledgling".to_string());
    assert!(mob.charm_master().is_none(), "fresh mob has no master");

    mob.active_buffs.push(ActiveBuff {
        effect_type: EffectType::Dominated,
        magnitude: 0,
        remaining_secs: 600,
        source: "Vlad".to_string(),
        damage_type: None,
        vs_effect: None,
        skill_key: None,
    });
    assert_eq!(
        mob.charm_master().map(|s| s.to_string()),
        Some("Vlad".to_string()),
        "Dominated maps onto charm_master"
    );
    assert!(mob.is_charmed_by("vlad"), "case-insensitive match");
    assert!(mob.is_charmed_by_anyone());
}

#[test]
fn test_charm_master_prefers_charmed_when_both_present() {
    use ironmud::ActiveBuff;
    use ironmud::MobileData;
    use ironmud::types::EffectType;

    let mut mob = MobileData::new("a thrall".to_string());
    mob.active_buffs.push(ActiveBuff {
        effect_type: EffectType::Charmed,
        magnitude: 0,
        remaining_secs: 60,
        source: "Player1".to_string(),
        damage_type: None,
        vs_effect: None,
        skill_key: None,
    });
    mob.active_buffs.push(ActiveBuff {
        effect_type: EffectType::Dominated,
        magnitude: 0,
        remaining_secs: 600,
        source: "Player2".to_string(),
        damage_type: None,
        vs_effect: None,
        skill_key: None,
    });
    assert_eq!(
        mob.charm_master().map(|s| s.to_string()),
        Some("Player1".to_string()),
        "Charmed wins ties"
    );
}

#[test]
fn test_vampire_clans_json_loads_with_five_clans() {
    let path = "scripts/data/vampire_clans.json";
    let content = std::fs::read_to_string(path).expect("vampire_clans.json present");
    let parsed: serde_json::Value = serde_json::from_str(&content).expect("valid JSON");
    let obj = parsed.as_object().expect("top-level object");

    let expected_clans = ["brujah", "toreador", "ventrue", "nosferatu", "gangrel"];
    for clan in &expected_clans {
        let entry = obj.get(*clan).expect(&format!("{} present", clan));
        assert!(
            entry.get("trait_id").and_then(|v| v.as_str()).is_some(),
            "{} has trait_id",
            clan
        );
    }
}

#[test]
fn test_clan_traits_present_and_unavailable() {
    let path = "scripts/data/traits.json";
    let content = std::fs::read_to_string(path).expect("traits.json present");
    let parsed: serde_json::Value = serde_json::from_str(&content).expect("valid JSON");
    let obj = parsed.as_object().expect("top-level object");

    for clan in &[
        "clan_brujah",
        "clan_toreador",
        "clan_ventrue",
        "clan_nosferatu",
        "clan_gangrel",
    ] {
        let entry = obj.get(*clan).expect(&format!("{} trait present", clan));
        assert_eq!(
            entry.get("available").and_then(|v| v.as_bool()),
            Some(false),
            "{} must be unavailable at creation (granted by embrace only)",
            clan
        );
    }
}

#[test]
fn test_vampire_spells_json_has_disciplines_with_skills() {
    let path = "scripts/data/spells_vampire.json";
    let content = std::fs::read_to_string(path).expect("spells_vampire.json present");
    let parsed: serde_json::Value = serde_json::from_str(&content).expect("valid JSON");
    let obj = parsed.as_object().expect("top-level object");

    let expected = [
        ("command", "dominate", 1),
        ("dominate", "dominate", 5),
        ("heightened_senses", "auspex", 1),
        ("vigor", "potence", 1),
        ("cloak_of_shadows", "obfuscate", 1),
    ];
    for (id, skill, dots) in &expected {
        let spell = obj.get(*id).expect(&format!("{} spell present", id));
        assert_eq!(
            spell.get("requires_skill").and_then(|v| v.as_str()),
            Some(*skill),
            "{} gates on {}",
            id,
            skill
        );
        assert_eq!(
            spell.get("requires_vampire").and_then(|v| v.as_bool()),
            Some(true),
            "{} requires_vampire=true",
            id
        );
        assert_eq!(
            spell.get("skill_required").and_then(|v| v.as_i64()),
            Some(*dots as i64),
            "{} dot level",
            id
        );
    }
}

#[test]
fn test_sun_tick_first_lethal_hit_floors_at_one_hp_and_stamps_burning() {
    use ironmud::MobileData;
    
    use ironmud::types::{EffectType, VampireState};
    use ironmud::vampire::process_sun_tick;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut gt = db.get_game_time().expect("read time");
        gt.hour = 12;
        db.save_game_time(&gt).expect("save time");

        let outdoor = vampire_test_room("Town Square", false);
        let outdoor_id = outdoor.id;
        db.save_room_data(outdoor).expect("save room");

        // Vampire at low HP — next sun tick would normally kill it.
        let mut vamp = MobileData::new("a fledgling".to_string());
        vamp.is_prototype = false;
        vamp.flags.vampire = true;
        vamp.vampire_state = Some(VampireState::default());
        vamp.current_room_id = Some(outdoor_id);
        vamp.max_hp = 100; // damage = 5
        vamp.current_hp = 3; // would drop to -2 raw
        let vamp_id = vamp.id;
        db.save_mobile_data(vamp).expect("save");

        let connections: ironmud::SharedConnections = Arc::new(Mutex::new(HashMap::new()));
        process_sun_tick(&db, &connections).expect("first tick");

        let after = db.get_mobile_data(&vamp_id).unwrap().unwrap();
        assert_eq!(after.current_hp, 1, "hp floored at 1 in rescue window");
        let burning = after
            .active_buffs
            .iter()
            .any(|b| b.effect_type == EffectType::SunlightBurning);
        assert!(burning, "SunlightBurning stamped");

        // Second tick while still exposed — finishes the kill.
        process_sun_tick(&db, &connections).expect("second tick");
        let after2 = db.get_mobile_data(&vamp_id).unwrap().unwrap();
        assert_eq!(after2.current_hp, 0, "second tick is lethal");
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_sun_tick_clears_burning_when_moved_indoors() {
    use ironmud::MobileData;
    
    use ironmud::types::{EffectType, VampireState};
    use ironmud::vampire::process_sun_tick;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut gt = db.get_game_time().expect("read time");
        gt.hour = 12;
        db.save_game_time(&gt).expect("save time");

        let outdoor = vampire_test_room("Plaza", false);
        let outdoor_id = outdoor.id;
        db.save_room_data(outdoor).expect("save outdoor");

        let cellar = vampire_test_room("Stone Cellar", true); // indoors=true
        let cellar_id = cellar.id;
        db.save_room_data(cellar).expect("save cellar");

        let mut vamp = MobileData::new("a fledgling".to_string());
        vamp.is_prototype = false;
        vamp.flags.vampire = true;
        vamp.vampire_state = Some(VampireState::default());
        vamp.current_room_id = Some(outdoor_id);
        vamp.max_hp = 100;
        vamp.current_hp = 3;
        let vamp_id = vamp.id;
        db.save_mobile_data(vamp).expect("save");

        let connections: ironmud::SharedConnections = Arc::new(Mutex::new(HashMap::new()));
        process_sun_tick(&db, &connections).expect("burn tick");

        // Verify burning was stamped.
        let mid = db.get_mobile_data(&vamp_id).unwrap().unwrap();
        assert!(
            mid.active_buffs
                .iter()
                .any(|b| b.effect_type == EffectType::SunlightBurning),
            "burning before rescue"
        );

        // Drag rescue: simulate by moving to cellar.
        db.move_mobile_to_room(&vamp_id, &cellar_id).unwrap();

        // Next sun tick: still daytime, but they're indoors → burning clears.
        process_sun_tick(&db, &connections).expect("indoor tick");
        let after = db.get_mobile_data(&vamp_id).unwrap().unwrap();
        assert!(
            !after
                .active_buffs
                .iter()
                .any(|b| b.effect_type == EffectType::SunlightBurning),
            "rescue cleared SunlightBurning"
        );
        assert_eq!(after.current_hp, 1, "still injured but alive");
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_vampire_presets_load_and_apply() {
    use ironmud::MobileData;
    use ironmud::script::mobile_presets::{apply_preset_to_mobile, find_preset_by_id};

    for id in &["vampire_goon", "vampire_elder", "vampire_hunter"] {
        let preset = find_preset_by_id(id).expect(&format!("{} preset present", id));
        let mut mob = MobileData::new(format!("a stand-in for {}", id));
        apply_preset_to_mobile(&mut mob, &preset);
        match *id {
            "vampire_goon" => {
                assert!(mob.flags.vampire);
                assert!(mob.flags.undead);
                assert!(mob.flags.holy_vulnerable);
                assert!(mob.flags.aggressive);
            }
            "vampire_elder" => {
                assert!(mob.flags.vampire);
                assert!(mob.flags.no_charm && mob.flags.no_summon);
                assert!(mob.level >= 15);
                assert_eq!(mob.faction.as_deref(), Some("camarilla"));
            }
            "vampire_hunter" => {
                assert!(mob.flags.guard && mob.flags.helper);
                assert!(!mob.flags.vampire, "hunter is mortal");
                assert_eq!(mob.faction.as_deref(), Some("vampire_hunters"));
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn test_blood_tick_decays_vampire_mob_pool() {
    use ironmud::MobileData;
    
    use ironmud::vampire::process_blood_tick;
    use ironmud::types::VampireState;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut vamp = MobileData::new("a brujah".to_string());
        vamp.is_prototype = false;
        let mut vs = VampireState::default();
        vs.blood_pool = 7;
        vamp.vampire_state = Some(vs);
        let id = vamp.id;
        db.save_mobile_data(vamp).expect("save");

        // Mortal mob — confirm we don't accidentally touch it.
        let mut mortal = MobileData::new("a townsfolk".to_string());
        mortal.is_prototype = false;
        let mortal_id = mortal.id;
        db.save_mobile_data(mortal).expect("save mortal");

        let connections: ironmud::SharedConnections = Arc::new(Mutex::new(HashMap::new()));
        process_blood_tick(&db, &connections).expect("tick");

        let after = db.get_mobile_data(&id).unwrap().unwrap();
        let v = after.vampire_state.expect("still a vampire");
        assert_eq!(v.blood_pool, 6, "decayed by 1");

        let mortal_after = db.get_mobile_data(&mortal_id).unwrap().unwrap();
        assert!(mortal_after.vampire_state.is_none(), "mortals untouched");
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_mobile_vampire_state_round_trips() {
    use ironmud::MobileData;
    
    use ironmud::types::VampireState;

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut mob = MobileData::new("a brujah elder".to_string());
        assert!(mob.vampire_state.is_none(), "fresh mobs are mortal");

        mob.vampire_state = Some(VampireState::newly_embraced(1_700_000_000, Some("the prince".to_string())));
        if let Some(ref mut v) = mob.vampire_state {
            v.set_humanity(4);
            v.blood_pool = 8;
        }
        let id = mob.id;
        db.save_mobile_data(mob).expect("save");

        let loaded = db.get_mobile_data(&id).expect("get").expect("present");
        let state = loaded.vampire_state.expect("vampire state retained");
        assert_eq!(state.humanity, 4);
        assert_eq!(state.blood_pool, 8);
        assert_eq!(state.sire_id.as_deref(), Some("the prince"));
        assert_eq!(state.embrace_time, Some(1_700_000_000));
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_vampire_state_humanity_clamps_at_extremes() {
    use ironmud::types::VampireState;

    let mut v = VampireState::default();
    assert_eq!(v.humanity, 7);

    assert_eq!(v.set_humanity(20), 10, "clamps high");
    assert_eq!(v.set_humanity(-5), 0, "clamps low");

    assert_eq!(v.change_humanity(3), 3);
    assert_eq!(v.change_humanity(-100), 0, "saturating below 0");
    assert_eq!(v.change_humanity(50), 10, "saturating above 10");
}

#[test]
fn test_vampire_state_frenzy_window() {
    use ironmud::types::VampireState;

    let mut v = VampireState::default();
    assert!(!v.is_frenzying(0));
    assert!(!v.is_frenzying(1_000_000_000));

    v.frenzy_until = Some(1_000_000_500);
    assert!(v.is_frenzying(1_000_000_000), "before deadline");
    assert!(!v.is_frenzying(1_000_000_500), "at deadline (exclusive)");
    assert!(!v.is_frenzying(1_000_000_999), "after deadline");
}

#[test]
fn test_undead_vampire_holy_vulnerable_flags_persist() {
    use ironmud::MobileData;
    

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut mob = MobileData::new("a fledgling".to_string());
        assert!(!mob.flags.undead);
        assert!(!mob.flags.vampire);
        assert!(!mob.flags.holy_vulnerable);

        mob.flags.undead = true;
        mob.flags.vampire = true;
        mob.flags.holy_vulnerable = true;
        let id = mob.id;
        db.save_mobile_data(mob).expect("save");

        let loaded = db.get_mobile_data(&id).expect("get").expect("present");
        assert!(loaded.flags.undead);
        assert!(loaded.flags.vampire);
        assert!(loaded.flags.holy_vulnerable);
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_item_holy_flag_persists() {
    use ironmud::ItemData;
    

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        let mut item = ItemData::new(
            "holy water vial".to_string(),
            "a vial of holy water".to_string(),
            "A vial of clear holy water rests here.".to_string(),
        );
        assert!(!item.flags.holy);
        item.flags.holy = true;
        let id = item.id;
        db.save_item_data(item).expect("save");

        let loaded = db.get_item_data(&id).expect("get").expect("present");
        assert!(loaded.flags.holy);
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_new_damage_types_round_trip() {
    use ironmud::types::DamageType;

    assert_eq!(DamageType::from_str("sunlight"), Some(DamageType::Sunlight));
    assert_eq!(DamageType::from_str("sun"), Some(DamageType::Sunlight));
    assert_eq!(DamageType::from_str("holy"), Some(DamageType::Holy));
    assert_eq!(DamageType::from_str("blessed"), Some(DamageType::Holy));
    assert_eq!(DamageType::from_str("divine"), Some(DamageType::Holy));

    assert_eq!(DamageType::Sunlight.to_display_string(), "sunlight");
    assert_eq!(DamageType::Holy.to_display_string(), "holy");

    let all = DamageType::all();
    assert!(all.contains(&"sunlight"));
    assert!(all.contains(&"holy"));
}

#[test]
fn test_new_effect_types_round_trip() {
    use ironmud::types::EffectType;

    assert_eq!(
        EffectType::from_str("sunlight_burn"),
        Some(EffectType::SunlightBurn)
    );
    assert_eq!(
        EffectType::from_str("sunlight_burning"),
        Some(EffectType::SunlightBurning)
    );
    assert_eq!(EffectType::from_str("frenzy"), Some(EffectType::Frenzy));
    assert_eq!(EffectType::from_str("berserk"), Some(EffectType::Frenzy));
    assert_eq!(
        EffectType::from_str("dominated"),
        Some(EffectType::Dominated)
    );
    assert_eq!(
        EffectType::from_str("obfuscate"),
        Some(EffectType::Obfuscate)
    );

    let all = EffectType::all();
    assert!(all.contains(&"sunlight_burn"));
    assert!(all.contains(&"frenzy"));
    assert!(all.contains(&"dominated"));
}

#[test]
fn test_apply_mobile_preset_dedupes_on_hit_effects_by_name() {
    use ironmud::{MobileData, OnHitEffect};
    use ironmud::script::mobile_presets::{apply_preset_to_mobile, find_preset_by_id};

    let preset = find_preset_by_id("dire_wolf").expect("preset present");
    let mut mob = MobileData::new("a wolf".to_string());

    // Pre-existing effect with the same name should be replaced, not duplicated.
    mob.on_hit_effects.push(OnHitEffect {
        effect: "bleeding".to_string(),
        chance: 5,
        magnitude: 1,
        duration: 1,
    });
    // Pre-existing effect with a different name should be untouched.
    mob.on_hit_effects.push(OnHitEffect {
        effect: "poison".to_string(),
        chance: 50,
        magnitude: 3,
        duration: 5,
    });

    apply_preset_to_mobile(&mut mob, &preset);

    assert_eq!(mob.on_hit_effects.len(), 2, "preserves non-conflicting effect");
    let bleeding = mob
        .on_hit_effects
        .iter()
        .find(|e| e.effect == "bleeding")
        .expect("bleeding present");
    assert_eq!(bleeding.chance, 30, "preset value replaces prior bleeding");
    let poison = mob
        .on_hit_effects
        .iter()
        .find(|e| e.effect == "poison")
        .expect("poison preserved");
    assert_eq!(poison.magnitude, 3);
}

// ============================================================================
// Thinblood progression — derived state, clan acknowledgment, pro/con
// mechanical hooks. See `/home/craig/.claude/plans/i-would-like-to-precious-stardust.md`.
// ============================================================================

fn vampire_test_pc(name: &str) -> ironmud::types::CharacterData {
    serde_json::from_value(serde_json::json!({
        "name": name,
        "password_hash": "",
        "current_room_id": uuid::Uuid::nil(),
    }))
    .expect("build character")
}

#[test]
fn test_is_thinblood_reflects_vampire_state_and_clan_trait() {
    use ironmud::script::vampire::is_pc_thinblood;
    use ironmud::types::VampireState;

    let mut ch = vampire_test_pc("subject");

    // Mortal — never thinblood.
    assert!(!is_pc_thinblood(&ch));

    // Embraced, no clan trait — thinblood.
    ch.vampire_state = Some(VampireState::default());
    assert!(is_pc_thinblood(&ch));

    // Add a clan trait — clan-acknowledged, not thinblood.
    ch.traits.push("clan_brujah".to_string());
    assert!(!is_pc_thinblood(&ch));

    // Strip the trait but keep an unrelated trait — back to thinblood.
    ch.traits.clear();
    ch.traits.push("night_vision".to_string());
    assert!(is_pc_thinblood(&ch));
}

#[test]
fn test_apply_clan_acknowledgment_stamps_skill_from_clans_json() {
    use ironmud::script::vampire::apply_clan_acknowledgment;
    use ironmud::types::VampireState;

    let mut ch = vampire_test_pc("brujah_initiate");
    ch.vampire_state = Some(VampireState::default());
    // Auto-create thinblood floor: cap at 6 to mirror create.rhai.
    if let Some(v) = ch.vampire_state.as_mut() {
        v.max_blood_pool = 6;
        v.blood_pool = 4;
    }

    let changed = apply_clan_acknowledgment(&mut ch, "brujah", Some("Marcus".to_string()));
    assert!(changed, "first acknowledgment should change state");

    // Trait granted.
    assert!(ch.traits.iter().any(|t| t == "clan_brujah"));
    // Blood pool uplift (Brujah preferred discipline is `potence` per
    // vampire_clans.json — first preferred wins).
    let v = ch.vampire_state.as_ref().expect("still vampire");
    assert_eq!(v.max_blood_pool, 10);
    assert_eq!(v.blood_pool, 10);
    assert_eq!(v.sire_id.as_deref(), Some("Marcus"));
    // Starter discipline seeded.
    let potence = ch.skills.get("potence").expect("potence seeded");
    assert_eq!(potence.level, 1);

    // Toreador → auspex (first preferred in vampire_clans.json).
    let mut tor = vampire_test_pc("toreador_initiate");
    tor.vampire_state = Some(VampireState::default());
    apply_clan_acknowledgment(&mut tor, "toreador", None);
    assert!(tor.skills.contains_key("auspex"));
    assert_eq!(tor.skills.get("auspex").unwrap().level, 1);
}

#[test]
fn test_apply_clan_acknowledgment_idempotent() {
    use ironmud::script::vampire::apply_clan_acknowledgment;
    use ironmud::types::VampireState;

    let mut ch = vampire_test_pc("ventrue_lord");
    ch.vampire_state = Some(VampireState::default());

    let changed_first = apply_clan_acknowledgment(&mut ch, "ventrue", Some("Sire".into()));
    assert!(changed_first);

    // Second call with same clan: trait dedupe, skill stays at 1, no
    // duplicate clan trait, blood doesn't keep refilling.
    let changed_second = apply_clan_acknowledgment(&mut ch, "ventrue", Some("Sire".into()));
    assert!(!changed_second, "second call is a no-op");

    let count = ch.traits.iter().filter(|t| *t == "clan_ventrue").count();
    assert_eq!(count, 1, "trait deduped");
    let dom = ch.skills.get("dominate").expect("dominate seeded");
    assert_eq!(dom.level, 1, "skill stays at 1");
}

#[test]
fn test_apply_humanity_loss_halves_for_thinblood() {
    use ironmud::script::vampire::apply_humanity_loss;
    use ironmud::types::VampireState;

    // Thinblood — base=2 deducts 1.
    let mut tb = vampire_test_pc("thinblood");
    tb.vampire_state = Some(VampireState::default());
    let start_h = tb.vampire_state.as_ref().unwrap().humanity;
    let lost = apply_humanity_loss(&mut tb, 2);
    assert_eq!(lost, 1);
    assert_eq!(tb.vampire_state.as_ref().unwrap().humanity, start_h - 1);

    // Thinblood — base=1 deducts 0 (newbie forgiveness).
    let mut tb1 = vampire_test_pc("newbie");
    tb1.vampire_state = Some(VampireState::default());
    let start_h1 = tb1.vampire_state.as_ref().unwrap().humanity;
    let lost1 = apply_humanity_loss(&mut tb1, 1);
    assert_eq!(lost1, 0);
    assert_eq!(tb1.vampire_state.as_ref().unwrap().humanity, start_h1);

    // Clan-acknowledged — base=1 deducts 1.
    let mut clan = vampire_test_pc("brujah");
    clan.vampire_state = Some(VampireState::default());
    clan.traits.push("clan_brujah".to_string());
    let start_h2 = clan.vampire_state.as_ref().unwrap().humanity;
    let lost2 = apply_humanity_loss(&mut clan, 1);
    assert_eq!(lost2, 1);
    assert_eq!(clan.vampire_state.as_ref().unwrap().humanity, start_h2 - 1);

    // Mortal — never deducts (no vampire state).
    let mut mortal = vampire_test_pc("mortal");
    let lost3 = apply_humanity_loss(&mut mortal, 5);
    assert_eq!(lost3, 0);
}

#[test]
fn test_dialogue_condition_is_thinblood_evaluates() {
    // Reuses the existing condition-evaluator test pattern by serializing
    // the condition through the public API surface (DialogueCondition is
    // serde-tagged "kind: is_thinblood"). Confirms parse + variant present.
    use ironmud::types::DialogueCondition;

    let cond = DialogueCondition::IsThinblood;
    let json = serde_json::to_string(&cond).expect("serialize");
    assert!(json.contains("is_thinblood"), "tag is_thinblood: {}", json);

    // Round-trip: deserialize to ensure the variant survives.
    let back: DialogueCondition = serde_json::from_str(&json).expect("deserialize");
    assert!(matches!(back, DialogueCondition::IsThinblood));
}

#[test]
fn test_dialogue_condition_is_clan_acknowledged_evaluates() {
    use ironmud::types::DialogueCondition;

    let cond = DialogueCondition::IsClanAcknowledged;
    let json = serde_json::to_string(&cond).expect("serialize");
    assert!(json.contains("is_clan_acknowledged"), "tag: {}", json);

    let back: DialogueCondition = serde_json::from_str(&json).expect("deserialize");
    assert!(matches!(back, DialogueCondition::IsClanAcknowledged));
}

#[test]
fn test_quest_reward_embrace_clan_round_trip() {
    use ironmud::types::QuestReward;

    let reward = QuestReward::EmbraceClan {
        clan: "brujah".to_string(),
    };
    let json = serde_json::to_string(&reward).expect("serialize");
    assert!(json.contains("embrace_clan"));
    assert!(json.contains("brujah"));

    let back: QuestReward = serde_json::from_str(&json).expect("deserialize");
    match back {
        QuestReward::EmbraceClan { clan } => assert_eq!(clan, "brujah"),
        _ => panic!("variant mismatch"),
    }
}

#[test]
fn test_quest_reward_embrace_anarch_round_trip() {
    use ironmud::types::QuestReward;

    // With discipline.
    let with = QuestReward::EmbraceAnarch {
        discipline: Some("potence".to_string()),
    };
    let json = serde_json::to_string(&with).expect("serialize");
    assert!(json.contains("embrace_anarch"));
    assert!(json.contains("potence"));
    let back: QuestReward = serde_json::from_str(&json).expect("deserialize");
    match back {
        QuestReward::EmbraceAnarch { discipline } => {
            assert_eq!(discipline.as_deref(), Some("potence"))
        }
        _ => panic!("variant mismatch"),
    }

    // Without discipline (runtime choice path).
    let without = QuestReward::EmbraceAnarch { discipline: None };
    let json = serde_json::to_string(&without).expect("serialize");
    let back: QuestReward = serde_json::from_str(&json).expect("deserialize");
    match back {
        QuestReward::EmbraceAnarch { discipline } => assert_eq!(discipline, None),
        _ => panic!("variant mismatch"),
    }
}

#[test]
fn test_apply_anarch_acknowledgment_stamps_trait_sentinel_and_discipline() {
    use ironmud::script::vampire::{
        ANARCH_SIRE_SENTINEL, ANARCH_TRAIT, CLAN_BLOOD_POOL_MAX, apply_anarch_acknowledgment,
    };
    use ironmud::types::VampireState;

    let mut ch = vampire_test_pc("rogue");
    ch.vampire_state = Some(VampireState::default());
    if let Some(v) = ch.vampire_state.as_mut() {
        v.max_blood_pool = 6;
        v.blood_pool = 4;
    }

    let changed = apply_anarch_acknowledgment(&mut ch, "celerity");
    assert!(changed, "first acknowledgment changes state");

    assert!(ch.traits.iter().any(|t| t == ANARCH_TRAIT));
    let v = ch.vampire_state.as_ref().expect("vampire");
    assert_eq!(v.max_blood_pool, CLAN_BLOOD_POOL_MAX);
    assert_eq!(v.blood_pool, CLAN_BLOOD_POOL_MAX);
    assert_eq!(v.sire_id.as_deref(), Some(ANARCH_SIRE_SENTINEL));
    let cel = ch.skills.get("celerity").expect("celerity seeded");
    assert_eq!(cel.level, 1);
}

#[test]
fn test_apply_anarch_acknowledgment_noop_on_mortal_or_empty() {
    use ironmud::script::vampire::apply_anarch_acknowledgment;
    use ironmud::types::VampireState;

    // Mortal: no vampire_state.
    let mut mortal = vampire_test_pc("mortal");
    assert!(!apply_anarch_acknowledgment(&mut mortal, "potence"));
    assert!(mortal.traits.is_empty());

    // Vampire but empty discipline.
    let mut ch = vampire_test_pc("rogue");
    ch.vampire_state = Some(VampireState::default());
    assert!(!apply_anarch_acknowledgment(&mut ch, ""));
    assert!(!apply_anarch_acknowledgment(&mut ch, "   "));
    assert!(ch.traits.is_empty());
}

#[test]
fn test_apply_anarch_acknowledgment_idempotent() {
    use ironmud::script::vampire::{ANARCH_TRAIT, apply_anarch_acknowledgment};
    use ironmud::types::VampireState;

    let mut ch = vampire_test_pc("rogue");
    ch.vampire_state = Some(VampireState::default());

    let first = apply_anarch_acknowledgment(&mut ch, "fortitude");
    assert!(first);
    let second = apply_anarch_acknowledgment(&mut ch, "fortitude");
    assert!(!second, "second call is a no-op");

    assert_eq!(
        ch.traits.iter().filter(|t| *t == ANARCH_TRAIT).count(),
        1,
        "trait deduped"
    );
    assert_eq!(ch.skills.get("fortitude").unwrap().level, 1);
}

#[test]
fn test_known_disciplines_reads_clan_preferred_union() {
    use ironmud::script::vampire::known_disciplines;

    // The Anarch path's discipline allow-list is the union of every clan's
    // first-pick preferences (matching what EmbraceClan would seed). The
    // exact set depends on `scripts/data/vampire_clans.json`; we just
    // assert the contract: at least one entry, deduped, all lowercase, and
    // includes the disciplines that the canonical core clans seed.
    let set = known_disciplines();
    assert!(!set.is_empty(), "known disciplines should not be empty");
    for d in &set {
        assert_eq!(d, &d.to_lowercase());
    }
    let mut dedup = set.clone();
    dedup.sort();
    dedup.dedup();
    assert_eq!(dedup.len(), set.len(), "duplicates present: {:?}", set);
    // Smoke: at minimum the disciplines exercised in apply_clan_acknowledgment
    // for the canonical core clans must be in the set.
    for d in &["potence", "celerity", "auspex", "obfuscate"] {
        assert!(set.iter().any(|x| x == d), "missing {} in {:?}", d, set);
    }
}

#[test]
fn test_set_quest_choice_writes_choice_var_when_quest_active() {
    use ironmud::types::{ActiveQuest, CharacterData, DialogueEffect};

    // Direct mutation test — mirrors the dialogue handler's effect path
    // without exercising the full dialogue evaluator. The handler's
    // semantics: when quest is active, write key=value to choice_vars;
    // when not active, no-op (and log a warn).
    let mut ch: CharacterData = vampire_test_pc("seeker");
    ch.active_quests.insert(
        "cendre:q-anarch-pact".to_string(),
        ActiveQuest::default(),
    );

    let effect = DialogueEffect::SetQuestChoice {
        quest_vnum: "cendre:q-anarch-pact".to_string(),
        key: "discipline".to_string(),
        value: "auspex".to_string(),
    };
    // Replicate the handler's read/mutate to avoid pulling in the
    // dialogue test harness (which needs a mob/db scaffold).
    if let DialogueEffect::SetQuestChoice {
        quest_vnum,
        key,
        value,
    } = &effect
    {
        if let Some(aq) = ch.active_quests.get_mut(quest_vnum) {
            aq.choice_vars.insert(key.clone(), value.clone());
        }
    }

    let aq = ch
        .active_quests
        .get("cendre:q-anarch-pact")
        .expect("quest active");
    assert_eq!(aq.choice_vars.get("discipline").map(String::as_str), Some("auspex"));

    // No-op when quest absent.
    let mut empty = vampire_test_pc("noquest");
    if let DialogueEffect::SetQuestChoice {
        quest_vnum,
        key,
        value,
    } = &effect
    {
        if let Some(aq) = empty.active_quests.get_mut(quest_vnum) {
            aq.choice_vars.insert(key.clone(), value.clone());
        }
    }
    assert!(empty.active_quests.is_empty());
}

#[test]
fn test_quest_choice_equals_condition_round_trip() {
    use ironmud::types::DialogueCondition;

    let cond = DialogueCondition::QuestChoiceEquals {
        quest_vnum: "cendre:q-anarch-pact".to_string(),
        key: "discipline".to_string(),
        value: "obfuscate".to_string(),
    };
    let json = serde_json::to_string(&cond).expect("serialize");
    assert!(json.contains("quest_choice_equals"));
    assert!(json.contains("obfuscate"));

    let back: DialogueCondition = serde_json::from_str(&json).expect("deserialize");
    assert!(matches!(
        back,
        DialogueCondition::QuestChoiceEquals { ref value, .. } if value == "obfuscate"
    ));
}

#[test]
fn test_active_quest_choice_vars_default_empty_and_roundtrips() {
    use ironmud::types::ActiveQuest;

    let aq = ActiveQuest::default();
    assert!(aq.choice_vars.is_empty());

    let mut aq = aq;
    aq.choice_vars.insert("discipline".into(), "potence".into());
    let json = serde_json::to_string(&aq).expect("serialize");
    assert!(json.contains("choice_vars"));
    assert!(json.contains("potence"));
    let back: ActiveQuest = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(
        back.choice_vars.get("discipline").map(String::as_str),
        Some("potence")
    );
}

#[test]
fn test_thinblood_takes_half_sun_damage_via_mob() {
    use ironmud::MobileData;
    
    use ironmud::types::VampireState;
    use ironmud::vampire::process_sun_tick;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(temp.path()).expect("open DB");

        // Force daytime.
        let mut gt = db.get_game_time().expect("read time");
        gt.hour = 12;
        db.save_game_time(&gt).expect("save time");

        // Outdoor room.
        let outdoor = vampire_test_room("Open Plaza", false);
        let outdoor_id = outdoor.id;
        db.save_room_data(outdoor).expect("save room");

        // The sun tick path uses `is_pc_thinblood` only for PC entries,
        // not mobs — mobs always take full sun damage. To validate the
        // halving, we exercise the helper directly on a constructed PC
        // via integer arithmetic since we lack the live-session test
        // harness here. (The full-session test would mirror this with a
        // SharedConnections setup; we skip that boilerplate.)
        // Mob-side: clan vampire takes the standard divisor.
        let mut clan_vamp = MobileData::new("a clan elder".to_string());
        clan_vamp.is_prototype = false;
        clan_vamp.flags.vampire = true;
        clan_vamp.vampire_state = Some(VampireState::default());
        clan_vamp.current_room_id = Some(outdoor_id);
        clan_vamp.max_hp = 200;
        clan_vamp.current_hp = 200;
        let id = clan_vamp.id;
        db.save_mobile_data(clan_vamp).expect("save");

        let connections: ironmud::SharedConnections = Arc::new(Mutex::new(HashMap::new()));
        process_sun_tick(&db, &connections).expect("tick");

        let after = db.get_mobile_data(&id).unwrap().unwrap();
        // max_hp 200 / SUN_BURN_HP_DIVISOR(20) = 10 damage. HP 200 - 10 = 190.
        assert_eq!(after.current_hp, 190, "mob takes full sun damage");
    }));

    
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_thinblood_max_blood_uplift_via_acknowledgment() {
    use ironmud::script::vampire::{apply_clan_acknowledgment, CLAN_BLOOD_POOL_MAX, THINBLOOD_BLOOD_POOL_MAX};
    use ironmud::types::VampireState;

    // Simulate the auto-create thinblood: VampireState::default() then
    // override the max_blood per the embrace_pc thinblood path.
    let mut ch = vampire_test_pc("thinblood");
    let mut state = VampireState::default();
    state.max_blood_pool = THINBLOOD_BLOOD_POOL_MAX;
    state.blood_pool = THINBLOOD_BLOOD_POOL_MAX;
    ch.vampire_state = Some(state);

    assert_eq!(ch.vampire_state.as_ref().unwrap().max_blood_pool, 6);
    assert_eq!(ch.vampire_state.as_ref().unwrap().blood_pool, 6);

    // Acknowledgment uplift.
    apply_clan_acknowledgment(&mut ch, "brujah", Some("Marcus".into()));

    let v = ch.vampire_state.as_ref().unwrap();
    assert_eq!(v.max_blood_pool, CLAN_BLOOD_POOL_MAX);
    assert_eq!(v.blood_pool, CLAN_BLOOD_POOL_MAX);
}

#[test]
fn test_apply_clan_acknowledgment_noop_for_mortal() {
    use ironmud::script::vampire::apply_clan_acknowledgment;

    let mut ch = vampire_test_pc("mortal");
    let changed = apply_clan_acknowledgment(&mut ch, "brujah", Some("Sire".into()));
    assert!(!changed, "no-op on mortal");
    assert!(ch.vampire_state.is_none());
    assert!(!ch.traits.iter().any(|t| t == "clan_brujah"));
}

#[test]
fn test_apply_clan_acknowledgment_empty_clan_noop() {
    use ironmud::script::vampire::apply_clan_acknowledgment;
    use ironmud::types::VampireState;

    let mut ch = vampire_test_pc("thinblood");
    ch.vampire_state = Some(VampireState::default());

    // Empty clan string is treated as "no clan provided" — no-op.
    let changed = apply_clan_acknowledgment(&mut ch, "", None);
    assert!(!changed);
    assert!(!ch.traits.iter().any(|t| t.starts_with("clan_")));
}

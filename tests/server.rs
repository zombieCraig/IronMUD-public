#![recursion_limit = "512"]

use anyhow::Result;
use ironmud::{World, load_command_metadata, load_game_data, load_scripts, run_server, script, watch_scripts};
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
        class_definitions: std::collections::HashMap::new(),
        trait_definitions: std::collections::HashMap::new(),
        race_suggestions: Vec::new(),
        race_definitions: std::collections::HashMap::new(),
        recipes: std::collections::HashMap::new(),
        transports: std::collections::HashMap::new(),
        spell_definitions: std::collections::HashMap::new(),
        chat_sender: None,
        shutdown_sender: None,
        shutdown_cancel_sender: None,
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
    let expected_not_found_msg = format!("Character '{}' not found.", non_existent_char);
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

    // Match: register_bool_flags!(engine, TypeName, flag1, flag2, ...)
    let bool_flags_re = Regex::new(r"register_bool_flags!\s*\(\s*\w+\s*,\s*(\w+)\s*,\s*([\s\S]*?)\);").unwrap();

    for entry in glob("src/script/**/*.rs").expect("Failed to glob src/script") {
        if let Ok(path) = entry {
            if let Ok(content) = fs::read_to_string(&path) {
                for cap in register_get_re.captures_iter(&content) {
                    getters_by_type
                        .entry(cap[2].to_string())
                        .or_default()
                        .insert(cap[1].to_string());
                }
                for cap in bool_flags_re.captures_iter(&content) {
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
    let setter_re = Regex::new(r#"register_set\s*\(\s*"([^"]+)""#).unwrap();
    let mut registered_setters: HashSet<String> = HashSet::new();

    for entry in glob("src/script/**/*.rs").expect("glob src/script") {
        if let Ok(path) = entry {
            if let Ok(content) = fs::read_to_string(&path) {
                for cap in setter_re.captures_iter(&content) {
                    registered_setters.insert(cap[1].to_string());
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

/// Extract bool field names from a Rust struct defined in src/types/mod.rs.
fn extract_bool_field_names(struct_name: &str) -> Vec<String> {
    use regex::Regex;
    let types_content = std::fs::read_to_string("src/types/mod.rs").expect("read src/types/mod.rs");

    let struct_re = Regex::new(&format!(r"pub struct {} \{{([^}}]+)\}}", struct_name)).unwrap();
    let struct_body = struct_re
        .captures(&types_content)
        .unwrap_or_else(|| panic!("{} struct not found", struct_name))
        .get(1)
        .unwrap()
        .as_str()
        .to_string();

    let field_re = Regex::new(r"pub\s+(\w+)\s*:\s*bool").unwrap();
    field_re.captures_iter(&struct_body).map(|c| c[1].to_string()).collect()
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
    let db_path = format!("test_seed_bidir_{}.db", std::process::id());
    let _ = std::fs::remove_dir_all(&db_path);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(&db_path).expect("open DB");
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

    let _ = std::fs::remove_dir_all(&db_path);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_seed_demo_world() {
    // Use a unique temp DB to avoid conflicts with other tests
    let db_path = format!("test_seed_{}.db", std::process::id());
    // Clean up any leftover DB from a previous failed run
    let _ = std::fs::remove_dir_all(&db_path);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = ironmud::db::Db::open(&db_path).expect("open DB");

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

    // Clean up temp DB
    let _ = std::fs::remove_dir_all(&db_path);

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

    fn open_temp_db(tag: &str) -> (Db, String) {
        let path = format!("test_migration_{}_{}.db", tag, std::process::id());
        let _ = std::fs::remove_dir_all(&path);
        let db = Db::open(&path).expect("open DB");
        (db, path)
    }

    fn cleanup(path: &str) {
        let _ = std::fs::remove_dir_all(path);
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
        let (db, path) = open_temp_db("spawn");
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
        cleanup(&path);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_migration_skips_when_no_capacity() {
        let (db, path) = open_temp_db("nocap");
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
        cleanup(&path);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_migration_respects_max_per_check() {
        let (db, path) = open_temp_db("maxcheck");
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
        cleanup(&path);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_migration_respects_interval() {
        let (db, path) = open_temp_db("interval");
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
        cleanup(&path);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_death_releases_residency() {
        let (db, path) = open_temp_db("death");
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
        cleanup(&path);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_guard_variation_never_when_chance_zero() {
        let (db, path) = open_temp_db("variation_zero");
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
        cleanup(&path);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_guard_variation_always_when_chance_one() {
        use ironmud::types::ActivityState;
        let (db, path) = open_temp_db("variation_one");
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
        cleanup(&path);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_guard_variation_keywords_include_guard() {
        let (db, path) = open_temp_db("variation_kw");
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
        cleanup(&path);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_healer_variation_never_when_chance_zero() {
        let (db, path) = open_temp_db("healer_zero");
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
        cleanup(&path);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_healer_variation_always_when_chance_one() {
        use ironmud::types::ActivityState;
        let (db, path) = open_temp_db("healer_one");
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
        cleanup(&path);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_healer_variation_keywords_include_healer() {
        let (db, path) = open_temp_db("healer_kw");
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
        cleanup(&path);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_scavenger_variation_never_when_chance_zero() {
        let (db, path) = open_temp_db("scav_zero");
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
        cleanup(&path);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_scavenger_variation_always_when_chance_one() {
        use ironmud::types::ActivityState;
        let (db, path) = open_temp_db("scav_one");
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
        cleanup(&path);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_scavenger_variation_keywords_include_scavenger() {
        let (db, path) = open_temp_db("scav_kw");
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
        cleanup(&path);
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

        let (db, path) = open_temp_db("aging_advance");
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
        cleanup(&path);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_aging_tick_is_day_gated() {
        use ironmud::aging::process_aging_tick;
        use ironmud::types::{Characteristics, MobileData};

        let (db, path) = open_temp_db("aging_gated");
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
        cleanup(&path);
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

        let (db, path) = open_temp_db("aging_death");
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
        cleanup(&path);
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
        let (db, path) = open_temp_db("grief_family");
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
        cleanup(&path);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_hated_parent_death_skips_grief() {
        use ironmud::types::{MobileData, Relationship, RelationshipKind, SocialState};

        let (db, path) = open_temp_db("grief_hated");
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
        cleanup(&path);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_set_family_relationship_writes_both_directions() {
        use ironmud::social::{FamilyError, set_family_relationship};
        use ironmud::types::{MobileData, RelationshipKind};

        let (db, path) = open_temp_db("family_set");
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
        cleanup(&path);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_partner_monogamy_blocks_second_partner() {
        use ironmud::social::{FamilyError, set_family_relationship};
        use ironmud::types::MobileData;

        let (db, path) = open_temp_db("monogamy");
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
        cleanup(&path);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_cohabitant_promotion_mints_shared_household() {
        use ironmud::migration::process_pair_housing;
        use ironmud::types::{Characteristics, MobileData, Relationship, RelationshipKind, SocialState};

        let (db, path) = open_temp_db("cohab_household");
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
        cleanup(&path);
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

        let (db, path) = open_temp_db("pregnancy");
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
        cleanup(&path);
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

        let (db, path) = open_temp_db("no_conceive");
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
        cleanup(&path);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_orphan_flagged_on_last_parent_death() {
        use ironmud::types::{Characteristics, MobileData, Relationship, RelationshipKind, SocialState};

        let (db, path) = open_temp_db("orphan_flag");
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
        cleanup(&path);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_orphan_adult_child_not_flagged() {
        // An adult child who loses a parent doesn't need an adopter.
        use ironmud::types::{Characteristics, MobileData, Relationship, RelationshipKind, SocialState};

        let (db, path) = open_temp_db("orphan_adult");
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
        cleanup(&path);
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

        let (db, path) = open_temp_db("orphan_adopt");
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
        cleanup(&path);
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

        let (db, path) = open_temp_db("adopt_weight");
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
        cleanup(&path);
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

        let (db, path) = open_temp_db("examine_cues");
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
        cleanup(&path);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_migration_spawns_parent_child_family() {
        use ironmud::types::{ImmigrationFamilyChance, RelationshipKind};

        let (db, path) = open_temp_db("family_spawn_pc");
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
        cleanup(&path);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_migration_spawns_sibling_pair() {
        use ironmud::types::{ImmigrationFamilyChance, RelationshipKind};

        let (db, path) = open_temp_db("family_spawn_sibs");
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
        cleanup(&path);
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

        let (db, path) = open_temp_db("grief_prune");
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
        cleanup(&path);
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
fn test_item_note_content_persists() {
    use ironmud::ItemData;
    use ironmud::db::Db;

    let db_path = format!("test_note_content_{}.db", std::process::id());
    let _ = std::fs::remove_dir_all(&db_path);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(&db_path).expect("open DB");

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

    let _ = std::fs::remove_dir_all(&db_path);
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
    use ironmud::db::Db;
    use ironmud::types::{
        AreaData, AreaFlags, AreaPermission, CombatZoneType, ImmigrationFamilyChance, ImmigrationVariationChances,
        RoomData, RoomExits, RoomFlags, WaterType,
    };
    use std::collections::HashMap as StdHashMap;

    let db_path = format!("test_area_default_flags_{}.db", std::process::id());
    let _ = std::fs::remove_dir_all(&db_path);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(&db_path).expect("open DB");

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
        };
        let room_id = room.id;
        db.save_room_data(room).expect("save room");
        let loaded = db.get_room_data(&room_id).expect("get").expect("present");
        assert!(loaded.flags.indoors);
        assert!(loaded.flags.no_windows);
        assert!(loaded.flags.dark);
    }));

    let _ = std::fs::remove_dir_all(&db_path);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
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
    use ironmud::db::Db;
    use ironmud::types::DoorState;

    let db_path = format!("test_buried_lock_vnums_{}.db", std::process::id());
    let _ = std::fs::remove_dir_all(&db_path);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(&db_path).expect("open DB");

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

    let _ = std::fs::remove_dir_all(&db_path);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_spawn_point_bury_on_spawn_field_persists() {
    use ironmud::db::Db;
    use ironmud::types::{SpawnEntityType, SpawnPointData};
    use uuid::Uuid;

    let db_path = format!("test_spawn_point_bury_{}.db", std::process::id());
    let _ = std::fs::remove_dir_all(&db_path);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(&db_path).expect("open DB");

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
        };
        let sp_no_id = sp_no_bury.id;
        db.save_spawn_point(sp_no_bury).expect("save");
        let loaded_no = db.get_spawn_point(&sp_no_id).expect("get").expect("present");
        assert!(!loaded_no.bury_on_spawn, "bury_on_spawn=false persists",);
    }));

    let _ = std::fs::remove_dir_all(&db_path);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_item_unique_flag_caps_spawn_at_one() {
    use ironmud::ItemData;
    use ironmud::db::Db;

    let db_path = format!("test_item_unique_cap_{}.db", std::process::id());
    let _ = std::fs::remove_dir_all(&db_path);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(&db_path).expect("open DB");

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

    let _ = std::fs::remove_dir_all(&db_path);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_mobile_world_max_count_caps_spawn() {
    use ironmud::db::Db;
    use ironmud::types::MobileData;

    let db_path = format!("test_mob_world_cap_{}.db", std::process::id());
    let _ = std::fs::remove_dir_all(&db_path);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(&db_path).expect("open DB");

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

    let _ = std::fs::remove_dir_all(&db_path);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_sanctuary_buff_on_prototype_carries_to_spawn() {
    use ironmud::db::Db;
    use ironmud::types::{ActiveBuff, EffectType, MobileData};

    let db_path = format!("test_sanctuary_proto_{}.db", std::process::id());
    let _ = std::fs::remove_dir_all(&db_path);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(&db_path).expect("open DB");

        let mut proto = MobileData::new("a glowing wisp".to_string());
        proto.is_prototype = true;
        proto.vnum = "test:wisp".to_string();
        proto.active_buffs.push(ActiveBuff {
            effect_type: EffectType::DamageReduction,
            magnitude: 50,
            remaining_secs: -1,
            source: "innate sanctuary".to_string(),
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

    let _ = std::fs::remove_dir_all(&db_path);
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

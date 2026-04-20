// src/script/ai.rs
// AI integration functions for Matrix, Claude, and Gemini

use rhai::Engine;
use crate::chat::{ChatMessage, ChatSender};
use crate::claude::{ClaudeSender, ClaudeRequest, DescriptionType, DescriptionContext, AiDescriptionTarget, AiTargetType};
use crate::gemini::{GeminiSender, GeminiRequest};
use crate::SharedConnections;

/// Register chat integration functions in the Rhai engine
/// This is called after the engine is created to add chat_broadcast and matrix_broadcast functions
pub fn set_chat_sender(engine: &mut Engine, sender: ChatSender) {
    // Register chat_broadcast function for Rhai scripts
    let sender2 = sender.clone();
    engine.register_fn("chat_broadcast", move |message: String| {
        let _ = sender.send(ChatMessage::Broadcast(message));
    });
    // Keep matrix_broadcast as backward-compatible alias
    engine.register_fn("matrix_broadcast", move |message: String| {
        let _ = sender2.send(ChatMessage::Broadcast(message));
    });
}

/// Register Claude AI integration functions in the Rhai engine
pub fn set_claude_sender(engine: &mut Engine, sender: ClaudeSender, connections: SharedConnections) {
    // ai_is_enabled() -> bool - Check if Claude integration is configured
    engine.register_fn("ai_is_enabled", || -> bool {
        std::env::var("CLAUDE_API_KEY").is_ok()
    });

    // ai_help_write(conn_id, type, prompt, context) -> request_id string (empty on error)
    let sender_clone = sender.clone();
    let conns = connections.clone();
    engine.register_fn("ai_help_write", move |
        connection_id: String,
        desc_type: String,
        prompt: String,
        context: rhai::Map,
    | -> String {
        let conn_id = match uuid::Uuid::parse_str(&connection_id) {
            Ok(id) => id,
            Err(_) => return String::new(),
        };

        let dtype = match DescriptionType::from_str(&desc_type) {
            Some(t) => t,
            None => return String::new(),
        };

        let ctx = DescriptionContext {
            entity_name: context.get("entity_name").and_then(|v| v.clone().into_string().ok()),
            room_title: context.get("room_title").and_then(|v| v.clone().into_string().ok()),
            area_name: context.get("area_name").and_then(|v| v.clone().into_string().ok()),
            entity_type: context.get("entity_type").and_then(|v| v.clone().into_string().ok()),
            theme: context.get("theme").and_then(|v| v.clone().into_string().ok()),
        };

        let request_id = uuid::Uuid::new_v4();

        // Store pending request in session
        {
            let mut conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get_mut(&conn_id) {
                session.pending_ai_request = Some(request_id);
            }
        }

        let request = ClaudeRequest::HelpMeWrite {
            request_id,
            connection_id: conn_id,
            desc_type: dtype,
            prompt,
            context: ctx,
        };

        if sender_clone.send(request).is_ok() {
            request_id.to_string()
        } else {
            String::new()
        }
    });

    // ai_rephrase(conn_id, type, existing_text, context) -> request_id string (empty on error)
    let sender_clone = sender.clone();
    let conns = connections.clone();
    engine.register_fn("ai_rephrase", move |
        connection_id: String,
        desc_type: String,
        existing_text: String,
        context: rhai::Map,
    | -> String {
        let conn_id = match uuid::Uuid::parse_str(&connection_id) {
            Ok(id) => id,
            Err(_) => return String::new(),
        };

        let dtype = match DescriptionType::from_str(&desc_type) {
            Some(t) => t,
            None => return String::new(),
        };

        let ctx = DescriptionContext {
            entity_name: context.get("entity_name").and_then(|v| v.clone().into_string().ok()),
            room_title: context.get("room_title").and_then(|v| v.clone().into_string().ok()),
            area_name: context.get("area_name").and_then(|v| v.clone().into_string().ok()),
            entity_type: context.get("entity_type").and_then(|v| v.clone().into_string().ok()),
            theme: context.get("theme").and_then(|v| v.clone().into_string().ok()),
        };

        let request_id = uuid::Uuid::new_v4();

        // Store pending request in session
        {
            let mut conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get_mut(&conn_id) {
                session.pending_ai_request = Some(request_id);
            }
        }

        let request = ClaudeRequest::Rephrase {
            request_id,
            connection_id: conn_id,
            desc_type: dtype,
            existing_text,
            context: ctx,
        };

        if sender_clone.send(request).is_ok() {
            request_id.to_string()
        } else {
            String::new()
        }
    });

    // ai_set_target(conn_id, target_type, entity_id, field) -> bool
    let conns = connections.clone();
    engine.register_fn("ai_set_target", move |
        connection_id: String,
        target_type: String,
        entity_id: String,
        field: String,
    | -> bool {
        let conn_id = match uuid::Uuid::parse_str(&connection_id) {
            Ok(id) => id,
            Err(_) => return false,
        };

        let entity_uuid = match uuid::Uuid::parse_str(&entity_id) {
            Ok(id) => id,
            Err(_) => return false,
        };

        let ttype = match AiTargetType::from_str(&target_type) {
            Some(t) => t,
            None => return false,
        };

        let target = AiDescriptionTarget {
            target_type: ttype,
            entity_id: entity_uuid,
            field,
        };

        let mut conns_lock = conns.lock().unwrap();
        if let Some(session) = conns_lock.get_mut(&conn_id) {
            session.pending_ai_target = Some(target);
            true
        } else {
            false
        }
    });

    // ai_clear_pending(conn_id) - Clear all pending AI state
    let conns = connections.clone();
    engine.register_fn("ai_clear_pending", move |connection_id: String| {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get_mut(&conn_id) {
                session.pending_ai_request = None;
                session.pending_ai_response = None;
                session.pending_ai_target = None;
            }
        }
    });
}

/// Register Gemini AI integration functions in the Rhai engine
pub fn set_gemini_sender(engine: &mut Engine, sender: GeminiSender, connections: SharedConnections) {
    // ai_is_enabled() -> bool - Check if Gemini integration is configured
    engine.register_fn("ai_is_enabled", || -> bool {
        std::env::var("GEMINI_API_KEY").is_ok()
    });

    // ai_help_write(conn_id, type, prompt, context) -> request_id string (empty on error)
    let sender_clone = sender.clone();
    let conns = connections.clone();
    engine.register_fn("ai_help_write", move |
        connection_id: String,
        desc_type: String,
        prompt: String,
        context: rhai::Map,
    | -> String {
        let conn_id = match uuid::Uuid::parse_str(&connection_id) {
            Ok(id) => id,
            Err(_) => return String::new(),
        };

        let dtype = match DescriptionType::from_str(&desc_type) {
            Some(t) => t,
            None => return String::new(),
        };

        let ctx = DescriptionContext {
            entity_name: context.get("entity_name").and_then(|v| v.clone().into_string().ok()),
            room_title: context.get("room_title").and_then(|v| v.clone().into_string().ok()),
            area_name: context.get("area_name").and_then(|v| v.clone().into_string().ok()),
            entity_type: context.get("entity_type").and_then(|v| v.clone().into_string().ok()),
            theme: context.get("theme").and_then(|v| v.clone().into_string().ok()),
        };

        let request_id = uuid::Uuid::new_v4();

        // Store pending request in session
        {
            let mut conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get_mut(&conn_id) {
                session.pending_ai_request = Some(request_id);
            }
        }

        let request = GeminiRequest::HelpMeWrite {
            request_id,
            connection_id: conn_id,
            desc_type: dtype,
            prompt,
            context: ctx,
        };

        if sender_clone.send(request).is_ok() {
            request_id.to_string()
        } else {
            String::new()
        }
    });

    // ai_rephrase(conn_id, type, existing_text, context) -> request_id string (empty on error)
    let sender_clone = sender.clone();
    let conns = connections.clone();
    engine.register_fn("ai_rephrase", move |
        connection_id: String,
        desc_type: String,
        existing_text: String,
        context: rhai::Map,
    | -> String {
        let conn_id = match uuid::Uuid::parse_str(&connection_id) {
            Ok(id) => id,
            Err(_) => return String::new(),
        };

        let dtype = match DescriptionType::from_str(&desc_type) {
            Some(t) => t,
            None => return String::new(),
        };

        let ctx = DescriptionContext {
            entity_name: context.get("entity_name").and_then(|v| v.clone().into_string().ok()),
            room_title: context.get("room_title").and_then(|v| v.clone().into_string().ok()),
            area_name: context.get("area_name").and_then(|v| v.clone().into_string().ok()),
            entity_type: context.get("entity_type").and_then(|v| v.clone().into_string().ok()),
            theme: context.get("theme").and_then(|v| v.clone().into_string().ok()),
        };

        let request_id = uuid::Uuid::new_v4();

        // Store pending request in session
        {
            let mut conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get_mut(&conn_id) {
                session.pending_ai_request = Some(request_id);
            }
        }

        let request = GeminiRequest::Rephrase {
            request_id,
            connection_id: conn_id,
            desc_type: dtype,
            existing_text,
            context: ctx,
        };

        if sender_clone.send(request).is_ok() {
            request_id.to_string()
        } else {
            String::new()
        }
    });

    // ai_set_target(conn_id, target_type, entity_id, field) -> bool
    let conns = connections.clone();
    engine.register_fn("ai_set_target", move |
        connection_id: String,
        target_type: String,
        entity_id: String,
        field: String,
    | -> bool {
        let conn_id = match uuid::Uuid::parse_str(&connection_id) {
            Ok(id) => id,
            Err(_) => return false,
        };

        let entity_uuid = match uuid::Uuid::parse_str(&entity_id) {
            Ok(id) => id,
            Err(_) => return false,
        };

        let ttype = match AiTargetType::from_str(&target_type) {
            Some(t) => t,
            None => return false,
        };

        let target = AiDescriptionTarget {
            target_type: ttype,
            entity_id: entity_uuid,
            field,
        };

        let mut conns_lock = conns.lock().unwrap();
        if let Some(session) = conns_lock.get_mut(&conn_id) {
            session.pending_ai_target = Some(target);
            true
        } else {
            false
        }
    });

    // ai_clear_pending(conn_id) - Clear all pending AI state
    let conns = connections.clone();
    engine.register_fn("ai_clear_pending", move |connection_id: String| {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get_mut(&conn_id) {
                session.pending_ai_request = None;
                session.pending_ai_response = None;
                session.pending_ai_target = None;
            }
        }
    });
}

/// Register stub AI functions when no AI backend is configured
/// These provide default behavior (ai_is_enabled returns false, etc.)
#[allow(dead_code)]
pub fn register_stubs(engine: &mut Engine) {
    // ai_is_enabled() -> bool - Always returns false when no AI configured
    engine.register_fn("ai_is_enabled", || -> bool {
        false
    });
}

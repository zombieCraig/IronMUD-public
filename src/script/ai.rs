// src/script/ai.rs
// AI integration functions for Matrix, Claude, and Gemini

use crate::SharedConnections;
use crate::ai_guard;
use crate::chat::{ChatMessage, ChatSender};
use crate::claude::{AiDescriptionTarget, AiJob, AiSender, AiTargetType, DescriptionContext, DescriptionType};
use rhai::Engine;

/// Register chat integration functions in the Rhai engine
/// This is called after the engine is created to add chat_broadcast and matrix_broadcast functions
pub fn set_chat_sender(engine: &mut Engine, sender: ChatSender) {
    // Mirror the chat sender into the email module so the quota path can
    // broadcast a one-shot admin warning when the daily/monthly cap is hit.
    crate::email::set_email_chat_sender(sender.clone());

    // Register chat_broadcast function for Rhai scripts
    let sender2 = sender.clone();
    engine.register_fn("chat_broadcast", move |message: String| {
        let _ = sender.send(ChatMessage::Broadcast(message));
    });
    // Keep matrix_broadcast as backward-compatible alias
    let sender3 = sender2.clone();
    engine.register_fn("matrix_broadcast", move |message: String| {
        let _ = sender2.send(ChatMessage::Broadcast(message));
    });
    // chat_broadcast_limited(key, secs, message) -> bool: broadcast unless
    // `key` already broadcast within `secs` seconds. For player-driven relays
    // (`pray`) so one player can't spend the bridge's shared outbound budget.
    engine.register_fn(
        "chat_broadcast_limited",
        move |key: String, secs: i64, message: String| -> bool {
            let gap = std::time::Duration::from_secs(secs.clamp(0, 86_400) as u64);
            if !crate::chat::cooldown_ok(&format!("relay\0{key}"), gap) {
                return false;
            }
            let _ = sender3.send(ChatMessage::Broadcast(message));
            true
        },
    );
}

/// Register the AI Rhai functions. `sender` is `None` when no provider is
/// running (no key, or both keys set): `ai_is_enabled()` is then false and
/// every request is refused.
pub fn set_ai_sender(engine: &mut Engine, sender: Option<AiSender>, connections: SharedConnections) {
    let enabled = sender.is_some();
    register_ai_functions(engine, enabled, connections, move |job: AiJob| {
        sender.as_ref().is_some_and(|tx| tx.send(job).is_ok())
    });
}

fn context_from_map(context: &rhai::Map) -> DescriptionContext {
    let field = |k: &str| {
        context
            .get(k)
            .and_then(|v| v.clone().into_string().ok())
            .map(|v| ai_guard::cap(&crate::session::sanitize_player_text(&v), ai_guard::MAX_CONTEXT_CHARS))
    };
    DescriptionContext {
        entity_name: field("entity_name"),
        room_title: field("room_title"),
        area_name: field("area_name"),
        entity_type: field("entity_type"),
        theme: field("theme"),
        season: field("season"),
        base_description: context
            .get("base_description")
            .and_then(|v| v.clone().into_string().ok())
            .map(|v| ai_guard::cap(&v, ai_guard::MAX_EXISTING_CHARS)),
    }
}

/// Shared Rhai surface for every AI provider. `submit` hands a job to the
/// provider's background task and reports whether it was queued. All the
/// limits in `crate::ai_guard` are applied here, once for every provider.
fn register_ai_functions(
    engine: &mut Engine,
    enabled: bool,
    connections: SharedConnections,
    submit: impl Fn(AiJob) -> bool + Send + Sync + 'static,
) {
    // ai_is_enabled() -> bool - Check if the provider is configured
    engine.register_fn("ai_is_enabled", move || -> bool { enabled });

    let submit = std::sync::Arc::new(submit);
    let start = {
        let conns = connections.clone();
        let submit = submit.clone();
        move |connection_id: String, desc_type: String, text: String, context: rhai::Map, rephrase: bool| -> String {
            let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) else {
                return String::new();
            };
            let Some(dtype) = DescriptionType::from_str(&desc_type) else {
                return String::new();
            };
            let who = {
                let c = conns.lock();
                match c.get(&conn_id) {
                    Some(s) => s
                        .character
                        .as_ref()
                        .map(|ch| ch.name.clone())
                        .unwrap_or_else(|| conn_id.to_string()),
                    None => return String::new(),
                }
            };
            if let Err(why) = ai_guard::begin(conn_id, &who) {
                crate::send_client_message(&conns, conn_id.to_string(), format!("{why}\n"));
                return String::new();
            }
            let max = if rephrase {
                ai_guard::MAX_EXISTING_CHARS
            } else {
                ai_guard::MAX_PROMPT_CHARS
            };
            let request_id = uuid::Uuid::new_v4();
            {
                let mut c = conns.lock();
                if let Some(session) = c.get_mut(&conn_id) {
                    session.pending_ai_request = Some(request_id);
                    session.pending_ai_response = None;
                }
            }
            let job = AiJob {
                request_id,
                connection_id: conn_id,
                desc_type: dtype,
                text: ai_guard::cap(&text, max),
                rephrase,
                context: context_from_map(&context),
            };
            if submit(job) {
                request_id.to_string()
            } else {
                ai_guard::finish(conn_id);
                String::new()
            }
        }
    };

    // ai_help_write(conn_id, type, prompt, context) -> request_id string (empty on error)
    let s1 = start.clone();
    engine.register_fn(
        "ai_help_write",
        move |connection_id: String, desc_type: String, prompt: String, context: rhai::Map| -> String {
            s1(connection_id, desc_type, prompt, context, false)
        },
    );

    // ai_rephrase(conn_id, type, existing_text, context) -> request_id string (empty on error)
    engine.register_fn(
        "ai_rephrase",
        move |connection_id: String, desc_type: String, existing_text: String, context: rhai::Map| -> String {
            start(connection_id, desc_type, existing_text, context, true)
        },
    );

    // ai_set_target(conn_id, target_type, entity_id, field) -> bool
    let conns = connections.clone();
    engine.register_fn(
        "ai_set_target",
        move |connection_id: String, target_type: String, entity_id: String, field: String| -> bool {
            let (Ok(conn_id), Ok(entity_uuid)) =
                (uuid::Uuid::parse_str(&connection_id), uuid::Uuid::parse_str(&entity_id))
            else {
                return false;
            };
            let Some(ttype) = AiTargetType::from_str(&target_type) else {
                return false;
            };
            let target = AiDescriptionTarget {
                target_type: ttype,
                entity_id: entity_uuid,
                field,
            };
            let mut conns_lock = conns.lock();
            if let Some(session) = conns_lock.get_mut(&conn_id) {
                session.pending_ai_target = Some(target);
                true
            } else {
                false
            }
        },
    );

    // ai_clear_pending(conn_id) - Clear all pending AI state
    let conns = connections.clone();
    engine.register_fn("ai_clear_pending", move |connection_id: String| {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns_lock = conns.lock();
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
    engine.register_fn("ai_is_enabled", || -> bool { false });
}

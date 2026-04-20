//! Claude AI integration module for IronMUD
//!
//! Provides AI-assisted description writing for OLC editors.
//! Uses message-passing to handle async API calls from sync Rhai scripts.

use serde::{Deserialize, Serialize};
use std::env;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::SharedConnections;

/// Configuration for Claude API, loaded from environment variables
#[derive(Clone)]
pub struct ClaudeConfig {
    pub api_key: String,
    pub model: String,
    pub max_tokens: u32,
}

impl ClaudeConfig {
    /// Load configuration from environment variables.
    /// Returns None if CLAUDE_API_KEY is missing.
    pub fn from_env() -> Option<Self> {
        let api_key = env::var("CLAUDE_API_KEY").ok()?;
        let model = env::var("CLAUDE_MODEL").unwrap_or_else(|_| "claude-sonnet-4-20250514".to_string());
        let max_tokens = env::var("CLAUDE_MAX_TOKENS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1024);

        Some(Self {
            api_key,
            model,
            max_tokens,
        })
    }
}

/// Types of AI description requests
#[derive(Debug, Clone)]
pub enum DescriptionType {
    RoomDescription,
    MobileShortDesc,
    MobileLongDesc,
    ItemShortDesc,
    ItemLongDesc,
}

impl DescriptionType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "room_desc" => Some(Self::RoomDescription),
            "mob_short" => Some(Self::MobileShortDesc),
            "mob_long" => Some(Self::MobileLongDesc),
            "item_short" => Some(Self::ItemShortDesc),
            "item_long" => Some(Self::ItemLongDesc),
            _ => None,
        }
    }
}

/// Context about the entity being described (for better prompts)
#[derive(Debug, Clone, Default)]
pub struct DescriptionContext {
    pub entity_name: Option<String>,
    pub room_title: Option<String>,
    pub area_name: Option<String>,
    pub entity_type: Option<String>,
    pub theme: Option<String>,
}

/// Request types that can be sent to the Claude background task
#[derive(Debug)]
pub enum ClaudeRequest {
    /// Generate a new description from a prompt
    HelpMeWrite {
        request_id: Uuid,
        connection_id: Uuid,
        desc_type: DescriptionType,
        prompt: String,
        context: DescriptionContext,
    },
    /// Rephrase an existing description
    Rephrase {
        request_id: Uuid,
        connection_id: Uuid,
        desc_type: DescriptionType,
        existing_text: String,
        context: DescriptionContext,
    },
}

/// AI-suggested extra description for rooms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedExtraDesc {
    pub keywords: Vec<String>,
    pub description: String,
}

/// Response from AI provider, delivered back to the connection
#[derive(Debug, Clone)]
pub struct AiResponse {
    pub request_id: Uuid,
    pub connection_id: Uuid,
    pub success: bool,
    pub description: Option<String>,
    pub extra_descs: Vec<SuggestedExtraDesc>,
    pub error: Option<String>,
}

/// Target for AI-generated description
#[derive(Debug, Clone)]
pub struct AiDescriptionTarget {
    pub target_type: AiTargetType,
    pub entity_id: Uuid,
    pub field: String,
}

#[derive(Debug, Clone)]
pub enum AiTargetType {
    Room,
    Mobile,
    Item,
}

impl AiTargetType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "room" => Some(Self::Room),
            "mobile" => Some(Self::Mobile),
            "item" => Some(Self::Item),
            _ => None,
        }
    }
}

/// Sender handle for game code to send requests to Claude
pub type ClaudeSender = mpsc::UnboundedSender<ClaudeRequest>;

/// Run the Claude integration background task
pub async fn run_claude_task(
    config: ClaudeConfig,
    connections: SharedConnections,
    mut rx: mpsc::UnboundedReceiver<ClaudeRequest>,
) {
    info!("Starting Claude AI integration task");

    // Create HTTP client for API calls
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("Failed to create HTTP client");

    while let Some(request) = rx.recv().await {
        let config = config.clone();
        let client = client.clone();
        let connections = connections.clone();

        // Spawn a task for each request to avoid blocking the receiver
        tokio::spawn(async move {
            let response = process_request(&config, &client, request).await;
            deliver_response(&connections, response);
        });
    }

    info!("Claude AI integration task shutting down");
}

async fn process_request(config: &ClaudeConfig, client: &reqwest::Client, request: ClaudeRequest) -> AiResponse {
    match request {
        ClaudeRequest::HelpMeWrite {
            request_id,
            connection_id,
            desc_type,
            prompt,
            context,
        } => {
            let system_prompt = build_system_prompt(&desc_type, &context);
            let user_prompt = build_help_me_write_prompt(&desc_type, &prompt, &context);

            match call_claude_api(config, client, &system_prompt, &user_prompt).await {
                Ok(text) => parse_ai_response(request_id, connection_id, &desc_type, &text),
                Err(e) => AiResponse {
                    request_id,
                    connection_id,
                    success: false,
                    description: None,
                    extra_descs: vec![],
                    error: Some(e.to_string()),
                },
            }
        }
        ClaudeRequest::Rephrase {
            request_id,
            connection_id,
            desc_type,
            existing_text,
            context,
        } => {
            let system_prompt = build_system_prompt(&desc_type, &context);
            let user_prompt = build_rephrase_prompt(&desc_type, &existing_text, &context);

            match call_claude_api(config, client, &system_prompt, &user_prompt).await {
                Ok(text) => parse_ai_response(request_id, connection_id, &desc_type, &text),
                Err(e) => AiResponse {
                    request_id,
                    connection_id,
                    success: false,
                    description: None,
                    extra_descs: vec![],
                    error: Some(e.to_string()),
                },
            }
        }
    }
}

#[derive(Serialize)]
struct ApiRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<ApiMessage>,
}

#[derive(Serialize)]
struct ApiMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ApiResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: Option<String>,
}

#[derive(Deserialize)]
struct ApiError {
    error: ApiErrorDetail,
}

#[derive(Deserialize)]
struct ApiErrorDetail {
    message: String,
}

async fn call_claude_api(
    config: &ClaudeConfig,
    client: &reqwest::Client,
    system_prompt: &str,
    user_prompt: &str,
) -> anyhow::Result<String> {
    let request_body = ApiRequest {
        model: config.model.clone(),
        max_tokens: config.max_tokens,
        system: system_prompt.to_string(),
        messages: vec![ApiMessage {
            role: "user".to_string(),
            content: user_prompt.to_string(),
        }],
    };

    debug!("Calling Claude API with model: {}", config.model);

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &config.api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await?;

    if !status.is_success() {
        // Try to parse error message
        if let Ok(error) = serde_json::from_str::<ApiError>(&body) {
            return Err(anyhow::anyhow!("API error: {}", error.error.message));
        }
        return Err(anyhow::anyhow!("API error ({}): {}", status, body));
    }

    let api_response: ApiResponse = serde_json::from_str(&body)?;

    // Extract text from first content block
    let text = api_response
        .content
        .into_iter()
        .find_map(|block| block.text)
        .ok_or_else(|| anyhow::anyhow!("No text content in response"))?;

    Ok(text)
}

fn deliver_response(connections: &SharedConnections, response: AiResponse) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&response.connection_id) {
        // Store the response for the Rhai script to pick up
        session.pending_ai_response = Some(response.clone());

        // Set OLC mode to ai_confirm to capture y/n input
        session.olc_mode = Some("ai_confirm".to_string());

        // Format and send the draft to the client
        let mut message = String::new();
        message.push_str("\n=== AI Generated Description ===\n");

        if let Some(ref desc) = response.description {
            message.push_str(desc);
            message.push('\n');
        }

        if !response.extra_descs.is_empty() {
            message.push_str("\n[Suggested Extra Descriptions:]\n");
            for (i, extra) in response.extra_descs.iter().enumerate() {
                message.push_str(&format!(
                    "  {}. Keywords: {}\n     {}\n",
                    i + 1,
                    extra.keywords.join(", "),
                    extra.description.replace('\n', "\n     ")
                ));
            }
        }

        if let Some(ref error) = response.error {
            message.push_str(&format!("\nError: {}\n", error));
            // Clear OLC mode on error - no confirmation needed
            session.olc_mode = None;
        } else {
            message.push_str("\nAccept this description? (y/n)\n");
        }

        let _ = session.sender.send(message);
    } else {
        warn!(
            "Connection {} not found when delivering AI response",
            response.connection_id
        );
    }
}

fn build_system_prompt(desc_type: &DescriptionType, _context: &DescriptionContext) -> String {
    match desc_type {
        DescriptionType::RoomDescription => {
            r#"You are a skilled MUD (Multi-User Dungeon) description writer. Write evocative, atmospheric room descriptions in second person ("You see...", "You are standing in...").

Keep descriptions to 2-4 sentences. Focus on sensory details (sight, sound, smell, touch). Do not include exits or directions.

For rooms, also suggest 2-3 extra descriptions that players can examine with keywords. These should describe notable features mentioned in the main description.

Return your response in EXACTLY this format:

DESCRIPTION:
<main room description here>

EXTRA:
KEYWORDS: <comma-separated keywords>
<extra description text>

EXTRA:
KEYWORDS: <comma-separated keywords>
<extra description text>"#
                .to_string()
        }
        DescriptionType::MobileShortDesc => {
            r#"You are a skilled MUD description writer. Write a short description for an NPC/mobile.

Short descriptions appear in room listings and should be ONE sentence describing the NPC in third person, ending with what they're doing (e.g., "A grizzled town guard stands here." or "A hooded merchant examines his wares.").

Return ONLY the short description, no formatting or labels."#
                .to_string()
        }
        DescriptionType::MobileLongDesc => {
            r#"You are a skilled MUD description writer. Write a long/detailed description for an NPC/mobile.

Long descriptions appear when players examine the NPC. Write 2-3 sentences describing their appearance, demeanor, and notable features. Use third person perspective.

Return ONLY the long description, no formatting or labels."#
                .to_string()
        }
        DescriptionType::ItemShortDesc => {
            r#"You are a skilled MUD description writer. Write a short description for an item/object.

Short descriptions appear in room listings and inventory. Write ONE brief phrase describing the item (e.g., "a gleaming steel longsword" or "a tattered leather pouch").

Return ONLY the short description, no formatting or labels."#
                .to_string()
        }
        DescriptionType::ItemLongDesc => {
            r#"You are a skilled MUD description writer. Write a long/detailed description for an item/object.

Long descriptions appear when players examine the item. Write 2-3 sentences describing its appearance, condition, and notable features.

Return ONLY the long description, no formatting or labels."#
                .to_string()
        }
    }
}

fn build_help_me_write_prompt(desc_type: &DescriptionType, prompt: &str, context: &DescriptionContext) -> String {
    match desc_type {
        DescriptionType::RoomDescription => {
            let mut p = String::new();
            if let Some(ref title) = context.room_title {
                p.push_str(&format!("Write a room description for \"{}\"", title));
            } else {
                p.push_str("Write a room description");
            }
            if let Some(ref area) = context.area_name {
                p.push_str(&format!(" in the area \"{}\"", area));
            }
            p.push('.');
            if let Some(ref theme) = context.theme {
                p.push_str(&format!("\nTheme: {}", theme));
            }
            p.push_str(&format!("\n\nThe builder wants: {}", prompt));
            p
        }
        DescriptionType::MobileShortDesc | DescriptionType::MobileLongDesc => {
            let mut p = String::new();
            if let Some(ref name) = context.entity_name {
                p.push_str(&format!("Write a description for an NPC named \"{}\"", name));
            } else {
                p.push_str("Write a description for an NPC");
            }
            p.push('.');
            if let Some(ref theme) = context.theme {
                p.push_str(&format!("\nThis NPC is in an area with theme: {}", theme));
            }
            p.push_str(&format!("\n\nThe builder wants: {}", prompt));
            p
        }
        DescriptionType::ItemShortDesc | DescriptionType::ItemLongDesc => {
            let mut p = String::new();
            if let Some(ref name) = context.entity_name {
                p.push_str(&format!("Write a description for an item named \"{}\"", name));
            } else {
                p.push_str("Write a description for an item");
            }
            if let Some(ref item_type) = context.entity_type {
                p.push_str(&format!(" (type: {})", item_type));
            }
            p.push('.');
            if let Some(ref theme) = context.theme {
                p.push_str(&format!("\nThis item is in an area with theme: {}", theme));
            }
            p.push_str(&format!("\n\nThe builder wants: {}", prompt));
            p
        }
    }
}

fn build_rephrase_prompt(desc_type: &DescriptionType, existing_text: &str, context: &DescriptionContext) -> String {
    match desc_type {
        DescriptionType::RoomDescription => {
            let mut p = String::new();
            p.push_str("Rephrase the following room description to be more evocative while keeping the same general meaning and features");
            if let Some(ref title) = context.room_title {
                p.push_str(&format!(" for room \"{}\"", title));
            }
            p.push('.');
            if let Some(ref theme) = context.theme {
                p.push_str(&format!("\nMaintain consistency with the area theme: {}", theme));
            }
            p.push_str(&format!("\n\n{}", existing_text));
            p
        }
        DescriptionType::MobileShortDesc | DescriptionType::MobileLongDesc => {
            let mut p = String::new();
            p.push_str(
                "Rephrase the following NPC description to be more evocative while keeping the same general meaning",
            );
            if let Some(ref name) = context.entity_name {
                p.push_str(&format!(" for \"{}\"", name));
            }
            p.push('.');
            if let Some(ref theme) = context.theme {
                p.push_str(&format!("\nMaintain consistency with the area theme: {}", theme));
            }
            p.push_str(&format!("\n\n{}", existing_text));
            p
        }
        DescriptionType::ItemShortDesc | DescriptionType::ItemLongDesc => {
            let mut p = String::new();
            p.push_str(
                "Rephrase the following item description to be more evocative while keeping the same general meaning",
            );
            if let Some(ref name) = context.entity_name {
                p.push_str(&format!(" for \"{}\"", name));
            }
            p.push('.');
            if let Some(ref theme) = context.theme {
                p.push_str(&format!("\nMaintain consistency with the area theme: {}", theme));
            }
            p.push_str(&format!("\n\n{}", existing_text));
            p
        }
    }
}

fn parse_ai_response(request_id: Uuid, connection_id: Uuid, desc_type: &DescriptionType, response: &str) -> AiResponse {
    match desc_type {
        DescriptionType::RoomDescription => {
            // Parse the structured response for room descriptions
            let mut description = None;
            let mut extra_descs = Vec::new();

            let response = response.trim();

            // Look for DESCRIPTION: section
            if let Some(desc_start) = response.find("DESCRIPTION:") {
                let after_desc = &response[desc_start + 12..];
                // Find where the description ends (at EXTRA: or end of string)
                let desc_end = after_desc.find("EXTRA:").unwrap_or(after_desc.len());
                description = Some(after_desc[..desc_end].trim().to_string());
            }

            // Parse EXTRA: sections
            let mut remaining = response;
            while let Some(extra_start) = remaining.find("EXTRA:") {
                remaining = &remaining[extra_start + 6..];

                // Find KEYWORDS: line
                if let Some(kw_start) = remaining.find("KEYWORDS:") {
                    let after_kw = &remaining[kw_start + 9..];
                    // Keywords end at newline
                    let kw_end = after_kw.find('\n').unwrap_or(after_kw.len());
                    let keywords_str = after_kw[..kw_end].trim();
                    let keywords: Vec<String> = keywords_str
                        .split(',')
                        .map(|s| s.trim().to_lowercase())
                        .filter(|s| !s.is_empty())
                        .collect();

                    // Description is everything after keywords until next EXTRA: or end
                    let desc_start = kw_end;
                    let next_extra = after_kw[desc_start..].find("EXTRA:");
                    let desc_end = next_extra.map(|e| desc_start + e).unwrap_or(after_kw.len());
                    let extra_desc = after_kw[desc_start..desc_end].trim().to_string();

                    if !keywords.is_empty() && !extra_desc.is_empty() {
                        extra_descs.push(SuggestedExtraDesc {
                            keywords,
                            description: extra_desc,
                        });
                    }

                    remaining = &after_kw[desc_end..];
                } else {
                    break;
                }
            }

            // If we couldn't parse structured format, use whole response as description
            if description.is_none() {
                description = Some(response.to_string());
            }

            AiResponse {
                request_id,
                connection_id,
                success: true,
                description,
                extra_descs,
                error: None,
            }
        }
        _ => {
            // For mobiles and items, the response is just the description
            AiResponse {
                request_id,
                connection_id,
                success: true,
                description: Some(response.trim().to_string()),
                extra_descs: vec![],
                error: None,
            }
        }
    }
}

//! Google Gemini AI integration module for IronMUD
//!
//! Provides AI-assisted description writing for OLC editors using Gemini API.
//! Uses message-passing to handle async API calls from sync Rhai scripts.

use serde::{Deserialize, Serialize};
use std::env;
use tracing::debug;

/// Configuration for Gemini API, loaded from environment variables
#[derive(Clone)]
pub struct GeminiConfig {
    pub api_key: String,
    pub model: String,
    pub max_tokens: u32,
}

impl GeminiConfig {
    /// Load configuration from environment variables.
    /// Returns None if GEMINI_API_KEY is missing.
    pub fn from_env() -> Option<Self> {
        let api_key = env::var("GEMINI_API_KEY").ok()?;
        let model = env::var("GEMINI_MODEL").unwrap_or_else(|_| "gemini-2.0-flash".to_string());
        let max_tokens = env::var("GEMINI_MAX_TOKENS")
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

impl crate::claude::AiBackend for GeminiConfig {
    const NAME: &'static str = "Gemini";
    async fn complete(
        &self,
        client: &reqwest::Client,
        system_prompt: &str,
        user_prompt: &str,
    ) -> anyhow::Result<String> {
        call_gemini_api(self, client, system_prompt, user_prompt).await
    }
}

// Gemini API request/response structures
#[derive(Serialize)]
struct GeminiApiRequest {
    contents: Vec<GeminiContent>,
    #[serde(rename = "systemInstruction")]
    system_instruction: GeminiContent,
    #[serde(rename = "generationConfig")]
    generation_config: GeminiGenerationConfig,
}

#[derive(Serialize)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[derive(Serialize)]
struct GeminiPart {
    text: String,
}

#[derive(Serialize)]
struct GeminiGenerationConfig {
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: u32,
}

#[derive(Deserialize)]
struct GeminiApiResponse {
    candidates: Option<Vec<GeminiCandidate>>,
    error: Option<GeminiApiError>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: GeminiContentResponse,
}

#[derive(Deserialize)]
struct GeminiContentResponse {
    parts: Vec<GeminiPartResponse>,
}

#[derive(Deserialize)]
struct GeminiPartResponse {
    text: Option<String>,
}

#[derive(Deserialize)]
struct GeminiApiError {
    message: String,
}

async fn call_gemini_api(
    config: &GeminiConfig,
    client: &reqwest::Client,
    system_prompt: &str,
    user_prompt: &str,
) -> anyhow::Result<String> {
    let request_body = GeminiApiRequest {
        contents: vec![GeminiContent {
            parts: vec![GeminiPart {
                text: user_prompt.to_string(),
            }],
        }],
        system_instruction: GeminiContent {
            parts: vec![GeminiPart {
                text: system_prompt.to_string(),
            }],
        },
        generation_config: GeminiGenerationConfig {
            max_output_tokens: config.max_tokens,
        },
    };

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        config.model
    );

    debug!("Calling Gemini API with model: {}", config.model);

    let response = client
        .post(&url)
        .header("x-goog-api-key", &config.api_key)
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await?;

    if !status.is_success() {
        // Try to parse error message
        if let Ok(error_response) = serde_json::from_str::<GeminiApiResponse>(&body) {
            if let Some(error) = error_response.error {
                return Err(anyhow::anyhow!("API error: {}", error.message));
            }
        }
        return Err(anyhow::anyhow!("API error ({}): {}", status, body));
    }

    let api_response: GeminiApiResponse = serde_json::from_str(&body)?;

    // Check for API-level error
    if let Some(error) = api_response.error {
        return Err(anyhow::anyhow!("API error: {}", error.message));
    }

    // Extract text from first candidate
    let text = api_response
        .candidates
        .and_then(|candidates| candidates.into_iter().next())
        .and_then(|candidate| candidate.content.parts.into_iter().next())
        .and_then(|part| part.text)
        .ok_or_else(|| anyhow::anyhow!("No text content in response"))?;

    Ok(text)
}

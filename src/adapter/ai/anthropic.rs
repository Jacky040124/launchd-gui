use std::env;

use reqwest::blocking::Client;
use serde_json::{json, Value};

use crate::adapter::ai::provider::{
    extract_suggested_actions, summarize_and_extract_notes, AiEditRequest, AiEditResponse,
    AiProvider,
};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct AnthropicProvider {
    endpoint: String,
    model: String,
    api_key: Option<String>,
}

impl Default for AnthropicProvider {
    fn default() -> Self {
        Self {
            endpoint: env::var("LAUNCHPAD_ANTHROPIC_BASE_URL")
                .unwrap_or_else(|_| "https://api.anthropic.com/v1/messages".to_string()),
            model: env::var("LAUNCHPAD_ANTHROPIC_MODEL")
                .unwrap_or_else(|_| "claude-3-5-sonnet-latest".to_string()),
            api_key: env::var("LAUNCHPAD_ANTHROPIC_API_KEY")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
        }
    }
}

impl AiProvider for AnthropicProvider {
    fn provider_name(&self) -> &'static str {
        "anthropic"
    }

    fn suggest_edit(&self, request: &AiEditRequest) -> AppResult<AiEditResponse> {
        let api_key = self.api_key.as_ref().ok_or_else(|| {
            AppError::Validation(
                "Anthropic API key missing. Set LAUNCHPAD_ANTHROPIC_API_KEY.".to_string(),
            )
        })?;

        let prompt = format!(
            "User request:\n{}\n\nCurrent plist XML:\n{}",
            request.user_prompt, request.xml_snapshot
        );
        let payload = json!({
            "model": self.model,
            "max_tokens": 700,
            "system": "You are a launchd plist assistant. Reply with concise bullet suggestions only.",
            "messages": [
                {"role": "user", "content": prompt}
            ]
        });

        let response = Client::new()
            .post(&self.endpoint)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&payload)
            .send()?;
        let status = response.status();
        let body = response.text()?;
        if !status.is_success() {
            return Err(AppError::Validation(format!(
                "Anthropic request failed with status {}: {}",
                status.as_u16(),
                truncate_for_error(&body)
            )));
        }

        let json_body: Value = serde_json::from_str(&body)?;
        let content = extract_anthropic_content(&json_body).ok_or_else(|| {
            AppError::Validation("Anthropic response missing content text.".to_string())
        })?;
        let (summary, suggested_patch_notes) = summarize_and_extract_notes(&content);
        let suggested_actions = extract_suggested_actions(&content);
        Ok(AiEditResponse {
            provider: self.provider_name().to_string(),
            summary,
            suggested_patch_notes,
            suggested_actions,
        })
    }
}

fn extract_anthropic_content(body: &Value) -> Option<String> {
    let texts = body
        .get("content")
        .and_then(|content| content.as_array())?
        .iter()
        .filter_map(|item| item.get("text").and_then(|value| value.as_str()))
        .collect::<Vec<_>>();
    (!texts.is_empty()).then(|| texts.join("\n"))
}

fn truncate_for_error(body: &str) -> String {
    let mut text = body.trim().to_string();
    if text.len() > 240 {
        text.truncate(240);
        text.push_str("...");
    }
    text
}

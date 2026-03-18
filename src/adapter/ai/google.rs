use std::env;

use reqwest::blocking::Client;
use serde_json::{json, Value};

use crate::adapter::ai::provider::{
    summarize_and_extract_notes, AiEditRequest, AiEditResponse, AiProvider,
};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct GoogleProvider {
    endpoint_base: String,
    model: String,
    api_key: Option<String>,
}

impl Default for GoogleProvider {
    fn default() -> Self {
        Self {
            endpoint_base: env::var("LAUNCHPAD_GOOGLE_BASE_URL")
                .unwrap_or_else(|_| "https://generativelanguage.googleapis.com/v1beta".to_string()),
            model: env::var("LAUNCHPAD_GOOGLE_MODEL")
                .unwrap_or_else(|_| "gemini-1.5-flash".to_string()),
            api_key: env::var("LAUNCHPAD_GOOGLE_API_KEY")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
        }
    }
}

impl AiProvider for GoogleProvider {
    fn provider_name(&self) -> &'static str {
        "google"
    }

    fn suggest_edit(&self, request: &AiEditRequest) -> AppResult<AiEditResponse> {
        let api_key = self.api_key.as_ref().ok_or_else(|| {
            AppError::Validation(
                "Google API key missing. Set LAUNCHPAD_GOOGLE_API_KEY.".to_string(),
            )
        })?;

        let endpoint = format!(
            "{}/models/{}:generateContent?key={}",
            self.endpoint_base, self.model, api_key
        );
        let prompt = format!(
            "You are a launchd plist assistant. Return concise bullet suggestions only.\n\
            User request:\n{}\n\nCurrent plist XML:\n{}",
            request.user_prompt, request.xml_snapshot
        );
        let payload = json!({
            "contents": [
                {
                    "parts": [
                        {"text": prompt}
                    ]
                }
            ]
        });

        let response = Client::new().post(endpoint).json(&payload).send()?;
        let status = response.status();
        let body = response.text()?;
        if !status.is_success() {
            return Err(AppError::Validation(format!(
                "Google request failed with status {}: {}",
                status.as_u16(),
                truncate_for_error(&body)
            )));
        }

        let json_body: Value = serde_json::from_str(&body)?;
        let content = extract_google_content(&json_body).ok_or_else(|| {
            AppError::Validation("Google response missing candidate text.".to_string())
        })?;
        let (summary, suggested_patch_notes) = summarize_and_extract_notes(&content);
        Ok(AiEditResponse {
            provider: self.provider_name().to_string(),
            summary,
            suggested_patch_notes,
        })
    }
}

fn extract_google_content(body: &Value) -> Option<String> {
    let parts = body
        .get("candidates")
        .and_then(|candidates| candidates.as_array())
        .and_then(|items| items.first())
        .and_then(|item| item.get("content"))
        .and_then(|content| content.get("parts"))
        .and_then(|parts| parts.as_array())?;
    let texts = parts
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

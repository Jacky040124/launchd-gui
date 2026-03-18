use std::env;

use reqwest::blocking::Client;
use serde_json::{json, Value};

use crate::adapter::ai::provider::{
    extract_suggested_actions, summarize_and_extract_notes, AiEditRequest, AiEditResponse,
    AiProvider, AiStreamResponse,
};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct OpenAiCompatibleProvider {
    pub provider_name: &'static str,
    pub endpoint: String,
    pub model: String,
    pub api_key: Option<String>,
}

impl OpenAiCompatibleProvider {
    pub fn new(provider_name: &'static str) -> Self {
        let prefix = provider_name.replace('-', "_").to_ascii_uppercase();
        let endpoint_key = format!("LAUNCHPAD_{}_BASE_URL", prefix);
        let model_key = format!("LAUNCHPAD_{}_MODEL", prefix);
        let api_key_key = format!("LAUNCHPAD_{}_API_KEY", prefix);

        let endpoint = env::var(endpoint_key).unwrap_or_else(|_| default_endpoint(provider_name));
        let model = env::var(model_key).unwrap_or_else(|_| default_model(provider_name));
        let api_key = env::var(api_key_key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        Self {
            provider_name,
            endpoint,
            model,
            api_key,
        }
    }
}

impl AiProvider for OpenAiCompatibleProvider {
    fn provider_name(&self) -> &'static str {
        self.provider_name
    }

    fn suggest_edit(&self, request: &AiEditRequest) -> AppResult<AiEditResponse> {
        let prompt = format!(
            "You are a launchd plist assistant. Return concise actionable bullets only.\n\
            User request:\n{}\n\nCurrent plist XML:\n{}",
            request.user_prompt, request.xml_snapshot
        );
        let payload = json!({
            "model": self.model,
            "temperature": 0.1,
            "messages": [
                {"role":"system","content":"You help users modify launchd plist safely."},
                {"role":"user","content":prompt}
            ]
        });

        let client = Client::new();
        let mut request_builder = client.post(&self.endpoint).json(&payload);
        if let Some(api_key) = &self.api_key {
            request_builder = request_builder.bearer_auth(api_key);
        }

        let response = request_builder.send()?;
        let status = response.status();
        let body = response.text()?;
        if !status.is_success() {
            return Err(AppError::Validation(format!(
                "{} request failed with status {}: {}",
                self.provider_name,
                status.as_u16(),
                truncate_for_error(&body)
            )));
        }

        let json_body: Value = serde_json::from_str(&body)?;
        let content = extract_openai_content(&json_body).ok_or_else(|| {
            AppError::Validation(format!(
                "{} response did not contain choices[0].message.content",
                self.provider_name
            ))
        })?;
        let (summary, suggested_patch_notes) = summarize_and_extract_notes(&content);
        let suggested_actions = extract_suggested_actions(&content);
        Ok(AiEditResponse {
            provider: self.provider_name.to_string(),
            summary,
            suggested_patch_notes,
            suggested_actions,
        })
    }

    fn suggest_edit_with_stream(&self, request: &AiEditRequest) -> AppResult<AiStreamResponse> {
        let prompt = format!(
            "You are a launchd plist assistant. Return concise actionable bullets only.\n\
            User request:\n{}\n\nCurrent plist XML:\n{}",
            request.user_prompt, request.xml_snapshot
        );
        let payload = json!({
            "model": self.model,
            "temperature": 0.1,
            "stream": true,
            "messages": [
                {"role":"system","content":"You help users modify launchd plist safely."},
                {"role":"user","content":prompt}
            ]
        });

        let client = Client::new();
        let mut request_builder = client.post(&self.endpoint).json(&payload);
        if let Some(api_key) = &self.api_key {
            request_builder = request_builder.bearer_auth(api_key);
        }

        let response = request_builder.send()?;
        let status = response.status();
        let body = response.text()?;
        if !status.is_success() {
            // Some openai-compatible providers don't support SSE stream responses.
            let fallback = self.suggest_edit(request)?;
            return Ok(AiStreamResponse::from_response(fallback));
        }

        let chunks = extract_openai_stream_chunks(&body);
        if chunks.is_empty() {
            let fallback = self.suggest_edit(request)?;
            return Ok(AiStreamResponse::from_response(fallback));
        }

        let content = chunks.join("");
        let (summary, suggested_patch_notes) = summarize_and_extract_notes(&content);
        let suggested_actions = extract_suggested_actions(&content);
        let response = AiEditResponse {
            provider: self.provider_name.to_string(),
            summary,
            suggested_patch_notes,
            suggested_actions,
        };
        Ok(AiStreamResponse { response, chunks })
    }
}

fn extract_openai_content(body: &Value) -> Option<String> {
    body.get("choices")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|item| item.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_str())
        .map(ToOwned::to_owned)
}

fn extract_openai_stream_chunks(raw: &str) -> Vec<String> {
    raw.lines()
        .filter_map(|line| line.trim().strip_prefix("data: "))
        .filter(|payload| *payload != "[DONE]" && !payload.trim().is_empty())
        .filter_map(|payload| serde_json::from_str::<Value>(payload).ok())
        .filter_map(|json| {
            json.get("choices")
                .and_then(|value| value.as_array())
                .and_then(|items| items.first())
                .and_then(|item| item.get("delta"))
                .and_then(|delta| delta.get("content"))
                .and_then(|content| content.as_str())
                .map(ToOwned::to_owned)
        })
        .collect()
}

fn default_endpoint(provider_name: &str) -> String {
    match provider_name {
        "openrouter" => "https://openrouter.ai/api/v1/chat/completions".to_string(),
        "lm-studio" => "http://127.0.0.1:1234/v1/chat/completions".to_string(),
        "ollama" => "http://127.0.0.1:11434/v1/chat/completions".to_string(),
        "xai" => "https://api.x.ai/v1/chat/completions".to_string(),
        _ => "https://api.openai.com/v1/chat/completions".to_string(),
    }
}

fn default_model(provider_name: &str) -> String {
    match provider_name {
        "openrouter" => "openai/gpt-4o-mini".to_string(),
        "lm-studio" => "local-model".to_string(),
        "ollama" => "llama3.1".to_string(),
        "xai" => "grok-2-latest".to_string(),
        _ => "gpt-4o-mini".to_string(),
    }
}

fn truncate_for_error(body: &str) -> String {
    let mut text = body.trim().to_string();
    if text.len() > 240 {
        text.truncate(240);
        text.push_str("...");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::extract_openai_stream_chunks;

    #[test]
    fn extract_openai_stream_chunks_collects_delta_content() {
        let payload = r#"
data: {"choices":[{"delta":{"content":"Hello"}}]}
data: {"choices":[{"delta":{"content":" world"}}]}
data: [DONE]
"#;
        let chunks = extract_openai_stream_chunks(payload);
        assert_eq!(chunks, vec!["Hello".to_string(), " world".to_string()]);
    }

    #[test]
    fn extract_openai_stream_chunks_ignores_invalid_lines() {
        let payload = r#"
event: ping
data: {"choices":[{"delta":{"role":"assistant"}}]}
data: invalid-json
data: [DONE]
"#;
        let chunks = extract_openai_stream_chunks(payload);
        assert!(chunks.is_empty());
    }
}

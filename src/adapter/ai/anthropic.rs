use crate::adapter::ai::provider::{AiEditRequest, AiEditResponse, AiProvider};
use crate::error::{AppError, AppResult};

#[derive(Debug, Default, Clone)]
pub struct AnthropicProvider;

impl AiProvider for AnthropicProvider {
    fn provider_name(&self) -> &'static str {
        "anthropic"
    }

    fn suggest_edit(&self, _request: &AiEditRequest) -> AppResult<AiEditResponse> {
        Err(AppError::Validation(
            "Anthropic provider is not configured in this build yet.".to_string(),
        ))
    }
}

use crate::adapter::ai::provider::{AiEditRequest, AiEditResponse, AiProvider};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct OpenAiCompatibleProvider {
    pub provider_name: &'static str,
}

impl OpenAiCompatibleProvider {
    pub fn new(provider_name: &'static str) -> Self {
        Self { provider_name }
    }
}

impl AiProvider for OpenAiCompatibleProvider {
    fn provider_name(&self) -> &'static str {
        self.provider_name
    }

    fn suggest_edit(&self, _request: &AiEditRequest) -> AppResult<AiEditResponse> {
        Err(AppError::Validation(format!(
            "{} provider is not configured in this build. Configure external AI endpoint in next phase.",
            self.provider_name
        )))
    }
}

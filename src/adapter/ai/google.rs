use crate::adapter::ai::provider::{AiEditRequest, AiEditResponse, AiProvider};
use crate::error::{AppError, AppResult};

#[derive(Debug, Default, Clone)]
pub struct GoogleProvider;

impl AiProvider for GoogleProvider {
    fn provider_name(&self) -> &'static str {
        "google"
    }

    fn suggest_edit(&self, _request: &AiEditRequest) -> AppResult<AiEditResponse> {
        Err(AppError::Validation(
            "Google provider is not configured in this build yet.".to_string(),
        ))
    }
}

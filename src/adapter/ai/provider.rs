use crate::error::AppResult;

#[derive(Debug, Clone)]
pub struct AiEditRequest {
    pub user_prompt: String,
    pub xml_snapshot: String,
}

#[derive(Debug, Clone)]
pub struct AiEditResponse {
    pub provider: String,
    pub summary: String,
    pub suggested_patch_notes: Vec<String>,
}

pub trait AiProvider: Send + Sync {
    fn provider_name(&self) -> &'static str;
    fn suggest_edit(&self, request: &AiEditRequest) -> AppResult<AiEditResponse>;
}

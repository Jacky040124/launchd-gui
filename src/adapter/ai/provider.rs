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

pub fn summarize_and_extract_notes(raw_text: &str) -> (String, Vec<String>) {
    let lines = raw_text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return ("No summary returned.".to_string(), Vec::new());
    }

    let summary = lines[0]
        .trim_start_matches("- ")
        .trim_start_matches("* ")
        .to_string();
    let notes = lines
        .iter()
        .map(|line| line.trim_start_matches("- ").trim_start_matches("* "))
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    (summary, notes)
}

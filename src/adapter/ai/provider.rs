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

impl AiEditResponse {
    pub fn stream_chunks(&self) -> Vec<String> {
        let mut chunks = Vec::new();
        if !self.summary.trim().is_empty() {
            chunks.push(self.summary.clone());
        }
        chunks.extend(self.suggested_patch_notes.clone());
        chunks
    }
}

#[derive(Debug, Clone)]
pub struct AiStreamResponse {
    pub response: AiEditResponse,
    pub chunks: Vec<String>,
}

impl AiStreamResponse {
    pub fn from_response(response: AiEditResponse) -> Self {
        let chunks = response.stream_chunks();
        Self { response, chunks }
    }
}

pub trait AiProvider: Send + Sync {
    fn provider_name(&self) -> &'static str;
    fn suggest_edit(&self, request: &AiEditRequest) -> AppResult<AiEditResponse>;

    fn suggest_edit_with_stream(&self, request: &AiEditRequest) -> AppResult<AiStreamResponse> {
        let response = self.suggest_edit(request)?;
        Ok(AiStreamResponse::from_response(response))
    }
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

#[cfg(test)]
mod tests {
    use super::AiEditResponse;

    #[test]
    fn stream_chunks_contains_summary_and_notes() {
        let response = AiEditResponse {
            provider: "mock".to_string(),
            summary: "summary".to_string(),
            suggested_patch_notes: vec!["note-a".to_string(), "note-b".to_string()],
        };
        let chunks = response.stream_chunks();
        assert_eq!(chunks, vec!["summary", "note-a", "note-b"]);
    }
}

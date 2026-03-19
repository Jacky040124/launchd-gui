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
    pub suggested_actions: Vec<String>,
}

impl AiEditResponse {
    pub fn stream_chunks(&self) -> Vec<String> {
        let mut chunks = Vec::new();
        if !self.summary.trim().is_empty() {
            chunks.push(self.summary.clone());
        }
        chunks.extend(self.suggested_patch_notes.clone());
        chunks.extend(
            self.suggested_actions
                .iter()
                .map(|action| format!("action:{action}")),
        );
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

pub fn extract_suggested_actions(raw_text: &str) -> Vec<String> {
    raw_text
        .lines()
        .map(str::trim)
        .filter_map(|line| {
            let normalized = line
                .trim_start_matches("- ")
                .trim_start_matches("* ")
                .trim();
            let action = normalized
                .strip_prefix("ACTION:")
                .or_else(|| normalized.strip_prefix("Action:"))
                .or_else(|| normalized.strip_prefix("action:"))?
                .trim()
                .to_ascii_lowercase();
            if action.is_empty() {
                None
            } else {
                Some(action)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{extract_suggested_actions, AiEditResponse};

    #[test]
    fn stream_chunks_contains_summary_and_notes() {
        let response = AiEditResponse {
            provider: "mock".to_string(),
            summary: "summary".to_string(),
            suggested_patch_notes: vec!["note-a".to_string(), "note-b".to_string()],
            suggested_actions: vec!["enable".to_string()],
        };
        let chunks = response.stream_chunks();
        assert_eq!(chunks, vec!["summary", "note-a", "note-b", "action:enable"]);
    }

    #[test]
    fn extract_suggested_actions_reads_action_lines() {
        let text = "- ACTION: load\n* Action: enable\n- action: restart";
        let actions = extract_suggested_actions(text);
        assert_eq!(
            actions,
            vec![
                "load".to_string(),
                "enable".to_string(),
                "restart".to_string()
            ]
        );
    }
}

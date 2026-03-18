use std::sync::Arc;

use crate::adapter::ai::claude_agent_sidecar::ClaudeAgentSidecarProvider;
use crate::adapter::ai::provider::{AiEditRequest, AiEditResponse, AiProvider};
use crate::error::AppResult;

pub struct AiService {
    provider: Arc<dyn AiProvider>,
}

impl AiService {
    pub fn new() -> Self {
        let provider: Arc<dyn AiProvider> = ClaudeAgentSidecarProvider::from_env()
            .map(|provider| Arc::new(provider) as Arc<dyn AiProvider>)
            .unwrap_or_else(|| Arc::new(HeuristicAiProvider));
        Self { provider }
    }

    pub fn with_provider(provider: Arc<dyn AiProvider>) -> Self {
        Self { provider }
    }

    pub fn suggest_edit(&self, prompt: &str, xml_snapshot: &str) -> AppResult<AiEditResponse> {
        self.provider.suggest_edit(&AiEditRequest {
            user_prompt: prompt.to_string(),
            xml_snapshot: xml_snapshot.to_string(),
        })
    }
}

impl Default for AiService {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default)]
struct HeuristicAiProvider;

impl AiProvider for HeuristicAiProvider {
    fn provider_name(&self) -> &'static str {
        "heuristic-local"
    }

    fn suggest_edit(&self, request: &AiEditRequest) -> AppResult<AiEditResponse> {
        let normalized = request.user_prompt.to_ascii_lowercase();
        let mut suggestions = Vec::new();

        if normalized.contains("run at load") || normalized.contains("开机") {
            suggestions.push("建议启用 RunAtLoad=true。".to_string());
        }
        if normalized.contains("keep alive") || normalized.contains("常驻") {
            suggestions.push("建议启用 KeepAlive=true 并检查退出循环风险。".to_string());
        }
        if normalized.contains("interval") || normalized.contains("定时") {
            suggestions.push("建议设置 StartInterval（秒）并禁用冲突的 KeepAlive。".to_string());
        }
        if normalized.contains("log") || normalized.contains("日志") {
            suggestions.push(
                "建议配置 StandardOutPath / StandardErrorPath 便于后续日志排查。".to_string(),
            );
        }
        if normalized.contains("env") || normalized.contains("环境变量") {
            suggestions.push("建议在 EnvironmentVariables 中补充 PATH 等关键变量。".to_string());
        }
        if suggestions.is_empty() {
            suggestions.push("可先描述目标：何时运行、是否常驻、依赖哪些环境变量。".to_string());
        }

        Ok(AiEditResponse {
            provider: self.provider_name().to_string(),
            summary: format!("基于当前配置与提示词的建议（{}）", self.provider_name()),
            suggested_patch_notes: suggestions,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::adapter::ai::provider::{AiEditRequest, AiEditResponse, AiProvider};
    use crate::error::AppResult;

    use super::AiService;

    #[derive(Debug)]
    struct MockProvider;

    impl AiProvider for MockProvider {
        fn provider_name(&self) -> &'static str {
            "mock"
        }

        fn suggest_edit(&self, request: &AiEditRequest) -> AppResult<AiEditResponse> {
            Ok(AiEditResponse {
                provider: self.provider_name().to_string(),
                summary: format!("echo:{}", request.user_prompt),
                suggested_patch_notes: vec!["note-a".to_string()],
            })
        }
    }

    #[test]
    fn ai_service_delegates_to_provider() {
        let service = AiService::with_provider(Arc::new(MockProvider));
        let response = service
            .suggest_edit("enable run at load", "<plist/>")
            .expect("ai response");
        assert_eq!(response.provider, "mock");
        assert!(response.summary.contains("enable run at load"));
    }
}

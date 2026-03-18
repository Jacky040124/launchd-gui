use std::collections::BTreeMap;
use std::sync::Arc;

use crate::adapter::ai::anthropic::AnthropicProvider;
use crate::adapter::ai::claude_agent_sidecar::ClaudeAgentSidecarProvider;
use crate::adapter::ai::google::GoogleProvider;
use crate::adapter::ai::openai_compat::OpenAiCompatibleProvider;
use crate::adapter::ai::provider::{AiEditRequest, AiEditResponse, AiProvider};
use crate::error::AppResult;

pub struct AiService {
    providers: BTreeMap<String, Arc<dyn AiProvider>>,
    active_provider: String,
}

impl AiService {
    pub fn new() -> Self {
        Self::new_with_default("heuristic-local")
    }

    pub fn new_with_default(default_provider: &str) -> Self {
        let mut providers: BTreeMap<String, Arc<dyn AiProvider>> = BTreeMap::new();
        providers.insert("heuristic-local".to_string(), Arc::new(HeuristicAiProvider));

        providers.insert(
            "openai".to_string(),
            Arc::new(OpenAiCompatibleProvider::new("openai")),
        );
        providers.insert(
            "openrouter".to_string(),
            Arc::new(OpenAiCompatibleProvider::new("openrouter")),
        );
        providers.insert(
            "lm-studio".to_string(),
            Arc::new(OpenAiCompatibleProvider::new("lm-studio")),
        );
        providers.insert(
            "ollama".to_string(),
            Arc::new(OpenAiCompatibleProvider::new("ollama")),
        );
        providers.insert(
            "xai".to_string(),
            Arc::new(OpenAiCompatibleProvider::new("xai")),
        );
        providers.insert("anthropic".to_string(), Arc::new(AnthropicProvider::default()));
        providers.insert("google".to_string(), Arc::new(GoogleProvider::default()));

        if let Some(sidecar) = ClaudeAgentSidecarProvider::from_env() {
            providers.insert("claude-sidecar".to_string(), Arc::new(sidecar));
        }

        let active_provider = if providers.contains_key(default_provider) {
            default_provider.to_string()
        } else {
            "heuristic-local".to_string()
        };

        Self {
            providers,
            active_provider,
        }
    }

    pub fn with_provider(provider: Arc<dyn AiProvider>) -> Self {
        let name = provider.provider_name().to_string();
        let mut providers: BTreeMap<String, Arc<dyn AiProvider>> = BTreeMap::new();
        providers.insert(name.clone(), provider);
        Self {
            providers,
            active_provider: name,
        }
    }

    pub fn available_providers(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }

    pub fn active_provider_name(&self) -> &str {
        &self.active_provider
    }

    pub fn set_active_provider(&mut self, provider: &str) -> bool {
        if self.providers.contains_key(provider) {
            self.active_provider = provider.to_string();
            return true;
        }
        false
    }

    pub fn cycle_provider(&mut self) -> String {
        let names = self.available_providers();
        let Some(current_index) = names.iter().position(|name| name == &self.active_provider)
        else {
            self.active_provider = "heuristic-local".to_string();
            return self.active_provider.clone();
        };
        let next_index = (current_index + 1) % names.len();
        self.active_provider = names[next_index].clone();
        self.active_provider.clone()
    }

    pub fn suggest_edit(&self, prompt: &str, xml_snapshot: &str) -> AppResult<AiEditResponse> {
        self.providers[&self.active_provider].suggest_edit(&AiEditRequest {
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

    #[test]
    fn ai_service_cycles_provider_names() {
        let mut service = AiService::new();
        let current = service.active_provider_name().to_string();
        let next = service.cycle_provider();
        assert_ne!(current, next);
    }
}

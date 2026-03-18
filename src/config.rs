use std::env;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub ai_default_provider: String,
}

impl AppConfig {
    pub fn from_env() -> Self {
        Self {
            ai_default_provider: env::var("LAUNCHPAD_AI_PROVIDER")
                .unwrap_or_else(|_| "heuristic-local".to_string()),
        }
    }
}

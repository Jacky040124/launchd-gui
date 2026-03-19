use std::env;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub ai_default_provider: String,
    pub quicklaunch_enabled: bool,
    pub quicklaunch_starred_only: bool,
    pub quicklaunch_max_items: usize,
    pub quicklaunch_group_by: String,
    pub quicklaunch_action_poll_ms: u64,
}

impl AppConfig {
    pub fn from_env() -> Self {
        Self {
            ai_default_provider: env::var("LAUNCHPAD_AI_PROVIDER")
                .unwrap_or_else(|_| "heuristic-local".to_string()),
            quicklaunch_enabled: env_flag("LAUNCHPAD_QUICKLAUNCH_ENABLE", false),
            quicklaunch_starred_only: env_flag("LAUNCHPAD_QUICKLAUNCH_STARRED_ONLY", true),
            quicklaunch_max_items: env::var("LAUNCHPAD_QUICKLAUNCH_MAX_ITEMS")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(12),
            quicklaunch_group_by: env::var("LAUNCHPAD_QUICKLAUNCH_GROUP_BY")
                .unwrap_or_else(|_| "scope".to_string()),
            quicklaunch_action_poll_ms: env::var("LAUNCHPAD_QUICKLAUNCH_ACTION_POLL_MS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(2500),
        }
    }
}

fn env_flag(key: &str, default: bool) -> bool {
    env::var(key)
        .ok()
        .map(|value| {
            let normalized = value.to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(default)
}

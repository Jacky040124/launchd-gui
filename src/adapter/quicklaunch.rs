use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::env;
use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickLaunchItem {
    pub id: String,
    pub title: String,
    pub status: String,
}

pub trait QuickLaunchProvider: Send + Sync {
    fn sync_items(&self, items: &[QuickLaunchItem]) -> AppResult<()>;
}

#[derive(Debug, Default)]
pub struct NoopQuickLaunchProvider;

impl QuickLaunchProvider for NoopQuickLaunchProvider {
    fn sync_items(&self, _items: &[QuickLaunchItem]) -> AppResult<()> {
        Err(AppError::Validation(
            "QuickLaunch menubar integration is not available on this build.".to_string(),
        ))
    }
}

#[derive(Debug, Clone)]
pub struct BridgeQuickLaunchProvider {
    executable: String,
}

impl BridgeQuickLaunchProvider {
    pub fn from_env() -> Option<Self> {
        env::var("LAUNCHPAD_QUICKLAUNCH_HELPER")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(|executable| Self { executable })
    }
}

impl QuickLaunchProvider for BridgeQuickLaunchProvider {
    fn sync_items(&self, items: &[QuickLaunchItem]) -> AppResult<()> {
        let payload = serde_json::to_string(items)?;
        let mut child = Command::new(&self.executable)
            .arg("--sync-json")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(payload.as_bytes())?;
        }

        let output = child.wait_with_output()?;
        if output.status.success() {
            return Ok(());
        }

        Err(AppError::CommandFailed {
            command: format!("{} --sync-json", self.executable),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::adapter::quicklaunch::{NoopQuickLaunchProvider, QuickLaunchProvider};

    #[test]
    fn noop_provider_returns_explicit_error() {
        let provider = NoopQuickLaunchProvider;
        let err = provider
            .sync_items(&[])
            .expect_err("should fail on noop provider");
        assert!(err
            .to_string()
            .contains("QuickLaunch menubar integration is not available"));
    }
}

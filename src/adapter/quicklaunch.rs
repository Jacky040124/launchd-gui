use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickLaunchItem {
    pub id: String,
    pub title: String,
    pub status: String,
    pub group: String,
    pub is_starred: bool,
    pub updated_at_unix_secs: u64,
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

#[derive(Debug, Clone)]
pub struct FileQuickLaunchProvider {
    path: PathBuf,
}

impl FileQuickLaunchProvider {
    pub fn new_default() -> Self {
        let path = env::var("HOME")
            .map(|home| PathBuf::from(home).join(".config/launchpad/quicklaunch-items.json"))
            .unwrap_or_else(|_| PathBuf::from(".launchpad-quicklaunch-items.json"));
        Self { path }
    }
}

impl QuickLaunchProvider for FileQuickLaunchProvider {
    fn sync_items(&self, items: &[QuickLaunchItem]) -> AppResult<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let payload = serde_json::to_string_pretty(items)?;
        fs::write(&self.path, payload)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use crate::adapter::quicklaunch::{
        FileQuickLaunchProvider, NoopQuickLaunchProvider, QuickLaunchItem, QuickLaunchProvider,
    };

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

    #[test]
    fn file_provider_writes_json_snapshot() {
        let temp = TempDir::new().expect("temp");
        let path = temp.path().join("quicklaunch.json");
        let provider = FileQuickLaunchProvider { path: path.clone() };
        provider
            .sync_items(&[QuickLaunchItem {
                id: "job-1".to_string(),
                title: "Demo".to_string(),
                status: "loaded".to_string(),
                group: "user-agent".to_string(),
                is_starred: true,
                updated_at_unix_secs: 123,
            }])
            .expect("sync to file");

        let saved = std::fs::read_to_string(path).expect("read");
        assert!(saved.contains("job-1"));
        assert!(saved.contains("loaded"));
        assert!(saved.contains("user-agent"));
    }
}

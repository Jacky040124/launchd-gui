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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickLaunchAction {
    pub action: String,
    pub job_ids: Vec<String>,
}

pub trait QuickLaunchProvider: Send + Sync {
    fn sync_items(&self, items: &[QuickLaunchItem]) -> AppResult<()>;

    fn drain_actions(&self) -> AppResult<Vec<QuickLaunchAction>> {
        Ok(Vec::new())
    }
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

    fn drain_actions(&self) -> AppResult<Vec<QuickLaunchAction>> {
        let output = Command::new(&self.executable)
            .arg("--drain-actions")
            .output()?;
        if !output.status.success() {
            return Err(AppError::CommandFailed {
                command: format!("{} --drain-actions", self.executable),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }

        parse_actions_payload(&String::from_utf8_lossy(&output.stdout))
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

fn parse_actions_payload(payload: &str) -> AppResult<Vec<QuickLaunchAction>> {
    if payload.trim().is_empty() {
        Ok(Vec::new())
    } else {
        Ok(serde_json::from_str(payload)?)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use crate::adapter::quicklaunch::{
        parse_actions_payload, FileQuickLaunchProvider, NoopQuickLaunchProvider, QuickLaunchAction,
        QuickLaunchItem, QuickLaunchProvider,
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

    #[test]
    fn parse_actions_payload_handles_empty_and_valid_json() {
        let empty = parse_actions_payload("").expect("empty");
        assert!(empty.is_empty());

        let parsed = parse_actions_payload(
            r#"[{"action":"start","job_ids":["id-a","id-b"]},{"action":"disable","job_ids":["id-c"]}]"#,
        )
        .expect("valid");
        assert_eq!(parsed.len(), 2);
        assert_eq!(
            parsed[0],
            QuickLaunchAction {
                action: "start".to_string(),
                job_ids: vec!["id-a".to_string(), "id-b".to_string()]
            }
        );
    }
}

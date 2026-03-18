use std::env;
use std::io::Write;
use std::process::{Command, Stdio};

use crate::adapter::ai::provider::{AiEditRequest, AiEditResponse, AiProvider};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct ClaudeAgentSidecarProvider {
    executable: String,
}

impl ClaudeAgentSidecarProvider {
    pub fn from_env() -> Option<Self> {
        env::var("LAUNCHPAD_CLAUDE_SIDECAR")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(|executable| Self { executable })
    }
}

impl AiProvider for ClaudeAgentSidecarProvider {
    fn provider_name(&self) -> &'static str {
        "claude-agent-sidecar"
    }

    fn suggest_edit(&self, request: &AiEditRequest) -> AppResult<AiEditResponse> {
        let mut child = Command::new(&self.executable)
            .arg("--mode")
            .arg("launchd-edit")
            .arg("--prompt")
            .arg(&request.user_prompt)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(request.xml_snapshot.as_bytes())?;
        }

        let output = child.wait_with_output()?;
        if !output.status.success() {
            return Err(AppError::CommandFailed {
                command: format!("{} --mode launchd-edit", self.executable),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }

        let summary = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(AiEditResponse {
            provider: self.provider_name().to_string(),
            summary: if summary.is_empty() {
                "Sidecar returned empty response.".to_string()
            } else {
                summary
            },
            suggested_patch_notes: Vec::new(),
        })
    }
}

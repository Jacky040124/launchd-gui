#[cfg(target_os = "macos")]
use std::process::Command;

use crate::error::{AppError, AppResult};

pub trait LogStreamClient: Send + Sync {
    fn recent_logs_for_label(
        &self,
        label: &str,
        minutes: u32,
        max_lines: usize,
    ) -> AppResult<String>;
}

#[derive(Debug, Default)]
pub struct SystemLogStreamClient;

impl LogStreamClient for SystemLogStreamClient {
    fn recent_logs_for_label(
        &self,
        label: &str,
        minutes: u32,
        max_lines: usize,
    ) -> AppResult<String> {
        fetch_recent_logs(label, minutes, max_lines)
    }
}

#[cfg(target_os = "macos")]
fn fetch_recent_logs(label: &str, minutes: u32, max_lines: usize) -> AppResult<String> {
    let escaped_label = label.replace('\"', "\\\"");
    let predicate = format!(
        "eventMessage CONTAINS[c] \"{escaped_label}\" OR process CONTAINS[c] \"{escaped_label}\""
    );
    let last_arg = format!("{minutes}m");
    let output = Command::new("log")
        .args([
            "show",
            "--style",
            "compact",
            "--last",
            last_arg.as_str(),
            "--predicate",
            predicate.as_str(),
        ])
        .output()?;

    if !output.status.success() {
        return Err(AppError::CommandFailed {
            command: "log show ...".to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    let text = String::from_utf8_lossy(&output.stdout).to_string();
    let mut lines: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    if lines.len() > max_lines {
        let start = lines.len().saturating_sub(max_lines);
        lines = lines[start..].to_vec();
    }

    Ok(lines.join("\n"))
}

#[cfg(not(target_os = "macos"))]
fn fetch_recent_logs(_label: &str, _minutes: u32, _max_lines: usize) -> AppResult<String> {
    Err(AppError::Validation(
        "Log viewer is supported on macOS builds only.".to_string(),
    ))
}

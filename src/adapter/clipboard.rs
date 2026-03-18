#[cfg(target_os = "macos")]
use std::io::Write;
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};

use crate::error::{AppError, AppResult};

pub trait ClipboardClient: Send + Sync {
    fn set_text(&self, text: &str) -> AppResult<()>;
}

#[derive(Debug, Default)]
pub struct SystemClipboardClient;

impl ClipboardClient for SystemClipboardClient {
    fn set_text(&self, text: &str) -> AppResult<()> {
        copy_to_clipboard(text)
    }
}

#[cfg(target_os = "macos")]
fn copy_to_clipboard(text: &str) -> AppResult<()> {
    let mut process = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(stdin) = process.stdin.as_mut() {
        stdin.write_all(text.as_bytes())?;
    }

    let output = process.wait_with_output()?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(AppError::CommandFailed {
        command: "pbcopy".to_string(),
        stderr: if stderr.is_empty() {
            "pbcopy failed".to_string()
        } else {
            stderr
        },
    })
}

#[cfg(not(target_os = "macos"))]
fn copy_to_clipboard(_text: &str) -> AppResult<()> {
    Err(AppError::Validation(
        "Clipboard copy is supported on macOS builds only.".to_string(),
    ))
}

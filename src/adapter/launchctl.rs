use std::process::Command;

use crate::domain::status::JobStatus;
use crate::error::{AppError, AppResult};

pub trait LaunchctlClient: Send + Sync {
    fn print(&self, target: &str) -> AppResult<String>;
    fn start(&self, target: &str) -> AppResult<()>;
    fn stop(&self, target: &str) -> AppResult<()>;
    fn kickstart(&self, target: &str) -> AppResult<()>;
    fn bootout(&self, domain: &str, path: &str) -> AppResult<()>;
}

#[derive(Debug, Default)]
pub struct SystemLaunchctlClient;

impl SystemLaunchctlClient {
    fn run_launchctl(&self, args: &[&str]) -> AppResult<String> {
        let output = Command::new("launchctl").args(args).output()?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }

        let stderr = {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if stderr.is_empty() {
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            } else {
                stderr
            }
        };

        Err(AppError::CommandFailed {
            command: format!("launchctl {}", args.join(" ")),
            stderr,
        })
    }
}

impl LaunchctlClient for SystemLaunchctlClient {
    fn print(&self, target: &str) -> AppResult<String> {
        self.run_launchctl(&["print", target])
    }

    fn start(&self, target: &str) -> AppResult<()> {
        self.run_launchctl(&["start", target]).map(|_| ())
    }

    fn stop(&self, target: &str) -> AppResult<()> {
        self.run_launchctl(&["stop", target]).map(|_| ())
    }

    fn kickstart(&self, target: &str) -> AppResult<()> {
        self.run_launchctl(&["kickstart", "-k", target]).map(|_| ())
    }

    fn bootout(&self, domain: &str, path: &str) -> AppResult<()> {
        self.run_launchctl(&["bootout", domain, path]).map(|_| ())
    }
}

pub fn current_uid() -> u32 {
    // SAFETY: libc::geteuid is thread-safe and has no preconditions.
    unsafe { libc::geteuid() as u32 }
}

pub fn status_from_print_output(output: &str) -> JobStatus {
    JobStatus::from_launchctl_output(output)
}

pub fn is_not_loaded_error(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("could not find service")
        || normalized.contains("service not found")
        || normalized.contains("no such process")
}

pub fn is_bootout_ignorable_error(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("could not find service")
        || normalized.contains("service not found")
        || normalized.contains("no such process")
        || normalized.contains("not loaded")
}

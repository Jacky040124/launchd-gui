use std::collections::HashMap;
use std::process::Command;

use crate::domain::job_detail::JobRuntimeDetails;
use crate::domain::status::JobStatus;
use crate::error::{AppError, AppResult};

pub trait LaunchctlClient: Send + Sync {
    fn list(&self) -> AppResult<String>;
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
    fn list(&self) -> AppResult<String> {
        self.run_launchctl(&["list"])
    }

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchctlListEntry {
    pub pid: Option<i32>,
    pub last_exit_status: Option<i32>,
    pub label: String,
}

impl LaunchctlListEntry {
    pub fn status(&self) -> JobStatus {
        if self.pid.is_some() {
            JobStatus::Running
        } else {
            JobStatus::Loaded
        }
    }
}

pub fn parse_list_output(output: &str) -> HashMap<String, LaunchctlListEntry> {
    let mut parsed = HashMap::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("PID") {
            continue;
        }

        let columns: Vec<&str> = trimmed.split_whitespace().collect();
        if columns.len() < 3 {
            continue;
        }

        let pid = parse_i32_column(columns[0]);
        let last_exit_status = parse_i32_column(columns[1]);
        let label = columns[2..].join(" ");
        if label.is_empty() {
            continue;
        }

        parsed.insert(
            label.clone(),
            LaunchctlListEntry {
                pid,
                last_exit_status,
                label,
            },
        );
    }
    parsed
}

pub fn parse_runtime_details_from_print_output(output: &str) -> JobRuntimeDetails {
    let mut details = JobRuntimeDetails::default();

    for line in output.lines() {
        let trimmed = line.trim();
        let normalized = trimmed.to_ascii_lowercase();

        if details.pid.is_none() && normalized.contains("pid =") {
            details.pid = extract_value(trimmed);
            continue;
        }

        if details.last_exit_status.is_none()
            && (normalized.contains("last exit code")
                || normalized.contains("last exit status")
                || normalized.starts_with("last status ="))
        {
            details.last_exit_status = extract_value(trimmed);
            continue;
        }

        if details.last_run.is_none()
            && (normalized.contains("last run")
                || normalized.contains("last fire time")
                || normalized.contains("last exit time")
                || normalized.contains("last ran"))
        {
            details.last_run = extract_value(trimmed);
        }
    }

    if details.is_empty() {
        details.raw_hint = Some("No runtime detail fields found in launchctl print output".into());
    }

    details
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

fn parse_i32_column(value: &str) -> Option<i32> {
    if value == "-" {
        None
    } else {
        value.parse::<i32>().ok()
    }
}

fn extract_value(line: &str) -> Option<String> {
    let (_, value) = line
        .split_once('=')
        .or_else(|| line.split_once(':'))
        .unwrap_or(("", line));

    let normalized = value.trim();
    (!normalized.is_empty()).then(|| normalized.to_string())
}

#[cfg(test)]
mod tests {
    use super::{parse_list_output, parse_runtime_details_from_print_output};

    #[test]
    fn parse_list_output_extracts_pid_and_status() {
        let output = "PID\tStatus\tLabel\n123\t0\tcom.demo.run\n-\t78\tcom.demo.idle\n";
        let parsed = parse_list_output(output);

        let running = parsed.get("com.demo.run").expect("running entry");
        assert_eq!(running.pid, Some(123));
        assert_eq!(running.last_exit_status, Some(0));

        let idle = parsed.get("com.demo.idle").expect("idle entry");
        assert_eq!(idle.pid, None);
        assert_eq!(idle.last_exit_status, Some(78));
    }

    #[test]
    fn parse_runtime_details_extracts_common_fields() {
        let output = "pid = 42\nlast exit code = 1\nlast fire time = 2026-03-18 09:00:00 +0000\n";
        let details = parse_runtime_details_from_print_output(output);

        assert_eq!(details.pid.as_deref(), Some("42"));
        assert_eq!(details.last_exit_status.as_deref(), Some("1"));
        assert_eq!(
            details.last_run.as_deref(),
            Some("2026-03-18 09:00:00 +0000")
        );
        assert!(details.raw_hint.is_none());
    }
}

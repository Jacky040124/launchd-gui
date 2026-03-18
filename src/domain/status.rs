use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    Running,
    Loaded,
    Disabled,
    Unknown,
}

impl JobStatus {
    pub fn from_launchctl_output(output: &str) -> Self {
        let normalized = output.to_ascii_lowercase();
        if normalized.contains("state = running")
            || normalized.contains("running = 1")
            || normalized.contains("active count = 1")
        {
            return JobStatus::Running;
        }

        if normalized.contains("disabled = true")
            || normalized.contains("is disabled")
            || normalized.contains("disabled services")
        {
            return JobStatus::Disabled;
        }

        if normalized.trim().is_empty() {
            JobStatus::Unknown
        } else {
            JobStatus::Loaded
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            JobStatus::Running => "running",
            JobStatus::Loaded => "loaded",
            JobStatus::Disabled => "disabled",
            JobStatus::Unknown => "unknown",
        }
    }
}

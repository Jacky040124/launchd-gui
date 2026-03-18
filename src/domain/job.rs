use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::domain::status::JobStatus;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobScope {
    UserAgent,
    GlobalAgent,
    SystemDaemon,
}

impl JobScope {
    pub fn target_for_label(&self, uid: u32, label: &str) -> String {
        match self {
            JobScope::UserAgent | JobScope::GlobalAgent => format!("gui/{uid}/{label}"),
            JobScope::SystemDaemon => format!("system/{label}"),
        }
    }

    pub fn bootout_domain(&self, uid: u32) -> String {
        match self {
            JobScope::UserAgent | JobScope::GlobalAgent => format!("gui/{uid}"),
            JobScope::SystemDaemon => "system".to_string(),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            JobScope::UserAgent => "user-agent",
            JobScope::GlobalAgent => "global-agent",
            JobScope::SystemDaemon => "system-daemon",
        }
    }
}

impl fmt::Display for JobScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobCapabilities {
    pub can_trigger: bool,
    pub can_delete: bool,
    pub trigger_reason: Option<String>,
    pub delete_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobSummary {
    pub id: String,
    pub label: String,
    pub path: PathBuf,
    pub scope: JobScope,
    pub status: JobStatus,
    pub is_starred: bool,
    pub capabilities: JobCapabilities,
    pub error: Option<String>,
}

impl JobSummary {
    pub fn status_text(&self) -> &'static str {
        self.status.as_str()
    }

    pub fn list_line(&self) -> String {
        let starred = if self.is_starred { "★ " } else { "" };
        format!(
            "{}{}  |  {}  |  {}",
            starred,
            self.label,
            self.scope.as_str(),
            self.status_text()
        )
    }
}

pub fn compute_capabilities(scope: &JobScope, path: &Path) -> JobCapabilities {
    if matches!(scope, JobScope::SystemDaemon) {
        return JobCapabilities {
            can_trigger: false,
            can_delete: false,
            trigger_reason: Some("System daemons are read-only in this app".to_string()),
            delete_reason: Some("System daemons are read-only in this app".to_string()),
        };
    }

    let can_delete = has_delete_permission(path);
    JobCapabilities {
        can_trigger: true,
        can_delete,
        trigger_reason: None,
        delete_reason: (!can_delete)
            .then_some("Insufficient filesystem permissions for deleting this plist".to_string()),
    }
}

fn has_delete_permission(path: &Path) -> bool {
    let file_writable = path
        .metadata()
        .map(|meta| !meta.permissions().readonly())
        .unwrap_or(false);

    let parent_writable = path
        .parent()
        .and_then(|parent| parent.metadata().ok())
        .map(|meta| !meta.permissions().readonly())
        .unwrap_or(false);

    file_writable && parent_writable
}

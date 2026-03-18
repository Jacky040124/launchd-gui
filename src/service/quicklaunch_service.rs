use std::sync::Arc;

use crate::adapter::quicklaunch::{QuickLaunchItem, QuickLaunchProvider};
use crate::domain::job::JobSummary;
use crate::error::AppResult;

#[derive(Debug, Clone)]
pub struct QuickLaunchConfig {
    pub enabled: bool,
    pub starred_only: bool,
    pub max_items: usize,
    pub group_by: QuickLaunchGroupBy,
}

impl Default for QuickLaunchConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            starred_only: true,
            max_items: 12,
            group_by: QuickLaunchGroupBy::Scope,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickLaunchGroupBy {
    Scope,
    Status,
    StarredScope,
}

impl QuickLaunchGroupBy {
    pub fn from_env_value(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "status" => Self::Status,
            "starred-scope" => Self::StarredScope,
            _ => Self::Scope,
        }
    }

    fn group_for_job(self, job: &JobSummary) -> String {
        match self {
            Self::Scope => format!("{}", job.scope),
            Self::Status => job.status_text().to_string(),
            Self::StarredScope => {
                let prefix = if job.is_starred { "starred" } else { "plain" };
                format!("{prefix}:{}", job.scope)
            }
        }
    }
}

pub struct QuickLaunchService {
    provider: Arc<dyn QuickLaunchProvider>,
    config: QuickLaunchConfig,
}

impl QuickLaunchService {
    pub fn new(provider: Arc<dyn QuickLaunchProvider>, config: QuickLaunchConfig) -> Self {
        Self { provider, config }
    }

    pub fn sync_jobs(&self, jobs: &[JobSummary]) -> AppResult<usize> {
        if !self.config.enabled {
            return Ok(0);
        }

        let items = jobs
            .iter()
            .filter(|job| !self.config.starred_only || job.is_starred)
            .take(self.config.max_items)
            .map(|job| QuickLaunchItem {
                id: job.id.clone(),
                title: format!("[{}] {}", job.scope, job.label),
                status: job.status_text().to_string(),
                group: self.config.group_by.group_for_job(job),
                is_starred: job.is_starred,
            })
            .collect::<Vec<_>>();

        self.provider.sync_items(&items)?;
        Ok(items.len())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use crate::adapter::quicklaunch::{QuickLaunchItem, QuickLaunchProvider};
    use crate::domain::job::{JobCapabilities, JobMetadata, JobScope, JobSummary};
    use crate::domain::status::JobStatus;
    use crate::error::AppResult;

    use super::{QuickLaunchConfig, QuickLaunchGroupBy, QuickLaunchService};

    #[derive(Debug, Default)]
    struct MockQuickLaunchProvider {
        synced: Mutex<Vec<QuickLaunchItem>>,
    }

    impl QuickLaunchProvider for MockQuickLaunchProvider {
        fn sync_items(&self, items: &[QuickLaunchItem]) -> AppResult<()> {
            *self.synced.lock().expect("lock") = items.to_vec();
            Ok(())
        }
    }

    #[test]
    fn sync_jobs_filters_starred_and_limits_count() {
        let provider = Arc::new(MockQuickLaunchProvider::default());
        let service = QuickLaunchService::new(
            provider.clone(),
            QuickLaunchConfig {
                enabled: true,
                starred_only: true,
                max_items: 1,
                group_by: QuickLaunchGroupBy::Scope,
            },
        );

        let jobs = vec![
            job_fixture("a", true),
            job_fixture("b", true),
            job_fixture("c", false),
        ];
        let synced = service.sync_jobs(&jobs).expect("sync");
        assert_eq!(synced, 1);

        let synced_items = provider.synced.lock().expect("lock").clone();
        assert_eq!(synced_items.len(), 1);
        assert!(synced_items[0].title.contains("a"));
        assert_eq!(synced_items[0].group, "user-agent");
        assert!(synced_items[0].is_starred);
    }

    #[test]
    fn group_mode_can_use_status() {
        let provider = Arc::new(MockQuickLaunchProvider::default());
        let service = QuickLaunchService::new(
            provider.clone(),
            QuickLaunchConfig {
                enabled: true,
                starred_only: false,
                max_items: 2,
                group_by: QuickLaunchGroupBy::Status,
            },
        );
        let mut jobs = vec![job_fixture("a", false)];
        jobs[0].status = JobStatus::Running;
        let _ = service.sync_jobs(&jobs).expect("sync");

        let synced_items = provider.synced.lock().expect("lock").clone();
        assert_eq!(synced_items[0].group, "running");
    }

    fn job_fixture(label: &str, is_starred: bool) -> JobSummary {
        JobSummary {
            id: format!("id-{label}"),
            label: label.to_string(),
            path: PathBuf::from(format!("/tmp/{label}.plist")),
            scope: JobScope::UserAgent,
            status: JobStatus::Loaded,
            is_starred,
            metadata: JobMetadata::default(),
            capabilities: JobCapabilities {
                can_trigger: true,
                can_delete: true,
                trigger_reason: None,
                delete_reason: None,
            },
            error: None,
        }
    }
}

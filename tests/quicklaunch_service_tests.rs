use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use launchpad::adapter::quicklaunch::{QuickLaunchAction, QuickLaunchItem, QuickLaunchProvider};
use launchpad::domain::job::{JobCapabilities, JobMetadata, JobScope, JobSummary};
use launchpad::domain::status::JobStatus;
use launchpad::error::AppResult;
use launchpad::service::quicklaunch_service::{
    QuickLaunchConfig, QuickLaunchGroupBy, QuickLaunchService,
};

#[derive(Debug, Default)]
struct MockQuickLaunchProvider {
    synced: Mutex<Vec<QuickLaunchItem>>,
    queued_actions: Mutex<Vec<QuickLaunchAction>>,
}

impl QuickLaunchProvider for MockQuickLaunchProvider {
    fn sync_items(&self, items: &[QuickLaunchItem]) -> AppResult<()> {
        *self.synced.lock().expect("lock") = items.to_vec();
        Ok(())
    }

    fn drain_actions(&self) -> AppResult<Vec<QuickLaunchAction>> {
        let mut queued = self.queued_actions.lock().expect("lock");
        let drained = queued.clone();
        queued.clear();
        Ok(drained)
    }
}

#[test]
fn sync_jobs_respects_starred_only_and_max_items() {
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
        job_fixture("alpha", JobScope::UserAgent, JobStatus::Loaded, true),
        job_fixture("beta", JobScope::GlobalAgent, JobStatus::Running, true),
        job_fixture("gamma", JobScope::UserAgent, JobStatus::Loaded, false),
    ];

    let count = service.sync_jobs(&jobs).expect("sync");
    assert_eq!(count, 1);

    let synced = provider.synced.lock().expect("lock").clone();
    assert_eq!(synced.len(), 1);
    assert!(synced[0].is_starred);
    assert!(synced[0].title.contains("alpha"));
}

#[test]
fn sync_jobs_group_mode_status_uses_status_group_text() {
    let provider = Arc::new(MockQuickLaunchProvider::default());
    let service = QuickLaunchService::new(
        provider.clone(),
        QuickLaunchConfig {
            enabled: true,
            starred_only: false,
            max_items: 5,
            group_by: QuickLaunchGroupBy::Status,
        },
    );
    let jobs = vec![job_fixture(
        "status-job",
        JobScope::GlobalAgent,
        JobStatus::Running,
        false,
    )];

    service.sync_jobs(&jobs).expect("sync");
    let synced = provider.synced.lock().expect("lock").clone();
    assert_eq!(synced[0].group, "running");
}

#[test]
fn drain_actions_returns_queued_actions_and_clears_provider_state() {
    let provider = Arc::new(MockQuickLaunchProvider::default());
    *provider.queued_actions.lock().expect("lock") = vec![QuickLaunchAction {
        action: "start".to_string(),
        job_ids: vec!["id-alpha".to_string()],
    }];
    let service = QuickLaunchService::new(
        provider.clone(),
        QuickLaunchConfig {
            enabled: true,
            starred_only: false,
            max_items: 5,
            group_by: QuickLaunchGroupBy::Scope,
        },
    );

    let first = service.drain_actions().expect("drain");
    assert_eq!(first.len(), 1);
    let second = service.drain_actions().expect("drain");
    assert!(second.is_empty());
}

fn job_fixture(label: &str, scope: JobScope, status: JobStatus, is_starred: bool) -> JobSummary {
    JobSummary {
        id: format!("id-{label}"),
        label: label.to_string(),
        path: PathBuf::from(format!("/tmp/{label}.plist")),
        scope,
        status,
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

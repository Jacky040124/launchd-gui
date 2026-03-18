use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use launchpad::adapter::fs_ops::FsOps;
use launchpad::adapter::launchctl::LaunchctlClient;
use launchpad::domain::job::{JobCapabilities, JobScope, JobSummary};
use launchpad::domain::status::JobStatus;
use launchpad::error::{AppError, AppResult};
use launchpad::service::delete_service::DeleteService;

#[derive(Debug, Default)]
struct MockLaunchctl {
    calls: Mutex<Vec<String>>,
    bootout_error: Mutex<Option<String>>,
}

impl MockLaunchctl {
    fn with_bootout_error(message: &str) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            bootout_error: Mutex::new(Some(message.to_string())),
        }
    }

    fn take_calls(&self) -> Vec<String> {
        self.calls.lock().expect("lock calls").clone()
    }
}

impl LaunchctlClient for MockLaunchctl {
    fn print(&self, _target: &str) -> AppResult<String> {
        Ok(String::new())
    }

    fn start(&self, _target: &str) -> AppResult<()> {
        Ok(())
    }

    fn stop(&self, _target: &str) -> AppResult<()> {
        Ok(())
    }

    fn kickstart(&self, _target: &str) -> AppResult<()> {
        Ok(())
    }

    fn bootout(&self, domain: &str, path: &str) -> AppResult<()> {
        self.calls
            .lock()
            .expect("lock calls")
            .push(format!("bootout:{domain}:{path}"));

        if let Some(stderr) = self.bootout_error.lock().expect("lock error").clone() {
            return Err(AppError::CommandFailed {
                command: "launchctl bootout ...".to_string(),
                stderr,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
struct MockFsOps {
    removed: Mutex<Vec<PathBuf>>,
}

impl MockFsOps {
    fn removed_paths(&self) -> Vec<PathBuf> {
        self.removed.lock().expect("lock removed").clone()
    }
}

impl FsOps for MockFsOps {
    fn remove_file(&self, path: &std::path::Path) -> AppResult<()> {
        self.removed
            .lock()
            .expect("lock removed")
            .push(path.to_path_buf());
        Ok(())
    }
}

#[test]
fn delete_runs_bootout_then_removes_file() {
    let launchctl = Arc::new(MockLaunchctl::default());
    let fs_ops = Arc::new(MockFsOps::default());
    let service = DeleteService::new(launchctl.clone(), fs_ops.clone(), 501);
    let job = delete_enabled_job();

    service.delete(&job).expect("delete should succeed");

    assert_eq!(
        launchctl.take_calls(),
        vec!["bootout:gui/501:/tmp/com.demo.agent.plist".to_string()]
    );
    assert_eq!(fs_ops.removed_paths(), vec![job.path.clone()]);
}

#[test]
fn delete_ignores_not_loaded_bootout_error() {
    let launchctl = Arc::new(MockLaunchctl::with_bootout_error("service not found"));
    let fs_ops = Arc::new(MockFsOps::default());
    let service = DeleteService::new(launchctl, fs_ops.clone(), 501);
    let job = delete_enabled_job();

    service
        .delete(&job)
        .expect("ignorable bootout error should still delete file");
    assert_eq!(fs_ops.removed_paths(), vec![job.path.clone()]);
}

#[test]
fn delete_fails_when_capability_disabled() {
    let launchctl = Arc::new(MockLaunchctl::default());
    let fs_ops = Arc::new(MockFsOps::default());
    let service = DeleteService::new(launchctl, fs_ops.clone(), 501);
    let mut job = delete_enabled_job();
    job.capabilities.can_delete = false;
    job.capabilities.delete_reason = Some("readonly".to_string());

    let err = service.delete(&job).expect_err("delete should fail");
    match err {
        AppError::Validation(reason) => assert_eq!(reason, "readonly"),
        other => panic!("unexpected error: {other}"),
    }
    assert!(fs_ops.removed_paths().is_empty());
}

fn delete_enabled_job() -> JobSummary {
    JobSummary {
        id: "job-id".to_string(),
        label: "com.demo.agent".to_string(),
        path: PathBuf::from("/tmp/com.demo.agent.plist"),
        scope: JobScope::UserAgent,
        status: JobStatus::Loaded,
        capabilities: JobCapabilities {
            can_trigger: true,
            can_delete: true,
            trigger_reason: None,
            delete_reason: None,
        },
        error: None,
    }
}

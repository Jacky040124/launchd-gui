use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use launchpad::adapter::launchctl::LaunchctlClient;
use launchpad::domain::action::TriggerAction;
use launchpad::domain::job::{JobCapabilities, JobScope, JobSummary};
use launchpad::domain::status::JobStatus;
use launchpad::error::{AppError, AppResult};
use launchpad::service::action_service::ActionService;

#[derive(Debug, Default)]
struct MockLaunchctl {
    calls: Mutex<Vec<String>>,
}

impl MockLaunchctl {
    fn take_calls(&self) -> Vec<String> {
        self.calls.lock().expect("lock calls").clone()
    }
}

impl LaunchctlClient for MockLaunchctl {
    fn print(&self, _target: &str) -> AppResult<String> {
        Ok(String::new())
    }

    fn start(&self, target: &str) -> AppResult<()> {
        self.calls
            .lock()
            .expect("lock calls")
            .push(format!("start:{target}"));
        Ok(())
    }

    fn stop(&self, target: &str) -> AppResult<()> {
        self.calls
            .lock()
            .expect("lock calls")
            .push(format!("stop:{target}"));
        Ok(())
    }

    fn kickstart(&self, target: &str) -> AppResult<()> {
        self.calls
            .lock()
            .expect("lock calls")
            .push(format!("kickstart:{target}"));
        Ok(())
    }

    fn bootout(&self, _domain: &str, _path: &str) -> AppResult<()> {
        Ok(())
    }
}

#[test]
fn execute_start_uses_scoped_target() {
    let launchctl = Arc::new(MockLaunchctl::default());
    let service = ActionService::new(launchctl.clone(), 501);
    let job = job_fixture(true);

    service
        .execute(&job, TriggerAction::Start)
        .expect("start should succeed");

    let calls = launchctl.take_calls();
    assert_eq!(calls, vec!["start:gui/501/com.demo.agent".to_string()]);
}

#[test]
fn execute_stop_and_kickstart_are_forwarded() {
    let launchctl = Arc::new(MockLaunchctl::default());
    let service = ActionService::new(launchctl.clone(), 501);
    let job = job_fixture(true);

    service
        .execute(&job, TriggerAction::Stop)
        .expect("stop should succeed");
    service
        .execute(&job, TriggerAction::Kickstart)
        .expect("kickstart should succeed");

    let calls = launchctl.take_calls();
    assert_eq!(
        calls,
        vec![
            "stop:gui/501/com.demo.agent".to_string(),
            "kickstart:gui/501/com.demo.agent".to_string()
        ]
    );
}

#[test]
fn execute_rejects_when_trigger_disabled() {
    let launchctl = Arc::new(MockLaunchctl::default());
    let service = ActionService::new(launchctl.clone(), 501);
    let mut job = job_fixture(false);
    job.capabilities.trigger_reason = Some("no permission".to_string());

    let error = service
        .execute(&job, TriggerAction::Start)
        .expect_err("disabled trigger should fail");

    match error {
        AppError::Validation(message) => assert_eq!(message, "no permission"),
        other => panic!("unexpected error: {other}"),
    }

    assert!(launchctl.take_calls().is_empty());
}

fn job_fixture(can_trigger: bool) -> JobSummary {
    JobSummary {
        id: "job-id".to_string(),
        label: "com.demo.agent".to_string(),
        path: PathBuf::from("/tmp/com.demo.agent.plist"),
        scope: JobScope::UserAgent,
        status: JobStatus::Loaded,
        capabilities: JobCapabilities {
            can_trigger,
            can_delete: true,
            trigger_reason: None,
            delete_reason: None,
        },
        error: None,
    }
}

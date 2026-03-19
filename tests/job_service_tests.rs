use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use launchpad::adapter::fs_scan::FileScanner;
use launchpad::adapter::launchctl::LaunchctlClient;
use launchpad::adapter::plist_reader::{PlistReader, PlistSummary};
use launchpad::domain::job::JobSummary;
use launchpad::domain::job::{JobMetadata, JobScope};
use launchpad::domain::job_detail::JobRuntimeDetails;
use launchpad::domain::status::JobStatus;
use launchpad::error::{AppError, AppResult};
use launchpad::service::job_service::JobService;
use tempfile::TempDir;

#[derive(Debug)]
struct MockPlistReader {
    label: Option<String>,
    run_at_load: Option<bool>,
    keep_alive: Option<bool>,
    disabled: Option<bool>,
    fail: bool,
}

impl PlistReader for MockPlistReader {
    fn read_summary(&self, _path: &Path) -> AppResult<PlistSummary> {
        if self.fail {
            return Err(AppError::Validation("plist parse failure".to_string()));
        }
        Ok(PlistSummary {
            label: self.label.clone(),
            run_at_load: self.run_at_load,
            keep_alive: self.keep_alive,
            disabled: self.disabled,
        })
    }
}

#[derive(Debug)]
struct MockLaunchctl {
    list_output: String,
    list_error: Option<String>,
    print_output: String,
    print_error: Option<String>,
    calls: Mutex<Vec<String>>,
}

impl LaunchctlClient for MockLaunchctl {
    fn list(&self) -> AppResult<String> {
        self.calls
            .lock()
            .expect("lock calls")
            .push("list".to_string());
        if let Some(stderr) = &self.list_error {
            return Err(AppError::CommandFailed {
                command: "launchctl list".to_string(),
                stderr: stderr.clone(),
            });
        }
        Ok(self.list_output.clone())
    }

    fn print(&self, _target: &str) -> AppResult<String> {
        self.calls
            .lock()
            .expect("lock calls")
            .push("print".to_string());
        if let Some(stderr) = &self.print_error {
            return Err(AppError::CommandFailed {
                command: "launchctl print ...".to_string(),
                stderr: stderr.clone(),
            });
        }
        Ok(self.print_output.clone())
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

    fn enable(&self, _target: &str) -> AppResult<()> {
        Ok(())
    }

    fn disable(&self, _target: &str) -> AppResult<()> {
        Ok(())
    }

    fn bootstrap(&self, _domain: &str, _path: &str) -> AppResult<()> {
        Ok(())
    }

    fn bootout(&self, _domain: &str, _path: &str) -> AppResult<()> {
        Ok(())
    }
}

#[test]
fn list_jobs_maps_running_and_loaded_status_from_launchctl_list() {
    let temp = TempDir::new().expect("temp dir");
    let plist_path = write_dummy_plist(temp.path(), "com.demo.running.plist");
    write_dummy_plist(temp.path(), "com.demo.idle.plist");

    let scanner = FileScanner::with_targets(vec![(temp.path().to_path_buf(), JobScope::UserAgent)]);
    let plist_reader = Arc::new(MockPlistReader {
        label: None,
        run_at_load: None,
        keep_alive: None,
        disabled: None,
        fail: false,
    });
    let launchctl = Arc::new(MockLaunchctl {
        list_output: "PID\tStatus\tLabel\n321\t0\tcom.demo.running\n-\t0\tcom.demo.idle\n".into(),
        list_error: None,
        print_output: String::new(),
        print_error: None,
        calls: Mutex::new(Vec::new()),
    });

    let service = JobService::new(scanner, plist_reader, launchctl, 501);
    let jobs = service.list_jobs().expect("list jobs");

    let running = jobs
        .iter()
        .find(|job| job.label == "com.demo.running")
        .expect("running job");
    assert_eq!(running.path, plist_path);
    assert_eq!(running.status, JobStatus::Running);

    let idle = jobs
        .iter()
        .find(|job| job.label == "com.demo.idle")
        .expect("idle job");
    assert_eq!(idle.status, JobStatus::Loaded);
}

#[test]
fn list_jobs_uses_filename_when_label_missing() {
    let temp = TempDir::new().expect("temp dir");
    write_dummy_plist(temp.path(), "com.demo.fallback.plist");

    let scanner = FileScanner::with_targets(vec![(temp.path().to_path_buf(), JobScope::UserAgent)]);
    let plist_reader = Arc::new(MockPlistReader {
        label: None,
        run_at_load: None,
        keep_alive: None,
        disabled: None,
        fail: false,
    });
    let launchctl = Arc::new(MockLaunchctl {
        list_output: "PID\tStatus\tLabel\n-\t0\tcom.demo.fallback\n".to_string(),
        list_error: None,
        print_output: String::new(),
        print_error: None,
        calls: Mutex::new(Vec::new()),
    });

    let service = JobService::new(scanner, plist_reader, launchctl, 501);
    let jobs = service.list_jobs().expect("list jobs");

    assert_eq!(jobs[0].label, "com.demo.fallback");
    assert_eq!(jobs[0].status, JobStatus::Loaded);
}

#[test]
fn list_jobs_gracefully_handles_plist_parse_failure() {
    let temp = TempDir::new().expect("temp dir");
    write_dummy_plist(temp.path(), "com.demo.broken.plist");

    let scanner = FileScanner::with_targets(vec![(temp.path().to_path_buf(), JobScope::UserAgent)]);
    let plist_reader = Arc::new(MockPlistReader {
        label: None,
        run_at_load: None,
        keep_alive: None,
        disabled: None,
        fail: true,
    });
    let launchctl = Arc::new(MockLaunchctl {
        list_output: String::new(),
        list_error: None,
        print_output: String::new(),
        print_error: None,
        calls: Mutex::new(Vec::new()),
    });

    let service = JobService::new(scanner, plist_reader, launchctl, 501);
    let jobs = service.list_jobs().expect("list jobs");

    assert_eq!(jobs[0].status, JobStatus::Unknown);
    assert!(jobs[0].error.is_some());
}

#[test]
fn list_jobs_maps_not_loaded_to_unknown_without_failing() {
    let temp = TempDir::new().expect("temp dir");
    write_dummy_plist(temp.path(), "com.demo.offline.plist");

    let scanner = FileScanner::with_targets(vec![(temp.path().to_path_buf(), JobScope::UserAgent)]);
    let plist_reader = Arc::new(MockPlistReader {
        label: Some("com.demo.offline".to_string()),
        run_at_load: None,
        keep_alive: None,
        disabled: None,
        fail: false,
    });
    let launchctl = Arc::new(MockLaunchctl {
        list_output: String::new(),
        list_error: Some("could not query list".to_string()),
        print_output: String::new(),
        print_error: None,
        calls: Mutex::new(Vec::new()),
    });

    let service = JobService::new(scanner, plist_reader, launchctl, 501);
    let jobs = service.list_jobs().expect("list jobs");

    assert_eq!(jobs[0].status, JobStatus::Unknown);
    assert!(jobs[0].error.is_none());
}

#[test]
fn list_jobs_does_not_call_print_per_job() {
    let temp = TempDir::new().expect("temp dir");
    write_dummy_plist(temp.path(), "com.demo.a.plist");
    write_dummy_plist(temp.path(), "com.demo.b.plist");

    let scanner = FileScanner::with_targets(vec![(temp.path().to_path_buf(), JobScope::UserAgent)]);
    let plist_reader = Arc::new(MockPlistReader {
        label: None,
        run_at_load: None,
        keep_alive: None,
        disabled: None,
        fail: false,
    });
    let launchctl = Arc::new(MockLaunchctl {
        list_output: "PID\tStatus\tLabel\n-\t0\tcom.demo.a\n-\t0\tcom.demo.b\n".to_string(),
        list_error: None,
        print_output: String::new(),
        print_error: None,
        calls: Mutex::new(Vec::new()),
    });

    let service = JobService::new(scanner, plist_reader, launchctl.clone(), 501);
    let _ = service.list_jobs().expect("list jobs");

    let calls = launchctl.calls.lock().expect("lock calls").clone();
    assert_eq!(calls, vec!["list".to_string()]);
}

#[test]
fn fetch_job_details_uses_launchctl_print() {
    let temp = TempDir::new().expect("temp dir");
    let path = write_dummy_plist(temp.path(), "com.demo.detail.plist");
    let scanner = FileScanner::with_targets(vec![(temp.path().to_path_buf(), JobScope::UserAgent)]);
    let plist_reader = Arc::new(MockPlistReader {
        label: Some("com.demo.detail".to_string()),
        run_at_load: None,
        keep_alive: None,
        disabled: None,
        fail: false,
    });
    let launchctl = Arc::new(MockLaunchctl {
        list_output: String::new(),
        list_error: None,
        print_output: "pid = 902\nlast exit code = 0\nlast run = 2026-03-18 18:33:20 +0000\n"
            .to_string(),
        print_error: None,
        calls: Mutex::new(Vec::new()),
    });
    let service = JobService::new(scanner, plist_reader, launchctl.clone(), 501);

    let details = service
        .fetch_job_details(&job_fixture("com.demo.detail", path))
        .expect("fetch details");
    assert_eq!(
        details,
        JobRuntimeDetails {
            pid: Some("902".to_string()),
            last_exit_status: Some("0".to_string()),
            last_run: Some("2026-03-18 18:33:20 +0000".to_string()),
            raw_hint: None,
        }
    );

    let calls = launchctl.calls.lock().expect("lock calls").clone();
    assert_eq!(calls, vec!["print".to_string()]);
}

fn write_dummy_plist(dir: &Path, filename: &str) -> PathBuf {
    let path = dir.join(filename);
    std::fs::write(&path, "dummy").expect("write plist fixture");
    path
}

fn job_fixture(label: &str, path: PathBuf) -> JobSummary {
    JobSummary {
        id: label.to_string(),
        label: label.to_string(),
        path,
        scope: JobScope::UserAgent,
        status: JobStatus::Unknown,
        is_starred: false,
        metadata: JobMetadata::default(),
        capabilities: launchpad::domain::job::JobCapabilities {
            can_trigger: true,
            can_delete: true,
            trigger_reason: None,
            delete_reason: None,
        },
        error: None,
    }
}

#[test]
fn list_jobs_sets_disabled_status_from_plist_metadata() {
    let temp = TempDir::new().expect("temp dir");
    write_dummy_plist(temp.path(), "com.demo.disabled.plist");

    let scanner = FileScanner::with_targets(vec![(temp.path().to_path_buf(), JobScope::UserAgent)]);
    let plist_reader = Arc::new(MockPlistReader {
        label: Some("com.demo.disabled".to_string()),
        run_at_load: Some(true),
        keep_alive: Some(false),
        disabled: Some(true),
        fail: false,
    });
    let launchctl = Arc::new(MockLaunchctl {
        list_output: "PID\tStatus\tLabel\n-\t0\tcom.demo.disabled\n".to_string(),
        list_error: None,
        print_output: String::new(),
        print_error: None,
        calls: Mutex::new(Vec::new()),
    });

    let service = JobService::new(scanner, plist_reader, launchctl, 501);
    let jobs = service.list_jobs().expect("list jobs");
    let job = jobs.first().expect("job");

    assert_eq!(job.status, JobStatus::Disabled);
    assert_eq!(job.metadata.run_at_load, Some(true));
    assert_eq!(job.metadata.keep_alive, Some(false));
    assert_eq!(job.metadata.disabled, Some(true));
}

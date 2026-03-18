use std::path::{Path, PathBuf};
use std::sync::Arc;

use launchpad::adapter::fs_scan::FileScanner;
use launchpad::adapter::launchctl::LaunchctlClient;
use launchpad::adapter::plist_reader::PlistReader;
use launchpad::domain::job::JobScope;
use launchpad::domain::status::JobStatus;
use launchpad::error::{AppError, AppResult};
use launchpad::service::job_service::JobService;
use tempfile::TempDir;

#[derive(Debug)]
struct MockPlistReader {
    label: Option<String>,
    fail: bool,
}

impl PlistReader for MockPlistReader {
    fn read_label(&self, _path: &Path) -> AppResult<Option<String>> {
        if self.fail {
            return Err(AppError::Validation("plist parse failure".to_string()));
        }
        Ok(self.label.clone())
    }
}

#[derive(Debug)]
struct MockLaunchctl {
    print_output: String,
    print_error: Option<String>,
}

impl LaunchctlClient for MockLaunchctl {
    fn print(&self, _target: &str) -> AppResult<String> {
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

    fn bootout(&self, _domain: &str, _path: &str) -> AppResult<()> {
        Ok(())
    }
}

#[test]
fn list_jobs_maps_running_status() {
    let temp = TempDir::new().expect("temp dir");
    let plist_path = write_dummy_plist(temp.path(), "com.demo.running.plist");

    let scanner = FileScanner::with_targets(vec![(temp.path().to_path_buf(), JobScope::UserAgent)]);
    let plist_reader = Arc::new(MockPlistReader {
        label: Some("com.demo.running".to_string()),
        fail: false,
    });
    let launchctl = Arc::new(MockLaunchctl {
        print_output: "state = running".to_string(),
        print_error: None,
    });

    let service = JobService::new(scanner, plist_reader, launchctl, 501);
    let jobs = service.list_jobs().expect("list jobs");

    assert_eq!(jobs.len(), 1);
    let job = &jobs[0];
    assert_eq!(job.label, "com.demo.running");
    assert_eq!(job.path, plist_path);
    assert_eq!(job.status, JobStatus::Running);
    assert!(job.capabilities.can_trigger);
}

#[test]
fn list_jobs_uses_filename_when_label_missing() {
    let temp = TempDir::new().expect("temp dir");
    write_dummy_plist(temp.path(), "com.demo.fallback.plist");

    let scanner = FileScanner::with_targets(vec![(temp.path().to_path_buf(), JobScope::UserAgent)]);
    let plist_reader = Arc::new(MockPlistReader {
        label: None,
        fail: false,
    });
    let launchctl = Arc::new(MockLaunchctl {
        print_output: "state = waiting".to_string(),
        print_error: None,
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
        fail: true,
    });
    let launchctl = Arc::new(MockLaunchctl {
        print_output: String::new(),
        print_error: None,
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
        fail: false,
    });
    let launchctl = Arc::new(MockLaunchctl {
        print_output: String::new(),
        print_error: Some("Could not find service".to_string()),
    });

    let service = JobService::new(scanner, plist_reader, launchctl, 501);
    let jobs = service.list_jobs().expect("list jobs");

    assert_eq!(jobs[0].status, JobStatus::Unknown);
    assert!(jobs[0].error.is_none());
}

fn write_dummy_plist(dir: &Path, filename: &str) -> PathBuf {
    let path = dir.join(filename);
    std::fs::write(&path, "dummy").expect("write plist fixture");
    path
}

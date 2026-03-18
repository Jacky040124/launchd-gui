use std::path::Path;
use std::sync::Arc;

use crate::adapter::fs_scan::FileScanner;
use crate::adapter::launchctl::{
    is_not_loaded_error, parse_list_output, parse_runtime_details_from_print_output,
    LaunchctlClient,
};
use crate::adapter::plist_reader::PlistReader;
use crate::domain::job::{compute_capabilities, JobSummary};
use crate::domain::job_detail::JobRuntimeDetails;
use crate::domain::status::JobStatus;
use crate::error::{AppError, AppResult};

pub struct JobService {
    scanner: FileScanner,
    plist_reader: Arc<dyn PlistReader>,
    launchctl: Arc<dyn LaunchctlClient>,
    uid: u32,
}

impl JobService {
    pub fn new(
        scanner: FileScanner,
        plist_reader: Arc<dyn PlistReader>,
        launchctl: Arc<dyn LaunchctlClient>,
        uid: u32,
    ) -> Self {
        Self {
            scanner,
            plist_reader,
            launchctl,
            uid,
        }
    }

    pub fn list_jobs(&self) -> AppResult<Vec<JobSummary>> {
        let scanned_files = self.scanner.scan()?;
        let mut jobs = Vec::with_capacity(scanned_files.len());
        let list_status_by_label = self
            .launchctl
            .list()
            .map(|output| parse_list_output(&output))
            .unwrap_or_default();

        for (path, scope) in scanned_files {
            let mut error = None;
            let label = match self.plist_reader.read_label(&path) {
                Ok(Some(label)) => label,
                Ok(None) => fallback_label(&path),
                Err(err) => {
                    error = Some(err.to_string());
                    fallback_label(&path)
                }
            };

            let status = if error.is_some() {
                JobStatus::Unknown
            } else if let Some(entry) = list_status_by_label.get(&label) {
                entry.status()
            } else {
                JobStatus::Unknown
            };

            jobs.push(JobSummary {
                id: path.to_string_lossy().to_string(),
                label,
                path: path.clone(),
                scope: scope.clone(),
                status,
                is_starred: false,
                capabilities: compute_capabilities(&scope, &path),
                error,
            });
        }

        jobs.sort_by(|a, b| a.label.cmp(&b.label));
        Ok(jobs)
    }

    pub fn fetch_job_details(&self, job: &JobSummary) -> AppResult<JobRuntimeDetails> {
        let target = job.scope.target_for_label(self.uid, &job.label);
        match self.launchctl.print(&target) {
            Ok(output) => Ok(parse_runtime_details_from_print_output(&output)),
            Err(AppError::CommandFailed { stderr, .. }) if is_not_loaded_error(&stderr) => {
                Ok(JobRuntimeDetails {
                    raw_hint: Some("Service is not currently loaded".to_string()),
                    ..JobRuntimeDetails::default()
                })
            }
            Err(err) => Err(err),
        }
    }
}

fn fallback_label(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "unknown-label".to_string())
}

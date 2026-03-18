use std::sync::Arc;

use crate::adapter::fs_ops::FsOps;
use crate::adapter::launchctl::{is_bootout_ignorable_error, LaunchctlClient};
use crate::domain::job::JobSummary;
use crate::error::{AppError, AppResult};

pub struct DeleteService {
    launchctl: Arc<dyn LaunchctlClient>,
    fs_ops: Arc<dyn FsOps>,
    uid: u32,
}

impl DeleteService {
    pub fn new(launchctl: Arc<dyn LaunchctlClient>, fs_ops: Arc<dyn FsOps>, uid: u32) -> Self {
        Self {
            launchctl,
            fs_ops,
            uid,
        }
    }

    pub fn delete(&self, job: &JobSummary) -> AppResult<()> {
        if !job.capabilities.can_delete {
            let reason = job
                .capabilities
                .delete_reason
                .clone()
                .unwrap_or_else(|| "Delete is disabled for this job".to_string());
            return Err(AppError::Validation(reason));
        }

        let domain = job.scope.bootout_domain(self.uid);
        let plist_path = job.path.to_string_lossy().to_string();

        match self.launchctl.bootout(&domain, &plist_path) {
            Ok(_) => {}
            Err(AppError::CommandFailed { stderr, .. }) if is_bootout_ignorable_error(&stderr) => {}
            Err(err) => return Err(err),
        }

        self.fs_ops.remove_file(&job.path)?;
        Ok(())
    }
}

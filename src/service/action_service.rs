use std::sync::Arc;

use crate::adapter::launchctl::LaunchctlClient;
use crate::domain::action::TriggerAction;
use crate::domain::job::JobSummary;
use crate::error::{AppError, AppResult};

pub struct ActionService {
    launchctl: Arc<dyn LaunchctlClient>,
    uid: u32,
}

impl ActionService {
    pub fn new(launchctl: Arc<dyn LaunchctlClient>, uid: u32) -> Self {
        Self { launchctl, uid }
    }

    pub fn execute(&self, job: &JobSummary, action: TriggerAction) -> AppResult<()> {
        if !job.capabilities.can_trigger {
            let reason = job
                .capabilities
                .trigger_reason
                .clone()
                .unwrap_or_else(|| "Trigger is disabled for this job".to_string());
            return Err(AppError::Validation(reason));
        }

        let target = job.scope.target_for_label(self.uid, &job.label);
        match action {
            TriggerAction::Start => self.launchctl.start(&target),
            TriggerAction::Stop => self.launchctl.stop(&target),
            TriggerAction::Kickstart => self.launchctl.kickstart(&target),
        }
    }
}

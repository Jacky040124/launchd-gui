use std::sync::Arc;

use crate::adapter::launchctl::{is_bootout_ignorable_error, LaunchctlClient};
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

        let service_target = job.scope.target_for_label(self.uid, &job.label);
        let domain_target = job.scope.bootout_domain(self.uid);
        let plist_path = job.path.to_string_lossy().to_string();
        match action {
            TriggerAction::Start => self.launchctl.start(&service_target),
            TriggerAction::Stop => self.launchctl.stop(&service_target),
            TriggerAction::Kickstart => self.launchctl.kickstart(&service_target),
            TriggerAction::Enable => self.launchctl.enable(&service_target),
            TriggerAction::Disable => self.launchctl.disable(&service_target),
            TriggerAction::Load => {
                if !job.path.exists() {
                    return Err(AppError::Validation(
                        "Load failed: plist file does not exist".to_string(),
                    ));
                }
                self.launchctl.bootstrap(&domain_target, &plist_path)
            }
            TriggerAction::Unload => match self.launchctl.bootout(&domain_target, &plist_path) {
                Ok(_) => Ok(()),
                Err(AppError::CommandFailed { stderr, .. })
                    if is_bootout_ignorable_error(&stderr) =>
                {
                    Ok(())
                }
                Err(err) => Err(err),
            },
        }
    }
}

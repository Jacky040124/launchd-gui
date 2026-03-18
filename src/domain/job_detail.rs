use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobRuntimeDetails {
    pub pid: Option<String>,
    pub last_exit_status: Option<String>,
    pub last_run: Option<String>,
    pub raw_hint: Option<String>,
}

impl JobRuntimeDetails {
    pub fn is_empty(&self) -> bool {
        self.pid.is_none()
            && self.last_exit_status.is_none()
            && self.last_run.is_none()
            && self.raw_hint.is_none()
    }
}

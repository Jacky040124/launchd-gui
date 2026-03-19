use crate::domain::job::JobSummary;
use crate::domain::status::JobStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TriStateFilter {
    #[default]
    Any,
    Yes,
    No,
}

impl TriStateFilter {
    pub fn cycle(self) -> Self {
        match self {
            Self::Any => Self::Yes,
            Self::Yes => Self::No,
            Self::No => Self::Any,
        }
    }

    pub fn matches_bool(self, value: Option<bool>) -> bool {
        match self {
            Self::Any => true,
            Self::Yes => value == Some(true),
            Self::No => value == Some(false),
        }
    }

    pub fn as_badge(self) -> &'static str {
        match self {
            Self::Any => "any",
            Self::Yes => "yes",
            Self::No => "no",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AdvancedFilter {
    pub status: Option<JobStatus>,
    pub disabled: TriStateFilter,
    pub run_at_load: TriStateFilter,
    pub keep_alive: TriStateFilter,
    pub has_error: TriStateFilter,
}

impl AdvancedFilter {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn matches(&self, job: &JobSummary) -> bool {
        let status_matches = self.status.is_none_or(|status| job.status == status);
        let disabled_matches = self.disabled.matches_bool(job.metadata.disabled);
        let run_at_load_matches = self.run_at_load.matches_bool(job.metadata.run_at_load);
        let keep_alive_matches = self.keep_alive.matches_bool(job.metadata.keep_alive);
        let has_error_matches = self.has_error.matches_bool(Some(job.error.is_some()));
        status_matches
            && disabled_matches
            && run_at_load_matches
            && keep_alive_matches
            && has_error_matches
    }

    pub fn badge_tokens(&self) -> Vec<String> {
        let mut tokens = Vec::new();
        if let Some(status) = self.status {
            tokens.push(format!("status:{}", status.as_str()));
        }

        if self.disabled != TriStateFilter::Any {
            tokens.push(format!("disabled:{}", self.disabled.as_badge()));
        }
        if self.run_at_load != TriStateFilter::Any {
            tokens.push(format!("run_at_load:{}", self.run_at_load.as_badge()));
        }
        if self.keep_alive != TriStateFilter::Any {
            tokens.push(format!("keep_alive:{}", self.keep_alive.as_badge()));
        }
        if self.has_error != TriStateFilter::Any {
            tokens.push(format!("has_error:{}", self.has_error.as_badge()));
        }

        tokens
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::domain::job::{JobCapabilities, JobMetadata, JobScope, JobSummary};
    use crate::domain::status::JobStatus;

    use super::{AdvancedFilter, TriStateFilter};

    #[test]
    fn tri_state_cycles_in_order() {
        assert_eq!(TriStateFilter::Any.cycle(), TriStateFilter::Yes);
        assert_eq!(TriStateFilter::Yes.cycle(), TriStateFilter::No);
        assert_eq!(TriStateFilter::No.cycle(), TriStateFilter::Any);
    }

    #[test]
    fn advanced_filter_matches_composite_conditions() {
        let filter = AdvancedFilter {
            status: Some(JobStatus::Loaded),
            disabled: TriStateFilter::No,
            run_at_load: TriStateFilter::Yes,
            keep_alive: TriStateFilter::Any,
            has_error: TriStateFilter::No,
        };
        let job = JobSummary {
            id: "id".to_string(),
            label: "label".to_string(),
            path: PathBuf::from("/tmp/demo.plist"),
            scope: JobScope::UserAgent,
            status: JobStatus::Loaded,
            is_starred: false,
            metadata: JobMetadata {
                run_at_load: Some(true),
                keep_alive: Some(false),
                disabled: Some(false),
            },
            capabilities: JobCapabilities {
                can_trigger: true,
                can_delete: true,
                trigger_reason: None,
                delete_reason: None,
            },
            error: None,
        };

        assert!(filter.matches(&job));
    }
}

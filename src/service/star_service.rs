use std::collections::HashSet;
use std::sync::Arc;

use crate::adapter::star_store::StarStore;
use crate::domain::job::JobSummary;
use crate::error::AppResult;

pub struct StarService {
    store: Arc<dyn StarStore>,
    starred_labels: HashSet<String>,
}

impl StarService {
    pub fn new(store: Arc<dyn StarStore>) -> Self {
        Self {
            store,
            starred_labels: HashSet::new(),
        }
    }

    pub fn load(&mut self) -> AppResult<()> {
        self.starred_labels = self.store.load_stars().unwrap_or_default();
        Ok(())
    }

    pub fn toggle(&mut self, label: &str) -> AppResult<bool> {
        let is_starred = if self.starred_labels.contains(label) {
            self.starred_labels.remove(label);
            false
        } else {
            self.starred_labels.insert(label.to_string());
            true
        };
        self.store.save_stars(&self.starred_labels)?;
        Ok(is_starred)
    }

    pub fn is_starred(&self, label: &str) -> bool {
        self.starred_labels.contains(label)
    }

    pub fn apply_to_jobs(&self, jobs: &mut [JobSummary]) {
        for job in jobs {
            job.is_starred = self.is_starred(&job.label);
        }
    }
}

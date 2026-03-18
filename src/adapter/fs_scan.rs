use std::env;
use std::fs;
use std::path::PathBuf;

use crate::domain::job::JobScope;
use crate::error::AppResult;

#[derive(Debug, Clone)]
pub struct FileScanner {
    targets: Vec<(PathBuf, JobScope)>,
}

impl FileScanner {
    pub fn new_default() -> Self {
        let mut targets = Vec::new();

        if let Ok(home) = env::var("HOME") {
            targets.push((
                PathBuf::from(home).join("Library/LaunchAgents"),
                JobScope::UserAgent,
            ));
        }

        targets.push((
            PathBuf::from("/Library/LaunchAgents"),
            JobScope::GlobalAgent,
        ));
        targets.push((
            PathBuf::from("/Library/LaunchDaemons"),
            JobScope::SystemDaemon,
        ));

        Self { targets }
    }

    pub fn with_targets(targets: Vec<(PathBuf, JobScope)>) -> Self {
        Self { targets }
    }

    pub fn scan(&self) -> AppResult<Vec<(PathBuf, JobScope)>> {
        let mut jobs = Vec::new();

        for (directory, scope) in &self.targets {
            if !directory.exists() {
                continue;
            }

            let entries = match fs::read_dir(directory) {
                Ok(entries) => entries,
                Err(_) => continue,
            };

            for entry in entries {
                let entry = entry?;
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }

                let is_plist = path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("plist"));

                if is_plist {
                    jobs.push((path, scope.clone()));
                }
            }
        }

        jobs.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(jobs)
    }
}

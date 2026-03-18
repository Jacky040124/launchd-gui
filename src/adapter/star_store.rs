use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

pub trait StarStore: Send + Sync {
    fn load_stars(&self) -> AppResult<HashSet<String>>;
    fn save_stars(&self, stars: &HashSet<String>) -> AppResult<()>;
}

#[derive(Debug, Clone)]
pub struct JsonStarStore {
    path: PathBuf,
}

impl JsonStarStore {
    pub fn new_default() -> Self {
        let path = env::var("HOME")
            .map(|home| PathBuf::from(home).join(".config/launchpad/stars.json"))
            .unwrap_or_else(|_| PathBuf::from(".launchpad-stars.json"));
        Self { path }
    }

    pub fn with_path(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl StarStore for JsonStarStore {
    fn load_stars(&self) -> AppResult<HashSet<String>> {
        if !self.path.exists() {
            return Ok(HashSet::new());
        }

        let raw = fs::read_to_string(&self.path)?;
        if raw.trim().is_empty() {
            return Ok(HashSet::new());
        }

        let stars = serde_json::from_str::<HashSet<String>>(&raw).or_else(|_| {
            let vec = serde_json::from_str::<Vec<String>>(&raw)?;
            Ok::<HashSet<String>, serde_json::Error>(vec.into_iter().collect())
        });

        stars.map_err(AppError::from)
    }

    fn save_stars(&self, stars: &HashSet<String>) -> AppResult<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let payload = serde_json::to_string_pretty(stars)?;
        fs::write(&self.path, payload)?;
        Ok(())
    }
}

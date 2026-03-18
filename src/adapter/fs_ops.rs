use std::fs;
use std::path::Path;

use crate::error::AppResult;

pub trait FsOps: Send + Sync {
    fn remove_file(&self, path: &Path) -> AppResult<()>;
}

#[derive(Debug, Default)]
pub struct SystemFsOps;

impl FsOps for SystemFsOps {
    fn remove_file(&self, path: &Path) -> AppResult<()> {
        fs::remove_file(path)?;
        Ok(())
    }
}

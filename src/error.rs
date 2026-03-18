use std::io;

use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("plist parse error: {0}")]
    Plist(#[from] plist::Error),

    #[error("command `{command}` failed: {stderr}")]
    CommandFailed { command: String, stderr: String },

    #[error("{0}")]
    Validation(String),
}

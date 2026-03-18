use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

impl DiagnosticSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticIssue {
    pub severity: DiagnosticSeverity,
    pub key: String,
    pub message: String,
    pub suggestion: String,
}

impl DiagnosticIssue {
    pub fn to_line(&self) -> String {
        format!(
            "[{}] {}: {} (建议: {})",
            self.severity.as_str(),
            self.key,
            self.message,
            self.suggestion
        )
    }
}

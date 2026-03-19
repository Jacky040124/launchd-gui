use std::path::Path;

use crate::error::AppResult;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlistSummary {
    pub label: Option<String>,
    pub run_at_load: Option<bool>,
    pub keep_alive: Option<bool>,
    pub disabled: Option<bool>,
}

pub trait PlistReader: Send + Sync {
    fn read_summary(&self, path: &Path) -> AppResult<PlistSummary>;
}

#[derive(Debug, Default)]
pub struct SystemPlistReader;

impl PlistReader for SystemPlistReader {
    fn read_summary(&self, path: &Path) -> AppResult<PlistSummary> {
        let value = plist::Value::from_file(path)?;
        let mut summary = PlistSummary::default();
        if let Some(dict) = value.as_dictionary() {
            summary.label = dict
                .get("Label")
                .and_then(|label| label.as_string())
                .map(ToOwned::to_owned);
            summary.run_at_load = dict.get("RunAtLoad").and_then(|value| value.as_boolean());
            summary.disabled = dict.get("Disabled").and_then(|value| value.as_boolean());
            summary.keep_alive = dict.get("KeepAlive").and_then(parse_keep_alive);
        }

        Ok(summary)
    }
}

fn parse_keep_alive(value: &plist::Value) -> Option<bool> {
    if let Some(bool_value) = value.as_boolean() {
        return Some(bool_value);
    }

    value.as_dictionary().map(|_| true)
}

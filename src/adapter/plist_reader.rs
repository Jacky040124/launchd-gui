use std::path::Path;

use crate::error::AppResult;

pub trait PlistReader: Send + Sync {
    fn read_label(&self, path: &Path) -> AppResult<Option<String>>;
}

#[derive(Debug, Default)]
pub struct SystemPlistReader;

impl PlistReader for SystemPlistReader {
    fn read_label(&self, path: &Path) -> AppResult<Option<String>> {
        let value = plist::Value::from_file(path)?;
        let label = value
            .as_dictionary()
            .and_then(|dict| dict.get("Label"))
            .and_then(|label| label.as_string())
            .map(ToOwned::to_owned);

        Ok(label)
    }
}

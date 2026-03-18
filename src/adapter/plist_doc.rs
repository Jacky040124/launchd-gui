use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::Path;

use plist::{Dictionary, Value};

use crate::domain::plist_document::{is_standard_managed_key, StandardPlistDocument};
use crate::error::AppResult;

pub trait PlistDocumentStore: Send + Sync {
    fn load_standard_document(&self, path: &Path) -> AppResult<StandardPlistDocument>;
    fn save_standard_document(
        &self,
        path: &Path,
        document: &StandardPlistDocument,
    ) -> AppResult<()>;
    fn to_xml(&self, document: &StandardPlistDocument) -> AppResult<String>;
}

#[derive(Debug, Default)]
pub struct SystemPlistDocumentStore;

impl PlistDocumentStore for SystemPlistDocumentStore {
    fn load_standard_document(&self, path: &Path) -> AppResult<StandardPlistDocument> {
        let value = Value::from_file(path)?;
        Ok(standard_document_from_value(&value))
    }

    fn save_standard_document(
        &self,
        path: &Path,
        document: &StandardPlistDocument,
    ) -> AppResult<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let value = standard_document_to_value(document);
        let mut file = fs::File::create(path)?;
        value.to_writer_xml(&mut file)?;
        Ok(())
    }

    fn to_xml(&self, document: &StandardPlistDocument) -> AppResult<String> {
        let value = standard_document_to_value(document);
        let mut output = Cursor::new(Vec::<u8>::new());
        value.to_writer_xml(&mut output)?;
        Ok(String::from_utf8_lossy(output.get_ref()).to_string())
    }
}

fn standard_document_from_value(value: &Value) -> StandardPlistDocument {
    let mut document = StandardPlistDocument::default();
    let Some(dict) = value.as_dictionary() else {
        return document;
    };

    document.label = dict
        .get("Label")
        .and_then(|value| value.as_string())
        .unwrap_or_default()
        .to_string();
    document.program = dict
        .get("Program")
        .and_then(|value| value.as_string())
        .unwrap_or_default()
        .to_string();
    document.program_arguments = dict
        .get("ProgramArguments")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_string().map(ToOwned::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    document.run_at_load = dict
        .get("RunAtLoad")
        .and_then(|value| value.as_boolean())
        .unwrap_or(false);
    document.keep_alive = dict
        .get("KeepAlive")
        .and_then(|value| {
            value
                .as_boolean()
                .or_else(|| value.as_dictionary().map(|_| true))
        })
        .unwrap_or(false);
    document.start_interval = dict.get("StartInterval").and_then(value_to_u64);
    document.working_directory = dict
        .get("WorkingDirectory")
        .and_then(|value| value.as_string())
        .map(ToOwned::to_owned);
    document.environment_variables = dict
        .get("EnvironmentVariables")
        .and_then(|value| value.as_dictionary())
        .map(dictionary_to_map)
        .unwrap_or_default();
    document.extra_string_keys = dict
        .iter()
        .filter_map(|(key, value)| {
            (!is_standard_managed_key(key))
                .then(|| {
                    value
                        .as_string()
                        .map(|text| (key.clone(), text.to_string()))
                })
                .flatten()
        })
        .collect();

    document
}

fn standard_document_to_value(document: &StandardPlistDocument) -> Value {
    let mut dict = Dictionary::new();
    dict.insert("Label".to_string(), Value::String(document.label.clone()));
    dict.insert(
        "Program".to_string(),
        Value::String(document.program.clone()),
    );

    if !document.program_arguments.is_empty() {
        dict.insert(
            "ProgramArguments".to_string(),
            Value::Array(
                document
                    .program_arguments
                    .iter()
                    .map(|arg| Value::String(arg.clone()))
                    .collect(),
            ),
        );
    }

    dict.insert(
        "RunAtLoad".to_string(),
        Value::Boolean(document.run_at_load),
    );
    dict.insert("KeepAlive".to_string(), Value::Boolean(document.keep_alive));

    if let Some(start_interval) = document.start_interval {
        dict.insert(
            "StartInterval".to_string(),
            Value::Integer(start_interval.into()),
        );
    }

    if let Some(working_directory) = &document.working_directory {
        if !working_directory.trim().is_empty() {
            dict.insert(
                "WorkingDirectory".to_string(),
                Value::String(working_directory.clone()),
            );
        }
    }

    if !document.environment_variables.is_empty() {
        let mut env_dict = Dictionary::new();
        for (key, value) in &document.environment_variables {
            env_dict.insert(key.clone(), Value::String(value.clone()));
        }
        dict.insert(
            "EnvironmentVariables".to_string(),
            Value::Dictionary(env_dict),
        );
    }

    for (key, value) in &document.extra_string_keys {
        if !is_standard_managed_key(key) && !key.trim().is_empty() {
            dict.insert(key.clone(), Value::String(value.clone()));
        }
    }

    Value::Dictionary(dict)
}

fn dictionary_to_map(dictionary: &Dictionary) -> BTreeMap<String, String> {
    dictionary
        .iter()
        .filter_map(|(key, value)| {
            value
                .as_string()
                .map(|text| (key.clone(), text.to_string()))
        })
        .collect()
}

fn value_to_u64(value: &Value) -> Option<u64> {
    value.as_unsigned_integer().or_else(|| {
        value
            .as_signed_integer()
            .and_then(|num| (num >= 0).then_some(num as u64))
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use tempfile::TempDir;

    use super::{PlistDocumentStore, SystemPlistDocumentStore};
    use crate::domain::plist_document::StandardPlistDocument;

    #[test]
    fn load_save_roundtrip_for_standard_document() {
        let temp_dir = TempDir::new().expect("temp dir");
        let path = temp_dir.path().join("com.demo.roundtrip.plist");
        let store = SystemPlistDocumentStore;
        let document = StandardPlistDocument {
            label: "com.demo.roundtrip".to_string(),
            program: "/usr/bin/true".to_string(),
            program_arguments: vec!["/usr/bin/true".to_string(), "--help".to_string()],
            run_at_load: true,
            keep_alive: false,
            start_interval: Some(60),
            working_directory: Some("/tmp".to_string()),
            environment_variables: BTreeMap::from([
                ("ENV_A".to_string(), "1".to_string()),
                ("ENV_B".to_string(), "2".to_string()),
            ]),
            extra_string_keys: BTreeMap::from([(
                "ThrottleInterval".to_string(),
                "120".to_string(),
            )]),
        };

        store
            .save_standard_document(&path, &document)
            .expect("save plist");
        let loaded = store.load_standard_document(&path).expect("load plist");

        assert_eq!(loaded, document);
    }

    #[test]
    fn xml_generation_contains_core_keys() {
        let store = SystemPlistDocumentStore;
        let document = StandardPlistDocument {
            label: "com.demo.xml".to_string(),
            program: "/usr/bin/echo".to_string(),
            run_at_load: true,
            ..StandardPlistDocument::default()
        };

        let xml = store.to_xml(&document).expect("xml");
        assert!(xml.contains("<key>Label</key>"));
        assert!(xml.contains("<string>com.demo.xml</string>"));
        assert!(xml.contains("<key>Program</key>"));
    }
}

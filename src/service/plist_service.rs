use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::adapter::plist_doc::PlistDocumentStore;
use crate::domain::job::JobScope;
use crate::domain::plist_document::StandardPlistDocument;
use crate::error::{AppError, AppResult};

pub struct PlistService {
    store: Arc<dyn PlistDocumentStore>,
    home_dir: Option<PathBuf>,
}

impl PlistService {
    pub fn new(store: Arc<dyn PlistDocumentStore>) -> Self {
        let home_dir = env::var("HOME").ok().map(PathBuf::from);
        Self { store, home_dir }
    }

    pub fn with_home_dir(store: Arc<dyn PlistDocumentStore>, home_dir: Option<PathBuf>) -> Self {
        Self { store, home_dir }
    }

    pub fn load_document(&self, path: &Path) -> AppResult<StandardPlistDocument> {
        self.store.load_standard_document(path)
    }

    pub fn save_existing(
        &self,
        path: &Path,
        scope: &JobScope,
        document: &StandardPlistDocument,
    ) -> AppResult<()> {
        self.validate_document(document)?;
        self.ensure_path_is_allowed(path, scope)?;
        self.store.save_standard_document(path, document)
    }

    pub fn create_new(
        &self,
        scope: JobScope,
        document: &StandardPlistDocument,
    ) -> AppResult<PathBuf> {
        self.validate_document(document)?;
        let path = self.new_job_path(&scope, &document.label)?;
        self.store.save_standard_document(&path, document)?;
        Ok(path)
    }

    pub fn xml_preview(&self, document: &StandardPlistDocument) -> AppResult<String> {
        self.validate_document(document)?;
        self.store.to_xml(document)
    }

    pub fn parse_program_arguments(&self, raw: &str) -> Vec<String> {
        raw.split_whitespace()
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }

    pub fn parse_env_pairs(&self, raw: &str) -> AppResult<BTreeMap<String, String>> {
        let mut envs = BTreeMap::new();
        for token in raw
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            let (key, value) = token.split_once('=').ok_or_else(|| {
                AppError::Validation(format!(
                    "Environment variable format invalid: `{token}` (expected KEY=VALUE)"
                ))
            })?;
            if key.trim().is_empty() {
                return Err(AppError::Validation(
                    "Environment variable key cannot be empty".to_string(),
                ));
            }
            envs.insert(key.trim().to_string(), value.trim().to_string());
        }
        Ok(envs)
    }

    pub fn format_env_pairs(&self, envs: &BTreeMap<String, String>) -> String {
        envs.iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn validate_document(&self, document: &StandardPlistDocument) -> AppResult<()> {
        if document.label.trim().is_empty() {
            return Err(AppError::Validation(
                "Label is required before saving plist".to_string(),
            ));
        }
        if document.program.trim().is_empty() {
            return Err(AppError::Validation(
                "Program is required before saving plist".to_string(),
            ));
        }
        Ok(())
    }

    fn ensure_path_is_allowed(&self, path: &Path, scope: &JobScope) -> AppResult<()> {
        let normalized = path.to_string_lossy();
        let is_allowed = match scope {
            JobScope::UserAgent => normalized.contains("/Library/LaunchAgents/"),
            JobScope::GlobalAgent => normalized.contains("/Library/LaunchAgents/"),
            JobScope::SystemDaemon => normalized.contains("/Library/LaunchDaemons/"),
        };
        if !is_allowed {
            return Err(AppError::Validation(format!(
                "Path is outside allowed scope directories: {}",
                path.display()
            )));
        }

        Ok(())
    }

    fn new_job_path(&self, scope: &JobScope, label: &str) -> AppResult<PathBuf> {
        let filename = format!("{label}.plist");
        let base = match scope {
            JobScope::UserAgent => {
                let Some(home_dir) = &self.home_dir else {
                    return Err(AppError::Validation(
                        "HOME is not available for creating user agent plist".to_string(),
                    ));
                };
                home_dir.join("Library/LaunchAgents")
            }
            JobScope::GlobalAgent => PathBuf::from("/Library/LaunchAgents"),
            JobScope::SystemDaemon => PathBuf::from("/Library/LaunchDaemons"),
        };
        Ok(base.join(filename))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use crate::adapter::plist_doc::PlistDocumentStore;
    use crate::domain::job::JobScope;
    use crate::domain::plist_document::StandardPlistDocument;
    use crate::error::{AppError, AppResult};

    use super::PlistService;

    #[derive(Debug, Default)]
    struct MockPlistStore {
        saved: Mutex<Vec<(PathBuf, StandardPlistDocument)>>,
    }

    impl PlistDocumentStore for MockPlistStore {
        fn load_standard_document(
            &self,
            _path: &std::path::Path,
        ) -> AppResult<StandardPlistDocument> {
            Ok(StandardPlistDocument::default())
        }

        fn save_standard_document(
            &self,
            path: &std::path::Path,
            document: &StandardPlistDocument,
        ) -> AppResult<()> {
            self.saved
                .lock()
                .expect("lock")
                .push((path.to_path_buf(), document.clone()));
            Ok(())
        }

        fn to_xml(&self, document: &StandardPlistDocument) -> AppResult<String> {
            Ok(format!(
                "<plist><string>{}</string></plist>",
                document.label
            ))
        }
    }

    #[test]
    fn create_new_generates_scope_specific_path() {
        let store = Arc::new(MockPlistStore::default());
        let service =
            PlistService::with_home_dir(store.clone(), Some(PathBuf::from("/Users/demo")));
        let doc = StandardPlistDocument {
            label: "com.demo.new".to_string(),
            program: "/bin/echo".to_string(),
            ..StandardPlistDocument::default()
        };

        let path = service
            .create_new(JobScope::UserAgent, &doc)
            .expect("create new");
        assert_eq!(
            path,
            PathBuf::from("/Users/demo/Library/LaunchAgents/com.demo.new.plist")
        );
    }

    #[test]
    fn save_existing_rejects_outside_scope() {
        let store = Arc::new(MockPlistStore::default());
        let service = PlistService::with_home_dir(store, Some(PathBuf::from("/Users/demo")));
        let doc = StandardPlistDocument {
            label: "com.demo.new".to_string(),
            program: "/bin/echo".to_string(),
            ..StandardPlistDocument::default()
        };

        let err = service
            .save_existing(
                PathBuf::from("/tmp/com.demo.new.plist").as_path(),
                &JobScope::UserAgent,
                &doc,
            )
            .expect_err("reject non-whitelisted path");

        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn parse_and_format_env_pairs_roundtrip() {
        let store = Arc::new(MockPlistStore::default());
        let service = PlistService::with_home_dir(store, Some(PathBuf::from("/Users/demo")));
        let parsed = service
            .parse_env_pairs("A=1, B = two")
            .expect("parse env pairs");
        assert_eq!(
            parsed,
            BTreeMap::from([
                ("A".to_string(), "1".to_string()),
                ("B".to_string(), "two".to_string())
            ])
        );
        assert_eq!(service.format_env_pairs(&parsed), "A=1, B=two");
    }
}

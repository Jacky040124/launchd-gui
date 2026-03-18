use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use launchpad::adapter::star_store::{JsonStarStore, StarStore};
use launchpad::error::AppResult;
use launchpad::service::star_service::StarService;
use tempfile::TempDir;

#[derive(Debug, Default)]
struct MockStarStore {
    stars: Mutex<HashSet<String>>,
}

impl StarStore for MockStarStore {
    fn load_stars(&self) -> AppResult<HashSet<String>> {
        Ok(self.stars.lock().expect("lock stars").clone())
    }

    fn save_stars(&self, stars: &HashSet<String>) -> AppResult<()> {
        *self.stars.lock().expect("lock stars") = stars.clone();
        Ok(())
    }
}

#[test]
fn toggle_adds_and_removes_star() {
    let store = Arc::new(MockStarStore::default());
    let mut service = StarService::new(store);
    service.load().expect("load stars");

    assert!(service.toggle("com.demo.star").expect("star"));
    assert!(service.is_starred("com.demo.star"));

    assert!(!service.toggle("com.demo.star").expect("unstar"));
    assert!(!service.is_starred("com.demo.star"));
}

#[test]
fn json_star_store_persists_to_disk() {
    let temp = TempDir::new().expect("temp dir");
    let path = temp.path().join("stars.json");
    let store = JsonStarStore::with_path(path.clone());
    let stars: HashSet<String> = ["com.demo.one".to_string(), "com.demo.two".to_string()]
        .into_iter()
        .collect();

    store.save_stars(&stars).expect("save stars");
    let loaded = store.load_stars().expect("load stars");
    assert_eq!(loaded, stars);
}

#[test]
fn json_star_store_accepts_legacy_vector_payload() {
    let temp = TempDir::new().expect("temp dir");
    let path: PathBuf = temp.path().join("stars.json");
    std::fs::write(&path, "[\"com.demo.legacy\"]").expect("write legacy payload");

    let store = JsonStarStore::with_path(path);
    let stars = store.load_stars().expect("load legacy stars");
    assert!(stars.contains("com.demo.legacy"));
}

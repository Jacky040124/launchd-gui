use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

use launchpad::adapter::quicklaunch::QuickLaunchItem;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = env::args().collect::<Vec<_>>();
    let command = args.get(1).map(String::as_str).unwrap_or("");
    let state_path = resolve_state_path(
        env::var("LAUNCHPAD_QUICKLAUNCH_STATE_PATH").ok(),
        env::var("HOME").ok(),
    );

    match command {
        "--sync-json" => {
            let mut payload = String::new();
            io::stdin().read_to_string(&mut payload)?;
            let items = parse_items_payload(&payload)?;
            write_state(&state_path, &items)?;
            println!("synced {} quicklaunch items", items.len());
        }
        "--list" => {
            let items = read_state(&state_path)?;
            for item in items {
                println!(
                    "{} | {} | {} | {} | updated:{}",
                    item.group, item.status, item.title, item.id, item.updated_at_unix_secs
                );
            }
        }
        "--summary" => {
            let items = read_state(&state_path)?;
            let grouped = summarize_groups(&items);
            for (group, count) in grouped {
                println!("{group}: {count}");
            }
        }
        _ => {
            eprintln!(
                "Usage:\n  quicklaunch_helper --sync-json\n  quicklaunch_helper --list\n  quicklaunch_helper --summary"
            );
            std::process::exit(2);
        }
    }

    Ok(())
}

fn parse_items_payload(payload: &str) -> Result<Vec<QuickLaunchItem>, serde_json::Error> {
    if payload.trim().is_empty() {
        Ok(Vec::new())
    } else {
        serde_json::from_str(payload)
    }
}

fn resolve_state_path(explicit: Option<String>, home: Option<String>) -> PathBuf {
    if let Some(explicit) = explicit.filter(|value| !value.trim().is_empty()) {
        return PathBuf::from(explicit);
    }

    if let Some(home) = home {
        return PathBuf::from(home).join(".config/launchpad/quicklaunch-helper-state.json");
    }

    PathBuf::from(".launchpad-quicklaunch-helper-state.json")
}

fn write_state(
    path: &PathBuf,
    items: &[QuickLaunchItem],
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_string_pretty(items)?;
    fs::write(path, payload)?;
    Ok(())
}

fn read_state(path: &PathBuf) -> Result<Vec<QuickLaunchItem>, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(path)?;
    Ok(parse_items_payload(&raw)?)
}

fn summarize_groups(items: &[QuickLaunchItem]) -> BTreeMap<String, usize> {
    let mut groups = BTreeMap::new();
    for item in items {
        *groups.entry(item.group.clone()).or_insert(0) += 1;
    }
    groups
}

#[cfg(test)]
mod tests {
    use launchpad::adapter::quicklaunch::QuickLaunchItem;

    use super::{parse_items_payload, resolve_state_path, summarize_groups};

    #[test]
    fn resolve_state_prefers_explicit_value() {
        let path = resolve_state_path(
            Some("/tmp/custom-quicklaunch.json".to_string()),
            Some("/Users/demo".to_string()),
        );
        assert_eq!(path.to_string_lossy(), "/tmp/custom-quicklaunch.json");
    }

    #[test]
    fn parse_empty_payload_returns_empty_list() {
        let items = parse_items_payload("").expect("parse");
        assert!(items.is_empty());
    }

    #[test]
    fn summarize_groups_counts_items() {
        let items = vec![
            QuickLaunchItem {
                id: "1".to_string(),
                title: "A".to_string(),
                status: "loaded".to_string(),
                group: "user-agent".to_string(),
                is_starred: true,
                updated_at_unix_secs: 1,
            },
            QuickLaunchItem {
                id: "2".to_string(),
                title: "B".to_string(),
                status: "running".to_string(),
                group: "user-agent".to_string(),
                is_starred: false,
                updated_at_unix_secs: 2,
            },
            QuickLaunchItem {
                id: "3".to_string(),
                title: "C".to_string(),
                status: "loaded".to_string(),
                group: "global-agent".to_string(),
                is_starred: true,
                updated_at_unix_secs: 3,
            },
        ];
        let grouped = summarize_groups(&items);
        assert_eq!(grouped.get("user-agent"), Some(&2));
        assert_eq!(grouped.get("global-agent"), Some(&1));
    }
}

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::path::PathBuf;

use launchpad::adapter::quicklaunch::{QuickLaunchAction, QuickLaunchItem};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = env::args().collect::<Vec<_>>();
    let command = args.get(1).map(String::as_str).unwrap_or("");
    let state_path = resolve_state_path(
        env::var("LAUNCHPAD_QUICKLAUNCH_STATE_PATH").ok(),
        env::var("HOME").ok(),
    );
    let actions_path = resolve_actions_path(
        env::var("LAUNCHPAD_QUICKLAUNCH_ACTIONS_PATH").ok(),
        &state_path,
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
        "--enqueue-action" => {
            let action = args.get(2).map(String::as_str).unwrap_or("");
            let job_ids_csv = args.get(3).map(String::as_str).unwrap_or("");
            let queued = enqueue_action(&actions_path, action, job_ids_csv)?;
            println!("queued action {action} for {queued} jobs");
        }
        "--drain-actions" => {
            let actions = drain_actions(&actions_path)?;
            println!("{}", serde_json::to_string(&actions)?);
        }
        _ => {
            eprintln!(
                "Usage:\n  quicklaunch_helper --sync-json\n  quicklaunch_helper --list\n  quicklaunch_helper --summary\n  quicklaunch_helper --enqueue-action <action> <id1,id2,...>\n  quicklaunch_helper --drain-actions"
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

fn resolve_actions_path(explicit: Option<String>, state_path: &Path) -> PathBuf {
    if let Some(explicit) = explicit.filter(|value| !value.trim().is_empty()) {
        return PathBuf::from(explicit);
    }

    let parent = state_path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    parent.join("quicklaunch-helper-actions.json")
}

fn write_state(path: &Path, items: &[QuickLaunchItem]) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_string_pretty(items)?;
    fs::write(path, payload)?;
    Ok(())
}

fn read_state(path: &Path) -> Result<Vec<QuickLaunchItem>, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(path)?;
    Ok(parse_items_payload(&raw)?)
}

fn write_actions(
    path: &Path,
    actions: &[QuickLaunchAction],
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_string_pretty(actions)?;
    fs::write(path, payload)?;
    Ok(())
}

fn read_actions(path: &Path) -> Result<Vec<QuickLaunchAction>, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_str(&raw)?)
}

fn parse_job_ids_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn enqueue_action(
    actions_path: &Path,
    action: &str,
    job_ids_csv: &str,
) -> Result<usize, Box<dyn std::error::Error>> {
    let normalized_action = action.trim().to_ascii_lowercase();
    if normalized_action.is_empty() {
        return Err("action is required".into());
    }
    let job_ids = parse_job_ids_csv(job_ids_csv);
    if job_ids.is_empty() {
        return Err("at least one job id is required".into());
    }

    let mut queued = read_actions(actions_path)?;
    queued.push(QuickLaunchAction {
        action: normalized_action,
        job_ids: job_ids.clone(),
    });
    write_actions(actions_path, &queued)?;
    Ok(job_ids.len())
}

fn drain_actions(path: &Path) -> Result<Vec<QuickLaunchAction>, Box<dyn std::error::Error>> {
    let queued = read_actions(path)?;
    write_actions(path, &[])?;
    Ok(queued)
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
    use std::path::PathBuf;

    use tempfile::TempDir;

    use launchpad::adapter::quicklaunch::QuickLaunchItem;

    use super::{
        drain_actions, enqueue_action, parse_items_payload, parse_job_ids_csv,
        resolve_actions_path, resolve_state_path, summarize_groups,
    };

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

    #[test]
    fn resolve_actions_path_prefers_explicit_value() {
        let state = PathBuf::from("/tmp/state.json");
        let path = resolve_actions_path(Some("/tmp/actions.json".to_string()), &state);
        assert_eq!(path.to_string_lossy(), "/tmp/actions.json");
    }

    #[test]
    fn parse_job_ids_csv_ignores_empty_segments() {
        let parsed = parse_job_ids_csv("a, b,, ,c");
        assert_eq!(
            parsed,
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }

    #[test]
    fn enqueue_and_drain_actions_roundtrip() {
        let temp = TempDir::new().expect("temp");
        let path = temp.path().join("actions.json");

        let queued = enqueue_action(&path, "start", "id-1,id-2").expect("enqueue");
        assert_eq!(queued, 2);
        enqueue_action(&path, "disable", "id-3").expect("enqueue");

        let drained = drain_actions(&path).expect("drain");
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].action, "start");
        assert_eq!(drained[1].action, "disable");

        let drained_again = drain_actions(&path).expect("drain");
        assert!(drained_again.is_empty());
    }
}

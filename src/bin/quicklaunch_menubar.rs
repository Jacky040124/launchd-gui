#[cfg(target_os = "macos")]
mod macos {
    use std::process::Command;
    use std::time::{Duration, Instant};

    use launchpad::adapter::launchctl::current_uid;
    use launchpad::adapter::quicklaunch::QuickLaunchItem;
    use launchpad::domain::action::TriggerAction;
    use launchpad::domain::job::JobScope;
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
    use tray_icon::{Icon, TrayIconBuilder};

    const SUMMARY_REFRESH_INTERVAL: Duration = Duration::from_secs(6);

    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let helper = std::env::var("LAUNCHPAD_QUICKLAUNCH_HELPER")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "quicklaunch_helper".to_string());

        let menu = Menu::new();
        let status_item = MenuItem::new("LaunchPad QuickLaunch", false, None);
        menu.append(&status_item)?;
        menu.append(&PredefinedMenuItem::separator())?;

        let starred = Submenu::new("Starred", true);
        let starred_start = MenuItem::new("Start Starred", true, None);
        let starred_stop = MenuItem::new("Stop Starred", true, None);
        let starred_restart = MenuItem::new("Restart Starred", true, None);
        let starred_enable = MenuItem::new("Enable Starred", true, None);
        let starred_disable = MenuItem::new("Disable Starred", true, None);
        starred.append(&starred_start)?;
        starred.append(&starred_stop)?;
        starred.append(&starred_restart)?;
        starred.append(&starred_enable)?;
        starred.append(&starred_disable)?;
        menu.append(&starred)?;

        let groups = Submenu::new("Groups", true);
        let user_start = MenuItem::new("Start user-agent", true, None);
        let user_stop = MenuItem::new("Stop user-agent", true, None);
        let user_restart = MenuItem::new("Restart user-agent", true, None);
        let global_start = MenuItem::new("Start global-agent", true, None);
        let global_stop = MenuItem::new("Stop global-agent", true, None);
        let global_restart = MenuItem::new("Restart global-agent", true, None);
        groups.append(&user_start)?;
        groups.append(&user_stop)?;
        groups.append(&user_restart)?;
        groups.append(&global_start)?;
        groups.append(&global_stop)?;
        groups.append(&global_restart)?;
        menu.append(&groups)?;

        menu.append(&PredefinedMenuItem::separator())?;
        let refresh_summary = MenuItem::new("Refresh Summary", true, None);
        let quit = MenuItem::new("Quit", true, None);
        menu.append(&refresh_summary)?;
        menu.append(&quit)?;

        let icon = build_icon()?;
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("LaunchPad QuickLaunch")
            .with_icon(icon)
            .build()?;

        let mut last_summary_update = Instant::now()
            .checked_sub(SUMMARY_REFRESH_INTERVAL)
            .unwrap_or_else(Instant::now);
        let uid = current_uid();

        loop {
            if last_summary_update.elapsed() >= SUMMARY_REFRESH_INTERVAL {
                let summary = fetch_summary(helper.as_str())
                    .unwrap_or_else(|err| format!("summary unavailable: {err}"));
                status_item.set_text(format!("QuickLaunch · {summary}"));
                let _ = tray.set_tooltip(Some(format!("LaunchPad QuickLaunch · {summary}")));
                last_summary_update = Instant::now();
            }

            match MenuEvent::receiver().recv_timeout(Duration::from_millis(400)) {
                Ok(event) => {
                    if event.id == quit.id() {
                        break;
                    }
                    if event.id == refresh_summary.id() {
                        let summary = fetch_summary(helper.as_str())
                            .unwrap_or_else(|err| format!("summary unavailable: {err}"));
                        status_item.set_text(format!("QuickLaunch · {summary}"));
                        let _ =
                            tray.set_tooltip(Some(format!("LaunchPad QuickLaunch · {summary}")));
                        last_summary_update = Instant::now();
                        continue;
                    }

                    let executed = if event.id == starred_start.id() {
                        execute_starred_action(helper.as_str(), "start", uid)
                    } else if event.id == starred_stop.id() {
                        execute_starred_action(helper.as_str(), "stop", uid)
                    } else if event.id == starred_restart.id() {
                        execute_starred_action(helper.as_str(), "restart", uid)
                    } else if event.id == starred_enable.id() {
                        execute_starred_action(helper.as_str(), "enable", uid)
                    } else if event.id == starred_disable.id() {
                        execute_starred_action(helper.as_str(), "disable", uid)
                    } else if event.id == user_start.id() {
                        execute_group_action(helper.as_str(), "user-agent", "start", uid)
                    } else if event.id == user_stop.id() {
                        execute_group_action(helper.as_str(), "user-agent", "stop", uid)
                    } else if event.id == user_restart.id() {
                        execute_group_action(helper.as_str(), "user-agent", "restart", uid)
                    } else if event.id == global_start.id() {
                        execute_group_action(helper.as_str(), "global-agent", "start", uid)
                    } else if event.id == global_stop.id() {
                        execute_group_action(helper.as_str(), "global-agent", "stop", uid)
                    } else if event.id == global_restart.id() {
                        execute_group_action(helper.as_str(), "global-agent", "restart", uid)
                    } else {
                        Ok("No action".to_string())
                    };

                    match executed {
                        Ok(message) => {
                            status_item.set_text(message.clone());
                            let _ = tray.set_tooltip(Some(message));
                        }
                        Err(err) => {
                            let msg = format!("Action execution failed: {err}");
                            status_item.set_text(msg.clone());
                            let _ = tray.set_tooltip(Some(msg));
                        }
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        Ok(())
    }

    fn build_icon() -> Result<Icon, Box<dyn std::error::Error>> {
        let width = 16;
        let height = 16;
        let mut rgba = Vec::with_capacity(width * height * 4);
        for _ in 0..(width * height) {
            rgba.extend_from_slice(&[0x17, 0x5c, 0xd3, 0xff]);
        }
        Ok(Icon::from_rgba(rgba, width as u32, height as u32)?)
    }

    fn fetch_summary(helper: &str) -> Result<String, Box<dyn std::error::Error>> {
        let output = Command::new(helper).arg("--summary").output()?;
        if !output.status.success() {
            return Err(format!(
                "helper summary failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )
            .into());
        }
        let summary = String::from_utf8_lossy(&output.stdout);
        let normalized = summary
            .lines()
            .filter(|line| !line.trim().is_empty())
            .take(3)
            .collect::<Vec<_>>()
            .join(" | ");
        if normalized.is_empty() {
            Ok("no tracked items".to_string())
        } else {
            Ok(normalized)
        }
    }

    fn fetch_items(helper: &str) -> Result<Vec<QuickLaunchItem>, Box<dyn std::error::Error>> {
        let output = Command::new(helper).arg("--list-json").output()?;
        if !output.status.success() {
            return Err(format!(
                "helper list-json failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )
            .into());
        }
        let raw = String::from_utf8_lossy(&output.stdout);
        if raw.trim().is_empty() {
            return Ok(Vec::new());
        }
        Ok(serde_json::from_str(&raw)?)
    }

    fn execute_starred_action(
        helper: &str,
        action: &str,
        uid: u32,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let items = fetch_items(helper)?;
        execute_action_for_items(items.iter().filter(|item| item.is_starred), action, uid)
    }

    fn execute_group_action(
        helper: &str,
        group: &str,
        action: &str,
        uid: u32,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let items = fetch_items(helper)?;
        execute_action_for_items(items.iter().filter(|item| item.group == group), action, uid)
    }

    fn execute_action_for_items<'a>(
        items: impl Iterator<Item = &'a QuickLaunchItem>,
        action: &str,
        uid: u32,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let trigger = TriggerAction::from_ui_value(action)
            .ok_or_else(|| format!("unsupported action: {action}"))?;
        let mut matched = 0usize;
        let mut success = 0usize;
        let mut failed = 0usize;
        let mut skipped = 0usize;

        for item in items {
            matched += 1;
            let Some((scope, label)) = parse_scope_and_label(item.title.as_str()) else {
                skipped += 1;
                continue;
            };
            match execute_launchctl_action(&scope, &label, trigger, uid) {
                Ok(_) => success += 1,
                Err(_) => failed += 1,
            }
        }

        if matched == 0 {
            Ok(format!(
                "No matching QuickLaunch items for action '{action}'"
            ))
        } else {
            Ok(format!(
                "Executed {action}: {success} success, {failed} failed, {skipped} skipped"
            ))
        }
    }

    fn parse_scope_and_label(title: &str) -> Option<(JobScope, String)> {
        let trimmed = title.trim();
        let bracket_start = trimmed.find('[')?;
        let bracket_end = trimmed.find(']')?;
        if bracket_end <= bracket_start + 1 {
            return None;
        }
        let scope_raw = &trimmed[bracket_start + 1..bracket_end];
        let label = trimmed.get(bracket_end + 1..)?.trim();
        if label.is_empty() {
            return None;
        }
        let scope = match scope_raw {
            "user-agent" => JobScope::UserAgent,
            "global-agent" => JobScope::GlobalAgent,
            "system-daemon" => JobScope::SystemDaemon,
            _ => return None,
        };
        Some((scope, label.to_string()))
    }

    fn execute_launchctl_action(
        scope: &JobScope,
        label: &str,
        action: TriggerAction,
        uid: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let target = scope.target_for_label(uid, label);
        let args: Vec<&str> = match action {
            TriggerAction::Start => vec!["start", target.as_str()],
            TriggerAction::Stop => vec!["stop", target.as_str()],
            TriggerAction::Kickstart => vec!["kickstart", target.as_str()],
            TriggerAction::Enable => vec!["enable", target.as_str()],
            TriggerAction::Disable => vec!["disable", target.as_str()],
            TriggerAction::Load | TriggerAction::Unload => {
                return Err("load/unload are not supported in menubar direct mode".into());
            }
        };
        let output = Command::new("launchctl").args(args).output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "launchctl failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )
            .into())
        }
    }
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    macos::run()
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("quicklaunch_menubar is only available on macOS builds.");
}

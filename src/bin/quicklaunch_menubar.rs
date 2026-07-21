#[cfg(target_os = "macos")]
mod macos {
    use std::env;
    use std::process::Command;
    use std::time::{Duration, Instant};

    use launchpad::adapter::quicklaunch::QuickLaunchItem;
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
    use tray_icon::{Icon, TrayIconBuilder};

    const SUMMARY_REFRESH_INTERVAL: Duration = Duration::from_secs(6);
    const DEFAULT_MENU_ITEMS_LIMIT: usize = 8;

    struct ItemActionControl {
        job_id: String,
        title: String,
        start: MenuItem,
        stop: MenuItem,
        restart: MenuItem,
        enable: MenuItem,
        disable: MenuItem,
    }

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

        let max_item_entries = env::var("LAUNCHPAD_QUICKLAUNCH_MENUBAR_MAX_ITEMS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_MENU_ITEMS_LIMIT);
        let items_submenu = Submenu::new("Items", true);
        let mut item_controls: Vec<ItemActionControl> = Vec::new();
        match fetch_items(helper.as_str()) {
            Ok(items) if !items.is_empty() => {
                for item in items.into_iter().take(max_item_entries) {
                    let label = format!(
                        "{} [{}]",
                        shorten_title(item.title.as_str(), 48),
                        item.status
                    );
                    let item_menu = Submenu::new(label.as_str(), true);
                    let start = MenuItem::new("Start", true, None);
                    let stop = MenuItem::new("Stop", true, None);
                    let restart = MenuItem::new("Restart", true, None);
                    let enable = MenuItem::new("Enable", true, None);
                    let disable = MenuItem::new("Disable", true, None);
                    item_menu.append(&start)?;
                    item_menu.append(&stop)?;
                    item_menu.append(&restart)?;
                    item_menu.append(&enable)?;
                    item_menu.append(&disable)?;
                    items_submenu.append(&item_menu)?;
                    item_controls.push(ItemActionControl {
                        job_id: item.id,
                        title: item.title,
                        start,
                        stop,
                        restart,
                        enable,
                        disable,
                    });
                }
            }
            _ => {
                let no_items = MenuItem::new("No synced items yet", false, None);
                items_submenu.append(&no_items)?;
            }
        }
        menu.append(&items_submenu)?;

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

        loop {
            if last_summary_update.elapsed() >= SUMMARY_REFRESH_INTERVAL {
                let (badge, detail) = fetch_status_badge(helper.as_str())
                    .unwrap_or_else(|err| ("⚪ summary unavailable".to_string(), err.to_string()));
                status_item.set_text(format!("QuickLaunch · {badge}"));
                let _ = tray.set_tooltip(Some(format!("LaunchPad QuickLaunch · {detail}")));
                last_summary_update = Instant::now();
            }

            match MenuEvent::receiver().recv_timeout(Duration::from_millis(400)) {
                Ok(event) => {
                    if event.id == quit.id() {
                        break;
                    }
                    if event.id == refresh_summary.id() {
                        let (badge, detail) =
                            fetch_status_badge(helper.as_str()).unwrap_or_else(|err| {
                                ("⚪ summary unavailable".to_string(), err.to_string())
                            });
                        status_item.set_text(format!("QuickLaunch · {badge}"));
                        let _ = tray.set_tooltip(Some(format!("LaunchPad QuickLaunch · {detail}")));
                        last_summary_update = Instant::now();
                        continue;
                    }

                    let mut queued = if event.id == starred_start.id() {
                        enqueue_starred_action(helper.as_str(), "start")
                    } else if event.id == starred_stop.id() {
                        enqueue_starred_action(helper.as_str(), "stop")
                    } else if event.id == starred_restart.id() {
                        enqueue_starred_action(helper.as_str(), "restart")
                    } else if event.id == starred_enable.id() {
                        enqueue_starred_action(helper.as_str(), "enable")
                    } else if event.id == starred_disable.id() {
                        enqueue_starred_action(helper.as_str(), "disable")
                    } else if event.id == user_start.id() {
                        enqueue_group_action(helper.as_str(), "user-agent", "start")
                    } else if event.id == user_stop.id() {
                        enqueue_group_action(helper.as_str(), "user-agent", "stop")
                    } else if event.id == user_restart.id() {
                        enqueue_group_action(helper.as_str(), "user-agent", "restart")
                    } else if event.id == global_start.id() {
                        enqueue_group_action(helper.as_str(), "global-agent", "start")
                    } else if event.id == global_stop.id() {
                        enqueue_group_action(helper.as_str(), "global-agent", "stop")
                    } else if event.id == global_restart.id() {
                        enqueue_group_action(helper.as_str(), "global-agent", "restart")
                    } else {
                        Ok("No action".to_string())
                    };
                    if queued.as_ref().is_ok_and(|message| message == "No action") {
                        for control in &item_controls {
                            let action = if event.id == control.start.id() {
                                Some("start")
                            } else if event.id == control.stop.id() {
                                Some("stop")
                            } else if event.id == control.restart.id() {
                                Some("restart")
                            } else if event.id == control.enable.id() {
                                Some("enable")
                            } else if event.id == control.disable.id() {
                                Some("disable")
                            } else {
                                None
                            };

                            if let Some(action) = action {
                                queued = enqueue_job_action(
                                    helper.as_str(),
                                    action,
                                    control.job_id.as_str(),
                                    control.title.as_str(),
                                );
                                break;
                            }
                        }
                    }

                    match queued {
                        Ok(message) => {
                            let queued =
                                format!("{message} (auto-applies shortly; refresh is optional)");
                            status_item.set_text(queued.clone());
                            let _ = tray.set_tooltip(Some(queued));
                        }
                        Err(err) => {
                            let msg = format!("Action queue failed: {err}");
                            status_item.set_text(msg.clone());
                            let _ = tray.set_tooltip(Some(msg));
                        }
                    }
                }
                Err(error) if error.is_disconnected() => break,
                Err(_) => {}
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

    fn shorten_title(value: &str, limit: usize) -> String {
        if value.chars().count() <= limit {
            return value.to_string();
        }
        let short = value.chars().take(limit).collect::<String>();
        format!("{short}…")
    }

    fn fetch_status_badge(helper: &str) -> Result<(String, String), Box<dyn std::error::Error>> {
        let items = fetch_items(helper)?;
        if items.is_empty() {
            return Ok((
                "⚪ no tracked items".to_string(),
                "no tracked items".to_string(),
            ));
        }

        let mut running = 0usize;
        let mut loaded = 0usize;
        let mut disabled = 0usize;
        let mut unknown = 0usize;
        for item in &items {
            match item.status.as_str() {
                "running" => running += 1,
                "loaded" => loaded += 1,
                "disabled" => disabled += 1,
                _ => unknown += 1,
            }
        }
        let badge = if running > 0 {
            format!("🟢 R{running} L{loaded} D{disabled}")
        } else if disabled > 0 {
            format!("🟡 R{running} L{loaded} D{disabled}")
        } else {
            format!("🔵 R{running} L{loaded} D{disabled}")
        };
        let detail = format!(
            "items={} running={} loaded={} disabled={} unknown={}",
            items.len(),
            running,
            loaded,
            disabled,
            unknown
        );
        Ok((badge, detail))
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
            Ok(Vec::new())
        } else {
            Ok(serde_json::from_str(raw.as_ref())?)
        }
    }

    fn enqueue_starred_action(
        helper: &str,
        action: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let output = Command::new(helper)
            .args(["--enqueue-starred-action", action])
            .output()?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(format!(
                "enqueue starred action failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )
            .into())
        }
    }

    fn enqueue_group_action(
        helper: &str,
        group: &str,
        action: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let output = Command::new(helper)
            .args(["--enqueue-group-action", group, action])
            .output()?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(format!(
                "enqueue group action failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )
            .into())
        }
    }

    fn enqueue_job_action(
        helper: &str,
        action: &str,
        job_id: &str,
        title: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let output = Command::new(helper)
            .args(["--enqueue-action", action, job_id])
            .output()?;
        if output.status.success() {
            let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok(format!("{result} · {}", shorten_title(title, 40)))
        } else {
            Err(format!(
                "enqueue job action failed: {}",
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

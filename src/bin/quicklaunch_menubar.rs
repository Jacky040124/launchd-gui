#[cfg(target_os = "macos")]
mod macos {
    use std::process::Command;
    use std::time::{Duration, Instant};

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
        let starred_enable = MenuItem::new("Enable Starred", true, None);
        let starred_disable = MenuItem::new("Disable Starred", true, None);
        starred.append(&starred_start)?;
        starred.append(&starred_stop)?;
        starred.append(&starred_enable)?;
        starred.append(&starred_disable)?;
        menu.append(&starred)?;

        let groups = Submenu::new("Groups", true);
        let user_start = MenuItem::new("Start user-agent", true, None);
        let user_stop = MenuItem::new("Stop user-agent", true, None);
        let global_start = MenuItem::new("Start global-agent", true, None);
        let global_stop = MenuItem::new("Stop global-agent", true, None);
        groups.append(&user_start)?;
        groups.append(&user_stop)?;
        groups.append(&global_start)?;
        groups.append(&global_stop)?;
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

                    let queued = if event.id == starred_start.id() {
                        enqueue_starred_action(helper.as_str(), "start")
                    } else if event.id == starred_stop.id() {
                        enqueue_starred_action(helper.as_str(), "stop")
                    } else if event.id == starred_enable.id() {
                        enqueue_starred_action(helper.as_str(), "enable")
                    } else if event.id == starred_disable.id() {
                        enqueue_starred_action(helper.as_str(), "disable")
                    } else if event.id == user_start.id() {
                        enqueue_group_action(helper.as_str(), "user-agent", "start")
                    } else if event.id == user_stop.id() {
                        enqueue_group_action(helper.as_str(), "user-agent", "stop")
                    } else if event.id == global_start.id() {
                        enqueue_group_action(helper.as_str(), "global-agent", "start")
                    } else if event.id == global_stop.id() {
                        enqueue_group_action(helper.as_str(), "global-agent", "stop")
                    } else {
                        Ok("No action".to_string())
                    };

                    match queued {
                        Ok(message) => {
                            status_item.set_text(message.clone());
                            let _ = tray.set_tooltip(Some(message));
                        }
                        Err(err) => {
                            let msg = format!("Action queue failed: {err}");
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
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    macos::run()
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("quicklaunch_menubar is only available on macOS builds.");
}

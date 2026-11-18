//! Programs to choose from: installed, running, and the one picked by clicking it (capture focus).

use super::*;

#[derive(Serialize)]
pub(crate) struct InstalledApp {
    name: String,
    /// A Start Menu shortcut, a program, or a Store app id.
    path: String,
}

/// Installed apps for the action field. Cached; the first call can take a few seconds.
#[tauri::command]
pub async fn installed_apps() -> Vec<InstalledApp> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(|| {
            let apps = oasis_platform::windows::app_discovery::windows_apps().installed_apps();
            apps.into_iter()
                .map(|(name, path)| InstalledApp { name, path })
                .collect()
        })
        .await
        .unwrap_or_default()
    }
    #[cfg(not(windows))]
    Vec::new()
}

#[derive(Serialize)]
pub(crate) struct RunningApp {
    pub(crate) name: String,
    /// The program's file, which the Launch action can start again.
    pub(crate) path: String,
    title: String,
}

/// Programs with a window open right now (one PowerShell call).
#[cfg(windows)]
pub(crate) fn running_programs() -> Vec<RunningApp> {
    use std::os::windows::process::CommandExt;
    const SCRIPT: &str = "Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowTitle } | ForEach-Object { [pscustomobject]@{ n = $_.ProcessName; p = $_.Path; t = $_.MainWindowTitle } } | ConvertTo-Json -Compress";
    let Ok(out) = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .creation_flags(0x0800_0000) // no console window
        .output()
    else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
        return Vec::new();
    };
    // One result comes back as an object, several as an array.
    let rows = match json {
        serde_json::Value::Array(rows) => rows,
        one @ serde_json::Value::Object(_) => vec![one],
        _ => Vec::new(),
    };
    let text = |row: &serde_json::Value, key: &str| {
        row.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    let mut seen = std::collections::BTreeSet::new();
    let mut apps = Vec::new();
    for row in &rows {
        let (name, path, title) = (text(row, "n"), text(row, "p"), text(row, "t"));
        // Skip what cannot be launched again, this app itself, and the Store-app frame.
        if path.is_empty()
            || name.eq_ignore_ascii_case("oasis-desktop")
            || name.eq_ignore_ascii_case("ApplicationFrameHost")
        {
            continue;
        }
        if seen.insert(path.to_lowercase()) {
            apps.push(RunningApp { name, path, title });
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

/// Programs with a window open right now, for the action field. Asked for when the box is clicked
/// rather than kept.
#[tauri::command]
pub async fn running_apps() -> Vec<RunningApp> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(running_programs)
            .await
            .unwrap_or_default()
    }
    #[cfg(not(windows))]
    Vec::new()
}

/// What was in front when the user clicked a section, for "pick the section" on a new set.
#[derive(Serialize)]
pub(crate) struct Captured {
    panel_id: String,
    panel_label: String,
    /// What to put in "window title or address contains": the site for a browser, else the title.
    context: String,
    /// The program clicked, as its process name ("code", "chrome").
    process: String,
}

/// What the overlay says while a section is being picked: seconds `left` to click, or what was `done` (caught).
#[derive(Serialize, Clone)]
pub(crate) struct PickPrompt {
    left: Option<u64>,
    done: Option<String>,
}

/// "Pick the section": OASIS gets out of the way (minimised), the overlay asks for a click and counts down,
/// and the first click in another program is read: which program and which part of it has the focus. Then
/// OASIS comes back. Esc, or no click in time, gives up.
#[tauri::command]
pub async fn capture_focus(app: AppHandle) -> Result<Captured, String> {
    #[cfg(windows)]
    {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.minimize();
        }
        let waiting = app.clone();
        let result = tauri::async_runtime::spawn_blocking(move || wait_for_pick(&waiting))
            .await
            .map_err(|e| e.to_string())?;
        let said = result.as_ref().ok().map(|c| {
            let what = if !c.panel_label.is_empty() {
                c.panel_label.clone()
            } else if !c.context.is_empty() {
                c.context.clone()
            } else {
                c.process.clone()
            };
            PickPrompt {
                left: None,
                done: Some(format!("Caught: {what}")),
            }
        });
        let _ = app.emit_to(overlay::LABEL, "pick-prompt", said);
        show_window(&app);
        result
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("Picking a section is only available on Windows.".into())
    }
}

/// How long the overlay waits for the click.
#[cfg(windows)]
pub(crate) const PICK_SECONDS: u64 = 20;

/// Waits for a click in a program other than OASIS and reads what it gave the focus to.
#[cfg(windows)]
pub(crate) fn wait_for_pick(app: &AppHandle) -> Result<Captured, String> {
    use oasis_win::keyboard::is_down;
    const LEFT_BUTTON: u16 = 0x01;
    const ESC: u16 = 0x1B;
    let start = std::time::Instant::now();
    let mut was_down = is_down(LEFT_BUTTON);
    let mut said = u64::MAX;
    loop {
        let left = PICK_SECONDS.saturating_sub(start.elapsed().as_secs());
        if left != said {
            said = left;
            let _ = app.emit_to(
                overlay::LABEL,
                "pick-prompt",
                Some(PickPrompt {
                    left: Some(left),
                    done: None,
                }),
            );
        }
        if left == 0 {
            return Err(format!(
                "No click in {PICK_SECONDS} seconds. Press Pick the section to try again."
            ));
        }
        if is_down(ESC) {
            return Err("Picking was cancelled.".into());
        }
        let down = is_down(LEFT_BUTTON);
        if was_down && !down {
            // the click has finished; give the program a moment to move the focus where it was clicked
            std::thread::sleep(std::time::Duration::from_millis(250));
            if let Ok(found) = read_focus() {
                return Ok(found);
            } // a click on OASIS itself (its taskbar button, say) does not count: keep waiting
        }
        was_down = down;
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
}

/// The program and panel that have the focus right now.
#[cfg(windows)]
pub(crate) fn read_focus() -> Result<Captured, String> {
    let mut tracker = oasis_platform::windows::foreground::foreground_app();
    tracker.set_panel_profiles_exist(true);
    let info = tracker.current_with_fresh_url();
    if info.process.eq_ignore_ascii_case("oasis-desktop") || info.process.is_empty() {
        return Err("OASIS itself was in front. Click the other program's section.".to_string());
    }
    let known = info.panel_id != "unknown" && !info.panel_id.is_empty();
    let site = info
        .browser_url
        .as_deref()
        .and_then(|u| u.split("://").nth(1))
        .and_then(|r| r.split('/').next())
        .unwrap_or("");
    // A panel is the condition by itself; a title would change with every file.
    let context = if known {
        String::new()
    } else if !site.is_empty() {
        site.to_string()
    } else {
        info.window_title.clone()
    };
    Ok(Captured {
        panel_id: if known { info.panel_id } else { String::new() },
        panel_label: info.panel_label,
        context,
        process: info.process,
    })
}

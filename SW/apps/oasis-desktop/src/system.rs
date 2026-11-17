//! Starting up: hand over to the administrator copy when that mode is on, keep to a single
//! copy, honour `--tray`, and the switches for "start with Windows" and "run as administrator".
//! Rules are in `oasis_core::startup`; the Windows calls are in `oasis_platform::windows::startup`.
//!
//! While rehearsing on a copy of the data (`OASIS_DATA_DIR`), the lock gets its own name so a
//! rehearsal can run beside the real app. The two switches still act on the real Windows login
//! entry and task, because that is what they are for.

use std::time::{Duration, Instant};

use oasis_core::startup::LaunchArgs;
use serde::Serialize;
use tauri::AppHandle;

/// What `begin` decided.
pub enum Begin {
    /// Carry on. `lock` must be kept alive until the app exits.
    Run(Startup),
    /// Another copy is running (and was asked to show itself), or the administrator copy took over.
    Exit,
}

pub struct Startup {
    /// Start with the window shown (not a tray start, or someone asked for it).
    pub show_window: bool,
    #[cfg(windows)]
    pub lock: Option<oasis_win::system::SingleInstance>,
}

#[derive(Serialize)]
pub struct SystemStatus {
    /// Whether OASIS will start when you sign in (read from Windows).
    pub start_with_windows: bool,
    pub run_as_administrator: bool,
    pub elevated: bool,
    pub admin_text: String,
    /// Restarting would make the status line true.
    pub needs_restart: bool,
    /// Running against a copy of the data: the switches still change the real Windows entries.
    pub rehearsal: bool,
}

#[cfg(windows)]
pub fn begin() -> Begin {
    use oasis_core::startup::{decide_handover, Handover};
    use oasis_engine::defaults::detect_paths;
    use oasis_platform::windows::startup as win;
    use oasis_win::system::{self, SingleInstance};

    let args = LaunchArgs::parse(std::env::args().skip(1));
    let (mutex, show_event) = instance_names();
    let paths = detect_paths();
    let root = paths.as_ref().map(|p| p.app_data_root.clone());

    // Only hand over to a task that starts exactly this program. The C# app uses the same task
    // name, so its task would start the C# app; and a rehearsal must never touch the real task.
    let this_exe = std::env::current_exe().ok();
    let setting_on = !rehearsing()
        && this_exe.as_deref().is_some_and(win::task_runs)
        && paths
            .as_ref()
            .is_some_and(|p| oasis_settings::system::read_run_as_administrator(&p.profiles()));
    let handover = decide_handover(
        setting_on,
        system::is_elevated(),
        SingleInstance::is_held(&mutex),
        args,
    );
    if let (Handover::StartElevated { show_window }, Some(root)) = (handover, root.as_ref()) {
        if show_window {
            let _ = win::request_window_on_next_start(root, false);
        }
        if win::start_elevated_instance() {
            return Begin::Exit;
        }
        let _ = win::consume_window_request(root);
        if let Some(p) = &paths {
            oasis_platform::security_log::SecurityLog::at(p.security_log()).write(
                "Administrator mode is on but the elevated start failed; running at normal level.",
            );
        }
    }

    // A start that replaces a closing copy waits for it; any other start gives up at once.
    let wait = if args.replaces_running_instance() {
        Duration::from_secs(15)
    } else {
        Duration::ZERO
    };
    let started = Instant::now();
    let lock = loop {
        if let Some(lock) = SingleInstance::try_own(&mutex, &show_event) {
            break lock;
        }
        if started.elapsed() >= wait {
            let _ = SingleInstance::signal_show(&show_event);
            return Begin::Exit;
        }
        std::thread::sleep(Duration::from_millis(100));
    };

    let asked = root
        .as_ref()
        .and_then(|r| win::consume_window_request(r))
        .is_some();
    Begin::Run(Startup {
        show_window: !args.tray || asked,
        lock: Some(lock),
    })
}

#[cfg(not(windows))]
pub fn begin() -> Begin {
    Begin::Run(Startup { show_window: true })
}

/// The lock names: the C# app's, or private ones while rehearsing on a data copy.
#[cfg(windows)]
fn instance_names() -> (String, String) {
    use oasis_platform::windows::startup::{SHOW_WINDOW_EVENT, SINGLE_INSTANCE_MUTEX};
    if rehearsing() {
        (
            format!("{SINGLE_INSTANCE_MUTEX}.Rehearsal"),
            format!("{SHOW_WINDOW_EVENT}.Rehearsal"),
        )
    } else {
        (
            SINGLE_INSTANCE_MUTEX.to_string(),
            SHOW_WINDOW_EVENT.to_string(),
        )
    }
}

fn rehearsing() -> bool {
    std::env::var_os("OASIS_DATA_DIR").is_some_and(|v| !v.is_empty())
}

/// Call `on_show` whenever a second launch asks this copy to show its window.
#[cfg(windows)]
pub fn watch_show_requests(startup: &mut Startup, on_show: impl Fn() + Send + 'static) {
    if let Some(lock) = startup.lock.as_mut() {
        lock.watch_show_requests(on_show);
    }
}

#[cfg(not(windows))]
pub fn watch_show_requests(_: &mut Startup, _: impl Fn() + Send + 'static) {}

// ---- commands ---------------------------------------------------------------------------------

#[cfg(windows)]
fn status_now(app: &AppHandle) -> SystemStatus {
    use oasis_platform::windows::startup as win;
    use tauri::Manager;

    let slot = app.state::<crate::EngineSlot>();
    let setting = crate::with_engine(&slot, |e| {
        Ok(oasis_settings::system::run_as_administrator(&e.settings()))
    })
    .unwrap_or(false);
    let exe = std::env::current_exe().unwrap_or_default();
    let admin = win::admin_status(setting, &exe);
    SystemStatus {
        start_with_windows: win::autostart_enabled(),
        run_as_administrator: setting,
        elevated: oasis_win::system::is_elevated(),
        admin_text: admin.text().to_string(),
        needs_restart: matches!(
            admin,
            win::AdminStatus::OnNotElevated | win::AdminStatus::OffStillElevated
        ),
        rehearsal: rehearsing(),
    }
}

#[cfg(not(windows))]
fn status_now(_: &AppHandle) -> SystemStatus {
    SystemStatus {
        start_with_windows: false,
        run_as_administrator: false,
        elevated: false,
        admin_text: "Only available on Windows".into(),
        needs_restart: false,
        rehearsal: rehearsing(),
    }
}

/// The two switches and what Windows says about them.
#[tauri::command]
pub async fn system_status(app: AppHandle) -> Result<SystemStatus, String> {
    tauri::async_runtime::spawn_blocking(move || status_now(&app))
        .await
        .map_err(|e| e.to_string())
}

/// Turn "start with Windows" on or off: saves the setting and changes the login entry.
#[tauri::command]
pub async fn set_start_with_windows(app: AppHandle, on: bool) -> Result<SystemStatus, String> {
    tauri::async_runtime::spawn_blocking(move || set_autostart_blocking(&app, on))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(windows)]
fn set_autostart_blocking(app: &AppHandle, on: bool) -> Result<SystemStatus, String> {
    use tauri::Manager;
    let slot = app.state::<crate::EngineSlot>();
    crate::with_engine(&slot, |engine| {
        engine
            .change(|s| {
                s.set_start_with_windows(on);
                Ok(())
            })
            .map_err(|e| e.to_string())
    })?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if !oasis_platform::windows::startup::set_autostart(on, &exe) {
        return Err("The setting was saved, but Windows refused to change the login entry.".into());
    }
    Ok(status_now(app))
}

#[cfg(not(windows))]
fn set_autostart_blocking(_: &AppHandle, _: bool) -> Result<SystemStatus, String> {
    Err("Only available on Windows.".into())
}

/// Turn administrator mode on or off. Either way Windows shows one approval prompt, for the
/// scheduled task rather than for OASIS. Nothing is saved if the prompt is declined.
#[tauri::command]
pub async fn set_run_as_administrator(app: AppHandle, on: bool) -> Result<SystemStatus, String> {
    tauri::async_runtime::spawn_blocking(move || set_admin_blocking(&app, on))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(windows)]
fn set_admin_blocking(app: &AppHandle, on: bool) -> Result<SystemStatus, String> {
    use oasis_platform::windows::startup as win;
    use tauri::Manager;

    let slot = app.state::<crate::EngineSlot>();
    let (paths, start_at_sign_in) = crate::with_engine(&slot, |e| {
        Ok((
            e.paths().clone(),
            oasis_settings::system::start_with_windows(&e.settings()),
        ))
    })?;
    let done = if on {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        win::install_admin_task(&paths, &exe, start_at_sign_in)
    } else {
        win::remove_admin_task(&paths)
    };
    if !done {
        return Err(if on {
            "Administrator mode was not turned on: the approval was declined or Windows refused."
        } else {
            "Administrator mode was not turned off: the approval was declined or Windows refused."
        }
        .into());
    }
    crate::with_engine(&slot, |engine| {
        engine
            .change(|s| {
                s.set_run_as_administrator(on);
                Ok(())
            })
            .map_err(|e| e.to_string())
    })?;
    Ok(status_now(app))
}

#[cfg(not(windows))]
fn set_admin_blocking(_: &AppHandle, _: bool) -> Result<SystemStatus, String> {
    Err("Only available on Windows.".into())
}

/// Start the right copy (administrator or normal, per the setting) and close this one.
#[tauri::command]
pub async fn restart_oasis(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || restart_blocking(&app))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(windows)]
fn restart_blocking(app: &AppHandle) -> Result<(), String> {
    use oasis_platform::windows::startup as win;
    use tauri::Manager;

    let slot = app.state::<crate::EngineSlot>();
    let (paths, admin) = crate::with_engine(&slot, |e| {
        Ok((
            e.paths().clone(),
            oasis_settings::system::run_as_administrator(&e.settings()),
        ))
    })?;
    let this_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if admin && (rehearsing() || !win::task_runs(&this_exe)) {
        return Err("The administrator task does not start this copy of OASIS, so it cannot restart it. Quit and start OASIS yourself.".into());
    }
    if admin {
        win::request_window_on_next_start(&paths.app_data_root, true).map_err(|e| e.to_string())?;
        if !win::start_elevated_instance() {
            let _ = win::consume_window_request(&paths.app_data_root);
            return Err("Windows could not start the administrator copy.".into());
        }
    } else if oasis_win::system::is_elevated() {
        // Starting from here would give the new copy administrator rights again.
        return Err("Quit OASIS from the tray and start it again yourself; started from here it would stay elevated.".into());
    } else {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        oasis_win::shell::open(&exe.to_string_lossy(), Some("--restart"))
            .map_err(|code| format!("Windows could not start OASIS (code {code})."))?;
    }
    crate::quit(app);
    Ok(())
}

#[cfg(not(windows))]
fn restart_blocking(_: &AppHandle) -> Result<(), String> {
    Err("Only available on Windows.".into())
}

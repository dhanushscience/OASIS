//! `oasis address [PROCESS]`: read a running browser's address bar, the way the matcher does.

use std::process::ExitCode;

#[cfg(not(windows))]
pub fn run(_: &[String]) -> ExitCode {
    eprintln!("Reading the address bar is only built for Windows so far.");
    ExitCode::FAILURE
}

#[cfg(windows)]
pub fn run(args: &[String]) -> ExitCode {
    use oasis_core::matching::{BrowserProcesses, ForegroundContext};
    use oasis_platform::foreground::WindowSource;
    use oasis_platform::windows::foreground::Win32Windows;
    use oasis_settings::matcher::matching_profile;
    use oasis_settings::Settings;
    use oasis_win::{focus, window};

    let wanted: Vec<&str> = match args.first() {
        Some(name) => vec![name.as_str()],
        None => BrowserProcesses::NAMES.to_vec(),
    };
    let mut reader = Win32Windows::default();
    let mut found_any = false;

    for name in wanted {
        for hwnd in focus::main_windows_of(name) {
            found_any = true;
            let title = window::window_title(hwnd);
            // Chromium builds its accessibility tree on first use, so try a few times.
            let mut address = String::new();
            for _ in 0..4 {
                address = reader.browser_url(hwnd.0 as u64);
                if !address.is_empty() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(600));
            }
            println!("{name}  \"{title}\"");
            println!("   address: {}", if address.is_empty() { "(could not be read)" } else { &address });

            // Which profile this window would pick, with the settings on this machine.
            if let Some(paths) = oasis_platform::paths::OasisPaths::detect() {
                let dir = paths.profiles().parent().map(|p| p.to_path_buf()).unwrap_or_default();
                if let Ok(oasis_platform::profile_store::LoadOutcome::Loaded { loaded, .. }) =
                    oasis_platform::profile_store::ProfileStore::new(&dir).load()
                {
                    let settings: Settings = loaded.settings;
                    let pick = |address: &str| {
                        let context = ForegroundContext::new(name, &title, address);
                        matching_profile(&settings.profiles, &context)
                            .map_or("(none)".to_string(), |i| settings.profiles[i].name.clone())
                    };
                    println!("   profile with the address:   {}", pick(&address));
                    println!("   profile from the title only: {}", pick(""));
                }
            }
        }
    }
    if !found_any {
        eprintln!("No browser window found. Open one, or name the process: oasis address brave");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

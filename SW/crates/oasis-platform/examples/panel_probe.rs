//! Print which panel of the program in front has the focus, once a second, to check panel
//! detection on your own windows: `cargo run -p oasis-platform --example panel_probe -- [seconds]`.
//! Click into an IDE's terminal, editor or AI chat while it runs. It prints the program, the panel
//! and how sure it is; it never prints text from the window.

#[cfg(windows)]
fn main() {
    use oasis_platform::windows::foreground::foreground_app;

    let seconds: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(15);
    let mut tracker = foreground_app();
    tracker.set_panel_profiles_exist(true);
    println!("Reading for {seconds} s. Click around in an IDE.");
    for _ in 0..seconds {
        let started = std::time::Instant::now();
        let info = tracker.current_for_press();
        println!(
            "{:<18} panel={:<14} family={:<12} confidence={:<6} focus-class={:<28} took {} ms",
            info.process,
            if info.panel_id.is_empty() { "-" } else { &info.panel_id },
            if info.app_family.is_empty() { "-" } else { &info.app_family },
            if info.detection_confidence.is_empty() { "-" } else { &info.detection_confidence },
            info.focus_class_name.chars().take(28).collect::<String>(),
            started.elapsed().as_millis()
        );
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

#[cfg(not(windows))]
fn main() {
    println!("Panel detection is only built for Windows so far.");
}

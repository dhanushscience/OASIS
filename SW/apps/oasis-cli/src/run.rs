//! `oasis run`: the whole chain, headless. Pad → gesture rules → profile matcher → action.
//!
//! A thin wrapper over `oasis-engine`, the same engine the desktop app runs; this just prints its
//! events. It is a harness, not the finished app: no overlay, no settings window, no voice.

use std::process::ExitCode;

#[cfg(not(windows))]
pub fn run(_: &[String]) -> ExitCode {
    eprintln!("`oasis run` is only built for Windows so far.");
    ExitCode::FAILURE
}

#[cfg(windows)]
pub fn run(args: &[String]) -> ExitCode {
    use std::time::{Duration, Instant};

    use oasis_engine::{defaults, start, EngineEvent};

    let dry_run = args.iter().any(|a| a == "--dry-run");
    let seconds: Option<u64> = args.iter().find_map(|a| a.parse().ok());

    let Some(config) = defaults::windows::config(!dry_run) else {
        eprintln!("No home folder found on this machine.");
        return ExitCode::FAILURE;
    };
    let parts = match defaults::windows::parts(&config.paths) {
        Ok(parts) => parts,
        Err(e) => {
            eprintln!("Bluetooth: {e}");
            return ExitCode::FAILURE;
        }
    };

    let engine = match start(config, parts, |event| match event {
        EngineEvent::PadStatus { status } => println!("pad: {status}"),
        EngineEvent::PadConnected { name } => println!("pad: connected to {name}"),
        EngineEvent::PadLost => println!("pad: link lost"),
        EngineEvent::PadUnavailable { reason } => println!("pad: {reason}"),
        EngineEvent::Battery { percent, charging, .. } => {
            println!("pad: battery {percent}%{}", if charging { " (charging)" } else { "" });
        }
        EngineEvent::Press(p) => println!(
            "BTN {}  [{} / {}] {} {}{}",
            p.button,
            p.profile,
            p.config,
            if p.kind.is_empty() { "(blank)" } else { &p.kind },
            p.value,
            if p.runs { "" } else { "   (not run)" }
        ),
        EngineEvent::Unmapped { button, gesture } => {
            println!("BTN {button}  ({gesture}) nothing to run");
        }
        EngineEvent::ActionFailed { what, reason } => println!("  ✗ {what}: {reason}"),
        EngineEvent::Note { text } => println!("note: {text}"),
        EngineEvent::Reminder { title, body } => println!("{title}: {body}"),
        EngineEvent::Notice { title, body } => println!("{title}: {body}"),
        EngineEvent::MeetingReminder { title, when, message, .. } => println!("meeting: {message} ({title}, {when})"),
        EngineEvent::MeetingReminderEnd => {}
        // The first tap only shows what the buttons do (the desktop app shows them on screen).
        EngineEvent::ShowLabels { profile, labels, .. } => {
            println!("labels [{profile}]: {}  (tap again to run)", labels.join(" | "));
        }
        EngineEvent::BridgeStatus { text, clients } => println!("browser bridge: {text} ({clients} connected)"),
        EngineEvent::YouTube { token, title } => println!("youtube: {} {title}", if token.is_empty() { "idle" } else { &token }),
        EngineEvent::Voice { phase } => println!("voice: {phase}"),
        EngineEvent::VoiceDropped { reason } => println!("voice: nothing to type ({reason})"),
        EngineEvent::VoiceCommand { heard, label, outcome, detail } => {
            println!("command: \"{heard}\" → {label} [{outcome}] {detail}");
        }
        EngineEvent::Confirm { summary, .. } => println!("{summary}"),
        EngineEvent::ConfirmEnd { approved } => println!("{}", if approved { "approved" } else { "not run" }),
        EngineEvent::Transcript { text, typed } => {
            println!("heard: \"{text}\"{}", if typed { "  (typed)" } else { "  (not typed)" });
        }
        EngineEvent::PadButton { .. } | EngineEvent::HideLabels | EngineEvent::VoiceLevel { .. } => {}
        EngineEvent::SettingsChanged | EngineEvent::Stopped => {}
    }) {
        Ok(engine) => engine,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    let status = engine.status();
    println!(
        "{} profiles loaded. {}",
        status.profiles_loaded,
        if dry_run {
            "DRY RUN: OASIS interface presses are shown, nothing is executed."
        } else if status.live {
            "OASIS interface presses will now RUN the actions in your profiles."
        } else {
            "Actions are disabled."
        }
    );
    println!("Stop with Ctrl+C.");

    let started = Instant::now();
    while seconds.is_none_or(|s| started.elapsed() < Duration::from_secs(s)) {
        std::thread::sleep(Duration::from_millis(100));
    }
    ExitCode::SUCCESS
}

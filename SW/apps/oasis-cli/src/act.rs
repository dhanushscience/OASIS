//! `oasis do <Type> <Value>`: run one button action by hand, exactly as a pad press would.

use std::process::ExitCode;

#[cfg(windows)]
pub fn run(args: &[String]) -> ExitCode {
    use oasis_core::actions::{run_action, ActionOutcome, ButtonAction};
    use oasis_platform::paths::OasisPaths;
    use oasis_platform::profile_store::{LoadOutcome, ProfileStore};
    use oasis_platform::security_log::SecurityLog;
    use oasis_platform::windows::actions::WindowsActions;

    let Some(kind) = args.first() else {
        eprintln!("usage: oasis do <Type> [Value]   e.g.  oasis do Shortcut \"Win+D\"");
        eprintln!("types: Shortcut, OpenUrl, LaunchApp, MouseClick, PasteText, RunScript");
        return ExitCode::from(2);
    };
    let value = args[1..].join(" ");
    let Some(paths) = OasisPaths::detect() else {
        eprintln!("No home folder found on this machine.");
        return ExitCode::FAILURE;
    };

    // The same switch the settings window has: scripts only run if the user allowed them.
    let allow_scripts = match ProfileStore::new(paths.profiles().parent().unwrap_or(&paths.profiles())).load() {
        Ok(LoadOutcome::Loaded { loaded, .. } | LoadOutcome::RecoveredFromLastGood { loaded, .. }) => {
            loaded.settings.allow_run_script_actions
        }
        _ => false,
    };

    let mut backend = WindowsActions::new(
        SecurityLog::at(paths.security_log()),
        allow_scripts,
    );
    let action = ButtonAction { kind: kind.clone(), value, steps: vec![] };
    match run_action(&mut backend, &action, 0) {
        ActionOutcome::Ignored => println!("Nothing to do: the value is blank."),
        ActionOutcome::Single(true) => println!("Done."),
        ActionOutcome::Single(false) => {
            eprintln!("It did not work (or is not allowed). See the security log: {}", paths.security_log().display());
            return ExitCode::FAILURE;
        }
        ActionOutcome::Automation(result) => {
            println!("{}", result.failure_message.as_deref().unwrap_or("Done."));
        }
    }
    ExitCode::SUCCESS
}

#[cfg(not(windows))]
pub fn run(_: &[String]) -> ExitCode {
    eprintln!("Running actions is only built for Windows so far.");
    ExitCode::FAILURE
}

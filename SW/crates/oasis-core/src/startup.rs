//! How OASIS starts: the launch arguments, and the scheduled task that starts it as
//! administrator. Port of `Oasis.Core/Elevation/ElevatedStartTask.cs` and the argument rules of
//! `Services/Elevation/AdminMode.cs`. Pure: nothing here touches the system.
//!
//! Administrator mode is a Windows scheduled task, not the Run key: Windows will not silently
//! start an elevated program from the Run key. Creating the task needs one approval; after that
//! Windows itself starts OASIS elevated, at sign-in and on demand.

/// Task Scheduler path of the task (the same as the C# app, so both builds share it).
pub const ELEVATED_TASK_NAME: &str = r"ProjectOASIS\RunAsAdministrator";
/// Passed by the task, so a start that should have been elevated but is not can be told apart.
pub const ELEVATED_START_ARGUMENT: &str = "--elevated-start";
/// Start hidden in the tray.
pub const TRAY_ARGUMENT: &str = "--tray";
/// Passed by an OASIS that restarts itself; the new one waits for the old one to let go.
pub const RESTART_ARGUMENT: &str = "--restart";
/// The Run-key value name and the arguments it carries (login startup).
pub const AUTOSTART_VALUE_NAME: &str = "ProjectOASIS";

/// What the command line asked for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LaunchArgs {
    /// Start with the window hidden (login startup, the elevated task).
    pub tray: bool,
    /// Started by the administrator task.
    pub elevated_start: bool,
    /// Started by an OASIS that is replacing itself.
    pub restart: bool,
}

impl LaunchArgs {
    /// Arguments are matched case-insensitively and `/tray` counts as `--tray`, as in C#.
    pub fn parse<I, S>(args: I) -> LaunchArgs
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut parsed = LaunchArgs::default();
        for arg in args {
            let arg = arg.as_ref();
            if arg.eq_ignore_ascii_case(TRAY_ARGUMENT) || arg.eq_ignore_ascii_case("/tray") {
                parsed.tray = true;
            } else if arg.eq_ignore_ascii_case(ELEVATED_START_ARGUMENT) {
                parsed.elevated_start = true;
            } else if arg.eq_ignore_ascii_case(RESTART_ARGUMENT) {
                parsed.restart = true;
            }
        }
        parsed
    }

    /// A start that takes over from an OASIS still closing: it waits for the lock rather than
    /// giving up at once.
    pub fn replaces_running_instance(self) -> bool {
        self.restart || self.elevated_start
    }
}

/// What to do first thing at startup when administrator mode may apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handover {
    /// Carry on in this process.
    Stay,
    /// Ask the task to start the elevated OASIS, then exit. `show_window` is whether the user
    /// started it to see the window (the task always starts in the tray).
    StartElevated { show_window: bool },
}

/// The decision of `AdminMode.TryHandOverToElevated`, without the side effects.
///
/// Stay when the mode is off, this process is already elevated, the task started us and we are
/// still not elevated (never loop), or an OASIS is already running (the normal path asks that one
/// to show its window).
pub fn decide_handover(
    setting_on: bool,
    already_elevated: bool,
    instance_running: bool,
    args: LaunchArgs,
) -> Handover {
    if already_elevated || !setting_on || args.elevated_start || instance_running {
        return Handover::Stay;
    }
    Handover::StartElevated { show_window: !args.tray }
}

/// The task definition as Task Scheduler XML. Written out in full because the defaults of a
/// hand-made task are wrong for an always-on tray app ("stop after 3 days", "only on AC power").
pub fn elevated_task_xml(exe_path: &str, user_sid: &str, start_at_sign_in: bool) -> String {
    let exe = xml_escape(exe_path);
    let sid = xml_escape(user_sid);
    let folder = xml_escape(directory_of(exe_path));
    let trigger = if start_at_sign_in {
        format!(
            "  <LogonTrigger>\n      <Enabled>true</Enabled>\n      <UserId>{sid}</UserId>\n    </LogonTrigger>\n"
        )
    } else {
        String::new()
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Starts OASIS with administrator rights so it can control windows that run as administrator. Created by OASIS when RUN AS ADMINISTRATOR is turned on; turn it off in OASIS to remove this task.</Description>
  </RegistrationInfo>
  <Triggers>
  {trigger}</Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>{sid}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>HighestAvailable</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>5</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{exe}</Command>
      <Arguments>{TRAY_ARGUMENT} {ELEVATED_START_ARGUMENT}</Arguments>
      <WorkingDirectory>{folder}</WorkingDirectory>
    </Exec>
  </Actions>
</Task>
"#
    )
}

/// The Run-key command for login startup: the quoted program path and `--tray`.
pub fn autostart_command(exe_path: &str) -> String {
    format!("\"{exe_path}\" {TRAY_ARGUMENT}")
}

fn directory_of(path: &str) -> &str {
    match path.rfind(['\\', '/']) {
        Some(cut) if cut > 0 => &path[..cut],
        _ => "",
    }
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_are_matched_without_regard_to_case_and_slash_tray_counts() {
        let a = LaunchArgs::parse(["--TRAY", "--Elevated-Start"]);
        assert!(a.tray && a.elevated_start && !a.restart);
        assert!(LaunchArgs::parse(["/tray"]).tray);
        assert_eq!(LaunchArgs::parse(["--unknown", "x"]), LaunchArgs::default());
    }

    #[test]
    fn a_restart_or_task_start_waits_for_the_old_instance() {
        assert!(LaunchArgs::parse(["--restart"]).replaces_running_instance());
        assert!(LaunchArgs::parse(["--elevated-start"]).replaces_running_instance());
        assert!(!LaunchArgs::parse(["--tray"]).replaces_running_instance());
    }

    #[test]
    fn handover_only_when_the_mode_is_on_and_there_is_something_to_hand_to() {
        let none = LaunchArgs::default();
        let hand = |setting, elevated, running, args| decide_handover(setting, elevated, running, args);
        assert_eq!(hand(true, false, false, none), Handover::StartElevated { show_window: true });
        assert_eq!(
            hand(true, false, false, LaunchArgs::parse(["--tray"])),
            Handover::StartElevated { show_window: false },
            "a tray start does not ask for the window"
        );
        assert_eq!(hand(false, false, false, none), Handover::Stay, "mode off");
        assert_eq!(hand(true, true, false, none), Handover::Stay, "already elevated");
        assert_eq!(hand(true, false, true, none), Handover::Stay, "one is already running");
        assert_eq!(
            hand(true, false, false, LaunchArgs::parse(["--elevated-start"])),
            Handover::Stay,
            "the task started us and we are not elevated: never loop"
        );
    }

    #[test]
    fn the_task_xml_has_the_settings_an_always_on_tray_app_needs() {
        let xml = elevated_task_xml(r"C:\Apps\OASIS & Co\oasis.exe", "S-1-5-21-1-2-3-1001", true);
        assert!(xml.contains(r"<Command>C:\Apps\OASIS &amp; Co\oasis.exe</Command>"));
        assert!(xml.contains(r"<WorkingDirectory>C:\Apps\OASIS &amp; Co</WorkingDirectory>"));
        assert!(xml.contains("<Arguments>--tray --elevated-start</Arguments>"));
        assert!(xml.contains("<RunLevel>HighestAvailable</RunLevel>"));
        assert!(xml.contains("<LogonType>InteractiveToken</LogonType>"));
        assert!(xml.contains("<MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>"));
        assert!(xml.contains("<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>"));
        assert!(xml.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
        assert!(xml.contains("<LogonTrigger>") && xml.contains("<UserId>S-1-5-21-1-2-3-1001</UserId>"));
    }

    #[test]
    fn without_start_at_sign_in_there_is_no_logon_trigger() {
        let xml = elevated_task_xml(r"C:\o\oasis.exe", "S-1-5-21-1", false);
        assert!(!xml.contains("LogonTrigger"));
        assert!(xml.contains("<Triggers>"));
    }

    #[test]
    fn the_run_key_command_quotes_the_path() {
        assert_eq!(autostart_command(r"C:\Program Files\OASIS\oasis.exe"), r#""C:\Program Files\OASIS\oasis.exe" --tray"#);
    }
}

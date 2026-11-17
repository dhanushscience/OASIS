//! Port of `ExecuteActionAsync`, `ExecuteStepsInternalAsync`, `ExecuteStepAsync` and
//! `TryExecuteSingleAction`. See the module docs in `mod.rs`.

use std::time::Duration;

use super::{try_allow, ActionBackend, ActionStep, ButtonAction, MouseClick, Next, UrlScope};
use crate::shortcut::Chord;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepOutcome {
    pub index: usize,
    pub kind: String,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutomationResult {
    pub success: bool,
    pub steps: Vec<StepOutcome>,
    pub failure_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionOutcome {
    /// A blank button: nothing to do.
    Ignored,
    Single(bool),
    Automation(AutomationResult),
}

/// Run what a pad button is configured to do.
pub fn run_action(
    backend: &mut dyn ActionBackend,
    action: &ButtonAction,
    button_index: i32,
) -> ActionOutcome {
    if action.kind == "JoinMeeting" {
        return ActionOutcome::Single(run_single(backend, "JoinMeeting", ""));
    }
    if action.kind == "Automation" {
        return ActionOutcome::Automation(run_steps(backend, &action.steps, button_index));
    }
    if action.value.trim().is_empty() {
        return ActionOutcome::Ignored;
    }
    ActionOutcome::Single(run_single(backend, &action.kind, &action.value))
}

/// How long a step that opens something may take to be ready before it counts as failed.
pub const READY_LIMIT: Duration = Duration::from_secs(15);

/// Most steps one press may run: a "failed → try again" link that never succeeds stops here.
const MAX_RUNS: usize = 100;

/// Steps that open something: the next step waits until it is really there.
fn opens_something(kind: &str) -> bool {
    matches!(kind, "LaunchApp" | "Launch App" | "OpenUrl" | "QuickLaunch")
}

/// Run steps from the first, following each step's links: on success its `on_ok` (the step below
/// unless it says otherwise), on failure its `on_fail` (stop and report, unless it says otherwise).
/// A failure that has its own path is not a failure of the automation.
pub fn run_steps(
    backend: &mut dyn ActionBackend,
    steps: &[ActionStep],
    _button_index: i32, // kept for callers; no step needs it since Wait for Release went
) -> AutomationResult {
    let mut outcomes = Vec::new();
    let mut failure_message = None;
    let mut index = 0;
    while index < steps.len() {
        if outcomes.len() >= MAX_RUNS {
            failure_message = Some(format!("Stopped after {MAX_RUNS} steps: the steps go round in a loop."));
            break;
        }
        let step = &steps[index];
        let next = |link: Next, auto: Option<usize>| match link {
            Next::Auto => auto,
            Next::Stop => None,
            Next::Step(n) => Some(n),
        };
        let then = next(step.on_ok, Some(index + 1)).filter(|&n| n < steps.len());
        let before = (opens_something(&step.kind) && then.is_some()).then(|| backend.front_window());
        let mut result = run_step(backend, step);
        // Smart wait: only when another step follows, which must not act before this one is ready.
        if let (Ok(()), Some(before)) = (&result, before) {
            if !backend.wait_until_ready(before, READY_LIMIT) {
                result = Err(format!("It did not open within {} seconds.", READY_LIMIT.as_secs()));
            }
        }
        let success = result.is_ok();
        let error = result.err();
        if !success && step.on_fail == Next::Auto {
            failure_message = Some(format!("Step {} ({}) failed: {}", index + 1, step.kind, error.as_deref().unwrap_or("")));
        }
        outcomes.push(StepOutcome { index, kind: step.kind.clone(), success, error });
        match if success { then } else { next(step.on_fail, None) } {
            Some(n) => index = n,
            None => break,
        }
    }
    AutomationResult { success: failure_message.is_none(), steps: outcomes, failure_message }
}

fn run_step(backend: &mut dyn ActionBackend, step: &ActionStep) -> Result<(), String> {
    match step.kind.as_str() {
        "Delay" => {
            let millis = step
                .value
                .trim()
                .parse::<i64>()
                .ok()
                .filter(|&ms| ms >= 0)
                .ok_or("Invalid delay value.")?;
            backend.pause(Duration::from_millis(millis as u64));
            Ok(())
        }
        // "Wait for Release" was removed; a step saved before that is skipped, not a failure.
        "WaitRelease" | "Wait for Release" => Ok(()),
        kind => {
            if run_single(backend, kind, &step.value) {
                Ok(())
            } else {
                Err(backend.take_problem().unwrap_or_else(|| describe_failure(kind, &step.value)))
            }
        }
    }
}

fn describe_failure(kind: &str, value: &str) -> String {
    match kind {
        "Shortcut" => format!("Could not send shortcut '{value}'."),
        "PasteText" => "Could not paste text (clipboard or shortcut failed).".into(),
        "MouseClick" => format!("Could not click at '{value}'."),
        "OpenUrl" => format!("Could not open URL '{value}'."),
        "Launch App" | "LaunchApp" => format!("Could not launch '{value}'."),
        "CloseApp" => format!("'{value}' has no window open to close."),
        "WindowCommand" | "WindowSplit" => "Could not do that to the window.".into(),
        "WindowLayout" => "Could not save or restore that layout.".into(),
        "QuickLaunch" => "Could not open that Quick Launch setup.".into(),
        "PlaySound" => format!("There is no sound called '{value}'."),
        "AutoHotkey" => "Could not run the AutoHotkey script: scripts are off in Settings, or AutoHotkey v2 is not installed.".into(),
        _ => format!("Could not run action '{kind}'."),
    }
}

/// One action. `false` if it could not be done or is not allowed.
pub fn run_single(backend: &mut dyn ActionBackend, kind: &str, value: &str) -> bool {
    match kind {
        "Shortcut" => Chord::parse(value).is_ok_and(|chord| backend.send_shortcut(&chord)),
        "OpenUrl" => match try_allow(value, UrlScope::Automation) {
            Some(url) => backend.open_url(&url),
            None => {
                backend.security_log(&format!("Blocked OpenUrl scheme: {value}"));
                false
            }
        },
        "Launch App" | "LaunchApp" => launch_app(backend, value),
        "CloseApp" => !value.trim().is_empty() && backend.close_app(value.trim()),
        "WindowCommand" | "WindowSplit" | "WindowLayout" => !value.trim().is_empty() && backend.window_action(kind, value.trim()),
        "RunScript" => {
            if !backend.run_script_allowed() {
                backend.security_log("Blocked RunScript — not enabled in settings.");
                return false;
            }
            backend.run_script(value);
            true
        }
        // The same switch as RunScript: an AutoHotkey script can do anything a program can.
        "AutoHotkey" => {
            if !backend.run_script_allowed() {
                backend.security_log("Blocked AutoHotkey — scripts are not enabled in settings.");
                return false;
            }
            !value.trim().is_empty() && backend.run_autohotkey(value)
        }
        "MouseClick" => MouseClick::parse(value).is_some_and(|click| backend.mouse_click(click)),
        "PasteText" => !value.is_empty() && backend.paste_text(value),
        "OpenSettings" => super::settings_pages::is_known(value) && backend.open_url(value),
        "JoinMeeting" => backend.join_meeting(),
        "QuickLaunch" => !value.trim().is_empty() && backend.quick_launch(value.trim()),
        "LockPc" => backend.system("lock"),
        "Chime" => matches!(value, "1" | "4") && backend.system(&format!("chime:{value}")),
        "PlaySound" => super::sounds::find(value).is_some() && backend.system(&format!("sound:{}", value.trim())),
        "Radio" if is_radio(value) => backend.system(&format!("radio:{}", value.replace('|', ":"))),
        "Brightness" if is_brightness(value) => backend.system(&format!("brightness:{value}")),
        "Theme" => matches!(value, "dark" | "light") && backend.system(&format!("theme-{value}")),
        _ => false,
    }
}

/// `wifi|off`, `bluetooth|on`, `airplane|on`.
fn is_radio(value: &str) -> bool {
    matches!(value.split_once('|'), Some(("wifi" | "bluetooth" | "airplane", "on" | "off")))
}

/// `70`, `+10` or `-10`: a level or a change, at most 100.
fn is_brightness(value: &str) -> bool {
    let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
    (1..=3).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit()) && digits.parse::<u32>().is_ok_and(|n| n <= 100)
}

/// `"chrome|https://example.com"` opens that page in that browser; anything else is handed to
/// the backend as an app to focus or start.
fn launch_app(backend: &mut dyn ActionBackend, value: &str) -> bool {
    let query = value.trim();
    if let Some(pipe) = query.find('|').filter(|&p| p > 0) {
        let (browser, url) = (query[..pipe].trim(), query[pipe + 1..].trim());
        if !browser.is_empty() && !url.is_empty() {
            match try_allow(url, UrlScope::Automation) {
                Some(allowed) => {
                    if backend.launch_browser_url(browser, &allowed) {
                        return true;
                    }
                }
                None => {
                    backend.security_log(&format!("Blocked browser URL launch: {url}"));
                    return false;
                }
            }
        }
    }
    backend.launch_app(query)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::MouseButton;

    #[derive(Default)]
    struct Fake {
        calls: Vec<String>,
        log: Vec<String>,
        allow_scripts: bool,
        fail: Vec<&'static str>,
        /// The smart wait never sees the opened thing arrive.
        never_ready: bool,
        /// What the backend says went wrong (W6).
        problem: Option<String>,
    }

    impl Fake {
        fn note(&mut self, name: &'static str, detail: impl std::fmt::Display) -> bool {
            self.calls.push(format!("{name}:{detail}"));
            !self.fail.contains(&name)
        }
    }

    impl ActionBackend for Fake {
        fn send_shortcut(&mut self, chord: &Chord) -> bool {
            self.note("shortcut", chord.keys.len())
        }
        fn open_url(&mut self, url: &str) -> bool {
            self.note("url", url)
        }
        fn launch_browser_url(&mut self, browser: &str, url: &str) -> bool {
            self.note("browser", format!("{browser}|{url}"))
        }
        fn launch_app(&mut self, query: &str) -> bool {
            self.note("app", query)
        }
        fn take_problem(&mut self) -> Option<String> {
            self.problem.take()
        }
        fn run_script(&mut self, script: &str) {
            self.note("script", script);
        }
        fn wait_until_ready(&mut self, _: Option<isize>, _: Duration) -> bool {
            self.calls.push("wait".into());
            !self.never_ready
        }
        fn run_autohotkey(&mut self, script: &str) -> bool {
            self.note("ahk", script)
        }
        fn mouse_click(&mut self, c: MouseClick) -> bool {
            self.note("click", format!("{},{},{:?}", c.x, c.y, c.button))
        }
        fn paste_text(&mut self, text: &str) -> bool {
            self.note("paste", text)
        }
        fn join_meeting(&mut self) -> bool {
            self.note("meeting", "")
        }
        fn system(&mut self, command: &str) -> bool {
            self.note("system", command)
        }
        fn pause(&mut self, d: Duration) {
            self.calls.push(format!("pause:{}", d.as_millis()));
        }
        fn run_script_allowed(&self) -> bool {
            self.allow_scripts
        }
        fn security_log(&mut self, message: &str) {
            self.log.push(message.to_string());
        }
    }

    fn step(kind: &str, value: &str) -> ActionStep {
        ActionStep { kind: kind.into(), value: value.into(), ..Default::default() }
    }

    fn action(kind: &str, value: &str) -> ButtonAction {
        ButtonAction { kind: kind.into(), value: value.into(), steps: vec![] }
    }

    #[test]
    fn a_blank_button_does_nothing() {
        let mut b = Fake::default();
        assert_eq!(run_action(&mut b, &action("Shortcut", "  "), 0), ActionOutcome::Ignored);
        assert!(b.calls.is_empty());
    }

    #[test]
    fn a_shortcut_is_parsed_and_sent() {
        let mut b = Fake::default();
        assert_eq!(run_action(&mut b, &action("Shortcut", "Ctrl+Shift+Z"), 0), ActionOutcome::Single(true));
        assert_eq!(b.calls, ["shortcut:3"]);
        // A shortcut that does not parse is not sent.
        assert!(!run_single(&mut b, "Shortcut", "Ctrl+Nope"));
        assert_eq!(b.calls.len(), 1);
    }

    #[test]
    fn open_url_goes_through_the_allowlist_and_blocked_ones_are_logged() {
        let mut b = Fake::default();
        assert!(run_single(&mut b, "OpenUrl", "netflix.com"));
        assert_eq!(b.calls, ["url:https://netflix.com"]);
        assert!(!run_single(&mut b, "OpenUrl", "javascript:alert(1)"));
        assert_eq!(b.calls.len(), 1);
        assert_eq!(b.log, ["Blocked OpenUrl scheme: javascript:alert(1)"]);
    }

    #[test]
    fn launch_app_passes_the_trimmed_query() {
        let mut b = Fake::default();
        assert!(run_single(&mut b, "LaunchApp", "  C:\\Apps\\tool.exe  "));
        assert!(run_single(&mut b, "Launch App", "notepad"));
        assert_eq!(b.calls, ["app:C:\\Apps\\tool.exe", "app:notepad"]);
    }

    #[test]
    fn a_browser_and_url_pair_opens_that_page_in_that_browser() {
        let mut b = Fake::default();
        assert!(run_single(&mut b, "LaunchApp", "chrome | youtube.com"));
        assert_eq!(b.calls, ["browser:chrome|https://youtube.com"]);
    }

    #[test]
    fn an_unknown_browser_falls_back_to_launching_the_whole_text() {
        let mut b = Fake { fail: vec!["browser"], ..Fake::default() };
        assert!(run_single(&mut b, "LaunchApp", "weirdbrowser|x.com"));
        assert_eq!(b.calls, ["browser:weirdbrowser|https://x.com", "app:weirdbrowser|x.com"]);
    }

    #[test]
    fn a_blocked_browser_url_is_never_launched_at_all() {
        let mut b = Fake::default();
        assert!(!run_single(&mut b, "LaunchApp", "chrome|javascript:alert(1)"));
        assert!(b.calls.is_empty(), "C# fell through to launching the text; we stop");
        assert_eq!(b.log, ["Blocked browser URL launch: javascript:alert(1)"]);
    }

    #[test]
    fn a_leading_pipe_is_not_a_browser_pair() {
        let mut b = Fake::default();
        run_single(&mut b, "LaunchApp", "|odd");
        assert_eq!(b.calls, ["app:|odd"]);
    }

    #[test]
    fn run_script_needs_the_setting() {
        let mut b = Fake::default();
        assert!(!run_single(&mut b, "RunScript", "Get-Date"));
        assert!(b.calls.is_empty());
        assert_eq!(b.log, ["Blocked RunScript — not enabled in settings."]);

        let mut b = Fake { allow_scripts: true, ..Fake::default() };
        assert!(run_single(&mut b, "RunScript", "Get-Date"));
        assert_eq!(b.calls, ["script:Get-Date"]);
    }

    #[test]
    fn autohotkey_needs_the_same_setting_and_a_script() {
        let mut b = Fake::default();
        assert!(!run_single(&mut b, "AutoHotkey", "MsgBox 1"));
        assert!(b.calls.is_empty());
        assert_eq!(b.log, ["Blocked AutoHotkey — scripts are not enabled in settings."]);

        let mut b = Fake { allow_scripts: true, ..Fake::default() };
        assert!(!run_single(&mut b, "AutoHotkey", "   "), "a blank script is a mistake, not a run");
        assert!(run_single(&mut b, "AutoHotkey", "MsgBox 1"));
        assert_eq!(b.calls, ["ahk:MsgBox 1"]);
    }

    #[test]
    fn open_settings_opens_only_listed_pages() {
        let mut b = Fake::default();
        assert!(run_single(&mut b, "OpenSettings", "ms-settings:bluetooth"));
        assert!(!run_single(&mut b, "OpenSettings", "ms-settings:bluetooth /x"));
        assert!(!run_single(&mut b, "OpenSettings", "https://example.com"));
        assert_eq!(b.calls.len(), 1);
    }

    #[test]
    fn system_commands_send_only_fixed_words() {
        let mut b = Fake::default();
        assert!(run_single(&mut b, "LockPc", "x"));
        assert!(run_single(&mut b, "Chime", "4") && !run_single(&mut b, "Chime", "400"));
        assert!(run_single(&mut b, "Theme", "dark"));
        assert!(!run_single(&mut b, "Theme", "dark; calc"));
        assert!(run_single(&mut b, "Radio", "wifi|off") && !run_single(&mut b, "Radio", "wifi|off;calc") && !run_single(&mut b, "Radio", "gps|on"));
        assert!(run_single(&mut b, "Brightness", "-10") && run_single(&mut b, "Brightness", "70"));
        for bad in ["101", "+", "1;calc", "-1000", ""] {
            assert!(!run_single(&mut b, "Brightness", bad), "{bad}");
        }
        assert_eq!(b.calls, ["system:lock", "system:chime:4", "system:theme-dark", "system:radio:wifi:off", "system:brightness:-10", "system:brightness:70"]);
    }

    #[test]
    fn mouse_click_paste_and_unknown_types() {
        let mut b = Fake::default();
        assert!(run_single(&mut b, "MouseClick", "10,20,right"));
        assert!(!run_single(&mut b, "MouseClick", "oops"));
        assert!(run_single(&mut b, "PasteText", "hello"));
        assert!(!run_single(&mut b, "PasteText", ""));
        assert!(!run_single(&mut b, "Teleport", "x"));
        assert_eq!(b.calls, ["click:10,20,Right", "paste:hello"]);
        assert_eq!(MouseButton::Right, MouseButton::Right);
    }

    #[test]
    fn join_meeting_ignores_the_value() {
        let mut b = Fake::default();
        assert_eq!(run_action(&mut b, &action("JoinMeeting", ""), 0), ActionOutcome::Single(true));
        assert_eq!(b.calls, ["meeting:"]);
    }

    #[test]
    fn an_automation_runs_its_steps_in_order() {
        let mut b = Fake::default();
        let a = ButtonAction {
            kind: "Automation".into(),
            value: String::new(),
            steps: vec![step("Shortcut", "Win+D"), step("Delay", "250"), step("OpenUrl", "x.com")],
        };
        let ActionOutcome::Automation(r) = run_action(&mut b, &a, 2) else { panic!("expected automation") };
        assert!(r.success && r.failure_message.is_none());
        assert_eq!(r.steps.len(), 3);
        assert_eq!(b.calls, ["shortcut:2", "pause:250", "url:https://x.com"]);
    }

    #[test]
    fn an_automation_stops_at_the_first_failure_and_says_which_step() {
        let mut b = Fake { fail: vec!["url"], ..Fake::default() };
        let r = run_steps(
            &mut b,
            &[step("Shortcut", "A"), step("OpenUrl", "x.com"), step("Shortcut", "B")],
            0,
        );
        assert!(!r.success);
        assert_eq!(r.steps.len(), 2, "the third step never ran");
        assert_eq!(
            r.failure_message.as_deref(),
            Some("Step 2 (OpenUrl) failed: Could not open URL 'x.com'.")
        );
    }

    #[test]
    fn delays_must_be_non_negative_numbers() {
        let mut b = Fake::default();
        for bad in ["-5", "soon", ""] {
            let r = run_steps(&mut b, &[step("Delay", bad)], 0);
            assert_eq!(r.failure_message.as_deref(), Some("Step 1 (Delay) failed: Invalid delay value."));
        }
        assert!(run_steps(&mut b, &[step("Delay", "0")], 0).success);
        assert_eq!(b.calls, ["pause:0"]);
    }

    #[test]
    fn the_next_step_waits_until_what_was_opened_is_ready() {
        let mut b = Fake::default();
        assert!(run_steps(&mut b, &[step("LaunchApp", "notepad"), step("Shortcut", "A")], 0).success);
        assert_eq!(b.calls, ["app:notepad", "wait", "shortcut:1"]);
        // The last step has nothing after it to protect: no wait.
        let mut b = Fake::default();
        run_steps(&mut b, &[step("Shortcut", "A"), step("LaunchApp", "notepad")], 0);
        assert_eq!(b.calls, ["shortcut:1", "app:notepad"]);
        // Never ready: that step fails, and the next one does not run.
        let mut b = Fake { never_ready: true, ..Fake::default() };
        let r = run_steps(&mut b, &[step("OpenUrl", "a.com"), step("Shortcut", "A")], 0);
        assert!(r.failure_message.unwrap().contains("did not open within 15 seconds"));
        assert_eq!(b.calls, ["url:https://a.com", "wait"]);
    }

    #[test]
    fn a_failed_launch_says_why_when_the_backend_knows() {
        let mut b = Fake { fail: vec!["app"], problem: Some("Chrome is open, but Windows would not bring it forward.".into()), ..Fake::default() };
        let r = run_steps(&mut b, &[step("LaunchApp", "chrome")], 0);
        assert!(r.failure_message.as_deref().is_some_and(|m| m.ends_with("Chrome is open, but Windows would not bring it forward.")), "{:?}", r.failure_message);
        let mut plain = Fake { fail: vec!["app"], ..Fake::default() };
        let r = run_steps(&mut plain, &[step("LaunchApp", "chrome")], 0);
        assert!(r.failure_message.as_deref().is_some_and(|m| m.ends_with("Could not launch 'chrome'.")), "{:?}", r.failure_message);
    }

    #[test]
    fn a_failed_step_takes_its_failure_path_and_that_is_not_a_failure() {
        let mut b = Fake { fail: vec!["app"], ..Fake::default() };
        let steps = [
            ActionStep { on_fail: Next::Step(2), ..step("LaunchApp", "nope") },
            ActionStep { on_ok: Next::Stop, ..step("Shortcut", "A") },
            step("OpenUrl", "help.com"),
        ];
        let r = run_steps(&mut b, &steps, 0);
        assert!(r.success, "{:?}", r.failure_message);
        assert_eq!(b.calls, ["app:nope", "url:https://help.com"]);
        // It worked: the success link, which stops after the shortcut.
        let mut b = Fake::default();
        run_steps(&mut b, &steps, 0);
        assert_eq!(b.calls, ["app:nope", "wait", "shortcut:1"]);
    }

    #[test]
    fn a_loop_that_never_succeeds_is_stopped() {
        let mut b = Fake { fail: vec!["app"], ..Fake::default() };
        let r = run_steps(&mut b, &[ActionStep { on_fail: Next::Step(0), ..step("LaunchApp", "x") }], 0);
        assert!(r.failure_message.unwrap().contains("loop"));
        assert_eq!(b.calls.len(), MAX_RUNS);
    }

    #[test]
    fn a_sound_step_plays_only_a_known_sound() {
        let mut b = Fake::default();
        assert!(run_single(&mut b, "PlaySound", "chime") && run_single(&mut b, "PlaySound", " buzz "));
        assert!(!run_single(&mut b, "PlaySound", "../../x") && !run_single(&mut b, "PlaySound", ""));
        assert_eq!(b.calls, ["system:sound:chime", "system:sound:buzz"]);
    }

    #[test]
    fn a_quick_launch_step_opens_its_setup_through_the_backend() {
        let mut b = Fake::default();
        assert!(!run_single(&mut b, "QuickLaunch", " "));
        assert!(!run_single(&mut b, "QuickLaunch", "work"), "the fake has no setups: the trait's default says no");
    }

    #[test]
    fn an_old_wait_for_release_step_is_skipped() {
        let mut b = Fake::default();
        assert!(run_steps(&mut b, &[step("WaitRelease", ""), step("Wait for Release", "")], 1).success);
        assert!(b.calls.is_empty());
    }

    #[test]
    fn failure_messages_name_the_action() {
        let mut b = Fake { fail: vec!["shortcut", "click", "paste", "app"], ..Fake::default() };
        let msg = |b: &mut Fake, kind: &str, value: &str| {
            run_steps(b, &[step(kind, value)], 0).failure_message.unwrap()
        };
        assert!(msg(&mut b, "Shortcut", "A").ends_with("Could not send shortcut 'A'."));
        assert!(msg(&mut b, "MouseClick", "1,2").ends_with("Could not click at '1,2'."));
        assert!(msg(&mut b, "PasteText", "x").ends_with("Could not paste text (clipboard or shortcut failed)."));
        assert!(msg(&mut b, "LaunchApp", "z").ends_with("Could not launch 'z'."));
        assert!(msg(&mut b, "Mystery", "z").ends_with("Could not run action 'Mystery'."));
    }

    #[test]
    fn an_empty_automation_succeeds_quietly() {
        let mut b = Fake::default();
        let ActionOutcome::Automation(r) = run_action(&mut b, &action("Automation", ""), 0) else {
            panic!("expected automation")
        };
        assert!(r.success && r.steps.is_empty());
    }
}

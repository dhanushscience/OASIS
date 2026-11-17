//! Port of the non-OS half of `Services/ContextTracker.cs`: what changed since the last look,
//! and when the browser address must be read again (reading it goes through UI Automation and
//! can be slow, so it is throttled).
//!
//! The OS supplies a [`WindowSource`]; this tracker works the same on every OS and is tested
//! with a fake one. The panel half (which part of an IDE has focus) is here too: the OS source
//! reads the accessibility tree, `oasis_core::matching::panel` decides.

use std::time::{Duration, Instant};

use oasis_core::matching::panel::{
    classify, merge_workbench, should_probe_workbench, workbench_panel, PanelState, UiSnapshot, WorkbenchNode,
};
use oasis_core::matching::BrowserProcesses;

use crate::ports::{ForegroundApp, ForegroundInfo, PlatformResult};

/// The foreground window as the OS reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowSnapshot {
    /// Opaque, stable for the life of the window.
    pub id: u64,
    pub title: String,
    /// Process name without folder or extension; empty if it could not be read.
    pub process: String,
    pub pid: Option<u32>,
}

pub trait WindowSource: Send {
    /// `None` when no window has focus.
    fn foreground(&mut self) -> Option<WindowSnapshot>;
    /// The address-bar text of a browser window; empty if it cannot be read.
    fn browser_url(&mut self, window_id: u64) -> String;
    /// The focused control of a window and its ancestors, for panel detection. Empty when the OS
    /// cannot say (the default).
    fn ui_snapshot(&mut self, _window_id: u64) -> UiSnapshot {
        UiSnapshot::default()
    }
    /// A scan of the window's controls, the second opinion for IDE panels. Empty by default.
    fn workbench_nodes(&mut self, _window_id: u64) -> Vec<WorkbenchNode> {
        Vec::new()
    }
}

/// How long a browser address is trusted when nothing about the window changed.
const URL_MAX_AGE: Duration = Duration::from_secs(2);

pub struct ForegroundTracker<S: WindowSource> {
    source: S,
    url_profiles_exist: bool,
    panel_profiles_exist: bool,
    last: Option<WindowSnapshot>,
    url: String,
    url_read_at: Option<Instant>,
    info: ForegroundInfo,
    panel_state: PanelState,
}

impl<S: WindowSource> ForegroundTracker<S> {
    pub fn new(source: S) -> ForegroundTracker<S> {
        ForegroundTracker {
            source,
            url_profiles_exist: false,
            panel_profiles_exist: false,
            last: None,
            url: String::new(),
            url_read_at: None,
            info: ForegroundInfo::default(),
            panel_state: PanelState::default(),
        }
    }

    /// Whether any profile matches on a website. The address is read only when one does.
    pub fn set_url_profiles_exist(&mut self, exist: bool) {
        self.url_profiles_exist = exist;
    }

    /// Whether any profile has an extra set of buttons for a panel or a window title. The panel is
    /// read only when one does: reading it goes through UI Automation.
    pub fn set_panel_profiles_exist(&mut self, exist: bool) {
        self.panel_profiles_exist = exist;
    }

    /// As [`ForegroundApp::current`], but re-reads the address even if the window is unchanged.
    pub fn current_with_fresh_url(&mut self) -> ForegroundInfo {
        self.read(Instant::now(), true)
    }

    /// What a button press needs: the panel is read again, so a focus that moved inside one window
    /// (editor to terminal) is seen. The address is not forced: reading it through UI Automation
    /// can take a second, which made every press in a browser lag. A new tab or page changes the
    /// window title, which reads it again, and it is never older than `URL_MAX_AGE`.
    pub fn current_for_press(&mut self) -> ForegroundInfo {
        self.read_with(Instant::now(), false, true)
    }

    pub fn read(&mut self, now: Instant, force_fresh_url: bool) -> ForegroundInfo {
        self.read_with(now, force_fresh_url, false)
    }

    pub fn read_with(&mut self, now: Instant, force_fresh_url: bool, force_fresh_panel: bool) -> ForegroundInfo {
        let Some(window) = self.source.foreground() else {
            return ForegroundInfo::default();
        };

        let (hwnd_changed, title_changed, app_changed) = match &self.last {
            None => (true, true, true),
            Some(prev) => (
                prev.id != window.id,
                prev.title != window.title,
                !prev.process.eq_ignore_ascii_case(&window.process),
            ),
        };
        let changed = hwnd_changed || title_changed || app_changed;
        if !changed && !force_fresh_url && !force_fresh_panel {
            return self.info.clone();
        }
        self.last = Some(window.clone());

        let need_url = self.url_profiles_exist && BrowserProcesses::is_known(Some(&window.process));
        if !need_url {
            self.url.clear();
        } else {
            let stale = self.url_read_at.is_none_or(|at| now.duration_since(at) >= URL_MAX_AGE);
            if force_fresh_url || title_changed || hwnd_changed || stale {
                self.url = self.source.browser_url(window.id);
                self.url_read_at = Some(now);
            }
        }

        // A panel is read when something changed or a fresh read was asked for, and only if some
        // profile can use it. Otherwise the last reading stands.
        let mut info = ForegroundInfo {
            process: window.process.to_lowercase(),
            window_title: window.title.clone(),
            browser_url: (!self.url.is_empty()).then(|| self.url.clone()),
            pid: window.pid,
            ..self.info.clone()
        };
        if self.panel_profiles_exist && (changed || force_fresh_panel) {
            let ui = self.source.ui_snapshot(window.id);
            let mut reading = classify(&window.process, None, &window.title, &self.url, &ui);
            if should_probe_workbench(&reading, &ui) {
                let nodes = self.source.workbench_nodes(window.id);
                reading = merge_workbench(reading.clone(), workbench_panel(&reading.app_family, &nodes));
            }
            let stable = self.panel_state.stabilize(reading, force_fresh_panel || force_fresh_url, now);
            info.panel_id = stable.panel_id;
            info.panel_label = stable.panel_label;
            info.app_family = stable.app_family;
            info.detection_confidence = stable.confidence;
            info.focus_class_name = stable.focus_class_name;
            info.ancestor_hint = stable.ancestor_hint;
        } else if !self.panel_profiles_exist {
            info.panel_id.clear();
            info.panel_label.clear();
            info.app_family.clear();
            info.detection_confidence.clear();
            info.focus_class_name.clear();
            info.ancestor_hint.clear();
        }
        self.info = info;
        self.info.clone()
    }
}

/// What the profile matcher reads, including which panel has focus when it was read.
impl From<&ForegroundInfo> for oasis_core::matching::ForegroundContext {
    fn from(info: &ForegroundInfo) -> Self {
        let mut context = oasis_core::matching::ForegroundContext::new(
            &info.process,
            &info.window_title,
            info.browser_url.as_deref().unwrap_or(""),
        );
        context.panel_id = info.panel_id.clone();
        context.panel_label = info.panel_label.clone();
        context.app_family = info.app_family.clone();
        if !info.detection_confidence.is_empty() {
            context.detection_confidence = info.detection_confidence.clone();
        }
        context.focus_class_name = info.focus_class_name.clone();
        context.ancestor_hint = info.ancestor_hint.clone();
        context
    }
}

impl<S: WindowSource> ForegroundApp for ForegroundTracker<S> {
    fn current(&mut self) -> PlatformResult<ForegroundInfo> {
        Ok(self.read(Instant::now(), false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct State {
        window: Option<WindowSnapshot>,
        url: String,
        url_reads: u32,
    }

    #[derive(Clone, Default)]
    struct Fake(Arc<Mutex<State>>);

    impl Fake {
        fn focus(&self, id: u64, title: &str, process: &str) {
            self.0.lock().unwrap().window = Some(WindowSnapshot {
                id,
                title: title.into(),
                process: process.into(),
                pid: Some(100 + id as u32),
            });
        }
        fn set_url(&self, url: &str) {
            self.0.lock().unwrap().url = url.into();
        }
        fn url_reads(&self) -> u32 {
            self.0.lock().unwrap().url_reads
        }
    }

    impl WindowSource for Fake {
        fn foreground(&mut self) -> Option<WindowSnapshot> {
            self.0.lock().unwrap().window.clone()
        }
        fn browser_url(&mut self, _: u64) -> String {
            let mut s = self.0.lock().unwrap();
            s.url_reads += 1;
            s.url.clone()
        }
    }

    fn tracker(fake: &Fake, urls: bool) -> ForegroundTracker<Fake> {
        let mut t = ForegroundTracker::new(fake.clone());
        t.set_url_profiles_exist(urls);
        t
    }

    #[test]
    fn no_window_gives_an_empty_context() {
        let fake = Fake::default();
        assert_eq!(tracker(&fake, true).read(Instant::now(), false), ForegroundInfo::default());
    }

    #[test]
    fn reports_lower_case_process_title_and_pid() {
        let fake = Fake::default();
        fake.focus(1, "Doc - Word", "WINWORD");
        let info = tracker(&fake, false).read(Instant::now(), false);
        assert_eq!(info.process, "winword");
        assert_eq!(info.window_title, "Doc - Word");
        assert_eq!(info.pid, Some(101));
        assert_eq!(info.browser_url, None);
    }

    #[test]
    fn the_address_is_read_only_for_browsers_and_only_when_a_profile_needs_it() {
        let fake = Fake::default();
        fake.set_url("https://netflix.com/browse");
        let now = Instant::now();

        fake.focus(1, "Netflix", "chrome");
        assert_eq!(tracker(&fake, false).read(now, false).browser_url, None);
        assert_eq!(fake.url_reads(), 0);

        let mut t = tracker(&fake, true);
        assert_eq!(t.read(now, false).browser_url.as_deref(), Some("https://netflix.com/browse"));
        fake.focus(2, "Notes", "notepad");
        assert_eq!(t.read(now, false).browser_url, None);
    }

    #[test]
    fn an_unchanged_window_is_answered_from_the_cache() {
        let fake = Fake::default();
        fake.focus(1, "Tab", "chrome");
        fake.set_url("a.com");
        let mut t = tracker(&fake, true);
        let now = Instant::now();
        t.read(now, false);
        fake.set_url("b.com");
        assert_eq!(t.read(now + Duration::from_secs(10), false).browser_url.as_deref(), Some("a.com"));
        assert_eq!(fake.url_reads(), 1);
    }

    #[test]
    fn a_title_change_re_reads_the_address() {
        let fake = Fake::default();
        fake.focus(1, "Tab 1", "chrome");
        fake.set_url("a.com");
        let mut t = tracker(&fake, true);
        let now = Instant::now();
        t.read(now, false);
        fake.focus(1, "Tab 2", "chrome");
        fake.set_url("b.com");
        assert_eq!(t.read(now, false).browser_url.as_deref(), Some("b.com"));
        assert_eq!(fake.url_reads(), 2);
    }

    #[test]
    fn forcing_a_fresh_address_re_reads_even_when_nothing_changed() {
        let fake = Fake::default();
        fake.focus(1, "Tab", "chrome");
        fake.set_url("a.com");
        let mut t = tracker(&fake, true);
        let now = Instant::now();
        t.read(now, false);
        fake.set_url("b.com");
        assert_eq!(t.read(now, true).browser_url.as_deref(), Some("b.com"));
    }

    #[test]
    fn process_names_compare_ignoring_case() {
        let fake = Fake::default();
        fake.focus(1, "T", "Chrome");
        let mut t = tracker(&fake, true);
        t.read(Instant::now(), false);
        fake.focus(1, "T", "chrome");
        t.read(Instant::now(), false);
        assert_eq!(fake.url_reads(), 1, "same app, same window, same title: no new read");
    }

    #[test]
    fn leaving_a_browser_forgets_the_address() {
        let fake = Fake::default();
        fake.focus(1, "Tab", "firefox");
        fake.set_url("a.com");
        let mut t = tracker(&fake, true);
        t.read(Instant::now(), false);
        fake.focus(2, "Notes", "notepad");
        t.read(Instant::now(), false);
        fake.focus(1, "Tab", "firefox");
        fake.set_url("c.com");
        assert_eq!(t.read(Instant::now(), false).browser_url.as_deref(), Some("c.com"));
    }

    #[test]
    fn works_through_the_foreground_app_trait() {
        let fake = Fake::default();
        fake.focus(1, "T", "code");
        let mut app: Box<dyn ForegroundApp> = Box::new(tracker(&fake, false));
        assert_eq!(app.current().unwrap().process, "code");
    }
}

#[cfg(test)]
mod panel_tests {
    use super::*;
    use oasis_core::matching::panel::UiNode;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct State {
        window: Option<WindowSnapshot>,
        focus_class: String,
        focus_name: String,
        ancestors: Vec<UiNode>,
        snapshots: u32,
        scans: u32,
        scan_nodes: Vec<WorkbenchNode>,
    }

    #[derive(Clone, Default)]
    struct Source(Arc<Mutex<State>>);

    impl Source {
        fn window(&self, id: u64, title: &str, process: &str) {
            self.0.lock().unwrap().window =
                Some(WindowSnapshot { id, title: title.into(), process: process.into(), pid: Some(1) });
        }
        fn focus(&self, name: &str, class: &str, ancestors: &[UiNode]) {
            let mut s = self.0.lock().unwrap();
            s.focus_name = name.into();
            s.focus_class = class.into();
            s.ancestors = ancestors.to_vec();
        }
    }

    impl WindowSource for Source {
        fn foreground(&mut self) -> Option<WindowSnapshot> {
            self.0.lock().unwrap().window.clone()
        }
        fn browser_url(&mut self, _: u64) -> String {
            String::new()
        }
        fn ui_snapshot(&mut self, _: u64) -> UiSnapshot {
            let mut s = self.0.lock().unwrap();
            s.snapshots += 1;
            let focus = UiNode { depth: 0, name: s.focus_name.clone(), class_name: s.focus_class.clone(), ..UiNode::default() };
            let mut chain = vec![focus.clone()];
            chain.extend(s.ancestors.clone());
            UiSnapshot { focus, ancestors: chain }
        }
        fn workbench_nodes(&mut self, _: u64) -> Vec<WorkbenchNode> {
            let mut s = self.0.lock().unwrap();
            s.scans += 1;
            s.scan_nodes.clone()
        }
    }

    fn terminal_focus(source: &Source) {
        source.focus("Terminal 1", "xterm-helper-textarea", &[UiNode { depth: 1, class_name: "xterm-screen".into(), ..UiNode::default() }]);
    }

    #[test]
    fn with_no_panel_profiles_the_panel_is_never_read() {
        let source = Source::default();
        source.window(1, "main.rs - Visual Studio Code", "Code");
        terminal_focus(&source);
        let mut tracker = ForegroundTracker::new(source.clone());
        let info = tracker.current_for_press();
        assert_eq!(info.panel_id, "");
        assert_eq!(source.0.lock().unwrap().snapshots, 0, "no UI Automation cost for nobody");
    }

    #[test]
    fn the_terminal_of_an_ide_is_seen_when_a_profile_uses_panels() {
        let source = Source::default();
        source.window(1, "main.rs - Visual Studio Code", "Code");
        terminal_focus(&source);
        let mut tracker = ForegroundTracker::new(source);
        tracker.set_panel_profiles_exist(true);
        let info = tracker.current_for_press();
        assert_eq!((info.panel_id.as_str(), info.app_family.as_str()), ("terminal", "electron-ide"));
        assert!(!info.detection_confidence.is_empty());
        let context = oasis_core::matching::ForegroundContext::from(&info);
        assert_eq!((context.panel_id.as_str(), context.app_family.as_str()), ("terminal", "electron-ide"));
    }

    #[test]
    fn a_focus_that_moves_inside_one_window_is_seen_at_the_next_press() {
        let source = Source::default();
        source.window(1, "main.rs - Visual Studio Code", "Code");
        terminal_focus(&source);
        let mut tracker = ForegroundTracker::new(source.clone());
        tracker.set_panel_profiles_exist(true);
        assert_eq!(tracker.current_for_press().panel_id, "terminal");

        // The title did not change, so only a press-time read can notice the move.
        source.focus("", "monaco-editor view-lines", &[UiNode { depth: 1, name: "editor".into(), ..UiNode::default() }]);
        assert_eq!(tracker.current().unwrap().panel_id, "terminal", "an ordinary look uses what it knew");
        assert_eq!(tracker.current_for_press().panel_id, "editor");
    }

    #[test]
    fn switching_to_another_program_clears_the_panel() {
        let source = Source::default();
        source.window(1, "x - Visual Studio Code", "Code");
        terminal_focus(&source);
        let mut tracker = ForegroundTracker::new(source.clone());
        tracker.set_panel_profiles_exist(true);
        assert_eq!(tracker.current_for_press().panel_id, "terminal");
        source.window(2, "Spotify", "spotify");
        source.focus("Search", "", &[]);
        let info = tracker.current_for_press();
        assert_eq!((info.panel_id.as_str(), info.app_family.as_str()), ("unknown", "generic"));
    }

    #[test]
    fn a_weak_reading_in_an_ide_asks_the_window_scan_for_a_second_opinion() {
        let source = Source::default();
        source.window(1, "x - Cursor", "Cursor");
        source.focus("", "Chrome_RenderWidgetHostHWND", &[]);
        source.0.lock().unwrap().scan_nodes = vec![WorkbenchNode {
            depth: 4,
            name: "Integrated Terminal".into(),
            class_name: String::new(),
            automation_id: String::new(),
            has_focus_within: true,
        }];
        let mut tracker = ForegroundTracker::new(source.clone());
        tracker.set_panel_profiles_exist(true);
        let info = tracker.current_for_press();
        assert_eq!(info.panel_id, "terminal", "{info:?}");
        assert_eq!(source.0.lock().unwrap().scans, 1);
    }

    #[test]
    fn a_strong_reading_does_not_scan_the_whole_window() {
        let source = Source::default();
        source.window(1, "x - Code", "Code");
        terminal_focus(&source);
        let mut tracker = ForegroundTracker::new(source.clone());
        tracker.set_panel_profiles_exist(true);
        tracker.current_for_press();
        assert_eq!(source.0.lock().unwrap().scans, 0);
    }

    #[test]
    fn switching_panel_profiles_off_forgets_the_reading() {
        let source = Source::default();
        source.window(1, "x - Code", "Code");
        terminal_focus(&source);
        let mut tracker = ForegroundTracker::new(source);
        tracker.set_panel_profiles_exist(true);
        assert_eq!(tracker.current_for_press().panel_id, "terminal");
        tracker.set_panel_profiles_exist(false);
        assert_eq!(tracker.current_for_press().panel_id, "");
    }
}

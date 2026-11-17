//! Opens a Quick Launch setup's items and puts each window where it was asked. Port of
//! `Oasis.Application/QuickLaunch/QuickLaunchRunner.cs`; the flow was proved on real apps in the
//! C# app:
//!
//! 1. remember the windows that exist now;
//! 2. start every app at once, but two items of the same app one after the other, so "the new
//!    window of this app" is never ambiguous;
//! 3. wait for each item's new window, place it, and check once more a second later, because some
//!    apps restore their own position just after showing;
//! 4. report every item: one that did not open or did not land is named, never hidden.
//!
//! Nothing is ever closed, and a window already in place is left alone.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use oasis_core::placement::{self, Desktop, DesktopWindow, Fit, Layout, PixelRect, Screen, Target, WindowPlacement};
use oasis_settings::quick_launch::{Item, ItemKind, Setup};

/// Starts apps, links and folders. Never elevates, never runs a script.
pub trait Launcher: Send + Sync {
    fn resolve(&self, item: &Item) -> Target;
    /// `new_window`: ask for a window of its own (a browser's new-window switch). Needed whenever
    /// the window is going to be placed: a link opened as a tab would mean moving the whole browser.
    fn start(&self, item: &Item, new_window: bool) -> Result<(), String>;
}

/// The real or pretend OS parts of Quick Launch.
#[derive(Clone)]
pub struct QuickLaunchParts {
    pub desktop: Arc<dyn Desktop>,
    pub launcher: Arc<dyn Launcher>,
}

/// How long to wait for things. Tests use tiny ones.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    pub window_timeout: Duration,
    pub poll: Duration,
    pub settle: Duration,
    pub recheck: Duration,
    pub after_replace: Duration,
    /// An app that is already open often answers a launch by bringing its old window forward. Only
    /// after this long is that window accepted.
    pub focus_fallback_after: Duration,
}

impl Timing {
    pub const REAL: Timing = Timing {
        window_timeout: Duration::from_secs(25),
        poll: Duration::from_millis(100),
        settle: Duration::from_millis(300),
        recheck: Duration::from_secs(1),
        after_replace: Duration::from_millis(700),
        focus_fallback_after: Duration::from_secs(3),
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Opened,
    Placed,
    PlacedAtMinimumSize,
    OpenedNotPlaced,
    FailedToOpen,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemResult {
    pub name: String,
    pub outcome: Outcome,
    pub detail: String,
}

impl ItemResult {
    pub fn is_problem(&self) -> bool {
        matches!(self.outcome, Outcome::OpenedNotPlaced | Outcome::FailedToOpen)
    }
}

/// One line for the screen and the log: how many are ready, then each problem by name.
pub fn summary(setup_name: &str, results: &[ItemResult]) -> String {
    if results.is_empty() {
        return format!("{setup_name}: nothing to open.");
    }
    let ready = results.iter().filter(|r| !r.is_problem()).count();
    let head = format!("{setup_name}: {ready} of {} ready.", results.len());
    let problems: Vec<String> = results
        .iter()
        .filter(|r| r.is_problem())
        .map(|r| {
            let what = if r.outcome == Outcome::FailedToOpen { "did not open" } else { "opened but was not placed" };
            if r.detail.is_empty() { format!("{} {what}", r.name) } else { format!("{} {what} ({})", r.name, r.detail) }
        })
        .collect();
    if problems.is_empty() { head } else { format!("{head} {}.", problems.join("; ")) }
}

/// A short name for an item: the last path segment, or the host of a link.
pub fn display_name(item: &Item) -> String {
    let target = item.target.trim().trim_end_matches(['\\', '/']);
    if item.kind == ItemKind::Url {
        let rest = target.split_once("://").map_or(target, |(_, r)| r);
        return rest.split(['/', '?', '#']).next().unwrap_or(rest).to_string();
    }
    let name = placement::stem(target);
    if name.is_empty() { target.to_string() } else { name }
}

pub struct Runner {
    desktop: Arc<dyn Desktop>,
    launcher: Arc<dyn Launcher>,
    timing: Timing,
}

struct Shared {
    before: Vec<DesktopWindow>,
    before_handles: Vec<i64>,
    foreground_at_start: i64,
    claimed: Mutex<Vec<i64>>,
}

impl Runner {
    pub fn new(parts: &QuickLaunchParts, timing: Timing) -> Runner {
        Runner { desktop: Arc::clone(&parts.desktop), launcher: Arc::clone(&parts.launcher), timing }
    }

    pub fn run(&self, setup: &Setup) -> Vec<ItemResult> {
        let _busy = crate::activity::work();
        let before = self.desktop.windows();
        let shared = Shared {
            before_handles: before.iter().map(|w| w.handle).collect(),
            before,
            foreground_at_start: self.desktop.foreground(),
            claimed: Mutex::new(Vec::new()),
        };
        let mut groups: BTreeMap<String, Vec<(usize, &Item, Target)>> = BTreeMap::new();
        for (index, item) in setup.items.iter().enumerate() {
            let target = self.launcher.resolve(item);
            groups.entry(target.app_key.to_lowercase()).or_default().push((index, item, target));
        }
        let results: Mutex<Vec<Option<ItemResult>>> = Mutex::new(vec![None; setup.items.len()]);
        thread::scope(|scope| {
            for group in groups.into_values() {
                let (shared, results) = (&shared, &results);
                scope.spawn(move || {
                    for (index, item, target) in group {
                        let result = self.run_item(item, &target, shared);
                        results.lock().unwrap_or_else(|e| e.into_inner())[index] = Some(result);
                    }
                });
            }
        });
        results.into_inner().unwrap_or_else(|e| e.into_inner()).into_iter().flatten().collect()
    }

    fn run_item(&self, item: &Item, target: &Target, shared: &Shared) -> ItemResult {
        let name = display_name(item);
        let result = |outcome, detail: &str| ItemResult { name: name.clone(), outcome, detail: detail.to_string() };

        if !item.wants_placement() {
            return match self.launcher.start(item, false) {
                Ok(()) => result(Outcome::Opened, ""),
                Err(why) => result(Outcome::FailedToOpen, &why),
            };
        }

        // A plain app that is already open: use the window it has. Launching it again would only
        // bring that window forward, and there would be no new window to wait for.
        let mut handle = 0;
        if item.kind == ItemKind::App && !item.open_another && item.open_with.is_empty() {
            let free: Vec<DesktopWindow> = {
                let claimed = shared.claimed.lock().unwrap_or_else(|e| e.into_inner());
                shared.before.iter().filter(|w| !claimed.contains(&w.handle)).cloned().collect()
            };
            handle = self.claim(shared, placement::pick_window(&free, target));
        }

        if handle == 0 {
            if let Err(why) = self.launcher.start(item, true) {
                return result(Outcome::FailedToOpen, &why);
            }
            handle = self.wait_for_window(target, shared);
            if handle == 0 {
                let secs = self.timing.window_timeout.as_secs_f64();
                return result(Outcome::OpenedNotPlaced, &format!("its window did not appear within {secs:.0} s"));
            }
            thread::sleep(self.timing.settle);
        }

        // A window the user minimized is still "open", so it is reused, but it must come back up:
        // a window that is already on the right screen was otherwise left minimized and never seen.
        if self.desktop.read(handle).is_some_and(|p| p.minimized) {
            self.desktop.restore(handle);
            thread::sleep(self.timing.settle);
        }

        let (mut fit, mut problem) = self.check(handle, item);
        if fit == Fit::Off {
            self.place(handle, item);
            thread::sleep(self.timing.recheck);
            (fit, problem) = self.check(handle, item);
            if fit == Fit::Off {
                self.place(handle, item);
                thread::sleep(self.timing.after_replace);
                (fit, problem) = self.check(handle, item);
            }
        }
        match fit {
            Fit::Exact => result(Outcome::Placed, ""),
            Fit::MinimumSize => result(Outcome::PlacedAtMinimumSize, "the app will not go smaller"),
            Fit::Off => result(Outcome::OpenedNotPlaced, &problem),
        }
    }

    /// Takes a window for one item; two items can never end up with the same window.
    fn claim(&self, shared: &Shared, handle: i64) -> i64 {
        let mut claimed = shared.claimed.lock().unwrap_or_else(|e| e.into_inner());
        if handle != 0 && !claimed.contains(&handle) {
            claimed.push(handle);
            handle
        } else {
            0
        }
    }

    fn wait_for_window(&self, target: &Target, shared: &Shared) -> i64 {
        let t = &self.timing;
        let polls = (t.window_timeout.as_millis() / t.poll.as_millis().max(1)) as u64;
        let fallback_from = (t.focus_fallback_after.as_millis() / t.poll.as_millis().max(1)) as u64;
        for i in 0..polls {
            let taken = shared.claimed.lock().unwrap_or_else(|e| e.into_inner()).clone();
            let found = placement::pick_fresh(&self.desktop.windows(), &shared.before_handles, &taken, target);
            let handle = self.claim(shared, found);
            if handle != 0 {
                return handle;
            }
            // No new window, but one of the app's old windows has come to the front since the
            // launch: the app answered by focusing it. The window the user was already in at the
            // start is never taken this way; that would move what they are working on.
            if i >= fallback_from {
                let front = self.desktop.foreground();
                if front != 0
                    && front != shared.foreground_at_start
                    && shared.before.iter().any(|w| w.handle == front && placement::process_matches(&w.process, target))
                {
                    let handle = self.claim(shared, front);
                    if handle != 0 {
                        return handle;
                    }
                }
            }
            thread::sleep(t.poll);
        }
        0
    }

    fn target_screen(&self, item: &Item, now: &WindowPlacement, screens: &[Screen]) -> Option<usize> {
        if item.screen.is_empty() { now.screen } else { placement::pick_screen(screens, &item.screen) }
    }

    fn place(&self, handle: i64, item: &Item) {
        let screens = self.desktop.screens();
        let Some(mut now) = self.desktop.read(handle) else { return };
        let Some(index) = self.target_screen(item, &now, &screens).filter(|i| *i < screens.len()) else { return };
        let work = screens[index].work;
        match item.effective_layout() {
            Layout::Maximized => {
                // Put a restored window on the screen first; maximizing happens where it sits.
                self.desktop.restore(handle);
                self.desktop.set_outer(handle, placement::inset(work));
                self.desktop.maximize(handle);
            }
            Layout::LeftHalf | Layout::RightHalf => {
                if now.maximized || now.minimized {
                    self.desktop.restore(handle);
                    let Some(restored) = self.desktop.read(handle) else { return };
                    now = restored;
                }
                let Some(target) = item.target_rect(work) else { return };
                self.desktop.set_outer(handle, placement::outer_for(target, now.outer, now.visible));
                if let Some(got) = self.desktop.read(handle) {
                    if let Some(kept) = placement::keep_on_screen(got.visible, target, work, item.effective_layout()) {
                        self.desktop.set_outer(handle, placement::outer_for(kept, got.outer, got.visible));
                    }
                }
            }
            Layout::AsIs => {
                // Screen only: keep the window's size and its offset from the corner.
                let Some(from) = now.screen.filter(|s| *s != index && *s < screens.len()) else { return };
                let was_maximized = now.maximized;
                if now.maximized || now.minimized {
                    self.desktop.restore(handle);
                    let Some(restored) = self.desktop.read(handle) else { return };
                    now = restored;
                }
                self.desktop.set_outer(handle, placement::move_between(now.outer, screens[from].work, work));
                if was_maximized {
                    self.desktop.maximize(handle);
                }
            }
        }
    }

    fn check(&self, handle: i64, item: &Item) -> (Fit, String) {
        let screens = self.desktop.screens();
        let Some(now) = self.desktop.read(handle) else { return (Fit::Off, "its window closed".into()) };
        if now.minimized {
            return (Fit::Off, "it stayed minimized".into());
        }
        let Some(index) = self.target_screen(item, &now, &screens).filter(|i| *i < screens.len()) else {
            let why = if screens.len() < 2 { "only one screen is connected" } else { "there is no such screen" };
            return (Fit::Off, why.into());
        };
        if now.screen != Some(index) {
            return (Fit::Off, "it is on the wrong screen".into());
        }
        match item.effective_layout() {
            Layout::AsIs => (Fit::Exact, String::new()),
            Layout::Maximized if now.maximized => (Fit::Exact, String::new()),
            Layout::Maximized => (Fit::Off, "it would not maximize".into()),
            half => {
                if now.maximized || now.minimized {
                    return (Fit::Off, "it is not in its half".into());
                }
                let work: PixelRect = screens[index].work;
                let Some(target) = item.target_rect(work) else { return (Fit::Exact, String::new()) };
                let fit = placement::check(now.visible, target, work, half);
                (fit, if fit == Fit::Off { "it moved itself".into() } else { String::new() })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI64, Ordering};

    /// A tiny pretend desktop: windows that appear when their app is started.
    struct Fake {
        windows: Mutex<Vec<(DesktopWindow, WindowPlacement)>>,
        screens: Vec<Screen>,
        next: AtomicI64,
        started: Mutex<Vec<String>>,
        /// Windows are made this wide by the app, whatever it is told (an app with a minimum size).
        min_width: Mutex<Option<i32>>,
    }

    fn screen(left: i32, primary: bool) -> Screen {
        let r = PixelRect::new(left, 0, left + 1920, 1080);
        Screen { bounds: r, work: r, primary }
    }

    impl Fake {
        fn new(screens: Vec<Screen>) -> Arc<Fake> {
            Arc::new(Fake { windows: Mutex::new(Vec::new()), screens, next: AtomicI64::new(100), started: Mutex::new(Vec::new()), min_width: Mutex::new(None) })
        }

        fn open(&self, process: &str, title: &str) -> i64 {
            let handle = self.next.fetch_add(1, Ordering::SeqCst);
            let r = PixelRect::new(300, 200, 1100, 800);
            let placed = WindowPlacement { outer: r, visible: r, maximized: false, minimized: false, screen: Some(self.screen_of(r)) };
            self.windows.lock().unwrap().push((DesktopWindow { handle, process: process.into(), title: title.into() }, placed));
            handle
        }

        fn screen_of(&self, r: PixelRect) -> usize {
            let mid = (r.left + r.right) / 2;
            self.screens.iter().position(|s| mid >= s.bounds.left && mid < s.bounds.right).unwrap_or(0)
        }

        fn placement(&self, handle: i64) -> WindowPlacement {
            self.windows.lock().unwrap().iter().find(|(w, _)| w.handle == handle).unwrap().1
        }
    }

    impl Desktop for Fake {
        fn windows(&self) -> Vec<DesktopWindow> {
            self.windows.lock().unwrap().iter().map(|(w, _)| w.clone()).collect()
        }
        fn foreground(&self) -> i64 {
            0
        }
        fn screens(&self) -> Vec<Screen> {
            self.screens.clone()
        }
        fn read(&self, handle: i64) -> Option<WindowPlacement> {
            self.windows.lock().unwrap().iter().find(|(w, _)| w.handle == handle).map(|(_, p)| *p)
        }
        fn restore(&self, handle: i64) {
            if let Some((_, p)) = self.windows.lock().unwrap().iter_mut().find(|(w, _)| w.handle == handle) {
                p.maximized = false;
                p.minimized = false;
            }
        }
        fn maximize(&self, handle: i64) {
            let work = self.screens[0].work;
            if let Some((_, p)) = self.windows.lock().unwrap().iter_mut().find(|(w, _)| w.handle == handle) {
                p.maximized = true;
                p.visible = work;
            }
        }
        fn set_outer(&self, handle: i64, outer: PixelRect) {
            let min = *self.min_width.lock().unwrap();
            let screens = self.screens.clone();
            if let Some((_, p)) = self.windows.lock().unwrap().iter_mut().find(|(w, _)| w.handle == handle) {
                let mut outer = outer;
                if let Some(min) = min.filter(|m| outer.width() < *m) {
                    outer.right = outer.left + min;
                }
                p.outer = outer;
                p.visible = outer;
                let mid = (outer.left + outer.right) / 2;
                p.screen = screens.iter().position(|s| mid >= s.bounds.left && mid < s.bounds.right);
            }
        }
    }

    struct FakeLauncher(Arc<Fake>);

    impl Launcher for FakeLauncher {
        fn resolve(&self, item: &Item) -> Target {
            let name = placement::stem(&item.target);
            Target { app_key: name.to_lowercase(), process_names: vec![name.clone()], title_hint: String::new() }
        }
        fn start(&self, item: &Item, _new_window: bool) -> Result<(), String> {
            if item.target.contains("missing") {
                return Err("it was not found on this PC".into());
            }
            self.0.started.lock().unwrap().push(item.target.clone());
            if !item.target.contains("silent") {
                self.0.open(&placement::stem(&item.target), &format!("{} window", item.target));
            }
            Ok(())
        }
    }

    fn tiny() -> Timing {
        let ms = Duration::from_millis;
        Timing { window_timeout: ms(300), poll: ms(5), settle: ms(1), recheck: ms(1), after_replace: ms(1), focus_fallback_after: ms(100) }
    }

    fn item(target: &str, screen: &str, layout: Layout) -> Item {
        Item { kind: ItemKind::App, target: target.into(), open_with: String::new(), screen: screen.into(), layout, region: None, open_another: false }
    }

    fn setup(items: Vec<Item>) -> Setup {
        Setup { id: "s".into(), name: "Desk".into(), start_with_oasis: false, voice_phrases: vec![], items }
    }

    fn run(fake: &Arc<Fake>, items: Vec<Item>) -> Vec<ItemResult> {
        let parts = QuickLaunchParts { desktop: fake.clone(), launcher: Arc::new(FakeLauncher(fake.clone())) };
        Runner::new(&parts, tiny()).run(&setup(items))
    }

    #[test]
    fn two_halves_land_side_by_side() {
        let fake = Fake::new(vec![screen(0, true)]);
        let results = run(&fake, vec![item("code", "", Layout::LeftHalf), item("chrome", "", Layout::RightHalf)]);
        assert!(results.iter().all(|r| r.outcome == Outcome::Placed), "{results:?}");
        let of = |process: &str| fake.placement(fake.windows().iter().find(|w| w.process == process).unwrap().handle).visible;
        let (left, right) = (of("code"), of("chrome"));
        assert_eq!((left.left, left.right, right.left, right.right), (0, 960, 960, 1920));
    }

    #[test]
    fn a_minimized_window_already_on_its_screen_is_brought_back_up() {
        let fake = Fake::new(vec![screen(0, true)]);
        let existing = fake.open("code", "old");
        fake.windows.lock().unwrap()[0].1.minimized = true;
        let results = run(&fake, vec![item("code", "main", Layout::AsIs)]);
        assert_eq!(results[0].outcome, Outcome::Placed, "{results:?}");
        assert!(!fake.placement(existing).minimized, "restored");
        assert!(fake.started.lock().unwrap().is_empty(), "the open window was used, not a new one");
    }

    #[test]
    fn a_grid_box_lands_where_it_was_drawn() {
        let fake = Fake::new(vec![screen(0, true)]);
        let mut quarter = item("code", "", Layout::AsIs);
        quarter.region = oasis_core::placement::Region::new(500, 500, 500, 500);
        let results = run(&fake, vec![quarter]);
        assert_eq!(results[0].outcome, Outcome::Placed, "{results:?}");
        let got = fake.placement(fake.windows()[0].handle).visible;
        assert_eq!((got.left, got.top, got.right, got.bottom), (960, 540, 1920, 1080));
    }

    #[test]
    fn an_item_with_no_placement_is_only_opened() {
        let fake = Fake::new(vec![screen(0, true)]);
        let results = run(&fake, vec![item("notepad", "", Layout::AsIs)]);
        assert_eq!(results[0].outcome, Outcome::Opened);
        assert_eq!(fake.placement(fake.windows()[0].handle).outer, PixelRect::new(300, 200, 1100, 800), "left where it opened");
    }

    #[test]
    fn a_window_already_open_is_moved_instead_of_opening_another() {
        let fake = Fake::new(vec![screen(0, true)]);
        let existing = fake.open("code", "old");
        let results = run(&fake, vec![item("code", "", Layout::LeftHalf)]);
        assert_eq!(results[0].outcome, Outcome::Placed);
        assert!(fake.started.lock().unwrap().is_empty(), "nothing was started");
        assert_eq!(fake.placement(existing).visible.right, 960);
    }

    #[test]
    fn a_second_screen_that_is_not_there_is_named() {
        let fake = Fake::new(vec![screen(0, true)]);
        let results = run(&fake, vec![item("code", "other:1", Layout::AsIs)]);
        assert_eq!(results[0].outcome, Outcome::OpenedNotPlaced);
        assert_eq!(results[0].detail, "only one screen is connected");
    }

    #[test]
    fn an_item_goes_to_the_other_screen() {
        let fake = Fake::new(vec![screen(0, true), screen(1920, false)]);
        let results = run(&fake, vec![item("code", "other:1", Layout::AsIs)]);
        assert_eq!(results[0].outcome, Outcome::Placed, "{results:?}");
        let handle = fake.windows()[0].handle;
        assert_eq!(fake.placement(handle).screen, Some(1));
    }

    #[test]
    fn an_app_that_will_not_shrink_is_reported_as_minimum_size() {
        let fake = Fake::new(vec![screen(0, true)]);
        *fake.min_width.lock().unwrap() = Some(1200);
        let results = run(&fake, vec![item("spotify", "", Layout::RightHalf)]);
        assert_eq!(results[0].outcome, Outcome::PlacedAtMinimumSize, "{results:?}");
    }

    #[test]
    fn one_that_fails_or_never_shows_a_window_is_named_and_the_rest_still_run() {
        let fake = Fake::new(vec![screen(0, true)]);
        let results = run(
            &fake,
            vec![item("missing-app", "", Layout::LeftHalf), item("silent-app", "", Layout::LeftHalf), item("code", "", Layout::RightHalf)],
        );
        assert_eq!(results[0].outcome, Outcome::FailedToOpen);
        assert_eq!(results[1].outcome, Outcome::OpenedNotPlaced);
        assert!(results[1].detail.starts_with("its window did not appear within"));
        assert_eq!(results[2].outcome, Outcome::Placed);
        let text = summary("Desk", &results);
        assert!(text.starts_with("Desk: 1 of 3 ready."), "{text}");
        assert!(text.contains("missing-app did not open (it was not found on this PC)") && text.contains("silent-app opened but was not placed"));
    }

    #[test]
    fn maximizing_works_and_is_checked() {
        let fake = Fake::new(vec![screen(0, true)]);
        let results = run(&fake, vec![item("code", "", Layout::Maximized)]);
        assert_eq!(results[0].outcome, Outcome::Placed, "{results:?}");
    }

    #[test]
    fn names_for_the_summary() {
        let url = Item { kind: ItemKind::Url, target: "https://github.com/x?y".into(), ..item("", "", Layout::AsIs) };
        assert_eq!(display_name(&url), "github.com");
        assert_eq!(display_name(&item("C:\\Program Files\\Code\\Code.exe", "", Layout::AsIs)), "Code");
        assert_eq!(summary("Empty", &[]), "Empty: nothing to open.");
    }
}

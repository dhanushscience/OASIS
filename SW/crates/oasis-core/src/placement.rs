//! Putting windows where Quick Launch was told: the arithmetic of halves and screens, which window
//! belongs to which launched item, and the desktop and launcher it needs from the OS. Port of
//! `QuickLaunchLayoutMath.cs`, `QuickLaunchWindowMatch.cs`, `QuickLaunchNaming.cs`,
//! `MonitorSelector.cs` and the screen-name part of `MonitorPhrase.cs`.
//!
//! Two facts drive the layout (measured on real Windows in the C# app):
//! 1. A Windows 10/11 window has an invisible resize border, so the rectangle you set (outer) is
//!    larger than the frame you see (visible). A half is measured on the visible frame, or two
//!    halves show a gap.
//! 2. An app may refuse the size. A window that comes back larger than its half is kept wholly on
//!    its own screen, never spilling onto the next one.

/// A rectangle in virtual-screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl PixelRect {
    pub fn new(left: i32, top: i32, right: i32, bottom: i32) -> PixelRect {
        PixelRect { left, top, right, bottom }
    }

    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

/// How an item's window sits on its screen. Stored as a number in `profiles.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Wherever the app puts itself.
    AsIs,
    Maximized,
    LeftHalf,
    RightHalf,
}

impl Layout {
    pub fn from_number(n: i64) -> Layout {
        match n {
            1 => Layout::Maximized,
            2 => Layout::LeftHalf,
            3 => Layout::RightHalf,
            _ => Layout::AsIs,
        }
    }
}

/// How well a window matches the place it was asked to take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    Off,
    /// Where it was asked, within a few pixels.
    Exact,
    /// On the right screen, against the right side, but larger than asked: the app will not shrink.
    MinimumSize,
}

/// Pixels of slack when comparing a window with its target.
pub const TOLERANCE: i32 = 12;

/// The visible rectangle a layout asks for on a screen's work area.
pub fn target(work: PixelRect, layout: Layout) -> PixelRect {
    let half = work.width() / 2;
    match layout {
        Layout::LeftHalf => PixelRect::new(work.left, work.top, work.left + half, work.bottom),
        Layout::RightHalf => PixelRect::new(work.left + half, work.top, work.right, work.bottom),
        _ => work,
    }
}

/// Which part of a screen a spoken "snap" takes (W8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

impl Side {
    pub fn from_word(word: &str) -> Option<Side> {
        Some(match word {
            "left" => Side::Left,
            "right" => Side::Right,
            "top" | "upper" => Side::Top,
            "bottom" | "lower" => Side::Bottom,
            _ => return None,
        })
    }
}

/// A half of a work area.
pub fn snap_to(work: PixelRect, side: Side) -> PixelRect {
    let (w, h) = (work.width() / 2, work.height() / 2);
    match side {
        Side::Left => PixelRect::new(work.left, work.top, work.left + w, work.bottom),
        Side::Right => PixelRect::new(work.left + w, work.top, work.right, work.bottom),
        Side::Top => PixelRect::new(work.left, work.top, work.right, work.top + h),
        Side::Bottom => PixelRect::new(work.left, work.top + h, work.right, work.bottom),
    }
}

/// Where `count` windows go when they share a screen (W8): two are side by side; three or more are
/// one big window on the left and the others stacked in equal rows on the right.
pub fn split(work: PixelRect, count: usize) -> Vec<PixelRect> {
    match count {
        0 => Vec::new(),
        1 => vec![work],
        2 => vec![snap_to(work, Side::Left), snap_to(work, Side::Right)],
        n => {
            let right = snap_to(work, Side::Right);
            let rows = (n - 1) as i32;
            let row = right.height() / rows;
            let mut rects = vec![snap_to(work, Side::Left)];
            for i in 0..rows {
                let top = right.top + i * row;
                let bottom = if i == rows - 1 { right.bottom } else { top + row };
                rects.push(PixelRect::new(right.left, top, right.right, bottom));
            }
            rects
        }
    }
}

/// The outer rectangle to set so the visible frame lands on `target`, given the window's current
/// outer and visible rectangles (their difference is the invisible border).
pub fn outer_for(target: PixelRect, outer: PixelRect, visible: PixelRect) -> PixelRect {
    let left = (visible.left - outer.left).max(0);
    let top = (visible.top - outer.top).max(0);
    let right = (outer.right - visible.right).max(0);
    let bottom = (outer.bottom - visible.bottom).max(0);
    PixelRect::new(target.left - left, target.top - top, target.right + right, target.bottom + bottom)
}

/// Where the visible frame should move when the app made it larger than `target`: wholly on the
/// screen, against the side it was asked for. `None` when it fits.
pub fn keep_on_screen(visible: PixelRect, target: PixelRect, work: PixelRect, layout: Layout) -> Option<PixelRect> {
    if visible.width() <= target.width() + TOLERANCE && visible.height() <= target.height() + TOLERANCE {
        return None;
    }
    let left = if layout == Layout::RightHalf { work.left.max(work.right - visible.width()) } else { work.left };
    Some(PixelRect::new(left, work.top, left + visible.width(), work.top + visible.height()))
}

/// Compares a window's visible frame with the half it was asked to take.
pub fn check(visible: PixelRect, target: PixelRect, work: PixelRect, layout: Layout) -> Fit {
    let off = [
        (visible.left - target.left).abs(),
        (visible.top - target.top).abs(),
        (visible.right - target.right).abs(),
        (visible.bottom - target.bottom).abs(),
    ]
    .into_iter()
    .max()
    .unwrap_or(0);
    if off <= TOLERANCE {
        return Fit::Exact;
    }
    let inside = visible.left >= work.left - 2 && visible.right <= work.right + 2 && visible.top >= work.top - 2 && visible.bottom <= work.bottom + 2;
    let anchored = if layout == Layout::RightHalf {
        (visible.right - work.right).abs() <= TOLERANCE
    } else {
        (visible.left - work.left).abs() <= TOLERANCE
    };
    let larger = visible.width() > target.width() || visible.height() > target.height();
    if inside && anchored && larger { Fit::MinimumSize } else { Fit::Off }
}

/// A modest restored rectangle well inside a work area, to maximize from.
pub fn inset(work: PixelRect) -> PixelRect {
    let width = (work.width() - 120).clamp(200, 900);
    let height = (work.height() - 120).clamp(150, 600);
    PixelRect::new(work.left + 60, work.top + 60, work.left + 60 + width, work.top + 60 + height)
}

// ---- screens ---------------------------------------------------------------------------------

/// One display: its full bounds, the work area (bounds minus the taskbar) and whether it is the main one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Screen {
    pub bounds: PixelRect,
    pub work: PixelRect,
    pub primary: bool,
}

/// The index of the screen a spec names, or `None` when there is no such screen. Specs: `main`,
/// `other:N` (the Nth screen that is not the main one, left to right), `left`, `right`, `screen:N`
/// (Windows' own order, from 1).
pub fn pick_screen(screens: &[Screen], spec: &str) -> Option<usize> {
    if screens.is_empty() {
        return None;
    }
    let by_left = |mut list: Vec<usize>| {
        list.sort_by_key(|i| screens[*i].bounds.left);
        list
    };
    let spec = spec.trim();
    if spec == "main" {
        return Some(screens.iter().position(|s| s.primary).unwrap_or(0));
    }
    if let Some(n) = spec.strip_prefix("other:").and_then(|n| n.parse::<usize>().ok()).filter(|n| *n >= 1) {
        let others = by_left((0..screens.len()).filter(|i| !screens[*i].primary).collect());
        return others.get(n - 1).copied();
    }
    if spec == "left" || spec == "right" {
        if screens.len() < 2 {
            return None;
        }
        let ordered = by_left((0..screens.len()).collect());
        return if spec == "left" { ordered.first().copied() } else { ordered.last().copied() };
    }
    spec.strip_prefix("screen:").and_then(|n| n.parse::<usize>().ok()).filter(|n| (1..=screens.len()).contains(n)).map(|n| n - 1)
}

/// Where a window goes when it moves between work areas: the same offset from the corner, shrunk
/// to fit, never hanging off the target.
pub fn move_between(window: PixelRect, source_work: PixelRect, target_work: PixelRect) -> PixelRect {
    let width = window.width().min(target_work.width());
    let height = window.height().min(target_work.height());
    let left = (target_work.left + (window.left - source_work.left)).clamp(target_work.left, (target_work.right - width).max(target_work.left));
    let top = (target_work.top + (window.top - source_work.top)).clamp(target_work.top, (target_work.bottom - height).max(target_work.top));
    PixelRect::new(left, top, left + width, top + height)
}


// ---- grid regions ----------------------------------------------------------------------------

/// A rectangle on a screen's work area in thousandths (0..=1000): "this much of the screen, from
/// here". Lets Quick Launch say thirds, quarters or any grid box, not only halves. The editor's grid
/// is 12 columns by 6 rows, so a region it makes is a whole number of those cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

/// Columns and rows of the editor's grid, and what a captured window is snapped to.
pub const GRID_COLS: i32 = 12;
pub const GRID_ROWS: i32 = 6;

impl Region {
    /// A region kept inside the screen and at least one cell big; `None` when it has no size.
    pub fn new(x: i64, y: i64, w: i64, h: i64) -> Option<Region> {
        let (x, y) = (x.clamp(0, 999), y.clamp(0, 999));
        let (w, h) = (w.clamp(0, 1000 - x), h.clamp(0, 1000 - y));
        (w >= 40 && h >= 40).then_some(Region { x: x as u16, y: y as u16, w: w as u16, h: h as u16 })
    }

    pub fn rect(&self, work: PixelRect) -> PixelRect {
        let at = |origin: i32, size: i32, thousandths: u16| origin + (i64::from(size) * i64::from(thousandths) / 1000) as i32;
        let (w, h) = (work.width(), work.height());
        PixelRect::new(at(work.left, w, self.x), at(work.top, h, self.y), at(work.left, w, self.x + self.w), at(work.top, h, self.y + self.h))
    }

    /// The layout whose side the region is anchored to, so a window that will not shrink is kept
    /// against the right edge when the region touches it.
    pub fn anchor(&self) -> Layout {
        if self.x + self.w >= 1000 && self.x > 0 { Layout::RightHalf } else { Layout::LeftHalf }
    }

    /// The whole screen: what a window that fills it, maximized or not, is stored as.
    pub fn is_full(&self) -> bool {
        self.x == 0 && self.y == 0 && self.w == 1000 && self.h == 1000
    }
}

fn snap(value: i32, size: i32, cells: i32) -> i64 {
    let cell = (i64::from(value) * i64::from(cells) * 2 + i64::from(size)) / (2 * i64::from(size).max(1));
    (cell.clamp(0, i64::from(cells)) * 1000 + i64::from(cells) / 2) / i64::from(cells)
}

/// Where a window's visible frame sits on `work`, snapped to the editor's grid. `None` when it is
/// smaller than one cell.
pub fn region_of(visible: PixelRect, work: PixelRect) -> Option<Region> {
    let (w, h) = (work.width().max(1), work.height().max(1));
    let (x0, x1) = (snap(visible.left - work.left, w, GRID_COLS), snap(visible.right - work.left, w, GRID_COLS));
    let (y0, y1) = (snap(visible.top - work.top, h, GRID_ROWS), snap(visible.bottom - work.top, h, GRID_ROWS));
    Region::new(x0, y0, x1 - x0, y1 - y0)
}

/// The spec `pick_screen` reads back for screen `index`: `main`, or `other:N` left to right.
pub fn screen_spec(screens: &[Screen], index: usize) -> String {
    let Some(screen) = screens.get(index) else { return String::new() };
    if screen.primary {
        return "main".into();
    }
    let mut others: Vec<usize> = (0..screens.len()).filter(|i| !screens[*i].primary).collect();
    others.sort_by_key(|i| screens[*i].bounds.left);
    others.iter().position(|i| *i == index).map_or(String::new(), |n| format!("other:{}", n + 1))
}

/// How a window is stored when captured: its screen, and the layout or region it fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Captured {
    pub layout: Layout,
    pub region: Option<Region>,
}

/// A maximized window is `Maximized`; one that fills the screen the same; a half is a half; any other
/// box is a region. A window too small to place is left "as it opens".
pub fn capture(now: &WindowPlacement, work: PixelRect) -> Captured {
    if now.maximized {
        return Captured { layout: Layout::Maximized, region: None };
    }
    match region_of(now.visible, work) {
        Some(r) if r.is_full() => Captured { layout: Layout::Maximized, region: None },
        Some(Region { x: 0, y: 0, w: 500, h: 1000 }) => Captured { layout: Layout::LeftHalf, region: None },
        Some(Region { x: 500, y: 0, w: 500, h: 1000 }) => Captured { layout: Layout::RightHalf, region: None },
        region => Captured { layout: Layout::AsIs, region },
    }
}

// ---- windows ---------------------------------------------------------------------------------

/// A top-level app window as the launcher sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopWindow {
    pub handle: i64,
    pub process: String,
    pub title: String,
}

/// Where a window is. `outer` is the rectangle Windows sets; `visible` is the frame the user sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowPlacement {
    pub outer: PixelRect,
    pub visible: PixelRect,
    pub maximized: bool,
    pub minimized: bool,
    /// Index into the screens, or `None` if unknown.
    pub screen: Option<usize>,
}

/// The desktop as Quick Launch needs it. Adapters never fail for a window that has gone: they
/// return `None` or do nothing.
pub trait Desktop: Send + Sync {
    /// Visible top-level app windows, front to back; no tool windows, hidden or cloaked ones.
    fn windows(&self) -> Vec<DesktopWindow>;
    /// The window the user is working in, or 0.
    fn foreground(&self) -> i64;
    fn screens(&self) -> Vec<Screen>;
    fn read(&self, handle: i64) -> Option<WindowPlacement>;
    /// Un-maximize and un-minimize.
    fn restore(&self, handle: i64);
    fn maximize(&self, handle: i64);
    /// Move and size the window's outer rectangle, without activating it.
    fn set_outer(&self, handle: i64, outer: PixelRect);
}

/// How a launched item's window is recognised: process names first, a title hint as the fallback.
/// `app_key` groups items of one app, which start one after the other so "the new window of this
/// app" is never ambiguous.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub app_key: String,
    pub process_names: Vec<String>,
    pub title_hint: String,
}

/// Lowercase letters and digits only.
pub fn compact(text: &str) -> String {
    text.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Whether `process` (a process name) belongs to `target`. "Visual Studio Code" runs as Code.exe:
/// a name of four letters or more inside the other counts. Shorter ones must match exactly.
pub fn process_matches(process: &str, target: &Target) -> bool {
    let process = compact(process);
    if process.is_empty() {
        return false;
    }
    target.process_names.iter().map(|n| compact(n)).filter(|n| !n.is_empty()).any(|wanted| {
        process == wanted
            || (process.len() >= 4 && wanted.contains(&process))
            || (wanted.len() >= 4 && process.contains(&wanted))
    })
}

/// The first of `windows` that belongs to `target`; 0 when none does. The title is only a
/// fallback, because an editor with "chrome" in its title is not Chrome.
pub fn pick_window(windows: &[DesktopWindow], target: &Target) -> i64 {
    if let Some(w) = windows.iter().find(|w| process_matches(&w.process, target)) {
        return w.handle;
    }
    let hint = compact(&target.title_hint);
    if hint.len() < 4 {
        return 0;
    }
    windows.iter().find(|w| compact(&w.title).contains(&hint)).map_or(0, |w| w.handle)
}

/// The first window that is new (not in `before`), not taken by another item and belongs to `target`.
pub fn pick_fresh(windows: &[DesktopWindow], before: &[i64], claimed: &[i64], target: &Target) -> i64 {
    let fresh: Vec<DesktopWindow> = windows.iter().filter(|w| !before.contains(&w.handle) && !claimed.contains(&w.handle)).cloned().collect();
    pick_window(&fresh, target)
}

// ---- names -----------------------------------------------------------------------------------

/// The switch that makes a browser open a link in a window of its own. Firefox spells it with one dash.
pub fn new_window_switch(browser_process: &str) -> &'static str {
    if browser_process.trim().eq_ignore_ascii_case("firefox") { "-new-window" } else { "--new-window" }
}

/// Likely process names for a Store app id like `SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify`: the
/// part after `!`, and the last segment of the package name. They are hints, not facts.
pub fn store_app_names(app_user_model_id: &str) -> Vec<String> {
    let id = app_user_model_id.trim();
    let mut names: Vec<String> = Vec::new();
    let bang = id.rfind('!');
    if let Some(b) = bang.filter(|b| *b + 1 < id.len()) {
        names.push(id[b + 1..].to_string());
    }
    let package = bang.map_or(id, |b| &id[..b]);
    let package = package.find('_').filter(|u| *u > 0).map_or(package, |u| &package[..u]);
    let last = package.rsplit('.').next().unwrap_or(package);
    if !last.is_empty() {
        names.push(last.to_string());
    }
    let mut seen: Vec<String> = Vec::new();
    names.retain(|n| {
        let key = n.to_lowercase();
        let fresh = !seen.contains(&key);
        seen.push(key);
        fresh
    });
    names
}

/// File name without folder or extension: `C:\x\Code.exe` → `Code`.
pub fn stem(path: &str) -> String {
    let text = path.trim().trim_matches('"').trim_end_matches(['\\', '/']);
    let name = text.rsplit(['\\', '/']).next().unwrap_or(text);
    match name.rfind('.') {
        Some(dot) if dot > 0 => name[..dot].to_string(),
        _ => name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: PixelRect = PixelRect { left: 0, top: 0, right: 1920, bottom: 1040 };

    #[test]
    fn a_spoken_snap_takes_half_of_the_screen() {
        assert_eq!(snap_to(WORK, Side::Left), PixelRect::new(0, 0, 960, 1040));
        assert_eq!(snap_to(WORK, Side::Right), PixelRect::new(960, 0, 1920, 1040));
        assert_eq!(snap_to(WORK, Side::Top), PixelRect::new(0, 0, 1920, 520));
        assert_eq!(snap_to(WORK, Side::Bottom), PixelRect::new(0, 520, 1920, 1040));
        assert_eq!(Side::from_word("upper"), Some(Side::Top));
        assert_eq!(Side::from_word("middle"), None);
    }

    #[test]
    fn two_windows_share_halves_and_more_make_one_big_and_a_stack() {
        assert_eq!(split(WORK, 2), vec![PixelRect::new(0, 0, 960, 1040), PixelRect::new(960, 0, 1920, 1040)]);
        let three = split(WORK, 3);
        assert_eq!(three, vec![PixelRect::new(0, 0, 960, 1040), PixelRect::new(960, 0, 1920, 520), PixelRect::new(960, 520, 1920, 1040)]);
        let four = split(WORK, 4);
        assert_eq!(four.len(), 4);
        assert_eq!(four[3].bottom, 1040, "the last row reaches the bottom whatever the rounding");
        assert!(four[1..].windows(2).all(|w| w[0].bottom == w[1].top), "rows touch, no gap");
        assert_eq!(split(WORK, 1), vec![WORK]);
        assert!(split(WORK, 0).is_empty());
    }

    #[test]
    fn halves_split_the_work_area() {
        assert_eq!(target(WORK, Layout::LeftHalf), PixelRect::new(0, 0, 960, 1040));
        assert_eq!(target(WORK, Layout::RightHalf), PixelRect::new(960, 0, 1920, 1040));
        assert_eq!(target(WORK, Layout::Maximized), WORK);
    }

    #[test]
    fn the_invisible_border_is_added_to_what_is_set() {
        let outer = PixelRect::new(100, 100, 900, 700);
        let visible = PixelRect::new(107, 100, 893, 693);
        let got = outer_for(PixelRect::new(0, 0, 960, 1040), outer, visible);
        assert_eq!(got, PixelRect::new(-7, 0, 967, 1047));
    }

    #[test]
    fn an_app_that_will_not_shrink_stays_on_its_own_screen_against_its_side() {
        let t = target(WORK, Layout::RightHalf);
        let wide = PixelRect::new(900, 0, 1920 + 40, 800);
        let kept = keep_on_screen(wide, t, WORK, Layout::RightHalf).unwrap();
        assert_eq!((kept.left, kept.right), (1920 - 1060, 1920));
        assert!(keep_on_screen(t, t, WORK, Layout::RightHalf).is_none());
    }

    #[test]
    fn fit_tells_exact_from_minimum_size_from_off() {
        let t = target(WORK, Layout::RightHalf);
        assert_eq!(check(t, t, WORK, Layout::RightHalf), Fit::Exact);
        assert_eq!(check(PixelRect::new(5, 0, 965, 1040), t, WORK, Layout::RightHalf), Fit::Off);
        let big = PixelRect::new(1920 - 1200, 0, 1920, 1040);
        assert_eq!(check(big, t, WORK, Layout::RightHalf), Fit::MinimumSize);
        assert_eq!(check(PixelRect::new(0, 0, 400, 400), t, WORK, Layout::RightHalf), Fit::Off);
    }

    fn two_screens() -> Vec<Screen> {
        let a = PixelRect::new(0, 0, 1920, 1080);
        let b = PixelRect::new(1920, 0, 3840, 1080);
        vec![Screen { bounds: b, work: b, primary: false }, Screen { bounds: a, work: a, primary: true }]
    }

    #[test]
    fn screens_are_picked_by_name() {
        let s = two_screens();
        assert_eq!(pick_screen(&s, "main"), Some(1));
        assert_eq!(pick_screen(&s, "other:1"), Some(0));
        assert_eq!(pick_screen(&s, "other:2"), None);
        assert_eq!(pick_screen(&s, "left"), Some(1));
        assert_eq!(pick_screen(&s, "right"), Some(0));
        assert_eq!(pick_screen(&s, "screen:2"), Some(1));
        assert_eq!(pick_screen(&s, "screen:3"), None);
        assert_eq!(pick_screen(&s[..1], "left"), None, "one screen has no left or right");
        assert_eq!(pick_screen(&[], "main"), None);
        assert_eq!(pick_screen(&s, "nonsense"), None);
    }

    #[test]
    fn a_moved_window_keeps_its_offset_and_never_hangs_off() {
        let src = PixelRect::new(0, 0, 1920, 1080);
        let dst = PixelRect::new(1920, 0, 3840, 1080);
        assert_eq!(move_between(PixelRect::new(100, 50, 600, 450), src, dst), PixelRect::new(2020, 50, 2520, 450));
        let big = move_between(PixelRect::new(1000, 0, 2900, 1200), src, dst);
        assert!(big.right <= dst.right && big.bottom <= dst.bottom && big.left >= dst.left);
    }

    #[test]
    fn regions_snap_to_the_grid_and_come_back_as_halves() {
        let work = PixelRect::new(0, 0, 1920, 1080);
        let wp = |v: PixelRect, maximized| WindowPlacement { outer: v, visible: v, maximized, minimized: false, screen: Some(0) };
        assert_eq!(capture(&wp(PixelRect::new(0, 0, 960, 1080), false), work).layout, Layout::LeftHalf);
        assert_eq!(capture(&wp(PixelRect::new(962, 3, 1920, 1078), false), work).layout, Layout::RightHalf);
        assert_eq!(capture(&wp(PixelRect::new(100, 100, 400, 400), true), work).layout, Layout::Maximized);
        assert_eq!(capture(&wp(PixelRect::new(0, 0, 1915, 1075), false), work).layout, Layout::Maximized);
        let quarter = capture(&wp(PixelRect::new(960, 540, 1920, 1080), false), work);
        assert_eq!(quarter.region, Region::new(500, 500, 500, 500));
        assert_eq!(quarter.region.unwrap().rect(work), PixelRect::new(960, 540, 1920, 1080));
        assert_eq!(region_of(PixelRect::new(10, 10, 30, 30), work), None, "smaller than a cell");
    }

    #[test]
    fn a_screen_spec_names_the_screen_it_came_from() {
        let s = two_screens();
        for i in 0..2 {
            assert_eq!(pick_screen(&s, &screen_spec(&s, i)), Some(i));
        }
    }

    fn w(handle: i64, process: &str, title: &str) -> DesktopWindow {
        DesktopWindow { handle, process: process.into(), title: title.into() }
    }

    #[test]
    fn a_window_belongs_to_an_app_by_process_first_and_title_last() {
        let code = Target { app_key: "code".into(), process_names: vec!["Visual Studio Code".into(), "code".into()], title_hint: "".into() };
        let windows = [w(1, "notepad", "Chrome notes"), w(2, "Code", "main.rs")];
        assert_eq!(pick_window(&windows, &code), 2);
        assert!(process_matches("chrome", &Target { app_key: "".into(), process_names: vec!["Google Chrome".into()], title_hint: "".into() }));
        assert!(!process_matches("ab", &Target { app_key: "".into(), process_names: vec!["abc".into()], title_hint: "".into() }), "short names match exactly");
        let by_title = Target { app_key: "x".into(), process_names: vec![], title_hint: "Docklight".into() };
        assert_eq!(pick_window(&[w(7, "x", "Docklight V2")], &by_title), 7);
        assert_eq!(pick_window(&[w(7, "x", "Docklight V2")], &Target { title_hint: "ab".into(), ..by_title }), 0, "a short hint is too weak");
    }

    #[test]
    fn a_fresh_window_is_new_and_unclaimed() {
        let t = Target { app_key: "chrome".into(), process_names: vec!["chrome".into()], title_hint: "".into() };
        let windows = [w(1, "chrome", "old"), w(2, "chrome", "taken"), w(3, "chrome", "new")];
        assert_eq!(pick_fresh(&windows, &[1], &[2], &t), 3);
        assert_eq!(pick_fresh(&windows, &[1, 3], &[2], &t), 0);
    }

    #[test]
    fn names_from_store_ids_paths_and_browsers() {
        assert_eq!(store_app_names("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify"), ["Spotify", "SpotifyMusic"]);
        assert_eq!(store_app_names("Microsoft.WindowsNotepad_8wekyb3d8bbwe!App"), ["App", "WindowsNotepad"]);
        assert_eq!(stem("C:\\x\\Code.exe"), "Code");
        assert_eq!(stem("\"C:\\x\\y\\\""), "y");
        assert_eq!(new_window_switch("Firefox"), "-new-window");
        assert_eq!(new_window_switch("chrome"), "--new-window");
        assert_eq!(compact("Visual-Studio Code"), "visualstudiocode");
        assert_eq!(inset(WORK), PixelRect::new(60, 60, 960, 660));
    }
}

//! Turning an app's friendly name ("spotify", "vs code", "notepad") into something that can be
//! started. Port of the decision logic of `Services/WindowsAppDiscovery.cs`.
//!
//! The order is the C# one: a real file; a Start Menu shortcut (unless a Store app has exactly
//! that name); a Store app; the usual install folders; the registry's App Paths; a fuzzy match on
//! the names of installed apps (so a speech slip like "doclight" still finds Docklight); and last
//! a plain `name.exe`. Where the names come from is the caller's business ([`AppSources`]), so
//! the rules are tested without a Start Menu.

use crate::matching::levenshtein;

/// What a name resolves to, and so how to start it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// A program file (or a bare `name.exe` for the system to find on its PATH).
    Exe(String),
    /// A Start Menu shortcut.
    Shortcut(String),
    /// A Store app, by its app id (`shell:AppsFolder\<id>`).
    Store(String),
}

/// The machine's installed apps, as the OS reports them.
pub trait AppSources {
    fn file_exists(&self, path: &str) -> bool;
    /// Expand `%NAME%` variables.
    fn expand_env(&self, text: &str) -> String;
    /// Start Menu shortcuts: `(name without extension, full path)`.
    fn start_menu_shortcuts(&self) -> Vec<(String, String)>;
    /// Start Apps (including the Store): `(display name, app id)`.
    fn start_apps(&self) -> Vec<(String, String)>;
    /// A program in the usual per-user and per-machine install folders whose folder name matches.
    fn install_folder_exe(&self, token: &str) -> Option<String>;
    /// A program registered under the registry's App Paths whose name matches.
    fn app_paths_exe(&self, token: &str) -> Option<String>;
}

/// The file name without folder and extension, lower case ("C:\x\Y.exe" → "y").
pub fn normalize_token(raw: &str) -> String {
    let name = raw.trim().rsplit(['\\', '/']).next().unwrap_or("");
    let stem = match name.rfind('.') {
        Some(dot) if dot > 0 => &name[..dot],
        _ => name,
    };
    stem.to_lowercase()
}

/// A name for comparing: lower case, no spaces, no trailing version number ("FreeCAD 1.0" and
/// "free cad" both become "freecad"). A name that is only a number is left alone.
pub fn squash(name: &str) -> String {
    let lower = name.to_lowercase();
    let mut words: Vec<&str> = lower.split_whitespace().collect();
    while words.len() > 1 && words.last().is_some_and(|w| w.chars().all(|c| c.is_ascii_digit() || c == '.')) {
        words.pop();
    }
    words.concat()
}

/// Equal, or either contains the other (spaces and version numbers ignored).
pub fn name_matches(token: &str, candidate: &str) -> bool {
    let (t, c) = (squash(token), squash(candidate));
    !c.is_empty() && !t.is_empty() && (c == t || c.contains(&t) || t.contains(&c))
}

/// How well a name fits, 0 to 1: equal, starts with, contains, or nothing.
pub fn score_name(token: &str, candidate: &str) -> f64 {
    let (token, c) = (squash(token), squash(candidate));
    let token = token.as_str();
    if c.is_empty() || token.is_empty() {
        return 0.0;
    }
    if c == token {
        1.0
    } else if c.starts_with(token) || token.starts_with(&c) {
        0.92
    } else if c.contains(token) || token.contains(&c) {
        0.85
    } else {
        0.0
    }
}

/// The best installed app name for a (possibly misheard) token, or `None`.
pub fn fuzzy_match_app_name(token: &str, src: &dyn AppSources) -> Option<String> {
    let needle = squash(&normalize_token(token));
    if needle.is_empty() {
        return None;
    }
    let mut candidates: Vec<String> = src.start_apps().into_iter().map(|(name, _)| name).collect();
    candidates.extend(src.start_menu_shortcuts().into_iter().map(|(name, _)| name));
    candidates.retain(|c| !c.trim().is_empty());
    candidates.dedup_by(|a, b| a.eq_ignore_ascii_case(b));

    let mut best: Option<String> = None;
    let mut best_distance = usize::MAX;
    for candidate in candidates.iter().cloned() {
        let hay = squash(&candidate);
        if hay == needle {
            return Some(candidate);
        }
        if hay.contains(&needle) || needle.contains(&hay) {
            let distance = hay.chars().count().abs_diff(needle.chars().count());
            if distance < best_distance {
                best_distance = distance;
                best = Some(candidate);
            }
            continue;
        }
        let distance = levenshtein(&needle, &hay);
        let allowed = 2.max(needle.chars().count().min(hay.chars().count()) / 3);
        if distance <= allowed && distance < best_distance {
            best_distance = distance;
            best = Some(candidate);
        }
    }
    best.or_else(|| sounds_like(&needle, &candidates))
}

/// The one installed app whose name sounds like `needle` ("clawed", "cloud" → "Claude"), when the
/// spelling is too far off for the letter count above. Only when exactly one app sounds like it.
fn sounds_like(needle: &str, candidates: &[String]) -> Option<String> {
    let key = sound_key(needle);
    if needle.chars().count() < 4 || key.chars().count() < 3 {
        return None;
    }
    let mut hits = candidates.iter().filter(|c| sound_key(&squash(c)) == key);
    let first = hits.next()?;
    hits.all(|other| other.eq_ignore_ascii_case(first)).then(|| first.clone())
}

/// How a name sounds, roughly: its consonants, with those that sound alike written the same
/// (c, k, q, g; s, z, x; d, t; f, v) and doubles merged. Vowels, h, w and y are dropped.
fn sound_key(name: &str) -> String {
    let mut key = String::new();
    for c in name.to_lowercase().chars() {
        let sound = match c {
            'a' | 'e' | 'i' | 'o' | 'u' | 'h' | 'w' | 'y' => continue,
            'c' | 'k' | 'q' | 'g' => 'k',
            's' | 'z' | 'x' => 's',
            'd' | 't' => 't',
            'f' | 'v' => 'f',
            c if c.is_alphanumeric() => c,
            _ => continue,
        };
        if !key.ends_with(sound) {
            key.push(sound);
        }
    }
    key
}

fn start_app_by_name(name: &str, src: &dyn AppSources) -> Option<String> {
    src.start_apps().into_iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, id)| id)
}

fn resolve_from_start_apps(token: &str, src: &dyn AppSources) -> Option<String> {
    let apps = src.start_apps();
    let mut best: Option<&(String, String)> = None;
    let mut best_score = 0.0;
    for app in &apps {
        let score = score_name(token, &app.0);
        if score > best_score {
            best_score = score;
            best = Some(app);
        }
    }
    if best_score >= 0.85 {
        return best.map(|(_, id)| id.clone());
    }
    fuzzy_match_app_name(token, src).and_then(|name| start_app_by_name(&name, src))
}

fn find_start_menu_shortcut(token: &str, src: &dyn AppSources) -> Option<String> {
    let mut candidates: Vec<(String, String)> =
        src.start_menu_shortcuts().into_iter().filter(|(name, _)| !name.trim().is_empty() && name_matches(token, name)).collect();
    // The exact name first, then the shortest path (usually the top-level shortcut).
    candidates.sort_by(|a, b| {
        score_name(token, &b.0)
            .partial_cmp(&score_name(token, &a.0))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.len().cmp(&b.1.len()))
    });
    candidates.into_iter().next().map(|(_, path)| path)
}

/// What `raw` should start, or `None` if it is blank.
pub fn resolve(raw: &str, src: &dyn AppSources) -> Option<Resolved> {
    resolve_at(raw, src, 0)
}

fn resolve_at(raw: &str, src: &dyn AppSources, depth: u8) -> Option<Resolved> {
    let trimmed = raw.trim().trim_matches('"');
    if trimmed.is_empty() {
        return None;
    }
    if src.file_exists(trimmed) {
        return Some(Resolved::Exe(trimmed.to_string()));
    }
    let expanded = src.expand_env(trimmed);
    if src.file_exists(&expanded) {
        return Some(Resolved::Exe(expanded));
    }

    let token = normalize_token(trimmed);
    if token.is_empty() {
        return None;
    }

    // 1. A Start Menu shortcut. But a Store app with exactly this name beats a shortcut that merely
    // contains the word: "notepad" is Windows Notepad, not Notepad++.
    let shortcut = find_start_menu_shortcut(token.as_str(), src);
    if let Some(lnk) = &shortcut {
        let lnk_name = normalize_token(lnk);
        if lnk_name != token {
            if let Some(id) = start_app_by_name(&token, src) {
                return Some(Resolved::Store(id));
            }
        }
    }
    if let Some(lnk) = shortcut {
        return Some(Resolved::Shortcut(lnk));
    }

    // 2. A Store (UWP) app.
    if let Some(id) = resolve_from_start_apps(&token, src) {
        return Some(Resolved::Store(id));
    }
    // 3. The usual install folders, 4. the registry.
    if let Some(exe) = src.install_folder_exe(&token) {
        return Some(Resolved::Exe(exe));
    }
    if let Some(exe) = src.app_paths_exe(&token) {
        return Some(Resolved::Exe(exe));
    }
    // 5. A fuzzy match on installed names, once (the match is itself a name that resolves).
    if depth == 0 {
        if let Some(fuzzy) = fuzzy_match_app_name(&token, src).filter(|f| !f.eq_ignore_ascii_case(&token)) {
            if let Some(found) = resolve_at(&fuzzy, src, 1) {
                return Some(found);
            }
        }
    }
    // 6. A path as given, or a bare program name for the system to find.
    if trimmed.contains(['\\', '/']) {
        return Some(Resolved::Exe(trimmed.to_string()));
    }
    let exe = if token.ends_with(".exe") { token } else { format!("{token}.exe") };
    Some(Resolved::Exe(exe))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Machine {
        files: Vec<&'static str>,
        shortcuts: Vec<(&'static str, &'static str)>,
        start_apps: Vec<(&'static str, &'static str)>,
        install: Vec<(&'static str, &'static str)>,
        app_paths: Vec<(&'static str, &'static str)>,
    }

    impl AppSources for Machine {
        fn file_exists(&self, path: &str) -> bool {
            self.files.contains(&path)
        }
        fn expand_env(&self, text: &str) -> String {
            text.replace("%LOCALAPPDATA%", r"C:\Users\u\AppData\Local")
        }
        fn start_menu_shortcuts(&self) -> Vec<(String, String)> {
            self.shortcuts.iter().map(|(n, p)| (n.to_string(), p.to_string())).collect()
        }
        fn start_apps(&self) -> Vec<(String, String)> {
            self.start_apps.iter().map(|(n, i)| (n.to_string(), i.to_string())).collect()
        }
        fn install_folder_exe(&self, token: &str) -> Option<String> {
            self.install.iter().find(|(k, _)| *k == token).map(|(_, p)| p.to_string())
        }
        fn app_paths_exe(&self, token: &str) -> Option<String> {
            self.app_paths.iter().find(|(k, _)| *k == token).map(|(_, p)| p.to_string())
        }
    }

    #[test]
    fn tokens_are_the_file_name_without_folder_or_extension_in_lower_case() {
        assert_eq!(normalize_token(r"C:\Apps\Spotify.EXE"), "spotify");
        assert_eq!(normalize_token("Visual Studio Code"), "visual studio code");
        assert_eq!(normalize_token("  chrome.exe "), "chrome");
        assert_eq!(normalize_token(".hidden"), ".hidden");
    }

    #[test]
    fn name_scores_follow_the_csharp_rules() {
        assert_eq!(score_name("spotify", "Spotify"), 1.0);
        assert_eq!(score_name("spot", "Spotify"), 0.92);
        assert_eq!(score_name("notepad", "Notepad++"), 0.92);
        assert_eq!(score_name("pad", "Notepad"), 0.85);
        assert_eq!(score_name("zzz", "Notepad"), 0.0);
        assert!(name_matches("pad", "NotePad") && !name_matches("zzz", "Notepad") && !name_matches("x", " "));
    }

    #[test]
    fn a_file_that_exists_is_used_as_it_is() {
        let machine = Machine { files: vec![r"C:\Tools\x.exe"], ..Default::default() };
        assert_eq!(resolve(r#""C:\Tools\x.exe""#, &machine), Some(Resolved::Exe(r"C:\Tools\x.exe".into())));
        let expanded = Machine { files: vec![r"C:\Users\u\AppData\Local\app.exe"], ..Default::default() };
        assert_eq!(resolve(r"%LOCALAPPDATA%\app.exe", &expanded), Some(Resolved::Exe(r"C:\Users\u\AppData\Local\app.exe".into())));
    }

    #[test]
    fn a_start_menu_shortcut_is_found_by_name() {
        let machine = Machine { shortcuts: vec![("Spotify", r"C:\SM\Spotify.lnk"), ("Slack", r"C:\SM\Slack.lnk")], ..Default::default() };
        assert_eq!(resolve("spotify", &machine), Some(Resolved::Shortcut(r"C:\SM\Spotify.lnk".into())));
    }

    #[test]
    fn the_exact_shortcut_beats_a_longer_one_and_the_shortest_path_breaks_a_tie() {
        let machine = Machine {
            shortcuts: vec![("Docklight Scripting", r"C:\SM\Docklight\Docklight Scripting.lnk"), ("Docklight", r"C:\SM\Docklight\Docklight.lnk")],
            ..Default::default()
        };
        assert_eq!(resolve("docklight", &machine), Some(Resolved::Shortcut(r"C:\SM\Docklight\Docklight.lnk".into())));
        let tie = Machine { shortcuts: vec![("Zed", r"C:\SM\a\b\Zed.lnk"), ("Zed", r"C:\SM\Zed.lnk")], ..Default::default() };
        assert_eq!(resolve("zed", &tie), Some(Resolved::Shortcut(r"C:\SM\Zed.lnk".into())));
    }

    #[test]
    fn a_store_app_with_exactly_that_name_beats_a_shortcut_that_only_contains_it() {
        let machine = Machine {
            shortcuts: vec![("Notepad++", r"C:\SM\Notepad++.lnk")],
            start_apps: vec![("Notepad", "Microsoft.WindowsNotepad_8wekyb3d8bbwe!App")],
            ..Default::default()
        };
        assert_eq!(resolve("notepad", &machine), Some(Resolved::Store("Microsoft.WindowsNotepad_8wekyb3d8bbwe!App".into())));
    }

    #[test]
    fn a_store_app_is_found_when_there_is_no_shortcut() {
        let machine = Machine { start_apps: vec![("Spotify", "SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify")], ..Default::default() };
        assert_eq!(resolve("Spotify", &machine), Some(Resolved::Store("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify".into())));
    }

    #[test]
    fn install_folders_and_the_registry_come_after_the_start_menu() {
        let machine = Machine {
            install: vec![("kicad", r"C:\Program Files\KiCad\bin\kicad.exe")],
            app_paths: vec![("winword", r"C:\Office\WINWORD.EXE")],
            ..Default::default()
        };
        assert_eq!(resolve("kicad", &machine), Some(Resolved::Exe(r"C:\Program Files\KiCad\bin\kicad.exe".into())));
        assert_eq!(resolve("winword", &machine), Some(Resolved::Exe(r"C:\Office\WINWORD.EXE".into())));
    }

    #[test]
    fn a_misheard_name_still_finds_the_app() {
        let machine = Machine { shortcuts: vec![("Docklight", r"C:\SM\Docklight.lnk")], ..Default::default() };
        assert_eq!(resolve("doclight", &machine), Some(Resolved::Shortcut(r"C:\SM\Docklight.lnk".into())));
        assert_eq!(fuzzy_match_app_name("doclight", &machine).as_deref(), Some("Docklight"));
        // Heard wrong, but it sounds the same.
        assert_eq!(sounds_like("clawed", &["Notepad".into(), "Claude".into()]).as_deref(), Some("Claude"));
        assert_eq!(sounds_like("cloud", &["Claude".into()]).as_deref(), Some("Claude"));
        assert_eq!(sounds_like("plot", &["Claude".into()]), None, "a different sound is not guessed");
        assert_eq!(sounds_like("clawed", &["Claude".into(), "Colt".into()]), None, "two apps sound like it: no guess");
        assert_eq!(sounds_like("cat", &["Kite".into()]), None, "too short to tell");
    }

    #[test]
    fn a_name_nothing_matches_becomes_a_bare_program_name_for_the_system_to_find() {
        let machine = Machine::default();
        assert_eq!(resolve("calc", &machine), Some(Resolved::Exe("calc.exe".into())));
        assert_eq!(resolve("tool.exe", &machine), Some(Resolved::Exe("tool.exe".into())));
        assert_eq!(resolve(r"D:\gone\thing.exe", &machine), Some(Resolved::Exe(r"D:\gone\thing.exe".into())), "a path is kept as given");
    }

    #[test]
    fn spaces_and_version_numbers_do_not_stop_a_match() {
        let machine = Machine {
            shortcuts: vec![("Uninstall FreeCAD 1.0", r"C:\SM\FreeCAD\Uninstall FreeCAD 1.0.lnk"), ("FreeCAD 1.0", r"C:\SM\FreeCAD 1.0.lnk")],
            ..Default::default()
        };
        for said in ["free cad", "freecad", "FreeCAD"] {
            assert_eq!(resolve(said, &machine), Some(Resolved::Shortcut(r"C:\SM\FreeCAD 1.0.lnk".into())), "{said}");
        }
    }

    #[test]
    fn a_blank_name_resolves_to_nothing() {
        assert_eq!(resolve("   ", &Machine::default()), None);
        assert_eq!(resolve("\"\"", &Machine::default()), None);
    }

    #[test]
    fn the_fuzzy_step_does_not_loop() {
        // A fuzzy name that itself fuzzy-matches something else must stop after one hop.
        let machine = Machine { start_apps: vec![("Alpha Beta", "id-ab")], ..Default::default() };
        assert!(resolve("alpha beat", &machine).is_some());
    }
}

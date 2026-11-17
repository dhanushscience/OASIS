//! Port of `Services/ShortcutChord.cs` and the timing of `ActionExecutor.TryExecuteShortcut`.
//!
//! A shortcut is text such as `Ctrl+Shift+'` or `Win+D`. [`Chord::parse`] turns it into OS-neutral
//! keys (the C# code produced Windows virtual-key codes; each OS backend maps these). [`plan`]
//! gives the exact order and pauses for pressing and releasing them, so every OS injects a
//! chord the same way: keys down in order, then up in reverse.

use std::time::Duration;

/// One key of a chord.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChordKey {
    Ctrl,
    Shift,
    Alt,
    /// Windows key / Command / Super.
    Win,
    /// `A`–`Z` (upper case) or `0`–`9`.
    Char(char),
    /// `F1` and up.
    Function(u8),
    /// One of `. , / ; ' [ ] \ - = \``.
    Punct(char),
    Space,
    Enter,
    Escape,
    Tab,
    /// Caps Lock, as a key to press or to use as a button.
    CapsLock,
    Backspace,
    Delete,
    Insert,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    VolumeUp,
    VolumeDown,
    VolumeMute,
    /// The media keys: play/pause, next track, previous track, stop.
    PlayPause,
    NextTrack,
    PrevTrack,
    MediaStop,
}

impl ChordKey {
    pub fn is_modifier(self) -> bool {
        matches!(self, ChordKey::Ctrl | ChordKey::Shift | ChordKey::Alt | ChordKey::Win)
    }

    /// Keys that carry the "extended" flag on Windows: the navigation block and volume keys.
    pub fn is_extended(self) -> bool {
        matches!(
            self,
            ChordKey::PageUp
                | ChordKey::PageDown
                | ChordKey::End
                | ChordKey::Home
                | ChordKey::Left
                | ChordKey::Up
                | ChordKey::Right
                | ChordKey::Down
                | ChordKey::Insert
                | ChordKey::Delete
                | ChordKey::VolumeMute
                | ChordKey::VolumeDown
                | ChordKey::VolumeUp
                | ChordKey::PlayPause
                | ChordKey::NextTrack
                | ChordKey::PrevTrack
                | ChordKey::MediaStop
        )
    }

    pub fn is_punct(self) -> bool {
        matches!(self, ChordKey::Punct(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chord {
    pub keys: Vec<ChordKey>,
}

impl Chord {
    /// `Err` carries a sentence for the user ("Unsupported key 'Foo'.").
    pub fn parse(shortcut: &str) -> Result<Chord, String> {
        if shortcut.trim().is_empty() {
            return Err("Shortcut is empty.".into());
        }
        let parts: Vec<&str> =
            shortcut.split('+').map(str::trim).filter(|p| !p.is_empty()).collect();
        if parts.is_empty() {
            return Err("Shortcut has no keys.".into());
        }
        let keys = parts
            .iter()
            .map(|part| {
                parse_key(&normalize_key_name(part))
                    .ok_or_else(|| format!("Unsupported key '{part}'."))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Chord { keys })
    }

    pub fn is_valid(shortcut: &str) -> bool {
        Chord::parse(shortcut).is_ok()
    }
}

/// How a shortcut is shown: the same keys, with `Ctrl`/`Shift`/`Alt`/`Win` spelled one way and
/// typographic quotes replaced. Other key names are left as typed.
pub fn normalize_display(shortcut: &str) -> String {
    if shortcut.trim().is_empty() {
        return String::new();
    }
    shortcut
        .split('+')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(normalize_key_name)
        .collect::<Vec<_>>()
        .join("+")
}

pub fn normalize_key_name(key: &str) -> String {
    let key = key.trim();
    if key.is_empty() {
        return String::new();
    }
    let key: String = key
        .chars()
        .map(|c| if matches!(c, '\u{2018}' | '\u{2019}' | '\u{2032}' | '\u{00B4}') { '\'' } else { c })
        .collect();

    match key.to_ascii_uppercase().as_str() {
        "CTRL" | "CONTROL" => return "Ctrl".into(),
        "SHIFT" => return "Shift".into(),
        "ALT" => return "Alt".into(),
        "WIN" | "WINDOWS" => return "Win".into(),
        _ => {}
    }
    if matches!(key.as_str(), "\"" | "\u{201C}" | "\u{201D}") {
        return "'".into();
    }
    key
}

fn parse_key(key: &str) -> Option<ChordKey> {
    let k = key.to_ascii_lowercase();
    match k.as_str() {
        "ctrl" | "control" => return Some(ChordKey::Ctrl),
        "shift" => return Some(ChordKey::Shift),
        "alt" => return Some(ChordKey::Alt),
        "win" | "windows" => return Some(ChordKey::Win),
        _ => {}
    }
    if let Some(p) = parse_punct(&k) {
        return Some(ChordKey::Punct(p));
    }
    let mut chars = k.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_ascii_lowercase() {
            return Some(ChordKey::Char(c.to_ascii_uppercase()));
        }
        if c.is_ascii_digit() {
            return Some(ChordKey::Char(c));
        }
    }
    if let Some(rest) = k.strip_prefix('f') {
        if let Ok(n @ 1..=24) = rest.parse::<u8>() {
            return Some(ChordKey::Function(n));
        }
    }
    Some(match k.as_str() {
        "space" => ChordKey::Space,
        "enter" | "return" => ChordKey::Enter,
        "esc" | "escape" => ChordKey::Escape,
        "tab" => ChordKey::Tab,
        "capslock" | "caps" => ChordKey::CapsLock,
        "back" | "backspace" => ChordKey::Backspace,
        "delete" | "del" => ChordKey::Delete,
        "up" => ChordKey::Up,
        "down" => ChordKey::Down,
        "left" => ChordKey::Left,
        "right" => ChordKey::Right,
        "home" => ChordKey::Home,
        "end" => ChordKey::End,
        "pageup" | "pgup" => ChordKey::PageUp,
        "pagedown" | "pgdn" => ChordKey::PageDown,
        "insert" | "ins" => ChordKey::Insert,
        "volumeup" | "volup" => ChordKey::VolumeUp,
        "volumedown" | "voldown" => ChordKey::VolumeDown,
        "volumemute" | "mute" => ChordKey::VolumeMute,
        "playpause" | "play" | "pause" | "mediaplaypause" => ChordKey::PlayPause,
        "nexttrack" | "next" | "medianexttrack" | "mediatracknext" => ChordKey::NextTrack,
        "prevtrack" | "previoustrack" | "previous" | "prev" | "mediaprevioustrack" | "mediatrackprevious" => ChordKey::PrevTrack,
        "mediastop" | "stopmedia" => ChordKey::MediaStop,
        _ => return None,
    })
}

fn parse_punct(key: &str) -> Option<char> {
    Some(match key {
        "." | "period" | "oemperiod" => '.',
        "," | "comma" | "oemcomma" => ',',
        "/" | "slash" | "oemquestion" => '/',
        ";" | "semicolon" | "oem1" => ';',
        "'" | "quote" | "oem7" | "oemquotes" | "\"" | "doublequote" => '\'',
        "[" | "lbracket" | "oem4" => '[',
        "]" | "rbracket" | "oem6" => ']',
        "\\" | "backslash" | "oem5" => '\\',
        "-" | "minus" | "dash" | "oemminus" => '-',
        "=" | "equals" | "equal" | "oemplus" => '=',
        "`" | "grave" | "backtick" | "oem3" => '`',
        _ => return None,
    })
}

/// One key event in a chord.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyStroke {
    pub key: ChordKey,
    pub release: bool,
    /// Send by scan code only. The last key of a chord, when it is punctuation, is sent this way
    /// so the layout of the focused window decides the character (C# `ShouldUseScanCodeOnly`).
    pub scancode_only: bool,
    /// Wait this long after the event before the next.
    pub pause_after: Duration,
}

/// Press every key in order, then release in reverse. Windows is given longer to register than
/// the others, and a pause separates the last press from the first release.
pub fn plan(chord: &Chord) -> Vec<KeyStroke> {
    let keys = &chord.keys;
    let last = keys.len().saturating_sub(1);
    let scancode_only =
        |i: usize| i == last && !keys[i].is_modifier() && keys[i].is_punct();
    let ms = Duration::from_millis;

    let mut strokes = Vec::with_capacity(keys.len() * 2);
    for (i, &key) in keys.iter().enumerate() {
        let pause_after = if i < last {
            if key == ChordKey::Win { ms(25) } else { ms(10) }
        } else {
            ms(10)
        };
        strokes.push(KeyStroke { key, release: false, scancode_only: scancode_only(i), pause_after });
    }
    for (i, &key) in keys.iter().enumerate().rev() {
        let pause_after = if i > 0 { ms(5) } else { Duration::ZERO };
        strokes.push(KeyStroke { key, release: true, scancode_only: scancode_only(i), pause_after });
    }
    strokes
}

#[cfg(test)]
mod tests {
    use super::*;
    use ChordKey::*;

    fn keys(s: &str) -> Vec<ChordKey> {
        Chord::parse(s).unwrap().keys
    }

    #[test]
    fn parses_modifiers_letters_digits_and_function_keys() {
        assert_eq!(keys("Ctrl+Shift+Z"), [Ctrl, Shift, Char('Z')]);
        assert_eq!(keys("control+c"), [Ctrl, Char('C')]);
        assert_eq!(keys("Windows+D"), [Win, Char('D')]);
        assert_eq!(keys("Alt+F4"), [Alt, Function(4)]);
        assert_eq!(keys("Ctrl+1"), [Ctrl, Char('1')]);
        assert_eq!(keys("f12"), [Function(12)]);
    }

    #[test]
    fn parses_named_keys_and_their_aliases() {
        assert_eq!(keys("Ctrl+Return"), [Ctrl, Enter]);
        assert_eq!(keys("Esc"), [Escape]);
        assert_eq!(keys("Shift+Del"), [Shift, Delete]);
        assert_eq!(keys("PgDn"), [PageDown]);
        assert_eq!(keys("Mute"), [VolumeMute]);
        assert_eq!(keys("PlayPause"), [PlayPause]);
        assert_eq!(keys("Next"), [NextTrack]);
        assert_eq!(keys("PrevTrack"), [PrevTrack]);
        assert_eq!(keys("MediaStop"), [MediaStop]);
        assert_eq!(keys("Ctrl+Back"), [Ctrl, Backspace]);
    }

    #[test]
    fn parses_punctuation_by_symbol_or_name_and_folds_typographic_quotes() {
        assert_eq!(keys("Ctrl+'"), [Ctrl, Punct('\'')]);
        assert_eq!(keys("Ctrl+\u{2019}"), [Ctrl, Punct('\'')]);
        assert_eq!(keys("Ctrl+\u{201C}"), [Ctrl, Punct('\'')]);
        assert_eq!(keys("Ctrl+Slash"), [Ctrl, Punct('/')]);
        assert_eq!(keys("Ctrl+oem3"), [Ctrl, Punct('`')]);
        assert_eq!(keys("Ctrl+-"), [Ctrl, Punct('-')]);
    }

    #[test]
    fn whitespace_and_empty_segments_are_ignored() {
        assert_eq!(keys(" Ctrl + C "), [Ctrl, Char('C')]);
        assert_eq!(keys("Ctrl++C"), [Ctrl, Char('C')]);
    }

    #[test]
    fn bad_input_says_why() {
        assert_eq!(Chord::parse("").unwrap_err(), "Shortcut is empty.");
        assert_eq!(Chord::parse("   ").unwrap_err(), "Shortcut is empty.");
        assert_eq!(Chord::parse("+ +").unwrap_err(), "Shortcut has no keys.");
        assert_eq!(Chord::parse("Ctrl+Foo").unwrap_err(), "Unsupported key 'Foo'.");
        assert!(Chord::is_valid("Win+D"));
        // A dangling "+" is dropped, as in C#, so this is just Ctrl.
        assert_eq!(keys("Ctrl+"), [Ctrl]);
    }

    #[test]
    fn function_keys_outside_f1_to_f24_are_refused() {
        // C# turned F0 and F25+ into unrelated virtual keys; refusing is the safe reading.
        for bad in ["F0", "F25", "F99", "Fx", "F-1"] {
            assert!(Chord::parse(bad).is_err(), "{bad}");
        }
        assert!(Chord::parse("F24").is_ok());
        // A lone F is the letter.
        assert_eq!(keys("F"), [Char('F')]);
    }

    #[test]
    fn display_normalises_modifier_names_and_quotes_only() {
        assert_eq!(normalize_display("control + shift + \u{2019}"), "Ctrl+Shift+'");
        assert_eq!(normalize_display("windows+d"), "Win+d");
        assert_eq!(normalize_display("ctrl+enter"), "Ctrl+enter");
        assert_eq!(normalize_display("  "), "");
    }

    #[test]
    fn classification() {
        assert!(Ctrl.is_modifier() && !Char('A').is_modifier());
        assert!(PageUp.is_extended() && VolumeMute.is_extended() && !Enter.is_extended());
        assert!(Punct(',').is_punct() && !Space.is_punct());
    }

    #[test]
    fn plan_presses_in_order_and_releases_in_reverse() {
        let strokes = plan(&Chord::parse("Ctrl+Shift+Z").unwrap());
        let order: Vec<(ChordKey, bool)> = strokes.iter().map(|s| (s.key, s.release)).collect();
        assert_eq!(
            order,
            [
                (Ctrl, false),
                (Shift, false),
                (Char('Z'), false),
                (Char('Z'), true),
                (Shift, true),
                (Ctrl, true)
            ]
        );
    }

    #[test]
    fn plan_pauses_match_the_csharp_executor() {
        let ms = Duration::from_millis;
        let pauses = |s: &str| -> Vec<Duration> {
            plan(&Chord::parse(s).unwrap()).iter().map(|k| k.pause_after).collect()
        };
        // Win gets 25 ms, other keys 10; 10 after the last press; 5 between releases; none last.
        assert_eq!(pauses("Win+D"), [ms(25), ms(10), ms(5), Duration::ZERO]);
        assert_eq!(pauses("Ctrl+C"), [ms(10), ms(10), ms(5), Duration::ZERO]);
        assert_eq!(pauses("Enter"), [ms(10), Duration::ZERO]);
    }

    #[test]
    fn only_a_trailing_punctuation_key_goes_by_scan_code() {
        let by_scan = |s: &str| -> Vec<bool> {
            plan(&Chord::parse(s).unwrap()).iter().map(|k| k.scancode_only).collect()
        };
        assert_eq!(by_scan("Ctrl+'"), [false, true, true, false]);
        assert_eq!(by_scan("Ctrl+C"), [false, false, false, false]);
        // Punctuation that is not last keeps its normal key code.
        assert_eq!(by_scan("'+Ctrl"), [false, false, false, false]);
    }
}

//! `InputInjector` for Windows: chords and text through `SendInput`.
//! Port of `TryExecuteShortcut` in `Services/ActionExecutor.cs`; the order and pauses come
//! from `oasis_core::shortcut::plan`, which is shared with the other OSes.

use std::thread;
use std::time::Duration;

use oasis_core::shortcut::{plan, Chord, ChordKey, KeyStroke};
use oasis_win::input::{self, InputError, KeyEvent};

use crate::ports::{InputInjector, PlatformError, PlatformResult};

/// The Windows virtual-key code for a chord key. Left-hand modifiers, as in C#.
pub fn virtual_key(key: ChordKey) -> u16 {
    match key {
        ChordKey::Ctrl => 0xA2,
        ChordKey::Shift => 0xA0,
        ChordKey::Alt => 0xA4,
        ChordKey::Win => 0x5B,
        ChordKey::Char(c) => c as u16,
        ChordKey::Function(n) => 0x6F + u16::from(n),
        ChordKey::Punct(c) => match c {
            '.' => 0xBE,
            ',' => 0xBC,
            '/' => 0xBF,
            ';' => 0xBA,
            '\'' => 0xDE,
            '[' => 0xDB,
            ']' => 0xDD,
            '\\' => 0xDC,
            '-' => 0xBD,
            '=' => 0xBB,
            _ => 0xC0, // '`'
        },
        ChordKey::Space => 0x20,
        ChordKey::Enter => 0x0D,
        ChordKey::Escape => 0x1B,
        ChordKey::Tab => 0x09,
        ChordKey::CapsLock => 0x14,
        ChordKey::Backspace => 0x08,
        ChordKey::Delete => 0x2E,
        ChordKey::Insert => 0x2D,
        ChordKey::Up => 0x26,
        ChordKey::Down => 0x28,
        ChordKey::Left => 0x25,
        ChordKey::Right => 0x27,
        ChordKey::Home => 0x24,
        ChordKey::End => 0x23,
        ChordKey::PageUp => 0x21,
        ChordKey::PageDown => 0x22,
        ChordKey::VolumeUp => 0xAF,
        ChordKey::VolumeDown => 0xAE,
        ChordKey::VolumeMute => 0xAD,
        ChordKey::PlayPause => 0xB3,
        ChordKey::NextTrack => 0xB0,
        ChordKey::PrevTrack => 0xB1,
        ChordKey::MediaStop => 0xB2,
    }
}

fn key_event(stroke: &KeyStroke) -> KeyEvent {
    KeyEvent {
        vk: virtual_key(stroke.key),
        release: stroke.release,
        scancode_only: stroke.scancode_only,
        extended: stroke.key.is_extended(),
    }
}

fn to_platform(e: InputError) -> PlatformError {
    PlatformError::PermissionNeeded(e.to_string())
}

/// Run a chord through `send`, waiting with `pause` between events. If a press is refused the
/// keys already down are released, so a failed chord never leaves Ctrl or Win stuck.
fn run_chord(
    chord: &Chord,
    send: &mut dyn FnMut(KeyEvent) -> Result<(), InputError>,
    pause: &mut dyn FnMut(Duration),
) -> Result<(), InputError> {
    let strokes = plan(chord);
    let mut pressed: Vec<KeyEvent> = Vec::new();
    for stroke in &strokes {
        let event = key_event(stroke);
        if let Err(e) = send(event) {
            for down in pressed.iter().rev() {
                let _ = send(KeyEvent { release: true, ..*down });
            }
            return Err(e);
        }
        if stroke.release {
            pressed.retain(|p| p.vk != event.vk);
        } else {
            pressed.push(event);
        }
        if !stroke.pause_after.is_zero() {
            pause(stroke.pause_after);
        }
    }
    Ok(())
}

pub struct WindowsInput;

impl InputInjector for WindowsInput {
    fn send_shortcut(&mut self, chord: &Chord) -> PlatformResult<()> {
        run_chord(chord, &mut input::send_key, &mut thread::sleep).map_err(to_platform)
    }

    fn type_text(&mut self, text: &str) -> PlatformResult<()> {
        input::send_text(text).map_err(to_platform)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(s: &str) -> Chord {
        Chord::parse(s).unwrap()
    }

    #[test]
    fn virtual_keys_match_the_csharp_table() {
        assert_eq!(virtual_key(ChordKey::Ctrl), 0xA2);
        assert_eq!(virtual_key(ChordKey::Win), 0x5B);
        assert_eq!(virtual_key(ChordKey::Char('A')), 0x41);
        assert_eq!(virtual_key(ChordKey::Char('7')), 0x37);
        assert_eq!(virtual_key(ChordKey::Function(1)), 0x70);
        assert_eq!(virtual_key(ChordKey::Function(12)), 0x7B);
        assert_eq!(virtual_key(ChordKey::Punct('\'')), 0xDE);
        assert_eq!(virtual_key(ChordKey::Punct('`')), 0xC0);
        assert_eq!(virtual_key(ChordKey::PageDown), 0x22);
        assert_eq!(virtual_key(ChordKey::VolumeMute), 0xAD);
    }

    #[test]
    fn every_parsable_name_maps_to_a_nonzero_key() {
        for name in [
            "ctrl", "shift", "alt", "win", "a", "z", "0", "9", "f1", "f24", ".", ",", "/", ";", "'",
            "[", "]", "\\", "-", "=", "`", "space", "enter", "esc", "tab", "back", "delete", "up",
            "down", "left", "right", "home", "end", "pageup", "pagedown", "insert", "volumeup",
            "volumedown", "mute", "playpause", "next", "prev", "mediastop",
        ] {
            let key = chord(name).keys[0];
            assert_ne!(virtual_key(key), 0, "{name}");
        }
    }

    #[test]
    fn a_chord_is_sent_down_in_order_then_up_in_reverse_with_the_planned_pauses() {
        let mut sent: Vec<(u16, bool, bool)> = Vec::new();
        let mut pauses: Vec<Duration> = Vec::new();
        run_chord(
            &chord("Ctrl+Shift+Z"),
            &mut |e| {
                sent.push((e.vk, e.release, e.extended));
                Ok(())
            },
            &mut |d| pauses.push(d),
        )
        .unwrap();
        assert_eq!(
            sent,
            [
                (0xA2, false, false),
                (0xA0, false, false),
                (0x5A, false, false),
                (0x5A, true, false),
                (0xA0, true, false),
                (0xA2, true, false)
            ]
        );
        let ms = Duration::from_millis;
        assert_eq!(pauses, [ms(10), ms(10), ms(10), ms(5), ms(5)]);
    }

    #[test]
    fn arrows_are_sent_as_extended_keys_and_trailing_punctuation_by_scan_code() {
        let mut sent = Vec::new();
        run_chord(&chord("Ctrl+Left"), &mut |e| { sent.push(e); Ok(()) }, &mut |_| {}).unwrap();
        assert!(sent.iter().any(|e| e.vk == 0x25 && e.extended));

        let mut sent = Vec::new();
        run_chord(&chord("Ctrl+'"), &mut |e| { sent.push(e); Ok(()) }, &mut |_| {}).unwrap();
        assert!(sent.iter().filter(|e| e.vk == 0xDE).all(|e| e.scancode_only));
        assert!(sent.iter().filter(|e| e.vk == 0xA2).all(|e| !e.scancode_only));
    }

    #[test]
    fn a_refused_key_releases_whatever_is_already_down() {
        let blocked = InputError::Blocked { offered: 1, accepted: 0, code: 5 };
        let mut log: Vec<(u16, bool)> = Vec::new();
        let result = run_chord(
            &chord("Ctrl+Shift+Z"),
            &mut |e| {
                if e.vk == 0x5A && !e.release {
                    return Err(blocked);
                }
                log.push((e.vk, e.release));
                Ok(())
            },
            &mut |_| {},
        );
        assert_eq!(result, Err(blocked));
        // Ctrl and Shift went down, then both came back up, last pressed first.
        assert_eq!(log, [(0xA2, false), (0xA0, false), (0xA0, true), (0xA2, true)]);
    }

    #[test]
    fn a_blocked_send_becomes_a_permission_message_for_the_ui() {
        let e = to_platform(InputError::Blocked { offered: 2, accepted: 0, code: 5 });
        assert!(matches!(e, PlatformError::PermissionNeeded(m) if m.contains("administrator")));
    }

    /// F24 is bound to nothing; see the same note in `oasis-win`.
    #[test]
    fn the_real_injector_accepts_an_f24_chord() {
        WindowsInput.send_shortcut(&chord("F24")).unwrap();
    }
}

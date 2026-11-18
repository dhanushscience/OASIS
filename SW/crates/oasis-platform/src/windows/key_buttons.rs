//! The three buttons from the keyboard on Windows: the system-wide key listener (`oasis_win::keyboard`)
//! feeding the OS-neutral matcher (`oasis_core::input::KeyMatcher`). The listener exists only while the
//! keyboard input is switched on.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use oasis_core::input::{Binding, KeyMatcher, KeyVerdict};
use oasis_core::shortcut::ChordKey;
use oasis_win::keyboard::KeyboardHook;

use crate::ports::{ButtonSource, Wake};

/// The key a Windows virtual-key code stands for. The right-hand modifiers count as the left ones.
pub fn chord_key_from_vk(vk: u32) -> Option<ChordKey> {
    Some(match vk {
        0x10 | 0xA0 | 0xA1 => ChordKey::Shift,
        0x11 | 0xA2 | 0xA3 => ChordKey::Ctrl,
        0x12 | 0xA4 | 0xA5 => ChordKey::Alt,
        0x5B | 0x5C => ChordKey::Win,
        0x30..=0x39 | 0x41..=0x5A => ChordKey::Char(char::from(vk as u8)),
        0x70..=0x87 => ChordKey::Function((vk - 0x6F) as u8),
        0xBE => ChordKey::Punct('.'),
        0xBC => ChordKey::Punct(','),
        0xBF => ChordKey::Punct('/'),
        0xBA => ChordKey::Punct(';'),
        0xDE => ChordKey::Punct('\''),
        0xDB => ChordKey::Punct('['),
        0xDD => ChordKey::Punct(']'),
        0xDC => ChordKey::Punct('\\'),
        0xBD => ChordKey::Punct('-'),
        0xBB => ChordKey::Punct('='),
        0xC0 => ChordKey::Punct('`'),
        0x20 => ChordKey::Space,
        0x0D => ChordKey::Enter,
        0x1B => ChordKey::Escape,
        0x09 => ChordKey::Tab,
        0x14 => ChordKey::CapsLock,
        0x08 => ChordKey::Backspace,
        0x2E => ChordKey::Delete,
        0x2D => ChordKey::Insert,
        0x26 => ChordKey::Up,
        0x28 => ChordKey::Down,
        0x25 => ChordKey::Left,
        0x27 => ChordKey::Right,
        0x24 => ChordKey::Home,
        0x23 => ChordKey::End,
        0x21 => ChordKey::PageUp,
        0x22 => ChordKey::PageDown,
        0xAF => ChordKey::VolumeUp,
        0xAE => ChordKey::VolumeDown,
        0xAD => ChordKey::VolumeMute,
        0xB3 => ChordKey::PlayPause,
        0xB0 => ChordKey::NextTrack,
        0xB1 => ChordKey::PrevTrack,
        0xB2 => ChordKey::MediaStop,
        _ => return None,
    })
}

struct Shared {
    matcher: KeyMatcher,
    /// Keep a chosen shortcut from the program in front.
    exclusive: bool,
}

pub struct KeyboardButtons {
    shared: Arc<Mutex<Shared>>,
    edges: Sender<(u8, bool)>,
    inbox: Receiver<(u8, bool)>,
    hook: Option<KeyboardHook>,
    problem: Option<String>,
    wake: Wake,
}

impl Default for KeyboardButtons {
    fn default() -> KeyboardButtons {
        KeyboardButtons::new()
    }
}

impl KeyboardButtons {
    pub fn new() -> KeyboardButtons {
        let (edges, inbox) = channel();
        KeyboardButtons {
            shared: Arc::new(Mutex::new(Shared { matcher: KeyMatcher::default(), exclusive: true })),
            edges,
            inbox,
            hook: None,
            problem: None,
            wake: std::sync::Arc::new(|| {}),
        }
    }

    fn start(&mut self) {
        let shared = Arc::clone(&self.shared);
        let edges = self.edges.clone();
        let wake = std::sync::Arc::clone(&self.wake);
        let handler = Box::new(move |vk: u32, down: bool| -> bool {
            let Some(key) = chord_key_from_vk(vk) else { return false };
            let Ok(mut state) = shared.lock() else { return false };
            let is_modifier = matches!(key, ChordKey::Ctrl | ChordKey::Shift | ChordKey::Alt | ChordKey::Win);
            let (ctrl, shift, alt, win) = oasis_win::keyboard::modifiers_held();
            if down && !is_modifier {
                // Judge the key against the modifiers that are really down, not the ones this listener
                // last heard about: one release it missed would otherwise leave a bare key such as Tab
                // dead until the next time every modifier happened to be pressed and let go.
                state.matcher.sync_modifiers(ctrl, shift, alt, win);
                // The window's own text boxes: a shortcut being recorded takes every key; a box being typed
                // in takes plain printable keys (so a tilde can be typed), but Tab, Caps Lock, F-keys and
                // chords with Ctrl, Alt or Win still run their button, and are kept from the box as
                // anywhere else. Only a press is let through: modifiers and releases always reach the
                // matcher, so it never loses track of what is held.
                match crate::ports::window_typing() {
                    crate::ports::RECORDING => return false,
                    crate::ports::TYPING
                        if !(ctrl || alt || win) && matches!(key, ChordKey::Char(_) | ChordKey::Punct(_) | ChordKey::Space) =>
                    {
                        return false
                    }
                    _ => {}
                }
            }
            let verdicts = state.matcher.key(key, down);
            for verdict in &verdicts {
                match verdict {
                    KeyVerdict::Press(b) => drop(edges.send((*b, true))),
                    KeyVerdict::Release(b) => drop(edges.send((*b, false))),
                    _ => {}
                }
            }
            if verdicts.iter().any(|v| matches!(v, KeyVerdict::Press(_) | KeyVerdict::Release(_))) {
                wake();
            }
            // A modifier's verdicts end in Pass, so it is never kept back; our own keys always are.
            state.exclusive && !matches!(verdicts.last(), None | Some(KeyVerdict::Pass))
        });
        match KeyboardHook::start(handler) {
            Ok(hook) => {
                self.hook = Some(hook);
                self.problem = None;
            }
            Err(e) => self.problem = Some(format!("The keyboard could not be listened to: {e}")),
        }
    }
}

impl ButtonSource for KeyboardButtons {
    fn set_wake(&mut self, wake: Wake) {
        crate::ports::set_window_wake(std::sync::Arc::clone(&wake));
        self.wake = wake;
    }

    fn configure(&mut self, listen: bool, chords: [String; 3], exclusive: bool) {
        let bindings = chords.map(|text| Binding::parse(&text));
        let released = match self.shared.lock() {
            Ok(mut state) => {
                state.exclusive = exclusive;
                if listen { state.matcher.set_bindings(bindings) } else { state.matcher.release_all() }
            }
            Err(_) => Vec::new(),
        };
        for verdict in released {
            if let KeyVerdict::Release(b) = verdict {
                let _ = self.edges.send((b, false));
                (self.wake)();
            }
        }
        if listen && self.hook.is_none() {
            self.start();
        } else if !listen {
            self.hook = None; // dropping it stops the listener
            self.problem = None;
        }
    }

    fn drain(&mut self) -> Vec<(u8, bool)> {
        // the key listener's edges, then the ones the window itself matched (see `ports::press_from_window`)
        let mut edges: Vec<(u8, bool)> = self.inbox.try_iter().collect();
        edges.extend(crate::ports::take_window_buttons());
        edges
    }

    fn problem(&self) -> Option<String> {
        self.problem.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows::input::virtual_key;

    /// Needs an interactive desktop: `cargo test -p oasis-platform key_buttons -- --ignored`.
    #[test]
    #[ignore = "needs an interactive Windows desktop"]
    fn a_shortcut_on_the_keyboard_becomes_a_button_edge_and_is_kept_from_other_programs() {
        use oasis_win::keyboard::send_foreign_key;
        use std::time::{Duration, Instant};
        // F24, F23 and F22: no real keyboard has them, so the test disturbs nobody typing
        let mut source = KeyboardButtons::new();
        source.configure(true, ["F24".into(), "F23".into(), String::new()], true);
        assert!(source.problem().is_none());
        let wait_for = |source: &mut KeyboardButtons, count: usize| {
            let mut edges = Vec::new();
            let end = Instant::now() + Duration::from_secs(3);
            while edges.len() < count && Instant::now() < end {
                edges.extend(source.drain());
                std::thread::sleep(Duration::from_millis(10));
            }
            edges
        };
        send_foreign_key(0x87, false);
        send_foreign_key(0x87, true);
        send_foreign_key(0x86, false);
        send_foreign_key(0x86, true);
        assert_eq!(wait_for(&mut source, 4), [(0, true), (0, false), (1, true), (1, false)]);
        // a key that is not a shortcut makes no edge
        send_foreign_key(0x85, false);
        send_foreign_key(0x85, true);
        std::thread::sleep(Duration::from_millis(300));
        assert!(source.drain().is_empty());
        // switched off: the listener is gone and a held button is let go
        source.configure(false, [String::new(), String::new(), String::new()], true);
        assert!(source.hook.is_none());
    }

    #[test]
    fn every_key_the_chords_can_name_comes_back_from_its_code() {
        let mut keys = vec![ChordKey::Ctrl, ChordKey::Shift, ChordKey::Alt, ChordKey::Win];
        keys.extend(('A'..='Z').chain('0'..='9').map(ChordKey::Char));
        keys.extend((1..=24).map(ChordKey::Function));
        keys.extend("., /;'[]\\-=`".chars().filter(|c| *c != ' ').map(ChordKey::Punct));
        keys.extend([
            ChordKey::Space, ChordKey::Enter, ChordKey::Escape, ChordKey::Tab, ChordKey::CapsLock, ChordKey::Backspace, ChordKey::Delete,
            ChordKey::Insert, ChordKey::Up, ChordKey::Down, ChordKey::Left, ChordKey::Right, ChordKey::Home,
            ChordKey::End, ChordKey::PageUp, ChordKey::PageDown, ChordKey::VolumeUp, ChordKey::VolumeDown,
            ChordKey::VolumeMute, ChordKey::PlayPause, ChordKey::NextTrack, ChordKey::PrevTrack, ChordKey::MediaStop,
        ]);
        for key in keys {
            assert_eq!(chord_key_from_vk(u32::from(virtual_key(key))), Some(key), "{key:?}");
        }
        assert_eq!(chord_key_from_vk(0xA3), Some(ChordKey::Ctrl), "right Ctrl is Ctrl");
        assert_eq!(chord_key_from_vk(0xFF), None);
    }
}

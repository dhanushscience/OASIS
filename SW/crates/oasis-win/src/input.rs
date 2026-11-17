//! Keyboard injection with `SendInput`. Port of `SendKeyEvent` / `CreateKeyInput` in
//! `Services/ActionExecutor.cs`, plus typing text by Unicode code unit.
//!
//! Windows drops input aimed at a window running at a higher privilege than OASIS (UIPI). That
//! shows up as `SendInput` accepting fewer events than offered; it is reported as
//! [`InputError::Blocked`].

use std::fmt;
use std::mem::size_of;

use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::UI::WindowsAndMessaging::SetCursorPos;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
    MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEINPUT,
    KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC, VK_RETURN,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputError {
    /// Windows accepted `accepted` of `offered` events. Usually the focused window is elevated.
    Blocked { offered: u32, accepted: u32, code: u32 },
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let InputError::Blocked { offered, accepted, code } = self;
        write!(
            f,
            "Windows did not accept the key presses ({accepted} of {offered}, error {code}). \
             The window in front may be running as administrator."
        )
    }
}

impl std::error::Error for InputError {}

/// One physical key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    /// Windows virtual-key code.
    pub vk: u16,
    pub release: bool,
    /// Send the scan code only (`wVk` = 0), so the focused window's layout picks the character.
    pub scancode_only: bool,
    /// Carries the "extended key" flag (arrows, Home/End, Insert/Delete, volume).
    pub extended: bool,
}

pub fn send_key(event: KeyEvent) -> Result<(), InputError> {
    send(&[key_input(event)])
}

/// Type `text` as characters, whatever the keyboard layout. A line break is sent as Enter.
pub fn send_text(text: &str) -> Result<(), InputError> {
    let inputs: Vec<INPUT> = text_events(text).into_iter().map(text_input).collect();
    if inputs.is_empty() {
        return Ok(());
    }
    send(&inputs)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Move the pointer to screen position `(x, y)` and press and release `button`. Port of
/// `TryExecuteMouseClick`: move, wait 50 ms for the pointer to land, then click in place.
#[allow(unsafe_code)]
pub fn click_at(x: i32, y: i32, button: MouseButton) -> Result<(), InputError> {
    // SAFETY: plain call.
    if unsafe { SetCursorPos(x, y) } == 0 {
        // SAFETY: no preconditions.
        return Err(InputError::Blocked { offered: 0, accepted: 0, code: unsafe { GetLastError() } });
    }
    std::thread::sleep(std::time::Duration::from_millis(50));
    let (down, up) = match button {
        MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
        MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
        MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
    };
    let mouse = |flags| INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 },
        },
    };
    send(&[mouse(down), mouse(up)])
}

/// What typing `text` takes, in order. Pure, so it can be tested without sending anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextEvent {
    Unit { unit: u16, release: bool },
    Enter { release: bool },
}

pub(crate) fn text_events(text: &str) -> Vec<TextEvent> {
    let mut events = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' || c == '\n' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            events.push(TextEvent::Enter { release: false });
            events.push(TextEvent::Enter { release: true });
            continue;
        }
        let mut units = [0u16; 2];
        for &unit in c.encode_utf16(&mut units).iter() {
            events.push(TextEvent::Unit { unit, release: false });
            events.push(TextEvent::Unit { unit, release: true });
        }
    }
    events
}

/// Stamped on every key OASIS sends, so the key listener can tell them from other programs' keys (a
/// remapping tool, an on-screen keyboard) and never answers itself.
pub const OASIS_KEY_TAG: usize = 0x4F41_5349;

fn keyboard(ki: KEYBDINPUT) -> INPUT {
    INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki } }
}

fn key_input(event: KeyEvent) -> INPUT {
    // SAFETY: plain call.
    #[allow(unsafe_code)]
    let scan = unsafe { MapVirtualKeyW(u32::from(event.vk), MAPVK_VK_TO_VSC) } as u16;
    let mut flags = if event.release { KEYEVENTF_KEYUP } else { 0 };
    let ki = if event.scancode_only {
        flags |= KEYEVENTF_SCANCODE;
        KEYBDINPUT { wVk: 0, wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: OASIS_KEY_TAG }
    } else {
        if event.extended {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        KEYBDINPUT { wVk: event.vk, wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: OASIS_KEY_TAG }
    };
    keyboard(ki)
}

fn text_input(event: TextEvent) -> INPUT {
    let up = |release: bool, base: u32| if release { base | KEYEVENTF_KEYUP } else { base };
    keyboard(match event {
        TextEvent::Unit { unit, release } => KEYBDINPUT {
            wVk: 0,
            wScan: unit,
            dwFlags: up(release, KEYEVENTF_UNICODE),
            time: 0,
            dwExtraInfo: OASIS_KEY_TAG,
        },
        TextEvent::Enter { release } => KEYBDINPUT {
            wVk: VK_RETURN,
            wScan: 0,
            dwFlags: up(release, 0),
            time: 0,
            dwExtraInfo: OASIS_KEY_TAG,
        },
    })
}

#[allow(unsafe_code)]
fn send(inputs: &[INPUT]) -> Result<(), InputError> {
    let offered = inputs.len() as u32;
    // SAFETY: `inputs` is a valid array of `offered` INPUT structs and the size passed is that
    // of one INPUT, as the API requires.
    let accepted = unsafe { SendInput(offered, inputs.as_ptr(), size_of::<INPUT>() as i32) };
    if accepted == offered {
        Ok(())
    } else {
        // SAFETY: no preconditions.
        Err(InputError::Blocked { offered, accepted, code: unsafe { GetLastError() } })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VK_F24: u16 = 0x87;

    #[test]
    fn text_becomes_one_down_and_one_up_per_utf16_unit() {
        let e = text_events("a");
        assert_eq!(
            e,
            [
                TextEvent::Unit { unit: 'a' as u16, release: false },
                TextEvent::Unit { unit: 'a' as u16, release: true }
            ]
        );
        // U+1F600 is two UTF-16 units: four events.
        assert_eq!(text_events("\u{1F600}").len(), 4);
        assert!(text_events("").is_empty());
    }

    #[test]
    fn line_breaks_become_one_enter_each() {
        let enters = |s: &str| {
            text_events(s)
                .iter()
                .filter(|e| matches!(e, TextEvent::Enter { release: false }))
                .count()
        };
        assert_eq!(enters("a\nb"), 1);
        assert_eq!(enters("a\r\nb"), 1);
        assert_eq!(enters("a\rb"), 1);
        assert_eq!(enters("a\n\nb"), 2);
    }

    #[test]
    fn key_inputs_carry_the_right_flags() {
        let ki = |event| -> KEYBDINPUT {
            let input = key_input(event);
            // SAFETY: `key_input` always builds the keyboard variant.
            #[allow(unsafe_code)]
            unsafe {
                input.Anonymous.ki
            }
        };
        let plain = KeyEvent { vk: 0x41, release: false, scancode_only: false, extended: false };
        assert_eq!(ki(plain).dwFlags, 0);
        assert_eq!(ki(plain).wVk, 0x41);
        assert_ne!(ki(plain).wScan, 0);
        assert_eq!(ki(KeyEvent { release: true, ..plain }).dwFlags, KEYEVENTF_KEYUP);
        assert_eq!(ki(KeyEvent { extended: true, ..plain }).dwFlags, KEYEVENTF_EXTENDEDKEY);

        let scan = ki(KeyEvent { scancode_only: true, release: true, ..plain });
        assert_eq!(scan.wVk, 0);
        assert_eq!(scan.dwFlags, KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP);
    }

    /// F24 is bound to nothing, so pressing it is safe on a live desktop. The only thing this
    /// proves is that `SendInput` accepts our structures. Nothing else is ever sent in tests.
    #[test]
    fn windows_accepts_a_press_and_release_of_f24() {
        let key = KeyEvent { vk: VK_F24, release: false, scancode_only: false, extended: false };
        send_key(key).unwrap();
        send_key(KeyEvent { release: true, ..key }).unwrap();
    }
}

//! The traits each OS backend implements. One trait per thing the C# app did through a
//! Windows API, named after what it does, not how.
//!
//! | Trait | C# origin | Windows | macOS | Linux |
//! |---|---|---|---|---|
//! | [`PadTransport`] | `Services/BleManager.cs` | WinRT GATT (btleplug) | CoreBluetooth (btleplug) | BlueZ over D-Bus (btleplug) |
//! | [`ForegroundApp`] | `Services/ContextTracker.cs`, `Services/Context/*` | `GetForegroundWindow` + UI Automation for the browser URL | `NSWorkspace.frontmostApplication` + Accessibility API | X11 `_NET_ACTIVE_WINDOW`; Wayland needs a compositor extension (GNOME/KDE) |
//! | [`InputInjector`] | `Services/ActionExecutor.cs` (`SendInput`) | `SendInput` | `CGEventPost` (needs Accessibility permission) | `uinput` (Wayland + X11) or XTest (X11) |
//! | [`SecretStore`] | `Platform/CredentialStore.cs` (DPAPI) | DPAPI / Credential Manager | Keychain | Secret Service (GNOME Keyring / KWallet) |
//! | [`MediaSessions`] | `Services/System/*` (GSMTC) | `GlobalSystemMediaTransportControls` | Now Playing (MediaRemote) | MPRIS over D-Bus |
//! | [`Microphone`] | `Services/Voice/MicrophoneCapture.cs` (NAudio) | WASAPI (cpal) | CoreAudio (cpal) | PipeWire/PulseAudio/ALSA (cpal) |
//! | [`Autostart`] | `Services/AutostartService.cs` | `HKCU\…\Run` / scheduled task when elevated | LaunchAgent plist | `~/.config/autostart/*.desktop` |
//!
//! Every method returns `Result<_, PlatformError>`. `PlatformError::Unsupported` carries a
//! sentence the UI can show as-is ("Reading the active window needs the OASIS GNOME extension
//! on Wayland"), which is how "missing capability → honest status" works across OSes.

use std::fmt;
use std::time::Duration;

use oasis_core::capability::{Capability, CapabilityState};

use crate::Os;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformError {
    /// This OS (or this desktop session) cannot do it; the text says why and what to do.
    Unsupported(String),
    /// The user has not granted a permission yet (macOS Accessibility, Bluetooth, microphone).
    PermissionNeeded(String),
    /// The device or service is not there right now (pad off, no mic, no keyring).
    Unavailable(String),
    Io(String),
}

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlatformError::Unsupported(s)
            | PlatformError::PermissionNeeded(s)
            | PlatformError::Unavailable(s)
            | PlatformError::Io(s) => f.write_str(s),
        }
    }
}

impl std::error::Error for PlatformError {}

impl PlatformError {
    /// The capability state to show when this error is the reason a feature cannot run.
    pub fn as_capability_state(&self) -> CapabilityState {
        CapabilityState::missing(self.to_string())
    }
}

pub type PlatformResult<T> = Result<T, PlatformError>;

/// A discovered pad.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PadInfo {
    pub id: String,
    pub name: String,
    pub rssi: Option<i16>,
}

/// What the OASIS window's own keyboard focus is, for the keyboard buttons: `NOT_TYPING`; `TYPING` (a text
/// box: letters, digits and punctuation are typing, so they are not taken for a button, but Tab, Caps
/// Lock, function keys and the like still are); `RECORDING` (a shortcut is being recorded: every key goes
/// to the box).
pub const NOT_TYPING: u8 = 0;
pub const TYPING: u8 = 1;
pub const RECORDING: u8 = 2;
static WINDOW_TYPING: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(NOT_TYPING);

pub fn set_window_typing(mode: u8) {
    WINDOW_TYPING.store(mode, std::sync::atomic::Ordering::Relaxed);
}

pub fn window_typing() -> u8 {
    WINDOW_TYPING.load(std::sync::atomic::Ordering::Relaxed)
}

/// Buttons pressed from the OASIS window itself: Windows does not hand the system-wide key listener the
/// keys typed while the window is in front, so the window matches its own keys and queues the press here.
static WINDOW_BUTTONS: std::sync::Mutex<Vec<(u8, bool)>> = std::sync::Mutex::new(Vec::new());
/// Called after the window queues a press, so whoever drains the queue need not poll it.
static WINDOW_WAKE: std::sync::Mutex<Option<Wake>> = std::sync::Mutex::new(None);

/// Tells the owner of a [`ButtonSource`] that edges are waiting.
pub type Wake = std::sync::Arc<dyn Fn() + Send + Sync>;

/// A button (0 to 2) the window pressed or let go. Anything else is ignored.
pub fn press_from_window(button: u8, down: bool) {
    if button < 3 {
        if let Ok(mut queue) = WINDOW_BUTTONS.lock() {
            queue.push((button, down));
        }
        if let Some(wake) = WINDOW_WAKE.lock().ok().and_then(|w| w.clone()) {
            wake();
        }
    }
}

/// Who to wake when the window queues a press.
pub fn set_window_wake(wake: Wake) {
    if let Ok(mut slot) = WINDOW_WAKE.lock() {
        *slot = Some(wake);
    }
}

/// The edges queued by `press_from_window` since the last call.
pub fn take_window_buttons() -> Vec<(u8, bool)> {
    WINDOW_BUTTONS.lock().map(|mut queue| std::mem::take(&mut *queue)).unwrap_or_default()
}

/// BLE link to the pad over the Nordic UART service (`oasis_protocol::gatt`).
///
/// Notifications are delivered as raw bytes; feed them to `oasis_app::PadLink`.
/// Another way of pressing the three buttons besides the pad (the keyboard, for now). The engine tells it
/// what to listen for, and asks it for the buttons that went down or up whenever it calls `wake`.
pub trait ButtonSource: Send {
    /// Called once before `configure`: the source calls `wake` whenever new edges are waiting, so
    /// the engine can sleep until then instead of asking all the time.
    fn set_wake(&mut self, wake: Wake);
    /// `listen` switches it on or off. `chords` is the shortcut for each button (empty = none);
    /// `exclusive` keeps a chosen shortcut from the program in front.
    fn configure(&mut self, listen: bool, chords: [String; 3], exclusive: bool);
    /// Button edges since the last call: (button 0 to 2, went down).
    fn drain(&mut self) -> Vec<(u8, bool)>;
    /// Why it cannot work, if it cannot (a sentence for the person).
    fn problem(&self) -> Option<String> {
        None
    }
}

pub trait PadTransport: Send {
    /// Scan for advertising OASIS pads (`oasis_protocol::gatt::is_oasis_ble_name`).
    fn scan(&mut self, timeout: Duration) -> PlatformResult<Vec<PadInfo>>;
    fn connect(&mut self, pad: &PadInfo) -> PlatformResult<()>;
    fn disconnect(&mut self);
    fn is_connected(&self) -> bool;
    /// Write one already-formatted command (`HostCommand::to_wire`).
    fn write(&mut self, wire: &str) -> PlatformResult<()>;
    /// Wait up to `timeout` for the next TX notification. `Ok(None)` on timeout.
    fn next_notification(&mut self, timeout: Duration) -> PlatformResult<Option<Vec<u8>>>;
}

/// What the user is looking at — drives profile switching.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ForegroundInfo {
    /// Lower-case process name without extension ("chrome", "code", "spotify").
    pub process: String,
    pub window_title: String,
    /// Address-bar URL when the foreground app is a browser and the OS lets us read it.
    pub browser_url: Option<String>,
    pub pid: Option<u32>,
    /// Which part of the program has focus (an IDE's `terminal`, `editor`, `agent`, …), when a
    /// profile uses panels and the OS could be asked. Empty otherwise.
    pub panel_id: String,
    pub panel_label: String,
    pub app_family: String,
    /// `none`, `low`, `medium` or `high`; empty when no panel was read.
    pub detection_confidence: String,
    pub focus_class_name: String,
    pub ancestor_hint: String,
}

pub trait ForegroundApp: Send {
    fn current(&mut self) -> PlatformResult<ForegroundInfo>;
}

/// Sends keystrokes to the focused window.
///
/// Media keys (play/pause, next, previous) are not here yet: the C# app sends them from the
/// action executor, and they come with that port.
pub trait InputInjector: Send {
    /// Press the chord's keys in order and release them in reverse, pausing as
    /// `oasis_core::shortcut::plan` says. Blocks for those pauses (tens of milliseconds).
    fn send_shortcut(&mut self, chord: &oasis_core::shortcut::Chord) -> PlatformResult<()>;
    /// Type `text` as characters, whatever the keyboard layout.
    fn type_text(&mut self, text: &str) -> PlatformResult<()>;
}

/// Per-user secret storage. Secrets never appear in `profiles.json` (C# GATE-3).
pub trait SecretStore: Send + Sync {
    fn get(&self, key: &str) -> PlatformResult<Option<String>>;
    fn set(&self, key: &str, value: &str) -> PlatformResult<()>;
    fn clear(&self, key: &str) -> PlatformResult<()>;
}

/// OS media sessions (what is playing, play/pause), used for media ducking.
pub trait MediaSessions: Send {
    fn playback(&mut self) -> PlatformResult<oasis_core::voice::MediaPlayback>;
    fn pause_all_playing(&mut self) -> PlatformResult<Vec<String>>;
    fn resume(&mut self, session_ids: &[String]) -> PlatformResult<()>;
}

/// Receives captured audio, called on the audio thread. Must not block.
pub type SampleSink = Box<dyn FnMut(&[f32]) + Send>;

/// 16 kHz mono PCM capture for speech-to-text.
pub trait Microphone: Send {
    fn start(&mut self, on_samples: SampleSink) -> PlatformResult<()>;
    fn stop(&mut self);
}

pub trait Autostart {
    fn is_enabled(&self) -> PlatformResult<bool>;
    fn set_enabled(&self, enabled: bool) -> PlatformResult<()>;
}

/// The status table every OS module reports until its backend lands. Shared so the three OS
/// modules cannot drift, and so it compiles (and is tested) on every OS.
pub fn not_built_yet(os: Os) -> Vec<(Capability, CapabilityState)> {
    let reason = |what: &str, how: &str| {
        CapabilityState::missing(format!(
            "{what} on {} is not built yet in the Rust app ({how}).",
            os.name()
        ))
    };
    let (ble, fg) = match os {
        Os::Windows => ("WinRT Bluetooth", "Win32 foreground window + UI Automation"),
        Os::MacOs => ("CoreBluetooth", "NSWorkspace + Accessibility"),
        Os::Linux => (
            "BlueZ",
            "X11 active window; Wayland needs a compositor extension",
        ),
        Os::Other => ("no backend", "no backend"),
    };
    vec![
        (Capability::Pad, reason("The OASIS interface connection", ble)),
        (
            Capability::BrowserBridge,
            reason("Foreground app and browser tracking", fg),
        ),
        (
            Capability::SpeechToText,
            reason(
                "Microphone capture and speech-to-text",
                "cpal + sherpa-onnx",
            ),
        ),
        (Capability::TextToSpeech, reason("Text-to-speech", "Piper")),
        (
            Capability::LocalPlanner,
            reason("The local planner", "llama.cpp"),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_queues_button_edges_and_only_for_buttons_that_exist() {
        take_window_buttons();
        press_from_window(1, true);
        press_from_window(1, false);
        press_from_window(3, true); // there is no fourth button
        assert_eq!(take_window_buttons(), [(1, true), (1, false)]);
        assert!(take_window_buttons().is_empty());
    }

    #[test]
    fn every_status_is_an_honest_missing_with_the_os_name() {
        for os in [Os::Windows, Os::MacOs, Os::Linux] {
            for (_, state) in not_built_yet(os) {
                let CapabilityState::Missing { reason, .. } = state else {
                    panic!()
                };
                assert!(reason.contains(os.name()), "{reason}");
            }
        }
    }

    #[test]
    fn errors_show_their_text() {
        let e = PlatformError::PermissionNeeded("Allow OASIS in Accessibility settings.".into());
        assert_eq!(e.to_string(), "Allow OASIS in Accessibility settings.");
        assert!(matches!(
            e.as_capability_state(),
            CapabilityState::Missing { .. }
        ));
    }
}

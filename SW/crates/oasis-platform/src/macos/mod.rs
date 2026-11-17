//! macOS backends. No C# equivalent exists; these are new.
//!
//! | Trait | Backend to build | Permission the user must grant |
//! |---|---|---|
//! | `PadTransport` | btleplug (CoreBluetooth) | Bluetooth (Info.plist `NSBluetoothAlwaysUsageDescription`) |
//! | `ForegroundApp` | `NSWorkspace.frontmostApplication`; browser URL via Accessibility (`AXURL`) or AppleScript for Safari/Chrome | Accessibility; Automation per browser |
//! | `InputInjector` | `CGEventCreateKeyboardEvent` + `CGEventPost`; media keys via `NSEvent` system-defined events | Accessibility |
//! | `SecretStore` | Keychain (`keyring` crate) | none |
//! | `MediaSessions` | MediaRemote "Now Playing" (private framework — may break on OS updates; fall back to `Unknown`) | none |
//! | `Microphone` | cpal (CoreAudio) | Microphone (`NSMicrophoneUsageDescription`) |
//! | `Autostart` | `~/Library/LaunchAgents/com.projectoasis.oasis.plist` | none |
//!
//! Note: "show desktop" maps to Mission Control's Show Desktop (F11 / `⌘ F3`), and the
//! Windows-key chords in user profiles need a Command-key translation table.

use oasis_core::capability::{Capability, CapabilityState};

pub fn backend_status() -> Vec<(Capability, CapabilityState)> {
    crate::ports::not_built_yet(crate::Os::MacOs)
}

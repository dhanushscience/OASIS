//! Windows backends. Same OS as the C# app, so each item has a direct C# source.
//!
//! | Trait | Where it is built | C# file ported from |
//! |---|---|---|
//! | `PadTransport` | `oasis-ble` (btleplug, WinRT GATT), reached through [`pad_connector`](crate::pad_connector) | `Services/BleManager.cs`, `Services/BleDeviceTrust.cs` |
//! | `ForegroundApp` | [`foreground`], over `oasis-win` (`GetForegroundWindow`, UI Automation) | `Services/ContextTracker.cs`, `Services/Context/*` |
//! | `InputInjector` | [`input`], [`key_buttons`], over `oasis-win` (`SendInput`, keyboard hook) | `Services/ActionExecutor.cs`, `Services/ShortcutChord.cs` |
//! | `SecretStore` | [`dpapi`] (`oasis-dpapi`, same file layout as `secrets/`) | `Platform/CredentialStore.cs` |
//! | `MediaSessions` | `oasis-win::media` | `Services/System/*` |
//! | `Microphone` | `oasis-voice` (cpal), not in this crate | `Services/Voice/MicrophoneCapture.cs` |
//! | `Autostart` | [`startup`] (Run key, scheduled task when elevated) | `Services/AutostartService.cs`, `Services/Elevation/*` |

use oasis_core::capability::{Capability, CapabilityState};

pub mod actions;
pub mod app_discovery;
pub mod dpapi;
pub mod foreground;
pub mod input;
pub mod key_buttons;
pub mod pc_check;
pub mod startup;
pub mod window_commands;

/// The pad link, foreground/browser tracking and speech-to-text are built on Windows (`oasis-ble`,
/// this module, `oasis-voice`); text-to-speech and the local planner are not.
pub fn backend_status() -> Vec<(Capability, CapabilityState)> {
    crate::ports::not_built_yet(crate::Os::Windows)
        .into_iter()
        .map(|(cap, state)| match cap {
            Capability::Pad | Capability::BrowserBridge | Capability::SpeechToText => {
                (cap, CapabilityState::Available)
            }
            _ => (cap, state),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_reports_what_is_built() {
        let status = backend_status();
        let state = |c: Capability| status.iter().find(|(k, _)| *k == c).map(|(_, s)| s.clone()).unwrap();
        assert_eq!(state(Capability::Pad), CapabilityState::Available);
        assert_eq!(state(Capability::SpeechToText), CapabilityState::Available);
        assert!(matches!(state(Capability::TextToSpeech), CapabilityState::Missing { .. }));
    }
}

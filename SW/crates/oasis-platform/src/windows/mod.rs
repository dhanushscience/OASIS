//! Windows backends. Same OS as the C# app, so each item has a direct C# source to port.
//!
//! | Trait | Backend to build | C# file to port from |
//! |---|---|---|
//! | `PadTransport` | btleplug (WinRT GATT) | `Services/BleManager.cs`, `Services/BleDeviceTrust.cs` |
//! | `ForegroundApp` | `windows` crate: `GetForegroundWindow`, `QueryFullProcessImageNameW`, UI Automation | `Services/ContextTracker.cs`, `Services/Context/*` |
//! | `InputInjector` | `SendInput` | `Services/ActionExecutor.cs`, `Services/ShortcutChord.cs` |
//! | `SecretStore` | DPAPI (`CryptProtectData`), same file layout as `secrets/` so existing secrets keep working | `Platform/CredentialStore.cs` |
//! | `MediaSessions` | `GlobalSystemMediaTransportControlsSessionManager` | `Services/System/*` |
//! | `Microphone` | cpal (WASAPI) | `Services/Voice/MicrophoneCapture.cs` |
//! | `Autostart` | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`; scheduled task `ProjectOASIS\RunAsAdministrator` when elevated | `Services/AutostartService.cs`, `Services/Elevation/*` |

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

pub fn backend_status() -> Vec<(Capability, CapabilityState)> {
    crate::ports::not_built_yet(crate::Os::Windows)
}

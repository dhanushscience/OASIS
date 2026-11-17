//! Linux backends. No C# equivalent exists; these are new.
//!
//! | Trait | Backend to build | Caveat |
//! |---|---|---|
//! | `PadTransport` | btleplug (BlueZ over D-Bus) | User must be allowed to use BlueZ (usually the default) |
//! | `ForegroundApp` | X11: `_NET_ACTIVE_WINDOW` + `_NET_WM_PID` (x11rb). Wayland: no standard API — ship a small GNOME Shell extension / KWin script that reports the focused window over D-Bus | Without it, profile switching falls back to the default profile and says so |
//! | `InputInjector` | `/dev/uinput` virtual keyboard (works on X11 and Wayland) | Needs a udev rule granting the user access to `/dev/uinput` |
//! | `SecretStore` | Secret Service over D-Bus (GNOME Keyring, KWallet) | Headless sessions may have no keyring → honest Missing |
//! | `MediaSessions` | MPRIS (`org.mpris.MediaPlayer2.*`) over D-Bus | Browsers expose MPRIS for YouTube tabs, which is enough for ducking |
//! | `Microphone` | cpal (PipeWire / PulseAudio / ALSA) | none |
//! | `Autostart` | `~/.config/autostart/oasis.desktop` | none |

use oasis_core::capability::{Capability, CapabilityState};

pub fn backend_status() -> Vec<(Capability, CapabilityState)> {
    crate::ports::not_built_yet(crate::Os::Linux)
}

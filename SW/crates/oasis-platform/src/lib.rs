//! # oasis-platform
//!
//! Everything that touches the operating system. Port of `SW/Platform/*` and
//! `SW/Oasis.Adapters.Windows`, generalised to Windows, macOS and Linux.
//!
//! Two kinds of thing live here:
//!
//! 1. **Portable adapters written with std only** — they already work on all three OSes:
//!    [`paths`] (where data, models and logs go), [`atomic_file`] (crash-safe writes),
//!    [`crash_log`] (a logger that never throws).
//! 2. **Ports** ([`ports`]) — one trait per OS feature (BLE pad link, foreground app, key
//!    injection, secrets, media sessions, microphone, autostart). Each OS module
//!    ([`windows`], [`macos`], [`linux`]) says where each trait is (or will be) implemented and
//!    reports today's status honestly through [`backend_status`].
//!
//! The rest of the app talks only to the traits, so a missing backend shows up as a
//! `CapabilityState::Missing` with a reason — never as a crash.

pub mod atomic_file;
pub mod browser_bridge;
pub mod crash_log;
pub mod extension_install;
pub mod foreground;
pub mod integrity;
pub mod pad_connector;
pub mod paths;
pub mod ports;
pub mod profile_icons;
pub mod profile_store;
pub mod secrets;
pub mod security_log;
pub mod trusted_device;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

use oasis_core::capability::{Capability, CapabilityState};

/// The OS this build targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    MacOs,
    Linux,
    Other,
}

impl Os {
    pub const fn current() -> Os {
        if cfg!(target_os = "windows") {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::MacOs
        } else if cfg!(target_os = "linux") {
            Os::Linux
        } else {
            Os::Other
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Os::Windows => "Windows",
            Os::MacOs => "macOS",
            Os::Linux => "Linux",
            Os::Other => "this OS",
        }
    }
}

/// What each OS-dependent capability can do on this build, today.
pub fn backend_status() -> Vec<(Capability, CapabilityState)> {
    #[cfg(target_os = "windows")]
    return windows::backend_status();
    #[cfg(target_os = "macos")]
    return macos::backend_status();
    #[cfg(target_os = "linux")]
    return linux::backend_status();
    #[allow(unreachable_code)]
    ports::not_built_yet(Os::current())
}

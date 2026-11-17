//! # oasis-win
//!
//! Safe wrappers over the Win32 calls OASIS needs. Empty on other OSes.
//!
//! | Module | Role | C# origin |
//! |---|---|---|
//! | [`window`] | Foreground window, its title and process | `Services/ContextTracker.cs` |
//! | [`clipboard`] | Put text on the clipboard (dictation is pasted) | `Services/ActionExecutor.cs` |
//! | [`focus`] | Find an app's window, bring it to the front | `Services/ActionExecutor.cs` |
//! | [`input`] | Keyboard and mouse injection (`SendInput`) | `Services/ActionExecutor.cs` |
//! | [`shell`] | Open a URL, file or program like Explorer | `Services/ActionExecutor.cs`, `UserLevelProcess` |
//! | [`uia`] | Read a window's text fields through UI Automation (browser address bar) | `Services/ContextTracker.cs` |
//! | [`uia_panel`] | The focused control and its ancestors, and a scan of a window (IDE panel detection) | `Services/Context/UiFocusProbe.cs`, `WorkbenchRegionProbe.cs` |
//! | [`hardware`] | Processor, AVX2, cores, memory, free disk, graphics cards (voice model choice) | New |
//! | [`network`] | Offline, metered or ordinary connection (model downloads wait on metered) | New |
//! | [`system`] | Elevation, user SID, single-copy lock, login-startup Run key | `ProcessElevation`, `SingleInstanceLock`, `AutostartService` |

#[cfg(windows)]
pub mod audio;
#[cfg(windows)]
pub mod clipboard;
#[cfg(windows)]
pub mod desktop;
#[cfg(windows)]
pub mod focus;
#[cfg(windows)]
pub mod hardware;
#[cfg(windows)]
pub mod input;
#[cfg(windows)]
pub mod job;
#[cfg(windows)]
pub mod keyboard;
#[cfg(windows)]
pub mod media;
#[cfg(windows)]
pub mod network;
#[cfg(windows)]
pub mod power;
#[cfg(windows)]
pub mod shell;
#[cfg(windows)]
pub mod system;
#[cfg(windows)]
pub mod uia;
#[cfg(windows)]
pub mod uia_panel;
#[cfg(windows)]
pub mod window;

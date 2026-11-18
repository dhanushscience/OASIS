//! # oasis-core
//!
//! The pure heart of OASIS, ported from `SW/Oasis.Core` (C#).
//!
//! **Rule:** nothing in this crate reads a clock, touches a file, opens a socket or calls an OS
//! API. Reducers take the current state and an event and return the next state plus a list of
//! *effects* (start a timer, dispatch a command, start the mic). A host in `oasis-app` interprets
//! those effects. That is what makes every rule here testable without a pad, a window or a mic.
//!
//! | Module | C# origin | What it decides |
//! |---|---|---|
//! | [`primitives`] | `Primitives.cs` | Button ids, masks, monotonic time |
//! | [`capability`] | `Capabilities/Capability.cs` | What optional features are available |
//! | [`input`] | `Input/*` | What a press, hold, double-tap or chord on the pad means |
//! | [`voice`] | `Voice/*` | Push-to-talk session lifecycle, media ducking, voice safety |
//! | [`matching`] | `Matching/*` | Which process is a browser, what is YouTube, fuzzy names |
//! | [`bridge`] | `Bridge/*`, `Services/BrowserBridge/YouTubeWatchState.cs` | Browser-extension trust rules and YouTube playback phase |
//! | [`shortcut`] | `Services/ShortcutChord.cs` | Shortcut text → keys, and how a chord is pressed and released |
//! | [`display_label`] | `Services/DisplayLabelFormatter.cs` | A button label fitted to the overlay: capitals, 14 characters |
//! | [`actions`] | `Services/ActionExecutor.cs` (rules), `Services/UrlLaunchPolicy.cs` | What a button does, what is allowed, how an automation runs |
//! | [`pad_connect`] | `Services/BleManager.cs` (policy) | When to connect to the pad, back-off, trust, ping rate |
//! | [`speech`] | `Oasis.Core/Speech/*` | Cleaning up dictated text: punctuation words, numbers, corrections, fillers, glossary, lists |
//! | [`battery`] | `Services/DeviceStatusService.cs`, `BatterySocFormatter.cs` | When to warn that the pad's battery is low |
//! | [`meetings`] | `Services/Meetings/*` (pure parts) | Which meeting is joinable, when to remind |
//! | [`onboarding`] | New | Getting Started: which step the first-run guide is on |
//! | [`placement`] | Quick Launch placement | Where windows go: halves, screens, which window belongs to which item |
//! | [`quick_launch`] | `QuickLaunch/QuickLaunchVoice.cs` | Saying a setup's name opens everything in it |
//! | [`startup`] | `Oasis.Core/Elevation/ElevatedStartTask.cs`, `Services/Elevation/AdminMode.cs` (rules) | Launch arguments, when to hand over to the administrator copy, the scheduled-task definition |

pub mod actions;
pub mod battery;
pub mod bridge;
pub mod capability;
pub mod display_label;
pub mod input;
pub mod matching;
pub mod meetings;
pub mod onboarding;
pub mod pad_connect;
pub mod placement;
pub mod primitives;
pub mod quick_launch;
pub mod shortcut;
pub mod speech;
pub mod startup;
pub mod voice;

pub use primitives::{ButtonId, ButtonMask, Clock, CommandId, Instant, SystemClock};

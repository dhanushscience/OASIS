//! What the engine tells whoever is watching it (the desktop window, the CLI).

use serde::Serialize;

/// A pad press that resolved to a button of the active profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Press {
    /// 1, 2 or 3, as printed on the pad.
    pub button: u8,
    pub profile: String,
    pub config: String,
    /// The action type (`Shortcut`, `LaunchApp`, …); empty for a button with nothing bound.
    pub kind: String,
    pub value: String,
    /// Whether the action was started. False in preview mode, for a blank button, or when the
    /// settings file failed its signature check.
    pub runs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EngineEvent {
    /// `Scanning`, `Connecting`, `Connected`, `Disconnected` or `Unavailable`.
    PadStatus { status: String },
    PadConnected { name: String },
    PadLost,
    /// Why Bluetooth cannot be used right now.
    PadUnavailable { reason: String },
    Battery { percent: u8, charging: bool, full: bool },
    Press(Press),
    /// A gesture with nothing wired to it yet (voice, AI hold, …).
    Unmapped { button: u8, gesture: String },
    ActionFailed { what: String, reason: String },
    /// Something the user should know that is not an error.
    Note { text: String },
    /// Something worth a quiet mention (a video ended): shown and logged, the window stays where it is.
    Notice { title: String, body: String },
    /// A meeting is about to start: the words to show, and whether it has a link to join.
    MeetingReminder { title: String, when: String, message: String, can_join: bool },
    /// The reminder above was answered (or the meeting went away).
    MeetingReminderEnd,
    /// An alarm, timer or reminder came due. The title and the words to show.
    Reminder { title: String, body: String },
    /// A pad button went down or up (the overlay lights its indicator).
    PadButton { button: u8, down: bool },
    /// Show the three button labels of the active profile for `millis`. The tap that caused this
    /// did not run anything; tapping again while they show does.
    ShowLabels { profile: String, labels: Vec<String>, millis: u64 },
    /// The labels should go away (time ran out, or an action ran).
    HideLabels,
    /// Push-to-talk: `listening`, `dictating`, `thinking` or `idle`.
    Voice { phase: String },
    /// While recording, about 20 times a second: how loud the microphone is, 0 to 100 (the pill's bars).
    VoiceLevel { level: u8 },
    /// A recording that produced nothing: `too_short`, `no_speech` (only quiet), or `no_microphone`
    /// (the microphone could not be started). The first-run guide turns these into captions.
    VoiceDropped { reason: String },
    /// A spoken command was understood (or not). `outcome`: `ran`, `preview` (actions are off),
    /// `asking` (waiting for a pad press), `blocked`, `no_match` or `ignored`.
    VoiceCommand { heard: String, label: String, outcome: String, detail: String },
    /// Waiting for the user: button 1 runs it, button 2 cancels.
    /// `close`: the app (or "this window") a close command would shut, so the overlay can ask it in words.
    Confirm { summary: String, close: Option<String> },
    /// A new microphone was plugged in: button 1 uses it, button 2 keeps the current one.
    /// Ends with `ConfirmEnd`.
    MicOffer { name: String },
    /// The question above was answered or timed out.
    ConfirmEnd { approved: bool },
    /// What was heard. `typed` is whether it was typed into the focused app (not in preview).
    Transcript { text: String, typed: bool },
    /// The browser-extension socket's status line and how many browsers are connected.
    BridgeStatus { text: String, clients: usize },
    /// What YouTube is doing in the browser: `youtube.playing`, `youtube.ended`,
    /// `youtube.shorts`, or empty when idle.
    YouTube { token: String, title: String },
    /// The settings in force changed (saved from a settings window, or reloaded from disk).
    SettingsChanged,
    Stopped,
}

/// A snapshot for a status window. Kept up to date by the engine.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct EngineStatus {
    pub running: bool,
    pub pad: String,
    pub pad_name: Option<String>,
    pub battery_percent: Option<u8>,
    pub charging: bool,
    pub latency_ms: Option<u32>,
    pub profiles_loaded: usize,
    /// Actions run when true; presses are only shown when false.
    pub live: bool,
    /// False if `profiles.json` did not match its signature: actions are blocked.
    pub integrity_ok: bool,
    pub last_press: Option<Press>,
    pub last_error: Option<String>,
    /// The browser-extension socket: `STOPPED`, `LISTENING · 127.0.0.1:17322`, an error, …
    pub bridge_status: String,
    pub bridge_clients: usize,
    /// `ConfigWhen` token of what YouTube is doing, or empty.
    pub youtube: String,
    pub youtube_title: String,
    /// Push-to-talk: `unavailable`, `off`, `no_model` or `ready`.
    pub voice: String,
    /// The last app in front that was not OASIS itself: what you were in before opening the window.
    pub front_app: Option<FrontApp>,
    /// Index in the profiles of the one matching `front_app` (the "everywhere" one if no other).
    pub front_profile: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct FrontApp {
    /// Lower-case process name without extension (`code`, `chrome`).
    pub process: String,
    pub title: String,
}

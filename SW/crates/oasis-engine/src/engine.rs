//! The loop. See the crate docs.

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant as StdInstant};

use oasis_app::{ArbiterHost, PadLink};
use oasis_core::bridge::{ConfigWhen, YouTubeWatchEvent, YouTubeWatchState};
use oasis_core::voice::command::{gate, intent};
use oasis_core::voice::command::RouterSettings;
use oasis_core::voice::command::safety::{fast_path, steps_of, FastPath, SafetyTier};
use oasis_core::voice::command::text::is_usable_command;
use oasis_core::voice::command::{route, RunningApp};
use oasis_core::voice::VoiceEvent;
use oasis_core::speech::pipeline as cleanup_pipeline;
use oasis_core::speech::style::Style;
use oasis_core::speech::{CleanupContext, CleanupRequest, GlossaryEntry};
use oasis_core::actions::{run_action, ActionBackend, ActionOutcome, ButtonAction};
use oasis_core::input::{ArbiterContext, ArbiterEffect, GestureClaimId, LabelPolicy, PadEvent};
use oasis_core::matching::ForegroundContext;
use oasis_core::Instant;
use oasis_platform::browser_bridge::{write_token_files, BridgeEvent, BridgeServer};
use oasis_platform::extension_install;
use oasis_platform::foreground::{ForegroundTracker, WindowSource};
use oasis_platform::pad_connector::{ConnectorEvent, PadConnector};
use oasis_platform::paths::OasisPaths;
use oasis_platform::ports::{ButtonSource, ForegroundApp, ForegroundInfo, PadTransport, PlatformResult};
use oasis_platform::profile_store::{LoadOutcome, ProfileStore, SaveError, SettingsIntegrity};
use oasis_platform::security_log::SecurityLog;
use oasis_platform::trusted_device::TrustedDevice;
use oasis_settings::arbiter_bridge::arbiter_config;
use oasis_settings::edit::EditError;
use oasis_settings::matcher::{has_panel_profiles, has_url_based_profiles, matching_profile, resolve_active_profile};
use oasis_settings::youtube::{bridge_enabled, bridge_port, extension_settings, gestures_active};
use oasis_settings::input::InputSettings;
use oasis_settings::voice::VoiceSettings;
use oasis_settings::voice_commands::{
    approval_mode, lexicon as voice_lexicon, router_settings as voice_router_settings, trusted_targets,
};
use oasis_settings::Settings;

use crate::reminders::Reminders;
use crate::use_counts::UseCounts;
use oasis_core::voice::command::reminders::{self as phrases, LocalTime};
use crate::events::{EngineEvent, EngineStatus, FrontApp, Press};
use crate::voice::{Heard, Voice, VoiceContext, VoiceParts};

// ---- what the engine is given ----------------------------------------------------------------

/// What the foreground app is. The real one is a `ForegroundTracker` over the OS.
pub trait ForegroundProbe: Send {
    /// Whether any profile matches on a website, which is what makes reading the address worth it.
    fn set_url_profiles_exist(&mut self, exist: bool);
    /// Whether any profile has a set of buttons for a panel or window title, which makes reading
    /// the focused control worth doing.
    fn set_panel_profiles_exist(&mut self, _exist: bool) {}
    fn current(&mut self) -> PlatformResult<ForegroundInfo>;
    /// What a button press needs: as `current`, but the panel is read again.
    fn current_for_press(&mut self) -> PlatformResult<ForegroundInfo> {
        self.current()
    }
}

impl<S: WindowSource> ForegroundProbe for ForegroundTracker<S> {
    fn set_url_profiles_exist(&mut self, exist: bool) {
        ForegroundTracker::set_url_profiles_exist(self, exist);
    }
    fn set_panel_profiles_exist(&mut self, exist: bool) {
        ForegroundTracker::set_panel_profiles_exist(self, exist);
    }
    fn current_for_press(&mut self) -> PlatformResult<ForegroundInfo> {
        Ok(ForegroundTracker::current_for_press(self))
    }
    fn current(&mut self) -> PlatformResult<ForegroundInfo> {
        ForegroundApp::current(self)
    }
}

/// Everything a backend needs that the engine knows.
pub struct BackendContext {
    pub security_log: SecurityLog,
    pub allow_run_script: bool,
}

/// Builds the backend for one action. Called on the worker thread, once per action.
pub type BackendFactory = Box<dyn Fn(BackendContext) -> Box<dyn ActionBackend> + Send>;

pub struct EngineParts<T: PadTransport> {
    pub transport: T,
    pub foreground: Box<dyn ForegroundProbe>,
    pub make_backend: BackendFactory,
    /// Push-to-talk. `None` leaves voice out (tests, a build without the speech engine).
    pub voice: Option<VoiceParts>,
    /// Windows and the launcher for Quick Launch, to put each window where it was asked. `None`
    /// opens the items but leaves the windows wherever the apps put them.
    pub quick_launch: Option<crate::quick_launch::QuickLaunchParts>,
    /// Spotify by voice. `None` leaves it out.
    pub spotify: Option<Arc<dyn crate::spotify::Runner>>,
    /// Meetings, reminders and joining. `None` leaves them out.
    pub meetings: Option<Arc<crate::meetings::Meetings>>,
    /// Another way of pressing the three buttons besides the pad (the keyboard). `None` leaves it out.
    pub buttons: Option<Box<dyn ButtonSource>>,
}

pub struct EngineConfig {
    pub paths: OasisPaths,
    /// Checks `profiles.json` against its signature. `None` skips the check.
    pub integrity: Option<Box<dyn SettingsIntegrity>>,
    /// Start with actions running. They are shown but not run until this is on.
    pub live: bool,
    /// The browser extension's local socket. `None` leaves it off (tests, no secret store).
    pub bridge: Option<BridgeConfig>,
    /// Run the Quick Launch setup marked "start with OASIS" this long after start. `None` runs
    /// nothing (the command line, tests, a restart OASIS made itself).
    pub startup_setup_after: Option<Duration>,
    /// Start with the keyboard buttons held back (see `EngineHandle::hold_keys`), so a guide that
    /// opens with the app never has them live for a moment first.
    pub hold_keys_at_start: bool,
}

/// What the engine needs to run the browser-extension socket. Whether it listens, and on which
/// port, comes from the settings (`BrowserBridgeEnabled`, `BrowserBridgePort`).
pub struct BridgeConfig {
    /// The per-user secret the extension must present.
    pub token: String,
    /// Where a bundled copy of the extension may be (see `extension_install`).
    pub bundled_extension: Vec<std::path::PathBuf>,
    /// Listen here instead of the settings' port (for rehearsing beside a running OASIS).
    pub port: Option<u16>,
}

#[derive(Debug)]
pub enum EngineError {
    /// There is no `profiles.json` to run.
    NoProfiles(std::path::PathBuf),
    Settings(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::NoProfiles(path) => write!(f, "No profiles.json at {}.", path.display()),
            EngineError::Settings(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for EngineError {}

/// Why a settings change did not happen. Nothing is changed in either case.
#[derive(Debug)]
pub enum ChangeError {
    /// The change itself is not allowed (a shortcut that does not parse, …).
    Invalid(EditError),
    /// It could not be saved: the file was edited elsewhere, or could not be written.
    Save(SaveError),
}

impl fmt::Display for ChangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChangeError::Invalid(e) => e.fmt(f),
            ChangeError::Save(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for ChangeError {}

// ---- shared state ----------------------------------------------------------------------------

type Emit = Arc<dyn Fn(EngineEvent) + Send + Sync>;

/// Lock order, everywhere: `settings`, then `store`. Never the other way round.
struct Shared {
    stop: AtomicBool,
    live: AtomicBool,
    /// False if `profiles.json` failed its signature check and was not accepted.
    integrity_ok: AtomicBool,
    /// The settings in force. The loop re-reads them when `generation` changes.
    settings: Mutex<Settings>,
    generation: AtomicU64,
    /// The keyboard buttons are held back (a guide is on screen). In memory only, never saved, so a
    /// crash or a quit cannot leave them dead.
    keys_held: AtomicBool,
    store: Mutex<ProfileStore>,
    paths: OasisPaths,
    status: Mutex<EngineStatus>,
    emit: Emit,
    /// Spotify commands. `None` leaves them out.
    spotify: Option<Arc<dyn crate::spotify::Runner>>,
    /// Meetings from the calendar. `None` leaves them out.
    meetings: Option<Arc<crate::meetings::Meetings>>,
    /// The low-battery warning has been given for this stretch of low charge.
    low_battery_warned: AtomicBool,
    /// Alarms, timers and reminders waiting to go off.
    reminders: Mutex<Reminders>,
    /// Presses per profile, for ordering profiles by use.
    use_counts: Mutex<UseCounts>,
}

impl Shared {
    fn update(&self, change: impl FnOnce(&mut EngineStatus)) {
        change(&mut self.status.lock().unwrap_or_else(|e| e.into_inner()));
    }

    fn emit(&self, event: EngineEvent) {
        (self.emit)(event);
    }

    fn settings(&self) -> MutexGuard<'_, Settings> {
        self.settings.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn store(&self) -> MutexGuard<'_, ProfileStore> {
        self.store.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Apply an edit to a copy, save it, and only then put it in force (see `EngineHandle::change`).
    fn change<R>(
        &self,
        change: impl FnOnce(&mut Settings) -> Result<R, EditError>,
    ) -> Result<R, ChangeError> {
        let mut current = self.settings();
        let mut draft = current.clone();
        let result = change(&mut draft).map_err(ChangeError::Invalid)?;
        self.store().save(&draft).map_err(ChangeError::Save)?;
        let profiles = draft.profiles.len();
        *current = draft;
        drop(current);
        self.settings_replaced(profiles);
        Ok(result)
    }

    /// New settings are in force: tell the loop and the status.
    fn settings_replaced(&self, profiles: usize) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.update(|s| s.profiles_loaded = profiles);
        self.emit(EngineEvent::SettingsChanged);
    }

    /// Read `profiles.json` again and put it in force, re-checking its signature.
    fn reload_from_disk(&self) -> Result<(), String> {
        let mut current = self.settings();
        let outcome = self.store().load();
        let (settings, verified) = match outcome {
            Ok(LoadOutcome::Loaded { loaded, signature_verified })
            | Ok(LoadOutcome::RecoveredFromLastGood { loaded, signature_verified, .. }) => {
                (loaded.settings, signature_verified)
            }
            Ok(LoadOutcome::NoFile) => return Err("There is no profiles.json to reload.".into()),
            Err(e) => return Err(e.to_string()),
        };
        let profiles = settings.profiles.len();
        *current = settings;
        drop(current);
        self.set_integrity(verified);
        self.settings_replaced(profiles);
        Ok(())
    }

    fn set_integrity(&self, ok: bool) {
        self.integrity_ok.store(ok, Ordering::SeqCst);
        if !ok {
            self.live.store(false, Ordering::SeqCst);
        }
        self.update(|s| {
            s.integrity_ok = ok;
            if !ok {
                s.live = false;
            }
        });
    }
}

struct Job {
    action: ButtonAction,
    button: i32,
    what: String,
    /// The "allow RunScript actions" setting when this press happened.
    allow_run_script: bool,
    /// Work to do on the worker before the action, in this job's place in the queue: a dictation's
    /// cleanup and polish (seconds), which must not hold up the engine loop. It returns the text to
    /// paste, or `None` when nothing is to be pasted.
    prepare: Option<Box<dyn FnOnce() -> Option<String> + Send>>,
}

// ---- the handle ------------------------------------------------------------------------------

pub struct EngineHandle {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    /// Lets "Try it" start a button's action. Dropped first on stop, or the worker never ends.
    jobs: Option<Sender<Job>>,
}

impl EngineHandle {
    /// Runs one saved button's action now, as if it had been pressed, for the "Try it" button.
    /// Held to the same rules as a press: actions must be on and `profiles.json` trusted.
    pub fn try_button(&self, profile: usize, config: usize, button: usize) -> Result<(), String> {
        if !self.shared.integrity_ok.load(Ordering::SeqCst) {
            return Err("profiles.json does not match its signature, so actions stay off.".into());
        }
        if !self.shared.live.load(Ordering::SeqCst) {
            return Err("Actions are switched off, so nothing runs.".into());
        }
        let settings = self.settings();
        let action = settings
            .profiles
            .get(profile)
            .and_then(|p| p.config_profiles.get(config))
            .and_then(|c| c.buttons.get(button))
            .ok_or("That button does not exist.")?
            .to_action();
        if !is_runnable(&action) {
            return Err("This button has nothing to run yet.".into());
        }
        let what = format!("{} {}", action.kind, action.value);
        let job = Job { action, button: button as i32, what, allow_run_script: settings.allow_run_script_actions, prepare: None };
        self.jobs.as_ref().ok_or("OASIS is stopping.")?.send(job).map_err(|_| "OASIS is stopping.".to_string())
    }

    pub fn status(&self) -> EngineStatus {
        self.shared.status.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Turn running actions on or off. Refused (returns `false`) while `profiles.json` fails its
    /// signature check, as in the C# app.
    pub fn set_live(&self, live: bool) -> bool {
        if live && !self.shared.integrity_ok.load(Ordering::SeqCst) {
            self.shared.emit(EngineEvent::Note {
                text: "profiles.json does not match its signature, so actions stay off.".into(),
            });
            return false;
        }
        self.shared.live.store(live, Ordering::SeqCst);
        self.shared.update(|s| s.live = live);
        true
    }

    /// A copy of the settings in force.
    pub fn settings(&self) -> Settings {
        self.shared.settings().clone()
    }

    /// The data folders this engine reads and writes (the real ones, or an `OASIS_DATA_DIR`
    /// copy).
    /// How many presses each profile has had, by profile name.
    pub fn profile_use_counts(&self) -> std::collections::BTreeMap<String, u64> {
        self.shared.use_counts.lock().unwrap_or_else(|e| e.into_inner()).counts()
    }

    pub fn paths(&self) -> &OasisPaths {
        &self.shared.paths
    }

    /// Change the settings, save them, and put them in force at once.
    ///
    /// `change` is applied to a copy. If it refuses, or the save fails (the file was changed by
    /// another program, the signature is untrusted, the disk is full), nothing changes anywhere.
    /// A running gesture finishes with the old settings; the next press uses the new ones.
    pub fn change<R>(
        &self,
        change: impl FnOnce(&mut Settings) -> Result<R, EditError>,
    ) -> Result<R, ChangeError> {
        self.shared.change(change)
    }

    /// Read `profiles.json` again, for example after the C# app edited it. Replaces the settings
    /// in force, and re-checks the signature.
    /// The meetings the engine tracks, for the settings screen. `None` when there is no calendar.
    pub fn meetings(&self) -> Option<Arc<crate::meetings::Meetings>> {
        self.shared.meetings.clone()
    }

    /// Answers the meeting reminder on screen (`join`, `snooze` or `dismiss`).
    pub fn answer_meeting_reminder(&self, action: &str) -> Result<(), String> {
        let action = crate::meetings::ReminderAction::parse(action).ok_or("That is not a reminder button.")?;
        let meetings = self.shared.meetings.as_ref().ok_or("Meetings are not set up in this build.")?;
        let result = meetings.answer_reminder(action, crate::spotify::now_unix());
        self.shared.emit(EngineEvent::MeetingReminderEnd);
        result
    }

    pub fn reload_settings(&self) -> Result<(), String> {
        self.shared.reload_from_disk()
    }

    /// Hold the keyboard buttons back, or let them go again, without touching the saved setting.
    /// The loop takes it up the way it takes up new settings.
    pub fn hold_keys(&self, held: bool) {
        self.shared.keys_held.store(held, Ordering::SeqCst);
        self.shared.generation.fetch_add(1, Ordering::SeqCst);
    }

    /// Keep a `profiles.json` that was edited outside OASIS: the signature mismatch is accepted,
    /// actions can run, and the next save signs the file again. The user's choice, never automatic.
    pub fn accept_edited_settings(&self) {
        let _current = self.shared.settings();
        self.shared.store().accept_edited();
        drop(_current);
        self.shared.set_integrity(true);
        self.shared.generation.fetch_add(1, Ordering::SeqCst);
        self.shared.emit(EngineEvent::Note {
            text: "Accepted the edited profiles.json. It will be signed again the next time settings are saved."
                .into(),
        });
    }

    /// Stop the loop and wait for it. Safe to call twice.
    pub fn stop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        self.jobs = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for EngineHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The engine loop's rest when nothing else holds it (no pad link to wait on): short, so a keyboard
/// press is still judged at once.
const LOOP_PAUSE: Duration = Duration::from_millis(4);

/// How often the loop looks for an outside edit of `profiles.json`.
const DISK_CHECK_EVERY: Duration = Duration::from_millis(1500);

// ---- start -----------------------------------------------------------------------------------

pub fn start<T>(
    mut config: EngineConfig,
    parts: EngineParts<T>,
    on_event: impl Fn(EngineEvent) + Send + Sync + 'static,
) -> Result<EngineHandle, EngineError>
where
    T: PadTransport + 'static,
{
    let emit: Emit = Arc::new(on_event);
    let (store, settings, verified) = load_settings(&mut config, &emit)?;

    let shared = Arc::new(Shared {
        stop: AtomicBool::new(false),
        live: AtomicBool::new(config.live && verified),
        integrity_ok: AtomicBool::new(verified),
        keys_held: AtomicBool::new(config.hold_keys_at_start),
        status: Mutex::new(EngineStatus {
            running: true,
            pad: "Scanning".into(),
            profiles_loaded: settings.profiles.len(),
            live: config.live && verified,
            integrity_ok: verified,
            ..EngineStatus::default()
        }),
        settings: Mutex::new(settings),
        generation: AtomicU64::new(0),
        store: Mutex::new(store),
        low_battery_warned: AtomicBool::new(false),
        spotify: parts.spotify.clone(),
        meetings: parts.meetings.clone(),
        reminders: Mutex::new(Reminders::load(config.paths.profiles().with_file_name("reminders.txt"))),
        use_counts: Mutex::new(UseCounts::load(config.paths.profiles().with_file_name("profile_use.json"))),
        paths: config.paths.clone(),
        emit,
    });
    if !verified {
        shared.emit(EngineEvent::Note {
            text: "profiles.json does not match its signature (edited outside OASIS). \
                   No OASIS interface press will run an action until it is restored or accepted."
                .into(),
        });
    }

    let connector = PadConnector::new(
        parts.transport,
        TrustedDevice::load(config.paths.trusted_device()),
    );
    let (jobs, inbox) = mpsc::channel::<Job>();
    let try_jobs = jobs.clone();

    let worker = spawn_worker(
        inbox,
        parts.make_backend,
        Arc::clone(&shared),
        config.paths.security_log(),
        parts.quick_launch,
    );

    let bridge_config = config.bridge.take();
    let voice_parts = parts.voice;
    let button_source = parts.buttons;
    let startup_setup_at = config.startup_setup_after.map(|after| StdInstant::now() + after);
    let loop_shared = Arc::clone(&shared);
    let foreground = parts.foreground;
    let thread = thread::Builder::new()
        .name("oasis-engine".into())
        .spawn(move || {
            run_loop(
                &loop_shared,
                connector,
                foreground,
                &jobs,
                LoopExtras {
                    bridge_config,
                    voice_parts,
                    startup_setup_at,
                    buttons: button_source,
                },
            );
            drop(jobs); // ends the worker
            let _ = worker.join();
            loop_shared.update(|s| s.running = false);
            loop_shared.emit(EngineEvent::Stopped);
        })
        .expect("a thread can be spawned");

    Ok(EngineHandle { shared, thread: Some(thread), jobs: Some(try_jobs) })
}

fn load_settings(
    config: &mut EngineConfig,
    emit: &Emit,
) -> Result<(ProfileStore, Settings, bool), EngineError> {
    let data_dir = config.paths.profiles().parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let mut store = ProfileStore::new(&data_dir);
    if let Some(integrity) = config.integrity.take() {
        store = store.with_integrity(integrity);
    }
    let (settings, verified) = match store.load() {
        Ok(LoadOutcome::Loaded { loaded, signature_verified }) => {
            (loaded.settings, signature_verified)
        }
        Ok(LoadOutcome::RecoveredFromLastGood { loaded, signature_verified, reason }) => {
            emit(EngineEvent::Note {
                text: format!("profiles.json was unreadable ({reason}); using the last good copy."),
            });
            (loaded.settings, signature_verified)
        }
        Ok(LoadOutcome::NoFile) => return Err(EngineError::NoProfiles(config.paths.profiles())),
        Err(e) => return Err(EngineError::Settings(e.to_string())),
    };
    Ok((store, settings, verified))
}

// ---- the loop --------------------------------------------------------------------------------

/// The optional parts of the loop, bundled so it takes few arguments.
struct LoopExtras {
    bridge_config: Option<BridgeConfig>,
    voice_parts: Option<VoiceParts>,
    startup_setup_at: Option<StdInstant>,
    buttons: Option<Box<dyn ButtonSource>>,
}

fn run_loop<T: PadTransport>(
    shared: &Arc<Shared>,
    mut connector: PadConnector<T>,
    mut foreground: Box<dyn ForegroundProbe>,
    jobs: &Sender<Job>,
    extras: LoopExtras,
) {
    let LoopExtras { bridge_config, voice_parts, mut startup_setup_at, buttons } = extras;
    let mut seen = shared.generation.load(Ordering::SeqCst);
    let mut settings = shared.settings().clone();
    let mut verified = shared.integrity_ok.load(Ordering::SeqCst);

    // Which ways of pressing the buttons are on: the pad, and the keyboard.
    let mut buttons = buttons;
    let mut input = InputSettings::from(&settings);
    let mut hardware_was_on = true;
    let mut held = shared.keys_held.load(Ordering::SeqCst);
    apply_input(&mut buttons, &input, &mut connector, shared);

    // Push-to-talk needs voice switched on and a speech model present; the gesture rules only
    // arm it then.
    let mut voice = voice_parts.map(Voice::new);
    let mut voice_ready = voice.as_ref().is_some_and(|v| v.ready(&settings));
    report_voice_state(shared, voice.as_ref(), &settings);
    // Load the speech model now, not on the first recording (VOICE_DECISIONS.md V9).
    if let Some(voice) = voice.as_mut() {
        voice.warm(&settings);
    }

    let mut link = PadLink::new();
    let mut arbiter = ArbiterHost::new(arbiter_config(&settings, voice_ready), true);
    let mut context = ArbiterContext::desktop();
    context.execution_allowed = verified;
    arbiter.set_context(context);
    foreground.set_url_profiles_exist(has_url_based_profiles(&settings.profiles));
    foreground.set_panel_profiles_exist(has_panel_profiles(&settings.profiles));

    // When the labels now on screen go away, if any.
    let mut peek_until: Option<Instant> = None;
    // A pad button was let go during this pass (see the push-to-talk pre-roll).
    let mut pending_release = false;
    // When the talk button last went down and how long the pre-roll took to open the mic, for the
    // press line in the timing log.
    let mut talk_down: Option<(StdInstant, u64)> = None;
    // "Voice is getting ready: 48%" is on the overlay (the talk button was pressed during the download).
    let mut getting_ready_shown = false;
    // A spoken command waiting for the pad to say yes or no.
    let mut pending: Option<Pending> = None;

    // The browser extension: what YouTube is doing decides which set of buttons a profile shows,
    // and whether the hidden YouTube gestures are live.
    let (youtube_tx, youtube_rx) = mpsc::channel::<YouTubeWatchEvent>();
    let mut bridge: Option<(BridgeServer, u16)> = None;
    sync_bridge(shared, bridge_config.as_ref(), &settings, &mut bridge, &youtube_tx);
    crate::polish::sync(&shared.paths.models_folder(), &settings);
    sync_preferences(&settings);
    let mut watch = YouTubeWatchState::default();
    let mut last_near_end = String::new();

    // profiles.json is checked for outside edits (the C# app, a text editor) a couple of times a second.
    let mut next_disk_check = StdInstant::now() + DISK_CHECK_EVERY;
    let mut last_tried = None;
    let mut next_reminder_check = StdInstant::now();
    let mut next_front_check = StdInstant::now();
    let mut last_front: Option<ForegroundInfo> = None;
    let mut next_meeting_check = StdInstant::now();
    let mut next_meeting_refresh = StdInstant::now();
    // The first memory line a minute after start, when the models have loaded.
    let mut next_memory_log = StdInstant::now() + Duration::from_secs(60);

    let start = StdInstant::now();
    while !shared.stop.load(Ordering::SeqCst) {
        if startup_setup_at.is_some_and(|at| StdInstant::now() >= at) {
            startup_setup_at = None;
            if verified && shared.live.load(Ordering::SeqCst) {
                if let Some(setup) = oasis_settings::quick_launch::startup_setup(&settings) {
                    let action = ButtonAction { kind: "QuickLaunch".into(), value: setup.id, steps: vec![] };
                    let _ = jobs.send(Job { action, button: -1, what: "start-up setup".into(), allow_run_script: false, prepare: None });
                }
            }
        }
        if StdInstant::now() >= next_meeting_check {
            next_meeting_check = StdInstant::now() + Duration::from_secs(2);
            if let Some(meetings) = shared.meetings.clone() {
                let now = crate::spotify::now_unix();
                if StdInstant::now() >= next_meeting_refresh {
                    next_meeting_refresh = StdInstant::now() + Duration::from_secs(60);
                    // The network can be slow: ask on a thread of its own.
                    let background = Arc::clone(&meetings);
                    thread::spawn(move || background.refresh(now));
                }
                let section = oasis_settings::meetings::MeetingSettings::from(&settings);
                if let Some(due) = meetings.due_reminder(now, section.reminders_enabled, section.reminder_minutes_before) {
                    let offset = local_now().offset_secs;
                    shared.emit(EngineEvent::MeetingReminder {
                        title: due.title.clone(),
                        when: oasis_core::meetings::range_text(&due, offset),
                        message: oasis_core::meetings::reminder_text(&due, now),
                        can_join: !due.join_url.trim().is_empty(),
                    });
                }
            }
        }
        if StdInstant::now() >= next_reminder_check {
            next_reminder_check = StdInstant::now() + Duration::from_secs(1);
            fire_due_reminders(shared, jobs);
        }
        if StdInstant::now() >= next_front_check {
            next_front_check = StdInstant::now() + FRONT_CHECK_EVERY;
            // Remember the last app that was not OASIS: the window offers to make it a profile.
            if let Some(info) = foreground.current().ok().filter(|i| !is_oasis(i) && !i.process.is_empty()) {
                last_front = Some(info);
            }
            // Matched again against the profiles in force every time, so a profile made while OASIS
            // is in front is the one for that app straight away.
            if let Some(info) = &last_front {
                let profile = matching_profile(&settings.profiles, &ForegroundContext::from(info));
                let front = FrontApp { process: info.process.clone(), title: info.window_title.clone() };
                shared.update(|s| {
                    s.front_app = Some(front);
                    s.front_profile = profile;
                });
            }
        }
        if StdInstant::now() >= next_disk_check {
            next_disk_check = StdInstant::now() + DISK_CHECK_EVERY;
            let changed = shared.store().changed_on_disk();
            if changed.is_some() && changed != last_tried {
                last_tried = changed;
                match shared.reload_from_disk() {
                    Ok(()) => shared.emit(EngineEvent::Note {
                        text: "profiles.json was changed by another program, so OASIS loaded it again.".into(),
                    }),
                    Err(e) => shared.emit(EngineEvent::Note {
                        text: format!("profiles.json changed but could not be read, so the old settings stay: {e}"),
                    }),
                }
            }
        }
        // New settings were saved or reloaded: take them up before the next event.
        let generation = shared.generation.load(Ordering::SeqCst);
        if generation != seen {
            seen = generation;
            settings = shared.settings().clone();
            let chosen = InputSettings::from(&settings);
            let held_now = shared.keys_held.load(Ordering::SeqCst);
            if chosen != input || held_now != held {
                input = chosen;
                held = held_now;
                apply_input(&mut buttons, &input, &mut connector, shared);
            }
            verified = shared.integrity_ok.load(Ordering::SeqCst);
            voice_ready = voice.as_ref().is_some_and(|v| v.ready(&settings));
            report_voice_state(shared, voice.as_ref(), &settings);
            if let Some(voice) = voice.as_mut() {
                voice.warm(&settings);
            }
            arbiter.set_config(arbiter_config(&settings, voice_ready));
            context.execution_allowed = verified;
            arbiter.set_context(context);
            foreground.set_url_profiles_exist(has_url_based_profiles(&settings.profiles));
            foreground.set_panel_profiles_exist(has_panel_profiles(&settings.profiles));
            sync_bridge(shared, bridge_config.as_ref(), &settings, &mut bridge, &youtube_tx);
            crate::polish::sync(&shared.paths.models_folder(), &settings);
            sync_preferences(&settings);
        }

        while let Ok(event) = youtube_rx.try_recv() {
            if oasis_core::bridge::youtube_watch::near_end_peek(&event, &mut last_near_end) && peek_until.is_none() && verified {
                if let Some((profile, labels)) = peek_labels(&settings, foreground.as_mut(), None, watch.current_when_token()) {
                    let peek = Duration::from_secs(12);
                    peek_until = Some(Instant::from_millis(start.elapsed().as_millis() as i64).plus(peek));
                    context.labels_on_screen = true;
                    arbiter.set_context(context);
                    shared.emit(EngineEvent::ShowLabels { profile, labels, millis: peek.as_millis() as u64 });
                }
            }
            if let Some(body) = oasis_core::bridge::youtube_watch::ended_notice(&event) {
                shared.emit(EngineEvent::Notice { title: "YouTube ended".into(), body });
            }
            if watch.apply(&event) {
                let (token, title) = (watch.current_when_token().to_string(), watch.title.clone());
                shared.update(|s| {
                    s.youtube = token.clone();
                    s.youtube_title = title.clone();
                });
                shared.emit(EngineEvent::YouTube { token, title });
            }
        }
        let watch_token = watch.current_when_token();

        let now = Instant::from_millis(start.elapsed().as_millis() as i64);
        let mut effects: Vec<ArbiterEffect> = Vec::new();

        if peek_until.is_some_and(|until| now >= until) {
            peek_until = None;
            context.labels_on_screen = false;
            arbiter.set_context(context);
            shared.emit(EngineEvent::HideLabels);
        }

        // With the pad off the connector is not asked (it would sit scanning); sleep a little instead.
        let mut connector_events = Vec::new();
        if input.hardware {
            if !hardware_was_on {
                hardware_was_on = true;
                shared.update(|s| s.pad = "Scanning".into());
                shared.emit(EngineEvent::PadStatus { status: "Scanning".into() });
            }
            // Looking for the pad holds this loop for 0.3 to 1.5 s, so not while a keyboard press is
            // being judged or the voice is busy: the hold, its start sound and the pill would wait.
            let busy = !matches!(arbiter.state(), oasis_core::input::PadState::Idle) || voice.as_ref().is_some_and(|v| !v.is_idle());
            if busy && !connector.is_connected() {
                thread::sleep(LOOP_PAUSE);
            } else {
                let polled = StdInstant::now();
                connector_events = connector.poll(now);
                // Between scans (5 s while Bluetooth is off, after a failed connect) the poll returns at
                // once; without a pause the loop then spun a whole CPU core.
                if !connector.is_connected() && polled.elapsed() < LOOP_PAUSE {
                    thread::sleep(LOOP_PAUSE);
                }
            }
        } else {
            if hardware_was_on {
                hardware_was_on = false;
                connector_events = connector.pause();
                shared.update(|s| s.pad = "Off".into());
                shared.emit(EngineEvent::PadStatus { status: "Off".into() });
            }
            thread::sleep(LOOP_PAUSE);
        }
        // What went down or up this turn, from the pad and from the keyboard, judged together.
        // (event, came from the keyboard)
        let mut pad_events: Vec<PadEvent> = Vec::new();
        for event in connector_events {
            match event {
                ConnectorEvent::Status(status) => {
                    shared.update(|s| s.pad = status.as_str().to_string());
                    shared.emit(EngineEvent::PadStatus { status: status.as_str().to_string() });
                }
                ConnectorEvent::UnavailableReason(reason) => {
                    shared.update(|s| s.last_error = Some(reason.clone()));
                    shared.emit(EngineEvent::PadUnavailable { reason });
                }
                ConnectorEvent::Connected { name } => {
                    shared.update(|s| s.pad_name = Some(name.clone()));
                    shared.emit(EngineEvent::PadConnected { name });
                }
                ConnectorEvent::Rssi(_) => {}
                ConnectorEvent::Disconnected => {
                    shared.update(|s| {
                        s.pad_name = None;
                        s.battery_percent = None;
                        s.charging = false;
                        s.latency_ms = None;
                    });
                    shared.emit(EngineEvent::PadLost);
                    shared.low_battery_warned.store(false, Ordering::SeqCst);
                    pad_events.push(link.on_disconnected());
                }
                ConnectorEvent::Notification(bytes) => {
                    let (events, telemetry_changed) = link.on_notification(&bytes, now);
                    pad_events.extend(events);
                    if telemetry_changed {
                        report_telemetry(shared, &link);
                    }
                }
            }
        }
        // The keyboard's buttons join the pad's.
        if let Some(source) = buttons.as_mut() {
            for (button, down) in source.drain() {
                let id = oasis_core::ButtonId(i32::from(button));
                pad_events.push(if down { PadEvent::Down(id, now) } else { PadEvent::Up(id, now) });
            }
        }
        // The pad and the keyboard's buttons run their actions on every window, OASIS's own included.
        for pad_event in pad_events {
            match pad_event {
                PadEvent::Down(b, _) => {
                    shared.emit(EngineEvent::PadButton {
                        button: (b.0 + 1) as u8,
                        down: true,
                    });
                    // Hold-2× and double-click volume are live only on a playing
                    // YouTube watch page in front, so look before the press is judged.
                    let active = is_youtube_gesture_button(&settings, b.0)
                        && foreground.current().ok().is_some_and(|info| {
                            gestures_active(&settings, &watch, &ForegroundContext::from(&info))
                        });
                    if context.youtube_gestures_active != active {
                        context.youtube_gestures_active = active;
                        arbiter.set_context(context);
                    }
                    // Voice is on but its model is still downloading: say so instead of nothing.
                    if !voice_ready && is_talk_button(&settings, b.0) && VoiceSettings::from(&settings).enabled {
                        if let Some(percent) = crate::model_download::progress() {
                            shared.emit(EngineEvent::Voice { phase: format!("getting_ready:{percent}") });
                            getting_ready_shown = true;
                        }
                    }
                    // Start listening before the hold is recognised, so the first
                    // words are not lost.
                    if let (true, Some(voice)) = (voice_ready && is_talk_button(&settings, b.0), voice.as_mut()) {
                        // Wakes an idle Bluetooth speaker during the hold, so the start cue (which has
                        // no lead-in of its own any more) is heard the moment the hold is recognised.
                        cue(&settings, VOICE_WAKE);
                        let down = StdInstant::now();
                        voice.preroll(&settings);
                        talk_down = Some((down, down.elapsed().as_millis() as u64));
                    }
                }
                PadEvent::Up(b, _) => {
                    shared.emit(EngineEvent::PadButton {
                        button: (b.0 + 1) as u8,
                        down: false,
                    });
                    if std::mem::take(&mut getting_ready_shown) {
                        shared.emit(EngineEvent::Voice { phase: "idle".into() });
                    }
                    pending_release = true;
                }
                _ => {}
            }
            arbiter.apply(pad_event, now, None, None, &mut |e| effects.push(e.clone()));
        }
        arbiter.fire_due(now, &mut |e| effects.push(e.clone()));

        // Push-to-talk: begin or end a capture as the gesture rules say. Whether this recording is
        // typed or run as a command is decided once, now, from the program and focus in front.
        let mut voice_requests: Vec<VoiceEvent> = Vec::new();
        let begins = effects.iter().any(|e| matches!(e, ArbiterEffect::StartVoiceCapture));
        let begin_dictate = match (begins, voice.as_ref()) {
            (true, Some(voice)) => {
                // The start sound and the pill come first, so the user knows at once they can talk:
                // the focus question below can take up to 250 ms and pausing media longer still.
                // "listening" and "dictating" look the same; the session's own event follows.
                if voice_ready {
                    cue(&settings, VOICE_IN);
                }
                let cue_at = StdInstant::now();
                if voice_ready && voice.is_idle() {
                    shared.emit(EngineEvent::Voice { phase: "listening".into() });
                }
                let shown_at = StdInstant::now();
                let process = foreground.current().map(|i| ForegroundContext::from(&i).app_name).unwrap_or_default();
                crate::polish::set_front_style(dictation_style(foreground.as_mut()));
                let asked = StdInstant::now();
                let dictate = voice.wants_dictation(&settings, &process);
                // One line per capture start: where the time between the press and the feedback went.
                if let Some((down, mic_open_ms)) = talk_down.take() {
                    let since = |at: StdInstant| at.saturating_duration_since(down).as_millis();
                    SecurityLog::at(shared.paths.voice_timing_log()).write(&format!(
                        "press cue_ms={} hud_ms={} mic_open_ms={mic_open_ms} focus_ms={}",
                        since(cue_at),
                        since(shown_at),
                        asked.elapsed().as_millis()
                    ));
                }
                dictate
            }
            _ => true,
        };
        effects.retain(|effect| match effect {
            ArbiterEffect::StartVoiceCapture => {
                voice_requests.push(VoiceEvent::BeginRequested {
                    at: now,
                    enabled: voice_ready,
                    model_ready: voice_ready,
                    dictate: begin_dictate,
                });
                false
            }
            ArbiterEffect::EndVoiceCapture => {
                voice_requests.push(VoiceEvent::EndRequested { at: now });
                false
            }
            ArbiterEffect::CancelVoiceCapture => {
                voice_requests.push(VoiceEvent::CancelRequested);
                false
            }
            _ => true,
        });
        if let Some(voice) = voice.as_mut() {
            let emit = |event: EngineEvent| shared.emit(event);
            let youtube_playing = watch_token == ConfigWhen::PLAYING;
            let ctx = VoiceContext {
                emit: &emit,
                settings: &settings,
                bridge: bridge.as_ref().map(|(server, _)| server),
                youtube_playing,
                log: &|line| SecurityLog::at(shared.paths.voice_timing_log()).write(line),
            };
            for request in voice_requests {
                voice.request(request, now, &ctx);
            }
            // A double-click lock: tell the pill to show the tick and cross. It stays "locked" on
            // the page until the phase goes to thinking or idle.
            if begins && matches!(arbiter.state(), oasis_core::input::PadState::VoiceLocked) {
                shared.emit(EngineEvent::Voice { phase: "locked".into() });
            }
            // A button was let go without the hold becoming a capture: drop the pre-roll.
            if std::mem::take(&mut pending_release) {
                voice.discard_preroll();
            }
            crate::activity::set_voice_active(!voice.is_idle());
            for heard in voice.tick(now, &ctx) {
                let spoken = SpokenContext {
                    shared,
                    settings: &settings,
                    verified,
                    jobs,
                    watch_token,
                };
                let (timing, dictate, words) = (
                    heard.timing,
                    heard.dictate,
                    heard.text.split_whitespace().count(),
                );
                let text_ms = timing.released.map(|r| r.elapsed().as_millis());
                let installed = |name: &str| voice.app_installed(name);
                let ask = handle_heard(&spoken, foreground.as_mut(), voice.running_apps(), &installed, heard, now);
                log_voice_timing(shared, &settings, timing, dictate, words, text_ms);
                if let Some(ask) = ask {
                    context.confirmation_pending = true;
                    arbiter.set_context(context);
                    let front = foreground.current().map(|i| ForegroundContext::from(&i).app_name).unwrap_or_default();
                    shared.emit(EngineEvent::Confirm { summary: ask.summary.clone(), close: close_target(&ask.action, &front) });
                    pending = Some(ask);
                }
            }
        }
        // The polish server is stopped after 10 idle minutes (frees ~1.2 GB) and started at the next press.
        for line in crate::polish::tick(voice.as_ref().is_some_and(|v| !v.is_idle())) {
            SecurityLog::at(shared.paths.voice_timing_log()).write(&line);
        }
        if StdInstant::now() >= next_memory_log {
            next_memory_log = StdInstant::now() + MEMORY_LOG_EVERY;
            let own = oasis_win::job::usage(std::process::id());
            let polish = crate::polish::server_pid().and_then(oasis_win::job::usage);
            SecurityLog::at(shared.paths.voice_timing_log()).write(&memory_line(own, polish));
        }

        // An unanswered question is dropped after a while, never left hanging over the pad.
        if pending.as_ref().is_some_and(|ask| now >= ask.until) {
            pending = None;
            context.confirmation_pending = false;
            arbiter.set_context(context);
            shared.emit(EngineEvent::ConfirmEnd { approved: false });
            shared.emit(EngineEvent::Note { text: "The voice command was not confirmed in time, so it was cancelled.".into() });
        }

        for effect in effects {
            match effect {
                ArbiterEffect::DispatchCommand { button, claim: GestureClaimId::Confirmation } => {
                    // The pad answers a spoken command's question: button 1 runs it, button 2
                    // cancels, anything else is ignored.
                    if let Some(ask) = pending.take_if(|_| matches!(button.0, 0 | 1)) {
                        let approved = button.0 == 0;
                        context.confirmation_pending = false;
                        arbiter.set_context(context);
                        shared.emit(EngineEvent::ConfirmEnd { approved });
                        if approved {
                            let _ = shared.change(|s| {
                                s.trust_voice_targets(&ask.steps);
                                Ok(())
                            });
                            run_voice_action(shared, jobs, &ask.action, &ask.label, &ask.heard);
                        } else {
                            shared.emit(EngineEvent::VoiceCommand {
                                heard: ask.heard,
                                label: ask.label,
                                outcome: "cancelled".into(),
                                detail: "Cancelled.".into(),
                            });
                        }
                    }
                }
                ArbiterEffect::DispatchCommand { button, claim } => {
                    // First tap reveals the labels; the second one (while they show) runs.
                    let reveal = (claim == GestureClaimId::BoundTap && peek_until.is_none() && verified)
                        .then(|| peek_labels(&settings, foreground.as_mut(), Some(button.0), watch_token))
                        .flatten();
                    if let Some((profile, labels)) = reveal {
                        let peek = Duration::from_secs(4);
                        peek_until = Some(now.plus(peek));
                        context.labels_on_screen = true;
                        arbiter.set_context(context);
                        shared.emit(EngineEvent::ShowLabels {
                            profile,
                            labels,
                            millis: peek.as_millis() as u64,
                        });
                        continue;
                    }
                    // Running something ends the label peek.
                    if peek_until.take().is_some() {
                        context.labels_on_screen = false;
                        arbiter.set_context(context);
                        shared.emit(EngineEvent::HideLabels);
                    }
                    let gesture = Gesture { button: button.0, claim, watch_token };
                    dispatch(shared, &settings, verified, foreground.as_mut(), jobs, gesture);
                }
                ArbiterEffect::SendToDevice(command) => {
                    if let Some((server, _)) = &bridge {
                        server.send_command(command);
                    }
                }
                ArbiterEffect::ShowPeek(duration) => {
                    if let Some((profile, labels)) =
                        peek_labels(&settings, foreground.as_mut(), None, watch_token)
                    {
                        let millis = duration.as_millis() as u64;
                        peek_until = Some(now.plus(duration));
                        context.labels_on_screen = true;
                        arbiter.set_context(context);
                        shared.emit(EngineEvent::ShowLabels { profile, labels, millis });
                    }
                }
                _ => {}
            }
        }
    }
    arbiter.shutdown();
    if let Some(mut voice) = voice.take() {
        voice.shutdown();
    }
    if let Some((mut server, _)) = bridge.take() {
        server.stop();
    }
    crate::polish::stop();
}

/// Take up the choice of input methods: the keyboard listener on or off, with its shortcuts, and how
/// long one look for the pad may block the loop (short while the keyboard must answer quickly).
fn apply_input<T: PadTransport>(
    buttons: &mut Option<Box<dyn ButtonSource>>,
    input: &InputSettings,
    connector: &mut PadConnector<T>,
    shared: &Shared,
) {
    // A listener that sees every key is worth a line in the security log whenever it is on or off.
    // Held back (a guide is on screen) counts as off for the listener, the log and the scan time.
    let held = shared.keys_held.load(Ordering::SeqCst);
    let keyboard = input.keyboard && !held;
    if let Some(source) = buttons.as_mut() {
        let log = SecurityLog::at(shared.paths.security_log());
        log.write(if keyboard {
            "Keyboard input switched on (listening for the chosen shortcuts only)."
        } else if input.keyboard {
            "Keyboard input is held back while the guide is on screen (not listening)."
        } else {
            "Keyboard input is off."
        });
        source.configure(keyboard, input.keys.clone(), input.exclusive);
        if let Some(text) = source.problem() {
            shared.emit(EngineEvent::Note { text });
        }
    }
    connector.set_scan_time((keyboard && buttons.is_some()).then_some(Duration::from_millis(300)));
}

/// One line per recording in `voice-timing.log`: where the wait after letting go of the button went.
/// Durations and counts only, never what was said. `text_ms`: release until the text was ready;
/// `handed_ms`: release until it was cleaned and handed to the paste (or the command decided).
/// The paste itself (clipboard, a fixed 120 ms wait, Ctrl+V) runs after that, on the worker.
fn log_voice_timing(
    shared: &Shared,
    settings: &Settings,
    timing: crate::voice::Timing,
    dictate: bool,
    words: usize,
    text_ms: Option<u128>,
) {
    let since = |ms: Option<u128>| ms.map_or_else(|| "-".to_string(), |m| m.to_string());
    let line =
        format!(
        "{} stt={} audio_ms={} segments={} load_ms={} stt_ms={} text_ms={} handed_ms={} words={}",
        if dictate { "dictate" } else { "command" },
        if VoiceSettings::from(settings).use_groq { "groq" } else { "local" },
        timing.audio_ms,
        timing.segments,
        timing.load_ms,
        timing.stt_ms,
        since(text_ms),
        since(timing.released.map(|r| r.elapsed().as_millis())),
        words,
    );
    SecurityLog::at(shared.paths.voice_timing_log()).write(&line);
}

/// How often a memory line goes to `voice-timing.log`, so a tester's log shows any growth.
const MEMORY_LOG_EVERY: Duration = Duration::from_secs(10 * 60);

/// The memory line: private MB and open handles of OASIS and of the polish server (`-` when it is
/// not running or could not be read).
fn memory_line(own: Option<(u64, u32)>, polish: Option<(u64, u32)>) -> String {
    let part = |u: Option<(u64, u32)>| u.map_or_else(|| "private_mb=- handles=-".to_string(), |(bytes, handles)| format!("private_mb={} handles={handles}", bytes / (1024 * 1024)));
    format!("memory oasis {} | polish-server {}", part(own), part(polish))
}

fn report_voice_state(shared: &Shared, voice: Option<&Voice>, settings: &Settings) {
    let word = voice.map_or("unavailable", |v| v.state_word(settings));
    shared.update(|s| s.voice = word.to_string());
}

/// Whether `button` is part of the push-to-talk combination.
fn is_talk_button(settings: &Settings, button: i32) -> bool {
    VoiceSettings::from(settings).push_to_talk_buttons.contains(&button)
}

/// A spoken command waiting for the pad: button 1 runs it, button 2 cancels.
struct Pending {
    action: ButtonAction,
    steps: Vec<(String, String)>,
    summary: String,
    heard: String,
    label: String,
    until: Instant,
}

/// How long an unanswered question stays on the pad.
const CONFIRM_TIMEOUT: Duration = Duration::from_secs(30);

/// What the handlers of a spoken phrase need.
struct SpokenContext<'a> {
    shared: &'a Arc<Shared>,
    settings: &'a Settings,
    verified: bool,
    jobs: &'a Sender<Job>,
    watch_token: &'a str,
}

/// Something was said. The intent gate (`docs/VOICE_DECISIONS.md` V3, V4) decides: with a text
/// field in front (`heard.dictate`), it is typed unless the transcript is a clear command whose
/// target exists (`intent::judge`: "open Chrome", "close WhatsApp" with WhatsApp open); with none,
/// it is a command. Returns a question for
/// the pad when the command needs approval or was a near miss.
fn handle_heard(
    ctx: &SpokenContext<'_>,
    foreground: &mut dyn ForegroundProbe,
    running: Vec<RunningApp>,
    installed: &dyn Fn(&str) -> bool,
    heard: Heard,
    now: Instant,
) -> Option<Pending> {
    let in_text_field = heard.dictate;
    // A fragment said on its own ("insert my LinkedIn") types its text exactly (V5), in a text
    // field or not. Without one it is typing by voice, so it follows that setting.
    if let Some(text) = oasis_core::speech::fragments::whole_phrase(&heard.text, &VoiceSettings::from(ctx.settings).replacements) {
        if in_text_field || voice_router_settings(ctx.settings).allow_type_text {
            let value = if text.ends_with(char::is_whitespace) { text.to_string() } else { format!("{text} ") };
            paste_typed(ctx.shared, ctx.settings, ctx.verified, ctx.jobs, text, value);
        } else {
            ctx.shared.emit(EngineEvent::VoiceCommand {
                heard: heard.text.clone(),
                label: "FRAGMENT".into(),
                outcome: "blocked".into(),
                detail: "Typing text by voice is off in Settings.".into(),
            });
        }
        return None;
    }
    let phrase = gate::prepare(&heard.text);
    if !in_text_field && gate::is_prose_about_writing(&phrase) {
        let style = dictation_style(foreground);
        type_dictation(ctx, heard, style, true);
        return None;
    }
    let spoken = SpokenPhrase { transcript: &heard.text, phrase: &phrase, in_text_field };
    match run_spoken_command(ctx, foreground, &running, installed, &spoken, now) {
        Spoken::Handled(ask) => ask,
        // Not a command after all: in a text field, it was meant to be typed.
        Spoken::NotACommand => {
            let style = dictation_style(foreground);
            type_dictation(ctx, heard, style, !in_text_field);
            None
        }
    }
}

/// A phrase on its way to being run: what was heard, what the gate made of it, and whether a
/// text field had focus.
struct SpokenPhrase<'a> {
    transcript: &'a str,
    phrase: &'a gate::Phrase,
    in_text_field: bool,
}

/// What became of a phrase tried as a command.
enum Spoken {
    /// Run, asked about, refused or reported; maybe with a question for the pad.
    Handled(Option<Pending>),
    /// In a text field, and not a sure command: type it instead.
    NotACommand,
}

/// Whether the program a `LaunchApp` names is really there: running now, installed, or a browser
/// with a page (`chrome|youtube.com`). A spoken "open the door" names nothing.
fn launch_target_exists(value: &str, running: &[RunningApp], installed: &dyn Fn(&str) -> bool) -> bool {
    value.contains('|') || oasis_core::voice::command::router::find_running(value, running).is_some() || installed(value)
}

fn run_spoken_command(
    ctx: &SpokenContext<'_>,
    foreground: &mut dyn ForegroundProbe,
    running: &[RunningApp],
    installed: &dyn Fn(&str) -> bool,
    spoken: &SpokenPhrase<'_>,
    now: Instant,
) -> Spoken {
    let (transcript, phrase, in_text_field) = (spoken.transcript, spoken.phrase, spoken.in_text_field);
    let say = |outcome: &str, label: &str, detail: &str| {
        ctx.shared.emit(EngineEvent::VoiceCommand {
            heard: transcript.to_string(),
            label: label.to_string(),
            outcome: outcome.to_string(),
            detail: detail.to_string(),
        });
    };
    let log = SecurityLog::at(ctx.shared.paths.security_log());

    if !is_usable_command(transcript) || phrase.command.is_empty() {
        if in_text_field {
            return Spoken::NotACommand;
        }
        say("ignored", "", "Heard nothing usable.");
        log.write(&format!("Voice ignored (empty/noise): \"{transcript}\""));
        return Spoken::Handled(None);
    }
    // "search this" with nothing after it (or cut off): no browser, no question, not typed (W15).
    if oasis_core::voice::command::web_search::is_empty_search(&phrase.command) {
        say("ignored", "", "A search with nothing to search for.");
        log.write(&format!("Voice ignored (empty search): \"{transcript}\""));
        return Spoken::Handled(None);
    }
    // The user's dictionary fixes words the speech model keeps hearing wrong, for commands as for dictation.
    let words: Vec<GlossaryEntry> = VoiceSettings::from(ctx.settings).dictation_glossary.iter().map(|(s, c)| GlossaryEntry::new(s, c)).collect();
    let text = oasis_core::speech::glossary::apply(&phrase.command, &words).0;

    let active = foreground.current_for_press().ok().and_then(|info| {
        let found = resolve_active_profile(&ctx.settings.profiles, &ForegroundContext::from(&info), ctx.watch_token)?;
        Some((found.profile, found.config?))
    });
    let lexicon = voice_lexicon(ctx.settings, active, running);
    let routed = route(
        &text,
        &lexicon,
        &RouterSettings {
            now: Some(local_now()),
            spotify_running: running.iter().any(|a| a.process.eq_ignore_ascii_case("spotify")),
            running: running.to_vec(),
            // "cloud" means Claude only on a PC that has it.
            installed: Some(["claude", "whatsapp", "kicad"].iter().filter(|a| installed(a)).map(|a| a.to_string()).collect()),
            ..voice_router_settings(ctx.settings)
        },
    );
    // An app that is not on this PC is no command: "open the door" stays words, "open chatgpt" opens
    // the site. In "open X on … and open Y on …" the apps that are there still open.
    let exists = |app: &str| launch_target_exists(app, running, installed);
    let (found, missing) = match routed.action.clone().filter(|_| routed.success) {
        Some(action) => without_missing_apps(intent::site_for_missing_app(action, &exists), &exists),
        None => (None, Vec::new()),
    };
    // With a text field in front, the transcript and its target must make a clear command; an
    // unsure phrase is typed into the field (intent::judge). Without one, the flow below decides.
    let evidence = intent::found(routed.success, routed.score, found.as_ref());
    let verdict = intent::judge(transcript, phrase, evidence, found.as_ref().map_or(SafetyTier::Safe, intent::tier), in_text_field, voice_router_settings(ctx.settings).edit_commands);
    let found = found.filter(|_| !in_text_field || verdict.command);
    let Some(action) = found else {
        // Nothing sure to run. In a text field it was meant to be typed; with no field found it is
        // typed too when it reads as prose ("I want to write an email…"), so a dictation is not lost
        // because the field was never clicked. One sentence that starts like a command and
        // matched nothing ("open the dor") is still reported, not typed.
        let prose = !gate::reads_as_command(phrase);
        if (in_text_field || prose) && !phrase.addressed {
            return Spoken::NotACommand;
        }
        let (ask, no_match) = offer_or_report(ctx, &routed, running, installed, transcript, now);
        // Never lose a transcript (1.7): a no_match that is long or does not start like a command
        // may be dictation into a field that was not found (or not readable): type it, and keep it
        // on the clipboard. A short command-like miss is reported and copied.
        if no_match && !phrase.addressed && (phrase.words > 4 || !oasis_core::voice::command::classifier::looks_like_command(&phrase.command)) {
            ctx.shared.emit(EngineEvent::Note { text: "That matched no command, so it was typed as text and kept on the clipboard.".into() });
            return Spoken::NotACommand;
        }
        if no_match && ctx.verified && ctx.shared.live.load(Ordering::SeqCst) {
            let _ = oasis_win::clipboard::set_text(transcript);
            ctx.shared.emit(EngineEvent::Note { text: "What you said is on the clipboard, in case it was meant as text.".into() });
        }
        return Spoken::Handled(ask);
    };
    for app in &missing {
        ctx.shared.emit(EngineEvent::Note { text: format!("There is no app called \"{app}\" on this PC; the rest still runs.") });
    }
    if !ctx.verified {
        say("blocked", &routed.label, "Voice actions are off: profiles.json did not match its signature.");
        return Spoken::Handled(None);
    }

    if starts_a_shell(&action) {
        log.write(&format!("Voice launch refused, it starts a shell: \"{text}\" → {}", action.value));
        say("blocked", &routed.label, "Blocked for safety.");
        return Spoken::Handled(None);
    }
    let trusted = trusted_targets(ctx.settings);
    match fast_path(&action, &trusted, approval_mode(ctx.settings)) {
        FastPath::Refuse => {
            log.write(&format!("Voice fast path refused: \"{text}\" → {} / {}", action.kind, action.value));
            say("blocked", &routed.label, "Blocked for safety.");
            Spoken::Handled(None)
        }
        FastPath::Run => {
            run_voice_action(ctx.shared, ctx.jobs, &action, &routed.label, transcript);
            Spoken::Handled(None)
        }
        FastPath::Ask if !ctx.shared.live.load(Ordering::SeqCst) => {
            say("preview", &routed.label, "It would ask first. Actions are off, so nothing happened.");
            Spoken::Handled(None)
        }
        FastPath::Ask => {
            let steps = steps_of(&action);
            let summary = format!("Run “{}”?  Button 1: run · Button 2: cancel", routed.label);
            say("asking", &routed.label, &summary);
            Spoken::Handled(Some(Pending {
                action,
                steps,
                summary,
                heard: transcript.to_string(),
                label: routed.label,
                until: now.plus(CONFIRM_TIMEOUT),
            }))
        }
    }
}

/// `action` without the apps that are not on this PC, and their names. One app: no action at all.
/// Several ("open X on … and open Y on …"): the missing app's launch and the move of its window
/// go, and what is left runs, as long as it still opens something.
fn without_missing_apps(action: ButtonAction, exists: &dyn Fn(&str) -> bool) -> (Option<ButtonAction>, Vec<String>) {
    let is_launch = |kind: &str| kind.eq_ignore_ascii_case("LaunchApp");
    if !action.kind.eq_ignore_ascii_case("Automation") {
        if is_launch(&action.kind) && !exists(&action.value) {
            return (None, vec![action.value]);
        }
        return (Some(action), Vec::new());
    }
    let (mut missing, mut steps, mut skip_window) = (Vec::new(), Vec::new(), false);
    for step in action.steps {
        if is_launch(&step.kind) && !exists(&step.value) {
            missing.push(step.value);
            skip_window = true;
            continue;
        }
        if std::mem::take(&mut skip_window) && step.kind.eq_ignore_ascii_case("WindowCommand") {
            continue;
        }
        steps.push(step);
    }
    if !missing.is_empty() && !steps.iter().any(|s| is_launch(&s.kind)) {
        return (None, missing);
    }
    (Some(ButtonAction { kind: action.kind, value: action.value, steps }), missing)
}

/// Nothing sure was heard. If one thing came close, it is safe and it exists, ask about it on the
/// pad ("Did you mean …?"); never run it unasked. Otherwise say it was not understood.
fn offer_or_report(
    ctx: &SpokenContext<'_>,
    routed: &oasis_core::voice::command::RouteResult,
    running: &[RunningApp],
    installed: &dyn Fn(&str) -> bool,
    transcript: &str,
    now: Instant,
) -> (Option<Pending>, bool) {
    let say = |outcome: &str, label: &str, detail: &str| {
        ctx.shared.emit(EngineEvent::VoiceCommand {
            heard: transcript.to_string(),
            label: label.to_string(),
            outcome: outcome.to_string(),
            detail: detail.to_string(),
        });
    };
    let trusted = trusted_targets(ctx.settings);
    let offer = routed
        .suggestion
        .clone()
        .filter(|(a, _)| ctx.verified && fast_path(a, &trusted, approval_mode(ctx.settings)) != FastPath::Refuse)
        .filter(|(a, _)| !a.kind.eq_ignore_ascii_case("LaunchApp") || launch_target_exists(&a.value, running, installed));
    if let Some((action, label)) = offer {
        if !ctx.shared.live.load(Ordering::SeqCst) {
            say("preview", &label, "It would ask whether you meant this. Actions are off, so nothing happened.");
            return (None, false);
        }
        let summary = format!("Did you mean “{label}”?  Button 1: run · Button 2: cancel");
        say("asking", &label, &summary);
        let steps = steps_of(&action);
        return (Some(Pending { action, steps, summary, heard: transcript.to_string(), label, until: now.plus(CONFIRM_TIMEOUT) }), false);
    }
    // The app that is missing, also when it is one step of "open X on … and open Y on …".
    let missing = routed.action.as_ref().and_then(|a| {
        steps_of(a).into_iter().find(|(kind, value)| kind.eq_ignore_ascii_case("LaunchApp") && !launch_target_exists(value, running, installed))
    });
    let detail = match missing {
        Some((_, app)) => format!("There is no app called \"{app}\" on this PC."),
        None => routed.message.clone().unwrap_or_else(|| "No match.".into()),
    };
    say("no_match", "", &detail);
    SecurityLog::at(ctx.shared.paths.security_log()).write(&format!("Voice no_match: \"{transcript}\" ({detail})"));
    (None, true)
}

/// How often the app in front is looked at for the window's "just used" and "in front".
const FRONT_CHECK_EVERY: Duration = Duration::from_millis(500);

/// Whether the window in front is OASIS's own (the engine runs inside the desktop app).
fn is_oasis(info: &ForegroundInfo) -> bool {
    info.pid == Some(std::process::id())
}

/// A spoken launch whose name looks harmless but whose real target is a shell: "command" is the
/// Command Prompt shortcut, "node.js command prompt" starts `cmd.exe`. `fast_path` checks the name.
fn starts_a_shell(action: &ButtonAction) -> bool {
    #[cfg(windows)]
    {
        steps_of(action).iter().any(|(kind, value)| {
            kind.eq_ignore_ascii_case("LaunchApp")
                && oasis_platform::windows::app_discovery::launch_target(value)
                    .is_some_and(|t| oasis_core::voice::command::safety::names_a_shell(&t))
        })
    }
    #[cfg(not(windows))]
    {
        let _ = action;
        false
    }
}

/// The wall clock now, for "in 10 minutes" and "at 7 pm".
fn local_now() -> LocalTime {
    let unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
    #[cfg(windows)]
    let offset_secs = oasis_win::system::utc_offset_seconds();
    #[cfg(not(windows))]
    let offset_secs = 0;
    LocalTime { unix, offset_secs }
}

/// Schedules or cancels what a spoken alarm, timer or reminder phrase asked for. `false` when the
/// action is not one of those, so the caller runs it as usual.
fn handle_reminder_action(shared: &Shared, action: &ButtonAction) -> bool {
    let now = local_now();
    let note = |text: String| shared.emit(EngineEvent::Note { text });
    match action.kind.as_str() {
        "Reminder" => {
            let mut parts = action.value.splitn(3, '|');
            let (kind, due, label) = (parts.next().unwrap_or(""), parts.next().and_then(|d| d.parse::<i64>().ok()), parts.next().unwrap_or(""));
            match due.filter(|d| *d > 0 && matches!(kind, "alarm" | "timer" | "reminder")) {
                Some(due) if shared.reminders.lock().unwrap_or_else(|e| e.into_inner()).add(kind, due, label) => {
                    note(phrases::describe(kind, due, &phrases::flat(label), now));
                }
                Some(_) => note("Too many alarms, timers and reminders are waiting; cancel some first.".into()),
                None => note("That alarm, timer or reminder had no usable time.".into()),
            }
            true
        }
        "CancelReminders" => {
            let removed = shared.reminders.lock().unwrap_or_else(|e| e.into_inner()).cancel(&action.value);
            note(match removed {
                0 => "Nothing to cancel.".into(),
                1 => "Cancelled 1.".into(),
                n => format!("Cancelled {n}."),
            });
            true
        }
        _ => false,
    }
}

/// Everything that has come due: shown, and (when actions are on) sounded.
fn fire_due_reminders(shared: &Shared, jobs: &Sender<Job>) {
    let now = local_now().unix;
    let due = shared.reminders.lock().unwrap_or_else(|e| e.into_inner()).take_due(now);
    for item in due {
        let (title, body) = item.message(now);
        shared.emit(EngineEvent::Reminder { title, body });
        if shared.live.load(Ordering::SeqCst) {
            let action = ButtonAction { kind: "Chime".into(), value: item.chimes().to_string(), steps: vec![] };
            let _ = jobs.send(Job { action, button: -1, what: "sound the reminder".into(), allow_run_script: false, prepare: None });
        }
    }
}

/// Start a spoken command's action. It never runs a script, whatever the setting says.
fn run_voice_action(
    shared: &Shared,
    jobs: &Sender<Job>,
    action: &ButtonAction,
    label: &str,
    heard: &str,
) {
    let runs = shared.live.load(Ordering::SeqCst);
    SecurityLog::at(shared.paths.security_log())
        .write(&format!("Voice action: \"{heard}\" → {} / {label} / {}", action.kind, action.value));
    shared.emit(EngineEvent::VoiceCommand {
        heard: heard.to_string(),
        label: label.to_string(),
        outcome: if runs { "ran" } else { "preview" }.into(),
        detail: if runs { String::new() } else { "Actions are off, so nothing happened.".into() },
    });
    if runs {
        let what = format!("{} {}", action.kind, action.value);
        let _ = jobs.send(Job { action: action.clone(), button: -1, what, allow_run_script: false, prepare: None });
    }
}

/// What was heard goes to the focused app as typing, like a keyboard. In preview mode, or with a
/// bad signature, it is shown and not typed.
/// How the program in front wants dictation written (`docs/VOICE_DECISIONS.md` V7).
fn dictation_style(foreground: &mut dyn ForegroundProbe) -> Style {
    foreground.current().map_or(Style::Default, |info| {
        let context = ForegroundContext::from(&info);
        oasis_core::speech::style::for_app(&context.app_name, &context.url)
    })
}

/// `keep_on_clipboard`: no text field was found, so the paste may land nowhere; the text then stays on
/// the clipboard for the user to paste (the paste puts the old clipboard back, which is this text).
///
/// Cleanup and the S1-mini polish can take seconds, so they run on the action worker, in this job's
/// place in the queue: the engine loop keeps reading the pad, and a press or command that comes
/// meanwhile still runs after this text is pasted.
fn type_dictation(ctx: &SpokenContext<'_>, heard: Heard, style: Style, keep_on_clipboard: bool) {
    let (shared, settings, verified) = (Arc::clone(ctx.shared), ctx.settings.clone(), ctx.verified);
    let allow_run_script = settings.allow_run_script_actions;
    let prepare = move || dictation_text(&shared, &settings, verified, &heard, style, keep_on_clipboard);
    let action = ButtonAction { kind: "PasteText".into(), value: String::new(), steps: vec![] };
    let what = "type what was said".to_string();
    let _ = ctx.jobs.send(Job { action, button: -1, what, allow_run_script, prepare: Some(Box::new(prepare)) });
}

/// A dictation cleaned, polished, kept and shown; the text to paste, or `None` when it is only shown.
fn dictation_text(shared: &Shared, settings: &Settings, verified: bool, heard: &Heard, style: Style, keep_on_clipboard: bool) -> Option<String> {
    let raw = heard.text.as_str();
    let mut trace = Vec::new();
    let text = clean_dictation(settings, raw, style, &heard.paragraphs, &mut trace);
    if let Some(note) = crate::polish::take_note() {
        SecurityLog::at(shared.paths.security_log()).write(&note);
    }
    if VoiceSettings::from(settings).keep_history && !text.trim().is_empty() {
        let now = local_now();
        let saved = crate::history::save(&shared.paths.dictation_history_folder(), now.unix, now.offset_secs, &format!("{style:?}"), raw, &text, heard.audio.as_ref());
        if let Err(why) = saved {
            SecurityLog::at(shared.paths.security_log()).write(&format!("The dictation history could not be saved: {why}"));
        }
    }
    let voice = VoiceSettings::from(settings);
    let typed = if voice.paste_tag { format!("{text} [typed using project OASIS] ") } else { format!("{text} ") };
    // Paste off: still shown and kept, never typed or put on the clipboard.
    let verified = verified && voice.paste_dictation;
    if keep_on_clipboard && verified && shared.live.load(Ordering::SeqCst) {
        let _ = oasis_win::clipboard::set_text(&typed);
    }
    write_dictation_trace(shared, style, raw, &trace, &typed);
    show_typed(shared, settings, verified, &text).then_some(typed)
}

/// One block per dictation in `dictation-trace.log`: the speech model's raw text, the text after
/// every cleanup stage, and exactly what is pasted. Line breaks are written as `\n` inside quotes.
// shortcut: always on and holds what was said; make it a Voice setting (off by default) before release.
fn write_dictation_trace(shared: &Shared, style: Style, raw: &str, trace: &[(String, String)], typed: &str) {
    let mut block = format!("dictation, style {style:?}\n  {:<16} {raw:?}", "1 raw (STT)");
    let mut last = raw.trim().to_string();
    for (stage, text) in trace {
        let mark = if *text == last { "  (same)" } else { "" };
        block.push_str(&format!("\n  {stage:<16} {text:?}{mark}"));
        last = text.clone();
    }
    block.push_str(&format!("\n  {:<16} {typed:?}", "3 typed"));
    SecurityLog::at(shared.paths.dictation_trace_log()).write(&block);
}

/// What a close command would shut: the named app, or the window in front (`front`) when none is named.
fn close_target(action: &ButtonAction, front: &str) -> Option<String> {
    let named = if action.kind.eq_ignore_ascii_case("CloseApp") {
        action.value.trim()
    } else if action.kind.eq_ignore_ascii_case("WindowCommand") {
        action.value.to_lowercase().strip_prefix("close|").map(|_| action.value[6..].trim())?
    } else {
        return None;
    };
    Some(if !named.is_empty() { named } else if !front.is_empty() { front } else { "this window" }.to_string())
}

/// The click when the mic opens, and the one when the text lands. Both are trimmed to their sound.
/// The start click has no lead-in (it used to wait 400 ms); `VOICE_WAKE`, the near-silent noise
/// that was that lead-in, plays at the press instead, to wake an idle Bluetooth speaker (make_cues.py).
const VOICE_IN: &[u8] = include_bytes!("../assets/voice-in.wav");
const VOICE_OUT: &[u8] = include_bytes!("../assets/voice-out.wav");
const VOICE_WAKE: &[u8] = include_bytes!("../assets/voice-wake.wav");

/// Plays one of the two cues, unless Settings › Pad › Sound is off.
fn cue(settings: &Settings, wav: &'static [u8]) {
    if VoiceSettings::from(settings).sounds {
        oasis_win::system::play_wav(wav);
    }
}

/// Type `value` where the cursor is, and show `shown` in the activity list. In preview mode, or
/// with a bad signature, it is only shown.
fn paste_typed(shared: &Shared, settings: &Settings, verified: bool, jobs: &Sender<Job>, shown: &str, value: String) {
    if show_typed(shared, settings, verified, shown) {
        let action = ButtonAction { kind: "PasteText".into(), value, steps: vec![] };
        let allow_run_script = settings.allow_run_script_actions;
        let what = "type what was said".to_string();
        let _ = jobs.send(Job { action, button: -1, what, allow_run_script, prepare: None });
    }
}

/// Shows `shown` in the activity list and, when it will be typed, plays the end cue. True when it
/// is to be typed (actions on, signature good); nothing is shown for an empty text.
fn show_typed(shared: &Shared, settings: &Settings, verified: bool, shown: &str) -> bool {
    if shown.is_empty() {
        return false;
    }
    let runs = verified && shared.live.load(Ordering::SeqCst);
    shared.emit(EngineEvent::Transcript { text: shown.to_string(), typed: runs });
    if runs {
        cue(settings, VOICE_OUT);
    }
    runs
}

/// Dictation as it will be typed: spoken punctuation, numbers, corrections, fillers, the user's own
/// spellings, capitals and lists, unless the user turned cleanup off; then their replacements.
/// `paragraphs`: the opening words after each long pause. The whole text is cleaned at once, so a
/// correction or a list can reach across a pause; a break is then placed only where those words
/// still open a sentence (V7). "…by Friday. [pause] No, Saturday." has no sentence left to break at.
fn clean_dictation(settings: &Settings, raw: &str, style: Style, paragraphs: &[String], trace: &mut Vec<(String, String)>) -> String {
    let voice = VoiceSettings::from(settings);
    let mut context = CleanupContext::dictate(true);
    context.style = style;
    context.glossary = voice.dictation_glossary.iter().map(|(s, c)| GlossaryEntry::new(s, c)).collect();
    let mut result = if voice.dictation_cleanup {
        cleanup_pipeline::run_traced(&CleanupRequest::new(raw, context.clone()), trace)
    } else {
        trace.push(("cleanup off".into(), raw.trim().to_string()));
        cleanup_pipeline::identity(&CleanupRequest::new(raw.trim(), context.clone()))
    };
    if style != Style::Exact {
        result.corrected_text = oasis_core::speech::layout::place_paragraphs(&result.corrected_text, paragraphs);
        trace.push(("2 rules + pauses".into(), result.corrected_text.clone()));
        // S1-mini, when this PC passed its test (V16); guarded and time-limited (V18).
        if voice.dictation_cleanup {
            let keep: Vec<String> = voice.dictation_glossary.iter().map(|(_, c)| c.clone()).collect();
            result = crate::polish::apply(result, &context, &keep);
            trace.push(("S1-mini polish".into(), result.corrected_text.clone()));
            for why in crate::polish::take_rejections() {
                trace.push(("polish rejected".into(), why));
            }
        }
    }
    let typed = oasis_core::speech::glossary::replace(&result.corrected_text, &voice.replacements);
    trace.push(("Replacements".into(), typed.clone()));
    typed
}

/// Whether `button` is one the hidden YouTube gestures use (hold-2×, volume up or down).
fn is_youtube_gesture_button(settings: &Settings, button: i32) -> bool {
    let config = arbiter_config(settings, false);
    [config.hold2x, config.volume_up, config.volume_down].into_iter().flatten().any(|b| b.0 == button)
}

/// Settings > Preferences reach the action backend. The browser defaults to Chrome; a browser that is
/// not installed falls back to the system's default where it is used (W14).
fn sync_preferences(settings: &Settings) {
    let browser = settings.extra.get("Preferences").and_then(|p| p.get("Browser")).and_then(|b| b.as_str()).unwrap_or("chrome");
    #[cfg(windows)]
    oasis_platform::windows::actions::set_preferred_browser(browser);
    #[cfg(not(windows))]
    let _ = browser;
}

/// Bring the browser-extension socket in line with the settings: start it, move it to another
/// port, push new settings to a running one, or stop it.
fn sync_bridge(
    shared: &Arc<Shared>,
    config: Option<&BridgeConfig>,
    settings: &Settings,
    current: &mut Option<(BridgeServer, u16)>,
    youtube: &Sender<YouTubeWatchEvent>,
) {
    let wanted = config.filter(|_| bridge_enabled(settings));
    let port = config.and_then(|c| c.port).unwrap_or_else(|| bridge_port(settings));
    match (wanted, current.as_ref()) {
        (Some(_), Some((server, running))) if *running == port => {
            server.push_settings();
            return;
        }
        (None, None) => return,
        _ => {}
    }
    if let Some((mut server, _)) = current.take() {
        server.stop();
    }
    let Some(config) = wanted else { return };

    // Publish the extension with this user's token beside it, where the browser loads it from.
    if let Some(folder) = extension_install::ensure_published(&shared.paths, &config.bundled_extension) {
        if let Err(e) = write_token_files(&folder, &config.token) {
            shared.emit(EngineEvent::Note { text: format!("The browser extension's token file was not written: {e}") });
        }
    }

    let events = Arc::clone(shared);
    let tx = youtube.clone();
    let settings_source = Arc::clone(shared);
    let started = BridgeServer::start(
        port,
        config.token.clone(),
        Some(SecurityLog::at(shared.paths.security_log())),
        move || extension_settings(&settings_source.settings()),
        move |event| match event {
            BridgeEvent::YouTube(event) => {
                let _ = tx.send(event);
            }
            BridgeEvent::Status(text) => {
                let clients = events.status.lock().map(|s| s.bridge_clients).unwrap_or(0);
                events.update(|s| s.bridge_status = text.clone());
                events.emit(EngineEvent::BridgeStatus { text, clients });
            }
            BridgeEvent::Clients(clients) => {
                let text = events.status.lock().map(|s| s.bridge_status.clone()).unwrap_or_default();
                events.update(|s| s.bridge_clients = clients);
                events.emit(EngineEvent::BridgeStatus { text, clients });
            }
            BridgeEvent::Hello => {}
        },
    );
    match started {
        Ok(server) => *current = Some((server, port)),
        Err(e) => {
            let text = format!("ERROR · {e}");
            shared.update(|s| s.bridge_status = text.clone());
            shared.emit(EngineEvent::BridgeStatus { text, clients: 0 });
        }
    }
}

fn report_telemetry(shared: &Shared, link: &PadLink) {
    let t = link.telemetry();
    shared.update(|s| {
        s.battery_percent = t.battery_percent;
        s.charging = t.is_charging;
        s.latency_ms = t.link_latency_ms;
    });
    if let Some(percent) = t.battery_percent {
        shared.emit(EngineEvent::Battery { percent, charging: t.is_charging, full: t.is_full });
        let (warn, warned) = oasis_core::battery::low_battery_check(percent, t.is_charging, shared.low_battery_warned.load(Ordering::SeqCst));
        shared.low_battery_warned.store(warned, Ordering::SeqCst);
        if warn {
            shared.emit(EngineEvent::Reminder { title: "OASIS — Low battery".into(), body: format!("Device battery is at {percent}%. Charge soon.") });
        }
    }
}

/// Whether a button has anything to run (a blank button shows a press and does nothing).
fn is_runnable(action: &ButtonAction) -> bool {
    matches!(action.kind.as_str(), "Automation" | "JoinMeeting") || !action.value.trim().is_empty()
}

/// The active profile's name and the three button labels to show, if labels are wanted for it
/// (the profile's own label mode can force them on or off over the general setting).
/// With `tapped`, the labels are only wanted if that button has something to run, as in C#.
fn peek_labels(
    settings: &Settings,
    foreground: &mut dyn ForegroundProbe,
    tapped: Option<i32>,
    watch_token: &str,
) -> Option<(String, Vec<String>)> {
    // Labels off everywhere: skip the fresh foreground read below, so the tap runs at once.
    let can_show = settings.show_overlay_labels
        || settings.profiles.iter().any(|p| {
            [p.label_mode.as_str()].into_iter().chain(p.config_profiles.iter().map(|c| c.label_mode.as_str()))
                .any(|m| LabelPolicy::effective(false, Some(m)))
        });
    if !can_show {
        return None;
    }
    let info = foreground.current_for_press().ok()?;
    let context = ForegroundContext::from(&info);
    let active = resolve_active_profile(&settings.profiles, &context, watch_token)?;
    let profile = &settings.profiles[active.profile];
    let config = &profile.config_profiles[active.config?];
    // The set's own mode wins over the profile's; blank follows the profile.
    let mode = if config.label_mode.trim().is_empty() { &profile.label_mode } else { &config.label_mode };
    if !LabelPolicy::effective(settings.show_overlay_labels, Some(mode)) {
        return None;
    }
    if let Some(button) = tapped {
        let action = config.buttons.get(usize::try_from(button).ok()?)?.to_action();
        if !is_runnable(&action) {
            return None;
        }
    }
    let labels = (0..3)
        .map(|i| config.buttons.get(i).map(|b| b.label.trim().to_string()).unwrap_or_default())
        .collect();
    Some((profile.name.clone(), labels))
}

/// A recognised gesture and what YouTube was doing when it happened.
struct Gesture<'a> {
    button: i32,
    claim: GestureClaimId,
    watch_token: &'a str,
}

/// A gesture was recognised: work out what it means for the app in front and start it.
fn dispatch(
    shared: &Shared,
    settings: &Settings,
    verified: bool,
    foreground: &mut dyn ForegroundProbe,
    jobs: &Sender<Job>,
    gesture: Gesture<'_>,
) {
    let Gesture { button, claim, watch_token } = gesture;
    let number = (button + 1).clamp(0, 255) as u8;
    let Some((press, action)) = plan(settings, foreground, button, claim, watch_token) else {
        shared.emit(EngineEvent::Unmapped { button: number, gesture: claim.as_str().to_string() });
        return;
    };

    let runs = verified && shared.live.load(Ordering::SeqCst) && is_runnable(&action);
    let press = Press { runs, ..press };
    if let Err(e) = shared.use_counts.lock().unwrap_or_else(|e| e.into_inner()).bump(&press.profile) {
        shared.emit(EngineEvent::Note { text: format!("Could not save how often profiles are used: {e}") });
    }
    shared.update(|s| s.last_press = Some(press.clone()));
    shared.emit(EngineEvent::Press(press.clone()));

    if runs {
        let what = format!("{} {}", press.kind, press.value);
        let allow_run_script = settings.allow_run_script_actions;
        let _ = jobs.send(Job { action, button, what, allow_run_script, prepare: None });
    }
}

/// The press and action for a gesture, or `None` if it is not wired.
fn plan(
    settings: &Settings,
    foreground: &mut dyn ForegroundProbe,
    button: i32,
    claim: GestureClaimId,
    watch_token: &str,
) -> Option<(Press, ButtonAction)> {
    let number = (button + 1).clamp(0, 255) as u8;
    match claim {
        GestureClaimId::ShowDesktop => Some((
            Press {
                button: number,
                profile: "(hold)".into(),
                config: "show desktop".into(),
                kind: "Shortcut".into(),
                value: "Win+D".into(),
                runs: false,
            },
            ButtonAction { kind: "Shortcut".into(), value: "Win+D".into(), steps: vec![] },
        )),
        GestureClaimId::BoundTap => {
            let info = foreground.current_for_press().ok()?;
            let context = ForegroundContext::from(&info);
            let active = resolve_active_profile(&settings.profiles, &context, watch_token)?;
            let profile = &settings.profiles[active.profile];
            let config = &profile.config_profiles[active.config?];
            let action = config.buttons.get(usize::try_from(button).ok()?)?.to_action();
            let press = Press {
                button: number,
                profile: profile.name.clone(),
                config: config.name.clone(),
                kind: action.kind.clone(),
                value: action.value.clone(),
                runs: false,
            };
            Some((press, action))
        }
        // Double-click on a volume button while a YouTube video plays: the system volume.
        GestureClaimId::DoubleClickVolume => {
            let up = arbiter_config(settings, false).volume_up.is_some_and(|b| b.0 == button);
            let key = if up { "VolumeUp" } else { "VolumeDown" };
            Some((
                Press {
                    button: number,
                    profile: "(YouTube)".into(),
                    config: "volume".into(),
                    kind: "Shortcut".into(),
                    value: key.into(),
                    runs: false,
                },
                ButtonAction { kind: "Shortcut".into(), value: key.into(), steps: vec![] },
            ))
        }
        _ => None,
    }
}

// ---- the worker ------------------------------------------------------------------------------

fn spawn_worker(
    inbox: Receiver<Job>,
    make_backend: BackendFactory,
    shared: Arc<Shared>,
    security_log: std::path::PathBuf,
    quick_launch: Option<crate::quick_launch::QuickLaunchParts>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("oasis-actions".into())
        .spawn(move || {
            // One action at a time, in order. A long step blocks only this thread; the engine
            // keeps reading the pad.
            for mut job in inbox {
                let _busy = crate::activity::work();
                if let Some(prepare) = job.prepare.take() {
                    match prepare() {
                        Some(text) => job.action.value = text,
                        None => continue,
                    }
                }
                if handle_reminder_action(&shared, &job.action) {
                    continue;
                }
                if job.action.kind.starts_with("Spotify") {
                    run_spotify(&shared, &job.action, &job.what);
                    continue;
                }
                let mut backend = make_backend(BackendContext {
                    security_log: SecurityLog::at(security_log.clone()),
                    allow_run_script: job.allow_run_script,
                });
                if job.action.kind == "QuickLaunch" {
                    run_quick_launch(&shared, backend.as_mut(), &job.action.value, quick_launch.as_ref());
                    continue;
                }
                run_job_action(&shared, backend.as_mut(), &job, quick_launch.as_ref());
            }
        })
        .expect("a thread can be spawned")
}

/// Runs one button's action. A Quick Launch step is an ordinary step (see [`JoinAware`]), so it can
/// sit anywhere in an automation, branches included.
fn run_job_action(
    shared: &Shared,
    backend: &mut dyn ActionBackend,
    job: &Job,
    quick: Option<&crate::quick_launch::QuickLaunchParts>,
) {
    let join = || join_meeting_now(shared);
    let mut aware = JoinAware {
        inner: backend,
        join: &join,
        shared,
        quick,
        problem: None,
    };
    let (action, what) = (&job.action, &job.what);
    match run_action(&mut aware, action, job.button) {
        ActionOutcome::Ignored | ActionOutcome::Single(true) => {}
        ActionOutcome::Single(false) => {
            let why = aware.take_problem().unwrap_or_else(|| "it did not run".into());
            report_failure(shared, what, &why)
        },
        ActionOutcome::Automation(result) => {
            if let Some(message) = result.failure_message {
                report_failure(shared, what, &message);
            }
        }
    }
}

/// Runs actions on the real backend, except that a "join meeting" step joins the meeting that is
/// due, through the calendar the engine holds, and a Quick Launch step opens a setup the engine holds.
struct JoinAware<'a> {
    inner: &'a mut dyn ActionBackend,
    join: &'a dyn Fn() -> bool,
    shared: &'a Shared,
    quick: Option<&'a crate::quick_launch::QuickLaunchParts>,
    /// Why the last layout action failed (W9); other failures come from `inner`.
    problem: Option<String>,
}

impl ActionBackend for JoinAware<'_> {
    fn send_shortcut(&mut self, chord: &oasis_core::shortcut::Chord) -> bool {
        self.inner.send_shortcut(chord)
    }
    fn open_url(&mut self, url: &str) -> bool {
        self.inner.open_url(url)
    }
    fn launch_browser_url(&mut self, browser: &str, url: &str) -> bool {
        self.inner.launch_browser_url(browser, url)
    }
    fn launch_app(&mut self, query: &str) -> bool {
        self.inner.launch_app(query)
    }
    fn take_problem(&mut self) -> Option<String> {
        self.problem.take().or_else(|| self.inner.take_problem())
    }
    fn window_action(&mut self, kind: &str, value: &str) -> bool {
        if kind != "WindowLayout" {
            return self.inner.window_action(kind, value);
        }
        match run_layout(self.shared, value, &mut |id| self.quick_launch(id)) {
            Ok(()) => true,
            Err(why) => {
                self.problem = Some(why);
                false
            }
        }
    }
    fn close_app(&mut self, process: &str) -> bool {
        self.inner.close_app(process)
    }
    fn run_script(&mut self, script: &str) {
        self.inner.run_script(script);
    }
    fn run_autohotkey(&mut self, script: &str) -> bool {
        self.inner.run_autohotkey(script)
    }
    fn mouse_click(&mut self, click: oasis_core::actions::MouseClick) -> bool {
        self.inner.mouse_click(click)
    }
    fn paste_text(&mut self, text: &str) -> bool {
        self.inner.paste_text(text)
    }
    fn join_meeting(&mut self) -> bool {
        (self.join)()
    }
    fn system(&mut self, command: &str) -> bool {
        self.inner.system(command)
    }
    fn pause(&mut self, duration: Duration) {
        self.inner.pause(duration);
    }
    fn quick_launch(&mut self, id: &str) -> bool {
        run_quick_launch(self.shared, self.inner, id, self.quick)
    }
    fn front_window(&mut self) -> Option<isize> {
        self.inner.front_window()
    }
    fn wait_until_ready(&mut self, before: Option<isize>, limit: Duration) -> bool {
        self.inner.wait_until_ready(before, limit)
    }
    fn run_script_allowed(&self) -> bool {
        self.inner.run_script_allowed()
    }
    fn security_log(&mut self, message: &str) {
        self.inner.security_log(message);
    }
}

/// Joins the meeting that is starting or running, and says which (or why not).
fn join_meeting_now(shared: &Shared) -> bool {
    let Some(meetings) = shared.meetings.as_deref() else {
        shared.emit(EngineEvent::Note { text: "Meetings are not set up in this build.".into() });
        return false;
    };
    let window = oasis_settings::meetings::MeetingSettings::from(&shared.settings()).join_window_minutes_before;
    match meetings.join_now(crate::spotify::now_unix(), window) {
        Ok(text) => {
            shared.emit(EngineEvent::Note { text });
            true
        }
        Err(reason) => {
            shared.emit(EngineEvent::Note { text: reason });
            false
        }
    }
}

/// Runs a Spotify command or play request and says how it went.
fn run_spotify(shared: &Shared, action: &ButtonAction, what: &str) {
    let Some(spotify) = shared.spotify.as_deref() else {
        report_failure(shared, what, "Spotify is not set up in this build");
        return;
    };
    match spotify.run(&action.kind, &action.value) {
        Ok(text) => shared.emit(EngineEvent::Note { text }),
        Err(reason) => report_failure(shared, what, &reason),
    }
}

/// Opens every item of a Quick Launch setup, one after another. One that fails does not stop the
/// rest; the summary says which did. Each item runs through the same rules as any spoken command.
/// `true` when the setup exists and every item opened.
fn run_quick_launch(
    shared: &Shared,
    backend: &mut dyn ActionBackend,
    id: &str,
    quick: Option<&crate::quick_launch::QuickLaunchParts>,
) -> bool {
    let found = {
        let settings = shared.settings();
        oasis_settings::quick_launch::find(&settings, id)
    };
    let Some(setup) = found else {
        report_failure(
            shared,
            "quick launch",
            "that setup is no longer in the settings",
        );
        return false;
    };
    if let Some(parts) = quick {
        let (setup, skipped) = oasis_settings::quick_launch::without_blocked(&setup);
        let results = crate::quick_launch::Runner::new(parts, crate::quick_launch::Timing::REAL).run(&setup);
        let mut text = crate::quick_launch::summary(&setup.name, &results);
        if !skipped.is_empty() {
            text.push_str(&format!(" {}.", skipped.join("; ")));
        }
        shared.emit(EngineEvent::Note { text });
        return results
            .iter()
            .all(|r| r.outcome != crate::quick_launch::Outcome::FailedToOpen);
    }
    let (action, skipped) = oasis_settings::quick_launch::to_action(&setup);
    let (mut opened, mut failed) = (0usize, skipped);
    for step in &action.steps {
        if step.kind == "Delay" {
            backend.pause(Duration::from_millis(step.value.parse().unwrap_or(0)));
            continue;
        }
        let single = ButtonAction { kind: step.kind.clone(), value: step.value.clone(), steps: vec![] };
        if matches!(run_action(backend, &single, -1), ActionOutcome::Single(true)) {
            opened += 1;
        } else {
            failed.push(format!("{} did not open", step.value));
        }
    }
    let mut text = format!("{}: {opened} of {} ready.", setup.name, setup.items.len());
    if !failed.is_empty() {
        text.push(' ');
        text.push_str(&failed.join("; "));
        text.push('.');
    }
    shared.emit(EngineEvent::Note { text });
    failed.is_empty()
}

fn report_failure(shared: &Shared, what: &str, reason: &str) {
    shared.update(|s| s.last_error = Some(format!("{what}: {reason}")));
    shared.emit(EngineEvent::ActionFailed { what: what.to_string(), reason: reason.to_string() });
}

// ---- spoken layouts (docs/APP_WINDOWS.md W9) -----------------------------------------------------

/// The Quick Launch setup a layout is kept as: "work" is "work layout"; nothing said is "my layout".
fn layout_name(spoken: &str) -> String {
    let name = spoken.trim().to_lowercase();
    let name = if name.is_empty() { "my layout".to_string() } else if name.ends_with("layout") { name } else { format!("{name} layout") };
    name.chars().take(40).collect()
}

/// "save this layout [as work]" records every open app window, its screen and its place, as a Quick
/// Launch setup. "restore [work] layout" runs that setup: apps not running are started, then placed.
/// `value` is `save|name` or `restore|name`.
fn run_layout(shared: &Shared, value: &str, restore: &mut dyn FnMut(&str) -> bool) -> Result<(), String> {
    let (verb, spoken) = value.split_once('|').unwrap_or((value, ""));
    let name = layout_name(spoken);
    match verb {
        "save" => {
            #[cfg(windows)]
            let mut items = crate::defaults::windows::capture_items();
            #[cfg(not(windows))]
            let mut items: Vec<oasis_settings::quick_launch::ItemEdit> = Vec::new();
            if items.is_empty() {
                return Err("There are no windows to save.".into());
            }
            items.truncate(40);
            let count = items.len();
            let id = oasis_settings::quick_launch::find(&shared.settings(), &name).map(|s| s.id).unwrap_or_default();
            let edit = oasis_settings::quick_launch::SetupEdit { id, name: name.clone(), call_name: String::new(), start_with_oasis: false, items };
            shared.change(|s| s.save_setup(edit).map_err(EditError::Invalid)).map_err(|e| e.to_string())?;
            shared.emit(EngineEvent::Note { text: format!("Saved {count} windows as “{name}”. Say “restore {name}” to put them back.") });
            Ok(())
        }
        "restore" => {
            let found = oasis_settings::quick_launch::find(&shared.settings(), &name);
            let Some(setup) = found else {
                return Err(format!("There is no saved layout called “{name}”. Say “save my layout” first."));
            };
            if restore(&setup.id) { Ok(()) } else { Err(format!("Could not restore “{name}”.")) }
        }
        other => Err(format!("I do not know the layout command '{other}'.")),
    }
}

#[cfg(test)]
mod layout_tests {
    use super::layout_name;

    #[test]
    fn a_layout_is_kept_under_its_spoken_name() {
        assert_eq!(layout_name(""), "my layout");
        assert_eq!(layout_name("Work"), "work layout");
        assert_eq!(layout_name("work layout"), "work layout");
        assert_eq!(layout_name(&"x".repeat(60)).chars().count(), 40);
    }
}

#[cfg(test)]
mod close_target_tests {
    use super::{close_target, ButtonAction};

    fn act(kind: &str, value: &str) -> ButtonAction {
        ButtonAction { kind: kind.into(), value: value.into(), steps: vec![] }
    }

    #[test]
    fn a_close_names_its_app_else_the_window_in_front_else_this_window() {
        assert_eq!(close_target(&act("CloseApp", "WhatsApp"), "x").as_deref(), Some("WhatsApp"));
        assert_eq!(close_target(&act("WindowCommand", "close|chrome"), "x").as_deref(), Some("chrome"));
        assert_eq!(close_target(&act("WindowCommand", "close|"), "Notepad").as_deref(), Some("Notepad"));
        assert_eq!(close_target(&act("WindowCommand", "close|"), "").as_deref(), Some("this window"));
        assert_eq!(close_target(&act("WindowCommand", "maximize|chrome"), "x"), None);
        assert_eq!(close_target(&act("LaunchApp", "chrome"), "x"), None);
    }
}

#[cfg(test)]
mod memory_line_tests {
    use super::memory_line;

    #[test]
    fn the_memory_line_shows_megabytes_and_handles_or_a_dash() {
        assert_eq!(
            memory_line(Some((785 * 1024 * 1024 + 5, 826)), Some((1179 * 1024 * 1024, 400))),
            "memory oasis private_mb=785 handles=826 | polish-server private_mb=1179 handles=400"
        );
        assert_eq!(memory_line(Some((2 * 1024 * 1024, 9)), None), "memory oasis private_mb=2 handles=9 | polish-server private_mb=- handles=-");
    }
}

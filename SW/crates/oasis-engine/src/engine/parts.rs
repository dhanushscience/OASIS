//! What the engine is given: the foreground probe, the action backend, the parts, the config, the errors.

use super::*;

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

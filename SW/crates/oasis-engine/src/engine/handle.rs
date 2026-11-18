//! The state the loop and the workers share, `EngineHandle`, and `start`.

use super::*;

// ---- shared state ----------------------------------------------------------------------------

pub(super) type Emit = Arc<dyn Fn(EngineEvent) + Send + Sync>;

/// Lock order, everywhere: `settings`, then `store`. Never the other way round.
pub(super) struct Shared {
    pub(super) stop: AtomicBool,
    pub(super) live: AtomicBool,
    /// False if `profiles.json` failed its signature check and was not accepted.
    pub(super) integrity_ok: AtomicBool,
    /// The settings in force. The loop re-reads them when `generation` changes.
    pub(super) settings: Mutex<Settings>,
    pub(super) generation: AtomicU64,
    /// The keyboard buttons are held back (a guide is on screen). In memory only, never saved, so a
    /// crash or a quit cannot leave them dead.
    pub(super) keys_held: AtomicBool,
    pub(super) store: Mutex<ProfileStore>,
    pub(super) paths: OasisPaths,
    pub(super) status: Mutex<EngineStatus>,
    pub(super) emit: Emit,
    /// Spotify commands. `None` leaves them out.
    pub(super) spotify: Option<Arc<dyn crate::spotify::Runner>>,
    /// Meetings from the calendar. `None` leaves them out.
    pub(super) meetings: Option<Arc<crate::meetings::Meetings>>,
    /// The low-battery warning has been given for this stretch of low charge.
    pub(super) low_battery_warned: AtomicBool,
    /// Alarms, timers and reminders waiting to go off.
    pub(super) reminders: Mutex<Reminders>,
    /// Presses per profile, for ordering profiles by use.
    pub(super) use_counts: Mutex<UseCounts>,
    /// The loop's inbox: it sleeps until something arrives here or the next thing is due.
    pub(super) inbox: Sender<Input>,
}

impl Shared {
    pub(super) fn update(&self, change: impl FnOnce(&mut EngineStatus)) {
        change(&mut self.status.lock().unwrap_or_else(|e| e.into_inner()));
    }

    pub(super) fn emit(&self, event: EngineEvent) {
        (self.emit)(event);
    }

    /// Have the loop look again now (new settings, keys held, stop) instead of at its next deadline.
    fn wake(&self) {
        let _ = self.inbox.send(Input::Wake);
    }

    pub(super) fn settings(&self) -> MutexGuard<'_, Settings> {
        self.settings.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(super) fn store(&self) -> MutexGuard<'_, ProfileStore> {
        self.store.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Apply an edit to a copy, save it, and only then put it in force (see `EngineHandle::change`).
    pub(super) fn change<R>(
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
        self.wake();
        self.update(|s| s.profiles_loaded = profiles);
        self.emit(EngineEvent::SettingsChanged);
    }

    /// Read `profiles.json` again and put it in force, re-checking its signature.
    pub(super) fn reload_from_disk(&self) -> Result<(), String> {
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

// ---- the handle ------------------------------------------------------------------------------

pub struct EngineHandle {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    /// Lets "Try it" start a button's action. Dropped first on stop, or the worker never ends.
    jobs: Option<Jobs>,
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
        self.shared.wake();
    }

    /// Keep a `profiles.json` that was edited outside OASIS: the signature mismatch is accepted,
    /// actions can run, and the next save signs the file again. The user's choice, never automatic.
    pub fn accept_edited_settings(&self) {
        let _current = self.shared.settings();
        self.shared.store().accept_edited();
        drop(_current);
        self.shared.set_integrity(true);
        self.shared.generation.fetch_add(1, Ordering::SeqCst);
        self.shared.wake();
        self.shared.emit(EngineEvent::Note {
            text: "Accepted the edited profiles.json. It will be signed again the next time settings are saved."
                .into(),
        });
    }

    /// Stop the loop and wait for it. Safe to call twice.
    pub fn stop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        self.shared.wake();
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
    let (inbox_tx, loop_inbox) = mpsc::channel::<Input>();

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
        inbox: inbox_tx,
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
    let (ordered_tx, inbox) = mpsc::channel::<Job>();
    let (free_tx, free_inbox) = mpsc::channel::<Job>();
    let jobs = Jobs { ordered: ordered_tx, free: free_tx };
    let try_jobs = jobs.clone();

    // Both workers build their backend from the same factory; the lock is held only for that call.
    let make_backend = Arc::new(Mutex::new(parts.make_backend));
    let worker = spawn_worker(
        "oasis-actions",
        inbox,
        Arc::clone(&make_backend),
        Arc::clone(&shared),
        config.paths.security_log(),
        parts.quick_launch.clone(),
    );
    let free_worker = spawn_worker(
        "oasis-actions-free",
        free_inbox,
        make_backend,
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
                loop_inbox,
                LoopExtras {
                    bridge_config,
                    voice_parts,
                    startup_setup_at,
                    buttons: button_source,
                },
            );
            drop(jobs); // ends the worker
            let _ = worker.join();
            let _ = free_worker.join();
            loop_shared.update(|s| s.running = false);
            loop_shared.emit(EngineEvent::Stopped);
        })
        .expect("a thread can be spawned");

    Ok(EngineHandle { shared, thread: Some(thread), jobs: Some(try_jobs) })
}

pub(super) fn load_settings(
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

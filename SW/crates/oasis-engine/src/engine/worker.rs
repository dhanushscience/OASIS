//! The two action workers (ordered and focus-free lanes) and the runners they call: meetings, Spotify, Quick Launch.

use super::*;

pub(super) struct Job {
    pub(super) action: ButtonAction,
    pub(super) button: i32,
    pub(super) what: String,
    /// The "allow RunScript actions" setting when this press happened.
    pub(super) allow_run_script: bool,
    /// Work to do on the worker before the action, in this job's place in the queue: a dictation's
    /// cleanup and polish (seconds), which must not hold up the engine loop. It returns the text to
    /// paste, or `None` when nothing is to be pasted.
    pub(super) prepare: Option<Box<dyn FnOnce() -> Option<String> + Send>>,
}

/// Where jobs go: the ordered lane (one at a time, so "open Word" finishes before the paste into
/// it), or a second lane for jobs that do not care which window is in front and that nothing
/// queued later depends on, so a slow "open app" does not hold up a volume key or a timer.
#[derive(Clone)]
pub(super) struct Jobs {
    pub(super) ordered: Sender<Job>,
    pub(super) free: Sender<Job>,
}

impl Jobs {
    pub(super) fn send(&self, job: Job) -> Result<(), mpsc::SendError<Job>> {
        // A job with `prepare` pastes dictation, so it always stays in order.
        let lane = if job.prepare.is_none() && oasis_core::actions::is_focus_free(&job.action) { &self.free } else { &self.ordered };
        lane.send(job)
    }
}

// ---- the worker ------------------------------------------------------------------------------

pub(super) fn spawn_worker(
    name: &str,
    inbox: Receiver<Job>,
    make_backend: Arc<Mutex<BackendFactory>>,
    shared: Arc<Shared>,
    security_log: std::path::PathBuf,
    quick_launch: Option<crate::quick_launch::QuickLaunchParts>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name(name.into())
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
                if ActionKind::is_spotify_text(&job.action.kind) {
                    run_spotify(&shared, &job.action, &job.what);
                    continue;
                }
                let mut backend = (make_backend.lock().unwrap_or_else(|e| e.into_inner()))(BackendContext {
                    security_log: SecurityLog::at(security_log.clone()),
                    allow_run_script: job.allow_run_script,
                });
                if job.action.kind() == ActionKind::QuickLaunch {
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
pub(super) fn run_job_action(
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
pub(super) struct JoinAware<'a> {
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
        if ActionKind::of(kind) != ActionKind::WindowLayout {
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
pub(super) fn join_meeting_now(shared: &Shared) -> bool {
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
pub(super) fn run_spotify(shared: &Shared, action: &ButtonAction, what: &str) {
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
pub(super) fn run_quick_launch(
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
        if step.kind() == ActionKind::Delay {
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

pub(super) fn report_failure(shared: &Shared, what: &str, reason: &str) {
    shared.update(|s| s.last_error = Some(format!("{what}: {reason}")));
    shared.emit(EngineEvent::ActionFailed { what: what.to_string(), reason: reason.to_string() });
}

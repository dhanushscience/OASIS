//! What wakes the engine loop, and the two threads that keep slow reads off it.
//!
//! The loop sleeps on one channel until something comes in or the next thing is due. Two kinds of
//! work used to hold it for a long time, so each now has a thread of its own:
//! - the pad thread scans for the pad (0.3 to 1.5 s a look) and reads it;
//! - the front thread reads which app, page and panel are in front (UI Automation, up to a
//!   second) and asks whether a text field has the focus (up to 250 ms).
//!
//! The loop stays the only writer of the gesture rules and the voice session: these threads only
//! post what they read.

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant as StdInstant};

use oasis_core::bridge::YouTubeWatchEvent;
use oasis_core::matching::ForegroundContext;
use oasis_core::Instant;
use oasis_platform::pad_connector::{ConnectorEvent, PadConnector};
use oasis_platform::ports::{ForegroundInfo, PadTransport, PlatformResult};
use oasis_settings::Settings;

use crate::engine::{dictation_style, ForegroundProbe};

/// Everything that wakes the loop.
pub(crate) enum Input {
    /// Look again: keyboard buttons are waiting, the settings changed, or it is time to stop.
    Wake,
    Pad(ConnectorEvent),
    YouTube(YouTubeWatchEvent),
    /// The app in front, when it changed.
    Front(ForegroundInfo),
    /// The answer to a `FrontAsk::Press`, in the order asked. `None`: no read was asked for, or it failed.
    ForPress(Option<ForegroundInfo>),
    /// The answer to a `FrontAsk::Capture`: whether the recording is typed, and how long the
    /// focus question took.
    Capture { dictate: bool, focus_ms: u128 },
}

/// What the loop asks the front thread. Answered one at a time, in order.
pub(crate) enum FrontAsk {
    /// Which profiles exist decides what is worth reading (see `ForegroundProbe`).
    Profiles { url: bool, panel: bool },
    /// A press is waiting: read what is in front now (`read`), or just answer in turn.
    Press { read: bool },
    /// A recording is starting: typed or a command?
    Capture(Box<Settings>),
}

/// How long the pad thread rests while there is no pad to read: between scans the connector
/// answers at once, and without a rest the thread would spin.
const PAD_REST: Duration = Duration::from_millis(50);

/// Owns the pad connector: scans, connects, reads, and posts each event. `power` switches looking
/// for the pad on (`true`) or off; it starts on. Ends when the loop goes away.
pub(crate) fn spawn_pad<T: PadTransport + 'static>(
    mut connector: PadConnector<T>,
    start: StdInstant,
    power: Receiver<bool>,
    inbox: Sender<Input>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("oasis-pad".into())
        .spawn(move || {
            let send = |events: Vec<ConnectorEvent>| events.into_iter().all(|e| inbox.send(Input::Pad(e)).is_ok());
            let (mut on, mut rest) = (true, Duration::ZERO);
            loop {
                // Every switch is taken before the next look. While off there is nothing to do but wait.
                let told = if on { power.recv_timeout(rest) } else { power.recv().map_err(|_| RecvTimeoutError::Disconnected) };
                match told {
                    Ok(wanted) => {
                        if on && !wanted && !send(connector.pause()) {
                            return;
                        }
                        (on, rest) = (wanted, Duration::ZERO);
                        continue;
                    }
                    Err(RecvTimeoutError::Disconnected) => return,
                    Err(RecvTimeoutError::Timeout) => {}
                }
                let polled = StdInstant::now();
                if !send(connector.poll(Instant::from_millis(start.elapsed().as_millis() as i64))) {
                    return;
                }
                rest = if !connector.is_connected() && polled.elapsed() < PAD_REST { PAD_REST } else { Duration::ZERO };
            }
        })
        .expect("a thread can be spawned")
}

/// How often the front thread looks at what is in front. Cheap while nothing changed (no UI
/// Automation then); it only keeps the YouTube gestures and the status window up to date. A press
/// never relies on it: it asks for a fresh read.
const FRONT_WATCH_EVERY: Duration = Duration::from_millis(100);

/// Owns the reads of what is in front. Posts `Front` when it changed, and answers each ask.
pub(crate) fn spawn_front(
    probe: SharedProbe,
    editable_focus: Box<dyn Fn() -> bool + Send>,
    asks: Receiver<FrontAsk>,
    inbox: Sender<Input>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("oasis-front".into())
        .spawn(move || {
            let mut probe = probe;
            let mut last: Option<ForegroundInfo> = None;
            let mut next_watch = StdInstant::now();
            loop {
                let (read, answer) = match asks.recv_timeout(next_watch.saturating_duration_since(StdInstant::now())) {
                    Err(RecvTimeoutError::Disconnected) => return,
                    Err(RecvTimeoutError::Timeout) => {
                        next_watch = StdInstant::now() + FRONT_WATCH_EVERY;
                        (probe.current().ok(), None)
                    }
                    Ok(FrontAsk::Profiles { url, panel }) => {
                        probe.set_url_profiles_exist(url);
                        probe.set_panel_profiles_exist(panel);
                        (None, None)
                    }
                    Ok(FrontAsk::Press { read }) => {
                        let info = if read { probe.current_for_press().ok() } else { None };
                        (info.clone(), Some(Input::ForPress(info)))
                    }
                    Ok(FrontAsk::Capture(settings)) => {
                        let info = probe.current().ok();
                        crate::polish::set_front_style(dictation_style(&mut probe));
                        let process = info.as_ref().map(|i| ForegroundContext::from(i).app_name).unwrap_or_default();
                        let asked = StdInstant::now();
                        let dictate = crate::voice::wants_dictation(&settings, &process, editable_focus.as_ref());
                        (info, Some(Input::Capture { dictate, focus_ms: asked.elapsed().as_millis() }))
                    }
                };
                // Any read keeps the loop's idea of what is in front fresh; only a change is sent.
                if let Some(info) = read.filter(|info| last.as_ref() != Some(info)) {
                    last = Some(info.clone());
                    if inbox.send(Input::Front(info)).is_err() {
                        return;
                    }
                }
                if answer.is_some_and(|answer| inbox.send(answer).is_err()) {
                    return;
                }
            }
        })
        .expect("a thread can be spawned")
}

/// The one foreground probe, shared by the front thread and the spoken-command handling on the
/// loop. Each call holds it only for that read.
#[derive(Clone)]
pub(crate) struct SharedProbe(pub(crate) Arc<Mutex<Box<dyn ForegroundProbe>>>);

impl SharedProbe {
    fn lock(&self) -> MutexGuard<'_, Box<dyn ForegroundProbe>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl ForegroundProbe for SharedProbe {
    fn set_url_profiles_exist(&mut self, exist: bool) {
        self.lock().set_url_profiles_exist(exist);
    }
    fn set_panel_profiles_exist(&mut self, exist: bool) {
        self.lock().set_panel_profiles_exist(exist);
    }
    fn current(&mut self) -> PlatformResult<ForegroundInfo> {
        self.lock().current()
    }
    fn current_for_press(&mut self) -> PlatformResult<ForegroundInfo> {
        self.lock().current_for_press()
    }
}

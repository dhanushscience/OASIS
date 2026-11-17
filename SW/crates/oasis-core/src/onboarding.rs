//! Getting Started, the first-run guide: which step it is on and what moves it on.
//!
//! Pure: no clock, no files, no windows. The desktop app feeds it events and facts, saves the
//! result and draws it.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    New,
    Running,
    /// The first press is done and no voice model is ready yet. The guide is off screen.
    WaitingForVoice,
    /// A voice model became ready: one notice offers the voice part.
    VoiceOffered,
    Dismissed,
    Complete,
}

/// The steps, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StepId {
    Welcome,
    /// "A vertical or a horizontal pop-up?": sets the overlay, the voice pop-up and the keys to match.
    Layout,
    Browser,
    /// Which microphone to listen with, asked before the voice part.
    Microphone,
    Groq,
    Exit,
    Device,
    FirstPress,
    Return,
    VoiceKey,
    VoiceOpen,
    VoiceType,
    /// "Close Notepad": a close always asks first, so this is where approving is learned.
    VoiceClose,
    /// "Try voice commands": a practice box and checklist in the welcome window. Never blocks the end.
    TryCommands,
    Done,
}

/// Where a step is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// The small window that opens in place of the application.
    Welcome,
    /// The application's own window.
    Main,
    /// Over another program, which is in front.
    Overlay,
}

/// Why a step cannot go on right now. A closed list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocked {
    Offline,
    Bluetooth,
    DeviceLost,
    Integrity,
    Download,
    NoModel,
    NoMicrophone,
}

/// The website the guide put on button 2 for the first press, so it can take it off again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Made {
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    pub status: Status,
    pub step: StepId,
    pub has_device: Option<bool>,
    pub skipped: Vec<StepId>,
    /// Live conditions, never saved.
    pub blocked: Vec<Blocked>,
    pub made: Option<Made>,
    pub voice_guide_ran: bool,
    pub notice_shown: bool,
}

/// What the caller observed when the event arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Facts {
    pub browsers_installed: usize,
    /// The PC is under the minimum for a local voice model.
    pub below_floor: bool,
    /// A local voice model is in use.
    pub voice_ready: bool,
    /// A Groq key that passed its test is stored.
    pub groq_key_set: bool,
    /// The voice steps exist in this build.
    pub voice_steps: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Next,
    SkipStep,
    SkipAll,
    /// "Back" on a welcome question, to change an answer.
    Back,
    DeviceAnswer(bool),
    BrowserChosen,
    GroqDone,
    GroqDeclined,
    PopupClosed,
    DeviceConnected,
    Press,
    MainFocused,
    Timeout,
    ModelReady,
    Blocked(Blocked),
    Unblocked(Blocked),
    CommandRan,
    TranscriptTyped,
    Replay,
}

const STEPS: [(StepId, &str); 15] = [
    (StepId::Welcome, "welcome"),
    (StepId::Layout, "layout"),
    (StepId::Browser, "browser"),
    (StepId::Microphone, "microphone"),
    (StepId::Groq, "groq"),
    (StepId::Exit, "exit"),
    (StepId::Device, "device"),
    (StepId::FirstPress, "first_press"),
    (StepId::Return, "return"),
    (StepId::VoiceKey, "voice_key"),
    (StepId::VoiceOpen, "voice_open"),
    (StepId::VoiceType, "voice_type"),
    (StepId::VoiceClose, "voice_close"),
    (StepId::TryCommands, "try_commands"),
    (StepId::Done, "done"),
];

impl StepId {
    pub fn id(self) -> &'static str {
        STEPS.iter().find(|(s, _)| *s == self).map_or("done", |(_, id)| id)
    }

    pub fn parse(id: &str) -> Option<StepId> {
        STEPS.iter().find(|(_, name)| *name == id).map(|(s, _)| *s)
    }

    pub fn surface(self) -> Surface {
        match self {
            StepId::Welcome | StepId::Layout | StepId::Browser | StepId::Microphone | StepId::Groq | StepId::Exit | StepId::TryCommands => Surface::Welcome,
            StepId::Return | StepId::VoiceOpen | StepId::VoiceType | StepId::VoiceClose => Surface::Overlay,
            _ => Surface::Main,
        }
    }
}

const STATUSES: [(Status, &str); 6] = [
    (Status::New, "new"),
    (Status::Running, "running"),
    (Status::WaitingForVoice, "waiting_for_voice"),
    (Status::VoiceOffered, "voice_offered"),
    (Status::Dismissed, "dismissed"),
    (Status::Complete, "complete"),
];

impl Status {
    pub fn id(self) -> &'static str {
        STATUSES.iter().find(|(s, _)| *s == self).map_or("dismissed", |(_, id)| id)
    }

    pub fn parse(id: &str) -> Option<Status> {
        STATUSES.iter().find(|(_, name)| *name == id).map(|(s, _)| *s)
    }

    /// The guide is on screen, step by step.
    pub fn active(self) -> bool {
        matches!(self, Status::New | Status::Running)
    }
}

impl Blocked {
    pub fn id(self) -> &'static str {
        match self {
            Blocked::Offline => "offline",
            Blocked::Bluetooth => "bluetooth",
            Blocked::DeviceLost => "device_lost",
            Blocked::Integrity => "integrity",
            Blocked::Download => "download",
            Blocked::NoModel => "no_model",
            Blocked::NoMicrophone => "no_microphone",
        }
    }
}

/// A first run: nothing done yet.
pub fn fresh() -> Progress {
    Progress {
        status: Status::New,
        step: StepId::Welcome,
        has_device: None,
        skipped: Vec::new(),
        blocked: Vec::new(),
        made: None,
        voice_guide_ran: false,
        notice_shown: false,
    }
}

/// No guide: what an unreadable progress file is taken as, so the keys are never held by mistake.
pub fn dismissed() -> Progress {
    Progress { status: Status::Dismissed, ..fresh() }
}

/// The keyboard buttons are held back until the step that asks for a press.
pub fn holds_keys(p: &Progress) -> bool {
    p.status.active() && p.step < StepId::FirstPress
}

/// The reason, if any, that the step on screen cannot go on. Reasons for other steps wait their turn.
pub fn blocking(p: &Progress) -> Option<Blocked> {
    use StepId::*;
    p.blocked.iter().copied().find(|b| match (p.step, b) {
        (Groq, Blocked::Offline) | (Device, Blocked::Bluetooth) | (FirstPress, Blocked::Integrity) => true,
        (FirstPress, Blocked::DeviceLost) => p.has_device == Some(true),
        (VoiceKey | VoiceOpen | VoiceType | VoiceClose, Blocked::NoMicrophone | Blocked::Download | Blocked::NoModel) => true,
        _ => false,
    })
}

/// Whether a button still holds what the guide put on it.
pub fn same_action(made: &Made, kind: &str, value: &str) -> bool {
    made.kind == kind && made.value.trim() == value.trim()
}

fn f_browser_asked(f: &Facts) -> bool {
    f.browsers_installed >= 2
}

fn voice_possible(f: &Facts) -> bool {
    f.voice_steps && (f.voice_ready || f.groq_key_set)
}

/// Leave the current step for the next one that applies.
fn advance(mut p: Progress, f: &Facts) -> Progress {
    use StepId::*;
    loop {
        p.step = match p.step {
            Welcome => Layout,
            Layout => Browser,
            Browser => Microphone,
            Microphone => Groq,
            Groq => Exit,
            Exit => Device,
            Device => FirstPress,
            FirstPress => Return,
            Return => VoiceKey,
            VoiceKey => VoiceOpen,
            VoiceOpen => VoiceType,
            VoiceType => VoiceClose,
            VoiceClose => TryCommands,
            TryCommands | Done => Done,
        };
        let passed_over = match p.step {
            Browser => f.browsers_installed < 2,
            Groq => !f.below_floor,
            Device => p.has_device != Some(true),
            _ => false,
        };
        if !passed_over {
            break;
        }
    }
    match p.step {
        VoiceKey if !voice_possible(f) => p.status = Status::WaitingForVoice,
        Done => p.status = Status::Complete,
        _ => {}
    }
    p
}

/// The guide after `event`. An event the current step has no use for changes nothing.
pub fn step(progress: &Progress, event: Event, facts: &Facts) -> Progress {
    use StepId::*;
    let mut p = progress.clone();
    match event {
        Event::Blocked(reason) => {
            if !p.blocked.contains(&reason) {
                p.blocked.push(reason);
            }
            return p;
        }
        Event::Unblocked(reason) => {
            p.blocked.retain(|b| *b != reason);
            return p;
        }
        // Again from the start. What the person has (a device, their buttons) is kept.
        Event::Replay => {
            p.status = Status::Running;
            p.step = Welcome;
            p.skipped.clear();
            p.notice_shown = false;
            return p;
        }
        _ => {}
    }
    match p.status {
        Status::Dismissed | Status::Complete => return p,
        Status::WaitingForVoice => {
            match event {
                Event::ModelReady if voice_possible(facts) => p.status = Status::VoiceOffered,
                Event::SkipAll => p.status = Status::Dismissed,
                _ => {}
            }
            return p;
        }
        Status::VoiceOffered => {
            match event {
                Event::Next => (p.status, p.notice_shown) = (Status::Running, true),
                Event::SkipAll => (p.status, p.notice_shown) = (Status::Dismissed, true),
                _ => {}
            }
            return p;
        }
        Status::New | Status::Running => {}
    }
    match event {
        Event::SkipAll => {
            p.status = Status::Dismissed;
            return p;
        }
        Event::Back => {
            // Only the welcome questions go back, to the nearest earlier one that was asked.
            p.step = match p.step {
                Layout => Welcome,
                Browser => Layout,
                Microphone if f_browser_asked(facts) => Browser,
                Microphone => Layout,
                Groq => Microphone,
                Exit if facts.below_floor => Groq,
                Exit => Microphone,
                other => other,
            };
            return p;
        }
        Event::SkipStep => {
            return match p.step {
                Exit | Done => p,
                // No website was opened, so there is nothing to come back from.
                FirstPress => {
                    p.skipped.extend([FirstPress, Return]);
                    p.step = Return;
                    p.status = Status::Running;
                    advance(p, facts)
                }
                current => {
                    p.skipped.push(current);
                    p.status = Status::Running;
                    advance(p, facts)
                }
            };
        }
        _ => {}
    }
    let moves_on = match (p.step, event) {
        (Welcome, Event::DeviceAnswer(has)) => {
            p.has_device = Some(has);
            true
        }
        (Browser, Event::BrowserChosen | Event::Next) => true,
        (Layout | Microphone, Event::Next) => true,
        (Groq, Event::GroqDone | Event::GroqDeclined) => true,
        (Exit, Event::PopupClosed) => true,
        // Closed early: the person chose the application, so the rest of the welcome is passed over.
        (Welcome | Layout | Browser | Microphone | Groq, Event::PopupClosed) => {
            p.step = Exit;
            true
        }
        (Device, Event::DeviceConnected) => true,
        // "Use the keyboard instead."
        (Device, Event::DeviceAnswer(false)) => {
            p.has_device = Some(false);
            true
        }
        (FirstPress, Event::Press) => true,
        (Return, Event::MainFocused | Event::Timeout) => true,
        (VoiceKey, Event::Next) => true,
        (VoiceOpen, Event::CommandRan) => true,
        (VoiceType, Event::TranscriptTyped) => {
            p.voice_guide_ran = true;
            true
        }
        (VoiceClose, Event::CommandRan) => true,
        // Finish, or the window closed: the practice is never a gate.
        (TryCommands, Event::Next | Event::PopupClosed) => true,
        _ => false,
    };
    if !moves_on {
        return p;
    }
    p.status = Status::Running;
    advance(p, facts)
}

#[cfg(test)]
mod tests {
    use super::*;

    const READY: Facts = Facts { browsers_installed: 2, below_floor: false, voice_ready: true, groq_key_set: false, voice_steps: true };

    fn run(start: Progress, facts: &Facts, events: &[Event]) -> Progress {
        events.iter().fold(start, |p, e| step(&p, *e, facts))
    }

    #[test]
    fn the_whole_guide_with_a_keyboard() {
        use Event::*;
        let p = run(fresh(), &READY, &[DeviceAnswer(false), Next, BrowserChosen, Next, PopupClosed]);
        assert_eq!((p.step, p.status), (StepId::FirstPress, Status::Running), "no device: the device step is passed over");
        let p = run(p, &READY, &[Press, MainFocused, Next, CommandRan, TranscriptTyped]);
        assert_eq!((p.step, p.status), (StepId::VoiceClose, Status::Running), "the close is taught last");
        let p = step(&p, CommandRan, &READY);
        assert_eq!((p.step, p.status, p.step.surface()), (StepId::TryCommands, Status::Running, Surface::Welcome), "then the practice, in the welcome window");
        assert!(!holds_keys(&p), "the talk key works during the practice");
        let p = step(&p, Next, &READY);
        assert_eq!((p.step, p.status), (StepId::Done, Status::Complete));
        assert!(p.voice_guide_ran);
    }

    #[test]
    fn the_practice_ends_the_guide_by_finish_skip_or_closing_the_window_and_back_stays() {
        let at = Progress { step: StepId::TryCommands, status: Status::Running, ..fresh() };
        for event in [Event::Next, Event::SkipStep, Event::PopupClosed] {
            let p = step(&at, event, &READY);
            assert_eq!((p.step, p.status), (StepId::Done, Status::Complete), "{event:?}");
        }
        assert_eq!(step(&at, Event::Back, &READY), at, "the voice steps before it are not gone back to");
        assert_eq!(step(&at, Event::SkipAll, &READY).status, Status::Dismissed);
        assert_eq!(blocking(&Progress { blocked: vec![Blocked::Download, Blocked::NoMicrophone], ..at }), None, "it never waits");
    }

    #[test]
    fn a_guide_finished_before_the_practice_step_existed_stays_finished() {
        let done = Progress { status: Status::Complete, step: StepId::Done, voice_guide_ran: true, ..fresh() };
        for event in [Event::Next, Event::PopupClosed, Event::SkipStep] {
            assert_eq!(step(&done, event, &READY), done);
        }
    }

    #[test]
    fn back_returns_to_the_previous_welcome_question_and_stops_at_the_first() {
        use Event::*;
        let p = run(fresh(), &READY, &[DeviceAnswer(false), Next, BrowserChosen]);
        assert_eq!(p.step, StepId::Microphone);
        let p = step(&p, Back, &READY);
        assert_eq!(p.step, StepId::Browser);
        let p = step(&p, Back, &READY);
        assert_eq!(p.step, StepId::Layout);
        let p = step(&p, Back, &READY);
        assert_eq!(p.step, StepId::Welcome);
        assert_eq!(step(&p, Back, &READY).step, StepId::Welcome);
        let one = Facts { browsers_installed: 1, ..READY };
        let p = run(fresh(), &one, &[DeviceAnswer(false), Next]);
        assert_eq!(step(&p, Back, &one).step, StepId::Layout, "the passed-over browser step is skipped");
    }

    #[test]
    fn with_a_device_the_guide_waits_for_it_or_takes_the_keyboard_instead() {
        use Event::*;
        let at_device = run(fresh(), &READY, &[DeviceAnswer(true), Next, BrowserChosen, Next, PopupClosed]);
        assert_eq!(at_device.step, StepId::Device);
        assert_eq!(step(&at_device, DeviceConnected, &READY).step, StepId::FirstPress);
        let keyboard = step(&at_device, DeviceAnswer(false), &READY);
        assert_eq!((keyboard.step, keyboard.has_device), (StepId::FirstPress, Some(false)));
    }

    #[test]
    fn one_browser_passes_the_browser_step_and_groq_is_only_for_a_pc_below_the_floor() {
        let one = Facts { browsers_installed: 1, ..READY };
        assert_eq!(run(fresh(), &one, &[Event::DeviceAnswer(false), Event::Next]).step, StepId::Microphone);
        let low = Facts { below_floor: true, ..READY };
        let p = run(fresh(), &low, &[Event::DeviceAnswer(false), Event::Next, Event::BrowserChosen, Event::Next]);
        assert_eq!(p.step, StepId::Groq);
        assert_eq!(step(&p, Event::GroqDeclined, &low).step, StepId::Exit);
    }

    #[test]
    fn the_microphone_is_asked_about_before_anything_else_that_needs_it_and_a_missing_one_does_not_stop_the_welcome() {
        use Event::*;
        let ask = run(fresh(), &READY, &[DeviceAnswer(false), Next, BrowserChosen]);
        assert_eq!(ask.step, StepId::Microphone);
        let missing = step(&ask, Blocked(super::Blocked::NoMicrophone), &READY);
        assert_eq!(blocking(&missing), None, "it blocks the voice steps, not this one");
        assert_eq!(step(&missing, Next, &READY).step, StepId::Exit, "the welcome goes on");
        let voice = Progress { step: StepId::VoiceOpen, ..missing };
        assert_eq!(blocking(&voice), Some(super::Blocked::NoMicrophone));
    }

    #[test]
    fn the_layout_question_comes_right_after_the_device_answer_and_next_moves_on() {
        let p = step(&fresh(), Event::DeviceAnswer(false), &READY);
        assert_eq!((p.step, p.step.surface()), (StepId::Layout, Surface::Welcome));
        assert!(holds_keys(&p), "the keys stay held while they are being chosen");
        assert_eq!(step(&p, Event::Next, &READY).step, StepId::Browser);
        assert_eq!(step(&p, Event::BrowserChosen, &READY), p, "only Next answers it");
    }

    #[test]
    fn closing_the_welcome_early_goes_to_the_application() {
        let p = step(&fresh(), Event::PopupClosed, &READY);
        assert_eq!((p.step, p.status), (StepId::FirstPress, Status::Running));
    }

    #[test]
    fn skip_all_ends_it_from_every_step_and_nothing_moves_it_afterwards() {
        for (id, _) in STEPS {
            let p = step(&Progress { step: id, status: Status::Running, ..fresh() }, Event::SkipAll, &READY);
            assert_eq!(p.status, Status::Dismissed, "{id:?}");
            assert_eq!(step(&p, Event::Next, &READY), p);
            assert!(!holds_keys(&p));
        }
    }

    #[test]
    fn skipping_the_first_press_skips_the_return_too() {
        let p = step(&Progress { step: StepId::FirstPress, status: Status::Running, ..fresh() }, Event::SkipStep, &READY);
        assert_eq!(p.step, StepId::VoiceKey);
        assert_eq!(p.skipped, [StepId::FirstPress, StepId::Return]);
    }

    #[test]
    fn the_return_step_ends_by_focus_or_by_timeout_and_both_are_ignored_elsewhere() {
        let back = Progress { step: StepId::Return, status: Status::Running, ..fresh() };
        assert_eq!(step(&back, Event::MainFocused, &READY).step, StepId::VoiceKey);
        assert_eq!(step(&back, Event::Timeout, &READY).step, StepId::VoiceKey);
        let press = Progress { step: StepId::FirstPress, status: Status::Running, ..fresh() };
        assert_eq!(step(&press, Event::Timeout, &READY), press);
        assert_eq!(step(&press, Event::MainFocused, &READY), press);
    }

    #[test]
    fn with_no_voice_model_the_guide_waits_then_offers_the_voice_part_once() {
        let none = Facts { voice_ready: false, ..READY };
        let back = Progress { step: StepId::Return, status: Status::Running, ..fresh() };
        let waiting = step(&back, Event::MainFocused, &none);
        assert_eq!((waiting.step, waiting.status), (StepId::VoiceKey, Status::WaitingForVoice));
        assert_eq!(step(&waiting, Event::ModelReady, &none).status, Status::WaitingForVoice, "not ready yet");
        let offered = step(&waiting, Event::ModelReady, &READY);
        assert_eq!(offered.status, Status::VoiceOffered);
        let started = step(&offered, Event::Next, &READY);
        assert_eq!((started.status, started.step, started.notice_shown), (Status::Running, StepId::VoiceKey, true));
    }

    #[test]
    fn a_build_without_the_voice_steps_stops_after_the_return() {
        let stage_a = Facts { voice_steps: false, ..READY };
        let back = Progress { step: StepId::Return, status: Status::Running, ..fresh() };
        let waiting = step(&back, Event::MainFocused, &stage_a);
        assert_eq!(waiting.status, Status::WaitingForVoice);
        assert_eq!(step(&waiting, Event::ModelReady, &stage_a).status, Status::WaitingForVoice);
    }

    #[test]
    fn a_tested_groq_key_stands_in_for_a_local_model() {
        let groq = Facts { voice_ready: false, below_floor: true, groq_key_set: true, ..READY };
        let back = Progress { step: StepId::Return, status: Status::Running, ..fresh() };
        assert_eq!(step(&back, Event::Timeout, &groq).status, Status::Running);
    }

    #[test]
    fn a_reason_blocks_only_the_step_it_belongs_to_and_clears_alone() {
        use Event::{Blocked as B, Unblocked as U};
        let p = run(fresh(), &READY, &[B(Blocked::Download), B(Blocked::NoMicrophone), B(Blocked::Bluetooth)]);
        assert_eq!(blocking(&p), None, "none of these belongs to the welcome step");
        let voice = Progress { step: StepId::VoiceOpen, ..p };
        assert_eq!(blocking(&voice), Some(Blocked::Download));
        let voice = step(&voice, U(Blocked::Download), &READY);
        assert_eq!(blocking(&voice), Some(Blocked::NoMicrophone), "the other reason is still there");
        assert_eq!(blocking(&step(&voice, U(Blocked::NoMicrophone), &READY)), None);
    }

    #[test]
    fn replay_starts_again_and_keeps_the_device_answer() {
        let done = Progress { status: Status::Complete, step: StepId::Done, has_device: Some(true), voice_guide_ran: true, ..fresh() };
        let again = step(&done, Event::Replay, &READY);
        assert_eq!((again.status, again.step, again.has_device), (Status::Running, StepId::Welcome, Some(true)));
        assert!(again.voice_guide_ran);
    }

    #[test]
    fn the_keys_are_held_only_before_the_first_press() {
        assert!(holds_keys(&fresh()));
        assert!(holds_keys(&Progress { step: StepId::Device, status: Status::Running, ..fresh() }));
        assert!(!holds_keys(&Progress { step: StepId::FirstPress, status: Status::Running, ..fresh() }));
        assert!(!holds_keys(&Progress { status: Status::WaitingForVoice, step: StepId::VoiceKey, ..fresh() }));
        assert!(!holds_keys(&dismissed()));
    }

    #[test]
    fn step_names_round_trip_and_an_unknown_one_is_refused() {
        for (id, name) in STEPS {
            assert_eq!(StepId::parse(name), Some(id));
            assert_eq!(id.id(), name);
        }
        assert_eq!(StepId::parse("a_step_from_a_newer_build"), None);
        for (status, name) in STATUSES {
            assert_eq!(Status::parse(name), Some(status));
        }
    }

    #[test]
    fn the_button_the_guide_made_is_recognised_until_it_is_changed() {
        let made = Made { kind: "OpenUrl".into(), value: "https://example.com/".into() };
        assert!(same_action(&made, "OpenUrl", " https://example.com/ "));
        assert!(!same_action(&made, "OpenUrl", "https://other.example/"));
        assert!(!same_action(&made, "LaunchApp", "https://example.com/"));
    }
}

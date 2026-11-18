//! A recognised gesture: the label peek, and what the button means for the app in front.

use super::*;

/// Whether a button has anything to run (a blank button shows a press and does nothing).
pub(super) fn is_runnable(action: &ButtonAction) -> bool {
    matches!(action.kind(), ActionKind::Automation | ActionKind::JoinMeeting) || !action.value.trim().is_empty()
}

/// The active profile's name and the three button labels to show, if labels are wanted for it
/// (the profile's own label mode can force them on or off over the general setting).
/// With `tapped`, the labels are only wanted if that button has something to run, as in C#.
pub(super) fn peek_labels(
    settings: &Settings,
    info: &ForegroundInfo,
    tapped: Option<i32>,
    watch_token: &str,
) -> Option<(String, Vec<String>)> {
    if !labels_possible(settings) {
        return None;
    }
    let context = ForegroundContext::from(info);
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

/// Whether any profile can show labels. When none can, a label peek needs no read of what is in front.
pub(super) fn labels_possible(settings: &Settings) -> bool {
    settings.show_overlay_labels
        || settings.profiles.iter().any(|p| {
            [p.label_mode.as_str()].into_iter().chain(p.config_profiles.iter().map(|c| c.label_mode.as_str()))
                .any(|m| LabelPolicy::effective(false, Some(m)))
        })
}

/// A recognised gesture and what YouTube was doing when it happened.
pub(super) struct Gesture {
    pub(super) button: i32,
    pub(super) claim: GestureClaimId,
    pub(super) watch_token: String,
}

/// A gesture was recognised: work out what it means for the app in front (`info`, read at the
/// press; `None` when it was not needed or could not be read) and start it.
pub(super) fn dispatch(
    shared: &Shared,
    settings: &Settings,
    verified: bool,
    info: Option<&ForegroundInfo>,
    jobs: &Jobs,
    gesture: Gesture,
) {
    let Gesture { button, claim, watch_token } = gesture;
    let number = (button + 1).clamp(0, 255) as u8;
    let Some((press, action)) = plan(settings, info, button, claim, &watch_token) else {
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
pub(super) fn plan(
    settings: &Settings,
    info: Option<&ForegroundInfo>,
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
                kind: ActionKind::Shortcut.as_str().into(),
                value: "Win+D".into(),
                runs: false,
            },
            ButtonAction { kind: ActionKind::Shortcut.as_str().into(), value: "Win+D".into(), steps: vec![] },
        )),
        GestureClaimId::BoundTap => {
            let context = ForegroundContext::from(info?);
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
                    kind: ActionKind::Shortcut.as_str().into(),
                    value: key.into(),
                    runs: false,
                },
                ButtonAction { kind: ActionKind::Shortcut.as_str().into(), value: key.into(), steps: vec![] },
            ))
        }
        _ => None,
    }
}

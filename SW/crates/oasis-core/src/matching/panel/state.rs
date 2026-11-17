//! Keeping the panel steady. Port of `Services/Context/ContextPanelState.cs`: a weak reading that
//! disagrees with a strong one a moment ago is not believed, so a flicker of the accessibility
//! tree does not flip the buttons under the user's finger.

use std::time::{Duration, Instant};

use super::classify::PanelResult;

/// A strong reading is held for this long against a weaker, different one.
const HOLD: Duration = Duration::from_millis(500);

fn rank(confidence: &str) -> u8 {
    match confidence.to_ascii_lowercase().as_str() {
        "high" => 3,
        "medium" => 2,
        "low" => 1,
        _ => 0,
    }
}

#[derive(Debug)]
pub struct PanelState {
    last_panel: String,
    last_confidence: String,
    last_at: Option<Instant>,
}

impl Default for PanelState {
    fn default() -> PanelState {
        PanelState { last_panel: "unknown".into(), last_confidence: "none".into(), last_at: None }
    }
}

impl PanelState {
    /// The reading to use. `force_fresh` takes the new one as it is.
    pub fn stabilize(&mut self, incoming: PanelResult, force_fresh: bool, now: Instant) -> PanelResult {
        if force_fresh || incoming.panel_id == self.last_panel || rank(&incoming.confidence) >= 2 {
            self.apply(&incoming, now);
            return incoming;
        }
        let held = rank(&self.last_confidence) >= 2
            && self.last_at.is_some_and(|at| now.duration_since(at) < HOLD)
            && self.last_panel != "unknown";
        if held {
            return PanelResult {
                panel_id: self.last_panel.clone(),
                confidence: self.last_confidence.clone(),
                ..incoming
            };
        }
        self.apply(&incoming, now);
        incoming
    }

    fn apply(&mut self, result: &PanelResult, now: Instant) {
        self.last_panel = result.panel_id.clone();
        self.last_confidence = result.confidence.clone();
        self.last_at = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(panel: &str, confidence: &str) -> PanelResult {
        PanelResult { panel_id: panel.into(), confidence: confidence.into(), ..PanelResult::default() }
    }

    #[test]
    fn a_confident_reading_is_taken_at_once() {
        let mut state = PanelState::default();
        let now = Instant::now();
        assert_eq!(state.stabilize(reading("terminal", "high"), false, now).panel_id, "terminal");
        assert_eq!(state.stabilize(reading("editor", "medium"), false, now).panel_id, "editor");
    }

    #[test]
    fn a_weak_disagreement_right_after_a_strong_reading_is_not_believed() {
        let mut state = PanelState::default();
        let t0 = Instant::now();
        state.stabilize(reading("terminal", "high"), false, t0);
        let flicker = state.stabilize(reading("unknown", "low"), false, t0 + Duration::from_millis(200));
        assert_eq!((flicker.panel_id.as_str(), flicker.confidence.as_str()), ("terminal", "high"));
    }

    #[test]
    fn after_the_hold_a_weak_reading_is_accepted() {
        let mut state = PanelState::default();
        let t0 = Instant::now();
        state.stabilize(reading("terminal", "high"), false, t0);
        let later = state.stabilize(reading("unknown", "low"), false, t0 + Duration::from_millis(600));
        assert_eq!(later.panel_id, "unknown");
    }

    #[test]
    fn a_forced_fresh_reading_is_always_taken() {
        let mut state = PanelState::default();
        let t0 = Instant::now();
        state.stabilize(reading("terminal", "high"), false, t0);
        assert_eq!(state.stabilize(reading("unknown", "none"), true, t0).panel_id, "unknown");
    }

    #[test]
    fn nothing_is_held_when_nothing_was_known() {
        let mut state = PanelState::default();
        assert_eq!(state.stabilize(reading("unknown", "low"), false, Instant::now()).panel_id, "unknown");
    }
}

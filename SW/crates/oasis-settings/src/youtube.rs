//! The YouTube and browser-bridge settings, and the rule for when the hidden YouTube gestures
//! (hold for 2×, double-click for volume) are live. Port of `EvaluateYouTubeWatchPlayingGestures`
//! and `LooksLikeYouTubeWatchPage` from `Services/OverlayService.cs`, and the settings message of
//! `BrowserBridgeService.BroadcastYouTubeSettingsAsync`.

use oasis_core::bridge::{ConfigWhen, YouTubeWatchState};
use oasis_core::matching::ForegroundContext;
use serde_json::{json, Value};

use crate::matcher::is_youtube_foreground;
use crate::model::Settings;

pub const DEFAULT_BRIDGE_PORT: u16 = 17322;

fn youtube(settings: &Settings) -> Option<&Value> {
    settings.extra.get("YouTube").filter(|v| v.is_object())
}

fn flag(section: Option<&Value>, key: &str, default: bool) -> bool {
    section.and_then(|s| s.get(key)).and_then(Value::as_bool).unwrap_or(default)
}

fn index(section: Option<&Value>, key: &str, default: i64) -> i64 {
    section.and_then(|s| s.get(key)).and_then(Value::as_i64).unwrap_or(default)
}

/// Whether the extension's local socket should be listening (default on, as in C#).
pub fn bridge_enabled(settings: &Settings) -> bool {
    settings.extra.get("BrowserBridgeEnabled").and_then(Value::as_bool).unwrap_or(true)
}

/// The socket's port. A missing, zero or out-of-range value means the default.
pub fn bridge_port(settings: &Settings) -> u16 {
    settings
        .extra
        .get("BrowserBridgePort")
        .and_then(Value::as_i64)
        .and_then(|p| u16::try_from(p).ok())
        .filter(|p| *p > 0)
        .unwrap_or(DEFAULT_BRIDGE_PORT)
}

pub fn gestures_enabled(settings: &Settings) -> bool {
    flag(youtube(settings), "GesturesEnabled", true)
}

/// What the extension is told (`oasis.settings` → `youtube`).
pub fn extension_settings(settings: &Settings) -> Value {
    let yt = youtube(settings);
    json!({
        "autoSkipAds": flag(yt, "AutoSkipAds", true),
        "gesturesEnabled": gestures_enabled(settings),
        "hold2xButtonIndex": index(yt, "Hold2xButtonIndex", 1),
        "volumeUpButtonIndex": index(yt, "VolumeUpButtonIndex", 0),
        "volumeDownButtonIndex": index(yt, "VolumeDownButtonIndex", 2),
    })
}

/// Hidden YouTube gestures are live only for a normal watch page that is playing and in front:
/// never Shorts, never the end screen, never a background tab that merely reports Playing.
pub fn gestures_active(
    settings: &Settings,
    state: &YouTubeWatchState,
    context: &ForegroundContext,
) -> bool {
    if !gestures_enabled(settings) || state.is_short {
        return false;
    }
    let token = state.current_when_token();
    if token == ConfigWhen::ENDED || token == ConfigWhen::SHORTS {
        return false;
    }
    if !is_youtube_foreground(context) {
        return false;
    }
    if token == ConfigWhen::PLAYING {
        return true;
    }
    // The bridge can lag behind a restart: the watch page is in front but "playing" is late.
    looks_like_watch_page(context, state)
}

/// A normal watch page in front (not home, search or Shorts). The bridge state is only a last
/// hint, after the caller has proven YouTube is in front.
fn looks_like_watch_page(context: &ForegroundContext, state: &YouTubeWatchState) -> bool {
    let url = context.url.to_ascii_lowercase();
    if !url.trim().is_empty() {
        if url.contains("/shorts/") {
            return false;
        }
        if url.contains("/watch") || url.contains("youtu.be/") {
            return true;
        }
    }
    let title = context.window_title.to_ascii_lowercase();
    if [" - google search", " - bing", " - duckduckgo"].iter().any(|s| title.contains(s)) {
        return false;
    }
    if title.contains(" - youtube") {
        return true;
    }
    // The address and title both came back empty while YouTube is still in front.
    !state.video_id.trim().is_empty() && !state.is_short
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis_core::bridge::YouTubeWatchEvent;

    fn settings(json: Value) -> Settings {
        serde_json::from_value(json).unwrap()
    }

    fn playing() -> YouTubeWatchState {
        let mut state = YouTubeWatchState::default();
        let mut event = YouTubeWatchEvent::new("youtube.started");
        event.video_id = Some("abc".into());
        event.surface = Some("watch".into());
        state.apply(&event);
        state
    }

    fn page(url: &str, title: &str) -> ForegroundContext {
        ForegroundContext {
            app_name: "chrome".into(),
            url: url.into(),
            window_title: title.into(),
            ..Default::default()
        }
    }

    #[test]
    fn the_defaults_match_csharp() {
        let s = Settings::default();
        assert!(bridge_enabled(&s) && gestures_enabled(&s));
        assert_eq!(bridge_port(&s), 17322);
        let sent = extension_settings(&s);
        assert_eq!(sent["autoSkipAds"], true);
        assert_eq!((sent["hold2xButtonIndex"].as_i64(), sent["volumeUpButtonIndex"].as_i64(), sent["volumeDownButtonIndex"].as_i64()), (Some(1), Some(0), Some(2)));
    }

    #[test]
    fn the_owners_real_settings_are_read() {
        let s = settings(json!({
            "BrowserBridgeEnabled": true, "BrowserBridgePort": 17322,
            "YouTube": { "AutoSkipAds": true, "GesturesEnabled": true, "Hold2xButtonIndex": 2,
                         "VolumeUpButtonIndex": 1, "VolumeDownButtonIndex": 0 }
        }));
        let sent = extension_settings(&s);
        assert_eq!(sent["hold2xButtonIndex"], 2);
        assert_eq!(sent["volumeUpButtonIndex"], 1);
        assert_eq!(sent["volumeDownButtonIndex"], 0);
    }

    #[test]
    fn a_bad_port_means_the_default() {
        for bad in [json!(0), json!(-5), json!(70000), json!("x"), json!(null)] {
            assert_eq!(bridge_port(&settings(json!({ "BrowserBridgePort": bad }))), 17322, "{bad}");
        }
        assert_eq!(bridge_port(&settings(json!({ "BrowserBridgePort": 4000 }))), 4000);
        assert!(!bridge_enabled(&settings(json!({ "BrowserBridgeEnabled": false }))));
    }

    #[test]
    fn gestures_are_live_for_a_playing_watch_page_in_front() {
        let s = Settings::default();
        assert!(gestures_active(&s, &playing(), &page("https://www.youtube.com/watch?v=abc", "T - YouTube")));
    }

    #[test]
    fn gestures_are_never_live_outside_a_playing_watch_page() {
        let s = Settings::default();
        let watch = page("https://www.youtube.com/watch?v=abc", "T - YouTube");
        assert!(!gestures_active(&s, &playing(), &page("https://example.com/", "Example")), "another site in front");
        assert!(!gestures_active(&s, &playing(), &page("https://www.google.com/search?q=youtube", "youtube - Google Search")), "a search result");
        assert!(!gestures_active(&settings(json!({ "YouTube": { "GesturesEnabled": false } })), &playing(), &watch), "switched off");

        let mut short = playing();
        let mut event = YouTubeWatchEvent::new("youtube.started");
        event.surface = Some("shorts".into());
        short.apply(&event);
        assert!(!gestures_active(&s, &short, &page("https://www.youtube.com/shorts/x", "S - YouTube")), "Shorts");

        let mut ended = playing();
        let mut end = YouTubeWatchEvent::new("youtube.progress");
        end.percent = Some(99);
        end.video_id = Some("abc".into());
        ended.apply(&end);
        assert!(!gestures_active(&s, &ended, &watch), "the end screen");
    }

    #[test]
    fn a_late_playing_event_does_not_switch_the_gestures_off_on_a_watch_page() {
        let idle = YouTubeWatchState::default();
        let s = Settings::default();
        assert!(gestures_active(&s, &idle, &page("https://www.youtube.com/watch?v=abc", "T - YouTube")));
        assert!(!gestures_active(&s, &idle, &page("https://www.youtube.com/", "YouTube")), "the home page is not a watch page");
    }
}

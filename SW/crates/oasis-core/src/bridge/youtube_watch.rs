//! Port of `Services/BrowserBridge/YouTubeWatchState.cs` and `Services/ConfigWhen.cs`.
//!
//! Reduces the extension's YouTube events to a durable phase — Idle, Playing, Ended — that
//! picks which button set a profile shows (`ConfigProfile.When`). The C# class was a locked
//! singleton with a `Changed` event; here it is a plain struct and [`YouTubeWatchState::apply`]
//! returns whether anything changed. The host owns it and raises its own notification.
//!
//! Rules (docs/PROTOCOLS.md, "Playback phase state machine"):
//! - Shorts always map to `youtube.shorts` and never to `youtube.ended`; Shorts loop constantly.
//! - `youtube.ready` on a watch page prefers Playing, so a new video after an end screen does
//!   not keep showing ENDED options.
//! - ≥ 98 % → Ended. Below 95 %, or any start → Playing, so seek-back or replay cannot stick.
//! - The 95–97 % band holds Ended briefly to stop it flapping.
//! - A new `videoId` leaves Ended immediately.

use crate::matching::text::contains_ignore_case;

/// Canonical `ConfigProfile.When` values.
pub struct ConfigWhen;

impl ConfigWhen {
    pub const PLAYING: &'static str = "youtube.playing";
    pub const ENDED: &'static str = "youtube.ended";
    pub const SHORTS: &'static str = "youtube.shorts";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum YouTubePlaybackPhase {
    #[default]
    Idle,
    Playing,
    Ended,
}

/// One event from the browser extension (the JSON frame, already decoded).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct YouTubeWatchEvent {
    pub event_type: String,
    pub video_id: Option<String>,
    pub title: Option<String>,
    pub url: Option<String>,
    pub surface: Option<String>,
    pub percent: Option<i32>,
}

impl YouTubeWatchEvent {
    pub fn new(event_type: &str) -> YouTubeWatchEvent {
        YouTubeWatchEvent {
            event_type: event_type.to_string(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct YouTubeWatchState {
    pub phase: YouTubePlaybackPhase,
    pub video_id: String,
    pub title: String,
    pub url: String,
    pub is_short: bool,
}

const NEAR_END_PERCENT: i32 = 98;
const RECOVER_PLAYING_PERCENT: i32 = 95;

fn trimmed(v: &Option<String>) -> &str {
    v.as_deref().unwrap_or("").trim()
}

/// Replace `field` with `value` if `value` is non-empty and different. Returns whether it did.
fn update(field: &mut String, value: &str) -> bool {
    if !value.is_empty() && field != value {
        *field = value.to_string();
        true
    } else {
        false
    }
}

impl YouTubeWatchState {
    /// Apply one event. Returns `true` when anything observable changed.
    pub fn apply(&mut self, evt: &YouTubeWatchEvent) -> bool {
        // Event names are read the same way everywhere: spaces trimmed, case ignored.
        let ty = evt.event_type.trim().to_ascii_lowercase();
        let ty = ty.as_str();
        if ty.is_empty() {
            return false;
        }
        let video_id = trimmed(&evt.video_id);
        let title = trimmed(&evt.title);
        let url = trimmed(&evt.url);
        let is_short = is_short_surface(evt);
        let mut raise = false;

        if ty == "youtube.ready" {
            raise |= self.set_short(is_short);
            raise |= update(&mut self.url, url);
            raise |= update(&mut self.video_id, video_id);
            raise |= update(&mut self.title, title);
            // Ready on a watch page: prefer Playing so a new video after an end screen (or a
            // missed play event) does not keep showing ENDED button options.
            if !is_short
                && matches!(
                    self.phase,
                    YouTubePlaybackPhase::Idle | YouTubePlaybackPhase::Ended
                )
            {
                self.phase = YouTubePlaybackPhase::Playing;
                raise = true;
            }
            return raise;
        }

        // Shorts loop and end constantly — never enter Ended for When matching.
        if is_short && ty.eq_ignore_ascii_case("youtube.ended") {
            raise |= self.set_short(is_short);
            raise |= update(&mut self.video_id, video_id);
            raise |= update(&mut self.title, title);
            if self.phase == YouTubePlaybackPhase::Ended {
                self.phase = YouTubePlaybackPhase::Playing;
                raise = true;
            }
            return raise;
        }

        let percent = evt.percent;
        let next = match ty {
            "youtube.started" => Some(YouTubePlaybackPhase::Playing),
            "youtube.progress" if !is_short && percent.is_some_and(|p| p >= NEAR_END_PERCENT) => {
                Some(YouTubePlaybackPhase::Ended)
            }
            "youtube.progress" => Some(YouTubePlaybackPhase::Playing),
            "youtube.ended" if !is_short => Some(YouTubePlaybackPhase::Ended),
            _ => None,
        };
        let Some(mut phase) = next else {
            return false;
        };

        let video_changed = !video_id.is_empty() && self.video_id != video_id;
        raise |= self.set_short(is_short);
        raise |= update(&mut self.url, url);
        if video_changed {
            self.video_id = video_id.to_string();
            raise = true;
            // A new video leaves ENDED immediately — do not wait for a play event.
            if !is_short && phase != YouTubePlaybackPhase::Ended {
                phase = YouTubePlaybackPhase::Playing;
            }
        }
        raise |= update(&mut self.title, title);

        // Keep ENDED only while still near the end of the same video (the ambiguous 95–97 %
        // band). Any start, seek-back or mid-play progress exits ENDED immediately.
        let hold_ended = self.phase == YouTubePlaybackPhase::Ended
            && phase == YouTubePlaybackPhase::Playing
            && ty.eq_ignore_ascii_case("youtube.progress")
            && !video_changed
            && percent.is_some_and(|p| p >= RECOVER_PLAYING_PERCENT);

        if !hold_ended && self.phase != phase {
            self.phase = phase;
            raise = true;
        }
        raise
    }

    /// The token matched by `ConfigProfile.When`. Shorts always map to
    /// [`ConfigWhen::SHORTS`], never to [`ConfigWhen::ENDED`].
    pub fn current_when_token(&self) -> &'static str {
        if self.is_short {
            return ConfigWhen::SHORTS;
        }
        match self.phase {
            YouTubePlaybackPhase::Playing => ConfigWhen::PLAYING,
            YouTubePlaybackPhase::Ended => ConfigWhen::ENDED,
            YouTubePlaybackPhase::Idle => "",
        }
    }

    fn set_short(&mut self, is_short: bool) -> bool {
        if self.is_short != is_short {
            self.is_short = is_short;
            true
        } else {
            false
        }
    }
}

/// The words for the notice shown when a video ends: its title, cut to 48 characters. `None` for
/// anything but the end of a video that is not a Short (Shorts loop constantly).
pub fn ended_notice(evt: &YouTubeWatchEvent) -> Option<String> {
    if !evt.event_type.trim().eq_ignore_ascii_case("youtube.ended") || is_short_surface(evt) {
        return None;
    }
    let title = trimmed(&evt.title);
    if title.is_empty() {
        return Some("YouTube video".to_string());
    }
    Some(if title.chars().count() > 48 { format!("{}…", title.chars().take(47).collect::<String>()) } else { title.to_string() })
}

/// A video is nearly over (98%) and the button labels should be shown for a moment, once per video.
/// `last_peeked` remembers the video; a new start or ready event clears it. Not for Shorts.
pub fn near_end_peek(evt: &YouTubeWatchEvent, last_peeked: &mut String) -> bool {
    let ty = evt.event_type.trim();
    let id = trimmed(&evt.video_id);
    if ty.eq_ignore_ascii_case("youtube.started") || ty.eq_ignore_ascii_case("youtube.ready") {
        if !id.is_empty() {
            last_peeked.clear();
        }
        return false;
    }
    if !ty.eq_ignore_ascii_case("youtube.progress") || is_short_surface(evt) || evt.percent.unwrap_or(0) < NEAR_END_PERCENT {
        return false;
    }
    if !id.is_empty() && id == last_peeked.as_str() {
        return false;
    }
    if !id.is_empty() {
        *last_peeked = id.to_string();
    }
    true
}

fn is_short_surface(evt: &YouTubeWatchEvent) -> bool {
    let surface = trimmed(&evt.surface);
    surface.eq_ignore_ascii_case("short")
        || surface.eq_ignore_ascii_case("shorts")
        || contains_ignore_case(evt.url.as_deref().unwrap_or(""), "/shorts/")
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_ended_notice_names_the_video_and_skips_shorts() {
        let mut ended = YouTubeWatchEvent::new("youtube.ended");
        assert_eq!(ended_notice(&ended).as_deref(), Some("YouTube video"));
        ended.title = Some("A talk".into());
        assert_eq!(ended_notice(&ended).as_deref(), Some("A talk"));
        ended.title = Some("x".repeat(60));
        assert_eq!(ended_notice(&ended).unwrap().chars().count(), 48);
        ended.url = Some("https://www.youtube.com/shorts/abc".into());
        assert_eq!(ended_notice(&ended), None);
        assert_eq!(ended_notice(&YouTubeWatchEvent::new("youtube.progress")), None);
    }

    #[test]
    fn event_names_are_read_alike_everywhere_whatever_their_case_or_spaces() {
        assert!(ended_notice(&YouTubeWatchEvent::new(" YouTube.Ended ")).is_some());
        let mut s = YouTubeWatchState::default();
        assert!(s.apply(&YouTubeWatchEvent::new(" YouTube.Started ")));
        assert_eq!(s.current_when_token(), ConfigWhen::PLAYING);
    }

    #[test]
    fn the_near_end_peek_comes_once_per_video_and_never_for_shorts() {
        let progress = |id: &str, pct: i32, url: &str| YouTubeWatchEvent {
            event_type: "youtube.progress".into(),
            video_id: Some(id.into()),
            percent: Some(pct),
            url: Some(url.into()),
            ..Default::default()
        };
        let mut last = String::new();
        assert!(!near_end_peek(&progress("a", 97, ""), &mut last));
        assert!(near_end_peek(&progress("a", 98, ""), &mut last));
        assert!(!near_end_peek(&progress("a", 99, ""), &mut last), "once");
        assert!(near_end_peek(&progress("b", 99, ""), &mut last), "another video");
        assert!(!near_end_peek(&progress("c", 99, "https://youtube.com/shorts/c"), &mut last));
        let mut started = YouTubeWatchEvent::new("youtube.started");
        started.video_id = Some("b".into());
        assert!(!near_end_peek(&started, &mut last));
        assert!(near_end_peek(&progress("b", 99, ""), &mut last), "watched again");
    }

    use super::*;

    fn ev(ty: &str) -> YouTubeWatchEvent {
        YouTubeWatchEvent::new(ty)
    }

    fn progress(p: i32) -> YouTubeWatchEvent {
        YouTubeWatchEvent {
            percent: Some(p),
            video_id: Some("v1".into()),
            ..ev("youtube.progress")
        }
    }

    #[test]
    fn ready_on_a_watch_page_prefers_playing() {
        let mut s = YouTubeWatchState::default();
        assert!(s.apply(&ev("youtube.ready")));
        assert_eq!(s.current_when_token(), ConfigWhen::PLAYING);
    }

    #[test]
    fn near_end_is_ended_and_mid_play_recovers() {
        let mut s = YouTubeWatchState::default();
        s.apply(&YouTubeWatchEvent {
            video_id: Some("v1".into()),
            ..ev("youtube.started")
        });
        s.apply(&progress(98));
        assert_eq!(s.phase, YouTubePlaybackPhase::Ended);
        // 95–97 band holds ENDED.
        s.apply(&progress(96));
        assert_eq!(s.phase, YouTubePlaybackPhase::Ended);
        // Seek back below 95 % recovers immediately.
        s.apply(&progress(40));
        assert_eq!(s.phase, YouTubePlaybackPhase::Playing);
    }

    #[test]
    fn a_new_video_leaves_ended_immediately() {
        let mut s = YouTubeWatchState::default();
        s.apply(&progress(99));
        assert_eq!(s.phase, YouTubePlaybackPhase::Ended);
        s.apply(&YouTubeWatchEvent {
            percent: Some(96),
            video_id: Some("v2".into()),
            ..ev("youtube.progress")
        });
        assert_eq!(s.phase, YouTubePlaybackPhase::Playing);
    }

    #[test]
    fn shorts_never_end() {
        let mut s = YouTubeWatchState::default();
        let short = |ty: &str| YouTubeWatchEvent {
            surface: Some("short".into()),
            ..ev(ty)
        };
        s.apply(&short("youtube.ready"));
        s.apply(&short("youtube.ended"));
        assert_ne!(s.phase, YouTubePlaybackPhase::Ended);
        assert_eq!(s.current_when_token(), ConfigWhen::SHORTS);

        let mut by_url = YouTubeWatchState::default();
        by_url.apply(&YouTubeWatchEvent {
            url: Some("https://www.youtube.com/shorts/abc".into()),
            ..ev("youtube.started")
        });
        assert_eq!(by_url.current_when_token(), ConfigWhen::SHORTS);
    }

    #[test]
    fn unknown_or_empty_events_change_nothing() {
        let mut s = YouTubeWatchState::default();
        assert!(!s.apply(&ev("")));
        assert!(!s.apply(&ev("youtube.ad_skipped")));
        assert_eq!(s, YouTubeWatchState::default());
        assert_eq!(s.current_when_token(), "");
    }
}

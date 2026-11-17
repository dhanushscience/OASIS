//! Port of `Voice/MediaDuckPolicy.cs`.
//!
//! Whether opening the mic may pause YouTube. The rule that matters: only something that was
//! playing is paused, because only what was paused here is resumed afterwards. The old ducker
//! paused whenever a video was merely loaded and then always resumed it, so a video the user
//! had paused on purpose started playing after every voice command.

/// What the OS media-session API reports about media outside Spotify.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaPlayback {
    /// The query failed or timed out; nothing is known.
    Unknown,
    /// At least one session is playing.
    Playing,
    /// Sessions were read and none is playing.
    NotPlaying,
}

pub struct MediaDuckPolicy;

impl MediaDuckPolicy {
    /// `watch_page_open`: the extension reported a watch page that has not ended (it does not
    /// report pauses). `youtube_foreground`: YouTube is the focused window.
    pub fn should_pause_youtube(
        media: MediaPlayback,
        watch_page_open: bool,
        youtube_foreground: bool,
    ) -> bool {
        media != MediaPlayback::NotPlaying && (watch_page_open || youtube_foreground)
    }

    /// The keyboard fallback is a play/pause toggle sent to the focused window. Only safe when
    /// that window is YouTube (anywhere else it types "k") and something is known to be playing
    /// (on a paused video the toggle starts it).
    pub fn may_toggle_with_keyboard(media: MediaPlayback, youtube_foreground: bool) -> bool {
        youtube_foreground && media == MediaPlayback::Playing
    }
}

#[cfg(test)]
mod tests {
    use super::{MediaDuckPolicy as P, MediaPlayback as M};

    #[test]
    fn a_paused_video_is_left_alone() {
        assert!(!P::should_pause_youtube(M::NotPlaying, true, false));
        assert!(!P::should_pause_youtube(M::NotPlaying, true, true));
    }

    #[test]
    fn a_playing_video_is_paused() {
        for (w, f) in [(true, false), (false, true), (true, true)] {
            assert!(P::should_pause_youtube(M::Playing, w, f));
        }
    }

    #[test]
    fn without_a_media_answer_the_extension_decides() {
        assert!(P::should_pause_youtube(M::Unknown, true, false));
    }

    #[test]
    fn nothing_is_sent_when_no_youtube_page_is_known() {
        assert!(!P::should_pause_youtube(M::Playing, false, false));
    }

    #[test]
    fn the_keyboard_toggle_needs_a_focused_playing_youtube() {
        assert!(P::may_toggle_with_keyboard(M::Playing, true));
        assert!(!P::may_toggle_with_keyboard(M::Playing, false));
        assert!(!P::may_toggle_with_keyboard(M::NotPlaying, true));
        assert!(!P::may_toggle_with_keyboard(M::Unknown, true));
    }
}

//! What is playing on this PC (Spotify, Apple Music, a browser tab on YouTube, …), through the
//! Windows media sessions that the keyboard's Pause key and the volume flyout also use.
//! Only what is really playing is paused, and only that is resumed, so a song you had paused
//! yourself never starts by itself.

use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};

/// Pauses every session that is playing. Returns their ids for [`resume`]; empty when nothing was
/// playing or Windows could not be asked.
pub fn pause_playing() -> Vec<String> {
    on_own_thread(|| -> windows::core::Result<Vec<String>> {
        let mut paused = Vec::new();
        for session in Manager::RequestAsync()?.join()?.GetSessions()? {
            if session.GetPlaybackInfo()?.PlaybackStatus()? == Status::Playing && session.TryPauseAsync()?.join()? {
                paused.push(session.SourceAppUserModelId()?.to_string());
            }
        }
        Ok(paused)
    })
    .unwrap_or_default()
}

/// Plays the sessions [`pause_playing`] paused, unless they were started again or closed since.
pub fn resume(ids: Vec<String>) {
    on_own_thread(move || -> windows::core::Result<Vec<String>> {
        for session in Manager::RequestAsync()?.join()?.GetSessions()? {
            let id = session.SourceAppUserModelId()?.to_string();
            if ids.contains(&id) && session.GetPlaybackInfo()?.PlaybackStatus()? == Status::Paused {
                session.TryPlayAsync()?.join()?;
            }
        }
        Ok(Vec::new())
    });
}

/// Windows' async calls are waited on from a thread of their own, so they never block (or fail
/// on) a thread that already owns a window or a COM apartment.
fn on_own_thread<T: Send + 'static>(f: impl FnOnce() -> windows::core::Result<T> + Send + 'static) -> Option<T> {
    std::thread::spawn(f).join().ok()?.ok()
}

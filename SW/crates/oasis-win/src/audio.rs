//! Playing a finished WAV recording. Replaces the NAudio playback of `OfflineTtsService.cs`.
//!
//! The sound is played from memory through the system's own player (`PlaySound`), so no audio
//! library is needed and no temporary file has to be kept open while it plays.

use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_MEMORY, SND_NODEFAULT};

/// Plays `wav` (a whole WAV file) and returns when it ends or [`stop`] is called. `false` if
/// Windows could not play it.
#[allow(unsafe_code)]
pub fn play_wav_blocking(wav: &[u8]) -> bool {
    if wav.len() < 44 || &wav[..4] != b"RIFF" {
        return false;
    }
    // SAFETY: without SND_ASYNC the call returns only when the sound is done, so `wav` outlives it.
    unsafe { PlaySoundW(wav.as_ptr().cast(), std::ptr::null_mut(), SND_MEMORY | SND_NODEFAULT) != 0 }
}

/// Cuts off any sound this program is playing.
#[allow(unsafe_code)]
pub fn stop() {
    // SAFETY: a null sound name stops playback.
    unsafe { PlaySoundW(std::ptr::null(), std::ptr::null_mut(), 0) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn something_that_is_not_a_wav_is_refused_without_calling_windows() {
        assert!(!play_wav_blocking(b"nope"));
        assert!(!play_wav_blocking(&[0u8; 100]));
    }
}

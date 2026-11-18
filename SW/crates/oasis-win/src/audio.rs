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

/// Sets the default speakers' volume straight to `arg`: a level (`70`) or a change (`+10`, `-10`),
/// as a percent. Reads the current level first, so a change starts from where the volume is and
/// never goes to zero on the way. A level above 0 also un-mutes. `false` if there is no output device.
#[allow(unsafe_code)]
pub fn set_volume(arg: &str) -> bool {
    use windows::Win32::Media::Audio::{eConsole, eRender, Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator, MMDeviceEnumerator};
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
    // SAFETY: plain COM calls on this thread; one that is already in another COM mode is fine as it is.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(devices) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else { return false };
        let Ok(speakers) = devices.GetDefaultAudioEndpoint(eRender, eConsole) else { return false };
        let Ok(volume) = speakers.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) else { return false };
        let current = volume.GetMasterVolumeLevelScalar().map_or(0.0, |v| v * 100.0);
        let Some(level) = target_percent(arg, current) else { return false };
        volume.SetMasterVolumeLevelScalar(level / 100.0, std::ptr::null()).is_ok() && (level == 0.0 || volume.SetMute(false, std::ptr::null()).is_ok())
    }
}

/// The level `arg` asks for, 0 to 100: `70` is 70; `+10` and `-10` change `current` by that much.
fn target_percent(arg: &str, current: f32) -> Option<f32> {
    let digits = arg.strip_prefix(['+', '-']).unwrap_or(arg);
    let n = f32::from(digits.parse::<u8>().ok()?);
    Some(match arg.as_bytes().first()? {
        b'+' => current + n,
        b'-' => current - n,
        _ => n,
    }
    .clamp(0.0, 100.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_level_is_taken_as_it_is_and_a_change_starts_from_the_current_volume() {
        assert_eq!(target_percent("70", 20.0), Some(70.0));
        assert_eq!(target_percent("+10", 20.0), Some(30.0));
        assert_eq!(target_percent("-30", 20.0), Some(0.0));
        assert_eq!(target_percent("+90", 50.0), Some(100.0));
        assert_eq!(target_percent("abc", 0.0), None);
    }

    #[test]
    fn something_that_is_not_a_wav_is_refused_without_calling_windows() {
        assert!(!play_wav_blocking(b"nope"));
        assert!(!play_wav_blocking(&[0u8; 100]));
    }
}

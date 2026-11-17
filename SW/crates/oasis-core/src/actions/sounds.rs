//! The sounds a `PlaySound` step can make: short tunes, so a long automation can say by ear whether it
//! worked. Five for success and five for trouble. A note is `(hertz, milliseconds)`; 0 hertz is a rest.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sound {
    pub id: &'static str,
    pub name: &'static str,
    /// A sound for when it worked (`true`) or for when it went wrong (`false`).
    pub success: bool,
    pub notes: &'static [(u32, u32)],
}

pub const SOUNDS: [Sound; 10] = [
    Sound { id: "chime", name: "Chime", success: true, notes: &[(784, 120), (988, 120), (1319, 260)] },
    Sound { id: "bell", name: "Bell", success: true, notes: &[(1568, 90), (0, 40), (1568, 240)] },
    Sound { id: "rise", name: "Rise", success: true, notes: &[(523, 90), (659, 90), (784, 90), (1047, 260)] },
    Sound { id: "ding", name: "Ding", success: true, notes: &[(1760, 320)] },
    Sound { id: "fanfare", name: "Fanfare", success: true, notes: &[(392, 110), (523, 110), (659, 110), (784, 300)] },
    Sound { id: "buzz", name: "Buzz", success: false, notes: &[(180, 220), (0, 60), (180, 220)] },
    Sound { id: "fall", name: "Fall", success: false, notes: &[(587, 140), (494, 140), (392, 140), (262, 300)] },
    Sound { id: "alarm", name: "Alarm", success: false, notes: &[(880, 120), (660, 120), (880, 120), (660, 120), (880, 120), (660, 120)] },
    Sound { id: "thud", name: "Thud", success: false, notes: &[(110, 450)] },
    Sound { id: "warning", name: "Warning", success: false, notes: &[(440, 110), (0, 70), (440, 110), (0, 70), (440, 110)] },
];

pub fn find(id: &str) -> Option<&'static Sound> {
    SOUNDS.iter().find(|s| s.id == id.trim())
}

const RATE: u32 = 22_050;

/// The sound as a WAV file in memory: soft sine notes with a quick fade in and out, so there are no
/// clicks. One buffer for the whole tune, so it starts at once and plays without gaps.
pub fn wav(sound: &Sound) -> Vec<u8> {
    let mut pcm: Vec<i16> = Vec::new();
    for &(hz, ms) in sound.notes {
        let n = (RATE as u64 * u64::from(ms) / 1000) as usize;
        let fade = (RATE as usize * 8 / 1000).min(n / 2).max(1);
        for i in 0..n {
            if hz == 0 {
                pcm.push(0);
                continue;
            }
            let t = i as f32 / RATE as f32;
            let w = 2.0 * std::f32::consts::PI * hz as f32 * t;
            // a little second harmonic, so it sounds like a bell and not a test tone
            let tone = w.sin() + 0.25 * (2.0 * w).sin();
            let edge = (i.min(n - 1 - i) as f32 / fade as f32).min(1.0);
            let decay = 1.0 - 0.5 * (i as f32 / n as f32);
            pcm.push((tone * edge * decay * 0.3 * f32::from(i16::MAX)) as i16);
        }
    }
    let data = (pcm.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for s in pcm {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// The same, built once per sound and kept: playing it in the background needs the bytes to stay put.
pub fn wav_cached(id: &str) -> Option<&'static [u8]> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<&'static str, &'static [u8]>>> = OnceLock::new();
    let sound = find(id)?;
    let mut cache = CACHE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    Some(*cache.entry(sound.id).or_insert_with(|| Box::leak(wav(sound).into_boxed_slice())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_for_success_five_for_trouble_each_findable() {
        assert_eq!(SOUNDS.iter().filter(|s| s.success).count(), 5);
        assert_eq!(SOUNDS.iter().filter(|s| !s.success).count(), 5);
        for s in SOUNDS {
            assert_eq!(find(s.id), Some(&s));
            // Windows plays 37 to 32767 Hz; a rest is 0. A tune stays short: it must not hold up a pad.
            assert!(s.notes.iter().all(|&(hz, ms)| (hz == 0 || (37..=32767).contains(&hz)) && ms > 0));
            assert!(s.notes.iter().map(|n| n.1).sum::<u32>() <= 1500, "{}", s.id);
        }
        assert!(find("nope").is_none());
    }

    #[test]
    fn every_sound_makes_a_whole_wav_file() {
        for s in SOUNDS {
            let w = wav(&s);
            assert_eq!(&w[..4], b"RIFF");
            assert_eq!(&w[8..16], b"WAVEfmt ");
            assert_eq!(u32::from_le_bytes(w[4..8].try_into().unwrap()), w.len() as u32 - 8);
            assert!(w.len() > 44 && wav_cached(s.id).is_some_and(|c| c == &w[..]));
        }
    }
}

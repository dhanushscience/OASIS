//! Turning what the microphone gives (any channel count, any rate) into what the speech model
//! wants: mono, 16 kHz, f32 in [-1, 1].

/// Speech models are trained on 16 kHz audio.
pub const MODEL_RATE: u32 = 16_000;

/// Average interleaved channels into one.
pub fn to_mono(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Resample mono audio from `from` Hz to [`MODEL_RATE`]. Going down averages the source samples
/// that fall inside each output sample (a cheap low-pass, so tones above the new Nyquist do not
/// fold back into speech); going up interpolates.
pub fn to_model_rate(samples: &[f32], from: u32) -> Vec<f32> {
    if from == MODEL_RATE || samples.is_empty() || from == 0 {
        return samples.to_vec();
    }
    to_model_rate_at(samples, from, 0.0).0
}

/// [`to_model_rate`] for one piece of a longer recording: the first output sample starts `phase`
/// source samples into `samples`. Also returns where the next output sample would start; the caller
/// drops the whole samples before it and carries the fraction as the next `phase`, so pieces join
/// without a gap or drift.
pub fn to_model_rate_at(samples: &[f32], from: u32, phase: f64) -> (Vec<f32>, f64) {
    if from == MODEL_RATE || from == 0 {
        return (samples.to_vec(), samples.len() as f64);
    }
    let ratio = f64::from(from) / f64::from(MODEL_RATE);
    let out_len = ((samples.len() as f64 - phase) / ratio).floor().max(0.0) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let start = phase + i as f64 * ratio;
        let end = (start + ratio).min(samples.len() as f64);
        if ratio > 1.0 {
            let (a, b) = (start.floor() as usize, (end.ceil() as usize).min(samples.len()));
            let window = &samples[a..b.max(a + 1).min(samples.len())];
            out.push(window.iter().sum::<f32>() / window.len() as f32);
        } else {
            let a = start.floor() as usize;
            let next = (a + 1).min(samples.len() - 1);
            let t = (start - a as f64) as f32;
            out.push(samples[a] * (1.0 - t) + samples[next] * t);
        }
    }
    (out, phase + out_len as f64 * ratio)
}

/// Loudness of a block, 0.0 to 1.0, for a level meter.
pub fn level(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mean_square = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    (mean_square.sqrt() * 3.0).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_is_averaged_to_mono() {
        assert_eq!(to_mono(&[1.0, 0.0, 0.5, 0.5], 2), [0.5, 0.5]);
        assert_eq!(to_mono(&[0.25, 0.75], 1), [0.25, 0.75]);
        assert_eq!(to_mono(&[1.0, 1.0, 1.0, 0.0, 0.0, 0.0], 3), [1.0, 0.0]);
    }

    #[test]
    fn a_rate_already_right_is_left_alone() {
        let audio = vec![0.1, 0.2, 0.3];
        assert_eq!(to_model_rate(&audio, MODEL_RATE), audio);
    }

    #[test]
    fn downsampling_gives_the_right_length_and_keeps_a_steady_level() {
        let one_second_48k = vec![0.5; 48_000];
        let out = to_model_rate(&one_second_48k, 48_000);
        assert_eq!(out.len(), 16_000);
        assert!(out.iter().all(|s| (s - 0.5).abs() < 1e-6));
        assert_eq!(to_model_rate(&vec![0.0; 44_100], 44_100).len(), 16_000);
    }

    #[test]
    fn a_tone_above_the_new_nyquist_is_flattened_not_folded_into_speech_range() {
        // 12 kHz at 48 kHz sampling: above the 8 kHz Nyquist of 16 kHz audio.
        let tone: Vec<f32> = (0..48_000).map(|i| (i as f32 * 2.0 * std::f32::consts::PI * 12_000.0 / 48_000.0).sin()).collect();
        let out = to_model_rate(&tone, 48_000);
        let rms = (out.iter().map(|s| s * s).sum::<f32>() / out.len() as f32).sqrt();
        assert!(rms < 0.5, "the averaging low-pass must cut it, got {rms}");
    }

    #[test]
    fn upsampling_interpolates() {
        let out = to_model_rate(&[0.0, 1.0, 0.0, 1.0], 8_000);
        assert_eq!(out.len(), 8);
        assert!((out[1] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn silence_has_no_level_and_a_loud_signal_is_capped() {
        assert_eq!(level(&[0.0; 100]), 0.0);
        assert_eq!(level(&[]), 0.0);
        assert_eq!(level(&[1.0; 100]), 1.0);
    }
}

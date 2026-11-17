//! Reading a 16-bit PCM WAV file as the model's 16 kHz mono, for the examples.

use std::path::Path;

use oasis_models::AudioClip;
use oasis_voice::resample::{to_model_rate, to_mono};

pub fn read_wav(path: &Path) -> Result<AudioClip, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let find = |tag: &[u8]| bytes.windows(4).position(|w| w == tag);
    let fmt = find(b"fmt ").ok_or("no fmt chunk")? + 8;
    let channels = u16::from_le_bytes([bytes[fmt + 2], bytes[fmt + 3]]) as usize;
    let rate = u32::from_le_bytes([
        bytes[fmt + 4],
        bytes[fmt + 5],
        bytes[fmt + 6],
        bytes[fmt + 7],
    ]);
    let bits = u16::from_le_bytes([bytes[fmt + 14], bytes[fmt + 15]]);
    if bits != 16 {
        return Err(format!("only 16-bit PCM is read, this is {bits}-bit"));
    }
    let data = find(b"data").ok_or("no data chunk")? + 8;
    let samples: Vec<f32> = bytes[data..]
        .chunks_exact(2)
        .map(|c| f32::from(i16::from_le_bytes([c[0], c[1]])) / 32768.0)
        .collect();
    Ok(AudioClip {
        samples: to_model_rate(&to_mono(&samples, channels), rate),
        sample_rate: 16_000,
    })
}

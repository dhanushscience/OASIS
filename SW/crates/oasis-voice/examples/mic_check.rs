//! Open the microphone for a moment and report how much audio arrived and how loud it was.
//! Nothing is saved or transcribed. `cargo run -p oasis-voice --example mic_check -- [device name]`

use std::time::Duration;

use oasis_voice::resample::level;
use oasis_voice::{device_names, Microphone};

fn main() -> Result<(), String> {
    let wanted = std::env::args().nth(1).unwrap_or_default();
    println!("microphones: {:?}", device_names());
    let mic = Microphone::new(&wanted);
    mic.start()?;
    std::thread::sleep(Duration::from_millis(1500));
    let clip = mic.stop();
    let peak = clip.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    println!(
        "heard {:.2}s at {} Hz, loudness {:.3}, peak {:.3}",
        clip.samples.len() as f32 / clip.sample_rate as f32,
        clip.sample_rate,
        level(&clip.samples),
        peak
    );
    Ok(())
}

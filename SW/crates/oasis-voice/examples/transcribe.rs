//! Transcribe a 16-bit PCM WAV file with the on-device model, to check voice end to end:
//! `cargo run -p oasis-voice --example transcribe -- <wav> <model-folder> <dll-folder>`

use std::path::Path;

use oasis_models::SttEngine;
use oasis_voice::{detect_model, SherpaTransducer};

#[path = "common/wav.rs"]
mod wav;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [path, model, dlls] = args.as_slice() else {
        return Err("usage: transcribe <wav> <model-folder> <dll-folder>".into());
    };
    let layout =
        detect_model(Path::new(model), false).ok_or("that folder is not a speech model")?;
    let mut engine = SherpaTransducer::load_model(Path::new(dlls), &layout, 4, "cpu")
        .map_err(|e| e.to_string())?;
    let clip = wav::read_wav(Path::new(path))?;
    let started = std::time::Instant::now();
    let text = engine.transcribe(&clip).map_err(|e| e.to_string())?;
    println!("{:.1}s of audio -> \"{text}\" in {} ms", clip.samples.len() as f32 / 16_000.0, started.elapsed().as_millis());
    Ok(())
}

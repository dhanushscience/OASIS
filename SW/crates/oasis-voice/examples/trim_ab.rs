//! A/B of trimming leading and trailing quiet before speech-to-text (spec 1.3 "Trim").
//!
//! `cargo run --release -p oasis-voice --example trim_ab -- <model-folder> <dll-folder> <folder>... [--threads N]`
//!
//! Every `<name>.wav` in each folder is decoded whole and after `trim_to_speech`. The reference is
//! `<name>.txt`, or the `"text"` field of `<name>.json` (DictationHistory); without one, only the
//! two transcripts are compared. Words are compared lowercased without punctuation. Prints one
//! line per clip and a summary: clips where trimmed is worse / better / same, words lost, decode time.

use std::path::Path;
use std::time::Instant;

use oasis_core::speech::audio::{has_speech, trim_to_speech};
use oasis_models::{AudioClip, SttEngine};
use oasis_voice::{detect_model, SherpaTransducer};

#[path = "common/wav.rs"]
mod wav;

fn main() -> Result<(), String> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let threads = match args.iter().position(|a| a == "--threads") {
        Some(i) => {
            let n = args.get(i + 1).and_then(|t| t.parse().ok()).ok_or("--threads needs a number")?;
            args.drain(i..i + 2);
            n
        }
        None => 4,
    };
    let [model, dlls, folders @ ..] = args.as_slice() else { return Err("usage: trim_ab <model-folder> <dll-folder> <folder>... [--threads N]".into()) };
    let layout = detect_model(Path::new(model), false).ok_or("that folder is not a speech model")?;
    let mut engine = SherpaTransducer::load_model(Path::new(dlls), &layout, threads, "cpu").map_err(|e| e.to_string())?;
    let _ = engine.transcribe(&AudioClip { samples: vec![0.0; 8_000], sample_rate: 16_000 }); // warm up

    let (mut worse, mut better, mut same, mut changed, mut no_speech, mut lost_words) = (0, 0, 0, 0, 0, 0);
    let (mut errs_whole, mut errs_trim, mut ref_words) = (0, 0, 0);
    let (mut ms_whole, mut ms_trim, mut s_whole, mut s_trim) = (0.0, 0.0, 0.0, 0.0);
    for folder in folders {
        let mut paths: Vec<_> = std::fs::read_dir(folder).map_err(|e| e.to_string())?.filter_map(|e| Some(e.ok()?.path())).filter(|p| p.extension().is_some_and(|x| x == "wav")).collect();
        paths.sort();
        for path in &paths {
            let Ok(clip) = wav::read_wav(path) else { continue };
            let trimmed = trim_to_speech(&clip.samples, 16_000);
            let (whole_text, whole_ms) = decode(&mut engine, &clip.samples);
            let (trim_text, trim_ms) = decode(&mut engine, trimmed);
            let (whole, trim) = (words(&whole_text), words(&trim_text));
            let reference = reference(path).map(|r| words(&r));
            let (ew, et) = match &reference {
                Some(r) => (edit_distance(r, &whole), edit_distance(r, &trim)),
                None => (0, edit_distance(&whole, &trim)), // no reference: the whole decode is the reference
            };
            let speech = has_speech(&clip.samples, 16_000);
            if !speech {
                no_speech += 1;
            }
            // Words the whole decode has and the trimmed one does not (a real word dropped).
            let lost = whole.iter().filter(|w| !trim.contains(w)).count();
            if speech {
                lost_words += lost;
            }
            match et.cmp(&ew) {
                std::cmp::Ordering::Greater => worse += 1,
                std::cmp::Ordering::Less => better += 1,
                std::cmp::Ordering::Equal => same += 1,
            }
            changed += usize::from(whole != trim);
            errs_whole += ew;
            errs_trim += et;
            ref_words += reference.as_ref().map_or(whole.len(), Vec::len);
            ms_whole += whole_ms;
            ms_trim += trim_ms;
            s_whole += clip.samples.len() as f64 / 16_000.0;
            s_trim += trimmed.len() as f64 / 16_000.0;
            let name = path.file_stem().unwrap_or_default().to_string_lossy();
            println!(
                "{name} speech={speech} {:.1}s->{:.1}s err {ew}->{et} lost {lost} dec {whole_ms:.0}->{trim_ms:.0}ms\n  whole: {whole_text}\n  trim:  {trim_text}",
                clip.samples.len() as f64 / 16_000.0,
                trimmed.len() as f64 / 16_000.0
            );
        }
    }
    let n = worse + better + same;
    println!("\nclips {n} (no speech {no_speech}): trimmed worse {worse}, better {better}, same {same}; transcript changed {changed}");
    println!("word errors vs reference: whole {errs_whole}, trimmed {errs_trim} of {ref_words} words; words lost on speech clips {lost_words}");
    println!("audio {s_whole:.1}s -> {s_trim:.1}s; decode {ms_whole:.0}ms -> {ms_trim:.0}ms (saved {:.0}ms, {:.0}%)", ms_whole - ms_trim, 100.0 * (ms_whole - ms_trim) / ms_whole.max(1.0));
    Ok(())
}

fn decode(engine: &mut SherpaTransducer, samples: &[f32]) -> (String, f64) {
    let started = Instant::now();
    let text = engine.transcribe(&AudioClip { samples: samples.to_vec(), sample_rate: 16_000 }).unwrap_or_default();
    (text, started.elapsed().as_secs_f64() * 1000.0)
}

/// `<name>.txt`, or the `"text"` string of `<name>.json`.
fn reference(wav: &Path) -> Option<String> {
    if let Ok(t) = std::fs::read_to_string(wav.with_extension("txt")) {
        return Some(t);
    }
    // shortcut: reads one JSON string field by hand (no serde in this crate); escapes other than \" are kept as is.
    let json = std::fs::read_to_string(wav.with_extension("json")).ok()?;
    let start = json.find("\"text\"")? + 6;
    let rest = &json[start..];
    let rest = &rest[rest.find('"')? + 1..];
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.extend(chars.next()),
            '"' => return Some(out),
            c => out.push(c),
        }
    }
    None
}

fn words(text: &str) -> Vec<String> {
    text.split_whitespace().map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()).filter(|w| !w.is_empty()).collect()
}

fn edit_distance(a: &[String], b: &[String]) -> usize {
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.iter().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            let cur = (prev + usize::from(x != y)).min(row[j] + 1).min(row[j + 1] + 1);
            prev = row[j + 1];
            row[j + 1] = cur;
        }
    }
    row[b.len()]
}

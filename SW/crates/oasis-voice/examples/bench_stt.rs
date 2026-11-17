//! Phase 0 bench (docs/VOICE_PIPELINE.md §6): speed, accuracy and memory of one speech model on this PC.
//!
//! `cargo run --release -p oasis-voice --example bench_stt -- <model-folder> <dll-folder> <clips-folder> [threads] [runs]`
//!
//! Every `<name>.wav` in the clips folder needs `<name>.txt` beside it with what was said. Each clip is
//! decoded once to warm up, then `runs` times (default 5). Prints one line per clip, then a summary as
//! JSON on the last line. Word error rate compares lower-case words with punctuation removed and number
//! words turned into digits on both sides, so "forty" and "40" agree.

#![allow(unsafe_code)] // one Windows call, at the bottom, to read this process's peak memory

use std::path::Path;
use std::time::Instant;

use oasis_models::SttEngine;
use oasis_voice::{detect_model, SherpaTransducer};

#[path = "common/wav.rs"]
mod wav;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (model, dlls, clips) = match args.as_slice() {
        [m, d, c, ..] => (Path::new(m), Path::new(d), Path::new(c)),
        _ => {
            return Err(
                "usage: bench_stt <model-folder> <dll-folder> <clips-folder> [threads] [runs]"
                    .into(),
            )
        }
    };
    let threads: i32 = args.get(3).and_then(|t| t.parse().ok()).unwrap_or(4);
    let runs: usize = args.get(4).and_then(|r| r.parse().ok()).unwrap_or(5).max(1);

    let layout = detect_model(model, false).ok_or("that folder is not a speech model")?;
    let base_mb = peak_mb();
    let loading = Instant::now();
    let mut engine =
        SherpaTransducer::load_model(dlls, &layout, threads, "cpu").map_err(|e| e.to_string())?;
    let load_ms = loading.elapsed().as_millis();

    let mut names: Vec<_> = std::fs::read_dir(clips)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "wav") && p.with_extension("txt").is_file())
        .collect();
    names.sort();

    let (mut audio_s, mut decode_s, mut errors, mut words) = (0.0f64, 0.0f64, 0usize, 0usize);
    let mut all_ms: Vec<f64> = Vec::new();
    // Clips up to 5 s: the wait after release once the audio is decoded in segments while talking.
    let mut short_ms: Vec<f64> = Vec::new();
    for path in &names {
        let clip = wav::read_wav(path)?;
        let reference =
            std::fs::read_to_string(path.with_extension("txt")).map_err(|e| e.to_string())?;
        let seconds = clip.samples.len() as f64 / 16_000.0;
        let mut text = engine.transcribe(&clip).map_err(|e| e.to_string())?; // warm-up
        let mut times = Vec::with_capacity(runs);
        for _ in 0..runs {
            let started = Instant::now();
            text = engine.transcribe(&clip).map_err(|e| e.to_string())?;
            times.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        times.sort_by(f64::total_cmp);
        let median = times[times.len() / 2];
        let (ref_words, hyp_words) = (normal_words(&reference), normal_words(&text));
        let wrong = edit_distance(&ref_words, &hyp_words);
        audio_s += seconds;
        decode_s += median / 1000.0;
        errors += wrong;
        words += ref_words.len();
        all_ms.extend(&times);
        if seconds <= 5.0 {
            short_ms.extend(&times);
        }
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        println!(
            "{name:<12} {seconds:5.1}s  median {median:7.1} ms  rtf {:.3}  errors {wrong}/{}  \"{text}\"",
            median / 1000.0 / seconds,
            ref_words.len()
        );
    }
    let model_name = model
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    println!(
        "{{\"model\":\"{model_name}\",\"threads\":{threads},\"clips\":{},\"audio_s\":{audio_s:.1},\"load_ms\":{load_ms},\"rtf\":{:.4},\"wer_pct\":{:.2},\"short_p50_ms\":{:.0},\"short_p95_ms\":{:.0},\"all_p95_ms\":{:.0},\"peak_mb\":{},\"model_mb\":{}}}",
        names.len(),
        decode_s / audio_s.max(f64::EPSILON),
        100.0 * errors as f64 / words.max(1) as f64,
        percentile(&mut short_ms.clone(), 0.50),
        percentile(&mut short_ms, 0.95),
        percentile(&mut all_ms, 0.95),
        peak_mb().saturating_sub(base_mb),
        folder_mb(model),
    );
    Ok(())
}

/// The value at `p` (0 to 1), nearest rank.
fn percentile(values: &mut [f64], p: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    values
        .get(((values.len() as f64 * p).ceil() as usize).saturating_sub(1))
        .copied()
        .unwrap_or(f64::NAN)
}

fn normal_words(text: &str) -> Vec<String> {
    let lowered: String = text
        .to_lowercase()
        .replace('%', " percent")
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '\'' {
                c
            } else {
                ' '
            }
        })
        .collect();
    oasis_core::speech::numbers::apply(&lowered)
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// Word-level Levenshtein distance: substitutions, insertions and deletions.
fn edit_distance(a: &[String], b: &[String]) -> usize {
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.iter().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = if x == y {
                prev
            } else {
                1 + prev.min(row[j]).min(row[j + 1])
            };
            prev = cur;
        }
    }
    row[b.len()]
}

fn folder_mb(folder: &Path) -> u64 {
    std::fs::read_dir(folder)
        .map(|d| {
            d.filter_map(|e| e.ok()?.metadata().ok())
                .filter(|m| m.is_file())
                .map(|m| m.len())
                .sum::<u64>()
                / 1_048_576
        })
        .unwrap_or(0)
}

/// `PROCESS_MEMORY_COUNTERS`.
#[repr(C)]
#[derive(Default)]
struct MemoryCounters {
    cb: u32,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> isize;
    fn K32GetProcessMemoryInfo(process: isize, counters: *mut MemoryCounters, size: u32) -> i32;
}

/// This process's peak working set, in MB.
fn peak_mb() -> u64 {
    let mut counters = MemoryCounters {
        cb: std::mem::size_of::<MemoryCounters>() as u32,
        ..MemoryCounters::default()
    };
    // SAFETY: the pseudo-handle from GetCurrentProcess is always valid; `counters` is a correctly
    // sized PROCESS_MEMORY_COUNTERS that lives across the call.
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
    if ok == 0 {
        0
    } else {
        counters.peak_working_set_size as u64 / 1_048_576
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<String> {
        normal_words(s)
    }

    #[test]
    fn errors_are_counted_per_word_and_numbers_agree() {
        assert_eq!(
            edit_distance(
                &w("Set the volume to forty percent."),
                &w("set the volume to 40 percent")
            ),
            0
        );
        assert_eq!(
            edit_distance(
                &w("Set the volume to forty percent."),
                &w("Set the volume to 40%.")
            ),
            0
        );
        assert_eq!(
            edit_distance(&w("open chrome now"), &w("open crime now")),
            1
        );
        assert_eq!(edit_distance(&w("a b c"), &w("a c")), 1);
        assert_eq!(edit_distance(&w("a b"), &w("a b c d")), 2);
        assert_eq!(edit_distance(&w(""), &w("x")), 1);
    }

    #[test]
    fn the_95th_percentile_is_the_nearest_rank() {
        let mut v: Vec<f64> = (1..=20).map(f64::from).collect();
        assert_eq!(percentile(&mut v, 0.95), 19.0);
        assert_eq!(percentile(&mut v, 0.50), 10.0);
        assert!(percentile(&mut [], 0.5).is_nan());
    }
}

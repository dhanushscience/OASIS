//! Offline replay of recorded dictations at several pause-cutting settings (spec 1.4).
//!
//! `cargo run --release -p oasis-voice --example replay_pauses -- <model-folder> <dll-folder> <history-folder> [threads]`
//!
//! Every `<name>.wav` in the history folder is cut with the real `cut_at_pause`, fed in 100 ms steps
//! like the live loop, at: whole recording, 400 ms/3 s, 850 ms/5 s, 1.2 s/8 s. The pieces are decoded
//! and joined; prints false breaks (a full stop followed by a function word) before and after the
//! join fix, those of them at a piece join (the only ones the join can touch; the rest are inside
//! one piece, mostly real sentence ends), pieces per clip, and the post-release wait (decode time
//! of the last piece).
//! `join` is a copy of `join_dictation_pieces` in oasis-engine (not reachable from this crate).

use std::path::Path;
use std::time::Instant;

use oasis_core::speech::audio::{cut_at_pause, has_speech, starts_paragraph};
use oasis_models::{AudioClip, SttEngine};
use oasis_voice::{detect_model, SherpaTransducer};

#[path = "common/wav.rs"]
mod wav;

const SETTINGS: [(&str, usize, usize); 4] = [("whole", 0, 0), ("400ms/3s", 400, 3000), ("850ms/5s", 850, 5000), ("1200ms/8s", 1200, 8000)];
const FUNCTION_WORDS: [&str; 24] = [
    "to", "that", "and", "so", "use", "when", "the", "but", "or", "then", "if", "because", "it", "this", "a", "of", "in", "for", "with",
    "is", "are", "on", "at", "by",
];

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [model, dlls, history, ..] = args.as_slice() else { return Err("usage: replay_pauses <model-folder> <dll-folder> <history-folder> [threads]".into()) };
    let threads: i32 = args.get(3).and_then(|t| t.parse().ok()).unwrap_or(4);
    let layout = detect_model(Path::new(model), false).ok_or("that folder is not a speech model")?;
    let mut engine = SherpaTransducer::load_model(Path::new(dlls), &layout, threads, "cpu").map_err(|e| e.to_string())?;

    let mut paths: Vec<_> = std::fs::read_dir(history).map_err(|e| e.to_string())?.filter_map(|e| Some(e.ok()?.path())).filter(|p| p.extension().is_some_and(|x| x == "wav")).collect();
    paths.sort();
    // per setting: clips, pieces, false breaks raw / joined, last-piece seconds and decode ms, joined false breaks at joins
    let mut tot = vec![(0usize, 0usize, 0usize, 0usize, Vec::<f64>::new(), Vec::<f64>::new(), 0usize); SETTINGS.len()];
    let mut skipped = 0;
    for path in &paths {
        let Ok(clip) = wav::read_wav(path) else { skipped += 1; continue };
        if !has_speech(&clip.samples, 16_000) {
            skipped += 1;
            continue;
        }
        for (i, (name, pause, min)) in SETTINGS.iter().enumerate() {
            let pieces = cut(&clip.samples, *pause, *min);
            let mut texts = Vec::new();
            let mut para = Vec::new();
            let (mut last_s, mut last_ms) = (0.0, 0.0);
            for (n, p) in pieces.iter().enumerate() {
                let started = Instant::now();
                let text = engine.transcribe(&AudioClip { samples: p.clone(), sample_rate: 16_000 }).unwrap_or_default();
                last_ms = started.elapsed().as_secs_f64() * 1000.0;
                last_s = p.len() as f64 / 16_000.0;
                if n > 0 && starts_paragraph(p, 16_000) {
                    para.push(text.split_whitespace().next().unwrap_or("").to_string());
                }
                texts.push(text);
            }
            let raw = texts.iter().map(|t| t.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ");
            let joined = join(&texts, &para);
            let at_joins = false_breaks(&joined).saturating_sub(texts.iter().map(|t| false_breaks(t)).sum());
            let t = &mut tot[i];
            t.0 += 1;
            t.1 += pieces.len();
            t.2 += false_breaks(&raw);
            t.3 += false_breaks(&joined);
            t.4.push(last_s);
            t.5.push(last_ms);
            t.6 += at_joins;
            println!("{} {name:<10} pieces {} raw_fb {} joined_fb {} at_joins {at_joins}  last {last_s:.1}s/{last_ms:.0}ms | {joined}", path.file_stem().unwrap().to_string_lossy(), pieces.len(), false_breaks(&raw), false_breaks(&joined));
        }
    }
    println!("\nskipped (no speech / unreadable): {skipped}");
    println!("{:<10} {:>5} {:>7} {:>9} {:>9} {:>10} {:>10} {:>10} {:>10} {:>8}", "setting", "clips", "pieces", "raw_fb", "joined_fb", "last_s_avg", "last_s_p95", "dec_ms_avg", "dec_ms_p95", "at_joins");
    for ((name, _, _), t) in SETTINGS.iter().zip(&mut tot) {
        let avg = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
        let p95 = |v: &mut Vec<f64>| {
            v.sort_by(f64::total_cmp);
            v.get(((v.len() as f64 * 0.95).ceil() as usize).saturating_sub(1)).copied().unwrap_or(f64::NAN)
        };
        println!("{name:<10} {:>5} {:>7} {:>9} {:>9} {:>10.1} {:>10.1} {:>10.0} {:>10.0} {:>8}", t.0, t.1, t.2, t.3, avg(&t.4), p95(&mut t.4), avg(&t.5), p95(&mut t.5), t.6);
    }
    Ok(())
}

/// The pieces the live loop would send: every 100 ms, cut at the first long enough pause.
fn cut(samples: &[f32], pause: usize, min: usize) -> Vec<Vec<f32>> {
    if pause == 0 {
        return vec![samples.to_vec()];
    }
    let (mut out, mut unsent) = (Vec::new(), Vec::new());
    for chunk in samples.chunks(1600) {
        unsent.extend_from_slice(chunk);
        while let Some(c) = cut_at_pause(&unsent, 16_000, pause, min) {
            let rest = unsent.split_off(c.min(unsent.len()));
            out.push(std::mem::replace(&mut unsent, rest));
        }
    }
    out.push(unsent);
    out
}

fn bare(token: &str) -> &str {
    token.trim_matches(|c: char| !c.is_alphanumeric())
}

/// Full stops directly followed by a function word, such as "to. Use" or "that. And".
fn false_breaks(text: &str) -> usize {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    tokens.windows(2).filter(|p| p[0].ends_with('.') && !p[0].ends_with("..") && FUNCTION_WORDS.contains(&bare(p[1]).to_lowercase().as_str())).count()
}

const CAPITAL_WORDS: [&str; 19] = [
    "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday", "january", "february", "march", "april", "june", "july", "august", "september", "october", "november", "december", "oasis",
];

/// Copy of `join_dictation_pieces` (oasis-engine voice.rs) with an empty Dictionary.
fn join(pieces: &[String], paragraphs: &[String]) -> String {
    let mut names: Vec<String> = Vec::new();
    for piece in pieces {
        let tokens: Vec<&str> = piece.split_whitespace().collect();
        for pair in tokens.windows(2) {
            if !pair[0].ends_with(['.', '!', '?']) && bare(pair[1]).starts_with(char::is_uppercase) {
                names.push(bare(pair[1]).to_lowercase());
            }
        }
    }
    let mut out = String::new();
    for piece in pieces.iter().map(|p| p.trim()).filter(|p| !p.is_empty()) {
        if out.is_empty() {
            out.push_str(piece);
            continue;
        }
        let opens_paragraph = paragraphs.iter().any(|o| !o.is_empty() && piece.starts_with(o.as_str()));
        if !out.ends_with(['.', '!', '?']) && opens_paragraph {
            out.truncate(out.trim_end_matches(',').len());
            out.push('.');
        }
        let word: String = piece.chars().take_while(|c| c.is_alphanumeric() || *c == '\'').collect();
        const NEVER_LAST: [&str; 13] = ["the", "an", "and", "or", "of", "my", "your", "our", "their", "because", "than", "which", "where"];
        const RARELY_LAST: [&str; 13] = ["to", "a", "but", "so", "that", "with", "for", "in", "on", "at", "by", "if", "into"];
        const OPENERS: [&str; 12] = ["and", "so", "use", "when", "the", "but", "or", "then", "if", "because", "it", "this"];
        let next = word.to_lowercase();
        let last = out.split_whitespace().last().map(|w| bare(w).to_lowercase()).unwrap_or_default();
        let continues = ["than", "with", "into"].contains(&next.as_str())
            || (next == "so" && piece.split_whitespace().nth(1).is_some_and(|w| bare(w).eq_ignore_ascii_case("that")));
        if out.ends_with('.') && !out.ends_with("..") && !opens_paragraph
            && (NEVER_LAST.contains(&last.as_str()) || (RARELY_LAST.contains(&last.as_str()) && OPENERS.contains(&next.as_str())) || continues)
        {
            out.pop();
        }
        let keeps = word == "I" || word.starts_with("I'") || word.chars().skip(1).any(|c| c.is_uppercase() || c.is_ascii_digit()) || CAPITAL_WORDS.contains(&word.to_lowercase().as_str()) || names.contains(&word.to_lowercase());
        let mut chars = piece.chars();
        let first = chars.next().unwrap_or(' ');
        let ended = out.ends_with(['.', '!', '?']);
        out.push(' ');
        if ended {
            out.extend(first.to_uppercase());
        } else if keeps {
            out.push(first);
        } else {
            out.extend(first.to_lowercase());
        }
        out.push_str(chars.as_str());
    }
    out
}

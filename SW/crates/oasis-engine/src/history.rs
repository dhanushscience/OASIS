//! The dictation history: when the user switched it on (Voice › "Keep my dictation history"), each
//! dictation is kept in `DictationHistory` as `<date>_<time>.wav` (what was recorded) beside
//! `<date>_<time>.json` (what the speech model heard and what was typed). It stays on this PC; sharing
//! the folder, to help train the speech model, is the user's own choice.

use std::fs;
use std::path::{Path, PathBuf};

use oasis_models::AudioClip;
use serde::{Deserialize, Serialize};

/// The newest entries the Insights screen lists.
const LISTED: usize = 300;

const README: &str = "Project OASIS - dictation history\r\n\r\n\
Each dictation is kept here as two files with the same name: a .wav (your voice) and a .json (what the\r\n\
speech model heard, \"raw\", and what OASIS typed, \"text\"). Nothing is sent anywhere.\r\n\r\n\
To help improve the speech model you may send this folder (or some of its files) to the developer.\r\n\
Turn the history off in OASIS, Insights, and delete files here whenever you like.\r\n";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// The file name without its extension: `2026-11-13_17-14-29`.
    pub id: String,
    /// Local time, `2026-11-13 17:14:29`.
    pub time: String,
    pub style: String,
    /// What the speech model heard, before any clean-up.
    pub raw: String,
    /// What was typed.
    pub text: String,
    pub audio_ms: u64,
    /// Whether the recording is still on disk (a user may delete it); filled in when listing.
    #[serde(default)]
    pub has_audio: bool,
}

/// `(time, id)` for a moment on the clock `offset_secs` from UTC.
fn stamp(unix: i64, offset_secs: i32) -> (String, String) {
    let local = unix + i64::from(offset_secs);
    let (days, secs) = (local.div_euclid(86_400), local.rem_euclid(86_400));
    let (y, m, d) = crate::spotify::civil_from_days(days);
    let (h, min, s) = (secs / 3600, secs % 3600 / 60, secs % 60);
    (format!("{y:04}-{m:02}-{d:02} {h:02}:{min:02}:{s:02}"), format!("{y:04}-{m:02}-{d:02}_{h:02}-{min:02}-{s:02}"))
}

/// Keep one dictation. `audio` is the whole recording, when there is one.
pub fn save(folder: &Path, unix: i64, offset_secs: i32, style: &str, raw: &str, text: &str, audio: Option<&AudioClip>) -> std::io::Result<()> {
    let (time, id) = stamp(unix, offset_secs);
    fs::create_dir_all(folder)?;
    let readme = folder.join("README.txt");
    if !readme.exists() {
        fs::write(readme, README)?;
    }
    let audio = audio.filter(|a| !a.samples.is_empty());
    let audio_ms = audio.map_or(0, |a| a.samples.len() as u64 * 1000 / u64::from(a.sample_rate.max(1)));
    if let Some(a) = audio {
        fs::write(folder.join(format!("{id}.wav")), crate::groq::wav(&a.samples, a.sample_rate))?;
    }
    let entry = Entry { id: id.clone(), time, style: style.to_string(), raw: raw.trim().to_string(), text: text.trim().to_string(), audio_ms, has_audio: false };
    fs::write(folder.join(format!("{id}.json")), serde_json::to_vec_pretty(&entry)?)
}

/// The newest entries, newest first. A file that cannot be read is skipped.
pub fn list(folder: &Path) -> Vec<Entry> {
    let Ok(dir) = fs::read_dir(folder) else { return Vec::new() };
    let mut ids: Vec<String> = dir
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().to_str()?.strip_suffix(".json").map(str::to_string))
        .filter(|id| is_id(id))
        .collect();
    ids.sort_unstable_by(|a, b| b.cmp(a));
    ids.truncate(LISTED);
    ids.iter()
        .filter_map(|id| {
            let mut entry: Entry = serde_json::from_slice(&fs::read(folder.join(format!("{id}.json"))).ok()?).ok()?;
            entry.id.clone_from(id);
            entry.has_audio = folder.join(format!("{id}.wav")).is_file();
            Some(entry)
        })
        .collect()
}

/// The recording of one entry, as a WAV file.
pub fn audio(folder: &Path, id: &str) -> Result<Vec<u8>, String> {
    if !is_id(id) {
        return Err("That is not a recording.".into());
    }
    fs::read(folder.join(format!("{id}.wav"))).map_err(|_| "The recording is not there any more.".to_string())
}

/// Forget one dictation: its recording and its text.
pub fn delete(folder: &Path, id: &str) -> Result<(), String> {
    if !is_id(id) {
        return Err("That is not a recording.".into());
    }
    for ext in ["wav", "json"] {
        match fs::remove_file(folder.join(format!("{id}.{ext}"))) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
            _ => {}
        }
    }
    Ok(())
}

/// Forget everything: every recording and text, and the exported training set. Other files are left alone.
pub fn delete_all(folder: &Path) -> Result<(), String> {
    let Ok(dir) = fs::read_dir(folder) else { return Ok(()) };
    for file in dir.filter_map(Result::ok).map(|e| e.path()) {
        let kept_id = file.file_stem().and_then(|s| s.to_str()).is_some_and(is_id)
            && matches!(file.extension().and_then(|e| e.to_str()), Some("wav" | "json"));
        if kept_id {
            fs::remove_file(&file).map_err(|e| e.to_string())?;
        }
    }
    match fs::remove_dir_all(folder.join("dataset")) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
        _ => Ok(()),
    }
}

/// Lay every kept dictation that still has its recording out as a training set in `<folder>/dataset`:
/// the `.wav` files and a `metadata.jsonl` (`file_name`, `raw`, `text`, `style`, `audio_ms`, `time`),
/// the Hugging Face "audiofolder" layout, so a tester's folder can be loaded as it is. Returns the count.
/// shortcut: rebuilt from scratch each time (copies the audio), fine for a few hundred clips.
pub fn export_dataset(folder: &Path) -> Result<(PathBuf, usize), String> {
    let dest = folder.join("dataset");
    let _ = fs::remove_dir_all(&dest);
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    let mut lines = String::new();
    let mut n = 0;
    for e in list(folder).into_iter().filter(|e| e.has_audio) {
        let file_name = format!("{}.wav", e.id);
        fs::copy(folder.join(&file_name), dest.join(&file_name)).map_err(|x| x.to_string())?;
        let row = serde_json::json!({ "file_name": file_name, "raw": e.raw, "text": e.text, "style": e.style, "audio_ms": e.audio_ms, "time": e.time });
        lines.push_str(&row.to_string());
        lines.push('\n');
        n += 1;
    }
    fs::write(dest.join("metadata.jsonl"), lines).map_err(|e| e.to_string())?;
    Ok((dest, n))
}

/// An id is only digits, `-` and `_`: the page cannot name a path outside the folder.
fn is_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 32 && id.chars().all(|c| c.is_ascii_digit() || matches!(c, '-' | '_'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kept_dictation_is_listed_newest_first_and_its_recording_can_be_read() {
        let folder = std::env::temp_dir().join(format!("oasis-history-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        let clip = AudioClip { samples: vec![0.1; 1600], sample_rate: 16_000 };
        save(&folder, 1_700_000_000, 0, "Default", " raw words ", "Typed words.", Some(&clip)).unwrap();
        save(&folder, 1_700_000_100, 0, "Exact", "no audio", "No audio", None).unwrap();

        let all = list(&folder);
        assert_eq!(all.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["2023-11-14_22-15-00", "2023-11-14_22-13-20"]);
        assert_eq!((all[1].raw.as_str(), all[1].text.as_str(), all[1].audio_ms, all[1].has_audio), ("raw words", "Typed words.", 100, true));
        assert!(!all[0].has_audio);
        assert_eq!(&audio(&folder, "2023-11-14_22-13-20").unwrap()[..4], b"RIFF");
        assert!(audio(&folder, "2023-11-14_22-15-00").is_err(), "no recording was kept");
        assert!(folder.join("README.txt").is_file());

        let (dest, n) = export_dataset(&folder).unwrap();
        assert_eq!(n, 1, "only the dictation with a recording is training data");
        let meta = fs::read_to_string(dest.join("metadata.jsonl")).unwrap();
        assert!(meta.contains("\"file_name\":\"2023-11-14_22-13-20.wav\"") && meta.contains("\"raw\":\"raw words\""));
        assert!(dest.join("2023-11-14_22-13-20.wav").is_file());

        delete(&folder, "2023-11-14_22-13-20").unwrap();
        assert_eq!(list(&folder).len(), 1);
        assert!(audio(&folder, "2023-11-14_22-13-20").is_err());
        assert!(delete(&folder, "../x").is_err());
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn the_page_cannot_name_a_path_outside_the_folder() {
        for id in ["", "..", "../secrets", "a\\b", "2023-11-14_22-13-20.wav", &"1".repeat(40)] {
            assert!(audio(Path::new("."), id).is_err(), "{id}");
        }
    }
}

//! Downloads a voice model set (`docs/VOICE_MODEL_SETUP.md` §4.4): one file at a time, written as
//! `<name>.part`, resumed with an HTTP range request after a break, checked against the SHA-256 in
//! the built-in manifest, and only then renamed into place. A file that fails the check is deleted.
//! A file already in place is skipped (it got there only after passing the check). A zip (the
//! llama.cpp runtime) is unpacked with Windows' own `tar.exe` once checked.
//!
//! A connection that stalls without dropping cannot hang the download: every request has a time
//! window ([`REQUEST_WINDOW`]). When it closes, the bytes that arrived are kept and the next request
//! asks for the rest. A slow line still finishes, window by window; only a window that brought
//! nothing at all counts as a failure. A pause therefore takes effect within one window too.
//!
//! Nothing about the user or the PC is sent: these are plain GitHub downloads.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use oasis_core::voice::models::{self, ModelFile};
use oasis_core::voice::placement::ModelSet;
use sha2::{Digest, Sha256};

/// The most one request may take, connection and body included.
pub const REQUEST_WINDOW: Duration = Duration::from_secs(30);
/// Tries per file before the download reports a failure (the caller tries again later).
const TRIES: u32 = 3;
const CHUNK: usize = 256 * 1024;

/// How far the voice model download is, in percent, for the overlay ("Voice is getting ready:
/// 48%"); `None` when no download is running. Set by the app's model manager.
static PROGRESS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(u8::MAX);

pub fn set_progress(percent: Option<u8>) {
    PROGRESS.store(percent.map_or(u8::MAX, |p| p.min(100)), Ordering::SeqCst);
}

pub fn progress() -> Option<u8> {
    Some(PROGRESS.load(Ordering::SeqCst)).filter(|p| *p <= 100)
}

/// How far the download is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub done_bytes: u64,
    pub total_bytes: u64,
    /// The file being downloaded now.
    pub asset: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadError {
    /// No connection, or the server could not be reached.
    Network(String),
    /// The server answered with an error status.
    Http(u16, &'static str),
    /// A file's SHA-256 did not match the manifest; it was deleted.
    Damaged(&'static str),
    /// Writing to the disk failed (full, or no permission).
    Disk(String),
    /// Paused by the user; resumes from where it stopped.
    Paused,
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadError::Network(why) => write!(f, "Could not reach the download server: {why}"),
            DownloadError::Http(code, asset) => write!(f, "The download server refused {asset} (HTTP {code})."),
            DownloadError::Damaged(asset) => write!(f, "{asset} arrived damaged and was deleted; it will be downloaded again."),
            DownloadError::Disk(why) => write!(f, "Could not write the voice models to the disk: {why}"),
            DownloadError::Paused => write!(f, "Paused."),
        }
    }
}

/// Download every file of `set` that is not in place yet into `models_dir`. `base_url` replaces the
/// manifest's (tests use a local server). `pause` stops it between chunks; calling again resumes.
pub fn download_set(
    set: ModelSet,
    models_dir: &Path,
    base_url: Option<&str>,
    pause: &AtomicBool,
    on_progress: &mut dyn FnMut(Progress),
) -> Result<(), DownloadError> {
    download_files(&models::files_for(set).collect::<Vec<_>>(), models_dir, base_url, REQUEST_WINDOW, pause, on_progress)
}

/// What this app put in the Models folder, written after each set is complete: the path and the
/// SHA-256 it was checked against. When a later app version names other files or other checksums,
/// what the record holds and the manifest no longer does is old and is cleared before the new files
/// are downloaded (`docs/VOICE_MODEL_SETUP.md` §4.5). Models the user put there are never listed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Installed {
    pub path: String,
    pub sha256: String,
    pub unzip: bool,
}

fn record_path(models_dir: &Path) -> PathBuf {
    models_dir.join("installed.json")
}

fn read_record(models_dir: &Path) -> Vec<Installed> {
    fs::read_to_string(record_path(models_dir)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn write_record(models_dir: &Path, record: &[Installed]) {
    if let Ok(text) = serde_json::to_string_pretty(record) {
        let _ = fs::write(record_path(models_dir), text);
    }
}

/// Note the files of `set` that are in place. Called when a set is complete, so a PC that got its
/// files before this record existed is covered the first time its set is found complete.
pub fn record_set(set: ModelSet, models_dir: &Path) {
    let mut record = read_record(models_dir);
    for file in models::files_for(set).filter(|f| in_place(f, models_dir)) {
        record.retain(|r| r.path != file.path);
        record.push(Installed { path: file.path.to_string(), sha256: file.sha256.to_string(), unzip: file.unzip });
    }
    write_record(models_dir, &record);
}

/// What the record holds that the built-in manifest does not name (the same path with another
/// checksum counts: it is an older version of that file).
pub fn stale(models_dir: &Path) -> Vec<Installed> {
    read_record(models_dir)
        .into_iter()
        .filter(|r| !models::FILES.iter().any(|f| f.path == r.path && f.sha256 == r.sha256))
        .collect()
}

/// A recorded path is trusted only if it stays inside the Models folder.
fn inside(path: &str) -> bool {
    !path.is_empty() && !path.starts_with('/') && !path.contains(['\\', ':']) && !path.split('/').any(|p| p == ".." || p.is_empty())
}

/// Delete the old files [`stale`] names, and forget them. Returns why something stayed (a file the
/// speech engine or the polish server still holds); that is tried again on the next start.
pub fn clean_stale(models_dir: &Path) -> Option<String> {
    let old = stale(models_dir);
    if old.is_empty() {
        return None;
    }
    let mut kept = Vec::new();
    let mut stuck = Vec::new();
    for entry in read_record(models_dir) {
        if !old.contains(&entry) {
            kept.push(entry);
            continue;
        }
        let path = models_dir.join(entry.path.replace('/', std::path::MAIN_SEPARATOR_STR));
        let gone = if !inside(&entry.path) {
            Ok(()) // never deleted: not a path this app would have written
        } else if entry.unzip {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        match gone {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => {
                stuck.push(format!("{} ({e})", entry.path));
                kept.push(entry);
            }
        }
    }
    write_record(models_dir, &kept);
    remove_empty_folders(models_dir);
    (!stuck.is_empty()).then(|| format!("Old voice models could not be removed yet: {}. They go when OASIS starts again.", stuck.join(", ")))
}

/// Folders the old files left empty (`stt/<old model>/`).
fn remove_empty_folders(models_dir: &Path) {
    for top in ["stt", "llm", "runtime", "tts", "vad"] {
        if let Ok(entries) = fs::read_dir(models_dir.join(top)) {
            for folder in entries.flatten().filter(|e| e.path().is_dir()) {
                let _ = fs::remove_dir(folder.path()); // fails, harmlessly, unless the folder is empty
            }
        }
    }
}

/// Unfinished downloads of files the manifest no longer names. A partial of a file it still names
/// stays: it is what a download resumes from.
pub fn clear_partials(models_dir: &Path) {
    let Ok(entries) = fs::read_dir(models_dir.join(".downloads")) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let current = name.strip_suffix(".part").is_some_and(|asset| models::FILES.iter().any(|f| f.asset == asset));
        if !current {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Whether every file of `set` is in place.
pub fn is_complete(set: ModelSet, models_dir: &Path) -> bool {
    models::files_for(set).all(|f| in_place(f, models_dir))
}

fn download_files(
    files: &[&'static ModelFile],
    models_dir: &Path,
    base_url: Option<&str>,
    window: Duration,
    pause: &AtomicBool,
    on_progress: &mut dyn FnMut(Progress),
) -> Result<(), DownloadError> {
    let total: u64 = files.iter().map(|f| f.bytes).sum();
    let mut done: u64 = files.iter().filter(|f| in_place(f, models_dir)).map(|f| f.bytes).sum();
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(window)).http_status_as_error(false).build().into();
    for file in files.iter().copied().filter(|f| !in_place(f, models_dir)) {
        let url = base_url.map_or_else(|| models::url(file), |b| format!("{b}{}", file.asset));
        let mut last = DownloadError::Network("no attempt".into());
        let mut ok = false;
        for attempt in 0..TRIES {
            if pause.load(Ordering::SeqCst) {
                return Err(DownloadError::Paused);
            }
            if attempt > 0 {
                std::thread::sleep(Duration::from_secs(2 * u64::from(attempt)));
            }
            match fetch(&agent, &url, file, models_dir, pause, &mut |bytes| on_progress(Progress { done_bytes: done + bytes, total_bytes: total, asset: file.asset })) {
                Ok(()) => {
                    ok = true;
                    break;
                }
                // A damaged file is downloaded once more from the start; a pause or a full disk stops.
                Err(e @ (DownloadError::Paused | DownloadError::Disk(_))) => return Err(e),
                // "Not found" and other refusals do not change by asking again; a timeout or a rate limit might.
                Err(e @ DownloadError::Http(code, _)) if (400..500).contains(&code) && code != 408 && code != 429 => return Err(e),
                Err(e) => last = e,
            }
        }
        if !ok {
            return Err(last);
        }
        done += file.bytes;
        on_progress(Progress { done_bytes: done, total_bytes: total, asset: file.asset });
    }
    Ok(())
}

/// Where a file ends up, and for a zip the marker that says it was unpacked.
fn final_path(file: &ModelFile, models_dir: &Path) -> PathBuf {
    let path = models_dir.join(file.path.replace('/', std::path::MAIN_SEPARATOR_STR));
    if file.unzip { path.join(format!(".unpacked-{}", &file.sha256[..16])) } else { path }
}

fn in_place(file: &ModelFile, models_dir: &Path) -> bool {
    let path = final_path(file, models_dir);
    if file.unzip {
        return path.exists();
    }
    fs::metadata(&path).is_ok_and(|m| m.len() == file.bytes)
}

/// Download one file to `.part`, resuming what is there, then check and move it into place.
fn fetch(
    agent: &ureq::Agent,
    url: &str,
    file: &'static ModelFile,
    models_dir: &Path,
    pause: &AtomicBool,
    progress: &mut dyn FnMut(u64),
) -> Result<(), DownloadError> {
    let downloads = models_dir.join(".downloads");
    fs::create_dir_all(&downloads).map_err(disk)?;
    let part = downloads.join(format!("{}.part", file.asset));
    let mut have = fs::metadata(&part).map_or(0, |m| m.len());
    if have > file.bytes {
        fs::remove_file(&part).map_err(disk)?;
        have = 0;
    }
    // Window after window until the file is whole; a window that brings nothing ends the try.
    while have < file.bytes {
        let before = have;
        match fetch_window(agent, url, file, &part, &mut have, pause, progress) {
            Ok(()) => {}
            // The window closed or the line dropped after some bytes: ask for the rest.
            Err(DownloadError::Network(_)) if have > before => continue,
            Err(e) => return Err(e),
        }
        if have == before {
            return Err(DownloadError::Network("the server sent nothing".into()));
        }
    }
    if sha256_of(&part).map_err(disk)? != file.sha256 {
        let _ = fs::remove_file(&part);
        return Err(DownloadError::Damaged(file.asset));
    }
    place(file, &part, models_dir)
}

/// One request, from byte `have` on, read until it ends or its window closes. Bytes that arrived
/// are written to `part` and counted in `have` either way.
fn fetch_window(
    agent: &ureq::Agent,
    url: &str,
    file: &'static ModelFile,
    part: &Path,
    have: &mut u64,
    pause: &AtomicBool,
    progress: &mut dyn FnMut(u64),
) -> Result<(), DownloadError> {
    let mut request = agent.get(url);
    if *have > 0 {
        request = request.header("Range", format!("bytes={have}-"));
    }
    let mut response = request.call().map_err(|e| DownloadError::Network(e.to_string()))?;
    let status = response.status().as_u16();
    let mut out = match status {
        206 => OpenOptions::new().append(true).open(part).map_err(disk)?,
        // The server sent the whole file: start over.
        200 => {
            *have = 0;
            File::create(part).map_err(disk)?
        }
        416 => {
            // Asked past the end: what is there is not what was expected.
            let _ = fs::remove_file(part);
            *have = 0;
            return Err(DownloadError::Http(status, file.asset));
        }
        _ => return Err(DownloadError::Http(status, file.asset)),
    };
    let mut body = response.body_mut().with_config().limit(file.bytes + 1).reader();
    let mut buffer = vec![0u8; CHUNK];
    let result = loop {
        if pause.load(Ordering::SeqCst) {
            break Err(DownloadError::Paused);
        }
        let n = match body.read(&mut buffer) {
            Ok(0) => break Ok(()),
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => break Err(DownloadError::Network(e.to_string())),
        };
        if let Err(e) = out.write_all(&buffer[..n]) {
            break Err(disk(e));
        }
        *have += n as u64;
        progress(*have);
    };
    out.flush().map_err(disk)?;
    result
}

/// Move a checked file into place, or unpack a checked zip and leave its marker.
fn place(file: &ModelFile, part: &Path, models_dir: &Path) -> Result<(), DownloadError> {
    let target = final_path(file, models_dir);
    if file.unzip {
        let folder = target.parent().expect("a marker sits in a folder");
        fs::create_dir_all(folder).map_err(disk)?;
        unzip(part, folder)?;
        fs::write(&target, file.sha256).map_err(disk)?;
        let _ = fs::remove_file(part);
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(disk)?;
    }
    fs::rename(part, &target).map_err(disk)
}

fn unzip(zip: &Path, folder: &Path) -> Result<(), DownloadError> {
    // Windows 10 and later ship bsdtar, which reads zip files.
    let tar = std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from("tar.exe"), |r| Path::new(&r).join("System32").join("tar.exe"));
    let mut command = std::process::Command::new(tar);
    command.arg("-xf").arg(zip).arg("-C").arg(folder);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // no console window
    }
    let status = command.status().map_err(|e| DownloadError::Disk(format!("could not unpack {}: {e}", zip.display())))?;
    if status.success() { Ok(()) } else { Err(DownloadError::Disk(format!("could not unpack {}", zip.display()))) }
}

fn sha256_of(path: &Path) -> io::Result<String> {
    let mut hasher = Sha256::new();
    let mut file = File::open(path)?;
    let mut buffer = vec![0u8; CHUNK];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn disk(e: io::Error) -> DownloadError {
    DownloadError::Disk(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis_core::voice::models::Group;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    fn sha(bytes: &[u8]) -> String {
        Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
    }

    fn leak(s: String) -> &'static str {
        Box::leak(s.into_boxed_str())
    }

    /// A test file of `len` bytes, and its manifest entry.
    fn entry(asset: &'static str, len: usize, claimed_sha: Option<&str>) -> (Vec<u8>, &'static ModelFile) {
        let body: Vec<u8> = (0..len).map(|i| (i * 7 % 251) as u8).collect();
        let sha = leak(claimed_sha.map_or_else(|| sha(&body), str::to_string));
        let file = Box::leak(Box::new(ModelFile { asset, path: leak(format!("stt/test/{asset}")), bytes: len as u64, sha256: sha, group: Group::SpeechLight, unzip: false }));
        (body, file)
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Mode {
        Ranges,
        NoRanges,
        /// The first answer sends half the file, then holds the connection open without a byte more.
        StallOnce,
    }

    /// A tiny web server for `files`. Returns its base URL and the Range headers it saw.
    fn serve(files: Vec<(&'static str, Vec<u8>)>, mode: Mode) -> (String, Arc<Mutex<Vec<String>>>) {
        let ranges = mode != Mode::NoRanges;
        let stalled = Arc::new(AtomicBool::new(false));
        let files = Arc::new(files);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen_by_server = Arc::clone(&seen);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                // Each answer on its own thread, so a stalled one does not hold up the next request.
                let (files, seen, stalled) = (Arc::clone(&files), Arc::clone(&seen_by_server), Arc::clone(&stalled));
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    let path = line.split_whitespace().nth(1).unwrap_or("/").trim_start_matches('/').to_string();
                    let mut range_from = None;
                    loop {
                        let mut header = String::new();
                        if reader.read_line(&mut header).unwrap_or(0) == 0 || header == "\r\n" {
                            break;
                        }
                        if let Some(v) = header.to_ascii_lowercase().strip_prefix("range: bytes=") {
                            seen.lock().unwrap().push(v.trim().to_string());
                            range_from = v.trim().trim_end_matches('-').parse::<usize>().ok();
                        }
                    }
                    let mut stream = stream;
                    let Some((_, body)) = files.iter().find(|(a, _)| *a == path) else {
                        let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                        return;
                    };
                    let (status, slice) = match range_from.filter(|_| ranges) {
                        Some(from) => ("206 Partial Content", &body[from.min(body.len())..]),
                        None => ("200 OK", &body[..]),
                    };
                    let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", slice.len());
                    if mode == Mode::StallOnce && !stalled.swap(true, Ordering::SeqCst) {
                        let _ = stream.write_all(&slice[..slice.len() / 2]);
                        let _ = stream.flush();
                        std::thread::sleep(Duration::from_secs(3));
                        return;
                    }
                    let _ = stream.write_all(slice);
                });
            }
        });
        (base, seen)
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oasis-dl-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn run(files: &[&'static ModelFile], dir: &Path, base: &str) -> (Result<(), DownloadError>, Vec<Progress>) {
        let mut seen = Vec::new();
        let result = download_files(files, dir, Some(base), Duration::from_millis(700), &AtomicBool::new(false), &mut |p| seen.push(p));
        (result, seen)
    }

    #[test]
    fn files_arrive_checked_and_in_place_and_are_not_fetched_twice() {
        let (a, fa) = entry("a.onnx", 300_000, None);
        let (b, fb) = entry("b.txt", 1_000, None);
        let (base, _) = serve(vec![("a.onnx", a.clone()), ("b.txt", b)], Mode::Ranges);
        let dir = temp_dir("fresh");
        let (result, progress) = run(&[fa, fb], &dir, &base);
        assert_eq!(result, Ok(()));
        assert_eq!(fs::read(dir.join("stt").join("test").join("a.onnx")).unwrap(), a);
        let last = progress.last().unwrap();
        assert_eq!((last.done_bytes, last.total_bytes), (301_000, 301_000));
        assert!(!dir.join(".downloads").join("a.onnx.part").exists(), "no part file left");
        // Second run: everything is in place, nothing is downloaded.
        let (again, progress) = run(&[fa, fb], &dir, "http://127.0.0.1:9/");
        assert_eq!(again, Ok(()));
        assert!(progress.is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_broken_download_resumes_where_it_stopped() {
        let (a, fa) = entry("big.onnx", 200_000, None);
        let (base, ranges) = serve(vec![("big.onnx", a.clone())], Mode::Ranges);
        let dir = temp_dir("resume");
        fs::create_dir_all(dir.join(".downloads")).unwrap();
        fs::write(dir.join(".downloads").join("big.onnx.part"), &a[..120_000]).unwrap();
        let (result, _) = run(&[fa], &dir, &base);
        assert_eq!(result, Ok(()));
        assert_eq!(ranges.lock().unwrap().as_slice(), ["120000-"], "asked only for the rest");
        assert_eq!(fs::read(dir.join("stt").join("test").join("big.onnx")).unwrap(), a);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_server_that_ignores_the_range_starts_the_file_over() {
        let (a, fa) = entry("c.onnx", 50_000, None);
        let (base, _) = serve(vec![("c.onnx", a.clone())], Mode::NoRanges);
        let dir = temp_dir("norange");
        fs::create_dir_all(dir.join(".downloads")).unwrap();
        fs::write(dir.join(".downloads").join("c.onnx.part"), &a[..10_000]).unwrap();
        assert_eq!(run(&[fa], &dir, &base).0, Ok(()));
        assert_eq!(fs::read(dir.join("stt").join("test").join("c.onnx")).unwrap(), a);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_connection_that_stalls_is_given_up_and_the_rest_asked_for() {
        let (a, fa) = entry("stall.onnx", 400_000, None);
        let (base, ranges) = serve(vec![("stall.onnx", a.clone())], Mode::StallOnce);
        let dir = temp_dir("stall");
        let started = std::time::Instant::now();
        assert_eq!(run(&[fa], &dir, &base).0, Ok(()));
        assert!(started.elapsed() < Duration::from_secs(3), "the stall was cut at the window, not waited out");
        assert_eq!(ranges.lock().unwrap().as_slice(), ["200000-"], "the half that arrived was kept");
        assert_eq!(fs::read(dir.join("stt").join("test").join("stall.onnx")).unwrap(), a);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_damaged_file_is_deleted_and_never_placed() {
        let (a, fa) = entry("bad.onnx", 5_000, Some(&"0".repeat(64)));
        let (base, _) = serve(vec![("bad.onnx", a)], Mode::Ranges);
        let dir = temp_dir("damaged");
        assert_eq!(run(&[fa], &dir, &base).0, Err(DownloadError::Damaged("bad.onnx")));
        assert!(!dir.join("stt").join("test").join("bad.onnx").exists());
        assert!(!dir.join(".downloads").join("bad.onnx.part").exists());
        let _ = fs::remove_dir_all(dir);
    }

    /// A models folder as an older app version left it: one file whose checksum the manifest has since
    /// changed, one that is no longer named at all, a runtime folder, and a model of the user's own.
    fn old_install(name: &str) -> PathBuf {
        let dir = temp_dir(name);
        let current = models::FILES.iter().find(|f| !f.unzip).unwrap();
        let put = |rel: &str, body: &[u8]| {
            let path = dir.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, body).unwrap();
        };
        put(current.path, b"old version");
        put("stt/retired-model/model.onnx", b"gone from the manifest");
        put("runtime/llama-old/llama-server.exe", b"old runtime");
        put("stt/my-own-model/model.onnx", b"mine");
        write_record(
            &dir,
            &[
                Installed { path: current.path.into(), sha256: "0".repeat(64), unzip: false },
                Installed { path: "stt/retired-model/model.onnx".into(), sha256: "1".repeat(64), unzip: false },
                Installed { path: "runtime/llama-old".into(), sha256: "2".repeat(64), unzip: true },
            ],
        );
        dir
    }

    #[test]
    fn old_files_are_found_and_cleared_and_the_users_own_models_stay() {
        let dir = old_install("stale");
        let current = models::FILES.iter().find(|f| !f.unzip).unwrap();
        assert_eq!(stale(&dir).len(), 3);
        assert_eq!(clean_stale(&dir), None);
        assert!(!dir.join(current.path.replace('/', std::path::MAIN_SEPARATOR_STR)).exists(), "the older version of a file is deleted");
        assert!(!dir.join("stt").join("retired-model").exists(), "a file the manifest dropped goes, and its empty folder");
        assert!(!dir.join("runtime").join("llama-old").exists(), "an old runtime folder goes whole");
        assert!(dir.join("stt").join("my-own-model").join("model.onnx").exists());
        assert!(stale(&dir).is_empty() && read_record(&dir).is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_file_that_is_current_is_never_stale_and_a_recorded_set_is_remembered() {
        let dir = temp_dir("record");
        for f in models::files_for(ModelSet::Light) {
            let path = dir.join(f.path.replace('/', std::path::MAIN_SEPARATOR_STR));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, vec![0u8; f.bytes as usize]).unwrap();
        }
        record_set(ModelSet::Light, &dir);
        assert_eq!(read_record(&dir).len(), models::files_for(ModelSet::Light).count());
        assert!(stale(&dir).is_empty(), "what the manifest names is current");
        assert_eq!(clean_stale(&dir), None);
        assert!(is_complete(ModelSet::Light, &dir), "nothing current was deleted");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_recorded_path_that_leaves_the_models_folder_is_not_deleted() {
        let dir = temp_dir("escape");
        let outside = dir.parent().unwrap().join(format!("oasis-outside-{}.txt", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(&outside, b"keep me").unwrap();
        let rel = format!("../{}", outside.file_name().unwrap().to_string_lossy());
        write_record(&dir, &[Installed { path: rel, sha256: "3".repeat(64), unzip: false }]);
        assert_eq!(clean_stale(&dir), None);
        assert!(outside.exists());
        let _ = fs::remove_file(outside);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn unfinished_downloads_of_dropped_files_go_and_current_ones_stay_for_resuming() {
        let dir = temp_dir("partials");
        let current = models::FILES[0].asset;
        fs::create_dir_all(dir.join(".downloads")).unwrap();
        fs::write(dir.join(".downloads").join(format!("{current}.part")), b"half").unwrap();
        fs::write(dir.join(".downloads").join("retired.onnx.part"), b"half").unwrap();
        clear_partials(&dir);
        assert!(dir.join(".downloads").join(format!("{current}.part")).exists());
        assert!(!dir.join(".downloads").join("retired.onnx.part").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_file_and_a_pause_are_reported() {
        let (_, fa) = entry("gone.onnx", 10, None);
        let (base, _) = serve(vec![], Mode::Ranges);
        let dir = temp_dir("missing");
        assert_eq!(run(&[fa], &dir, &base).0, Err(DownloadError::Http(404, "gone.onnx")));
        let paused = download_files(&[fa], &dir, Some(&base), REQUEST_WINDOW, &AtomicBool::new(true), &mut |_| {});
        assert_eq!(paused, Err(DownloadError::Paused));
        let _ = fs::remove_dir_all(dir);
    }
}

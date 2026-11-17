//! Times the downloaded models on this PC (`docs/VOICE_MODEL_SETUP.md` §2.3); the verdict is
//! `oasis_core::voice::self_check`. Two measurements:
//!
//! - **Speech:** the speech model decodes a bundled 10-second test sentence (synthetic speech from
//!   a Piper voice, so no microphone is involved): once to warm up, then three times; the middle
//!   result counts.
//! - **Polish:** S1-mini is started on the graphics card through the downloaded llama.cpp
//!   (Vulkan) runtime, answers one request to warm up, then polishes a test sentence, timed.
//!   The server is stopped again afterwards.

use std::io::Read;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use oasis_core::voice::self_check::Timings;
use oasis_models::{AudioClip, SttEngine};
use oasis_voice::{detect_model, dll_search_folders, find_dll_folder, SherpaTransducer};

/// Ten seconds of speech, 16 kHz mono 16-bit; `assets/selfcheck.txt` has the words.
const TEST_CLIP: &[u8] = include_bytes!("../assets/selfcheck-16k.wav");
/// How long the polish server may take to load the model before the check gives up.
const SERVER_START: Duration = Duration::from_secs(90);
const POLISH_REQUEST: Duration = Duration::from_secs(30);

// The control line and system prompt S1-mini was benched with (scripts/bench/polish/bench_polish.py).
// Step 9 owns the real polish prompt; these only have to make the model do a normal job once.
const S1_SYSTEM: &str = "You are a text normalizer for speech-to-text transcripts. The input begins with a control line specifying the styling, structure, and context settings; clean the transcript to match those settings and output only the cleaned text.";
const S1_CONTROL: &str = "[Styling: semi-formal] [Structure: prose] [Context: general]";
const WARM_UP_TEXT: &str = "so um we should uh meet on tuesday";
const TEST_TEXT: &str = "so um i tested the new board this morning and the the battery lasted almost twelve hours with the radio on";

/// Where the speech engine's DLLs are: `OASIS_SHERPA_DIR`, beside the program, or a `native`
/// folder beside it.
pub fn sherpa_dll_folder() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
    let chosen = std::env::var_os("OASIS_SHERPA_DIR").filter(|v| !v.is_empty()).map(PathBuf::from);
    find_dll_folder(&dll_search_folders(exe_dir.as_deref(), chosen.as_deref()))
}

/// The test clip's samples as 16 kHz mono floats.
fn test_clip() -> AudioClip {
    let data = TEST_CLIP.windows(4).position(|w| w == b"data").map_or(44, |i| i + 8);
    let samples = TEST_CLIP[data..].chunks_exact(2).map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0).collect();
    AudioClip { samples, sample_rate: 16_000 }
}

/// Time the speech model in `model_folder`: (seconds of speech, median seconds to decode them).
pub fn time_speech(model_folder: &Path) -> Result<(f64, f64), String> {
    let layout = detect_model(model_folder, false).ok_or("The speech model folder is incomplete.")?;
    let dlls = sherpa_dll_folder().ok_or("The speech engine files were not found next to OASIS.")?;
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get().min(4) as i32);
    let mut engine = SherpaTransducer::load_model(&dlls, &layout, threads, "cpu").map_err(|e| e.to_string())?;
    let clip = test_clip();
    let audio_s = clip.samples.len() as f64 / 16_000.0;
    let decode = |engine: &mut SherpaTransducer| -> Result<f64, String> {
        let started = Instant::now();
        let text = engine.transcribe(&clip).map_err(|e| e.to_string())?;
        if text.trim().is_empty() {
            return Err("The speech model heard nothing in the test sentence.".into());
        }
        Ok(started.elapsed().as_secs_f64())
    };
    decode(&mut engine)?; // warm-up
    let mut runs = [decode(&mut engine)?, decode(&mut engine)?, decode(&mut engine)?];
    runs.sort_by(f64::total_cmp);
    Ok((audio_s, runs[1]))
}

/// Stops the polish server however the check ends.
struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Start S1-mini on the graphics card and time one polish of the test sentence, in milliseconds.
pub fn time_polish(runtime_folder: &Path, gguf: &Path) -> Result<u64, String> {
    let exe = runtime_folder.join("llama-server.exe");
    if !exe.is_file() || !gguf.is_file() {
        return Err("the polish files are not downloaded".into());
    }
    // A free port: ask the system for one, then let go of it for the server.
    let port = TcpListener::bind("127.0.0.1:0").and_then(|l| l.local_addr()).map_err(|e| e.to_string())?.port();
    let mut command = Command::new(&exe);
    command
        .args(["-m"])
        .arg(gguf)
        .args(["--host", "127.0.0.1", "--port", &port.to_string(), "-c", "4096", "-ngl", "99", "--jinja", "-np", "1", "--no-webui", "--cache-ram", "0"])
        .current_dir(runtime_folder)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // no console window
    }
    let mut server = Server(command.spawn().map_err(|e| format!("could not start the polish server: {e}"))?);
    #[cfg(windows)]
    oasis_win::job::end_with_this_process(server.0.id());

    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(POLISH_REQUEST)).http_status_as_error(false).build().into();
    let base = format!("http://127.0.0.1:{port}");
    let started = Instant::now();
    loop {
        if let Ok(Some(status)) = server.0.try_wait() {
            return Err(format!("the polish server stopped at start ({status})"));
        }
        if agent.get(&format!("{base}/health")).call().is_ok_and(|r| r.status().as_u16() == 200) {
            break;
        }
        if started.elapsed() > SERVER_START {
            return Err("the polish server did not start in time".into());
        }
        std::thread::sleep(Duration::from_millis(400));
    }

    let ask = |text: &str| -> Result<(u64, String), String> {
        let body = serde_json::json!({
            "messages": [
                { "role": "system", "content": S1_SYSTEM },
                { "role": "user", "content": format!("{S1_CONTROL}\n{text}") },
            ],
            "temperature": 0,
            "cache_prompt": true,
            "chat_template_kwargs": { "enable_thinking": false },
        });
        let started = Instant::now();
        let mut response = agent
            .post(&format!("{base}/v1/chat/completions"))
            .content_type("application/json")
            .send(body.to_string().as_bytes())
            .map_err(|e| e.to_string())?;
        let mut raw = String::new();
        response.body_mut().as_reader().read_to_string(&mut raw).map_err(|e| e.to_string())?;
        let ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let reply: serde_json::Value = serde_json::from_str(&raw).map_err(|_| "the polish server answered with something unreadable".to_string())?;
        let text = reply.pointer("/choices/0/message/content").and_then(|c| c.as_str()).unwrap_or("").trim().to_string();
        Ok((ms, text))
    };
    ask(WARM_UP_TEXT)?; // loads the graphics kernels and caches the instructions, as real use does
    let (ms, text) = ask(TEST_TEXT)?;
    if text.is_empty() {
        return Err("S1-mini answered with nothing".into());
    }
    Ok(ms)
}

/// Both measurements for the set in `models_dir`: speech always, polish when `polish`.
pub fn measure(models_dir: &Path, speech_folder: &str, polish: bool) -> Result<Timings, String> {
    let (audio_s, decode_s) = time_speech(&models_dir.join("stt").join(speech_folder))?;
    let mut timings = Timings { audio_s, decode_s, polish_ms: None, polish_error: None };
    if polish {
        match time_polish(&models_dir.join("runtime").join("llama-vulkan"), &models_dir.join("llm").join("s1-mini").join("s1-mini-q4_k_m.gguf")) {
            Ok(ms) => timings.polish_ms = Some(ms),
            Err(why) => timings.polish_error = Some(why),
        }
    }
    Ok(timings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_clip_is_about_ten_seconds_of_sound() {
        let clip = test_clip();
        let seconds = clip.samples.len() as f64 / 16_000.0;
        assert!((10.0..11.5).contains(&seconds), "{seconds}");
        assert!(clip.samples.iter().any(|s| s.abs() > 0.1), "it is not silence");
    }

    #[test]
    fn missing_files_are_reported_not_started() {
        let nowhere = std::env::temp_dir().join("oasis-no-such-folder");
        assert!(time_polish(&nowhere, &nowhere.join("x.gguf")).unwrap_err().contains("not downloaded"));
        assert!(time_speech(&nowhere).is_err());
    }

    /// The Light set's single-file model loads and decodes: `OASIS_LIGHT_FOLDER` names its folder.
    #[test]
    #[ignore = "needs the downloaded Light model"]
    fn the_light_model_loads_and_decodes() {
        let folder = std::path::PathBuf::from(std::env::var("OASIS_LIGHT_FOLDER").unwrap());
        let (audio_s, decode_s) = time_speech(&folder).unwrap();
        println!("light: {audio_s:.1} s of speech in {decode_s:.2} s");
    }

    /// Times this PC's real models: `cargo test -p oasis-engine self_check -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs the downloaded models"]
    fn this_pcs_models() {
        let models = std::path::PathBuf::from(std::env::var("LOCALAPPDATA").unwrap()).join("ProjectOASIS").join("Models");
        let t = measure(&models, "parakeet-tdt-0.6b-v2", true).unwrap();
        println!("{t:?}");
        let outcome = oasis_core::voice::self_check::decide(oasis_core::voice::placement::ModelSet::Quality, &t);
        println!("{outcome:?}");
        assert!(t.decode_s > 0.0);
    }
}

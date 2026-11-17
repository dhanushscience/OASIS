//! `oasis` — headless harness for the Rust port.
//!
//! ```text
//! oasis replay [--youtube] [--voice] [--ai] [--user-pad] [FILE]   replay timed pad lines
//! oasis paths                                                     where OASIS keeps files here
//! oasis status                                                    what works on this OS today
//! oasis do <Type> [Value]                                         run one button action, as a pad press would
//! oasis address [PROCESS]                                         read a browser's address bar and show the profile it picks
//! oasis run [--dry-run] [SECONDS]                                pad → profiles → actions, for real (Windows)
//! oasis ble-scan [SECONDS]                                        list pads in range (changes nothing)
//! oasis ble-listen [SECONDS]                                      connect to the pad and print what it sends
//! ```
//!
//! `replay` reads lines of `<ms> <wire message>` (from FILE or stdin), e.g.
//!
//! ```text
//! 0    BTN:1:D
//! 400  BTN:1:U
//! 1200 BAT:80:C
//! ```
//!
//! and runs them through the same code the app will use: `LineDecoder` → `PadLink` →
//! `ArbiterHost` (with its own timers). Timers fire at their exact deadlines between lines.
//! Every effect is printed with its time, so you can check what a gesture does without a pad.

mod act;
mod address;
mod ble;
mod run;

use std::io::{self, BufRead, Read};
use std::process::ExitCode;

use oasis_app::{ArbiterHost, PadLink};
use oasis_core::input::{ArbiterConfig, ArbiterContext, ArbiterEffect};
use oasis_core::Instant;
use oasis_platform::paths::OasisPaths;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("replay") => replay(&args[1..]),
        Some("paths") => paths(),
        Some("status") => status(),
        Some("pc-check") => pc_check(),
        Some("download-models") => download_models(&args[1..]),
        Some("do") => act::run(&args[1..]),
        Some("address") => address::run(&args[1..]),
        Some("run") => run::run(&args[1..]),
        Some("ble-scan") => ble::scan(&args[1..]),
        Some("ble-listen") => ble::listen(&args[1..]),
        _ => {
            eprintln!("usage: oasis <replay|paths|status|pc-check|download-models|do|address|run|ble-scan|ble-listen> …  (see `oasis replay --help`)");
            ExitCode::from(2)
        }
    }
}

fn paths() -> ExitCode {
    let Some(p) = OasisPaths::detect() else {
        eprintln!("No home folder found on this machine.");
        return ExitCode::FAILURE;
    };
    println!("profiles   {}", p.profiles().display());
    println!("crash log  {}", p.crash_log().display());
    println!("secrets    {}", p.secrets_folder().display());
    println!("models     {}", p.models_folder().display());
    println!("  stt      {}", p.stt_folder().display());
    println!("  llm      {}", p.llm_folder().display());
    println!("  tts      {}", p.tts_folder().display());
    ExitCode::SUCCESS
}

/// What this PC has, and which voice model set it would get (`docs/VOICE_MODEL_SETUP.md` §2.2).
#[cfg(windows)]
fn pc_check() -> ExitCode {
    use oasis_core::voice::placement::{place, Placement};
    let Some(p) = OasisPaths::detect() else {
        eprintln!("No home folder found on this machine.");
        return ExitCode::FAILURE;
    };
    let f = oasis_platform::windows::pc_check::pc_facts(&p.models_folder());
    let gb = |b: u64| b as f64 / f64::from(1u32 << 30);
    println!("processor  {} ({} cores, {} threads, AVX2 {})", f.processor, f.cores, f.threads, if f.avx2 { "yes" } else { "no" });
    println!("memory     {:.1} GB", gb(f.memory_bytes));
    println!("free disk  {:.1} GB ({})", gb(f.free_disk_bytes), p.models_folder().display());
    for g in &f.gpus {
        println!("graphics   {} ({:.1} GB of its own)", g.name, gb(g.memory_bytes));
    }
    match place(&f) {
        Placement::Set { set, wanted, reason } if set == wanted => println!("
set        {}
           {reason}", set.name()),
        Placement::Set { set, wanted, reason } => println!("
set        {} (suits {})
           {reason}", set.name(), wanted.name()),
        Placement::BelowFloor { reason } => println!("
set        none
           {reason}"),
        Placement::NoDisk { reason, .. } => println!("
set        none (disk)
           {reason}"),
    }
    ExitCode::SUCCESS
}

#[cfg(not(windows))]
fn pc_check() -> ExitCode {
    eprintln!("The PC check is for Windows.");
    ExitCode::FAILURE
}

/// Download a voice model set from the models release into a folder (the real Models folder is
/// the app's job): `oasis download-models <light|balanced|quality> <folder>`.
fn download_models(args: &[String]) -> ExitCode {
    use oasis_core::voice::placement::ModelSet;
    let set = match args.first().map(String::as_str) {
        Some("light") => ModelSet::Light,
        Some("balanced") => ModelSet::Balanced,
        Some("quality") => ModelSet::Quality,
        _ => {
            eprintln!("usage: oasis download-models <light|balanced|quality> <folder>");
            return ExitCode::from(2);
        }
    };
    let Some(folder) = args.get(1) else {
        eprintln!("usage: oasis download-models <light|balanced|quality> <folder>");
        return ExitCode::from(2);
    };
    let started = std::time::Instant::now();
    let mut shown = 0;
    let result = oasis_engine::model_download::download_set(set, std::path::Path::new(folder), None, &std::sync::atomic::AtomicBool::new(false), &mut |p| {
        let percent = p.done_bytes * 100 / p.total_bytes.max(1);
        if percent >= shown + 10 || p.done_bytes == p.total_bytes {
            shown = percent;
            println!("{percent:>3}%  {:>5} of {} MB  {}", p.done_bytes / 1_000_000, p.total_bytes / 1_000_000, p.asset);
        }
    });
    match result {
        Ok(()) => {
            println!("{} is in place in {folder} ({:.0} s).", set.name(), started.elapsed().as_secs_f64());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn status() -> ExitCode {
    println!("OS: {}", oasis_platform::Os::current().name());
    for (cap, state) in oasis_platform::backend_status() {
        println!("{cap:?}: {state:?}");
    }
    ExitCode::SUCCESS
}

fn replay(args: &[String]) -> ExitCode {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("oasis replay [--youtube] [--voice] [--ai] [--user-pad] [FILE]");
        println!("  lines: <ms> <wire message>, e.g. `0 BTN:1:D`; `#` starts a comment");
        return ExitCode::SUCCESS;
    }
    let mut config = if args.iter().any(|a| a == "--user-pad") {
        ArbiterConfig::user_pad()
    } else {
        ArbiterConfig::product_defaults()
    };
    if args.iter().any(|a| a == "--voice") {
        config.voice_enabled = true;
    }
    if args.iter().any(|a| a == "--ai") {
        config.ai_enabled = true;
    }
    let context = ArbiterContext::new(args.iter().any(|a| a == "--youtube"));

    let input = match args.iter().find(|a| !a.starts_with("--")) {
        Some(path) => match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{path}: {e}");
                return ExitCode::FAILURE;
            }
        },
        None => {
            let mut s = String::new();
            if let Err(e) = io::stdin().lock().read_to_string(&mut s) {
                eprintln!("stdin: {e}");
                return ExitCode::FAILURE;
            }
            s
        }
    };

    match run_replay(&input, config, context) {
        Ok(lines) => {
            for l in lines {
                println!("{l}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// The replay engine, separate from I/O so it is testable.
pub fn run_replay(
    input: &str,
    config: ArbiterConfig,
    context: ArbiterContext,
) -> Result<Vec<String>, String> {
    // PowerShell prepends a byte-order mark to piped text; it is not part of the first number.
    let input = input.trim_start_matches('\u{feff}');
    let mut host = ArbiterHost::new(config, true);
    host.set_context(context);
    let mut link = PadLink::new();
    let mut out = Vec::new();

    let fmt =
        |at: Instant, e: &ArbiterEffect| format!("{:>6} ms  {}", at.0 / 1_000_000, describe(e));

    for (n, raw) in io::Cursor::new(input).lines().enumerate() {
        let raw = raw.map_err(|e| e.to_string())?;
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let (ms, wire) = line
            .split_once(char::is_whitespace)
            .ok_or_else(|| format!("line {}: expected `<ms> <message>`", n + 1))?;
        let ms: i64 = ms
            .parse()
            .map_err(|_| format!("line {}: `{ms}` is not a time in ms", n + 1))?;
        let now = Instant::from_millis(ms);

        // Fire every timer that came due before this line, at its own deadline.
        while let Some(deadline) = host.timers().next_deadline() {
            if !now.is_at_or_after(deadline) {
                break;
            }
            host.fire_due(deadline, &mut |e| out.push(fmt(deadline, e)));
        }

        let (events, telemetry_changed) = link.on_notification(wire.trim().as_bytes(), now);
        for evt in events {
            host.apply(evt, now, None, None, &mut |e| out.push(fmt(now, e)));
        }
        if telemetry_changed {
            let t = link.telemetry();
            out.push(format!(
                "{:>6} ms  telemetry battery={} charging={} full={} latency={}",
                ms,
                t.battery_percent.map_or("?".into(), |p| format!("{p}%")),
                t.is_charging,
                t.is_full,
                t.link_latency_ms.map_or("?".into(), |l| format!("{l} ms")),
            ));
        }
    }

    // Let pending timers run out.
    while let Some(deadline) = host.timers().next_deadline() {
        host.fire_due(deadline, &mut |e| out.push(fmt(deadline, e)));
    }
    Ok(out)
}

fn describe(e: &ArbiterEffect) -> String {
    match e {
        ArbiterEffect::DispatchCommand { button, claim } => {
            format!("dispatch BTN {} as {claim}", button.0 + 1)
        }
        ArbiterEffect::StartVoiceCapture => "voice: start capture".into(),
        ArbiterEffect::EndVoiceCapture => "voice: end capture".into(),
        ArbiterEffect::CancelVoiceCapture => "voice: cancel capture".into(),
        ArbiterEffect::ShowPeek(d) => format!("overlay: show labels for {} ms", d.as_millis()),
        ArbiterEffect::SendToDevice(cmd) => format!("extension: {cmd}"),
        ArbiterEffect::SetTimer { name, delay } => format!("timer {name} {} ms", delay.as_millis()),
        ArbiterEffect::CancelTimer(name) => format!("cancel {name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_youtube_hold_engages_and_restores_2x() {
        let out = run_replay(
            "0 BTN:1:D\n400 BTN:1:U\n",
            ArbiterConfig::product_defaults(),
            ArbiterContext::youtube(),
        )
        .unwrap();
        assert!(
            out.iter()
                .any(|l| l.contains("250 ms  extension: playbackRate.hold2x")),
            "{out:#?}"
        );
        assert!(
            out.iter()
                .any(|l| l.contains("400 ms  extension: playbackRate.restore")),
            "{out:#?}"
        );
    }

    #[test]
    fn a_desktop_hold_on_btn3_shows_the_desktop() {
        let out = run_replay(
            "0 BTN:2:D\n1000 BTN:2:U\n",
            ArbiterConfig::product_defaults(),
            ArbiterContext::desktop(),
        )
        .unwrap();
        assert!(
            out.iter()
                .any(|l| l.contains("800 ms  dispatch BTN 3 as show-desktop")),
            "{out:#?}"
        );
    }

    #[test]
    fn telemetry_and_bad_lines() {
        let out = run_replay(
            "10 BAT:55\n",
            ArbiterConfig::product_defaults(),
            ArbiterContext::desktop(),
        )
        .unwrap();
        assert!(out[0].contains("battery=55%"), "{out:?}");
        assert!(run_replay(
            "x BTN:0:D",
            ArbiterConfig::product_defaults(),
            ArbiterContext::desktop()
        )
        .is_err());
    }
}

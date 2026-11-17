//! Voice model setup (`docs/VOICE_MODEL_SETUP.md` §2 to §5): check the PC, pick a model set (or
//! take the user's choice), download what is missing in the background, and point voice at it so
//! the engine loads it in place, without a restart.
//!
//! One thread does the work. The window sees it through the `models-status` event and the
//! `models_*` commands (Settings → Voice models, the top strip, the first-start pop-up).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use oasis_core::voice::models;
use oasis_core::voice::placement::{place, ModelSet, Placement, PcFacts, DISK_SPARE_BYTES};
use oasis_core::voice::self_check::decide;
use oasis_engine::model_download::{self, DownloadError};
use oasis_win::network::{connection, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

/// How long to wait before trying again when offline, on a metered line, or after a network error.
const RETRY_AFTER: Duration = Duration::from_secs(180);
/// How long to wait for the engine to start before going on without it: the PC check and the
/// download do not need it, only switching voice to the new model does.
const ENGINE_WAIT: Duration = Duration::from_secs(8);
/// The first test waits this long after the app starts: the engine is loading the speech model
/// for voice meanwhile, and the two would compete for the processor.
const SETTLE: Duration = Duration::from_secs(14);
/// A result that would move the PC to a lighter set is checked once more after this long.
const RECHECK_AFTER: Duration = Duration::from_secs(8);
/// The most often progress is sent to the window.
const EMIT_EVERY: Duration = Duration::from_millis(250);
const SETS: [ModelSet; 3] = [ModelSet::Light, ModelSet::Balanced, ModelSet::Quality];

/// What the window shows. Sent as `models-status` whenever it changes.
#[derive(Clone, Serialize)]
pub struct ModelsInfo {
    /// "auto", "light", "balanced", "quality", or "none" (removed; download nothing).
    choice: String,
    /// The set the PC check picked, and why.
    recommended: Option<String>,
    reason: String,
    /// What the PC has, one line each.
    pc: Vec<String>,
    /// The set being downloaded or in use.
    target: Option<String>,
    sets: Vec<SetOption>,
    /// "checking", "downloading", "paused", "metered", "offline", "failed", "testing", "ready", "none".
    phase: String,
    /// What the test of the models on this PC found, in plain words.
    check: Option<String>,
    /// The test found this PC slow even with the set in use.
    check_slow: bool,
    /// S1-mini polish works on this PC (the Quality set passed its test).
    polish: bool,
    done_mb: u64,
    total_mb: u64,
    eta_s: Option<u64>,
    file: String,
    error: Option<String>,
    /// Voice works now with a model on this PC. The first-run guide's one signal that it can start its voice part.
    voice_ready: bool,
    /// This PC is under the minimum for any local model (not the same as a full disk).
    below_floor: bool,
    /// Basic is in use and this PC can run Balanced: Settings offers "Better accuracy".
    upgrade: bool,
}

/// For the first-run guide: (the PC is below the minimum, voice works with a local model).
pub fn guide_facts(app: &AppHandle) -> (bool, bool) {
    let state = app.state::<ModelsState>();
    let info = state.info.lock().unwrap_or_else(|p| p.into_inner());
    (info.below_floor, info.voice_ready)
}

#[derive(Clone, Serialize)]
struct SetOption {
    id: &'static str,
    label: &'static str,
    size_mb: u64,
    installed: bool,
    /// The PC meets what the set needs.
    suits: bool,
    what: &'static str,
}

impl Default for ModelsInfo {
    fn default() -> Self {
        ModelsInfo {
            choice: "auto".into(),
            recommended: None,
            reason: String::new(),
            pc: Vec::new(),
            target: None,
            sets: Vec::new(),
            phase: "checking".into(),
            check: None,
            check_slow: false,
            polish: false,
            done_mb: 0,
            total_mb: 0,
            eta_s: None,
            file: String::new(),
            error: None,
            voice_ready: false,
            below_floor: false,
            upgrade: false,
        }
    }
}

enum Command {
    Choose(String),
    Go,
    DownloadAnyway,
    CheckAgain,
    Remove,
}

pub struct ModelsState {
    info: Mutex<ModelsInfo>,
    pause: Arc<AtomicBool>,
    commands: Mutex<Option<Sender<Command>>>,
}

impl Default for ModelsState {
    fn default() -> Self {
        ModelsState { info: Mutex::new(ModelsInfo::default()), pause: Arc::new(AtomicBool::new(false)), commands: Mutex::new(None) }
    }
}

fn change(app: &AppHandle, f: impl FnOnce(&mut ModelsInfo)) {
    let state = app.state::<ModelsState>();
    let mut info = state.info.lock().unwrap_or_else(|p| p.into_inner());
    f(&mut info);
    let out = info.clone();
    drop(info);
    let _ = app.emit("models-status", &out);
    // every change passes here, so this is the one place the guide hears of them
    crate::onboarding::on_models(app, out.voice_ready, &out.phase);
}

fn send(app: &AppHandle, command: Command) {
    let state = app.state::<ModelsState>();
    let commands = state.commands.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(tx) = commands.as_ref() {
        let _ = tx.send(command);
    }
}

/// Starts the setup thread; call once from setup.
pub fn start(app: &AppHandle) {
    let Some(paths) = oasis_platform::paths::OasisPaths::detect() else { return };
    let (tx, rx) = mpsc::channel();
    *app.state::<ModelsState>().commands.lock().unwrap_or_else(|p| p.into_inner()) = Some(tx);
    let (app, dir) = (app.clone(), paths.models_folder());
    std::thread::Builder::new().name("oasis-models".into()).spawn(move || run(&app, &dir, &rx)).ok();
}

fn set_id(set: ModelSet) -> &'static str {
    match set {
        ModelSet::Light => "light",
        ModelSet::Balanced => "balanced",
        ModelSet::Quality => "quality",
    }
}

fn set_of(id: &str) -> Option<ModelSet> {
    SETS.into_iter().find(|s| set_id(*s) == id)
}

fn mb(bytes: u64) -> u64 {
    bytes.div_ceil(1_000_000)
}

/// The user's choice, from `Voice.ModelSet` in the settings (Automatic when unset).
fn read_choice(app: &AppHandle) -> Option<String> {
    crate::with_engine(&app.state::<crate::EngineSlot>(), |engine| {
        Ok(engine.settings().extra.get("Voice").and_then(|v| v.get("ModelSet")).and_then(Value::as_str).unwrap_or("auto").to_string())
    })
    .ok()
}

/// Write `Voice.<name>` = `value` when it differs. False while the engine is not running.
fn write_voice(app: &AppHandle, name: &str, value: &str) -> bool {
    crate::with_engine(&app.state::<crate::EngineSlot>(), |engine| {
        let current = engine.settings().extra.get("Voice").and_then(|v| v.get(name)).and_then(Value::as_str).map(str::to_string);
        if current.as_deref() == Some(value) {
            return Ok(());
        }
        engine
            .change(|s| {
                let voice = s.extra.entry("Voice").or_insert_with(|| json!({}));
                if let Some(map) = voice.as_object_mut() {
                    map.insert(name.into(), json!(value));
                }
                Ok(())
            })
            .map_err(|e| e.to_string())
    })
    .is_ok()
}

fn pc_lines(f: &PcFacts) -> Vec<String> {
    let gb = |b: u64| b as f64 / f64::from(1u32 << 30);
    let mut lines = vec![
        format!("{} ({} cores, {} threads{})", f.processor, f.cores, f.threads, if f.avx2 { "" } else { ", no AVX2" }),
        format!("{:.1} GB of memory", gb(f.memory_bytes)),
    ];
    if let Some(g) = f.gpus.iter().max_by_key(|g| g.memory_bytes) {
        lines.push(format!("{} ({:.1} GB of its own)", g.name, gb(g.memory_bytes)));
    }
    lines.push(format!("{:.1} GB free on the disk", gb(f.free_disk_bytes)));
    lines
}

fn describe(dir: &Path, facts: &PcFacts, placement: &Placement, info: &mut ModelsInfo) {
    let (recommended, reason, best) = match placement {
        Placement::Set { set, wanted, reason } => (Some(*set), reason.clone(), Some(*wanted)),
        Placement::NoDisk { wanted, reason } => (None, reason.clone(), Some(*wanted)),
        Placement::BelowFloor { reason } => (None, reason.clone(), None),
    };
    info.recommended = recommended.map(|s| set_id(s).to_string());
    info.reason = reason;
    info.pc = pc_lines(facts);
    info.sets = SETS
        .into_iter()
        .map(|set| SetOption {
            id: set_id(set),
            label: match set {
                ModelSet::Light => "Basic",
                ModelSet::Balanced => "Balanced",
                ModelSet::Quality => "Quality",
            },
            size_mb: mb(set.download_bytes()),
            installed: model_download::is_complete(set, dir),
            suits: best.is_some_and(|b| set <= b),
            what: match set {
                // 4.6% against 0.9% of words wrong (VOICE_BENCH.md §3)
                ModelSet::Light => "Fastest, less accurate. A small speech model that gets about five times as many words wrong as Balanced. For PCs too slow for Balanced.",
                ModelSet::Balanced => "The accurate speech model. Needs 4 cores and 6 GB of memory.",
                ModelSet::Quality => "The accurate speech model, plus S1-mini by Superwhisper to polish text on the graphics card. Needs a card with 4 GB of its own.",
            },
        })
        .collect();
}

/// What the test of one set found, kept in `Models\selfcheck.json` so it is not repeated at every
/// start. Valid for the models release it was made with.
#[derive(Clone, Serialize, Deserialize)]
struct CheckRecord {
    set: String,
    release: String,
    /// The set this PC should use (lower than `set` when the PC was too slow for it).
    effective: String,
    polish: bool,
    summary: String,
    slow: bool,
}

fn checks_path(dir: &Path) -> PathBuf {
    dir.join("selfcheck.json")
}

fn load_checks(dir: &Path) -> Vec<CheckRecord> {
    std::fs::read_to_string(checks_path(dir)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn saved_check(dir: &Path, set: ModelSet) -> Option<CheckRecord> {
    load_checks(dir).into_iter().find(|r| r.set == set_id(set) && r.release == models::RELEASE)
}

fn save_check(dir: &Path, record: &CheckRecord) {
    let mut all: Vec<CheckRecord> = load_checks(dir).into_iter().filter(|r| r.set != record.set).collect();
    all.push(record.clone());
    if let Ok(text) = serde_json::to_string_pretty(&all) {
        let _ = std::fs::write(checks_path(dir), text);
    }
}

/// Time the set's models on this PC and judge them (VOICE_MODEL_SETUP.md section 2.3).
fn run_check(dir: &Path, set: ModelSet, since_start: Duration) -> Result<CheckRecord, String> {
    std::thread::sleep(SETTLE.saturating_sub(since_start));
    let measure = || oasis_engine::self_check::measure(dir, models::speech_folder(set), set == ModelSet::Quality);
    let mut timings = measure()?;
    let mut outcome = decide(set, &timings);
    // A busy moment only ever makes a measurement worse, so a verdict that lowers the set is
    // confirmed by a second test, and the better of the two counts.
    if outcome.set < set || (set == ModelSet::Quality && !outcome.polish) {
        std::thread::sleep(RECHECK_AFTER);
        timings = oasis_core::voice::self_check::better(&timings, &measure()?);
        outcome = decide(set, &timings);
    }
    let record = CheckRecord {
        set: set_id(set).into(),
        release: models::RELEASE.into(),
        effective: set_id(outcome.set).into(),
        polish: outcome.polish,
        summary: outcome.summary,
        slow: outcome.slow,
    };
    save_check(dir, &record);
    // Polish failing leaves BALANCED, whose speech model was just timed: no second test needed.
    if outcome.set == ModelSet::Balanced && set == ModelSet::Quality {
        save_check(dir, &CheckRecord { set: set_id(ModelSet::Balanced).into(), effective: set_id(ModelSet::Balanced).into(), polish: false, ..record.clone() });
    }
    Ok(record)
}

/// Delete the manifest's files that `keep` does not use (after moving down to a lighter set).
fn prune_to(dir: &Path, keep: ModelSet) {
    let needed: Vec<&str> = models::files_for(keep).map(|f| f.path).collect();
    for file in models::FILES.iter().filter(|f| !needed.contains(&f.path)) {
        let path = dir.join(file.path.replace('/', std::path::MAIN_SEPARATOR_STR));
        let _ = if file.unzip { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
    }
}

/// Whether the small set is fetched first: automatic mode, BALANCED as the target (a QUALITY PC waits
/// for the full download instead, for the better accuracy from the first word), no complete set
/// on the PC yet, and room on the disk for both sets plus the spare gigabyte (`need` is the missing
/// bytes of both). On a PC that already has a model nothing changes, so no install downloads twice.
fn starter(auto: bool, target: Option<ModelSet>, any_complete: bool, free: u64, need: u64) -> bool {
    auto && target == Some(ModelSet::Balanced) && !any_complete && free >= need + DISK_SPARE_BYTES
}

/// Whether Automatic stays on BASIC although the PC check now suits a bigger set: BASIC is complete and
/// was tested here (so it was in use), and none of the bigger set is on disk. A set already in use is
/// never changed under the user; Settings offers the upgrade instead. `auto` is false while this start's
/// own starter runs, whose second download must go on.
fn keeps_basic(auto: bool, target: Option<ModelSet>, basic_in_use: bool, bigger_started: bool) -> bool {
    auto && target.is_some_and(|t| t > ModelSet::Light) && basic_in_use && !bigger_started
}

/// "Better accuracy" is offered while BASIC is in use, the PC check says BALANCED fits (cores, memory,
/// disk), and the self-check has not found BALANCED too slow here.
fn offers_upgrade(in_use: Option<ModelSet>, fits: Option<ModelSet>, too_slow: bool) -> bool {
    in_use == Some(ModelSet::Light) && fits.is_some_and(|f| f >= ModelSet::Balanced) && !too_slow
}

/// The self-check of a bigger set found this PC too slow for it (this models release).
fn tested_too_slow(dir: &Path) -> bool {
    load_checks(dir).iter().any(|r| r.release == models::RELEASE && r.set != set_id(ModelSet::Light) && r.effective == set_id(ModelSet::Light))
}

/// Delete the files `keep` does not use, trying again for a few seconds while the engine lets go of the
/// model it had open.
fn prune_when_free(dir: &Path, keep: ModelSet) {
    let needed: Vec<&str> = models::files_for(keep).map(|f| f.path).collect();
    for _ in 0..8 {
        prune_to(dir, keep);
        let left = models::FILES.iter().any(|f| !needed.contains(&f.path) && dir.join(f.path.replace('/', std::path::MAIN_SEPARATOR_STR)).exists());
        if !left {
            return;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn run(app: &AppHandle, dir: &Path, rx: &Receiver<Command>) {
    let pause = Arc::clone(&app.state::<ModelsState>().pause);
    let mut checked: Option<(PcFacts, Placement)> = None;
    let mut anyway = false;
    // Old files are cleared once per start, before any download (docs/VOICE_MODEL_SETUP.md section 4.5).
    let mut cleaned = false;
    // Set when the test found this PC too slow for the automatic set: Automatic then stops here.
    let mut cap: Option<ModelSet> = None;
    // What the self-check said when it set `cap`, shown with the lighter set's own result.
    let mut capped_why: Option<String> = None;
    // The starter: on a fresh PC the small set is fetched first so voice works within minutes, then the
    // PC's own set follows and takes over while voice is idle. `starting` is on from the first decision to
    // the switch; `starter_done` once the small set is in use and the real target is being fetched.
    let mut starting = false;
    let mut starter_done = false;
    let started = Instant::now();
    loop {
        // The engine starts alongside this thread. Its settings say what the user chose; if it
        // does not start, Automatic is assumed and the download goes ahead without it.
        let Some(choice) = read_choice(app).or_else(|| (started.elapsed() > ENGINE_WAIT).then(|| "auto".to_string())) else {
            std::thread::sleep(Duration::from_secs(1));
            continue;
        };
        if !cleaned {
            cleaned = true;
            clean_old_models(app, dir);
        }
        if checked.is_none() {
            change(app, |i| i.phase = "checking".into());
            let facts = oasis_platform::windows::pc_check::pc_facts(dir);
            let mut placement = place(&facts);
            // A test switch, also in release builds: treat this PC as too weak for a local model, so the
            // Groq screen of the first-run guide can be seen on a normal PC. It only makes OASIS offer Groq.
            if std::env::var_os("OASIS_TEST_BELOW_FLOOR").is_some_and(|v| v == "1") {
                placement = Placement::BelowFloor { reason: "Test switch: this PC is treated as below the minimum for local models.".into() };
            }
            checked = Some((facts, placement));
        }
        let (facts, placement) = checked.as_ref().expect("checked above");
        let auto = match placement {
            Placement::Set { set, .. } => Some(*set),
            _ => None,
        };
        let wanted = if choice == "auto" { auto } else { set_of(&choice) };
        if let Some(w) = wanted.filter(|_| choice == "auto" && cap.is_none()) {
            // A verdict saved at an earlier start still holds, so the bigger set is not fetched again only to be dropped.
            if let Some(r) = saved_check(dir, w).filter(|r| set_of(&r.effective).is_some_and(|e| e < w)) {
                cap = set_of(&r.effective);
                capped_why = Some(r.summary);
            } else if keeps_basic(!starting, wanted, model_download::is_complete(ModelSet::Light, dir) && saved_check(dir, ModelSet::Light).is_some(), already(w, dir) > 0) {
                cap = Some(ModelSet::Light);
            }
        }
        let target = match (choice == "auto", wanted, cap) {
            (true, Some(w), Some(c)) => Some(w.min(c)),
            _ => wanted,
        };
        // Decide the starter once, and drop it when the target is no longer a bigger set (a manual
        // choice, or the test found this PC too slow for it).
        if !target.is_some_and(|t| t > ModelSet::Light) {
            starting = false;
            starter_done = false;
        } else if !starting && choice == "auto" {
            let any_complete = SETS.into_iter().any(|s| model_download::is_complete(s, dir));
            if !any_complete {
                let need = [ModelSet::Light, target.unwrap_or(ModelSet::Light)].into_iter().map(|s| s.download_bytes().saturating_sub(already(s, dir))).sum::<u64>();
                let free = oasis_platform::windows::pc_check::pc_facts(dir).free_disk_bytes;
                if starter(true, target, any_complete, free, need) {
                    starting = true;
                    starter_done = false;
                }
            }
        }
        let light_first = starting && !starter_done;
        let target_now = if light_first { Some(ModelSet::Light) } else { target };
        change(app, |i| {
            describe(dir, facts, placement, i);
            i.below_floor = matches!(placement, Placement::BelowFloor { .. });
            i.choice.clone_from(&choice);
            i.target = target_now.map(|t| set_id(t).to_string());
            i.error = None;
            i.upgrade = false;
        });

        let next = match target_now {
            None => {
                model_download::set_progress(None);
                change(app, |i| {
                    i.phase = "none".into();
                    i.voice_ready = false;
                });
                wait(rx, None)
            }
            Some(set) if model_download::is_complete(set, dir) => {
                model_download::set_progress(None);
                // Time the models on this PC once per set and release (section 2.3).
                let record = match saved_check(dir, set) {
                    Some(r) => Ok(r),
                    None => {
                        change(app, |i| i.phase = "testing".into());
                        run_check(dir, set, started.elapsed())
                    }
                };
                let note = match &record {
                    Ok(r) => {
                        let effective = set_of(&r.effective).unwrap_or(set);
                        if choice == "auto" && effective < set {
                            cap = Some(effective);
                            capped_why = Some(r.summary.clone());
                            continue;
                        }
                        if choice == "auto" && cap.is_some() && effective == set && set == ModelSet::Light {
                            prune_to(dir, ModelSet::Light);
                        }
                        // A set the user chose stays, whatever the test found.
                        if effective < set {
                            format!("{} You chose {}, so OASIS keeps it.", r.summary, set.name())
                        } else {
                            // say why a bigger set was dropped, not only how the lighter one did
                            match &capped_why {
                                Some(why) if *why != r.summary && choice == "auto" => format!("{why} {}", r.summary),
                                _ => r.summary.clone(),
                            }
                        }
                    }
                    Err(why) => format!("The models could not be tested on this PC ({why}). They are used as they are."),
                };
                let (slow, polish) = record.as_ref().map_or((false, false), |r| (r.slow, r.polish && r.effective == set_id(set)));
                let upgrade = offers_upgrade(Some(set), auto, tested_too_slow(dir));
                // Remember what is in place, so a later release knows what to clear.
                model_download::record_set(set, dir);
                // The PC's own set is ready while the small one is in use: switch only when nothing is
                // being decoded, so one dictation is never handled by two models.
                let switching = starting && starter_done;
                if switching {
                    while oasis_engine::activity::busy() {
                        std::thread::sleep(Duration::from_secs(2));
                    }
                }
                // Point voice at the set's speech model; the engine loads it in place.
                // The engine starts or stops the polish server to match (`oasis_engine::polish`).
                write_voice(app, "PolishModel", if polish { "s1-mini" } else { "" });
                if write_voice(app, "ModelFileName", models::speech_folder(set)) {
                    change(app, |i| {
                        // only here: in the branch below the engine is not running and voice is not switched on
                        i.voice_ready = true;
                        i.phase = "ready".into();
                        i.check = Some(note);
                        i.check_slow = slow;
                        i.polish = polish;
                        i.upgrade = upgrade;
                    });
                    if switching {
                        // the small set's files are not needed any more
                        prune_when_free(dir, set);
                        starting = false;
                        starter_done = false;
                        wait(rx, None)
                    } else if light_first {
                        // the small set is in use: go on at once with the PC's own set
                        starter_done = true;
                        None
                    } else {
                        wait(rx, None)
                    }
                } else {
                    // The engine is not running, so voice cannot be switched yet; try again soon.
                    change(app, |i| {
                        i.phase = "ready".into();
                        i.check = Some(note);
                        i.check_slow = slow;
                        i.polish = polish;
                        i.upgrade = upgrade;
                        i.error = Some("OASIS could not start its engine, so voice is not switched on yet. The models are downloaded.".into());
                    });
                    wait(rx, Some(Duration::from_secs(5)))
                }
            }
            Some(set) => download(app, dir, set, &pause, &mut anyway, rx),
        };
        match next {
            Some(Command::Choose(c)) => {
                anyway = false;
                cap = None;
                capped_why = None;
                starting = false;
                starter_done = false;
                write_voice(app, "ModelSet", &c);
            }
            Some(Command::DownloadAnyway) => anyway = true,
            Some(Command::CheckAgain) => {
                checked = None;
                cap = None;
                capped_why = None;
                starting = false;
                starter_done = false;
                let _ = std::fs::remove_file(checks_path(dir));
            }
            Some(Command::Remove) => {
                cap = None;
                capped_why = None;
                starting = false;
                starter_done = false;
                // The engine keeps the speech model open; pointing voice at a folder with nothing in
                // it makes it let go (it loads in place, so it also unloads in place).
                write_voice(app, "PolishModel", "");
                write_voice(app, "ModelFileName", EMPTY_MODEL_FOLDER);
                write_voice(app, "ModelSet", "none");
                let left = remove_when_free(dir);
                let _ = std::fs::remove_file(checks_path(dir));
                checked = None;
                if let Some(why) = left {
                    change(app, |i| i.error = Some(why));
                    // Something still holds a file: the rest goes when OASIS starts again.
                    crate::restart::request(app, "Finish removing the voice models");
                }
            }
            Some(Command::Go) | None => {}
        }
    }
}

/// Download `set`, waiting while offline or on a metered line. Returns the command that ended
/// the wait, if any (`None` means: look again).
fn download(app: &AppHandle, dir: &Path, set: ModelSet, pause: &AtomicBool, anyway: &mut bool, rx: &Receiver<Command>) -> Option<Command> {
    let missing = set.download_bytes().saturating_sub(already(set, dir));
    let free = oasis_platform::windows::pc_check::pc_facts(dir).free_disk_bytes;
    if free < missing + DISK_SPARE_BYTES {
        change(app, |i| {
            i.phase = "failed".into();
            i.error = Some(format!("Not enough free disk for {}: it needs {} MB plus 1 GB to spare. Free up some space, or choose a lighter set.", set.name(), mb(missing)));
        });
        return wait(rx, Some(RETRY_AFTER));
    }
    match connection() {
        Connection::Offline => {
            change(app, |i| i.phase = "offline".into());
            return wait(rx, Some(RETRY_AFTER));
        }
        Connection::Metered if !*anyway => {
            change(app, |i| i.phase = "metered".into());
            return wait(rx, Some(RETRY_AFTER));
        }
        _ => {}
    }

    pause.store(false, Ordering::SeqCst);
    let started = Instant::now();
    let mut first: Option<u64> = None;
    let mut last_emit = Instant::now() - EMIT_EVERY;
    change(app, |i| {
        i.phase = "downloading".into();
        i.error = None;
        i.check = None;
        i.check_slow = false;
        i.polish = false;
    });
    let result = model_download::download_set(set, dir, None, pause, &mut |p| {
        let start = *first.get_or_insert(p.done_bytes);
        let percent = u8::try_from(p.done_bytes * 100 / p.total_bytes.max(1)).unwrap_or(100);
        model_download::set_progress(Some(percent));
        if last_emit.elapsed() < EMIT_EVERY && p.done_bytes < p.total_bytes {
            return;
        }
        last_emit = Instant::now();
        let secs = started.elapsed().as_secs_f64();
        let rate = (p.done_bytes - start) as f64 / secs.max(0.001);
        let eta = (secs > 3.0 && rate > 0.0).then(|| ((p.total_bytes - p.done_bytes) as f64 / rate) as u64);
        change(app, |i| {
            i.done_mb = p.done_bytes / 1_000_000;
            i.total_mb = mb(p.total_bytes);
            i.eta_s = eta;
            i.file = p.asset.to_string();
        });
    });
    model_download::set_progress(None);
    match result {
        Ok(()) => {
            change(app, |i| i.sets.iter_mut().filter(|s| s.id == set_id(set)).for_each(|s| s.installed = true));
            None
        }
        Err(DownloadError::Paused) => {
            change(app, |i| i.phase = "paused".into());
            wait(rx, None)
        }
        Err(e @ DownloadError::Network(_)) => {
            change(app, |i| {
                i.phase = "offline".into();
                i.error = Some(e.to_string());
            });
            wait(rx, Some(RETRY_AFTER))
        }
        Err(e) => {
            change(app, |i| {
                i.phase = "failed".into();
                i.error = Some(e.to_string());
            });
            wait(rx, None)
        }
    }
}

/// When a newer app names other model files than the ones on this PC, clear the old ones first so
/// the disk is not holding both and nothing old can be loaded by mistake. The speech engine and the
/// polish server hold their files open, so voice is switched off while they go; the normal flow then
/// downloads the new files and points voice at them. Models the user put there are not touched.
fn clean_old_models(app: &AppHandle, dir: &Path) {
    model_download::clear_partials(dir);
    if model_download::stale(dir).is_empty() {
        return;
    }
    change(app, |i| i.phase = "cleaning".into());
    write_voice(app, "PolishModel", "");
    write_voice(app, "ModelFileName", EMPTY_MODEL_FOLDER);
    let mut left = model_download::clean_stale(dir);
    for _ in 0..8 {
        if left.is_none() {
            return;
        }
        std::thread::sleep(Duration::from_secs(1));
        left = model_download::clean_stale(dir);
    }
    if let Some(why) = left {
        change(app, |i| i.error = Some(why));
        crate::restart::request(app, "Finish clearing the old voice models");
    }
}

/// A speech model folder name with nothing in it: voice then has no model, and lets go of the old one.
const EMPTY_MODEL_FOLDER: &str = "no-model";

/// Remove the models, trying again for a few seconds while the engine lets go of the one it had open.
fn remove_when_free(dir: &Path) -> Option<String> {
    let mut left = remove(dir);
    for _ in 0..8 {
        if left.is_none() {
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
        left = remove(dir);
    }
    left
}

/// Bytes of `set` already in place.
fn already(set: ModelSet, dir: &Path) -> u64 {
    models::files_for(set).filter(|f| dir.join(f.path).exists()).map(|f| f.bytes).sum()
}

/// Wait for a command, or until `timeout` passes (`None`: until a command comes).
fn wait(rx: &Receiver<Command>, timeout: Option<Duration>) -> Option<Command> {
    match timeout {
        Some(t) => rx.recv_timeout(t).ok(),
        None => rx.recv().ok(),
    }
}

/// Delete every file the manifest put in place, and unfinished downloads. Only what the manifest
/// names: other models the user put in the Models folder stay. Returns why something stayed.
fn remove(dir: &Path) -> Option<String> {
    let mut stuck = Vec::new();
    let _ = std::fs::remove_file(dir.join("installed.json"));
    for file in models::FILES {
        let path: PathBuf = dir.join(file.path.replace('/', std::path::MAIN_SEPARATOR_STR));
        let gone = if file.unzip { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
        if let Err(e) = gone.filter_not_found() {
            stuck.push(format!("{} ({e})", file.path));
        }
    }
    let _ = std::fs::remove_dir_all(dir.join(".downloads"));
    (!stuck.is_empty()).then(|| format!("Some files are still in use and stayed: {}.", stuck.join(", ")))
}

trait NotFoundIsFine {
    fn filter_not_found(self) -> std::io::Result<()>;
}

impl NotFoundIsFine for std::io::Result<()> {
    fn filter_not_found(self) -> std::io::Result<()> {
        match self {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

// ---- commands -------------------------------------------------------------------------------

#[tauri::command]
pub fn models_status(app: AppHandle) -> ModelsInfo {
    app.state::<ModelsState>().info.lock().map(|i| i.clone()).unwrap_or_default()
}

/// "auto", "light", "balanced" or "quality". A download under way stops and the new set starts.
#[tauri::command]
pub fn models_choose(app: AppHandle, choice: String) -> Result<(), String> {
    if choice != "auto" && set_of(&choice).is_none() {
        return Err(format!("There is no model set called {choice}."));
    }
    app.state::<ModelsState>().pause.store(true, Ordering::SeqCst);
    send(&app, Command::Choose(choice));
    Ok(())
}

#[tauri::command]
pub fn models_pause(app: AppHandle) {
    app.state::<ModelsState>().pause.store(true, Ordering::SeqCst);
}

/// Resume after a pause, or try again after a failure or while waiting for a connection.
#[tauri::command]
pub fn models_resume(app: AppHandle) {
    send(&app, Command::Go);
}

/// Download on a metered connection after all.
#[tauri::command]
pub fn models_download_anyway(app: AppHandle) {
    send(&app, Command::DownloadAnyway);
}

#[tauri::command]
pub fn models_check_again(app: AppHandle) {
    app.state::<ModelsState>().pause.store(true, Ordering::SeqCst);
    send(&app, Command::CheckAgain);
}

/// Delete the downloaded models; nothing is downloaded again until a set is chosen.
#[tauri::command]
pub fn models_remove(app: AppHandle) {
    app.state::<ModelsState>().pause.store(true, Ordering::SeqCst);
    send(&app, Command::Remove);
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_000_000_000;

    #[test]
    fn the_starter_runs_only_when_all_five_conditions_hold() {
        let big = Some(ModelSet::Balanced);
        assert!(starter(true, big, false, 20 * GB, 2 * GB), "the normal case");
        assert!(!starter(true, Some(ModelSet::Quality), false, 20 * GB, 2 * GB), "a QUALITY PC downloads QUALITY straight away");
        assert!(!starter(false, big, false, 20 * GB, 2 * GB), "a manual choice is downloaded as chosen");
        assert!(!starter(true, Some(ModelSet::Light), false, 20 * GB, 2 * GB), "the small set is already the target");
        assert!(!starter(true, None, false, 20 * GB, 2 * GB), "no target");
        assert!(!starter(true, big, true, 20 * GB, 2 * GB), "a PC that already has a model downloads nothing twice");
        assert!(!starter(true, big, false, 2 * GB, 2 * GB), "no room for both plus the spare gigabyte");
        assert!(starter(true, big, false, 2 * GB + DISK_SPARE_BYTES, 2 * GB), "exactly enough");
    }

    #[test]
    fn an_install_already_on_basic_keeps_it() {
        let big = Some(ModelSet::Balanced);
        assert!(keeps_basic(true, big, true, false), "placed BASIC by the old 7.5 GB bar: stays, Settings offers the upgrade");
        assert!(!keeps_basic(false, big, true, false), "a manual choice is handled as chosen");
        assert!(!keeps_basic(true, Some(ModelSet::Light), true, false), "nothing bigger to keep it from");
        assert!(!keeps_basic(true, big, false, false), "a fresh PC gets its set");
        assert!(!keeps_basic(true, big, true, true), "the starter's second download, cut off by a restart, goes on");
    }

    #[test]
    fn the_upgrade_is_offered_only_when_this_pc_can_run_balanced() {
        let fits = Some(ModelSet::Balanced);
        assert!(offers_upgrade(Some(ModelSet::Light), fits, false));
        assert!(offers_upgrade(Some(ModelSet::Light), Some(ModelSet::Quality), false));
        assert!(!offers_upgrade(Some(ModelSet::Light), fits, true), "the test found BALANCED too slow here");
        assert!(!offers_upgrade(Some(ModelSet::Light), Some(ModelSet::Light), false), "too few cores, too little memory or disk");
        assert!(!offers_upgrade(Some(ModelSet::Light), None, false), "below the floor");
        assert!(!offers_upgrade(Some(ModelSet::Balanced), fits, false), "already the better model");
        assert!(!offers_upgrade(None, fits, false), "nothing in use");
    }

    #[test]
    fn a_saved_verdict_of_too_slow_counts_for_later_starts() {
        let dir = std::env::temp_dir().join(format!("oasis-models-slow-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!tested_too_slow(&dir));
        let record = |set: &str, effective: &str| CheckRecord { set: set.into(), release: models::RELEASE.into(), effective: effective.into(), polish: false, summary: String::new(), slow: false };
        save_check(&dir, &record("light", "light"));
        assert!(!tested_too_slow(&dir), "BASIC's own test says nothing about BALANCED");
        save_check(&dir, &record("balanced", "light"));
        assert!(tested_too_slow(&dir));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A models folder with every manifest file (empty) and one model of the user's own.
    fn folder_with_everything(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oasis-models-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for file in models::FILES {
            let path = dir.join(file.path.replace('/', std::path::MAIN_SEPARATOR_STR));
            // The runtime is a folder (unpacked from a zip); the others are files.
            let target = if file.unzip { path.join("llama-server.exe") } else { path };
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(&target, b"x").unwrap();
        }
        std::fs::create_dir_all(dir.join("stt").join("my-own-model")).unwrap();
        std::fs::write(dir.join("stt").join("my-own-model").join("model.onnx"), b"mine").unwrap();
        dir
    }

    #[test]
    fn removing_deletes_what_the_manifest_put_there_and_nothing_else() {
        let dir = folder_with_everything("remove");
        assert_eq!(remove(&dir), None);
        assert!(!dir.join("stt").join("parakeet-tdt-0.6b-v2").join("encoder.int8.onnx").exists());
        assert!(!dir.join("runtime").join("llama-vulkan").exists());
        assert!(dir.join("stt").join("my-own-model").join("model.onnx").exists(), "the user's own model stays");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn moving_down_to_light_deletes_only_the_heavier_models() {
        let dir = folder_with_everything("prune");
        prune_to(&dir, ModelSet::Light);
        assert!(dir.join("stt").join("parakeet-tdt-ctc-110m").join("model.int8.onnx").exists());
        assert!(!dir.join("stt").join("parakeet-tdt-0.6b-v2").join("encoder.int8.onnx").exists());
        assert!(!dir.join("llm").join("s1-mini").join("s1-mini-q4_k_m.gguf").exists());
        assert!(dir.join("stt").join("my-own-model").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_file_in_use_is_reported_not_forced() {
        let dir = folder_with_everything("inuse");
        let path = dir.join("stt").join("parakeet-tdt-ctc-110m").join("tokens.txt");
        // Held the way a model loader holds it: readable by others, not deletable.
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        std::os::windows::fs::OpenOptionsExt::share_mode(&mut options, 1);
        let held = options.open(&path).unwrap();
        let left = remove(&dir);
        if cfg!(windows) {
            assert!(left.is_some_and(|why| why.contains("tokens.txt")));
        }
        drop(held);
        assert_eq!(remove(&dir), None, "once it is let go, it goes");
        let _ = std::fs::remove_dir_all(dir);
    }
}

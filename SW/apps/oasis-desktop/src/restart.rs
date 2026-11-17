//! A restart that has to wait for a quiet moment (`docs/VOICE_MODEL_SETUP.md` section 5). The app
//! asks for one with [`request`]; the window decides when it is safe (no unsaved change, no dialog
//! open, nothing typed or clicked for a minute, see `web/src/restart.svelte.js`) and the engine
//! says whether it is busy. Nothing here restarts by itself.

use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};

#[derive(Default)]
pub struct RestartState(Mutex<Option<String>>);

/// Ask for a restart; `reason` is the sentence the window shows ("Finish removing the voice models").
pub fn request(app: &AppHandle, reason: &str) {
    *app.state::<RestartState>().0.lock().unwrap_or_else(|p| p.into_inner()) = Some(reason.to_string());
    let _ = app.emit("restart-needed", reason);
}

/// The reason a restart is waiting for, if one is.
#[tauri::command]
pub fn restart_pending(app: AppHandle) -> Option<String> {
    app.state::<RestartState>().0.lock().ok().and_then(|r| r.clone())
}

/// A recording, transcription, pad action or Quick Launch run is in progress.
#[tauri::command]
pub fn engine_busy() -> bool {
    oasis_engine::activity::busy()
}

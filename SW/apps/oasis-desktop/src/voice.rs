//! Voice: the kept dictations (Insights), the Groq key, and the HUD's "save this word?" answer.

use super::*;

/// The HUD's answer to a word fixed by hand: save it to the dictionary, or not.
#[tauri::command]
pub fn learn_answer(
    app: AppHandle,
    slot: State<'_, EngineSlot>,
    save: bool,
    words: Vec<(String, String)>,
) -> Result<(), String> {
    hud::end_learn(&app);
    if !save {
        return Ok(());
    }
    with_engine(&slot, |engine| {
        engine
            .change(|s| {
                words.iter().try_for_each(|(heard, written)| {
                    oasis_settings::fields::add_glossary_word(s, heard, written)
                })
            })
            .map_err(|e| e.to_string())
    })
}

/// Look for newly plugged-in microphones twice a second and offer each one (HUD and pad buttons).
/// shortcut: polling, not Windows device notifications; swap if a scan ever shows up in the CPU use.
pub fn watch_microphones(app: &AppHandle) {
    let app = app.clone();
    let _ = std::thread::Builder::new().name("oasis-mic-watch".into()).spawn(move || {
        let mut seen = oasis_voice::device_names();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            let now = oasis_voice::device_names();
            if let Some(engine) = app.state::<EngineSlot>().handle.lock().ok().as_ref().and_then(|h| h.as_ref()) {
                let saved = oasis_settings::voice::VoiceSettings::from(&engine.settings()).microphone.to_lowercase();
                // a mic the user already chose coming back needs no question
                if let Some(name) = now.iter().find(|n| !seen.contains(n) && (saved.is_empty() || !n.to_lowercase().starts_with(&saved))) {
                    engine.offer_microphone(name);
                }
            }
            seen = now;
        }
    });
}

/// The kept dictations, newest first, for the Insights screen.
#[tauri::command]
pub fn history_list(slot: State<'_, EngineSlot>) -> Result<Vec<oasis_engine::history::Entry>, String> {
    with_engine(&slot, |engine| Ok(oasis_engine::history::list(&engine.paths().dictation_history_folder())))
}

/// One kept recording as base64 WAV, for the page to play.
#[tauri::command]
pub fn history_audio(slot: State<'_, EngineSlot>, id: String) -> Result<String, String> {
    use base64::Engine as _;
    with_engine(&slot, |engine| {
        let wav = oasis_engine::history::audio(&engine.paths().dictation_history_folder(), &id)?;
        Ok(base64::engine::general_purpose::STANDARD.encode(wav))
    })
}

/// Forget one kept dictation (recording and text).
#[tauri::command]
pub fn history_delete(slot: State<'_, EngineSlot>, id: String) -> Result<(), String> {
    with_engine(&slot, |engine| oasis_engine::history::delete(&engine.paths().dictation_history_folder(), &id))
}

/// Forget every kept dictation (recordings and texts).
#[tauri::command]
pub fn history_delete_all(slot: State<'_, EngineSlot>) -> Result<(), String> {
    with_engine(&slot, |engine| oasis_engine::history::delete_all(&engine.paths().dictation_history_folder()))
}

/// Build the training set (audio + `metadata.jsonl`) inside the history folder and show it.
/// Returns how many clips it holds.
#[tauri::command]
pub fn history_export_dataset(slot: State<'_, EngineSlot>) -> Result<usize, String> {
    with_engine(&slot, |engine| {
        let (dest, n) = oasis_engine::history::export_dataset(&engine.paths().dictation_history_folder())?;
        #[cfg(windows)]
        let _ = oasis_win::shell::open(&dest.to_string_lossy(), None);
        let _ = &dest;
        Ok(n)
    })
}

/// Show the folder of kept dictations, to share it or clear it out.
#[tauri::command]
pub fn open_history_folder(slot: State<'_, EngineSlot>) -> Result<(), String> {
    with_engine(&slot, |engine| {
        let folder = engine.paths().dictation_history_folder();
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        #[cfg(windows)]
        {
            oasis_win::shell::open(&folder.to_string_lossy(), None)
                .map_err(|code| format!("Windows could not open the folder (code {code})."))
        }
        #[cfg(not(windows))]
        {
            Err(format!("The history is in {}", folder.display()))
        }
    })
}

/// The Groq API key lives in the Windows-protected secret store, never in `profiles.json`.
#[cfg(windows)]
pub(crate) fn groq_store() -> Result<oasis_platform::windows::dpapi::DpapiSecretStore, String> {
    let paths = oasis_engine::defaults::detect_paths().ok_or("No data folder was found.")?;
    Ok(oasis_platform::windows::dpapi::DpapiSecretStore::new(
        paths.secrets_folder(),
    ))
}

/// Whether a Groq key is stored. The key itself is never sent to the window.
#[tauri::command]
pub fn groq_key_status() -> bool {
    #[cfg(windows)]
    {
        groq_store()
            .ok()
            .is_some_and(|s| oasis_engine::defaults::windows::groq_key(&s).is_some())
    }
    #[cfg(not(windows))]
    false
}

/// Store the key (a blank one removes it). The window asks the engine to reload speech afterwards.
#[tauri::command]
pub fn set_groq_key(slot: State<'_, EngineSlot>, key: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        use oasis_platform::ports::SecretStore;
        let store = groq_store()?;
        let key = key.trim();
        if key.is_empty() {
            store
                .clear(oasis_platform::secrets::GROQ_API_KEY)
                .map_err(|e| e.to_string())?;
        } else {
            store
                .set(oasis_platform::secrets::GROQ_API_KEY, key)
                .map_err(|e| e.to_string())?;
        }
        // The speech engine may have been waiting for a key: let it look again.
        with_engine(&slot, |engine| engine.reload_settings())
    }
    #[cfg(not(windows))]
    {
        let _ = (slot, key);
        Err("Groq is only wired up on Windows.".into())
    }
}

/// Try the stored key with a quarter second of silence. Returns what to tell the user.
#[tauri::command]
pub async fn test_groq_key(slot: State<'_, EngineSlot>) -> Result<String, String> {
    #[cfg(windows)]
    {
        let model = with_engine(&slot, |engine| {
            Ok(oasis_settings::voice::VoiceSettings::from(&engine.settings()).groq_model)
        })?;
        let key = oasis_engine::defaults::windows::groq_key(&groq_store()?)
            .ok_or("No Groq key is stored yet.")?;
        Ok(match oasis_engine::groq::test_key(&key, &model) {
            None => "The key works.".to_string(),
            Some(reason) => reason,
        })
    }
    #[cfg(not(windows))]
    {
        let _ = slot;
        Err("Groq is only wired up on Windows.".into())
    }
}

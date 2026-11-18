//! Connected accounts: Spotify and the meetings calendar.

use super::*;

/// Spotify's saved sign-in (shared with the C# app) and where it is set up.
#[cfg(windows)]
pub(crate) fn spotify_auth() -> Result<std::sync::Arc<oasis_engine::spotify::Auth>, String> {
    let paths = oasis_engine::defaults::detect_paths().ok_or("No data folder was found.")?;
    Ok(std::sync::Arc::clone(
        &oasis_engine::defaults::windows::spotify_hub(&paths).auth,
    ))
}

/// One line for the settings screen: set up, connected or not.
#[tauri::command]
pub async fn spotify_status() -> String {
    #[cfg(windows)]
    {
        // Really tries the sign-in (it may need the network), so it runs off the window thread.
        tauri::async_runtime::spawn_blocking(|| spotify_auth().map_or_else(|e| e, |a| a.verify()))
            .await
            .unwrap_or_else(|e| e.to_string())
    }
    #[cfg(not(windows))]
    "Spotify is only wired up on Windows.".to_string()
}

/// Sign in through the browser. Waits up to two minutes for you to finish there.
#[tauri::command]
pub async fn spotify_connect() -> Result<String, String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(|| {
            let auth = spotify_auth()?;
            auth.connect(&|url| oasis_win::shell::open(url, None).is_ok())?;
            Ok(auth.status())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    Err("Spotify is only wired up on Windows.".into())
}

#[tauri::command]
pub fn spotify_disconnect() -> Result<String, String> {
    #[cfg(windows)]
    {
        let auth = spotify_auth()?;
        auth.disconnect()?;
        Ok(auth.status())
    }
    #[cfg(not(windows))]
    Err("Spotify is only wired up on Windows.".into())
}

/// The calendar's state for the settings screen: one line, the error if the last refresh failed,
/// and the next few meetings.
#[derive(serde::Serialize)]
pub(crate) struct MeetingsReply {
    status: String,
    connected: bool,
    error: String,
    upcoming: Vec<MeetingLine>,
}

#[derive(serde::Serialize)]
pub(crate) struct MeetingLine {
    title: String,
    when: String,
    can_join: bool,
}

#[tauri::command]
pub fn meetings_status(slot: State<'_, EngineSlot>) -> Result<MeetingsReply, String> {
    with_engine(&slot, |engine| {
        let Some(meetings) = engine.meetings() else {
            return Ok(MeetingsReply {
                status: "Meetings are not available in this build.".into(),
                connected: false,
                error: String::new(),
                upcoming: vec![],
            });
        };
        #[cfg(windows)]
        let offset = oasis_win::system::utc_offset_seconds();
        #[cfg(not(windows))]
        let offset = 0;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        Ok(MeetingsReply {
            status: meetings.calendar().status(),
            connected: meetings.calendar().connected(),
            error: meetings.last_error(),
            upcoming: meetings
                .upcoming_lines(now, offset, 6)
                .into_iter()
                .map(|(title, when, can_join)| MeetingLine {
                    title,
                    when,
                    can_join,
                })
                .collect(),
        })
    })
}

/// Sign in to Google in the browser (waits up to five minutes), then load the meetings.
#[tauri::command]
pub async fn meetings_connect(slot: State<'_, EngineSlot>) -> Result<String, String> {
    let meetings = with_engine(&slot, |engine| {
        engine
            .meetings()
            .ok_or_else(|| "Meetings are not available in this build.".to_string())
    })?;
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(windows)]
        let open = |url: &str| oasis_win::shell::open(url, None).is_ok();
        #[cfg(not(windows))]
        let open = |_: &str| false;
        meetings.calendar().connect(&open)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        meetings.refresh(now);
        Ok(meetings.calendar().status())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn meetings_disconnect(slot: State<'_, EngineSlot>) -> Result<String, String> {
    with_engine(&slot, |engine| {
        let meetings = engine
            .meetings()
            .ok_or("Meetings are not available in this build.")?;
        meetings.calendar().disconnect()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        meetings.refresh(now);
        Ok(meetings.calendar().status())
    })
}

/// A button on the meeting reminder: `join`, `snooze` or `dismiss`.
#[tauri::command]
pub fn meeting_reminder_action(slot: State<'_, EngineSlot>, action: String) -> Result<(), String> {
    with_engine(&slot, |engine| engine.answer_meeting_reminder(&action))
}

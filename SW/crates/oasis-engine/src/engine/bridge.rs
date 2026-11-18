//! The browser extension (YouTube) socket and the preferences the action backend reads.

use super::*;

/// Whether `button` is one the hidden YouTube gestures use (hold-2×, volume up or down).
pub(super) fn is_youtube_gesture_button(settings: &Settings, button: i32) -> bool {
    let config = arbiter_config(settings, false);
    [config.hold2x, config.volume_up, config.volume_down].into_iter().flatten().any(|b| b.0 == button)
}

/// Settings > Preferences reach the action backend. The browser defaults to Chrome; a browser that is
/// not installed falls back to the system's default where it is used (W14).
pub(super) fn sync_preferences(settings: &Settings) {
    let browser = settings.extra.get("Preferences").and_then(|p| p.get("Browser")).and_then(|b| b.as_str()).unwrap_or("chrome");
    #[cfg(windows)]
    oasis_platform::windows::actions::set_preferred_browser(browser);
    #[cfg(not(windows))]
    let _ = browser;
}

/// Bring the browser-extension socket in line with the settings: start it, move it to another
/// port, push new settings to a running one, or stop it.
pub(super) fn sync_bridge(
    shared: &Arc<Shared>,
    config: Option<&BridgeConfig>,
    settings: &Settings,
    current: &mut Option<(BridgeServer, u16)>,
) {
    let wanted = config.filter(|_| bridge_enabled(settings));
    let port = config.and_then(|c| c.port).unwrap_or_else(|| bridge_port(settings));
    match (wanted, current.as_ref()) {
        (Some(_), Some((server, running))) if *running == port => {
            server.push_settings();
            return;
        }
        (None, None) => return,
        _ => {}
    }
    if let Some((mut server, _)) = current.take() {
        server.stop();
    }
    let Some(config) = wanted else { return };

    // Publish the extension with this user's token beside it, where the browser loads it from.
    if let Some(folder) = extension_install::ensure_published(&shared.paths, &config.bundled_extension) {
        if let Err(e) = write_token_files(&folder, &config.token) {
            shared.emit(EngineEvent::Note { text: format!("The browser extension's token file was not written: {e}") });
        }
    }

    let events = Arc::clone(shared);
    let tx = shared.inbox.clone();
    let settings_source = Arc::clone(shared);
    let started = BridgeServer::start(
        port,
        config.token.clone(),
        Some(SecurityLog::at(shared.paths.security_log())),
        move || extension_settings(&settings_source.settings()),
        move |event| match event {
            BridgeEvent::YouTube(event) => {
                let _ = tx.send(Input::YouTube(event));
            }
            BridgeEvent::Status(text) => {
                let clients = events.status.lock().map(|s| s.bridge_clients).unwrap_or(0);
                events.update(|s| s.bridge_status = text.clone());
                events.emit(EngineEvent::BridgeStatus { text, clients });
            }
            BridgeEvent::Clients(clients) => {
                let text = events.status.lock().map(|s| s.bridge_status.clone()).unwrap_or_default();
                events.update(|s| s.bridge_clients = clients);
                events.emit(EngineEvent::BridgeStatus { text, clients });
            }
            BridgeEvent::Hello => {}
        },
    );
    match started {
        Ok(server) => *current = Some((server, port)),
        Err(e) => {
            let text = format!("ERROR · {e}");
            shared.update(|s| s.bridge_status = text.clone());
            shared.emit(EngineEvent::BridgeStatus { text, clients: 0 });
        }
    }
}

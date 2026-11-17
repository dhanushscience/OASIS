//! Meetings from your calendar: the Google sign-in, the upcoming list, reminders, and joining.
//! Port of `Services/Meetings/{GoogleCalendarProvider, MeetingService, MeetingOAuthDefaults}.cs`.
//!
//! The Google account is the one the C# app connected: the same saved token, and the same client
//! id and secret. Outlook is not ported yet.
//!
//! What the calendar says is behind a small [`Calendar`] trait, so the cache, the reminders and
//! joining are tested without the network.

use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use oasis_core::actions::{try_allow, UrlScope};
use oasis_core::meetings::{self as rules, Meeting, ReminderState};
use oasis_platform::ports::SecretStore;
use serde_json::{json, Value};

use crate::spotify::{agent, base64url, days_from_civil, now_unix, percent_encode, read_error, wait_for_code};

const SCOPES: &str = "https://www.googleapis.com/auth/calendar.events https://www.googleapis.com/auth/calendar.readonly";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const CALENDAR: &str = "https://www.googleapis.com/calendar/v3/calendars/primary";
/// Reminders come back this many minutes after "snooze".
pub const SNOOZE_MINUTES: i64 = 5;

pub const SETUP_HINT: &str =
    "Google sign-in is not set up. Add your Google client id and secret (see the older OASIS app's Meeting settings), or a google_oauth.json next to profiles.json.";

// ---- the calendar --------------------------------------------------------------------------------

pub trait Calendar: Send + Sync {
    fn connected(&self) -> bool;
    /// One line for the settings screen.
    fn status(&self) -> String;
    fn upcoming(&self, from: i64, to: i64) -> Result<Vec<Meeting>, String>;
    /// Sign in through the browser. `open` shows a page in the browser.
    fn connect(&self, open: &dyn Fn(&str) -> bool) -> Result<(), String>;
    fn disconnect(&self) -> Result<(), String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoogleConfig {
    pub client_id: String,
    pub client_secret: String,
}

impl GoogleConfig {
    /// The id and secret from, in order: the settings (`Meetings.GoogleClientId` with the saved
    /// secret), the environment, then `google_oauth.json` in `dirs`.
    pub fn load(settings_client_id: &str, saved_secret: Option<String>, env: (Option<String>, Option<String>), dirs: &[PathBuf]) -> Option<GoogleConfig> {
        let clean = |s: Option<String>| s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let file = dirs.iter().find_map(|d| std::fs::read_to_string(d.join("google_oauth.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()));
        let from_file = |key: &str| clean(file.as_ref().and_then(|f| f.get(key)).and_then(Value::as_str).map(str::to_string));
        let id = clean(Some(settings_client_id.to_string())).or_else(|| clean(env.0.clone())).or_else(|| from_file("ClientId"))?;
        let secret = clean(saved_secret).or_else(|| clean(env.1)).or_else(|| from_file("ClientSecret"))?;
        Some(GoogleConfig { client_id: id, client_secret: secret })
    }
}

/// The Google sign-in saved the way the C# app saves it (`AccessToken`, `RefreshToken`, …).
pub struct Google {
    store: Arc<dyn SecretStore>,
    config: Option<GoogleConfig>,
    /// The short-lived access token and when it runs out.
    cached: Mutex<Option<(String, i64)>>,
}

fn refresh_token_of(json: &str) -> Option<String> {
    let v: Value = serde_json::from_str(json).ok()?;
    ["RefreshToken", "refresh_token"].iter().find_map(|k| v.get(k).and_then(Value::as_str)).filter(|t| !t.trim().is_empty()).map(str::to_string)
}

fn scope_of(json: &str) -> String {
    serde_json::from_str::<Value>(json)
        .ok()
        .and_then(|v| ["Scope", "scope"].iter().find_map(|k| v.get(k).and_then(Value::as_str).map(str::to_string)))
        .unwrap_or_default()
}

fn grants_calendar(scope: &str) -> bool {
    scope.split_whitespace().any(|s| {
        s.ends_with("/auth/calendar.events") || s.ends_with("/auth/calendar") || s.ends_with("/auth/calendar.readonly")
    })
}

impl Google {
    pub fn new(store: Arc<dyn SecretStore>, config: Option<GoogleConfig>) -> Google {
        Google { store, config, cached: Mutex::new(None) }
    }

    fn saved(&self) -> Option<String> {
        self.store.get(oasis_platform::secrets::GOOGLE_TOKEN).ok().flatten()
    }

    fn forget(&self) {
        let _ = self.store.clear(oasis_platform::secrets::GOOGLE_TOKEN);
        *self.cached.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    fn post_token(&self, form: &[(&str, &str)]) -> Result<Value, String> {
        let body = form.iter().map(|(k, v)| format!("{k}={}", percent_encode(v))).collect::<Vec<_>>().join("&");
        let mut response = agent()
            .post(TOKEN_URL)
            .content_type("application/x-www-form-urlencoded")
            .send(body.as_str())
            .map_err(|e| format!("Google request failed: {e}"))?;
        let status = response.status().as_u16();
        let text = response.body_mut().read_to_string().unwrap_or_default();
        let v: Value = serde_json::from_str(&text).map_err(|_| "Google's answer could not be read.".to_string())?;
        if (200..300).contains(&status) {
            return Ok(v);
        }
        let error = v.get("error").and_then(Value::as_str).unwrap_or("");
        if error == "invalid_grant" {
            self.forget();
            return Err("Google sign-in expired. Connect again in the settings.".into());
        }
        Err(v.get("error_description").and_then(Value::as_str).unwrap_or(if error.is_empty() { "Google sign-in failed." } else { error }).to_string())
    }

    /// An access token that works, refreshed when it ran out.
    fn access_token(&self) -> Result<String, String> {
        if let Some((token, expires)) = self.cached.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            if now_unix() < expires - 60 {
                return Ok(token);
            }
        }
        let config = self.config.as_ref().ok_or(SETUP_HINT)?;
        let saved = self.saved().ok_or("Google Calendar is not connected.")?;
        let refresh = refresh_token_of(&saved).ok_or("Google Calendar is not connected.")?;
        let v = self.post_token(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh),
            ("client_id", &config.client_id),
            ("client_secret", &config.client_secret),
        ])?;
        let access = v.get("access_token").and_then(Value::as_str).filter(|a| !a.is_empty()).ok_or("Google's answer had no access token.")?.to_string();
        let expires = now_unix() + v.get("expires_in").and_then(Value::as_i64).unwrap_or(3600);
        *self.cached.lock().unwrap_or_else(|e| e.into_inner()) = Some((access.clone(), expires));
        Ok(access)
    }

    fn get(&self, url: &str) -> Result<String, String> {
        let token = self.access_token()?;
        let mut response = agent().get(url).header("Authorization", format!("Bearer {token}")).call().map_err(|e| format!("Google request failed: {e}"))?;
        let status = response.status().as_u16();
        let body = response.body_mut().read_to_string().unwrap_or_default();
        if (200..300).contains(&status) {
            return Ok(body);
        }
        if matches!(status, 401 | 403) {
            // Revoked, or the permission is missing: the saved sign-in is no use any more.
            self.forget();
            return Err("Google Calendar needs you to connect again (the permission was removed or expired).".into());
        }
        Err(read_error(&body, &format!("Google returned {status}.")))
    }
}

/// `2026-11-08T15:30:00+05:30`, `…Z`, with or without fractions → seconds since 1970.
pub fn rfc3339_to_unix(text: &str) -> Option<i64> {
    let t = text.trim();
    let (date, rest) = t.split_once(['T', 't'])?;
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let split = rest.find(['Z', 'z', '+', '-']).unwrap_or(rest.len());
    let (clock, zone) = rest.split_at(split);
    let clock = clock.split('.').next()?;
    let mut hms = clock.split(':').map(|p| p.parse::<i64>().ok());
    let (h, mi, s) = (hms.next()??, hms.next()??, hms.next().flatten().unwrap_or(0));
    let offset = match zone.chars().next() {
        Some(sign @ ('+' | '-')) => {
            let (zh, zm) = zone[1..].split_once(':').unwrap_or((&zone[1..], "0"));
            let secs = zh.parse::<i64>().ok()? * 3600 + zm.parse::<i64>().ok()? * 60;
            if sign == '+' { secs } else { -secs }
        }
        _ => 0,
    };
    ((1..=12).contains(&m) && (1..=31).contains(&day)).then(|| days_from_civil(y, m, day) * 86_400 + h * 3600 + mi * 60 + s - offset)
}

/// `2026-11-08T10:00:00Z` for a moment, which is what the calendar's time limits take.
pub fn unix_to_rfc3339(unix: i64) -> String {
    let iso = crate::spotify::unix_to_iso(unix);
    format!("{}Z", iso.split('.').next().unwrap_or(&iso))
}

fn first_link(text: &str) -> Option<String> {
    let start = text.find("https://").or_else(|| text.find("http://"))?;
    let rest = &text[start..];
    let end = rest.find(|c: char| c.is_whitespace() || matches!(c, '"' | '<' | '>')).unwrap_or(rest.len());
    let link = rest[..end].trim_end_matches(['.', ',', ')']);
    (!link.is_empty()).then(|| link.to_string())
}

fn time_of(v: Option<&Value>) -> Option<(i64, bool)> {
    let v = v?;
    if let Some(dt) = v.get("dateTime").and_then(Value::as_str) {
        return rfc3339_to_unix(dt).map(|t| (t, false));
    }
    let date = v.get("date").and_then(Value::as_str)?;
    rfc3339_to_unix(&format!("{date}T00:00:00Z")).map(|t| (t, true))
}

/// One calendar event as a meeting; `None` for one with no usable time or a cancelled one.
pub fn meeting_of(event: &Value) -> Option<Meeting> {
    if event.get("status").and_then(Value::as_str) == Some("cancelled") {
        return None;
    }
    let (start, all_day) = time_of(event.get("start"))?;
    let (end, _) = time_of(event.get("end"))?;
    let text = |k: &str| event.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
    let join_url = Some(text("hangoutLink"))
        .filter(|l| !l.is_empty())
        .or_else(|| {
            event.pointer("/conferenceData/entryPoints").and_then(Value::as_array).and_then(|list| {
                list.iter().find(|e| e.get("entryPointType").and_then(Value::as_str) == Some("video")).and_then(|e| e.get("uri")).and_then(Value::as_str).map(str::to_string)
            })
        })
        .or_else(|| first_link(&text("description")))
        .or_else(|| first_link(&text("location")))
        .unwrap_or_default();
    let title = text("summary");
    Some(Meeting {
        id: text("id"),
        title: if title.is_empty() { "(No title)".into() } else { title },
        start,
        end,
        join_url,
        all_day,
        provider: "google".into(),
    })
}

impl Calendar for Google {
    fn connected(&self) -> bool {
        self.saved().is_some_and(|t| refresh_token_of(&t).is_some())
    }

    fn status(&self) -> String {
        if self.config.is_none() {
            return format!("Setup needed. {SETUP_HINT}");
        }
        match self.saved() {
            Some(t) if refresh_token_of(&t).is_some() => {
                if grants_calendar(&scope_of(&t)) || scope_of(&t).is_empty() { "Connected.".into() } else { "Connected, but reconnect to give OASIS calendar access.".into() }
            }
            _ => "Not connected.".into(),
        }
    }

    fn upcoming(&self, from: i64, to: i64) -> Result<Vec<Meeting>, String> {
        if !self.connected() {
            return Ok(Vec::new());
        }
        let url = format!(
            "{CALENDAR}/events?timeMin={}&timeMax={}&singleEvents=true&orderBy=startTime&maxResults=100",
            percent_encode(&unix_to_rfc3339(from)),
            percent_encode(&unix_to_rfc3339(to))
        );
        let body = self.get(&url)?;
        let v: Value = serde_json::from_str(&body).map_err(|_| "Google's answer could not be read.".to_string())?;
        Ok(v.get("items").and_then(Value::as_array).map(|items| items.iter().filter_map(meeting_of).collect()).unwrap_or_default())
    }

    fn connect(&self, open: &dyn Fn(&str) -> bool) -> Result<(), String> {
        let config = self.config.as_ref().ok_or(SETUP_HINT)?;
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| format!("No port was free for the sign-in ({e})."))?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let redirect = format!("http://127.0.0.1:{port}/");
        let mut state_bytes = [0u8; 16];
        getrandom::fill(&mut state_bytes).map_err(|e| format!("No random source: {e}"))?;
        let state = base64url(&state_bytes);
        let url = format!(
            "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&state={state}&access_type=offline&prompt=consent&include_granted_scopes=true",
            percent_encode(&config.client_id),
            percent_encode(&redirect),
            percent_encode(SCOPES)
        );
        if !open(&url) {
            return Err("The browser could not be opened.".into());
        }
        let code = wait_for_code(&listener, "/", &state, "Google", Duration::from_secs(300))?;
        let v = self.post_token(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", &redirect),
            ("client_id", &config.client_id),
            ("client_secret", &config.client_secret),
        ])?;
        let refresh = v.get("refresh_token").and_then(Value::as_str).filter(|r| !r.is_empty()).ok_or("Google did not give a lasting sign-in. Try again and allow every permission.")?;
        let scope = v.get("scope").and_then(Value::as_str).unwrap_or("");
        if !scope.split_whitespace().any(|s| s.ends_with("/auth/calendar.events") || s.ends_with("/auth/calendar")) {
            return Err("Google did not grant calendar access. On the consent screen, tick every permission box and try again.".into());
        }
        let expires = v.get("expires_in").and_then(Value::as_i64).unwrap_or(3600);
        let access = v.get("access_token").and_then(Value::as_str).unwrap_or("");
        // The same field names the C# app writes, so it can read this sign-in too.
        let saved = json!({
            "AccessToken": access, "TokenType": "Bearer", "ExpiresInSeconds": expires, "RefreshToken": refresh, "Scope": scope,
            "IdToken": v.get("id_token").and_then(Value::as_str), "Issued": crate::spotify::unix_to_iso(now_unix()), "IssuedUtc": crate::spotify::unix_to_iso(now_unix()), "IsStale": false
        });
        self.store.set(oasis_platform::secrets::GOOGLE_TOKEN, &saved.to_string()).map_err(|e| e.to_string())?;
        *self.cached.lock().unwrap_or_else(|e| e.into_inner()) = Some((access.to_string(), now_unix() + expires));
        Ok(())
    }

    fn disconnect(&self) -> Result<(), String> {
        self.forget();
        Ok(())
    }
}

// ---- the service ---------------------------------------------------------------------------------

/// What the reminder window lets you do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReminderAction {
    Join,
    Snooze,
    Dismiss,
}

impl ReminderAction {
    pub fn parse(text: &str) -> Option<ReminderAction> {
        match text {
            "join" => Some(ReminderAction::Join),
            "snooze" => Some(ReminderAction::Snooze),
            "dismiss" => Some(ReminderAction::Dismiss),
            _ => None,
        }
    }
}

#[derive(Default)]
struct State {
    cached: Vec<Meeting>,
    reminders: ReminderState,
    active: Option<Meeting>,
    last_error: String,
}

pub struct Meetings {
    calendar: Arc<dyn Calendar>,
    /// Opens a page in the browser (a meeting link).
    opener: Arc<dyn Fn(&str) -> bool + Send + Sync>,
    state: Mutex<State>,
    refreshing: AtomicBool,
}

impl Meetings {
    pub fn new(calendar: Arc<dyn Calendar>, opener: Arc<dyn Fn(&str) -> bool + Send + Sync>) -> Meetings {
        Meetings { calendar, opener, state: Mutex::new(State::default()), refreshing: AtomicBool::new(false) }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn calendar(&self) -> &dyn Calendar {
        self.calendar.as_ref()
    }

    /// Asks the calendar again (the day before to a week ahead). A failure keeps what is shown.
    /// Does nothing if a refresh is already running.
    pub fn refresh(&self, now: i64) {
        if self.refreshing.swap(true, Ordering::SeqCst) {
            return;
        }
        let result = if self.calendar.connected() { self.calendar.upcoming(now - 86_400, now + 7 * 86_400) } else { Ok(Vec::new()) };
        let mut state = self.state();
        match result {
            Ok(list) => {
                state.cached = rules::merge(list);
                state.last_error.clear();
                let cached = state.cached.clone();
                state.reminders.prune(&cached);
            }
            Err(e) => state.last_error = e,
        }
        drop(state);
        self.refreshing.store(false, Ordering::SeqCst);
    }

    pub fn meetings(&self) -> Vec<Meeting> {
        self.state().cached.clone()
    }

    pub fn last_error(&self) -> String {
        self.state().last_error.clone()
    }

    /// The meeting to remind about now, once. `None` while one is already being shown.
    pub fn due_reminder(&self, now: i64, enabled: bool, minutes_before: i64) -> Option<Meeting> {
        if !enabled || !self.calendar.connected() {
            return None;
        }
        let mut state = self.state();
        if state.active.is_some() {
            return None;
        }
        let cached = state.cached.clone();
        let due = state.reminders.due(&cached, now, minutes_before).cloned();
        state.active = due.clone();
        due
    }

    pub fn active_reminder(&self) -> Option<Meeting> {
        self.state().active.clone()
    }

    /// Opens a meeting's link in the browser, if the address is allowed.
    pub fn join(&self, meeting: &Meeting) -> Result<(), String> {
        let url = try_allow(&meeting.join_url, UrlScope::MeetingJoin).ok_or("This meeting has no link OASIS may open.")?;
        if (self.opener)(&url) { Ok(()) } else { Err("The meeting link could not be opened.".into()) }
    }

    /// The earliest meeting that can be joined now, opened.
    pub fn join_now(&self, now: i64, window_minutes: i64) -> Result<String, String> {
        let meeting = {
            let state = self.state();
            rules::joinable_now(&state.cached, now, window_minutes).cloned()
        };
        let meeting = meeting.ok_or("No meeting with a link is starting or running.")?;
        self.join(&meeting)?;
        Ok(format!("Joining {}", meeting.title))
    }

    /// What the reminder window's buttons do. `Err` only for a join that could not open.
    pub fn answer_reminder(&self, action: ReminderAction, now: i64) -> Result<(), String> {
        let Some(meeting) = self.state().active.take() else { return Ok(()) };
        let mut result = Ok(());
        match action {
            ReminderAction::Join => {
                result = self.join(&meeting);
                self.state().reminders.dismiss(&meeting);
            }
            ReminderAction::Snooze => self.state().reminders.snooze(&meeting, now, SNOOZE_MINUTES),
            ReminderAction::Dismiss => self.state().reminders.dismiss(&meeting),
        }
        result
    }

    /// For the settings screen: the next meetings, as `(title, when, can_join)`.
    pub fn upcoming_lines(&self, now: i64, offset_secs: i32, limit: usize) -> Vec<(String, String, bool)> {
        self.state()
            .cached
            .iter()
            .filter(|m| m.end > now)
            .take(limit)
            .map(|m| (m.title.clone(), rules::range_text(m, offset_secs), !m.join_url.trim().is_empty()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_times_with_offsets_and_dates_are_read() {
        assert_eq!(rfc3339_to_unix("2026-11-07T10:00:00Z"), Some(1_794_045_600));
        assert_eq!(rfc3339_to_unix("2026-11-07T15:30:00+05:30"), Some(1_794_045_600));
        assert_eq!(rfc3339_to_unix("2026-11-07T05:00:00.123-05:00"), Some(1_794_045_600));
        assert_eq!(rfc3339_to_unix("2026-11-07T10:00Z"), Some(1_794_045_600));
        assert_eq!(rfc3339_to_unix("junk"), None);
        assert_eq!(unix_to_rfc3339(1_794_045_600), "2026-11-07T10:00:00Z");
    }

    #[test]
    fn events_become_meetings_with_the_best_join_link() {
        let timed = json!({ "id": "a", "summary": "Standup", "start": { "dateTime": "2026-11-07T10:00:00Z" }, "end": { "dateTime": "2026-11-07T10:30:00Z" },
            "hangoutLink": "https://meet.google.com/abc", "description": "see https://zoom.us/j/1" });
        let m = meeting_of(&timed).unwrap();
        assert_eq!((m.title.as_str(), m.join_url.as_str(), m.all_day, m.end - m.start), ("Standup", "https://meet.google.com/abc", false, 1800));

        let in_description = json!({ "id": "b", "start": { "dateTime": "2026-11-07T10:00:00Z" }, "end": { "dateTime": "2026-11-07T11:00:00Z" },
            "description": "Join: https://zoom.us/j/123, pass 4" });
        let m = meeting_of(&in_description).unwrap();
        assert_eq!((m.title.as_str(), m.join_url.as_str()), ("(No title)", "https://zoom.us/j/123"));

        let conference = json!({ "id": "c", "start": { "dateTime": "2026-11-07T10:00:00Z" }, "end": { "dateTime": "2026-11-07T11:00:00Z" },
            "conferenceData": { "entryPoints": [{ "entryPointType": "phone", "uri": "tel:1" }, { "entryPointType": "video", "uri": "https://v.example/x" }] } });
        assert_eq!(meeting_of(&conference).unwrap().join_url, "https://v.example/x");

        let all_day = json!({ "id": "d", "summary": "Holiday", "start": { "date": "2026-11-07" }, "end": { "date": "2026-11-08" } });
        let m = meeting_of(&all_day).unwrap();
        assert!(m.all_day && m.end - m.start == 86_400);
        assert!(meeting_of(&json!({ "id": "e", "status": "cancelled", "start": { "date": "2026-11-07" }, "end": { "date": "2026-11-08" } })).is_none());
        assert!(meeting_of(&json!({ "id": "f" })).is_none());
    }

    #[test]
    fn the_saved_google_sign_in_is_read_in_either_naming() {
        assert_eq!(refresh_token_of(r#"{"AccessToken":"a","RefreshToken":"r"}"#).as_deref(), Some("r"));
        assert_eq!(refresh_token_of(r#"{"refresh_token":"r2"}"#).as_deref(), Some("r2"));
        assert_eq!(refresh_token_of(r#"{"RefreshToken":" "}"#), None);
        assert_eq!(refresh_token_of("junk"), None);
        assert!(grants_calendar("openid https://www.googleapis.com/auth/calendar.events"));
        assert!(!grants_calendar("openid email"));
    }

    #[test]
    fn the_client_id_and_secret_come_from_settings_the_environment_or_a_file() {
        let dir = std::env::temp_dir().join(format!("oasis-google-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("google_oauth.json"), r#"{"ClientId":"file-id","ClientSecret":"file-secret"}"#).unwrap();
        let dirs = [dir.clone()];
        let c = GoogleConfig::load(" settings-id ", Some("saved".into()), (None, None), &dirs).unwrap();
        assert_eq!((c.client_id.as_str(), c.client_secret.as_str()), ("settings-id", "saved"));
        let c = GoogleConfig::load("", None, (Some("env-id".into()), Some("env-secret".into())), &dirs).unwrap();
        assert_eq!((c.client_id.as_str(), c.client_secret.as_str()), ("env-id", "env-secret"));
        let c = GoogleConfig::load("", None, (None, None), &dirs).unwrap();
        assert_eq!((c.client_id.as_str(), c.client_secret.as_str()), ("file-id", "file-secret"));
        assert!(GoogleConfig::load("id-only", None, (None, None), &[]).is_none(), "an id with no secret is no sign-in");
    }

    // ---- the service, against a pretend calendar -------------------------------------------------

    struct Fake {
        connected: AtomicBool,
        meetings: Mutex<Vec<Meeting>>,
        fail: AtomicBool,
    }

    impl Calendar for Fake {
        fn connected(&self) -> bool {
            self.connected.load(Ordering::SeqCst)
        }
        fn status(&self) -> String {
            "fake".into()
        }
        fn upcoming(&self, _: i64, _: i64) -> Result<Vec<Meeting>, String> {
            if self.fail.load(Ordering::SeqCst) { Err("offline".into()) } else { Ok(self.meetings.lock().unwrap().clone()) }
        }
        fn connect(&self, _: &dyn Fn(&str) -> bool) -> Result<(), String> {
            Ok(())
        }
        fn disconnect(&self) -> Result<(), String> {
            self.connected.store(false, Ordering::SeqCst);
            Ok(())
        }
    }

    fn meeting(id: &str, start: i64, url: &str) -> Meeting {
        Meeting { id: id.into(), title: format!("Meeting {id}"), start, end: start + 1800, join_url: url.into(), all_day: false, provider: "google".into() }
    }

    fn service(meetings: Vec<Meeting>) -> (Meetings, Arc<Fake>, Arc<Mutex<Vec<String>>>) {
        let fake = Arc::new(Fake { connected: AtomicBool::new(true), meetings: Mutex::new(meetings), fail: AtomicBool::new(false) });
        let opened = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&opened);
        let service = Meetings::new(fake.clone(), Arc::new(move |url: &str| {
            log.lock().unwrap().push(url.to_string());
            true
        }));
        (service, fake, opened)
    }

    #[test]
    fn a_failed_refresh_keeps_the_meetings_already_shown() {
        let (s, fake, _) = service(vec![meeting("a", 10_000, "https://meet.google.com/a")]);
        s.refresh(9_000);
        assert_eq!(s.meetings().len(), 1);
        fake.fail.store(true, Ordering::SeqCst);
        s.refresh(9_100);
        assert_eq!(s.meetings().len(), 1, "kept");
        assert_eq!(s.last_error(), "offline");
        fake.fail.store(false, Ordering::SeqCst);
        s.refresh(9_200);
        assert_eq!(s.last_error(), "");
    }

    #[test]
    fn join_now_opens_the_earliest_joinable_link_and_only_allowed_ones() {
        let (s, _, opened) = service(vec![meeting("late", 20_000, "https://meet.google.com/late"), meeting("soon", 10_000, "https://meet.google.com/soon")]);
        s.refresh(9_500);
        assert_eq!(s.join_now(9_500, 15).unwrap(), "Joining Meeting soon");
        assert_eq!(*opened.lock().unwrap(), ["https://meet.google.com/soon"]);
        assert!(s.join_now(1_000, 15).is_err(), "nothing is near");

        let (bad, _, opened) = service(vec![meeting("x", 10_000, "javascript:alert(1)")]);
        bad.refresh(9_500);
        assert!(bad.join_now(9_500, 15).is_err());
        assert!(opened.lock().unwrap().is_empty(), "a blocked address is never opened");
    }

    #[test]
    fn a_reminder_is_shown_once_then_answered() {
        let (s, _, opened) = service(vec![meeting("a", 10_000, "https://meet.google.com/a")]);
        s.refresh(9_000);
        assert!(s.due_reminder(9_000, true, 5).is_none(), "too early");
        let due = s.due_reminder(9_750, true, 5).unwrap();
        assert_eq!(due.id, "a");
        assert!(s.due_reminder(9_760, true, 5).is_none(), "one at a time");
        s.answer_reminder(ReminderAction::Snooze, 9_760).unwrap();
        assert!(s.due_reminder(9_800, true, 5).is_none(), "snoozed");
        assert!(s.due_reminder(9_760 + 301, true, 5).is_some(), "back after the snooze");
        s.answer_reminder(ReminderAction::Join, 10_100).unwrap();
        assert_eq!(*opened.lock().unwrap(), ["https://meet.google.com/a"]);
        assert!(s.due_reminder(10_200, true, 5).is_none(), "joined means done");
        assert!(s.due_reminder(9_750, false, 5).is_none(), "reminders off");
    }

    #[test]
    fn a_disconnected_calendar_shows_nothing_and_reminds_of_nothing() {
        let (s, fake, _) = service(vec![meeting("a", 10_000, "https://x")]);
        s.refresh(9_000);
        fake.disconnect().unwrap();
        assert!(s.due_reminder(9_900, true, 5).is_none());
        s.refresh(9_900);
        assert!(s.meetings().is_empty());
    }

    #[test]
    fn the_settings_list_skips_finished_meetings() {
        let (s, _, _) = service(vec![meeting("old", 1_000, ""), meeting("next", 10_000, "https://x")]);
        s.refresh(5_000);
        let lines = s.upcoming_lines(5_000, 0, 5);
        assert_eq!(lines.len(), 1);
        assert_eq!((lines[0].0.as_str(), lines[0].2), ("Meeting next", true));
        assert_eq!(ReminderAction::parse("snooze"), Some(ReminderAction::Snooze));
        assert_eq!(ReminderAction::parse("nope"), None);
    }
}

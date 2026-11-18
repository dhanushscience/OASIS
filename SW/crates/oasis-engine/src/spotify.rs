//! Spotify by voice: the account connection and the playback commands. Port of
//! `Services/Spotify/*.cs` (`SpotifyAuthService`, `SpotifyApiClient`, `SpotifyPlaybackService`,
//! `SpotifyOAuthDefaults`, `SpotifyPkce`).
//!
//! The account is the one the C# app connected: the token is read from, and written back to, the
//! same protected secret, and the client id comes from the same `spotify_oauth.json`. Connecting
//! from here uses the same sign-in (PKCE, a page on `127.0.0.1`).
//!
//! The playback rules run against a small [`Api`] trait, so they are tested without the network;
//! [`UreqApi`] is the real one.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine as _;
use oasis_core::actions::ActionKind;
use oasis_core::voice::command::spotify::{self as rules, Candidate, Device, QueryKind};
use oasis_platform::ports::SecretStore;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const REQUIRED_SCOPES: &str =
    "user-modify-playback-state user-read-playback-state user-library-read playlist-read-private playlist-read-collaborative user-read-private";
const DEFAULT_PORT: u16 = 8765;
const DEFAULT_PATH: &str = "/callback";
const TOKEN_URL: &str = "https://accounts.spotify.com/api/token";
const API: &str = "https://api.spotify.com/v1";

// ---- what the engine sees ----------------------------------------------------------------------

/// Runs one Spotify action for the engine. `kind` is `SpotifyControl` or `SpotifyPlay`; the answer
/// is a sentence to show, or the reason it did not work.
pub trait Runner: Send + Sync {
    fn run(&self, kind: &str, value: &str) -> Result<String, String>;
}

// ---- the sign-in ---------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthConfig {
    pub client_id: String,
    pub port: u16,
    pub path: String,
}

impl OAuthConfig {
    pub fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}{}", self.port, self.path)
    }

    /// The first usable `spotify_oauth.json` among `dirs`, else `OASIS_SPOTIFY_CLIENT_ID`.
    pub fn load(dirs: &[PathBuf], env_client_id: Option<&str>) -> Option<OAuthConfig> {
        let file = dirs.iter().find_map(|d| std::fs::read_to_string(d.join("spotify_oauth.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()));
        let text = |v: &Option<Value>, key: &str| v.as_ref().and_then(|v| v.get(key)).and_then(Value::as_str).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let client_id = text(&file, "client_id").or_else(|| env_client_id.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()))?;
        let port = file.as_ref().and_then(|v| v.get("redirect_port")).and_then(Value::as_u64).filter(|p| (1..65536).contains(p)).map_or(DEFAULT_PORT, |p| p as u16);
        let path = text(&file, "redirect_path").map_or(DEFAULT_PATH.to_string(), |p| if p.starts_with('/') { p } else { format!("/{p}") });
        Some(OAuthConfig { client_id, port, path })
    }

    pub fn setup_hint(&self) -> String {
        format!("Register {} as a redirect address in your Spotify developer dashboard.", self.redirect_uri())
    }
}

pub const SETUP_HINT: &str = "Add spotify_oauth.json (with your Spotify developer client_id) next to profiles.json, or set OASIS_SPOTIFY_CLIENT_ID.";

/// The saved sign-in. Stored as the C# app stores it, so the two share one connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub access: String,
    pub refresh: String,
    /// Seconds since 1970.
    pub expires_at: i64,
    pub scope: String,
}

impl Token {
    pub fn expired(&self, now: i64) -> bool {
        self.expires_at <= now + 60
    }

    pub fn has_all_scopes(&self) -> bool {
        let granted: Vec<&str> = self.scope.split_whitespace().collect();
        REQUIRED_SCOPES.split_whitespace().all(|r| granted.iter().any(|g| g.eq_ignore_ascii_case(r)))
    }

    pub fn from_json(text: &str) -> Option<Token> {
        let v: Value = serde_json::from_str(text).ok()?;
        let get = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let token = Token { access: get("AccessToken"), refresh: get("RefreshToken"), expires_at: iso_to_unix(&get("ExpiresAtUtc")).unwrap_or(0), scope: get("Scope") };
        (!token.refresh.trim().is_empty()).then_some(token)
    }

    pub fn to_json(&self) -> String {
        json!({ "AccessToken": self.access, "RefreshToken": self.refresh, "ExpiresAtUtc": unix_to_iso(self.expires_at), "Scope": self.scope }).to_string()
    }
}

pub(crate) fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub(crate) fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `2026-11-08T10:20:30.1234567Z` → seconds since 1970. Only the UTC form C# writes.
pub fn iso_to_unix(text: &str) -> Option<i64> {
    let t = text.trim().trim_end_matches('Z');
    let (date, time) = t.split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let time = time.split('.').next()?;
    let mut hms = time.split(':').map(|p| p.parse::<i64>().ok());
    let (h, mi, s) = (hms.next()??, hms.next()??, hms.next()??);
    ((1..=12).contains(&m) && (1..=31).contains(&day)).then(|| days_from_civil(y, m, day) * 86_400 + h * 3600 + mi * 60 + s)
}

pub fn unix_to_iso(unix: i64) -> String {
    let (y, m, d) = civil_from_days(unix.div_euclid(86_400));
    let s = unix.rem_euclid(86_400);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.0000000Z", s / 3600, s % 3600 / 60, s % 60)
}

pub(crate) fn now_unix() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

pub(crate) fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                if let Ok(v) = u8::from_str_radix(&text[i + 1..i + 3], 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
                out.push(b'%');
            }
            b'+' => out.push(b' '),
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `code=abc&state=xyz` → the pairs, decoded.
pub fn parse_query(query: &str) -> Vec<(String, String)> {
    query
        .trim_start_matches('?')
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (percent_decode(k), percent_decode(v)))
        .collect()
}

pub(crate) fn base64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// A random verifier and its S256 challenge.
pub fn pkce_pair() -> Result<(String, String), String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| format!("No random source: {e}"))?;
    let verifier = base64url(&bytes);
    let challenge = base64url(&Sha256::digest(verifier.as_bytes()));
    Ok((verifier, challenge))
}

// ---- the network ---------------------------------------------------------------------------------

/// What Spotify's web API is asked. Errors are sentences.
pub trait Api: Send + Sync {
    fn get(&self, token: &str, url: &str) -> Result<String, String>;
    /// PUT or POST. `body` is JSON, if any.
    fn send(&self, token: &str, method: &str, url: &str, body: Option<&str>) -> Result<(), String>;
}

pub struct UreqApi;

pub(crate) fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(20))).http_status_as_error(false).build().into()
}

pub(crate) fn read_error(body: &str, fallback: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            v.pointer("/error/message").and_then(Value::as_str).or_else(|| v.get("error").and_then(Value::as_str)).map(str::to_string)
        })
        .unwrap_or_else(|| fallback.to_string())
}

/// What to tell the user for a failed Spotify call, in words that say what to do.
pub fn friendly_error(status: u16, body: &str) -> String {
    let text = body.to_lowercase();
    match status {
        401 => "Spotify rejected the sign-in. Press Connect / reconnect in the settings.".into(),
        403 if text.contains("premium") => "Spotify Premium is needed to control playback.".into(),
        403 if text.contains("scope") || text.contains("insufficient") => "Spotify needs more permission. Press Connect / reconnect in the settings.".into(),
        429 => "Spotify is limiting requests. Try again in a moment.".into(),
        _ => read_error(body, &format!("Spotify returned {status}.")),
    }
}

impl Api for UreqApi {
    fn get(&self, token: &str, url: &str) -> Result<String, String> {
        let mut response = agent().get(url).header("Authorization", format!("Bearer {token}")).call().map_err(|e| format!("Spotify request failed: {e}"))?;
        let status = response.status().as_u16();
        let body = response.body_mut().read_to_string().unwrap_or_default();
        if (200..300).contains(&status) { Ok(body) } else { Err(friendly_error(status, &body)) }
    }

    fn send(&self, token: &str, method: &str, url: &str, body: Option<&str>) -> Result<(), String> {
        let agent = agent();
        let auth = format!("Bearer {token}");
        let result = match (method, body) {
            ("PUT", Some(b)) => agent.put(url).header("Authorization", auth).content_type("application/json").send(b),
            ("PUT", None) => agent.put(url).header("Authorization", auth).send_empty(),
            (_, Some(b)) => agent.post(url).header("Authorization", auth).content_type("application/json").send(b),
            (_, None) => agent.post(url).header("Authorization", auth).send_empty(),
        };
        let mut response = result.map_err(|e| format!("Spotify request failed: {e}"))?;
        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            return Ok(());
        }
        let text = response.body_mut().read_to_string().unwrap_or_default();
        Err(friendly_error(status, &text))
    }
}

/// Gives the playback code an access token that works.
pub trait TokenSource: Send + Sync {
    fn access_token(&self) -> Result<String, String>;
}

/// The saved sign-in: refreshed when it runs out, saved again, shared with the C# app.
pub struct Auth {
    store: Arc<dyn SecretStore>,
    config: Option<OAuthConfig>,
}

impl Auth {
    pub fn new(store: Arc<dyn SecretStore>, config: Option<OAuthConfig>) -> Auth {
        Auth { store, config }
    }

    pub fn token(&self) -> Option<Token> {
        let json = self.store.get(oasis_platform::secrets::SPOTIFY_TOKEN).ok().flatten()?;
        Token::from_json(&json)
    }

    pub fn connected(&self) -> bool {
        self.token().is_some()
    }

    /// One line for the settings screen.
    pub fn status(&self) -> String {
        if self.config.is_none() {
            return format!("Setup needed. {SETUP_HINT}");
        }
        match self.token() {
            None => "Not connected.".into(),
            Some(t) if !t.has_all_scopes() => "Connected, but reconnect to give OASIS library access.".into(),
            Some(_) => "Connected.".into(),
        }
    }

    /// The line for the settings screen after really trying the sign-in: an expired or revoked one
    /// is found now, not at the next spoken command. May ask Spotify for a fresh token.
    pub fn verify(&self) -> String {
        let Some(token) = self.token() else { return self.status() };
        if self.config.is_none() || !token.has_all_scopes() {
            return self.status();
        }
        match self.access_token() {
            Ok(_) => "Connected.".into(),
            Err(reason) => reason,
        }
    }

    pub fn disconnect(&self) -> Result<(), String> {
        self.store.clear(oasis_platform::secrets::SPOTIFY_TOKEN).map_err(|e| e.to_string())
    }

    fn save(&self, token: &Token) {
        let _ = self.store.set(oasis_platform::secrets::SPOTIFY_TOKEN, &token.to_json());
    }

    fn token_request(&self, form: &[(&str, &str)], previous_refresh: &str) -> Result<Token, String> {
        let body = form.iter().map(|(k, v)| format!("{k}={}", percent_encode(v))).collect::<Vec<_>>().join("&");
        let mut response = agent()
            .post(TOKEN_URL)
            .content_type("application/x-www-form-urlencoded")
            .send(body.as_str())
            .map_err(|e| format!("Spotify request failed: {e}"))?;
        let status = response.status().as_u16();
        let text = response.body_mut().read_to_string().unwrap_or_default();
        let v: Value = serde_json::from_str(&text).map_err(|_| "Spotify's answer could not be read.".to_string())?;
        if !(200..300).contains(&status) {
            let error = v.get("error").and_then(Value::as_str).unwrap_or("");
            if error.contains("invalid") && !previous_refresh.is_empty() {
                let _ = self.disconnect();
                return Err("Spotify session expired. Connect again in the settings.".into());
            }
            return Err(v.get("error_description").and_then(Value::as_str).unwrap_or(if error.is_empty() { "Spotify sign-in failed." } else { error }).to_string());
        }
        let access = v.get("access_token").and_then(Value::as_str).filter(|a| !a.is_empty()).ok_or("Spotify's answer had no access token.")?;
        let refresh = v.get("refresh_token").and_then(Value::as_str).filter(|r| !r.is_empty()).unwrap_or(previous_refresh);
        Ok(Token {
            access: access.to_string(),
            refresh: refresh.to_string(),
            expires_at: now_unix() + v.get("expires_in").and_then(Value::as_i64).unwrap_or(3600).max(60),
            scope: v.get("scope").and_then(Value::as_str).unwrap_or("").to_string(),
        })
    }

    /// Signs in through the browser: opens the Spotify page, waits (up to two minutes) for it to
    /// come back to this PC, and saves the token. `open` shows a page in the browser.
    pub fn connect(&self, open: &dyn Fn(&str) -> bool) -> Result<(), String> {
        let config = self.config.as_ref().ok_or(SETUP_HINT)?;
        let (verifier, challenge) = pkce_pair()?;
        let mut state_bytes = [0u8; 16];
        getrandom::fill(&mut state_bytes).map_err(|e| format!("No random source: {e}"))?;
        let state: String = state_bytes.iter().map(|b| format!("{b:02X}")).collect();
        let force = !self.token().is_some_and(|t| t.has_all_scopes());
        let url = format!(
            "https://accounts.spotify.com/authorize?client_id={}&response_type=code&redirect_uri={}&scope={}&state={state}&code_challenge_method=S256&code_challenge={challenge}{}",
            percent_encode(&config.client_id),
            percent_encode(&config.redirect_uri()),
            percent_encode(REQUIRED_SCOPES),
            if force { "&show_dialog=true" } else { "" }
        );
        let listener = TcpListener::bind(("127.0.0.1", config.port)).map_err(|e| format!("Port {} is busy ({e}). Close whatever uses it and try again.", config.port))?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        if !open(&url) {
            return Err("The browser could not be opened.".into());
        }
        let code = wait_for_code(&listener, &config.path, &state, "Spotify", Duration::from_secs(120))?;
        let redirect = config.redirect_uri();
        let token = self.token_request(
            &[("grant_type", "authorization_code"), ("code", &code), ("redirect_uri", &redirect), ("client_id", &config.client_id), ("code_verifier", &verifier)],
            "",
        )?;
        self.save(&token);
        Ok(())
    }
}

/// Waits for the browser to come back to this PC with the sign-in code. `service` names who is
/// signing in on the pages the browser shows.
pub(crate) fn wait_for_code(listener: &TcpListener, path: &str, state: &str, service: &str, timeout: Duration) -> Result<String, String> {
    let deadline = Instant::now() + timeout;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                if let Some(result) = handle_callback(stream, path, state, service) {
                    return result;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() > deadline {
                    return Err(format!("{service} sign-in timed out."));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// Reads one request from the browser. `Some(Ok(code))` when it carries the right code; `Some(Err)`
/// when sign-in failed; `None` for anything else (a stray request), which is answered and ignored.
fn handle_callback(mut stream: std::net::TcpStream, path: &str, state: &str, service: &str) -> Option<Result<String, String>> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buffer = [0u8; 4096];
    let n = stream.read(&mut buffer).unwrap_or(0);
    let request = String::from_utf8_lossy(&buffer[..n]).into_owned();
    let target = request.lines().next()?.split_whitespace().nth(1)?.to_string();
    let (route, query) = target.split_once('?').unwrap_or((&target, ""));
    let mut reply = |html: &str| {
        let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}", html.len());
    };
    let page = |title: &str, line: &str| format!("<html><body style='font-family:sans-serif;text-align:center;padding:40px'><h2>{title}</h2><p>{line}</p></body></html>");
    if route != path && route != "/" {
        reply("<html><body>Not found</body></html>");
        return None;
    }
    let pairs = parse_query(query);
    let find = |k: &str| pairs.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
    if let Some(error) = find("error") {
        reply(&page(&format!("{service} connection failed"), &error.replace('<', "")));
        return Some(Err(if error == "access_denied" { format!("{service} connection cancelled.") } else { format!("{service} sign-in error: {error}") }));
    }
    let Some(code) = find("code") else {
        reply(&page(&format!("Waiting for {service}…"), "Finish signing in in the browser."));
        return None;
    };
    if find("state").as_deref() != Some(state) {
        reply(&page(&format!("{service} connection failed"), "State mismatch. Try again from OASIS."));
        return Some(Err(format!("{service} sign-in state mismatch. Try again.")));
    }
    reply(&page(&format!("{service} connected to OASIS"), "You can close this window."));
    Some(Ok(code))
}

impl TokenSource for Auth {
    fn access_token(&self) -> Result<String, String> {
        let token = self.token().ok_or("Spotify is not connected. Connect it in the settings.")?;
        if !token.expired(now_unix()) {
            return Ok(token.access);
        }
        let config = self.config.as_ref().ok_or(SETUP_HINT)?;
        let fresh = self.token_request(&[("grant_type", "refresh_token"), ("refresh_token", &token.refresh), ("client_id", &config.client_id)], &token.refresh)?;
        self.save(&fresh);
        Ok(fresh.access)
    }
}

// ---- the playback --------------------------------------------------------------------------------

/// What the playback code needs from the desktop.
pub trait Host: Send + Sync {
    /// Open `spotify:` addresses (a track, or just the app).
    fn open(&self, uri: &str);
    /// Bring the Spotify window forward.
    fn focus(&self);
}

#[derive(Debug, Clone, Copy)]
pub struct Timing {
    pub device_wait: Duration,
    pub device_poll: Duration,
    pub after_transfer: Duration,
    pub confirm_polls: u32,
    pub confirm_every: Duration,
}

impl Timing {
    pub const REAL: Timing = Timing {
        device_wait: Duration::from_secs(8),
        device_poll: Duration::from_millis(400),
        after_transfer: Duration::from_millis(400),
        confirm_polls: 6,
        confirm_every: Duration::from_millis(300),
    };
}

pub struct Playback<'a> {
    pub api: &'a dyn Api,
    pub tokens: &'a dyn TokenSource,
    pub host: &'a dyn Host,
    pub timing: Timing,
}

fn device_param(device: Option<&str>) -> String {
    device.map_or(String::new(), |d| format!("device_id={}", percent_encode(d)))
}

fn with_device(path: &str, query: &str, device: Option<&str>) -> String {
    let device = device_param(device);
    let joined = [query.to_string(), device].into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join("&");
    if joined.is_empty() { format!("{API}{path}") } else { format!("{API}{path}?{joined}") }
}

fn candidates(body: &str, pointer: &str, by: &dyn Fn(&Value) -> String) -> Vec<Candidate> {
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    v.pointer(pointer)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|i| {
                    let uri = i.get("uri")?.as_str()?.trim();
                    let name = i.get("name")?.as_str()?.trim();
                    (!uri.is_empty() && !name.is_empty()).then(|| Candidate {
                        uri: uri.to_string(),
                        name: name.to_string(),
                        by: by(i),
                        popularity: i.get("popularity").and_then(Value::as_u64).unwrap_or(0) as u32,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn artists_of(item: &Value) -> String {
    item.get("artists")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| x.get("name").and_then(Value::as_str)).filter(|n| !n.trim().is_empty()).collect::<Vec<_>>().join(", "))
        .unwrap_or_default()
}

fn owner_of(item: &Value) -> String {
    item.pointer("/owner/display_name").and_then(Value::as_str).unwrap_or("").to_string()
}

fn no_active_device(message: &str) -> bool {
    let m = message.to_lowercase();
    m.contains("no active device") || m.contains("no_active_device") || m.contains("device not found")
}

impl Playback<'_> {
    fn token(&self) -> Result<String, String> {
        self.tokens.access_token()
    }

    fn get(&self, token: &str, url: &str) -> Result<String, String> {
        self.api.get(token, url)
    }

    fn search(&self, token: &str, kind: &str, term: &str, limit: u32, pointer: &str, by: &dyn Fn(&Value) -> String) -> Result<Vec<Candidate>, String> {
        let url = format!("{API}/search?q={}&type={kind}&limit={}", percent_encode(term), limit.clamp(1, 10));
        Ok(candidates(&self.get(token, &url)?, pointer, by))
    }

    fn devices(&self, token: &str) -> Result<Vec<Device>, String> {
        let body = self.get(token, &format!("{API}/me/player/devices"))?;
        let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
        Ok(v.get("devices")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .map(|d| Device {
                        id: d.get("id").and_then(Value::as_str).unwrap_or("").to_string(),
                        name: d.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                        kind: d.get("type").and_then(Value::as_str).unwrap_or("").to_string(),
                        active: d.get("is_active").and_then(Value::as_bool).unwrap_or(false),
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    fn is_playing(&self, token: &str) -> Option<bool> {
        let body = self.get(token, &format!("{API}/me/player")).ok()?;
        serde_json::from_str::<Value>(&body).ok()?.get("is_playing").and_then(Value::as_bool)
    }

    /// The id of this PC's Spotify. A fresh launch takes seconds to appear as a device, so the app
    /// is woken once and asked again until the wait runs out.
    fn ensure_device(&self, token: &str) -> Result<String, String> {
        let deadline = Instant::now() + self.timing.device_wait;
        let mut woke = false;
        let devices = loop {
            let found = self.devices(token)?;
            if !found.is_empty() {
                break found;
            }
            if !woke {
                self.host.focus();
                self.host.open("spotify:");
                woke = true;
            }
            if Instant::now() >= deadline {
                return Err("No Spotify device found. Open the Spotify app on this PC, then try again.".into());
            }
            std::thread::sleep(self.timing.device_poll);
        };
        let computer = rules::pick_device(&devices).ok_or("No usable Spotify playback device found.")?;
        // An active phone or speaker would make the desktop window open while the track plays elsewhere.
        if !computer.active {
            let body = json!({ "device_ids": [computer.id], "play": false }).to_string();
            self.api.send(token, "PUT", &format!("{API}/me/player"), Some(&body))?;
            std::thread::sleep(self.timing.after_transfer);
        }
        Ok(computer.id.clone())
    }

    fn play_body(&self, token: &str, device: Option<&str>, body: &Value) -> Result<(), String> {
        self.api.send(token, "PUT", &with_device("/me/player/play", "", device), Some(&body.to_string()))
    }

    fn play_with_retry(&self, token: &str, device: Option<&str>, body: &Value) -> Result<(), String> {
        match self.play_body(token, device, body) {
            Err(e) if no_active_device(&e) => {
                let retry = self.ensure_device(token)?;
                self.play_body(token, Some(&retry), body)
            }
            other => other,
        }
    }

    /// Does what a control phrase says (`pause`, `next`, `volume 40`, …).
    pub fn control(&self, command: &str) -> Result<String, String> {
        let token = self.token()?;
        let device = self.ensure_device(&token)?;
        let d = Some(device.as_str());
        let cmd = command.trim().to_lowercase();
        let put = |path: &str, query: &str| self.api.send(&token, "PUT", &with_device(path, query, d), None);
        let post = |path: &str| self.api.send(&token, "POST", &with_device(path, "", d), None);
        match cmd.as_str() {
            "pause" | "stop" => put("/me/player/pause", "").map(|()| "Paused Spotify".into()),
            "resume" | "play" | "unpause" | "continue" => {
                self.api.send(&token, "PUT", &with_device("/me/player/play", "", d), Some("{}")).map(|()| "Resumed Spotify".into())
            }
            "next" | "skip" | "skip next" => post("/me/player/next").map(|()| "Skipped to next track".into()),
            "previous" | "prev" | "back" | "skip back" => post("/me/player/previous").map(|()| "Went to previous track".into()),
            "shuffle on" | "enable shuffle" | "shuffle" => put("/me/player/shuffle", "state=true").map(|()| "Shuffle on".into()),
            "shuffle off" | "disable shuffle" => put("/me/player/shuffle", "state=false").map(|()| "Shuffle off".into()),
            "repeat" | "repeat on" | "repeat context" | "repeat playlist" => put("/me/player/repeat", "state=context").map(|()| "Repeat on".into()),
            "repeat track" | "repeat song" | "repeat one" => put("/me/player/repeat", "state=track").map(|()| "Repeat track".into()),
            "repeat off" | "disable repeat" => put("/me/player/repeat", "state=off").map(|()| "Repeat off".into()),
            other => {
                if let Some(pct) = other.strip_prefix("volume ").and_then(|p| p.trim().trim_end_matches('%').parse::<u32>().ok()) {
                    let pct = pct.min(100);
                    return put("/me/player/volume", &format!("volume_percent={pct}")).map(|()| format!("Spotify volume {pct}%"));
                }
                if let Some(ms) = other.strip_prefix("seek ").and_then(rules::seek_ms) {
                    return put("/me/player/seek", &format!("position_ms={ms}")).map(|()| format!("Seeked to {}s", ms / 1000));
                }
                Err(format!("Unknown Spotify control: {command}"))
            }
        }
    }

    /// Plays (or queues) what was asked for.
    pub fn play(&self, query: &str) -> Result<String, String> {
        let query = rules::normalize_query(query.trim().strip_prefix("spotify:").unwrap_or(query.trim()));
        if query.is_empty() {
            return Err("Nothing to play.".into());
        }
        let token = self.token()?;
        let device = self.ensure_device(&token)?;
        let d = Some(device.as_str());
        if rules::is_generic_query(&query) {
            // "play some music": resume what was playing.
            self.play_body(&token, d, &json!({}))?;
            return Ok("Resumed Spotify".into());
        }
        let (kind, term) = rules::classify_query(&query);
        match kind {
            QueryKind::LikedSongs => {
                let liked = self.liked_songs(&token)?.ok_or("Could not find your Liked Songs playlist. Reconnect Spotify to give OASIS library access.")?;
                self.play_body(&token, d, &json!({ "context_uri": liked.uri }))?;
                Ok(format!("Playing {}", liked.name))
            }
            QueryKind::UserPlaylist => {
                let playlist = self.resolve_playlist(&token, &term)?.ok_or_else(|| format!("No playlist in your library matched \"{term}\"."))?;
                self.play_body(&token, d, &json!({ "context_uri": playlist.uri }))?;
                Ok(format!("Playing playlist {}", playlist.name))
            }
            QueryKind::Album => {
                let albums = self.search(&token, "album", &term, 5, "/albums/items", &artists_of)?;
                let album = rules::pick_album_or_artist(&albums, &term).ok_or_else(|| format!("No album found for \"{term}\"."))?;
                self.play_body(&token, d, &json!({ "context_uri": album.uri }))?;
                Ok(format!("Playing album {}", album.name))
            }
            QueryKind::Artist => {
                let artists = self.search(&token, "artist", &term, 5, "/artists/items", &|_| String::new())?;
                let artist = rules::pick_album_or_artist(&artists, &term).ok_or_else(|| format!("No artist found for \"{term}\"."))?;
                self.play_body(&token, d, &json!({ "context_uri": artist.uri }))?;
                Ok(format!("Playing {}", artist.name))
            }
            QueryKind::Queue => {
                let tracks = self.search(&token, "track", &term, 5, "/tracks/items", &artists_of)?;
                let best = rules::pick_track(&tracks, &term).ok_or_else(|| format!("No Spotify tracks found to queue for \"{term}\"."))?;
                self.api.send(&token, "POST", &with_device("/me/player/queue", &format!("uri={}", percent_encode(&best.uri)), d), None)?;
                Ok(format!("Queued {}", label(best)))
            }
            QueryKind::Track => {
                let tracks = self.search(&token, "track", &term, 10, "/tracks/items", &artists_of)?;
                let best = rules::pick_track(&tracks, &term).ok_or_else(|| format!("No Spotify tracks found for \"{term}\"."))?;
                self.play_with_retry(&token, d, &json!({ "uris": [best.uri] }))?;
                // The web call often says "done" while the desktop window stays idle: opening the
                // track's own address is what switches the app to the song.
                self.host.open(&best.uri);
                self.host.focus();
                self.confirm(&token, &best.uri, d);
                Ok(format!("Playing {}", label(best)))
            }
        }
    }

    fn confirm(&self, token: &str, uri: &str, device: Option<&str>) {
        for _ in 0..self.timing.confirm_polls {
            std::thread::sleep(self.timing.confirm_every);
            if self.is_playing(token) == Some(true) {
                return;
            }
        }
        self.host.open(uri);
        if let Ok(retry) = self.ensure_device(token) {
            let _ = self.play_body(token, Some(retry.as_str()).or(device), &json!({ "uris": [uri] }));
        }
    }

    fn playlists(&self, token: &str, max: u32) -> Result<Vec<Candidate>, String> {
        let body = self.get(token, &format!("{API}/me/playlists?limit={}", max.clamp(1, 50)))?;
        Ok(candidates(&body, "/items", &owner_of))
    }

    fn liked_songs(&self, token: &str) -> Result<Option<Candidate>, String> {
        let lists = self.playlists(token, 50)?;
        Ok(lists
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case("Liked Songs"))
            .or_else(|| lists.iter().find(|p| p.name.to_lowercase().contains("liked")))
            .cloned())
    }

    fn resolve_playlist(&self, token: &str, term: &str) -> Result<Option<Candidate>, String> {
        if term.trim().is_empty() {
            return self.liked_songs(token);
        }
        let mine = self.playlists(token, 50)?;
        if let Some(found) = rules::pick_playlist(&mine, term) {
            return Ok(Some(found.clone()));
        }
        let searched = self.search(token, "playlist", term, 5, "/playlists/items", &owner_of)?;
        Ok(rules::pick_playlist(&searched, term).cloned())
    }
}

fn label(track: &Candidate) -> String {
    if track.by.is_empty() { track.name.clone() } else { format!("{} — {}", track.name, track.by) }
}

// ---- glued together for the engine ---------------------------------------------------------------

/// The real Spotify: the saved sign-in, the web API and this PC's desktop.
pub struct Hub {
    pub auth: Arc<Auth>,
    pub host: Arc<dyn Host>,
}

impl Runner for Hub {
    fn run(&self, kind: &str, value: &str) -> Result<String, String> {
        if self.auth.config.is_none() {
            return Err(SETUP_HINT.into());
        }
        let playback = Playback { api: &UreqApi, tokens: self.auth.as_ref(), host: self.host.as_ref(), timing: Timing::REAL };
        match ActionKind::of(kind) {
            ActionKind::SpotifyControl => playback.control(value),
            ActionKind::SpotifyPlay => playback.play(value),
            _ => Err(format!("Unknown Spotify action: {kind}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn the_saved_token_reads_and_writes_the_way_csharp_does() {
        let text = r#"{"AccessToken":"a","RefreshToken":"r","ExpiresAtUtc":"2026-11-08T10:20:30.1234567Z","Scope":"user-read-private"}"#;
        let token = Token::from_json(text).unwrap();
        assert_eq!((token.access.as_str(), token.refresh.as_str()), ("a", "r"));
        assert_eq!(token.expires_at, 1_794_133_230);
        assert_eq!(unix_to_iso(token.expires_at), "2026-11-08T10:20:30.0000000Z");
        assert_eq!(Token::from_json(&token.to_json()).unwrap(), token);
        assert!(Token::from_json(r#"{"AccessToken":"a","RefreshToken":""}"#).is_none(), "a token with no refresh part is not a sign-in");
        assert!(Token::from_json("junk").is_none());
        assert!(token.expired(token.expires_at - 30) && !token.expired(token.expires_at - 3600));
        assert!(!token.has_all_scopes());
        let full = Token { scope: REQUIRED_SCOPES.into(), ..token };
        assert!(full.has_all_scopes());
    }

    #[test]
    fn failures_say_what_to_do() {
        assert!(friendly_error(401, "").contains("Connect / reconnect"));
        assert_eq!(friendly_error(403, r#"{"error":{"status":403,"message":"Player command failed: Premium required","reason":"PREMIUM_REQUIRED"}}"#), "Spotify Premium is needed to control playback.");
        assert!(friendly_error(403, "Insufficient client scope").contains("more permission"));
        assert!(friendly_error(429, "").contains("limiting"));
        assert_eq!(friendly_error(400, r#"{"error":{"message":"Bad thing"}}"#), "Bad thing");
        assert_eq!(friendly_error(500, "<html>"), "Spotify returned 500.");
    }

    #[test]
    fn dates_round_trip_across_leap_years() {
        for unix in [0, 951_782_400, 1_709_251_199, 1_794_133_230, 4_102_444_800] {
            assert_eq!(iso_to_unix(&unix_to_iso(unix)), Some(unix));
        }
        assert_eq!(iso_to_unix("2024-02-29T00:00:00Z"), Some(1_709_164_800));
        assert_eq!(iso_to_unix("nonsense"), None);
    }

    #[test]
    fn urls_and_queries_are_encoded_and_decoded() {
        assert_eq!(percent_encode("a b&c=é"), "a%20b%26c%3D%C3%A9");
        assert_eq!(percent_decode("a%20b+c%3D%C3%A9%"), "a b c=é%");
        assert_eq!(parse_query("?code=x%2By&state=S"), [("code".to_string(), "x+y".to_string()), ("state".to_string(), "S".to_string())]);
    }

    #[test]
    fn the_pkce_challenge_is_the_hash_of_the_verifier() {
        let (verifier, challenge) = pkce_pair().unwrap();
        assert_eq!(verifier.len(), 43);
        assert_eq!(challenge, base64url(&Sha256::digest(verifier.as_bytes())));
        assert!(!challenge.contains(['+', '/', '=']));
    }

    #[test]
    fn the_client_id_comes_from_the_file_then_the_environment() {
        let dir = std::env::temp_dir().join(format!("oasis-spotify-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("spotify_oauth.json"), r#"{"client_id":" abc ","redirect_port":9000,"redirect_path":"cb"}"#).unwrap();
        let config = OAuthConfig::load(std::slice::from_ref(&dir), Some("env")).unwrap();
        assert_eq!((config.client_id.as_str(), config.port, config.path.as_str()), ("abc", 9000, "/cb"));
        assert_eq!(config.redirect_uri(), "http://127.0.0.1:9000/cb");
        let none = OAuthConfig::load(&[dir.join("missing")], Some("env")).unwrap();
        assert_eq!((none.client_id.as_str(), none.port, none.path.as_str()), ("env", 8765, "/callback"));
        assert!(OAuthConfig::load(&[], None).is_none());
    }

    // ---- the playback, against a pretend Spotify -------------------------------------------------

    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<String>>,
        devices: Mutex<Vec<Value>>,
        playing_after: Mutex<u32>,
        reject_first_play: Mutex<bool>,
    }

    impl Api for Fake {
        fn get(&self, _: &str, url: &str) -> Result<String, String> {
            self.calls.lock().unwrap().push(format!("GET {}", url.trim_start_matches(API)));
            if url.contains("/me/player/devices") {
                return Ok(json!({ "devices": *self.devices.lock().unwrap() }).to_string());
            }
            if url.ends_with("/me/player") {
                let mut left = self.playing_after.lock().unwrap();
                let playing = *left == 0;
                *left = left.saturating_sub(1);
                return Ok(json!({ "is_playing": playing }).to_string());
            }
            if url.contains("type=track") {
                return Ok(json!({ "tracks": { "items": [
                    { "uri": "spotify:track:1", "name": "Yellow Submarine", "popularity": 50, "artists": [{ "name": "The Beatles" }] },
                    { "uri": "spotify:track:2", "name": "Yellow", "popularity": 90, "artists": [{ "name": "Coldplay" }] } ] } }).to_string());
            }
            if url.contains("/me/playlists") {
                return Ok(json!({ "items": [
                    { "uri": "spotify:playlist:liked", "name": "Liked Songs", "owner": { "display_name": "me" } },
                    { "uri": "spotify:playlist:gym", "name": "Gym Mix", "owner": { "display_name": "me" } } ] }).to_string());
            }
            Ok("{}".into())
        }

        fn send(&self, _: &str, method: &str, url: &str, body: Option<&str>) -> Result<(), String> {
            self.calls.lock().unwrap().push(format!("{method} {}{}", url.trim_start_matches(API), body.map_or(String::new(), |b| format!(" {b}"))));
            if url.contains("/me/player/play") && std::mem::take(&mut *self.reject_first_play.lock().unwrap()) {
                return Err("Player command failed: No active device found".into());
            }
            Ok(())
        }
    }

    struct Tokens;
    impl TokenSource for Tokens {
        fn access_token(&self) -> Result<String, String> {
            Ok("t".into())
        }
    }

    #[derive(Default)]
    struct FakeHost(Mutex<Vec<String>>);
    impl Host for FakeHost {
        fn open(&self, uri: &str) {
            self.0.lock().unwrap().push(format!("open {uri}"));
        }
        fn focus(&self) {
            self.0.lock().unwrap().push("focus".into());
        }
    }

    fn quick() -> Timing {
        let ms = Duration::from_millis;
        Timing { device_wait: ms(60), device_poll: ms(5), after_transfer: ms(1), confirm_polls: 3, confirm_every: ms(1) }
    }

    fn pc(active: bool) -> Value {
        json!({ "id": "pc1", "name": "PC", "type": "Computer", "is_active": active })
    }

    fn setup(devices: Vec<Value>) -> (Fake, FakeHost) {
        let fake = Fake::default();
        *fake.devices.lock().unwrap() = devices;
        (fake, FakeHost::default())
    }

    fn calls(fake: &Fake) -> Vec<String> {
        fake.calls.lock().unwrap().clone()
    }

    #[test]
    fn controls_go_to_the_computer_and_wake_it_when_it_is_not_the_active_device() {
        let (fake, host) = setup(vec![pc(false)]);
        let p = Playback { api: &fake, tokens: &Tokens, host: &host, timing: quick() };
        assert_eq!(p.control("pause").unwrap(), "Paused Spotify");
        let c = calls(&fake);
        assert_eq!(c[1], "PUT /me/player {\"device_ids\":[\"pc1\"],\"play\":false}", "{c:?}");
        assert_eq!(c[2], "PUT /me/player/pause?device_id=pc1");
        assert_eq!(p.control("volume 150").unwrap(), "Spotify volume 100%");
        assert_eq!(p.control("seek 30s").unwrap(), "Seeked to 30s");
        assert!(calls(&fake).iter().any(|c| c == "PUT /me/player/volume?volume_percent=100&device_id=pc1"));
        assert!(calls(&fake).iter().any(|c| c == "PUT /me/player/seek?position_ms=30000&device_id=pc1"));
        assert!(p.control("juggle").unwrap_err().contains("Unknown"));
    }

    #[test]
    fn with_no_device_the_app_is_woken_once_and_then_given_up_on() {
        let (fake, host) = setup(vec![]);
        let p = Playback { api: &fake, tokens: &Tokens, host: &host, timing: quick() };
        assert!(p.control("next").unwrap_err().contains("No Spotify device found"));
        assert_eq!(*host.0.lock().unwrap(), ["focus", "open spotify:"], "woken once");
    }

    #[test]
    fn a_track_is_searched_chosen_played_and_opened_in_the_app() {
        let (fake, host) = setup(vec![pc(true)]);
        let p = Playback { api: &fake, tokens: &Tokens, host: &host, timing: quick() };
        assert_eq!(p.play("yellow").unwrap(), "Playing Yellow — Coldplay");
        let c = calls(&fake);
        assert!(c.iter().any(|c| c == "PUT /me/player/play?device_id=pc1 {\"uris\":[\"spotify:track:2\"]}"), "{c:?}");
        assert_eq!(*host.0.lock().unwrap(), ["open spotify:track:2", "focus"]);
    }

    #[test]
    fn a_play_that_is_refused_for_want_of_a_device_is_tried_again() {
        let (fake, host) = setup(vec![pc(true)]);
        *fake.reject_first_play.lock().unwrap() = true;
        let p = Playback { api: &fake, tokens: &Tokens, host: &host, timing: quick() };
        p.play("yellow").unwrap();
        let plays = calls(&fake).into_iter().filter(|c| c.starts_with("PUT /me/player/play")).count();
        assert_eq!(plays, 2);
    }

    #[test]
    fn if_the_app_never_starts_playing_the_track_is_opened_and_sent_again() {
        let (fake, host) = setup(vec![pc(true)]);
        *fake.playing_after.lock().unwrap() = 99;
        let p = Playback { api: &fake, tokens: &Tokens, host: &host, timing: quick() };
        p.play("yellow").unwrap();
        let plays = calls(&fake).into_iter().filter(|c| c.starts_with("PUT /me/player/play")).count();
        assert_eq!(plays, 2, "once, then again after the checks failed");
        assert_eq!(host.0.lock().unwrap().iter().filter(|h| h.starts_with("open spotify:track")).count(), 2);
    }

    #[test]
    fn liked_songs_playlists_and_queue_requests_use_the_right_calls() {
        let (fake, host) = setup(vec![pc(true)]);
        let p = Playback { api: &fake, tokens: &Tokens, host: &host, timing: quick() };
        assert_eq!(p.play("my liked songs").unwrap(), "Playing Liked Songs");
        assert_eq!(p.play("my gym playlist").unwrap(), "Playing playlist Gym Mix");
        assert!(p.play("my zzz playlist").unwrap_err().contains("No playlist"));
        assert!(p.play("queue yellow").unwrap().starts_with("Queued Yellow"));
        assert_eq!(p.play("some music").unwrap(), "Resumed Spotify");
        let c = calls(&fake);
        assert!(c.iter().any(|c| c.contains("context_uri\":\"spotify:playlist:liked")));
        assert!(c.iter().any(|c| c.starts_with("POST /me/player/queue?uri=spotify%3Atrack%3A2")), "{c:?}");
        assert_eq!(p.play("   ").unwrap_err(), "Nothing to play.");
    }
}

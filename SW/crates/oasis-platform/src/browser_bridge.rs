//! The local WebSocket the OASIS browser extension connects to (`ws://127.0.0.1:17322`). Port of
//! `Services/BrowserBridge/BrowserBridgeService.cs`. The rules for who may connect and what a
//! message may be are in `oasis_core::bridge`; this is the socket.
//!
//! Everything on the socket is untrusted local input, so in order: the page's origin is checked
//! before the socket opens, the first frame must be a `hello` carrying the per-user token (within
//! 2 s), only `hello`, `ping` and `youtube.*` frames are read, YouTube frames are limited to 60 a
//! second, and frames are capped at 64 KiB. The extension can change which buttons show; it can
//! never add an action (`BrowserBridgePolicy::can_introduce_actions` is always false).
//!
//! Plain threads and the synchronous `tungstenite`: no async runtime (DECISIONS D3).

use std::io::{self, ErrorKind};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use oasis_core::bridge::{BrowserBridgePolicy, YouTubeWatchEvent};
use serde_json::{json, Value};
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{accept_hdr_with_config, Message, WebSocket};

use crate::security_log::SecurityLog;

pub const DEFAULT_PORT: u16 = 17322;
const HELLO_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_FRAME: usize = 64 * 1024;
const MAX_YOUTUBE_EVENTS_PER_SECOND: u32 = 60;
/// How often an idle connection checks for something to send or a stop request.
const POLL: Duration = Duration::from_millis(100);

/// What the bridge tells its owner.
#[derive(Debug, Clone, PartialEq)]
pub enum BridgeEvent {
    /// A `youtube.*` frame from the extension.
    YouTube(YouTubeWatchEvent),
    /// An accepted extension said hello (the owner may want to push settings; the bridge already
    /// sends the current ones).
    Hello,
    /// The status line changed (`LISTENING · 127.0.0.1:17322`, an error, an unauthorized client).
    Status(String),
    /// The number of connected extensions changed.
    Clients(usize),
}

type OnEvent = Arc<dyn Fn(BridgeEvent) + Send + Sync>;
/// The `youtube` object sent to the extension (`oasis.settings`), read fresh on every hello.
type SettingsSource = Arc<dyn Fn() -> Value + Send + Sync>;

struct Shared {
    stop: AtomicBool,
    clients: Mutex<Vec<Sender<String>>>,
    connected: AtomicUsize,
    rejected: AtomicUsize,
    token: String,
    security_log: Option<SecurityLog>,
    on_event: OnEvent,
    settings: SettingsSource,
}

pub struct BridgeServer {
    shared: Arc<Shared>,
    port: u16,
    accept: Option<JoinHandle<()>>,
}

impl BridgeServer {
    /// Listen on `127.0.0.1:port`. `port` 0 picks a free one (see [`BridgeServer::port`]).
    pub fn start(
        port: u16,
        token: String,
        security_log: Option<SecurityLog>,
        settings: impl Fn() -> Value + Send + Sync + 'static,
        on_event: impl Fn(BridgeEvent) + Send + Sync + 'static,
    ) -> io::Result<BridgeServer> {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            clients: Mutex::new(Vec::new()),
            connected: AtomicUsize::new(0),
            rejected: AtomicUsize::new(0),
            token,
            security_log,
            on_event: Arc::new(on_event),
            settings: Arc::new(settings),
        });
        (shared.on_event)(BridgeEvent::Status(format!("LISTENING · 127.0.0.1:{port}")));

        let accept_shared = Arc::clone(&shared);
        let accept = thread::Builder::new()
            .name("oasis-bridge-accept".into())
            .spawn(move || accept_loop(&listener, &accept_shared))?;
        Ok(BridgeServer { shared, port, accept: Some(accept) })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn clients(&self) -> usize {
        self.shared.connected.load(Ordering::SeqCst)
    }

    /// Send `{ "type": "oasis.command", "command": … }` to every connected extension.
    pub fn send_command(&self, command: &str) {
        broadcast(&self.shared, json!({ "type": "oasis.command", "command": command }));
    }

    /// Send the current YouTube settings to every connected extension.
    pub fn push_settings(&self) {
        broadcast(&self.shared, settings_message(&self.shared));
    }

    pub fn stop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(accept) = self.accept.take() {
            let _ = accept.join();
        }
        (self.shared.on_event)(BridgeEvent::Status("STOPPED".into()));
    }
}

impl Drop for BridgeServer {
    fn drop(&mut self) {
        if self.accept.is_some() {
            self.stop();
        }
    }
}

fn settings_message(shared: &Shared) -> Value {
    json!({ "type": "oasis.settings", "youtube": (shared.settings)() })
}

fn broadcast(shared: &Shared, message: Value) {
    let text = message.to_string();
    if let Ok(mut clients) = shared.clients.lock() {
        // A client whose thread has ended has dropped its receiver: forget it.
        clients.retain(|client| client.send(text.clone()).is_ok());
    }
}

fn accept_loop(listener: &TcpListener, shared: &Arc<Shared>) {
    while !shared.stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let shared = Arc::clone(shared);
                let _ = thread::Builder::new()
                    .name("oasis-bridge-client".into())
                    .spawn(move || handle_client(stream, &shared));
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(50)),
            Err(_) => thread::sleep(Duration::from_millis(200)),
        }
    }
}

/// The origin check, run by the handshake before the socket opens.
// The handshake callback's signature is fixed by tungstenite, large error type included.
#[allow(clippy::result_large_err)]
fn check_origin(request: &Request, response: Response) -> Result<Response, ErrorResponse> {
    let origin = request.headers().get("Origin").and_then(|v| v.to_str().ok());
    if BrowserBridgePolicy::is_allowed_origin(origin) {
        Ok(response)
    } else {
        let mut refusal = ErrorResponse::new(Some("origin not allowed".to_string()));
        *refusal.status_mut() = tungstenite::http::StatusCode::FORBIDDEN;
        Err(refusal)
    }
}

fn handle_client(stream: TcpStream, shared: &Arc<Shared>) {
    // An accepted socket inherits the listener's non-blocking mode on Windows; reads must wait.
    let _ = stream.set_nonblocking(false);
    // The handshake itself must not hang forever on a silent connection.
    let _ = stream.set_read_timeout(Some(HELLO_TIMEOUT));
    let _ = stream.set_nodelay(true);
    let config = WebSocketConfig::default().max_message_size(Some(MAX_FRAME)).max_frame_size(Some(MAX_FRAME));
    let Ok(mut socket) = accept_hdr_with_config(stream, check_origin, Some(config)) else {
        return;
    };

    let (outbox, inbox) = mpsc::channel::<String>();
    shared.connected.fetch_add(1, Ordering::SeqCst);
    (shared.on_event)(BridgeEvent::Clients(shared.connected.load(Ordering::SeqCst)));

    serve(&mut socket, &outbox, &inbox, shared);

    let _ = socket.close(None);
    let left = shared.connected.fetch_sub(1, Ordering::SeqCst) - 1;
    (shared.on_event)(BridgeEvent::Clients(left));
}

fn serve(
    socket: &mut WebSocket<TcpStream>,
    outbox: &Sender<String>,
    inbox: &Receiver<String>,
    shared: &Arc<Shared>,
) {
    // The first frame must be a valid hello, within the timeout set on the stream.
    let hello_deadline = Instant::now() + HELLO_TIMEOUT;
    let hello = loop {
        match socket.read() {
            Ok(Message::Text(text)) => break text.to_string(),
            Ok(Message::Ping(_) | Message::Pong(_)) => {}
            Ok(_) => return,
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(e) => {
                note_rejected(shared, &format!("no hello: {e}"));
                return;
            }
        }
        if Instant::now() > hello_deadline {
            note_rejected(shared, "no hello within 2s");
            return;
        }
    };
    if !is_authorized_hello(&hello, &shared.token) {
        note_rejected(shared, "token mismatch");
        let _ = socket.close(Some(tungstenite::protocol::CloseFrame {
            code: tungstenite::protocol::frame::coding::CloseCode::Policy,
            reason: "unauthorized".into(),
        }));
        return;
    }

    // An accepted client clears the "unauthorized" banner.
    if shared.rejected.swap(0, Ordering::SeqCst) >= 3 {
        let port = socket.get_ref().local_addr().map(|a| a.port()).unwrap_or(DEFAULT_PORT);
        (shared.on_event)(BridgeEvent::Status(format!("LISTENING · 127.0.0.1:{port}")));
    }
    if let Ok(mut clients) = shared.clients.lock() {
        clients.push(outbox.clone());
    }
    (shared.on_event)(BridgeEvent::Hello);
    let _ = outbox.send(settings_message(shared).to_string());

    // From here reads time out quickly so queued messages go out and a stop request is noticed.
    let _ = socket.get_ref().set_read_timeout(Some(POLL));
    let mut window_start = Instant::now();
    let mut events_in_window = 0u32;
    while !shared.stop.load(Ordering::SeqCst) {
        loop {
            match inbox.try_recv() {
                Ok(text) => {
                    if socket.send(Message::text(text)).is_err() {
                        return;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                let Some(frame) = parse_frame(&text) else { continue };
                if frame.is_youtube {
                    if window_start.elapsed() > Duration::from_secs(1) {
                        window_start = Instant::now();
                        events_in_window = 0;
                    }
                    events_in_window += 1;
                    if events_in_window > MAX_YOUTUBE_EVENTS_PER_SECOND {
                        continue;
                    }
                }
                if let Some(event) = frame.event {
                    (shared.on_event)(BridgeEvent::YouTube(event));
                } else if frame.message_type == "hello" {
                    let _ = outbox.send(settings_message(shared).to_string());
                }
            }
            Ok(Message::Close(_)) => return,
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(_) => return,
        }
    }
}

/// A surviving frame. Anything not `hello`, `ping` or `youtube.*` never gets this far.
#[derive(Debug, PartialEq)]
struct Frame {
    message_type: String,
    is_youtube: bool,
    event: Option<YouTubeWatchEvent>,
}

fn parse_frame(text: &str) -> Option<Frame> {
    let doc: Value = serde_json::from_str(text).ok()?;
    let message_type = doc.get("type")?.as_str()?.to_string();
    if !BrowserBridgePolicy::is_known_message_type(Some(&message_type)) {
        return None;
    }
    let is_youtube = message_type.to_ascii_lowercase().starts_with("youtube.");
    let text_field = |key: &str| doc.get(key).and_then(Value::as_str).map(str::to_string);
    let event = is_youtube.then(|| YouTubeWatchEvent {
        event_type: message_type.clone(),
        video_id: text_field("videoId"),
        title: text_field("title"),
        url: text_field("url"),
        surface: text_field("surface"),
        percent: doc
            .get("percent")
            .and_then(Value::as_i64)
            .and_then(|p| i32::try_from(p).ok()),
    });
    Some(Frame { message_type, is_youtube, event })
}

fn is_authorized_hello(text: &str, token: &str) -> bool {
    let Ok(doc) = serde_json::from_str::<Value>(text) else { return false };
    let is_hello = doc
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|t| t.eq_ignore_ascii_case("hello"));
    let offered = doc.get("token").and_then(Value::as_str).unwrap_or("");
    is_hello && !token.is_empty() && constant_time_equals(offered, token)
}

/// Compares without stopping at the first difference, so timing says nothing about the token.
fn constant_time_equals(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// A rejected handshake used to be silent on both ends, so a stale extension could be refused
/// every 1.5 s for days. Say so once per burst in the log, and in the status line after three.
fn note_rejected(shared: &Shared, reason: &str) {
    let count = shared.rejected.fetch_add(1, Ordering::SeqCst) + 1;
    if let (true, Some(log)) = (matches!(count, 1 | 10 | 100), shared.security_log.as_ref()) {
        log.write(&format!("BrowserBridge: rejected handshake ({reason}), {count} since last accepted client."));
    }
    if count >= 3 {
        (shared.on_event)(BridgeEvent::Status(format!(
            "UNAUTHORIZED CLIENT · {reason} · reload the OASIS Bridge extension"
        )));
    }
}

// ---- publishing the extension -----------------------------------------------------------------

/// Write the token where the extension reads it. JSON is what the extension fetches on every
/// connect (a service worker caches `importScripts` results forever); the `.js` form stays for
/// extension copies published before 1.3.0.
pub fn write_token_files(folder: &Path, token: &str) -> io::Result<()> {
    std::fs::create_dir_all(folder)?;
    let encoded = Value::String(token.to_string()).to_string();
    std::fs::write(folder.join("bridge-token.json"), format!("{{\"token\":{encoded}}}\n"))?;
    std::fs::write(folder.join("bridge-token.js"), format!("self.OASIS_BRIDGE_TOKEN = {encoded};\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    fn recording_server(token: &str) -> (BridgeServer, Receiver<BridgeEvent>) {
        let (tx, rx) = channel();
        let tx = Mutex::new(tx);
        let server = BridgeServer::start(
            0,
            token.to_string(),
            None,
            || json!({ "gesturesEnabled": true, "hold2xButtonIndex": 1 }),
            move |event| {
                let _ = tx.lock().unwrap().send(event);
            },
        )
        .expect("the bridge starts on a free port");
        (server, rx)
    }

    #[allow(clippy::result_large_err)]
    fn connect(port: u16, origin: Option<&str>) -> tungstenite::Result<WebSocket<TcpStream>> {
        let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut request = format!("ws://127.0.0.1:{port}/oasis").into_client_request_for_test();
        if let Some(origin) = origin {
            request.headers_mut().insert("Origin", origin.parse().unwrap());
        }
        tungstenite::client(request, stream).map(|(socket, _)| socket).map_err(|e| match e {
            tungstenite::HandshakeError::Failure(e) => e,
            tungstenite::HandshakeError::Interrupted(_) => tungstenite::Error::ConnectionClosed,
        })
    }

    trait IntoRequest {
        fn into_client_request_for_test(self) -> tungstenite::handshake::client::Request;
    }
    impl IntoRequest for String {
        fn into_client_request_for_test(self) -> tungstenite::handshake::client::Request {
            use tungstenite::client::IntoClientRequest;
            self.into_client_request().unwrap()
        }
    }

    fn next_event(rx: &Receiver<BridgeEvent>, want: impl Fn(&BridgeEvent) -> bool) -> BridgeEvent {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let event = rx.recv_timeout(left).expect("the expected event arrived");
            if want(&event) {
                return event;
            }
        }
    }

    const EXTENSION: &str = "chrome-extension://abcdefghijklmnop";

    #[test]
    fn the_right_token_gets_in_receives_settings_and_its_youtube_events_arrive() {
        let (server, rx) = recording_server("secret");
        let mut ws = connect(server.port(), Some(EXTENSION)).expect("the handshake works");
        ws.send(Message::text(r#"{"type":"hello","token":"secret"}"#)).unwrap();

        let settings = ws.read().unwrap().to_string();
        let settings: Value = serde_json::from_str(&settings).unwrap();
        assert_eq!(settings["type"], "oasis.settings");
        assert_eq!(settings["youtube"]["hold2xButtonIndex"], 1);

        ws.send(Message::text(
            r#"{"type":"youtube.progress","videoId":"abc","title":"T","url":"https://www.youtube.com/watch?v=abc","surface":"watch","percent":40}"#,
        ))
        .unwrap();
        let BridgeEvent::YouTube(event) = next_event(&rx, |e| matches!(e, BridgeEvent::YouTube(_))) else { unreachable!() };
        assert_eq!(event.event_type, "youtube.progress");
        assert_eq!((event.video_id.as_deref(), event.percent), (Some("abc"), Some(40)));
        assert_eq!(server.clients(), 1);
    }

    #[test]
    fn a_wrong_token_is_refused_and_nothing_it_sends_is_read() {
        let (server, rx) = recording_server("secret");
        let mut ws = connect(server.port(), Some(EXTENSION)).unwrap();
        ws.send(Message::text(r#"{"type":"hello","token":"guess"}"#)).unwrap();
        let _ = ws.send(Message::text(r#"{"type":"youtube.started","videoId":"x"}"#));
        thread::sleep(Duration::from_millis(500));
        assert!(rx.try_iter().all(|e| !matches!(e, BridgeEvent::YouTube(_))));
        assert_eq!(server.clients(), 0, "a refused client is not counted once it has gone");
    }

    #[test]
    fn a_web_page_origin_is_refused_before_the_socket_opens() {
        let (server, _rx) = recording_server("secret");
        assert!(connect(server.port(), Some("https://evil.example")).is_err());
        assert!(connect(server.port(), Some(EXTENSION)).is_ok(), "the extension still can");
    }

    #[test]
    fn commands_reach_every_connected_extension() {
        let (server, _rx) = recording_server("secret");
        let mut ws = connect(server.port(), Some(EXTENSION)).unwrap();
        ws.send(Message::text(r#"{"type":"hello","token":"secret"}"#)).unwrap();
        let _settings = ws.read().unwrap();
        server.send_command("playbackRate.hold2x");
        let message: Value = serde_json::from_str(&ws.read().unwrap().to_string()).unwrap();
        assert_eq!(message["type"], "oasis.command");
        assert_eq!(message["command"], "playbackRate.hold2x");
    }

    #[test]
    fn a_connection_that_never_says_hello_is_dropped() {
        let (server, rx) = recording_server("secret");
        let _ws = connect(server.port(), Some(EXTENSION)).unwrap();
        thread::sleep(Duration::from_millis(2600));
        assert_eq!(server.clients(), 0);
        let _ = rx; // the rejection is counted, not announced, until three have happened
    }

    #[test]
    fn frames_are_filtered_and_decoded() {
        assert!(parse_frame(r#"{"type":"oasis.command","command":"x"}"#).is_none(), "the extension cannot send commands");
        assert!(parse_frame(r#"{"type":"run","value":"calc"}"#).is_none());
        assert!(parse_frame("not json").is_none());
        assert!(parse_frame(r#"{"no":"type"}"#).is_none());
        let ping = parse_frame(r#"{"type":"ping"}"#).unwrap();
        assert!(!ping.is_youtube && ping.event.is_none());
        let ended = parse_frame(r#"{"type":"youtube.ended","percent":"high"}"#).unwrap();
        assert_eq!(ended.event.unwrap().percent, None, "a non-number percent is ignored");
    }

    #[test]
    fn the_hello_needs_the_exact_token() {
        assert!(is_authorized_hello(r#"{"type":"HELLO","token":"abc"}"#, "abc"));
        assert!(!is_authorized_hello(r#"{"type":"hello","token":"abd"}"#, "abc"));
        assert!(!is_authorized_hello(r#"{"type":"hello"}"#, "abc"));
        assert!(!is_authorized_hello(r#"{"type":"ping","token":"abc"}"#, "abc"));
        assert!(!is_authorized_hello(r#"{"type":"hello","token":""}"#, ""), "an empty token never matches");
        assert!(!is_authorized_hello("nope", "abc"));
    }

    #[test]
    fn the_token_files_have_the_shape_the_extension_reads() {
        let dir = std::env::temp_dir().join(format!("oasis-bridge-token-{}", std::process::id()));
        write_token_files(&dir, "a\"b").unwrap();
        let json: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("bridge-token.json")).unwrap()).unwrap();
        assert_eq!(json["token"], "a\"b");
        let js = std::fs::read_to_string(dir.join("bridge-token.js")).unwrap();
        assert_eq!(js, "self.OASIS_BRIDGE_TOKEN = \"a\\\"b\";\n");
    }
}

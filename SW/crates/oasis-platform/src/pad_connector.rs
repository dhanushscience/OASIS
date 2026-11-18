//! The connect/reconnect loop of `Services/BleManager.cs`, over any [`PadTransport`].
//!
//! Poll-driven and single-threaded: the owner calls [`PadConnector::poll`] in a loop (the
//! desktop app's main loop, or a thread of its own). Each call does at most one of: scan and
//! maybe connect, or read notifications from the connected pad and send a latency ping when due.
//! The rules (back-off, debounce, trust, ping rate) are `oasis_core::pad_connect`; what the
//! Bluetooth stack does is the transport's business. This file only decides *when*.
//!
//! Raw notification bytes are passed on untouched, for `oasis_app::PadLink` to decode.
//!
//! Differences from C#, on purpose: the "direct connect to the trusted pad when no scan is
//! running" fallback is not ported (every transport here connects from a scan result), and the
//! status text and events are returned from `poll` instead of raised as .NET events.

use std::time::Duration;

use oasis_core::pad_connect::{
    ConnectPolicy, PING_FIRST_DELAY, RESCAN_DELAY,
};
use oasis_core::Instant;
use oasis_protocol::HostCommand;

use crate::ports::{PadInfo, PadTransport, PlatformError, PlatformResult};
use crate::trusted_device::TrustedDevice;

/// How long `poll` waits for a notification when connected.
const NOTIFICATION_WAIT: Duration = Duration::from_millis(50);
/// How long one scan lasts.
const SCAN_TIME: Duration = Duration::from_millis(1500);
/// Retry delay when Bluetooth is off or missing.
const UNAVAILABLE_RETRY: Duration = Duration::from_secs(5);

/// The status text of the C# manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadStatus {
    Scanning,
    Connecting,
    Connected,
    Disconnected,
    Unavailable,
}

impl PadStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            PadStatus::Scanning => "Scanning",
            PadStatus::Connecting => "Connecting",
            PadStatus::Connected => "Connected",
            PadStatus::Disconnected => "Disconnected",
            PadStatus::Unavailable => "Unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectorEvent {
    Status(PadStatus),
    /// Why Bluetooth is unavailable, for the status panel.
    UnavailableReason(String),
    Connected { name: String },
    Disconnected,
    /// Raw TX bytes from the pad.
    Notification(Vec<u8>),
    /// Signal strength heard while scanning.
    Rssi(i16),
}

pub struct PadConnector<T: PadTransport> {
    transport: T,
    trust: TrustedDevice,
    policy: ConnectPolicy,
    connected_name: Option<String>,
    next_scan_at: Option<Instant>,
    next_ping_at: Option<Instant>,
    pings_enabled: bool,
}

impl<T: PadTransport> PadConnector<T> {
    pub fn new(transport: T, trust: TrustedDevice) -> PadConnector<T> {
        PadConnector {
            transport,
            trust,
            policy: ConnectPolicy::new(),
            connected_name: None,
            next_scan_at: None,
            next_ping_at: None,
            pings_enabled: false,
        }
    }

    /// Let go of the pad and stop looking for it (the person turned the pad off). The events say so.
    pub fn pause(&mut self) -> Vec<ConnectorEvent> {
        let was_connected = self.is_connected();
        self.transport.disconnect();
        self.connected_name = None;
        self.next_ping_at = None;
        self.next_scan_at = None;
        if was_connected {
            self.policy.on_disconnect();
            vec![ConnectorEvent::Disconnected]
        } else {
            Vec::new()
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connected_name.is_some()
    }

    /// Latency pings run only while someone is looking at the numbers (the settings window), as
    /// in C#.
    pub fn set_pings_enabled(&mut self, enabled: bool, now: Instant) {
        self.pings_enabled = enabled;
        self.next_ping_at = (enabled && self.is_connected()).then(|| {
            self.policy.mark_link_active(now);
            now.plus(PING_FIRST_DELAY)
        });
    }

    /// Send a command to the pad. Fails if there is no link.
    pub fn send(&mut self, command: &HostCommand) -> PlatformResult<()> {
        if !self.is_connected() {
            return Err(PlatformError::Unavailable("The OASIS interface is not connected.".into()));
        }
        self.transport.write(&command.to_wire())
    }

    pub fn poll(&mut self, now: Instant) -> Vec<ConnectorEvent> {
        let mut events = Vec::new();
        if self.is_connected() {
            self.poll_connected(now, &mut events);
        } else {
            self.poll_disconnected(now, &mut events);
        }
        events
    }

    // ---- connected ---------------------------------------------------------------------

    fn poll_connected(&mut self, now: Instant, events: &mut Vec<ConnectorEvent>) {
        if self.next_ping_at.is_some_and(|at| now.is_at_or_after(at)) {
            // A failed ping is not an error: if the link is gone, `is_connected` says so below.
            let _ = self.transport.write(&HostCommand::Ping.to_wire());
            self.next_ping_at = Some(now.plus(self.policy.ping_interval(now)));
        }

        match self.transport.next_notification(NOTIFICATION_WAIT) {
            Ok(Some(bytes)) => {
                if contains_button_message(&bytes) {
                    self.policy.mark_link_active(now);
                }
                events.push(ConnectorEvent::Notification(bytes));
            }
            Ok(None) => {}
            Err(_) => self.transport.disconnect(),
        }

        if !self.transport.is_connected() {
            self.on_link_lost(now, events);
        }
    }

    fn on_link_lost(&mut self, now: Instant, events: &mut Vec<ConnectorEvent>) {
        self.connected_name = None;
        self.next_ping_at = None;
        self.policy.on_disconnect();
        self.next_scan_at = Some(now.plus(RESCAN_DELAY));
        events.push(ConnectorEvent::Disconnected);
        events.push(ConnectorEvent::Status(PadStatus::Disconnected));
        events.push(ConnectorEvent::Status(PadStatus::Scanning));
    }

    // ---- looking for the pad -----------------------------------------------------------

    fn poll_disconnected(&mut self, now: Instant, events: &mut Vec<ConnectorEvent>) {
        if self.next_scan_at.is_some_and(|at| !now.is_at_or_after(at)) {
            return;
        }
        self.next_scan_at = None;

        let pads = match self.transport.scan(SCAN_TIME) {
            Ok(pads) => pads,
            Err(e) => {
                events.push(ConnectorEvent::Status(PadStatus::Unavailable));
                events.push(ConnectorEvent::UnavailableReason(e.to_string()));
                self.next_scan_at = Some(now.plus(UNAVAILABLE_RETRY));
                return;
            }
        };

        for pad in pads {
            if !self.trust.is_trusted(&pad.id) {
                continue;
            }
            self.policy.note_advertisement(&pad.id, now);
            if let Some(rssi) = pad.rssi {
                events.push(ConnectorEvent::Rssi(rssi));
            }
            if self.policy.cooldown_remaining(now).is_some()
                || !self.policy.watcher_connect_allowed(now)
            {
                continue;
            }
            self.policy.note_watcher_connect(now);
            self.try_connect(&pad, now, events);
            if self.is_connected() {
                return;
            }
        }
    }

    fn try_connect(&mut self, pad: &PadInfo, now: Instant, events: &mut Vec<ConnectorEvent>) {
        events.push(ConnectorEvent::Status(PadStatus::Connecting));
        match self.transport.connect(pad) {
            Ok(()) => {
                // The first pad that connects becomes the trusted one. Failing to save that is
                // not fatal: the link works, it is just not remembered.
                let _ = self.trust.trust(&pad.id, &pad.name, std::time::SystemTime::now());
                self.policy.clear_cooldown();
                self.policy.mark_link_active(now);
                self.connected_name = Some(pad.name.clone());
                self.next_ping_at = self.pings_enabled.then(|| now.plus(PING_FIRST_DELAY));
                events.push(ConnectorEvent::Status(PadStatus::Connected));
                events.push(ConnectorEvent::Connected { name: pad.name.clone() });
            }
            Err(_) => {
                // The pad was advertising a moment ago, so retry soon rather than backing off hard.
                let fresh = self.policy.has_recent_advertisement(&pad.id, now);
                self.policy.arm_cooldown(now, fresh);
                self.next_scan_at = Some(now.plus(RESCAN_DELAY));
                events.push(ConnectorEvent::Status(PadStatus::Scanning));
            }
        }
    }
}

fn contains_button_message(bytes: &[u8]) -> bool {
    String::from_utf8_lossy(bytes).to_ascii_uppercase().contains("BTN:")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Script {
        pads: Vec<PadInfo>,
        scan_error: Option<PlatformError>,
        connect_results: VecDeque<bool>,
        connected: bool,
        notifications: VecDeque<Vec<u8>>,
        written: Vec<String>,
        scans: u32,
        connects: u32,
    }

    #[derive(Clone, Default)]
    struct Fake(Arc<Mutex<Script>>);

    impl Fake {
        fn with<R>(&self, f: impl FnOnce(&mut Script) -> R) -> R {
            f(&mut self.0.lock().unwrap())
        }
    }

    impl PadTransport for Fake {
        fn scan(&mut self, _: Duration) -> PlatformResult<Vec<PadInfo>> {
            self.with(|s| {
                s.scans += 1;
                match &s.scan_error {
                    Some(e) => Err(e.clone()),
                    None => Ok(s.pads.clone()),
                }
            })
        }
        fn connect(&mut self, _: &PadInfo) -> PlatformResult<()> {
            self.with(|s| {
                s.connects += 1;
                if s.connect_results.pop_front().unwrap_or(true) {
                    s.connected = true;
                    Ok(())
                } else {
                    Err(PlatformError::Unavailable("refused".into()))
                }
            })
        }
        fn disconnect(&mut self) {
            self.with(|s| s.connected = false);
        }
        fn is_connected(&self) -> bool {
            self.with(|s| s.connected)
        }
        fn write(&mut self, wire: &str) -> PlatformResult<()> {
            self.with(|s| s.written.push(wire.to_string()));
            Ok(())
        }
        fn next_notification(&mut self, _: Duration) -> PlatformResult<Option<Vec<u8>>> {
            Ok(self.with(|s| s.notifications.pop_front()))
        }
    }

    fn pad(id: &str) -> PadInfo {
        PadInfo { id: id.into(), name: "OASIS_INTERFACE".into(), rssi: Some(-60) }
    }

    fn trust_file(name: &str, existing: Option<&str>) -> TrustedDevice {
        let dir = std::env::temp_dir().join(format!("oasis-conn-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path: PathBuf = dir.join("trusted_device.json");
        if let Some(id) = existing {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(&path, format!(r#"{{"Address":"{id}"}}"#)).unwrap();
        }
        TrustedDevice::load(path)
    }

    fn t(ms: i64) -> Instant {
        Instant::from_millis(ms)
    }

    fn connector(name: &str, existing: Option<&str>) -> (PadConnector<Fake>, Fake) {
        let fake = Fake::default();
        fake.with(|s| s.pads = vec![pad("100")]);
        (PadConnector::new(fake.clone(), trust_file(name, existing)), fake)
    }

    fn has_status(events: &[ConnectorEvent], status: PadStatus) -> bool {
        events.contains(&ConnectorEvent::Status(status))
    }

    #[test]
    fn the_first_pad_found_connects_and_becomes_trusted() {
        let (mut c, _) = connector("first", None);
        let events = c.poll(t(0));
        assert!(has_status(&events, PadStatus::Connecting));
        assert!(has_status(&events, PadStatus::Connected));
        assert!(events.contains(&ConnectorEvent::Connected { name: "OASIS_INTERFACE".into() }));
        assert!(events.contains(&ConnectorEvent::Rssi(-60)));
        assert!(c.is_connected());
        assert_eq!(c.trust.id(), Some("100"));
    }

    #[test]
    fn once_a_pad_is_trusted_others_are_ignored() {
        let (mut c, fake) = connector("others", Some("999"));
        c.poll(t(0));
        assert!(!c.is_connected());
        assert_eq!(fake.with(|s| s.connects), 0);
    }

    #[test]
    fn a_refused_connect_backs_off_and_retries_after_the_cooldown_and_debounce() {
        let (mut c, fake) = connector("backoff", Some("100"));
        fake.with(|s| s.connect_results = VecDeque::from([false]));

        let events = c.poll(t(0));
        assert!(!c.is_connected());
        assert!(has_status(&events, PadStatus::Scanning));
        assert_eq!(fake.with(|s| s.connects), 1);

        // Rescan delay (400 ms) has not passed.
        c.poll(t(300));
        assert_eq!(fake.with(|s| s.scans), 1);
        // Scans again, but the 1.5 s cooldown and the 2.5 s spacing still hold the connect.
        c.poll(t(2000));
        assert_eq!(fake.with(|s| s.scans), 2);
        assert_eq!(fake.with(|s| s.connects), 1);
        // Spacing passed: connects.
        c.poll(t(2600));
        assert!(c.is_connected());
        assert_eq!(fake.with(|s| s.connects), 2);
    }

    #[test]
    fn notifications_are_passed_on_as_raw_bytes() {
        let (mut c, fake) = connector("notify", Some("100"));
        c.poll(t(0));
        fake.with(|s| s.notifications.push_back(b"BTN:1:D\n".to_vec()));
        assert_eq!(c.poll(t(10)), [ConnectorEvent::Notification(b"BTN:1:D\n".to_vec())]);
        assert!(c.poll(t(20)).is_empty());
    }

    #[test]
    fn losing_the_link_is_reported_and_scanning_resumes_after_the_delay() {
        let (mut c, fake) = connector("lost", Some("100"));
        c.poll(t(0));
        fake.with(|s| s.connected = false);
        let events = c.poll(t(1000));
        assert!(events.contains(&ConnectorEvent::Disconnected));
        assert!(has_status(&events, PadStatus::Disconnected));
        assert!(has_status(&events, PadStatus::Scanning));
        assert!(!c.is_connected());

        let scans = fake.with(|s| s.scans);
        c.poll(t(1200));
        assert_eq!(fake.with(|s| s.scans), scans, "still inside the 400 ms rescan delay");
        c.poll(t(1500));
        assert!(c.is_connected(), "reconnects at once: the disconnect cleared the back-off");
    }

    #[test]
    fn bluetooth_being_off_is_reported_and_retried() {
        let (mut c, fake) = connector("off", Some("100"));
        fake.with(|s| s.scan_error = Some(PlatformError::Unavailable("Bluetooth is off.".into())));
        let events = c.poll(t(0));
        assert!(has_status(&events, PadStatus::Unavailable));
        assert!(events.contains(&ConnectorEvent::UnavailableReason("Bluetooth is off.".into())));

        fake.with(|s| s.scan_error = None);
        c.poll(t(4000));
        assert!(!c.is_connected());
        c.poll(t(5000));
        assert!(c.is_connected());
    }

    #[test]
    fn pings_start_2_seconds_after_connecting_then_every_5_and_only_when_enabled() {
        let (mut c, fake) = connector("ping", Some("100"));
        c.poll(t(0));
        c.poll(t(3000));
        assert!(fake.with(|s| s.written.is_empty()), "pings are off until asked for");

        c.set_pings_enabled(true, t(10_000));
        c.poll(t(11_000));
        assert!(fake.with(|s| s.written.is_empty()));
        c.poll(t(12_000));
        assert_eq!(fake.with(|s| s.written.clone()), ["PING\n"]);
        c.poll(t(16_000));
        assert_eq!(fake.with(|s| s.written.len()), 1);
        c.poll(t(17_000));
        assert_eq!(fake.with(|s| s.written.len()), 2);

        c.set_pings_enabled(false, t(18_000));
        c.poll(t(40_000));
        assert_eq!(fake.with(|s| s.written.len()), 2);
    }

    #[test]
    fn pings_slow_to_two_minutes_once_the_pad_has_been_quiet_for_30_seconds() {
        let (mut c, fake) = connector("idle", Some("100"));
        c.poll(t(0));
        c.set_pings_enabled(true, t(0));
        c.poll(t(2000)); // first ping; next due at 7 s
        c.poll(t(7000));
        c.poll(t(12_000));
        c.poll(t(17_000));
        c.poll(t(22_000));
        c.poll(t(27_000));
        c.poll(t(32_000)); // 32 s since activity at t(0): idle, so the next is 120 s away
        let count = fake.with(|s| s.written.len());
        c.poll(t(100_000));
        assert_eq!(fake.with(|s| s.written.len()), count);
        c.poll(t(152_000));
        assert_eq!(fake.with(|s| s.written.len()), count + 1);
    }

    #[test]
    fn button_traffic_keeps_the_fast_ping_rate() {
        let (mut c, fake) = connector("active", Some("100"));
        c.poll(t(0));
        c.set_pings_enabled(true, t(0));
        fake.with(|s| s.notifications.push_back(b"BTN:0:D\n".to_vec()));
        c.poll(t(29_000));
        c.poll(t(29_001));
        assert!(!c.policy.link_idle(t(40_000)));
    }

    #[test]
    fn sending_needs_a_link() {
        let (mut c, fake) = connector("send", Some("100"));
        assert!(c.send(&HostCommand::Ping).is_err());
        c.poll(t(0));
        c.send(&HostCommand::Ping).unwrap();
        assert_eq!(fake.with(|s| s.written.clone()), ["PING\n"]);
    }
}

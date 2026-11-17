//! The decisions of `Services/BleManager.cs` that are not Bluetooth: when to try connecting,
//! how long to back off after a failure, which pad to trust, how often to probe latency.
//!
//! No clock reads: every method takes `now`. The connection driver in `oasis-app` calls these,
//! and a real or fake transport does the Bluetooth. Pad identity is an opaque string (on
//! Windows and Linux the Bluetooth address in decimal, as the C# trust file stores it).

use std::time::Duration;

use crate::Instant;

const fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// Back-off after a failed connect starts here and doubles (4 s, 8 s, 15 s, 15 s…).
pub const CONNECT_COOLDOWN_BASE: Duration = ms(4000);
pub const CONNECT_COOLDOWN_MAX: Duration = ms(15000);
/// Used instead when the pad was advertising just now: it is awake, so try again soon.
pub const CONNECT_COOLDOWN_SHORT: Duration = ms(1500);
/// Advertisements arrive many times a second; connect attempts started by them are spaced out.
pub const WATCHER_CONNECT_MIN_INTERVAL: Duration = ms(2500);
/// An advertisement this recent counts as "the pad is awake right now".
pub const RECENT_ADV_WINDOW: Duration = ms(8000);
pub const DEVICE_OPEN_TIMEOUT: Duration = ms(8000);
pub const DEVICE_OPEN_TIMEOUT_FRESH_ADV: Duration = ms(4000);
pub const LINK_WAIT_TIMEOUT: Duration = ms(10_000);
/// After a failed connect, wait this long before scanning again.
pub const RESCAN_DELAY: Duration = ms(400);
/// With a trusted pad and no scan running, connect directly after this delay.
pub const TRUSTED_DIRECT_CONNECT_DELAY: Duration = ms(5000);
pub const PING_INTERVAL_ACTIVE: Duration = ms(5000);
pub const PING_INTERVAL_IDLE: Duration = ms(120_000);
pub const PING_FIRST_DELAY: Duration = ms(2000);
/// Matches firmware `CONN_IDLE_AFTER_MS`: no button traffic for this long and pings slow down.
pub const LINK_IDLE_AFTER: Duration = ms(30_000);

/// Whether `id` may connect. With no trusted pad yet every OASIS pad is acceptable (the first
/// one to connect becomes trusted); afterwards only that one.
pub fn is_trusted(trusted: Option<&str>, id: &str) -> bool {
    trusted.is_none_or(|t| t == id)
}

#[derive(Debug, Default)]
pub struct ConnectPolicy {
    fail_streak: u32,
    blocked_until: Option<Instant>,
    last_watcher_connect: Option<Instant>,
    last_adv: Option<(String, Instant)>,
    last_link_activity: Option<Instant>,
}

impl ConnectPolicy {
    pub fn new() -> ConnectPolicy {
        ConnectPolicy::default()
    }

    // ---- cooldown ---------------------------------------------------------------------

    /// Time left before another connect attempt is allowed.
    pub fn cooldown_remaining(&self, now: Instant) -> Option<Duration> {
        let until = self.blocked_until?;
        until.is_at_or_after(now).then(|| until.since(now)).filter(|d| !d.is_zero())
    }

    /// Block connecting after a failure and return how long. A `short` cooldown does not count
    /// toward the back-off.
    pub fn arm_cooldown(&mut self, now: Instant, short: bool) -> Duration {
        let cooldown = if short {
            CONNECT_COOLDOWN_SHORT
        } else {
            self.fail_streak = (self.fail_streak + 1).min(6);
            (CONNECT_COOLDOWN_BASE * (1 << (self.fail_streak - 1).min(3))).min(CONNECT_COOLDOWN_MAX)
        };
        self.blocked_until = Some(now.plus(cooldown));
        cooldown
    }

    pub fn clear_cooldown(&mut self) {
        self.fail_streak = 0;
        self.blocked_until = None;
    }

    pub fn fail_streak(&self) -> u32 {
        self.fail_streak
    }

    /// The link dropped: start fresh so the pad reconnects as soon as it advertises.
    pub fn on_disconnect(&mut self) {
        self.clear_cooldown();
        self.last_watcher_connect = None;
        self.last_link_activity = None;
    }

    // ---- advertisements ---------------------------------------------------------------

    pub fn note_advertisement(&mut self, id: &str, now: Instant) {
        self.last_adv = Some((id.to_string(), now));
    }

    pub fn has_recent_advertisement(&self, id: &str, now: Instant) -> bool {
        self.last_adv
            .as_ref()
            .is_some_and(|(adv_id, at)| adv_id == id && now.since(*at) <= RECENT_ADV_WINDOW)
    }

    /// How long to wait for the OS to open the device. Shorter when the pad is known awake.
    pub fn device_open_timeout(&self, id: &str, now: Instant) -> Duration {
        if self.has_recent_advertisement(id, now) {
            DEVICE_OPEN_TIMEOUT_FRESH_ADV
        } else {
            DEVICE_OPEN_TIMEOUT
        }
    }

    // ---- advertisement-triggered connects ----------------------------------------------

    /// `false` if the last attempt that an advertisement started was under 2.5 s ago.
    pub fn watcher_connect_allowed(&self, now: Instant) -> bool {
        self.last_watcher_connect.is_none_or(|at| now.since(at) >= WATCHER_CONNECT_MIN_INTERVAL)
    }

    pub fn note_watcher_connect(&mut self, now: Instant) {
        self.last_watcher_connect = Some(now);
    }

    // ---- latency pings ----------------------------------------------------------------

    /// A button message arrived: the link is in use.
    pub fn mark_link_active(&mut self, now: Instant) {
        self.last_link_activity = Some(now);
    }

    /// Clears the idle clock (pings stopped or the link closed).
    pub fn reset_link_activity(&mut self) {
        self.last_link_activity = None;
    }

    pub fn link_idle(&self, now: Instant) -> bool {
        self.last_link_activity.is_some_and(|at| now.since(at) >= LINK_IDLE_AFTER)
    }

    pub fn ping_interval(&self, now: Instant) -> Duration {
        if self.link_idle(now) {
            PING_INTERVAL_IDLE
        } else {
            PING_INTERVAL_ACTIVE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(ms: i64) -> Instant {
        Instant::from_millis(ms)
    }

    #[test]
    fn the_first_pad_is_accepted_and_then_only_that_one() {
        assert!(is_trusted(None, "111"));
        assert!(is_trusted(Some("111"), "111"));
        assert!(!is_trusted(Some("111"), "222"));
    }

    #[test]
    fn failures_back_off_4_8_15_15_seconds() {
        let mut p = ConnectPolicy::new();
        let secs = |p: &mut ConnectPolicy| p.arm_cooldown(t(0), false).as_millis();
        assert_eq!([secs(&mut p), secs(&mut p), secs(&mut p), secs(&mut p)], [4000, 8000, 15000, 15000]);
        assert_eq!(p.fail_streak(), 4);
    }

    #[test]
    fn the_failure_streak_stops_counting_at_six() {
        let mut p = ConnectPolicy::new();
        for _ in 0..20 {
            p.arm_cooldown(t(0), false);
        }
        assert_eq!(p.fail_streak(), 6);
    }

    #[test]
    fn a_short_cooldown_is_1500_ms_and_does_not_extend_the_back_off() {
        let mut p = ConnectPolicy::new();
        assert_eq!(p.arm_cooldown(t(0), true), CONNECT_COOLDOWN_SHORT);
        assert_eq!(p.fail_streak(), 0);
        assert_eq!(p.arm_cooldown(t(0), false), CONNECT_COOLDOWN_BASE);
    }

    #[test]
    fn the_cooldown_counts_down_and_ends() {
        let mut p = ConnectPolicy::new();
        assert_eq!(p.cooldown_remaining(t(0)), None);
        p.arm_cooldown(t(1000), false);
        assert_eq!(p.cooldown_remaining(t(1000)), Some(ms(4000)));
        assert_eq!(p.cooldown_remaining(t(3000)), Some(ms(2000)));
        assert_eq!(p.cooldown_remaining(t(5000)), None);
        assert_eq!(p.cooldown_remaining(t(9000)), None);
    }

    #[test]
    fn a_disconnect_clears_the_back_off_and_the_debounce() {
        let mut p = ConnectPolicy::new();
        p.arm_cooldown(t(0), false);
        p.note_watcher_connect(t(0));
        p.on_disconnect();
        assert_eq!(p.cooldown_remaining(t(1)), None);
        assert_eq!(p.fail_streak(), 0);
        assert!(p.watcher_connect_allowed(t(1)));
    }

    #[test]
    fn advertisement_triggered_connects_are_spaced_2500_ms() {
        let mut p = ConnectPolicy::new();
        assert!(p.watcher_connect_allowed(t(0)));
        p.note_watcher_connect(t(1000));
        assert!(!p.watcher_connect_allowed(t(1000)));
        assert!(!p.watcher_connect_allowed(t(3499)));
        assert!(p.watcher_connect_allowed(t(3500)));
    }

    #[test]
    fn a_recent_advertisement_shortens_the_open_timeout() {
        let mut p = ConnectPolicy::new();
        assert_eq!(p.device_open_timeout("a", t(0)), DEVICE_OPEN_TIMEOUT);
        p.note_advertisement("a", t(1000));
        assert_eq!(p.device_open_timeout("a", t(5000)), DEVICE_OPEN_TIMEOUT_FRESH_ADV);
        assert_eq!(p.device_open_timeout("a", t(9000)), DEVICE_OPEN_TIMEOUT_FRESH_ADV);
        assert_eq!(p.device_open_timeout("a", t(9001)), DEVICE_OPEN_TIMEOUT);
        // Another pad's advertisement says nothing about this one.
        assert_eq!(p.device_open_timeout("b", t(2000)), DEVICE_OPEN_TIMEOUT);
    }

    #[test]
    fn pings_slow_down_after_30_seconds_without_button_traffic() {
        let mut p = ConnectPolicy::new();
        // Nothing seen yet: not idle (C# treats "never active" as active).
        assert_eq!(p.ping_interval(t(60_000)), PING_INTERVAL_ACTIVE);
        p.mark_link_active(t(0));
        assert_eq!(p.ping_interval(t(29_999)), PING_INTERVAL_ACTIVE);
        assert_eq!(p.ping_interval(t(30_000)), PING_INTERVAL_IDLE);
        p.mark_link_active(t(31_000));
        assert_eq!(p.ping_interval(t(32_000)), PING_INTERVAL_ACTIVE);
        p.mark_link_active(t(0));
        p.reset_link_activity();
        assert_eq!(p.ping_interval(t(99_000)), PING_INTERVAL_ACTIVE);
    }
}

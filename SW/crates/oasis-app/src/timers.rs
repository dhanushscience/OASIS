//! Named one-shot timers, polled by the host (replaces C# `ITimerScheduler` callbacks).
//!
//! Setting a name that is already set replaces its deadline, exactly like the C# scheduler's
//! dictionary. The host asks [`TimerQueue::next_deadline`] how long its event loop may sleep and
//! calls [`TimerQueue::take_due`] when it wakes.

use std::collections::BTreeMap;
use std::time::Duration;

use oasis_core::Instant;

#[derive(Debug, Default, Clone)]
pub struct TimerQueue {
    deadlines: BTreeMap<&'static str, Instant>,
}

impl TimerQueue {
    pub fn new() -> TimerQueue {
        TimerQueue::default()
    }

    pub fn set(&mut self, name: &'static str, now: Instant, delay: Duration) {
        self.deadlines.insert(name, now.plus(delay));
    }

    pub fn cancel(&mut self, name: &str) {
        self.deadlines.remove(name);
    }

    pub fn cancel_all(&mut self) {
        self.deadlines.clear();
    }

    pub fn is_set(&self, name: &str) -> bool {
        self.deadlines.contains_key(name)
    }

    pub fn is_empty(&self) -> bool {
        self.deadlines.is_empty()
    }

    /// The earliest deadline, if any timer is set.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.deadlines.values().min().copied()
    }

    /// Removes and returns every timer whose deadline is at or before `now`, earliest first.
    pub fn take_due(&mut self, now: Instant) -> Vec<&'static str> {
        let mut due: Vec<(&'static str, Instant)> = self
            .deadlines
            .iter()
            .filter(|(_, d)| now.is_at_or_after(**d))
            .map(|(n, d)| (*n, *d))
            .collect();
        due.sort_by_key(|(_, d)| *d);
        for (name, _) in &due {
            self.deadlines.remove(name);
        }
        due.into_iter().map(|(n, _)| n).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_replace_cancel_and_take_due() {
        let mut q = TimerQueue::new();
        let t0 = Instant::from_millis(0);
        q.set("a", t0, Duration::from_millis(100));
        q.set("b", t0, Duration::from_millis(50));
        q.set("a", t0, Duration::from_millis(200)); // replaces
        assert_eq!(q.next_deadline(), Some(Instant::from_millis(50)));
        assert_eq!(q.take_due(Instant::from_millis(60)), vec!["b"]);
        assert!(q.take_due(Instant::from_millis(150)).is_empty());
        q.cancel("a");
        assert!(q.is_empty());
    }
}

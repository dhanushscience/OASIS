//! Glue between the wire protocol and the pad reducer (the parsing half of C# `BleManager`).
//!
//! Feed it raw notification bytes; it returns pad events for the arbiter and keeps battery and
//! latency telemetry for the status panel. The transport (BLE on each OS) lives in
//! `oasis-platform`; this file does not care where the bytes came from, so the same code runs
//! against a real pad, a recorded log, or the `oasis-cli` simulator.

use oasis_core::input::PadEvent;
use oasis_core::{ButtonId, Instant};
use oasis_protocol::{BatteryCharge, ButtonEdge, DeviceMessage, LineDecoder};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PadTelemetry {
    pub battery_percent: Option<u8>,
    pub is_charging: bool,
    pub is_full: bool,
    pub link_latency_ms: Option<u32>,
    /// Time of the last button message (C# `MarkLinkActive`).
    pub last_active: Option<Instant>,
}

#[derive(Debug, Default)]
pub struct PadLink {
    decoder: LineDecoder,
    telemetry: PadTelemetry,
}

impl PadLink {
    pub fn new() -> PadLink {
        PadLink::default()
    }

    pub fn telemetry(&self) -> &PadTelemetry {
        &self.telemetry
    }

    /// Handle one BLE notification. Returns the pad events it carried, in order, plus whether
    /// telemetry changed.
    pub fn on_notification(&mut self, bytes: &[u8], now: Instant) -> (Vec<PadEvent>, bool) {
        let mut events = Vec::new();
        let mut telemetry_changed = false;
        for msg in self.decoder.push_and_parse(bytes) {
            match msg {
                DeviceMessage::Button { index, edge } => {
                    self.telemetry.last_active = Some(now);
                    let b = ButtonId(index);
                    events.push(match edge {
                        ButtonEdge::Down => PadEvent::Down(b, now),
                        ButtonEdge::Up => PadEvent::Up(b, now),
                    });
                }
                DeviceMessage::Battery { percent, charge } => {
                    let pct = DeviceMessage::battery_percent_clamped(percent);
                    let charging = charge == BatteryCharge::Charging;
                    let full = charge == BatteryCharge::Full;
                    let t = &mut self.telemetry;
                    if t.battery_percent != Some(pct)
                        || t.is_charging != charging
                        || t.is_full != full
                    {
                        t.battery_percent = Some(pct);
                        t.is_charging = charging;
                        t.is_full = full;
                        telemetry_changed = true;
                    }
                }
                DeviceMessage::Latency { ms } => {
                    self.telemetry.link_latency_ms = Some(ms);
                    telemetry_changed = true;
                }
                // Power telemetry is recognised and deliberately ignored; unknown lines are
                // ignored, never fatal (PROTOCOLS.md).
                DeviceMessage::Power(_) | DeviceMessage::Unknown(_) => {}
            }
        }
        (events, telemetry_changed)
    }

    /// The link dropped: the arbiter must cancel every gesture.
    pub fn on_disconnected(&mut self) -> PadEvent {
        self.decoder = LineDecoder::new();
        PadEvent::Disconnected
    }
}

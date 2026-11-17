//! Device → host messages. Parsing mirrors `BleManager.TxCharacteristic_ValueChanged` and
//! `TryParseBatteryMessage` in the C# app so the two read the same bytes the same way.

/// Which edge of a button press. Already debounced (30 ms) by the firmware.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonEdge {
    Down,
    Up,
}

/// The suffix after `BAT:{n}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatteryCharge {
    /// `BAT:{n}` — on battery.
    Discharging,
    /// `BAT:{n}:C` — USB present, charging.
    Charging,
    /// `BAT:{n}:F` — USB present, cell full.
    Full,
    /// Any other suffix. Not charging, kept for the log.
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceMessage {
    /// `BTN:{i}:D` / `BTN:{i}:U`. A bare `BTN:{i}` (no edge) is a press, as in the C# app.
    /// `index` is not range-checked here; the reducer ignores non-pad indices.
    Button { index: i32, edge: ButtonEdge },
    /// `BAT:…`. `percent` is as sent; use [`DeviceMessage::battery_percent_clamped`] for display.
    Battery { percent: i32, charge: BatteryCharge },
    /// `LAT:{ms}`, reply to `PING`. Negative values are clamped to 0.
    Latency { ms: u32 },
    /// `PWR:*` power telemetry — recognised and deliberately ignored.
    Power(String),
    /// Anything else, including malformed known prefixes. Ignore it; never drop the link.
    Unknown(String),
}

impl DeviceMessage {
    pub fn battery_percent_clamped(percent: i32) -> u8 {
        percent.clamp(0, 100) as u8
    }

    /// Whether this message proves the link is alive and in use (C# `MarkLinkActive`).
    pub fn marks_link_active(&self) -> bool {
        matches!(self, DeviceMessage::Button { .. })
    }
}

fn strip_prefix_ignore_case<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len()
        && s.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
    {
        s.get(prefix.len()..)
    } else {
        None
    }
}

/// .NET `int.TryParse` accepts surrounding whitespace and a leading sign.
fn parse_int(s: &str) -> Option<i32> {
    s.trim().parse().ok()
}

/// Parse one line (without its newline). Never fails.
pub fn parse_line(line: &str) -> DeviceMessage {
    let message = line.trim();
    let unknown = || DeviceMessage::Unknown(message.to_string());

    if let Some(payload) = strip_prefix_ignore_case(message, "BAT:") {
        return match payload.split_once(':') {
            Some((pct, suffix)) => match parse_int(pct) {
                Some(percent) => {
                    let suffix = suffix.trim();
                    let charge = if suffix.eq_ignore_ascii_case("C") {
                        BatteryCharge::Charging
                    } else if suffix.eq_ignore_ascii_case("F") {
                        BatteryCharge::Full
                    } else if suffix.is_empty() {
                        BatteryCharge::Discharging
                    } else {
                        BatteryCharge::Other(suffix.to_string())
                    };
                    DeviceMessage::Battery { percent, charge }
                }
                None => unknown(),
            },
            None => match parse_int(payload) {
                Some(percent) => DeviceMessage::Battery {
                    percent,
                    charge: BatteryCharge::Discharging,
                },
                None => unknown(),
            },
        };
    }

    if let Some(payload) = strip_prefix_ignore_case(message, "BTN:") {
        let mut parts = payload.split(':');
        let Some(index) = parts.next().and_then(parse_int) else {
            return unknown();
        };
        return match parts.next() {
            None => DeviceMessage::Button {
                index,
                edge: ButtonEdge::Down,
            },
            Some(state) if state.eq_ignore_ascii_case("D") => DeviceMessage::Button {
                index,
                edge: ButtonEdge::Down,
            },
            Some(state) if state.eq_ignore_ascii_case("U") => DeviceMessage::Button {
                index,
                edge: ButtonEdge::Up,
            },
            Some(_) => unknown(),
        };
    }

    if strip_prefix_ignore_case(message, "PWR:").is_some() {
        return DeviceMessage::Power(message.to_string());
    }

    if let Some(payload) = strip_prefix_ignore_case(message, "LAT:") {
        return match parse_int(payload) {
            Some(ms) => DeviceMessage::Latency {
                ms: ms.max(0) as u32,
            },
            None => unknown(),
        };
    }

    unknown()
}

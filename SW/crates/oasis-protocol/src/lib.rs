//! # oasis-protocol
//!
//! The device link between the pad and the computer, exactly as specified in
//! `docs/PROTOCOLS.md` §1 of the C# repo. **Frozen**: firmware and app version independently.
//!
//! - [`gatt`]: service/characteristic UUIDs and advertised names.
//! - [`device`]: device → host messages (`BTN:`, `BAT:`, `LAT:`, `PWR:`), parsed leniently.
//! - [`host`]: host → device commands (`SCR:*`, `PING`), formatted with the trailing newline.
//! - [`LineDecoder`]: turns BLE notifications (arbitrary byte chunks) into complete lines.
//!
//! Rule from the spec: unknown messages are **ignored, never fatal**. Nothing here returns an
//! error for a line it does not understand; it returns [`DeviceMessage::Unknown`].

pub mod device;
pub mod gatt;
pub mod host;

pub use device::{parse_line, BatteryCharge, ButtonEdge, DeviceMessage};
pub use host::HostCommand;

/// Splits BLE notifications into protocol lines.
///
/// The spec says every message is newline-terminated. The C# app treated each notification as
/// one message and trimmed it, so a firmware that ever sends a notification *without* the
/// newline still works there. This decoder honours both: it splits on `\n`, and at the end of
/// each notification it also flushes a trailing fragment, because Nordic UART notifications
/// from the OASIS firmware are always whole messages.
#[derive(Debug, Default)]
pub struct LineDecoder {
    pending: Vec<u8>,
}

/// Longest line we keep buffering. Anything longer is garbage from a confused peer.
const MAX_LINE: usize = 512;

impl LineDecoder {
    pub fn new() -> LineDecoder {
        LineDecoder::default()
    }

    /// Feed one notification; returns the complete lines it contained, trimmed, without
    /// empty lines.
    pub fn push_notification(&mut self, bytes: &[u8]) -> Vec<String> {
        let mut out = Vec::new();
        for &b in bytes {
            if b == b'\n' {
                self.flush_into(&mut out);
            } else if self.pending.len() < MAX_LINE {
                self.pending.push(b);
            }
        }
        self.flush_into(&mut out);
        out
    }

    /// Feed one notification and parse every line in it.
    pub fn push_and_parse(&mut self, bytes: &[u8]) -> Vec<DeviceMessage> {
        self.push_notification(bytes)
            .iter()
            .map(|l| parse_line(l))
            .collect()
    }

    fn flush_into(&mut self, out: &mut Vec<String>) {
        if self.pending.is_empty() {
            return;
        }
        let line = String::from_utf8_lossy(&self.pending).trim().to_string();
        self.pending.clear();
        if !line.is_empty() {
            out.push(line);
        }
    }
}

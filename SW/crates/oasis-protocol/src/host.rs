//! Host → device commands (PROTOCOLS.md §1 "Host → device").
//!
//! `SCR:*` is never sent by the shipped build ([`crate::gatt::DEVICE_HAS_DISPLAY`] is false);
//! it is kept so the display can come back without a protocol change.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostCommand {
    /// Profile title, max ~14 ASCII characters on the display.
    ScreenTitle(String),
    /// Per-button label, button 0..=2.
    ScreenButtonLabel { button: u8, label: String },
    /// AI "thinking" pulse on/off.
    ScreenThinking(bool),
    /// Reset screen state.
    ScreenClear,
    /// Latency probe; the device replies `LAT:{ms}`.
    Ping,
}

impl HostCommand {
    /// The exact bytes to write to the RX characteristic, newline included. Newlines inside
    /// user text are replaced with spaces so a label can never inject a second command.
    pub fn to_wire(&self) -> String {
        let clean = |s: &str| s.replace(['\r', '\n'], " ");
        let body = match self {
            HostCommand::ScreenTitle(t) => format!("SCR:TITLE:{}", clean(t)),
            HostCommand::ScreenButtonLabel { button, label } => {
                format!("SCR:B{}:{}", button, clean(label))
            }
            HostCommand::ScreenThinking(on) => format!("SCR:THINK:{}", u8::from(*on)),
            HostCommand::ScreenClear => "SCR:CLR".to_string(),
            HostCommand::Ping => "PING".to_string(),
        };
        body + "\n"
    }

    /// `SCR:*` commands need a display; the shipped pad has none.
    pub fn needs_display(&self) -> bool {
        !matches!(self, HostCommand::Ping)
    }
}

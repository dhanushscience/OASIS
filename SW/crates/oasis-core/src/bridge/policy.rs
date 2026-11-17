//! Port of `Bridge/BrowserBridgePolicy.cs` (GATE-1 origin and message-type rules).
//!
//! The per-user token stops a random web page and another local user on loopback. It does
//! **not** stop a process running as the same OS user — that process can read the token file.
//! This is defence in depth, not a boundary; keep saying so wherever it is used.

use crate::matching::text::{is_blank, starts_with_ignore_case};

pub struct BrowserBridgePolicy;

impl BrowserBridgePolicy {
    /// Extensions (Chrome, Firefox, Safari), loopback pages and a missing/`null` origin.
    pub fn is_allowed_origin(origin: Option<&str>) -> bool {
        if is_blank(origin) || origin == Some("null") {
            return true;
        }
        let o = origin.unwrap_or("");
        [
            "chrome-extension://",
            "moz-extension://",
            "safari-web-extension://",
            "http://127.0.0.1",
            "http://localhost",
        ]
        .iter()
        .any(|p| starts_with_ignore_case(o, p))
    }

    pub fn is_known_message_type(message_type: Option<&str>) -> bool {
        if is_blank(message_type) {
            return false;
        }
        let t = message_type.unwrap_or("");
        t == "hello" || t == "ping" || starts_with_ignore_case(t, "youtube.")
    }

    /// Bridge input may change YouTube gating and overlay presentation. It can never introduce
    /// a new action type.
    pub fn can_introduce_actions(_message_type: Option<&str>) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::BrowserBridgePolicy as P;

    #[test]
    fn trusted_origins_are_allowed() {
        for o in [
            None,
            Some(""),
            Some("null"),
            Some("chrome-extension://abcdef"),
            Some("http://127.0.0.1:17322"),
        ] {
            assert!(P::is_allowed_origin(o), "{o:?}");
        }
    }

    #[test]
    fn web_page_origins_are_rejected() {
        for o in ["https://evil.example", "https://youtube.com", "file://"] {
            assert!(!P::is_allowed_origin(Some(o)), "{o}");
        }
    }

    #[test]
    fn known_types_are_accepted_and_unknown_dropped() {
        for t in ["hello", "ping", "youtube.progress", "youtube.ended"] {
            assert!(P::is_known_message_type(Some(t)), "{t}");
        }
        for t in ["execute", "RunScript", "oasis.command"] {
            assert!(!P::is_known_message_type(Some(t)), "{t}");
        }
    }

    #[test]
    fn bridge_messages_cannot_introduce_actions() {
        assert!(!P::can_introduce_actions(Some("youtube.ended")));
        assert!(!P::can_introduce_actions(Some("RunScript")));
    }
}

//! GATT identifiers and device names (PROTOCOLS.md §1 "GATT"; C# `Platform/OasisPaths.cs`).

/// Nordic UART Service.
pub const SERVICE_UUID: &str = "6E400001-B5A3-F393-E0A9-E50E24DCCA9E";
/// Host → device (write / write-without-response).
pub const RX_CHARACTERISTIC_UUID: &str = "6E400002-B5A3-F393-E0A9-E50E24DCCA9E";
/// Device → host (notify).
pub const TX_CHARACTERISTIC_UUID: &str = "6E400003-B5A3-F393-E0A9-E50E24DCCA9E";

/// The name the shipped firmware advertises.
pub const DEVICE_NAME: &str = "OASIS_INTERFACE";
/// The name older firmware advertised.
pub const LEGACY_DEVICE_NAME: &str = "OASIS Macro Pad";

/// Whether an advertised name is an OASIS pad. Any `OASIS*` prefix counts, so a renamed device
/// still pairs.
pub fn is_oasis_ble_name(name: Option<&str>) -> bool {
    let Some(name) = name.filter(|n| !n.is_empty()) else {
        return false;
    };
    name.eq_ignore_ascii_case(DEVICE_NAME)
        || name.eq_ignore_ascii_case(LEGACY_DEVICE_NAME)
        || (name.len() >= 5 && name.as_bytes()[..5].eq_ignore_ascii_case(b"OASIS"))
}

/// Matches firmware `OASIS_HAS_DISPLAY 0`: shipped pads have no screen, so the app never sends
/// `SCR:*`. Flip both this and the firmware flag to bring the display back.
pub const DEVICE_HAS_DISPLAY: bool = false;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(is_oasis_ble_name(Some("OASIS_INTERFACE")));
        assert!(is_oasis_ble_name(Some("oasis macro pad")));
        assert!(is_oasis_ble_name(Some("OASIS-2")));
        assert!(!is_oasis_ble_name(Some("OASI")));
        assert!(!is_oasis_ble_name(Some("Keyboard")));
        assert!(!is_oasis_ble_name(Some("")));
        assert!(!is_oasis_ble_name(None));
    }
}

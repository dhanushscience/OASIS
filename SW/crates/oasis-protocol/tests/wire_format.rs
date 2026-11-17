//! Pins the frozen wire format. Every example comes from docs/PROTOCOLS.md §1 or from how the
//! C# `BleManager` parses a line.

use oasis_protocol::*;

#[test]
fn buttons() {
    assert_eq!(
        parse_line("BTN:0:D"),
        DeviceMessage::Button {
            index: 0,
            edge: ButtonEdge::Down
        }
    );
    assert_eq!(
        parse_line("BTN:2:U"),
        DeviceMessage::Button {
            index: 2,
            edge: ButtonEdge::Up
        }
    );
    assert_eq!(
        parse_line("btn:1:d"),
        DeviceMessage::Button {
            index: 1,
            edge: ButtonEdge::Down
        }
    );
    // A bare index is a press (C#: ButtonPressed when there is no state part).
    assert_eq!(
        parse_line("BTN:1"),
        DeviceMessage::Button {
            index: 1,
            edge: ButtonEdge::Down
        }
    );
    assert!(matches!(parse_line("BTN:x:D"), DeviceMessage::Unknown(_)));
    assert!(matches!(parse_line("BTN:1:Q"), DeviceMessage::Unknown(_)));
}

#[test]
fn battery() {
    assert_eq!(
        parse_line("BAT:80"),
        DeviceMessage::Battery {
            percent: 80,
            charge: BatteryCharge::Discharging
        }
    );
    assert_eq!(
        parse_line("BAT:80:C"),
        DeviceMessage::Battery {
            percent: 80,
            charge: BatteryCharge::Charging
        }
    );
    assert_eq!(
        parse_line("BAT:100:F"),
        DeviceMessage::Battery {
            percent: 100,
            charge: BatteryCharge::Full
        }
    );
    assert_eq!(
        parse_line("BAT:50:X"),
        DeviceMessage::Battery {
            percent: 50,
            charge: BatteryCharge::Other("X".into())
        }
    );
    assert!(matches!(parse_line("BAT:abc"), DeviceMessage::Unknown(_)));
    assert_eq!(DeviceMessage::battery_percent_clamped(140), 100);
    assert_eq!(DeviceMessage::battery_percent_clamped(-3), 0);
}

#[test]
fn latency_power_and_unknown() {
    assert_eq!(parse_line("LAT:12"), DeviceMessage::Latency { ms: 12 });
    assert_eq!(parse_line("LAT:-5"), DeviceMessage::Latency { ms: 0 });
    assert!(matches!(parse_line("PWR:IDLE"), DeviceMessage::Power(_)));
    assert!(matches!(parse_line("HELLO"), DeviceMessage::Unknown(_)));
    assert!(matches!(parse_line(""), DeviceMessage::Unknown(_)));
}

#[test]
fn decoder_splits_lines_and_flushes_whole_notifications() {
    let mut d = LineDecoder::new();
    assert_eq!(d.push_notification(b"BTN:0:D\n"), vec!["BTN:0:D"]);
    // Two messages in one notification.
    assert_eq!(
        d.push_notification(b"BTN:0:U\nBAT:77\n"),
        vec!["BTN:0:U", "BAT:77"]
    );
    // A notification without the newline is still one message.
    assert_eq!(d.push_notification(b"LAT:9"), vec!["LAT:9"]);
    // CRLF and blank lines are tolerated.
    assert_eq!(d.push_notification(b"\r\n\nBTN:1:D\r\n"), vec!["BTN:1:D"]);
    let parsed = d.push_and_parse(b"BTN:2:D\nGARBAGE\n");
    assert_eq!(
        parsed[0],
        DeviceMessage::Button {
            index: 2,
            edge: ButtonEdge::Down
        }
    );
    assert!(matches!(parsed[1], DeviceMessage::Unknown(_)));
}

#[test]
fn host_commands() {
    assert_eq!(HostCommand::Ping.to_wire(), "PING\n");
    assert_eq!(
        HostCommand::ScreenTitle("YouTube".into()).to_wire(),
        "SCR:TITLE:YouTube\n"
    );
    assert_eq!(
        HostCommand::ScreenButtonLabel {
            button: 1,
            label: "PLAY".into()
        }
        .to_wire(),
        "SCR:B1:PLAY\n"
    );
    assert_eq!(HostCommand::ScreenThinking(true).to_wire(), "SCR:THINK:1\n");
    assert_eq!(HostCommand::ScreenClear.to_wire(), "SCR:CLR\n");
    // A label cannot inject a second command.
    assert_eq!(
        HostCommand::ScreenTitle("a\nPING".into()).to_wire(),
        "SCR:TITLE:a PING\n"
    );
    assert!(!HostCommand::Ping.needs_display());
    assert!(HostCommand::ScreenClear.needs_display());
}

#[test]
fn gatt_ids_are_nordic_uart() {
    assert_eq!(gatt::SERVICE_UUID, "6E400001-B5A3-F393-E0A9-E50E24DCCA9E");
    assert_eq!(
        gatt::RX_CHARACTERISTIC_UUID,
        "6E400002-B5A3-F393-E0A9-E50E24DCCA9E"
    );
    assert_eq!(
        gatt::TX_CHARACTERISTIC_UUID,
        "6E400003-B5A3-F393-E0A9-E50E24DCCA9E"
    );
}

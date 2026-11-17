//! `oasis ble-scan` and `oasis ble-listen`: try the real pad from the command line.

use std::process::ExitCode;
use std::time::{Duration, Instant as StdInstant};

use oasis_app::PadLink;
use oasis_ble::BleTransport;
use oasis_core::Instant;
use oasis_platform::pad_connector::{ConnectorEvent, PadConnector};
use oasis_platform::paths::OasisPaths;
use oasis_platform::ports::PadTransport;
use oasis_platform::trusted_device::TrustedDevice;

fn seconds(args: &[String], default: u64) -> Duration {
    Duration::from_secs(args.first().and_then(|s| s.parse().ok()).unwrap_or(default))
}

/// Look for pads for N seconds (default 5) and list them. Changes nothing on disk.
pub fn scan(args: &[String]) -> ExitCode {
    let mut transport = match BleTransport::new() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Bluetooth: {e}");
            return ExitCode::FAILURE;
        }
    };
    let wait = seconds(args, 5);
    println!("Scanning for {} s …", wait.as_secs());
    let trusted = OasisPaths::detect().map(|p| TrustedDevice::load(p.trusted_device()));
    match transport.scan(wait) {
        Ok(pads) if pads.is_empty() => {
            println!("No OASIS interface heard. Press an interface button to wake it and try again.");
            ExitCode::SUCCESS
        }
        Ok(pads) => {
            for pad in pads {
                let rssi = pad.rssi.map_or("?".to_string(), |r| format!("{r} dBm"));
                let trust = match &trusted {
                    Some(t) if t.id() == Some(pad.id.as_str()) => "  (the trusted pad)",
                    Some(t) if t.has_trusted_device() => "  (not the trusted pad)",
                    _ => "",
                };
                println!("{}  id {}  {}{}", pad.name, pad.id, rssi, trust);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Scan failed: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Connect to the trusted pad (or the first one, if none is trusted yet) and print what it
/// sends for N seconds (default 30). Press the buttons.
pub fn listen(args: &[String]) -> ExitCode {
    let transport = match BleTransport::new() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Bluetooth: {e}");
            return ExitCode::FAILURE;
        }
    };
    let Some(paths) = OasisPaths::detect() else {
        eprintln!("No home folder found on this machine.");
        return ExitCode::FAILURE;
    };
    let mut connector = PadConnector::new(transport, TrustedDevice::load(paths.trusted_device()));
    let mut link = PadLink::new();
    let total = seconds(args, 30);
    let start = StdInstant::now();
    println!("Listening for {} s. Press the OASIS interface buttons …", total.as_secs());

    while start.elapsed() < total {
        let now = Instant::from_millis(start.elapsed().as_millis() as i64);
        for event in connector.poll(now) {
            match event {
                ConnectorEvent::Status(s) => println!("[{:>6} ms] status: {}", now.0 / 1_000_000, s.as_str()),
                ConnectorEvent::UnavailableReason(r) => println!("           {r}"),
                ConnectorEvent::Connected { name } => println!("           connected to {name}"),
                ConnectorEvent::Disconnected => {
                    println!("           link lost");
                    println!("           {:?}", link.on_disconnected());
                }
                ConnectorEvent::Rssi(r) => println!("           heard at {r} dBm"),
                ConnectorEvent::Notification(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    println!("[{:>6} ms] pad says {:?}", now.0 / 1_000_000, text.trim());
                    let (events, _) = link.on_notification(&bytes, now);
                    for e in events {
                        println!("           {e:?}");
                    }
                }
            }
        }
        if !connector.is_connected() {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    let t = link.telemetry();
    println!(
        "Done. battery {:?}%  charging {}  latency {:?} ms",
        t.battery_percent, t.is_charging, t.link_latency_ms
    );
    ExitCode::SUCCESS
}

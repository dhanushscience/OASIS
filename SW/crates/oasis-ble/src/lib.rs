//! # oasis-ble
//!
//! [`BleTransport`] is the real `PadTransport`: it finds the pad over Bluetooth LE, connects,
//! subscribes to the Nordic UART TX characteristic and writes commands to RX. Port of the
//! Bluetooth half of `Services/BleManager.cs` (WinRT there, `btleplug` here, so the same code
//! serves Windows, macOS and Linux). *When* to connect, back off or ping is not decided here;
//! that is `oasis_platform::pad_connector::PadConnector`.
//!
//! `btleplug` is async. A small tokio runtime lives inside the transport and every call blocks
//! on it, so callers see the plain blocking `PadTransport` trait.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use btleplug::api::{
    Central, CharPropFlags, Characteristic, Manager as _, Peripheral as _, ScanFilter, WriteType,
};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures::StreamExt;
use oasis_core::pad_connect::DEVICE_OPEN_TIMEOUT;
use oasis_platform::ports::{PadInfo, PadTransport, PlatformError, PlatformResult};
use oasis_protocol::gatt::{
    is_oasis_ble_name, DEVICE_NAME, RX_CHARACTERISTIC_UUID, SERVICE_UUID, TX_CHARACTERISTIC_UUID,
};
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;
use uuid::Uuid;

/// Attempts at finding the UART characteristics after connecting (C# tries 4 times).
const GATT_ATTEMPTS: u32 = 4;

struct Link {
    peripheral: Peripheral,
    rx: Characteristic,
    /// Cleared when the notification stream ends, which is how a dropped link shows up.
    alive: Arc<AtomicBool>,
    forwarder: JoinHandle<()>,
}

pub struct BleTransport {
    runtime: Runtime,
    adapter: Adapter,
    known: HashMap<String, Peripheral>,
    link: Option<Link>,
    notifications: Receiver<Vec<u8>>,
    sender: Sender<Vec<u8>>,
}

impl BleTransport {
    /// Fails with a sentence for the status panel if this computer has no Bluetooth adapter.
    pub fn new() -> PlatformResult<BleTransport> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_time()
            .build()
            .map_err(|e| PlatformError::Io(e.to_string()))?;
        let adapter = runtime.block_on(async {
            let manager = Manager::new().await.map_err(map_error)?;
            let adapters = manager.adapters().await.map_err(map_error)?;
            adapters.into_iter().next().ok_or_else(|| {
                PlatformError::Unavailable("This computer has no Bluetooth adapter.".into())
            })
        })?;
        let (sender, notifications) = mpsc::channel();
        Ok(BleTransport { runtime, adapter, known: HashMap::new(), link: None, notifications, sender })
    }
}

fn service_uuid() -> Uuid {
    Uuid::parse_str(SERVICE_UUID).expect("SERVICE_UUID is a valid UUID")
}

/// The pad id: the Bluetooth address as a decimal number, which is what the C# app stored in
/// `trusted_device.json`.
pub fn pad_id(address: btleplug::api::BDAddr) -> String {
    u64::from(address).to_string()
}

fn map_error(e: btleplug::Error) -> PlatformError {
    match e {
        btleplug::Error::PermissionDenied => PlatformError::PermissionNeeded(
            "OASIS needs permission to use Bluetooth. Allow it in the system settings.".into(),
        ),
        other => PlatformError::Unavailable(other.to_string()),
    }
}

/// Whether an advertisement is one of ours: an OASIS name, or the Nordic UART service.
fn is_pad(name: Option<&str>, services: &[Uuid]) -> bool {
    is_oasis_ble_name(name) || services.contains(&service_uuid())
}

/// Whether the adapter has heard one of our pads so far.
async fn pad_heard(adapter: &Adapter) -> bool {
    for peripheral in adapter.peripherals().await.unwrap_or_default() {
        if let Ok(Some(props)) = peripheral.properties().await {
            let name = props.local_name.or(props.advertisement_name);
            if is_pad(name.as_deref(), &props.services) {
                return true;
            }
        }
    }
    false
}

impl PadTransport for BleTransport {
    fn scan(&mut self, timeout: Duration) -> PlatformResult<Vec<PadInfo>> {
        let adapter = &self.adapter;
        let found = self.runtime.block_on(async {
            // Filter by the UART service so the OS only reports pads, not every earbud in range.
            adapter
                .start_scan(ScanFilter { services: vec![service_uuid()] })
                .await
                .map_err(map_error)?;
            // Stop as soon as a pad is heard, not after the full time: connecting starts that much sooner.
            let deadline = tokio::time::Instant::now() + timeout;
            loop {
                tokio::time::sleep(Duration::from_millis(50)).await;
                if tokio::time::Instant::now() >= deadline || pad_heard(adapter).await {
                    break;
                }
            }
            let peripherals = adapter.peripherals().await.map_err(map_error);
            let _ = adapter.stop_scan().await;

            let mut found = Vec::new();
            for peripheral in peripherals? {
                let Ok(Some(props)) = peripheral.properties().await else { continue };
                let name = props.local_name.clone().or(props.advertisement_name.clone());
                if !is_pad(name.as_deref(), &props.services) {
                    continue;
                }
                found.push((peripheral, name, props.rssi, props.address));
            }
            Ok::<_, PlatformError>(found)
        })?;

        let mut pads = Vec::with_capacity(found.len());
        for (peripheral, name, rssi, address) in found {
            let id = pad_id(address);
            self.known.insert(id.clone(), peripheral);
            pads.push(PadInfo { id, name: name.unwrap_or_else(|| DEVICE_NAME.to_string()), rssi });
        }
        Ok(pads)
    }

    fn connect(&mut self, pad: &PadInfo) -> PlatformResult<()> {
        self.disconnect();
        let peripheral = self
            .known
            .get(&pad.id)
            .cloned()
            .ok_or_else(|| PlatformError::Unavailable("That OASIS interface is no longer in range.".into()))?;

        let (rx, tx, stream) = self.runtime.block_on(async {
            tokio::time::timeout(DEVICE_OPEN_TIMEOUT, peripheral.connect())
                .await
                .map_err(|_| {
                    PlatformError::Unavailable(
                        "Windows could not connect to the OASIS interface. Press an interface button once, wait a few seconds, and it will retry."
                            .into(),
                    )
                })?
                .map_err(map_error)?;

            let (rx_uuid, tx_uuid) = (uuid_of(RX_CHARACTERISTIC_UUID), uuid_of(TX_CHARACTERISTIC_UUID));
            let mut last_error = PlatformError::Unavailable("The OASIS interface's UART service was not found.".into());
            for attempt in 1..=GATT_ATTEMPTS {
                if attempt > 1 {
                    tokio::time::sleep(Duration::from_millis(200 * u64::from(attempt))).await;
                }
                if let Err(e) = peripheral.discover_services().await {
                    last_error = map_error(e);
                    continue;
                }
                let chars = peripheral.characteristics();
                let find = |u: Uuid| chars.iter().find(|c| c.uuid == u).cloned();
                let (Some(rx), Some(tx)) = (find(rx_uuid), find(tx_uuid)) else { continue };

                // Subscribe before returning so the first BAT: message is not missed.
                let stream = peripheral.notifications().await.map_err(map_error)?;
                match peripheral.subscribe(&tx).await {
                    Ok(()) => return Ok((rx, tx, stream)),
                    Err(e) => last_error = map_error(e),
                }
            }
            let _ = peripheral.disconnect().await;
            Err(last_error)
        })?;

        let alive = Arc::new(AtomicBool::new(true));
        let forwarder = {
            let (alive, sender, tx_uuid) = (Arc::clone(&alive), self.sender.clone(), tx.uuid);
            let mut stream = stream;
            self.runtime.spawn(async move {
                while let Some(n) = stream.next().await {
                    if n.uuid == tx_uuid {
                        let _ = sender.send(n.value);
                    }
                }
                alive.store(false, Ordering::SeqCst);
            })
        };
        self.link = Some(Link { peripheral, rx, alive, forwarder });
        Ok(())
    }

    fn disconnect(&mut self) {
        if let Some(link) = self.link.take() {
            link.forwarder.abort();
            let peripheral = link.peripheral;
            let _ = self.runtime.block_on(peripheral.disconnect());
        }
        while self.notifications.try_recv().is_ok() {}
    }

    fn is_connected(&self) -> bool {
        self.link.as_ref().is_some_and(|link| {
            link.alive.load(Ordering::SeqCst)
                && self.runtime.block_on(link.peripheral.is_connected()).unwrap_or(false)
        })
    }

    fn write(&mut self, wire: &str) -> PlatformResult<()> {
        let link = self
            .link
            .as_ref()
            .ok_or_else(|| PlatformError::Unavailable("The OASIS interface is not connected.".into()))?;
        let write_type = write_type(link.rx.properties);
        self.runtime
            .block_on(link.peripheral.write(&link.rx, wire.as_bytes(), write_type))
            .map_err(map_error)
    }

    fn next_notification(&mut self, timeout: Duration) -> PlatformResult<Option<Vec<u8>>> {
        Ok(self.notifications.recv_timeout(timeout).ok())
    }
}

impl Drop for BleTransport {
    fn drop(&mut self) {
        self.disconnect();
    }
}

fn uuid_of(s: &str) -> Uuid {
    Uuid::parse_str(s).expect("GATT UUIDs are valid")
}

/// Without response when the pad allows it (faster), as in C#.
fn write_type(properties: CharPropFlags) -> WriteType {
    if properties.contains(CharPropFlags::WRITE_WITHOUT_RESPONSE) {
        WriteType::WithoutResponse
    } else {
        WriteType::WithResponse
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use btleplug::api::BDAddr;

    #[test]
    fn the_pad_id_is_the_address_in_decimal_like_the_csharp_trust_file() {
        // The address stored by the C# app: 189285357480946.
        let address = BDAddr::try_from(189_285_357_480_946u64).unwrap();
        assert_eq!(pad_id(address), "189285357480946");
    }

    #[test]
    fn a_device_is_ours_by_name_or_by_service() {
        assert!(is_pad(Some("OASIS_INTERFACE"), &[]));
        assert!(is_pad(None, &[service_uuid()]));
        assert!(!is_pad(Some("Headphones"), &[Uuid::nil()]));
        assert!(!is_pad(None, &[]));
    }

    #[test]
    fn writes_go_without_response_when_the_pad_allows_it() {
        assert_eq!(write_type(CharPropFlags::WRITE_WITHOUT_RESPONSE | CharPropFlags::WRITE), WriteType::WithoutResponse);
        assert_eq!(write_type(CharPropFlags::WRITE), WriteType::WithResponse);
    }

    #[test]
    fn a_missing_permission_says_what_to_do() {
        let e = map_error(btleplug::Error::PermissionDenied);
        assert!(matches!(e, PlatformError::PermissionNeeded(m) if m.contains("permission")));
    }
}

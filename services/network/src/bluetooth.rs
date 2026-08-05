// bluetooth.rs — Userspace Bluetooth proxy via FreeDesktop BlueZ / D-Bus
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use tracing::{info, warn};
use zbus::{zvariant, Connection};
use zethra_hal::{BluetoothAdapter, BluetoothAdapterState, BluetoothDevice, BluetoothHal};

/// Represents an asynchronous D-Bus proxy interface to FreeDesktop BlueZ daemons.
/// Falls back to an in-memory simulated state machine when D-Bus is unavailable
/// or during automated CI validation.
pub struct BluezProxy {
    dbus_connection: Option<Connection>,
    simulated: bool,
    simulated_state: SimulatedBluetoothState,
}

#[derive(Debug, Clone)]
struct SimulatedBluetoothState {
    adapters: Vec<BluetoothAdapter>,
    devices: Vec<BluetoothDevice>,
}

impl Default for SimulatedBluetoothState {
    fn default() -> Self {
        Self {
            adapters: vec![BluetoothAdapter {
                adapter_id: "hci0".to_string(),
                address: "00:11:22:33:44:55".to_string(),
                name: "Zethra-HCI".to_string(),
                state: BluetoothAdapterState::PoweredOn,
                discoverable: true,
            }],
            devices: vec![
                BluetoothDevice {
                    address: "AA:BB:CC:DD:EE:01".to_string(),
                    name: "Zethra-Pencil".to_string(),
                    rssi: -45,
                    is_connected: false,
                },
                BluetoothDevice {
                    address: "AA:BB:CC:DD:EE:02".to_string(),
                    name: "Audio-Pods".to_string(),
                    rssi: -62,
                    is_connected: false,
                },
            ],
        }
    }
}

impl BluezProxy {
    /// Try to initialize a live D-Bus system connection to BlueZ.
    pub async fn try_new() -> Result<Self> {
        match Connection::system().await {
            Ok(conn) => {
                info!("✓ Connected to FreeDesktop D-Bus System Bus for BlueZ proxy");
                Ok(Self {
                    dbus_connection: Some(conn),
                    simulated: false,
                    simulated_state: SimulatedBluetoothState::default(),
                })
            }
            Err(e) => {
                warn!(error = %e, "D-Bus system bus unreachable for BlueZ");
                Err(anyhow::anyhow!("D-Bus system bus unreachable: {}", e))
            }
        }
    }

    /// Initialize a simulated BlueZ proxy directly for zero-hardware CI validation.
    pub fn new_simulated() -> Self {
        info!("Initializing Simulated BlueZ Proxy for deterministic CI validation");
        Self {
            dbus_connection: None,
            simulated: true,
            simulated_state: SimulatedBluetoothState::default(),
        }
    }

    /// Try initializing D-Bus, gracefully reverting to simulation if D-Bus is unavailable.
    pub async fn try_new_with_fallback() -> Self {
        match Self::try_new().await {
            Ok(proxy) => proxy,
            Err(e) => {
                warn!(
                    error = %e,
                    "Failed to bind BlueZ D-Bus; switching to simulated test fallback"
                );
                Self::new_simulated()
            }
        }
    }
}

#[async_trait::async_trait]
impl BluetoothHal for BluezProxy {
    async fn list_adapters(&mut self) -> Result<Vec<BluetoothAdapter>> {
        if self.simulated || self.dbus_connection.is_none() {
            info!("Simulated BlueZ: enumerating local Bluetooth adapters");
            return Ok(self.simulated_state.adapters.clone());
        }

        let conn = self
            .dbus_connection
            .as_ref()
            .context("D-Bus connection lost")?;
        let proxy =
            zbus::Proxy::new(conn, "org.bluez", "/", "org.freedesktop.DBus.ObjectManager").await?;

        // Inspect managed objects for org.bluez.Adapter1 interfaces
        type ManagedObjects = std::collections::HashMap<
            zvariant::OwnedObjectPath,
            std::collections::HashMap<
                String,
                std::collections::HashMap<String, zvariant::OwnedValue>,
            >,
        >;

        let mut adapters = Vec::new();
        match proxy
            .call::<_, _, ManagedObjects>("GetManagedObjects", &())
            .await
        {
            Ok(objects) => {
                for (path, interfaces) in objects {
                    if let Some(props) = interfaces.get("org.bluez.Adapter1") {
                        let path_str = path.as_str();
                        let adapter_id = path_str.rsplit('/').next().unwrap_or("hci0").to_string();
                        let address = props
                            .get("Address")
                            .and_then(|v| v.downcast_ref::<String>().ok())
                            .unwrap_or_else(|| "00:00:00:00:00:00".to_string());
                        let name = props
                            .get("Name")
                            .and_then(|v| v.downcast_ref::<String>().ok())
                            .unwrap_or_else(|| adapter_id.clone());
                        let powered = props
                            .get("Powered")
                            .and_then(|v| v.downcast_ref::<bool>().ok())
                            .unwrap_or(false);
                        let discoverable = props
                            .get("Discoverable")
                            .and_then(|v| v.downcast_ref::<bool>().ok())
                            .unwrap_or(false);

                        let state = if powered {
                            BluetoothAdapterState::PoweredOn
                        } else {
                            BluetoothAdapterState::PoweredOff
                        };

                        adapters.push(BluetoothAdapter {
                            adapter_id,
                            address,
                            name,
                            state,
                            discoverable,
                        });
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "Failed to query BlueZ managed objects; returning empty adapter list");
            }
        }

        info!(
            count = adapters.len(),
            "D-Bus BlueZ: enumerated local adapters"
        );
        Ok(adapters)
    }

    async fn set_powered(&mut self, adapter_id: &str, powered: bool) -> Result<()> {
        if self.simulated || self.dbus_connection.is_none() {
            if let Some(adapter) = self
                .simulated_state
                .adapters
                .iter_mut()
                .find(|a| a.adapter_id == adapter_id)
            {
                adapter.state = if powered {
                    BluetoothAdapterState::PoweredOn
                } else {
                    BluetoothAdapterState::PoweredOff
                };
                info!(
                    adapter_id = %adapter_id,
                    powered,
                    "Simulated BlueZ: updated adapter powered state"
                );
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Simulated adapter ID not found: {}",
                    adapter_id
                ))
            }
        } else {
            let conn = self
                .dbus_connection
                .as_ref()
                .context("D-Bus connection lost")?;
            let path = format!("/org/bluez/{}", adapter_id);
            let adapter_proxy =
                zbus::Proxy::new(conn, "org.bluez", path.as_str(), "org.bluez.Adapter1").await?;

            adapter_proxy
                .set_property("Powered", zvariant::Value::from(powered))
                .await?;
            info!(adapter_id = %adapter_id, powered, "D-Bus BlueZ: set adapter powered state");
            Ok(())
        }
    }

    async fn scan_devices(&mut self, adapter_id: &str) -> Result<Vec<BluetoothDevice>> {
        if self.simulated || self.dbus_connection.is_none() {
            info!(adapter_id = %adapter_id, "Simulated BlueZ: returning simulated peripheral devices");
            return Ok(self.simulated_state.devices.clone());
        }

        let conn = self
            .dbus_connection
            .as_ref()
            .context("D-Bus connection lost")?;
        let path = format!("/org/bluez/{}", adapter_id);
        let adapter_proxy =
            zbus::Proxy::new(conn, "org.bluez", path.as_str(), "org.bluez.Adapter1").await?;

        // Initiate device discovery
        let _ = adapter_proxy.call::<_, _, ()>("StartDiscovery", &()).await;

        // In a live scan, we inspect org.bluez.Device1 objects via ObjectManager
        let om_proxy =
            zbus::Proxy::new(conn, "org.bluez", "/", "org.freedesktop.DBus.ObjectManager").await?;

        type ManagedObjects = std::collections::HashMap<
            zvariant::OwnedObjectPath,
            std::collections::HashMap<
                String,
                std::collections::HashMap<String, zvariant::OwnedValue>,
            >,
        >;

        let mut devices = Vec::new();
        if let Ok(objects) = om_proxy
            .call::<_, _, ManagedObjects>("GetManagedObjects", &())
            .await
        {
            for (_, interfaces) in objects {
                if let Some(props) = interfaces.get("org.bluez.Device1") {
                    let address = props
                        .get("Address")
                        .and_then(|v| v.downcast_ref::<String>().ok())
                        .unwrap_or_else(|| "00:00:00:00:00:00".to_string());
                    let name = props
                        .get("Name")
                        .and_then(|v| v.downcast_ref::<String>().ok())
                        .unwrap_or_else(|| "Unknown Device".to_string());
                    let rssi = props
                        .get("RSSI")
                        .and_then(|v| v.downcast_ref::<i16>().ok())
                        .map(|v| v as i32)
                        .unwrap_or(-80);
                    let is_connected = props
                        .get("Connected")
                        .and_then(|v| v.downcast_ref::<bool>().ok())
                        .unwrap_or(false);

                    devices.push(BluetoothDevice {
                        address,
                        name,
                        rssi,
                        is_connected,
                    });
                }
            }
        }

        info!(adapter_id = %adapter_id, count = devices.len(), "D-Bus BlueZ: scanned peripheral devices");
        Ok(devices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_simulated_bluez_lifecycle() {
        let mut proxy = BluezProxy::new_simulated();

        // 1. Enumerate adapters
        let adapters = proxy
            .list_adapters()
            .await
            .expect("Failed to list adapters");
        assert_eq!(adapters.len(), 1);
        assert_eq!(adapters[0].adapter_id, "hci0");
        assert_eq!(adapters[0].state, BluetoothAdapterState::PoweredOn);

        // 2. Scan devices
        let devices = proxy
            .scan_devices("hci0")
            .await
            .expect("Failed to scan devices");
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].name, "Zethra-Pencil");

        // 3. Toggle power off
        proxy
            .set_powered("hci0", false)
            .await
            .expect("Failed to power off adapter");
        let adapters_after = proxy
            .list_adapters()
            .await
            .expect("Failed to query adapters");
        assert_eq!(adapters_after[0].state, BluetoothAdapterState::PoweredOff);
    }
}

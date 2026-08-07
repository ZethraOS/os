// zethra-networkd — ZethraOS Network Manager
// SPDX-License-Identifier: Apache-2.0

mod bluetooth;
mod dns;
mod netlink;
mod wifi;

use crate::bluetooth::BluezProxy;
use crate::dns::DnsResolver;
use crate::netlink::{LinuxNetworkDriver, NetlinkMonitor};
use crate::wifi::WifiManager;
use anyhow::Result;
use tracing::info;
use zethra_hal::{BluetoothAdapter, BluetoothDevice, BluetoothHal, NetworkHal, NetworkInterface};

pub struct NetworkOrchestrator {
    wifi: WifiManager,
    net_driver: Box<dyn NetworkHal + Send + Sync>,
    bt_driver: Box<dyn BluetoothHal + Send + Sync>,
}

impl Default for NetworkOrchestrator {
    fn default() -> Self {
        Self::new_with_drivers(
            Box::new(LinuxNetworkDriver::new_simulated()),
            Box::new(BluezProxy::new_simulated()),
        )
    }
}

impl NetworkOrchestrator {
    pub async fn new() -> Result<Self> {
        let net_driver = Box::new(LinuxNetworkDriver::try_new());
        let bt_driver = Box::new(BluezProxy::try_new_with_fallback().await);
        Ok(Self {
            wifi: WifiManager::new("wlan0"),
            net_driver,
            bt_driver,
        })
    }

    pub fn new_with_drivers(
        net_driver: Box<dyn NetworkHal + Send + Sync>,
        bt_driver: Box<dyn BluetoothHal + Send + Sync>,
    ) -> Self {
        Self {
            wifi: WifiManager::new("wlan0"),
            net_driver,
            bt_driver,
        }
    }

    pub async fn discover_interfaces(&mut self) -> Result<Vec<NetworkInterface>> {
        self.net_driver.list_interfaces().await
    }

    pub async fn discover_adapters(&mut self) -> Result<Vec<BluetoothAdapter>> {
        self.bt_driver.list_adapters().await
    }

    pub async fn scan_wireless(&mut self, interface: &str) -> Result<Vec<String>> {
        self.net_driver.scan_wifi(interface).await
    }

    pub async fn scan_bluetooth(&mut self, adapter_id: &str) -> Result<Vec<BluetoothDevice>> {
        self.bt_driver.scan_devices(adapter_id).await
    }

    pub async fn toggle_bluetooth_adapter(
        &mut self,
        adapter_id: &str,
        powered: bool,
    ) -> Result<()> {
        self.bt_driver.set_powered(adapter_id, powered).await
    }

    pub async fn toggle_network_interface(&mut self, interface: &str, up: bool) -> Result<()> {
        self.net_driver.set_interface_state(interface, up).await
    }

    pub async fn run(&self) -> Result<()> {
        info!("ZethraOS Network Orchestrator starting");

        // Step 1: Initialize DNS (allow simulated failure on unprivileged hosts/CI)
        let _ = DnsResolver::apply_system_config(&["1.1.1.1", "8.8.8.8"]).await;

        // Step 2: Spawn Netlink Monitor
        tokio::spawn(async move {
            let _ = NetlinkMonitor::run().await;
        });

        // Step 3: Spawn Wi-Fi Monitor
        let wifi = self.wifi.clone();
        tokio::spawn(async move {
            let _ = wifi.run_monitoring().await;
        });

        // Keep running in normal execution mode
        if !std::env::var("ZETHRA_CI_TEST_MODE")
            .map(|v| v == "1")
            .unwrap_or(false)
        {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
            }
        }

        Ok(())
    }
}

// WifiManager must be Clone so it can be moved into a spawned monitoring task.
// The clone preserves the original interface name rather than resetting to "wlan0".
impl Clone for WifiManager {
    fn clone(&self) -> Self {
        Self::new(&self.interface)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()))
        .init();

    let orchestrator = NetworkOrchestrator::new().await?;
    orchestrator.run().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use zethra_hal::{BluetoothAdapterState, NetworkInterfaceState};

    #[tokio::test]
    async fn test_orchestrator_discovery_and_segregation() {
        let mut orchestrator = NetworkOrchestrator::default();

        // 1. Verify LAN/PAN discovery
        let interfaces = orchestrator
            .discover_interfaces()
            .await
            .expect("Failed interface discovery");
        assert_eq!(interfaces.len(), 2);
        assert_eq!(interfaces[0].name, "eth0");
        assert_eq!(interfaces[1].name, "wlan0");
        assert!(interfaces[1].is_wireless);

        // 2. Verify Wi-Fi SSID broadcast scan
        let networks = orchestrator
            .scan_wireless("wlan0")
            .await
            .expect("Failed wireless scan");
        assert!(networks.contains(&"Zethra-Mesh-5G".to_string()));

        // 3. Verify BlueZ adapter enumeration
        let adapters = orchestrator
            .discover_adapters()
            .await
            .expect("Failed adapter discovery");
        assert_eq!(adapters.len(), 1);
        assert_eq!(adapters[0].adapter_id, "hci0");
        assert_eq!(adapters[0].state, BluetoothAdapterState::PoweredOn);

        // 4. Verify Bluetooth device scan
        let bt_devices = orchestrator
            .scan_bluetooth("hci0")
            .await
            .expect("Failed Bluetooth scan");
        assert_eq!(bt_devices.len(), 2);

        // 5. Toggle link operational states
        orchestrator
            .toggle_bluetooth_adapter("hci0", false)
            .await
            .expect("Failed to turn off Bluetooth");
        let adapters_off = orchestrator.discover_adapters().await.unwrap();
        assert_eq!(adapters_off[0].state, BluetoothAdapterState::PoweredOff);

        orchestrator
            .toggle_network_interface("eth0", false)
            .await
            .expect("Failed to turn off eth0");
        let ifaces_off = orchestrator.discover_interfaces().await.unwrap();
        assert_eq!(ifaces_off[0].state, NetworkInterfaceState::Down);

        // 6. Confirm WWAN interface operational barrier (segregation from Telephony)
        assert!(orchestrator
            .toggle_network_interface("wwan0", true)
            .await
            .is_err());
        assert!(orchestrator
            .toggle_network_interface("rmnet0", false)
            .await
            .is_err());

        // 7. Confirm newly-documented WWAN prefixes are also rejected
        assert!(orchestrator
            .toggle_network_interface("cdc-wdm0", true)
            .await
            .is_err());
        assert!(orchestrator
            .toggle_network_interface("pdp0", false)
            .await
            .is_err());
        assert!(orchestrator
            .toggle_network_interface("mbim0", true)
            .await
            .is_err());
    }

    /// Regression test: WifiManager::clone() must preserve the original interface name.
    /// Before the fix, clone() always hard-coded "wlan0", silently routing the monitoring
    /// task to the wrong wpa_supplicant socket.
    #[test]
    fn test_wifi_manager_clone_preserves_interface() {
        let original = WifiManager::new("wlp2s0");
        let cloned = original.clone();

        assert_eq!(
            cloned.interface, "wlp2s0",
            "Cloned WifiManager must preserve the original interface name, not reset to 'wlan0'"
        );
        assert_eq!(
            cloned.socket_path, "/var/run/wpa_supplicant/wlp2s0",
            "Cloned WifiManager must derive the socket path from the original interface"
        );
    }

    /// Regression test: WifiManager::clone() for the default "wlan0" interface still works.
    #[test]
    fn test_wifi_manager_clone_wlan0_still_works() {
        let original = WifiManager::new("wlan0");
        let cloned = original.clone();
        assert_eq!(cloned.interface, "wlan0");
        assert_eq!(cloned.socket_path, "/var/run/wpa_supplicant/wlan0");
    }
}

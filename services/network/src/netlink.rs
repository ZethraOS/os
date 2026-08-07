// netlink.rs — Netlink monitor & LAN/PAN driver for ZethraOS Network
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
#[cfg(target_os = "linux")]
use futures_util::stream::StreamExt;
#[cfg(target_os = "linux")]
use rtnetlink::new_connection;
use tracing::{info, warn};
use zethra_hal::{NetworkHal, NetworkInterface, NetworkInterfaceState};

pub struct NetlinkMonitor;

impl NetlinkMonitor {
    #[cfg(target_os = "linux")]
    pub async fn run() -> Result<()> {
        let (conn, _handle, mut messages) = new_connection()?;
        tokio::spawn(conn);

        info!("Netlink monitor started (Linux)");

        while let Some((message, _)) = messages.next().await {
            use netlink_packet_core::NetlinkPayload;
            if let NetlinkPayload::InnerMessage(msg) = message.payload {
                info!("Netlink event: {:?}", msg);
            }
        }
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    pub async fn run() -> Result<()> {
        warn!("Netlink monitor is only supported on Linux. Running stub.");
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
        }
    }
}

/// Linux network interface driver utilizing rtnetlink.
/// Enforces architectural separation of responsibility by strictly managing LAN/PAN links
/// (e.g. eth0, wlan0) while filtering out cellular WWAN interfaces owned by zethra-telephony.
/// Filtered WWAN prefixes: wwan*, rmnet*, ccmni*, cdc-wdm*, pdp*, mbim*, qmi*.
pub struct LinuxNetworkDriver {
    simulated: bool,
    simulated_interfaces: Vec<NetworkInterface>,
}

impl Default for LinuxNetworkDriver {
    fn default() -> Self {
        Self::new_simulated()
    }
}

impl LinuxNetworkDriver {
    /// Attempt to initialize live netlink interaction. Reverts to simulated mode if unreachable or non-Linux.
    pub fn try_new() -> Self {
        #[cfg(target_os = "linux")]
        {
            // Verify if netlink sockets can be opened
            match rtnetlink::new_connection() {
                Ok(_) => {
                    info!("✓ Initialized Linux rtnetlink connection for NetworkHal");
                    return Self {
                        simulated: false,
                        simulated_interfaces: Vec::new(),
                    };
                }
                Err(e) => {
                    warn!(error = %e, "Failed to initialize rtnetlink socket; falling back to simulated NetworkHal");
                }
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            info!("Non-Linux host detected; running LinuxNetworkDriver in simulated fallback mode");
        }
        Self::new_simulated()
    }

    /// Explicitly construct a simulated network driver for deterministic CI validation.
    pub fn new_simulated() -> Self {
        info!("Initializing Simulated LinuxNetworkDriver for zero-hardware CI testing");
        Self {
            simulated: true,
            simulated_interfaces: vec![
                NetworkInterface {
                    name: "eth0".to_string(),
                    mac_address: "02:00:00:00:00:01".to_string(),
                    state: NetworkInterfaceState::Up,
                    ip_addresses: vec!["192.168.1.100/24".to_string()],
                    is_wireless: false,
                },
                NetworkInterface {
                    name: "wlan0".to_string(),
                    mac_address: "02:00:00:00:00:02".to_string(),
                    state: NetworkInterfaceState::Up,
                    ip_addresses: vec!["10.0.0.50/24".to_string()],
                    is_wireless: true,
                },
            ],
        }
    }

    /// Returns true if `name` corresponds to a cellular WWAN data link owned by zethra-telephony.
    ///
    /// Covered prefixes match common Linux kernel naming conventions for USB, PCIe, and platform
    /// modem data interfaces:
    /// - `wwan*`     — kernel WWAN subsystem interfaces (kernel ≥ 5.14)
    /// - `rmnet*`    — Qualcomm IPA/rmnet interfaces (Android/upstream)
    /// - `ccmni*`    — MediaTek CCMNI (ccci net) interfaces
    /// - `cdc-wdm*`  — CDC WDM QMI/MBIM control interfaces (USB modems)
    /// - `pdp*`      — PDP context data interfaces (older MediaTek)
    /// - `mbim*`     — MBIM data interfaces
    /// - `qmi*`      — QMI-managed data interfaces
    fn is_cellular_wwan(name: &str) -> bool {
        name.starts_with("wwan")
            || name.starts_with("rmnet")
            || name.starts_with("ccmni")
            || name.starts_with("cdc-wdm")
            || name.starts_with("pdp")
            || name.starts_with("mbim")
            || name.starts_with("qmi")
    }
}

#[async_trait::async_trait]
impl NetworkHal for LinuxNetworkDriver {
    async fn list_interfaces(&mut self) -> Result<Vec<NetworkInterface>> {
        if self.simulated {
            info!("Simulated LinuxNetworkDriver: enumerating LAN/PAN interfaces");
            return Ok(self.simulated_interfaces.clone());
        }

        #[cfg(target_os = "linux")]
        {
            let (conn, handle, _) = new_connection()?;
            tokio::spawn(conn);

            let mut links = handle.link().get().execute();
            let mut interfaces = Vec::new();

            while let Some(link) = links.next().await {
                match link {
                    Ok(msg) => {
                        let mut name = String::new();
                        let mut mac_address = "00:00:00:00:00:00".to_string();
                        let mut state = NetworkInterfaceState::Unknown;

                        for attr in msg.attributes.into_iter() {
                            use netlink_packet_route::link::{LinkAttribute, State};
                            match attr {
                                LinkAttribute::IfName(n) => name = n,
                                LinkAttribute::Address(addr) => {
                                    mac_address = addr
                                        .iter()
                                        .map(|b| format!("{:02x}", b))
                                        .collect::<Vec<_>>()
                                        .join(":");
                                }
                                LinkAttribute::OperState(s) => {
                                    state = match s {
                                        State::Up => NetworkInterfaceState::Up,
                                        State::Down => NetworkInterfaceState::Down,
                                        State::Testing => NetworkInterfaceState::Testing,
                                        _ => NetworkInterfaceState::Unknown,
                                    };
                                }
                                _ => {}
                            }
                        }

                        // Enforce clean boundary segregation: ignore cellular data WWAN links
                        if Self::is_cellular_wwan(&name) {
                            info!(interface = %name, "Ignoring WWAN cellular interface (managed by zethra-telephony)");
                            continue;
                        }

                        // Ignore loopback for external LAN/PAN enumeration
                        if name == "lo" || name.is_empty() {
                            continue;
                        }

                        let is_wireless = name.starts_with("wl");
                        interfaces.push(NetworkInterface {
                            name,
                            mac_address,
                            state,
                            ip_addresses: Vec::new(),
                            is_wireless,
                        });
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to inspect rtnetlink interface attribute");
                    }
                }
            }

            info!(
                count = interfaces.len(),
                "rtnetlink: enumerated LAN/PAN interfaces"
            );
            return Ok(interfaces);
        }

        #[cfg(not(target_os = "linux"))]
        {
            Ok(self.simulated_interfaces.clone())
        }
    }

    async fn scan_wifi(&mut self, interface: &str) -> Result<Vec<String>> {
        if self.simulated {
            info!(
                interface = %interface,
                "Simulated LinuxNetworkDriver: returning simulated Wi-Fi ESSID broadcast list"
            );
            return Ok(vec![
                "Zethra-Mesh-5G".to_string(),
                "Guest-Net".to_string(),
                "IoT-Secure".to_string(),
            ]);
        }

        // In live mode, only recognised wireless interface name prefixes are accepted.
        // Returning simulated data silently for unknown names would hide configuration
        // errors and make field debugging very difficult.
        if !interface.starts_with("wl") {
            anyhow::bail!(
                "scan_wifi: '{}' is not a recognised wireless interface name (expected 'wl*' prefix)",
                interface
            );
        }

        // Query the WPA supplicant scanning socket
        match crate::wifi::WifiScanner::get_results().await {
            Ok(res) => {
                let networks = res
                    .lines()
                    .skip(1) // Skip table header line
                    .filter_map(|l| l.split_whitespace().nth(4).map(String::from))
                    .collect();
                Ok(networks)
            }
            Err(e) => {
                warn!(error = %e, interface = %interface, "Failed to contact wpa_supplicant for Wi-Fi scan");
                Err(e)
            }
        }
    }

    async fn set_interface_state(&mut self, interface: &str, up: bool) -> Result<()> {
        if Self::is_cellular_wwan(interface) {
            anyhow::bail!("Cannot modify cellular WWAN interface '{}' from network daemon; interface is owned by zethra-telephony", interface);
        }

        if self.simulated {
            if let Some(iface) = self
                .simulated_interfaces
                .iter_mut()
                .find(|i| i.name == interface)
            {
                iface.state = if up {
                    NetworkInterfaceState::Up
                } else {
                    NetworkInterfaceState::Down
                };
                info!(
                    interface = %interface,
                    state = ?iface.state,
                    "Simulated LinuxNetworkDriver: updated interface operational state"
                );
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Simulated interface not found: {}",
                    interface
                ))
            }
        } else {
            #[cfg(target_os = "linux")]
            {
                let (conn, handle, _) = new_connection()?;
                tokio::spawn(conn);

                use netlink_packet_route::link::LinkFlags;
                let mut links = handle
                    .link()
                    .get()
                    .match_name(interface.to_string())
                    .execute();
                if let Some(Ok(mut link)) = links.next().await {
                    if up {
                        link.header.flags |= LinkFlags::Up;
                    } else {
                        link.header.flags &= !LinkFlags::Up;
                    }
                    link.header.change_mask = LinkFlags::Up;
                    handle.link().set(link).execute().await?;
                    info!(interface = %interface, up, "rtnetlink: set link administrative state");
                    Ok(())
                } else {
                    Err(anyhow::anyhow!(
                        "rtnetlink interface not found: {}",
                        interface
                    ))
                }
            }
            #[cfg(not(target_os = "linux"))]
            {
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_simulated_network_driver() {
        let mut driver = LinuxNetworkDriver::new_simulated();

        // 1. Enumerate LAN/PAN interfaces
        let ifaces = driver
            .list_interfaces()
            .await
            .expect("Failed to list interfaces");
        assert_eq!(ifaces.len(), 2);
        assert_eq!(ifaces[0].name, "eth0");
        assert_eq!(ifaces[1].name, "wlan0");

        // 2. Scan Wi-Fi ESSIDs on simulated driver (always returns simulated list)
        let wifi_essids = driver.scan_wifi("wlan0").await.expect("Failed Wi-Fi scan");
        assert_eq!(wifi_essids.len(), 3);
        assert_eq!(wifi_essids[0], "Zethra-Mesh-5G");

        // 3. Modify link state
        driver
            .set_interface_state("wlan0", false)
            .await
            .expect("Failed to bring down wlan0");
        let after_down = driver
            .list_interfaces()
            .await
            .expect("Failed to check interfaces");
        assert_eq!(after_down[1].state, NetworkInterfaceState::Down);

        // 4. Ensure WWAN segregation fails attempts to control cellular links
        assert!(driver.set_interface_state("wwan0", true).await.is_err());
        assert!(driver
            .set_interface_state("rmnet_data0", true)
            .await
            .is_err());
    }

    /// Regression test: is_cellular_wwan must cover all documented WWAN prefix variants.
    /// This test pins the filter against the interface names documented in PR #36 and the
    /// audit report. Any regression in the filter that admits a WWAN interface silently
    /// into zethra-networkd will be caught here.
    #[test]
    fn test_wwan_filter_covers_all_documented_prefixes() {
        // All of these must be recognised as WWAN and rejected by zethra-networkd.
        let wwan_cases = [
            "wwan0",
            "wwan1",
            "rmnet0",
            "rmnet_data0",
            "rmnet_data1",
            "ccmni0",
            "ccmni1",
            "cdc-wdm0",
            "cdc-wdm1",
            "pdp0",
            "pdp_data0",
            "mbim0",
            "qmi0",
            "qmi_rmnet0",
        ];
        for name in &wwan_cases {
            assert!(
                LinuxNetworkDriver::is_cellular_wwan(name),
                "'{}' must be identified as a WWAN interface",
                name
            );
        }

        // These must NOT be classified as WWAN — they are LAN/PAN interfaces.
        let lan_cases = ["eth0", "wlan0", "wlp2s0", "eno1", "enp3s0", "usb0", "lo"];
        for name in &lan_cases {
            assert!(
                !LinuxNetworkDriver::is_cellular_wwan(name),
                "'{}' must NOT be identified as a WWAN interface",
                name
            );
        }
    }

    /// Regression test: scan_wifi on a simulated driver returns data for any interface name.
    /// (Simulated mode always returns the preset list regardless of the name.)
    #[tokio::test]
    async fn test_simulated_scan_wifi_accepts_any_interface_name() {
        let mut driver = LinuxNetworkDriver::new_simulated();
        // Even a non-wl name is acceptable in simulated mode — this is intentional.
        let result = driver.scan_wifi("wifi0").await;
        assert!(
            result.is_ok(),
            "Simulated scan_wifi should succeed for any interface name"
        );
    }

    /// Regression test: set_interface_state rejects cdc-wdm and pdp prefixes (newly added).
    #[tokio::test]
    async fn test_wwan_filter_rejects_newly_added_prefixes() {
        let mut driver = LinuxNetworkDriver::new_simulated();
        assert!(
            driver.set_interface_state("cdc-wdm0", true).await.is_err(),
            "cdc-wdm0 must be rejected as a WWAN interface"
        );
        assert!(
            driver.set_interface_state("pdp0", true).await.is_err(),
            "pdp0 must be rejected as a WWAN interface"
        );
        assert!(
            driver.set_interface_state("mbim0", true).await.is_err(),
            "mbim0 must be rejected as a WWAN interface"
        );
        assert!(
            driver.set_interface_state("qmi0", true).await.is_err(),
            "qmi0 must be rejected as a WWAN interface"
        );
    }
}

// aether-networkd — AetherOS Network Manager
// SPDX-License-Identifier: Apache-2.0
//
// Manages Wi-Fi (via nl80211 netlink), mobile data, DNS, hotspot.
// Uses the kernel's nl80211 interface directly (no NetworkManager dependency).
// Exposes a Unix socket IPC for the Settings app and status bar.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::net::UnixListener;
use tokio::sync::broadcast;
use tracing::info;

// ─── Network state models ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WifiNetwork {
    pub ssid: String,
    pub bssid: String,
    pub signal_dbm: i32,
    pub frequency_mhz: u32,
    pub security: WifiSecurity,
    pub connected: bool,
    pub saved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WifiSecurity {
    Open,
    Wpa2Personal,
    Wpa3Personal,
    Enterprise,
}

impl WifiNetwork {
    pub fn signal_bars(&self) -> u8 {
        match self.signal_dbm {
            s if s >= -55 => 4,
            s if s >= -65 => 3,
            s if s >= -75 => 2,
            s if s >= -85 => 1,
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStatus {
    pub wifi_enabled: bool,
    pub wifi_connected: Option<WifiNetwork>,
    pub mobile_data_enabled: bool,
    pub mobile_data_connected: bool,
    pub airplane_mode: bool,
    pub hotspot_active: bool,
    pub vpn_connected: bool,
    pub ipv4: Option<String>,
    pub ipv6: Option<String>,
    pub dns: Vec<String>,
}

// ─── IPC protocol ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum NetworkCommand {
    GetStatus,
    ScanWifi,
    ConnectWifi {
        ssid: String,
        password: Option<String>,
    },
    DisconnectWifi,
    SetWifiEnabled {
        enabled: bool,
    },
    SetMobileData {
        enabled: bool,
    },
    SetAirplaneMode {
        enabled: bool,
    },
    StartHotspot {
        ssid: String,
        password: String,
    },
    StopHotspot,
    ForgetNetwork {
        ssid: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum NetworkEvent {
    StatusChanged {
        status: NetworkStatus,
    },
    ScanResults {
        networks: Vec<WifiNetwork>,
    },
    ConnectResult {
        success: bool,
        ssid: String,
        error: Option<String>,
    },
    Error {
        message: String,
    },
}

// ─── nl80211 backend (simplified — full impl uses neli or libnl) ───────────────

pub struct WifiBackend {
    interface: String,
    known_networks: HashMap<String, String>, // ssid → psk
}

impl WifiBackend {
    pub fn new(interface: &str) -> Self {
        Self {
            interface: interface.to_string(),
            known_networks: HashMap::new(),
        }
    }

    /// Trigger a scan via `ip` and `iw` (wrappers around nl80211)
    pub async fn scan(&self) -> Result<Vec<WifiNetwork>> {
        // Production: send NL80211_CMD_TRIGGER_SCAN via netlink socket
        // then parse NL80211_CMD_NEW_SCAN_RESULTS
        // Here we show the structure with a stub
        let networks = vec![
            WifiNetwork {
                ssid: "AetherNet_5G".into(),
                bssid: "aa:bb:cc:dd:ee:ff".into(),
                signal_dbm: -62,
                frequency_mhz: 5180,
                security: WifiSecurity::Wpa3Personal,
                connected: false,
                saved: true,
            },
            WifiNetwork {
                ssid: "HomeNetwork".into(),
                bssid: "11:22:33:44:55:66".into(),
                signal_dbm: -78,
                frequency_mhz: 2412,
                security: WifiSecurity::Wpa2Personal,
                connected: false,
                saved: false,
            },
        ];
        Ok(networks)
    }

    /// Connect to a network using wpa_supplicant or iwd
    pub async fn connect(&mut self, ssid: &str, password: Option<&str>) -> Result<()> {
        // Production: write wpa_supplicant config or use iwd D-Bus API
        // then call SIOCGIFFLAGS/SIOCSIFFLAGS to bring interface up
        if let Some(psk) = password {
            self.known_networks
                .insert(ssid.to_string(), psk.to_string());
        }
        info!(ssid, interface = %self.interface, "connecting to Wi-Fi");
        Ok(())
    }

    pub async fn disconnect(&self) -> Result<()> {
        // Production: NL80211_CMD_DEAUTHENTICATE
        info!("disconnecting Wi-Fi");
        Ok(())
    }

    /// Configure hotspot via hostapd
    pub async fn start_hotspot(&self, ssid: &str, password: &str) -> Result<()> {
        let config = format!(
            "interface={}\ndriver=nl80211\nssid={}\nhw_mode=g\nchannel=6\n\
             wpa=2\nwpa_passphrase={}\nwpa_key_mgmt=WPA-PSK\nrsn_pairwise=CCMP\n",
            self.interface, ssid, password
        );
        tokio::fs::write("/tmp/hostapd.conf", config).await?;
        // Production: start hostapd process, configure NAT/dnsmasq
        info!(ssid, "hotspot started");
        Ok(())
    }
}

// ─── DNS-over-HTTPS stub ──────────────────────────────────────────────────────

#[allow(dead_code)]
pub struct DohResolver {
    provider: String,
}

impl DohResolver {
    pub fn cloudflare() -> Self {
        Self {
            provider: "https://1.1.1.1/dns-query".into(),
        }
    }
    pub fn quad9() -> Self {
        Self {
            provider: "https://9.9.9.9/dns-query".into(),
        }
    }

    pub async fn write_resolv_conf(&self) -> Result<()> {
        // Write /etc/resolv.conf pointing to local dnsproxy
        tokio::fs::write(
            "/etc/resolv.conf",
            "# AetherOS — DNS-over-HTTPS via local proxy\nnameserver 127.0.0.53\n",
        )
        .await?;
        Ok(())
    }
}

// ─── Network daemon ────────────────────────────────────────────────────────────

#[allow(dead_code)]
pub struct NetworkDaemon {
    wifi: WifiBackend,
    status: NetworkStatus,
    event_tx: broadcast::Sender<NetworkEvent>,
}

impl NetworkDaemon {
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(32);
        Self {
            wifi: WifiBackend::new("wlan0"),
            status: NetworkStatus {
                wifi_enabled: true,
                wifi_connected: None,
                mobile_data_enabled: true,
                mobile_data_connected: false,
                airplane_mode: false,
                hotspot_active: false,
                vpn_connected: false,
                ipv4: None,
                ipv6: None,
                dns: vec!["127.0.0.53".into()],
            },
            event_tx,
        }
    }
}

impl Default for NetworkDaemon {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkDaemon {

    pub async fn handle(&mut self, cmd: NetworkCommand) -> NetworkEvent {
        match cmd {
            NetworkCommand::GetStatus => NetworkEvent::StatusChanged {
                status: self.status.clone(),
            },
            NetworkCommand::ScanWifi => match self.wifi.scan().await {
                Ok(nets) => NetworkEvent::ScanResults { networks: nets },
                Err(e) => NetworkEvent::Error {
                    message: e.to_string(),
                },
            },
            NetworkCommand::ConnectWifi { ssid, password } => {
                match self.wifi.connect(&ssid, password.as_deref()).await {
                    Ok(_) => {
                        self.status.wifi_connected = Some(WifiNetwork {
                            ssid: ssid.clone(),
                            bssid: "".into(),
                            signal_dbm: -65,
                            frequency_mhz: 5180,
                            security: WifiSecurity::Wpa3Personal,
                            connected: true,
                            saved: true,
                        });
                        NetworkEvent::ConnectResult {
                            success: true,
                            ssid,
                            error: None,
                        }
                    }
                    Err(e) => NetworkEvent::ConnectResult {
                        success: false,
                        ssid,
                        error: Some(e.to_string()),
                    },
                }
            }
            NetworkCommand::DisconnectWifi => {
                let _ = self.wifi.disconnect().await;
                self.status.wifi_connected = None;
                NetworkEvent::StatusChanged {
                    status: self.status.clone(),
                }
            }
            NetworkCommand::SetWifiEnabled { enabled } => {
                self.status.wifi_enabled = enabled;
                if !enabled {
                    self.status.wifi_connected = None;
                }
                NetworkEvent::StatusChanged {
                    status: self.status.clone(),
                }
            }
            NetworkCommand::SetMobileData { enabled } => {
                self.status.mobile_data_enabled = enabled;
                NetworkEvent::StatusChanged {
                    status: self.status.clone(),
                }
            }
            NetworkCommand::SetAirplaneMode { enabled } => {
                self.status.airplane_mode = enabled;
                if enabled {
                    self.status.wifi_enabled = false;
                    self.status.mobile_data_enabled = false;
                    self.status.wifi_connected = None;
                }
                NetworkEvent::StatusChanged {
                    status: self.status.clone(),
                }
            }
            NetworkCommand::StartHotspot { ssid, password } => {
                match self.wifi.start_hotspot(&ssid, &password).await {
                    Ok(_) => {
                        self.status.hotspot_active = true;
                        NetworkEvent::StatusChanged {
                            status: self.status.clone(),
                        }
                    }
                    Err(e) => NetworkEvent::Error {
                        message: e.to_string(),
                    },
                }
            }
            NetworkCommand::StopHotspot => {
                self.status.hotspot_active = false;
                NetworkEvent::StatusChanged {
                    status: self.status.clone(),
                }
            }
            NetworkCommand::ForgetNetwork { ssid } => {
                self.wifi.known_networks.remove(&ssid);
                NetworkEvent::StatusChanged {
                    status: self.status.clone(),
                }
            }
        }
    }

    pub async fn run(self) -> Result<()> {
        let socket_path = "/run/aether/network.sock";
        let _ = std::fs::remove_file(socket_path);
        let listener = UnixListener::bind(socket_path)?;
        info!("AetherOS network daemon ready");

        // Setup DNS-over-HTTPS
        let _ = DohResolver::cloudflare().write_resolv_conf().await;

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    info!("network IPC client connected");
                    // In production: spawn client handler similar to telephony daemon
                    drop(stream);
                }
                Err(e) => tracing::error!("accept: {}", e),
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("aether_network=info,warn")
        .init();
    info!("AetherOS network daemon starting");
    NetworkDaemon::new().run().await
}

// aether-networkd — AetherOS Network Manager
// SPDX-License-Identifier: Apache-2.0

mod wifi;
mod dns;
mod netlink;

use anyhow::Result;
use tracing::info;
use crate::wifi::WifiManager;
use crate::dns::DnsResolver;
use crate::netlink::NetlinkMonitor;

pub struct NetworkOrchestrator {
    wifi: WifiManager,
}

impl Default for NetworkOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkOrchestrator {
    pub fn new() -> Self {
        Self {
            wifi: WifiManager::new("wlan0"),
        }
    }

    pub async fn run(&self) -> Result<()> {
        info!("AetherOS Network Orchestrator starting");

        // Step 1: Initialize DNS
        DnsResolver::apply_system_config(&["1.1.1.1", "8.8.8.8"]).await?;

        // Step 2: Spawn Netlink Monitor
        tokio::spawn(async move {
            let _ = NetlinkMonitor::run().await;
        });

        // Step 3: Spawn Wi-Fi Monitor
        let wifi = self.wifi.clone();
        tokio::spawn(async move {
            let _ = wifi.run_monitoring().await;
        });

        // Keep running
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
        }
    }
}

// Ensure WifiManager is Cloneable for task spawning
impl Clone for WifiManager {
    fn clone(&self) -> Self {
        Self::new("wlan0")
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()))
        .init();

    let orchestrator = NetworkOrchestrator::new();
    orchestrator.run().await
}

// aether-otad — AetherOS Over-The-Air Update Client
// SPDX-License-Identifier: Apache-2.0

mod partition;
mod rollback;
mod server;
mod signing;

use crate::partition::PartitionManager;
use crate::rollback::PostUpdateMonitor;
use crate::server::UpdateServerClient;
use crate::signing::SignatureVerifier;
use anyhow::Result;
use tokio::time::{interval, Duration};
use tracing::info;

pub struct OtaOrchestrator {
    server_client: UpdateServerClient,
    verifier: SignatureVerifier,
    _partition_mgr: PartitionManager,
}

impl OtaOrchestrator {
    pub fn new(public_key: &str, server_url: &str) -> Result<Self> {
        Ok(Self {
            server_client: UpdateServerClient::new(server_url),
            verifier: SignatureVerifier::new(public_key)?,
            _partition_mgr: PartitionManager,
        })
    }

    pub async fn run(&self) -> Result<()> {
        info!("AetherOS OTA Orchestrator starting");
        if std::path::Path::new("/data/ota/post_reboot_monitor.json").exists() {
            tokio::spawn(async move {
                let _ = PostUpdateMonitor::new().run().await;
            });
        }
        let mut interval = interval(Duration::from_secs(4 * 3600));
        loop {
            interval.tick().await;
            if let Ok(Some(m)) = self.server_client.check_updates("0.1.0", "dev").await {
                let path = std::path::Path::new("/tmp/ota.zip");
                self.server_client
                    .download_payload(&m.payload_url, path)
                    .await?;
                SignatureVerifier::verify_payload_sha256(path, &m.payload_sha256).await?;
                if self.verifier.verify(m.version.as_bytes(), &m.signature_b64) {
                    let slot = PartitionManager::get_current_slot()?.inactive();
                    self._partition_mgr.flash_to_slot(path, &slot).await?;
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()))
        .init();

    let public_key = std::env::var("AETHER_OTA_PUBLIC_KEY").unwrap_or_else(|_| {
        "0000000000000000000000000000000000000000000000000000000000000000".to_string()
    });
    let server_url = std::env::var("AETHER_OTA_SERVER")
        .unwrap_or_else(|_| "https://updates.aetheros.dev".to_string());

    let orchestrator = OtaOrchestrator::new(&public_key, &server_url)?;
    orchestrator.run().await
}

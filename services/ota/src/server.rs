// server.rs — Update server client for AetherOS OTA
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateManifest {
    pub version: String,
    pub channel: String,
    pub build_id: String,
    pub payload_url: String,
    pub payload_sha256: String,
    pub signature_b64: String,
    pub rollout_percent: u8,
}

pub struct UpdateServerClient {
    client: reqwest::Client,
    base_url: String,
}

impl UpdateServerClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.to_string(),
        }
    }

    pub async fn check_updates(
        &self,
        current_version: &str,
        channel: &str,
    ) -> Result<Option<UpdateManifest>> {
        let url = format!(
            "{}/api/v1/updates?version={}&channel={}",
            self.base_url, current_version, channel
        );
        info!(url, "Polling update server");

        let resp = self.client.get(&url).send().await?;
        if resp.status().as_u16() == 204 {
            return Ok(None);
        }

        let manifest: UpdateManifest = resp.json().await?;

        // Respect rollout percentage
        if !self.is_in_rollout(manifest.rollout_percent) {
            info!("Update available but device is outside rollout percentage");
            return Ok(None);
        }

        Ok(Some(manifest))
    }

    pub async fn download_payload(&self, url: &str, dest: &std::path::Path) -> Result<()> {
        let mut resp = self.client.get(url).send().await?.error_for_status()?;
        let mut file = std::fs::File::create(dest)?;
        while let Some(chunk) = resp.chunk().await? {
            use std::io::Write;
            file.write_all(&chunk)?;
        }
        Ok(())
    }

    fn is_in_rollout(&self, percent: u8) -> bool {
        if percent >= 100 {
            return true;
        }
        if percent == 0 {
            return false;
        }

        // Simple deterministic check for demonstration
        // In reality, this would use a hardware ID hash
        (Utc::now().timestamp() % 100) < percent as i64
    }
}

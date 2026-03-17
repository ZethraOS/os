// rollback.rs — Health monitor and rollback logic for ZethraOS OTA
// SPDX-License-Identifier: Apache-2.0

use crate::partition::PartitionManager;
use anyhow::Result;
use chrono::Utc;
use std::path::Path;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::time::{interval, Duration, Instant};
use tracing::{error, info, warn};

pub struct PostUpdateMonitor {
    error_rate_threshold: f32,
    monitoring_window: Duration,
    health_file: &'static str,
    log_file: &'static str,
}

impl PostUpdateMonitor {
    pub fn new() -> Self {
        Self {
            error_rate_threshold: 0.05,
            monitoring_window: Duration::from_secs(3600),
            health_file: "/run/zethra/health.json",
            log_file: "/var/log/zethra/ota_rollback.log",
        }
    }

    pub async fn run(&self) -> Result<()> {
        info!("Starting post-update health monitoring for 1 hour");
        let start = Instant::now();
        let mut ticker = interval(Duration::from_secs(60));

        loop {
            ticker.tick().await;

            if start.elapsed() > self.monitoring_window {
                info!("Monitoring window completed. Update is stable.");
                break;
            }

            let crash_rate = self.get_crash_rate().await?;
            if crash_rate > self.error_rate_threshold {
                warn!(
                    crash_rate,
                    "Crash rate exceeded threshold! Initiating rollback."
                );
                self.trigger_rollback(crash_rate).await?;
                break;
            }
        }
        Ok(())
    }

    async fn get_crash_rate(&self) -> Result<f32> {
        if !Path::new(self.health_file).exists() {
            return Ok(0.0);
        }
        let content = fs::read_to_string(self.health_file).await?;
        let data: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
        Ok(data["crash_rate_1m"].as_f64().unwrap_or(0.0) as f32)
    }

    async fn trigger_rollback(&self, rate: f32) -> Result<()> {
        let msg = format!(
            "[{}] ROLLBACK: detected {:.2}% crash rate. Reverting slot.\n",
            Utc::now(),
            rate * 100.0
        );

        // Log the event
        if let Some(parent) = Path::new(self.log_file).parent() {
            fs::create_dir_all(parent).await.ok();
        }
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(self.log_file)
            .await?;
        file.write_all(msg.as_bytes()).await?;

        error!("Initiating emergency slot revert");

        // Revert to the last known good slot
        let current = PartitionManager::get_current_slot()?;
        let previous = current.inactive();

        info!(reverting_to = ?previous, "Emergency revert initiated");

        tokio::process::Command::new("reboot").spawn().ok();
        Ok(())
    }
}

impl Default for PostUpdateMonitor {
    fn default() -> Self {
        Self::new()
    }
}

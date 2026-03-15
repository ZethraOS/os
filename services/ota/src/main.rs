// aether-otad — AetherOS Over-The-Air Update Client
// SPDX-License-Identifier: Apache-2.0
//
// Handles the full update lifecycle on the device side:
//   1. Poll update server for available builds
//   2. Verify cryptographic signature (ed25519)
//   3. Download with resume support
//   4. Verify payload hash (SHA-256)
//   5. Apply to inactive partition (A/B partition scheme)
//   6. Set bootloader flag to boot new partition on next reboot
//   7. Watch for post-boot health; roll back if error rate spikes
//
// A/B Update scheme (same as modern Android):
//   Slot A: active (running)   Slot B: inactive (update target)
//   After successful update: Slot B becomes active next boot
//   On 3 failed boots from Slot B: bootloader reverts to Slot A

use anyhow::{Context, Result};
use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};

// ─── Update metadata from the server ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateManifest {
    pub version: String,
    pub channel: UpdateChannel,
    pub build_id: String,
    pub released_at: DateTime<Utc>,
    pub required: bool,          // security patches may be required
    pub rollout_percent: u8,     // 0–100 for gradual rollout
    pub min_battery_percent: u8, // don't update below this
    pub payload_url: String,
    pub payload_size_bytes: u64,
    pub payload_sha256: String,
    pub signature_b64: String, // ed25519 over manifest JSON
    pub changelog: String,
    pub security_patch_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum UpdateChannel {
    Dev,
    Beta,
    Stable,
}

// ─── Local device state ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceUpdateState {
    pub current_version: String,
    pub current_slot: Slot,
    pub channel: UpdateChannel,
    pub last_check: Option<DateTime<Utc>>,
    pub pending_update: Option<UpdateManifest>,
    pub download_progress: Option<DownloadProgress>,
    pub auto_download: bool,
    pub auto_install_wifi_only: bool,
    pub install_hour_start: u8, // only install between these hours (night update)
    pub install_hour_end: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Slot {
    A,
    B,
}

impl Slot {
    pub fn inactive(&self) -> Self {
        match self {
            Slot::A => Slot::B,
            Slot::B => Slot::A,
        }
    }
    pub fn block_device(&self) -> &str {
        match self {
            Slot::A => "/dev/block/by-name/system_a",
            Slot::B => "/dev/block/by-name/system_b",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub started_at: DateTime<Utc>,
    pub speed_bytes_sec: u64,
}

impl DownloadProgress {
    pub fn percent(&self) -> f32 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        (self.bytes_downloaded as f32 / self.total_bytes as f32) * 100.0
    }

    pub fn eta_secs(&self) -> u64 {
        if self.speed_bytes_sec == 0 {
            return u64::MAX;
        }
        (self.total_bytes - self.bytes_downloaded) / self.speed_bytes_sec
    }
}

// ─── Signature verifier ────────────────────────────────────────────────────

pub struct SignatureVerifier {
    _public_key_bytes: [u8; 32],
}

impl SignatureVerifier {
    // The public key is baked into the OS image at build time
    // and is also stored in the bootloader for extra integrity
    pub fn new(public_key_hex: &str) -> Result<Self> {
        let bytes = hex::decode(public_key_hex)?;
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes[..32]);
        Ok(Self {
            _public_key_bytes: key,
        })
    }

    pub fn verify(&self, manifest_json: &str, signature_b64: &str) -> bool {
        // In production: use ed25519_dalek
        // ed25519_dalek::PublicKey::from_bytes(&self.public_key_bytes)
        //     .and_then(|pk| {
        //         let sig_bytes = base64::decode(signature_b64).unwrap();
        //         let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes).unwrap();
        //         pk.verify(manifest_json.as_bytes(), &sig)
        //     }).is_ok()
        info!(
            "signature verification (stub) for {} bytes",
            manifest_json.len()
        );
        !signature_b64.is_empty() // stub
    }
}

// ─── Downloader with resume support ────────────────────────────────────────

pub struct Downloader {
    client: reqwest::Client,
    download_dir: PathBuf,
}

impl Downloader {
    pub fn new(download_dir: &str) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap(),
            download_dir: PathBuf::from(download_dir),
        }
    }

    pub async fn download(
        &self,
        manifest: &UpdateManifest,
        mut progress_cb: impl FnMut(DownloadProgress),
    ) -> Result<PathBuf> {
        let filename = format!("aetheros-{}-{}.zip", manifest.version, manifest.build_id);
        let dest = self.download_dir.join(&filename);
        let partial = dest.with_extension("zip.partial");

        // Resume from partial download
        let bytes_done = if partial.exists() {
            fs::metadata(&partial).await?.len()
        } else {
            0
        };

        info!(
            version = %manifest.version,
            size = manifest.payload_size_bytes,
            resuming_from = bytes_done,
            "downloading OTA"
        );

        let mut req = self.client.get(&manifest.payload_url);
        if bytes_done > 0 {
            req = req.header("Range", format!("bytes={}-", bytes_done));
        }

        let resp = req.send().await.context("download request failed")?;
        if !resp.status().is_success() && resp.status().as_u16() != 206 {
            anyhow::bail!("server returned {}", resp.status());
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(bytes_done > 0)
            .write(true)
            .open(&partial)
            .await?;

        let mut downloaded = bytes_done;
        let started_at = Utc::now();
        let mut stream = resp.bytes_stream();

        use futures_util::StreamExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("download chunk error")?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;

            let elapsed = (Utc::now() - started_at).num_seconds().max(1) as u64;
            progress_cb(DownloadProgress {
                bytes_downloaded: downloaded,
                total_bytes: manifest.payload_size_bytes,
                started_at,
                speed_bytes_sec: downloaded / elapsed,
            });
        }
        file.flush().await?;
        drop(file);

        // Verify hash
        let hash = self.sha256_file(&partial).await?;
        if hash != manifest.payload_sha256 {
            fs::remove_file(&partial).await.ok();
            anyhow::bail!(
                "payload hash mismatch: expected {} got {}",
                manifest.payload_sha256,
                hash
            );
        }

        fs::rename(&partial, &dest).await?;
        info!(path = %dest.display(), "download verified");
        Ok(dest)
    }

    async fn sha256_file(&self, path: &Path) -> Result<String> {
        use sha2::{Digest, Sha256};
        let mut file = File::open(path).await?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 65536];
        loop {
            let n = file.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    }
}

// ─── Partition writer ──────────────────────────────────────────────────────

pub struct PartitionWriter;

impl PartitionWriter {
    /// Flash the OTA zip payload to the inactive slot.
    /// In production this uses a delta patcher (like Chromium OS's update_engine).
    pub async fn apply_to_slot(&self, payload_path: &Path, slot: &Slot) -> Result<()> {
        let target = slot.block_device();
        info!(
            payload = %payload_path.display(),
            target,
            "applying OTA to inactive slot"
        );

        // Step 1: Unzip payload
        // Step 2: Verify partition signatures
        // Step 3: Write each partition image with dd / direct block write
        // Step 4: Update bootloader metadata to mark slot as ready

        // In a real implementation this is a careful, atomic write:
        // - Write system, vendor, product, odm partitions
        // - Never touch the currently-running slot
        // - Only set the boot flag AFTER all partitions are written

        // Set BCB (Boot Control Block) to try the new slot
        self.set_active_slot_next_boot(slot).await?;
        info!(slot = ?slot, "slot marked active for next boot");
        Ok(())
    }

    async fn set_active_slot_next_boot(&self, slot: &Slot) -> Result<()> {
        let slot_char = match slot {
            Slot::A => "a",
            Slot::B => "b",
        };
        // Write to bootloader control block
        let bcb_path = "/dev/block/by-name/misc";
        info!(slot = slot_char, bcb = bcb_path, "setting boot slot");
        // In production: write the BCB structure defined in bootctrl.h
        Ok(())
    }
}

// ─── Health monitor — post-update rollback ─────────────────────────────────

pub struct PostUpdateMonitor {
    error_rate_threshold: f32,
    monitoring_window_secs: u64,
}

impl PostUpdateMonitor {
    pub fn new() -> Self {
        Self {
            error_rate_threshold: 0.05,   // >5% crash rate triggers rollback
            monitoring_window_secs: 3600, // watch for 1 hour
        }
    }
}

impl Default for PostUpdateMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl PostUpdateMonitor {

    pub async fn monitor_and_rollback_if_needed(&self) -> Result<()> {
        let mut ticker = interval(Duration::from_secs(60));
        let start = std::time::Instant::now();

        loop {
            ticker.tick().await;

            let elapsed = start.elapsed().as_secs();
            if elapsed > self.monitoring_window_secs {
                info!("post-update monitoring window complete — update stable");
                break;
            }

            let error_rate = self.sample_error_rate().await;
            if error_rate > self.error_rate_threshold {
                warn!(
                    error_rate,
                    threshold = self.error_rate_threshold,
                    "post-update error spike detected — initiating rollback"
                );
                self.rollback().await?;
                break;
            }

            info!(error_rate, elapsed_secs = elapsed, "post-update health OK");
        }
        Ok(())
    }

    async fn sample_error_rate(&self) -> f32 {
        // Read from the crash reporter's counter in /run/aether/health.json
        let path = "/run/aether/health.json";
        let content = fs::read_to_string(path).await.unwrap_or_default();
        let data: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
        data["crash_rate_1m"].as_f64().unwrap_or(0.0) as f32
    }

    async fn rollback(&self) -> Result<()> {
        error!("ROLLBACK: switching back to previous slot");
        // Set BCB to the previous slot
        // On next reboot the system returns to the last known-good state
        let writer = PartitionWriter;
        writer.set_active_slot_next_boot(&Slot::A).await?;
        // Schedule immediate reboot
        tokio::process::Command::new("reboot").spawn().ok();
        Ok(())
    }
}

// ─── OTA daemon ───────────────────────────────────────────────────────────

pub struct OtaDaemon {
    state: DeviceUpdateState,
    verifier: SignatureVerifier,
    downloader: Downloader,
    partition_writer: PartitionWriter,
    update_server: String,
    client: reqwest::Client,
}

impl OtaDaemon {
    pub fn new(public_key_hex: &str, update_server: &str) -> Result<Self> {
        let state = DeviceUpdateState {
            current_version: std::fs::read_to_string("/etc/aether/version")
                .unwrap_or_else(|_| "0.1.0".to_string())
                .trim()
                .to_string(),
            current_slot: Slot::A,
            channel: UpdateChannel::Dev,
            last_check: None,
            pending_update: None,
            download_progress: None,
            auto_download: true,
            auto_install_wifi_only: true,
            install_hour_start: 2,
            install_hour_end: 5,
        };
        Ok(Self {
            state,
            verifier: SignatureVerifier::new(public_key_hex)?,
            downloader: Downloader::new("/data/ota/downloads"),
            partition_writer: PartitionWriter,
            update_server: update_server.to_string(),
            client: reqwest::Client::new(),
        })
    }

    pub async fn check_for_updates(&mut self) -> Result<Option<UpdateManifest>> {
        let url = format!(
            "{}/api/v1/updates?version={}&channel={:?}&device=generic_arm64",
            self.update_server, self.state.current_version, self.state.channel
        );

        info!("checking for updates: {}", url);
        self.state.last_check = Some(Utc::now());

        let resp = self.client.get(&url).send().await?;
        if resp.status().as_u16() == 204 {
            info!("no updates available");
            return Ok(None);
        }

        let manifest: UpdateManifest = resp.json().await?;

        // Verify signature before trusting anything
        let manifest_json = serde_json::to_string(&manifest)?;
        if !self
            .verifier
            .verify(&manifest_json, &manifest.signature_b64)
        {
            anyhow::bail!("OTA manifest signature verification FAILED");
        }

        info!(version = %manifest.version, "update available");
        self.state.pending_update = Some(manifest.clone());
        Ok(Some(manifest))
    }

    pub async fn install_update(&mut self, manifest: UpdateManifest) -> Result<()> {
        let target_slot = self.state.current_slot.inactive();
        info!(
            version = %manifest.version,
            target = ?target_slot,
            "beginning OTA installation"
        );

        // Download
        let state = &mut self.state;
        let payload_path = self
            .downloader
            .download(&manifest, |p| {
                state.download_progress = Some(p.clone());
                info!(
                    "{:.1}% @ {:.1} MB/s — ETA {}s",
                    p.percent(),
                    p.speed_bytes_sec as f32 / 1_000_000.0,
                    p.eta_secs()
                );
            })
            .await?;

        // Apply to inactive slot
        self.partition_writer
            .apply_to_slot(&payload_path, &target_slot)
            .await?;

        // Clean up download
        fs::remove_file(&payload_path).await.ok();

        info!(
            version = %manifest.version,
            "OTA applied — will activate on next reboot"
        );

        // Start monitoring after reboot
        // (This spawns as a separate oneshot service post-reboot)
        self.write_post_reboot_monitor_flag(&manifest.version)
            .await?;

        Ok(())
    }

    async fn write_post_reboot_monitor_flag(&self, new_version: &str) -> Result<()> {
        let flag = serde_json::json!({
            "monitor": true,
            "new_version": new_version,
            "applied_at": Utc::now(),
        });
        fs::write(
            "/data/ota/post_reboot_monitor.json",
            serde_json::to_string_pretty(&flag)?,
        )
        .await?;
        Ok(())
    }

    pub async fn run(mut self) -> Result<()> {
        info!(
            "AetherOS OTA daemon starting (version {})",
            self.state.current_version
        );

        // Check if we just rebooted after an update
        if Path::new("/data/ota/post_reboot_monitor.json").exists() {
            info!("post-update boot detected — starting health monitor");
            let monitor = PostUpdateMonitor::new();
            tokio::spawn(async move {
                let _ = monitor.monitor_and_rollback_if_needed().await;
            });
            fs::remove_file("/data/ota/post_reboot_monitor.json")
                .await
                .ok();
        }

        let mut check_ticker = interval(Duration::from_secs(4 * 60 * 60)); // every 4h

        loop {
            check_ticker.tick().await;

            match self.check_for_updates().await {
                Ok(Some(manifest)) => {
                    let now_hour = chrono::Utc::now().hour();
                    let in_window = now_hour >= self.state.install_hour_start as u32
                        && now_hour < self.state.install_hour_end as u32;

                    if self.state.auto_download && (in_window || manifest.required) {
                        if let Err(e) = self.install_update(manifest).await {
                            error!("OTA install failed: {}", e);
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => warn!("update check failed: {}", e),
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("aether_otad=info,warn")
        .init();

    let public_key = std::env::var("AETHER_OTA_PUBLIC_KEY").unwrap_or_else(|_| {
        "0000000000000000000000000000000000000000000000000000000000000000".to_string()
    });
    let server = std::env::var("AETHER_OTA_SERVER")
        .unwrap_or_else(|_| "https://updates.aetheros.dev".to_string());

    OtaDaemon::new(&public_key, &server)?.run().await
}

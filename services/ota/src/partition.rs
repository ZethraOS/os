// partition.rs — A/B partition manager for AetherOS OTA
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Slot {
    A,
    B,
}

impl Slot {
    pub fn from_suffix(suffix: &str) -> Option<Self> {
        match suffix {
            "_a" => Some(Slot::A),
            "_b" => Some(Slot::B),
            _ => None,
        }
    }

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

pub struct PartitionManager;

impl PartitionManager {
    pub fn get_current_slot() -> Result<Slot> {
        let cmdline = fs::read_to_string("/proc/cmdline").unwrap_or_default();

        for arg in cmdline.split_whitespace() {
            if let Some(suffix) = arg.strip_prefix("androidboot.slot_suffix=") {
                if let Some(slot) = Slot::from_suffix(suffix) {
                    return Ok(slot);
                }
            }
        }

        // Fallback or default for systems without slot_suffix
        warn!("slot_suffix not found in cmdline, defaulting to Slot A");
        Ok(Slot::A)
    }

    pub async fn flash_to_slot(&self, payload_path: &Path, slot: &Slot) -> Result<()> {
        let target = slot.block_device();
        info!(payload = %payload_path.display(), target, "Flashing OTA to inactive slot");

        // Step 1: In production this would use dd or a specialized block writer
        // For the purpose of this implementation we simulate the write
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        self.set_active_slot_next_boot(slot).await?;
        Ok(())
    }

    async fn set_active_slot_next_boot(&self, slot: &Slot) -> Result<()> {
        let bcb_path = "/dev/block/by-name/misc";
        info!(slot = ?slot, bcb = bcb_path, "Updating BCB (Boot Control Block) for next boot");
        // Simulated BCB write
        Ok(())
    }
}

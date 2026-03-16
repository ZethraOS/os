// at.rs — AT command engine for AetherOS Telephony
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio_serial::{SerialPortBuilderExt, SerialStream};
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub enum AtPriority {
    Normal,
    High,
}

#[derive(Debug, Clone)]
pub struct AtCommand {
    pub cmd: String,
    pub priority: AtPriority,
    pub timeout: Duration,
}

pub struct AtEngine {
    port: SerialStream,
    queue: VecDeque<AtCommand>,
}

impl AtEngine {
    pub fn new(device: &str, baud_rate: u32) -> Result<Self> {
        let port = tokio_serial::new(device, baud_rate)
            .open_native_async()
            .context("Failed to open serial port")?;
        Ok(Self {
            port,
            queue: VecDeque::new(),
        })
    }

    pub async fn send_command(&mut self, cmd: AtCommand) -> Result<String> {
        match cmd.priority {
            AtPriority::High => self.queue.push_front(cmd),
            AtPriority::Normal => self.queue.push_back(cmd),
        }
        self.process_queue().await
    }

    async fn process_queue(&mut self) -> Result<String> {
        if let Some(at_cmd) = self.queue.pop_front() {
            let cmd_bytes = format!("{}\r\n", at_cmd.cmd).into_bytes();
            self.port.write_all(&cmd_bytes).await?;

            let mut reader = BufReader::new(&mut self.port);
            let mut response = String::new();

            tokio::select! {
                res = reader.read_line(&mut response) => {
                    res?;
                    info!(cmd = at_cmd.cmd, response = response.trim(), "AT command executed");
                    Ok(response)
                }
                _ = tokio::time::sleep(at_cmd.timeout) => {
                    warn!(cmd = at_cmd.cmd, "AT command timed out");
                    Err(anyhow::anyhow!("AT command timeout"))
                }
            }
        } else {
            Err(anyhow::anyhow!("Queue empty"))
        }
    }

    pub async fn monitor_urc(&mut self) -> Result<()> {
        let mut reader = BufReader::new(&mut self.port);
        let mut line = String::new();
        loop {
            line.clear();
            reader.read_line(&mut line).await?;
            let trimmed = line.trim();
            if trimmed.starts_with("+CRING:") {
                info!("Incoming call detected");
            } else if trimmed.starts_with("+CMT:") {
                info!("Incoming SMS detected");
            } else if trimmed.starts_with("+CREG:") {
                info!("Network registration state changed");
            }
        }
    }
}

// at.rs — AT command engine for ZethraOS Telephony
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio_serial::{SerialPortBuilderExt, SerialStream};
use tracing::{info, warn};
use zethra_hal::{CallInfo, NetworkRegistrationStatus, SmsMessage, TelephonyHal};

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

#[async_trait::async_trait]
impl TelephonyHal for AtEngine {
    async fn get_registration_status(&mut self) -> Result<NetworkRegistrationStatus> {
        let cmd = AtCommand {
            cmd: "AT+CREG?".to_string(),
            priority: AtPriority::Normal,
            timeout: Duration::from_secs(2),
        };
        match self.send_command(cmd).await {
            Ok(resp) if resp.contains(",1") || resp.contains(",5") => {
                Ok(NetworkRegistrationStatus::RegisteredHome)
            }
            _ => Ok(NetworkRegistrationStatus::NotRegistered),
        }
    }

    async fn dial_call(&mut self, number: &str, is_emergency: bool) -> Result<String> {
        let cmd = AtCommand {
            cmd: format!("ATD{};", number),
            priority: if is_emergency {
                AtPriority::High
            } else {
                AtPriority::Normal
            },
            timeout: Duration::from_secs(10),
        };
        self.send_command(cmd).await?;
        let call_id = format!("at-call-{}", number);
        info!(number = %number, call_id = %call_id, is_emergency, "AT Engine: Call dialed");
        Ok(call_id)
    }

    async fn hangup_call(&mut self, call_id: &str) -> Result<()> {
        let cmd = AtCommand {
            cmd: "ATH".to_string(),
            priority: AtPriority::High,
            timeout: Duration::from_secs(3),
        };
        self.send_command(cmd).await?;
        info!(call_id = %call_id, "AT Engine: Call hung up");
        Ok(())
    }

    async fn send_sms(&mut self, destination: &str, text: &str) -> Result<()> {
        let pdu = crate::pdu::PduEncoder::create_submit_pdu(destination, text)?;
        let len = (pdu.len() - 2) / 2; // Subtract SMSC header byte length approximation
        let cmd = AtCommand {
            cmd: format!("AT+CMGS={}\r{}\x1A", len, pdu),
            priority: AtPriority::Normal,
            timeout: Duration::from_secs(10),
        };
        self.send_command(cmd).await?;
        info!(destination = %destination, "AT Engine: SMS PDU transmitted");
        Ok(())
    }

    async fn pull_incoming_sms(&mut self) -> Result<Vec<SmsMessage>> {
        // In serial AT mode, incoming messages are captured asynchronously via monitor_urc or AT+CMGL polling.
        Ok(Vec::new())
    }

    async fn list_active_calls(&mut self) -> Result<Vec<CallInfo>> {
        let cmd = AtCommand {
            cmd: "AT+CLCC".to_string(),
            priority: AtPriority::Normal,
            timeout: Duration::from_secs(2),
        };
        let _ = self.send_command(cmd).await;
        Ok(Vec::new())
    }
}

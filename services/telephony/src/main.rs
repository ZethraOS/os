// aether-telephonyd — AetherOS Telephony Daemon
// SPDX-License-Identifier: Apache-2.0
//
// Manages voice calls, SMS, SIM state, and signal reporting.
// The modem is abstracted via a ModemBackend enum so we can swap
// real AT-command hardware for a simulator during local development.

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, warn};
use uuid::Uuid;

// ─── Data models ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CallState {
    Idle,
    Dialing {
        number: String,
    },
    Ringing {
        number: String,
        incoming: bool,
    },
    Active {
        number: String,
        started_at: DateTime<Utc>,
    },
    Held,
    Ended {
        duration_secs: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Call {
    pub id: String,
    pub number: String,
    pub state: CallState,
    pub incoming: bool,
    pub created_at: DateTime<Utc>,
}

impl Call {
    pub fn new_outgoing(number: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            number: number.to_string(),
            state: CallState::Dialing {
                number: number.to_string(),
            },
            incoming: false,
            created_at: Utc::now(),
        }
    }
    pub fn new_incoming(number: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            number: number.to_string(),
            state: CallState::Ringing {
                number: number.to_string(),
                incoming: true,
            },
            incoming: true,
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sms {
    pub id: String,
    pub from: String,
    pub to: String,
    pub body: String,
    pub timestamp: DateTime<Utc>,
    pub delivered: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimInfo {
    pub iccid: String,
    pub imsi: String,
    pub operator: String,
    pub mcc: String,
    pub mnc: String,
    pub pin_state: PinState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinState {
    Ready,
    PinRequired,
    PukRequired,
    Absent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalInfo {
    pub rssi_dbm: i32,
    pub rsrp_dbm: Option<i32>,
    pub technology: RadioTech,
    pub bars: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RadioTech {
    Gsm,
    Umts,
    Lte,
    Nr5g,
    Unknown,
}

impl SignalInfo {
    pub fn from_rssi(rssi: i32, tech: RadioTech) -> Self {
        let bars = match rssi {
            i if i >= -65 => 4,
            i if i >= -75 => 3,
            i if i >= -85 => 2,
            i if i >= -95 => 1,
            _ => 0,
        };
        Self {
            rssi_dbm: rssi,
            rsrp_dbm: None,
            technology: tech,
            bars,
        }
    }
}

// ─── IPC messages ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum TelephonyCommand {
    Dial { number: String },
    Answer { call_id: String },
    Hangup { call_id: String },
    Hold { call_id: String },
    SendSms { to: String, body: String },
    GetSimInfo,
    GetSignal,
    GetActiveCalls,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum TelephonyEvent {
    CallStateChanged { call: Call },
    IncomingCall { call: Call },
    SmsReceived { sms: Sms },
    SmsDelivered { id: String },
    SignalChanged { signal: SignalInfo },
    SimStateChanged { sim: SimInfo },
    Error { message: String },
}

// ─── ModemBackend trait ───────────────────────────────────────────────────────
// async_trait makes this dyn-compatible by boxing the futures.

#[async_trait]
pub trait ModemBackend: Send + Sync {
    async fn dial(&self, number: &str) -> Result<()>;
    async fn answer(&self) -> Result<()>;
    async fn hangup(&self) -> Result<()>;
    async fn send_sms(&self, to: &str, body: &str) -> Result<String>;
    async fn get_signal(&self) -> Result<SignalInfo>;
    async fn get_sim_info(&self) -> Result<SimInfo>;
}

// ─── Simulated modem (local dev / CI) ─────────────────────────────────────────

pub struct SimulatedModem {
    event_tx: broadcast::Sender<TelephonyEvent>,
}

impl SimulatedModem {
    pub fn new(event_tx: broadcast::Sender<TelephonyEvent>) -> Self {
        Self { event_tx }
    }
}

#[async_trait]
impl ModemBackend for SimulatedModem {
    async fn dial(&self, number: &str) -> Result<()> {
        info!(number, "[SIM] dialing");
        let call = Call::new_outgoing(number);
        let _ = self
            .event_tx
            .send(TelephonyEvent::CallStateChanged { call });
        // Simulate ringing after 1 second
        let tx = self.event_tx.clone();
        let num = number.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            let mut c = Call::new_outgoing(&num);
            c.state = CallState::Ringing {
                number: num.clone(),
                incoming: false,
            };
            let _ = tx.send(TelephonyEvent::CallStateChanged { call: c });
        });
        Ok(())
    }

    async fn answer(&self) -> Result<()> {
        info!("[SIM] answering call");
        Ok(())
    }

    async fn hangup(&self) -> Result<()> {
        info!("[SIM] hanging up");
        Ok(())
    }

    async fn send_sms(&self, to: &str, body: &str) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        info!(to, body_len = body.len(), "[SIM] SMS sent");
        let tx = self.event_tx.clone();
        let sid = id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            let _ = tx.send(TelephonyEvent::SmsDelivered { id: sid });
        });
        Ok(id)
    }

    async fn get_signal(&self) -> Result<SignalInfo> {
        Ok(SignalInfo::from_rssi(-72, RadioTech::Lte))
    }

    async fn get_sim_info(&self) -> Result<SimInfo> {
        Ok(SimInfo {
            iccid: "8991101200003204510".into(),
            imsi: "310260000000000".into(),
            operator: "AetherNet (simulated)".into(),
            mcc: "310".into(),
            mnc: "260".into(),
            pin_state: PinState::Ready,
        })
    }
}

// ─── AT command modem (real hardware) ────────────────────────────────────────

pub struct AtModem {
    device_path: String,
}

impl AtModem {
    pub fn new(device_path: &str) -> Self {
        Self {
            device_path: device_path.to_string(),
        }
    }

    async fn send_at(&self, cmd: &str) -> Result<String> {
        // Production: open self.device_path as serial, write cmd\r\n, read until OK/ERROR
        info!(cmd, device = %self.device_path, "AT command (stub)");
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        Ok("OK".to_string())
    }
}

#[async_trait]
impl ModemBackend for AtModem {
    async fn dial(&self, number: &str) -> Result<()> {
        self.send_at(&format!("ATD{};", number)).await?;
        Ok(())
    }
    async fn answer(&self) -> Result<()> {
        self.send_at("ATA").await?;
        Ok(())
    }
    async fn hangup(&self) -> Result<()> {
        self.send_at("ATH").await?;
        Ok(())
    }
    async fn send_sms(&self, to: &str, body: &str) -> Result<String> {
        self.send_at("AT+CMGF=0").await?;
        let _ = (to, body); // PDU encoding in production
        Ok(Uuid::new_v4().to_string())
    }
    async fn get_signal(&self) -> Result<SignalInfo> {
        let _resp = self.send_at("AT+CSQ").await?;
        Ok(SignalInfo::from_rssi(-77, RadioTech::Lte))
    }
    async fn get_sim_info(&self) -> Result<SimInfo> {
        self.send_at("AT+CIMI").await?;
        self.send_at("AT+CCID").await?;
        Ok(SimInfo {
            iccid: String::new(),
            imsi: String::new(),
            operator: String::new(),
            mcc: String::new(),
            mnc: String::new(),
            pin_state: PinState::Ready,
        })
    }
}

// ─── Modem enum — wraps both backends to stay dyn-compatible ─────────────────
// This avoids Box<dyn Trait> issues with async methods on older compilers.

pub enum Modem {
    Simulated(SimulatedModem),
    At(AtModem),
}

impl Modem {
    async fn dial(&self, number: &str) -> Result<()> {
        match self {
            Modem::Simulated(m) => m.dial(number).await,
            Modem::At(m) => m.dial(number).await,
        }
    }
    async fn answer(&self) -> Result<()> {
        match self {
            Modem::Simulated(m) => m.answer().await,
            Modem::At(m) => m.answer().await,
        }
    }
    async fn hangup(&self) -> Result<()> {
        match self {
            Modem::Simulated(m) => m.hangup().await,
            Modem::At(m) => m.hangup().await,
        }
    }
    async fn send_sms(&self, to: &str, body: &str) -> Result<String> {
        match self {
            Modem::Simulated(m) => m.send_sms(to, body).await,
            Modem::At(m) => m.send_sms(to, body).await,
        }
    }
    async fn get_signal(&self) -> Result<SignalInfo> {
        match self {
            Modem::Simulated(m) => m.get_signal().await,
            Modem::At(m) => m.get_signal().await,
        }
    }
    async fn get_sim_info(&self) -> Result<SimInfo> {
        match self {
            Modem::Simulated(m) => m.get_sim_info().await,
            Modem::At(m) => m.get_sim_info().await,
        }
    }
}

// ─── Telephony daemon ─────────────────────────────────────────────────────────

pub struct TelephonyDaemon {
    modem: Modem,
    active_calls: HashMap<String, Call>,
    event_tx: broadcast::Sender<TelephonyEvent>,
}

impl TelephonyDaemon {
    pub fn new(modem: Modem, event_tx: broadcast::Sender<TelephonyEvent>) -> Self {
        Self {
            modem,
            active_calls: HashMap::new(),
            event_tx,
        }
    }

    pub async fn handle_command(&mut self, cmd: TelephonyCommand) -> TelephonyEvent {
        match cmd {
            TelephonyCommand::Dial { number } => match self.modem.dial(&number).await {
                Ok(_) => {
                    let call = Call::new_outgoing(&number);
                    self.active_calls.insert(call.id.clone(), call.clone());
                    TelephonyEvent::CallStateChanged { call }
                }
                Err(e) => TelephonyEvent::Error {
                    message: e.to_string(),
                },
            },
            TelephonyCommand::Answer { call_id } => {
                if let Some(call) = self.active_calls.get_mut(&call_id) {
                    let _ = self.modem.answer().await;
                    call.state = CallState::Active {
                        number: call.number.clone(),
                        started_at: Utc::now(),
                    };
                    TelephonyEvent::CallStateChanged { call: call.clone() }
                } else {
                    TelephonyEvent::Error {
                        message: format!("call {} not found", call_id),
                    }
                }
            }
            TelephonyCommand::Hangup { call_id } => {
                let _ = self.modem.hangup().await;
                if let Some(mut call) = self.active_calls.remove(&call_id) {
                    let duration = match &call.state {
                        CallState::Active { started_at, .. } => {
                            (Utc::now() - *started_at).num_seconds().max(0) as u64
                        }
                        _ => 0,
                    };
                    call.state = CallState::Ended {
                        duration_secs: duration,
                    };
                    TelephonyEvent::CallStateChanged { call }
                } else {
                    TelephonyEvent::Error {
                        message: "no active call".into(),
                    }
                }
            }
            TelephonyCommand::SendSms { to, body } => match self.modem.send_sms(&to, &body).await {
                Ok(id) => TelephonyEvent::SmsDelivered { id },
                Err(e) => TelephonyEvent::Error {
                    message: e.to_string(),
                },
            },
            TelephonyCommand::GetSignal => match self.modem.get_signal().await {
                Ok(sig) => TelephonyEvent::SignalChanged { signal: sig },
                Err(e) => TelephonyEvent::Error {
                    message: e.to_string(),
                },
            },
            TelephonyCommand::GetSimInfo => match self.modem.get_sim_info().await {
                Ok(sim) => TelephonyEvent::SimStateChanged { sim },
                Err(e) => TelephonyEvent::Error {
                    message: e.to_string(),
                },
            },
            TelephonyCommand::GetActiveCalls => match self.active_calls.values().next() {
                Some(call) => TelephonyEvent::CallStateChanged { call: call.clone() },
                None => TelephonyEvent::Error {
                    message: "no active calls".into(),
                },
            },
            TelephonyCommand::Hold { call_id } => {
                if let Some(call) = self.active_calls.get_mut(&call_id) {
                    call.state = CallState::Held;
                    TelephonyEvent::CallStateChanged { call: call.clone() }
                } else {
                    TelephonyEvent::Error {
                        message: "call not found".into(),
                    }
                }
            }
        }
    }

    pub fn on_incoming_call(&mut self, number: &str) {
        let call = Call::new_incoming(number);
        self.active_calls.insert(call.id.clone(), call.clone());
        let _ = self.event_tx.send(TelephonyEvent::IncomingCall { call });
    }

    pub async fn run_ipc(&mut self, socket_path: &str) -> Result<()> {
        let _ = std::fs::remove_file(socket_path);
        // Ensure parent dir exists
        if let Some(parent) = std::path::Path::new(socket_path).parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let listener = UnixListener::bind(socket_path)
            .with_context(|| format!("binding IPC socket {}", socket_path))?;

        info!(socket = socket_path, "telephony IPC ready");

        let (cmd_tx, mut cmd_rx) =
            mpsc::channel::<(TelephonyCommand, mpsc::Sender<TelephonyEvent>)>(32);

        // Accept loop
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let tx = cmd_tx.clone();
                        tokio::spawn(handle_client(stream, tx));
                    }
                    Err(e) => error!("accept error: {}", e),
                }
            }
        });

        let event_tx = self.event_tx.clone();
        let mut signal_ticker = tokio::time::interval(tokio::time::Duration::from_secs(10));

        loop {
            tokio::select! {
                Some((cmd, reply_tx)) = cmd_rx.recv() => {
                    let event = self.handle_command(cmd).await;
                    let _ = reply_tx.send(event).await;
                }
                _ = signal_ticker.tick() => {
                    if let Ok(sig) = self.modem.get_signal().await {
                        let _ = event_tx.send(TelephonyEvent::SignalChanged { signal: sig });
                    }
                }
            }
        }
    }
}

async fn handle_client(
    stream: UnixStream,
    cmd_tx: mpsc::Sender<(TelephonyCommand, mpsc::Sender<TelephonyEvent>)>,
) {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        match serde_json::from_str::<TelephonyCommand>(&line) {
            Ok(cmd) => {
                let (reply_tx, mut reply_rx) = mpsc::channel(1);
                let _ = cmd_tx.send((cmd, reply_tx)).await;
                if let Some(event) = reply_rx.recv().await {
                    let json = serde_json::to_string(&event).unwrap_or_default();
                    let _ = writer.write_all(format!("{}\n", json).as_bytes()).await;
                }
            }
            Err(e) => warn!("bad command from client: {}", e),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()))
        .init();

    info!("AetherOS telephony daemon starting");

    let (event_tx, _) = broadcast::channel(64);
    let device = std::env::var("MODEM_DEVICE").unwrap_or_default();

    let modem = if device.is_empty() {
        info!("mode: simulated modem (set MODEM_DEVICE=/dev/ttyUSB0 for real hardware)");
        Modem::Simulated(SimulatedModem::new(event_tx.clone()))
    } else {
        info!(device, "mode: AT command modem");
        Modem::At(AtModem::new(&device))
    };

    let socket = std::env::var("TELEPHONY_SOCKET")
        .unwrap_or_else(|_| "/tmp/aether/telephony.sock".to_string());

    let mut daemon = TelephonyDaemon::new(modem, event_tx);
    daemon.run_ipc(&socket).await
}

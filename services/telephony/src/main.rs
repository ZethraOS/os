mod at;
mod emergency;
mod pdu;

use crate::at::AtEngine;
use crate::emergency::EmergencyHandler;
use anyhow::Result;
use tracing::info;

pub struct TelephonyOrchestrator {
    #[allow(dead_code)]
    at_engine: AtEngine,
    #[allow(dead_code)]
    emergency: EmergencyHandler,
}

impl TelephonyOrchestrator {
    pub async fn new(device: &str) -> Result<Self> {
        let at_engine = AtEngine::new(device, 115200)?;
        Ok(Self {
            at_engine,
            emergency: EmergencyHandler::new(),
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        info!("Telephony Orchestrator running");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()))
        .init();

    let device = std::env::var("MODEM_DEVICE").unwrap_or_else(|_| "/dev/ttyUSB0".to_string());

    info!(device, "Starting AetherOS Telephony Service");

    let mut orchestrator = TelephonyOrchestrator::new(&device).await?;
    orchestrator.run().await?;

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
    }
}

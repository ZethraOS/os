mod at;
mod emergency;
mod pdu;
mod proxy;

use crate::at::AtEngine;
use crate::emergency::EmergencyHandler;
use crate::proxy::ModemManagerProxy;
use anyhow::Result;
use std::path::Path;
use tracing::{info, warn};
use zethra_hal::{CallInfo, NetworkRegistrationStatus, SmsMessage, TelephonyHal};

pub struct TelephonyOrchestrator {
    backend: Box<dyn TelephonyHal + Send + Sync>,
    emergency: EmergencyHandler,
}

impl TelephonyOrchestrator {
    pub async fn new(device: &str) -> Result<Self> {
        let backend: Box<dyn TelephonyHal + Send + Sync> = match ModemManagerProxy::try_new().await
        {
            Ok(proxy) => {
                info!("✓ Using native asynchronous D-Bus ModemManagerProxy as primary telephony backend");
                Box::new(proxy)
            }
            Err(e) => {
                warn!("Primary D-Bus interface unavailable ({}). Inspecting secondary fallback backends...", e);
                if Path::new(device).exists() {
                    info!(device = %device, "✓ Physical serial modem node detected; initializing fallback AtEngine");
                    Box::new(AtEngine::new(device, 115200)?)
                } else {
                    // check_simulation_gate enforces ZETHRA_ALLOW_SIMULATED=1 in production.
                    // If the gate returns Err, the ? propagates it and the daemon exits non-zero
                    // with an ERROR log rather than silently entering simulation mode.
                    crate::proxy::check_simulation_gate(&format!(
                        "D-Bus unavailable ({}) and serial device '{}' not found",
                        e, device
                    ))?;
                    warn!(device = %device, "ZETHRA_ALLOW_SIMULATED=1: falling back to simulated ModemManagerProxy for test execution");
                    Box::new(ModemManagerProxy::new_simulated())
                }
            }
        };

        Ok(Self {
            backend,
            emergency: EmergencyHandler::new(),
        })
    }

    pub fn new_with_backend(backend: Box<dyn TelephonyHal + Send + Sync>) -> Self {
        Self {
            backend,
            emergency: EmergencyHandler::new(),
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        info!("Telephony Orchestrator service running and monitoring cellular signaling");
        let status = self.backend.get_registration_status().await?;
        info!(registration = ?status, "Initial cellular registration state");
        Ok(())
    }

    pub async fn dial(&mut self, number: &str) -> Result<String> {
        let is_emergency = self.emergency.is_emergency(number);
        if is_emergency || self.emergency.allow_unauthenticated(number) {
            info!(number = %number, "Initiating high-priority unauthenticated emergency cellular call");
            self.backend.dial_call(number, true).await
        } else {
            let status = self.backend.get_registration_status().await?;
            if status == NetworkRegistrationStatus::EmergencyOnly
                || status == NetworkRegistrationStatus::NotRegistered
                || status == NetworkRegistrationStatus::RegistrationDenied
            {
                return Err(anyhow::anyhow!("Cannot dial outbound call without cellular network registration (current status: {:?})", status));
            }
            self.backend.dial_call(number, false).await
        }
    }

    pub async fn hangup(&mut self, call_id: &str) -> Result<()> {
        self.backend.hangup_call(call_id).await
    }

    pub async fn send_sms(&mut self, destination: &str, text: &str) -> Result<()> {
        let status = self.backend.get_registration_status().await?;
        if status == NetworkRegistrationStatus::EmergencyOnly
            || status == NetworkRegistrationStatus::NotRegistered
            || status == NetworkRegistrationStatus::RegistrationDenied
        {
            return Err(anyhow::anyhow!("Cannot transmit SMS without valid cellular network registration (current status: {:?})", status));
        }
        self.backend.send_sms(destination, text).await
    }

    pub async fn check_inbox(&mut self) -> Result<Vec<SmsMessage>> {
        self.backend.pull_incoming_sms().await
    }

    pub async fn list_calls(&mut self) -> Result<Vec<CallInfo>> {
        self.backend.list_active_calls().await
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()))
        .init();

    let device = std::env::var("MODEM_DEVICE").unwrap_or_else(|_| "/dev/ttyUSB0".to_string());

    info!(
        device,
        "Starting ZethraOS Telephony Service (zethra-telephonyd)"
    );

    let mut orchestrator = TelephonyOrchestrator::new(&device).await?;
    orchestrator.run().await?;

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy::ModemManagerProxy;

    #[tokio::test]
    async fn test_orchestrator_simulated_routing() {
        let backend = Box::new(ModemManagerProxy::new_simulated());
        let mut orchestrator = TelephonyOrchestrator::new_with_backend(backend);

        orchestrator
            .run()
            .await
            .expect("Orchestrator failed to run");

        // Dial regular call
        let call_id = orchestrator
            .dial("555-0100")
            .await
            .expect("Failed standard dial");
        assert_eq!(
            orchestrator
                .list_calls()
                .await
                .expect("Failed list_calls")
                .len(),
            1
        );
        orchestrator.hangup(&call_id).await.expect("Failed hangup");
        assert_eq!(
            orchestrator
                .list_calls()
                .await
                .expect("Failed list_calls")
                .len(),
            0
        );

        // Dial emergency call (must succeed regardless of registration)
        let em_id = orchestrator
            .dial("911")
            .await
            .expect("Failed emergency dial");
        assert_eq!(
            orchestrator
                .list_calls()
                .await
                .expect("Failed list_calls")
                .len(),
            1
        );
        orchestrator.hangup(&em_id).await.expect("Failed hangup");
        assert_eq!(
            orchestrator
                .list_calls()
                .await
                .expect("Failed list_calls")
                .len(),
            0
        );

        // Transmit SMS and pull inbox
        orchestrator
            .send_sms("555-0100", "Testing orchestrator")
            .await
            .expect("Failed send_sms");
        let inbox = orchestrator
            .check_inbox()
            .await
            .expect("Failed check_inbox");
        assert_eq!(inbox.len(), 1);
    }
}

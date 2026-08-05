// proxy.rs — Userspace cellular proxy via FreeDesktop ModemManager / D-Bus
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use tracing::{info, warn};
use zbus::{zvariant, Connection};
use zethra_hal::{
    CallDirection, CallInfo, CallState, NetworkRegistrationStatus, SmsMessage, TelephonyHal,
};

/// Represents an asynchronous D-Bus proxy interface to FreeDesktop ModemManager.
/// Falls back to an in-memory simulated state machine when D-Bus is unavailable
/// or during automated CI validation.
pub struct ModemManagerProxy {
    dbus_connection: Option<Connection>,
    simulated: bool,
    simulated_state: SimulatedCellularState,
}

#[derive(Debug, Clone)]
struct SimulatedCellularState {
    registration: NetworkRegistrationStatus,
    active_calls: Vec<CallInfo>,
    sms_inbox: Vec<SmsMessage>,
    call_counter: u32,
}

impl ModemManagerProxy {
    /// Try to initialize a live D-Bus system connection to ModemManager.
    /// If D-Bus is unreachable (e.g. in macOS build host or CI container), returns an error
    /// unless fallback_simulated is invoked.
    pub async fn try_new() -> Result<Self> {
        match Connection::system().await {
            Ok(conn) => {
                info!("✓ Connected to FreeDesktop D-Bus System Bus for ModemManager proxy");
                Ok(Self {
                    dbus_connection: Some(conn),
                    simulated: false,
                    simulated_state: SimulatedCellularState::default(),
                })
            }
            Err(e) => {
                warn!(
                    "D-Bus System Bus unavailable ({}). ModemManagerProxy cannot bind to live ModemManager.",
                    e
                );
                Err(anyhow::anyhow!("D-Bus system connection failed: {}", e))
            }
        }
    }

    /// Initialize a mock/simulated cellular proxy for deterministic CI testing and benchtop development.
    pub fn new_simulated() -> Self {
        info!("Initializing simulated ModemManagerProxy state machine for local validation");
        Self {
            dbus_connection: None,
            simulated: true,
            simulated_state: SimulatedCellularState {
                registration: NetworkRegistrationStatus::RegisteredHome,
                active_calls: Vec::new(),
                sms_inbox: vec![SmsMessage {
                    message_id: "sms-sim-01".to_string(),
                    sender: "+18005551234".to_string(),
                    text: "Welcome to ZethraOS Telephony!".to_string(),
                    timestamp_epoch: 1722816000,
                }],
                call_counter: 0,
            },
        }
    }
}

impl Default for SimulatedCellularState {
    fn default() -> Self {
        Self {
            registration: NetworkRegistrationStatus::Unknown,
            active_calls: Vec::new(),
            sms_inbox: Vec::new(),
            call_counter: 0,
        }
    }
}

#[async_trait::async_trait]
impl TelephonyHal for ModemManagerProxy {
    async fn get_registration_status(&mut self) -> Result<NetworkRegistrationStatus> {
        if self.simulated || self.dbus_connection.is_none() {
            info!(
                status = ?self.simulated_state.registration,
                "Simulated ModemManager: get_registration_status"
            );
            return Ok(self.simulated_state.registration);
        }

        let conn = self
            .dbus_connection
            .as_ref()
            .context("D-Bus connection lost")?;
        let proxy = zbus::Proxy::new(
            conn,
            "org.freedesktop.ModemManager1",
            "/org/freedesktop/ModemManager1/Modem/0",
            "org.freedesktop.ModemManager1.Modem.Modem3gpp",
        )
        .await?;

        let reg_state: Result<u32, _> = proxy.get_property("RegistrationState").await;
        match reg_state {
            Ok(1) => Ok(NetworkRegistrationStatus::RegisteredHome),
            Ok(5) => Ok(NetworkRegistrationStatus::RegisteredRoaming),
            Ok(2) => Ok(NetworkRegistrationStatus::Searching),
            Ok(3) => Ok(NetworkRegistrationStatus::RegistrationDenied),
            Ok(8) => Ok(NetworkRegistrationStatus::EmergencyOnly),
            _ => Ok(NetworkRegistrationStatus::NotRegistered),
        }
    }

    async fn dial_call(&mut self, number: &str, is_emergency: bool) -> Result<String> {
        if self.simulated || self.dbus_connection.is_none() {
            self.simulated_state.call_counter += 1;
            let call_id = format!("call-sim-{}", self.simulated_state.call_counter);
            info!(
                number = %number,
                call_id = %call_id,
                is_emergency,
                "Simulated ModemManager: dial_call initiated"
            );
            self.simulated_state.active_calls.push(CallInfo {
                call_id: call_id.clone(),
                number: number.to_string(),
                state: CallState::Active,
                direction: CallDirection::Outbound,
            });
            return Ok(call_id);
        }

        let conn = self
            .dbus_connection
            .as_ref()
            .context("D-Bus connection lost")?;
        let proxy = zbus::Proxy::new(
            conn,
            "org.freedesktop.ModemManager1",
            "/org/freedesktop/ModemManager1/Modem/0",
            "org.freedesktop.ModemManager1.Modem.Voice",
        )
        .await?;

        let mut properties = std::collections::HashMap::new();
        properties.insert("number", zvariant::Value::from(number));
        if is_emergency {
            properties.insert("emergency", zvariant::Value::from(true));
        }

        let path: zvariant::OwnedObjectPath = proxy.call("CreateCall", &(properties)).await?;
        info!(
            path = %path,
            number = %number,
            is_emergency,
            "D-Bus ModemManager: call object created"
        );
        Ok(path.to_string())
    }

    async fn hangup_call(&mut self, call_id: &str) -> Result<()> {
        if self.simulated || self.dbus_connection.is_none() {
            if let Some(pos) = self
                .simulated_state
                .active_calls
                .iter()
                .position(|c| c.call_id == call_id)
            {
                self.simulated_state.active_calls.remove(pos);
                info!(call_id = %call_id, "Simulated ModemManager: hangup_call terminated call");
                Ok(())
            } else {
                Err(anyhow::anyhow!("Simulated call ID not found: {}", call_id))
            }
        } else {
            let conn = self
                .dbus_connection
                .as_ref()
                .context("D-Bus connection lost")?;
            let call_proxy = zbus::Proxy::new(
                conn,
                "org.freedesktop.ModemManager1",
                call_id,
                "org.freedesktop.ModemManager1.Call",
            )
            .await?;
            call_proxy.call::<_, _, ()>("Hangup", &()).await?;
            info!(call_id = %call_id, "D-Bus ModemManager: hangup_call executed");
            Ok(())
        }
    }

    async fn send_sms(&mut self, destination: &str, text: &str) -> Result<()> {
        if self.simulated || self.dbus_connection.is_none() {
            info!(
                destination = %destination,
                text_len = text.len(),
                "Simulated ModemManager: send_sms dispatched"
            );
            return Ok(());
        }

        let conn = self
            .dbus_connection
            .as_ref()
            .context("D-Bus connection lost")?;
        let sms_proxy = zbus::Proxy::new(
            conn,
            "org.freedesktop.ModemManager1",
            "/org/freedesktop/ModemManager1/Modem/0",
            "org.freedesktop.ModemManager1.Modem.Messaging",
        )
        .await?;

        let mut properties = std::collections::HashMap::new();
        properties.insert("number", zvariant::Value::from(destination));
        properties.insert("text", zvariant::Value::from(text));

        let path: zvariant::OwnedObjectPath = sms_proxy.call("Create", &(properties)).await?;
        let msg_proxy = zbus::Proxy::new(
            conn,
            "org.freedesktop.ModemManager1",
            path.to_string(),
            "org.freedesktop.ModemManager1.Sms",
        )
        .await?;
        msg_proxy.call::<_, _, ()>("Send", &()).await?;
        info!(
            destination = %destination,
            "D-Bus ModemManager: SMS sent successfully via D-Bus"
        );
        Ok(())
    }

    async fn pull_incoming_sms(&mut self) -> Result<Vec<SmsMessage>> {
        if self.simulated || self.dbus_connection.is_none() {
            let messages = std::mem::take(&mut self.simulated_state.sms_inbox);
            if !messages.is_empty() {
                info!(
                    count = messages.len(),
                    "Simulated ModemManager: delivered buffered incoming SMS"
                );
            }
            return Ok(messages);
        }

        Ok(Vec::new())
    }

    async fn list_active_calls(&mut self) -> Result<Vec<CallInfo>> {
        if self.simulated || self.dbus_connection.is_none() {
            return Ok(self.simulated_state.active_calls.clone());
        }
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_simulated_proxy_lifecycle() {
        let mut proxy = ModemManagerProxy::new_simulated();
        let status = proxy
            .get_registration_status()
            .await
            .expect("Failed to query registration status");
        assert_eq!(status, NetworkRegistrationStatus::RegisteredHome);

        let call_id = proxy
            .dial_call("555-0199", false)
            .await
            .expect("Failed to dial call");
        assert!(call_id.starts_with("call-sim-"));

        let active = proxy
            .list_active_calls()
            .await
            .expect("Failed to list active calls");
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].call_id, call_id);
        assert_eq!(active[0].state, CallState::Active);

        proxy
            .hangup_call(&call_id)
            .await
            .expect("Failed to hangup active call");

        let post_hangup = proxy
            .list_active_calls()
            .await
            .expect("Failed to list active calls post hangup");
        assert!(post_hangup.is_empty());

        proxy
            .send_sms("555-0199", "Test message")
            .await
            .expect("Failed to send SMS");

        let inbox = proxy
            .pull_incoming_sms()
            .await
            .expect("Failed to pull SMS inbox");
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].text, "Welcome to ZethraOS Telephony!");

        // Second pull should be empty as messages are consumed
        let inbox2 = proxy
            .pull_incoming_sms()
            .await
            .expect("Failed to pull SMS inbox on retry");
        assert!(inbox2.is_empty());
    }
}

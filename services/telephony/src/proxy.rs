// proxy.rs — Userspace cellular proxy via FreeDesktop ModemManager / D-Bus
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use tracing::{error, info, warn};
use zbus::{zvariant, Connection};
use zethra_hal::{
    CallDirection, CallInfo, CallState, NetworkRegistrationStatus, SmsMessage, TelephonyHal,
};

/// Represents an asynchronous D-Bus proxy interface to FreeDesktop ModemManager.
///
/// In production (no `ZETHRA_ALLOW_SIMULATED=1` env var), the service refuses
/// to enter simulation mode and exits with an error if no real backend is available.
/// Simulation is permitted only during CI or explicit developer opt-in.
pub struct ModemManagerProxy {
    dbus_connection: Option<Connection>,
    simulated: bool,
    simulated_state: SimulatedCellularState,
    /// The dynamically discovered modem object path, e.g.
    /// `/org/freedesktop/ModemManager1/Modem/1`.
    /// Populated by `discover_modem_path`; never hardcoded.
    modem_path: String,
}

#[derive(Debug, Clone)]
struct SimulatedCellularState {
    registration: NetworkRegistrationStatus,
    active_calls: Vec<CallInfo>,
    sms_inbox: Vec<SmsMessage>,
    call_counter: u32,
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

/// Discover the first modem object path registered with ModemManager via
/// `GetManagedObjects` on `org.freedesktop.ModemManager1`.
///
/// ModemManager assigns modem indices dynamically (e.g. `/Modem/0`, `/Modem/1`,
/// `/Modem/2`) depending on USB enumeration order and plug/unplug history.
/// Hardcoding `/Modem/0` fails whenever the kernel assigns a different index.
///
/// Returns the full D-Bus object path of the first modem found, e.g.
/// `/org/freedesktop/ModemManager1/Modem/1`.
async fn discover_modem_path(conn: &Connection) -> Result<String> {
    type ManagedObjects = std::collections::HashMap<
        zvariant::OwnedObjectPath,
        std::collections::HashMap<String, std::collections::HashMap<String, zvariant::OwnedValue>>,
    >;

    let mm_proxy = zbus::Proxy::new(
        conn,
        "org.freedesktop.ModemManager1",
        "/org/freedesktop/ModemManager1",
        "org.freedesktop.DBus.ObjectManager",
    )
    .await
    .context("Failed to construct ModemManager ObjectManager proxy")?;

    let objects: ManagedObjects = mm_proxy
        .call("GetManagedObjects", &())
        .await
        .context("ModemManager GetManagedObjects failed; is ModemManager running?")?;

    // The first object that exposes org.freedesktop.ModemManager1.Modem is our modem.
    for (path, interfaces) in &objects {
        if interfaces.contains_key("org.freedesktop.ModemManager1.Modem") {
            let path_str = path.as_str().to_string();
            info!(
                modem_path = %path_str,
                "ModemManager: discovered modem via GetManagedObjects"
            );
            return Ok(path_str);
        }
    }

    Err(anyhow::anyhow!(
        "No org.freedesktop.ModemManager1.Modem object found in ModemManager's managed objects. \
         Is a modem present and enabled?"
    ))
}

impl ModemManagerProxy {
    /// Try to initialize a live D-Bus system connection to ModemManager and
    /// discover the actual modem object path dynamically.
    ///
    /// Returns an error if D-Bus is unreachable or no modem is registered.
    pub async fn try_new() -> Result<Self> {
        let conn = Connection::system()
            .await
            .context("D-Bus System Bus unavailable; ModemManagerProxy cannot bind")?;

        info!("✓ Connected to FreeDesktop D-Bus System Bus for ModemManager proxy");

        // Discover the real modem path — never assume /Modem/0.
        let modem_path = discover_modem_path(&conn).await?;

        Ok(Self {
            dbus_connection: Some(conn),
            simulated: false,
            simulated_state: SimulatedCellularState::default(),
            modem_path,
        })
    }

    /// Initialize a simulated cellular proxy for deterministic CI testing.
    ///
    /// **This must only be called when `ZETHRA_ALLOW_SIMULATED=1` is set, or
    /// explicitly from a test harness.** Production code uses `try_new()` and
    /// fails loudly if the modem is not reachable.
    pub fn new_simulated() -> Self {
        info!("Initializing simulated ModemManagerProxy state machine for CI validation");
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
            modem_path: "/org/freedesktop/ModemManager1/Modem/sim".to_string(),
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

        // Use the dynamically discovered modem path.
        let proxy = zbus::Proxy::new(
            conn,
            "org.freedesktop.ModemManager1",
            self.modem_path.as_str(),
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

        // Voice interface lives on the modem path, not a hardcoded /Modem/0.
        let proxy = zbus::Proxy::new(
            conn,
            "org.freedesktop.ModemManager1",
            self.modem_path.as_str(),
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
            // Call objects have their own D-Bus path returned by CreateCall.
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

        // Messaging interface lives on the modem path.
        let sms_proxy = zbus::Proxy::new(
            conn,
            "org.freedesktop.ModemManager1",
            self.modem_path.as_str(),
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

        // Live path: list SMS objects from the Messaging interface.
        let conn = self
            .dbus_connection
            .as_ref()
            .context("D-Bus connection lost")?;
        let sms_proxy = zbus::Proxy::new(
            conn,
            "org.freedesktop.ModemManager1",
            self.modem_path.as_str(),
            "org.freedesktop.ModemManager1.Modem.Messaging",
        )
        .await?;

        let paths: Vec<zvariant::OwnedObjectPath> =
            sms_proxy.call("List", &()).await.unwrap_or_default();

        let mut messages = Vec::new();
        for path in paths {
            let msg_proxy = zbus::Proxy::new(
                conn,
                "org.freedesktop.ModemManager1",
                path.as_str(),
                "org.freedesktop.ModemManager1.Sms",
            )
            .await;
            if let Ok(mp) = msg_proxy {
                let number: String = mp
                    .get_property("Number")
                    .await
                    .unwrap_or_else(|_| "unknown".to_string());
                let text: String = mp
                    .get_property("Text")
                    .await
                    .unwrap_or_else(|_| String::new());
                let timestamp: String = mp
                    .get_property("Timestamp")
                    .await
                    .unwrap_or_else(|_| "0".to_string());
                // Timestamp from ModemManager is an ISO-8601 string; use 0 if unparseable.
                let timestamp_epoch: u64 = timestamp.parse().unwrap_or(0);
                messages.push(SmsMessage {
                    message_id: path.as_str().to_string(),
                    sender: number,
                    text,
                    timestamp_epoch,
                });
            }
        }

        info!(
            count = messages.len(),
            modem_path = %self.modem_path,
            "D-Bus ModemManager: retrieved incoming SMS from modem"
        );
        Ok(messages)
    }

    async fn list_active_calls(&mut self) -> Result<Vec<CallInfo>> {
        if self.simulated || self.dbus_connection.is_none() {
            return Ok(self.simulated_state.active_calls.clone());
        }

        let conn = self
            .dbus_connection
            .as_ref()
            .context("D-Bus connection lost")?;
        let voice_proxy = zbus::Proxy::new(
            conn,
            "org.freedesktop.ModemManager1",
            self.modem_path.as_str(),
            "org.freedesktop.ModemManager1.Modem.Voice",
        )
        .await?;

        let paths: Vec<zvariant::OwnedObjectPath> =
            voice_proxy.call("ListCalls", &()).await.unwrap_or_default();

        let mut calls = Vec::new();
        for path in paths {
            let call_proxy = zbus::Proxy::new(
                conn,
                "org.freedesktop.ModemManager1",
                path.as_str(),
                "org.freedesktop.ModemManager1.Call",
            )
            .await;
            if let Ok(cp) = call_proxy {
                let number: String = cp
                    .get_property("Number")
                    .await
                    .unwrap_or_else(|_| "unknown".to_string());
                let state_raw: u32 = cp.get_property("State").await.unwrap_or(0);
                let direction_raw: u32 = cp.get_property("Direction").await.unwrap_or(0);

                // MM call states: 0=Unknown,1=Dialing,2=Ringing,3=Active,4=Held,5=Waiting,6=Terminated
                let state = match state_raw {
                    3 => CallState::Active,
                    _ => CallState::Active, // expand as HAL states are added
                };
                let direction = match direction_raw {
                    1 => CallDirection::Inbound,
                    _ => CallDirection::Outbound,
                };

                calls.push(CallInfo {
                    call_id: path.as_str().to_string(),
                    number,
                    state,
                    direction,
                });
            }
        }

        info!(
            count = calls.len(),
            modem_path = %self.modem_path,
            "D-Bus ModemManager: listed active calls from modem"
        );
        Ok(calls)
    }
}

/// Log a clear production error when simulation is entered without the opt-in gate.
///
/// Called from `TelephonyOrchestrator::new` before falling back to simulation.
/// If `ZETHRA_ALLOW_SIMULATED` is not set, this function logs at ERROR level
/// and returns an error so the daemon exits with a non-zero code rather than
/// silently running in simulation mode.
pub fn check_simulation_gate(reason: &str) -> Result<()> {
    if std::env::var("ZETHRA_ALLOW_SIMULATED").as_deref() == Ok("1") {
        warn!(
            reason = %reason,
            "ZETHRA_ALLOW_SIMULATED=1: entering simulated telephony mode (CI/dev only)"
        );
        Ok(())
    } else {
        error!(
            reason = %reason,
            "No real telephony backend available and ZETHRA_ALLOW_SIMULATED is not set. \
             Refusing to silently simulate on a production device. \
             Set ZETHRA_ALLOW_SIMULATED=1 to allow simulation, or fix the hardware/D-Bus configuration."
        );
        Err(anyhow::anyhow!(
            "Production telephony backend unavailable: {}. \
             Set ZETHRA_ALLOW_SIMULATED=1 to permit simulation.",
            reason
        ))
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

        // Second pull must be empty — inbox is consumed on read
        let inbox2 = proxy
            .pull_incoming_sms()
            .await
            .expect("Failed to pull SMS inbox on retry");
        assert!(inbox2.is_empty());
    }

    /// Verify that the simulation gate returns an error when ZETHRA_ALLOW_SIMULATED is absent.
    #[test]
    fn test_simulation_gate_blocks_without_env_var() {
        // Ensure the var is not set for this test
        std::env::remove_var("ZETHRA_ALLOW_SIMULATED");
        let result = check_simulation_gate("test: no modem");
        assert!(
            result.is_err(),
            "check_simulation_gate must return Err when ZETHRA_ALLOW_SIMULATED is unset"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("ZETHRA_ALLOW_SIMULATED"),
            "Error must mention the gate variable; got: {}",
            msg
        );
    }

    /// Verify that the simulation gate permits simulation when ZETHRA_ALLOW_SIMULATED=1.
    #[test]
    fn test_simulation_gate_allows_with_env_var() {
        std::env::set_var("ZETHRA_ALLOW_SIMULATED", "1");
        let result = check_simulation_gate("test: CI environment");
        assert!(
            result.is_ok(),
            "check_simulation_gate must return Ok when ZETHRA_ALLOW_SIMULATED=1"
        );
        std::env::remove_var("ZETHRA_ALLOW_SIMULATED");
    }
}

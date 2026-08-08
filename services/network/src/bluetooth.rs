// bluetooth.rs — Userspace Bluetooth proxy via FreeDesktop BlueZ / D-Bus
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use tracing::{info, warn};
use zbus::{zvariant, Connection};
use zethra_hal::{BluetoothAdapter, BluetoothAdapterState, BluetoothDevice, BluetoothHal};

/// Default duration to hold an active BlueZ discovery session so that nearby
/// devices have time to broadcast at least one advertisement packet.
/// Classic BT inquiry time is a multiple of 1.28 s; 10 s covers ≥ 7 rounds.
const DEFAULT_SCAN_WINDOW: std::time::Duration = std::time::Duration::from_secs(10);

/// Represents an asynchronous D-Bus proxy interface to FreeDesktop BlueZ daemons.
/// Falls back to an in-memory simulated state machine when D-Bus is unavailable
/// or during automated CI validation.
pub struct BluezProxy {
    dbus_connection: Option<Connection>,
    simulated: bool,
    simulated_state: SimulatedBluetoothState,
    /// How long to hold the BlueZ discovery session open before querying.
    /// Configurable so callers (including tests that want a zero-duration window
    /// for the live D-Bus path) can inject a shorter value.
    scan_window: std::time::Duration,
}

#[derive(Debug, Clone)]
struct SimulatedBluetoothState {
    adapters: Vec<BluetoothAdapter>,
    devices: Vec<BluetoothDevice>,
}

impl Default for SimulatedBluetoothState {
    fn default() -> Self {
        Self {
            adapters: vec![BluetoothAdapter {
                adapter_id: "hci0".to_string(),
                address: "00:11:22:33:44:55".to_string(),
                name: "Zethra-HCI".to_string(),
                state: BluetoothAdapterState::PoweredOn,
                discoverable: true,
            }],
            devices: vec![
                BluetoothDevice {
                    address: "AA:BB:CC:DD:EE:01".to_string(),
                    name: "Zethra-Pencil".to_string(),
                    rssi: -45,
                    is_connected: false,
                },
                BluetoothDevice {
                    address: "AA:BB:CC:DD:EE:02".to_string(),
                    name: "Audio-Pods".to_string(),
                    rssi: -62,
                    is_connected: false,
                },
            ],
        }
    }
}

impl BluezProxy {
    /// Try to initialize a live D-Bus system connection to BlueZ.
    pub async fn try_new() -> Result<Self> {
        match Connection::system().await {
            Ok(conn) => {
                info!("✓ Connected to FreeDesktop D-Bus System Bus for BlueZ proxy");
                Ok(Self {
                    dbus_connection: Some(conn),
                    simulated: false,
                    simulated_state: SimulatedBluetoothState::default(),
                    scan_window: DEFAULT_SCAN_WINDOW,
                })
            }
            Err(e) => {
                warn!(error = %e, "D-Bus system bus unreachable for BlueZ");
                Err(anyhow::anyhow!("D-Bus system bus unreachable: {}", e))
            }
        }
    }

    /// Initialize a simulated BlueZ proxy directly for zero-hardware CI validation.
    pub fn new_simulated() -> Self {
        info!("Initializing Simulated BlueZ Proxy for deterministic CI validation");
        Self {
            dbus_connection: None,
            simulated: true,
            simulated_state: SimulatedBluetoothState::default(),
            scan_window: DEFAULT_SCAN_WINDOW,
        }
    }

    /// Try initializing D-Bus, gracefully reverting to simulation if D-Bus is unavailable.
    pub async fn try_new_with_fallback() -> Self {
        match Self::try_new().await {
            Ok(proxy) => proxy,
            Err(e) => {
                warn!(
                    error = %e,
                    "Failed to bind BlueZ D-Bus; switching to simulated test fallback"
                );
                Self::new_simulated()
            }
        }
    }

    /// Override the discovery dwell window.
    ///
    /// Used by callers that need a non-default window duration, including
    /// integration tests that verify the live-path proxy construction and
    /// D-Bus sequencing without waiting the full 10 s.
    #[allow(dead_code)]
    pub fn with_scan_window(mut self, window: std::time::Duration) -> Self {
        self.scan_window = window;
        self
    }
}

/// Shared type alias for the BlueZ `GetManagedObjects` response.
type ManagedObjects = std::collections::HashMap<
    zvariant::OwnedObjectPath,
    std::collections::HashMap<String, std::collections::HashMap<String, zvariant::OwnedValue>>,
>;

/// Parse `org.bluez.Device1` entries out of a BlueZ `GetManagedObjects` response.
///
/// Extracted as a free function so it can be tested without a D-Bus connection.
fn parse_bluez_devices(objects: ManagedObjects) -> Vec<BluetoothDevice> {
    let mut devices = Vec::new();
    for (_, interfaces) in objects {
        if let Some(props) = interfaces.get("org.bluez.Device1") {
            let address = props
                .get("Address")
                .and_then(|v| v.downcast_ref::<String>().ok())
                .unwrap_or_else(|| "00:00:00:00:00:00".to_string());
            let name = props
                .get("Name")
                .and_then(|v| v.downcast_ref::<String>().ok())
                .unwrap_or_else(|| "Unknown Device".to_string());
            let rssi = props
                .get("RSSI")
                .and_then(|v| v.downcast_ref::<i16>().ok())
                .map(|v| v as i32)
                .unwrap_or(-80);
            let is_connected = props
                .get("Connected")
                .and_then(|v| v.downcast_ref::<bool>().ok())
                .unwrap_or(false);

            devices.push(BluetoothDevice {
                address,
                name,
                rssi,
                is_connected,
            });
        }
    }
    devices
}

#[async_trait::async_trait]
impl BluetoothHal for BluezProxy {
    async fn list_adapters(&mut self) -> Result<Vec<BluetoothAdapter>> {
        if self.simulated || self.dbus_connection.is_none() {
            info!("Simulated BlueZ: enumerating local Bluetooth adapters");
            return Ok(self.simulated_state.adapters.clone());
        }

        let conn = self
            .dbus_connection
            .as_ref()
            .context("D-Bus connection lost")?;
        let proxy =
            zbus::Proxy::new(conn, "org.bluez", "/", "org.freedesktop.DBus.ObjectManager").await?;

        let mut adapters = Vec::new();
        match proxy
            .call::<_, _, ManagedObjects>("GetManagedObjects", &())
            .await
        {
            Ok(objects) => {
                for (path, interfaces) in objects {
                    if let Some(props) = interfaces.get("org.bluez.Adapter1") {
                        let path_str = path.as_str();
                        let adapter_id = path_str.rsplit('/').next().unwrap_or("hci0").to_string();
                        let address = props
                            .get("Address")
                            .and_then(|v| v.downcast_ref::<String>().ok())
                            .unwrap_or_else(|| "00:00:00:00:00:00".to_string());
                        let name = props
                            .get("Name")
                            .and_then(|v| v.downcast_ref::<String>().ok())
                            .unwrap_or_else(|| adapter_id.clone());
                        let powered = props
                            .get("Powered")
                            .and_then(|v| v.downcast_ref::<bool>().ok())
                            .unwrap_or(false);
                        let discoverable = props
                            .get("Discoverable")
                            .and_then(|v| v.downcast_ref::<bool>().ok())
                            .unwrap_or(false);

                        let state = if powered {
                            BluetoothAdapterState::PoweredOn
                        } else {
                            BluetoothAdapterState::PoweredOff
                        };

                        adapters.push(BluetoothAdapter {
                            adapter_id,
                            address,
                            name,
                            state,
                            discoverable,
                        });
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "Failed to query BlueZ managed objects; returning empty adapter list");
            }
        }

        info!(
            count = adapters.len(),
            "D-Bus BlueZ: enumerated local adapters"
        );
        Ok(adapters)
    }

    async fn set_powered(&mut self, adapter_id: &str, powered: bool) -> Result<()> {
        if self.simulated || self.dbus_connection.is_none() {
            if let Some(adapter) = self
                .simulated_state
                .adapters
                .iter_mut()
                .find(|a| a.adapter_id == adapter_id)
            {
                adapter.state = if powered {
                    BluetoothAdapterState::PoweredOn
                } else {
                    BluetoothAdapterState::PoweredOff
                };
                info!(
                    adapter_id = %adapter_id,
                    powered,
                    "Simulated BlueZ: updated adapter powered state"
                );
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Simulated adapter ID not found: {}",
                    adapter_id
                ))
            }
        } else {
            let conn = self
                .dbus_connection
                .as_ref()
                .context("D-Bus connection lost")?;
            let path = format!("/org/bluez/{}", adapter_id);
            let adapter_proxy =
                zbus::Proxy::new(conn, "org.bluez", path.as_str(), "org.bluez.Adapter1").await?;

            adapter_proxy
                .set_property("Powered", zvariant::Value::from(powered))
                .await?;
            info!(adapter_id = %adapter_id, powered, "D-Bus BlueZ: set adapter powered state");
            Ok(())
        }
    }

    /// Scan for nearby Bluetooth devices via BlueZ.
    ///
    /// # Live path sequence (D-Bus available)
    ///
    /// All D-Bus proxy objects are constructed **before** `StartDiscovery` is
    /// called so that no fallible operation can leave discovery running without
    /// a corresponding `StopDiscovery`.
    ///
    /// 1. Construct `org.bluez.Adapter1` and `org.freedesktop.DBus.ObjectManager`
    ///    proxies.  If either fails the function returns immediately with no
    ///    discovery session open.
    ///
    /// 2. `StartDiscovery` — starts active scanning on the named adapter.
    ///    Failure is a hard error returned to the caller; we do **not** return
    ///    `Ok([])` because an empty list would be ambiguous with "no devices
    ///    found during a successful scan".
    ///
    /// 3. `sleep(scan_window)` — suspends the task so BlueZ can populate its
    ///    internal device cache from received advertisement packets.  Only an
    ///    immutable `&Connection` is held across this await point.
    ///
    /// 4. `GetManagedObjects` — queries the device cache after the dwell window.
    ///    The result is stored; errors are surfaced only after `StopDiscovery`.
    ///
    /// 5. `StopDiscovery` — attempted unconditionally after step 2 succeeds,
    ///    regardless of the outcome of steps 3 and 4.  A `StopDiscovery`
    ///    failure is logged as a warning and does **not** suppress the scan
    ///    result.  The service policy is best-effort cleanup: we always try,
    ///    we always log the outcome, and we do not treat a failed stop as
    ///    fatal to the scan result.  Whether BlueZ internally expires the
    ///    session after a StopDiscovery failure is implementation-defined and
    ///    not relied upon here.
    ///
    /// # Cancellation safety
    ///
    /// This future is not cancellation-safe. If it is dropped after
    /// `StartDiscovery` succeeds, `StopDiscovery` may not run. The current API
    /// does not provide an asynchronous cancellation cleanup hook, so callers
    /// must avoid aborting an active scan or use a higher-level lifecycle
    /// wrapper that owns cleanup.
    ///
    /// # Simulated path (CI / no D-Bus)
    ///
    /// Returns the preset device list immediately without touching D-Bus.
    /// The StartDiscovery → sleep → StopDiscovery lifecycle is not exercised;
    /// that remains an integration-test concern requiring a real BlueZ daemon.
    async fn scan_devices(&mut self, adapter_id: &str) -> Result<Vec<BluetoothDevice>> {
        if self.simulated || self.dbus_connection.is_none() {
            info!(adapter_id = %adapter_id, "Simulated BlueZ: returning simulated peripheral devices");
            return Ok(self.simulated_state.devices.clone());
        }

        // Borrow the connection immutably for the duration of this call.
        // No mutable access to `self` is needed after this point.
        let conn = self
            .dbus_connection
            .as_ref()
            .context("D-Bus connection lost")?;

        let scan_window = self.scan_window;

        // ── Step 1: Construct all proxies before starting discovery ───────────
        //
        // Both proxy objects are built here, before StartDiscovery, so that no
        // fallible D-Bus operation can fire after discovery is active.  If
        // either construction fails the function returns immediately with no
        // discovery session open.
        let adapter_path = format!("/org/bluez/{}", adapter_id);
        let adapter_proxy = zbus::Proxy::new(
            conn,
            "org.bluez",
            adapter_path.as_str(),
            "org.bluez.Adapter1",
        )
        .await
        .with_context(|| {
            format!(
                "BlueZ: failed to construct Adapter1 proxy for '{}'",
                adapter_id
            )
        })?;

        let om_proxy =
            zbus::Proxy::new(conn, "org.bluez", "/", "org.freedesktop.DBus.ObjectManager")
                .await
                .context("BlueZ: failed to construct ObjectManager proxy")?;

        // ── Step 2: StartDiscovery ────────────────────────────────────────────
        adapter_proxy
            .call::<_, _, ()>("StartDiscovery", &())
            .await
            .with_context(|| {
                format!(
                    "BlueZ: StartDiscovery failed for adapter '{}'; \
                     check that the adapter is powered on and not already scanning",
                    adapter_id
                )
            })?;
        info!(
            adapter_id = %adapter_id,
            scan_secs = scan_window.as_secs(),
            "BlueZ: StartDiscovery active; waiting for advertisement dwell window"
        );

        // ── Step 3: Dwell window ──────────────────────────────────────────────
        //
        // Suspend the task so BlueZ can populate its device cache.  Only the
        // immutable &Connection (via the proxy objects) is held across this
        // await point.
        //
        // CANCELLATION NOTE: if this future is dropped while the sleep is
        // pending, execution stops here and StopDiscovery is never called.
        // See the function-level doc comment for the caller's responsibility.
        tokio::time::sleep(scan_window).await;

        // ── Step 4: GetManagedObjects ─────────────────────────────────────────
        //
        // Query the device cache assembled during the dwell window.  The result
        // is stored — not immediately propagated — so StopDiscovery can still
        // run regardless of whether this call succeeds.
        let objects_result = om_proxy
            .call::<_, _, ManagedObjects>("GetManagedObjects", &())
            .await;

        // ── Step 5: StopDiscovery (unconditional after Step 2 succeeded) ──────
        //
        // Policy: always attempt StopDiscovery; always log the outcome; never
        // suppress the scan result because of a StopDiscovery failure.
        // We do not assert anything about BlueZ's internal session-expiry
        // behaviour on StopDiscovery failure — that is implementation-defined.
        match adapter_proxy.call::<_, _, ()>("StopDiscovery", &()).await {
            Ok(()) => info!(adapter_id = %adapter_id, "BlueZ: StopDiscovery completed"),
            Err(e) => warn!(
                error = %e,
                adapter_id = %adapter_id,
                "BlueZ: StopDiscovery failed; the discovery session may remain \
                 active until BlueZ applies its own management policy"
            ),
        }

        // ── Step 6: Surface the GetManagedObjects result ──────────────────────
        //
        // Only now — after StopDiscovery has been attempted — do we propagate
        // any error from the GetManagedObjects call.
        let objects =
            objects_result.context("BlueZ: GetManagedObjects failed after scan window")?;

        let devices = parse_bluez_devices(objects);
        info!(
            adapter_id = %adapter_id,
            count = devices.len(),
            "D-Bus BlueZ: scan complete"
        );
        Ok(devices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Simulated-path unit tests ────────────────────────────────────────────
    //
    // All tests below exercise the simulated branch of BluezProxy.
    // The live BlueZ discovery lifecycle (proxy construction → StartDiscovery
    // → sleep → GetManagedObjects → StopDiscovery) is an integration concern
    // that requires a running BlueZ daemon on the system D-Bus and is not
    // exercised in these unit tests.

    #[tokio::test]
    async fn test_simulated_bluez_lifecycle() {
        let mut proxy = BluezProxy::new_simulated();

        // 1. Enumerate adapters
        let adapters = proxy
            .list_adapters()
            .await
            .expect("Failed to list adapters");
        assert_eq!(adapters.len(), 1);
        assert_eq!(adapters[0].adapter_id, "hci0");
        assert_eq!(adapters[0].state, BluetoothAdapterState::PoweredOn);

        // 2. Scan devices — simulated path returns the preset list immediately.
        //    No D-Bus is contacted; StartDiscovery/StopDiscovery/sleep are not called.
        let devices = proxy
            .scan_devices("hci0")
            .await
            .expect("Failed to scan devices");
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].name, "Zethra-Pencil");
        assert_eq!(devices[1].name, "Audio-Pods");

        // 3. Toggle power off and confirm state is reflected
        proxy
            .set_powered("hci0", false)
            .await
            .expect("Failed to power off adapter");
        let adapters_after = proxy
            .list_adapters()
            .await
            .expect("Failed to query adapters after power-off");
        assert_eq!(adapters_after[0].state, BluetoothAdapterState::PoweredOff);
    }

    /// Regression: simulated scan_devices is idempotent — successive calls
    /// return the same device list without consuming or mutating state.
    /// This mirrors the BlueZ object-cache model where GetManagedObjects is
    /// non-destructive.
    #[tokio::test]
    async fn test_simulated_scan_devices_idempotent() {
        let mut proxy = BluezProxy::new_simulated();

        let first = proxy.scan_devices("hci0").await.expect("First scan failed");
        let second = proxy
            .scan_devices("hci0")
            .await
            .expect("Second scan failed");

        assert_eq!(
            first.len(),
            second.len(),
            "Simulated scan_devices must be idempotent"
        );
        assert_eq!(
            first[0].address, second[0].address,
            "Device addresses must be stable across scans"
        );
    }

    /// Regression: set_powered on an unknown adapter ID returns an error in
    /// simulated mode, matching the BlueZ `UnknownObject` D-Bus error.
    #[tokio::test]
    async fn test_simulated_set_powered_unknown_adapter_returns_error() {
        let mut proxy = BluezProxy::new_simulated();

        let result = proxy.set_powered("hci99", true).await;
        assert!(
            result.is_err(),
            "set_powered on unknown adapter must return an error"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("hci99"),
            "Error message must name the unknown adapter; got: {}",
            msg
        );
    }

    /// Regression: independent proxy instances do not share simulated state.
    /// Each instance owns its own SimulatedBluetoothState so mutations in one
    /// proxy must not affect another.
    #[tokio::test]
    async fn test_simulated_proxy_instances_are_independent() {
        let mut proxy_a = BluezProxy::new_simulated();
        let mut proxy_b = BluezProxy::new_simulated();

        proxy_a
            .set_powered("hci0", false)
            .await
            .expect("Failed to power off in proxy_a");

        let a_adapters = proxy_a.list_adapters().await.unwrap();
        let b_adapters = proxy_b.list_adapters().await.unwrap();

        assert_eq!(
            a_adapters[0].state,
            BluetoothAdapterState::PoweredOff,
            "proxy_a should reflect the power-off"
        );
        assert_eq!(
            b_adapters[0].state,
            BluetoothAdapterState::PoweredOn,
            "proxy_b must not be affected by proxy_a state changes"
        );
    }

    /// Test that with_scan_window correctly overrides the dwell duration so
    /// callers can inject a non-default window.
    #[test]
    fn test_with_scan_window_overrides_default() {
        let proxy =
            BluezProxy::new_simulated().with_scan_window(std::time::Duration::from_millis(100));
        assert_eq!(
            proxy.scan_window,
            std::time::Duration::from_millis(100),
            "with_scan_window must store the provided duration"
        );
    }

    /// Test that the default scan window matches the documented constant.
    #[test]
    fn test_default_scan_window_is_ten_seconds() {
        let proxy = BluezProxy::new_simulated();
        assert_eq!(
            proxy.scan_window, DEFAULT_SCAN_WINDOW,
            "Default scan window must equal DEFAULT_SCAN_WINDOW"
        );
        assert_eq!(
            proxy.scan_window.as_secs(),
            10,
            "Default scan window must be 10 seconds"
        );
    }

    // ── parse_bluez_devices unit tests ───────────────────────────────────────
    //
    // parse_bluez_devices is a pure function that does not require D-Bus.
    // These tests verify the parsing logic for the GetManagedObjects response
    // that would be received from a live BlueZ daemon after a scan window.

    /// An empty managed-objects map produces an empty device list.
    #[test]
    fn test_parse_bluez_devices_empty_map_yields_no_devices() {
        let objects = std::collections::HashMap::new();
        let devices = parse_bluez_devices(objects);
        assert!(
            devices.is_empty(),
            "Empty managed-objects map must yield no devices"
        );
    }

    /// Objects that have no org.bluez.Device1 interface are ignored.
    /// This simulates adapter-only entries in the managed-objects response.
    #[test]
    fn test_parse_bluez_devices_skips_non_device_objects() {
        let mut objects = std::collections::HashMap::new();
        // Insert an org.bluez.Adapter1 entry with no Device1 interface
        let mut adapter_ifaces: std::collections::HashMap<
            String,
            std::collections::HashMap<String, zvariant::OwnedValue>,
        > = std::collections::HashMap::new();
        adapter_ifaces.insert("org.bluez.Adapter1".to_string(), Default::default());
        objects.insert(
            zvariant::OwnedObjectPath::try_from("/org/bluez/hci0").unwrap(),
            adapter_ifaces,
        );

        let devices = parse_bluez_devices(objects);
        assert!(
            devices.is_empty(),
            "Adapter-only objects must not produce Device entries"
        );
    }

    // ── Live-path contract documentation ────────────────────────────────────
    //
    // The following properties of the live scan_devices path are stated here
    // as documentation. They cannot be unit-tested without a real BlueZ daemon
    // and are validated by integration tests on target hardware.
    //
    // PROPERTY 1 — All proxy construction occurs before StartDiscovery.
    //   Both the Adapter1 proxy and the ObjectManager proxy are built before
    //   StartDiscovery is called. If either construction fails, the function
    //   returns an error with no discovery session open.
    //
    // PROPERTY 2 — StartDiscovery failure is a hard error.
    //   scan_devices returns Err(...). It does NOT return Ok(vec![]).
    //
    // PROPERTY 3 — StopDiscovery is attempted on every normal and error return
    //   path after StartDiscovery succeeds, including GetManagedObjects failure.
    //   It is NOT called if the future is dropped mid-sleep (cancellation).
    //
    // PROPERTY 4 — StopDiscovery failure does not suppress the scan result.
    //   The failure is logged as a warning. The service makes no assumption
    //   about BlueZ's internal behaviour on a failed StopDiscovery call.
    //
    // PROPERTY 5 — Cancellation safety.
    //   This future is not cancellation-safe. If it is dropped after
    //   StartDiscovery succeeds, StopDiscovery may not run. The current API
    //   does not provide an asynchronous cancellation cleanup hook, so callers
    //   must avoid aborting an active scan or use a higher-level lifecycle
    //   wrapper that owns cleanup.
}

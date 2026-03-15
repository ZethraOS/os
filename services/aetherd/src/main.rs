// aetherd — AetherOS Init System (PID 1)
// SPDX-License-Identifier: Apache-2.0
//
// LOCAL DEV BUILD: filesystem mounts are stubbed out.
// On real device: uncomment nix calls in mount_essential_filesystems().

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};

// ─── Unit file format ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct Unit {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub after: Vec<String>,
    pub exec_start: String,
    pub restart: RestartPolicy,
    pub restart_delay_ms: Option<u64>,
    pub watchdog_sec: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RestartPolicy {
    Always,
    OnFailure,
    Never,
}

// ─── Service state ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum ServiceState {
    Pending,
    Starting,
    Running { pid: u32 },
    Failed { exit_code: i32, restarts: u32 },
    Stopped,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceStatus {
    pub name: String,
    pub state: ServiceState,
    pub restarts: u32,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
}

use chrono::Utc;

#[derive(Debug)]
pub enum SupervisorEvent {
    ServiceExited { name: String, exit_code: i32 },
    HealthCheck,
    Shutdown,
}

pub struct Supervisor {
    units: HashMap<String, Unit>,
    statuses: HashMap<String, ServiceStatus>,
    event_tx: mpsc::Sender<SupervisorEvent>,
    event_rx: mpsc::Receiver<SupervisorEvent>,
}

impl Supervisor {
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::channel(128);
        Self {
            units: HashMap::new(),
            statuses: HashMap::new(),
            event_tx,
            event_rx,
        }
    }

    pub fn load_units(&mut self, units_dir: &Path) -> Result<()> {
        let pattern = units_dir.join("*.unit.toml");
        for entry in glob::glob(pattern.to_str().unwrap())
            .context("invalid units glob")?
            .flatten()
        {
            let contents = std::fs::read_to_string(&entry)?;
            let unit: Unit =
                toml::from_str(&contents).with_context(|| format!("parsing unit {:?}", entry))?;
            info!(name = %unit.name, "loaded unit");
            self.statuses.insert(
                unit.name.clone(),
                ServiceStatus {
                    name: unit.name.clone(),
                    state: ServiceState::Pending,
                    restarts: 0,
                    started_at: None,
                },
            );
            self.units.insert(unit.name.clone(), unit);
        }
        Ok(())
    }

    fn boot_order(&self) -> Vec<String> {
        let mut visited: std::collections::HashSet<String> = Default::default();
        let mut order: Vec<String> = Vec::new();
        fn visit(
            name: &str,
            units: &HashMap<String, Unit>,
            visited: &mut std::collections::HashSet<String>,
            order: &mut Vec<String>,
        ) {
            if visited.contains(name) {
                return;
            }
            visited.insert(name.to_string());
            if let Some(unit) = units.get(name) {
                for dep in &unit.after {
                    visit(dep, units, visited, order);
                }
            }
            order.push(name.to_string());
        }
        let names: Vec<String> = self.units.keys().cloned().collect();
        for name in &names {
            visit(name, &self.units, &mut visited, &mut order);
        }
        order
    }

    async fn spawn_service(&mut self, name: &str) -> Result<()> {
        let unit = self.units.get(name).context("unit not found")?.clone();
        info!(name, exec = %unit.exec_start, "starting service");
        let parts: Vec<&str> = unit.exec_start.split_whitespace().collect();
        let (cmd, args) = parts.split_first().context("empty exec_start")?;
        let tx = self.event_tx.clone();
        let service_name = name.to_string();
        let restart_delay = unit.restart_delay_ms.unwrap_or(1000);
        let policy = unit.restart.clone();

        let child = Command::new(cmd)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("spawning {}", unit.exec_start))?;

        let pid = child.id().unwrap_or(0);
        if let Some(status) = self.statuses.get_mut(name) {
            status.state = ServiceState::Running { pid };
            status.started_at = Some(Utc::now());
        }

        tokio::spawn(async move {
            let output = child.wait_with_output().await;
            let exit_code = output.map(|o| o.status.code().unwrap_or(-1)).unwrap_or(-1);
            warn!(name = %service_name, exit_code, "service exited");
            if policy != RestartPolicy::Never {
                sleep(Duration::from_millis(restart_delay)).await;
            }
            let _ = tx
                .send(SupervisorEvent::ServiceExited {
                    name: service_name,
                    exit_code,
                })
                .await;
        });
        Ok(())
    }

    async fn handle_exit(&mut self, name: &str, exit_code: i32) {
        let should_restart = if let Some(status) = self.statuses.get_mut(name) {
            status.restarts += 1;
            let r = status.restarts;
            let policy = self
                .units
                .get(name)
                .map(|u| u.restart.clone())
                .unwrap_or(RestartPolicy::Never);
            status.state = ServiceState::Failed {
                exit_code,
                restarts: r,
            };
            policy == RestartPolicy::Always
                || (policy == RestartPolicy::OnFailure && exit_code != 0)
        } else {
            false
        };
        if should_restart {
            if let Err(e) = self.spawn_service(name).await {
                error!(name, error = %e, "restart failed");
            }
        }
    }

    async fn emit_health_report(&self) {
        let report: Vec<&ServiceStatus> = self.statuses.values().collect();
        if let Ok(json) = serde_json::to_string_pretty(&report) {
            let dir = std::path::Path::new("/tmp/aether");
            let _ = std::fs::create_dir_all(dir);
            let _ = tokio::fs::write("/tmp/aether/health.json", json).await;
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        info!("aetherd: PID {}", std::process::id());

        let units_dir = PathBuf::from(
            std::env::var("AETHER_UNITS_DIR").unwrap_or_else(|_| "build/configs/units".to_string()),
        );

        if units_dir.exists() {
            self.load_units(&units_dir)?;
            info!("loaded {} units", self.units.len());
        } else {
            warn!(
                "units dir {:?} not found — running dry (no services to start)",
                units_dir
            );
        }

        for name in self.boot_order() {
            if let Err(e) = self.spawn_service(&name).await {
                error!(name, error = %e, "failed to start service — continuing");
            }
        }

        let tx = self.event_tx.clone();
        tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(30)).await;
                let _ = tx.send(SupervisorEvent::HealthCheck).await;
            }
        });

        loop {
            match self.event_rx.recv().await {
                Some(SupervisorEvent::ServiceExited { name, exit_code }) => {
                    self.handle_exit(&name, exit_code).await;
                }
                Some(SupervisorEvent::HealthCheck) => {
                    self.emit_health_report().await;
                }
                Some(SupervisorEvent::Shutdown) | None => {
                    info!("aetherd shutting down");
                    break;
                }
            }
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "aetherd=info".to_string()))
        .init();
    info!("AetherOS init system starting");
    let mut supervisor = Supervisor::new();
    supervisor.run().await
}

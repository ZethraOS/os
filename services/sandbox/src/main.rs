// main.rs — AetherOS Sandbox Orchestrator
// SPDX-License-Identifier: Apache-2.0

mod runtime;
mod permissions;
mod ipc;
mod lifecycle;

use anyhow::Result;
use tracing::info;
use crate::runtime::SandboxRuntime;
use crate::lifecycle::{LifecycleManager, AppState};
use crate::ipc::SandboxIPC;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()))
        .init();

    info!("AetherOS Sandbox Service starting");

    let runtime = SandboxRuntime::new()?;
    let mut lifecycle = LifecycleManager::new();
    let ipc = SandboxIPC::new("demo-app");

    lifecycle.set_state(AppState::Running);
    
    // Spawn IPC bridge
    tokio::spawn(async move {
        let _ = ipc.run_bridge().await;
    });

    // Simulated app run (logic would come from manager/OS core)
    let wasm_bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]; // Minimal WASM header
    if let Err(e) = runtime.load_and_run(&wasm_bytes, 1000).await {
        info!("Simulated run failed (expected): {}", e);
    }

    // Keep service alive
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
    }
}

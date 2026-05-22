// ipc.rs — Unix socket bridge for ZethraOS sandboxed apps
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tracing::{error, info};

pub struct SandboxIPC {
    socket_path: String,
}

impl SandboxIPC {
    pub fn new(app_id: &str) -> Self {
        Self {
            socket_path: format!("/run/zethra/sandbox/{}.sock", app_id),
        }
    }

    pub async fn run_bridge(&self) -> Result<()> {
        let _ = std::fs::remove_file(&self.socket_path);
        let listener = UnixListener::bind(&self.socket_path)?;
        info!(path = %self.socket_path, "Sandbox IPC bridge ready");

        loop {
            match listener.accept().await {
                Ok((mut stream, _)) => {
                    tokio::spawn(async move {
                        let mut buf = [0u8; 1024];
                        if let Ok(n) = stream.read(&mut buf).await {
                            info!(
                                "Received message from sandboxed app: {}",
                                String::from_utf8_lossy(&buf[..n])
                            );
                            // In real impl, would route this to relevant OS service based on message type
                            let _ = stream.write_all(b"{\"status\":\"ack\"}").await;
                        }
                    });
                }
                Err(e) => error!("IPC bridge error: {}", e),
            }
        }
    }
}

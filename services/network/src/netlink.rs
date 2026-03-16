// netlink.rs — Netlink monitor for AetherOS Network
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
#[cfg(target_os = "linux")]
use futures_util::stream::StreamExt;
#[cfg(target_os = "linux")]
use rtnetlink::new_connection;
#[cfg(target_os = "linux")]
use tracing::info;
#[cfg(not(target_os = "linux"))]
use tracing::warn;

pub struct NetlinkMonitor;

impl NetlinkMonitor {
    #[cfg(target_os = "linux")]
    pub async fn run() -> Result<()> {
        let (conn, _handle, mut messages) = new_connection()?;
        tokio::spawn(conn);

        info!("Netlink monitor started (Linux)");

        while let Some((message, _metadata)) = messages.next().await {
            match message.payload {
                rtnetlink::packet_core::NetlinkPayload::InnerMessage(
                    rtnetlink::packet_route::RtnlMessage::NewLink(_link),
                ) => {
                    info!("Network link state change detected");
                }
                rtnetlink::packet_core::NetlinkPayload::InnerMessage(
                    rtnetlink::packet_route::RtnlMessage::NewAddress(_addr),
                ) => {
                    info!("IP address change detected");
                }
                _ => {}
            }
        }
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    pub async fn run() -> Result<()> {
        warn!("Netlink monitor is only supported on Linux. Running stub.");
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
        }
    }
}

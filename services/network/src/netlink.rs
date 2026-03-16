use anyhow::Result;
#[cfg(target_os = "linux")]
use futures_util::stream::StreamExt;
#[cfg(target_os = "linux")]
use rtnetlink::new_connection;
use tracing::warn;
#[cfg(target_os = "linux")]
use tracing::info;

pub struct NetlinkMonitor;

impl NetlinkMonitor {
    #[cfg(target_os = "linux")]
    pub async fn run() -> Result<()> {
        let (conn, _handle, mut messages) = new_connection()?;
        tokio::spawn(conn);

        info!("Netlink monitor started (Linux)");

        while let Some((message, _metadata)) = messages.next().await {
            match message {
                rtnetlink::packet::NetlinkMessage::RtmNewLink(_link) => {
                    info!("Network link state change detected");
                }
                rtnetlink::packet::NetlinkMessage::RtmNewAddr(_addr) => {
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

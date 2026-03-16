// dns.rs — Secure DNS and DoH for AetherOS
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use tokio::fs;
use tracing::info;

pub struct DnsResolver;

impl DnsResolver {
    pub async fn apply_system_config(nameservers: &[&str]) -> Result<()> {
        let mut config = String::from("# AetherOS System Resolver\n");
        for ns in nameservers {
            config.push_str(&format!("nameserver {}\n", ns));
        }
        fs::write("/etc/resolv.conf", config).await?;
        info!("Applied system DNS configuration");
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn resolve_doh(domain: &str) -> Result<String> {
        info!(domain, "Resolving via DoH (Cloudflare)");
        let client = reqwest::Client::new();
        let url = format!("https://1.1.1.1/dns-query?name={}&type=A", domain);
        
        let resp = client.get(url)
            .header("accept", "application/dns-json")
            .send()
            .await?;
            
        let json: serde_json::Value = resp.json().await?;
        // Simplified parsing of DNS-over-HTTPS JSON response
        if let Some(answer) = json["Answer"].as_array().and_then(|a| a.first()) {
            if let Some(data) = answer["data"].as_str() {
                return Ok(data.to_string());
            }
        }
        
        anyhow::bail!("Failed to resolve via DoH")
    }
}

// zethra-ai-daemon — ZethraOS Self-Healing Intelligence Layer
// SPDX-License-Identifier: Apache-2.0

mod analyzer;
mod models;
mod patcher;
mod provider;
mod repair;
mod watcher;

use crate::analyzer::{live_analyze, mock_analyze};
use crate::models::*;
use crate::patcher::write_patch;
use crate::provider::ProviderConfig;
use crate::watcher::watch_crashes;

use anyhow::Result;
use chrono::Utc;
use tokio::time::Duration;
use tracing::{error, info, warn};
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()))
        .init();

    let provider = ProviderConfig::detect();
    let crash_dir = std::env::var("ZETHRA_CRASH_DIR")
        .unwrap_or_else(|_| format!("{}/zethra/crashes", std::env::temp_dir().display()));
    let repo_path = std::env::var("ZETHRA_REPO_PATH").unwrap_or_else(|_| ".".to_string());

    info!("══════════════════════════════════════════");
    info!("  ZethraAI daemon — self-healing pipeline");
    match &provider {
        Some(p) => {
            info!("  Provider   : {}", p.name);
            info!("  Model      : {}", p.model);
        }
        None => {
            info!("  Mode       : MOCK  (no API key — offline)");
            info!("  Free options:");
            info!("    Groq      → console.groq.com  (free, fast llama3)");
            info!("    OpenRouter→ openrouter.ai      (free models available)");
            info!("    Google    → aistudio.google.com (free tier, Gemini 2.0 Flash)");
            info!("    Ollama    → ollama.com         (local, completely free)");
        }
    }
    info!("  Crash dir  : {}", crash_dir);
    info!("  Output     : {}/patches/", repo_path);
    info!("══════════════════════════════════════════");

    let (tx, mut rx) = tokio::sync::mpsc::channel::<Issue>(644);

    // Crash watcher
    let wd = crash_dir.clone();
    let wt = tx.clone();
    tokio::spawn(async move { watch_crashes(wd, wt).await });

    // Demo issues in mock mode
    if provider.is_none() {
        let t1 = tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(2)).await;
            info!("──────────────────────────────────────────");
            info!("  [DEMO] injecting kernel panic");
            info!("──────────────────────────────────────────");
            let _ = t1
                .send(Issue::KernelPanic {
                    id: Uuid::new_v4().to_string(),
                    timestamp: Utc::now(),
                    component: "wifi_qcom".to_string(),
                    raw_log: "BUG: kernel NULL pointer dereference\nmodule: wifi_qcom".to_string(),
                })
                .await;
        });
        let t2 = tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(5)).await;
            info!("──────────────────────────────────────────");
            info!("  [DEMO] injecting app crash");
            info!("──────────────────────────────────────────");
            let _ = t2
                .send(Issue::AppCrash {
                    id: Uuid::new_v4().to_string(),
                    timestamp: Utc::now(),
                    package: "zethra.dialer".to_string(),
                    stack_trace: "SIGSEGV in EventLoop::dispatch at event_loop.rs:88".to_string(),
                    signal: 11,
                })
                .await;
        });
    }

    // Main loop
    while let Some(issue) = rx.recv().await {
        info!("┌─ {} — {}", issue.kind(), issue.summary());

        let result = match &provider {
            None => {
                tokio::time::sleep(Duration::from_millis(400)).await;
                mock_analyze(&issue)
            }
            Some(p) => match live_analyze(&issue, p).await {
                Ok(r) => r,
                Err(e) => {
                    error!("API error: {} — falling back to mock", e);
                    mock_analyze(&issue)
                }
            },
        };

        info!(
            "│  root cause : {} [{}]",
            result.root_cause.description, result.root_cause.cause_type
        );
        info!(
            "│  confidence : {:.0}%   severity: {}   by: {}",
            result.proposed_fix.confidence * 100.0,
            result.impact.severity,
            result.generated_by
        );
        info!(
            "│  fix        : {} [{}]",
            result.proposed_fix.description, result.proposed_fix.fix_type
        );

        match write_patch(&repo_path, &result).await {
            Ok(p) => info!("│  patch      : {}", p.display()),
            Err(e) => {
                error!("patch write error: {}", e);
                continue;
            }
        }

        let auto = result.proposed_fix.confidence >= 0.92
            && !result.impact.data_loss_risk
            && !result.impact.security_risk
            && !result.impact.reboot_required
            && result.impact.severity != "Critical"
            && !result.patch_diff.is_empty();

        if auto {
            info!("└─ ✓ AUTO-MERGE eligible (immune system approved)");
        } else {
            warn!("└─ → HUMAN REVIEW needed (impact/risk assessment restricted auto-merge)");
        }
        info!("");
    }

    Ok(())
}

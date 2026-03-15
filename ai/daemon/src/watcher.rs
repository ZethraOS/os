use crate::models::Issue;
use chrono::Utc;
use tokio::fs;
use tokio::time::{interval, Duration};
use tracing::info;
use uuid::Uuid;

// ─── Crash watcher ────────────────────────────────────────────────────────────

pub async fn watch_crashes(log_dir: String, tx: tokio::sync::mpsc::Sender<Issue>) {
    fs::create_dir_all(&log_dir).await.ok();
    info!(dir = %log_dir, "watching for *.crash files");
    let mut seen: std::collections::HashSet<String> = Default::default();
    let mut ticker = interval(Duration::from_secs(3));
    loop {
        ticker.tick().await;
        let mut dir = match fs::read_dir(&log_dir).await {
            Ok(d) => d,
            Err(_) => continue,
        };
        while let Ok(Some(entry)) = dir.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            if seen.contains(&name) || !name.ends_with(".crash") {
                continue;
            }
            seen.insert(name.clone());
            if let Ok(content) = fs::read_to_string(entry.path()).await {
                let issue = if content.contains("Kernel panic") || content.contains("BUG:") {
                    let comp = content
                        .lines()
                        .find(|l| l.contains("module:") || l.contains("driver:"))
                        .and_then(|l| l.split(':').last())
                        .unwrap_or("unknown")
                        .trim()
                        .to_string();
                    Issue::KernelPanic {
                        id: Uuid::new_v4().to_string(),
                        timestamp: Utc::now(),
                        component: comp,
                        raw_log: content,
                    }
                } else {
                    Issue::AppCrash {
                        id: Uuid::new_v4().to_string(),
                        timestamp: Utc::now(),
                        package: name.trim_end_matches(".crash").to_string(),
                        stack_trace: content,
                        signal: 11,
                    }
                };
                info!(file = %name, kind = issue.kind(), "new crash detected");
                let _ = tx.send(issue).await;
            }
        }
    }
}

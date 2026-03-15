use crate::models::AnalysisResult;
use anyhow::Result;
use std::path::PathBuf;
use tokio::fs;

// ─── Patch writer ─────────────────────────────────────────────────────────────

pub async fn write_patch(repo: &str, r: &AnalysisResult) -> Result<PathBuf> {
    let short = &r.issue_id[..8];
    let dir = PathBuf::from(repo).join("patches/staged");
    fs::create_dir_all(&dir).await?;
    fs::write(dir.join(format!("fix-{}.patch", short)), &r.patch_diff).await?;
    fs::write(
        dir.join(format!("fix-{}.meta.json", short)),
        serde_json::to_string_pretty(r)?,
    )
    .await?;
    if !r.test_cases.is_empty() {
        let tdir = PathBuf::from(repo).join("patches/tests");
        fs::create_dir_all(&tdir).await?;
        let src = r
            .test_cases
            .iter()
            .map(|t| format!("// {}\n{}", t.description, t.code))
            .collect::<Vec<_>>()
            .join("\n\n");
        fs::write(tdir.join(format!("test_{}.rs", short)), src).await?;
    }
    Ok(dir.join(format!("fix-{}.patch", short)))
}

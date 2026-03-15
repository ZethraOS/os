use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ─── Issue types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Issue {
    KernelPanic {
        id: String,
        timestamp: DateTime<Utc>,
        raw_log: String,
        component: String,
    },
    AppCrash {
        id: String,
        timestamp: DateTime<Utc>,
        package: String,
        stack_trace: String,
        signal: i32,
    },
    SecurityCve {
        id: String,
        cve_id: String,
        severity: String,
        description: String,
        affected_component: String,
    },
}

impl Issue {
    pub fn id(&self) -> &str {
        match self {
            Issue::KernelPanic { id, .. } => id,
            Issue::AppCrash { id, .. } => id,
            Issue::SecurityCve { id, .. } => id,
        }
    }
    pub fn kind(&self) -> &str {
        match self {
            Issue::KernelPanic { .. } => "KernelPanic",
            Issue::AppCrash { .. } => "AppCrash",
            Issue::SecurityCve { .. } => "SecurityCve",
        }
    }
    pub fn summary(&self) -> String {
        match self {
            Issue::KernelPanic { component, .. } => format!("kernel panic in {}", component),
            Issue::AppCrash {
                package, signal, ..
            } => format!("{} crashed (signal {})", package, signal),
            Issue::SecurityCve {
                cve_id, severity, ..
            } => format!("{} [{}]", cve_id, severity),
        }
    }
}

// ─── Structured analysis types ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootCause {
    pub description: String,
    #[serde(alias = "type", default)]
    pub cause_type: String, // "null_ptr", "race_condition", "buffer_overflow", etc.
    pub subsystem: Option<String>, // "wifi", "memory", "ipc", "scheduler"
    pub cwe_id: Option<String>,    // e.g. "CWE-476" — maps to security databases
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedFix {
    pub description: String,
    pub fix_type: String, // "logic_fix", "bounds_check", "refactor"
    pub confidence: f32,
    pub lines: Option<Vec<AffectedLine>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffectedLine {
    pub file: String,
    pub line_number: usize,
    pub original: String,
    pub replacement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactAssessment {
    pub severity: String,       // "Critical", "High", "Medium", "Low"
    pub affected_users: String, // "all", "wifi_users", "none"
    pub data_loss_risk: bool,
    pub security_risk: bool,
    pub reboot_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub issue_id: String,
    pub root_cause: RootCause,
    pub affected_files: Vec<String>,
    pub proposed_fix: ProposedFix,
    pub patch_diff: String,
    pub test_cases: Vec<TestCase>,
    pub impact: ImpactAssessment,
    pub explanation: String,
    pub generated_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    pub name: String,
    pub description: String,
    pub code: String,
    #[serde(default = "default_test_type")]
    pub test_type: String, // "unit", "integration", "regression"
}

pub fn default_test_type() -> String {
    "regression".to_string()
}

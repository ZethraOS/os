// aether-ai-daemon — AetherOS Self-Healing Intelligence Layer
// SPDX-License-Identifier: Apache-2.0
//
// Supports multiple AI providers — use whichever you have access to:
//
//   PROVIDER        ENV VARS NEEDED                          FREE TIER?
//   ─────────────────────────────────────────────────────────────────────
//   mock            (none)                                   ✓ always free
//   anthropic       ANTHROPIC_API_KEY                        paid
//   openai          OPENAI_API_KEY                           paid (cheap)
//   groq            GROQ_API_KEY                             ✓ free tier
//   together        TOGETHER_API_KEY                         ✓ free tier
//   ollama          OLLAMA_HOST (default: localhost:11434)   ✓ local/free
//   openrouter      OPENROUTER_API_KEY                       ✓ free models
//   google          GOOGLE_API_KEY                           ✓ free tier (Gemini)
//   grok            XAI_API_KEY                              paid (xAI)
//
// Usage — set one of these before running:
//   AETHER_AI_PROVIDER=mock         ./dev.sh run-ai
//   AETHER_AI_PROVIDER=groq         GROQ_API_KEY=gsk_...  ./dev.sh run-ai-live
//   AETHER_AI_PROVIDER=openrouter   OPENROUTER_API_KEY=sk-or-... ./dev.sh run-ai-live
//   AETHER_AI_PROVIDER=ollama       OLLAMA_HOST=localhost:11434   ./dev.sh run-ai-live
//   AETHER_AI_PROVIDER=openai       OPENAI_API_KEY=sk-...         ./dev.sh run-ai-live
//   AETHER_AI_PROVIDER=google       GOOGLE_API_KEY=AIza...         ./dev.sh run-ai-live
//   AETHER_AI_PROVIDER=grok         XAI_API_KEY=xai-...           ./dev.sh run-ai-live
//   AETHER_AI_PROVIDER=anthropic    ANTHROPIC_API_KEY=sk-ant-...  ./dev.sh run-ai-live

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};
use uuid::Uuid;

// ─── Provider config ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub name:    &'static str,
    pub base_url: String,
    pub api_key:  String,
    pub model:    String,
    /// How to send the API key — "Bearer" (OpenAI style) or "x-api-key" (Anthropic)
    pub auth_style: AuthStyle,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AuthStyle { Bearer, XApiKey, GoogleApiKey }

impl ProviderConfig {
    pub fn detect() -> Option<Self> {
        let provider = std::env::var("AETHER_AI_PROVIDER")
            .unwrap_or_else(|_| "auto".to_string())
            .to_lowercase();

        match provider.as_str() {
            "mock" => None,

            "anthropic" => Some(Self {
                name: "Anthropic Claude",
                base_url: "https://api.anthropic.com/v1/messages".into(),
                api_key: std::env::var("ANTHROPIC_API_KEY").unwrap_or_default(),
                model: std::env::var("AETHER_MODEL")
                    .unwrap_or_else(|_| "claude-haiku-4-5-20251001".into()),  // cheapest Claude
                auth_style: AuthStyle::XApiKey,
            }),

            "openai" => Some(Self {
                name: "OpenAI",
                base_url: "https://api.openai.com/v1/chat/completions".into(),
                api_key: std::env::var("OPENAI_API_KEY").unwrap_or_default(),
                model: std::env::var("AETHER_MODEL")
                    .unwrap_or_else(|_| "gpt-4o-mini".into()),  // cheapest GPT-4 class
                auth_style: AuthStyle::Bearer,
            }),

            "groq" => Some(Self {
                name: "Groq (free tier)",
                base_url: "https://api.groq.com/openai/v1/chat/completions".into(),
                api_key: std::env::var("GROQ_API_KEY").unwrap_or_default(),
                model: std::env::var("AETHER_MODEL")
                    .unwrap_or_else(|_| "llama-3.3-70b-versatile".into()),
                auth_style: AuthStyle::Bearer,
            }),

            "together" => Some(Self {
                name: "Together AI (free tier)",
                base_url: "https://api.together.xyz/v1/chat/completions".into(),
                api_key: std::env::var("TOGETHER_API_KEY").unwrap_or_default(),
                model: std::env::var("AETHER_MODEL")
                    .unwrap_or_else(|_| "meta-llama/Llama-3-70b-chat-hf".into()),
                auth_style: AuthStyle::Bearer,
            }),

            "openrouter" => Some(Self {
                name: "OpenRouter (has free models)",
                base_url: "https://openrouter.ai/api/v1/chat/completions".into(),
                api_key: std::env::var("OPENROUTER_API_KEY").unwrap_or_default(),
                // Free models on OpenRouter (no credit card needed):
                model: std::env::var("AETHER_MODEL")
                    .unwrap_or_else(|_| "meta-llama/llama-3.3-70b-instruct:free".into()),
                auth_style: AuthStyle::Bearer,
            }),

            "google" | "gemini" => Some(Self {
                name: "Google Gemini",
                // API key goes in URL for Google's REST API
                base_url: format!(
                    "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
                    std::env::var("AETHER_MODEL").unwrap_or_else(|_| "gemini-2.0-flash".into()),
                    std::env::var("GOOGLE_API_KEY").unwrap_or_default()
                ),
                api_key: std::env::var("GOOGLE_API_KEY").unwrap_or_default(),
                model: std::env::var("AETHER_MODEL")
                    .unwrap_or_else(|_| "gemini-2.0-flash".into()),
                auth_style: AuthStyle::GoogleApiKey,
            }),

            "grok" | "xai" => Some(Self {
                name: "xAI Grok",
                base_url: "https://api.x.ai/v1/chat/completions".into(),
                api_key: std::env::var("XAI_API_KEY").unwrap_or_default(),
                model: std::env::var("AETHER_MODEL")
                    .unwrap_or_else(|_| "grok-beta".into()),
                auth_style: AuthStyle::Bearer,
            }),

                        "ollama" => Some(Self {
                name: "Ollama (local, free)",
                base_url: format!(
                    "http://{}/v1/chat/completions",
                    std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "localhost:11434".into())
                ),
                api_key: String::new(),  // no key needed
                model: std::env::var("AETHER_MODEL")
                    .unwrap_or_else(|_| "llama3.2".into()),
                auth_style: AuthStyle::Bearer,
            }),

            // auto: pick whichever key is set
            _ => {
                if let Ok(k) = std::env::var("GROQ_API_KEY") {
                    return Some(Self {
                        name: "Groq (auto-detected)",
                        base_url: "https://api.groq.com/openai/v1/chat/completions".into(),
                        api_key: k,
                        model: "llama-3.3-70b-versatile".into(),
                        auth_style: AuthStyle::Bearer,
                    });
                }
                if let Ok(k) = std::env::var("OPENROUTER_API_KEY") {
                    return Some(Self {
                        name: "OpenRouter (auto-detected)",
                        base_url: "https://openrouter.ai/api/v1/chat/completions".into(),
                        api_key: k,
                        model: "meta-llama/llama-3.3-70b-instruct:free".into(),
                        auth_style: AuthStyle::Bearer,
                    });
                }
                if let Ok(k) = std::env::var("TOGETHER_API_KEY") {
                    return Some(Self {
                        name: "Together AI (auto-detected)",
                        base_url: "https://api.together.xyz/v1/chat/completions".into(),
                        api_key: k,
                        model: "meta-llama/Llama-3-70b-chat-hf".into(),
                        auth_style: AuthStyle::Bearer,
                    });
                }
                if let Ok(k) = std::env::var("OPENAI_API_KEY") {
                    return Some(Self {
                        name: "OpenAI (auto-detected)",
                        base_url: "https://api.openai.com/v1/chat/completions".into(),
                        api_key: k,
                        model: "gpt-4o-mini".into(),
                        auth_style: AuthStyle::Bearer,
                    });
                }
                if let Ok(k) = std::env::var("ANTHROPIC_API_KEY") {
                    return Some(Self {
                        name: "Anthropic (auto-detected)",
                        base_url: "https://api.anthropic.com/v1/messages".into(),
                        api_key: k,
                        model: "claude-haiku-4-5-20251001".into(),
                        auth_style: AuthStyle::XApiKey,
                    });
                }
                if let Ok(k) = std::env::var("GOOGLE_API_KEY") {
                    let model = "gemini-2.0-flash".to_string();
                    return Some(Self {
                        name: "Google Gemini (auto-detected)",
                        base_url: format!(
                            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
                            model, k
                        ),
                        api_key: k,
                        model,
                        auth_style: AuthStyle::GoogleApiKey,
                    });
                }
                if let Ok(k) = std::env::var("XAI_API_KEY") {
                    return Some(Self {
                        name: "xAI Grok (auto-detected)",
                        base_url: "https://api.x.ai/v1/chat/completions".into(),
                        api_key: k,
                        model: "grok-beta".into(),
                        auth_style: AuthStyle::Bearer,
                    });
                }
                // Check if Ollama is running locally
                if std::net::TcpStream::connect("127.0.0.1:11434").is_ok() {
                    return Some(Self {
                        name: "Ollama (auto-detected, local)",
                        base_url: "http://localhost:11434/v1/chat/completions".into(),
                        api_key: String::new(),
                        model: "llama3.2".into(),
                        auth_style: AuthStyle::Bearer,
                    });
                }
                None  // fall back to mock
            }
        }
    }
}

// ─── Issue types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Issue {
    KernelPanic {
        id: String, timestamp: DateTime<Utc>,
        raw_log: String, component: String,
    },
    AppCrash {
        id: String, timestamp: DateTime<Utc>,
        package: String, stack_trace: String, signal: i32,
    },
    SecurityCve {
        id: String, cve_id: String, severity: String,
        description: String, affected_component: String,
    },
}

impl Issue {
    pub fn id(&self) -> &str {
        match self {
            Issue::KernelPanic  { id, .. } => id,
            Issue::AppCrash     { id, .. } => id,
            Issue::SecurityCve  { id, .. } => id,
        }
    }
    pub fn kind(&self) -> &str {
        match self {
            Issue::KernelPanic  { .. } => "KernelPanic",
            Issue::AppCrash     { .. } => "AppCrash",
            Issue::SecurityCve  { .. } => "SecurityCve",
        }
    }
    pub fn summary(&self) -> String {
        match self {
            Issue::KernelPanic  { component, .. }        => format!("kernel panic in {}", component),
            Issue::AppCrash     { package, signal, .. }  => format!("{} crashed (signal {})", package, signal),
            Issue::SecurityCve  { cve_id, severity, .. } => format!("{} [{}]", cve_id, severity),
        }
    }
}

// ─── Structured analysis types ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootCause {
    pub description: String,
    #[serde(alias = "type", default)]
    pub cause_type:  String,        // "null_ptr", "race_condition", "buffer_overflow", etc.
    pub subsystem:   Option<String>, // "wifi", "memory", "ipc", "scheduler"
    pub cwe_id:      Option<String>, // e.g. "CWE-476" — maps to security databases
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedFix {
    pub description: String,
    pub fix_type:    String,        // "logic_fix", "bounds_check", "refactor"
    pub confidence:  f32,
    pub lines:       Option<Vec<AffectedLine>>,
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
    pub severity:        String,    // "Critical", "High", "Medium", "Low"
    pub affected_users:  String,    // "all", "wifi_users", "none"
    pub data_loss_risk:  bool,
    pub security_risk:   bool,
    pub reboot_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub issue_id:       String,
    pub root_cause:     RootCause,
    pub affected_files: Vec<String>,
    pub proposed_fix:   ProposedFix,
    pub patch_diff:     String,
    pub test_cases:     Vec<TestCase>,
    pub impact:         ImpactAssessment,
    pub explanation:    String,
    pub generated_by:   String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    pub name:        String,
    pub description: String,
    pub code:        String,
    #[serde(default = "default_test_type")]
    pub test_type:   String,        // "unit", "integration", "regression"
}

fn default_test_type() -> String { "regression".to_string() }

// ─── Mock engine ──────────────────────────────────────────────────────────────

fn mock_analyze(issue: &Issue) -> AnalysisResult {
    let short = &issue.id()[..8];
    let (root_cause, files, fix, diff, impact) = match issue {
        Issue::KernelPanic { component, .. } => (
            RootCause {
                description: format!("NULL pointer dereference in {} driver during IRQ handling", component),
                cause_type: "null_ptr".into(),
                subsystem: Some(component.clone()),
                cwe_id: Some("CWE-476".into()),
            },
            vec![format!("drivers/{c}/{c}.c", c = component)],
            ProposedFix {
                description: format!("Add NULL guard before dereferencing device pointer in {}_irq_handler()", component),
                fix_type: "logic_fix".into(),
                confidence: 0.87,
                lines: None,
            },
            format!("--- a/drivers/{c}/{c}.c\n+++ b/drivers/{c}/{c}.c\n\
                     @@ -142,6 +142,10 @@\n \tdev = dev_id;\n\
                     +\tif (!dev) {{\n+\t\treturn IRQ_HANDLED;\n+\t}}\n", c = component),
            ImpactAssessment {
                severity: "Medium".into(),
                affected_users: "all".into(),
                data_loss_risk: false,
                security_risk: false,
                reboot_required: true,
            },
        ),
        Issue::AppCrash { package, .. } => (
            RootCause {
                description: format!("Use-after-free in {}::EventLoop during async callback after destroy", package),
                cause_type: "uaf".into(),
                subsystem: Some("userspace_app".into()),
                cwe_id: Some("CWE-416".into()),
            },
            vec![format!("apps/{}/src/event_loop.rs", package)],
            ProposedFix {
                description: "Use Arc::downgrade() in async closures instead of cloning a strong Arc".to_string(),
                fix_type: "refactor".into(),
                confidence: 0.94,
                lines: None,
            },
            format!("--- a/apps/{p}/src/event_loop.rs\n+++ b/apps/{p}/src/event_loop.rs\n\
                     @@ -88 +88 @@\n-\tlet a = self.activity.clone();\n\
                     +\tlet a = Arc::downgrade(&self.activity);\n", p = package),
            ImpactAssessment {
                severity: "Low".into(),
                affected_users: "app_users".into(),
                data_loss_risk: false,
                security_risk: false,
                reboot_required: false,
            },
        ),
        Issue::SecurityCve { cve_id, affected_component, .. } => (
            RootCause {
                description: format!("{}: input not sanitised before use in {}", cve_id, affected_component),
                cause_type: "integer_overflow".into(),
                subsystem: Some("network_stack".into()),
                cwe_id: Some("CWE-190".into()),
            },
            vec![format!("services/{}/src/auth.rs", affected_component)],
            ProposedFix {
                description: "Reject tokens > 4096 bytes; add length check in verify_token()".to_string(),
                fix_type: "logic_fix".into(),
                confidence: 0.89,
                lines: None,
            },
            format!("--- a/services/{c}/src/auth.rs\n+++ b/services/{c}/src/auth.rs\n\
                     @@ -201 +201 @@\n\
                     +\tif token.len() > 4096 {{ return Err(AuthError::TooLong); }}\n",
                     c = affected_component),
            ImpactAssessment {
                severity: "High".into(),
                affected_users: "remote_users".into(),
                data_loss_risk: false,
                security_risk: true,
                reboot_required: false,
            },
        ),
    };
    AnalysisResult {
        issue_id: issue.id().to_string(),
        root_cause, affected_files: files, proposed_fix: fix, patch_diff: diff,
        test_cases: vec![TestCase {
            name: format!("test_{}_regression", short),
            description: format!("Regression test: {}", issue.summary()),
            code: format!("#[test]\nfn test_{}_fix() {{\n    assert!(true); // TODO: real checks\n}}", short),
            test_type: "regression".into(),
        }],
        impact,
        explanation: format!(
            "[MOCK] Simulated analysis for: {}.\nSet AETHER_AI_PROVIDER to use a real model.",
            issue.summary()
        ),
        generated_by: "mock".to_string(),
    }
}

// ─── Universal live API caller ────────────────────────────────────────────────
// Supports both OpenAI-compatible (chat/completions) and Anthropic (messages) APIs.

async fn live_analyze(issue: &Issue, provider: &ProviderConfig) -> Result<AnalysisResult> {
    let client = reqwest::Client::new();

    let system = "You are AetherAI, an OS self-healing system. \
        Analyze the issue and respond ONLY with valid JSON matching this schema: \
        {\"issue_id\":\"[ID]\",\"root_cause\":{\"description\":\"\",\"type\":\"\",\"subsystem\":\"\",\"cwe_id\":\"\"},\
        \"affected_files\":[],\"proposed_fix\":{\"description\":\"\",\"fix_type\":\"\",\"confidence\":0.2},\
        \"patch_diff\":\"\",\"test_cases\":[{\"name\":\"\",\"description\":\"\",\"code\":\"\",\"test_type\":\"\"}],\
        \"impact\":{\"severity\":\"Critical|High|Medium|Low\",\"affected_users\":\"\",\"data_loss_risk\":false,\"security_risk\":false,\"reboot_required\":false},\
        \"explanation\":\"\"}";

    let user_msg = format!(
        "Analyze this OS issue and produce a fix:\n\n{}",
        serde_json::to_string_pretty(issue)?
    );

    // Build request body — Anthropic uses a different schema than OpenAI
    let body = if provider.auth_style == AuthStyle::XApiKey {
        // Anthropic messages API
        serde_json::json!({
            "model": provider.model,
            "max_tokens": 4096,
            "system": system,
            "messages": [{ "role": "user", "content": user_msg }]
        })
    } else if provider.auth_style == AuthStyle::GoogleApiKey {
        // Google Gemini generateContent API
        serde_json::json!({
            "contents": [{
                "parts": [{"text": format!("{system}\n\n{user_msg}")}]
            }],
            "generationConfig": {
                "maxOutputTokens": 4096,
                "temperature": 0.2,
                "responseMimeType": "application/json"
            }
        })
    } else if provider.base_url.contains("/api/chat") {
        // Ollama native API fallback
        serde_json::json!({
            "model": provider.model,
            "stream": false,
            "format": "json",
            "messages": [
                { "role": "system", "content": system },
                { "role": "user",   "content": user_msg }
            ],
            "options": {
                "num_predict": 4096,
                "temperature": 0.2
            }
        })
    } else {
        // OpenAI-compatible (Groq, Together, OpenRouter, OpenAI)
        serde_json::json!({
            "model": provider.model,
            "max_tokens": 4096,
            "stream": false,
            "response_format": { "type": "json_object" },
            "messages": [
                { "role": "system", "content": system },
                { "role": "user",   "content": user_msg }
            ]
        })
    };

    let mut req = client.post(&provider.base_url).json(&body);

    // Auth header
    req = match provider.auth_style {
        AuthStyle::XApiKey => req
            .header("x-api-key", &provider.api_key)
            .header("anthropic-version", "2023-06-01"),
        AuthStyle::Bearer if !provider.api_key.is_empty() =>
            req.header("Authorization", format!("Bearer {}", provider.api_key)),
        AuthStyle::GoogleApiKey => req,  // key is already in the URL
        _ => req,
    };

    let resp: reqwest::Response = req.send().await.context("API request failed")?;
    let status = resp.status();
    let body_text: String = resp.text().await?;

    if !status.is_success() {
        anyhow::bail!("API error {}: {}", status, &body_text[..1024.min(body_text.len())]);
    }

    let rj: serde_json::Value = serde_json::from_str(&body_text)
        .with_context(|| format!("parsing API response: {}", &body_text[..2048.min(body_text.len())]))?;

    // Extract text from either Anthropic, Google, Ollama, or OpenAI response format
    let text = if provider.auth_style == AuthStyle::XApiKey {
        rj["content"][0]["text"].as_str().unwrap_or("").to_string()
    } else if provider.auth_style == AuthStyle::GoogleApiKey {
        rj["candidates"][0]["content"]["parts"][0]["text"].as_str().unwrap_or("").to_string()
    } else if provider.base_url.contains("/api/chat") {
        rj["message"]["content"].as_str().unwrap_or("").to_string()
    } else {
        rj["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string()
    };

    if text.is_empty() {
        anyhow::bail!("empty response from {}: {}", provider.name, &body_text[..300.min(body_text.len())]);
    }

    // Strip markdown fences or extra garbage
    let mut clean = text.trim();
    if clean.contains("```json") {
        clean = clean.split("```json").nth(1).unwrap_or(clean).split("```").collect::<Vec<_>>()[0].trim();
    } else if clean.contains("```") {
        clean = clean.split("```").nth(1).unwrap_or(clean).split("```").collect::<Vec<_>>()[0].trim();
    }

    // Parse into a Value first to handle schema variations gracefully
    let val: serde_json::Value = serde_json::from_str(&repair_json(clean))
        .with_context(|| format!("JSON decode error. Content was:\n{}", &clean[..2048.min(clean.len())]))?;

    // Permissive mapping to AnalysisResult
    let result = AnalysisResult {
        issue_id: val["issue_id"].as_str().unwrap_or(issue.id()).to_string(),
        root_cause: {
            let rc = &val["root_cause"];
            RootCause {
                description: rc["description"].as_str()
                    .or_else(|| rc.as_str())          // flat string fallback
                    .unwrap_or("Unknown").to_string(),
                cause_type: rc["type"].as_str()
                    .or_else(|| rc["cause_type"].as_str())
                    .unwrap_or("unknown").to_string(),
                subsystem: rc["subsystem"].as_str().map(|s| s.to_string()),
                cwe_id:    rc["cwe_id"].as_str().map(|s| s.to_string()),
            }
        },
        affected_files: val["affected_files"].as_array()
            .or_else(|| val["affectedFiles"].as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default(),
        proposed_fix: {
            let pf = &val["proposed_fix"];
            ProposedFix {
                description: match pf {
                    serde_json::Value::String(s) => s.clone(),
                    _ => pf["description"].as_str()
                            .unwrap_or("No fix proposed").to_string(),
                },
                fix_type:   pf["fix_type"].as_str().unwrap_or("unknown").to_string(),
                confidence: match &pf["confidence"] {
                    serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.5) as f32,
                    serde_json::Value::String(s) => s.parse().unwrap_or(0.5),
                    _ => match &val["confidence"] { // fallback to top-level confidence if nested is missing
                        serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.5) as f32,
                        serde_json::Value::String(s) => s.parse().unwrap_or(0.5),
                        _ => 0.5,
                    },
                },
                lines: None,
            }
        },
        patch_diff: val["patch_diff"].as_str()
            .or_else(|| val["patchDiff"].as_str())
            .unwrap_or_default().to_string(),
        test_cases: val["test_cases"].as_array()
            .or_else(|| val["testCases"].as_array())
            .map(|arr| arr.iter().filter_map(|v| {
                let name = v["name"].as_str()?;  // skip if missing
                Some(TestCase {
                    name: name.to_string(),
                    description: v["description"].as_str().unwrap_or("").to_string(),
                    code: v["code"].as_str().unwrap_or("").to_string(),
                    test_type: v["test_type"].as_str().unwrap_or("unit").to_string(),
                })
            }).collect())
            .unwrap_or_default(),
        impact: {
            let imp = &val["impact"];
            ImpactAssessment {
                severity: imp["severity"].as_str()
                    .or_else(|| val["risk_level"].as_str())
                    .unwrap_or("Medium").to_string(),
                affected_users:  imp["affected_users"].as_str()
                    .or_else(|| imp["affectedUsers"].as_str())
                    .unwrap_or("unknown").to_string(),
                data_loss_risk:  imp["data_loss_risk"].as_bool().unwrap_or(false),
                security_risk:   imp["security_risk"].as_bool().unwrap_or(false),
                reboot_required: imp["reboot_required"].as_bool().unwrap_or(false),
            }
        },
        explanation: val["explanation"].as_str().unwrap_or("").to_string(),
        generated_by: format!("{} ({})", provider.name, provider.model),
    };

    Ok(result)
}

/// Attempts to fix truncated JSON and normalises quotes.
fn repair_json(input: &str) -> String {
    let input = &input.replace('\'', "\"");
    let mut repaired = input.to_string();

    let mut open_braces = 0;
    let mut open_brackets = 0;
    let mut in_quote = false;
    let mut escaped = false;

    for c in repaired.chars() {
        if escaped { escaped = false; continue; }
        match c {
            '\\' => escaped = true,
            '"'  => in_quote = !in_quote,
            '{' if !in_quote => open_braces += 1,
            '}' if !in_quote => open_braces -= 1,
            '[' if !in_quote => open_brackets += 1,
            ']' if !in_quote => open_brackets -= 1,
            _ => {}
        }
    }

    if in_quote { repaired.push('"'); }
    while open_brackets > 0 { repaired.push(']'); open_brackets -= 1; }
    while open_braces > 0   { repaired.push('}'); open_braces -= 1; }

    repaired
}

// ─── Crash watcher ────────────────────────────────────────────────────────────

async fn watch_crashes(log_dir: String, tx: tokio::sync::mpsc::Sender<Issue>) {
    fs::create_dir_all(&log_dir).await.ok();
    info!(dir = %log_dir, "watching for *.crash files");
    let mut seen: std::collections::HashSet<String> = Default::default();
    let mut ticker = interval(Duration::from_secs(3));
    loop {
        ticker.tick().await;
        let mut dir = match fs::read_dir(&log_dir).await { Ok(d) => d, Err(_) => continue };
        while let Ok(Some(entry)) = dir.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            if seen.contains(&name) || !name.ends_with(".crash") { continue; }
            seen.insert(name.clone());
            if let Ok(content) = fs::read_to_string(entry.path()).await {
                let issue = if content.contains("Kernel panic") || content.contains("BUG:") {
                    let comp = content.lines()
                        .find(|l| l.contains("module:") || l.contains("driver:"))
                        .and_then(|l| l.split(':').last()).unwrap_or("unknown").trim().to_string();
                    Issue::KernelPanic { id: Uuid::new_v4().to_string(), timestamp: Utc::now(),
                        component: comp, raw_log: content }
                } else {
                    Issue::AppCrash { id: Uuid::new_v4().to_string(), timestamp: Utc::now(),
                        package: name.trim_end_matches(".crash").to_string(),
                        stack_trace: content, signal: 11 }
                };
                info!(file = %name, kind = issue.kind(), "new crash detected");
                let _ = tx.send(issue).await;
            }
        }
    }
}

// ─── Patch writer ─────────────────────────────────────────────────────────────

async fn write_patch(repo: &str, r: &AnalysisResult) -> Result<PathBuf> {
    let short = &r.issue_id[..8];
    let dir = PathBuf::from(repo).join("patches/staged");
    fs::create_dir_all(&dir).await?;
    fs::write(dir.join(format!("fix-{}.patch", short)), &r.patch_diff).await?;
    fs::write(dir.join(format!("fix-{}.meta.json", short)), serde_json::to_string_pretty(r)?).await?;
    if !r.test_cases.is_empty() {
        let tdir = PathBuf::from(repo).join("patches/tests");
        fs::create_dir_all(&tdir).await?;
        let src = r.test_cases.iter()
            .map(|t| format!("// {}\n{}", t.description, t.code))
            .collect::<Vec<_>>().join("\n\n");
        fs::write(tdir.join(format!("test_{}.rs", short)), src).await?;
    }
    Ok(dir.join(format!("fix-{}.patch", short)))
}

// ─── Entry point ─────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()))
        .init();

    let provider = ProviderConfig::detect();
    let crash_dir = std::env::var("AETHER_CRASH_DIR")
        .unwrap_or_else(|_| format!("{}/aether/crashes", std::env::temp_dir().display()));
    let repo_path = std::env::var("AETHER_REPO_PATH").unwrap_or_else(|_| ".".to_string());
    let threshold = 0.92_f32;

    info!("══════════════════════════════════════════");
    info!("  AetherAI daemon — self-healing pipeline");
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

    let (tx, mut rx) = tokio::sync::mpsc::channel::<Issue>(64);

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
            let _ = t1.send(Issue::KernelPanic {
                id: Uuid::new_v4().to_string(), timestamp: Utc::now(),
                component: "wifi_qcom".to_string(),
                raw_log: "BUG: kernel NULL pointer dereference\nmodule: wifi_qcom".to_string(),
            }).await;
        });
        let t2 = tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(5)).await;
            info!("──────────────────────────────────────────");
            info!("  [DEMO] injecting app crash");
            info!("──────────────────────────────────────────");
            let _ = t2.send(Issue::AppCrash {
                id: Uuid::new_v4().to_string(), timestamp: Utc::now(),
                package: "aether.dialer".to_string(),
                stack_trace: "SIGSEGV in EventLoop::dispatch at event_loop.rs:88".to_string(),
                signal: 11,
            }).await;
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
            Some(p) => {
                match live_analyze(&issue, p).await {
                    Ok(r)  => r,
                    Err(e) => {
                        error!("API error: {} — falling back to mock", e);
                        mock_analyze(&issue)
                    }
                }
            }
        };

        info!("│  root cause : {} [{}]", result.root_cause.description, result.root_cause.cause_type);
        info!("│  confidence : {:.0}%   severity: {}   by: {}", result.proposed_fix.confidence * 100.0, result.impact.severity, result.generated_by);
        info!("│  fix        : {} [{}]", result.proposed_fix.description, result.proposed_fix.fix_type);

        match write_patch(&repo_path, &result).await {
            Ok(p)  => info!("│  patch      : {}", p.display()),
            Err(e) => { error!("patch write error: {}", e); continue; }
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

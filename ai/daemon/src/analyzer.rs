use crate::models::*;
use crate::provider::*;
use crate::repair::repair_json;
use anyhow::{Context, Result};

// ─── Mock engine ──────────────────────────────────────────────────────────────

pub fn mock_analyze(issue: &Issue) -> AnalysisResult {
    let short = &issue.id()[..8];
    let (root_cause, files, fix, diff, impact) = match issue {
        Issue::KernelPanic { component, .. } => (
            RootCause {
                description: format!(
                    "NULL pointer dereference in {} driver during IRQ handling",
                    component
                ),
                cause_type: "null_ptr".into(),
                subsystem: Some(component.clone()),
                cwe_id: Some("CWE-476".into()),
            },
            vec![format!("drivers/{c}/{c}.c", c = component)],
            ProposedFix {
                description: format!(
                    "Add NULL guard before dereferencing device pointer in {}_irq_handler()",
                    component
                ),
                fix_type: "logic_fix".into(),
                confidence: 0.87,
                lines: None,
            },
            format!(
                "--- a/drivers/{c}/{c}.c\n+++ b/drivers/{c}/{c}.c\n\
                     @@ -142,6 +142,10 @@\n \tdev = dev_id;\n\
                     +\tif (!dev) {{\n+\t\treturn IRQ_HANDLED;\n+\t}}\n",
                c = component
            ),
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
                description: format!(
                    "Use-after-free in {}::EventLoop during async callback after destroy",
                    package
                ),
                cause_type: "uaf".into(),
                subsystem: Some("userspace_app".into()),
                cwe_id: Some("CWE-416".into()),
            },
            vec![format!("apps/{}/src/event_loop.rs", package)],
            ProposedFix {
                description:
                    "Use Arc::downgrade() in async closures instead of cloning a strong Arc"
                        .to_string(),
                fix_type: "refactor".into(),
                confidence: 0.94,
                lines: None,
            },
            format!(
                "--- a/apps/{p}/src/event_loop.rs\n+++ b/apps/{p}/src/event_loop.rs\n\
                     @@ -88 +88 @@\n-\tlet a = self.activity.clone();\n\
                     +\tlet a = Arc::downgrade(&self.activity);\n",
                p = package
            ),
            ImpactAssessment {
                severity: "Low".into(),
                affected_users: "app_users".into(),
                data_loss_risk: false,
                security_risk: false,
                reboot_required: false,
            },
        ),
        Issue::SecurityCve {
            cve_id,
            affected_component,
            ..
        } => (
            RootCause {
                description: format!(
                    "{}: input not sanitised before use in {}",
                    cve_id, affected_component
                ),
                cause_type: "integer_overflow".into(),
                subsystem: Some("network_stack".into()),
                cwe_id: Some("CWE-190".into()),
            },
            vec![format!("services/{}/src/auth.rs", affected_component)],
            ProposedFix {
                description: "Reject tokens > 4096 bytes; add length check in verify_token()"
                    .to_string(),
                fix_type: "logic_fix".into(),
                confidence: 0.89,
                lines: None,
            },
            format!(
                "--- a/services/{c}/src/auth.rs\n+++ b/services/{c}/src/auth.rs\n\
                     @@ -201 +201 @@\n\
                     +\tif token.len() > 4096 {{ return Err(AuthError::TooLong); }}\n",
                c = affected_component
            ),
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
        root_cause,
        affected_files: files,
        proposed_fix: fix,
        patch_diff: diff,
        test_cases: vec![TestCase {
            name: format!("test_{}_regression", short),
            description: format!("Regression test: {}", issue.summary()),
            code: format!(
                "#[test]\nfn test_{}_fix() {{\n    assert!(true); // TODO: real checks\n}}",
                short
            ),
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

pub async fn live_analyze(issue: &Issue, provider: &ProviderConfig) -> Result<AnalysisResult> {
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
        AuthStyle::Bearer if !provider.api_key.is_empty() => {
            req.header("Authorization", format!("Bearer {}", provider.api_key))
        }
        AuthStyle::GoogleApiKey => req, // key is already in the URL
        _ => req,
    };

    let resp: reqwest::Response = req.send().await.context("API request failed")?;
    let status = resp.status();
    let body_text: String = resp.text().await?;

    if !status.is_success() {
        anyhow::bail!(
            "API error {}: {}",
            status,
            &body_text[..1024.min(body_text.len())]
        );
    }

    let rj: serde_json::Value = serde_json::from_str(&body_text).with_context(|| {
        format!(
            "parsing API response: {}",
            &body_text[..2048.min(body_text.len())]
        )
    })?;

    // Extract text from either Anthropic, Google, Ollama, or OpenAI response format
    let text = if provider.auth_style == AuthStyle::XApiKey {
        rj["content"][0]["text"].as_str().unwrap_or("").to_string()
    } else if provider.auth_style == AuthStyle::GoogleApiKey {
        rj["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string()
    } else if provider.base_url.contains("/api/chat") {
        rj["message"]["content"].as_str().unwrap_or("").to_string()
    } else {
        rj["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string()
    };

    if text.is_empty() {
        anyhow::bail!(
            "empty response from {}: {}",
            provider.name,
            &body_text[..300.min(body_text.len())]
        );
    }

    // Strip markdown fences or extra garbage
    let mut clean = text.trim();
    if clean.contains("```json") {
        clean = clean
            .split("```json")
            .nth(1)
            .unwrap_or(clean)
            .split("```")
            .collect::<Vec<_>>()[0]
            .trim();
    } else if clean.contains("```") {
        clean = clean
            .split("```")
            .nth(1)
            .unwrap_or(clean)
            .split("```")
            .collect::<Vec<_>>()[0]
            .trim();
    }

    // Parse into a Value first to handle schema variations gracefully
    let val: serde_json::Value = serde_json::from_str(&repair_json(clean)).with_context(|| {
        format!(
            "JSON decode error. Content was:\n{}",
            &clean[..2048.min(clean.len())]
        )
    })?;

    // Permissive mapping to AnalysisResult
    let result = AnalysisResult {
        issue_id: val["issue_id"].as_str().unwrap_or(issue.id()).to_string(),
        root_cause: {
            let rc = &val["root_cause"];
            RootCause {
                description: rc["description"]
                    .as_str()
                    .or_else(|| rc.as_str()) // flat string fallback
                    .unwrap_or("Unknown")
                    .to_string(),
                cause_type: rc["type"]
                    .as_str()
                    .or_else(|| rc["cause_type"].as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                subsystem: rc["subsystem"].as_str().map(|s| s.to_string()),
                cwe_id: rc["cwe_id"].as_str().map(|s| s.to_string()),
            }
        },
        affected_files: val["affected_files"]
            .as_array()
            .or_else(|| val["affectedFiles"].as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        proposed_fix: {
            let pf = &val["proposed_fix"];
            ProposedFix {
                description: match pf {
                    serde_json::Value::String(s) => s.clone(),
                    _ => pf["description"]
                        .as_str()
                        .unwrap_or("No fix proposed")
                        .to_string(),
                },
                fix_type: pf["fix_type"].as_str().unwrap_or("unknown").to_string(),
                confidence: match &pf["confidence"] {
                    serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.5) as f32,
                    serde_json::Value::String(s) => s.parse().unwrap_or(0.5),
                    _ => match &val["confidence"] {
                        // fallback to top-level confidence if nested is missing
                        serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.5) as f32,
                        serde_json::Value::String(s) => s.parse().unwrap_or(0.5),
                        _ => 0.5,
                    },
                },
                lines: None,
            }
        },
        patch_diff: val["patch_diff"]
            .as_str()
            .or_else(|| val["patchDiff"].as_str())
            .unwrap_or_default()
            .to_string(),
        test_cases: val["test_cases"]
            .as_array()
            .or_else(|| val["testCases"].as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| {
                        let name = v["name"].as_str()?; // skip if missing
                        Some(TestCase {
                            name: name.to_string(),
                            description: v["description"].as_str().unwrap_or("").to_string(),
                            code: v["code"].as_str().unwrap_or("").to_string(),
                            test_type: v["test_type"].as_str().unwrap_or("unit").to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        impact: {
            let imp = &val["impact"];
            ImpactAssessment {
                severity: imp["severity"]
                    .as_str()
                    .or_else(|| val["risk_level"].as_str())
                    .unwrap_or("Medium")
                    .to_string(),
                affected_users: imp["affected_users"]
                    .as_str()
                    .or_else(|| imp["affectedUsers"].as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                data_loss_risk: imp["data_loss_risk"].as_bool().unwrap_or(false),
                security_risk: imp["security_risk"].as_bool().unwrap_or(false),
                reboot_required: imp["reboot_required"].as_bool().unwrap_or(false),
            }
        },
        explanation: val["explanation"].as_str().unwrap_or("").to_string(),
        generated_by: format!("{} ({})", provider.name, provider.model),
    };

    Ok(result)
}

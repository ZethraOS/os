#[derive(Debug, Clone, PartialEq)]
pub enum AuthStyle {
    Bearer,
    XApiKey,
    GoogleApiKey,
}

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub name: &'static str,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    /// How to send the API key — "Bearer" (OpenAI style) or "x-api-key" (Anthropic)
    pub auth_style: AuthStyle,
}

impl ProviderConfig {
    pub fn detect() -> Option<Self> {
        let provider = std::env::var("ZETHRA_AI_PROVIDER")
            .unwrap_or_else(|_| "auto".to_string())
            .to_lowercase();

        match provider.as_str() {
            "mock" => None,

            "anthropic" => Some(Self {
                name: "Anthropic Claude",
                base_url: "https://api.anthropic.com/v1/messages".into(),
                api_key: std::env::var("ANTHROPIC_API_KEY").unwrap_or_default(),
                model: std::env::var("ZETHRA_MODEL")
                    .unwrap_or_else(|_| "claude-haiku-4-5-20251001".into()), // cheapest Claude
                auth_style: AuthStyle::XApiKey,
            }),

            "openai" => Some(Self {
                name: "OpenAI",
                base_url: "https://api.openai.com/v1/chat/completions".into(),
                api_key: std::env::var("OPENAI_API_KEY").unwrap_or_default(),
                model: std::env::var("ZETHRA_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into()), // cheapest GPT-4 class
                auth_style: AuthStyle::Bearer,
            }),

            "groq" => Some(Self {
                name: "Groq (free tier)",
                base_url: "https://api.groq.com/openai/v1/chat/completions".into(),
                api_key: std::env::var("GROQ_API_KEY").unwrap_or_default(),
                model: std::env::var("ZETHRA_MODEL").unwrap_or_else(|_| "llama-3.3-70b-versatile".into()),
                auth_style: AuthStyle::Bearer,
            }),

            "together" => Some(Self {
                name: "Together AI (free tier)",
                base_url: "https://api.together.xyz/v1/chat/completions".into(),
                api_key: std::env::var("TOGETHER_API_KEY").unwrap_or_default(),
                model: std::env::var("ZETHRA_MODEL")
                    .unwrap_or_else(|_| "meta-llama/Llama-3-70b-chat-hf".into()),
                auth_style: AuthStyle::Bearer,
            }),

            "openrouter" => Some(Self {
                name: "OpenRouter (has free models)",
                base_url: "https://openrouter.ai/api/v1/chat/completions".into(),
                api_key: std::env::var("OPENROUTER_API_KEY").unwrap_or_default(),
                // Free models on OpenRouter (no credit card needed):
                model: std::env::var("ZETHRA_MODEL")
                    .unwrap_or_else(|_| "meta-llama/llama-3.3-70b-instruct:free".into()),
                auth_style: AuthStyle::Bearer,
            }),

            "google" | "gemini" => Some(Self {
                name: "Google Gemini",
                // API key goes in URL for Google's REST API
                base_url: format!(
                    "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
                    std::env::var("ZETHRA_MODEL").unwrap_or_else(|_| "gemini-2.0-flash".into()),
                    std::env::var("GOOGLE_API_KEY").unwrap_or_default()
                ),
                api_key: std::env::var("GOOGLE_API_KEY").unwrap_or_default(),
                model: std::env::var("ZETHRA_MODEL").unwrap_or_else(|_| "gemini-2.0-flash".into()),
                auth_style: AuthStyle::GoogleApiKey,
            }),

            "grok" | "xai" => Some(Self {
                name: "xAI Grok",
                base_url: "https://api.x.ai/v1/chat/completions".into(),
                api_key: std::env::var("XAI_API_KEY").unwrap_or_default(),
                model: std::env::var("ZETHRA_MODEL").unwrap_or_else(|_| "grok-beta".into()),
                auth_style: AuthStyle::Bearer,
            }),

            "ollama" => Some(Self {
                name: "Ollama (local, free)",
                base_url: format!(
                    "http://{}/v1/chat/completions",
                    std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "localhost:11434".into())
                ),
                api_key: String::new(), // no key needed
                model: std::env::var("ZETHRA_MODEL").unwrap_or_else(|_| "llama3.2".into()),
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
                None // fall back to mock
            }
        }
    }
}

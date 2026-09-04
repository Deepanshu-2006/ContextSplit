use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use serde::{Deserialize, Serialize};

use super::mock::MockAIReasoner;
use super::models::AIReasoningError;
use super::remote::RemoteLLMReasoner;
use super::traits::AIReasoner;

/// Categorical type identifier for AI Context Reasoning providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AIProviderType {
    Mock,
    RemoteLlm,
    Ollama,
    OpenAi,
    Gemini,
}

impl fmt::Display for AIProviderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AIProviderType::Mock => write!(f, "mock"),
            AIProviderType::RemoteLlm => write!(f, "remote_llm"),
            AIProviderType::Ollama => write!(f, "ollama"),
            AIProviderType::OpenAi => write!(f, "openai"),
            AIProviderType::Gemini => write!(f, "gemini"),
        }
    }
}

/// Isolated configuration for AI context reasoning providers.
/// Protects sensitive credentials from accidental logging or serialization leakage.
#[derive(Clone, Serialize, Deserialize)]
pub struct AIProviderConfig {
    pub provider_type: AIProviderType,
    pub model_name: String,
    pub endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    pub timeout_seconds: u64,
    pub max_retries: u32,
    pub custom_headers: HashMap<String, String>,
}

// Custom Debug implementation to ensure API keys are masked in logs and traces.
impl fmt::Debug for AIProviderConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AIProviderConfig")
            .field("provider_type", &self.provider_type)
            .field("model_name", &self.model_name)
            .field("endpoint", &self.endpoint)
            .field(
                "api_key",
                &self.api_key.as_ref().map(|k| {
                    if k.len() > 4 {
                        format!("{}***", &k[..3])
                    } else {
                        "***".to_string()
                    }
                }),
            )
            .field("timeout_seconds", &self.timeout_seconds)
            .field("max_retries", &self.max_retries)
            .field("custom_headers_count", &self.custom_headers.len())
            .finish()
    }
}

impl Default for AIProviderConfig {
    fn default() -> Self {
        Self::default_mock()
    }
}

impl AIProviderConfig {
    /// Constructs the standard development mock provider configuration.
    pub fn default_mock() -> Self {
        Self {
            provider_type: AIProviderType::Mock,
            model_name: "mock-reasoner-v1.0".to_string(),
            endpoint: None,
            api_key: None,
            timeout_seconds: 5,
            max_retries: 0,
            custom_headers: HashMap::new(),
        }
    }

    /// Constructs a mock provider configuration with a customized model name.
    pub fn mock(model_name: &str) -> Self {
        Self {
            provider_type: AIProviderType::Mock,
            model_name: model_name.to_string(),
            endpoint: None,
            api_key: None,
            timeout_seconds: 5,
            max_retries: 0,
            custom_headers: HashMap::new(),
        }
    }

    /// Constructs a configuration for a remote LLM provider.
    pub fn remote(
        provider_type: AIProviderType,
        model_name: &str,
        endpoint: Option<String>,
        api_key: Option<String>,
    ) -> Self {
        Self {
            provider_type,
            model_name: model_name.to_string(),
            endpoint,
            api_key,
            timeout_seconds: 15,
            max_retries: 2,
            custom_headers: HashMap::new(),
        }
    }

    /// Sets custom timeout and retry limits.
    pub fn with_timeout_and_retries(mut self, timeout_seconds: u64, max_retries: u32) -> Self {
        self.timeout_seconds = timeout_seconds;
        self.max_retries = max_retries;
        self
    }

    /// Discovers provider configuration from external environment variables without hardcoding secrets.
    /// Priority:
    /// 1. Explicit `CONTEXTSPLIT_LLM_PROVIDER` (e.g., "openai", "ollama", "gemini", "mock", "remote")
    /// 2. Implicit detection via `OPENAI_API_KEY`, `OLLAMA_HOST`, `GEMINI_API_KEY`
    /// 3. Fallback to `MockAIReasoner` for seamless, deterministic local execution.
    pub fn from_env() -> Self {
        let timeout_seconds = std::env::var("CONTEXTSPLIT_LLM_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(15);

        let max_retries = std::env::var("CONTEXTSPLIT_LLM_MAX_RETRIES")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(2);

        let custom_model = std::env::var("CONTEXTSPLIT_LLM_MODEL").ok();
        let custom_endpoint = std::env::var("CONTEXTSPLIT_LLM_ENDPOINT").ok();
        let custom_key = std::env::var("CONTEXTSPLIT_LLM_API_KEY").ok();

        if let Ok(prov_str) = std::env::var("CONTEXTSPLIT_LLM_PROVIDER") {
            match prov_str.to_lowercase().as_str() {
                "openai" => {
                    let key = custom_key.or_else(|| std::env::var("OPENAI_API_KEY").ok());
                    let endpoint = custom_endpoint
                        .or_else(|| std::env::var("OPENAI_BASE_URL").ok())
                        .or_else(|| Some("https://api.openai.com/v1".to_string()));
                    let model = custom_model.unwrap_or_else(|| "gpt-4o-mini".to_string());
                    return Self::remote(AIProviderType::OpenAi, &model, endpoint, key)
                        .with_timeout_and_retries(timeout_seconds, max_retries);
                }
                "ollama" => {
                    let endpoint = custom_endpoint
                        .or_else(|| std::env::var("OLLAMA_HOST").ok())
                        .or_else(|| Some("http://localhost:11434".to_string()));
                    let model = custom_model.unwrap_or_else(|| "llama3".to_string());
                    return Self::remote(AIProviderType::Ollama, &model, endpoint, custom_key)
                        .with_timeout_and_retries(timeout_seconds, max_retries);
                }
                "gemini" => {
                    let key = custom_key.or_else(|| std::env::var("GEMINI_API_KEY").ok());
                    let endpoint = custom_endpoint
                        .or_else(|| Some("https://generativelanguage.googleapis.com/v1beta".to_string()));
                    let model = custom_model.unwrap_or_else(|| "gemini-1.5-flash".to_string());
                    return Self::remote(AIProviderType::Gemini, &model, endpoint, key)
                        .with_timeout_and_retries(timeout_seconds, max_retries);
                }
                "remote" | "remote_llm" => {
                    let model = custom_model.unwrap_or_else(|| "remote-llm".to_string());
                    return Self::remote(AIProviderType::RemoteLlm, &model, custom_endpoint, custom_key)
                        .with_timeout_and_retries(timeout_seconds, max_retries);
                }
                "mock" => {
                    let model = custom_model.unwrap_or_else(|| "mock-reasoner-v1.0".to_string());
                    return Self::mock(&model);
                }
                _ => {}
            }
        }

        // Implicit vendor discovery from environment
        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            let endpoint = custom_endpoint
                .or_else(|| std::env::var("OPENAI_BASE_URL").ok())
                .or_else(|| Some("https://api.openai.com/v1".to_string()));
            let model = custom_model.unwrap_or_else(|| "gpt-4o-mini".to_string());
            return Self::remote(AIProviderType::OpenAi, &model, endpoint, Some(key))
                .with_timeout_and_retries(timeout_seconds, max_retries);
        }

        if let Ok(host) = std::env::var("OLLAMA_HOST") {
            let model = custom_model.unwrap_or_else(|| "llama3".to_string());
            return Self::remote(AIProviderType::Ollama, &model, Some(host), None)
                .with_timeout_and_retries(timeout_seconds, max_retries);
        }

        if let Ok(key) = std::env::var("GEMINI_API_KEY") {
            let endpoint = custom_endpoint
                .or_else(|| Some("https://generativelanguage.googleapis.com/v1beta".to_string()));
            let model = custom_model.unwrap_or_else(|| "gemini-1.5-flash".to_string());
            return Self::remote(AIProviderType::Gemini, &model, endpoint, Some(key))
                .with_timeout_and_retries(timeout_seconds, max_retries);
        }

        // Default to MockAIReasoner when no external provider is configured
        Self::default_mock()
    }

    /// Validates configuration parameters.
    pub fn validate(&self) -> Result<(), String> {
        if self.model_name.trim().is_empty() {
            return Err("Model name cannot be empty".to_string());
        }

        if self.provider_type != AIProviderType::Mock {
            if let Some(endpoint) = &self.endpoint {
                if endpoint.trim().is_empty() {
                    return Err("Remote endpoint cannot be empty if specified".to_string());
                }
            }
        }

        Ok(())
    }
}

/// Provider status structure for runtime inspection and UI visibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIProviderStatus {
    pub provider_id: String,
    pub provider_name: String,
    pub provider_type: String,
    pub is_available: bool,
    pub is_mock: bool,
}

/// Extracts safe status information from an AIReasoner instance.
pub fn get_reasoner_status(reasoner: &dyn AIReasoner) -> AIProviderStatus {
    AIProviderStatus {
        provider_id: reasoner.provider_id().to_string(),
        provider_name: reasoner.provider_name().to_string(),
        provider_type: reasoner.provider_type().to_string(),
        is_available: reasoner.is_available(),
        is_mock: reasoner.provider_id() == "mock",
    }
}

/// Factory that instantiates an AIReasoner conforming to the specified configuration.
pub fn create_ai_reasoner(config: &AIProviderConfig) -> Result<Arc<dyn AIReasoner>, AIReasoningError> {
    config.validate().map_err(AIReasoningError::InvalidConfiguration)?;

    match config.provider_type {
        AIProviderType::Mock => Ok(Arc::new(MockAIReasoner::with_model(&config.model_name))),
        AIProviderType::RemoteLlm
        | AIProviderType::Ollama
        | AIProviderType::OpenAi
        | AIProviderType::Gemini => Ok(Arc::new(RemoteLLMReasoner::new(config.clone()))),
    }
}

/// Factory that creates the default development AI Reasoner (MockAIReasoner).
pub fn create_default_ai_reasoner() -> Arc<dyn AIReasoner> {
    Arc::new(MockAIReasoner::new())
}

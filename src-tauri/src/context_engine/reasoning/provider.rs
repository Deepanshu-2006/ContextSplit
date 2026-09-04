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
            timeout_seconds: 30,
            max_retries: 2,
            custom_headers: HashMap::new(),
        }
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

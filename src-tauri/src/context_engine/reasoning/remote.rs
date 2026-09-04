use std::time::Duration;
use serde_json::{json, Value};

use super::models::{
    AIReasoningError, ContextReasoningResult, EvidencePackage, ReasoningProviderInfo,
};
use super::provider::AIProviderConfig;
use super::traits::AIReasoner;
use super::validator::{parse_and_validate_response, validate_explanation_grounding};

/// Generic adapter for Remote LLM providers (e.g., OpenAI, Ollama, Gemini, OpenRouter, vLLM).
/// Features:
/// - Structured JSON output enforcement
/// - Strict schema validation & injection scanning
/// - Configurable timeout & bounded exponential backoff retries
/// - Sanitized logging without leaking sensitive API credentials
/// - Pure read-only reasoning over `&EvidencePackage` with zero database/workspace mutation
#[derive(Debug, Clone)]
pub struct RemoteLLMReasoner {
    pub config: AIProviderConfig,
}

impl RemoteLLMReasoner {
    pub fn new(config: AIProviderConfig) -> Self {
        Self { config }
    }

    /// Resolves the chat completion endpoint URL based on provider configuration.
    pub fn resolve_chat_endpoint(&self) -> Result<String, AIReasoningError> {
        let base = self
            .config
            .endpoint
            .as_deref()
            .ok_or_else(|| AIReasoningError::ProviderUnavailable("Remote LLM endpoint is not configured".to_string()))?
            .trim_end_matches('/');

        if base.ends_with("/chat/completions") {
            Ok(base.to_string())
        } else if base.ends_with("/v1") {
            Ok(format!("{}/chat/completions", base))
        } else {
            Ok(format!("{}/v1/chat/completions", base))
        }
    }

    /// Returns a sanitized URL for logging without revealing query parameters or tokens.
    fn sanitized_endpoint(&self) -> String {
        self.config
            .endpoint
            .as_deref()
            .and_then(|u| url::Url::parse(u).ok())
            .map(|mut parsed| {
                parsed.set_query(None);
                parsed.set_password(None).ok();
                parsed.set_username("").ok();
                parsed.to_string()
            })
            .unwrap_or_else(|| self.config.endpoint.clone().unwrap_or_else(|| "unconfigured".to_string()))
    }

    /// Generates the strict system prompt detailing the output schema and reasoning requirements.
    pub fn generate_system_prompt(&self) -> &'static str {
        r#"You are ContextSplit's AI Context Reasoner.
Your task is to analyze an EvidencePackage containing facts gathered by the deterministic Context Engine and produce a structured contextual interpretation with a human-readable explanation.

CRITICAL ARCHITECTURAL CONSTRAINTS:
1. Purely Advisory: You CANNOT move items, create workspaces, modify databases, or delete items.
2. Grounded Evidence Only: The explanation must ONLY use evidence present in the EvidencePackage. DO NOT introduce unsupported facts, fictitious workspaces, unobserved activities, or speculative details.
3. No Executable Code: NEVER emit executable instructions, SQL statements, shell commands, script tags, or code execution calls.
4. DO NOT Expose Internal Scores or Embeddings: NEVER include raw internal scores, confidence percentages, decimal numbers (e.g. 0.85, 89%), weights, or embeddings (e.g. cosine similarity, vectors, [0.12, ...]) in the explanation. Internal confidence belongs ONLY in the numeric "confidence" field.
5. Communicate Uncertainty: If evidence is insufficient, ambiguous, or competing, the explanation must clearly communicate uncertainty.

HUMAN-READABLE EXPLANATION GUIDELINES:
- Generate clear, concise natural language that explains the context association to the user.
- Reference actual domain names, apps, related items, or activity sessions present in the EvidencePackage.
- When evidence is strong:
  Example: "This appears related to Paris Trip because it is similar to several recent travel-research items and shares domains already associated with that context."
- When evidence is insufficient or ambiguous:
  Example: "This may be related to Paris Trip, but the available evidence is limited."
  Example: "This may be related to either Work or Personal, but the available evidence shows competing signals between them."
  Example: "There is insufficient evidence to associate this item with a context; no matching workspaces or activity patterns were found."

JSON OUTPUT SCHEMA:
Output MUST be valid JSON adhering exactly to the following schema:
{
  "decision": "existing_context" | "new_context" | "uncertain" | "no_context",
  "suggested_context_reference": string | null,
  "suggested_name": string | null,
  "confidence": float (between 0.0 and 1.0),
  "explanation": string (human-readable natural language explanation without scores or embeddings),
  "evidence_references": [string, ...],
  "uncertainty_reason": string | null
}

DECISION RULES:
- "existing_context": Use when item clearly belongs to an existing workspace. "suggested_context_reference" MUST be non-empty and match a candidate in the evidence package.
- "new_context": Use when item forms a cohesive new activity cluster. "suggested_name" MUST be non-empty.
- "uncertain": Use when there are competing candidate contexts or ambiguity. "uncertainty_reason" MUST be non-empty, and explanation must communicate uncertainty.
- "no_context": Use when evidence is insufficient or matching contexts are archived. Item remains in Inbox. Explanation must communicate limited or missing evidence.
"#
    }

    /// Formats the EvidencePackage into structured user prompt content.
    pub fn format_user_prompt(&self, package: &EvidencePackage) -> String {
        let package_json = serde_json::to_string_pretty(package)
            .unwrap_or_else(|_| "{}".to_string());
        format!(
            "Analyze the following EvidencePackage and return your advisory interpretation as JSON:\n\n```json\n{}\n```",
            package_json
        )
    }

    /// Combines system and user prompts into a full prompt representation.
    pub fn build_full_prompt(&self, package: &EvidencePackage) -> String {
        format!(
            "=== SYSTEM ===\n{}\n=== USER ===\n{}",
            self.generate_system_prompt(),
            self.format_user_prompt(package)
        )
    }

    /// Constructs the structured request payload adhering to OpenAI/Ollama compatible API.
    pub fn build_request_payload(&self, package: &EvidencePackage) -> Value {
        json!({
            "model": self.config.model_name,
            "messages": [
                { "role": "system", "content": self.generate_system_prompt() },
                { "role": "user", "content": self.format_user_prompt(package) }
            ],
            "response_format": { "type": "json_object" },
            "temperature": 0.1,
            "max_tokens": 1500
        })
    }

    /// Parses and validates a raw response string from a remote LLM.
    pub fn process_remote_response(&self, raw_json: &str) -> Result<ContextReasoningResult, AIReasoningError> {
        let mut result = parse_and_validate_response(raw_json)
            .map_err(|e| AIReasoningError::InvalidResponse(e.to_string()))?;

        result.provider_info = Some(ReasoningProviderInfo {
            provider_id: self.provider_id().to_string(),
            provider_type: "remote_llm".to_string(),
            model_version: self.config.model_name.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        });

        Ok(result)
    }

    /// Extracts response content string from an OpenAI/Ollama compatible chat response JSON.
    pub fn extract_content_from_response(&self, response_body: &Value) -> Result<String, AIReasoningError> {
        if let Some(content) = response_body["choices"][0]["message"]["content"].as_str() {
            return Ok(content.trim().to_string());
        }

        if let Some(content) = response_body["message"]["content"].as_str() {
            return Ok(content.trim().to_string());
        }

        if response_body.get("decision").is_some() {
            return Ok(response_body.to_string());
        }

        Err(AIReasoningError::InvalidResponse(
            "Response body missing expected choices[0].message.content structure".to_string(),
        ))
    }
}

impl AIReasoner for RemoteLLMReasoner {
    fn provider_name(&self) -> &str {
        &self.config.model_name
    }

    fn provider_id(&self) -> &str {
        match self.config.provider_type {
            super::provider::AIProviderType::Ollama => "ollama",
            super::provider::AIProviderType::OpenAi => "openai",
            super::provider::AIProviderType::Gemini => "gemini",
            _ => "remote_llm",
        }
    }

    fn provider_type(&self) -> &str {
        "remote_llm"
    }

    fn is_available(&self) -> bool {
        self.config.endpoint.is_some()
    }

    fn analyze_context(
        &self,
        package: &EvidencePackage,
    ) -> Result<ContextReasoningResult, AIReasoningError> {
        let endpoint_url = self.resolve_chat_endpoint()?;
        let sanitized_url = self.sanitized_endpoint();
        let payload = self.build_request_payload(package);

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(self.config.timeout_seconds))
            .build()
            .map_err(|e| AIReasoningError::ProviderUnavailable(format!("Failed to build HTTP client: {}", e)))?;

        let max_attempts = self.config.max_retries.saturating_add(1);
        let mut last_error = None;

        for attempt in 1..=max_attempts {
            // Sanitized logging without sensitive tokens
            #[cfg(debug_assertions)]
            eprintln!(
                "[AIReasoner:remote] Attempt {}/{} -> provider: '{}', model: '{}', endpoint: '{}'",
                attempt, max_attempts, self.provider_id(), self.config.model_name, sanitized_url
            );

            let mut request = client.post(&endpoint_url).json(&payload);

            if let Some(key) = &self.config.api_key {
                request = request.header("Authorization", format!("Bearer {}", key));
            }

            for (k, v) in &self.config.custom_headers {
                request = request.header(k, v);
            }

            match request.send() {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        match response.json::<Value>() {
                            Ok(body) => match self.extract_content_from_response(&body) {
                                Ok(content_str) => match self.process_remote_response(&content_str) {
                                    Ok(result) => {
                                        if let Err(e) = validate_explanation_grounding(&result, package) {
                                            last_error = Some(AIReasoningError::InvalidResponse(format!(
                                                "AI Reasoner output failed factual grounding: {}",
                                                e
                                            )));
                                        } else {
                                            return Ok(result);
                                        }
                                    }
                                    Err(err) => {
                                        last_error = Some(err);
                                    }
                                },
                                Err(err) => {
                                    last_error = Some(err);
                                }
                            },
                            Err(e) => {
                                last_error = Some(AIReasoningError::InvalidResponse(format!(
                                    "Failed to parse provider JSON response: {}",
                                    e
                                )));
                            }
                        }
                    } else if status.as_u16() == 401 || status.as_u16() == 403 {
                        // Unrecoverable authorization error - do not retry
                        return Err(AIReasoningError::ProviderUnavailable(format!(
                            "Authentication failed (HTTP {}) connecting to remote LLM at '{}'. Check credentials.",
                            status, sanitized_url
                        )));
                    } else if status.as_u16() == 429 || status.is_server_error() {
                        // Transient rate limit or server error - eligible for retry
                        last_error = Some(AIReasoningError::InferenceFailed(format!(
                            "Remote LLM returned transient error status {} from '{}'",
                            status, sanitized_url
                        )));
                    } else {
                        // Client error (e.g. 400 Bad Request)
                        return Err(AIReasoningError::InferenceFailed(format!(
                            "Remote LLM rejected request with status {} from '{}'",
                            status, sanitized_url
                        )));
                    }
                }
                Err(err) => {
                    let is_timeout = err.is_timeout();
                    last_error = Some(AIReasoningError::ProviderUnavailable(format!(
                        "Network failure calling '{}': {}{}",
                        sanitized_url,
                        err,
                        if is_timeout { " (request timed out)" } else { "" }
                    )));
                }
            }

            // Backoff before next retry if attempts remain
            if attempt < max_attempts {
                let backoff_ms = 200 * (1 << (attempt - 1));
                std::thread::sleep(Duration::from_millis(backoff_ms));
            }
        }

        Err(last_error.unwrap_or_else(|| {
            AIReasoningError::ProviderUnavailable(format!(
                "Failed to execute contextual reasoning after {} attempts to '{}'",
                max_attempts, sanitized_url
            ))
        }))
    }
}


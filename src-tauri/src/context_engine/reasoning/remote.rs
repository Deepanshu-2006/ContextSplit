use super::models::{AIReasoningError, ContextReasoningResult, EvidencePackage};
use super::provider::AIProviderConfig;
use super::traits::AIReasoner;
use super::validator::parse_and_validate_response;

/// Generic architecture for Remote LLM providers (e.g., OpenAI, Anthropic, Gemini, Ollama, vLLM).
/// Encapsulates prompt generation, structured schema enforcement, and response validation.
///
/// NOTE: In accordance with architectural guidelines, this provider does not initiate
/// live external network calls during this phase ("Do not connect to an external API yet").
/// It prepares the exact request payload formatting and schema validation for remote activation.
#[derive(Debug, Clone)]
pub struct RemoteLLMReasoner {
    pub config: AIProviderConfig,
}

impl RemoteLLMReasoner {
    pub fn new(config: AIProviderConfig) -> Self {
        Self { config }
    }

    /// Generates the strict system prompt detailing the output schema and reasoning requirements.
    pub fn generate_system_prompt(&self) -> &'static str {
        r#"You are ContextSplit's AI Context Reasoner.
Your task is to analyze an EvidencePackage containing facts gathered by the deterministic Context Engine and produce a structured contextual interpretation.

CRITICAL ARCHITECTURAL CONSTRAINTS:
1. You are purely advisory. You CANNOT move items, create workspaces, modify databases, or delete items.
2. Ground all deductions strictly in the provided evidence. DO NOT invent facts, workspace IDs, or evidence.
3. NEVER emit executable instructions, SQL statements, shell commands, script tags, or code execution calls.
4. Output MUST be valid JSON adhering exactly to the following schema:

{
  "decision": "existing_context" | "new_context" | "uncertain" | "no_context",
  "suggested_context_reference": string | null,
  "suggested_name": string | null,
  "confidence": float (between 0.0 and 1.0),
  "explanation": string (plain natural language explanation),
  "evidence_references": [string, ...],
  "uncertainty_reason": string | null
}

DECISION RULES:
- "existing_context": Use when item clearly belongs to an existing workspace. "suggested_context_reference" MUST be non-empty.
- "new_context": Use when item forms a cohesive new activity cluster. "suggested_name" MUST be non-empty.
- "uncertain": Use when there are competing candidate contexts or ambiguity. "uncertainty_reason" MUST be non-empty.
- "no_context": Use when evidence is insufficient (< 25% confidence) or matching contexts are archived. Item remains in Inbox.
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

    /// Parses and validates a raw response string from a remote LLM.
    pub fn process_remote_response(&self, raw_json: &str) -> Result<ContextReasoningResult, AIReasoningError> {
        parse_and_validate_response(raw_json).map_err(|e| AIReasoningError::InvalidResponse(e.to_string()))
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
        _package: &EvidencePackage,
    ) -> Result<ContextReasoningResult, AIReasoningError> {
        // Enforce the rule: Do not connect to an external API yet.
        // If an endpoint is provided, report that external network transport is deferred to production integration.
        let endpoint_desc = self
            .config
            .endpoint
            .as_deref()
            .unwrap_or("unconfigured");

        Err(AIReasoningError::ProviderUnavailable(format!(
            "Remote LLM provider '{}' ({}) at endpoint '{}' is deferred: external network connections are not enabled yet. Use MockAIReasoner for development.",
            self.config.model_name,
            self.provider_id(),
            endpoint_desc
        )))
    }
}

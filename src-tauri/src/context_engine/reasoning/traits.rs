use super::models::{AIReasoningError, ContextReasoningResult, EvidencePackage};

/// Pluggable interface for AI context reasoning providers.
/// Enables swapping LLM backends (Mock, Ollama, Gemini, OpenAI, Anthropic)
/// without altering the Context Engine or database schema.
///
/// CRITICAL ARCHITECTURAL INVARIANT:
/// Implementors of this trait have ZERO direct access to the database,
/// filesystem, or application state. They receive strictly an immutable
/// `&EvidencePackage` and return an advisory `ContextReasoningResult`.
pub trait AIReasoner: Send + Sync {
    /// Analyzes structured facts from an EvidencePackage and returns an interpretation.
    fn analyze_context(&self, package: &EvidencePackage) -> Result<ContextReasoningResult, AIReasoningError>;

    /// Suggests a concise (2–4 words), evidence-grounded name for a new ContextCandidate.
    fn suggest_candidate_name(&self, package: &EvidencePackage) -> Result<String, AIReasoningError> {
        let result = self.analyze_context(package)?;
        if let Some(name) = result.suggested_name {
            if !name.trim().is_empty() {
                return Ok(name.trim().to_string());
            }
        }
        Err(AIReasoningError::InvalidResponse(
            "No candidate name could be suggested from evidence".to_string(),
        ))
    }

    /// Human-readable display name of the reasoning provider (e.g., "Mock AI Reasoner", "Gemini 1.5 Flash").
    fn provider_name(&self) -> &str;

    /// Unique identifier for the provider (e.g., "mock", "ollama", "gemini").
    fn provider_id(&self) -> &str;

    /// Type/category of the provider (e.g., "mock", "remote_llm").
    fn provider_type(&self) -> &str {
        self.provider_id()
    }

    /// Checks if the provider is currently available and properly configured.
    fn is_available(&self) -> bool {
        true
    }
}


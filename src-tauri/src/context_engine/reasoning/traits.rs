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

    /// Human-readable display name of the reasoning provider (e.g., "Mock AI Reasoner", "Gemini 1.5 Flash").
    fn provider_name(&self) -> &str;

    /// Unique identifier for the provider (e.g., "mock", "ollama", "gemini").
    fn provider_id(&self) -> &str;
}

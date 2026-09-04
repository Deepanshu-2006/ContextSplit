use serde::{Deserialize, Serialize};

/// Curated, structured evidence compiled by the deterministic Context Engine.
/// This is the SOLE data structure passed to the AI reasoning layer.
/// It contains verified facts, relationships, and context evaluations,
/// without exposing arbitrary raw files or unvetted user data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidencePackage {
    pub target_item: TargetItemEvidence,
    pub candidate_contexts: Vec<CandidateContextEvidence>,
    pub relationships: Vec<ItemRelationshipEvidence>,
    pub session_context: Option<SessionEvidence>,
    pub temporal_context: TemporalEvidence,
    pub system_metadata: EvidenceMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetItemEvidence {
    pub id: String,
    pub item_type: String,
    pub title: Option<String>,
    pub content_snippet: String,
    pub app_name: Option<String>,
    pub domain: Option<String>,
    pub session_id: Option<String>,
    pub captured_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateContextEvidence {
    pub context_id: String,
    pub name: String,
    /// "workspace" | "candidate"
    pub context_type: String,
    /// "active" | "dormant" | "archived"
    pub lifecycle_state: String,
    pub deterministic_score: f64,
    pub deterministic_confidence: f64,
    pub confidence_band: String,
    pub evidence_factors: Vec<String>,
    pub item_count: usize,
    pub temporal_recency_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemRelationshipEvidence {
    pub other_item_id: String,
    pub other_item_title: Option<String>,
    pub relationship_type: String,
    pub strength: f64,
    pub target_context_id: Option<String>,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvidence {
    pub session_id: String,
    pub item_count: usize,
    pub associated_workspaces: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalEvidence {
    pub age_days: f64,
    pub recency_decay_weight: f64,
    pub is_recent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceMetadata {
    pub generated_at: String,
    pub engine_version: String,
    pub privacy_boundary: String,
}

/// Structured interpretation returned by the AI reasoning layer.
/// The AI layer provides an interpretation and advice, but cannot directly
/// modify items, memberships, or workspaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextReasoningResult {
    pub recommended_context_id: Option<String>,
    pub recommended_context_name: Option<String>,
    pub confidence: f64,
    pub confidence_band: String,
    pub reasoning_summary: String,
    pub key_factors: Vec<String>,
    pub alternative_interpretations: Vec<AlternativeContextReasoning>,
    pub is_ambiguous: bool,
    /// "suggest_workspace" | "keep_inbox" | "suggest_new_context" | "review_conflict"
    pub suggested_action: String,
    pub suggested_label: Option<String>,
    pub provider_info: ReasoningProviderInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlternativeContextReasoning {
    pub context_id: String,
    pub context_name: String,
    pub confidence: f64,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningProviderInfo {
    pub provider_id: String,
    pub provider_type: String,
    pub model_version: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AIReasoningError {
    ProviderUnavailable(String),
    InvalidEvidence(String),
    InferenceFailed(String),
}

impl std::fmt::Display for AIReasoningError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AIReasoningError::ProviderUnavailable(msg) => write!(f, "AI provider unavailable: {}", msg),
            AIReasoningError::InvalidEvidence(msg) => write!(f, "Invalid evidence package: {}", msg),
            AIReasoningError::InferenceFailed(msg) => write!(f, "AI reasoning inference failed: {}", msg),
        }
    }
}

impl std::error::Error for AIReasoningError {}

use serde::{Deserialize, Serialize};

/// Curated, structured evidence compiled by the deterministic Context Engine.
/// This is the SOLE data structure passed to the AI reasoning layer.
///
/// CRITICAL PRIVACY & ARCHITECTURAL INVARIANTS:
/// 1. Only contains information strictly necessary for contextual reasoning.
/// 2. Does NOT send the entire database (filters to top relevant items and signals).
/// 3. Does NOT include unnecessary private information (scrubs credentials, tokens, passwords).
/// 4. Does NOT include raw vector embeddings (`Vec<f32>` is completely absent).
/// 5. Does NOT include internal database IDs, raw rowids, or system process IDs
///    except for controlled reference IDs (`item_id`, `context_id`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidencePackage {
    /// Curated metadata of the target item being reasoned about.
    pub target_item: TargetItemEvidence,

    /// Candidate contexts (workspaces or detected candidates) evaluated by the deterministic engine.
    pub candidate_contexts: Vec<CandidateContextEvidence>,

    /// Top relevant related items that connect to the candidate contexts.
    pub relevant_related_items: Vec<RelatedItemEvidence>,

    /// Specific item-to-item relationship facts with strengths and human-readable summaries.
    pub relationships: Vec<ItemRelationshipEvidence>,

    /// Human-readable semantic similarity summaries for top matches (NO raw vectors!).
    pub semantic_similarity_summaries: Vec<SemanticSimilaritySummary>,

    /// Relevant context sessions (only sessions associated with this item or related items).
    pub relevant_sessions: Vec<SessionEvidence>,

    /// Backward-compatible primary session context for the target item.
    pub session_context: Option<SessionEvidence>,

    /// Semantic profile summaries for candidate workspaces (theme, topical alignment, match level).
    pub workspace_semantic_profiles: Vec<WorkspaceProfileSummary>,

    /// Filtered, privacy-sanitized contextual signals (active app, domain, user action types).
    pub contextual_signals: Vec<CuratedContextSignal>,

    /// Temporal recency and decay evidence.
    pub temporal_context: TemporalEvidence,

    /// Package metadata ensuring reproducibility, versioning, and privacy boundaries.
    pub system_metadata: EvidenceMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RelatedItemEvidence {
    pub item_id: String,
    pub title: Option<String>,
    pub item_type: String,
    pub snippet: String,
    pub workspace_id: Option<String>,
    pub workspace_name: Option<String>,
    pub relevance_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ItemRelationshipEvidence {
    pub other_item_id: String,
    pub other_item_title: Option<String>,
    pub relationship_type: String,
    pub strength: f64,
    pub target_context_id: Option<String>,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SemanticSimilaritySummary {
    pub other_item_id: String,
    pub other_item_title: Option<String>,
    pub workspace_name: Option<String>,
    pub similarity_score: f64,
    pub similarity_band: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionEvidence {
    pub session_id: String,
    pub item_count: usize,
    pub is_target_session: bool,
    pub associated_workspaces: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceProfileSummary {
    pub workspace_id: String,
    pub workspace_name: String,
    pub has_profile: bool,
    pub alignment_score: Option<f64>,
    pub alignment_band: Option<String>,
    pub theme_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CuratedContextSignal {
    pub signal_type: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TemporalEvidence {
    pub age_days: f64,
    pub recency_decay_weight: f64,
    pub is_recent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceMetadata {
    pub generated_at: String,
    pub engine_version: String,
    pub total_db_items_excluded: usize,
    pub privacy_boundary: String,
}

/// Structured interpretation returned by the AI reasoning layer.
/// The AI layer provides an interpretation and advice, but cannot directly
/// modify items, memberships, or workspaces.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AlternativeContextReasoning {
    pub context_id: String,
    pub context_name: String,
    pub confidence: f64,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

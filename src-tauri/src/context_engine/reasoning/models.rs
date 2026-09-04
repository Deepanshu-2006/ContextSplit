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

    /// Assessment of deterministic evidence strength, communicating weakness or ambiguity.
    pub deterministic_assessment: DeterministicEvidenceAssessment,

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

/// Explicit assessment of deterministic evidence strength.
/// Communicates weakness, ambiguity, and competing candidate contexts to the AI reasoner.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeterministicEvidenceAssessment {
    /// Overall confidence band: "high" | "moderate" | "uncertain" | "weak" | "none"
    pub confidence_band: String,
    /// True if deterministic evidence is weak or below the actionable threshold (< 0.25)
    pub is_weak_evidence: bool,
    /// True if competing signals or top candidates are close in score
    pub is_ambiguous: bool,
    /// Names of competing candidate contexts when ambiguous
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub competing_contexts: Vec<String>,
    /// Natural-language summary of deterministic evidence strength or weakness
    pub assessment_summary: String,
}

/// Explicit decision categories for ContextReasoningResult.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextReasoningDecision {
    ExistingContext,
    NewContext,
    Uncertain,
    NoContext,
}

impl std::fmt::Display for ContextReasoningDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContextReasoningDecision::ExistingContext => write!(f, "existing_context"),
            ContextReasoningDecision::NewContext => write!(f, "new_context"),
            ContextReasoningDecision::Uncertain => write!(f, "uncertain"),
            ContextReasoningDecision::NoContext => write!(f, "no_context"),
        }
    }
}

/// Strict interpretation returned by the AI reasoning layer.
///
/// CRITICAL ARCHITECTURAL RULES:
/// 1. Schema must be validated before use.
/// 2. Must never contain executable code, SQL commands, or script tags.
/// 3. Must not directly mutate database records or application state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextReasoningResult {
    /// Categorical decision: "existing_context" | "new_context" | "uncertain" | "no_context"
    pub decision: ContextReasoningDecision,

    /// Reference to an existing workspace ID or candidate ID (required if decision == existing_context).
    pub suggested_context_reference: Option<String>,

    /// Suggested human-readable name (required if decision == new_context; optional if existing_context).
    pub suggested_name: Option<String>,

    /// Normalized confidence score strictly in range [0.0, 1.0].
    pub confidence: f64,

    /// Human-readable explanation of why this decision was made.
    pub explanation: String,

    /// Specific evidence items or factors grounded in the EvidencePackage.
    pub evidence_references: Vec<String>,

    /// Explicit rationale when the decision is uncertain or no_context.
    pub uncertainty_reason: Option<String>,

    /// Optional provider information.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_info: Option<ReasoningProviderInfo>,

    /// Optional alternative context evaluations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternative_interpretations: Vec<AlternativeContextReasoning>,
}

impl ContextReasoningResult {
    /// Validates this result against the strict schema.
    pub fn validate(&self) -> Result<(), super::validator::ReasoningValidationError> {
        super::validator::validate_reasoning_result(self)
    }
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
    InvalidResponse(String),
    InvalidConfiguration(String),
}

impl std::fmt::Display for AIReasoningError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AIReasoningError::ProviderUnavailable(msg) => write!(f, "AI provider unavailable: {}", msg),
            AIReasoningError::InvalidEvidence(msg) => write!(f, "Invalid evidence package: {}", msg),
            AIReasoningError::InferenceFailed(msg) => write!(f, "AI reasoning inference failed: {}", msg),
            AIReasoningError::InvalidResponse(msg) => write!(f, "AI response failed schema validation: {}", msg),
            AIReasoningError::InvalidConfiguration(msg) => write!(f, "Invalid AI provider configuration: {}", msg),
        }
    }
}

impl std::error::Error for AIReasoningError {}

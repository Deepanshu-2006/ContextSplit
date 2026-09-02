use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub workspace_id: Option<String>,
    pub session_id: Option<String>,
    pub item_type: String, // "text", "image", "url", "file"
    pub content: String,
    pub title: Option<String>,
    pub metadata: String, // JSON string
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContextSignal {
    pub id: String,
    pub workspace_id: Option<String>,
    pub item_id: Option<String>,
    pub signal_type: String,
    pub session_id: Option<String>,
    pub payload: String, // JSON string
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub started_at: String,
    pub last_activity_at: String,
    pub ended_at: Option<String>,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ItemRelationship {
    pub id: String,
    pub item_a_id: String,
    pub item_b_id: String,
    pub relationship_type: String,
    pub strength: f64,
    pub metadata: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ItemEmbedding {
    pub item_id: String,
    pub embedding: Vec<f32>,
    pub provider: String,
    pub model: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ItemEmbeddingMetadata {
    pub item_id: String,
    pub provider: String,
    pub model: String,
    pub created_at: String,
    pub dimensions: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceEmbedding {
    pub workspace_id: String,
    pub embedding: Vec<f32>,
    pub provider: String,
    pub model: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ItemSemanticStatus {
    pub item_id: String,
    pub status: String,
    pub retries: i32,
    pub model_version: Option<String>,
    pub updated_at: String,
}

/// A detected potential context grouping. Separate from Workspace.
/// A Workspace is user-confirmed; a ContextCandidate is a suggestion from the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextCandidate {
    pub id: String,
    /// "detected" | "suggested" | "accepted" | "dismissed" | "expired"
    pub status: String,
    /// 0.0 – 1.0
    pub confidence: f64,
    /// Human-readable inferred label (e.g. "KitKat project"), may be null
    pub label: Option<String>,
    /// JSON: detection_reason, item_count, signal_summary, etc.
    pub metadata: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Join table: which Items belong to a ContextCandidate, with per-item relevance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextCandidateItem {
    pub candidate_id: String,
    pub item_id: String,
    /// How central this item is to the candidate (0.0 – 1.0)
    pub relevance_score: f64,
    pub added_at: String,
}

/// Full candidate + its member items, returned by the discovery commands.
#[derive(Debug, Serialize, Deserialize)]
pub struct CandidateWithItems {
    pub candidate: ContextCandidate,
    pub items: Vec<CandidateMember>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CandidateMember {
    pub membership: ContextCandidateItem,
    pub item: Item,
}

// ---------------------------------------------------------------------------
// Candidate Scoring Models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrongestRelationship {
    pub item_a_id: String,
    pub item_b_id: String,
    pub relationship_type: String,
    pub strength: f64,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributingItem {
    pub item_id: String,
    pub relevance_score: f64,
    pub title: Option<String>,
    pub item_type: String,
    pub primary_signal: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRequirements {
    pub min_item_count: usize,
    pub min_avg_relationship_strength: f64,
    pub min_semantic_coherence: f64,
    pub min_independent_signals: usize,
    pub min_confidence: f64,
}

impl Default for EvidenceRequirements {
    fn default() -> Self {
        Self {
            min_item_count: 2,
            min_avg_relationship_strength: 0.50,
            min_semantic_coherence: 0.40,
            min_independent_signals: 2,
            min_confidence: 0.45,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateScoringWeights {
    pub item_count_weight: f64,
    pub item_count_saturation: usize,
    pub relationship_strength_weight: f64,
    pub semantic_similarity_weight: f64,
    pub shared_session_weight: f64,
    pub temporal_proximity_weight: f64,
    pub common_domain_weight: f64,
    pub common_app_weight: f64,
    pub unassigned_items_weight: f64,
    pub user_history_weight: f64,
    pub workspace_suppression_threshold: f64,
    pub evidence_requirements: EvidenceRequirements,
}

impl Default for CandidateScoringWeights {
    fn default() -> Self {
        Self {
            item_count_weight: 0.15,
            item_count_saturation: 5,
            relationship_strength_weight: 0.35,
            semantic_similarity_weight: 0.15,
            shared_session_weight: 0.10,
            temporal_proximity_weight: 0.10,
            common_domain_weight: 0.05,
            common_app_weight: 0.05,
            unassigned_items_weight: 0.05,
            user_history_weight: 0.00,
            workspace_suppression_threshold: 0.60,
            evidence_requirements: EvidenceRequirements::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSuppressionMatch {
    pub workspace_id: String,
    pub workspace_name: String,
    pub match_score: f64,
    /// "strong_match" (suppressed) | "ambiguous_match" | "novel"
    pub match_type: String,
    pub is_suppressed: bool,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateScore {
    pub confidence: f64,
    pub confidence_percentage: String,
    pub evidence: Vec<String>,
    pub strongest_relationships: Vec<StrongestRelationship>,
    pub contributing_items: Vec<ContributingItem>,
    pub component_scores: std::collections::HashMap<String, f64>,
    pub workspace_match: Option<WorkspaceSuppressionMatch>,
    pub is_evidence_sufficient: bool,
    pub insufficient_evidence_reasons: Vec<String>,
}


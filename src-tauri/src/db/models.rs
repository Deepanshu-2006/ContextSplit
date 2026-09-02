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

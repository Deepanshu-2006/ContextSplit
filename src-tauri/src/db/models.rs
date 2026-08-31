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

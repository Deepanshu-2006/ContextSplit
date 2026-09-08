use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Core categorization for contextual evidence items.
/// Covers direct capture observations, environment metadata, graph connections,
/// semantic inferences, and user interaction history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceCategory {
    /// Frontmost operating system application name or bundle/process identifier
    SourceApp,
    /// Active window title when capture or activity occurred
    SourceWindow,
    /// Extracted web domain/hostname (e.g. "github.com")
    BrowserDomain,
    /// Full or normalized URL metadata
    BrowserUrl,
    /// Mechanism by which content entered the system (clipboard, drag_and_drop, notch, manual)
    CaptureMethod,
    /// Close temporal proximity between items (e.g., captured within minutes)
    TemporalProximity,
    /// Activity session association (items captured within the same user session)
    SameSession,
    /// Direct graph relationship between items in the relationship store
    RelatedItem,
    /// Embedding vector cosine similarity between item representations
    SemanticSimilarity,
    /// Historical user organization patterns associated with workspaces
    WorkspaceHistory,
    /// Explicit, authoritative user assignment to a workspace
    ConfirmedMembership,
    /// Explicit user feedback (e.g., dismissing a suggestion or candidate)
    UserFeedback,
    /// File metadata (filename, extension, MIME type, size)
    FileMetadata,
    /// Data type classification (text, url, image, file, code snippet)
    ContentType,
}

impl EvidenceCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            EvidenceCategory::SourceApp => "source_app",
            EvidenceCategory::SourceWindow => "source_window",
            EvidenceCategory::BrowserDomain => "browser_domain",
            EvidenceCategory::BrowserUrl => "browser_url",
            EvidenceCategory::CaptureMethod => "capture_method",
            EvidenceCategory::TemporalProximity => "temporal_proximity",
            EvidenceCategory::SameSession => "same_session",
            EvidenceCategory::RelatedItem => "related_item",
            EvidenceCategory::SemanticSimilarity => "semantic_similarity",
            EvidenceCategory::WorkspaceHistory => "workspace_history",
            EvidenceCategory::ConfirmedMembership => "confirmed_membership",
            EvidenceCategory::UserFeedback => "user_feedback",
            EvidenceCategory::FileMetadata => "file_metadata",
            EvidenceCategory::ContentType => "content_type",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "source_app" => Ok(EvidenceCategory::SourceApp),
            "source_window" => Ok(EvidenceCategory::SourceWindow),
            "browser_domain" => Ok(EvidenceCategory::BrowserDomain),
            "browser_url" => Ok(EvidenceCategory::BrowserUrl),
            "capture_method" => Ok(EvidenceCategory::CaptureMethod),
            "temporal_proximity" => Ok(EvidenceCategory::TemporalProximity),
            "same_session" => Ok(EvidenceCategory::SameSession),
            "related_item" => Ok(EvidenceCategory::RelatedItem),
            "semantic_similarity" => Ok(EvidenceCategory::SemanticSimilarity),
            "workspace_history" => Ok(EvidenceCategory::WorkspaceHistory),
            "confirmed_membership" => Ok(EvidenceCategory::ConfirmedMembership),
            "user_feedback" => Ok(EvidenceCategory::UserFeedback),
            "file_metadata" => Ok(EvidenceCategory::FileMetadata),
            "content_type" => Ok(EvidenceCategory::ContentType),
            other => Err(format!("Unknown evidence category: '{}'", other)),
        }
    }

    /// Default provenance classification for standard evidence of this category.
    pub fn default_provenance(&self) -> EvidenceProvenance {
        match self {
            EvidenceCategory::SourceApp
            | EvidenceCategory::SourceWindow
            | EvidenceCategory::BrowserDomain
            | EvidenceCategory::BrowserUrl
            | EvidenceCategory::CaptureMethod
            | EvidenceCategory::SameSession
            | EvidenceCategory::ConfirmedMembership
            | EvidenceCategory::UserFeedback
            | EvidenceCategory::FileMetadata
            | EvidenceCategory::ContentType => EvidenceProvenance::Observed,

            EvidenceCategory::TemporalProximity
            | EvidenceCategory::RelatedItem
            | EvidenceCategory::SemanticSimilarity
            | EvidenceCategory::WorkspaceHistory => EvidenceProvenance::Inferred,
        }
    }
}

impl std::fmt::Display for EvidenceCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Explicit provenance distinguishing directly captured empirical facts from
/// derived/computed/inferred contextual relationships.
///
/// CRITICAL INVARIANT: Observed evidence and inferred evidence must remain
/// strictly distinguishable across the entire pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceProvenance {
    /// Direct, empirical facts captured from system events, OS window/app monitors,
    /// payload headers, or explicit user clicks/moves.
    Observed,
    /// Computed, model-derived, or graph-calculated metrics (e.g., semantic similarity,
    /// decay weights, graph clustering, inferred associations).
    Inferred,
}

impl EvidenceProvenance {
    pub fn as_str(&self) -> &'static str {
        match self {
            EvidenceProvenance::Observed => "observed",
            EvidenceProvenance::Inferred => "inferred",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "observed" => Ok(EvidenceProvenance::Observed),
            "inferred" => Ok(EvidenceProvenance::Inferred),
            other => Err(format!("Unknown evidence provenance: '{}'", other)),
        }
    }

    pub fn is_observed(&self) -> bool {
        matches!(self, EvidenceProvenance::Observed)
    }

    pub fn is_inferred(&self) -> bool {
        matches!(self, EvidenceProvenance::Inferred)
    }
}

impl std::fmt::Display for EvidenceProvenance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Strongly-typed entity kind referenced by an evidence item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Item,
    Session,
    Signal,
    Workspace,
    Candidate,
    Relationship,
    External,
}

impl EntityType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntityType::Item => "item",
            EntityType::Session => "session",
            EntityType::Signal => "signal",
            EntityType::Workspace => "workspace",
            EntityType::Candidate => "candidate",
            EntityType::Relationship => "relationship",
            EntityType::External => "external",
        }
    }
}

impl std::fmt::Display for EntityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Explicit entity reference providing traceability back to source database records or objects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityReference {
    pub entity_type: EntityType,
    pub entity_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl EntityReference {
    pub fn new(entity_type: EntityType, entity_id: impl Into<String>) -> Self {
        Self {
            entity_type,
            entity_id: entity_id.into(),
            label: None,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn item(id: impl Into<String>) -> Self {
        Self::new(EntityType::Item, id)
    }

    pub fn session(id: impl Into<String>) -> Self {
        Self::new(EntityType::Session, id)
    }

    pub fn signal(id: impl Into<String>) -> Self {
        Self::new(EntityType::Signal, id)
    }

    pub fn workspace(id: impl Into<String>, name: Option<String>) -> Self {
        let mut ref_obj = Self::new(EntityType::Workspace, id);
        ref_obj.label = name;
        ref_obj
    }

    pub fn candidate(id: impl Into<String>, label: Option<String>) -> Self {
        let mut ref_obj = Self::new(EntityType::Candidate, id);
        ref_obj.label = label;
        ref_obj
    }
}

/// Human-readable explanation metadata associated with an evidence item.
/// Designed for explainability in the UI without exposing internal vector floats.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceExplanation {
    /// Concise summary title (e.g., "Source Application: Google Chrome")
    pub title: String,
    /// Detailed natural-language explanation (e.g., "Captured while working in Google Chrome.")
    pub description: String,
    /// Optional short badge string for UI display (e.g., "App: Chrome", "Domain: github.com")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
}

impl EvidenceExplanation {
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            badge: None,
        }
    }

    pub fn with_badge(mut self, badge: impl Into<String>) -> Self {
        self.badge = Some(badge.into());
        self
    }
}

/// A unified, platform-independent representation of a single piece of contextual evidence.
///
/// CRITICAL ARCHITECTURAL RULES:
/// 1. ContextEvidence is **evidence only**. It does NOT decide a Workspace or ContextCandidate.
/// 2. Observed facts and Inferred relationships remain strictly distinguishable via `provenance`.
/// 3. Does not mix raw system signals with final context scoring decisions.
/// 4. Platform-independent (no OS-specific headers or non-portable types).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextEvidence {
    /// Unique identifier for this evidence record
    pub id: String,
    /// Category of evidence
    pub category: EvidenceCategory,
    /// Whether this evidence is an empirical observation or a calculated inference
    pub provenance: EvidenceProvenance,
    /// Normalized confidence or strength weight, strictly clamped to [0.0, 1.0]
    pub strength: f64,
    /// ID of the target Item this evidence applies to
    pub target_item_id: String,
    /// Optional reference to the source entity (related item, session, signal, workspace)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<EntityReference>,
    /// RFC 3339 timestamp when the evidence was observed or generated
    pub observed_at: String,
    /// Human-readable explanation metadata
    pub explanation: EvidenceExplanation,
    /// Additional structured attributes (sanitized, non-private key-values)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<String, serde_json::Value>,
}

impl ContextEvidence {
    /// Base constructor with strength clamping and auto-generated ID.
    pub fn new(
        category: EvidenceCategory,
        provenance: EvidenceProvenance,
        strength: f64,
        target_item_id: impl Into<String>,
        explanation: EvidenceExplanation,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            category,
            provenance,
            strength: strength.clamp(0.0, 1.0),
            target_item_id: target_item_id.into(),
            source_ref: None,
            observed_at: Utc::now().to_rfc3339(),
            explanation,
            attributes: HashMap::new(),
        }
    }

    /// Sets the source entity reference.
    pub fn with_source_ref(mut self, source_ref: EntityReference) -> Self {
        self.source_ref = Some(source_ref);
        self
    }

    /// Sets custom timestamp.
    pub fn with_timestamp(mut self, timestamp: impl Into<String>) -> Self {
        self.observed_at = timestamp.into();
        self
    }

    /// Adds a key-value attribute.
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    // =========================================================================
    // Standard Observed Evidence Factory Methods
    // =========================================================================

    /// Observed evidence: frontmost active application.
    pub fn observed_app(item_id: impl Into<String>, app_name: &str, timestamp: Option<&str>) -> Self {
        let t_id = item_id.into();
        let expl = EvidenceExplanation::new(
            format!("Application: {}", app_name),
            format!("Captured while active in {}.", app_name),
        )
        .with_badge(format!("App: {}", app_name));

        let mut ev = Self::new(
            EvidenceCategory::SourceApp,
            EvidenceProvenance::Observed,
            1.0,
            t_id,
            expl,
        );
        if let Some(ts) = timestamp {
            ev.observed_at = ts.to_string();
        }
        ev.attributes.insert("app_name".to_string(), serde_json::json!(app_name));
        ev
    }

    /// Observed evidence: active window title.
    pub fn observed_window(item_id: impl Into<String>, window_title: &str, timestamp: Option<&str>) -> Self {
        let t_id = item_id.into();
        let expl = EvidenceExplanation::new(
            "Active Window Title",
            format!("Captured from window titled '{}'.", window_title),
        );

        let mut ev = Self::new(
            EvidenceCategory::SourceWindow,
            EvidenceProvenance::Observed,
            1.0,
            t_id,
            expl,
        );
        if let Some(ts) = timestamp {
            ev.observed_at = ts.to_string();
        }
        ev.attributes.insert("window_title".to_string(), serde_json::json!(window_title));
        ev
    }

    /// Observed evidence: browser domain.
    pub fn observed_domain(item_id: impl Into<String>, domain: &str, url_origin: Option<&str>, timestamp: Option<&str>) -> Self {
        let t_id = item_id.into();
        let expl = EvidenceExplanation::new(
            format!("Web Domain: {}", domain),
            format!("Captured from website domain '{}'.", domain),
        )
        .with_badge(format!("Domain: {}", domain));

        let mut ev = Self::new(
            EvidenceCategory::BrowserDomain,
            EvidenceProvenance::Observed,
            1.0,
            t_id,
            expl,
        );
        if let Some(ts) = timestamp {
            ev.observed_at = ts.to_string();
        }
        ev.attributes.insert("domain".to_string(), serde_json::json!(domain));
        if let Some(url) = url_origin {
            ev.attributes.insert("url_origin".to_string(), serde_json::json!(url));
        }
        ev
    }

    /// Observed evidence: URL metadata.
    pub fn observed_url(item_id: impl Into<String>, url: &str, timestamp: Option<&str>) -> Self {
        let t_id = item_id.into();
        let expl = EvidenceExplanation::new("Captured URL", format!("Original web URL: {}", url));
        let mut ev = Self::new(
            EvidenceCategory::BrowserUrl,
            EvidenceProvenance::Observed,
            1.0,
            t_id,
            expl,
        );
        if let Some(ts) = timestamp {
            ev.observed_at = ts.to_string();
        }
        ev.attributes.insert("url".to_string(), serde_json::json!(url));
        ev
    }

    /// Observed evidence: capture method.
    pub fn observed_capture_method(item_id: impl Into<String>, method: &str, timestamp: Option<&str>) -> Self {
        let t_id = item_id.into();
        let expl = EvidenceExplanation::new(
            format!("Capture Method: {}", method),
            format!("Item captured via {}.", method),
        );
        let mut ev = Self::new(
            EvidenceCategory::CaptureMethod,
            EvidenceProvenance::Observed,
            1.0,
            t_id,
            expl,
        );
        if let Some(ts) = timestamp {
            ev.observed_at = ts.to_string();
        }
        ev.attributes.insert("capture_method".to_string(), serde_json::json!(method));
        ev
    }

    /// Observed evidence: same session overlap.
    pub fn observed_session(item_id: impl Into<String>, session_id: &str, timestamp: Option<&str>) -> Self {
        let t_id = item_id.into();
        let expl = EvidenceExplanation::new(
            "Context Session",
            "Captured during an active user working session.",
        )
        .with_badge("Same Session");

        let mut ev = Self::new(
            EvidenceCategory::SameSession,
            EvidenceProvenance::Observed,
            1.0,
            t_id,
            expl,
        )
        .with_source_ref(EntityReference::session(session_id));

        if let Some(ts) = timestamp {
            ev.observed_at = ts.to_string();
        }
        ev.attributes.insert("session_id".to_string(), serde_json::json!(session_id));
        ev
    }

    /// Observed evidence: file metadata.
    pub fn observed_file_metadata(
        item_id: impl Into<String>,
        filename: &str,
        mime_type: Option<&str>,
        size_bytes: Option<usize>,
        timestamp: Option<&str>,
    ) -> Self {
        let t_id = item_id.into();
        let expl = EvidenceExplanation::new(
            format!("File: {}", filename),
            format!("Captured file '{}'.", filename),
        )
        .with_badge(format!("File: {}", filename));

        let mut ev = Self::new(
            EvidenceCategory::FileMetadata,
            EvidenceProvenance::Observed,
            1.0,
            t_id,
            expl,
        );
        if let Some(ts) = timestamp {
            ev.observed_at = ts.to_string();
        }
        ev.attributes.insert("filename".to_string(), serde_json::json!(filename));
        if let Some(m) = mime_type {
            ev.attributes.insert("mime_type".to_string(), serde_json::json!(m));
        }
        if let Some(s) = size_bytes {
            ev.attributes.insert("size_bytes".to_string(), serde_json::json!(s));
        }
        ev
    }

    /// Observed evidence: content type.
    pub fn observed_content_type(item_id: impl Into<String>, content_type: &str, timestamp: Option<&str>) -> Self {
        let t_id = item_id.into();
        let expl = EvidenceExplanation::new(
            format!("Content Type: {}", content_type),
            format!("Payload format is {}.", content_type),
        );
        let mut ev = Self::new(
            EvidenceCategory::ContentType,
            EvidenceProvenance::Observed,
            1.0,
            t_id,
            expl,
        );
        if let Some(ts) = timestamp {
            ev.observed_at = ts.to_string();
        }
        ev.attributes.insert("content_type".to_string(), serde_json::json!(content_type));
        ev
    }

    /// Observed evidence: explicit user action.
    pub fn observed_user_action(
        item_id: impl Into<String>,
        action: &str,
        workspace_id: Option<&str>,
        workspace_name: Option<&str>,
        timestamp: Option<&str>,
    ) -> Self {
        let t_id = item_id.into();
        let desc = if let Some(ws_name) = workspace_name {
            format!("User action '{}' associated with workspace '{}'.", action, ws_name)
        } else {
            format!("User action '{}'.", action)
        };

        let expl = EvidenceExplanation::new("User Organization History", desc);
        let mut ev = Self::new(
            EvidenceCategory::WorkspaceHistory,
            EvidenceProvenance::Observed,
            1.0,
            t_id,
            expl,
        );
        if let Some(ws_id) = workspace_id {
            ev = ev.with_source_ref(EntityReference::workspace(ws_id, workspace_name.map(|s| s.to_string())));
        }
        if let Some(ts) = timestamp {
            ev.observed_at = ts.to_string();
        }
        ev.attributes.insert("action".to_string(), serde_json::json!(action));
        ev
    }

    /// Observed evidence: confirmed membership.
    pub fn observed_confirmed_membership(
        item_id: impl Into<String>,
        workspace_id: &str,
        workspace_name: Option<&str>,
        timestamp: Option<&str>,
    ) -> Self {
        let t_id = item_id.into();
        let name_display = workspace_name.unwrap_or("workspace");
        let expl = EvidenceExplanation::new(
            "Confirmed Membership",
            format!("Item explicitly confirmed in workspace '{}'.", name_display),
        );

        let mut ev = Self::new(
            EvidenceCategory::ConfirmedMembership,
            EvidenceProvenance::Observed,
            1.0,
            t_id,
            expl,
        )
        .with_source_ref(EntityReference::workspace(workspace_id, workspace_name.map(|s| s.to_string())));

        if let Some(ts) = timestamp {
            ev.observed_at = ts.to_string();
        }
        ev.attributes.insert("workspace_id".to_string(), serde_json::json!(workspace_id));
        ev
    }

    /// Observed evidence: user feedback (e.g. rejection or dismissal).
    pub fn observed_user_feedback(
        item_id: impl Into<String>,
        feedback_type: &str,
        target_ref: Option<EntityReference>,
        timestamp: Option<&str>,
    ) -> Self {
        let t_id = item_id.into();
        let expl = EvidenceExplanation::new(
            "User Feedback",
            format!("Recorded user feedback '{}'.", feedback_type),
        );

        let mut ev = Self::new(
            EvidenceCategory::UserFeedback,
            EvidenceProvenance::Observed,
            1.0,
            t_id,
            expl,
        );
        if let Some(ref_obj) = target_ref {
            ev = ev.with_source_ref(ref_obj);
        }
        if let Some(ts) = timestamp {
            ev.observed_at = ts.to_string();
        }
        ev.attributes.insert("feedback_type".to_string(), serde_json::json!(feedback_type));
        ev
    }

    // =========================================================================
    // Standard Inferred Evidence Factory Methods
    // =========================================================================

    /// Inferred evidence: semantic similarity via embedding vector comparison.
    pub fn inferred_semantic_similarity(
        item_id: impl Into<String>,
        other_item_id: &str,
        similarity_score: f64,
        other_title: Option<&str>,
        workspace_name: Option<&str>,
        timestamp: Option<&str>,
    ) -> Self {
        let t_id = item_id.into();
        let title_display = other_title.unwrap_or("another item");
        let ws_display = workspace_name.unwrap_or("Inbox");

        let band = if similarity_score >= 0.80 {
            "Very High"
        } else if similarity_score >= 0.60 {
            "High"
        } else if similarity_score >= 0.40 {
            "Moderate"
        } else {
            "Weak"
        };

        let expl = EvidenceExplanation::new(
            format!("{} Semantic Similarity", band),
            format!(
                "Semantically similar to '{}' in {} (strong conceptual alignment).",
                title_display, ws_display
            ),
        )
        .with_badge(format!("Semantic: {}", band));

        let mut ev = Self::new(
            EvidenceCategory::SemanticSimilarity,
            EvidenceProvenance::Inferred,
            similarity_score,
            t_id,
            expl,
        )
        .with_source_ref(EntityReference::item(other_item_id).with_label(title_display));

        if let Some(ts) = timestamp {
            ev.observed_at = ts.to_string();
        }
        ev.attributes.insert("other_item_id".to_string(), serde_json::json!(other_item_id));
        if let Some(ws) = workspace_name {
            ev.attributes.insert("workspace_name".to_string(), serde_json::json!(ws));
        }
        ev
    }

    /// Inferred evidence: direct graph relationship.
    pub fn inferred_related_item(
        item_id: impl Into<String>,
        other_item_id: &str,
        relationship_type: &str,
        strength: f64,
        other_title: Option<&str>,
        timestamp: Option<&str>,
    ) -> Self {
        let t_id = item_id.into();
        let formatted_type = relationship_type.replace('_', " ");
        let title_display = other_title.unwrap_or("related item");

        let expl = EvidenceExplanation::new(
            format!("Relationship: {}", formatted_type),
            format!("Connected to '{}' via {}.", title_display, formatted_type),
        );

        let mut ev = Self::new(
            EvidenceCategory::RelatedItem,
            EvidenceProvenance::Inferred,
            strength,
            t_id,
            expl,
        )
        .with_source_ref(EntityReference::item(other_item_id).with_label(title_display));

        if let Some(ts) = timestamp {
            ev.observed_at = ts.to_string();
        }
        ev.attributes.insert("relationship_type".to_string(), serde_json::json!(relationship_type));
        ev.attributes.insert("other_item_id".to_string(), serde_json::json!(other_item_id));
        ev
    }

    /// Inferred evidence: temporal proximity.
    pub fn inferred_temporal_proximity(
        item_id: impl Into<String>,
        other_item_id: &str,
        delta_secs: i64,
        strength: f64,
        timestamp: Option<&str>,
    ) -> Self {
        let t_id = item_id.into();
        let minutes = (delta_secs.abs() / 60).max(1);
        let expl = EvidenceExplanation::new(
            "Temporal Proximity",
            format!("Captured within {} minute{} of related activity.", minutes, if minutes == 1 { "" } else { "s" }),
        );

        let mut ev = Self::new(
            EvidenceCategory::TemporalProximity,
            EvidenceProvenance::Inferred,
            strength,
            t_id,
            expl,
        )
        .with_source_ref(EntityReference::item(other_item_id));

        if let Some(ts) = timestamp {
            ev.observed_at = ts.to_string();
        }
        ev.attributes.insert("delta_secs".to_string(), serde_json::json!(delta_secs));
        ev
    }

    /// Inferred evidence: workspace history affinity with temporal decay.
    pub fn inferred_workspace_history(
        item_id: impl Into<String>,
        workspace_id: &str,
        workspace_name: &str,
        decay_weight: f64,
        timestamp: Option<&str>,
    ) -> Self {
        let t_id = item_id.into();
        let expl = EvidenceExplanation::new(
            format!("Historical Pattern: {}", workspace_name),
            format!("Consistent with your historical organization pattern for {}.", workspace_name),
        );

        let mut ev = Self::new(
            EvidenceCategory::WorkspaceHistory,
            EvidenceProvenance::Inferred,
            decay_weight,
            t_id,
            expl,
        )
        .with_source_ref(EntityReference::workspace(workspace_id, Some(workspace_name.to_string())));

        if let Some(ts) = timestamp {
            ev.observed_at = ts.to_string();
        }
        ev.attributes.insert("workspace_id".to_string(), serde_json::json!(workspace_id));
        ev
    }
}

/// A container managing a collection of `ContextEvidence` records for a single item or cluster.
/// Provides deterministic filtering, provenance aggregation, and evidence metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ContextEvidenceSet {
    pub target_item_id: String,
    pub items: Vec<ContextEvidence>,
}

impl ContextEvidenceSet {
    pub fn new(target_item_id: impl Into<String>) -> Self {
        Self {
            target_item_id: target_item_id.into(),
            items: Vec::new(),
        }
    }

    pub fn add(&mut self, evidence: ContextEvidence) {
        self.items.push(evidence);
    }

    pub fn extend(&mut self, evidences: impl IntoIterator<Item = ContextEvidence>) {
        self.items.extend(evidences);
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Returns all observed empirical facts.
    pub fn observed(&self) -> Vec<&ContextEvidence> {
        self.items.iter().filter(|e| e.provenance.is_observed()).collect()
    }

    /// Returns all inferred / computed relationships.
    pub fn inferred(&self) -> Vec<&ContextEvidence> {
        self.items.iter().filter(|e| e.provenance.is_inferred()).collect()
    }

    /// Filters evidence by specific category.
    pub fn by_category(&self, category: EvidenceCategory) -> Vec<&ContextEvidence> {
        self.items.iter().filter(|e| e.category == category).collect()
    }

    /// Filters evidence by provenance.
    pub fn by_provenance(&self, provenance: EvidenceProvenance) -> Vec<&ContextEvidence> {
        self.items.iter().filter(|e| e.provenance == provenance).collect()
    }

    /// Computes sum of all evidence weights.
    pub fn total_weight(&self) -> f64 {
        self.items.iter().map(|e| e.strength).sum()
    }

    /// Computes average evidence strength.
    pub fn average_strength(&self) -> f64 {
        if self.items.is_empty() {
            0.0
        } else {
            self.total_weight() / self.items.len() as f64
        }
    }

    /// Returns maximum individual evidence strength.
    pub fn max_strength(&self) -> f64 {
        self.items.iter().map(|e| e.strength).fold(0.0, f64::max)
    }

    /// Counts distinct evidence categories present with strength >= threshold.
    pub fn independent_signal_count(&self, min_strength_threshold: f64) -> usize {
        let mut categories = HashSet::new();
        for e in &self.items {
            if e.strength >= min_strength_threshold {
                categories.insert(e.category);
            }
        }
        categories.len()
    }

    /// Filters evidence records meeting or exceeding a minimum strength floor.
    pub fn filter_by_min_strength(&self, min_strength: f64) -> Vec<&ContextEvidence> {
        self.items.iter().filter(|e| e.strength >= min_strength).collect()
    }

    /// Returns deduplicated natural-language explanation summaries sorted by weight descending.
    pub fn summary_explanations(&self) -> Vec<String> {
        let mut sorted_items = self.items.clone();
        sorted_items.sort_by(|a, b| b.strength.partial_cmp(&a.strength).unwrap_or(std::cmp::Ordering::Equal));

        let mut seen = HashSet::new();
        let mut summaries = Vec::new();
        for item in sorted_items {
            if seen.insert(item.explanation.description.clone()) {
                summaries.push(item.explanation.description);
            }
        }
        summaries
    }
}

// =========================================================================
// Context Evidence Collector Service
// =========================================================================

/// Configuration options for contextual evidence collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceCollectorOptions {
    /// Minimum relationship strength required to include an ItemRelationship (default: 0.30)
    pub min_relationship_strength: f64,
    /// Minimum semantic cosine similarity to include item-item semantic evidence (default: 0.40)
    pub min_semantic_similarity: f64,
    /// Minimum semantic cosine similarity to include workspace profile evidence (default: 0.30)
    pub min_workspace_profile_similarity: f64,
    /// Whether to evaluate workspace semantic profiles (default: true)
    pub include_workspace_profiles: bool,
    /// Whether to evaluate workspace history signals with decay (default: true)
    pub include_workspace_history: bool,
    /// Whether to evaluate user feedback (dismissals, confirms) (default: true)
    pub include_user_feedback: bool,
    /// Optional custom temporal decay configuration (default: standard exponential)
    pub temporal_decay: Option<crate::context_engine::temporal::TemporalDecayConfig>,
    /// Limit on number of relationships processed (default: 50)
    pub max_related_items: usize,
}

impl Default for EvidenceCollectorOptions {
    fn default() -> Self {
        Self {
            min_relationship_strength: 0.30,
            min_semantic_similarity: 0.40,
            min_workspace_profile_similarity: 0.30,
            include_workspace_profiles: true,
            include_workspace_history: true,
            include_user_feedback: true,
            temporal_decay: None,
            max_related_items: 50,
        }
    }
}

/// Collector service responsible for gathering normalized contextual evidence for an Item.
///
/// CRITICAL ARCHITECTURAL CONSTRAINTS:
/// 1. READ-ONLY: Never writes or updates any database tables, items, or memberships.
/// 2. NO SCORING: Does not compute final workspace scores or context decisions.
/// 3. NO LLM: Purely deterministic local query and normalization.
/// 4. GRACEFUL DEGRADATION: Missing signals, embeddings, or relationships do not fail the query.
#[derive(Debug, Clone, Default)]
pub struct ContextEvidenceCollector {
    pub options: EvidenceCollectorOptions,
}

impl ContextEvidenceCollector {
    pub fn new() -> Self {
        Self {
            options: EvidenceCollectorOptions::default(),
        }
    }

    pub fn with_options(options: EvidenceCollectorOptions) -> Self {
        Self { options }
    }

    /// Gathers all available contextual evidence for the specified item_id.
    pub fn collect(&self, conn: &rusqlite::Connection, item_id: &str) -> Result<ContextEvidenceSet, String> {
        let item = crate::db::operations::get_item(conn, item_id)
            .map_err(|e| format!("Failed to retrieve item '{}': {}", item_id, e))?;

        let mut raw_evidences: Vec<ContextEvidence> = Vec::new();

        // ---------------------------------------------------------------------
        // 1. Item-Level Metadata & Content Classification (Observed)
        // ---------------------------------------------------------------------
        if !item.item_type.is_empty() {
            raw_evidences.push(ContextEvidence::observed_content_type(
                item_id,
                &item.item_type,
                Some(&item.created_at),
            ));
        }

        if let Ok(meta_val) = serde_json::from_str::<serde_json::Value>(&item.metadata) {
            // URL metadata in item payload
            if let Some(url) = meta_val.get("url").or_else(|| meta_val.get("source_url")).and_then(|v| v.as_str()) {
                if !url.is_empty() {
                    raw_evidences.push(ContextEvidence::observed_url(item_id, url, Some(&item.created_at)));
                    if let Some(domain) = meta_val.get("domain").and_then(|v| v.as_str()) {
                        raw_evidences.push(ContextEvidence::observed_domain(item_id, domain, Some(url), Some(&item.created_at)));
                    } else if let Ok(parsed_url) = url::Url::parse(url) {
                        if let Some(host) = parsed_url.host_str() {
                            raw_evidences.push(ContextEvidence::observed_domain(item_id, host, Some(url), Some(&item.created_at)));
                        }
                    }
                }
            }

            // File metadata in item payload
            if let Some(filename) = meta_val.get("filename").or_else(|| meta_val.get("file_name")).and_then(|v| v.as_str()) {
                let mime = meta_val.get("mime_type").or_else(|| meta_val.get("file_type")).and_then(|v| v.as_str());
                let size = meta_val.get("file_size").or_else(|| meta_val.get("size")).and_then(|v| v.as_u64()).map(|s| s as usize);
                raw_evidences.push(ContextEvidence::observed_file_metadata(
                    item_id,
                    filename,
                    mime,
                    size,
                    Some(&item.created_at),
                ));
            }

            // Capture method in item payload
            if let Some(method) = meta_val.get("capture_method").or_else(|| meta_val.get("method")).or_else(|| meta_val.get("source")).and_then(|v| v.as_str()) {
                raw_evidences.push(ContextEvidence::observed_capture_method(
                    item_id,
                    method,
                    Some(&item.created_at),
                ));
            }

            // App Name in item payload
            if let Some(app) = meta_val.get("app_name").or_else(|| meta_val.get("source_app")).and_then(|v| v.as_str()) {
                raw_evidences.push(ContextEvidence::observed_app(item_id, app, Some(&item.created_at)));
            }

            // Window Title in item payload
            if let Some(win) = meta_val.get("window_title").and_then(|v| v.as_str()) {
                raw_evidences.push(ContextEvidence::observed_window(item_id, win, Some(&item.created_at)));
            }
        }

        // ---------------------------------------------------------------------
        // 2. ContextSignals Store (Observed facts and user actions)
        // ---------------------------------------------------------------------
        let signals = crate::context_engine::operations::list_context_signals(conn, None, Some(item_id))
            .unwrap_or_default();

        let now = Utc::now();
        let db_decay = crate::context_engine::temporal::get_temporal_decay_config(conn);
        let decay_config = self.options.temporal_decay.as_ref().unwrap_or(&db_decay);

        for sig in &signals {
            let sig_ref = EntityReference::signal(&sig.id);
            if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&sig.payload) {
                match sig.signal_type.as_str() {
                    "active_app" => {
                        if let Some(app) = payload.get("app_name").or_else(|| payload.get("bundle_id")).or_else(|| payload.get("process_name")).and_then(|v| v.as_str()) {
                            let mut ev = ContextEvidence::observed_app(item_id, app, Some(&sig.created_at))
                                .with_source_ref(sig_ref.clone());
                            if let Some(proc) = payload.get("process_name").and_then(|v| v.as_str()) {
                                ev = ev.with_attribute("process_name", proc);
                            }
                            raw_evidences.push(ev);
                        }
                        if let Some(win) = payload.get("window_title").and_then(|v| v.as_str()) {
                            if !win.trim().is_empty() {
                                raw_evidences.push(
                                    ContextEvidence::observed_window(item_id, win, Some(&sig.created_at))
                                        .with_source_ref(sig_ref.clone()),
                                );
                            }
                        }
                    }
                    "window_title" => {
                        if let Some(win) = payload.get("window_title").or_else(|| payload.get("title")).and_then(|v| v.as_str()) {
                            raw_evidences.push(
                                ContextEvidence::observed_window(item_id, win, Some(&sig.created_at))
                                    .with_source_ref(sig_ref.clone()),
                            );
                        }
                    }
                    "url_metadata" => {
                        let url_opt = payload.get("url").and_then(|v| v.as_str());
                        if let Some(url) = url_opt {
                            raw_evidences.push(
                                ContextEvidence::observed_url(item_id, url, Some(&sig.created_at))
                                    .with_source_ref(sig_ref.clone()),
                            );
                        }
                        if let Some(domain) = payload.get("domain").and_then(|v| v.as_str()) {
                            raw_evidences.push(
                                ContextEvidence::observed_domain(item_id, domain, url_opt, Some(&sig.created_at))
                                    .with_source_ref(sig_ref.clone()),
                            );
                        } else if let Some(url) = url_opt {
                            if let Ok(parsed_url) = url::Url::parse(url) {
                                if let Some(host) = parsed_url.host_str() {
                                    raw_evidences.push(
                                        ContextEvidence::observed_domain(item_id, host, Some(url), Some(&sig.created_at))
                                            .with_source_ref(sig_ref.clone()),
                                    );
                                }
                            }
                        }
                    }
                    "file_metadata" => {
                        if let Some(filename) = payload.get("filename").or_else(|| payload.get("file_name")).or_else(|| payload.get("name")).and_then(|v| v.as_str()) {
                            let mime = payload.get("mime_type").and_then(|v| v.as_str());
                            let size = payload.get("size_bytes").or_else(|| payload.get("size")).and_then(|v| v.as_u64()).map(|s| s as usize);
                            raw_evidences.push(
                                ContextEvidence::observed_file_metadata(
                                    item_id,
                                    filename,
                                    mime,
                                    size,
                                    Some(&sig.created_at),
                                )
                                .with_source_ref(sig_ref.clone()),
                            );
                        }
                    }
                    "capture_event" => {
                        if let Some(method) = payload.get("capture_method").or_else(|| payload.get("method")).or_else(|| payload.get("source")).and_then(|v| v.as_str()) {
                            raw_evidences.push(
                                ContextEvidence::observed_capture_method(item_id, method, Some(&sig.created_at))
                                    .with_source_ref(sig_ref.clone()),
                            );
                        }
                    }
                    "user_action" => {
                        if self.options.include_workspace_history {
                            let action = payload.get("action").and_then(|v| v.as_str()).unwrap_or("user_action");
                            let target_ws = payload.get("destination_workspace")
                                .or_else(|| payload.get("workspace_id"))
                                .or_else(|| payload.get("suggested_workspace"))
                                .and_then(|v| v.as_str())
                                .or(sig.workspace_id.as_deref());

                            if let Some(ws_id) = target_ws {
                                let ws_name = crate::db::operations::get_workspace(conn, ws_id)
                                    .map(|w| w.name)
                                    .unwrap_or_else(|_| "Workspace".to_string());
                                let (decay_weight, _expl) = decay_config.compute_weight_and_explanation(&sig.created_at, now);
                                let mut ev = ContextEvidence::inferred_workspace_history(
                                    item_id,
                                    ws_id,
                                    &ws_name,
                                    decay_weight,
                                    Some(&sig.created_at),
                                );
                                ev = ev.with_attribute("action", action);
                                raw_evidences.push(ev);
                            }
                        }
                    }
                    "membership_dismissed" | "suggestion_dismissed" => {
                        if self.options.include_user_feedback {
                            let ws_id = payload.get("workspace_id").and_then(|v| v.as_str()).or(sig.workspace_id.as_deref());
                            let ws_ref = ws_id.map(|w| {
                                let name = crate::db::operations::get_workspace(conn, w).ok().map(|ws| ws.name);
                                EntityReference::workspace(w, name)
                            });
                            raw_evidences.push(ContextEvidence::observed_user_feedback(
                                item_id,
                                &sig.signal_type,
                                ws_ref,
                                Some(&sig.created_at),
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }

        // ---------------------------------------------------------------------
        // 3. Activity Sessions (Observed SameSession & Co-Session Count)
        // ---------------------------------------------------------------------
        if let Some(ref session_id) = item.session_id {
            if !session_id.is_empty() {
                let co_session_count: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM items WHERE session_id = ?1 AND id != ?2",
                    rusqlite::params![session_id, item_id],
                    |row| row.get(0),
                ).unwrap_or(0);

                let mut ev = ContextEvidence::observed_session(item_id, session_id, Some(&item.created_at));
                ev = ev.with_attribute("co_session_item_count", co_session_count);
                raw_evidences.push(ev);
            }
        }

        // ---------------------------------------------------------------------
        // 4. Graph Relationships (Inferred RelatedItem, Temporal, Semantic)
        // ---------------------------------------------------------------------
        let relationships = crate::db::operations::get_item_relationships(conn, item_id).unwrap_or_default();
        for rel in relationships.iter().take(self.options.max_related_items) {
            if rel.strength < self.options.min_relationship_strength {
                continue;
            }

            let other_id = if rel.item_a_id == item_id {
                &rel.item_b_id
            } else {
                &rel.item_a_id
            };

            let other_item = crate::db::operations::get_item(conn, other_id).ok();
            let other_title = other_item.as_ref().and_then(|i| i.title.as_deref());
            let ws_name = other_item
                .as_ref()
                .and_then(|i| i.workspace_id.as_deref())
                .and_then(|ws_id| crate::db::operations::get_workspace(conn, ws_id).ok().map(|w| w.name));

            let decay_weight = if let Some(ref other) = other_item {
                decay_config.compute_weight(&other.created_at, now)
            } else {
                1.0
            };
            let decayed_strength = rel.strength * decay_weight;

            match rel.relationship_type.as_str() {
                "temporal_proximity" => {
                    let mut delta_secs: i64 = 60;
                    if let Some(ref meta_str) = rel.metadata {
                        if let Ok(meta_val) = serde_json::from_str::<serde_json::Value>(meta_str) {
                            if let Some(secs) = meta_val.get("delta_secs").and_then(|v| v.as_i64()) {
                                delta_secs = secs;
                            }
                        }
                    } else if let Some(ref other) = other_item {
                        if let (Ok(dt1), Ok(dt2)) = (
                            DateTime::parse_from_rfc3339(&item.created_at),
                            DateTime::parse_from_rfc3339(&other.created_at),
                        ) {
                            delta_secs = dt1.signed_duration_since(dt2).num_seconds().abs();
                        }
                    }
                    raw_evidences.push(ContextEvidence::inferred_temporal_proximity(
                        item_id,
                        other_id,
                        delta_secs,
                        decayed_strength,
                        Some(&rel.created_at),
                    ));
                }
                "semantic_similarity" => {
                    if decayed_strength >= self.options.min_semantic_similarity {
                        raw_evidences.push(ContextEvidence::inferred_semantic_similarity(
                            item_id,
                            other_id,
                            decayed_strength,
                            other_title,
                            ws_name.as_deref(),
                            Some(&rel.created_at),
                        ));
                    }
                }
                _ => {
                    raw_evidences.push(ContextEvidence::inferred_related_item(
                        item_id,
                        other_id,
                        &rel.relationship_type,
                        decayed_strength,
                        other_title,
                        Some(&rel.created_at),
                    ));
                }
            }
        }

        // ---------------------------------------------------------------------
        // 5. Context Memberships (Observed Confirmed or Feedback)
        // ---------------------------------------------------------------------
        let memberships = crate::db::operations::list_item_memberships(conn, item_id).unwrap_or_default();
        for m in &memberships {
            let ws_name = crate::db::operations::get_workspace(conn, &m.workspace_id)
                .ok()
                .map(|w| w.name);
            if m.membership_type == "confirmed" {
                raw_evidences.push(ContextEvidence::observed_confirmed_membership(
                    item_id,
                    &m.workspace_id,
                    ws_name.as_deref(),
                    Some(&m.created_at),
                ));
            } else if m.membership_type == "dismissed" && self.options.include_user_feedback {
                raw_evidences.push(ContextEvidence::observed_user_feedback(
                    item_id,
                    "membership_dismissed",
                    Some(EntityReference::workspace(&m.workspace_id, ws_name)),
                    Some(&m.created_at),
                ));
            }
        }

        // ---------------------------------------------------------------------
        // 6. Workspace Semantic Profiles (Inferred Cosine Comparison)
        // ---------------------------------------------------------------------
        if self.options.include_workspace_profiles {
            if let Ok(Some(item_emb)) = crate::db::operations::get_item_embedding(conn, item_id) {
                if let Ok(workspaces) = crate::db::operations::list_workspaces(conn) {
                    for ws in workspaces {
                        if let Ok(Some(ws_emb)) = crate::db::operations::get_workspace_embedding(conn, &ws.id) {
                            if item_emb.provider == ws_emb.provider && item_emb.model == ws_emb.model {
                                let sim = crate::semantic::cosine_similarity(&item_emb.embedding, &ws_emb.embedding) as f64;
                                if sim >= self.options.min_workspace_profile_similarity {
                                    let band = if sim >= 0.80 {
                                        "Very High"
                                    } else if sim >= 0.60 {
                                        "High"
                                    } else {
                                        "Moderate"
                                    };
                                    let expl = EvidenceExplanation::new(
                                        format!("{} Profile Alignment", band),
                                        format!("Semantically aligned with the '{}' workspace profile.", ws.name),
                                    )
                                    .with_badge(format!("Profile: {}", band));

                                    let mut ev = ContextEvidence::new(
                                        EvidenceCategory::SemanticSimilarity,
                                        EvidenceProvenance::Inferred,
                                        sim,
                                        item_id,
                                        expl,
                                    )
                                    .with_source_ref(EntityReference::workspace(&ws.id, Some(ws.name.clone())));
                                    ev = ev.with_attribute("workspace_id", ws.id.clone());
                                    ev = ev.with_attribute("workspace_name", ws.name.clone());
                                    ev = ev.with_attribute("profile_similarity", sim);
                                    raw_evidences.push(ev);
                                }
                            }
                        }
                    }
                }
            }
        }

        // ---------------------------------------------------------------------
        // 7. Deduplication & Normalization
        // ---------------------------------------------------------------------
        let mut set = ContextEvidenceSet::new(item_id);
        let mut seen_keys = HashSet::new();

        for ev in raw_evidences {
            let key = Self::deduplication_key(&ev);
            if seen_keys.insert(key) {
                set.add(ev);
            }
        }

        Ok(set)
    }

    fn deduplication_key(ev: &ContextEvidence) -> String {
        let source_key = ev.source_ref.as_ref().map(|r| format!("{}:{}", r.entity_type.as_str(), r.entity_id)).unwrap_or_default();
        let attr_key = match ev.category {
            EvidenceCategory::SourceApp => ev.attributes.get("app_name").and_then(|v| v.as_str()).unwrap_or(""),
            EvidenceCategory::SourceWindow => ev.attributes.get("window_title").and_then(|v| v.as_str()).unwrap_or(""),
            EvidenceCategory::BrowserDomain => ev.attributes.get("domain").and_then(|v| v.as_str()).unwrap_or(""),
            EvidenceCategory::BrowserUrl => ev.attributes.get("url").and_then(|v| v.as_str()).unwrap_or(""),
            EvidenceCategory::ContentType => ev.attributes.get("content_type").and_then(|v| v.as_str()).unwrap_or(""),
            EvidenceCategory::CaptureMethod => ev.attributes.get("capture_method").and_then(|v| v.as_str()).unwrap_or(""),
            EvidenceCategory::FileMetadata => ev.attributes.get("filename").and_then(|v| v.as_str()).unwrap_or(""),
            EvidenceCategory::WorkspaceHistory => ev.attributes.get("workspace_id").and_then(|v| v.as_str()).unwrap_or(""),
            _ => "",
        };

        format!("{}:{}:{}:{}:{}", ev.category.as_str(), ev.provenance.as_str(), ev.target_item_id, source_key, attr_key)
    }
}

/// Convenience helper to collect evidence for an item using default collector options.
pub fn collect_item_evidence(conn: &rusqlite::Connection, item_id: &str) -> Result<ContextEvidenceSet, String> {
    let collector = ContextEvidenceCollector::new();
    collector.collect(conn, item_id)
}

// =========================================================================
// Unit Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_evidence_strength_clamping() {
        let expl = EvidenceExplanation::new("Test", "Test description");
        let ev_over = ContextEvidence::new(
            EvidenceCategory::SourceApp,
            EvidenceProvenance::Observed,
            1.45,
            "item_123",
            expl.clone(),
        );
        assert_eq!(ev_over.strength, 1.0, "Strength above 1.0 must be clamped to 1.0");

        let ev_under = ContextEvidence::new(
            EvidenceCategory::SourceApp,
            EvidenceProvenance::Observed,
            -0.25,
            "item_123",
            expl,
        );
        assert_eq!(ev_under.strength, 0.0, "Strength below 0.0 must be clamped to 0.0");
    }

    #[test]
    fn test_observed_and_inferred_provenance_distinction() {
        let observed_app = ContextEvidence::observed_app("item_1", "VS Code", None);
        assert!(observed_app.provenance.is_observed());
        assert!(!observed_app.provenance.is_inferred());
        assert_eq!(observed_app.category, EvidenceCategory::SourceApp);

        let inferred_sim = ContextEvidence::inferred_semantic_similarity(
            "item_1",
            "item_2",
            0.88,
            Some("Architecture Docs"),
            Some("ContextSplit"),
            None,
        );
        assert!(inferred_sim.provenance.is_inferred());
        assert!(!inferred_sim.provenance.is_observed());
        assert_eq!(inferred_sim.category, EvidenceCategory::SemanticSimilarity);
        assert_eq!(inferred_sim.strength, 0.88);
    }

    #[test]
    fn test_all_evidence_categories_serialization_roundtrip() {
        let categories = vec![
            EvidenceCategory::SourceApp,
            EvidenceCategory::SourceWindow,
            EvidenceCategory::BrowserDomain,
            EvidenceCategory::BrowserUrl,
            EvidenceCategory::CaptureMethod,
            EvidenceCategory::TemporalProximity,
            EvidenceCategory::SameSession,
            EvidenceCategory::RelatedItem,
            EvidenceCategory::SemanticSimilarity,
            EvidenceCategory::WorkspaceHistory,
            EvidenceCategory::ConfirmedMembership,
            EvidenceCategory::UserFeedback,
            EvidenceCategory::FileMetadata,
            EvidenceCategory::ContentType,
        ];

        for cat in categories {
            let str_val = cat.as_str();
            let parsed = EvidenceCategory::from_str(str_val).expect("Should parse back");
            assert_eq!(cat, parsed);

            let json = serde_json::to_string(&cat).unwrap();
            let deserialized: EvidenceCategory = serde_json::from_str(&json).unwrap();
            assert_eq!(cat, deserialized);
        }
    }

    #[test]
    fn test_evidence_set_metrics_and_filtering() {
        let mut set = ContextEvidenceSet::new("target_item_1");

        set.add(ContextEvidence::observed_app("target_item_1", "Safari", None));
        set.add(ContextEvidence::observed_domain("target_item_1", "figma.com", None, None));
        set.add(ContextEvidence::inferred_semantic_similarity(
            "target_item_1",
            "other_1",
            0.92,
            Some("Design System"),
            Some("UI Workspace"),
            None,
        ));
        set.add(ContextEvidence::inferred_temporal_proximity(
            "target_item_1",
            "other_2",
            120,
            0.50,
            None,
        ));

        assert_eq!(set.len(), 4);
        assert_eq!(set.observed().len(), 2);
        assert_eq!(set.inferred().len(), 2);
        assert_eq!(set.by_category(EvidenceCategory::SourceApp).len(), 1);
        assert_eq!(set.by_category(EvidenceCategory::BrowserDomain).len(), 1);
        assert_eq!(set.by_category(EvidenceCategory::SemanticSimilarity).len(), 1);

        // Independent signals count with threshold >= 0.40
        assert_eq!(set.independent_signal_count(0.40), 4);
        // Independent signals count with threshold >= 0.90 (App=1.0, Domain=1.0, Semantic=0.92)
        assert_eq!(set.independent_signal_count(0.90), 3);

        // Explanations are deduplicated and formatted
        let summaries = set.summary_explanations();
        assert_eq!(summaries.len(), 4);
    }

    #[test]
    fn test_no_raw_float_vector_leakage_in_explanations() {
        let semantic_ev = ContextEvidence::inferred_semantic_similarity(
            "item_1",
            "item_2",
            0.874392,
            Some("Paris Itinerary"),
            Some("Travel"),
            None,
        );

        let desc = &semantic_ev.explanation.description;
        // Invariant: Explanations must be clean natural language templates without exposing raw floats like 0.874392
        assert!(!desc.contains("0.87"), "Explanation description should not expose raw float: {}", desc);
        assert!(!desc.contains("["), "Explanation description should not contain vector brackets");
    }

    #[test]
    fn test_evidence_model_is_strictly_evidence_only() {
        // Invariant: ContextEvidence holds facts and weights, but has no methods to mutate workspaces,
        // assign candidates, or alter application database state directly.
        let ev = ContextEvidence::observed_app("item_xyz", "Terminal", None);
        assert_eq!(ev.target_item_id, "item_xyz");
        assert_eq!(ev.category, EvidenceCategory::SourceApp);
        assert_eq!(ev.provenance, EvidenceProvenance::Observed);
    }

    #[test]
    fn test_collector_complete_evidence() {
        let conn = setup_test_db();
        let ws = crate::db::operations::create_workspace(&conn, "Trip to Japan").unwrap();
        let session = crate::db::operations::create_session(&conn).unwrap();

        // 1. Create Target Item
        let item_target = crate::db::operations::create_item(
            &conn,
            None,
            Some(&session.id),
            "url",
            "https://www.japan-guide.com/e/e2164.html",
            Some("Tokyo Travel Guide"),
            r#"{"capture_method":"clipboard","source_app":"Google Chrome","domain":"japan-guide.com"}"#,
        ).unwrap();

        // 2. Create Co-session Item in the same workspace
        let item_other = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "Shinjuku hotel reservation notes",
            Some("Hotel Notes"),
            "{}",
        ).unwrap();

        // 3. Create ContextSignals for Target Item
        let _ = crate::context_engine::operations::create_context_signal(
            &conn,
            None,
            Some(&item_target.id),
            "active_app",
            Some(&session.id),
            r#"{"app_name":"Google Chrome","process_name":"chrome","window_title":"Tokyo Travel Guide - Japan Guide"}"#,
        ).unwrap();

        let _ = crate::context_engine::operations::create_context_signal(
            &conn,
            None,
            Some(&item_target.id),
            "url_metadata",
            Some(&session.id),
            r#"{"url":"https://www.japan-guide.com/e/e2164.html","domain":"japan-guide.com"}"#,
        ).unwrap();

        let _ = crate::context_engine::operations::create_context_signal(
            &conn,
            None,
            Some(&item_target.id),
            "user_action",
            None,
            &format!(r#"{{"action":"moved_to_workspace","destination_workspace":"{}"}}"#, ws.id),
        ).unwrap();

        // 4. Create Item Relationships
        let _ = crate::db::operations::create_item_relationship(
            &conn,
            &item_target.id,
            &item_other.id,
            "temporal_proximity",
            0.85,
            Some(r#"{"delta_secs":120}"#),
        ).unwrap();

        let _ = crate::db::operations::create_item_relationship(
            &conn,
            &item_target.id,
            &item_other.id,
            "semantic_similarity",
            0.78,
            None,
        ).unwrap();

        // 5. Create Confirmed Membership
        let _ = crate::db::operations::create_or_update_membership(
            &conn,
            &item_target.id,
            &ws.id,
            "confirmed",
            1.0,
            "user",
        ).unwrap();

        // 6. Create Semantic Embeddings (matching vectors for high cosine similarity)
        let vec = vec![0.5_f32, 0.5, 0.5, 0.5];
        let _ = crate::db::operations::save_item_embedding(
            &conn,
            &item_target.id,
            &vec,
            "local",
            "all-MiniLM-L6-v2",
        ).unwrap();

        // Save workspace profile embedding
        let now_str = Utc::now().to_rfc3339();
        let bytes: Vec<u8> = vec.iter().flat_map(|f| f.to_ne_bytes()).collect();
        conn.execute(
            "INSERT INTO workspace_embeddings (workspace_id, embedding, provider, model, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![ws.id, bytes, "local", "all-MiniLM-L6-v2", now_str],
        ).unwrap();

        // Run Collector
        let evidence_set = collect_item_evidence(&conn, &item_target.id).unwrap();

        assert_eq!(evidence_set.target_item_id, item_target.id);
        assert!(!evidence_set.is_empty(), "Evidence set must not be empty");

        // Verify Observed vs Inferred distinction
        let observed = evidence_set.observed();
        let inferred = evidence_set.inferred();

        assert!(!observed.is_empty(), "Should contain observed evidence");
        assert!(!inferred.is_empty(), "Should contain inferred evidence");

        // Check specific observed categories
        assert!(!evidence_set.by_category(EvidenceCategory::ContentType).is_empty());
        assert!(!evidence_set.by_category(EvidenceCategory::SourceApp).is_empty());
        assert!(!evidence_set.by_category(EvidenceCategory::SourceWindow).is_empty());
        assert!(!evidence_set.by_category(EvidenceCategory::BrowserDomain).is_empty());
        assert!(!evidence_set.by_category(EvidenceCategory::BrowserUrl).is_empty());
        assert!(!evidence_set.by_category(EvidenceCategory::CaptureMethod).is_empty());
        assert!(!evidence_set.by_category(EvidenceCategory::SameSession).is_empty());
        assert!(!evidence_set.by_category(EvidenceCategory::ConfirmedMembership).is_empty());

        // Check specific inferred categories
        assert!(!evidence_set.by_category(EvidenceCategory::TemporalProximity).is_empty());
        assert!(!evidence_set.by_category(EvidenceCategory::SemanticSimilarity).is_empty());
        assert!(!evidence_set.by_category(EvidenceCategory::WorkspaceHistory).is_empty());

        // Verify all strengths are strictly between [0.0, 1.0]
        for ev in &evidence_set.items {
            assert!(ev.strength >= 0.0 && ev.strength <= 1.0, "Strength out of bounds: {}", ev.strength);
        }

        // Verify explanations contain no raw floats or vector dumps
        for expl in evidence_set.summary_explanations() {
            assert!(!expl.contains("0.78"), "Explanation must not expose raw float: {}", expl);
            assert!(!expl.contains("["), "Explanation must not expose raw vector: {}", expl);
        }
    }

    #[test]
    fn test_collector_missing_signals() {
        let conn = setup_test_db();
        let item = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Notes on meeting with frontend team",
            Some("Meeting Notes"),
            "{}",
        ).unwrap();

        // No signals inserted
        let result = collect_item_evidence(&conn, &item.id);
        assert!(result.is_ok(), "Collector must succeed even with no signals");
        let evidence_set = result.unwrap();

        // Should have ContentType observed
        assert_eq!(evidence_set.by_category(EvidenceCategory::ContentType).len(), 1);
        assert_eq!(evidence_set.observed().len(), 1);
        assert_eq!(evidence_set.inferred().len(), 0);
    }

    #[test]
    fn test_collector_missing_semantic_representation() {
        let conn = setup_test_db();
        let ws = crate::db::operations::create_workspace(&conn, "Design System").unwrap();
        let item = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "image",
            "screenshot_dashboard.png",
            Some("Dashboard Mockup"),
            r#"{"filename":"screenshot_dashboard.png","mime_type":"image/png","file_size":2048}"#,
        ).unwrap();

        // No item embedding or workspace embedding saved
        let result = collect_item_evidence(&conn, &item.id);
        assert!(result.is_ok(), "Collector must succeed without semantic embeddings");
        let evidence_set = result.unwrap();

        // Should have ContentType and FileMetadata observed
        assert!(!evidence_set.by_category(EvidenceCategory::ContentType).is_empty());
        assert!(!evidence_set.by_category(EvidenceCategory::FileMetadata).is_empty());
        assert!(evidence_set.by_category(EvidenceCategory::SemanticSimilarity).is_empty());
    }

    #[test]
    fn test_collector_no_session() {
        let conn = setup_test_db();
        let item = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Just a standalone snippet",
            None,
            "{}",
        ).unwrap();

        let evidence_set = collect_item_evidence(&conn, &item.id).unwrap();
        assert!(evidence_set.by_category(EvidenceCategory::SameSession).is_empty());
    }

    #[test]
    fn test_collector_no_related_items() {
        let conn = setup_test_db();
        let item = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://crates.io",
            Some("Rust Crates"),
            "{}",
        ).unwrap();

        let evidence_set = collect_item_evidence(&conn, &item.id).unwrap();
        assert!(evidence_set.by_category(EvidenceCategory::RelatedItem).is_empty());
        assert!(evidence_set.by_category(EvidenceCategory::TemporalProximity).is_empty());
    }

    #[test]
    fn test_collector_new_unassigned_item() {
        let conn = setup_test_db();
        let item = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Unprocessed inbox item",
            None,
            "{}",
        ).unwrap();

        let evidence_set = collect_item_evidence(&conn, &item.id).unwrap();
        assert_eq!(evidence_set.target_item_id, item.id);
        assert_eq!(evidence_set.len(), 1);
        assert_eq!(evidence_set.items[0].category, EvidenceCategory::ContentType);
        assert_eq!(evidence_set.items[0].provenance, EvidenceProvenance::Observed);
    }

    #[test]
    fn test_collector_read_only_invariant() {
        let conn = setup_test_db();
        let ws = crate::db::operations::create_workspace(&conn, "Research").unwrap();
        let session = crate::db::operations::create_session(&conn).unwrap();
        let item = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "Research notes on quantum computing",
            Some("Quantum Notes"),
            r#"{"capture_method":"manual"}"#,
        ).unwrap();

        let _ = crate::context_engine::operations::create_context_signal(
            &conn,
            None,
            Some(&item.id),
            "active_app",
            Some(&session.id),
            r#"{"app_name":"Notes"}"#,
        ).unwrap();

        // Measure counts before collection
        let item_count_before: i64 = conn.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0)).unwrap();
        let ws_count_before: i64 = conn.query_row("SELECT COUNT(*) FROM workspaces", [], |r| r.get(0)).unwrap();
        let signal_count_before: i64 = conn.query_row("SELECT COUNT(*) FROM context_signals", [], |r| r.get(0)).unwrap();
        let rel_count_before: i64 = conn.query_row("SELECT COUNT(*) FROM item_relationships", [], |r| r.get(0)).unwrap();
        let membership_count_before: i64 = conn.query_row("SELECT COUNT(*) FROM context_memberships", [], |r| r.get(0)).unwrap();
        let candidate_count_before: i64 = conn.query_row("SELECT COUNT(*) FROM context_candidates", [], |r| r.get(0)).unwrap();

        // Run collector multiple times
        let _ = collect_item_evidence(&conn, &item.id).unwrap();
        let _ = collect_item_evidence(&conn, &item.id).unwrap();

        // Measure counts after collection
        let item_count_after: i64 = conn.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0)).unwrap();
        let ws_count_after: i64 = conn.query_row("SELECT COUNT(*) FROM workspaces", [], |r| r.get(0)).unwrap();
        let signal_count_after: i64 = conn.query_row("SELECT COUNT(*) FROM context_signals", [], |r| r.get(0)).unwrap();
        let rel_count_after: i64 = conn.query_row("SELECT COUNT(*) FROM item_relationships", [], |r| r.get(0)).unwrap();
        let membership_count_after: i64 = conn.query_row("SELECT COUNT(*) FROM context_memberships", [], |r| r.get(0)).unwrap();
        let candidate_count_after: i64 = conn.query_row("SELECT COUNT(*) FROM context_candidates", [], |r| r.get(0)).unwrap();

        // Assert strictly zero DB changes (Read-Only invariant)
        assert_eq!(item_count_before, item_count_after);
        assert_eq!(ws_count_before, ws_count_after);
        assert_eq!(signal_count_before, signal_count_after);
        assert_eq!(rel_count_before, rel_count_after);
        assert_eq!(membership_count_before, membership_count_after);
        assert_eq!(candidate_count_before, candidate_count_after);
    }
}

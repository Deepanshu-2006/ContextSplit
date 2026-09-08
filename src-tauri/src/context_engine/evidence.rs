use chrono::Utc;
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
// Unit Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

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
}

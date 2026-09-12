use chrono::Utc;
use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

use crate::context_engine::evidence::{EntityReference, EvidenceProvenance};


/// Core classification of contextual evidence used to infer and understand tasks.
///
/// Every evidence item explicitly maps to an empirical observation (`Observed`)
/// or a model/graph/temporal inference (`Inferred`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskEvidenceType {
    /// Bounded, sanitized text excerpts and titles from recently captured items
    RecentItemContent,
    /// Vector embedding cosine similarities between items or task clusters
    SemanticSimilarity,
    /// Recurring salient terms and keywords across recent items in the context
    RepeatedTerms,
    /// Document filenames, source code stems, extensions, and file paths
    Filenames,
    /// Web URLs, domains, paths, repository/PR numbers, and issue tracker slugs
    UrlsDomains,
    /// Operating system foreground applications and active window titles
    ApplicationContext,
    /// User activity sessions grouping items temporally
    RecentSessions,
    /// Direct graph edges linking items (same session, domain, app, semantic)
    ItemRelationships,
    /// Sequence of contextual activity, switches, and returns over time
    ContextualActivitySequence,
    /// Previously inferred or active task candidates in the context
    PreviousTaskCandidates,
    /// Explicit user feedback (moves, candidate confirmations, dismissals)
    UserFeedback,
}

impl TaskEvidenceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskEvidenceType::RecentItemContent => "recent_item_content",
            TaskEvidenceType::SemanticSimilarity => "semantic_similarity",
            TaskEvidenceType::RepeatedTerms => "repeated_terms",
            TaskEvidenceType::Filenames => "filenames",
            TaskEvidenceType::UrlsDomains => "urls_domains",
            TaskEvidenceType::ApplicationContext => "application_context",
            TaskEvidenceType::RecentSessions => "recent_sessions",
            TaskEvidenceType::ItemRelationships => "item_relationships",
            TaskEvidenceType::ContextualActivitySequence => "contextual_activity_sequence",
            TaskEvidenceType::PreviousTaskCandidates => "previous_task_candidates",
            TaskEvidenceType::UserFeedback => "user_feedback",
        }
    }

    pub fn from_str(s: &str) -> std::result::Result<Self, String> {
        match s.trim().to_lowercase().as_str() {
            "recent_item_content" | "item_content" | "content" => Ok(TaskEvidenceType::RecentItemContent),
            "semantic_similarity" | "similarity" => Ok(TaskEvidenceType::SemanticSimilarity),
            "repeated_terms" | "terms" | "keywords" => Ok(TaskEvidenceType::RepeatedTerms),
            "filenames" | "filename" | "files" => Ok(TaskEvidenceType::Filenames),
            "urls_domains" | "url" | "domain" | "urls" => Ok(TaskEvidenceType::UrlsDomains),
            "application_context" | "app" | "window_title" => Ok(TaskEvidenceType::ApplicationContext),
            "recent_sessions" | "sessions" | "session" => Ok(TaskEvidenceType::RecentSessions),
            "item_relationships" | "relationships" | "graph" => Ok(TaskEvidenceType::ItemRelationships),
            "contextual_activity_sequence" | "activity_sequence" | "transitions" => {
                Ok(TaskEvidenceType::ContextualActivitySequence)
            }
            "previous_task_candidates" | "task_candidates" | "prior_tasks" => {
                Ok(TaskEvidenceType::PreviousTaskCandidates)
            }
            "user_feedback" | "feedback" | "actions" => Ok(TaskEvidenceType::UserFeedback),
            other => Err(format!("Unknown task evidence type: '{}'", other)),
        }
    }

    /// Default provenance classification for this evidence type.
    pub fn default_provenance(&self) -> EvidenceProvenance {
        match self {
            TaskEvidenceType::RecentItemContent
            | TaskEvidenceType::Filenames
            | TaskEvidenceType::UrlsDomains
            | TaskEvidenceType::ApplicationContext
            | TaskEvidenceType::RecentSessions
            | TaskEvidenceType::UserFeedback => EvidenceProvenance::Observed,

            TaskEvidenceType::SemanticSimilarity
            | TaskEvidenceType::RepeatedTerms
            | TaskEvidenceType::ItemRelationships
            | TaskEvidenceType::ContextualActivitySequence
            | TaskEvidenceType::PreviousTaskCandidates => EvidenceProvenance::Inferred,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            TaskEvidenceType::RecentItemContent => "Recent Item Content",
            TaskEvidenceType::SemanticSimilarity => "Semantic Similarity",
            TaskEvidenceType::RepeatedTerms => "Repeated Terms",
            TaskEvidenceType::Filenames => "Filenames & Paths",
            TaskEvidenceType::UrlsDomains => "URLs & Domains",
            TaskEvidenceType::ApplicationContext => "Application & Window Context",
            TaskEvidenceType::RecentSessions => "Recent Sessions",
            TaskEvidenceType::ItemRelationships => "Item Relationships",
            TaskEvidenceType::ContextualActivitySequence => "Context Activity Sequence",
            TaskEvidenceType::PreviousTaskCandidates => "Previous Task Candidates",
            TaskEvidenceType::UserFeedback => "User Feedback",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            TaskEvidenceType::RecentItemContent => "Bounded content excerpts from recently captured items",
            TaskEvidenceType::SemanticSimilarity => "Topological cosine similarity between items or embeddings",
            TaskEvidenceType::RepeatedTerms => "Salient recurring keywords across items in the active context",
            TaskEvidenceType::Filenames => "Filenames, document extensions, and code stems",
            TaskEvidenceType::UrlsDomains => "Normalized web domains, PR/issue slugs, and query targets",
            TaskEvidenceType::ApplicationContext => "Active desktop application and window title telemetry",
            TaskEvidenceType::RecentSessions => "Temporal user activity sessions bounding the work",
            TaskEvidenceType::ItemRelationships => "Direct relationship graph edges between context items",
            TaskEvidenceType::ContextualActivitySequence => "Sequential transitions, switches, and returns",
            TaskEvidenceType::PreviousTaskCandidates => "Historical or active inferred task candidates",
            TaskEvidenceType::UserFeedback => "Explicit user organization actions and dismissals",
        }
    }
}

impl std::fmt::Display for TaskEvidenceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A single piece of evidence contributing to task understanding and inference.
///
/// CRITICAL PRIVACY & ARCHITECTURAL INVARIANTS:
/// 1. Only derives from bounded, existing contextual records (no unrestricted clipboard history).
/// 2. Never uses continuous surveillance, keystroke logging, or screen pixel scraping.
/// 3. Strictly distinguishes `Observed` empirical facts from `Inferred` relationships.
/// 4. Remains bounded to the relevant context and temporal activity window.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskEvidence {
    pub id: String,
    pub evidence_type: TaskEvidenceType,
    pub provenance: EvidenceProvenance,
    /// Normalized strength weight strictly clamped to [0.0, 1.0]
    pub strength: f64,
    /// Short human-readable summary (e.g. "Active App: VS Code - main.rs")
    pub summary: String,
    /// Detailed description for inspection and reasoning
    pub detail: String,
    /// Optional reference to the underlying entity (Item, Session, Signal, Workspace, etc.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_entity: Option<EntityReference>,
    /// ISO-8601 UTC timestamp of observation or calculation
    pub observed_at: String,
    /// Sanitized key-value metadata attributes
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<String, Value>,
}

impl TaskEvidence {
    pub fn new(
        evidence_type: TaskEvidenceType,
        provenance: EvidenceProvenance,
        strength: f64,
        summary: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            evidence_type,
            provenance,
            strength: strength.clamp(0.0, 1.0),
            summary: summary.into(),
            detail: detail.into(),
            source_entity: None,
            observed_at: Utc::now().to_rfc3339(),
            attributes: HashMap::new(),
        }
    }

    pub fn with_source_entity(mut self, entity: EntityReference) -> Self {
        self.source_entity = Some(entity);
        self
    }

    pub fn with_observed_at(mut self, timestamp: impl Into<String>) -> Self {
        self.observed_at = timestamp.into();
        self
    }

    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    pub fn is_observed(&self) -> bool {
        self.provenance.is_observed()
    }

    pub fn is_inferred(&self) -> bool {
        self.provenance.is_inferred()
    }

    // =========================================================================
    // Factory Constructors for Standard Evidence
    // =========================================================================

    /// Observed evidence: frontmost application and window title.
    pub fn observed_app(
        app_name: &str,
        window_title: Option<&str>,
        timestamp: Option<&str>,
    ) -> Self {
        let title_clean = window_title.map(clean_window_title);
        let summary = match &title_clean {
            Some(t) if !t.is_empty() => format!("{}: {}", app_name, t),
            _ => format!("Active App: {}", app_name),
        };
        let detail = format!(
            "User active in application '{}'{}",
            app_name,
            title_clean
                .as_ref()
                .map(|t| format!(" with window title '{}'", t))
                .unwrap_or_default()
        );

        let mut ev = Self::new(
            TaskEvidenceType::ApplicationContext,
            EvidenceProvenance::Observed,
            0.85,
            summary,
            detail,
        );
        if let Some(ts) = timestamp {
            ev.observed_at = ts.to_string();
        }
        ev.attributes.insert("app_name".to_string(), Value::String(app_name.to_string()));
        if let Some(t) = title_clean {
            ev.attributes.insert("window_title".to_string(), Value::String(t));
        }
        ev
    }

    /// Observed evidence: web URL and domain.
    pub fn observed_url_domain(
        url: &str,
        domain: &str,
        path_slug: Option<&str>,
        timestamp: Option<&str>,
    ) -> Self {
        let summary = match path_slug {
            Some(slug) if !slug.is_empty() => format!("{}/{}", domain, slug),
            _ => format!("Domain: {}", domain),
        };
        let detail = format!("Web navigation to '{}' on domain '{}'", url, domain);

        let mut ev = Self::new(
            TaskEvidenceType::UrlsDomains,
            EvidenceProvenance::Observed,
            0.80,
            summary,
            detail,
        );
        if let Some(ts) = timestamp {
            ev.observed_at = ts.to_string();
        }
        ev.attributes.insert("url".to_string(), Value::String(url.to_string()));
        ev.attributes.insert("domain".to_string(), Value::String(domain.to_string()));
        if let Some(slug) = path_slug {
            ev.attributes.insert("path_slug".to_string(), Value::String(slug.to_string()));
        }
        ev
    }

    /// Observed evidence: filename and extension.
    pub fn observed_filename(
        filename: &str,
        extension: Option<&str>,
        timestamp: Option<&str>,
    ) -> Self {
        let summary = format!("File: {}", filename);
        let detail = format!(
            "Working on file '{}'{}",
            filename,
            extension
                .map(|ext| format!(" (type: .{})", ext))
                .unwrap_or_default()
        );

        let mut ev = Self::new(
            TaskEvidenceType::Filenames,
            EvidenceProvenance::Observed,
            0.85,
            summary,
            detail,
        );
        if let Some(ts) = timestamp {
            ev.observed_at = ts.to_string();
        }
        ev.attributes.insert("filename".to_string(), Value::String(filename.to_string()));
        if let Some(ext) = extension {
            ev.attributes.insert("extension".to_string(), Value::String(ext.to_string()));
        }
        ev
    }

    /// Observed evidence: sanitized, bounded item content excerpt.
    /// Invariant: Snippet is strictly truncated to prevent unbounded memory dumps.
    pub fn observed_item_content(
        item_id: &str,
        title: Option<&str>,
        snippet: &str,
        item_type: &str,
        timestamp: Option<&str>,
    ) -> Self {
        let bounded_snippet = truncate_snippet(snippet, 150);
        let summary = match title {
            Some(t) if !t.trim().is_empty() => format!("Item: {}", t.trim()),
            _ => format!("{} item ({})", item_type, &bounded_snippet[..bounded_snippet.len().min(30)]),
        };
        let detail = format!("Captured {} content: \"{}\"", item_type, bounded_snippet);

        let mut ev = Self::new(
            TaskEvidenceType::RecentItemContent,
            EvidenceProvenance::Observed,
            0.75,
            summary,
            detail,
        )
        .with_source_entity(EntityReference::item(item_id));

        if let Some(ts) = timestamp {
            ev.observed_at = ts.to_string();
        }
        ev.attributes.insert("item_id".to_string(), Value::String(item_id.to_string()));
        ev.attributes.insert("item_type".to_string(), Value::String(item_type.to_string()));
        ev.attributes.insert("snippet".to_string(), Value::String(bounded_snippet));
        if let Some(t) = title {
            ev.attributes.insert("title".to_string(), Value::String(t.to_string()));
        }
        ev
    }

    /// Observed evidence: temporal activity session.
    pub fn observed_session(
        session_id: &str,
        item_count: usize,
        started_at: &str,
    ) -> Self {
        let summary = format!("Session: {} ({} items)", &session_id[..session_id.len().min(8)], item_count);
        let detail = format!("Activity grouped within session '{}' containing {} items", session_id, item_count);

        let mut ev = Self::new(
            TaskEvidenceType::RecentSessions,
            EvidenceProvenance::Observed,
            0.70,
            summary,
            detail,
        )
        .with_source_entity(EntityReference::session(session_id))
        .with_observed_at(started_at);

        ev.attributes.insert("session_id".to_string(), Value::String(session_id.to_string()));
        ev.attributes.insert("item_count".to_string(), Value::Number(item_count.into()));
        ev
    }

    /// Observed evidence: explicit user organization action or feedback.
    pub fn observed_user_feedback(
        action: &str,
        description: &str,
        timestamp: Option<&str>,
    ) -> Self {
        let summary = format!("User Action: {}", action.replace('_', " "));
        let detail = description.to_string();

        let mut ev = Self::new(
            TaskEvidenceType::UserFeedback,
            EvidenceProvenance::Observed,
            0.95,
            summary,
            detail,
        );
        if let Some(ts) = timestamp {
            ev.observed_at = ts.to_string();
        }
        ev.attributes.insert("action".to_string(), Value::String(action.to_string()));
        ev
    }

    /// Inferred evidence: semantic embedding similarity between items or task centroid.
    pub fn inferred_semantic_similarity(
        item_a_id: &str,
        item_b_id: &str,
        similarity: f64,
        explanation: &str,
    ) -> Self {
        let sim_pct = (similarity.clamp(0.0, 1.0) * 100.0).round();
        let summary = format!("Semantic Similarity ({:.0}%)", sim_pct);
        let detail = explanation.to_string();

        let mut ev = Self::new(
            TaskEvidenceType::SemanticSimilarity,
            EvidenceProvenance::Inferred,
            similarity,
            summary,
            detail,
        )
        .with_source_entity(EntityReference::item(item_a_id));

        ev.attributes.insert("item_a_id".to_string(), Value::String(item_a_id.to_string()));
        ev.attributes.insert("item_b_id".to_string(), Value::String(item_b_id.to_string()));
        ev.attributes.insert(
            "similarity_score".to_string(),
            serde_json::Number::from_f64(similarity)
                .map(Value::Number)
                .unwrap_or(Value::Null),
        );
        ev
    }

    /// Inferred evidence: salient recurring keywords across items.
    pub fn inferred_repeated_terms(
        terms: &[String],
        frequency_summary: &str,
        strength: f64,
    ) -> Self {
        let term_list = terms.join(", ");
        let summary = format!("Key Terms: {}", term_list);
        let detail = frequency_summary.to_string();

        let mut ev = Self::new(
            TaskEvidenceType::RepeatedTerms,
            EvidenceProvenance::Inferred,
            strength,
            summary,
            detail,
        );
        ev.attributes.insert(
            "terms".to_string(),
            Value::Array(terms.iter().map(|t| Value::String(t.clone())).collect()),
        );
        ev
    }

    /// Inferred evidence: item-to-item relationship graph connection.
    pub fn inferred_item_relationship(
        item_a_id: &str,
        item_b_id: &str,
        relationship_type: &str,
        strength: f64,
    ) -> Self {
        let strength_pct = (strength.clamp(0.0, 1.0) * 100.0).round();
        let summary = format!("Link: {} ({:.0}%)", relationship_type.replace('_', " "), strength_pct);
        let detail = format!(
            "Strong relationship ('{}', strength {:.2}) connects items",
            relationship_type, strength
        );

        let mut ev = Self::new(
            TaskEvidenceType::ItemRelationships,
            EvidenceProvenance::Inferred,
            strength,
            summary,
            detail,
        )
        .with_source_entity(EntityReference::item(item_a_id));

        ev.attributes.insert("item_a_id".to_string(), Value::String(item_a_id.to_string()));
        ev.attributes.insert("item_b_id".to_string(), Value::String(item_b_id.to_string()));
        ev.attributes.insert("relationship_type".to_string(), Value::String(relationship_type.to_string()));
        ev.attributes.insert(
            "strength".to_string(),
            serde_json::Number::from_f64(strength)
                .map(Value::Number)
                .unwrap_or(Value::Null),
        );
        ev
    }

    /// Inferred evidence: contextual activity sequence / transition.
    pub fn inferred_activity_sequence(
        transition_type: &str,
        from_context: Option<&str>,
        to_context: Option<&str>,
        confidence: f64,
        timestamp: Option<&str>,
    ) -> Self {
        let to_display = to_context.unwrap_or("Active Context");
        let from_display = from_context.unwrap_or("Previous");
        let summary = match transition_type {
            "context_switch" => format!("Switched from {} to {}", from_display, to_display),
            "context_return" => format!("Returned to {}", to_display),
            "context_start" => format!("Started {}", to_display),
            "context_end" => format!("Ended {}", from_display),
            _ => format!("Transition in {}", to_display),
        };
        let detail = format!(
            "Contextual transition '{}' detected with {:.0}% confidence",
            transition_type, confidence * 100.0
        );

        let mut ev = Self::new(
            TaskEvidenceType::ContextualActivitySequence,
            EvidenceProvenance::Inferred,
            confidence,
            summary,
            detail,
        );
        if let Some(ts) = timestamp {
            ev.observed_at = ts.to_string();
        }
        ev.attributes.insert("transition_type".to_string(), Value::String(transition_type.to_string()));
        if let Some(f) = from_context {
            ev.attributes.insert("from_context".to_string(), Value::String(f.to_string()));
        }
        if let Some(t) = to_context {
            ev.attributes.insert("to_context".to_string(), Value::String(t.to_string()));
        }
        ev
    }

    /// Inferred evidence: existing or historical task candidate in the context.
    pub fn inferred_previous_task_candidate(
        task_id: &str,
        title: &str,
        status: &str,
        confidence: f64,
    ) -> Self {
        let summary = format!("Prior Task: {} ({})", title, status);
        let detail = format!(
            "Existing task candidate '{}' (status: {}, confidence: {:.0}%)",
            title, status, confidence * 100.0
        );

        let mut ev = Self::new(
            TaskEvidenceType::PreviousTaskCandidates,
            EvidenceProvenance::Inferred,
            confidence,
            summary,
            detail,
        );
        ev.attributes.insert("task_id".to_string(), Value::String(task_id.to_string()));
        ev.attributes.insert("title".to_string(), Value::String(title.to_string()));
        ev.attributes.insert("status".to_string(), Value::String(status.to_string()));
        ev
    }
}

/// A structured bundle of contextual task evidence collected for a specific context or activity cluster.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskEvidenceSet {
    pub context_id: Option<String>,
    pub target_item_id: Option<String>,
    pub generated_at: String,
    pub evidence: Vec<TaskEvidence>,
    pub time_window_seconds: i64,
    pub total_items_scanned: usize,
}

impl TaskEvidenceSet {
    pub fn new(context_id: Option<String>, target_item_id: Option<String>) -> Self {
        Self {
            context_id,
            target_item_id,
            generated_at: Utc::now().to_rfc3339(),
            evidence: Vec::new(),
            time_window_seconds: 86400,
            total_items_scanned: 0,
        }
    }

    pub fn add_evidence(&mut self, ev: TaskEvidence) {
        self.evidence.push(ev);
    }

    pub fn is_empty(&self) -> bool {
        self.evidence.is_empty()
    }

    pub fn len(&self) -> usize {
        self.evidence.len()
    }

    pub fn observed_evidence(&self) -> Vec<&TaskEvidence> {
        self.evidence.iter().filter(|e| e.is_observed()).collect()
    }

    pub fn inferred_evidence(&self) -> Vec<&TaskEvidence> {
        self.evidence.iter().filter(|e| e.is_inferred()).collect()
    }

    pub fn by_type(&self, ev_type: TaskEvidenceType) -> Vec<&TaskEvidence> {
        self.evidence.iter().filter(|e| e.evidence_type == ev_type).collect()
    }

    /// Computes overall evidence diversity (count of distinct active evidence types).
    pub fn evidence_type_diversity(&self) -> usize {
        let types: HashSet<TaskEvidenceType> = self.evidence.iter().map(|e| e.evidence_type).collect();
        types.len()
    }

    /// Computes aggregated evidence strength weighted by type reliability.
    pub fn aggregate_strength(&self) -> f64 {
        if self.evidence.is_empty() {
            return 0.0;
        }
        let total_weight: f64 = self.evidence.iter().map(|e| e.strength).sum();
        (total_weight / self.evidence.len() as f64).clamp(0.0, 1.0)
    }

    /// Extracts unique salient terms found across repeated terms evidence.
    pub fn salient_terms(&self) -> Vec<String> {
        let mut terms = Vec::new();
        for ev in self.by_type(TaskEvidenceType::RepeatedTerms) {
            if let Some(Value::Array(arr)) = ev.attributes.get("terms") {
                for v in arr {
                    if let Some(s) = v.as_str() {
                        if !terms.contains(&s.to_string()) {
                            terms.push(s.to_string());
                        }
                    }
                }
            }
        }
        terms
    }
}

/// Configuration options for bounded task evidence collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEvidenceCollectorOptions {
    /// Maximum lookback time window in seconds (default: 86400 / 24 hours)
    pub time_window_seconds: i64,
    /// Maximum number of recent items to scan (default: 20)
    pub max_recent_items: usize,
    /// Maximum number of contextual signals to inspect (default: 30)
    pub max_signals: usize,
    /// Maximum number of prior task candidates to check (default: 5)
    pub max_previous_candidates: usize,
    /// Minimum relationship graph strength threshold (default: 0.50)
    pub min_relationship_strength: f64,
    /// Minimum semantic similarity threshold (default: 0.70)
    pub min_semantic_similarity: f64,
    /// Maximum character length for item snippets to protect privacy (default: 150)
    pub max_snippet_length: usize,
}

impl Default for TaskEvidenceCollectorOptions {
    fn default() -> Self {
        Self {
            time_window_seconds: 86400, // 24 hours
            max_recent_items: 20,
            max_signals: 30,
            max_previous_candidates: 5,
            min_relationship_strength: 0.50,
            min_semantic_similarity: 0.70,
            max_snippet_length: 150,
        }
    }
}

/// Collector service that derives structured `TaskEvidenceSet` records from existing contextual data.
///
/// Features:
/// - Strictly event-driven (no background keystroke/screen surveillance).
/// - Privacy-preserving bounded text truncation.
/// - Multi-heuristic extraction across all 11 task evidence types.
/// - Clear separation of observed facts and inferred relationships.
#[derive(Debug, Default, Clone)]
pub struct TaskEvidenceCollector {
    pub options: TaskEvidenceCollectorOptions,
}

impl TaskEvidenceCollector {
    pub fn new() -> Self {
        Self {
            options: TaskEvidenceCollectorOptions::default(),
        }
    }

    pub fn with_options(options: TaskEvidenceCollectorOptions) -> Self {
        Self { options }
    }

    /// Collects task evidence for a specific workspace context within the configured time window.
    pub fn collect_for_context(
        &self,
        conn: &Connection,
        workspace_id: &str,
    ) -> Result<TaskEvidenceSet> {
        let mut evidence_set = TaskEvidenceSet::new(Some(workspace_id.to_string()), None);
        evidence_set.time_window_seconds = self.options.time_window_seconds;

        let window_start = (Utc::now() - chrono::Duration::seconds(self.options.time_window_seconds)).to_rfc3339();

        // 1. Fetch recent Items associated with this context
        let mut item_stmt = conn.prepare(
            "SELECT DISTINCT i.id, i.workspace_id, i.session_id, i.item_type, i.content, i.title, i.metadata, i.created_at
             FROM items i
             LEFT JOIN context_memberships cm ON i.id = cm.item_id AND cm.workspace_id = ?1
             WHERE (i.workspace_id = ?1 OR cm.workspace_id = ?1)
               AND i.created_at >= ?2
             ORDER BY i.created_at DESC
             LIMIT ?3",
        )?;

        struct RawItem {
            id: String,
            session_id: Option<String>,
            item_type: String,
            content: String,
            title: Option<String>,
            metadata_str: String,
            created_at: String,
        }

        let item_rows = item_stmt.query_map(
            params![workspace_id, window_start, self.options.max_recent_items as i64],
            |row| {
                Ok(RawItem {
                    id: row.get(0)?,
                    session_id: row.get(2)?,
                    item_type: row.get(3)?,
                    content: row.get(4)?,
                    title: row.get(5)?,
                    metadata_str: row.get(6)?,
                    created_at: row.get(7)?,
                })
            },
        )?;

        let mut items = Vec::new();
        let mut item_ids = Vec::new();
        let mut session_ids = HashSet::new();
        let mut text_corpus: Vec<String> = Vec::new();

        for item_res in item_rows {
            if let Ok(item) = item_res {
                if let Some(ref s_id) = item.session_id {
                    session_ids.insert(s_id.clone());
                }
                item_ids.push(item.id.clone());

                // 1.1 Observed Recent Item Content
                let bounded_content = truncate_snippet(&item.content, self.options.max_snippet_length);
                evidence_set.add_evidence(TaskEvidence::observed_item_content(
                    &item.id,
                    item.title.as_deref(),
                    &bounded_content,
                    &item.item_type,
                    Some(&item.created_at),
                ));

                if let Some(ref t) = item.title {
                    text_corpus.push(t.clone());
                }
                text_corpus.push(bounded_content);

                // 1.2 Observed Filenames (from title or file item)
                if let Some(ref t) = item.title {
                    if let Some(ext) = extract_file_extension(t) {
                        evidence_set.add_evidence(TaskEvidence::observed_filename(
                            t,
                            Some(&ext),
                            Some(&item.created_at),
                        ));
                    }
                }

                // 1.3 Observed URLs & Domains (from url items or metadata)
                if item.item_type == "url" {
                    if let Ok(parsed) = url::Url::parse(&item.content) {
                        let domain = parsed.host_str().unwrap_or("unknown");
                        let slug = extract_url_path_slug(&parsed);
                        evidence_set.add_evidence(TaskEvidence::observed_url_domain(
                            &item.content,
                            domain,
                            slug.as_deref(),
                            Some(&item.created_at),
                        ));
                    }
                } else if let Ok(meta_val) = serde_json::from_str::<Value>(&item.metadata_str) {
                    if let Some(url_str) = meta_val.get("original_url").and_then(|v| v.as_str()) {
                        if let Ok(parsed) = url::Url::parse(url_str) {
                            let domain = parsed.host_str().unwrap_or("unknown");
                            let slug = extract_url_path_slug(&parsed);
                            evidence_set.add_evidence(TaskEvidence::observed_url_domain(
                                url_str,
                                domain,
                                slug.as_deref(),
                                Some(&item.created_at),
                            ));
                        }
                    }
                    if let Some(app) = meta_val.get("app_name").and_then(|v| v.as_str()) {
                        let win_title = meta_val.get("window_title").and_then(|v| v.as_str());
                        evidence_set.add_evidence(TaskEvidence::observed_app(
                            app,
                            win_title,
                            Some(&item.created_at),
                        ));
                    }
                }

                items.push(item);
            }
        }

        evidence_set.total_items_scanned = items.len();

        // 2. Fetch context signals (Application context, Window titles, User feedback)
        let mut sig_stmt = conn.prepare(
            "SELECT id, item_id, signal_type, session_id, payload, created_at
             FROM context_signals
             WHERE (workspace_id = ?1 OR item_id IN (SELECT id FROM items WHERE workspace_id = ?1))
               AND created_at >= ?2
             ORDER BY created_at DESC
             LIMIT ?3",
        )?;

        if let Ok(sig_rows) = sig_stmt.query_map(
            params![workspace_id, window_start, self.options.max_signals as i64],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        ) {
            for res in sig_rows.flatten() {
                let (_sig_id, it_id, sig_type, s_id, payload_str, created_at) = res;
                if let Some(ref sess) = s_id {
                    session_ids.insert(sess.clone());
                }
                let payload: Value = serde_json::from_str(&payload_str).unwrap_or(Value::Null);

                match sig_type.as_str() {
                    "active_app" => {
                        if let Some(app) = payload.get("app_name").and_then(|v| v.as_str()) {
                            let win = payload.get("window_title").and_then(|v| v.as_str());
                            evidence_set.add_evidence(TaskEvidence::observed_app(
                                app,
                                win,
                                Some(&created_at),
                            ));
                        }
                    }
                    "window_title" => {
                        if let Some(title) = payload.get("window_title").and_then(|v| v.as_str()) {
                            let app = payload.get("app_name").and_then(|v| v.as_str()).unwrap_or("Desktop");
                            evidence_set.add_evidence(TaskEvidence::observed_app(
                                app,
                                Some(title),
                                Some(&created_at),
                            ));
                        }
                    }
                    "url_metadata" | "domain" => {
                        if let Some(dom) = payload.get("domain").and_then(|v| v.as_str()) {
                            let full_url = payload.get("url").and_then(|v| v.as_str()).unwrap_or(dom);
                            evidence_set.add_evidence(TaskEvidence::observed_url_domain(
                                full_url,
                                dom,
                                None,
                                Some(&created_at),
                            ));
                        }
                    }
                    "user_action" | "candidate_accepted" | "candidate_renamed" | "candidate_dismissed" => {
                        let action = payload.get("action").and_then(|v| v.as_str()).unwrap_or(&sig_type);
                        let desc = format!("User executed '{}' in context", action);
                        let mut ev = TaskEvidence::observed_user_feedback(action, &desc, Some(&created_at));
                        if let Some(ref i_id) = it_id {
                            ev = ev.with_source_entity(EntityReference::item(i_id));
                        }
                        evidence_set.add_evidence(ev);
                    }
                    _ => {}
                }
            }
        }

        // 3. Fetch Recent Sessions
        for s_id in &session_ids {
            if let Ok(sess) = crate::db::operations::get_session(conn, s_id) {
                let session_item_count = items.iter().filter(|i| i.session_id.as_deref() == Some(s_id.as_str())).count();
                evidence_set.add_evidence(TaskEvidence::observed_session(
                    &sess.id,
                    session_item_count,
                    &sess.started_at,
                ));
            }
        }

        // 4. Fetch Item Relationships
        if !item_ids.is_empty() {
            let placeholders = vec!["?"; item_ids.len()].join(",");
            let rel_sql = format!(
                "SELECT item_a_id, item_b_id, relationship_type, strength, created_at
                 FROM item_relationships
                 WHERE item_a_id IN ({}) AND item_b_id IN ({}) AND strength >= ?
                 ORDER BY strength DESC
                 LIMIT 20",
                placeholders, placeholders
            );

            if let Ok(mut rel_stmt) = conn.prepare(&rel_sql) {
                let mut params_vec: Vec<&dyn rusqlite::ToSql> = Vec::new();
                for id in &item_ids {
                    params_vec.push(id);
                }
                for id in &item_ids {
                    params_vec.push(id);
                }
                params_vec.push(&self.options.min_relationship_strength);

                if let Ok(rows) = rel_stmt.query_map(rusqlite::params_from_iter(params_vec), |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, f64>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                }) {
                    for r_res in rows.flatten() {
                        let (a_id, b_id, rel_type, strength, _created_at) = r_res;
                        if rel_type == "semantic_similarity" {
                            let expl = format!("Embedding cosine similarity between items is {:.0}%", strength * 100.0);
                            evidence_set.add_evidence(TaskEvidence::inferred_semantic_similarity(
                                &a_id, &b_id, strength, &expl,
                            ));
                        } else {
                            evidence_set.add_evidence(TaskEvidence::inferred_item_relationship(
                                &a_id, &b_id, &rel_type, strength,
                            ));
                        }
                    }
                }
            }
        }

        // 5. Inferred Repeated Terms
        let salient_terms = extract_salient_terms(&text_corpus, 5);
        if !salient_terms.is_empty() {
            let summary = format!("Salient recurring terms: {}", salient_terms.join(", "));
            evidence_set.add_evidence(TaskEvidence::inferred_repeated_terms(
                &salient_terms,
                &summary,
                0.80,
            ));
        }

        // 6. Inferred Contextual Activity Sequence / Transitions
        if let Ok(transitions) = crate::db::operations::list_context_transitions_for_context(conn, workspace_id) {
            for trans in transitions.into_iter().take(5) {
                evidence_set.add_evidence(TaskEvidence::inferred_activity_sequence(
                    trans.transition_type.as_str(),
                    trans.from_context_id.as_deref(),
                    trans.to_context_id.as_deref(),
                    trans.confidence,
                    Some(&trans.detected_at),
                ));
            }
        }

        // 7. Inferred Previous Task Candidates
        if let Ok(candidates) = crate::db::operations::list_task_candidates_for_context(conn, workspace_id) {
            for cand in candidates.into_iter().take(self.options.max_previous_candidates) {
                evidence_set.add_evidence(TaskEvidence::inferred_previous_task_candidate(
                    &cand.id,
                    &cand.title,
                    cand.status.as_str(),
                    cand.confidence,
                ));
            }
        }

        Ok(evidence_set)
    }

    /// Collects task evidence focused around a single target Item.
    pub fn collect_for_item(
        &self,
        conn: &Connection,
        item_id: &str,
    ) -> Result<TaskEvidenceSet> {
        let item = crate::db::operations::get_item(conn, item_id)?;
        let mut evidence_set = TaskEvidenceSet::new(item.workspace_id.clone(), Some(item_id.to_string()));
        evidence_set.time_window_seconds = self.options.time_window_seconds;

        // 1. Target Item Content
        let bounded_snippet = truncate_snippet(&item.content, self.options.max_snippet_length);
        evidence_set.add_evidence(TaskEvidence::observed_item_content(
            &item.id,
            item.title.as_deref(),
            &bounded_snippet,
            &item.item_type,
            Some(&item.created_at),
        ));

        // 2. Filename (if applicable)
        if let Some(ref t) = item.title {
            if let Some(ext) = extract_file_extension(t) {
                evidence_set.add_evidence(TaskEvidence::observed_filename(
                    t,
                    Some(&ext),
                    Some(&item.created_at),
                ));
            }
        }

        // 3. URLs & Domains
        if item.item_type == "url" {
            if let Ok(parsed) = url::Url::parse(&item.content) {
                let domain = parsed.host_str().unwrap_or("unknown");
                let slug = extract_url_path_slug(&parsed);
                evidence_set.add_evidence(TaskEvidence::observed_url_domain(
                    &item.content,
                    domain,
                    slug.as_deref(),
                    Some(&item.created_at),
                ));
            }
        }

        // 4. Signals for this item
        let signals = crate::context_engine::operations::list_context_signals(conn, None, Some(item_id))
            .unwrap_or_default();

        for sig in signals {
            let payload: Value = serde_json::from_str(&sig.payload).unwrap_or(Value::Null);
            if sig.signal_type == "active_app" {
                if let Some(app) = payload.get("app_name").and_then(|v| v.as_str()) {
                    let win = payload.get("window_title").and_then(|v| v.as_str());
                    evidence_set.add_evidence(TaskEvidence::observed_app(
                        app,
                        win,
                        Some(&sig.created_at),
                    ));
                }
            } else if sig.signal_type == "url_metadata" {
                if let Some(dom) = payload.get("domain").and_then(|v| v.as_str()) {
                    let full_url = payload.get("url").and_then(|v| v.as_str()).unwrap_or(dom);
                    evidence_set.add_evidence(TaskEvidence::observed_url_domain(
                        full_url,
                        dom,
                        None,
                        Some(&sig.created_at),
                    ));
                }
            }
        }

        // 5. Graph Relationships for this item
        if let Ok(mut rel_stmt) = conn.prepare(
            "SELECT item_a_id, item_b_id, relationship_type, strength, created_at
             FROM item_relationships
             WHERE (item_a_id = ?1 OR item_b_id = ?1) AND strength >= ?2
             ORDER BY strength DESC
             LIMIT 10",
        ) {
            if let Ok(rows) = rel_stmt.query_map(params![item_id, self.options.min_relationship_strength], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, String>(4)?,
                ))
            }) {
                for r_res in rows.flatten() {
                    let (a_id, b_id, rel_type, strength, _ts) = r_res;
                    if rel_type == "semantic_similarity" {
                        evidence_set.add_evidence(TaskEvidence::inferred_semantic_similarity(
                            &a_id, &b_id, strength, "Semantic similarity with connected item",
                        ));
                    } else {
                        evidence_set.add_evidence(TaskEvidence::inferred_item_relationship(
                            &a_id, &b_id, &rel_type, strength,
                        ));
                    }
                }
            }
        }

        // 6. Associated Session
        if let Some(ref s_id) = item.session_id {
            if let Ok(sess) = crate::db::operations::get_session(conn, s_id) {
                evidence_set.add_evidence(TaskEvidence::observed_session(
                    &sess.id,
                    1,
                    &sess.started_at,
                ));
            }
        }

        // 7. Associated Context's Task Candidates
        if let Some(ref ws_id) = item.workspace_id {
            if let Ok(candidates) = crate::db::operations::list_task_candidates_for_context(conn, ws_id) {
                for cand in candidates.into_iter().take(3) {
                    evidence_set.add_evidence(TaskEvidence::inferred_previous_task_candidate(
                        &cand.id,
                        &cand.title,
                        cand.status.as_str(),
                        cand.confidence,
                    ));
                }
            }
        }

        evidence_set.total_items_scanned = 1;
        Ok(evidence_set)
    }
}

// =========================================================================
// Keyword & Text Utility Helpers
// =========================================================================

/// Clean application suffixes from window titles (e.g. "main.rs - Visual Studio Code" -> "main.rs")
pub fn clean_window_title(raw: &str) -> String {
    let mut title = raw.trim().to_string();
    let suffixes = [
        " - Google Chrome",
        " - Mozilla Firefox",
        " - Safari",
        " - Visual Studio Code",
        " - VS Code",
        " - Xcode",
        " - Sublime Text",
        " - Microsoft Word",
        " - Preview",
        " - Notes",
        " - Slack",
        " - Terminal",
        " — Visual Studio Code",
    ];

    for suff in suffixes {
        if title.ends_with(suff) {
            title = title[..title.len() - suff.len()].trim().to_string();
        }
    }
    title
}

/// Truncates string to `max_len` without breaking multibyte characters.
pub fn truncate_snippet(s: &str, max_len: usize) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() <= max_len {
        trimmed.to_string()
    } else {
        let truncated: String = trimmed.chars().take(max_len).collect();
        format!("{}...", truncated)
    }
}

/// Extracts file extension from a title or path if valid.
pub fn extract_file_extension(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if let Some(pos) = trimmed.rfind('.') {
        let ext = &trimmed[pos + 1..];
        if !ext.is_empty() && ext.len() <= 8 && !ext.contains(' ') && !ext.contains('/') {
            return Some(ext.to_lowercase());
        }
    }
    None
}

/// Extracts meaningful path slug from a URL (e.g. `/org/repo/pull/142` -> `org/repo/pull/142`)
pub fn extract_url_path_slug(parsed: &url::Url) -> Option<String> {
    let path = parsed.path().trim_matches('/');
    if !path.is_empty() && path != "index.html" && path != "home" {
        Some(path.to_string())
    } else {
        None
    }
}

/// Stop words list for filtering salient task terms.
static STOP_WORDS: &[&str] = &[
    "a", "about", "above", "after", "again", "against", "all", "am", "an", "and",
    "any", "are", "aren't", "as", "at", "be", "because", "been", "before", "being",
    "below", "between", "both", "but", "by", "can", "can't", "cannot", "could",
    "did", "do", "does", "doing", "don't", "down", "during", "each", "few", "for",
    "from", "further", "had", "has", "have", "having", "he", "her", "here", "hers",
    "herself", "him", "himself", "his", "how", "i", "if", "in", "into", "is",
    "isn't", "it", "it's", "its", "itself", "let's", "me", "more", "most", "must",
    "my", "myself", "no", "nor", "not", "of", "off", "on", "once", "only", "or",
    "other", "ought", "our", "ours", "ourselves", "out", "over", "own", "same",
    "she", "should", "so", "some", "such", "than", "that", "the", "their", "theirs",
    "them", "themselves", "then", "there", "these", "they", "this", "those", "through",
    "to", "too", "under", "until", "up", "very", "was", "we", "were", "what",
    "when", "where", "which", "while", "who", "whom", "why", "with", "would",
    "you", "your", "yours", "yourself", "yourselves", "http", "https", "com", "org",
    "net", "io", "dev", "app", "file", "page", "item", "null", "undefined",
];

/// Extracts salient, recurring keywords from a corpus of recent texts.
pub fn extract_salient_terms(corpus: &[String], top_k: usize) -> Vec<String> {
    let stop_set: HashSet<&'static str> = STOP_WORDS.iter().cloned().collect();
    let mut term_counts: HashMap<String, usize> = HashMap::new();

    for text in corpus {
        let words = text
            .to_lowercase()
            .replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', " ")
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| c == '-' || c == '_').to_string())
            .filter(|w| w.len() >= 3 && !stop_set.contains(w.as_str()))
            .collect::<HashSet<String>>();

        for w in words {
            *term_counts.entry(w).or_insert(0) += 1;
        }
    }

    let mut sorted_terms: Vec<(String, usize)> = term_counts.into_iter().collect();
    sorted_terms.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    sorted_terms
        .into_iter()
        .take(top_k)
        .map(|(term, _)| term)
        .collect()
}

// =========================================================================
// Unit Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::TaskCandidate;
    use crate::db::operations::{create_item, create_session, create_workspace, create_task_candidate};
    use crate::context_engine::operations::create_context_signal;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_task_evidence_types_and_provenance_invariants() {
        // Verify all 11 evidence types map to their proper default provenance
        for ev_type in [
            TaskEvidenceType::RecentItemContent,
            TaskEvidenceType::Filenames,
            TaskEvidenceType::UrlsDomains,
            TaskEvidenceType::ApplicationContext,
            TaskEvidenceType::RecentSessions,
            TaskEvidenceType::UserFeedback,
        ] {
            assert_eq!(ev_type.default_provenance(), EvidenceProvenance::Observed);
        }

        for ev_type in [
            TaskEvidenceType::SemanticSimilarity,
            TaskEvidenceType::RepeatedTerms,
            TaskEvidenceType::ItemRelationships,
            TaskEvidenceType::ContextualActivitySequence,
            TaskEvidenceType::PreviousTaskCandidates,
        ] {
            assert_eq!(ev_type.default_provenance(), EvidenceProvenance::Inferred);
        }

        // Test string parsing
        for ev_type in [
            TaskEvidenceType::RecentItemContent,
            TaskEvidenceType::SemanticSimilarity,
            TaskEvidenceType::RepeatedTerms,
            TaskEvidenceType::Filenames,
            TaskEvidenceType::UrlsDomains,
            TaskEvidenceType::ApplicationContext,
            TaskEvidenceType::RecentSessions,
            TaskEvidenceType::ItemRelationships,
            TaskEvidenceType::ContextualActivitySequence,
            TaskEvidenceType::PreviousTaskCandidates,
            TaskEvidenceType::UserFeedback,
        ] {
            let s = ev_type.as_str();
            assert_eq!(TaskEvidenceType::from_str(s).unwrap(), ev_type);
        }
    }

    #[test]
    fn test_privacy_bounded_snippet_truncation() {
        let long_text = "A".repeat(500);
        let truncated = truncate_snippet(&long_text, 150);
        assert!(truncated.len() <= 155); // 150 + "..."
        assert!(truncated.ends_with("..."));

        let short_text = "Short text under limit";
        assert_eq!(truncate_snippet(short_text, 150), short_text);
    }

    #[test]
    fn test_window_title_cleaning() {
        assert_eq!(
            clean_window_title("main.rs - Visual Studio Code"),
            "main.rs"
        );
        assert_eq!(
            clean_window_title("PR #142: Fix transition hysteresis - Google Chrome"),
            "PR #142: Fix transition hysteresis"
        );
        assert_eq!(
            clean_window_title("Canvas - Stanford University - Safari"),
            "Canvas - Stanford University"
        );
    }

    #[test]
    fn test_extract_salient_terms() {
        let corpus = vec![
            "Lecture 4 Dynamic Programming in Rust".to_string(),
            "Dynamic Programming assignment 2 algorithms".to_string(),
            "Rust algorithms and Dynamic Programming slides".to_string(),
        ];

        let terms = extract_salient_terms(&corpus, 3);
        assert!(terms.contains(&"dynamic".to_string()));
        assert!(terms.contains(&"programming".to_string()));
        assert!(terms.contains(&"algorithms".to_string()) || terms.contains(&"rust".to_string()));
        // Stop words must not be present
        assert!(!terms.contains(&"in".to_string()));
        assert!(!terms.contains(&"and".to_string()));
    }

    #[test]
    fn test_task_evidence_collector_for_context() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "ContextSplit Development").unwrap();
        let session = create_session(&conn).unwrap();

        // 1. Create Items
        let item1 = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "code",
            "pub fn detect_transitions() -> TransitionEvaluationResult { ... }",
            Some("transitions.rs"),
            "{\"app_name\":\"VS Code\",\"window_title\":\"transitions.rs - Visual Studio Code\"}",
        )
        .unwrap();

        let item2 = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://github.com/my-org/contextsplit/pull/142",
            Some("PR #142: Fix transition hysteresis"),
            "{\"domain\":\"github.com\"}",
        )
        .unwrap();

        // 2. Create Signals
        let sig_payload = serde_json::json!({
            "app_name": "VS Code",
            "window_title": "transitions.rs - Visual Studio Code"
        })
        .to_string();
        let _ = create_context_signal(&conn, Some(&ws.id), Some(&item1.id), "active_app", Some(&session.id), &sig_payload).unwrap();

        // 3. Create Item Relationship
        let _ = crate::db::operations::create_item_relationship(
            &conn,
            &item1.id,
            &item2.id,
            "same_session",
            0.90,
            Some("{}"),
        )
        .unwrap();

        // 4. Create Task Candidate
        let task_cand = TaskCandidate::new(
            Some(ws.id.clone()),
            "Fixing Transition Hysteresis Bug",
            0.92,
        );
        let _ = create_task_candidate(&conn, &task_cand).unwrap();

        // 5. Collect Evidence
        let collector = TaskEvidenceCollector::new();
        let evidence_set = collector.collect_for_context(&conn, &ws.id).unwrap();

        assert_eq!(evidence_set.context_id.as_deref(), Some(ws.id.as_str()));
        assert!(!evidence_set.is_empty());
        assert_eq!(evidence_set.total_items_scanned, 2);

        // Verify Observed Evidence
        let observed = evidence_set.observed_evidence();
        assert!(!observed.is_empty(), "Must have observed empirical facts");
        let has_app = observed.iter().any(|e| e.evidence_type == TaskEvidenceType::ApplicationContext);
        let has_content = observed.iter().any(|e| e.evidence_type == TaskEvidenceType::RecentItemContent);
        let has_filename = observed.iter().any(|e| e.evidence_type == TaskEvidenceType::Filenames);
        let has_url = observed.iter().any(|e| e.evidence_type == TaskEvidenceType::UrlsDomains);
        let has_session = observed.iter().any(|e| e.evidence_type == TaskEvidenceType::RecentSessions);

        assert!(has_app, "Must contain observed application evidence");
        assert!(has_content, "Must contain observed content evidence");
        assert!(has_filename, "Must contain observed filename evidence");
        assert!(has_url, "Must contain observed URL/domain evidence");
        assert!(has_session, "Must contain observed session evidence");

        // Verify Inferred Evidence
        let inferred = evidence_set.inferred_evidence();
        assert!(!inferred.is_empty(), "Must have inferred relationship facts");
        let has_rel = inferred.iter().any(|e| e.evidence_type == TaskEvidenceType::ItemRelationships);
        let has_terms = inferred.iter().any(|e| e.evidence_type == TaskEvidenceType::RepeatedTerms);
        let has_prev_task = inferred.iter().any(|e| e.evidence_type == TaskEvidenceType::PreviousTaskCandidates);

        assert!(has_rel, "Must contain inferred relationship evidence");
        assert!(has_terms, "Must contain inferred repeated terms evidence");
        assert!(has_prev_task, "Must contain inferred previous task candidate evidence");

        // Verify Evidence Diversity & Strength
        assert!(evidence_set.evidence_type_diversity() >= 5);
        assert!(evidence_set.aggregate_strength() > 0.60);
    }

    #[test]
    fn test_task_evidence_collector_for_item() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Bio Lab").unwrap();

        let item = create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Spectrophotometry readings for protein assay at 595nm.",
            Some("Lab_Report_3.docx"),
            "{\"app_name\":\"Microsoft Word\"}",
        )
        .unwrap();

        let collector = TaskEvidenceCollector::new();
        let evidence_set = collector.collect_for_item(&conn, &item.id).unwrap();

        assert_eq!(evidence_set.target_item_id.as_deref(), Some(item.id.as_str()));
        assert_eq!(evidence_set.context_id.as_deref(), Some(ws.id.as_str()));

        let has_filename = evidence_set.evidence.iter().any(|e| e.evidence_type == TaskEvidenceType::Filenames);
        let has_content = evidence_set.evidence.iter().any(|e| e.evidence_type == TaskEvidenceType::RecentItemContent);

        assert!(has_filename);
        assert!(has_content);
    }
}

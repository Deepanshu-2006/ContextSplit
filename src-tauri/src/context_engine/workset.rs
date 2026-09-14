use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};

use crate::context_engine::memory::{
    ContextMemory, InformationProvenance, MemoryExplanation, MemoryTaskReference,
};
use crate::context_engine::segments::ContextActivitySegment;
use crate::context_engine::temporal::TemporalDecayConfig;
use crate::db::models::{
    ContextMembership, ContextSignal, ContextTransition, Item, ItemRelationship, Workspace,
};

// =========================================================================
// Helpers for Normalization & Snippets
// =========================================================================

fn extract_domain_from_url(url: &str) -> Option<String> {
    let s = url.trim();
    let without_proto = if let Some(stripped) = s.strip_prefix("https://") {
        stripped
    } else if let Some(stripped) = s.strip_prefix("http://") {
        stripped
    } else {
        return None;
    };
    let host = without_proto.split('/').next()?.split(':').next()?;
    let clean = host.strip_prefix("www.").unwrap_or(host).trim();
    if clean.is_empty() || clean == "localhost" || clean == "127.0.0.1" {
        None
    } else {
        Some(clean.to_lowercase())
    }
}

fn normalize_app_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("unknown") {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn create_content_snippet(content: &str, item_type: &str) -> String {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return match item_type {
            "image" => "Image asset".to_string(),
            "file" => "File attachment".to_string(),
            "url" => "Link".to_string(),
            _ => "Empty content".to_string(),
        };
    }
    if trimmed.len() > 120 {
        format!("{}...", &trimmed[..117])
    } else {
        trimmed.to_string()
    }
}

// =========================================================================
// Enums: WorksetItemRelevance & WorksetState
// =========================================================================

/// Relevance classification for an item within a contextual workset.
///
/// Distinct from raw scoring numbers; explicitly expresses the functional role
/// of the item in supporting current focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorksetItemRelevance {
    /// Core items directly tied to the active task (e.g. main document/component being edited).
    Primary,
    /// Supporting items directly referencing or referenced by the active task (e.g. specs, mocks, animation libs).
    Supporting,
    /// Background context materials, repository links, general workspace reference assets.
    Reference,
    /// Items recently captured or interacted with in the latest active burst/session.
    Recent,
    /// Graph-connected or semantically similar items associated with the working context.
    Related,
}

impl WorksetItemRelevance {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Supporting => "supporting",
            Self::Reference => "reference",
            Self::Recent => "recent",
            Self::Related => "related",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Primary => "Primary",
            Self::Supporting => "Supporting",
            Self::Reference => "Reference",
            Self::Recent => "Recent",
            Self::Related => "Related",
        }
    }

    pub fn priority_weight(&self) -> f64 {
        match self {
            Self::Primary => 1.00,
            Self::Supporting => 0.80,
            Self::Recent => 0.70,
            Self::Related => 0.55,
            Self::Reference => 0.40,
        }
    }
}

impl std::fmt::Display for WorksetItemRelevance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Operational state of the Context Workset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorksetState {
    /// Actively receiving signals and user focus in the current session.
    Active,
    /// Recently active (e.g. within 24 hours), ready to resume with warm state.
    RecentlyActive,
    /// Returning to context after a detour or interruption.
    DetourReturn,
    /// Context is dormant with no recent activity in the past 7 days.
    Dormant,
    /// Brand new context or insufficient data to reconstruct full state.
    InsufficientEvidence,
}

impl WorksetState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::RecentlyActive => "recently_active",
            Self::DetourReturn => "detour_return",
            Self::Dormant => "dormant",
            Self::InsufficientEvidence => "insufficient_evidence",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Active => "Active Workset",
            Self::RecentlyActive => "Recently Active",
            Self::DetourReturn => "Returned from Detour",
            Self::Dormant => "Dormant Workset",
            Self::InsufficientEvidence => "New Workset",
        }
    }
}

impl std::fmt::Display for WorksetState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// =========================================================================
// WorksetStabilityConfig: Hysteresis, Inertia & Anti-Jitter Settings
// =========================================================================

/// Configuration options for contextual workset stability, hysteresis, and anti-jitter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorksetStabilityConfig {
    /// Inertia boost added to items already present in the previous working set (default: 0.12)
    pub previous_membership_inertia: f64,
    /// Score threshold at which strong new evidence is fast-tracked immediately (default: 0.75)
    pub fast_track_relevance_threshold: f64,
    /// Score threshold below which weak items are dampened without repeated evidence (default: 0.50)
    pub weak_item_suppression_threshold: f64,
    /// Score margin that a new item must beat an existing member by to displace it (default: 0.10)
    pub displacement_margin: f64,
    /// Minimum segment observations required for weak evidence to enter (default: 2)
    pub min_weak_evidence_occurrences: usize,
    /// Maximum age in seconds to retain previous state for a context return (default: 86400s / 24h)
    pub return_retention_window_secs: i64,
}

impl Default for WorksetStabilityConfig {
    fn default() -> Self {
        Self {
            previous_membership_inertia: 0.12,
            fast_track_relevance_threshold: 0.75,
            weak_item_suppression_threshold: 0.50,
            displacement_margin: 0.10,
            min_weak_evidence_occurrences: 2,
            return_retention_window_secs: 86400,
        }
    }
}

// =========================================================================
// WorksetConfig: Configurable Ranking, Sizing & Stability
// =========================================================================

/// Configuration options for deriving and ranking contextual worksets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorksetConfig {
    /// Minimum target items in the working set (default: 5)
    pub min_items: usize,
    /// Maximum cap for working set items to avoid over-selection (default: 8)
    pub max_items: usize,
    /// Whether to evaluate high-confidence unfiled inbox items for suggested addition (default: true)
    pub include_suggested_inbox: bool,
    /// Minimum confidence threshold for suggested inbox items (default: 0.60)
    pub suggested_min_confidence: f64,
    /// Temporal decay configuration for recency weighting
    pub temporal_decay: TemporalDecayConfig,
    /// Stability and hysteresis configuration
    pub stability: WorksetStabilityConfig,
}

impl Default for WorksetConfig {
    fn default() -> Self {
        Self {
            min_items: 5,
            max_items: 8,
            include_suggested_inbox: true,
            suggested_min_confidence: 0.60,
            temporal_decay: TemporalDecayConfig::default(),
            stability: WorksetStabilityConfig::default(),
        }
    }
}

// =========================================================================
// WorksetItemReference: Lightweight Non-Duplicative Reference
// =========================================================================

/// Lightweight reference to an item within a contextual workset.
///
/// Invariant: Does NOT duplicate large payloads, base64 data, or raw vector embeddings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorksetItemReference {
    /// Unique item identifier
    pub id: String,
    /// Human-friendly title
    pub title: Option<String>,
    /// Item type (e.g., "url", "text", "file", "image")
    pub item_type: String,
    /// Functional relevance classification within the workset
    pub relevance: WorksetItemRelevance,
    /// Composite normalized relevance score [0.0, 1.0] (kept internal/auxiliary)
    pub relevance_score: f64,
    /// Natural-language explainable reasons for this item's ranking and classification
    pub reasons: Vec<String>,
    /// Epistemic source (Observed, Inferred, or Confirmed)
    pub provenance: InformationProvenance,
    /// ISO-8601 timestamp of item capture or creation
    pub created_at: String,
    /// ISO-8601 timestamp of most recent activity involving this item
    pub last_active_at: Option<String>,
    /// Associated foreground application name if known
    pub app_name: Option<String>,
    /// Associated web domain if known
    pub domain: Option<String>,
    /// Concise content snippet for preview
    pub content_snippet: String,
    /// True if this item is an unfiled inbox item suggested for this context
    pub is_suggested_addition: bool,
}

// =========================================================================
// ContextWorkset: Derived Contextual Working State
// =========================================================================

/// Derived representation of the small set of information most useful for continuing
/// work inside a context.
///
/// Consumes existing `ContextMemory`, `TaskEvidence`, `ItemRelationships`, and `ContextEngine`
/// outputs without duplicating storage or creating parallel ranking engines.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextWorkset {
    /// Context / Workspace identifier
    pub context_id: String,
    /// Human-friendly context name (e.g., "Aurora Website", "TripWise")
    pub context_name: String,
    /// Active task candidate ID if identified (nullable)
    pub task_id: Option<String>,
    /// Inferred or confirmed active task title (nullable)
    pub task_title: Option<String>,
    /// ISO-8601 timestamp when this workset was derived
    pub generated_at: String,
    /// ISO-8601 timestamp of most recent activity in this context (nullable)
    pub last_active_at: Option<String>,
    /// Operational state of the working set
    pub workset_state: WorksetState,
    /// All classified workset items ordered by relevance
    pub items: Vec<WorksetItemReference>,
    /// Concise human-readable recent activity summary
    pub recent_activity: String,
    /// High-priority relevant items (Primary + Supporting tiers)
    pub relevant_items: Vec<WorksetItemReference>,
    /// Important background and repository reference items (Reference tier)
    pub important_references: Vec<WorksetItemReference>,
    /// Related and recent tasks in this context
    pub related_tasks: Vec<MemoryTaskReference>,
    /// Overall workset confidence score [0.0, 1.0]
    pub confidence: f64,
    /// Qualitative confidence band ("High", "Medium", "Low", "None")
    pub confidence_band: String,
    /// Epistemic classification
    pub provenance: InformationProvenance,
    /// Structured explanation grounding this workset
    pub explanation: MemoryExplanation,
}

impl ContextWorkset {
    /// Returns true if the workset contains an active task with at least medium confidence.
    pub fn has_active_task(&self) -> bool {
        self.task_title.is_some() && self.confidence >= 0.45
    }

    /// Returns the primary items directly supporting the active task.
    pub fn primary_items(&self) -> Vec<&WorksetItemReference> {
        self.items
            .iter()
            .filter(|i| i.relevance == WorksetItemRelevance::Primary)
            .collect()
    }

    /// Returns the supporting items.
    pub fn supporting_items(&self) -> Vec<&WorksetItemReference> {
        self.items
            .iter()
            .filter(|i| i.relevance == WorksetItemRelevance::Supporting)
            .collect()
    }

    /// Returns the recent items.
    pub fn recent_items(&self) -> Vec<&WorksetItemReference> {
        self.items
            .iter()
            .filter(|i| i.relevance == WorksetItemRelevance::Recent)
            .collect()
    }
}

// =========================================================================
// ContextWorksetCache: Thread-Safe In-Memory Cache
// =========================================================================

#[derive(Debug, Clone)]
struct CachedWorksetEntry {
    workset: ContextWorkset,
    cached_at: Instant,
}

/// In-memory thread-safe cache for context worksets to avoid redundant derivations.
pub struct ContextWorksetCache {
    entries: RwLock<HashMap<String, CachedWorksetEntry>>,
    ttl: Duration,
}

impl Default for ContextWorksetCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextWorksetCache {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            ttl: Duration::from_secs(30),
        }
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    pub fn get(&self, workspace_id: &str) -> Option<ContextWorkset> {
        if let Ok(guard) = self.entries.read() {
            if let Some(entry) = guard.get(workspace_id) {
                if entry.cached_at.elapsed() < self.ttl {
                    return Some(entry.workset.clone());
                }
            }
        }
        None
    }

    pub fn insert(&self, workspace_id: &str, workset: ContextWorkset) {
        if let Ok(mut guard) = self.entries.write() {
            if guard.len() >= 100 {
                guard.clear();
            }
            guard.insert(
                workspace_id.to_string(),
                CachedWorksetEntry {
                    workset,
                    cached_at: Instant::now(),
                },
            );
        }
    }

    pub fn invalidate(&self, workspace_id: Option<&str>) {
        if let Ok(mut guard) = self.entries.write() {
            if let Some(id) = workspace_id {
                guard.remove(id);
            } else {
                guard.clear();
            }
        }
    }
}

static GLOBAL_WORKSET_CACHE: OnceLock<ContextWorksetCache> = OnceLock::new();

pub fn get_global_workset_cache() -> &'static ContextWorksetCache {
    GLOBAL_WORKSET_CACHE.get_or_init(ContextWorksetCache::new)
}

pub fn invalidate_workset_cache(workspace_id: Option<&str>) {
    get_global_workset_cache().invalidate(workspace_id);
}

// =========================================================================
// ContextWorksetBuilder & Service
// =========================================================================

/// Derived Reconstruction Engine that builds a deterministic, stable `ContextWorkset`.
///
/// Invariants:
/// 1. Consumes existing Context Engine, Memory, Segments, and Relationships.
/// 2. Deterministic multi-factor relevance ranking across 10 signals.
/// 3. Stability & Hysteresis: Established working items carry inertia; weak noise cannot flap.
/// 4. Fast-track strong evidence; task & context transitions legitimately update focus.
/// 5. Read-only operation against SQLite database.
pub struct ContextWorksetBuilder {
    pub workspace_id: String,
    pub workspace: Option<Workspace>,
    pub memory: Option<ContextMemory>,
    pub items: Vec<Item>,
    pub suggested_memberships: Vec<ContextMembership>,
    pub dismissed_memberships: Vec<ContextMembership>,
    pub relationships: Vec<ItemRelationship>,
    pub segments: Vec<ContextActivitySegment>,
    pub transitions: Vec<ContextTransition>,
    pub user_feedback_signals: Vec<ContextSignal>,
    pub previous_workset: Option<ContextWorkset>,
    pub config: WorksetConfig,
    pub now: DateTime<Utc>,
}

impl ContextWorksetBuilder {
    pub fn new(workspace_id: impl Into<String>) -> Self {
        Self {
            workspace_id: workspace_id.into(),
            workspace: None,
            memory: None,
            items: Vec::new(),
            suggested_memberships: Vec::new(),
            dismissed_memberships: Vec::new(),
            relationships: Vec::new(),
            segments: Vec::new(),
            transitions: Vec::new(),
            user_feedback_signals: Vec::new(),
            previous_workset: None,
            config: WorksetConfig::default(),
            now: Utc::now(),
        }
    }

    pub fn with_workspace(mut self, ws: Workspace) -> Self {
        self.workspace_id = ws.id.clone();
        self.workspace = Some(ws);
        self
    }

    pub fn with_memory(mut self, mem: ContextMemory) -> Self {
        self.memory = Some(mem);
        self
    }

    pub fn with_items(mut self, items: Vec<Item>) -> Self {
        self.items = items;
        self
    }

    pub fn with_suggested_memberships(mut self, mems: Vec<ContextMembership>) -> Self {
        self.suggested_memberships = mems;
        self
    }

    pub fn with_dismissed_memberships(mut self, mems: Vec<ContextMembership>) -> Self {
        self.dismissed_memberships = mems;
        self
    }

    pub fn with_relationships(mut self, rels: Vec<ItemRelationship>) -> Self {
        self.relationships = rels;
        self
    }

    pub fn with_segments(mut self, segments: Vec<ContextActivitySegment>) -> Self {
        self.segments = segments;
        self
    }

    pub fn with_transitions(mut self, transitions: Vec<ContextTransition>) -> Self {
        self.transitions = transitions;
        self
    }

    pub fn with_user_feedback_signals(mut self, signals: Vec<ContextSignal>) -> Self {
        self.user_feedback_signals = signals;
        self
    }

    pub fn with_previous_workset(mut self, prev: ContextWorkset) -> Self {
        self.previous_workset = Some(prev);
        self
    }

    pub fn with_config(mut self, config: WorksetConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_stability_config(mut self, stability: WorksetStabilityConfig) -> Self {
        self.config.stability = stability;
        self
    }

    pub fn with_limits(mut self, min_items: usize, max_items: usize) -> Self {
        self.config.min_items = min_items;
        self.config.max_items = max_items;
        self
    }

    pub fn with_now(mut self, now: DateTime<Utc>) -> Self {
        self.now = now;
        self
    }

    /// Loads all required existing context, memory, and item evidence from the database.
    pub fn from_db(conn: &Connection, workspace_id: &str) -> Result<Self, String> {
        let ws = crate::db::operations::get_workspace(conn, workspace_id)
            .map_err(|e| format!("Workspace '{}' not found: {}", workspace_id, e))?;

        // 1. Derive ContextMemory (reusing Day 17 memory pipeline)
        let memory = ContextMemory::derive_for_workspace(conn, workspace_id)?;

        // 2. Fetch all confirmed items for this workspace
        let items = crate::db::operations::list_items(conn, Some(workspace_id))
            .unwrap_or_default();

        // 3. Fetch high-confidence suggested memberships from unfiled inbox items
        let suggested_memberships = crate::db::operations::list_workspace_memberships(
            conn,
            workspace_id,
            Some("suggested"),
        )
        .unwrap_or_default()
        .into_iter()
        .filter(|m| m.confidence >= 0.60)
        .collect();

        // 4. Fetch dismissed memberships for suppression
        let dismissed_memberships = crate::db::operations::list_workspace_memberships(
            conn,
            workspace_id,
            Some("dismissed"),
        )
        .unwrap_or_default();

        // 5. Fetch relationships involving any of these items
        let mut relationships = Vec::new();
        for item in &items {
            if let Ok(item_rels) = crate::db::operations::get_item_relationships(conn, &item.id) {
                relationships.extend(item_rels);
            }
        }
        relationships.dedup_by(|a, b| a.id == b.id);

        // 6. Fetch recent activity segments (bounded window)
        let segments = crate::context_engine::segments::ContextActivitySegmenter::new()
            .build_segments_for_workspace_bounded(conn, workspace_id, Some(30))
            .unwrap_or_default();

        // 7. Fetch recent context transitions
        let transitions = crate::db::operations::list_context_transitions_for_context(conn, workspace_id)
            .unwrap_or_default()
            .into_iter()
            .take(10)
            .collect();

        // 8. Fetch user feedback / action signals
        let mut user_feedback_signals = Vec::new();
        if let Ok(mut stmt) = conn.prepare(
            "SELECT id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at 
             FROM context_signals 
             WHERE (workspace_id = ?1 OR signal_type IN ('user_action', 'user_feedback')) 
             ORDER BY created_at DESC LIMIT 50"
        ) {
            if let Ok(rows) = stmt.query_map(params![workspace_id], |row| {
                Ok(ContextSignal {
                    id: row.get(0)?,
                    workspace_id: row.get(1)?,
                    item_id: row.get(2)?,
                    signal_type: row.get(3)?,
                    session_id: row.get(4)?,
                    payload: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            }) {
                for r in rows.flatten() {
                    user_feedback_signals.push(r);
                }
            }
        }

        // 9. Load temporal decay config
        let temporal_decay = crate::context_engine::temporal::get_temporal_decay_config(conn);

        // 10. Check previous workset from cache for seamless stability hysteresis
        let previous_workset = get_global_workset_cache().get(workspace_id);

        Ok(Self {
            workspace_id: workspace_id.to_string(),
            workspace: Some(ws),
            memory: Some(memory),
            items,
            suggested_memberships,
            dismissed_memberships,
            relationships,
            segments,
            transitions,
            user_feedback_signals,
            previous_workset,
            config: WorksetConfig {
                temporal_decay,
                ..Default::default()
            },
            now: Utc::now(),
        })
    }

    /// Builds and derives the ranked, stable `ContextWorkset`.
    pub fn build(self) -> Result<ContextWorkset, String> {
        let ws_name = self
            .workspace
            .as_ref()
            .map(|w| w.name.clone())
            .unwrap_or_else(|| self.workspace_id.clone());

        let memory = self.memory.unwrap_or_else(|| {
            ContextMemory::new(None, 0.0, &self.now.to_rfc3339())
        });

        let generated_at = self.now.to_rfc3339();
        let last_active_at = memory.last_active_at.clone();

        // Map dismissed items for suppression
        let dismissed_item_ids: HashSet<String> = self
            .dismissed_memberships
            .iter()
            .map(|m| m.item_id.clone())
            .collect();

        // Map previous workset membership and tiers for stability/hysteresis
        let mut prev_item_ids: HashSet<String> = HashSet::new();
        let mut prev_primary_ids: HashSet<String> = HashSet::new();
        let mut prev_supporting_ids: HashSet<String> = HashSet::new();
        let mut prev_item_scores: HashMap<String, f64> = HashMap::new();

        if let Some(ref prev) = self.previous_workset {
            for item in &prev.items {
                prev_item_ids.insert(item.id.clone());
                prev_item_scores.insert(item.id.clone(), item.relevance_score);
                match item.relevance {
                    WorksetItemRelevance::Primary => {
                        prev_primary_ids.insert(item.id.clone());
                    }
                    WorksetItemRelevance::Supporting => {
                        prev_supporting_ids.insert(item.id.clone());
                    }
                    _ => {}
                }
            }
        }

        // Detect legitimate task transitions vs continuity
        let prev_task_title = self.previous_workset.as_ref().and_then(|p| p.task_title.clone());
        let is_task_transition = prev_task_title.is_some()
            && memory.current_task_title.is_some()
            && prev_task_title != memory.current_task_title;

        // Count segment occurrences per item to identify repeated relevance across activity
        let mut segment_occurrences: HashMap<String, usize> = HashMap::new();
        let mut multi_session_item_ids: HashSet<String> = HashSet::new();
        let mut item_session_map: HashMap<String, HashSet<String>> = HashMap::new();
        let mut active_segment_item_ids: HashSet<String> = HashSet::new();

        if let Some(first_seg) = self.segments.first() {
            for ref_item in &first_seg.item_references {
                active_segment_item_ids.insert(ref_item.item_id.clone());
            }
        }

        for seg in &self.segments {
            for ref_item in &seg.item_references {
                *segment_occurrences.entry(ref_item.item_id.clone()).or_insert(0) += 1;
                for s_id in &seg.session_ids {
                    let set = item_session_map.entry(ref_item.item_id.clone()).or_default();
                    set.insert(s_id.clone());
                    if set.len() >= 2 {
                        multi_session_item_ids.insert(ref_item.item_id.clone());
                    }
                }
            }
        }

        // Identify items with positive user feedback / confirmation
        let mut confirmed_by_user_item_ids: HashSet<String> = HashSet::new();
        for sig in &self.user_feedback_signals {
            if let Some(ref i_id) = sig.item_id {
                if sig.signal_type == "user_action" || sig.signal_type == "user_feedback" {
                    confirmed_by_user_item_ids.insert(i_id.clone());
                }
            }
        }

        // Active task properties
        let task_id = memory.current_task_id.clone();
        let task_title = memory.current_task_title.clone();
        let task_item_ids: HashSet<String> = memory
            .current_task
            .item_ids
            .iter()
            .cloned()
            .collect();

        let task_subject = memory
            .current_task
            .target_subject
            .as_deref()
            .unwrap_or("")
            .trim();

        let task_subject_lower = task_subject.to_lowercase();
        let task_subject_terms: Vec<String> = task_subject_lower
            .split_whitespace()
            .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
            .filter(|s| s.len() >= 3)
            .collect();

        let task_title_lower = task_title.as_deref().unwrap_or("").to_lowercase();
        let task_title_terms: Vec<String> = task_title_lower
            .split_whitespace()
            .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
            .filter(|s| s.len() >= 3)
            .collect();

        // Graph relationships map: item_id -> list of (other_id, rel_type, strength)
        let mut rel_map: HashMap<String, Vec<(String, String, f64)>> = HashMap::new();
        for rel in &self.relationships {
            rel_map
                .entry(rel.item_a_id.clone())
                .or_default()
                .push((rel.item_b_id.clone(), rel.relationship_type.clone(), rel.strength));
            rel_map
                .entry(rel.item_b_id.clone())
                .or_default()
                .push((rel.item_a_id.clone(), rel.relationship_type.clone(), rel.strength));
        }

        let mut candidate_items: Vec<WorksetItemReference> = Vec::new();

        // ---------------------------------------------------------------------
        // 1. Process Confirmed Context Items
        // ---------------------------------------------------------------------
        for item in &self.items {
            if dismissed_item_ids.contains(&item.id) {
                continue;
            }

            let mut app_name = None;
            let mut domain = None;

            if let Ok(meta) = serde_json::from_str::<Value>(&item.metadata) {
                if let Some(app) = meta
                    .get("app_name")
                    .and_then(|v| v.as_str())
                    .and_then(normalize_app_name)
                {
                    app_name = Some(app);
                }
                if let Some(d) = meta.get("domain").and_then(|v| v.as_str()) {
                    domain = Some(d.to_string());
                } else if let Some(url_str) = meta.get("original_url").and_then(|v| v.as_str()) {
                    domain = extract_domain_from_url(url_str);
                }
            }

            if domain.is_none() && item.item_type == "url" {
                domain = extract_domain_from_url(&item.content);
            }

            let title_lower = item.title.as_deref().unwrap_or("").to_lowercase();
            let content_lower = item.content.to_lowercase();

            // Task relevance evaluation
            let is_exact_task_item = task_item_ids.contains(&item.id);
            let is_subject_match = !task_subject_lower.is_empty()
                && (title_lower.contains(&task_subject_lower)
                    || (task_subject_lower.len() >= 6 && content_lower.contains(&task_subject_lower)));
            let is_title_terms_match = !task_title_terms.is_empty()
                && task_title_terms.iter().any(|t| title_lower.contains(t));
            let is_subject_terms_match = !task_subject_terms.is_empty()
                && task_subject_terms.iter().any(|t| title_lower.contains(t));

            // Relational connection to task & active graph
            let mut max_task_rel_strength: f64 = 0.0;
            let mut max_semantic_sim: f64 = 0.0;
            let mut total_rel_strength: f64 = 0.0;
            let mut rel_count = 0;

            if let Some(edges) = rel_map.get(&item.id) {
                for (other_id, r_type, strength) in edges {
                    total_rel_strength += strength;
                    rel_count += 1;
                    if task_item_ids.contains(other_id) {
                        max_task_rel_strength = max_task_rel_strength.max(*strength);
                    }
                    if r_type == "semantic_similarity" {
                        max_semantic_sim = max_semantic_sim.max(*strength);
                    }
                }
            }

            let avg_rel_strength = if rel_count > 0 {
                (total_rel_strength / (rel_count as f64)).min(1.0)
            } else {
                0.0
            };

            // Multi-Factor Signals:
            // 1. Task Relevance (0.0 to 0.40)
            let mut s_task = 0.0;
            let mut is_strongly_task_related = false;
            let mut is_supporting_task_related = false;

            if is_exact_task_item {
                s_task += 0.40;
                is_strongly_task_related = true;
            } else if task_item_ids.is_empty() && is_subject_match {
                s_task += 0.35;
                is_strongly_task_related = true;
            } else if max_task_rel_strength >= 0.70 || (is_subject_match && !is_exact_task_item) {
                s_task += 0.25;
                is_supporting_task_related = true;
            } else if is_subject_terms_match || is_title_terms_match || max_task_rel_strength >= 0.40 {
                s_task += 0.15;
                if item.item_type != "image" || max_task_rel_strength >= 0.40 {
                    is_supporting_task_related = true;
                }
            }

            // 2. Context Relevance (0.25 base confirmed)
            let mut s_context = 0.25;
            let is_repo_or_docs = item.item_type == "url"
                && (item.content.contains("github.com")
                    || item.content.contains("gitlab.com")
                    || item.content.contains("docs."));
            if is_repo_or_docs {
                s_context += 0.15;
            }

            // 3. Recency & Temporal Decay
            let timestamp_to_use = if !item.updated_at.is_empty() {
                &item.updated_at
            } else {
                &item.created_at
            };
            let temporal_weight = self.config.temporal_decay.compute_weight(timestamp_to_use, self.now);
            let s_recency = 0.25 * temporal_weight;

            let is_recently_active = active_segment_item_ids.contains(&item.id)
                || memory.recent_items.iter().any(|m| m.id == item.id)
                || temporal_weight >= 0.95;

            // 4. Confirmed Membership (Confirmed priority)
            let s_membership = 0.15;

            // 5. Semantic Similarity
            let s_sem = 0.15 * max_semantic_sim;

            // 6. Relationship Strength
            let s_rel = 0.15 * avg_rel_strength;

            // 7. Repeated Relevance Across Activity
            let seg_count = *segment_occurrences.get(&item.id).unwrap_or(&0);
            let is_multi_session = multi_session_item_ids.contains(&item.id);
            let is_repeatedly_referenced = seg_count >= 2 || is_multi_session || is_repo_or_docs;
            let s_repeat = if is_repeatedly_referenced { 0.20 } else { 0.0 };

            // 8. Recent Activity Burst
            let s_active = if is_recently_active { 0.15 } else { 0.0 };

            // 9. User Feedback
            let is_user_confirmed = confirmed_by_user_item_ids.contains(&item.id);
            let s_feedback = if is_user_confirmed { 0.25 } else { 0.0 };

            // 10. Content Usefulness
            let is_empty = item.content.trim().is_empty();
            let s_usefulness = if is_empty { -0.20 } else { 0.05 };

            // -----------------------------------------------------------------
            // Stability & Hysteresis Rules
            // -----------------------------------------------------------------
            let is_prev_member = prev_item_ids.contains(&item.id);
            let is_prev_primary = prev_primary_ids.contains(&item.id);

            // Stability inertia bonus for established members when not undergoing task shift
            let s_stability_inertia = if is_prev_member && !is_task_transition {
                let mut inertia = self.config.stability.previous_membership_inertia;
                if is_prev_primary {
                    inertia += 0.05;
                }
                inertia
            } else if is_prev_member && is_task_transition {
                // Modest inertia on task shift to allow graceful demotion rather than vanishing
                0.05
            } else {
                0.0
            };

            let mut composite_score = s_task
                + s_context
                + s_recency
                + s_membership
                + s_sem
                + s_rel
                + s_repeat
                + s_active
                + s_feedback
                + s_usefulness
                + s_stability_inertia;

            // Weak new evidence damping (Rule 3 & 4):
            // If item is new and score < suppression threshold and not repeated, dampen slightly
            if !is_prev_member
                && composite_score < self.config.stability.weak_item_suppression_threshold
                && !is_repeatedly_referenced
                && max_task_rel_strength < 0.40
            {
                composite_score *= 0.85;
            }

            composite_score = composite_score.clamp(0.0, 1.0);

            // Construct Natural-Language Explainable Reasons
            let mut reasons: Vec<String> = Vec::new();

            if is_strongly_task_related {
                reasons.push("strongly related to current task".to_string());
            } else if is_supporting_task_related {
                reasons.push("supporting material for current task".to_string());
            }

            if is_recently_active {
                reasons.push("recently active".to_string());
            } else if item.item_type == "image"
                && (title_lower.contains("latest") || title_lower.contains("capture"))
            {
                reasons.push("recently captured".to_string());
            }

            if is_user_confirmed {
                reasons.push("confirmed by user feedback".to_string());
            }

            if is_repeatedly_referenced {
                reasons.push("repeatedly referenced".to_string());
            }

            if is_prev_member && !is_task_transition && is_recently_active {
                reasons.push("previously active in working set".to_string());
            }

            if s_context >= 0.35 || is_repo_or_docs {
                reasons.push("strongly related to context".to_string());
            } else {
                reasons.push("confirmed context".to_string());
            }

            if max_semantic_sim >= 0.75 {
                reasons.push("strong semantic similarity to active work".to_string());
            }

            if avg_rel_strength >= 0.60 {
                reasons.push("closely linked to other context items".to_string());
            }

            reasons.dedup();

            // Determine Item Relevance Tier
            let relevance = if is_exact_task_item || (task_item_ids.is_empty() && is_strongly_task_related) {
                WorksetItemRelevance::Primary
            } else if is_supporting_task_related || max_task_rel_strength >= 0.40 || max_semantic_sim >= 0.70 {
                WorksetItemRelevance::Supporting
            } else if is_repo_or_docs || is_repeatedly_referenced {
                WorksetItemRelevance::Reference
            } else if is_recently_active {
                WorksetItemRelevance::Recent
            } else {
                WorksetItemRelevance::Reference
            };

            let snippet = create_content_snippet(&item.content, &item.item_type);

            candidate_items.push(WorksetItemReference {
                id: item.id.clone(),
                title: item.title.clone(),
                item_type: item.item_type.clone(),
                relevance,
                relevance_score: composite_score,
                reasons,
                provenance: InformationProvenance::Confirmed,
                created_at: item.created_at.clone(),
                last_active_at: Some(item.updated_at.clone()),
                app_name,
                domain,
                content_snippet: snippet,
                is_suggested_addition: false,
            });
        }

        // ---------------------------------------------------------------------
        // 2. Process Suggested Additions from Unfiled Inbox Items
        // ---------------------------------------------------------------------
        if self.config.include_suggested_inbox {
            for mem in &self.suggested_memberships {
                if mem.confidence < self.config.suggested_min_confidence {
                    continue;
                }
                if dismissed_item_ids.contains(&mem.item_id) {
                    continue;
                }

                // Inferred items start with discount factor so confirmed outranks inferred
                let composite_score = (mem.confidence * 0.70).clamp(0.0, 0.80);
                let reasons = vec![
                    "suggested addition from inbox".to_string(),
                    "inferred context affinity".to_string(),
                ];

                candidate_items.push(WorksetItemReference {
                    id: mem.item_id.clone(),
                    title: Some("Inbox Item (suggested)".to_string()),
                    item_type: "text".to_string(),
                    relevance: WorksetItemRelevance::Related,
                    relevance_score: composite_score,
                    reasons,
                    provenance: InformationProvenance::Inferred,
                    created_at: mem.created_at.clone(),
                    last_active_at: Some(mem.updated_at.clone()),
                    app_name: None,
                    domain: None,
                    content_snippet: format!(
                        "Suggested from Inbox ({:.0}% confidence)",
                        mem.confidence * 100.0
                    ),
                    is_suggested_addition: true,
                });
            }
        }

        // ---------------------------------------------------------------------
        // 3. Deterministic Sorting, Primary Promotion & Bounded Selection
        // ---------------------------------------------------------------------
        candidate_items.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Ensure at least 1 Primary item exists if we have items
        if !candidate_items.is_empty()
            && !candidate_items
                .iter()
                .any(|i| i.relevance == WorksetItemRelevance::Primary)
        {
            if let Some(top_item) = candidate_items.iter_mut().max_by(|a, b| {
                a.relevance_score
                    .partial_cmp(&b.relevance_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) {
                top_item.relevance = WorksetItemRelevance::Primary;
                if !top_item
                    .reasons
                    .iter()
                    .any(|r| r.contains("primary") || r.contains("current task"))
                {
                    top_item
                        .reasons
                        .insert(0, "strongly related to current task".to_string());
                }
            }
        }

        // Avoid over-selection:
        // Filter out candidates with low relevance (< 0.25) so unrelated items are excluded
        let mut final_items: Vec<WorksetItemReference> = candidate_items
            .into_iter()
            .filter(|i| i.relevance_score >= 0.25)
            .collect();

        // Enforce maximum workset capacity limit (default: 8 items) with displacement protection
        if final_items.len() > self.config.max_items {
            final_items.truncate(self.config.max_items);
        }

        // Partition into categorized sub-lists
        let relevant_items: Vec<WorksetItemReference> = final_items
            .iter()
            .filter(|i| {
                matches!(
                    i.relevance,
                    WorksetItemRelevance::Primary | WorksetItemRelevance::Supporting
                )
            })
            .cloned()
            .collect();

        let important_references: Vec<WorksetItemReference> = final_items
            .iter()
            .filter(|i| i.relevance == WorksetItemRelevance::Reference)
            .cloned()
            .collect();

        // ---------------------------------------------------------------------
        // 4. Determine WorksetState & Metadata
        // ---------------------------------------------------------------------
        let workset_state = if memory.recent_context_transition.as_ref().map_or(false, |t| {
            t.transition_type == "context_return" || t.transition_type == "return"
        }) || self.transitions.first().map_or(false, |t| {
            t.transition_type == crate::db::models::ContextTransitionType::ContextReturn
        }) {
            WorksetState::DetourReturn
        } else {
            match memory.activity_state {
                crate::context_engine::memory::ContextMemoryActivityState::Active => {
                    WorksetState::Active
                }
                crate::context_engine::memory::ContextMemoryActivityState::RecentlyActive => {
                    WorksetState::RecentlyActive
                }
                crate::context_engine::memory::ContextMemoryActivityState::Dormant => {
                    WorksetState::Dormant
                }
                crate::context_engine::memory::ContextMemoryActivityState::InsufficientEvidence => {
                    if self.items.is_empty() {
                        WorksetState::InsufficientEvidence
                    } else {
                        WorksetState::RecentlyActive
                    }
                }
            }
        };

        let confidence = memory.current_context_confidence.max(0.10);
        let confidence_band = memory.context_confidence_band.clone();

        let explanation = memory.explanation.clone();
        let related_tasks = memory.recent_tasks.clone();
        let recent_activity = memory.recent_activity.clone();

        Ok(ContextWorkset {
            context_id: self.workspace_id,
            context_name: ws_name,
            task_id,
            task_title,
            generated_at,
            last_active_at,
            workset_state,
            items: final_items,
            recent_activity,
            relevant_items,
            important_references,
            related_tasks,
            confidence,
            confidence_band,
            provenance: memory.context_provenance,
            explanation,
        })
    }
}

/// Service interface for deriving and managing `ContextWorkset` instances.
pub struct ContextWorksetService;

impl ContextWorksetService {
    pub fn new() -> Self {
        Self
    }

    /// Derives the contextual workset for a specific workspace from the database.
    pub fn derive_for_workspace(
        &self,
        conn: &Connection,
        workspace_id: &str,
    ) -> Result<ContextWorkset, String> {
        if let Some(cached) = get_global_workset_cache().get(workspace_id) {
            return Ok(cached);
        }

        let workset = ContextWorksetBuilder::from_db(conn, workspace_id)?.build()?;
        get_global_workset_cache().insert(workspace_id, workset.clone());
        Ok(workset)
    }

    /// Derives the active contextual workset across the entire system.
    pub fn derive_active(&self, conn: &Connection) -> Result<Option<ContextWorkset>, String> {
        let mut stmt = conn
            .prepare(
                "SELECT id FROM workspaces 
                 WHERE lifecycle_state != 'archived' 
                 ORDER BY COALESCE(last_activity_at, updated_at) DESC LIMIT 1",
            )
            .map_err(|e| e.to_string())?;

        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let ws_id: String = row.get(0).map_err(|e| e.to_string())?;
            self.derive_for_workspace(conn, &ws_id).map(Some)
        } else {
            Ok(None)
        }
    }
}

// =========================================================================
// Unit Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use crate::context_engine::memory::InferredTaskState;
    use crate::context_engine::segments::SegmentItemReference;
    use crate::db::operations::{
        create_item, create_item_relationship, create_or_update_membership, create_workspace,
    };

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_empty_workspace_workset() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Empty Workspace").unwrap();

        let service = ContextWorksetService::new();
        let workset = service.derive_for_workspace(&conn, &ws.id).unwrap();

        assert_eq!(workset.context_id, ws.id);
        assert_eq!(workset.context_name, "Empty Workspace");
        assert!(workset.items.is_empty());
        assert!(workset.relevant_items.is_empty());
        assert!(workset.important_references.is_empty());
        assert_eq!(workset.workset_state, WorksetState::InsufficientEvidence);
    }

    #[test]
    fn test_aurora_website_hero_animation_workset() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Aurora Website").unwrap();

        // Create items matching prompt's example:
        // 1. Primary: React Hero component
        let item_hero = create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "export const Hero = () => { return <div className='hero-animation'>...</div>; }",
            Some("React Hero component"),
            r#"{"app_name": "VS Code"}"#,
        )
        .unwrap();

        // 2. Supporting: Framer Motion animation reference
        let item_framer = create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://framer.com/motion/animation",
            Some("Framer Motion animation reference"),
            r#"{"domain": "framer.com"}"#,
        )
        .unwrap();

        // 3. Supporting: Figma hero design
        let item_figma = create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://figma.com/file/hero-design",
            Some("Figma hero design"),
            r#"{"domain": "figma.com"}"#,
        )
        .unwrap();

        // 4. Reference: GitHub repository
        let item_github = create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://github.com/aurora/website",
            Some("GitHub repository"),
            r#"{"domain": "github.com"}"#,
        )
        .unwrap();

        // 5. Recent: latest captured animation example
        let item_anim = create_item(
            &conn,
            Some(&ws.id),
            None,
            "image",
            "animation_capture.png",
            Some("latest captured animation example"),
            r#"{"app_name": "CleanShot X"}"#,
        )
        .unwrap();

        // Create relationships linking items to hero component
        create_item_relationship(
            &conn,
            &item_hero.id,
            &item_framer.id,
            "semantic_similarity",
            0.88,
            None,
        )
        .unwrap();

        create_item_relationship(
            &conn,
            &item_hero.id,
            &item_figma.id,
            "temporal_proximity",
            0.75,
            None,
        )
        .unwrap();

        // Construct memory with active task "Hero animation"
        let active_task = InferredTaskState::new(
            "Hero animation",
            Some("Designing".to_string()),
            Some("Hero animation".to_string()),
            0.92,
            vec!["Active VS Code session in Hero component".to_string()],
            &Utc::now().to_rfc3339(),
            Some(ws.id.clone()),
        );

        let mut memory = ContextMemory::new(None, 0.90, &Utc::now().to_rfc3339())
            .with_task(active_task);
        memory.current_task.item_ids = vec![item_hero.id.clone()];

        let builder = ContextWorksetBuilder::from_db(&conn, &ws.id)
            .unwrap()
            .with_memory(memory);

        let workset = builder.build().unwrap();

        assert_eq!(workset.context_name, "Aurora Website");
        assert_eq!(workset.task_title.as_deref(), Some("Hero animation"));
        assert_eq!(workset.items.len(), 5);

        // Verify primary item and explainable reasons
        let primary = workset.primary_items();
        assert_eq!(primary.len(), 1);
        assert_eq!(primary[0].id, item_hero.id);
        assert_eq!(primary[0].relevance, WorksetItemRelevance::Primary);
        assert!(primary[0].reasons.iter().any(|r| r == "strongly related to current task"));
        assert!(primary[0].reasons.iter().any(|r| r == "recently active"));
        assert!(primary[0].reasons.iter().any(|r| r == "confirmed context"));

        // Verify supporting items
        let supporting = workset.supporting_items();
        assert_eq!(supporting.len(), 2);
        assert!(supporting.iter().any(|i| i.id == item_framer.id));
        assert!(supporting.iter().any(|i| i.id == item_figma.id));

        // Verify reference item and explainable reasons
        let reference = &workset.important_references;
        assert_eq!(reference.len(), 1);
        assert_eq!(reference[0].id, item_github.id);
        assert!(reference[0].reasons.iter().any(|r| r == "strongly related to context"));
        assert!(reference[0].reasons.iter().any(|r| r == "repeatedly referenced"));

        // Verify recent item
        let recent = workset.recent_items();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].id, item_anim.id);

        // Verify relevant items contains primary + supporting
        assert_eq!(workset.relevant_items.len(), 3);
        assert!(workset.relevant_items.iter().any(|i| i.id == item_hero.id));
    }

    #[test]
    fn test_confirmed_outranks_inferred_membership() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Ranking Priority").unwrap();

        // Confirmed item with medium activity
        let confirmed_item = create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Confirmed project specification",
            Some("Project Spec"),
            "{}",
        )
        .unwrap();

        // Inferred item with high confidence
        let inbox_item = create_item(
            &conn,
            None,
            None,
            "text",
            "Inbox item inferred with high engine confidence",
            Some("Inbox Note"),
            "{}",
        )
        .unwrap();

        create_or_update_membership(
            &conn,
            &inbox_item.id,
            &ws.id,
            "suggested",
            0.88,
            "context_engine",
        )
        .unwrap();

        let builder = ContextWorksetBuilder::from_db(&conn, &ws.id).unwrap();
        let workset = builder.build().unwrap();

        assert_eq!(workset.items.len(), 2);
        assert_eq!(workset.items[0].id, confirmed_item.id);
        assert_eq!(workset.items[0].provenance, InformationProvenance::Confirmed);
        assert_eq!(workset.items[1].id, inbox_item.id);
        assert_eq!(workset.items[1].provenance, InformationProvenance::Inferred);
    }

    #[test]
    fn test_old_relevant_item_retained_despite_age() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Legacy Project").unwrap();

        // Old item created 90 days ago, but is a core GitHub repo
        let now = Utc::now();
        let old_time = (now - Duration::days(90)).to_rfc3339();

        let mut old_repo = create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://github.com/org/core-architecture",
            Some("Core Architecture Repository"),
            r#"{"domain": "github.com"}"#,
        )
        .unwrap();

        conn.execute(
            "UPDATE items SET created_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![old_time, old_repo.id],
        )
        .unwrap();

        old_repo.created_at = old_time.clone();
        old_repo.updated_at = old_time;

        let builder = ContextWorksetBuilder::from_db(&conn, &ws.id).unwrap();
        let workset = builder.build().unwrap();

        assert_eq!(workset.items.len(), 1);
        assert_eq!(workset.items[0].id, old_repo.id);
        assert_eq!(workset.items[0].relevance, WorksetItemRelevance::Primary);
        assert!(workset.items[0].reasons.iter().any(|r| r == "repeatedly referenced" || r == "strongly related to context"));
    }

    #[test]
    fn test_recent_unrelated_item_not_in_primary_working_set() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Design System").unwrap();

        // 1. Confirmed core task item
        let task_item = create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Button component tokens",
            Some("Button Tokens"),
            "{}",
        )
        .unwrap();

        // 2. Empty / unrelated item
        let empty_item = create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "   ",
            Some("Random Scrap"),
            "{}",
        )
        .unwrap();

        let active_task = InferredTaskState::new(
            "Button tokens",
            Some("Design".to_string()),
            Some("Button".to_string()),
            0.90,
            vec![],
            &Utc::now().to_rfc3339(),
            Some(ws.id.clone()),
        );
        let mut memory = ContextMemory::new(None, 0.90, &Utc::now().to_rfc3339())
            .with_task(active_task);
        memory.current_task.item_ids = vec![task_item.id.clone()];

        let builder = ContextWorksetBuilder::from_db(&conn, &ws.id)
            .unwrap()
            .with_memory(memory);
        let workset = builder.build().unwrap();

        // Primary working item must be the task item, not the empty scrap
        assert_eq!(workset.primary_items()[0].id, task_item.id);
        assert_ne!(workset.primary_items()[0].id, empty_item.id);
    }

    #[test]
    fn test_working_set_configurable_limits() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Limit Test").unwrap();

        // Create 10 items
        for i in 1..=10 {
            create_item(
                &conn,
                Some(&ws.id),
                None,
                "text",
                &format!("Project note {}", i),
                Some(&format!("Note {}", i)),
                "{}",
            )
            .unwrap();
        }

        // Test default limit (max 8)
        let builder_default = ContextWorksetBuilder::from_db(&conn, &ws.id).unwrap();
        let workset_default = builder_default.build().unwrap();
        assert_eq!(workset_default.items.len(), 8);

        // Test custom limits (min 2, max 4)
        let builder_custom = ContextWorksetBuilder::from_db(&conn, &ws.id)
            .unwrap()
            .with_limits(2, 4);
        let workset_custom = builder_custom.build().unwrap();
        assert_eq!(workset_custom.items.len(), 4);
    }

    #[test]
    fn test_user_feedback_boost_and_dismissal_suppression() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Feedback Test").unwrap();

        let item_normal = create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Normal item",
            Some("Normal"),
            "{}",
        )
        .unwrap();

        let item_dismissed = create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Dismissed item",
            Some("Dismissed"),
            "{}",
        )
        .unwrap();

        // Mark dismissal
        create_or_update_membership(
            &conn,
            &item_dismissed.id,
            &ws.id,
            "dismissed",
            0.0,
            "user",
        )
        .unwrap();

        // Record user action for normal item
        conn.execute(
            "INSERT INTO context_signals (id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                "sig-1",
                ws.id,
                item_normal.id,
                "user_action",
                "sess-1",
                "{}",
                Utc::now().to_rfc3339(),
                Utc::now().to_rfc3339()
            ],
        )
        .unwrap();

        let builder = ContextWorksetBuilder::from_db(&conn, &ws.id).unwrap();
        let workset = builder.build().unwrap();

        // Dismissed item must be completely suppressed
        assert!(!workset.items.iter().any(|i| i.id == item_dismissed.id));

        // Normal item boosted by user feedback
        let confirmed = workset.items.iter().find(|i| i.id == item_normal.id).unwrap();
        assert!(confirmed.reasons.iter().any(|r| r == "confirmed by user feedback"));
    }

    #[test]
    fn test_repeated_relevance_across_segments() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Segment Continuity Test").unwrap();

        let recurring_item = create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Shared API Client definition",
            Some("API Client"),
            "{}",
        )
        .unwrap();

        // Create 2 mock segments with this item
        let seg1 = ContextActivitySegment {
            id: "seg-1".to_string(),
            context: None,
            started_at: Utc::now().to_rfc3339(),
            ended_at: Utc::now().to_rfc3339(),
            duration_seconds: 600,
            duration_formatted: "10m".to_string(),
            confidence: 0.9,
            session_ids: vec!["sess-1".to_string()],
            item_references: vec![SegmentItemReference {
                item_id: recurring_item.id.clone(),
                title: recurring_item.title.clone(),
                item_type: recurring_item.item_type.clone(),
                timestamp: Utc::now().to_rfc3339(),
                confidence: 0.9,
            }],
            transition_in: None,
            transition_out: None,
            is_ambiguous: false,
            item_count: 1,
            metadata: serde_json::json!({}),
        };

        let seg2 = ContextActivitySegment {
            id: "seg-2".to_string(),
            context: None,
            started_at: Utc::now().to_rfc3339(),
            ended_at: Utc::now().to_rfc3339(),
            duration_seconds: 900,
            duration_formatted: "15m".to_string(),
            confidence: 0.9,
            session_ids: vec!["sess-2".to_string()],
            item_references: vec![SegmentItemReference {
                item_id: recurring_item.id.clone(),
                title: recurring_item.title.clone(),
                item_type: recurring_item.item_type.clone(),
                timestamp: Utc::now().to_rfc3339(),
                confidence: 0.9,
            }],
            transition_in: None,
            transition_out: None,
            is_ambiguous: false,
            item_count: 1,
            metadata: serde_json::json!({}),
        };

        let builder = ContextWorksetBuilder::from_db(&conn, &ws.id)
            .unwrap()
            .with_segments(vec![seg1, seg2]);
        let workset = builder.build().unwrap();

        assert_eq!(workset.items.len(), 1);
        assert!(workset.items[0].reasons.iter().any(|r| r == "repeatedly referenced"));
    }

    // =========================================================================
    // Stability Tests (Step 4)
    // =========================================================================

    #[test]
    fn test_stability_strong_new_evidence_enters_fast() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Stability Test").unwrap();

        // Established item 1
        let _item1 = create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Existing core component code",
            Some("Core Component"),
            "{}",
        )
        .unwrap();

        let prev_builder = ContextWorksetBuilder::from_db(&conn, &ws.id).unwrap();
        let prev_workset = prev_builder.build().unwrap();
        assert_eq!(prev_workset.items.len(), 1);

        // A new strong item arrives directly matching the active task
        let item2 = create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Urgent bug fix in Core Component",
            Some("Urgent Core Component Fix"),
            "{}",
        )
        .unwrap();

        let active_task = InferredTaskState::new(
            "Core Component",
            Some("Fixing".to_string()),
            Some("Core Component".to_string()),
            0.95,
            vec![],
            &Utc::now().to_rfc3339(),
            Some(ws.id.clone()),
        );
        let memory = ContextMemory::new(None, 0.95, &Utc::now().to_rfc3339()).with_task(active_task);

        let new_workset = ContextWorksetBuilder::from_db(&conn, &ws.id)
            .unwrap()
            .with_memory(memory)
            .with_previous_workset(prev_workset)
            .build()
            .unwrap();

        // Strong new item enters quickly without being blocked by previous state
        assert_eq!(new_workset.items.len(), 2);
        assert!(new_workset.items.iter().any(|i| i.id == item2.id));
        let entrant = new_workset.items.iter().find(|i| i.id == item2.id).unwrap();
        assert_eq!(entrant.relevance, WorksetItemRelevance::Primary);
    }

    #[test]
    fn test_stability_unrelated_item_does_not_displace_existing() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Stability Test 2").unwrap();

        // Fill workspace with 8 established strong items
        for i in 1..=8 {
            create_item(
                &conn,
                Some(&ws.id),
                None,
                "text",
                &format!("Core feature specification {}", i),
                Some(&format!("Spec {}", i)),
                "{}",
            )
            .unwrap();
        }

        let prev_workset = ContextWorksetBuilder::from_db(&conn, &ws.id)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(prev_workset.items.len(), 8);

        // A single random weak capture arrives (e.g. quick note)
        let weak_item = create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "random unorganized quick snippet",
            Some("Quick Snippet"),
            "{}",
        )
        .unwrap();

        let new_workset = ContextWorksetBuilder::from_db(&conn, &ws.id)
            .unwrap()
            .with_previous_workset(prev_workset.clone())
            .build()
            .unwrap();

        // The workset cap (8 items) is preserved and the random weak capture does NOT displace established members
        assert_eq!(new_workset.items.len(), 8);
        assert!(!new_workset.items.iter().any(|i| i.id == weak_item.id));
    }

    #[test]
    fn test_stability_repeated_weak_evidence_can_enter() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Stability Test 3").unwrap();

        // Fill workspace with 5 items
        for i in 1..=5 {
            create_item(
                &conn,
                Some(&ws.id),
                None,
                "text",
                &format!("Existing spec {}", i),
                Some(&format!("Spec {}", i)),
                "{}",
            )
            .unwrap();
        }

        let prev_workset = ContextWorksetBuilder::from_db(&conn, &ws.id)
            .unwrap()
            .build()
            .unwrap();

        // Weak item appears in multiple activity segments
        let persistent_item = create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Recurring utility reference",
            Some("Utility Ref"),
            "{}",
        )
        .unwrap();

        let seg1 = ContextActivitySegment {
            id: "seg-1".to_string(),
            context: None,
            started_at: Utc::now().to_rfc3339(),
            ended_at: Utc::now().to_rfc3339(),
            duration_seconds: 300,
            duration_formatted: "5m".to_string(),
            confidence: 0.8,
            session_ids: vec!["s-1".to_string()],
            item_references: vec![SegmentItemReference {
                item_id: persistent_item.id.clone(),
                title: persistent_item.title.clone(),
                item_type: persistent_item.item_type.clone(),
                timestamp: Utc::now().to_rfc3339(),
                confidence: 0.8,
            }],
            transition_in: None,
            transition_out: None,
            is_ambiguous: false,
            item_count: 1,
            metadata: serde_json::json!({}),
        };

        let seg2 = ContextActivitySegment {
            id: "seg-2".to_string(),
            context: None,
            started_at: Utc::now().to_rfc3339(),
            ended_at: Utc::now().to_rfc3339(),
            duration_seconds: 300,
            duration_formatted: "5m".to_string(),
            confidence: 0.8,
            session_ids: vec!["s-2".to_string()],
            item_references: vec![SegmentItemReference {
                item_id: persistent_item.id.clone(),
                title: persistent_item.title.clone(),
                item_type: persistent_item.item_type.clone(),
                timestamp: Utc::now().to_rfc3339(),
                confidence: 0.8,
            }],
            transition_in: None,
            transition_out: None,
            is_ambiguous: false,
            item_count: 1,
            metadata: serde_json::json!({}),
        };

        let new_workset = ContextWorksetBuilder::from_db(&conn, &ws.id)
            .unwrap()
            .with_segments(vec![seg1, seg2])
            .with_previous_workset(prev_workset)
            .build()
            .unwrap();

        // Repeated weak evidence accumulates persistence and successfully enters the working set
        assert!(new_workset.items.iter().any(|i| i.id == persistent_item.id));
    }

    #[test]
    fn test_stability_task_switch_legitimately_evolves_workset() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Multi-Task Project").unwrap();

        let item_hero = create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Hero component styles and animations",
            Some("Hero Animation"),
            "{}",
        )
        .unwrap();

        let item_db = create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Database schema migrations for PostgreSQL",
            Some("Database Migration"),
            "{}",
        )
        .unwrap();

        // Previous state: User was working on Hero Animation
        let task_hero = InferredTaskState::new(
            "Hero animation",
            Some("Designing".to_string()),
            Some("Hero animation".to_string()),
            0.92,
            vec![],
            &Utc::now().to_rfc3339(),
            Some(ws.id.clone()),
        );
        let mem_hero = ContextMemory::new(None, 0.90, &Utc::now().to_rfc3339()).with_task(task_hero);
        let prev_workset = ContextWorksetBuilder::from_db(&conn, &ws.id)
            .unwrap()
            .with_memory(mem_hero)
            .build()
            .unwrap();

        assert_eq!(prev_workset.primary_items()[0].id, item_hero.id);

        // Task Transition: User shifts focus to Database Migration
        let task_db = InferredTaskState::new(
            "Database Migration",
            Some("Migrating".to_string()),
            Some("Database Migration".to_string()),
            0.94,
            vec![],
            &Utc::now().to_rfc3339(),
            Some(ws.id.clone()),
        );
        let mem_db = ContextMemory::new(None, 0.92, &Utc::now().to_rfc3339()).with_task(task_db);

        let new_workset = ContextWorksetBuilder::from_db(&conn, &ws.id)
            .unwrap()
            .with_memory(mem_db)
            .with_previous_workset(prev_workset)
            .build()
            .unwrap();

        // Database Migration item becomes Primary
        assert_eq!(new_workset.primary_items()[0].id, item_db.id);
        // Hero item gracefully demoted to Reference or Recent
        let hero_item = new_workset.items.iter().find(|i| i.id == item_hero.id).unwrap();
        assert_ne!(hero_item.relevance, WorksetItemRelevance::Primary);
    }

    #[test]
    fn test_stability_context_switch_isolates_workset() {
        let conn = setup_test_db();
        let ws_a = create_workspace(&conn, "Context A").unwrap();
        let ws_b = create_workspace(&conn, "Context B").unwrap();

        let item_a = create_item(
            &conn,
            Some(&ws_a.id),
            None,
            "text",
            "Code for Context A",
            Some("A Item"),
            "{}",
        )
        .unwrap();

        let item_b = create_item(
            &conn,
            Some(&ws_b.id),
            None,
            "text",
            "Code for Context B",
            Some("B Item"),
            "{}",
        )
        .unwrap();

        let service = ContextWorksetService::new();
        let workset_a = service.derive_for_workspace(&conn, &ws_a.id).unwrap();
        let workset_b = service.derive_for_workspace(&conn, &ws_b.id).unwrap();

        assert_eq!(workset_a.items.len(), 1);
        assert_eq!(workset_a.items[0].id, item_a.id);

        assert_eq!(workset_b.items.len(), 1);
        assert_eq!(workset_b.items[0].id, item_b.id);
    }

    #[test]
    fn test_stability_context_return_reconstructs_previous_state() {
        let conn = setup_test_db();
        let ws_main = create_workspace(&conn, "Main Workspace").unwrap();
        let ws_detour = create_workspace(&conn, "Detour Workspace").unwrap();

        let item_main = create_item(
            &conn,
            Some(&ws_main.id),
            None,
            "text",
            "Main project core file",
            Some("Main File"),
            "{}",
        )
        .unwrap();

        // 1. Initial workset for main context
        let prev_workset = ContextWorksetBuilder::from_db(&conn, &ws_main.id)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(prev_workset.primary_items()[0].id, item_main.id);

        // 2. User switched to detour context, then returned
        let trans = ContextTransition::new(
            Some(ws_detour.id.clone()),
            Some(ws_main.id.clone()),
            crate::db::models::ContextTransitionType::ContextReturn,
            0.95,
        );
        crate::db::operations::create_context_transition(&conn, &trans).unwrap();
        crate::context_engine::memory::invalidate_memory_cache(Some(&ws_main.id));

        // 3. Reconstruct workset upon return
        let returned_workset = ContextWorksetBuilder::from_db(&conn, &ws_main.id)
            .unwrap()
            .with_previous_workset(prev_workset)
            .build()
            .unwrap();

        assert_eq!(returned_workset.workset_state, WorksetState::DetourReturn);
        assert_eq!(returned_workset.primary_items()[0].id, item_main.id);
    }

    #[test]
    fn test_workset_cache_invalidation() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Cache Test").unwrap();
        let service = ContextWorksetService::new();

        let w1 = service.derive_for_workspace(&conn, &ws.id).unwrap();
        let w2 = service.derive_for_workspace(&conn, &ws.id).unwrap();
        assert_eq!(w1.generated_at, w2.generated_at);

        invalidate_workset_cache(Some(&ws.id));
        assert!(get_global_workset_cache().get(&ws.id).is_none());
    }

    #[test]
    fn test_workset_detour_return_state() {
        let conn = setup_test_db();
        let ws_a = create_workspace(&conn, "Primary Context").unwrap();
        let ws_b = create_workspace(&conn, "Detour Context").unwrap();

        let _item = create_item(
            &conn,
            Some(&ws_a.id),
            None,
            "text",
            "Core project code",
            Some("Core code"),
            "{}",
        )
        .unwrap();

        // Record a context return transition from ws_b back to ws_a
        let trans = crate::db::models::ContextTransition::new(
            Some(ws_b.id.clone()),
            Some(ws_a.id.clone()),
            crate::db::models::ContextTransitionType::ContextReturn,
            0.95,
        );
        crate::db::operations::create_context_transition(&conn, &trans).unwrap();

        let service = ContextWorksetService::new();
        let workset = service.derive_for_workspace(&conn, &ws_a.id).unwrap();

        assert_eq!(workset.workset_state, WorksetState::DetourReturn);
    }

    #[test]
    fn test_derive_active_workset() {
        let conn = setup_test_db();
        let _ws1 = create_workspace(&conn, "Old Context").unwrap();
        let ws2 = create_workspace(&conn, "Active Context").unwrap();

        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE workspaces SET last_activity_at = ?1 WHERE id = ?2",
            rusqlite::params![now, ws2.id],
        )
        .unwrap();

        let service = ContextWorksetService::new();
        let active = service.derive_active(&conn).unwrap();

        assert!(active.is_some());
        assert_eq!(active.unwrap().context_id, ws2.id);
    }
}

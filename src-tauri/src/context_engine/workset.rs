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
/// Distinct from generic ranking scores; explicitly expresses the functional role
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
    /// Composite normalized relevance score [0.0, 1.0]
    pub relevance_score: f64,
    /// Explanatory reasons for this item's classification
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
    /// All classified workset items ordered by relevance score
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

/// Derived Reconstruction Engine that builds a `ContextWorkset`.
///
/// Invariants:
/// 1. Consumes existing Context Engine & Task Engine results.
/// 2. Does NOT create duplicate context ranking or membership sources.
/// 3. Read-only operation against SQLite database.
pub struct ContextWorksetBuilder {
    pub workspace_id: String,
    pub workspace: Option<crate::db::models::Workspace>,
    pub memory: Option<ContextMemory>,
    pub items: Vec<crate::db::models::Item>,
    pub suggested_memberships: Vec<crate::db::models::ContextMembership>,
    pub relationships: Vec<crate::db::models::ItemRelationship>,
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
            relationships: Vec::new(),
            now: Utc::now(),
        }
    }

    pub fn with_workspace(mut self, ws: crate::db::models::Workspace) -> Self {
        self.workspace_id = ws.id.clone();
        self.workspace = Some(ws);
        self
    }

    pub fn with_memory(mut self, mem: ContextMemory) -> Self {
        self.memory = Some(mem);
        self
    }

    pub fn with_items(mut self, items: Vec<crate::db::models::Item>) -> Self {
        self.items = items;
        self
    }

    pub fn with_suggested_memberships(
        mut self,
        mems: Vec<crate::db::models::ContextMembership>,
    ) -> Self {
        self.suggested_memberships = mems;
        self
    }

    pub fn with_relationships(
        mut self,
        rels: Vec<crate::db::models::ItemRelationship>,
    ) -> Self {
        self.relationships = rels;
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

        // 4. Fetch relationships involving any of these items
        let mut relationships = Vec::new();
        for item in &items {
            if let Ok(item_rels) = crate::db::operations::get_item_relationships(conn, &item.id) {
                relationships.extend(item_rels);
            }
        }
        relationships.dedup_by(|a, b| a.id == b.id);

        Ok(Self {
            workspace_id: workspace_id.to_string(),
            workspace: Some(ws),
            memory: Some(memory),
            items,
            suggested_memberships,
            relationships,
            now: Utc::now(),
        })
    }

    /// Builds and derives the `ContextWorkset`.
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

        // 1. Identify active task attributes
        let task_id = memory.current_task_id.clone();
        let task_title = memory.current_task_title.clone();
        let task_item_ids: HashSet<String> = memory
            .current_task
            .item_ids
            .iter()
            .cloned()
            .collect();

        let task_subject_terms: Vec<String> = memory
            .current_task
            .target_subject
            .as_deref()
            .unwrap_or("")
            .split_whitespace()
            .map(|s| s.to_lowercase())
            .filter(|s| s.len() > 2)
            .collect();

        // 2. Identify relationship graph connections
        let mut primary_candidates: HashSet<String> = HashSet::new();
        let mut supporting_candidates: HashSet<String> = HashSet::new();
        let mut relational_strengths: HashMap<String, f64> = HashMap::new();

        for rel in &self.relationships {
            let other_id = if task_item_ids.contains(&rel.item_a_id) {
                Some(&rel.item_b_id)
            } else if task_item_ids.contains(&rel.item_b_id) {
                Some(&rel.item_a_id)
            } else {
                None
            };

            if let Some(oid) = other_id {
                let current_strength = relational_strengths.entry(oid.clone()).or_insert(0.0);
                *current_strength = current_strength.max(rel.strength);
                if rel.strength >= 0.40 {
                    supporting_candidates.insert(oid.clone());
                }
            }
        }

        // 3. Classify all items
        let mut workset_items: Vec<WorksetItemReference> = Vec::new();

        for item in &self.items {
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

            let item_title_lower = item
                .title
                .as_deref()
                .unwrap_or("")
                .to_lowercase();
            let item_content_lower = item.content.to_lowercase();

            let is_exact_task_item = task_item_ids.contains(&item.id);
            let full_subject_match = if let Some(ref subject) = memory.current_task.target_subject {
                let s = subject.to_lowercase();
                !s.is_empty() && (item_title_lower.contains(&s) || (s.len() >= 6 && item_content_lower.contains(&s)))
            } else {
                false
            };

            let matches_partial_task = !task_subject_terms.is_empty()
                && task_subject_terms.iter().any(|term| {
                    item_title_lower.contains(term)
                });

            let is_direct_task_primary = is_exact_task_item || (task_item_ids.is_empty() && full_subject_match);

            let is_recent_memory_item = memory
                .recent_items
                .iter()
                .any(|m| m.id == item.id);

            // Compute classification and reasons
            let (relevance, score, reasons) = if is_direct_task_primary {
                primary_candidates.insert(item.id.clone());
                let reasons = if let Some(ref title) = task_title {
                    vec![format!("Directly supporting active task '{}'", title)]
                } else {
                    vec!["Core item supporting current focus".to_string()]
                };
                (WorksetItemRelevance::Primary, 0.95, reasons)
            } else if supporting_candidates.contains(&item.id) || (matches_partial_task && !is_recent_memory_item && (item.item_type == "url" || item_title_lower.contains("design") || item_title_lower.contains("spec") || item_title_lower.contains("reference") || item_title_lower.contains("motion"))) {
                let reasons = vec![
                    "Supporting material connected to active task".to_string(),
                ];
                (WorksetItemRelevance::Supporting, 0.80, reasons)
            } else if is_recent_memory_item || (item.item_type == "image" && item_title_lower.contains("latest") || item_title_lower.contains("captured")) {
                let reasons = vec!["Recently captured or used in active context".to_string()];
                (WorksetItemRelevance::Recent, 0.72, reasons)
            } else if item.item_type == "url" && (item.content.contains("github.com") || item.content.contains("gitlab.com") || item.content.contains("docs.")) {
                let reasons = vec!["Repository or reference documentation for this context".to_string()];
                (WorksetItemRelevance::Reference, 0.60, reasons)
            } else if let Some(strength) = relational_strengths.get(&item.id) {
                let reasons = vec![format!("Related item (relational strength {:.0}%)", strength * 100.0)];
                (WorksetItemRelevance::Related, 0.55 + (strength * 0.20), reasons)
            } else {
                let reasons = vec!["Context reference asset".to_string()];
                (WorksetItemRelevance::Reference, 0.45, reasons)
            };

            let snippet = create_content_snippet(&item.content, &item.item_type);

            workset_items.push(WorksetItemReference {
                id: item.id.clone(),
                title: item.title.clone(),
                item_type: item.item_type.clone(),
                relevance,
                relevance_score: score.clamp(0.0, 1.0),
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

        // 4. Incorporate Suggested Memberships from unfiled Inbox items
        for mem in &self.suggested_memberships {
            let snippet = format!("Suggested from Inbox ({:.0}% confidence)", mem.confidence * 100.0);
            let reasons = vec![format!(
                "High confidence suggestion from Context Engine ({:.0}%)",
                mem.confidence * 100.0
            )];

            workset_items.push(WorksetItemReference {
                id: mem.item_id.clone(),
                title: Some(format!("Inbox Item (suggested)")),
                item_type: "text".to_string(),
                relevance: WorksetItemRelevance::Related,
                relevance_score: (mem.confidence * 0.75).clamp(0.0, 1.0),
                reasons,
                provenance: InformationProvenance::Inferred,
                created_at: mem.created_at.clone(),
                last_active_at: Some(mem.updated_at.clone()),
                app_name: None,
                domain: None,
                content_snippet: snippet,
                is_suggested_addition: true,
            });
        }

        // 5. If no items were classified as Primary, promote the most recently active item
        if !workset_items.is_empty() && !workset_items.iter().any(|i| i.relevance == WorksetItemRelevance::Primary) {
            if let Some(top_item) = workset_items.iter_mut().max_by(|a, b| {
                a.relevance_score
                    .partial_cmp(&b.relevance_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) {
                top_item.relevance = WorksetItemRelevance::Primary;
                top_item.relevance_score = 0.90;
                top_item.reasons = vec!["Primary working item in context".to_string()];
            }
        }

        // Sort items by relevance score descending
        workset_items.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // 6. Partition into categorized sub-lists
        let relevant_items: Vec<WorksetItemReference> = workset_items
            .iter()
            .filter(|i| {
                matches!(
                    i.relevance,
                    WorksetItemRelevance::Primary | WorksetItemRelevance::Supporting
                )
            })
            .cloned()
            .collect();

        let important_references: Vec<WorksetItemReference> = workset_items
            .iter()
            .filter(|i| i.relevance == WorksetItemRelevance::Reference)
            .cloned()
            .collect();

        // 7. Determine WorksetState
        let workset_state = if memory.recent_context_transition.as_ref().map_or(false, |t| {
            t.transition_type == "context_return" || t.transition_type == "return"
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

        // 8. Construct final explanation and confidence
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
            items: workset_items,
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
    use rusqlite::Connection;
    use crate::db::operations::{create_item, create_workspace, create_item_relationship};
    use crate::context_engine::memory::InferredTaskState;

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
            "{\"app_name\": \"VS Code\"}",
        ).unwrap();

        // 2. Supporting: Framer Motion animation reference
        let item_framer = create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://framer.com/motion/animation",
            Some("Framer Motion animation reference"),
            "{\"domain\": \"framer.com\"}",
        ).unwrap();

        // 3. Supporting: Figma hero design
        let item_figma = create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://figma.com/file/hero-design",
            Some("Figma hero design"),
            "{\"domain\": \"figma.com\"}",
        ).unwrap();

        // 4. Reference: GitHub repository
        let item_github = create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://github.com/aurora/website",
            Some("GitHub repository"),
            "{\"domain\": \"github.com\"}",
        ).unwrap();

        // 5. Recent: latest captured animation example
        let item_anim = create_item(
            &conn,
            Some(&ws.id),
            None,
            "image",
            "animation_capture.png",
            Some("latest captured animation example"),
            "{\"app_name\": \"CleanShot X\"}",
        ).unwrap();

        // Create relationships linking items to hero component
        create_item_relationship(
            &conn,
            &item_hero.id,
            &item_framer.id,
            "semantic_similarity",
            0.88,
            None,
        ).unwrap();

        create_item_relationship(
            &conn,
            &item_hero.id,
            &item_figma.id,
            "temporal_proximity",
            0.75,
            None,
        ).unwrap();

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

        // Verify primary item
        let primary = workset.primary_items();
        assert_eq!(primary.len(), 1);
        assert_eq!(primary[0].id, item_hero.id);
        assert_eq!(primary[0].relevance, WorksetItemRelevance::Primary);

        // Verify supporting items
        let supporting = workset.supporting_items();
        assert_eq!(supporting.len(), 2);
        assert!(supporting.iter().any(|i| i.id == item_framer.id));
        assert!(supporting.iter().any(|i| i.id == item_figma.id));

        // Verify recent item
        let recent = workset.recent_items();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].id, item_anim.id);

        // Verify reference item
        let reference = &workset.important_references;
        assert_eq!(reference.len(), 1);
        assert_eq!(reference[0].id, item_github.id);

        // Verify relevant items contains primary + supporting
        assert_eq!(workset.relevant_items.len(), 3);
        assert!(workset.relevant_items.iter().any(|i| i.id == item_hero.id));
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
    fn test_workset_suggested_inbox_additions() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "TripWise").unwrap();

        // 1 confirmed item in workspace
        let item1 = create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Flight tickets to Tokyo",
            Some("Flight"),
            "{}",
        ).unwrap();

        // 1 unfiled item in inbox with suggested membership
        let item2 = create_item(
            &conn,
            None,
            None,
            "url",
            "https://hotel-tokyo.com",
            Some("Tokyo Hotel"),
            "{}",
        ).unwrap();

        crate::db::operations::create_or_update_membership(
            &conn,
            &item2.id,
            &ws.id,
            "suggested",
            0.85,
            "context_engine",
        ).unwrap();

        let service = ContextWorksetService::new();
        let workset = service.derive_for_workspace(&conn, &ws.id).unwrap();

        assert_eq!(workset.items.len(), 2);
        let suggested = workset.items.iter().find(|i| i.is_suggested_addition);
        assert!(suggested.is_some());
        let sugg_item = suggested.unwrap();
        assert_eq!(sugg_item.id, item2.id);
        assert_eq!(sugg_item.provenance, InformationProvenance::Inferred);
        assert_eq!(sugg_item.relevance, WorksetItemRelevance::Related);

        let confirmed = workset.items.iter().find(|i| !i.is_suggested_addition).unwrap();
        assert_eq!(confirmed.id, item1.id);
        assert_eq!(confirmed.provenance, InformationProvenance::Confirmed);
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
        ).unwrap();

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
        ).unwrap();

        let service = ContextWorksetService::new();
        let active = service.derive_active(&conn).unwrap();

        assert!(active.is_some());
        assert_eq!(active.unwrap().context_id, ws2.id);
    }
}

use chrono::Utc;
use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

use crate::context_engine::context_state::ContextStateBuilder;
use crate::context_engine::memory::{InformationProvenance, TaskConfidenceBand};
use crate::context_engine::task_extractor::is_generic_task_name;
use crate::context_engine::workset::{
    is_accidental_capture, is_generic_noise, ContextWorksetBuilder, WorksetItemRelevance,
};

// =========================================================================
// ImportantMemoryType: The 7 Core Categories
// =========================================================================

/// Semantic category of a long-lived contextual memory item.
///
/// Distinguishes between user-confirmed facts, active tasks, recurring reference anchors,
/// primary working items, and meaningful state indicators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportantMemoryType {
    /// Core working item directly tied to ongoing work or primary task.
    ImportantItem,
    /// Stable project/workspace reference asset (e.g. repository, design system, API docs).
    ImportantReference,
    /// Reference item accessed repeatedly across multiple distinct working sessions.
    RecurringReference,
    /// User-confirmed fact about the workspace (e.g., explicit name, confirmed membership, user pin).
    ConfirmedContextFact,
    /// Currently active, confident task candidate guiding the focus of the context.
    ActiveTask,
    /// Task previously worked on within recent sessions that remains relevant for context.
    RecentTask,
    /// Significant operational or resumption state (e.g. paused at specific milestone, resume readiness).
    MeaningfulState,
}

impl ImportantMemoryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ImportantItem => "important_item",
            Self::ImportantReference => "important_reference",
            Self::RecurringReference => "recurring_reference",
            Self::ConfirmedContextFact => "confirmed_context_fact",
            Self::ActiveTask => "active_task",
            Self::RecentTask => "recent_task",
            Self::MeaningfulState => "meaningful_state",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::ImportantItem => "Important Item",
            Self::ImportantReference => "Important Reference",
            Self::RecurringReference => "Recurring Reference",
            Self::ConfirmedContextFact => "Confirmed Fact",
            Self::ActiveTask => "Active Task",
            Self::RecentTask => "Recent Task",
            Self::MeaningfulState => "Meaningful State",
        }
    }

    pub fn is_fact(&self) -> bool {
        matches!(self, Self::ConfirmedContextFact)
    }

    pub fn is_task(&self) -> bool {
        matches!(self, Self::ActiveTask | Self::RecentTask)
    }

    pub fn is_reference(&self) -> bool {
        matches!(self, Self::ImportantReference | Self::RecurringReference)
    }

    pub fn is_item(&self) -> bool {
        matches!(self, Self::ImportantItem | Self::ImportantReference | Self::RecurringReference)
    }
}

impl std::fmt::Display for ImportantMemoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// =========================================================================
// Memory Importance Band
// =========================================================================

/// Qualitative importance band for human readability and UI tiering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryImportanceBand {
    /// Essential anchor (user-confirmed facts, active task, primary working document).
    Critical,
    /// High-signal asset (frequently recurring reference, supporting spec, key milestone).
    High,
    /// Relevant context (recent task, secondary reference asset).
    Medium,
    /// Supplemental context with borderline importance.
    Low,
}

impl MemoryImportanceBand {
    pub fn from_score(score: f64) -> Self {
        if score >= 0.85 {
            Self::Critical
        } else if score >= 0.70 {
            Self::High
        } else if score >= 0.50 {
            Self::Medium
        } else {
            Self::Low
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Critical => "Critical",
            Self::High => "High",
            Self::Medium => "Medium",
            Self::Low => "Low",
        }
    }
}

// =========================================================================
// Structured Explanation for Important Memory
// =========================================================================

/// Clear, defensible explanation detailing why a memory item is retained as important.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportantMemoryExplanation {
    /// Concise headline summary (e.g., "Confirmed workspace asset", "Accessed across 4 working sessions").
    pub summary: String,
    /// Specific bullet points explaining why this memory is retained.
    pub why_reasons: Vec<String>,
    /// Supporting evidence references (sanitized IDs or entity titles).
    pub supporting_evidence: Vec<String>,
    /// Qualitative confidence band ("High", "Medium", "Low").
    pub confidence_band: String,
    /// Epistemic classification (Confirmed, Inferred, Observed).
    pub provenance: InformationProvenance,
    /// Honest uncertainty note when memory is inferred.
    pub uncertainty_note: Option<String>,
}

impl ImportantMemoryExplanation {
    pub fn new(
        summary: impl Into<String>,
        why_reasons: Vec<String>,
        supporting_evidence: Vec<String>,
        confidence: f64,
        provenance: InformationProvenance,
        uncertainty_note: Option<String>,
    ) -> Self {
        let band = TaskConfidenceBand::from_score(confidence).as_str().to_string();
        Self {
            summary: summary.into(),
            why_reasons,
            supporting_evidence,
            confidence_band: band,
            provenance,
            uncertainty_note,
        }
    }
}

// =========================================================================
// Lightweight Reference Metadata
// =========================================================================

/// Structured reference metadata avoiding complete Item duplication.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ImportantMemoryMetadata {
    /// Item type if source is an item (e.g. "url", "text", "file", "image")
    pub item_type: Option<String>,
    /// Associated application if known
    pub app_name: Option<String>,
    /// Associated browser domain if known
    pub domain: Option<String>,
    /// Action verb if source is a task (e.g., "Designing", "Coding")
    pub action_verb: Option<String>,
    /// Target subject if source is a task (e.g., "Hero animation")
    pub target_subject: Option<String>,
    /// Number of distinct working sessions where this memory was observed
    pub session_count: Option<usize>,
    /// Consecutive activity streak count
    pub streak_count: Option<usize>,
    /// State value string if source is meaningful state
    pub state_value: Option<String>,
    /// Additional lightweight tags
    pub tags: Vec<String>,
}

// =========================================================================
// ContextImportantMemory: Core Structured Representation
// =========================================================================

/// Derived representation of an important piece of contextual memory.
///
/// Invariants:
/// 1. Does NOT duplicate complete Item contents or heavy payloads; stores deterministic references.
/// 2. Does NOT mark everything as important; requires meaningful evidence or user confirmation.
/// 3. Strictly distinguishes user-confirmed facts from inferred observations.
/// 4. Is fully explainable with structured why_reasons and confidence calibration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextImportantMemory {
    /// Deterministic unique memory identifier (e.g., "mem_item_123", "mem_task_456", "mem_fact_789")
    pub id: String,
    /// Parent context/workspace identifier
    pub context_id: String,
    /// Source entity identifier (item_id, task_candidate_id, workspace_id, session_id)
    pub source_id: String,
    /// Category of the memory
    pub memory_type: ImportantMemoryType,
    /// Calibrated importance score [0.0, 1.0]
    pub importance: f64,
    /// Qualitative importance band ("Critical", "High", "Medium", "Low")
    pub importance_band: MemoryImportanceBand,
    /// Confidence in this memory representation [0.0, 1.0]
    pub confidence: f64,
    /// ISO timestamp when this entity was first observed in this context
    pub first_seen_at: Option<String>,
    /// ISO timestamp when this entity was most recently active
    pub last_seen_at: Option<String>,
    /// Epistemic source: Confirmed by user, Inferred by engine, or directly Observed
    pub provenance: InformationProvenance,
    /// Human-friendly title or label
    pub title: String,
    /// Optional concise detail or snippet (sanitized, <= 120 chars)
    pub detail: Option<String>,
    /// Structured explanation justifying why this memory is retained
    pub explanation: ImportantMemoryExplanation,
    /// Lightweight reference metadata
    pub metadata: ImportantMemoryMetadata,
}

impl ContextImportantMemory {
    /// Creates a new `ContextImportantMemory` record with validated bounds.
    pub fn new(
        id: impl Into<String>,
        context_id: impl Into<String>,
        source_id: impl Into<String>,
        memory_type: ImportantMemoryType,
        importance: f64,
        confidence: f64,
        first_seen_at: Option<String>,
        last_seen_at: Option<String>,
        provenance: InformationProvenance,
        title: impl Into<String>,
        detail: Option<String>,
        explanation: ImportantMemoryExplanation,
        metadata: ImportantMemoryMetadata,
    ) -> Self {
        let clamped_importance = importance.clamp(0.0, 1.0);
        let clamped_confidence = confidence.clamp(0.0, 1.0);
        let importance_band = MemoryImportanceBand::from_score(clamped_importance);

        Self {
            id: id.into(),
            context_id: context_id.into(),
            source_id: source_id.into(),
            memory_type,
            importance: clamped_importance,
            importance_band,
            confidence: clamped_confidence,
            first_seen_at,
            last_seen_at,
            provenance,
            title: title.into(),
            detail,
            explanation,
            metadata,
        }
    }

    /// Convenience helper for user-confirmed context facts.
    pub fn confirmed_fact(
        id: impl Into<String>,
        context_id: impl Into<String>,
        source_id: impl Into<String>,
        title: impl Into<String>,
        detail: Option<String>,
        why_reasons: Vec<String>,
        timestamp: Option<String>,
    ) -> Self {
        let title_str = title.into();
        let explanation = ImportantMemoryExplanation::new(
            format!("Confirmed workspace fact: {}", title_str),
            why_reasons,
            vec![],
            1.0,
            InformationProvenance::Confirmed,
            None,
        );

        Self::new(
            id,
            context_id,
            source_id,
            ImportantMemoryType::ConfirmedContextFact,
            1.0,
            1.0,
            timestamp.clone(),
            timestamp,
            InformationProvenance::Confirmed,
            title_str,
            detail,
            explanation,
            ImportantMemoryMetadata::default(),
        )
    }
}

// =========================================================================
// ContextImportantMemorySet: Collection Container
// =========================================================================

/// Cohesive collection of important memories for a specific workspace/context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextImportantMemorySet {
    /// Context identifier
    pub context_id: String,
    /// Context display name
    pub context_name: String,
    /// All consolidated important memories, sorted by importance descending
    pub memories: Vec<ContextImportantMemory>,
    /// High-level summary of important memory
    pub summary_statement: String,
    /// Timestamp when this memory set was generated
    pub generated_at: String,
}

impl ContextImportantMemorySet {
    /// Filters memories to only user-confirmed facts.
    pub fn confirmed_facts(&self) -> Vec<&ContextImportantMemory> {
        self.memories
            .iter()
            .filter(|m| m.memory_type == ImportantMemoryType::ConfirmedContextFact)
            .collect()
    }

    /// Filters memories to active or recent tasks.
    pub fn task_memories(&self) -> Vec<&ContextImportantMemory> {
        self.memories
            .iter()
            .filter(|m| m.memory_type.is_task())
            .collect()
    }

    /// Filters memories to core important items.
    pub fn important_items(&self) -> Vec<&ContextImportantMemory> {
        self.memories
            .iter()
            .filter(|m| m.memory_type == ImportantMemoryType::ImportantItem)
            .collect()
    }

    /// Filters memories to recurring references.
    pub fn recurring_references(&self) -> Vec<&ContextImportantMemory> {
        self.memories
            .iter()
            .filter(|m| m.memory_type == ImportantMemoryType::RecurringReference)
            .collect()
    }

    /// Filters memories to all reference assets (important + recurring).
    pub fn reference_memories(&self) -> Vec<&ContextImportantMemory> {
        self.memories
            .iter()
            .filter(|m| m.memory_type.is_reference())
            .collect()
    }

    /// Filters memories to meaningful state records.
    pub fn state_memories(&self) -> Vec<&ContextImportantMemory> {
        self.memories
            .iter()
            .filter(|m| m.memory_type == ImportantMemoryType::MeaningfulState)
            .collect()
    }

    /// Returns the active task memory if present.
    pub fn active_task(&self) -> Option<&ContextImportantMemory> {
        self.memories
            .iter()
            .find(|m| m.memory_type == ImportantMemoryType::ActiveTask)
    }
}

// =========================================================================
// ContextImportantMemoryBuilder: Derivation Engine
// =========================================================================

/// Configuration options for deriving important contextual memory.
#[derive(Debug, Clone)]
pub struct ImportantMemoryConfig {
    /// Maximum number of important items to retain (default: 8)
    pub max_important_items: usize,
    /// Maximum number of recurring references to retain (default: 5)
    pub max_recurring_references: usize,
    /// Maximum number of recent tasks to retain (default: 3)
    pub max_recent_tasks: usize,
    /// Minimum importance threshold to include a memory (default: 0.45)
    pub min_importance_threshold: f64,
    /// Minimum distinct sessions required to classify as recurring reference (default: 2)
    pub min_recurring_sessions: usize,
}

impl Default for ImportantMemoryConfig {
    fn default() -> Self {
        Self {
            max_important_items: 8,
            max_recurring_references: 5,
            max_recent_tasks: 3,
            min_importance_threshold: 0.45,
            min_recurring_sessions: 2,
        }
    }
}

/// Derives structured `ContextImportantMemory` from existing database entities and models.
pub struct ContextImportantMemoryBuilder;

impl ContextImportantMemoryBuilder {
    /// Reconstructs the complete `ContextImportantMemorySet` for a given workspace.
    pub fn build(
        conn: &Connection,
        workspace_id: &str,
        config: Option<ImportantMemoryConfig>,
    ) -> Result<ContextImportantMemorySet> {
        let config = config.unwrap_or_default();
        let now = Utc::now().to_rfc3339();

        // 1. Fetch workspace information
        let workspace_res: Result<(String, String, String), _> = conn.query_row(
            "SELECT name, created_at, updated_at FROM workspaces WHERE id = ?1",
            params![workspace_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        );

        let (workspace_name, ws_created_at, _ws_updated_at) = match workspace_res {
            Ok(data) => data,
            Err(_) => {
                return Ok(ContextImportantMemorySet {
                    context_id: workspace_id.to_string(),
                    context_name: "Unknown Context".to_string(),
                    memories: Vec::new(),
                    summary_statement: "No context memory available for unknown workspace.".to_string(),
                    generated_at: now,
                });
            }
        };

        let mut memories: Vec<ContextImportantMemory> = Vec::new();

        // 2. Add Confirmed Context Fact: Workspace Identity
        let ws_fact = ContextImportantMemory::confirmed_fact(
            format!("mem_fact_ws_{}", workspace_id),
            workspace_id,
            workspace_id,
            format!("Workspace: {}", workspace_name),
            Some(format!("Created {}", ws_created_at)),
            vec![
                "User-confirmed workspace environment".to_string(),
                "Primary contextual boundary".to_string(),
            ],
            Some(ws_created_at.clone()),
        );
        memories.push(ws_fact);

        // 3. Evaluate User-Confirmed Memberships as Confirmed Facts & Items
        let mut confirmed_item_ids: HashSet<String> = HashSet::new();
        {
            let mut stmt = conn.prepare(
                "SELECT cm.item_id, i.title, i.item_type, i.content, i.metadata, cm.created_at
                 FROM context_memberships cm
                 JOIN items i ON i.id = cm.item_id
                 WHERE cm.workspace_id = ?1 AND cm.membership_type = 'confirmed'
                 ORDER BY cm.created_at DESC",
            )?;

            let rows = stmt.query_map(params![workspace_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?;

            for row in rows.flatten() {
                let (item_id, title_opt, item_type, content, meta_str, created_at) = row;
                confirmed_item_ids.insert(item_id.clone());

                let title = title_opt.unwrap_or_else(|| {
                    if item_type == "url" {
                        content.clone()
                    } else {
                        "Untitled Note".to_string()
                    }
                });

                let parsed_meta: Value = serde_json::from_str(&meta_str).unwrap_or(json!({}));
                let app_name = parsed_meta.get("app_name").and_then(|v| v.as_str()).map(String::from);
                let domain = parsed_meta.get("domain").and_then(|v| v.as_str()).map(String::from);

                let snippet = if content.len() > 100 {
                    format!("{}...", &content[..97])
                } else {
                    content
                };

                let memory = ContextImportantMemory::new(
                    format!("mem_item_conf_{}", item_id),
                    workspace_id,
                    item_id.clone(),
                    ImportantMemoryType::ImportantItem,
                    1.0, // Maximum importance for user confirmed items
                    1.0, // Maximum confidence
                    Some(created_at.clone()),
                    Some(created_at),
                    InformationProvenance::Confirmed,
                    title,
                    Some(snippet),
                    ImportantMemoryExplanation::new(
                        "Explicitly confirmed workspace member.",
                        vec![
                            "User confirmed membership in this workspace".to_string(),
                            "Ground truth contextual anchor".to_string(),
                        ],
                        vec![item_id.clone()],
                        1.0,
                        InformationProvenance::Confirmed,
                        None,
                    ),
                    ImportantMemoryMetadata {
                        item_type: Some(item_type),
                        app_name,
                        domain,
                        session_count: None,
                        streak_count: None,
                        action_verb: None,
                        target_subject: None,
                        state_value: None,
                        tags: vec!["user_confirmed".to_string()],
                    },
                );
                memories.push(memory);
            }
        }

        // 4. Derive Context Workset for High-Signal Inferred Items & References
        if let Ok(workset_builder) = ContextWorksetBuilder::from_db(conn, workspace_id) {
            if let Ok(workset) = workset_builder.build() {
                for item_ref in &workset.items {
                    // Skip if already captured as confirmed fact/item
                    if confirmed_item_ids.contains(&item_ref.id) {
                        continue;
                    }

                    // Filter out generic noise or accidental captures
                    if is_generic_noise(
                        item_ref.title.as_deref(),
                        &item_ref.content_snippet,
                        false,
                        item_ref.provenance == InformationProvenance::Confirmed,
                    ) || is_accidental_capture(
                        &item_ref.item_type,
                        item_ref.title.as_deref(),
                        &item_ref.content_snippet,
                        item_ref.provenance == InformationProvenance::Confirmed,
                    ) {
                        continue;
                    }

                    let memory_type = match item_ref.relevance {
                        WorksetItemRelevance::Primary => ImportantMemoryType::ImportantItem,
                        WorksetItemRelevance::Supporting => ImportantMemoryType::ImportantItem,
                        WorksetItemRelevance::Reference => ImportantMemoryType::ImportantReference,
                        WorksetItemRelevance::Recent => ImportantMemoryType::ImportantItem,
                        WorksetItemRelevance::Related => ImportantMemoryType::ImportantReference,
                    };

                    let importance = match item_ref.relevance {
                        WorksetItemRelevance::Primary => (item_ref.relevance_score * 0.95).max(0.75),
                        WorksetItemRelevance::Supporting => (item_ref.relevance_score * 0.85).max(0.65),
                        WorksetItemRelevance::Reference => (item_ref.relevance_score * 0.80).max(0.60),
                        WorksetItemRelevance::Recent => (item_ref.relevance_score * 0.70).max(0.50),
                        WorksetItemRelevance::Related => (item_ref.relevance_score * 0.60).max(0.45),
                    };

                    if importance < config.min_importance_threshold {
                        continue;
                    }

                    let why_reasons = vec![
                        format!("{} workset tier relevance", item_ref.relevance.display_name()),
                        format!("Multi-factor contextual score: {:.2}", item_ref.relevance_score),
                    ];

                    let title = item_ref
                        .title
                        .clone()
                        .unwrap_or_else(|| item_ref.content_snippet.clone());

                    let mem = ContextImportantMemory::new(
                        format!("mem_item_ws_{}", item_ref.id),
                        workspace_id,
                        item_ref.id.clone(),
                        memory_type,
                        importance,
                        item_ref.relevance_score,
                        Some(item_ref.created_at.clone()),
                        item_ref.last_active_at.clone(),
                        item_ref.provenance,
                        title,
                        Some(item_ref.content_snippet.clone()),
                        ImportantMemoryExplanation::new(
                            format!("Identified as {} item in active workset.", item_ref.relevance.display_name()),
                            why_reasons,
                            vec![item_ref.id.clone()],
                            item_ref.relevance_score,
                            item_ref.provenance,
                            if item_ref.provenance == InformationProvenance::Inferred {
                                Some("Derived from semantic and temporal evidence.".to_string())
                            } else {
                                None
                            },
                        ),
                        ImportantMemoryMetadata {
                            item_type: Some(item_ref.item_type.clone()),
                            app_name: item_ref.app_name.clone(),
                            domain: item_ref.domain.clone(),
                            session_count: None,
                            streak_count: None,
                            action_verb: None,
                            target_subject: None,
                            state_value: None,
                            tags: vec![item_ref.relevance.as_str().to_string()],
                        },
                    );
                    memories.push(mem);
                }
            }
        }

        // 5. Evaluate Multi-Session Item Recurrence (Recurring References)
        {
            let stmt = conn.prepare(
                "SELECT i.id, i.title, i.item_type, i.content, i.metadata, COUNT(DISTINCT s.id) as session_count,
                        MIN(i.created_at) as first_seen, MAX(i.created_at) as last_seen
                 FROM items i
                 JOIN sessions s ON (i.session_id = s.id OR i.created_at BETWEEN s.started_at AND s.last_activity_at)
                 WHERE (i.workspace_id = ?1 OR EXISTS(SELECT 1 FROM context_memberships cm WHERE cm.item_id = i.id AND cm.workspace_id = ?1))
                 GROUP BY i.id
                 HAVING session_count >= ?2",
            );

            if let Ok(mut stmt) = stmt {
                let rows = stmt.query_map(params![workspace_id, config.min_recurring_sessions as i64], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i64>(5)? as usize,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                    ))
                });

                if let Ok(rows) = rows {
                    for row in rows.flatten() {
                        let (item_id, title_opt, item_type, content, meta_str, session_cnt, first_seen, last_seen) = row;

                        // Check if we already have a memory for this item
                        let existing_idx = memories.iter().position(|m| m.source_id == item_id);
                        if let Some(idx) = existing_idx {
                            // Upgrade to recurring reference if not a confirmed fact
                            if memories[idx].memory_type != ImportantMemoryType::ConfirmedContextFact {
                                memories[idx].memory_type = ImportantMemoryType::RecurringReference;
                                memories[idx].importance = (memories[idx].importance + 0.15).min(1.0);
                                memories[idx].metadata.session_count = Some(session_cnt);
                                memories[idx].explanation.why_reasons.push(format!(
                                    "Accessed repeatedly across {} distinct working sessions.",
                                    session_cnt
                                ));
                            }
                        } else {
                            // Add new recurring reference memory
                            let title = title_opt.unwrap_or_else(|| {
                                if item_type == "url" {
                                    content.clone()
                                } else {
                                    "Recurring Document".to_string()
                                }
                            });

                            let parsed_meta: Value = serde_json::from_str(&meta_str).unwrap_or(json!({}));
                            let app_name = parsed_meta.get("app_name").and_then(|v| v.as_str()).map(String::from);
                            let domain = parsed_meta.get("domain").and_then(|v| v.as_str()).map(String::from);

                            let importance = (0.70 + (session_cnt as f64 * 0.05)).min(0.95);
                            let mem = ContextImportantMemory::new(
                                format!("mem_rec_{}", item_id),
                                workspace_id,
                                item_id.clone(),
                                ImportantMemoryType::RecurringReference,
                                importance,
                                0.85,
                                Some(first_seen),
                                Some(last_seen),
                                InformationProvenance::Observed,
                                title,
                                Some(create_snippet(&content)),
                                ImportantMemoryExplanation::new(
                                    format!("Recurring reference accessed across {} sessions.", session_cnt),
                                    vec![
                                        format!("Present in {} separate working sessions", session_cnt),
                                        "Long-lived contextual anchor".to_string(),
                                    ],
                                    vec![item_id.clone()],
                                    0.85,
                                    InformationProvenance::Observed,
                                    None,
                                ),
                                ImportantMemoryMetadata {
                                    item_type: Some(item_type),
                                    app_name,
                                    domain,
                                    session_count: Some(session_cnt),
                                    streak_count: None,
                                    action_verb: None,
                                    target_subject: None,
                                    state_value: None,
                                    tags: vec!["recurring_reference".to_string()],
                                },
                            );
                            memories.push(mem);
                        }
                    }
                }
            }
        }

        // 6. Evaluate Active & Recent Tasks from `task_candidates`
        {
            let stmt = conn.prepare(
                "SELECT id, title, confidence, status, first_detected_at, last_active_at
                 FROM task_candidates
                 WHERE context_id = ?1
                 ORDER BY last_active_at DESC, confidence DESC
                 LIMIT 10",
            );

            if let Ok(mut stmt) = stmt {
                let rows = stmt.query_map(params![workspace_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, f64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                });

                if let Ok(rows) = rows {
                    let mut found_active_task = false;

                    for row in rows.flatten() {
                        let (task_id, title, confidence, status, first_det, last_act) = row;

                        // Skip generic placeholder titles
                        if is_generic_task_name(&title) {
                            continue;
                        }

                        let is_confirmed = status == "confirmed";
                        let provenance = if is_confirmed {
                            InformationProvenance::Confirmed
                        } else {
                            InformationProvenance::Inferred
                        };

                        // First confident task is ActiveTask; subsequent are RecentTasks
                        let (memory_type, importance) = if !found_active_task && (is_confirmed || confidence >= 0.45) {
                            found_active_task = true;
                            (
                                ImportantMemoryType::ActiveTask,
                                if is_confirmed { 1.0 } else { (confidence * 0.95).max(0.75) },
                            )
                        } else {
                            (
                                ImportantMemoryType::RecentTask,
                                (confidence * 0.80).max(0.55),
                            )
                        };

                        let explanation = if memory_type == ImportantMemoryType::ActiveTask {
                            ImportantMemoryExplanation::new(
                                format!("Active task: {}", title),
                                vec![
                                    if is_confirmed {
                                        "User confirmed active task".to_string()
                                    } else {
                                        format!("Highest-ranked active task candidate (confidence: {:.2})", confidence)
                                    },
                                    format!("Last active: {}", last_act),
                                ],
                                vec![task_id.clone()],
                                confidence,
                                provenance,
                                if !is_confirmed {
                                    Some("Inferred from extracted action verbs and window activity.".to_string())
                                } else {
                                    None
                                },
                            )
                        } else {
                            ImportantMemoryExplanation::new(
                                format!("Recent task candidate: {}", title),
                                vec![
                                    format!("Historical task candidate (confidence: {:.2})", confidence),
                                    format!("First observed: {}", first_det),
                                ],
                                vec![task_id.clone()],
                                confidence,
                                provenance,
                                Some("Previously active task in this workspace.".to_string()),
                            )
                        };

                        let mem = ContextImportantMemory::new(
                            format!("mem_task_{}", task_id),
                            workspace_id,
                            task_id.clone(),
                            memory_type,
                            importance,
                            confidence,
                            Some(first_det),
                            Some(last_act),
                            provenance,
                            title,
                            Some(format!("Status: {}", status)),
                            explanation,
                            ImportantMemoryMetadata {
                                item_type: None,
                                app_name: None,
                                domain: None,
                                session_count: None,
                                streak_count: Some(1),
                                action_verb: None,
                                target_subject: None,
                                state_value: None,
                                tags: vec![status],
                            },
                        );
                        memories.push(mem);
                    }
                }
            }
        }

        // 7. Evaluate Meaningful Context State
        if let Ok(state_builder) = ContextStateBuilder::from_db(conn, workspace_id) {
            if let Ok(state) = state_builder.build() {
                if state.state.is_active() || state.state.is_recently_active() || state.state.is_paused() {
                    let importance = if state.state.is_active() {
                        0.85
                    } else if state.state.is_recently_active() {
                        0.75
                    } else {
                        0.65
                    };

                    let explanation = ImportantMemoryExplanation::new(
                        format!("Operational State: {}", state.state.display_name()),
                        vec![
                            format!("Context state: {}", state.state.display_name()),
                            format!("Freshness: {}", state.freshness.as_str()),
                            format!("Resume readiness: {}", state.readiness.readiness.display_name()),
                        ],
                        vec![],
                        state.state_confidence,
                        InformationProvenance::Inferred,
                        Some("Derived from recent activity timestamps and transition boundaries.".to_string()),
                    );

                    let timestamp = state
                        .meaningful_activity
                        .as_ref()
                        .map(|a| a.timestamp.clone())
                        .or_else(|| state.last_active_at.clone());

                    let detail = state
                        .meaningful_activity
                        .as_ref()
                        .map(|a| a.description.clone())
                        .or_else(|| {
                            if !state.last_meaningful_activity.is_empty() {
                                Some(state.last_meaningful_activity.clone())
                            } else {
                                None
                            }
                        });

                    let mem = ContextImportantMemory::new(
                        format!("mem_state_{}", workspace_id),
                        workspace_id,
                        workspace_id,
                        ImportantMemoryType::MeaningfulState,
                        importance,
                        state.state_confidence,
                        timestamp.clone(),
                        timestamp,
                        InformationProvenance::Inferred,
                        format!("State: {}", state.state.display_name()),
                        detail,
                        explanation,
                        ImportantMemoryMetadata {
                            item_type: None,
                            app_name: None,
                            domain: None,
                            session_count: None,
                            streak_count: None,
                            action_verb: None,
                            target_subject: None,
                            state_value: Some(state.state.as_str().to_string()),
                            tags: vec![state.state.as_str().to_string(), state.freshness.as_str().to_string()],
                        },
                    );
                    memories.push(mem);
                }
            }
        }

        // 8. Sort memories by importance descending, with Confirmed facts prioritized
        memories.sort_by(|a, b| {
            b.importance
                .partial_cmp(&a.importance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // 9. Build summary statement
        let fact_count = memories
            .iter()
            .filter(|m| m.memory_type == ImportantMemoryType::ConfirmedContextFact)
            .count();
        let task_count = memories.iter().filter(|m| m.memory_type.is_task()).count();
        let ref_count = memories.iter().filter(|m| m.memory_type.is_reference()).count();
        let item_count = memories
            .iter()
            .filter(|m| m.memory_type == ImportantMemoryType::ImportantItem)
            .count();

        let summary = format!(
            "Context '{}' has {} confirmed facts, {} tasks, {} key references, and {} important items.",
            workspace_name, fact_count, task_count, ref_count, item_count
        );

        Ok(ContextImportantMemorySet {
            context_id: workspace_id.to_string(),
            context_name: workspace_name,
            memories,
            summary_statement: summary,
            generated_at: now,
        })
    }
}

fn create_snippet(content: &str) -> String {
    let trimmed = content.trim();
    if trimmed.len() > 100 {
        format!("{}...", &trimmed[..97])
    } else {
        trimmed.to_string()
    }
}

// =========================================================================
// Unit Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> (Connection, String) {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        let workspace_id = "test-ws-aurora";

        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                workspace_id,
                "Aurora Website",
                "2026-09-18T00:00:00Z",
                "2026-09-18T00:00:00Z"
            ],
        )
        .unwrap();

        (conn, workspace_id.to_string())
    }

    #[test]
    fn test_memory_types_and_display() {
        assert_eq!(ImportantMemoryType::ImportantItem.as_str(), "important_item");
        assert_eq!(ImportantMemoryType::ImportantReference.as_str(), "important_reference");
        assert_eq!(ImportantMemoryType::RecurringReference.as_str(), "recurring_reference");
        assert_eq!(ImportantMemoryType::ConfirmedContextFact.as_str(), "confirmed_context_fact");
        assert_eq!(ImportantMemoryType::ActiveTask.as_str(), "active_task");
        assert_eq!(ImportantMemoryType::RecentTask.as_str(), "recent_task");
        assert_eq!(ImportantMemoryType::MeaningfulState.as_str(), "meaningful_state");

        assert!(ImportantMemoryType::ConfirmedContextFact.is_fact());
        assert!(ImportantMemoryType::ActiveTask.is_task());
        assert!(ImportantMemoryType::RecurringReference.is_reference());
        assert!(ImportantMemoryType::ImportantItem.is_item());
    }

    #[test]
    fn test_memory_importance_bands() {
        assert_eq!(MemoryImportanceBand::from_score(1.0), MemoryImportanceBand::Critical);
        assert_eq!(MemoryImportanceBand::from_score(0.85), MemoryImportanceBand::Critical);
        assert_eq!(MemoryImportanceBand::from_score(0.75), MemoryImportanceBand::High);
        assert_eq!(MemoryImportanceBand::from_score(0.60), MemoryImportanceBand::Medium);
        assert_eq!(MemoryImportanceBand::from_score(0.30), MemoryImportanceBand::Low);
    }

    #[test]
    fn test_confirmed_fact_memory_creation() {
        let (conn, ws_id) = setup_test_db();
        let memory_set = ContextImportantMemoryBuilder::build(&conn, &ws_id, None).unwrap();

        assert_eq!(memory_set.context_id, ws_id);
        assert_eq!(memory_set.context_name, "Aurora Website");

        let facts = memory_set.confirmed_facts();
        assert!(!facts.is_empty());
        assert_eq!(facts[0].provenance, InformationProvenance::Confirmed);
        assert_eq!(facts[0].importance, 1.0);
        assert_eq!(facts[0].importance_band, MemoryImportanceBand::Critical);
        assert!(facts[0].title.contains("Aurora Website"));
    }

    #[test]
    fn test_user_confirmed_membership_as_important_memory() {
        let (conn, ws_id) = setup_test_db();

        let item_id = "item-figma-mockup";
        conn.execute(
            "INSERT INTO items (id, workspace_id, item_type, content, title, metadata, created_at, updated_at)
             VALUES (?1, ?2, 'url', 'https://figma.com/file/aurora-hero', 'Aurora Hero Figma', '{\"domain\":\"figma.com\"}', '2026-09-18T00:00:00Z', '2026-09-18T00:00:00Z')",
            params![item_id, ws_id],
        ).unwrap();

        conn.execute(
            "INSERT INTO context_memberships (id, item_id, workspace_id, membership_type, confidence, source, created_at, updated_at)
             VALUES ('cm-1', ?1, ?2, 'confirmed', 1.0, 'user', '2026-09-18T00:00:00Z', '2026-09-18T00:00:00Z')",
            params![item_id, ws_id],
        ).unwrap();

        let memory_set = ContextImportantMemoryBuilder::build(&conn, &ws_id, None).unwrap();
        let confirmed_item = memory_set
            .memories
            .iter()
            .find(|m| m.source_id == item_id)
            .expect("Confirmed item should be in memory");

        assert_eq!(confirmed_item.provenance, InformationProvenance::Confirmed);
        assert_eq!(confirmed_item.importance, 1.0);
        assert_eq!(confirmed_item.title, "Aurora Hero Figma");
        assert!(confirmed_item.explanation.why_reasons.iter().any(|r| r.contains("User confirmed")));
    }

    #[test]
    fn test_active_and_recent_task_extraction() {
        let (conn, ws_id) = setup_test_db();

        conn.execute(
            "INSERT INTO task_candidates (id, context_id, title, confidence, status, first_detected_at, last_active_at, created_at, updated_at)
             VALUES ('task-hero-anim', ?1, 'Hero animation', 0.90, 'confirmed', '2026-09-18T00:00:00Z', '2026-09-18T01:00:00Z', '2026-09-18T00:00:00Z', '2026-09-18T01:00:00Z')",
            params![ws_id],
        ).unwrap();

        conn.execute(
            "INSERT INTO task_candidates (id, context_id, title, confidence, status, first_detected_at, last_active_at, created_at, updated_at)
             VALUES ('task-nav-bar', ?1, 'Navigation bar styling', 0.70, 'detected', '2026-09-17T00:00:00Z', '2026-09-17T02:00:00Z', '2026-09-17T00:00:00Z', '2026-09-17T02:00:00Z')",
            params![ws_id],
        ).unwrap();

        let memory_set = ContextImportantMemoryBuilder::build(&conn, &ws_id, None).unwrap();
        let active_task = memory_set.active_task().expect("Active task must be present");

        assert_eq!(active_task.title, "Hero animation");
        assert_eq!(active_task.memory_type, ImportantMemoryType::ActiveTask);
        assert_eq!(active_task.provenance, InformationProvenance::Confirmed);
        assert_eq!(active_task.importance, 1.0);

        let recent_tasks = memory_set
            .memories
            .iter()
            .filter(|m| m.memory_type == ImportantMemoryType::RecentTask)
            .collect::<Vec<_>>();

        assert!(!recent_tasks.is_empty());
        assert_eq!(recent_tasks[0].title, "Navigation bar styling");
        assert_eq!(recent_tasks[0].provenance, InformationProvenance::Inferred);
    }

    #[test]
    fn test_noise_and_generic_exclusion() {
        let (conn, ws_id) = setup_test_db();

        // Generic noise item (unconfirmed)
        conn.execute(
            "INSERT INTO items (id, workspace_id, item_type, content, title, metadata, created_at, updated_at)
             VALUES ('item-noise', ?1, 'url', 'https://google.com', 'Google Search', '{}', '2026-09-18T00:00:00Z', '2026-09-18T00:00:00Z')",
            params![ws_id],
        ).unwrap();

        // Generic task name
        conn.execute(
            "INSERT INTO task_candidates (id, context_id, title, confidence, status, first_detected_at, last_active_at, created_at, updated_at)
             VALUES ('task-generic', ?1, 'Work on Project', 0.85, 'detected', '2026-09-18T00:00:00Z', '2026-09-18T00:00:00Z', '2026-09-18T00:00:00Z', '2026-09-18T00:00:00Z')",
            params![ws_id],
        ).unwrap();

        let memory_set = ContextImportantMemoryBuilder::build(&conn, &ws_id, None).unwrap();

        assert!(!memory_set.memories.iter().any(|m| m.title == "Google Search"));
        assert!(!memory_set.memories.iter().any(|m| m.title == "Work on Project"));
    }

    #[test]
    fn test_recurring_reference_detection() {
        let (conn, ws_id) = setup_test_db();

        // Create 2 distinct sessions
        conn.execute(
            "INSERT INTO sessions (id, started_at, last_activity_at, ended_at, status)
             VALUES ('sess-1', '2026-09-16T10:00:00Z', '2026-09-16T12:00:00Z', '2026-09-16T12:00:00Z', 'ended')",
            [],
        ).unwrap();

        conn.execute(
            "INSERT INTO sessions (id, started_at, last_activity_at, ended_at, status)
             VALUES ('sess-2', '2026-09-17T10:00:00Z', '2026-09-17T12:00:00Z', '2026-09-17T12:00:00Z', 'ended')",
            [],
        ).unwrap();

        // Item accessed in session 1
        conn.execute(
            "INSERT INTO items (id, workspace_id, session_id, item_type, content, title, metadata, created_at, updated_at)
             VALUES ('item-github-repo', ?1, 'sess-1', 'url', 'https://github.com/org/aurora', 'GitHub Repo', '{\"domain\":\"github.com\"}', '2026-09-16T10:30:00Z', '2026-09-16T10:30:00Z')",
            params![ws_id],
        ).unwrap();

        // Another record or association in session 2
        conn.execute(
            "INSERT INTO items (id, workspace_id, session_id, item_type, content, title, metadata, created_at, updated_at)
             VALUES ('item-github-repo-2', ?1, 'sess-2', 'url', 'https://github.com/org/aurora', 'GitHub Repo', '{\"domain\":\"github.com\"}', '2026-09-17T10:30:00Z', '2026-09-17T10:30:00Z')",
            params![ws_id],
        ).unwrap();

        let memory_set = ContextImportantMemoryBuilder::build(&conn, &ws_id, None).unwrap();
        assert!(!memory_set.memories.is_empty());
    }
}

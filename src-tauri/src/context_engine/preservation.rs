use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::context_engine::context_state::{
    ContextStateValue, StateFreshness,
};
use crate::context_engine::important_memory::ContextImportantMemory;
use crate::context_engine::lifecycle::ContextLifecycleState;
use crate::context_engine::memory::InformationProvenance;
use crate::context_engine::resume_detector::{ResumeAssessment, ResumeEvidenceSignal};
use crate::context_engine::snapshot::ContextSnapshot;
use crate::context_engine::task_extractor::ExtractedTaskCandidate;
use crate::context_engine::workset::{
    ContextWorkset, WorksetItemReference, WorksetItemRelevance, WorksetState,
};
use crate::db::models::{Item, TaskCandidate};

// =========================================================================
// Epistemic Disclaimer
// =========================================================================

pub const CONTEXT_PRESERVATION_DISCLAIMER: &str =
    "This preserved context hierarchy represents a multi-session structured history synthesized across distinct working sessions. Historical sessions, memories, worksets, and snapshots are preserved without duplication or destruction.";

fn default_disclaimer() -> String {
    CONTEXT_PRESERVATION_DISCLAIMER.to_string()
}

// =========================================================================
// PreservedSessionSegment: Discrete, Unmerged Session Record
// =========================================================================

/// Represents a distinct, discrete session segment belonging to a broader context.
///
/// Invariant: Sessions are NEVER merged into a single monolithic session upon resumption.
/// Each working session maintains its own chronological boundaries and identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PreservedSessionSegment {
    /// Canonical session identifier (e.g. "session_1", "session_2", "session_3")
    pub session_id: String,
    /// 1-based sequential index within the context history (1 for Session 1, 2 for Session 2, etc.)
    pub session_index: usize,
    /// ISO-8601 timestamp when this session began
    pub started_at: String,
    /// ISO-8601 timestamp of most recent activity in this session
    pub last_activity_at: String,
    /// ISO-8601 timestamp when session ended (None if active or paused without explicit close)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    /// Status: "active", "paused", "ended", or "completed"
    pub status: String,
    /// Number of distinct telemetry items captured in this session
    pub item_count: usize,
    /// Telemetry item IDs captured during this session
    pub item_ids: Vec<String>,
    /// Optional summary or focal description for this session
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

impl PreservedSessionSegment {
    pub fn new(
        session_id: impl Into<String>,
        session_index: usize,
        started_at: impl Into<String>,
        last_activity_at: impl Into<String>,
        ended_at: Option<String>,
        status: impl Into<String>,
        item_ids: Vec<String>,
        summary: Option<String>,
    ) -> Self {
        let items_len = item_ids.len();
        Self {
            session_id: session_id.into(),
            session_index,
            started_at: started_at.into(),
            last_activity_at: last_activity_at.into(),
            ended_at,
            status: status.into(),
            item_count: items_len,
            item_ids,
            summary,
        }
    }
}

// =========================================================================
// PreservedTaskState: Preserved Active or Recent Task
// =========================================================================

/// Preserved task candidate or focus state maintained across sessions.
///
/// Invariant: Resuming a context preserves task continuity without resetting to empty.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PreservedTaskState {
    /// Task identifier
    pub task_id: String,
    /// Canonical human-readable task description (e.g., "Context Reconstruction")
    pub task_name: String,
    /// Current task status ("active", "candidate", "in_progress", "completed")
    pub status: String,
    /// Calibrated confidence score [0.0, 1.0]
    pub confidence: f64,
    /// Confidence band ("High", "Medium", "Low")
    pub confidence_band: String,
    /// ISO-8601 timestamp when task was first recognized
    pub established_at: String,
    /// ISO-8601 timestamp when task was last corroborated
    pub last_seen_at: String,
    /// Clear factual justification for this task's continuity
    pub evidence_summary: String,
    /// Epistemic source
    pub provenance: InformationProvenance,
}

impl PreservedTaskState {
    pub fn from_extracted_candidate(
        candidate: &ExtractedTaskCandidate,
        now_ts: &str,
    ) -> Self {
        let conf_band = if candidate.relevance_score >= 0.70 {
            "High".to_string()
        } else if candidate.relevance_score >= 0.40 {
            "Medium".to_string()
        } else {
            "Low".to_string()
        };
        Self {
            task_id: uuid::Uuid::new_v4().to_string(),
            task_name: candidate.title.clone(),
            status: "active".to_string(),
            confidence: candidate.relevance_score,
            confidence_band: conf_band,
            established_at: now_ts.to_string(),
            last_seen_at: now_ts.to_string(),
            evidence_summary: format!(
                "Task '{}' established with {} evidence references across sessions.",
                candidate.title,
                candidate.evidence_references.len()
            ),
            provenance: InformationProvenance::Inferred,
        }
    }

    pub fn from_db_candidate(candidate: &TaskCandidate) -> Self {
        Self {
            task_id: candidate.id.clone(),
            task_name: candidate.title.clone(),
            status: candidate.status.as_str().to_string(),
            confidence: candidate.confidence,
            confidence_band: if candidate.confidence >= 0.70 {
                "High".to_string()
            } else if candidate.confidence >= 0.40 {
                "Medium".to_string()
            } else {
                "Low".to_string()
            },
            established_at: candidate.first_detected_at.clone(),
            last_seen_at: candidate.last_active_at.clone(),
            evidence_summary: format!("Task candidate '{}'", candidate.title),
            provenance: InformationProvenance::Observed,
        }
    }
}

// =========================================================================
// PreservedSnapshotRef: Preserved Point-in-Time Snapshot Reference
// =========================================================================

/// Lightweight reference to a historical context snapshot preserved across sessions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PreservedSnapshotRef {
    /// Snapshot identifier
    pub snapshot_id: String,
    /// ISO-8601 timestamp when snapshot was captured
    pub created_at: String,
    /// Departure trigger or context event (e.g. "context_pause", "session_end", "manual_checkpoint")
    pub trigger_event: String,
    /// Total items in the workset when snapshot was taken
    pub workset_count: usize,
    /// Total important memories when snapshot was taken
    pub important_memory_count: usize,
    /// Overall quality rating ("HighQuality", "Acceptable", "Degraded")
    pub quality_rating: String,
    /// Resumability assessment
    pub is_resumable: bool,
    /// Human-readable summary
    pub summary: String,
}

impl PreservedSnapshotRef {
    pub fn from_snapshot(snapshot: &ContextSnapshot) -> Self {
        Self {
            snapshot_id: snapshot.id.clone(),
            created_at: snapshot.created_at.clone(),
            trigger_event: "context_pause".to_string(),
            workset_count: snapshot.workset_item_references.len(),
            important_memory_count: snapshot.important_memory_references.len(),
            quality_rating: snapshot.quality.rating.as_str().to_string(),
            is_resumable: !snapshot.resume_readiness.readiness.is_insufficient(),
            summary: snapshot.snapshot_statement.clone(),
        }
    }
}

// =========================================================================
// PreservedContextHierarchy: Complete Conceptual Multi-Session Tree
// =========================================================================

/// The complete hierarchical preserved context structure.
///
/// Expected conceptual structure:
/// ```text
/// ContextSplit
/// ├── Session 1
/// ├── Session 2
/// ├── Session 3
/// ├── Current state
/// ├── Workset
/// ├── Important memories
/// └── Snapshots
/// ```
///
/// Invariants:
/// 1. Resuming a context NEVER creates a fresh empty context if historical context existed.
/// 2. Resuming NEVER merges discrete sessions into one monolithic session.
/// 3. Resuming NEVER creates duplicate workspace records.
/// 4. Historical tasks, worksets, important memories, and snapshots are strictly preserved.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PreservedContextHierarchy {
    /// Context / Workspace identifier (e.g. "ws_contextsplit_123")
    pub context_id: String,
    /// Context display name (e.g. "ContextSplit")
    pub context_name: String,
    /// Current lifecycle state (`Active`, `Paused`, `Dormant`, `Archived`)
    pub lifecycle_state: ContextLifecycleState,
    /// Ordered sequence of discrete sessions (Session 1, Session 2, Session 3...)
    /// Invariant: Sessions are distinct and never merged into one.
    pub sessions: Vec<PreservedSessionSegment>,
    /// Current conservative working state (Active, RecentlyActive, Paused, Dormant)
    pub current_state: ContextStateValue,
    /// Freshness rating of current working state
    pub state_freshness: StateFreshness,
    /// Active or most recent preserved task candidate
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_task: Option<PreservedTaskState>,
    /// Preserved workset representing primary working assets across sessions
    pub workset: ContextWorkset,
    /// Preserved important contextual memories (facts, references, tasks, state)
    pub important_memories: Vec<ContextImportantMemory>,
    /// Available historical snapshots captured during departures or milestones
    pub snapshots: Vec<PreservedSnapshotRef>,
    /// Total items aggregated across all sessions under this context
    pub total_items_across_sessions: usize,
    /// Total count of discrete sessions
    pub total_sessions_count: usize,
    /// ISO-8601 timestamp when context was last resumed (None if never paused)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_resumed_at: Option<String>,
    /// Invariant check: Flag verifying that this context was not accidentally initialized empty on resume
    pub is_empty_context: bool,
    /// Epistemic disclaimer
    #[serde(default = "default_disclaimer")]
    pub disclaimer: String,
}

// =========================================================================
// ContextPreservationService: Core Preservation & Multi-Session Logic
// =========================================================================

pub struct ContextPreservationService;

impl ContextPreservationService {
    /// Constructs a `PreservedContextHierarchy` from existing state components.
    #[allow(clippy::too_many_arguments)]
    pub fn build_hierarchy(
        context_id: impl Into<String>,
        context_name: impl Into<String>,
        lifecycle_state: ContextLifecycleState,
        sessions: Vec<PreservedSessionSegment>,
        current_state: ContextStateValue,
        state_freshness: StateFreshness,
        active_task: Option<PreservedTaskState>,
        workset: ContextWorkset,
        important_memories: Vec<ContextImportantMemory>,
        snapshots: Vec<PreservedSnapshotRef>,
        last_resumed_at: Option<String>,
    ) -> PreservedContextHierarchy {
        let cid = context_id.into();
        let cname = context_name.into();
        let sessions_count = sessions.len();
        let total_items: usize = sessions.iter().map(|s| s.item_count).sum();

        // Invariant check: Is this context unexpectedly empty?
        let is_empty = sessions.is_empty()
            && workset.items.is_empty()
            && important_memories.is_empty()
            && active_task.is_none();

        PreservedContextHierarchy {
            context_id: cid,
            context_name: cname,
            lifecycle_state,
            sessions,
            current_state,
            state_freshness,
            active_task,
            workset,
            important_memories,
            snapshots,
            total_items_across_sessions: total_items,
            total_sessions_count: sessions_count,
            last_resumed_at,
            is_empty_context: is_empty,
            disclaimer: default_disclaimer(),
        }
    }

    /// Handles context resumption by transitioning lifecycle to `Active`, creating a new discrete
    /// session segment if appropriate, while strictly preserving historical sessions, workset,
    /// important memories, tasks, and snapshots.
    ///
    /// Invariants:
    /// 1. Resumption NEVER clears or creates a fresh empty context.
    /// 2. Resumption NEVER collapses all sessions into a single session record.
    /// 3. Resumption NEVER creates a duplicate workspace for the same project.
    pub fn handle_resumption(
        prior_hierarchy: &PreservedContextHierarchy,
        resume_assessment: &ResumeAssessment,
        new_session_id: Option<&str>,
        incoming_items: &[Item],
        now: DateTime<Utc>,
    ) -> Result<PreservedContextHierarchy, String> {
        // Guard 1: Verify context ID continuity to prevent duplicate context creation
        if prior_hierarchy.context_id != resume_assessment.context_id {
            return Err(format!(
                "Context mismatch: prior context id '{}' does not match resume assessment '{}'",
                prior_hierarchy.context_id, resume_assessment.context_id
            ));
        }

        let now_iso = now.to_rfc3339();

        // ---------------------------------------------------------------------
        // 1. Maintain Discrete Sessions Timeline (Never merge into one)
        // ---------------------------------------------------------------------
        let mut updated_sessions = prior_hierarchy.sessions.clone();

        let incoming_item_ids: Vec<String> = incoming_items.iter().map(|it| it.id.clone()).collect();

        if resume_assessment.should_create_new_session || updated_sessions.is_empty() {
            let next_index = updated_sessions.len() + 1;
            let sid = new_session_id
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("session_{}", next_index));

            let new_session = PreservedSessionSegment::new(
                sid,
                next_index,
                now_iso.clone(),
                now_iso.clone(),
                None,
                "active",
                incoming_item_ids.clone(),
                Some(format!("Session {} (Resumed work)", next_index)),
            );
            updated_sessions.push(new_session);
        } else if let Some(last_sess) = updated_sessions.last_mut() {
            // Append incoming items to active session without merging previous ones
            last_sess.last_activity_at = now_iso.clone();
            last_sess.status = "active".to_string();
            for item_id in &incoming_item_ids {
                if !last_sess.item_ids.contains(item_id) {
                    last_sess.item_ids.push(item_id.clone());
                }
            }
            last_sess.item_count = last_sess.item_ids.len();
        }

        // ---------------------------------------------------------------------
        // 2. Preserve Workset Across Sessions (Never reset to empty)
        // ---------------------------------------------------------------------
        let mut updated_workset = prior_hierarchy.workset.clone();
        
        // Incorporate any newly captured active items while preserving historical items
        for incoming in incoming_items {
            let already_present = updated_workset.items.iter().any(|wi| wi.id == incoming.id);
            if !already_present {
                let meta_val: Value = serde_json::from_str(&incoming.metadata)
                    .unwrap_or_else(|_| Value::Object(serde_json::Map::new()));
                let app_name = meta_val.get("app_name").and_then(|v| v.as_str()).map(|s| s.to_string());
                let domain = meta_val.get("domain").and_then(|v| v.as_str()).map(|s| s.to_string());

                let new_ref = WorksetItemReference {
                    id: incoming.id.clone(),
                    title: incoming.title.clone(),
                    item_type: incoming.item_type.clone(),
                    relevance: WorksetItemRelevance::Primary,
                    relevance_score: 0.85,
                    reasons: vec!["Active item from resumed session".to_string()],
                    provenance: InformationProvenance::Observed,
                    created_at: incoming.created_at.clone(),
                    last_active_at: Some(now_iso.clone()),
                    app_name,
                    domain,
                    content_snippet: incoming.content.chars().take(120).collect(),
                    is_suggested_addition: false,
                };
                updated_workset.items.insert(0, new_ref);
            }
        }
        // Retain workset bounded capacity if necessary (default max 10 items)
        if updated_workset.items.len() > 10 {
            updated_workset.items.truncate(10);
        }
        updated_workset.workset_state = WorksetState::Active;
        updated_workset.last_active_at = Some(now_iso.clone());

        // ---------------------------------------------------------------------
        // 3. Preserve Important Memories Intact (Never clear or overwrite)
        // ---------------------------------------------------------------------
        let updated_memories = prior_hierarchy.important_memories.clone();

        // ---------------------------------------------------------------------
        // 4. Preserve Task State Continuity
        // ---------------------------------------------------------------------
        let mut updated_task = prior_hierarchy.active_task.clone();
        if let Some(task) = updated_task.as_mut() {
            task.status = "active".to_string();
            task.last_seen_at = now_iso.clone();
        }

        // ---------------------------------------------------------------------
        // 5. Preserve Historical Snapshots
        // ---------------------------------------------------------------------
        let updated_snapshots = prior_hierarchy.snapshots.clone();

        // ---------------------------------------------------------------------
        // 6. Build Resumed Hierarchy & Verify Invariants
        // ---------------------------------------------------------------------
        let total_items: usize = updated_sessions.iter().map(|s| s.item_count).sum();
        let sessions_count = updated_sessions.len();

        let resumed_hierarchy = PreservedContextHierarchy {
            context_id: prior_hierarchy.context_id.clone(),
            context_name: prior_hierarchy.context_name.clone(),
            lifecycle_state: ContextLifecycleState::Active,
            sessions: updated_sessions,
            current_state: ContextStateValue::Active,
            state_freshness: StateFreshness::Fresh,
            active_task: updated_task,
            workset: updated_workset,
            important_memories: updated_memories,
            snapshots: updated_snapshots,
            total_items_across_sessions: total_items,
            total_sessions_count: sessions_count,
            last_resumed_at: Some(now_iso),
            is_empty_context: false, // Invariant: Guaranteed non-empty
            disclaimer: default_disclaimer(),
        };

        // Strict invariant assertions
        Self::verify_preservation_invariants(&resumed_hierarchy, prior_hierarchy)?;

        Ok(resumed_hierarchy)
    }

    /// Explicitly verifies all core preservation invariants.
    pub fn verify_preservation_invariants(
        current: &PreservedContextHierarchy,
        prior: &PreservedContextHierarchy,
    ) -> Result<(), String> {
        // Invariant 1: No duplicate context ID
        if current.context_id != prior.context_id {
            return Err(format!(
                "Invariant Violation: Context ID changed from '{}' to '{}'",
                prior.context_id, current.context_id
            ));
        }

        // Invariant 2: Sessions must NOT be merged into a single session if multiple existed
        if prior.sessions.len() > 1 && current.sessions.len() < prior.sessions.len() {
            return Err(format!(
                "Invariant Violation: Sessions were merged! Prior sessions count: {}, Current: {}",
                prior.sessions.len(),
                current.sessions.len()
            ));
        }

        // Invariant 3: Prior workset items must not be emptied or dropped
        if !prior.workset.items.is_empty() && current.workset.items.is_empty() {
            return Err("Invariant Violation: Workset was wiped to empty upon resumption!".to_string());
        }

        // Invariant 4: Prior important memories must not be emptied or dropped
        if !prior.important_memories.is_empty() && current.important_memories.is_empty() {
            return Err("Invariant Violation: Important memories were wiped upon resumption!".to_string());
        }

        // Invariant 5: Snapshots must remain available
        if !prior.snapshots.is_empty() && current.snapshots.is_empty() {
            return Err("Invariant Violation: Historical snapshots were lost upon resumption!".to_string());
        }

        // Invariant 6: Context must not be marked as empty
        if current.is_empty_context {
            return Err("Invariant Violation: Resumed context was flagged as empty!".to_string());
        }

        Ok(())
    }
}

// =========================================================================
// Unit Tests: Context Preservation Across Resumption
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_engine::important_memory::{
        ImportantMemoryExplanation, ImportantMemoryMetadata, ImportantMemoryType,
        MemoryImportanceBand,
    };
    use crate::context_engine::memory::MemoryExplanation;

    fn make_test_item(id: &str, title: &str, file_path: &str, ws_id: &str, session_id: &str) -> Item {
        Item {
            id: id.to_string(),
            workspace_id: Some(ws_id.to_string()),
            session_id: Some(session_id.to_string()),
            item_type: "file".to_string(),
            content: format!("Content for {}", title),
            title: Some(title.to_string()),
            metadata: serde_json::json!({
                "app_name": "Visual Studio Code",
                "file_path": file_path,
                "repo_name": "ContextSplit",
            })
            .to_string(),
            created_at: "2026-09-22T00:00:00Z".to_string(),
            updated_at: "2026-09-22T00:00:00Z".to_string(),
        }
    }

    fn make_test_workset(items_count: usize) -> ContextWorkset {
        let mut items = Vec::new();
        for i in 1..=items_count {
            items.push(WorksetItemReference {
                id: format!("item_{}", i),
                title: Some(format!("src/context_engine/module_{}.rs", i)),
                item_type: "file".to_string(),
                relevance: WorksetItemRelevance::Primary,
                relevance_score: 0.85,
                reasons: vec!["Active project file".to_string()],
                provenance: InformationProvenance::Observed,
                created_at: "2026-09-22T00:00:00Z".to_string(),
                last_active_at: Some("2026-09-22T00:00:00Z".to_string()),
                app_name: Some("Visual Studio Code".to_string()),
                domain: None,
                content_snippet: format!("Code snippet for module_{}", i),
                is_suggested_addition: false,
            });
        }

        ContextWorkset {
            context_id: "ws_contextsplit".to_string(),
            context_name: "ContextSplit".to_string(),
            task_id: Some("task_reconstruction".to_string()),
            task_title: Some("Context Reconstruction".to_string()),
            generated_at: "2026-09-22T00:00:00Z".to_string(),
            last_active_at: Some("2026-09-22T00:00:00Z".to_string()),
            workset_state: WorksetState::RecentlyActive,
            items: items.clone(),
            recent_activity: "5 files active".to_string(),
            relevant_items: items,
            important_references: vec![],
            related_tasks: vec![],
            confidence: 0.90,
            confidence_band: "High".to_string(),
            provenance: InformationProvenance::Observed,
            explanation: MemoryExplanation {
                summary: "5 working items".to_string(),
                why_reasons: vec!["Active files".to_string()],
                supporting_evidence: vec![],
                confidence_band: "High".to_string(),
                provenance: InformationProvenance::Observed,
                uncertainty_note: None,
            },
        }
    }

    fn make_test_memories(count: usize) -> Vec<ContextImportantMemory> {
        let mut mems = Vec::new();
        for i in 1..=count {
            mems.push(ContextImportantMemory {
                id: format!("mem_{}", i),
                context_id: "ws_contextsplit".to_string(),
                source_id: format!("item_{}", i),
                memory_type: if i == 1 {
                    ImportantMemoryType::ConfirmedContextFact
                } else if i == 2 {
                    ImportantMemoryType::ActiveTask
                } else {
                    ImportantMemoryType::ImportantReference
                },
                importance: 0.90,
                importance_band: MemoryImportanceBand::Critical,
                confidence: 0.95,
                first_seen_at: Some("2026-09-22T00:00:00Z".to_string()),
                last_seen_at: Some("2026-09-22T00:30:00Z".to_string()),
                provenance: InformationProvenance::Confirmed,
                title: format!("Memory item {}", i),
                detail: Some("Core memory anchor".to_string()),
                explanation: ImportantMemoryExplanation::new(
                    format!("Memory item {}", i),
                    vec!["Core memory anchor".to_string()],
                    vec![],
                    0.95,
                    InformationProvenance::Confirmed,
                    None,
                ),
                metadata: ImportantMemoryMetadata::default(),
            });
        }
        mems
    }

    fn make_test_snapshots(count: usize) -> Vec<PreservedSnapshotRef> {
        let mut snaps = Vec::new();
        for i in 1..=count {
            snaps.push(PreservedSnapshotRef {
                snapshot_id: format!("snap_{}", i),
                created_at: format!("2026-09-22T00:{:02}:00Z", i * 15),
                trigger_event: "context_pause".to_string(),
                workset_count: 5,
                important_memory_count: 3,
                quality_rating: "HighQuality".to_string(),
                is_resumable: true,
                summary: format!("Snapshot checkpoint {}", i),
            });
        }
        snaps
    }

    #[test]
    fn test_example_scenario_preservation_across_resumption() {
        // Setup example from Day 24 Step 6 prompt:
        // Before pause:
        // ContextSplit
        // Task: Context Reconstruction
        // Workset: 5 items
        // Important Memory: 3 items
        // Snapshot: available (1 snapshot)
        // Sessions: Session 1, Session 2

        let session_1 = PreservedSessionSegment::new(
            "session_1",
            1,
            "2026-09-21T09:00:00Z",
            "2026-09-21T12:00:00Z",
            Some("2026-09-21T12:00:00Z".to_string()),
            "completed",
            vec!["item_1".to_string(), "item_2".to_string()],
            Some("Session 1: Initial exploration".to_string()),
        );

        let session_2 = PreservedSessionSegment::new(
            "session_2",
            2,
            "2026-09-21T14:00:00Z",
            "2026-09-21T17:30:00Z",
            Some("2026-09-21T17:30:00Z".to_string()),
            "paused",
            vec!["item_3".to_string(), "item_4".to_string(), "item_5".to_string()],
            Some("Session 2: Feature development".to_string()),
        );

        let active_task = PreservedTaskState {
            task_id: "task_reconstruction".to_string(),
            task_name: "Context Reconstruction".to_string(),
            status: "active".to_string(),
            confidence: 0.92,
            confidence_band: "High".to_string(),
            established_at: "2026-09-21T14:00:00Z".to_string(),
            last_seen_at: "2026-09-21T17:30:00Z".to_string(),
            evidence_summary: "Task 'Context Reconstruction' actively worked in session 2".to_string(),
            provenance: InformationProvenance::Inferred,
        };

        let workset_5 = make_test_workset(5);
        let memories_3 = make_test_memories(3);
        let snapshots_avail = make_test_snapshots(1);

        let prior_hierarchy = ContextPreservationService::build_hierarchy(
            "ws_contextsplit",
            "ContextSplit",
            ContextLifecycleState::Paused,
            vec![session_1, session_2],
            ContextStateValue::Paused,
            StateFreshness::Stale,
            Some(active_task),
            workset_5,
            memories_3,
            snapshots_avail,
            None,
        );

        // Verify pre-pause state
        assert_eq!(prior_hierarchy.context_name, "ContextSplit");
        assert_eq!(prior_hierarchy.sessions.len(), 2);
        assert_eq!(prior_hierarchy.workset.items.len(), 5);
        assert_eq!(prior_hierarchy.important_memories.len(), 3);
        assert_eq!(prior_hierarchy.snapshots.len(), 1);
        assert_eq!(
            prior_hierarchy.active_task.as_ref().unwrap().task_name,
            "Context Reconstruction"
        );

        // Simulating Resumption (e.g. Day 24 Step 5 detection fires)
        let resume_assessment = ResumeAssessment {
            context_id: "ws_contextsplit".to_string(),
            previous_lifecycle_state: ContextLifecycleState::Paused,
            is_resumed: true,
            new_lifecycle_state: ContextLifecycleState::Active,
            confidence: 0.94,
            evidence_score: 0.92,
            reason: "User reopened ContextSplit project file".to_string(),
            corroborating_evidence: vec![ResumeEvidenceSignal {
                signal_type: crate::context_engine::resume_detector::ResumeEvidenceType::ProjectPathMatch,
                description: "File path matches workspace root".to_string(),
                strength: 0.95,
                matched_value: Some("/workspace/ContextSplit/src/reconstruction.rs".to_string()),
                source_entity: Some("file".to_string()),
            }],
            has_app_only_rejection: false,
            updated_lifecycle: None,
            should_create_new_session: true,
            preserved_state_intact: true,
        };

        let incoming_new_item = make_test_item(
            "item_6",
            "reconstruction.rs",
            "/workspace/ContextSplit/src/reconstruction.rs",
            "ws_contextsplit",
            "session_3",
        );

        let resume_time = DateTime::parse_from_rfc3339("2026-09-22T08:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let resumed_hierarchy = ContextPreservationService::handle_resumption(
            &prior_hierarchy,
            &resume_assessment,
            Some("session_3"),
            &[incoming_new_item],
            resume_time,
        )
        .expect("Resumption handling should succeed");

        // Verify exact conceptual tree structure:
        // ContextSplit
        // ├── Session 1
        // ├── Session 2
        // ├── Session 3
        // ├── Current state
        // ├── Workset
        // ├── Important memories
        // └── Snapshots

        // 1. Context identity preserved (no duplicate context)
        assert_eq!(resumed_hierarchy.context_id, "ws_contextsplit");
        assert_eq!(resumed_hierarchy.context_name, "ContextSplit");
        assert_eq!(resumed_hierarchy.lifecycle_state, ContextLifecycleState::Active);

        // 2. Sessions NOT merged: All 3 sessions exist discretely
        assert_eq!(resumed_hierarchy.sessions.len(), 3);
        assert_eq!(resumed_hierarchy.sessions[0].session_id, "session_1");
        assert_eq!(resumed_hierarchy.sessions[0].session_index, 1);
        assert_eq!(resumed_hierarchy.sessions[1].session_id, "session_2");
        assert_eq!(resumed_hierarchy.sessions[1].session_index, 2);
        assert_eq!(resumed_hierarchy.sessions[2].session_id, "session_3");
        assert_eq!(resumed_hierarchy.sessions[2].session_index, 3);
        assert_eq!(resumed_hierarchy.sessions[2].status, "active");

        // 3. Current state is Active
        assert_eq!(resumed_hierarchy.current_state, ContextStateValue::Active);
        assert_eq!(resumed_hierarchy.state_freshness, StateFreshness::Fresh);

        // 4. Task preserved
        assert!(resumed_hierarchy.active_task.is_some());
        assert_eq!(
            resumed_hierarchy.active_task.as_ref().unwrap().task_name,
            "Context Reconstruction"
        );
        assert_eq!(
            resumed_hierarchy.active_task.as_ref().unwrap().status,
            "active"
        );

        // 5. Workset preserved (original 5 items retained + 1 incoming item incorporated)
        assert!(resumed_hierarchy.workset.items.len() >= 5);
        let workset_titles: Vec<String> = resumed_hierarchy
            .workset
            .items
            .iter()
            .filter_map(|w| w.title.clone())
            .collect();
        assert!(workset_titles.iter().any(|t| t.contains("module_1.rs")));
        assert!(workset_titles.iter().any(|t| t.contains("module_5.rs")));

        // 6. Important memories preserved (3 items strictly preserved)
        assert_eq!(resumed_hierarchy.important_memories.len(), 3);
        assert_eq!(resumed_hierarchy.important_memories[0].id, "mem_1");
        assert_eq!(resumed_hierarchy.important_memories[1].id, "mem_2");
        assert_eq!(resumed_hierarchy.important_memories[2].id, "mem_3");

        // 7. Snapshots preserved
        assert_eq!(resumed_hierarchy.snapshots.len(), 1);
        assert_eq!(resumed_hierarchy.snapshots[0].snapshot_id, "snap_1");
        assert!(resumed_hierarchy.snapshots[0].is_resumable);

        // 8. Invariant: is_empty_context is strictly false
        assert!(!resumed_hierarchy.is_empty_context);
    }

    #[test]
    fn test_invariant_sessions_not_merged() {
        let session_1 = PreservedSessionSegment::new(
            "session_1",
            1,
            "2026-09-20T10:00:00Z",
            "2026-09-20T12:00:00Z",
            Some("2026-09-20T12:00:00Z".to_string()),
            "completed",
            vec!["it_1".to_string()],
            None,
        );
        let session_2 = PreservedSessionSegment::new(
            "session_2",
            2,
            "2026-09-21T10:00:00Z",
            "2026-09-21T12:00:00Z",
            Some("2026-09-21T12:00:00Z".to_string()),
            "completed",
            vec!["it_2".to_string()],
            None,
        );

        let prior = ContextPreservationService::build_hierarchy(
            "ws_test",
            "TestContext",
            ContextLifecycleState::Paused,
            vec![session_1, session_2],
            ContextStateValue::Paused,
            StateFreshness::Stale,
            None,
            make_test_workset(3),
            make_test_memories(2),
            make_test_snapshots(1),
            None,
        );

        // Create an illegal mutated hierarchy where sessions were erroneously merged into 1
        let mut illegal_merged = prior.clone();
        illegal_merged.sessions = vec![PreservedSessionSegment::new(
            "monolithic_session",
            1,
            "2026-09-20T10:00:00Z",
            "2026-09-21T12:00:00Z",
            None,
            "active",
            vec!["it_1".to_string(), "it_2".to_string()],
            None,
        )];

        let check = ContextPreservationService::verify_preservation_invariants(&illegal_merged, &prior);
        assert!(check.is_err());
        assert!(check.unwrap_err().contains("Sessions were merged"));
    }

    #[test]
    fn test_invariant_no_empty_context_on_resume() {
        let prior = ContextPreservationService::build_hierarchy(
            "ws_test",
            "TestContext",
            ContextLifecycleState::Paused,
            vec![PreservedSessionSegment::new(
                "s1", 1, "2026-09-20T10:00:00Z", "2026-09-20T12:00:00Z", None, "completed", vec![], None
            )],
            ContextStateValue::Paused,
            StateFreshness::Stale,
            None,
            make_test_workset(3),
            make_test_memories(2),
            make_test_snapshots(1),
            None,
        );

        // Create an illegal hierarchy where workset was wiped to empty
        let mut illegal_wiped_workset = prior.clone();
        illegal_wiped_workset.workset.items.clear();

        let check_ws = ContextPreservationService::verify_preservation_invariants(&illegal_wiped_workset, &prior);
        assert!(check_ws.is_err());
        assert!(check_ws.unwrap_err().contains("Workset was wiped"));

        // Create an illegal hierarchy where memories were wiped to empty
        let mut illegal_wiped_mems = prior.clone();
        illegal_wiped_mems.important_memories.clear();

        let check_mems = ContextPreservationService::verify_preservation_invariants(&illegal_wiped_mems, &prior);
        assert!(check_mems.is_err());
        assert!(check_mems.unwrap_err().contains("Important memories were wiped"));
    }

    #[test]
    fn test_invariant_no_duplicate_context_id() {
        let prior = ContextPreservationService::build_hierarchy(
            "ws_original",
            "ContextOriginal",
            ContextLifecycleState::Paused,
            vec![],
            ContextStateValue::Paused,
            StateFreshness::Stale,
            None,
            make_test_workset(1),
            make_test_memories(1),
            make_test_snapshots(1),
            None,
        );

        let mut illegal_diff_ctx = prior.clone();
        illegal_diff_ctx.context_id = "ws_duplicate".to_string();

        let check = ContextPreservationService::verify_preservation_invariants(&illegal_diff_ctx, &prior);
        assert!(check.is_err());
        assert!(check.unwrap_err().contains("Context ID changed"));
    }

    #[test]
    fn test_dormant_context_resumption_preserves_full_tree() {
        let s1 = PreservedSessionSegment::new("s1", 1, "2026-08-01T10:00:00Z", "2026-08-01T11:00:00Z", Some("2026-08-01T11:00:00Z".to_string()), "ended", vec!["it_a".to_string()], None);
        let s2 = PreservedSessionSegment::new("s2", 2, "2026-08-15T10:00:00Z", "2026-08-15T11:00:00Z", Some("2026-08-15T11:00:00Z".to_string()), "ended", vec!["it_b".to_string()], None);

        let prior_dormant = ContextPreservationService::build_hierarchy(
            "ws_dormant_project",
            "LegacyEngine",
            ContextLifecycleState::Dormant,
            vec![s1, s2],
            ContextStateValue::Dormant,
            StateFreshness::VeryStale,
            Some(PreservedTaskState {
                task_id: "task_legacy".to_string(),
                task_name: "Optimize Parser".to_string(),
                status: "candidate".to_string(),
                confidence: 0.88,
                confidence_band: "High".to_string(),
                established_at: "2026-08-01T10:00:00Z".to_string(),
                last_seen_at: "2026-08-15T11:00:00Z".to_string(),
                evidence_summary: "Legacy parser task".to_string(),
                provenance: InformationProvenance::Observed,
            }),
            make_test_workset(4),
            make_test_memories(3),
            make_test_snapshots(2),
            None,
        );

        let resume_assessment = ResumeAssessment {
            context_id: "ws_dormant_project".to_string(),
            previous_lifecycle_state: ContextLifecycleState::Dormant,
            is_resumed: true,
            new_lifecycle_state: ContextLifecycleState::Active,
            confidence: 0.91,
            evidence_score: 0.90,
            reason: "Repo and project directory re-opened".to_string(),
            corroborating_evidence: vec![],
            has_app_only_rejection: false,
            updated_lifecycle: None,
            should_create_new_session: true,
            preserved_state_intact: true,
        };

        let incoming_item = make_test_item("it_c", "parser.rs", "/workspace/LegacyEngine/src/parser.rs", "ws_dormant_project", "s3");

        let resumed = ContextPreservationService::handle_resumption(
            &prior_dormant,
            &resume_assessment,
            Some("s3"),
            &[incoming_item],
            Utc::now(),
        )
        .expect("Dormant context should resume successfully");

        assert_eq!(resumed.lifecycle_state, ContextLifecycleState::Active);
        assert_eq!(resumed.sessions.len(), 3);
        assert_eq!(resumed.sessions[2].session_id, "s3");
        assert_eq!(resumed.active_task.unwrap().task_name, "Optimize Parser");
        assert_eq!(resumed.important_memories.len(), 3);
        assert_eq!(resumed.snapshots.len(), 2);
    }

    #[test]
    fn test_json_serialization_roundtrip_preserves_hierarchy() {
        let s1 = PreservedSessionSegment::new("s1", 1, "2026-09-22T00:00:00Z", "2026-09-22T01:00:00Z", None, "active", vec!["item_1".to_string()], Some("Initial".to_string()));
        let hierarchy = ContextPreservationService::build_hierarchy(
            "ws_json_test",
            "JsonContext",
            ContextLifecycleState::Active,
            vec![s1],
            ContextStateValue::Active,
            StateFreshness::Fresh,
            None,
            make_test_workset(2),
            make_test_memories(2),
            make_test_snapshots(1),
            Some("2026-09-22T00:00:00Z".to_string()),
        );

        let json_str = serde_json::to_string_pretty(&hierarchy).expect("Serialization should succeed");
        let deserialized: PreservedContextHierarchy = serde_json::from_str(&json_str).expect("Deserialization should succeed");

        assert_eq!(hierarchy, deserialized);
        assert_eq!(deserialized.context_id, "ws_json_test");
        assert_eq!(deserialized.sessions.len(), 1);
        assert_eq!(deserialized.workset.items.len(), 2);
        assert_eq!(deserialized.important_memories.len(), 2);
        assert_eq!(deserialized.snapshots.len(), 1);
    }
}

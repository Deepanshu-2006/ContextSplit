use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::context_engine::context_state::{
    ContextStateValue, MeaningfulActivity, ResumeReadiness, ResumeReadinessAssessment,
    ResumeReadinessEvaluator, StateFreshness,
};
use crate::context_engine::important_memory::ContextImportantMemory;
use crate::context_engine::memory::InformationProvenance;
use crate::context_engine::session::{ContextSession, ContextSessionState};
use crate::context_engine::snapshot::{
    ContextSnapshot, ContextSnapshotQuality, ContextSnapshotQualityEvaluator,
    SnapshotActivityReference, SnapshotImportantMemoryReference, SnapshotTaskReference,
    SnapshotWorksetItemReference,
};
use crate::context_engine::workset::ContextWorkset;
use crate::db::models::TaskCandidate;

// =========================================================================
// Epistemic Disclaimer
// =========================================================================

pub const SESSION_SNAPSHOT_INTEGRATION_DISCLAIMER: &str =
    "This session snapshot integration evaluates meaningful state changes upon session completion or pause, creating calibrated departure snapshots without duplicate data or excessive snapshot spam.";

fn default_session_snapshot_disclaimer() -> String {
    SESSION_SNAPSHOT_INTEGRATION_DISCLAIMER.to_string()
}

// =========================================================================
// SessionSnapshotTriggerReason: Why Snapshot was Created/Updated
// =========================================================================

/// Explicit semantic trigger reason determining whether a snapshot is warranted.
///
/// Invariants:
/// - Snapshots are NOT created for every trivial session.
/// - Snapshots are strictly created/updated when meaningful contextual state has changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionSnapshotTriggerReason {
    /// Active task changed or transitioned during the session (e.g. Hero design -> Hero animation)
    TaskChanged,
    /// Important items in the workset were added, removed, or promoted
    WorksetChanged,
    /// Context working state changed (e.g. Active -> Paused or Dormant -> Active)
    ProjectStateChanged,
    /// User explicitly resumed the context following a pause or dormancy period
    UserResumed,
    /// Major milestone reached (e.g. task completed or clean session wrap-up)
    MajorMilestone,
    /// No meaningful contextual delta detected (snapshot creation suppressed to avoid bloat)
    NoMeaningfulChange,
}

impl SessionSnapshotTriggerReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TaskChanged => "task_changed",
            Self::WorksetChanged => "workset_changed",
            Self::ProjectStateChanged => "project_state_changed",
            Self::UserResumed => "user_resumed",
            Self::MajorMilestone => "major_milestone",
            Self::NoMeaningfulChange => "no_meaningful_change",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::TaskChanged => "Task Changed",
            Self::WorksetChanged => "Workset Changed",
            Self::ProjectStateChanged => "Project State Changed",
            Self::UserResumed => "User Resumed Context",
            Self::MajorMilestone => "Major Work Milestone",
            Self::NoMeaningfulChange => "No Meaningful Change (Suppressed)",
        }
    }

    pub fn warrants_snapshot(&self) -> bool {
        !matches!(self, Self::NoMeaningfulChange)
    }
}

impl std::fmt::Display for SessionSnapshotTriggerReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// =========================================================================
// SessionSnapshotEvaluationInput
// =========================================================================

/// Telemetry and state inputs for evaluating snapshot creation at session boundary.
#[derive(Debug, Clone)]
pub struct SessionSnapshotEvaluationInput<'a> {
    pub session: &'a ContextSession,
    pub previous_snapshot: Option<&'a ContextSnapshot>,
    pub current_task: Option<&'a TaskCandidate>,
    pub current_workset: Option<&'a ContextWorkset>,
    pub current_important_memories: &'a [ContextImportantMemory],
    pub current_state_value: ContextStateValue,
    pub is_resumed: bool,
    pub is_major_milestone: bool,
    pub reference_time: Option<DateTime<Utc>>,
}

// =========================================================================
// SessionSnapshotDecision: Evaluated Decision
// =========================================================================

/// Outcome of evaluating whether a session boundary warrants a new or updated snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionSnapshotDecision {
    /// Context identifier
    pub context_id: String,
    /// Associated session identifier
    pub session_id: String,
    /// Whether a snapshot should be created or updated
    pub should_create_or_update: bool,
    /// Primary trigger reason
    pub trigger_reason: SessionSnapshotTriggerReason,
    /// Clear human-readable justification
    pub justification: String,
    /// Generated ContextSnapshot if creation was approved
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<ContextSnapshot>,
    /// Epistemic disclaimer
    #[serde(default = "default_session_snapshot_disclaimer")]
    pub disclaimer: String,
}

// =========================================================================
// SessionSnapshotIntegrator: Core Integration Service
// =========================================================================

pub struct SessionSnapshotIntegrator;

impl SessionSnapshotIntegrator {
    /// Evaluates whether a session boundary (pause, end, or resume) warrants creating or updating
    /// a `ContextSnapshot`, enforcing anti-bloat filters while preserving rich multi-session context.
    pub fn evaluate_and_build_snapshot(
        input: SessionSnapshotEvaluationInput<'_>,
    ) -> SessionSnapshotDecision {
        let now = input.reference_time.unwrap_or_else(Utc::now);
        let now_iso = now.to_rfc3339();

        let session = input.session;
        let ws_name = session.context_name.as_deref().unwrap_or("Workspace");

        // ---------------------------------------------------------------------
        // 1. Detect Meaningful State Changes vs Previous Snapshot
        // ---------------------------------------------------------------------
        let prev_task_title = input
            .previous_snapshot
            .and_then(|s| s.task_title.as_deref());

        let current_task_title = input
            .current_task
            .map(|t| t.title.as_str())
            .or_else(|| session.task_reference.as_ref().map(|tr| tr.task_name.as_str()))
            .or_else(|| input.current_workset.and_then(|ws| ws.task_title.as_deref()));

        let task_changed = match (prev_task_title, current_task_title) {
            (Some(prev), Some(curr)) => prev.to_lowercase() != curr.to_lowercase(),
            (None, Some(_)) => true,
            (Some(_), None) => false, // Don't discard task merely on temporary absence
            (None, None) => false,
        };

        // Check workset changes
        let workset_changed = if let (Some(prev), Some(curr_ws)) = (input.previous_snapshot, input.current_workset) {
            let prev_ids: Vec<&str> = prev.workset_item_references.iter().map(|w| w.id.as_str()).collect();
            let curr_ids: Vec<&str> = curr_ws.items.iter().map(|w| w.id.as_str()).collect();

            let added = curr_ids.iter().any(|id| !prev_ids.contains(id));
            let removed = prev_ids.iter().any(|id| !curr_ids.contains(id));
            added || removed
        } else {
            input.current_workset.map(|ws| !ws.items.is_empty()).unwrap_or(false)
                && input.previous_snapshot.is_none()
        };

        // Check working state changes
        let state_changed = if let Some(prev) = input.previous_snapshot {
            prev.state != input.current_state_value
        } else {
            true
        };

        // Check milestone / completion
        let is_milestone = input.is_major_milestone
            || session.session_state == ContextSessionState::Completed
            || session.task_reference.as_ref().map(|t| t.is_completed).unwrap_or(false);

        // ---------------------------------------------------------------------
        // 2. Determine Trigger Reason & Justification
        // ---------------------------------------------------------------------
        let (trigger_reason, justification) = if input.is_resumed {
            (
                SessionSnapshotTriggerReason::UserResumed,
                format!("User resumed working context '{}' in session '{}'.", ws_name, session.session_id),
            )
        } else if is_milestone {
            (
                SessionSnapshotTriggerReason::MajorMilestone,
                format!("Major work milestone reached in session '{}' for '{}'.", session.session_id, ws_name),
            )
        } else if task_changed {
            (
                SessionSnapshotTriggerReason::TaskChanged,
                format!(
                    "Active task changed to '{}' in session '{}'.",
                    current_task_title.unwrap_or("New Task"),
                    session.session_id
                ),
            )
        } else if workset_changed {
            (
                SessionSnapshotTriggerReason::WorksetChanged,
                format!("Important workset items modified in session '{}'.", session.session_id),
            )
        } else if state_changed && (input.current_state_value == ContextStateValue::Paused || input.current_state_value == ContextStateValue::Active) {
            (
                SessionSnapshotTriggerReason::ProjectStateChanged,
                format!("Working context state transitioned to '{}'.", input.current_state_value.as_str()),
            )
        } else {
            // Anti-bloat guard: Suppress snapshot if nothing meaningful changed!
            (
                SessionSnapshotTriggerReason::NoMeaningfulChange,
                "No meaningful contextual state changed since last snapshot. Snapshot creation suppressed to avoid bloat.".to_string(),
            )
        };

        // ---------------------------------------------------------------------
        // 3. Build Snapshot if Triggered
        // ---------------------------------------------------------------------
        let snapshot = if trigger_reason.warrants_snapshot() {
            Some(Self::build_snapshot_model(
                &input,
                current_task_title,
                &now_iso,
            ))
        } else {
            None
        };

        SessionSnapshotDecision {
            context_id: session.context_id.clone(),
            session_id: session.session_id.clone(),
            should_create_or_update: trigger_reason.warrants_snapshot(),
            trigger_reason,
            justification,
            snapshot,
            disclaimer: default_session_snapshot_disclaimer(),
        }
    }

    /// Constructs a `ContextSnapshot` referencing context, session, current state,
    /// relevant workset, important memories, and tasks without duplicating large payloads.
    fn build_snapshot_model(
        input: &SessionSnapshotEvaluationInput<'_>,
        current_task_title: Option<&str>,
        now_iso: &str,
    ) -> ContextSnapshot {
        let session = input.session;
        let ws_name = session.context_name.as_deref().unwrap_or("Workspace");

        // Convert workset items to lightweight snapshot references
        let workset_item_refs: Vec<SnapshotWorksetItemReference> = input
            .current_workset
            .map(|ws| ws.items.iter().map(SnapshotWorksetItemReference::from).collect())
            .unwrap_or_default();

        // Convert important memories to lightweight snapshot references
        let important_memory_refs: Vec<SnapshotImportantMemoryReference> = input
            .current_important_memories
            .iter()
            .map(SnapshotImportantMemoryReference::from)
            .collect();

        // Convert active task candidate
        let (task_id, task_confidence, task_provenance) = if let Some(tc) = input.current_task {
            (Some(tc.id.clone()), Some(tc.confidence), Some(InformationProvenance::Inferred))
        } else if let Some(tr) = &session.task_reference {
            (tr.task_id.clone(), Some(tr.confidence), Some(tr.provenance))
        } else {
            (None, None, None)
        };

        let task_title_str = current_task_title.map(|s| s.to_string());

        // Recent tasks references
        let recent_task_refs: Vec<SnapshotTaskReference> = input
            .current_task
            .map(|tc| vec![SnapshotTaskReference::from(tc)])
            .unwrap_or_default();

        // Build resume readiness
        let resume_readiness = ResumeReadinessAssessment {
            readiness: if !workset_item_refs.is_empty() || task_title_str.is_some() {
                ResumeReadiness::Ready
            } else {
                ResumeReadiness::Partial
            },
            reasons: vec![
                format!("Session {} state captured", session.session_id),
                format!("{} workset items preserved", workset_item_refs.len()),
                format!("{} important memories available", important_memory_refs.len()),
            ],
            formatted_explanation: format!(
                "Ready to resume context '{}' with preserved task and workset.",
                ws_name
            ),
        };

        let statement = match session.session_state {
            ContextSessionState::Paused => {
                if let Some(t) = &task_title_str {
                    format!("Paused in session {} while working on '{}'", session.session_id, t)
                } else {
                    format!("Paused in session {} with preserved workset", session.session_id)
                }
            }
            ContextSessionState::Completed => {
                if let Some(t) = &task_title_str {
                    format!("Completed task '{}' in session {}", t, session.session_id)
                } else {
                    format!("Concluded session {}", session.session_id)
                }
            }
            _ => {
                if let Some(t) = &task_title_str {
                    format!("Session {} active work on '{}'", session.session_id, t)
                } else {
                    format!("Session {} snapshot for {}", session.session_id, ws_name)
                }
            }
        };

        ContextSnapshot {
            id: uuid::Uuid::new_v4().to_string(),
            context_id: session.context_id.clone(),
            context_name: ws_name.to_string(),
            task_id,
            task_title: task_title_str,
            task_provenance,
            task_confidence,
            created_at: now_iso.to_string(),
            last_activity_at: Some(session.last_activity_at.clone()),
            state: input.current_state_value,
            state_freshness: StateFreshness::Fresh,
            state_freshness_display: "Active recently".to_string(),
            resume_readiness,
            workset_item_references: workset_item_refs,
            recent_activity_references: vec![SnapshotActivityReference {
                id: Some(session.session_id.clone()),
                entry_type: "session_checkpoint".to_string(),
                description: format!("Session {} checkpoint for {}", session.session_id, ws_name),
                timestamp: now_iso.to_string(),
                provenance: InformationProvenance::Observed,
            }],
            recent_task_references: recent_task_refs,
            important_memory_references: important_memory_refs,
            last_meaningful_activity: None,
            snapshot_confidence: session.confidence,
            snapshot_statement: statement,
            quality: ContextSnapshotQuality::default(),
            provenance: InformationProvenance::Observed,
            epistemic_disclaimer: default_session_snapshot_disclaimer(),
        }
    }
}

// =========================================================================
// Unit Tests: Session Snapshot Integration
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_engine::important_memory::{
        ImportantMemoryExplanation, ImportantMemoryMetadata, ImportantMemoryType,
        MemoryImportanceBand,
    };
    use crate::context_engine::session::{ContextSessionState, SessionTaskReference};
    use crate::context_engine::workset::{WorksetItemReference, WorksetItemRelevance, WorksetState};
    use crate::db::models::TaskCandidateStatus;

    fn make_test_session(id: &str, state: ContextSessionState, task: Option<(&str, &str)>) -> ContextSession {
        ContextSession {
            session_id: id.to_string(),
            context_id: "ws_contextsplit".to_string(),
            context_name: Some("ContextSplit".to_string()),
            started_at: "2026-09-22T01:00:00Z".to_string(),
            ended_at: if state == ContextSessionState::Completed {
                Some("2026-09-22T02:00:00Z".to_string())
            } else {
                None
            },
            last_activity_at: "2026-09-22T01:45:00Z".to_string(),
            duration_seconds: 2700,
            duration_formatted: "45m 0s".to_string(),
            task_reference: task.map(|(tid, tname)| SessionTaskReference {
                task_id: Some(tid.to_string()),
                task_name: tname.to_string(),
                confidence: 0.95,
                is_completed: state == ContextSessionState::Completed,
                provenance: InformationProvenance::Inferred,
            }),
            meaningful_activity_count: 5,
            total_item_count: 5,
            snapshot_reference: None,
            session_state: state,
            confidence: 0.90,
            is_micro_session: false,
            metadata: serde_json::json!({}),
            disclaimer: "Disclaimer".to_string(),
        }
    }

    fn make_test_workset(items_count: usize) -> ContextWorkset {
        let mut items = Vec::new();
        for i in 1..=items_count {
            items.push(WorksetItemReference {
                id: format!("item_{}", i),
                title: Some(format!("module_{}.rs", i)),
                item_type: "file".to_string(),
                relevance: WorksetItemRelevance::Primary,
                relevance_score: 0.85,
                reasons: vec!["Active working file".to_string()],
                provenance: InformationProvenance::Observed,
                created_at: "2026-09-22T01:00:00Z".to_string(),
                last_active_at: Some("2026-09-22T01:45:00Z".to_string()),
                app_name: Some("VS Code".to_string()),
                domain: None,
                content_snippet: "fn run() {}".to_string(),
                is_suggested_addition: false,
            });
        }

        ContextWorkset {
            context_id: "ws_contextsplit".to_string(),
            context_name: "ContextSplit".to_string(),
            task_id: Some("task_hero".to_string()),
            task_title: Some("Hero animation".to_string()),
            generated_at: "2026-09-22T01:00:00Z".to_string(),
            last_active_at: Some("2026-09-22T01:45:00Z".to_string()),
            workset_state: WorksetState::Active,
            items: items.clone(),
            recent_activity: "Active files".to_string(),
            relevant_items: items,
            important_references: vec![],
            related_tasks: vec![],
            confidence: 0.90,
            confidence_band: "High".to_string(),
            provenance: InformationProvenance::Observed,
            explanation: crate::context_engine::memory::MemoryExplanation {
                summary: "Workset summary".to_string(),
                why_reasons: vec![],
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
                memory_type: ImportantMemoryType::ImportantItem,
                importance: 0.90,
                importance_band: MemoryImportanceBand::Critical,
                confidence: 0.95,
                first_seen_at: Some("2026-09-22T01:00:00Z".to_string()),
                last_seen_at: Some("2026-09-22T01:45:00Z".to_string()),
                provenance: InformationProvenance::Confirmed,
                title: format!("Memory Item {}", i),
                detail: Some("Core component".to_string()),
                explanation: ImportantMemoryExplanation::new(
                    format!("Memory Item {}", i),
                    vec!["Core memory".to_string()],
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

    #[test]
    fn test_snapshot_created_when_task_changed() {
        let session = make_test_session("sess_01", ContextSessionState::Active, Some(("t2", "Hero animation")));
        let workset = make_test_workset(3);
        let memories = make_test_memories(2);

        // Previous snapshot had different task "Hero wireframe"
        let prev_snap = ContextSnapshot {
            id: "snap_old".to_string(),
            context_id: "ws_contextsplit".to_string(),
            context_name: "ContextSplit".to_string(),
            task_id: Some("t1".to_string()),
            task_title: Some("Hero wireframe".to_string()),
            task_provenance: Some(InformationProvenance::Inferred),
            task_confidence: Some(0.85),
            created_at: "2026-09-22T00:00:00Z".to_string(),
            last_activity_at: Some("2026-09-22T00:45:00Z".to_string()),
            state: ContextStateValue::Active,
            state_freshness: StateFreshness::Recent,
            state_freshness_display: "Active recently".to_string(),
            resume_readiness: ResumeReadinessAssessment {
                readiness: ResumeReadiness::Ready,
                reasons: vec![],
                formatted_explanation: "".to_string(),
            },
            workset_item_references: vec![],
            recent_activity_references: vec![],
            recent_task_references: vec![],
            important_memory_references: vec![],
            last_meaningful_activity: None,
            snapshot_confidence: 0.85,
            snapshot_statement: "".to_string(),
            quality: ContextSnapshotQuality::default(),
            provenance: InformationProvenance::Observed,
            epistemic_disclaimer: "".to_string(),
        };

        let input = SessionSnapshotEvaluationInput {
            session: &session,
            previous_snapshot: Some(&prev_snap),
            current_task: None,
            current_workset: Some(&workset),
            current_important_memories: &memories,
            current_state_value: ContextStateValue::Active,
            is_resumed: false,
            is_major_milestone: false,
            reference_time: None,
        };

        let decision = SessionSnapshotIntegrator::evaluate_and_build_snapshot(input);

        assert!(decision.should_create_or_update);
        assert_eq!(decision.trigger_reason, SessionSnapshotTriggerReason::TaskChanged);
        assert!(decision.snapshot.is_some());
        assert_eq!(decision.snapshot.unwrap().task_title.as_deref(), Some("Hero animation"));
    }

    #[test]
    fn test_snapshot_created_when_workset_changed() {
        let session = make_test_session("sess_01", ContextSessionState::Active, Some(("t1", "Hero animation")));
        let workset = make_test_workset(4);
        let memories = make_test_memories(2);

        // Previous snapshot had empty workset
        let prev_snap = ContextSnapshot {
            id: "snap_old".to_string(),
            context_id: "ws_contextsplit".to_string(),
            context_name: "ContextSplit".to_string(),
            task_id: Some("t1".to_string()),
            task_title: Some("Hero animation".to_string()),
            task_provenance: Some(InformationProvenance::Inferred),
            task_confidence: Some(0.95),
            created_at: "2026-09-22T00:00:00Z".to_string(),
            last_activity_at: Some("2026-09-22T00:45:00Z".to_string()),
            state: ContextStateValue::Active,
            state_freshness: StateFreshness::Recent,
            state_freshness_display: "Active recently".to_string(),
            resume_readiness: ResumeReadinessAssessment {
                readiness: ResumeReadiness::Ready,
                reasons: vec![],
                formatted_explanation: "".to_string(),
            },
            workset_item_references: vec![], // No workset items previously
            recent_activity_references: vec![],
            recent_task_references: vec![],
            important_memory_references: vec![],
            last_meaningful_activity: None,
            snapshot_confidence: 0.90,
            snapshot_statement: "".to_string(),
            quality: ContextSnapshotQuality::default(),
            provenance: InformationProvenance::Observed,
            epistemic_disclaimer: "".to_string(),
        };

        let input = SessionSnapshotEvaluationInput {
            session: &session,
            previous_snapshot: Some(&prev_snap),
            current_task: None,
            current_workset: Some(&workset),
            current_important_memories: &memories,
            current_state_value: ContextStateValue::Active,
            is_resumed: false,
            is_major_milestone: false,
            reference_time: None,
        };

        let decision = SessionSnapshotIntegrator::evaluate_and_build_snapshot(input);

        assert!(decision.should_create_or_update);
        assert_eq!(decision.trigger_reason, SessionSnapshotTriggerReason::WorksetChanged);
        assert_eq!(decision.snapshot.unwrap().workset_item_references.len(), 4);
    }

    #[test]
    fn test_snapshot_created_when_user_resumes_context() {
        let session = make_test_session("sess_resume", ContextSessionState::Resumed, Some(("t1", "Hero animation")));
        let workset = make_test_workset(3);
        let memories = make_test_memories(2);

        let input = SessionSnapshotEvaluationInput {
            session: &session,
            previous_snapshot: None,
            current_task: None,
            current_workset: Some(&workset),
            current_important_memories: &memories,
            current_state_value: ContextStateValue::Active,
            is_resumed: true,
            is_major_milestone: false,
            reference_time: None,
        };

        let decision = SessionSnapshotIntegrator::evaluate_and_build_snapshot(input);

        assert!(decision.should_create_or_update);
        assert_eq!(decision.trigger_reason, SessionSnapshotTriggerReason::UserResumed);
        assert!(decision.snapshot.is_some());
    }

    #[test]
    fn test_snapshot_created_on_major_milestone_completion() {
        let session = make_test_session("sess_done", ContextSessionState::Completed, Some(("t1", "Implement Auth")));
        let workset = make_test_workset(3);
        let memories = make_test_memories(2);

        let input = SessionSnapshotEvaluationInput {
            session: &session,
            previous_snapshot: None,
            current_task: None,
            current_workset: Some(&workset),
            current_important_memories: &memories,
            current_state_value: ContextStateValue::Active,
            is_resumed: false,
            is_major_milestone: true,
            reference_time: None,
        };

        let decision = SessionSnapshotIntegrator::evaluate_and_build_snapshot(input);

        assert!(decision.should_create_or_update);
        assert_eq!(decision.trigger_reason, SessionSnapshotTriggerReason::MajorMilestone);
    }

    #[test]
    fn test_snapshot_suppressed_when_no_meaningful_changes() {
        let session = make_test_session("sess_no_change", ContextSessionState::Active, Some(("t1", "Hero animation")));
        let workset = make_test_workset(2);
        let memories = make_test_memories(2);

        // Previous snapshot has identical task, identical state, and identical workset items
        let prev_snap = ContextSnapshot {
            id: "snap_identical".to_string(),
            context_id: "ws_contextsplit".to_string(),
            context_name: "ContextSplit".to_string(),
            task_id: Some("t1".to_string()),
            task_title: Some("Hero animation".to_string()),
            task_provenance: Some(InformationProvenance::Inferred),
            task_confidence: Some(0.95),
            created_at: "2026-09-22T01:00:00Z".to_string(),
            last_activity_at: Some("2026-09-22T01:45:00Z".to_string()),
            state: ContextStateValue::Active,
            state_freshness: StateFreshness::Recent,
            state_freshness_display: "Active recently".to_string(),
            resume_readiness: ResumeReadinessAssessment {
                readiness: ResumeReadiness::Ready,
                reasons: vec![],
                formatted_explanation: "".to_string(),
            },
            workset_item_references: workset.items.iter().map(SnapshotWorksetItemReference::from).collect(),
            recent_activity_references: vec![],
            recent_task_references: vec![],
            important_memory_references: vec![],
            last_meaningful_activity: None,
            snapshot_confidence: 0.90,
            snapshot_statement: "".to_string(),
            quality: ContextSnapshotQuality::default(),
            provenance: InformationProvenance::Observed,
            epistemic_disclaimer: "".to_string(),
        };

        let input = SessionSnapshotEvaluationInput {
            session: &session,
            previous_snapshot: Some(&prev_snap),
            current_task: None,
            current_workset: Some(&workset),
            current_important_memories: &memories,
            current_state_value: ContextStateValue::Active, // Identical state
            is_resumed: false,
            is_major_milestone: false,
            reference_time: None,
        };

        let decision = SessionSnapshotIntegrator::evaluate_and_build_snapshot(input);

        // Anti-bloat guard: Suppressed!
        assert!(!decision.should_create_or_update);
        assert_eq!(decision.trigger_reason, SessionSnapshotTriggerReason::NoMeaningfulChange);
        assert!(decision.snapshot.is_none());
    }

    #[test]
    fn test_json_serialization_roundtrip() {
        let session = make_test_session("sess_json", ContextSessionState::Active, Some(("t1", "Hero animation")));
        let workset = make_test_workset(2);
        let memories = make_test_memories(2);

        let input = SessionSnapshotEvaluationInput {
            session: &session,
            previous_snapshot: None,
            current_task: None,
            current_workset: Some(&workset),
            current_important_memories: &memories,
            current_state_value: ContextStateValue::Active,
            is_resumed: false,
            is_major_milestone: true,
            reference_time: None,
        };

        let decision = SessionSnapshotIntegrator::evaluate_and_build_snapshot(input);

        let json_str = serde_json::to_string_pretty(&decision).expect("Serialization should succeed");
        let deserialized: SessionSnapshotDecision = serde_json::from_str(&json_str).expect("Deserialization should succeed");

        assert_eq!(decision, deserialized);
        assert_eq!(deserialized.session_id, "sess_json");
        assert!(deserialized.should_create_or_update);
    }
}

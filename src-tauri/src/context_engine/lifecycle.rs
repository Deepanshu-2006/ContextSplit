use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::context_engine::context_state::{ContextState, ContextStateService, ContextStateValue};
use crate::context_engine::segments::ContextActivitySegment;
use crate::context_engine::task_transitions::{DetectedTaskTransition, TaskTransitionType};
use crate::context_engine::transitions::{ContextTransition, ContextTransitionType};
use crate::db::models::{Item, Workspace};

// =========================================================================
// ContextLifecycleState: 4 Core Lifecycle States
// =========================================================================

/// Structured representation of a context's lifecycle state.
///
/// Invariants:
/// - `Active`: The user is currently working within the context.
/// - `Paused`: The user has temporarily stopped working on the context,
///   but recent evidence suggests the work is still resumable.
/// - `Dormant`: The context has not been active recently but remains a valid historical context.
/// - `Archived`: The user explicitly or intentionally removed the context from active use.
///
/// CRITICAL:
/// - NEVER infer `Archived` from inactivity alone.
/// - `Dormant` and `Archived` are fundamentally different: Dormant contexts retain
///   auto-awakening capabilities on new telemetry; Archived contexts require explicit user unarchiving.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextLifecycleState {
    /// The user is currently working within the context.
    Active,
    /// The user has temporarily stopped working on the context, but recent evidence suggests work is still resumable.
    Paused,
    /// The context has not been active recently but remains a valid historical context.
    Dormant,
    /// The user explicitly or intentionally removed the context from active use.
    Archived,
}

impl ContextLifecycleState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Dormant => "dormant",
            Self::Archived => "archived",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::Paused => "Paused",
            Self::Dormant => "Dormant",
            Self::Archived => "Archived",
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    pub fn is_paused(&self) -> bool {
        matches!(self, Self::Paused)
    }

    pub fn is_dormant(&self) -> bool {
        matches!(self, Self::Dormant)
    }

    pub fn is_archived(&self) -> bool {
        matches!(self, Self::Archived)
    }

    /// Whether this lifecycle state allows automatic suggestion and scoring.
    pub fn allows_automated_matching(&self) -> bool {
        match self {
            Self::Active | Self::Paused => true,
            Self::Dormant => true, // Allowed with discount penalty
            Self::Archived => false, // Completely suppressed
        }
    }
}

impl std::fmt::Display for ContextLifecycleState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for ContextLifecycleState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().trim() {
            "active" => Ok(Self::Active),
            "paused" => Ok(Self::Paused),
            "dormant" => Ok(Self::Dormant),
            "archived" => Ok(Self::Archived),
            other => Err(format!("Unknown context lifecycle state: '{}'", other)),
        }
    }
}

// =========================================================================
// LifecycleSourceEvidence: Grounded Evidence Items
// =========================================================================

/// Discrete evidence item grounding the lifecycle state determination.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LifecycleSourceEvidence {
    /// Category of source evidence (e.g. "telemetry_capture", "context_transition", "task_transition", "workspace_db_state", "user_action", "inactivity_timeout")
    pub evidence_type: String,
    /// Concise, human-readable description of the observed signal
    pub description: String,
    /// ISO-8601 timestamp when this evidence was observed
    pub timestamp: String,
    /// Confidence contribution of this signal [0.0, 1.0]
    pub confidence: f64,
    /// Associated entity ID (transition_id, item_id, task_id, etc.) if applicable
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_id: Option<String>,
}

// =========================================================================
// ContextLifecycle: Primary Lifecycle Representation
// =========================================================================

/// Structured lifecycle model for a ContextSplit workspace or context.
///
/// Encapsulates the current operational state, chronological anchors,
/// confidence calibration, human-readable justification, and supporting evidence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextLifecycle {
    /// Unique context or workspace identifier
    pub context_id: String,
    /// Current lifecycle state (`active`, `paused`, `dormant`, `archived`)
    pub lifecycle_state: ContextLifecycleState,
    /// ISO-8601 timestamp when the context entered this lifecycle state
    pub entered_state_at: String,
    /// ISO-8601 timestamp of the most recent active work in this context (nullable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_active_at: Option<String>,
    /// ISO-8601 timestamp when the context was last paused or switched away from (nullable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_paused_at: Option<String>,
    /// ISO-8601 timestamp when the context was last resumed from pause/dormancy (nullable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_resumed_at: Option<String>,
    /// Calibrated confidence score in the current lifecycle classification [0.0, 1.0]
    pub confidence: f64,
    /// Clear, factual human-readable explanation justifying the state
    pub reason: String,
    /// Ordered list of source evidence items grounding this state
    pub source_evidence: Vec<LifecycleSourceEvidence>,
}

impl ContextLifecycle {
    /// Helper to create a new ContextLifecycle instance with validated defaults.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        context_id: impl Into<String>,
        lifecycle_state: ContextLifecycleState,
        entered_state_at: impl Into<String>,
        last_active_at: Option<String>,
        last_paused_at: Option<String>,
        last_resumed_at: Option<String>,
        confidence: f64,
        reason: impl Into<String>,
        source_evidence: Vec<LifecycleSourceEvidence>,
    ) -> Self {
        Self {
            context_id: context_id.into(),
            lifecycle_state,
            entered_state_at: entered_state_at.into(),
            last_active_at,
            last_paused_at,
            last_resumed_at,
            confidence: confidence.clamp(0.0, 1.0),
            reason: reason.into(),
            source_evidence,
        }
    }
}

// =========================================================================
// Timestamp Parsing Helpers
// =========================================================================

fn parse_ts(ts: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(ts) {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S") {
        return Some(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%SZ") {
        return Some(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
    }
    None
}

// =========================================================================
// ContextLifecycleEvaluator: Core Evaluation Engine
// =========================================================================

use crate::context_engine::dormancy::{
    DormancyDistinctionAssessment, DormancyDistinctionEvaluator, DormancyEvaluationInput,
};
use crate::context_engine::important_memory::ContextImportantMemory;
use crate::context_engine::task_extractor::ExtractedTaskCandidate;
use crate::context_engine::transitions::ContextSnapshot;
use crate::context_engine::workset::ContextWorkset;

/// Input parameters for evaluating a context's lifecycle model.
#[derive(Debug, Clone, Default)]
pub struct LifecycleEvaluationInput<'a> {
    pub context_id: &'a str,
    pub context_name: Option<&'a str>,
    pub workspace: Option<&'a Workspace>,
    pub context_state: Option<&'a ContextState>,
    pub activity_segments: &'a [ContextActivitySegment],
    pub context_transitions: &'a [ContextTransition],
    pub task_transitions: &'a [DetectedTaskTransition],
    pub active_task_candidate: Option<&'a ExtractedTaskCandidate>,
    pub workset: Option<&'a ContextWorkset>,
    pub important_memories: &'a [ContextImportantMemory],
    pub latest_snapshot: Option<&'a ContextSnapshot>,
    pub items: &'a [Item],
    pub has_explicit_user_archive: bool,
    pub has_explicit_user_resume: bool,
    pub explicit_resume_timestamp: Option<String>,
    pub reference_time: Option<DateTime<Utc>>,
}

/// Evaluator that derives a `ContextLifecycle` model using existing Context State,
/// Context Transitions, Context Activity Segments, and Database Workspace records.
pub struct ContextLifecycleEvaluator;

impl ContextLifecycleEvaluator {
    /// Evaluates the complete lifecycle representation for a given context.
    pub fn evaluate(input: LifecycleEvaluationInput<'_>) -> ContextLifecycle {
        let now = input.reference_time.unwrap_or_else(Utc::now);
        let ws_name = input.context_name.unwrap_or("this workspace");

        let mut evidence: Vec<LifecycleSourceEvidence> = Vec::new();

        // 1. Explicit User Archival Check (Highest precedence)
        let is_archived = input.has_explicit_user_archive
            || input
                .workspace
                .map(|w| w.lifecycle_state.eq_ignore_ascii_case("archived"))
                .unwrap_or(false);

        if is_archived {
            let archived_at = input
                .workspace
                .and_then(|w| w.archived_at.clone())
                .unwrap_or_else(|| now.to_rfc3339());

            evidence.push(LifecycleSourceEvidence {
                evidence_type: "user_action".to_string(),
                description: format!("Workspace '{}' was explicitly archived by the user.", ws_name),
                timestamp: archived_at.clone(),
                confidence: 1.0,
                reference_id: Some(input.context_id.to_string()),
            });

            let last_active = input
                .workspace
                .and_then(|w| w.last_activity_at.clone())
                .or_else(|| input.items.last().map(|i| i.created_at.clone()));

            return ContextLifecycle::new(
                input.context_id,
                ContextLifecycleState::Archived,
                archived_at,
                last_active,
                None,
                None,
                1.0,
                format!("Explicitly archived by user. Workspace '{}' is removed from active matching.", ws_name),
                evidence,
            );
        }

        // 2. Discover Chronological Anchors (last_active_at, last_paused_at, last_resumed_at)
        let mut latest_active_dt: Option<DateTime<Utc>> = None;
        let mut latest_active_str: Option<String> = None;

        let mut update_active = |ts_str: &str| {
            if let Some(dt) = parse_ts(ts_str) {
                if latest_active_dt.is_none() || Some(dt) > latest_active_dt {
                    latest_active_dt = Some(dt);
                    latest_active_str = Some(ts_str.to_string());
                }
            }
        };

        // Telemetry items
        for item in input.items {
            update_active(&item.created_at);
            update_active(&item.updated_at);
        }

        // Segments
        for seg in input.activity_segments {
            if seg.context.as_ref().map(|c| c.id.as_str()) == Some(input.context_id) {
                update_active(&seg.started_at);
                update_active(&seg.ended_at);
            }
        }

        // Workspace DB record
        if let Some(ws) = input.workspace {
            if let Some(ref la) = ws.last_activity_at {
                update_active(la);
            }
        }

        // Context state record
        if let Some(cs) = input.context_state {
            if let Some(ref la) = cs.last_active_at {
                update_active(la);
            }
        }

        // 3. Discover Last Resumed and Last Paused Anchors
        let mut last_resumed_str: Option<String> = input.explicit_resume_timestamp.clone();
        let mut last_paused_str: Option<String> = None;
        let mut latest_resumed_dt: Option<DateTime<Utc>> = input.explicit_resume_timestamp.as_deref().and_then(parse_ts);
        let mut latest_paused_dt: Option<DateTime<Utc>> = None;

        for trans in input.context_transitions {
            let is_to = trans.to_context_id.as_deref() == Some(input.context_id);
            let is_from = trans.from_context_id.as_deref() == Some(input.context_id);

            if let Some(dt) = parse_ts(&trans.detected_at) {
                if is_to && (trans.transition_type == ContextTransitionType::ContextReturn || trans.transition_type == ContextTransitionType::ContextStart) {
                    if latest_resumed_dt.is_none() || Some(dt) > latest_resumed_dt {
                        latest_resumed_dt = Some(dt);
                        last_resumed_str = Some(trans.detected_at.clone());
                    }
                }

                if is_from && trans.transition_type == ContextTransitionType::ContextSwitch {
                    if latest_paused_dt.is_none() || Some(dt) > latest_paused_dt {
                        latest_paused_dt = Some(dt);
                        last_paused_str = Some(trans.detected_at.clone());
                    }
                }
            }
        }

        // Also check task transitions for task end (pause anchor) or resume anchor
        for tt in input.task_transitions {
            if let Some(dt) = parse_ts(&tt.timestamp) {
                if tt.transition_type == TaskTransitionType::TaskEnd {
                    if latest_paused_dt.is_none() || Some(dt) > latest_paused_dt {
                        latest_paused_dt = Some(dt);
                        last_paused_str = Some(tt.timestamp.clone());
                    }
                } else if tt.transition_type == TaskTransitionType::TaskReturn {
                    if latest_resumed_dt.is_none() || Some(dt) > latest_resumed_dt {
                        latest_resumed_dt = Some(dt);
                        last_resumed_str = Some(tt.timestamp.clone());
                    }
                }
            }
        }

        // 4. Calculate Elapsed Inactivity Duration
        let elapsed_seconds = latest_active_dt.map(|dt| (now - dt).num_seconds().max(0));

        // 5. Evaluate Lifecycle State
        // Invariant: Never infer archived from inactivity.
        // Active: <= 15 minutes with active corroboration / active segment, or explicitly resumed.
        // Paused: Switched away, or 15 minutes to 7 days idle.
        // Dormant: > 7 days idle (or > 30 days in DB).
        let (state, confidence, entered_at, reason) = if latest_active_dt.is_none() && !input.has_explicit_user_resume {
            // Zero evidence or brand new
            let entered = input
                .workspace
                .map(|w| w.created_at.clone())
                .unwrap_or_else(|| now.to_rfc3339());
            (
                ContextLifecycleState::Paused,
                0.50,
                entered,
                format!("New context '{}' with no recorded activity yet.", ws_name),
            )
        } else {
            let elapsed = elapsed_seconds.unwrap_or(0);

            // Check if context transitions explicitly switched away from this context
            let latest_context_trans = input.context_transitions.iter()
                .filter(|t| t.from_context_id.as_deref() == Some(input.context_id) || t.to_context_id.as_deref() == Some(input.context_id))
                .max_by_key(|t| {
                    parse_ts(&t.detected_at).unwrap_or_else(|| DateTime::from_timestamp(0, 0).unwrap())
                });

            let explicitly_switched_away = latest_context_trans.map(|t| {
                t.from_context_id.as_deref() == Some(input.context_id)
                    && t.to_context_id.as_deref() != Some(input.context_id)
            }).unwrap_or(false);

            // Check active state from context state service
            let state_is_active = input
                .context_state
                .map(|cs| cs.state == ContextStateValue::Active)
                .unwrap_or(false);

            let explicitly_resumed = input.has_explicit_user_resume;

            if !explicitly_switched_away && (state_is_active || explicitly_resumed || (elapsed <= 900 && !input.items.is_empty())) {
                // Active State: Currently working within context (<= 15 minutes) or user explicitly resumed
                let entered = if let Some(ref r_str) = last_resumed_str {
                    r_str.clone()
                } else if let Some(ref a_str) = latest_active_str {
                    a_str.clone()
                } else {
                    now.to_rfc3339()
                };

                if input.has_explicit_user_resume {
                    evidence.push(LifecycleSourceEvidence {
                        evidence_type: "user_action".to_string(),
                        description: format!("Context '{}' was explicitly resumed by the user.", ws_name),
                        timestamp: input.explicit_resume_timestamp.clone().unwrap_or_else(|| now.to_rfc3339()),
                        confidence: 1.0,
                        reference_id: Some(input.context_id.to_string()),
                    });
                } else {
                    evidence.push(LifecycleSourceEvidence {
                        evidence_type: "telemetry_capture".to_string(),
                        description: format!("Active work verified within immediate 15-minute observation window (last activity {}s ago).", elapsed),
                        timestamp: latest_active_str.clone().unwrap_or_else(|| now.to_rfc3339()),
                        confidence: 0.90,
                        reference_id: None,
                    });
                }

                if let Some(ref r_str) = last_resumed_str {
                    if !input.has_explicit_user_resume {
                        evidence.push(LifecycleSourceEvidence {
                            evidence_type: "context_transition".to_string(),
                            description: format!("Returned to context '{}'.", ws_name),
                            timestamp: r_str.clone(),
                            confidence: 0.88,
                            reference_id: None,
                        });
                    }
                }

                (
                    ContextLifecycleState::Active,
                    if input.has_explicit_user_resume { 1.0 } else { 0.90 },
                    entered,
                    if input.has_explicit_user_resume {
                        format!("Explicitly resumed by user. Context '{}' is active.", ws_name)
                    } else {
                        format!("User is currently working in '{}'.", ws_name)
                    },
                )
            } else {
                // Inactive context: Evaluate multidimensional distinction between PAUSED and DORMANT
                let dormancy_input = DormancyEvaluationInput {
                    context_id: input.context_id,
                    context_name: input.context_name,
                    workspace: input.workspace,
                    items: input.items,
                    activity_segments: input.activity_segments,
                    context_transitions: input.context_transitions,
                    task_transitions: input.task_transitions,
                    active_task_candidate: input.active_task_candidate,
                    workset: input.workset,
                    important_memories: input.important_memories,
                    latest_snapshot: input.latest_snapshot,
                    reference_time: Some(now),
                };

                let distinction = DormancyDistinctionEvaluator::evaluate(dormancy_input, None);

                let entered = if distinction.is_dormant {
                    if let Some(ref dt) = latest_active_dt {
                        (*dt + chrono::Duration::days(7)).to_rfc3339()
                    } else {
                        now.to_rfc3339()
                    }
                } else {
                    if let Some(ref p_str) = last_paused_str {
                        p_str.clone()
                    } else if let Some(ref dt) = latest_active_dt {
                        (*dt + chrono::Duration::minutes(15)).to_rfc3339()
                    } else {
                        now.to_rfc3339()
                    }
                };

                for ev in distinction.source_evidence {
                    evidence.push(ev);
                }

                (
                    distinction.lifecycle_state,
                    distinction.confidence,
                    entered,
                    distinction.reason,
                )
            }
        };

        // Add workspace DB record evidence
        if let Some(ws) = input.workspace {
            evidence.push(LifecycleSourceEvidence {
                evidence_type: "workspace_db_state".to_string(),
                description: format!("Database workspace record: '{}' (lifecycle: {}).", ws.name, ws.lifecycle_state),
                timestamp: ws.updated_at.clone(),
                confidence: 0.95,
                reference_id: Some(ws.id.clone()),
            });
        }

        ContextLifecycle::new(
            input.context_id,
            state,
            entered_at,
            latest_active_str,
            last_paused_str,
            last_resumed_str,
            confidence,
            reason,
            evidence,
        )
    }
}

// =========================================================================
// ContextLifecycleService: Database Query & Retrieval Interface
// =========================================================================

/// Service for querying and evaluating context lifecycles directly from the database.
pub struct ContextLifecycleService;

impl ContextLifecycleService {
    /// Loads and evaluates the `ContextLifecycle` model for a workspace from the database.
    pub fn get_context_lifecycle(conn: &Connection, workspace_id: &str) -> Result<ContextLifecycle, String> {
        let ws = crate::db::operations::get_workspace(conn, workspace_id).map_err(|e| e.to_string())?;

        let mut items = Vec::new();
        if let Ok(mut item_stmt) = conn.prepare(
            "SELECT id, workspace_id, session_id, item_type, content, title, metadata, created_at, updated_at
             FROM items
             WHERE workspace_id = ?1
             ORDER BY created_at DESC LIMIT 50",
        ) {
            if let Ok(item_iter) = item_stmt.query_map(rusqlite::params![workspace_id], |row| {
                Ok(Item {
                    id: row.get(0)?,
                    workspace_id: row.get(1)?,
                    session_id: row.get(2)?,
                    item_type: row.get(3)?,
                    content: row.get(4)?,
                    title: row.get(5)?,
                    metadata: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            }) {
                items = item_iter.filter_map(|r| r.ok()).collect();
            }
        }

        let state_svc = ContextStateService::new();
        let state_res = state_svc.derive_for_context(conn, workspace_id);
        let context_state = state_res.as_ref().ok();

        let context_transitions = crate::db::operations::list_context_transitions_for_context(conn, workspace_id)
            .unwrap_or_default();

        let mut has_explicit_user_resume = false;
        let mut explicit_resume_timestamp: Option<String> = None;
        if let Ok(mut sig_stmt) = conn.prepare(
            "SELECT created_at FROM context_signals
             WHERE workspace_id = ?1 AND signal_type = 'context_resumed'
             ORDER BY created_at DESC LIMIT 1",
        ) {
            if let Ok(mut rows) = sig_stmt.query(rusqlite::params![workspace_id]) {
                if let Ok(Some(row)) = rows.next() {
                    let ts: String = row.get(0).unwrap_or_default();
                    explicit_resume_timestamp = Some(ts);
                    has_explicit_user_resume = true;
                }
            }
        }

        let input = LifecycleEvaluationInput {
            context_id: workspace_id,
            context_name: Some(&ws.name),
            workspace: Some(&ws),
            context_state,
            activity_segments: &[],
            context_transitions: &context_transitions,
            task_transitions: &[],
            active_task_candidate: None,
            workset: None,
            important_memories: &[],
            latest_snapshot: None,
            items: &items,
            has_explicit_user_archive: ws.lifecycle_state.eq_ignore_ascii_case("archived"),
            has_explicit_user_resume,
            explicit_resume_timestamp,
            reference_time: None,
        };

        Ok(ContextLifecycleEvaluator::evaluate(input))
    }
}

// =========================================================================
// Unit & Integration Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_engine::context_state::{ContextResumeState, ResumeReadiness, ResumeReadinessAssessment, StateExplanation, StateFreshness};
    use crate::db::models::ContextTransitionType;

    fn make_test_item(id: &str, ws_id: &str, created_at: &str) -> Item {
        Item {
            id: id.to_string(),
            workspace_id: Some(ws_id.to_string()),
            session_id: Some("sess_1".to_string()),
            item_type: "text".to_string(),
            content: format!("Content for {}", id),
            title: Some(format!("Item {}", id)),
            metadata: "{}".to_string(),
            created_at: created_at.to_string(),
            updated_at: created_at.to_string(),
        }
    }

    fn make_test_workspace(id: &str, name: &str, lifecycle: &str, last_activity: Option<&str>) -> Workspace {
        let ts = last_activity.unwrap_or("2026-09-20T10:00:00Z").to_string();
        Workspace {
            id: id.to_string(),
            name: name.to_string(),
            created_at: ts.clone(),
            updated_at: ts.clone(),
            lifecycle_state: lifecycle.to_string(),
            last_activity_at: last_activity.map(|s| s.to_string()),
            archived_at: if lifecycle == "archived" { Some(ts) } else { None },
        }
    }

    fn make_test_context_state(ws_id: &str, state: ContextStateValue, last_active: &str) -> ContextState {
        ContextState {
            context_id: ws_id.to_string(),
            context_name: Some("Test Context".to_string()),
            task_id: Some("task_1".to_string()),
            task_title: Some("Build Feature".to_string()),
            state,
            freshness: StateFreshness::Fresh,
            freshness_display: "Active recently".to_string(),
            last_active_at: Some(last_active.to_string()),
            last_activity_segment_id: None,
            last_transition_id: None,
            last_meaningful_item_id: None,
            last_meaningful_item_title: None,
            current_workset: None,
            recent_item_ids: vec!["item_1".to_string()],
            recent_activity: Some("1 item recorded".to_string()),
            active_application: Some("VS Code".to_string()),
            active_domain: None,
            recent_applications: vec!["VS Code".to_string()],
            latest_meaningful_action: Some("Working on Build Feature".to_string()),
            last_meaningful_activity: "Working in VS Code.".to_string(),
            meaningful_activity: None,
            resume_state: ContextResumeState {
                context_id: ws_id.to_string(),
                context_name: "Test Context".to_string(),
                left_off_on: Some("Build Feature".to_string()),
                last_meaningful_activity: Some("Working in VS Code.".to_string()),
                last_activity_type: None,
                recent_work: Vec::new(),
                formatted_resume_summary: "Working in VS Code.".to_string(),
                readiness: ResumeReadinessAssessment {
                    readiness: ResumeReadiness::Ready,
                    reasons: vec!["Ready".to_string()],
                    formatted_explanation: "Ready".to_string(),
                },
                state,
                freshness: StateFreshness::Fresh,
                freshness_display: "Active recently".to_string(),
                confidence: 0.90,
                reconstructed_at: last_active.to_string(),
            },
            readiness: ResumeReadinessAssessment {
                readiness: ResumeReadiness::Ready,
                reasons: vec!["Ready".to_string()],
                formatted_explanation: "Ready".to_string(),
            },
            explanation: StateExplanation {
                title: "Explanation".to_string(),
                reasons: Vec::new(),
                uncertainty_note: None,
            },
            state_confidence: 0.90,
            provenance: "observed".to_string(),
            has_user_feedback: false,
            reconstructed_at: last_active.to_string(),
        }
    }

    #[test]
    fn test_lifecycle_active_state_detection() {
        let now = Utc::now();
        let recent_time = (now - chrono::Duration::minutes(5)).to_rfc3339();

        let ws = make_test_workspace("ws_payx", "PayX", "active", Some(&recent_time));
        let item = make_test_item("item_1", "ws_payx", &recent_time);
        let cs = make_test_context_state("ws_payx", ContextStateValue::Active, &recent_time);

        let input = LifecycleEvaluationInput {
            context_id: "ws_payx",
            context_name: Some("PayX"),
            workspace: Some(&ws),
            context_state: Some(&cs),
            activity_segments: &[],
            context_transitions: &[],
            task_transitions: &[],
            active_task_candidate: None,
            workset: None,
            important_memories: &[],
            latest_snapshot: None,
            items: &[item],
            has_explicit_user_archive: false,
            has_explicit_user_resume: false,
            explicit_resume_timestamp: None,
            reference_time: Some(now),
        };

        let lifecycle = ContextLifecycleEvaluator::evaluate(input);
        assert_eq!(lifecycle.lifecycle_state, ContextLifecycleState::Active);
        assert!(lifecycle.confidence >= 0.85);
        assert!(lifecycle.reason.contains("currently working in 'PayX'"));
        assert_eq!(lifecycle.last_active_at, Some(recent_time));
    }

    #[test]
    fn test_lifecycle_paused_state_on_context_switch() {
        let now = Utc::now();
        let switched_time = (now - chrono::Duration::hours(2)).to_rfc3339();

        let ws = make_test_workspace("ws_payx", "PayX", "active", Some(&switched_time));
        let item = make_test_item("item_1", "ws_payx", &switched_time);

        let mut switch_transition = ContextTransition::new(
            Some("ws_payx".to_string()),
            Some("ws_contextsplit".to_string()),
            ContextTransitionType::ContextSwitch,
            0.88,
        );
        switch_transition.detected_at = switched_time.clone();

        let input = LifecycleEvaluationInput {
            context_id: "ws_payx",
            context_name: Some("PayX"),
            workspace: Some(&ws),
            context_state: None,
            activity_segments: &[],
            context_transitions: &[switch_transition],
            task_transitions: &[],
            active_task_candidate: None,
            workset: None,
            important_memories: &[],
            latest_snapshot: None,
            items: &[item],
            has_explicit_user_archive: false,
            has_explicit_user_resume: false,
            explicit_resume_timestamp: None,
            reference_time: Some(now),
        };

        let lifecycle = ContextLifecycleEvaluator::evaluate(input);
        assert_eq!(lifecycle.lifecycle_state, ContextLifecycleState::Paused);
        assert!(lifecycle.reason.contains("temporarily paused and ready to resume") || lifecycle.reason.contains("Paused context"));
        assert_eq!(lifecycle.last_paused_at, Some(switched_time));
    }

    #[test]
    fn test_lifecycle_dormant_state_after_extended_inactivity() {
        let now = Utc::now();
        let old_time = (now - chrono::Duration::days(14)).to_rfc3339();

        let ws = make_test_workspace("ws_tokyo", "Tokyo Trip", "active", Some(&old_time));
        let item = make_test_item("item_tokyo", "ws_tokyo", &old_time);

        let input = LifecycleEvaluationInput {
            context_id: "ws_tokyo",
            context_name: Some("Tokyo Trip"),
            workspace: Some(&ws),
            context_state: None,
            activity_segments: &[],
            context_transitions: &[],
            task_transitions: &[],
            active_task_candidate: None,
            workset: None,
            important_memories: &[],
            latest_snapshot: None,
            items: &[item],
            has_explicit_user_archive: false,
            has_explicit_user_resume: false,
            explicit_resume_timestamp: None,
            reference_time: Some(now),
        };

        let lifecycle = ContextLifecycleEvaluator::evaluate(input);
        assert_eq!(lifecycle.lifecycle_state, ContextLifecycleState::Dormant);
        assert!(lifecycle.reason.contains("Dormant context"));
        assert!(lifecycle.source_evidence.iter().any(|e| e.evidence_type == "dormancy_assessment"));
    }

    #[test]
    fn test_lifecycle_never_infer_archived_from_inactivity() {
        let now = Utc::now();
        // 90 days of inactivity!
        let ancient_time = (now - chrono::Duration::days(90)).to_rfc3339();

        let ws = make_test_workspace("ws_ancient", "Old Project", "active", Some(&ancient_time));
        let item = make_test_item("item_old", "ws_ancient", &ancient_time);

        let input = LifecycleEvaluationInput {
            context_id: "ws_ancient",
            context_name: Some("Old Project"),
            workspace: Some(&ws),
            context_state: None,
            activity_segments: &[],
            context_transitions: &[],
            task_transitions: &[],
            active_task_candidate: None,
            workset: None,
            important_memories: &[],
            latest_snapshot: None,
            items: &[item],
            has_explicit_user_archive: false,
            has_explicit_user_resume: false,
            explicit_resume_timestamp: None,
            reference_time: Some(now),
        };

        let lifecycle = ContextLifecycleEvaluator::evaluate(input);
        // Invariant: Inactivity MUST produce Dormant, NEVER Archived!
        assert_ne!(lifecycle.lifecycle_state, ContextLifecycleState::Archived);
        assert_eq!(lifecycle.lifecycle_state, ContextLifecycleState::Dormant);
    }

    #[test]
    fn test_lifecycle_archived_requires_explicit_user_action() {
        let now = Utc::now();
        let active_time = (now - chrono::Duration::minutes(2)).to_rfc3339();

        // Explicitly archived workspace
        let ws = make_test_workspace("ws_archived", "Archived Project", "archived", Some(&active_time));
        let item = make_test_item("item_1", "ws_archived", &active_time);

        let input = LifecycleEvaluationInput {
            context_id: "ws_archived",
            context_name: Some("Archived Project"),
            workspace: Some(&ws),
            context_state: None,
            activity_segments: &[],
            context_transitions: &[],
            task_transitions: &[],
            active_task_candidate: None,
            workset: None,
            important_memories: &[],
            latest_snapshot: None,
            items: &[item],
            has_explicit_user_archive: true,
            has_explicit_user_resume: false,
            explicit_resume_timestamp: None,
            reference_time: Some(now),
        };

        let lifecycle = ContextLifecycleEvaluator::evaluate(input);
        assert_eq!(lifecycle.lifecycle_state, ContextLifecycleState::Archived);
        assert_eq!(lifecycle.confidence, 1.0);
        assert!(lifecycle.reason.contains("Explicitly archived by user"));
    }

    #[test]
    fn test_lifecycle_resumed_timestamp_on_context_return() {
        let now = Utc::now();
        let resume_time = (now - chrono::Duration::minutes(3)).to_rfc3339();

        let ws = make_test_workspace("ws_payx", "PayX", "active", Some(&resume_time));
        let item = make_test_item("item_1", "ws_payx", &resume_time);

        let mut return_transition = ContextTransition::new(
            Some("ws_contextsplit".to_string()),
            Some("ws_payx".to_string()),
            ContextTransitionType::ContextReturn,
            0.92,
        );
        return_transition.detected_at = resume_time.clone();

        let input = LifecycleEvaluationInput {
            context_id: "ws_payx",
            context_name: Some("PayX"),
            workspace: Some(&ws),
            context_state: None,
            activity_segments: &[],
            context_transitions: &[return_transition],
            task_transitions: &[],
            active_task_candidate: None,
            workset: None,
            important_memories: &[],
            latest_snapshot: None,
            items: &[item],
            has_explicit_user_archive: false,
            has_explicit_user_resume: false,
            explicit_resume_timestamp: None,
            reference_time: Some(now),
        };

        let lifecycle = ContextLifecycleEvaluator::evaluate(input);
        assert_eq!(lifecycle.lifecycle_state, ContextLifecycleState::Active);
        assert_eq!(lifecycle.last_resumed_at, Some(resume_time));
        assert!(lifecycle.source_evidence.iter().any(|e| e.evidence_type == "context_transition"));
    }

    #[test]
    fn test_lifecycle_json_serialization_roundtrip() {
        let lifecycle = ContextLifecycle::new(
            "ws_payx",
            ContextLifecycleState::Active,
            "2026-09-22T00:00:00Z",
            Some("2026-09-22T00:00:00Z".to_string()),
            Some("2026-09-21T18:00:00Z".to_string()),
            Some("2026-09-22T00:00:00Z".to_string()),
            0.95,
            "User is actively editing payment frontend.",
            vec![LifecycleSourceEvidence {
                evidence_type: "telemetry_capture".to_string(),
                description: "Observed active VS Code window in PayX repository".to_string(),
                timestamp: "2026-09-22T00:00:00Z".to_string(),
                confidence: 0.95,
                reference_id: Some("item_123".to_string()),
            }],
        );

        let json = serde_json::to_string(&lifecycle).expect("Serialization failed");
        assert!(json.contains("\"lifecycle_state\":\"active\""));
        assert!(json.contains("\"context_id\":\"ws_payx\""));

        let deserialized: ContextLifecycle = serde_json::from_str(&json).expect("Deserialization failed");
        assert_eq!(deserialized, lifecycle);
    }

    #[test]
    fn test_multi_context_lifecycle_transitions_simulation() {
        let now = Utc::now();
        let t1 = (now - chrono::Duration::minutes(45)).to_rfc3339();
        let t2 = (now - chrono::Duration::minutes(30)).to_rfc3339();
        let t3 = (now - chrono::Duration::minutes(15)).to_rfc3339();
        let t4 = (now - chrono::Duration::minutes(2)).to_rfc3339();

        // 1. Contexts Setup
        let ws_split = make_test_workspace("ws_split", "ContextSplit", "active", Some(&t1));
        let ws_payx = make_test_workspace("ws_payx", "PayX", "active", Some(&t2));
        let ws_univ = make_test_workspace("ws_univ", "University OS", "active", Some(&t3));

        let items_split = vec![
            make_test_item("item_split_1", "ws_split", &t1),
            make_test_item("item_split_2", "ws_split", &t4),
        ];
        let items_payx = vec![
            make_test_item("item_payx_1", "ws_payx", &t2),
        ];
        let items_univ = vec![
            make_test_item("item_univ_1", "ws_univ", &t3),
        ];

        // ---------------------------------------------------------------------
        // Transition 1: ContextSplit -> PayX
        // ---------------------------------------------------------------------
        let mut trans_1 = ContextTransition::new(
            Some("ws_split".to_string()),
            Some("ws_payx".to_string()),
            ContextTransitionType::ContextSwitch,
            0.92,
        );
        trans_1.detected_at = t2.clone();

        // Evaluate ContextSplit after switch to PayX
        let input_split_after_payx = LifecycleEvaluationInput {
            context_id: "ws_split",
            context_name: Some("ContextSplit"),
            workspace: Some(&ws_split),
            context_state: None,
            activity_segments: &[],
            context_transitions: &[trans_1.clone()],
            task_transitions: &[],
            active_task_candidate: None,
            workset: None,
            important_memories: &[],
            latest_snapshot: None,
            items: &items_split[0..1],
            has_explicit_user_archive: false,
            has_explicit_user_resume: false,
            explicit_resume_timestamp: None,
            reference_time: Some(now - chrono::Duration::minutes(29)),
        };
        let lc_split_p1 = ContextLifecycleEvaluator::evaluate(input_split_after_payx);
        assert_eq!(lc_split_p1.lifecycle_state, ContextLifecycleState::Paused);

        // Evaluate PayX when active
        let input_payx_active = LifecycleEvaluationInput {
            context_id: "ws_payx",
            context_name: Some("PayX"),
            workspace: Some(&ws_payx),
            context_state: None,
            activity_segments: &[],
            context_transitions: &[trans_1.clone()],
            task_transitions: &[],
            active_task_candidate: None,
            workset: None,
            important_memories: &[],
            latest_snapshot: None,
            items: &items_payx,
            has_explicit_user_archive: false,
            has_explicit_user_resume: false,
            explicit_resume_timestamp: None,
            reference_time: Some(now - chrono::Duration::minutes(29)),
        };
        let lc_payx_p1 = ContextLifecycleEvaluator::evaluate(input_payx_active);
        assert_eq!(lc_payx_p1.lifecycle_state, ContextLifecycleState::Active);

        // ---------------------------------------------------------------------
        // Transition 2: PayX -> University OS
        // ---------------------------------------------------------------------
        let mut trans_2 = ContextTransition::new(
            Some("ws_payx".to_string()),
            Some("ws_univ".to_string()),
            ContextTransitionType::ContextSwitch,
            0.90,
        );
        trans_2.detected_at = t3.clone();

        // Evaluate PayX after switch to University OS
        let input_payx_after_univ = LifecycleEvaluationInput {
            context_id: "ws_payx",
            context_name: Some("PayX"),
            workspace: Some(&ws_payx),
            context_state: None,
            activity_segments: &[],
            context_transitions: &[trans_1.clone(), trans_2.clone()],
            task_transitions: &[],
            active_task_candidate: None,
            workset: None,
            important_memories: &[],
            latest_snapshot: None,
            items: &items_payx,
            has_explicit_user_archive: false,
            has_explicit_user_resume: false,
            explicit_resume_timestamp: None,
            reference_time: Some(now - chrono::Duration::minutes(14)),
        };
        let lc_payx_p2 = ContextLifecycleEvaluator::evaluate(input_payx_after_univ);
        assert_eq!(lc_payx_p2.lifecycle_state, ContextLifecycleState::Paused);

        // Evaluate University OS when active
        let input_univ_active = LifecycleEvaluationInput {
            context_id: "ws_univ",
            context_name: Some("University OS"),
            workspace: Some(&ws_univ),
            context_state: None,
            activity_segments: &[],
            context_transitions: &[trans_1.clone(), trans_2.clone()],
            task_transitions: &[],
            active_task_candidate: None,
            workset: None,
            important_memories: &[],
            latest_snapshot: None,
            items: &items_univ,
            has_explicit_user_archive: false,
            has_explicit_user_resume: false,
            explicit_resume_timestamp: None,
            reference_time: Some(now - chrono::Duration::minutes(14)),
        };
        let lc_univ_p2 = ContextLifecycleEvaluator::evaluate(input_univ_active);
        assert_eq!(lc_univ_p2.lifecycle_state, ContextLifecycleState::Active);

        // ---------------------------------------------------------------------
        // Transition 3: University OS -> ContextSplit
        // ---------------------------------------------------------------------
        let mut trans_3 = ContextTransition::new(
            Some("ws_univ".to_string()),
            Some("ws_split".to_string()),
            ContextTransitionType::ContextReturn,
            0.95,
        );
        trans_3.detected_at = t4.clone();

        // ContextSplit becomes ACTIVE
        let input_split_returned = LifecycleEvaluationInput {
            context_id: "ws_split",
            context_name: Some("ContextSplit"),
            workspace: Some(&ws_split),
            context_state: None,
            activity_segments: &[],
            context_transitions: &[trans_1.clone(), trans_2.clone(), trans_3.clone()],
            task_transitions: &[],
            active_task_candidate: None,
            workset: None,
            important_memories: &[],
            latest_snapshot: None,
            items: &items_split,
            has_explicit_user_archive: false,
            has_explicit_user_resume: false,
            explicit_resume_timestamp: None,
            reference_time: Some(now),
        };
        let lc_split_final = ContextLifecycleEvaluator::evaluate(input_split_returned);
        assert_eq!(lc_split_final.lifecycle_state, ContextLifecycleState::Active);

        // PayX remains PAUSED
        let input_payx_final = LifecycleEvaluationInput {
            context_id: "ws_payx",
            context_name: Some("PayX"),
            workspace: Some(&ws_payx),
            context_state: None,
            activity_segments: &[],
            context_transitions: &[trans_1.clone(), trans_2.clone(), trans_3.clone()],
            task_transitions: &[],
            active_task_candidate: None,
            workset: None,
            important_memories: &[],
            latest_snapshot: None,
            items: &items_payx,
            has_explicit_user_archive: false,
            has_explicit_user_resume: false,
            explicit_resume_timestamp: None,
            reference_time: Some(now),
        };
        let lc_payx_final = ContextLifecycleEvaluator::evaluate(input_payx_final);
        assert_eq!(lc_payx_final.lifecycle_state, ContextLifecycleState::Paused);

        // University OS becomes PAUSED
        let input_univ_final = LifecycleEvaluationInput {
            context_id: "ws_univ",
            context_name: Some("University OS"),
            workspace: Some(&ws_univ),
            context_state: None,
            activity_segments: &[],
            context_transitions: &[trans_1, trans_2, trans_3],
            task_transitions: &[],
            active_task_candidate: None,
            workset: None,
            important_memories: &[],
            latest_snapshot: None,
            items: &items_univ,
            has_explicit_user_archive: false,
            has_explicit_user_resume: false,
            explicit_resume_timestamp: None,
            reference_time: Some(now),
        };
        let lc_univ_final = ContextLifecycleEvaluator::evaluate(input_univ_final);
        assert_eq!(lc_univ_final.lifecycle_state, ContextLifecycleState::Paused);
    }

    #[test]
    fn test_long_dormant_context_resumption_simulation() {
        let now = Utc::now();
        let t_old = (now - chrono::Duration::days(25)).to_rfc3339();
        let t_new = (now - chrono::Duration::minutes(2)).to_rfc3339();

        let ws = make_test_workspace("ws_tokyo", "Tokyo Trip", "active", Some(&t_old));
        let items_hist = vec![
            make_test_item("item_flight", "ws_tokyo", &t_old),
            make_test_item("item_hotel", "ws_tokyo", &t_old),
            make_test_item("item_itinerary", "ws_tokyo", &t_old),
            make_test_item("item_map", "ws_tokyo", &t_old),
        ];

        // 1. Inactivity period -> Dormant
        let input_dormant = LifecycleEvaluationInput {
            context_id: "ws_tokyo",
            context_name: Some("Tokyo Trip"),
            workspace: Some(&ws),
            context_state: None,
            activity_segments: &[],
            context_transitions: &[],
            task_transitions: &[],
            active_task_candidate: None,
            workset: None,
            important_memories: &[],
            latest_snapshot: None,
            items: &items_hist,
            has_explicit_user_archive: false,
            has_explicit_user_resume: false,
            explicit_resume_timestamp: None,
            reference_time: Some(now - chrono::Duration::days(1)),
        };
        let lc_dormant = ContextLifecycleEvaluator::evaluate(input_dormant);
        assert_eq!(lc_dormant.lifecycle_state, ContextLifecycleState::Dormant);

        // 2. Strong new activity -> Active
        let mut items_resumed = items_hist.clone();
        items_resumed.push(make_test_item("item_ghibli", "ws_tokyo", &t_new));

        let mut resume_trans = ContextTransition::new(
            None,
            Some("ws_tokyo".to_string()),
            ContextTransitionType::ContextReturn,
            0.95,
        );
        resume_trans.detected_at = t_new.clone();

        let input_resumed = LifecycleEvaluationInput {
            context_id: "ws_tokyo",
            context_name: Some("Tokyo Trip"),
            workspace: Some(&ws),
            context_state: None,
            activity_segments: &[],
            context_transitions: &[resume_trans],
            task_transitions: &[],
            active_task_candidate: None,
            workset: None,
            important_memories: &[],
            latest_snapshot: None,
            items: &items_resumed,
            has_explicit_user_archive: false,
            has_explicit_user_resume: false,
            explicit_resume_timestamp: None,
            reference_time: Some(now),
        };
        let lc_resumed = ContextLifecycleEvaluator::evaluate(input_resumed);
        assert_eq!(lc_resumed.lifecycle_state, ContextLifecycleState::Active);
        assert_eq!(lc_resumed.last_resumed_at, Some(t_new));
    }

    #[test]
    fn test_explicit_user_archive_lifecycle_evaluation() {
        let now = Utc::now();
        let t_archive = now.to_rfc3339();
        let ws = make_test_workspace("ws_arch", "Retired Project", "archived", Some(&t_archive));
        let items = vec![make_test_item("item_1", "ws_arch", &t_archive)];

        let input = LifecycleEvaluationInput {
            context_id: "ws_arch",
            context_name: Some("Retired Project"),
            workspace: Some(&ws),
            context_state: None,
            activity_segments: &[],
            context_transitions: &[],
            task_transitions: &[],
            active_task_candidate: None,
            workset: None,
            important_memories: &[],
            latest_snapshot: None,
            items: &items,
            has_explicit_user_archive: true,
            has_explicit_user_resume: false,
            explicit_resume_timestamp: None,
            reference_time: Some(now),
        };

        let lc = ContextLifecycleEvaluator::evaluate(input);
        assert_eq!(lc.lifecycle_state, ContextLifecycleState::Archived);
        assert!(lc.reason.contains("Explicitly archived by user"));
        assert_eq!(lc.confidence, 1.0);
        assert!(lc.source_evidence.iter().any(|e| e.evidence_type == "user_action"));
    }

    #[test]
    fn test_inactivity_alone_never_infers_archived() {
        let now = Utc::now();
        // 180 days of complete inactivity
        let t_very_old = (now - chrono::Duration::days(180)).to_rfc3339();
        let ws = make_test_workspace("ws_dormant", "Very Old Project", "active", Some(&t_very_old));
        let items = vec![make_test_item("item_old", "ws_dormant", &t_very_old)];

        let input = LifecycleEvaluationInput {
            context_id: "ws_dormant",
            context_name: Some("Very Old Project"),
            workspace: Some(&ws),
            context_state: None,
            activity_segments: &[],
            context_transitions: &[],
            task_transitions: &[],
            active_task_candidate: None,
            workset: None,
            important_memories: &[],
            latest_snapshot: None,
            items: &items,
            has_explicit_user_archive: false,
            has_explicit_user_resume: false,
            explicit_resume_timestamp: None,
            reference_time: Some(now),
        };

        let lc = ContextLifecycleEvaluator::evaluate(input);
        // CRITICAL INVARIANT: Inactivity alone MUST derive Dormant, NEVER Archived!
        assert_eq!(lc.lifecycle_state, ContextLifecycleState::Dormant);
        assert_ne!(lc.lifecycle_state, ContextLifecycleState::Archived);
        assert!(lc.lifecycle_state.allows_automated_matching());
    }

    #[test]
    fn test_explicit_user_resume_from_dormant_evaluation() {
        let now = Utc::now();
        let t_old = (now - chrono::Duration::days(60)).to_rfc3339();
        let t_resume = (now - chrono::Duration::minutes(1)).to_rfc3339();

        let ws = make_test_workspace("ws_revived", "Revived Project", "active", Some(&t_resume));
        let items = vec![make_test_item("item_old", "ws_revived", &t_old)];

        let input = LifecycleEvaluationInput {
            context_id: "ws_revived",
            context_name: Some("Revived Project"),
            workspace: Some(&ws),
            context_state: None,
            activity_segments: &[],
            context_transitions: &[],
            task_transitions: &[],
            active_task_candidate: None,
            workset: None,
            important_memories: &[],
            latest_snapshot: None,
            items: &items,
            has_explicit_user_archive: false,
            has_explicit_user_resume: true,
            explicit_resume_timestamp: Some(t_resume.clone()),
            reference_time: Some(now),
        };

        let lc = ContextLifecycleEvaluator::evaluate(input);
        assert_eq!(lc.lifecycle_state, ContextLifecycleState::Active);
        assert_eq!(lc.last_resumed_at, Some(t_resume));
        assert_eq!(lc.confidence, 1.0);
        assert!(lc.source_evidence.iter().any(|e| e.evidence_type == "user_action"));
    }
}

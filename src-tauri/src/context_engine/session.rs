use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::context_engine::memory::InformationProvenance;
use crate::context_engine::snapshot::ContextSnapshot;
use crate::db::models::{Item, TaskCandidate};

// =========================================================================
// Epistemic Disclaimer
// =========================================================================

pub const CONTEXT_SESSION_DISCLAIMER: &str =
    "This ContextSession represents an algorithmic synthesis of a meaningful period of work within a context. It distinguishes clean completions, graceful pauses, interruptions, and resumptions without fabricating artificial timestamps.";

fn default_session_disclaimer() -> String {
    CONTEXT_SESSION_DISCLAIMER.to_string()
}

// =========================================================================
// ContextSessionState: 6 Core Intelligent Session States
// =========================================================================

/// Semantic working state of a contextual session.
///
/// Invariants:
/// - `Active`: The session is currently underway with recent continuous activity.
/// - `Completed`: The session was cleanly concluded (e.g., active task was completed or clean session end).
/// - `Paused`: The session was gracefully suspended (e.g. detour, micro-break, or context switch with snapshot).
/// - `Interrupted`: The session ended unexpectedly or abruptly (e.g. crash, power loss, sudden prolonged silence).
///   CRITICAL: `ended_at` MUST NOT be fabricated when a session is Interrupted.
/// - `Resumed`: The session segment specifically represents resumed work following a prior pause or dormancy.
/// - `Partial`: The session contains minimal, noisy, or ambiguous activity insufficient to confirm a complete session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextSessionState {
    /// Actively receiving continuous contextual activity.
    Active,
    /// Cleanly finished with completed task milestones or explicit session closure.
    Completed,
    /// Gracefully paused with departure snapshot or pause detection.
    Paused,
    /// Terminated abruptly without clean closure or departure checkpoint.
    Interrupted,
    /// Resumed working session segment following prior pause/dormancy.
    Resumed,
    /// Scant, ambiguous, or low-evidence activity segment (e.g., isolated clicks or momentary app glance).
    Partial,
}

impl ContextSessionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Completed => "completed",
            Self::Paused => "paused",
            Self::Interrupted => "interrupted",
            Self::Resumed => "resumed",
            Self::Partial => "partial",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Active => "Active Session",
            Self::Completed => "Completed Session",
            Self::Paused => "Paused Session",
            Self::Interrupted => "Interrupted Session",
            Self::Resumed => "Resumed Session",
            Self::Partial => "Partial Session",
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    pub fn is_completed(&self) -> bool {
        matches!(self, Self::Completed)
    }

    pub fn is_paused(&self) -> bool {
        matches!(self, Self::Paused)
    }

    pub fn is_interrupted(&self) -> bool {
        matches!(self, Self::Interrupted)
    }

    pub fn is_resumed(&self) -> bool {
        matches!(self, Self::Resumed)
    }

    pub fn is_partial(&self) -> bool {
        matches!(self, Self::Partial)
    }
}

impl std::fmt::Display for ContextSessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// =========================================================================
// SessionTaskReference: Preserved Task Association
// =========================================================================

/// Lightweight reference to a task candidate or active objective associated with a session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionTaskReference {
    /// Task identifier (nullable if candidate is inferred from session telemetry)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// Task title or focal objective (e.g. "Context Reconstruction", "Fix Auth Pipeline")
    pub task_name: String,
    /// Confidence score [0.0, 1.0]
    pub confidence: f64,
    /// Whether this task was completed during the session
    pub is_completed: bool,
    /// Epistemic provenance
    pub provenance: InformationProvenance,
}

// =========================================================================
// SessionSnapshotReference: Preserved Snapshot Association
// =========================================================================

/// Lightweight reference to a departure or milestone snapshot attached to a session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionSnapshotReference {
    /// Unique snapshot identifier
    pub snapshot_id: String,
    /// ISO-8601 timestamp when snapshot was captured
    pub created_at: String,
    /// Quality rating of the snapshot ("HighQuality", "Acceptable", "Degraded")
    pub quality_rating: String,
    /// Total workset items captured in snapshot
    pub workset_count: usize,
    /// Resumability assessment
    pub is_resumable: bool,
    /// Concise snapshot summary statement
    pub summary: String,
}

impl SessionSnapshotReference {
    pub fn from_snapshot(snapshot: &ContextSnapshot) -> Self {
        Self {
            snapshot_id: snapshot.id.clone(),
            created_at: snapshot.created_at.clone(),
            quality_rating: snapshot.quality.rating.as_str().to_string(),
            workset_count: snapshot.workset_item_references.len(),
            is_resumable: !snapshot.resume_readiness.readiness.is_insufficient(),
            summary: snapshot.snapshot_statement.clone(),
        }
    }
}

// =========================================================================
// ContextSession: Primary Contextual Session Model
// =========================================================================

/// Structured representation of a meaningful period of contextual work.
///
/// Invariants:
/// 1. A session represents genuine contextual work, NOT raw application focus changes or tiny noise bursts.
/// 2. Does NOT fabricate an `ended_at` timestamp if the session was interrupted unexpectedly.
/// 3. Maintains explicit state classification: Active, Completed, Paused, Interrupted, Resumed, Partial.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextSession {
    /// Canonical session identifier (e.g. "sess_20260922_001")
    pub session_id: String,
    /// Associated workspace / context identifier
    pub context_id: String,
    /// Associated workspace name (e.g. "ContextSplit")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_name: Option<String>,
    /// ISO-8601 timestamp when this session began
    pub started_at: String,
    /// ISO-8601 timestamp when this session cleanly ended (None if Active or Interrupted!)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    /// ISO-8601 timestamp of the most recent recorded activity in this session
    pub last_activity_at: String,
    /// Total duration of the session in seconds (from start to end, or start to last_activity)
    pub duration_seconds: i64,
    /// Human-friendly duration string (e.g. "45m 12s", "1h 30m", "15s")
    pub duration_formatted: String,
    /// Associated active or completed task
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_reference: Option<SessionTaskReference>,
    /// Total count of verified meaningful contextual events (excluding noise/focus hops)
    pub meaningful_activity_count: usize,
    /// Total raw telemetry items associated with session
    pub total_item_count: usize,
    /// Reference to departure checkpoint snapshot if available
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_reference: Option<SessionSnapshotReference>,
    /// Semantic session state
    pub session_state: ContextSessionState,
    /// Confidence calibration score in this session model [0.0, 1.0]
    pub confidence: f64,
    /// Whether this session was flagged as a low-evidence micro-burst (< 3 events)
    pub is_micro_session: bool,
    /// Structured diagnostic and metadata attributes
    pub metadata: Value,
    /// Epistemic disclaimer
    #[serde(default = "default_session_disclaimer")]
    pub disclaimer: String,
}

// =========================================================================
// SessionIntelligenceConfig
// =========================================================================

/// Calibration parameters for ContextSession intelligence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionIntelligenceConfig {
    /// Minimum count of meaningful activity events to qualify as a full session (default: 3).
    pub min_meaningful_activity_for_session: usize,
    /// Timeout in seconds of unexpected silence after which an unclosed session is marked Interrupted (default: 1800s / 30m).
    pub interruption_timeout_secs: i64,
    /// Minimum active window in seconds before a session is eligible for Completed status (default: 60s).
    pub min_duration_for_completion_secs: i64,
    /// Active threshold in seconds (default: 900s / 15m).
    pub active_recency_threshold_secs: i64,
}

impl Default for SessionIntelligenceConfig {
    fn default() -> Self {
        Self {
            min_meaningful_activity_for_session: 3,
            interruption_timeout_secs: 1800, // 30 minutes
            min_duration_for_completion_secs: 60,
            active_recency_threshold_secs: 900, // 15 minutes
        }
    }
}

// =========================================================================
// SessionEvaluationInput
// =========================================================================

/// Input parameters for evaluating and constructing a `ContextSession`.
#[derive(Debug, Clone)]
pub struct SessionEvaluationInput<'a> {
    pub session_id: &'a str,
    pub context_id: &'a str,
    pub context_name: Option<&'a str>,
    pub raw_started_at: &'a str,
    pub raw_last_activity_at: &'a str,
    pub raw_ended_at: Option<&'a str>,
    pub items: &'a [Item],
    pub task_candidate: Option<&'a TaskCandidate>,
    pub is_task_completed: bool,
    pub has_pause_checkpoint: bool,
    pub is_resumed_context: bool,
    pub latest_snapshot: Option<&'a ContextSnapshot>,
    pub reference_time: Option<DateTime<Utc>>,
}

// =========================================================================
// ContextSessionEvaluator & Helper Functions
// =========================================================================

pub struct ContextSessionEvaluator;

impl ContextSessionEvaluator {
    /// Filters raw items into meaningful contextual activity events, removing
    /// trivial window focus hops, empty captures, and generic background noise.
    pub fn filter_meaningful_activity(items: &[Item]) -> Vec<&Item> {
        items
            .iter()
            .filter(|it| {
                // 1. Exclude empty items
                if it.content.trim().is_empty() {
                    return false;
                }

                // 2. Parse metadata
                if let Ok(meta_val) = serde_json::from_str::<Value>(&it.metadata) {
                    // Exclude isolated focus hops without active document or work
                    let is_focus_hop_only = meta_val
                        .get("event_type")
                        .and_then(|v| v.as_str())
                        .map(|s| s == "focus_change" || s == "app_activated")
                        .unwrap_or(false);

                    let has_content_payload = meta_val.get("file_path").is_some()
                        || meta_val.get("url").is_some()
                        || it.content.len() > 20;

                    if is_focus_hop_only && !has_content_payload {
                        return false;
                    }
                }

                true
            })
            .collect()
    }

    /// Evaluates input telemetry and determines the true semantic `ContextSessionState`.
    ///
    /// Invariant: Does NOT fabricate `ended_at` when `Interrupted`.
    pub fn evaluate_session(
        input: SessionEvaluationInput<'_>,
        config: Option<&SessionIntelligenceConfig>,
    ) -> ContextSession {
        let default_cfg = SessionIntelligenceConfig::default();
        let cfg = config.unwrap_or(&default_cfg);
        let now = input.reference_time.unwrap_or_else(Utc::now);

        let start_dt = parse_timestamp_utc(input.raw_started_at);
        let last_act_dt = parse_timestamp_utc(input.raw_last_activity_at);
        let end_dt = input.raw_ended_at.map(parse_timestamp_utc);

        // Meaningful activity filtering
        let meaningful_items = Self::filter_meaningful_activity(input.items);
        let meaningful_count = meaningful_items.len();
        let total_count = input.items.len();

        let is_micro = meaningful_count < cfg.min_meaningful_activity_for_session;

        let seconds_since_last_act = (now - last_act_dt).num_seconds().max(0);

        // Determine session duration
        let effective_end_dt = end_dt.unwrap_or(last_act_dt);
        let duration_seconds = (effective_end_dt - start_dt).num_seconds().max(0);
        let duration_formatted = format_session_duration(duration_seconds);

        // ---------------------------------------------------------------------
        // Semantic State Determination
        // ---------------------------------------------------------------------
        let (state, effective_ended_at) = if meaningful_count == 0 && input.raw_ended_at.is_none() {
            // Zero meaningful activity -> Partial micro-noise session
            (ContextSessionState::Partial, None)
        } else if input.is_resumed_context {
            // Resumed working session segment
            if input.raw_ended_at.is_some() {
                (ContextSessionState::Completed, input.raw_ended_at.map(|s| s.to_string()))
            } else if seconds_since_last_act <= cfg.active_recency_threshold_secs {
                (ContextSessionState::Active, None)
            } else if input.has_pause_checkpoint {
                (ContextSessionState::Paused, Some(input.raw_last_activity_at.to_string()))
            } else {
                // Invariant: Interrupted -> Do NOT fabricate ended_at
                (ContextSessionState::Interrupted, None)
            }
        } else if input.has_pause_checkpoint {
            // Explicit pause checkpoint detected
            (ContextSessionState::Paused, input.raw_ended_at.map(|s| s.to_string()).or_else(|| Some(input.raw_last_activity_at.to_string())))
        } else if input.is_task_completed || (input.raw_ended_at.is_some() && duration_seconds >= cfg.min_duration_for_completion_secs) {
            // Clean completion
            (ContextSessionState::Completed, input.raw_ended_at.map(|s| s.to_string()).or_else(|| Some(input.raw_last_activity_at.to_string())))
        } else if input.raw_ended_at.is_none() && seconds_since_last_act <= cfg.active_recency_threshold_secs {
            // Active session
            (ContextSessionState::Active, None)
        } else if input.raw_ended_at.is_none() && seconds_since_last_act > cfg.interruption_timeout_secs {
            // Unexpected termination / long silence without clean end -> INTERRUPTED
            // CRITICAL INVARIANT: ended_at MUST BE NONE (Not fabricated!)
            (ContextSessionState::Interrupted, None)
        } else if let Some(ended) = input.raw_ended_at {
            (ContextSessionState::Completed, Some(ended.to_string()))
        } else {
            (ContextSessionState::Partial, None)
        };

        // Build Task Reference
        let task_reference = input.task_candidate.map(|tc| SessionTaskReference {
            task_id: Some(tc.id.clone()),
            task_name: tc.title.clone(),
            confidence: tc.confidence,
            is_completed: input.is_task_completed,
            provenance: InformationProvenance::Observed,
        });

        // Build Snapshot Reference
        let snapshot_reference = input.latest_snapshot.map(SessionSnapshotReference::from_snapshot);

        // Confidence calculation
        let base_confidence = if is_micro {
            0.45
        } else if meaningful_count >= 10 {
            0.95
        } else {
            0.75 + (meaningful_count as f64 * 0.02)
        };

        ContextSession {
            session_id: input.session_id.to_string(),
            context_id: input.context_id.to_string(),
            context_name: input.context_name.map(|s| s.to_string()),
            started_at: input.raw_started_at.to_string(),
            ended_at: effective_ended_at,
            last_activity_at: input.raw_last_activity_at.to_string(),
            duration_seconds,
            duration_formatted,
            task_reference,
            meaningful_activity_count: meaningful_count,
            total_item_count: total_count,
            snapshot_reference,
            session_state: state,
            confidence: base_confidence.clamp(0.0, 1.0),
            is_micro_session: is_micro,
            metadata: serde_json::json!({
                "seconds_since_last_activity": seconds_since_last_act,
                "meaningful_ratio": if total_count > 0 { meaningful_count as f64 / total_count as f64 } else { 0.0 },
                "has_pause_checkpoint": input.has_pause_checkpoint,
                "is_task_completed": input.is_task_completed,
            }),
            disclaimer: default_session_disclaimer(),
        }
    }
}

// =========================================================================
// Duration & Timestamp Helpers
// =========================================================================

pub(crate) fn parse_timestamp_utc(ts: &str) -> DateTime<Utc> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(ts) {
        return dt.with_timezone(&Utc);
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S") {
        return DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc);
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%SZ") {
        return DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc);
    }
    Utc::now()
}

pub fn format_session_duration(seconds: i64) -> String {
    let s = seconds.max(0);
    let hours = s / 3600;
    let minutes = (s % 3600) / 60;
    let rem_seconds = s % 60;

    if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, rem_seconds)
    } else {
        format!("{}s", rem_seconds)
    }
}

// =========================================================================
// Unit Tests: Context Session Model & Intelligence
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_engine::context_state::{ContextStateValue, ResumeReadiness, ResumeReadinessAssessment, StateFreshness};
    use crate::context_engine::snapshot::ContextSnapshotQuality;
    use crate::db::models::TaskCandidateStatus;

    fn make_test_item(id: &str, title: &str, content: &str, event_type: &str, file_path: Option<&str>) -> Item {
        Item {
            id: id.to_string(),
            workspace_id: Some("ws_contextsplit".to_string()),
            session_id: Some("session_123".to_string()),
            item_type: "file".to_string(),
            content: content.to_string(),
            title: Some(title.to_string()),
            metadata: serde_json::json!({
                "app_name": "Visual Studio Code",
                "event_type": event_type,
                "file_path": file_path,
            })
            .to_string(),
            created_at: "2026-09-22T00:00:00Z".to_string(),
            updated_at: "2026-09-22T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_active_session_evaluation() {
        let items = vec![
            make_test_item("1", "mod1.rs", "pub mod test;", "document_edit", Some("/src/mod1.rs")),
            make_test_item("2", "mod2.rs", "fn run() {}", "document_edit", Some("/src/mod2.rs")),
            make_test_item("3", "mod3.rs", "struct State;", "document_edit", Some("/src/mod3.rs")),
        ];

        let ref_now = DateTime::parse_from_rfc3339("2026-09-22T00:10:00Z").unwrap().with_timezone(&Utc);

        let input = SessionEvaluationInput {
            session_id: "sess_active",
            context_id: "ws_contextsplit",
            context_name: Some("ContextSplit"),
            raw_started_at: "2026-09-22T00:00:00Z",
            raw_last_activity_at: "2026-09-22T00:08:00Z", // 2 min ago (within 15 min active threshold)
            raw_ended_at: None,
            items: &items,
            task_candidate: None,
            is_task_completed: false,
            has_pause_checkpoint: false,
            is_resumed_context: false,
            latest_snapshot: None,
            reference_time: Some(ref_now),
        };

        let session = ContextSessionEvaluator::evaluate_session(input, None);

        assert_eq!(session.session_state, ContextSessionState::Active);
        assert!(session.ended_at.is_none());
        assert_eq!(session.meaningful_activity_count, 3);
        assert_eq!(session.duration_seconds, 480);
        assert_eq!(session.duration_formatted, "8m 0s");
        assert!(!session.is_micro_session);
    }

    #[test]
    fn test_completed_session_with_ended_at() {
        let items = vec![
            make_test_item("1", "auth.rs", "pub fn login() {}", "document_edit", Some("/src/auth.rs")),
            make_test_item("2", "auth_test.rs", "#[test] fn test_login()", "document_edit", Some("/src/auth_test.rs")),
            make_test_item("3", "auth_doc.md", "# Auth Documentation", "document_edit", Some("/docs/auth.md")),
        ];

        let task = TaskCandidate {
            id: "task_auth".to_string(),
            context_id: Some("ws_contextsplit".to_string()),
            title: "Implement Auth Flow".to_string(),
            confidence: 0.95,
            status: TaskCandidateStatus::Active,
            first_detected_at: "2026-09-22T00:00:00Z".to_string(),
            last_active_at: "2026-09-22T01:30:00Z".to_string(),
            created_at: "2026-09-22T00:00:00Z".to_string(),
            updated_at: "2026-09-22T01:30:00Z".to_string(),
        };

        let input = SessionEvaluationInput {
            session_id: "sess_completed",
            context_id: "ws_contextsplit",
            context_name: Some("ContextSplit"),
            raw_started_at: "2026-09-22T00:00:00Z",
            raw_last_activity_at: "2026-09-22T01:30:00Z",
            raw_ended_at: Some("2026-09-22T01:30:00Z"),
            items: &items,
            task_candidate: Some(&task),
            is_task_completed: true,
            has_pause_checkpoint: false,
            is_resumed_context: false,
            latest_snapshot: None,
            reference_time: None,
        };

        let session = ContextSessionEvaluator::evaluate_session(input, None);

        assert_eq!(session.session_state, ContextSessionState::Completed);
        assert_eq!(session.ended_at.as_deref(), Some("2026-09-22T01:30:00Z"));
        assert_eq!(session.duration_seconds, 5400);
        assert_eq!(session.duration_formatted, "1h 30m");
        assert!(session.task_reference.is_some());
        assert!(session.task_reference.as_ref().unwrap().is_completed);
    }

    #[test]
    fn test_interrupted_session_does_not_fabricate_ended_at() {
        // Sudden unexpected silence (e.g. crash or power loss 2 hours ago without clean exit)
        let items = vec![
            make_test_item("1", "parser.rs", "fn parse() { ... }", "document_edit", Some("/src/parser.rs")),
            make_test_item("2", "parser.rs", "let token = lex();", "document_edit", Some("/src/parser.rs")),
            make_test_item("3", "parser.rs", "match token { ... }", "document_edit", Some("/src/parser.rs")),
        ];

        let ref_now = DateTime::parse_from_rfc3339("2026-09-22T04:00:00Z").unwrap().with_timezone(&Utc);

        let input = SessionEvaluationInput {
            session_id: "sess_interrupted",
            context_id: "ws_contextsplit",
            context_name: Some("ContextSplit"),
            raw_started_at: "2026-09-22T01:00:00Z",
            raw_last_activity_at: "2026-09-22T01:45:00Z", // Activity ceased abruptly at 01:45 (2h 15m ago)
            raw_ended_at: None, // Abrupt termination
            items: &items,
            task_candidate: None,
            is_task_completed: false,
            has_pause_checkpoint: false,
            is_resumed_context: false,
            latest_snapshot: None,
            reference_time: Some(ref_now),
        };

        let session = ContextSessionEvaluator::evaluate_session(input, None);

        // Invariant: Session is Interrupted and ended_at MUST NOT be fabricated!
        assert_eq!(session.session_state, ContextSessionState::Interrupted);
        assert_eq!(session.ended_at, None, "ended_at must be None for interrupted sessions");
        assert_eq!(session.last_activity_at, "2026-09-22T01:45:00Z");
        assert_eq!(session.duration_seconds, 2700);
        assert_eq!(session.duration_formatted, "45m 0s");
    }

    #[test]
    fn test_paused_session_with_snapshot_reference() {
        let items = vec![
            make_test_item("1", "spec.md", "# Context Split Spec", "document_edit", Some("/docs/spec.md")),
            make_test_item("2", "mod.rs", "pub mod context;", "document_edit", Some("/src/mod.rs")),
            make_test_item("3", "context.rs", "struct Context;", "document_edit", Some("/src/context.rs")),
        ];

        // Snapshot captured at departure
        let snapshot = ContextSnapshot {
            id: "snap_departure_01".to_string(),
            context_id: "ws_contextsplit".to_string(),
            context_name: "ContextSplit".to_string(),
            task_id: Some("task_arch".to_string()),
            task_title: Some("Design Architecture".to_string()),
            task_provenance: Some(InformationProvenance::Confirmed),
            task_confidence: Some(0.92),
            created_at: "2026-09-22T02:00:00Z".to_string(),
            last_activity_at: Some("2026-09-22T02:00:00Z".to_string()),
            state: ContextStateValue::Paused,
            state_freshness: StateFreshness::Recent,
            state_freshness_display: "Paused recently".to_string(),
            resume_readiness: ResumeReadinessAssessment {
                readiness: ResumeReadiness::Ready,
                reasons: vec!["Active task saved".to_string()],
                formatted_explanation: "Ready to resume".to_string(),
            },
            workset_item_references: vec![],
            recent_activity_references: vec![],
            recent_task_references: vec![],
            important_memory_references: vec![],
            last_meaningful_activity: None,
            snapshot_confidence: 0.90,
            snapshot_statement: "Paused during architecture design".to_string(),
            quality: ContextSnapshotQuality::default(),
            provenance: InformationProvenance::Observed,
            epistemic_disclaimer: "Disclaimer".to_string(),
        };

        let input = SessionEvaluationInput {
            session_id: "sess_paused",
            context_id: "ws_contextsplit",
            context_name: Some("ContextSplit"),
            raw_started_at: "2026-09-22T01:00:00Z",
            raw_last_activity_at: "2026-09-22T02:00:00Z",
            raw_ended_at: None,
            items: &items,
            task_candidate: None,
            is_task_completed: false,
            has_pause_checkpoint: true, // Graceful pause detector fired
            is_resumed_context: false,
            latest_snapshot: Some(&snapshot),
            reference_time: None,
        };

        let session = ContextSessionEvaluator::evaluate_session(input, None);

        assert_eq!(session.session_state, ContextSessionState::Paused);
        assert!(session.snapshot_reference.is_some());
        assert_eq!(
            session.snapshot_reference.as_ref().unwrap().snapshot_id,
            "snap_departure_01"
        );
        assert!(session.snapshot_reference.as_ref().unwrap().is_resumable);
    }

    #[test]
    fn test_resumed_session_representation() {
        let items = vec![
            make_test_item("1", "resumed.rs", "fn resume_work() {}", "document_edit", Some("/src/resumed.rs")),
            make_test_item("2", "resumed.rs", "resume_work();", "document_edit", Some("/src/resumed.rs")),
            make_test_item("3", "resumed_test.rs", "#[test] fn test_resume() {}", "document_edit", Some("/src/resumed_test.rs")),
        ];

        let ref_now = DateTime::parse_from_rfc3339("2026-09-22T03:10:00Z").unwrap().with_timezone(&Utc);

        let input = SessionEvaluationInput {
            session_id: "sess_resumed_segment",
            context_id: "ws_contextsplit",
            context_name: Some("ContextSplit"),
            raw_started_at: "2026-09-22T03:00:00Z",
            raw_last_activity_at: "2026-09-22T03:08:00Z",
            raw_ended_at: None,
            items: &items,
            task_candidate: None,
            is_task_completed: false,
            has_pause_checkpoint: false,
            is_resumed_context: true, // Session created upon resumption
            latest_snapshot: None,
            reference_time: Some(ref_now),
        };

        let session = ContextSessionEvaluator::evaluate_session(input, None);

        assert_eq!(session.session_state, ContextSessionState::Active);
        assert_eq!(session.duration_seconds, 480);
        assert_eq!(session.meaningful_activity_count, 3);
    }

    #[test]
    fn test_partial_micro_session_filters_trivial_focus_hops() {
        // Single empty focus switch without document activity
        let noisy_items = vec![
            make_test_item("1", "Focus Hop", "", "focus_change", None),
            make_test_item("2", "App Activated", "", "app_activated", None),
        ];

        let ref_now = DateTime::parse_from_rfc3339("2026-09-22T02:00:00Z").unwrap().with_timezone(&Utc);

        let input = SessionEvaluationInput {
            session_id: "sess_noise",
            context_id: "ws_contextsplit",
            context_name: Some("ContextSplit"),
            raw_started_at: "2026-09-22T01:00:00Z",
            raw_last_activity_at: "2026-09-22T01:01:00Z",
            raw_ended_at: None,
            items: &noisy_items,
            task_candidate: None,
            is_task_completed: false,
            has_pause_checkpoint: false,
            is_resumed_context: false,
            latest_snapshot: None,
            reference_time: Some(ref_now),
        };

        let session = ContextSessionEvaluator::evaluate_session(input, None);

        // Meaningful items should be 0, session marked as Partial micro-session
        assert_eq!(session.meaningful_activity_count, 0);
        assert!(session.is_micro_session);
        assert_eq!(session.session_state, ContextSessionState::Partial);
    }

    #[test]
    fn test_json_serialization_roundtrip() {
        let items = vec![
            make_test_item("1", "a.rs", "fn a() {}", "document_edit", Some("/src/a.rs")),
            make_test_item("2", "b.rs", "fn b() {}", "document_edit", Some("/src/b.rs")),
            make_test_item("3", "c.rs", "fn c() {}", "document_edit", Some("/src/c.rs")),
        ];

        let input = SessionEvaluationInput {
            session_id: "sess_json",
            context_id: "ws_contextsplit",
            context_name: Some("ContextSplit"),
            raw_started_at: "2026-09-22T00:00:00Z",
            raw_last_activity_at: "2026-09-22T00:45:00Z",
            raw_ended_at: Some("2026-09-22T00:45:00Z"),
            items: &items,
            task_candidate: None,
            is_task_completed: true,
            has_pause_checkpoint: false,
            is_resumed_context: false,
            latest_snapshot: None,
            reference_time: None,
        };

        let session = ContextSessionEvaluator::evaluate_session(input, None);

        let json_str = serde_json::to_string_pretty(&session).expect("Serialization should succeed");
        let deserialized: ContextSession = serde_json::from_str(&json_str).expect("Deserialization should succeed");

        assert_eq!(session, deserialized);
        assert_eq!(deserialized.session_id, "sess_json");
        assert_eq!(deserialized.session_state, ContextSessionState::Completed);
    }

    #[test]
    fn test_system_sleep_interruption_does_not_inflate_duration_or_fabricate_activity() {
        // Active work for 30 minutes, then machine went to sleep for 8 hours
        let items = vec![
            make_test_item("1", "pause_detector.rs", "fn detect_pause()", "document_edit", Some("/src/pause_detector.rs")),
            make_test_item("2", "lifecycle.rs", "pub enum ContextLifecycleState", "document_edit", Some("/src/lifecycle.rs")),
        ];

        let ref_now = DateTime::parse_from_rfc3339("2026-09-22T08:30:00Z").unwrap().with_timezone(&Utc);

        let input = SessionEvaluationInput {
            session_id: "sess_sleep",
            context_id: "ws_contextsplit",
            context_name: Some("ContextSplit"),
            raw_started_at: "2026-09-22T00:00:00Z",
            raw_last_activity_at: "2026-09-22T00:30:00Z", // Sleep occurred at 00:30
            raw_ended_at: None, // No clean exit occurred
            items: &items,
            task_candidate: None,
            is_task_completed: false,
            has_pause_checkpoint: false,
            is_resumed_context: false,
            latest_snapshot: None,
            reference_time: Some(ref_now),
        };

        let session = ContextSessionEvaluator::evaluate_session(input, None);

        // 1. Session is Interrupted, not Completed
        assert_eq!(session.session_state, ContextSessionState::Interrupted);
        assert_eq!(session.ended_at, None, "Downtime must not fabricate ended_at");

        // 2. Duration is based strictly on actual active work (30 min), NOT inflated by 8 hours of sleep
        assert_eq!(session.duration_seconds, 1800);
        assert_eq!(session.duration_formatted, "30m 0s");
        assert_eq!(session.meaningful_activity_count, 2);
    }

    #[test]
    fn test_application_crash_distinguished_from_normal_completion() {
        let items = vec![
            make_test_item("1", "engine.rs", "pub fn start_engine()", "document_edit", Some("/src/engine.rs")),
            make_test_item("2", "engine.rs", "engine.poll()", "document_edit", Some("/src/engine.rs")),
        ];

        let ref_now = DateTime::parse_from_rfc3339("2026-09-22T03:00:00Z").unwrap().with_timezone(&Utc);

        // Scenario A: Unexpected crash (no raw_ended_at, no task completion, long silence)
        let input_crash = SessionEvaluationInput {
            session_id: "sess_crash",
            context_id: "ws_contextsplit",
            context_name: Some("ContextSplit"),
            raw_started_at: "2026-09-22T01:00:00Z",
            raw_last_activity_at: "2026-09-22T01:25:00Z",
            raw_ended_at: None,
            items: &items,
            task_candidate: None,
            is_task_completed: false,
            has_pause_checkpoint: false,
            is_resumed_context: false,
            latest_snapshot: None,
            reference_time: Some(ref_now),
        };
        let session_crash = ContextSessionEvaluator::evaluate_session(input_crash, None);
        assert_eq!(session_crash.session_state, ContextSessionState::Interrupted);
        assert_eq!(session_crash.ended_at, None);

        // Scenario B: Normal clean completion
        let input_normal = SessionEvaluationInput {
            session_id: "sess_clean",
            context_id: "ws_contextsplit",
            context_name: Some("ContextSplit"),
            raw_started_at: "2026-09-22T01:00:00Z",
            raw_last_activity_at: "2026-09-22T01:25:00Z",
            raw_ended_at: Some("2026-09-22T01:25:00Z"),
            items: &items,
            task_candidate: None,
            is_task_completed: true,
            has_pause_checkpoint: false,
            is_resumed_context: false,
            latest_snapshot: None,
            reference_time: Some(ref_now),
        };
        let session_normal = ContextSessionEvaluator::evaluate_session(input_normal, None);
        assert_eq!(session_normal.session_state, ContextSessionState::Completed);
        assert_eq!(session_normal.ended_at.as_deref(), Some("2026-09-22T01:25:00Z"));
    }

    #[test]
    fn test_application_and_system_restart_reconstructs_context_without_filler() {
        let items = vec![
            make_test_item("1", "reconstruction.rs", "fn reconstruct()", "document_edit", Some("/src/reconstruction.rs")),
        ];

        let ref_now = DateTime::parse_from_rfc3339("2026-09-22T12:00:00Z").unwrap().with_timezone(&Utc);

        let input = SessionEvaluationInput {
            session_id: "sess_reboot",
            context_id: "ws_contextsplit",
            context_name: Some("ContextSplit"),
            raw_started_at: "2026-09-22T02:00:00Z",
            raw_last_activity_at: "2026-09-22T02:15:00Z", // System restarted shortly after 02:15
            raw_ended_at: None,
            items: &items,
            task_candidate: None,
            is_task_completed: false,
            has_pause_checkpoint: false,
            is_resumed_context: false,
            latest_snapshot: None,
            reference_time: Some(ref_now),
        };

        let session = ContextSessionEvaluator::evaluate_session(input, None);

        // System downtime of 9+ hours must NOT be included as work
        assert_eq!(session.session_state, ContextSessionState::Interrupted);
        assert_eq!(session.duration_seconds, 900);
        assert_eq!(session.duration_formatted, "15m 0s");
        assert_eq!(session.meaningful_activity_count, 1);
    }
}

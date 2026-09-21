use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::context_engine::context_state::{ContextState, ResumeReadiness};
use crate::context_engine::lifecycle::LifecycleSourceEvidence;
use crate::context_engine::segments::ContextActivitySegment;
use crate::context_engine::task_transitions::{DetectedTaskTransition, TaskTransitionType};
use crate::context_engine::transitions::{ContextTransition, ContextTransitionType};
use crate::context_engine::workset::ContextWorkset;
use crate::db::models::{Item, Workspace};

// =========================================================================
// PauseCause: Specific Causal Triggers for Context Pause
// =========================================================================

/// Specific causal reason why a context transitioned into or remains in a `Paused` state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PauseCause {
    /// Another workspace became active or user explicitly switched focus.
    ContextSwitch,
    /// User closed project files, repository windows, or navigated away from project root.
    ProjectDeparture,
    /// Active task was concluded, ended, or marked paused.
    MeaningfulActivityStopped,
    /// User engaged in temporary unrelated browsing (e.g. social media, news, YouTube) without switching projects.
    UnrelatedDetour,
    /// Extended gap in telemetry timestamps indicating OS sleep, screen lock, or machine standby.
    SystemSleep,
    /// Continuous inactivity beyond threshold without an explicit switch to another project.
    InactivityTimeout,
    /// Primary application associated with this workspace was closed.
    ApplicationClosed,
}

impl PauseCause {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ContextSwitch => "context_switch",
            Self::ProjectDeparture => "project_departure",
            Self::MeaningfulActivityStopped => "meaningful_activity_stopped",
            Self::UnrelatedDetour => "unrelated_detour",
            Self::SystemSleep => "system_sleep",
            Self::InactivityTimeout => "inactivity_timeout",
            Self::ApplicationClosed => "application_closed",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::ContextSwitch => "Context Switch to Another Workspace",
            Self::ProjectDeparture => "Departed Project / Repo Directory",
            Self::MeaningfulActivityStopped => "Active Task Stopped",
            Self::UnrelatedDetour => "Temporary Unrelated Browsing Detour",
            Self::SystemSleep => "System Sleep / Lock Standby",
            Self::InactivityTimeout => "Inactivity Timeout",
            Self::ApplicationClosed => "Application Closed",
        }
    }
}

impl std::fmt::Display for PauseCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// =========================================================================
// PauseDetectionConfig: Configurable Stability and Horizons
// =========================================================================

/// Configuration parameters for conservative context pause detection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PauseDetectionConfig {
    /// Inactivity duration in seconds below which activity is treated as a micro-break, not a full pause (default: 900s / 15 min).
    pub short_inactivity_threshold_secs: i64,
    /// Minimum time gap in seconds between telemetry captures indicating OS sleep/lock (default: 1800s / 30 min).
    pub system_sleep_gap_threshold_secs: i64,
    /// Minimum duration of unrelated browsing required to classify as a detour pause (default: 180s / 3 min).
    pub detour_inactivity_threshold_secs: i64,
    /// Minimum confidence of competing context required to trigger a context switch pause (default: 0.65).
    pub switch_confidence_threshold: f64,
    /// Whether pause detection preserves active tasks (default: true).
    pub preserve_task_on_pause: bool,
    /// Whether pause detection preserves worksets and readiness (default: true).
    pub preserve_workset_on_pause: bool,
}

impl Default for PauseDetectionConfig {
    fn default() -> Self {
        Self {
            short_inactivity_threshold_secs: 900,  // 15 minutes
            system_sleep_gap_threshold_secs: 1800, // 30 minutes
            detour_inactivity_threshold_secs: 180, // 3 minutes
            switch_confidence_threshold: 0.65,
            preserve_task_on_pause: true,
            preserve_workset_on_pause: true,
        }
    }
}

// =========================================================================
// PauseAssessment: Structured Assessment Outcome
// =========================================================================

/// Detailed outcome of conservative context pause evaluation.
///
/// Invariants:
/// - A pause MUST NEVER discard or reset working memory, tasks, or worksets.
/// - The context MUST REMAIN fully resumable (`is_resumable = true`).
/// - Short inactivity periods ($\le 15$ min) MUST NOT be treated as abandonment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PauseAssessment {
    /// Context / Workspace identifier
    pub context_id: String,
    /// Whether this context is currently evaluated as paused
    pub is_paused: bool,
    /// Specific causal reason if paused (nullable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pause_cause: Option<PauseCause>,
    /// Confidence in the pause assessment [0.0, 1.0]
    pub pause_confidence: f64,
    /// ISO-8601 timestamp when the pause began (nullable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused_at: Option<String>,
    /// Invariant: Paused contexts remain fully resumable
    pub is_resumable: bool,
    /// Preserved active task ID (nullable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preserved_task_id: Option<String>,
    /// Preserved active task title (nullable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preserved_task_title: Option<String>,
    /// Number of preserved working items in workset
    pub preserved_workset_count: usize,
    /// Number of preserved long-lived important memories
    pub preserved_memory_count: usize,
    /// Preserved resume readiness classification
    pub preserved_readiness: ResumeReadiness,
    /// Human-readable explanation of the pause determination
    pub explanation: String,
    /// Grounding evidence items
    pub evidence: Vec<LifecycleSourceEvidence>,
}

impl PauseAssessment {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        context_id: impl Into<String>,
        is_paused: bool,
        pause_cause: Option<PauseCause>,
        pause_confidence: f64,
        paused_at: Option<String>,
        preserved_task_id: Option<String>,
        preserved_task_title: Option<String>,
        preserved_workset_count: usize,
        preserved_memory_count: usize,
        preserved_readiness: ResumeReadiness,
        explanation: impl Into<String>,
        evidence: Vec<LifecycleSourceEvidence>,
    ) -> Self {
        Self {
            context_id: context_id.into(),
            is_paused,
            pause_cause,
            pause_confidence: pause_confidence.clamp(0.0, 1.0),
            paused_at,
            is_resumable: true, // Invariant: Always resumable
            preserved_task_id,
            preserved_task_title,
            preserved_workset_count,
            preserved_memory_count,
            preserved_readiness,
            explanation: explanation.into(),
            evidence,
        }
    }
}

// =========================================================================
// PauseEvaluationInput: Multi-Factor Signals
// =========================================================================

/// Multi-factor input signals for evaluating whether a context is paused.
#[derive(Debug, Clone, Default)]
pub struct PauseEvaluationInput<'a> {
    pub context_id: &'a str,
    pub context_name: Option<&'a str>,
    pub workspace: Option<&'a Workspace>,
    pub context_state: Option<&'a ContextState>,
    pub current_workset: Option<&'a ContextWorkset>,
    pub important_memory_count: usize,
    pub activity_segments: &'a [ContextActivitySegment],
    pub context_transitions: &'a [ContextTransition],
    pub task_transitions: &'a [DetectedTaskTransition],
    pub items: &'a [Item],
    /// If another workspace is currently confirmed active in the foreground
    pub active_other_context_id: Option<&'a str>,
    /// Confidence of the competing workspace if active
    pub active_other_context_confidence: Option<f64>,
    /// Time gap in seconds between telemetry captures indicating OS sleep
    pub last_telemetry_gap_seconds: Option<i64>,
    /// Whether the primary IDE/application for this context was closed
    pub is_application_closed: bool,
    /// Whether user is currently engaged in unrelated peripheral browsing
    pub is_unrelated_browsing: bool,
    /// Duration of unrelated browsing in seconds
    pub unrelated_browsing_duration_secs: Option<i64>,
    /// Reference time for evaluation (defaults to Utc::now)
    pub reference_time: Option<DateTime<Utc>>,
}

// =========================================================================
// ContextPauseDetector: Core Detection Engine
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

/// Conservative context pause detection engine.
#[derive(Debug, Clone)]
pub struct ContextPauseDetector {
    pub config: PauseDetectionConfig,
}

impl Default for ContextPauseDetector {
    fn default() -> Self {
        Self::new(PauseDetectionConfig::default())
    }
}

impl ContextPauseDetector {
    pub fn new(config: PauseDetectionConfig) -> Self {
        Self { config }
    }

    /// Evaluates whether the given context is paused using multi-factor contextual evidence.
    pub fn evaluate(&self, input: PauseEvaluationInput<'_>) -> PauseAssessment {
        let now = input.reference_time.unwrap_or_else(Utc::now);
        let ws_name = input.context_name.unwrap_or("this workspace");

        let mut evidence: Vec<LifecycleSourceEvidence> = Vec::new();

        // 1. Extract Preserved Attributes (Tasks, Workset, Memory, Readiness)
        let (preserved_task_id, preserved_task_title) = if let Some(cs) = input.context_state {
            (cs.task_id.clone(), cs.task_title.clone())
        } else if let Some(ws) = input.current_workset {
            (ws.task_id.clone(), ws.task_title.clone())
        } else {
            (None, None)
        };

        let preserved_workset_count = input
            .current_workset
            .map(|w| w.items.len())
            .unwrap_or_else(|| input.items.len());

        let preserved_memory_count = input.important_memory_count;

        let preserved_readiness = input
            .context_state
            .map(|cs| cs.readiness.readiness)
            .unwrap_or_else(|| {
                if preserved_task_title.is_some() && preserved_workset_count > 0 {
                    ResumeReadiness::Ready
                } else if preserved_workset_count > 0 {
                    ResumeReadiness::MostlyReady
                } else {
                    ResumeReadiness::Partial
                }
            });

        // 2. Discover Latest Context Activity Timestamp
        let mut latest_active_dt: Option<DateTime<Utc>> = None;
        let mut latest_active_str: Option<String> = None;

        let mut consider_active = |ts_str: &str| {
            if let Some(dt) = parse_ts(ts_str) {
                if latest_active_dt.is_none() || Some(dt) > latest_active_dt {
                    latest_active_dt = Some(dt);
                    latest_active_str = Some(ts_str.to_string());
                }
            }
        };

        for item in input.items {
            consider_active(&item.created_at);
        }

        for seg in input.activity_segments {
            if seg.context.as_ref().map(|c| c.id.as_str()) == Some(input.context_id) {
                consider_active(&seg.ended_at);
            }
        }

        if let Some(ws) = input.workspace {
            if let Some(ref la) = ws.last_activity_at {
                consider_active(la);
            }
        }

        if let Some(cs) = input.context_state {
            if let Some(ref la) = cs.last_active_at {
                consider_active(la);
            }
        }

        let elapsed_seconds = latest_active_dt.map(|dt| (now - dt).num_seconds().max(0)).unwrap_or(0);

        // 3. Multi-Factor Cause Evaluation

        // Case A: Context Switch (Another context confirmed active in foreground)
        if let Some(other_id) = input.active_other_context_id {
            if other_id != input.context_id {
                let conf = input.active_other_context_confidence.unwrap_or(0.85);
                if conf >= self.config.switch_confidence_threshold {
                    let paused_at = now.to_rfc3339();
                    evidence.push(LifecycleSourceEvidence {
                        evidence_type: "context_switch".to_string(),
                        description: format!("Focus switched to competing workspace '{}' (confidence: {:.2}).", other_id, conf),
                        timestamp: paused_at.clone(),
                        confidence: conf,
                        reference_id: Some(other_id.to_string()),
                    });

                    return PauseAssessment::new(
                        input.context_id,
                        true,
                        Some(PauseCause::ContextSwitch),
                        conf,
                        Some(paused_at),
                        preserved_task_id,
                        preserved_task_title,
                        preserved_workset_count,
                        preserved_memory_count,
                        preserved_readiness,
                        format!("Work on '{}' is paused while working in '{}'. All context state is preserved.", ws_name, other_id),
                        evidence,
                    );
                }
            }
        }

        // Check recent context transitions for an explicit ContextSwitch away
        let latest_trans = input.context_transitions.iter().max_by_key(|t| {
            parse_ts(&t.detected_at).unwrap_or_else(|| DateTime::from_timestamp(0, 0).unwrap())
        });

        if let Some(trans) = latest_trans {
            if trans.from_context_id.as_deref() == Some(input.context_id)
                && trans.to_context_id.as_deref() != Some(input.context_id)
                && trans.transition_type == ContextTransitionType::ContextSwitch
            {
                let paused_at = trans.detected_at.clone();
                evidence.push(LifecycleSourceEvidence {
                    evidence_type: "context_transition".to_string(),
                    description: format!("Recorded context switch transition away from '{}'.", ws_name),
                    timestamp: paused_at.clone(),
                    confidence: trans.confidence,
                    reference_id: Some(trans.id.clone()),
                });

                return PauseAssessment::new(
                    input.context_id,
                    true,
                    Some(PauseCause::ContextSwitch),
                    trans.confidence,
                    Some(paused_at),
                    preserved_task_id,
                    preserved_task_title,
                    preserved_workset_count,
                    preserved_memory_count,
                    preserved_readiness,
                    format!("Context '{}' paused on focus switch. Resume readiness remains intact.", ws_name),
                    evidence,
                );
            }
        }

        // Case B: Application Closed / Project Departure
        if input.is_application_closed {
            let paused_at = now.to_rfc3339();
            evidence.push(LifecycleSourceEvidence {
                evidence_type: "application_closed".to_string(),
                description: format!("Primary application for '{}' was closed.", ws_name),
                timestamp: paused_at.clone(),
                confidence: 0.90,
                reference_id: None,
            });

            return PauseAssessment::new(
                input.context_id,
                true,
                Some(PauseCause::ApplicationClosed),
                0.90,
                Some(paused_at),
                preserved_task_id,
                preserved_task_title,
                preserved_workset_count,
                preserved_memory_count,
                preserved_readiness,
                format!("Workspace application closed for '{}'. Work is paused and ready to resume upon relaunch.", ws_name),
                evidence,
            );
        }

        // Case C: System Sleep / Standby Gap (e.g. laptop closed or screen locked)
        if let Some(gap) = input.last_telemetry_gap_seconds {
            if gap >= self.config.system_sleep_gap_threshold_secs {
                let paused_at = if let Some(ref la) = latest_active_str {
                    la.clone()
                } else {
                    now.to_rfc3339()
                };

                evidence.push(LifecycleSourceEvidence {
                    evidence_type: "system_sleep".to_string(),
                    description: format!("Detected system sleep / lock telemetry gap of {} minutes.", gap / 60),
                    timestamp: paused_at.clone(),
                    confidence: 0.92,
                    reference_id: None,
                });

                return PauseAssessment::new(
                    input.context_id,
                    true,
                    Some(PauseCause::SystemSleep),
                    0.92,
                    Some(paused_at),
                    preserved_task_id,
                    preserved_task_title,
                    preserved_workset_count,
                    preserved_memory_count,
                    preserved_readiness,
                    format!("Context '{}' paused across system sleep gap ({}m). Ready for instant morning resumption.", ws_name, gap / 60),
                    evidence,
                );
            }
        }

        // Case D: Unrelated Peripheral Browsing Detour
        if input.is_unrelated_browsing {
            let detour_dur = input.unrelated_browsing_duration_secs.unwrap_or(0);
            if detour_dur >= self.config.detour_inactivity_threshold_secs {
                let paused_at = now.to_rfc3339();
                evidence.push(LifecycleSourceEvidence {
                    evidence_type: "unrelated_detour".to_string(),
                    description: format!("Observed unrelated peripheral browsing for {}s without workspace focus.", detour_dur),
                    timestamp: paused_at.clone(),
                    confidence: 0.82,
                    reference_id: None,
                });

                return PauseAssessment::new(
                    input.context_id,
                    true,
                    Some(PauseCause::UnrelatedDetour),
                    0.82,
                    Some(paused_at),
                    preserved_task_id,
                    preserved_task_title,
                    preserved_workset_count,
                    preserved_memory_count,
                    preserved_readiness,
                    format!("Context '{}' paused during temporary peripheral detour. Core workset protected from contamination.", ws_name),
                    evidence,
                );
            }
        }

        // Case E: Meaningful Task Concluded or Ended
        if let Some(last_tt) = input.task_transitions.last() {
            if last_tt.transition_type == TaskTransitionType::TaskEnd {
                let paused_at = last_tt.timestamp.clone();
                evidence.push(LifecycleSourceEvidence {
                    evidence_type: "task_transition".to_string(),
                    description: format!("Active task concluded / ended (transition: {}).", last_tt.id),
                    timestamp: paused_at.clone(),
                    confidence: 0.80,
                    reference_id: Some(last_tt.id.clone()),
                });

                return PauseAssessment::new(
                    input.context_id,
                    true,
                    Some(PauseCause::MeaningfulActivityStopped),
                    0.80,
                    Some(paused_at),
                    preserved_task_id,
                    preserved_task_title,
                    preserved_workset_count,
                    preserved_memory_count,
                    preserved_readiness,
                    format!("Task in '{}' concluded. Context is paused awaiting next task.", ws_name),
                    evidence,
                );
            }
        }

        // Case F: Short Inactivity vs. Inactivity Timeout
        // Rule: Inactivity <= 15 minutes is a micro-break, NOT a full pause.
        if elapsed_seconds <= self.config.short_inactivity_threshold_secs && latest_active_dt.is_some() {
            // Context is NOT paused; remains warm and active
            evidence.push(LifecycleSourceEvidence {
                evidence_type: "telemetry_capture".to_string(),
                description: format!("Recent telemetry observed within {}s (<= {}s micro-break threshold).", elapsed_seconds, self.config.short_inactivity_threshold_secs),
                timestamp: latest_active_str.unwrap_or_else(|| now.to_rfc3339()),
                confidence: 0.90,
                reference_id: None,
            });

            return PauseAssessment::new(
                input.context_id,
                false,
                None,
                0.90,
                None,
                preserved_task_id,
                preserved_task_title,
                preserved_workset_count,
                preserved_memory_count,
                preserved_readiness,
                format!("Active work in '{}' within immediate observation window. Context is actively running.", ws_name),
                evidence,
            );
        }

        // Case G: Sustained Absence of Evidence (> 15 minutes)
        let paused_at = if let Some(ref la) = latest_active_str {
            la.clone()
        } else {
            now.to_rfc3339()
        };

        evidence.push(LifecycleSourceEvidence {
            evidence_type: "inactivity_timeout".to_string(),
            description: format!("Sustained absence of context activity for {} minutes (> {}m threshold).", elapsed_seconds / 60, self.config.short_inactivity_threshold_secs / 60),
            timestamp: paused_at.clone(),
            confidence: 0.78,
            reference_id: None,
        });

        PauseAssessment::new(
            input.context_id,
            true,
            Some(PauseCause::InactivityTimeout),
            0.78,
            Some(paused_at),
            preserved_task_id,
            preserved_task_title,
            preserved_workset_count,
            preserved_memory_count,
            preserved_readiness,
            format!("Context '{}' paused after {}m of inactivity. Ready to resume with full state.", ws_name, elapsed_seconds / 60),
            evidence,
        )
    }
}

// =========================================================================
// Unit Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_engine::context_state::{ContextResumeState, ResumeReadinessAssessment, StateExplanation, StateFreshness, ContextStateValue};
    use crate::context_engine::memory::{InformationProvenance, MemoryExplanation};
    use crate::context_engine::workset::{ContextWorkset, WorksetItemReference, WorksetItemRelevance, WorksetState};

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

    fn make_test_workset(ws_id: &str, task_title: &str, count: usize) -> ContextWorkset {
        let mut items = Vec::new();
        for i in 0..count {
            items.push(WorksetItemReference {
                id: format!("item_{}", i),
                title: Some(format!("Workset Item {}", i)),
                item_type: "text".to_string(),
                relevance: WorksetItemRelevance::Primary,
                relevance_score: 0.85,
                reasons: vec!["Active file".to_string()],
                provenance: InformationProvenance::Observed,
                created_at: "2026-09-22T00:00:00Z".to_string(),
                last_active_at: Some("2026-09-22T00:10:00Z".to_string()),
                app_name: Some("VS Code".to_string()),
                domain: None,
                content_snippet: "Content snippet".to_string(),
                is_suggested_addition: false,
            });
        }

        ContextWorkset {
            context_id: ws_id.to_string(),
            context_name: "Test Context".to_string(),
            task_id: Some("task_123".to_string()),
            task_title: Some(task_title.to_string()),
            generated_at: "2026-09-22T00:10:00Z".to_string(),
            last_active_at: Some("2026-09-22T00:10:00Z".to_string()),
            workset_state: WorksetState::Active,
            items: items.clone(),
            recent_activity: "Coding feature".to_string(),
            relevant_items: items,
            important_references: Vec::new(),
            related_tasks: Vec::new(),
            confidence: 0.90,
            confidence_band: "High".to_string(),
            provenance: InformationProvenance::Observed,
            explanation: MemoryExplanation {
                summary: "Summary".to_string(),
                why_reasons: Vec::new(),
                supporting_evidence: Vec::new(),
                confidence_band: "High".to_string(),
                provenance: InformationProvenance::Observed,
                uncertainty_note: None,
            },
        }
    }

    #[test]
    fn test_pause_short_inactivity_not_paused() {
        let detector = ContextPauseDetector::default();
        let now = Utc::now();
        // 5 minutes ago (well within 15m micro-break threshold)
        let active_time = (now - chrono::Duration::minutes(5)).to_rfc3339();

        let item = make_test_item("item_1", "ws_payx", &active_time);
        let workset = make_test_workset("ws_payx", "Payment Gateway API", 3);

        let input = PauseEvaluationInput {
            context_id: "ws_payx",
            context_name: Some("PayX"),
            current_workset: Some(&workset),
            items: &[item],
            active_other_context_id: None,
            reference_time: Some(now),
            ..Default::default()
        };

        let assessment = detector.evaluate(input);
        assert!(!assessment.is_paused);
        assert_eq!(assessment.pause_cause, None);
        assert!(assessment.is_resumable);
        assert_eq!(assessment.preserved_task_title, Some("Payment Gateway API".to_string()));
        assert_eq!(assessment.preserved_workset_count, 3);
        assert!(assessment.explanation.contains("actively running"));
    }

    #[test]
    fn test_pause_context_switch_triggers_pause() {
        let detector = ContextPauseDetector::default();
        let now = Utc::now();
        let active_time = (now - chrono::Duration::minutes(2)).to_rfc3339();

        let item = make_test_item("item_1", "ws_payx", &active_time);
        let workset = make_test_workset("ws_payx", "Payment Gateway API", 4);

        let input = PauseEvaluationInput {
            context_id: "ws_payx",
            context_name: Some("PayX"),
            current_workset: Some(&workset),
            items: &[item],
            active_other_context_id: Some("ws_contextsplit"),
            active_other_context_confidence: Some(0.92),
            reference_time: Some(now),
            ..Default::default()
        };

        let assessment = detector.evaluate(input);
        assert!(assessment.is_paused);
        assert_eq!(assessment.pause_cause, Some(PauseCause::ContextSwitch));
        assert!(assessment.pause_confidence >= 0.90);
        assert!(assessment.is_resumable);
        assert_eq!(assessment.preserved_task_title, Some("Payment Gateway API".to_string()));
        assert_eq!(assessment.preserved_workset_count, 4);
        assert!(assessment.explanation.contains("paused while working in 'ws_contextsplit'"));
    }

    #[test]
    fn test_pause_application_close_triggers_pause() {
        let detector = ContextPauseDetector::default();
        let now = Utc::now();
        let active_time = (now - chrono::Duration::minutes(1)).to_rfc3339();

        let item = make_test_item("item_1", "ws_payx", &active_time);
        let workset = make_test_workset("ws_payx", "Stripe Webhooks", 2);

        let input = PauseEvaluationInput {
            context_id: "ws_payx",
            context_name: Some("PayX"),
            current_workset: Some(&workset),
            items: &[item],
            is_application_closed: true,
            reference_time: Some(now),
            ..Default::default()
        };

        let assessment = detector.evaluate(input);
        assert!(assessment.is_paused);
        assert_eq!(assessment.pause_cause, Some(PauseCause::ApplicationClosed));
        assert!(assessment.is_resumable);
        assert_eq!(assessment.preserved_task_title, Some("Stripe Webhooks".to_string()));
        assert_eq!(assessment.preserved_workset_count, 2);
    }

    #[test]
    fn test_pause_user_returning_quickly_resumes() {
        let detector = ContextPauseDetector::default();
        let now = Utc::now();
        let active_time = (now - chrono::Duration::seconds(30)).to_rfc3339();

        let item = make_test_item("item_1", "ws_payx", &active_time);
        let workset = make_test_workset("ws_payx", "Checkout Form", 3);

        let input = PauseEvaluationInput {
            context_id: "ws_payx",
            context_name: Some("PayX"),
            current_workset: Some(&workset),
            items: &[item],
            active_other_context_id: None,
            is_application_closed: false,
            is_unrelated_browsing: false,
            reference_time: Some(now),
            ..Default::default()
        };

        let assessment = detector.evaluate(input);
        assert!(!assessment.is_paused);
        assert!(assessment.is_resumable);
        assert_eq!(assessment.preserved_task_title, Some("Checkout Form".to_string()));
    }

    #[test]
    fn test_pause_unrelated_browsing_detour_preserves_workset() {
        let detector = ContextPauseDetector::default();
        let now = Utc::now();
        let active_time = (now - chrono::Duration::minutes(5)).to_rfc3339();

        let item = make_test_item("item_1", "ws_payx", &active_time);
        let workset = make_test_workset("ws_payx", "Auth Token Refresh", 5);

        let input = PauseEvaluationInput {
            context_id: "ws_payx",
            context_name: Some("PayX"),
            current_workset: Some(&workset),
            items: &[item],
            is_unrelated_browsing: true,
            unrelated_browsing_duration_secs: Some(300), // 5 minutes on YouTube
            reference_time: Some(now),
            ..Default::default()
        };

        let assessment = detector.evaluate(input);
        assert!(assessment.is_paused);
        assert_eq!(assessment.pause_cause, Some(PauseCause::UnrelatedDetour));
        assert!(assessment.is_resumable);
        assert_eq!(assessment.preserved_task_title, Some("Auth Token Refresh".to_string()));
        assert_eq!(assessment.preserved_workset_count, 5);
        assert!(assessment.explanation.contains("Core workset protected from contamination"));
    }

    #[test]
    fn test_pause_system_sleep_detection() {
        let detector = ContextPauseDetector::default();
        let now = Utc::now();
        let active_time = (now - chrono::Duration::hours(2)).to_rfc3339();

        let item = make_test_item("item_1", "ws_payx", &active_time);
        let workset = make_test_workset("ws_payx", "Nightly Batch Job", 2);

        let input = PauseEvaluationInput {
            context_id: "ws_payx",
            context_name: Some("PayX"),
            current_workset: Some(&workset),
            items: &[item],
            last_telemetry_gap_seconds: Some(7200), // 2 hours laptop sleep
            reference_time: Some(now),
            ..Default::default()
        };

        let assessment = detector.evaluate(input);
        assert!(assessment.is_paused);
        assert_eq!(assessment.pause_cause, Some(PauseCause::SystemSleep));
        assert!(assessment.is_resumable);
        assert!(assessment.pause_confidence >= 0.90);
        assert!(assessment.explanation.contains("system sleep"));
    }

    #[test]
    fn test_pause_preserves_task_workset_and_readiness() {
        let detector = ContextPauseDetector::default();
        let now = Utc::now();
        let active_time = (now - chrono::Duration::minutes(45)).to_rfc3339();

        let item = make_test_item("item_1", "ws_payx", &active_time);
        let workset = make_test_workset("ws_payx", "Complex Data Migration", 8);

        let input = PauseEvaluationInput {
            context_id: "ws_payx",
            context_name: Some("PayX"),
            current_workset: Some(&workset),
            important_memory_count: 6,
            items: &[item],
            reference_time: Some(now),
            ..Default::default()
        };

        let assessment = detector.evaluate(input);
        assert!(assessment.is_paused);
        assert_eq!(assessment.pause_cause, Some(PauseCause::InactivityTimeout));
        assert!(assessment.is_resumable);
        assert_eq!(assessment.preserved_task_title, Some("Complex Data Migration".to_string()));
        assert_eq!(assessment.preserved_workset_count, 8);
        assert_eq!(assessment.preserved_memory_count, 6);
        assert_eq!(assessment.preserved_readiness, ResumeReadiness::Ready);
    }

    #[test]
    fn test_pause_json_serialization() {
        let assessment = PauseAssessment::new(
            "ws_payx",
            true,
            Some(PauseCause::ContextSwitch),
            0.92,
            Some("2026-09-22T00:30:00Z".to_string()),
            Some("task_123".to_string()),
            Some("Payment API".to_string()),
            4,
            3,
            ResumeReadiness::Ready,
            "Context paused on context switch.",
            vec![LifecycleSourceEvidence {
                evidence_type: "context_switch".to_string(),
                description: "Switched to ContextSplit".to_string(),
                timestamp: "2026-09-22T00:30:00Z".to_string(),
                confidence: 0.92,
                reference_id: Some("ws_contextsplit".to_string()),
            }],
        );

        let json = serde_json::to_string(&assessment).expect("Serialization failed");
        assert!(json.contains("\"is_paused\":true"));
        assert!(json.contains("\"pause_cause\":\"context_switch\""));
        assert!(json.contains("\"is_resumable\":true"));

        let deserialized: PauseAssessment = serde_json::from_str(&json).expect("Deserialization failed");
        assert_eq!(deserialized, assessment);
    }
}

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::context_engine::important_memory::ContextImportantMemory;
use crate::context_engine::lifecycle::{ContextLifecycleState, LifecycleSourceEvidence};
use crate::context_engine::segments::ContextActivitySegment;
use crate::context_engine::task_extractor::ExtractedTaskCandidate;
use crate::context_engine::task_transitions::DetectedTaskTransition;
use crate::context_engine::transitions::{ContextSnapshot, ContextTransition, ContextTransitionType};
use crate::context_engine::workset::ContextWorkset;
use crate::db::models::{Item, Workspace};

// =========================================================================
// Dimension Scores & Indicator Breakdowns
// =========================================================================

/// Detailed scores across all 7 evaluated dimensions for Paused vs Dormant distinction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DormancyDimensionScores {
    /// Dimension 1: Recency of activity score [0.0 = recent, 1.0 = ancient]
    pub recency_score: f64,
    /// Dimension 2: Recent context transitions & multi-context work cycle influence [-0.30 to 0.0]
    pub transition_influence: f64,
    /// Dimension 3: Task status influence [-0.25 for in-progress, +0.20 for completed/none]
    pub task_status_influence: f64,
    /// Dimension 4: Recent resume history influence [-0.25 to 0.0]
    pub resume_history_influence: f64,
    /// Dimension 5: Important memory salience influence [-0.20 to 0.0]
    pub memory_salience_influence: f64,
    /// Dimension 6: Workset in-flight resources influence [-0.25 to +0.10]
    pub workset_influence: f64,
    /// Dimension 7: Historical activity pattern recurrence influence [-0.20 for recurring, +0.20 for isolated]
    pub pattern_recurrence_influence: f64,
    /// Final aggregate calibrated dormancy score [0.0 = Definitely Paused, 1.0 = Definitely Dormant]
    pub aggregate_dormancy_score: f64,
}

/// Detailed signals detected during work cycle evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WorkCycleIndicators {
    /// Inactivity elapsed in seconds
    pub elapsed_seconds: i64,
    /// Inactivity formatted as human-readable string (e.g., "2 days, 4 hours")
    pub elapsed_human: String,
    /// Active in-progress task detected
    pub has_in_progress_task: bool,
    /// Active task title if present
    pub in_progress_task_title: Option<String>,
    /// Task was completed or abandoned
    pub task_completed_or_done: bool,
    /// Number of active items in workset
    pub active_workset_item_count: usize,
    /// Number of high-salience important memories attached
    pub high_salience_memory_count: usize,
    /// Number of context transitions in the last 48 hours
    pub recent_transitions_count: usize,
    /// Number of resumes in the last 7 days
    pub recent_resumes_count: usize,
    /// Whether historical activity demonstrates recurring multi-day cadence
    pub has_recurring_pattern: bool,
    /// Whether this was an isolated single-session interaction
    pub is_isolated_session: bool,
}

// =========================================================================
// DormancyDistinctionAssessment: Primary Distinction Result
// =========================================================================

/// Result of evaluating whether an inactive context is PAUSED or DORMANT.
///
/// Invariants:
/// - PAUSED: The context was recently active and is likely still part of the user's current work cycle.
/// - DORMANT: The context has not been relevant recently but remains a valid historical context.
/// - Invariant: Never use arbitrary time alone; evaluate tasks, worksets, transitions, resumes, memories, and patterns.
/// - Invariant: Do NOT delete dormant contexts.
/// - Invariant: Do NOT lower historical importance simply because the context is dormant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DormancyDistinctionAssessment {
    /// Unique context or workspace identifier
    pub context_id: String,
    /// Classified lifecycle state (`Paused` or `Dormant`)
    pub lifecycle_state: ContextLifecycleState,
    /// True if classified as Dormant, false if Paused
    pub is_dormant: bool,
    /// Calibrated dormancy score in [0.0, 1.0]. >= 0.50 indicates Dormant.
    pub dormancy_score: f64,
    /// Confidence score in the distinction [0.0, 1.0]
    pub confidence: f64,
    /// Factual, human-readable justification of the determination
    pub reason: String,
    /// Detailed dimension scores
    pub dimension_scores: DormancyDimensionScores,
    /// Observed work cycle indicators
    pub work_cycle_indicators: WorkCycleIndicators,
    /// Grounded source evidence items
    pub source_evidence: Vec<LifecycleSourceEvidence>,
    /// Invariant assertion: Historical importance of memories and items is preserved 100% intact.
    pub historical_importance_preserved: bool,
}

// =========================================================================
// Dormancy Evaluation Input & Configuration
// =========================================================================

/// Configuration thresholds for the Dormancy vs Paused distinction engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DormancyEvaluationConfig {
    /// Dormancy classification score threshold (default: 0.50)
    pub dormancy_threshold: f64,
    /// Seconds under which a context is considered in the immediate work cycle regardless (default: 86400 = 24h)
    pub immediate_cycle_seconds: i64,
    /// Seconds after which inactivity creates a baseline presumption of dormancy unless offset (default: 604800 = 7 days)
    pub baseline_dormancy_seconds: i64,
    /// Seconds after which a context is almost certainly dormant without ongoing evidence (default: 2592000 = 30 days)
    pub deep_dormancy_seconds: i64,
}

impl Default for DormancyEvaluationConfig {
    fn default() -> Self {
        Self {
            dormancy_threshold: 0.50,
            immediate_cycle_seconds: 86400,        // 1 day
            baseline_dormancy_seconds: 7 * 86400,  // 7 days
            deep_dormancy_seconds: 30 * 86400,     // 30 days
        }
    }
}

/// Comprehensive input data for evaluating Paused vs Dormant distinction.
#[derive(Debug, Clone, Default)]
pub struct DormancyEvaluationInput<'a> {
    pub context_id: &'a str,
    pub context_name: Option<&'a str>,
    pub workspace: Option<&'a Workspace>,
    pub items: &'a [Item],
    pub activity_segments: &'a [ContextActivitySegment],
    pub context_transitions: &'a [ContextTransition],
    pub task_transitions: &'a [DetectedTaskTransition],
    pub active_task_candidate: Option<&'a ExtractedTaskCandidate>,
    pub workset: Option<&'a ContextWorkset>,
    pub important_memories: &'a [ContextImportantMemory],
    pub latest_snapshot: Option<&'a ContextSnapshot>,
    pub reference_time: Option<DateTime<Utc>>,
}

// =========================================================================
// DormancyDistinctionEvaluator: Core Engine
// =========================================================================

pub struct DormancyDistinctionEvaluator;

impl DormancyDistinctionEvaluator {
    /// Evaluates whether an inactive context is PAUSED or DORMANT using multidimensional signals.
    pub fn evaluate(
        input: DormancyEvaluationInput<'_>,
        config: Option<&DormancyEvaluationConfig>,
    ) -> DormancyDistinctionAssessment {
        let default_cfg = DormancyEvaluationConfig::default();
        let cfg = config.unwrap_or(&default_cfg);
        let now = input.reference_time.unwrap_or_else(Utc::now);
        let ws_name = input.context_name.unwrap_or("this workspace");

        let mut evidence: Vec<LifecycleSourceEvidence> = Vec::new();
        let mut indicators = WorkCycleIndicators::default();

        // ---------------------------------------------------------------------
        // 1. Determine Latest Activity Timestamp & Inactivity Elapsed
        // ---------------------------------------------------------------------
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

        for item in input.items {
            update_active(&item.created_at);
            update_active(&item.updated_at);
        }

        for seg in input.activity_segments {
            if seg.context.as_ref().map(|c| c.id.as_str()) == Some(input.context_id) {
                update_active(&seg.started_at);
                update_active(&seg.ended_at);
            }
        }

        if let Some(ws) = input.workspace {
            if let Some(ref la) = ws.last_activity_at {
                update_active(la);
            }
        }

        if let Some(snap) = input.latest_snapshot {
            update_active(&snap.last_seen_at);
        }

        let elapsed_seconds = if let Some(la_dt) = latest_active_dt {
            (now - la_dt).num_seconds().max(0)
        } else if let Some(ws) = input.workspace {
            if let Some(created_dt) = parse_ts(&ws.created_at) {
                (now - created_dt).num_seconds().max(0)
            } else {
                0
            }
        } else {
            0
        };

        indicators.elapsed_seconds = elapsed_seconds;
        indicators.elapsed_human = format_duration_human(elapsed_seconds);

        // ---------------------------------------------------------------------
        // Dimension 1: Recency of Activity [0.0, 1.0]
        // ---------------------------------------------------------------------
        let recency_score = if elapsed_seconds <= cfg.immediate_cycle_seconds {
            // Within 24 hours: base dormancy score is very low (0.0 to 0.15)
            (elapsed_seconds as f64 / cfg.immediate_cycle_seconds as f64) * 0.15
        } else if elapsed_seconds <= 3 * 86400 {
            // 1 to 3 days: 0.15 to 0.35 (standard weekend/break window)
            0.15 + ((elapsed_seconds - 86400) as f64 / (2.0 * 86400.0)) * 0.20
        } else if elapsed_seconds <= cfg.baseline_dormancy_seconds {
            // 3 to 7 days: 0.35 to 0.65 (transitional period)
            0.35 + ((elapsed_seconds - 3 * 86400) as f64 / (4.0 * 86400.0)) * 0.30
        } else if elapsed_seconds <= cfg.deep_dormancy_seconds {
            // 7 to 30 days: 0.65 to 0.90
            0.65 + ((elapsed_seconds - cfg.baseline_dormancy_seconds) as f64
                / (cfg.deep_dormancy_seconds - cfg.baseline_dormancy_seconds) as f64)
                * 0.25
        } else {
            // > 30 days: 0.90 to 1.0
            1.0
        };

        // ---------------------------------------------------------------------
        // Dimension 2: Recent Context Transitions & Multi-Context Work Cycle
        // ---------------------------------------------------------------------
        let recent_window_48h = now - chrono::Duration::hours(48);
        let mut recent_transitions_count = 0;

        for trans in input.context_transitions {
            let is_involving = trans.from_context_id.as_deref() == Some(input.context_id)
                || trans.to_context_id.as_deref() == Some(input.context_id);

            if is_involving {
                if let Some(t_dt) = parse_ts(&trans.detected_at) {
                    if t_dt >= recent_window_48h {
                        recent_transitions_count += 1;
                    }
                }
            }
        }

        indicators.recent_transitions_count = recent_transitions_count;

        let transition_influence = if recent_transitions_count >= 3 {
            evidence.push(LifecycleSourceEvidence {
                evidence_type: "context_transitions".to_string(),
                description: format!(
                    "High recent transition frequency ({} switches in last 48h) indicates active multi-context workflow.",
                    recent_transitions_count
                ),
                timestamp: now.to_rfc3339(),
                confidence: 0.90,
                reference_id: None,
            });
            -0.30
        } else if recent_transitions_count >= 1 {
            evidence.push(LifecycleSourceEvidence {
                evidence_type: "context_transitions".to_string(),
                description: format!(
                    "Context was part of recent workflow switches ({} transition in 48h).",
                    recent_transitions_count
                ),
                timestamp: now.to_rfc3339(),
                confidence: 0.85,
                reference_id: None,
            });
            -0.18
        } else {
            0.0
        };

        // ---------------------------------------------------------------------
        // Dimension 3: Task Status (InProgress vs Completed vs None)
        // ---------------------------------------------------------------------
        let mut task_status_influence = 0.0;

        if let Some(task) = input.active_task_candidate {
            let title_lower = task.title.to_lowercase();
            let is_done_or_completed = title_lower.contains("complete")
                || title_lower.contains("finished")
                || title_lower.contains("done")
                || title_lower.contains("submitted");

            if is_done_or_completed {
                indicators.task_completed_or_done = true;
                task_status_influence = 0.20;
                evidence.push(LifecycleSourceEvidence {
                    evidence_type: "task_status".to_string(),
                    description: format!("Associated task '{}' is marked completed/done.", task.title),
                    timestamp: now.to_rfc3339(),
                    confidence: 0.80,
                    reference_id: None,
                });
            } else {
                indicators.has_in_progress_task = true;
                indicators.in_progress_task_title = Some(task.title.clone());
                task_status_influence = -0.25;
                evidence.push(LifecycleSourceEvidence {
                    evidence_type: "task_status".to_string(),
                    description: format!(
                        "In-flight task '{}' remains active and unfinished (relevance: {:.2}).",
                        task.title, task.relevance_score
                    ),
                    timestamp: now.to_rfc3339(),
                    confidence: 0.88,
                    reference_id: None,
                });
            }
        }

        // ---------------------------------------------------------------------
        // Dimension 4: Recent Resume History
        // ---------------------------------------------------------------------
        let recent_window_7d = now - chrono::Duration::days(7);
        let mut recent_resumes_count = 0;

        for trans in input.context_transitions {
            if trans.to_context_id.as_deref() == Some(input.context_id)
                && (trans.transition_type == ContextTransitionType::ContextReturn
                    || trans.transition_type == ContextTransitionType::ContextStart)
            {
                if let Some(t_dt) = parse_ts(&trans.detected_at) {
                    if t_dt >= recent_window_7d {
                        recent_resumes_count += 1;
                    }
                }
            }
        }

        indicators.recent_resumes_count = recent_resumes_count;

        let resume_history_influence = if recent_resumes_count >= 2 {
            evidence.push(LifecycleSourceEvidence {
                evidence_type: "resume_history".to_string(),
                description: format!(
                    "Context has been resumed {} times in the last 7 days.",
                    recent_resumes_count
                ),
                timestamp: now.to_rfc3339(),
                confidence: 0.85,
                reference_id: None,
            });
            -0.25
        } else if recent_resumes_count == 1 {
            -0.12
        } else {
            0.0
        };

        // ---------------------------------------------------------------------
        // Dimension 5: Important Memory Salience
        // ---------------------------------------------------------------------
        // Invariant: Do NOT lower historical importance of memories simply because context is dormant!
        // We evaluate high-salience memories to see if there are active blockers or key open items.
        let high_salience_memories: Vec<&ContextImportantMemory> = input
            .important_memories
            .iter()
            .filter(|m| m.importance >= 0.70 || m.confidence >= 0.80)
            .collect();

        indicators.high_salience_memory_count = high_salience_memories.len();

        let memory_salience_influence = if high_salience_memories.is_empty() {
            0.0
        } else if high_salience_memories.len() >= 3 {
            evidence.push(LifecycleSourceEvidence {
                evidence_type: "important_memory".to_string(),
                description: format!(
                    "Preserved {} high-salience contextual memories (unresolved architectural context / blockers).",
                    high_salience_memories.len()
                ),
                timestamp: now.to_rfc3339(),
                confidence: 0.82,
                reference_id: None,
            });
            -0.20
        } else {
            evidence.push(LifecycleSourceEvidence {
                evidence_type: "important_memory".to_string(),
                description: format!(
                    "Preserved {} high-salience contextual memory.",
                    high_salience_memories.len()
                ),
                timestamp: now.to_rfc3339(),
                confidence: 0.80,
                reference_id: None,
            });
            -0.10
        };

        // ---------------------------------------------------------------------
        // Dimension 6: Workset In-Flight Resources
        // ---------------------------------------------------------------------
        let active_workset_count = input
            .workset
            .map(|ws| ws.items.len())
            .unwrap_or(0);

        indicators.active_workset_item_count = active_workset_count;

        let workset_influence = if active_workset_count >= 3 {
            evidence.push(LifecycleSourceEvidence {
                evidence_type: "workset_state".to_string(),
                description: format!(
                    "Active workset retains {} open files/resources ready for instant resumption.",
                    active_workset_count
                ),
                timestamp: now.to_rfc3339(),
                confidence: 0.88,
                reference_id: None,
            });
            -0.25
        } else if active_workset_count >= 1 {
            -0.12
        } else if elapsed_seconds > 3 * 86400 {
            // Empty workset + extended inactivity shifts slightly towards dormant
            0.10
        } else {
            0.0
        };

        // ---------------------------------------------------------------------
        // Dimension 7: Historical Activity Patterns & Recurrence
        // ---------------------------------------------------------------------
        // Check if segments/items span multiple distinct calendar days
        let mut distinct_days = std::collections::HashSet::new();
        for seg in input.activity_segments {
            if let Some(dt) = parse_ts(&seg.started_at) {
                distinct_days.insert(dt.format("%Y-%m-%d").to_string());
            }
        }
        for item in input.items {
            if let Some(dt) = parse_ts(&item.created_at) {
                distinct_days.insert(dt.format("%Y-%m-%d").to_string());
            }
        }

        let total_activity_records = input.items.len() + input.activity_segments.len();
        let has_recurring = distinct_days.len() >= 3;
        let is_isolated = distinct_days.len() <= 1 && total_activity_records <= 3;

        indicators.has_recurring_pattern = has_recurring;
        indicators.is_isolated_session = is_isolated;

        let pattern_recurrence_influence = if has_recurring {
            evidence.push(LifecycleSourceEvidence {
                evidence_type: "historical_pattern".to_string(),
                description: format!(
                    "Context demonstrates established recurring activity pattern across {} distinct days.",
                    distinct_days.len()
                ),
                timestamp: now.to_rfc3339(),
                confidence: 0.80,
                reference_id: None,
            });
            -0.20
        } else if is_isolated && elapsed_seconds > 2 * 86400 {
            evidence.push(LifecycleSourceEvidence {
                evidence_type: "historical_pattern".to_string(),
                description: "Context represents an isolated single-session query with no subsequent cadence.".to_string(),
                timestamp: now.to_rfc3339(),
                confidence: 0.75,
                reference_id: None,
            });
            0.20
        } else {
            0.0
        };

        // ---------------------------------------------------------------------
        // Aggregate Calibrated Score Computation
        // ---------------------------------------------------------------------
        let aggregate_dormancy_score = (recency_score
            + transition_influence
            + task_status_influence
            + resume_history_influence
            + memory_salience_influence
            + workset_influence
            + pattern_recurrence_influence)
            .clamp(0.0, 1.0);

        let dimension_scores = DormancyDimensionScores {
            recency_score,
            transition_influence,
            task_status_influence,
            resume_history_influence,
            memory_salience_influence,
            workset_influence,
            pattern_recurrence_influence,
            aggregate_dormancy_score,
        };

        let is_dormant = aggregate_dormancy_score >= cfg.dormancy_threshold;
        let state = if is_dormant {
            ContextLifecycleState::Dormant
        } else {
            ContextLifecycleState::Paused
        };

        let confidence = if is_dormant {
            (0.65 + (aggregate_dormancy_score - cfg.dormancy_threshold) * 0.70).clamp(0.60, 0.98)
        } else {
            (0.65 + (cfg.dormancy_threshold - aggregate_dormancy_score) * 0.70).clamp(0.60, 0.98)
        };

        let reason = if is_dormant {
            if indicators.task_completed_or_done {
                format!(
                    "Dormant context. Work on '{}' was completed and no active work cycle was observed for {} (score: {:.2}). Preserved as historical context.",
                    ws_name, indicators.elapsed_human, aggregate_dormancy_score
                )
            } else if indicators.is_isolated_session {
                format!(
                    "Dormant context. '{}' was an isolated task inactive for {} with no recurring activity (score: {:.2}). Preserved for historical reference.",
                    ws_name, indicators.elapsed_human, aggregate_dormancy_score
                )
            } else {
                format!(
                    "Dormant context. '{}' has had sustained absence from current work cycles (inactive for {}, score: {:.2}). Historical context and all memories preserved intact.",
                    ws_name, indicators.elapsed_human, aggregate_dormancy_score
                )
            }
        } else {
            if indicators.has_in_progress_task {
                format!(
                    "Paused context. In-flight task '{}' is actively open in '{}' (inactive for {}, score: {:.2}). Preserved ready for instant resumption.",
                    indicators.in_progress_task_title.as_deref().unwrap_or("Active Task"),
                    ws_name, indicators.elapsed_human, aggregate_dormancy_score
                )
            } else if indicators.recent_transitions_count > 0 {
                format!(
                    "Paused context. '{}' is part of an active multi-context work cycle with recent switches (score: {:.2}).",
                    ws_name, aggregate_dormancy_score
                )
            } else if indicators.has_recurring_pattern {
                format!(
                    "Paused context. '{}' has an established recurring schedule and open workset (score: {:.2}).",
                    ws_name, aggregate_dormancy_score
                )
            } else {
                format!(
                    "Paused context. '{}' was recently active ({}) and remains part of the current work cycle (score: {:.2}).",
                    ws_name, indicators.elapsed_human, aggregate_dormancy_score
                )
            }
        };

        evidence.push(LifecycleSourceEvidence {
            evidence_type: if is_dormant {
                "dormancy_assessment".to_string()
            } else {
                "paused_assessment".to_string()
            },
            description: reason.clone(),
            timestamp: now.to_rfc3339(),
            confidence,
            reference_id: Some(input.context_id.to_string()),
        });

        DormancyDistinctionAssessment {
            context_id: input.context_id.to_string(),
            lifecycle_state: state,
            is_dormant,
            dormancy_score: aggregate_dormancy_score,
            confidence,
            reason,
            dimension_scores,
            work_cycle_indicators: indicators,
            source_evidence: evidence,
            historical_importance_preserved: true,
        }
    }
}

// =========================================================================
// Helper Formatting & Timestamp Functions
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

fn format_duration_human(seconds: i64) -> String {
    if seconds < 60 {
        format!("{} seconds", seconds)
    } else if seconds < 3600 {
        format!("{} minutes", seconds / 60)
    } else if seconds < 86400 {
        let hours = seconds / 3600;
        let mins = (seconds % 3600) / 60;
        if mins > 0 {
            format!("{}h {}m", hours, mins)
        } else {
            format!("{} hours", hours)
        }
    } else {
        let days = seconds / 86400;
        let hours = (seconds % 86400) / 3600;
        if hours > 0 {
            format!("{}d {}h", days, hours)
        } else {
            format!("{} days", days)
        }
    }
}

// =========================================================================
// Unit Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_engine::important_memory::{
        ContextImportantMemory, ImportantMemoryExplanation, ImportantMemoryMetadata,
        ImportantMemoryType,
    };
    use crate::context_engine::memory::{InformationProvenance, MemoryExplanation};
    use crate::context_engine::workset::{
        ContextWorkset, WorksetItemReference, WorksetItemRelevance, WorksetState,
    };
    use serde_json::json;

    fn make_test_item(id: &str, ws_id: &str, ts: &str) -> Item {
        Item {
            id: id.to_string(),
            workspace_id: Some(ws_id.to_string()),
            session_id: Some("sess_1".to_string()),
            item_type: "text".to_string(),
            content: format!("Item {}", id),
            title: Some(format!("Title {}", id)),
            metadata: "{}".to_string(),
            created_at: ts.to_string(),
            updated_at: ts.to_string(),
        }
    }

    fn make_test_task(title: &str, relevance: f64) -> ExtractedTaskCandidate {
        ExtractedTaskCandidate {
            title: title.to_string(),
            action_verb: Some("Implementing".to_string()),
            target_subject: Some("Feature".to_string()),
            relevance_score: relevance,
            evidence_references: Vec::new(),
            item_ids: vec!["item_1".to_string()],
            primary_evidence_types: Vec::new(),
            salient_terms: vec!["auth".to_string(), "jwt".to_string()],
            context_id: Some("ws_test".to_string()),
            metadata: json!({}),
        }
    }

    fn make_test_workset(ws_id: &str, count: usize) -> ContextWorkset {
        let mut items = Vec::new();
        for i in 0..count {
            items.push(WorksetItemReference {
                id: format!("item_{}", i),
                title: Some(format!("src/lib_{}.rs", i)),
                item_type: "file".to_string(),
                relevance: WorksetItemRelevance::Primary,
                relevance_score: 0.90,
                reasons: vec!["active open file".to_string()],
                provenance: InformationProvenance::Observed,
                created_at: "2026-09-20T10:00:00Z".to_string(),
                last_active_at: Some("2026-09-20T10:00:00Z".to_string()),
                app_name: Some("VS Code".to_string()),
                domain: None,
                content_snippet: "fn main() {}".to_string(),
                is_suggested_addition: false,
            });
        }
        ContextWorkset {
            context_id: ws_id.to_string(),
            context_name: "Test Context".to_string(),
            task_id: Some("task_1".to_string()),
            task_title: Some("Active Task".to_string()),
            generated_at: "2026-09-20T10:00:00Z".to_string(),
            last_active_at: Some("2026-09-20T10:00:00Z".to_string()),
            workset_state: WorksetState::Active,
            items: items.clone(),
            recent_activity: "Working in VS Code".to_string(),
            relevant_items: items,
            important_references: Vec::new(),
            related_tasks: Vec::new(),
            confidence: 0.90,
            confidence_band: "High".to_string(),
            provenance: InformationProvenance::Observed,
            explanation: MemoryExplanation {
                summary: "Active workset".to_string(),
                why_reasons: vec!["Open files".to_string()],
                supporting_evidence: vec!["VS Code window active".to_string()],
                confidence_band: "High".to_string(),
                provenance: InformationProvenance::Observed,
                uncertainty_note: None,
            },
        }
    }

    fn make_test_memory(id: &str, ws_id: &str, importance: f64) -> ContextImportantMemory {
        ContextImportantMemory::new(
            id,
            ws_id,
            "source_1",
            ImportantMemoryType::ImportantItem,
            importance,
            0.90,
            Some("2026-09-15T10:00:00Z".to_string()),
            Some("2026-09-20T10:00:00Z".to_string()),
            InformationProvenance::Observed,
            format!("Architectural Decision {}", id),
            Some("Using JWT with Ed25519 signing tokens".to_string()),
            ImportantMemoryExplanation {
                summary: "Explicit architectural decision".to_string(),
                why_reasons: vec!["High structural importance".to_string()],
                supporting_evidence: vec!["Item 123".to_string()],
                confidence_band: "High".to_string(),
                provenance: InformationProvenance::Observed,
                uncertainty_note: None,
            },
            ImportantMemoryMetadata {
                item_type: Some("text".to_string()),
                app_name: Some("VS Code".to_string()),
                domain: None,
                action_verb: Some("Architecting".to_string()),
                target_subject: Some("JWT Auth".to_string()),
                session_count: Some(5),
                streak_count: Some(3),
                state_value: None,
                tags: vec!["architecture".to_string(), "security".to_string()],
            },
        )
    }

    #[test]
    fn test_dormancy_recent_inactivity_with_active_task_is_paused() {
        let now = Utc::now();
        // Inactive for 2 days
        let ts_2d_ago = (now - chrono::Duration::days(2)).to_rfc3339();

        let item = make_test_item("i1", "ws_auth", &ts_2d_ago);
        let task = make_test_task("Implementing OAuth2 Refresh Flow", 0.88);
        let workset = make_test_workset("ws_auth", 3);
        let memory = make_test_memory("mem1", "ws_auth", 0.85);

        let input = DormancyEvaluationInput {
            context_id: "ws_auth",
            context_name: Some("Auth Service"),
            workspace: None,
            items: &[item],
            activity_segments: &[],
            context_transitions: &[],
            task_transitions: &[],
            active_task_candidate: Some(&task),
            workset: Some(&workset),
            important_memories: &[memory],
            latest_snapshot: None,
            reference_time: Some(now),
        };

        let assessment = DormancyDistinctionEvaluator::evaluate(input, None);
        assert!(!assessment.is_dormant, "Should be PAUSED due to in-flight task and active workset");
        assert_eq!(assessment.lifecycle_state, ContextLifecycleState::Paused);
        assert!(assessment.dormancy_score < 0.50);
        assert!(assessment.reason.contains("In-flight task"));
        assert!(assessment.historical_importance_preserved);
    }

    #[test]
    fn test_dormancy_completed_task_with_empty_workset_becomes_dormant() {
        let now = Utc::now();
        // Inactive for 4 days
        let ts_4d_ago = (now - chrono::Duration::days(4)).to_rfc3339();

        let item = make_test_item("i1", "ws_tax", &ts_4d_ago);
        let completed_task = make_test_task("Done Tax Filing 2025 Submission", 0.90);

        let input = DormancyEvaluationInput {
            context_id: "ws_tax",
            context_name: Some("Tax Filing"),
            workspace: None,
            items: &[item],
            activity_segments: &[],
            context_transitions: &[],
            task_transitions: &[],
            active_task_candidate: Some(&completed_task),
            workset: None,
            important_memories: &[],
            latest_snapshot: None,
            reference_time: Some(now),
        };

        let assessment = DormancyDistinctionEvaluator::evaluate(input, None);
        assert!(assessment.is_dormant, "Should be DORMANT due to completed task and empty workset");
        assert_eq!(assessment.lifecycle_state, ContextLifecycleState::Dormant);
        assert!(assessment.dormancy_score >= 0.50);
        assert!(assessment.reason.contains("was completed"));
        assert!(assessment.historical_importance_preserved);
    }

    #[test]
    fn test_dormancy_recurring_project_over_weekend_remains_paused() {
        let now = Utc::now();
        // 3 days idle over a long weekend
        let d3 = (now - chrono::Duration::days(3)).to_rfc3339();
        let d4 = (now - chrono::Duration::days(4)).to_rfc3339();
        let d5 = (now - chrono::Duration::days(5)).to_rfc3339();

        let items = vec![
            make_test_item("i1", "ws_proj", &d5),
            make_test_item("i2", "ws_proj", &d4),
            make_test_item("i3", "ws_proj", &d3),
        ];

        let workset = make_test_workset("ws_proj", 4);
        let task = make_test_task("Refactoring Database Migrations", 0.85);

        let input = DormancyEvaluationInput {
            context_id: "ws_proj",
            context_name: Some("Core Backend"),
            workspace: None,
            items: &items,
            activity_segments: &[],
            context_transitions: &[],
            task_transitions: &[],
            active_task_candidate: Some(&task),
            workset: Some(&workset),
            important_memories: &[],
            latest_snapshot: None,
            reference_time: Some(now),
        };

        let assessment = DormancyDistinctionEvaluator::evaluate(input, None);
        assert!(!assessment.is_dormant, "Recurring multi-day project should remain PAUSED over 3-day gap");
        assert_eq!(assessment.lifecycle_state, ContextLifecycleState::Paused);
        assert!(assessment.work_cycle_indicators.has_recurring_pattern);
    }

    #[test]
    fn test_dormancy_isolated_single_session_becomes_dormant() {
        let now = Utc::now();
        // 3 days idle for a single isolated search
        let ts_3d = (now - chrono::Duration::days(3)).to_rfc3339();
        let item = make_test_item("i1", "ws_flight", &ts_3d);

        let input = DormancyEvaluationInput {
            context_id: "ws_flight",
            context_name: Some("Flight Booking"),
            workspace: None,
            items: &[item],
            activity_segments: &[],
            context_transitions: &[],
            task_transitions: &[],
            active_task_candidate: None,
            workset: None,
            important_memories: &[],
            latest_snapshot: None,
            reference_time: Some(now),
        };

        let assessment = DormancyDistinctionEvaluator::evaluate(input, None);
        assert!(assessment.is_dormant, "Isolated single-session context should become DORMANT after 3 days");
        assert_eq!(assessment.lifecycle_state, ContextLifecycleState::Dormant);
        assert!(assessment.work_cycle_indicators.is_isolated_session);
    }

    #[test]
    fn test_dormancy_recent_context_transitions_keep_context_paused() {
        let now = Utc::now();
        // 2 days idle, but switched to/from in the last 24h as part of multi-context sprint
        let ts_2d = (now - chrono::Duration::days(2)).to_rfc3339();
        let ts_12h = (now - chrono::Duration::hours(12)).to_rfc3339();

        let item = make_test_item("i1", "ws_frontend", &ts_2d);

        let mut trans1 = ContextTransition::new(
            Some("ws_frontend".to_string()),
            Some("ws_backend".to_string()),
            ContextTransitionType::ContextSwitch,
            0.90,
        );
        trans1.detected_at = ts_12h.clone();

        let mut trans2 = ContextTransition::new(
            Some("ws_backend".to_string()),
            Some("ws_frontend".to_string()),
            ContextTransitionType::ContextReturn,
            0.92,
        );
        trans2.detected_at = ts_12h;

        let input = DormancyEvaluationInput {
            context_id: "ws_frontend",
            context_name: Some("Frontend App"),
            workspace: None,
            items: &[item],
            activity_segments: &[],
            context_transitions: &[trans1, trans2],
            task_transitions: &[],
            active_task_candidate: None,
            workset: None,
            important_memories: &[],
            latest_snapshot: None,
            reference_time: Some(now),
        };

        let assessment = DormancyDistinctionEvaluator::evaluate(input, None);
        assert!(!assessment.is_dormant, "Active context switching in 24h keeps context PAUSED");
        assert_eq!(assessment.lifecycle_state, ContextLifecycleState::Paused);
        assert!(assessment.dimension_scores.transition_influence < 0.0);
    }

    #[test]
    fn test_dormancy_preserves_historical_importance_and_memories() {
        let now = Utc::now();
        // 45 days idle (deep dormancy)
        let ts_45d = (now - chrono::Duration::days(45)).to_rfc3339();

        let item = make_test_item("i_old", "ws_historical", &ts_45d);
        let memory = make_test_memory("mem_crit", "ws_historical", 0.95);

        let input = DormancyEvaluationInput {
            context_id: "ws_historical",
            context_name: Some("Historical Migration"),
            workspace: None,
            items: &[item],
            activity_segments: &[],
            context_transitions: &[],
            task_transitions: &[],
            active_task_candidate: None,
            workset: None,
            important_memories: &[memory.clone()],
            latest_snapshot: None,
            reference_time: Some(now),
        };

        let assessment = DormancyDistinctionEvaluator::evaluate(input, None);
        assert!(assessment.is_dormant);
        assert_eq!(assessment.lifecycle_state, ContextLifecycleState::Dormant);
        // Critical invariant: Historical importance is strictly preserved!
        assert!(assessment.historical_importance_preserved);
        assert_eq!(memory.importance, 0.95, "Memory importance score must never be degraded");
    }

    #[test]
    fn test_dormancy_json_serialization_roundtrip() {
        let now = Utc::now();
        let assessment = DormancyDistinctionAssessment {
            context_id: "ws_demo".to_string(),
            lifecycle_state: ContextLifecycleState::Paused,
            is_dormant: false,
            dormancy_score: 0.28,
            confidence: 0.88,
            reason: "Paused context. In-flight task is actively open.".to_string(),
            dimension_scores: DormancyDimensionScores {
                recency_score: 0.25,
                transition_influence: -0.15,
                task_status_influence: -0.20,
                resume_history_influence: -0.10,
                memory_salience_influence: -0.10,
                workset_influence: -0.15,
                pattern_recurrence_influence: -0.10,
                aggregate_dormancy_score: 0.28,
            },
            work_cycle_indicators: WorkCycleIndicators {
                elapsed_seconds: 7200,
                elapsed_human: "2 hours".to_string(),
                has_in_progress_task: true,
                in_progress_task_title: Some("Build UI".to_string()),
                task_completed_or_done: false,
                active_workset_item_count: 3,
                high_salience_memory_count: 2,
                recent_transitions_count: 1,
                recent_resumes_count: 1,
                has_recurring_pattern: true,
                is_isolated_session: false,
            },
            source_evidence: vec![LifecycleSourceEvidence {
                evidence_type: "paused_assessment".to_string(),
                description: "Paused context evaluation completed.".to_string(),
                timestamp: now.to_rfc3339(),
                confidence: 0.88,
                reference_id: Some("ws_demo".to_string()),
            }],
            historical_importance_preserved: true,
        };

        let json = serde_json::to_string(&assessment).expect("Serialization failed");
        assert!(json.contains("\"is_dormant\":false"));
        assert!(json.contains("\"lifecycle_state\":\"paused\""));

        let deserialized: DormancyDistinctionAssessment =
            serde_json::from_str(&json).expect("Deserialization failed");
        assert_eq!(deserialized, assessment);
    }
}

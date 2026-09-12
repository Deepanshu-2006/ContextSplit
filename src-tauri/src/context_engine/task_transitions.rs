use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::VecDeque;
use uuid::Uuid;

use crate::context_engine::task_extractor::is_generic_task_name;
use crate::context_engine::task_ranking::RankedTaskCandidate;

// =========================================================================
// Data Models & Configuration
// =========================================================================

/// Classification of transitions between inferred tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskTransitionType {
    /// Emergence of initial task activity within a context
    TaskStart,
    /// Decisive shift from Task A to Task B within the same context
    TaskSwitch,
    /// Resumption of a previously active Task A after a detour to Task B
    TaskReturn,
    /// Inactivity gap or completion of the active task
    TaskEnd,
    /// Conflicting or low-confidence task shift without sufficient stability
    AmbiguousTaskChange,
}

impl TaskTransitionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskTransitionType::TaskStart => "task_start",
            TaskTransitionType::TaskSwitch => "task_switch",
            TaskTransitionType::TaskReturn => "task_return",
            TaskTransitionType::TaskEnd => "task_end",
            TaskTransitionType::AmbiguousTaskChange => "ambiguous_task_change",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "task_start" => Ok(TaskTransitionType::TaskStart),
            "task_switch" => Ok(TaskTransitionType::TaskSwitch),
            "task_return" => Ok(TaskTransitionType::TaskReturn),
            "task_end" => Ok(TaskTransitionType::TaskEnd),
            "ambiguous_task_change" => Ok(TaskTransitionType::AmbiguousTaskChange),
            _ => Err(format!("Unknown task transition type: '{}'", s)),
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            TaskTransitionType::TaskStart => "Task Started",
            TaskTransitionType::TaskSwitch => "Task Switched",
            TaskTransitionType::TaskReturn => "Task Resumed",
            TaskTransitionType::TaskEnd => "Task Ended",
            TaskTransitionType::AmbiguousTaskChange => "Ambiguous Task Change",
        }
    }
}

/// Working memory snapshot of an active task within a context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskSnapshot {
    pub title: String,
    pub action_verb: Option<String>,
    pub target_subject: Option<String>,
    pub confidence: f64,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub streak_count: usize,
    pub item_ids: Vec<String>,
    pub context_id: Option<String>,
}

impl TaskSnapshot {
    pub fn new(
        title: impl Into<String>,
        verb: Option<String>,
        subject: Option<String>,
        confidence: f64,
        timestamp: &str,
        context_id: Option<String>,
    ) -> Self {
        Self {
            title: title.into(),
            action_verb: verb,
            target_subject: subject,
            confidence,
            first_seen_at: timestamp.to_string(),
            last_seen_at: timestamp.to_string(),
            streak_count: 1,
            item_ids: Vec::new(),
            context_id,
        }
    }

    /// Checks if another candidate title/subject represents the same continuing task.
    pub fn is_same_task(&self, other_title: &str, other_subject: Option<&str>) -> bool {
        let title_a = self.title.trim().to_lowercase();
        let title_b = other_title.trim().to_lowercase();

        if title_a == title_b {
            return true;
        }

        // Compare target subjects if present
        if let (Some(ref sub_a), Some(sub_b)) = (&self.target_subject, other_subject) {
            let s_a = sub_a.trim().to_lowercase();
            let s_b = sub_b.trim().to_lowercase();
            if s_a == s_b || s_a.contains(&s_b) || s_b.contains(&s_a) {
                return true;
            }
        }

        // Word overlap check (Jaccard similarity on tokens)
        let words_a: std::collections::HashSet<&str> = title_a.split_whitespace().collect();
        let words_b: std::collections::HashSet<&str> = title_b.split_whitespace().collect();
        let intersection = words_a.intersection(&words_b).count();
        let union = words_a.union(&words_b).count();

        if union > 0 && (intersection as f64 / union as f64) >= 0.60 {
            return true;
        }

        false
    }

    /// Updates active task state on continuing observation.
    pub fn touch(&mut self, timestamp: &str, item_id: Option<&str>, confidence: f64) {
        self.last_seen_at = timestamp.to_string();
        self.streak_count += 1;
        self.confidence = (self.confidence * 0.7 + confidence * 0.3).clamp(0.0, 1.0);
        if let Some(id) = item_id {
            if !self.item_ids.iter().any(|i| i == id) {
                self.item_ids.push(id.to_string());
            }
        }
    }
}

/// Provisional task transition under observation awaiting sustained hysteresis confirmation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingTaskTransition {
    pub target_title: String,
    pub target_subject: Option<String>,
    pub target_verb: Option<String>,
    pub confidence: f64,
    pub first_observed_at: String,
    pub last_observed_at: String,
    pub observation_count: usize,
    pub evidence: Vec<String>,
    pub trigger_item_id: Option<String>,
    pub context_id: Option<String>,
}

/// Structured record of a detected task transition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DetectedTaskTransition {
    pub id: String,
    pub transition_type: TaskTransitionType,
    pub from_task: Option<String>,
    pub to_task: Option<String>,
    pub context_id: Option<String>,
    pub timestamp: String,
    pub confidence: f64,
    pub evidence: Vec<String>,
    pub trigger_item_id: Option<String>,
    pub duration_seconds: Option<i64>,
    pub metadata: Value,
}

/// Evaluation result of the Task Transition Detector for a stream event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskTransitionEvaluationResult {
    /// Active task continues steadily without a shift
    TaskContinuation,
    /// Verified, confirmed task transition emitted
    TransitionDetected(DetectedTaskTransition),
    /// Candidate task shift buffered in hysteresis pending 2nd observation
    ObservationBuffered,
    /// Transient noise or weak single-item fluctuation suppressed
    NoiseFiltered,
}

/// Configuration options for task transition stability, hysteresis, and return horizons.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTransitionConfig {
    /// Minimum confidence threshold required to initiate a task transition (default: 0.45)
    pub confidence_threshold: f64,
    /// Minimum consecutive observations required to confirm moderate transition (default: 2)
    pub min_consecutive_observations: usize,
    /// Confidence threshold to fast-track immediate task transitions (default: 0.85)
    pub fast_track_confidence: f64,
    /// Maximum inactive gap in seconds before emitting TaskEnd (default: 1800s / 30m)
    pub inactivity_threshold_seconds: i64,
    /// Maximum elapsed time since previous activity to detect a TaskReturn (default: 86400s / 24h)
    pub max_return_gap_seconds: i64,
    /// Maximum history depth of past task snapshots (default: 5)
    pub max_history_depth: usize,
}

impl Default for TaskTransitionConfig {
    fn default() -> Self {
        Self {
            confidence_threshold: 0.45,
            min_consecutive_observations: 2,
            fast_track_confidence: 0.85,
            inactivity_threshold_seconds: 1800,
            max_return_gap_seconds: 86400,
            max_history_depth: 5,
        }
    }
}

// =========================================================================
// Task Transition Detector Engine
// =========================================================================

/// State machine for detecting intra-context task transitions (`task_start`, `task_switch`,
/// `task_return`, `task_end`, `ambiguous_task_change`) with hysteresis and anti-jitter.
#[derive(Debug, Clone)]
pub struct TaskTransitionDetector {
    pub config: TaskTransitionConfig,
    pub current_task: Option<TaskSnapshot>,
    pub task_history: VecDeque<TaskSnapshot>,
    pub pending_transition: Option<PendingTaskTransition>,
    pub last_emitted: Option<(TaskTransitionType, String, String)>,
}

impl Default for TaskTransitionDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskTransitionDetector {
    pub fn new() -> Self {
        Self {
            config: TaskTransitionConfig::default(),
            current_task: None,
            task_history: VecDeque::new(),
            pending_transition: None,
            last_emitted: None,
        }
    }

    pub fn with_config(config: TaskTransitionConfig) -> Self {
        Self {
            config,
            current_task: None,
            task_history: VecDeque::new(),
            pending_transition: None,
            last_emitted: None,
        }
    }

    /// Resets working memory state.
    pub fn reset(&mut self) {
        self.current_task = None;
        self.task_history.clear();
        self.pending_transition = None;
        self.last_emitted = None;
    }

    /// Evaluates a stream event using the top ranked task candidate.
    pub fn evaluate_ranked_candidate(
        &mut self,
        ranked: Option<&RankedTaskCandidate>,
        timestamp: &str,
        item_id: Option<&str>,
        context_id: Option<&str>,
    ) -> TaskTransitionEvaluationResult {
        let current_time = timestamp
            .parse::<DateTime<Utc>>()
            .unwrap_or_else(|_| Utc::now());

        // 1. Inactivity Gap Check: If elapsed time > inactivity threshold, emit TaskEnd
        let inactivity_end = if let Some(ref active) = self.current_task {
            let last_dt = active
                .last_seen_at
                .parse::<DateTime<Utc>>()
                .unwrap_or(current_time);
            let gap_seconds = current_time.signed_duration_since(last_dt).num_seconds();

            if gap_seconds > self.config.inactivity_threshold_seconds {
                let from_title = active.title.clone();
                let last_seen = active.last_seen_at.clone();
                let first_seen_dt = active
                    .first_seen_at
                    .parse::<DateTime<Utc>>()
                    .unwrap_or(last_dt);
                let duration = last_dt.signed_duration_since(first_seen_dt).num_seconds().max(0);
                Some((from_title, last_seen, duration, gap_seconds))
            } else {
                None
            }
        } else {
            None
        };

        if let Some((from_title, last_seen, duration, gap_seconds)) = inactivity_end {
            self.archive_current_task();

            let transition = DetectedTaskTransition {
                id: Uuid::new_v4().to_string(),
                transition_type: TaskTransitionType::TaskEnd,
                from_task: Some(from_title),
                to_task: None,
                context_id: context_id.map(|s| s.to_string()),
                timestamp: last_seen,
                confidence: 0.90,
                evidence: vec![format!(
                    "Inactivity gap of {}s exceeded {}s threshold",
                    gap_seconds, self.config.inactivity_threshold_seconds
                )],
                trigger_item_id: None,
                duration_seconds: Some(duration),
                metadata: json!({ "gap_seconds": gap_seconds }),
            };

            self.last_emitted = Some((
                TaskTransitionType::TaskEnd,
                transition.from_task.clone().unwrap_or_default(),
                String::new(),
            ));

            return TaskTransitionEvaluationResult::TransitionDetected(transition);
        }

        // If no candidate is provided, return NoiseFiltered
        let top = match ranked {
            Some(r) => r,
            None => return TaskTransitionEvaluationResult::NoiseFiltered,
        };

        // 2. Anti-Generic & Noise Filter
        // Filter out unspecialized generic task candidates or candidates with confidence below threshold
        if is_generic_task_name(&top.candidate.title) {
            let is_specialized = top
                .candidate
                .metadata
                .get("is_specialized")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !is_specialized {
                return TaskTransitionEvaluationResult::NoiseFiltered;
            }
        }

        if top.confidence < self.config.confidence_threshold {
            return TaskTransitionEvaluationResult::NoiseFiltered;
        }

        let cand_title = &top.candidate.title;
        let cand_subject = top.candidate.target_subject.as_deref();
        let cand_verb = top.candidate.action_verb.clone();
        let cand_conf = top.confidence;

        // 3. Task Continuation Check: Does this candidate match the currently active task?
        if let Some(ref mut active) = self.current_task {
            if active.is_same_task(cand_title, cand_subject) {
                active.touch(timestamp, item_id, cand_conf);
                self.pending_transition = None;
                return TaskTransitionEvaluationResult::TaskContinuation;
            }
        }

        // 4. Task Return Check: Does this candidate match a previous task in task_history?
        let mut return_snapshot_idx: Option<usize> = None;
        for (idx, snap) in self.task_history.iter().enumerate() {
            if snap.is_same_task(cand_title, cand_subject) {
                let last_seen_dt = snap
                    .last_seen_at
                    .parse::<DateTime<Utc>>()
                    .unwrap_or(current_time);
                let gap_seconds = current_time
                    .signed_duration_since(last_seen_dt)
                    .num_seconds()
                    .max(0);

                if gap_seconds <= self.config.max_return_gap_seconds {
                    return_snapshot_idx = Some(idx);
                    break;
                }
            }
        }

        // 5. Hysteresis & Transition Emission Logic
        let is_fast_track = cand_conf >= self.config.fast_track_confidence
            && top.candidate.item_ids.len() >= 2;

        let evidence_summary: Vec<String> = top
            .supporting_evidence
            .iter()
            .map(|e| format!("{}: {}", e.evidence_type.as_str(), e.summary))
            .collect();

        // Check pending hysteresis buffer
        let is_pending_match = if let Some(ref pending) = self.pending_transition {
            pending.target_title == *cand_title
                || (pending.target_subject.is_some()
                    && pending.target_subject.as_deref() == cand_subject)
        } else {
            false
        };

        if is_pending_match {
            let mut pending = self.pending_transition.take().unwrap();
            pending.observation_count += 1;
            pending.last_observed_at = timestamp.to_string();

            if pending.observation_count >= self.config.min_consecutive_observations {
                // Hysteresis met: confirm transition!
                let (trans_type, from_title) = if let Some(ret_idx) = return_snapshot_idx {
                    // Task Return
                    let prev = self.task_history.remove(ret_idx);
                    let from_title = self.current_task.as_ref().map(|t| t.title.clone());
                    self.archive_current_task();
                    self.current_task = Some(TaskSnapshot {
                        title: pending.target_title.clone(),
                        action_verb: pending.target_verb,
                        target_subject: pending.target_subject,
                        confidence: pending.confidence,
                        first_seen_at: prev.map(|p| p.first_seen_at).unwrap_or_else(|| timestamp.to_string()),
                        last_seen_at: timestamp.to_string(),
                        streak_count: 1,
                        item_ids: item_id.map(|id| vec![id.to_string()]).unwrap_or_default(),
                        context_id: context_id.map(|s| s.to_string()),
                    });
                    (TaskTransitionType::TaskReturn, from_title)
                } else if let Some(ref current) = self.current_task {
                    // Task Switch
                    let from_title = Some(current.title.clone());
                    self.archive_current_task();
                    self.current_task = Some(TaskSnapshot::new(
                        pending.target_title.clone(),
                        pending.target_verb,
                        pending.target_subject,
                        pending.confidence,
                        timestamp,
                        context_id.map(|s| s.to_string()),
                    ));
                    (TaskTransitionType::TaskSwitch, from_title)
                } else {
                    // Task Start
                    self.current_task = Some(TaskSnapshot::new(
                        pending.target_title.clone(),
                        pending.target_verb,
                        pending.target_subject,
                        pending.confidence,
                        timestamp,
                        context_id.map(|s| s.to_string()),
                    ));
                    (TaskTransitionType::TaskStart, None)
                };

                let transition = DetectedTaskTransition {
                    id: Uuid::new_v4().to_string(),
                    transition_type: trans_type,
                    from_task: from_title,
                    to_task: Some(pending.target_title.clone()),
                    context_id: context_id.map(|s| s.to_string()),
                    timestamp: timestamp.to_string(),
                    confidence: pending.confidence,
                    evidence: pending.evidence,
                    trigger_item_id: item_id.map(|s| s.to_string()),
                    duration_seconds: None,
                    metadata: json!({ "observation_count": pending.observation_count }),
                };

                self.last_emitted = Some((
                    trans_type,
                    transition.from_task.clone().unwrap_or_default(),
                    transition.to_task.clone().unwrap_or_default(),
                ));

                return TaskTransitionEvaluationResult::TransitionDetected(transition);
            } else {
                // Re-buffer pending transition
                self.pending_transition = Some(pending);
                return TaskTransitionEvaluationResult::ObservationBuffered;
            }
        }

        // Fast-track immediate transition on overwhelming evidence
        if is_fast_track {
            self.pending_transition = None;
            let (trans_type, from_title) = if let Some(ret_idx) = return_snapshot_idx {
                let prev = self.task_history.remove(ret_idx);
                let from_title = self.current_task.as_ref().map(|t| t.title.clone());
                self.archive_current_task();
                self.current_task = Some(TaskSnapshot {
                    title: cand_title.clone(),
                    action_verb: cand_verb,
                    target_subject: cand_subject.map(|s| s.to_string()),
                    confidence: cand_conf,
                    first_seen_at: prev.map(|p| p.first_seen_at).unwrap_or_else(|| timestamp.to_string()),
                    last_seen_at: timestamp.to_string(),
                    streak_count: 1,
                    item_ids: item_id.map(|id| vec![id.to_string()]).unwrap_or_default(),
                    context_id: context_id.map(|s| s.to_string()),
                });
                (TaskTransitionType::TaskReturn, from_title)
            } else if let Some(ref current) = self.current_task {
                let from_title = Some(current.title.clone());
                self.archive_current_task();
                self.current_task = Some(TaskSnapshot::new(
                    cand_title.clone(),
                    cand_verb,
                    cand_subject.map(|s| s.to_string()),
                    cand_conf,
                    timestamp,
                    context_id.map(|s| s.to_string()),
                ));
                (TaskTransitionType::TaskSwitch, from_title)
            } else {
                self.current_task = Some(TaskSnapshot::new(
                    cand_title.clone(),
                    cand_verb,
                    cand_subject.map(|s| s.to_string()),
                    cand_conf,
                    timestamp,
                    context_id.map(|s| s.to_string()),
                ));
                (TaskTransitionType::TaskStart, None)
            };

            let transition = DetectedTaskTransition {
                id: Uuid::new_v4().to_string(),
                transition_type: trans_type,
                from_task: from_title,
                to_task: Some(cand_title.clone()),
                context_id: context_id.map(|s| s.to_string()),
                timestamp: timestamp.to_string(),
                confidence: cand_conf,
                evidence: evidence_summary,
                trigger_item_id: item_id.map(|s| s.to_string()),
                duration_seconds: None,
                metadata: json!({ "fast_tracked": true }),
            };

            self.last_emitted = Some((
                trans_type,
                transition.from_task.clone().unwrap_or_default(),
                transition.to_task.clone().unwrap_or_default(),
            ));

            return TaskTransitionEvaluationResult::TransitionDetected(transition);
        }

        // Buffer moderate candidate in pending transition
        self.pending_transition = Some(PendingTaskTransition {
            target_title: cand_title.clone(),
            target_subject: cand_subject.map(|s| s.to_string()),
            target_verb: cand_verb,
            confidence: cand_conf,
            first_observed_at: timestamp.to_string(),
            last_observed_at: timestamp.to_string(),
            observation_count: 1,
            evidence: evidence_summary,
            trigger_item_id: item_id.map(|s| s.to_string()),
            context_id: context_id.map(|s| s.to_string()),
        });

        TaskTransitionEvaluationResult::ObservationBuffered
    }

    /// Evaluates a sequence of ranked task events and returns all detected transitions.
    pub fn evaluate_sequence(
        &mut self,
        events: &[(RankedTaskCandidate, String, Option<String>)],
        context_id: Option<&str>,
    ) -> Vec<DetectedTaskTransition> {
        let mut transitions = Vec::new();
        for (ranked, timestamp, item_id) in events {
            if let TaskTransitionEvaluationResult::TransitionDetected(t) = self
                .evaluate_ranked_candidate(
                    Some(ranked),
                    timestamp,
                    item_id.as_deref(),
                    context_id,
                )
            {
                transitions.push(t);
            }
        }
        transitions
    }

    /// Archives the currently active task snapshot into bounded history.
    fn archive_current_task(&mut self) {
        if let Some(task) = self.current_task.take() {
            if self.task_history.len() >= self.config.max_history_depth {
                self.task_history.pop_back();
            }
            self.task_history.push_front(task);
        }
    }
}

// =========================================================================
// Unit & Scenario Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_engine::task_extractor::{ExtractedTaskCandidate, TaskCandidateEvidenceRef};
    use crate::context_engine::task_evidence::{TaskEvidence, TaskEvidenceType};
    use crate::context_engine::task_ranking::TaskRankingDimensionScores;

    fn make_ranked_candidate(
        title: &str,
        verb: Option<&str>,
        subject: Option<&str>,
        confidence: f64,
        item_count: usize,
    ) -> RankedTaskCandidate {
        let ev = TaskEvidence::new(
            TaskEvidenceType::Filenames,
            TaskEvidenceType::Filenames.default_provenance(),
            0.85,
            format!("Evidence for {}", title),
            "Detail".to_string(),
        );

        let item_ids: Vec<String> = (0..item_count).map(|i| format!("item_{}", i)).collect();

        let candidate = ExtractedTaskCandidate {
            title: title.to_string(),
            action_verb: verb.map(|s| s.to_string()),
            target_subject: subject.map(|s| s.to_string()),
            relevance_score: confidence,
            evidence_references: vec![TaskCandidateEvidenceRef::from_task_evidence(&ev)],
            item_ids,
            primary_evidence_types: vec![TaskEvidenceType::Filenames],
            salient_terms: vec![],
            context_id: Some("ws_test".to_string()),
            metadata: json!({ "is_specialized": false }),
        };

        RankedTaskCandidate {
            candidate,
            score: confidence,
            confidence,
            confidence_band: if confidence >= 0.75 {
                "high".to_string()
            } else {
                "medium".to_string()
            },
            rank: 1,
            supporting_evidence: vec![],
            contradictory_evidence: vec![],
            dimension_scores: TaskRankingDimensionScores {
                recency_score: confidence,
                evidence_strength_score: 0.85,
                evidence_diversity_score: 0.70,
                semantic_coherence_score: 0.80,
                activity_frequency_score: 0.80,
                session_continuity_score: 0.80,
                contextual_relevance_score: 0.85,
                stability_score: 0.80,
            },
            margin_over_next: None,
        }
    }

    #[test]
    fn test_scenario_portfolio_hero_animation_to_case_study() {
        let mut detector = TaskTransitionDetector::new();

        // 1. Initial sustained activity in "Hero animation"
        let hero_1 = make_ranked_candidate("Hero animation", Some("Implementing"), Some("Hero animation"), 0.88, 2);
        let r1 = detector.evaluate_ranked_candidate(Some(&hero_1), "2026-09-13T01:00:00Z", Some("i1"), Some("ws_portfolio"));

        // Fast-tracked task_start
        match r1 {
            TaskTransitionEvaluationResult::TransitionDetected(t) => {
                assert_eq!(t.transition_type, TaskTransitionType::TaskStart);
                assert_eq!(t.to_task.as_deref(), Some("Hero animation"));
            }
            _ => panic!("Expected TaskStart"),
        }

        // Hero animation continues
        let hero_2 = make_ranked_candidate("Hero animation", Some("Implementing"), Some("Hero animation"), 0.85, 2);
        let r2 = detector.evaluate_ranked_candidate(Some(&hero_2), "2026-09-13T01:05:00Z", Some("i2"), Some("ws_portfolio"));
        assert_eq!(r2, TaskTransitionEvaluationResult::TaskContinuation);

        // 2. Transition to "Case study writing" (Observation 1 -> Buffered, Observation 2 -> Confirmed Switch)
        let case_study = make_ranked_candidate("Case study writing", Some("Drafting"), Some("Case study writing"), 0.78, 2);

        let r3 = detector.evaluate_ranked_candidate(Some(&case_study), "2026-09-13T01:10:00Z", Some("i3"), Some("ws_portfolio"));
        assert_eq!(r3, TaskTransitionEvaluationResult::ObservationBuffered);

        let r4 = detector.evaluate_ranked_candidate(Some(&case_study), "2026-09-13T01:15:00Z", Some("i4"), Some("ws_portfolio"));
        match r4 {
            TaskTransitionEvaluationResult::TransitionDetected(t) => {
                assert_eq!(t.transition_type, TaskTransitionType::TaskSwitch);
                assert_eq!(t.from_task.as_deref(), Some("Hero animation"));
                assert_eq!(t.to_task.as_deref(), Some("Case study writing"));
            }
            _ => panic!("Expected TaskSwitch"),
        }
    }

    #[test]
    fn test_scenario_college_os_assignment_to_database() {
        let mut detector = TaskTransitionDetector::new();

        // 1. Initial OS assignment
        let os_task = make_ranked_candidate("OS assignment", Some("Coding"), Some("OS assignment"), 0.88, 2);
        let r1 = detector.evaluate_ranked_candidate(Some(&os_task), "2026-09-13T02:00:00Z", Some("i1"), Some("ws_college"));
        assert!(matches!(r1, TaskTransitionEvaluationResult::TransitionDetected(_)));

        // 2. Fast-tracked switch to Database assignment
        let db_task = make_ranked_candidate("Database assignment", Some("Designing"), Some("Database assignment"), 0.88, 2);
        let r2 = detector.evaluate_ranked_candidate(Some(&db_task), "2026-09-13T02:20:00Z", Some("i2"), Some("ws_college"));

        match r2 {
            TaskTransitionEvaluationResult::TransitionDetected(t) => {
                assert_eq!(t.transition_type, TaskTransitionType::TaskSwitch);
                assert_eq!(t.from_task.as_deref(), Some("OS assignment"));
                assert_eq!(t.to_task.as_deref(), Some("Database assignment"));
            }
            _ => panic!("Expected fast-tracked TaskSwitch"),
        }
    }

    #[test]
    fn test_noisy_unrelated_captures_filtered() {
        let mut detector = TaskTransitionDetector::new();

        // Active task: "OS assignment"
        let os_task = make_ranked_candidate("OS assignment", Some("Coding"), Some("OS assignment"), 0.88, 2);
        detector.evaluate_ranked_candidate(Some(&os_task), "2026-09-13T02:00:00Z", Some("i1"), Some("ws_college"));

        // Single isolated noisy capture: "YouTube Video" (confidence 0.40)
        let noisy = make_ranked_candidate("YouTube Video", None, None, 0.40, 1);
        let r_noise = detector.evaluate_ranked_candidate(Some(&noisy), "2026-09-13T02:05:00Z", Some("i_noise"), Some("ws_college"));
        assert_eq!(r_noise, TaskTransitionEvaluationResult::NoiseFiltered);

        // Generic Google Search
        let generic = make_ranked_candidate("Google Search", None, None, 0.50, 1);
        let r_gen = detector.evaluate_ranked_candidate(Some(&generic), "2026-09-13T02:07:00Z", Some("i_gen"), Some("ws_college"));
        assert_eq!(r_gen, TaskTransitionEvaluationResult::NoiseFiltered);

        // Continuation of OS assignment remains active
        let os_cont = make_ranked_candidate("OS assignment", Some("Coding"), Some("OS assignment"), 0.85, 2);
        let r_cont = detector.evaluate_ranked_candidate(Some(&os_cont), "2026-09-13T02:10:00Z", Some("i2"), Some("ws_college"));
        assert_eq!(r_cont, TaskTransitionEvaluationResult::TaskContinuation);
        assert_eq!(detector.current_task.as_ref().unwrap().title, "OS assignment");
    }

    #[test]
    fn test_task_return_detection() {
        let mut detector = TaskTransitionDetector::new();

        // 1. Task A starts: "Hero animation"
        let task_a = make_ranked_candidate("Hero animation", Some("Implementing"), Some("Hero animation"), 0.88, 2);
        detector.evaluate_ranked_candidate(Some(&task_a), "2026-09-13T01:00:00Z", Some("i1"), Some("ws_portfolio"));

        // 2. Switch to Task B: "Case study writing"
        let task_b = make_ranked_candidate("Case study writing", Some("Drafting"), Some("Case study writing"), 0.88, 2);
        detector.evaluate_ranked_candidate(Some(&task_b), "2026-09-13T01:30:00Z", Some("i2"), Some("ws_portfolio"));

        // 3. Return to Task A: "Hero animation"
        let task_a_return = make_ranked_candidate("Hero animation", Some("Implementing"), Some("Hero animation"), 0.88, 2);
        let r_return = detector.evaluate_ranked_candidate(Some(&task_a_return), "2026-09-13T02:00:00Z", Some("i3"), Some("ws_portfolio"));

        match r_return {
            TaskTransitionEvaluationResult::TransitionDetected(t) => {
                assert_eq!(t.transition_type, TaskTransitionType::TaskReturn);
                assert_eq!(t.from_task.as_deref(), Some("Case study writing"));
                assert_eq!(t.to_task.as_deref(), Some("Hero animation"));
            }
            _ => panic!("Expected TaskReturn"),
        }
    }

    #[test]
    fn test_inactivity_task_end() {
        let mut detector = TaskTransitionDetector::new();

        // 1. Active task at 01:00
        let task = make_ranked_candidate("Hero animation", Some("Implementing"), Some("Hero animation"), 0.88, 2);
        detector.evaluate_ranked_candidate(Some(&task), "2026-09-13T01:00:00Z", Some("i1"), Some("ws_portfolio"));

        // 2. Next activity at 02:30 (90 minutes / 5400s later)
        let next_task = make_ranked_candidate("Case study writing", Some("Drafting"), Some("Case study writing"), 0.88, 2);
        let r = detector.evaluate_ranked_candidate(Some(&next_task), "2026-09-13T02:30:00Z", Some("i2"), Some("ws_portfolio"));

        // Inactivity should trigger TaskEnd for the previous task
        match r {
            TaskTransitionEvaluationResult::TransitionDetected(t) => {
                assert_eq!(t.transition_type, TaskTransitionType::TaskEnd);
                assert_eq!(t.from_task.as_deref(), Some("Hero animation"));
            }
            _ => panic!("Expected TaskEnd due to inactivity"),
        }
    }
}

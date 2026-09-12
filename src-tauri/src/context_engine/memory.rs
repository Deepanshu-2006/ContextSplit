use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::context_engine::task_extractor::is_generic_task_name;
use crate::context_engine::task_ranking::{RankedTaskCandidate, TaskAmbiguityState};
use crate::context_engine::task_transitions::{
    DetectedTaskTransition, TaskSnapshot, TaskTransitionType,
};
use crate::context_engine::transitions::{ContextReference, ContextSnapshot};

// =========================================================================
// Data Models: Task Understanding Working Memory
// =========================================================================

/// Qualitative confidence band specifically calibrated for inferred tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskConfidenceBand {
    High,
    Medium,
    Low,
    Insufficient,
}

impl TaskConfidenceBand {
    pub fn from_score(score: f64) -> Self {
        if score >= 0.75 {
            TaskConfidenceBand::High
        } else if score >= 0.50 {
            TaskConfidenceBand::Medium
        } else if score >= 0.35 {
            TaskConfidenceBand::Low
        } else {
            TaskConfidenceBand::Insufficient
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            TaskConfidenceBand::High => "High",
            TaskConfidenceBand::Medium => "Medium",
            TaskConfidenceBand::Low => "Low",
            TaskConfidenceBand::Insufficient => "Insufficient",
        }
    }

    pub fn is_sufficient(&self) -> bool {
        matches!(self, TaskConfidenceBand::High | TaskConfidenceBand::Medium)
    }
}

/// Detailed state of an inferred task currently active in contextual memory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InferredTaskState {
    /// Inferred task title (e.g., "Hero animation", "CS106B Recursion Assignment", or "Unknown")
    pub title: String,
    /// Extracted action verb (e.g., Some("Designing"), Some("Debugging"))
    pub action_verb: Option<String>,
    /// Target subject/entity (e.g., Some("Hero animation"), Some("Recursion Assignment"))
    pub target_subject: Option<String>,
    /// Calibrated numeric task confidence [0.0, 1.0] (strictly decoupled from context confidence)
    pub confidence: f64,
    /// Qualitative task confidence band ("High", "Medium", "Low", "Insufficient")
    pub confidence_band: String,
    /// Human-readable evidence items supporting this task inference
    pub evidence: Vec<String>,
    /// Whether this task has sufficient confidence and unambiguous support
    pub is_confident: bool,
    /// Ambiguity state if derived from candidate ranking
    pub ambiguity_state: Option<TaskAmbiguityState>,
    /// Timestamp when this task was first observed
    pub first_seen_at: Option<String>,
    /// Timestamp of most recent activity observed for this task
    pub last_seen_at: Option<String>,
    /// Number of consecutive consistent observations
    pub streak_count: usize,
    /// Item IDs supporting this task
    pub item_ids: Vec<String>,
    /// ID of the parent context/workspace
    pub context_id: Option<String>,
}

impl InferredTaskState {
    /// Represents an unassigned or insufficiently confident task state.
    pub fn unknown() -> Self {
        Self {
            title: "Unknown".to_string(),
            action_verb: None,
            target_subject: None,
            confidence: 0.0,
            confidence_band: TaskConfidenceBand::Insufficient.as_str().to_string(),
            evidence: Vec::new(),
            is_confident: false,
            ambiguity_state: Some(TaskAmbiguityState::InsufficientEvidence),
            first_seen_at: None,
            last_seen_at: None,
            streak_count: 0,
            item_ids: Vec::new(),
            context_id: None,
        }
    }

    /// Creates a confident task state from explicit attributes.
    pub fn new(
        title: impl Into<String>,
        verb: Option<String>,
        subject: Option<String>,
        confidence: f64,
        evidence: Vec<String>,
        timestamp: &str,
        context_id: Option<String>,
    ) -> Self {
        let band = TaskConfidenceBand::from_score(confidence);
        let is_confident = band.is_sufficient() && confidence >= 0.45;
        let title_str = title.into();

        if is_confident && !is_generic_task_name(&title_str) {
            Self {
                title: title_str,
                action_verb: verb,
                target_subject: subject,
                confidence,
                confidence_band: band.as_str().to_string(),
                evidence,
                is_confident: true,
                ambiguity_state: Some(TaskAmbiguityState::ClearLeader),
                first_seen_at: Some(timestamp.to_string()),
                last_seen_at: Some(timestamp.to_string()),
                streak_count: 1,
                item_ids: Vec::new(),
                context_id,
            }
        } else {
            Self {
                title: "Unknown".to_string(),
                action_verb: None,
                target_subject: None,
                confidence,
                confidence_band: TaskConfidenceBand::Insufficient.as_str().to_string(),
                evidence: Vec::new(),
                is_confident: false,
                ambiguity_state: Some(TaskAmbiguityState::InsufficientEvidence),
                first_seen_at: Some(timestamp.to_string()),
                last_seen_at: Some(timestamp.to_string()),
                streak_count: 0,
                item_ids: Vec::new(),
                context_id,
            }
        }
    }

    /// Constructs an `InferredTaskState` from a `RankedTaskCandidate`.
    pub fn from_ranked(
        ranked: &RankedTaskCandidate,
        context_id: Option<String>,
        timestamp: &str,
    ) -> Self {
        let band = TaskConfidenceBand::from_score(ranked.confidence);
        let has_sufficient_evidence = band.is_sufficient()
            && !is_generic_task_name(&ranked.candidate.title);

        if has_sufficient_evidence {
            let evidence_summaries: Vec<String> = if !ranked.supporting_evidence.is_empty() {
                ranked
                    .supporting_evidence
                    .iter()
                    .map(|e| e.summary.clone())
                    .collect()
            } else {
                vec!["contextual activity evidence".to_string()]
            };

            let item_ids: Vec<String> = ranked
                .supporting_evidence
                .iter()
                .filter_map(|e| e.source_entity.clone())
                .collect();

            Self {
                title: ranked.candidate.title.clone(),
                action_verb: ranked.candidate.action_verb.clone(),
                target_subject: ranked.candidate.target_subject.clone(),
                confidence: ranked.confidence,
                confidence_band: band.as_str().to_string(),
                evidence: evidence_summaries,
                is_confident: true,
                ambiguity_state: Some(TaskAmbiguityState::ClearLeader),
                first_seen_at: Some(timestamp.to_string()),
                last_seen_at: Some(timestamp.to_string()),
                streak_count: 1,
                item_ids,
                context_id,
            }
        } else {
            Self {
                title: "Unknown".to_string(),
                action_verb: None,
                target_subject: None,
                confidence: ranked.confidence,
                confidence_band: TaskConfidenceBand::Insufficient.as_str().to_string(),
                evidence: Vec::new(),
                is_confident: false,
                ambiguity_state: Some(TaskAmbiguityState::InsufficientEvidence),
                first_seen_at: Some(timestamp.to_string()),
                last_seen_at: Some(timestamp.to_string()),
                streak_count: 0,
                item_ids: Vec::new(),
                context_id,
            }
        }
    }

    /// Constructs an `InferredTaskState` from a working `TaskSnapshot`.
    pub fn from_snapshot(snapshot: &TaskSnapshot, evidence: Vec<String>) -> Self {
        let band = TaskConfidenceBand::from_score(snapshot.confidence);
        let is_confident = snapshot.confidence >= 0.45 && !is_generic_task_name(&snapshot.title);

        if is_confident {
            Self {
                title: snapshot.title.clone(),
                action_verb: snapshot.action_verb.clone(),
                target_subject: snapshot.target_subject.clone(),
                confidence: snapshot.confidence,
                confidence_band: band.as_str().to_string(),
                evidence,
                is_confident: true,
                ambiguity_state: Some(TaskAmbiguityState::ClearLeader),
                first_seen_at: Some(snapshot.first_seen_at.clone()),
                last_seen_at: Some(snapshot.last_seen_at.clone()),
                streak_count: snapshot.streak_count,
                item_ids: snapshot.item_ids.clone(),
                context_id: snapshot.context_id.clone(),
            }
        } else {
            Self {
                title: "Unknown".to_string(),
                action_verb: None,
                target_subject: None,
                confidence: snapshot.confidence,
                confidence_band: TaskConfidenceBand::Insufficient.as_str().to_string(),
                evidence: Vec::new(),
                is_confident: false,
                ambiguity_state: Some(TaskAmbiguityState::InsufficientEvidence),
                first_seen_at: Some(snapshot.first_seen_at.clone()),
                last_seen_at: Some(snapshot.last_seen_at.clone()),
                streak_count: snapshot.streak_count,
                item_ids: snapshot.item_ids.clone(),
                context_id: snapshot.context_id.clone(),
            }
        }
    }

    /// Converts this active task state into a `TaskSnapshot` for history tracking.
    pub fn to_snapshot(&self) -> Option<TaskSnapshot> {
        if !self.is_confident || self.title == "Unknown" {
            return None;
        }

        let first = self
            .first_seen_at
            .clone()
            .unwrap_or_else(|| Utc::now().to_rfc3339());
        let last = self
            .last_seen_at
            .clone()
            .unwrap_or_else(|| Utc::now().to_rfc3339());

        let mut snap = TaskSnapshot::new(
            &self.title,
            self.action_verb.clone(),
            self.target_subject.clone(),
            self.confidence,
            &first,
            self.context_id.clone(),
        );
        snap.last_seen_at = last;
        snap.streak_count = self.streak_count;
        snap.item_ids = self.item_ids.clone();
        Some(snap)
    }

    /// Returns `true` if this task is marked as unknown or insufficient.
    pub fn is_unknown(&self) -> bool {
        !self.is_confident || self.title == "Unknown"
    }
}

// =========================================================================
// ContextMemory: Unified Working Memory
// =========================================================================

/// Unified working memory maintaining the active contextual state,
/// separate context identity, currently inferred task, and historical traces.
///
/// Invariants:
/// 1. Context Identity (e.g., Workspace "Portfolio") is strictly preserved and never overwritten by task names.
/// 2. Task Confidence is decoupled from Context Confidence.
/// 3. If evidence is insufficient, `current_task` cleanly defaults to `"Unknown"` with zero forced guessing.
/// 4. Task history and context history are maintained as distinct chronological sequences.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextMemory {
    /// Active Context identity (Workspace or Candidate reference)
    pub context: Option<ContextReference>,
    /// Calibrated Context confidence [0.0, 1.0]
    pub context_confidence: f64,
    /// Qualitative Context confidence band ("High", "Medium", "Low", "None")
    pub context_confidence_band: String,
    /// Currently inferred task within this context
    pub current_task: InferredTaskState,
    /// Bounded history of past inferred tasks in this context
    pub task_history: Vec<TaskSnapshot>,
    /// Bounded history of past active contexts
    pub context_history: Vec<ContextSnapshot>,
    /// Maximum history depth to maintain (default 5)
    pub max_history_depth: usize,
    /// Timestamp when current context was first observed
    pub first_seen_at: Option<String>,
    /// Timestamp of most recent activity in this context
    pub last_active_at: Option<String>,
    /// Active session ID if known
    pub session_id: Option<String>,
}

impl ContextMemory {
    pub const DEFAULT_MAX_HISTORY_DEPTH: usize = 5;

    /// Creates a new `ContextMemory` with a known context reference.
    pub fn new(
        context: Option<ContextReference>,
        context_confidence: f64,
        timestamp: &str,
    ) -> Self {
        let band = if context_confidence >= 0.75 {
            "High"
        } else if context_confidence >= 0.50 {
            "Medium"
        } else if context_confidence >= 0.25 {
            "Low"
        } else {
            "None"
        };

        Self {
            context,
            context_confidence,
            context_confidence_band: band.to_string(),
            current_task: InferredTaskState::unknown(),
            task_history: Vec::new(),
            context_history: Vec::new(),
            max_history_depth: Self::DEFAULT_MAX_HISTORY_DEPTH,
            first_seen_at: Some(timestamp.to_string()),
            last_active_at: Some(timestamp.to_string()),
            session_id: None,
        }
    }

    /// Constructs `ContextMemory` from an existing `ContextSnapshot`.
    pub fn from_snapshot(snapshot: &ContextSnapshot) -> Self {
        Self {
            context: Some(snapshot.reference.clone()),
            context_confidence: snapshot.confidence,
            context_confidence_band: snapshot.confidence_band.clone(),
            current_task: InferredTaskState::unknown(),
            task_history: Vec::new(),
            context_history: Vec::new(),
            max_history_depth: Self::DEFAULT_MAX_HISTORY_DEPTH,
            first_seen_at: Some(snapshot.first_seen_at.clone()),
            last_active_at: Some(snapshot.last_seen_at.clone()),
            session_id: snapshot.session_id.clone(),
        }
    }

    /// Fluent builder to attach an active task state.
    pub fn with_task(mut self, task: InferredTaskState) -> Self {
        self.current_task = task;
        self
    }

    /// Fluent builder to set session ID.
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Fluent builder to attach task history.
    pub fn with_task_history(mut self, history: Vec<TaskSnapshot>) -> Self {
        self.task_history = history;
        self
    }

    /// Fluent builder to attach context history.
    pub fn with_context_history(mut self, history: Vec<ContextSnapshot>) -> Self {
        self.context_history = history;
        self
    }

    // =========================================================================
    // Accessors & Queries
    // =========================================================================

    /// Returns the context reference if active.
    pub fn context_identity(&self) -> Option<&ContextReference> {
        self.context.as_ref()
    }

    /// Returns the active context / workspace name (e.g., "Portfolio", "College").
    pub fn context_name(&self) -> &str {
        self.context
            .as_ref()
            .map(|c| c.name.as_str())
            .unwrap_or("Unknown")
    }

    /// Returns the active context ID if present.
    pub fn context_id(&self) -> Option<&str> {
        self.context.as_ref().map(|c| c.id.as_str())
    }

    /// Returns the calibrated context confidence.
    pub fn context_confidence(&self) -> f64 {
        self.context_confidence
    }

    /// Returns the qualitative context confidence band.
    pub fn context_confidence_band(&self) -> &str {
        &self.context_confidence_band
    }

    /// Returns a reference to the active `InferredTaskState`.
    pub fn current_task(&self) -> &InferredTaskState {
        &self.current_task
    }

    /// Returns the title of the current task if confident, otherwise `"Unknown"`.
    pub fn current_task_title(&self) -> &str {
        if self.current_task.is_confident && self.current_task.title != "Unknown" {
            &self.current_task.title
        } else {
            "Unknown"
        }
    }

    /// Returns the task confidence score.
    pub fn task_confidence(&self) -> f64 {
        self.current_task.confidence
    }

    /// Returns the qualitative task confidence band ("High", "Medium", "Low", "Insufficient").
    pub fn task_confidence_band(&self) -> &str {
        &self.current_task.confidence_band
    }

    /// Returns supporting evidence lines for the active task.
    pub fn task_evidence(&self) -> &[String] {
        &self.current_task.evidence
    }

    /// Returns whether a confident task has been inferred for this context.
    pub fn has_confident_task(&self) -> bool {
        self.current_task.is_confident && self.current_task.title != "Unknown"
    }

    /// Returns past tasks recorded within this context.
    pub fn task_history(&self) -> &[TaskSnapshot] {
        &self.task_history
    }

    /// Returns past contexts recorded in working memory.
    pub fn context_history(&self) -> &[ContextSnapshot] {
        &self.context_history
    }

    // =========================================================================
    // State Transitions & Mutation
    // =========================================================================

    /// Updates the active task state from a ranked candidate.
    /// Preserves existing task if same task continues, archives to history on switch,
    /// or falls back to "Unknown" if evidence is insufficient.
    pub fn update_from_ranked_candidate(
        &mut self,
        ranked: &RankedTaskCandidate,
        timestamp: &str,
    ) {
        let band = TaskConfidenceBand::from_score(ranked.confidence);
        let is_sufficient = band.is_sufficient()
            && !is_generic_task_name(&ranked.candidate.title);

        if is_sufficient {
            let matches_current = self.has_confident_task()
                && (self.current_task.title.eq_ignore_ascii_case(&ranked.candidate.title)
                    || (self.current_task.target_subject.is_some()
                        && self.current_task.target_subject == ranked.candidate.target_subject));

            if matches_current {
                // Continuation of current task
                self.current_task.confidence = ranked.confidence;
                self.current_task.confidence_band =
                    TaskConfidenceBand::from_score(ranked.confidence).as_str().to_string();
                self.current_task.last_seen_at = Some(timestamp.to_string());
                self.current_task.streak_count += 1;

                // Update evidence if fresh
                if !ranked.supporting_evidence.is_empty() {
                    let mut fresh_evidence: Vec<String> = ranked
                        .supporting_evidence
                        .iter()
                        .map(|e| e.summary.clone())
                        .collect();
                    for old_ev in &self.current_task.evidence {
                        if !fresh_evidence.contains(old_ev) {
                            fresh_evidence.push(old_ev.clone());
                        }
                    }
                    fresh_evidence.truncate(6);
                    self.current_task.evidence = fresh_evidence;
                }
            } else {
                // New task switch within this context: archive previous active task
                self.archive_current_task();

                let mut new_task = InferredTaskState::from_ranked(
                    ranked,
                    self.context_id().map(|s| s.to_string()),
                    timestamp,
                );
                new_task.first_seen_at = Some(timestamp.to_string());
                new_task.last_seen_at = Some(timestamp.to_string());
                self.current_task = new_task;
            }
        } else {
            // Insufficient evidence - do NOT force a task
            if !self.has_confident_task() {
                self.current_task = InferredTaskState::unknown();
                self.current_task.confidence = ranked.confidence;
                self.current_task.last_seen_at = Some(timestamp.to_string());
            }
        }

        self.last_active_at = Some(timestamp.to_string());
    }

    /// Updates the active task from an observed `TaskSnapshot`.
    pub fn update_from_snapshot(&mut self, snapshot: &TaskSnapshot, evidence: Vec<String>) {
        if self.has_confident_task() && !snapshot.is_same_task(&self.current_task.title, self.current_task.target_subject.as_deref()) {
            self.archive_current_task();
        }
        self.current_task = InferredTaskState::from_snapshot(snapshot, evidence);
        self.last_active_at = Some(snapshot.last_seen_at.clone());
    }

    /// Updates state from a detected `DetectedTaskTransition`.
    pub fn update_from_task_transition(&mut self, transition: &DetectedTaskTransition) {
        match transition.transition_type {
            TaskTransitionType::TaskStart
            | TaskTransitionType::TaskSwitch
            | TaskTransitionType::TaskReturn => {
                if let Some(to_task_title) = &transition.to_task {
                    if self.has_confident_task() && !self.current_task.title.eq_ignore_ascii_case(to_task_title) {
                        self.archive_current_task();
                    }
                    let mut task_state = InferredTaskState::new(
                        to_task_title,
                        None,
                        None,
                        transition.confidence,
                        transition.evidence.clone(),
                        &transition.timestamp,
                        transition.context_id.clone(),
                    );
                    task_state.confidence = transition.confidence;
                    task_state.confidence_band =
                        TaskConfidenceBand::from_score(transition.confidence).as_str().to_string();
                    self.current_task = task_state;
                    self.last_active_at = Some(transition.timestamp.clone());
                }
            }
            TaskTransitionType::TaskEnd => {
                self.archive_current_task();
                self.current_task = InferredTaskState::unknown();
                self.last_active_at = Some(transition.timestamp.clone());
            }
            TaskTransitionType::AmbiguousTaskChange => {
                // Keep existing task or mark ambiguous
                if self.has_confident_task() {
                    self.current_task.confidence = (self.current_task.confidence * 0.8).max(0.1);
                    self.current_task.confidence_band =
                        TaskConfidenceBand::from_score(self.current_task.confidence).as_str().to_string();
                    self.current_task.ambiguity_state = Some(TaskAmbiguityState::HighlyAmbiguous);
                }
                self.last_active_at = Some(transition.timestamp.clone());
            }
        }
    }

    /// Switches context identity to a new context, archiving previous context and task.
    pub fn switch_context(
        &mut self,
        new_context: ContextReference,
        confidence: f64,
        timestamp: &str,
    ) {
        // Archive current context snapshot
        if let Some(ctx) = self.context.take() {
            let snap = ContextSnapshot {
                reference: ctx,
                confidence: self.context_confidence,
                confidence_band: self.context_confidence_band.clone(),
                first_seen_at: self.first_seen_at.clone().unwrap_or_else(|| timestamp.to_string()),
                last_seen_at: self.last_active_at.clone().unwrap_or_else(|| timestamp.to_string()),
                streak_count: 1,
                session_id: self.session_id.clone(),
            };
            self.push_context_history(snap);
        }

        // Archive current task
        self.archive_current_task();

        let band = if confidence >= 0.75 {
            "High"
        } else if confidence >= 0.50 {
            "Medium"
        } else if confidence >= 0.25 {
            "Low"
        } else {
            "None"
        };

        self.context = Some(new_context);
        self.context_confidence = confidence;
        self.context_confidence_band = band.to_string();
        self.current_task = InferredTaskState::unknown();
        self.first_seen_at = Some(timestamp.to_string());
        self.last_active_at = Some(timestamp.to_string());
    }

    /// Clears and archives the active task (e.g. on task completion or long pause).
    pub fn clear_current_task(&mut self, timestamp: Option<&str>) {
        self.archive_current_task();
        self.current_task = InferredTaskState::unknown();
        if let Some(ts) = timestamp {
            self.last_active_at = Some(ts.to_string());
        }
    }

    /// Touches activity timestamp.
    pub fn touch_activity(&mut self, timestamp: &str) {
        self.last_active_at = Some(timestamp.to_string());
        if self.current_task.is_confident {
            self.current_task.last_seen_at = Some(timestamp.to_string());
        }
    }

    /// Internal helper to archive current active task to `task_history`.
    fn archive_current_task(&mut self) {
        if let Some(snap) = self.current_task.to_snapshot() {
            // Don't push duplicate if matching the top of history
            if let Some(front) = self.task_history.first_mut() {
                if front.is_same_task(&snap.title, snap.target_subject.as_deref()) {
                    front.last_seen_at = snap.last_seen_at;
                    front.streak_count += snap.streak_count;
                    return;
                }
            }
            self.task_history.insert(0, snap);
            while self.task_history.len() > self.max_history_depth {
                self.task_history.pop();
            }
        }
    }

    /// Internal helper to push a context snapshot to `context_history`.
    fn push_context_history(&mut self, snapshot: ContextSnapshot) {
        if let Some(front) = self.context_history.first_mut() {
            if front.reference.id == snapshot.reference.id {
                front.last_seen_at = snapshot.last_seen_at;
                front.streak_count += snapshot.streak_count;
                return;
            }
        }
        self.context_history.insert(0, snapshot);
        while self.context_history.len() > self.max_history_depth {
            self.context_history.pop();
        }
    }

    // =========================================================================
    // Human-Readable Formatting & Serialization
    // =========================================================================

    /// Formats the context working memory into the required standardized output:
    ///
    /// ```text
    /// Context:
    /// Portfolio
    ///
    /// Current task:
    /// Hero animation
    ///
    /// Task confidence:
    /// High
    ///
    /// Task evidence:
    /// - recent Figma activity
    /// - related React items
    /// - semantic cluster
    /// - same activity segment
    /// ```
    ///
    /// Or if task confidence is insufficient:
    /// ```text
    /// Context:
    /// Portfolio
    ///
    /// Current task:
    /// Unknown
    /// ```
    pub fn format_summary(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("Context:\n{}\n\n", self.context_name()));

        if self.has_confident_task() {
            out.push_str(&format!("Current task:\n{}\n\n", self.current_task.title));
            out.push_str(&format!("Task confidence:\n{}\n", self.current_task.confidence_band));

            if !self.current_task.evidence.is_empty() {
                out.push_str("\nTask evidence:\n");
                for ev in &self.current_task.evidence {
                    out.push_str(&format!("- {}\n", ev));
                }
            }
        } else {
            out.push_str("Current task:\nUnknown");
        }

        out.trim_end().to_string()
    }

    /// Formats the summary into a JSON payload for IPC / frontend serialization.
    pub fn to_json(&self) -> Value {
        json!({
            "context": self.context,
            "context_name": self.context_name(),
            "context_confidence": self.context_confidence,
            "context_confidence_band": self.context_confidence_band,
            "current_task": {
                "title": self.current_task_title(),
                "action_verb": self.current_task.action_verb,
                "target_subject": self.current_task.target_subject,
                "confidence": self.current_task.confidence,
                "confidence_band": self.current_task.confidence_band,
                "evidence": self.current_task.evidence,
                "is_confident": self.current_task.is_confident,
                "first_seen_at": self.current_task.first_seen_at,
                "last_seen_at": self.current_task.last_seen_at,
                "streak_count": self.current_task.streak_count,
            },
            "task_history_count": self.task_history.len(),
            "context_history_count": self.context_history.len(),
            "first_seen_at": self.first_seen_at,
            "last_active_at": self.last_active_at,
            "session_id": self.session_id,
        })
    }
}

// =========================================================================
// Unit & Integration Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_engine::task_extractor::{ExtractedTaskCandidate, TaskCandidateEvidenceRef};

    #[test]
    fn test_format_summary_confident_portfolio_hero_animation() {
        let ctx = ContextReference::workspace("ws-portfolio-1", "Portfolio");
        let memory = ContextMemory::new(Some(ctx), 0.95, "2026-09-13T10:00:00Z").with_task(
            InferredTaskState::new(
                "Hero animation",
                Some("Designing".to_string()),
                Some("Hero animation".to_string()),
                0.85,
                vec![
                    "recent Figma activity".to_string(),
                    "related React items".to_string(),
                    "semantic cluster".to_string(),
                    "same activity segment".to_string(),
                ],
                "2026-09-13T10:00:00Z",
                Some("ws-portfolio-1".to_string()),
            ),
        );

        assert_eq!(memory.context_name(), "Portfolio");
        assert_eq!(memory.current_task_title(), "Hero animation");
        assert_eq!(memory.task_confidence_band(), "High");
        assert!(memory.has_confident_task());

        let expected = "\
Context:
Portfolio

Current task:
Hero animation

Task confidence:
High

Task evidence:
- recent Figma activity
- related React items
- semantic cluster
- same activity segment";

        assert_eq!(memory.format_summary(), expected);
    }

    #[test]
    fn test_format_summary_insufficient_confidence_falls_back_to_unknown() {
        let ctx = ContextReference::workspace("ws-portfolio-1", "Portfolio");
        let memory = ContextMemory::new(Some(ctx), 0.90, "2026-09-13T10:00:00Z");

        assert_eq!(memory.context_name(), "Portfolio");
        assert_eq!(memory.current_task_title(), "Unknown");
        assert!(!memory.has_confident_task());

        let expected = "\
Context:
Portfolio

Current task:
Unknown";

        assert_eq!(memory.format_summary(), expected);
    }

    #[test]
    fn test_do_not_replace_underlying_workspace_name() {
        let ctx = ContextReference::workspace("ws-college-1", "College");
        let mut memory = ContextMemory::new(Some(ctx), 0.92, "2026-09-13T09:00:00Z");

        // Attach a task with title "Operating Systems Assignment 3"
        let task = InferredTaskState::new(
            "Operating Systems Assignment 3",
            Some("Coding".to_string()),
            Some("OS Assignment 3".to_string()),
            0.88,
            vec!["virtual memory files".to_string()],
            "2026-09-13T09:00:00Z",
            Some("ws-college-1".to_string()),
        );
        memory.current_task = task;

        // Context name MUST remain "College", NOT "Operating Systems Assignment 3"
        assert_eq!(memory.context_name(), "College");
        assert_eq!(memory.current_task_title(), "Operating Systems Assignment 3");
        assert_eq!(memory.context_identity().unwrap().name, "College");
        assert_eq!(memory.context_identity().unwrap().id, "ws-college-1");
    }

    #[test]
    fn test_decoupled_confidence_metrics() {
        let ctx = ContextReference::workspace("ws-finance", "Personal Finance");
        // High context confidence (95%), but moderate/low task confidence (40%)
        let memory = ContextMemory::new(Some(ctx), 0.95, "2026-09-13T12:00:00Z").with_task(
            InferredTaskState::new(
                "Tax deduction research",
                Some("Researching".to_string()),
                Some("Tax deduction".to_string()),
                0.40, // Low task confidence (< 0.45)
                vec!["IRS pdf".to_string()],
                "2026-09-13T12:00:00Z",
                Some("ws-finance".to_string()),
            ),
        );

        // Context is 95% High
        assert_eq!(memory.context_confidence(), 0.95);
        assert_eq!(memory.context_confidence_band(), "High");

        // Task confidence was insufficient, so task is Unknown
        assert_eq!(memory.current_task_title(), "Unknown");
        assert!(!memory.has_confident_task());
    }

    #[test]
    fn test_task_history_tracking_on_switch() {
        let ctx = ContextReference::workspace("ws-portfolio", "Portfolio");
        let mut memory = ContextMemory::new(Some(ctx), 0.95, "2026-09-13T14:00:00Z");

        // 1. First Task: "Hero animation"
        let task1 = InferredTaskState::new(
            "Hero animation",
            Some("Designing".to_string()),
            Some("Hero animation".to_string()),
            0.85,
            vec!["Figma design".to_string()],
            "2026-09-13T14:00:00Z",
            Some("ws-portfolio".to_string()),
        );
        memory.current_task = task1;
        assert_eq!(memory.current_task_title(), "Hero animation");
        assert_eq!(memory.task_history().len(), 0);

        // 2. Switch Task: "Case study writing"
        let ranked2 = RankedTaskCandidate {
            candidate: ExtractedTaskCandidate {
                title: "Case study writing".to_string(),
                action_verb: Some("Writing".to_string()),
                target_subject: Some("Case study".to_string()),
                relevance_score: 0.88,
                evidence_references: Vec::new(),
                item_ids: vec!["item-2".to_string()],
                primary_evidence_types: vec![crate::context_engine::task_evidence::TaskEvidenceType::RecentItemContent],
                salient_terms: vec!["case".to_string(), "study".to_string()],
                context_id: Some("ws-portfolio".to_string()),
                metadata: serde_json::json!({}),
            },
            score: 0.86,
            confidence: 0.84,
            confidence_band: "high".to_string(),
            rank: 1,
            supporting_evidence: vec![TaskCandidateEvidenceRef {
                evidence_type: crate::context_engine::task_evidence::TaskEvidenceType::RecentItemContent,
                provenance: crate::context_engine::evidence::EvidenceProvenance::Observed,
                summary: "case study draft".to_string(),
                strength: 0.85,
                source_entity: Some("item-2".to_string()),
            }],
            contradictory_evidence: Vec::new(),
            dimension_scores: crate::context_engine::task_ranking::TaskRankingDimensionScores {
                recency_score: 0.9,
                evidence_strength_score: 0.85,
                evidence_diversity_score: 0.8,
                semantic_coherence_score: 0.85,
                activity_frequency_score: 0.8,
                session_continuity_score: 0.9,
                contextual_relevance_score: 0.9,
                stability_score: 0.8,
            },
            margin_over_next: Some(0.3),
        };

        memory.update_from_ranked_candidate(&ranked2, "2026-09-13T14:30:00Z");

        // Current task is now "Case study writing"
        assert_eq!(memory.current_task_title(), "Case study writing");

        // Previous task "Hero animation" is preserved in task_history
        assert_eq!(memory.task_history().len(), 1);
        assert_eq!(memory.task_history()[0].title, "Hero animation");

        // Context name is still "Portfolio"
        assert_eq!(memory.context_name(), "Portfolio");
    }

    #[test]
    fn test_context_switch_archives_both_context_and_task() {
        let ctx1 = ContextReference::workspace("ws-portfolio", "Portfolio");
        let mut memory = ContextMemory::new(Some(ctx1), 0.95, "2026-09-13T10:00:00Z").with_task(
            InferredTaskState::new(
                "Hero animation",
                Some("Designing".to_string()),
                Some("Hero animation".to_string()),
                0.85,
                vec!["Figma".to_string()],
                "2026-09-13T10:00:00Z",
                Some("ws-portfolio".to_string()),
            ),
        );

        // Switch to "College" workspace
        let ctx2 = ContextReference::workspace("ws-college", "College");
        memory.switch_context(ctx2, 0.90, "2026-09-13T11:00:00Z");

        // Verify context updated
        assert_eq!(memory.context_name(), "College");
        assert_eq!(memory.context_confidence(), 0.90);

        // Verify current task reset to Unknown for the new context
        assert_eq!(memory.current_task_title(), "Unknown");
        assert!(!memory.has_confident_task());

        // Verify context history recorded "Portfolio"
        assert_eq!(memory.context_history().len(), 1);
        assert_eq!(memory.context_history()[0].reference.name, "Portfolio");

        // Verify task history recorded "Hero animation"
        assert_eq!(memory.task_history().len(), 1);
        assert_eq!(memory.task_history()[0].title, "Hero animation");
    }
}

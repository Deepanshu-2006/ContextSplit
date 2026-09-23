use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =========================================================================
// Epistemic Disclaimer
// =========================================================================

pub const CONTEXT_DECISION_TRACE_DISCLAIMER: &str =
    "ContextSplit decision traces record deterministic execution paths, evidence vectors, and confidence evaluations for intelligence explainability and debugging. They contain structural references and telemetry summaries, strictly omitting raw sensitive inputs.";

// =========================================================================
// Decision Type
// =========================================================================

/// Explicit category of an intelligence decision point in ContextSplit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionType {
    /// Telemetry item assigned or inferred to a context workspace.
    ContextAssignment,
    /// Telemetry item rejected, blocked, or quarantined as noise/contamination.
    ContextRejection,
    /// Active context changed, started, or returned (hysteresis evaluation).
    ContextTransition,
    /// Task candidate extracted, ranked, or associated with a context.
    TaskAssignment,
    /// Contextual episodic memory structure assembled and created.
    MemoryCreation,
    /// Memory evaluated for importance, tiering, or decay.
    MemoryImportance,
    /// Context state snapshot created or evaluated by quality gates.
    SnapshotCreation,
    /// Workspace or session lifecycle state updated (active, paused, dormant, archived).
    LifecycleTransition,
    /// Full context reconstruction synthesized across state, workset, and tasks.
    ContextReconstruction,
    /// Resume readiness assessed for a developer returning to a context.
    ResumeReadiness,
    /// User explicit feedback applied with precedence and deterministic adaptation.
    FeedbackAdaptation,
}

impl DecisionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ContextAssignment => "context_assignment",
            Self::ContextRejection => "context_rejection",
            Self::ContextTransition => "context_transition",
            Self::TaskAssignment => "task_assignment",
            Self::MemoryCreation => "memory_creation",
            Self::MemoryImportance => "memory_importance",
            Self::SnapshotCreation => "snapshot_creation",
            Self::LifecycleTransition => "lifecycle_transition",
            Self::ContextReconstruction => "context_reconstruction",
            Self::ResumeReadiness => "resume_readiness",
            Self::FeedbackAdaptation => "feedback_adaptation",
        }
    }

    pub fn display_title(&self) -> &'static str {
        match self {
            Self::ContextAssignment => "Context Assignment",
            Self::ContextRejection => "Context Rejection",
            Self::ContextTransition => "Context Transition",
            Self::TaskAssignment => "Task Assignment",
            Self::MemoryCreation => "Memory Creation",
            Self::MemoryImportance => "Memory Importance",
            Self::SnapshotCreation => "Snapshot Creation",
            Self::LifecycleTransition => "Lifecycle Transition",
            Self::ContextReconstruction => "Context Reconstruction",
            Self::ResumeReadiness => "Resume Readiness",
            Self::FeedbackAdaptation => "Feedback Adaptation",
        }
    }

    pub fn category(&self) -> &'static str {
        match self {
            Self::ContextAssignment | Self::ContextRejection => "classification",
            Self::ContextTransition | Self::LifecycleTransition => "lifecycle",
            Self::TaskAssignment => "task",
            Self::MemoryCreation | Self::MemoryImportance => "memory",
            Self::SnapshotCreation | Self::ContextReconstruction | Self::ResumeReadiness => {
                "reconstruction"
            }
            Self::FeedbackAdaptation => "adaptation",
        }
    }
}

// =========================================================================
// Source Entity Reference
// =========================================================================

/// Structural reference to an existing entity without duplicating raw payload data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "entity_type", rename_all = "snake_case")]
pub enum SourceEntityRef {
    Item {
        id: String,
        item_type: Option<String>,
        app_name: Option<String>,
        title_hint: Option<String>,
    },
    Workspace {
        id: String,
        name: String,
    },
    Transition {
        id: String,
        transition_type: String,
    },
    Task {
        id: String,
        title: String,
    },
    Memory {
        id: String,
        memory_type: Option<String>,
    },
    Snapshot {
        id: String,
        context_id: String,
    },
    Session {
        id: String,
    },
    Feedback {
        id: String,
        action: String,
    },
    Generic {
        type_name: String,
        id: String,
        label: Option<String>,
    },
}

impl SourceEntityRef {
    pub fn entity_id(&self) -> &str {
        match self {
            Self::Item { id, .. } => id,
            Self::Workspace { id, .. } => id,
            Self::Transition { id, .. } => id,
            Self::Task { id, .. } => id,
            Self::Memory { id, .. } => id,
            Self::Snapshot { id, .. } => id,
            Self::Session { id, .. } => id,
            Self::Feedback { id, .. } => id,
            Self::Generic { id, .. } => id,
        }
    }

    pub fn type_name(&self) -> &str {
        match self {
            Self::Item { .. } => "item",
            Self::Workspace { .. } => "workspace",
            Self::Transition { .. } => "transition",
            Self::Task { .. } => "task",
            Self::Memory { .. } => "memory",
            Self::Snapshot { .. } => "snapshot",
            Self::Session { .. } => "session",
            Self::Feedback { .. } => "feedback",
            Self::Generic { type_name, .. } => type_name.as_str(),
        }
    }

    pub fn summary(&self) -> String {
        match self {
            Self::Item {
                id,
                app_name,
                title_hint,
                ..
            } => {
                let app = app_name.as_deref().unwrap_or("App");
                let title = title_hint.as_deref().unwrap_or("item");
                format!("{}: {} [{}]", app, title, id)
            }
            Self::Workspace { id, name } => format!("Workspace '{}' [{}]", name, id),
            Self::Transition {
                id,
                transition_type,
            } => format!("Transition '{}' [{}]", transition_type, id),
            Self::Task { id, title } => format!("Task '{}' [{}]", title, id),
            Self::Memory { id, memory_type } => {
                format!("Memory ({}) [{}]", memory_type.as_deref().unwrap_or("general"), id)
            }
            Self::Snapshot { id, context_id } => {
                format!("Snapshot [{}] for context [{}]", id, context_id)
            }
            Self::Session { id } => format!("Session [{}]", id),
            Self::Feedback { id, action } => format!("Feedback '{}' [{}]", action, id),
            Self::Generic {
                type_name,
                id,
                label,
            } => format!(
                "{} '{}' [{}]",
                type_name,
                label.as_deref().unwrap_or(""),
                id
            ),
        }
    }
}

// =========================================================================
// Candidate Context Trace
// =========================================================================

/// Summary of a candidate workspace evaluated during a decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateContextTrace {
    pub context_id: String,
    pub context_name: String,
    pub raw_score: f32,
    pub calibrated_confidence: f32,
    pub confidence_band: String,
    pub rank: usize,
    pub is_winner: bool,
    pub is_dormant_penalized: bool,
    pub margin_to_next: Option<f32>,
}

// =========================================================================
// Evidence Considered Trace
// =========================================================================

/// Structured summary of an evidence signal evaluated during a decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceConsideredTrace {
    pub evidence_id: Option<String>,
    pub category: String,
    pub provenance: String,
    pub strength: f64,
    pub effective_weight: f64,
    pub summary: String,
    pub is_contradictory: bool,
    pub is_correlated: bool,
}

// =========================================================================
// Evidence Strength Summary
// =========================================================================

/// Aggregate metrics describing evidence quantity, diversity, and confidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceStrengthSummary {
    pub total_evidence_count: usize,
    pub independent_groups_count: usize,
    pub diversity_score: f32,
    pub composite_score: f32,
    pub calibrated_confidence: f32,
    pub confidence_band: String,
    pub contradiction_penalty: f32,
}

impl Default for EvidenceStrengthSummary {
    fn default() -> Self {
        Self {
            total_evidence_count: 0,
            independent_groups_count: 0,
            diversity_score: 1.0,
            composite_score: 0.0,
            calibrated_confidence: 0.0,
            confidence_band: "very_low".to_string(),
            contradiction_penalty: 0.0,
        }
    }
}

// =========================================================================
// Feedback Considered Trace
// =========================================================================

/// Summary of an applicable user feedback rule or precedence record evaluated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedbackConsideredTrace {
    pub feedback_id: Option<String>,
    pub action: String,
    pub scope: String,
    pub target_ref: String,
    pub precedence_tier: u32,
    pub is_applied: bool,
    pub adaptation_delta: Option<f32>,
    pub rationale: String,
}

// =========================================================================
// Final Decision Outcome
// =========================================================================

/// Structured outcome of the decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinalDecisionOutcome {
    pub status: String,
    pub primary_context_id: Option<String>,
    pub primary_context_name: Option<String>,
    pub target_entity_id: Option<String>,
    pub decision_tier: Option<String>,
    pub action_taken: String,
    pub details: HashMap<String, serde_json::Value>,
}

impl Default for FinalDecisionOutcome {
    fn default() -> Self {
        Self {
            status: "unresolved".to_string(),
            primary_context_id: None,
            primary_context_name: None,
            target_entity_id: None,
            decision_tier: None,
            action_taken: "none".to_string(),
            details: HashMap::new(),
        }
    }
}

// =========================================================================
// Context Decision Trace (Core Model)
// =========================================================================

/// A comprehensive, developer-oriented execution trace for an intelligence decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextDecisionTrace {
    /// Unique identifier for this decision instance
    pub decision_id: String,
    /// RFC3339 UTC timestamp when the decision was executed
    pub timestamp: String,
    /// Categorical decision type
    pub decision_type: DecisionType,
    /// Structural reference to the source entity
    pub source_entity: SourceEntityRef,
    /// Evaluated workspace candidates and rankings
    pub candidate_contexts: Vec<CandidateContextTrace>,
    /// Individual evidence signals evaluated
    pub evidence_considered: Vec<EvidenceConsideredTrace>,
    /// Aggregate evidence diversity and strength metrics
    pub evidence_strength: EvidenceStrengthSummary,
    /// User feedback entries evaluated and applied
    pub feedback_considered: Vec<FeedbackConsideredTrace>,
    /// Workspace or context lifecycle state at time of evaluation
    pub lifecycle_state: Option<String>,
    /// Structured verdict and outcome
    pub final_decision: FinalDecisionOutcome,
    /// Human-readable diagnostic rationale
    pub reason: String,
    /// Execution duration in microseconds
    pub processing_duration_us: u64,
    /// Execution duration in milliseconds
    pub processing_duration_ms: f64,
    /// Epistemic disclaimer
    pub disclaimer: &'static str,
    /// Additional debugging metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

// =========================================================================
// Configuration & Filters
// =========================================================================

/// Configuration options for the ContextDecisionTracer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionTraceConfig {
    /// Whether decision tracing is enabled
    pub enabled: bool,
    /// Maximum number of traces to retain in the in-memory ring buffer
    pub max_retained_traces: usize,
    /// Whether to automatically sanitize/redact sensitive patterns in titles/URLs
    pub sanitize_sensitive_data: bool,
    /// Minimum execution duration threshold in microseconds to record trace
    pub min_duration_filter_us: Option<u64>,
    /// Specific decision types to trace (if None, all are traced)
    pub enabled_types: Option<HashSet<DecisionType>>,
}

impl Default for DecisionTraceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_retained_traces: 1000,
            sanitize_sensitive_data: true,
            min_duration_filter_us: None,
            enabled_types: None,
        }
    }
}

/// Query filter for retrieving decision traces.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DecisionTraceFilter {
    pub decision_types: Option<Vec<DecisionType>>,
    pub context_id: Option<String>,
    pub source_entity_id: Option<String>,
    pub min_confidence: Option<f32>,
    pub status: Option<String>,
    pub limit: Option<usize>,
    pub since_timestamp: Option<String>,
}

/// Aggregate statistics for the decision tracer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionTraceStats {
    pub total_recorded: usize,
    pub currently_retained: usize,
    pub by_type: HashMap<String, usize>,
    pub average_duration_us: f64,
    pub max_duration_us: u64,
    pub oldest_timestamp: Option<String>,
    pub newest_timestamp: Option<String>,
}

// =========================================================================
// In-Memory Ring Buffer & Storage
// =========================================================================

struct DecisionTraceStore {
    traces: VecDeque<ContextDecisionTrace>,
    config: DecisionTraceConfig,
    total_recorded_count: usize,
    total_duration_us: u64,
    max_duration_us: u64,
    by_type_count: HashMap<String, usize>,
}

impl DecisionTraceStore {
    fn new(config: DecisionTraceConfig) -> Self {
        Self {
            traces: VecDeque::with_capacity(config.max_retained_traces.min(2048)),
            config,
            total_recorded_count: 0,
            total_duration_us: 0,
            max_duration_us: 0,
            by_type_count: HashMap::new(),
        }
    }

    fn should_record(&self, trace: &ContextDecisionTrace) -> bool {
        if !self.config.enabled {
            return false;
        }

        if let Some(min_us) = self.config.min_duration_filter_us {
            if trace.processing_duration_us < min_us {
                return false;
            }
        }

        if let Some(ref enabled_types) = self.config.enabled_types {
            if !enabled_types.contains(&trace.decision_type) {
                return false;
            }
        }

        true
    }

    fn record(&mut self, mut trace: ContextDecisionTrace) {
        if !self.should_record(&trace) {
            return;
        }

        if self.config.sanitize_sensitive_data {
            sanitize_trace(&mut trace);
        }

        self.total_recorded_count += 1;
        self.total_duration_us += trace.processing_duration_us;
        if trace.processing_duration_us > self.max_duration_us {
            self.max_duration_us = trace.processing_duration_us;
        }

        let type_key = trace.decision_type.as_str().to_string();
        *self.by_type_count.entry(type_key).or_insert(0) += 1;

        if self.traces.len() >= self.config.max_retained_traces {
            self.traces.pop_front();
        }

        self.traces.push_back(trace);
    }

    fn get_recent(&self, limit: usize) -> Vec<ContextDecisionTrace> {
        let n = limit.min(self.traces.len());
        self.traces.iter().rev().take(n).cloned().collect()
    }

    fn query(&self, filter: &DecisionTraceFilter) -> Vec<ContextDecisionTrace> {
        let limit = filter.limit.unwrap_or(50);
        let mut matches = Vec::new();

        for trace in self.traces.iter().rev() {
            if let Some(ref types) = filter.decision_types {
                if !types.contains(&trace.decision_type) {
                    continue;
                }
            }

            if let Some(ref ctx_id) = filter.context_id {
                let matches_primary = trace
                    .final_decision
                    .primary_context_id
                    .as_deref()
                    == Some(ctx_id.as_str());
                let matches_candidate = trace
                    .candidate_contexts
                    .iter()
                    .any(|c| &c.context_id == ctx_id);
                if !matches_primary && !matches_candidate {
                    continue;
                }
            }

            if let Some(ref src_id) = filter.source_entity_id {
                if trace.source_entity.entity_id() != src_id {
                    continue;
                }
            }

            if let Some(min_conf) = filter.min_confidence {
                if trace.evidence_strength.calibrated_confidence < min_conf {
                    continue;
                }
            }

            if let Some(ref status) = filter.status {
                if &trace.final_decision.status != status {
                    continue;
                }
            }

            if let Some(ref since) = filter.since_timestamp {
                if trace.timestamp < *since {
                    continue;
                }
            }

            matches.push(trace.clone());
            if matches.len() >= limit {
                break;
            }
        }

        matches
    }

    fn get_by_id(&self, id: &str) -> Option<ContextDecisionTrace> {
        self.traces.iter().find(|t| t.decision_id == id).cloned()
    }

    fn clear(&mut self) {
        self.traces.clear();
    }

    fn stats(&self) -> DecisionTraceStats {
        let count = self.traces.len();
        let avg_duration = if self.total_recorded_count > 0 {
            self.total_duration_us as f64 / self.total_recorded_count as f64
        } else {
            0.0
        };

        let oldest = self.traces.front().map(|t| t.timestamp.clone());
        let newest = self.traces.back().map(|t| t.timestamp.clone());

        DecisionTraceStats {
            total_recorded: self.total_recorded_count,
            currently_retained: count,
            by_type: self.by_type_count.clone(),
            average_duration_us: avg_duration,
            max_duration_us: self.max_duration_us,
            oldest_timestamp: oldest,
            newest_timestamp: newest,
        }
    }
}

// =========================================================================
// Global Singleton Accessor
// =========================================================================

static GLOBAL_DECISION_TRACER: OnceLock<RwLock<DecisionTraceStore>> = OnceLock::new();

fn get_tracer_store() -> &'static RwLock<DecisionTraceStore> {
    GLOBAL_DECISION_TRACER.get_or_init(|| RwLock::new(DecisionTraceStore::new(DecisionTraceConfig::default())))
}

/// Primary service for recording and inspecting contextual decision traces.
pub struct ContextDecisionTracer;

impl ContextDecisionTracer {
    /// Record a completed decision trace in the non-blocking global store.
    pub fn record(trace: ContextDecisionTrace) {
        if let Ok(mut store) = get_tracer_store().write() {
            store.record(trace);
        }
    }

    /// Retrieve the most recent decision traces (ordered from newest to oldest).
    pub fn get_recent(limit: usize) -> Vec<ContextDecisionTrace> {
        if let Ok(store) = get_tracer_store().read() {
            store.get_recent(limit)
        } else {
            Vec::new()
        }
    }

    /// Query decision traces with flexible multi-criteria filtering.
    pub fn query(filter: &DecisionTraceFilter) -> Vec<ContextDecisionTrace> {
        if let Ok(store) = get_tracer_store().read() {
            store.query(filter)
        } else {
            Vec::new()
        }
    }

    /// Retrieve a single decision trace by its unique ID.
    pub fn get_by_id(id: &str) -> Option<ContextDecisionTrace> {
        if let Ok(store) = get_tracer_store().read() {
            store.get_by_id(id)
        } else {
            None
        }
    }

    /// Retrieve traces related to a specific workspace context.
    pub fn get_by_context(context_id: &str, limit: usize) -> Vec<ContextDecisionTrace> {
        Self::query(&DecisionTraceFilter {
            context_id: Some(context_id.to_string()),
            limit: Some(limit),
            ..Default::default()
        })
    }

    /// Retrieve traces of a specific decision type.
    pub fn get_by_type(decision_type: DecisionType, limit: usize) -> Vec<ContextDecisionTrace> {
        Self::query(&DecisionTraceFilter {
            decision_types: Some(vec![decision_type]),
            limit: Some(limit),
            ..Default::default()
        })
    }

    /// Clear all currently retained decision traces.
    pub fn clear() {
        if let Ok(mut store) = get_tracer_store().write() {
            store.clear();
        }
    }

    /// Retrieve operational metrics and statistics about recorded decision traces.
    pub fn stats() -> DecisionTraceStats {
        if let Ok(store) = get_tracer_store().read() {
            store.stats()
        } else {
            DecisionTraceStats {
                total_recorded: 0,
                currently_retained: 0,
                by_type: HashMap::new(),
                average_duration_us: 0.0,
                max_duration_us: 0,
                oldest_timestamp: None,
                newest_timestamp: None,
            }
        }
    }

    /// Update tracer configuration at runtime.
    pub fn update_config(config: DecisionTraceConfig) {
        if let Ok(mut store) = get_tracer_store().write() {
            store.config = config;
        }
    }

    /// Get current configuration.
    pub fn get_config() -> DecisionTraceConfig {
        if let Ok(store) = get_tracer_store().read() {
            store.config.clone()
        } else {
            DecisionTraceConfig::default()
        }
    }
}

// =========================================================================
// Fluent Builder Pattern
// =========================================================================

/// Ergonomic builder for constructing and timing `ContextDecisionTrace` instances.
pub struct ContextDecisionTraceBuilder {
    decision_id: String,
    timestamp: String,
    decision_type: DecisionType,
    source_entity: SourceEntityRef,
    candidate_contexts: Vec<CandidateContextTrace>,
    evidence_considered: Vec<EvidenceConsideredTrace>,
    evidence_strength: EvidenceStrengthSummary,
    feedback_considered: Vec<FeedbackConsideredTrace>,
    lifecycle_state: Option<String>,
    final_decision: FinalDecisionOutcome,
    reason: String,
    start_instant: Instant,
    metadata: HashMap<String, serde_json::Value>,
}

impl ContextDecisionTraceBuilder {
    pub fn new(decision_type: DecisionType, source_entity: SourceEntityRef) -> Self {
        Self {
            decision_id: Uuid::new_v4().to_string(),
            timestamp: Utc::now().to_rfc3339(),
            decision_type,
            source_entity,
            candidate_contexts: Vec::new(),
            evidence_considered: Vec::new(),
            evidence_strength: EvidenceStrengthSummary::default(),
            feedback_considered: Vec::new(),
            lifecycle_state: None,
            final_decision: FinalDecisionOutcome::default(),
            reason: String::new(),
            start_instant: Instant::now(),
            metadata: HashMap::new(),
        }
    }

    // ---------------------------------------------------------------------
    // Specialized Constructors for the 10 Major Decision Points
    // ---------------------------------------------------------------------

    pub fn new_context_assignment(
        item_id: &str,
        app_name: Option<&str>,
        title_hint: Option<&str>,
    ) -> Self {
        Self::new(
            DecisionType::ContextAssignment,
            SourceEntityRef::Item {
                id: item_id.to_string(),
                item_type: Some("telemetry_item".to_string()),
                app_name: app_name.map(|s| s.to_string()),
                title_hint: title_hint.map(|s| s.to_string()),
            },
        )
    }

    pub fn new_context_rejection(
        item_id: &str,
        app_name: Option<&str>,
        title_hint: Option<&str>,
    ) -> Self {
        Self::new(
            DecisionType::ContextRejection,
            SourceEntityRef::Item {
                id: item_id.to_string(),
                item_type: Some("telemetry_item".to_string()),
                app_name: app_name.map(|s| s.to_string()),
                title_hint: title_hint.map(|s| s.to_string()),
            },
        )
    }

    pub fn new_context_transition(
        transition_id: &str,
        transition_type: &str,
    ) -> Self {
        Self::new(
            DecisionType::ContextTransition,
            SourceEntityRef::Transition {
                id: transition_id.to_string(),
                transition_type: transition_type.to_string(),
            },
        )
    }

    pub fn new_task_assignment(
        task_id: &str,
        task_title: &str,
    ) -> Self {
        Self::new(
            DecisionType::TaskAssignment,
            SourceEntityRef::Task {
                id: task_id.to_string(),
                title: task_title.to_string(),
            },
        )
    }

    pub fn new_memory_creation(
        memory_id: &str,
        memory_type: Option<&str>,
    ) -> Self {
        Self::new(
            DecisionType::MemoryCreation,
            SourceEntityRef::Memory {
                id: memory_id.to_string(),
                memory_type: memory_type.map(|s| s.to_string()),
            },
        )
    }

    pub fn new_memory_importance(
        memory_id: &str,
        memory_type: Option<&str>,
    ) -> Self {
        Self::new(
            DecisionType::MemoryImportance,
            SourceEntityRef::Memory {
                id: memory_id.to_string(),
                memory_type: memory_type.map(|s| s.to_string()),
            },
        )
    }

    pub fn new_snapshot_creation(
        snapshot_id: &str,
        context_id: &str,
    ) -> Self {
        Self::new(
            DecisionType::SnapshotCreation,
            SourceEntityRef::Snapshot {
                id: snapshot_id.to_string(),
                context_id: context_id.to_string(),
            },
        )
    }

    pub fn new_lifecycle_transition(
        workspace_id: &str,
        workspace_name: &str,
    ) -> Self {
        Self::new(
            DecisionType::LifecycleTransition,
            SourceEntityRef::Workspace {
                id: workspace_id.to_string(),
                name: workspace_name.to_string(),
            },
        )
    }

    pub fn new_context_reconstruction(
        workspace_id: &str,
        workspace_name: &str,
    ) -> Self {
        Self::new(
            DecisionType::ContextReconstruction,
            SourceEntityRef::Workspace {
                id: workspace_id.to_string(),
                name: workspace_name.to_string(),
            },
        )
    }

    pub fn new_resume_readiness(
        workspace_id: &str,
        workspace_name: &str,
    ) -> Self {
        Self::new(
            DecisionType::ResumeReadiness,
            SourceEntityRef::Workspace {
                id: workspace_id.to_string(),
                name: workspace_name.to_string(),
            },
        )
    }

    pub fn new_feedback_adaptation(
        feedback_id: &str,
        action: &str,
    ) -> Self {
        Self::new(
            DecisionType::FeedbackAdaptation,
            SourceEntityRef::Feedback {
                id: feedback_id.to_string(),
                action: action.to_string(),
            },
        )
    }

    // ---------------------------------------------------------------------
    // Builder Chaining Methods
    // ---------------------------------------------------------------------

    pub fn with_decision_id(mut self, id: impl Into<String>) -> Self {
        self.decision_id = id.into();
        self
    }

    pub fn with_candidate(mut self, candidate: CandidateContextTrace) -> Self {
        self.candidate_contexts.push(candidate);
        self
    }

    pub fn with_candidates(mut self, candidates: Vec<CandidateContextTrace>) -> Self {
        self.candidate_contexts = candidates;
        self
    }

    pub fn with_evidence(mut self, evidence: EvidenceConsideredTrace) -> Self {
        self.evidence_considered.push(evidence);
        self
    }

    pub fn with_evidence_list(mut self, evidence_list: Vec<EvidenceConsideredTrace>) -> Self {
        self.evidence_considered = evidence_list;
        self
    }

    pub fn with_evidence_strength(mut self, strength: EvidenceStrengthSummary) -> Self {
        self.evidence_strength = strength;
        self
    }

    pub fn with_feedback(mut self, feedback: FeedbackConsideredTrace) -> Self {
        self.feedback_considered.push(feedback);
        self
    }

    pub fn with_feedback_list(mut self, feedback_list: Vec<FeedbackConsideredTrace>) -> Self {
        self.feedback_considered = feedback_list;
        self
    }

    pub fn with_lifecycle_state(mut self, state: impl Into<String>) -> Self {
        self.lifecycle_state = Some(state.into());
        self
    }

    pub fn with_final_decision(mut self, decision: FinalDecisionOutcome) -> Self {
        self.final_decision = decision;
        self
    }

    pub fn with_outcome(
        mut self,
        status: &str,
        context_id: Option<&str>,
        context_name: Option<&str>,
        tier: Option<&str>,
        action_taken: &str,
    ) -> Self {
        self.final_decision = FinalDecisionOutcome {
            status: status.to_string(),
            primary_context_id: context_id.map(|s| s.to_string()),
            primary_context_name: context_name.map(|s| s.to_string()),
            target_entity_id: None,
            decision_tier: tier.map(|s| s.to_string()),
            action_taken: action_taken.to_string(),
            details: HashMap::new(),
        };
        self
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = reason.into();
        self
    }

    pub fn with_metadata(mut self, key: &str, value: serde_json::Value) -> Self {
        self.metadata.insert(key.to_string(), value);
        self
    }

    /// Build the `ContextDecisionTrace`, automatically calculating elapsed duration.
    pub fn build(self) -> ContextDecisionTrace {
        let elapsed = self.start_instant.elapsed();
        let duration_us = elapsed.as_micros() as u64;
        let duration_ms = elapsed.as_secs_f64() * 1000.0;

        ContextDecisionTrace {
            decision_id: self.decision_id,
            timestamp: self.timestamp,
            decision_type: self.decision_type,
            source_entity: self.source_entity,
            candidate_contexts: self.candidate_contexts,
            evidence_considered: self.evidence_considered,
            evidence_strength: self.evidence_strength,
            feedback_considered: self.feedback_considered,
            lifecycle_state: self.lifecycle_state,
            final_decision: self.final_decision,
            reason: self.reason,
            processing_duration_us: duration_us,
            processing_duration_ms: duration_ms,
            disclaimer: CONTEXT_DECISION_TRACE_DISCLAIMER,
            metadata: self.metadata,
        }
    }

    /// Build and immediately record into the global `ContextDecisionTracer`.
    pub fn record(self) -> ContextDecisionTrace {
        let trace = self.build();
        ContextDecisionTracer::record(trace.clone());
        trace
    }
}

// =========================================================================
// Sensitive Data Redaction & Sanitization
// =========================================================================

/// Redacts sensitive auth tokens, passwords, and secrets from strings in traces.
pub fn sanitize_string(s: &str) -> String {
    let mut clean = s.to_string();

    // Sensitive keys sorted longest to shortest to prevent partial substring collisions
    let sensitive_keys = [
        "access_token=",
        "password=",
        "api_key=",
        "bearer=",
        "secret=",
        "token=",
        "auth=",
        "key=",
    ];

    for key in sensitive_keys {
        loop {
            let lower = clean.to_lowercase();
            if let Some(idx) = lower.find(key) {
                let val_start = idx + key.len();
                let val_end = clean[val_start..]
                    .find(|c: char| c == '&' || c == ' ' || c == '\n' || c == '\t' || c == '"' || c == '\'')
                    .map(|i| val_start + i)
                    .unwrap_or(clean.len());

                // If this occurrence is already [REDACTED], avoid infinite loop
                if val_start < clean.len() && clean[val_start..].starts_with("[REDACTED]") {
                    break;
                }

                let redacted = format!("{}[REDACTED]{}", &clean[..val_start], &clean[val_end..]);
                clean = redacted;
            } else {
                break;
            }
        }
    }

    // Limit maximum length to prevent oversized memory retention
    if clean.len() > 300 {
        format!("{}... [truncated]", &clean[..290])
    } else {
        clean
    }
}

pub fn sanitize_trace(trace: &mut ContextDecisionTrace) {
    trace.reason = sanitize_string(&trace.reason);

    for ev in &mut trace.evidence_considered {
        ev.summary = sanitize_string(&ev.summary);
    }

    if let SourceEntityRef::Item {
        ref mut title_hint, ..
    } = trace.source_entity
    {
        if let Some(ref mut title) = title_hint {
            *title = sanitize_string(title);
        }
    }
}

// =========================================================================
// Unit Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decision_type_metadata() {
        assert_eq!(DecisionType::ContextAssignment.as_str(), "context_assignment");
        assert_eq!(DecisionType::ContextRejection.category(), "classification");
        assert_eq!(DecisionType::ContextTransition.category(), "lifecycle");
        assert_eq!(DecisionType::TaskAssignment.category(), "task");
        assert_eq!(DecisionType::MemoryImportance.category(), "memory");
        assert_eq!(DecisionType::SnapshotCreation.category(), "reconstruction");
        assert_eq!(DecisionType::ResumeReadiness.display_title(), "Resume Readiness");
    }

    #[test]
    fn test_source_entity_ref_summaries() {
        let item_ref = SourceEntityRef::Item {
            id: "item-123".to_string(),
            item_type: Some("browser".to_string()),
            app_name: Some("Google Chrome".to_string()),
            title_hint: Some("Pull Request #42".to_string()),
        };
        assert_eq!(item_ref.entity_id(), "item-123");
        assert_eq!(item_ref.type_name(), "item");
        assert!(item_ref.summary().contains("Google Chrome"));
        assert!(item_ref.summary().contains("Pull Request #42"));

        let ws_ref = SourceEntityRef::Workspace {
            id: "ws-999".to_string(),
            name: "ContextSplit".to_string(),
        };
        assert_eq!(ws_ref.entity_id(), "ws-999");
        assert_eq!(ws_ref.type_name(), "workspace");
        assert!(ws_ref.summary().contains("ContextSplit"));
    }

    #[test]
    fn test_builder_and_store_all_10_decision_types() {
        let mut store = DecisionTraceStore::new(DecisionTraceConfig::default());

        // 1. Context Assignment
        let trace1 = ContextDecisionTraceBuilder::new_context_assignment("item-1", Some("VSCode"), Some("main.rs"))
            .with_outcome("assigned", Some("ctx-1"), Some("Project A"), Some("confirmed"), "Assigned file to Project A")
            .with_reason("Window path matches project directory")
            .with_evidence_strength(EvidenceStrengthSummary {
                total_evidence_count: 3,
                independent_groups_count: 2,
                diversity_score: 1.0,
                composite_score: 0.88,
                calibrated_confidence: 0.92,
                confidence_band: "very_high".to_string(),
                contradiction_penalty: 0.0,
            })
            .build();
        assert_eq!(trace1.decision_type, DecisionType::ContextAssignment);
        assert_eq!(trace1.final_decision.status, "assigned");
        store.record(trace1);

        // 2. Context Rejection
        let trace2 = ContextDecisionTraceBuilder::new_context_rejection("item-2", Some("Finder"), Some("Trash"))
            .with_outcome("rejected", None, None, None, "Quarantined system noise")
            .with_reason("Ephemeral glance and utility noise filter triggered")
            .build();
        assert_eq!(trace2.decision_type, DecisionType::ContextRejection);
        store.record(trace2);

        // 3. Context Transition
        let trace3 = ContextDecisionTraceBuilder::new_context_transition("trans-1", "ContextSwitch")
            .with_outcome("transitioned", Some("ctx-2"), Some("Project B"), None, "Switched from Project A to Project B")
            .with_reason("Observed 3 consecutive events in Project B")
            .build();
        assert_eq!(trace3.decision_type, DecisionType::ContextTransition);
        store.record(trace3);

        // 4. Task Assignment
        let trace4 = ContextDecisionTraceBuilder::new_task_assignment("task-1", "Implement Auth")
            .with_outcome("detected", Some("ctx-1"), Some("Project A"), Some("in_progress"), "Assigned task candidate")
            .with_reason("Git branch feature/auth-flow matches title tokens")
            .build();
        assert_eq!(trace4.decision_type, DecisionType::TaskAssignment);
        store.record(trace4);

        // 5. Memory Creation
        let trace5 = ContextDecisionTraceBuilder::new_memory_creation("mem-1", Some("episodic"))
            .with_outcome("created", Some("ctx-1"), Some("Project A"), None, "Created episodic memory cluster")
            .with_reason("Assembled 15 items across active focus segment")
            .build();
        assert_eq!(trace5.decision_type, DecisionType::MemoryCreation);
        store.record(trace5);

        // 6. Memory Importance
        let trace6 = ContextDecisionTraceBuilder::new_memory_importance("mem-1", Some("episodic"))
            .with_outcome("scored", Some("ctx-1"), Some("Project A"), Some("Critical"), "Evaluated importance tier")
            .with_reason("Dwell time exceeds 45 minutes across 3 sessions")
            .build();
        assert_eq!(trace6.decision_type, DecisionType::MemoryImportance);
        store.record(trace6);

        // 7. Snapshot Creation
        let trace7 = ContextDecisionTraceBuilder::new_snapshot_creation("snap-1", "ctx-1")
            .with_outcome("saved", Some("ctx-1"), Some("Project A"), Some("Excellent"), "Captured context state snapshot")
            .with_reason("Triggered by context transition after 20 minutes focus")
            .build();
        assert_eq!(trace7.decision_type, DecisionType::SnapshotCreation);
        store.record(trace7);

        // 8. Lifecycle Transition
        let trace8 = ContextDecisionTraceBuilder::new_lifecycle_transition("ctx-1", "Project A")
            .with_lifecycle_state("paused")
            .with_outcome("updated", Some("ctx-1"), Some("Project A"), None, "Moved lifecycle to paused")
            .with_reason("No telemetry observed for 30 minutes")
            .build();
        assert_eq!(trace8.decision_type, DecisionType::LifecycleTransition);
        store.record(trace8);

        // 9. Context Reconstruction
        let trace9 = ContextDecisionTraceBuilder::new_context_reconstruction("ctx-1", "Project A")
            .with_outcome("reconstructed", Some("ctx-1"), Some("Project A"), Some("Complete"), "Synthesized 9 context sections")
            .with_reason("Loaded fresh state, workset, and active tasks")
            .build();
        assert_eq!(trace9.decision_type, DecisionType::ContextReconstruction);
        store.record(trace9);

        // 10. Resume Readiness
        let trace10 = ContextDecisionTraceBuilder::new_resume_readiness("ctx-1", "Project A")
            .with_outcome("assessed", Some("ctx-1"), Some("Project A"), Some("Ready"), "Assessed resume readiness")
            .with_reason("High workset density and clear active task")
            .build();
        assert_eq!(trace10.decision_type, DecisionType::ResumeReadiness);
        store.record(trace10);

        // Verify stats
        let stats = store.stats();
        assert_eq!(stats.currently_retained, 10);
        assert_eq!(stats.total_recorded, 10);
        assert_eq!(stats.by_type.get("context_assignment"), Some(&1));
        assert_eq!(stats.by_type.get("resume_readiness"), Some(&1));
    }

    #[test]
    fn test_query_filtering_isolated() {
        let mut store = DecisionTraceStore::new(DecisionTraceConfig::default());

        let trace1 = ContextDecisionTraceBuilder::new_context_assignment("item-10", Some("AppA"), None)
            .with_outcome("assigned", Some("ctx-alpha"), Some("Alpha"), None, "Assigned")
            .build();
        store.record(trace1);

        let trace2 = ContextDecisionTraceBuilder::new_context_assignment("item-20", Some("AppB"), None)
            .with_outcome("assigned", Some("ctx-beta"), Some("Beta"), None, "Assigned")
            .build();
        store.record(trace2);

        let trace3 = ContextDecisionTraceBuilder::new_context_rejection("item-30", Some("Noise"), None)
            .with_outcome("rejected", None, None, None, "Rejected")
            .build();
        store.record(trace3);

        // Query by context_id
        let alpha_traces = store.query(&DecisionTraceFilter {
            context_id: Some("ctx-alpha".to_string()),
            limit: Some(10),
            ..Default::default()
        });
        assert_eq!(alpha_traces.len(), 1);
        assert_eq!(alpha_traces[0].source_entity.entity_id(), "item-10");

        // Query by decision type
        let rejection_traces = store.query(&DecisionTraceFilter {
            decision_types: Some(vec![DecisionType::ContextRejection]),
            limit: Some(10),
            ..Default::default()
        });
        assert_eq!(rejection_traces.len(), 1);
        assert_eq!(rejection_traces[0].source_entity.entity_id(), "item-30");

        // Query by multi-criteria filter
        let filter = DecisionTraceFilter {
            decision_types: Some(vec![DecisionType::ContextAssignment]),
            context_id: Some("ctx-beta".to_string()),
            limit: Some(5),
            ..Default::default()
        };
        let beta_assignment = store.query(&filter);
        assert_eq!(beta_assignment.len(), 1);
        assert_eq!(beta_assignment[0].source_entity.entity_id(), "item-20");
    }

    #[test]
    fn test_sensitive_data_sanitization_unit() {
        let mut trace = ContextDecisionTraceBuilder::new_context_assignment(
            "item-auth",
            Some("Browser"),
            Some("Dashboard ?token=secret_12345&tab=home"),
        )
        .with_reason("Navigated to https://example.com/api?access_token=super_secret_token_99999&auth=true")
        .with_evidence(EvidenceConsideredTrace {
            evidence_id: Some("ev-1".to_string()),
            category: "BrowserUrl".to_string(),
            provenance: "browser".to_string(),
            strength: 0.9,
            effective_weight: 0.8,
            summary: "URL contains key=my_api_key_12345 in query".to_string(),
            is_contradictory: false,
            is_correlated: false,
        })
        .build();

        sanitize_trace(&mut trace);

        // Verify tokens are redacted
        assert!(!trace.reason.contains("super_secret_token_99999"));
        assert!(trace.reason.contains("[REDACTED]"));

        if let SourceEntityRef::Item { title_hint, .. } = &trace.source_entity {
            let title = title_hint.as_ref().unwrap();
            assert!(!title.contains("secret_12345"));
            assert!(title.contains("[REDACTED]"));
        } else {
            panic!("Expected Item source entity");
        }

        assert!(!trace.evidence_considered[0].summary.contains("my_api_key_12345"));
        assert!(trace.evidence_considered[0].summary.contains("[REDACTED]"));
    }

    #[test]
    fn test_ring_buffer_bounded_capacity_isolated() {
        let mut cfg = DecisionTraceConfig::default();
        cfg.max_retained_traces = 5;
        let mut store = DecisionTraceStore::new(cfg);

        for i in 0..10 {
            let item_id = format!("item-{}", i);
            let trace = ContextDecisionTraceBuilder::new_context_assignment(&item_id, None, None)
                .with_outcome("assigned", Some("ctx-1"), None, None, "Assigned")
                .build();
            store.record(trace);
        }

        let stats = store.stats();
        assert_eq!(stats.currently_retained, 5);
        assert_eq!(stats.total_recorded, 10);

        let recent = store.get_recent(10);
        assert_eq!(recent.len(), 5);
        // Newest should be item-9
        assert_eq!(recent[0].source_entity.entity_id(), "item-9");
        // Oldest retained should be item-5
        assert_eq!(recent[4].source_entity.entity_id(), "item-5");
    }
}


use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::context_engine::task_extractor::is_generic_task_name;
use crate::context_engine::task_ranking::{RankedTaskCandidate, TaskAmbiguityState};
use crate::context_engine::task_transitions::{
    DetectedTaskTransition, TaskSnapshot, TaskTransitionType,
};
use crate::context_engine::transitions::{
    ContextReference, ContextSnapshot,
};

// =========================================================================
// Helpers for Domain and App Normalization
// =========================================================================

fn extract_domain_from_url(url: &str) -> Option<String> {
    let s = url.trim();
    let without_proto = if let Some(stripped) = s.strip_prefix("https://") {
        stripped
    } else if let Some(stripped) = s.strip_prefix("http://") {
        stripped
    } else {
        return None;
    };
    let host = without_proto.split('/').next()?.split(':').next()?;
    let clean = host.strip_prefix("www.").unwrap_or(host).trim();
    if clean.is_empty() || clean == "localhost" || clean == "127.0.0.1" {
        None
    } else {
        Some(clean.to_lowercase())
    }
}

fn normalize_app_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("unknown") {
        None
    } else {
        Some(trimmed.to_string())
    }
}

// =========================================================================
// Epistemic Source: Information Provenance
// =========================================================================

/// Clearly distinguishes the epistemic source of truth for context memory items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InformationProvenance {
    /// Things directly supported by stored activity (e.g. captured items, window events, raw signals).
    Observed,
    /// Things inferred by Context Engine / Task Engine (e.g. inferred task, confidence scores, detected candidate clusters).
    Inferred,
    /// Things explicitly confirmed by the user (e.g. confirmed workspace membership, accepted candidate, manual assignment).
    Confirmed,
}

impl InformationProvenance {
    pub fn as_str(&self) -> &'static str {
        match self {
            InformationProvenance::Observed => "observed",
            InformationProvenance::Inferred => "inferred",
            InformationProvenance::Confirmed => "confirmed",
        }
    }
}

// =========================================================================
// Lightweight Memory References
// =========================================================================

/// Lightweight reference to a captured item within contextual memory.
/// Avoids duplicating entire item bodies, payloads, or vector embeddings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryItemReference {
    /// Unique item identifier
    pub id: String,
    /// Human-friendly title or snippet
    pub title: Option<String>,
    /// Item type (e.g., "url", "text", "file", "image")
    pub item_type: String,
    /// Timestamp when the item was captured or added
    pub timestamp: String,
    /// Epistemic source (Observed, Confirmed, or Inferred)
    pub provenance: InformationProvenance,
    /// Associated application name if available
    pub app_name: Option<String>,
    /// Associated domain if available
    pub domain: Option<String>,
}

/// Lightweight reference to an inferred or confirmed task within contextual memory.
/// Uses IDs and concise summaries rather than full candidate evidence graphs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryTaskReference {
    /// Task candidate ID if persisted in task_candidates table
    pub id: Option<String>,
    /// Task title (e.g., "Hero animation", "Recursion Assignment")
    pub title: String,
    /// Extracted action verb (e.g., "Designing", "Coding")
    pub action_verb: Option<String>,
    /// Target subject (e.g., "Hero animation", "Assignment")
    pub target_subject: Option<String>,
    /// Decoupled task confidence [0.0, 1.0]
    pub confidence: f64,
    /// Qualitative confidence band ("High", "Medium", "Low", "Insufficient")
    pub confidence_band: String,
    /// Epistemic source (Inferred or Confirmed)
    pub provenance: InformationProvenance,
    /// Timestamp when this task was first observed
    pub first_seen_at: Option<String>,
    /// Timestamp of most recent activity observed for this task
    pub last_active_at: Option<String>,
    /// Number of consecutive consistent observations
    pub streak_count: usize,
    /// Status if tracked ("detected", "active", "completed", "confirmed")
    pub status: Option<String>,
}

/// Lightweight reference to a context transition within contextual memory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryTransitionReference {
    /// Transition ID if persisted in context_transitions table
    pub id: Option<String>,
    /// Transition type (e.g., "context_switch", "context_return", "context_start", "context_end")
    pub transition_type: String,
    /// Previous context ID if applicable
    pub from_context_id: Option<String>,
    /// Previous context name if known
    pub from_context_name: Option<String>,
    /// Target context ID if applicable
    pub to_context_id: Option<String>,
    /// Target context name if known
    pub to_context_name: Option<String>,
    /// Transition confidence [0.0, 1.0]
    pub confidence: f64,
    /// Timestamp when transition was recorded
    pub detected_at: String,
    /// Epistemic source
    pub provenance: InformationProvenance,
}

/// Chronological milestone entry in the contextual activity sequence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryActivitySequenceEntry {
    /// Entry category: "item_captured", "context_transition", "task_transition", "session_burst", "user_confirmation"
    pub entry_type: String,
    /// Concise human-readable description of the activity milestone
    pub description: String,
    /// Timestamp of the event
    pub timestamp: String,
    /// Epistemic source
    pub provenance: InformationProvenance,
    /// Relevant entity ID (item_id, transition_id, task_id)
    pub reference_id: Option<String>,
}

/// Activity state of the context within memory reconstruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextMemoryActivityState {
    /// Active working session within the last hour.
    Active,
    /// Recently active within the past 24 hours.
    RecentlyActive,
    /// Dormant context with no recent activity or explicitly marked dormant.
    Dormant,
    /// Brand new context or insufficient data to reconstruct working activity.
    InsufficientEvidence,
}

impl ContextMemoryActivityState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::RecentlyActive => "recently_active",
            Self::Dormant => "dormant",
            Self::InsufficientEvidence => "insufficient_evidence",
        }
    }
}

/// Structured, human-readable explanation grounding a contextual memory statement.
/// Explains why the engine determined the user was working on a specific task/context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryExplanation {
    /// Concise summary headline (e.g. "Reconstructed from 4 recent items and persistent task candidate activity.")
    pub summary: String,
    /// Clean, user-friendly reasons answering "Why?"
    /// e.g. ["4 related recent items", "semantic similarity between recent items", "same Aurora Website context", "task candidate remained active across sessions"]
    pub why_reasons: Vec<String>,
    /// Supporting evidence references (observed and inferred, sanitized)
    pub supporting_evidence: Vec<String>,
    /// Qualitative confidence band ("High", "Medium", "Low", "Insufficient")
    pub confidence_band: String,
    /// Epistemic classification
    pub provenance: InformationProvenance,
    /// Honest uncertainty note (never claiming certainty beyond underlying evidence)
    pub uncertainty_note: Option<String>,
}

// =========================================================================
// Task Confidence & State Models
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
/// 5. Derives memory dynamically from existing database evidence without duplicating sources of truth.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextMemory {
    /// Context identifier (e.g., workspace ID)
    pub context_id: String,
    /// Context human-friendly name (e.g., "Portfolio")
    pub context_name: String,
    /// Current task candidate ID if identified (nullable)
    pub current_task_id: Option<String>,
    /// Current inferred task title (nullable: None if insufficient evidence or unknown)
    pub current_task_title: Option<String>,
    /// Current task confidence score [0.0, 1.0] (nullable: None if insufficient)
    pub current_task_confidence: Option<f64>,
    /// Epistemic source of current task (Confirmed by user, Inferred by engine)
    pub current_task_provenance: Option<InformationProvenance>,
    /// Calibrated confidence score for the context container itself [0.0, 1.0]
    pub current_context_confidence: f64,
    /// Qualitative confidence band for context ("High", "Medium", "Low", "None")
    pub context_confidence_band: String,
    /// Epistemic source of context assignment (Confirmed by user, Inferred by engine)
    pub context_provenance: InformationProvenance,
    /// Most recent activity timestamp across items, signals, and sessions
    pub last_active_at: Option<String>,
    /// Most recently active task (either previous task or current task snapshot)
    pub last_active_task: Option<MemoryTaskReference>,
    /// Concise human-readable recent activity summary
    pub recent_activity: String,
    /// Lightweight references to recent items (no heavy payloads)
    pub recent_items: Vec<MemoryItemReference>,
    /// Lightweight references to recent tasks in this context
    pub recent_tasks: Vec<MemoryTaskReference>,
    /// Most recent context transition involving this context (if any)
    pub recent_context_transition: Option<MemoryTransitionReference>,
    /// Chronological sequence of recent contextual activity milestones
    pub activity_sequence: Vec<MemoryActivitySequenceEntry>,
    /// When this memory snapshot was derived/updated
    pub memory_updated_at: String,
    /// Activity state of the context ("active", "recently_active", "dormant", "insufficient_evidence")
    pub activity_state: ContextMemoryActivityState,
    /// Reconstructed working memory statement answering "What was the user working on?"
    pub memory_statement: String,
    /// Structured, human-readable explanation answering "Why?" for this memory statement
    pub explanation: MemoryExplanation,

    // Backward-compatibility / helper state:
    /// Inferred task state with detailed evidence lines (for legacy callers)
    pub current_task: InferredTaskState,
    /// Active Context reference (if present)
    pub context: Option<ContextReference>,
    /// Legacy alias for current_context_confidence
    pub context_confidence: f64,
    /// Bounded history of past inferred task snapshots
    pub task_history: Vec<TaskSnapshot>,
    /// Bounded history of past context snapshots
    pub context_history: Vec<ContextSnapshot>,
    /// Maximum history depth
    pub max_history_depth: usize,
    /// When current context was first observed
    pub first_seen_at: Option<String>,
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
        let ctx_id = context.as_ref().map(|c| c.id.clone()).unwrap_or_default();
        let ctx_name = context.as_ref().map(|c| c.name.clone()).unwrap_or_else(|| "Unknown".to_string());

        let default_explanation = MemoryExplanation {
            summary: format!("Contextual memory initialized for {}.", ctx_name),
            why_reasons: vec![format!("same {} context", ctx_name)],
            supporting_evidence: Vec::new(),
            confidence_band: band.to_string(),
            provenance: InformationProvenance::Observed,
            uncertainty_note: None,
        };

        Self {
            context_id: ctx_id,
            context_name: ctx_name.clone(),
            current_task_id: None,
            current_task_title: None,
            current_task_confidence: None,
            current_task_provenance: None,
            current_context_confidence: context_confidence,
            context_confidence_band: band.to_string(),
            context_provenance: InformationProvenance::Confirmed,
            last_active_at: Some(timestamp.to_string()),
            last_active_task: None,
            recent_activity: "No activity recorded yet in this context.".to_string(),
            recent_items: Vec::new(),
            recent_tasks: Vec::new(),
            recent_context_transition: None,
            activity_sequence: Vec::new(),
            memory_updated_at: timestamp.to_string(),
            activity_state: ContextMemoryActivityState::InsufficientEvidence,
            memory_statement: format!("No recent activity recorded in {}.", ctx_name),
            explanation: default_explanation,

            context,
            current_task: InferredTaskState::unknown(),
            context_confidence,
            task_history: Vec::new(),
            context_history: Vec::new(),
            max_history_depth: Self::DEFAULT_MAX_HISTORY_DEPTH,
            first_seen_at: Some(timestamp.to_string()),
            session_id: None,
        }
    }

    /// Constructs `ContextMemory` from an existing `ContextSnapshot`.
    pub fn from_snapshot(snapshot: &ContextSnapshot) -> Self {
        let mut mem = Self::new(
            Some(snapshot.reference.clone()),
            snapshot.confidence,
            &snapshot.last_seen_at,
        );
        mem.first_seen_at = Some(snapshot.first_seen_at.clone());
        mem.session_id = snapshot.session_id.clone();
        mem
    }

    /// Fluent builder to attach an active task state.
    pub fn with_task(mut self, task: InferredTaskState) -> Self {
        if task.is_confident && task.title != "Unknown" {
            self.current_task_title = Some(task.title.clone());
            self.current_task_confidence = Some(task.confidence);
            self.current_task_provenance = Some(InformationProvenance::Inferred);
            let mem_task = MemoryTaskReference {
                id: None,
                title: task.title.clone(),
                action_verb: task.action_verb.clone(),
                target_subject: task.target_subject.clone(),
                confidence: task.confidence,
                confidence_band: task.confidence_band.clone(),
                provenance: InformationProvenance::Inferred,
                first_seen_at: task.first_seen_at.clone(),
                last_active_at: task.last_seen_at.clone(),
                streak_count: task.streak_count,
                status: Some("active".to_string()),
            };
            self.last_active_task = Some(mem_task.clone());
            self.recent_tasks = vec![mem_task];
            self.activity_state = ContextMemoryActivityState::Active;
            self.memory_statement = format!("You are working on {}.", task.title);

            let mut why_reasons = Vec::new();
            if !task.evidence.is_empty() {
                why_reasons.extend(task.evidence.clone());
            } else {
                why_reasons.push("consistent candidate detected across recent activity".to_string());
            }
            why_reasons.push(format!("same {} context", self.context_name));

            self.explanation = MemoryExplanation {
                summary: format!("Reconstructed from recent activity and contextual signals in {}.", self.context_name),
                why_reasons,
                supporting_evidence: task.evidence.clone(),
                confidence_band: task.confidence_band.clone(),
                provenance: InformationProvenance::Inferred,
                uncertainty_note: if task.confidence_band == "Medium" {
                    Some("Inferred from recent patterns; may evolve with further activity.".to_string())
                } else if task.confidence_band == "Low" {
                    Some("Preliminary signal with limited corroborating evidence.".to_string())
                } else {
                    None
                },
            };
        } else {
            self.current_task_title = None;
            self.current_task_confidence = None;
            self.current_task_provenance = None;
        }
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

    /// Fluent builder to set recent items.
    pub fn with_recent_items(mut self, items: Vec<MemoryItemReference>) -> Self {
        self.recent_items = items;
        self
    }

    /// Fluent builder to set recent tasks.
    pub fn with_recent_tasks(mut self, tasks: Vec<MemoryTaskReference>) -> Self {
        self.recent_tasks = tasks;
        self
    }

    /// Fluent builder to set recent context transition.
    pub fn with_transition(mut self, transition: MemoryTransitionReference) -> Self {
        self.recent_context_transition = Some(transition);
        self
    }

    /// Fluent builder to set activity sequence.
    pub fn with_activity_sequence(mut self, sequence: Vec<MemoryActivitySequenceEntry>) -> Self {
        self.activity_sequence = sequence;
        self
    }

    /// Fluent builder to set activity state.
    pub fn with_activity_state(mut self, state: ContextMemoryActivityState) -> Self {
        self.activity_state = state;
        self
    }

    /// Fluent builder to set memory statement.
    pub fn with_memory_statement(mut self, statement: impl Into<String>) -> Self {
        self.memory_statement = statement.into();
        self
    }

    // =========================================================================
    // Core Derivation from Existing Database Evidence
    // =========================================================================

    /// Derives the contextual working memory snapshot for a specific workspace from existing database records.
    /// Does NOT create duplicate sources of truth; strictly aggregates items, signals, transitions, and tasks.
    pub fn derive_for_workspace(
        conn: &Connection,
        workspace_id: &str,
    ) -> Result<Self, String> {
        ContextMemoryBuilder::from_db(conn, workspace_id)?.build()
    }

    /// Derives the currently active contextual working memory across the entire system.
    pub fn derive_active(conn: &Connection) -> Result<Option<Self>, String> {
        let mut stmt = conn.prepare(
            "SELECT id FROM workspaces 
             WHERE lifecycle_state != 'archived' 
             ORDER BY COALESCE(last_activity_at, updated_at) DESC LIMIT 1",
        ).map_err(|e| e.to_string())?;

        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let ws_id: String = row.get(0).map_err(|e| e.to_string())?;
            Self::derive_for_workspace(conn, &ws_id).map(Some)
        } else {
            Ok(None)
        }
    }
}

// =========================================================================
// ContextMemoryBuilder: Working Memory Reconstruction Engine
// =========================================================================

/// Builder and reconstruction engine that derives `ContextMemory` snapshots
/// answering "What was the user working on?" from existing multi-modal data.
#[derive(Debug, Clone)]
pub struct ContextMemoryBuilder {
    pub workspace_id: String,
    pub workspace: Option<crate::db::models::Workspace>,
    pub items: Vec<crate::db::models::Item>,
    pub memberships: Vec<crate::db::models::ContextMembership>,
    pub segments: Vec<crate::context_engine::segments::ContextActivitySegment>,
    pub context_transitions: Vec<crate::db::models::ContextTransition>,
    pub task_candidates: Vec<crate::db::models::TaskCandidate>,
    pub task_transitions: Vec<crate::context_engine::task_transitions::DetectedTaskTransition>,
    pub evidence_set: Option<crate::context_engine::task_evidence::TaskEvidenceSet>,
    pub activity_summary: Option<crate::context_engine::summary::ContextActivitySummary>,
    pub now: DateTime<Utc>,
}

impl ContextMemoryBuilder {
    pub fn new(workspace_id: impl Into<String>) -> Self {
        Self {
            workspace_id: workspace_id.into(),
            workspace: None,
            items: Vec::new(),
            memberships: Vec::new(),
            segments: Vec::new(),
            context_transitions: Vec::new(),
            task_candidates: Vec::new(),
            task_transitions: Vec::new(),
            evidence_set: None,
            activity_summary: None,
            now: Utc::now(),
        }
    }

    pub fn with_workspace(mut self, ws: crate::db::models::Workspace) -> Self {
        self.workspace_id = ws.id.clone();
        self.workspace = Some(ws);
        self
    }

    pub fn with_items(mut self, items: Vec<crate::db::models::Item>) -> Self {
        self.items = items;
        self
    }

    pub fn with_memberships(mut self, memberships: Vec<crate::db::models::ContextMembership>) -> Self {
        self.memberships = memberships;
        self
    }

    pub fn with_segments(mut self, segments: Vec<crate::context_engine::segments::ContextActivitySegment>) -> Self {
        self.segments = segments;
        self
    }

    pub fn with_context_transitions(mut self, transitions: Vec<crate::db::models::ContextTransition>) -> Self {
        self.context_transitions = transitions;
        self
    }

    pub fn with_task_candidates(mut self, candidates: Vec<crate::db::models::TaskCandidate>) -> Self {
        self.task_candidates = candidates;
        self
    }

    pub fn with_task_transitions(mut self, transitions: Vec<crate::context_engine::task_transitions::DetectedTaskTransition>) -> Self {
        self.task_transitions = transitions;
        self
    }

    pub fn with_evidence_set(mut self, evidence: crate::context_engine::task_evidence::TaskEvidenceSet) -> Self {
        self.evidence_set = Some(evidence);
        self
    }

    pub fn with_activity_summary(mut self, summary: crate::context_engine::summary::ContextActivitySummary) -> Self {
        self.activity_summary = Some(summary);
        self
    }

    pub fn with_now(mut self, now: DateTime<Utc>) -> Self {
        self.now = now;
        self
    }

    pub fn from_db(conn: &Connection, workspace_id: &str) -> Result<Self, String> {
        let ws = crate::db::operations::get_workspace(conn, workspace_id)
            .map_err(|e| format!("Workspace '{}' not found: {}", workspace_id, e))?;

        // 1. Fetch Items
        let mut item_stmt = conn.prepare(
            "SELECT DISTINCT i.id, i.workspace_id, i.session_id, i.item_type, i.content, i.title, i.metadata, i.created_at, i.updated_at
             FROM items i
             LEFT JOIN context_memberships cm ON i.id = cm.item_id AND cm.workspace_id = ?1
             WHERE i.workspace_id = ?1 OR cm.workspace_id = ?1
             ORDER BY i.created_at DESC LIMIT 50",
        ).map_err(|e| e.to_string())?;

        let item_rows = item_stmt.query_map(params![workspace_id], |row| {
            Ok(crate::db::models::Item {
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
        }).map_err(|e| e.to_string())?;

        let mut items = Vec::new();
        for item_res in item_rows {
            if let Ok(item) = item_res {
                items.push(item);
            }
        }

        // 2. Fetch Memberships
        let mut mem_stmt = conn.prepare(
            "SELECT id, item_id, workspace_id, membership_type, confidence, source, created_at, updated_at
             FROM context_memberships
             WHERE workspace_id = ?1",
        ).map_err(|e| e.to_string())?;

        let mem_rows = mem_stmt.query_map(params![workspace_id], |row| {
            Ok(crate::db::models::ContextMembership {
                id: row.get(0)?,
                item_id: row.get(1)?,
                workspace_id: row.get(2)?,
                membership_type: row.get(3)?,
                confidence: row.get(4)?,
                source: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        }).map_err(|e| e.to_string())?;

        let mut memberships = Vec::new();
        for m in mem_rows {
            if let Ok(mem) = m {
                memberships.push(mem);
            }
        }

        // 3. Task Candidates
        let task_candidates = crate::db::operations::list_task_candidates_for_context(conn, workspace_id)
            .unwrap_or_default();

        // 4. Context Transitions
        let context_transitions = crate::db::operations::list_context_transitions_for_context(conn, workspace_id)
            .unwrap_or_default();

        // 5. Segments
        let segments = crate::context_engine::segments::ContextActivitySegmenter::new()
            .build_segments_for_workspace(conn, workspace_id)
            .unwrap_or_default();

        // 6. Evidence Set
        let evidence_set = crate::context_engine::task_evidence::TaskEvidenceCollector::new()
            .collect_for_context(conn, workspace_id)
            .ok();

        // 7. Activity Summary
        let activity_summary = Some(
            crate::context_engine::summary::ContextActivitySummaryService::new()
                .summarize_segments(&segments, None, None),
        );

        Ok(Self {
            workspace_id: workspace_id.to_string(),
            workspace: Some(ws),
            items,
            memberships,
            segments,
            context_transitions,
            task_candidates,
            task_transitions: Vec::new(),
            evidence_set,
            activity_summary,
            now: Utc::now(),
        })
    }

    pub fn build(self) -> Result<ContextMemory, String> {
        let ws_name = self.workspace.as_ref().map(|w| w.name.clone()).unwrap_or_else(|| self.workspace_id.clone());
        let lifecycle_state = self.workspace.as_ref().map(|w| w.lifecycle_state.clone()).unwrap_or_else(|| "active".to_string());
        let created_at = self.workspace.as_ref().map(|w| w.created_at.clone()).unwrap_or_else(|| self.now.to_rfc3339());

        let now_iso = self.now.to_rfc3339();
        let context_confidence = match lifecycle_state.as_str() {
            "active" => 1.0,
            "dormant" => 0.5,
            "archived" => 0.2,
            _ => 0.8,
        };
        let confidence_band = if context_confidence >= 0.75 {
            "High"
        } else if context_confidence >= 0.50 {
            "Medium"
        } else if context_confidence >= 0.25 {
            "Low"
        } else {
            "None"
        };

        // 1. Convert Items to MemoryItemReference
        let mut recent_items: Vec<MemoryItemReference> = Vec::new();
        for item in &self.items {
            let mut app_name = None;
            let mut domain = None;
            if let Ok(meta) = serde_json::from_str::<Value>(&item.metadata) {
                if let Some(app) = meta.get("app_name").and_then(|v| v.as_str()).and_then(normalize_app_name) {
                    app_name = Some(app);
                }
                if let Some(d) = meta.get("domain").and_then(|v| v.as_str()) {
                    domain = Some(d.to_string());
                } else if let Some(url_str) = meta.get("original_url").and_then(|v| v.as_str()) {
                    domain = extract_domain_from_url(url_str);
                }
            }
            if domain.is_none() && item.item_type == "url" {
                domain = extract_domain_from_url(&item.content);
            }

            let mem = self.memberships.iter().find(|m| m.item_id == item.id);
            let provenance = match mem.map(|m| m.membership_type.as_str()) {
                Some("confirmed") => InformationProvenance::Confirmed,
                Some("suggested") | Some("inferred") => InformationProvenance::Inferred,
                _ => InformationProvenance::Observed,
            };

            let display_title = item.title.clone().or_else(|| {
                if item.content.len() > 60 {
                    Some(format!("{}...", &item.content[..60]))
                } else if !item.content.trim().is_empty() {
                    Some(item.content.clone())
                } else {
                    None
                }
            });

            if !recent_items.iter().any(|existing| existing.id == item.id) {
                recent_items.push(MemoryItemReference {
                    id: item.id.clone(),
                    title: display_title,
                    item_type: item.item_type.clone(),
                    timestamp: item.created_at.clone(),
                    provenance,
                    app_name,
                    domain,
                });
            }
        }
        recent_items.truncate(20);

        // 2. Context Transition
        let recent_context_transition = self.context_transitions.first().map(|ct| MemoryTransitionReference {
            id: Some(ct.id.clone()),
            transition_type: ct.transition_type.to_string(),
            from_context_id: ct.from_context_id.clone(),
            from_context_name: None,
            to_context_id: ct.to_context_id.clone(),
            to_context_name: None,
            confidence: ct.confidence,
            detected_at: ct.detected_at.clone(),
            provenance: InformationProvenance::Inferred,
        });

        // 3. Task Inference & Ranking
        let mut current_task_id = None;
        let mut current_task_title = None;
        let mut current_task_confidence = None;
        let mut current_task_provenance = InformationProvenance::Inferred;
        let mut current_task_state = InferredTaskState::unknown();
        let mut recent_tasks: Vec<MemoryTaskReference> = Vec::new();

        // Priority 1: User-confirmed / Active Task Candidate
        let mut extracted_candidates = Vec::new();
        let confirmed_candidate = self.task_candidates.iter().find(|tc| {
            (tc.status == crate::db::models::TaskCandidateStatus::Active
                || tc.status.as_str() == "confirmed")
                && tc.confidence >= 0.50
                && !is_generic_task_name(&tc.title)
        });

        if let Some(cand) = confirmed_candidate {
            current_task_id = Some(cand.id.clone());
            current_task_title = Some(cand.title.clone());
            current_task_confidence = Some(cand.confidence);
            current_task_provenance = InformationProvenance::Confirmed;
            current_task_state = InferredTaskState::new(
                &cand.title,
                None,
                None,
                cand.confidence,
                vec![format!("User-confirmed active task: '{}'", cand.title)],
                &cand.last_active_at,
                Some(self.workspace_id.clone()),
            );
            recent_tasks.push(MemoryTaskReference {
                id: Some(cand.id.clone()),
                title: cand.title.clone(),
                action_verb: None,
                target_subject: None,
                confidence: cand.confidence,
                confidence_band: TaskConfidenceBand::from_score(cand.confidence).as_str().to_string(),
                provenance: InformationProvenance::Confirmed,
                first_seen_at: Some(cand.first_detected_at.clone()),
                last_active_at: Some(cand.last_active_at.clone()),
                streak_count: 1,
                status: Some(cand.status.as_str().to_string()),
            });
        } else {
            // Priority 2: Extract & Rank Candidates from Evidence Set or Items
            let rank_result = if let Some(evidence_set) = &self.evidence_set {
                let candidates = crate::context_engine::task_extractor::TaskCandidateExtractor::extract_from_evidence(evidence_set, None);
                crate::context_engine::task_ranking::TaskCandidateRanker::rank_candidates(&candidates, Some(context_confidence), None)
            } else if !self.items.is_empty() {
                let mut ev_set = crate::context_engine::task_evidence::TaskEvidenceSet::new(Some(self.workspace_id.clone()), None);
                for item in &self.items {
                    let ev = crate::context_engine::task_evidence::TaskEvidence::observed_item_content(
                        &item.id,
                        item.title.as_deref(),
                        &item.content,
                        &item.item_type,
                        Some(&item.created_at),
                    );
                    ev_set.add_evidence(ev);
                }
                let candidates = crate::context_engine::task_extractor::TaskCandidateExtractor::extract_from_evidence(&ev_set, None);
                crate::context_engine::task_ranking::TaskCandidateRanker::rank_candidates(&candidates, Some(context_confidence), None)
            } else {
                crate::context_engine::task_ranking::TaskRankingResult {
                    ranked_candidates: Vec::new(),
                    top_candidate: None,
                    ambiguity_state: crate::context_engine::task_ranking::TaskAmbiguityState::InsufficientEvidence,
                    context_id: Some(self.workspace_id.clone()),
                    context_confidence: Some(context_confidence),
                    total_candidates_evaluated: 0,
                }
            };

            for r in &rank_result.ranked_candidates {
                extracted_candidates.push(r.candidate.clone());
            }

            if let Some(top) = &rank_result.top_candidate {
                let is_generic = is_generic_task_name(&top.candidate.title);
                let is_confident = top.confidence >= 0.45 && !is_generic && rank_result.ambiguity_state == crate::context_engine::task_ranking::TaskAmbiguityState::ClearLeader;

                if is_confident {
                    current_task_title = Some(top.candidate.title.clone());
                    current_task_confidence = Some(top.confidence);
                    current_task_provenance = InformationProvenance::Inferred;
                    current_task_state = InferredTaskState::from_ranked(
                        top,
                        Some(self.workspace_id.clone()),
                        &now_iso,
                    );
                }
            }
        }

        if let Some(title) = &current_task_title {
            recent_tasks.push(MemoryTaskReference {
                id: current_task_id.clone(),
                title: title.clone(),
                action_verb: current_task_state.action_verb.clone(),
                target_subject: current_task_state.target_subject.clone(),
                confidence: current_task_state.confidence,
                confidence_band: current_task_state.confidence_band.clone(),
                provenance: current_task_provenance,
                first_seen_at: current_task_state.first_seen_at.clone(),
                last_active_at: current_task_state.last_seen_at.clone(),
                streak_count: current_task_state.streak_count,
                status: Some("active".to_string()),
            });
        }

        for pc in &self.task_candidates {
            if !recent_tasks.iter().any(|t| t.title.eq_ignore_ascii_case(&pc.title)) {
                let prov = if pc.status.as_str() == "active" || pc.status.as_str() == "confirmed" {
                    InformationProvenance::Confirmed
                } else {
                    InformationProvenance::Inferred
                };
                recent_tasks.push(MemoryTaskReference {
                    id: Some(pc.id.clone()),
                    title: pc.title.clone(),
                    action_verb: None,
                    target_subject: None,
                    confidence: pc.confidence,
                    confidence_band: TaskConfidenceBand::from_score(pc.confidence).as_str().to_string(),
                    provenance: prov,
                    first_seen_at: Some(pc.first_detected_at.clone()),
                    last_active_at: Some(pc.last_active_at.clone()),
                    streak_count: 1,
                    status: Some(pc.status.as_str().to_string()),
                });
            }
        }

        for ec in &extracted_candidates {
            if !recent_tasks.iter().any(|t| t.title.eq_ignore_ascii_case(&ec.title)) {
                recent_tasks.push(MemoryTaskReference {
                    id: None,
                    title: ec.title.clone(),
                    action_verb: ec.action_verb.clone(),
                    target_subject: ec.target_subject.clone(),
                    confidence: ec.relevance_score,
                    confidence_band: TaskConfidenceBand::from_score(ec.relevance_score).as_str().to_string(),
                    provenance: InformationProvenance::Inferred,
                    first_seen_at: None,
                    last_active_at: None,
                    streak_count: 1,
                    status: Some("detected".to_string()),
                });
            }
        }
        recent_tasks.truncate(5);

        let last_active_task = if recent_tasks.len() > 1 && current_task_title.is_some() {
            Some(recent_tasks[0].clone())
        } else {
            recent_tasks.first().cloned()
        };

        // 4. Determine last_active_at
        let mut last_active_at = self.workspace.as_ref().and_then(|w| w.last_activity_at.clone());
        for item in &recent_items {
            if last_active_at.as_deref().map(|curr| item.timestamp.as_str() > curr).unwrap_or(true) {
                last_active_at = Some(item.timestamp.clone());
            }
        }
        if let Some(trans) = &recent_context_transition {
            if last_active_at.as_deref().map(|curr| trans.detected_at.as_str() > curr).unwrap_or(true) {
                last_active_at = Some(trans.detected_at.clone());
            }
        }
        for seg in &self.segments {
            if last_active_at.as_deref().map(|curr| seg.ended_at.as_str() > curr).unwrap_or(true) {
                last_active_at = Some(seg.ended_at.clone());
            }
        }

        // 5. Activity State Classification
        let activity_state = if lifecycle_state == "dormant" || lifecycle_state == "archived" {
            ContextMemoryActivityState::Dormant
        } else if recent_items.is_empty() && recent_context_transition.is_none() && self.task_candidates.is_empty() && self.segments.is_empty() {
            ContextMemoryActivityState::InsufficientEvidence
        } else if let Some(last_ts) = &last_active_at {
            if let Ok(dt) = DateTime::parse_from_rfc3339(last_ts) {
                let elapsed_secs = (self.now - dt.with_timezone(&Utc)).num_seconds().max(0);
                if elapsed_secs <= 3600 {
                    ContextMemoryActivityState::Active
                } else if elapsed_secs <= 86400 {
                    ContextMemoryActivityState::RecentlyActive
                } else {
                    ContextMemoryActivityState::Dormant
                }
            } else {
                ContextMemoryActivityState::RecentlyActive
            }
        } else {
            ContextMemoryActivityState::InsufficientEvidence
        };

        // 6. Natural Language Memory Statement ("What was the user working on?")
        let memory_statement = match (&current_task_title, activity_state) {
            (Some(task), ContextMemoryActivityState::Active) => {
                format!("You are working on {}.", task)
            }
            (Some(task), ContextMemoryActivityState::RecentlyActive) => {
                format!("You were working on {}.", task)
            }
            (Some(task), ContextMemoryActivityState::Dormant) => {
                format!("Context is dormant. You previously worked on {}.", task)
            }
            (Some(task), ContextMemoryActivityState::InsufficientEvidence) => {
                format!("You were working on {}.", task)
            }
            (None, ContextMemoryActivityState::Active) => {
                format!("You are active in {}.", ws_name)
            }
            (None, ContextMemoryActivityState::RecentlyActive) => {
                format!("You were recently active in {}.", ws_name)
            }
            (None, ContextMemoryActivityState::Dormant) => {
                if let Some(ts) = &last_active_at {
                    format!("Context is dormant. Last activity was recorded on {}.", ts)
                } else {
                    "Context is dormant.".to_string()
                }
            }
            (None, ContextMemoryActivityState::InsufficientEvidence) => {
                format!("No recent activity recorded in {}.", ws_name)
            }
        };

        // 7. Activity Sequence
        let mut activity_sequence = Vec::new();
        if let Some(trans) = &recent_context_transition {
            activity_sequence.push(MemoryActivitySequenceEntry {
                entry_type: "context_transition".to_string(),
                description: format!("Context transition: {}", trans.transition_type),
                timestamp: trans.detected_at.clone(),
                provenance: trans.provenance,
                reference_id: trans.id.clone(),
            });
        }
        for trans in &self.task_transitions {
            activity_sequence.push(MemoryActivitySequenceEntry {
                entry_type: "task_transition".to_string(),
                description: format!("Task transition: {} -> {}", trans.from_task.as_deref().unwrap_or("None"), trans.to_task.as_deref().unwrap_or("None")),
                timestamp: trans.timestamp.clone(),
                provenance: InformationProvenance::Inferred,
                reference_id: Some(trans.id.clone()),
            });
        }
        for item in recent_items.iter().take(5) {
            let desc = if let Some(t) = &item.title {
                format!("Captured {} '{}'", item.item_type, t)
            } else {
                format!("Captured {} item", item.item_type)
            };
            activity_sequence.push(MemoryActivitySequenceEntry {
                entry_type: "item_captured".to_string(),
                description: desc,
                timestamp: item.timestamp.clone(),
                provenance: item.provenance,
                reference_id: Some(item.id.clone()),
            });
        }
        if let Some(title) = &current_task_title {
            activity_sequence.push(MemoryActivitySequenceEntry {
                entry_type: "task_active".to_string(),
                description: format!("Active task: {}", title),
                timestamp: last_active_at.clone().unwrap_or_else(|| now_iso.clone()),
                provenance: current_task_provenance,
                reference_id: current_task_id.clone(),
            });
        }
        activity_sequence.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        // 8. Recent Activity Summary String
        let recent_activity = match (&current_task_title, activity_state) {
            (Some(title), ContextMemoryActivityState::Active) => {
                let conf_pct = (current_task_confidence.unwrap_or(0.0) * 100.0) as u32;
                if recent_items.is_empty() {
                    format!("Active working on '{}' ({}% confidence).", title, conf_pct)
                } else {
                    format!(
                        "Active working on '{}' ({}% confidence) with {} recent item{}.",
                        title,
                        conf_pct,
                        recent_items.len(),
                        if recent_items.len() == 1 { "" } else { "s" }
                    )
                }
            }
            (Some(title), ContextMemoryActivityState::RecentlyActive) => {
                let conf_pct = (current_task_confidence.unwrap_or(0.0) * 100.0) as u32;
                if recent_items.is_empty() {
                    format!("Recently worked on '{}' ({}% confidence).", title, conf_pct)
                } else {
                    format!(
                        "Recently worked on '{}' ({}% confidence) with {} recent item{}.",
                        title,
                        conf_pct,
                        recent_items.len(),
                        if recent_items.len() == 1 { "" } else { "s" }
                    )
                }
            }
            (Some(title), ContextMemoryActivityState::Dormant) => {
                let last_ts = last_active_at.as_deref().unwrap_or("unknown date");
                format!("Context is dormant. Previously worked on '{}' (last active {}).", title, last_ts)
            }
            (None, ContextMemoryActivityState::Dormant) => {
                let last_ts = last_active_at.as_deref().unwrap_or("unknown date");
                format!("Context is dormant. Last activity was recorded on {}.", last_ts)
            }
            (None, ContextMemoryActivityState::Active) => {
                if recent_items.is_empty() {
                    format!("Active in {}.", ws_name)
                } else {
                    format!(
                        "Active in {} with {} recent item{}.",
                        ws_name,
                        recent_items.len(),
                        if recent_items.len() == 1 { "" } else { "s" }
                    )
                }
            }
            (None, ContextMemoryActivityState::RecentlyActive) => {
                if recent_items.is_empty() {
                    format!("Recently active in {}.", ws_name)
                } else {
                    format!(
                        "Recently active in {} with {} recent item{}.",
                        ws_name,
                        recent_items.len(),
                        if recent_items.len() == 1 { "" } else { "s" }
                    )
                }
            }
            _ => "No activity recorded yet in this context.".to_string(),
        };

        let ctx_ref = ContextReference::workspace(&self.workspace_id, &ws_name);

        // 9. Synthesize Explainable Memory (Why?)
        let mut why_reasons = Vec::new();
        let mut supporting_evidence = Vec::new();

        if !recent_items.is_empty() {
            let count = recent_items.len();
            why_reasons.push(format!("{} related recent item{}", count, if count == 1 { "" } else { "s" }));
            for item in recent_items.iter().take(4) {
                if let Some(ref t) = item.title {
                    supporting_evidence.push(format!("Captured item: '{}'", t));
                }
            }
        }

        if let Some(ref task) = current_task_title {
            if current_task_provenance == InformationProvenance::Confirmed {
                why_reasons.push(format!("confirmed task in {}", ws_name));
            } else {
                if recent_items.len() >= 2 {
                    why_reasons.push("semantic similarity between recent items".to_string());
                }
                if self.task_candidates.iter().any(|c| c.title.eq_ignore_ascii_case(task)) {
                    why_reasons.push("task candidate remained active across sessions".to_string());
                }
            }
        }

        why_reasons.push(format!("same {} context", ws_name));

        if let Some(ref trans) = recent_context_transition {
            if trans.transition_type == "context_return" || trans.transition_type == "return" {
                why_reasons.push(format!("returned to {} context", ws_name));
            }
        }

        let explanation_conf_band = if current_task_title.is_some() {
            current_task_state.confidence_band.clone()
        } else {
            confidence_band.to_string()
        };

        let uncertainty_note = if current_task_title.is_some() {
            if explanation_conf_band == "Medium" {
                Some("Inferred from recent patterns; may evolve with further activity.".to_string())
            } else if explanation_conf_band == "Low" || explanation_conf_band == "Insufficient" {
                Some("Preliminary signal with limited corroborating evidence.".to_string())
            } else {
                None
            }
        } else {
            None
        };

        let explanation_summary = if let Some(ref _task) = current_task_title {
            format!("Reconstructed from recent activity and contextual signals in {}.", ws_name)
        } else {
            format!("Reconstructed from recent activity signals in {}.", ws_name)
        };

        let explanation = MemoryExplanation {
            summary: explanation_summary,
            why_reasons,
            supporting_evidence,
            confidence_band: explanation_conf_band,
            provenance: current_task_provenance,
            uncertainty_note,
        };

        Ok(ContextMemory {
            context_id: self.workspace_id.clone(),
            context_name: ws_name,
            current_task_id,
            current_task_title: current_task_title.clone(),
            current_task_confidence,
            current_task_provenance: if current_task_title.is_some() { Some(current_task_provenance) } else { None },
            current_context_confidence: context_confidence,
            context_confidence_band: confidence_band.to_string(),
            context_provenance: InformationProvenance::Confirmed,
            last_active_at,
            last_active_task,
            recent_activity,
            recent_items,
            recent_tasks,
            recent_context_transition,
            activity_sequence,
            memory_updated_at: now_iso,
            activity_state,
            memory_statement,
            explanation,

            current_task: current_task_state,
            context: Some(ctx_ref),
            context_confidence,
            task_history: Vec::new(),
            context_history: Vec::new(),
            max_history_depth: ContextMemory::DEFAULT_MAX_HISTORY_DEPTH,
            first_seen_at: Some(created_at),
            session_id: None,
        })
    }
}

// =========================================================================
// Accessors & Queries
// =========================================================================

impl ContextMemory {
    /// Returns the context reference if active.
    pub fn context_identity(&self) -> Option<&ContextReference> {
        self.context.as_ref()
    }

    /// Returns the active context / workspace name (e.g., "Portfolio", "College").
    pub fn context_name(&self) -> &str {
        if !self.context_name.is_empty() {
            &self.context_name
        } else {
            self.context
                .as_ref()
                .map(|c| c.name.as_str())
                .unwrap_or("Unknown")
        }
    }

    /// Returns the title of the current task if confident, otherwise `"Unknown"`.
    pub fn active_task_title(&self) -> &str {
        if let Some(title) = &self.current_task_title {
            title.as_str()
        } else if self.current_task.is_confident && self.current_task.title != "Unknown" {
            &self.current_task.title
        } else {
            "Unknown"
        }
    }

    /// Legacy helper returning context ID if present.
    pub fn context_id_str(&self) -> Option<&str> {
        if !self.context_id.is_empty() {
            Some(&self.context_id)
        } else {
            self.context.as_ref().map(|c| c.id.as_str())
        }
    }

    /// Returns the calibrated context confidence.
    pub fn context_confidence(&self) -> f64 {
        self.current_context_confidence
    }

    /// Returns the qualitative context confidence band.
    pub fn context_confidence_band(&self) -> &str {
        &self.context_confidence_band
    }

    /// Returns a reference to the active `InferredTaskState`.
    pub fn current_task(&self) -> &InferredTaskState {
        &self.current_task
    }

    /// Returns the task confidence score.
    pub fn task_confidence(&self) -> f64 {
        self.current_task_confidence.unwrap_or(self.current_task.confidence)
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
        self.current_task_title.is_some()
            || (self.current_task.is_confident && self.current_task.title != "Unknown")
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
                self.current_task_confidence = Some(ranked.confidence);

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
                if let Some(prev_title) = self.current_task_title.clone() {
                    self.last_active_task = Some(MemoryTaskReference {
                        id: self.current_task_id.clone(),
                        title: prev_title,
                        action_verb: self.current_task.action_verb.clone(),
                        target_subject: self.current_task.target_subject.clone(),
                        confidence: self.current_task.confidence,
                        confidence_band: self.current_task.confidence_band.clone(),
                        provenance: InformationProvenance::Inferred,
                        first_seen_at: self.current_task.first_seen_at.clone(),
                        last_active_at: self.current_task.last_seen_at.clone(),
                        streak_count: self.current_task.streak_count,
                        status: Some("active".to_string()),
                    });
                }
                self.archive_current_task();

                let mut new_task = InferredTaskState::from_ranked(
                    ranked,
                    self.context_id_str().map(|s| s.to_string()),
                    timestamp,
                );
                new_task.first_seen_at = Some(timestamp.to_string());
                new_task.last_seen_at = Some(timestamp.to_string());
                self.current_task_title = Some(new_task.title.clone());
                self.current_task_confidence = Some(new_task.confidence);
                self.current_task = new_task;
            }
        } else {
            // Insufficient evidence - do NOT force a task
            if !self.has_confident_task() {
                self.current_task = InferredTaskState::unknown();
                self.current_task.confidence = ranked.confidence;
                self.current_task.last_seen_at = Some(timestamp.to_string());
                self.current_task_title = None;
                self.current_task_confidence = None;
            }
        }

        self.last_active_at = Some(timestamp.to_string());
        self.memory_updated_at = timestamp.to_string();
    }

    /// Updates the active task from an observed `TaskSnapshot`.
    pub fn update_from_snapshot(&mut self, snapshot: &TaskSnapshot, evidence: Vec<String>) {
        if self.has_confident_task() && !snapshot.is_same_task(&self.current_task.title, self.current_task.target_subject.as_deref()) {
            if let Some(prev_title) = self.current_task_title.clone() {
                self.last_active_task = Some(MemoryTaskReference {
                    id: self.current_task_id.clone(),
                    title: prev_title,
                    action_verb: self.current_task.action_verb.clone(),
                    target_subject: self.current_task.target_subject.clone(),
                    confidence: self.current_task.confidence,
                    confidence_band: self.current_task.confidence_band.clone(),
                    provenance: InformationProvenance::Inferred,
                    first_seen_at: self.current_task.first_seen_at.clone(),
                    last_active_at: self.current_task.last_seen_at.clone(),
                    streak_count: self.current_task.streak_count,
                    status: Some("active".to_string()),
                });
            }
            self.archive_current_task();
        }
        self.current_task = InferredTaskState::from_snapshot(snapshot, evidence);
        if self.current_task.is_confident && self.current_task.title != "Unknown" {
            self.current_task_title = Some(self.current_task.title.clone());
            self.current_task_confidence = Some(self.current_task.confidence);
        } else {
            self.current_task_title = None;
            self.current_task_confidence = None;
        }
        self.last_active_at = Some(snapshot.last_seen_at.clone());
        self.memory_updated_at = snapshot.last_seen_at.clone();
    }

    /// Updates state from a detected `DetectedTaskTransition`.
    pub fn update_from_task_transition(&mut self, transition: &DetectedTaskTransition) {
        match transition.transition_type {
            TaskTransitionType::TaskStart
            | TaskTransitionType::TaskSwitch
            | TaskTransitionType::TaskReturn => {
                if let Some(to_task_title) = &transition.to_task {
                    if self.has_confident_task() && !self.current_task.title.eq_ignore_ascii_case(to_task_title) {
                        self.last_active_task = Some(MemoryTaskReference {
                            id: self.current_task_id.clone(),
                            title: self.current_task.title.clone(),
                            action_verb: self.current_task.action_verb.clone(),
                            target_subject: self.current_task.target_subject.clone(),
                            confidence: self.current_task.confidence,
                            confidence_band: self.current_task.confidence_band.clone(),
                            provenance: InformationProvenance::Inferred,
                            first_seen_at: self.current_task.first_seen_at.clone(),
                            last_active_at: self.current_task.last_seen_at.clone(),
                            streak_count: self.current_task.streak_count,
                            status: Some("active".to_string()),
                        });
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
                    self.current_task_title = Some(to_task_title.clone());
                    self.current_task_confidence = Some(transition.confidence);
                    self.current_task = task_state;
                    self.last_active_at = Some(transition.timestamp.clone());
                    self.memory_updated_at = transition.timestamp.clone();
                }
            }
            TaskTransitionType::TaskEnd => {
                if self.has_confident_task() {
                    self.last_active_task = Some(MemoryTaskReference {
                        id: self.current_task_id.clone(),
                        title: self.current_task.title.clone(),
                        action_verb: self.current_task.action_verb.clone(),
                        target_subject: self.current_task.target_subject.clone(),
                        confidence: self.current_task.confidence,
                        confidence_band: self.current_task.confidence_band.clone(),
                        provenance: InformationProvenance::Inferred,
                        first_seen_at: self.current_task.first_seen_at.clone(),
                        last_active_at: self.current_task.last_seen_at.clone(),
                        streak_count: self.current_task.streak_count,
                        status: Some("ended".to_string()),
                    });
                }
                self.archive_current_task();
                self.current_task = InferredTaskState::unknown();
                self.current_task_title = None;
                self.current_task_confidence = None;
                self.last_active_at = Some(transition.timestamp.clone());
                self.memory_updated_at = transition.timestamp.clone();
            }
            TaskTransitionType::AmbiguousTaskChange => {
                if self.has_confident_task() {
                    self.current_task.confidence = (self.current_task.confidence * 0.8).max(0.1);
                    self.current_task.confidence_band =
                        TaskConfidenceBand::from_score(self.current_task.confidence).as_str().to_string();
                    self.current_task.ambiguity_state = Some(TaskAmbiguityState::HighlyAmbiguous);
                    self.current_task_confidence = Some(self.current_task.confidence);
                }
                self.last_active_at = Some(transition.timestamp.clone());
                self.memory_updated_at = transition.timestamp.clone();
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
                confidence: self.current_context_confidence,
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

        self.context_id = new_context.id.clone();
        self.context_name = new_context.name.clone();
        self.context = Some(new_context);
        self.current_context_confidence = confidence;
        self.context_confidence = confidence;
        self.context_confidence_band = band.to_string();
        self.current_task = InferredTaskState::unknown();
        self.current_task_id = None;
        self.current_task_title = None;
        self.current_task_confidence = None;
        self.first_seen_at = Some(timestamp.to_string());
        self.last_active_at = Some(timestamp.to_string());
        self.memory_updated_at = timestamp.to_string();
    }

    /// Clears and archives the active task (e.g. on task completion or long pause).
    pub fn clear_current_task(&mut self, timestamp: Option<&str>) {
        self.archive_current_task();
        self.current_task = InferredTaskState::unknown();
        self.current_task_id = None;
        self.current_task_title = None;
        self.current_task_confidence = None;
        if let Some(ts) = timestamp {
            self.last_active_at = Some(ts.to_string());
            self.memory_updated_at = ts.to_string();
        }
    }

    /// Touches activity timestamp.
    pub fn touch_activity(&mut self, timestamp: &str) {
        self.last_active_at = Some(timestamp.to_string());
        self.memory_updated_at = timestamp.to_string();
        if self.current_task.is_confident {
            self.current_task.last_seen_at = Some(timestamp.to_string());
        }
    }

    /// Internal helper to archive current active task to `task_history`.
    fn archive_current_task(&mut self) {
        if let Some(snap) = self.current_task.to_snapshot() {
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
    pub fn format_summary(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("Context:\n{}\n\n", self.context_name()));

        if self.has_confident_task() {
            out.push_str(&format!("Current task:\n{}\n\n", self.active_task_title()));
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
            "context_id": self.context_id,
            "context_name": self.context_name(),
            "current_task_id": self.current_task_id,
            "current_task_title": self.current_task_title,
            "current_task_confidence": self.current_task_confidence,
            "current_task_provenance": self.current_task_provenance.map(|p| p.as_str()),
            "current_context_confidence": self.current_context_confidence,
            "context_confidence_band": self.context_confidence_band,
            "context_provenance": self.context_provenance,
            "last_active_at": self.last_active_at,
            "last_active_task": self.last_active_task,
            "recent_activity": self.recent_activity,
            "recent_items": self.recent_items,
            "recent_tasks": self.recent_tasks,
            "recent_context_transition": self.recent_context_transition,
            "activity_sequence": self.activity_sequence,
            "memory_updated_at": self.memory_updated_at,
            "activity_state": self.activity_state.as_str(),
            "memory_statement": self.memory_statement,
            "context": self.context,
            "current_task": {
                "title": self.active_task_title(),
                "action_verb": self.current_task.action_verb,
                "target_subject": self.current_task.target_subject,
                "confidence": self.current_task.confidence,
                "confidence_band": self.current_task.confidence_band,
                "is_confident": self.current_task.is_confident,
                "evidence": self.current_task.evidence,
            },
            "has_confident_task": self.has_confident_task(),
            "task_history_count": self.task_history.len(),
            "context_history_count": self.context_history.len(),
            "first_seen_at": self.first_seen_at,
            "session_id": self.session_id,
        })
    }
}

// =========================================================================
// Repository / Service Interface
// =========================================================================

/// Repository abstraction for deriving and reconstructing ContextMemory snapshots.
pub trait ContextMemoryRepository: Send + Sync {
    /// Derives the contextual memory snapshot for a given workspace/context.
    fn derive_context_memory(&self, conn: &Connection, workspace_id: &str) -> Result<ContextMemory, String>;

    /// Derives the currently active global contextual memory snapshot across the app.
    fn derive_active_context_memory(&self, conn: &Connection) -> Result<Option<ContextMemory>, String>;

    /// Reconstructs the contextual working memory answering "What was the user working on?".
    fn reconstruct_memory(&self, conn: &Connection, workspace_id: &str) -> Result<ContextMemory, String>;

    /// Reconstructs the currently active global contextual working memory.
    fn reconstruct_active_memory(&self, conn: &Connection) -> Result<Option<ContextMemory>, String>;
}

/// Concrete stateless service providing ContextMemory derivation and reconstruction.
#[derive(Debug, Default, Clone)]
pub struct ContextMemoryService;

impl ContextMemoryService {
    pub fn new() -> Self {
        Self
    }

    pub fn reconstruct_memory(&self, conn: &Connection, workspace_id: &str) -> Result<ContextMemory, String> {
        ContextMemoryBuilder::from_db(conn, workspace_id)?.build()
    }

    pub fn reconstruct_active_memory(&self, conn: &Connection) -> Result<Option<ContextMemory>, String> {
        ContextMemory::derive_active(conn)
    }
}

impl ContextMemoryRepository for ContextMemoryService {
    fn derive_context_memory(&self, conn: &Connection, workspace_id: &str) -> Result<ContextMemory, String> {
        ContextMemory::derive_for_workspace(conn, workspace_id)
    }

    fn derive_active_context_memory(&self, conn: &Connection) -> Result<Option<ContextMemory>, String> {
        ContextMemory::derive_active(conn)
    }

    fn reconstruct_memory(&self, conn: &Connection, workspace_id: &str) -> Result<ContextMemory, String> {
        self.reconstruct_memory(conn, workspace_id)
    }

    fn reconstruct_active_memory(&self, conn: &Connection) -> Result<Option<ContextMemory>, String> {
        self.reconstruct_active_memory(conn)
    }
}

// =========================================================================
// Unit & Integration Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_engine::task_extractor::{ExtractedTaskCandidate, TaskCandidateEvidenceRef};

    fn setup_in_memory_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_empty_context() {
        let conn = setup_in_memory_db();
        let ws = crate::db::operations::create_workspace(&conn, "Empty Workspace").unwrap();

        let memory = ContextMemory::derive_for_workspace(&conn, &ws.id).unwrap();

        assert_eq!(memory.context_id, ws.id);
        assert_eq!(memory.context_name, "Empty Workspace");
        assert!(memory.current_task_id.is_none());
        assert!(memory.current_task_title.is_none());
        assert!(memory.current_task_confidence.is_none());
        assert_eq!(memory.current_context_confidence, 1.0);
        assert_eq!(memory.context_provenance, InformationProvenance::Confirmed);
        assert!(memory.last_active_task.is_none());
        assert!(memory.recent_items.is_empty());
        assert!(memory.recent_tasks.is_empty());
        assert!(memory.recent_context_transition.is_none());
        assert!(memory.activity_sequence.is_empty());
        assert!(memory.recent_activity.contains("No activity recorded yet"));
        assert_eq!(memory.active_task_title(), "Unknown");
        assert!(!memory.has_confident_task());
    }

    #[test]
    fn test_context_with_one_activity() {
        let conn = setup_in_memory_db();
        let ws = crate::db::operations::create_workspace(&conn, "Design System").unwrap();

        let item = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://figma.com/file/123/Button-Components",
            Some("Button Components Figma"),
            r#"{"app_name":"Figma","domain":"figma.com"}"#,
        )
        .unwrap();

        let memory = ContextMemory::derive_for_workspace(&conn, &ws.id).unwrap();

        assert_eq!(memory.context_id, ws.id);
        assert_eq!(memory.recent_items.len(), 1);
        let ref_item = &memory.recent_items[0];
        assert_eq!(ref_item.id, item.id);
        assert_eq!(ref_item.title.as_deref(), Some("Button Components Figma"));
        assert_eq!(ref_item.item_type, "url");
        assert_eq!(ref_item.provenance, InformationProvenance::Confirmed);
        assert_eq!(ref_item.app_name.as_deref(), Some("Figma"));
        assert_eq!(ref_item.domain.as_deref(), Some("figma.com"));
        assert!(memory.last_active_at.is_some());
        assert!(!memory.activity_sequence.is_empty());
        let item_entry = memory
            .activity_sequence
            .iter()
            .find(|e| e.entry_type == "item_captured")
            .expect("item_captured entry should be present in activity sequence");
        assert_eq!(item_entry.provenance, InformationProvenance::Confirmed);
    }

    #[test]
    fn test_context_with_multiple_tasks() {
        let conn = setup_in_memory_db();
        let ws = crate::db::operations::create_workspace(&conn, "Product Work").unwrap();

        // Items for Task 1: Hero Animation
        let _i1 = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://figma.com/design/hero-animation",
            Some("Hero Animation Board"),
            r#"{"app_name":"Figma","domain":"figma.com"}"#,
        )
        .unwrap();
        let _i2 = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Framer motion code for hero animation transition",
            Some("Hero Animation Code"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();

        // Also add a persisted task candidate for Task 2: Case Study
        let candidate2 = crate::db::models::TaskCandidate {
            id: "task-case-study".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Case study writing".to_string(),
            confidence: 0.72,
            status: crate::db::models::TaskCandidateStatus::Detected,
            first_detected_at: "2026-09-13T10:00:00Z".to_string(),
            last_active_at: "2026-09-13T10:30:00Z".to_string(),
            created_at: "2026-09-13T10:00:00Z".to_string(),
            updated_at: "2026-09-13T10:30:00Z".to_string(),
        };
        crate::db::operations::create_task_candidate(&conn, &candidate2).unwrap();

        let memory = ContextMemory::derive_for_workspace(&conn, &ws.id).unwrap();

        assert_eq!(memory.context_id, ws.id);
        assert!(memory.recent_tasks.len() >= 2);
        assert!(memory.recent_tasks.iter().any(|t| t.title.to_lowercase().contains("case study")));
    }

    #[test]
    fn test_context_with_task_transition() {
        let conn = setup_in_memory_db();
        let ws = crate::db::operations::create_workspace(&conn, "CS106B").unwrap();

        let mut memory = ContextMemory::derive_for_workspace(&conn, &ws.id).unwrap();

        // Initial task state
        let task1 = InferredTaskState::new(
            "Recursion Assignment",
            Some("Coding".to_string()),
            Some("Recursion Assignment".to_string()),
            0.85,
            vec!["recursion.cpp".to_string()],
            "2026-09-13T10:00:00Z",
            Some(ws.id.clone()),
        );
        memory.current_task = task1.clone();
        memory.current_task_title = Some("Recursion Assignment".to_string());
        memory.current_task_confidence = Some(0.85);

        // Detect a task transition to "Priority Queue Implementation"
        let transition = DetectedTaskTransition {
            id: "trans-task-1".to_string(),
            transition_type: TaskTransitionType::TaskSwitch,
            from_task: Some("Recursion Assignment".to_string()),
            to_task: Some("Priority Queue Implementation".to_string()),
            confidence: 0.88,
            timestamp: "2026-09-13T11:00:00Z".to_string(),
            context_id: Some(ws.id.clone()),
            evidence: vec!["pqueue.cpp".to_string()],
            trigger_item_id: None,
            duration_seconds: None,
            metadata: serde_json::json!({}),
        };

        memory.update_from_task_transition(&transition);

        assert_eq!(memory.current_task_title.as_deref(), Some("Priority Queue Implementation"));
        assert_eq!(memory.current_task_confidence, Some(0.88));
        assert!(memory.last_active_task.is_some());
        assert_eq!(memory.last_active_task.as_ref().unwrap().title, "Recursion Assignment");
        assert_eq!(memory.task_history.len(), 1);
        assert_eq!(memory.task_history[0].title, "Recursion Assignment");
    }

    #[test]
    fn test_dormant_context() {
        let conn = setup_in_memory_db();
        let ws = crate::db::operations::create_workspace(&conn, "Archive Project").unwrap();
        crate::db::operations::update_workspace_lifecycle_state(&conn, &ws.id, "dormant").unwrap();

        let memory = ContextMemory::derive_for_workspace(&conn, &ws.id).unwrap();

        assert_eq!(memory.context_id, ws.id);
        assert_eq!(memory.current_context_confidence, 0.5);
        assert_eq!(memory.context_confidence_band, "Medium");
        assert!(memory.recent_activity.to_lowercase().contains("dormant"));
    }

    #[test]
    fn test_returned_context() {
        let conn = setup_in_memory_db();
        let ws_a = crate::db::operations::create_workspace(&conn, "Main Workspace").unwrap();
        let ws_b = crate::db::operations::create_workspace(&conn, "Detour Workspace").unwrap();

        let transition = crate::db::models::ContextTransition {
            id: "trans-return-1".to_string(),
            from_context_id: Some(ws_b.id.clone()),
            to_context_id: Some(ws_a.id.clone()),
            transition_type: crate::context_engine::transitions::ContextTransitionType::ContextReturn,
            confidence: 0.90,
            detected_at: "2026-09-13T14:00:00Z".to_string(),
            session_id: None,
            trigger_item_id: None,
            trigger_signal_id: None,
            metadata: "{}".to_string(),
            created_at: "2026-09-13T14:00:00Z".to_string(),
        };
        crate::db::operations::create_context_transition(&conn, &transition).unwrap();

        let memory = ContextMemory::derive_for_workspace(&conn, &ws_a.id).unwrap();

        assert!(memory.recent_context_transition.is_some());
        let tr = memory.recent_context_transition.as_ref().unwrap();
        assert_eq!(tr.transition_type, "context_return");
        assert_eq!(tr.from_context_id.as_deref(), Some(ws_b.id.as_str()));
        assert_eq!(tr.to_context_id.as_deref(), Some(ws_a.id.as_str()));
        assert_eq!(tr.provenance, InformationProvenance::Inferred);
        assert!(memory.activity_sequence.iter().any(|e| e.entry_type == "context_transition"));
    }

    #[test]
    fn test_confirmed_vs_inferred_information() {
        let conn = setup_in_memory_db();
        let ws = crate::db::operations::create_workspace(&conn, "Research Work").unwrap();

        // 1. Confirmed item via membership
        let item_conf = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Explicitly confirmed notes",
            Some("Confirmed Notes"),
            "{}",
        )
        .unwrap();
        crate::db::operations::create_or_update_membership(
            &conn,
            &item_conf.id,
            &ws.id,
            "confirmed",
            1.0,
            "user",
        )
        .unwrap();

        // 2. Inferred item via membership
        let item_inf = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Suggested paper draft",
            Some("Suggested Draft"),
            "{}",
        )
        .unwrap();
        crate::db::operations::create_or_update_membership(
            &conn,
            &item_inf.id,
            &ws.id,
            "suggested",
            0.65,
            "context_engine",
        )
        .unwrap();

        // 3. Directly captured item in workspace (Observed)
        let item_obs = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://arxiv.org/abs/1234.5678",
            Some("Arxiv Paper"),
            "{}",
        )
        .unwrap();
        conn.execute(
            "DELETE FROM context_memberships WHERE item_id = ?1",
            rusqlite::params![item_obs.id],
        )
        .unwrap();

        // 4. Confirmed task candidate
        let cand_conf = crate::db::models::TaskCandidate {
            id: "task-lit-review".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Literature Review".to_string(),
            confidence: 0.95,
            status: crate::db::models::TaskCandidateStatus::Active,
            first_detected_at: "2026-09-13T09:00:00Z".to_string(),
            last_active_at: "2026-09-13T09:30:00Z".to_string(),
            created_at: "2026-09-13T09:00:00Z".to_string(),
            updated_at: "2026-09-13T09:30:00Z".to_string(),
        };
        crate::db::operations::create_task_candidate(&conn, &cand_conf).unwrap();

        let memory = ContextMemory::derive_for_workspace(&conn, &ws.id).unwrap();

        // Context is confirmed (user workspace)
        assert_eq!(memory.context_provenance, InformationProvenance::Confirmed);

        // Check item provenances
        let conf_ref = memory.recent_items.iter().find(|i| i.id == item_conf.id).unwrap();
        assert_eq!(conf_ref.provenance, InformationProvenance::Confirmed);

        let inf_ref = memory.recent_items.iter().find(|i| i.id == item_inf.id).unwrap();
        assert_eq!(inf_ref.provenance, InformationProvenance::Inferred);

        let obs_ref = memory.recent_items.iter().find(|i| i.id == item_obs.id).unwrap();
        assert_eq!(obs_ref.provenance, InformationProvenance::Observed);

        // Check task provenance
        let task_ref = memory.recent_tasks.iter().find(|t| t.id.as_deref() == Some("task-lit-review")).unwrap();
        assert_eq!(task_ref.provenance, InformationProvenance::Confirmed);
    }

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
        assert_eq!(memory.active_task_title(), "Hero animation");
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
        assert_eq!(memory.active_task_title(), "Unknown");
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
        memory.current_task_title = Some("Operating Systems Assignment 3".to_string());
        memory.current_task_confidence = Some(0.88);

        assert_eq!(memory.context_name(), "College");
        assert_eq!(memory.active_task_title(), "Operating Systems Assignment 3");
        assert_eq!(memory.context_identity().unwrap().name, "College");
        assert_eq!(memory.context_identity().unwrap().id, "ws-college-1");
    }

    #[test]
    fn test_decoupled_confidence_metrics() {
        let ctx = ContextReference::workspace("ws-finance", "Personal Finance");
        let memory = ContextMemory::new(Some(ctx), 0.95, "2026-09-13T12:00:00Z").with_task(
            InferredTaskState::new(
                "Tax deduction research",
                Some("Researching".to_string()),
                Some("Tax deduction".to_string()),
                0.40,
                vec!["IRS pdf".to_string()],
                "2026-09-13T12:00:00Z",
                Some("ws-finance".to_string()),
            ),
        );

        assert_eq!(memory.context_confidence(), 0.95);
        assert_eq!(memory.context_confidence_band(), "High");

        assert_eq!(memory.active_task_title(), "Unknown");
        assert!(!memory.has_confident_task());
    }

    #[test]
    fn test_task_history_tracking_on_switch() {
        let ctx = ContextReference::workspace("ws-portfolio", "Portfolio");
        let mut memory = ContextMemory::new(Some(ctx), 0.95, "2026-09-13T14:00:00Z");

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
        memory.current_task_title = Some("Hero animation".to_string());
        memory.current_task_confidence = Some(0.85);

        assert_eq!(memory.active_task_title(), "Hero animation");
        assert_eq!(memory.task_history().len(), 0);

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

        assert_eq!(memory.active_task_title(), "Case study writing");
        assert_eq!(memory.task_history().len(), 1);
        assert_eq!(memory.task_history()[0].title, "Hero animation");
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

        let ctx2 = ContextReference::workspace("ws-college", "College");
        memory.switch_context(ctx2, 0.90, "2026-09-13T11:00:00Z");

        assert_eq!(memory.context_name(), "College");
        assert_eq!(memory.context_confidence(), 0.90);
        assert_eq!(memory.active_task_title(), "Unknown");
        assert!(!memory.has_confident_task());

        assert_eq!(memory.context_history().len(), 1);
        assert_eq!(memory.context_history()[0].reference.name, "Portfolio");

        assert_eq!(memory.task_history().len(), 1);
        assert_eq!(memory.task_history()[0].title, "Hero animation");
    }

    #[test]
    fn test_reconstruction_empty_context() {
        let conn = setup_in_memory_db();
        let ws = crate::db::operations::create_workspace(&conn, "New Workspace").unwrap();

        let memory = ContextMemoryService::new()
            .reconstruct_memory(&conn, &ws.id)
            .unwrap();

        assert_eq!(memory.context_id, ws.id);
        assert_eq!(memory.context_name, "New Workspace");
        assert_eq!(memory.activity_state, ContextMemoryActivityState::InsufficientEvidence);
        assert_eq!(memory.memory_statement, "No recent activity recorded in New Workspace.");
        assert_eq!(memory.active_task_title(), "Unknown");
        assert!(!memory.has_confident_task());
    }

    #[test]
    fn test_reconstruction_aurora_website_hero_animation() {
        let conn = setup_in_memory_db();
        let ws = crate::db::operations::create_workspace(&conn, "Aurora Website").unwrap();

        let fixed_now = chrono::DateTime::parse_from_rfc3339("2026-09-14T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        // 1. Figma hero design
        let i1 = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://figma.com/file/aurora-hero-design",
            Some("Figma hero design"),
            r#"{"app_name":"Figma","domain":"figma.com"}"#,
        ).unwrap();
        conn.execute("UPDATE items SET created_at = '2026-09-14T11:45:00Z' WHERE id = ?1", rusqlite::params![i1.id]).unwrap();

        // 2. React Hero component
        let i2 = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "export const Hero = () => <motion.div animate={{ opacity: 1 }} />",
            Some("React Hero component"),
            r#"{"app_name":"VS Code"}"#,
        ).unwrap();
        conn.execute("UPDATE items SET created_at = '2026-09-14T11:48:00Z' WHERE id = ?1", rusqlite::params![i2.id]).unwrap();

        // 3. Framer Motion animation
        let i3 = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Framer motion animation curve spring transition",
            Some("Framer Motion animation"),
            r#"{"app_name":"VS Code"}"#,
        ).unwrap();
        conn.execute("UPDATE items SET created_at = '2026-09-14T11:50:00Z' WHERE id = ?1", rusqlite::params![i3.id]).unwrap();

        // 4. GitHub repository (generic tool capture)
        let i4 = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://github.com/aurora/website",
            Some("GitHub repository"),
            r#"{"app_name":"Google Chrome","domain":"github.com"}"#,
        ).unwrap();
        conn.execute("UPDATE items SET created_at = '2026-09-14T11:52:00Z' WHERE id = ?1", rusqlite::params![i4.id]).unwrap();

        // Confirmed task candidate: "Hero animation"
        let cand = crate::db::models::TaskCandidate {
            id: "task-hero-anim".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Hero animation".to_string(),
            confidence: 0.90,
            status: crate::db::models::TaskCandidateStatus::Active,
            first_detected_at: "2026-09-14T11:45:00Z".to_string(),
            last_active_at: "2026-09-14T11:52:00Z".to_string(),
            created_at: "2026-09-14T11:45:00Z".to_string(),
            updated_at: "2026-09-14T11:52:00Z".to_string(),
        };
        crate::db::operations::create_task_candidate(&conn, &cand).unwrap();

        let memory = ContextMemoryBuilder::from_db(&conn, &ws.id)
            .unwrap()
            .with_now(fixed_now)
            .build()
            .unwrap();

        assert_eq!(memory.activity_state, ContextMemoryActivityState::Active);
        assert_eq!(memory.active_task_title(), "Hero animation");
        assert_eq!(memory.memory_statement, "You are working on Hero animation.");
        assert!(!memory.memory_statement.contains("GitHub"));
    }

    #[test]
    fn test_reconstruction_weak_evidence_suppresses_generic_task() {
        let conn = setup_in_memory_db();
        let ws = crate::db::operations::create_workspace(&conn, "Aurora Website").unwrap();

        let fixed_now = chrono::DateTime::parse_from_rfc3339("2026-09-14T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        // Only 1 item: GitHub repository (generic tool without specific task)
        let i = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://github.com/aurora/website",
            Some("GitHub repository"),
            r#"{"app_name":"Google Chrome","domain":"github.com"}"#,
        ).unwrap();
        conn.execute("UPDATE items SET created_at = '2026-09-14T06:00:00Z' WHERE id = ?1", rusqlite::params![i.id]).unwrap();

        let memory = ContextMemoryBuilder::from_db(&conn, &ws.id)
            .unwrap()
            .with_now(fixed_now)
            .build()
            .unwrap();

        assert_eq!(memory.activity_state, ContextMemoryActivityState::RecentlyActive);
        assert_eq!(memory.active_task_title(), "Unknown");
        assert!(!memory.has_confident_task());
        // Must say "You were recently active in Aurora Website.", NOT "You were working on GitHub."
        assert_eq!(memory.memory_statement, "You were recently active in Aurora Website.");
        assert!(!memory.memory_statement.to_lowercase().contains("github"));
    }

    #[test]
    fn test_reconstruction_dormant_state() {
        let conn = setup_in_memory_db();
        let ws = crate::db::operations::create_workspace(&conn, "Legacy Backend").unwrap();
        crate::db::operations::update_workspace_lifecycle_state(&conn, &ws.id, "dormant").unwrap();

        let cand = crate::db::models::TaskCandidate {
            id: "task-db-mig".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Database migration".to_string(),
            confidence: 0.85,
            status: crate::db::models::TaskCandidateStatus::Active,
            first_detected_at: "2026-09-01T10:00:00Z".to_string(),
            last_active_at: "2026-09-01T12:00:00Z".to_string(),
            created_at: "2026-09-01T10:00:00Z".to_string(),
            updated_at: "2026-09-01T12:00:00Z".to_string(),
        };
        crate::db::operations::create_task_candidate(&conn, &cand).unwrap();

        let memory = ContextMemoryBuilder::from_db(&conn, &ws.id)
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(memory.activity_state, ContextMemoryActivityState::Dormant);
        assert!(memory.memory_statement.contains("dormant"));
        assert!(memory.memory_statement.contains("Database migration"));
    }

    #[test]
    fn test_reconstruction_recently_active_state() {
        let conn = setup_in_memory_db();
        let ws = crate::db::operations::create_workspace(&conn, "Mobile App").unwrap();

        let fixed_now = chrono::DateTime::parse_from_rfc3339("2026-09-14T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let cand = crate::db::models::TaskCandidate {
            id: "task-push".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Implementing push notifications".to_string(),
            confidence: 0.88,
            status: crate::db::models::TaskCandidateStatus::Active,
            first_detected_at: "2026-09-14T07:00:00Z".to_string(),
            last_active_at: "2026-09-14T07:30:00Z".to_string(),
            created_at: "2026-09-14T07:00:00Z".to_string(),
            updated_at: "2026-09-14T07:30:00Z".to_string(),
        };
        crate::db::operations::create_task_candidate(&conn, &cand).unwrap();

        let memory = ContextMemoryBuilder::from_db(&conn, &ws.id)
            .unwrap()
            .with_now(fixed_now)
            .build()
            .unwrap();

        assert_eq!(memory.activity_state, ContextMemoryActivityState::RecentlyActive);
        assert_eq!(memory.memory_statement, "You were working on Implementing push notifications.");
    }

    #[test]
    fn test_reconstruction_confirmed_task_priority() {
        let conn = setup_in_memory_db();
        let ws = crate::db::operations::create_workspace(&conn, "DevOps").unwrap();

        // Confirmed active task
        let cand = crate::db::models::TaskCandidate {
            id: "task-k8s".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Kubernetes cluster migration".to_string(),
            confidence: 0.95,
            status: crate::db::models::TaskCandidateStatus::Active,
            first_detected_at: "2026-09-14T11:00:00Z".to_string(),
            last_active_at: "2026-09-14T11:30:00Z".to_string(),
            created_at: "2026-09-14T11:00:00Z".to_string(),
            updated_at: "2026-09-14T11:30:00Z".to_string(),
        };
        crate::db::operations::create_task_candidate(&conn, &cand).unwrap();

        // New unconfirmed capture item
        let _item = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Drafting weekly team email update",
            Some("Weekly Email"),
            "{}",
        ).unwrap();

        let memory = ContextMemoryBuilder::from_db(&conn, &ws.id)
            .unwrap()
            .build()
            .unwrap();

        // Must prioritize confirmed task
        assert_eq!(memory.active_task_title(), "Kubernetes cluster migration");
        assert_eq!(memory.current_task_provenance, Some(InformationProvenance::Confirmed));
    }

    #[test]
    fn test_reconstruction_activity_segments_and_transitions() {
        let conn = setup_in_memory_db();
        let ws = crate::db::operations::create_workspace(&conn, "Analytics").unwrap();

        let t_switch = DetectedTaskTransition {
            id: "trans-123".to_string(),
            transition_type: TaskTransitionType::TaskSwitch,
            from_task: Some("Old Task".to_string()),
            to_task: Some("New Task".to_string()),
            confidence: 0.90,
            timestamp: "2026-09-14T11:00:00Z".to_string(),
            context_id: Some(ws.id.clone()),
            evidence: vec!["log.txt".to_string()],
            trigger_item_id: None,
            duration_seconds: None,
            metadata: serde_json::json!({}),
        };

        let builder = ContextMemoryBuilder::new(&ws.id)
            .with_workspace(ws)
            .with_task_transitions(vec![t_switch]);

        let memory = builder.build().unwrap();
        assert!(memory.activity_sequence.iter().any(|e| e.entry_type == "task_transition"));
    }

    #[test]
    fn test_deterministic_reconstruction_with_frozen_clock() {
        let conn = setup_in_memory_db();
        let ws = crate::db::operations::create_workspace(&conn, "Deterministic Lab").unwrap();

        let fixed_now = chrono::DateTime::parse_from_rfc3339("2026-09-14T15:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let m1 = ContextMemoryBuilder::from_db(&conn, &ws.id)
            .unwrap()
            .with_now(fixed_now)
            .build()
            .unwrap();

        let m2 = ContextMemoryBuilder::from_db(&conn, &ws.id)
            .unwrap()
            .with_now(fixed_now)
            .build()
            .unwrap();

        assert_eq!(m1, m2);
    }
}

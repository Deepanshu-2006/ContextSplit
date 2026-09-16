use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::context_engine::context_state::{
    ContextState, ContextStateValue, MeaningfulActivity, ResumeReadiness,
    ResumeReadinessAssessment, ResumeReadinessEvaluator, StateFreshness,
};
use crate::context_engine::memory::{
    ContextMemory, InformationProvenance, MemoryActivitySequenceEntry, MemoryTaskReference,
};
use crate::context_engine::workset::{
    ContextWorkset, WorksetItemReference, WorksetItemRelevance,
};

// =========================================================================
// Standard Epistemic Disclaimer
// =========================================================================

pub const CONTEXTUAL_SNAPSHOT_DISCLAIMER: &str =
    "This snapshot is an algorithmic contextual reconstruction based on recorded activity signals. It does not represent an exact recording of user attention or mental state.";

// =========================================================================
// Lightweight Snapshot References (No Full Objects / No Duplication)
// =========================================================================

/// Lightweight reference to an item within a contextual snapshot.
///
/// Invariant: Does NOT store full document bodies, heavy payloads, or vector embeddings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotWorksetItemReference {
    /// Unique item identifier
    pub id: String,
    /// Human-friendly title or label
    pub title: Option<String>,
    /// Item type (e.g. "url", "text", "file", "image")
    pub item_type: String,
    /// Functional relevance classification within the snapshot
    pub relevance: WorksetItemRelevance,
    /// Composite normalized relevance score [0.0, 1.0]
    pub relevance_score: f64,
    /// Epistemic source (Observed, Inferred, or Confirmed)
    pub provenance: InformationProvenance,
    /// Associated foreground application name if known
    pub app_name: Option<String>,
    /// Associated web domain if known
    pub domain: Option<String>,
    /// Concise content snippet for quick preview
    pub content_snippet: String,
    /// Explainable reasons for inclusion in workset
    pub reasons: Vec<String>,
    /// ISO-8601 timestamp of item capture
    pub created_at: String,
    /// ISO-8601 timestamp of last known activity involving this item
    pub last_active_at: Option<String>,
}

impl From<&WorksetItemReference> for SnapshotWorksetItemReference {
    fn from(item: &WorksetItemReference) -> Self {
        Self {
            id: item.id.clone(),
            title: item.title.clone(),
            item_type: item.item_type.clone(),
            relevance: item.relevance,
            relevance_score: item.relevance_score,
            provenance: item.provenance,
            app_name: item.app_name.clone(),
            domain: item.domain.clone(),
            content_snippet: item.content_snippet.clone(),
            reasons: item.reasons.clone(),
            created_at: item.created_at.clone(),
            last_active_at: item.last_active_at.clone(),
        }
    }
}

/// Lightweight reference to a task in a contextual snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotTaskReference {
    /// Task candidate identifier if tracked in database
    pub id: Option<String>,
    /// Task title (e.g. "Hero animation", "Recursion Assignment")
    pub title: String,
    /// Extracted action verb (e.g. "Designing", "Coding")
    pub action_verb: Option<String>,
    /// Target subject/entity (e.g. "Hero animation", "Assignment")
    pub target_subject: Option<String>,
    /// Calibrated numeric task confidence [0.0, 1.0]
    pub confidence: f64,
    /// Qualitative confidence band ("High", "Medium", "Low", "Insufficient")
    pub confidence_band: String,
    /// Epistemic source (Inferred or Confirmed)
    pub provenance: InformationProvenance,
    /// Timestamp when first observed
    pub first_seen_at: Option<String>,
    /// Timestamp of most recent activity
    pub last_active_at: Option<String>,
    /// Inferred or confirmed status
    pub status: Option<String>,
}

impl From<&MemoryTaskReference> for SnapshotTaskReference {
    fn from(task: &MemoryTaskReference) -> Self {
        Self {
            id: task.id.clone(),
            title: task.title.clone(),
            action_verb: task.action_verb.clone(),
            target_subject: task.target_subject.clone(),
            confidence: task.confidence,
            confidence_band: task.confidence_band.clone(),
            provenance: task.provenance,
            first_seen_at: task.first_seen_at.clone(),
            last_active_at: task.last_active_at.clone(),
            status: task.status.clone(),
        }
    }
}

/// Lightweight reference to a chronological activity milestone in a contextual snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotActivityReference {
    /// Relevant entity reference ID (item_id, transition_id, segment_id)
    pub id: Option<String>,
    /// Activity entry category: "item_captured", "context_transition", "task_transition", "session_burst", "user_confirmation"
    pub entry_type: String,
    /// Concise human-readable description of the activity milestone
    pub description: String,
    /// ISO-8601 timestamp of the activity event
    pub timestamp: String,
    /// Epistemic source (Observed, Inferred, Confirmed)
    pub provenance: InformationProvenance,
}

impl From<&MemoryActivitySequenceEntry> for SnapshotActivityReference {
    fn from(entry: &MemoryActivitySequenceEntry) -> Self {
        Self {
            id: entry.reference_id.clone(),
            entry_type: entry.entry_type.clone(),
            description: entry.description.clone(),
            timestamp: entry.timestamp.clone(),
            provenance: entry.provenance,
        }
    }
}

// =========================================================================
// ContextSnapshot: Point-in-Time Contextual Reconstruction
// =========================================================================

/// Point-in-time contextual snapshot representing what a user's work looked like
/// inside a context at the time of departure or checkpoint.
///
/// Invariants:
/// 1. Purely a contextual reconstruction; never claims exact attention or mental focus.
/// 2. Does NOT duplicate full Item or Task bodies; uses lightweight references.
/// 3. Clearly differentiates `Observed`, `Inferred`, and `Confirmed` data.
/// 4. If evidence is insufficient, leaves task nullable and readiness marked as `Insufficient`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextSnapshot {
    /// Unique snapshot identifier (UUID)
    pub id: String,
    /// Context / Workspace identifier
    pub context_id: String,
    /// Context display name (e.g. "Aurora Website", "University CS")
    pub context_name: String,
    /// Inferred or confirmed active task ID at departure (nullable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// Inferred or confirmed active task title at departure (nullable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_title: Option<String>,
    /// Epistemic provenance of the task (Observed, Inferred, Confirmed)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_provenance: Option<InformationProvenance>,
    /// Decoupled confidence score for the task [0.0, 1.0] (nullable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_confidence: Option<f64>,
    /// ISO-8601 timestamp when this snapshot was created/captured
    pub created_at: String,
    /// ISO-8601 timestamp of the most recent recorded activity in this context (nullable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity_at: Option<String>,
    /// Conservative context working state (Active, RecentlyActive, Paused, Dormant, Uncertain, InsufficientEvidence)
    pub state: ContextStateValue,
    /// State freshness classification (Fresh, Recent, Stale, VeryStale)
    pub state_freshness: StateFreshness,
    /// Defensible relative display wording (e.g. "Active recently", "Last active 2 days ago")
    pub state_freshness_display: String,
    /// Internal resume readiness assessment and human-readable reasons
    pub resume_readiness: ResumeReadinessAssessment,
    /// Lightweight references to the high-signal workset items at snapshot time
    pub workset_item_references: Vec<SnapshotWorksetItemReference>,
    /// Lightweight references to chronological activity sequence milestones
    pub recent_activity_references: Vec<SnapshotActivityReference>,
    /// Lightweight references to recent tasks in this context
    pub recent_task_references: Vec<SnapshotTaskReference>,
    /// Structured representation of the last meaningful activity (nullable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_meaningful_activity: Option<MeaningfulActivity>,
    /// Calibrated snapshot confidence score [0.0, 1.0]
    pub snapshot_confidence: f64,
    /// Concise, natural reconstruction answering: "What did my work look like when I last left this context?"
    pub snapshot_statement: String,
    /// Primary epistemic source of this snapshot
    pub provenance: InformationProvenance,
    /// Explicit non-mental-state disclaimer
    pub epistemic_disclaimer: String,
}

impl ContextSnapshot {
    /// Creates a new `ContextSnapshot` with full explicit fields.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        context_id: impl Into<String>,
        context_name: impl Into<String>,
        task_id: Option<String>,
        task_title: Option<String>,
        task_provenance: Option<InformationProvenance>,
        task_confidence: Option<f64>,
        last_activity_at: Option<String>,
        state: ContextStateValue,
        state_freshness: StateFreshness,
        state_freshness_display: impl Into<String>,
        resume_readiness: ResumeReadinessAssessment,
        workset_item_references: Vec<SnapshotWorksetItemReference>,
        recent_activity_references: Vec<SnapshotActivityReference>,
        recent_task_references: Vec<SnapshotTaskReference>,
        last_meaningful_activity: Option<MeaningfulActivity>,
        snapshot_confidence: f64,
        provenance: InformationProvenance,
    ) -> Self {
        let now = Utc::now().to_rfc3339();
        let ctx_name_str = context_name.into();
        let ctx_id_str = context_id.into();

        let statement = Self::generate_snapshot_statement(
            &ctx_name_str,
            task_title.as_deref(),
            last_meaningful_activity.as_ref(),
            &workset_item_references,
            state,
            state_freshness,
        );

        Self {
            id: Uuid::new_v4().to_string(),
            context_id: ctx_id_str,
            context_name: ctx_name_str,
            task_id,
            task_title,
            task_provenance,
            task_confidence: task_confidence.map(|c| c.clamp(0.0, 1.0)),
            created_at: now,
            last_activity_at,
            state,
            state_freshness,
            state_freshness_display: state_freshness_display.into(),
            resume_readiness,
            workset_item_references,
            recent_activity_references,
            recent_task_references,
            last_meaningful_activity,
            snapshot_confidence: snapshot_confidence.clamp(0.0, 1.0),
            snapshot_statement: statement,
            provenance,
            epistemic_disclaimer: CONTEXTUAL_SNAPSHOT_DISCLAIMER.to_string(),
        }
    }

    /// Reconstructs a `ContextSnapshot` directly from a derived `ContextState`.
    pub fn from_context_state(state: &ContextState) -> Self {
        let workset_refs: Vec<SnapshotWorksetItemReference> = state
            .current_workset
            .as_ref()
            .map(|ws| ws.items.iter().map(SnapshotWorksetItemReference::from).collect())
            .unwrap_or_default();

        let task_prov = if state.has_user_feedback {
            Some(InformationProvenance::Confirmed)
        } else if state.task_title.is_some() {
            Some(InformationProvenance::Inferred)
        } else {
            None
        };

        let prov = if state.has_user_feedback {
            InformationProvenance::Confirmed
        } else if state.state_confidence >= 0.75 {
            InformationProvenance::Observed
        } else {
            InformationProvenance::Inferred
        };

        let recent_tasks: Vec<SnapshotTaskReference> = state
            .current_workset
            .as_ref()
            .map(|ws| {
                ws.related_tasks
                    .iter()
                    .map(SnapshotTaskReference::from)
                    .collect()
            })
            .unwrap_or_default();

        let statement = if !state.resume_state.formatted_resume_summary.is_empty() {
            state.resume_state.formatted_resume_summary.clone()
        } else {
            Self::generate_snapshot_statement(
                state.context_name.as_deref().unwrap_or("this workspace"),
                state.task_title.as_deref(),
                state.meaningful_activity.as_ref(),
                &workset_refs,
                state.state,
                state.freshness,
            )
        };

        Self {
            id: Uuid::new_v4().to_string(),
            context_id: state.context_id.clone(),
            context_name: state
                .context_name
                .clone()
                .unwrap_or_else(|| "Workspace".to_string()),
            task_id: state.task_id.clone(),
            task_title: state.task_title.clone(),
            task_provenance: task_prov,
            task_confidence: if state.task_title.is_some() {
                Some(state.state_confidence)
            } else {
                None
            },
            created_at: Utc::now().to_rfc3339(),
            last_activity_at: state.last_active_at.clone(),
            state: state.state,
            state_freshness: state.freshness,
            state_freshness_display: state.freshness_display.clone(),
            resume_readiness: state.readiness.clone(),
            workset_item_references: workset_refs,
            recent_activity_references: Vec::new(),
            recent_task_references: recent_tasks,
            last_meaningful_activity: state.meaningful_activity.clone(),
            snapshot_confidence: state.state_confidence,
            snapshot_statement: statement,
            provenance: prov,
            epistemic_disclaimer: CONTEXTUAL_SNAPSHOT_DISCLAIMER.to_string(),
        }
    }

    /// Reconstructs a `ContextSnapshot` by fusing `ContextMemory`, `ContextWorkset`, and `ContextResumeState`.
    pub fn from_memory_and_workset(
        memory: &ContextMemory,
        workset: Option<&ContextWorkset>,
        meaningful_activity: Option<MeaningfulActivity>,
    ) -> Self {
        let workset_refs: Vec<SnapshotWorksetItemReference> = workset
            .map(|ws| ws.items.iter().map(SnapshotWorksetItemReference::from).collect())
            .unwrap_or_default();

        let activity_refs: Vec<SnapshotActivityReference> = memory
            .activity_sequence
            .iter()
            .map(SnapshotActivityReference::from)
            .collect();

        let task_refs: Vec<SnapshotTaskReference> = memory
            .recent_tasks
            .iter()
            .map(SnapshotTaskReference::from)
            .collect();

        let state_val = match memory.activity_state {
            crate::context_engine::memory::ContextMemoryActivityState::Active => {
                ContextStateValue::Active
            }
            crate::context_engine::memory::ContextMemoryActivityState::RecentlyActive => {
                ContextStateValue::RecentlyActive
            }
            crate::context_engine::memory::ContextMemoryActivityState::Dormant => {
                ContextStateValue::Dormant
            }
            crate::context_engine::memory::ContextMemoryActivityState::InsufficientEvidence => {
                ContextStateValue::InsufficientEvidence
            }
        };

        let now = Utc::now();
        let elapsed_secs = memory
            .last_active_at
            .as_deref()
            .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
            .map(|dt| (now - dt.with_timezone(&Utc)).num_seconds().max(0))
            .unwrap_or(86400 * 14);

        let freshness = StateFreshness::classify(elapsed_secs);
        let freshness_display = freshness.display_wording(elapsed_secs);

        let active_task_title = memory
            .current_task_title
            .clone()
            .or_else(|| workset.and_then(|ws| ws.task_title.clone()));

        let active_task_id = memory
            .current_task_id
            .clone()
            .or_else(|| workset.and_then(|ws| ws.task_id.clone()));

        let readiness = ResumeReadinessEvaluator::evaluate_with_freshness(
            state_val,
            active_task_title.as_deref(),
            workset_refs.len(),
            meaningful_activity.is_some(),
            !activity_refs.is_empty(),
            false,
            freshness,
        );

        let statement = Self::generate_snapshot_statement(
            &memory.context_name,
            active_task_title.as_deref(),
            meaningful_activity.as_ref(),
            &workset_refs,
            state_val,
            freshness,
        );

        Self {
            id: Uuid::new_v4().to_string(),
            context_id: memory.context_id.clone(),
            context_name: memory.context_name.clone(),
            task_id: active_task_id,
            task_title: active_task_title,
            task_provenance: memory.current_task_provenance,
            task_confidence: memory.current_task_confidence,
            created_at: Utc::now().to_rfc3339(),
            last_activity_at: memory.last_active_at.clone(),
            state: state_val,
            state_freshness: freshness,
            state_freshness_display: freshness_display,
            resume_readiness: readiness,
            workset_item_references: workset_refs,
            recent_activity_references: activity_refs,
            recent_task_references: task_refs,
            last_meaningful_activity: meaningful_activity,
            snapshot_confidence: memory.current_context_confidence,
            snapshot_statement: statement,
            provenance: memory.context_provenance,
            epistemic_disclaimer: CONTEXTUAL_SNAPSHOT_DISCLAIMER.to_string(),
        }
    }

    /// Generates a concise natural language statement answering:
    /// "What did my work look like when I last left this context?"
    pub fn generate_snapshot_statement(
        context_name: &str,
        task_title: Option<&str>,
        last_meaningful_activity: Option<&MeaningfulActivity>,
        workset_items: &[SnapshotWorksetItemReference],
        state: ContextStateValue,
        freshness: StateFreshness,
    ) -> String {
        if state == ContextStateValue::InsufficientEvidence || (workset_items.is_empty() && task_title.is_none() && last_meaningful_activity.is_none()) {
            return format!("No prior working activity recorded in {}.", context_name);
        }

        let mut parts = Vec::new();

        // 1. Task or Context focus
        if let Some(task) = task_title {
            parts.push(format!("Left off on task '{}'", task));
        } else {
            parts.push(format!("Working in {}", context_name));
        }

        // 2. Last meaningful activity or primary item
        if let Some(act) = last_meaningful_activity {
            parts.push(format!("last meaningful activity was '{}'", act.description));
        } else if let Some(primary) = workset_items.iter().find(|i| i.relevance == WorksetItemRelevance::Primary) {
            let item_name = primary.title.as_deref().unwrap_or("active document");
            if let Some(ref app) = primary.app_name {
                parts.push(format!("last focused on '{}' in {}", item_name, app));
            } else {
                parts.push(format!("last focused on '{}'", item_name));
            }
        }

        // 3. Supporting materials count
        let supporting_count = workset_items.iter().filter(|i| i.relevance == WorksetItemRelevance::Primary || i.relevance == WorksetItemRelevance::Supporting).count();
        if supporting_count > 0 {
            if supporting_count == 1 {
                parts.push("with 1 core item ready".to_string());
            } else {
                parts.push(format!("with {} core items ready", supporting_count));
            }
        }

        // 4. Relative time indication
        let time_note = match freshness {
            StateFreshness::Fresh => "recently",
            StateFreshness::Recent => "yesterday",
            StateFreshness::Stale => "several days ago",
            StateFreshness::VeryStale => "over a week ago",
        };
        parts.push(format!("({})", time_note));

        parts.join(", ")
    }

    /// Returns `true` if this snapshot indicates active or warm working focus.
    pub fn is_working(&self) -> bool {
        matches!(self.state, ContextStateValue::Active | ContextStateValue::RecentlyActive)
    }

    /// Returns `true` if this snapshot is marked dormant.
    pub fn is_dormant(&self) -> bool {
        self.state == ContextStateValue::Dormant || self.state_freshness == StateFreshness::VeryStale
    }

    /// Returns `true` if a specific task was present at snapshot time.
    pub fn has_task(&self) -> bool {
        self.task_title.is_some()
    }

    /// Returns `true` if resume readiness is Ready or MostlyReady.
    pub fn is_ready_to_resume(&self) -> bool {
        matches!(
            self.resume_readiness.readiness,
            ResumeReadiness::Ready | ResumeReadiness::MostlyReady
        )
    }

    /// Returns the primary workset items.
    pub fn primary_items(&self) -> Vec<&SnapshotWorksetItemReference> {
        self.workset_item_references
            .iter()
            .filter(|i| i.relevance == WorksetItemRelevance::Primary)
            .collect()
    }

    /// Returns the supporting workset items.
    pub fn supporting_items(&self) -> Vec<&SnapshotWorksetItemReference> {
        self.workset_item_references
            .iter()
            .filter(|i| i.relevance == WorksetItemRelevance::Supporting)
            .collect()
    }

    /// Returns all core materials (Primary + Supporting).
    pub fn core_items(&self) -> Vec<&SnapshotWorksetItemReference> {
        self.workset_item_references
            .iter()
            .filter(|i| {
                matches!(
                    i.relevance,
                    WorksetItemRelevance::Primary | WorksetItemRelevance::Supporting
                )
            })
            .collect()
    }
}

// =========================================================================
// ContextSnapshotBuilder: Fluent Construction Engine
// =========================================================================

/// Builder for constructing `ContextSnapshot` instances with customizable inputs.
#[derive(Debug, Clone)]
pub struct ContextSnapshotBuilder {
    pub context_id: String,
    pub context_name: String,
    pub task_id: Option<String>,
    pub task_title: Option<String>,
    pub task_provenance: Option<InformationProvenance>,
    pub task_confidence: Option<f64>,
    pub last_activity_at: Option<String>,
    pub state: ContextStateValue,
    pub state_freshness: StateFreshness,
    pub state_freshness_display: String,
    pub resume_readiness: Option<ResumeReadinessAssessment>,
    pub workset_item_references: Vec<SnapshotWorksetItemReference>,
    pub recent_activity_references: Vec<SnapshotActivityReference>,
    pub recent_task_references: Vec<SnapshotTaskReference>,
    pub last_meaningful_activity: Option<MeaningfulActivity>,
    pub snapshot_confidence: f64,
    pub provenance: InformationProvenance,
}

impl ContextSnapshotBuilder {
    pub fn new(context_id: impl Into<String>, context_name: impl Into<String>) -> Self {
        Self {
            context_id: context_id.into(),
            context_name: context_name.into(),
            task_id: None,
            task_title: None,
            task_provenance: None,
            task_confidence: None,
            last_activity_at: None,
            state: ContextStateValue::InsufficientEvidence,
            state_freshness: StateFreshness::Fresh,
            state_freshness_display: "Active recently".to_string(),
            resume_readiness: None,
            workset_item_references: Vec::new(),
            recent_activity_references: Vec::new(),
            recent_task_references: Vec::new(),
            last_meaningful_activity: None,
            snapshot_confidence: 0.50,
            provenance: InformationProvenance::Inferred,
        }
    }

    pub fn with_task(
        mut self,
        task_id: Option<String>,
        task_title: impl Into<String>,
        confidence: f64,
        provenance: InformationProvenance,
    ) -> Self {
        self.task_id = task_id;
        self.task_title = Some(task_title.into());
        self.task_confidence = Some(confidence.clamp(0.0, 1.0));
        self.task_provenance = Some(provenance);
        self
    }

    pub fn without_task(mut self) -> Self {
        self.task_id = None;
        self.task_title = None;
        self.task_confidence = None;
        self.task_provenance = None;
        self
    }

    pub fn with_state(mut self, state: ContextStateValue) -> Self {
        self.state = state;
        self
    }

    pub fn with_freshness(mut self, freshness: StateFreshness, elapsed_seconds: i64) -> Self {
        self.state_freshness = freshness;
        self.state_freshness_display = freshness.display_wording(elapsed_seconds);
        self
    }

    pub fn with_last_activity_at(mut self, timestamp: impl Into<String>) -> Self {
        self.last_activity_at = Some(timestamp.into());
        self
    }

    pub fn with_workset_items(mut self, items: Vec<SnapshotWorksetItemReference>) -> Self {
        self.workset_item_references = items;
        self
    }

    pub fn add_workset_item(mut self, item: SnapshotWorksetItemReference) -> Self {
        self.workset_item_references.push(item);
        self
    }

    pub fn with_activity_references(mut self, activities: Vec<SnapshotActivityReference>) -> Self {
        self.recent_activity_references = activities;
        self
    }

    pub fn with_task_references(mut self, tasks: Vec<SnapshotTaskReference>) -> Self {
        self.recent_task_references = tasks;
        self
    }

    pub fn with_meaningful_activity(mut self, activity: MeaningfulActivity) -> Self {
        self.last_meaningful_activity = Some(activity);
        self
    }

    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.snapshot_confidence = confidence.clamp(0.0, 1.0);
        self
    }

    pub fn with_provenance(mut self, provenance: InformationProvenance) -> Self {
        self.provenance = provenance;
        self
    }

    pub fn build(self) -> ContextSnapshot {
        let has_recent_activity = !self.recent_activity_references.is_empty()
            || self.state == ContextStateValue::Active
            || self.state == ContextStateValue::RecentlyActive
            || self.state_freshness == StateFreshness::Fresh;

        let readiness = self.resume_readiness.unwrap_or_else(|| {
            ResumeReadinessEvaluator::evaluate_with_freshness(
                self.state,
                self.task_title.as_deref(),
                self.workset_item_references.len(),
                self.last_meaningful_activity.is_some(),
                has_recent_activity,
                self.provenance == InformationProvenance::Confirmed,
                self.state_freshness,
            )
        });

        ContextSnapshot::new(
            self.context_id,
            self.context_name,
            self.task_id,
            self.task_title,
            self.task_provenance,
            self.task_confidence,
            self.last_activity_at,
            self.state,
            self.state_freshness,
            self.state_freshness_display,
            readiness,
            self.workset_item_references,
            self.recent_activity_references,
            self.recent_task_references,
            self.last_meaningful_activity,
            self.snapshot_confidence,
            self.provenance,
        )
    }
}

// =========================================================================
// Unit Tests (Day 20 Step 2)
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_dummy_workset_item(
        id: &str,
        title: &str,
        relevance: WorksetItemRelevance,
        score: f64,
        app: Option<&str>,
    ) -> SnapshotWorksetItemReference {
        SnapshotWorksetItemReference {
            id: id.to_string(),
            title: Some(title.to_string()),
            item_type: "url".to_string(),
            relevance,
            relevance_score: score,
            provenance: InformationProvenance::Observed,
            app_name: app.map(|a| a.to_string()),
            domain: Some("figma.com".to_string()),
            content_snippet: format!("https://figma.com/file/{}", id),
            reasons: vec!["frequently accessed during task".to_string()],
            created_at: "2026-09-17T00:00:00Z".to_string(),
            last_active_at: Some("2026-09-17T00:30:00Z".to_string()),
        }
    }

    #[test]
    fn test_active_context_snapshot() {
        let items = vec![
            create_dummy_workset_item("i1", "Hero Component", WorksetItemRelevance::Primary, 0.95, Some("VS Code")),
            create_dummy_workset_item("i2", "Framer Motion API", WorksetItemRelevance::Supporting, 0.85, Some("Chrome")),
        ];

        let snapshot = ContextSnapshotBuilder::new("ws-aurora", "Aurora Website")
            .with_state(ContextStateValue::Active)
            .with_freshness(StateFreshness::Fresh, 300)
            .with_task(Some("t1".to_string()), "Hero Animation", 0.92, InformationProvenance::Inferred)
            .with_workset_items(items)
            .with_confidence(0.90)
            .build();

        assert_eq!(snapshot.state, ContextStateValue::Active);
        assert_eq!(snapshot.state_freshness, StateFreshness::Fresh);
        assert!(snapshot.is_working());
        assert!(!snapshot.is_dormant());
        assert!(snapshot.has_task());
        assert_eq!(snapshot.task_title.as_deref(), Some("Hero Animation"));
        assert_eq!(snapshot.task_provenance, Some(InformationProvenance::Inferred));
        assert_eq!(snapshot.resume_readiness.readiness, ResumeReadiness::Ready);
        assert_eq!(snapshot.primary_items().len(), 1);
        assert_eq!(snapshot.supporting_items().len(), 1);
        assert_eq!(snapshot.core_items().len(), 2);
        assert!(snapshot.snapshot_statement.contains("Hero Animation"));
        assert_eq!(snapshot.epistemic_disclaimer, CONTEXTUAL_SNAPSHOT_DISCLAIMER);
    }

    #[test]
    fn test_recently_active_context_snapshot() {
        let items = vec![
            create_dummy_workset_item("i1", "Dashboard Layout", WorksetItemRelevance::Primary, 0.90, Some("Figma")),
            create_dummy_workset_item("i2", "Tailwind Config", WorksetItemRelevance::Supporting, 0.80, Some("VS Code")),
        ];

        let snapshot = ContextSnapshotBuilder::new("ws-dashboard", "Analytics Dashboard")
            .with_state(ContextStateValue::RecentlyActive)
            .with_freshness(StateFreshness::Recent, 14400) // 4 hours ago
            .with_task(Some("t2".to_string()), "Refactoring Dashboard", 0.88, InformationProvenance::Inferred)
            .with_workset_items(items)
            .with_confidence(0.85)
            .build();

        assert_eq!(snapshot.state, ContextStateValue::RecentlyActive);
        assert_eq!(snapshot.state_freshness, StateFreshness::Recent);
        assert!(snapshot.is_working());
        assert!(!snapshot.is_dormant());
        assert_eq!(snapshot.resume_readiness.readiness, ResumeReadiness::Ready);
        assert!(snapshot.is_ready_to_resume());
    }

    #[test]
    fn test_dormant_context_snapshot() {
        let items = vec![
            create_dummy_workset_item("i1", "Legacy Specs", WorksetItemRelevance::Supporting, 0.60, Some("Chrome")),
        ];

        let snapshot = ContextSnapshotBuilder::new("ws-old", "Legacy Project")
            .with_state(ContextStateValue::Dormant)
            .with_freshness(StateFreshness::VeryStale, 86400 * 10) // 10 days ago
            .with_task(Some("t3".to_string()), "Documentation", 0.70, InformationProvenance::Inferred)
            .with_workset_items(items)
            .with_confidence(0.60)
            .build();

        assert_eq!(snapshot.state, ContextStateValue::Dormant);
        assert_eq!(snapshot.state_freshness, StateFreshness::VeryStale);
        assert!(snapshot.is_dormant());
        assert!(!snapshot.is_working());
        assert_eq!(snapshot.resume_readiness.readiness, ResumeReadiness::Partial);
    }

    #[test]
    fn test_task_present_snapshot() {
        let snapshot = ContextSnapshotBuilder::new("ws-1", "Project X")
            .with_state(ContextStateValue::RecentlyActive)
            .with_freshness(StateFreshness::Fresh, 1800)
            .with_task(Some("t-x".to_string()), "Authentication Flow", 0.95, InformationProvenance::Confirmed)
            .add_workset_item(create_dummy_workset_item("i1", "OAuth Spec", WorksetItemRelevance::Primary, 0.90, None))
            .add_workset_item(create_dummy_workset_item("i2", "Token Handler", WorksetItemRelevance::Supporting, 0.88, None))
            .build();

        assert!(snapshot.has_task());
        assert_eq!(snapshot.task_title.as_deref(), Some("Authentication Flow"));
        assert_eq!(snapshot.task_provenance, Some(InformationProvenance::Confirmed));
        assert!(snapshot.snapshot_statement.contains("Authentication Flow"));
    }

    #[test]
    fn test_task_absent_snapshot() {
        let snapshot = ContextSnapshotBuilder::new("ws-2", "General Exploration")
            .with_state(ContextStateValue::RecentlyActive)
            .with_freshness(StateFreshness::Fresh, 2400)
            .without_task()
            .add_workset_item(create_dummy_workset_item("i1", "Article 1", WorksetItemRelevance::Supporting, 0.70, None))
            .add_workset_item(create_dummy_workset_item("i2", "Article 2", WorksetItemRelevance::Supporting, 0.65, None))
            .add_workset_item(create_dummy_workset_item("i3", "Article 3", WorksetItemRelevance::Supporting, 0.60, None))
            .build();

        assert!(!snapshot.has_task());
        assert_eq!(snapshot.task_title, None);
        assert_eq!(snapshot.task_provenance, None);
        assert_eq!(snapshot.resume_readiness.readiness, ResumeReadiness::MostlyReady);
        assert!(snapshot.snapshot_statement.contains("Working in General Exploration"));
    }

    #[test]
    fn test_strong_workset_snapshot() {
        let items = vec![
            create_dummy_workset_item("i1", "Core API", WorksetItemRelevance::Primary, 0.98, Some("VS Code")),
            create_dummy_workset_item("i2", "Test Suite", WorksetItemRelevance::Primary, 0.92, Some("Terminal")),
            create_dummy_workset_item("i3", "Documentation", WorksetItemRelevance::Supporting, 0.85, Some("Chrome")),
            create_dummy_workset_item("i4", "Design Mock", WorksetItemRelevance::Supporting, 0.80, Some("Figma")),
        ];

        let snapshot = ContextSnapshotBuilder::new("ws-strong", "Core Backend")
            .with_state(ContextStateValue::Active)
            .with_freshness(StateFreshness::Fresh, 600)
            .with_task(Some("t-backend".to_string()), "API Endpoints", 0.94, InformationProvenance::Inferred)
            .with_workset_items(items)
            .build();

        assert_eq!(snapshot.workset_item_references.len(), 4);
        assert_eq!(snapshot.primary_items().len(), 2);
        assert_eq!(snapshot.supporting_items().len(), 2);
        assert_eq!(snapshot.core_items().len(), 4);
        assert_eq!(snapshot.resume_readiness.readiness, ResumeReadiness::Ready);
    }

    #[test]
    fn test_weak_workset_snapshot() {
        let items = vec![
            create_dummy_workset_item("i1", "Peripheral Note", WorksetItemRelevance::Reference, 0.35, None),
        ];

        let snapshot = ContextSnapshotBuilder::new("ws-weak", "Sparse Workspace")
            .with_state(ContextStateValue::RecentlyActive)
            .with_freshness(StateFreshness::Recent, 86400)
            .without_task()
            .with_workset_items(items)
            .build();

        assert_eq!(snapshot.workset_item_references.len(), 1);
        assert_eq!(snapshot.primary_items().len(), 0);
        assert_eq!(snapshot.supporting_items().len(), 0);
        assert_eq!(snapshot.resume_readiness.readiness, ResumeReadiness::Partial);
    }

    #[test]
    fn test_insufficient_evidence_snapshot() {
        let snapshot = ContextSnapshotBuilder::new("ws-empty", "Brand New Workspace")
            .with_state(ContextStateValue::InsufficientEvidence)
            .with_freshness(StateFreshness::VeryStale, 86400 * 30)
            .without_task()
            .build();

        assert_eq!(snapshot.state, ContextStateValue::InsufficientEvidence);
        assert_eq!(snapshot.resume_readiness.readiness, ResumeReadiness::Insufficient);
        assert!(!snapshot.is_working());
        assert!(snapshot.workset_item_references.is_empty());
        assert!(snapshot.snapshot_statement.contains("No prior working activity recorded"));
    }

    #[test]
    fn test_provenance_differentiation() {
        let observed_item = SnapshotWorksetItemReference {
            id: "obs-1".to_string(),
            title: Some("Direct Capture".to_string()),
            item_type: "url".to_string(),
            relevance: WorksetItemRelevance::Primary,
            relevance_score: 0.90,
            provenance: InformationProvenance::Observed,
            app_name: Some("Safari".to_string()),
            domain: Some("apple.com".to_string()),
            content_snippet: "https://apple.com".to_string(),
            reasons: vec!["directly captured by user".to_string()],
            created_at: "2026-09-17T00:00:00Z".to_string(),
            last_active_at: None,
        };

        let confirmed_task = SnapshotTaskReference {
            id: Some("tsk-conf".to_string()),
            title: "Confirmed Milestone".to_string(),
            action_verb: Some("Implementing".to_string()),
            target_subject: Some("Milestone".to_string()),
            confidence: 1.0,
            confidence_band: "High".to_string(),
            provenance: InformationProvenance::Confirmed,
            first_seen_at: Some("2026-09-17T00:00:00Z".to_string()),
            last_active_at: Some("2026-09-17T00:15:00Z".to_string()),
            status: Some("active".to_string()),
        };

        let inferred_activity = SnapshotActivityReference {
            id: Some("trans-1".to_string()),
            entry_type: "task_transition".to_string(),
            description: "Started task: Confirmed Milestone".to_string(),
            timestamp: "2026-09-17T00:00:00Z".to_string(),
            provenance: InformationProvenance::Inferred,
        };

        let snapshot = ContextSnapshotBuilder::new("ws-prov", "Provenance Test")
            .with_state(ContextStateValue::Active)
            .with_task(Some("tsk-conf".to_string()), "Confirmed Milestone", 1.0, InformationProvenance::Confirmed)
            .add_workset_item(observed_item)
            .with_task_references(vec![confirmed_task])
            .with_activity_references(vec![inferred_activity])
            .with_provenance(InformationProvenance::Confirmed)
            .build();

        assert_eq!(snapshot.task_provenance, Some(InformationProvenance::Confirmed));
        assert_eq!(snapshot.provenance, InformationProvenance::Confirmed);
        assert_eq!(snapshot.workset_item_references[0].provenance, InformationProvenance::Observed);
        assert_eq!(snapshot.recent_task_references[0].provenance, InformationProvenance::Confirmed);
        assert_eq!(snapshot.recent_activity_references[0].provenance, InformationProvenance::Inferred);
    }
}

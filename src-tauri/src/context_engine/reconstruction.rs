use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::context_engine::context_state::{
    ContextStateService, ContextStateValue, MeaningfulActivity, ResumeReadiness, StateFreshness,
};
use crate::context_engine::important_memory::{
    ContextImportantMemoryBuilder, ImportantMemoryType, MemoryImportanceBand,
};
use crate::context_engine::memory::{ContextMemoryService, InformationProvenance};
use crate::context_engine::snapshot::{
    ContextSnapshotService, SnapshotActivityReference, SnapshotImportantMemoryReference,
    SnapshotQualityRating, SnapshotTaskReference, SnapshotWorksetItemReference,
};
use crate::context_engine::workset::{ContextWorksetService, WorksetItemRelevance};
use crate::db::models::TaskCandidateStatus;

// =========================================================================
// Epistemic Disclaimer
// =========================================================================

pub const CONTEXT_RECONSTRUCTION_DISCLAIMER: &str =
    "This reconstruction is an algorithmic contextual synthesis based on recorded activity signals. It does not represent an exact recording of user mental state or attention.";

// =========================================================================
// Reconstruction Section Status
// =========================================================================

/// Explicit availability and epistemic status of any reconstructed section.
///
/// Invariants:
/// - `Available`: High-confidence, verified evidence fully available.
/// - `Partial`: Context or items identified, but key information is incomplete.
/// - `Uncertain`: Conflicting, ambiguous, or low-confidence evidence.
/// - `Unavailable`: Zero evidence, unobserved, or deleted entity.
///
/// CRITICAL: Missing or ambiguous information MUST NEVER be fabricated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconstructionSectionStatus {
    /// Section is fully resolved with high confidence and verified evidence.
    Available,
    /// Section is partially resolved with incomplete or weak evidence.
    Partial,
    /// Section evidence is ambiguous, conflicting, or below confidence threshold.
    Uncertain,
    /// Section data is absent, unobserved, or unavailable.
    Unavailable,
}

impl ReconstructionSectionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Partial => "partial",
            Self::Uncertain => "uncertain",
            Self::Unavailable => "unavailable",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Available => "Available",
            Self::Partial => "Partial",
            Self::Uncertain => "Uncertain",
            Self::Unavailable => "Unavailable",
        }
    }

    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }

    pub fn is_partial(&self) -> bool {
        matches!(self, Self::Partial)
    }

    pub fn is_uncertain(&self) -> bool {
        matches!(self, Self::Uncertain)
    }

    pub fn is_unavailable(&self) -> bool {
        matches!(self, Self::Unavailable)
    }

    pub fn is_present(&self) -> bool {
        matches!(self, Self::Available | Self::Partial)
    }
}

impl std::fmt::Display for ReconstructionSectionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// =========================================================================
// Lightweight Reference Aliases & Sub-Models
// =========================================================================

pub type ReconstructionWorksetItemRef = SnapshotWorksetItemReference;
pub type ReconstructionTaskRef = SnapshotTaskReference;
pub type ReconstructionActivityRef = SnapshotActivityReference;
pub type ReconstructionImportantMemoryRef = SnapshotImportantMemoryReference;

/// Lightweight reference to a context or workspace entity.
///
/// Invariant: Does NOT duplicate complete Workspace database models.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReconstructionContextRef {
    /// Canonical workspace / context identifier
    pub id: String,
    /// Context display name
    pub name: String,
    /// Lifecycle state ("active", "dormant", "archived")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_state: Option<String>,
    /// Total associated items count
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_count: Option<usize>,
    /// ISO-8601 timestamp of most recent activity
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_active_at: Option<String>,
    /// Epistemic source
    pub provenance: InformationProvenance,
}

/// Lightweight reference to a point-in-time departure snapshot.
///
/// Invariant: Does NOT store full snapshot trees or heavy payloads.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReconstructionSnapshotRef {
    /// Unique snapshot identifier
    pub id: String,
    /// Context identifier
    pub context_id: String,
    /// ISO-8601 timestamp of departure / capture
    pub created_at: String,
    /// Quality classification rating
    pub quality_rating: SnapshotQualityRating,
    /// Quality score [0.0, 1.0]
    pub quality_score: f64,
    /// Active task title at departure if identified
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_title: Option<String>,
    /// Total workset items count at departure
    pub workset_count: usize,
    /// Snapshot summary narrative
    pub snapshot_statement: String,
    /// Whether this snapshot safely supported resumption
    pub can_confidently_resume: bool,
}

// =========================================================================
// Reconstruction Section Definitions
// =========================================================================

/// Context identification section of a reconstruction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReconstructionContextSection {
    /// Section availability status
    pub status: ReconstructionSectionStatus,
    /// Context identifier (None if uncertain or unavailable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Context display name (e.g. "Aurora Website", "Unknown project")
    pub name: String,
    /// Lifecycle state if resolved
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_state: Option<String>,
    /// Epistemic provenance
    pub provenance: InformationProvenance,
    /// Calibrated confidence score [0.0, 1.0]
    pub confidence: f64,
}

impl ReconstructionContextSection {
    pub fn available(id: impl Into<String>, name: impl Into<String>, lifecycle_state: Option<String>, provenance: InformationProvenance, confidence: f64) -> Self {
        let name_str = name.into();
        Self {
            status: ReconstructionSectionStatus::Available,
            id: Some(id.into()),
            name: crate::context_engine::reasoning::privacy::scrub_private_data(&name_str),
            lifecycle_state,
            provenance,
            confidence: confidence.clamp(0.0, 1.0),
        }
    }

    pub fn partial(name: impl Into<String>, confidence: f64) -> Self {
        let name_str = name.into();
        Self {
            status: ReconstructionSectionStatus::Partial,
            id: None,
            name: crate::context_engine::reasoning::privacy::scrub_private_data(&name_str),
            lifecycle_state: None,
            provenance: InformationProvenance::Inferred,
            confidence: confidence.clamp(0.0, 1.0),
        }
    }

    pub fn uncertain(name_hint: Option<String>) -> Self {
        let name_str = name_hint.unwrap_or_else(|| "Unknown project".to_string());
        Self {
            status: ReconstructionSectionStatus::Uncertain,
            id: None,
            name: crate::context_engine::reasoning::privacy::scrub_private_data(&name_str),
            lifecycle_state: None,
            provenance: InformationProvenance::Inferred,
            confidence: 0.30,
        }
    }

    pub fn unavailable() -> Self {
        Self {
            status: ReconstructionSectionStatus::Unavailable,
            id: None,
            name: "Unassigned".to_string(),
            lifecycle_state: None,
            provenance: InformationProvenance::Inferred,
            confidence: 0.0,
        }
    }
}

/// Task identification section of a reconstruction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReconstructionTaskSection {
    /// Section availability status
    pub status: ReconstructionSectionStatus,
    /// Task title if known (e.g. "Hero animation")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Extracted action verb
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_verb: Option<String>,
    /// Target subject/entity
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_subject: Option<String>,
    /// Decoupled task confidence score [0.0, 1.0]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// Epistemic source
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<InformationProvenance>,
    /// Lightweight task reference if tracked
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_ref: Option<ReconstructionTaskRef>,
    /// Rationale or explanatory reasons
    #[serde(default)]
    pub reasons: Vec<String>,
}

impl ReconstructionTaskSection {
    pub fn available(task_ref: ReconstructionTaskRef) -> Self {
        let title = crate::context_engine::reasoning::privacy::scrub_private_data(&task_ref.title);
        let action_verb = task_ref.action_verb.as_ref().map(|v| crate::context_engine::reasoning::privacy::scrub_private_data(v));
        let target_subject = task_ref.target_subject.as_ref().map(|s| crate::context_engine::reasoning::privacy::scrub_private_data(s));
        let conf = task_ref.confidence;
        let prov = task_ref.provenance;

        Self {
            status: ReconstructionSectionStatus::Available,
            title: Some(title),
            action_verb,
            target_subject,
            confidence: Some(conf),
            provenance: Some(prov),
            task_ref: Some(task_ref),
            reasons: vec!["Grounded active task identified from focused activity".to_string()],
        }
    }

    pub fn partial(title: impl Into<String>, confidence: f64) -> Self {
        let title_clean = crate::context_engine::reasoning::privacy::scrub_private_data(&title.into());
        Self {
            status: ReconstructionSectionStatus::Partial,
            title: Some(title_clean),
            action_verb: None,
            target_subject: None,
            confidence: Some(confidence.clamp(0.0, 1.0)),
            provenance: Some(InformationProvenance::Inferred),
            task_ref: None,
            reasons: vec!["Task candidate inferred with partial evidence".to_string()],
        }
    }

    pub fn uncertain(candidate_hint: Option<String>) -> Self {
        let reasons = if let Some(ref hint) = candidate_hint {
            vec![format!("Uncertain task candidate '{}' lacks corroborating evidence", hint)]
        } else {
            vec!["Conflicting or ambiguous activity signals prevent confident task inference".to_string()]
        };

        Self {
            status: ReconstructionSectionStatus::Uncertain,
            title: candidate_hint.map(|h| crate::context_engine::reasoning::privacy::scrub_private_data(&h)),
            action_verb: None,
            target_subject: None,
            confidence: Some(0.35),
            provenance: Some(InformationProvenance::Inferred),
            task_ref: None,
            reasons,
        }
    }

    pub fn unavailable() -> Self {
        Self {
            status: ReconstructionSectionStatus::Unavailable,
            title: None,
            action_verb: None,
            target_subject: None,
            confidence: None,
            provenance: None,
            task_ref: None,
            reasons: vec!["No active or recent task detected".to_string()],
        }
    }
}

/// Conservative context operational state section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReconstructionStateSection {
    /// Section availability status
    pub status: ReconstructionSectionStatus,
    /// Conservative working state classification
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_value: Option<ContextStateValue>,
    /// State freshness tier
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freshness: Option<StateFreshness>,
    /// Defensible relative display wording (e.g. "Active recently", "Last active 2 days ago")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freshness_display: Option<String>,
    /// ISO-8601 timestamp of last recorded activity
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_active_at: Option<String>,
    /// Natural language explanation of state
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
}

impl ReconstructionStateSection {
    pub fn available(
        state_value: ContextStateValue,
        freshness: StateFreshness,
        freshness_display: impl Into<String>,
        last_active_at: Option<String>,
        explanation: Option<String>,
    ) -> Self {
        Self {
            status: ReconstructionSectionStatus::Available,
            state_value: Some(state_value),
            freshness: Some(freshness),
            freshness_display: Some(freshness_display.into()),
            last_active_at,
            explanation,
        }
    }

    pub fn partial(
        state_value: ContextStateValue,
        freshness_display: impl Into<String>,
    ) -> Self {
        Self {
            status: ReconstructionSectionStatus::Partial,
            state_value: Some(state_value),
            freshness: None,
            freshness_display: Some(freshness_display.into()),
            last_active_at: None,
            explanation: Some("State estimated with limited activity history".to_string()),
        }
    }

    pub fn uncertain(explanation: Option<String>) -> Self {
        Self {
            status: ReconstructionSectionStatus::Uncertain,
            state_value: Some(ContextStateValue::Uncertain),
            freshness: None,
            freshness_display: Some("Uncertain state".to_string()),
            last_active_at: None,
            explanation: explanation.or_else(|| Some("Ambiguous or conflicting contextual signals".to_string())),
        }
    }

    pub fn unavailable() -> Self {
        Self {
            status: ReconstructionSectionStatus::Unavailable,
            state_value: Some(ContextStateValue::InsufficientEvidence),
            freshness: None,
            freshness_display: Some("No state recorded".to_string()),
            last_active_at: None,
            explanation: Some("Insufficient evidence to reconstruct operational state".to_string()),
        }
    }
}

/// Last meaningful activity section of a reconstruction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReconstructionMeaningfulActivitySection {
    /// Section availability status
    pub status: ReconstructionSectionStatus,
    /// Verified meaningful activity if identified
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<MeaningfulActivity>,
    /// Concise human-readable description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// ISO-8601 timestamp
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Provenance ("observed", "inferred", "confirmed")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
    /// Confidence score [0.0, 1.0]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
}

impl ReconstructionMeaningfulActivitySection {
    pub fn available(act: MeaningfulActivity) -> Self {
        Self {
            status: ReconstructionSectionStatus::Available,
            description: Some(act.description.clone()),
            timestamp: Some(act.timestamp.clone()),
            provenance: Some(act.provenance.clone()),
            confidence: Some(act.confidence),
            activity: Some(act),
        }
    }

    pub fn partial(description: impl Into<String>, timestamp: Option<String>, confidence: f64) -> Self {
        Self {
            status: ReconstructionSectionStatus::Partial,
            activity: None,
            description: Some(description.into()),
            timestamp,
            provenance: Some("inferred".to_string()),
            confidence: Some(confidence.clamp(0.0, 1.0)),
        }
    }

    pub fn uncertain() -> Self {
        Self {
            status: ReconstructionSectionStatus::Uncertain,
            activity: None,
            description: Some("Uncertain or uncorroborated activity".to_string()),
            timestamp: None,
            provenance: Some("inferred".to_string()),
            confidence: Some(0.35),
        }
    }

    pub fn unavailable() -> Self {
        Self {
            status: ReconstructionSectionStatus::Unavailable,
            activity: None,
            description: None,
            timestamp: None,
            provenance: None,
            confidence: None,
        }
    }
}

/// High-signal working asset references (workset) section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReconstructionWorksetSection {
    /// Section availability status
    pub status: ReconstructionSectionStatus,
    /// Lightweight references to workset items
    #[serde(default)]
    pub items: Vec<ReconstructionWorksetItemRef>,
    /// Total count of items in the workset
    pub total_count: usize,
    /// Count of primary / high-focus items
    pub primary_count: usize,
    /// Count of supporting items
    pub supporting_count: usize,
    /// Count of peripheral or weak items
    pub weak_count: usize,
    /// Workset quality score [0.0, 1.0]
    pub quality_score: f64,
    /// Concise human-readable summary (e.g. "5 items", "1 weak item")
    pub summary: String,
}

impl ReconstructionWorksetSection {
    pub fn available(items: Vec<ReconstructionWorksetItemRef>, quality_score: f64) -> Self {
        let total = items.len();
        let primary = items.iter().filter(|i| i.relevance == WorksetItemRelevance::Primary).count();
        let supporting = items.iter().filter(|i| i.relevance == WorksetItemRelevance::Supporting).count();
        let weak = total.saturating_sub(primary + supporting);

        let summary = if total == 1 {
            "1 item".to_string()
        } else {
            format!("{} items", total)
        };

        Self {
            status: ReconstructionSectionStatus::Available,
            items,
            total_count: total,
            primary_count: primary,
            supporting_count: supporting,
            weak_count: weak,
            quality_score: quality_score.clamp(0.0, 1.0),
            summary,
        }
    }

    pub fn partial(items: Vec<ReconstructionWorksetItemRef>, quality_score: f64) -> Self {
        let total = items.len();
        let primary = items.iter().filter(|i| i.relevance == WorksetItemRelevance::Primary).count();
        let supporting = items.iter().filter(|i| i.relevance == WorksetItemRelevance::Supporting).count();
        let weak = total.saturating_sub(primary + supporting);

        let summary = if total == 1 {
            "1 item".to_string()
        } else {
            format!("{} items", total)
        };

        Self {
            status: ReconstructionSectionStatus::Partial,
            items,
            total_count: total,
            primary_count: primary,
            supporting_count: supporting,
            weak_count: weak,
            quality_score: quality_score.clamp(0.0, 1.0),
            summary,
        }
    }

    pub fn uncertain(weak_count: usize, weak_items: Vec<ReconstructionWorksetItemRef>) -> Self {
        let count = weak_count.max(weak_items.len());
        let summary = if count == 1 {
            "1 weak item".to_string()
        } else {
            format!("{} weak items", count)
        };

        Self {
            status: ReconstructionSectionStatus::Uncertain,
            items: weak_items,
            total_count: count,
            primary_count: 0,
            supporting_count: 0,
            weak_count: count,
            quality_score: 0.25,
            summary,
        }
    }

    pub fn unavailable() -> Self {
        Self {
            status: ReconstructionSectionStatus::Unavailable,
            items: Vec::new(),
            total_count: 0,
            primary_count: 0,
            supporting_count: 0,
            weak_count: 0,
            quality_score: 0.0,
            summary: "0 items".to_string(),
        }
    }
}

/// Long-lived important memory references section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReconstructionImportantMemoriesSection {
    /// Section availability status
    pub status: ReconstructionSectionStatus,
    /// Lightweight references to important memories
    #[serde(default)]
    pub memories: Vec<ReconstructionImportantMemoryRef>,
    /// Total count of important memories
    pub total_count: usize,
    /// Count of user-pinned or confirmed anchors
    pub pinned_count: usize,
    /// Concise human-readable summary (e.g. "3", "0")
    pub summary: String,
}

impl ReconstructionImportantMemoriesSection {
    pub fn available(memories: Vec<ReconstructionImportantMemoryRef>) -> Self {
        let total = memories.len();
        let pinned = memories
            .iter()
            .filter(|m| m.provenance == InformationProvenance::Confirmed || m.importance_band == MemoryImportanceBand::Critical)
            .count();

        Self {
            status: ReconstructionSectionStatus::Available,
            memories,
            total_count: total,
            pinned_count: pinned,
            summary: total.to_string(),
        }
    }

    pub fn partial(memories: Vec<ReconstructionImportantMemoryRef>) -> Self {
        let total = memories.len();
        let pinned = memories
            .iter()
            .filter(|m| m.provenance == InformationProvenance::Confirmed)
            .count();

        Self {
            status: ReconstructionSectionStatus::Partial,
            memories,
            total_count: total,
            pinned_count: pinned,
            summary: total.to_string(),
        }
    }

    pub fn uncertain() -> Self {
        Self {
            status: ReconstructionSectionStatus::Uncertain,
            memories: Vec::new(),
            total_count: 0,
            pinned_count: 0,
            summary: "uncertain".to_string(),
        }
    }

    pub fn unavailable() -> Self {
        Self {
            status: ReconstructionSectionStatus::Unavailable,
            memories: Vec::new(),
            total_count: 0,
            pinned_count: 0,
            summary: "0".to_string(),
        }
    }
}

/// Latest departure snapshot reference section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReconstructionSnapshotSection {
    /// Section availability status
    pub status: ReconstructionSectionStatus,
    /// Lightweight reference to latest departure snapshot if exists
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<ReconstructionSnapshotRef>,
    /// Snapshot identifier if available
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
    /// ISO-8601 creation timestamp
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captured_at: Option<String>,
    /// Quality classification
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_rating: Option<SnapshotQualityRating>,
    /// Human-readable snapshot summary statement
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub statement: Option<String>,
}

impl ReconstructionSnapshotSection {
    pub fn available(snapshot: ReconstructionSnapshotRef) -> Self {
        Self {
            status: ReconstructionSectionStatus::Available,
            snapshot_id: Some(snapshot.id.clone()),
            captured_at: Some(snapshot.created_at.clone()),
            quality_rating: Some(snapshot.quality_rating),
            statement: Some(snapshot.snapshot_statement.clone()),
            snapshot: Some(snapshot),
        }
    }

    pub fn partial(snapshot_id: impl Into<String>, captured_at: impl Into<String>, statement: Option<String>) -> Self {
        Self {
            status: ReconstructionSectionStatus::Partial,
            snapshot: None,
            snapshot_id: Some(snapshot_id.into()),
            captured_at: Some(captured_at.into()),
            quality_rating: Some(SnapshotQualityRating::Partial),
            statement,
        }
    }

    pub fn uncertain() -> Self {
        Self {
            status: ReconstructionSectionStatus::Uncertain,
            snapshot: None,
            snapshot_id: None,
            captured_at: None,
            quality_rating: Some(SnapshotQualityRating::Insufficient),
            statement: Some("Uncertain departure snapshot".to_string()),
        }
    }

    pub fn unavailable() -> Self {
        Self {
            status: ReconstructionSectionStatus::Unavailable,
            snapshot: None,
            snapshot_id: None,
            captured_at: None,
            quality_rating: None,
            statement: None,
        }
    }
}

/// Recent chronological activity milestones section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReconstructionRecentActivitySection {
    /// Section availability status
    pub status: ReconstructionSectionStatus,
    /// Lightweight references to activity milestones
    #[serde(default)]
    pub activities: Vec<ReconstructionActivityRef>,
    /// Total count of activity milestones
    pub total_count: usize,
    /// ISO-8601 start timestamp of time window if known
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_window_start: Option<String>,
    /// ISO-8601 end timestamp of time window if known
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_window_end: Option<String>,
}

impl ReconstructionRecentActivitySection {
    pub fn available(activities: Vec<ReconstructionActivityRef>, window_start: Option<String>, window_end: Option<String>) -> Self {
        let total = activities.len();
        Self {
            status: ReconstructionSectionStatus::Available,
            activities,
            total_count: total,
            time_window_start: window_start,
            time_window_end: window_end,
        }
    }

    pub fn partial(activities: Vec<ReconstructionActivityRef>) -> Self {
        let total = activities.len();
        Self {
            status: ReconstructionSectionStatus::Partial,
            activities,
            total_count: total,
            time_window_start: None,
            time_window_end: None,
        }
    }

    pub fn uncertain() -> Self {
        Self {
            status: ReconstructionSectionStatus::Uncertain,
            activities: Vec::new(),
            total_count: 0,
            time_window_start: None,
            time_window_end: None,
        }
    }

    pub fn unavailable() -> Self {
        Self {
            status: ReconstructionSectionStatus::Unavailable,
            activities: Vec::new(),
            total_count: 0,
            time_window_start: None,
            time_window_end: None,
        }
    }
}

/// Actionable resume readiness assessment section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReconstructionResumeReadinessSection {
    /// Section availability status
    pub status: ReconstructionSectionStatus,
    /// Internal readiness classification
    pub readiness: ResumeReadiness,
    /// Decoupled readiness score [0.0, 1.0] if evaluated
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    /// Critical gate: whether work can be safely resumed
    pub can_resume: bool,
    /// Human-readable reasons supporting the rating
    #[serde(default)]
    pub reasons: Vec<String>,
    /// Specific missing factors if incomplete
    #[serde(default)]
    pub missing_factors: Vec<String>,
    /// Detailed formatted explanation
    pub formatted_explanation: String,
}

impl ReconstructionResumeReadinessSection {
    pub fn ready(reasons: Vec<String>, formatted_explanation: impl Into<String>) -> Self {
        Self {
            status: ReconstructionSectionStatus::Available,
            readiness: ResumeReadiness::Ready,
            score: Some(0.95),
            can_resume: true,
            reasons,
            missing_factors: Vec::new(),
            formatted_explanation: formatted_explanation.into(),
        }
    }

    pub fn mostly_ready(reasons: Vec<String>, missing_factors: Vec<String>, formatted_explanation: impl Into<String>) -> Self {
        Self {
            status: ReconstructionSectionStatus::Available,
            readiness: ResumeReadiness::MostlyReady,
            score: Some(0.75),
            can_resume: true,
            reasons,
            missing_factors,
            formatted_explanation: formatted_explanation.into(),
        }
    }

    pub fn partial(reasons: Vec<String>, missing_factors: Vec<String>, formatted_explanation: impl Into<String>) -> Self {
        Self {
            status: ReconstructionSectionStatus::Partial,
            readiness: ResumeReadiness::Partial,
            score: Some(0.50),
            can_resume: false,
            reasons,
            missing_factors,
            formatted_explanation: formatted_explanation.into(),
        }
    }

    pub fn insufficient(missing_factors: Vec<String>, formatted_explanation: impl Into<String>) -> Self {
        Self {
            status: ReconstructionSectionStatus::Unavailable,
            readiness: ResumeReadiness::Insufficient,
            score: Some(0.15),
            can_resume: false,
            reasons: vec!["Insufficient evidence to support resumption".to_string()],
            missing_factors,
            formatted_explanation: formatted_explanation.into(),
        }
    }

    pub fn uncertain(reasons: Vec<String>) -> Self {
        Self {
            status: ReconstructionSectionStatus::Uncertain,
            readiness: ResumeReadiness::Insufficient,
            score: Some(0.25),
            can_resume: false,
            reasons,
            missing_factors: vec!["Uncertain context or ambiguous signals".to_string()],
            formatted_explanation: "Contextual signals are uncertain or contradictory.".to_string(),
        }
    }

    pub fn unavailable() -> Self {
        Self {
            status: ReconstructionSectionStatus::Unavailable,
            readiness: ResumeReadiness::Insufficient,
            score: None,
            can_resume: false,
            reasons: vec!["No readiness data available".to_string()],
            missing_factors: vec!["All contextual dimensions absent".to_string()],
            formatted_explanation: "Insufficient evidence to evaluate resume readiness.".to_string(),
        }
    }
}

/// Overall composite confidence evaluation of the reconstruction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReconstructionConfidence {
    /// Section availability status
    pub status: ReconstructionSectionStatus,
    /// Calibrated normalized confidence score [0.0, 1.0]
    pub score: f64,
    /// Qualitative confidence band ("High", "Medium", "Low", "Insufficient")
    pub band: String,
    /// Explanatory rationale
    #[serde(default)]
    pub reasons: Vec<String>,
}

impl ReconstructionConfidence {
    pub fn new(score: f64, reasons: Vec<String>) -> Self {
        let clamped = score.clamp(0.0, 1.0);
        let band = if clamped >= 0.75 {
            "High"
        } else if clamped >= 0.50 {
            "Medium"
        } else if clamped >= 0.35 {
            "Low"
        } else {
            "Insufficient"
        };

        let status = if clamped >= 0.70 {
            ReconstructionSectionStatus::Available
        } else if clamped >= 0.40 {
            ReconstructionSectionStatus::Partial
        } else if clamped >= 0.20 {
            ReconstructionSectionStatus::Uncertain
        } else {
            ReconstructionSectionStatus::Unavailable
        };

        Self {
            status,
            score: clamped,
            band: band.to_string(),
            reasons,
        }
    }

    pub fn high(score: f64) -> Self {
        Self::new(score.max(0.80), vec!["Strong multi-source evidence corroborates reconstruction".to_string()])
    }

    pub fn medium(score: f64) -> Self {
        Self::new(score.clamp(0.50, 0.79), vec!["Moderate contextual evidence available".to_string()])
    }

    pub fn low(score: f64) -> Self {
        Self::new(score.clamp(0.35, 0.49), vec!["Limited or peripheral evidence".to_string()])
    }

    pub fn insufficient() -> Self {
        Self::new(0.15, vec!["Negligible or conflicting contextual evidence".to_string()])
    }
}

// =========================================================================
// ContextReconstruction: Unified Canonical Reconstruction Model
// =========================================================================

/// Canonical Context Reconstruction Model.
///
/// Orchestrates derived and persisted contextual signals into a high-fidelity,
/// explainable representation of a user's context.
///
/// Invariants:
/// 1. Uses lightweight references; NEVER duplicates full Item, Task, Session, or Context rows.
/// 2. Explicitly supports incomplete reconstruction across all sections (`available`, `partial`, `uncertain`, `unavailable`).
/// 3. NEVER fabricates missing information.
/// 4. Accompanied by standard epistemic disclaimer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextReconstruction {
    /// Context / workspace identification section
    pub context: ReconstructionContextSection,
    /// Active or inferred task section
    pub current_task: ReconstructionTaskSection,
    /// Conservative operational state & freshness section
    pub current_state: ReconstructionStateSection,
    /// Last verified meaningful activity section
    pub last_meaningful_activity: ReconstructionMeaningfulActivitySection,
    /// High-signal working asset references (workset) section
    pub workset: ReconstructionWorksetSection,
    /// Long-lived contextual memory references section
    pub important_memories: ReconstructionImportantMemoriesSection,
    /// Latest departure snapshot reference section
    pub latest_snapshot: ReconstructionSnapshotSection,
    /// Recent chronological activity milestones section
    pub recent_activity: ReconstructionRecentActivitySection,
    /// Actionable resume readiness assessment section
    pub resume_readiness: ReconstructionResumeReadinessSection,
    /// Overall composite confidence evaluation
    pub confidence: ReconstructionConfidence,
    /// ISO-8601 timestamp when this reconstruction was derived
    pub reconstructed_at: String,
    /// Concise, natural executive summary statement
    pub summary_statement: String,
    /// Standard epistemic disclaimer
    pub epistemic_disclaimer: String,
}

impl ContextReconstruction {
    /// Creates a new `ContextReconstruction` with explicit sections.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        context: ReconstructionContextSection,
        current_task: ReconstructionTaskSection,
        current_state: ReconstructionStateSection,
        last_meaningful_activity: ReconstructionMeaningfulActivitySection,
        workset: ReconstructionWorksetSection,
        important_memories: ReconstructionImportantMemoriesSection,
        latest_snapshot: ReconstructionSnapshotSection,
        recent_activity: ReconstructionRecentActivitySection,
        resume_readiness: ReconstructionResumeReadinessSection,
        confidence: ReconstructionConfidence,
        reconstructed_at: Option<String>,
    ) -> Self {
        let ts = reconstructed_at.unwrap_or_else(|| Utc::now().to_rfc3339());
        let summary = Self::generate_summary_statement(
            &context,
            &current_task,
            &current_state,
            &workset,
            &resume_readiness,
        );

        Self {
            context,
            current_task,
            current_state,
            last_meaningful_activity,
            workset,
            important_memories,
            latest_snapshot,
            recent_activity,
            resume_readiness,
            confidence,
            reconstructed_at: ts,
            summary_statement: summary,
            epistemic_disclaimer: CONTEXT_RECONSTRUCTION_DISCLAIMER.to_string(),
        }
    }

    /// Generates a natural, factual summary statement reflecting the exact state without robotic strings.
    pub fn generate_summary_statement(
        context: &ReconstructionContextSection,
        current_task: &ReconstructionTaskSection,
        current_state: &ReconstructionStateSection,
        workset: &ReconstructionWorksetSection,
        resume_readiness: &ReconstructionResumeReadinessSection,
    ) -> String {
        let ctx_name = if context.status.is_present() && !context.name.trim().is_empty() {
            context.name.as_str()
        } else if context.status == ReconstructionSectionStatus::Uncertain {
            "Unknown project"
        } else {
            "Unassigned workspace"
        };

        let task_part = match current_task.status {
            ReconstructionSectionStatus::Available => {
                if let Some(ref title) = current_task.title {
                    format!("on '{}'", title)
                } else {
                    "on active work".to_string()
                }
            }
            ReconstructionSectionStatus::Partial => {
                if let Some(ref title) = current_task.title {
                    format!("likely on '{}'", title)
                } else {
                    "on ongoing work".to_string()
                }
            }
            ReconstructionSectionStatus::Uncertain => "with uncertain task focus".to_string(),
            ReconstructionSectionStatus::Unavailable => "with no active task".to_string(),
        };

        let state_part = match current_state.status {
            ReconstructionSectionStatus::Available => {
                if let Some(ref disp) = current_state.freshness_display {
                    disp.to_lowercase()
                } else if let Some(ref val) = current_state.state_value {
                    val.as_str().replace('_', " ")
                } else {
                    "recently active".to_string()
                }
            }
            ReconstructionSectionStatus::Partial => "state partially known".to_string(),
            ReconstructionSectionStatus::Uncertain => "state uncertain".to_string(),
            ReconstructionSectionStatus::Unavailable => "no recent activity recorded".to_string(),
        };

        let workset_part = match workset.status {
            ReconstructionSectionStatus::Available | ReconstructionSectionStatus::Partial => {
                if workset.total_count == 1 {
                    "1 key reference".to_string()
                } else if workset.total_count > 0 {
                    format!("{} key references", workset.total_count)
                } else {
                    "no open documents".to_string()
                }
            }
            ReconstructionSectionStatus::Uncertain => {
                if workset.weak_count > 0 {
                    format!("{} weak item", workset.weak_count)
                } else {
                    "uncertain workset".to_string()
                }
            }
            ReconstructionSectionStatus::Unavailable => "no workset items".to_string(),
        };

        let readiness_part = resume_readiness.readiness.display_name();

        format!(
            "Context: {}. Working {} ({}). Workset: {}. Resume readiness: {}.",
            ctx_name, task_part, state_part, workset_part, readiness_part
        )
    }

    /// Convenience predicate: Whether the reconstruction is rich and complete across all major sections.
    pub fn is_complete(&self) -> bool {
        self.context.status == ReconstructionSectionStatus::Available
            && self.current_task.status == ReconstructionSectionStatus::Available
            && self.current_state.status == ReconstructionSectionStatus::Available
            && self.workset.status == ReconstructionSectionStatus::Available
            && self.resume_readiness.can_resume
    }

    /// Convenience predicate: Whether the context can be safely resumed without friction.
    pub fn is_resumable(&self) -> bool {
        self.resume_readiness.can_resume
    }

    /// Convenience predicate: Whether any key section is uncertain or ambiguous.
    pub fn has_uncertainty(&self) -> bool {
        self.context.status == ReconstructionSectionStatus::Uncertain
            || self.current_task.status == ReconstructionSectionStatus::Uncertain
            || self.current_state.status == ReconstructionSectionStatus::Uncertain
            || self.workset.status == ReconstructionSectionStatus::Uncertain
            || self.resume_readiness.status == ReconstructionSectionStatus::Uncertain
    }

    /// Total count of available sections.
    pub fn available_sections_count(&self) -> usize {
        let mut count = 0;
        if self.context.status.is_available() { count += 1; }
        if self.current_task.status.is_available() { count += 1; }
        if self.current_state.status.is_available() { count += 1; }
        if self.last_meaningful_activity.status.is_available() { count += 1; }
        if self.workset.status.is_available() { count += 1; }
        if self.important_memories.status.is_available() { count += 1; }
        if self.latest_snapshot.status.is_available() { count += 1; }
        if self.recent_activity.status.is_available() { count += 1; }
        if self.resume_readiness.status.is_available() { count += 1; }
        count
    }

    /// Total count of unavailable sections.
    pub fn unavailable_sections_count(&self) -> usize {
        let mut count = 0;
        if self.context.status.is_unavailable() { count += 1; }
        if self.current_task.status.is_unavailable() { count += 1; }
        if self.current_state.status.is_unavailable() { count += 1; }
        if self.last_meaningful_activity.status.is_unavailable() { count += 1; }
        if self.workset.status.is_unavailable() { count += 1; }
        if self.important_memories.status.is_unavailable() { count += 1; }
        if self.latest_snapshot.status.is_unavailable() { count += 1; }
        if self.recent_activity.status.is_unavailable() { count += 1; }
        if self.resume_readiness.status.is_unavailable() { count += 1; }
        count
    }
}

// =========================================================================
// ContextReconstructionBuilder: Fluent Construction API
// =========================================================================

/// Fluent builder for assembling a `ContextReconstruction`.
#[derive(Debug, Clone)]
pub struct ContextReconstructionBuilder {
    context: Option<ReconstructionContextSection>,
    current_task: Option<ReconstructionTaskSection>,
    current_state: Option<ReconstructionStateSection>,
    last_meaningful_activity: Option<ReconstructionMeaningfulActivitySection>,
    workset: Option<ReconstructionWorksetSection>,
    important_memories: Option<ReconstructionImportantMemoriesSection>,
    latest_snapshot: Option<ReconstructionSnapshotSection>,
    recent_activity: Option<ReconstructionRecentActivitySection>,
    resume_readiness: Option<ReconstructionResumeReadinessSection>,
    confidence: Option<ReconstructionConfidence>,
    reconstructed_at: Option<String>,
}

impl Default for ContextReconstructionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextReconstructionBuilder {
    pub fn new() -> Self {
        Self {
            context: None,
            current_task: None,
            current_state: None,
            last_meaningful_activity: None,
            workset: None,
            important_memories: None,
            latest_snapshot: None,
            recent_activity: None,
            resume_readiness: None,
            confidence: None,
            reconstructed_at: None,
        }
    }

    pub fn context(mut self, section: ReconstructionContextSection) -> Self {
        self.context = Some(section);
        self
    }

    pub fn current_task(mut self, section: ReconstructionTaskSection) -> Self {
        self.current_task = Some(section);
        self
    }

    pub fn current_state(mut self, section: ReconstructionStateSection) -> Self {
        self.current_state = Some(section);
        self
    }

    pub fn last_meaningful_activity(mut self, section: ReconstructionMeaningfulActivitySection) -> Self {
        self.last_meaningful_activity = Some(section);
        self
    }

    pub fn workset(mut self, section: ReconstructionWorksetSection) -> Self {
        self.workset = Some(section);
        self
    }

    pub fn important_memories(mut self, section: ReconstructionImportantMemoriesSection) -> Self {
        self.important_memories = Some(section);
        self
    }

    pub fn latest_snapshot(mut self, section: ReconstructionSnapshotSection) -> Self {
        self.latest_snapshot = Some(section);
        self
    }

    pub fn recent_activity(mut self, section: ReconstructionRecentActivitySection) -> Self {
        self.recent_activity = Some(section);
        self
    }

    pub fn resume_readiness(mut self, section: ReconstructionResumeReadinessSection) -> Self {
        self.resume_readiness = Some(section);
        self
    }

    pub fn confidence(mut self, confidence: ReconstructionConfidence) -> Self {
        self.confidence = Some(confidence);
        self
    }

    pub fn reconstructed_at(mut self, ts: impl Into<String>) -> Self {
        self.reconstructed_at = Some(ts.into());
        self
    }

    /// Builds the `ContextReconstruction`, defaulting unprovided sections to `unavailable` or `uncertain`
    /// WITHOUT fabricating missing data.
    pub fn build(self) -> ContextReconstruction {
        let context = self.context.unwrap_or_else(ReconstructionContextSection::unavailable);
        let current_task = self.current_task.unwrap_or_else(ReconstructionTaskSection::unavailable);
        let current_state = self.current_state.unwrap_or_else(ReconstructionStateSection::unavailable);
        let last_meaningful_activity = self.last_meaningful_activity.unwrap_or_else(ReconstructionMeaningfulActivitySection::unavailable);
        let workset = self.workset.unwrap_or_else(ReconstructionWorksetSection::unavailable);
        let important_memories = self.important_memories.unwrap_or_else(ReconstructionImportantMemoriesSection::unavailable);
        let latest_snapshot = self.latest_snapshot.unwrap_or_else(ReconstructionSnapshotSection::unavailable);
        let recent_activity = self.recent_activity.unwrap_or_else(ReconstructionRecentActivitySection::unavailable);
        let resume_readiness = self.resume_readiness.unwrap_or_else(ReconstructionResumeReadinessSection::unavailable);
        let confidence = self.confidence.unwrap_or_else(ReconstructionConfidence::insufficient);

        ContextReconstruction::new(
            context,
            current_task,
            current_state,
            last_meaningful_activity,
            workset,
            important_memories,
            latest_snapshot,
            recent_activity,
            resume_readiness,
            confidence,
            self.reconstructed_at,
        )
    }
}

// =========================================================================
// ContextReconstructionService: Deterministic Orchestration Service
// =========================================================================

/// Concrete service providing deterministic contextual reconstruction.
///
/// Orchestrates the 12-step deterministic reconstruction pipeline:
/// 1. Workspace Identity & Lifecycle State
/// 2. Context Evidence & Membership Vetoes
/// 3. Current / Recent Task Candidates
/// 4. Context Memory (Working & Episodic Sequences)
/// 5. Context Workset (Primary / Supporting Working Assets)
/// 6. Context Operational State & Freshness
/// 7. Important Contextual Memories & Anchors
/// 8. Latest Departure Snapshot (if enabled)
/// 9. Recent Meaningful Activity Milestones
/// 10. Resume Readiness Assessment
/// 11. Composite Reconstruction Confidence
/// 12. Assembled ContextReconstruction Narrative
#[derive(Debug, Default, Clone)]
pub struct ContextReconstructionService;

impl ContextReconstructionService {
    pub fn new() -> Self {
        Self
    }

    /// Reconstructs the context for a given context / workspace identifier.
    ///
    /// Executes the deterministic 12-step pipeline with graceful degradation across all subsystems.
    pub fn reconstruct_context(
        &self,
        conn: &Connection,
        context_id: &str,
    ) -> Result<ContextReconstruction, String> {
        let now_ts = Utc::now().to_rfc3339();

        // -----------------------------------------------------------------
        // Step 1: Load Context
        // -----------------------------------------------------------------
        let ws_res = crate::db::operations::get_workspace(conn, context_id);
        let (context_section, ws_name, is_workspace_found) = match ws_res {
            Ok(ws) => {
                let name = ws.name.clone();
                let status = Some(ws.lifecycle_state.clone());
                (
                    ReconstructionContextSection::available(
                        ws.id,
                        name.clone(),
                        status,
                        InformationProvenance::Confirmed,
                        1.0,
                    ),
                    name,
                    true,
                )
            }
            Err(_) => {
                // Check if items exist for this context ID
                let items_count = crate::db::operations::list_items(conn, Some(context_id))
                    .map(|it| it.len())
                    .unwrap_or(0);
                if items_count > 0 {
                    (
                        ReconstructionContextSection::partial(
                            format!("Context {}", context_id),
                            0.60,
                        ),
                        format!("Context {}", context_id),
                        false,
                    )
                } else {
                    (
                        ReconstructionContextSection::uncertain(Some("Unknown project".to_string())),
                        "Unknown project".to_string(),
                        false,
                    )
                }
            }
        };

        // -----------------------------------------------------------------
        // Step 2: Retrieve Relevant Context Evidence & Veto Check
        // -----------------------------------------------------------------
        let raw_items = crate::db::operations::list_items(conn, Some(context_id)).unwrap_or_default();
        // Invariant: Hard veto on items marked 'dismissed' in context_memberships
        let active_items: Vec<_> = raw_items
            .into_iter()
            .filter(|it| {
                !it.metadata.contains("\"status\":\"dismissed\"")
                    && !it.metadata.contains("\"membership_status\":\"dismissed\"")
            })
            .collect();

        // -----------------------------------------------------------------
        // Step 3: Determine Current / Recent Task
        // -----------------------------------------------------------------
        let task_candidates = crate::db::operations::list_task_candidates_for_context(conn, context_id).unwrap_or_default();
        // Filter out inactive and dismissed tasks (Status Override Invariant)
        let mut active_task_candidates: Vec<_> = task_candidates
            .into_iter()
            .filter(|tc| tc.status != TaskCandidateStatus::Inactive && tc.status != TaskCandidateStatus::Dismissed)
            .collect();
        active_task_candidates.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));

        let mut current_task_section = if let Some(best_task) = active_task_candidates.first() {
            if best_task.confidence >= 0.65 {
                let task_ref = SnapshotTaskReference::from(best_task);
                ReconstructionTaskSection::available(task_ref)
            } else if best_task.confidence >= 0.35 {
                ReconstructionTaskSection::partial(&best_task.title, best_task.confidence)
            } else {
                ReconstructionTaskSection::uncertain(Some(best_task.title.clone()))
            }
        } else {
            ReconstructionTaskSection::unavailable()
        };

        // -----------------------------------------------------------------
        // Step 4: Retrieve ContextMemory
        // -----------------------------------------------------------------
        let memory_opt = ContextMemoryService::new()
            .reconstruct_memory(conn, context_id)
            .ok();

        // If task is still unavailable but memory resolved an active task with high confidence, use it
        if current_task_section.status == ReconstructionSectionStatus::Unavailable {
            if let Some(ref mem) = memory_opt {
                if let Some(ref title) = mem.current_task_title {
                    let conf = mem.current_task_confidence.unwrap_or(0.60);
                    if conf >= 0.65 {
                        current_task_section = ReconstructionTaskSection::partial(title, conf);
                    } else if conf >= 0.35 {
                        current_task_section = ReconstructionTaskSection::partial(title, conf);
                    }
                }
            }
        }

        // -----------------------------------------------------------------
        // Step 5: Retrieve ContextWorkset
        // -----------------------------------------------------------------
        let workset_res = ContextWorksetService::new().derive_for_workspace(conn, context_id);
        let workset_section = match workset_res {
            Ok(ws) => {
                let snapshot_items: Vec<SnapshotWorksetItemReference> = ws
                    .items
                    .iter()
                    .map(SnapshotWorksetItemReference::from)
                    .collect();

                let quality_score = ws.confidence;

                if ws.items.is_empty() {
                    ReconstructionWorksetSection::unavailable()
                } else if quality_score >= 0.60 && ws.items.len() >= 2 {
                    ReconstructionWorksetSection::available(snapshot_items, quality_score)
                } else if ws.items.len() == 1 && quality_score < 0.40 {
                    ReconstructionWorksetSection::uncertain(1, snapshot_items)
                } else {
                    ReconstructionWorksetSection::partial(snapshot_items, quality_score)
                }
            }
            Err(_) => {
                if !active_items.is_empty() {
                    ReconstructionWorksetSection::partial(vec![], 0.30)
                } else {
                    ReconstructionWorksetSection::unavailable()
                }
            }
        };

        // -----------------------------------------------------------------
        // Step 6: Determine ContextState
        // -----------------------------------------------------------------
        let state_res = ContextStateService::new().derive_for_context(conn, context_id);
        let (current_state_section, resume_readiness_section, meaningful_act_section) = match state_res {
            Ok(state) => {
                let state_expl = if !state.explanation.reasons.is_empty() {
                    format!("{}: {}", state.explanation.title, state.explanation.reasons.iter().map(|r| r.text.as_str()).collect::<Vec<_>>().join("; "))
                } else {
                    state.explanation.title.clone()
                };
                let state_sec = ReconstructionStateSection::available(
                    state.state,
                    state.freshness,
                    state.freshness_display,
                    state.last_active_at,
                    Some(state_expl),
                );

                let readiness_sec = match state.readiness.readiness {
                    ResumeReadiness::Ready => ReconstructionResumeReadinessSection::ready(
                        state.readiness.reasons,
                        state.readiness.formatted_explanation,
                    ),
                    ResumeReadiness::MostlyReady => ReconstructionResumeReadinessSection::mostly_ready(
                        state.readiness.reasons,
                        vec![],
                        state.readiness.formatted_explanation,
                    ),
                    ResumeReadiness::Partial => ReconstructionResumeReadinessSection::partial(
                        state.readiness.reasons,
                        vec![],
                        state.readiness.formatted_explanation,
                    ),
                    ResumeReadiness::Insufficient => ReconstructionResumeReadinessSection::insufficient(
                        vec![],
                        state.readiness.formatted_explanation,
                    ),
                };

                let meaningful_sec = if let Some(act) = state.meaningful_activity {
                    ReconstructionMeaningfulActivitySection::available(act)
                } else {
                    ReconstructionMeaningfulActivitySection::unavailable()
                };

                (state_sec, readiness_sec, meaningful_sec)
            }
            Err(_) => {
                let state_sec = if is_workspace_found {
                    ReconstructionStateSection::partial(
                        ContextStateValue::RecentlyActive,
                        "State estimated with limited activity history",
                    )
                } else {
                    ReconstructionStateSection::unavailable()
                };

                let readiness_sec = ReconstructionResumeReadinessSection::insufficient(
                    vec!["State derivation unavailable".to_string()],
                    "Insufficient evidence to evaluate readiness.",
                );

                let meaningful_sec = ReconstructionMeaningfulActivitySection::unavailable();

                (state_sec, readiness_sec, meaningful_sec)
            }
        };

        // -----------------------------------------------------------------
        // Step 7: Retrieve Important Contextual Memory
        // -----------------------------------------------------------------
        let important_memories_section = match ContextImportantMemoryBuilder::build(conn, context_id, None) {
            Ok(mem_set) => {
                let snapshot_mems: Vec<SnapshotImportantMemoryReference> = mem_set
                    .memories
                    .iter()
                    .map(SnapshotImportantMemoryReference::from)
                    .collect();

                if snapshot_mems.is_empty() {
                    ReconstructionImportantMemoriesSection::unavailable()
                } else {
                    ReconstructionImportantMemoriesSection::available(snapshot_mems)
                }
            }
            Err(_) => ReconstructionImportantMemoriesSection::unavailable(),
        };

        // -----------------------------------------------------------------
        // Step 8: Retrieve Latest ContextSnapshot
        // -----------------------------------------------------------------
        let latest_snapshot_section = if ContextSnapshotService::is_snapshots_enabled(conn) {
            match ContextSnapshotService::new().derive_for_context_cached(conn, context_id) {
                Ok(snap) => {
                    let snap_ref = ReconstructionSnapshotRef {
                        id: snap.id,
                        context_id: snap.context_id,
                        created_at: snap.created_at,
                        quality_rating: snap.quality.rating,
                        quality_score: snap.quality.score,
                        task_title: snap.task_title,
                        workset_count: snap.workset_item_references.len(),
                        snapshot_statement: snap.snapshot_statement,
                        can_confidently_resume: snap.quality.can_confidently_resume,
                    };
                    ReconstructionSnapshotSection::available(snap_ref)
                }
                Err(_) => ReconstructionSnapshotSection::unavailable(),
            }
        } else {
            ReconstructionSnapshotSection::unavailable()
        };

        // -----------------------------------------------------------------
        // Step 9: Retrieve Recent Meaningful Activity Milestones
        // -----------------------------------------------------------------
        let recent_activity_section = if let Some(ref mem) = memory_opt {
            let act_refs: Vec<SnapshotActivityReference> = mem
                .activity_sequence
                .iter()
                .map(SnapshotActivityReference::from)
                .collect();

            if act_refs.is_empty() {
                ReconstructionRecentActivitySection::unavailable()
            } else {
                ReconstructionRecentActivitySection::available(act_refs, None, None)
            }
        } else {
            ReconstructionRecentActivitySection::unavailable()
        };

        // -----------------------------------------------------------------
        // Step 10 & 11: Calculate / Resolve Overall Reconstruction Confidence
        // -----------------------------------------------------------------
        let mut conf_score = 0.0;
        let mut conf_reasons = Vec::new();

        if context_section.status.is_available() {
            conf_score += 0.20;
            conf_reasons.push(format!("Workspace '{}' confirmed", ws_name));
        } else if context_section.status.is_partial() {
            conf_score += 0.10;
        }

        if current_task_section.status.is_available() {
            let t_conf = current_task_section.confidence.unwrap_or(0.80);
            conf_score += 0.25 * t_conf;
            if let Some(ref t_title) = current_task_section.title {
                conf_reasons.push(format!("Active task '{}' identified", t_title));
            }
        } else if current_task_section.status.is_partial() {
            conf_score += 0.10;
        }

        if workset_section.status.is_available() {
            conf_score += 0.25 * workset_section.quality_score;
            conf_reasons.push(format!("{} workset items available", workset_section.total_count));
        } else if workset_section.status.is_partial() {
            conf_score += 0.10;
        }

        if current_state_section.status.is_available() {
            let fresh_weight = match current_state_section.freshness {
                Some(StateFreshness::Fresh) => 0.15,
                Some(StateFreshness::Recent) => 0.12,
                Some(StateFreshness::Stale) => 0.06,
                Some(StateFreshness::VeryStale) | None => 0.02,
            };
            conf_score += fresh_weight;
        }

        if important_memories_section.status.is_available() {
            conf_score += 0.10;
        }

        if meaningful_act_section.status.is_available() {
            conf_score += 0.05;
        }

        if !is_workspace_found {
            conf_score = conf_score.min(0.35);
            conf_reasons.push("Context entity not found in database".to_string());
        }

        let confidence = ReconstructionConfidence::new(conf_score, conf_reasons);

        // -----------------------------------------------------------------
        // Step 12: Assemble and Return ContextReconstruction
        // -----------------------------------------------------------------
        Ok(ContextReconstruction::new(
            context_section,
            current_task_section,
            current_state_section,
            meaningful_act_section,
            workset_section,
            important_memories_section,
            latest_snapshot_section,
            recent_activity_section,
            resume_readiness_section,
            confidence,
            Some(now_ts),
        ))
    }

    /// Reconstructs the currently active workspace/context if one is active.
    pub fn reconstruct_active(&self, conn: &Connection) -> Result<Option<ContextReconstruction>, String> {
        let active_memory = ContextMemoryService::new().reconstruct_active_memory(conn).ok().flatten();
        if let Some(mem) = active_memory {
            if let Some(ref ctx) = mem.context {
                return self.reconstruct_context(conn, &ctx.id).map(Some);
            }
        }
        Ok(None)
    }

    /// Reconstructs all registered workspaces/contexts.
    pub fn reconstruct_all(&self, conn: &Connection) -> Result<Vec<ContextReconstruction>, String> {
        let workspaces = crate::db::operations::list_workspaces(conn)
            .map_err(|e| format!("Failed to list workspaces: {}", e))?;
        let mut reconstructions = Vec::new();
        for ws in workspaces {
            if let Ok(rec) = self.reconstruct_context(conn, &ws.id) {
                reconstructions.push(rec);
            }
        }
        Ok(reconstructions)
    }
}

// =========================================================================
// Unit Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_engine::important_memory::ImportantMemoryType;
    use crate::context_engine::snapshot::SnapshotTaskReference;

    #[test]
    fn test_example_1_complete_aurora_website_reconstruction() {
        // Build Example 1 from prompt:
        // Context: Aurora Website
        // Task: Hero animation
        // State: recently_active
        // Workset: 5 items
        // Important memories: 3
        // Latest snapshot: available
        // Resume readiness: ready

        let task_ref = SnapshotTaskReference {
            id: Some("task_1".to_string()),
            title: "Hero animation".to_string(),
            action_verb: Some("Designing".to_string()),
            target_subject: Some("Hero animation".to_string()),
            confidence: 0.90,
            confidence_band: "High".to_string(),
            provenance: InformationProvenance::Inferred,
            first_seen_at: Some("2026-09-19T00:00:00Z".to_string()),
            last_active_at: Some("2026-09-19T00:10:00Z".to_string()),
            status: Some("active".to_string()),
        };

        let workset_items: Vec<SnapshotWorksetItemReference> = (1..=5)
            .map(|i| SnapshotWorksetItemReference {
                id: format!("item_{}", i),
                title: Some(format!("Hero Spec Part {}", i)),
                item_type: "url".to_string(),
                relevance: if i <= 2 { WorksetItemRelevance::Primary } else { WorksetItemRelevance::Supporting },
                relevance_score: 0.85,
                provenance: InformationProvenance::Observed,
                app_name: Some("Arc".to_string()),
                domain: Some("figma.com".to_string()),
                content_snippet: "Hero animation specifications".to_string(),
                reasons: vec!["Relevant to active task".to_string()],
                created_at: "2026-09-19T00:05:00Z".to_string(),
                last_active_at: Some("2026-09-19T00:10:00Z".to_string()),
            })
            .collect();

        let important_mems: Vec<SnapshotImportantMemoryReference> = (1..=3)
            .map(|i| SnapshotImportantMemoryReference {
                id: format!("mem_{}", i),
                source_id: format!("item_{}", i),
                memory_type: ImportantMemoryType::ImportantReference,
                title: format!("Aurora Website Repo {}", i),
                detail: Some("Main web client repo".to_string()),
                importance: 0.90,
                importance_band: MemoryImportanceBand::Critical,
                confidence: 0.95,
                provenance: InformationProvenance::Confirmed,
                first_seen_at: Some("2026-09-15T00:00:00Z".to_string()),
                last_seen_at: Some("2026-09-19T00:10:00Z".to_string()),
                app_name: Some("VS Code".to_string()),
                domain: None,
            })
            .collect();

        let snapshot_ref = ReconstructionSnapshotRef {
            id: "snap_1".to_string(),
            context_id: "ctx_aurora".to_string(),
            created_at: "2026-09-19T00:10:00Z".to_string(),
            quality_rating: SnapshotQualityRating::Complete,
            quality_score: 0.92,
            task_title: Some("Hero animation".to_string()),
            workset_count: 5,
            snapshot_statement: "Departed Aurora Website on Hero animation with 5 active items.".to_string(),
            can_confidently_resume: true,
        };

        let reconstruction = ContextReconstructionBuilder::new()
            .context(ReconstructionContextSection::available(
                "ctx_aurora",
                "Aurora Website",
                Some("active".to_string()),
                InformationProvenance::Confirmed,
                0.95,
            ))
            .current_task(ReconstructionTaskSection::available(task_ref))
            .current_state(ReconstructionStateSection::available(
                ContextStateValue::RecentlyActive,
                StateFreshness::Recent,
                "Active recently",
                Some("2026-09-19T00:10:00Z".to_string()),
                Some("Recent focused activity inside workspace".to_string()),
            ))
            .workset(ReconstructionWorksetSection::available(workset_items, 0.90))
            .important_memories(ReconstructionImportantMemoriesSection::available(important_mems))
            .latest_snapshot(ReconstructionSnapshotSection::available(snapshot_ref))
            .resume_readiness(ReconstructionResumeReadinessSection::ready(
                vec!["Active task identified".to_string(), "5 workset items available".to_string()],
                "Ready to resume with high confidence.",
            ))
            .confidence(ReconstructionConfidence::high(0.92))
            .build();

        assert_eq!(reconstruction.context.status, ReconstructionSectionStatus::Available);
        assert_eq!(reconstruction.context.name, "Aurora Website");
        assert_eq!(reconstruction.current_task.status, ReconstructionSectionStatus::Available);
        assert_eq!(reconstruction.current_task.title.as_deref(), Some("Hero animation"));
        assert_eq!(reconstruction.current_state.status, ReconstructionSectionStatus::Available);
        assert_eq!(reconstruction.current_state.state_value, Some(ContextStateValue::RecentlyActive));
        assert_eq!(reconstruction.workset.status, ReconstructionSectionStatus::Available);
        assert_eq!(reconstruction.workset.total_count, 5);
        assert_eq!(reconstruction.workset.summary, "5 items");
        assert_eq!(reconstruction.important_memories.status, ReconstructionSectionStatus::Available);
        assert_eq!(reconstruction.important_memories.total_count, 3);
        assert_eq!(reconstruction.important_memories.summary, "3");
        assert_eq!(reconstruction.latest_snapshot.status, ReconstructionSectionStatus::Available);
        assert_eq!(reconstruction.resume_readiness.status, ReconstructionSectionStatus::Available);
        assert_eq!(reconstruction.resume_readiness.readiness, ResumeReadiness::Ready);
        assert!(reconstruction.resume_readiness.can_resume);
        assert!(reconstruction.is_resumable());
        assert!(reconstruction.is_complete());
        assert!(!reconstruction.has_uncertainty());
    }

    #[test]
    fn test_example_2_uncertain_project_reconstruction() {
        // Build Example 2 from prompt:
        // Context: Unknown project
        // Task: uncertain
        // State: uncertain
        // Workset: 1 weak item
        // Resume readiness: insufficient

        let weak_item = SnapshotWorksetItemReference {
            id: "item_weak_1".to_string(),
            title: Some("Untitled Page".to_string()),
            item_type: "url".to_string(),
            relevance: WorksetItemRelevance::Recent,
            relevance_score: 0.30,
            provenance: InformationProvenance::Observed,
            app_name: Some("Chrome".to_string()),
            domain: Some("google.com".to_string()),
            content_snippet: "Google search".to_string(),
            reasons: vec!["Weak peripheral match".to_string()],
            created_at: "2026-09-18T12:00:00Z".to_string(),
            last_active_at: None,
        };

        let reconstruction = ContextReconstructionBuilder::new()
            .context(ReconstructionContextSection::uncertain(Some("Unknown project".to_string())))
            .current_task(ReconstructionTaskSection::uncertain(None))
            .current_state(ReconstructionStateSection::uncertain(None))
            .workset(ReconstructionWorksetSection::uncertain(1, vec![weak_item]))
            .resume_readiness(ReconstructionResumeReadinessSection::insufficient(
                vec!["No active task".to_string(), "Weak workset".to_string(), "Uncertain context".to_string()],
                "Insufficient evidence to safely resume work.",
            ))
            .confidence(ReconstructionConfidence::insufficient())
            .build();

        assert_eq!(reconstruction.context.status, ReconstructionSectionStatus::Uncertain);
        assert_eq!(reconstruction.context.name, "Unknown project");
        assert_eq!(reconstruction.current_task.status, ReconstructionSectionStatus::Uncertain);
        assert_eq!(reconstruction.current_task.title, None);
        assert_eq!(reconstruction.current_state.status, ReconstructionSectionStatus::Uncertain);
        assert_eq!(reconstruction.workset.status, ReconstructionSectionStatus::Uncertain);
        assert_eq!(reconstruction.workset.weak_count, 1);
        assert_eq!(reconstruction.workset.summary, "1 weak item");
        assert_eq!(reconstruction.resume_readiness.readiness, ResumeReadiness::Insufficient);
        assert!(!reconstruction.resume_readiness.can_resume);
        assert!(!reconstruction.is_resumable());
        assert!(!reconstruction.is_complete());
        assert!(reconstruction.has_uncertainty());
    }

    #[test]
    fn test_empty_and_unavailable_reconstruction_without_fabrication() {
        let reconstruction = ContextReconstructionBuilder::new().build();

        assert_eq!(reconstruction.context.status, ReconstructionSectionStatus::Unavailable);
        assert_eq!(reconstruction.current_task.status, ReconstructionSectionStatus::Unavailable);
        assert_eq!(reconstruction.current_task.title, None);
        assert_eq!(reconstruction.current_state.status, ReconstructionSectionStatus::Unavailable);
        assert_eq!(reconstruction.last_meaningful_activity.status, ReconstructionSectionStatus::Unavailable);
        assert_eq!(reconstruction.workset.status, ReconstructionSectionStatus::Unavailable);
        assert_eq!(reconstruction.workset.total_count, 0);
        assert_eq!(reconstruction.important_memories.status, ReconstructionSectionStatus::Unavailable);
        assert_eq!(reconstruction.important_memories.total_count, 0);
        assert_eq!(reconstruction.latest_snapshot.status, ReconstructionSectionStatus::Unavailable);
        assert_eq!(reconstruction.recent_activity.status, ReconstructionSectionStatus::Unavailable);
        assert_eq!(reconstruction.resume_readiness.status, ReconstructionSectionStatus::Unavailable);
        assert_eq!(reconstruction.resume_readiness.readiness, ResumeReadiness::Insufficient);
        assert!(!reconstruction.resume_readiness.can_resume);
        assert_eq!(reconstruction.unavailable_sections_count(), 9);
    }

    #[test]
    fn test_partial_reconstruction_scenario() {
        let reconstruction = ContextReconstructionBuilder::new()
            .context(ReconstructionContextSection::available(
                "ctx_tripwise",
                "TripWise",
                Some("active".to_string()),
                InformationProvenance::Confirmed,
                0.90,
            ))
            .current_task(ReconstructionTaskSection::partial("API Integration", 0.60))
            .current_state(ReconstructionStateSection::partial(ContextStateValue::RecentlyActive, "Active 3 hours ago"))
            .workset(ReconstructionWorksetSection::partial(
                vec![SnapshotWorksetItemReference {
                    id: "item_api".to_string(),
                    title: Some("Stripe Integration Docs".to_string()),
                    item_type: "url".to_string(),
                    relevance: WorksetItemRelevance::Primary,
                    relevance_score: 0.75,
                    provenance: InformationProvenance::Observed,
                    app_name: Some("Safari".to_string()),
                    domain: Some("stripe.com".to_string()),
                    content_snippet: "API documentation".to_string(),
                    reasons: vec!["Relevant to API integration".to_string()],
                    created_at: "2026-09-19T00:00:00Z".to_string(),
                    last_active_at: None,
                }],
                0.70,
            ))
            .resume_readiness(ReconstructionResumeReadinessSection::partial(
                vec!["Task identified with partial confidence".to_string()],
                vec!["Single workset reference".to_string()],
                "Partial reconstruction available.",
            ))
            .confidence(ReconstructionConfidence::medium(0.65))
            .build();

        assert_eq!(reconstruction.context.status, ReconstructionSectionStatus::Available);
        assert_eq!(reconstruction.current_task.status, ReconstructionSectionStatus::Partial);
        assert_eq!(reconstruction.current_task.title.as_deref(), Some("API Integration"));
        assert_eq!(reconstruction.current_state.status, ReconstructionSectionStatus::Partial);
        assert_eq!(reconstruction.workset.status, ReconstructionSectionStatus::Partial);
        assert_eq!(reconstruction.resume_readiness.status, ReconstructionSectionStatus::Partial);
        assert_eq!(reconstruction.resume_readiness.readiness, ResumeReadiness::Partial);
        assert!(!reconstruction.resume_readiness.can_resume);
    }

    #[test]
    fn test_json_serialization_and_deserialization_roundtrip() {
        let reconstruction = ContextReconstructionBuilder::new()
            .context(ReconstructionContextSection::available(
                "ctx_123",
                "ContextSplit App",
                Some("active".to_string()),
                InformationProvenance::Confirmed,
                0.95,
            ))
            .current_task(ReconstructionTaskSection::partial("Context Engine Refactoring", 0.75))
            .current_state(ReconstructionStateSection::available(
                ContextStateValue::Active,
                StateFreshness::Fresh,
                "Active now",
                Some("2026-09-19T00:15:00Z".to_string()),
                Some("Active developer workspace".to_string()),
            ))
            .resume_readiness(ReconstructionResumeReadinessSection::ready(
                vec!["Active session".to_string()],
                "Ready to resume.",
            ))
            .confidence(ReconstructionConfidence::high(0.88))
            .build();

        let json_str = serde_json::to_string(&reconstruction).expect("Must serialize to JSON");
        assert!(json_str.contains("\"context\":"));
        assert!(json_str.contains("\"current_task\":"));
        assert!(json_str.contains("\"current_state\":"));
        assert!(json_str.contains("\"last_meaningful_activity\":"));
        assert!(json_str.contains("\"workset\":"));
        assert!(json_str.contains("\"important_memories\":"));
        assert!(json_str.contains("\"latest_snapshot\":"));
        assert!(json_str.contains("\"recent_activity\":"));
        assert!(json_str.contains("\"resume_readiness\":"));
        assert!(json_str.contains("\"confidence\":"));
        assert!(json_str.contains("\"reconstructed_at\":"));
        assert!(json_str.contains("\"summary_statement\":"));
        assert!(json_str.contains("\"epistemic_disclaimer\":"));

        let deserialized: ContextReconstruction = serde_json::from_str(&json_str)
            .expect("Must deserialize from JSON");
        assert_eq!(deserialized, reconstruction);
    }

    #[test]
    fn test_section_status_helper_methods() {
        assert!(ReconstructionSectionStatus::Available.is_available());
        assert!(!ReconstructionSectionStatus::Available.is_partial());
        assert!(!ReconstructionSectionStatus::Available.is_uncertain());
        assert!(!ReconstructionSectionStatus::Available.is_unavailable());
        assert!(ReconstructionSectionStatus::Available.is_present());

        assert!(ReconstructionSectionStatus::Partial.is_partial());
        assert!(ReconstructionSectionStatus::Partial.is_present());

        assert!(ReconstructionSectionStatus::Uncertain.is_uncertain());
        assert!(!ReconstructionSectionStatus::Uncertain.is_present());

        assert!(ReconstructionSectionStatus::Unavailable.is_unavailable());
        assert!(!ReconstructionSectionStatus::Unavailable.is_present());

        assert_eq!(ReconstructionSectionStatus::Available.as_str(), "available");
        assert_eq!(ReconstructionSectionStatus::Partial.as_str(), "partial");
        assert_eq!(ReconstructionSectionStatus::Uncertain.as_str(), "uncertain");
        assert_eq!(ReconstructionSectionStatus::Unavailable.as_str(), "unavailable");
    }

    #[test]
    fn test_privacy_scrubbing_in_reconstruction() {
        let sensitive_title = "GitHub Token: ghp_12345678901234567890123456789012 in Project Aurora";
        let context_sec = ReconstructionContextSection::available(
            "ctx_sec",
            sensitive_title,
            None,
            InformationProvenance::Inferred,
            0.80,
        );

        // Verify that sensitive tokens are scrubbed
        assert!(!context_sec.name.contains("ghp_12345678901234567890123456789012"));
        assert!(context_sec.name.contains("[API_KEY_REDACTED]"));
    }

    // =========================================================================
    // Service Integration & Pipeline Unit Tests
    // =========================================================================

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();

        let settings = crate::db::settings::PrivacySettings {
            enable_active_app: true,
            enable_window_title: true,
            enable_browser_domain: true,
            enable_session_tracking: true,
            enable_semantic_processing: true,
            semantic_provider: "local".to_string(),
            enable_semantic_suggestions: true,
            enable_ai_reasoning: false,
            ai_gating_enabled: false,
            ai_gating_confidence_threshold: 0.9,
            capture_paused: false,
            enable_context_snapshots: true,
        };
        crate::db::settings::update_settings(&conn, settings).unwrap();
        conn
    }

    #[test]
    fn test_service_reconstruct_context_complete_aurora_pipeline() {
        let conn = setup_test_db();
        let ws = crate::db::operations::create_workspace(&conn, "Aurora Website").unwrap();

        // Add 3 high-relevance items
        for i in 1..=3 {
            let meta = serde_json::json!({
                "app_name": "Figma",
                "domain": "figma.com",
                "window_title": format!("Hero Animation Part {}", i),
            }).to_string();

            let item = crate::db::operations::create_item(
                &conn,
                Some(&ws.id),
                None,
                "url",
                &format!("https://figma.com/file/hero_{}", i),
                Some(&format!("Hero Animation Part {}", i)),
                &meta,
            ).unwrap();

            crate::db::operations::confirm_membership(&conn, &item.id, &ws.id).unwrap();
        }

        // Add active task candidate
        let tc = crate::db::models::TaskCandidate {
            id: "tc_hero".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Hero animation".to_string(),
            confidence: 0.88,
            status: TaskCandidateStatus::Active,
            first_detected_at: Utc::now().to_rfc3339(),
            last_active_at: Utc::now().to_rfc3339(),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };
        crate::db::operations::create_task_candidate(&conn, &tc).unwrap();

        let service = ContextReconstructionService::new();
        let reconstruction = service.reconstruct_context(&conn, &ws.id).unwrap();

        assert_eq!(reconstruction.context.status, ReconstructionSectionStatus::Available);
        assert_eq!(reconstruction.context.name, "Aurora Website");
        assert_eq!(reconstruction.current_task.status, ReconstructionSectionStatus::Available);
        assert_eq!(reconstruction.current_task.title.as_deref(), Some("Hero animation"));
        assert_eq!(reconstruction.workset.status, ReconstructionSectionStatus::Available);
        assert!(reconstruction.workset.total_count >= 2);
        assert_eq!(reconstruction.resume_readiness.readiness, ResumeReadiness::Ready);
        assert!(reconstruction.resume_readiness.can_resume);
        assert!(reconstruction.confidence.score >= 0.70);
        assert!(reconstruction.is_resumable());
    }

    #[test]
    fn test_service_reconstruct_context_snapshots_disabled_graceful_degradation() {
        let conn = setup_test_db();
        // Disable snapshots
        let mut settings = crate::db::settings::get_settings(&conn).unwrap();
        settings.enable_context_snapshots = false;
        crate::db::settings::update_settings(&conn, settings).unwrap();

        let ws = crate::db::operations::create_workspace(&conn, "Aurora Website").unwrap();
        let item = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Color palette tokens",
            Some("Color tokens"),
            "{}",
        ).unwrap();
        crate::db::operations::confirm_membership(&conn, &item.id, &ws.id).unwrap();

        let service = ContextReconstructionService::new();
        let reconstruction = service.reconstruct_context(&conn, &ws.id).unwrap();

        // Snapshot is unavailable due to user settings
        assert_eq!(reconstruction.latest_snapshot.status, ReconstructionSectionStatus::Unavailable);
        // But the rest of the reconstruction succeeds seamlessly!
        assert_eq!(reconstruction.context.status, ReconstructionSectionStatus::Available);
        assert_eq!(reconstruction.context.name, "Aurora Website");
    }

    #[test]
    fn test_service_reconstruct_context_no_task_no_fabrication() {
        let conn = setup_test_db();
        let ws = crate::db::operations::create_workspace(&conn, "Backend Service").unwrap();
        let item = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Config values",
            Some("Config"),
            "{}",
        ).unwrap();
        crate::db::operations::confirm_membership(&conn, &item.id, &ws.id).unwrap();

        let service = ContextReconstructionService::new();
        let reconstruction = service.reconstruct_context(&conn, &ws.id).unwrap();

        // Invariant: No active task candidate exists; must NOT fabricate one.
        assert_eq!(reconstruction.current_task.status, ReconstructionSectionStatus::Unavailable);
        assert_eq!(reconstruction.current_task.title, None);
    }

    #[test]
    fn test_service_reconstruct_context_unknown_workspace() {
        let conn = setup_test_db();
        let service = ContextReconstructionService::new();
        let reconstruction = service.reconstruct_context(&conn, "non_existent_workspace_id").unwrap();

        // Invariant: Missing workspace degrades gracefully with Uncertain status and Insufficient readiness
        assert_eq!(reconstruction.context.status, ReconstructionSectionStatus::Uncertain);
        assert_eq!(reconstruction.resume_readiness.readiness, ResumeReadiness::Insufficient);
        assert!(!reconstruction.resume_readiness.can_resume);
        assert!(!reconstruction.is_resumable());
    }

    #[test]
    fn test_service_reconstruct_context_dismissed_item_veto() {
        let conn = setup_test_db();
        let ws = crate::db::operations::create_workspace(&conn, "Design System").unwrap();

        // Add 1 confirmed item
        let item1 = crate::db::operations::create_item(&conn, Some(&ws.id), None, "url", "https://figma.com", Some("Figma DS"), "{}").unwrap();
        crate::db::operations::confirm_membership(&conn, &item1.id, &ws.id).unwrap();

        // Add 1 dismissed item
        let item2 = crate::db::operations::create_item(&conn, Some(&ws.id), None, "url", "https://spam.com", Some("Spam Link"), "{}").unwrap();
        crate::db::operations::dismiss_membership(&conn, &item2.id, &ws.id).unwrap();

        let service = ContextReconstructionService::new();
        let reconstruction = service.reconstruct_context(&conn, &ws.id).unwrap();

        // Invariant: Dismissed item must never appear in workset
        let item_ids: Vec<_> = reconstruction.workset.items.iter().map(|i| i.id.as_str()).collect();
        assert!(!item_ids.contains(&item2.id.as_str()));
    }

    #[test]
    fn test_service_reconstruct_context_completed_task_not_active() {
        let conn = setup_test_db();
        let ws = crate::db::operations::create_workspace(&conn, "Core App").unwrap();

        // Create inactive task candidate
        let tc = crate::db::models::TaskCandidate {
            id: "tc_mig".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Finished Migration".to_string(),
            confidence: 0.95,
            status: TaskCandidateStatus::Inactive,
            first_detected_at: Utc::now().to_rfc3339(),
            last_active_at: Utc::now().to_rfc3339(),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };
        crate::db::operations::create_task_candidate(&conn, &tc).unwrap();

        let service = ContextReconstructionService::new();
        let reconstruction = service.reconstruct_context(&conn, &ws.id).unwrap();

        // Invariant: Inactive/completed task candidate must not be reconstructed as active
        assert_eq!(reconstruction.current_task.status, ReconstructionSectionStatus::Unavailable);
        assert_eq!(reconstruction.current_task.title, None);
    }

    #[test]
    fn test_service_deterministic_output() {
        let conn = setup_test_db();
        let ws = crate::db::operations::create_workspace(&conn, "Deterministic Project").unwrap();

        let service = ContextReconstructionService::new();
        let rec1 = service.reconstruct_context(&conn, &ws.id).unwrap();
        let rec2 = service.reconstruct_context(&conn, &ws.id).unwrap();

        assert_eq!(rec1.context, rec2.context);
        assert_eq!(rec1.current_task, rec2.current_task);
        assert_eq!(rec1.current_state, rec2.current_state);
        assert_eq!(rec1.workset, rec2.workset);
        assert_eq!(rec1.resume_readiness, rec2.resume_readiness);
        assert_eq!(rec1.summary_statement, rec2.summary_statement);
    }

    #[test]
    fn test_service_reconstruct_all_and_active() {
        let conn = setup_test_db();
        let _ws1 = crate::db::operations::create_workspace(&conn, "Workspace A").unwrap();
        let _ws2 = crate::db::operations::create_workspace(&conn, "Workspace B").unwrap();

        let service = ContextReconstructionService::new();
        let all = service.reconstruct_all(&conn).unwrap();
        assert_eq!(all.len(), 2);
        let names: Vec<_> = all.iter().map(|r| r.context.name.as_str()).collect();
        assert!(names.contains(&"Workspace A"));
        assert!(names.contains(&"Workspace B"));
    }
}

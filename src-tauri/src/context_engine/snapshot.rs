use std::collections::HashSet;

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::context_engine::context_state::{
    ContextState, ContextStateBuilder, ContextStateValue, MeaningfulActivity,
    ResumeReadiness, ResumeReadinessAssessment, ResumeReadinessEvaluator, StateFreshness,
};
use crate::context_engine::memory::{
    ContextMemory, ContextMemoryActivityState, ContextMemoryBuilder, InformationProvenance,
    MemoryActivitySequenceEntry, MemoryItemReference, MemoryTaskReference,
};
use crate::context_engine::workset::{
    ContextWorkset, ContextWorksetBuilder, ContextWorksetService, WorksetItemReference,
    WorksetItemRelevance,
};
use crate::db::models::TaskCandidate;

// =========================================================================
// Standard Epistemic Disclaimer
// =========================================================================

pub const CONTEXTUAL_SNAPSHOT_DISCLAIMER: &str =
    "This snapshot is an algorithmic contextual reconstruction based on recorded activity signals. It does not represent an exact recording of user attention or mental state.";

// =========================================================================
// Helpers for Parsing & Timestamp Math
// =========================================================================

#[allow(dead_code)]
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

impl From<&TaskCandidate> for SnapshotTaskReference {
    fn from(task: &TaskCandidate) -> Self {
        let band = if task.confidence >= 0.75 {
            "High"
        } else if task.confidence >= 0.50 {
            "Medium"
        } else if task.confidence >= 0.35 {
            "Low"
        } else {
            "Insufficient"
        };

        Self {
            id: Some(task.id.clone()),
            title: task.title.clone(),
            action_verb: None,
            target_subject: None,
            confidence: task.confidence,
            confidence_band: band.to_string(),
            provenance: InformationProvenance::Inferred,
            first_seen_at: Some(task.first_detected_at.clone()),
            last_active_at: Some(task.last_active_at.clone()),
            status: Some(task.status.as_str().to_string()),
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
// ContextSnapshotQuality: Step 4 Quality Control & Readiness Classification
// =========================================================================

/// Qualitative classification rating for a contextual snapshot.
///
/// Invariants:
/// - `Complete`: Rich contextual state (clear task, strong workset, recent activity, fresh state, ready).
/// - `Useful`: Sufficient meaningful information to resume work without friction.
/// - `Partial`: Context identified, but missing key details (uncertain task, weak workset, or stale activity).
/// - `Insufficient`: Missing meaningful task, weak or isolated item, stale/old/no activity, or conflicting signals.
///   AN INSUFFICIENT SNAPSHOT MUST NEVER BE PRESENTED AS A CONFIDENT RESUME STATE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotQualityRating {
    /// Full, rich contextual state available.
    Complete,
    /// Sufficient meaningful information to resume work without friction.
    Useful,
    /// Context identified, but missing key details.
    Partial,
    /// Insufficient evidence or conflicting signals.
    Insufficient,
}

impl SnapshotQualityRating {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Useful => "useful",
            Self::Partial => "partial",
            Self::Insufficient => "insufficient",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Complete => "Complete",
            Self::Useful => "Useful",
            Self::Partial => "Partial",
            Self::Insufficient => "Insufficient",
        }
    }

    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Complete)
    }

    pub fn is_useful(&self) -> bool {
        matches!(self, Self::Useful)
    }

    pub fn is_partial(&self) -> bool {
        matches!(self, Self::Partial)
    }

    pub fn is_insufficient(&self) -> bool {
        matches!(self, Self::Insufficient)
    }

    pub fn is_usable(&self) -> bool {
        matches!(self, Self::Complete | Self::Useful)
    }
}

impl std::fmt::Display for SnapshotQualityRating {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Comprehensive quality control evaluation of a ContextSnapshot.
///
/// Evaluates:
/// - context confidence
/// - task confidence
/// - workset quality
/// - recent activity
/// - state freshness
/// - evidence diversity
/// - contradictions
/// - resume readiness
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextSnapshotQuality {
    /// Qualitative classification rating: complete, useful, partial, insufficient
    pub rating: SnapshotQualityRating,
    /// Overall calibrated composite quality score [0.0, 1.0]
    pub score: f64,
    /// Calibrated context confidence score [0.0, 1.0]
    pub context_confidence: f64,
    /// Calibrated task confidence score [0.0, 1.0] (nullable if task is absent)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_confidence: Option<f64>,
    /// Quality score of the workset [0.0, 1.0]
    pub workset_quality_score: f64,
    /// Number of strong items in workset (Primary/Secondary or score >= 0.70)
    pub strong_workset_count: usize,
    /// Total count of items in the workset
    pub total_workset_count: usize,
    /// Number of recent activity sequence references
    pub recent_activity_count: usize,
    /// Whether a verified last meaningful activity was identified
    pub has_meaningful_activity: bool,
    /// State freshness classification
    pub state_freshness: StateFreshness,
    /// Evidence diversity score across distinct apps and item types [0.0, 1.0]
    pub evidence_diversity_score: f64,
    /// Number of distinct applications represented in workset/activity
    pub distinct_apps_count: usize,
    /// Whether conflicting signals or contradictions were detected
    pub has_contradictions: bool,
    /// Specific contradiction details if detected
    pub contradictions: Vec<String>,
    /// Calibrated evaluation reasons
    pub reasons: Vec<String>,
    /// Human-readable explanation block
    pub explanation: String,
    /// CRITICAL INVARIANT: Flag indicating whether this snapshot can be safely presented
    /// as a confident resume state. Strictly `false` for Insufficient and low-confidence snapshots.
    pub can_confidently_resume: bool,
}

impl Default for ContextSnapshotQuality {
    fn default() -> Self {
        Self {
            rating: SnapshotQualityRating::Insufficient,
            score: 0.0,
            context_confidence: 0.0,
            task_confidence: None,
            workset_quality_score: 0.0,
            strong_workset_count: 0,
            total_workset_count: 0,
            recent_activity_count: 0,
            has_meaningful_activity: false,
            state_freshness: StateFreshness::Fresh,
            evidence_diversity_score: 0.0,
            distinct_apps_count: 0,
            has_contradictions: false,
            contradictions: Vec::new(),
            reasons: vec!["Insufficient evidence to evaluate quality".to_string()],
            explanation: "Insufficient evidence to evaluate snapshot quality.".to_string(),
            can_confidently_resume: false,
        }
    }
}

/// Evaluator for assessing snapshot quality and ensuring insufficient snapshots
/// are never presented as confident resume states.
pub struct ContextSnapshotQualityEvaluator;

impl ContextSnapshotQualityEvaluator {
    pub fn new() -> Self {
        Self
    }

    /// Evaluates all contextual dimensions of a snapshot without arbitrary numerical jumps.
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate(
        context_confidence: f64,
        task_confidence: Option<f64>,
        task_title: Option<&str>,
        workset_items: &[SnapshotWorksetItemReference],
        recent_activity: &[SnapshotActivityReference],
        last_meaningful_activity: Option<&MeaningfulActivity>,
        state: ContextStateValue,
        state_freshness: StateFreshness,
        resume_readiness: &ResumeReadinessAssessment,
    ) -> ContextSnapshotQuality {
        let mut reasons: Vec<String> = Vec::new();
        let mut contradictions: Vec<String> = Vec::new();

        // 1. Context Confidence
        let ctx_conf = context_confidence.clamp(0.0, 1.0);

        // 2. Task Confidence & Presence
        let has_task = task_title.is_some() && !task_title.unwrap().trim().is_empty();
        let effective_task_conf = if has_task {
            task_confidence.unwrap_or(0.65).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // 3. Workset Quality
        let total_workset = workset_items.len();
        let strong_workset_count = workset_items
            .iter()
            .filter(|i| {
                matches!(
                    i.relevance,
                    WorksetItemRelevance::Primary | WorksetItemRelevance::Supporting
                ) || i.relevance_score >= 0.70
            })
            .count();
        let weak_workset_count = workset_items
            .iter()
            .filter(|i| {
                matches!(i.relevance, WorksetItemRelevance::Related)
                    || i.relevance_score < 0.40
            })
            .count();

        let workset_quality_score = if total_workset == 0 {
            0.0
        } else {
            let base_score = (strong_workset_count as f64 * 1.0
                + (total_workset.saturating_sub(strong_workset_count + weak_workset_count)) as f64 * 0.6
                + weak_workset_count as f64 * 0.2)
                / (total_workset as f64);
            let count_scaling = (total_workset as f64 / 4.0).min(1.0);
            (base_score * 0.7 + count_scaling * 0.3).clamp(0.0, 1.0)
        };

        // 4. Recent Activity & Meaningful Focus
        let has_meaningful_act = last_meaningful_activity.is_some();
        let recent_activity_count = recent_activity.len();
        let has_recent_activity = recent_activity_count > 0
            || has_meaningful_act
            || state == ContextStateValue::Active
            || state == ContextStateValue::RecentlyActive
            || state_freshness == StateFreshness::Fresh;

        // 5. State Freshness Score
        let freshness_score = match state_freshness {
            StateFreshness::Fresh => 1.00,
            StateFreshness::Recent => 0.85,
            StateFreshness::Stale => 0.45,
            StateFreshness::VeryStale => 0.15,
        };

        // 6. Evidence Diversity
        let mut distinct_apps = HashSet::new();
        let mut distinct_types = HashSet::new();
        for item in workset_items {
            if let Some(ref app) = item.app_name {
                if !app.trim().is_empty() {
                    distinct_apps.insert(app.to_lowercase());
                }
            }
            if !item.item_type.trim().is_empty() {
                distinct_types.insert(item.item_type.to_lowercase());
            }
        }
        for act in recent_activity {
            distinct_types.insert(act.entry_type.to_lowercase());
        }
        let distinct_apps_count = distinct_apps.len();
        let evidence_diversity_score = if total_workset == 0 && recent_activity_count == 0 {
            0.0
        } else if distinct_apps_count >= 3 || (distinct_apps_count >= 2 && distinct_types.len() >= 2) {
            1.00
        } else if distinct_apps_count >= 2 || (total_workset >= 2 && distinct_types.len() >= 2) {
            0.80
        } else if total_workset >= 2 {
            0.60
        } else {
            0.35
        };

        // 7. Contradictions & Conflict Detection
        if state == ContextStateValue::Uncertain {
            contradictions.push("Ambiguous or conflicting contextual signals detected".to_string());
        }
        if state == ContextStateValue::InsufficientEvidence {
            contradictions.push("Insufficient evidence to reconstruct working state".to_string());
        }
        if state == ContextStateValue::Dormant && state_freshness == StateFreshness::Fresh {
            contradictions.push("State is marked dormant despite recent activity timestamp".to_string());
        }
        if has_task && effective_task_conf < 0.40 {
            contradictions.push("Task candidate confidence is below reliable threshold".to_string());
        }
        let has_contradictions = !contradictions.is_empty();

        // 8. Rating Classification (Inspecting calibrated bands)
        let is_dormant = state == ContextStateValue::Dormant || state_freshness == StateFreshness::VeryStale;
        let is_insufficient_state = state == ContextStateValue::InsufficientEvidence
            || resume_readiness.readiness == ResumeReadiness::Insufficient;

        let rating = if is_insufficient_state
            || (total_workset == 0 && !has_task && !has_meaningful_act)
            || (total_workset <= 1 && weak_workset_count == 1 && !has_task && (is_dormant || !has_recent_activity))
            || (total_workset <= 1 && !has_task && is_dormant)
            || (ctx_conf < 0.35 && total_workset <= 1 && !has_task)
            || (has_contradictions && ctx_conf < 0.50)
        {
            SnapshotQualityRating::Insufficient
        } else if has_task
            && effective_task_conf >= 0.70
            && strong_workset_count >= 2
            && total_workset >= 3
            && (has_meaningful_act || recent_activity_count >= 2)
            && !is_dormant
            && (state_freshness == StateFreshness::Fresh || state_freshness == StateFreshness::Recent)
            && !has_contradictions
            && (resume_readiness.readiness == ResumeReadiness::Ready)
        {
            SnapshotQualityRating::Complete
        } else if !is_dormant
            && state_freshness != StateFreshness::VeryStale
            && (resume_readiness.readiness == ResumeReadiness::Ready || resume_readiness.readiness == ResumeReadiness::MostlyReady)
            && (
                (has_task && effective_task_conf >= 0.50 && total_workset >= 1 && has_recent_activity)
                || (strong_workset_count >= 2 && (has_meaningful_act || recent_activity_count >= 2))
                || (total_workset >= 3 && has_recent_activity)
            )
        {
            SnapshotQualityRating::Useful
        } else {
            SnapshotQualityRating::Partial
        };

        // 9. Reasons Formulation
        if has_task {
            reasons.push(format!(
                "Task identified ('{}', {:.0}% confidence)",
                task_title.unwrap(),
                effective_task_conf * 100.0
            ));
        } else {
            reasons.push("No specific active task identified".to_string());
        }

        if strong_workset_count > 0 {
            reasons.push(format!(
                "{} strong workset {} available (out of {} total)",
                strong_workset_count,
                if strong_workset_count == 1 { "item" } else { "items" },
                total_workset
            ));
        } else if total_workset > 0 {
            reasons.push(format!("{} peripheral/weak workset items", total_workset));
        } else {
            reasons.push("No workset items captured".to_string());
        }

        if has_meaningful_act {
            reasons.push("Verified meaningful activity milestone recorded".to_string());
        } else if has_recent_activity {
            reasons.push("Recent activity signals present".to_string());
        } else {
            reasons.push("No recent activity recorded".to_string());
        }

        reasons.push(format!("State freshness: {}", state_freshness.as_str()));
        if distinct_apps_count > 1 {
            reasons.push(format!("Evidence spanned across {} applications", distinct_apps_count));
        }

        // 10. Calibrated Composite Score [0.0, 1.0]
        let mut score = (ctx_conf * 0.25)
            + (effective_task_conf * 0.25)
            + (workset_quality_score * 0.25)
            + (freshness_score * 0.15)
            + (evidence_diversity_score * 0.10);

        if has_contradictions {
            score *= 0.70;
        }

        match rating {
            SnapshotQualityRating::Complete => {
                score = score.max(0.80).clamp(0.0, 1.0);
            }
            SnapshotQualityRating::Useful => {
                score = score.clamp(0.60, 0.89);
            }
            SnapshotQualityRating::Partial => {
                score = score.clamp(0.35, 0.65);
            }
            SnapshotQualityRating::Insufficient => {
                score = score.min(0.35).clamp(0.0, 1.0);
            }
        }

        // 11. CRITICAL INVARIANT: can_confidently_resume
        let can_confidently_resume = match rating {
            SnapshotQualityRating::Complete => true,
            SnapshotQualityRating::Useful => true,
            SnapshotQualityRating::Partial => false,
            SnapshotQualityRating::Insufficient => false,
        };

        let explanation = format!(
            "Quality Rating: {} (Score: {:.0}%). {}",
            rating.display_name(),
            score * 100.0,
            reasons.join(". ")
        );

        ContextSnapshotQuality {
            rating,
            score,
            context_confidence: ctx_conf,
            task_confidence: if has_task { Some(effective_task_conf) } else { None },
            workset_quality_score,
            strong_workset_count,
            total_workset_count: total_workset,
            recent_activity_count,
            has_meaningful_activity: has_meaningful_act,
            state_freshness,
            evidence_diversity_score,
            distinct_apps_count,
            has_contradictions,
            contradictions,
            reasons,
            explanation,
            can_confidently_resume,
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
    /// Evaluated quality control assessment
    #[serde(default)]
    pub quality: ContextSnapshotQuality,
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

        let quality = ContextSnapshotQualityEvaluator::evaluate(
            snapshot_confidence,
            task_confidence,
            task_title.as_deref(),
            &workset_item_references,
            &recent_activity_references,
            last_meaningful_activity.as_ref(),
            state,
            state_freshness,
            &resume_readiness,
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
            quality,
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

        let quality = ContextSnapshotQualityEvaluator::evaluate(
            state.state_confidence,
            if state.task_title.is_some() {
                Some(state.state_confidence)
            } else {
                None
            },
            state.task_title.as_deref(),
            &workset_refs,
            &[],
            state.meaningful_activity.as_ref(),
            state.state,
            state.freshness,
            &state.readiness,
        );

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
            quality,
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

        let has_recent_activity = !activity_refs.is_empty()
            || state_val == ContextStateValue::Active
            || state_val == ContextStateValue::RecentlyActive
            || freshness == StateFreshness::Fresh;

        let readiness = ResumeReadinessEvaluator::evaluate_with_freshness(
            state_val,
            active_task_title.as_deref(),
            workset_refs.len(),
            meaningful_activity.is_some(),
            has_recent_activity,
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

        let quality = ContextSnapshotQualityEvaluator::evaluate(
            memory.current_context_confidence,
            memory.current_task_confidence,
            active_task_title.as_deref(),
            &workset_refs,
            &activity_refs,
            meaningful_activity.as_ref(),
            state_val,
            freshness,
            &readiness,
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
            quality,
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
        if state == ContextStateValue::InsufficientEvidence
            || (workset_items.is_empty()
                && task_title.is_none()
                && last_meaningful_activity.is_none())
        {
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
        } else if let Some(primary) = workset_items
            .iter()
            .find(|i| i.relevance == WorksetItemRelevance::Primary)
        {
            let item_name = primary.title.as_deref().unwrap_or("active document");
            if let Some(ref app) = primary.app_name {
                parts.push(format!("last focused on '{}' in {}", item_name, app));
            } else {
                parts.push(format!("last focused on '{}'", item_name));
            }
        }

        // 3. Supporting materials count
        let supporting_count = workset_items
            .iter()
            .filter(|i| {
                i.relevance == WorksetItemRelevance::Primary
                    || i.relevance == WorksetItemRelevance::Supporting
            })
            .count();
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
        matches!(
            self.state,
            ContextStateValue::Active | ContextStateValue::RecentlyActive
        )
    }

    /// Returns `true` if this snapshot is marked dormant.
    pub fn is_dormant(&self) -> bool {
        self.state == ContextStateValue::Dormant
            || self.state_freshness == StateFreshness::VeryStale
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

    /// Evaluates snapshot quality rating: Complete, Useful, Partial, or Insufficient.
    pub fn evaluate_quality(&self) -> ContextSnapshotQuality {
        ContextSnapshotQualityEvaluator::evaluate(
            self.snapshot_confidence,
            self.task_confidence,
            self.task_title.as_deref(),
            &self.workset_item_references,
            &self.recent_activity_references,
            self.last_meaningful_activity.as_ref(),
            self.state,
            self.state_freshness,
            &self.resume_readiness,
        )
    }

    /// Returns `true` if this snapshot has complete quality rating.
    pub fn is_complete(&self) -> bool {
        self.quality.rating.is_complete()
    }

    /// Returns `true` if this snapshot is useful to resume work without friction.
    pub fn is_useful(&self) -> bool {
        self.quality.rating.is_useful()
    }

    /// Returns `true` if this snapshot is partial.
    pub fn is_partial(&self) -> bool {
        self.quality.rating.is_partial()
    }

    /// Returns `true` if this snapshot is insufficient.
    pub fn is_insufficient(&self) -> bool {
        self.quality.rating.is_insufficient()
    }

    /// CRITICAL INVARIANT: Returns `true` only if this snapshot can be confidently presented
    /// as a resume state. Strictly `false` for Insufficient or Partial snapshots.
    pub fn can_confidently_resume(&self) -> bool {
        self.quality.can_confidently_resume
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
    pub quality: Option<ContextSnapshotQuality>,
    pub provenance: InformationProvenance,
    pub reference_time: Option<DateTime<Utc>>,
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
            quality: None,
            provenance: InformationProvenance::Inferred,
            reference_time: None,
        }
    }

    /// Loads underlying sources from SQLite database and constructs the full builder.
    pub fn from_db(conn: &Connection, context_id: &str) -> Result<Self, String> {
        let ws = crate::db::operations::get_workspace(conn, context_id).ok();
        let context_name = ws
            .as_ref()
            .map(|w| w.name.clone())
            .unwrap_or_else(|| "Workspace".to_string());

        // 1. Derive ContextState
        let state_builder = ContextStateBuilder::from_db(conn, context_id)?;
        let state = state_builder.build().ok();

        // 2. Derive ContextWorkset
        let workset = ContextWorksetBuilder::from_db(conn, context_id)
            .and_then(|b| b.build())
            .ok();

        // 3. Derive ContextMemory
        let memory = ContextMemoryBuilder::from_db(conn, context_id)
            .and_then(|b| b.build())
            .ok();

        // 4. Workset item references
        let workset_refs: Vec<SnapshotWorksetItemReference> = workset
            .as_ref()
            .map(|ws| {
                ws.items
                    .iter()
                    .map(SnapshotWorksetItemReference::from)
                    .collect()
            })
            .unwrap_or_default();

        // 5. Activity references
        let activity_refs: Vec<SnapshotActivityReference> = memory
            .as_ref()
            .map(|mem| {
                mem.activity_sequence
                    .iter()
                    .map(SnapshotActivityReference::from)
                    .collect()
            })
            .unwrap_or_default();

        // 6. Task references
        let task_refs: Vec<SnapshotTaskReference> = memory
            .as_ref()
            .map(|mem| {
                mem.recent_tasks
                    .iter()
                    .map(SnapshotTaskReference::from)
                    .collect()
            })
            .unwrap_or_default();

        // 7. Last meaningful activity
        let meaningful_activity = state.as_ref().and_then(|s| s.meaningful_activity.clone());

        // 8. Determine active task
        let (task_id, task_title, task_conf, task_prov) = if let Some(ref st) = state {
            if let Some(ref title) = st.task_title {
                let prov = if st.has_user_feedback {
                    InformationProvenance::Confirmed
                } else {
                    InformationProvenance::Inferred
                };
                (
                    st.task_id.clone(),
                    Some(title.clone()),
                    Some(st.state_confidence),
                    Some(prov),
                )
            } else {
                (None, None, None, None)
            }
        } else if let Some(ref ws_obj) = workset {
            if let Some(ref title) = ws_obj.task_title {
                (
                    ws_obj.task_id.clone(),
                    Some(title.clone()),
                    Some(ws_obj.confidence),
                    Some(ws_obj.provenance),
                )
            } else {
                (None, None, None, None)
            }
        } else {
            (None, None, None, None)
        };

        // 9. State & Freshness
        let state_val = state
            .as_ref()
            .map(|s| s.state)
            .unwrap_or(ContextStateValue::InsufficientEvidence);

        let freshness = state
            .as_ref()
            .map(|s| s.freshness)
            .unwrap_or(StateFreshness::VeryStale);

        let freshness_display = state
            .as_ref()
            .map(|s| s.freshness_display.clone())
            .unwrap_or_else(|| freshness.display_wording(86400 * 30));

        let last_activity_at = state
            .as_ref()
            .and_then(|s| s.last_active_at.clone())
            .or_else(|| ws.and_then(|w| w.last_activity_at));

        let readiness = state.as_ref().map(|s| s.readiness.clone());

        let snapshot_confidence = state.as_ref().map(|s| s.state_confidence).unwrap_or(0.0);

        let provenance = if state.as_ref().map(|s| s.has_user_feedback).unwrap_or(false) {
            InformationProvenance::Confirmed
        } else if !workset_refs.is_empty() {
            InformationProvenance::Observed
        } else {
            InformationProvenance::Inferred
        };

        Ok(Self {
            context_id: context_id.to_string(),
            context_name,
            task_id,
            task_title,
            task_provenance: task_prov,
            task_confidence: task_conf,
            last_activity_at,
            state: state_val,
            state_freshness: freshness,
            state_freshness_display: freshness_display,
            resume_readiness: readiness,
            workset_item_references: workset_refs,
            recent_activity_references: activity_refs,
            recent_task_references: task_refs,
            last_meaningful_activity: meaningful_activity,
            snapshot_confidence,
            quality: None,
            provenance,
            reference_time: None,
        })
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

    pub fn with_quality(mut self, quality: ContextSnapshotQuality) -> Self {
        self.quality = Some(quality);
        self
    }

    pub fn with_reference_time(mut self, time: DateTime<Utc>) -> Self {
        self.reference_time = Some(time);
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

        let mut snapshot = ContextSnapshot::new(
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
        );

        if let Some(quality) = self.quality {
            snapshot.quality = quality;
        }

        snapshot
    }
}

// =========================================================================
// ContextSnapshotService: Deterministic Synthesis Service
// =========================================================================

/// Deterministic service for deriving and reconstructing point-in-time `ContextSnapshot`s.
///
/// Invariants:
/// 1. Reuses existing ContextState, ContextWorkset, and ContextMemory ranking without duplicating heuristics.
/// 2. Deterministic execution; zero random LLM calls.
/// 3. Never fabricates tasks, items, or actions.
pub struct ContextSnapshotService;

impl Default for ContextSnapshotService {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextSnapshotService {
    pub fn new() -> Self {
        Self
    }

    /// Derives the point-in-time `ContextSnapshot` for a specified context from the database.
    pub fn derive_for_context(
        &self,
        conn: &Connection,
        context_id: &str,
    ) -> Result<ContextSnapshot, String> {
        let builder = ContextSnapshotBuilder::from_db(conn, context_id)?;
        Ok(builder.build())
    }

    /// Derives the active `ContextSnapshot` across all workspaces.
    pub fn derive_active(&self, conn: &Connection) -> Result<Option<ContextSnapshot>, String> {
        let workset_service = ContextWorksetService::new();
        if let Ok(Some(active_ws)) = workset_service.derive_active(conn) {
            let snapshot = self.derive_for_context(conn, &active_ws.context_id)?;
            return Ok(Some(snapshot));
        }
        Ok(None)
    }

    /// Derives a snapshot evaluated with a specific reference time (e.g. historical departure).
    pub fn derive_at_departure(
        &self,
        conn: &Connection,
        context_id: &str,
        reference_time: Option<DateTime<Utc>>,
    ) -> Result<ContextSnapshot, String> {
        let mut builder = ContextSnapshotBuilder::from_db(conn, context_id)?;
        if let Some(ref_time) = reference_time {
            builder = builder.with_reference_time(ref_time);
        }
        Ok(builder.build())
    }
}

// =========================================================================
// Unit & Integration Tests (Day 20 Step 3)
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::TaskCandidateStatus;
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

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
            create_dummy_workset_item(
                "i1",
                "Hero Component",
                WorksetItemRelevance::Primary,
                0.95,
                Some("VS Code"),
            ),
            create_dummy_workset_item(
                "i2",
                "Framer Motion API",
                WorksetItemRelevance::Supporting,
                0.85,
                Some("Chrome"),
            ),
        ];

        let snapshot = ContextSnapshotBuilder::new("ws-aurora", "Aurora Website")
            .with_state(ContextStateValue::Active)
            .with_freshness(StateFreshness::Fresh, 300)
            .with_task(
                Some("t1".to_string()),
                "Hero Animation",
                0.92,
                InformationProvenance::Inferred,
            )
            .with_workset_items(items)
            .with_confidence(0.90)
            .build();

        assert_eq!(snapshot.state, ContextStateValue::Active);
        assert_eq!(snapshot.state_freshness, StateFreshness::Fresh);
        assert!(snapshot.is_working());
        assert!(!snapshot.is_dormant());
        assert!(snapshot.has_task());
        assert_eq!(snapshot.task_title.as_deref(), Some("Hero Animation"));
        assert_eq!(
            snapshot.task_provenance,
            Some(InformationProvenance::Inferred)
        );
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
            create_dummy_workset_item(
                "i1",
                "Dashboard Layout",
                WorksetItemRelevance::Primary,
                0.90,
                Some("Figma"),
            ),
            create_dummy_workset_item(
                "i2",
                "Tailwind Config",
                WorksetItemRelevance::Supporting,
                0.80,
                Some("VS Code"),
            ),
        ];

        let snapshot = ContextSnapshotBuilder::new("ws-dashboard", "Analytics Dashboard")
            .with_state(ContextStateValue::RecentlyActive)
            .with_freshness(StateFreshness::Recent, 14400) // 4 hours ago
            .with_task(
                Some("t2".to_string()),
                "Refactoring Dashboard",
                0.88,
                InformationProvenance::Inferred,
            )
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
        let items = vec![create_dummy_workset_item(
            "i1",
            "Legacy Specs",
            WorksetItemRelevance::Supporting,
            0.60,
            Some("Chrome"),
        )];

        let snapshot = ContextSnapshotBuilder::new("ws-old", "Legacy Project")
            .with_state(ContextStateValue::Dormant)
            .with_freshness(StateFreshness::VeryStale, 86400 * 10) // 10 days ago
            .with_task(
                Some("t3".to_string()),
                "Documentation",
                0.70,
                InformationProvenance::Inferred,
            )
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
            .with_task(
                Some("t-x".to_string()),
                "Authentication Flow",
                0.95,
                InformationProvenance::Confirmed,
            )
            .add_workset_item(create_dummy_workset_item(
                "i1",
                "OAuth Spec",
                WorksetItemRelevance::Primary,
                0.90,
                None,
            ))
            .add_workset_item(create_dummy_workset_item(
                "i2",
                "Token Handler",
                WorksetItemRelevance::Supporting,
                0.88,
                None,
            ))
            .build();

        assert!(snapshot.has_task());
        assert_eq!(snapshot.task_title.as_deref(), Some("Authentication Flow"));
        assert_eq!(
            snapshot.task_provenance,
            Some(InformationProvenance::Confirmed)
        );
        assert!(snapshot.snapshot_statement.contains("Authentication Flow"));
    }

    #[test]
    fn test_task_absent_snapshot() {
        let snapshot = ContextSnapshotBuilder::new("ws-2", "General Exploration")
            .with_state(ContextStateValue::RecentlyActive)
            .with_freshness(StateFreshness::Fresh, 2400)
            .without_task()
            .add_workset_item(create_dummy_workset_item(
                "i1",
                "Article 1",
                WorksetItemRelevance::Supporting,
                0.70,
                None,
            ))
            .add_workset_item(create_dummy_workset_item(
                "i2",
                "Article 2",
                WorksetItemRelevance::Supporting,
                0.65,
                None,
            ))
            .add_workset_item(create_dummy_workset_item(
                "i3",
                "Article 3",
                WorksetItemRelevance::Supporting,
                0.60,
                None,
            ))
            .build();

        assert!(!snapshot.has_task());
        assert_eq!(snapshot.task_title, None);
        assert_eq!(snapshot.task_provenance, None);
        assert_eq!(
            snapshot.resume_readiness.readiness,
            ResumeReadiness::MostlyReady
        );
        assert!(snapshot
            .snapshot_statement
            .contains("Working in General Exploration"));
    }

    #[test]
    fn test_strong_workset_snapshot() {
        let items = vec![
            create_dummy_workset_item(
                "i1",
                "Core API",
                WorksetItemRelevance::Primary,
                0.98,
                Some("VS Code"),
            ),
            create_dummy_workset_item(
                "i2",
                "Test Suite",
                WorksetItemRelevance::Primary,
                0.92,
                Some("Terminal"),
            ),
            create_dummy_workset_item(
                "i3",
                "Documentation",
                WorksetItemRelevance::Supporting,
                0.85,
                Some("Chrome"),
            ),
            create_dummy_workset_item(
                "i4",
                "Design Mock",
                WorksetItemRelevance::Supporting,
                0.80,
                Some("Figma"),
            ),
        ];

        let snapshot = ContextSnapshotBuilder::new("ws-strong", "Core Backend")
            .with_state(ContextStateValue::Active)
            .with_freshness(StateFreshness::Fresh, 600)
            .with_task(
                Some("t-backend".to_string()),
                "API Endpoints",
                0.94,
                InformationProvenance::Inferred,
            )
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
        let items = vec![create_dummy_workset_item(
            "i1",
            "Peripheral Note",
            WorksetItemRelevance::Reference,
            0.35,
            None,
        )];

        let snapshot = ContextSnapshotBuilder::new("ws-weak", "Sparse Workspace")
            .with_state(ContextStateValue::RecentlyActive)
            .with_freshness(StateFreshness::Recent, 86400)
            .without_task()
            .with_workset_items(items)
            .build();

        assert_eq!(snapshot.workset_item_references.len(), 1);
        assert_eq!(snapshot.primary_items().len(), 0);
        assert_eq!(snapshot.supporting_items().len(), 0);
        assert_eq!(
            snapshot.resume_readiness.readiness,
            ResumeReadiness::Partial
        );
    }

    #[test]
    fn test_insufficient_evidence_snapshot() {
        let snapshot = ContextSnapshotBuilder::new("ws-empty", "Brand New Workspace")
            .with_state(ContextStateValue::InsufficientEvidence)
            .with_freshness(StateFreshness::VeryStale, 86400 * 30)
            .without_task()
            .build();

        assert_eq!(snapshot.state, ContextStateValue::InsufficientEvidence);
        assert_eq!(
            snapshot.resume_readiness.readiness,
            ResumeReadiness::Insufficient
        );
        assert!(!snapshot.is_working());
        assert!(snapshot.workset_item_references.is_empty());
        assert!(snapshot
            .snapshot_statement
            .contains("No prior working activity recorded"));
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
            .with_task(
                Some("tsk-conf".to_string()),
                "Confirmed Milestone",
                1.0,
                InformationProvenance::Confirmed,
            )
            .add_workset_item(observed_item)
            .with_task_references(vec![confirmed_task])
            .with_activity_references(vec![inferred_activity])
            .with_provenance(InformationProvenance::Confirmed)
            .build();

        assert_eq!(
            snapshot.task_provenance,
            Some(InformationProvenance::Confirmed)
        );
        assert_eq!(snapshot.provenance, InformationProvenance::Confirmed);
        assert_eq!(
            snapshot.workset_item_references[0].provenance,
            InformationProvenance::Observed
        );
        assert_eq!(
            snapshot.recent_task_references[0].provenance,
            InformationProvenance::Confirmed
        );
        assert_eq!(
            snapshot.recent_activity_references[0].provenance,
            InformationProvenance::Inferred
        );
    }

    #[test]
    fn test_service_derive_from_database_clean_workspace() {
        let conn = setup_test_db();

        // 1. Create Workspace
        let ws = crate::db::operations::create_workspace(&conn, "Aurora Website").unwrap();

        // 2. Add high-relevance items
        let _item1 = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://figma.com/file/123/Aurora-Hero",
            Some("Aurora Hero Figma Spec"),
            r#"{"app_name": "Figma", "domain": "figma.com", "user_confirmed": true}"#,
        )
        .unwrap();

        let _item2 = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "export const HeroAnimation = () => { return <motion.div ... />; };",
            Some("HeroAnimation.tsx"),
            r#"{"app_name": "VS Code"}"#,
        )
        .unwrap();

        // 3. Add task candidate
        let mut task = TaskCandidate::new(Some(ws.id.clone()), "Hero Animation", 0.90);
        task.status = TaskCandidateStatus::Active;
        let _ = crate::db::operations::create_task_candidate(&conn, &task).unwrap();

        // 4. Derive snapshot via ContextSnapshotService
        let service = ContextSnapshotService::new();
        let snapshot = service.derive_for_context(&conn, &ws.id).unwrap();

        assert_eq!(snapshot.context_id, ws.id);
        assert_eq!(snapshot.context_name, "Aurora Website");
        assert!(snapshot.has_task());
        assert_eq!(snapshot.task_title.as_deref(), Some("Hero Animation"));
        assert!(snapshot.workset_item_references.len() >= 2);
        assert_eq!(snapshot.resume_readiness.readiness, ResumeReadiness::Ready);
        assert!(
            snapshot.snapshot_statement.contains("Hero Animation")
                || snapshot.snapshot_statement.contains("Aurora")
        );
        assert_eq!(snapshot.epistemic_disclaimer, CONTEXTUAL_SNAPSHOT_DISCLAIMER);
    }

    #[test]
    fn test_service_determinism_multiple_calls() {
        let conn = setup_test_db();
        let ws = crate::db::operations::create_workspace(&conn, "Determinism Test").unwrap();

        let _ = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://docs.rs/tokio",
            Some("Tokio Docs"),
            r#"{"app_name": "Chrome", "domain": "docs.rs"}"#,
        )
        .unwrap();

        let service = ContextSnapshotService::new();
        let snap1 = service.derive_for_context(&conn, &ws.id).unwrap();
        let snap2 = service.derive_for_context(&conn, &ws.id).unwrap();

        // Check essential deterministic properties
        assert_eq!(snap1.context_id, snap2.context_id);
        assert_eq!(snap1.context_name, snap2.context_name);
        assert_eq!(snap1.task_title, snap2.task_title);
        assert_eq!(snap1.state, snap2.state);
        assert_eq!(snap1.state_freshness, snap2.state_freshness);
        assert_eq!(
            snap1.workset_item_references.len(),
            snap2.workset_item_references.len()
        );
        assert_eq!(snap1.snapshot_statement, snap2.snapshot_statement);
    }

    #[test]
    fn test_service_empty_context() {
        let conn = setup_test_db();
        let ws = crate::db::operations::create_workspace(&conn, "Empty WS").unwrap();

        let service = ContextSnapshotService::new();
        let snapshot = service.derive_for_context(&conn, &ws.id).unwrap();

        assert_eq!(snapshot.context_id, ws.id);
        assert_eq!(snapshot.state, ContextStateValue::InsufficientEvidence);
        assert_eq!(
            snapshot.resume_readiness.readiness,
            ResumeReadiness::Insufficient
        );
        assert!(!snapshot.has_task());
        assert!(snapshot.workset_item_references.is_empty());
        assert!(snapshot
            .snapshot_statement
            .contains("No prior working activity recorded"));
        assert_eq!(
            snapshot.quality.rating,
            SnapshotQualityRating::Insufficient
        );
        assert_eq!(snapshot.can_confidently_resume(), false);
    }

    #[test]
    fn test_quality_complete_aurora_website() {
        // Example from prompt:
        // Aurora Website
        // Task identified
        // Workset contains 5 strong items
        // Recent activity exists
        // → complete/useful
        let items = vec![
            create_dummy_workset_item("i1", "Hero Component", WorksetItemRelevance::Primary, 0.95, Some("VS Code")),
            create_dummy_workset_item("i2", "Framer Motion API", WorksetItemRelevance::Supporting, 0.88, Some("Chrome")),
            create_dummy_workset_item("i3", "Tailwind Theme", WorksetItemRelevance::Supporting, 0.82, Some("VS Code")),
            create_dummy_workset_item("i4", "Design Mockup", WorksetItemRelevance::Supporting, 0.85, Some("Figma")),
            create_dummy_workset_item("i5", "Asset Export", WorksetItemRelevance::Supporting, 0.78, Some("Finder")),
        ];

        let activity = vec![
            SnapshotActivityReference {
                id: Some("act1".to_string()),
                entry_type: "item_captured".to_string(),
                description: "Edited Hero Component in VS Code".to_string(),
                timestamp: "2026-09-17T00:40:00Z".to_string(),
                provenance: InformationProvenance::Observed,
            },
            SnapshotActivityReference {
                id: Some("act2".to_string()),
                entry_type: "session_burst".to_string(),
                description: "Focused activity burst in Aurora Website".to_string(),
                timestamp: "2026-09-17T00:42:00Z".to_string(),
                provenance: InformationProvenance::Observed,
            },
        ];

        let snapshot = ContextSnapshotBuilder::new("ws-aurora", "Aurora Website")
            .with_state(ContextStateValue::Active)
            .with_freshness(StateFreshness::Fresh, 180)
            .with_task(
                Some("t-aurora".to_string()),
                "Hero Animation",
                0.92,
                InformationProvenance::Inferred,
            )
            .with_workset_items(items)
            .with_activity_references(activity)
            .with_confidence(0.92)
            .build();

        assert_eq!(snapshot.quality.rating, SnapshotQualityRating::Complete);
        assert!(snapshot.is_complete());
        assert!(snapshot.quality.rating.is_usable());
        assert!(!snapshot.is_insufficient());
        assert_eq!(snapshot.can_confidently_resume(), true);
        assert_eq!(snapshot.quality.strong_workset_count, 5);
        assert!(snapshot.quality.evidence_diversity_score >= 0.80);
        assert_eq!(snapshot.quality.has_contradictions, false);
        assert!(snapshot.quality.score >= 0.80);
    }

    #[test]
    fn test_quality_insufficient_random_context() {
        // Example from prompt:
        // Random context
        // One weak item
        // No meaningful task
        // Old activity
        // → insufficient
        // An insufficient snapshot must NOT be presented as a confident resume state.
        let items = vec![
            create_dummy_workset_item(
                "w1",
                "Random Search",
                WorksetItemRelevance::Related,
                0.30,
                Some("Chrome"),
            ),
        ];

        let snapshot = ContextSnapshotBuilder::new("ws-random", "Random Context")
            .with_state(ContextStateValue::Dormant)
            .with_freshness(StateFreshness::VeryStale, 86400 * 14) // 14 days old
            .without_task()
            .with_workset_items(items)
            .with_confidence(0.35)
            .build();

        assert_eq!(snapshot.quality.rating, SnapshotQualityRating::Insufficient);
        assert!(snapshot.is_insufficient());
        assert!(!snapshot.is_complete());
        assert!(!snapshot.is_useful());

        // CRITICAL INVARIANT TEST:
        assert_eq!(
            snapshot.can_confidently_resume(),
            false,
            "An insufficient snapshot must NOT be presented as a confident resume state."
        );
        assert!(snapshot.quality.score <= 0.35);
        assert!(snapshot.quality.reasons.iter().any(|r| r.contains("peripheral") || r.contains("No specific active task")));
    }

    #[test]
    fn test_quality_useful_rich_workset_implied_task() {
        let items = vec![
            create_dummy_workset_item("i1", "DB Schema", WorksetItemRelevance::Primary, 0.90, Some("VS Code")),
            create_dummy_workset_item("i2", "Migration Guide", WorksetItemRelevance::Supporting, 0.80, Some("Chrome")),
            create_dummy_workset_item("i3", "SQL Queries", WorksetItemRelevance::Supporting, 0.75, Some("DataGrip")),
        ];

        let snapshot = ContextSnapshotBuilder::new("ws-backend", "Backend Architecture")
            .with_state(ContextStateValue::RecentlyActive)
            .with_freshness(StateFreshness::Recent, 3600 * 3) // 3 hours ago
            .without_task()
            .with_workset_items(items)
            .with_confidence(0.80)
            .build();

        assert_eq!(snapshot.quality.rating, SnapshotQualityRating::Useful);
        assert!(snapshot.is_useful());
        assert!(!snapshot.is_insufficient());
        assert_eq!(snapshot.can_confidently_resume(), true);
    }

    #[test]
    fn test_quality_partial_sparse_context() {
        let items = vec![
            create_dummy_workset_item("i1", "Draft Note", WorksetItemRelevance::Supporting, 0.60, Some("Notes")),
        ];

        let snapshot = ContextSnapshotBuilder::new("ws-notes", "Quick Notes")
            .with_state(ContextStateValue::Paused)
            .with_freshness(StateFreshness::Stale, 86400 * 3) // 3 days ago
            .without_task()
            .with_workset_items(items)
            .with_confidence(0.55)
            .build();

        assert_eq!(snapshot.quality.rating, SnapshotQualityRating::Partial);
        assert!(snapshot.is_partial());
        assert!(!snapshot.is_complete());
        assert_eq!(snapshot.can_confidently_resume(), false);
    }

    #[test]
    fn test_quality_contradictions_detection() {
        let items = vec![
            create_dummy_workset_item("i1", "Doc 1", WorksetItemRelevance::Supporting, 0.50, None),
        ];

        let snapshot = ContextSnapshotBuilder::new("ws-conflict", "Conflicted WS")
            .with_state(ContextStateValue::Uncertain)
            .with_freshness(StateFreshness::Fresh, 300)
            .with_task(
                Some("t-weak".to_string()),
                "Vague Task",
                0.30, // low confidence task
                InformationProvenance::Inferred,
            )
            .with_workset_items(items)
            .with_confidence(0.40)
            .build();

        assert!(snapshot.quality.has_contradictions);
        assert!(snapshot.quality.contradictions.len() >= 1);
        assert_eq!(snapshot.quality.rating, SnapshotQualityRating::Insufficient);
        assert_eq!(snapshot.can_confidently_resume(), false);
    }

    #[test]
    fn test_quality_evidence_diversity_score() {
        let multi_app_items = vec![
            create_dummy_workset_item("i1", "Code", WorksetItemRelevance::Primary, 0.90, Some("VS Code")),
            create_dummy_workset_item("i2", "Docs", WorksetItemRelevance::Supporting, 0.85, Some("Chrome")),
            create_dummy_workset_item("i3", "Design", WorksetItemRelevance::Supporting, 0.80, Some("Figma")),
        ];

        let single_app_items = vec![
            create_dummy_workset_item("i1", "Code 1", WorksetItemRelevance::Primary, 0.90, Some("VS Code")),
            create_dummy_workset_item("i2", "Code 2", WorksetItemRelevance::Supporting, 0.85, Some("VS Code")),
        ];

        let snap_multi = ContextSnapshotBuilder::new("ws-m", "Multi App WS")
            .with_workset_items(multi_app_items)
            .build();

        let snap_single = ContextSnapshotBuilder::new("ws-s", "Single App WS")
            .with_workset_items(single_app_items)
            .build();

        assert!(snap_multi.quality.evidence_diversity_score > snap_single.quality.evidence_diversity_score);
        assert_eq!(snap_multi.quality.distinct_apps_count, 3);
        assert_eq!(snap_single.quality.distinct_apps_count, 1);
    }

    #[test]
    fn test_quality_service_end_to_end_evaluation() {
        let conn = setup_test_db();
        let ws = crate::db::operations::create_workspace(&conn, "E2E Quality Test").unwrap();

        // 1. Insert multiple items
        for idx in 1..=4 {
            let _ = crate::db::operations::create_item(
                &conn,
                Some(&ws.id),
                None,
                "file",
                &format!("src/module_{}.rs", idx),
                Some(&format!("Module {}", idx)),
                r#"{"app_name": "VS Code"}"#,
            )
            .unwrap();
        }

        // 2. Insert active task
        let mut task = TaskCandidate::new(Some(ws.id.clone()), "Build Quality Engine", 0.90);
        task.status = TaskCandidateStatus::Active;
        let _ = crate::db::operations::create_task_candidate(&conn, &task).unwrap();

        let service = ContextSnapshotService::new();
        let snapshot = service.derive_for_context(&conn, &ws.id).unwrap();

        assert!(snapshot.quality.rating == SnapshotQualityRating::Complete || snapshot.quality.rating == SnapshotQualityRating::Useful);
        assert!(snapshot.can_confidently_resume());
        assert!(snapshot.quality.total_workset_count >= 2);
    }
}

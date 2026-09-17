use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

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
        let title = item
            .title
            .as_ref()
            .map(|t| crate::context_engine::reasoning::privacy::scrub_private_data(t));

        let mut snippet =
            crate::context_engine::reasoning::privacy::scrub_private_data(&item.content_snippet);
        if snippet.chars().count() > 250 {
            let truncated: String = snippet.chars().take(250).collect();
            snippet = format!("{}...", truncated);
        }

        let reasons = item
            .reasons
            .iter()
            .map(|r| crate::context_engine::reasoning::privacy::scrub_private_data(r))
            .collect();

        Self {
            id: item.id.clone(),
            title,
            item_type: item.item_type.clone(),
            relevance: item.relevance,
            relevance_score: item.relevance_score,
            provenance: item.provenance,
            app_name: item.app_name.clone(),
            domain: item.domain.clone(),
            content_snippet: snippet,
            reasons,
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
            title: crate::context_engine::reasoning::privacy::scrub_private_data(&task.title),
            action_verb: task
                .action_verb
                .as_ref()
                .map(|v| crate::context_engine::reasoning::privacy::scrub_private_data(v)),
            target_subject: task
                .target_subject
                .as_ref()
                .map(|s| crate::context_engine::reasoning::privacy::scrub_private_data(s)),
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
            title: crate::context_engine::reasoning::privacy::scrub_private_data(&task.title),
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
            description: crate::context_engine::reasoning::privacy::scrub_private_data(
                &entry.description,
            ),
            timestamp: entry.timestamp.clone(),
            provenance: entry.provenance,
        }
    }
}

/// Lightweight reference to an important contextual memory in a snapshot.
///
/// Invariants:
/// - Does NOT copy full heavy memory payloads; stores lightweight reference metadata.
/// - Distinguishes durable historical memory from current dynamic working state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotImportantMemoryReference {
    /// Unique memory identifier (e.g. "mem_item_123", "mem_ref_456")
    pub id: String,
    /// Source reference identifier (item_id, task_id, workspace_id)
    pub source_id: String,
    /// Semantic category of the memory
    pub memory_type: crate::context_engine::important_memory::ImportantMemoryType,
    /// Human-friendly title or label
    pub title: String,
    /// Optional concise detail or snippet
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Calibrated importance score [0.0, 1.0]
    pub importance: f64,
    /// Qualitative importance band
    pub importance_band: crate::context_engine::important_memory::MemoryImportanceBand,
    /// Confidence score [0.0, 1.0]
    pub confidence: f64,
    /// Epistemic source
    pub provenance: InformationProvenance,
    /// First seen ISO-8601 timestamp
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_seen_at: Option<String>,
    /// Last seen ISO-8601 timestamp
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen_at: Option<String>,
    /// Associated application name if known
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    /// Associated web domain if known
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
}

impl From<&crate::context_engine::important_memory::ContextImportantMemory>
    for SnapshotImportantMemoryReference
{
    fn from(mem: &crate::context_engine::important_memory::ContextImportantMemory) -> Self {
        Self {
            id: mem.id.clone(),
            source_id: mem.source_id.clone(),
            memory_type: mem.memory_type,
            title: crate::context_engine::reasoning::privacy::scrub_private_data(&mem.title),
            detail: mem
                .detail
                .as_ref()
                .map(|d| crate::context_engine::reasoning::privacy::scrub_private_data(d)),
            importance: mem.importance,
            importance_band: mem.importance_band,
            confidence: mem.confidence,
            provenance: mem.provenance,
            first_seen_at: mem.first_seen_at.clone(),
            last_seen_at: mem.last_seen_at.clone(),
            app_name: mem.metadata.app_name.clone(),
            domain: mem.metadata.domain.clone(),
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
    /// Lightweight references to important contextual memories (e.g. project repositories, Figma projects, design specs)
    #[serde(default)]
    pub important_memory_references: Vec<SnapshotImportantMemoryReference>,
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
        important_memory_references: Vec<SnapshotImportantMemoryReference>,
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
            important_memory_references,
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
            important_memory_references: Vec::new(),
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
            important_memory_references: Vec::new(),
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
    pub important_memory_references: Vec<SnapshotImportantMemoryReference>,
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
            important_memory_references: Vec::new(),
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

        // 7. Important memories (references only - historical contextual anchors)
        let important_memories: Vec<SnapshotImportantMemoryReference> =
            crate::context_engine::important_memory::ContextImportantMemoryBuilder::build(
                conn,
                context_id,
                None,
            )
            .map(|set| set.memories)
            .unwrap_or_default()
            .into_iter()
            .filter(|m| {
                m.memory_type
                    != crate::context_engine::important_memory::ImportantMemoryType::ConfirmedContextFact
                    || !m.title.starts_with("Workspace:")
            })
            .take(8)
            .map(|m| SnapshotImportantMemoryReference::from(&m))
            .collect();

        // 8. Last meaningful activity
        let meaningful_activity = state.as_ref().and_then(|s| s.meaningful_activity.clone());

        // 9. Determine active task
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

        // 10. State & Freshness
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
            important_memory_references: important_memories,
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

    pub fn with_important_memory_references(
        mut self,
        memories: Vec<SnapshotImportantMemoryReference>,
    ) -> Self {
        self.important_memory_references = memories;
        self
    }

    pub fn add_important_memory_reference(
        mut self,
        memory: SnapshotImportantMemoryReference,
    ) -> Self {
        self.important_memory_references.push(memory);
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
            self.important_memory_references,
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
// SnapshotTrigger & Classification (Day 20 Step 5)
// =========================================================================

/// Explicit categories of meaningful events that trigger context snapshot generation.
///
/// Invariants:
/// - Snapshots are refreshed AFTER meaningful events (task transition, context transition,
///   context return, meaningful segment completion, significant workset change, user confirmation/correction, explicit request).
/// - NEVER create snapshots for: every clipboard capture, every app switch, every render,
///   every mouse movement, or trivial duplicate activity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum SnapshotTrigger {
    /// Task transition detected or confirmed within the context
    TaskTransition {
        task_id: Option<String>,
        task_title: String,
        transition_type: String,
    },
    /// Context transition away from or into this context
    ContextTransition {
        from_context_id: Option<String>,
        to_context_id: Option<String>,
        transition_type: String,
    },
    /// Return to a previously active context after an absence
    ContextReturn {
        context_id: String,
        gap_seconds: u64,
    },
    /// Completion of a verified meaningful activity burst / segment
    MeaningfulSegmentCompletion {
        segment_id: Option<String>,
        item_count: usize,
    },
    /// Significant evolution in the active workset (e.g., primary item change, >= 2 core additions)
    SignificantWorksetChange {
        added_count: usize,
        removed_count: usize,
        primary_item_changed: bool,
    },
    /// User confirmed context membership, task candidate, or state
    UserConfirmation {
        entity_type: String,
        entity_id: String,
    },
    /// User corrected or moved items/tasks between contexts
    UserCorrection {
        previous_context_id: Option<String>,
        corrected_context_id: String,
    },
    /// Explicit user or system request to reconstruct state
    ExplicitRequest {
        reason: String,
    },
    /// Trivial / noisy activity that must NOT create snapshots
    SuppressedNoise {
        event_type: String,
        reason: String,
    },
}

impl SnapshotTrigger {
    /// Returns `true` if this trigger represents a substantive event warranting snapshot creation.
    pub fn is_meaningful(&self) -> bool {
        !matches!(self, Self::SuppressedNoise { .. })
    }

    /// Returns `true` if this trigger requires immediate execution without debouncing delay.
    pub fn is_high_priority(&self) -> bool {
        matches!(
            self,
            Self::UserConfirmation { .. }
                | Self::UserCorrection { .. }
                | Self::ExplicitRequest { .. }
                | Self::ContextTransition { .. }
        )
    }

    /// Returns `true` if this trigger is suppressed noise.
    pub fn is_suppressed(&self) -> bool {
        matches!(self, Self::SuppressedNoise { .. })
    }

    pub fn trigger_name(&self) -> &'static str {
        match self {
            Self::TaskTransition { .. } => "task_transition",
            Self::ContextTransition { .. } => "context_transition",
            Self::ContextReturn { .. } => "context_return",
            Self::MeaningfulSegmentCompletion { .. } => "meaningful_segment_completion",
            Self::SignificantWorksetChange { .. } => "significant_workset_change",
            Self::UserConfirmation { .. } => "user_confirmation",
            Self::UserCorrection { .. } => "user_correction",
            Self::ExplicitRequest { .. } => "explicit_request",
            Self::SuppressedNoise { .. } => "suppressed_noise",
        }
    }
}

/// Classifier for filtering incoming runtime events into meaningful snapshot triggers vs suppressed noise.
pub struct SnapshotTriggerClassifier;

impl SnapshotTriggerClassifier {
    /// Classifies an event by name and optional flags.
    pub fn classify_event(event_name: &str, is_duplicate: bool) -> SnapshotTrigger {
        if is_duplicate {
            return SnapshotTrigger::SuppressedNoise {
                event_type: event_name.to_string(),
                reason: "Trivial duplicate capture suppressed".to_string(),
            };
        }

        match event_name {
            "clipboard_capture" | "clipboard_changed" | "clipboard_item" => {
                SnapshotTrigger::SuppressedNoise {
                    event_type: event_name.to_string(),
                    reason: "Raw clipboard capture does not warrant immediate snapshot".to_string(),
                }
            }
            "app_switch" | "window_focus" | "window_switched" => {
                SnapshotTrigger::SuppressedNoise {
                    event_type: event_name.to_string(),
                    reason: "App switch alone does not represent context completion".to_string(),
                }
            }
            "render" | "render_frame" | "draw" => {
                SnapshotTrigger::SuppressedNoise {
                    event_type: event_name.to_string(),
                    reason: "UI rendering event suppressed".to_string(),
                }
            }
            "mouse_move" | "cursor_movement" | "scroll" => {
                SnapshotTrigger::SuppressedNoise {
                    event_type: event_name.to_string(),
                    reason: "Input gesture noise suppressed".to_string(),
                }
            }
            "user_confirm_context" | "user_confirmed_task" | "user_confirmation" => {
                SnapshotTrigger::UserConfirmation {
                    entity_type: "context_item".to_string(),
                    entity_id: String::new(),
                }
            }
            "user_correct_context" | "user_moved_item" | "user_correction" => {
                SnapshotTrigger::UserCorrection {
                    previous_context_id: None,
                    corrected_context_id: String::new(),
                }
            }
            "task_transition" | "task_started" | "task_switched" => {
                SnapshotTrigger::TaskTransition {
                    task_id: None,
                    task_title: "Active Task".to_string(),
                    transition_type: "inferred".to_string(),
                }
            }
            "context_transition" | "workspace_switched" => {
                SnapshotTrigger::ContextTransition {
                    from_context_id: None,
                    to_context_id: None,
                    transition_type: "switch".to_string(),
                }
            }
            "context_return" | "returned_to_context" => {
                SnapshotTrigger::ContextReturn {
                    context_id: String::new(),
                    gap_seconds: 300,
                }
            }
            "segment_completed" | "meaningful_segment" => {
                SnapshotTrigger::MeaningfulSegmentCompletion {
                    segment_id: None,
                    item_count: 1,
                }
            }
            "workset_evolved" | "workset_changed" => {
                SnapshotTrigger::SignificantWorksetChange {
                    added_count: 1,
                    removed_count: 0,
                    primary_item_changed: true,
                }
            }
            "reconstruct_snapshot" | "refresh_snapshot" => {
                SnapshotTrigger::ExplicitRequest {
                    reason: "Explicit client reconstruction request".to_string(),
                }
            }
            _ => SnapshotTrigger::SuppressedNoise {
                event_type: event_name.to_string(),
                reason: "Unrecognized raw signal suppressed".to_string(),
            },
        }
    }
}

/// Internal pending trigger entry for a context.
#[derive(Debug, Clone)]
struct PendingTriggerEntry {
    trigger: SnapshotTrigger,
    #[allow(dead_code)]
    first_scheduled_at: Instant,
    last_updated_at: Instant,
    coalesce_count: usize,
    is_high_priority: bool,
}

/// Asynchronous trigger coalescer and debouncer for snapshot generation.
///
/// Invariants:
/// 1. Capture pipeline NEVER waits for snapshot generation (synchronously schedules in memory).
/// 2. Coalesces rapid successive triggers within the debounce window into a single snapshot generation.
/// 3. Drops suppressed noise events immediately without locking or queue bloat.
#[derive(Debug)]
pub struct SnapshotTriggerCoalescer {
    pending: Arc<Mutex<HashMap<String, PendingTriggerEntry>>>,
    total_received: Arc<Mutex<usize>>,
    total_scheduled: Arc<Mutex<usize>>,
    total_suppressed: Arc<Mutex<usize>>,
    total_coalesced: Arc<Mutex<usize>>,
    default_debounce: Duration,
}

impl Default for SnapshotTriggerCoalescer {
    fn default() -> Self {
        Self::new()
    }
}

impl SnapshotTriggerCoalescer {
    pub fn new() -> Self {
        Self {
            pending: Arc::new(Mutex::new(HashMap::new())),
            total_received: Arc::new(Mutex::new(0)),
            total_scheduled: Arc::new(Mutex::new(0)),
            total_suppressed: Arc::new(Mutex::new(0)),
            total_coalesced: Arc::new(Mutex::new(0)),
            default_debounce: Duration::from_millis(1500),
        }
    }

    pub fn with_debounce(mut self, duration: Duration) -> Self {
        self.default_debounce = duration;
        self
    }

    /// Schedules an incoming trigger for a context.
    /// Returns `true` if scheduled/coalesced, `false` if dropped as noise.
    pub fn schedule(&self, context_id: &str, trigger: SnapshotTrigger) -> bool {
        if let Ok(mut count) = self.total_received.lock() {
            *count += 1;
        }

        if trigger.is_suppressed() {
            if let Ok(mut count) = self.total_suppressed.lock() {
                *count += 1;
            }
            return false;
        }

        if let Ok(mut count) = self.total_scheduled.lock() {
            *count += 1;
        }

        let is_high = trigger.is_high_priority();
        let now = Instant::now();

        if let Ok(mut guard) = self.pending.lock() {
            if let Some(entry) = guard.get_mut(context_id) {
                entry.coalesce_count += 1;
                entry.last_updated_at = now;
                if is_high {
                    entry.is_high_priority = true;
                }
                entry.trigger = trigger;
                if let Ok(mut coalesced) = self.total_coalesced.lock() {
                    *coalesced += 1;
                }
            } else {
                guard.insert(
                    context_id.to_string(),
                    PendingTriggerEntry {
                        trigger,
                        first_scheduled_at: now,
                        last_updated_at: now,
                        coalesce_count: 1,
                        is_high_priority: is_high,
                    },
                );
            }
        }

        true
    }

    /// Drains and returns all context triggers that have satisfied their debounce window
    /// or were flagged high priority.
    pub fn pop_ready(&self) -> Vec<(String, SnapshotTrigger)> {
        self.pop_ready_with_debounce(self.default_debounce)
    }

    /// Drains and returns ready triggers using an explicit debounce window.
    pub fn pop_ready_with_debounce(&self, debounce: Duration) -> Vec<(String, SnapshotTrigger)> {
        let mut ready = Vec::new();
        let now = Instant::now();

        if let Ok(mut guard) = self.pending.lock() {
            let mut keys_to_remove = Vec::new();
            for (ctx_id, entry) in guard.iter() {
                if entry.is_high_priority || now.duration_since(entry.last_updated_at) >= debounce {
                    ready.push((ctx_id.clone(), entry.trigger.clone()));
                    keys_to_remove.push(ctx_id.clone());
                }
            }
            for k in keys_to_remove {
                guard.remove(&k);
            }
        }

        ready
    }

    /// Checks if a context has a pending snapshot trigger.
    pub fn is_pending(&self, context_id: &str) -> bool {
        self.pending
            .lock()
            .map(|g| g.contains_key(context_id))
            .unwrap_or(false)
    }

    /// Returns the number of times triggers have been coalesced for a pending context.
    pub fn coalesce_count(&self, context_id: &str) -> usize {
        self.pending
            .lock()
            .ok()
            .and_then(|g| g.get(context_id).map(|e| e.coalesce_count))
            .unwrap_or(0)
    }

    pub fn total_received_count(&self) -> usize {
        self.total_received.lock().map(|c| *c).unwrap_or(0)
    }

    pub fn total_scheduled_count(&self) -> usize {
        self.total_scheduled.lock().map(|c| *c).unwrap_or(0)
    }

    pub fn total_suppressed_count(&self) -> usize {
        self.total_suppressed.lock().map(|c| *c).unwrap_or(0)
    }

    pub fn total_coalesced_count(&self) -> usize {
        self.total_coalesced.lock().map(|c| *c).unwrap_or(0)
    }

    pub fn pending_contexts_count(&self) -> usize {
        self.pending.lock().map(|g| g.len()).unwrap_or(0)
    }

    pub fn clear(&self) {
        if let Ok(mut guard) = self.pending.lock() {
            guard.clear();
        }
    }
}

// =========================================================================
// ContextSnapshotCache: In-Memory High Performance Cache
// =========================================================================

#[derive(Debug, Clone)]
struct CachedSnapshotEntry {
    snapshot: ContextSnapshot,
    cached_at: Instant,
}

/// High-performance thread-safe cache for derived `ContextSnapshot` instances.
pub struct ContextSnapshotCache {
    entries: RwLock<HashMap<String, CachedSnapshotEntry>>,
    ttl: Duration,
}

impl Default for ContextSnapshotCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextSnapshotCache {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            ttl: Duration::from_secs(30),
        }
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    pub fn get(&self, context_id: &str) -> Option<ContextSnapshot> {
        if let Ok(guard) = self.entries.read() {
            if let Some(entry) = guard.get(context_id) {
                if entry.cached_at.elapsed() < self.ttl {
                    return Some(entry.snapshot.clone());
                }
            }
        }
        None
    }

    pub fn insert(&self, context_id: &str, snapshot: ContextSnapshot) {
        if let Ok(mut guard) = self.entries.write() {
            if guard.len() >= 100 {
                guard.clear();
            }
            guard.insert(
                context_id.to_string(),
                CachedSnapshotEntry {
                    snapshot,
                    cached_at: Instant::now(),
                },
            );
        }
    }

    pub fn invalidate(&self, context_id: &str) {
        if let Ok(mut guard) = self.entries.write() {
            guard.remove(context_id);
        }
    }

    pub fn invalidate_all(&self) {
        if let Ok(mut guard) = self.entries.write() {
            guard.clear();
        }
    }

    pub fn len(&self) -> usize {
        self.entries.read().map(|g| g.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

static GLOBAL_SNAPSHOT_CACHE: OnceLock<ContextSnapshotCache> = OnceLock::new();

pub fn get_global_snapshot_cache() -> &'static ContextSnapshotCache {
    GLOBAL_SNAPSHOT_CACHE.get_or_init(ContextSnapshotCache::new)
}

pub fn invalidate_snapshot_cache(context_id: &str) {
    get_global_snapshot_cache().invalidate(context_id);
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

    /// Checks whether contextual snapshots are enabled in the user's PrivacySettings.
    pub fn is_snapshots_enabled(conn: &Connection) -> bool {
        crate::db::settings::get_settings(conn)
            .map(|s| s.enable_context_snapshots)
            .unwrap_or(true)
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

    /// Derives the snapshot, utilizing the global cache if valid.
    pub fn derive_for_context_cached(
        &self,
        conn: &Connection,
        context_id: &str,
    ) -> Result<ContextSnapshot, String> {
        let cache = get_global_snapshot_cache();
        if let Some(cached) = cache.get(context_id) {
            return Ok(cached);
        }
        let snapshot = self.derive_for_context(conn, context_id)?;
        cache.insert(context_id, snapshot.clone());
        Ok(snapshot)
    }

    /// Handles a snapshot trigger for a context: invalidates cache and triggers update if enabled.
    pub fn handle_trigger(
        &self,
        conn: &Connection,
        context_id: &str,
        trigger: SnapshotTrigger,
    ) -> Result<Option<ContextSnapshot>, String> {
        if trigger.is_suppressed() || !Self::is_snapshots_enabled(conn) {
            return Ok(None);
        }
        invalidate_snapshot_cache(context_id);
        let snapshot = self.derive_for_context(conn, context_id)?;
        get_global_snapshot_cache().insert(context_id, snapshot.clone());
        Ok(Some(snapshot))
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

    /// Records a derived snapshot into historical tracking for the context (in-memory buffer).
    pub fn record_snapshot_history(&self, snapshot: ContextSnapshot) -> bool {
        get_global_snapshot_history().record_snapshot(snapshot)
    }

    /// Records a derived snapshot into both in-memory history and SQLite database with bounded retention.
    pub fn record_snapshot_history_with_db(
        &self,
        conn: &Connection,
        snapshot: ContextSnapshot,
    ) -> Result<bool, String> {
        let recorded = self.record_snapshot_history(snapshot.clone());
        let _ = crate::db::operations::save_context_snapshot(
            conn,
            &snapshot,
            DEFAULT_MAX_SNAPSHOT_RETENTION,
        );
        Ok(recorded)
    }

    /// Retrieves the chronological snapshot history for a context from memory.
    pub fn get_snapshot_history(&self, context_id: &str) -> Option<ContextSnapshotHistory> {
        get_global_snapshot_history().get_history(context_id)
    }

    /// Retrieves the chronological snapshot history for a context, falling back to bounded database query.
    pub fn get_snapshot_history_from_db(
        &self,
        conn: &Connection,
        context_id: &str,
    ) -> Result<Option<ContextSnapshotHistory>, String> {
        if let Some(hist) = self.get_snapshot_history(context_id) {
            if !hist.is_empty() {
                return Ok(Some(hist));
            }
        }
        let db_snaps = crate::db::operations::list_context_snapshots_bounded(
            conn,
            context_id,
            DEFAULT_MAX_SNAPSHOT_RETENTION,
        )
        .map_err(|e| e.to_string())?;

        if db_snaps.is_empty() {
            return Ok(None);
        }

        let ctx_name = db_snaps
            .first()
            .map(|s| s.context_name.clone())
            .unwrap_or_else(|| "Workspace".to_string());
        let mut history = ContextSnapshotHistory::new(context_id, ctx_name)
            .with_max_capacity(DEFAULT_MAX_SNAPSHOT_RETENTION);
        for s in db_snaps {
            history.add_snapshot(s);
        }
        Ok(Some(history))
    }

    /// Retrieves the latest snapshot recorded for a context.
    pub fn get_latest_snapshot(&self, context_id: &str) -> Option<ContextSnapshot> {
        get_global_snapshot_history().get_latest(context_id)
    }

    /// Retrieves the previous snapshot before the latest recorded for a context.
    pub fn get_previous_snapshot(&self, context_id: &str) -> Option<ContextSnapshot> {
        get_global_snapshot_history().get_previous(context_id)
    }

    /// Compares the latest snapshot with the previous snapshot for a context.
    pub fn compare_latest_with_previous(&self, context_id: &str) -> Option<ContextSnapshotDiff> {
        let history = self.get_snapshot_history(context_id)?;
        history.compare_latest_with_previous()
    }

    /// Compares two snapshots directly.
    pub fn compare_snapshots(
        &self,
        previous: Option<&ContextSnapshot>,
        current: &ContextSnapshot,
    ) -> ContextSnapshotDiff {
        ContextSnapshotComparator::compare(previous, current)
    }
}

// =========================================================================
// ContextSnapshotHistory: Bounded Lightweight Snapshot Timeline (Day 20 Step 6)
// =========================================================================

/// Default maximum number of retained snapshots per context to prevent unbounded memory growth.
pub const DEFAULT_MAX_SNAPSHOT_RETENTION: usize = 20;

/// Bounded chronological history of lightweight snapshots for a specific context.
///
/// Invariants:
/// 1. Represents meaningful working states over time (e.g. Hero design -> Hero animation -> Responsive hero implementation).
/// 2. Bounded retention ensures history does not grow indefinitely.
/// 3. Does NOT delete underlying Items or raw activity telemetry.
/// 4. Snapshot history is derived context history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextSnapshotHistory {
    /// Context / Workspace identifier
    pub context_id: String,
    /// Context display name
    pub context_name: String,
    /// Chronological list of retained snapshots (oldest to newest)
    pub snapshots: Vec<ContextSnapshot>,
    /// Maximum capacity of snapshots retained for this context
    pub max_capacity: usize,
}

impl ContextSnapshotHistory {
    /// Creates a new empty `ContextSnapshotHistory`.
    pub fn new(context_id: impl Into<String>, context_name: impl Into<String>) -> Self {
        Self {
            context_id: context_id.into(),
            context_name: context_name.into(),
            snapshots: Vec::new(),
            max_capacity: DEFAULT_MAX_SNAPSHOT_RETENTION,
        }
    }

    /// Customizes the retention capacity.
    pub fn with_max_capacity(mut self, capacity: usize) -> Self {
        self.max_capacity = capacity.max(1);
        self.enforce_retention();
        self
    }

    /// Adds a new snapshot to history if it represents a meaningful state change.
    /// Returns `true` if added, `false` if skipped as redundant/duplicate.
    pub fn add_snapshot(&mut self, snapshot: ContextSnapshot) -> bool {
        if snapshot.context_id != self.context_id {
            return false;
        }

        // Anti-duplication: Check if state, task, and core workset are identical to latest snapshot
        if let Some(latest) = self.snapshots.last() {
            let same_task = latest.task_title == snapshot.task_title;
            let same_state = latest.state == snapshot.state;
            let same_core_items = latest.core_items().len() == snapshot.core_items().len()
                && latest
                    .core_items()
                    .iter()
                    .all(|i| snapshot.core_items().iter().any(|si| si.id == i.id));

            if same_task && same_state && same_core_items {
                return false;
            }
        }

        self.snapshots.push(snapshot);
        self.enforce_retention();
        true
    }

    /// Enforces bounded retention by dropping oldest snapshots when capacity is exceeded.
    /// Note: Does NOT delete underlying Items or activities from database.
    fn enforce_retention(&mut self) {
        if self.snapshots.len() > self.max_capacity {
            let excess = self.snapshots.len() - self.max_capacity;
            self.snapshots.drain(0..excess);
        }
    }

    /// Returns a reference to the latest / most recent snapshot.
    pub fn latest(&self) -> Option<&ContextSnapshot> {
        self.snapshots.last()
    }

    /// Returns a reference to the previous snapshot before the latest.
    pub fn previous(&self) -> Option<&ContextSnapshot> {
        if self.snapshots.len() >= 2 {
            self.snapshots.get(self.snapshots.len() - 2)
        } else {
            None
        }
    }

    /// Returns the snapshot at `n` steps prior to the latest (0 = latest, 1 = previous, 2 = 2nd previous...).
    pub fn nth_previous(&self, n: usize) -> Option<&ContextSnapshot> {
        if self.snapshots.is_empty() || n >= self.snapshots.len() {
            None
        } else {
            let idx = self.snapshots.len() - 1 - n;
            self.snapshots.get(idx)
        }
    }

    /// Returns all snapshots in chronological order.
    pub fn all(&self) -> &[ContextSnapshot] {
        &self.snapshots
    }

    /// Finds snapshots associated with a specific task title substring.
    pub fn find_by_task(&self, task_query: &str) -> Vec<&ContextSnapshot> {
        let q = task_query.to_lowercase();
        self.snapshots
            .iter()
            .filter(|s| {
                s.task_title
                    .as_ref()
                    .map(|t| t.to_lowercase().contains(&q))
                    .unwrap_or(false)
            })
            .collect()
    }

    /// Finds snapshot by ID.
    pub fn find_by_id(&self, snapshot_id: &str) -> Option<&ContextSnapshot> {
        self.snapshots.iter().find(|s| s.id == snapshot_id)
    }

    /// Number of retained snapshots.
    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    pub fn clear(&mut self) {
        self.snapshots.clear();
    }

    /// Compares the latest snapshot with the previous snapshot in history.
    pub fn compare_latest_with_previous(&self) -> Option<ContextSnapshotDiff> {
        let latest = self.latest()?;
        let previous = self.previous();
        Some(ContextSnapshotComparator::compare(previous, latest))
    }

    /// Compares two specific snapshots in history by their IDs.
    pub fn compare_snapshots(&self, from_id: &str, to_id: &str) -> Option<ContextSnapshotDiff> {
        let from = self.find_by_id(from_id);
        let to = self.find_by_id(to_id)?;
        Some(ContextSnapshotComparator::compare(from, to))
    }
}

/// In-memory thread-safe repository for contextual snapshot histories across all workspaces.
pub struct ContextSnapshotHistoryRepository {
    histories: RwLock<HashMap<String, ContextSnapshotHistory>>,
    default_capacity: usize,
}

impl Default for ContextSnapshotHistoryRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextSnapshotHistoryRepository {
    pub fn new() -> Self {
        Self {
            histories: RwLock::new(HashMap::new()),
            default_capacity: DEFAULT_MAX_SNAPSHOT_RETENTION,
        }
    }

    pub fn with_default_capacity(mut self, capacity: usize) -> Self {
        self.default_capacity = capacity;
        self
    }

    /// Records a snapshot into the appropriate context's history buffer.
    pub fn record_snapshot(&self, snapshot: ContextSnapshot) -> bool {
        let ctx_id = snapshot.context_id.clone();
        let ctx_name = snapshot.context_name.clone();

        if let Ok(mut guard) = self.histories.write() {
            let history = guard.entry(ctx_id.clone()).or_insert_with(|| {
                ContextSnapshotHistory::new(ctx_id, ctx_name)
                    .with_max_capacity(self.default_capacity)
            });
            history.add_snapshot(snapshot)
        } else {
            false
        }
    }

    /// Retrieves the snapshot history for a context.
    pub fn get_history(&self, context_id: &str) -> Option<ContextSnapshotHistory> {
        self.histories
            .read()
            .ok()
            .and_then(|g| g.get(context_id).cloned())
    }

    /// Retrieves the latest snapshot for a context.
    pub fn get_latest(&self, context_id: &str) -> Option<ContextSnapshot> {
        self.histories
            .read()
            .ok()
            .and_then(|g| g.get(context_id).and_then(|h| h.latest().cloned()))
    }

    /// Retrieves the previous snapshot for a context.
    pub fn get_previous(&self, context_id: &str) -> Option<ContextSnapshot> {
        self.histories
            .read()
            .ok()
            .and_then(|g| g.get(context_id).and_then(|h| h.previous().cloned()))
    }

    /// Clears history for a specific context.
    pub fn clear_context(&self, context_id: &str) {
        if let Ok(mut guard) = self.histories.write() {
            guard.remove(context_id);
        }
    }

    /// Clears all snapshot histories.
    pub fn clear_all(&self) {
        if let Ok(mut guard) = self.histories.write() {
            guard.clear();
        }
    }
}

static GLOBAL_SNAPSHOT_HISTORY: OnceLock<ContextSnapshotHistoryRepository> = OnceLock::new();

pub fn get_global_snapshot_history() -> &'static ContextSnapshotHistoryRepository {
    GLOBAL_SNAPSHOT_HISTORY.get_or_init(ContextSnapshotHistoryRepository::new)
}

pub fn record_snapshot_history(snapshot: ContextSnapshot) -> bool {
    get_global_snapshot_history().record_snapshot(snapshot)
}

pub fn get_context_snapshot_history(context_id: &str) -> Option<ContextSnapshotHistory> {
    get_global_snapshot_history().get_history(context_id)
}

// =========================================================================
// ContextSnapshotDiff & ContextSnapshotComparator (Day 20 Step 7)
// =========================================================================

/// Structured factual difference between two snapshots of the same context.
///
/// Invariants:
/// 1. Purely factual comparison based directly on recorded snapshot evidence.
/// 2. Does NOT speculate or infer reasons why the user switched tasks.
/// 3. Does NOT describe user mental intent beyond tangible evidence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextSnapshotDiff {
    /// Context identifier
    pub context_id: String,
    /// Context display name
    pub context_name: String,
    /// ID of the earlier/baseline snapshot (None if comparing against an empty baseline)
    pub from_snapshot_id: Option<String>,
    /// ID of the current/target snapshot
    pub to_snapshot_id: String,
    /// Timestamp of baseline snapshot (None if baseline was empty)
    pub from_timestamp: Option<String>,
    /// Timestamp of current snapshot
    pub to_timestamp: String,
    /// Whether the inferred/confirmed active task changed
    pub task_changed: bool,
    /// Previous active task title (None if absent)
    pub previous_task: Option<String>,
    /// Current active task title (None if absent)
    pub current_task: Option<String>,
    /// Factual description of task change (e.g. "Hero design → Hero animation")
    pub task_change_summary: Option<String>,
    /// Whether the working context state changed
    pub state_changed: bool,
    /// Previous context state
    pub previous_state: Option<ContextStateValue>,
    /// Current context state
    pub current_state: ContextStateValue,
    /// Items newly added to the workset in the current snapshot
    pub added_items: Vec<SnapshotWorksetItemReference>,
    /// Items present in previous snapshot but no longer in the current workset
    pub removed_items: Vec<SnapshotWorksetItemReference>,
    /// Items retained across both snapshots
    pub retained_items: Vec<SnapshotWorksetItemReference>,
    /// Whether resume readiness changed
    pub readiness_changed: bool,
    /// Previous readiness
    pub previous_readiness: Option<ResumeReadiness>,
    /// Current readiness
    pub current_readiness: ResumeReadiness,
    /// Confidence score delta (current - previous)
    pub confidence_delta: f64,
    /// Deterministic, non-speculative factual summary answering:
    /// "What changed since I last worked on this?"
    pub formatted_diff_summary: String,
    /// True if there are any meaningful differences (task, state, added/removed items, readiness)
    pub has_meaningful_changes: bool,
}

impl ContextSnapshotDiff {
    /// Returns `true` if there are no meaningful changes between snapshots.
    pub fn is_empty(&self) -> bool {
        !self.has_meaningful_changes
    }

    /// Number of items added to the workset.
    pub fn added_count(&self) -> usize {
        self.added_items.len()
    }

    /// Number of items removed from the workset.
    pub fn removed_count(&self) -> usize {
        self.removed_items.len()
    }

    /// Number of items retained in the workset.
    pub fn retained_count(&self) -> usize {
        self.retained_items.len()
    }
}

/// Purely factual, non-speculative comparator for contextual snapshots.
///
/// Invariants:
/// 1. Determines what changed between two snapshots without guessing user intent.
/// 2. Does not infer why the user changed tasks.
/// 3. Does not describe user intent beyond the concrete evidence.
pub struct ContextSnapshotComparator;

impl ContextSnapshotComparator {
    /// Compares two snapshots of a context and produces a structured delta.
    pub fn compare(
        previous: Option<&ContextSnapshot>,
        current: &ContextSnapshot,
    ) -> ContextSnapshotDiff {
        let (from_id, from_ts, prev_task, prev_state, prev_readiness, prev_items, prev_conf) =
            if let Some(prev) = previous {
                (
                    Some(prev.id.clone()),
                    Some(prev.created_at.clone()),
                    prev.task_title.clone(),
                    Some(prev.state),
                    Some(prev.resume_readiness.readiness),
                    &prev.workset_item_references[..],
                    prev.snapshot_confidence,
                )
            } else {
                (None, None, None, None, None, &[][..], 0.0)
            };

        // 1. Task change detection
        let task_changed = prev_task != current.task_title;
        let task_change_summary = if task_changed {
            match (&prev_task, &current.task_title) {
                (Some(p), Some(c)) => Some(format!("{} → {}", p, c)),
                (None, Some(c)) => Some(format!("Started task: {}", c)),
                (Some(p), None) => Some(format!("Cleared task (was '{}')", p)),
                (None, None) => None,
            }
        } else {
            None
        };

        // 2. State change detection
        let state_changed = prev_state.map(|s| s != current.state).unwrap_or(false);

        // 3. Workset Item Diffs (match by item ID or matching title+item_type)
        let mut added_items = Vec::new();
        let mut retained_items = Vec::new();
        let mut removed_items = Vec::new();

        for curr_item in &current.workset_item_references {
            let matched = prev_items.iter().any(|pi| {
                pi.id == curr_item.id
                    || (pi.title.is_some()
                        && pi.title == curr_item.title
                        && pi.item_type == curr_item.item_type)
            });
            if matched {
                retained_items.push(curr_item.clone());
            } else {
                added_items.push(curr_item.clone());
            }
        }

        for prev_item in prev_items {
            let matched = current.workset_item_references.iter().any(|ci| {
                ci.id == prev_item.id
                    || (ci.title.is_some()
                        && ci.title == prev_item.title
                        && ci.item_type == prev_item.item_type)
            });
            if !matched {
                removed_items.push(prev_item.clone());
            }
        }

        // 4. Readiness & Confidence
        let current_readiness = current.resume_readiness.readiness;
        let readiness_changed = prev_readiness
            .map(|r| r != current_readiness)
            .unwrap_or(false);
        let confidence_delta = if previous.is_some() {
            current.snapshot_confidence - prev_conf
        } else {
            0.0
        };

        // 5. Meaningful changes flag
        let has_meaningful_changes = task_changed
            || state_changed
            || !added_items.is_empty()
            || !removed_items.is_empty()
            || readiness_changed;

        // 6. Formatted Summary (Deterministic factual explanation)
        let formatted_diff_summary = Self::format_summary(
            &current.context_name,
            task_change_summary.as_deref(),
            &added_items,
            &removed_items,
            state_changed,
            prev_state,
            current.state,
        );

        ContextSnapshotDiff {
            context_id: current.context_id.clone(),
            context_name: current.context_name.clone(),
            from_snapshot_id: from_id,
            to_snapshot_id: current.id.clone(),
            from_timestamp: from_ts,
            to_timestamp: current.created_at.clone(),
            task_changed,
            previous_task: prev_task,
            current_task: current.task_title.clone(),
            task_change_summary,
            state_changed,
            previous_state: prev_state,
            current_state: current.state,
            added_items,
            removed_items,
            retained_items,
            readiness_changed,
            previous_readiness: prev_readiness,
            current_readiness,
            confidence_delta,
            formatted_diff_summary,
            has_meaningful_changes,
        }
    }

    /// Formats a concise, factual diff summary answering:
    /// "What changed since I last worked on this?"
    fn format_summary(
        context_name: &str,
        task_change_summary: Option<&str>,
        added_items: &[SnapshotWorksetItemReference],
        removed_items: &[SnapshotWorksetItemReference],
        state_changed: bool,
        prev_state: Option<ContextStateValue>,
        current_state: ContextStateValue,
    ) -> String {
        let mut sections = Vec::new();

        if let Some(task_summary) = task_change_summary {
            sections.push(format!("Task changed:\n{}", task_summary));
        }

        if !added_items.is_empty() {
            let item_names: Vec<String> = added_items
                .iter()
                .map(|i| i.title.clone().unwrap_or_else(|| i.id.clone()))
                .collect();
            sections.push(format!("Added:\n{}", item_names.join("\n")));
        }

        if !removed_items.is_empty() {
            let item_names: Vec<String> = removed_items
                .iter()
                .map(|i| i.title.clone().unwrap_or_else(|| i.id.clone()))
                .collect();
            sections.push(format!("Removed:\n{}", item_names.join("\n")));
        }

        if state_changed {
            if let Some(ps) = prev_state {
                sections.push(format!(
                    "State changed:\n{} → {}",
                    ps.display_name(),
                    current_state.display_name()
                ));
            }
        }

        if sections.is_empty() {
            format!("No significant changes recorded for {}.", context_name)
        } else {
            sections.join("\n\n")
        }
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

    // =========================================================================
    // STEP 5 TESTS: Snapshot Creation Triggers, Coalescing & Caching
    // =========================================================================

    #[test]
    fn test_trigger_meaningful_events_classification() {
        let meaningful_events = vec![
            "task_transition",
            "context_transition",
            "context_return",
            "segment_completed",
            "workset_evolved",
            "user_confirm_context",
            "user_correct_context",
            "reconstruct_snapshot",
        ];

        for event in meaningful_events {
            let trigger = SnapshotTriggerClassifier::classify_event(event, false);
            assert!(
                trigger.is_meaningful(),
                "Event '{}' must be classified as meaningful",
                event
            );
            assert!(
                !trigger.is_suppressed(),
                "Event '{}' must NOT be suppressed",
                event
            );
        }
    }

    #[test]
    fn test_trigger_noisy_events_suppressed() {
        let noisy_events = vec![
            "clipboard_capture",
            "clipboard_changed",
            "app_switch",
            "window_focus",
            "render",
            "render_frame",
            "mouse_move",
            "cursor_movement",
        ];

        for event in noisy_events {
            let trigger = SnapshotTriggerClassifier::classify_event(event, false);
            assert!(
                trigger.is_suppressed(),
                "Noisy event '{}' must be suppressed",
                event
            );
            assert!(
                !trigger.is_meaningful(),
                "Noisy event '{}' must not be meaningful",
                event
            );
        }

        // Trivial duplicate items must also be suppressed unconditionally
        let dup_trigger = SnapshotTriggerClassifier::classify_event("task_transition", true);
        assert!(dup_trigger.is_suppressed());
    }

    #[test]
    fn test_coalescer_noisy_burst_creates_zero_scheduled_snapshots() {
        let coalescer = SnapshotTriggerCoalescer::new();

        // Simulate a rapid burst of 200 noisy events (clipboard, mouse, app focus)
        for _ in 0..100 {
            let t1 = SnapshotTriggerClassifier::classify_event("clipboard_capture", false);
            let t2 = SnapshotTriggerClassifier::classify_event("mouse_move", false);
            assert!(!coalescer.schedule("ws-1", t1));
            assert!(!coalescer.schedule("ws-1", t2));
        }

        assert_eq!(coalescer.total_received_count(), 200);
        assert_eq!(coalescer.total_suppressed_count(), 200);
        assert_eq!(coalescer.total_scheduled_count(), 0);
        assert_eq!(coalescer.pending_contexts_count(), 0);
        assert!(coalescer.pop_ready().is_empty());
    }

    #[test]
    fn test_coalescer_meaningful_triggers_coalesce() {
        let coalescer = SnapshotTriggerCoalescer::new()
            .with_debounce(Duration::from_millis(500));

        // Schedule 5 rapid meaningful triggers for "ws-aurora"
        for i in 0..5 {
            let trigger = SnapshotTrigger::TaskTransition {
                task_id: Some(format!("t-{}", i)),
                task_title: format!("Task Version {}", i),
                transition_type: "inferred".to_string(),
            };
            assert!(coalescer.schedule("ws-aurora", trigger));
        }

        assert_eq!(coalescer.total_scheduled_count(), 5);
        assert_eq!(coalescer.total_coalesced_count(), 4);
        assert_eq!(coalescer.pending_contexts_count(), 1);
        assert_eq!(coalescer.coalesce_count("ws-aurora"), 5);

        // Before debounce window expires, pop_ready should return nothing
        let immediate_ready = coalescer.pop_ready_with_debounce(Duration::from_secs(10));
        assert!(immediate_ready.is_empty());

        // With zero debounce window, the coalesced trigger is popped as 1 single snapshot job
        let ready = coalescer.pop_ready_with_debounce(Duration::from_millis(0));
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].0, "ws-aurora");
        if let SnapshotTrigger::TaskTransition { ref task_title, .. } = ready[0].1 {
            assert_eq!(task_title, "Task Version 4");
        } else {
            panic!("Expected latest task transition trigger");
        }

        // Queue is now empty
        assert_eq!(coalescer.pending_contexts_count(), 0);
    }

    #[test]
    fn test_coalescer_high_priority_bypass_debounce() {
        let coalescer = SnapshotTriggerCoalescer::new()
            .with_debounce(Duration::from_secs(60)); // Long debounce

        let trigger = SnapshotTrigger::UserConfirmation {
            entity_type: "context_item".to_string(),
            entity_id: "item-123".to_string(),
        };

        assert!(trigger.is_high_priority());
        assert!(coalescer.schedule("ws-priority", trigger));

        // High priority bypasses the 60s debounce and is immediately ready
        let ready = coalescer.pop_ready();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].0, "ws-priority");
    }

    #[test]
    fn test_snapshot_cache_invalidation_and_ttl() {
        let cache = ContextSnapshotCache::new().with_ttl(Duration::from_secs(5));
        let snapshot = ContextSnapshotBuilder::new("ws-cache", "Cached Workspace")
            .with_state(ContextStateValue::Active)
            .build();

        assert_eq!(cache.get("ws-cache"), None);

        cache.insert("ws-cache", snapshot.clone());
        assert_eq!(cache.get("ws-cache"), Some(snapshot));

        // Test explicit invalidation
        cache.invalidate("ws-cache");
        assert_eq!(cache.get("ws-cache"), None);
    }

    #[test]
    fn test_service_handle_trigger_e2e() {
        let conn = setup_test_db();
        let ws = crate::db::operations::create_workspace(&conn, "Trigger Service WS").unwrap();

        let _ = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://react.dev",
            Some("React Docs"),
            r#"{"app_name": "Chrome"}"#,
        )
        .unwrap();

        let service = ContextSnapshotService::new();

        // 1. Noisy trigger returns None
        let noise = SnapshotTrigger::SuppressedNoise {
            event_type: "mouse_move".to_string(),
            reason: "gesture".to_string(),
        };
        let res_noise = service.handle_trigger(&conn, &ws.id, noise).unwrap();
        assert!(res_noise.is_none());

        // 2. Meaningful trigger produces and caches snapshot
        let trigger = SnapshotTrigger::UserConfirmation {
            entity_type: "workspace".to_string(),
            entity_id: ws.id.clone(),
        };
        let res_meaningful = service.handle_trigger(&conn, &ws.id, trigger).unwrap();
        assert!(res_meaningful.is_some());
        let snap = res_meaningful.unwrap();
        assert_eq!(snap.context_id, ws.id);

        // Cached retrieval is immediate
        let cached = service.derive_for_context_cached(&conn, &ws.id).unwrap();
        assert_eq!(cached.id, snap.id);
    }

    // =========================================================================
    // STEP 6 TESTS: Snapshot History & Bounded Retention
    // =========================================================================

    #[test]
    fn test_snapshot_history_progression_aurora_website() {
        // Example from prompt:
        // Aurora Website
        // Snapshot 1: Hero design
        // Snapshot 2: Hero animation
        // Snapshot 3: Responsive hero implementation
        let mut history = ContextSnapshotHistory::new("ws-aurora", "Aurora Website");

        // 1. Snapshot 1: Hero design
        let snap1 = ContextSnapshotBuilder::new("ws-aurora", "Aurora Website")
            .with_state(ContextStateValue::Active)
            .with_freshness(StateFreshness::Fresh, 3600 * 5)
            .with_task(
                Some("t1".to_string()),
                "Hero design",
                0.90,
                InformationProvenance::Inferred,
            )
            .add_workset_item(create_dummy_workset_item(
                "item-figma-hero",
                "Hero Layout Design",
                WorksetItemRelevance::Primary,
                0.95,
                Some("Figma"),
            ))
            .build();

        assert!(history.add_snapshot(snap1.clone()));
        assert_eq!(history.len(), 1);
        assert_eq!(
            history.latest().and_then(|s| s.task_title.as_deref()),
            Some("Hero design")
        );
        assert!(history.previous().is_none());

        // 2. Snapshot 2: Hero animation
        let snap2 = ContextSnapshotBuilder::new("ws-aurora", "Aurora Website")
            .with_state(ContextStateValue::Active)
            .with_freshness(StateFreshness::Fresh, 3600 * 2)
            .with_task(
                Some("t2".to_string()),
                "Hero animation",
                0.92,
                InformationProvenance::Inferred,
            )
            .add_workset_item(create_dummy_workset_item(
                "item-figma-hero",
                "Hero Layout Design",
                WorksetItemRelevance::Supporting,
                0.80,
                Some("Figma"),
            ))
            .add_workset_item(create_dummy_workset_item(
                "item-motion-tsx",
                "HeroAnimation.tsx",
                WorksetItemRelevance::Primary,
                0.96,
                Some("VS Code"),
            ))
            .build();

        assert!(history.add_snapshot(snap2.clone()));
        assert_eq!(history.len(), 2);
        assert_eq!(
            history.latest().and_then(|s| s.task_title.as_deref()),
            Some("Hero animation")
        );
        assert_eq!(
            history.previous().and_then(|s| s.task_title.as_deref()),
            Some("Hero design")
        );

        // 3. Snapshot 3: Responsive hero implementation
        let snap3 = ContextSnapshotBuilder::new("ws-aurora", "Aurora Website")
            .with_state(ContextStateValue::Active)
            .with_freshness(StateFreshness::Fresh, 300)
            .with_task(
                Some("t3".to_string()),
                "Responsive hero implementation",
                0.95,
                InformationProvenance::Confirmed,
            )
            .add_workset_item(create_dummy_workset_item(
                "item-motion-tsx",
                "HeroAnimation.tsx",
                WorksetItemRelevance::Primary,
                0.96,
                Some("VS Code"),
            ))
            .add_workset_item(create_dummy_workset_item(
                "item-media-css",
                "hero-responsive.css",
                WorksetItemRelevance::Supporting,
                0.88,
                Some("VS Code"),
            ))
            .build();

        assert!(history.add_snapshot(snap3.clone()));
        assert_eq!(history.len(), 3);

        // Verify latest, previous, nth_previous
        let latest = history.latest().unwrap();
        assert_eq!(
            latest.task_title.as_deref(),
            Some("Responsive hero implementation")
        );
        assert_eq!(latest.state, ContextStateValue::Active);
        assert_eq!(latest.core_items().len(), 2);
        assert!(!latest.created_at.is_empty());

        let previous = history.previous().unwrap();
        assert_eq!(previous.task_title.as_deref(), Some("Hero animation"));

        let earliest = history.nth_previous(2).unwrap();
        assert_eq!(earliest.task_title.as_deref(), Some("Hero design"));

        assert!(history.nth_previous(3).is_none());

        // Test search by task
        let found_animation = history.find_by_task("animation");
        assert_eq!(found_animation.len(), 1);
        assert_eq!(
            found_animation[0].task_title.as_deref(),
            Some("Hero animation")
        );

        let found_hero = history.find_by_task("hero");
        assert_eq!(found_hero.len(), 3);
    }

    #[test]
    fn test_snapshot_history_bounded_retention() {
        let mut history = ContextSnapshotHistory::new("ws-bounded", "Bounded Workspace")
            .with_max_capacity(3);

        // Add 6 snapshots with distinct task titles
        for i in 1..=6 {
            let snap = ContextSnapshotBuilder::new("ws-bounded", "Bounded Workspace")
                .with_state(ContextStateValue::Active)
                .with_task(
                    Some(format!("t-{}", i)),
                    format!("Task Milestone {}", i),
                    0.90,
                    InformationProvenance::Inferred,
                )
                .add_workset_item(create_dummy_workset_item(
                    &format!("item-{}", i),
                    &format!("Item {}", i),
                    WorksetItemRelevance::Primary,
                    0.90,
                    Some("VS Code"),
                ))
                .build();
            history.add_snapshot(snap);
        }

        // Capacity is 3, so only the 3 newest snapshots (Milestones 4, 5, 6) should be retained
        assert_eq!(history.len(), 3);
        assert_eq!(
            history.latest().and_then(|s| s.task_title.as_deref()),
            Some("Task Milestone 6")
        );
        assert_eq!(
            history.previous().and_then(|s| s.task_title.as_deref()),
            Some("Task Milestone 5")
        );
        assert_eq!(
            history.nth_previous(2).and_then(|s| s.task_title.as_deref()),
            Some("Task Milestone 4")
        );
        assert!(history.nth_previous(3).is_none());

        // Milestones 1, 2, 3 have been dropped from the history buffer
        assert!(history.find_by_task("Milestone 1").is_empty());
        assert!(history.find_by_task("Milestone 2").is_empty());
        assert!(history.find_by_task("Milestone 3").is_empty());
        assert!(!history.find_by_task("Milestone 4").is_empty());
    }

    #[test]
    fn test_snapshot_history_anti_duplication() {
        let mut history = ContextSnapshotHistory::new("ws-dup", "Anti Duplication Test");

        let snap1 = ContextSnapshotBuilder::new("ws-dup", "Anti Duplication Test")
            .with_state(ContextStateValue::Active)
            .with_task(
                Some("t1".to_string()),
                "Identical Task",
                0.90,
                InformationProvenance::Inferred,
            )
            .add_workset_item(create_dummy_workset_item(
                "item-1",
                "Item 1",
                WorksetItemRelevance::Primary,
                0.90,
                Some("VS Code"),
            ))
            .build();

        // Adding for the first time succeeds
        assert!(history.add_snapshot(snap1.clone()));
        assert_eq!(history.len(), 1);

        // Adding identical snapshot with same task, state, and core workset is rejected
        let snap1_dup = ContextSnapshotBuilder::new("ws-dup", "Anti Duplication Test")
            .with_state(ContextStateValue::Active)
            .with_task(
                Some("t1".to_string()),
                "Identical Task",
                0.90,
                InformationProvenance::Inferred,
            )
            .add_workset_item(create_dummy_workset_item(
                "item-1",
                "Item 1",
                WorksetItemRelevance::Primary,
                0.90,
                Some("VS Code"),
            ))
            .build();

        assert!(!history.add_snapshot(snap1_dup));
        assert_eq!(history.len(), 1);

        // A snapshot with a different task succeeds
        let snap2 = ContextSnapshotBuilder::new("ws-dup", "Anti Duplication Test")
            .with_state(ContextStateValue::Active)
            .with_task(
                Some("t2".to_string()),
                "Different Task",
                0.90,
                InformationProvenance::Inferred,
            )
            .add_workset_item(create_dummy_workset_item(
                "item-1",
                "Item 1",
                WorksetItemRelevance::Primary,
                0.90,
                Some("VS Code"),
            ))
            .build();

        assert!(history.add_snapshot(snap2));
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn test_snapshot_history_repository_and_service() {
        let repo = ContextSnapshotHistoryRepository::new().with_default_capacity(5);

        let snap = ContextSnapshotBuilder::new("ws-repo", "Repo Test")
            .with_state(ContextStateValue::Active)
            .with_task(
                Some("t-repo".to_string()),
                "Repository Task",
                0.95,
                InformationProvenance::Confirmed,
            )
            .add_workset_item(create_dummy_workset_item(
                "item-repo",
                "Repo Doc",
                WorksetItemRelevance::Primary,
                0.90,
                Some("VS Code"),
            ))
            .build();

        assert!(repo.record_snapshot(snap.clone()));
        assert_eq!(
            repo.get_latest("ws-repo").and_then(|s| s.task_title),
            Some("Repository Task".to_string())
        );
        assert!(repo.get_previous("ws-repo").is_none());

        let history = repo.get_history("ws-repo").unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history.context_id, "ws-repo");

        repo.clear_context("ws-repo");
        assert!(repo.get_history("ws-repo").is_none());
    }

    // =========================================================================
    // STEP 7 TESTS: Snapshot Comparison & Differential Analysis
    // =========================================================================

    #[test]
    fn test_snapshot_comparison_example_aurora_website() {
        // Example from prompt:
        // Previous:
        // Task: Hero design
        // Workset: Figma design, Hero reference
        //
        // Current:
        // Task: Hero animation
        // Workset: React Hero component, Framer Motion reference, GitHub repository
        //
        // Output:
        // Task changed: Hero design → Hero animation
        // Added: React Hero component, Framer Motion reference, GitHub repository
        // Removed: Figma design, Hero reference (Old hero reference)
        let prev_snap = ContextSnapshotBuilder::new("ws-aurora", "Aurora Website")
            .with_state(ContextStateValue::Active)
            .with_task(
                Some("t1".to_string()),
                "Hero design",
                0.90,
                InformationProvenance::Inferred,
            )
            .add_workset_item(create_dummy_workset_item(
                "item-figma",
                "Figma design",
                WorksetItemRelevance::Primary,
                0.95,
                Some("Figma"),
            ))
            .add_workset_item(create_dummy_workset_item(
                "item-hero-ref",
                "Hero reference",
                WorksetItemRelevance::Supporting,
                0.80,
                Some("Chrome"),
            ))
            .build();

        let curr_snap = ContextSnapshotBuilder::new("ws-aurora", "Aurora Website")
            .with_state(ContextStateValue::Active)
            .with_task(
                Some("t2".to_string()),
                "Hero animation",
                0.95,
                InformationProvenance::Confirmed,
            )
            .add_workset_item(create_dummy_workset_item(
                "item-react-hero",
                "React Hero component",
                WorksetItemRelevance::Primary,
                0.98,
                Some("VS Code"),
            ))
            .add_workset_item(create_dummy_workset_item(
                "item-framer-motion",
                "Framer Motion reference",
                WorksetItemRelevance::Supporting,
                0.88,
                Some("Chrome"),
            ))
            .add_workset_item(create_dummy_workset_item(
                "item-github-repo",
                "GitHub repository",
                WorksetItemRelevance::Supporting,
                0.85,
                Some("Chrome"),
            ))
            .build();

        let diff = ContextSnapshotComparator::compare(Some(&prev_snap), &curr_snap);

        assert_eq!(diff.context_id, "ws-aurora");
        assert_eq!(diff.context_name, "Aurora Website");
        assert_eq!(diff.from_snapshot_id, Some(prev_snap.id.clone()));
        assert_eq!(diff.to_snapshot_id, curr_snap.id.clone());

        // 1. Task changed
        assert!(diff.task_changed);
        assert_eq!(diff.previous_task.as_deref(), Some("Hero design"));
        assert_eq!(diff.current_task.as_deref(), Some("Hero animation"));
        assert_eq!(
            diff.task_change_summary.as_deref(),
            Some("Hero design → Hero animation")
        );

        // 2. Added items
        assert_eq!(diff.added_count(), 3);
        let added_titles: Vec<&str> = diff
            .added_items
            .iter()
            .filter_map(|i| i.title.as_deref())
            .collect();
        assert!(added_titles.contains(&"React Hero component"));
        assert!(added_titles.contains(&"Framer Motion reference"));
        assert!(added_titles.contains(&"GitHub repository"));

        // 3. Removed items
        assert_eq!(diff.removed_count(), 2);
        let removed_titles: Vec<&str> = diff
            .removed_items
            .iter()
            .filter_map(|i| i.title.as_deref())
            .collect();
        assert!(removed_titles.contains(&"Figma design"));
        assert!(removed_titles.contains(&"Hero reference"));

        // 4. Formatted summary checks
        assert!(diff
            .formatted_diff_summary
            .contains("Task changed:\nHero design → Hero animation"));
        assert!(diff.formatted_diff_summary.contains("Added:"));
        assert!(diff.formatted_diff_summary.contains("React Hero component"));
        assert!(diff.formatted_diff_summary.contains("Removed:"));
        assert!(diff.formatted_diff_summary.contains("Figma design"));

        // 5. Invariant check: Pure factual statement without mental speculation
        assert!(diff.has_meaningful_changes);
        assert!(!diff.is_empty());
    }

    #[test]
    fn test_snapshot_comparison_no_changes() {
        let snap1 = ContextSnapshotBuilder::new("ws-static", "Static Workspace")
            .with_state(ContextStateValue::Active)
            .with_task(
                Some("t1".to_string()),
                "Ongoing Task",
                0.90,
                InformationProvenance::Inferred,
            )
            .add_workset_item(create_dummy_workset_item(
                "i1",
                "Spec Doc",
                WorksetItemRelevance::Primary,
                0.90,
                Some("Chrome"),
            ))
            .build();

        let snap2 = snap1.clone();

        let diff = ContextSnapshotComparator::compare(Some(&snap1), &snap2);

        assert!(!diff.task_changed);
        assert!(!diff.state_changed);
        assert!(!diff.readiness_changed);
        assert_eq!(diff.added_count(), 0);
        assert_eq!(diff.removed_count(), 0);
        assert_eq!(diff.retained_count(), 1);
        assert!(!diff.has_meaningful_changes);
        assert!(diff.is_empty());
        assert!(diff
            .formatted_diff_summary
            .contains("No significant changes recorded"));
    }

    #[test]
    fn test_snapshot_comparison_state_and_readiness_changes() {
        let snap1 = ContextSnapshotBuilder::new("ws-dormant-switch", "Project Delta")
            .with_state(ContextStateValue::Active)
            .with_freshness(StateFreshness::Fresh, 300)
            .with_task(
                Some("t-delta".to_string()),
                "Feature Delta",
                0.95,
                InformationProvenance::Confirmed,
            )
            .with_confidence(0.95)
            .add_workset_item(create_dummy_workset_item(
                "item-1",
                "Shared Item",
                WorksetItemRelevance::Primary,
                0.90,
                Some("VS Code"),
            ))
            .build();

        let snap2 = ContextSnapshotBuilder::new("ws-dormant-switch", "Project Delta")
            .with_state(ContextStateValue::Dormant)
            .with_freshness(StateFreshness::VeryStale, 86400 * 10)
            .with_task(
                Some("t-delta".to_string()),
                "Feature Delta",
                0.60,
                InformationProvenance::Inferred,
            )
            .with_confidence(0.60)
            .add_workset_item(create_dummy_workset_item(
                "item-1",
                "Shared Item",
                WorksetItemRelevance::Supporting,
                0.60,
                Some("VS Code"),
            ))
            .build();

        let diff = ContextSnapshotComparator::compare(Some(&snap1), &snap2);

        assert!(!diff.task_changed);
        assert!(diff.state_changed);
        assert_eq!(diff.previous_state, Some(ContextStateValue::Active));
        assert_eq!(diff.current_state, ContextStateValue::Dormant);
        assert!(diff.readiness_changed);
        assert_eq!(diff.retained_count(), 1);
        assert_eq!(diff.added_count(), 0);
        assert_eq!(diff.removed_count(), 0);
        assert!(diff.confidence_delta < 0.0);
        assert!(diff.formatted_diff_summary.contains("State changed:"));
    }

    #[test]
    fn test_snapshot_comparison_baseline_none() {
        let curr = ContextSnapshotBuilder::new("ws-first", "First Snapshot")
            .with_state(ContextStateValue::Active)
            .with_task(
                Some("t-init".to_string()),
                "Initial Setup",
                0.90,
                InformationProvenance::Inferred,
            )
            .add_workset_item(create_dummy_workset_item(
                "i1",
                "Init Config",
                WorksetItemRelevance::Primary,
                0.90,
                Some("VS Code"),
            ))
            .build();

        let diff = ContextSnapshotComparator::compare(None, &curr);

        assert_eq!(diff.from_snapshot_id, None);
        assert_eq!(diff.from_timestamp, None);
        assert!(diff.task_changed);
        assert_eq!(
            diff.task_change_summary.as_deref(),
            Some("Started task: Initial Setup")
        );
        assert_eq!(diff.added_count(), 1);
        assert_eq!(diff.removed_count(), 0);
        assert!(diff.has_meaningful_changes);
    }

    #[test]
    fn test_snapshot_history_and_service_comparison_convenience_methods() {
        let mut history = ContextSnapshotHistory::new("ws-history-diff", "History Diff WS");

        let snap1 = ContextSnapshotBuilder::new("ws-history-diff", "History Diff WS")
            .with_state(ContextStateValue::Active)
            .with_task(
                Some("t1".to_string()),
                "Design Phase",
                0.90,
                InformationProvenance::Inferred,
            )
            .add_workset_item(create_dummy_workset_item(
                "item-a",
                "Mockup A",
                WorksetItemRelevance::Primary,
                0.90,
                Some("Figma"),
            ))
            .build();

        let snap2 = ContextSnapshotBuilder::new("ws-history-diff", "History Diff WS")
            .with_state(ContextStateValue::Active)
            .with_task(
                Some("t2".to_string()),
                "Implementation Phase",
                0.95,
                InformationProvenance::Confirmed,
            )
            .add_workset_item(create_dummy_workset_item(
                "item-b",
                "Component B",
                WorksetItemRelevance::Primary,
                0.95,
                Some("VS Code"),
            ))
            .build();

        let id1 = snap1.id.clone();
        let id2 = snap2.id.clone();

        history.add_snapshot(snap1);
        history.add_snapshot(snap2);

        // 1. compare_latest_with_previous
        let latest_diff = history.compare_latest_with_previous().unwrap();
        assert!(latest_diff.task_changed);
        assert_eq!(
            latest_diff.task_change_summary.as_deref(),
            Some("Design Phase → Implementation Phase")
        );
        assert_eq!(latest_diff.added_count(), 1);
        assert_eq!(latest_diff.removed_count(), 1);

        // 2. compare_snapshots by ID
        let id_diff = history.compare_snapshots(&id1, &id2).unwrap();
        assert_eq!(id_diff.from_snapshot_id, Some(id1));
        assert_eq!(id_diff.to_snapshot_id, id2);

        // 3. Service direct comparison
        let service = ContextSnapshotService::new();
        let s_diff = service.compare_snapshots(
            history.nth_previous(1),
            history.latest().unwrap(),
        );
        assert_eq!(s_diff.task_change_summary, latest_diff.task_change_summary);
    }

    #[test]
    fn test_privacy_scrubbing_on_snapshot_references() {
        use crate::context_engine::workset::WorksetItemReference;

        // 1. WorksetItemReference with secrets
        let item_with_secret = WorksetItemReference {
            id: "item-secret-1".to_string(),
            title: Some("Database config: postgres://admin:secretpass123@db.example.com/mydb".to_string()),
            item_type: "text".to_string(),
            relevance: WorksetItemRelevance::Primary,
            relevance_score: 0.95,
            provenance: InformationProvenance::Observed,
            app_name: Some("Terminal".to_string()),
            domain: None,
            content_snippet: "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.e30.t-IDsk_sample and API key sk-abcdef1234567890abcdef1234567890".to_string(),
            reasons: vec!["password: mySuperSecretPassword!".to_string()],
            created_at: Utc::now().to_rfc3339(),
            last_active_at: Some(Utc::now().to_rfc3339()),
            is_suggested_addition: false,
        };

        let snapshot_ref = SnapshotWorksetItemReference::from(&item_with_secret);

        assert!(!snapshot_ref.title.as_ref().unwrap().contains("secretpass123"));
        assert!(snapshot_ref.title.as_ref().unwrap().contains("[CONNECTION_STRING_REDACTED]"));
        assert!(!snapshot_ref.content_snippet.contains("Bearer eyJ"));
        assert!(!snapshot_ref.content_snippet.contains("sk-abcdef1234567890"));
        assert!(snapshot_ref.content_snippet.contains("[REDACTED]") || snapshot_ref.content_snippet.contains("[API_KEY_REDACTED]"));
        assert!(!snapshot_ref.reasons[0].contains("mySuperSecretPassword"));

        // 2. Task reference with credentials
        let task_cand = TaskCandidate {
            id: "tc-1".to_string(),
            context_id: Some("ws-1".to_string()),
            title: "Fix bug with token Bearer abcdef1234567890~+=".to_string(),
            confidence: 0.85,
            status: crate::db::models::TaskCandidateStatus::Detected,
            first_detected_at: Utc::now().to_rfc3339(),
            last_active_at: Utc::now().to_rfc3339(),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };

        let snap_task = SnapshotTaskReference::from(&task_cand);
        assert!(!snap_task.title.contains("abcdef1234567890"));
        assert!(snap_task.title.contains("Bearer [REDACTED]"));
    }

    #[test]
    fn test_privacy_setting_disabled_suppresses_snapshots() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();

        // Explicitly disable context snapshots in settings
        let mut settings = crate::db::settings::get_settings(&conn).unwrap();
        settings.enable_context_snapshots = false;
        crate::db::settings::update_settings(&conn, settings).unwrap();

        assert!(!ContextSnapshotService::is_snapshots_enabled(&conn));

        let service = ContextSnapshotService::new();
        let trigger = SnapshotTrigger::TaskTransition {
            task_id: Some("t-code".to_string()),
            task_title: "Code".to_string(),
            transition_type: "switch".to_string(),
        };

        // When disabled, handle_trigger must return Ok(None) and not generate snapshots
        let result = service.handle_trigger(&conn, "test-ws", trigger).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_privacy_setting_disabled_preserves_existing_history() {
        let repo = ContextSnapshotHistoryRepository::new();
        let snap = ContextSnapshotBuilder::new("ws-persist-test", "Persist WS")
            .with_state(ContextStateValue::Active)
            .build();

        let snap_id = snap.id.clone();
        assert!(repo.record_snapshot(snap));

        // When setting is disabled, existing history remains intact
        let history = repo.get_history("ws-persist-test").unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history.latest().unwrap().id, snap_id);
    }

    #[test]
    fn test_storage_optimization_bounded_history_pruning() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();

        // Create a workspace and underlying items
        let ws = crate::db::operations::create_workspace(&conn, "Storage WS").unwrap();
        let item1 = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Core item 1",
            Some("My Important Doc"),
            "{}",
        )
        .unwrap();

        let service = ContextSnapshotService::new();

        // Generate and record 40 snapshots with different tasks
        for i in 0..40 {
            let snap = ContextSnapshotBuilder::new(&ws.id, &ws.name)
                .with_task(
                    Some(format!("t-{}", i)),
                    &format!("Task Iteration {}", i),
                    0.80,
                    InformationProvenance::Inferred,
                )
                .add_workset_item(create_dummy_workset_item(
                    &item1.id,
                    "My Important Doc",
                    WorksetItemRelevance::Primary,
                    0.95,
                    Some("Notes"),
                ))
                .build();

            let _ = service.record_snapshot_history_with_db(&conn, snap);
        }

        // 1. Verify DB retention bounded to DEFAULT_MAX_SNAPSHOT_RETENTION (20)
        let db_snaps = crate::db::operations::list_context_snapshots_bounded(&conn, &ws.id, 100).unwrap();
        assert_eq!(db_snaps.len(), DEFAULT_MAX_SNAPSHOT_RETENTION);

        // 2. CRITICAL INVARIANT: Underlying Items MUST NEVER be deleted by snapshot retention pruning!
        let underlying_items = crate::db::operations::list_items(&conn, Some(&ws.id)).unwrap();
        assert_eq!(underlying_items.len(), 1);
        assert_eq!(underlying_items[0].id, item1.id);
    }

    #[test]
    fn test_storage_performance_benchmark() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();

        let ws = crate::db::operations::create_workspace(&conn, "Perf WS").unwrap();
        let service = ContextSnapshotService::new();

        let start = Instant::now();

        // Run 50 snapshot derivations, comparisons, and DB storage cycles
        for i in 0..50 {
            let snap = ContextSnapshotBuilder::new(&ws.id, &ws.name)
                .with_task(
                    Some(format!("t-perf-{}", i)),
                    &format!("Performance Task {}", i),
                    0.85,
                    InformationProvenance::Inferred,
                )
                .build();

            let _ = service.record_snapshot_history_with_db(&conn, snap);
            let _ = service.compare_latest_with_previous(&ws.id);
        }

        let elapsed = start.elapsed();
        // 50 full cycles with SQLite inserts + pruning should complete well under 500ms
        assert!(
            elapsed < Duration::from_millis(500),
            "Snapshot storage pipeline took too long: {:?}",
            elapsed
        );
    }

    #[test]
    fn test_snapshot_integrates_important_memory_references_lightweight() {
        use crate::context_engine::important_memory::{
            ImportantMemoryType, MemoryImportanceBand,
        };

        // 1. Construct lightweight important memory references (e.g., GitHub repo, Figma project, Hero design reference)
        let repo_ref = SnapshotImportantMemoryReference {
            id: "mem-repo-1".to_string(),
            source_id: "item-github-123".to_string(),
            memory_type: ImportantMemoryType::ImportantReference,
            title: "Aurora GitHub repository".to_string(),
            detail: Some("github.com/org/aurora-web".to_string()),
            importance: 0.95,
            importance_band: MemoryImportanceBand::Critical,
            confidence: 0.92,
            provenance: InformationProvenance::Confirmed,
            first_seen_at: Some("2026-09-10T10:00:00Z".to_string()),
            last_seen_at: Some("2026-09-17T18:00:00Z".to_string()),
            app_name: Some("Google Chrome".to_string()),
            domain: Some("github.com".to_string()),
        };

        let figma_ref = SnapshotImportantMemoryReference {
            id: "mem-figma-2".to_string(),
            source_id: "item-figma-456".to_string(),
            memory_type: ImportantMemoryType::ImportantReference,
            title: "Figma project".to_string(),
            detail: Some("figma.com/file/aurora-hero".to_string()),
            importance: 0.88,
            importance_band: MemoryImportanceBand::High,
            confidence: 0.89,
            provenance: InformationProvenance::Observed,
            first_seen_at: Some("2026-09-12T11:00:00Z".to_string()),
            last_seen_at: Some("2026-09-17T18:30:00Z".to_string()),
            app_name: Some("Figma".to_string()),
            domain: Some("figma.com".to_string()),
        };

        let design_ref = SnapshotImportantMemoryReference {
            id: "mem-design-3".to_string(),
            source_id: "item-hero-spec-789".to_string(),
            memory_type: ImportantMemoryType::ImportantItem,
            title: "Hero design reference".to_string(),
            detail: Some("CSS keyframes and timing curves".to_string()),
            importance: 0.82,
            importance_band: MemoryImportanceBand::High,
            confidence: 0.85,
            provenance: InformationProvenance::Observed,
            first_seen_at: Some("2026-09-15T14:00:00Z".to_string()),
            last_seen_at: Some("2026-09-17T19:00:00Z".to_string()),
            app_name: Some("Cursor".to_string()),
            domain: None,
        };

        // 2. Build snapshot with these important memories
        let snapshot = ContextSnapshotBuilder::new("ws-aurora-1", "Aurora Website")
            .with_task(
                Some("task-hero-anim".to_string()),
                "Hero animation",
                0.90,
                InformationProvenance::Inferred,
            )
            .with_important_memory_references(vec![
                repo_ref.clone(),
                figma_ref.clone(),
                design_ref.clone(),
            ])
            .build();

        // 3. Verify snapshot contains the references and not heavyweight duplicated payload objects
        assert_eq!(snapshot.context_name, "Aurora Website");
        assert_eq!(snapshot.task_title.as_deref(), Some("Hero animation"));
        assert_eq!(snapshot.important_memory_references.len(), 3);

        let titles: Vec<&str> = snapshot
            .important_memory_references
            .iter()
            .map(|r| r.title.as_str())
            .collect();
        assert!(titles.contains(&"Aurora GitHub repository"));
        assert!(titles.contains(&"Figma project"));
        assert!(titles.contains(&"Hero design reference"));

        // Verify JSON serialization includes lightweight references cleanly
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(json.contains("Aurora GitHub repository"));
        assert!(json.contains("important_memory_references"));
        assert!(json.contains("important_reference"));
    }

    #[test]
    fn test_snapshot_distinguishes_historical_memory_vs_current_state_on_sparse_return() {
        use crate::context_engine::important_memory::{
            ImportantMemoryType, MemoryImportanceBand,
        };

        // Scenario: User returns to "Aurora Website" after several days of inactivity.
        // Recent activity is sparse/absent.
        // Snapshot should still know:
        // - Important project references (GitHub, Figma)
        // - Previous task ("Hero animation")
        // - Relevant workset items
        // BUT should NOT pretend the user is still actively in-flight on the old task.

        let repo_ref = SnapshotImportantMemoryReference {
            id: "mem-repo".to_string(),
            source_id: "item-gh".to_string(),
            memory_type: ImportantMemoryType::ImportantReference,
            title: "Aurora GitHub repository".to_string(),
            detail: None,
            importance: 0.92,
            importance_band: MemoryImportanceBand::Critical,
            confidence: 0.95,
            provenance: InformationProvenance::Confirmed,
            first_seen_at: Some("2026-09-01T00:00:00Z".to_string()),
            last_seen_at: Some("2026-09-10T00:00:00Z".to_string()),
            app_name: Some("Safari".to_string()),
            domain: Some("github.com".to_string()),
        };

        let workset_item = create_dummy_workset_item(
            "item-hero-code",
            "Hero.tsx",
            WorksetItemRelevance::Primary,
            0.88,
            Some("Cursor"),
        );

        let previous_task = SnapshotTaskReference {
            id: Some("task-hero".to_string()),
            title: "Hero animation".to_string(),
            action_verb: Some("Animating".to_string()),
            target_subject: Some("Hero".to_string()),
            confidence: 0.85,
            confidence_band: "High".to_string(),
            provenance: InformationProvenance::Inferred,
            first_seen_at: Some("2026-09-10T12:00:00Z".to_string()),
            last_active_at: Some("2026-09-10T18:00:00Z".to_string()),
            status: Some("paused".to_string()),
        };

        // Elapsed time is 3 days (259200 seconds)
        let elapsed_seconds = 3 * 86400;
        let freshness = StateFreshness::classify(elapsed_seconds);
        assert_eq!(freshness, StateFreshness::Stale);

        let snapshot = ContextSnapshotBuilder::new("ws-aurora-ret", "Aurora Website")
            .with_task(
                Some("task-hero".to_string()),
                "Hero animation",
                0.85,
                InformationProvenance::Inferred,
            )
            .with_state(ContextStateValue::Dormant)
            .with_freshness(freshness, elapsed_seconds)
            .with_task_references(vec![previous_task])
            .add_workset_item(workset_item)
            .add_important_memory_reference(repo_ref)
            .build();

        // 1. Verify Historical Memory is preserved
        assert_eq!(snapshot.important_memory_references.len(), 1);
        assert_eq!(
            snapshot.important_memory_references[0].title,
            "Aurora GitHub repository"
        );
        assert_eq!(snapshot.recent_task_references.len(), 1);
        assert_eq!(
            snapshot.recent_task_references[0].title,
            "Hero animation"
        );
        assert_eq!(snapshot.workset_item_references.len(), 1);

        // 2. Verify Current State is explicitly distinguished as Dormant / Stale
        assert_eq!(snapshot.state, ContextStateValue::Dormant);
        assert_eq!(snapshot.state_freshness, StateFreshness::Stale);
        assert!(snapshot.is_dormant());
        assert!(!snapshot.is_working()); // NOT actively in flight

        // 3. Statement reflects historical departure rather than active session
        assert!(
            snapshot.snapshot_statement.contains("Left off on task 'Hero animation'"),
            "Statement was: {}",
            snapshot.snapshot_statement
        );
        assert!(
            snapshot.snapshot_statement.contains("several days ago")
                || snapshot.snapshot_statement.contains("stale"),
            "Statement was: {}",
            snapshot.snapshot_statement
        );
    }

    #[test]
    fn test_snapshot_from_db_pulls_promoted_important_memories() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();

        let ws = crate::db::operations::create_workspace(&conn, "Aurora App").unwrap();

        // Create a confirmed/frequent item in DB
        let item = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://github.com/aurora/repo",
            Some("Aurora GitHub repository"),
            "{}",
        )
        .unwrap();

        // Record user memory control action to keep it important
        let _ = crate::context_engine::important_memory::record_context_memory_control(
            &conn,
            &ws.id,
            &item.id,
            crate::context_engine::important_memory::UserMemoryControlAction::KeepImportant,
        );

        // Build snapshot from DB
        let builder = ContextSnapshotBuilder::from_db(&conn, &ws.id).unwrap();
        let snapshot = builder.build();

        assert_eq!(snapshot.context_name, "Aurora App");
        // Verify important memory references contains the confirmed memory
        assert!(
            !snapshot.important_memory_references.is_empty(),
            "Expected snapshot to contain important memory references from DB"
        );
        let mem = snapshot
            .important_memory_references
            .iter()
            .find(|m| m.title == "Aurora GitHub repository")
            .expect("Expected snapshot to contain 'Aurora GitHub repository'");
        assert_eq!(mem.provenance, InformationProvenance::Confirmed);
    }
}



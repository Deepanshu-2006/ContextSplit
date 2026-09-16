use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

use crate::context_engine::memory::{
    ContextMemory, ContextMemoryActivityState, ContextMemoryBuilder, ContextMemoryService,
};
use crate::context_engine::segments::{ContextActivitySegment, ContextActivitySegmenter};
use crate::context_engine::task_extractor::is_generic_task_name;
use crate::context_engine::task_transitions::{
    DetectedTaskTransition, TaskTransitionType,
};
use crate::context_engine::transitions::{
    ContextTransition, ContextTransitionType,
};
use crate::context_engine::workset::{
    is_accidental_capture, is_generic_noise, ContextWorkset, ContextWorksetBuilder,
    ContextWorksetService, WorksetItemRelevance, WorksetState,
};
use crate::db::models::{ContextEvidenceItem, Item, TaskCandidate, TaskCandidateStatus};

// =========================================================================
// ContextStateValue: Conservative Working State Classification
// =========================================================================

/// Conservative classification of the user's working state inside a context.
///
/// Invariants:
/// - Never claim `Active` merely because an item exists.
/// - Never treat application presence as proof of work.
/// - Never treat the last captured item as the complete state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextStateValue {
    /// Actively focused and working inside this context within the immediate active window (<= 15 min).
    Active,
    /// Recently active (15 min - 24 hours ago), ready to resume with warm state.
    RecentlyActive,
    /// Working session was paused (e.g., switched away to another context, ended task, or brief pause).
    Paused,
    /// No activity for an extended period (> 7 days), cold state.
    Dormant,
    /// Conflicting signals, ambiguous context switches, or insufficient stability.
    Uncertain,
    /// Brand new context or zero/negligible evidence to reconstruct state.
    InsufficientEvidence,
}

impl ContextStateValue {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::RecentlyActive => "recently_active",
            Self::Paused => "paused",
            Self::Dormant => "dormant",
            Self::Uncertain => "uncertain",
            Self::InsufficientEvidence => "insufficient_evidence",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::RecentlyActive => "Recently Active",
            Self::Paused => "Paused",
            Self::Dormant => "Dormant",
            Self::Uncertain => "Uncertain",
            Self::InsufficientEvidence => "Insufficient Evidence",
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    pub fn is_recently_active(&self) -> bool {
        matches!(self, Self::RecentlyActive)
    }

    pub fn is_paused(&self) -> bool {
        matches!(self, Self::Paused)
    }

    pub fn is_dormant(&self) -> bool {
        matches!(self, Self::Dormant)
    }

    pub fn is_uncertain(&self) -> bool {
        matches!(self, Self::Uncertain)
    }

    pub fn is_insufficient(&self) -> bool {
        matches!(self, Self::InsufficientEvidence)
    }
}

impl std::fmt::Display for ContextStateValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// =========================================================================
// State Freshness (Day 19 Step 11)
// =========================================================================

/// State freshness classification based on contextual activity timestamps.
///
/// Invariants:
/// - Classifies as: fresh, recent, stale, very_stale.
/// - Does NOT make arbitrary claims such as exact user attention (e.g. "You focused for 2 hours").
/// - UI wording uses defensible phrasing: "Active recently", "Last active yesterday", "Last active 5 days ago".
/// - Influences resume readiness, current context display, and workset prioritization.
/// - NEVER deletes old state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateFreshness {
    /// Active within the last 2 hours.
    Fresh,
    /// Active within the last 48 hours (today or yesterday).
    Recent,
    /// Active within the last 2 to 7 days.
    Stale,
    /// Inactive for more than 7 days, or no recorded activity history.
    VeryStale,
}

impl StateFreshness {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Recent => "recent",
            Self::Stale => "stale",
            Self::VeryStale => "very_stale",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Fresh => "Fresh",
            Self::Recent => "Recent",
            Self::Stale => "Stale",
            Self::VeryStale => "Very Stale",
        }
    }

    pub fn is_fresh(&self) -> bool {
        matches!(self, Self::Fresh)
    }

    pub fn is_recent(&self) -> bool {
        matches!(self, Self::Recent)
    }

    pub fn is_stale(&self) -> bool {
        matches!(self, Self::Stale)
    }

    pub fn is_very_stale(&self) -> bool {
        matches!(self, Self::VeryStale)
    }

    /// Classifies freshness from elapsed seconds since last active contextual event.
    pub fn classify(elapsed_seconds: i64) -> Self {
        if elapsed_seconds < 7200 {
            // < 2 hours
            Self::Fresh
        } else if elapsed_seconds < 172800 {
            // < 48 hours (today / yesterday)
            Self::Recent
        } else if elapsed_seconds < 604800 {
            // 2 - 7 days
            Self::Stale
        } else {
            // > 7 days
            Self::VeryStale
        }
    }

    /// Generates defensible, non-arbitrary display wording for the UI.
    ///
    /// Avoids unsupported attention claims like "You focused for 2 hours".
    pub fn display_wording(&self, elapsed_seconds: i64) -> String {
        if elapsed_seconds < 300 {
            "Active now".to_string()
        } else if elapsed_seconds < 3600 {
            let mins = (elapsed_seconds / 60).max(1);
            format!("Active {}m ago", mins)
        } else if elapsed_seconds < 7200 {
            "Active recently".to_string()
        } else if elapsed_seconds < 86400 {
            let hours = (elapsed_seconds / 3600).max(1);
            if hours == 1 {
                "Last active 1 hour ago".to_string()
            } else {
                format!("Last active {} hours ago", hours)
            }
        } else if elapsed_seconds < 172800 {
            "Last active yesterday".to_string()
        } else {
            let days = elapsed_seconds / 86400;
            if days <= 1 {
                "Last active yesterday".to_string()
            } else {
                format!("Last active {} days ago", days)
            }
        }
    }
}

impl std::fmt::Display for StateFreshness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

fn default_state_freshness() -> StateFreshness {
    StateFreshness::Fresh
}

// =========================================================================
// Meaningful Activity Models & Classification (Day 19 Step 4)
// =========================================================================

/// Activity type classification for a meaningful contextual event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeaningfulActivityType {
    /// User explicitly created, saved, moved, or organized an item in this context.
    UserOrganization,
    /// User confirmed workspace membership, relationship, or task candidate.
    ConfirmedActivity,
    /// Decisive task transition (e.g. task started, resumed, or switched).
    TaskTransition,
    /// Contextual shift (e.g. context return, context switch, or session initiated).
    ContextTransition,
    /// High-confidence or primary task-grounded item capture (e.g., component, spec, doc).
    TaskRelatedItem,
    /// Strongly related or graph-connected supporting item.
    StronglyRelatedItem,
    /// Continuous, coherent contextual activity burst/segment.
    ActivitySegment,
    /// Generic or weak peripheral item (suppressed if higher quality activities exist).
    PeripheralCapture,
}

impl MeaningfulActivityType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UserOrganization => "user_organization",
            Self::ConfirmedActivity => "confirmed_activity",
            Self::TaskTransition => "task_transition",
            Self::ContextTransition => "context_transition",
            Self::TaskRelatedItem => "task_related_item",
            Self::StronglyRelatedItem => "strongly_related_item",
            Self::ActivitySegment => "activity_segment",
            Self::PeripheralCapture => "peripheral_capture",
        }
    }

    pub fn priority_weight(&self) -> f64 {
        match self {
            Self::UserOrganization => 1.00,
            Self::ConfirmedActivity => 0.95,
            Self::TaskTransition => 0.88,
            Self::ContextTransition => 0.82,
            Self::TaskRelatedItem => 0.78,
            Self::StronglyRelatedItem => 0.68,
            Self::ActivitySegment => 0.58,
            Self::PeripheralCapture => 0.20,
        }
    }
}

impl std::fmt::Display for MeaningfulActivityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Entity reference associated with a meaningful activity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeaningfulEntityReference {
    /// "item", "task", "context", "segment", "transition"
    pub entity_type: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Detailed structured representation of the last meaningful activity.
///
/// Filters out raw application launches, window focus events, duplicate captures,
/// and technical internal events in favor of grounded, substantive user focus.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeaningfulActivity {
    /// Meaningful activity classification
    pub activity_type: MeaningfulActivityType,
    /// ISO-8601 timestamp when this activity took place
    pub timestamp: String,
    /// Associated entity reference (item, task, transition, context)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<MeaningfulEntityReference>,
    /// Epistemic classification ("observed", "inferred", "confirmed")
    pub provenance: String,
    /// Calibrated activity confidence [0.0, 1.0]
    pub confidence: f64,
    /// Concise, factual human-readable headline (e.g. "Saved Framer Motion animation reference")
    pub description: String,
    /// Detailed natural explanation answering why this activity was chosen
    pub explanation: String,
}

// =========================================================================
// MeaningfulActivityResolver: Candidate Scoring & Selection Engine
// =========================================================================

/// Resolver responsible for extracting and ranking meaningful contextual activity.
///
/// Strictly ignores:
/// - application launch alone
/// - window switch alone
/// - weak semantic match without corroboration
/// - duplicate captures
/// - unrelated clipboard items
/// - technical/internal events
pub struct MeaningfulActivityResolver;

impl MeaningfulActivityResolver {
    pub fn new() -> Self {
        Self
    }

    /// Resolves the last meaningful activity for a context from available evidence.
    #[allow(clippy::too_many_arguments)]
    pub fn resolve_last_meaningful_activity(
        context_id: &str,
        context_name: Option<&str>,
        items: &[Item],
        workset: Option<&ContextWorkset>,
        _memory: Option<&ContextMemory>,
        segments: &[ContextActivitySegment],
        transitions: &[ContextTransition],
        _task_candidates: &[TaskCandidate],
        task_transitions: &[DetectedTaskTransition],
        has_user_feedback: bool,
    ) -> Option<MeaningfulActivity> {
        let ws_name = context_name.unwrap_or("this workspace");
        let mut candidates: Vec<MeaningfulActivity> = Vec::new();
        let mut seen_keys: HashSet<String> = HashSet::new();

        // 1. User Organization / Confirmed Activity
        for item in items {
            let is_user_confirmed = has_user_feedback
                || item.metadata.contains("user_confirmed")
                || item.metadata.contains("manual");

            let title_clean = item.title.as_deref().unwrap_or("").trim();
            if is_user_confirmed && !title_clean.is_empty() {
                let key = format!("user_org:{}", item.id);
                if seen_keys.insert(key) {
                    candidates.push(MeaningfulActivity {
                        activity_type: MeaningfulActivityType::UserOrganization,
                        timestamp: item.updated_at.clone(),
                        reference: Some(MeaningfulEntityReference {
                            entity_type: "item".to_string(),
                            id: item.id.clone(),
                            title: Some(title_clean.to_string()),
                        }),
                        provenance: "confirmed".to_string(),
                        confidence: 0.95,
                        description: format!("Organized {}", title_clean),
                        explanation: format!("Explicit user action organizing '{}' in {}.", title_clean, ws_name),
                    });
                }
            }
        }

        // 2. Task Transitions
        for tt in task_transitions {
            if tt.context_id.as_deref() == Some(context_id) || tt.context_id.is_none() {
                let (desc, expl) = match tt.transition_type {
                    TaskTransitionType::TaskStart => {
                        let target = tt.to_task.as_deref().unwrap_or("new task");
                        (
                            format!("Started task: {}", target),
                            format!("Began focused activity on '{}' in {}.", target, ws_name),
                        )
                    }
                    TaskTransitionType::TaskSwitch => {
                        let from = tt.from_task.as_deref().unwrap_or("previous task");
                        let to = tt.to_task.as_deref().unwrap_or("new task");
                        (
                            format!("Switched task from {} to {}", from, to),
                            format!("Transitioned working focus from '{}' to '{}'.", from, to),
                        )
                    }
                    TaskTransitionType::TaskReturn => {
                        let target = tt.to_task.as_deref().unwrap_or("task");
                        (
                            format!("Resumed task: {}", target),
                            format!("Resumed previous working task '{}' in {}.", target, ws_name),
                        )
                    }
                    TaskTransitionType::TaskEnd => {
                        let target = tt.from_task.as_deref().unwrap_or("active task");
                        (
                            format!("Completed or paused task: {}", target),
                            format!("Concluded active segment for task '{}'.", target),
                        )
                    }
                    TaskTransitionType::AmbiguousTaskChange => continue,
                };

                let key = format!("task_trans:{}", tt.id);
                if seen_keys.insert(key) {
                    candidates.push(MeaningfulActivity {
                        activity_type: MeaningfulActivityType::TaskTransition,
                        timestamp: tt.timestamp.clone(),
                        reference: Some(MeaningfulEntityReference {
                            entity_type: "task_transition".to_string(),
                            id: tt.id.clone(),
                            title: tt.to_task.clone().or_else(|| tt.from_task.clone()),
                        }),
                        provenance: "inferred".to_string(),
                        confidence: tt.confidence,
                        description: desc,
                        explanation: expl,
                    });
                }
            }
        }

        // 3. Context Transitions
        for trans in transitions {
            if trans.to_context_id.as_deref() == Some(context_id) {
                if trans.transition_type == ContextTransitionType::ContextReturn {
                    let key = format!("ctx_trans:{}", trans.id);
                    if seen_keys.insert(key) {
                        candidates.push(MeaningfulActivity {
                            activity_type: MeaningfulActivityType::ContextTransition,
                            timestamp: trans.detected_at.clone(),
                            reference: Some(MeaningfulEntityReference {
                                entity_type: "context_transition".to_string(),
                                id: trans.id.clone(),
                                title: Some(ws_name.to_string()),
                            }),
                            provenance: "inferred".to_string(),
                            confidence: trans.confidence,
                            description: format!("Returned to {} after interruption", ws_name),
                            explanation: format!("Detected return to '{}' context after temporary detour.", ws_name),
                        });
                    }
                }
            }
        }

        // 4. Task-Related Items from Workset (Primary and Supporting tiers)
        if let Some(ws) = workset {
            for w_item in &ws.items {
                // Filter out generic noise or accidental captures
                if is_generic_noise(w_item.title.as_deref(), &w_item.content_snippet, true, false)
                    || is_accidental_capture(&w_item.item_type, w_item.title.as_deref(), &w_item.content_snippet, false)
                {
                    continue;
                }

                let is_primary = w_item.relevance == WorksetItemRelevance::Primary;
                let is_supporting = w_item.relevance == WorksetItemRelevance::Supporting;

                if is_primary || is_supporting {
                    let act_type = if is_primary {
                        MeaningfulActivityType::TaskRelatedItem
                    } else {
                        MeaningfulActivityType::StronglyRelatedItem
                    };

                    let title_text = w_item.title.as_deref().unwrap_or("content");
                    let app_text = w_item.app_name.as_deref();

                    let desc = if let Some(app) = app_text {
                        format!("Saved {} in {}", title_text, app)
                    } else if let Some(ref dom) = w_item.domain {
                        format!("Saved {} on {}", title_text, dom)
                    } else {
                        format!("Saved {}", title_text)
                    };

                    let expl = if !w_item.reasons.is_empty() {
                        w_item.reasons.join(". ")
                    } else {
                        format!("High-relevance {} item supporting current work in {}.", w_item.relevance.display_name(), ws_name)
                    };

                    let key = format!("workset_item:{}", w_item.id);
                    if seen_keys.insert(key) {
                        candidates.push(MeaningfulActivity {
                            activity_type: act_type,
                            timestamp: w_item.last_active_at.clone().unwrap_or_else(|| w_item.created_at.clone()),
                            reference: Some(MeaningfulEntityReference {
                                entity_type: "item".to_string(),
                                id: w_item.id.clone(),
                                title: w_item.title.clone(),
                            }),
                            provenance: w_item.provenance.as_str().to_string(),
                            confidence: w_item.relevance_score.max(0.65),
                            description: desc,
                            explanation: expl,
                        });
                    }
                }
            }
        }

        // 5. Items directly from Item table (Quality filtered)
        for item in items {
            let title = item.title.as_deref().unwrap_or("").trim();
            if is_generic_noise(Some(title), &item.content, false, false)
                || is_accidental_capture(&item.item_type, Some(title), &item.content, false)
            {
                continue;
            }

            let mut app_name: Option<String> = None;
            let mut domain: Option<String> = None;
            if let Ok(meta) = serde_json::from_str::<Value>(&item.metadata) {
                if let Some(app) = meta.get("app_name").and_then(|v| v.as_str()) {
                    let trimmed = app.trim();
                    if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("unknown") {
                        app_name = Some(trimmed.to_string());
                    }
                }
                if let Some(dom) = meta.get("domain").and_then(|v| v.as_str()) {
                    let trimmed = dom.trim();
                    if !trimmed.is_empty() && trimmed != "localhost" {
                        domain = Some(trimmed.to_string());
                    }
                }
            }

            let desc = if !title.is_empty() {
                if let Some(ref app) = app_name {
                    format!("Saved {} in {}", title, app)
                } else if let Some(ref dom) = domain {
                    format!("Saved {} on {}", title, dom)
                } else {
                    format!("Saved {}", title)
                }
            } else if let Some(ref app) = app_name {
                format!("Saved {} reference", app)
            } else if let Some(ref dom) = domain {
                format!("Saved {} reference", dom)
            } else {
                "Captured contextual reference".to_string()
            };

            let key = format!("item:{}", item.id);
            if seen_keys.insert(key) {
                candidates.push(MeaningfulActivity {
                    activity_type: MeaningfulActivityType::PeripheralCapture,
                    timestamp: item.created_at.clone(),
                    reference: Some(MeaningfulEntityReference {
                        entity_type: "item".to_string(),
                        id: item.id.clone(),
                        title: item.title.clone(),
                    }),
                    provenance: "observed".to_string(),
                    confidence: 0.55,
                    description: desc,
                    explanation: format!("Observed contextual capture in {}.", ws_name),
                });
            }
        }

        // 6. Meaningful Activity Segments
        for seg in segments {
            if seg.context.as_ref().map(|c| c.id.as_str()) == Some(context_id) && !seg.is_ambiguous && seg.confidence >= 0.50 {
                let count = seg.item_count;
                let desc = if count > 1 {
                    format!("Active work session ({} items)", count)
                } else {
                    "Active work session".to_string()
                };

                let key = format!("segment:{}", seg.id);
                if seen_keys.insert(key) {
                    candidates.push(MeaningfulActivity {
                        activity_type: MeaningfulActivityType::ActivitySegment,
                        timestamp: seg.ended_at.clone(),
                        reference: Some(MeaningfulEntityReference {
                            entity_type: "segment".to_string(),
                            id: seg.id.clone(),
                            title: Some(seg.duration_formatted.clone()),
                        }),
                        provenance: "observed".to_string(),
                        confidence: seg.confidence,
                        description: desc,
                        explanation: format!("Continuous activity segment lasting {} in {}.", seg.duration_formatted, ws_name),
                    });
                }
            }
        }

        if candidates.is_empty() {
            return None;
        }

        // Rank candidates: Composite score = type weight (65%) + confidence (20%) + recency (15%)
        let now = Utc::now();
        candidates.sort_by(|a, b| {
            let score_a = Self::compute_composite_score(a, now);
            let score_b = Self::compute_composite_score(b, now);
            score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
        });

        candidates.into_iter().next()
    }

    fn compute_composite_score(activity: &MeaningfulActivity, now: DateTime<Utc>) -> f64 {
        let type_weight = activity.activity_type.priority_weight();
        let conf = activity.confidence.clamp(0.0, 1.0);

        let recency_factor = if let Some(dt) = parse_ts(&activity.timestamp) {
            let elapsed_hours = (now - dt).num_seconds().max(0) as f64 / 3600.0;
            if elapsed_hours <= 1.0 {
                1.0
            } else if elapsed_hours <= 24.0 {
                0.85
            } else if elapsed_hours <= 168.0 {
                0.60
            } else {
                0.35
            }
        } else {
            0.50
        };

        (type_weight * 0.65) + (conf * 0.20) + (recency_factor * 0.15)
    }
}

// =========================================================================
// ResumeReadiness & Evaluator (Day 19 Step 6)
// =========================================================================

/// Internal classification of how ready ContextSplit is to help the user resume a context.
///
/// Invariants:
/// - Do NOT expose a numerical score to the user.
/// - Pure internal readiness classification based on evidence completeness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResumeReadiness {
    /// Full, rich contextual state available (clear task, quality workset, recent activity).
    Ready,
    /// High-confidence context and workset, with minor missing details (e.g. implied task or slightly older activity).
    MostlyReady,
    /// Context identified but task uncertain or activity/workset limited.
    Partial,
    /// No prior activity or negligible evidence.
    Insufficient,
}

impl ResumeReadiness {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::MostlyReady => "mostly_ready",
            Self::Partial => "partial",
            Self::Insufficient => "insufficient",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::MostlyReady => "Mostly Ready",
            Self::Partial => "Partial",
            Self::Insufficient => "Insufficient",
        }
    }

    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    pub fn is_mostly_ready(&self) -> bool {
        matches!(self, Self::MostlyReady)
    }

    pub fn is_partial(&self) -> bool {
        matches!(self, Self::Partial)
    }

    pub fn is_insufficient(&self) -> bool {
        matches!(self, Self::Insufficient)
    }
}

impl std::fmt::Display for ResumeReadiness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Detailed evaluation of resume readiness with human-readable rationale.
///
/// Invariant: Does not expose raw numerical scores to the user.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResumeReadinessAssessment {
    /// Readiness classification
    pub readiness: ResumeReadiness,
    /// Explanatory bullet points (e.g., ["current task identified", "5 relevant workset items", "recent activity available"])
    pub reasons: Vec<String>,
    /// Human-readable explanation block
    pub formatted_explanation: String,
}

/// Evaluator for determining how much information ContextSplit has available to help the user resume.
pub struct ResumeReadinessEvaluator;

impl ResumeReadinessEvaluator {
    pub fn new() -> Self {
        Self
    }

    /// Evaluates resume readiness from derived context state attributes.
    ///
    /// Evaluates:
    /// - context confidence
    /// - task confidence
    /// - workset quality
    /// - recent activity
    /// - last meaningful activity
    /// - relevant item availability
    /// - recent state freshness
    /// - conflicting evidence
    pub fn evaluate(
        state: ContextStateValue,
        left_off_on: Option<&str>,
        workset_count: usize,
        has_meaningful_activity: bool,
        has_recent_activity: bool,
        has_user_feedback: bool,
    ) -> ResumeReadinessAssessment {
        Self::evaluate_with_freshness(
            state,
            left_off_on,
            workset_count,
            has_meaningful_activity,
            has_recent_activity,
            has_user_feedback,
            StateFreshness::Fresh,
        )
    }

    /// Evaluates resume readiness taking state freshness into account.
    pub fn evaluate_with_freshness(
        state: ContextStateValue,
        left_off_on: Option<&str>,
        workset_count: usize,
        has_meaningful_activity: bool,
        has_recent_activity: bool,
        _has_user_feedback: bool,
        freshness: StateFreshness,
    ) -> ResumeReadinessAssessment {
        let is_task_identified = left_off_on.is_some();
        let is_dormant = state == ContextStateValue::Dormant;
        let is_uncertain = state == ContextStateValue::Uncertain;
        let is_insufficient = state == ContextStateValue::InsufficientEvidence;

        let mut reasons: Vec<String> = Vec::new();

        // 1. Insufficient Evidence
        if is_insufficient || (workset_count == 0 && !is_task_identified && !has_recent_activity) {
            reasons.push("no prior activity recorded".to_string());
            reasons.push("insufficient evidence to reconstruct state".to_string());

            let formatted = Self::format_explanation(ResumeReadiness::Insufficient, &reasons);
            return ResumeReadinessAssessment {
                readiness: ResumeReadiness::Insufficient,
                reasons,
                formatted_explanation: formatted,
            };
        }

        // 2. Ready: Current task identified + relevant workset items (>= 2) + recent activity available + not uncertain/dormant + fresh/recent
        if is_task_identified
            && workset_count >= 2
            && (has_recent_activity || state == ContextStateValue::Active || state == ContextStateValue::RecentlyActive)
            && !is_uncertain
            && !is_dormant
            && (freshness == StateFreshness::Fresh || freshness == StateFreshness::Recent)
        {
            reasons.push("current task identified".to_string());
            if workset_count > 0 {
                if workset_count == 1 {
                    reasons.push("1 relevant workset item".to_string());
                } else {
                    reasons.push(format!("{} relevant workset items", workset_count));
                }
            }
            if has_recent_activity {
                reasons.push("recent activity available".to_string());
            }

            let formatted = Self::format_explanation(ResumeReadiness::Ready, &reasons);
            return ResumeReadinessAssessment {
                readiness: ResumeReadiness::Ready,
                reasons,
                formatted_explanation: formatted,
            };
        }

        // 3. Mostly Ready:
        // Case A: Task identified + at least 1 workset item / meaningful activity (and not very stale)
        // Case B: Rich workset (>= 3) + recent activity, but task is general / implied
        // Case C: Task identified + workset >= 2, but state is stale (older than 48h)
        if !is_uncertain && !is_dormant && (
            (is_task_identified && (workset_count >= 1 || has_meaningful_activity) && freshness != StateFreshness::VeryStale)
            || (workset_count >= 3 && has_recent_activity && freshness != StateFreshness::VeryStale)
            || (is_task_identified && workset_count >= 2 && freshness == StateFreshness::Stale)
        ) {
            reasons.push("context identified".to_string());
            if is_task_identified {
                reasons.push("current task identified".to_string());
            } else {
                reasons.push("supporting workset available".to_string());
            }
            if workset_count > 0 {
                if workset_count == 1 {
                    reasons.push("1 relevant workset item".to_string());
                } else {
                    reasons.push(format!("{} relevant workset items", workset_count));
                }
            }
            if has_recent_activity {
                reasons.push("recent activity available".to_string());
            }
            if freshness == StateFreshness::Stale {
                reasons.push("state is stale (last active several days ago)".to_string());
            }

            let formatted = Self::format_explanation(ResumeReadiness::MostlyReady, &reasons);
            return ResumeReadinessAssessment {
                readiness: ResumeReadiness::MostlyReady,
                reasons,
                formatted_explanation: formatted,
            };
        }

        // 4. Partial:
        // Context identified, but task uncertain, limited activity, dormant, stale/very stale, or competing signals
        reasons.push("context identified".to_string());
        if !is_task_identified {
            reasons.push("task uncertain".to_string());
        }
        if is_dormant || !has_recent_activity || workset_count <= 1 {
            reasons.push("limited recent activity".to_string());
        }
        if is_uncertain {
            reasons.push("competing context signals detected".to_string());
        }
        if freshness == StateFreshness::VeryStale {
            reasons.push("state is stale (inactive for over a week)".to_string());
        }

        let formatted = Self::format_explanation(ResumeReadiness::Partial, &reasons);
        ResumeReadinessAssessment {
            readiness: ResumeReadiness::Partial,
            reasons,
            formatted_explanation: formatted,
        }
    }

    fn format_explanation(readiness: ResumeReadiness, reasons: &[String]) -> String {
        let mut lines = Vec::new();
        lines.push("Resume readiness:".to_string());
        lines.push(readiness.display_name().to_string());
        lines.push(String::new());
        lines.push("Because:".to_string());
        for r in reasons {
            lines.push(format!("- {}", r));
        }
        lines.join("\n")
    }
}

// =========================================================================
// ContextResumeState: Human-Readable Contextual Resume Representation (Day 19 Step 5)
// =========================================================================

/// Human-readable resume state representing where the user left off in a context.
///
/// Derived from `ContextMemory`, `ContextWorkset`, `MeaningfulActivity`, and recent activity.
/// Answers: "Where did I leave this work?" in clean, concise, human-ready format.
///
/// Invariants:
/// - Never invent a task if evidence is insufficient or ambiguous.
/// - Never launch applications or files (pure state reconstruction).
/// - Deduplicate and clean recent work references.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextResumeState {
    /// Context / Workspace identifier
    pub context_id: String,
    /// Context / Workspace display name (e.g. "Aurora Website", "University OS")
    pub context_name: String,
    /// Specific task the user left off on (nullable, never invented without evidence)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_off_on: Option<String>,
    /// Clean title/description of the last meaningful activity (nullable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_meaningful_activity: Option<String>,
    /// Classification of the last meaningful activity (nullable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity_type: Option<MeaningfulActivityType>,
    /// Clean list of recent work items/artifacts (e.g. ["Figma hero design", "React Hero component", "Framer Motion reference"])
    #[serde(default)]
    pub recent_work: Vec<String>,
    /// Formatted human-readable resume summary block
    pub formatted_resume_summary: String,
    /// Internal resume readiness assessment (Day 19 Step 6)
    pub readiness: ResumeReadinessAssessment,
    /// Conservative state of the context
    pub state: ContextStateValue,
    /// State freshness classification (Day 19 Step 11)
    #[serde(default = "default_state_freshness")]
    pub freshness: StateFreshness,
    /// Defensible relative display wording (e.g. "Active recently", "Last active yesterday")
    #[serde(default)]
    pub freshness_display: String,
    /// Calibrated state confidence score [0.0, 1.0]
    pub confidence: f64,
    /// ISO-8601 timestamp when this resume state was reconstructed
    pub reconstructed_at: String,
}

// =========================================================================
// StateExplanation & Progressive Disclosure (Day 19 Step 8)
// =========================================================================

/// Single grounded reason explaining why ContextSplit reconstructed this state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StateReason {
    /// Human-readable explanation sentence (e.g., "Hero animation was the most recent active task")
    pub text: String,
    /// Classification distinguishing "Observed" vs "Inferred"
    pub provenance: String,
}

/// Structured explanation for ContextState reasoning with progressive disclosure.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StateExplanation {
    /// Header summary (e.g. "Why ContextSplit thinks this:")
    pub title: String,
    /// List of grounded reasons distinguishing Observed vs Inferred
    pub reasons: Vec<StateReason>,
    /// Optional subtle uncertainty note if evidence is sparse or conflicting
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uncertainty_note: Option<String>,
}

impl Default for StateExplanation {
    fn default() -> Self {
        Self {
            title: "Why ContextSplit thinks this:".to_string(),
            reasons: Vec::new(),
            uncertainty_note: None,
        }
    }
}

// =========================================================================
// ContextState: Derived Contextual Working State Representation
// =========================================================================

/// Derived representation of the user's last known working state inside a context.
///
/// Answering: "Where did I leave this work?"
///
/// Combines `ContextMemory`, `ContextWorkset`, `ContextActivitySegments`, `ContextTransitions`,
/// `TaskCandidates`, `TaskTransitions`, `Items`, and `ContextEvidence` without duplicating data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextState {
    /// Context / Workspace identifier
    pub context_id: String,
    /// Human-friendly context name (e.g. "Aurora Website", "Portfolio")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_name: Option<String>,
    /// Active or last inferred task ID if known (nullable)
    pub task_id: Option<String>,
    /// Active or last inferred task title if known (nullable)
    pub task_title: Option<String>,
    /// Conservative working state classification
    pub state: ContextStateValue,
    /// State freshness classification (Day 19 Step 11)
    #[serde(default = "default_state_freshness")]
    pub freshness: StateFreshness,
    /// Defensible relative display wording (e.g. "Active recently", "Last active yesterday")
    #[serde(default)]
    pub freshness_display: String,
    /// ISO-8601 timestamp of last known activity in this context (nullable)
    pub last_active_at: Option<String>,
    /// ID of the last continuous activity segment (nullable)
    pub last_activity_segment_id: Option<String>,
    /// ID of the last recorded context transition (nullable)
    pub last_transition_id: Option<String>,
    /// ID of the last meaningful item captured or interacted with (nullable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_meaningful_item_id: Option<String>,
    /// Title or label of the last meaningful item (nullable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_meaningful_item_title: Option<String>,
    /// Derived contextual workset (nullable)
    pub current_workset: Option<ContextWorkset>,
    /// Relevant recent item IDs associated with this state
    pub recent_item_ids: Vec<String>,
    /// Concise human-readable summary of recent activity (nullable)
    pub recent_activity: Option<String>,
    /// Foreground application associated with recent work if known (nullable)
    pub active_application: Option<String>,
    /// Web domain associated with recent work if known (nullable)
    pub active_domain: Option<String>,
    /// Applications observed across recent context activity
    #[serde(default)]
    pub recent_applications: Vec<String>,
    /// Human-readable description of the latest meaningful action (nullable)
    pub latest_meaningful_action: Option<String>,
    /// Deterministic natural reconstruction answering "Where did I leave this work?"
    #[serde(default)]
    pub last_meaningful_activity: String,
    /// Structured details of the last meaningful activity (nullable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meaningful_activity: Option<MeaningfulActivity>,
    /// Derived human-readable resume state (Day 19 Step 5)
    pub resume_state: ContextResumeState,
    /// Internal resume readiness assessment (Day 19 Step 6)
    pub readiness: ResumeReadinessAssessment,
    /// Progressive disclosure state explanation (Day 19 Step 8)
    #[serde(default)]
    pub explanation: StateExplanation,
    /// Calibrated state confidence score [0.0, 1.0]
    pub state_confidence: f64,
    /// Epistemic source ("confirmed", "inferred", "observed")
    #[serde(default = "default_inferred_provenance")]
    pub provenance: String,
    /// Whether user feedback explicitly grounded this state
    #[serde(default)]
    pub has_user_feedback: bool,
    /// ISO-8601 timestamp when this state was reconstructed
    pub reconstructed_at: String,
}

fn default_inferred_provenance() -> String {
    "inferred".to_string()
}

impl ContextState {
    pub fn is_working(&self) -> bool {
        matches!(self.state, ContextStateValue::Active | ContextStateValue::RecentlyActive)
    }

    pub fn last_meaningful_workset(&self) -> Option<&ContextWorkset> {
        self.current_workset.as_ref()
    }

    pub fn get_resume_state(&self) -> &ContextResumeState {
        &self.resume_state
    }
}

// =========================================================================
// Helpers for Parsing & Timestamp Math
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

// =========================================================================
// ContextStateBuilder: Fluid Derivation Pipeline
// =========================================================================

/// Builder and derivation engine for `ContextState`.
#[derive(Debug, Clone)]
pub struct ContextStateBuilder {
    pub context_id: String,
    pub context_name: Option<String>,
    pub memory: Option<ContextMemory>,
    pub workset: Option<ContextWorkset>,
    pub activity_segments: Vec<ContextActivitySegment>,
    pub context_transitions: Vec<ContextTransition>,
    pub task_candidates: Vec<TaskCandidate>,
    pub task_transitions: Vec<DetectedTaskTransition>,
    pub items: Vec<Item>,
    pub evidence: Vec<ContextEvidenceItem>,
    pub has_user_feedback: bool,
    pub reference_time: Option<DateTime<Utc>>,
}

impl ContextStateBuilder {
    pub fn new(context_id: impl Into<String>) -> Self {
        Self {
            context_id: context_id.into(),
            context_name: None,
            memory: None,
            workset: None,
            activity_segments: Vec::new(),
            context_transitions: Vec::new(),
            task_candidates: Vec::new(),
            task_transitions: Vec::new(),
            items: Vec::new(),
            evidence: Vec::new(),
            has_user_feedback: false,
            reference_time: None,
        }
    }

    pub fn with_context_name(mut self, name: impl Into<String>) -> Self {
        self.context_name = Some(name.into());
        self
    }

    pub fn with_memory(mut self, memory: Option<ContextMemory>) -> Self {
        if let Some(ref m) = memory {
            if self.context_name.is_none() && !m.context_name.is_empty() {
                self.context_name = Some(m.context_name.clone());
            }
        }
        self.memory = memory;
        self
    }

    pub fn with_workset(mut self, workset: Option<ContextWorkset>) -> Self {
        if let Some(ref w) = workset {
            if self.context_name.is_none() && !w.context_name.is_empty() {
                self.context_name = Some(w.context_name.clone());
            }
        }
        self.workset = workset;
        self
    }

    pub fn with_activity_segments(mut self, segments: Vec<ContextActivitySegment>) -> Self {
        self.activity_segments = segments;
        self
    }

    pub fn with_context_transitions(mut self, transitions: Vec<ContextTransition>) -> Self {
        self.context_transitions = transitions;
        self
    }

    pub fn with_task_candidates(mut self, candidates: Vec<TaskCandidate>) -> Self {
        self.task_candidates = candidates;
        self
    }

    pub fn with_task_transitions(mut self, transitions: Vec<DetectedTaskTransition>) -> Self {
        self.task_transitions = transitions;
        self
    }

    pub fn with_items(mut self, items: Vec<Item>) -> Self {
        self.items = items;
        self
    }

    pub fn with_evidence(mut self, evidence: Vec<ContextEvidenceItem>) -> Self {
        self.evidence = evidence;
        self
    }

    pub fn with_user_feedback(mut self, has_feedback: bool) -> Self {
        self.has_user_feedback = has_feedback;
        self
    }

    pub fn with_reference_time(mut self, time: DateTime<Utc>) -> Self {
        self.reference_time = Some(time);
        self
    }

    /// Loads underlying sources from the database and prepares the builder.
    pub fn from_db(conn: &Connection, context_id: &str) -> Result<Self, String> {
        let ws = crate::db::operations::get_workspace(conn, context_id).ok();
        let context_name = ws.map(|w| w.name);

        let workset = ContextWorksetBuilder::from_db(conn, context_id)
            .and_then(|b| b.build())
            .ok();

        let memory = ContextMemoryBuilder::from_db(conn, context_id)
            .and_then(|b| b.build())
            .ok();

        let activity_segments = Vec::new();

        let mut trans_stmt = conn.prepare(
            "SELECT id, from_context_id, to_context_id, transition_type, confidence, detected_at, session_id, trigger_item_id, trigger_signal_id, metadata, created_at 
             FROM context_transitions 
             WHERE from_context_id = ?1 OR to_context_id = ?1 
             ORDER BY detected_at DESC LIMIT 20",
        ).map_err(|e| e.to_string())?;

        let context_transitions: Vec<ContextTransition> = trans_stmt
            .query_map(rusqlite::params![context_id], |row| {
                let trans_type_str: String = row.get(3)?;
                let transition_type = ContextTransitionType::from_str_lenient(&trans_type_str)
                    .unwrap_or(ContextTransitionType::ContextSwitch);
                Ok(ContextTransition {
                    id: row.get(0)?,
                    from_context_id: row.get(1)?,
                    to_context_id: row.get(2)?,
                    transition_type,
                    confidence: row.get(4)?,
                    detected_at: row.get(5)?,
                    session_id: row.get(6)?,
                    trigger_item_id: row.get(7)?,
                    trigger_signal_id: row.get(8)?,
                    metadata: row.get(9)?,
                    created_at: row.get(10)?,
                })
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

        let mut task_stmt = conn.prepare(
            "SELECT id, context_id, title, confidence, status, first_detected_at, last_active_at, created_at, updated_at 
             FROM task_candidates 
             WHERE context_id = ?1 
             ORDER BY last_active_at DESC LIMIT 10",
        ).map_err(|e| e.to_string())?;

        let task_candidates: Vec<TaskCandidate> = task_stmt
            .query_map(rusqlite::params![context_id], |row| {
                let status_str: String = row.get(4)?;
                let status = match status_str.as_str() {
                    "detected" => TaskCandidateStatus::Detected,
                    "active" => TaskCandidateStatus::Active,
                    "inactive" => TaskCandidateStatus::Inactive,
                    "uncertain" => TaskCandidateStatus::Uncertain,
                    "dismissed" => TaskCandidateStatus::Dismissed,
                    _ => TaskCandidateStatus::Detected,
                };
                Ok(TaskCandidate {
                    id: row.get(0)?,
                    context_id: row.get(1)?,
                    title: row.get(2)?,
                    confidence: row.get(3)?,
                    status,
                    first_detected_at: row.get(5)?,
                    last_active_at: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

        let items = crate::db::operations::list_items(conn, Some(context_id))
            .unwrap_or_default();

        let mut has_user_feedback = false;
        if let Ok(mut mem_stmt) = conn.prepare("SELECT source FROM context_memberships WHERE workspace_id = ?1") {
            if let Ok(rows) = mem_stmt.query_map(rusqlite::params![context_id], |row| row.get::<_, String>(0)) {
                for s in rows.flatten() {
                    if s == "manual" || s == "user_confirmed" {
                        has_user_feedback = true;
                        break;
                    }
                }
            }
        }

        Ok(Self {
            context_id: context_id.to_string(),
            context_name,
            memory,
            workset,
            activity_segments,
            context_transitions,
            task_candidates,
            task_transitions: Vec::new(),
            items,
            evidence: Vec::new(),
            has_user_feedback,
            reference_time: None,
        })
    }

    /// Derives the conservative `ContextState` representation answering "Where did I leave this work?".
    pub fn build(self) -> Result<ContextState, String> {
        let now = self.reference_time.unwrap_or_else(Utc::now);
        let reconstructed_at = now.to_rfc3339();

        // Context name resolution
        let context_name = self.context_name.or_else(|| {
            self.workset.as_ref().map(|w| w.context_name.clone()).or_else(|| {
                self.memory.as_ref().map(|m| m.context_name.clone())
            })
        });

        // 1. Resolve Last Known Activity Timestamp
        let mut latest_ts: Option<DateTime<Utc>> = None;
        let mut latest_ts_str: Option<String> = None;

        let mut consider_ts = |ts_str: &str| {
            if let Some(parsed) = parse_ts(ts_str) {
                if latest_ts.is_none() || Some(parsed) > latest_ts {
                    latest_ts = Some(parsed);
                    latest_ts_str = Some(ts_str.to_string());
                }
            }
        };

        // From context activity segments
        for seg in &self.activity_segments {
            if seg.context.as_ref().map(|c| &c.id) == Some(&self.context_id) {
                consider_ts(&seg.ended_at);
                consider_ts(&seg.started_at);
            }
        }

        // From transitions
        for trans in &self.context_transitions {
            if trans.from_context_id.as_deref() == Some(&self.context_id)
                || trans.to_context_id.as_deref() == Some(&self.context_id)
            {
                consider_ts(&trans.detected_at);
            }
        }

        // From task candidates
        for task in &self.task_candidates {
            if task.context_id.as_deref() == Some(&self.context_id) {
                consider_ts(&task.last_active_at);
            }
        }

        // From memory
        if let Some(mem) = &self.memory {
            if let Some(la) = &mem.last_active_at {
                consider_ts(la);
            }
        }

        // From workset
        if let Some(ws) = &self.workset {
            if let Some(la) = &ws.last_active_at {
                consider_ts(la);
            }
        }

        // From items
        for item in &self.items {
            consider_ts(&item.created_at);
            consider_ts(&item.updated_at);
        }

        let last_active_at = latest_ts_str.clone();

        // 2. Identify Latest Activity Segment and Transition IDs
        let mut context_segments: Vec<&ContextActivitySegment> = self
            .activity_segments
            .iter()
            .filter(|s| s.context.as_ref().map(|c| &c.id) == Some(&self.context_id))
            .collect();
        context_segments.sort_by(|a, b| {
            parse_ts(&b.ended_at)
                .unwrap_or(Utc::now())
                .cmp(&parse_ts(&a.ended_at).unwrap_or(Utc::now()))
        });

        let latest_segment = context_segments.first().copied();
        let last_activity_segment_id = latest_segment.map(|s| s.id.clone());

        let mut sorted_transitions = self.context_transitions.clone();
        sorted_transitions.sort_by(|a, b| {
            parse_ts(&b.detected_at)
                .unwrap_or(Utc::now())
                .cmp(&parse_ts(&a.detected_at).unwrap_or(Utc::now()))
        });

        let latest_transition = sorted_transitions.first();
        let last_transition_id = latest_transition.map(|t| t.id.clone());

        // 3. Resolve Last Meaningful Task Candidate (Nullable, zero forced guessing)
        let mut task_id: Option<String> = None;
        let mut task_title: Option<String> = None;

        if let Some(ws) = &self.workset {
            if let (Some(id), Some(title)) = (&ws.task_id, &ws.task_title) {
                if !title.is_empty() && !title.eq_ignore_ascii_case("unknown") && ws.confidence >= 0.40 {
                    task_id = Some(id.clone());
                    task_title = Some(title.clone());
                }
            }
        }

        if task_title.is_none() {
            if let Some(mem) = &self.memory {
                if let (Some(title), conf) = (&mem.current_task_title, mem.current_task_confidence) {
                    if !title.is_empty() && !title.eq_ignore_ascii_case("unknown") && conf.unwrap_or(0.0) >= 0.40 {
                        task_id = mem.current_task_id.clone();
                        task_title = Some(title.clone());
                    }
                }
            }
        }

        if task_title.is_none() {
            let mut valid_candidates: Vec<&TaskCandidate> = self
                .task_candidates
                .iter()
                .filter(|c| {
                    c.context_id.as_deref() == Some(&self.context_id)
                        && c.confidence >= 0.45
                        && !matches!(c.status, TaskCandidateStatus::Dismissed | TaskCandidateStatus::Inactive)
                        && !c.title.eq_ignore_ascii_case("unknown")
                })
                .collect();
            valid_candidates.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
            if let Some(best) = valid_candidates.first() {
                task_id = Some(best.id.clone());
                task_title = Some(best.title.clone());
            }
        }

        // 4. Resolve Recent Item IDs and Last Meaningful Item
        let mut recent_item_ids = Vec::new();
        let mut seen_items = HashSet::new();
        let mut last_meaningful_item_id: Option<String> = None;
        let mut last_meaningful_item_title: Option<String> = None;

        if let Some(ws) = &self.workset {
            for item in &ws.items {
                if seen_items.insert(item.id.clone()) {
                    if last_meaningful_item_id.is_none() {
                        last_meaningful_item_id = Some(item.id.clone());
                        last_meaningful_item_title = item.title.clone();
                    }
                    recent_item_ids.push(item.id.clone());
                    if recent_item_ids.len() >= 10 {
                        break;
                    }
                }
            }
        }

        if recent_item_ids.len() < 10 {
            if let Some(seg) = latest_segment {
                for item_ref in &seg.item_references {
                    if seen_items.insert(item_ref.item_id.clone()) {
                        if last_meaningful_item_id.is_none() {
                            last_meaningful_item_id = Some(item_ref.item_id.clone());
                            last_meaningful_item_title = item_ref.title.clone();
                        }
                        recent_item_ids.push(item_ref.item_id.clone());
                        if recent_item_ids.len() >= 10 {
                            break;
                        }
                    }
                }
            }
        }

        if recent_item_ids.len() < 10 {
            for item in &self.items {
                if seen_items.insert(item.id.clone()) {
                    if last_meaningful_item_id.is_none() {
                        last_meaningful_item_id = Some(item.id.clone());
                        last_meaningful_item_title = item.title.clone();
                    }
                    recent_item_ids.push(item.id.clone());
                    if recent_item_ids.len() >= 10 {
                        break;
                    }
                }
            }
        }

        // 5. Resolve Active Applications, Domains & Multi-App Tracking
        let mut app_counts: HashMap<String, usize> = HashMap::new();
        let mut active_domain: Option<String> = None;

        if let Some(ws) = &self.workset {
            for item in &ws.items {
                if let Some(ref app) = item.app_name {
                    if !app.trim().is_empty() && !app.eq_ignore_ascii_case("unknown") {
                        *app_counts.entry(app.trim().to_string()).or_insert(0) += 1;
                    }
                }
                if active_domain.is_none() && item.domain.is_some() {
                    active_domain = item.domain.clone();
                }
            }
        }

        for item in &self.items {
            if let Ok(metadata) = serde_json::from_str::<Value>(&item.metadata) {
                if let Some(app) = metadata.get("app_name").and_then(|v| v.as_str()) {
                    let trimmed = app.trim();
                    if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("unknown") {
                        *app_counts.entry(trimmed.to_string()).or_insert(0) += 1;
                    }
                }
                if active_domain.is_none() {
                    if let Some(dom) = metadata.get("domain").and_then(|v| v.as_str()) {
                        let trimmed = dom.trim();
                        if !trimmed.is_empty() && trimmed != "localhost" {
                            active_domain = Some(trimmed.to_string());
                        }
                    }
                }
            }
        }

        let mut app_list: Vec<(String, usize)> = app_counts.into_iter().collect();
        app_list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        let active_application = app_list.first().map(|(app, _)| app.clone());
        let recent_applications: Vec<String> = app_list.into_iter().map(|(app, _)| app).collect();

        // 6. Resolve Recent Activity Summary
        let recent_activity: Option<String> = if let Some(ws) = &self.workset {
            if !ws.recent_activity.is_empty() {
                Some(ws.recent_activity.clone())
            } else {
                None
            }
        } else if let Some(mem) = &self.memory {
            if !mem.recent_activity.is_empty() {
                Some(mem.recent_activity.clone())
            } else {
                None
            }
        } else if !recent_item_ids.is_empty() {
            Some(format!("{} items recorded in this context", recent_item_ids.len()))
        } else {
            None
        };

        // 7. Resolve Latest Meaningful Action (Factual, grounded, zero fabricated verbs)
        let mut latest_meaningful_action: Option<String> = None;

        // Check task transitions
        if let Some(last_task_trans) = self.task_transitions.last() {
            match last_task_trans.transition_type {
                TaskTransitionType::TaskStart => {
                    let title = last_task_trans.to_task.as_deref().unwrap_or("new task");
                    latest_meaningful_action = Some(format!("Started task: {}", title));
                }
                TaskTransitionType::TaskSwitch => {
                    let from = last_task_trans.from_task.as_deref().unwrap_or("previous");
                    let to = last_task_trans.to_task.as_deref().unwrap_or("new task");
                    latest_meaningful_action = Some(format!("Switched task from {} to {}", from, to));
                }
                TaskTransitionType::TaskReturn => {
                    let title = last_task_trans.to_task.as_deref().unwrap_or("task");
                    latest_meaningful_action = Some(format!("Resumed task: {}", title));
                }
                TaskTransitionType::TaskEnd => {
                    let title = last_task_trans.from_task.as_deref().unwrap_or("active task");
                    latest_meaningful_action = Some(format!("Completed or paused task: {}", title));
                }
                TaskTransitionType::AmbiguousTaskChange => {}
            }
        }

        // Check context transitions
        if latest_meaningful_action.is_none() {
            if let Some(trans) = latest_transition {
                if trans.to_context_id.as_deref() == Some(&self.context_id) {
                    if trans.transition_type == ContextTransitionType::ContextReturn {
                        latest_meaningful_action = Some("Returned to context after interruption".to_string());
                    } else if trans.transition_type == ContextTransitionType::ContextStart {
                        latest_meaningful_action = Some("Initiated working context session".to_string());
                    } else {
                        latest_meaningful_action = Some("Switched focus to this context".to_string());
                    }
                } else if trans.from_context_id.as_deref() == Some(&self.context_id) {
                    latest_meaningful_action = Some("Switched away to another context".to_string());
                }
            }
        }

        // Fallback to active task or segment action
        if latest_meaningful_action.is_none() {
            if let Some(title) = &task_title {
                latest_meaningful_action = Some(format!("Working on {}", title));
            } else if let Some(title) = &last_meaningful_item_title {
                latest_meaningful_action = Some(format!("Interacting with {}", title));
            }
        }

        // 8. Conservative State Classification & Confidence Scoring
        let has_no_data = self.items.is_empty()
            && self.activity_segments.is_empty()
            && self.context_transitions.is_empty()
            && self.memory.is_none()
            && self.workset.is_none()
            && self.evidence.is_empty();

        let elapsed_seconds = latest_ts.map(|t| (now - t).num_seconds().max(0));
        let freshness = match elapsed_seconds {
            Some(secs) => StateFreshness::classify(secs),
            None => StateFreshness::VeryStale,
        };
        let freshness_display = match elapsed_seconds {
            Some(secs) => freshness.display_wording(secs),
            None => "No prior activity".to_string(),
        };

        let (state, mut state_confidence) = if has_no_data {
            (ContextStateValue::InsufficientEvidence, 0.0)
        } else if self.items.is_empty() && latest_segment.is_none() && self.evidence.is_empty() {
            (ContextStateValue::InsufficientEvidence, 0.0)
        } else if let Some(mem) = &self.memory {
            if mem.activity_state == ContextMemoryActivityState::InsufficientEvidence
                && self.items.is_empty()
                && context_segments.is_empty()
            {
                (ContextStateValue::InsufficientEvidence, 0.10)
            } else {
                Self::evaluate_state(
                    &self.context_id,
                    elapsed_seconds,
                    latest_segment,
                    latest_transition,
                    &self.task_transitions,
                    &self.workset,
                    &self.memory,
                    &self.items,
                )
            }
        } else {
            Self::evaluate_state(
                &self.context_id,
                elapsed_seconds,
                latest_segment,
                latest_transition,
                &self.task_transitions,
                &self.workset,
                &self.memory,
                &self.items,
            )
        };

        // User feedback increases confidence and solidifies state
        let provenance = if self.has_user_feedback {
            state_confidence = (state_confidence * 0.30 + 0.70).clamp(0.75, 1.0);
            "confirmed".to_string()
        } else if self.items.is_empty() {
            "observed".to_string()
        } else {
            "inferred".to_string()
        };

        // 9. Resolve Meaningful Activity using MeaningfulActivityResolver (Day 19 Step 4)
        let meaningful_activity = MeaningfulActivityResolver::resolve_last_meaningful_activity(
            &self.context_id,
            context_name.as_deref(),
            &self.items,
            self.workset.as_ref(),
            self.memory.as_ref(),
            &self.activity_segments,
            &self.context_transitions,
            &self.task_candidates,
            &self.task_transitions,
            self.has_user_feedback,
        );

        // 10. Reconstruct "Where did I leave this work?" (Deterministic natural phrasing)
        let last_meaningful_activity = Self::construct_meaningful_activity_statement(
            context_name.as_deref().unwrap_or("this workspace"),
            state,
            task_title.as_deref(),
            &recent_applications,
            active_domain.as_deref(),
            last_meaningful_item_title.as_deref(),
            latest_transition,
            self.task_transitions.last(),
            &self.items,
        );

        // 11. Derive Human-Readable Resume State (Day 19 Step 5)
        let resume_state = Self::construct_resume_state(
            &self.context_id,
            context_name.as_deref().unwrap_or("Workspace"),
            state,
            freshness,
            &freshness_display,
            task_title.as_deref(),
            &meaningful_activity,
            last_meaningful_item_title.as_deref(),
            self.workset.as_ref(),
            self.memory.as_ref(),
            &self.items,
            state_confidence,
            self.has_user_feedback,
            &reconstructed_at,
        );

        let readiness = resume_state.readiness.clone();

        // 12. Derive Progressive Disclosure State Explanation (Day 19 Step 8)
        let is_task_confirmed = self.has_user_feedback
            || self.workset.as_ref().map(|w| matches!(w.provenance, crate::context_engine::memory::InformationProvenance::Confirmed)).unwrap_or(false);
        let explanation = Self::construct_state_explanation(
            context_name.as_deref().unwrap_or("Workspace"),
            state,
            task_title.as_deref(),
            is_task_confirmed,
            self.workset.as_ref(),
            meaningful_activity.as_ref(),
            active_application.as_deref(),
            latest_transition,
            &self.items,
        );

        Ok(ContextState {
            context_id: self.context_id,
            context_name,
            task_id,
            task_title,
            state,
            freshness,
            freshness_display,
            last_active_at,
            last_activity_segment_id,
            last_transition_id,
            last_meaningful_item_id,
            last_meaningful_item_title,
            current_workset: self.workset,
            recent_item_ids,
            recent_activity,
            active_application,
            active_domain,
            recent_applications,
            latest_meaningful_action,
            last_meaningful_activity,
            meaningful_activity,
            resume_state,
            readiness,
            explanation,
            state_confidence,
            provenance,
            has_user_feedback: self.has_user_feedback,
            reconstructed_at,
        })
    }

    /// Constructs the progressive disclosure explanation for why ContextSplit reconstructed this state.
    ///
    /// Distinguishes:
    /// - "Observed" (factual event, confirmed task, observed app context, verified return)
    /// - "Inferred" (statistical candidate, predicted activity)
    ///
    /// Example:
    /// Why ContextSplit thinks this:
    /// • Hero animation was the most recent active task (Inferred)
    /// • 3 related items were recently active (Observed)
    /// • Aurora Website remained the active context (Observed)
    /// • Figma was the latest recorded application context (Observed)
    #[allow(clippy::too_many_arguments)]
    fn construct_state_explanation(
        context_name: &str,
        state: ContextStateValue,
        task_title: Option<&str>,
        task_confirmed: bool,
        workset: Option<&ContextWorkset>,
        meaningful_activity: Option<&MeaningfulActivity>,
        active_app: Option<&str>,
        latest_transition: Option<&ContextTransition>,
        items: &[Item],
    ) -> StateExplanation {
        let mut reasons = Vec::new();

        // 1. Task reasoning
        if let Some(task) = task_title {
            if !task.trim().is_empty() && !task.eq_ignore_ascii_case("not clear yet") {
                let prov = if task_confirmed { "Observed" } else { "Inferred" };
                reasons.push(StateReason {
                    text: format!("{} was the most recent active task", task),
                    provenance: prov.to_string(),
                });
            }
        }

        // 2. Related items count
        let item_count = workset.map(|w| w.items.len()).unwrap_or(items.len());
        if item_count > 0 {
            reasons.push(StateReason {
                text: format!(
                    "{} related item{} recently active",
                    item_count,
                    if item_count == 1 { " was" } else { "s were" }
                ),
                provenance: "Observed".to_string(),
            });
        }

        // 3. Context presence
        if let Some(trans) = latest_transition {
            if trans.transition_type == ContextTransitionType::ContextReturn {
                reasons.push(StateReason {
                    text: format!("Returned to '{}' context after temporary detour", context_name),
                    provenance: "Observed".to_string(),
                });
            } else {
                reasons.push(StateReason {
                    text: format!("{} remained the active context", context_name),
                    provenance: "Observed".to_string(),
                });
            }
        } else {
            reasons.push(StateReason {
                text: format!("{} remained the active context", context_name),
                provenance: "Observed".to_string(),
            });
        }

        // 4. Recorded application
        if let Some(app) = active_app {
            if !app.trim().is_empty() && !app.eq_ignore_ascii_case("unknown") {
                reasons.push(StateReason {
                    text: format!("{} was the latest recorded application context", app),
                    provenance: "Observed".to_string(),
                });
            }
        }

        // 5. Meaningful activity if distinct
        if let Some(act) = meaningful_activity {
            if !act.description.trim().is_empty()
                && !reasons.iter().any(|r| r.text.contains(&act.description))
            {
                let prov = if act.provenance == "observed" || act.provenance == "confirmed" {
                    "Observed"
                } else {
                    "Inferred"
                };
                reasons.push(StateReason {
                    text: format!("Last activity: {}", act.description),
                    provenance: prov.to_string(),
                });
            }
        }

        // 6. Uncertainty Note if sparse or uncertain
        let uncertainty_note = if state == ContextStateValue::Uncertain || state == ContextStateValue::InsufficientEvidence {
            Some("Context signals are emerging or sparse. State will update as more activity occurs.".to_string())
        } else {
            None
        };

        StateExplanation {
            title: "Why ContextSplit thinks this:".to_string(),
            reasons,
            uncertainty_note,
        }
    }

    /// Constructs the deterministic natural working state answer to: "Where did I leave this work?"
    ///
    /// Rules:
    /// - Do NOT infer more than evidence supports.
    /// - Do NOT fabricate actions (e.g. "editing") unless an explicit signal exists.
    /// - If task + app: "Last known activity: Hero design in Figma."
    /// - If only URL captured: "Last known activity: Figma page related to Aurora Website."
    /// - If multi-app: "Last known activity: Hero design across Safari and VSCode."
    #[allow(clippy::too_many_arguments)]
    fn construct_meaningful_activity_statement(
        context_name: &str,
        state: ContextStateValue,
        task_title: Option<&str>,
        recent_apps: &[String],
        active_domain: Option<&str>,
        last_item_title: Option<&str>,
        latest_transition: Option<&ContextTransition>,
        last_task_transition: Option<&DetectedTaskTransition>,
        items: &[Item],
    ) -> String {
        // 1. Insufficient Evidence
        if state == ContextStateValue::InsufficientEvidence {
            return format!("No prior activity recorded for {}.", context_name);
        }

        // 2. Uncertain State
        if state == ContextStateValue::Uncertain {
            return format!(
                "Uncertain recent activity in {} with competing context signals.",
                context_name
            );
        }

        // 3. Context Return
        if let Some(trans) = latest_transition {
            if trans.transition_type == ContextTransitionType::ContextReturn {
                if let Some(task) = task_title {
                    return format!("Returned to {} working on {}.", context_name, task);
                } else {
                    return format!("Returned to {} context after interruption.", context_name);
                }
            }
        }

        // 4. Task Transition / Task Switch
        if let Some(tt) = last_task_transition {
            if tt.transition_type == TaskTransitionType::TaskSwitch {
                let from = tt.from_task.as_deref().unwrap_or("previous task");
                let to = tt.to_task.as_deref().unwrap_or("new task");
                return format!("Switched task from {} to {}.", from, to);
            } else if tt.transition_type == TaskTransitionType::TaskStart {
                let to = tt.to_task.as_deref().unwrap_or("new task");
                return format!("Started task: {}.", to);
            }
        }

        // 5. Context Switch Away / Paused
        if state == ContextStateValue::Paused {
            if let Some(trans) = latest_transition {
                if trans.transition_type == ContextTransitionType::ContextSwitch {
                    if let Some(task) = task_title {
                        if let Some(app) = recent_apps.first() {
                            return format!("Paused work on {} in {}.", task, app);
                        } else {
                            return format!("Paused work on {}.", task);
                        }
                    } else {
                        return format!("Paused work in {}.", context_name);
                    }
                }
            }
        }

        // 6. Dormant Context
        if state == ContextStateValue::Dormant {
            if let Some(task) = task_title {
                return format!(
                    "Dormant context. Last known activity: {} (no recent activity for over a week).",
                    task
                );
            } else {
                return format!("Dormant context with no recent activity.");
            }
        }

        // 7. Confident Task + App(s)
        if let Some(task) = task_title {
            if recent_apps.len() > 1 {
                return format!(
                    "Last known activity: {} across {} and {}.",
                    task, recent_apps[0], recent_apps[1]
                );
            } else if let Some(app) = recent_apps.first() {
                return format!("Last known activity: {} in {}.", task, app);
            } else {
                return format!("Last known activity: {}.", task);
            }
        }

        // 8. Weak Evidence / Single Capture without clear task
        // "If only a URL was captured: 'Last known activity: Figma page related to Aurora Website.'"
        // "Do NOT fabricate actions."
        if let Some(item_title) = last_item_title {
            if !item_title.trim().is_empty() && !item_title.eq_ignore_ascii_case("untitled") {
                if let Some(app) = recent_apps.first() {
                    return format!("Last known activity: {} in {}.", item_title, app);
                } else if let Some(dom) = active_domain {
                    return format!("Last known activity: {} on {}.", item_title, dom);
                } else {
                    return format!("Last known activity: {} related to {}.", item_title, context_name);
                }
            }
        }

        if let Some(app) = recent_apps.first() {
            return format!("Last known activity: {} page related to {}.", app, context_name);
        }

        if let Some(dom) = active_domain {
            return format!("Last known activity: {} page related to {}.", dom, context_name);
        }

        if !items.is_empty() {
            return format!("Last known activity: viewing items in {}.", context_name);
        }

        format!("Last known activity in {}.", context_name)
    }

    #[allow(clippy::too_many_arguments)]
    fn evaluate_state(
        context_id: &str,
        elapsed_seconds: Option<i64>,
        latest_segment: Option<&ContextActivitySegment>,
        latest_transition: Option<&ContextTransition>,
        task_transitions: &[DetectedTaskTransition],
        workset: &Option<ContextWorkset>,
        memory: &Option<ContextMemory>,
        items: &[Item],
    ) -> (ContextStateValue, f64) {
        let elapsed = elapsed_seconds.unwrap_or(i64::MAX);

        // Check for severe ambiguity or contradictory transitions
        if let Some(t) = latest_transition {
            if t.transition_type == ContextTransitionType::AmbiguousTransition && t.confidence < 0.40 {
                return (ContextStateValue::Uncertain, 0.30);
            }
        }
        if let Some(tt) = task_transitions.last() {
            if tt.transition_type == TaskTransitionType::AmbiguousTaskChange && tt.confidence < 0.40 {
                return (ContextStateValue::Uncertain, 0.35);
            }
        }

        // Check for Dormancy (> 7 days or 604,800 seconds)
        if elapsed >= 7 * 86400 {
            let base_conf = if !items.is_empty() { 0.75 } else { 0.50 };
            return (ContextStateValue::Dormant, base_conf);
        }

        // Check if context memory indicates dormant state
        if let Some(mem) = memory {
            if mem.activity_state == ContextMemoryActivityState::Dormant && elapsed >= 3 * 86400 {
                return (ContextStateValue::Dormant, 0.70);
            }
        }

        // Check if user transitioned AWAY from this context (Paused)
        if let Some(trans) = latest_transition {
            if trans.from_context_id.as_deref() == Some(context_id)
                && trans.to_context_id.as_deref() != Some(context_id)
                && trans.transition_type == ContextTransitionType::ContextSwitch
            {
                return (ContextStateValue::Paused, trans.confidence.clamp(0.60, 0.95));
            }
        }

        // Check if task ended recently
        if let Some(tt) = task_transitions.last() {
            if tt.transition_type == TaskTransitionType::TaskEnd {
                return (ContextStateValue::Paused, 0.65);
            }
        }

        // Check for Active state
        // Invariant: Do NOT claim active work merely because an item exists or an app is present.
        // Requires recent activity (<= 15 minutes / 900s), active activity segment or corroborated interaction,
        // and sufficient confidence.
        if elapsed <= 900 {
            let has_active_segment = latest_segment
                .map(|s| !s.is_ambiguous && s.confidence >= 0.45)
                .unwrap_or(false);

            let has_active_workset = workset
                .as_ref()
                .map(|w| matches!(w.workset_state, WorksetState::Active | WorksetState::DetourReturn))
                .unwrap_or(false);

            let has_active_memory = memory
                .as_ref()
                .map(|m| m.activity_state == ContextMemoryActivityState::Active)
                .unwrap_or(false);

            let has_context_return = latest_transition
                .map(|t| {
                    t.to_context_id.as_deref() == Some(context_id)
                        && t.transition_type == ContextTransitionType::ContextReturn
                })
                .unwrap_or(false);

            // Conservative: Require at least one active behavioral corroboration
            if has_active_segment || has_active_workset || has_active_memory || has_context_return {
                let conf = if (has_active_segment || has_context_return) && (has_active_workset || has_active_memory) {
                    0.90
                } else {
                    0.80
                };
                return (ContextStateValue::Active, conf);
            }

            // If elapsed <= 900 but no corroborated segment, do not overclaim Active.
            // Fall back to RecentlyActive or Uncertain if items are single/weak.
            if items.len() <= 1 {
                return (ContextStateValue::RecentlyActive, 0.50);
            }
            return (ContextStateValue::RecentlyActive, 0.65);
        }

        // Check for Paused (30 min - 7 days after explicit switch or session close)
        if elapsed > 1800 && elapsed < 86400 {
            if let Some(ws) = workset {
                if ws.workset_state == WorksetState::DetourReturn {
                    return (ContextStateValue::RecentlyActive, 0.70);
                }
            }
            return (ContextStateValue::RecentlyActive, 0.65);
        }

        if elapsed >= 86400 {
            // 24h - 7 days: Paused / warm historical state
            return (ContextStateValue::Paused, 0.60);
        }

        // 15 - 30 minutes: Recently active
        (ContextStateValue::RecentlyActive, 0.70)
    }

    /// Derives the human-readable resume state representing where the user left off.
    ///
    /// Rules:
    /// - Strictly ground tasks in evidence. Never invent a task.
    /// - Extract clean meaningful activity descriptions.
    /// - Aggregate deduplicated recent work items from workset, memory, and items.
    /// - Format multi-line summary block faithfully matching user-facing specifications.
    #[allow(clippy::too_many_arguments)]
    fn construct_resume_state(
        context_id: &str,
        context_name: &str,
        state: ContextStateValue,
        freshness: StateFreshness,
        freshness_display: &str,
        task_title: Option<&str>,
        meaningful_activity: &Option<MeaningfulActivity>,
        last_meaningful_item_title: Option<&str>,
        workset: Option<&ContextWorkset>,
        memory: Option<&ContextMemory>,
        items: &[Item],
        confidence: f64,
        has_user_feedback: bool,
        reconstructed_at: &str,
    ) -> ContextResumeState {
        // 1. Resolve "left_off_on" (Strict: do NOT invent a task!)
        let left_off_on = if state == ContextStateValue::InsufficientEvidence
            || state == ContextStateValue::Uncertain
        {
            None
        } else if let Some(title) = task_title {
            let trimmed = title.trim();
            if !trimmed.is_empty()
                && !is_generic_task_name(trimmed)
                && !trimmed.eq_ignore_ascii_case("untitled")
                && !trimmed.eq_ignore_ascii_case("unknown")
            {
                Some(trimmed.to_string())
            } else {
                None
            }
        } else {
            None
        };

        // 2. Resolve "last_meaningful_activity"
        let (last_act_text, last_act_type) = if let Some(act) = meaningful_activity {
            let title_candidate = act
                .reference
                .as_ref()
                .and_then(|r| r.title.as_deref())
                .map(|t| t.trim())
                .filter(|t| !t.is_empty() && !t.eq_ignore_ascii_case("untitled"));

            let text = if let Some(title) = title_candidate {
                title.to_string()
            } else {
                act.description.clone()
            };
            (Some(text), Some(act.activity_type))
        } else if let Some(title) = last_meaningful_item_title {
            let trimmed = title.trim();
            if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("untitled") {
                (Some(trimmed.to_string()), None)
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        // 3. Resolve "recent_work"
        let mut recent_work: Vec<String> = Vec::new();
        let mut seen_titles: HashSet<String> = HashSet::new();

        // Add from workset items first (ordered by relevance)
        if let Some(ws) = workset {
            for item in &ws.items {
                if let Some(ref title) = item.title {
                    let clean = title.trim();
                    if !clean.is_empty()
                        && !is_generic_noise(Some(clean), &item.content_snippet, true, false)
                    {
                        let norm = clean.to_lowercase();
                        if seen_titles.insert(norm) {
                            recent_work.push(clean.to_string());
                            if recent_work.len() >= 5 {
                                break;
                            }
                        }
                    }
                }
            }
        }

        // Add from memory recent items if we need more
        if recent_work.len() < 5 {
            if let Some(mem) = memory {
                for item_ref in &mem.recent_items {
                    if let Some(ref title) = item_ref.title {
                        let clean = title.trim();
                        if !clean.is_empty() {
                            let norm = clean.to_lowercase();
                            if seen_titles.insert(norm) {
                                recent_work.push(clean.to_string());
                                if recent_work.len() >= 5 {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Add from items directly if we need more
        if recent_work.len() < 5 {
            for item in items {
                if let Some(ref title) = item.title {
                    let clean = title.trim();
                    if !clean.is_empty()
                        && !is_generic_noise(Some(clean), &item.content, false, false)
                    {
                        let norm = clean.to_lowercase();
                        if seen_titles.insert(norm) {
                            recent_work.push(clean.to_string());
                            if recent_work.len() >= 5 {
                                break;
                            }
                        }
                    }
                }
            }
        }

        // 4. Construct Formatted Resume Summary Block
        let mut lines = Vec::new();
        lines.push(context_name.to_string());
        lines.push(String::new());

        if let Some(ref task) = left_off_on {
            lines.push("You left off on:".to_string());
            lines.push(task.clone());
            lines.push(String::new());

            if let Some(ref act) = last_act_text {
                lines.push("Last meaningful activity:".to_string());
                lines.push(act.clone());
                lines.push(String::new());
            }
        } else if let Some(ref act) = last_act_text {
            lines.push("Last known activity:".to_string());
            lines.push(act.clone());
            lines.push(String::new());
        } else if state == ContextStateValue::InsufficientEvidence {
            lines.push("No prior activity recorded.".to_string());
            lines.push(String::new());
        }

        if !recent_work.is_empty() {
            lines.push("Recent work:".to_string());
            for item in &recent_work {
                lines.push(item.clone());
            }
        }

        while lines.last().map(|s| s.is_empty()).unwrap_or(false) {
            lines.pop();
        }

        let formatted_resume_summary = lines.join("\n");

        // 5. Evaluate Resume Readiness (Day 19 Step 6) with State Freshness (Day 19 Step 11)
        let workset_count = workset.map(|w| w.items.len()).unwrap_or(0);
        let count_for_readiness = if workset_count > 0 {
            workset_count
        } else {
            recent_work.len()
        };

        let has_recent_activity = !recent_work.is_empty()
            || state == ContextStateValue::Active
            || state == ContextStateValue::RecentlyActive;

        let readiness = ResumeReadinessEvaluator::evaluate_with_freshness(
            state,
            left_off_on.as_deref(),
            count_for_readiness,
            last_act_text.is_some(),
            has_recent_activity,
            has_user_feedback,
            freshness,
        );

        ContextResumeState {
            context_id: context_id.to_string(),
            context_name: context_name.to_string(),
            left_off_on,
            last_meaningful_activity: last_act_text,
            last_activity_type: last_act_type,
            recent_work,
            formatted_resume_summary,
            readiness,
            state,
            freshness,
            freshness_display: freshness_display.to_string(),
            confidence,
            reconstructed_at: reconstructed_at.to_string(),
        }
    }
}

// =========================================================================
// ContextStateService: Public Interface
// =========================================================================

/// Service for reconstructing and querying derived `ContextState`.
pub struct ContextStateService;

impl ContextStateService {
    pub fn new() -> Self {
        Self
    }

    /// Derives the contextual working state for a specific context/workspace.
    pub fn derive_for_context(
        &self,
        conn: &Connection,
        context_id: &str,
    ) -> Result<ContextState, String> {
        ContextStateBuilder::from_db(conn, context_id)?.build()
    }

    /// Reconstructs the human-readable resume state for a context.
    ///
    /// Answers: "Where did I leave this work?" in structured, human-readable format.
    /// Never launches applications or files (pure state reconstruction).
    pub fn get_resume_state(
        &self,
        conn: &Connection,
        context_id: &str,
    ) -> Result<ContextResumeState, String> {
        let state = self.derive_for_context(conn, context_id)?;
        Ok(state.resume_state)
    }

    /// Evaluates the resume readiness of a context without exposing numerical scores.
    pub fn evaluate_readiness(
        &self,
        conn: &Connection,
        context_id: &str,
    ) -> Result<ResumeReadinessAssessment, String> {
        let state = self.derive_for_context(conn, context_id)?;
        Ok(state.readiness)
    }

    /// Derives contextual working states for all available contexts/workspaces.
    pub fn derive_all(&self, conn: &Connection) -> Result<Vec<ContextState>, String> {
        let workspaces = crate::db::operations::list_workspaces(conn)
            .map_err(|e| format!("Failed to list workspaces: {}", e))?;
        let mut states = Vec::new();
        for ws in workspaces {
            if let Ok(state) = self.derive_for_context(conn, &ws.id) {
                states.push(state);
            }
        }
        Ok(states)
    }

    /// Reconstructs human-readable resume states for all available contexts/workspaces.
    pub fn get_all_resume_states(
        &self,
        conn: &Connection,
    ) -> Result<Vec<ContextResumeState>, String> {
        let states = self.derive_all(conn)?;
        Ok(states.into_iter().map(|s| s.resume_state).collect())
    }
}

// =========================================================================
// Unit Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_engine::memory::{InformationProvenance, MemoryExplanation};
    use crate::context_engine::segments::{ContextActivitySegment, SegmentItemReference};
    use crate::context_engine::task_transitions::{DetectedTaskTransition, TaskTransitionType};
    use crate::context_engine::transitions::{
        ContextReference, ContextTransition, ContextTransitionType,
    };
    use crate::context_engine::workset::{
        ContextWorkset, WorksetItemReference, WorksetItemRelevance, WorksetState,
    };
    use crate::db::models::{Item, TaskCandidate, TaskCandidateStatus};
    use chrono::{Duration, Utc};

    fn make_test_item(id: &str, title: &str, app: Option<&str>, created_at: &str) -> Item {
        let metadata = match app {
            Some(a) => format!(r#"{{"app_name":"{}"}}"#, a),
            None => "{}".to_string(),
        };
        Item {
            id: id.to_string(),
            workspace_id: Some("ctx-1".to_string()),
            session_id: None,
            item_type: "text".to_string(),
            content: format!("Content for {}", title),
            title: Some(title.to_string()),
            metadata,
            created_at: created_at.to_string(),
            updated_at: created_at.to_string(),
        }
    }

    // =====================================================================
    // STEP 4 REQUIRED TESTS: Meaningful Activity Resolver
    // =====================================================================

    #[test]
    fn test_resolve_saved_item_over_application_launch() {
        let now = Utc::now();
        let item_time = (now - Duration::minutes(3)).to_rfc3339();

        let item = Item {
            id: "item-framer".to_string(),
            workspace_id: Some("ctx-1".to_string()),
            session_id: None,
            item_type: "url".to_string(),
            content: "https://www.framer.com/motion/animation/".to_string(),
            title: Some("Saved Framer Motion animation reference".to_string()),
            metadata: r#"{"app_name":"Chrome","domain":"framer.com"}"#.to_string(),
            created_at: item_time.clone(),
            updated_at: item_time.clone(),
        };

        let workset_item = WorksetItemReference {
            id: "item-framer".to_string(),
            title: Some("Framer Motion animation reference".to_string()),
            item_type: "url".to_string(),
            relevance: WorksetItemRelevance::Primary,
            relevance_score: 0.92,
            reasons: vec!["Core animation reference".to_string()],
            provenance: InformationProvenance::Observed,
            created_at: item_time.clone(),
            last_active_at: Some(item_time.clone()),
            app_name: Some("Chrome".to_string()),
            domain: Some("framer.com".to_string()),
            content_snippet: "https://www.framer.com/motion/animation/".to_string(),
            is_suggested_addition: false,
        };

        let workset = ContextWorkset {
            context_id: "ctx-1".to_string(),
            context_name: "Aurora Website".to_string(),
            task_id: Some("task-1".to_string()),
            task_title: Some("Hero animation".to_string()),
            generated_at: now.to_rfc3339(),
            last_active_at: Some(item_time.clone()),
            workset_state: WorksetState::Active,
            items: vec![workset_item],
            recent_activity: "1 animation item saved".to_string(),
            relevant_items: vec![],
            important_references: vec![],
            related_tasks: vec![],
            confidence: 0.92,
            confidence_band: "High".to_string(),
            provenance: InformationProvenance::Observed,
            explanation: MemoryExplanation {
                summary: "Hero animation workset".to_string(),
                why_reasons: vec![],
                supporting_evidence: vec![],
                confidence_band: "High".to_string(),
                provenance: InformationProvenance::Observed,
                uncertainty_note: None,
            },
        };

        let meaningful = MeaningfulActivityResolver::resolve_last_meaningful_activity(
            "ctx-1",
            Some("Aurora Website"),
            &[item],
            Some(&workset),
            None,
            &[],
            &[],
            &[],
            &[],
            false,
        );

        assert!(meaningful.is_some());
        let act = meaningful.unwrap();
        // Explicit meaningful description rather than generic "Chrome was active"
        assert!(act.description.contains("Framer Motion animation reference"));
        assert!(act.description.contains("Chrome"));
        assert_eq!(act.activity_type, MeaningfulActivityType::TaskRelatedItem);
        assert!(act.confidence >= 0.85);
    }

    #[test]
    fn test_resolve_user_organization_highest_priority() {
        let now = Utc::now();
        let org_time = (now - Duration::minutes(10)).to_rfc3339();

        let item = Item {
            id: "item-org".to_string(),
            workspace_id: Some("ctx-1".to_string()),
            session_id: None,
            item_type: "file".to_string(),
            content: "docs/architecture.md".to_string(),
            title: Some("Architecture Specs".to_string()),
            metadata: r#"{"source":"user_confirmed","user_action":"organized"}"#.to_string(),
            created_at: org_time.clone(),
            updated_at: org_time.clone(),
        };

        let meaningful = MeaningfulActivityResolver::resolve_last_meaningful_activity(
            "ctx-1",
            Some("Backend Core"),
            &[item],
            None,
            None,
            &[],
            &[],
            &[],
            &[],
            true, // user feedback confirmed
        );

        assert!(meaningful.is_some());
        let act = meaningful.unwrap();
        assert_eq!(act.activity_type, MeaningfulActivityType::UserOrganization);
        assert_eq!(act.provenance, "confirmed");
        assert_eq!(act.description, "Organized Architecture Specs");
    }

    #[test]
    fn test_resolve_task_transition_meaningful_activity() {
        let now = Utc::now();
        let tt_time = (now - Duration::minutes(4)).to_rfc3339();

        let task_trans = DetectedTaskTransition {
            id: "tt-1".to_string(),
            context_id: Some("ctx-1".to_string()),
            transition_type: TaskTransitionType::TaskSwitch,
            from_task: Some("Hero Animation".to_string()),
            to_task: Some("Contact Form Integration".to_string()),
            confidence: 0.88,
            timestamp: tt_time.clone(),
            evidence: vec!["Observed submit handlers in form.tsx".to_string()],
            trigger_item_id: None,
            duration_seconds: None,
            metadata: serde_json::json!({}),
        };

        let meaningful = MeaningfulActivityResolver::resolve_last_meaningful_activity(
            "ctx-1",
            Some("Aurora Website"),
            &[],
            None,
            None,
            &[],
            &[],
            &[],
            &[task_trans],
            false,
        );

        assert!(meaningful.is_some());
        let act = meaningful.unwrap();
        assert_eq!(act.activity_type, MeaningfulActivityType::TaskTransition);
        assert_eq!(
            act.description,
            "Switched task from Hero Animation to Contact Form Integration"
        );
        assert_eq!(act.reference.unwrap().id, "tt-1");
    }

    #[test]
    fn test_resolve_context_transition_meaningful_activity() {
        let now = Utc::now();
        let ret_time = (now - Duration::minutes(2)).to_rfc3339();

        let transition = ContextTransition {
            id: "trans-ret-1".to_string(),
            from_context_id: Some("ctx-detour".to_string()),
            to_context_id: Some("ctx-1".to_string()),
            transition_type: ContextTransitionType::ContextReturn,
            confidence: 0.90,
            detected_at: ret_time.clone(),
            session_id: None,
            trigger_item_id: None,
            trigger_signal_id: None,
            metadata: "{}".to_string(),
            created_at: ret_time.clone(),
        };

        let meaningful = MeaningfulActivityResolver::resolve_last_meaningful_activity(
            "ctx-1",
            Some("Portfolio"),
            &[],
            None,
            None,
            &[],
            &[transition],
            &[],
            &[],
            false,
        );

        assert!(meaningful.is_some());
        let act = meaningful.unwrap();
        assert_eq!(act.activity_type, MeaningfulActivityType::ContextTransition);
        assert_eq!(act.description, "Returned to Portfolio after interruption");
    }

    #[test]
    fn test_resolve_avoids_duplicate_captures_and_window_switches() {
        let now = Utc::now();
        let time = (now - Duration::minutes(5)).to_rfc3339();

        // Generic noise item and duplicate captures
        let noise_item = Item {
            id: "item-noise".to_string(),
            workspace_id: Some("ctx-1".to_string()),
            session_id: None,
            item_type: "url".to_string(),
            content: "chrome://newtab".to_string(),
            title: Some("New Tab".to_string()),
            metadata: "{}".to_string(),
            created_at: time.clone(),
            updated_at: time.clone(),
        };

        let empty_item = Item {
            id: "item-empty".to_string(),
            workspace_id: Some("ctx-1".to_string()),
            session_id: None,
            item_type: "text".to_string(),
            content: "".to_string(),
            title: Some("Untitled".to_string()),
            metadata: "{}".to_string(),
            created_at: time.clone(),
            updated_at: time.clone(),
        };

        let meaningful = MeaningfulActivityResolver::resolve_last_meaningful_activity(
            "ctx-1",
            Some("Aurora Website"),
            &[noise_item, empty_item],
            None,
            None,
            &[],
            &[],
            &[],
            &[],
            false,
        );

        // Noise and empty items are filtered out completely
        assert!(meaningful.is_none());
    }

    // =====================================================================
    // STEP 3 TESTS: Context State Reconstruction
    // =====================================================================

    #[test]
    fn test_strong_evidence() {
        let now = Utc::now();
        let recent_ts = (now - Duration::minutes(2)).to_rfc3339();

        let item = make_test_item("item-1", "Hero design", Some("Figma"), &recent_ts);
        let seg = ContextActivitySegment {
            id: "seg-1".to_string(),
            context: Some(ContextReference::workspace("ctx-1", "Aurora Website")),
            started_at: (now - Duration::minutes(15)).to_rfc3339(),
            ended_at: recent_ts.clone(),
            duration_seconds: 780,
            duration_formatted: "13m".to_string(),
            confidence: 0.90,
            session_ids: vec!["sess-1".to_string()],
            item_references: vec![SegmentItemReference {
                item_id: "item-1".to_string(),
                title: Some("Hero design".to_string()),
                item_type: "text".to_string(),
                timestamp: recent_ts.clone(),
                confidence: 0.90,
            }],
            transition_in: None,
            transition_out: None,
            is_ambiguous: false,
            item_count: 1,
            metadata: serde_json::json!({}),
        };

        let task = TaskCandidate {
            id: "task-1".to_string(),
            context_id: Some("ctx-1".to_string()),
            title: "Hero design".to_string(),
            confidence: 0.90,
            status: TaskCandidateStatus::Active,
            first_detected_at: (now - Duration::minutes(30)).to_rfc3339(),
            last_active_at: recent_ts.clone(),
            created_at: (now - Duration::minutes(30)).to_rfc3339(),
            updated_at: recent_ts.clone(),
        };

        let state = ContextStateBuilder::new("ctx-1")
            .with_context_name("Aurora Website")
            .with_reference_time(now)
            .with_items(vec![item])
            .with_activity_segments(vec![seg])
            .with_task_candidates(vec![task])
            .build()
            .unwrap();

        assert_eq!(state.state, ContextStateValue::Active);
        assert_eq!(state.task_title.as_deref(), Some("Hero design"));
        assert_eq!(state.active_application.as_deref(), Some("Figma"));
        assert_eq!(state.last_meaningful_activity, "Last known activity: Hero design in Figma.");
        assert!(state.meaningful_activity.is_some());
    }

    #[test]
    fn test_weak_evidence() {
        let now = Utc::now();
        let capture_ts = (now - Duration::minutes(20)).to_rfc3339();

        let item = Item {
            id: "item-url".to_string(),
            workspace_id: Some("ctx-1".to_string()),
            session_id: None,
            item_type: "url".to_string(),
            content: "https://figma.com/file/12345/Aurora".to_string(),
            title: None,
            metadata: r#"{"app_name":"Figma","domain":"figma.com"}"#.to_string(),
            created_at: capture_ts.clone(),
            updated_at: capture_ts.clone(),
        };

        let state = ContextStateBuilder::new("ctx-1")
            .with_context_name("Aurora Website")
            .with_reference_time(now)
            .with_items(vec![item])
            .build()
            .unwrap();

        assert_eq!(state.task_title, None);
        assert_eq!(state.active_application.as_deref(), Some("Figma"));
        assert_eq!(state.last_meaningful_activity, "Last known activity: Figma page related to Aurora Website.");
    }

    #[test]
    fn test_conflicting_evidence() {
        let now = Utc::now();
        let ambiguous_ts = (now - Duration::minutes(5)).to_rfc3339();

        let item = make_test_item("item-1", "Ambiguous signal", None, &ambiguous_ts);

        let ambiguous_trans = ContextTransition {
            id: "trans-amb".to_string(),
            from_context_id: Some("ctx-1".to_string()),
            to_context_id: Some("ctx-2".to_string()),
            transition_type: ContextTransitionType::AmbiguousTransition,
            confidence: 0.30,
            detected_at: ambiguous_ts.clone(),
            session_id: None,
            trigger_item_id: None,
            trigger_signal_id: None,
            metadata: "{}".to_string(),
            created_at: ambiguous_ts.clone(),
        };

        let state = ContextStateBuilder::new("ctx-1")
            .with_context_name("Aurora Website")
            .with_reference_time(now)
            .with_items(vec![item])
            .with_context_transitions(vec![ambiguous_trans])
            .build()
            .unwrap();

        assert_eq!(state.state, ContextStateValue::Uncertain);
        assert_eq!(
            state.last_meaningful_activity,
            "Uncertain recent activity in Aurora Website with competing context signals."
        );
    }

    #[test]
    fn test_multiple_applications() {
        let now = Utc::now();
        let recent_ts = (now - Duration::minutes(5)).to_rfc3339();

        let item_code = make_test_item("item-1", "Hero component", Some("VSCode"), &recent_ts);
        let item_browser = make_test_item("item-2", "Hero animation specs", Some("Safari"), &recent_ts);

        let seg = ContextActivitySegment {
            id: "seg-multi".to_string(),
            context: Some(ContextReference::workspace("ctx-1", "Portfolio")),
            started_at: (now - Duration::minutes(20)).to_rfc3339(),
            ended_at: recent_ts.clone(),
            duration_seconds: 900,
            duration_formatted: "15m".to_string(),
            confidence: 0.88,
            session_ids: vec!["sess-1".to_string()],
            item_references: vec![],
            transition_in: None,
            transition_out: None,
            is_ambiguous: false,
            item_count: 2,
            metadata: serde_json::json!({}),
        };

        let task = TaskCandidate {
            id: "task-1".to_string(),
            context_id: Some("ctx-1".to_string()),
            title: "Hero section".to_string(),
            confidence: 0.88,
            status: TaskCandidateStatus::Active,
            first_detected_at: (now - Duration::minutes(30)).to_rfc3339(),
            last_active_at: recent_ts.clone(),
            created_at: (now - Duration::minutes(30)).to_rfc3339(),
            updated_at: recent_ts.clone(),
        };

        let state = ContextStateBuilder::new("ctx-1")
            .with_context_name("Portfolio")
            .with_reference_time(now)
            .with_items(vec![item_code, item_browser])
            .with_activity_segments(vec![seg])
            .with_task_candidates(vec![task])
            .build()
            .unwrap();

        assert_eq!(state.state, ContextStateValue::Active);
        assert_eq!(state.recent_applications.len(), 2);
        assert!(state.recent_applications.contains(&"VSCode".to_string()));
        assert!(state.recent_applications.contains(&"Safari".to_string()));
        assert_eq!(
            state.last_meaningful_activity,
            "Last known activity: Hero section across Safari and VSCode."
        );
    }

    #[test]
    fn test_task_transition() {
        let now = Utc::now();
        let task_shift_time = (now - Duration::minutes(4)).to_rfc3339();

        let item = make_test_item("item-1", "Footer contact form", Some("VSCode"), &task_shift_time);

        let task_trans = DetectedTaskTransition {
            id: "tt-1".to_string(),
            context_id: Some("ctx-1".to_string()),
            transition_type: TaskTransitionType::TaskSwitch,
            from_task: Some("Hero Animation".to_string()),
            to_task: Some("Contact Form Integration".to_string()),
            confidence: 0.82,
            timestamp: task_shift_time.clone(),
            evidence: vec![],
            trigger_item_id: None,
            duration_seconds: None,
            metadata: serde_json::json!({}),
        };

        let state = ContextStateBuilder::new("ctx-1")
            .with_context_name("Aurora Website")
            .with_reference_time(now)
            .with_items(vec![item])
            .with_task_transitions(vec![task_trans])
            .build()
            .unwrap();

        assert_eq!(
            state.last_meaningful_activity,
            "Switched task from Hero Animation to Contact Form Integration."
        );
    }

    #[test]
    fn test_context_transition() {
        let now = Utc::now();
        let switch_time = (now - Duration::minutes(25)).to_rfc3339();

        let item = make_test_item("item-1", "Hero design", Some("Figma"), &switch_time);

        let transition = ContextTransition {
            id: "trans-switch-1".to_string(),
            from_context_id: Some("ctx-1".to_string()),
            to_context_id: Some("ctx-2".to_string()),
            transition_type: ContextTransitionType::ContextSwitch,
            confidence: 0.90,
            detected_at: switch_time.clone(),
            session_id: None,
            trigger_item_id: None,
            trigger_signal_id: None,
            metadata: "{}".to_string(),
            created_at: switch_time.clone(),
        };

        let task = TaskCandidate {
            id: "task-1".to_string(),
            context_id: Some("ctx-1".to_string()),
            title: "Hero design".to_string(),
            confidence: 0.85,
            status: TaskCandidateStatus::Active,
            first_detected_at: (now - Duration::minutes(60)).to_rfc3339(),
            last_active_at: switch_time.clone(),
            created_at: (now - Duration::minutes(60)).to_rfc3339(),
            updated_at: switch_time.clone(),
        };

        let state = ContextStateBuilder::new("ctx-1")
            .with_context_name("Aurora Website")
            .with_reference_time(now)
            .with_items(vec![item])
            .with_context_transitions(vec![transition])
            .with_task_candidates(vec![task])
            .build()
            .unwrap();

        assert_eq!(state.state, ContextStateValue::Paused);
        assert_eq!(
            state.last_meaningful_activity,
            "Paused work on Hero design in Figma."
        );
    }

    #[test]
    fn test_long_inactivity() {
        let now = Utc::now();
        let two_weeks_ago = (now - Duration::days(14)).to_rfc3339();

        let item = make_test_item("item-old", "Archived database specs", None, &two_weeks_ago);

        let task = TaskCandidate {
            id: "task-old".to_string(),
            context_id: Some("ctx-1".to_string()),
            title: "Database Migration".to_string(),
            confidence: 0.80,
            status: TaskCandidateStatus::Active,
            first_detected_at: two_weeks_ago.clone(),
            last_active_at: two_weeks_ago.clone(),
            created_at: two_weeks_ago.clone(),
            updated_at: two_weeks_ago.clone(),
        };

        let state = ContextStateBuilder::new("ctx-1")
            .with_context_name("Backend Core")
            .with_reference_time(now)
            .with_items(vec![item])
            .with_task_candidates(vec![task])
            .build()
            .unwrap();

        assert_eq!(state.state, ContextStateValue::Dormant);
        assert_eq!(
            state.last_meaningful_activity,
            "Dormant context. Last known activity: Database Migration (no recent activity for over a week)."
        );
    }

    #[test]
    fn test_context_return() {
        let now = Utc::now();
        let return_time = (now - Duration::minutes(2)).to_rfc3339();

        let item = make_test_item("item-1", "Hero animation code", Some("Xcode"), &return_time);

        let return_transition = ContextTransition {
            id: "trans-ret-1".to_string(),
            from_context_id: Some("ctx-detour".to_string()),
            to_context_id: Some("ctx-1".to_string()),
            transition_type: ContextTransitionType::ContextReturn,
            confidence: 0.88,
            detected_at: return_time.clone(),
            session_id: None,
            trigger_item_id: None,
            trigger_signal_id: None,
            metadata: "{}".to_string(),
            created_at: return_time.clone(),
        };

        let task = TaskCandidate {
            id: "task-1".to_string(),
            context_id: Some("ctx-1".to_string()),
            title: "Hero Animation".to_string(),
            confidence: 0.88,
            status: TaskCandidateStatus::Active,
            first_detected_at: (now - Duration::minutes(40)).to_rfc3339(),
            last_active_at: return_time.clone(),
            created_at: (now - Duration::minutes(40)).to_rfc3339(),
            updated_at: return_time.clone(),
        };

        let state = ContextStateBuilder::new("ctx-1")
            .with_context_name("Aurora Website")
            .with_reference_time(now)
            .with_items(vec![item])
            .with_context_transitions(vec![return_transition])
            .with_task_candidates(vec![task])
            .build()
            .unwrap();

        assert_eq!(state.state, ContextStateValue::Active);
        assert_eq!(
            state.last_meaningful_activity,
            "Returned to Aurora Website working on Hero Animation."
        );
    }

    // =====================================================================
    // STEP 2 BASELINE REGRESSION TESTS
    // =====================================================================

    #[test]
    fn test_active_context() {
        let now = Utc::now();
        let recent_ts = (now - Duration::minutes(3)).to_rfc3339();

        let item = make_test_item("item-1", "HeroBanner Component", Some("VSCode"), &recent_ts);
        let seg = ContextActivitySegment {
            id: "seg-1".to_string(),
            context: Some(ContextReference::workspace("ctx-1", "Portfolio")),
            started_at: (now - Duration::minutes(10)).to_rfc3339(),
            ended_at: recent_ts.clone(),
            duration_seconds: 420,
            duration_formatted: "7m".to_string(),
            confidence: 0.85,
            session_ids: vec!["sess-1".to_string()],
            item_references: vec![SegmentItemReference {
                item_id: "item-1".to_string(),
                title: Some("HeroBanner Component".to_string()),
                item_type: "text".to_string(),
                timestamp: recent_ts.clone(),
                confidence: 0.85,
            }],
            transition_in: None,
            transition_out: None,
            is_ambiguous: false,
            item_count: 1,
            metadata: serde_json::json!({}),
        };

        let task = TaskCandidate {
            id: "task-1".to_string(),
            context_id: Some("ctx-1".to_string()),
            title: "Build Hero Section".to_string(),
            confidence: 0.85,
            status: TaskCandidateStatus::Active,
            first_detected_at: (now - Duration::minutes(20)).to_rfc3339(),
            last_active_at: recent_ts.clone(),
            created_at: (now - Duration::minutes(20)).to_rfc3339(),
            updated_at: recent_ts.clone(),
        };

        let state = ContextStateBuilder::new("ctx-1")
            .with_reference_time(now)
            .with_items(vec![item])
            .with_activity_segments(vec![seg])
            .with_task_candidates(vec![task])
            .build()
            .unwrap();

        assert_eq!(state.state, ContextStateValue::Active);
        assert!(state.state_confidence >= 0.75);
        assert_eq!(state.task_title.as_deref(), Some("Build Hero Section"));
        assert_eq!(state.task_id.as_deref(), Some("task-1"));
        assert_eq!(state.last_activity_segment_id.as_deref(), Some("seg-1"));
        assert_eq!(state.active_application.as_deref(), Some("VSCode"));
        assert!(state.latest_meaningful_action.is_some());
    }

    #[test]
    fn test_recently_active_context() {
        let now = Utc::now();
        let two_hours_ago = (now - Duration::hours(2)).to_rfc3339();

        let item = make_test_item("item-1", "Trip planning itinerary", Some("Safari"), &two_hours_ago);
        let seg = ContextActivitySegment {
            id: "seg-1".to_string(),
            context: Some(ContextReference::workspace("ctx-1", "TripWise")),
            started_at: (now - Duration::hours(3)).to_rfc3339(),
            ended_at: two_hours_ago.clone(),
            duration_seconds: 3600,
            duration_formatted: "1h".to_string(),
            confidence: 0.80,
            session_ids: vec!["sess-1".to_string()],
            item_references: vec![],
            transition_in: None,
            transition_out: None,
            is_ambiguous: false,
            item_count: 1,
            metadata: serde_json::json!({}),
        };

        let state = ContextStateBuilder::new("ctx-1")
            .with_reference_time(now)
            .with_items(vec![item])
            .with_activity_segments(vec![seg])
            .build()
            .unwrap();

        assert_eq!(state.state, ContextStateValue::RecentlyActive);
        assert!(state.state_confidence >= 0.60);
        assert_eq!(state.last_active_at, Some(two_hours_ago));
    }

    #[test]
    fn test_paused_context() {
        let now = Utc::now();
        let switch_time = (now - Duration::minutes(25)).to_rfc3339();

        let item = make_test_item("item-1", "Database schema", Some("DataGrip"), &switch_time);

        let transition = ContextTransition {
            id: "trans-1".to_string(),
            from_context_id: Some("ctx-1".to_string()),
            to_context_id: Some("ctx-2".to_string()),
            transition_type: ContextTransitionType::ContextSwitch,
            confidence: 0.90,
            detected_at: switch_time.clone(),
            session_id: None,
            trigger_item_id: None,
            trigger_signal_id: None,
            metadata: "{}".to_string(),
            created_at: switch_time.clone(),
        };

        let state = ContextStateBuilder::new("ctx-1")
            .with_reference_time(now)
            .with_items(vec![item])
            .with_context_transitions(vec![transition])
            .build()
            .unwrap();

        assert_eq!(state.state, ContextStateValue::Paused);
        assert_eq!(state.last_transition_id.as_deref(), Some("trans-1"));
        assert_eq!(
            state.latest_meaningful_action.as_deref(),
            Some("Switched away to another context")
        );
    }

    #[test]
    fn test_dormant_context() {
        let now = Utc::now();
        let two_weeks_ago = (now - Duration::days(14)).to_rfc3339();

        let item = make_test_item("item-old", "Archived specs", None, &two_weeks_ago);

        let state = ContextStateBuilder::new("ctx-1")
            .with_reference_time(now)
            .with_items(vec![item])
            .build()
            .unwrap();

        assert_eq!(state.state, ContextStateValue::Dormant);
        assert!(state.state_confidence > 0.0);
    }

    #[test]
    fn test_missing_task() {
        let now = Utc::now();
        let five_mins_ago = (now - Duration::minutes(5)).to_rfc3339();

        let item = make_test_item("item-1", "Scratch notes", Some("Notes"), &five_mins_ago);

        let seg = ContextActivitySegment {
            id: "seg-1".to_string(),
            context: Some(ContextReference::workspace("ctx-1", "General Workspace")),
            started_at: (now - Duration::minutes(10)).to_rfc3339(),
            ended_at: five_mins_ago.clone(),
            duration_seconds: 300,
            duration_formatted: "5m".to_string(),
            confidence: 0.70,
            session_ids: vec![],
            item_references: vec![],
            transition_in: None,
            transition_out: None,
            is_ambiguous: false,
            item_count: 1,
            metadata: serde_json::json!({}),
        };

        let state = ContextStateBuilder::new("ctx-1")
            .with_reference_time(now)
            .with_items(vec![item])
            .with_activity_segments(vec![seg])
            .build()
            .unwrap();

        assert_eq!(state.task_id, None);
        assert_eq!(state.task_title, None);
        assert_eq!(state.state, ContextStateValue::Active);
    }

    #[test]
    fn test_missing_workset() {
        let now = Utc::now();
        let ten_mins_ago = (now - Duration::minutes(10)).to_rfc3339();

        let item = make_test_item("item-1", "Design mock", Some("Figma"), &ten_mins_ago);

        let state = ContextStateBuilder::new("ctx-1")
            .with_reference_time(now)
            .with_items(vec![item])
            .with_workset(None)
            .build()
            .unwrap();

        assert_eq!(state.current_workset, None);
        assert_eq!(state.state, ContextStateValue::RecentlyActive);
        assert_eq!(state.recent_item_ids, vec!["item-1".to_string()]);
    }

    #[test]
    fn test_insufficient_evidence() {
        let now = Utc::now();

        let state = ContextStateBuilder::new("ctx-empty")
            .with_reference_time(now)
            .build()
            .unwrap();

        assert_eq!(state.state, ContextStateValue::InsufficientEvidence);
        assert_eq!(state.state_confidence, 0.0);
        assert_eq!(state.task_id, None);
        assert_eq!(state.task_title, None);
        assert_eq!(state.last_active_at, None);
        assert!(state.recent_item_ids.is_empty());
    }

    // =====================================================================
    // STEP 5 REQUIRED TESTS: Context Resume State
    // =====================================================================

    #[test]
    fn test_resume_state_aurora_hero_animation() {
        let now = Utc::now();
        let t1 = (now - Duration::minutes(15)).to_rfc3339();
        let t2 = (now - Duration::minutes(10)).to_rfc3339();
        let t3 = (now - Duration::minutes(3)).to_rfc3339();

        let item1 = make_test_item("item-1", "Figma hero design", Some("Figma"), &t1);
        let item2 = make_test_item("item-2", "React Hero component", Some("VSCode"), &t2);
        let item3 = make_test_item("item-3", "Framer Motion reference", Some("Chrome"), &t3);

        let workset_item1 = WorksetItemReference {
            id: "item-1".to_string(),
            title: Some("Figma hero design".to_string()),
            item_type: "text".to_string(),
            relevance: WorksetItemRelevance::Supporting,
            relevance_score: 0.85,
            reasons: vec!["Hero design mock".to_string()],
            provenance: InformationProvenance::Observed,
            created_at: t1.clone(),
            last_active_at: Some(t1.clone()),
            app_name: Some("Figma".to_string()),
            domain: None,
            content_snippet: "Figma design mock for hero".to_string(),
            is_suggested_addition: false,
        };

        let workset_item2 = WorksetItemReference {
            id: "item-2".to_string(),
            title: Some("React Hero component".to_string()),
            item_type: "code".to_string(),
            relevance: WorksetItemRelevance::Supporting,
            relevance_score: 0.88,
            reasons: vec!["React implementation".to_string()],
            provenance: InformationProvenance::Observed,
            created_at: t2.clone(),
            last_active_at: Some(t2.clone()),
            app_name: Some("VSCode".to_string()),
            domain: None,
            content_snippet: "export const Hero = () => ...".to_string(),
            is_suggested_addition: false,
        };

        let workset_item3 = WorksetItemReference {
            id: "item-3".to_string(),
            title: Some("Framer Motion animation reference".to_string()),
            item_type: "url".to_string(),
            relevance: WorksetItemRelevance::Primary,
            relevance_score: 0.94,
            reasons: vec!["Animation docs".to_string()],
            provenance: InformationProvenance::Observed,
            created_at: t3.clone(),
            last_active_at: Some(t3.clone()),
            app_name: Some("Chrome".to_string()),
            domain: Some("framer.com".to_string()),
            content_snippet: "https://www.framer.com/motion/animation/".to_string(),
            is_suggested_addition: false,
        };

        let workset = ContextWorkset {
            context_id: "ctx-aurora".to_string(),
            context_name: "Aurora Website".to_string(),
            task_id: Some("task-hero".to_string()),
            task_title: Some("Hero animation".to_string()),
            generated_at: now.to_rfc3339(),
            last_active_at: Some(t3.clone()),
            workset_state: WorksetState::Active,
            items: vec![workset_item1, workset_item2, workset_item3],
            recent_activity: "3 active items".to_string(),
            relevant_items: vec![],
            important_references: vec![],
            related_tasks: vec![],
            confidence: 0.90,
            confidence_band: "High".to_string(),
            provenance: InformationProvenance::Observed,
            explanation: MemoryExplanation {
                summary: "Hero animation".to_string(),
                why_reasons: vec![],
                supporting_evidence: vec![],
                confidence_band: "High".to_string(),
                provenance: InformationProvenance::Observed,
                uncertainty_note: None,
            },
        };

        let task = TaskCandidate {
            id: "task-hero".to_string(),
            context_id: Some("ctx-aurora".to_string()),
            title: "Hero animation".to_string(),
            confidence: 0.90,
            status: TaskCandidateStatus::Active,
            first_detected_at: t1.clone(),
            last_active_at: t3.clone(),
            created_at: t1.clone(),
            updated_at: t3.clone(),
        };

        let state = ContextStateBuilder::new("ctx-aurora")
            .with_context_name("Aurora Website")
            .with_reference_time(now)
            .with_items(vec![item1, item2, item3])
            .with_workset(Some(workset))
            .with_task_candidates(vec![task])
            .build()
            .unwrap();

        let resume = state.get_resume_state();
        assert_eq!(resume.context_id, "ctx-aurora");
        assert_eq!(resume.context_name, "Aurora Website");
        assert_eq!(resume.left_off_on.as_deref(), Some("Hero animation"));
        assert!(resume.last_meaningful_activity.is_some());
        assert!(resume
            .last_meaningful_activity
            .as_ref()
            .unwrap()
            .contains("Framer Motion animation reference"));
        assert!(resume.recent_work.len() >= 3);
        assert!(resume.recent_work.contains(&"Figma hero design".to_string()));
        assert!(resume.recent_work.contains(&"React Hero component".to_string()));

        let summary = &resume.formatted_resume_summary;
        assert!(summary.starts_with("Aurora Website"));
        assert!(summary.contains("You left off on:\nHero animation"));
        assert!(summary.contains("Last meaningful activity:"));
        assert!(summary.contains("Recent work:"));
    }

    #[test]
    fn test_resume_state_university_os_scheduling() {
        let now = Utc::now();
        let t1 = (now - Duration::minutes(20)).to_rfc3339();
        let t2 = (now - Duration::minutes(5)).to_rfc3339();

        let item = make_test_item("item-rr", "Round Robin implementation", Some("VSCode"), &t2);

        let workset_item = WorksetItemReference {
            id: "item-rr".to_string(),
            title: Some("Round Robin implementation".to_string()),
            item_type: "code".to_string(),
            relevance: WorksetItemRelevance::Primary,
            relevance_score: 0.95,
            reasons: vec!["Primary scheduler code".to_string()],
            provenance: InformationProvenance::Observed,
            created_at: t2.clone(),
            last_active_at: Some(t2.clone()),
            app_name: Some("VSCode".to_string()),
            domain: None,
            content_snippet: "void schedule_round_robin() ...".to_string(),
            is_suggested_addition: false,
        };

        let workset = ContextWorkset {
            context_id: "ctx-os".to_string(),
            context_name: "University OS".to_string(),
            task_id: Some("task-sched".to_string()),
            task_title: Some("Process scheduling assignment".to_string()),
            generated_at: now.to_rfc3339(),
            last_active_at: Some(t2.clone()),
            workset_state: WorksetState::Active,
            items: vec![workset_item],
            recent_activity: "Scheduler implemented".to_string(),
            relevant_items: vec![],
            important_references: vec![],
            related_tasks: vec![],
            confidence: 0.92,
            confidence_band: "High".to_string(),
            provenance: InformationProvenance::Observed,
            explanation: MemoryExplanation {
                summary: "Process scheduling".to_string(),
                why_reasons: vec![],
                supporting_evidence: vec![],
                confidence_band: "High".to_string(),
                provenance: InformationProvenance::Observed,
                uncertainty_note: None,
            },
        };

        let task = TaskCandidate {
            id: "task-sched".to_string(),
            context_id: Some("ctx-os".to_string()),
            title: "Process scheduling assignment".to_string(),
            confidence: 0.95,
            status: TaskCandidateStatus::Active,
            first_detected_at: t1.clone(),
            last_active_at: t2.clone(),
            created_at: t1.clone(),
            updated_at: t2.clone(),
        };

        let state = ContextStateBuilder::new("ctx-os")
            .with_context_name("University OS")
            .with_reference_time(now)
            .with_items(vec![item])
            .with_workset(Some(workset))
            .with_task_candidates(vec![task])
            .build()
            .unwrap();

        let resume = state.get_resume_state();
        assert_eq!(resume.context_name, "University OS");
        assert_eq!(resume.left_off_on.as_deref(), Some("Process scheduling assignment"));
        assert_eq!(
            resume.last_meaningful_activity.as_deref(),
            Some("Round Robin implementation")
        );

        let summary = &resume.formatted_resume_summary;
        assert!(summary.contains("University OS"));
        assert!(summary.contains("You left off on:\nProcess scheduling assignment"));
        assert!(summary.contains("Last meaningful activity:\nRound Robin implementation"));
    }

    #[test]
    fn test_resume_state_insufficient_evidence_no_task_invented() {
        let now = Utc::now();
        let five_mins_ago = (now - Duration::minutes(5)).to_rfc3339();

        let item = make_test_item("item-figma", "Figma hero design", Some("Figma"), &five_mins_ago);

        let state = ContextStateBuilder::new("ctx-aurora")
            .with_context_name("Aurora Website")
            .with_reference_time(now)
            .with_items(vec![item])
            .build()
            .unwrap();

        let resume = state.get_resume_state();
        assert_eq!(resume.context_name, "Aurora Website");
        // Strict invariant: Never invent a task without evidence!
        assert_eq!(resume.left_off_on, None);
        assert_eq!(
            resume.last_meaningful_activity.as_deref(),
            Some("Figma hero design")
        );

        let summary = &resume.formatted_resume_summary;
        assert!(summary.contains("Aurora Website"));
        assert!(!summary.contains("You left off on:"));
        assert!(summary.contains("Last known activity:\nFigma hero design"));
    }

    #[test]
    fn test_resume_state_empty_context() {
        let now = Utc::now();

        let state = ContextStateBuilder::new("ctx-empty")
            .with_context_name("Empty Project")
            .with_reference_time(now)
            .build()
            .unwrap();

        let resume = state.get_resume_state();
        assert_eq!(resume.context_name, "Empty Project");
        assert_eq!(resume.left_off_on, None);
        assert_eq!(resume.last_meaningful_activity, None);
        assert!(resume.recent_work.is_empty());
        assert_eq!(resume.state, ContextStateValue::InsufficientEvidence);
        assert!(resume.formatted_resume_summary.contains("No prior activity recorded."));
    }

    #[test]
    fn test_get_resume_state_service_end_to_end() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();

        let ws = crate::db::operations::create_workspace(&conn, "Design System").unwrap();

        let _item = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "code",
            "Button component code",
            Some("Button Component"),
            r#"{"app_name":"VSCode"}"#,
        )
        .unwrap();

        let service = ContextStateService::new();
        let resume = service.get_resume_state(&conn, &ws.id).unwrap();

        assert_eq!(resume.context_id, ws.id);
        assert_eq!(resume.context_name, "Design System");
        assert_eq!(resume.last_meaningful_activity.as_deref(), Some("Button Component"));
    }

    // =====================================================================
    // STEP 6 REQUIRED TESTS: Resume Readiness Evaluator
    // =====================================================================

    #[test]
    fn test_readiness_ready_aurora_website() {
        let assessment = ResumeReadinessEvaluator::evaluate(
            ContextStateValue::Active,
            Some("Hero animation"),
            5,
            true,
            true,
            false,
        );

        assert_eq!(assessment.readiness, ResumeReadiness::Ready);
        assert!(assessment.readiness.is_ready());
        assert_eq!(
            assessment.reasons,
            vec![
                "current task identified".to_string(),
                "5 relevant workset items".to_string(),
                "recent activity available".to_string(),
            ]
        );

        let formatted = &assessment.formatted_explanation;
        assert!(formatted.contains("Resume readiness:\nReady"));
        assert!(formatted.contains("Because:"));
        assert!(formatted.contains("- current task identified"));
        assert!(formatted.contains("- 5 relevant workset items"));
        assert!(formatted.contains("- recent activity available"));
    }

    #[test]
    fn test_readiness_partial_random_context() {
        let assessment = ResumeReadinessEvaluator::evaluate(
            ContextStateValue::RecentlyActive,
            None,
            1,
            false,
            false,
            false,
        );

        assert_eq!(assessment.readiness, ResumeReadiness::Partial);
        assert!(assessment.readiness.is_partial());
        assert_eq!(
            assessment.reasons,
            vec![
                "context identified".to_string(),
                "task uncertain".to_string(),
                "limited recent activity".to_string(),
            ]
        );

        let formatted = &assessment.formatted_explanation;
        assert!(formatted.contains("Resume readiness:\nPartial"));
        assert!(formatted.contains("Because:"));
        assert!(formatted.contains("- context identified"));
        assert!(formatted.contains("- task uncertain"));
        assert!(formatted.contains("- limited recent activity"));
    }

    #[test]
    fn test_readiness_mostly_ready_with_task_and_single_workset_item() {
        let assessment = ResumeReadinessEvaluator::evaluate(
            ContextStateValue::RecentlyActive,
            Some("Process scheduling assignment"),
            1,
            true,
            true,
            false,
        );

        assert_eq!(assessment.readiness, ResumeReadiness::MostlyReady);
        assert!(assessment.readiness.is_mostly_ready());
        assert!(assessment.reasons.contains(&"current task identified".to_string()));
    }

    #[test]
    fn test_readiness_insufficient_empty_context() {
        let assessment = ResumeReadinessEvaluator::evaluate(
            ContextStateValue::InsufficientEvidence,
            None,
            0,
            false,
            false,
            false,
        );

        assert_eq!(assessment.readiness, ResumeReadiness::Insufficient);
        assert!(assessment.readiness.is_insufficient());
        assert!(assessment.reasons.contains(&"no prior activity recorded".to_string()));
    }

    #[test]
    fn test_readiness_dormant_context_is_partial() {
        let assessment = ResumeReadinessEvaluator::evaluate(
            ContextStateValue::Dormant,
            Some("Old task"),
            2,
            true,
            false,
            false,
        );

        assert_eq!(assessment.readiness, ResumeReadiness::Partial);
        assert!(assessment.reasons.contains(&"limited recent activity".to_string()));
    }

    #[test]
    fn test_readiness_service_evaluate_readiness_end_to_end() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();

        let ws = crate::db::operations::create_workspace(&conn, "Aurora Website").unwrap();

        let _item = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://www.framer.com/motion/animation/",
            Some("Framer Motion animation reference"),
            r#"{"app_name":"Chrome"}"#,
        )
        .unwrap();

        let service = ContextStateService::new();
        let readiness = service.evaluate_readiness(&conn, &ws.id).unwrap();

        assert!(!readiness.reasons.is_empty());
        assert!(readiness.formatted_explanation.contains("Resume readiness:"));
    }

    #[test]
    fn test_state_explanation_distinguishes_observed_and_inferred() {
        let explanation = ContextStateBuilder::construct_state_explanation(
            "Aurora Website",
            ContextStateValue::Active,
            Some("Hero animation"),
            false, // Inferred task
            None,
            None,
            Some("Figma"),
            None,
            &[],
        );

        assert_eq!(explanation.title, "Why ContextSplit thinks this:");
        assert!(!explanation.reasons.is_empty());

        let task_reason = explanation.reasons.iter().find(|r| r.text.contains("Hero animation")).unwrap();
        assert_eq!(task_reason.provenance, "Inferred");

        let app_reason = explanation.reasons.iter().find(|r| r.text.contains("Figma")).unwrap();
        assert_eq!(app_reason.provenance, "Observed");

        let ws_reason = explanation.reasons.iter().find(|r| r.text.contains("Aurora Website")).unwrap();
        assert_eq!(ws_reason.provenance, "Observed");
    }

    #[test]
    fn test_state_explanation_uncertain_context_has_uncertainty_note() {
        let explanation = ContextStateBuilder::construct_state_explanation(
            "TripWise",
            ContextStateValue::Uncertain,
            None,
            false,
            None,
            None,
            None,
            None,
            &[],
        );

        assert!(explanation.uncertainty_note.is_some());
        assert!(explanation.uncertainty_note.unwrap().contains("emerging or sparse"));
    }

    // =========================================================================
    // Day 19 Step 9: Restore Context State After App Restart
    // =========================================================================

    #[test]
    fn test_context_state_restored_across_app_restart() {
        // Setup a persistent SQLite file on disk
        let db_path = format!(
            "{}/contextsplit_restart_test_{}.db",
            std::env::temp_dir().display(),
            uuid::Uuid::new_v4()
        );

        let ws_id: String;

        // Session 1: User works in context and closes app
        {
            let conn1 = Connection::open(&db_path).unwrap();
            crate::db::run_migrations(&conn1).unwrap();

            let ws = crate::db::operations::create_workspace(&conn1, "Aurora Website").unwrap();
            ws_id = ws.id.clone();

            // Create items in workspace
            let _item1 = crate::db::operations::create_item(
                &conn1,
                Some(&ws.id),
                None,
                "url",
                "https://www.figma.com/design/aurora-hero",
                Some("Figma hero design"),
                r#"{"app_name":"Figma"}"#,
            )
            .unwrap();

            let _item2 = crate::db::operations::create_item(
                &conn1,
                Some(&ws.id),
                None,
                "code",
                "export const Hero = () => <div>Hero</div>;",
                Some("React Hero component"),
                r#"{"app_name":"VS Code"}"#,
            )
            .unwrap();

            let _item3 = crate::db::operations::create_item(
                &conn1,
                Some(&ws.id),
                None,
                "url",
                "https://framer.com/motion",
                Some("Framer Motion reference"),
                r#"{"app_name":"Chrome"}"#,
            )
            .unwrap();

            // Create active task candidate
            let now_str = Utc::now().to_rfc3339();
            conn1.execute(
                "INSERT INTO task_candidates (id, context_id, title, confidence, status, first_detected_at, last_active_at, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    uuid::Uuid::new_v4().to_string(),
                    ws.id,
                    "Hero animation",
                    0.92,
                    "active",
                    now_str,
                    now_str,
                    now_str,
                    now_str,
                ],
            ).unwrap();

            // Explicit context transition
            let trans_id = uuid::Uuid::new_v4().to_string();
            conn1.execute(
                "INSERT INTO context_transitions (id, from_context_id, to_context_id, transition_type, confidence, detected_at, created_at)
                 VALUES (?1, NULL, ?2, 'context_start', 0.90, ?3, ?3)",
                rusqlite::params![trans_id, ws.id, now_str],
            ).unwrap();

            // Verify state is derivable in Session 1
            let state1 = ContextStateService::new().derive_for_context(&conn1, &ws.id).unwrap();
            assert_eq!(state1.context_name.as_deref(), Some("Aurora Website"));
            assert_eq!(state1.task_title.as_deref(), Some("Hero animation"));
            assert!(state1.is_working());

            // Drop conn1 -> simulates application termination / restart
        }

        // Session 2: ContextSplit restarts with a completely fresh connection
        {
            let conn2 = Connection::open(&db_path).unwrap();

            // Reconstruct ContextState entirely from persisted data
            let service = ContextStateService::new();
            let state2 = service.derive_for_context(&conn2, &ws_id).unwrap();

            // 1. Context identity
            assert_eq!(state2.context_id, ws_id);
            assert_eq!(state2.context_name.as_deref(), Some("Aurora Website"));

            // 2. You were working on: Hero animation
            assert_eq!(state2.task_title.as_deref(), Some("Hero animation"));

            // 3. Last active timestamp preserved
            assert!(state2.last_active_at.is_some());

            // 4. Continue with: relevant items preserved
            assert_eq!(state2.recent_item_ids.len(), 3);
            let resume = state2.resume_state;
            assert_eq!(resume.left_off_on.as_deref(), Some("Hero animation"));
            assert!(!resume.recent_work.is_empty());

            // 5. State explanation with progressive disclosure intact
            assert_eq!(state2.explanation.title, "Why ContextSplit thinks this:");
            assert!(!state2.explanation.reasons.is_empty());
            assert!(state2.explanation.reasons.iter().any(|r| r.text.contains("Hero animation")));

            // 6. Integrated Context Overview also works seamlessly
            let overview = crate::context_engine::overview::get_context_overview(&conn2, &ws_id).unwrap();
            assert_eq!(overview.name, "Aurora Website");
            assert!(overview.context_state.is_some());
            let overview_state = overview.context_state.unwrap();
            assert_eq!(overview_state.task_title.as_deref(), Some("Hero animation"));
        }

        // Clean up temporary database file
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn test_stale_context_not_falsely_active_after_restart() {
        let db_path = format!(
            "{}/contextsplit_stale_test_{}.db",
            std::env::temp_dir().display(),
            uuid::Uuid::new_v4()
        );

        let ws_id: String;

        // Session 1: Create workspace with activity 14 days ago
        {
            let conn1 = Connection::open(&db_path).unwrap();
            crate::db::run_migrations(&conn1).unwrap();

            let ws = crate::db::operations::create_workspace(&conn1, "University OS").unwrap();
            ws_id = ws.id.clone();

            let old_time = (Utc::now() - chrono::Duration::days(14)).to_rfc3339();

            let item = crate::db::operations::create_item(
                &conn1,
                Some(&ws.id),
                None,
                "text",
                "Round robin scheduling algorithm notes",
                Some("Process scheduling notes"),
                r#"{"app_name":"Notes"}"#,
            )
            .unwrap();

            // Update item creation time to 14 days ago
            conn1.execute(
                "UPDATE items SET created_at = ?1 WHERE id = ?2",
                rusqlite::params![old_time, item.id],
            ).unwrap();

            // Task candidate from 14 days ago
            conn1.execute(
                "INSERT INTO task_candidates (id, context_id, title, confidence, status, first_detected_at, last_active_at, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    uuid::Uuid::new_v4().to_string(),
                    ws.id,
                    "Process scheduling assignment",
                    0.80,
                    "detected",
                    old_time,
                    old_time,
                    old_time,
                    old_time,
                ],
            ).unwrap();
        }

        // Session 2: Fresh restart
        {
            let conn2 = Connection::open(&db_path).unwrap();

            let state = ContextStateService::new().derive_for_context(&conn2, &ws_id).unwrap();

            // Invariant: Stale context must NOT be falsely marked active
            assert_ne!(state.state, ContextStateValue::Active);
            assert!(
                state.state == ContextStateValue::Dormant
                    || state.state == ContextStateValue::Paused
                    || state.state == ContextStateValue::RecentlyActive
            );

            // Reconstructed resume memory is still intact for when the user decides to continue
            assert_eq!(state.context_name.as_deref(), Some("University OS"));
            assert_eq!(state.task_title.as_deref(), Some("Process scheduling assignment"));
            assert!(state.resume_state.left_off_on.is_some());
        }

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn test_multiple_independent_context_states() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();

        let now_str = Utc::now().to_rfc3339();

        // 1. Context A: Aurora Website
        let ws_aurora = crate::db::operations::create_workspace(&conn, "Aurora Website").unwrap();
        let _item_a1 = crate::db::operations::create_item(
            &conn,
            Some(&ws_aurora.id),
            None,
            "url",
            "https://www.figma.com/design/aurora-hero",
            Some("Figma hero design"),
            r#"{"app_name":"Figma"}"#,
        ).unwrap();
        let _item_a2 = crate::db::operations::create_item(
            &conn,
            Some(&ws_aurora.id),
            None,
            "code",
            "export const Hero = () => <div>Hero</div>;",
            Some("React Hero component"),
            r#"{"app_name":"VS Code"}"#,
        ).unwrap();
        conn.execute(
            "INSERT INTO task_candidates (id, context_id, title, confidence, status, first_detected_at, last_active_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                uuid::Uuid::new_v4().to_string(),
                ws_aurora.id,
                "Hero animation",
                0.92,
                "active",
                now_str,
                now_str,
                now_str,
                now_str,
            ],
        ).unwrap();

        // 2. Context B: University OS
        let ws_os = crate::db::operations::create_workspace(&conn, "University OS").unwrap();
        let _item_b1 = crate::db::operations::create_item(
            &conn,
            Some(&ws_os.id),
            None,
            "text",
            "Round robin scheduler algorithm code in C",
            Some("Round Robin implementation"),
            r#"{"app_name":"CLion"}"#,
        ).unwrap();
        conn.execute(
            "INSERT INTO task_candidates (id, context_id, title, confidence, status, first_detected_at, last_active_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                uuid::Uuid::new_v4().to_string(),
                ws_os.id,
                "Process scheduling assignment",
                0.88,
                "active",
                now_str,
                now_str,
                now_str,
                now_str,
            ],
        ).unwrap();

        // 3. Context C: Tokyo Trip
        let ws_tokyo = crate::db::operations::create_workspace(&conn, "Tokyo Trip").unwrap();
        let _item_c1 = crate::db::operations::create_item(
            &conn,
            Some(&ws_tokyo.id),
            None,
            "url",
            "https://hotel-booking.example.com/tokyo",
            Some("Tokyo hotel research"),
            r#"{"app_name":"Safari"}"#,
        ).unwrap();
        conn.execute(
            "INSERT INTO task_candidates (id, context_id, title, confidence, status, first_detected_at, last_active_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                uuid::Uuid::new_v4().to_string(),
                ws_tokyo.id,
                "7-day itinerary",
                0.85,
                "active",
                now_str,
                now_str,
                now_str,
                now_str,
            ],
        ).unwrap();

        let service = ContextStateService::new();

        // Switching sequence: Aurora -> University OS -> Tokyo Trip -> Aurora
        // Step 1: Aurora
        let aurora_state_1 = service.derive_for_context(&conn, &ws_aurora.id).unwrap();
        assert_eq!(aurora_state_1.context_name.as_deref(), Some("Aurora Website"));
        assert_eq!(aurora_state_1.task_title.as_deref(), Some("Hero animation"));
        assert_eq!(aurora_state_1.resume_state.left_off_on.as_deref(), Some("Hero animation"));
        assert!(aurora_state_1.resume_state.recent_work.iter().any(|w| w.contains("Figma hero design")));

        // Step 2: University OS
        let os_state_1 = service.derive_for_context(&conn, &ws_os.id).unwrap();
        assert_eq!(os_state_1.context_name.as_deref(), Some("University OS"));
        assert_eq!(os_state_1.task_title.as_deref(), Some("Process scheduling assignment"));
        assert_eq!(os_state_1.resume_state.left_off_on.as_deref(), Some("Process scheduling assignment"));
        assert!(os_state_1.resume_state.recent_work.iter().any(|w| w.contains("Round Robin implementation")));

        // Step 3: Tokyo Trip
        let tokyo_state_1 = service.derive_for_context(&conn, &ws_tokyo.id).unwrap();
        assert_eq!(tokyo_state_1.context_name.as_deref(), Some("Tokyo Trip"));
        assert_eq!(tokyo_state_1.task_title.as_deref(), Some("7-day itinerary"));
        assert_eq!(tokyo_state_1.resume_state.left_off_on.as_deref(), Some("7-day itinerary"));
        assert!(tokyo_state_1.resume_state.recent_work.iter().any(|w| w.contains("Tokyo hotel research")));

        // Step 4: Aurora returns with Aurora's previous state (state was NOT overwritten or corrupted)
        let aurora_state_2 = service.derive_for_context(&conn, &ws_aurora.id).unwrap();
        assert_eq!(aurora_state_2.context_id, ws_aurora.id);
        assert_eq!(aurora_state_2.context_name.as_deref(), Some("Aurora Website"));
        assert_eq!(aurora_state_2.task_title.as_deref(), Some("Hero animation"));
        assert_eq!(aurora_state_2.resume_state.left_off_on.as_deref(), Some("Hero animation"));

        // Step 5: University OS returns with its own state
        let os_state_2 = service.derive_for_context(&conn, &ws_os.id).unwrap();
        assert_eq!(os_state_2.context_id, ws_os.id);
        assert_eq!(os_state_2.context_name.as_deref(), Some("University OS"));
        assert_eq!(os_state_2.task_title.as_deref(), Some("Process scheduling assignment"));
        assert_eq!(os_state_2.resume_state.left_off_on.as_deref(), Some("Process scheduling assignment"));

        // Step 6: Tokyo Trip returns with its own state
        let tokyo_state_2 = service.derive_for_context(&conn, &ws_tokyo.id).unwrap();
        assert_eq!(tokyo_state_2.context_id, ws_tokyo.id);
        assert_eq!(tokyo_state_2.context_name.as_deref(), Some("Tokyo Trip"));
        assert_eq!(tokyo_state_2.task_title.as_deref(), Some("7-day itinerary"));
        assert_eq!(tokyo_state_2.resume_state.left_off_on.as_deref(), Some("7-day itinerary"));

        // Batch test: derive_all & get_all_resume_states
        let all_states = service.derive_all(&conn).unwrap();
        assert_eq!(all_states.len(), 3);
        let all_resumes = service.get_all_resume_states(&conn).unwrap();
        assert_eq!(all_resumes.len(), 3);

        let aurora_resume = all_resumes.iter().find(|r| r.context_id == ws_aurora.id).unwrap();
        assert_eq!(aurora_resume.context_name, "Aurora Website");
        assert_eq!(aurora_resume.left_off_on.as_deref(), Some("Hero animation"));

        let os_resume = all_resumes.iter().find(|r| r.context_id == ws_os.id).unwrap();
        assert_eq!(os_resume.context_name, "University OS");
        assert_eq!(os_resume.left_off_on.as_deref(), Some("Process scheduling assignment"));

        let tokyo_resume = all_resumes.iter().find(|r| r.context_id == ws_tokyo.id).unwrap();
        assert_eq!(tokyo_resume.context_name, "Tokyo Trip");
        assert_eq!(tokyo_resume.left_off_on.as_deref(), Some("7-day itinerary"));
    }

    #[test]
    fn test_state_freshness_classification_and_wording() {
        // 1. Fresh (< 2 hours)
        let fresh_secs_now = 60; // 1 min ago
        let f_now = StateFreshness::classify(fresh_secs_now);
        assert_eq!(f_now, StateFreshness::Fresh);
        assert_eq!(f_now.display_wording(fresh_secs_now), "Active now");

        let fresh_secs_mins = 15 * 60; // 15 mins ago
        let f_mins = StateFreshness::classify(fresh_secs_mins);
        assert_eq!(f_mins, StateFreshness::Fresh);
        assert_eq!(f_mins.display_wording(fresh_secs_mins), "Active 15m ago");

        let fresh_secs_recent = 90 * 60; // 1.5 hours ago
        let f_recent = StateFreshness::classify(fresh_secs_recent);
        assert_eq!(f_recent, StateFreshness::Fresh);
        assert_eq!(f_recent.display_wording(fresh_secs_recent), "Active recently");

        // 2. Recent (< 48 hours)
        let recent_secs_hours = 5 * 3600; // 5 hours ago
        let r_hours = StateFreshness::classify(recent_secs_hours);
        assert_eq!(r_hours, StateFreshness::Recent);
        assert_eq!(r_hours.display_wording(recent_secs_hours), "Last active 5 hours ago");

        let recent_secs_yesterday = 28 * 3600; // 28 hours ago
        let r_yesterday = StateFreshness::classify(recent_secs_yesterday);
        assert_eq!(r_yesterday, StateFreshness::Recent);
        assert_eq!(r_yesterday.display_wording(recent_secs_yesterday), "Last active yesterday");

        // 3. Stale (2 - 7 days)
        let stale_secs = 5 * 86400; // 5 days ago
        let s_days = StateFreshness::classify(stale_secs);
        assert_eq!(s_days, StateFreshness::Stale);
        assert_eq!(s_days.display_wording(stale_secs), "Last active 5 days ago");

        // 4. Very Stale (> 7 days)
        let very_stale_secs = 14 * 86400; // 14 days ago
        let vs_days = StateFreshness::classify(very_stale_secs);
        assert_eq!(vs_days, StateFreshness::VeryStale);
        assert_eq!(vs_days.display_wording(very_stale_secs), "Last active 14 days ago");
    }

    #[test]
    fn test_state_freshness_influences_readiness_and_workset_prioritization() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();

        let ws = crate::db::operations::create_workspace(&conn, "Aurora Website").unwrap();

        // Items created at different times
        let now = Utc::now();
        let time_fresh = now.to_rfc3339();
        let time_stale = (now - chrono::Duration::days(5)).to_rfc3339();
        let time_very_stale = (now - chrono::Duration::days(14)).to_rfc3339();

        let item_old = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://old-link.com",
            Some("Old initial reference"),
            r#"{"app_name":"Safari"}"#,
        ).unwrap();
        conn.execute("UPDATE items SET created_at = ?1, updated_at = ?1 WHERE id = ?2", rusqlite::params![time_very_stale, item_old.id]).unwrap();
        conn.execute("UPDATE context_memberships SET created_at = ?1, updated_at = ?1 WHERE item_id = ?2", rusqlite::params![time_very_stale, item_old.id]).unwrap();

        let item_medium = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "code",
            "const stale = true;",
            Some("Medium reference"),
            r#"{"app_name":"VS Code"}"#,
        ).unwrap();
        conn.execute("UPDATE items SET created_at = ?1, updated_at = ?1 WHERE id = ?2", rusqlite::params![time_stale, item_medium.id]).unwrap();
        conn.execute("UPDATE context_memberships SET created_at = ?1, updated_at = ?1 WHERE item_id = ?2", rusqlite::params![time_stale, item_medium.id]).unwrap();

        let item_fresh = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://framer.com/motion",
            Some("Latest Framer reference"),
            r#"{"app_name":"Chrome"}"#,
        ).unwrap();
        conn.execute("UPDATE items SET created_at = ?1, updated_at = ?1 WHERE id = ?2", rusqlite::params![time_fresh, item_fresh.id]).unwrap();
        conn.execute("UPDATE context_memberships SET created_at = ?1, updated_at = ?1 WHERE item_id = ?2", rusqlite::params![time_fresh, item_fresh.id]).unwrap();

        conn.execute(
            "INSERT INTO task_candidates (id, context_id, title, confidence, status, first_detected_at, last_active_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                uuid::Uuid::new_v4().to_string(),
                ws.id,
                "Hero animation",
                0.92,
                "active",
                time_fresh,
                time_fresh,
                time_fresh,
                time_fresh,
            ],
        ).unwrap();

        let service = ContextStateService::new();
        let state = service.derive_for_context(&conn, &ws.id).unwrap();

        // 1. Freshness classification
        assert_eq!(state.freshness, StateFreshness::Fresh);
        assert!(state.freshness_display.contains("Active"));

        // 2. Workset / Recency prioritization: Fresh item is prioritized over very stale item
        assert!(!state.recent_item_ids.is_empty());
        assert_eq!(state.recent_item_ids[0], item_fresh.id);

        // 3. Old state is NEVER deleted: All 3 items remain safely stored in DB
        let all_items = crate::db::operations::list_items(&conn, Some(&ws.id)).unwrap();
        assert_eq!(all_items.len(), 3);
        assert!(all_items.iter().any(|i| i.id == item_old.id));
        assert!(all_items.iter().any(|i| i.id == item_medium.id));
        assert!(all_items.iter().any(|i| i.id == item_fresh.id));

        // 4. Stale context test: Modify timestamp of workspace, task and latest items to 5 days ago
        conn.execute("UPDATE workspaces SET last_activity_at = ?1, updated_at = ?1 WHERE id = ?2", rusqlite::params![time_stale, ws.id]).unwrap();
        conn.execute("UPDATE task_candidates SET last_active_at = ?1, first_detected_at = ?1, created_at = ?1, updated_at = ?1 WHERE context_id = ?2", rusqlite::params![time_stale, ws.id]).unwrap();
        conn.execute("UPDATE items SET created_at = ?1, updated_at = ?1 WHERE id = ?2", rusqlite::params![time_stale, item_fresh.id]).unwrap();
        conn.execute("UPDATE context_memberships SET created_at = ?1, updated_at = ?1 WHERE item_id = ?2", rusqlite::params![time_stale, item_fresh.id]).unwrap();

        crate::context_engine::memory::invalidate_memory_cache(Some(&ws.id));
        let stale_state = service.derive_for_context(&conn, &ws.id).unwrap();
        assert_eq!(stale_state.freshness, StateFreshness::Stale);
        assert_eq!(stale_state.freshness_display, "Last active 5 days ago");
        assert_eq!(stale_state.resume_state.freshness, StateFreshness::Stale);
        assert_eq!(stale_state.resume_state.freshness_display, "Last active 5 days ago");

        // 5. Very stale context test: Modify timestamp of workspace, task and all items to 14 days ago
        conn.execute("UPDATE workspaces SET last_activity_at = ?1, updated_at = ?1 WHERE id = ?2", rusqlite::params![time_very_stale, ws.id]).unwrap();
        conn.execute("UPDATE task_candidates SET last_active_at = ?1, first_detected_at = ?1, created_at = ?1, updated_at = ?1 WHERE context_id = ?2", rusqlite::params![time_very_stale, ws.id]).unwrap();
        conn.execute("UPDATE items SET created_at = ?1, updated_at = ?1 WHERE workspace_id = ?2", rusqlite::params![time_very_stale, ws.id]).unwrap();
        conn.execute("UPDATE context_memberships SET created_at = ?1, updated_at = ?1 WHERE workspace_id = ?2", rusqlite::params![time_very_stale, ws.id]).unwrap();

        crate::context_engine::memory::invalidate_memory_cache(Some(&ws.id));
        let very_stale_state = service.derive_for_context(&conn, &ws.id).unwrap();
        assert_eq!(very_stale_state.freshness, StateFreshness::VeryStale);
        assert_eq!(very_stale_state.freshness_display, "Last active 14 days ago");
        assert_eq!(very_stale_state.readiness.readiness, ResumeReadiness::Partial);
        assert!(very_stale_state.readiness.reasons.iter().any(|r| r.contains("over a week")));
    }
}

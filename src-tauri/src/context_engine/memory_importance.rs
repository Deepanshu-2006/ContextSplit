use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

use crate::context_engine::memory::InformationProvenance;
use crate::context_engine::task_extractor::is_generic_task_name;
use crate::context_engine::workset::{
    is_accidental_capture, is_generic_noise, ContextWorksetBuilder, WorksetItemRelevance,
};

// =========================================================================
// MemoryImportanceTier: The 4 Core Tiers
// =========================================================================

/// Five-tier qualitative classification for contextual memory importance.
///
/// Invariants:
/// - System is strictly conservative.
/// - Recency alone NEVER grants `Critical` or `Important` status.
/// - Accidental captures and generic noise are filtered to `Temporary`.
/// - Temporary and Stale items are never deleted; only their memory importance is classified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryImportanceTier {
    /// Essential anchor (user-confirmed facts, active task with high streak, primary repo/design).
    Critical,
    /// High-signal asset (confirmed workspace item, recurring reference across sessions, primary workset item).
    Important,
    /// Supplemental/historical context (completed task, peripheral reference, older session item).
    Useful,
    /// Ephemeral/transient item (single glance, unconfirmed single lookup, low-attention capture).
    Temporary,
    /// Inactive, unreinforced item that has aged past decay thresholds without multi-session grounding.
    Stale,
}

impl MemoryImportanceTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::Important => "important",
            Self::Useful => "useful",
            Self::Temporary => "temporary",
            Self::Stale => "stale",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Critical => "Critical",
            Self::Important => "Important",
            Self::Useful => "Useful",
            Self::Temporary => "Temporary",
            Self::Stale => "Stale",
        }
    }

    pub fn is_retained(&self) -> bool {
        matches!(self, Self::Critical | Self::Important)
    }

    pub fn is_temporary(&self) -> bool {
        matches!(self, Self::Temporary)
    }

    pub fn is_useful(&self) -> bool {
        matches!(self, Self::Useful)
    }

    pub fn is_stale(&self) -> bool {
        matches!(self, Self::Stale)
    }
}

impl std::fmt::Display for MemoryImportanceTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// =========================================================================
// Dimension Scores for Transparent Calibration
// =========================================================================

/// Detailed score breakdown across the 10 memory importance dimensions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MemoryDimensionScores {
    /// 1. User confirmation (1.0 if confirmed by user or pinned, 0.0 otherwise)
    pub user_confirmation_score: f64,
    /// 2. Repeated relevance across evaluations
    pub repeated_relevance_score: f64,
    /// 3. Task relevance (alignment with active/recent task candidate)
    pub task_relevance_score: f64,
    /// 4. Context relevance (multi-factor workspace affinity)
    pub context_relevance_score: f64,
    /// 5. Workset membership score (Primary/Supporting/Reference tiering)
    pub workset_score: f64,
    /// 6. Recency score (bounded temporal proximity)
    pub recency_score: f64,
    /// 7. Relationship strength (graph connectivity in item_relationships)
    pub relationship_graph_score: f64,
    /// 8. Multi-session appearance score (presence in distinct working sessions)
    pub session_recurrence_score: f64,
    /// 9. User feedback score (explicit keep/pin boost vs dismiss penalty)
    pub user_feedback_score: f64,
    /// 10. Historical usefulness score (presence in snapshots/past milestones)
    pub historical_usefulness_score: f64,
    /// Evidence diversity score (number of independent corroborating signal types)
    pub evidence_diversity_score: f64,
    /// Noise penalty applied if generic or accidental characteristics detected
    pub noise_penalty: f64,
}

// =========================================================================
// Input Model for Evaluation
// =========================================================================

/// Comprehensive input structure capturing the 10 evidence dimensions for an entity.
#[derive(Debug, Clone)]
pub struct MemoryImportanceInput<'a> {
    pub entity_id: &'a str,
    pub title: Option<&'a str>,
    pub content: &'a str,
    pub item_type: &'a str,
    pub provenance: InformationProvenance,
    pub is_user_confirmed: bool,
    pub user_feedback: Option<&'a str>, // e.g. "keep", "dismiss", "pinned"
    pub task_relevance: f64,            // [0.0, 1.0]
    pub context_relevance: f64,         // [0.0, 1.0]
    pub workset_relevance: Option<WorksetItemRelevance>,
    pub session_count: usize,           // number of distinct sessions where observed
    pub relationship_degree: usize,     // number of item_relationships edges
    pub relationship_strength: f64,     // max/avg edge strength in graph
    pub first_seen_at: Option<&'a str>,
    pub last_seen_at: Option<&'a str>,
    pub is_active_task: bool,
    pub is_completed_task: bool,
    pub snapshot_presence_count: usize, // number of historical snapshots containing this
    pub signal_diversity_count: usize,  // distinct signal types (app, title, url, session)
}

impl<'a> MemoryImportanceInput<'a> {
    pub fn simple(
        entity_id: &'a str,
        title: Option<&'a str>,
        content: &'a str,
        item_type: &'a str,
    ) -> Self {
        Self {
            entity_id,
            title,
            content,
            item_type,
            provenance: InformationProvenance::Observed,
            is_user_confirmed: false,
            user_feedback: None,
            task_relevance: 0.0,
            context_relevance: 0.5,
            workset_relevance: None,
            session_count: 1,
            relationship_degree: 0,
            relationship_strength: 0.0,
            first_seen_at: None,
            last_seen_at: None,
            is_active_task: false,
            is_completed_task: false,
            snapshot_presence_count: 0,
            signal_diversity_count: 1,
        }
    }
}

// =========================================================================
// Evaluation Result
// =========================================================================

/// Complete evaluation result determining contextual importance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryImportanceEvaluation {
    /// Entity ID being evaluated
    pub entity_id: String,
    /// Calibrated importance score [0.0, 1.0]
    pub score: f64,
    /// Qualitative tier (Critical, Important, Useful, Temporary)
    pub tier: MemoryImportanceTier,
    /// Confidence in this evaluation [0.0, 1.0]
    pub confidence: f64,
    /// Dimension scores breakdown
    pub dimension_scores: MemoryDimensionScores,
    /// Explainable reasons answering why this tier was assigned
    pub why_reasons: Vec<String>,
    /// Whether this information should be retained in long-lived memory
    pub is_retained_as_important: bool,
}

// =========================================================================
// ContextMemoryImportanceEvaluator
// =========================================================================

/// Evaluator engine for determining contextual memory importance.
///
/// Implements the 10-dimension evaluation with conservative guardrails:
/// 1. User confirmation
/// 2. Repeated relevance
/// 3. Task relevance
/// 4. Context relevance
/// 5. Workset membership
/// 6. Recency
/// 7. Relationship strength
/// 8. Repeated appearance across sessions
/// 9. User feedback
/// 10. Historical usefulness
pub struct ContextMemoryImportanceEvaluator;

impl ContextMemoryImportanceEvaluator {
    /// Evaluates memory importance for an input specification.
    pub fn evaluate(input: &MemoryImportanceInput) -> MemoryImportanceEvaluation {
        let mut why_reasons = Vec::new();
        let mut scores = MemoryDimensionScores::default();

        // 1. Check for Accidental Capture (Hard Filter)
        if is_accidental_capture(
            input.item_type,
            input.title,
            input.content,
            input.is_user_confirmed,
        ) {
            scores.noise_penalty = 0.90;
            why_reasons.push("Accidental or empty capture filtered to temporary state.".to_string());
            return MemoryImportanceEvaluation {
                entity_id: input.entity_id.to_string(),
                score: 0.05,
                tier: MemoryImportanceTier::Temporary,
                confidence: 0.95,
                dimension_scores: scores,
                why_reasons,
                is_retained_as_important: false,
            };
        }

        // 2. Check for Generic Noise (Hard Filter unless user confirmed or task grounded)
        let is_task_grounded = input.task_relevance >= 0.60 || input.is_active_task;
        if is_generic_noise(
            input.title,
            input.content,
            is_task_grounded,
            input.is_user_confirmed,
        ) {
            scores.noise_penalty = 0.80;
            why_reasons.push("Generic noise without specific context or task grounding.".to_string());
            return MemoryImportanceEvaluation {
                entity_id: input.entity_id.to_string(),
                score: 0.15,
                tier: MemoryImportanceTier::Temporary,
                confidence: 0.90,
                dimension_scores: scores,
                why_reasons,
                is_retained_as_important: false,
            };
        }

        // -------------------------------------------------------------
        // Dimension 1: User Confirmation & Provenance
        // -------------------------------------------------------------
        scores.user_confirmation_score = if input.is_user_confirmed
            || input.provenance == InformationProvenance::Confirmed
        {
            why_reasons.push("Explicitly confirmed by user.".to_string());
            1.0
        } else {
            0.0
        };

        // -------------------------------------------------------------
        // Dimension 2: Repeated Relevance
        // -------------------------------------------------------------
        scores.repeated_relevance_score = match input.workset_relevance {
            Some(WorksetItemRelevance::Primary) => {
                why_reasons.push("Primary working set item.".to_string());
                0.90
            }
            Some(WorksetItemRelevance::Supporting) => {
                why_reasons.push("Supporting working set asset.".to_string());
                0.75
            }
            Some(WorksetItemRelevance::Reference) => {
                why_reasons.push("Core context reference anchor.".to_string());
                0.85
            }
            Some(WorksetItemRelevance::Recent) => 0.45,
            Some(WorksetItemRelevance::Related) => 0.40,
            None => 0.20,
        };

        // -------------------------------------------------------------
        // Dimension 3: Task Relevance
        // -------------------------------------------------------------
        scores.task_relevance_score = if input.is_active_task {
            if input.task_relevance >= 0.50 {
                why_reasons.push("Current active task guiding focus.".to_string());
                input.task_relevance.max(0.75).clamp(0.0, 1.0)
            } else {
                why_reasons.push("Low-confidence task candidate.".to_string());
                input.task_relevance.clamp(0.0, 1.0)
            }
        } else if input.is_completed_task {
            why_reasons.push("Completed task with historical reference value.".to_string());
            0.50
        } else {
            input.task_relevance.clamp(0.0, 1.0)
        };

        // -------------------------------------------------------------
        // Dimension 4: Context Relevance
        // -------------------------------------------------------------
        scores.context_relevance_score = input.context_relevance.clamp(0.0, 1.0);

        // -------------------------------------------------------------
        // Dimension 5: Workset Membership
        // -------------------------------------------------------------
        scores.workset_score = match input.workset_relevance {
            Some(WorksetItemRelevance::Primary) => 1.0,
            Some(WorksetItemRelevance::Reference) => 0.90,
            Some(WorksetItemRelevance::Supporting) => 0.75,
            Some(WorksetItemRelevance::Recent) => 0.50,
            Some(WorksetItemRelevance::Related) => 0.40,
            None => 0.10,
        };

        // -------------------------------------------------------------
        // Dimension 6: Recency (Guardrail: Recency alone != permanence)
        // -------------------------------------------------------------
        scores.recency_score = if let Some(last_ts) = input.last_seen_at {
            compute_recency_score(last_ts)
        } else {
            0.50
        };

        // -------------------------------------------------------------
        // Dimension 7: Relationship Strength
        // -------------------------------------------------------------
        scores.relationship_graph_score = if input.relationship_degree >= 3 {
            why_reasons.push(format!(
                "Highly connected anchor node ({} semantic relationships).",
                input.relationship_degree
            ));
            0.90
        } else if input.relationship_degree == 2 {
            0.70
        } else if input.relationship_degree == 1 {
            0.45
        } else {
            0.15
        };

        // -------------------------------------------------------------
        // Dimension 8: Repeated Appearance Across Sessions
        // -------------------------------------------------------------
        scores.session_recurrence_score = if input.session_count >= 3 {
            why_reasons.push(format!(
                "Consistently referenced across {} distinct working sessions.",
                input.session_count
            ));
            1.0
        } else if input.session_count == 2 {
            why_reasons.push("Accessed across 2 distinct working sessions.".to_string());
            0.75
        } else {
            0.20
        };

        // -------------------------------------------------------------
        // Dimension 9: User Feedback
        // -------------------------------------------------------------
        scores.user_feedback_score = match input.user_feedback {
            Some("pinned") => {
                why_reasons.push("Pinned by user as key contextual memory.".to_string());
                1.0
            }
            Some("keep") => {
                why_reasons.push("Confirmed by user feedback.".to_string());
                0.90
            }
            Some("dismiss") | Some("remove") | Some("unpinned") | Some("forget") | Some("rejected") => {
                why_reasons.push("Dismissed or unpinned by user feedback.".to_string());
                0.0
            }
            _ => 0.50,
        };

        // -------------------------------------------------------------
        // Dimension 10: Historical Usefulness
        // -------------------------------------------------------------
        scores.historical_usefulness_score = if input.snapshot_presence_count >= 3 {
            why_reasons.push(format!(
                "Preserved across {} historical snapshots.",
                input.snapshot_presence_count
            ));
            0.85
        } else if input.snapshot_presence_count >= 1 {
            0.55
        } else {
            0.20
        };

        // -------------------------------------------------------------
        // Evidence Diversity Score
        // -------------------------------------------------------------
        scores.evidence_diversity_score = if input.signal_diversity_count >= 3 {
            1.0
        } else if input.signal_diversity_count == 2 {
            0.70
        } else {
            0.40
        };

        // -------------------------------------------------------------
        // Calibrated Composite Weighting
        // -------------------------------------------------------------
        let mut base_score: f64;

        if input.is_completed_task {
            // Completed tasks settle into useful/historical tier
            base_score = 0.40
                + (scores.historical_usefulness_score * 0.15)
                + (scores.session_recurrence_score * 0.10);
        } else if input.is_user_confirmed || input.user_feedback == Some("pinned") {
            // User-confirmed items start with high baseline
            base_score = 0.70
                + (scores.task_relevance_score * 0.10)
                + (scores.session_recurrence_score * 0.10)
                + (scores.relationship_graph_score * 0.10);
        } else if input.is_active_task && input.task_relevance >= 0.50 {
            // Active tasks with solid relevance are critical/important while active
            base_score = 0.65
                + (scores.task_relevance_score * 0.15)
                + (scores.evidence_diversity_score * 0.10)
                + (scores.recency_score * 0.10);
        } else {
            // Standard Inferred / Observed Items
            // Conservative weighting: Multi-session recurrence & Graph centrality matter much more than recency alone
            base_score = (scores.session_recurrence_score * 0.25)
                + (scores.relationship_graph_score * 0.20)
                + (scores.workset_score * 0.20)
                + (scores.task_relevance_score * 0.15)
                + (scores.context_relevance_score * 0.10)
                + (scores.recency_score * 0.10);

            // Guardrail against one-off recent lookups becoming permanent
            if input.session_count <= 1
                && input.relationship_degree == 0
                && input.workset_relevance != Some(WorksetItemRelevance::Primary)
            {
                // Single-session, isolated, non-primary item is capped at useful/temporary
                base_score = base_score.min(0.55);
            }
        }

        // Apply user dismissal penalty if present
        if input.user_feedback == Some("dismiss")
            || input.user_feedback == Some("remove")
            || input.user_feedback == Some("unpinned")
            || input.user_feedback == Some("forget")
            || input.user_feedback == Some("rejected")
        {
            base_score *= 0.30;
        }

        let final_score = base_score.clamp(0.0, 1.0);

        // -------------------------------------------------------------
        // Tier Determination
        // -------------------------------------------------------------
        let initial_tier = if final_score >= 0.85
            && (input.is_user_confirmed
                || input.is_active_task
                || input.session_count >= 3
                || input.workset_relevance == Some(WorksetItemRelevance::Primary))
        {
            MemoryImportanceTier::Critical
        } else if final_score >= 0.65 {
            MemoryImportanceTier::Important
        } else if final_score >= 0.40 {
            MemoryImportanceTier::Useful
        } else {
            MemoryImportanceTier::Temporary
        };

        let confidence = if input.is_user_confirmed {
            1.0
        } else {
            (scores.evidence_diversity_score * 0.50 + final_score * 0.50).clamp(0.40, 0.95)
        };

        // -------------------------------------------------------------
        // Quality Guard Verification
        // -------------------------------------------------------------
        let preliminary_eval = MemoryImportanceEvaluation {
            entity_id: input.entity_id.to_string(),
            score: final_score,
            tier: initial_tier,
            confidence,
            dimension_scores: scores.clone(),
            why_reasons: why_reasons.clone(),
            is_retained_as_important: initial_tier.is_retained(),
        };

        let guard_result = ContextMemoryQualityGuard::verify_candidate(input, &preliminary_eval);

        let (final_tier, is_retained_as_important) = if !guard_result.is_accepted {
            for reason in &guard_result.failure_reasons {
                why_reasons.push(format!("Quality Guard: {}", reason));
            }
            // Downgrade to Temporary/Useful if guard fails
            let downgraded = if initial_tier == MemoryImportanceTier::Critical
                || initial_tier == MemoryImportanceTier::Important
            {
                MemoryImportanceTier::Temporary
            } else {
                initial_tier
            };
            (downgraded, false)
        } else {
            (initial_tier, initial_tier.is_retained())
        };

        MemoryImportanceEvaluation {
            entity_id: input.entity_id.to_string(),
            score: final_score,
            tier: final_tier,
            confidence,
            dimension_scores: scores,
            why_reasons,
            is_retained_as_important,
        }
    }

    /// Evaluates an existing item in the database for a workspace.
    pub fn evaluate_item(
        conn: &Connection,
        workspace_id: &str,
        item_id: &str,
    ) -> Result<MemoryImportanceEvaluation> {
        // Fetch item details
        let item_res: Result<(String, Option<String>, String, String, String), _> = conn.query_row(
            "SELECT item_type, title, content, metadata, created_at FROM items WHERE id = ?1",
            params![item_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        );

        let (item_type, title_opt, content, meta_str, created_at) = match item_res {
            Ok(data) => data,
            Err(_) => {
                return Ok(MemoryImportanceEvaluation {
                    entity_id: item_id.to_string(),
                    score: 0.0,
                    tier: MemoryImportanceTier::Temporary,
                    confidence: 0.0,
                    dimension_scores: MemoryDimensionScores::default(),
                    why_reasons: vec!["Item not found in database.".to_string()],
                    is_retained_as_important: false,
                });
            }
        };

        // Check user confirmation from context_memberships & workspace_memberships
        let mut is_confirmed: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM context_memberships WHERE item_id = ?1 AND workspace_id = ?2 AND membership_type = 'confirmed'",
                params![item_id, workspace_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0) > 0;

        if !is_confirmed {
            is_confirmed = conn
                .query_row(
                    "SELECT COUNT(*) FROM workspace_memberships WHERE item_id = ?1 AND workspace_id = ?2 AND (user_confirmed = 1 OR pinned = 1)",
                    params![item_id, workspace_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0) > 0;
        }

        let parsed_meta: Value = serde_json::from_str(&meta_str).unwrap_or(json!({}));
        let mut user_feedback_owned = parsed_meta.get("user_feedback").and_then(|v| v.as_str()).map(String::from);

        // Resolve effective feedback state from persistent signals
        if let Ok(res) = crate::context_engine::feedback::FeedbackConflictResolver::resolve_from_db(conn, item_id, workspace_id) {
            match res.effective_state {
                crate::context_engine::feedback::EffectiveFeedbackState::Confirmed { .. } => {
                    is_confirmed = true;
                    user_feedback_owned = Some("pinned".to_string());
                }
                crate::context_engine::feedback::EffectiveFeedbackState::Rejected { .. } => {
                    is_confirmed = false;
                    if res.latest_action == Some(crate::context_engine::feedback::FeedbackAction::RemoveImportance) {
                        user_feedback_owned = Some("unpinned".to_string());
                    } else {
                        user_feedback_owned = Some("remove".to_string());
                    }
                }
                _ => {}
            }
        }

        // Count distinct sessions
        let session_count: usize = conn
            .query_row(
                "SELECT COUNT(DISTINCT s.id) FROM items i
                 JOIN sessions s ON (i.session_id = s.id OR i.created_at BETWEEN s.started_at AND s.last_activity_at)
                 WHERE i.id = ?1",
                params![item_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0) as usize;

        // Count graph relationships
        let relationship_degree: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM item_relationships WHERE item_a_id = ?1 OR item_b_id = ?1",
                params![item_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0) as usize;

        // Count snapshot presences
        let snapshot_presence_count: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM context_snapshots WHERE context_id = ?1 AND snapshot_json LIKE ?2",
                params![workspace_id, format!("%{}%", item_id)],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0) as usize;

        // Derive workset tier if available
        let workset_relevance = if let Ok(workset_builder) = ContextWorksetBuilder::from_db(conn, workspace_id) {
            if let Ok(workset) = workset_builder.build() {
                workset.items.iter().find(|i| i.id == item_id).map(|i| i.relevance)
            } else {
                None
            }
        } else {
            None
        };

        let input = MemoryImportanceInput {
            entity_id: item_id,
            title: title_opt.as_deref(),
            content: &content,
            item_type: &item_type,
            provenance: if is_confirmed {
                InformationProvenance::Confirmed
            } else {
                InformationProvenance::Observed
            },
            is_user_confirmed: is_confirmed,
            user_feedback: user_feedback_owned.as_deref(),
            task_relevance: if workset_relevance == Some(WorksetItemRelevance::Primary) { 0.85 } else { 0.50 },
            context_relevance: 0.70,
            workset_relevance,
            session_count: session_count.max(1),
            relationship_degree,
            relationship_strength: if relationship_degree > 0 { 0.80 } else { 0.0 },
            first_seen_at: Some(&created_at),
            last_seen_at: Some(&created_at),
            is_active_task: false,
            is_completed_task: false,
            snapshot_presence_count,
            signal_diversity_count: if relationship_degree > 0 { 3 } else { 2 },
        };

        Ok(Self::evaluate(&input))
    }
}

fn compute_recency_score(timestamp: &str) -> f64 {
    let now = Utc::now();
    let dt = if let Ok(parsed) = DateTime::parse_from_rfc3339(timestamp) {
        parsed.with_timezone(&Utc)
    } else if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S") {
        DateTime::from_naive_utc_and_offset(naive, Utc)
    } else {
        return 0.50;
    };

    let elapsed = now.signed_duration_since(dt).num_seconds().max(0);
    let hours = elapsed as f64 / 3600.0;

    if hours <= 2.0 {
        1.0
    } else if hours <= 24.0 {
        0.85
    } else if hours <= 72.0 {
        0.70
    } else if hours <= 168.0 {
        0.50
    } else {
        0.30
    }
}

// =========================================================================
// ContextMemoryQualityGuard (Step 10)
// =========================================================================

/// Returns true if an entity's title or content is solely an application name
/// without specific document, file, URL, or project context.
pub fn is_app_name_only(title: Option<&str>, content: &str) -> bool {
    let t = title.unwrap_or("").trim().to_lowercase();
    let c = content.trim().to_lowercase();

    let app_names = [
        "google chrome", "chrome", "safari", "firefox", "arc", "brave", "edge",
        "visual studio code", "vs code", "vscode", "code", "cursor", "sublime text",
        "sublime", "xcode", "intellij", "webstorm", "pycharm", "clion",
        "terminal", "iterm", "iterm2", "alacritty", "kitty", "warp", "bash", "zsh", "fish",
        "slack", "discord", "zoom", "microsoft teams", "teams", "messages", "telegram", "whatsapp",
        "figma", "notion", "notes", "apple notes", "obsidian", "linear", "jira",
        "finder", "preview", "system settings", "settings", "calculator", "mail", "outlook",
    ];

    let t_is_app = app_names.iter().any(|&a| t == a);
    let c_is_app = app_names.iter().any(|&a| c == a);

    (t_is_app && (c.is_empty() || c_is_app)) || (c_is_app && (t.is_empty() || t_is_app))
}

/// Returns true if a URL is an unrelated generic homepage or popular domain lookup
/// without dedicated repository or contextual depth.
pub fn is_unrelated_popular_domain(content: &str, title: Option<&str>) -> bool {
    let c = content.trim().to_lowercase();
    let t = title.unwrap_or("").trim().to_lowercase();

    let generic_domains = [
        "https://google.com", "https://google.com/", "https://www.google.com", "https://www.google.com/",
        "http://google.com", "http://google.com/", "http://www.google.com", "http://www.google.com/",
        "https://youtube.com", "https://youtube.com/", "https://www.youtube.com", "https://www.youtube.com/",
        "https://reddit.com", "https://reddit.com/", "https://www.reddit.com", "https://www.reddit.com/",
        "https://twitter.com", "https://twitter.com/", "https://www.twitter.com", "https://www.twitter.com/",
        "https://x.com", "https://x.com/", "https://www.x.com", "https://www.x.com/",
        "https://bing.com", "https://bing.com/", "https://www.bing.com", "https://www.bing.com/",
        "https://duckduckgo.com", "https://duckduckgo.com/", "https://www.duckduckgo.com", "https://www.duckduckgo.com/",
        "https://netflix.com", "https://netflix.com/", "https://www.netflix.com", "https://www.netflix.com/",
        "https://spotify.com", "https://spotify.com/", "https://open.spotify.com", "https://open.spotify.com/",
        "https://wikipedia.org", "https://wikipedia.org/", "https://en.wikipedia.org", "https://en.wikipedia.org/",
        "https://instagram.com", "https://instagram.com/", "https://facebook.com", "https://facebook.com/",
        "https://tiktok.com", "https://tiktok.com/", "https://twitch.tv", "https://twitch.tv/",
    ];

    generic_domains.iter().any(|&d| c == d)
        || c.starts_with("https://www.google.com/search")
        || c.starts_with("https://google.com/search")
        || c.starts_with("https://duckduckgo.com/?q=")
        || c.starts_with("https://www.bing.com/search")
        || (t.starts_with("google search") || t.starts_with("search - "))
}

/// Normalizes memory title and details into a canonical key to prevent duplicate memories.
pub fn normalize_memory_key(title: &str, detail: Option<&str>) -> String {
    let mut t = title.trim().to_lowercase();
    if let Some(stripped) = t.strip_prefix("workspace: ") {
        t = stripped.to_string();
    } else if let Some(stripped) = t.strip_prefix("state: ") {
        t = stripped.to_string();
    } else if let Some(stripped) = t.strip_prefix("active task: ") {
        t = stripped.to_string();
    } else if let Some(stripped) = t.strip_prefix("recent task: ") {
        t = stripped.to_string();
    } else if let Some(stripped) = t.strip_prefix("task: ") {
        t = stripped.to_string();
    }

    if t.starts_with("http://") || t.starts_with("https://") {
        let clean = t.strip_prefix("https://").or_else(|| t.strip_prefix("http://")).unwrap_or(&t);
        let clean_no_www = clean.strip_prefix("www.").unwrap_or(clean);
        let base_url = clean_no_www.split('?').next().unwrap_or(clean_no_www);
        return format!("url:{}", base_url.trim_end_matches('/'));
    }

    if let Some(d) = detail {
        let d_trim = d.trim().to_lowercase();
        if d_trim.starts_with("http://") || d_trim.starts_with("https://") {
            let clean = d_trim.strip_prefix("https://").or_else(|| d_trim.strip_prefix("http://")).unwrap_or(&d_trim);
            let clean_no_www = clean.strip_prefix("www.").unwrap_or(clean);
            let base_url = clean_no_www.split('?').next().unwrap_or(clean_no_www);
            return format!("url:{}", base_url.trim_end_matches('/'));
        }
    }

    format!("title:{}", t)
}

/// Evaluation verdict produced by the `ContextMemoryQualityGuard`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryQualityGuardResult {
    /// Whether the candidate passed all quality checks
    pub is_accepted: bool,
    /// Detailed failure reasons if rejected
    pub failure_reasons: Vec<String>,
    /// Quality score [0.0, 1.0]
    pub quality_score: f64,
}

impl MemoryQualityGuardResult {
    pub fn accepted(score: f64) -> Self {
        Self {
            is_accepted: true,
            failure_reasons: Vec::new(),
            quality_score: score.clamp(0.0, 1.0),
        }
    }

    pub fn rejected(reasons: Vec<String>) -> Self {
        Self {
            is_accepted: false,
            failure_reasons: reasons,
            quality_score: 0.0,
        }
    }
}

/// Final guardrail engine verifying contextual memories before they become retained as important memory.
///
/// Core principle:
/// Prefer **"No important memory"** over **"Confidently remember something wrong."**
///
/// Verifies the 9 Core Quality Invariants:
/// 1. Evidence exists.
/// 2. Evidence is sufficiently diverse.
/// 3. The information is relevant to the context.
/// 4. It is not contradicted by stronger evidence.
/// 5. It is not simply a duplicate.
/// 6. It is not obviously generic.
/// 7. It is not solely based on application name.
/// 8. It is not solely based on one weak semantic match.
/// 9. It has not been explicitly rejected by the user.
pub struct ContextMemoryQualityGuard;

impl ContextMemoryQualityGuard {
    /// Evaluates an inferred candidate memory input and its preliminary score against all 9 quality invariants.
    pub fn verify_candidate(
        input: &MemoryImportanceInput,
        evaluation: &MemoryImportanceEvaluation,
    ) -> MemoryQualityGuardResult {
        let mut failure_reasons = Vec::new();

        // 9. Check: Not explicitly rejected or unpinned by the user
        if input.user_feedback == Some("dismiss")
            || input.user_feedback == Some("remove")
            || input.user_feedback == Some("forget")
            || input.user_feedback == Some("rejected")
            || input.user_feedback == Some("unpinned")
        {
            failure_reasons.push("Explicitly rejected or dismissed by user feedback.".to_string());
            return MemoryQualityGuardResult::rejected(failure_reasons);
        }

        // User confirmed memories always pass with maximum score
        if input.is_user_confirmed || input.provenance == InformationProvenance::Confirmed {
            return MemoryQualityGuardResult::accepted(1.0);
        }

        // 1. Check: Evidence exists
        let title_clean = input.title.unwrap_or("").trim();
        let content_clean = input.content.trim();
        if title_clean.is_empty() && content_clean.is_empty() {
            failure_reasons.push("Missing evidence: entity has neither title nor content.".to_string());
        }

        // 6. Check: Not obviously generic
        if is_generic_noise(input.title, input.content, input.is_active_task, input.is_user_confirmed)
            || is_generic_task_name(title_clean)
        {
            failure_reasons.push("Entity is generic noise or a placeholder without specific context.".to_string());
        }

        // 7. Check: Not solely based on application name
        if is_app_name_only(input.title, input.content) {
            failure_reasons.push("Entity is solely an application name without project or document grounding.".to_string());
        }

        // Check: Unrelated popular domain without multi-session grounding
        if is_unrelated_popular_domain(input.content, input.title) && input.session_count <= 2 {
            failure_reasons.push("Generic popular domain lookup without specific context relevance.".to_string());
        }

        // 2. Check: Evidence is sufficiently diverse
        if input.signal_diversity_count < 2
            && input.session_count <= 1
            && input.relationship_degree == 0
            && input.workset_relevance != Some(WorksetItemRelevance::Primary)
            && !input.is_active_task
        {
            failure_reasons.push("Insufficient evidence diversity: single isolated capture without multi-session or relationship corroboration.".to_string());
        }

        // 3. Check: The information is relevant to the context
        if input.context_relevance < 0.35 && input.workset_relevance.is_none() && !input.is_active_task {
            failure_reasons.push(format!("Insufficient context relevance score ({:.2} < 0.35).", input.context_relevance));
        }

        // 8. Check: Not solely based on one weak semantic match
        if input.task_relevance > 0.0
            && input.task_relevance < 0.60
            && input.relationship_degree == 0
            && input.session_count <= 1
            && input.signal_diversity_count <= 1
            && input.workset_relevance != Some(WorksetItemRelevance::Primary)
        {
            failure_reasons.push("Solely based on a weak, isolated semantic match without temporal or relational support.".to_string());
        }

        // 4. Check: Not contradicted by stronger evidence or low task confidence
        if input.is_active_task && (input.task_relevance < 0.50 || evaluation.confidence < 0.50) {
            failure_reasons.push("Task candidate has insufficient confidence (< 0.50) to be considered active context memory.".to_string());
        }

        if failure_reasons.is_empty() {
            let quality_score = (evaluation.score * 0.50 + evaluation.confidence * 0.50).clamp(0.0, 1.0);
            MemoryQualityGuardResult::accepted(quality_score)
        } else {
            MemoryQualityGuardResult::rejected(failure_reasons)
        }
    }

    /// Evaluates a `ContextImportantMemory` against quality guard invariants.
    pub fn verify_memory(
        memory: &crate::context_engine::important_memory::ContextImportantMemory,
    ) -> MemoryQualityGuardResult {
        // User confirmed memories always pass
        if memory.provenance == InformationProvenance::Confirmed {
            return MemoryQualityGuardResult::accepted(memory.importance);
        }

        let mut failure_reasons = Vec::new();

        // 1. Evidence exists
        if memory.title.trim().is_empty() {
            failure_reasons.push("Missing memory title.".to_string());
        }

        // 6. Generic noise
        let snippet = memory.detail.as_deref().unwrap_or("");
        if is_generic_noise(
            Some(&memory.title),
            snippet,
            memory.memory_type == crate::context_engine::important_memory::ImportantMemoryType::ActiveTask,
            false,
        ) || is_generic_task_name(&memory.title)
        {
            failure_reasons.push("Generic placeholder or noise.".to_string());
        }

        // 7. Solely app name
        if is_app_name_only(Some(&memory.title), snippet) {
            failure_reasons.push("Solely an application name.".to_string());
        }

        // Unrelated popular domain
        if is_unrelated_popular_domain(snippet, Some(&memory.title))
            && memory.metadata.session_count.unwrap_or(1) <= 2
        {
            failure_reasons.push("Unrelated popular domain lookup.".to_string());
        }

        // 8. Weak semantic match without corroboration
        if memory.confidence < 0.45 && memory.importance < 0.60 {
            failure_reasons.push("Insufficient confidence and importance.".to_string());
        }

        // Low importance threshold
        if memory.importance < 0.50 {
            failure_reasons.push("Importance score below 0.50 minimum guard threshold.".to_string());
        }

        if failure_reasons.is_empty() {
            MemoryQualityGuardResult::accepted(memory.importance)
        } else {
            MemoryQualityGuardResult::rejected(failure_reasons)
        }
    }

    /// Filters candidate memories, ensuring only guard-verified memories are retained.
    pub fn filter_memories(
        memories: Vec<crate::context_engine::important_memory::ContextImportantMemory>,
    ) -> Vec<crate::context_engine::important_memory::ContextImportantMemory> {
        let mut seen_keys = HashSet::new();
        let mut seen_source_ids = HashSet::new();
        let mut filtered = Vec::new();

        for mem in memories {
            // 5. Check: Canonical duplicate prevention
            let canonical_key = normalize_memory_key(&mem.title, mem.detail.as_deref());
            if seen_keys.contains(&canonical_key) || seen_source_ids.contains(&mem.source_id) {
                continue;
            }

            let verdict = Self::verify_memory(&mem);
            if verdict.is_accepted {
                seen_keys.insert(canonical_key);
                seen_source_ids.insert(mem.source_id.clone());
                filtered.push(mem);
            }
        }

        // Budget capping: "FEWER BUT MORE USEFUL MEMORIES"
        // Retain the top high-value contextual memories (max 12)
        filtered.sort_by(|a, b| {
            b.importance
                .partial_cmp(&a.importance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if filtered.len() > 12 {
            filtered.truncate(12);
        }

        filtered
    }
}

// =========================================================================
// Unit Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init;

    fn setup_test_db() -> (Connection, String) {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        let ws_id = "test-ws-tokyo";

        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at) VALUES (?1, 'Tokyo Trip', '2026-09-18T00:00:00Z', '2026-09-18T00:00:00Z')",
            params![ws_id],
        ).unwrap();

        (conn, ws_id.to_string())
    }

    #[test]
    fn test_scenario_tokyo_7day_itinerary_important() {
        // Tokyo 7-day itinerary: highly relevant, multi-session, structured document
        let input = MemoryImportanceInput {
            entity_id: "doc-tokyo-itinerary",
            title: Some("Tokyo 7-day itinerary"),
            content: "Day 1: Shinjuku, Day 2: Shibuya & Harajuku, Day 3: Asakusa & Akihabara...",
            item_type: "text",
            provenance: InformationProvenance::Observed,
            is_user_confirmed: false,
            user_feedback: None,
            task_relevance: 0.85,
            context_relevance: 0.90,
            workset_relevance: Some(WorksetItemRelevance::Primary),
            session_count: 3,
            relationship_degree: 4,
            relationship_strength: 0.85,
            first_seen_at: Some("2026-09-15T10:00:00Z"),
            last_seen_at: Some("2026-09-18T00:00:00Z"),
            is_active_task: false,
            is_completed_task: false,
            snapshot_presence_count: 2,
            signal_diversity_count: 4,
        };

        let result = ContextMemoryImportanceEvaluator::evaluate(&input);
        assert!(result.is_retained_as_important);
        assert!(matches!(
            result.tier,
            MemoryImportanceTier::Important | MemoryImportanceTier::Critical
        ));
        assert!(result.score >= 0.70);
    }

    #[test]
    fn test_scenario_random_tokyo_restaurant_link_temporary() {
        // Random Tokyo restaurant link captured once: single session, 0 relationships, unconfirmed
        let input = MemoryImportanceInput {
            entity_id: "url-ramen-shop",
            title: Some("Ramen Yashichi - Shinjuku"),
            content: "https://tabelog.com/tokyo/A1304/A130401/13000000/",
            item_type: "url",
            provenance: InformationProvenance::Observed,
            is_user_confirmed: false,
            user_feedback: None,
            task_relevance: 0.30,
            context_relevance: 0.45,
            workset_relevance: Some(WorksetItemRelevance::Recent),
            session_count: 1,
            relationship_degree: 0,
            relationship_strength: 0.0,
            first_seen_at: Some("2026-09-18T00:00:00Z"),
            last_seen_at: Some("2026-09-18T00:00:00Z"),
            is_active_task: false,
            is_completed_task: false,
            snapshot_presence_count: 0,
            signal_diversity_count: 1,
        };

        let result = ContextMemoryImportanceEvaluator::evaluate(&input);
        assert!(!result.is_retained_as_important);
        assert!(matches!(
            result.tier,
            MemoryImportanceTier::Temporary | MemoryImportanceTier::Useful
        ));
        assert!(result.score < 0.60);
    }

    #[test]
    fn test_scenario_confirmed_project_repository_important() {
        // Confirmed project repository: user confirmed, reference asset
        let input = MemoryImportanceInput {
            entity_id: "repo-aurora",
            title: Some("aurora-frontend repository"),
            content: "https://github.com/company/aurora-frontend",
            item_type: "url",
            provenance: InformationProvenance::Confirmed,
            is_user_confirmed: true,
            user_feedback: Some("pinned"),
            task_relevance: 0.80,
            context_relevance: 1.0,
            workset_relevance: Some(WorksetItemRelevance::Reference),
            session_count: 4,
            relationship_degree: 5,
            relationship_strength: 0.90,
            first_seen_at: Some("2026-09-10T00:00:00Z"),
            last_seen_at: Some("2026-09-18T00:00:00Z"),
            is_active_task: false,
            is_completed_task: false,
            snapshot_presence_count: 5,
            signal_diversity_count: 4,
        };

        let result = ContextMemoryImportanceEvaluator::evaluate(&input);
        assert!(result.is_retained_as_important);
        assert_eq!(result.tier, MemoryImportanceTier::Critical);
        assert!(result.score >= 0.85);
    }

    #[test]
    fn test_scenario_random_unrelated_image_temporary() {
        // Random unrelated image: no title, generic, 0 graph links
        let input = MemoryImportanceInput {
            entity_id: "img-random",
            title: None,
            content: "base64-random-snippet",
            item_type: "image",
            provenance: InformationProvenance::Observed,
            is_user_confirmed: false,
            user_feedback: None,
            task_relevance: 0.05,
            context_relevance: 0.10,
            workset_relevance: None,
            session_count: 1,
            relationship_degree: 0,
            relationship_strength: 0.0,
            first_seen_at: Some("2026-09-18T00:00:00Z"),
            last_seen_at: Some("2026-09-18T00:00:00Z"),
            is_active_task: false,
            is_completed_task: false,
            snapshot_presence_count: 0,
            signal_diversity_count: 1,
        };

        let result = ContextMemoryImportanceEvaluator::evaluate(&input);
        assert_eq!(result.tier, MemoryImportanceTier::Temporary);
        assert!(!result.is_retained_as_important);
        assert!(result.score < 0.30);
    }

    #[test]
    fn test_scenario_current_active_task_important() {
        // Current active task
        let input = MemoryImportanceInput {
            entity_id: "task-hero-anim",
            title: Some("Hero animation"),
            content: "Designing hero section animation in React and Framer Motion",
            item_type: "task",
            provenance: InformationProvenance::Inferred,
            is_user_confirmed: false,
            user_feedback: None,
            task_relevance: 0.95,
            context_relevance: 0.90,
            workset_relevance: Some(WorksetItemRelevance::Primary),
            session_count: 2,
            relationship_degree: 3,
            relationship_strength: 0.85,
            first_seen_at: Some("2026-09-18T00:00:00Z"),
            last_seen_at: Some("2026-09-18T01:00:00Z"),
            is_active_task: true,
            is_completed_task: false,
            snapshot_presence_count: 1,
            signal_diversity_count: 3,
        };

        let result = ContextMemoryImportanceEvaluator::evaluate(&input);
        assert!(result.is_retained_as_important);
        assert!(matches!(
            result.tier,
            MemoryImportanceTier::Critical | MemoryImportanceTier::Important
        ));
    }

    #[test]
    fn test_scenario_old_completed_task_useful() {
        // Old completed task: historical reference
        let input = MemoryImportanceInput {
            entity_id: "task-setup-vite",
            title: Some("Vite boilerplate setup"),
            content: "Initialized Next.js / Vite template for workspace",
            item_type: "task",
            provenance: InformationProvenance::Confirmed,
            is_user_confirmed: true,
            user_feedback: None,
            task_relevance: 0.40,
            context_relevance: 0.70,
            workset_relevance: None,
            session_count: 1,
            relationship_degree: 1,
            relationship_strength: 0.50,
            first_seen_at: Some("2026-09-01T00:00:00Z"),
            last_seen_at: Some("2026-09-01T02:00:00Z"),
            is_active_task: false,
            is_completed_task: true,
            snapshot_presence_count: 4,
            signal_diversity_count: 2,
        };

        let result = ContextMemoryImportanceEvaluator::evaluate(&input);
        assert_eq!(result.tier, MemoryImportanceTier::Useful);
        assert!(!result.tier.is_temporary());
    }

    #[test]
    fn test_generic_noise_and_accidental_captures_suppression() {
        let accidental = MemoryImportanceInput {
            entity_id: "acc-1",
            title: Some(""),
            content: "about:blank",
            item_type: "url",
            provenance: InformationProvenance::Observed,
            is_user_confirmed: false,
            user_feedback: None,
            task_relevance: 0.0,
            context_relevance: 0.0,
            workset_relevance: None,
            session_count: 1,
            relationship_degree: 0,
            relationship_strength: 0.0,
            first_seen_at: None,
            last_seen_at: None,
            is_active_task: false,
            is_completed_task: false,
            snapshot_presence_count: 0,
            signal_diversity_count: 1,
        };

        let res = ContextMemoryImportanceEvaluator::evaluate(&accidental);
        assert_eq!(res.tier, MemoryImportanceTier::Temporary);
        assert!(res.score <= 0.15);

        let generic = MemoryImportanceInput {
            entity_id: "gen-1",
            title: Some("Google Search"),
            content: "https://google.com",
            item_type: "url",
            provenance: InformationProvenance::Observed,
            is_user_confirmed: false,
            user_feedback: None,
            task_relevance: 0.10,
            context_relevance: 0.10,
            workset_relevance: None,
            session_count: 5, // high count should NOT save generic noise!
            relationship_degree: 0,
            relationship_strength: 0.0,
            first_seen_at: None,
            last_seen_at: None,
            is_active_task: false,
            is_completed_task: false,
            snapshot_presence_count: 0,
            signal_diversity_count: 1,
        };

        let res2 = ContextMemoryImportanceEvaluator::evaluate(&generic);
        assert_eq!(res2.tier, MemoryImportanceTier::Temporary);
        assert!(res2.score <= 0.20);
    }

    // =========================================================================
    // Adversarial Quality Guard Tests (Step 10)
    // =========================================================================

    #[test]
    fn test_guard_adversarial_repeated_generic_text() {
        // Scenario 1: Repeated generic text like "New Tab", "Untitled", "Search..."
        // Even if captured 10 times across 4 sessions, it MUST NOT become important memory.
        let input = MemoryImportanceInput {
            entity_id: "generic-repeated-1",
            title: Some("New Tab"),
            content: "chrome://newtab",
            item_type: "url",
            provenance: InformationProvenance::Observed,
            is_user_confirmed: false,
            user_feedback: None,
            task_relevance: 0.10,
            context_relevance: 0.20,
            workset_relevance: None,
            session_count: 5,
            relationship_degree: 0,
            relationship_strength: 0.0,
            first_seen_at: Some("2026-09-10T00:00:00Z"),
            last_seen_at: Some("2026-09-18T00:00:00Z"),
            is_active_task: false,
            is_completed_task: false,
            snapshot_presence_count: 0,
            signal_diversity_count: 1,
        };

        let eval = ContextMemoryImportanceEvaluator::evaluate(&input);
        let guard_verdict = ContextMemoryQualityGuard::verify_candidate(&input, &eval);

        assert!(!guard_verdict.is_accepted);
        assert!(!eval.is_retained_as_important);
        assert_eq!(eval.tier, MemoryImportanceTier::Temporary);
        assert!(guard_verdict
            .failure_reasons
            .iter()
            .any(|r| r.contains("generic noise")));
    }

    #[test]
    fn test_guard_adversarial_same_website_used_by_multiple_contexts() {
        // Scenario 2: Same website (e.g. stackoverflow.com or search engine) used casually
        // across multiple contexts without specific context grounding.
        let input = MemoryImportanceInput {
            entity_id: "shared-so-lookup",
            title: Some("Stack Overflow - Where Developers Learn"),
            content: "https://stackoverflow.com",
            item_type: "url",
            provenance: InformationProvenance::Observed,
            is_user_confirmed: false,
            user_feedback: None,
            task_relevance: 0.20,
            context_relevance: 0.25, // Low specific context relevance
            workset_relevance: None,
            session_count: 3,
            relationship_degree: 0,
            relationship_strength: 0.0,
            first_seen_at: Some("2026-09-10T00:00:00Z"),
            last_seen_at: Some("2026-09-18T00:00:00Z"),
            is_active_task: false,
            is_completed_task: false,
            snapshot_presence_count: 0,
            signal_diversity_count: 1,
        };

        let eval = ContextMemoryImportanceEvaluator::evaluate(&input);
        let guard_verdict = ContextMemoryQualityGuard::verify_candidate(&input, &eval);

        assert!(!guard_verdict.is_accepted);
        assert!(!eval.is_retained_as_important);
        assert!(guard_verdict
            .failure_reasons
            .iter()
            .any(|r| r.contains("context relevance")));
    }

    #[test]
    fn test_guard_adversarial_same_app_across_multiple_tasks() {
        // Scenario 3: Item solely represents an application name (e.g. "Terminal" or "VS Code")
        // without document/project payload.
        let input = MemoryImportanceInput {
            entity_id: "app-terminal-only",
            title: Some("Terminal"),
            content: "Terminal",
            item_type: "app",
            provenance: InformationProvenance::Observed,
            is_user_confirmed: false,
            user_feedback: None,
            task_relevance: 0.50,
            context_relevance: 0.50,
            workset_relevance: None,
            session_count: 4,
            relationship_degree: 1,
            relationship_strength: 0.40,
            first_seen_at: Some("2026-09-10T00:00:00Z"),
            last_seen_at: Some("2026-09-18T00:00:00Z"),
            is_active_task: false,
            is_completed_task: false,
            snapshot_presence_count: 0,
            signal_diversity_count: 2,
        };

        let eval = ContextMemoryImportanceEvaluator::evaluate(&input);
        let guard_verdict = ContextMemoryQualityGuard::verify_candidate(&input, &eval);

        assert!(!guard_verdict.is_accepted);
        assert!(!eval.is_retained_as_important);
        assert!(guard_verdict
            .failure_reasons
            .iter()
            .any(|r| r.contains("application name")));
    }

    #[test]
    fn test_guard_adversarial_duplicate_captures() {
        // Scenario 4: Duplicate captures of the same URL in the same context.
        use crate::context_engine::important_memory::{
            ContextImportantMemory, ImportantMemoryExplanation, ImportantMemoryMetadata,
            ImportantMemoryType,
        };

        let mem1 = ContextImportantMemory::new(
            "mem-1".to_string(),
            "ws-1",
            "item-dup-1".to_string(),
            ImportantMemoryType::ImportantReference,
            0.85,
            0.90,
            Some("2026-09-18T00:00:00Z".to_string()),
            Some("2026-09-18T01:00:00Z".to_string()),
            InformationProvenance::Observed,
            "Aurora Architecture RFC".to_string(),
            Some("Detailed design documentation".to_string()),
            ImportantMemoryExplanation::new(
                "Primary doc".to_string(),
                vec!["Architecture doc".to_string()],
                vec!["item-dup-1".to_string()],
                0.90,
                InformationProvenance::Observed,
                None,
            ),
            ImportantMemoryMetadata::default(),
        );

        // Duplicate item with same title
        let mem2 = ContextImportantMemory::new(
            "mem-2".to_string(),
            "ws-1",
            "item-dup-2".to_string(),
            ImportantMemoryType::ImportantReference,
            0.82,
            0.88,
            Some("2026-09-18T00:05:00Z".to_string()),
            Some("2026-09-18T01:05:00Z".to_string()),
            InformationProvenance::Observed,
            "Aurora Architecture RFC".to_string(),
            Some("Detailed design documentation duplicate".to_string()),
            ImportantMemoryExplanation::new(
                "Duplicate doc".to_string(),
                vec!["Architecture doc duplicate".to_string()],
                vec!["item-dup-2".to_string()],
                0.88,
                InformationProvenance::Observed,
                None,
            ),
            ImportantMemoryMetadata::default(),
        );

        let filtered = ContextMemoryQualityGuard::filter_memories(vec![mem1.clone(), mem2]);
        assert_eq!(filtered.len(), 1, "Duplicate memory must be deduplicated by Quality Guard");
        assert_eq!(filtered[0].id, "mem-1");
    }

    #[test]
    fn test_guard_adversarial_unrelated_popular_domain() {
        // Scenario 5: Random popular domain (e.g. YouTube / Reddit / Google homepage)
        let input = MemoryImportanceInput {
            entity_id: "url-yt-home",
            title: Some("YouTube"),
            content: "https://youtube.com",
            item_type: "url",
            provenance: InformationProvenance::Observed,
            is_user_confirmed: false,
            user_feedback: None,
            task_relevance: 0.10,
            context_relevance: 0.15,
            workset_relevance: None,
            session_count: 1,
            relationship_degree: 0,
            relationship_strength: 0.0,
            first_seen_at: Some("2026-09-18T00:00:00Z"),
            last_seen_at: Some("2026-09-18T00:00:00Z"),
            is_active_task: false,
            is_completed_task: false,
            snapshot_presence_count: 0,
            signal_diversity_count: 1,
        };

        let eval = ContextMemoryImportanceEvaluator::evaluate(&input);
        let guard_verdict = ContextMemoryQualityGuard::verify_candidate(&input, &eval);

        assert!(!guard_verdict.is_accepted);
        assert!(!eval.is_retained_as_important);
        assert!(guard_verdict
            .failure_reasons
            .iter()
            .any(|r| r.contains("popular domain") || r.contains("generic noise")));
    }

    #[test]
    fn test_guard_adversarial_old_stale_memory() {
        // Scenario 6: Unconfirmed memory with zero multi-session grounding that has decayed over 30 days
        let input = MemoryImportanceInput {
            entity_id: "old-transient-item",
            title: Some("Old transient lookup note"),
            content: "Some temporary lookup text from 30 days ago",
            item_type: "text",
            provenance: InformationProvenance::Observed,
            is_user_confirmed: false,
            user_feedback: None,
            task_relevance: 0.10,
            context_relevance: 0.40,
            workset_relevance: None,
            session_count: 1,
            relationship_degree: 0,
            relationship_strength: 0.0,
            first_seen_at: Some("2026-08-18T00:00:00Z"),
            last_seen_at: Some("2026-08-18T00:00:00Z"),
            is_active_task: false,
            is_completed_task: false,
            snapshot_presence_count: 0,
            signal_diversity_count: 1,
        };

        let eval = ContextMemoryImportanceEvaluator::evaluate(&input);
        let guard_verdict = ContextMemoryQualityGuard::verify_candidate(&input, &eval);

        assert!(!guard_verdict.is_accepted);
        assert!(!eval.is_retained_as_important);
        assert_eq!(eval.tier, MemoryImportanceTier::Temporary);
    }

    #[test]
    fn test_guard_adversarial_conflicting_user_feedback() {
        // Scenario 7: Inferred item had strong signals, but user explicitly clicked "dismiss" / "remove"
        let input = MemoryImportanceInput {
            entity_id: "item-dismissed",
            title: Some("Potentially relevant API specification"),
            content: "REST endpoints for user management service",
            item_type: "text",
            provenance: InformationProvenance::Observed,
            is_user_confirmed: false,
            user_feedback: Some("dismiss"), // User feedback contradicts inferred importance
            task_relevance: 0.90,
            context_relevance: 0.95,
            workset_relevance: Some(WorksetItemRelevance::Primary),
            session_count: 3,
            relationship_degree: 4,
            relationship_strength: 0.85,
            first_seen_at: Some("2026-09-10T00:00:00Z"),
            last_seen_at: Some("2026-09-18T00:00:00Z"),
            is_active_task: false,
            is_completed_task: false,
            snapshot_presence_count: 2,
            signal_diversity_count: 3,
        };

        let eval = ContextMemoryImportanceEvaluator::evaluate(&input);
        let guard_verdict = ContextMemoryQualityGuard::verify_candidate(&input, &eval);

        assert!(!guard_verdict.is_accepted);
        assert!(!eval.is_retained_as_important);
        assert!(guard_verdict
            .failure_reasons
            .iter()
            .any(|r| r.contains("rejected or dismissed by user")));
    }

    #[test]
    fn test_guard_adversarial_weak_semantic_similarity() {
        // Scenario 8: Weak semantic match (0.42) with no relational or multi-session backing
        let input = MemoryImportanceInput {
            entity_id: "weak-semantic-doc",
            title: Some("Random loosely matching document"),
            content: "Contains a single keyword match with low relevance",
            item_type: "text",
            provenance: InformationProvenance::Observed,
            is_user_confirmed: false,
            user_feedback: None,
            task_relevance: 0.42, // Weak semantic score
            context_relevance: 0.40,
            workset_relevance: None,
            session_count: 1,
            relationship_degree: 0,
            relationship_strength: 0.0,
            first_seen_at: Some("2026-09-18T00:00:00Z"),
            last_seen_at: Some("2026-09-18T00:00:00Z"),
            is_active_task: false,
            is_completed_task: false,
            snapshot_presence_count: 0,
            signal_diversity_count: 1,
        };

        let eval = ContextMemoryImportanceEvaluator::evaluate(&input);
        let guard_verdict = ContextMemoryQualityGuard::verify_candidate(&input, &eval);

        assert!(!guard_verdict.is_accepted);
        assert!(!eval.is_retained_as_important);
        assert!(guard_verdict
            .failure_reasons
            .iter()
            .any(|r| r.contains("weak, isolated semantic match")));
    }

    #[test]
    fn test_guard_adversarial_single_item_context() {
        // Scenario 9: Workspace has only 1 captured item from a single glance lookup
        let input = MemoryImportanceInput {
            entity_id: "lone-item-1",
            title: Some("Quick calculation in terminal"),
            content: "echo $((42 * 100))",
            item_type: "text",
            provenance: InformationProvenance::Observed,
            is_user_confirmed: false,
            user_feedback: None,
            task_relevance: 0.10,
            context_relevance: 0.30,
            workset_relevance: None,
            session_count: 1,
            relationship_degree: 0,
            relationship_strength: 0.0,
            first_seen_at: Some("2026-09-18T00:00:00Z"),
            last_seen_at: Some("2026-09-18T00:00:00Z"),
            is_active_task: false,
            is_completed_task: false,
            snapshot_presence_count: 0,
            signal_diversity_count: 1,
        };

        let eval = ContextMemoryImportanceEvaluator::evaluate(&input);
        assert!(!eval.is_retained_as_important);
        assert_eq!(eval.tier, MemoryImportanceTier::Temporary);
    }

    #[test]
    fn test_guard_adversarial_multiple_competing_tasks() {
        // Scenario 10: Task candidate with low confidence (0.35) competing with ambiguous signals
        let input = MemoryImportanceInput {
            entity_id: "task-competing-1",
            title: Some("Uncertain background indexing"),
            content: "Background indexing task candidate with low confidence",
            item_type: "task",
            provenance: InformationProvenance::Inferred,
            is_user_confirmed: false,
            user_feedback: None,
            task_relevance: 0.35,
            context_relevance: 0.40,
            workset_relevance: None,
            session_count: 1,
            relationship_degree: 0,
            relationship_strength: 0.0,
            first_seen_at: Some("2026-09-18T00:00:00Z"),
            last_seen_at: Some("2026-09-18T00:00:00Z"),
            is_active_task: true, // Marked as active candidate, but confidence is too low
            is_completed_task: false,
            snapshot_presence_count: 0,
            signal_diversity_count: 1,
        };

        let eval = ContextMemoryImportanceEvaluator::evaluate(&input);
        let guard_verdict = ContextMemoryQualityGuard::verify_candidate(&input, &eval);

        assert!(!guard_verdict.is_accepted);
        assert!(!eval.is_retained_as_important);
        assert!(guard_verdict
            .failure_reasons
            .iter()
            .any(|r| r.contains("insufficient confidence")));
    }
}

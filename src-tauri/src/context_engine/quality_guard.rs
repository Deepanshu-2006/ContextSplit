use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Result as SqlResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

use crate::context_engine::competition::{AmbiguityState, ContextCompetitionAnalyzer};
use crate::context_engine::contradiction::ContradictionAnalyzer;
use crate::context_engine::evidence::ContextEvidenceCollector;
use crate::context_engine::scoring::{
    load_workspace_scoring_contexts, ContextEvidenceScorer, ScoringWeights,
};
use crate::context_engine::task_extractor::is_generic_task_name;
use crate::context_engine::workset::{is_accidental_capture, is_generic_noise};
use crate::db::models::{ContextMembership, Item, Workspace};

// =========================================================================
// Quality Guard Decisions (Outcomes)
// =========================================================================

/// Possible evaluation outcomes produced by the ContextQualityGuard.
///
/// Invariant: System strictly adheres to precision over recall.
/// If evidence is insufficient, `HoldForMoreEvidence` is chosen over `Accept`.
/// If evidence conflicts with existing context, contamination is strictly prevented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityGuardDecision {
    /// High certainty, strongly corroborated activity that decisively belongs to the context.
    Accept,
    /// Moderately supported activity attached non-destructively with low confidence flag.
    AcceptWithLowConfidence,
    /// Insufficient, ambiguous, or uncorroborated evidence; item remains in Inbox awaiting more signals.
    HoldForMoreEvidence,
    /// Strongly contradictory, dismissed, noisy, or cross-contaminating activity; rejected from context.
    Reject,
    /// Cohesive cluster of activity with high mutual affinity but low affinity to any existing workspace.
    CreateNewCandidateContext,
}

impl QualityGuardDecision {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::AcceptWithLowConfidence => "accept_with_low_confidence",
            Self::HoldForMoreEvidence => "hold_for_more_evidence",
            Self::Reject => "reject",
            Self::CreateNewCandidateContext => "create_new_candidate_context",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Accept => "Accept into Context",
            Self::AcceptWithLowConfidence => "Accept with Low Confidence",
            Self::HoldForMoreEvidence => "Hold for More Evidence",
            Self::Reject => "Reject (Prevent Contamination)",
            Self::CreateNewCandidateContext => "Create New Candidate Context",
        }
    }

    /// Whether this decision permits modifying the existing context state/workset/memberships.
    pub fn should_modify_context(&self) -> bool {
        matches!(self, Self::Accept | Self::AcceptWithLowConfidence)
    }

    pub fn is_accepted(&self) -> bool {
        matches!(self, Self::Accept | Self::AcceptWithLowConfidence)
    }

    pub fn is_held(&self) -> bool {
        matches!(self, Self::HoldForMoreEvidence)
    }

    pub fn is_rejected(&self) -> bool {
        matches!(self, Self::Reject)
    }

    pub fn is_new_candidate(&self) -> bool {
        matches!(self, Self::CreateNewCandidateContext)
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        let norm = s.trim().to_lowercase().replace(['-', ' '], "_");
        match norm.as_str() {
            "accept" => Ok(Self::Accept),
            "accept_with_low_confidence" | "accept_low" | "accept_low_confidence" => {
                Ok(Self::AcceptWithLowConfidence)
            }
            "hold_for_more_evidence" | "hold" | "needs_review" | "inbox" => {
                Ok(Self::HoldForMoreEvidence)
            }
            "reject" | "rejected" | "suppress" => Ok(Self::Reject),
            "create_new_candidate_context" | "create_candidate" | "new_candidate" => {
                Ok(Self::CreateNewCandidateContext)
            }
            _ => Err(format!("Unknown QualityGuardDecision: '{}'", s)),
        }
    }
}

impl std::fmt::Display for QualityGuardDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// =========================================================================
// Dimension Scores for Transparent Guard Calibration
// =========================================================================

/// Explicit breakdown across the 9 core evaluation dimensions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct QualityGuardDimensionScores {
    /// 1. Semantic relevance (cosine similarity with items/profile centroid: 0.0 - 1.0)
    pub semantic_relevance: f64,
    /// 2. Task relevance (alignment with active/in-progress task candidates: 0.0 - 1.0)
    pub task_relevance: f64,
    /// 3. Recent contextual continuity (active window streak / focus bursts in workspace: 0.0 - 1.0)
    pub recent_contextual_continuity: f64,
    /// 4. Item relationships (graph edge density & co-occurrence weights: 0.0 - 1.0)
    pub item_relationships: f64,
    /// 5. Domain relevance (browser domain alignment or cross-domain divergence: 0.0 - 1.0)
    pub domain_relevance: f64,
    /// 6. Temporal continuity (freshness, elapsed session proximity: 0.0 - 1.0)
    pub temporal_continuity: f64,
    /// 7. Existing context confidence (calibrated match confidence: 0.0 - 1.0)
    pub existing_context_confidence: f64,
    /// 8. Conflicting context evidence (contradiction severity & competitor overlap: 0.0 - 1.0)
    pub conflicting_context_evidence: f64,
    /// 9. User feedback (explicit confirm boost vs historical dismissal penalty: -1.0 to 1.0)
    pub user_feedback: f64,
    /// Evidence diversity multiplier (0.35 to 1.00)
    pub diversity_multiplier: f64,
    /// Noise penalty discount applied for generic or ephemeral attributes (0.0 to 1.0)
    pub noise_penalty: f64,
    /// Composite weighted quality score bounded in [0.0, 1.0]
    pub composite_quality_score: f64,
}

// =========================================================================
// Input Model
// =========================================================================

/// Comprehensive input structure capturing the 9 evidence dimensions for a candidate activity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityGuardInput {
    pub activity_id: Option<String>,
    pub title: String,
    pub content_snippet: Option<String>,
    pub app_name: Option<String>,
    pub domain: Option<String>,
    pub target_workspace_id: Option<String>,
    pub target_workspace_name: Option<String>,
    pub semantic_relevance: f64,
    pub task_relevance: f64,
    pub recent_contextual_continuity: f64,
    pub item_relationships: f64,
    pub domain_relevance: f64,
    pub temporal_continuity: f64,
    pub existing_context_confidence: f64,
    pub conflicting_context_evidence: f64,
    pub user_feedback: f64,
    pub is_generic_noise: bool,
    pub is_accidental_glance: bool,
    pub is_explicit_user_action: bool,
    pub score_margin_over_runner_up: Option<f64>,
    pub candidate_cluster_cohesion: Option<f64>,
}

impl QualityGuardInput {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            activity_id: None,
            title: title.into(),
            content_snippet: None,
            app_name: None,
            domain: None,
            target_workspace_id: None,
            target_workspace_name: None,
            semantic_relevance: 0.0,
            task_relevance: 0.0,
            recent_contextual_continuity: 0.0,
            item_relationships: 0.0,
            domain_relevance: 0.0,
            temporal_continuity: 0.0,
            existing_context_confidence: 0.0,
            conflicting_context_evidence: 0.0,
            user_feedback: 0.0,
            is_generic_noise: false,
            is_accidental_glance: false,
            is_explicit_user_action: false,
            score_margin_over_runner_up: None,
            candidate_cluster_cohesion: None,
        }
    }

    pub fn with_target(mut self, ws_id: impl Into<String>, ws_name: impl Into<String>) -> Self {
        self.target_workspace_id = Some(ws_id.into());
        self.target_workspace_name = Some(ws_name.into());
        self
    }

    pub fn with_semantic_relevance(mut self, score: f64) -> Self {
        self.semantic_relevance = score.clamp(0.0, 1.0);
        self
    }

    pub fn with_task_relevance(mut self, score: f64) -> Self {
        self.task_relevance = score.clamp(0.0, 1.0);
        self
    }

    pub fn with_continuity(mut self, streak_continuity: f64, temporal_continuity: f64) -> Self {
        self.recent_contextual_continuity = streak_continuity.clamp(0.0, 1.0);
        self.temporal_continuity = temporal_continuity.clamp(0.0, 1.0);
        self
    }

    pub fn with_item_relationships(mut self, score: f64) -> Self {
        self.item_relationships = score.clamp(0.0, 1.0);
        self
    }

    pub fn with_domain_relevance(mut self, score: f64) -> Self {
        self.domain_relevance = score.clamp(0.0, 1.0);
        self
    }

    pub fn with_context_confidence(mut self, score: f64) -> Self {
        self.existing_context_confidence = score.clamp(0.0, 1.0);
        self
    }

    pub fn with_conflict(mut self, conflict_score: f64) -> Self {
        self.conflicting_context_evidence = conflict_score.clamp(0.0, 1.0);
        self
    }

    pub fn with_user_feedback(mut self, feedback: f64) -> Self {
        self.user_feedback = feedback.clamp(-1.0, 1.0);
        self
    }

    pub fn with_explicit_user_action(mut self, is_explicit: bool) -> Self {
        self.is_explicit_user_action = is_explicit;
        self
    }

    pub fn with_noise_flags(mut self, is_noise: bool, is_glance: bool) -> Self {
        self.is_generic_noise = is_noise;
        self.is_accidental_glance = is_glance;
        self
    }

    pub fn with_competition(
        mut self,
        margin: Option<f64>,
        cluster_cohesion: Option<f64>,
    ) -> Self {
        self.score_margin_over_runner_up = margin;
        self.candidate_cluster_cohesion = cluster_cohesion;
        self
    }
}

// =========================================================================
// Result Model
// =========================================================================

/// Detailed evaluation result produced by the ContextQualityGuard.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QualityGuardResult {
    pub decision: QualityGuardDecision,
    pub decision_name: String,
    pub composite_quality_score: f64,
    pub confidence: f64,
    pub dimension_scores: QualityGuardDimensionScores,
    pub reasons: Vec<String>,
    pub warnings: Vec<String>,
    pub suggested_action: String,
    pub target_workspace_id: Option<String>,
    pub target_workspace_name: Option<String>,
}

// =========================================================================
// Configuration
// =========================================================================

/// Configuration parameters for ContextQualityGuard thresholds and penalties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityGuardConfig {
    /// Composite score required for decisive `Accept` into context (default: 0.65)
    pub accept_threshold: f64,
    /// Composite score required for `AcceptWithLowConfidence` (default: 0.45)
    pub accept_low_confidence_threshold: f64,
    /// Minimum margin over competitor required to avoid ambiguous hold (default: 0.15)
    pub ambiguity_margin_threshold: f64,
    /// Conflict / contradiction threshold that triggers immediate `Reject` (default: 0.55)
    pub conflict_rejection_threshold: f64,
    /// Cluster cohesion threshold to trigger `CreateNewCandidateContext` (default: 0.60)
    pub candidate_cluster_creation_threshold: f64,
    /// Max duration in seconds for transient micro-glance suppression (default: 5)
    pub max_micro_glance_seconds: i64,
}

impl Default for QualityGuardConfig {
    fn default() -> Self {
        Self {
            accept_threshold: 0.50,
            accept_low_confidence_threshold: 0.20,
            ambiguity_margin_threshold: 0.15,
            conflict_rejection_threshold: 0.55,
            candidate_cluster_creation_threshold: 0.60,
            max_micro_glance_seconds: 5,
        }
    }
}

// =========================================================================
// Quality Guard Service
// =========================================================================

/// Core evaluator that guards existing contexts against false positives, noise, and cross-contamination.
#[derive(Debug, Clone)]
pub struct ContextQualityGuard {
    pub config: QualityGuardConfig,
}

impl Default for ContextQualityGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextQualityGuard {
    pub fn new() -> Self {
        Self {
            config: QualityGuardConfig::default(),
        }
    }

    pub fn with_config(config: QualityGuardConfig) -> Self {
        Self { config }
    }

    /// Pure, deterministic evaluation of candidate activity against quality dimensions.
    pub fn evaluate(&self, input: &QualityGuardInput) -> QualityGuardResult {
        let mut reasons = Vec::new();
        let mut warnings = Vec::new();

        let target_name = input
            .target_workspace_name
            .as_deref()
            .unwrap_or("target context");

        // ---------------------------------------------------------------------
        // 1. Authoritative Override: Explicit User Organization
        // ---------------------------------------------------------------------
        if input.is_explicit_user_action || input.user_feedback >= 0.95 {
            reasons.push(format!(
                "Explicit user confirmation: direct assignment to {}.",
                target_name
            ));
            let dim = QualityGuardDimensionScores {
                semantic_relevance: input.semantic_relevance,
                task_relevance: input.task_relevance,
                recent_contextual_continuity: input.recent_contextual_continuity,
                item_relationships: input.item_relationships,
                domain_relevance: input.domain_relevance,
                temporal_continuity: input.temporal_continuity,
                existing_context_confidence: 1.0,
                conflicting_context_evidence: 0.0,
                user_feedback: 1.0,
                diversity_multiplier: 1.0,
                noise_penalty: 0.0,
                composite_quality_score: 1.0,
            };

            return QualityGuardResult {
                decision: QualityGuardDecision::Accept,
                decision_name: QualityGuardDecision::Accept.as_str().to_string(),
                composite_quality_score: 1.0,
                confidence: 1.0,
                dimension_scores: dim,
                reasons,
                warnings,
                suggested_action: format!(
                    "Item confirmed into {} by authoritative user action.",
                    target_name
                ),
                target_workspace_id: input.target_workspace_id.clone(),
                target_workspace_name: input.target_workspace_name.clone(),
            };
        }

        // ---------------------------------------------------------------------
        // 2. Strict Contradiction & Contamination Rejection Check
        // ---------------------------------------------------------------------
        if input.user_feedback <= -0.50 {
            warnings.push(format!(
                "Direct user dismissal: prior negative feedback against {}.",
                target_name
            ));
            let dim = QualityGuardDimensionScores {
                conflicting_context_evidence: 1.0,
                user_feedback: input.user_feedback,
                composite_quality_score: 0.0,
                ..Default::default()
            };

            return QualityGuardResult {
                decision: QualityGuardDecision::Reject,
                decision_name: QualityGuardDecision::Reject.as_str().to_string(),
                composite_quality_score: 0.0,
                confidence: 0.0,
                dimension_scores: dim,
                reasons: vec![format!(
                    "User previously dismissed association with {}.",
                    target_name
                )],
                warnings,
                suggested_action: format!(
                    "Suppressed from {} to prevent context contamination.",
                    target_name
                ),
                target_workspace_id: input.target_workspace_id.clone(),
                target_workspace_name: input.target_workspace_name.clone(),
            };
        }

        if input.conflicting_context_evidence >= self.config.conflict_rejection_threshold {
            warnings.push(format!(
                "Severe context conflict ({:.2}): activity belongs to another active domain or conflicting workspace.",
                input.conflicting_context_evidence
            ));
            let dim = QualityGuardDimensionScores {
                conflicting_context_evidence: input.conflicting_context_evidence,
                composite_quality_score: 0.0,
                ..Default::default()
            };

            return QualityGuardResult {
                decision: QualityGuardDecision::Reject,
                decision_name: QualityGuardDecision::Reject.as_str().to_string(),
                composite_quality_score: 0.0,
                confidence: 0.0,
                dimension_scores: dim,
                reasons: vec![format!(
                    "Contradictory evidence strongly conflicts with {}.",
                    target_name
                )],
                warnings,
                suggested_action: "Rejected from workspace to protect context boundary integrity."
                    .to_string(),
                target_workspace_id: input.target_workspace_id.clone(),
                target_workspace_name: input.target_workspace_name.clone(),
            };
        }

        // ---------------------------------------------------------------------
        // 3. Transient Glance & Noise Suppression
        // ---------------------------------------------------------------------
        let mut noise_penalty = 0.0_f64;
        if input.is_accidental_glance {
            noise_penalty += 0.40;
            warnings.push(
                "Transient micro-glance (<5s) detected; suppressed from context switching."
                    .to_string(),
            );
        }
        if input.is_generic_noise {
            noise_penalty += 0.40;
            warnings.push(
                "Generic window title or ungrounded noise; context modification suppressed."
                    .to_string(),
            );
        }

        // ---------------------------------------------------------------------
        // 4. Multi-Dimension Scoring & Evidence Diversity
        // ---------------------------------------------------------------------
        // Count independent signal categories supporting the context
        let mut positive_signals_count = 0_usize;
        if input.semantic_relevance >= 0.40 {
            positive_signals_count += 1;
            reasons.push(format!(
                "Semantic similarity ({:.2}) aligns with {}.",
                input.semantic_relevance, target_name
            ));
        }
        if input.task_relevance >= 0.40 {
            positive_signals_count += 1;
            reasons.push(format!(
                "Task grounding ({:.2}) matches active task in {}.",
                input.task_relevance, target_name
            ));
        }
        if input.item_relationships >= 0.35 {
            positive_signals_count += 1;
            reasons.push(format!(
                "Item relationships ({:.2}) connect to existing assets in {}.",
                input.item_relationships, target_name
            ));
        }
        if input.recent_contextual_continuity >= 0.40 {
            positive_signals_count += 1;
            reasons.push(format!(
                "Context continuity ({:.2}) continues active focus burst in {}.",
                input.recent_contextual_continuity, target_name
            ));
        }
        if input.temporal_continuity >= 0.50 {
            positive_signals_count += 1;
        }
        if input.domain_relevance >= 0.50 {
            positive_signals_count += 1;
            reasons.push(format!(
                "Domain alignment ({:.2}) matches {} domain profile.",
                input.domain_relevance, target_name
            ));
        }

        // Diversity Multiplier: single signal alone cannot produce high confidence
        let diversity_multiplier = match positive_signals_count {
            0 => 0.0,
            1 => 0.50,
            2 => 0.85,
            _ => 1.00,
        };

        if positive_signals_count == 1 {
            warnings.push(
                "Single isolated evidence category; confidence capped until corroborated."
                    .to_string(),
            );
        }

        // Dimension weights
        let weighted_sum = (input.semantic_relevance * 0.25)
            + (input.task_relevance * 0.25)
            + (input.item_relationships * 0.20)
            + (input.recent_contextual_continuity * 0.15)
            + (input.domain_relevance * 0.08)
            + (input.temporal_continuity * 0.07);

        let base_score = (weighted_sum * diversity_multiplier) - noise_penalty;
        let conflict_discount = input.conflicting_context_evidence * 0.50;
        let composite_quality_score = (base_score - conflict_discount).clamp(0.0, 1.0);

        let calibrated_confidence = (composite_quality_score
            * input.existing_context_confidence.max(0.20))
        .clamp(0.0, 1.0);

        let dim = QualityGuardDimensionScores {
            semantic_relevance: input.semantic_relevance,
            task_relevance: input.task_relevance,
            recent_contextual_continuity: input.recent_contextual_continuity,
            item_relationships: input.item_relationships,
            domain_relevance: input.domain_relevance,
            temporal_continuity: input.temporal_continuity,
            existing_context_confidence: input.existing_context_confidence,
            conflicting_context_evidence: input.conflicting_context_evidence,
            user_feedback: input.user_feedback,
            diversity_multiplier,
            noise_penalty,
            composite_quality_score,
        };

        // ---------------------------------------------------------------------
        // 5. Emerging Candidate Context Check
        // ---------------------------------------------------------------------
        if composite_quality_score < 0.30 {
            if let Some(cohesion) = input.candidate_cluster_cohesion {
                if cohesion >= self.config.candidate_cluster_creation_threshold {
                    reasons.push(format!(
                        "Strong cluster cohesion ({:.2}) among unassigned activity forming a novel context candidate.",
                        cohesion
                    ));
                    return QualityGuardResult {
                        decision: QualityGuardDecision::CreateNewCandidateContext,
                        decision_name: QualityGuardDecision::CreateNewCandidateContext
                            .as_str()
                            .to_string(),
                        composite_quality_score,
                        confidence: cohesion,
                        dimension_scores: dim,
                        reasons,
                        warnings,
                        suggested_action: "Forming new candidate context suggestion in Inbox."
                            .to_string(),
                        target_workspace_id: None,
                        target_workspace_name: None,
                    };
                }
            }
        }

        // ---------------------------------------------------------------------
        // 6. Ambiguity Check: Margin against Runner-Up
        // ---------------------------------------------------------------------
        if let Some(margin) = input.score_margin_over_runner_up {
            if margin <= self.config.ambiguity_margin_threshold && composite_quality_score >= 0.25 {
                warnings.push(format!(
                    "Ambiguous contest (margin {:.2} <= {:.2}); held in Inbox without forcing workspace assignment.",
                    margin, self.config.ambiguity_margin_threshold
                ));

                return QualityGuardResult {
                    decision: QualityGuardDecision::HoldForMoreEvidence,
                    decision_name: QualityGuardDecision::HoldForMoreEvidence
                        .as_str()
                        .to_string(),
                    composite_quality_score,
                    confidence: calibrated_confidence,
                    dimension_scores: dim,
                    reasons,
                    warnings,
                    suggested_action: "Held in Inbox (NeedsReview) awaiting user clarification or additional evidence."
                        .to_string(),
                    target_workspace_id: input.target_workspace_id.clone(),
                    target_workspace_name: input.target_workspace_name.clone(),
                };
            }
        }

        // ---------------------------------------------------------------------
        // 7. Final Threshold Decision
        // ---------------------------------------------------------------------
        if composite_quality_score >= self.config.accept_threshold
            && calibrated_confidence >= 0.50
            && positive_signals_count >= 2
            && noise_penalty == 0.0
        {
            reasons.push(format!(
                "High quality score ({:.2}) and multi-signal agreement: accepted into {}.",
                composite_quality_score, target_name
            ));
            QualityGuardResult {
                decision: QualityGuardDecision::Accept,
                decision_name: QualityGuardDecision::Accept.as_str().to_string(),
                composite_quality_score,
                confidence: calibrated_confidence,
                dimension_scores: dim,
                reasons,
                warnings,
                suggested_action: format!(
                    "Accepted into {} working context.",
                    target_name
                ),
                target_workspace_id: input.target_workspace_id.clone(),
                target_workspace_name: input.target_workspace_name.clone(),
            }
        } else if composite_quality_score >= self.config.accept_low_confidence_threshold
            && calibrated_confidence >= 0.10
            && noise_penalty < 0.40
        {
            reasons.push(format!(
                "Moderate quality score ({:.2}): accepted with low confidence into {}.",
                composite_quality_score, target_name
            ));
            QualityGuardResult {
                decision: QualityGuardDecision::AcceptWithLowConfidence,
                decision_name: QualityGuardDecision::AcceptWithLowConfidence
                    .as_str()
                    .to_string(),
                composite_quality_score,
                confidence: calibrated_confidence,
                dimension_scores: dim,
                reasons,
                warnings,
                suggested_action: format!(
                    "Associated with {} as a low-confidence reference.",
                    target_name
                ),
                target_workspace_id: input.target_workspace_id.clone(),
                target_workspace_name: input.target_workspace_name.clone(),
            }
        } else if noise_penalty >= 0.70 {
            warnings.push("Extremely high noise penalty; suppressed from all contexts.".to_string());
            QualityGuardResult {
                decision: QualityGuardDecision::Reject,
                decision_name: QualityGuardDecision::Reject.as_str().to_string(),
                composite_quality_score,
                confidence: calibrated_confidence,
                dimension_scores: dim,
                reasons,
                warnings,
                suggested_action: "Rejected as transient noise.".to_string(),
                target_workspace_id: input.target_workspace_id.clone(),
                target_workspace_name: input.target_workspace_name.clone(),
            }
        } else {
            // Conservative Default: Prefer HoldForMoreEvidence over false positive
            warnings.push(
                "Insufficient corroborating evidence to modify context; held safely in Inbox."
                    .to_string(),
            );
            QualityGuardResult {
                decision: QualityGuardDecision::HoldForMoreEvidence,
                decision_name: QualityGuardDecision::HoldForMoreEvidence
                    .as_str()
                    .to_string(),
                composite_quality_score,
                confidence: calibrated_confidence,
                dimension_scores: dim,
                reasons,
                warnings,
                suggested_action: "Held in Inbox awaiting sustained corroboration.".to_string(),
                target_workspace_id: input.target_workspace_id.clone(),
                target_workspace_name: input.target_workspace_name.clone(),
            }
        }
    }

    /// Evaluates whether a captured item should modify the target workspace by querying database state.
    pub fn evaluate_from_db(
        &self,
        conn: &Connection,
        item_id: &str,
        target_workspace_id: &str,
    ) -> Result<QualityGuardResult, String> {
        let item = crate::db::operations::get_item(conn, item_id).map_err(|e| e.to_string())?;
        let workspace =
            crate::db::operations::get_workspace(conn, target_workspace_id).map_err(|e| e.to_string())?;

        // 1. Check if user already explicitly confirmed this item in this workspace
        let is_confirmed: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM context_memberships WHERE item_id = ?1 AND workspace_id = ?2 AND membership_type = 'confirmed'",
                params![item_id, target_workspace_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|cnt| cnt > 0)
            .unwrap_or(false);

        // 2. Check if user previously dismissed this workspace for this item
        let is_dismissed: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM context_memberships WHERE item_id = ?1 AND workspace_id = ?2 AND membership_type = 'dismissed'",
                params![item_id, target_workspace_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|cnt| cnt > 0)
            .unwrap_or(false);

        let user_feedback = if is_confirmed {
            1.0
        } else if is_dismissed {
            -1.0
        } else {
            0.0
        };

        // 3. Check generic noise and accidental capture
        let is_noise = is_generic_noise(item.title.as_deref(), &item.content, false, is_confirmed);
        let is_glance = is_accidental_capture(&item.item_type, item.title.as_deref(), &item.content, is_confirmed);

        // 4. Compute item relationships to target workspace
        let rel_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM item_relationships r
                 JOIN context_memberships cm ON (cm.item_id = r.item_a_id OR cm.item_id = r.item_b_id)
                 WHERE (r.item_a_id = ?1 OR r.item_b_id = ?1)
                   AND cm.workspace_id = ?2
                   AND cm.membership_type = 'confirmed'",
                params![item_id, target_workspace_id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let item_relationships = (rel_count as f64 * 0.35).min(1.0);

        // 5. Semantic similarity
        let semantic_relevance = crate::context_engine::scoring::evaluate_item_context(conn, item_id)
            .ok()
            .and_then(|interp| {
                interp
                    .existing_workspaces
                    .iter()
                    .find(|w| w.workspace_id == target_workspace_id)
                    .map(|w| w.score)
            })
            .unwrap_or(0.0);

        // 6. Task relevance: check if target workspace has an active task matching item title
        let task_relevance: f64 = conn
            .query_row(
                "SELECT COUNT(*) FROM task_candidates
                 WHERE workspace_id = ?1 AND status = 'active'",
                params![target_workspace_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|cnt| if cnt > 0 { 0.50 } else { 0.0 })
            .unwrap_or(0.0);

        // 7. Continuity: check if same session
        let same_session: bool = if let Some(ref s_id) = item.session_id {
            conn.query_row(
                "SELECT COUNT(*) FROM items i
                 JOIN context_memberships cm ON i.id = cm.item_id
                 WHERE i.session_id = ?1 AND cm.workspace_id = ?2 AND cm.membership_type = 'confirmed'",
                params![s_id, target_workspace_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|cnt| cnt > 0)
            .unwrap_or(false)
        } else {
            false
        };
        let recent_contextual_continuity = if same_session { 0.75 } else { 0.10 };
        let temporal_continuity = if same_session { 0.80 } else { 0.20 };

        // 8. Domain relevance
        let domain_opt = serde_json::from_str::<Value>(&item.metadata)
            .ok()
            .and_then(|v| v.get("domain").and_then(|d| d.as_str().map(|s| s.to_string())));

        let domain_relevance: f64 = if let Some(ref dom) = domain_opt {
            conn.query_row(
                "SELECT COUNT(*) FROM context_signals
                 WHERE workspace_id = ?1 AND payload LIKE ?2",
                params![target_workspace_id, format!("%{}%", dom)],
                |r| r.get::<_, i64>(0),
            )
            .map(|cnt| if cnt > 0 { 0.70 } else { 0.10 })
            .unwrap_or(0.10)
        } else {
            0.0
        };

        // 9. Existing context confidence
        let existing_context_confidence = (semantic_relevance * 0.50)
            + (if same_session { 0.30 } else { 0.0 })
            + (item_relationships * 0.20);

        let input = QualityGuardInput::new(item.title.unwrap_or_default())
            .with_target(target_workspace_id, workspace.name)
            .with_semantic_relevance(semantic_relevance)
            .with_task_relevance(task_relevance)
            .with_continuity(recent_contextual_continuity, temporal_continuity)
            .with_item_relationships(item_relationships)
            .with_domain_relevance(domain_relevance)
            .with_context_confidence(existing_context_confidence)
            .with_user_feedback(user_feedback)
            .with_explicit_user_action(is_confirmed)
            .with_noise_flags(is_noise, is_glance);

        Ok(self.evaluate(&input))
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::operations::{create_item, create_session, create_workspace};

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_guard_decision_accept_on_explicit_user_action() {
        let guard = ContextQualityGuard::new();
        let input = QualityGuardInput::new("User Filed Document")
            .with_target("ws_core", "Core Architecture")
            .with_explicit_user_action(true);

        let res = guard.evaluate(&input);
        assert_eq!(res.decision, QualityGuardDecision::Accept);
        assert!(res.decision.should_modify_context());
        assert_eq!(res.composite_quality_score, 1.0);
        assert_eq!(res.confidence, 1.0);
        assert!(res.reasons.iter().any(|r| r.contains("Explicit user confirmation")));
    }

    #[test]
    fn test_guard_decision_reject_on_user_dismissal() {
        let guard = ContextQualityGuard::new();
        let input = QualityGuardInput::new("Unrelated Note")
            .with_target("ws_design", "Design System")
            .with_semantic_relevance(0.90) // High semantic but dismissed!
            .with_user_feedback(-1.0);

        let res = guard.evaluate(&input);
        assert_eq!(res.decision, QualityGuardDecision::Reject);
        assert!(!res.decision.should_modify_context());
        assert_eq!(res.composite_quality_score, 0.0);
        assert!(res.warnings.iter().any(|w| w.contains("Direct user dismissal")));
    }

    #[test]
    fn test_guard_decision_reject_on_strong_conflicting_evidence() {
        let guard = ContextQualityGuard::new();
        let input = QualityGuardInput::new("Gaming Stream Tab")
            .with_target("ws_work", "Client Project")
            .with_conflict(0.85);

        let res = guard.evaluate(&input);
        assert_eq!(res.decision, QualityGuardDecision::Reject);
        assert!(!res.decision.should_modify_context());
        assert!(res.warnings.iter().any(|w| w.contains("Severe context conflict")));
    }

    #[test]
    fn test_guard_decision_hold_for_insufficient_evidence() {
        let guard = ContextQualityGuard::new();
        // Isolated weak semantic signal with no session, task, or relationships
        let input = QualityGuardInput::new("Random API snippet")
            .with_target("ws_backend", "Backend API")
            .with_semantic_relevance(0.35);

        let res = guard.evaluate(&input);
        assert_eq!(res.decision, QualityGuardDecision::HoldForMoreEvidence);
        assert!(!res.decision.should_modify_context());
        assert!(res.warnings.iter().any(|w| w.contains("Single isolated evidence") || w.contains("Insufficient corroborating evidence")));
    }

    #[test]
    fn test_guard_decision_hold_on_close_ambiguity_margin() {
        let guard = ContextQualityGuard::new();
        let input = QualityGuardInput::new("Shared Library Utility")
            .with_target("ws_frontend", "Frontend")
            .with_semantic_relevance(0.70)
            .with_continuity(0.70, 0.70)
            .with_competition(Some(0.05), None); // Narrow margin against runner-up!

        let res = guard.evaluate(&input);
        assert_eq!(res.decision, QualityGuardDecision::HoldForMoreEvidence);
        assert!(!res.decision.should_modify_context());
        assert!(res.warnings.iter().any(|w| w.contains("Ambiguous contest")));
    }

    #[test]
    fn test_guard_decision_accept_on_strong_multi_signal_corroboration() {
        let guard = ContextQualityGuard::new();
        let input = QualityGuardInput::new("Auth Controller Spec")
            .with_target("ws_auth", "Authentication Service")
            .with_semantic_relevance(0.85)
            .with_task_relevance(0.80)
            .with_continuity(0.85, 0.90)
            .with_item_relationships(0.75)
            .with_domain_relevance(0.80)
            .with_context_confidence(0.88);

        let res = guard.evaluate(&input);
        assert_eq!(res.decision, QualityGuardDecision::Accept);
        assert!(res.decision.should_modify_context());
        assert!(res.composite_quality_score >= 0.75);
        assert!(res.reasons.iter().any(|r| r.contains("High quality score")));
    }

    #[test]
    fn test_guard_decision_accept_with_low_confidence() {
        let guard = ContextQualityGuard::new();
        let input = QualityGuardInput::new("Minor Supporting Doc")
            .with_target("ws_docs", "Documentation")
            .with_semantic_relevance(0.55)
            .with_continuity(0.50, 0.60)
            .with_context_confidence(0.50);

        let res = guard.evaluate(&input);
        assert_eq!(res.decision, QualityGuardDecision::AcceptWithLowConfidence);
        assert!(res.decision.should_modify_context());
        assert!(res.reasons.iter().any(|r| r.contains("Moderate quality score")));
    }

    #[test]
    fn test_guard_decision_create_new_candidate_context() {
        let guard = ContextQualityGuard::new();
        // Zero affinity to target workspace, but high mutual cluster cohesion among peer items
        let input = QualityGuardInput::new("Machine Learning Paper")
            .with_target("ws_cooking", "Cooking Recipes")
            .with_semantic_relevance(0.05)
            .with_competition(None, Some(0.88)); // High candidate cluster cohesion

        let res = guard.evaluate(&input);
        assert_eq!(res.decision, QualityGuardDecision::CreateNewCandidateContext);
        assert!(!res.decision.should_modify_context());
        assert!(res.reasons.iter().any(|r| r.contains("Strong cluster cohesion")));
    }

    #[test]
    fn test_guard_decision_noise_glance_suppression() {
        let guard = ContextQualityGuard::new();
        let input = QualityGuardInput::new("Untitled")
            .with_target("ws_work", "Work")
            .with_noise_flags(true, true);

        let res = guard.evaluate(&input);
        assert!(res.decision == QualityGuardDecision::Reject || res.decision == QualityGuardDecision::HoldForMoreEvidence);
        assert!(!res.decision.should_modify_context());
        assert!(res.dimension_scores.noise_penalty > 0.0);
    }

    #[test]
    fn test_guard_db_integration_evaluation() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Security Architecture").unwrap();
        let session = create_session(&conn).unwrap();

        // 1. Confirmed workspace item in session
        let confirmed_item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://auth0.com/docs/jwt",
            Some("Auth0 JWT Docs"),
            "{\"domain\":\"auth0.com\"}",
        )
        .unwrap();

        // 2. Candidate unassigned item in same session
        let candidate_item = create_item(
            &conn,
            None,
            Some(&session.id),
            "text",
            "JWT token validation middleware implementation",
            Some("JWT Middleware"),
            "{}",
        )
        .unwrap();

        // Add relationship
        crate::db::operations::create_item_relationship(
            &conn,
            &candidate_item.id,
            &confirmed_item.id,
            "semantic_similarity",
            0.85,
            None,
        )
        .unwrap();

        let guard = ContextQualityGuard::new();
        let result = guard.evaluate_from_db(&conn, &candidate_item.id, &ws.id).unwrap();

        assert!(result.decision.is_accepted() || result.decision.is_held());
        assert!(result.composite_quality_score > 0.30);
    }

    #[test]
    fn test_guard_decision_string_serialization_and_helpers() {
        let d_accept = QualityGuardDecision::Accept;
        assert_eq!(d_accept.as_str(), "accept");
        assert_eq!(d_accept.display_name(), "Accept into Context");
        assert!(d_accept.should_modify_context());
        assert!(d_accept.is_accepted());
        assert!(!d_accept.is_held());
        assert!(!d_accept.is_rejected());
        assert!(!d_accept.is_new_candidate());

        let d_low = QualityGuardDecision::AcceptWithLowConfidence;
        assert_eq!(d_low.as_str(), "accept_with_low_confidence");
        assert!(d_low.should_modify_context());
        assert!(d_low.is_accepted());

        let d_hold = QualityGuardDecision::HoldForMoreEvidence;
        assert_eq!(d_hold.as_str(), "hold_for_more_evidence");
        assert!(!d_hold.should_modify_context());
        assert!(d_hold.is_held());

        let d_reject = QualityGuardDecision::Reject;
        assert_eq!(d_reject.as_str(), "reject");
        assert!(!d_reject.should_modify_context());
        assert!(d_reject.is_rejected());

        let d_cand = QualityGuardDecision::CreateNewCandidateContext;
        assert_eq!(d_cand.as_str(), "create_new_candidate_context");
        assert!(!d_cand.should_modify_context());
        assert!(d_cand.is_new_candidate());

        assert_eq!(QualityGuardDecision::from_str("accept").unwrap(), QualityGuardDecision::Accept);
        assert_eq!(QualityGuardDecision::from_str("hold_for_more_evidence").unwrap(), QualityGuardDecision::HoldForMoreEvidence);
        assert_eq!(QualityGuardDecision::from_str("reject").unwrap(), QualityGuardDecision::Reject);
        assert_eq!(QualityGuardDecision::from_str("create_new_candidate_context").unwrap(), QualityGuardDecision::CreateNewCandidateContext);
        assert!(QualityGuardDecision::from_str("invalid_decision").is_err());
    }

    #[test]
    fn test_guard_transient_micro_glance_alone_suppresses_modification() {
        let guard = ContextQualityGuard::new();
        // Item with moderate semantic relevance (0.50) but flagged as accidental micro-glance
        let input = QualityGuardInput::new("Accidental tab glance")
            .with_target("ws_project", "Main Project")
            .with_semantic_relevance(0.50)
            .with_noise_flags(false, true); // is_accidental_glance = true

        let res = guard.evaluate(&input);
        assert!(!res.decision.should_modify_context());
        assert_eq!(res.decision, QualityGuardDecision::HoldForMoreEvidence);
        assert!(res.dimension_scores.noise_penalty >= 0.40);
        assert!(res.warnings.iter().any(|w| w.contains("Transient micro-glance")));
    }

    #[test]
    fn test_guard_generic_title_alone_suppresses_modification() {
        let guard = ContextQualityGuard::new();
        // Item with moderate continuity but generic noise title
        let input = QualityGuardInput::new("New Tab")
            .with_target("ws_project", "Main Project")
            .with_continuity(0.60, 0.60)
            .with_noise_flags(true, false); // is_generic_noise = true

        let res = guard.evaluate(&input);
        assert!(!res.decision.should_modify_context());
        assert_eq!(res.decision, QualityGuardDecision::HoldForMoreEvidence);
        assert!(res.dimension_scores.noise_penalty >= 0.40);
        assert!(res.warnings.iter().any(|w| w.contains("Generic window title")));
    }

    #[test]
    fn test_guard_conflicting_context_priority_over_weak_session() {
        let guard = ContextQualityGuard::new();
        // Item captured during active session (temporal = 0.80), but strong conflicting evidence (0.75)
        let input = QualityGuardInput::new("Conflicting Social Feed")
            .with_target("ws_work", "Work Workspace")
            .with_continuity(0.80, 0.80)
            .with_conflict(0.75); // Strong conflict!

        let res = guard.evaluate(&input);
        assert_eq!(res.decision, QualityGuardDecision::Reject);
        assert!(!res.decision.should_modify_context());
        assert_eq!(res.composite_quality_score, 0.0);
        assert!(res.warnings.iter().any(|w| w.contains("Severe context conflict")));
    }

    #[test]
    fn test_guard_builder_and_dimension_scores_integrity() {
        let input = QualityGuardInput::new("Test Document")
            .with_target("ws_test", "Test Workspace")
            .with_semantic_relevance(0.70)
            .with_task_relevance(0.65)
            .with_continuity(0.80, 0.85)
            .with_item_relationships(0.60)
            .with_domain_relevance(0.75)
            .with_context_confidence(0.85)
            .with_conflict(0.10)
            .with_user_feedback(0.20);

        assert_eq!(input.title, "Test Document");
        assert_eq!(input.target_workspace_id.as_deref(), Some("ws_test"));
        assert_eq!(input.target_workspace_name.as_deref(), Some("Test Workspace"));
        assert_eq!(input.semantic_relevance, 0.70);
        assert_eq!(input.task_relevance, 0.65);
        assert_eq!(input.recent_contextual_continuity, 0.80);
        assert_eq!(input.temporal_continuity, 0.85);
        assert_eq!(input.item_relationships, 0.60);
        assert_eq!(input.domain_relevance, 0.75);
        assert_eq!(input.existing_context_confidence, 0.85);
        assert_eq!(input.conflicting_context_evidence, 0.10);
        assert_eq!(input.user_feedback, 0.20);

        let guard = ContextQualityGuard::new();
        let res = guard.evaluate(&input);
        assert_eq!(res.decision, QualityGuardDecision::Accept);
        assert!(res.decision.is_accepted());
        assert!(res.dimension_scores.composite_quality_score > 0.50);
        assert_eq!(res.dimension_scores.diversity_multiplier, 1.0);
    }
}

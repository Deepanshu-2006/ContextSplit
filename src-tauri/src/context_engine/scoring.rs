use rusqlite::Connection;
use serde::{Serialize, Deserialize};
use std::cmp::Ordering;

use crate::context_engine::evidence::{
    ContextEvidenceCollector, ContextEvidenceSet,
    EvidenceCategory,
};
use crate::context_engine::diversity::{
    EvidenceDiversityCalculator, EvidenceDiversityResult,
};
use crate::context_engine::contradiction::{
    ContradictionAnalyzer, ContextContradictionReport,
};
use crate::context_engine::competition::{
    AmbiguityState, ContextCandidateSummary, ContextCompetitionAnalysis,
    ContextCompetitionAnalyzer,
};
use crate::context_engine::calibration::{
    CalibrationInput, ConfidenceBand, ConfidenceCalibrator,
};

/// Structured contribution of an individual evidence category to the final context score.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceContribution {
    pub category: String,
    pub raw_strength: f32,
    pub weighted_score: f32,
    pub description: String,
    pub is_positive: bool,
}

/// Structured record of contradictory user evidence or lifecycle suppression.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContradictoryEvidence {
    pub category: String,
    pub description: String,
    pub penalty: f32,
}

/// Diagnostic factors detailing how confidence was calculated from evidence diversity,
/// reliability, agreement, ambiguity, evidence volume, and contradiction penalties.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfidenceFactors {
    /// Number of distinct independent evidence categories supporting this context
    pub diversity_count: usize,
    /// Diversity multiplier applied to raw score (0.35 - 1.0)
    pub diversity_multiplier: f32,
    /// Weighted average reliability score of contributing evidence categories (0.0 - 1.0)
    pub reliability_score: f32,
    /// Agreement proportion: relative support for this context vs other candidate contexts (0.0 - 1.0)
    pub agreement_score: f32,
    /// Ambiguity penalty discount (0.0 if unambiguous, > 0.0 if competing with close runner-up)
    pub ambiguity_penalty: f32,
    /// Evidence volume score (0.0 - 1.0)
    pub evidence_volume_score: f32,
    /// Total count of positive evidence items (evidence quantity)
    #[serde(default)]
    pub evidence_quantity: usize,
    /// Number of distinct granular categories
    #[serde(default)]
    pub evidence_categories_count: usize,
    /// Number of independent conceptual evidence groups
    #[serde(default)]
    pub independent_groups_count: usize,
    /// Normalized diversity score in [0.0, 1.0]
    #[serde(default)]
    pub diversity_score: f32,
    /// Detailed structured diversity calculation result
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diversity_result: Option<EvidenceDiversityResult>,
    /// Contradiction strength level [0.0, 1.0]
    #[serde(default)]
    pub contradiction_strength: f32,
    /// Total penalty discount deducted from confidence due to contradictions [0.0, 1.0]
    #[serde(default)]
    pub contradiction_penalty: f32,
    /// Absolute score difference between top candidate and runner-up
    #[serde(default)]
    pub score_margin: Option<f32>,
    /// Relative confidence taking competing candidates into account [0.0, 1.0]
    #[serde(default)]
    pub relative_confidence: Option<f32>,
    /// Explicit ambiguity state classification
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ambiguity_state: Option<AmbiguityState>,
    /// Explicit 5-tier calibrated confidence band
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub band: Option<ConfidenceBand>,
}

/// Comprehensive, structured scoring information for a candidate context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextScoreResult {
    pub context_id: String,
    pub context_name: String,
    pub total_score: f32,
    pub confidence: f32,
    pub confidence_band: String,
    pub evidence_contributions: Vec<EvidenceContribution>,
    pub strongest_reasons: Vec<String>,
    pub contradictory_evidence: Vec<ContradictoryEvidence>,
    pub uncertainty_indicators: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence_factors: Option<ConfidenceFactors>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contradiction_report: Option<ContextContradictionReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub competition_analysis: Option<ContextCompetitionAnalysis>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calibrated_band: Option<ConfidenceBand>,
}

impl ContextScoreResult {
    pub fn to_workspace_score(&self) -> WorkspaceScore {
        WorkspaceScore {
            workspace_id: self.context_id.clone(),
            total_score: self.total_score,
            confidence: self.confidence,
            confidence_band: self.confidence_band.clone(),
            reasons: self.strongest_reasons.clone(),
            score_result: Some(self.clone()),
        }
    }
}

/// Backward-compatible WorkspaceScore model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceScore {
    pub workspace_id: String,
    pub total_score: f32,
    pub confidence: f32,
    pub confidence_band: String,
    pub reasons: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_result: Option<ContextScoreResult>,
}

/// Canonical thresholds and margins for contextual scoring and confidence evaluation.
pub const CONFIDENCE_BAND_HIGH: f32 = 0.70;
pub const CONFIDENCE_BAND_MEDIUM: f32 = 0.35;
pub const CONFIDENCE_BAND_LOW: f32 = 0.15;
pub const AMBIGUITY_MARGIN: f32 = 0.15;
pub const AMBIGUITY_RUNNER_UP_FLOOR: f32 = 0.20;

/// Explicit, modular, and configurable scoring weights for the Context Engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringWeights {
    // Confirmed membership
    pub confirmed_membership_weight: f32,

    // Historical user organization
    pub workspace_history: f32,

    // Graph relationships
    pub related_item_per_match: f32,
    pub related_item_max: f32,

    // Semantic similarity
    pub semantic_similarity_max: f32,
    pub workspace_profile_similarity_max: f32,

    // Session
    pub same_session_per_match: f32,
    pub same_session_max: f32,

    // Weak signals with explicit anti-dominance caps
    pub same_source_app_per_match: f32,
    pub same_source_app_max: f32,
    pub same_domain_per_match: f32,
    pub same_domain_max: f32,
    pub same_content_type_per_match: f32,
    pub same_content_type_max: f32,

    // Contradiction and user feedback penalties
    pub user_feedback_dismissal_penalty: f32,

    // Anti-dominance cap for isolated weak signals
    pub weak_signals_isolated_cap: f32,

    // Confidence thresholds
    pub threshold_high: f32,
    pub threshold_medium: f32,
    pub threshold_low: f32,

    // Temporal decay configuration
    pub temporal_decay: crate::context_engine::temporal::TemporalDecayConfig,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            confirmed_membership_weight: 1.0,
            workspace_history: 0.4,
            related_item_per_match: 0.2,
            related_item_max: 0.5,
            semantic_similarity_max: 0.4,
            workspace_profile_similarity_max: 0.45,
            same_session_per_match: 0.1,
            same_session_max: 0.3,
            same_source_app_per_match: 0.05,
            same_source_app_max: 0.2,
            same_domain_per_match: 0.1,
            same_domain_max: 0.3,
            same_content_type_per_match: 0.02,
            same_content_type_max: 0.1,
            user_feedback_dismissal_penalty: 0.50,
            weak_signals_isolated_cap: 0.24,
            threshold_high: CONFIDENCE_BAND_HIGH,
            threshold_medium: CONFIDENCE_BAND_MEDIUM,
            threshold_low: CONFIDENCE_BAND_LOW,
            temporal_decay: crate::context_engine::temporal::TemporalDecayConfig::default(),
        }
    }
}

/// In-memory contextual representation of a candidate workspace for scoring.
#[derive(Debug, Clone, Default)]
pub struct WorkspaceScoringContext {
    pub id: String,
    pub name: String,
    pub lifecycle_state: String,
    pub item_ids: Vec<String>,
    pub item_types: Vec<String>,
    pub app_names: Vec<String>,
    pub domains: Vec<String>,
    pub sessions: Vec<String>,
    pub profile_embedding: Option<Vec<f32>>,
    pub profile_provider: Option<String>,
    pub profile_model: Option<String>,
}

impl WorkspaceScoringContext {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            lifecycle_state: "active".to_string(),
            item_ids: Vec::new(),
            item_types: Vec::new(),
            app_names: Vec::new(),
            domains: Vec::new(),
            sessions: Vec::new(),
            profile_embedding: None,
            profile_provider: None,
            profile_model: None,
        }
    }

    pub fn with_lifecycle_state(mut self, state: impl Into<String>) -> Self {
        self.lifecycle_state = state.into();
        self
    }
}

/// Centralized scoring engine that consumes `ContextEvidence` and applies the deterministic scoring pipeline:
///
/// ContextEvidence
///      ↓
/// Evidence Normalization
///      ↓
/// Evidence Weighting (Configurable ScoringWeights, Anti-Dominance Caps, Contradiction Penalties)
///      ↓
/// Context Score (Bounded [0.0, 1.0], Lifecycle Modifiers)
///      ↓
/// Confidence (Confidence Bands: High / Medium / Low)
///      ↓
/// Explanation (Structured Contributions, Strongest Reasons, Contradictory Evidence, Uncertainty Indicators)
#[derive(Debug, Clone)]
pub struct ContextEvidenceScorer {
    pub weights: ScoringWeights,
    pub enable_semantic: bool,
}

impl Default for ContextEvidenceScorer {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextEvidenceScorer {
    pub fn new() -> Self {
        Self {
            weights: ScoringWeights::default(),
            enable_semantic: true,
        }
    }

    pub fn with_weights(weights: ScoringWeights) -> Self {
        Self {
            weights,
            enable_semantic: true,
        }
    }

    pub fn with_semantic_enabled(mut self, enabled: bool) -> Self {
        self.enable_semantic = enabled;
        self
    }

    /// Evaluates a normalized `ContextEvidenceSet` against a single workspace context.
    pub fn score_evidence_against_workspace(
        &self,
        evidence_set: &ContextEvidenceSet,
        ws: &WorkspaceScoringContext,
    ) -> ContextScoreResult {
        let mut contributions: Vec<EvidenceContribution> = Vec::new();
        let mut contradictory: Vec<ContradictoryEvidence> = Vec::new();
        let mut uncertainty: Vec<String> = Vec::new();
        let mut strongest_reasons: Vec<String> = Vec::new();

        let mut has_strong_signal = false;
        let mut only_weak_signals = true;

        // ---------------------------------------------------------------------
        // 1. Confirmed Membership (Observed, Authoritative)
        // ---------------------------------------------------------------------
        for ev in evidence_set.by_category(EvidenceCategory::ConfirmedMembership) {
            let is_match = ev.source_ref.as_ref().map(|r| r.entity_id == ws.id).unwrap_or(false)
                || ev.attributes.get("workspace_id").and_then(|v| v.as_str()) == Some(&ws.id);

            if is_match {
                has_strong_signal = true;
                only_weak_signals = false;
                let weighted = (self.weights.confirmed_membership_weight * ev.strength as f32).min(1.0);
                contributions.push(EvidenceContribution {
                    category: "confirmed_membership".to_string(),
                    raw_strength: ev.strength as f32,
                    weighted_score: weighted,
                    description: format!("Explicit user assignment to {}", ws.name),
                    is_positive: true,
                });
                strongest_reasons.push(format!("Explicit user assignment to {}", ws.name));
            }
        }

        // ---------------------------------------------------------------------
        // 2. Workspace History (User Actions with Temporal Decay)
        // ---------------------------------------------------------------------
        for ev in evidence_set.by_category(EvidenceCategory::WorkspaceHistory) {
            let is_match = ev.source_ref.as_ref().map(|r| r.entity_id == ws.id).unwrap_or(false)
                || ev.attributes.get("workspace_id").and_then(|v| v.as_str()) == Some(&ws.id);

            if is_match {
                has_strong_signal = true;
                only_weak_signals = false;
                let decay_weight = ev.strength as f32;
                let weighted = self.weights.workspace_history * decay_weight;
                let reason = format!("User previously organized similar Items into {}", ws.name);

                contributions.push(EvidenceContribution {
                    category: "workspace_history".to_string(),
                    raw_strength: decay_weight,
                    weighted_score: weighted,
                    description: reason.clone(),
                    is_positive: true,
                });
                strongest_reasons.push(reason);
                break; // Only apply primary history once
            }
        }

        // ---------------------------------------------------------------------
        // 3. Item Relationships (Graph Edges & Temporal Proximity)
        // ---------------------------------------------------------------------
        let mut rel_score_sum = 0.0_f32;
        let mut rel_match_count = 0_usize;

        for ev in evidence_set.items.iter().filter(|e| e.category == EvidenceCategory::RelatedItem || e.category == EvidenceCategory::TemporalProximity) {
            let other_id = ev.attributes.get("other_item_id").and_then(|v| v.as_str())
                .or_else(|| ev.source_ref.as_ref().map(|r| r.entity_id.as_str()));

            if let Some(oid) = other_id {
                let belongs_to_ws = ws.item_ids.iter().any(|id| id == oid)
                    || ev.attributes.get("workspace_id").and_then(|v| v.as_str()) == Some(&ws.id)
                    || ev.attributes.get("workspace_name").and_then(|v| v.as_str()) == Some(&ws.name);

                if belongs_to_ws {
                    rel_score_sum += ev.strength as f32;
                    rel_match_count += 1;
                }
            }
        }

        if rel_match_count > 0 {
            has_strong_signal = true;
            only_weak_signals = false;
            let weighted = (rel_score_sum * self.weights.related_item_per_match).min(self.weights.related_item_max);
            let reason = format!("{} related Items already belong to {}", rel_match_count, ws.name);
            contributions.push(EvidenceContribution {
                category: "related_items".to_string(),
                raw_strength: (rel_score_sum / rel_match_count as f32).min(1.0),
                weighted_score: weighted,
                description: reason.clone(),
                is_positive: true,
            });
            strongest_reasons.push(reason);
        }

        // ---------------------------------------------------------------------
        // 4. Semantic Similarity (Item-Item and Workspace Semantic Profile Centroid)
        // ---------------------------------------------------------------------
        if self.enable_semantic {
            let mut max_item_semantic = 0.0_f32;
            let mut profile_semantic = 0.0_f32;

            for ev in evidence_set.by_category(EvidenceCategory::SemanticSimilarity) {
                if let Some(prof_sim) = ev.attributes.get("profile_similarity").and_then(|v| v.as_f64()) {
                    let target_ws_id = ev.attributes.get("workspace_id").and_then(|v| v.as_str())
                        .or_else(|| ev.source_ref.as_ref().map(|r| r.entity_id.as_str()));
                    if target_ws_id == Some(&ws.id) && prof_sim as f32 > profile_semantic {
                        profile_semantic = prof_sim as f32;
                    }
                } else {
                    let other_id = ev.attributes.get("other_item_id").and_then(|v| v.as_str())
                        .or_else(|| ev.source_ref.as_ref().map(|r| r.entity_id.as_str()));
                    let belongs_to_ws = if let Some(oid) = other_id {
                        ws.item_ids.iter().any(|id| id == oid)
                    } else {
                        false
                    } || ev.attributes.get("workspace_id").and_then(|v| v.as_str()) == Some(&ws.id)
                      || ev.attributes.get("workspace_name").and_then(|v| v.as_str()) == Some(&ws.name);

                    if belongs_to_ws && (ev.strength as f32) > max_item_semantic {
                        max_item_semantic = ev.strength as f32;
                    }
                }
            }

            if max_item_semantic > 0.0 {
                has_strong_signal = true;
                only_weak_signals = false;
                let weighted = max_item_semantic.min(self.weights.semantic_similarity_max);
                let reason = format!("Semantic similarity with {} Items: {:.2}", ws.name, max_item_semantic);
                contributions.push(EvidenceContribution {
                    category: "semantic_similarity_items".to_string(),
                    raw_strength: max_item_semantic,
                    weighted_score: weighted,
                    description: reason.clone(),
                    is_positive: true,
                });
                strongest_reasons.push(reason);
            }

            if profile_semantic > 0.0 {
                has_strong_signal = true;
                only_weak_signals = false;
                let weighted = profile_semantic.min(self.weights.workspace_profile_similarity_max);
                let reason = format!("Matches {} semantic profile: {:.2}", ws.name, profile_semantic);
                contributions.push(EvidenceContribution {
                    category: "semantic_profile".to_string(),
                    raw_strength: profile_semantic,
                    weighted_score: weighted,
                    description: reason.clone(),
                    is_positive: true,
                });
                strongest_reasons.push(reason);
            }
        }

        // ---------------------------------------------------------------------
        // 5. Same Session (Observed Co-Session Overlap)
        // ---------------------------------------------------------------------
        let mut session_match_count = 0_usize;
        for ev in evidence_set.by_category(EvidenceCategory::SameSession) {
            let session_id = ev.attributes.get("session_id").and_then(|v| v.as_str())
                .or_else(|| ev.source_ref.as_ref().map(|r| r.entity_id.as_str()));

            if let Some(s_id) = session_id {
                session_match_count += ws.sessions.iter().filter(|&s| s == s_id).count();
            }
        }

        if session_match_count > 0 {
            has_strong_signal = true;
            only_weak_signals = false;
            let weighted = (session_match_count as f32 * self.weights.same_session_per_match).min(self.weights.same_session_max);
            let reason = "Same Context Session".to_string();
            contributions.push(EvidenceContribution {
                category: "same_session".to_string(),
                raw_strength: (session_match_count as f32 / (session_match_count as f32 + 1.0)).min(1.0),
                weighted_score: weighted,
                description: reason.clone(),
                is_positive: true,
            });
            strongest_reasons.push(reason);
        }

        // ---------------------------------------------------------------------
        // 6. Source Application (Weak Signal with Anti-Dominance Cap)
        // ---------------------------------------------------------------------
        let mut app_match_count = 0_usize;
        let mut matched_app_name: Option<String> = None;

        for ev in evidence_set.by_category(EvidenceCategory::SourceApp) {
            if let Some(app) = ev.attributes.get("app_name").and_then(|v| v.as_str()) {
                let matches = ws.app_names.iter().filter(|&a| a == app).count();
                if matches > 0 {
                    app_match_count += matches;
                    if matched_app_name.is_none() {
                        matched_app_name = Some(app.to_string());
                    }
                }
            }
        }

        if app_match_count > 0 {
            let app_str = matched_app_name.unwrap_or_else(|| "Source".to_string());
            let weighted = (app_match_count as f32 * self.weights.same_source_app_per_match).min(self.weights.same_source_app_max);
            let reason = format!("Same {} source app", app_str);
            contributions.push(EvidenceContribution {
                category: "source_app".to_string(),
                raw_strength: (app_match_count as f32 * 0.1).min(1.0),
                weighted_score: weighted,
                description: reason.clone(),
                is_positive: true,
            });
            strongest_reasons.push(reason);
        }

        // ---------------------------------------------------------------------
        // 7. Browser Domain (Weak Signal with Anti-Dominance Cap)
        // ---------------------------------------------------------------------
        let mut domain_match_count = 0_usize;
        let mut matched_domain_pretty: Option<String> = None;

        for ev in evidence_set.by_category(EvidenceCategory::BrowserDomain) {
            if let Some(domain) = ev.attributes.get("domain").and_then(|v| v.as_str()) {
                let matches = ws.domains.iter().filter(|&d| d == domain).count();
                if matches > 0 {
                    domain_match_count += matches;
                    if matched_domain_pretty.is_none() {
                        let display_domain = domain.strip_prefix("www.").unwrap_or(domain);
                        let domain_name = display_domain.split('.').next().unwrap_or(domain);
                        let mut chars = domain_name.chars();
                        let pretty = match chars.next() {
                            None => String::new(),
                            Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
                        };
                        matched_domain_pretty = Some(if pretty.is_empty() { domain.to_string() } else { pretty });
                    }
                }
            }
        }

        if domain_match_count > 0 {
            let dom_str = matched_domain_pretty.unwrap_or_else(|| "Web".to_string());
            let weighted = (domain_match_count as f32 * self.weights.same_domain_per_match).min(self.weights.same_domain_max);
            let reason = format!("Same {} domain", dom_str);
            contributions.push(EvidenceContribution {
                category: "browser_domain".to_string(),
                raw_strength: (domain_match_count as f32 * 0.2).min(1.0),
                weighted_score: weighted,
                description: reason.clone(),
                is_positive: true,
            });
            strongest_reasons.push(reason);
        }

        // ---------------------------------------------------------------------
        // 8. Content Type (Minor Signal)
        // ---------------------------------------------------------------------
        let mut type_match_count = 0_usize;
        for ev in evidence_set.by_category(EvidenceCategory::ContentType) {
            if let Some(c_type) = ev.attributes.get("content_type").and_then(|v| v.as_str()) {
                let matches = ws.item_types.iter().filter(|&t| t == c_type).count();
                if matches > 0 {
                    type_match_count += matches;
                }
            }
        }

        if type_match_count > 0 {
            let weighted = (type_match_count as f32 * self.weights.same_content_type_per_match).min(self.weights.same_content_type_max);
            let reason = format!("Shares content type with {} items in {}", type_match_count, ws.name);
            contributions.push(EvidenceContribution {
                category: "content_type".to_string(),
                raw_strength: (type_match_count as f32 * 0.05).min(1.0),
                weighted_score: weighted,
                description: reason.clone(),
                is_positive: true,
            });
            strongest_reasons.push(reason);
        }

        // ---------------------------------------------------------------------
        // 9. User Feedback (Contradictory / Negative Evidence)
        // ---------------------------------------------------------------------
        for ev in evidence_set.by_category(EvidenceCategory::UserFeedback) {
            let is_match = ev.source_ref.as_ref().map(|r| r.entity_id == ws.id).unwrap_or(false)
                || ev.attributes.get("workspace_id").and_then(|v| v.as_str()) == Some(&ws.id);

            if is_match {
                let feedback_type = ev.attributes.get("feedback_type").and_then(|v| v.as_str()).unwrap_or("user_dismissal");
                let penalty = self.weights.user_feedback_dismissal_penalty;
                let description = format!("User previously rejected suggestion ({}) for {}", feedback_type, ws.name);

                contradictory.push(ContradictoryEvidence {
                    category: "user_feedback".to_string(),
                    description: description.clone(),
                    penalty,
                });
                uncertainty.push(format!("Prior negative user feedback against suggestion for {}", ws.name));
            }
        }

        // ---------------------------------------------------------------------
        // 10. Anti-Dominance Invariant Enforcement
        // ---------------------------------------------------------------------
        // If NO strong signal exists and only weak signals are present, cap total score
        // so that a single app, domain, content type or recent timestamp cannot dominate.
        let mut raw_positive_score: f32 = contributions.iter().filter(|c| c.is_positive).map(|c| c.weighted_score).sum();

        if !has_strong_signal && only_weak_signals && raw_positive_score > 0.0 {
            if raw_positive_score > self.weights.weak_signals_isolated_cap {
                raw_positive_score = self.weights.weak_signals_isolated_cap;
            }
            uncertainty.push("Only weak peripheral signals (application, domain, or content type) detected without session, graph, or semantic backing.".to_string());
        }

        // Apply contradictory penalties
        let total_penalty: f32 = contradictory.iter().map(|c| c.penalty).sum();
        let mut total_score = (raw_positive_score - total_penalty).max(0.0);

        // ---------------------------------------------------------------------
        // 11. Lifecycle State Modifiers
        // ---------------------------------------------------------------------
        if ws.lifecycle_state == "archived" {
            total_score = 0.0;
            let reason = format!("{} is archived by user; suggestions suppressed", ws.name);
            contradictory.push(ContradictoryEvidence {
                category: "lifecycle_state".to_string(),
                description: reason.clone(),
                penalty: 1.0,
            });
            strongest_reasons.push(reason);
        } else if ws.lifecycle_state == "dormant" {
            total_score *= 0.50;
            let reason = format!("{} is dormant (no recent meaningful activity)", ws.name);
            uncertainty.push(format!("{} is dormant; suggestions discounted by 50%", ws.name));
            strongest_reasons.push(reason);
        }

        total_score = total_score.clamp(0.0, 1.0);

        let contradiction_report = ContradictionAnalyzer::new().analyze_candidate(
            evidence_set,
            ws,
            &[ws.clone()],
            total_score,
            &contributions,
            None,
        );

        for c in &contradiction_report.contradictory_evidence {
            if !contradictory.iter().any(|existing| existing.category == c.category && existing.description == c.description) {
                contradictory.push(c.clone());
            }
        }

        if let Some(ref expl) = contradiction_report.contradiction_explanation {
            if !uncertainty.contains(expl) {
                uncertainty.push(expl.clone());
            }
        }

        // Compute Confidence separated from score
        let (confidence, confidence_band, confidence_factors) = evaluate_confidence_with_contradiction(
            &contributions,
            total_score,
            None,
            None,
            None,
            contradiction_report.contradiction_strength,
            contradiction_report.confidence_penalty,
        );

        if confidence_factors.diversity_count <= 1 && total_score >= 0.40 {
            uncertainty.push("High affinity score relies on a single evidence category; confidence is limited until corroborated by independent signals.".to_string());
        }

        let competition_analysis = Some(ContextCompetitionAnalyzer::new().analyze_candidates(&[ContextCandidateSummary {
            context_id: ws.id.clone(),
            context_name: ws.name.clone(),
            score: total_score,
            confidence,
            confidence_band: confidence_band.clone(),
        }]));

        let cal_band = ConfidenceBand::from_str_lenient(&confidence_band);

        ContextScoreResult {
            context_id: ws.id.clone(),
            context_name: ws.name.clone(),
            total_score,
            confidence,
            confidence_band,
            evidence_contributions: contributions,
            strongest_reasons,
            contradictory_evidence: contradictory,
            uncertainty_indicators: uncertainty,
            confidence_factors: Some(confidence_factors),
            contradiction_report: Some(contradiction_report),
            competition_analysis,
            calibrated_band: Some(cal_band),
        }
    }

    /// Evaluates an evidence set against multiple candidate workspaces, sorted descending by score.
    /// In a second pass, calibrates confidence using candidate margin, runner-up score, agreement across contexts,
    /// cross-context contradiction analysis, and contextual competition analysis.
    pub fn score_evidence(
        &self,
        evidence_set: &ContextEvidenceSet,
        workspaces: &[WorkspaceScoringContext],
    ) -> Vec<ContextScoreResult> {
        let mut results: Vec<ContextScoreResult> = workspaces
            .iter()
            .map(|ws| self.score_evidence_against_workspace(evidence_set, ws))
            .collect();

        // Sort descending by total_score
        results.sort_by(|a, b| b.total_score.partial_cmp(&a.total_score).unwrap_or(Ordering::Equal));

        let num_workspaces = results.len();
        if num_workspaces == 0 {
            return results;
        }

        let total_score_sum: f32 = results.iter().map(|r| r.total_score).sum();

        let candidate_scores: Vec<(String, f32, Vec<EvidenceContribution>)> = results
            .iter()
            .map(|r| (r.context_id.clone(), r.total_score, r.evidence_contributions.clone()))
            .collect();

        let contradiction_analysis = ContradictionAnalyzer::new().analyze_all(
            evidence_set,
            workspaces,
            &candidate_scores,
        );

        // Pre-compute initial competition analysis across candidate scores
        let initial_competition = ContextCompetitionAnalyzer::new().analyze(&results);

        // Second pass: Calibrate confidence with margin, runner-up, agreement, contradiction, and competition analysis
        for i in 0..num_workspaces {
            let (margin_to_runner_up, runner_up_score, runner_up_name) = if i == 0 {
                if num_workspaces > 1 {
                    (
                        Some((results[0].total_score - results[1].total_score).max(0.0)),
                        Some(results[1].total_score),
                        Some(results[1].context_name.clone()),
                    )
                } else {
                    (None, None, None)
                }
            } else {
                (
                    Some((results[0].total_score - results[i].total_score).abs()),
                    Some(results[0].total_score),
                    Some(results[0].context_name.clone()),
                )
            };

            let agreement_score = if total_score_sum > 0.0 {
                Some(results[i].total_score / total_score_sum)
            } else {
                Some(1.0)
            };

            let report = contradiction_analysis.reports.get(&results[i].context_id).cloned();
            let (c_strength, c_penalty) = match &report {
                Some(rep) => (rep.contradiction_strength, rep.confidence_penalty),
                None => (0.0, 0.0),
            };

            if let Some(ref rep) = report {
                for c in &rep.contradictory_evidence {
                    if !results[i].contradictory_evidence.iter().any(|existing| existing.category == c.category && existing.description == c.description) {
                        results[i].contradictory_evidence.push(c.clone());
                    }
                }
                if let Some(ref expl) = rep.contradiction_explanation {
                    if !results[i].uncertainty_indicators.contains(expl) {
                        results[i].uncertainty_indicators.push(expl.clone());
                    }
                }
            }

            let (conf, band, mut factors) = evaluate_confidence_with_contradiction(
                &results[i].evidence_contributions,
                results[i].total_score,
                margin_to_runner_up,
                runner_up_score,
                agreement_score,
                c_strength,
                c_penalty,
            );

            // Populate competition factors
            factors.score_margin = margin_to_runner_up;
            factors.ambiguity_state = Some(initial_competition.ambiguity_state);
            factors.relative_confidence = Some(
                ContextCompetitionAnalyzer::calculate_relative_confidence(
                    results[i].total_score,
                    runner_up_score.unwrap_or(0.0),
                    conf,
                ),
            );

            let cal_band = ConfidenceBand::from_str_lenient(&band);
            results[i].confidence = conf;
            results[i].confidence_band = band;
            results[i].confidence_factors = Some(factors);
            results[i].contradiction_report = report;
            results[i].calibrated_band = Some(cal_band);

            if let Some(r_name) = runner_up_name {
                if let Some(margin) = margin_to_runner_up {
                    if margin <= AMBIGUITY_MARGIN && results[i].total_score >= AMBIGUITY_RUNNER_UP_FLOOR {
                        let msg = format!("Ambiguous context assignment: competing closely with candidate '{}' (margin {:.2})", r_name, margin);
                        if !results[i].uncertainty_indicators.contains(&msg) {
                            results[i].uncertainty_indicators.push(msg);
                        }
                    }
                }
            }
        }

        // Finalize competition analysis with calibrated candidate results
        let final_competition = ContextCompetitionAnalyzer::new().analyze(&results);
        for res in &mut results {
            res.competition_analysis = Some(final_competition.clone());
        }

        results
    }
}

/// Maps an evidence category string into an independent conceptual category group for diversity calculations.
pub fn map_category_group(category: &str) -> &'static str {
    EvidenceDiversityCalculator::map_category_to_group(category).as_str()
}

/// Category reliability ratings based on verifiability and noise profile.
pub fn category_reliability(category: &str) -> f32 {
    match category {
        "confirmed_membership" => 1.00,
        "user_feedback" => 0.95,
        "same_session" => 0.85,
        "workspace_history" => 0.80,
        "related_items" => 0.75,
        "semantic_similarity_items" | "semantic_profile" => 0.70,
        "browser_domain" => 0.55,
        "source_app" => 0.45,
        "temporal_proximity" => 0.40,
        "content_type" => 0.35,
        _ => 0.50,
    }
}

/// Evaluates confidence as a distinct, principled assessment of conclusion trustworthiness.
pub fn evaluate_confidence(
    contributions: &[EvidenceContribution],
    total_score: f32,
    margin_to_runner_up: Option<f32>,
    runner_up_score: Option<f32>,
    agreement_score: Option<f32>,
) -> (f32, String, ConfidenceFactors) {
    evaluate_confidence_with_contradiction(
        contributions,
        total_score,
        margin_to_runner_up,
        runner_up_score,
        agreement_score,
        0.0,
        0.0,
    )
}

/// Evaluates confidence incorporating evidence diversity, reliability, volume, agreement,
/// ambiguity margin, and contradictory evidence penalties via the centralized ConfidenceCalibrator.
pub fn evaluate_confidence_with_contradiction(
    contributions: &[EvidenceContribution],
    total_score: f32,
    margin_to_runner_up: Option<f32>,
    runner_up_score: Option<f32>,
    agreement_score: Option<f32>,
    contradiction_strength: f32,
    contradiction_penalty: f32,
) -> (f32, String, ConfidenceFactors) {
    let input = CalibrationInput::new(total_score, contributions)
        .with_competition(margin_to_runner_up, runner_up_score, None)
        .with_agreement(agreement_score)
        .with_contradiction(contradiction_strength, contradiction_penalty);

    let result = ConfidenceCalibrator::new().calibrate(&input);
    (
        result.calibrated_confidence,
        result.band.display_name().to_string(),
        result.factors,
    )
}

/// Helper to load workspace scoring contexts from the database.
pub fn load_workspace_scoring_contexts(conn: &Connection) -> Result<Vec<WorkspaceScoringContext>, String> {
    let workspaces = crate::db::operations::list_workspaces(conn).map_err(|e| e.to_string())?;
    let mut contexts = Vec::new();

    for ws in workspaces {
        let mut ctx = WorkspaceScoringContext::new(&ws.id, &ws.name)
            .with_lifecycle_state(&ws.lifecycle_state);

        let ws_items = crate::db::operations::list_items(conn, Some(&ws.id)).unwrap_or_default();
        let ws_item_ids: Vec<String> = ws_items.iter().map(|i| i.id.clone()).collect();
        ctx.item_types = ws_items.iter().map(|i| i.item_type.clone()).collect();
        ctx.item_ids = ws_item_ids.clone();

        if !ws_item_ids.is_empty() {
            let placeholders: String = vec!["?"; ws_item_ids.len()].join(",");
            let sql = format!("SELECT signal_type, session_id, payload FROM context_signals WHERE item_id IN ({})", placeholders);

            if let Ok(mut stmt) = conn.prepare(&sql) {
                let params = rusqlite::params_from_iter(ws_item_ids.iter());
                if let Ok(iter) = stmt.query_map(params, |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                }) {
                    for row_res in iter.flatten() {
                        let (sig_type, session_id, payload_str) = row_res;
                        if let Some(s_id) = session_id {
                            ctx.sessions.push(s_id);
                        }
                        if sig_type == "active_app" {
                            if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&payload_str) {
                                if let Some(app_name) = payload.get("app_name").and_then(|v| v.as_str()) {
                                    ctx.app_names.push(app_name.to_string());
                                }
                            }
                        } else if sig_type == "url_metadata" {
                            if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&payload_str) {
                                if let Some(domain) = payload.get("domain").and_then(|v| v.as_str()) {
                                    ctx.domains.push(domain.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Ok(Some(ws_emb)) = crate::db::operations::get_workspace_embedding(conn, &ws.id) {
            ctx.profile_embedding = Some(ws_emb.embedding);
            ctx.profile_provider = Some(ws_emb.provider);
            ctx.profile_model = Some(ws_emb.model);
        }

        contexts.push(ctx);
    }

    Ok(contexts)
}

/// Scores an Item across all existing workspaces using the unified `ContextEvidence` pipeline.
pub fn score_item(conn: &Connection, item_id: &str) -> Result<Vec<WorkspaceScore>, String> {
    let mut weights = ScoringWeights::default();
    weights.temporal_decay = crate::context_engine::temporal::get_temporal_decay_config(conn);

    let enable_semantic = if let Ok(settings) = crate::db::settings::get_settings(conn) {
        settings.enable_semantic_suggestions
    } else {
        true
    };

    // 1. Gather ContextEvidence via the ContextEvidenceCollector service
    let evidence_set = ContextEvidenceCollector::new().collect(conn, item_id)?;

    // 2. Load Workspace Contexts
    let ws_contexts = load_workspace_scoring_contexts(conn)?;
    if ws_contexts.is_empty() {
        return Ok(Vec::new());
    }

    // 3. Execute Unified Context Scoring Pipeline
    let scorer = ContextEvidenceScorer::with_weights(weights)
        .with_semantic_enabled(enable_semantic);
    let results = scorer.score_evidence(&evidence_set, &ws_contexts);

    // 4. Convert and filter by low threshold
    let mut scores = Vec::new();
    for res in results {
        if res.total_score >= scorer.weights.threshold_low {
            scores.push(res.to_workspace_score());
        }
    }

    Ok(scores)
}

/// Evaluates a newly captured or unassigned Item against existing Workspaces and potential new ContextCandidates.
///
/// Returns structured scoring information and unified contextual interpretations with explainable reasons.
/// Does not move or create anything automatically.
pub fn evaluate_item_context(
    conn: &Connection,
    item_id: &str,
) -> Result<crate::db::models::UnifiedContextInterpretation, String> {
    let mut weights = ScoringWeights::default();
    weights.temporal_decay = crate::context_engine::temporal::get_temporal_decay_config(conn);

    let enable_semantic = if let Ok(settings) = crate::db::settings::get_settings(conn) {
        settings.enable_semantic_suggestions
    } else {
        true
    };

    // 1. Collect ContextEvidence
    let evidence_set = ContextEvidenceCollector::new().collect(conn, item_id)?;

    // 2. Load Workspace Scoring Contexts
    let ws_contexts = load_workspace_scoring_contexts(conn)?;

    // 3. Score against all existing workspaces using pure pipeline
    let scorer = ContextEvidenceScorer::with_weights(weights)
        .with_semantic_enabled(enable_semantic);
    let scored_results = scorer.score_evidence(&evidence_set, &ws_contexts);

    let mut existing_workspaces = Vec::new();
    for res in &scored_results {
        let clamped_score = (res.total_score as f64).clamp(0.0, 1.0);
        let clamped_confidence = (res.confidence as f64).clamp(0.0, 1.0);

        let explanation = crate::context_engine::explanation::explain_from_score_result(
            &evidence_set,
            res,
            "suggested",
            item_id,
        );
        let final_reasons = if !explanation.why_reasons.is_empty() {
            explanation.why_reasons
        } else {
            vec![format!("Baseline comparison with minimal contextual overlap with {}.", res.context_name)]
        };

        existing_workspaces.push(crate::db::models::ExistingWorkspaceEvaluation {
            workspace_id: res.context_id.clone(),
            workspace_name: res.context_name.clone(),
            score: clamped_score,
            confidence: clamped_confidence,
            confidence_percentage: format!("{:.0}%", (clamped_confidence * 100.0).round()),
            confidence_band: res.confidence_band.clone(),
            reasons: final_reasons,
        });
    }

    existing_workspaces.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));

    // --- Evaluate Against Potential New ContextCandidates ---
    let mut potential_new_contexts = Vec::new();

    // Check if the item already belongs to an active detected/suggested candidate
    let mut candidate_stmt = conn.prepare(
        "SELECT c.id, c.label, c.confidence,
                (SELECT COUNT(*) FROM context_candidate_items WHERE candidate_id = c.id) as item_count
         FROM context_candidates c
         JOIN context_candidate_items ci ON c.id = ci.candidate_id
         WHERE ci.item_id = ?1 AND c.status IN ('detected', 'suggested')
         ORDER BY c.confidence DESC",
    ).map_err(|e| e.to_string())?;

    let candidate_rows = candidate_stmt.query_map(rusqlite::params![item_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, usize>(3)?,
        ))
    }).map_err(|e| e.to_string())?;

    let mut found_candidate = false;
    for c_res in candidate_rows.flatten() {
        found_candidate = true;
        let (cand_id, label, conf, count) = c_res;
        let band = ConfidenceBand::from_f64(conf).display_name();
        let display_label = label.unwrap_or_else(|| "possible new context".to_string());
        potential_new_contexts.push(crate::db::models::PotentialNewContextEvaluation {
            candidate_id: Some(cand_id),
            label: display_label,
            confidence: conf,
            confidence_percentage: format!("{:.0}%", (conf * 100.0).round()),
            confidence_band: band.to_string(),
            item_count: count,
            reasons: vec![
                format!("Detected Context Candidate with {} related items", count),
                "Cohesive session and relationship cluster".to_string(),
            ],
        });
    }

    if !found_candidate {
        let target_item = crate::db::operations::get_item(conn, item_id).map_err(|e| e.to_string())?;
        let relationships = crate::db::operations::get_item_relationships(conn, item_id).unwrap_or_default();
        let all_unassigned = crate::db::operations::list_items(conn, None).unwrap_or_default();
        let mut peer_ids = Vec::new();

        for other in &all_unassigned {
            if other.id != item_id {
                let same_session = target_item.session_id.is_some() && other.session_id == target_item.session_id;
                let has_rel = relationships.iter().any(|r| (r.item_a_id == other.id || r.item_b_id == other.id) && r.strength >= 0.4);
                if same_session || has_rel {
                    peer_ids.push(other.id.clone());
                }
            }
        }

        if !peer_ids.is_empty() {
            let mut group = vec![item_id.to_string()];
            group.extend(peer_ids);
            if let Ok(group_score) = crate::context_engine::discovery::score_candidate_group(conn, &group, None) {
                let conf = group_score.confidence;
                let band = ConfidenceBand::from_f64(conf).display_name();
                let group_label = crate::context_engine::discovery::generate_candidate_name(conn, &group)
                    .unwrap_or_else(|_| "possible new context".to_string());

                potential_new_contexts.push(crate::db::models::PotentialNewContextEvaluation {
                    candidate_id: None,
                    label: group_label,
                    confidence: conf,
                    confidence_percentage: format!("{:.0}%", (conf * 100.0).round()),
                    confidence_band: band.to_string(),
                    item_count: group.len(),
                    reasons: group_score.evidence,
                });
            }
        } else {
            let max_existing = existing_workspaces.first().map(|w| w.score).unwrap_or(0.0);
            let (conf, reasons) = if max_existing >= 0.60 {
                let c = ((1.0 - max_existing) * 0.35).clamp(0.01, 0.05);
                let top_name = &existing_workspaces[0].workspace_name;
                (c, vec![format!("Strongly matches existing workspace '{}' ({:.0}%); novel context unlikely", top_name, max_existing * 100.0)])
            } else if max_existing >= 0.30 {
                let c = ((1.0 - max_existing) * 0.30).clamp(0.05, 0.20);
                (c, vec!["Moderate overlap with existing workspaces; weak evidence for a standalone new context".to_string()])
            } else {
                let c = (0.50 * (1.0 - max_existing)).clamp(0.20, 0.45);
                (c, vec!["Does not strongly match existing workspaces; potential starting point for a new context pending further activity".to_string()])
            };

            let band = ConfidenceBand::from_f64(conf).display_name();
            potential_new_contexts.push(crate::db::models::PotentialNewContextEvaluation {
                candidate_id: None,
                label: "possible new context".to_string(),
                confidence: conf,
                confidence_percentage: format!("{:.0}%", (conf * 100.0).round()),
                confidence_band: band.to_string(),
                item_count: 1,
                reasons,
            });
        }
    }

    potential_new_contexts.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(Ordering::Equal));

    // --- Unified Interpretation & Summary ---
    let top_ws = existing_workspaces.first();
    let top_new = potential_new_contexts.first();

    let target_rels = crate::db::operations::get_item_relationships(conn, item_id).unwrap_or_default();
    let meaningful_rels_count = target_rels.iter().filter(|r| r.strength >= 0.30).count();

    let (interpretation_kind, no_context_reason, primary_interpretation, summary) = if let Some(ws) = top_ws {
        let is_ambiguous = existing_workspaces.len() > 1
            && (existing_workspaces[0].score - existing_workspaces[1].score).abs() <= AMBIGUITY_MARGIN as f64
            && existing_workspaces[1].score >= AMBIGUITY_RUNNER_UP_FLOOR as f64;

        if is_ambiguous {
            let ws2 = &existing_workspaces[1];
            (
                crate::db::models::ContextInterpretationKind::Ambiguous,
                None,
                "ambiguous".to_string(),
                format!(
                    "Ambiguous match: competing affinity between '{}' ({}) and '{}' ({}); item remains in Inbox without forced assignment",
                    ws.workspace_name, ws.confidence_percentage, ws2.workspace_name, ws2.confidence_percentage
                ),
            )
        } else if ws.confidence >= ConfidenceBand::MEDIUM_THRESHOLD as f64
            && ws.score >= 0.35
            && (top_new.is_none() || ws.confidence >= top_new.unwrap().confidence)
        {
            (
                crate::db::models::ContextInterpretationKind::ExistingContext,
                None,
                "existing_context".to_string(),
                format!("Strong match to existing workspace '{}' ({})", ws.workspace_name, ws.confidence_percentage),
            )
        } else if let Some(new_ctx) = top_new.filter(|n| n.confidence >= ConfidenceBand::HIGH_THRESHOLD as f64) {
            (
                crate::db::models::ContextInterpretationKind::NewContextCandidate,
                None,
                "new_context_candidate".to_string(),
                format!("Cohesive new context detected: '{}' ({})", new_ctx.label, new_ctx.confidence_percentage),
            )
        } else {
            let (reason, explanation) = diagnose_no_context_reason(&evidence_set, &scored_results, meaningful_rels_count);
            (
                crate::db::models::ContextInterpretationKind::NoContext,
                Some(reason),
                "no_context".to_string(),
                format!("No strong context yet — {}", explanation),
            )
        }
    } else {
        let (reason, explanation) = diagnose_no_context_reason(&evidence_set, &scored_results, meaningful_rels_count);
        (
            crate::db::models::ContextInterpretationKind::NoContext,
            Some(reason),
            "no_context".to_string(),
            format!("No strong context yet — {}", explanation),
        )
    };

    Ok(crate::db::models::UnifiedContextInterpretation {
        item_id: item_id.to_string(),
        existing_workspaces,
        potential_new_contexts,
        primary_interpretation,
        interpretation_kind: Some(interpretation_kind),
        summary,
        no_context_reason,
    })
}

/// Diagnoses why an Item has no confident context, returning an explicit NoContextReason and detailed explanation.
pub fn diagnose_no_context_reason(
    evidence_set: &ContextEvidenceSet,
    scored_results: &[ContextScoreResult],
    relationships_count: usize,
) -> (crate::db::models::NoContextReason, String) {
    use crate::db::models::NoContextReason;
    use crate::context_engine::evidence::EvidenceCategory;

    // 1. Zero evidence
    if evidence_set.items.is_empty() {
        return (
            NoContextReason::ZeroEvidence,
            NoContextReason::ZeroEvidence.explanation().to_string(),
        );
    }

    // 2. Conflicting contexts (prior user dismissals or strong contradiction >= 0.50)
    let has_user_dismissal = evidence_set
        .items
        .iter()
        .any(|e| e.category == EvidenceCategory::UserFeedback);
    let has_strong_contradiction = scored_results.iter().any(|r| {
        r.contradiction_report
            .as_ref()
            .map_or(0.0, |c| c.contradiction_strength)
            >= 0.50
            || (r.total_score == 0.0 && !r.contradictory_evidence.is_empty())
    });

    if has_user_dismissal || has_strong_contradiction {
        return (
            NoContextReason::ConflictingContexts,
            NoContextReason::ConflictingContexts.explanation().to_string(),
        );
    }

    // 3. Insufficient evidence diversity (single group or low diversity score despite multiple items)
    let diversity = EvidenceDiversityCalculator::calculate_from_evidence_set(evidence_set);
    if diversity.evidence_quantity >= 2 && diversity.independent_groups_count <= 1 {
        return (
            NoContextReason::InsufficientEvidenceDiversity,
            NoContextReason::InsufficientEvidenceDiversity.explanation().to_string(),
        );
    }

    // 4. Isolated weak peripheral evidence (only app/domain/content_type without session/semantic/graph/history)
    let has_substantive = evidence_set.items.iter().any(|e| {
        matches!(
            e.category,
            EvidenceCategory::ConfirmedMembership
                | EvidenceCategory::SameSession
                | EvidenceCategory::WorkspaceHistory
                | EvidenceCategory::RelatedItem
                | EvidenceCategory::SemanticSimilarity
        )
    });

    if !evidence_set.items.is_empty() && !has_substantive {
        return (
            NoContextReason::WeakEvidence,
            NoContextReason::WeakEvidence.explanation().to_string(),
        );
    }

    // 5. Low semantic relevance (semantic similarity was evaluated but below meaningful threshold)
    let semantic_evidences: Vec<_> = evidence_set
        .items
        .iter()
        .filter(|e| e.category == EvidenceCategory::SemanticSimilarity)
        .collect();
    let has_semantic = !semantic_evidences.is_empty();
    let all_semantic_low = has_semantic && semantic_evidences.iter().all(|e| e.strength < 0.35);
    if all_semantic_low {
        return (
            NoContextReason::LowSemanticRelevance,
            NoContextReason::LowSemanticRelevance.explanation().to_string(),
        );
    }

    // 6. No meaningful relationships (no graph edges with existing workspaces)
    let has_graph_evidence = evidence_set
        .items
        .iter()
        .any(|e| e.category == EvidenceCategory::RelatedItem);
    if relationships_count == 0 && !has_graph_evidence && scored_results.iter().all(|r| r.total_score < 0.25) {
        return (
            NoContextReason::NoMeaningfulRelationships,
            NoContextReason::NoMeaningfulRelationships.explanation().to_string(),
        );
    }

    // 7. Insufficient evidence (general fallback)
    (
        NoContextReason::InsufficientEvidence,
        NoContextReason::InsufficientEvidence.explanation().to_string(),
    )
}

/// Recalculates confidence for suggested ContextMemberships associated with a workspace.
pub fn recalculate_workspace_suggested_memberships(
    conn: &Connection,
    workspace_id: &str,
    trigger_item_id: Option<&str>,
) -> Result<Vec<crate::db::models::ContextMembership>, String> {
    let ws = crate::db::operations::get_workspace(conn, workspace_id)
        .map_err(|e| format!("Workspace '{}' not found: {}", workspace_id, e))?;

    if ws.lifecycle_state == "archived" {
        return Ok(Vec::new());
    }

    let mut target_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    if let Ok(mut stmt) = conn.prepare(
        "SELECT item_id FROM context_memberships WHERE workspace_id = ?1 AND membership_type = 'suggested'"
    ) {
        if let Ok(rows) = stmt.query_map(rusqlite::params![workspace_id], |row| row.get::<_, String>(0)) {
            for r in rows.flatten() {
                target_ids.insert(r);
            }
        }
    }

    if let Some(t_id) = trigger_item_id {
        if let Ok(mut rel_stmt) = conn.prepare(
            "SELECT DISTINCT CASE WHEN r.item_a_id = ?1 THEN r.item_b_id ELSE r.item_a_id END as other_id
             FROM item_relationships r
             JOIN items other ON other.id = (CASE WHEN r.item_a_id = ?1 THEN r.item_b_id ELSE r.item_a_id END)
             WHERE (r.item_a_id = ?1 OR r.item_b_id = ?1)
               AND other.workspace_id IS NULL
               AND r.strength >= 0.40"
        ) {
            if let Ok(rows) = rel_stmt.query_map(rusqlite::params![t_id], |row| row.get::<_, String>(0)) {
                for r in rows.flatten() {
                    target_ids.insert(r);
                }
            }
        }
    }

    if target_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut updated_memberships = Vec::new();

    for target_item_id in target_ids {
        if Some(target_item_id.as_str()) == trigger_item_id {
            continue;
        }

        let existing_membership = crate::db::operations::get_item_workspace_membership(conn, &target_item_id, workspace_id)
            .ok()
            .flatten();

        if let Some(ref m) = existing_membership {
            if m.membership_type == "confirmed" || m.membership_type == "dismissed" {
                continue;
            }
        }

        if let Ok(item) = crate::db::operations::get_item(conn, &target_item_id) {
            if item.workspace_id.is_some() && item.workspace_id.as_deref() != Some(workspace_id) {
                continue;
            }
        }

        let scores = score_item(conn, &target_item_id).unwrap_or_default();
        let is_ambiguous = scores.len() > 1
            && (scores[0].total_score - scores[1].total_score).abs() <= AMBIGUITY_MARGIN
            && scores[1].total_score >= AMBIGUITY_RUNNER_UP_FLOOR;

        let new_score = scores.into_iter().find(|s| s.workspace_id == workspace_id);
        let new_confidence = new_score.map(|s| s.confidence as f64).unwrap_or(0.0).clamp(0.0, 1.0);

        let old_confidence = existing_membership.as_ref().map(|m| m.confidence).unwrap_or(0.0);
        let delta = (new_confidence - old_confidence).abs();

        let should_qualify = if existing_membership.is_some() {
            delta >= 0.01
        } else {
            new_confidence >= (CONFIDENCE_BAND_MEDIUM as f64 - 0.05) && !is_ambiguous
        };

        if should_qualify {
            let membership = crate::db::operations::create_or_update_membership(
                conn,
                &target_item_id,
                workspace_id,
                "suggested",
                new_confidence,
                "context_engine",
            ).map_err(|e| e.to_string())?;

            let payload = serde_json::json!({
                "action": "membership_confidence_recalculated",
                "workspace_id": workspace_id,
                "workspace_name": ws.name,
                "item_id": target_item_id,
                "previous_confidence": old_confidence,
                "new_confidence": new_confidence,
                "delta": new_confidence - old_confidence,
                "trigger_item_id": trigger_item_id,
                "reason": format!(
                    "Confidence dynamically updated from {:.0}% to {:.0}% as new items were added to {}",
                    old_confidence * 100.0,
                    new_confidence * 100.0,
                    ws.name
                )
            }).to_string();

            let _ = crate::context_engine::operations::create_context_signal(
                conn,
                Some(workspace_id),
                Some(&target_item_id),
                "membership_confidence_recalculated",
                None,
                &payload,
            );

            updated_memberships.push(membership);
        }
    }

    Ok(updated_memberships)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use crate::db::operations::{create_workspace, create_item};
    use crate::context_engine::operations::create_context_signal;
    use crate::context_engine::evidence::ContextEvidence;
    use crate::context_engine::contradiction::ContradictionKind;

    fn setup_in_memory_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_scoring_history_and_content_type() {
        let conn = setup_in_memory_db();
        
        let ws1 = create_workspace(&conn, "Work").unwrap();
        let ws2 = create_workspace(&conn, "Personal").unwrap();
        
        // Item in WS1 (Text)
        let _i1 = create_item(&conn, Some(&ws1.id), None, "text", "test content 1", None, "{}").unwrap();
        let _i2 = create_item(&conn, Some(&ws1.id), None, "text", "test content 2", None, "{}").unwrap();
        
        // Item in Inbox (Text)
        let target = create_item(&conn, None, None, "text", "target text", None, "{}").unwrap();
        
        // Mock a move signal indicating target WAS previously in WS2
        let payload = serde_json::json!({
            "action": "moved_to_workspace",
            "destination_workspace": ws2.id
        }).to_string();
        
        let _ = create_context_signal(
            &conn,
            None,
            Some(&target.id),
            "user_action",
            None,
            &payload
        ).unwrap();
        
        let scores = score_item(&conn, &target.id).unwrap();
        assert_eq!(scores.len(), 1);
        
        let w2_score = scores.iter().find(|s| s.workspace_id == ws2.id).unwrap();
        
        assert!(w2_score.total_score >= 0.4);
    }
    
    #[test]
    fn test_scoring_app_and_domain() {
        let conn = setup_in_memory_db();
        
        let ws = create_workspace(&conn, "Research").unwrap();
        
        // Item in WS
        let i1 = create_item(&conn, Some(&ws.id), None, "url", "https://example.com", None, "{}").unwrap();
        
        // Signal for i1
        let payload1 = serde_json::json!({
            "domain": "example.com"
        }).to_string();
        let _ = create_context_signal(&conn, Some(&ws.id), Some(&i1.id), "url_metadata", None, &payload1).unwrap();
        
        // Target Item
        let target = create_item(&conn, None, None, "url", "https://example.com/2", None, "{}").unwrap();
        
        let payload2 = serde_json::json!({
            "domain": "example.com"
        }).to_string();
        let _ = create_context_signal(&conn, None, Some(&target.id), "url_metadata", None, &payload2).unwrap();
        
        let payload3 = serde_json::json!({
            "action": "moved_to_workspace",
            "destination_workspace": ws.id
        }).to_string();
        let _ = create_context_signal(&conn, None, Some(&target.id), "user_action", None, &payload3).unwrap();
        
        let scores = score_item(&conn, &target.id).unwrap();
        assert_eq!(scores.len(), 1);
        
        let score = &scores[0];
        assert_eq!(score.workspace_id, ws.id);
        
        // history = 0.4
        // content_type_matches = 1 (0.02)
        // domain_match = 1 (0.1)
        // Total = 0.52
        assert!((score.total_score - 0.52).abs() < f32::EPSILON);
        assert!(score.reasons.iter().any(|r| r.contains("Same Example domain")));
    }
    
    #[test]
    fn test_scoring_with_relationships() {
        let conn = setup_in_memory_db();
        
        let ws = create_workspace(&conn, "Portfolio").unwrap();
        
        let target = create_item(&conn, None, None, "text", "target", None, "{}").unwrap();
        let related_item_1 = create_item(&conn, Some(&ws.id), None, "text", "r1", None, "{}").unwrap();
        let related_item_2 = create_item(&conn, Some(&ws.id), None, "text", "r2", None, "{}").unwrap();
        
        // Target -> related 1 (strength 0.6)
        let _ = crate::db::operations::create_item_relationship(
            &conn,
            &target.id,
            &related_item_1.id,
            "temporal_proximity",
            0.6,
            None
        ).unwrap();
        
        // Target -> related 2 (strength 0.4)
        let _ = crate::db::operations::create_item_relationship(
            &conn,
            &target.id,
            &related_item_2.id,
            "same_session",
            0.4,
            None
        ).unwrap();
        
        // Target -> related 3 (strength 0.8) to push score above threshold
        let related_item_3 = create_item(&conn, Some(&ws.id), None, "text", "r3", None, "{}").unwrap();
        let _ = crate::db::operations::create_item_relationship(
            &conn,
            &target.id,
            &related_item_3.id,
            "same_app",
            0.8,
            None
        ).unwrap();
        
        let scores = score_item(&conn, &target.id).unwrap();
        assert_eq!(scores.len(), 1);
        
        let score = &scores[0];
        assert_eq!(score.workspace_id, ws.id);
        
        // Content type match for 3 items: 3 * 0.02 = 0.06
        // Relationships: (0.6 + 0.4 + 0.8) = 1.8 * related_item_per_match (0.2) = 0.36
        // Total = 0.42
        assert!((score.total_score - 0.42).abs() < f32::EPSILON);
        assert!(score.reasons.iter().any(|r| r == "3 related Items already belong to Portfolio"));
    }
    
    #[test]
    fn test_scoring_with_semantic_similarity() {
        let conn = setup_in_memory_db();
        
        let ws_semantic = create_workspace(&conn, "AI Workspace").unwrap();
        let ws_history = create_workspace(&conn, "Manual Workspace").unwrap();
        
        let target = create_item(&conn, None, None, "text", "target", None, "{}").unwrap();
        let related_sem = create_item(&conn, Some(&ws_semantic.id), None, "text", "r_sem", None, "{}").unwrap();
        
        // Semantic relationship with high score
        let _ = crate::db::operations::create_item_relationship(
            &conn,
            &target.id,
            &related_sem.id,
            "semantic_similarity",
            0.95,
            None
        ).unwrap();
        
        // User explicitly moved an item to ws_history
        let payload = serde_json::json!({
            "action": "moved_to_workspace",
            "destination_workspace": ws_history.id
        }).to_string();
        
        let _ = create_context_signal(
            &conn,
            None,
            Some(&target.id),
            "user_action",
            None,
            &payload
        ).unwrap();
        
        let scores = score_item(&conn, &target.id).unwrap();
        assert_eq!(scores.len(), 2);
        
        let score_history = scores.iter().find(|s| s.workspace_id == ws_history.id).unwrap();
        let score_semantic = scores.iter().find(|s| s.workspace_id == ws_semantic.id).unwrap();
        
        assert!(score_semantic.reasons.iter().any(|r| r == "Semantic similarity with AI Workspace Items: 0.95"));
        assert!(score_history.reasons.iter().any(|r| r == "User previously organized similar Items into Manual Workspace"));
    }
    
    #[test]
    fn test_scoring_with_workspace_profile() {
        let conn = setup_in_memory_db();
        
        let ws = create_workspace(&conn, "Profile WS").unwrap();
        
        let target = create_item(&conn, None, None, "text", "target", None, "{}").unwrap();
        crate::db::operations::save_item_embedding(&conn, &target.id, &[1.0, 0.0, 0.0], "test", "test").unwrap();
        
        // Mock a workspace profile that is identical to target
        let bytes: Vec<u8> = vec![1.0f32, 0.0, 0.0].iter().flat_map(|&f| f.to_ne_bytes()).collect();
        conn.execute(
            "INSERT INTO workspace_embeddings (workspace_id, embedding, provider, model, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![ws.id, bytes, "test", "test", "time"],
        ).unwrap();
        
        let scores = score_item(&conn, &target.id).unwrap();
        assert_eq!(scores.len(), 1);
        
        let score = &scores[0];
        assert!((score.total_score - 0.45).abs() < f32::EPSILON);
        assert!(score.reasons.iter().any(|r| r == "Matches Profile WS semantic profile: 1.00"));
    }

    #[test]
    fn test_scoring_confidence_bands() {
        let conn = setup_in_memory_db();
        
        let ws_high = create_workspace(&conn, "High WS").unwrap();
        let ws_med = create_workspace(&conn, "Medium WS").unwrap();
        let ws_low = create_workspace(&conn, "Low WS").unwrap();
        let ws_filtered = create_workspace(&conn, "Filtered WS").unwrap();
        
        let target = create_item(&conn, None, None, "text", "target", None, "{}").unwrap();
        
        // High WS: 3 independent signals: history (0.40) + semantic (0.40) + related_items (0.20) -> High band
        let r_high = create_item(&conn, Some(&ws_high.id), None, "text", "rh", None, "{}").unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &r_high.id, "related_item", 1.0, None).unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &r_high.id, "semantic_similarity", 0.95, None).unwrap();
        let payload_high = serde_json::json!({"action": "moved_to_workspace", "destination_workspace": ws_high.id}).to_string();
        let _ = create_context_signal(&conn, None, Some(&target.id), "user_action", None, &payload_high).unwrap();
        
        // Medium WS: 2 signals: history (0.40) + related_item (0.20) -> Medium band
        let r_med = create_item(&conn, Some(&ws_med.id), None, "text", "rm", None, "{}").unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &r_med.id, "related_item", 1.0, None).unwrap();
        let payload_med = serde_json::json!({"action": "moved_to_workspace", "destination_workspace": ws_med.id}).to_string();
        let _ = create_context_signal(&conn, None, Some(&target.id), "user_action", None, &payload_med).unwrap();
        
        // Low WS: 1 relationship signal (0.20) -> score 0.20 >= threshold_low -> Low confidence band
        let r_low = create_item(&conn, Some(&ws_low.id), None, "text", "rl", None, "{}").unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &r_low.id, "related_item", 1.0, None).unwrap();
        
        // Filtered WS: score < 0.2
        let _ = create_item(&conn, Some(&ws_filtered.id), None, "text", "rf", None, "{}").unwrap();
        
        let scores = score_item(&conn, &target.id).unwrap();
        
        assert_eq!(scores.len(), 3);
        
        let high_score = scores.iter().find(|s| s.workspace_id == ws_high.id).unwrap();
        assert!(high_score.confidence_band == "High" || high_score.confidence_band == "Very High");
        
        let med_score = scores.iter().find(|s| s.workspace_id == ws_med.id).unwrap();
        assert_eq!(med_score.confidence_band, "Medium");
        
        let low_score = scores.iter().find(|s| s.workspace_id == ws_low.id).unwrap();
        assert!(low_score.confidence_band == "Low" || low_score.confidence_band == "Very Low");
        
        let filtered_score = scores.iter().find(|s| s.workspace_id == ws_filtered.id);
        assert!(filtered_score.is_none());
    }

    #[test]
    fn test_evaluate_item_context_unified_interpretation() {
        let conn = setup_in_memory_db();

        let ws_paris = create_workspace(&conn, "Paris Trip").unwrap();
        let ws_portfolio = create_workspace(&conn, "Portfolio").unwrap();
        let ws_college = create_workspace(&conn, "College").unwrap();

        let _port_item = create_item(&conn, Some(&ws_portfolio.id), None, "code", "const x = 42;", Some("Project code"), "{\"app_name\":\"VSCode\"}").unwrap();
        let _coll_item = create_item(&conn, Some(&ws_college.id), None, "pdf", "Calculus notes", Some("Math Lecture"), "{\"app_name\":\"Preview\"}").unwrap();

        let paris_item1 = create_item(&conn, Some(&ws_paris.id), None, "url", "https://airbnb.com/paris", Some("Paris Hotel"), "{\"app_name\":\"Safari\"}").unwrap();
        let paris_item2 = create_item(&conn, Some(&ws_paris.id), None, "url", "https://booking.com/paris-flights", Some("Paris Flight"), "{\"app_name\":\"Safari\"}").unwrap();

        let _ = create_context_signal(&conn, Some(&ws_paris.id), Some(&paris_item1.id), "url_metadata", None, "{\"domain\":\"airbnb.com\"}").unwrap();
        let _ = create_context_signal(&conn, Some(&ws_paris.id), Some(&paris_item1.id), "active_app", None, "{\"app_name\":\"Safari\"}").unwrap();
        let _ = create_context_signal(&conn, Some(&ws_paris.id), Some(&paris_item2.id), "url_metadata", None, "{\"domain\":\"booking.com\"}").unwrap();
        let _ = create_context_signal(&conn, Some(&ws_paris.id), Some(&paris_item2.id), "active_app", None, "{\"app_name\":\"Safari\"}").unwrap();

        let target = create_item(&conn, None, None, "url", "https://tripadvisor.com/paris-bistro", Some("Paris restaurant"), "{\"app_name\":\"Safari\"}").unwrap();
        let _ = create_context_signal(&conn, None, Some(&target.id), "active_app", None, "{\"app_name\":\"Safari\"}").unwrap();
        let _ = create_context_signal(&conn, None, Some(&target.id), "url_metadata", None, "{\"domain\":\"tripadvisor.com\"}").unwrap();

        let hist_payload = serde_json::json!({
            "action": "moved_to_workspace",
            "destination_workspace": ws_paris.id
        }).to_string();
        let _ = create_context_signal(&conn, None, Some(&target.id), "user_action", None, &hist_payload).unwrap();

        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &paris_item1.id, "same_app", 1.0, None).unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &paris_item2.id, "same_app", 1.0, None).unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &paris_item1.id, "temporal_proximity", 0.9, None).unwrap();

        let interpretation = evaluate_item_context(&conn, &target.id).unwrap();

        assert_eq!(interpretation.existing_workspaces.len(), 3);
        assert_eq!(interpretation.existing_workspaces[0].workspace_name, "Paris Trip");
        assert!(interpretation.existing_workspaces[0].score >= 0.70);
        assert!(interpretation.existing_workspaces[0].confidence_band == "Very High" || interpretation.existing_workspaces[0].confidence_band == "High");

        let portfolio_eval = interpretation.existing_workspaces.iter().find(|w| w.workspace_name == "Portfolio").unwrap();
        let college_eval = interpretation.existing_workspaces.iter().find(|w| w.workspace_name == "College").unwrap();
        assert!(portfolio_eval.score < 0.20);
        assert!(college_eval.score < 0.20);

        assert!(!interpretation.potential_new_contexts.is_empty());
        let new_ctx = &interpretation.potential_new_contexts[0];
        assert_eq!(new_ctx.label, "possible new context");
        assert!(new_ctx.confidence <= 0.05);

        assert!(!interpretation.existing_workspaces[0].reasons.is_empty());
        assert!(!new_ctx.reasons.is_empty());
        assert!(
            interpretation.primary_interpretation == "existing_context"
                || interpretation.primary_interpretation == "existing_workspace"
        );
        assert_eq!(
            interpretation.interpretation_kind,
            Some(crate::db::models::ContextInterpretationKind::ExistingContext)
        );
        assert!(interpretation.summary.contains("Paris Trip"));

        let target_after = crate::db::operations::get_item(&conn, &target.id).unwrap();
        assert_eq!(target_after.workspace_id, None);
        assert_eq!(crate::db::operations::list_items(&conn, Some(&ws_paris.id)).unwrap().len(), 2);
    }

    #[test]
    fn test_dynamic_context_membership_confidence_recalculation() {
        let conn = setup_in_memory_db();

        let ws_paris = create_workspace(&conn, "Paris Trip").unwrap();

        let initial_item = create_item(
            &conn,
            Some(&ws_paris.id),
            None,
            "url",
            "https://booking.com/paris-hotel",
            Some("Paris Hotel"),
            "{\"app_name\":\"Safari\",\"original_url\":\"https://booking.com/paris-hotel\"}",
        ).unwrap();

        let restaurant = create_item(
            &conn,
            None,
            None,
            "url",
            "https://tripadvisor.com/paris-bistro",
            Some("Paris restaurant"),
            "{\"app_name\":\"Safari\",\"original_url\":\"https://tripadvisor.com/paris-bistro\"}",
        ).unwrap();

        let _ = crate::db::operations::create_item_relationship(&conn, &restaurant.id, &initial_item.id, "same_app", 1.0, None).unwrap();

        let initial_membership = crate::db::operations::create_or_update_membership(
            &conn,
            &restaurant.id,
            &ws_paris.id,
            "suggested",
            0.20,
            "context_engine",
        ).unwrap();
        assert_eq!(initial_membership.confidence, 0.20);
        assert_eq!(initial_membership.membership_type, "suggested");

        let flight = create_item(
            &conn,
            Some(&ws_paris.id),
            None,
            "url",
            "https://airfrance.com/flight-paris",
            Some("Paris Flight"),
            "{\"app_name\":\"Safari\",\"original_url\":\"https://airfrance.com/flight-paris\"}",
        ).unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &restaurant.id, &flight.id, "same_app", 1.0, None).unwrap();

        let museum = create_item(
            &conn,
            Some(&ws_paris.id),
            None,
            "text",
            "Louvre tickets 10am, Orsay 3pm",
            Some("Paris Museum Guide"),
            "{\"app_name\":\"Safari\"}",
        ).unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &restaurant.id, &museum.id, "same_app", 1.0, None).unwrap();

        let hist_payload = serde_json::json!({
            "action": "moved_to_workspace",
            "destination_workspace": ws_paris.id
        }).to_string();
        let _ = create_context_signal(&conn, None, Some(&restaurant.id), "user_action", None, &hist_payload).unwrap();

        let updated = recalculate_workspace_suggested_memberships(&conn, &ws_paris.id, Some(&museum.id)).unwrap();
        assert!(!updated.is_empty());

        let stored_membership = crate::db::operations::get_item_workspace_membership(&conn, &restaurant.id, &ws_paris.id)
            .unwrap()
            .unwrap();

        assert!(
            stored_membership.confidence > initial_membership.confidence,
            "Confidence must increase dynamically with corroborating evidence (was {}, now {})",
            initial_membership.confidence,
            stored_membership.confidence
        );

        let rerun = recalculate_workspace_suggested_memberships(&conn, &ws_paris.id, None).unwrap();
        assert!(rerun.is_empty());

        assert_eq!(stored_membership.membership_type, "suggested");

        let target_item_after = crate::db::operations::get_item(&conn, &restaurant.id).unwrap();
        assert_eq!(target_item_after.workspace_id, None);

        let signals = crate::context_engine::operations::list_context_signals(&conn, Some(&ws_paris.id), Some(&restaurant.id)).unwrap();
        let recalc_signals: Vec<_> = signals
            .iter()
            .filter(|s| s.payload.contains("membership_confidence_recalculated"))
            .collect();
        assert!(!recalc_signals.is_empty());

        let latest_payload: serde_json::Value = serde_json::from_str(&recalc_signals.first().unwrap().payload).unwrap();
        assert!(latest_payload["new_confidence"].as_f64().unwrap() > 0.20);
        assert!(latest_payload["delta"].as_f64().unwrap() != 0.0);
        assert_eq!(latest_payload["workspace_id"].as_str().unwrap(), ws_paris.id);
    }

    #[test]
    fn test_temporal_context_weighting_recent_vs_old() {
        let conn = setup_in_memory_db();
        let now = chrono::Utc::now();
        let time_1h_ago = (now - chrono::Duration::hours(1)).to_rfc3339();
        let time_90d_ago = (now - chrono::Duration::days(90)).to_rfc3339();

        let ws_recent = create_workspace(&conn, "Recent Project").unwrap();
        let ws_old = create_workspace(&conn, "Archived Project").unwrap();

        let recent_item = create_item(
            &conn,
            Some(&ws_recent.id),
            None,
            "text",
            "Project architecture notes",
            Some("Recent Architecture"),
            "{}",
        ).unwrap();
        conn.execute("UPDATE items SET created_at = ?1 WHERE id = ?2", rusqlite::params![time_1h_ago, recent_item.id]).unwrap();

        let old_item = create_item(
            &conn,
            Some(&ws_old.id),
            None,
            "text",
            "Project architecture notes",
            Some("Old Architecture"),
            "{}",
        ).unwrap();
        conn.execute("UPDATE items SET created_at = ?1 WHERE id = ?2", rusqlite::params![time_90d_ago, old_item.id]).unwrap();

        let target = create_item(
            &conn,
            None,
            None,
            "text",
            "Target architecture query",
            Some("Target Query"),
            "{}",
        ).unwrap();

        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &recent_item.id, "same_topic", 0.90, None).unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &old_item.id, "same_topic", 0.90, None).unwrap();

        let interpretation = evaluate_item_context(&conn, &target.id).unwrap();
        let eval_recent = interpretation.existing_workspaces.iter().find(|w| w.workspace_id == ws_recent.id).unwrap();
        let eval_old = interpretation.existing_workspaces.iter().find(|w| w.workspace_id == ws_old.id).unwrap();

        assert!(eval_recent.score > eval_old.score);
        assert!(eval_old.score > 0.0);

        let old_item_check = crate::db::operations::get_item(&conn, &old_item.id);
        assert!(old_item_check.is_ok());

        let confirmed_old_membership = crate::db::operations::get_item_workspace_membership(&conn, &old_item.id, &ws_old.id).unwrap().unwrap();
        assert_eq!(confirmed_old_membership.membership_type, "confirmed");
        assert_eq!(confirmed_old_membership.confidence, 1.0);

        let mut no_decay_config = crate::context_engine::temporal::TemporalDecayConfig::default();
        no_decay_config.strategy = "none".to_string();
        crate::context_engine::temporal::update_temporal_decay_config(&conn, &no_decay_config).unwrap();

        let interp_no_decay = evaluate_item_context(&conn, &target.id).unwrap();
        let r_no_decay = interp_no_decay.existing_workspaces.iter().find(|w| w.workspace_id == ws_recent.id).unwrap();
        let o_no_decay = interp_no_decay.existing_workspaces.iter().find(|w| w.workspace_id == ws_old.id).unwrap();

        assert!((r_no_decay.score - o_no_decay.score).abs() < 0.001);
    }

    #[test]
    fn test_lifecycle_states_influence_contextual_suggestions() {
        let conn = setup_in_memory_db();

        let ws_active = create_workspace(&conn, "Active WS").unwrap();
        let ws_dormant = create_workspace(&conn, "Dormant WS").unwrap();
        let ws_archived = create_workspace(&conn, "Archived WS").unwrap();

        let item_act = create_item(&conn, Some(&ws_active.id), None, "text", "React Hooks", Some("React 1"), "{}").unwrap();
        let item_dor = create_item(&conn, Some(&ws_dormant.id), None, "text", "React Hooks", Some("React 2"), "{}").unwrap();
        let item_arc = create_item(&conn, Some(&ws_archived.id), None, "text", "React Hooks", Some("React 3"), "{}").unwrap();

        let _ = crate::db::operations::update_workspace_lifecycle_state(&conn, &ws_dormant.id, "dormant").unwrap();
        let _ = crate::db::operations::archive_workspace(&conn, &ws_archived.id).unwrap();

        let target = create_item(&conn, None, None, "text", "React Hooks Tutorial", Some("Target React"), "{}").unwrap();

        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &item_act.id, "same_topic", 0.90, None).unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &item_dor.id, "same_topic", 0.90, None).unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &item_arc.id, "same_topic", 0.90, None).unwrap();

        let interp = evaluate_item_context(&conn, &target.id).unwrap();

        let eval_active = interp.existing_workspaces.iter().find(|w| w.workspace_id == ws_active.id).unwrap();
        let eval_dormant = interp.existing_workspaces.iter().find(|w| w.workspace_id == ws_dormant.id).unwrap();
        let eval_archived = interp.existing_workspaces.iter().find(|w| w.workspace_id == ws_archived.id).unwrap();

        assert!(eval_active.score > 0.0);
        assert!(eval_active.score > eval_dormant.score);
        assert!(eval_dormant.score > 0.0);
        assert_eq!(eval_archived.score, 0.0);
        assert!(eval_archived.reasons.iter().any(|r| r.contains("archived by user")));

        let moved = crate::db::operations::move_item(&conn, &target.id, Some(&ws_archived.id)).unwrap();
        assert_eq!(moved.workspace_id.as_deref(), Some(ws_archived.id.as_str()));
        let mem = crate::db::operations::get_item_workspace_membership(&conn, &target.id, &ws_archived.id).unwrap().unwrap();
        assert_eq!(mem.membership_type, "confirmed");
        assert_eq!(mem.confidence, 1.0);
    }

    // =========================================================================
    // NEW EXTENSIVE UNIT TESTS FOR CENTRALIZED CONTEXT EVIDENCE SCORING PIPELINE
    // =========================================================================

    #[test]
    fn test_pure_pipeline_with_context_evidence_set() {
        // Pure pipeline execution directly on ContextEvidenceSet without DB queries
        let mut evidence_set = ContextEvidenceSet::new("target_item_xyz");
        evidence_set.add(ContextEvidence::inferred_workspace_history("target_item_xyz", "ws_1", "Project Alpha", 1.0, None));
        evidence_set.add(ContextEvidence::inferred_semantic_similarity("target_item_xyz", "item_other", 0.85, Some("Alpha Design"), Some("Project Alpha"), None));
        evidence_set.add(ContextEvidence::observed_app("target_item_xyz", "Figma", None));

        let mut ws = WorkspaceScoringContext::new("ws_1", "Project Alpha");
        ws.app_names.push("Figma".to_string());
        ws.item_ids.push("item_other".to_string());

        let scorer = ContextEvidenceScorer::default();
        let result = scorer.score_evidence_against_workspace(&evidence_set, &ws);

        assert_eq!(result.context_id, "ws_1");
        assert_eq!(result.context_name, "Project Alpha");
        // history (0.4) + semantic (0.4) + app (0.05) = 0.85 -> clamped to High band
        assert!(result.total_score >= 0.80, "Total score should reflect weighted evidence contributions (actual: {})", result.total_score);
        assert_eq!(result.confidence_band, "High");
        assert_eq!(result.evidence_contributions.len(), 3);
        assert!(result.contradictory_evidence.is_empty());
        assert!(!result.strongest_reasons.is_empty());
    }

    #[test]
    fn test_anti_dominance_single_source_app_cannot_dominate() {
        // Invariant: Source application alone must NOT dominate or reach medium/high confidence
        let mut evidence_set = ContextEvidenceSet::new("target_item_app_only");
        evidence_set.add(ContextEvidence::observed_app("target_item_app_only", "Google Chrome", None));

        let mut ws = WorkspaceScoringContext::new("ws_random", "Random Workspace");
        // Add 10 items with Google Chrome to test saturation cap
        for _ in 0..10 {
            ws.app_names.push("Google Chrome".to_string());
        }

        let scorer = ContextEvidenceScorer::default();
        let result = scorer.score_evidence_against_workspace(&evidence_set, &ws);

        // App alone is capped at same_source_app_max (0.20) and isolated weak signal cap (0.24)
        assert!(result.total_score <= 0.24, "Single source app must not exceed weak signal cap (actual: {})", result.total_score);
        assert!(result.confidence_band == "Low" || result.confidence_band == "Very Low", "Single source app must never trigger Medium or High confidence");
        assert!(result.uncertainty_indicators.iter().any(|u| u.contains("weak peripheral signals")));
    }

    #[test]
    fn test_anti_dominance_single_domain_cannot_dominate() {
        // Invariant: Web domain alone must NOT dominate or reach medium/high confidence
        let mut evidence_set = ContextEvidenceSet::new("target_item_dom_only");
        evidence_set.add(ContextEvidence::observed_domain("target_item_dom_only", "github.com", None, None));

        let mut ws = WorkspaceScoringContext::new("ws_code", "Code Repository");
        for _ in 0..10 {
            ws.domains.push("github.com".to_string());
        }

        let scorer = ContextEvidenceScorer::default();
        let result = scorer.score_evidence_against_workspace(&evidence_set, &ws);

        assert!(result.total_score <= 0.24, "Single web domain must not exceed weak signal cap (actual: {})", result.total_score);
        assert!(result.confidence_band == "Low" || result.confidence_band == "Very Low", "Single web domain must never trigger Medium or High confidence");
        assert!(result.uncertainty_indicators.iter().any(|u| u.contains("weak peripheral signals")));
    }

    #[test]
    fn test_anti_dominance_multiple_isolated_weak_signals_capped() {
        // Invariant: Combining multiple weak signals (app + domain + content type) without session/semantic/graph
        // must still be capped below threshold_medium (0.35)
        let mut evidence_set = ContextEvidenceSet::new("target_weak_combo");
        evidence_set.add(ContextEvidence::observed_app("target_weak_combo", "Safari", None));
        evidence_set.add(ContextEvidence::observed_domain("target_weak_combo", "example.com", None, None));
        evidence_set.add(ContextEvidence::observed_content_type("target_weak_combo", "url", None));

        let mut ws = WorkspaceScoringContext::new("ws_general", "General Workspace");
        ws.app_names.push("Safari".to_string());
        ws.domains.push("example.com".to_string());
        ws.item_types.push("url".to_string());

        let scorer = ContextEvidenceScorer::default();
        let result = scorer.score_evidence_against_workspace(&evidence_set, &ws);

        assert!(result.total_score <= 0.24, "Isolated weak signals combination must be strictly capped at 0.24 (actual: {})", result.total_score);
        assert!(result.confidence_band == "Low" || result.confidence_band == "Very Low");
        assert!(result.uncertainty_indicators.iter().any(|u| u.contains("weak peripheral signals")));
    }

    #[test]
    fn test_contradictory_user_feedback_reduces_suggestion() {
        // Invariant: Strong contradictory user evidence (dismissals/rejections) actively penalizes suggestions
        let mut evidence_set = ContextEvidenceSet::new("target_rejected");
        // Positive signals
        evidence_set.add(ContextEvidence::inferred_workspace_history("target_rejected", "ws_dismissed", "Dismissed Project", 1.0, None));
        // Negative user feedback
        evidence_set.add(ContextEvidence::observed_user_feedback(
            "target_rejected",
            "suggestion_dismissed",
            Some(crate::context_engine::evidence::EntityReference::workspace("ws_dismissed", Some("Dismissed Project".to_string()))),
            None,
        ));

        let ws = WorkspaceScoringContext::new("ws_dismissed", "Dismissed Project");

        let scorer = ContextEvidenceScorer::default();
        let result = scorer.score_evidence_against_workspace(&evidence_set, &ws);

        // History gave +0.40, dismissal gave -0.50 -> clamped to 0.0
        assert_eq!(result.total_score, 0.0, "Contradictory user dismissal must reduce suggestion score to 0.0");
        assert!(!result.contradictory_evidence.is_empty(), "Contradictory evidence must be populated");
        assert!(result.contradictory_evidence.iter().any(|c| c.category == "user_feedback"));
        assert!(result.uncertainty_indicators.iter().any(|u| u.contains("negative user feedback")));
    }

    #[test]
    fn test_structured_scoring_information_fields() {
        // Invariant: Structured output contains all required fields:
        // context ID, total score, confidence, evidence contributions, strongest reasons, contradictory evidence, uncertainty indicators
        let mut evidence_set = ContextEvidenceSet::new("target_structured");
        evidence_set.add(ContextEvidence::observed_confirmed_membership("target_structured", "ws_auth", Some("Auth WS"), None));

        let ws = WorkspaceScoringContext::new("ws_auth", "Auth WS");

        let scorer = ContextEvidenceScorer::default();
        let result = scorer.score_evidence_against_workspace(&evidence_set, &ws);

        assert_eq!(result.context_id, "ws_auth");
        assert_eq!(result.context_name, "Auth WS");
        assert_eq!(result.total_score, 1.0);
        assert_eq!(result.confidence, 1.0);
        assert_eq!(result.confidence_band, "Very High");
        assert!(!result.evidence_contributions.is_empty());
        assert_eq!(result.evidence_contributions[0].category, "confirmed_membership");
        assert_eq!(result.evidence_contributions[0].raw_strength, 1.0);
        assert_eq!(result.evidence_contributions[0].weighted_score, 1.0);
        assert!(result.evidence_contributions[0].is_positive);
        assert!(!result.strongest_reasons.is_empty());
    }

    #[test]
    fn test_deterministic_scoring_reproducibility() {
        // Invariant: Running scorer multiple times produces 100% reproducible identical scores and reasons
        let mut evidence_set = ContextEvidenceSet::new("target_det");
        evidence_set.add(ContextEvidence::inferred_workspace_history("target_det", "ws_1", "WS 1", 0.95, None));
        evidence_set.add(ContextEvidence::observed_app("target_det", "Safari", None));

        let mut ws = WorkspaceScoringContext::new("ws_1", "WS 1");
        ws.app_names.push("Safari".to_string());

        let scorer = ContextEvidenceScorer::default();
        let res1 = scorer.score_evidence_against_workspace(&evidence_set, &ws);
        let res2 = scorer.score_evidence_against_workspace(&evidence_set, &ws);

        assert_eq!(res1, res2, "Scoring results must be 100% deterministic and reproducible");
    }

    #[test]
    fn test_zero_mutation_invariant() {
        let conn = setup_in_memory_db();
        let ws = create_workspace(&conn, "Untouched Workspace").unwrap();
        let item = create_item(&conn, None, None, "text", "Unassigned Item", Some("Unassigned"), "{}").unwrap();

        // Run scoring and interpretation
        let _ = score_item(&conn, &item.id).unwrap();
        let _ = evaluate_item_context(&conn, &item.id).unwrap();

        // Invariant: Target item remains in Inbox (workspace_id == None), workspace has 0 items
        let item_after = crate::db::operations::get_item(&conn, &item.id).unwrap();
        assert_eq!(item_after.workspace_id, None, "Scoring must NEVER automatically assign items");

        let ws_items = crate::db::operations::list_items(&conn, Some(&ws.id)).unwrap();
        assert_eq!(ws_items.len(), 0, "Scoring must NEVER mutate workspace item memberships");
    }

    #[test]
    fn test_single_strong_semantic_signal_has_limited_confidence() {
        // Invariant: One strong semantic similarity signal allows high score/affinity,
        // but confidence MUST remain limited (e.g. <= 0.45) due to lack of evidence diversity.
        let mut evidence_set = ContextEvidenceSet::new("target_semantic_only");
        evidence_set.add(ContextEvidence::inferred_semantic_similarity(
            "target_semantic_only",
            "item_peer",
            0.95,
            Some("AI Research Paper"),
            Some("AI Lab"),
            None,
        ));

        let mut ws = WorkspaceScoringContext::new("ws_ai", "AI Lab");
        ws.item_ids.push("item_peer".to_string());

        let scorer = ContextEvidenceScorer::default();
        let result = scorer.score_evidence_against_workspace(&evidence_set, &ws);

        // Affinity score can be substantial (e.g. 0.40)
        assert!(result.total_score >= 0.40, "Total score should reflect semantic match (actual: {})", result.total_score);

        // But confidence must remain bounded and conservative
        assert!(
            result.confidence <= 0.45,
            "Single semantic signal must have limited confidence <= 0.45 (actual: {})",
            result.confidence
        );
        assert_ne!(result.confidence_band, "High", "Single semantic signal must never reach High confidence band");
        assert!(result.confidence_factors.is_some());
        let factors = result.confidence_factors.unwrap();
        assert_eq!(factors.diversity_count, 1);
        assert!(result.uncertainty_indicators.iter().any(|u| u.contains("relies on a single evidence category")));
    }

    #[test]
    fn test_multiple_independent_agreeing_signals_yield_high_confidence() {
        // Invariant: Multiple independent signals agreeing (e.g. history + session + graph + semantic)
        // yields high score AND high confidence (>= 0.70, "High" band).
        let mut evidence_set = ContextEvidenceSet::new("target_multi");
        evidence_set.add(ContextEvidence::inferred_workspace_history("target_multi", "ws_proj", "Project Falcon", 1.0, None));
        evidence_set.add(ContextEvidence::inferred_semantic_similarity("target_multi", "falcon_doc", 0.90, Some("Falcon Architecture"), Some("Project Falcon"), None));
        evidence_set.add(ContextEvidence::observed_session("target_multi", "sess_falcon", None));
        evidence_set.add(ContextEvidence::observed_app("target_multi", "VSCode", None));

        let mut ws = WorkspaceScoringContext::new("ws_proj", "Project Falcon");
        ws.sessions.push("sess_falcon".to_string());
        ws.item_ids.push("falcon_doc".to_string());
        ws.app_names.push("VSCode".to_string());

        let scorer = ContextEvidenceScorer::default();
        let result = scorer.score_evidence_against_workspace(&evidence_set, &ws);

        assert!(result.total_score >= 0.80, "Multi-signal affinity score should be >= 0.80 (actual: {})", result.total_score);
        assert!(
            result.confidence >= 0.70,
            "Multiple independent agreeing signals must yield high confidence >= 0.70 (actual: {})",
            result.confidence
        );
        assert!(result.confidence_band == "High" || result.confidence_band == "Very High");
        let factors = result.confidence_factors.unwrap();
        assert!(factors.diversity_count >= 3);
        assert_eq!(factors.diversity_multiplier, 1.0);
    }

    #[test]
    fn test_nearly_identical_scores_decrease_confidence_via_ambiguity_margin() {
        // Invariant: Two contexts with nearly identical scores experience ambiguity margin damping,
        // reducing confidence so the system does not prematurely force a decision.
        let mut evidence_set = ContextEvidenceSet::new("target_ambiguous");
        evidence_set.add(ContextEvidence::observed_app("target_ambiguous", "Figma", None));
        evidence_set.add(ContextEvidence::observed_domain("target_ambiguous", "figma.com", None, None));
        evidence_set.add(ContextEvidence::inferred_semantic_similarity("target_ambiguous", "item_a", 0.88, Some("UI Mockup"), Some("Workspace A"), None));
        evidence_set.add(ContextEvidence::inferred_semantic_similarity("target_ambiguous", "item_b", 0.86, Some("UI Spec"), Some("Workspace B"), None));

        let mut ws_a = WorkspaceScoringContext::new("ws_a", "Workspace A");
        ws_a.item_ids.push("item_a".to_string());
        ws_a.app_names.push("Figma".to_string());
        ws_a.domains.push("figma.com".to_string());

        let mut ws_b = WorkspaceScoringContext::new("ws_b", "Workspace B");
        ws_b.item_ids.push("item_b".to_string());
        ws_b.app_names.push("Figma".to_string());
        ws_b.domains.push("figma.com".to_string());

        let scorer = ContextEvidenceScorer::default();
        let results = scorer.score_evidence(&evidence_set, &[ws_a, ws_b]);

        assert_eq!(results.len(), 2);
        let top_1 = &results[0];
        let top_2 = &results[1];

        // Margin between scores is very small
        let margin = (top_1.total_score - top_2.total_score).abs();
        assert!(margin <= AMBIGUITY_MARGIN);

        // Confidence of top_1 must be significantly lower than its total_score due to ambiguity penalty
        assert!(
            top_1.confidence < top_1.total_score,
            "Ambiguity must damp confidence below raw score (score: {}, conf: {})",
            top_1.total_score,
            top_1.confidence
        );
        assert!(top_1.confidence <= 0.45, "Ambiguous candidates must not have high confidence (actual: {})", top_1.confidence);
        assert!(top_1.uncertainty_indicators.iter().any(|u| u.contains("Ambiguous context assignment")));
    }

    #[test]
    fn test_very_little_evidence_yields_low_confidence() {
        // Invariant: Very little evidence (e.g. single peripheral weak signal) yields low confidence.
        let mut evidence_set = ContextEvidenceSet::new("target_sparse");
        evidence_set.add(ContextEvidence::observed_content_type("target_sparse", "url", None));

        let mut ws = WorkspaceScoringContext::new("ws_sparse", "Sparse WS");
        ws.item_types.push("url".to_string());

        let scorer = ContextEvidenceScorer::default();
        let result = scorer.score_evidence_against_workspace(&evidence_set, &ws);

        assert!(result.total_score <= 0.10);
        assert!(
            result.confidence <= 0.15,
            "Sparse evidence must result in low confidence <= 0.15 (actual: {})",
            result.confidence
        );
        assert!(result.confidence_band == "Low" || result.confidence_band == "Very Low");
    }

    #[test]
    fn test_repeated_correlated_urls_same_domain_bounded_confidence() {
        // Invariant: Multiple repeated URLs from the same domain increase quantity/affinity,
        // but because they all belong to a single independent group (WebOrigin),
        // diversity score and multiplier remain strictly bounded.
        let mut evidence_set = ContextEvidenceSet::new("target_github_urls");
        for i in 1..=10 {
            let mut ev = ContextEvidence::observed_domain(
                "target_github_urls",
                "github.com",
                Some(&format!("https://github.com/rust-lang/rust/pull/{}", 1000 + i)),
                None,
            );
            ev.strength = 1.0;
            evidence_set.add(ev);
        }

        let mut ws = WorkspaceScoringContext::new("ws_rust", "Rust Development");
        ws.domains.push("github.com".to_string());

        let scorer = ContextEvidenceScorer::default();
        let result = scorer.score_evidence_against_workspace(&evidence_set, &ws);

        assert!(result.total_score > 0.0, "Total score should have domain score");
        assert!(
            result.confidence <= 0.35,
            "10 URLs from same domain must not inflate confidence beyond 0.35 (actual: {})",
            result.confidence
        );
        assert_ne!(result.confidence_band, "High");

        let factors = result.confidence_factors.expect("confidence factors must be present");
        assert_eq!(factors.independent_groups_count, 1);
        assert_eq!(factors.diversity_count, 1);
        assert!(factors.diversity_multiplier <= 0.35);
        assert!(factors.diversity_score <= 0.30);
    }

    #[test]
    fn test_correlated_peripheral_groups_vs_independent_strong_groups() {
        // Setup 1: Only correlated/peripheral signals (SourceApp + BrowserDomain)
        let mut evidence_set_peripheral = ContextEvidenceSet::new("target_peripheral");
        evidence_set_peripheral.add(ContextEvidence::observed_app("target_peripheral", "Google Chrome", None));
        evidence_set_peripheral.add(ContextEvidence::observed_domain("target_peripheral", "docs.google.com", None, None));

        let mut ws = WorkspaceScoringContext::new("ws_docs", "Google Docs Project");
        ws.app_names.push("Google Chrome".to_string());
        ws.domains.push("docs.google.com".to_string());

        let scorer = ContextEvidenceScorer::default();
        let result_peripheral = scorer.score_evidence_against_workspace(&evidence_set_peripheral, &ws);

        let factors_p = result_peripheral.confidence_factors.unwrap();
        assert_eq!(factors_p.independent_groups_count, 2);
        // 2 peripheral groups yield a conservative 0.50 diversity multiplier
        assert_eq!(factors_p.diversity_multiplier, 0.50);
        assert!(
            result_peripheral.confidence <= 0.35,
            "Peripheral app+domain signals should have bounded confidence (actual: {})",
            result_peripheral.confidence
        );

        // Setup 2: Add independent strong groups (Session + Semantic Centroid Match + History)
        let mut evidence_set_strong = evidence_set_peripheral.clone();
        evidence_set_strong.add(ContextEvidence::inferred_workspace_history(
            "target_peripheral",
            "ws_docs",
            "Google Docs Project",
            1.0,
            None,
        ));
        evidence_set_strong.add(ContextEvidence::observed_session("target_peripheral", "session_active", None));
        evidence_set_strong.add(ContextEvidence::inferred_semantic_similarity(
            "target_peripheral",
            "doc_peer",
            0.92,
            Some("Project Specification"),
            Some("Google Docs Project"),
            None,
        ));
        ws.sessions.push("session_active".to_string());
        ws.item_ids.push("doc_peer".to_string());

        let result_strong = scorer.score_evidence_against_workspace(&evidence_set_strong, &ws);
        let factors_s = result_strong.confidence_factors.unwrap();
        assert!(factors_s.independent_groups_count >= 3);
        assert_eq!(factors_s.diversity_multiplier, 1.00);
        assert!(factors_s.diversity_score >= 0.90);
        assert!(
            result_strong.confidence >= 0.70,
            "Independent strong signals must yield high confidence >= 0.70 (actual: {})",
            result_strong.confidence
        );
        assert!(result_strong.confidence_band == "High" || result_strong.confidence_band == "Very High");
    }

    #[test]
    fn test_evidence_diversity_calculator_direct_integration() {
        let mut contribs = Vec::new();
        contribs.push(EvidenceContribution {
            category: "same_session".to_string(),
            raw_strength: 1.0,
            weighted_score: 0.3,
            description: "Session match".to_string(),
            is_positive: true,
        });
        contribs.push(EvidenceContribution {
            category: "workspace_history".to_string(),
            raw_strength: 0.8,
            weighted_score: 0.4,
            description: "History match".to_string(),
            is_positive: true,
        });

        let (conf, band, factors) = evaluate_confidence(&contribs, 0.70, None, None, None);
        assert_eq!(factors.evidence_quantity, 2);
        assert_eq!(factors.evidence_categories_count, 2);
        assert_eq!(factors.independent_groups_count, 2);
        assert_eq!(factors.diversity_multiplier, 0.85); // 2 strong groups
        assert_eq!(factors.diversity_score, 0.80);
        assert!(factors.diversity_result.is_some());
        assert!(conf >= 0.45);
        assert_eq!(band, "Medium");
    }

    #[test]
    fn test_contradiction_detection_clear_winner() {
        // Clear winner: Context A has session, semantic, and history evidence; Context B has none.
        let mut evidence_set = ContextEvidenceSet::new("target_clear_scoring");
        evidence_set.add(ContextEvidence::observed_session("target_clear_scoring", "sess_a", None));
        evidence_set.add(ContextEvidence::inferred_semantic_similarity(
            "target_clear_scoring",
            "item_a",
            0.92,
            Some("Doc A"),
            Some("Context A"),
            None,
        ));
        evidence_set.add(ContextEvidence::inferred_workspace_history(
            "target_clear_scoring",
            "ws_a",
            "Context A",
            1.0,
            None,
        ));

        let mut ws_a = WorkspaceScoringContext::new("ws_a", "Context A");
        ws_a.sessions.push("sess_a".to_string());
        ws_a.item_ids.push("item_a".to_string());

        let ws_b = WorkspaceScoringContext::new("ws_b", "Context B");

        let scorer = ContextEvidenceScorer::default();
        let results = scorer.score_evidence(&evidence_set, &[ws_a, ws_b]);

        assert_eq!(results.len(), 2);
        let top = &results[0];
        assert_eq!(top.context_id, "ws_a");
        assert!(top.total_score >= 0.75);
        assert!(top.confidence >= 0.70);
        assert_eq!(top.confidence_band, "High");

        let factors = top.confidence_factors.as_ref().unwrap();
        assert_eq!(factors.contradiction_strength, 0.0);
        assert_eq!(factors.contradiction_penalty, 0.0);
    }

    #[test]
    fn test_contradiction_detection_two_competing_contexts() {
        // Two competing contexts: Both Context A and Context B have strong evidence with a narrow margin.
        let mut evidence_set = ContextEvidenceSet::new("target_competing_scoring");
        evidence_set.add(ContextEvidence::inferred_semantic_similarity(
            "target_competing_scoring",
            "item_a",
            0.88,
            Some("Doc A"),
            Some("Context A"),
            None,
        ));
        evidence_set.add(ContextEvidence::inferred_semantic_similarity(
            "target_competing_scoring",
            "item_b",
            0.86,
            Some("Doc B"),
            Some("Context B"),
            None,
        ));

        let mut ws_a = WorkspaceScoringContext::new("ws_a", "Context A");
        ws_a.item_ids.push("item_a".to_string());

        let mut ws_b = WorkspaceScoringContext::new("ws_b", "Context B");
        ws_b.item_ids.push("item_b".to_string());

        let scorer = ContextEvidenceScorer::default();
        let results = scorer.score_evidence(&evidence_set, &[ws_a, ws_b]);

        assert_eq!(results.len(), 2);
        let top = &results[0];
        // Invariant: Competing contexts must damp confidence below raw score and below High band
        assert!(top.confidence < top.total_score);
        assert!(top.confidence <= 0.45, "Competing candidates must not have high confidence (actual: {})", top.confidence);
        assert_ne!(top.confidence_band, "High");

        let factors = top.confidence_factors.as_ref().unwrap();
        assert!(factors.contradiction_strength > 0.0);
        assert!(top.uncertainty_indicators.iter().any(|u| u.contains("Competing evidence") || u.contains("Ambiguous context assignment")));
    }

    #[test]
    fn test_contradiction_detection_semantic_history_conflict() {
        // Semantic similarity points to Context A while user organization history points to Context B.
        let mut evidence_set = ContextEvidenceSet::new("target_sem_hist_scoring");
        evidence_set.add(ContextEvidence::inferred_semantic_similarity(
            "target_sem_hist_scoring",
            "item_a",
            0.90,
            Some("Research Paper"),
            Some("Context A"),
            None,
        ));
        evidence_set.add(ContextEvidence::inferred_workspace_history(
            "target_sem_hist_scoring",
            "ws_b",
            "Context B",
            1.0,
            None,
        ));

        let mut ws_a = WorkspaceScoringContext::new("ws_a", "Context A");
        ws_a.item_ids.push("item_a".to_string());

        let ws_b = WorkspaceScoringContext::new("ws_b", "Context B");

        let scorer = ContextEvidenceScorer::default();
        let results = scorer.score_evidence(&evidence_set, &[ws_a, ws_b]);

        let res_a = results.iter().find(|r| r.context_id == "ws_a").unwrap();
        assert!(res_a.contradiction_report.is_some());
        let rep_a = res_a.contradiction_report.as_ref().unwrap();
        assert_eq!(rep_a.contradiction_kind, Some(ContradictionKind::SemanticHistoryConflict));
        assert!(rep_a.contradiction_strength >= 0.70);
        assert!(res_a.confidence <= 0.45, "Semantic/History conflict must suppress confidence");
        assert!(res_a.uncertainty_indicators.iter().any(|u| u.contains("Semantic similarity points to 'Context A' while user organization history points to 'Context B'")));
    }

    #[test]
    fn test_contradiction_detection_explicit_user_correction() {
        // Explicit user feedback dismissal against Context A.
        let mut evidence_set = ContextEvidenceSet::new("target_user_corr_scoring");
        evidence_set.add(ContextEvidence::observed_app("target_user_corr_scoring", "Safari", None));
        evidence_set.add(ContextEvidence::observed_user_dismissal("target_user_corr_scoring", "ws_a", Some("Context A"), None));

        let mut ws_a = WorkspaceScoringContext::new("ws_a", "Context A");
        ws_a.app_names.push("Safari".to_string());

        let scorer = ContextEvidenceScorer::default();
        let result = scorer.score_evidence_against_workspace(&evidence_set, &ws_a);

        assert_eq!(result.total_score, 0.0);
        assert_eq!(result.confidence, 0.0);
        assert_eq!(result.confidence_band, "Very Low");
        assert!(result.contradictory_evidence.iter().any(|c| c.category == "user_feedback"));
        assert!(result.uncertainty_indicators.iter().any(|u| u.contains("Prior negative user feedback") || u.contains("User previously rejected")));
    }

    #[test]
    fn test_contradiction_detection_weak_discrepancy_retains_high_confidence_when_strong_evidence() {
        // Context A has strong multi-signal evidence (session + semantic + history),
        // but shares a minor application (Slack) that also exists in Context B.
        let mut evidence_set = ContextEvidenceSet::new("target_weak_scoring");
        evidence_set.add(ContextEvidence::inferred_workspace_history("target_weak_scoring", "ws_a", "Context A", 1.0, None));
        evidence_set.add(ContextEvidence::observed_session("target_weak_scoring", "sess_a", None));
        evidence_set.add(ContextEvidence::inferred_semantic_similarity(
            "target_weak_scoring",
            "doc_a",
            0.92,
            Some("Project Spec"),
            Some("Context A"),
            None,
        ));
        evidence_set.add(ContextEvidence::observed_app("target_weak_scoring", "Slack", None));

        let mut ws_a = WorkspaceScoringContext::new("ws_a", "Context A");
        ws_a.sessions.push("sess_a".to_string());
        ws_a.item_ids.push("doc_a".to_string());

        let mut ws_b = WorkspaceScoringContext::new("ws_b", "Context B");
        ws_b.app_names.push("Slack".to_string());

        let scorer = ContextEvidenceScorer::default();
        let results = scorer.score_evidence(&evidence_set, &[ws_a, ws_b]);

        let top = &results[0];
        assert_eq!(top.context_id, "ws_a");
        assert!(top.total_score >= 0.80);
        // Weak contradiction does NOT destroy confidence when backed by 3 independent strong groups
        assert!(top.confidence >= 0.65);
        assert_eq!(top.confidence_band, "High");
    }

    #[test]
    fn test_contradiction_prevents_blindly_choosing_highest_raw_score() {
        // Scenario: Context A has high raw score (0.50) purely from history, but item was captured
        // in an active session firmly belonging to Context B (score 0.40).
        let mut evidence_set = ContextEvidenceSet::new("target_session_conflict");
        evidence_set.add(ContextEvidence::inferred_workspace_history(
            "target_session_conflict",
            "ws_a",
            "Context A",
            1.0,
            None,
        ));
        evidence_set.add(ContextEvidence::observed_session("target_session_conflict", "sess_b", None));
        evidence_set.add(ContextEvidence::inferred_semantic_similarity(
            "target_session_conflict",
            "item_b",
            0.85,
            Some("Doc B"),
            Some("Context B"),
            None,
        ));

        let ws_a = WorkspaceScoringContext::new("ws_a", "Context A");

        let mut ws_b = WorkspaceScoringContext::new("ws_b", "Context B");
        ws_b.sessions.push("sess_b".to_string());
        ws_b.item_ids.push("item_b".to_string());

        let scorer = ContextEvidenceScorer::default();
        let results = scorer.score_evidence(&evidence_set, &[ws_a, ws_b]);

        let res_a = results.iter().find(|r| r.context_id == "ws_a").unwrap();
        let res_b = results.iter().find(|r| r.context_id == "ws_b").unwrap();

        // Context A suffers session conflict contradiction
        let rep_a = res_a.contradiction_report.as_ref().unwrap();
        assert_eq!(rep_a.contradiction_kind, Some(ContradictionKind::SessionConflict));
        assert!(rep_a.contradiction_strength >= 0.80);

        // Context B has higher confidence than Context A because Context B has session + semantic alignment
        assert!(res_b.confidence > res_a.confidence, "Context with aligned session and no session contradiction must have higher confidence (B: {}, A: {})", res_b.confidence, res_a.confidence);
    }

    #[test]
    fn test_contextual_competition_clear_winner() {
        // Portfolio: strong evidence across multiple independent groups (history, session, semantic, relationships) -> score ~0.92+
        // College: weak evidence (domain only) -> score ~0.41
        // Expected: AmbiguityState::Clear, score_margin ~0.51+
        let mut evidence_set = ContextEvidenceSet::new("target_comp_clear");
        evidence_set.add(ContextEvidence::inferred_workspace_history(
            "target_comp_clear",
            "ws_port",
            "Portfolio",
            1.0,
            None,
        ));
        evidence_set.add(ContextEvidence::observed_session("target_comp_clear", "sess_port", None));
        evidence_set.add(ContextEvidence::inferred_semantic_similarity(
            "target_comp_clear",
            "item_port",
            0.92,
            Some("Portfolio Spec"),
            Some("Portfolio"),
            None,
        ));
        evidence_set.add(ContextEvidence::inferred_related_item(
            "target_comp_clear",
            "item_rel",
            "shares_domain",
            0.90,
            Some("Related Doc"),
            None,
        ));
        evidence_set.add(ContextEvidence::observed_domain("target_comp_clear", "github.com", None, None));

        let mut ws_port = WorkspaceScoringContext::new("ws_port", "Portfolio");
        ws_port.sessions.push("sess_port".to_string());
        ws_port.item_ids.push("item_port".to_string());
        ws_port.item_ids.push("item_rel".to_string());

        let mut ws_col = WorkspaceScoringContext::new("ws_col", "College");
        ws_col.domains.push("github.com".to_string());

        let scorer = ContextEvidenceScorer::default();
        let results = scorer.score_evidence(&evidence_set, &[ws_port, ws_col]);

        assert_eq!(results[0].context_name, "Portfolio");
        assert_eq!(results[1].context_name, "College");

        let comp = results[0].competition_analysis.as_ref().expect("Competition analysis must be present");
        assert_eq!(comp.ambiguity_state, AmbiguityState::Clear);
        assert_eq!(comp.top_context.as_ref().unwrap().context_name, "Portfolio");
        assert_eq!(comp.second_best_context.as_ref().unwrap().context_name, "College");
        assert!(comp.score_margin >= 0.30, "Margin must be >= 0.30 for clear winner (got {})", comp.score_margin);
        assert!(comp.relative_confidence >= 0.40);
        assert!(comp.explanation.contains("Clear context match for 'Portfolio'"));

        let factors = results[0].confidence_factors.as_ref().unwrap();
        assert_eq!(factors.ambiguity_state, Some(AmbiguityState::Clear));
        assert_eq!(factors.score_margin, Some(comp.score_margin));
    }

    #[test]
    fn test_contextual_competition_ambiguous_margin() {
        // Portfolio and College both have strong, competing evidence -> score ~0.72 vs ~0.68
        // Expected: AmbiguityState::Ambiguous, narrow margin <= 0.15
        let mut evidence_set = ContextEvidenceSet::new("target_comp_ambig");
        evidence_set.add(ContextEvidence::inferred_workspace_history(
            "target_comp_ambig",
            "ws_port",
            "Portfolio",
            0.90,
            None,
        ));
        evidence_set.add(ContextEvidence::inferred_workspace_history(
            "target_comp_ambig",
            "ws_col",
            "College",
            0.85,
            None,
        ));
        evidence_set.add(ContextEvidence::inferred_semantic_similarity(
            "target_comp_ambig",
            "doc_port",
            0.90,
            Some("Port Doc"),
            Some("Portfolio"),
            None,
        ));
        evidence_set.add(ContextEvidence::inferred_semantic_similarity(
            "target_comp_ambig",
            "doc_col",
            0.85,
            Some("College Doc"),
            Some("College"),
            None,
        ));

        let mut ws_port = WorkspaceScoringContext::new("ws_port", "Portfolio");
        ws_port.item_ids.push("doc_port".to_string());

        let mut ws_col = WorkspaceScoringContext::new("ws_col", "College");
        ws_col.item_ids.push("doc_col".to_string());

        let scorer = ContextEvidenceScorer::default();
        let results = scorer.score_evidence(&evidence_set, &[ws_port, ws_col]);

        let comp = results[0].competition_analysis.as_ref().expect("Competition analysis must be present");
        assert_eq!(comp.ambiguity_state, AmbiguityState::Ambiguous);
        assert!(comp.score_margin <= 0.15, "Margin must be <= 0.15 for ambiguous (got {})", comp.score_margin);
        assert!(comp.explanation.contains("Ambiguous context assignment"));

        // Both candidates should have uncertainty indicator about ambiguous assignment
        assert!(results[0].uncertainty_indicators.iter().any(|u| u.contains("Ambiguous") || u.contains("competing")));
    }

    #[test]
    fn test_contextual_competition_unknown_weak_evidence() {
        // Portfolio and College only share weak, peripheral signals -> score ~0.43 vs ~0.41
        // Expected: AmbiguityState::Unknown
        let mut evidence_set = ContextEvidenceSet::new("target_comp_unknown");
        evidence_set.add(ContextEvidence::observed_app("target_comp_unknown", "Chrome", None));
        evidence_set.add(ContextEvidence::observed_domain("target_comp_unknown", "google.com", None, None));

        let mut ws_port = WorkspaceScoringContext::new("ws_port", "Portfolio");
        ws_port.app_names.push("Chrome".to_string());
        ws_port.domains.push("google.com".to_string());

        let mut ws_col = WorkspaceScoringContext::new("ws_col", "College");
        ws_col.app_names.push("Chrome".to_string());
        ws_col.domains.push("google.com".to_string());

        let scorer = ContextEvidenceScorer::default();
        let results = scorer.score_evidence(&evidence_set, &[ws_port, ws_col]);

        let comp = results[0].competition_analysis.as_ref().expect("Competition analysis must be present");
        assert_eq!(comp.ambiguity_state, AmbiguityState::Unknown);
        assert!(comp.explanation.contains("Unknown context assignment"));
    }

    #[test]
    fn test_contextual_competition_probable_margin() {
        // Portfolio has moderate score (~0.65), College has lower score (~0.40) -> margin ~0.25
        // Expected: AmbiguityState::Probable
        let mut evidence_set = ContextEvidenceSet::new("target_comp_prob");
        evidence_set.add(ContextEvidence::observed_session("target_comp_prob", "sess_port", None));
        evidence_set.add(ContextEvidence::inferred_semantic_similarity(
            "target_comp_prob",
            "doc_port",
            0.80,
            Some("Port Doc"),
            Some("Portfolio"),
            None,
        ));
        evidence_set.add(ContextEvidence::observed_app("target_comp_prob", "VSCode", None));

        let mut ws_port = WorkspaceScoringContext::new("ws_port", "Portfolio");
        ws_port.sessions.push("sess_port".to_string());
        ws_port.item_ids.push("doc_port".to_string());

        let mut ws_col = WorkspaceScoringContext::new("ws_col", "College");
        ws_col.app_names.push("VSCode".to_string());

        let scorer = ContextEvidenceScorer::default();
        let results = scorer.score_evidence(&evidence_set, &[ws_port, ws_col]);

        let comp = results[0].competition_analysis.as_ref().expect("Competition analysis must be present");
        assert_eq!(comp.ambiguity_state, AmbiguityState::Probable);
        assert!(comp.score_margin >= 0.15);
        assert!(comp.explanation.contains("Probable context match for 'Portfolio'"));
    }

    #[test]
    fn test_contextual_competition_zero_membership_mutation() {
        let conn = setup_in_memory_db();
        let ws1 = crate::db::operations::create_workspace(&conn, "Portfolio").unwrap();
        let ws2 = crate::db::operations::create_workspace(&conn, "College").unwrap();
        let item = crate::db::operations::create_item(&conn, None, None, "url", "https://example.com", Some("Competition Test Doc"), "{}").unwrap();

        // Run evaluate_item_context
        let scores = evaluate_item_context(&conn, &item.id).unwrap();
        assert!(!scores.existing_workspaces.is_empty());

        // Invariant check: Item's workspace membership and item row must NOT be modified
        let item_check = crate::db::operations::get_item(&conn, &item.id).unwrap();
        assert_eq!(item_check.workspace_id, None, "Scoring and competition analysis must never mutate item workspace_id");

        let m1 = crate::db::operations::get_item_workspace_membership(&conn, &item.id, &ws1.id).unwrap();
        assert!(m1.is_none() || m1.unwrap().membership_type != "confirmed");
        let m2 = crate::db::operations::get_item_workspace_membership(&conn, &item.id, &ws2.id).unwrap();
        assert!(m2.is_none() || m2.unwrap().membership_type != "confirmed");
    }

    #[test]
    fn test_no_context_zero_evidence_returns_explicit_diagnostic() {
        use crate::db::models::{ContextInterpretationKind, NoContextReason};

        let conn = setup_in_memory_db();
        let _ws = crate::db::operations::create_workspace(&conn, "Portfolio").unwrap();
        let item = crate::db::operations::create_item(&conn, None, None, "", "empty item", None, "{}").unwrap();

        let interp = evaluate_item_context(&conn, &item.id).unwrap();

        assert_eq!(interp.interpretation_kind, Some(ContextInterpretationKind::NoContext));
        assert_eq!(interp.primary_interpretation, "no_context");
        assert_eq!(interp.no_context_reason, Some(NoContextReason::ZeroEvidence));
        assert!(interp.summary.starts_with("No strong context yet"));
        assert!(interp.summary.contains("No contextual evidence or capture metadata is available"));

        // Direct diagnostics test
        let empty_set = ContextEvidenceSet::new("empty");
        let (reason, expl) = diagnose_no_context_reason(&empty_set, &[], 0);
        assert_eq!(reason, NoContextReason::ZeroEvidence);
        assert_eq!(reason.as_str(), "zero_evidence");
        assert!(expl.contains("No contextual evidence"));
    }

    #[test]
    fn test_no_context_weak_peripheral_signals_only() {
        use crate::db::models::{ContextInterpretationKind, NoContextReason};

        let conn = setup_in_memory_db();
        let ws = crate::db::operations::create_workspace(&conn, "General Web").unwrap();
        let item = crate::db::operations::create_item(&conn, None, None, "url", "https://example.com/page", Some("Example"), "{\"app_name\":\"Safari\"}").unwrap();
        let _ = create_context_signal(&conn, None, Some(&item.id), "active_app", None, "{\"app_name\":\"Safari\"}").unwrap();
        let _ = create_context_signal(&conn, None, Some(&item.id), "url_metadata", None, "{\"domain\":\"example.com\"}").unwrap();

        // Workspace has same app
        let ws_item = crate::db::operations::create_item(&conn, Some(&ws.id), None, "url", "https://other.com", None, "{\"app_name\":\"Safari\"}").unwrap();
        let _ = create_context_signal(&conn, Some(&ws.id), Some(&ws_item.id), "active_app", None, "{\"app_name\":\"Safari\"}").unwrap();

        let interp = evaluate_item_context(&conn, &item.id).unwrap();

        assert_eq!(interp.interpretation_kind, Some(ContextInterpretationKind::NoContext));
        assert_eq!(interp.primary_interpretation, "no_context");
        assert_eq!(interp.no_context_reason, Some(NoContextReason::WeakEvidence));
        assert!(interp.summary.starts_with("No strong context yet"));
        assert!(interp.summary.contains("isolated weak peripheral signals"));
    }

    #[test]
    fn test_no_context_conflicting_contexts_user_dismissal() {
        use crate::db::models::{ContextInterpretationKind, NoContextReason};

        let conn = setup_in_memory_db();
        let ws = crate::db::operations::create_workspace(&conn, "Dismissed WS").unwrap();
        let item = crate::db::operations::create_item(&conn, None, None, "text", "doc content", Some("Doc"), "{}").unwrap();

        // History signal points to WS, but user feedback dismisses WS
        let hist_payload = serde_json::json!({"action": "moved_to_workspace", "destination_workspace": ws.id}).to_string();
        let _ = create_context_signal(&conn, None, Some(&item.id), "user_action", None, &hist_payload).unwrap();
        let dismiss_payload = serde_json::json!({"action": "suggestion_dismissed", "workspace_id": ws.id}).to_string();
        let _ = create_context_signal(&conn, None, Some(&item.id), "suggestion_dismissed", None, &dismiss_payload).unwrap();

        let interp = evaluate_item_context(&conn, &item.id).unwrap();

        assert_eq!(interp.interpretation_kind, Some(ContextInterpretationKind::NoContext));
        assert_eq!(interp.no_context_reason, Some(NoContextReason::ConflictingContexts));
        assert!(interp.summary.starts_with("No strong context yet"));
        assert!(interp.summary.contains("Strong contradictory evidence") || interp.summary.contains("user dismissal"));
    }

    #[test]
    fn test_no_context_no_meaningful_relationships() {
        use crate::db::models::NoContextReason;
        use crate::context_engine::evidence::EvidenceCategory;

        let mut evidence_set = ContextEvidenceSet::new("target_no_rels");
        evidence_set.add(ContextEvidence::new(
            EvidenceCategory::WorkspaceHistory,
            crate::context_engine::evidence::EvidenceProvenance::Inferred,
            0.15,
            "target_no_rels",
            crate::context_engine::evidence::EvidenceExplanation::new("Weak history", "Historical correlation"),
        ));

        let ws = WorkspaceScoringContext::new("ws_isolated", "Isolated Workspace");
        let scorer = ContextEvidenceScorer::default();
        let results = scorer.score_evidence(&evidence_set, &[ws]);

        let (reason, explanation) = diagnose_no_context_reason(&evidence_set, &results, 0);

        assert_eq!(reason, NoContextReason::NoMeaningfulRelationships);
        assert!(explanation.contains("Item has no meaningful contextual graph relationships"));
    }

    #[test]
    fn test_no_context_low_semantic_relevance() {
        use crate::db::models::NoContextReason;
        use crate::context_engine::evidence::EvidenceCategory;

        let mut evidence_set = ContextEvidenceSet::new("target_low_sem");
        evidence_set.add(ContextEvidence::new(
            EvidenceCategory::SemanticSimilarity,
            crate::context_engine::evidence::EvidenceProvenance::Inferred,
            0.18,
            "target_low_sem",
            crate::context_engine::evidence::EvidenceExplanation::new("Low similarity", "Low cosine similarity"),
        ));

        let ws = WorkspaceScoringContext::new("ws_sem", "Semantic Workspace");
        let scorer = ContextEvidenceScorer::default();
        let results = scorer.score_evidence(&evidence_set, &[ws]);

        let (reason, explanation) = diagnose_no_context_reason(&evidence_set, &results, 2);

        assert_eq!(reason, NoContextReason::LowSemanticRelevance);
        assert!(explanation.contains("Semantic similarity to workspace contents and profiles is too low"));
    }

    #[test]
    fn test_no_context_insufficient_evidence_diversity() {
        use crate::db::models::NoContextReason;

        let mut evidence_set = ContextEvidenceSet::new("target_repeated_domain");
        for i in 1..=5 {
            evidence_set.add(ContextEvidence::observed_domain("target_repeated_domain", "example.com", Some(&format!("https://example.com/{}", i)), None));
        }

        let mut ws = WorkspaceScoringContext::new("ws_dev", "Dev Workspace");
        ws.domains.push("example.com".to_string());

        let scorer = ContextEvidenceScorer::default();
        let results = scorer.score_evidence(&evidence_set, &[ws]);

        let (reason, explanation) = diagnose_no_context_reason(&evidence_set, &results, 0);

        assert_eq!(reason, NoContextReason::InsufficientEvidenceDiversity);
        assert!(explanation.contains("lacks independent signal categories"));
    }

    #[test]
    fn test_first_class_context_interpretation_kinds_matrix() {
        use crate::db::models::ContextInterpretationKind;

        assert_eq!(ContextInterpretationKind::ExistingContext.as_str(), "existing_context");
        assert_eq!(ContextInterpretationKind::NewContextCandidate.as_str(), "new_context_candidate");
        assert_eq!(ContextInterpretationKind::Ambiguous.as_str(), "ambiguous");
        assert_eq!(ContextInterpretationKind::NoContext.as_str(), "no_context");

        assert_eq!(ContextInterpretationKind::from_str_lenient("existing_workspace"), ContextInterpretationKind::ExistingContext);
        assert_eq!(ContextInterpretationKind::from_str_lenient("new_context"), ContextInterpretationKind::NewContextCandidate);
        assert_eq!(ContextInterpretationKind::from_str_lenient("uncertain"), ContextInterpretationKind::Ambiguous);
        assert_eq!(ContextInterpretationKind::from_str_lenient("unassigned"), ContextInterpretationKind::NoContext);
        assert_eq!(ContextInterpretationKind::from_str_lenient("none"), ContextInterpretationKind::NoContext);
    }
}

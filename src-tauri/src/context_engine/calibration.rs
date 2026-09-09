use serde::{Deserialize, Serialize};

use crate::context_engine::competition::{AmbiguityState, ContextCompetitionAnalyzer};
use crate::context_engine::diversity::{
    EvidenceDiversityCalculator, EvidenceDiversityResult,
};
use crate::context_engine::scoring::{category_reliability, ConfidenceFactors, EvidenceContribution};

/// Explicit 5-tier confidence classification for ContextSplit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceBand {
    /// Insufficient, negligible, or absent contextual evidence (score < 0.15 or zero evidence)
    VeryLow,
    /// Weak, isolated, or heavily disputed contextual evidence (0.15 <= conf < 0.40)
    Low,
    /// Moderate contextual alignment or single strong category awaiting corroboration (0.40 <= conf < 0.65)
    Medium,
    /// Strong, corroborated contextual match across multiple independent evidence sources (0.65 <= conf < 0.85)
    High,
    /// Definite, decisive contextual match with authoritative or multi-signal corroboration (conf >= 0.85)
    VeryHigh,
}

impl ConfidenceBand {
    pub const VERY_HIGH_THRESHOLD: f32 = 0.85;
    pub const HIGH_THRESHOLD: f32 = 0.65;
    pub const MEDIUM_THRESHOLD: f32 = 0.40;
    pub const LOW_THRESHOLD: f32 = 0.15;

    pub fn as_str(&self) -> &'static str {
        match self {
            ConfidenceBand::VeryLow => "very_low",
            ConfidenceBand::Low => "low",
            ConfidenceBand::Medium => "medium",
            ConfidenceBand::High => "high",
            ConfidenceBand::VeryHigh => "very_high",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ConfidenceBand::VeryLow => "Very Low",
            ConfidenceBand::Low => "Low",
            ConfidenceBand::Medium => "Medium",
            ConfidenceBand::High => "High",
            ConfidenceBand::VeryHigh => "Very High",
        }
    }

    /// Whether this confidence band represents an actionable, high-certainty suggestion.
    pub fn is_actionable(&self) -> bool {
        matches!(self, ConfidenceBand::High | ConfidenceBand::VeryHigh)
    }

    /// Whether this confidence band represents a suggestion requiring user review.
    pub fn requires_review(&self) -> bool {
        matches!(
            self,
            ConfidenceBand::VeryLow | ConfidenceBand::Low | ConfidenceBand::Medium
        )
    }

    /// Lenient parser supporting both new 5-tier strings and legacy formats.
    pub fn from_str_lenient(s: &str) -> Self {
        let normalized = s.trim().to_lowercase().replace(['-', ' '], "_");
        match normalized.as_str() {
            "very_high" | "veryhigh" | "authoritative" => ConfidenceBand::VeryHigh,
            "high" => ConfidenceBand::High,
            "medium" | "moderate" => ConfidenceBand::Medium,
            "low" | "weak" | "uncertain" => ConfidenceBand::Low,
            "very_low" | "verylow" | "none" | "" => ConfidenceBand::VeryLow,
            _ => ConfidenceBand::Low,
        }
    }

    /// Map a numeric confidence score directly into a ConfidenceBand.
    pub fn from_confidence(confidence: f32) -> Self {
        if confidence >= Self::VERY_HIGH_THRESHOLD {
            ConfidenceBand::VeryHigh
        } else if confidence >= Self::HIGH_THRESHOLD {
            ConfidenceBand::High
        } else if confidence >= Self::MEDIUM_THRESHOLD {
            ConfidenceBand::Medium
        } else if confidence >= Self::LOW_THRESHOLD {
            ConfidenceBand::Low
        } else {
            ConfidenceBand::VeryLow
        }
    }

    pub fn from_f64(confidence: f64) -> Self {
        Self::from_confidence(confidence as f32)
    }
}

/// Input parameters provided to the `ConfidenceCalibrator` service.
#[derive(Debug, Clone)]
pub struct CalibrationInput<'a> {
    pub raw_score: f32,
    pub contributions: &'a [EvidenceContribution],
    pub margin_to_runner_up: Option<f32>,
    pub runner_up_score: Option<f32>,
    pub agreement_score: Option<f32>,
    pub contradiction_strength: f32,
    pub contradiction_penalty: f32,
    pub ambiguity_state: Option<AmbiguityState>,
}

impl<'a> CalibrationInput<'a> {
    pub fn new(raw_score: f32, contributions: &'a [EvidenceContribution]) -> Self {
        Self {
            raw_score,
            contributions,
            margin_to_runner_up: None,
            runner_up_score: None,
            agreement_score: None,
            contradiction_strength: 0.0,
            contradiction_penalty: 0.0,
            ambiguity_state: None,
        }
    }

    pub fn with_competition(
        mut self,
        margin: Option<f32>,
        runner_up_score: Option<f32>,
        ambiguity_state: Option<AmbiguityState>,
    ) -> Self {
        self.margin_to_runner_up = margin;
        self.runner_up_score = runner_up_score;
        self.ambiguity_state = ambiguity_state;
        self
    }

    pub fn with_contradiction(mut self, strength: f32, penalty: f32) -> Self {
        self.contradiction_strength = strength;
        self.contradiction_penalty = penalty;
        self
    }

    pub fn with_agreement(mut self, agreement: Option<f32>) -> Self {
        self.agreement_score = agreement;
        self
    }
}

/// Output result produced by `ConfidenceCalibrator`.
#[derive(Debug, Clone, PartialEq)]
pub struct CalibrationResult {
    pub calibrated_confidence: f32,
    pub band: ConfidenceBand,
    pub band_name: String,
    pub factors: ConfidenceFactors,
    pub rationale: Vec<String>,
}

/// Centralized Confidence Calibration Service for ContextSplit.
///
/// Combines raw scores, evidence diversity, reliability ratings, contradictions,
/// competition margins, and minimum evidence requirements into a principled,
/// calibrated confidence value and structured confidence band.
#[derive(Debug, Clone, Default)]
pub struct ConfidenceCalibrator;

impl ConfidenceCalibrator {
    pub const AMBIGUITY_MARGIN: f32 = 0.15;
    pub const AMBIGUITY_RUNNER_UP_FLOOR: f32 = 0.20;

    pub fn new() -> Self {
        Self
    }

    /// Calibrates confidence from structured input parameters.
    pub fn calibrate(&self, input: &CalibrationInput) -> CalibrationResult {
        let contributions = input.contributions;
        let total_score = input.raw_score;
        let margin_to_runner_up = input.margin_to_runner_up;
        let runner_up_score = input.runner_up_score;
        let agreement_score = input.agreement_score;
        let contradiction_strength = input.contradiction_strength;
        let contradiction_penalty = input.contradiction_penalty;

        let mut rationale = Vec::new();

        // 1. Authoritative explicit confirmed membership is always 100% confident
        if contributions
            .iter()
            .any(|c| c.category == "confirmed_membership" && c.is_positive && c.weighted_score > 0.0)
        {
            let div = EvidenceDiversityCalculator::calculate_from_contributions(contributions);
            rationale.push("Confirmed membership provides 100% authoritative confidence.".to_string());
            return CalibrationResult {
                calibrated_confidence: 1.0,
                band: ConfidenceBand::VeryHigh,
                band_name: "very_high".to_string(),
                factors: ConfidenceFactors {
                    diversity_count: div.independent_groups_count.max(1),
                    diversity_multiplier: 1.0,
                    reliability_score: 1.0,
                    agreement_score: 1.0,
                    ambiguity_penalty: 0.0,
                    evidence_volume_score: 1.0,
                    evidence_quantity: div.evidence_quantity,
                    evidence_categories_count: div.evidence_categories_count,
                    independent_groups_count: div.independent_groups_count,
                    diversity_score: 1.0,
                    diversity_result: Some(div),
                    contradiction_strength: 0.0,
                    contradiction_penalty: 0.0,
                    score_margin: None,
                    relative_confidence: Some(1.0),
                    ambiguity_state: Some(AmbiguityState::Clear),
                    band: Some(ConfidenceBand::VeryHigh),
                },
                rationale,
            };
        }

        let pos_contribs: Vec<&EvidenceContribution> = contributions
            .iter()
            .filter(|c| c.is_positive && c.weighted_score > 0.0)
            .collect();

        if pos_contribs.is_empty() || total_score <= 0.0 {
            rationale.push("Zero positive contextual evidence available.".to_string());
            return CalibrationResult {
                calibrated_confidence: 0.0,
                band: ConfidenceBand::VeryLow,
                band_name: "very_low".to_string(),
                factors: ConfidenceFactors {
                    diversity_count: 0,
                    diversity_multiplier: 0.0,
                    reliability_score: 0.0,
                    agreement_score: 0.0,
                    ambiguity_penalty: 0.0,
                    evidence_volume_score: 0.0,
                    evidence_quantity: 0,
                    evidence_categories_count: 0,
                    independent_groups_count: 0,
                    diversity_score: 0.0,
                    diversity_result: None,
                    contradiction_strength,
                    contradiction_penalty,
                    score_margin: None,
                    relative_confidence: Some(0.0),
                    ambiguity_state: Some(AmbiguityState::Unknown),
                    band: Some(ConfidenceBand::VeryLow),
                },
                rationale,
            };
        }

        // 2. Evidence Diversity & Independence
        let diversity_result = EvidenceDiversityCalculator::calculate_from_contributions(contributions);
        let diversity_count = diversity_result.independent_groups_count;
        let diversity_multiplier = diversity_result.diversity_multiplier;
        let diversity_score = diversity_result.diversity_score;

        if diversity_count <= 1 {
            rationale.push("Single evidence category limits confidence until corroborated by independent sources.".to_string());
        }

        // 3. Category Reliability (weighted by positive contributions)
        let total_pos_weight: f32 = pos_contribs.iter().map(|c| c.weighted_score).sum();
        let reliability_score = if total_pos_weight > 0.0 {
            pos_contribs
                .iter()
                .map(|c| c.weighted_score * category_reliability(&c.category))
                .sum::<f32>()
                / total_pos_weight
        } else {
            0.50
        };
        let reliability_multiplier = 0.70 + (0.30 * reliability_score);

        // 4. Evidence Volume Score
        let evidence_volume_score = match pos_contribs.len() {
            0 => 0.0,
            1 => 0.75,
            2 => 0.90,
            _ => 1.00,
        };

        // 5. Agreement Score
        let agreement = agreement_score.unwrap_or(1.0).clamp(0.0, 1.0);
        let agreement_multiplier = 0.70 + (0.30 * agreement);

        // 6. Ambiguity & Competition Penalty
        let ambiguity_penalty = match (margin_to_runner_up, runner_up_score) {
            (Some(margin), Some(runner_up))
                if margin <= Self::AMBIGUITY_MARGIN && runner_up >= Self::AMBIGUITY_RUNNER_UP_FLOOR =>
            {
                0.35 * (1.0 - (margin / Self::AMBIGUITY_MARGIN).clamp(0.0, 1.0))
            }
            _ => 0.0,
        };

        if ambiguity_penalty > 0.0 {
            rationale.push(format!(
                "Ambiguity penalty (-{:.2}) applied due to close competition.",
                ambiguity_penalty
            ));
        }

        if contradiction_penalty > 0.0 || contradiction_strength > 0.0 {
            rationale.push(format!(
                "Contradiction discount applied (penalty: {:.2}, strength: {:.2}).",
                contradiction_penalty, contradiction_strength
            ));
        }

        let total_penalty =
            ambiguity_penalty.max(contradiction_penalty) + (0.08 * contradiction_strength);

        let base_confidence = total_score
            * diversity_multiplier
            * reliability_multiplier
            * evidence_volume_score
            * agreement_multiplier;

        let mut clamped_confidence = (base_confidence - total_penalty).clamp(0.0, 1.0);

        let ambiguity_state = input.ambiguity_state.or_else(|| {
            Some(ContextCompetitionAnalyzer::classify_ambiguity(
                total_score,
                runner_up_score.unwrap_or(0.0),
                clamped_confidence,
            ))
        });

        let relative_confidence = Some(ContextCompetitionAnalyzer::calculate_relative_confidence(
            total_score,
            runner_up_score.unwrap_or(0.0),
            clamped_confidence,
        ));

        // 7. Minimum Evidence Requirements & Confidence Band Classification
        let band = Self::classify_band_with_gating(
            clamped_confidence,
            total_score,
            &diversity_result,
            contradiction_strength,
            ambiguity_state,
            margin_to_runner_up,
        );

        // Adjust clamped_confidence if band was capped by gating rules
        match band {
            ConfidenceBand::Medium if clamped_confidence >= ConfidenceBand::HIGH_THRESHOLD => {
                clamped_confidence = (ConfidenceBand::HIGH_THRESHOLD - 0.01).min(clamped_confidence);
            }
            ConfidenceBand::Low if clamped_confidence >= ConfidenceBand::MEDIUM_THRESHOLD => {
                clamped_confidence = (ConfidenceBand::MEDIUM_THRESHOLD - 0.01).min(clamped_confidence);
            }
            ConfidenceBand::VeryLow if clamped_confidence >= ConfidenceBand::LOW_THRESHOLD => {
                clamped_confidence = (ConfidenceBand::LOW_THRESHOLD - 0.01).min(clamped_confidence);
            }
            _ => {}
        }

        let factors = ConfidenceFactors {
            diversity_count,
            diversity_multiplier,
            reliability_score,
            agreement_score: agreement,
            ambiguity_penalty,
            evidence_volume_score,
            evidence_quantity: diversity_result.evidence_quantity,
            evidence_categories_count: diversity_result.evidence_categories_count,
            independent_groups_count: diversity_result.independent_groups_count,
            diversity_score,
            diversity_result: Some(diversity_result),
            contradiction_strength,
            contradiction_penalty,
            score_margin: margin_to_runner_up,
            relative_confidence,
            ambiguity_state,
            band: Some(band),
        };

        CalibrationResult {
            calibrated_confidence: clamped_confidence,
            band,
            band_name: band.as_str().to_string(),
            factors,
            rationale,
        }
    }

    /// Principle-based minimum evidence gating and confidence band classification.
    pub fn classify_band_with_gating(
        confidence: f32,
        raw_score: f32,
        diversity: &EvidenceDiversityResult,
        contradiction_strength: f32,
        ambiguity: Option<AmbiguityState>,
        margin: Option<f32>,
    ) -> ConfidenceBand {
        let is_ambiguous = ambiguity == Some(AmbiguityState::Ambiguous);
        let is_unknown = ambiguity == Some(AmbiguityState::Unknown);
        let is_clear = ambiguity == Some(AmbiguityState::Clear);

        // 1. Very High Band
        // Requires: score >= 0.75, conf >= 0.85, >= 3 positive items, >= 2 independent groups,
        // clear ambiguity state (or margin >= 0.30), low contradiction (< 0.20)
        if confidence >= ConfidenceBand::VERY_HIGH_THRESHOLD
            && raw_score >= 0.75
            && diversity.evidence_quantity >= 3
            && diversity.independent_groups_count >= 2
            && (is_clear || margin.unwrap_or(1.0) >= 0.30)
            && contradiction_strength < 0.20
        {
            return ConfidenceBand::VeryHigh;
        }

        // 2. High Band
        // Requires: conf >= 0.65, raw_score >= 0.55, >= 2 positive items, >= 2 independent groups,
        // not ambiguous, contradiction < 0.40
        if confidence >= ConfidenceBand::HIGH_THRESHOLD
            && raw_score >= 0.50
            && diversity.evidence_quantity >= 2
            && diversity.independent_groups_count >= 2
            && !is_ambiguous
            && !is_unknown
            && contradiction_strength < 0.40
        {
            return ConfidenceBand::High;
        }

        // 3. Medium Band
        // Requires: conf >= 0.40, raw_score >= 0.35, >= 1 positive item, contradiction < 0.70
        if confidence >= ConfidenceBand::MEDIUM_THRESHOLD
            && raw_score >= 0.35
            && diversity.evidence_quantity >= 1
            && contradiction_strength < 0.70
            && !is_unknown
        {
            return ConfidenceBand::Medium;
        }

        // 4. Low Band
        if confidence >= ConfidenceBand::LOW_THRESHOLD && diversity.evidence_quantity >= 1 {
            return ConfidenceBand::Low;
        }

        // 5. Very Low Band
        ConfidenceBand::VeryLow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_evidence_yields_very_low() {
        let contribs: Vec<EvidenceContribution> = Vec::new();
        let input = CalibrationInput::new(0.0, &contribs);
        let result = ConfidenceCalibrator::new().calibrate(&input);

        assert_eq!(result.band, ConfidenceBand::VeryLow);
        assert_eq!(result.band_name, "very_low");
        assert_eq!(result.calibrated_confidence, 0.0);
    }

    #[test]
    fn test_confirmed_membership_yields_very_high() {
        let contribs = vec![EvidenceContribution {
            category: "confirmed_membership".to_string(),
            raw_strength: 1.0,
            weighted_score: 1.0,
            description: "Confirmed membership".to_string(),
            is_positive: true,
        }];
        let input = CalibrationInput::new(1.0, &contribs);
        let result = ConfidenceCalibrator::new().calibrate(&input);

        assert_eq!(result.band, ConfidenceBand::VeryHigh);
        assert_eq!(result.calibrated_confidence, 1.0);
        assert!(result.band.is_actionable());
    }

    #[test]
    fn test_isolated_strong_semantic_signal_capped_at_medium() {
        // High semantic similarity (raw score 0.40) but isolated to a single group
        let contribs = vec![EvidenceContribution {
            category: "semantic_similarity".to_string(),
            raw_strength: 0.95,
            weighted_score: 0.40,
            description: "Semantic match".to_string(),
            is_positive: true,
        }];
        let input = CalibrationInput::new(0.40, &contribs);
        let result = ConfidenceCalibrator::new().calibrate(&input);

        assert_ne!(result.band, ConfidenceBand::High);
        assert_ne!(result.band, ConfidenceBand::VeryHigh);
        assert!(result.calibrated_confidence < ConfidenceBand::HIGH_THRESHOLD);
    }

    #[test]
    fn test_weak_peripheral_signals_capped_at_low() {
        // Only source application
        let contribs = vec![EvidenceContribution {
            category: "source_app".to_string(),
            raw_strength: 1.0,
            weighted_score: 0.05,
            description: "Same app".to_string(),
            is_positive: true,
        }];
        let input = CalibrationInput::new(0.05, &contribs);
        let result = ConfidenceCalibrator::new().calibrate(&input);

        assert_eq!(result.band, ConfidenceBand::VeryLow);
        assert!(!result.band.is_actionable());
    }

    #[test]
    fn test_two_competing_contexts_ambiguous_caps_confidence() {
        // Strong scores but competing closely (0.76 vs 0.69 -> margin 0.07)
        let contribs = vec![
            EvidenceContribution {
                category: "workspace_history".to_string(),
                raw_strength: 0.90,
                weighted_score: 0.36,
                description: "History".to_string(),
                is_positive: true,
            },
            EvidenceContribution {
                category: "semantic_similarity".to_string(),
                raw_strength: 0.90,
                weighted_score: 0.36,
                description: "Semantic".to_string(),
                is_positive: true,
            },
        ];
        let input = CalibrationInput::new(0.72, &contribs)
            .with_competition(Some(0.07), Some(0.65), Some(AmbiguityState::Ambiguous));
        let result = ConfidenceCalibrator::new().calibrate(&input);

        assert_ne!(result.band, ConfidenceBand::High);
        assert_ne!(result.band, ConfidenceBand::VeryHigh);
        assert!(result.band == ConfidenceBand::Medium || result.band == ConfidenceBand::Low);
    }

    #[test]
    fn test_two_independent_agreeing_signals_yields_high() {
        // Session + Semantic similarity without competition
        let contribs = vec![
            EvidenceContribution {
                category: "same_session".to_string(),
                raw_strength: 1.0,
                weighted_score: 0.30,
                description: "Session".to_string(),
                is_positive: true,
            },
            EvidenceContribution {
                category: "semantic_similarity".to_string(),
                raw_strength: 0.90,
                weighted_score: 0.36,
                description: "Semantic".to_string(),
                is_positive: true,
            },
            EvidenceContribution {
                category: "workspace_history".to_string(),
                raw_strength: 0.85,
                weighted_score: 0.34,
                description: "History".to_string(),
                is_positive: true,
            },
        ];
        let input = CalibrationInput::new(0.85, &contribs)
            .with_competition(Some(0.40), Some(0.20), Some(AmbiguityState::Clear));
        let result = ConfidenceCalibrator::new().calibrate(&input);

        assert!(result.band == ConfidenceBand::High || result.band == ConfidenceBand::VeryHigh);
        assert!(result.band.is_actionable());
    }

    #[test]
    fn test_three_independent_signals_decisive_margin_yields_very_high() {
        let contribs = vec![
            EvidenceContribution {
                category: "workspace_history".to_string(),
                raw_strength: 1.0,
                weighted_score: 0.40,
                description: "User History".to_string(),
                is_positive: true,
            },
            EvidenceContribution {
                category: "same_session".to_string(),
                raw_strength: 1.0,
                weighted_score: 0.30,
                description: "Active Session".to_string(),
                is_positive: true,
            },
            EvidenceContribution {
                category: "semantic_similarity".to_string(),
                raw_strength: 0.95,
                weighted_score: 0.38,
                description: "Semantic match".to_string(),
                is_positive: true,
            },
        ];
        let input = CalibrationInput::new(0.95, &contribs)
            .with_competition(Some(0.60), Some(0.15), Some(AmbiguityState::Clear));
        let result = ConfidenceCalibrator::new().calibrate(&input);

        assert_eq!(result.band, ConfidenceBand::VeryHigh);
        assert!(result.calibrated_confidence >= ConfidenceBand::VERY_HIGH_THRESHOLD);
    }

    #[test]
    fn test_contradiction_suppresses_confidence_band() {
        let contribs = vec![
            EvidenceContribution {
                category: "same_session".to_string(),
                raw_strength: 1.0,
                weighted_score: 0.30,
                description: "Session".to_string(),
                is_positive: true,
            },
            EvidenceContribution {
                category: "semantic_similarity".to_string(),
                raw_strength: 0.90,
                weighted_score: 0.36,
                description: "Semantic".to_string(),
                is_positive: true,
            },
        ];
        // Strong contradiction (e.g. user dismissal or semantic/history conflict)
        let input = CalibrationInput::new(0.66, &contribs).with_contradiction(0.80, 0.40);
        let result = ConfidenceCalibrator::new().calibrate(&input);

        assert_ne!(result.band, ConfidenceBand::High);
        assert_ne!(result.band, ConfidenceBand::VeryHigh);
        assert!(result.calibrated_confidence < ConfidenceBand::HIGH_THRESHOLD);
    }

    #[test]
    fn test_lenient_parsing_and_display() {
        assert_eq!(ConfidenceBand::from_str_lenient("very_high"), ConfidenceBand::VeryHigh);
        assert_eq!(ConfidenceBand::from_str_lenient("High"), ConfidenceBand::High);
        assert_eq!(ConfidenceBand::from_str_lenient("medium"), ConfidenceBand::Medium);
        assert_eq!(ConfidenceBand::from_str_lenient("LOW"), ConfidenceBand::Low);
        assert_eq!(ConfidenceBand::from_str_lenient("uncertain"), ConfidenceBand::Low);
        assert_eq!(ConfidenceBand::from_str_lenient("none"), ConfidenceBand::VeryLow);

        assert_eq!(ConfidenceBand::VeryHigh.as_str(), "very_high");
        assert_eq!(ConfidenceBand::VeryHigh.display_name(), "Very High");
    }
}

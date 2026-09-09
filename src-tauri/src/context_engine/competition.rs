use serde::{Deserialize, Serialize};

use crate::context_engine::scoring::ContextScoreResult;

/// Explicit classification of contextual ambiguity across candidate contexts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AmbiguityState {
    /// Decisive winner with high score, high confidence, and a large score margin over competitors
    /// (e.g. Portfolio 92 vs College 41 -> clear)
    Clear,
    /// Plausible winner with a meaningful lead and sufficient evidence, but moderate rather than overwhelming certainty
    /// (e.g. Portfolio 70 vs College 48 -> probable)
    Probable,
    /// Multiple strong candidate contexts with close scores and narrow score margin
    /// (e.g. Portfolio 76 vs College 69 -> ambiguous)
    Ambiguous,
    /// Insufficient, sparse, or uniformly weak evidence across all candidate contexts
    /// (e.g. Portfolio 43 vs College 41 -> unknown)
    Unknown,
}

impl AmbiguityState {
    pub fn as_str(&self) -> &'static str {
        match self {
            AmbiguityState::Clear => "clear",
            AmbiguityState::Probable => "probable",
            AmbiguityState::Ambiguous => "ambiguous",
            AmbiguityState::Unknown => "unknown",
        }
    }

    /// Whether this ambiguity state represents a trustworthy context assignment recommendation.
    pub fn is_confident(&self) -> bool {
        matches!(self, AmbiguityState::Clear | AmbiguityState::Probable)
    }

    /// Returns a human-friendly description of what this ambiguity state signifies.
    pub fn description(&self) -> &'static str {
        match self {
            AmbiguityState::Clear => "Decisive context match with strong score margin over alternatives",
            AmbiguityState::Probable => "Likely context match with sufficient lead over alternatives",
            AmbiguityState::Ambiguous => "Ambiguous assignment between multiple competing contexts with close scores",
            AmbiguityState::Unknown => "Context assignment unknown due to insufficient or weak overall evidence",
        }
    }
}

/// Lightweight structured summary of a candidate context in competition analysis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextCandidateSummary {
    pub context_id: String,
    pub context_name: String,
    pub score: f32,
    pub confidence: f32,
    pub confidence_band: String,
}

impl ContextCandidateSummary {
    pub fn from_score_result(res: &ContextScoreResult) -> Self {
        Self {
            context_id: res.context_id.clone(),
            context_name: res.context_name.clone(),
            score: res.total_score,
            confidence: res.confidence,
            confidence_band: res.confidence_band.clone(),
        }
    }
}

/// Comprehensive contextual competition analysis result across candidate contexts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextCompetitionAnalysis {
    /// Top-ranked candidate context (if any)
    pub top_context: Option<ContextCandidateSummary>,
    /// Runner-up / second-best candidate context (if available)
    pub second_best_context: Option<ContextCandidateSummary>,
    /// Absolute score difference between top candidate and second-best candidate
    pub score_margin: f32,
    /// Confidence scaled relative to the strength of competing candidates in [0.0, 1.0]
    pub relative_confidence: f32,
    /// Explicit ambiguity classification
    pub ambiguity_state: AmbiguityState,
    /// Human-readable explanation of the competition dynamics and decision rationale
    pub explanation: String,
    /// All evaluated candidate contexts in ranked order
    pub all_candidates: Vec<ContextCandidateSummary>,
}

/// Dedicated analyzer for evaluating contextual competition between candidate contexts.
#[derive(Debug, Clone, Default)]
pub struct ContextCompetitionAnalyzer;

impl ContextCompetitionAnalyzer {
    pub const CLEAR_TOP_SCORE_FLOOR: f32 = 0.70;
    pub const CLEAR_CONFIDENCE_FLOOR: f32 = 0.40;
    pub const CLEAR_MARGIN_FLOOR: f32 = 0.30;
    pub const PROBABLE_TOP_SCORE_FLOOR: f32 = 0.45;
    pub const PROBABLE_MARGIN_FLOOR: f32 = 0.15;
    pub const AMBIGUOUS_SCORE_FLOOR: f32 = 0.35;
    pub const AMBIGUOUS_MAX_MARGIN: f32 = 0.15;
    pub const UNKNOWN_SCORE_CEILING: f32 = 0.35;
    pub const UNKNOWN_CLOSE_SCORE_CEILING: f32 = 0.50;

    pub fn new() -> Self {
        Self
    }

    /// Evaluates contextual competition from a collection of `ContextScoreResult` objects.
    pub fn analyze(&self, results: &[ContextScoreResult]) -> ContextCompetitionAnalysis {
        let summaries: Vec<ContextCandidateSummary> = results
            .iter()
            .map(ContextCandidateSummary::from_score_result)
            .collect();

        self.analyze_candidates(&summaries)
    }

    /// Evaluates contextual competition from candidate summaries.
    pub fn analyze_candidates(&self, candidates: &[ContextCandidateSummary]) -> ContextCompetitionAnalysis {
        let mut sorted = candidates.to_vec();
        sorted.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        if sorted.is_empty() {
            return ContextCompetitionAnalysis {
                top_context: None,
                second_best_context: None,
                score_margin: 0.0,
                relative_confidence: 0.0,
                ambiguity_state: AmbiguityState::Unknown,
                explanation: "No candidate contexts available for competition analysis.".to_string(),
                all_candidates: Vec::new(),
            };
        }

        let top = sorted[0].clone();
        let second = sorted.get(1).cloned();

        let top_score = top.score;
        let top_conf = top.confidence;
        let second_score = second.as_ref().map(|s| s.score).unwrap_or(0.0);

        let score_margin = (top_score - second_score).max(0.0);
        let relative_confidence = Self::calculate_relative_confidence(top_score, second_score, top_conf);
        let ambiguity_state = Self::classify_ambiguity(top_score, second_score, top_conf);

        let explanation = Self::format_explanation(
            &top,
            second.as_ref(),
            score_margin,
            relative_confidence,
            ambiguity_state,
        );

        ContextCompetitionAnalysis {
            top_context: Some(top),
            second_best_context: second,
            score_margin,
            relative_confidence,
            ambiguity_state,
            explanation,
            all_candidates: sorted,
        }
    }

    /// Explicitly classifies the ambiguity state between candidate contexts based on
    /// top score, second score, and top confidence.
    ///
    /// Rules:
    /// - **Clear**: Top score >= 0.70, confidence >= 0.40, margin >= 0.30 (or single candidate >= 0.70)
    /// - **Probable**: Top score >= 0.45, margin >= 0.15 (lead over second place)
    /// - **Ambiguous**: Top score >= 0.45, second-best >= 0.35, margin < 0.15 (competing close candidates)
    /// - **Unknown**: Top score < 0.35, or (top score < 0.50 and margin < 0.15) (sparse or uniformly weak evidence)
    pub fn classify_ambiguity(top_score: f32, second_score: f32, top_confidence: f32) -> AmbiguityState {
        if top_score <= 0.0 {
            return AmbiguityState::Unknown;
        }

        if top_score < Self::UNKNOWN_SCORE_CEILING {
            return AmbiguityState::Unknown;
        }

        if second_score > 0.0 {
            let margin = (top_score - second_score).max(0.0);
            const EPS: f32 = 1e-4;

            // Case 1: Low overall scores with narrow margin -> Unknown
            // Example: Portfolio 43, College 41 -> Unknown
            if top_score < Self::UNKNOWN_CLOSE_SCORE_CEILING && margin < Self::PROBABLE_MARGIN_FLOOR - EPS {
                return AmbiguityState::Unknown;
            }

            // Case 2: Substantial scores with narrow margin -> Ambiguous
            // Example: Portfolio 76, College 69 -> Ambiguous
            if top_score >= Self::PROBABLE_TOP_SCORE_FLOOR
                && second_score >= Self::AMBIGUOUS_SCORE_FLOOR
                && margin < Self::AMBIGUOUS_MAX_MARGIN - EPS
            {
                return AmbiguityState::Ambiguous;
            }

            // Case 3: High score, solid confidence, decisive margin -> Clear
            // Example: Portfolio 92, College 41 -> Clear
            if top_score >= Self::CLEAR_TOP_SCORE_FLOOR
                && top_confidence >= Self::CLEAR_CONFIDENCE_FLOOR
                && margin >= Self::CLEAR_MARGIN_FLOOR - EPS
            {
                return AmbiguityState::Clear;
            }

            // Case 4: Meaningful score and lead -> Probable
            // Example: Portfolio 70, College 48 -> Probable
            if top_score >= Self::PROBABLE_TOP_SCORE_FLOOR && margin >= Self::PROBABLE_MARGIN_FLOOR - EPS {
                return AmbiguityState::Probable;
            }

            AmbiguityState::Unknown
        } else {
            // Single candidate context in pool
            if top_score >= Self::CLEAR_TOP_SCORE_FLOOR && top_confidence >= Self::CLEAR_CONFIDENCE_FLOOR {
                AmbiguityState::Clear
            } else if top_score >= Self::PROBABLE_TOP_SCORE_FLOOR {
                AmbiguityState::Probable
            } else {
                AmbiguityState::Unknown
            }
        }
    }

    /// Computes relative confidence taking into account candidate score margin and runner-up score.
    ///
    /// Formula:
    /// relative_confidence = clamp(top_confidence * (1.0 - (second_score / top_score)), 0.0, 1.0)
    pub fn calculate_relative_confidence(top_score: f32, second_score: f32, top_confidence: f32) -> f32 {
        if top_score <= 0.0 || top_confidence <= 0.0 {
            return 0.0;
        }

        if second_score <= 0.0 {
            return top_confidence.clamp(0.0, 1.0);
        }

        let ratio = (1.0 - (second_score / top_score).clamp(0.0, 1.0)).clamp(0.0, 1.0);
        (top_confidence * ratio).clamp(0.0, 1.0)
    }

    /// Generates a human-readable explanation of competition analysis dynamics.
    fn format_explanation(
        top: &ContextCandidateSummary,
        second: Option<&ContextCandidateSummary>,
        margin: f32,
        relative_conf: f32,
        state: AmbiguityState,
    ) -> String {
        match state {
            AmbiguityState::Clear => {
                if let Some(sec) = second {
                    format!(
                        "Clear context match for '{}' ({:.0} score, {:.0}% conf) with a decisive {:.2} margin over '{}' ({:.0} score). Relative confidence: {:.0}%.",
                        top.context_name,
                        top.score * 100.0,
                        top.confidence * 100.0,
                        margin,
                        sec.context_name,
                        sec.score * 100.0,
                        relative_conf * 100.0,
                    )
                } else {
                    format!(
                        "Clear context match for '{}' ({:.0} score, {:.0}% conf) with no competing candidates.",
                        top.context_name,
                        top.score * 100.0,
                        top.confidence * 100.0,
                    )
                }
            }
            AmbiguityState::Probable => {
                if let Some(sec) = second {
                    format!(
                        "Probable context match for '{}' ({:.0} score) leading '{}' ({:.0} score) by {:.2} margin.",
                        top.context_name,
                        top.score * 100.0,
                        sec.context_name,
                        sec.score * 100.0,
                        margin,
                    )
                } else {
                    format!(
                        "Probable context match for '{}' ({:.0} score, {:.0}% conf).",
                        top.context_name,
                        top.score * 100.0,
                        top.confidence * 100.0,
                    )
                }
            }
            AmbiguityState::Ambiguous => {
                if let Some(sec) = second {
                    format!(
                        "Ambiguous context assignment: '{}' ({:.0} score) and '{}' ({:.0} score) are closely competing within {:.2} margin.",
                        top.context_name,
                        top.score * 100.0,
                        sec.context_name,
                        sec.score * 100.0,
                        margin,
                    )
                } else {
                    format!(
                        "Ambiguous context assignment for '{}' due to close competing evidence.",
                        top.context_name,
                    )
                }
            }
            AmbiguityState::Unknown => {
                if let Some(sec) = second {
                    format!(
                        "Unknown context assignment: evidence is weak across candidates ('{}' at {:.0} score vs '{}' at {:.0} score).",
                        top.context_name,
                        top.score * 100.0,
                        sec.context_name,
                        sec.score * 100.0,
                    )
                } else {
                    format!(
                        "Unknown context assignment: evidence for '{}' ({:.0} score) is too weak to establish identity.",
                        top.context_name,
                        top.score * 100.0,
                    )
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clear_winner_example() {
        // Portfolio 92, College 41 -> clear
        let top_score = 0.92;
        let second_score = 0.41;
        let top_conf = 0.85;

        let state = ContextCompetitionAnalyzer::classify_ambiguity(top_score, second_score, top_conf);
        assert_eq!(state, AmbiguityState::Clear);

        let margin = (top_score - second_score).max(0.0);
        assert!((margin - 0.51).abs() < 1e-4);

        let rel_conf = ContextCompetitionAnalyzer::calculate_relative_confidence(top_score, second_score, top_conf);
        // rel_conf = 0.85 * (1.0 - 0.41/0.92) = 0.85 * 0.5543 = 0.471
        assert!(rel_conf > 0.40);
        assert!(rel_conf <= 0.85);
    }

    #[test]
    fn test_ambiguous_example() {
        // Portfolio 76, College 69 -> ambiguous
        let top_score = 0.76;
        let second_score = 0.69;
        let top_conf = 0.70;

        let state = ContextCompetitionAnalyzer::classify_ambiguity(top_score, second_score, top_conf);
        assert_eq!(state, AmbiguityState::Ambiguous);

        let margin = (top_score - second_score).max(0.0);
        assert!((margin - 0.07).abs() < 1e-4);

        let rel_conf = ContextCompetitionAnalyzer::calculate_relative_confidence(top_score, second_score, top_conf);
        // rel_conf = 0.70 * (1.0 - 0.69/0.76) = 0.70 * 0.0921 = 0.064
        assert!(rel_conf < 0.10);
    }

    #[test]
    fn test_unknown_weak_example() {
        // Portfolio 43, College 41 -> unknown
        let top_score = 0.43;
        let second_score = 0.41;
        let top_conf = 0.30;

        let state = ContextCompetitionAnalyzer::classify_ambiguity(top_score, second_score, top_conf);
        assert_eq!(state, AmbiguityState::Unknown);

        let margin = (top_score - second_score).max(0.0);
        assert!((margin - 0.02).abs() < 1e-4);
    }

    #[test]
    fn test_probable_moderate_lead_example() {
        // Portfolio 70, College 48 -> probable
        let top_score = 0.70;
        let second_score = 0.48;
        let top_conf = 0.60;

        let state = ContextCompetitionAnalyzer::classify_ambiguity(top_score, second_score, top_conf);
        assert_eq!(state, AmbiguityState::Probable);

        let margin = (top_score - second_score).max(0.0);
        assert!((margin - 0.22).abs() < 1e-4);
        assert!(margin >= 0.15);
    }

    #[test]
    fn test_competition_analysis_candidates_pipeline() {
        let candidates = vec![
            ContextCandidateSummary {
                context_id: "ws-port".to_string(),
                context_name: "Portfolio".to_string(),
                score: 0.92,
                confidence: 0.88,
                confidence_band: "High".to_string(),
            },
            ContextCandidateSummary {
                context_id: "ws-col".to_string(),
                context_name: "College".to_string(),
                score: 0.41,
                confidence: 0.35,
                confidence_band: "Medium".to_string(),
            },
            ContextCandidateSummary {
                context_id: "ws-tax".to_string(),
                context_name: "Taxes".to_string(),
                score: 0.10,
                confidence: 0.10,
                confidence_band: "Low".to_string(),
            },
        ];

        let analyzer = ContextCompetitionAnalyzer::new();
        let analysis = analyzer.analyze_candidates(&candidates);

        assert_eq!(analysis.ambiguity_state, AmbiguityState::Clear);
        assert_eq!(analysis.top_context.as_ref().unwrap().context_name, "Portfolio");
        assert_eq!(analysis.second_best_context.as_ref().unwrap().context_name, "College");
        assert!((analysis.score_margin - 0.51).abs() < 1e-4);
        assert!(analysis.relative_confidence > 0.40);
        assert!(analysis.explanation.contains("Clear context match for 'Portfolio'"));
    }

    #[test]
    fn test_single_candidate_pool() {
        let candidates = vec![ContextCandidateSummary {
            context_id: "ws-solo".to_string(),
            context_name: "Sole Project".to_string(),
            score: 0.75,
            confidence: 0.80,
            confidence_band: "High".to_string(),
        }];

        let analyzer = ContextCompetitionAnalyzer::new();
        let analysis = analyzer.analyze_candidates(&candidates);

        assert_eq!(analysis.ambiguity_state, AmbiguityState::Clear);
        assert_eq!(analysis.second_best_context, None);
        assert_eq!(analysis.score_margin, 0.75);
        assert_eq!(analysis.relative_confidence, 0.80);
    }

    #[test]
    fn test_empty_candidates_pool() {
        let candidates: Vec<ContextCandidateSummary> = Vec::new();
        let analyzer = ContextCompetitionAnalyzer::new();
        let analysis = analyzer.analyze_candidates(&candidates);

        assert_eq!(analysis.ambiguity_state, AmbiguityState::Unknown);
        assert_eq!(analysis.top_context, None);
        assert_eq!(analysis.second_best_context, None);
        assert_eq!(analysis.score_margin, 0.0);
        assert_eq!(analysis.relative_confidence, 0.0);
    }

    #[test]
    fn test_different_score_margins_matrix() {
        // Test margin 0.00 (identical scores)
        let s_ident = ContextCompetitionAnalyzer::classify_ambiguity(0.70, 0.70, 0.65);
        assert_eq!(s_ident, AmbiguityState::Ambiguous);

        // Test margin 0.05 (tight competition)
        let s_tight = ContextCompetitionAnalyzer::classify_ambiguity(0.80, 0.75, 0.75);
        assert_eq!(s_tight, AmbiguityState::Ambiguous);

        // Test margin 0.15 (boundary between ambiguous and probable)
        let s_bound = ContextCompetitionAnalyzer::classify_ambiguity(0.65, 0.50, 0.60);
        assert_eq!(s_bound, AmbiguityState::Probable);

        // Test margin 0.35 (clear lead with high score)
        let s_clear = ContextCompetitionAnalyzer::classify_ambiguity(0.85, 0.50, 0.70);
        assert_eq!(s_clear, AmbiguityState::Clear);

        // Test very low overall scores
        let s_low = ContextCompetitionAnalyzer::classify_ambiguity(0.20, 0.18, 0.15);
        assert_eq!(s_low, AmbiguityState::Unknown);
    }
}

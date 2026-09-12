use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::context_engine::task_extractor::{
    is_generic_task_name, ExtractedTaskCandidate, TaskCandidateEvidenceRef,
};

// =========================================================================
// Data Models & Types
// =========================================================================

/// Explicit classification of ambiguity among ranked task candidates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskAmbiguityState {
    /// Decisive winner with strong multi-item evidence and clear margin over competitors
    ClearLeader,
    /// Multiple strong competing task candidates with close scores (narrow margin)
    CloseContenders,
    /// Conflicting signals, overlapping evidence, or high contradiction across candidates
    HighlyAmbiguous,
    /// Sparse evidence, single unverified item, or generic unspecialized candidates
    InsufficientEvidence,
}

impl TaskAmbiguityState {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskAmbiguityState::ClearLeader => "clear_leader",
            TaskAmbiguityState::CloseContenders => "close_contenders",
            TaskAmbiguityState::HighlyAmbiguous => "highly_ambiguous",
            TaskAmbiguityState::InsufficientEvidence => "insufficient_evidence",
        }
    }

    pub fn is_confident(&self) -> bool {
        matches!(self, TaskAmbiguityState::ClearLeader)
    }

    pub fn description(&self) -> &'static str {
        match self {
            TaskAmbiguityState::ClearLeader => "Decisive task leader with strong multi-item support and clear margin",
            TaskAmbiguityState::CloseContenders => "Multiple plausible task candidates with close relevance scores",
            TaskAmbiguityState::HighlyAmbiguous => "Conflicting evidence signals across competing task candidates",
            TaskAmbiguityState::InsufficientEvidence => "Insufficient or sparse evidence to confidently identify a specific task",
        }
    }
}

/// Detailed breakdown of scores across all 8 task ranking dimensions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskRankingDimensionScores {
    /// Recency score based on temporal decay of latest item activity [0.0, 1.0]
    pub recency_score: f64,
    /// Aggregate evidence strength across observed facts and inferences [0.0, 1.0]
    pub evidence_strength_score: f64,
    /// Diversity of independent evidence types backing the candidate [0.0, 1.0]
    pub evidence_diversity_score: f64,
    /// Semantic coherence across item titles, content, and repeated terms [0.0, 1.0]
    pub semantic_coherence_score: f64,
    /// Activity frequency / item volume in the task cluster [0.0, 1.0]
    pub activity_frequency_score: f64,
    /// Session continuity across active temporal bursts [0.0, 1.0]
    pub session_continuity_score: f64,
    /// Contextual relevance / alignment with the parent workspace [0.0, 1.0]
    pub contextual_relevance_score: f64,
    /// Task stability and persistence over time [0.0, 1.0]
    pub stability_score: f64,
}

/// A ranked and calibrated task candidate with full attribution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RankedTaskCandidate {
    /// The underlying extracted task candidate
    pub candidate: ExtractedTaskCandidate,
    /// Composite multi-factor ranking score [0.0, 1.0]
    pub score: f64,
    /// Calibrated task confidence [0.0, 1.0] (strictly decoupled from context confidence)
    pub confidence: f64,
    /// Qualitative confidence band ("high", "medium", "low", "insufficient")
    pub confidence_band: String,
    /// 1-indexed ranking order
    pub rank: usize,
    /// Explicit supporting evidence items
    pub supporting_evidence: Vec<TaskCandidateEvidenceRef>,
    /// Explicit contradictory or conflicting evidence items
    pub contradictory_evidence: Vec<TaskCandidateEvidenceRef>,
    /// Breakdown across all 8 ranking dimensions
    pub dimension_scores: TaskRankingDimensionScores,
    /// Score margin over the next-ranked candidate (if any)
    pub margin_over_next: Option<f64>,
}

/// Unified result of candidate ranking within a context or activity cluster.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskRankingResult {
    /// Ordered list of ranked task candidates (top candidate first)
    pub ranked_candidates: Vec<RankedTaskCandidate>,
    /// Top-ranked candidate if any exists
    pub top_candidate: Option<RankedTaskCandidate>,
    /// Ambiguity state across the candidate set
    pub ambiguity_state: TaskAmbiguityState,
    /// Context / Workspace ID if known
    pub context_id: Option<String>,
    /// Independent context confidence for comparison (e.g. 95% Context vs 72% Task)
    pub context_confidence: Option<f64>,
    /// Total number of candidates evaluated
    pub total_candidates_evaluated: usize,
}

/// Configuration weights and thresholds for the Task Candidate Ranker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRankingConfig {
    pub recency_weight: f64,
    pub evidence_strength_weight: f64,
    pub evidence_diversity_weight: f64,
    pub semantic_coherence_weight: f64,
    pub activity_frequency_weight: f64,
    pub session_continuity_weight: f64,
    pub contextual_relevance_weight: f64,
    pub stability_weight: f64,
    /// Maximum allowed confidence for a task backed by only 1 item (default: 0.45)
    pub single_item_max_confidence: f64,
    /// Score penalty for generic unspecialized titles (default: 0.35)
    pub generic_penalty: f64,
    /// Margin required over runner-up to be classified as ClearLeader (default: 0.15)
    pub clear_margin_threshold: f64,
}

impl Default for TaskRankingConfig {
    fn default() -> Self {
        Self {
            recency_weight: 0.15,
            evidence_strength_weight: 0.20,
            evidence_diversity_weight: 0.15,
            semantic_coherence_weight: 0.15,
            activity_frequency_weight: 0.10,
            session_continuity_weight: 0.10,
            contextual_relevance_weight: 0.10,
            stability_weight: 0.05,
            single_item_max_confidence: 0.45,
            generic_penalty: 0.35,
            clear_margin_threshold: 0.15,
        }
    }
}

// =========================================================================
// Task Candidate Ranker Service
// =========================================================================

/// Multi-factor ranking and confidence calibration engine for contextual task candidates.
pub struct TaskCandidateRanker;

impl TaskCandidateRanker {
    /// Ranks task candidates across 8 dimensions, calibrates task confidence independently
    /// from context confidence, and identifies supporting and contradictory evidence.
    pub fn rank_candidates(
        candidates: &[ExtractedTaskCandidate],
        context_confidence: Option<f64>,
        config: Option<&TaskRankingConfig>,
    ) -> TaskRankingResult {
        let cfg = config.cloned().unwrap_or_default();
        let total_candidates = candidates.len();

        if candidates.is_empty() {
            return TaskRankingResult {
                ranked_candidates: Vec::new(),
                top_candidate: None,
                ambiguity_state: TaskAmbiguityState::InsufficientEvidence,
                context_id: None,
                context_confidence,
                total_candidates_evaluated: 0,
            };
        }

        let context_id = candidates.first().and_then(|c| c.context_id.clone());

        // 1. Compute multi-factor scores and dimension breakdowns for each candidate
        let mut scored_candidates: Vec<(
            ExtractedTaskCandidate,
            f64,
            f64,
            TaskRankingDimensionScores,
            Vec<TaskCandidateEvidenceRef>,
            Vec<TaskCandidateEvidenceRef>,
        )> = Vec::new();

        for (i, candidate) in candidates.iter().enumerate() {
            let dimensions = Self::compute_dimension_scores(candidate, &cfg);
            let raw_score = Self::calculate_composite_score(&dimensions, candidate, &cfg);
            let confidence = Self::calibrate_task_confidence(raw_score, candidate, &dimensions, &cfg);

            // Separate supporting evidence (this candidate's evidence) vs contradictory evidence (from other candidates)
            let supporting = candidate.evidence_references.clone();
            let contradictory = Self::find_contradictory_evidence(i, candidates);

            scored_candidates.push((
                candidate.clone(),
                raw_score,
                confidence,
                dimensions,
                supporting,
                contradictory,
            ));
        }

        // 2. Sort descending by calibrated confidence and score
        scored_candidates.sort_by(|a, b| {
            b.2.partial_cmp(&a.2)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
        });

        // 3. Compute score margins and assemble RankedTaskCandidate structs
        let mut ranked: Vec<RankedTaskCandidate> = Vec::new();
        for (i, (candidate, score, confidence, dimensions, supporting, contradictory)) in
            scored_candidates.into_iter().enumerate()
        {
            let rank = i + 1;
            let confidence_band = match confidence {
                c if c >= 0.75 => "high".to_string(),
                c if c >= 0.50 => "medium".to_string(),
                c if c >= 0.30 => "low".to_string(),
                _ => "insufficient".to_string(),
            };

            ranked.push(RankedTaskCandidate {
                candidate,
                score,
                confidence,
                confidence_band,
                rank,
                supporting_evidence: supporting,
                contradictory_evidence: contradictory,
                dimension_scores: dimensions,
                margin_over_next: None,
            });
        }

        // Compute margins over next candidate
        for i in 0..ranked.len() {
            if i + 1 < ranked.len() {
                let margin = (ranked[i].confidence - ranked[i + 1].confidence).max(0.0);
                ranked[i].margin_over_next = Some(margin);
            }
        }

        // 4. Determine Ambiguity State
        let ambiguity_state = Self::determine_ambiguity_state(&ranked, &cfg);
        let top_candidate = ranked.first().cloned();

        TaskRankingResult {
            ranked_candidates: ranked,
            top_candidate,
            ambiguity_state,
            context_id,
            context_confidence,
            total_candidates_evaluated: total_candidates,
        }
    }

    // =========================================================================
    // Dimension Scoring
    // =========================================================================

    /// Computes individual scores across all 8 ranking dimensions.
    fn compute_dimension_scores(
        candidate: &ExtractedTaskCandidate,
        _config: &TaskRankingConfig,
    ) -> TaskRankingDimensionScores {
        let item_count = candidate.item_ids.len();

        // 1. Recency Score: Based on relevance / activity score from extraction
        let recency_score = candidate.relevance_score.clamp(0.0, 1.0);

        // 2. Evidence Strength Score: Average strength of attached evidence references
        let evidence_strength_score = if !candidate.evidence_references.is_empty() {
            let sum: f64 = candidate.evidence_references.iter().map(|e| e.strength).sum();
            (sum / candidate.evidence_references.len() as f64).clamp(0.0, 1.0)
        } else {
            0.30
        };

        // 3. Evidence Diversity Score: Ratio of unique evidence types (up to 4 types for max score)
        let unique_types: HashSet<_> = candidate
            .evidence_references
            .iter()
            .map(|e| e.evidence_type)
            .collect();
        let evidence_diversity_score = (unique_types.len() as f64 / 3.0).min(1.0);

        // 4. Semantic Coherence Score: Presence of salient terms & action verb
        let mut coherence: f64 = 0.50;
        if candidate.action_verb.is_some() && candidate.target_subject.is_some() {
            coherence += 0.25;
        }
        if !candidate.salient_terms.is_empty() {
            coherence += 0.25;
        }
        let semantic_coherence_score = coherence.clamp(0.0, 1.0);

        // 5. Activity Frequency Score: Scaled by number of items in the cluster
        let activity_frequency_score = match item_count {
            0 => 0.20,
            1 => 0.40,
            2 => 0.70,
            3..=5 => 0.90,
            _ => 1.00,
        };

        // 6. Session Continuity Score: Multi-item persistence vs isolated transient click
        let session_continuity_score = if item_count >= 2 {
            0.85
        } else if item_count == 1 {
            0.40
        } else {
            0.20
        };

        // 7. Contextual Relevance Score: Alignment with parent context (bonus for context assignment)
        let contextual_relevance_score = if candidate.context_id.is_some() {
            0.85
        } else {
            0.50
        };

        // 8. Task Stability Score: Higher when supported by both observed facts and inferred collocations
        let has_observed = candidate
            .evidence_references
            .iter()
            .any(|e| e.provenance.is_observed());
        let has_inferred = candidate
            .evidence_references
            .iter()
            .any(|e| e.provenance.is_inferred());
        let stability_score = match (has_observed, has_inferred) {
            (true, true) => 0.95,
            (true, false) => 0.75,
            (false, true) => 0.60,
            (false, false) => 0.30,
        };

        TaskRankingDimensionScores {
            recency_score,
            evidence_strength_score,
            evidence_diversity_score,
            semantic_coherence_score,
            activity_frequency_score,
            session_continuity_score,
            contextual_relevance_score,
            stability_score,
        }
    }

    /// Combines 8 dimension scores into a composite raw score [0.0, 1.0].
    fn calculate_composite_score(
        dims: &TaskRankingDimensionScores,
        candidate: &ExtractedTaskCandidate,
        config: &TaskRankingConfig,
    ) -> f64 {
        let weighted_sum = dims.recency_score * config.recency_weight
            + dims.evidence_strength_score * config.evidence_strength_weight
            + dims.evidence_diversity_score * config.evidence_diversity_weight
            + dims.semantic_coherence_score * config.semantic_coherence_weight
            + dims.activity_frequency_score * config.activity_frequency_weight
            + dims.session_continuity_score * config.session_continuity_weight
            + dims.contextual_relevance_score * config.contextual_relevance_weight
            + dims.stability_score * config.stability_weight;

        let total_weight = config.recency_weight
            + config.evidence_strength_weight
            + config.evidence_diversity_weight
            + config.semantic_coherence_weight
            + config.activity_frequency_weight
            + config.session_continuity_weight
            + config.contextual_relevance_weight
            + config.stability_weight;

        let mut score = if total_weight > 0.0 {
            weighted_sum / total_weight
        } else {
            0.50
        };

        // Apply Generic Task Name Penalty
        if is_generic_task_name(&candidate.title) {
            let is_specialized = candidate
                .metadata
                .get("is_specialized")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !is_specialized {
                score -= config.generic_penalty;
            }
        }

        score.clamp(0.05, 0.98)
    }

    /// Calibrates task confidence independently from context confidence.
    ///
    /// Critical Invariant: A single captured item MUST NOT create a high-confidence task.
    fn calibrate_task_confidence(
        raw_score: f64,
        candidate: &ExtractedTaskCandidate,
        _dims: &TaskRankingDimensionScores,
        config: &TaskRankingConfig,
    ) -> f64 {
        let mut confidence = raw_score;

        // INVARIANT: Single-item ceiling
        // A single item (or 0 items) cannot create a high-confidence task candidate.
        if candidate.item_ids.len() <= 1 {
            confidence = confidence.min(config.single_item_max_confidence);
        }

        // INVARIANT: Generic unspecialized titles ceiling
        if is_generic_task_name(&candidate.title) {
            let is_specialized = candidate
                .metadata
                .get("is_specialized")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !is_specialized {
                confidence = confidence.min(0.30);
            }
        }

        confidence.clamp(0.05, 0.98)
    }

    /// Finds contradictory evidence from other competing candidates in the same session/segment.
    fn find_contradictory_evidence(
        candidate_idx: usize,
        all_candidates: &[ExtractedTaskCandidate],
    ) -> Vec<TaskCandidateEvidenceRef> {
        let current = &all_candidates[candidate_idx];
        let current_items: HashSet<_> = current.item_ids.iter().collect();
        let mut contradictory = Vec::new();

        for (i, other) in all_candidates.iter().enumerate() {
            if i == candidate_idx {
                continue;
            }

            // Evidence from a different task in the same context represents contradictory focus
            for ev in &other.evidence_references {
                // If evidence is from an item not in current candidate's cluster
                let is_distinct_item = ev
                    .source_entity
                    .as_ref()
                    .map(|id| !current_items.contains(id))
                    .unwrap_or(false);

                if is_distinct_item && ev.strength >= 0.70 {
                    contradictory.push(ev.clone());
                }
            }
        }

        contradictory.truncate(5);
        contradictory
    }

    /// Determines the overall ambiguity state of the ranked candidates.
    fn determine_ambiguity_state(
        ranked: &[RankedTaskCandidate],
        config: &TaskRankingConfig,
    ) -> TaskAmbiguityState {
        if ranked.is_empty() {
            return TaskAmbiguityState::InsufficientEvidence;
        }

        let top = &ranked[0];

        // If top candidate has insufficient confidence or is single-item generic
        if top.confidence < 0.40 {
            return TaskAmbiguityState::InsufficientEvidence;
        }

        // Single candidate with solid confidence
        if ranked.len() == 1 {
            if top.confidence >= 0.60 && top.candidate.item_ids.len() >= 2 {
                return TaskAmbiguityState::ClearLeader;
            } else {
                return TaskAmbiguityState::InsufficientEvidence;
            }
        }

        let runner_up = &ranked[1];
        let margin = top.confidence - runner_up.confidence;

        // If runner-up is also strong and margin is small -> CloseContenders
        if runner_up.confidence >= 0.50 && margin < config.clear_margin_threshold {
            return TaskAmbiguityState::CloseContenders;
        }

        // If top candidate has significant contradictory evidence
        if top.contradictory_evidence.len() >= 3 && top.confidence < 0.70 {
            return TaskAmbiguityState::HighlyAmbiguous;
        }

        // If margin is decisive and top confidence is high -> ClearLeader
        if margin >= config.clear_margin_threshold && top.confidence >= 0.60 {
            TaskAmbiguityState::ClearLeader
        } else if top.confidence >= 0.50 {
            TaskAmbiguityState::CloseContenders
        } else {
            TaskAmbiguityState::InsufficientEvidence
        }
    }
}

// =========================================================================
// Unit Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_engine::task_evidence::{TaskEvidence, TaskEvidenceType};
    use serde_json::json;

    fn make_test_candidate(
        title: &str,
        verb: Option<&str>,
        subject: Option<&str>,
        relevance: f64,
        item_ids: Vec<String>,
        evidence_types: Vec<TaskEvidenceType>,
    ) -> ExtractedTaskCandidate {
        let evidence_references = evidence_types
            .into_iter()
            .map(|t| {
                let ev = TaskEvidence::new(
                    t,
                    t.default_provenance(),
                    0.85,
                    format!("Evidence for {}", title),
                    "Detail".to_string(),
                );
                TaskCandidateEvidenceRef::from_task_evidence(&ev)
            })
            .collect();

        ExtractedTaskCandidate {
            title: title.to_string(),
            action_verb: verb.map(|s| s.to_string()),
            target_subject: subject.map(|s| s.to_string()),
            relevance_score: relevance,
            evidence_references,
            item_ids,
            primary_evidence_types: vec![TaskEvidenceType::RecentItemContent],
            salient_terms: vec!["test".to_string()],
            context_id: Some("ws_portfolio".to_string()),
            metadata: json!({ "is_specialized": false }),
        }
    }

    #[test]
    fn test_decoupled_task_vs_context_confidence() {
        // Test user prompt example:
        // Context: Portfolio — 95%
        // Task: Hero animation — 72%
        let candidate = make_test_candidate(
            "Hero animation implementation",
            Some("Implementing"),
            Some("Hero animation"),
            0.70,
            vec!["item_1".to_string(), "item_2".to_string()],
            vec![
                TaskEvidenceType::Filenames,
                TaskEvidenceType::ApplicationContext,
            ],
        );

        let context_confidence = Some(0.95);
        let result = TaskCandidateRanker::rank_candidates(&[candidate], context_confidence, None);

        assert!(result.top_candidate.is_some());
        let top = result.top_candidate.unwrap();

        // Task confidence is independent from context confidence
        assert_eq!(result.context_confidence, Some(0.95));
        assert!(
            top.confidence >= 0.70 && top.confidence <= 0.85,
            "Expected task confidence around ~72-80%, got: {}",
            top.confidence
        );
        assert_ne!(
            top.confidence, 0.95,
            "Task confidence should not simply copy context confidence"
        );
    }

    #[test]
    fn test_single_item_confidence_ceiling() {
        // Single item task MUST NOT have high confidence (ceiling <= 0.45)
        let single_item_candidate = make_test_candidate(
            "Implementing Stripe Checkout",
            Some("Implementing"),
            Some("Stripe Checkout"),
            0.95, // High initial relevance
            vec!["item_single".to_string()], // ONLY 1 ITEM
            vec![TaskEvidenceType::UrlsDomains],
        );

        let result = TaskCandidateRanker::rank_candidates(&[single_item_candidate], None, None);
        let top = result.top_candidate.unwrap();

        assert!(
            top.confidence <= 0.45,
            "Single item candidate confidence must be capped at <= 0.45, got: {}",
            top.confidence
        );
        assert!(top.confidence_band == "low" || top.confidence_band == "insufficient");
    }

    #[test]
    fn test_multi_item_multi_evidence_high_confidence() {
        // Multi-item task with diverse evidence should achieve high confidence (> 0.75)
        let robust_candidate = make_test_candidate(
            "Reviewing PR #42: Transition Engine",
            Some("Reviewing"),
            Some("PR #42"),
            0.90,
            vec![
                "item_pr".to_string(),
                "item_file".to_string(),
                "item_vscode".to_string(),
            ],
            vec![
                TaskEvidenceType::UrlsDomains,
                TaskEvidenceType::Filenames,
                TaskEvidenceType::ApplicationContext,
                TaskEvidenceType::RepeatedTerms,
            ],
        );

        let result = TaskCandidateRanker::rank_candidates(&[robust_candidate], None, None);
        let top = result.top_candidate.unwrap();

        assert!(
            top.confidence >= 0.75,
            "Multi-item diverse candidate should have high confidence, got: {}",
            top.confidence
        );
        assert_eq!(top.confidence_band, "high");
        assert_eq!(result.ambiguity_state, TaskAmbiguityState::ClearLeader);
    }

    #[test]
    fn test_generic_term_suppression() {
        // Generic "Coding" vs Specific "Implementing JWT Verifier"
        let generic_candidate = make_test_candidate(
            "Coding",
            None,
            None,
            0.80,
            vec!["item_1".to_string(), "item_2".to_string()],
            vec![TaskEvidenceType::ApplicationContext],
        );

        let specific_candidate = make_test_candidate(
            "Implementing JWT Verifier",
            Some("Implementing"),
            Some("JWT Verifier"),
            0.80,
            vec!["item_1".to_string(), "item_2".to_string()],
            vec![TaskEvidenceType::Filenames, TaskEvidenceType::ApplicationContext],
        );

        let result = TaskCandidateRanker::rank_candidates(
            &[generic_candidate, specific_candidate],
            None,
            None,
        );

        assert_eq!(result.ranked_candidates[0].candidate.title, "Implementing JWT Verifier");
        assert_eq!(result.ranked_candidates[1].candidate.title, "Coding");
        assert!(
            result.ranked_candidates[0].confidence > result.ranked_candidates[1].confidence,
            "Specific candidate must outrank generic candidate"
        );
    }

    #[test]
    fn test_close_contenders_ambiguity_state() {
        // Two strong competing tasks in the same context
        let candidate_a = make_test_candidate(
            "Implementing Dark Mode Theme",
            Some("Implementing"),
            Some("Dark Mode Theme"),
            0.82,
            vec!["item_1".to_string(), "item_2".to_string()],
            vec![TaskEvidenceType::Filenames, TaskEvidenceType::ApplicationContext],
        );

        let candidate_b = make_test_candidate(
            "Designing Responsive Grid Layout",
            Some("Designing"),
            Some("Responsive Grid Layout"),
            0.80,
            vec!["item_3".to_string(), "item_4".to_string()],
            vec![TaskEvidenceType::UrlsDomains, TaskEvidenceType::ApplicationContext],
        );

        let result = TaskCandidateRanker::rank_candidates(
            &[candidate_a, candidate_b],
            None,
            None,
        );

        assert_eq!(result.ranked_candidates.len(), 2);
        assert_eq!(result.ambiguity_state, TaskAmbiguityState::CloseContenders);
        assert!(result.ranked_candidates[0].margin_over_next.unwrap() < 0.15);
    }

    #[test]
    fn test_contradictory_evidence_detection() {
        let mut candidate_a = make_test_candidate(
            "Implementing Dark Mode Theme",
            Some("Implementing"),
            Some("Dark Mode Theme"),
            0.85,
            vec!["item_1".to_string(), "item_2".to_string()],
            vec![TaskEvidenceType::Filenames],
        );

        let mut candidate_b = make_test_candidate(
            "Booking Paris Trip",
            Some("Booking"),
            Some("Paris Trip"),
            0.80,
            vec!["item_3".to_string()],
            vec![TaskEvidenceType::UrlsDomains],
        );

        // Attach source items to evidence
        candidate_a.evidence_references[0].source_entity = Some("item_1".to_string());
        candidate_b.evidence_references[0].source_entity = Some("item_3".to_string());

        let result = TaskCandidateRanker::rank_candidates(
            &[candidate_a, candidate_b],
            None,
            None,
        );

        let top = &result.ranked_candidates[0];
        assert!(!top.supporting_evidence.is_empty());
        assert!(!top.contradictory_evidence.is_empty(), "Should capture contradictory evidence from competing task");
    }
}

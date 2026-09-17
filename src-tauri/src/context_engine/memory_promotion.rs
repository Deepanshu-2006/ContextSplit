use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

use crate::context_engine::memory::InformationProvenance;
use crate::context_engine::memory_importance::{
    ContextMemoryImportanceEvaluator, MemoryDimensionScores, MemoryImportanceEvaluation,
    MemoryImportanceInput, MemoryImportanceTier,
};
use crate::context_engine::task_extractor::is_generic_task_name;
use crate::context_engine::workset::{
    is_accidental_capture, is_generic_noise, ContextWorksetBuilder, WorksetItemRelevance,
};

// =========================================================================
// MemoryPromotionAction: Promotion Decision Actions
// =========================================================================

/// Action determined by the Memory Promotion Engine for an entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryPromotionAction {
    /// Promoted to a higher importance tier (e.g. Temporary -> Useful, Useful -> Important, Important -> Critical).
    Promote,
    /// Retained at current tier with reinforced evidence.
    Retain,
    /// Demoted to a lower tier due to staleness, task divergence, or decay.
    Demote,
    /// Blocked from promotion (e.g. generic noise, accidental duplicates, unrelated repetition).
    Block,
}

impl MemoryPromotionAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Promote => "promote",
            Self::Retain => "retain",
            Self::Demote => "demote",
            Self::Block => "block",
        }
    }
}

// =========================================================================
// MemoryPromotionInput
// =========================================================================

/// Input signals evaluated for promoting, retaining, or demoting memory.
#[derive(Debug, Clone)]
pub struct MemoryPromotionInput<'a> {
    pub entity_id: &'a str,
    pub title: Option<&'a str>,
    pub content: &'a str,
    pub item_type: &'a str,
    pub previous_tier: Option<MemoryImportanceTier>,
    pub provenance: InformationProvenance,
    pub is_user_confirmed: bool,
    pub user_feedback: Option<&'a str>,
    pub task_relevance: f64,
    pub context_relevance: f64,
    pub workset_relevance: Option<WorksetItemRelevance>,
    pub session_count: usize,
    pub distinct_task_count: usize,
    pub relationship_degree: usize,
    pub relationship_strength: f64,
    pub first_seen_at: Option<&'a str>,
    pub last_seen_at: Option<&'a str>,
    pub is_active_task: bool,
    pub is_completed_task: bool,
    pub snapshot_presence_count: usize,
    pub signal_diversity_count: usize,
    pub has_contradictions: bool,
    pub is_accidental_duplicate: bool,
}

// =========================================================================
// MemoryPromotionDecision
// =========================================================================

/// Structured decision explaining whether and why a memory was promoted, retained, or demoted.
///
/// Invariants:
/// 1. Promotion NEVER happens solely because of capture frequency.
/// 2. Inferred information is NEVER silently converted to `Confirmed`.
/// 3. User-confirmed information remains distinct and immune to automatic demotion.
/// 4. Stale, unconfirmed items decay gracefully.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryPromotionDecision {
    /// Entity ID being evaluated
    pub entity_id: String,
    /// Previous importance tier before evaluation
    pub previous_tier: MemoryImportanceTier,
    /// Resulting importance tier after promotion rules
    pub current_tier: MemoryImportanceTier,
    /// Action taken (Promote, Retain, Demote, Block)
    pub action: MemoryPromotionAction,
    /// Epistemic source: Strictly preserved (Confirmed vs Inferred vs Observed)
    pub provenance: InformationProvenance,
    /// Human-readable reasons justifying promotion or retention
    pub promotion_reasons: Vec<String>,
    /// Reasons explaining any blockers or demotions
    pub blockers: Vec<String>,
    /// Whether user confirmed this entity
    pub is_user_confirmed: bool,
    /// Effective importance score [0.0, 1.0]
    pub effective_score: f64,
    /// Decay multiplier applied [0.0, 1.0] (1.0 = no decay)
    pub decay_applied: f64,
}

// =========================================================================
// ContextMemoryPromotionEvaluator
// =========================================================================

/// Evaluator that determines conservative memory promotion across sessions.
pub struct ContextMemoryPromotionEvaluator;

impl ContextMemoryPromotionEvaluator {
    /// Evaluates promotion decision for an input entity.
    pub fn evaluate(input: &MemoryPromotionInput) -> MemoryPromotionDecision {
        let prev_tier = input.previous_tier.unwrap_or(MemoryImportanceTier::Temporary);
        let mut reasons = Vec::new();
        let mut blockers = Vec::new();

        // 1. Accidental Capture or Duplicate Gate (Immediate Block)
        if input.is_accidental_duplicate
            || is_accidental_capture(
                input.item_type,
                input.title,
                input.content,
                input.is_user_confirmed,
            )
        {
            blockers.push("Accidental capture or duplicate rapid burst blocked from memory promotion.".to_string());
            return MemoryPromotionDecision {
                entity_id: input.entity_id.to_string(),
                previous_tier: prev_tier,
                current_tier: MemoryImportanceTier::Temporary,
                action: if prev_tier != MemoryImportanceTier::Temporary {
                    MemoryPromotionAction::Demote
                } else {
                    MemoryPromotionAction::Block
                },
                provenance: input.provenance,
                promotion_reasons: vec![],
                blockers,
                is_user_confirmed: input.is_user_confirmed,
                effective_score: 0.05,
                decay_applied: 1.0,
            };
        }

        // 2. Generic Noise Gate
        let is_task_grounded = input.task_relevance >= 0.60 || input.is_active_task;
        if is_generic_noise(
            input.title,
            input.content,
            is_task_grounded,
            input.is_user_confirmed,
        ) {
            blockers.push("Generic noise without task or workspace grounding cannot be promoted.".to_string());
            return MemoryPromotionDecision {
                entity_id: input.entity_id.to_string(),
                previous_tier: prev_tier,
                current_tier: MemoryImportanceTier::Temporary,
                action: if prev_tier != MemoryImportanceTier::Temporary {
                    MemoryPromotionAction::Demote
                } else {
                    MemoryPromotionAction::Block
                },
                provenance: input.provenance,
                promotion_reasons: vec![],
                blockers,
                is_user_confirmed: input.is_user_confirmed,
                effective_score: 0.15,
                decay_applied: 1.0,
            };
        }

        // 3. User Confirmation (Instant Critical/Important promotion with Confirmed Provenance)
        if input.is_user_confirmed || input.provenance == InformationProvenance::Confirmed {
            reasons.push("Explicitly confirmed by user.".to_string());
            if input.user_feedback == Some("pinned") {
                reasons.push("Pinned as key contextual memory anchor.".to_string());
            }

            let tier = if input.task_relevance >= 0.80 || input.workset_relevance == Some(WorksetItemRelevance::Primary) {
                MemoryImportanceTier::Critical
            } else {
                MemoryImportanceTier::Important
            };

            let action = if tier_rank(tier) > tier_rank(prev_tier) {
                MemoryPromotionAction::Promote
            } else {
                MemoryPromotionAction::Retain
            };

            return MemoryPromotionDecision {
                entity_id: input.entity_id.to_string(),
                previous_tier: prev_tier,
                current_tier: tier,
                action,
                provenance: InformationProvenance::Confirmed,
                promotion_reasons: reasons,
                blockers: vec![],
                is_user_confirmed: true,
                effective_score: 1.0,
                decay_applied: 1.0, // Confirmed memories are immune to automatic decay
            };
        }

        // 4. Calculate Base Importance Evaluation
        let importance_input = MemoryImportanceInput {
            entity_id: input.entity_id,
            title: input.title,
            content: input.content,
            item_type: input.item_type,
            provenance: input.provenance,
            is_user_confirmed: input.is_user_confirmed,
            user_feedback: input.user_feedback,
            task_relevance: input.task_relevance,
            context_relevance: input.context_relevance,
            workset_relevance: input.workset_relevance,
            session_count: input.session_count,
            relationship_degree: input.relationship_degree,
            relationship_strength: input.relationship_strength,
            first_seen_at: input.first_seen_at,
            last_seen_at: input.last_seen_at,
            is_active_task: input.is_active_task,
            is_completed_task: input.is_completed_task,
            snapshot_presence_count: input.snapshot_presence_count,
            signal_diversity_count: input.signal_diversity_count,
        };

        let base_eval = ContextMemoryImportanceEvaluator::evaluate(&importance_input);

        // 5. Compute Temporal Staleness & Decay
        let (decay_factor, is_stale) = if let Some(last_ts) = input.last_seen_at {
            compute_temporal_decay(last_ts)
        } else {
            (1.0, false)
        };

        let mut effective_score = base_eval.score * decay_factor;

        if is_stale {
            blockers.push(format!("Inactivity decay applied ({:.0}% retention factor).", decay_factor * 100.0));
        }

        // 6. Repeated Unrelated Item Check (Anti-Frequency Exploitation)
        // If an item has high session count or capture frequency, but low context/task relevance
        if input.session_count >= 2 && input.context_relevance < 0.40 && input.task_relevance < 0.35 {
            effective_score = effective_score.min(0.35);
            blockers.push("Repeated frequency without task or context relevance does not earn promotion.".to_string());
        }

        // 7. Multi-Session Meaningful Recurrence Promotion Gate
        let has_multisession_evidence = !input.has_contradictions
            && input.session_count >= 2
            && input.context_relevance >= 0.60
            && (input.task_relevance >= 0.45 || input.workset_relevance == Some(WorksetItemRelevance::Reference))
            && input.signal_diversity_count >= 2;

        if has_multisession_evidence {
            reasons.push(format!(
                "Promoted via consistent multi-session presence ({} sessions) with high task/context alignment.",
                input.session_count
            ));
            effective_score = effective_score.max(0.72);
        }

        // 8. Central to Multiple Tasks Promotion Gate
        if !input.has_contradictions && input.distinct_task_count >= 2 && input.context_relevance >= 0.65 {
            reasons.push(format!(
                "Promoted as core anchor central across {} distinct tasks.",
                input.distinct_task_count
            ));
            effective_score = effective_score.max(0.75);
        }

        // 9. Conflicting Evidence Penalty (hard suppression)
        if input.has_contradictions {
            effective_score = (effective_score * 0.60).min(0.50);
            blockers.push("Contradictory contextual signals detected, suppressing promotion.".to_string());
        }

        // 10. Determine Final Tier
        let target_tier = if effective_score >= 0.85
            && (input.is_active_task || input.session_count >= 3 || input.workset_relevance == Some(WorksetItemRelevance::Primary))
        {
            MemoryImportanceTier::Critical
        } else if effective_score >= 0.65 {
            MemoryImportanceTier::Important
        } else if effective_score >= 0.40 {
            MemoryImportanceTier::Useful
        } else {
            MemoryImportanceTier::Temporary
        };

        // Determine Action (Promote vs Retain vs Demote vs Block)
        let action = if tier_rank(target_tier) > tier_rank(prev_tier) {
            MemoryPromotionAction::Promote
        } else if tier_rank(target_tier) < tier_rank(prev_tier) {
            MemoryPromotionAction::Demote
        } else if target_tier == MemoryImportanceTier::Temporary && !blockers.is_empty() {
            MemoryPromotionAction::Block
        } else {
            MemoryPromotionAction::Retain
        };

        // Epistemic Integrity: Inferred information remains strictly Inferred or Observed
        let final_provenance = if input.provenance == InformationProvenance::Confirmed || input.is_user_confirmed {
            InformationProvenance::Confirmed
        } else if input.provenance == InformationProvenance::Inferred || input.is_active_task {
            InformationProvenance::Inferred
        } else {
            InformationProvenance::Observed
        };

        MemoryPromotionDecision {
            entity_id: input.entity_id.to_string(),
            previous_tier: prev_tier,
            current_tier: target_tier,
            action,
            provenance: final_provenance,
            promotion_reasons: reasons,
            blockers,
            is_user_confirmed: false,
            effective_score,
            decay_applied: decay_factor,
        }
    }
}

fn tier_rank(tier: MemoryImportanceTier) -> usize {
    match tier {
        MemoryImportanceTier::Stale => 0,
        MemoryImportanceTier::Temporary => 1,
        MemoryImportanceTier::Useful => 2,
        MemoryImportanceTier::Important => 3,
        MemoryImportanceTier::Critical => 4,
    }
}

fn compute_temporal_decay(last_active_at: &str) -> (f64, bool) {
    let now = Utc::now();
    let dt = if let Ok(parsed) = DateTime::parse_from_rfc3339(last_active_at) {
        parsed.with_timezone(&Utc)
    } else if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(last_active_at, "%Y-%m-%d %H:%M:%S") {
        DateTime::from_naive_utc_and_offset(naive, Utc)
    } else {
        return (1.0, false);
    };

    let elapsed = now.signed_duration_since(dt).num_seconds().max(0);
    let days = elapsed as f64 / 86400.0;

    if days <= 3.0 {
        (1.0, false)
    } else if days <= 7.0 {
        (0.85, false)
    } else if days <= 14.0 {
        (0.65, true)
    } else if days <= 30.0 {
        (0.45, true)
    } else {
        (0.25, true)
    }
}

// =========================================================================
// Unit Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repeated_relevant_item_promoted() {
        // Appears across multiple sessions with task & context relevance
        let input = MemoryPromotionInput {
            entity_id: "doc-api-spec",
            title: Some("API Specification v2"),
            content: "Detailed endpoints for ContextSplit memory refinement engine",
            item_type: "text",
            previous_tier: Some(MemoryImportanceTier::Useful),
            provenance: InformationProvenance::Observed,
            is_user_confirmed: false,
            user_feedback: None,
            task_relevance: 0.85,
            context_relevance: 0.90,
            workset_relevance: Some(WorksetItemRelevance::Primary),
            session_count: 3,
            distinct_task_count: 1,
            relationship_degree: 3,
            relationship_strength: 0.80,
            first_seen_at: Some("2026-09-15T00:00:00Z"),
            last_seen_at: Some("2026-09-18T00:00:00Z"),
            is_active_task: false,
            is_completed_task: false,
            snapshot_presence_count: 2,
            signal_diversity_count: 3,
            has_contradictions: false,
            is_accidental_duplicate: false,
        };

        let decision = ContextMemoryPromotionEvaluator::evaluate(&input);
        assert_eq!(decision.action, MemoryPromotionAction::Promote);
        assert!(matches!(
            decision.current_tier,
            MemoryImportanceTier::Important | MemoryImportanceTier::Critical
        ));
        assert_eq!(decision.provenance, InformationProvenance::Observed); // Provenance NOT silently converted to Confirmed
    }

    #[test]
    fn test_repeated_unrelated_item_not_promoted() {
        // High frequency / multi-session count, but LOW context and task relevance
        let input = MemoryPromotionInput {
            entity_id: "url-sports-news",
            title: Some("Latest Sports Scores"),
            content: "https://sports.example.com/scores",
            item_type: "url",
            previous_tier: Some(MemoryImportanceTier::Temporary),
            provenance: InformationProvenance::Observed,
            is_user_confirmed: false,
            user_feedback: None,
            task_relevance: 0.10,
            context_relevance: 0.20,
            workset_relevance: None,
            session_count: 6, // High session count should NOT promote unrelated items!
            distinct_task_count: 0,
            relationship_degree: 0,
            relationship_strength: 0.0,
            first_seen_at: Some("2026-09-10T00:00:00Z"),
            last_seen_at: Some("2026-09-18T00:00:00Z"),
            is_active_task: false,
            is_completed_task: false,
            snapshot_presence_count: 0,
            signal_diversity_count: 1,
            has_contradictions: false,
            is_accidental_duplicate: false,
        };

        let decision = ContextMemoryPromotionEvaluator::evaluate(&input);
        assert_eq!(decision.current_tier, MemoryImportanceTier::Temporary);
        assert!(!decision.current_tier.is_retained());
        assert!(decision.blockers.iter().any(|b| b.contains("Repeated frequency without task or context relevance")));
    }

    #[test]
    fn test_user_confirmation_promoted_and_distinct() {
        // User explicitly confirms item
        let input = MemoryPromotionInput {
            entity_id: "repo-core",
            title: Some("Core Engine Repository"),
            content: "https://github.com/company/core-engine",
            item_type: "url",
            previous_tier: Some(MemoryImportanceTier::Temporary),
            provenance: InformationProvenance::Confirmed,
            is_user_confirmed: true,
            user_feedback: Some("pinned"),
            task_relevance: 0.70,
            context_relevance: 0.80,
            workset_relevance: Some(WorksetItemRelevance::Reference),
            session_count: 1,
            distinct_task_count: 1,
            relationship_degree: 2,
            relationship_strength: 0.75,
            first_seen_at: Some("2026-09-18T00:00:00Z"),
            last_seen_at: Some("2026-09-18T00:00:00Z"),
            is_active_task: false,
            is_completed_task: false,
            snapshot_presence_count: 1,
            signal_diversity_count: 3,
            has_contradictions: false,
            is_accidental_duplicate: false,
        };

        let decision = ContextMemoryPromotionEvaluator::evaluate(&input);
        assert_eq!(decision.action, MemoryPromotionAction::Promote);
        assert_eq!(decision.current_tier, MemoryImportanceTier::Important);
        assert_eq!(decision.provenance, InformationProvenance::Confirmed);
        assert!(decision.is_user_confirmed);
    }

    #[test]
    fn test_task_relevance_central_item_promoted() {
        // Item central to multiple distinct tasks in the workspace
        let input = MemoryPromotionInput {
            entity_id: "doc-design-system",
            title: Some("Aurora Design System"),
            content: "Figma design tokens, typography, and color variables",
            item_type: "file",
            previous_tier: Some(MemoryImportanceTier::Useful),
            provenance: InformationProvenance::Observed,
            is_user_confirmed: false,
            user_feedback: None,
            task_relevance: 0.85,
            context_relevance: 0.95,
            workset_relevance: Some(WorksetItemRelevance::Supporting),
            session_count: 2,
            distinct_task_count: 3, // Central to 3 tasks!
            relationship_degree: 5,
            relationship_strength: 0.90,
            first_seen_at: Some("2026-09-14T00:00:00Z"),
            last_seen_at: Some("2026-09-18T00:00:00Z"),
            is_active_task: false,
            is_completed_task: false,
            snapshot_presence_count: 3,
            signal_diversity_count: 3,
            has_contradictions: false,
            is_accidental_duplicate: false,
        };

        let decision = ContextMemoryPromotionEvaluator::evaluate(&input);
        assert_eq!(decision.action, MemoryPromotionAction::Promote);
        assert!(matches!(
            decision.current_tier,
            MemoryImportanceTier::Important | MemoryImportanceTier::Critical
        ));
        assert!(decision.promotion_reasons.iter().any(|r| r.contains("distinct tasks")));
    }

    #[test]
    fn test_stale_item_demoted() {
        // Previously Important item that has been untouched for 45 days (unconfirmed)
        let input = MemoryPromotionInput {
            entity_id: "doc-old-notes",
            title: Some("Sprint 1 Scratchpad"),
            content: "Old meeting notes from August",
            item_type: "text",
            previous_tier: Some(MemoryImportanceTier::Important),
            provenance: InformationProvenance::Observed,
            is_user_confirmed: false,
            user_feedback: None,
            task_relevance: 0.20,
            context_relevance: 0.40,
            workset_relevance: None,
            session_count: 2,
            distinct_task_count: 0,
            relationship_degree: 1,
            relationship_strength: 0.30,
            first_seen_at: Some("2026-08-01T00:00:00Z"),
            last_seen_at: Some("2026-08-03T00:00:00Z"), // 45+ days stale!
            is_active_task: false,
            is_completed_task: false,
            snapshot_presence_count: 1,
            signal_diversity_count: 1,
            has_contradictions: false,
            is_accidental_duplicate: false,
        };

        let decision = ContextMemoryPromotionEvaluator::evaluate(&input);
        assert_eq!(decision.action, MemoryPromotionAction::Demote);
        assert!(matches!(
            decision.current_tier,
            MemoryImportanceTier::Useful | MemoryImportanceTier::Temporary
        ));
        assert!(decision.decay_applied <= 0.30);
    }

    #[test]
    fn test_conflicting_evidence_blocks_promotion() {
        // High apparent score but conflicting context signals
        let input = MemoryPromotionInput {
            entity_id: "doc-ambiguous-component",
            title: Some("Shared Button Component"),
            content: "React Button used across multiple competing workspaces",
            item_type: "file",
            previous_tier: Some(MemoryImportanceTier::Useful),
            provenance: InformationProvenance::Observed,
            is_user_confirmed: false,
            user_feedback: None,
            task_relevance: 0.70,
            context_relevance: 0.70,
            workset_relevance: Some(WorksetItemRelevance::Supporting),
            session_count: 2,
            distinct_task_count: 1,
            relationship_degree: 2,
            relationship_strength: 0.60,
            first_seen_at: Some("2026-09-17T00:00:00Z"),
            last_seen_at: Some("2026-09-18T00:00:00Z"),
            is_active_task: false,
            is_completed_task: false,
            snapshot_presence_count: 1,
            signal_diversity_count: 2,
            has_contradictions: true, // Contradiction flag set!
            is_accidental_duplicate: false,
        };

        let decision = ContextMemoryPromotionEvaluator::evaluate(&input);
        assert_ne!(decision.action, MemoryPromotionAction::Promote);
        assert!(decision.blockers.iter().any(|b| b.contains("Contradictory contextual signals")));
    }

    #[test]
    fn test_accidental_duplicate_captures_blocked() {
        // Accidental burst / duplicate capture
        let input = MemoryPromotionInput {
            entity_id: "url-empty-tab",
            title: Some(""),
            content: "chrome://newtab",
            item_type: "url",
            previous_tier: Some(MemoryImportanceTier::Temporary),
            provenance: InformationProvenance::Observed,
            is_user_confirmed: false,
            user_feedback: None,
            task_relevance: 0.0,
            context_relevance: 0.0,
            workset_relevance: None,
            session_count: 5,
            distinct_task_count: 0,
            relationship_degree: 0,
            relationship_strength: 0.0,
            first_seen_at: None,
            last_seen_at: None,
            is_active_task: false,
            is_completed_task: false,
            snapshot_presence_count: 0,
            signal_diversity_count: 1,
            has_contradictions: false,
            is_accidental_duplicate: true,
        };

        let decision = ContextMemoryPromotionEvaluator::evaluate(&input);
        assert_eq!(decision.action, MemoryPromotionAction::Block);
        assert_eq!(decision.current_tier, MemoryImportanceTier::Temporary);
    }
}

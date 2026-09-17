use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

use crate::context_engine::important_memory::ImportantMemoryType;
use crate::context_engine::memory::InformationProvenance;
use crate::context_engine::memory_importance::MemoryImportanceTier;
use crate::context_engine::workset::WorksetItemRelevance;

// =========================================================================
// MemoryDecayPolicy: Configurable Decay Rules
// =========================================================================

/// Configurable policies governing contextual memory decay rates and retention floors.
///
/// Invariants:
/// - System does NOT make arbitrary hardcoded claims about exact time thresholds.
/// - Polices can be adjusted per context/workspace needs.
/// - User-confirmed memories are strictly protected with zero decay.
/// - Underlying Items, Evidence, and Records are NEVER deleted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryDecayPolicy {
    /// Grace period (in hours) where a memory experiences zero decay (100% retention).
    pub grace_period_hours: f64,
    /// Half-life (in days) for temporary/inferred captures.
    pub half_life_days_temporary: f64,
    /// Half-life (in days) for useful working items.
    pub half_life_days_useful: f64,
    /// Half-life (in days) for important contextual items.
    pub half_life_days_important: f64,
    /// Half-life (in days) for reference assets (repos, API docs, style guides).
    pub half_life_days_reference: f64,
    /// Half-life (in days) for active ongoing tasks.
    pub half_life_days_active_task: f64,
    /// Baseline minimum retention floor for general items [0.0, 1.0].
    pub min_retention_floor_base: f64,
    /// Minimum retention floor for reference assets [0.0, 1.0].
    pub min_retention_floor_reference: f64,
    /// Minimum retention floor for multi-session grounded items [0.0, 1.0].
    pub min_retention_floor_multisession: f64,
    /// Importance score threshold below which unconfirmed inactive memory becomes `Stale`.
    pub stale_threshold_score: f64,
    /// Inactivity period (in days) required before unreinforced items can transition to `Stale`.
    pub stale_inactivity_days_unconfirmed: f64,
    /// Dampening multiplier for context dormancy (0.0 = full decay, 1.0 = paused decay during workspace sleep).
    pub dormancy_decay_dampening: f64,
}

impl Default for MemoryDecayPolicy {
    fn default() -> Self {
        Self {
            grace_period_hours: 24.0,
            half_life_days_temporary: 3.0,
            half_life_days_useful: 14.0,
            half_life_days_important: 30.0,
            half_life_days_reference: 60.0,
            half_life_days_active_task: 90.0,
            min_retention_floor_base: 0.05,
            min_retention_floor_reference: 0.40,
            min_retention_floor_multisession: 0.35,
            stale_threshold_score: 0.20,
            stale_inactivity_days_unconfirmed: 21.0,
            dormancy_decay_dampening: 0.70,
        }
    }
}

// =========================================================================
// MemoryDecayInput
// =========================================================================

/// Input parameters for evaluating memory importance decay.
#[derive(Debug, Clone)]
pub struct MemoryDecayInput<'a> {
    /// Entity ID being evaluated
    pub entity_id: &'a str,
    /// Initial base importance score before decay [0.0, 1.0]
    pub initial_importance: f64,
    /// Initial importance tier before decay
    pub initial_tier: MemoryImportanceTier,
    /// Provenance of the information (Confirmed, Inferred, Observed)
    pub provenance: InformationProvenance,
    /// Whether explicitly confirmed or pinned by user
    pub is_user_confirmed: bool,
    /// User feedback tag (e.g. "pinned", "thumbs_up", "dismissed")
    pub user_feedback: Option<&'a str>,
    /// Semantic memory type if categorized
    pub memory_type: Option<ImportantMemoryType>,
    /// Timestamp when this memory was last relevant or observed
    pub last_relevant_at: Option<&'a str>,
    /// Timestamp when this memory was first seen
    pub first_seen_at: Option<&'a str>,
    /// Explicit reference timestamp for deterministic evaluation (defaults to Utc::now())
    pub now: Option<DateTime<Utc>>,
    /// Whether tied to a currently active task
    pub is_active_task: bool,
    /// Whether associated with a recently completed task
    pub is_recent_task: bool,
    /// Current workset membership
    pub workset_relevance: Option<WorksetItemRelevance>,
    /// Number of distinct working sessions where this memory was relevant
    pub session_count: usize,
    /// Number of distinct tasks associated with this memory
    pub distinct_task_count: usize,
    /// Historical usefulness score [0.0, 1.0] (derived from repeated retrievals or snapshot inclusions)
    pub historical_usefulness_score: f64,
    /// Fraction of elapsed time during which the entire context/workspace was dormant [0.0, 1.0]
    pub context_dormancy_ratio: f64,
}

// =========================================================================
// MemoryDecayResult
// =========================================================================

/// Result of evaluating contextual memory decay.
///
/// Invariants:
/// - Only modifies importance score and tier for contextual retrieval.
/// - Underlying Items, Evidence, and Task transitions are strictly preserved.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryDecayResult {
    /// Entity ID evaluated
    pub entity_id: String,
    /// Pre-decay baseline importance score [0.0, 1.0]
    pub initial_importance: f64,
    /// Effective importance score after decay [0.0, 1.0]
    pub decayed_importance: f64,
    /// Importance tier before decay
    pub initial_tier: MemoryImportanceTier,
    /// Resulting importance tier after decay (Critical, Important, Useful, Temporary, or Stale)
    pub decayed_tier: MemoryImportanceTier,
    /// Retention factor multiplier [0.0, 1.0] (1.0 = zero decay)
    pub decay_factor: f64,
    /// Half-life used in decay calculation (in days)
    pub half_life_days_applied: f64,
    /// Elapsed time since last relevance (in days)
    pub elapsed_days: f64,
    /// Effective retention floor applied [0.0, 1.0]
    pub retention_floor: f64,
    /// Whether this memory has aged into `Stale` status
    pub is_stale: bool,
    /// Whether memory is immune to decay (e.g. user-confirmed)
    pub is_immune: bool,
    /// Explanations justifying the decay calculation or decay resistance
    pub decay_reasons: Vec<String>,
}

// =========================================================================
// ContextMemoryDecayEvaluator
// =========================================================================

/// Evaluator that computes contextual memory importance decay over time.
pub struct ContextMemoryDecayEvaluator;

impl ContextMemoryDecayEvaluator {
    /// Evaluates memory importance decay using the default policy.
    pub fn evaluate(input: &MemoryDecayInput) -> MemoryDecayResult {
        Self::evaluate_with_policy(input, &MemoryDecayPolicy::default())
    }

    /// Evaluates memory importance decay using a custom configurable policy.
    pub fn evaluate_with_policy(
        input: &MemoryDecayInput,
        policy: &MemoryDecayPolicy,
    ) -> MemoryDecayResult {
        let mut reasons = Vec::new();

        // 1. User-Confirmed Memory Immunity Gate
        let is_confirmed = input.is_user_confirmed
            || input.provenance == InformationProvenance::Confirmed
            || input.memory_type == Some(ImportantMemoryType::ConfirmedContextFact)
            || input.user_feedback == Some("pinned");

        if is_confirmed {
            reasons.push("User-confirmed contextual memory is immune to automatic decay.".to_string());
            if input.user_feedback == Some("pinned") {
                reasons.push("Pinned context anchor retains maximum permanence.".to_string());
            }

            return MemoryDecayResult {
                entity_id: input.entity_id.to_string(),
                initial_importance: input.initial_importance,
                decayed_importance: input.initial_importance.max(0.70),
                initial_tier: input.initial_tier,
                decayed_tier: if input.initial_tier == MemoryImportanceTier::Stale
                    || input.initial_tier == MemoryImportanceTier::Temporary
                {
                    MemoryImportanceTier::Important
                } else {
                    input.initial_tier
                },
                decay_factor: 1.0,
                half_life_days_applied: f64::INFINITY,
                elapsed_days: 0.0,
                retention_floor: 1.0,
                is_stale: false,
                is_immune: true,
                decay_reasons: reasons,
            };
        }

        // 2. Parse Timestamps and Elapsed Duration
        let now = input.now.unwrap_or_else(Utc::now);
        let elapsed_seconds = if let Some(ts_str) = input.last_relevant_at {
            parse_timestamp_to_seconds_elapsed(ts_str, now)
        } else {
            0.0
        };

        let elapsed_hours = elapsed_seconds / 3600.0;
        let raw_elapsed_days = elapsed_seconds / 86400.0;

        // 3. Grace Period Check
        if elapsed_hours <= policy.grace_period_hours {
            reasons.push(format!(
                "Fresh memory within grace period ({:.1}h elapsed), no decay applied.",
                elapsed_hours
            ));

            return MemoryDecayResult {
                entity_id: input.entity_id.to_string(),
                initial_importance: input.initial_importance,
                decayed_importance: input.initial_importance,
                initial_tier: input.initial_tier,
                decayed_tier: input.initial_tier,
                decay_factor: 1.0,
                half_life_days_applied: policy.half_life_days_temporary,
                elapsed_days: raw_elapsed_days,
                retention_floor: policy.min_retention_floor_base,
                is_stale: false,
                is_immune: false,
                decay_reasons: reasons,
            };
        }

        // 4. Context Dormancy Dampening
        // If the workspace as a whole was dormant, don't penalize this item as harshly
        let dormancy_ratio = input.context_dormancy_ratio.clamp(0.0, 1.0);
        let dampening_factor = 1.0 - (policy.dormancy_decay_dampening * dormancy_ratio);
        let effective_elapsed_days = (raw_elapsed_days * dampening_factor).max(0.0);

        if dormancy_ratio >= 0.50 {
            reasons.push(format!(
                "Workspace context dormancy ({:.0}%) slowed effective decay.",
                dormancy_ratio * 100.0
            ));
        }

        // 5. Select Half-Life Based on Item Type and Semantics
        let (half_life, half_life_reason) = if input.is_active_task
            || input.memory_type == Some(ImportantMemoryType::ActiveTask)
        {
            (
                policy.half_life_days_active_task,
                "Active task orientation grants extended retention half-life.",
            )
        } else if input.workset_relevance == Some(WorksetItemRelevance::Reference)
            || input.memory_type == Some(ImportantMemoryType::ImportantReference)
            || input.memory_type == Some(ImportantMemoryType::RecurringReference)
        {
            (
                policy.half_life_days_reference,
                "Reference asset classification maintains long-term half-life.",
            )
        } else if input.initial_tier == MemoryImportanceTier::Critical
            || input.initial_tier == MemoryImportanceTier::Important
        {
            (
                policy.half_life_days_important,
                "Established important status provides standard important half-life.",
            )
        } else if input.initial_tier == MemoryImportanceTier::Useful || input.is_recent_task {
            (
                policy.half_life_days_useful,
                "Useful working context decays with moderate half-life.",
            )
        } else {
            (
                policy.half_life_days_temporary,
                "Inferred temporary capture decays rapidly without reinforcement.",
            )
        };
        reasons.push(half_life_reason.to_string());

        // 6. Compute Retention Floor (Mitigating Factors)
        let mut retention_floor = policy.min_retention_floor_base;

        // A. Reference Assets retain higher minimum floor
        if input.workset_relevance == Some(WorksetItemRelevance::Reference)
            || input.memory_type == Some(ImportantMemoryType::ImportantReference)
            || input.memory_type == Some(ImportantMemoryType::RecurringReference)
        {
            retention_floor = retention_floor.max(policy.min_retention_floor_reference);
            reasons.push(format!(
                "Reference asset retains baseline floor of {:.0}%.",
                policy.min_retention_floor_reference * 100.0
            ));
        }

        // B. Repeated Multi-Session Grounding increases floor
        if input.session_count >= 2 || input.distinct_task_count >= 2 {
            let multi_bonus = ((input.session_count as f64 * 0.05)
                + (input.distinct_task_count as f64 * 0.05))
                .min(0.20);
            let multi_floor = (policy.min_retention_floor_multisession + multi_bonus).min(0.55);
            retention_floor = retention_floor.max(multi_floor);
            reasons.push(format!(
                "Repeated relevance across {} sessions and {} tasks raised retention floor to {:.0}%.",
                input.session_count,
                input.distinct_task_count,
                retention_floor * 100.0
            ));
        }

        // C. Historical Usefulness Score bonus
        if input.historical_usefulness_score > 0.0 {
            let use_bonus = (input.historical_usefulness_score * 0.20).min(0.20);
            retention_floor = (retention_floor + use_bonus).min(0.60);
            reasons.push(format!(
                "Historical usefulness score ({:.2}) contributed to retention floor.",
                input.historical_usefulness_score
            ));
        }

        // 7. Calculate Exponential Decay Factor
        // decay = 0.5 ^ (effective_elapsed_days / half_life)
        let raw_decay = if half_life > 0.0 {
            0.5_f64.powf(effective_elapsed_days / half_life)
        } else {
            0.0
        };

        let decay_factor = raw_decay.max(retention_floor).clamp(0.0, 1.0);
        let decayed_importance = (input.initial_importance * decay_factor).clamp(0.0, 1.0);

        // 8. Determine Stale Status and Resulting Tier
        let is_stale = decayed_importance < policy.stale_threshold_score
            && raw_elapsed_days >= policy.stale_inactivity_days_unconfirmed
            && input.session_count <= 1
            && input.workset_relevance != Some(WorksetItemRelevance::Reference);

        let decayed_tier = if is_stale {
            reasons.push(format!(
                "Memory aged {:.1} days without multi-session reinforcement, transitioned to Stale.",
                raw_elapsed_days
            ));
            MemoryImportanceTier::Stale
        } else if decayed_importance >= 0.80
            && (input.is_active_task || input.session_count >= 3)
        {
            MemoryImportanceTier::Critical
        } else if decayed_importance >= 0.60 {
            MemoryImportanceTier::Important
        } else if decayed_importance >= 0.35 {
            MemoryImportanceTier::Useful
        } else {
            MemoryImportanceTier::Temporary
        };

        if decayed_tier != input.initial_tier && !is_stale {
            reasons.push(format!(
                "Tier transitioned from {} to {} based on decayed importance score ({:.2}).",
                input.initial_tier.display_name(),
                decayed_tier.display_name(),
                decayed_importance
            ));
        }

        MemoryDecayResult {
            entity_id: input.entity_id.to_string(),
            initial_importance: input.initial_importance,
            decayed_importance,
            initial_tier: input.initial_tier,
            decayed_tier,
            decay_factor,
            half_life_days_applied: half_life,
            elapsed_days: raw_elapsed_days,
            retention_floor,
            is_stale,
            is_immune: false,
            decay_reasons: reasons,
        }
    }

    /// Evaluates decay across a batch of memory items.
    pub fn batch_evaluate(
        inputs: &[MemoryDecayInput],
        policy: &MemoryDecayPolicy,
    ) -> Vec<MemoryDecayResult> {
        inputs
            .iter()
            .map(|input| Self::evaluate_with_policy(input, policy))
            .collect()
    }

    /// Evaluates decay for all memories in a given workspace from the database.
    ///
    /// GUARANTEE: Never deletes underlying Items or evidence rows from the database.
    pub fn evaluate_workspace_memories(
        conn: &Connection,
        workspace_id: &str,
        policy: Option<&MemoryDecayPolicy>,
    ) -> Result<Vec<MemoryDecayResult>> {
        let active_policy = policy.cloned().unwrap_or_default();
        let now = Utc::now();

        let mut stmt = conn.prepare(
            "SELECT i.id, i.type, i.created_at,
                    COALESCE(wm.user_confirmed, 0),
                    COALESCE(wm.pinned, 0),
                    COALESCE(wm.confidence, 0.5)
             FROM items i
             LEFT JOIN workspace_memberships wm ON i.id = wm.item_id AND wm.workspace_id = ?1
             WHERE wm.workspace_id = ?1 OR wm.workspace_id IS NULL
             ORDER BY i.created_at DESC",
        )?;

        let rows = stmt.query_map(params![workspace_id], |row| {
            let id: String = row.get(0)?;
            let item_type: String = row.get(1)?;
            let created_at: String = row.get(2)?;
            let user_confirmed: i64 = row.get(3)?;
            let pinned: i64 = row.get(4)?;
            let confidence: f64 = row.get(5)?;
            Ok((id, item_type, created_at, user_confirmed, pinned, confidence))
        })?;

        let mut results = Vec::new();

        for row in rows {
            let (id, item_type, created_at, user_confirmed, pinned, confidence) = row?;
            let is_confirmed = user_confirmed == 1 || pinned == 1;

            let provenance = if is_confirmed {
                InformationProvenance::Confirmed
            } else {
                InformationProvenance::Observed
            };

            let initial_tier = if is_confirmed {
                MemoryImportanceTier::Important
            } else if confidence >= 0.70 {
                MemoryImportanceTier::Useful
            } else {
                MemoryImportanceTier::Temporary
            };

            let input = MemoryDecayInput {
                entity_id: &id,
                initial_importance: confidence,
                initial_tier,
                provenance,
                is_user_confirmed: is_confirmed,
                user_feedback: if pinned == 1 { Some("pinned") } else { None },
                memory_type: if item_type == "task" {
                    Some(ImportantMemoryType::RecentTask)
                } else {
                    Some(ImportantMemoryType::ImportantItem)
                },
                last_relevant_at: Some(&created_at),
                first_seen_at: Some(&created_at),
                now: Some(now),
                is_active_task: false,
                is_recent_task: item_type == "task",
                workset_relevance: None,
                session_count: 1,
                distinct_task_count: 0,
                historical_usefulness_score: 0.0,
                context_dormancy_ratio: 0.0,
            };

            results.push(Self::evaluate_with_policy(&input, &active_policy));
        }

        Ok(results)
    }
}

// =========================================================================
// Helper Functions
// =========================================================================

fn parse_timestamp_to_seconds_elapsed(ts_str: &str, now: DateTime<Utc>) -> f64 {
    let dt = if let Ok(parsed) = DateTime::parse_from_rfc3339(ts_str) {
        parsed.with_timezone(&Utc)
    } else if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(ts_str, "%Y-%m-%d %H:%M:%S") {
        DateTime::from_naive_utc_and_offset(naive, Utc)
    } else {
        return 0.0;
    };

    now.signed_duration_since(dt).num_seconds().max(0) as f64
}

// =========================================================================
// Unit Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_fresh_memory_no_decay() {
        // Memory created 4 hours ago (within 24h grace period)
        let now = Utc::now();
        let created_at = (now - Duration::hours(4)).to_rfc3339();

        let input = MemoryDecayInput {
            entity_id: "fresh-doc-1",
            initial_importance: 0.75,
            initial_tier: MemoryImportanceTier::Important,
            provenance: InformationProvenance::Observed,
            is_user_confirmed: false,
            user_feedback: None,
            memory_type: Some(ImportantMemoryType::ImportantItem),
            last_relevant_at: Some(&created_at),
            first_seen_at: Some(&created_at),
            now: Some(now),
            is_active_task: false,
            is_recent_task: false,
            workset_relevance: Some(WorksetItemRelevance::Primary),
            session_count: 1,
            distinct_task_count: 1,
            historical_usefulness_score: 0.0,
            context_dormancy_ratio: 0.0,
        };

        let result = ContextMemoryDecayEvaluator::evaluate(&input);

        assert_eq!(result.decay_factor, 1.0, "Fresh memory within grace period must have 1.0 decay factor");
        assert_eq!(result.decayed_importance, 0.75, "Importance must not be reduced during grace period");
        assert_eq!(result.decayed_tier, MemoryImportanceTier::Important);
        assert!(!result.is_stale);
        assert!(!result.is_immune);
    }

    #[test]
    fn test_aging_memory_gradual_decay() {
        // Unreinforced useful memory aged 14 days (1 full useful half-life)
        let now = Utc::now();
        let created_at = (now - Duration::days(14)).to_rfc3339();

        let input = MemoryDecayInput {
            entity_id: "aging-item-1",
            initial_importance: 0.60,
            initial_tier: MemoryImportanceTier::Useful,
            provenance: InformationProvenance::Inferred,
            is_user_confirmed: false,
            user_feedback: None,
            memory_type: Some(ImportantMemoryType::ImportantItem),
            last_relevant_at: Some(&created_at),
            first_seen_at: Some(&created_at),
            now: Some(now),
            is_active_task: false,
            is_recent_task: false,
            workset_relevance: None,
            session_count: 1,
            distinct_task_count: 0,
            historical_usefulness_score: 0.0,
            context_dormancy_ratio: 0.0,
        };

        let result = ContextMemoryDecayEvaluator::evaluate(&input);

        // After 1 half-life (14 days), decay factor is approx 0.50
        assert!(
            (result.decay_factor - 0.50).abs() < 0.05,
            "After 1 half-life, decay factor should be ~0.50, got {}",
            result.decay_factor
        );
        assert!(
            (result.decayed_importance - 0.30).abs() < 0.05,
            "Decayed importance should be ~0.30, got {}",
            result.decayed_importance
        );
        assert_eq!(result.decayed_tier, MemoryImportanceTier::Temporary);
        assert!(!result.is_stale);
    }

    #[test]
    fn test_stale_memory_transitions_to_stale_tier() {
        // Unconfirmed, unreinforced item inactive for 35 days with single glance
        let now = Utc::now();
        let created_at = (now - Duration::days(35)).to_rfc3339();

        let input = MemoryDecayInput {
            entity_id: "stale-tab-1",
            initial_importance: 0.30,
            initial_tier: MemoryImportanceTier::Temporary,
            provenance: InformationProvenance::Inferred,
            is_user_confirmed: false,
            user_feedback: None,
            memory_type: None,
            last_relevant_at: Some(&created_at),
            first_seen_at: Some(&created_at),
            now: Some(now),
            is_active_task: false,
            is_recent_task: false,
            workset_relevance: None,
            session_count: 1,
            distinct_task_count: 0,
            historical_usefulness_score: 0.0,
            context_dormancy_ratio: 0.0,
        };

        let result = ContextMemoryDecayEvaluator::evaluate(&input);

        assert!(result.is_stale, "Inactive unreinforced item past 21 days must be marked stale");
        assert_eq!(result.decayed_tier, MemoryImportanceTier::Stale);
        assert!(result.decayed_importance < 0.20);
    }

    #[test]
    fn test_confirmed_memory_zero_decay() {
        // User confirmed item aged 90 days
        let now = Utc::now();
        let created_at = (now - Duration::days(90)).to_rfc3339();

        let input = MemoryDecayInput {
            entity_id: "confirmed-fact-1",
            initial_importance: 0.95,
            initial_tier: MemoryImportanceTier::Critical,
            provenance: InformationProvenance::Confirmed,
            is_user_confirmed: true,
            user_feedback: Some("pinned"),
            memory_type: Some(ImportantMemoryType::ConfirmedContextFact),
            last_relevant_at: Some(&created_at),
            first_seen_at: Some(&created_at),
            now: Some(now),
            is_active_task: false,
            is_recent_task: false,
            workset_relevance: Some(WorksetItemRelevance::Primary),
            session_count: 10,
            distinct_task_count: 4,
            historical_usefulness_score: 1.0,
            context_dormancy_ratio: 0.0,
        };

        let result = ContextMemoryDecayEvaluator::evaluate(&input);

        assert!(result.is_immune, "Confirmed memory must be flagged immune");
        assert_eq!(result.decay_factor, 1.0, "Confirmed memory must have 1.0 decay factor (zero decay)");
        assert_eq!(result.decayed_importance, 0.95, "Confirmed memory must retain original importance score");
        assert_eq!(result.decayed_tier, MemoryImportanceTier::Critical);
        assert!(!result.is_stale);
    }

    #[test]
    fn test_repeatedly_relevant_memory_retains_higher_floor() {
        // Repeatedly relevant memory across 6 sessions and 3 tasks, aged 45 days
        let now = Utc::now();
        let created_at = (now - Duration::days(45)).to_rfc3339();

        let input = MemoryDecayInput {
            entity_id: "multisession-anchor-1",
            initial_importance: 0.85,
            initial_tier: MemoryImportanceTier::Important,
            provenance: InformationProvenance::Observed,
            is_user_confirmed: false,
            user_feedback: None,
            memory_type: Some(ImportantMemoryType::RecurringReference),
            last_relevant_at: Some(&created_at),
            first_seen_at: Some(&created_at),
            now: Some(now),
            is_active_task: false,
            is_recent_task: false,
            workset_relevance: Some(WorksetItemRelevance::Reference),
            session_count: 6,
            distinct_task_count: 3,
            historical_usefulness_score: 0.80,
            context_dormancy_ratio: 0.0,
        };

        let result = ContextMemoryDecayEvaluator::evaluate(&input);

        assert!(
            result.retention_floor >= 0.45,
            "Multi-session and useful memory should have retention floor >= 0.45, got {}",
            result.retention_floor
        );
        assert!(
            result.decayed_importance >= 0.35,
            "Decayed importance should remain above useful threshold, got {}",
            result.decayed_importance
        );
        assert!(!result.is_stale, "Multi-session memory must not decay into Stale");
        assert!(matches!(result.decayed_tier, MemoryImportanceTier::Useful | MemoryImportanceTier::Important));
    }

    #[test]
    fn test_old_but_still_useful_reference() {
        // Project repository reference aged 60 days
        let now = Utc::now();
        let created_at = (now - Duration::days(60)).to_rfc3339();

        let input = MemoryDecayInput {
            entity_id: "repo-ref-1",
            initial_importance: 0.80,
            initial_tier: MemoryImportanceTier::Important,
            provenance: InformationProvenance::Observed,
            is_user_confirmed: false,
            user_feedback: None,
            memory_type: Some(ImportantMemoryType::ImportantReference),
            last_relevant_at: Some(&created_at),
            first_seen_at: Some(&created_at),
            now: Some(now),
            is_active_task: false,
            is_recent_task: false,
            workset_relevance: Some(WorksetItemRelevance::Reference),
            session_count: 2,
            distinct_task_count: 1,
            historical_usefulness_score: 0.60,
            context_dormancy_ratio: 0.0,
        };

        let result = ContextMemoryDecayEvaluator::evaluate(&input);

        assert!(
            result.retention_floor >= 0.40,
            "Reference asset must have floor >= 0.40, got {}",
            result.retention_floor
        );
        assert!(
            result.decayed_importance >= 0.35,
            "Reference asset should maintain Useful/Important score, got {}",
            result.decayed_importance
        );
        assert!(!result.is_stale, "Old reference asset must NOT become Stale");
        assert!(matches!(result.decayed_tier, MemoryImportanceTier::Useful | MemoryImportanceTier::Important));
    }
}

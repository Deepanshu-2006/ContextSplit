use std::collections::HashMap;
use serde::{Deserialize, Serialize};

use crate::context_engine::decision_trace::{
    ContextDecisionTrace, ContextDecisionTracer, DecisionType, EvidenceConsideredTrace,
    EvidenceStrengthRating,
};

// =========================================================================
// Epistemic Disclaimer
// =========================================================================

pub const CONTEXT_DECISION_EXPLANATION_DISCLAIMER: &str =
    "ContextSplit decision explanations translate internal execution traces and evidence sets into developer diagnostics. All reasons are strictly derived from observed telemetry signals and deterministic heuristics, strictly avoiding ungrounded inferences.";

// =========================================================================
// Structured Decision Explanation Model
// =========================================================================

/// Structured, developer-readable diagnostic explanation of an intelligence decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextDecisionExplanation {
    /// ID of the source decision trace
    pub decision_id: String,
    /// High-level diagnostic headline (e.g., "WHY WAS THIS ITEM ASSIGNED TO PAYX?")
    pub headline: String,
    /// Primary dominant evidence that directly prompted the decision
    pub primary_evidence: Vec<String>,
    /// Secondary or corroborating evidence supporting the decision
    pub supporting_evidence: Vec<String>,
    /// Evidence that contradicted or penalized candidate contexts
    pub contradictory_evidence: Vec<String>,
    /// Competing alternative candidates that were evaluated and rejected
    pub rejected_alternatives: Vec<String>,
    /// Any ambiguity or uncertainty indicators
    pub uncertainty_notes: Vec<String>,
    /// Summary of the final verdict
    pub final_decision: String,
    /// Full narrative summary paragraph
    pub summary_paragraph: String,
    /// Epistemic disclaimer
    pub disclaimer: &'static str,
}

impl ContextDecisionExplanation {
    /// Formats the explanation into a clean, developer-readable plain text report.
    pub fn format_report(&self) -> String {
        let mut out = String::new();

        out.push_str(&self.headline);
        out.push_str("\n\n");

        if !self.primary_evidence.is_empty() {
            out.push_str("Primary evidence:\n");
            for ev in &self.primary_evidence {
                out.push_str(&format!("- {}\n", ev));
            }
            out.push('\n');
        }

        if !self.supporting_evidence.is_empty() {
            out.push_str("Supporting evidence:\n");
            for ev in &self.supporting_evidence {
                out.push_str(&format!("- {}\n", ev));
            }
            out.push('\n');
        }

        if !self.contradictory_evidence.is_empty() {
            out.push_str("Contradictory evidence:\n");
            for ev in &self.contradictory_evidence {
                out.push_str(&format!("- {}\n", ev));
            }
            out.push('\n');
        }

        if !self.rejected_alternatives.is_empty() {
            out.push_str("Rejected alternatives:\n");
            for alt in &self.rejected_alternatives {
                out.push_str(&format!("- {}\n", alt));
            }
            out.push('\n');
        }

        if !self.uncertainty_notes.is_empty() {
            out.push_str("Uncertainty notes:\n");
            for note in &self.uncertainty_notes {
                out.push_str(&format!("- {}\n", note));
            }
            out.push('\n');
        }

        out.push_str("Final decision:\n");
        out.push_str(&self.final_decision);
        out.push('\n');

        out
    }
}

// =========================================================================
// Explanation Service
// =========================================================================

/// Service for converting internal `ContextDecisionTrace` records into developer explanations.
pub struct ContextDecisionExplanationService;

impl ContextDecisionExplanationService {
    /// Explains a specific `ContextDecisionTrace` by generating a `ContextDecisionExplanation`.
    pub fn explain(trace: &ContextDecisionTrace) -> ContextDecisionExplanation {
        let target_name = trace
            .final_decision
            .primary_context_name
            .as_deref()
            .or(trace.final_decision.primary_context_id.as_deref())
            .unwrap_or("Unknown Target");

        let headline = match trace.decision_type {
            DecisionType::ContextAssignment => {
                format!("WHY WAS THIS ITEM ASSIGNED TO {}?", target_name.to_uppercase())
            }
            DecisionType::ContextRejection => {
                format!("WHY WAS THIS ITEM REJECTED FROM {}?", target_name.to_uppercase())
            }
            DecisionType::ContextTransition => {
                format!("WHY DID FOCUS SWITCH TO {}?", target_name.to_uppercase())
            }
            DecisionType::TaskAssignment => {
                format!("WHY WAS THIS TASK ASSIGNED TO {}?", target_name.to_uppercase())
            }
            DecisionType::MemoryCreation => {
                format!("WHY WAS CONTEXT MEMORY CREATED FOR {}?", target_name.to_uppercase())
            }
            DecisionType::MemoryImportance => {
                format!("WHY WAS MEMORY IMPORTANCE EVALUATED FOR {}?", target_name.to_uppercase())
            }
            DecisionType::SnapshotCreation => {
                format!("WHY WAS CONTEXT SNAPSHOT CREATED FOR {}?", target_name.to_uppercase())
            }
            DecisionType::LifecycleTransition => {
                format!("WHY DID LIFECYCLE CHANGE FOR {}?", target_name.to_uppercase())
            }
            DecisionType::ContextReconstruction => {
                format!("WHY WAS CONTEXT RECONSTRUCTED FOR {}?", target_name.to_uppercase())
            }
            DecisionType::ResumeReadiness => {
                format!("WHY WAS RESUME READINESS ASSESSED FOR {}?", target_name.to_uppercase())
            }
            DecisionType::FeedbackAdaptation => {
                format!("WHY WAS USER FEEDBACK ADAPTED FOR {}?", target_name.to_uppercase())
            }
        };

        let mut primary_evidence = Vec::new();
        let mut supporting_evidence = Vec::new();
        let mut contradictory_evidence = Vec::new();

        for ev in &trace.evidence_considered {
            if ev.is_contradictory {
                contradictory_evidence.push(format_evidence_bullet(ev));
                continue;
            }

            let is_primary = match ev.strength_rating {
                Some(EvidenceStrengthRating::Definite) | Some(EvidenceStrengthRating::Strong) => true,
                Some(EvidenceStrengthRating::Medium) | Some(EvidenceStrengthRating::Weak) => false,
                None => ev.strength >= 0.70,
            };

            let bullet = format_evidence_bullet(ev);
            if is_primary {
                primary_evidence.push(bullet);
            } else {
                supporting_evidence.push(bullet);
            }
        }

        // Add feedback entries to supporting/primary evidence
        for fb in &trace.feedback_considered {
            if fb.is_applied {
                let bullet = if !fb.rationale.is_empty() {
                    fb.rationale.clone()
                } else {
                    format!("User feedback: {}", fb.action)
                };
                if fb.precedence_tier <= 2 {
                    primary_evidence.push(bullet);
                } else {
                    supporting_evidence.push(bullet);
                }
            }
        }

        // If primary evidence is empty but reason exists, use reason
        if primary_evidence.is_empty() && !trace.reason.is_empty() {
            primary_evidence.push(trace.reason.clone());
        }

        // Collect rejected alternatives from candidate contexts
        let mut rejected_alternatives = Vec::new();
        for candidate in &trace.candidate_contexts {
            if !candidate.is_winner {
                rejected_alternatives.push(candidate.context_name.clone());
            }
        }

        // Collect uncertainty notes
        let mut uncertainty_notes = Vec::new();
        if trace.evidence_strength.diversity_score < 0.70 {
            uncertainty_notes.push(format!(
                "Low evidence diversity score ({:.2}) from only {} independent group(s)",
                trace.evidence_strength.diversity_score,
                trace.evidence_strength.independent_groups_count
            ));
        }
        if trace.evidence_strength.contradiction_penalty > 0.0 {
            uncertainty_notes.push(format!(
                "Contradiction penalty applied (-{:.2})",
                trace.evidence_strength.contradiction_penalty
            ));
        }

        let final_decision = if let Some(ref name) = trace.final_decision.primary_context_name {
            name.clone()
        } else if !trace.final_decision.status.is_empty() && trace.final_decision.status != "unresolved" {
            trace.final_decision.status.clone()
        } else {
            target_name.to_string()
        };

        let summary_paragraph = format!(
            "Decision '{}' reached for entity [{}] with {} primary signal(s) and {} supporting signal(s) in {:.2}ms.",
            trace.decision_type.as_str(),
            trace.source_entity.entity_id(),
            primary_evidence.len(),
            supporting_evidence.len(),
            trace.processing_duration_ms
        );

        ContextDecisionExplanation {
            decision_id: trace.decision_id.clone(),
            headline,
            primary_evidence,
            supporting_evidence,
            contradictory_evidence,
            rejected_alternatives,
            uncertainty_notes,
            final_decision,
            summary_paragraph,
            disclaimer: CONTEXT_DECISION_EXPLANATION_DISCLAIMER,
        }
    }

    /// Looks up a decision trace by ID in the global tracer and generates an explanation.
    pub fn explain_by_id(decision_id: &str) -> Option<ContextDecisionExplanation> {
        let trace = ContextDecisionTracer::get_by_id(decision_id)?;
        Some(Self::explain(&trace))
    }

    /// Explains recent decision traces.
    pub fn explain_recent(limit: usize) -> Vec<ContextDecisionExplanation> {
        ContextDecisionTracer::get_recent(limit)
            .into_iter()
            .map(|t| Self::explain(&t))
            .collect()
    }

    /// Explains a lifecycle transition trace specifically using the developer diagnostics format.
    pub fn explain_lifecycle_transition(
        trace: &ContextDecisionTrace,
    ) -> crate::context_engine::lifecycle_explanation::ContextLifecycleTransitionExplanation {
        crate::context_engine::lifecycle_explanation::ContextLifecycleExplanationService::explain_decision_trace(trace)
    }
}

// =========================================================================
// Evidence Formatting Helpers
// =========================================================================

fn format_evidence_bullet(ev: &EvidenceConsideredTrace) -> String {
    let name_lower = ev.signal_name.to_lowercase();

    if ev.signal_value.is_empty() {
        return ev.signal_name.clone();
    }

    if name_lower == "repository" {
        format!("{} repository matched", ev.signal_value)
    } else if name_lower == "project path" || name_lower == "path" {
        format!("Project path matched {}", ev.signal_value)
    } else if name_lower == "recent context activity" || name_lower == "activity" {
        format!("Recent activity was {}-related", ev.signal_value)
    } else if name_lower == "application" {
        format!("Application was {}", ev.signal_value)
    } else if name_lower == "window title" {
        format!("Window title contained '{}'", ev.signal_value)
    } else if name_lower == "browser domain" {
        format!("Browser domain was {}", ev.signal_value)
    } else if name_lower == "user feedback" {
        ev.signal_value.clone()
    } else if name_lower == "semantic similarity" {
        if ev.signal_value == "medium" || ev.signal_value == "strong" {
            format!("Semantic similarity was {}", ev.signal_value)
        } else {
            format!("Semantic similarity: {}", ev.signal_value)
        }
    } else if !ev.summary.is_empty() {
        ev.summary.clone()
    } else {
        format!("{}: {}", ev.signal_name, ev.signal_value)
    }
}

// =========================================================================
// Unit Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_engine::decision_trace::{
        CandidateContextTrace, ContextDecisionTraceBuilder,
    };

    #[test]
    fn test_decision_explanation_generation_matching_example() {
        let trace = ContextDecisionTraceBuilder::new_context_assignment(
            "item-payx",
            Some("VS Code"),
            Some("CheckoutController.ts"),
        )
        .with_outcome("accepted", Some("ctx-payx"), Some("PayX"), Some("confirmed"), "Assigned item to PayX")
        .with_candidate(CandidateContextTrace {
            context_id: "ctx-payx".to_string(),
            context_name: "PayX".to_string(),
            raw_score: 0.95,
            calibrated_confidence: 0.95,
            confidence_band: "very_high".to_string(),
            rank: 1,
            is_winner: true,
            is_dormant_penalized: false,
            margin_to_next: Some(0.40),
        })
        .with_candidate(CandidateContextTrace {
            context_id: "ctx-cs".to_string(),
            context_name: "ContextSplit".to_string(),
            raw_score: 0.55,
            calibrated_confidence: 0.50,
            confidence_band: "moderate".to_string(),
            rank: 2,
            is_winner: false,
            is_dormant_penalized: false,
            margin_to_next: Some(0.20),
        })
        .with_candidate(CandidateContextTrace {
            context_id: "ctx-uos".to_string(),
            context_name: "University OS".to_string(),
            raw_score: 0.35,
            calibrated_confidence: 0.30,
            confidence_band: "low".to_string(),
            rank: 3,
            is_winner: false,
            is_dormant_penalized: false,
            margin_to_next: None,
        })
        .with_evidence(
            EvidenceConsideredTrace::new("Repository", "PayX")
                .with_strength_rating(EvidenceStrengthRating::Strong),
        )
        .with_evidence(
            EvidenceConsideredTrace::new("Project path", "PayX")
                .with_strength_rating(EvidenceStrengthRating::Strong),
        )
        .with_evidence(
            EvidenceConsideredTrace::new("Recent context activity", "PayX")
                .with_strength_rating(EvidenceStrengthRating::Strong),
        )
        .with_evidence(
            EvidenceConsideredTrace::new("Related files were active", "")
                .with_strength_rating(EvidenceStrengthRating::Medium),
        )
        .with_evidence(
            EvidenceConsideredTrace::new("User feedback", "User previously confirmed this repository")
                .with_strength_rating(EvidenceStrengthRating::Medium),
        )
        .build();

        let explanation = ContextDecisionExplanationService::explain(&trace);

        assert_eq!(explanation.headline, "WHY WAS THIS ITEM ASSIGNED TO PAYX?");
        assert_eq!(explanation.final_decision, "PayX");

        // Primary evidence check
        assert!(explanation.primary_evidence.iter().any(|s| s.contains("PayX repository matched")));
        assert!(explanation.primary_evidence.iter().any(|s| s.contains("Project path matched PayX")));
        assert!(explanation.primary_evidence.iter().any(|s| s.contains("Recent activity was PayX-related")));

        // Supporting evidence check
        assert!(explanation.supporting_evidence.iter().any(|s| s.contains("Related files were active")));
        assert!(explanation.supporting_evidence.iter().any(|s| s.contains("User previously confirmed this repository")));

        // Rejected alternatives check
        assert_eq!(explanation.rejected_alternatives, vec!["ContextSplit", "University OS"]);

        // Verify formatted report
        let report = explanation.format_report();
        let expected = "\
WHY WAS THIS ITEM ASSIGNED TO PAYX?

Primary evidence:
- PayX repository matched
- Project path matched PayX
- Recent activity was PayX-related

Supporting evidence:
- Related files were active
- User previously confirmed this repository

Rejected alternatives:
- ContextSplit
- University OS

Final decision:
PayX
";
        assert_eq!(report.trim(), expected.trim());
    }

    #[test]
    fn test_decision_explanation_rejection() {
        let trace = ContextDecisionTraceBuilder::new_context_rejection("item-noise", Some("Finder"), Some("Trash"))
            .with_outcome("rejected", Some("ctx-cs"), Some("ContextSplit"), None, "Quarantined system noise")
            .with_reason("Ephemeral glance and utility noise filter triggered")
            .build();

        let explanation = ContextDecisionExplanationService::explain(&trace);
        assert_eq!(explanation.headline, "WHY WAS THIS ITEM REJECTED FROM CONTEXTSPLIT?");
        assert!(explanation.primary_evidence.iter().any(|s| s.contains("Ephemeral glance")));
    }

    #[test]
    fn test_decision_explanation_transition() {
        let trace = ContextDecisionTraceBuilder::new_context_transition("trans-1", "ContextSwitch")
            .with_outcome("transitioned", Some("ctx-cs"), Some("ContextSplit"), None, "Switched focus to ContextSplit")
            .with_reason("Observed 3 consecutive telemetry events inside project directory")
            .build();

        let explanation = ContextDecisionExplanationService::explain(&trace);
        assert_eq!(explanation.headline, "WHY DID FOCUS SWITCH TO CONTEXTSPLIT?");
        assert!(explanation.primary_evidence.iter().any(|s| s.contains("Observed 3 consecutive telemetry events")));
    }

    #[test]
    fn test_lifecycle_transition_explanation_via_decision_service() {
        let trace = ContextDecisionTraceBuilder::new_lifecycle_transition("ctx-cs", "ContextSplit")
            .with_lifecycle_state("active")
            .with_outcome("resumed", Some("ctx-cs"), Some("ContextSplit"), Some("active"), "ContextSplit resumed")
            .with_reason("ContextSplit repository opened and project files active")
            .build();

        let expl = ContextDecisionExplanationService::explain_lifecycle_transition(&trace);
        assert_eq!(expl.context_name, "ContextSplit");
        assert_eq!(expl.state_transition_display(), "PAUSED → ACTIVE");
        assert!(expl.is_valid_evidence_based());
        assert!(expl.format_diagnostic().contains("ContextSplit"));
        assert!(expl.format_diagnostic().contains("PAUSED → ACTIVE"));
    }
}

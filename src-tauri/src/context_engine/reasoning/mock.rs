use super::models::{
    AIReasoningError, AlternativeContextReasoning, ContextReasoningResult, EvidencePackage,
    ReasoningProviderInfo,
};
use super::traits::AIReasoner;

/// Deterministic mock AI reasoning provider.
/// Simulates an LLM's interpretation over structured evidence packages without
/// invoking external APIs or neural weights.
///
/// CRITICAL ARCHITECTURAL RULES SATISFIED:
/// 1. Operates purely on `&EvidencePackage` (immutable).
/// 2. Has NO access to database connections or file I/O.
/// 3. Does NOT invent evidence; all facts and context IDs are derived from the package.
/// 4. Cannot mutate state, move items, create workspaces, or alter memberships.
#[derive(Debug, Default, Clone)]
pub struct MockAIReasoner {
    pub model_name: String,
}

impl MockAIReasoner {
    pub fn new() -> Self {
        Self {
            model_name: "mock-reasoner-v1.0".to_string(),
        }
    }

    pub fn with_model(model_name: &str) -> Self {
        Self {
            model_name: model_name.to_string(),
        }
    }
}

impl AIReasoner for MockAIReasoner {
    fn provider_name(&self) -> &str {
        "Mock AI Reasoner (Deterministic Simulation)"
    }

    fn provider_id(&self) -> &str {
        "mock"
    }

    fn analyze_context(
        &self,
        package: &EvidencePackage,
    ) -> Result<ContextReasoningResult, AIReasoningError> {
        let now = chrono::Utc::now().to_rfc3339();
        let provider_info = ReasoningProviderInfo {
            provider_id: self.provider_id().to_string(),
            provider_type: "mock_simulation".to_string(),
            model_version: self.model_name.clone(),
            timestamp: now,
        };

        // 1. Check if candidate contexts exist
        if package.candidate_contexts.is_empty() {
            return Ok(ContextReasoningResult {
                recommended_context_id: None,
                recommended_context_name: None,
                confidence: 0.0,
                confidence_band: "Low".to_string(),
                reasoning_summary: "No candidate workspaces or context candidates match this item. Item remains in Inbox awaiting further activity signals or explicit user placement.".to_string(),
                key_factors: vec![
                    "Zero candidate contexts provided in the evidence package.".to_string(),
                    "No shared session, domain, or semantic associations detected.".to_string(),
                ],
                alternative_interpretations: Vec::new(),
                is_ambiguous: false,
                suggested_action: "keep_inbox".to_string(),
                suggested_label: None,
                provider_info,
            });
        }

        // 2. Separate active/dormant candidates from explicitly archived ones
        let mut active_candidates: Vec<_> = package
            .candidate_contexts
            .iter()
            .filter(|c| c.lifecycle_state != "archived")
            .cloned()
            .collect();

        // Sort descending by deterministic score
        active_candidates.sort_by(|a, b| {
            b.deterministic_score
                .partial_cmp(&a.deterministic_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // If all candidates are archived
        if active_candidates.is_empty() {
            let archived_names: Vec<String> = package
                .candidate_contexts
                .iter()
                .map(|c| format!("'{}' (archived)", c.name))
                .collect();
            return Ok(ContextReasoningResult {
                recommended_context_id: None,
                recommended_context_name: None,
                confidence: 0.0,
                confidence_band: "Low".to_string(),
                reasoning_summary: format!(
                    "Item matches candidate contexts ({}), but all matching contexts are archived by user preference. Active suggestions are suppressed.",
                    archived_names.join(", ")
                ),
                key_factors: vec![
                    "User lifecycle settings have archived matching contexts.".to_string(),
                    "Suppression applied to avoid surfacing inactive workspaces.".to_string(),
                ],
                alternative_interpretations: Vec::new(),
                is_ambiguous: false,
                suggested_action: "keep_inbox".to_string(),
                suggested_label: None,
                provider_info,
            });
        }

        let top = &active_candidates[0];

        // 3. Case: Weak contextual evidence across all candidates (< 0.25)
        if top.deterministic_score < 0.25 {
            let mut factors = top.evidence_factors.clone();
            if factors.is_empty() {
                factors.push(format!("Top candidate '{}' scored only {:.0}%.", top.name, top.deterministic_score * 100.0));
            }

            return Ok(ContextReasoningResult {
                recommended_context_id: None,
                recommended_context_name: None,
                confidence: top.deterministic_score,
                confidence_band: "Low".to_string(),
                reasoning_summary: format!(
                    "Weak contextual evidence observed. While '{}' showed marginal affinity ({:.0}%), the signals are insufficient to warrant a confident context suggestion.",
                    top.name,
                    top.deterministic_score * 100.0
                ),
                key_factors: factors,
                alternative_interpretations: Vec::new(),
                is_ambiguous: false,
                suggested_action: "keep_inbox".to_string(),
                suggested_label: None,
                provider_info,
            });
        }

        // 4. Case: Ambiguous conflict between top candidates (difference <= 0.12 and second >= 0.35)
        let is_ambiguous = active_candidates.len() > 1
            && (top.deterministic_score - active_candidates[1].deterministic_score).abs() <= 0.12
            && active_candidates[1].deterministic_score >= 0.35;

        if is_ambiguous {
            let second = &active_candidates[1];
            let avg_conf = (top.deterministic_score + second.deterministic_score) / 2.0;

            let mut key_factors = Vec::new();
            key_factors.push(format!(
                "Competing affinity between '{}' ({:.0}%) and '{}' ({:.0}%).",
                top.name,
                top.deterministic_score * 100.0,
                second.name,
                second.deterministic_score * 100.0
            ));
            for f in top.evidence_factors.iter().take(2) {
                key_factors.push(format!("{}: {}", top.name, f));
            }
            for f in second.evidence_factors.iter().take(2) {
                key_factors.push(format!("{}: {}", second.name, f));
            }

            let alternatives = active_candidates
                .iter()
                .skip(1)
                .map(|c| AlternativeContextReasoning {
                    context_id: c.context_id.clone(),
                    context_name: c.name.clone(),
                    confidence: c.deterministic_score,
                    rationale: format!(
                        "Close competitor with {:.0}% confidence. {}",
                        c.deterministic_score * 100.0,
                        c.evidence_factors.first().cloned().unwrap_or_default()
                    ),
                })
                .collect();

            return Ok(ContextReasoningResult {
                recommended_context_id: Some(top.context_id.clone()),
                recommended_context_name: Some(top.name.clone()),
                confidence: avg_conf,
                confidence_band: "Medium".to_string(),
                reasoning_summary: format!(
                    "Contextual ambiguity detected: Item exhibits balanced signals between '{}' ({:.0}%) and '{}' ({:.0}%). User review recommended before confirming membership.",
                    top.name,
                    top.deterministic_score * 100.0,
                    second.name,
                    second.deterministic_score * 100.0
                ),
                key_factors,
                alternative_interpretations: alternatives,
                is_ambiguous: true,
                suggested_action: "review_conflict".to_string(),
                suggested_label: None,
                provider_info,
            });
        }

        // 5. Case: Clear, single leading context recommendation
        let mut key_factors = top.evidence_factors.clone();
        if key_factors.is_empty() {
            key_factors.push(format!("Deterministic context score is {:.0}%.", top.deterministic_score * 100.0));
        }

        // Enrich with temporal context if recent
        if package.temporal_context.is_recent {
            key_factors.push(format!(
                "Activity captured recently ({:.1} days ago), retaining high temporal relevance.",
                package.temporal_context.age_days
            ));
        }

        // Enrich with session facts if applicable
        if let Some(sess) = &package.session_context {
            if sess.associated_workspaces.contains(&top.context_id) {
                key_factors.push(format!(
                    "Item belongs to an active session shared with {} other items in '{}'.",
                    sess.item_count, top.name
                ));
            }
        }

        let alternatives: Vec<AlternativeContextReasoning> = active_candidates
            .iter()
            .skip(1)
            .map(|c| AlternativeContextReasoning {
                context_id: c.context_id.clone(),
                context_name: c.name.clone(),
                confidence: c.deterministic_score,
                rationale: format!(
                    "Lower affinity match ({:.0}%). {}",
                    c.deterministic_score * 100.0,
                    c.evidence_factors.first().cloned().unwrap_or_default()
                ),
            })
            .collect();

        let suggested_action = if top.context_type == "workspace" {
            "suggest_workspace".to_string()
        } else {
            "suggest_new_context".to_string()
        };

        let summary = format!(
            "Item strongly aligns with '{}' ({:.0}% confidence, {}). Inferred from grounded evidence including {}.",
            top.name,
            top.deterministic_score * 100.0,
            top.confidence_band,
            if !top.evidence_factors.is_empty() {
                top.evidence_factors[0].to_lowercase()
            } else {
                "cross-signal contextual coherence".to_string()
            }
        );

        Ok(ContextReasoningResult {
            recommended_context_id: Some(top.context_id.clone()),
            recommended_context_name: Some(top.name.clone()),
            confidence: top.deterministic_score,
            confidence_band: top.confidence_band.clone(),
            reasoning_summary: summary,
            key_factors,
            alternative_interpretations: alternatives,
            is_ambiguous: false,
            suggested_action,
            suggested_label: if top.context_type == "candidate" {
                Some(top.name.clone())
            } else {
                None
            },
            provider_info,
        })
    }
}

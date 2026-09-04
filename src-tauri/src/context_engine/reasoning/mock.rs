use super::models::{
    AIReasoningError, AlternativeContextReasoning, ContextReasoningDecision,
    ContextReasoningResult, EvidencePackage, ReasoningProviderInfo,
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
/// 5. Strictly validates output against the ContextReasoningResult schema before returning.
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
        let provider_info = Some(ReasoningProviderInfo {
            provider_id: self.provider_id().to_string(),
            provider_type: "mock_simulation".to_string(),
            model_version: self.model_name.clone(),
            timestamp: now,
        });

        // 1. Check if candidate contexts exist
        if package.candidate_contexts.is_empty() {
            let result = ContextReasoningResult {
                decision: ContextReasoningDecision::NoContext,
                suggested_context_reference: None,
                suggested_name: None,
                confidence: 0.0,
                explanation: "There is insufficient evidence to associate this item with a context; no matching workspaces or activity patterns were detected. The item remains in Inbox.".to_string(),
                evidence_references: vec![
                    "Zero candidate contexts provided in the evidence package.".to_string(),
                    "No shared session, domain, or semantic associations detected.".to_string(),
                ],
                uncertainty_reason: Some("No candidate contexts or matching signals available in evidence package.".to_string()),
                alternative_interpretations: Vec::new(),
                provider_info,
            };
            result.validate().map_err(|e| AIReasoningError::InvalidResponse(e.to_string()))?;
            super::validator::validate_explanation_grounding(&result, package)
                .map_err(|e| AIReasoningError::InvalidResponse(e.to_string()))?;
            return Ok(result);
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
                .map(|c| c.name.clone())
                .collect();
            let result = ContextReasoningResult {
                decision: ContextReasoningDecision::NoContext,
                suggested_context_reference: None,
                suggested_name: None,
                confidence: 0.0,
                explanation: format!(
                    "This item may have potential overlap with {}, but the available evidence is limited because matching contexts are archived.",
                    archived_names.join(", ")
                ),
                evidence_references: vec![
                    "Matching contexts are explicitly archived by user settings.".to_string(),
                    "Context suggestions are suppressed for archived workspaces.".to_string(),
                ],
                uncertainty_reason: Some("All matching contexts are explicitly archived by user.".to_string()),
                alternative_interpretations: Vec::new(),
                provider_info,
            };
            result.validate().map_err(|e| AIReasoningError::InvalidResponse(e.to_string()))?;
            super::validator::validate_explanation_grounding(&result, package)
                .map_err(|e| AIReasoningError::InvalidResponse(e.to_string()))?;
            return Ok(result);
        }

        let top = &active_candidates[0];

        // 3. Case: Weak contextual evidence across all candidates (< 0.25)
        if top.deterministic_score < 0.25 {
            let mut factors = top.evidence_factors.clone();
            if factors.is_empty() {
                factors.push(format!("Top candidate '{}' shows minimal contextual affinity.", top.name));
            }

            let result = ContextReasoningResult {
                decision: ContextReasoningDecision::NoContext,
                suggested_context_reference: None,
                suggested_name: None,
                confidence: top.deterministic_score,
                explanation: format!(
                    "This may be related to {}, but the available evidence is limited.",
                    top.name
                ),
                evidence_references: factors,
                uncertainty_reason: Some(format!(
                    "Available signals for '{}' are below the actionable threshold of 25%.",
                    top.name
                )),
                alternative_interpretations: Vec::new(),
                provider_info,
            };
            result.validate().map_err(|e| AIReasoningError::InvalidResponse(e.to_string()))?;
            super::validator::validate_explanation_grounding(&result, package)
                .map_err(|e| AIReasoningError::InvalidResponse(e.to_string()))?;
            return Ok(result);
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
                "Competing affinity between '{}' and '{}'.",
                top.name, second.name
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
                        "Close competitor with high affinity. {}",
                        c.evidence_factors.first().cloned().unwrap_or_default()
                    ),
                })
                .collect();

            let result = ContextReasoningResult {
                decision: ContextReasoningDecision::Uncertain,
                suggested_context_reference: Some(top.context_id.clone()),
                suggested_name: Some(top.name.clone()),
                confidence: avg_conf,
                explanation: format!(
                    "This may be related to either '{}' or '{}', but the available evidence shows competing signals between them.",
                    top.name,
                    second.name
                ),
                evidence_references: key_factors,
                uncertainty_reason: Some(format!(
                    "Competing signals between '{}' and '{}' with score difference under 12%.",
                    top.name, second.name
                )),
                alternative_interpretations: alternatives,
                provider_info,
            };
            result.validate().map_err(|e| AIReasoningError::InvalidResponse(e.to_string()))?;
            super::validator::validate_explanation_grounding(&result, package)
                .map_err(|e| AIReasoningError::InvalidResponse(e.to_string()))?;
            return Ok(result);
        }

        // 5. Case: Clear, single leading context recommendation
        let mut key_factors = top.evidence_factors.clone();
        if key_factors.is_empty() {
            key_factors.push("Strong contextual affinity inferred from grounded evidence factors.".to_string());
        }

        if package.temporal_context.is_recent {
            key_factors.push(format!(
                "Activity captured recently ({:.1} days ago), retaining high temporal relevance.",
                package.temporal_context.age_days
            ));
        }

        if let Some(sess) = &package.session_context {
            if sess.associated_workspaces.contains(&top.name) || sess.associated_workspaces.contains(&top.context_id) {
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
                    "Alternative candidate with lower affinity. {}",
                    c.evidence_factors.first().cloned().unwrap_or_default()
                ),
            })
            .collect();

        let decision = if top.context_type == "workspace" {
            ContextReasoningDecision::ExistingContext
        } else {
            ContextReasoningDecision::NewContext
        };

        // Synthesize grounded human-readable explanation
        let is_travel_keyword = |text: &str| {
            let lower = text.to_lowercase();
            lower.contains("flight")
                || lower.contains("hotel")
                || lower.contains("paris")
                || lower.contains("trip")
                || lower.contains("travel")
                || lower.contains("tour")
                || lower.contains("booking")
                || lower.contains("cdg")
                || lower.contains("airbnb")
                || lower.contains("eurostar")
                || lower.contains("reservation")
        };

        let has_travel = package.target_item.title.as_deref().map_or(false, is_travel_keyword)
            || is_travel_keyword(&package.target_item.content_snippet)
            || is_travel_keyword(&top.name);

        let has_similar_items = !package.relevant_related_items.is_empty()
            || !package.semantic_similarity_summaries.is_empty()
            || top.evidence_factors.iter().any(|f| f.to_lowercase().contains("similar"));

        let has_shared_domain = package.target_item.domain.is_some()
            || top.evidence_factors.iter().any(|f| f.to_lowercase().contains("domain"));

        let has_shared_session = package.session_context.is_some()
            || !package.relevant_sessions.is_empty()
            || top.evidence_factors.iter().any(|f| f.to_lowercase().contains("session"));

        let explanation = if top.context_type == "workspace" {
            if has_similar_items && has_shared_domain {
                if has_travel {
                    format!(
                        "This appears related to {} because it is similar to several recent travel-research items and shares domains already associated with that context.",
                        top.name
                    )
                } else {
                    format!(
                        "This appears related to {} because it is similar to several recent items and shares domains already associated with that context.",
                        top.name
                    )
                }
            } else if has_similar_items && has_shared_session {
                format!(
                    "This appears related to {} because it is similar to recent items and was captured during the same activity session.",
                    top.name
                )
            } else if has_similar_items {
                format!(
                    "This appears related to {} because it is similar to several recent items associated with that context.",
                    top.name
                )
            } else if has_shared_domain {
                format!(
                    "This appears related to {} because it shares domains already associated with that context.",
                    top.name
                )
            } else if has_shared_session {
                format!(
                    "This appears related to {} because it was captured during an active session associated with that context.",
                    top.name
                )
            } else {
                format!(
                    "This appears related to {} based on consistent activity and domain patterns associated with that context.",
                    top.name
                )
            }
        } else {
            // New Context
            if has_similar_items && has_shared_session {
                format!(
                    "This appears to form a new context for {} because it represents a distinct cluster of related activity items captured in the same session.",
                    top.name
                )
            } else if has_similar_items {
                format!(
                    "This appears to form a new context for {} because it forms a cohesive cluster of related items distinct from existing workspaces.",
                    top.name
                )
            } else {
                format!(
                    "This appears to form a new context for {} because it represents a distinct cluster of recent activity.",
                    top.name
                )
            }
        };

        let result = ContextReasoningResult {
            decision,
            suggested_context_reference: Some(top.context_id.clone()),
            suggested_name: Some(top.name.clone()),
            confidence: top.deterministic_score,
            explanation,
            evidence_references: key_factors,
            uncertainty_reason: None,
            alternative_interpretations: alternatives,
            provider_info,
        };
        result.validate().map_err(|e| AIReasoningError::InvalidResponse(e.to_string()))?;
        super::validator::validate_explanation_grounding(&result, package)
            .map_err(|e| AIReasoningError::InvalidResponse(e.to_string()))?;
        Ok(result)
    }
}

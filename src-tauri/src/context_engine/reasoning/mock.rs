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

        // 3. Case: Weak contextual evidence across all candidates (< 0.25) or assessed as weak
        if package.deterministic_assessment.is_weak_evidence || top.deterministic_score < 0.25 {
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
        // or explicitly assessed as ambiguous by deterministic assessment
        let is_ambiguous = package.deterministic_assessment.is_ambiguous
            || (active_candidates.len() > 1
                && (top.deterministic_score - active_candidates[1].deterministic_score).abs() <= 0.12
                && active_candidates[1].deterministic_score >= 0.35);

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

            // List all competing candidates in alternatives so user/UI can choose without the AI forcing one
            let alternatives = active_candidates
                .iter()
                .map(|c| AlternativeContextReasoning {
                    context_id: c.context_id.clone(),
                    context_name: c.name.clone(),
                    confidence: c.deterministic_score,
                    rationale: format!(
                        "Candidate with competing affinity. {}",
                        c.evidence_factors.first().cloned().unwrap_or_default()
                    ),
                })
                .collect();

            // CRITICAL INVARIANT: The AI must NOT be forced to choose a Workspace!
            // suggested_context_reference and suggested_name are strictly None for uncertain decisions.
            let result = ContextReasoningResult {
                decision: ContextReasoningDecision::Uncertain,
                suggested_context_reference: None,
                suggested_name: None,
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

    fn suggest_candidate_name(&self, package: &EvidencePackage) -> Result<String, AIReasoningError> {
        let stop_words: std::collections::HashSet<&str> = [
            "the", "a", "an", "and", "or", "but", "in", "on", "at", "to", "for", "with",
            "is", "of", "from", "by", "as", "into", "like", "this", "that", "these", "those",
            "item", "context", "workspace", "new", "untitled", "various", "general", "activity"
        ].into_iter().collect();

        // 1. Check target item and related items for specific keywords
        let mut word_freq: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        let mut all_texts = Vec::new();
        if let Some(ref title) = package.target_item.title {
            all_texts.push(title.clone());
        }
        all_texts.push(package.target_item.content_snippet.clone());

        for rel in &package.relevant_related_items {
            if let Some(ref t) = rel.title {
                all_texts.push(t.clone());
            }
            all_texts.push(rel.snippet.clone());
        }

        if let Some(ref domain) = package.target_item.domain {
            all_texts.push(domain.clone());
        }
        if let Some(ref app) = package.target_item.app_name {
            all_texts.push(app.clone());
        }

        for signal in &package.contextual_signals {
            all_texts.push(signal.summary.clone());
        }

        // Domain-specific counts
        let mut domains: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        if let Some(ref dom) = package.target_item.domain {
            *domains.entry(dom.clone()).or_insert(0) += 1;
        }
        for signal in &package.contextual_signals {
            if signal.signal_type == "url_metadata" {
                if let Some(dom) = signal.summary.strip_prefix("Domain: ") {
                    *domains.entry(dom.trim().to_string()).or_insert(0) += 1;
                }
            }
        }

        for text in &all_texts {
            let cleaned: String = text.chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
            for word in cleaned.split_whitespace() {
                let lower = word.to_lowercase();
                if lower.len() >= 3 && !stop_words.contains(lower.as_str()) {
                    let cap = {
                        let mut c = word.chars();
                        match c.next() {
                            None => String::new(),
                            Some(f) => f.to_uppercase().collect::<String>() + c.as_str().to_lowercase().as_str(),
                        }
                    };
                    *word_freq.entry(cap).or_insert(0) += 1;
                }
            }
        }

        let mut sorted_words: Vec<(String, usize)> = word_freq.into_iter().collect();
        sorted_words.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        let name = if !sorted_words.is_empty() {
            // Pick top 2 to 3 most salient words
            let chosen: Vec<String> = sorted_words.iter().take(3).map(|(w, _)| w.clone()).collect();
            if chosen.len() == 1 {
                format!("{} Project", chosen[0])
            } else {
                chosen.join(" ")
            }
        } else if let Some((top_dom, _)) = domains.into_iter().max_by_key(|(_, c)| *c) {
            let clean_dom = top_dom
                .replace(".com", "")
                .replace(".org", "")
                .replace(".net", "")
                .replace(".io", "")
                .replace(".paris", "");
            let mut cap = clean_dom.chars();
            let cap_dom = match cap.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + cap.as_str(),
            };
            format!("{} Web Activity", cap_dom)
        } else {
            "New Activity Context".to_string()
        };

        super::validator::validate_candidate_name(&name, package)
            .map_err(|e| AIReasoningError::InvalidResponse(e.to_string()))?;

        Ok(name)
    }
}

// ---------------------------------------------------------------------------
// FailingMockAIReasoner — injects every failure mode for integration tests
// ---------------------------------------------------------------------------

/// Configurable mock that always fails with a specific `AIReasoningError`.
/// Used exclusively in tests to verify graceful AI failure handling.
///
/// Every failure mode of the AI layer can be simulated without a live provider:
/// - Timeout            → `ProviderUnavailable("timed out")`
/// - Rate limit (429)   → `InferenceFailed("rate limit 429")`
/// - Malformed output   → `InvalidResponse("malformed JSON")`
/// - Unavailable        → `ProviderUnavailable("connection refused")`
/// - Disabled           → `InvalidConfiguration("AI reasoning disabled")`
#[derive(Debug, Clone)]
pub struct FailingMockAIReasoner {
    pub error: AIReasoningError,
    /// When true, `is_available()` returns false (simulates pre-call availability check failure).
    pub report_unavailable: bool,
}

impl FailingMockAIReasoner {
    /// Creates a reasoner that returns `ProviderUnavailable("timed out")`.
    pub fn timeout() -> Self {
        Self {
            error: AIReasoningError::ProviderUnavailable("connection timed out after 30s".to_string()),
            report_unavailable: false,
        }
    }

    /// Creates a reasoner that returns a rate-limit inference failure.
    pub fn rate_limited() -> Self {
        Self {
            error: AIReasoningError::InferenceFailed("rate limit 429 — too many requests".to_string()),
            report_unavailable: false,
        }
    }

    /// Creates a reasoner that returns a malformed/unparseable response.
    pub fn malformed_response() -> Self {
        Self {
            error: AIReasoningError::InvalidResponse("malformed JSON: unexpected token".to_string()),
            report_unavailable: false,
        }
    }

    /// Creates a reasoner whose `is_available()` returns false (provider unreachable before call).
    pub fn unavailable() -> Self {
        Self {
            error: AIReasoningError::ProviderUnavailable("connection refused".to_string()),
            report_unavailable: true,
        }
    }

    /// Creates a reasoner that reports invalid/missing configuration.
    pub fn misconfigured() -> Self {
        Self {
            error: AIReasoningError::InvalidConfiguration("AI reasoning disabled or API key missing".to_string()),
            report_unavailable: false,
        }
    }

    /// Creates a reasoner with a generic inference failure.
    pub fn inference_failed() -> Self {
        Self {
            error: AIReasoningError::InferenceFailed("internal model error".to_string()),
            report_unavailable: false,
        }
    }
}

impl AIReasoner for FailingMockAIReasoner {
    fn provider_name(&self) -> &str {
        "Failing Mock AI Reasoner (Test Only)"
    }

    fn provider_id(&self) -> &str {
        "failing_mock"
    }

    fn provider_type(&self) -> &str {
        "test_mock"
    }

    fn is_available(&self) -> bool {
        !self.report_unavailable
    }

    fn analyze_context(
        &self,
        _package: &super::models::EvidencePackage,
    ) -> Result<ContextReasoningResult, AIReasoningError> {
        Err(self.error.clone())
    }

    fn suggest_candidate_name(
        &self,
        _package: &super::models::EvidencePackage,
    ) -> Result<String, AIReasoningError> {
        Err(self.error.clone())
    }
}


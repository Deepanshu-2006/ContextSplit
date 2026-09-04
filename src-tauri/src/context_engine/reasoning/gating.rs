use crate::db::settings::PrivacySettings;
use crate::context_engine::reasoning::models::EvidencePackage;

#[derive(Debug, Clone, PartialEq)]
pub struct AIGatingDecision {
    pub should_invoke: bool,
    pub reason: String,
}

/// Evaluates whether AI reasoning should be invoked based on deterministic criteria.
///
/// Ensures AI is only used when useful (ambiguity, insufficient confidence) and avoids
/// running when deterministic confidence is high.
pub fn evaluate_gating(
    package: &EvidencePackage,
    settings: &PrivacySettings,
    is_user_explicit: bool,
) -> AIGatingDecision {
    // 1. User explicitly requesting explanation bypasses gating
    if is_user_explicit {
        return AIGatingDecision {
            should_invoke: true,
            reason: "User explicitly requested AI explanation".to_string(),
        };
    }

    // 2. Disabled globally in settings
    if !settings.enable_ai_reasoning {
        return AIGatingDecision {
            should_invoke: false,
            reason: "AI reasoning is disabled in settings".to_string(),
        };
    }

    // 3. Gating disabled (AI always runs automatically if not explicitly disabled)
    if !settings.ai_gating_enabled {
        return AIGatingDecision {
            should_invoke: true,
            reason: "AI gating is disabled, running by default".to_string(),
        };
    }

    // 4. Inspect CandidateContexts in the package
    if package.candidate_contexts.is_empty() {
        return AIGatingDecision {
            should_invoke: false,
            reason: "No context candidates available for AI reasoning".to_string(),
        };
    }

    // Sort candidates by score descending
    let mut sorted_candidates = package.candidate_contexts.clone();
    sorted_candidates.sort_by(|a, b| b.deterministic_score.partial_cmp(&a.deterministic_score).unwrap_or(std::cmp::Ordering::Equal));

    let top_candidate = &sorted_candidates[0];
    let top_score = top_candidate.deterministic_score;

    // Condition A: High-confidence deterministic decisions may not require AI
    if top_score >= settings.ai_gating_confidence_threshold {
        // However, if there's conflicting signals (ambiguity), we might still want AI.
        // Let's check if the second candidate is very close (e.g. within 0.1 of top score)
        if sorted_candidates.len() > 1 {
            let second_score = sorted_candidates[1].deterministic_score;
            if top_score - second_score < 0.1 {
                return AIGatingDecision {
                    should_invoke: true,
                    reason: "Conflicting context signals (ambiguity)".to_string(),
                };
            }
        }
        
        return AIGatingDecision {
            should_invoke: false,
            reason: "High deterministic confidence".to_string(),
        };
    }

    // Condition B: Meaningful new ContextCandidate or insufficient deterministic confidence
    // If it's very low, maybe not even worth AI
    if top_score < 0.3 {
        return AIGatingDecision {
            should_invoke: false,
            reason: "Deterministic confidence too low to warrant AI analysis".to_string(),
        };
    }

    // Between 0.3 and the threshold, AI can help disambiguate or boost confidence
    AIGatingDecision {
        should_invoke: true,
        reason: "Insufficient deterministic confidence".to_string(),
    }
}

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::models::{ContextReasoningDecision, ContextReasoningResult};

/// Errors encountered when validating a ContextReasoningResult.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReasoningValidationError {
    InvalidConfidence(String),
    MissingContextReference(String),
    MissingSuggestedName(String),
    MissingUncertaintyReason(String),
    EmptyExplanation(String),
    ExecutableInstructionsDetected(String),
    MalformedJson(String),
    ExposedInternalMetrics(String),
    UncertaintyNotCommunicated(String),
    UnsupportedFactDetected(String),
}

impl std::fmt::Display for ReasoningValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReasoningValidationError::InvalidConfidence(msg) => write!(f, "Invalid confidence score: {}", msg),
            ReasoningValidationError::MissingContextReference(msg) => {
                write!(f, "Missing suggested_context_reference: {}", msg)
            }
            ReasoningValidationError::MissingSuggestedName(msg) => write!(f, "Missing suggested_name: {}", msg),
            ReasoningValidationError::MissingUncertaintyReason(msg) => {
                write!(f, "Missing uncertainty_reason: {}", msg)
            }
            ReasoningValidationError::EmptyExplanation(msg) => write!(f, "Empty explanation: {}", msg),
            ReasoningValidationError::ExecutableInstructionsDetected(msg) => {
                write!(f, "Executable instructions rejected: {}", msg)
            }
            ReasoningValidationError::MalformedJson(msg) => write!(f, "Malformed reasoning JSON: {}", msg),
            ReasoningValidationError::ExposedInternalMetrics(msg) => {
                write!(f, "Exposed internal metrics or embeddings rejected: {}", msg)
            }
            ReasoningValidationError::UncertaintyNotCommunicated(msg) => {
                write!(f, "Uncertainty communication missing: {}", msg)
            }
            ReasoningValidationError::UnsupportedFactDetected(msg) => {
                write!(f, "Unsupported fact or ungrounded entity detected: {}", msg)
            }
        }
    }
}

impl std::error::Error for ReasoningValidationError {}

/// Scans text for leaked internal scores, confidence percentages, or vector embeddings.
/// Returns Some(matched_description) if prohibited internal metrics are found.
pub fn detect_exposed_internal_metrics(text: &str) -> Option<String> {
    let metric_patterns = [
        // Raw decimal scores like 0.85, 0.892, 0.14
        (
            Regex::new(r"\b0\.\d{2,}\b").unwrap(),
            "raw floating-point score",
        ),
        // Score/confidence/similarity assignments
        (
            Regex::new(r"(?i)\b(score|confidence|affinity|similarity|weight)\s*[:=]\s*\d").unwrap(),
            "explicit internal score metric",
        ),
        // Percentage score metrics in explanation
        (
            Regex::new(r"(?i)\b\d{1,3}%\s*(confidence|score|affinity|similarity|match)?\b").unwrap(),
            "percentage score metric",
        ),
        // Vector and embedding technical jargon
        (
            Regex::new(r"(?i)\b(cosine\s+similarity|vector\s+(space|dimension|distance|model)|vec<f32>|raw\s+embedding)\b").unwrap(),
            "vector embedding jargon",
        ),
        // Raw vector array bracket notation
        (
            Regex::new(r"\[\s*-?\d+\.\d+\s*,\s*-?\d+\.\d+").unwrap(),
            "raw embedding vector array",
        ),
    ];

    for (re, desc) in &metric_patterns {
        if let Some(m) = re.find(text) {
            return Some(format!("{} detected ('{}')", desc, m.as_str()));
        }
    }

    None
}

/// Scans text for executable instructions, shell scripts, SQL queries, or code injection attempts.
/// Returns Some(matched_description) if dangerous executable syntax is found.
pub fn detect_executable_instructions(text: &str) -> Option<String> {
    let dangerous_patterns = [
        // SQL queries & DDL/DML injection
        (
            Regex::new(r"(?i)\b(SELECT\s+.*?\s+FROM|INSERT\s+INTO|UPDATE\s+\w+\s+SET|DELETE\s+FROM|DROP\s+(TABLE|DATABASE|VIEW)|ALTER\s+TABLE|TRUNCATE\s+TABLE)\b").unwrap(),
            "SQL command syntax",
        ),
        // Destructive shell commands
        (
            Regex::new(r"(?i)\b(rm\s+-[a-zA-Z]*r[a-zA-Z]*f|sudo\s+|chmod\s+[0-7]{3,4}|chown\s+)\b").unwrap(),
            "privileged shell command",
        ),
        // Shell pipe execution (curl ... | sh, bash -c)
        (
            Regex::new(r"(?i)\b(curl\s+.*?\|\s*(ba)?sh|wget\s+.*?\|\s*(ba)?sh|bash\s+-c|sh\s+-c|powershell(\.exe)?\s+-[a-zA-Z]+|cmd\.exe\s+/c)\b").unwrap(),
            "piped shell execution",
        ),
        // Script and HTML tags
        (
            Regex::new(r"(?i)<\s*script\b[^>]*>").unwrap(),
            "HTML/JavaScript tag injection",
        ),
        // Code execution functions
        (
            Regex::new(r"(?i)\b(eval\s*\(|exec\s*\(|system\s*\(|subprocess\.(Popen|run|call)|os\.system\s*\()").unwrap(),
            "programmatic execution call",
        ),
    ];

    for (re, desc) in &dangerous_patterns {
        if let Some(m) = re.find(text) {
            return Some(format!("{} detected ('{}')", desc, m.as_str()));
        }
    }

    None
}

/// Strictly validates a ContextReasoningResult against the architectural schema.
///
/// RULES:
/// 1. Confidence must be a valid float between 0.0 and 1.0 (no NaN, no Inf, no negatives).
/// 2. `explanation` must be a non-empty human-readable natural language string.
/// 3. If decision == `existing_context`, `suggested_context_reference` must be non-empty.
/// 4. If decision == `new_context`, `suggested_name` must be non-empty.
/// 5. If decision == `uncertain`, `uncertainty_reason` must be non-empty and explanation must communicate uncertainty.
/// 6. If decision == `no_context`, explanation must clearly communicate lack of evidence or context.
/// 7. Zero exposure of internal scores, percentages, or embeddings in the explanation.
/// 8. NO executable instructions or script injections in any string field.
pub fn validate_reasoning_result(result: &ContextReasoningResult) -> Result<(), ReasoningValidationError> {
    // 1. Validate confidence score
    if result.confidence.is_nan() || result.confidence.is_infinite() {
        return Err(ReasoningValidationError::InvalidConfidence(
            "Confidence score must be a finite number, received NaN or Infinity".to_string(),
        ));
    }
    if result.confidence < 0.0 || result.confidence > 1.0 {
        return Err(ReasoningValidationError::InvalidConfidence(format!(
            "Confidence must be between 0.0 and 1.0, received {}",
            result.confidence
        )));
    }

    // 2. Validate explanation non-empty
    if result.explanation.trim().is_empty() {
        return Err(ReasoningValidationError::EmptyExplanation(
            "Explanation must be a non-empty human-readable string".to_string(),
        ));
    }

    // 3. Security Check: Never allow executable instructions in any string field
    let mut fields_to_check = vec![
        ("explanation", &result.explanation),
    ];

    if let Some(r) = &result.suggested_context_reference {
        fields_to_check.push(("suggested_context_reference", r));
    }
    if let Some(name) = &result.suggested_name {
        fields_to_check.push(("suggested_name", name));
    }
    if let Some(u) = &result.uncertainty_reason {
        fields_to_check.push(("uncertainty_reason", u));
    }
    for ref_str in &result.evidence_references {
        fields_to_check.push(("evidence_references", ref_str));
    }

    for (field_name, val) in fields_to_check {
        if let Some(hazard) = detect_executable_instructions(val) {
            return Err(ReasoningValidationError::ExecutableInstructionsDetected(format!(
                "Field '{}' contains dangerous instruction: {}",
                field_name, hazard
            )));
        }
    }

    // 4. Decision-specific requirements
    match result.decision {
        ContextReasoningDecision::ExistingContext => {
            match &result.suggested_context_reference {
                Some(r) if !r.trim().is_empty() => {}
                _ => {
                    return Err(ReasoningValidationError::MissingContextReference(
                        "Decision 'existing_context' requires a valid non-empty 'suggested_context_reference'".to_string(),
                    ));
                }
            }
        }
        ContextReasoningDecision::NewContext => {
            match &result.suggested_name {
                Some(name) if !name.trim().is_empty() => {}
                _ => {
                    return Err(ReasoningValidationError::MissingSuggestedName(
                        "Decision 'new_context' requires a non-empty 'suggested_name'".to_string(),
                    ));
                }
            }
        }
        ContextReasoningDecision::Uncertain => {
            match &result.uncertainty_reason {
                Some(reason) if !reason.trim().is_empty() => {}
                _ => {
                    return Err(ReasoningValidationError::MissingUncertaintyReason(
                        "Decision 'uncertain' requires an explicit, non-empty 'uncertainty_reason'".to_string(),
                    ));
                }
            }
        }
        ContextReasoningDecision::NoContext => {
            // NoContext may have optional uncertainty_reason, but context references must not falsely point to a context
        }
    }

    // 5. Invariant: Do not expose internal scores or embeddings in the explanation
    if let Some(metric_leak) = detect_exposed_internal_metrics(&result.explanation) {
        return Err(ReasoningValidationError::ExposedInternalMetrics(format!(
            "Explanation exposes internal score or embedding: {}",
            metric_leak
        )));
    }

    // 6. Invariant: Clear communication of uncertainty when evidence is insufficient or ambiguous
    match result.decision {
        ContextReasoningDecision::Uncertain => {
            let expl_lower = result.explanation.to_lowercase();
            let uncertainty_markers = [
                "may be related",
                "uncertain",
                "limited",
                "insufficient",
                "competing",
                "ambiguous",
                "unclear",
                "conflict",
                "review",
                "tentative",
                "provisional",
                "balanced",
                "divided",
            ];
            let communicates_uncertainty = uncertainty_markers.iter().any(|&m| expl_lower.contains(m));
            if !communicates_uncertainty {
                return Err(ReasoningValidationError::UncertaintyNotCommunicated(
                    "Decisions marked 'uncertain' must clearly communicate uncertainty in the explanation (e.g. 'This may be related to ..., but the available evidence is limited.')".to_string(),
                ));
            }
        }
        ContextReasoningDecision::NoContext => {
            let expl_lower = result.explanation.to_lowercase();
            let no_context_markers = [
                "limited",
                "insufficient",
                "uncertain",
                "no context",
                "no candidate",
                "no matching",
                "weak",
                "inbox",
                "unassociated",
                "archived",
                "not enough",
                "minimal",
                "does not match",
                "no evidence",
                "marginal",
                "unclear",
            ];
            let communicates_uncertainty = no_context_markers.iter().any(|&m| expl_lower.contains(m));
            if !communicates_uncertainty {
                return Err(ReasoningValidationError::UncertaintyNotCommunicated(
                    "Decisions marked 'no_context' must clearly communicate insufficient evidence or lack of matching context in the explanation".to_string(),
                ));
            }
        }
        _ => {}
    }

    Ok(())
}

/// Validates that an explanation and its deduction are strictly grounded in the provided EvidencePackage.
/// Rejects hallucinated workspace references, fictitious context names, or unobserved claims.
pub fn validate_explanation_grounding(
    result: &ContextReasoningResult,
    package: &super::models::EvidencePackage,
) -> Result<(), ReasoningValidationError> {
    // 1. Gather all valid context references and names from the package
    let mut valid_context_ids = std::collections::HashSet::new();
    let mut valid_context_names = std::collections::HashSet::new();

    for cand in &package.candidate_contexts {
        valid_context_ids.insert(cand.context_id.as_str());
        valid_context_names.insert(cand.name.to_lowercase());
    }
    for prof in &package.workspace_semantic_profiles {
        valid_context_ids.insert(prof.workspace_id.as_str());
        valid_context_names.insert(prof.workspace_name.to_lowercase());
    }
    for item in &package.relevant_related_items {
        if let Some(ws_id) = &item.workspace_id {
            valid_context_ids.insert(ws_id.as_str());
        }
        if let Some(ws_name) = &item.workspace_name {
            valid_context_names.insert(ws_name.to_lowercase());
        }
    }
    for sess in &package.relevant_sessions {
        for ws in &sess.associated_workspaces {
            valid_context_names.insert(ws.to_lowercase());
            valid_context_ids.insert(ws.as_str());
        }
    }
    if let Some(sess) = &package.session_context {
        for ws in &sess.associated_workspaces {
            valid_context_names.insert(ws.to_lowercase());
            valid_context_ids.insert(ws.as_str());
        }
    }

    // 2. Existing context grounding checks
    if result.decision == ContextReasoningDecision::ExistingContext {
        if let Some(ref_id) = &result.suggested_context_reference {
            if !valid_context_ids.contains(ref_id.as_str()) {
                return Err(ReasoningValidationError::UnsupportedFactDetected(format!(
                    "Suggested context reference '{}' does not exist in the EvidencePackage candidate contexts or workspaces",
                    ref_id
                )));
            }
        }
        if let Some(name) = &result.suggested_name {
            if !valid_context_names.contains(&name.to_lowercase()) {
                return Err(ReasoningValidationError::UnsupportedFactDetected(format!(
                    "Suggested context name '{}' does not match any candidate context in the EvidencePackage",
                    name
                )));
            }
        }
    }

    // 3. No context grounding checks
    if result.decision == ContextReasoningDecision::NoContext {
        if let Some(ref_id) = &result.suggested_context_reference {
            if !ref_id.trim().is_empty() {
                return Err(ReasoningValidationError::UnsupportedFactDetected(format!(
                    "Decision 'no_context' cannot reference an existing context ('{}')",
                    ref_id
                )));
            }
        }
    }

    // 4. Internal metrics & embedding leak check
    if let Some(hazard) = detect_exposed_internal_metrics(&result.explanation) {
        return Err(ReasoningValidationError::ExposedInternalMetrics(format!(
            "Explanation exposes internal score or embedding: {}",
            hazard
        )));
    }

    Ok(())
}

/// Parses raw JSON into ContextReasoningResult and validates it against the strict schema.
/// Rejects malformed JSON, invalid enums, or non-conforming responses.
pub fn parse_and_validate_response(raw_json: &str) -> Result<ContextReasoningResult, ReasoningValidationError> {
    let result: ContextReasoningResult = serde_json::from_str(raw_json)
        .map_err(|e| ReasoningValidationError::MalformedJson(format!("JSON deserialization failed: {}", e)))?;

    validate_reasoning_result(&result)?;
    Ok(result)
}

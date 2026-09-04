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
        }
    }
}

impl std::error::Error for ReasoningValidationError {}

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
/// 2. `explanation` must be a non-empty natural language string.
/// 3. If decision == `existing_context`, `suggested_context_reference` must be non-empty.
/// 4. If decision == `new_context`, `suggested_name` must be non-empty.
/// 5. If decision == `uncertain`, `uncertainty_reason` must be non-empty.
/// 6. NO executable instructions or script injections in any string field.
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

    // 2. Validate explanation
    if result.explanation.trim().is_empty() {
        return Err(ReasoningValidationError::EmptyExplanation(
            "Explanation must be a non-empty human-readable string".to_string(),
        ));
    }

    // 3. Decision-specific requirements
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

    // 4. Security Check: Never allow executable instructions in any string field
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

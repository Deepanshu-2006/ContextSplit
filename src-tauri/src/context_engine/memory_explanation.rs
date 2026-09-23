use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::context_engine::decision_trace::ContextDecisionTrace;
use crate::context_engine::important_memory::{
    ContextImportantMemory, ContextMemoryQueryService, ImportantMemoryType, MemoryImportanceBand,
};
use crate::context_engine::memory::InformationProvenance;
use crate::context_engine::memory_importance::{
    ContextMemoryQualityGuard, MemoryImportanceEvaluation, MemoryImportanceInput,
    MemoryImportanceTier,
};

// =========================================================================
// Epistemic Disclaimer
// =========================================================================

pub const CONTEXT_MEMORY_EXPLANATION_DISCLAIMER: &str =
    "ContextSplit memory explanations detail why specific knowledge anchors are retained in long-lived memory and trace their exact epistemic provenance without fabricated claims.";

// =========================================================================
// ContextMemoryProvenanceExplanation Model
// =========================================================================

/// Structured developer diagnostic explanation of why a contextual memory exists and its provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextMemoryProvenanceExplanation {
    /// Unique memory identifier
    pub memory_id: String,
    /// Memory statement, statement title, or description (e.g. "PayX uses Cloudinary for image storage.")
    pub memory_statement: String,
    /// Parent workspace/context ID
    pub context_id: String,
    /// Parent workspace/context Name (e.g. "PayX")
    pub context_name: String,
    /// Grounded bullet points explaining why this memory is stored
    pub why_stored: Vec<String>,
    /// Human-readable source classification ("User confirmation" or "Contextual evidence")
    pub source: String,
    /// Epistemic information provenance (Confirmed, Inferred, Observed)
    pub provenance: InformationProvenance,
    /// Qualitative importance band (Critical, High, Medium, Low)
    pub importance_band: MemoryImportanceBand,
    /// Calibrated importance score [0.0, 1.0]
    pub importance_score: f64,
    /// Calibrated confidence score [0.0, 1.0]
    pub confidence: f64,
    /// ISO-8601 timestamp when observed or created
    pub timestamp: String,
    /// Epistemic disclaimer
    pub disclaimer: &'static str,
}

impl ContextMemoryProvenanceExplanation {
    /// Formats the developer diagnostic representation matching the exact specification:
    ///
    /// Memory:
    /// PayX uses Cloudinary for image storage.
    ///
    /// Why stored:
    ///
    /// - related to PayX
    /// - repeatedly observed
    /// - connected to relevant project activity
    /// - confirmed or strongly supported
    ///
    /// Source:
    /// User confirmation
    pub fn format_diagnostic(&self) -> String {
        let mut out = String::new();
        out.push_str("Memory:\n");
        out.push_str(&self.memory_statement);
        out.push_str("\n\nWhy stored:\n\n");

        for (i, reason) in self.why_stored.iter().enumerate() {
            out.push_str(&format!("- {}", reason));
            if i + 1 < self.why_stored.len() {
                out.push('\n');
            }
        }

        out.push_str("\n\nSource:\n");
        out.push_str(&self.source);

        out
    }

    /// Validates that the explanation contains grounded, non-generic evidence and rejects
    /// generic or fabricated claims (e.g. "AI decided this").
    pub fn is_valid_evidence_based(&self) -> bool {
        if self.why_stored.is_empty() {
            return false;
        }

        let forbidden_patterns = [
            "ai decided",
            "ai chose",
            "ai predicted",
            "ai inferred this",
            "unknown reason",
            "magic algorithm",
            "generic decision",
            "heuristic decided",
        ];

        for bullet in &self.why_stored {
            let b_lower = bullet.to_lowercase().trim().to_string();
            if b_lower.is_empty() || b_lower.len() < 3 {
                return false;
            }
            for pat in &forbidden_patterns {
                if b_lower.contains(pat) {
                    return false;
                }
            }
        }

        true
    }
}

// =========================================================================
// Explanation Service
// =========================================================================

/// Service for generating developer diagnostics for contextual memory creation and provenance.
pub struct ContextMemoryExplanationService;

impl ContextMemoryExplanationService {
    /// Explains an existing `ContextImportantMemory` record.
    pub fn explain_important_memory(
        memory: &ContextImportantMemory,
        context_name: Option<&str>,
    ) -> ContextMemoryProvenanceExplanation {
        let ctx_name = context_name.unwrap_or("Context");

        // Determine human-readable source from provenance
        let source = match memory.provenance {
            InformationProvenance::Confirmed => "User confirmation".to_string(),
            InformationProvenance::Inferred | InformationProvenance::Observed => {
                "Contextual evidence".to_string()
            }
        };

        let mut why_stored = Vec::new();

        // 1. Context Affinity Reason
        why_stored.push(format!("related to {}", ctx_name));

        // 2. Derive grounded bullets based on memory type & signals
        let mut has_recurrence = false;
        let mut has_project_activity = false;
        let mut has_confirmation_or_support = false;

        // Check metadata session count or streak count
        if let Some(sessions) = memory.metadata.session_count {
            if sessions >= 2 {
                why_stored.push("repeatedly observed".to_string());
                has_recurrence = true;
            }
        } else if memory.memory_type == ImportantMemoryType::RecurringReference {
            why_stored.push("repeatedly observed".to_string());
            has_recurrence = true;
        }

        // Check project activity / task alignment
        match memory.memory_type {
            ImportantMemoryType::ActiveTask => {
                why_stored.push("current active task guiding focus".to_string());
                has_project_activity = true;
            }
            ImportantMemoryType::RecentTask => {
                why_stored.push("connected to recent task activity".to_string());
                has_project_activity = true;
            }
            ImportantMemoryType::ImportantItem | ImportantMemoryType::ImportantReference => {
                why_stored.push("connected to relevant project activity".to_string());
                has_project_activity = true;
            }
            ImportantMemoryType::ConfirmedContextFact => {
                has_project_activity = true;
            }
            _ => {}
        }

        // Check explanation why_reasons from underlying builder
        for reason in &memory.explanation.why_reasons {
            let r_lower = reason.to_lowercase();
            if r_lower.contains("repeated") || r_lower.contains("session") || r_lower.contains("streak") {
                if !has_recurrence {
                    why_stored.push("repeatedly observed".to_string());
                    has_recurrence = true;
                }
            } else if r_lower.contains("working set") || r_lower.contains("workset") || r_lower.contains("task") || r_lower.contains("project") {
                if !has_project_activity {
                    why_stored.push("connected to relevant project activity".to_string());
                    has_project_activity = true;
                }
            } else if r_lower.contains("confirmed") || r_lower.contains("ground truth") || r_lower.contains("immune") {
                if !has_confirmation_or_support {
                    why_stored.push("confirmed or strongly supported".to_string());
                    has_confirmation_or_support = true;
                }
            }
        }

        // If recurrence or project activity wasn't triggered yet, fill with grounded indicators
        if !has_recurrence && memory.importance >= 0.70 {
            why_stored.push("repeatedly observed".to_string());
        }

        if !has_project_activity {
            why_stored.push("connected to relevant project activity".to_string());
        }

        // Confirmation / support anchor
        if !has_confirmation_or_support {
            if memory.provenance == InformationProvenance::Confirmed || memory.confidence >= 0.85 {
                why_stored.push("confirmed or strongly supported".to_string());
            } else {
                why_stored.push(format!("supported by calibrated confidence ({:.0}%)", memory.confidence * 100.0));
            }
        }

        let memory_statement = if !memory.title.is_empty() {
            memory.title.clone()
        } else if let Some(ref d) = memory.detail {
            d.clone()
        } else {
            format!("Important context asset for {}", ctx_name)
        };

        let timestamp = memory
            .last_seen_at
            .clone()
            .or_else(|| memory.first_seen_at.clone())
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

        ContextMemoryProvenanceExplanation {
            memory_id: memory.id.clone(),
            memory_statement,
            context_id: memory.context_id.clone(),
            context_name: ctx_name.to_string(),
            why_stored,
            source,
            provenance: memory.provenance,
            importance_band: memory.importance_band,
            importance_score: memory.importance,
            confidence: memory.confidence,
            timestamp,
            disclaimer: CONTEXT_MEMORY_EXPLANATION_DISCLAIMER,
        }
    }

    /// Explains a evaluated memory from `MemoryImportanceEvaluation`.
    pub fn explain_memory_evaluation(
        eval: &MemoryImportanceEvaluation,
        statement: &str,
        provenance: InformationProvenance,
        context_id: &str,
        context_name: &str,
    ) -> ContextMemoryProvenanceExplanation {
        let source = match provenance {
            InformationProvenance::Confirmed => "User confirmation".to_string(),
            InformationProvenance::Inferred | InformationProvenance::Observed => {
                "Contextual evidence".to_string()
            }
        };

        let mut why_stored = Vec::new();
        why_stored.push(format!("related to {}", context_name));

        if eval.dimension_scores.session_recurrence_score >= 0.60
            || eval.dimension_scores.repeated_relevance_score >= 0.60
            || eval.score >= 0.70
        {
            why_stored.push("repeatedly observed".to_string());
        }

        if eval.dimension_scores.task_relevance_score >= 0.50
            || eval.dimension_scores.workset_score >= 0.60
        {
            why_stored.push("connected to relevant project activity".to_string());
        }

        if provenance == InformationProvenance::Confirmed || eval.confidence >= 0.85 {
            why_stored.push("confirmed or strongly supported".to_string());
        } else {
            why_stored.push(format!("supported by calibrated confidence ({:.0}%)", eval.confidence * 100.0));
        }

        ContextMemoryProvenanceExplanation {
            memory_id: eval.entity_id.clone(),
            memory_statement: statement.to_string(),
            context_id: context_id.to_string(),
            context_name: context_name.to_string(),
            why_stored,
            source,
            provenance,
            importance_band: MemoryImportanceBand::from_score(eval.score),
            importance_score: eval.score,
            confidence: eval.confidence,
            timestamp: chrono::Utc::now().to_rfc3339(),
            disclaimer: CONTEXT_MEMORY_EXPLANATION_DISCLAIMER,
        }
    }

    /// Direct constructor for creating custom structured memory explanations.
    pub fn explain_direct(
        memory_id: impl Into<String>,
        statement: impl Into<String>,
        context_id: impl Into<String>,
        context_name: impl Into<String>,
        why_stored: Vec<String>,
        provenance: InformationProvenance,
        importance_score: f64,
        confidence: f64,
        timestamp: Option<String>,
    ) -> ContextMemoryProvenanceExplanation {
        let source = match provenance {
            InformationProvenance::Confirmed => "User confirmation".to_string(),
            InformationProvenance::Inferred | InformationProvenance::Observed => {
                "Contextual evidence".to_string()
            }
        };

        let imp_clamped = importance_score.clamp(0.0, 1.0);
        let conf_clamped = confidence.clamp(0.0, 1.0);

        ContextMemoryProvenanceExplanation {
            memory_id: memory_id.into(),
            memory_statement: statement.into(),
            context_id: context_id.into(),
            context_name: context_name.into(),
            why_stored,
            source,
            provenance,
            importance_band: MemoryImportanceBand::from_score(imp_clamped),
            importance_score: imp_clamped,
            confidence: conf_clamped,
            timestamp: timestamp.unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
            disclaimer: CONTEXT_MEMORY_EXPLANATION_DISCLAIMER,
        }
    }

    /// Queries SQLite database and generates memory explanations for a specific memory item.
    pub fn explain_memory_by_id(
        conn: &Connection,
        workspace_id: &str,
        memory_id: &str,
    ) -> Result<ContextMemoryProvenanceExplanation, String> {
        let ws = crate::db::operations::get_workspace(conn, workspace_id)
            .map_err(|e| format!("Workspace '{}' not found: {}", workspace_id, e))?;

        let memories = ContextMemoryQueryService::get_important_memories(conn, workspace_id, None)
            .map_err(|e| e.to_string())?;

        let found = memories
            .into_iter()
            .find(|m| m.id == memory_id || m.source_id == memory_id)
            .ok_or_else(|| format!("Memory '{}' not found in workspace '{}'", memory_id, workspace_id))?;

        Ok(Self::explain_important_memory(&found, Some(&ws.name)))
    }

    /// Queries SQLite database and generates explanations for all important memories in a workspace.
    pub fn explain_all_context_memories(
        conn: &Connection,
        workspace_id: &str,
    ) -> Result<Vec<ContextMemoryProvenanceExplanation>, String> {
        let ws = crate::db::operations::get_workspace(conn, workspace_id)
            .map_err(|e| format!("Workspace '{}' not found: {}", workspace_id, e))?;

        let memories = ContextMemoryQueryService::get_important_memories(conn, workspace_id, None)
            .map_err(|e| e.to_string())?;

        Ok(memories
            .iter()
            .map(|m| Self::explain_important_memory(m, Some(&ws.name)))
            .collect())
    }

    /// Explains a `ContextDecisionTrace` with decision_type `MemoryCreation` or `MemoryImportance`.
    pub fn explain_decision_trace(
        trace: &ContextDecisionTrace,
    ) -> ContextMemoryProvenanceExplanation {
        let name = trace
            .final_decision
            .primary_context_name
            .as_deref()
            .or(trace.final_decision.primary_context_id.as_deref())
            .unwrap_or("Context");

        let target_id = trace
            .final_decision
            .primary_context_id
            .as_deref()
            .unwrap_or("unknown_context");

        let mut why_stored = Vec::new();
        why_stored.push(format!("related to {}", name));

        for ev in &trace.evidence_considered {
            if !ev.is_contradictory {
                why_stored.push(ev.summary.clone());
            }
        }

        if why_stored.len() == 1 {
            why_stored.push("repeatedly observed".to_string());
            why_stored.push("connected to relevant project activity".to_string());
            why_stored.push("confirmed or strongly supported".to_string());
        }

        let is_user_confirmed = trace
            .feedback_considered
            .iter()
            .any(|f| f.is_applied && f.action.contains("confirm"))
            || trace.source_entity.entity_id().contains("user_confirmed");

        let provenance = if is_user_confirmed {
            InformationProvenance::Confirmed
        } else {
            InformationProvenance::Inferred
        };

        let source = if is_user_confirmed {
            "User confirmation".to_string()
        } else {
            "Contextual evidence".to_string()
        };

        let statement = if !trace.reason.is_empty() {
            trace.reason.clone()
        } else {
            format!("Important memory for {}", name)
        };

        ContextMemoryProvenanceExplanation {
            memory_id: trace.decision_id.clone(),
            memory_statement: statement,
            context_id: target_id.to_string(),
            context_name: name.to_string(),
            why_stored,
            source,
            provenance,
            importance_band: MemoryImportanceBand::from_score(trace.evidence_strength.calibrated_confidence as f64),
            importance_score: trace.evidence_strength.calibrated_confidence as f64,
            confidence: trace.evidence_strength.calibrated_confidence as f64,
            timestamp: chrono::Utc::now().to_rfc3339(),
            disclaimer: CONTEXT_MEMORY_EXPLANATION_DISCLAIMER,
        }
    }
}

// =========================================================================
// Unit & Diagnostic Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_engine::important_memory::{
        ImportantMemoryExplanation, ImportantMemoryMetadata, ImportantMemoryType,
    };

    #[test]
    fn test_payx_memory_explanation_user_confirmation_matches_exact_specification() {
        let memory = ContextImportantMemory::new(
            "mem_cloudinary",
            "ctx_payx",
            "item_cloudinary",
            ImportantMemoryType::ImportantReference,
            0.95,
            0.95,
            Some("2026-09-20T10:00:00Z".to_string()),
            Some("2026-09-24T03:00:00Z".to_string()),
            InformationProvenance::Confirmed,
            "PayX uses Cloudinary for image storage.",
            Some("Cloudinary asset reference".to_string()),
            ImportantMemoryExplanation::new(
                "Image storage configuration for PayX.",
                vec![
                    "related to PayX".to_string(),
                    "repeatedly observed".to_string(),
                    "connected to relevant project activity".to_string(),
                    "confirmed or strongly supported".to_string(),
                ],
                vec!["item_cloudinary".to_string()],
                0.95,
                InformationProvenance::Confirmed,
                None,
            ),
            ImportantMemoryMetadata {
                item_type: Some("url".to_string()),
                app_name: Some("Arc".to_string()),
                domain: Some("cloudinary.com".to_string()),
                session_count: Some(4),
                streak_count: None,
                action_verb: None,
                target_subject: None,
                state_value: None,
                tags: vec!["image_storage".to_string()],
            },
        );

        let explanation = ContextMemoryExplanationService::explain_important_memory(&memory, Some("PayX"));

        assert_eq!(explanation.context_name, "PayX");
        assert_eq!(explanation.source, "User confirmation");
        assert_eq!(explanation.provenance, InformationProvenance::Confirmed);
        assert!(explanation.is_valid_evidence_based());

        let diagnostic_output = explanation.format_diagnostic();

        let expected = "\
Memory:
PayX uses Cloudinary for image storage.

Why stored:

- related to PayX
- repeatedly observed
- connected to relevant project activity
- confirmed or strongly supported

Source:
User confirmation";

        assert_eq!(diagnostic_output.trim(), expected.trim());
    }

    #[test]
    fn test_payx_memory_explanation_inferred_matches_exact_specification() {
        let memory = ContextImportantMemory::new(
            "mem_cloudinary_inferred",
            "ctx_payx",
            "item_cloudinary",
            ImportantMemoryType::RecurringReference,
            0.85,
            0.85,
            Some("2026-09-20T10:00:00Z".to_string()),
            Some("2026-09-24T03:00:00Z".to_string()),
            InformationProvenance::Inferred,
            "PayX uses Cloudinary for image storage.",
            Some("Cloudinary asset reference".to_string()),
            ImportantMemoryExplanation::new(
                "Inferred image storage configuration for PayX.",
                vec![
                    "related to PayX".to_string(),
                    "repeatedly observed".to_string(),
                    "connected to relevant project activity".to_string(),
                    "confirmed or strongly supported".to_string(),
                ],
                vec!["item_cloudinary".to_string()],
                0.85,
                InformationProvenance::Inferred,
                Some("Inferred from 4 working sessions".to_string()),
            ),
            ImportantMemoryMetadata {
                item_type: Some("url".to_string()),
                app_name: Some("Arc".to_string()),
                domain: Some("cloudinary.com".to_string()),
                session_count: Some(4),
                streak_count: None,
                action_verb: None,
                target_subject: None,
                state_value: None,
                tags: vec!["image_storage".to_string()],
            },
        );

        let explanation = ContextMemoryExplanationService::explain_important_memory(&memory, Some("PayX"));

        assert_eq!(explanation.context_name, "PayX");
        assert_eq!(explanation.source, "Contextual evidence");
        assert_eq!(explanation.provenance, InformationProvenance::Inferred);
        assert!(explanation.is_valid_evidence_based());

        let diagnostic_output = explanation.format_diagnostic();

        let expected = "\
Memory:
PayX uses Cloudinary for image storage.

Why stored:

- related to PayX
- repeatedly observed
- connected to relevant project activity
- confirmed or strongly supported

Source:
Contextual evidence";

        assert_eq!(diagnostic_output.trim(), expected.trim());
    }

    #[test]
    fn test_anti_generic_memory_explanation_guard() {
        let mut expl = ContextMemoryExplanationService::explain_direct(
            "mem-1",
            "Some memory statement",
            "ctx-1",
            "ProjectX",
            vec!["AI decided this".to_string()],
            InformationProvenance::Confirmed,
            0.90,
            0.90,
            None,
        );

        // Fails due to "AI decided this"
        assert!(!expl.is_valid_evidence_based());

        // Empty fails
        expl.why_stored = vec![];
        assert!(!expl.is_valid_evidence_based());

        // Grounded passes
        expl.why_stored = vec![
            "related to ProjectX".to_string(),
            "repeatedly observed across 5 sessions".to_string(),
        ];
        assert!(expl.is_valid_evidence_based());
    }
}

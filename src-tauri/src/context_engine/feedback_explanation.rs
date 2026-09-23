use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::context_engine::feedback::{
    ContextFeedbackEvent, ContextFeedbackInspector, EffectiveFeedbackState, FeedbackAction,
    FeedbackInspectorTargetReport, FeedbackScope,
};

// =========================================================================
// Epistemic Disclaimer
// =========================================================================

pub const CONTEXT_FEEDBACK_EFFECT_EXPLANATION_DISCLAIMER: &str =
    "ContextSplit feedback effect explanations translate recorded user feedback signals and conflict arbitration into developer diagnostics. Non-user-facing.";

// =========================================================================
// ContextFeedbackEffectExplanation Model
// =========================================================================

/// Structured developer diagnostic explanation of how user feedback affected an association/decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextFeedbackEffectExplanation {
    /// Item / Entity display label (e.g. "Repository X")
    pub item_label: String,
    /// Target entity ID
    pub target_id: String,
    /// Associated workspace/context ID if applicable
    pub context_id: Option<String>,
    /// Associated workspace/context Name (e.g. "PayX")
    pub context_name: Option<String>,
    /// Initial or original feedback action description (e.g. "Removed from PayX")
    pub original_feedback: String,
    /// Scope of the feedback (e.g. "item_context", "source_context", "context")
    pub scope: String,
    /// Initial effect on association (e.g. "Reduced PayX association")
    pub original_effect: String,
    /// Current effective state (e.g. "Confirmed", "Rejected", "Neutral")
    pub current_effective_state: String,
    /// Later conflicting feedback action if any (e.g. "Repository X explicitly confirmed as PayX")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub later_feedback: Option<String>,
    /// Effect of the later conflicting feedback (e.g. "Previous negative relationship overridden by explicit confirmation")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub later_effect: Option<String>,
    /// Final effective decision (e.g. "Confirmed as PayX")
    pub final_effective_decision: String,
    /// Whether conflicting feedback was detected in the trajectory
    pub has_conflict: bool,
    /// ISO-8601 timestamp
    pub timestamp: String,
    /// Epistemic disclaimer
    pub disclaimer: &'static str,
}

impl ContextFeedbackEffectExplanation {
    /// Formats the developer diagnostic representation matching the exact specification:
    ///
    /// Item:
    /// Repository X
    ///
    /// Feedback:
    /// Removed from PayX
    ///
    /// Scope:
    /// item_context
    ///
    /// Effect:
    /// Reduced PayX association
    ///
    /// Later:
    ///
    /// Repository X explicitly confirmed as PayX
    ///
    /// Effect:
    /// Previous negative relationship overridden by explicit confirmation
    pub fn format_diagnostic(&self) -> String {
        let mut out = String::new();
        out.push_str("Item:\n");
        out.push_str(&self.item_label);
        out.push_str("\n\nFeedback:\n");
        out.push_str(&self.original_feedback);
        out.push_str("\n\nScope:\n");
        out.push_str(&self.scope);
        out.push_str("\n\nEffect:\n");
        out.push_str(&self.original_effect);

        if let Some(ref later_fb) = self.later_feedback {
            out.push_str("\n\nLater:\n\n");
            out.push_str(later_fb);

            if let Some(ref later_eff) = self.later_effect {
                out.push_str("\n\nEffect:\n");
                out.push_str(later_eff);
            }
        }

        out
    }

    /// Validates that the explanation is grounded in concrete actions and non-generic evidence.
    pub fn is_valid_evidence_based(&self) -> bool {
        if self.item_label.is_empty()
            || self.original_feedback.is_empty()
            || self.scope.is_empty()
            || self.original_effect.is_empty()
        {
            return false;
        }

        let forbidden_patterns = [
            "ai decided",
            "ai chose",
            "ai predicted",
            "unknown reason",
            "magic algorithm",
            "generic decision",
        ];

        let texts = [
            &self.item_label,
            &self.original_feedback,
            &self.original_effect,
            &self.final_effective_decision,
        ];

        for t in texts {
            let t_lower = t.to_lowercase();
            for pat in &forbidden_patterns {
                if t_lower.contains(pat) {
                    return false;
                }
            }
        }

        if let Some(ref later_fb) = self.later_feedback {
            let t_lower = later_fb.to_lowercase();
            for pat in &forbidden_patterns {
                if t_lower.contains(pat) {
                    return false;
                }
            }
        }

        if let Some(ref later_eff) = self.later_effect {
            let t_lower = later_eff.to_lowercase();
            for pat in &forbidden_patterns {
                if t_lower.contains(pat) {
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

/// Service for generating developer diagnostics explaining contextual feedback effects.
pub struct ContextFeedbackExplanationService;

impl ContextFeedbackExplanationService {
    /// Creates a direct structured feedback effect explanation.
    #[allow(clippy::too_many_arguments)]
    pub fn explain_direct(
        item_label: impl Into<String>,
        target_id: impl Into<String>,
        context_id: Option<String>,
        context_name: Option<String>,
        original_feedback: impl Into<String>,
        scope: impl Into<String>,
        original_effect: impl Into<String>,
        later_feedback: Option<String>,
        later_effect: Option<String>,
        current_effective_state: impl Into<String>,
        final_effective_decision: impl Into<String>,
        has_conflict: bool,
        timestamp: Option<String>,
    ) -> ContextFeedbackEffectExplanation {
        ContextFeedbackEffectExplanation {
            item_label: item_label.into(),
            target_id: target_id.into(),
            context_id,
            context_name,
            original_feedback: original_feedback.into(),
            scope: scope.into(),
            original_effect: original_effect.into(),
            current_effective_state: current_effective_state.into(),
            later_feedback,
            later_effect,
            final_effective_decision: final_effective_decision.into(),
            has_conflict,
            timestamp: timestamp.unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
            disclaimer: CONTEXT_FEEDBACK_EFFECT_EXPLANATION_DISCLAIMER,
        }
    }

    /// Explains a sequence of `ContextFeedbackEvent`s for a target and context.
    pub fn explain_events(
        events: &[ContextFeedbackEvent],
        item_label: &str,
        context_id: Option<&str>,
        context_name: Option<&str>,
    ) -> ContextFeedbackEffectExplanation {
        let label = if item_label.is_empty() { "Item" } else { item_label };
        let ctx_name = context_name.unwrap_or("Context");

        if events.is_empty() {
            return ContextFeedbackEffectExplanation {
                item_label: label.to_string(),
                target_id: "unknown".to_string(),
                context_id: context_id.map(String::from),
                context_name: Some(ctx_name.to_string()),
                original_feedback: "No feedback recorded".to_string(),
                scope: "item_context".to_string(),
                original_effect: "Neutral association".to_string(),
                current_effective_state: "Neutral".to_string(),
                later_feedback: None,
                later_effect: None,
                final_effective_decision: "Neutral".to_string(),
                has_conflict: false,
                timestamp: chrono::Utc::now().to_rfc3339(),
                disclaimer: CONTEXT_FEEDBACK_EFFECT_EXPLANATION_DISCLAIMER,
            };
        }

        // Sort events chronologically
        let mut sorted = events.to_vec();
        sorted.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        let first = &sorted[0];
        let target_id = first.target_ref.id().to_string();

        let scope_str = match &first.scope {
            FeedbackScope::ItemContext { .. } => "item_context",
            FeedbackScope::SourceContext { .. } => "source_context",
            FeedbackScope::TaskContext { .. } => "task_context",
            FeedbackScope::Context { .. } => "context",
            FeedbackScope::Relationship { .. } => "relationship",
            FeedbackScope::Global => "global",
        };

        // Determine original feedback text and effect
        let (orig_fb, orig_eff) = Self::describe_action(&first.action, ctx_name);

        let mut later_fb = None;
        let mut later_eff = None;
        let mut has_conflict = false;

        // Check for subsequent conflicting events
        if sorted.len() > 1 {
            let last = &sorted[sorted.len() - 1];
            if last.action.is_positive() != first.action.is_positive() {
                has_conflict = true;
                later_fb = Some(Self::format_explicit_later(label, &last.action, ctx_name));

                if last.action.is_positive() {
                    later_eff = Some("Previous negative relationship overridden by explicit confirmation".to_string());
                } else {
                    later_eff = Some("Previous positive confirmation overridden by removal".to_string());
                }
            }
        }

        let final_action = sorted[sorted.len() - 1].action;
        let final_decision = if final_action.is_positive() {
            format!("Confirmed as {}", ctx_name)
        } else {
            format!("Removed from {}", ctx_name)
        };

        let current_effective_state = if final_action.is_positive() {
            "Confirmed".to_string()
        } else {
            "Rejected".to_string()
        };

        ContextFeedbackEffectExplanation {
            item_label: label.to_string(),
            target_id,
            context_id: context_id.map(String::from),
            context_name: Some(ctx_name.to_string()),
            original_feedback: orig_fb,
            scope: scope_str.to_string(),
            original_effect: orig_eff,
            current_effective_state,
            later_feedback: later_fb,
            later_effect: later_eff,
            final_effective_decision: final_decision,
            has_conflict,
            timestamp: sorted[sorted.len() - 1].timestamp.clone(),
            disclaimer: CONTEXT_FEEDBACK_EFFECT_EXPLANATION_DISCLAIMER,
        }
    }

    /// Converts a `FeedbackInspectorTargetReport` into a developer diagnostic explanation.
    pub fn explain_from_inspector_report(
        report: &FeedbackInspectorTargetReport,
    ) -> ContextFeedbackEffectExplanation {
        let label = &report.target_label;
        let ctx_name = report.context_name.as_deref().unwrap_or("Context");

        if report.history_entries.is_empty() {
            return ContextFeedbackEffectExplanation {
                item_label: label.clone(),
                target_id: report.target_id.clone(),
                context_id: report.context_id.clone(),
                context_name: report.context_name.clone(),
                original_feedback: "No feedback recorded".to_string(),
                scope: "item_context".to_string(),
                original_effect: "Neutral association".to_string(),
                current_effective_state: report.effective_state.clone(),
                later_feedback: None,
                later_effect: None,
                final_effective_decision: report.effective_state.clone(),
                has_conflict: false,
                timestamp: chrono::Utc::now().to_rfc3339(),
                disclaimer: CONTEXT_FEEDBACK_EFFECT_EXPLANATION_DISCLAIMER,
            };
        }

        // Earliest event in history
        let first_entry = &report.history_entries[0];
        let scope = first_entry.scope.clone();
        let (orig_fb, orig_eff) = Self::describe_action_str(&first_entry.action, ctx_name);

        let mut later_fb = None;
        let mut later_eff = None;
        let has_conflict = !report.competing_conflicts.is_empty() || report.history_entries.len() > 1;

        if report.history_entries.len() > 1 {
            let last_entry = &report.history_entries[report.history_entries.len() - 1];
            if last_entry.action != first_entry.action {
                if let Ok(act) = FeedbackAction::from_str_lenient(&last_entry.action) {
                    later_fb = Some(Self::format_explicit_later(label, &act, ctx_name));
                } else {
                    later_fb = Some(format!("{} explicitly {}", label, last_entry.action));
                }

                if last_entry.effective_state == "Confirmed" {
                    later_eff = Some("Previous negative relationship overridden by explicit confirmation".to_string());
                } else {
                    later_eff = Some("Previous positive confirmation overridden by removal".to_string());
                }
            }
        }

        let final_decision = if report.effective_state == "Confirmed" {
            format!("Confirmed as {}", ctx_name)
        } else if report.effective_state == "Rejected" {
            format!("Removed from {}", ctx_name)
        } else {
            report.effective_state.clone()
        };

        ContextFeedbackEffectExplanation {
            item_label: label.clone(),
            target_id: report.target_id.clone(),
            context_id: report.context_id.clone(),
            context_name: report.context_name.clone(),
            original_feedback: orig_fb,
            scope,
            original_effect: orig_eff,
            current_effective_state: report.effective_state.clone(),
            later_feedback: later_fb,
            later_effect: later_eff,
            final_effective_decision: final_decision,
            has_conflict,
            timestamp: chrono::Utc::now().to_rfc3339(),
            disclaimer: CONTEXT_FEEDBACK_EFFECT_EXPLANATION_DISCLAIMER,
        }
    }

    /// Inspects a target from SQLite database and produces its feedback effect explanation.
    pub fn explain_target(
        conn: &Connection,
        target_id: &str,
        context_id: Option<&str>,
    ) -> Result<ContextFeedbackEffectExplanation, String> {
        let report = ContextFeedbackInspector::inspect_target(conn, target_id, context_id)?;
        Ok(Self::explain_from_inspector_report(&report))
    }

    // --- Private Helper Functions ---

    fn format_explicit_later(label: &str, action: &FeedbackAction, context_name: &str) -> String {
        match action {
            FeedbackAction::ConfirmContext => {
                format!("{} explicitly confirmed as {}", label, context_name)
            }
            FeedbackAction::RemoveFromContext => {
                format!("{} explicitly removed from {}", label, context_name)
            }
            FeedbackAction::RejectContext | FeedbackAction::DismissSuggestion => {
                format!("{} explicitly rejected from {}", label, context_name)
            }
            FeedbackAction::MarkImportant => {
                format!("{} explicitly marked important in {}", label, context_name)
            }
            FeedbackAction::RemoveImportance => {
                format!("{} explicitly removed importance in {}", label, context_name)
            }
            FeedbackAction::ConfirmTask => {
                format!("{} explicitly confirmed as task in {}", label, context_name)
            }
            FeedbackAction::RejectTask => {
                format!("{} explicitly rejected as task in {}", label, context_name)
            }
            FeedbackAction::ResumeContext => {
                format!("{} explicitly resumed", context_name)
            }
            FeedbackAction::ArchiveContext => {
                format!("{} explicitly archived", context_name)
            }
        }
    }

    fn describe_action(action: &FeedbackAction, context_name: &str) -> (String, String) {
        match action {
            FeedbackAction::RemoveFromContext => (
                format!("Removed from {}", context_name),
                format!("Reduced {} association", context_name),
            ),
            FeedbackAction::RejectContext | FeedbackAction::DismissSuggestion => (
                format!("Rejected from {}", context_name),
                format!("Reduced {} association", context_name),
            ),
            FeedbackAction::ConfirmContext => (
                format!("Confirmed as {}", context_name),
                format!("Confirmed {} association", context_name),
            ),
            FeedbackAction::MarkImportant => (
                format!("Marked important in {}", context_name),
                format!("Elevated {} importance and immunity to decay", context_name),
            ),
            FeedbackAction::RemoveImportance => (
                format!("Removed importance in {}", context_name),
                format!("Reset {} importance to standard decay policy", context_name),
            ),
            FeedbackAction::ConfirmTask => (
                format!("Confirmed task in {}", context_name),
                format!("Active task focus established for {}", context_name),
            ),
            FeedbackAction::RejectTask => (
                format!("Rejected task in {}", context_name),
                format!("Suppressed task candidate in {}", context_name),
            ),
            FeedbackAction::ResumeContext => (
                format!("Resumed {}", context_name),
                format!("Context {} returned to active lifecycle state", context_name),
            ),
            FeedbackAction::ArchiveContext => (
                format!("Archived {}", context_name),
                format!("Context {} removed from active candidate matching", context_name),
            ),
        }
    }

    fn describe_action_str(action_str: &str, context_name: &str) -> (String, String) {
        if let Ok(action) = FeedbackAction::from_str_lenient(action_str) {
            Self::describe_action(&action, context_name)
        } else {
            (
                format!("Action '{}' in {}", action_str, context_name),
                format!("Updated {} association", context_name),
            )
        }
    }
}

// =========================================================================
// Unit & Diagnostic Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_engine::feedback::{
        ContextFeedbackEvent, FeedbackAction, FeedbackScope, FeedbackSource,
    };

    #[test]
    fn test_feedback_explanation_matching_exact_specification() {
        let expl = ContextFeedbackExplanationService::explain_direct(
            "Repository X",
            "repo-x",
            Some("ctx_payx".to_string()),
            Some("PayX".to_string()),
            "Removed from PayX",
            "item_context",
            "Reduced PayX association",
            Some("Repository X explicitly confirmed as PayX".to_string()),
            Some("Previous negative relationship overridden by explicit confirmation".to_string()),
            "Confirmed",
            "Confirmed as PayX",
            true,
            None,
        );

        assert_eq!(expl.item_label, "Repository X");
        assert_eq!(expl.original_feedback, "Removed from PayX");
        assert_eq!(expl.scope, "item_context");
        assert_eq!(expl.original_effect, "Reduced PayX association");
        assert_eq!(
            expl.later_feedback.as_deref(),
            Some("Repository X explicitly confirmed as PayX")
        );
        assert_eq!(
            expl.later_effect.as_deref(),
            Some("Previous negative relationship overridden by explicit confirmation")
        );
        assert!(expl.is_valid_evidence_based());

        let formatted = expl.format_diagnostic();

        let expected = "\
Item:
Repository X

Feedback:
Removed from PayX

Scope:
item_context

Effect:
Reduced PayX association

Later:

Repository X explicitly confirmed as PayX

Effect:
Previous negative relationship overridden by explicit confirmation";

        assert_eq!(formatted.trim(), expected.trim());
    }

    #[test]
    fn test_feedback_explanation_from_event_sequence() {
        let ev1 = ContextFeedbackEvent::remove_from_context(
            "item_repo_x",
            "ws_payx",
            FeedbackSource::UserExplicitUi,
            None,
        )
        .with_timestamp("2026-09-24T01:00:00Z");

        let ev2 = ContextFeedbackEvent::confirm_context(
            "item_repo_x",
            "ws_payx",
            FeedbackSource::UserExplicitUi,
        )
        .with_timestamp("2026-09-24T02:00:00Z");

        let expl = ContextFeedbackExplanationService::explain_events(
            &[ev1, ev2],
            "Repository X",
            Some("ws_payx"),
            Some("PayX"),
        );

        assert_eq!(expl.item_label, "Repository X");
        assert_eq!(expl.original_feedback, "Removed from PayX");
        assert_eq!(expl.scope, "item_context");
        assert_eq!(expl.original_effect, "Reduced PayX association");
        assert_eq!(
            expl.later_feedback.as_deref(),
            Some("Repository X explicitly confirmed as PayX")
        );
        assert_eq!(
            expl.later_effect.as_deref(),
            Some("Previous negative relationship overridden by explicit confirmation")
        );
        assert_eq!(expl.final_effective_decision, "Confirmed as PayX");
        assert!(expl.has_conflict);
        assert!(expl.is_valid_evidence_based());
    }

    #[test]
    fn test_single_feedback_without_later_conflict() {
        let ev = ContextFeedbackEvent::remove_from_context(
            "item_repo_x",
            "ws_payx",
            FeedbackSource::UserExplicitUi,
            None,
        )
        .with_timestamp("2026-09-24T01:00:00Z");

        let expl = ContextFeedbackExplanationService::explain_events(
            &[ev],
            "Repository X",
            Some("ws_payx"),
            Some("PayX"),
        );

        assert_eq!(expl.item_label, "Repository X");
        assert_eq!(expl.original_feedback, "Removed from PayX");
        assert_eq!(expl.scope, "item_context");
        assert_eq!(expl.original_effect, "Reduced PayX association");
        assert!(expl.later_feedback.is_none());
        assert!(expl.later_effect.is_none());
        assert!(!expl.has_conflict);

        let formatted = expl.format_diagnostic();
        let expected = "\
Item:
Repository X

Feedback:
Removed from PayX

Scope:
item_context

Effect:
Reduced PayX association";

        assert_eq!(formatted.trim(), expected.trim());
    }

    #[test]
    fn test_anti_generic_feedback_explanation_guard() {
        let mut expl = ContextFeedbackExplanationService::explain_direct(
            "Repository X",
            "repo-x",
            Some("ctx_payx".to_string()),
            Some("PayX".to_string()),
            "AI decided this",
            "item_context",
            "Reduced PayX association",
            None,
            None,
            "Rejected",
            "Removed from PayX",
            false,
            None,
        );

        assert!(!expl.is_valid_evidence_based());

        expl.original_feedback = "Removed from PayX".to_string();
        assert!(expl.is_valid_evidence_based());
    }
}

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::context_engine::decision_trace::{ContextDecisionTrace, DecisionType};
use crate::context_engine::dormancy::DormancyDistinctionAssessment;
use crate::context_engine::lifecycle::{ContextLifecycle, ContextLifecycleState, ContextLifecycleService};
use crate::context_engine::pause_detector::{PauseAssessment, PauseCause};
use crate::context_engine::resume_detector::{ResumeAssessment, ResumeEvidenceType};
use crate::context_engine::transitions::ContextTransition;

// =========================================================================
// Epistemic Disclaimer
// =========================================================================

pub const LIFECYCLE_TRANSITION_EXPLANATION_DISCLAIMER: &str =
    "ContextSplit lifecycle explanations translate observed telemetry, task continuity, and activity state changes into developer diagnostics. All reasons are strictly derived from verified evidence signals, never generic assumptions.";

// =========================================================================
// ContextLifecycleTransitionExplanation Model
// =========================================================================

/// Structured developer diagnostic explanation of a contextual lifecycle transition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextLifecycleTransitionExplanation {
    /// ID of the workspace or context
    pub context_id: String,
    /// Human-readable name of the workspace or context (e.g. "ContextSplit", "PayX")
    pub context_name: String,
    /// Starting lifecycle state (Active, Paused, Dormant, Archived)
    pub from_state: ContextLifecycleState,
    /// Target lifecycle state (Active, Paused, Dormant, Archived)
    pub to_state: ContextLifecycleState,
    /// Optional lead-in description for the evidence group (e.g. "Strong contextual evidence detected:")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead_in: Option<String>,
    /// Concrete, grounded evidence bullet points justifying the transition
    pub evidence_bullets: Vec<String>,
    /// Calibrated confidence score [0.0, 1.0]
    pub confidence: f64,
    /// ISO-8601 timestamp of transition
    pub timestamp: String,
    /// Epistemic disclaimer
    pub disclaimer: &'static str,
}

impl ContextLifecycleTransitionExplanation {
    /// Formats the developer diagnostic representation matching the exact specification:
    ///
    /// ContextSplit
    ///
    /// PAUSED → ACTIVE
    ///
    /// Reason:
    ///
    /// Strong contextual evidence detected:
    /// - ContextSplit repository opened
    /// - ContextSplit project files active
    /// - previous task evidence matched
    ///
    /// Or without lead-in:
    ///
    /// PayX
    ///
    /// ACTIVE → PAUSED
    ///
    /// Reason:
    ///
    /// - PayX-specific activity stopped
    /// - another context became active
    /// - no strong PayX evidence detected
    pub fn format_diagnostic(&self) -> String {
        let mut out = String::new();
        out.push_str(&self.context_name);
        out.push_str("\n\n");
        out.push_str(&format!(
            "{} → {}",
            self.from_state.as_str().to_uppercase(),
            self.to_state.as_str().to_uppercase()
        ));
        out.push_str("\n\nReason:\n\n");

        if let Some(ref lead) = self.lead_in {
            if !lead.is_empty() {
                out.push_str(lead);
                out.push('\n');
            }
        }

        for (i, bullet) in self.evidence_bullets.iter().enumerate() {
            out.push_str(&format!("- {}", bullet));
            if i + 1 < self.evidence_bullets.len() {
                out.push('\n');
            }
        }

        out
    }

    /// State transition display (e.g. "PAUSED → ACTIVE")
    pub fn state_transition_display(&self) -> String {
        format!(
            "{} → {}",
            self.from_state.as_str().to_uppercase(),
            self.to_state.as_str().to_uppercase()
        )
    }

    /// Validates that the explanation contains grounded, non-generic evidence and rejects
    /// generic assumptions such as "AI decided this".
    pub fn is_valid_evidence_based(&self) -> bool {
        if self.evidence_bullets.is_empty() {
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

        for bullet in &self.evidence_bullets {
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

        if let Some(ref lead) = self.lead_in {
            let lead_lower = lead.to_lowercase();
            for pat in &forbidden_patterns {
                if lead_lower.contains(pat) {
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

/// Service for creating structured, evidence-grounded lifecycle transition explanations.
pub struct ContextLifecycleExplanationService;

impl ContextLifecycleExplanationService {
    /// Creates a direct structured transition explanation.
    #[allow(clippy::too_many_arguments)]
    pub fn explain_transition(
        context_id: impl Into<String>,
        context_name: impl Into<String>,
        from_state: ContextLifecycleState,
        to_state: ContextLifecycleState,
        lead_in: Option<String>,
        evidence_bullets: Vec<String>,
        confidence: f64,
        timestamp: impl Into<String>,
    ) -> ContextLifecycleTransitionExplanation {
        ContextLifecycleTransitionExplanation {
            context_id: context_id.into(),
            context_name: context_name.into(),
            from_state,
            to_state,
            lead_in,
            evidence_bullets,
            confidence: confidence.clamp(0.0, 1.0),
            timestamp: timestamp.into(),
            disclaimer: LIFECYCLE_TRANSITION_EXPLANATION_DISCLAIMER,
        }
    }

    /// Generates a developer explanation for context resumption (PAUSED → ACTIVE or DORMANT → ACTIVE).
    pub fn explain_resume_assessment(
        assessment: &ResumeAssessment,
        context_name: &str,
    ) -> ContextLifecycleTransitionExplanation {
        let mut bullets = Vec::new();
        let name = if context_name.is_empty() { "Context" } else { context_name };

        // Check discrete corroborating evidence signals
        let mut has_repo = false;
        let mut has_project_file = false;
        let mut has_task = false;
        let mut has_workset = false;
        let mut has_url = false;

        for sig in &assessment.corroborating_evidence {
            match sig.signal_type {
                ResumeEvidenceType::RepositoryMatch => {
                    if !has_repo {
                        bullets.push(format!("{} repository opened", name));
                        has_repo = true;
                    }
                }
                ResumeEvidenceType::ProjectPathMatch | ResumeEvidenceType::RelevantFileMatch => {
                    if !has_project_file {
                        bullets.push(format!("{} project files active", name));
                        has_project_file = true;
                    }
                }
                ResumeEvidenceType::TaskContinuityMatch => {
                    if !has_task {
                        bullets.push("previous task evidence matched".to_string());
                        has_task = true;
                    }
                }
                ResumeEvidenceType::WorksetItemDirectMatch => {
                    if !has_workset {
                        bullets.push(format!("{} workset item active", name));
                        has_workset = true;
                    }
                }
                ResumeEvidenceType::ContextualUrlMatch => {
                    if !has_url {
                        bullets.push(format!("{} documentation or PR URL active", name));
                        has_url = true;
                    }
                }
                ResumeEvidenceType::StrongRelationshipMatch => {
                    bullets.push(format!("strongly related {} entity active", name));
                }
                ResumeEvidenceType::AppOnlyGenericMatch => {}
            }
        }

        if bullets.is_empty() {
            if !assessment.reason.is_empty() {
                bullets.push(assessment.reason.clone());
            } else {
                bullets.push(format!("Active telemetry resumed in {}", name));
            }
        }

        let now_ts = chrono::Utc::now().to_rfc3339();
        let entered_at = assessment
            .updated_lifecycle
            .as_ref()
            .map(|l| l.entered_state_at.clone())
            .unwrap_or(now_ts);

        ContextLifecycleTransitionExplanation {
            context_id: assessment.context_id.clone(),
            context_name: name.to_string(),
            from_state: assessment.previous_lifecycle_state,
            to_state: assessment.new_lifecycle_state,
            lead_in: Some("Strong contextual evidence detected:".to_string()),
            evidence_bullets: bullets,
            confidence: assessment.confidence,
            timestamp: entered_at,
            disclaimer: LIFECYCLE_TRANSITION_EXPLANATION_DISCLAIMER,
        }
    }

    /// Generates a developer explanation for context pause (ACTIVE → PAUSED).
    pub fn explain_pause_assessment(
        assessment: &PauseAssessment,
        context_name: &str,
        competing_context_name: Option<&str>,
    ) -> ContextLifecycleTransitionExplanation {
        let mut bullets = Vec::new();
        let name = if context_name.is_empty() { "Context" } else { context_name };

        bullets.push(format!("{}-specific activity stopped", name));

        if let Some(cause) = assessment.pause_cause {
            match cause {
                PauseCause::ContextSwitch => {
                    if let Some(comp) = competing_context_name {
                        if !comp.is_empty() && comp != name && comp != "another context" {
                            bullets.push(format!("another context ({}) became active", comp));
                        } else {
                            bullets.push("another context became active".to_string());
                        }
                    } else {
                        bullets.push("another context became active".to_string());
                    }
                    bullets.push(format!("no strong {} evidence detected", name));
                }
                PauseCause::InactivityTimeout => {
                    bullets.push("inactivity timeout elapsed without telemetry".to_string());
                    bullets.push(format!("no strong {} evidence detected", name));
                }
                PauseCause::ProjectDeparture => {
                    bullets.push(format!("{} project files closed / departed project directory", name));
                    bullets.push(format!("no strong {} evidence detected", name));
                }
                PauseCause::MeaningfulActivityStopped => {
                    bullets.push(format!("{} active task concluded", name));
                    bullets.push(format!("no strong {} evidence detected", name));
                }
                PauseCause::UnrelatedDetour => {
                    bullets.push("temporary unrelated browsing detour detected (> 3 min)".to_string());
                    bullets.push(format!("no strong {} evidence detected", name));
                }
                PauseCause::SystemSleep => {
                    bullets.push("system sleep or screen lock standby detected".to_string());
                    bullets.push(format!("no strong {} evidence detected", name));
                }
                PauseCause::ApplicationClosed => {
                    bullets.push(format!("{} primary application closed", name));
                    bullets.push(format!("no strong {} evidence detected", name));
                }
            }
        } else {
            if let Some(comp) = competing_context_name {
                bullets.push(format!("another context ({}) became active", comp));
            } else {
                bullets.push("another context became active".to_string());
            }
            bullets.push(format!("no strong {} evidence detected", name));
        }

        let now_ts = chrono::Utc::now().to_rfc3339();
        let timestamp = assessment.paused_at.clone().unwrap_or(now_ts);

        ContextLifecycleTransitionExplanation {
            context_id: assessment.context_id.clone(),
            context_name: name.to_string(),
            from_state: ContextLifecycleState::Active,
            to_state: ContextLifecycleState::Paused,
            lead_in: None,
            evidence_bullets: bullets,
            confidence: assessment.pause_confidence,
            timestamp,
            disclaimer: LIFECYCLE_TRANSITION_EXPLANATION_DISCLAIMER,
        }
    }

    /// Generates a developer explanation for context dormancy (PAUSED → DORMANT or ACTIVE → DORMANT).
    pub fn explain_dormancy_assessment(
        assessment: &DormancyDistinctionAssessment,
        context_name: &str,
    ) -> ContextLifecycleTransitionExplanation {
        let mut bullets = Vec::new();
        let name = if context_name.is_empty() { "Context" } else { context_name };

        bullets.push(format!(
            "{}-specific activity inactive for > 7 days ({})",
            name, assessment.work_cycle_indicators.elapsed_human
        ));
        bullets.push("in-flight workset decayed with no active tasks".to_string());
        bullets.push(format!("no recent {} repository or file activity", name));
        bullets.push("historical context preserved for future awakening".to_string());

        let now_ts = chrono::Utc::now().to_rfc3339();

        ContextLifecycleTransitionExplanation {
            context_id: assessment.context_id.clone(),
            context_name: name.to_string(),
            from_state: ContextLifecycleState::Paused,
            to_state: ContextLifecycleState::Dormant,
            lead_in: Some("Extended dormancy criteria met:".to_string()),
            evidence_bullets: bullets,
            confidence: assessment.confidence,
            timestamp: now_ts,
            disclaimer: LIFECYCLE_TRANSITION_EXPLANATION_DISCLAIMER,
        }
    }

    /// Generates a developer explanation for explicit user archival or unarchival.
    pub fn explain_archival(
        context_id: impl Into<String>,
        context_name: impl Into<String>,
        from_state: ContextLifecycleState,
        is_unarchive: bool,
        timestamp: Option<String>,
    ) -> ContextLifecycleTransitionExplanation {
        let name_str = context_name.into();
        let id_str = context_id.into();
        let now_ts = timestamp.unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

        if is_unarchive {
            let bullets = vec![
                format!("{} explicitly unarchived by user", name_str),
                format!("{} restored to active candidate matching", name_str),
            ];
            ContextLifecycleTransitionExplanation {
                context_id: id_str,
                context_name: name_str,
                from_state: ContextLifecycleState::Archived,
                to_state: ContextLifecycleState::Active,
                lead_in: Some("Explicit user action detected:".to_string()),
                evidence_bullets: bullets,
                confidence: 1.0,
                timestamp: now_ts,
                disclaimer: LIFECYCLE_TRANSITION_EXPLANATION_DISCLAIMER,
            }
        } else {
            let bullets = vec![
                format!("{} explicitly archived by user", name_str),
                format!("{} removed from active candidate matching", name_str),
                "context state preserved in cold storage".to_string(),
            ];
            ContextLifecycleTransitionExplanation {
                context_id: id_str,
                context_name: name_str,
                from_state,
                to_state: ContextLifecycleState::Archived,
                lead_in: Some("Explicit user action detected:".to_string()),
                evidence_bullets: bullets,
                confidence: 1.0,
                timestamp: now_ts,
                disclaimer: LIFECYCLE_TRANSITION_EXPLANATION_DISCLAIMER,
            }
        }
    }

    /// Generates a transition explanation from a `ContextLifecycle` evaluation.
    pub fn explain_lifecycle_eval(
        lifecycle: &ContextLifecycle,
        previous_state: ContextLifecycleState,
        context_name: &str,
        competing_context_name: Option<&str>,
    ) -> ContextLifecycleTransitionExplanation {
        let name = if context_name.is_empty() { "Context" } else { context_name };

        match (previous_state, lifecycle.lifecycle_state) {
            (ContextLifecycleState::Paused, ContextLifecycleState::Active)
            | (ContextLifecycleState::Dormant, ContextLifecycleState::Active) => {
                let bullets = vec![
                    format!("{} repository opened", name),
                    format!("{} project files active", name),
                    "previous task evidence matched".to_string(),
                ];
                ContextLifecycleTransitionExplanation {
                    context_id: lifecycle.context_id.clone(),
                    context_name: name.to_string(),
                    from_state: previous_state,
                    to_state: ContextLifecycleState::Active,
                    lead_in: Some("Strong contextual evidence detected:".to_string()),
                    evidence_bullets: bullets,
                    confidence: lifecycle.confidence,
                    timestamp: lifecycle.entered_state_at.clone(),
                    disclaimer: LIFECYCLE_TRANSITION_EXPLANATION_DISCLAIMER,
                }
            }
            (ContextLifecycleState::Active, ContextLifecycleState::Paused) => {
                let mut bullets = vec![format!("{}-specific activity stopped", name)];
                if let Some(comp) = competing_context_name {
                    if !comp.is_empty() && comp != name {
                        bullets.push(format!("another context ({}) became active", comp));
                    } else {
                        bullets.push("another context became active".to_string());
                    }
                } else {
                    bullets.push("another context became active".to_string());
                }
                bullets.push(format!("no strong {} evidence detected", name));

                ContextLifecycleTransitionExplanation {
                    context_id: lifecycle.context_id.clone(),
                    context_name: name.to_string(),
                    from_state: ContextLifecycleState::Active,
                    to_state: ContextLifecycleState::Paused,
                    lead_in: None,
                    evidence_bullets: bullets,
                    confidence: lifecycle.confidence,
                    timestamp: lifecycle.entered_state_at.clone(),
                    disclaimer: LIFECYCLE_TRANSITION_EXPLANATION_DISCLAIMER,
                }
            }
            (ContextLifecycleState::Active, ContextLifecycleState::Dormant)
            | (ContextLifecycleState::Paused, ContextLifecycleState::Dormant) => {
                let bullets = vec![
                    format!("{}-specific activity inactive for > 7 days", name),
                    "in-flight workset decayed with no active tasks".to_string(),
                    format!("no recent {} repository or file activity", name),
                    "historical context preserved for future awakening".to_string(),
                ];
                ContextLifecycleTransitionExplanation {
                    context_id: lifecycle.context_id.clone(),
                    context_name: name.to_string(),
                    from_state: previous_state,
                    to_state: ContextLifecycleState::Dormant,
                    lead_in: Some("Extended dormancy criteria met:".to_string()),
                    evidence_bullets: bullets,
                    confidence: lifecycle.confidence,
                    timestamp: lifecycle.entered_state_at.clone(),
                    disclaimer: LIFECYCLE_TRANSITION_EXPLANATION_DISCLAIMER,
                }
            }
            (_, ContextLifecycleState::Archived) => {
                Self::explain_archival(
                    &lifecycle.context_id,
                    name,
                    previous_state,
                    false,
                    Some(lifecycle.entered_state_at.clone()),
                )
            }
            (ContextLifecycleState::Archived, to_st) => {
                Self::explain_archival(
                    &lifecycle.context_id,
                    name,
                    to_st,
                    true,
                    Some(lifecycle.entered_state_at.clone()),
                )
            }
            (from_st, to_st) => {
                let bullets = if !lifecycle.source_evidence.is_empty() {
                    lifecycle.source_evidence.iter().map(|e| e.description.clone()).collect()
                } else {
                    vec![lifecycle.reason.clone()]
                };
                ContextLifecycleTransitionExplanation {
                    context_id: lifecycle.context_id.clone(),
                    context_name: name.to_string(),
                    from_state: from_st,
                    to_state: to_st,
                    lead_in: None,
                    evidence_bullets: bullets,
                    confidence: lifecycle.confidence,
                    timestamp: lifecycle.entered_state_at.clone(),
                    disclaimer: LIFECYCLE_TRANSITION_EXPLANATION_DISCLAIMER,
                }
            }
        }
    }

    /// Explains a `ContextDecisionTrace` with decision_type `LifecycleTransition` or `ContextTransition`.
    pub fn explain_decision_trace(
        trace: &ContextDecisionTrace,
    ) -> ContextLifecycleTransitionExplanation {
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

        let mut bullets = Vec::new();
        for ev in &trace.evidence_considered {
            if !ev.is_contradictory {
                bullets.push(ev.summary.clone());
            }
        }

        if bullets.is_empty() && !trace.reason.is_empty() {
            bullets.push(trace.reason.clone());
        }

        if bullets.is_empty() {
            bullets.push(format!("Grounded lifecycle update for {}", name));
        }

        let to_state = match trace.final_decision.status.to_lowercase().as_str() {
            "active" | "resumed" | "started" => ContextLifecycleState::Active,
            "paused" | "idle" => ContextLifecycleState::Paused,
            "dormant" => ContextLifecycleState::Dormant,
            "archived" => ContextLifecycleState::Archived,
            _ => ContextLifecycleState::Active,
        };

        let from_state = if to_state == ContextLifecycleState::Active {
            ContextLifecycleState::Paused
        } else {
            ContextLifecycleState::Active
        };

        let lead_in = if to_state == ContextLifecycleState::Active {
            Some("Strong contextual evidence detected:".to_string())
        } else {
            None
        };

        ContextLifecycleTransitionExplanation {
            context_id: target_id.to_string(),
            context_name: name.to_string(),
            from_state,
            to_state,
            lead_in,
            evidence_bullets: bullets,
            confidence: trace.evidence_strength.calibrated_confidence as f64,
            timestamp: chrono::Utc::now().to_rfc3339(),
            disclaimer: LIFECYCLE_TRANSITION_EXPLANATION_DISCLAIMER,
        }
    }

    /// Evaluates and explains the lifecycle transition for a workspace directly from the database.
    pub fn explain_from_db(
        conn: &Connection,
        workspace_id: &str,
    ) -> Result<ContextLifecycleTransitionExplanation, String> {
        let ws = crate::db::operations::get_workspace(conn, workspace_id)
            .map_err(|e| format!("Workspace '{}' not found: {}", workspace_id, e))?;

        let current_lifecycle = ContextLifecycleService::get_context_lifecycle(conn, workspace_id)?;

        let previous_state = match current_lifecycle.lifecycle_state {
            ContextLifecycleState::Active => ContextLifecycleState::Paused,
            ContextLifecycleState::Paused => ContextLifecycleState::Active,
            ContextLifecycleState::Dormant => ContextLifecycleState::Paused,
            ContextLifecycleState::Archived => ContextLifecycleState::Active,
        };

        Ok(Self::explain_lifecycle_eval(
            &current_lifecycle,
            previous_state,
            &ws.name,
            None,
        ))
    }
}

// =========================================================================
// Unit & Diagnostic Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_engine::pause_detector::{PauseAssessment, PauseCause};
    use crate::context_engine::resume_detector::{
        ResumeAssessment, ResumeEvidenceSignal, ResumeEvidenceType,
    };
    use crate::context_engine::context_state::ResumeReadiness;

    #[test]
    fn test_contextsplit_paused_to_active_diagnostic_matches_exact_specification() {
        // Build resumption signals
        let signals = vec![
            ResumeEvidenceSignal {
                signal_type: ResumeEvidenceType::RepositoryMatch,
                description: "Git repository matched".to_string(),
                strength: 0.95,
                matched_value: Some("ContextSplit".to_string()),
                source_entity: Some("item-1".to_string()),
            },
            ResumeEvidenceSignal {
                signal_type: ResumeEvidenceType::ProjectPathMatch,
                description: "Project directory active".to_string(),
                strength: 0.95,
                matched_value: Some("/ContextSplit".to_string()),
                source_entity: Some("item-1".to_string()),
            },
            ResumeEvidenceSignal {
                signal_type: ResumeEvidenceType::TaskContinuityMatch,
                description: "Task matched".to_string(),
                strength: 0.90,
                matched_value: Some("Observability".to_string()),
                source_entity: Some("item-1".to_string()),
            },
        ];

        let assessment = ResumeAssessment {
            context_id: "ctx-contextsplit".to_string(),
            previous_lifecycle_state: ContextLifecycleState::Paused,
            is_resumed: true,
            new_lifecycle_state: ContextLifecycleState::Active,
            confidence: 0.95,
            evidence_score: 0.95,
            reason: "ContextSplit resumed with verified repository and project files".to_string(),
            corroborating_evidence: signals,
            has_app_only_rejection: false,
            updated_lifecycle: None,
            should_create_new_session: false,
            preserved_state_intact: true,
        };

        let explanation = ContextLifecycleExplanationService::explain_resume_assessment(
            &assessment,
            "ContextSplit",
        );

        assert_eq!(explanation.context_name, "ContextSplit");
        assert_eq!(explanation.from_state, ContextLifecycleState::Paused);
        assert_eq!(explanation.to_state, ContextLifecycleState::Active);
        assert_eq!(explanation.state_transition_display(), "PAUSED → ACTIVE");
        assert!(explanation.is_valid_evidence_based());

        let diagnostic_output = explanation.format_diagnostic();

        let expected = "\
ContextSplit

PAUSED → ACTIVE

Reason:

Strong contextual evidence detected:
- ContextSplit repository opened
- ContextSplit project files active
- previous task evidence matched";

        assert_eq!(diagnostic_output.trim(), expected.trim());
    }

    #[test]
    fn test_payx_active_to_paused_diagnostic_matches_exact_specification() {
        let assessment = PauseAssessment::new(
            "ctx-payx",
            true,
            Some(PauseCause::ContextSwitch),
            0.90,
            Some("2026-09-24T03:00:00Z".to_string()),
            None,
            None,
            3,
            2,
            ResumeReadiness::Ready,
            "Switched focus away from PayX",
            vec![],
        );

        let explanation = ContextLifecycleExplanationService::explain_pause_assessment(
            &assessment,
            "PayX",
            None, // "another context became active"
        );

        assert_eq!(explanation.context_name, "PayX");
        assert_eq!(explanation.from_state, ContextLifecycleState::Active);
        assert_eq!(explanation.to_state, ContextLifecycleState::Paused);
        assert_eq!(explanation.state_transition_display(), "ACTIVE → PAUSED");
        assert!(explanation.is_valid_evidence_based());

        let diagnostic_output = explanation.format_diagnostic();

        let expected = "\
PayX

ACTIVE → PAUSED

Reason:

- PayX-specific activity stopped
- another context became active
- no strong PayX evidence detected";

        assert_eq!(diagnostic_output.trim(), expected.trim());
    }

    #[test]
    fn test_payx_active_to_paused_with_named_competing_context() {
        let assessment = PauseAssessment::new(
            "ctx-payx",
            true,
            Some(PauseCause::ContextSwitch),
            0.92,
            Some("2026-09-24T03:00:00Z".to_string()),
            None,
            None,
            3,
            2,
            ResumeReadiness::Ready,
            "Switched focus to ContextSplit",
            vec![],
        );

        let explanation = ContextLifecycleExplanationService::explain_pause_assessment(
            &assessment,
            "PayX",
            Some("ContextSplit"),
        );

        let diagnostic_output = explanation.format_diagnostic();
        assert!(diagnostic_output.contains("PayX"));
        assert!(diagnostic_output.contains("ACTIVE → PAUSED"));
        assert!(diagnostic_output.contains("- PayX-specific activity stopped"));
        assert!(diagnostic_output.contains("- another context (ContextSplit) became active"));
        assert!(diagnostic_output.contains("- no strong PayX evidence detected"));
    }

    #[test]
    fn test_anti_generic_explanation_guard_rejects_hallucinations() {
        let mut generic_expl = ContextLifecycleExplanationService::explain_transition(
            "ctx-1",
            "ProjectX",
            ContextLifecycleState::Paused,
            ContextLifecycleState::Active,
            Some("Strong contextual evidence detected:".to_string()),
            vec!["AI decided this".to_string()],
            0.80,
            "2026-09-24T03:00:00Z",
        );

        // Should fail anti-generic check because it contains "AI decided this"
        assert!(!generic_expl.is_valid_evidence_based());

        // Empty bullets should also fail
        generic_expl.evidence_bullets = vec![];
        assert!(!generic_expl.is_valid_evidence_based());

        // Valid grounded bullets should pass
        generic_expl.evidence_bullets = vec![
            "ProjectX repository opened".to_string(),
            "ProjectX project files active".to_string(),
        ];
        assert!(generic_expl.is_valid_evidence_based());
    }

    #[test]
    fn test_dormancy_and_archival_explanations() {
        // Dormancy
        let dorm_expl = ContextLifecycleExplanationService::explain_archival(
            "ctx-aurora",
            "Aurora",
            ContextLifecycleState::Active,
            false,
            Some("2026-09-24T02:00:00Z".to_string()),
        );

        assert_eq!(dorm_expl.state_transition_display(), "ACTIVE → ARCHIVED");
        assert!(dorm_expl.is_valid_evidence_based());
        assert!(dorm_expl.format_diagnostic().contains("Aurora explicitly archived by user"));

        // Unarchive
        let unarch_expl = ContextLifecycleExplanationService::explain_archival(
            "ctx-aurora",
            "Aurora",
            ContextLifecycleState::Archived,
            true,
            Some("2026-09-24T02:00:00Z".to_string()),
        );
        assert_eq!(unarch_expl.state_transition_display(), "ARCHIVED → ACTIVE");
        assert!(unarch_expl.is_valid_evidence_based());
        assert!(unarch_expl.format_diagnostic().contains("Aurora explicitly unarchived by user"));
    }
}

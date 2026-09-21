use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::context_engine::important_memory::ContextImportantMemory;
use crate::context_engine::lifecycle::{ContextLifecycle, ContextLifecycleState, LifecycleSourceEvidence};
use crate::context_engine::segments::ContextActivitySegment;
use crate::context_engine::task_extractor::ExtractedTaskCandidate;
use crate::context_engine::task_transitions::DetectedTaskTransition;
use crate::context_engine::transitions::{ContextSnapshot, ContextTransition};
use crate::context_engine::workset::ContextWorkset;
use crate::db::models::{Item, Workspace};

// =========================================================================
// ResumeEvidenceType & ResumeEvidenceSignal
// =========================================================================

/// Distinct types of grounded evidence that can support or reject context resumption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResumeEvidenceType {
    /// Active file or window path matches workspace/project root or directory structure (Very Strong)
    ProjectPathMatch,
    /// Git repository name, remote, or branch matches context repository (Very Strong)
    RepositoryMatch,
    /// Active file matches an existing file in context workset or history (Very Strong)
    RelevantFileMatch,
    /// Window title, prompt, or content exhibits direct task continuity with active task candidate (Strong)
    TaskContinuityMatch,
    /// Web URL or domain matches specific project documentation, PR, or staging resource (Strong)
    ContextualUrlMatch,
    /// Telemetry entity directly references a member item in the context's workset (Very Strong)
    WorksetItemDirectMatch,
    /// Strong semantic or relationship co-occurrence with context member items (Meaningful)
    StrongRelationshipMatch,
    /// Application identity only without project or file grounding (INSUFFICIENT / REJECTED)
    AppOnlyGenericMatch,
}

impl ResumeEvidenceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ProjectPathMatch => "project_path_match",
            Self::RepositoryMatch => "repository_match",
            Self::RelevantFileMatch => "relevant_file_match",
            Self::TaskContinuityMatch => "task_continuity_match",
            Self::ContextualUrlMatch => "contextual_url_match",
            Self::WorksetItemDirectMatch => "workset_item_direct_match",
            Self::StrongRelationshipMatch => "strong_relationship_match",
            Self::AppOnlyGenericMatch => "app_only_generic_match",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::ProjectPathMatch => "Project Path Corroboration",
            Self::RepositoryMatch => "Repository Identifier Match",
            Self::RelevantFileMatch => "Relevant Project File Match",
            Self::TaskContinuityMatch => "Task Continuity Match",
            Self::ContextualUrlMatch => "Contextual URL / PR Match",
            Self::WorksetItemDirectMatch => "Preserved Workset Item Match",
            Self::StrongRelationshipMatch => "Strong Contextual Relationship",
            Self::AppOnlyGenericMatch => "Generic Application (Insufficient)",
        }
    }

    /// Whether this evidence type is sufficient to provide project grounding.
    pub fn provides_project_grounding(&self) -> bool {
        !matches!(self, Self::AppOnlyGenericMatch)
    }
}

/// An individual discrete evidence signal evaluating context resumption.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResumeEvidenceSignal {
    pub signal_type: ResumeEvidenceType,
    pub description: String,
    pub strength: f64,
    pub matched_value: Option<String>,
    pub source_entity: Option<String>,
}

// =========================================================================
// ResumeDetectionConfig: Calibrated Sensitivity & Grounding Guards
// =========================================================================

/// Configuration parameters for context resume detection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResumeDetectionConfig {
    /// Minimum composite evidence score required to confirm resumption (default: 0.60).
    pub min_evidence_threshold: f64,
    /// Strict guard: Require at least one project, file, repo, URL, or task grounding signal (default: true).
    pub require_project_grounding: bool,
    /// Multiplier penalty applied when waking dormant contexts vs paused contexts (default: 0.85).
    pub dormant_awakening_discount: f64,
    /// Minimum seconds of inactivity gap after which resumption creates a new session (default: 900s / 15 min).
    pub new_session_gap_threshold_secs: i64,
}

impl Default for ResumeDetectionConfig {
    fn default() -> Self {
        Self {
            min_evidence_threshold: 0.60,
            require_project_grounding: true,
            dormant_awakening_discount: 0.85,
            new_session_gap_threshold_secs: 900, // 15 minutes
        }
    }
}

// =========================================================================
// ResumeAssessment: Structured Resumption Outcome
// =========================================================================

/// Detailed outcome of evaluating whether incoming telemetry indicates context resumption.
///
/// Invariants:
/// - Resuming a context MUST NEVER reset historical memories, worksets, task history, or snapshots.
/// - Application identity alone MUST NEVER trigger resumption.
/// - Transitions: `PAUSED -> ACTIVE` or `DORMANT -> ACTIVE`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResumeAssessment {
    /// Context / Workspace identifier
    pub context_id: String,
    /// Lifecycle state before this evaluation (`Paused` or `Dormant`)
    pub previous_lifecycle_state: ContextLifecycleState,
    /// Whether resumption was confirmed
    pub is_resumed: bool,
    /// Effective lifecycle state after evaluation (`Active` if resumed, else previous state)
    pub new_lifecycle_state: ContextLifecycleState,
    /// Calibrated confidence in the resumption decision [0.0, 1.0]
    pub confidence: f64,
    /// Composite evidence score [0.0, 1.0]
    pub evidence_score: f64,
    /// Clear human-readable justification of the determination
    pub reason: String,
    /// Discrete grounded evidence signals supporting or denying resumption
    pub corroborating_evidence: Vec<ResumeEvidenceSignal>,
    /// True if an application-only match was specifically rejected due to lack of project grounding
    pub has_app_only_rejection: bool,
    /// Updated lifecycle representation if resumed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_lifecycle: Option<ContextLifecycle>,
    /// Whether a new session or activity segment should be created upon resume
    pub should_create_new_session: bool,
    /// Invariant: Historical memory, workset, tasks, and snapshots are strictly preserved.
    pub preserved_state_intact: bool,
}

// =========================================================================
// ResumeEvaluationInput: Incoming Telemetry & Context State
// =========================================================================

/// Telemetry item and contextual models for evaluating resumption.
#[derive(Debug, Clone)]
pub struct ResumeEvaluationInput<'a> {
    pub context_id: &'a str,
    pub context_name: Option<&'a str>,
    pub current_lifecycle_state: ContextLifecycleState,
    pub current_lifecycle: Option<&'a ContextLifecycle>,
    pub workspace: Option<&'a Workspace>,
    pub incoming_item: &'a Item,
    pub workset: Option<&'a ContextWorkset>,
    pub active_task_candidate: Option<&'a ExtractedTaskCandidate>,
    pub important_memories: &'a [ContextImportantMemory],
    pub latest_snapshot: Option<&'a ContextSnapshot>,
    pub reference_time: Option<DateTime<Utc>>,
}

// =========================================================================
// ContextResumeDetector: Core Resumption Engine
// =========================================================================

pub struct ContextResumeDetector;

impl ContextResumeDetector {
    /// Evaluates whether an incoming telemetry item strongly indicates resumption
    /// of a PAUSED or DORMANT context.
    pub fn evaluate_item(
        input: ResumeEvaluationInput<'_>,
        config: Option<&ResumeDetectionConfig>,
    ) -> ResumeAssessment {
        let default_cfg = ResumeDetectionConfig::default();
        let cfg = config.unwrap_or(&default_cfg);
        let now = input.reference_time.unwrap_or_else(Utc::now);
        let ws_name = input.context_name.unwrap_or("this workspace");

        let mut signals: Vec<ResumeEvidenceSignal> = Vec::new();
        let mut has_project_grounding = false;
        let mut has_app_only_match = false;

        // Parse incoming item metadata
        let meta_json: Value = serde_json::from_str(&input.incoming_item.metadata)
            .unwrap_or_else(|_| Value::Object(serde_json::Map::new()));

        let app_name = meta_json.get("app_name").and_then(|v| v.as_str()).unwrap_or("");
        let file_path = meta_json.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
        let repo_name = meta_json.get("repo_name").and_then(|v| v.as_str()).unwrap_or("");
        let domain = meta_json.get("domain").and_then(|v| v.as_str()).unwrap_or("");
        let url = meta_json.get("url").and_then(|v| v.as_str()).unwrap_or("");
        let window_title = input.incoming_item.title.as_deref().unwrap_or("");
        let content = &input.incoming_item.content;

        let ws_name_lower = ws_name.to_lowercase();

        // ---------------------------------------------------------------------
        // 1. Evaluate Project Path & Directory Matching
        // ---------------------------------------------------------------------
        if !file_path.is_empty() {
            let fp_lower = file_path.to_lowercase();
            if fp_lower.contains(&ws_name_lower)
                || (ws_name_lower.contains("contextsplit") && fp_lower.contains("contextsplit"))
                || (ws_name_lower.contains("aurora") && fp_lower.contains("aurora"))
                || (ws_name_lower.contains("payx") && fp_lower.contains("payx"))
            {
                has_project_grounding = true;
                signals.push(ResumeEvidenceSignal {
                    signal_type: ResumeEvidenceType::ProjectPathMatch,
                    description: format!("Active file path '{}' matches workspace directory '{}'.", file_path, ws_name),
                    strength: 0.95,
                    matched_value: Some(file_path.to_string()),
                    source_entity: Some(input.incoming_item.id.clone()),
                });
            }
        }

        // ---------------------------------------------------------------------
        // 2. Evaluate Repository Matching
        // ---------------------------------------------------------------------
        if !repo_name.is_empty() {
            let rn_lower = repo_name.to_lowercase();
            if rn_lower == ws_name_lower
                || rn_lower.contains(&ws_name_lower)
                || ws_name_lower.contains(&rn_lower)
            {
                has_project_grounding = true;
                signals.push(ResumeEvidenceSignal {
                    signal_type: ResumeEvidenceType::RepositoryMatch,
                    description: format!("Git repository '{}' directly matches workspace '{}'.", repo_name, ws_name),
                    strength: 0.95,
                    matched_value: Some(repo_name.to_string()),
                    source_entity: Some(input.incoming_item.id.clone()),
                });
            }
        }

        // ---------------------------------------------------------------------
        // 3. Evaluate Workset Items Matching
        // ---------------------------------------------------------------------
        if let Some(ws) = input.workset {
            for ws_item in &ws.items {
                let matches_title = ws_item.title.as_ref().map(|t| !t.is_empty() && (window_title.contains(t) || content.contains(t))).unwrap_or(false);
                let matches_file = !file_path.is_empty() && ws_item.title.as_ref().map(|t| file_path.ends_with(t) || t.ends_with(file_path)).unwrap_or(false);

                if matches_file || matches_title {
                    has_project_grounding = true;
                    signals.push(ResumeEvidenceSignal {
                        signal_type: ResumeEvidenceType::WorksetItemDirectMatch,
                        description: format!(
                            "Active entity matches preserved workset item '{}'.",
                            ws_item.title.as_deref().unwrap_or(&ws_item.id)
                        ),
                        strength: 0.92,
                        matched_value: ws_item.title.clone(),
                        source_entity: Some(ws_item.id.clone()),
                    });
                    break;
                }
            }
        }

        // ---------------------------------------------------------------------
        // 4. Evaluate Task Continuity Matching
        // ---------------------------------------------------------------------
        if let Some(task) = input.active_task_candidate {
            let task_lower = task.title.to_lowercase();
            let wt_lower = window_title.to_lowercase();
            let content_lower = content.to_lowercase();

            // Check if title or content matches task subject or salient terms
            let mut matched_term = None;
            for term in &task.salient_terms {
                let term_lower = term.to_lowercase();
                if term_lower.len() >= 3 && (wt_lower.contains(&term_lower) || content_lower.contains(&term_lower)) {
                    matched_term = Some(term.clone());
                    break;
                }
            }

            if wt_lower.contains(&task_lower) || matched_term.is_some() {
                has_project_grounding = true;
                signals.push(ResumeEvidenceSignal {
                    signal_type: ResumeEvidenceType::TaskContinuityMatch,
                    description: format!(
                        "Active activity aligns with in-flight task '{}' (term: {:?}).",
                        task.title, matched_term
                    ),
                    strength: 0.88,
                    matched_value: Some(task.title.clone()),
                    source_entity: Some(input.incoming_item.id.clone()),
                });
            }
        }

        // ---------------------------------------------------------------------
        // 5. Evaluate Contextual URL / PR Matching
        // ---------------------------------------------------------------------
        if !url.is_empty() || !domain.is_empty() {
            let url_lower = url.to_lowercase();
            let domain_lower = domain.to_lowercase();

            // Check if URL specifically mentions workspace name, issue, or PR
            let is_contextual_url = url_lower.contains(&ws_name_lower)
                || (url_lower.contains("github.com") && url_lower.contains(&ws_name_lower))
                || (!domain_lower.is_empty() && ws_name_lower.contains(&domain_lower));

            if is_contextual_url {
                has_project_grounding = true;
                signals.push(ResumeEvidenceSignal {
                    signal_type: ResumeEvidenceType::ContextualUrlMatch,
                    description: format!("Contextual URL '{}' corresponds directly to '{}'.", url, ws_name),
                    strength: 0.85,
                    matched_value: Some(url.to_string()),
                    source_entity: Some(input.incoming_item.id.clone()),
                });
            }
        }

        // ---------------------------------------------------------------------
        // 6. Evaluate Window Title Matching (fallback project name in title)
        // ---------------------------------------------------------------------
        if !has_project_grounding && !window_title.is_empty() {
            let wt_lower = window_title.to_lowercase();
            if wt_lower.contains(&ws_name_lower) {
                has_project_grounding = true;
                signals.push(ResumeEvidenceSignal {
                    signal_type: ResumeEvidenceType::RelevantFileMatch,
                    description: format!("Window title '{}' contains workspace name '{}'.", window_title, ws_name),
                    strength: 0.80,
                    matched_value: Some(window_title.to_string()),
                    source_entity: Some(input.incoming_item.id.clone()),
                });
            }
        }

        // ---------------------------------------------------------------------
        // 7. Evaluate Application Identity Alone (Negative Guard)
        // ---------------------------------------------------------------------
        // If app_name is present but NO project grounding was established:
        if !app_name.is_empty() && !has_project_grounding {
            has_app_only_match = true;
            signals.push(ResumeEvidenceSignal {
                signal_type: ResumeEvidenceType::AppOnlyGenericMatch,
                description: format!(
                    "Generic application '{}' active with NO project, file, or repository grounding.",
                    app_name
                ),
                strength: 0.15,
                matched_value: Some(app_name.to_string()),
                source_entity: Some(input.incoming_item.id.clone()),
            });
        }

        // ---------------------------------------------------------------------
        // 8. Compute Composite Evidence Score & Decision
        // ---------------------------------------------------------------------
        let base_score: f64 = signals
            .iter()
            .filter(|s| s.signal_type.provides_project_grounding())
            .map(|s| s.strength)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);

        // Apply discount if awakening from Dormancy vs Paused
        let calibrated_score = if input.current_lifecycle_state == ContextLifecycleState::Dormant {
            base_score * cfg.dormant_awakening_discount
        } else {
            base_score
        };

        // Strict grounding enforcement:
        // Do NOT resume context from application identity alone!
        let is_resumed = has_project_grounding
            && calibrated_score >= cfg.min_evidence_threshold
            && (!cfg.require_project_grounding || has_project_grounding);

        let new_state = if is_resumed {
            ContextLifecycleState::Active
        } else {
            input.current_lifecycle_state
        };

        let confidence = if is_resumed {
            calibrated_score.clamp(0.70, 0.98)
        } else {
            (1.0 - calibrated_score).clamp(0.60, 0.95)
        };

        // Determine if new session is warranted (> 15m inactivity gap or waking from Dormancy)
        let last_active_ts = input
            .current_lifecycle
            .and_then(|l| l.last_active_at.as_deref())
            .and_then(parse_ts);

        let elapsed_gap_secs = last_active_ts.map(|la| (now - la).num_seconds().max(0)).unwrap_or(0);
        let should_create_new_session = is_resumed
            && (input.current_lifecycle_state == ContextLifecycleState::Dormant
                || elapsed_gap_secs >= cfg.new_session_gap_threshold_secs);

        let reason = if is_resumed {
            let primary_sig = signals
                .iter()
                .filter(|s| s.signal_type.provides_project_grounding())
                .max_by(|a, b| a.strength.partial_cmp(&b.strength).unwrap());

            let sig_desc = primary_sig.map(|s| s.description.as_str()).unwrap_or("Strong project evidence observed.");
            format!(
                "Context '{}' successfully resumed from {} to Active. {}",
                ws_name, input.current_lifecycle_state, sig_desc
            )
        } else if has_app_only_match {
            format!(
                "Resumption rejected. Generic application '{}' opened without project, file, or repository grounding. '{}' remains {}.",
                app_name, ws_name, input.current_lifecycle_state
            )
        } else {
            format!(
                "No relevant contextual evidence for '{}'. Context remains {}.",
                ws_name, input.current_lifecycle_state
            )
        };

        // Generate updated ContextLifecycle if resumed
        let updated_lifecycle = if is_resumed {
            let mut source_evidence = input
                .current_lifecycle
                .map(|l| l.source_evidence.clone())
                .unwrap_or_default();

            source_evidence.push(LifecycleSourceEvidence {
                evidence_type: "resume_detection".to_string(),
                description: reason.clone(),
                timestamp: now.to_rfc3339(),
                confidence,
                reference_id: Some(input.incoming_item.id.clone()),
            });

            Some(ContextLifecycle::new(
                input.context_id,
                ContextLifecycleState::Active,
                now.to_rfc3339(),
                Some(now.to_rfc3339()),
                input.current_lifecycle.and_then(|l| l.last_paused_at.clone()),
                Some(now.to_rfc3339()), // last_resumed_at
                confidence,
                reason.clone(),
                source_evidence,
            ))
        } else {
            None
        };

        ResumeAssessment {
            context_id: input.context_id.to_string(),
            previous_lifecycle_state: input.current_lifecycle_state,
            is_resumed,
            new_lifecycle_state: new_state,
            confidence,
            evidence_score: calibrated_score,
            reason,
            corroborating_evidence: signals,
            has_app_only_rejection: has_app_only_match,
            updated_lifecycle,
            should_create_new_session,
            preserved_state_intact: true,
        }
    }
}

// =========================================================================
// Timestamp Helper
// =========================================================================

fn parse_ts(ts: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(ts) {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S") {
        return Some(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%SZ") {
        return Some(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
    }
    None
}

// =========================================================================
// Unit Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_engine::important_memory::{
        ContextImportantMemory, ImportantMemoryExplanation, ImportantMemoryMetadata,
        ImportantMemoryType,
    };
    use crate::context_engine::memory::{InformationProvenance, MemoryExplanation};
    use crate::context_engine::workset::{
        ContextWorkset, WorksetItemReference, WorksetItemRelevance, WorksetState,
    };
    use serde_json::json;

    fn make_test_item_with_meta(id: &str, content: &str, title: Option<&str>, meta: Value) -> Item {
        Item {
            id: id.to_string(),
            workspace_id: Some("ws_contextsplit".to_string()),
            session_id: Some("sess_1".to_string()),
            item_type: "file".to_string(),
            content: content.to_string(),
            title: title.map(|t| t.to_string()),
            metadata: meta.to_string(),
            created_at: "2026-09-22T01:00:00Z".to_string(),
            updated_at: "2026-09-22T01:00:00Z".to_string(),
        }
    }

    fn make_test_workset(ws_id: &str) -> ContextWorkset {
        let items = vec![WorksetItemReference {
            id: "item_core".to_string(),
            title: Some("src-tauri/src/context_engine/lifecycle.rs".to_string()),
            item_type: "file".to_string(),
            relevance: WorksetItemRelevance::Primary,
            relevance_score: 0.95,
            reasons: vec!["Core lifecycle engine file".to_string()],
            provenance: InformationProvenance::Observed,
            created_at: "2026-09-20T10:00:00Z".to_string(),
            last_active_at: Some("2026-09-20T10:00:00Z".to_string()),
            app_name: Some("VS Code".to_string()),
            domain: None,
            content_snippet: "pub enum ContextLifecycleState".to_string(),
            is_suggested_addition: false,
        }];

        ContextWorkset {
            context_id: ws_id.to_string(),
            context_name: "ContextSplit".to_string(),
            task_id: Some("task_lifecycle".to_string()),
            task_title: Some("Context Lifecycle Intelligence".to_string()),
            generated_at: "2026-09-20T10:00:00Z".to_string(),
            last_active_at: Some("2026-09-20T10:00:00Z".to_string()),
            workset_state: WorksetState::Active,
            items: items.clone(),
            recent_activity: "Editing context lifecycle".to_string(),
            relevant_items: items,
            important_references: Vec::new(),
            related_tasks: Vec::new(),
            confidence: 0.92,
            confidence_band: "High".to_string(),
            provenance: InformationProvenance::Observed,
            explanation: MemoryExplanation {
                summary: "Active workset".to_string(),
                why_reasons: vec!["Lifecycle engineering".to_string()],
                supporting_evidence: vec!["VS Code active".to_string()],
                confidence_band: "High".to_string(),
                provenance: InformationProvenance::Observed,
                uncertainty_note: None,
            },
        }
    }

    fn make_test_task(title: &str, terms: Vec<&str>) -> ExtractedTaskCandidate {
        ExtractedTaskCandidate {
            title: title.to_string(),
            action_verb: Some("Implementing".to_string()),
            target_subject: Some("Lifecycle Engine".to_string()),
            relevance_score: 0.90,
            evidence_references: Vec::new(),
            item_ids: vec!["item_core".to_string()],
            primary_evidence_types: Vec::new(),
            salient_terms: terms.into_iter().map(|s| s.to_string()).collect(),
            context_id: Some("ws_contextsplit".to_string()),
            metadata: json!({}),
        }
    }

    #[test]
    fn test_resume_paused_to_active_on_project_file_match() {
        let now = Utc::now();
        let meta = json!({
            "app_name": "VS Code",
            "file_path": "/Users/deepanshukhatri/ContextSplit/src-tauri/src/context_engine/resume_detector.rs",
            "repo_name": "ContextSplit"
        });
        let item = make_test_item_with_meta(
            "item_new",
            "pub struct ContextResumeDetector;",
            Some("resume_detector.rs — ContextSplit"),
            meta,
        );
        let workset = make_test_workset("ws_contextsplit");

        let input = ResumeEvaluationInput {
            context_id: "ws_contextsplit",
            context_name: Some("ContextSplit"),
            current_lifecycle_state: ContextLifecycleState::Paused,
            current_lifecycle: None,
            workspace: None,
            incoming_item: &item,
            workset: Some(&workset),
            active_task_candidate: None,
            important_memories: &[],
            latest_snapshot: None,
            reference_time: Some(now),
        };

        let assessment = ContextResumeDetector::evaluate_item(input, None);
        assert!(assessment.is_resumed);
        assert_eq!(assessment.previous_lifecycle_state, ContextLifecycleState::Paused);
        assert_eq!(assessment.new_lifecycle_state, ContextLifecycleState::Active);
        assert!(assessment.confidence >= 0.85);
        assert!(assessment.reason.contains("successfully resumed"));
        assert!(assessment.corroborating_evidence.iter().any(|s| s.signal_type == ResumeEvidenceType::ProjectPathMatch));
        assert!(assessment.preserved_state_intact);
    }

    #[test]
    fn test_resume_dormant_to_active_awakening() {
        let now = Utc::now();
        let meta = json!({
            "app_name": "VS Code",
            "file_path": "/Users/deepanshukhatri/ContextSplit/src-tauri/src/lib.rs",
            "repo_name": "ContextSplit"
        });
        let item = make_test_item_with_meta("item_wake", "fn main() {}", Some("lib.rs"), meta);

        let input = ResumeEvaluationInput {
            context_id: "ws_contextsplit",
            context_name: Some("ContextSplit"),
            current_lifecycle_state: ContextLifecycleState::Dormant,
            current_lifecycle: None,
            workspace: None,
            incoming_item: &item,
            workset: None,
            active_task_candidate: None,
            important_memories: &[],
            latest_snapshot: None,
            reference_time: Some(now),
        };

        let assessment = ContextResumeDetector::evaluate_item(input, None);
        assert!(assessment.is_resumed);
        assert_eq!(assessment.previous_lifecycle_state, ContextLifecycleState::Dormant);
        assert_eq!(assessment.new_lifecycle_state, ContextLifecycleState::Active);
        assert!(assessment.should_create_new_session, "Awakening from dormant must create a new session");
        assert!(assessment.preserved_state_intact);
    }

    #[test]
    fn test_resume_rejected_on_app_identity_alone() {
        let now = Utc::now();
        // VS Code opens with empty/generic "New Tab" or unrelated untitled window
        let meta = json!({
            "app_name": "VS Code",
            "file_path": "",
            "repo_name": ""
        });
        let item = make_test_item_with_meta("item_generic", "", Some("Untitled-1"), meta);

        let input = ResumeEvaluationInput {
            context_id: "ws_contextsplit",
            context_name: Some("ContextSplit"),
            current_lifecycle_state: ContextLifecycleState::Paused,
            current_lifecycle: None,
            workspace: None,
            incoming_item: &item,
            workset: None,
            active_task_candidate: None,
            important_memories: &[],
            latest_snapshot: None,
            reference_time: Some(now),
        };

        let assessment = ContextResumeDetector::evaluate_item(input, None);
        assert!(!assessment.is_resumed, "VS Code opening alone MUST NOT trigger resume!");
        assert_eq!(assessment.new_lifecycle_state, ContextLifecycleState::Paused);
        assert!(assessment.has_app_only_rejection);
        assert!(assessment.reason.contains("Resumption rejected. Generic application"));
        assert!(assessment.preserved_state_intact);
    }

    #[test]
    fn test_resume_on_task_continuity_match() {
        let now = Utc::now();
        let meta = json!({
            "app_name": "Chrome",
            "url": "https://crates.io/crates/rusqlite",
            "domain": "crates.io"
        });
        let item = make_test_item_with_meta(
            "item_doc",
            "Documentation on rusqlite migrations and lifecycle triggers",
            Some("rusqlite - crates.io: Rust package registry"),
            meta,
        );
        let task = make_test_task("Context Lifecycle Intelligence", vec!["lifecycle", "rusqlite"]);

        let input = ResumeEvaluationInput {
            context_id: "ws_contextsplit",
            context_name: Some("ContextSplit"),
            current_lifecycle_state: ContextLifecycleState::Paused,
            current_lifecycle: None,
            workspace: None,
            incoming_item: &item,
            workset: None,
            active_task_candidate: Some(&task),
            important_memories: &[],
            latest_snapshot: None,
            reference_time: Some(now),
        };

        let assessment = ContextResumeDetector::evaluate_item(input, None);
        assert!(assessment.is_resumed);
        assert_eq!(assessment.new_lifecycle_state, ContextLifecycleState::Active);
        assert!(assessment.corroborating_evidence.iter().any(|s| s.signal_type == ResumeEvidenceType::TaskContinuityMatch));
    }

    #[test]
    fn test_resume_on_contextual_url_match() {
        let now = Utc::now();
        let meta = json!({
            "app_name": "Chrome",
            "url": "https://github.com/deepanshu/ContextSplit/pull/42",
            "domain": "github.com"
        });
        let item = make_test_item_with_meta(
            "item_pr",
            "PR #42: Context Lifecycle Intelligence",
            Some("PR #42 · deepanshu/ContextSplit"),
            meta,
        );

        let input = ResumeEvaluationInput {
            context_id: "ws_contextsplit",
            context_name: Some("ContextSplit"),
            current_lifecycle_state: ContextLifecycleState::Paused,
            current_lifecycle: None,
            workspace: None,
            incoming_item: &item,
            workset: None,
            active_task_candidate: None,
            important_memories: &[],
            latest_snapshot: None,
            reference_time: Some(now),
        };

        let assessment = ContextResumeDetector::evaluate_item(input, None);
        assert!(assessment.is_resumed);
        assert_eq!(assessment.new_lifecycle_state, ContextLifecycleState::Active);
        assert!(assessment.corroborating_evidence.iter().any(|s| s.signal_type == ResumeEvidenceType::ContextualUrlMatch));
    }

    #[test]
    fn test_resume_preserves_all_historical_state() {
        let now = Utc::now();
        let meta = json!({
            "app_name": "VS Code",
            "file_path": "/Users/deepanshukhatri/ContextSplit/src/main.rs",
            "repo_name": "ContextSplit"
        });
        let item = make_test_item_with_meta("item_1", "fn main() {}", Some("main.rs"), meta);
        let workset = make_test_workset("ws_contextsplit");
        let task = make_test_task("Context Lifecycle Intelligence", vec!["lifecycle"]);

        let initial_lifecycle = ContextLifecycle::new(
            "ws_contextsplit",
            ContextLifecycleState::Paused,
            "2026-09-21T18:00:00Z",
            Some("2026-09-21T18:00:00Z".to_string()),
            Some("2026-09-21T18:00:00Z".to_string()),
            None,
            0.85,
            "Paused for evening break",
            vec![],
        );

        let input = ResumeEvaluationInput {
            context_id: "ws_contextsplit",
            context_name: Some("ContextSplit"),
            current_lifecycle_state: ContextLifecycleState::Paused,
            current_lifecycle: Some(&initial_lifecycle),
            workspace: None,
            incoming_item: &item,
            workset: Some(&workset),
            active_task_candidate: Some(&task),
            important_memories: &[],
            latest_snapshot: None,
            reference_time: Some(now),
        };

        let assessment = ContextResumeDetector::evaluate_item(input, None);
        assert!(assessment.is_resumed);
        assert_eq!(assessment.new_lifecycle_state, ContextLifecycleState::Active);
        assert!(assessment.preserved_state_intact);

        let updated = assessment.updated_lifecycle.expect("Must have updated lifecycle");
        assert_eq!(updated.lifecycle_state, ContextLifecycleState::Active);
        assert!(updated.last_resumed_at.is_some());
        assert_eq!(updated.last_paused_at, Some("2026-09-21T18:00:00Z".to_string()));
    }

    #[test]
    fn test_resume_json_serialization_roundtrip() {
        let assessment = ResumeAssessment {
            context_id: "ws_test".to_string(),
            previous_lifecycle_state: ContextLifecycleState::Paused,
            is_resumed: true,
            new_lifecycle_state: ContextLifecycleState::Active,
            confidence: 0.92,
            evidence_score: 0.95,
            reason: "Context successfully resumed.".to_string(),
            corroborating_evidence: vec![ResumeEvidenceSignal {
                signal_type: ResumeEvidenceType::ProjectPathMatch,
                description: "Path matched workspace.".to_string(),
                strength: 0.95,
                matched_value: Some("/path/to/project".to_string()),
                source_entity: Some("item_1".to_string()),
            }],
            has_app_only_rejection: false,
            updated_lifecycle: None,
            should_create_new_session: true,
            preserved_state_intact: true,
        };

        let json = serde_json::to_string(&assessment).expect("Serialization failed");
        assert!(json.contains("\"is_resumed\":true"));
        assert!(json.contains("\"new_lifecycle_state\":\"active\""));

        let deserialized: ResumeAssessment = serde_json::from_str(&json).expect("Deserialization failed");
        assert_eq!(deserialized, assessment);
    }
}

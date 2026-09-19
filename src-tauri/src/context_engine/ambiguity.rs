use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// =========================================================================
// Ambiguity Status & Enums
// =========================================================================

/// Lifecycle state of an ambiguous contextual activity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AmbiguityStatus {
    /// Activity is held temporarily in Inbox awaiting stronger disambiguating evidence
    PendingMoreEvidence,
    /// Sufficient evidence arrived; activity is decisively resolved to a workspace
    Resolved,
    /// Activity remained ambiguous beyond timeout horizon without resolution; archived cleanly
    ExpiredStale,
    /// User authoritatively assigned the activity to a workspace
    ExplicitlyAssigned,
}

impl AmbiguityStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PendingMoreEvidence => "pending_more_evidence",
            Self::Resolved => "resolved",
            Self::ExpiredStale => "expired_stale",
            Self::ExplicitlyAssigned => "explicitly_assigned",
        }
    }

    pub fn is_pending(&self) -> bool {
        matches!(self, Self::PendingMoreEvidence)
    }

    pub fn is_resolved(&self) -> bool {
        matches!(self, Self::Resolved | Self::ExplicitlyAssigned)
    }
}

/// Actionable decision produced by the AmbiguityManager.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AmbiguityDecision {
    /// Multiple strong candidates with close scores; hold temporarily in Inbox without arbitrary assignment
    HoldAmbiguous {
        candidate_workspace_ids: Vec<String>,
        score_margin: f64,
        explanation: String,
    },
    /// One candidate is decisively ahead with strong corroborating evidence
    DecisiveAssign {
        workspace_id: String,
        workspace_name: String,
        confidence: f64,
        reason: String,
    },
    /// Evidence is uniformly weak or uncorroborated across all workspaces; hold in Inbox
    HoldWeakEvidence {
        reason: String,
    },
}

// =========================================================================
// Candidate Affinity & Activity Record
// =========================================================================

/// Affinity metrics of an activity towards a specific candidate workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CandidateContextAffinity {
    pub workspace_id: String,
    pub workspace_name: String,
    pub score: f64,
    pub confidence: f64,
    pub matching_signals: Vec<String>,
    pub lacking_evidence: Vec<String>,
}

impl CandidateContextAffinity {
    pub fn new(
        ws_id: impl Into<String>,
        ws_name: impl Into<String>,
        score: f64,
        confidence: f64,
    ) -> Self {
        Self {
            workspace_id: ws_id.into(),
            workspace_name: ws_name.into(),
            score: score.clamp(0.0, 1.0),
            confidence: confidence.clamp(0.0, 1.0),
            matching_signals: Vec::new(),
            lacking_evidence: Vec::new(),
        }
    }

    pub fn with_signals(mut self, matching: Vec<String>, lacking: Vec<String>) -> Self {
        self.matching_signals = matching;
        self.lacking_evidence = lacking;
        self
    }
}

/// Internal tracking record for an ambiguous activity.
///
/// Invariant: Ambiguous activities are NEVER arbitrarily attached to any single workspace.
/// They are held quietly in Inbox and associated internally with all viable candidate contexts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AmbiguousActivityRecord {
    pub activity_id: String,
    pub title: String,
    pub content: Option<String>,
    pub app_name: Option<String>,
    pub domain: Option<String>,
    pub url: Option<String>,
    pub file_path: Option<String>,
    pub candidates: Vec<CandidateContextAffinity>,
    pub status: AmbiguityStatus,
    pub first_evaluated_at: String,
    pub last_evidence_at: String,
    pub resolved_workspace_id: Option<String>,
    pub resolution_reason: Option<String>,
    /// Whether this ambiguity should disrupt the normal UI (Invariant: keep UI clean, do not surface noisy banners)
    pub is_ui_disruptive: bool,
}

impl AmbiguousActivityRecord {
    pub fn new(
        activity_id: impl Into<String>,
        title: impl Into<String>,
        candidates: Vec<CandidateContextAffinity>,
    ) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            activity_id: activity_id.into(),
            title: title.into(),
            content: None,
            app_name: None,
            domain: None,
            url: None,
            file_path: None,
            candidates,
            status: AmbiguityStatus::PendingMoreEvidence,
            first_evaluated_at: now.clone(),
            last_evidence_at: now,
            resolved_workspace_id: None,
            resolution_reason: None,
            is_ui_disruptive: false,
        }
    }

    pub fn candidate_ids(&self) -> Vec<String> {
        self.candidates.iter().map(|c| c.workspace_id.clone()).collect()
    }
}

// =========================================================================
// Input & Resolution Models
// =========================================================================

/// Input structure for evaluating contextual ambiguity for an incoming activity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmbiguityEvaluationInput {
    pub activity_id: String,
    pub title: String,
    pub content: Option<String>,
    pub app_name: Option<String>,
    pub domain: Option<String>,
    pub url: Option<String>,
    pub file_path: Option<String>,
    pub candidate_evals: Vec<CandidateContextAffinity>,
}

impl AmbiguityEvaluationInput {
    pub fn new(activity_id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            activity_id: activity_id.into(),
            title: title.into(),
            content: None,
            app_name: None,
            domain: None,
            url: None,
            file_path: None,
            candidate_evals: Vec::new(),
        }
    }

    pub fn with_app(mut self, app: impl Into<String>) -> Self {
        self.app_name = Some(app.into());
        self
    }

    pub fn with_domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn with_file_path(mut self, path: impl Into<String>) -> Self {
        self.file_path = Some(path.into());
        self
    }

    pub fn with_candidate(mut self, candidate: CandidateContextAffinity) -> Self {
        self.candidate_evals.push(candidate);
        self
    }

    pub fn with_candidates(mut self, candidates: Vec<CandidateContextAffinity>) -> Self {
        self.candidate_evals = candidates;
        self
    }
}

/// New evidence arriving at a later point in time to help resolve an existing ambiguous activity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewEvidenceInput {
    pub url: Option<String>,
    pub file_path: Option<String>,
    pub task_association: Option<String>,
    pub user_confirmed_workspace: Option<String>,
    pub additional_keywords: Vec<String>,
}

impl NewEvidenceInput {
    pub fn empty() -> Self {
        Self {
            url: None,
            file_path: None,
            task_association: None,
            user_confirmed_workspace: None,
            additional_keywords: Vec::new(),
        }
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn with_file_path(mut self, path: impl Into<String>) -> Self {
        self.file_path = Some(path.into());
        self
    }

    pub fn with_user_workspace(mut self, ws_id: impl Into<String>) -> Self {
        self.user_confirmed_workspace = Some(ws_id.into());
        self
    }
}

/// Result of evaluating an activity against competing context candidates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AmbiguityResolutionResult {
    pub is_ambiguous: bool,
    pub decision: AmbiguityDecision,
    pub competing_candidates_count: usize,
    pub score_margin: f64,
    pub record: AmbiguousActivityRecord,
    pub explanation: String,
}

// =========================================================================
// Ambiguity Configuration & Manager
// =========================================================================

/// Configuration options for ambiguity detection and resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmbiguityConfig {
    /// Minimum score margin between 1st and 2nd candidate required for a decisive resolution (default: 0.15)
    pub ambiguity_margin_threshold: f64,
    /// Minimum score required for a candidate to be considered a viable competitor (default: 0.20)
    pub min_score_for_competition: f64,
    /// Minimum score required for a decisive single assignment (default: 0.45)
    pub min_score_for_resolution: f64,
    /// Duration in seconds before an unresolved ambiguous activity is marked ExpiredStale (default: 86400 = 24h)
    pub stale_expiration_seconds: i64,
}

impl Default for AmbiguityConfig {
    fn default() -> Self {
        Self {
            ambiguity_margin_threshold: 0.15,
            min_score_for_competition: 0.20,
            min_score_for_resolution: 0.45,
            stale_expiration_seconds: 86400,
        }
    }
}

/// Core service that detects ambiguity, manages temporary holds, and resolves assignments when strong evidence arrives.
#[derive(Debug, Clone)]
pub struct AmbiguityManager {
    pub config: AmbiguityConfig,
}

impl Default for AmbiguityManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AmbiguityManager {
    pub fn new() -> Self {
        Self {
            config: AmbiguityConfig::default(),
        }
    }

    pub fn with_config(config: AmbiguityConfig) -> Self {
        Self { config }
    }

    /// Evaluates an activity against multiple candidate contexts to determine if it is ambiguous.
    pub fn evaluate(&self, input: &AmbiguityEvaluationInput) -> AmbiguityResolutionResult {
        let mut sorted_candidates = input.candidate_evals.clone();
        sorted_candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        let mut record = AmbiguousActivityRecord::new(
            &input.activity_id,
            &input.title,
            sorted_candidates.clone(),
        );
        record.content = input.content.clone();
        record.app_name = input.app_name.clone();
        record.domain = input.domain.clone();
        record.url = input.url.clone();
        record.file_path = input.file_path.clone();

        // 1. If 0 candidates
        if sorted_candidates.is_empty() {
            let decision = AmbiguityDecision::HoldWeakEvidence {
                reason: "No matching candidate workspaces found.".to_string(),
            };
            return AmbiguityResolutionResult {
                is_ambiguous: false,
                decision,
                competing_candidates_count: 0,
                score_margin: 0.0,
                record,
                explanation: "No candidate workspaces available for evaluation.".to_string(),
            };
        }

        let top = &sorted_candidates[0];

        // 2. If top score is below resolution threshold
        if top.score < self.config.min_score_for_resolution {
            // Check if multiple candidates share weak scores
            let viable_competitors: Vec<&CandidateContextAffinity> = sorted_candidates
                .iter()
                .filter(|c| c.score >= self.config.min_score_for_competition)
                .collect();

            if viable_competitors.len() >= 2 {
                let margin = if sorted_candidates.len() >= 2 {
                    sorted_candidates[0].score - sorted_candidates[1].score
                } else {
                    0.0
                };

                let comp_ids: Vec<String> = viable_competitors.iter().map(|c| c.workspace_id.clone()).collect();
                let comp_names: Vec<&str> = viable_competitors.iter().map(|c| c.workspace_name.as_str()).collect();

                let explanation = format!(
                    "Activity '{}' is shared equally across {} contexts ({}) without decisive evidence. Held quietly in Inbox.",
                    input.title,
                    comp_names.len(),
                    comp_names.join(", ")
                );

                record.status = AmbiguityStatus::PendingMoreEvidence;
                let decision = AmbiguityDecision::HoldAmbiguous {
                    candidate_workspace_ids: comp_ids,
                    score_margin: margin,
                    explanation: explanation.clone(),
                };

                return AmbiguityResolutionResult {
                    is_ambiguous: true,
                    decision,
                    competing_candidates_count: viable_competitors.len(),
                    score_margin: margin,
                    record,
                    explanation,
                };
            }

            let decision = AmbiguityDecision::HoldWeakEvidence {
                reason: format!(
                    "Top score ({:.2}) for '{}' is below resolution threshold ({:.2}).",
                    top.score, top.workspace_name, self.config.min_score_for_resolution
                ),
            };
            return AmbiguityResolutionResult {
                is_ambiguous: false,
                decision,
                competing_candidates_count: viable_competitors.len(),
                score_margin: 0.0,
                record,
                explanation: "Evidence is insufficient for assignment.".to_string(),
            };
        }

        // 3. If single candidate with decisive score
        if sorted_candidates.len() == 1 {
            record.status = AmbiguityStatus::Resolved;
            record.resolved_workspace_id = Some(top.workspace_id.clone());
            record.resolution_reason = Some(format!(
                "Sole candidate '{}' decisively supported with score {:.2}.",
                top.workspace_name, top.score
            ));

            let decision = AmbiguityDecision::DecisiveAssign {
                workspace_id: top.workspace_id.clone(),
                workspace_name: top.workspace_name.clone(),
                confidence: top.confidence,
                reason: record.resolution_reason.clone().unwrap(),
            };

            return AmbiguityResolutionResult {
                is_ambiguous: false,
                decision,
                competing_candidates_count: 1,
                score_margin: top.score,
                record,
                explanation: format!("Decisive match for '{}'.", top.workspace_name),
            };
        }

        // 4. Multiple candidates: check score margin
        let runner_up = &sorted_candidates[1];
        let score_margin = top.score - runner_up.score;

        if score_margin < self.config.ambiguity_margin_threshold {
            // Competitive contest between top candidates (e.g. 2, 3, or more candidates with close scores)
            let competing: Vec<&CandidateContextAffinity> = sorted_candidates
                .iter()
                .filter(|c| (top.score - c.score) <= self.config.ambiguity_margin_threshold)
                .collect();

            let comp_ids: Vec<String> = competing.iter().map(|c| c.workspace_id.clone()).collect();
            let comp_names: Vec<&str> = competing.iter().map(|c| c.workspace_name.as_str()).collect();

            let explanation = format!(
                "Activity '{}' competes closely across {} candidate contexts ({}) with margin {:.2} <= {:.2}. Held in Inbox without arbitrary assignment.",
                input.title,
                competing.len(),
                comp_names.join(", "),
                score_margin,
                self.config.ambiguity_margin_threshold
            );

            record.status = AmbiguityStatus::PendingMoreEvidence;
            let decision = AmbiguityDecision::HoldAmbiguous {
                candidate_workspace_ids: comp_ids,
                score_margin,
                explanation: explanation.clone(),
            };

            AmbiguityResolutionResult {
                is_ambiguous: true,
                decision,
                competing_candidates_count: competing.len(),
                score_margin,
                record,
                explanation,
            }
        } else {
            // Decisive winner
            record.status = AmbiguityStatus::Resolved;
            record.resolved_workspace_id = Some(top.workspace_id.clone());
            record.resolution_reason = Some(format!(
                "Decisive winner '{}' (score {:.2}) exceeds runner-up '{}' (score {:.2}) by margin {:.2}.",
                top.workspace_name, top.score, runner_up.workspace_name, runner_up.score, score_margin
            ));

            let decision = AmbiguityDecision::DecisiveAssign {
                workspace_id: top.workspace_id.clone(),
                workspace_name: top.workspace_name.clone(),
                confidence: top.confidence,
                reason: record.resolution_reason.clone().unwrap(),
            };

            AmbiguityResolutionResult {
                is_ambiguous: false,
                decision,
                competing_candidates_count: sorted_candidates.len(),
                score_margin,
                record,
                explanation: format!("Decisive match for '{}' with sufficient margin.", top.workspace_name),
            }
        }
    }

    /// Re-evaluates a previously ambiguous record when new evidence arrives.
    pub fn re_evaluate_with_new_evidence(
        &self,
        record: &mut AmbiguousActivityRecord,
        new_evidence: &NewEvidenceInput,
    ) -> AmbiguityResolutionResult {
        record.last_evidence_at = Utc::now().to_rfc3339();

        // 1. Authoritative user override
        if let Some(ref ws_id) = new_evidence.user_confirmed_workspace {
            let ws_name = record
                .candidates
                .iter()
                .find(|c| c.workspace_id == *ws_id)
                .map(|c| c.workspace_name.clone())
                .unwrap_or_else(|| ws_id.clone());

            record.status = AmbiguityStatus::ExplicitlyAssigned;
            record.resolved_workspace_id = Some(ws_id.clone());
            record.resolution_reason = Some(format!("User explicitly confirmed assignment to {}.", ws_name));

            let decision = AmbiguityDecision::DecisiveAssign {
                workspace_id: ws_id.clone(),
                workspace_name: ws_name.clone(),
                confidence: 1.0,
                reason: format!("User explicit confirmation to {}.", ws_name),
            };

            return AmbiguityResolutionResult {
                is_ambiguous: false,
                decision,
                competing_candidates_count: 1,
                score_margin: 1.0,
                record: record.clone(),
                explanation: format!("Resolved via explicit user action to {}.", ws_name),
            };
        }

        // 2. Evaluate new URL / repo / file_path evidence against existing candidate affinities
        if let Some(ref url) = new_evidence.url {
            record.url = Some(url.clone());
        }
        if let Some(ref path) = new_evidence.file_path {
            record.file_path = Some(path.clone());
        }

        // Boost affinity for candidates matching the new evidence
        for candidate in &mut record.candidates {
            let ws_name_lower = candidate.workspace_name.to_lowercase();
            let mut matched_new = false;

            if let Some(ref u) = new_evidence.url {
                if u.to_lowercase().contains(&ws_name_lower) {
                    candidate.score = (candidate.score + 0.45).min(1.0);
                    candidate.confidence = (candidate.confidence + 0.40).min(1.0);
                    candidate.matching_signals.push(format!("corroborating_url:{}", u));
                    matched_new = true;
                }
            }

            if let Some(ref p) = new_evidence.file_path {
                if p.to_lowercase().contains(&ws_name_lower) {
                    candidate.score = (candidate.score + 0.50).min(1.0);
                    candidate.confidence = (candidate.confidence + 0.45).min(1.0);
                    candidate.matching_signals.push(format!("corroborating_project_path:{}", p));
                    matched_new = true;
                }
            }

            if let Some(ref task) = new_evidence.task_association {
                if task.to_lowercase().contains(&ws_name_lower) {
                    candidate.score = (candidate.score + 0.40).min(1.0);
                    candidate.confidence = (candidate.confidence + 0.35).min(1.0);
                    candidate.matching_signals.push(format!("corroborating_task:{}", task));
                    matched_new = true;
                }
            }

            if !matched_new && (new_evidence.url.is_some() || new_evidence.file_path.is_some()) {
                // If another candidate strongly matched and this one didn't, apply a slight divergence penalty
                candidate.score = (candidate.score - 0.15).max(0.0);
            }
        }

        let eval_input = AmbiguityEvaluationInput {
            activity_id: record.activity_id.clone(),
            title: record.title.clone(),
            content: record.content.clone(),
            app_name: record.app_name.clone(),
            domain: record.domain.clone(),
            url: record.url.clone(),
            file_path: record.file_path.clone(),
            candidate_evals: record.candidates.clone(),
        };

        let result = self.evaluate(&eval_input);
        record.status = result.record.status;
        record.resolved_workspace_id = result.record.resolved_workspace_id.clone();
        record.resolution_reason = result.record.resolution_reason.clone();
        result
    }

    /// Prunes stale ambiguous records that have exceeded the expiration horizon without resolution.
    pub fn prune_stale_records(
        &self,
        records: &mut Vec<AmbiguousActivityRecord>,
        now: DateTime<Utc>,
    ) -> Vec<String> {
        let mut expired_ids = Vec::new();

        for record in records.iter_mut() {
            if record.status.is_pending() {
                let parsed_time = DateTime::parse_from_rfc3339(&record.first_evaluated_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or(now);

                let age = (now - parsed_time).num_seconds();
                if age > self.config.stale_expiration_seconds {
                    record.status = AmbiguityStatus::ExpiredStale;
                    record.resolution_reason = Some(format!(
                        "Ambiguity expired after {} seconds without corroborating evidence; archived cleanly.",
                        age
                    ));
                    expired_ids.push(record.activity_id.clone());
                }
            }
        }

        expired_ids
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ambiguity_with_two_candidate_contexts_equal_evidence() {
        let manager = AmbiguityManager::new();
        // User opens GitHub with no repo information. Both PayX and ContextSplit use GitHub & VS Code.
        let cand_payx = CandidateContextAffinity::new("ws_payx", "PayX", 0.30, 0.25)
            .with_signals(vec!["app:Chrome".to_string(), "domain:github.com".to_string()], vec!["repo_slug".to_string()]);
        let cand_cs = CandidateContextAffinity::new("ws_cs", "ContextSplit", 0.30, 0.25)
            .with_signals(vec!["app:Chrome".to_string(), "domain:github.com".to_string()], vec!["repo_slug".to_string()]);

        let input = AmbiguityEvaluationInput::new("act_1", "GitHub: Where the world builds software")
            .with_app("Google Chrome")
            .with_domain("github.com")
            .with_candidates(vec![cand_payx, cand_cs]);

        let res = manager.evaluate(&input);

        assert!(res.is_ambiguous);
        assert_eq!(res.competing_candidates_count, 2);
        assert_eq!(res.score_margin, 0.0);
        assert!(matches!(res.decision, AmbiguityDecision::HoldAmbiguous { .. }));
        assert_eq!(res.record.status, AmbiguityStatus::PendingMoreEvidence);
        assert_eq!(res.record.resolved_workspace_id, None);
        assert!(!res.record.is_ui_disruptive); // Invariant: do not surface noisy banners
    }

    #[test]
    fn test_ambiguity_with_three_candidate_contexts() {
        let manager = AmbiguityManager::new();
        // User opens Figma. Contexts A (PayX), B (ContextSplit), and C (Portfolio) all use Figma.
        let cand_a = CandidateContextAffinity::new("ws_payx", "PayX", 0.35, 0.30);
        let cand_b = CandidateContextAffinity::new("ws_cs", "ContextSplit", 0.33, 0.30);
        let cand_c = CandidateContextAffinity::new("ws_port", "Portfolio Design", 0.32, 0.30);

        let input = AmbiguityEvaluationInput::new("act_2", "Figma - Workspace Dashboard")
            .with_app("Figma")
            .with_candidates(vec![cand_a, cand_b, cand_c]);

        let res = manager.evaluate(&input);

        assert!(res.is_ambiguous);
        assert_eq!(res.competing_candidates_count, 3);
        assert!(res.score_margin <= 0.05);
        assert!(matches!(res.decision, AmbiguityDecision::HoldAmbiguous { .. }));
        assert_eq!(res.record.status, AmbiguityStatus::PendingMoreEvidence);
    }

    #[test]
    fn test_equal_evidence_does_not_arbitrarily_attach_to_either_context() {
        let manager = AmbiguityManager::new();
        let cand_payx = CandidateContextAffinity::new("ws_payx", "PayX", 0.50, 0.40);
        let cand_cs = CandidateContextAffinity::new("ws_cs", "ContextSplit", 0.50, 0.40);

        let input = AmbiguityEvaluationInput::new("act_3", "VS Code - Shared Library")
            .with_app("Visual Studio Code")
            .with_candidates(vec![cand_payx, cand_cs]);

        let res = manager.evaluate(&input);

        assert!(res.is_ambiguous);
        assert_eq!(res.score_margin, 0.0);
        assert_eq!(res.record.resolved_workspace_id, None);
        assert!(res.explanation.contains("competes closely across 2 candidate contexts"));
    }

    #[test]
    fn test_strong_evidence_arriving_later_resolves_ambiguity() {
        let manager = AmbiguityManager::new();
        // 1. Initially ambiguous
        let cand_payx = CandidateContextAffinity::new("ws_payx", "PayX", 0.30, 0.25);
        let cand_cs = CandidateContextAffinity::new("ws_cs", "ContextSplit", 0.30, 0.25);

        let input = AmbiguityEvaluationInput::new("act_4", "GitHub Repository")
            .with_app("Google Chrome")
            .with_candidates(vec![cand_payx, cand_cs]);

        let initial_res = manager.evaluate(&input);
        assert!(initial_res.is_ambiguous);
        let mut record = initial_res.record;

        // 2. Strong corroborating evidence arrives later: URL updated to github.com/myorg/PayX
        let new_evidence = NewEvidenceInput::empty()
            .with_url("https://github.com/myorg/PayX/pull/42")
            .with_file_path("/Projects/PayX/src/billing.rs");

        let resolved_res = manager.re_evaluate_with_new_evidence(&mut record, &new_evidence);

        assert!(!resolved_res.is_ambiguous);
        assert_eq!(record.status, AmbiguityStatus::Resolved);
        assert_eq!(record.resolved_workspace_id, Some("ws_payx".to_string()));
        assert!(matches!(
            resolved_res.decision,
            AmbiguityDecision::DecisiveAssign { ref workspace_id, .. } if workspace_id == "ws_payx"
        ));
    }

    #[test]
    fn test_stale_candidates_pruning() {
        let config = AmbiguityConfig {
            stale_expiration_seconds: 3600, // 1 hour
            ..Default::default()
        };
        let manager = AmbiguityManager::with_config(config);

        let now = Utc::now();
        let past = (now - chrono::Duration::seconds(7200)).to_rfc3339(); // 2 hours ago

        let cand_payx = CandidateContextAffinity::new("ws_payx", "PayX", 0.30, 0.25);
        let cand_cs = CandidateContextAffinity::new("ws_cs", "ContextSplit", 0.30, 0.25);

        let mut record = AmbiguousActivityRecord::new("act_5", "GitHub", vec![cand_payx, cand_cs]);
        record.first_evaluated_at = past.clone();
        record.last_evidence_at = past;

        let mut records = vec![record];
        let pruned_ids = manager.prune_stale_records(&mut records, now);

        assert_eq!(pruned_ids.len(), 1);
        assert_eq!(pruned_ids[0], "act_5");
        assert_eq!(records[0].status, AmbiguityStatus::ExpiredStale);
        assert!(records[0].resolution_reason.as_ref().unwrap().contains("expired"));
    }

    #[test]
    fn test_decisive_initial_lead_assigns_without_ambiguity() {
        let manager = AmbiguityManager::new();
        let cand_payx = CandidateContextAffinity::new("ws_payx", "PayX", 0.85, 0.80);
        let cand_cs = CandidateContextAffinity::new("ws_cs", "ContextSplit", 0.25, 0.20);

        let input = AmbiguityEvaluationInput::new("act_6", "PayX Billing Engine")
            .with_candidates(vec![cand_payx, cand_cs]);

        let res = manager.evaluate(&input);

        assert!(!res.is_ambiguous);
        assert_eq!(res.score_margin, 0.60);
        assert_eq!(res.record.status, AmbiguityStatus::Resolved);
        assert_eq!(res.record.resolved_workspace_id, Some("ws_payx".to_string()));
    }
}

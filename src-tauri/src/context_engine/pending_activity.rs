use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =========================================================================
// Contextual Activity Status (Step 7)
// =========================================================================

/// Lifecycle status for an internal contextual activity candidate.
///
/// Invariant: Only `Assigned` activity may modify downstream context structures
/// (`ContextMemory`, `ContextWorkset`, `ContextState`, `ContextSnapshot`, `ImportantMemory`).
/// `Pending`, `Rejected`, and `Ambiguous` states are strictly gated to prevent
/// weak early evidence from contaminating contexts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextualActivityStatus {
    /// Confirmed or decisively accepted into a target context with strong corroborated evidence.
    Assigned,
    /// Unassigned activity held internally awaiting stronger corroborating signals.
    Pending,
    /// Contaminating, noise, or dismissed activity rejected from attaching to any context.
    Rejected,
    /// Activity with plausible affinity to multiple candidate contexts; held until disambiguated.
    Ambiguous,
}

impl ContextualActivityStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Assigned => "assigned",
            Self::Pending => "pending",
            Self::Rejected => "rejected",
            Self::Ambiguous => "ambiguous",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Assigned => "Assigned to Context",
            Self::Pending => "Pending Corroborating Evidence",
            Self::Rejected => "Rejected (Contamination Defense)",
            Self::Ambiguous => "Ambiguous (Multiple Candidates)",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        let norm = s.trim().to_lowercase().replace(['-', ' '], "_");
        match norm.as_str() {
            "assigned" | "accepted" | "confirmed" => Ok(Self::Assigned),
            "pending" | "unassigned" | "held" | "waiting" => Ok(Self::Pending),
            "rejected" | "dismissed" | "contaminated" => Ok(Self::Rejected),
            "ambiguous" | "split" | "competing" => Ok(Self::Ambiguous),
            _ => Err(format!("Unknown ContextualActivityStatus: '{}'", s)),
        }
    }

    pub fn is_assigned(&self) -> bool {
        matches!(self, Self::Assigned)
    }

    pub fn is_pending(&self) -> bool {
        matches!(self, Self::Pending)
    }

    pub fn is_rejected(&self) -> bool {
        matches!(self, Self::Rejected)
    }

    pub fn is_ambiguous(&self) -> bool {
        matches!(self, Self::Ambiguous)
    }

    /// Whether this activity status permits modifying any context state, workset, memory, or snapshot.
    pub fn can_modify_context(&self) -> bool {
        matches!(self, Self::Assigned)
    }

    /// Gating check: strictly prevents `Pending`, `Ambiguous`, and `Rejected` activity
    /// from modifying `ContextMemory`.
    pub fn can_alter_context_memory(&self) -> bool {
        self.can_modify_context()
    }

    /// Gating check: strictly prevents non-assigned activity from modifying `ContextWorkset`.
    pub fn can_alter_context_workset(&self) -> bool {
        self.can_modify_context()
    }

    /// Gating check: strictly prevents non-assigned activity from modifying `ContextState`.
    pub fn can_alter_context_state(&self) -> bool {
        self.can_modify_context()
    }

    /// Gating check: strictly prevents non-assigned activity from modifying `ContextSnapshot`.
    pub fn can_alter_context_snapshot(&self) -> bool {
        self.can_modify_context()
    }

    /// Gating check: strictly prevents non-assigned activity from modifying `ImportantMemory`.
    pub fn can_alter_important_memory(&self) -> bool {
        self.can_modify_context()
    }
}

impl std::fmt::Display for ContextualActivityStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// =========================================================================
// Supporting Evidence Signal
// =========================================================================

/// Individual piece of supporting or disambiguating evidence for an activity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActivityEvidenceSignal {
    /// Signal source: "project_repo", "file_path", "window_title", "url_domain",
    /// "task_link", "user_confirmation", "semantic_embedding", "temporal_streak"
    pub source: String,
    /// Associated candidate workspace ID (if attributed to a specific context)
    pub workspace_id: Option<String>,
    /// Confidence of this specific signal [0.0, 1.0]
    pub confidence: f64,
    /// Relative weight of this signal type in evaluation [0.0, 2.0]
    pub weight: f64,
    /// Whether this signal represents a strong deterministic anchor (e.g. verified repo path, user confirm)
    pub is_strong: bool,
    /// Human-readable explanation of the signal
    pub description: String,
    /// RFC3339 timestamp when signal was observed
    pub observed_at: String,
}

impl ActivityEvidenceSignal {
    pub fn new(
        source: impl Into<String>,
        workspace_id: Option<String>,
        confidence: f64,
        weight: f64,
        is_strong: bool,
        description: impl Into<String>,
    ) -> Self {
        Self {
            source: source.into(),
            workspace_id,
            confidence: confidence.clamp(0.0, 1.0),
            weight: weight.clamp(0.0, 2.0),
            is_strong,
            description: description.into(),
            observed_at: Utc::now().to_rfc3339(),
        }
    }

    pub fn user_confirmation(workspace_id: &str) -> Self {
        Self::new(
            "user_confirmation",
            Some(workspace_id.to_string()),
            1.0,
            2.0,
            true,
            "User explicitly confirmed activity assignment",
        )
    }

    pub fn project_repo_match(workspace_id: &str, repo_name: &str) -> Self {
        Self::new(
            "project_repo",
            Some(workspace_id.to_string()),
            0.95,
            1.5,
            true,
            format!("Direct project repository match: '{}'", repo_name),
        )
    }

    pub fn file_path_match(workspace_id: &str, file_path: &str) -> Self {
        Self::new(
            "file_path",
            Some(workspace_id.to_string()),
            0.90,
            1.4,
            true,
            format!("Workspace file path match: '{}'", file_path),
        )
    }

    pub fn task_transition_match(workspace_id: &str, task_title: &str) -> Self {
        Self::new(
            "task_link",
            Some(workspace_id.to_string()),
            0.88,
            1.3,
            true,
            format!("Associated active task: '{}'", task_title),
        )
    }

    pub fn weak_app_signal(app_name: &str) -> Self {
        Self::new(
            "app_identity_only",
            None,
            0.30,
            0.4,
            false,
            format!("Generic application identity: '{}'", app_name),
        )
    }

    pub fn contamination_signal(reason: &str) -> Self {
        Self::new(
            "contamination_defense",
            None,
            0.10,
            1.5,
            false,
            format!("Contamination detected: {}", reason),
        )
    }
}

// =========================================================================
// Pending Contextual Activity Record
// =========================================================================

/// Internal record for a contextual activity, tracking its status from ingestion
/// through evidence accumulation to assignment or rejection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingContextualActivity {
    /// Unique identifier for this activity event
    pub activity_id: String,
    /// Associated item ID in the database (if already created)
    pub item_id: Option<String>,
    /// Activity title / window title / tab title
    pub title: String,
    /// Content or text payload
    pub content: Option<String>,
    /// Application name
    pub app_name: Option<String>,
    /// Domain if browser activity
    pub domain: Option<String>,
    /// Full URL if browser activity
    pub url: Option<String>,
    /// File path if editor/terminal activity
    pub file_path: Option<String>,
    /// Current lifecycle status (assigned | pending | rejected | ambiguous)
    pub status: ContextualActivityStatus,
    /// Target workspace ID if assigned
    pub assigned_workspace_id: Option<String>,
    /// Assigned confidence score [0.0, 1.0]
    pub assigned_confidence: f64,
    /// Candidate workspace IDs under consideration
    pub candidate_workspace_ids: Vec<String>,
    /// Computed affinity score per candidate workspace ID
    pub candidate_scores: HashMap<String, f64>,
    /// Accumulated evidence signals
    pub accumulated_evidence: Vec<ActivityEvidenceSignal>,
    /// Timestamp when activity was first captured
    pub first_seen_at: String,
    /// Timestamp when activity was last updated with evidence
    pub last_seen_at: String,
    /// Reason explaining current status or assignment
    pub resolution_reason: Option<String>,
    /// Whether this record is strictly internal (Invariant: default true, do not clutter UI inbox)
    pub internal_only: bool,
}

impl PendingContextualActivity {
    pub fn new(activity_id: impl Into<String>, title: impl Into<String>) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            activity_id: activity_id.into(),
            item_id: None,
            title: title.into(),
            content: None,
            app_name: None,
            domain: None,
            url: None,
            file_path: None,
            status: ContextualActivityStatus::Pending,
            assigned_workspace_id: None,
            assigned_confidence: 0.0,
            candidate_workspace_ids: Vec::new(),
            candidate_scores: HashMap::new(),
            accumulated_evidence: Vec::new(),
            first_seen_at: now.clone(),
            last_seen_at: now,
            resolution_reason: Some("Awaiting corroborating evidence".to_string()),
            internal_only: true,
        }
    }

    pub fn with_item_id(mut self, item_id: impl Into<String>) -> Self {
        self.item_id = Some(item_id.into());
        self
    }

    pub fn with_app_name(mut self, app_name: impl Into<String>) -> Self {
        self.app_name = Some(app_name.into());
        self
    }

    pub fn with_url_and_domain(
        mut self,
        url: Option<String>,
        domain: Option<String>,
    ) -> Self {
        self.url = url;
        self.domain = domain;
        self
    }

    pub fn with_file_path(mut self, file_path: impl Into<String>) -> Self {
        self.file_path = Some(file_path.into());
        self
    }

    pub fn with_candidates(mut self, candidates: Vec<String>) -> Self {
        self.candidate_workspace_ids = candidates;
        self
    }

    /// Appends a new evidence signal and updates timestamps.
    pub fn add_signal(&mut self, signal: ActivityEvidenceSignal) {
        self.last_seen_at = Utc::now().to_rfc3339();
        self.accumulated_evidence.push(signal);
    }

    /// Evaluates accumulated evidence to determine whether this activity can transition
    /// from `Pending` or `Ambiguous` to `Assigned` (or `Rejected`).
    pub fn evaluate_status(&mut self, config: &PendingActivityConfig) {
        // 1. Check for explicit user confirmation
        if let Some(user_signal) = self
            .accumulated_evidence
            .iter()
            .find(|s| s.source == "user_confirmation")
        {
            if let Some(ref ws_id) = user_signal.workspace_id {
                self.status = ContextualActivityStatus::Assigned;
                self.assigned_workspace_id = Some(ws_id.clone());
                self.assigned_confidence = 1.0;
                self.resolution_reason =
                    Some("User authoritatively assigned activity to context".to_string());
                return;
            }
        }

        // 2. Check for contamination defense trigger
        if self
            .accumulated_evidence
            .iter()
            .any(|s| s.source == "contamination_defense")
        {
            self.status = ContextualActivityStatus::Rejected;
            self.assigned_workspace_id = None;
            self.assigned_confidence = 0.0;
            self.resolution_reason =
                Some("Contamination defense triggered: unrelated or noise activity".to_string());
            return;
        }

        // 3. Aggregate scores per candidate workspace
        let mut scores: HashMap<String, f64> = HashMap::new();
        let mut strong_counts: HashMap<String, usize> = HashMap::new();

        for signal in &self.accumulated_evidence {
            if let Some(ref ws_id) = signal.workspace_id {
                let weighted_score = signal.confidence * signal.weight;
                *scores.entry(ws_id.clone()).or_insert(0.0) += weighted_score;
                if signal.is_strong {
                    *strong_counts.entry(ws_id.clone()).or_insert(0) += 1;
                }
            }
        }

        self.candidate_scores = scores.clone();

        if scores.is_empty() {
            // No workspace-attributed evidence yet -> remains Pending
            self.status = ContextualActivityStatus::Pending;
            self.assigned_workspace_id = None;
            self.assigned_confidence = 0.0;
            self.resolution_reason =
                Some("No workspace-attributed evidence available yet".to_string());
            return;
        }

        // Sort candidates by total score descending
        let mut ranked: Vec<(String, f64)> = scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let top = &ranked[0];
        let top_ws = &top.0;
        let top_score = top.1;
        let top_strong_count = strong_counts.get(top_ws).copied().unwrap_or(0);

        // Normalize top confidence [0.0, 1.0] (single strong signal ~1.42 gives 1.0)
        let normalized_conf = (top_score / 1.4).min(1.0);

        if ranked.len() >= 2 {
            let runner_up = &ranked[1];
            let runner_up_score = runner_up.1;
            let margin = top_score - runner_up_score;

            // If margin is too small and both candidates have reasonable affinity -> Ambiguous
            if margin < config.min_score_margin && top_score >= config.ambiguity_threshold {
                self.status = ContextualActivityStatus::Ambiguous;
                self.assigned_workspace_id = None;
                self.assigned_confidence = normalized_conf;
                self.resolution_reason = Some(format!(
                    "Competing candidates '{}' ({:.2}) and '{}' ({:.2}) with insufficient margin ({:.2})",
                    top_ws, top_score, runner_up.0, runner_up_score, margin
                ));
                return;
            }
        }

        // Check if top candidate meets assignment criteria
        let meets_score = normalized_conf >= config.min_assign_confidence;
        let meets_strong = if config.require_strong_signal_for_assignment {
            top_strong_count >= config.min_strong_signals
        } else {
            true
        };

        if meets_score && meets_strong {
            self.status = ContextualActivityStatus::Assigned;
            self.assigned_workspace_id = Some(top_ws.clone());
            self.assigned_confidence = normalized_conf;
            self.resolution_reason = Some(format!(
                "Assigned to workspace '{}' with {:.1}% confidence ({} strong signals)",
                top_ws,
                normalized_conf * 100.0,
                top_strong_count
            ));
        } else {
            self.status = ContextualActivityStatus::Pending;
            self.assigned_workspace_id = None;
            self.assigned_confidence = normalized_conf;
            self.resolution_reason = Some(format!(
                "Evidence insufficient for definitive assignment: confidence {:.2} / {:.2}, strong signals {} / {}",
                normalized_conf,
                config.min_assign_confidence,
                top_strong_count,
                config.min_strong_signals
            ));
        }
    }
}

// =========================================================================
// Configuration
// =========================================================================

/// Configuration parameters for Pending Activity management.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingActivityConfig {
    /// Minimum confidence required to promote from Pending/Ambiguous to Assigned (default: 0.65)
    pub min_assign_confidence: f64,
    /// Minimum score margin over runner-up candidate to avoid ambiguity (default: 0.25)
    pub min_score_margin: f64,
    /// Threshold score where competing candidate triggers Ambiguous state (default: 0.40)
    pub ambiguity_threshold: f64,
    /// Whether at least one strong signal (project repo, file path, task link) is required (default: true)
    pub require_strong_signal_for_assignment: bool,
    /// Minimum number of strong signals required (default: 1)
    pub min_strong_signals: usize,
    /// Maximum age in seconds before stale unassigned pending activities are pruned (default: 86400 = 24h)
    pub max_pending_age_seconds: i64,
}

impl Default for PendingActivityConfig {
    fn default() -> Self {
        Self {
            min_assign_confidence: 0.65,
            min_score_margin: 0.25,
            ambiguity_threshold: 0.40,
            require_strong_signal_for_assignment: true,
            min_strong_signals: 1,
            max_pending_age_seconds: 86400,
        }
    }
}

// =========================================================================
// Pending Activity Manager (Coordinator)
// =========================================================================

/// In-memory manager and registry for pending contextual activities.
///
/// Ensures all candidate activities are held safely in a non-contaminating state
/// until sufficient corroborated evidence warrants context modification.
#[derive(Debug, Clone, Default)]
pub struct PendingActivityManager {
    activities: HashMap<String, PendingContextualActivity>,
    config: PendingActivityConfig,
}

impl PendingActivityManager {
    pub fn new(config: PendingActivityConfig) -> Self {
        Self {
            activities: HashMap::new(),
            config,
        }
    }

    /// Registers a new candidate activity. Returns reference to the created record.
    pub fn register_activity(
        &mut self,
        activity_id: impl Into<String>,
        title: impl Into<String>,
    ) -> &PendingContextualActivity {
        let id_str = activity_id.into();
        let record = PendingContextualActivity::new(id_str.clone(), title);
        self.activities.insert(id_str.clone(), record);
        self.activities.get(&id_str).unwrap()
    }

    /// Ingests an activity with initial signals, evaluates its state immediately.
    pub fn ingest_activity(
        &mut self,
        mut activity: PendingContextualActivity,
    ) -> &PendingContextualActivity {
        activity.evaluate_status(&self.config);
        let id = activity.activity_id.clone();
        self.activities.insert(id.clone(), activity);
        self.activities.get(&id).unwrap()
    }

    /// Adds a new evidence signal to an existing activity and re-evaluates its status.
    pub fn add_evidence(
        &mut self,
        activity_id: &str,
        signal: ActivityEvidenceSignal,
    ) -> Option<&PendingContextualActivity> {
        if let Some(record) = self.activities.get_mut(activity_id) {
            record.add_signal(signal);
            record.evaluate_status(&self.config);
            Some(&*record)
        } else {
            None
        }
    }

    /// Gets an activity record by ID.
    pub fn get_activity(&self, activity_id: &str) -> Option<&PendingContextualActivity> {
        self.activities.get(activity_id)
    }

    /// Checks whether an activity is allowed to alter `ContextMemory`.
    pub fn can_alter_context_memory(&self, activity_id: &str) -> bool {
        self.activities
            .get(activity_id)
            .map(|a| a.status.can_alter_context_memory())
            .unwrap_or(false)
    }

    /// Checks whether an activity is allowed to alter `ContextWorkset`.
    pub fn can_alter_context_workset(&self, activity_id: &str) -> bool {
        self.activities
            .get(activity_id)
            .map(|a| a.status.can_alter_context_workset())
            .unwrap_or(false)
    }

    /// Checks whether an activity is allowed to alter `ContextState`.
    pub fn can_alter_context_state(&self, activity_id: &str) -> bool {
        self.activities
            .get(activity_id)
            .map(|a| a.status.can_alter_context_state())
            .unwrap_or(false)
    }

    /// Checks whether an activity is allowed to alter `ContextSnapshot`.
    pub fn can_alter_context_snapshot(&self, activity_id: &str) -> bool {
        self.activities
            .get(activity_id)
            .map(|a| a.status.can_alter_context_snapshot())
            .unwrap_or(false)
    }

    /// Checks whether an activity is allowed to alter `ImportantMemory`.
    pub fn can_alter_important_memory(&self, activity_id: &str) -> bool {
        self.activities
            .get(activity_id)
            .map(|a| a.status.can_alter_important_memory())
            .unwrap_or(false)
    }

    /// Returns all assigned activities for a specific workspace.
    pub fn get_assigned_for_workspace(&self, workspace_id: &str) -> Vec<&PendingContextualActivity> {
        self.activities
            .values()
            .filter(|a| {
                a.status.is_assigned()
                    && a.assigned_workspace_id.as_deref() == Some(workspace_id)
            })
            .collect()
    }

    /// Returns all activities currently in `Pending` status.
    pub fn get_pending_activities(&self) -> Vec<&PendingContextualActivity> {
        self.activities
            .values()
            .filter(|a| a.status.is_pending())
            .collect()
    }

    /// Returns all activities currently in `Ambiguous` status.
    pub fn get_ambiguous_activities(&self) -> Vec<&PendingContextualActivity> {
        self.activities
            .values()
            .filter(|a| a.status.is_ambiguous())
            .collect()
    }

    /// Returns all activities currently in `Rejected` status.
    pub fn get_rejected_activities(&self) -> Vec<&PendingContextualActivity> {
        self.activities
            .values()
            .filter(|a| a.status.is_rejected())
            .collect()
    }

    /// Cleans up stale pending activities older than `max_pending_age_seconds`.
    pub fn cleanup_stale(&mut self) -> usize {
        let now = Utc::now();
        let max_age = chrono::Duration::seconds(self.config.max_pending_age_seconds);

        let initial_len = self.activities.len();
        self.activities.retain(|_, record| {
            if record.status.is_assigned() {
                return true; // Keep assigned records
            }
            if let Ok(first_seen) = DateTime::parse_from_rfc3339(&record.first_seen_at) {
                let age = now.signed_duration_since(first_seen.with_timezone(&Utc));
                age <= max_age
            } else {
                true
            }
        });
        initial_len - self.activities.len()
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state_is_pending_and_cannot_modify_context() {
        let mut mgr = PendingActivityManager::new(PendingActivityConfig::default());
        let act = mgr.register_activity("act_1", "VS Code - Untitled");

        assert_eq!(act.status, ContextualActivityStatus::Pending);
        assert!(!act.status.can_modify_context());
        assert!(!act.status.can_alter_context_memory());
        assert!(!act.status.can_alter_context_workset());
        assert!(!act.status.can_alter_context_state());
        assert!(!act.status.can_alter_context_snapshot());
        assert!(!act.status.can_alter_important_memory());

        assert!(!mgr.can_alter_context_memory("act_1"));
        assert!(!mgr.can_alter_context_workset("act_1"));
        assert!(!mgr.can_alter_context_state("act_1"));
        assert!(!mgr.can_alter_context_snapshot("act_1"));
        assert!(!mgr.can_alter_important_memory("act_1"));
    }

    #[test]
    fn test_weak_app_only_signal_remains_pending() {
        let mut mgr = PendingActivityManager::new(PendingActivityConfig::default());
        mgr.register_activity("act_weak", "Chrome");

        let updated = mgr
            .add_evidence("act_weak", ActivityEvidenceSignal::weak_app_signal("Google Chrome"))
            .unwrap();

        assert_eq!(updated.status, ContextualActivityStatus::Pending);
        assert!(updated.assigned_workspace_id.is_none());
        assert!(!updated.status.can_modify_context());
        assert!(!mgr.can_alter_context_memory("act_weak"));
        assert!(!mgr.can_alter_context_workset("act_weak"));
    }

    #[test]
    fn test_strong_evidence_promotes_pending_to_assigned() {
        let mut mgr = PendingActivityManager::new(PendingActivityConfig::default());
        mgr.register_activity("act_repo", "PayX - PaymentGateway.ts");

        // Add strong project repo match
        let updated = mgr
            .add_evidence(
                "act_repo",
                ActivityEvidenceSignal::project_repo_match("ws_payx", "PayX"),
            )
            .unwrap();

        assert_eq!(updated.status, ContextualActivityStatus::Assigned);
        assert_eq!(updated.assigned_workspace_id.as_deref(), Some("ws_payx"));
        assert!(updated.assigned_confidence >= 0.65);
        assert!(updated.status.can_modify_context());
        assert!(mgr.can_alter_context_memory("act_repo"));
        assert!(mgr.can_alter_context_workset("act_repo"));
        assert!(mgr.can_alter_context_state("act_repo"));
        assert!(mgr.can_alter_context_snapshot("act_repo"));
        assert!(mgr.can_alter_important_memory("act_repo"));

        let ws_assigned = mgr.get_assigned_for_workspace("ws_payx");
        assert_eq!(ws_assigned.len(), 1);
        assert_eq!(ws_assigned[0].activity_id, "act_repo");
    }

    #[test]
    fn test_competing_candidates_trigger_ambiguous_state() {
        let mut mgr = PendingActivityManager::new(PendingActivityConfig::default());
        mgr.register_activity("act_ambig", "GitHub - Pull Request #42");

        // Evidence for Workspace A (PayX)
        mgr.add_evidence(
            "act_ambig",
            ActivityEvidenceSignal::new(
                "url_match",
                Some("ws_payx".to_string()),
                0.70,
                1.0,
                false,
                "GitHub domain matching PayX history",
            ),
        );

        // Equal evidence for Workspace B (ContextSplit)
        let updated = mgr
            .add_evidence(
                "act_ambig",
                ActivityEvidenceSignal::new(
                    "url_match",
                    Some("ws_contextsplit".to_string()),
                    0.72,
                    1.0,
                    false,
                    "GitHub domain matching ContextSplit history",
                ),
            )
            .unwrap();

        assert_eq!(updated.status, ContextualActivityStatus::Ambiguous);
        assert!(updated.assigned_workspace_id.is_none());
        assert!(!updated.status.can_modify_context());
        assert!(!mgr.can_alter_context_memory("act_ambig"));
        assert!(!mgr.can_alter_context_workset("act_ambig"));

        assert_eq!(mgr.get_ambiguous_activities().len(), 1);
    }

    #[test]
    fn test_ambiguous_resolves_to_assigned_when_stronger_evidence_arrives() {
        let mut mgr = PendingActivityManager::new(PendingActivityConfig::default());
        mgr.register_activity("act_resolve", "Pull Request Details");

        // Initial ambiguous signals
        mgr.add_evidence(
            "act_resolve",
            ActivityEvidenceSignal::new(
                "keyword",
                Some("ws_payx".to_string()),
                0.60,
                1.0,
                false,
                "Keyword match PayX",
            ),
        );
        mgr.add_evidence(
            "act_resolve",
            ActivityEvidenceSignal::new(
                "keyword",
                Some("ws_contextsplit".to_string()),
                0.62,
                1.0,
                false,
                "Keyword match ContextSplit",
            ),
        );

        let record_ambig = mgr.get_activity("act_resolve").unwrap();
        assert_eq!(record_ambig.status, ContextualActivityStatus::Ambiguous);

        // Decisive evidence arrives for ContextSplit: file path match
        let resolved = mgr
            .add_evidence(
                "act_resolve",
                ActivityEvidenceSignal::file_path_match(
                    "ws_contextsplit",
                    "/ContextSplit/src/App.tsx",
                ),
            )
            .unwrap();

        assert_eq!(resolved.status, ContextualActivityStatus::Assigned);
        assert_eq!(resolved.assigned_workspace_id.as_deref(), Some("ws_contextsplit"));
        assert!(mgr.can_alter_context_memory("act_resolve"));
        assert!(mgr.can_alter_context_workset("act_resolve"));
    }

    #[test]
    fn test_contamination_defense_triggers_rejected_state() {
        let mut mgr = PendingActivityManager::new(PendingActivityConfig::default());
        mgr.register_activity("act_noise", "YouTube - Cat Videos Compilation");

        let updated = mgr
            .add_evidence(
                "act_noise",
                ActivityEvidenceSignal::contamination_signal("Entertainment domain detour while in PayX context"),
            )
            .unwrap();

        assert_eq!(updated.status, ContextualActivityStatus::Rejected);
        assert!(updated.assigned_workspace_id.is_none());
        assert!(!updated.status.can_modify_context());
        assert!(!mgr.can_alter_context_memory("act_noise"));
        assert!(!mgr.can_alter_context_workset("act_noise"));
        assert!(!mgr.can_alter_context_state("act_noise"));
        assert!(!mgr.can_alter_context_snapshot("act_noise"));
        assert!(!mgr.can_alter_important_memory("act_noise"));

        assert_eq!(mgr.get_rejected_activities().len(), 1);
    }

    #[test]
    fn test_explicit_user_confirmation_immediately_assigns() {
        let mut mgr = PendingActivityManager::new(PendingActivityConfig::default());
        mgr.register_activity("act_manual", "Notes on API Architecture");

        let confirmed = mgr
            .add_evidence(
                "act_manual",
                ActivityEvidenceSignal::user_confirmation("ws_target"),
            )
            .unwrap();

        assert_eq!(confirmed.status, ContextualActivityStatus::Assigned);
        assert_eq!(confirmed.assigned_workspace_id.as_deref(), Some("ws_target"));
        assert_eq!(confirmed.assigned_confidence, 1.0);
        assert!(mgr.can_alter_context_memory("act_manual"));
    }

    #[test]
    fn test_internal_only_flag_preserves_ui_cleanliness() {
        let act = PendingContextualActivity::new("act_clean", "Transient activity");
        assert!(act.internal_only, "Invariant: internal_only must be true by default to avoid UI noise");
    }
}

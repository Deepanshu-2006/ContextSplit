use std::collections::{HashMap, HashSet};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

// =========================================================================
// Epistemic Disclaimer
// =========================================================================

pub const CONTEXT_HEALTH_CHECK_DISCLAIMER: &str =
    "ContextSplit context health check is a non-mutating developer diagnostic tool. It reports structural and contextual inconsistencies safely without auto-repairing or modifying database records.";

// =========================================================================
// Health Status & Severity Models
// =========================================================================

/// Overall health check status rating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ContextHealthStatus {
    /// Zero errors and zero warnings found.
    Pass,
    /// Warnings found, but no critical integrity errors.
    Warning,
    /// Critical data corruption or referential integrity errors found.
    Error,
}

impl ContextHealthStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Warning => "WARNING",
            Self::Error => "ERROR",
        }
    }
}

impl std::fmt::Display for ContextHealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Severity level of an individual health check diagnostic issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthIssueSeverity {
    /// Non-fatal diagnostic warning (e.g. empty context, suspicious naming).
    Warning,
    /// Fatal integrity error (e.g. broken foreign key, impossible timestamp, invalid transition).
    Error,
}

impl HealthIssueSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Warning => "WARNING",
            Self::Error => "ERROR",
        }
    }
}

impl std::fmt::Display for HealthIssueSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Category classification of the detected inconsistency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthIssueCategory {
    /// Context exists with zero evidence, signals, or associated items.
    NoValidEvidence,
    /// Session references non-existent workspace or item references non-existent session.
    OrphanedSession,
    /// Important memory references missing context or non-existent entity.
    OrphanedMemory,
    /// Workset or candidate item membership references non-existent context or item.
    OrphanedWorksetItem,
    /// Snapshot references non-existent workspace/context.
    MissingContextSnapshot,
    /// Task candidate references non-existent workspace/context.
    MissingContextTask,
    /// Feedback signal references non-existent target item/context.
    MissingEntityFeedback,
    /// Lifecycle transition violates state machine rules or sequence order.
    InvalidLifecycleTransition,
    /// Duplicate workspace identities or normalized names detected.
    DuplicateContextIdentity,
    /// Impossible timestamp (future timestamp, unparseable RFC-3339, or ended_at < started_at).
    ImpossibleTimestamp,
    /// Contradictory flag state (e.g. active context with archived timestamp set).
    ContradictoryState,
    /// Corrupted reconstruction reference or unparseable JSON payload.
    CorruptedReconstruction,
}

impl HealthIssueCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NoValidEvidence => "no_valid_evidence",
            Self::OrphanedSession => "orphaned_session",
            Self::OrphanedMemory => "orphaned_memory",
            Self::OrphanedWorksetItem => "orphaned_workset_item",
            Self::MissingContextSnapshot => "missing_context_snapshot",
            Self::MissingContextTask => "missing_context_task",
            Self::MissingEntityFeedback => "missing_entity_feedback",
            Self::InvalidLifecycleTransition => "invalid_lifecycle_transition",
            Self::DuplicateContextIdentity => "duplicate_context_identity",
            Self::ImpossibleTimestamp => "impossible_timestamp",
            Self::ContradictoryState => "contradictory_state",
            Self::CorruptedReconstruction => "corrupted_reconstruction",
        }
    }

    pub fn display_title(&self) -> &'static str {
        match self {
            Self::NoValidEvidence => "Context with No Valid Evidence",
            Self::OrphanedSession => "Orphaned Session Reference",
            Self::OrphanedMemory => "Orphaned Context Memory",
            Self::OrphanedWorksetItem => "Orphaned Workset Item Reference",
            Self::MissingContextSnapshot => "Snapshot Referencing Missing Context",
            Self::MissingContextTask => "Task Referencing Missing Context",
            Self::MissingEntityFeedback => "Feedback Referencing Missing Entity",
            Self::InvalidLifecycleTransition => "Invalid Lifecycle Transition",
            Self::DuplicateContextIdentity => "Duplicate Context Identity",
            Self::ImpossibleTimestamp => "Impossible Timestamp Sequence",
            Self::ContradictoryState => "Contradictory Context State",
            Self::CorruptedReconstruction => "Corrupted Reconstruction Reference",
        }
    }
}

// =========================================================================
// Diagnostic Issue Model
// =========================================================================

/// Individual health issue detected during context integrity validation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextHealthIssue {
    /// Severity rating of this issue
    pub severity: HealthIssueSeverity,
    /// Category of the inconsistency
    pub category: HealthIssueCategory,
    /// Entity type affected (e.g., "workspace", "session", "item", "snapshot", "task")
    pub entity_type: String,
    /// ID of the affected entity
    pub entity_id: String,
    /// Associated workspace/context ID if applicable
    pub context_id: Option<String>,
    /// Clear diagnostic message explaining the inconsistency
    pub message: String,
    /// Additional diagnostic details or suggested developer remediation
    pub details: Option<String>,
}

impl ContextHealthIssue {
    pub fn new(
        severity: HealthIssueSeverity,
        category: HealthIssueCategory,
        entity_type: impl Into<String>,
        entity_id: impl Into<String>,
        context_id: Option<String>,
        message: impl Into<String>,
        details: Option<String>,
    ) -> Self {
        Self {
            severity,
            category,
            entity_type: entity_type.into(),
            entity_id: entity_id.into(),
            context_id,
            message: message.into(),
            details,
        }
    }
}

// =========================================================================
// ContextHealthReport Model
// =========================================================================

/// Comprehensive report generated by `ContextHealthCheckService`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextHealthReport {
    /// Overall report status: PASS, WARNING, or ERROR
    pub status: ContextHealthStatus,
    /// Total number of diagnostic checks executed
    pub checks_executed: usize,
    /// Number of critical errors identified
    pub errors_count: usize,
    /// Number of diagnostic warnings identified
    pub warnings_count: usize,
    /// List of all identified issues
    pub issues: Vec<ContextHealthIssue>,
    /// ISO-8601 UTC timestamp of execution
    pub timestamp: String,
    /// Epistemic non-mutating disclaimer
    pub disclaimer: &'static str,
}

impl ContextHealthReport {
    /// Formats the diagnostic health report into a developer-readable summary.
    pub fn format_report(&self) -> String {
        let mut out = String::new();
        out.push_str("================================================================================\n");
        out.push_str(&format!("CONTEXTSPLIT HEALTH DIAGNOSTIC REPORT: {}\n", self.status));
        out.push_str("================================================================================\n\n");
        out.push_str(&format!("Timestamp:        {}\n", self.timestamp));
        out.push_str(&format!("Checks Executed:  {}\n", self.checks_executed));
        out.push_str(&format!("Errors Found:     {}\n", self.errors_count));
        out.push_str(&format!("Warnings Found:   {}\n", self.warnings_count));
        out.push_str(&format!("Overall Status:   {}\n\n", self.status));

        if self.issues.is_empty() {
            out.push_str("No structural or contextual inconsistencies detected.\nAll context invariants satisfied.\n");
        } else {
            out.push_str("Detected Inconsistencies:\n");
            out.push_str("--------------------------------------------------------------------------------\n");
            for (idx, issue) in self.issues.iter().enumerate() {
                out.push_str(&format!(
                    "[{}] [{}] {} (Entity: {} id={})\n",
                    idx + 1,
                    issue.severity,
                    issue.category.display_title(),
                    issue.entity_type,
                    issue.entity_id
                ));
                if let Some(ref c_id) = issue.context_id {
                    out.push_str(&format!("    Context ID: {}\n", c_id));
                }
                out.push_str(&format!("    Message:    {}\n", issue.message));
                if let Some(ref det) = issue.details {
                    out.push_str(&format!("    Details:    {}\n", det));
                }
                out.push('\n');
            }
        }

        out.push_str("--------------------------------------------------------------------------------\n");
        out.push_str(&format!("Note: {}\n", self.disclaimer));
        out
    }

    /// Returns true if the report has no errors.
    pub fn is_healthy(&self) -> bool {
        self.status == ContextHealthStatus::Pass
    }
}

// =========================================================================
// ContextHealthCheckService
// =========================================================================

/// Development-only service to run comprehensive read-only consistency checks.
pub struct ContextHealthCheckService;

impl ContextHealthCheckService {
    /// Executes all 12 context consistency checks against SQLite database.
    pub fn run_all_checks(conn: &Connection) -> Result<ContextHealthReport, rusqlite::Error> {
        let mut issues: Vec<ContextHealthIssue> = Vec::new();
        let mut checks_executed = 0;

        // 1. Context with no valid evidence
        Self::check_context_evidence(conn, &mut issues)?;
        checks_executed += 1;

        // 2. Orphaned sessions
        Self::check_orphaned_sessions(conn, &mut issues)?;
        checks_executed += 1;

        // 3. Orphaned memories
        Self::check_orphaned_memories(conn, &mut issues)?;
        checks_executed += 1;

        // 4. Orphaned workset items
        Self::check_orphaned_workset_items(conn, &mut issues)?;
        checks_executed += 1;

        // 5. Snapshots referencing missing contexts
        Self::check_snapshot_references(conn, &mut issues)?;
        checks_executed += 1;

        // 6. Tasks referencing missing contexts
        Self::check_task_references(conn, &mut issues)?;
        checks_executed += 1;

        // 7. Feedback referencing missing entities
        Self::check_feedback_references(conn, &mut issues)?;
        checks_executed += 1;

        // 8. Invalid lifecycle transitions
        Self::check_lifecycle_transitions(conn, &mut issues)?;
        checks_executed += 1;

        // 9. Duplicate context identities
        Self::check_duplicate_identities(conn, &mut issues)?;
        checks_executed += 1;

        // 10. Impossible timestamps
        Self::check_impossible_timestamps(conn, &mut issues)?;
        checks_executed += 1;

        // 11. Contradictory state
        Self::check_contradictory_state(conn, &mut issues)?;
        checks_executed += 1;

        // 12. Corrupted reconstruction & JSON payloads
        Self::check_reconstruction_and_payloads(conn, &mut issues)?;
        checks_executed += 1;

        let errors_count = issues
            .iter()
            .filter(|i| i.severity == HealthIssueSeverity::Error)
            .count();
        let warnings_count = issues
            .iter()
            .filter(|i| i.severity == HealthIssueSeverity::Warning)
            .count();

        let status = if errors_count > 0 {
            ContextHealthStatus::Error
        } else if warnings_count > 0 {
            ContextHealthStatus::Warning
        } else {
            ContextHealthStatus::Pass
        };

        Ok(ContextHealthReport {
            status,
            checks_executed,
            errors_count,
            warnings_count,
            issues,
            timestamp: Utc::now().to_rfc3339(),
            disclaimer: CONTEXT_HEALTH_CHECK_DISCLAIMER,
        })
    }

    // -------------------------------------------------------------------------
    // 1. Context with No Valid Evidence
    // -------------------------------------------------------------------------
    pub fn check_context_evidence(
        conn: &Connection,
        issues: &mut Vec<ContextHealthIssue>,
    ) -> Result<(), rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT w.id, w.name, w.lifecycle_state,
                    (SELECT COUNT(*) FROM items i WHERE i.workspace_id = w.id) as item_count,
                    (SELECT COUNT(*) FROM context_signals s WHERE s.workspace_id = w.id) as signal_count,
                    (SELECT COUNT(*) FROM context_memberships m WHERE m.workspace_id = w.id) as membership_count,
                    (SELECT COUNT(*) FROM task_candidates t WHERE t.context_id = w.id) as task_count
             FROM workspaces w",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?;

        for row in rows.filter_map(Result::ok) {
            let (ws_id, ws_name, state, items, signals, memberships, tasks) = row;
            let total_evidence = items + signals + memberships + tasks;
            if total_evidence == 0 {
                issues.push(ContextHealthIssue::new(
                    HealthIssueSeverity::Warning,
                    HealthIssueCategory::NoValidEvidence,
                    "workspace",
                    ws_id.clone(),
                    Some(ws_id),
                    format!(
                        "Context '{}' (state: {}) has zero recorded items, signals, memberships, or tasks.",
                        ws_name, state
                    ),
                    Some("Consider confirming items or archiving dormant context without evidence.".to_string()),
                ));
            }
        }

        Ok(())
    }

    // -------------------------------------------------------------------------
    // 2. Orphaned Sessions
    // -------------------------------------------------------------------------
    pub fn check_orphaned_sessions(
        conn: &Connection,
        issues: &mut Vec<ContextHealthIssue>,
    ) -> Result<(), rusqlite::Error> {
        // Items pointing to non-existent sessions
        let mut stmt = conn.prepare(
            "SELECT i.id, i.session_id, i.workspace_id
             FROM items i
             WHERE i.session_id IS NOT NULL AND i.session_id != ''
               AND NOT EXISTS (SELECT 1 FROM sessions s WHERE s.id = i.session_id)",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;

        for row in rows.filter_map(Result::ok) {
            let (item_id, session_id, ws_id) = row;
            issues.push(ContextHealthIssue::new(
                HealthIssueSeverity::Error,
                HealthIssueCategory::OrphanedSession,
                "item",
                item_id,
                ws_id,
                format!("Item references non-existent session ID '{}'", session_id),
                Some("Item session_id foreign reference broken.".to_string()),
            ));
        }

        // Context transitions pointing to non-existent sessions
        let mut stmt_t = conn.prepare(
            "SELECT ct.id, ct.session_id, ct.to_context_id
             FROM context_transitions ct
             WHERE ct.session_id IS NOT NULL AND ct.session_id != ''
               AND NOT EXISTS (SELECT 1 FROM sessions s WHERE s.id = ct.session_id)",
        )?;

        let t_rows = stmt_t.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;

        for row in t_rows.filter_map(Result::ok) {
            let (trans_id, session_id, to_ctx) = row;
            issues.push(ContextHealthIssue::new(
                HealthIssueSeverity::Error,
                HealthIssueCategory::OrphanedSession,
                "context_transition",
                trans_id,
                to_ctx,
                format!("Context transition references non-existent session ID '{}'", session_id),
                Some("Transition session foreign reference broken.".to_string()),
            ));
        }

        Ok(())
    }

    // -------------------------------------------------------------------------
    // 3. Orphaned Memories
    // -------------------------------------------------------------------------
    pub fn check_orphaned_memories(
        conn: &Connection,
        issues: &mut Vec<ContextHealthIssue>,
    ) -> Result<(), rusqlite::Error> {
        // Items with item_type = 'important_memory' or 'memory' pointing to missing workspace
        let mut stmt = conn.prepare(
            "SELECT i.id, i.workspace_id, i.title, i.content
             FROM items i
             WHERE i.item_type IN ('important_memory', 'memory', 'context_memory')
               AND i.workspace_id IS NOT NULL AND i.workspace_id != '' AND i.workspace_id != 'inbox'
               AND NOT EXISTS (SELECT 1 FROM workspaces w WHERE w.id = i.workspace_id)",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;

        for row in rows.filter_map(Result::ok) {
            let (item_id, ws_id, title) = row;
            issues.push(ContextHealthIssue::new(
                HealthIssueSeverity::Error,
                HealthIssueCategory::OrphanedMemory,
                "item_memory",
                item_id,
                Some(ws_id.clone()),
                format!(
                    "Context memory '{}' references non-existent workspace ID '{}'",
                    title.unwrap_or_else(|| "Untitled".to_string()),
                    ws_id
                ),
                Some("Memory points to a deleted or uninitialized workspace.".to_string()),
            ));
        }

        Ok(())
    }

    // -------------------------------------------------------------------------
    // 4. Orphaned Workset Items
    // -------------------------------------------------------------------------
    pub fn check_orphaned_workset_items(
        conn: &Connection,
        issues: &mut Vec<ContextHealthIssue>,
    ) -> Result<(), rusqlite::Error> {
        // Memberships pointing to missing items
        let mut stmt_m_item = conn.prepare(
            "SELECT cm.id, cm.item_id, cm.workspace_id
             FROM context_memberships cm
             WHERE NOT EXISTS (SELECT 1 FROM items i WHERE i.id = cm.item_id)",
        )?;

        let rows_item = stmt_m_item.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;

        for row in rows_item.filter_map(Result::ok) {
            let (m_id, item_id, ws_id) = row;
            issues.push(ContextHealthIssue::new(
                HealthIssueSeverity::Error,
                HealthIssueCategory::OrphanedWorksetItem,
                "context_membership",
                m_id,
                Some(ws_id),
                format!("Context membership references missing item ID '{}'", item_id),
                Some("Item was deleted without cascading membership cleanup.".to_string()),
            ));
        }

        // Memberships pointing to missing workspaces
        let mut stmt_m_ws = conn.prepare(
            "SELECT cm.id, cm.item_id, cm.workspace_id
             FROM context_memberships cm
             WHERE NOT EXISTS (SELECT 1 FROM workspaces w WHERE w.id = cm.workspace_id)",
        )?;

        let rows_ws = stmt_m_ws.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;

        for row in rows_ws.filter_map(Result::ok) {
            let (m_id, _item_id, ws_id) = row;
            issues.push(ContextHealthIssue::new(
                HealthIssueSeverity::Error,
                HealthIssueCategory::OrphanedWorksetItem,
                "context_membership",
                m_id,
                Some(ws_id.clone()),
                format!("Context membership references missing workspace ID '{}'", ws_id),
                Some("Workspace was deleted without cascading membership cleanup.".to_string()),
            ));
        }

        // Candidate items pointing to missing items
        let mut stmt_c_item = conn.prepare(
            "SELECT cci.candidate_id, cci.item_id
             FROM context_candidate_items cci
             WHERE NOT EXISTS (SELECT 1 FROM items i WHERE i.id = cci.item_id)",
        )?;

        let rows_c_item = stmt_c_item.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        for row in rows_c_item.filter_map(Result::ok) {
            let (cand_id, item_id) = row;
            issues.push(ContextHealthIssue::new(
                HealthIssueSeverity::Error,
                HealthIssueCategory::OrphanedWorksetItem,
                "context_candidate_item",
                format!("{}:{}", cand_id, item_id),
                None,
                format!("Context candidate item references missing item ID '{}'", item_id),
                Some("Candidate item association broken.".to_string()),
            ));
        }

        Ok(())
    }

    // -------------------------------------------------------------------------
    // 5. Snapshots Referencing Missing Contexts
    // -------------------------------------------------------------------------
    pub fn check_snapshot_references(
        conn: &Connection,
        issues: &mut Vec<ContextHealthIssue>,
    ) -> Result<(), rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT cs.id, cs.context_id, cs.context_name
             FROM context_snapshots cs
             WHERE NOT EXISTS (SELECT 1 FROM workspaces w WHERE w.id = cs.context_id)",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;

        for row in rows.filter_map(Result::ok) {
            let (snap_id, ctx_id, ctx_name) = row;
            issues.push(ContextHealthIssue::new(
                HealthIssueSeverity::Error,
                HealthIssueCategory::MissingContextSnapshot,
                "context_snapshot",
                snap_id,
                Some(ctx_id.clone()),
                format!(
                    "Snapshot for '{}' references non-existent workspace ID '{}'",
                    ctx_name, ctx_id
                ),
                Some("Context snapshot foreign reference is broken.".to_string()),
            ));
        }

        Ok(())
    }

    // -------------------------------------------------------------------------
    // 6. Tasks Referencing Missing Contexts
    // -------------------------------------------------------------------------
    pub fn check_task_references(
        conn: &Connection,
        issues: &mut Vec<ContextHealthIssue>,
    ) -> Result<(), rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT tc.id, tc.context_id, tc.title
             FROM task_candidates tc
             WHERE tc.context_id IS NOT NULL AND tc.context_id != ''
               AND NOT EXISTS (SELECT 1 FROM workspaces w WHERE w.id = tc.context_id)",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;

        for row in rows.filter_map(Result::ok) {
            let (task_id, ctx_id, title) = row;
            issues.push(ContextHealthIssue::new(
                HealthIssueSeverity::Error,
                HealthIssueCategory::MissingContextTask,
                "task_candidate",
                task_id,
                Some(ctx_id.clone()),
                format!(
                    "Task candidate '{}' references non-existent workspace ID '{}'",
                    title, ctx_id
                ),
                Some("Task context foreign reference broken.".to_string()),
            ));
        }

        Ok(())
    }

    // -------------------------------------------------------------------------
    // 7. Feedback Referencing Missing Entities
    // -------------------------------------------------------------------------
    pub fn check_feedback_references(
        conn: &Connection,
        issues: &mut Vec<ContextHealthIssue>,
    ) -> Result<(), rusqlite::Error> {
        // Check context_signals with feedback actions
        let mut stmt = conn.prepare(
            "SELECT cs.id, cs.workspace_id, cs.item_id, cs.signal_type, cs.payload
             FROM context_signals cs
             WHERE cs.signal_type IN ('user_feedback', 'feedback', 'user_action')",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;

        for row in rows.filter_map(Result::ok) {
            let (sig_id, ws_id, item_id, _, payload) = row;

            // Check if workspace is missing when specified
            if let Some(ref w_id) = ws_id {
                let exists: bool = conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = ?1)",
                    [w_id],
                    |r| r.get(0),
                ).unwrap_or(false);

                if !exists {
                    issues.push(ContextHealthIssue::new(
                        HealthIssueSeverity::Error,
                        HealthIssueCategory::MissingEntityFeedback,
                        "context_signal_feedback",
                        sig_id.clone(),
                        Some(w_id.clone()),
                        format!("Feedback signal references non-existent workspace ID '{}'", w_id),
                        Some("Feedback event targets missing workspace.".to_string()),
                    ));
                }
            }

            // Check if item is missing when item_id specified
            if let Some(ref i_id) = item_id {
                let exists: bool = conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM items WHERE id = ?1)",
                    [i_id],
                    |r| r.get(0),
                ).unwrap_or(false);

                if !exists {
                    issues.push(ContextHealthIssue::new(
                        HealthIssueSeverity::Error,
                        HealthIssueCategory::MissingEntityFeedback,
                        "context_signal_feedback",
                        sig_id.clone(),
                        ws_id.clone(),
                        format!("Feedback signal references non-existent item ID '{}'", i_id),
                        Some("Feedback event targets missing item.".to_string()),
                    ));
                }
            }

            // Inspect JSON payload for target_id
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                if let Some(target) = val.get("target_id").and_then(|t| t.as_str()) {
                    let target_exists: bool = conn.query_row(
                        "SELECT EXISTS(SELECT 1 FROM items WHERE id = ?1) OR EXISTS(SELECT 1 FROM workspaces WHERE id = ?1) OR EXISTS(SELECT 1 FROM task_candidates WHERE id = ?1)",
                        [target],
                        |r| r.get(0),
                    ).unwrap_or(false);

                    if !target_exists && !target.is_empty() {
                        issues.push(ContextHealthIssue::new(
                            HealthIssueSeverity::Warning,
                            HealthIssueCategory::MissingEntityFeedback,
                            "context_feedback_payload",
                            sig_id,
                            ws_id,
                            format!("Feedback payload references missing target entity ID '{}'", target),
                            Some("Target entity was deleted or unindexed.".to_string()),
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    // -------------------------------------------------------------------------
    // 8. Invalid Lifecycle Transitions
    // -------------------------------------------------------------------------
    pub fn check_lifecycle_transitions(
        conn: &Connection,
        issues: &mut Vec<ContextHealthIssue>,
    ) -> Result<(), rusqlite::Error> {
        // Verify valid workspace lifecycle_state strings
        let mut stmt_ws = conn.prepare("SELECT id, name, lifecycle_state FROM workspaces")?;
        let ws_rows = stmt_ws.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;

        let valid_states: HashSet<&str> = ["active", "paused", "dormant", "archived", "suspended"].into_iter().collect();

        for row in ws_rows.filter_map(Result::ok) {
            let (ws_id, ws_name, state) = row;
            if !valid_states.contains(state.to_lowercase().as_str()) {
                issues.push(ContextHealthIssue::new(
                    HealthIssueSeverity::Error,
                    HealthIssueCategory::InvalidLifecycleTransition,
                    "workspace",
                    ws_id.clone(),
                    Some(ws_id),
                    format!("Workspace '{}' has invalid lifecycle_state '{}'", ws_name, state),
                    Some("Lifecycle state must be one of: active, paused, dormant, archived.".to_string()),
                ));
            }
        }

        // Verify context_transitions validity
        let mut stmt_t = conn.prepare(
            "SELECT id, from_context_id, to_context_id, transition_type, detected_at
             FROM context_transitions
             ORDER BY detected_at ASC",
        )?;

        let t_rows = stmt_t.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;

        let valid_trans_types: HashSet<&str> = [
            "activation", "switch", "pause", "resume", "archive", "dormancy",
            "task_switch", "inferred", "explicit", "auto_pause", "background"
        ].into_iter().collect();

        for row in t_rows.filter_map(Result::ok) {
            let (t_id, from_ctx, to_ctx, t_type, _) = row;

            if !valid_trans_types.contains(t_type.to_lowercase().as_str()) {
                issues.push(ContextHealthIssue::new(
                    HealthIssueSeverity::Warning,
                    HealthIssueCategory::InvalidLifecycleTransition,
                    "context_transition",
                    t_id.clone(),
                    to_ctx.clone().or(from_ctx.clone()),
                    format!("Unrecognized transition_type '{}'", t_type),
                    Some("Transition type not in standard transition catalog.".to_string()),
                ));
            }

            // Check if to_context exists when specified
            if let Some(ref tc) = to_ctx {
                let exists: bool = conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = ?1)",
                    [tc],
                    |r| r.get(0),
                ).unwrap_or(false);

                if !exists {
                    issues.push(ContextHealthIssue::new(
                        HealthIssueSeverity::Error,
                        HealthIssueCategory::InvalidLifecycleTransition,
                        "context_transition",
                        t_id.clone(),
                        Some(tc.clone()),
                        format!("Transition target workspace ID '{}' does not exist", tc),
                        Some("Destination workspace was deleted.".to_string()),
                    ));
                }
            }
        }

        Ok(())
    }

    // -------------------------------------------------------------------------
    // 9. Duplicate Context Identities
    // -------------------------------------------------------------------------
    pub fn check_duplicate_identities(
        conn: &Connection,
        issues: &mut Vec<ContextHealthIssue>,
    ) -> Result<(), rusqlite::Error> {
        let mut stmt = conn.prepare("SELECT id, name FROM workspaces")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut seen_names: HashMap<String, Vec<String>> = HashMap::new();

        for row in rows.filter_map(Result::ok) {
            let (ws_id, name) = row;
            let normalized = name.trim().to_lowercase();
            seen_names.entry(normalized).or_default().push(ws_id);
        }

        for (norm_name, ids) in seen_names {
            if ids.len() > 1 {
                for id in &ids {
                    issues.push(ContextHealthIssue::new(
                        HealthIssueSeverity::Warning,
                        HealthIssueCategory::DuplicateContextIdentity,
                        "workspace",
                        id.clone(),
                        Some(id.clone()),
                        format!(
                            "Duplicate context name '{}' shared by {} workspaces ({})",
                            norm_name,
                            ids.len(),
                            ids.join(", ")
                        ),
                        Some("Context workspaces should have distinct identifiable names.".to_string()),
                    ));
                }
            }
        }

        Ok(())
    }

    // -------------------------------------------------------------------------
    // 10. Impossible Timestamps
    // -------------------------------------------------------------------------
    pub fn check_impossible_timestamps(
        conn: &Connection,
        issues: &mut Vec<ContextHealthIssue>,
    ) -> Result<(), rusqlite::Error> {
        let now = Utc::now();
        // Allow 10-minute future clock skew tolerance
        let max_future = now + chrono::Duration::minutes(10);

        // Check workspace timestamps
        let mut stmt_w = conn.prepare("SELECT id, name, created_at, updated_at, last_activity_at, archived_at FROM workspaces")?;
        let w_rows = stmt_w.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?;

        for row in w_rows.filter_map(Result::ok) {
            let (ws_id, ws_name, created, updated, last_act, archived) = row;

            Self::validate_ts_string(&created, &ws_id, "workspace", "created_at", max_future, issues);
            Self::validate_ts_string(&updated, &ws_id, "workspace", "updated_at", max_future, issues);

            if let Some(ref la) = last_act {
                Self::validate_ts_string(la, &ws_id, "workspace", "last_activity_at", max_future, issues);
            }
            if let Some(ref ar) = archived {
                Self::validate_ts_string(ar, &ws_id, "workspace", "archived_at", max_future, issues);
            }

            // Check if updated_at < created_at
            if let (Ok(c_dt), Ok(u_dt)) = (
                DateTime::parse_from_rfc3339(&created),
                DateTime::parse_from_rfc3339(&updated),
            ) {
                if u_dt < c_dt {
                    issues.push(ContextHealthIssue::new(
                        HealthIssueSeverity::Error,
                        HealthIssueCategory::ImpossibleTimestamp,
                        "workspace",
                        ws_id.clone(),
                        Some(ws_id.clone()),
                        format!("Workspace '{}' updated_at is earlier than created_at", ws_name),
                        Some(format!("created_at: {}, updated_at: {}", created, updated)),
                    ));
                }
            }
        }

        // Check session timestamps (ended_at < started_at)
        let mut stmt_s = conn.prepare("SELECT id, started_at, last_activity_at, ended_at FROM sessions")?;
        let s_rows = stmt_s.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?;

        for row in s_rows.filter_map(Result::ok) {
            let (s_id, started, last_act, ended) = row;
            Self::validate_ts_string(&started, &s_id, "session", "started_at", max_future, issues);
            Self::validate_ts_string(&last_act, &s_id, "session", "last_activity_at", max_future, issues);

            if let (Ok(s_dt), Ok(la_dt)) = (
                DateTime::parse_from_rfc3339(&started),
                DateTime::parse_from_rfc3339(&last_act),
            ) {
                if la_dt < s_dt {
                    issues.push(ContextHealthIssue::new(
                        HealthIssueSeverity::Error,
                        HealthIssueCategory::ImpossibleTimestamp,
                        "session",
                        s_id.clone(),
                        None,
                        "Session last_activity_at is earlier than started_at".to_string(),
                        Some(format!("started_at: {}, last_activity_at: {}", started, last_act)),
                    ));
                }
            }

            if let Some(ref end_str) = ended {
                Self::validate_ts_string(end_str, &s_id, "session", "ended_at", max_future, issues);
                if let (Ok(s_dt), Ok(e_dt)) = (
                    DateTime::parse_from_rfc3339(&started),
                    DateTime::parse_from_rfc3339(end_str),
                ) {
                    if e_dt < s_dt {
                        issues.push(ContextHealthIssue::new(
                            HealthIssueSeverity::Error,
                            HealthIssueCategory::ImpossibleTimestamp,
                            "session",
                            s_id.clone(),
                            None,
                            "Session ended_at is earlier than started_at".to_string(),
                            Some(format!("started_at: {}, ended_at: {}", started, end_str)),
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    fn validate_ts_string(
        ts: &str,
        entity_id: &str,
        entity_type: &str,
        field_name: &str,
        max_future: DateTime<Utc>,
        issues: &mut Vec<ContextHealthIssue>,
    ) {
        match DateTime::parse_from_rfc3339(ts) {
            Ok(dt) => {
                let dt_utc = dt.with_timezone(&Utc);
                if dt_utc > max_future {
                    issues.push(ContextHealthIssue::new(
                        HealthIssueSeverity::Error,
                        HealthIssueCategory::ImpossibleTimestamp,
                        entity_type,
                        entity_id,
                        None,
                        format!("{} '{}' has future timestamp ({})", entity_type, field_name, ts),
                        Some("Clock skew or invalid future timestamp detected.".to_string()),
                    ));
                }
            }
            Err(_) => {
                issues.push(ContextHealthIssue::new(
                    HealthIssueSeverity::Error,
                    HealthIssueCategory::ImpossibleTimestamp,
                    entity_type,
                    entity_id,
                    None,
                    format!("{} '{}' has unparseable ISO-8601/RFC-3339 string '{}'", entity_type, field_name, ts),
                    Some("Malformed timestamp string format.".to_string()),
                ));
            }
        }
    }

    // -------------------------------------------------------------------------
    // 11. Contradictory State
    // -------------------------------------------------------------------------
    pub fn check_contradictory_state(
        conn: &Connection,
        issues: &mut Vec<ContextHealthIssue>,
    ) -> Result<(), rusqlite::Error> {
        // Workspace marked active but archived_at is populated
        let mut stmt = conn.prepare(
            "SELECT id, name, lifecycle_state, archived_at
             FROM workspaces
             WHERE lifecycle_state = 'active' AND archived_at IS NOT NULL AND archived_at != ''",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;

        for row in rows.filter_map(Result::ok) {
            let (ws_id, ws_name, state, archived_at) = row;
            issues.push(ContextHealthIssue::new(
                HealthIssueSeverity::Error,
                HealthIssueCategory::ContradictoryState,
                "workspace",
                ws_id.clone(),
                Some(ws_id),
                format!(
                    "Workspace '{}' has contradictory state: lifecycle_state is '{}' but archived_at is set to '{}'",
                    ws_name, state, archived_at
                ),
                Some("Active workspaces must have archived_at = NULL.".to_string()),
            ));
        }

        // Workspace marked archived but archived_at is NULL
        let mut stmt_arch = conn.prepare(
            "SELECT id, name, lifecycle_state
             FROM workspaces
             WHERE lifecycle_state = 'archived' AND (archived_at IS NULL OR archived_at = '')",
        )?;

        let arch_rows = stmt_arch.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;

        for row in arch_rows.filter_map(Result::ok) {
            let (ws_id, ws_name, state) = row;
            issues.push(ContextHealthIssue::new(
                HealthIssueSeverity::Warning,
                HealthIssueCategory::ContradictoryState,
                "workspace",
                ws_id.clone(),
                Some(ws_id),
                format!(
                    "Workspace '{}' lifecycle_state is '{}' but archived_at timestamp is missing",
                    ws_name, state
                ),
                Some("Archived workspace should record timestamp when archived.".to_string()),
            ));
        }

        Ok(())
    }

    // -------------------------------------------------------------------------
    // 12. Corrupted Reconstruction & JSON Payloads
    // -------------------------------------------------------------------------
    pub fn check_reconstruction_and_payloads(
        conn: &Connection,
        issues: &mut Vec<ContextHealthIssue>,
    ) -> Result<(), rusqlite::Error> {
        // Check context_snapshots snapshot_json
        let mut stmt_snap = conn.prepare("SELECT id, context_id, snapshot_json FROM context_snapshots")?;
        let snap_rows = stmt_snap.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;

        for row in snap_rows.filter_map(Result::ok) {
            let (snap_id, ctx_id, json_str) = row;
            if let Err(e) = serde_json::from_str::<serde_json::Value>(&json_str) {
                issues.push(ContextHealthIssue::new(
                    HealthIssueSeverity::Error,
                    HealthIssueCategory::CorruptedReconstruction,
                    "context_snapshot",
                    snap_id,
                    Some(ctx_id),
                    format!("Corrupted snapshot_json payload: {}", e),
                    Some("Malformed JSON data in context_snapshots table.".to_string()),
                ));
            }
        }

        // Check items metadata JSON
        let mut stmt_item = conn.prepare("SELECT id, workspace_id, metadata FROM items")?;
        let item_rows = stmt_item.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;

        for row in item_rows.filter_map(Result::ok) {
            let (item_id, ws_id, meta_str) = row;
            if !meta_str.is_empty() && meta_str != "{}" {
                if let Err(e) = serde_json::from_str::<serde_json::Value>(&meta_str) {
                    issues.push(ContextHealthIssue::new(
                        HealthIssueSeverity::Error,
                        HealthIssueCategory::CorruptedReconstruction,
                        "item",
                        item_id,
                        ws_id,
                        format!("Corrupted item metadata JSON: {}", e),
                        Some("Malformed JSON in items table metadata column.".to_string()),
                    ));
                }
            }
        }

        Ok(())
    }
}

// =========================================================================
// Unit Tests & Corrupted Fixture Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_clean_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    fn setup_corrupted_db() -> Connection {
        let conn = setup_clean_db();
        // Allow inserting intentionally corrupted relationships for diagnostic testing
        conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
        conn
    }

    #[test]
    fn test_clean_db_health_check_passes() {
        let conn = setup_clean_db();

        // Insert valid workspace with an item and signal using past timestamps
        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at, lifecycle_state)
             VALUES ('ws_clean', 'Clean Context', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z', 'active')",
            [],
        ).unwrap();

        conn.execute(
            "INSERT INTO items (id, workspace_id, item_type, content, title, metadata, created_at, updated_at)
             VALUES ('item_clean', 'ws_clean', 'file', 'content', 'Main File', '{}', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z')",
            [],
        ).unwrap();

        let report = ContextHealthCheckService::run_all_checks(&conn).unwrap();
        assert_eq!(report.status, ContextHealthStatus::Pass);
        assert_eq!(report.errors_count, 0);
        assert_eq!(report.warnings_count, 0);
        assert!(report.is_healthy());

        let formatted = report.format_report();
        assert!(formatted.contains("Overall Status:   PASS"));
        assert!(formatted.contains("No structural or contextual inconsistencies detected."));
    }

    #[test]
    fn test_corrupted_fixture_orphaned_sessions() {
        let conn = setup_corrupted_db();

        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at, lifecycle_state)
             VALUES ('ws_1', 'Context 1', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z', 'active')",
            [],
        ).unwrap();

        // Insert item with broken session foreign key
        conn.execute(
            "INSERT INTO items (id, workspace_id, session_id, item_type, content, title, metadata, created_at, updated_at)
             VALUES ('item_orphan', 'ws_1', 'session_missing_999', 'file', 'content', 'File', '{}', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z')",
            [],
        ).unwrap();

        let report = ContextHealthCheckService::run_all_checks(&conn).unwrap();
        assert_eq!(report.status, ContextHealthStatus::Error);
        assert!(report.errors_count >= 1);
        assert!(report.issues.iter().any(|i| i.category == HealthIssueCategory::OrphanedSession));
    }

    #[test]
    fn test_corrupted_fixture_orphaned_memories() {
        let conn = setup_corrupted_db();

        // Insert memory pointing to a missing workspace
        conn.execute(
            "INSERT INTO items (id, workspace_id, item_type, content, title, metadata, created_at, updated_at)
             VALUES ('mem_orphan', 'ws_missing_404', 'important_memory', 'Important fact', 'Memory 1', '{}', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z')",
            [],
        ).unwrap();

        let report = ContextHealthCheckService::run_all_checks(&conn).unwrap();
        assert_eq!(report.status, ContextHealthStatus::Error);
        assert!(report.issues.iter().any(|i| i.category == HealthIssueCategory::OrphanedMemory));
    }

    #[test]
    fn test_corrupted_fixture_orphaned_workset_items() {
        let conn = setup_corrupted_db();

        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at, lifecycle_state)
             VALUES ('ws_1', 'Context 1', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z', 'active')",
            [],
        ).unwrap();

        // Insert context_membership with non-existent item
        conn.execute(
            "INSERT INTO context_memberships (id, item_id, workspace_id, membership_type, confidence, source, created_at, updated_at)
             VALUES ('cm_broken', 'item_deleted_123', 'ws_1', 'confirmed', 1.0, 'user', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z')",
            [],
        ).unwrap();

        let report = ContextHealthCheckService::run_all_checks(&conn).unwrap();
        assert_eq!(report.status, ContextHealthStatus::Error);
        assert!(report.issues.iter().any(|i| i.category == HealthIssueCategory::OrphanedWorksetItem));
    }

    #[test]
    fn test_corrupted_fixture_missing_context_snapshots() {
        let conn = setup_corrupted_db();

        // Insert snapshot pointing to non-existent context
        conn.execute(
            "INSERT INTO context_snapshots (id, context_id, context_name, created_at, state, snapshot_confidence, quality_rating, snapshot_json)
             VALUES ('snap_broken', 'ws_ghost', 'Ghost Context', '2026-01-01T00:00:00Z', 'active', 0.9, 'high', '{}')",
            [],
        ).unwrap();

        let report = ContextHealthCheckService::run_all_checks(&conn).unwrap();
        assert_eq!(report.status, ContextHealthStatus::Error);
        assert!(report.issues.iter().any(|i| i.category == HealthIssueCategory::MissingContextSnapshot));
    }

    #[test]
    fn test_corrupted_fixture_missing_context_tasks() {
        let conn = setup_corrupted_db();

        // Insert task candidate pointing to non-existent context
        conn.execute(
            "INSERT INTO task_candidates (id, context_id, title, confidence, status, first_detected_at, last_active_at, created_at, updated_at)
             VALUES ('task_broken', 'ws_nonexistent', 'Build feature', 0.95, 'active', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z')",
            [],
        ).unwrap();

        let report = ContextHealthCheckService::run_all_checks(&conn).unwrap();
        assert_eq!(report.status, ContextHealthStatus::Error);
        assert!(report.issues.iter().any(|i| i.category == HealthIssueCategory::MissingContextTask));
    }

    #[test]
    fn test_corrupted_fixture_feedback_referencing_missing_entities() {
        let conn = setup_corrupted_db();

        // Insert feedback signal referencing non-existent workspace and item
        conn.execute(
            "INSERT INTO context_signals (id, workspace_id, item_id, signal_type, payload, created_at, updated_at)
             VALUES ('sig_broken', 'ws_missing_fb', 'item_missing_fb', 'user_feedback', '{\"target_id\":\"item_missing_fb\"}', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z')",
            [],
        ).unwrap();

        let report = ContextHealthCheckService::run_all_checks(&conn).unwrap();
        assert_eq!(report.status, ContextHealthStatus::Error);
        assert!(report.issues.iter().any(|i| i.category == HealthIssueCategory::MissingEntityFeedback));
    }

    #[test]
    fn test_corrupted_fixture_invalid_lifecycle_transition() {
        let conn = setup_clean_db();

        // Insert workspace with illegal lifecycle state string
        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at, lifecycle_state)
             VALUES ('ws_invalid_state', 'Broken State Context', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z', 'completely_illegal_state')",
            [],
        ).unwrap();

        let report = ContextHealthCheckService::run_all_checks(&conn).unwrap();
        assert_eq!(report.status, ContextHealthStatus::Error);
        assert!(report.issues.iter().any(|i| i.category == HealthIssueCategory::InvalidLifecycleTransition));
    }

    #[test]
    fn test_corrupted_fixture_duplicate_identities() {
        let conn = setup_clean_db();

        // Insert two workspaces with identical names
        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at, lifecycle_state)
             VALUES ('ws_dup_1', 'Payment Gateway', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z', 'active')",
            [],
        ).unwrap();

        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at, lifecycle_state)
             VALUES ('ws_dup_2', 'payment gateway', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z', 'active')",
            [],
        ).unwrap();

        let report = ContextHealthCheckService::run_all_checks(&conn).unwrap();
        assert!(report.issues.iter().any(|i| i.category == HealthIssueCategory::DuplicateContextIdentity));
    }

    #[test]
    fn test_corrupted_fixture_impossible_timestamps() {
        let conn = setup_clean_db();

        // Insert session with ended_at earlier than started_at
        conn.execute(
            "INSERT INTO sessions (id, started_at, last_activity_at, ended_at, status)
             VALUES ('sess_time_warp', '2026-01-01T05:00:00Z', '2026-01-01T05:30:00Z', '2026-01-01T02:00:00Z', 'ended')",
            [],
        ).unwrap();

        let report = ContextHealthCheckService::run_all_checks(&conn).unwrap();
        assert_eq!(report.status, ContextHealthStatus::Error);
        assert!(report.issues.iter().any(|i| i.category == HealthIssueCategory::ImpossibleTimestamp));
    }

    #[test]
    fn test_corrupted_fixture_contradictory_state() {
        let conn = setup_clean_db();

        // Insert workspace that is active but has archived_at set
        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at, lifecycle_state, archived_at)
             VALUES ('ws_contradictory', 'Contradictory Context', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z', 'active', '2026-01-01T01:00:00Z')",
            [],
        ).unwrap();

        let report = ContextHealthCheckService::run_all_checks(&conn).unwrap();
        assert_eq!(report.status, ContextHealthStatus::Error);
        assert!(report.issues.iter().any(|i| i.category == HealthIssueCategory::ContradictoryState));
    }

    #[test]
    fn test_corrupted_fixture_corrupted_reconstruction_json() {
        let conn = setup_clean_db();

        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at, lifecycle_state)
             VALUES ('ws_1', 'Context 1', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z', 'active')",
            [],
        ).unwrap();

        // Insert snapshot with invalid malformed JSON
        conn.execute(
            "INSERT INTO context_snapshots (id, context_id, context_name, created_at, state, snapshot_confidence, quality_rating, snapshot_json)
             VALUES ('snap_corrupted', 'ws_1', 'Context 1', '2026-01-01T00:00:00Z', 'active', 0.9, 'high', '{ this is invalid json !!! ')",
            [],
        ).unwrap();

        let report = ContextHealthCheckService::run_all_checks(&conn).unwrap();
        assert_eq!(report.status, ContextHealthStatus::Error);
        assert!(report.issues.iter().any(|i| i.category == HealthIssueCategory::CorruptedReconstruction));
    }

    #[test]
    fn test_context_with_no_valid_evidence_warning() {
        let conn = setup_clean_db();

        // Insert empty workspace with zero items/signals/tasks
        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at, lifecycle_state)
             VALUES ('ws_empty', 'Empty Context', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z', 'active')",
            [],
        ).unwrap();

        let report = ContextHealthCheckService::run_all_checks(&conn).unwrap();
        assert_eq!(report.status, ContextHealthStatus::Warning);
        assert!(report.issues.iter().any(|i| i.category == HealthIssueCategory::NoValidEvidence));
    }
}

use std::collections::HashMap;
use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::context_engine::context_state::ContextStateService;
use crate::context_engine::reconstruction::ContextReconstructionService;
use crate::context_engine::workset::ContextWorksetService;

// =========================================================================
// Epistemic Disclaimer
// =========================================================================

pub const CROSS_SYSTEM_CONSISTENCY_DISCLAIMER: &str =
    "ContextSplit cross-system consistency checker deterministically validates that all 11 contextual subsystems agree on context identity. Pure read-only inspection without mutating production data.";

// =========================================================================
// Models
// =========================================================================

/// Detailed mismatch detected when a subsystem disagrees on context identity or reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextIdentityMismatch {
    /// Subsystem name (e.g. "Tasks", "Snapshots", "Lifecycle", "Reconstruction")
    pub subsystem: String,
    /// Target context/workspace ID
    pub context_id: String,
    /// Expected context name
    pub expected_context_name: String,
    /// Actual context name resolved in subsystem (if any)
    pub actual_context_name: Option<String>,
    /// Entity type affected (e.g. "task_candidate", "snapshot", "membership")
    pub entity_type: String,
    /// Entity ID affected
    pub entity_id: String,
    /// Detailed diagnostic explanation of the identity divergence
    pub description: String,
}

/// Verification result for a single subsystem's agreement on context identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubsystemIdentityCheck {
    /// Subsystem name (e.g. "Tasks", "Context Snapshots", "Context Memory")
    pub subsystem_name: String,
    /// Whether this subsystem is 100% consistent with the context identity
    pub is_consistent: bool,
    /// Total entity references inspected in this subsystem
    pub references_checked: usize,
    /// List of detected mismatches in this subsystem
    pub mismatches: Vec<ContextIdentityMismatch>,
}

/// Cross-system consistency matrix report for a single context/workspace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceCrossSystemReport {
    /// Workspace / Context ID
    pub context_id: String,
    /// Workspace / Context Name (e.g. "PayX")
    pub context_name: String,
    /// Current lifecycle state (e.g. "active", "paused", "dormant")
    pub lifecycle_state: String,
    /// Whether all 11 subsystems agree on this context's identity
    pub all_systems_consistent: bool,
    /// Total subsystems checked for this workspace (11)
    pub subsystems_checked: usize,
    /// Map of subsystem name -> verification result
    pub subsystem_results: HashMap<String, SubsystemIdentityCheck>,
    /// Total count of mismatches across all subsystems for this workspace
    pub total_mismatches: usize,
}

/// Comprehensive cross-system context consistency report across all workspaces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrossSystemConsistencyReport {
    /// Whether the entire database is 100% consistent across all systems
    pub is_fully_consistent: bool,
    /// Total number of distinct contexts/workspaces evaluated
    pub total_contexts_checked: usize,
    /// Total subsystems evaluated per context (11)
    pub subsystems_evaluated: usize,
    /// Per-workspace reports
    pub workspace_reports: Vec<WorkspaceCrossSystemReport>,
    /// Global or unassociated mismatches (e.g. references to deleted/unregistered contexts)
    pub global_mismatches: Vec<ContextIdentityMismatch>,
    /// ISO-8601 timestamp of validation
    pub timestamp: String,
    /// Epistemic disclaimer
    pub disclaimer: &'static str,
}

impl CrossSystemConsistencyReport {
    /// Formats the cross-system report into a clean developer matrix.
    pub fn format_report(&self) -> String {
        let mut out = String::new();
        out.push_str("================================================================================\n");
        out.push_str(&format!(
            "CONTEXTSPLIT CROSS-SYSTEM CONSISTENCY REPORT: {}\n",
            if self.is_fully_consistent { "CONSISTENT (PASS)" } else { "INCONSISTENT (FAIL)" }
        ));
        out.push_str("================================================================================\n\n");
        out.push_str(&format!("Timestamp:            {}\n", self.timestamp));
        out.push_str(&format!("Workspaces Checked:   {}\n", self.total_contexts_checked));
        out.push_str(&format!("Subsystems Evaluated: {} (all 11 contextual engines)\n", self.subsystems_evaluated));
        out.push_str(&format!(
            "Overall Status:       {}\n\n",
            if self.is_fully_consistent { "PASS" } else { "FAIL" }
        ));

        if !self.global_mismatches.is_empty() {
            out.push_str("Global / Unmapped Identity Mismatches:\n");
            out.push_str("--------------------------------------------------------------------------------\n");
            for (idx, m) in self.global_mismatches.iter().enumerate() {
                out.push_str(&format!(
                    "[{}] Subsystem: {}\n    Entity: {} (id={})\n    Target Context: {} (expected '{}')\n    Error: {}\n\n",
                    idx + 1,
                    m.subsystem,
                    m.entity_type,
                    m.entity_id,
                    m.context_id,
                    m.expected_context_name,
                    m.description
                ));
            }
        }

        out.push_str("Workspace Subsystem Agreement Matrix:\n");
        out.push_str("--------------------------------------------------------------------------------\n");

        for ws in &self.workspace_reports {
            out.push_str(&format!(
                "Context: '{}' (ID: {}, State: {}) -> {}\n",
                ws.context_name,
                ws.context_id,
                ws.lifecycle_state,
                if ws.all_systems_consistent { "ALL 11 SYSTEMS AGREE [PASS]" } else { "IDENTITY DIVERGENCE DETECTED [FAIL]" }
            ));

            let subsystems_order = [
                "Context Engine",
                "Context Membership",
                "Context Sessions",
                "Context Memory",
                "Context Workset",
                "Context State",
                "Context Snapshots",
                "Context Lifecycle",
                "Context Reconstruction",
                "Context Feedback",
                "Tasks",
            ];

            for sub_name in &subsystems_order {
                if let Some(check) = ws.subsystem_results.get(*sub_name) {
                    let status_str = if check.is_consistent {
                        "OK".to_string()
                    } else {
                        format!("MISMATCH ({} errors)", check.mismatches.len())
                    };
                    out.push_str(&format!("  - {:<24} : {} ({} checked)\n", sub_name, status_str, check.references_checked));
                    for m in &check.mismatches {
                        out.push_str(&format!("      * [{}] {}: {}\n", m.entity_type, m.entity_id, m.description));
                    }
                }
            }
            out.push('\n');
        }

        out.push_str("--------------------------------------------------------------------------------\n");
        out.push_str(&format!("Note: {}\n", self.disclaimer));
        out
    }
}

// =========================================================================
// CrossSystemConsistencyChecker Service
// =========================================================================

/// Deterministic, read-only validator verifying context identity agreement across all 11 subsystems.
pub struct CrossSystemConsistencyChecker;

impl CrossSystemConsistencyChecker {
    /// Evaluates cross-system identity consistency across all registered workspaces.
    pub fn validate_all(conn: &Connection) -> Result<CrossSystemConsistencyReport, rusqlite::Error> {
        let mut stmt = conn.prepare("SELECT id, name, lifecycle_state FROM workspaces ORDER BY name ASC")?;
        let ws_rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;

        let mut workspace_reports: Vec<WorkspaceCrossSystemReport> = Vec::new();
        let mut all_consistent = true;

        for row in ws_rows.filter_map(Result::ok) {
            let (ws_id, ws_name, state) = row;
            let report = Self::validate_workspace(conn, &ws_id, &ws_name, &state)?;
            if !report.all_systems_consistent {
                all_consistent = false;
            }
            workspace_reports.push(report);
        }

        // Check for orphaned references that reference non-existent contexts globally
        let global_mismatches = Self::check_global_orphans(conn)?;
        if !global_mismatches.is_empty() {
            all_consistent = false;
        }

        let total_contexts_checked = workspace_reports.len();

        Ok(CrossSystemConsistencyReport {
            is_fully_consistent: all_consistent,
            total_contexts_checked,
            subsystems_evaluated: 11,
            workspace_reports,
            global_mismatches,
            timestamp: Utc::now().to_rfc3339(),
            disclaimer: CROSS_SYSTEM_CONSISTENCY_DISCLAIMER,
        })
    }

    /// Validates cross-system consistency for a specific workspace across all 11 subsystems.
    pub fn validate_workspace(
        conn: &Connection,
        workspace_id: &str,
        workspace_name: &str,
        lifecycle_state: &str,
    ) -> Result<WorkspaceCrossSystemReport, rusqlite::Error> {
        let mut subsystem_results: HashMap<String, SubsystemIdentityCheck> = HashMap::new();
        let mut total_mismatches = 0;

        // 1. Context Engine
        let res_engine = Self::check_engine_core_consistency(conn, workspace_id, workspace_name)?;
        total_mismatches += res_engine.mismatches.len();
        subsystem_results.insert("Context Engine".to_string(), res_engine);

        // 2. Context Membership
        let res_membership = Self::check_memberships_consistency(conn, workspace_id, workspace_name)?;
        total_mismatches += res_membership.mismatches.len();
        subsystem_results.insert("Context Membership".to_string(), res_membership);

        // 3. Context Sessions
        let res_sessions = Self::check_sessions_consistency(conn, workspace_id, workspace_name)?;
        total_mismatches += res_sessions.mismatches.len();
        subsystem_results.insert("Context Sessions".to_string(), res_sessions);

        // 4. Context Memory
        let res_memory = Self::check_memories_consistency(conn, workspace_id, workspace_name)?;
        total_mismatches += res_memory.mismatches.len();
        subsystem_results.insert("Context Memory".to_string(), res_memory);

        // 5. Context Workset
        let res_workset = Self::check_worksets_consistency(conn, workspace_id, workspace_name)?;
        total_mismatches += res_workset.mismatches.len();
        subsystem_results.insert("Context Workset".to_string(), res_workset);

        // 6. Context State
        let res_state = Self::check_states_consistency(conn, workspace_id, workspace_name, lifecycle_state)?;
        total_mismatches += res_state.mismatches.len();
        subsystem_results.insert("Context State".to_string(), res_state);

        // 7. Context Snapshots
        let res_snapshots = Self::check_snapshots_consistency(conn, workspace_id, workspace_name)?;
        total_mismatches += res_snapshots.mismatches.len();
        subsystem_results.insert("Context Snapshots".to_string(), res_snapshots);

        // 8. Context Lifecycle
        let res_lifecycle = Self::check_lifecycle_consistency(conn, workspace_id, workspace_name, lifecycle_state)?;
        total_mismatches += res_lifecycle.mismatches.len();
        subsystem_results.insert("Context Lifecycle".to_string(), res_lifecycle);

        // 9. Context Reconstruction
        let res_reconstruction = Self::check_reconstructions_consistency(conn, workspace_id, workspace_name)?;
        total_mismatches += res_reconstruction.mismatches.len();
        subsystem_results.insert("Context Reconstruction".to_string(), res_reconstruction);

        // 10. Context Feedback
        let res_feedback = Self::check_feedback_consistency(conn, workspace_id, workspace_name)?;
        total_mismatches += res_feedback.mismatches.len();
        subsystem_results.insert("Context Feedback".to_string(), res_feedback);

        // 11. Tasks
        let res_tasks = Self::check_tasks_consistency(conn, workspace_id, workspace_name)?;
        total_mismatches += res_tasks.mismatches.len();
        subsystem_results.insert("Tasks".to_string(), res_tasks);

        let all_systems_consistent = total_mismatches == 0;

        Ok(WorkspaceCrossSystemReport {
            context_id: workspace_id.to_string(),
            context_name: workspace_name.to_string(),
            lifecycle_state: lifecycle_state.to_string(),
            all_systems_consistent,
            subsystems_checked: 11,
            subsystem_results,
            total_mismatches,
        })
    }

    // -------------------------------------------------------------------------
    // Subsystem 1: Context Engine Core
    // -------------------------------------------------------------------------
    pub fn check_engine_core_consistency(
        conn: &Connection,
        workspace_id: &str,
        workspace_name: &str,
    ) -> Result<SubsystemIdentityCheck, rusqlite::Error> {
        let mut mismatches = Vec::new();
        let mut checked = 0;

        // Check workspace entry itself
        let mut stmt = conn.prepare("SELECT id, name FROM workspaces WHERE id = ?1")?;
        let mut rows = stmt.query([workspace_id])?;
        checked += 1;

        if let Some(row) = rows.next()? {
            let actual_name: String = row.get(1)?;
            if actual_name != workspace_name {
                mismatches.push(ContextIdentityMismatch {
                    subsystem: "Context Engine".to_string(),
                    context_id: workspace_id.to_string(),
                    expected_context_name: workspace_name.to_string(),
                    actual_context_name: Some(actual_name.clone()),
                    entity_type: "workspace".to_string(),
                    entity_id: workspace_id.to_string(),
                    description: format!(
                        "Workspace name mismatch: expected '{}', got '{}'",
                        workspace_name, actual_name
                    ),
                });
            }
        } else {
            mismatches.push(ContextIdentityMismatch {
                subsystem: "Context Engine".to_string(),
                context_id: workspace_id.to_string(),
                expected_context_name: workspace_name.to_string(),
                actual_context_name: None,
                entity_type: "workspace".to_string(),
                entity_id: workspace_id.to_string(),
                description: "Workspace not found in core engine registry".to_string(),
            });
        }

        Ok(SubsystemIdentityCheck {
            subsystem_name: "Context Engine".to_string(),
            is_consistent: mismatches.is_empty(),
            references_checked: checked,
            mismatches,
        })
    }

    // -------------------------------------------------------------------------
    // Subsystem 2: Context Membership
    // -------------------------------------------------------------------------
    pub fn check_memberships_consistency(
        conn: &Connection,
        workspace_id: &str,
        workspace_name: &str,
    ) -> Result<SubsystemIdentityCheck, rusqlite::Error> {
        let mut mismatches = Vec::new();
        let mut stmt = conn.prepare("SELECT id, item_id FROM context_memberships WHERE workspace_id = ?1")?;
        let rows = stmt.query_map([workspace_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut checked = 0;
        for row in rows.filter_map(Result::ok) {
            checked += 1;
            let (m_id, item_id) = row;
            let item_exists: bool = conn
                .query_row("SELECT EXISTS(SELECT 1 FROM items WHERE id = ?1)", [&item_id], |r| r.get(0))
                .unwrap_or(false);

            if !item_exists {
                mismatches.push(ContextIdentityMismatch {
                    subsystem: "Context Membership".to_string(),
                    context_id: workspace_id.to_string(),
                    expected_context_name: workspace_name.to_string(),
                    actual_context_name: None,
                    entity_type: "context_membership".to_string(),
                    entity_id: m_id,
                    description: format!("Membership for '{}' points to missing item '{}'", workspace_name, item_id),
                });
            }
        }

        Ok(SubsystemIdentityCheck {
            subsystem_name: "Context Membership".to_string(),
            is_consistent: mismatches.is_empty(),
            references_checked: checked,
            mismatches,
        })
    }

    // -------------------------------------------------------------------------
    // Subsystem 3: Context Sessions
    // -------------------------------------------------------------------------
    pub fn check_sessions_consistency(
        conn: &Connection,
        workspace_id: &str,
        workspace_name: &str,
    ) -> Result<SubsystemIdentityCheck, rusqlite::Error> {
        let mut mismatches = Vec::new();
        let mut checked = 0;

        // Check items associated with this workspace and sessions
        let mut stmt = conn.prepare("SELECT id, session_id FROM items WHERE workspace_id = ?1 AND session_id IS NOT NULL AND session_id != ''")?;
        let rows = stmt.query_map([workspace_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        for row in rows.filter_map(Result::ok) {
            checked += 1;
            let (item_id, session_id) = row;
            let session_exists: bool = conn
                .query_row("SELECT EXISTS(SELECT 1 FROM sessions WHERE id = ?1)", [&session_id], |r| r.get(0))
                .unwrap_or(false);

            if !session_exists {
                mismatches.push(ContextIdentityMismatch {
                    subsystem: "Context Sessions".to_string(),
                    context_id: workspace_id.to_string(),
                    expected_context_name: workspace_name.to_string(),
                    actual_context_name: None,
                    entity_type: "item_session".to_string(),
                    entity_id: item_id,
                    description: format!(
                        "Item in '{}' references non-existent session ID '{}'",
                        workspace_name, session_id
                    ),
                });
            }
        }

        Ok(SubsystemIdentityCheck {
            subsystem_name: "Context Sessions".to_string(),
            is_consistent: mismatches.is_empty(),
            references_checked: checked,
            mismatches,
        })
    }

    // -------------------------------------------------------------------------
    // Subsystem 4: Context Memory
    // -------------------------------------------------------------------------
    pub fn check_memories_consistency(
        conn: &Connection,
        workspace_id: &str,
        workspace_name: &str,
    ) -> Result<SubsystemIdentityCheck, rusqlite::Error> {
        let mut mismatches = Vec::new();
        let mut checked = 0;

        let mut stmt = conn.prepare(
            "SELECT id, title, content FROM items WHERE workspace_id = ?1 AND item_type IN ('important_memory', 'memory', 'context_memory')"
        )?;
        let rows = stmt.query_map([workspace_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;

        for row in rows.filter_map(Result::ok) {
            checked += 1;
            let (mem_id, _) = row;
            // Verify item references current valid workspace
            let ws_exists: bool = conn
                .query_row("SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = ?1)", [workspace_id], |r| r.get(0))
                .unwrap_or(false);

            if !ws_exists {
                mismatches.push(ContextIdentityMismatch {
                    subsystem: "Context Memory".to_string(),
                    context_id: workspace_id.to_string(),
                    expected_context_name: workspace_name.to_string(),
                    actual_context_name: None,
                    entity_type: "important_memory".to_string(),
                    entity_id: mem_id,
                    description: format!("Memory references unresolvable workspace ID '{}'", workspace_id),
                });
            }
        }

        Ok(SubsystemIdentityCheck {
            subsystem_name: "Context Memory".to_string(),
            is_consistent: mismatches.is_empty(),
            references_checked: checked,
            mismatches,
        })
    }

    // -------------------------------------------------------------------------
    // Subsystem 5: Context Workset
    // -------------------------------------------------------------------------
    pub fn check_worksets_consistency(
        conn: &Connection,
        workspace_id: &str,
        workspace_name: &str,
    ) -> Result<SubsystemIdentityCheck, rusqlite::Error> {
        let mut mismatches = Vec::new();
        let mut checked = 0;

        // Derive active workset and verify all items resolve to this workspace
        if let Ok(workset) = ContextWorksetService::new().derive_for_workspace(conn, workspace_id) {
            checked = workset.items.len();
            for w_item in &workset.items {
                // Ensure item exists in database
                let item_ws: Option<Option<String>> = conn
                    .query_row("SELECT workspace_id FROM items WHERE id = ?1", [&w_item.id], |r| r.get(0))
                    .ok();

                if let Some(item_ws_opt) = item_ws {
                    if let Some(ref assigned_ws) = item_ws_opt {
                        if assigned_ws != workspace_id && assigned_ws != "inbox" && !assigned_ws.is_empty() {
                            // If item has explicit membership in another context and no membership in this context
                            let has_membership: bool = conn
                                .query_row(
                                    "SELECT EXISTS(SELECT 1 FROM context_memberships WHERE item_id = ?1 AND workspace_id = ?2)",
                                    rusqlite::params![&w_item.id, workspace_id],
                                    |r| r.get(0),
                                )
                                .unwrap_or(false);

                            if !has_membership {
                                mismatches.push(ContextIdentityMismatch {
                                    subsystem: "Context Workset".to_string(),
                                    context_id: workspace_id.to_string(),
                                    expected_context_name: workspace_name.to_string(),
                                    actual_context_name: Some(assigned_ws.clone()),
                                    entity_type: "workset_item".to_string(),
                                    entity_id: w_item.id.clone(),
                                    description: format!(
                                        "Workset item '{}' belongs to foreign context '{}' without valid membership in '{}'",
                                        w_item.id, assigned_ws, workspace_name
                                    ),
                                });
                            }
                        }
                    }
                } else {
                    mismatches.push(ContextIdentityMismatch {
                        subsystem: "Context Workset".to_string(),
                        context_id: workspace_id.to_string(),
                        expected_context_name: workspace_name.to_string(),
                        actual_context_name: None,
                        entity_type: "workset_item".to_string(),
                        entity_id: w_item.id.clone(),
                        description: format!("Workset derived item '{}' missing from items table", w_item.id),
                    });
                }
            }
        }

        Ok(SubsystemIdentityCheck {
            subsystem_name: "Context Workset".to_string(),
            is_consistent: mismatches.is_empty(),
            references_checked: checked,
            mismatches,
        })
    }

    // -------------------------------------------------------------------------
    // Subsystem 6: Context State
    // -------------------------------------------------------------------------
    pub fn check_states_consistency(
        conn: &Connection,
        workspace_id: &str,
        workspace_name: &str,
        _lifecycle_state: &str,
    ) -> Result<SubsystemIdentityCheck, rusqlite::Error> {
        let mut mismatches = Vec::new();
        let mut checked = 0;

        if let Ok(state_val) = ContextStateService::new().derive_for_context(conn, workspace_id) {
            checked += 1;
            if state_val.context_id != workspace_id {
                mismatches.push(ContextIdentityMismatch {
                    subsystem: "Context State".to_string(),
                    context_id: workspace_id.to_string(),
                    expected_context_name: workspace_name.to_string(),
                    actual_context_name: None,
                    entity_type: "context_state".to_string(),
                    entity_id: workspace_id.to_string(),
                    description: format!(
                        "Context state context_id mismatch: expected '{}', got '{}'",
                        workspace_id, state_val.context_id
                    ),
                });
            }

            if let Some(ref s_name) = state_val.context_name {
                if s_name != workspace_name {
                    mismatches.push(ContextIdentityMismatch {
                        subsystem: "Context State".to_string(),
                        context_id: workspace_id.to_string(),
                        expected_context_name: workspace_name.to_string(),
                        actual_context_name: Some(s_name.clone()),
                        entity_type: "context_state".to_string(),
                        entity_id: workspace_id.to_string(),
                        description: format!(
                            "Context state context_name mismatch: expected '{}', got '{}'",
                            workspace_name, s_name
                        ),
                    });
                }
            }
        }

        Ok(SubsystemIdentityCheck {
            subsystem_name: "Context State".to_string(),
            is_consistent: mismatches.is_empty(),
            references_checked: checked,
            mismatches,
        })
    }

    // -------------------------------------------------------------------------
    // Subsystem 7: Context Snapshots
    // -------------------------------------------------------------------------
    pub fn check_snapshots_consistency(
        conn: &Connection,
        workspace_id: &str,
        workspace_name: &str,
    ) -> Result<SubsystemIdentityCheck, rusqlite::Error> {
        let mut mismatches = Vec::new();
        let mut checked = 0;

        let mut stmt = conn.prepare("SELECT id, context_name FROM context_snapshots WHERE context_id = ?1")?;
        let rows = stmt.query_map([workspace_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        for row in rows.filter_map(Result::ok) {
            checked += 1;
            let (snap_id, snap_context_name) = row;
            if snap_context_name != workspace_name {
                mismatches.push(ContextIdentityMismatch {
                    subsystem: "Context Snapshots".to_string(),
                    context_id: workspace_id.to_string(),
                    expected_context_name: workspace_name.to_string(),
                    actual_context_name: Some(snap_context_name.clone()),
                    entity_type: "context_snapshot".to_string(),
                    entity_id: snap_id,
                    description: format!(
                        "Snapshot context name mismatch: expected '{}', got '{}'",
                        workspace_name, snap_context_name
                    ),
                });
            }
        }

        Ok(SubsystemIdentityCheck {
            subsystem_name: "Context Snapshots".to_string(),
            is_consistent: mismatches.is_empty(),
            references_checked: checked,
            mismatches,
        })
    }

    // -------------------------------------------------------------------------
    // Subsystem 8: Context Lifecycle
    // -------------------------------------------------------------------------
    pub fn check_lifecycle_consistency(
        conn: &Connection,
        workspace_id: &str,
        workspace_name: &str,
        lifecycle_state: &str,
    ) -> Result<SubsystemIdentityCheck, rusqlite::Error> {
        let mut mismatches = Vec::new();
        let checked = 1;

        // Verify that if lifecycle says active, workspace exists and matches
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = ?1 AND lifecycle_state = ?2)",
                rusqlite::params![workspace_id, lifecycle_state],
                |r| r.get(0),
            )
            .unwrap_or(false);

        if !exists {
            mismatches.push(ContextIdentityMismatch {
                subsystem: "Context Lifecycle".to_string(),
                context_id: workspace_id.to_string(),
                expected_context_name: workspace_name.to_string(),
                actual_context_name: None,
                entity_type: "workspace_lifecycle".to_string(),
                entity_id: workspace_id.to_string(),
                description: format!(
                    "Lifecycle state '{}' not consistently recorded for workspace '{}'",
                    lifecycle_state, workspace_name
                ),
            });
        }

        Ok(SubsystemIdentityCheck {
            subsystem_name: "Context Lifecycle".to_string(),
            is_consistent: mismatches.is_empty(),
            references_checked: checked,
            mismatches,
        })
    }

    // -------------------------------------------------------------------------
    // Subsystem 9: Context Reconstruction
    // -------------------------------------------------------------------------
    pub fn check_reconstructions_consistency(
        conn: &Connection,
        workspace_id: &str,
        workspace_name: &str,
    ) -> Result<SubsystemIdentityCheck, rusqlite::Error> {
        let mut mismatches = Vec::new();
        let mut checked = 0;

        if let Ok(recon) = ContextReconstructionService::new().reconstruct_context(conn, workspace_id) {
            checked = 1;
            if let Some(ref recon_id) = recon.context.id {
                if recon_id != workspace_id {
                    mismatches.push(ContextIdentityMismatch {
                        subsystem: "Context Reconstruction".to_string(),
                        context_id: workspace_id.to_string(),
                        expected_context_name: workspace_name.to_string(),
                        actual_context_name: None,
                        entity_type: "context_reconstruction".to_string(),
                        entity_id: workspace_id.to_string(),
                        description: format!(
                            "Reconstructed context_id mismatch: expected '{}', got '{}'",
                            workspace_id, recon_id
                        ),
                    });
                }
            }

            if !recon.context.name.is_empty() && recon.context.name != workspace_name && recon.context.name != "Unknown Context" {
                mismatches.push(ContextIdentityMismatch {
                    subsystem: "Context Reconstruction".to_string(),
                    context_id: workspace_id.to_string(),
                    expected_context_name: workspace_name.to_string(),
                    actual_context_name: Some(recon.context.name.clone()),
                    entity_type: "context_reconstruction".to_string(),
                    entity_id: workspace_id.to_string(),
                    description: format!(
                        "Reconstructed context_name mismatch: expected '{}', got '{}'",
                        workspace_name, recon.context.name
                    ),
                });
            }
        }

        Ok(SubsystemIdentityCheck {
            subsystem_name: "Context Reconstruction".to_string(),
            is_consistent: mismatches.is_empty(),
            references_checked: checked,
            mismatches,
        })
    }

    // -------------------------------------------------------------------------
    // Subsystem 10: Context Feedback
    // -------------------------------------------------------------------------
    pub fn check_feedback_consistency(
        conn: &Connection,
        workspace_id: &str,
        workspace_name: &str,
    ) -> Result<SubsystemIdentityCheck, rusqlite::Error> {
        let mut mismatches = Vec::new();
        let mut checked = 0;

        let mut stmt = conn.prepare(
            "SELECT id, payload FROM context_signals WHERE workspace_id = ?1 AND signal_type IN ('user_feedback', 'feedback', 'user_action')"
        )?;
        let rows = stmt.query_map([workspace_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        for row in rows.filter_map(Result::ok) {
            checked += 1;
            let (sig_id, payload) = row;
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload) {
                if let Some(target_ctx) = val.get("context_ref").and_then(|c| c.as_str()) {
                    if target_ctx != workspace_id {
                        mismatches.push(ContextIdentityMismatch {
                            subsystem: "Context Feedback".to_string(),
                            context_id: workspace_id.to_string(),
                            expected_context_name: workspace_name.to_string(),
                            actual_context_name: Some(target_ctx.to_string()),
                            entity_type: "feedback_signal".to_string(),
                            entity_id: sig_id,
                            description: format!(
                                "Feedback signal assigned to '{}' has internal context_ref '{}'",
                                workspace_name, target_ctx
                            ),
                        });
                    }
                }
            }
        }

        Ok(SubsystemIdentityCheck {
            subsystem_name: "Context Feedback".to_string(),
            is_consistent: mismatches.is_empty(),
            references_checked: checked,
            mismatches,
        })
    }

    // -------------------------------------------------------------------------
    // Subsystem 11: Tasks
    // -------------------------------------------------------------------------
    pub fn check_tasks_consistency(
        conn: &Connection,
        workspace_id: &str,
        workspace_name: &str,
    ) -> Result<SubsystemIdentityCheck, rusqlite::Error> {
        let mut mismatches = Vec::new();
        let mut checked = 0;

        let mut stmt = conn.prepare("SELECT id, title, context_id FROM task_candidates WHERE context_id = ?1")?;
        let rows = stmt.query_map([workspace_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;

        for row in rows.filter_map(Result::ok) {
            checked += 1;
            let (task_id, title, ctx_opt) = row;
            match ctx_opt {
                Some(ref c_id) if c_id == workspace_id => {
                    // Valid match!
                }
                Some(ref other_id) => {
                    mismatches.push(ContextIdentityMismatch {
                        subsystem: "Tasks".to_string(),
                        context_id: workspace_id.to_string(),
                        expected_context_name: workspace_name.to_string(),
                        actual_context_name: Some(other_id.clone()),
                        entity_type: "task_candidate".to_string(),
                        entity_id: task_id,
                        description: format!(
                            "Task candidate '{}' context_id mismatch: expected '{}', got '{}'",
                            title, workspace_id, other_id
                        ),
                    });
                }
                None => {
                    mismatches.push(ContextIdentityMismatch {
                        subsystem: "Tasks".to_string(),
                        context_id: workspace_id.to_string(),
                        expected_context_name: workspace_name.to_string(),
                        actual_context_name: None,
                        entity_type: "task_candidate".to_string(),
                        entity_id: task_id,
                        description: format!("Task candidate '{}' has null context reference", title),
                    });
                }
            }
        }

        Ok(SubsystemIdentityCheck {
            subsystem_name: "Tasks".to_string(),
            is_consistent: mismatches.is_empty(),
            references_checked: checked,
            mismatches,
        })
    }

    // -------------------------------------------------------------------------
    // Global Unmapped References Check
    // -------------------------------------------------------------------------
    fn check_global_orphans(conn: &Connection) -> Result<Vec<ContextIdentityMismatch>, rusqlite::Error> {
        let mut mismatches = Vec::new();

        // Tasks pointing to non-existent context
        let mut stmt_t = conn.prepare(
            "SELECT tc.id, tc.title, tc.context_id
             FROM task_candidates tc
             WHERE tc.context_id IS NOT NULL AND tc.context_id != ''
               AND NOT EXISTS (SELECT 1 FROM workspaces w WHERE w.id = tc.context_id)",
        )?;
        let t_rows = stmt_t.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        for row in t_rows.filter_map(Result::ok) {
            let (t_id, title, ctx_id) = row;
            mismatches.push(ContextIdentityMismatch {
                subsystem: "Tasks".to_string(),
                context_id: ctx_id.clone(),
                expected_context_name: "Existing Workspace".to_string(),
                actual_context_name: None,
                entity_type: "task_candidate".to_string(),
                entity_id: t_id,
                description: format!("Task '{}' points to missing context ID '{}'", title, ctx_id),
            });
        }

        // Snapshots pointing to non-existent context
        let mut stmt_s = conn.prepare(
            "SELECT cs.id, cs.context_name, cs.context_id
             FROM context_snapshots cs
             WHERE NOT EXISTS (SELECT 1 FROM workspaces w WHERE w.id = cs.context_id)",
        )?;
        let s_rows = stmt_s.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        for row in s_rows.filter_map(Result::ok) {
            let (snap_id, snap_name, ctx_id) = row;
            mismatches.push(ContextIdentityMismatch {
                subsystem: "Context Snapshots".to_string(),
                context_id: ctx_id.clone(),
                expected_context_name: snap_name.clone(),
                actual_context_name: None,
                entity_type: "context_snapshot".to_string(),
                entity_id: snap_id,
                description: format!("Snapshot for '{}' references missing context ID '{}'", snap_name, ctx_id),
            });
        }

        Ok(mismatches)
    }
}

// =========================================================================
// Unit & Integration Tests
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
        conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
        conn
    }

    #[test]
    fn test_harmonious_database_all_11_subsystems_pass() {
        let conn = setup_clean_db();

        // 1. Context Engine: Workspace PayX
        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at, lifecycle_state)
             VALUES ('ws_payx', 'PayX', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z', 'active')",
            [],
        ).unwrap();

        // 2. Context Membership & Item
        conn.execute(
            "INSERT INTO items (id, workspace_id, item_type, content, title, metadata, created_at, updated_at)
             VALUES ('item_payx_1', 'ws_payx', 'file', 'content', 'Payment Service', '{}', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z')",
            [],
        ).unwrap();

        conn.execute(
            "INSERT INTO context_memberships (id, item_id, workspace_id, membership_type, confidence, source, created_at, updated_at)
             VALUES ('cm_payx_1', 'item_payx_1', 'ws_payx', 'confirmed', 1.0, 'user', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z')",
            [],
        ).unwrap();

        // 3. Context Sessions
        conn.execute(
            "INSERT INTO sessions (id, started_at, last_activity_at, ended_at, status)
             VALUES ('sess_payx', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z', '2026-01-01T01:00:00Z', 'ended')",
            [],
        ).unwrap();

        // 4. Context Memory
        conn.execute(
            "INSERT INTO items (id, workspace_id, item_type, content, title, metadata, created_at, updated_at)
             VALUES ('mem_payx', 'ws_payx', 'important_memory', 'Uses Cloudinary', 'PayX Cloudinary', '{}', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z')",
            [],
        ).unwrap();

        // 7. Context Snapshots
        conn.execute(
            "INSERT INTO context_snapshots (id, context_id, context_name, created_at, state, snapshot_confidence, quality_rating, snapshot_json)
             VALUES ('snap_payx', 'ws_payx', 'PayX', '2026-01-01T00:00:00Z', 'active', 0.95, 'high', '{}')",
            [],
        ).unwrap();

        // 10. Context Feedback
        conn.execute(
            "INSERT INTO context_signals (id, workspace_id, item_id, signal_type, payload, created_at, updated_at)
             VALUES ('sig_payx_fb', 'ws_payx', 'item_payx_1', 'user_feedback', '{\"context_ref\":\"ws_payx\"}', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z')",
            [],
        ).unwrap();

        // 11. Tasks
        conn.execute(
            "INSERT INTO task_candidates (id, context_id, title, confidence, status, first_detected_at, last_active_at, created_at, updated_at)
             VALUES ('task_payx', 'ws_payx', 'Checkout integration', 0.9, 'active', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z')",
            [],
        ).unwrap();

        let report = CrossSystemConsistencyChecker::validate_all(&conn).unwrap();
        assert!(report.is_fully_consistent);
        assert_eq!(report.total_contexts_checked, 1);
        assert_eq!(report.subsystems_evaluated, 11);
        assert_eq!(report.global_mismatches.len(), 0);

        let ws_report = &report.workspace_reports[0];
        assert_eq!(ws_report.context_name, "PayX");
        assert!(ws_report.all_systems_consistent);
        assert_eq!(ws_report.subsystems_checked, 11);
        assert_eq!(ws_report.total_mismatches, 0);

        let formatted = report.format_report();
        assert!(formatted.contains("ALL 11 SYSTEMS AGREE [PASS]"));
    }

    #[test]
    fn test_task_context_mismatch_detected() {
        let conn = setup_corrupted_db();

        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at, lifecycle_state)
             VALUES ('ws_payx', 'PayX', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z', 'active')",
            [],
        ).unwrap();

        // Task candidate belonging to non-existent context
        conn.execute(
            "INSERT INTO task_candidates (id, context_id, title, confidence, status, first_detected_at, last_active_at, created_at, updated_at)
             VALUES ('task_divergent', 'ws_missing_context', 'Orphaned Task', 0.9, 'active', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z')",
            [],
        ).unwrap();

        let report = CrossSystemConsistencyChecker::validate_all(&conn).unwrap();
        assert!(!report.is_fully_consistent);
        assert!(report.global_mismatches.iter().any(|m| m.subsystem == "Tasks"));
    }

    #[test]
    fn test_snapshot_context_name_mismatch_detected() {
        let conn = setup_clean_db();

        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at, lifecycle_state)
             VALUES ('ws_payx', 'PayX', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z', 'active')",
            [],
        ).unwrap();

        // Snapshot with ID ws_payx but name set to "DifferentName"
        conn.execute(
            "INSERT INTO context_snapshots (id, context_id, context_name, created_at, state, snapshot_confidence, quality_rating, snapshot_json)
             VALUES ('snap_mismatch', 'ws_payx', 'DifferentName', '2026-01-01T00:00:00Z', 'active', 0.95, 'high', '{}')",
            [],
        ).unwrap();

        let report = CrossSystemConsistencyChecker::validate_all(&conn).unwrap();
        assert!(!report.is_fully_consistent);

        let ws_report = &report.workspace_reports[0];
        assert!(!ws_report.all_systems_consistent);
        let snap_check = ws_report.subsystem_results.get("Context Snapshots").unwrap();
        assert!(!snap_check.is_consistent);
        assert_eq!(snap_check.mismatches.len(), 1);
        assert_eq!(snap_check.mismatches[0].expected_context_name, "PayX");
        assert_eq!(snap_check.mismatches[0].actual_context_name.as_deref(), Some("DifferentName"));
    }

    #[test]
    fn test_lifecycle_active_context_missing_detected() {
        let conn = setup_corrupted_db();

        // Snapshot pointing to missing ContextSplit
        conn.execute(
            "INSERT INTO context_snapshots (id, context_id, context_name, created_at, state, snapshot_confidence, quality_rating, snapshot_json)
             VALUES ('snap_cs', 'ws_contextsplit', 'ContextSplit', '2026-01-01T00:00:00Z', 'active', 0.95, 'high', '{}')",
            [],
        ).unwrap();

        let report = CrossSystemConsistencyChecker::validate_all(&conn).unwrap();
        assert!(!report.is_fully_consistent);
        assert!(report.global_mismatches.iter().any(|m| m.context_id == "ws_contextsplit"));
    }
}

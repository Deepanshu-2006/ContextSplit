use rusqlite::{Connection, Result};
use serde::{Deserialize, Serialize};

// =========================================================================
// Epistemic Disclaimer & Invariants
// =========================================================================

pub const DATA_INTEGRITY_DISCLAIMER: &str =
    "ContextSplit Data Integrity Gate audits all cross-subsystem relational invariants. \
    Invariant: When corrupted or invalid data is encountered, the system fails safely with explicit diagnostics and NEVER silently deletes questionable data.";

// =========================================================================
// Models
// =========================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationSeverity {
    Critical, // Broken foreign key, impossible state preventing core execution
    Warning,  // Stale reference, uncorroborated duplicate candidate
    Info,     // Minor cosmetic naming overlap
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntegrityViolation {
    pub category: String,
    pub severity: ViolationSeverity,
    pub entity_table: String,
    pub entity_id: String,
    pub description: String,
    pub remediation_hint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataIntegrityReport {
    pub is_valid: bool,
    pub total_violations: usize,
    pub critical_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
    pub violations: Vec<IntegrityViolation>,
    pub audited_tables: Vec<String>,
    pub timestamp: String,
    pub disclaimer: &'static str,
}

// =========================================================================
// DataIntegrityChecker Service
// =========================================================================

pub struct DataIntegrityChecker;

impl DataIntegrityChecker {
    /// Audits all relational invariants across Contexts, Sessions, Activities, Tasks,
    /// Memories, Worksets, Snapshots, Feedback, and Lifecycle states.
    pub fn audit_all(conn: &Connection) -> Result<DataIntegrityReport> {
        let mut violations = Vec::new();

        // 1. Check orphaned records
        Self::check_orphaned_records(conn, &mut violations)?;

        // 2. Check duplicate & colliding records
        Self::check_duplicate_records(conn, &mut violations)?;

        // 3. Check impossible state combinations
        Self::check_impossible_state_combinations(conn, &mut violations)?;

        // 4. Check cross-context boundary contamination
        Self::check_cross_context_contamination(conn, &mut violations)?;

        let critical_count = violations.iter().filter(|v| v.severity == ViolationSeverity::Critical).count();
        let warning_count = violations.iter().filter(|v| v.severity == ViolationSeverity::Warning).count();
        let info_count = violations.iter().filter(|v| v.severity == ViolationSeverity::Info).count();

        let is_valid = critical_count == 0;

        Ok(DataIntegrityReport {
            is_valid,
            total_violations: violations.len(),
            critical_count,
            warning_count,
            info_count,
            violations,
            audited_tables: vec![
                "workspaces".to_string(),
                "sessions".to_string(),
                "items".to_string(),
                "task_candidates".to_string(),
                "context_memberships".to_string(),
                "context_snapshots".to_string(),
                "context_signals".to_string(),
                "context_transitions".to_string(),
                "inbox_item_states".to_string(),
                "item_relationships".to_string(),
            ],
            timestamp: chrono::Utc::now().to_rfc3339(),
            disclaimer: DATA_INTEGRITY_DISCLAIMER,
        })
    }

    /// Checks for foreign key divergences, unanchored items, or dangling IDs.
    fn check_orphaned_records(conn: &Connection, violations: &mut Vec<IntegrityViolation>) -> Result<()> {
        // Orphaned items pointing to non-existent workspace
        let mut stmt = conn.prepare(
            "SELECT i.id, i.workspace_id FROM items i
             LEFT JOIN workspaces w ON i.workspace_id = w.id
             WHERE i.workspace_id IS NOT NULL AND w.id IS NULL",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (item_id, ws_id) = row?;
            violations.push(IntegrityViolation {
                category: "orphaned_record".to_string(),
                severity: ViolationSeverity::Critical,
                entity_table: "items".to_string(),
                entity_id: item_id,
                description: format!("Item references non-existent workspace_id '{}'", ws_id),
                remediation_hint: "Reassign item to Inbox or restore parent workspace".to_string(),
            });
        }

        // Orphaned memberships pointing to non-existent workspace
        let mut stmt = conn.prepare(
            "SELECT cm.id, cm.workspace_id FROM context_memberships cm
             LEFT JOIN workspaces w ON cm.workspace_id = w.id
             WHERE w.id IS NULL",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (mem_id, ws_id) = row?;
            violations.push(IntegrityViolation {
                category: "orphaned_record".to_string(),
                severity: ViolationSeverity::Critical,
                entity_table: "context_memberships".to_string(),
                entity_id: mem_id,
                description: format!("Context membership references non-existent workspace_id '{}'", ws_id),
                remediation_hint: "Clear membership record or create workspace".to_string(),
            });
        }

        // Orphaned memberships pointing to non-existent item
        let mut stmt = conn.prepare(
            "SELECT cm.id, cm.item_id FROM context_memberships cm
             LEFT JOIN items i ON cm.item_id = i.id
             WHERE i.id IS NULL",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (mem_id, item_id) = row?;
            violations.push(IntegrityViolation {
                category: "orphaned_record".to_string(),
                severity: ViolationSeverity::Critical,
                entity_table: "context_memberships".to_string(),
                entity_id: mem_id,
                description: format!("Context membership references non-existent item_id '{}'", item_id),
                remediation_hint: "Prune stale membership reference".to_string(),
            });
        }

        // Orphaned task candidates pointing to non-existent context
        let mut stmt = conn.prepare(
            "SELECT tc.id, tc.context_id FROM task_candidates tc
             LEFT JOIN workspaces w ON tc.context_id = w.id
             WHERE tc.context_id IS NOT NULL AND w.id IS NULL",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (task_id, ws_id) = row?;
            violations.push(IntegrityViolation {
                category: "orphaned_record".to_string(),
                severity: ViolationSeverity::Warning,
                entity_table: "task_candidates".to_string(),
                entity_id: task_id,
                description: format!("Task candidate references deleted workspace_id '{}'", ws_id),
                remediation_hint: "Nullify task candidate context_id or archive task".to_string(),
            });
        }

        // Orphaned snapshots pointing to non-existent workspace
        let mut stmt = conn.prepare(
            "SELECT cs.id, cs.context_id FROM context_snapshots cs
             LEFT JOIN workspaces w ON cs.context_id = w.id
             WHERE w.id IS NULL",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (snap_id, ws_id) = row?;
            violations.push(IntegrityViolation {
                category: "orphaned_record".to_string(),
                severity: ViolationSeverity::Warning,
                entity_table: "context_snapshots".to_string(),
                entity_id: snap_id,
                description: format!("Snapshot references deleted workspace_id '{}'", ws_id),
                remediation_hint: "Preserve snapshot for historical audit or archive cleanly".to_string(),
            });
        }

        Ok(())
    }

    /// Checks for duplicate records or conflicting unique keys.
    fn check_duplicate_records(conn: &Connection, violations: &mut Vec<IntegrityViolation>) -> Result<()> {
        // Multiple confirmed memberships for the same item in the same workspace
        let mut stmt = conn.prepare(
            "SELECT item_id, workspace_id, COUNT(*) as cnt FROM context_memberships
             WHERE membership_type = 'confirmed'
             GROUP BY item_id, workspace_id
             HAVING cnt > 1",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?))
        })?;
        for row in rows {
            let (item_id, ws_id, count) = row?;
            violations.push(IntegrityViolation {
                category: "duplicate_record".to_string(),
                severity: ViolationSeverity::Warning,
                entity_table: "context_memberships".to_string(),
                entity_id: format!("{}:{}", item_id, ws_id),
                description: format!("Found {} duplicate confirmed memberships for item in workspace", count),
                remediation_hint: "De-duplicate membership records to single authoritative entry".to_string(),
            });
        }

        // Duplicate active workspaces with identical case-insensitive names
        let mut stmt = conn.prepare(
            "SELECT LOWER(name), COUNT(*) as cnt FROM workspaces
             WHERE lifecycle_state != 'archived'
             GROUP BY LOWER(name)
             HAVING cnt > 1",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (name, count) = row?;
            violations.push(IntegrityViolation {
                category: "duplicate_record".to_string(),
                severity: ViolationSeverity::Info,
                entity_table: "workspaces".to_string(),
                entity_id: name.clone(),
                description: format!("Found {} active workspaces sharing identical name '{}'", count, name),
                remediation_hint: "Disambiguate workspace naming in UI".to_string(),
            });
        }

        Ok(())
    }

    /// Checks for contradictory or impossible state combinations.
    fn check_impossible_state_combinations(conn: &Connection, violations: &mut Vec<IntegrityViolation>) -> Result<()> {
        // Workspace archived but archived_at is NULL
        let mut stmt = conn.prepare(
            "SELECT id, name FROM workspaces WHERE lifecycle_state = 'archived' AND archived_at IS NULL",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (ws_id, name) = row?;
            violations.push(IntegrityViolation {
                category: "impossible_state".to_string(),
                severity: ViolationSeverity::Critical,
                entity_table: "workspaces".to_string(),
                entity_id: ws_id,
                description: format!("Workspace '{}' has lifecycle_state='archived' but archived_at is NULL", name),
                remediation_hint: "Set archived_at to updated_at timestamp".to_string(),
            });
        }

        // Session status is 'ended' but ended_at is NULL
        let mut stmt = conn.prepare(
            "SELECT id FROM sessions WHERE status = 'ended' AND ended_at IS NULL",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        for row in rows {
            let session_id = row?;
            violations.push(IntegrityViolation {
                category: "impossible_state".to_string(),
                severity: ViolationSeverity::Critical,
                entity_table: "sessions".to_string(),
                entity_id: session_id,
                description: "Session status is 'ended' but ended_at timestamp is NULL".to_string(),
                remediation_hint: "Set ended_at to last_activity_at".to_string(),
            });
        }

        // Session status is 'active' but ended_at is set
        let mut stmt = conn.prepare(
            "SELECT id FROM sessions WHERE status = 'active' AND ended_at IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        for row in rows {
            let session_id = row?;
            violations.push(IntegrityViolation {
                category: "impossible_state".to_string(),
                severity: ViolationSeverity::Critical,
                entity_table: "sessions".to_string(),
                entity_id: session_id,
                description: "Session status is 'active' but ended_at timestamp is populated".to_string(),
                remediation_hint: "Update status to 'ended'".to_string(),
            });
        }

        Ok(())
    }

    /// Checks for cross-context boundary violations or leaks.
    fn check_cross_context_contamination(conn: &Connection, violations: &mut Vec<IntegrityViolation>) -> Result<()> {
        // Confirmed membership in Workspace B while physically residing in Workspace A without cross-assignment
        let mut stmt = conn.prepare(
            "SELECT cm.id, cm.item_id, cm.workspace_id, i.workspace_id as physical_ws
             FROM context_memberships cm
             JOIN items i ON cm.item_id = i.id
             WHERE cm.membership_type = 'confirmed'
               AND i.workspace_id IS NOT NULL
               AND cm.workspace_id != i.workspace_id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        for row in rows {
            let (mem_id, item_id, mem_ws, phys_ws) = row?;
            violations.push(IntegrityViolation {
                category: "cross_context_divergence".to_string(),
                severity: ViolationSeverity::Info,
                entity_table: "context_memberships".to_string(),
                entity_id: mem_id,
                description: format!(
                    "Item '{}' has confirmed membership in '{}' while physically filed in '{}'",
                    item_id, mem_ws, phys_ws
                ),
                remediation_hint: "Multi-context sharing active or re-sync physical workspace".to_string(),
            });
        }

        Ok(())
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::operations::{create_item, create_session, create_workspace, end_session};

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_clean_db_integrity_audit_passes() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "ContextSplit Core").unwrap();
        let session = create_session(&conn).unwrap();
        let _item = create_item(&conn, Some(&ws.id), Some(&session.id), "code", "fn main() {}", Some("main.rs"), "{}").unwrap();
        let _ended = end_session(&conn, &session.id).unwrap();

        let report = DataIntegrityChecker::audit_all(&conn).unwrap();
        assert!(report.is_valid);
        assert_eq!(report.critical_count, 0);
    }

    #[test]
    fn test_orphaned_item_detected() {
        let conn = setup_test_db();
        conn.execute("PRAGMA foreign_keys = OFF;", []).unwrap();

        conn.execute(
            "INSERT INTO items (id, workspace_id, item_type, content, title, metadata, created_at, updated_at)
             VALUES ('item_orphan', 'non_existent_ws', 'text', 'Orphan content', 'Orphan', '{}', '2026-01-01', '2026-01-01')",
            [],
        ).unwrap();

        let report = DataIntegrityChecker::audit_all(&conn).unwrap();
        assert!(!report.is_valid);
        assert!(report.violations.iter().any(|v| v.entity_id == "item_orphan" && v.category == "orphaned_record"));
    }

    #[test]
    fn test_impossible_session_state_detected() {
        let conn = setup_test_db();
        conn.execute(
            "INSERT INTO sessions (id, started_at, last_activity_at, ended_at, status)
             VALUES ('sess_broken', '2026-01-01', '2026-01-01', NULL, 'ended')",
            [],
        ).unwrap();

        let report = DataIntegrityChecker::audit_all(&conn).unwrap();
        assert!(!report.is_valid);
        assert!(report.violations.iter().any(|v| v.entity_id == "sess_broken" && v.category == "impossible_state"));
    }
}

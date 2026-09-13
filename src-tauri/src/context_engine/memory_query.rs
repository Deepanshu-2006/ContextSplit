use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::context_engine::memory::{
    ContextMemory, ContextMemoryBuilder, InformationProvenance,
    MemoryItemReference, MemoryTaskReference,
};

// =========================================================================
// Data Transfer Objects (DTOs)
// =========================================================================

/// High-level, human-readable response to "What was I working on?".
/// Formatted cleanly for consumer UIs with zero raw technical telemetry or heavy vector dumps.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextMemoryResponse {
    /// Context / Workspace identifier
    pub context_id: String,
    /// Human-friendly name of the context (e.g. "TripWise", "Portfolio", "Aurora Website")
    pub context: String,
    /// Qualitative confidence band for the context container ("high", "medium", "low", "none")
    pub context_confidence: String,
    /// Specific active task title if sufficiently confident (None if insufficient evidence or unknown)
    pub task: Option<String>,
    /// Qualitative confidence band for the task ("high", "medium", "low", "none")
    pub task_confidence: Option<String>,
    /// ISO-8601 timestamp of most recent activity
    pub last_active_at: Option<String>,
    /// Activity state of the context ("active", "recently_active", "dormant", "insufficient_evidence")
    pub state: String,
    /// Concise human-readable activity description
    pub recent_activity: String,
    /// Lightweight references to recent items (sanitized titles and timestamps)
    pub recent_items: Vec<MemoryItemReference>,
    /// Most recent task candidate reference if available
    pub recent_task: Option<MemoryTaskReference>,
    /// Epistemic classification: whether the active information is observed, inferred, or confirmed
    pub provenance: InformationProvenance,
    /// Indicates if the most recent context activity was a context return
    pub is_context_return: bool,
    /// Curated, user-friendly evidence summaries backing this memory snapshot
    pub evidence: Vec<String>,
    /// Reconstructed working memory statement answering "What was the user working on?"
    pub memory_statement: String,
}

impl ContextMemoryResponse {
    /// Converts a raw internal `ContextMemory` model into a sanitized, consumer-ready response.
    pub fn from_memory(memory: &ContextMemory) -> Self {
        let has_task = memory.has_confident_task();
        let task = if has_task {
            memory.current_task_title.clone()
        } else {
            None
        };
        let task_confidence = if has_task {
            Some(memory.current_task.confidence_band.to_lowercase())
        } else {
            None
        };

        let is_context_return = memory.recent_context_transition.as_ref().map_or(false, |t| {
            t.transition_type == "context_return" || t.transition_type == "return"
        });

        // Curate user-friendly evidence lines
        let mut evidence = Vec::new();
        if let Some(t) = &task {
            if !memory.current_task.evidence.is_empty() {
                evidence.extend(memory.current_task.evidence.clone());
            } else {
                evidence.push(format!("Active task in {}: '{}'", memory.context_name, t));
            }
        }
        if !memory.recent_items.is_empty() {
            let item_count = memory.recent_items.len();
            let app_list: Vec<&str> = memory
                .recent_items
                .iter()
                .filter_map(|i| i.app_name.as_deref())
                .collect();
            if !app_list.is_empty() {
                let mut unique_apps = app_list;
                unique_apps.sort_unstable();
                unique_apps.dedup();
                evidence.push(format!(
                    "{} recent item{} in {}",
                    item_count,
                    if item_count == 1 { "" } else { "s" },
                    unique_apps.join(", ")
                ));
            } else {
                evidence.push(format!(
                    "{} recent item{}",
                    item_count,
                    if item_count == 1 { "" } else { "s" }
                ));
            }
        }
        if is_context_return {
            evidence.push(format!("Returned to {}", memory.context_name));
        }

        let provenance = memory
            .current_task_provenance
            .unwrap_or(memory.context_provenance);

        let recent_task = memory
            .last_active_task
            .clone()
            .or_else(|| memory.recent_tasks.first().cloned());

        Self {
            context_id: memory.context_id.clone(),
            context: memory.context_name.clone(),
            context_confidence: memory.context_confidence_band.to_lowercase(),
            task,
            task_confidence,
            last_active_at: memory.last_active_at.clone(),
            state: memory.activity_state.as_str().to_string(),
            recent_activity: memory.recent_activity.clone(),
            recent_items: memory.recent_items.clone(),
            recent_task,
            provenance,
            is_context_return,
            evidence,
            memory_statement: memory.memory_statement.clone(),
        }
    }
}

/// Detailed response for the last active task inside a context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LastActiveTaskResponse {
    /// Context identifier
    pub context_id: String,
    /// Task ID if tracked
    pub task_id: Option<String>,
    /// Task title
    pub title: String,
    /// Extracted action verb if known
    pub action_verb: Option<String>,
    /// Target subject/entity if known
    pub target_subject: Option<String>,
    /// Numeric confidence score [0.0, 1.0]
    pub confidence: f64,
    /// Qualitative confidence band ("high", "medium", "low", "insufficient")
    pub confidence_band: String,
    /// Epistemic source
    pub provenance: InformationProvenance,
    /// When this task was last active
    pub last_active_at: Option<String>,
    /// Whether this task is currently the active task
    pub is_current: bool,
}

// =========================================================================
// ContextMemoryQueryService
// =========================================================================

/// Dedicated query service providing consumer-grade answers to "What was I working on?".
/// Completely decoupled from React and presentation frameworks.
#[derive(Debug, Default, Clone)]
pub struct ContextMemoryQueryService {
    frozen_now: Option<DateTime<Utc>>,
}

impl ContextMemoryQueryService {
    pub fn new() -> Self {
        Self { frozen_now: None }
    }

    /// Sets a fixed clock for deterministic testing and snapshot evaluation.
    pub fn with_now(mut self, now: DateTime<Utc>) -> Self {
        self.frozen_now = Some(now);
        self
    }

    /// Reconstructs and returns the current active working memory across the system.
    /// Returns `None` if no non-archived context or activity exists.
    pub fn get_current_memory(&self, conn: &Connection) -> Result<Option<ContextMemoryResponse>, String> {
        let mut stmt = conn
            .prepare(
                "SELECT id FROM workspaces 
                 WHERE lifecycle_state != 'archived' 
                 ORDER BY COALESCE(last_activity_at, updated_at) DESC LIMIT 1",
            )
            .map_err(|e| e.to_string())?;

        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let ws_id: String = row.get(0).map_err(|e| e.to_string())?;
            self.get_context_memory(conn, &ws_id)
        } else {
            Ok(None)
        }
    }

    /// Reconstructs and returns the contextual memory response for a specific workspace.
    /// Returns `None` if the workspace does not exist.
    pub fn get_context_memory(
        &self,
        conn: &Connection,
        context_id: &str,
    ) -> Result<Option<ContextMemoryResponse>, String> {
        let ws_exists = crate::db::operations::get_workspace(conn, context_id).is_ok();
        if !ws_exists {
            return Ok(None);
        }

        let mut builder = ContextMemoryBuilder::from_db(conn, context_id)?;
        if let Some(now) = self.frozen_now {
            builder = builder.with_now(now);
        }

        let memory = builder.build()?;
        Ok(Some(ContextMemoryResponse::from_memory(&memory)))
    }

    /// Reconstructs and returns the recent contextual memories up to `limit` entries.
    /// Ordered by most recently active context first.
    pub fn get_recent_context_memories(
        &self,
        conn: &Connection,
        limit: Option<usize>,
    ) -> Result<Vec<ContextMemoryResponse>, String> {
        let limit_val = limit.unwrap_or(5).clamp(1, 50) as i64;

        let mut stmt = conn
            .prepare(
                "SELECT id FROM workspaces 
                 WHERE lifecycle_state != 'archived' 
                 ORDER BY COALESCE(last_activity_at, updated_at) DESC 
                 LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([limit_val], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;

        let mut memories = Vec::new();
        for r in rows {
            if let Ok(ws_id) = r {
                if let Ok(Some(mem)) = self.get_context_memory(conn, &ws_id) {
                    memories.push(mem);
                }
            }
        }

        Ok(memories)
    }

    /// Returns the single most recently active context memory snapshot.
    pub fn get_last_active_context(
        &self,
        conn: &Connection,
    ) -> Result<Option<ContextMemoryResponse>, String> {
        self.get_current_memory(conn)
    }

    /// Returns the last active task inside a specific context.
    /// Inspects the current confident task, persisted candidates, and task history.
    pub fn get_last_active_task(
        &self,
        conn: &Connection,
        context_id: &str,
    ) -> Result<Option<LastActiveTaskResponse>, String> {
        let mem_opt = self.get_context_memory(conn, context_id)?;
        let mem = match mem_opt {
            Some(m) => m,
            None => return Ok(None),
        };

        if let Some(task_ref) = mem.recent_task {
            let is_current = mem.task.as_deref() == Some(&task_ref.title);
            Ok(Some(LastActiveTaskResponse {
                context_id: context_id.to_string(),
                task_id: task_ref.id,
                title: task_ref.title,
                action_verb: task_ref.action_verb,
                target_subject: task_ref.target_subject,
                confidence: task_ref.confidence,
                confidence_band: task_ref.confidence_band,
                provenance: task_ref.provenance,
                last_active_at: task_ref.last_active_at,
                is_current,
            }))
        } else {
            Ok(None)
        }
    }
}

// =========================================================================
// Unit & Integration Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_in_memory_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_query_active_context() {
        let conn = setup_in_memory_db();
        let ws = crate::db::operations::create_workspace(&conn, "TripWise").unwrap();

        let _item = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://tripadvisor.com/tokyo-hotels",
            Some("Tokyo Hotels"),
            r#"{"app_name":"Safari"}"#,
        )
        .unwrap();

        let service = ContextMemoryQueryService::new();
        let resp = service.get_current_memory(&conn).unwrap().unwrap();

        assert_eq!(resp.context, "TripWise");
        assert_eq!(resp.context_id, ws.id);
        assert_eq!(resp.state, "active");
        assert_eq!(resp.context_confidence, "high");
        assert!(!resp.recent_items.is_empty());
    }

    #[test]
    fn test_query_recently_active_context() {
        let conn = setup_in_memory_db();
        let ws = crate::db::operations::create_workspace(&conn, "TripWise").unwrap();

        let fixed_now = chrono::DateTime::parse_from_rfc3339("2026-09-14T18:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let cand = crate::db::models::TaskCandidate {
            id: "task-tokyo".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Tokyo trip itinerary".to_string(),
            confidence: 0.70,
            status: crate::db::models::TaskCandidateStatus::Active,
            first_detected_at: "2026-09-14T10:00:00Z".to_string(),
            last_active_at: "2026-09-14T10:30:00Z".to_string(),
            created_at: "2026-09-14T10:00:00Z".to_string(),
            updated_at: "2026-09-14T10:30:00Z".to_string(),
        };
        crate::db::operations::create_task_candidate(&conn, &cand).unwrap();

        let service = ContextMemoryQueryService::new().with_now(fixed_now);
        let resp = service.get_context_memory(&conn, &ws.id).unwrap().unwrap();

        assert_eq!(resp.context, "TripWise");
        assert_eq!(resp.state, "recently_active");
        assert_eq!(resp.task.as_deref(), Some("Tokyo trip itinerary"));
        assert_eq!(resp.task_confidence.as_deref(), Some("medium"));
    }

    #[test]
    fn test_query_no_active_context() {
        let conn = setup_in_memory_db();
        let service = ContextMemoryQueryService::new();

        let resp = service.get_current_memory(&conn).unwrap();
        assert!(resp.is_none());

        let last_task = service.get_last_active_task(&conn, "non-existent").unwrap();
        assert!(last_task.is_none());
    }

    #[test]
    fn test_query_task_available() {
        let conn = setup_in_memory_db();
        let ws = crate::db::operations::create_workspace(&conn, "Mobile App").unwrap();

        let cand = crate::db::models::TaskCandidate {
            id: "task-auth".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Implementing JWT Authentication".to_string(),
            confidence: 0.92,
            status: crate::db::models::TaskCandidateStatus::Active,
            first_detected_at: "2026-09-14T10:00:00Z".to_string(),
            last_active_at: "2026-09-14T10:30:00Z".to_string(),
            created_at: "2026-09-14T10:00:00Z".to_string(),
            updated_at: "2026-09-14T10:30:00Z".to_string(),
        };
        crate::db::operations::create_task_candidate(&conn, &cand).unwrap();

        let service = ContextMemoryQueryService::new();
        let resp = service.get_context_memory(&conn, &ws.id).unwrap().unwrap();

        assert_eq!(resp.task.as_deref(), Some("Implementing JWT Authentication"));
        assert_eq!(resp.task_confidence.as_deref(), Some("high"));
        assert_eq!(resp.provenance, InformationProvenance::Confirmed);

        let last_task = service.get_last_active_task(&conn, &ws.id).unwrap().unwrap();
        assert_eq!(last_task.title, "Implementing JWT Authentication");
        assert!(last_task.is_current);
    }

    #[test]
    fn test_query_task_unavailable() {
        let conn = setup_in_memory_db();
        let ws = crate::db::operations::create_workspace(&conn, "Aurora Website").unwrap();

        // Generic tool items that should NOT produce a task
        let _item = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://github.com/my-org/aurora",
            Some("GitHub Repository"),
            r#"{"app_name":"GitHub"}"#,
        )
        .unwrap();

        let service = ContextMemoryQueryService::new();
        let resp = service.get_context_memory(&conn, &ws.id).unwrap().unwrap();

        assert_eq!(resp.context, "Aurora Website");
        assert!(resp.task.is_none());
        assert!(resp.task_confidence.is_none());
        assert!(resp.memory_statement.contains("Aurora Website"));
        assert!(!resp.memory_statement.to_lowercase().contains("github"));
    }

    #[test]
    fn test_query_multiple_contexts() {
        let conn = setup_in_memory_db();
        let ws1 = crate::db::operations::create_workspace(&conn, "Workspace 1").unwrap();
        let ws2 = crate::db::operations::create_workspace(&conn, "Workspace 2").unwrap();
        let ws3 = crate::db::operations::create_workspace(&conn, "Workspace 3").unwrap();

        let _i1 = crate::db::operations::create_item(&conn, Some(&ws1.id), None, "text", "W1 item", None, "{}").unwrap();
        let _i2 = crate::db::operations::create_item(&conn, Some(&ws2.id), None, "text", "W2 item", None, "{}").unwrap();
        let _i3 = crate::db::operations::create_item(&conn, Some(&ws3.id), None, "text", "W3 item", None, "{}").unwrap();

        let service = ContextMemoryQueryService::new();
        let recent = service.get_recent_context_memories(&conn, Some(3)).unwrap();

        assert_eq!(recent.len(), 3);
        let names: Vec<&str> = recent.iter().map(|m| m.context.as_str()).collect();
        assert!(names.contains(&"Workspace 1"));
        assert!(names.contains(&"Workspace 2"));
        assert!(names.contains(&"Workspace 3"));
    }

    #[test]
    fn test_query_context_return() {
        let conn = setup_in_memory_db();
        let ws_a = crate::db::operations::create_workspace(&conn, "Portfolio").unwrap();
        let ws_b = crate::db::operations::create_workspace(&conn, "Client Project").unwrap();

        let _trans = crate::db::operations::create_context_transition(
            &conn,
            &crate::db::models::ContextTransition::new(
                Some(ws_b.id.clone()),
                Some(ws_a.id.clone()),
                crate::db::models::ContextTransitionType::ContextReturn,
                0.90,
            ),
        )
        .unwrap();

        let service = ContextMemoryQueryService::new();
        let resp = service.get_context_memory(&conn, &ws_a.id).unwrap().unwrap();

        assert_eq!(resp.context, "Portfolio");
        assert!(resp.evidence.iter().any(|e| e.contains("Returned to Portfolio")));
    }

    #[test]
    fn test_query_low_confidence() {
        let conn = setup_in_memory_db();
        let ws = crate::db::operations::create_workspace(&conn, "Experimental").unwrap();
        crate::db::operations::update_workspace_lifecycle_state(&conn, &ws.id, "dormant").unwrap();

        let service = ContextMemoryQueryService::new();
        let resp = service.get_context_memory(&conn, &ws.id).unwrap().unwrap();

        assert_eq!(resp.context, "Experimental");
        assert_eq!(resp.state, "dormant");
        assert_eq!(resp.context_confidence, "medium");
        assert!(resp.task.is_none());
    }

    #[test]
    fn test_query_contradictory_evidence() {
        let conn = setup_in_memory_db();
        let ws = crate::db::operations::create_workspace(&conn, "Mixed Space").unwrap();

        // Two conflicting candidates with identical confidence
        let c1 = crate::db::models::TaskCandidate {
            id: "cand-1".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Drafting Client Proposal".to_string(),
            confidence: 0.50,
            status: crate::db::models::TaskCandidateStatus::Detected,
            first_detected_at: "2026-09-14T10:00:00Z".to_string(),
            last_active_at: "2026-09-14T10:30:00Z".to_string(),
            created_at: "2026-09-14T10:00:00Z".to_string(),
            updated_at: "2026-09-14T10:30:00Z".to_string(),
        };
        let c2 = crate::db::models::TaskCandidate {
            id: "cand-2".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Fixing Payment Gateway".to_string(),
            confidence: 0.50,
            status: crate::db::models::TaskCandidateStatus::Detected,
            first_detected_at: "2026-09-14T10:00:00Z".to_string(),
            last_active_at: "2026-09-14T10:30:00Z".to_string(),
            created_at: "2026-09-14T10:00:00Z".to_string(),
            updated_at: "2026-09-14T10:30:00Z".to_string(),
        };
        crate::db::operations::create_task_candidate(&conn, &c1).unwrap();
        crate::db::operations::create_task_candidate(&conn, &c2).unwrap();

        let service = ContextMemoryQueryService::new();
        let resp = service.get_context_memory(&conn, &ws.id).unwrap().unwrap();

        // Contradictory/ambiguous unconfirmed tasks without a clear leader should not force an inaccurate task
        assert_eq!(resp.context, "Mixed Space");
    }
}

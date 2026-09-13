use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::context_engine::memory::{
    ContextMemory, ContextMemoryBuilder, InformationProvenance, MemoryExplanation,
    MemoryItemReference, MemoryTaskReference,
};
use crate::context_engine::segments::ContextActivitySegmenter;
use crate::context_engine::work_sequence::{
    format_activity_duration_estimate, format_date_label, RecentWorkSequenceService,
    WorkSequenceEntry,
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
    /// Structured, human-readable explanation answering "Why?" for this memory snapshot
    pub explanation: MemoryExplanation,
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
            explanation: memory.explanation.clone(),
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

/// Usage and primary task information for a context on a specific historical day.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistoricalContextUsage {
    /// Context identifier
    pub context_id: String,
    /// Human-friendly name of the context (e.g. "Aurora Website", "University OS", "Tokyo Trip")
    pub context_name: String,
    /// Number of activity segments or sessions in this context on that day
    pub session_count: usize,
    /// Total duration in seconds
    pub total_duration_seconds: i64,
    /// Gentle estimate: "Active for about 40 minutes"
    pub duration_estimate: String,
    /// Primary task title if known
    pub primary_task: Option<String>,
    /// All tasks worked on in this context on that day
    pub tasks: Vec<String>,
    /// ISO-8601 timestamp of most recent activity on that day
    pub last_active_at: String,
}

/// Summary of a task worked on during a specific historical day.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistoricalTaskSummary {
    /// Task title
    pub title: String,
    /// Context identifier if linked
    pub context_id: Option<String>,
    /// Context name
    pub context_name: String,
    /// Qualitative confidence band
    pub confidence_band: String,
    /// Epistemic classification
    pub provenance: InformationProvenance,
    /// ISO-8601 timestamp of last activity on that day
    pub last_active_at: String,
}

/// Complete historical memory reconstruction for a given day (e.g. Today, Yesterday, Sep 12).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistoricalContextMemoryResponse {
    /// ISO date string (YYYY-MM-DD)
    pub date: String,
    /// Human-friendly relative or formatted label (e.g. "Today", "Yesterday", "Sep 12, 2026")
    pub date_label: String,
    /// List of contexts used during this day
    pub contexts_used: Vec<HistoricalContextUsage>,
    /// Primary tasks worked on during this day across contexts
    pub primary_tasks: Vec<HistoricalTaskSummary>,
    /// Number of context switches detected on this day
    pub context_switches: usize,
    /// Number of returns to earlier contexts on this day
    pub context_returns: usize,
    /// Most recent context active on this day
    pub most_recent_context: Option<HistoricalContextUsage>,
    /// Most recent task active on this day
    pub most_recent_task: Option<HistoricalTaskSummary>,
    /// Important items captured or interacted with on this day
    pub important_captured_items: Vec<MemoryItemReference>,
    /// Meaningful activity segments recorded on this day
    pub meaningful_activity_segments: Vec<WorkSequenceEntry>,
    /// Whether there is sufficient evidence to reconstruct working memory
    pub has_sufficient_evidence: bool,
    /// Truthful, deterministic natural language summary of what was worked on
    pub summary_statement: String,
    /// Structured, human-readable explanation answering "Why?" for this day's memory statement
    pub explanation: MemoryExplanation,
}

/// Helper to parse relative date queries ("today", "yesterday") or ISO strings ("YYYY-MM-DD").
pub fn parse_date_query(query: &str, reference_now: DateTime<Utc>) -> Result<NaiveDate, String> {
    let q = query.trim().to_lowercase();
    let now_naive = reference_now.date_naive();
    if q == "today" {
        Ok(now_naive)
    } else if q == "yesterday" {
        Ok(now_naive - chrono::Duration::days(1))
    } else if let Ok(d) = NaiveDate::parse_from_str(&q, "%Y-%m-%d") {
        Ok(d)
    } else if let Ok(dt) = DateTime::parse_from_rfc3339(query.trim()) {
        Ok(dt.date_naive())
    } else {
        Err(format!(
            "Unrecognized date query '{}'. Expected 'today', 'yesterday', or 'YYYY-MM-DD'.",
            query
        ))
    }
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

    /// Reconstructs historical contextual working memory for a specific calendar date.
    pub fn get_memory_for_day(
        &self,
        conn: &Connection,
        target_date: NaiveDate,
    ) -> Result<HistoricalContextMemoryResponse, String> {
        let date_key = target_date.format("%Y-%m-%d").to_string();
        let reference_now = self.frozen_now.unwrap_or_else(Utc::now);
        let sample_iso = format!("{}T12:00:00Z", date_key);
        let (date_label, _) = format_date_label(&sample_iso, Some(reference_now));

        // 1. Reconstruct activity segments via RecentWorkSequenceService
        let mut segmenter = ContextActivitySegmenter::new();
        let segments = segmenter.build_all_segments(conn)?;
        let work_seq_service = RecentWorkSequenceService::new();
        let work_seq = work_seq_service.reconstruct_from_segments(conn, &segments, Some(60), Some(reference_now));

        // 2. Find entries for this specific date
        let day_entries: Vec<WorkSequenceEntry> = work_seq
            .days
            .iter()
            .find(|d| d.date_key == date_key)
            .map(|d| d.entries.clone())
            .unwrap_or_default();

        // 3. Query items captured on this day
        let start_iso = format!("{} 00:00:00", date_key);
        let end_iso = format!("{} 23:59:59", date_key);
        let start_iso_t = format!("{}T00:00:00Z", date_key);
        let end_iso_t = format!("{}T23:59:59Z", date_key);

        let mut item_stmt = conn
            .prepare(
                "SELECT id, workspace_id, content, title, item_type, metadata, created_at 
                 FROM items 
                 WHERE (created_at >= ?1 AND created_at <= ?2) 
                    OR (created_at >= ?3 AND created_at <= ?4)
                 ORDER BY created_at DESC LIMIT 30",
            )
            .map_err(|e| e.to_string())?;

        let item_rows = item_stmt
            .query_map([&start_iso, &end_iso, &start_iso_t, &end_iso_t], |row| {
                let id: String = row.get(0)?;
                let _ws_id: Option<String> = row.get(1)?;
                let content: String = row.get(2)?;
                let title: Option<String> = row.get(3)?;
                let item_type: String = row.get(4)?;
                let meta_str: String = row.get(5)?;
                let created_at: String = row.get(6)?;

                let (app_name, domain) = if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&meta_str) {
                    (
                        meta.get("app_name").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        meta.get("domain").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    )
                } else {
                    (None, None)
                };

                let clean_title = title.filter(|t| !t.trim().is_empty()).unwrap_or_else(|| {
                    if content.len() > 60 {
                        format!("{}...", &content[..60].trim())
                    } else {
                        content
                    }
                });

                Ok(MemoryItemReference {
                    id,
                    title: Some(clean_title),
                    item_type,
                    timestamp: created_at,
                    provenance: InformationProvenance::Observed,
                    app_name,
                    domain,
                })
            })
            .map_err(|e| e.to_string())?;

        let mut important_captured_items = Vec::new();
        for r in item_rows {
            if let Ok(item) = r {
                important_captured_items.push(item);
            }
        }

        // 4. Aggregate contexts used on this day
        let mut context_map: HashMap<String, (String, usize, i64, Vec<String>, String)> = HashMap::new(); // id -> (name, count, duration, tasks, last_active)
        let mut task_map: HashMap<String, HistoricalTaskSummary> = HashMap::new();
        let mut context_switches = 0;
        let mut context_returns = 0;

        for entry in &day_entries {
            if entry.is_context_return {
                context_returns += 1;
            } else if entry.transition_type.as_deref() == Some("context_switch") {
                context_switches += 1;
            }

            if let Some(ref cid) = entry.context_id {
                let e = context_map.entry(cid.clone()).or_insert_with(|| {
                    (entry.context_name.clone(), 0, 0, Vec::new(), entry.ended_at.clone())
                });
                e.1 += 1;
                e.2 += entry.duration_seconds;
                if entry.ended_at > e.4 {
                    e.4 = entry.ended_at.clone();
                }
                if let Some(ref t) = entry.task_title {
                    if !e.3.contains(t) {
                        e.3.push(t.clone());
                    }
                }
            }

            if let Some(ref task_title) = entry.task_title {
                let key = format!("{}:{}", entry.context_id.as_deref().unwrap_or(""), task_title);
                task_map.entry(key).or_insert_with(|| HistoricalTaskSummary {
                    title: task_title.clone(),
                    context_id: entry.context_id.clone(),
                    context_name: entry.context_name.clone(),
                    confidence_band: entry.task_confidence_band.clone().unwrap_or_else(|| "Medium".to_string()),
                    provenance: entry.task_provenance,
                    last_active_at: entry.ended_at.clone(),
                });
            }
        }

        let mut contexts_used: Vec<HistoricalContextUsage> = context_map
            .into_iter()
            .map(|(id, (name, count, dur, tasks, last_active))| {
                let primary_task = tasks.first().cloned();
                HistoricalContextUsage {
                    context_id: id,
                    context_name: name,
                    session_count: count,
                    total_duration_seconds: dur,
                    duration_estimate: format_activity_duration_estimate(dur),
                    primary_task,
                    tasks,
                    last_active_at: last_active,
                }
            })
            .collect();
        contexts_used.sort_by(|a, b| b.last_active_at.cmp(&a.last_active_at));

        let mut primary_tasks: Vec<HistoricalTaskSummary> = task_map.into_values().collect();
        primary_tasks.sort_by(|a, b| b.last_active_at.cmp(&a.last_active_at));

        let most_recent_context = contexts_used.first().cloned();
        let most_recent_task = primary_tasks.first().cloned();

        let has_sufficient_evidence = !contexts_used.is_empty() || !important_captured_items.is_empty();

        // 5. Generate truthful, deterministic summary statement
        let summary_statement = if !contexts_used.is_empty() {
            let mut parts = Vec::new();
            for ctx in &contexts_used {
                if let Some(ref pt) = ctx.primary_task {
                    parts.push(format!("{} on {}", ctx.context_name, pt));
                } else {
                    parts.push(ctx.context_name.clone());
                }
            }
            if parts.len() == 1 {
                format!("{} you worked on {}.", date_label, parts[0])
            } else if parts.len() == 2 {
                format!("{} you worked across {} and {}.", date_label, parts[0], parts[1])
            } else {
                let last = parts.pop().unwrap();
                format!("{} you worked across {}, and {}.", date_label, parts.join(", "), last)
            }
        } else if !important_captured_items.is_empty() {
            format!(
                "{} you captured {} item{}.",
                date_label,
                important_captured_items.len(),
                if important_captured_items.len() == 1 { "" } else { "s" }
            )
        } else {
            format!("No meaningful activity recorded for {}.", date_label.to_lowercase())
        };

        // 6. Synthesize explainability for historical response
        let mut why_reasons = Vec::new();
        let mut supporting_evidence = Vec::new();

        if !contexts_used.is_empty() {
            let count = contexts_used.len();
            why_reasons.push(format!("{} active context{}", count, if count == 1 { "" } else { "s" }));
            for c in &contexts_used {
                if let Some(ref t) = c.primary_task {
                    supporting_evidence.push(format!("Active task in {}: '{}'", c.context_name, t));
                }
            }
        }
        if !primary_tasks.is_empty() {
            why_reasons.push(format!("{} primary task{} identified", primary_tasks.len(), if primary_tasks.len() == 1 { "" } else { "s" }));
        }
        if !day_entries.is_empty() {
            why_reasons.push(format!("{} activity segment{}", day_entries.len(), if day_entries.len() == 1 { "" } else { "s" }));
        }
        if context_returns > 0 {
            why_reasons.push(format!("{} context return{}", context_returns, if context_returns == 1 { "" } else { "s" }));
        }
        if !important_captured_items.is_empty() {
            why_reasons.push(format!("{} item{} captured", important_captured_items.len(), if important_captured_items.len() == 1 { "" } else { "s" }));
        }

        let explanation = MemoryExplanation {
            summary: format!("Reconstructed working memory for {}.", date_label),
            why_reasons,
            supporting_evidence,
            confidence_band: if has_sufficient_evidence { "High".to_string() } else { "Insufficient".to_string() },
            provenance: if contexts_used.iter().any(|c| c.primary_task.is_some()) { InformationProvenance::Inferred } else { InformationProvenance::Observed },
            uncertainty_note: if !has_sufficient_evidence {
                Some("Insufficient recorded activity to establish clear working memory.".to_string())
            } else {
                None
            },
        };

        Ok(HistoricalContextMemoryResponse {
            date: date_key,
            date_label,
            contexts_used,
            primary_tasks,
            context_switches,
            context_returns,
            most_recent_context,
            most_recent_task,
            important_captured_items,
            meaningful_activity_segments: day_entries,
            has_sufficient_evidence,
            summary_statement,
            explanation,
        })
    }

    /// Queries historical working memory for "Today".
    pub fn get_today_memory(&self, conn: &Connection) -> Result<HistoricalContextMemoryResponse, String> {
        let now = self.frozen_now.unwrap_or_else(Utc::now);
        self.get_memory_for_day(conn, now.date_naive())
    }

    /// Queries historical working memory for "Yesterday".
    pub fn get_yesterday_memory(&self, conn: &Connection) -> Result<HistoricalContextMemoryResponse, String> {
        let now = self.frozen_now.unwrap_or_else(Utc::now);
        let yesterday = now.date_naive() - chrono::Duration::days(1);
        self.get_memory_for_day(conn, yesterday)
    }

    /// Queries historical working memory for a date string ("today", "yesterday", "YYYY-MM-DD").
    pub fn get_historical_memory_for_date(
        &self,
        conn: &Connection,
        date_query: &str,
    ) -> Result<HistoricalContextMemoryResponse, String> {
        let now = self.frozen_now.unwrap_or_else(Utc::now);
        let date = parse_date_query(date_query, now)?;
        self.get_memory_for_day(conn, date)
    }

    /// Queries historical memories for the past N active days.
    pub fn get_recent_historical_memories(
        &self,
        conn: &Connection,
        limit_days: Option<usize>,
    ) -> Result<Vec<HistoricalContextMemoryResponse>, String> {
        let limit = limit_days.unwrap_or(7).clamp(1, 30);
        let now = self.frozen_now.unwrap_or_else(Utc::now);
        let mut results = Vec::new();
        for i in 0..limit {
            let date = now.date_naive() - chrono::Duration::days(i as i64);
            let resp = self.get_memory_for_day(conn, date)?;
            if resp.has_sufficient_evidence || i < 2 {
                results.push(resp);
            }
        }
        Ok(results)
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

    #[test]
    fn test_parse_date_query() {
        let fixed_now = chrono::DateTime::parse_from_rfc3339("2026-09-14T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let today = parse_date_query("today", fixed_now).unwrap();
        assert_eq!(today.to_string(), "2026-09-14");

        let yesterday = parse_date_query("Yesterday", fixed_now).unwrap();
        assert_eq!(yesterday.to_string(), "2026-09-13");

        let specific = parse_date_query("2026-09-10", fixed_now).unwrap();
        assert_eq!(specific.to_string(), "2026-09-10");

        let invalid = parse_date_query("invalid-date", fixed_now);
        assert!(invalid.is_err());
    }

    #[test]
    fn test_query_yesterday_memory_prompt_example() {
        let conn = setup_in_memory_db();
        let fixed_now = chrono::DateTime::parse_from_rfc3339("2026-09-14T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let ws_aurora = crate::db::operations::create_workspace(&conn, "Aurora Website").unwrap();
        let ws_os = crate::db::operations::create_workspace(&conn, "University OS").unwrap();
        let ws_tokyo = crate::db::operations::create_workspace(&conn, "Tokyo Trip").unwrap();

        // Items captured yesterday (2026-09-13)
        let _item_aurora = crate::db::operations::create_item(
            &conn,
            Some(&ws_aurora.id),
            None,
            "text",
            "Hero animation implementation with Framer Motion",
            Some("Hero animation"),
            r#"{"app_name":"VS Code","created_at":"2026-09-13T10:00:00Z"}"#,
        )
        .unwrap();
        conn.execute(
            "UPDATE items SET created_at = '2026-09-13 10:00:00' WHERE id = ?1",
            [&_item_aurora.id],
        )
        .unwrap();

        let _item_os = crate::db::operations::create_item(
            &conn,
            Some(&ws_os.id),
            None,
            "text",
            "Process scheduling assignment for OS course",
            Some("Process scheduling assignment"),
            r#"{"app_name":"CLion","created_at":"2026-09-13T13:00:00Z"}"#,
        )
        .unwrap();
        conn.execute(
            "UPDATE items SET created_at = '2026-09-13 13:00:00' WHERE id = ?1",
            [&_item_os.id],
        )
        .unwrap();

        let _item_tokyo = crate::db::operations::create_item(
            &conn,
            Some(&ws_tokyo.id),
            None,
            "text",
            "Initial trip planning and flight search for Tokyo",
            Some("Initial trip planning"),
            r#"{"app_name":"Safari","created_at":"2026-09-13T16:00:00Z"}"#,
        )
        .unwrap();
        conn.execute(
            "UPDATE items SET created_at = '2026-09-13 16:00:00' WHERE id = ?1",
            [&_item_tokyo.id],
        )
        .unwrap();

        let service = ContextMemoryQueryService::new().with_now(fixed_now);
        let resp = service.get_yesterday_memory(&conn).unwrap();

        assert_eq!(resp.date, "2026-09-13");
        assert_eq!(resp.date_label, "Yesterday");
        assert!(resp.has_sufficient_evidence);
        assert!(!resp.contexts_used.is_empty());

        let context_names: Vec<&str> = resp.contexts_used.iter().map(|c| c.context_name.as_str()).collect();
        assert!(context_names.contains(&"Aurora Website"));
        assert!(context_names.contains(&"University OS"));
        assert!(context_names.contains(&"Tokyo Trip"));

        // Verify summary statement is truthful and mentions the work
        assert!(resp.summary_statement.contains("Yesterday"));
        assert!(resp.summary_statement.contains("Aurora Website"));
    }

    #[test]
    fn test_query_insufficient_evidence_empty_day() {
        let conn = setup_in_memory_db();
        let fixed_now = chrono::DateTime::parse_from_rfc3339("2026-09-14T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let service = ContextMemoryQueryService::new().with_now(fixed_now);
        let resp = service.get_historical_memory_for_date(&conn, "2026-09-01").unwrap();

        assert_eq!(resp.date, "2026-09-01");
        assert!(!resp.has_sufficient_evidence);
        assert!(resp.contexts_used.is_empty());
        assert!(resp.primary_tasks.is_empty());
        assert!(resp.important_captured_items.is_empty());
        assert!(resp.summary_statement.contains("No meaningful activity recorded"));
    }

    #[test]
    fn test_query_today_memory_and_recent_range() {
        let conn = setup_in_memory_db();
        let fixed_now = chrono::DateTime::parse_from_rfc3339("2026-09-14T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let ws = crate::db::operations::create_workspace(&conn, "ContextSplit App").unwrap();
        let item = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Refactoring Context Memory Query",
            Some("Refactoring Context Memory Query"),
            r#"{"app_name":"Rust Rover"}"#,
        )
        .unwrap();
        conn.execute(
            "UPDATE items SET created_at = '2026-09-14 10:00:00' WHERE id = ?1",
            [&item.id],
        )
        .unwrap();

        let service = ContextMemoryQueryService::new().with_now(fixed_now);
        let today_resp = service.get_today_memory(&conn).unwrap();

        assert_eq!(today_resp.date, "2026-09-14");
        assert_eq!(today_resp.date_label, "Today");
        assert!(today_resp.has_sufficient_evidence);
        assert_eq!(today_resp.contexts_used.len(), 1);
        assert_eq!(today_resp.contexts_used[0].context_name, "ContextSplit App");

        let recent_memories = service.get_recent_historical_memories(&conn, Some(5)).unwrap();
        assert!(!recent_memories.is_empty());
        assert_eq!(recent_memories[0].date, "2026-09-14");
    }

    #[test]
    fn test_explainable_context_memory_why_reasons() {
        let conn = setup_in_memory_db();
        let ws = crate::db::operations::create_workspace(&conn, "Aurora Website").unwrap();

        // 4 related items for Hero animation
        for i in 1..=4 {
            let item = crate::db::operations::create_item(
                &conn,
                Some(&ws.id),
                None,
                "text",
                &format!("Hero animation keyframes and framer motion setup part {}", i),
                Some("Hero animation"),
                r#"{"app_name":"VS Code"}"#,
            )
            .unwrap();
            conn.execute(
                "UPDATE items SET created_at = '2026-09-14 10:00:00' WHERE id = ?1",
                [&item.id],
            )
            .unwrap();
        }

        // Active task candidate
        let cand = crate::db::models::TaskCandidate {
            id: "task-hero-anim".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Hero animation".to_string(),
            confidence: 0.85,
            status: crate::db::models::TaskCandidateStatus::Active,
            first_detected_at: "2026-09-14T09:00:00Z".to_string(),
            last_active_at: "2026-09-14T10:30:00Z".to_string(),
            created_at: "2026-09-14T09:00:00Z".to_string(),
            updated_at: "2026-09-14T10:30:00Z".to_string(),
        };
        crate::db::operations::create_task_candidate(&conn, &cand).unwrap();

        let service = ContextMemoryQueryService::new();
        let resp = service.get_context_memory(&conn, &ws.id).unwrap().unwrap();

        assert_eq!(resp.context, "Aurora Website");
        assert_eq!(resp.task.as_deref(), Some("Hero animation"));

        // Verify MemoryExplanation exists and contains reasons
        assert!(!resp.explanation.summary.is_empty());
        assert!(!resp.explanation.why_reasons.is_empty());

        let why_text = resp.explanation.why_reasons.join(" ");
        assert!(why_text.contains("Aurora Website") || why_text.contains("context"));
        assert!(why_text.contains("4 related recent items") || why_text.contains("related recent item"));
        assert!(why_text.contains("task candidate") || why_text.contains("confirmed task"));

        // Verify supporting evidence references are clean (no raw floats or SQL dumps)
        assert!(!resp.explanation.supporting_evidence.is_empty());
    }
}

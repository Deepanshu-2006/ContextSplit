use rusqlite::{params, Connection};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use crate::db::models::{
    ContextTimeline, TimelineEvent, TimelineItemSummary, TimelineSession, TimelineUserAction,
};

/// Retrieves the contextual timeline for a specific workspace/context.
/// Reconstructs contextual activity over time using existing sessions, signals,
/// items, relationships, and memberships.
/// Local-only, non-continuous, no screenshot/keystroke logging.
pub fn get_context_timeline(
    conn: &Connection,
    workspace_id: &str,
) -> Result<ContextTimeline, String> {
    // 1. Fetch Workspace
    let ws = crate::db::operations::get_workspace(conn, workspace_id)
        .map_err(|e| format!("Workspace '{}' not found: {}", workspace_id, e))?;

    // 2. Fetch all Items associated with this Workspace
    // Includes items physically in the workspace and items with a ContextMembership to it
    let mut item_stmt = conn.prepare(
        "SELECT DISTINCT i.id, i.workspace_id, i.session_id, i.item_type, i.content, i.title, i.metadata, i.created_at,
                COALESCE(cm.membership_type, CASE WHEN i.workspace_id = ?1 THEN 'confirmed' ELSE 'suggested' END) as m_type
         FROM items i
         LEFT JOIN context_memberships cm ON i.id = cm.item_id AND cm.workspace_id = ?1
         WHERE i.workspace_id = ?1 OR cm.workspace_id = ?1
         ORDER BY i.created_at DESC",
    ).map_err(|e| e.to_string())?;

    let item_rows = item_stmt.query_map(params![workspace_id], |row| {
        let id: String = row.get(0)?;
        let session_id: Option<String> = row.get(2)?;
        let item_type: String = row.get(3)?;
        let content: String = row.get(4)?;
        let title: Option<String> = row.get(5)?;
        let metadata_str: String = row.get(6)?;
        let created_at: String = row.get(7)?;
        let membership_type: String = row.get(8)?;

        let mut app_name = None;
        let mut domain = None;

        if let Ok(meta) = serde_json::from_str::<Value>(&metadata_str) {
            if let Some(app) = meta.get("app_name").and_then(|v| v.as_str()) {
                app_name = Some(app.to_string());
            }
            if let Some(url_str) = meta.get("original_url").and_then(|v| v.as_str()) {
                if let Ok(parsed) = url::Url::parse(url_str) {
                    domain = parsed.host_str().map(|h| h.to_string());
                }
            }
        }

        let content_snippet = if content.len() > 120 {
            format!("{}...", &content[..120])
        } else {
            content
        };

        Ok(TimelineItemSummary {
            id,
            title,
            item_type,
            content_snippet,
            captured_at: created_at,
            session_id,
            app_name,
            domain,
            membership_type,
        })
    }).map_err(|e| e.to_string())?;

    let mut items = Vec::new();
    let mut item_map = HashMap::new();
    let mut session_ids = HashSet::new();

    for item_res in item_rows {
        if let Ok(item) = item_res {
            if let Some(s_id) = &item.session_id {
                session_ids.insert(s_id.clone());
            }
            item_map.insert(item.id.clone(), item.clone());
            items.push(item);
        }
    }

    // Also look for session IDs in context signals linked to this workspace
    if let Ok(mut sig_session_stmt) = conn.prepare(
        "SELECT DISTINCT session_id FROM context_signals WHERE workspace_id = ?1 AND session_id IS NOT NULL",
    ) {
        if let Ok(rows) = sig_session_stmt.query_map(params![workspace_id], |row| row.get::<_, String>(0)) {
            for s_id_res in rows.flatten() {
                session_ids.insert(s_id_res);
            }
        }
    }

    // 3. Fetch Associated Sessions
    let mut sessions = Vec::new();
    for s_id in &session_ids {
        if let Ok(sess) = crate::db::operations::get_session(conn, s_id) {
            let session_items: Vec<TimelineItemSummary> = items
                .iter()
                .filter(|i| i.session_id.as_deref() == Some(s_id.as_str()))
                .cloned()
                .collect();

            let item_count = session_items.len();
            sessions.push(TimelineSession {
                id: sess.id,
                started_at: sess.started_at,
                last_activity_at: sess.last_activity_at,
                ended_at: sess.ended_at,
                status: sess.status,
                item_count,
                items: session_items,
            });
        }
    }
    // Sort sessions descending by started_at
    sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));

    // 4. Fetch Major User Actions
    let mut major_actions = Vec::new();
    let item_ids: Vec<String> = items.iter().map(|i| i.id.clone()).collect();

    let user_actions_query = if !item_ids.is_empty() {
        let placeholders = vec!["?"; item_ids.len()].join(",");
        format!(
            "SELECT id, item_id, payload, created_at FROM context_signals
             WHERE signal_type = 'user_action' AND (workspace_id = ?1 OR item_id IN ({}))
             ORDER BY created_at DESC",
            placeholders
        )
    } else {
        "SELECT id, item_id, payload, created_at FROM context_signals
         WHERE signal_type = 'user_action' AND workspace_id = ?1
         ORDER BY created_at DESC".to_string()
    };

    if let Ok(mut act_stmt) = conn.prepare(&user_actions_query) {
        let mut query_params: Vec<&dyn rusqlite::ToSql> = vec![&workspace_id];
        for id in &item_ids {
            query_params.push(id);
        }

        if let Ok(rows) = act_stmt.query_map(rusqlite::params_from_iter(query_params), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        }) {
            for row_res in rows.flatten() {
                let (sig_id, it_id, payload_str, timestamp) = row_res;
                let payload_json: Value = serde_json::from_str(&payload_str).unwrap_or(Value::Null);
                let action_raw = payload_json
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("user_action")
                    .to_string();

                let it_title = it_id.as_ref().and_then(|id| {
                    item_map.get(id).and_then(|item| item.title.clone())
                });

                let description = match action_raw.as_str() {
                    "moved_to_workspace" => {
                        if let Some(t) = &it_title {
                            format!("Moved '{}' into {}", t, ws.name)
                        } else {
                            format!("Moved item into {}", ws.name)
                        }
                    }
                    "workspace_created_from_candidate" => {
                        format!("Created workspace '{}' from detected candidate", ws.name)
                    }
                    "candidate_accepted" => format!("Accepted context candidate for {}", ws.name),
                    "candidate_renamed" => {
                        let new_name = payload_json.get("new_name").and_then(|v| v.as_str()).unwrap_or(&ws.name);
                        format!("Renamed candidate to '{}'", new_name)
                    }
                    "candidate_item_removed" => "Removed item from candidate group".to_string(),
                    "suggestion_accepted" => {
                        if let Some(t) = &it_title {
                            format!("Accepted suggestion: Added '{}' to {}", t, ws.name)
                        } else {
                            format!("Accepted suggestion: Added item to {}", ws.name)
                        }
                    }
                    "suggestion_dismissed" => "Dismissed context suggestion".to_string(),
                    "membership_dismissed" => "Dismissed context membership".to_string(),
                    other => other.replace('_', " "),
                };

                major_actions.push(TimelineUserAction {
                    signal_id: sig_id,
                    action_type: action_raw,
                    item_id: it_id,
                    item_title: it_title,
                    timestamp,
                    description,
                    metadata: payload_json,
                });
            }
        }
    }

    // 5. Construct Chronological Activity Feed
    let mut workspace_names = HashMap::new();
    if let Ok(all_workspaces) = crate::db::operations::list_workspaces(conn) {
        for w in all_workspaces {
            workspace_names.insert(w.id, w.name);
        }
    }

    let mut chronological_activity = Vec::new();

    // A) Item capture events (Observed Activity)
    for item in &items {
        let display_title = item.title.clone().unwrap_or_else(|| format!("{} item", item.item_type));
        let src_label = item.app_name.clone().or_else(|| item.domain.clone()).unwrap_or_else(|| "Capture".to_string());
        let ws_name = item.session_id.as_deref().and_then(|_| Some(ws.name.clone())).unwrap_or_else(|| ws.name.clone());

        chronological_activity.push(TimelineEvent {
            id: format!("cap-{}", item.id),
            event_type: "item_captured".to_string(),
            timestamp: item.captured_at.clone(),
            title: display_title,
            description: format!("Type: {} • Source: {}", item.item_type, src_label),
            item_id: Some(item.id.clone()),
            session_id: item.session_id.clone(),
            metadata: serde_json::json!({
                "kind": "observed_activity",
                "item_type": item.item_type,
                "membership_type": item.membership_type,
                "app_name": item.app_name,
                "domain": item.domain,
                "inferred_context": {
                    "id": ws.id,
                    "name": ws_name,
                }
            }),
        });
    }

    // B) Session start and end events
    for session in &sessions {
        chronological_activity.push(TimelineEvent {
            id: format!("sess-start-{}", session.id),
            event_type: "session_started".to_string(),
            timestamp: session.started_at.clone(),
            title: format!("Context session active ({} items)", session.item_count),
            description: format!("ContextSplit session started with {} associated items", session.item_count),
            item_id: None,
            session_id: Some(session.id.clone()),
            metadata: serde_json::json!({
                "status": session.status,
                "item_count": session.item_count,
            }),
        });

        if let Some(ended_at) = &session.ended_at {
            chronological_activity.push(TimelineEvent {
                id: format!("sess-end-{}", session.id),
                event_type: "session_ended".to_string(),
                timestamp: ended_at.clone(),
                title: "Context session completed".to_string(),
                description: format!("Session concluded with status: {}", session.status),
                item_id: None,
                session_id: Some(session.id.clone()),
                metadata: serde_json::json!({
                    "status": session.status,
                }),
            });
        }
    }

    // C) Major user actions
    for action in &major_actions {
        chronological_activity.push(TimelineEvent {
            id: format!("act-{}", action.signal_id),
            event_type: "user_action".to_string(),
            timestamp: action.timestamp.clone(),
            title: action.description.clone(),
            description: format!("User action: {}", action.action_type),
            item_id: action.item_id.clone(),
            session_id: None,
            metadata: action.metadata.clone(),
        });
    }

    // D) Strong Item Relationships within this context
    if !item_ids.is_empty() {
        let placeholders = vec!["?"; item_ids.len()].join(",");
        let rel_sql = format!(
            "SELECT id, item_a_id, item_b_id, relationship_type, strength, created_at
             FROM item_relationships
             WHERE item_a_id IN ({}) AND item_b_id IN ({}) AND strength >= 0.70",
            placeholders, placeholders
        );

        if let Ok(mut rel_stmt) = conn.prepare(&rel_sql) {
            let mut params_vec: Vec<&dyn rusqlite::ToSql> = Vec::new();
            for id in &item_ids {
                params_vec.push(id);
            }
            for id in &item_ids {
                params_vec.push(id);
            }

            if let Ok(rows) = rel_stmt.query_map(rusqlite::params_from_iter(params_vec), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, f64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            }) {
                for r_res in rows.flatten() {
                    let (rel_id, a_id, b_id, rel_type, strength, rel_time) = r_res;
                    let title_a = item_map.get(&a_id).and_then(|i| i.title.clone()).unwrap_or_else(|| "Item".to_string());
                    let title_b = item_map.get(&b_id).and_then(|i| i.title.clone()).unwrap_or_else(|| "Item".to_string());

                    chronological_activity.push(TimelineEvent {
                        id: format!("rel-{}", rel_id),
                        event_type: "relationship_formed".to_string(),
                        timestamp: rel_time,
                        title: format!("Link detected: {} ({:.0}%)", rel_type.replace('_', " "), strength * 100.0),
                        description: format!("'{}' connected with '{}'", title_a, title_b),
                        item_id: Some(a_id),
                        session_id: None,
                        metadata: serde_json::json!({
                            "relationship_type": rel_type,
                            "strength": strength,
                            "item_b_id": b_id,
                        }),
                    });
                }
            }
        }
    }

    // E) Context Transitions (Inferred transitions)
    let mut recorded_transition_ids = HashSet::new();

    // E.1) Persisted transitions for this workspace
    if let Ok(transitions) = crate::db::operations::list_context_transitions_for_context(conn, workspace_id) {
        for trans in transitions {
            recorded_transition_ids.insert(trans.id.clone());

            let from_name = trans.from_context_id.as_deref().and_then(|id| workspace_names.get(id)).cloned();
            let to_name = trans.to_context_id.as_deref().and_then(|id| workspace_names.get(id)).cloned();

            let target_display = to_name.clone().unwrap_or_else(|| ws.name.clone());
            let source_display = from_name.clone().unwrap_or_else(|| "Previous Context".to_string());

            let (arrow_title, desc_summary) = match trans.transition_type {
                crate::db::models::ContextTransitionType::ContextSwitch => (
                    format!("↳ Switched to {}", target_display),
                    format!("Context switch inferred from {} with {:.0}% confidence", source_display, trans.confidence * 100.0),
                ),
                crate::db::models::ContextTransitionType::ContextReturn => (
                    format!("↳ Returned to {}", target_display),
                    format!("Resumed previous activity in {} with {:.0}% confidence", target_display, trans.confidence * 100.0),
                ),
                crate::db::models::ContextTransitionType::ContextStart => (
                    format!("↳ Started {}", target_display),
                    format!("Initial activity emerged in {} with {:.0}% confidence", target_display, trans.confidence * 100.0),
                ),
                crate::db::models::ContextTransitionType::ContextEnd => (
                    format!("↳ Concluded {}", source_display),
                    format!("Inactivity gap detected in {} with {:.0}% confidence", source_display, trans.confidence * 100.0),
                ),
                crate::db::models::ContextTransitionType::AmbiguousTransition => (
                    "↳ Ambiguous context transition".to_string(),
                    format!("Uncertain contextual shift with {:.0}% confidence", trans.confidence * 100.0),
                ),
            };

            let mut evidence_list: Vec<String> = Vec::new();
            if let Ok(meta_json) = serde_json::from_str::<Value>(&trans.metadata) {
                if let Some(ev_arr) = meta_json.get("evidence").and_then(|v| v.as_array()) {
                    for ev in ev_arr {
                        if let Some(s) = ev.as_str() {
                            evidence_list.push(s.to_string());
                        }
                    }
                }
            }

            chronological_activity.push(TimelineEvent {
                id: format!("trans-{}", trans.id),
                event_type: "context_transition".to_string(),
                timestamp: trans.detected_at.clone(),
                title: arrow_title,
                description: desc_summary,
                item_id: trans.trigger_item_id.clone(),
                session_id: trans.session_id.clone(),
                metadata: serde_json::json!({
                    "kind": "inferred_transition",
                    "transition_type": trans.transition_type.as_str(),
                    "confidence": trans.confidence,
                    "from_context_id": trans.from_context_id,
                    "from_context_name": from_name,
                    "to_context_id": trans.to_context_id,
                    "to_context_name": to_name,
                    "evidence": evidence_list,
                }),
            });
        }
    }

    // E.2) If no persisted transitions exist, dynamically derive segments/transitions
    if recorded_transition_ids.is_empty() {
        if let Ok(segments) = crate::context_engine::segments::ContextActivitySegmenter::new().build_segments_for_workspace(conn, workspace_id) {
            for seg in segments {
                if let Some(t_in) = seg.transition_in {
                    if recorded_transition_ids.insert(t_in.id.clone()) {
                        let from_name = t_in.from_context.as_ref().map(|c| c.name.clone());
                        let to_name = t_in.to_context.as_ref().map(|c| c.name.clone());

                        let target_display = to_name.clone().unwrap_or_else(|| ws.name.clone());
                        let source_display = from_name.clone().unwrap_or_else(|| "Previous Context".to_string());

                        let (arrow_title, desc_summary) = match t_in.transition_type {
                            crate::db::models::ContextTransitionType::ContextSwitch => (
                                format!("↳ Switched to {}", target_display),
                                format!("Context switch inferred from {} with {:.0}% confidence", source_display, t_in.confidence * 100.0),
                            ),
                            crate::db::models::ContextTransitionType::ContextReturn => (
                                format!("↳ Returned to {}", target_display),
                                format!("Resumed previous activity in {} with {:.0}% confidence", target_display, t_in.confidence * 100.0),
                            ),
                            crate::db::models::ContextTransitionType::ContextStart => (
                                format!("↳ Started {}", target_display),
                                format!("Initial activity emerged in {} with {:.0}% confidence", target_display, t_in.confidence * 100.0),
                            ),
                            crate::db::models::ContextTransitionType::ContextEnd => (
                                format!("↳ Concluded {}", source_display),
                                format!("Inactivity gap detected in {} with {:.0}% confidence", source_display, t_in.confidence * 100.0),
                            ),
                            crate::db::models::ContextTransitionType::AmbiguousTransition => (
                                "↳ Ambiguous context transition".to_string(),
                                format!("Uncertain contextual shift with {:.0}% confidence", t_in.confidence * 100.0),
                            ),
                        };

                        chronological_activity.push(TimelineEvent {
                            id: format!("trans-{}", t_in.id),
                            event_type: "context_transition".to_string(),
                            timestamp: t_in.timestamp.clone(),
                            title: arrow_title,
                            description: desc_summary,
                            item_id: t_in.trigger_item_id.clone(),
                            session_id: t_in.session_id.clone(),
                            metadata: serde_json::json!({
                                "kind": "inferred_transition",
                                "transition_type": t_in.transition_type.as_str(),
                                "confidence": t_in.confidence,
                                "from_context_id": t_in.from_context.as_ref().map(|c| c.id.clone()),
                                "from_context_name": from_name,
                                "to_context_id": t_in.to_context.as_ref().map(|c| c.id.clone()),
                                "to_context_name": to_name,
                                "evidence": t_in.evidence,
                                "duration_since_previous_activity_seconds": t_in.duration_since_previous_activity_seconds,
                            }),
                        });
                    }
                }
            }
        }
    }

    // Sort chronological activity descending (most recent first)
    chronological_activity.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    // Find earliest and latest activity timestamps
    let first_activity_at = chronological_activity.iter().map(|e| &e.timestamp).min().cloned();
    let latest_activity_at = chronological_activity.iter().map(|e| &e.timestamp).max().cloned();

    Ok(ContextTimeline {
        workspace_id: ws.id,
        workspace_name: ws.name,
        created_at: ws.created_at,
        total_items: items.len(),
        total_sessions: sessions.len(),
        first_activity_at,
        latest_activity_at,
        sessions,
        items,
        major_actions,
        chronological_activity,
    })
}

// ---------------------------------------------------------------------------
// Day 26 Step 5: Development-Only Context Timeline Inspector
// ---------------------------------------------------------------------------

pub const CONTEXT_TIMELINE_INSPECTOR_DISCLAIMER: &str =
    "Development-only context timeline inspector. Strictly non-hallucinatory, aggregates existing database records without secondary event stores.";

/// High-level event categories for the developer timeline inspector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextTimelineCategory {
    Lifecycle,
    Task,
    Memory,
    Assignment,
    Feedback,
    Snapshot,
}

impl ContextTimelineCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Lifecycle => "lifecycle",
            Self::Task => "task",
            Self::Memory => "memory",
            Self::Assignment => "assignment",
            Self::Feedback => "feedback",
            Self::Snapshot => "snapshot",
        }
    }

    pub fn parse_str(s: &str) -> Option<Self> {
        match s.to_lowercase().trim() {
            "lifecycle" => Some(Self::Lifecycle),
            "task" => Some(Self::Task),
            "memory" => Some(Self::Memory),
            "assignment" => Some(Self::Assignment),
            "feedback" => Some(Self::Feedback),
            "snapshot" => Some(Self::Snapshot),
            _ => None,
        }
    }
}

/// A concise, developer-readable event in the contextual timeline.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ContextTimelineInspectorEvent {
    pub id: String,
    pub timestamp: String,
    pub time_display: String,
    pub category: ContextTimelineCategory,
    pub headline: String,
    pub detail: Option<String>,
    pub entity_id: Option<String>,
    pub metadata: serde_json::Value,
}

/// Filter options for querying the contextual timeline.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ContextTimelineFilter {
    pub categories: Option<HashSet<ContextTimelineCategory>>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub search_query: Option<String>,
    pub limit: Option<usize>,
    pub reverse: bool,
}

impl ContextTimelineFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_category(mut self, cat: ContextTimelineCategory) -> Self {
        let mut set = self.categories.unwrap_or_default();
        set.insert(cat);
        self.categories = Some(set);
        self
    }

    pub fn with_categories<I: IntoIterator<Item = ContextTimelineCategory>>(mut self, cats: I) -> Self {
        let mut set = self.categories.unwrap_or_default();
        for c in cats {
            set.insert(c);
        }
        self.categories = Some(set);
        self
    }

    pub fn with_time_range(mut self, start: Option<String>, end: Option<String>) -> Self {
        self.start_time = start;
        self.end_time = end;
        self
    }

    pub fn with_search(mut self, query: impl Into<String>) -> Self {
        self.search_query = Some(query.into());
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn with_reverse(mut self, reverse: bool) -> Self {
        self.reverse = reverse;
        self
    }

    pub fn only_lifecycle() -> Self {
        Self::new().with_category(ContextTimelineCategory::Lifecycle)
    }

    pub fn only_task() -> Self {
        Self::new().with_category(ContextTimelineCategory::Task)
    }

    pub fn only_memory() -> Self {
        Self::new().with_category(ContextTimelineCategory::Memory)
    }

    pub fn only_assignment() -> Self {
        Self::new().with_category(ContextTimelineCategory::Assignment)
    }

    pub fn only_feedback() -> Self {
        Self::new().with_category(ContextTimelineCategory::Feedback)
    }

    pub fn only_snapshot() -> Self {
        Self::new().with_category(ContextTimelineCategory::Snapshot)
    }

    pub fn matches(&self, event: &ContextTimelineInspectorEvent) -> bool {
        if let Some(cats) = &self.categories {
            if !cats.contains(&event.category) {
                return false;
            }
        }
        if let Some(start) = &self.start_time {
            if event.timestamp < *start {
                return false;
            }
        }
        if let Some(end) = &self.end_time {
            if event.timestamp > *end {
                return false;
            }
        }
        if let Some(q) = &self.search_query {
            let q_lower = q.to_lowercase();
            let headline_match = event.headline.to_lowercase().contains(&q_lower);
            let detail_match = event
                .detail
                .as_ref()
                .map(|d| d.to_lowercase().contains(&q_lower))
                .unwrap_or(false);
            if !headline_match && !detail_match {
                return false;
            }
        }
        true
    }
}

/// Chronological report containing contextual timeline events for developer inspection.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ContextTimelineInspectorReport {
    pub context_id: String,
    pub context_name: String,
    pub total_events: usize,
    pub events: Vec<ContextTimelineInspectorEvent>,
    pub categories_present: Vec<ContextTimelineCategory>,
    pub first_event_at: Option<String>,
    pub latest_event_at: Option<String>,
}

impl ContextTimelineInspectorReport {
    /// Formats the chronological events into the standard developer-readable timeline view.
    /// Example:
    /// 09:00
    /// Context became active
    ///
    /// 09:05
    /// Repository detected
    pub fn format_developer_view(&self) -> String {
        let mut out = String::new();
        for (i, ev) in self.events.iter().enumerate() {
            if i > 0 {
                out.push_str("\n\n");
            }
            out.push_str(&format!("{}\n{}", ev.time_display, ev.headline));
        }
        out
    }

    /// Formats the timeline view including optional sanitized details.
    pub fn format_detailed_developer_view(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("CONTEXT TIMELINE: {}\n", self.context_name.to_uppercase()));
        out.push_str(&format!("ID: {}\n", self.context_id));
        out.push_str(&format!("Total Events: {}\n", self.total_events));
        out.push_str("----------------------------------------\n\n");
        for (i, ev) in self.events.iter().enumerate() {
            if i > 0 {
                out.push_str("\n\n");
            }
            out.push_str(&format!("{}\n{}", ev.time_display, ev.headline));
            if let Some(detail) = &ev.detail {
                if !detail.is_empty() {
                    out.push_str(&format!("\n  {}", detail));
                }
            }
        }
        out
    }
}

/// Formats a raw timestamp or ISO-8601 string to a clean `HH:MM` developer time display.
pub fn format_timestamp_display(ts: &str) -> String {
    if ts.len() == 5 && ts.contains(':') {
        return ts.to_string();
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        return dt.format("%H:%M").to_string();
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S") {
        return dt.format("%H:%M").to_string();
    }
    if let Some(t_idx) = ts.find('T') {
        if ts.len() >= t_idx + 6 {
            return ts[t_idx + 1..t_idx + 6].to_string();
        }
    }
    ts.to_string()
}

/// Inspector service to build contextual timelines without duplicate event stores.
pub struct ContextTimelineInspector;

impl ContextTimelineInspector {
    /// Reconstructs the chronological event timeline for a given context directly from existing database tables.
    pub fn inspect_context(
        conn: &Connection,
        context_id: &str,
        filter: Option<&ContextTimelineFilter>,
    ) -> Result<ContextTimelineInspectorReport, String> {
        let ws = crate::db::operations::get_workspace(conn, context_id)
            .map_err(|e| format!("Context '{}' not found: {}", context_id, e))?;

        let mut events: Vec<ContextTimelineInspectorEvent> = Vec::new();

        // 1. LIFECYCLE: Sessions & Transitions
        // Fetch sessions linked to this context
        let mut session_ids = HashSet::new();
        if let Ok(all_sessions) = crate::db::operations::list_sessions(conn) {
            for sess in all_sessions {
                // Check if session had activity in this workspace
                let has_items = conn.query_row(
                    "SELECT COUNT(*) FROM items WHERE session_id = ?1 AND (workspace_id = ?2 OR id IN (SELECT item_id FROM context_memberships WHERE workspace_id = ?2))",
                    params![sess.id, context_id],
                    |row| row.get::<_, i64>(0),
                ).unwrap_or(0) > 0;

                let has_signals = conn.query_row(
                    "SELECT COUNT(*) FROM context_signals WHERE session_id = ?1 AND workspace_id = ?2",
                    params![sess.id, context_id],
                    |row| row.get::<_, i64>(0),
                ).unwrap_or(0) > 0;

                if has_items || has_signals {
                    session_ids.insert(sess.id.clone());

                    events.push(ContextTimelineInspectorEvent {
                        id: format!("sess-start-{}", sess.id),
                        timestamp: sess.started_at.clone(),
                        time_display: format_timestamp_display(&sess.started_at),
                        category: ContextTimelineCategory::Lifecycle,
                        headline: "Context became active".to_string(),
                        detail: Some(format!("Session status: {}", sess.status)),
                        entity_id: Some(sess.id.clone()),
                        metadata: serde_json::json!({
                            "session_id": sess.id,
                            "status": sess.status,
                        }),
                    });

                    if let Some(ended_at) = &sess.ended_at {
                        let is_paused = sess.status.to_lowercase().contains("pause")
                            || sess.status.to_lowercase().contains("idle");
                        let headline = if is_paused {
                            "Context paused".to_string()
                        } else {
                            "Context concluded".to_string()
                        };

                        events.push(ContextTimelineInspectorEvent {
                            id: format!("sess-end-{}", sess.id),
                            timestamp: ended_at.clone(),
                            time_display: format_timestamp_display(ended_at),
                            category: ContextTimelineCategory::Lifecycle,
                            headline,
                            detail: Some(format!("Session status: {}", sess.status)),
                            entity_id: Some(sess.id.clone()),
                            metadata: serde_json::json!({
                                "session_id": sess.id,
                                "status": sess.status,
                            }),
                        });
                    }
                }
            }
        }

        // Transitions for this context
        if let Ok(transitions) = crate::db::operations::list_context_transitions_for_context(conn, context_id) {
            for trans in transitions {
                let is_target = trans.to_context_id.as_deref() == Some(context_id);
                let (category, headline) = match trans.transition_type {
                    crate::db::models::ContextTransitionType::ContextStart => {
                        (ContextTimelineCategory::Lifecycle, "Context became active".to_string())
                    }
                    crate::db::models::ContextTransitionType::ContextReturn => {
                        (ContextTimelineCategory::Lifecycle, "Context resumed".to_string())
                    }
                    crate::db::models::ContextTransitionType::ContextEnd => {
                        (ContextTimelineCategory::Lifecycle, "Context paused".to_string())
                    }
                    crate::db::models::ContextTransitionType::ContextSwitch => {
                        if is_target {
                            (ContextTimelineCategory::Lifecycle, "Context became active".to_string())
                        } else {
                            (ContextTimelineCategory::Lifecycle, "Context paused".to_string())
                        }
                    }
                    crate::db::models::ContextTransitionType::AmbiguousTransition => {
                        (ContextTimelineCategory::Lifecycle, "Context transition detected".to_string())
                    }
                };

                events.push(ContextTimelineInspectorEvent {
                    id: format!("trans-{}", trans.id),
                    timestamp: trans.detected_at.clone(),
                    time_display: format_timestamp_display(&trans.detected_at),
                    category,
                    headline,
                    detail: Some(format!("Transition confidence: {:.0}%", trans.confidence * 100.0)),
                    entity_id: Some(trans.id),
                    metadata: serde_json::json!({
                        "transition_type": trans.transition_type.as_str(),
                        "confidence": trans.confidence,
                    }),
                });
            }
        }

        // 2. ASSIGNMENT & FEEDBACK: Context Signals
        if let Ok(mut sig_stmt) = conn.prepare(
            "SELECT id, item_id, session_id, signal_type, payload, created_at
             FROM context_signals
             WHERE workspace_id = ?1 OR session_id IN (SELECT session_id FROM items WHERE workspace_id = ?1)
             ORDER BY created_at ASC"
        ) {
            if let Ok(rows) = sig_stmt.query_map(params![context_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            }) {
                for r in rows.flatten() {
                    let (sig_id, it_id, sess_id, sig_type, payload_str, created_at) = r;
                    let payload: serde_json::Value = serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null);

                    let (cat, headline, detail) = match sig_type.as_str() {
                        "repository_signal" | "git_remote" | "repository" => {
                            let repo_name = payload.get("repository")
                                .or_else(|| payload.get("repo"))
                                .or_else(|| payload.get("repo_name"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("Repository");
                            (
                                ContextTimelineCategory::Assignment,
                                "Repository detected".to_string(),
                                Some(repo_name.to_string()),
                            )
                        }
                        "project_path" | "path_match" => {
                            let path = payload.get("path").and_then(|v| v.as_str()).unwrap_or("Project path");
                            (
                                ContextTimelineCategory::Assignment,
                                "Project path matched".to_string(),
                                Some(path.to_string()),
                            )
                        }
                        "user_action" | "feedback" => {
                            let action = payload.get("action").and_then(|v| v.as_str()).unwrap_or("action");
                            match action {
                                "rejected" | "candidate_item_removed" | "membership_dismissed" | "suggestion_dismissed" | "item_dismissed" => (
                                    ContextTimelineCategory::Feedback,
                                    "User rejected unrelated item".to_string(),
                                    payload.get("reason").and_then(|v| v.as_str()).map(|s| s.to_string()),
                                ),
                                "confirmed" | "moved_to_workspace" | "candidate_accepted" | "suggestion_accepted" => (
                                    ContextTimelineCategory::Feedback,
                                    "User confirmed repository relationship".to_string(),
                                    Some("Confirmed assignment".to_string()),
                                ),
                                "task_accepted" | "task_completed" => (
                                    ContextTimelineCategory::Feedback,
                                    "User accepted task candidate".to_string(),
                                    None,
                                ),
                                other => (
                                    ContextTimelineCategory::Feedback,
                                    format!("User action: {}", other.replace('_', " ")),
                                    None,
                                ),
                            }
                        }
                        "pause_signal" => (
                            ContextTimelineCategory::Lifecycle,
                            "Context paused".to_string(),
                            None,
                        ),
                        "resume_signal" => (
                            ContextTimelineCategory::Lifecycle,
                            "Context resumed".to_string(),
                            None,
                        ),
                        "important_item" | "memory_signal" | "important_memory" => {
                            let title = payload.get("title").and_then(|v| v.as_str()).unwrap_or("Important item");
                            (
                                ContextTimelineCategory::Memory,
                                "Important item identified".to_string(),
                                Some(title.to_string()),
                            )
                        }
                        _ => {
                            // Check if payload indicates a repository
                            if let Some(repo) = payload.get("repository").and_then(|v| v.as_str()) {
                                (
                                    ContextTimelineCategory::Assignment,
                                    "Repository detected".to_string(),
                                    Some(repo.to_string()),
                                )
                            } else {
                                continue;
                            }
                        }
                    };

                    events.push(ContextTimelineInspectorEvent {
                        id: format!("sig-{}", sig_id),
                        timestamp: created_at.clone(),
                        time_display: format_timestamp_display(&created_at),
                        category: cat,
                        headline,
                        detail,
                        entity_id: it_id.or(sess_id),
                        metadata: payload,
                    });
                }
            }
        }

        // 3. TASK: Task candidates
        if let Ok(candidates) = crate::db::operations::list_task_candidates_for_context(conn, context_id) {
            for cand in candidates {
                events.push(ContextTimelineInspectorEvent {
                    id: format!("task-cand-{}", cand.id),
                    timestamp: cand.created_at.clone(),
                    time_display: format_timestamp_display(&cand.created_at),
                    category: ContextTimelineCategory::Task,
                    headline: "Task candidate created".to_string(),
                    detail: Some(cand.title.clone()),
                    entity_id: Some(cand.id.clone()),
                    metadata: serde_json::json!({
                        "task_id": cand.id,
                        "title": cand.title,
                        "status": cand.status.as_str(),
                        "confidence": cand.confidence,
                    }),
                });

                if cand.updated_at != cand.created_at {
                    let status_headline = match cand.status {
                        crate::db::models::TaskCandidateStatus::Active => "Task candidate activated",
                        crate::db::models::TaskCandidateStatus::Confirmed => "Task candidate confirmed",
                        crate::db::models::TaskCandidateStatus::Dismissed => "Task candidate dismissed",
                        _ => "Task candidate updated",
                    };
                    events.push(ContextTimelineInspectorEvent {
                        id: format!("task-update-{}", cand.id),
                        timestamp: cand.updated_at.clone(),
                        time_display: format_timestamp_display(&cand.updated_at),
                        category: ContextTimelineCategory::Task,
                        headline: status_headline.to_string(),
                        detail: Some(cand.title.clone()),
                        entity_id: Some(cand.id.clone()),
                        metadata: serde_json::json!({
                            "task_id": cand.id,
                            "status": cand.status.as_str(),
                        }),
                    });
                }
            }
        }

        // 4. MEMORY: Important Memories & High-Signal Items
        if let Ok(important_memories) = crate::context_engine::important_memory::ContextMemoryQueryService::get_important_memories(conn, context_id, None) {
            for mem in important_memories {
                // Only include item/document memories (skip tasks and workspace facts)
                if mem.memory_type == crate::context_engine::important_memory::ImportantMemoryType::ActiveTask
                    || mem.memory_type == crate::context_engine::important_memory::ImportantMemoryType::RecentTask
                    || mem.memory_type == crate::context_engine::important_memory::ImportantMemoryType::ConfirmedContextFact
                    || mem.memory_type == crate::context_engine::important_memory::ImportantMemoryType::MeaningfulState
                {
                    continue;
                }
                let ts = mem.first_seen_at.as_deref().or(mem.last_seen_at.as_deref()).unwrap_or(&ws.created_at);
                events.push(ContextTimelineInspectorEvent {
                    id: format!("mem-{}", mem.id),
                    timestamp: ts.to_string(),
                    time_display: format_timestamp_display(ts),
                    category: ContextTimelineCategory::Memory,
                    headline: "Important item identified".to_string(),
                    detail: Some(format!("{} ({:?} band, importance {:.2})", mem.title, mem.importance_band, mem.importance)),
                    entity_id: Some(mem.source_id.clone()),
                    metadata: serde_json::json!({
                        "memory_id": mem.id,
                        "source_id": mem.source_id,
                        "importance_band": format!("{:?}", mem.importance_band),
                        "importance": mem.importance,
                    }),
                });
            }
        }

        // Also check items assigned or confirmed to this workspace for important items
        if let Ok(items) = crate::db::operations::list_items(conn, Some(context_id)) {
            for it in items {
                let already_captured = events.iter().any(|e| {
                    e.category == ContextTimelineCategory::Memory && e.entity_id.as_deref() == Some(&it.id)
                });
                if already_captured {
                    continue;
                }

                let is_important = it.item_type == "file"
                    || it.metadata.contains("VS Code")
                    || it.metadata.contains("important")
                    || it.metadata.contains("core");
                if is_important {
                    events.push(ContextTimelineInspectorEvent {
                        id: format!("item-mem-{}", it.id),
                        timestamp: it.created_at.clone(),
                        time_display: format_timestamp_display(&it.created_at),
                        category: ContextTimelineCategory::Memory,
                        headline: "Important item identified".to_string(),
                        detail: it.title.clone(),
                        entity_id: Some(it.id.clone()),
                        metadata: serde_json::json!({
                            "item_id": it.id,
                            "item_type": it.item_type,
                        }),
                    });
                }
            }
        }

        // 5. SNAPSHOT: Context Snapshots
        if let Ok(snapshots) = crate::db::operations::list_context_snapshots_bounded(conn, context_id, 100) {
            for (idx, snap) in snapshots.iter().enumerate() {
                let headline = if idx > 0 || snapshots.len() == 1 {
                    "Context snapshot updated".to_string()
                } else {
                    "Context snapshot created".to_string()
                };

                let items_cnt = snap.workset_item_references.len();
                let tasks_cnt = snap.recent_task_references.len();

                events.push(ContextTimelineInspectorEvent {
                    id: format!("snap-{}", snap.id),
                    timestamp: snap.created_at.clone(),
                    time_display: format_timestamp_display(&snap.created_at),
                    category: ContextTimelineCategory::Snapshot,
                    headline,
                    detail: Some(format!("State {:?} ({} items, {} tasks)", snap.state, items_cnt, tasks_cnt)),
                    entity_id: Some(snap.id.clone()),
                    metadata: serde_json::json!({
                        "snapshot_id": snap.id,
                        "state": format!("{:?}", snap.state),
                        "item_count": items_cnt,
                    }),
                });
            }
        }

        // Sort events chronologically (ascending by timestamp by default)
        events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        // Deduplicate identical headline + timestamp events
        let mut deduplicated: Vec<ContextTimelineInspectorEvent> = Vec::new();
        let mut seen = HashSet::new();
        for ev in events {
            let key = format!("{}:{}:{}", ev.timestamp, ev.headline, ev.category.as_str());
            if seen.insert(key) {
                deduplicated.push(ev);
            }
        }

        // Apply filter if provided
        let filtered_events: Vec<ContextTimelineInspectorEvent> = if let Some(flt) = filter {
            let mut matched: Vec<_> = deduplicated.into_iter().filter(|ev| flt.matches(ev)).collect();
            if flt.reverse {
                matched.reverse();
            }
            if let Some(lim) = flt.limit {
                matched.truncate(lim);
            }
            matched
        } else {
            deduplicated
        };

        // Extract categories present and timestamp bounds
        let mut cats_present = HashSet::new();
        for ev in &filtered_events {
            cats_present.insert(ev.category);
        }
        let categories_present: Vec<ContextTimelineCategory> = cats_present.into_iter().collect();

        let first_event_at = filtered_events.first().map(|e| e.timestamp.clone());
        let latest_event_at = filtered_events.last().map(|e| e.timestamp.clone());
        let total_events = filtered_events.len();

        Ok(ContextTimelineInspectorReport {
            context_id: ws.id,
            context_name: ws.name,
            total_events,
            events: filtered_events,
            categories_present,
            first_event_at,
            latest_event_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use crate::db::operations::{create_item, create_session, create_workspace};
    use crate::context_engine::operations::create_context_signal;

    fn setup_in_memory_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_context_timeline_retrieval() {
        let conn = setup_in_memory_db();

        // 1. Create a Workspace
        let ws = create_workspace(&conn, "Paris Vacation").unwrap();

        // 2. Create a Session
        let session = create_session(&conn).unwrap();

        // 3. Create items associated with this session and workspace
        let item1 = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://airbnb.com/paris-loft",
            Some("Paris Loft Booking"),
            "{\"app_name\":\"Safari\",\"original_url\":\"https://airbnb.com/paris-loft\"}",
        ).unwrap();

        let item2 = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "Museum tickets: Louvre, Orsay, Centre Pompidou",
            Some("Museum Itinerary"),
            "{\"app_name\":\"Notes\"}",
        ).unwrap();

        // Item 3 in Inbox, but with a suggested ContextMembership to Paris Vacation
        let item3 = create_item(
            &conn,
            None,
            Some(&session.id),
            "url",
            "https://booking.com/paris-trains",
            Some("Paris Train Tickets"),
            "{\"app_name\":\"Safari\"}",
        ).unwrap();
        crate::db::operations::create_or_update_membership(&conn, &item3.id, &ws.id, "suggested", 0.85, "context_engine").unwrap();

        // 4. Record a relationship between item1 and item2
        let _ = crate::db::operations::create_item_relationship(
            &conn,
            &item1.id,
            &item2.id,
            "same_session",
            0.95,
            Some("{\"session\":\"same\"}"),
        ).unwrap();

        // 5. Record a user action signal
        let action_payload = serde_json::json!({
            "action": "moved_to_workspace",
            "destination_workspace": ws.id,
            "source": "inbox"
        }).to_string();
        let _ = create_context_signal(
            &conn,
            Some(&ws.id),
            Some(&item1.id),
            "user_action",
            Some(&session.id),
            &action_payload,
        ).unwrap();

        // 6. Retrieve the Context Timeline
        let timeline = get_context_timeline(&conn, &ws.id).unwrap();

        // Assertions
        assert_eq!(timeline.workspace_id, ws.id);
        assert_eq!(timeline.workspace_name, "Paris Vacation");
        assert_eq!(timeline.total_items, 3); // 2 confirmed + 1 suggested
        assert_eq!(timeline.total_sessions, 1);

        // Verify Sessions
        assert_eq!(timeline.sessions.len(), 1);
        assert_eq!(timeline.sessions[0].id, session.id);
        assert_eq!(timeline.sessions[0].item_count, 3);

        // Verify Items
        assert_eq!(timeline.items.len(), 3);
        let confirmed_count = timeline.items.iter().filter(|i| i.membership_type == "confirmed").count();
        let suggested_count = timeline.items.iter().filter(|i| i.membership_type == "suggested").count();
        assert_eq!(confirmed_count, 2);
        assert_eq!(suggested_count, 1);

        // Verify Major User Actions
        assert!(!timeline.major_actions.is_empty());
        let move_action = &timeline.major_actions[0];
        assert_eq!(move_action.action_type, "moved_to_workspace");
        assert!(move_action.description.contains("Paris Loft Booking"));

        // Verify Chronological Activity Feed
        assert!(!timeline.chronological_activity.is_empty());
        let event_types: HashSet<&str> = timeline.chronological_activity.iter().map(|e| e.event_type.as_str()).collect();
        assert!(event_types.contains("item_captured"));
        assert!(event_types.contains("session_started"));
        assert!(event_types.contains("user_action"));
        assert!(event_types.contains("relationship_formed"));

        // Verify timestamps
        assert!(timeline.first_activity_at.is_some());
        assert!(timeline.latest_activity_at.is_some());
    }

    #[test]
    fn test_context_timeline_includes_transitions() {
        let conn = setup_in_memory_db();
        let ws_college = create_workspace(&conn, "College").unwrap();
        let ws_portfolio = create_workspace(&conn, "Portfolio").unwrap();
        let session = create_session(&conn).unwrap();

        // 1. Create a persisted context switch transition: College -> Portfolio
        let trans = crate::db::models::ContextTransition {
            id: uuid::Uuid::new_v4().to_string(),
            from_context_id: Some(ws_college.id.clone()),
            to_context_id: Some(ws_portfolio.id.clone()),
            transition_type: crate::db::models::ContextTransitionType::ContextSwitch,
            confidence: 0.92,
            detected_at: "2026-09-10T10:32:00Z".to_string(),
            session_id: Some(session.id.clone()),
            trigger_item_id: None,
            trigger_signal_id: None,
            metadata: "{\"evidence\":[\"Active app switched to Figma\",\"Semantic match 0.88\"]}".to_string(),
            created_at: "2026-09-10T10:32:00Z".to_string(),
        };
        crate::db::operations::create_context_transition(&conn, &trans).unwrap();

        // 2. Fetch timeline for Portfolio
        let timeline = get_context_timeline(&conn, &ws_portfolio.id).unwrap();

        let transition_events: Vec<_> = timeline
            .chronological_activity
            .iter()
            .filter(|e| e.event_type == "context_transition")
            .collect();

        assert_eq!(transition_events.len(), 1);
        let trans_ev = &transition_events[0];
        assert_eq!(trans_ev.title, "↳ Switched to Portfolio");
        assert!(trans_ev.description.contains("92% confidence"));
        assert_eq!(trans_ev.metadata.get("kind").and_then(|v| v.as_str()), Some("inferred_transition"));
        assert_eq!(trans_ev.metadata.get("transition_type").and_then(|v| v.as_str()), Some("context_switch"));
        assert_eq!(trans_ev.metadata.get("from_context_name").and_then(|v| v.as_str()), Some("College"));
        assert_eq!(trans_ev.metadata.get("to_context_name").and_then(|v| v.as_str()), Some("Portfolio"));
    }

    #[test]
    fn test_context_timeline_inspector_matching_example() {
        let conn = setup_in_memory_db();
        let ws = create_workspace(&conn, "PayX").unwrap();
        let session = create_session(&conn).unwrap();

        // 09:00 - Context became active
        let _ = conn.execute(
            "UPDATE sessions SET started_at = '2026-09-24T09:00:00Z', status = 'active' WHERE id = ?1",
            params![session.id],
        );
        let _ = create_context_signal(
            &conn,
            Some(&ws.id),
            None,
            "session_active",
            Some(&session.id),
            "{\"status\":\"active\"}",
        ).unwrap();

        // 09:05 - Repository detected
        let sig_repo = serde_json::json!({
            "repository": "PayX",
            "remote": "github.com/org/payx"
        }).to_string();
        let _ = conn.execute(
            "INSERT INTO context_signals (id, workspace_id, item_id, session_id, signal_type, payload, created_at, updated_at)
             VALUES (?1, ?2, NULL, ?3, 'repository_signal', ?4, '2026-09-24T09:05:00Z', '2026-09-24T09:05:00Z')",
            params![uuid::Uuid::new_v4().to_string(), ws.id, session.id, sig_repo],
        ).unwrap();

        // 09:12 - Task candidate created
        let cand = crate::db::models::TaskCandidate {
            id: uuid::Uuid::new_v4().to_string(),
            context_id: Some(ws.id.clone()),
            title: "Implement payment webhook listener".to_string(),
            confidence: 0.95,
            status: crate::db::models::TaskCandidateStatus::Detected,
            first_detected_at: "2026-09-24T09:12:00Z".to_string(),
            last_active_at: "2026-09-24T09:12:00Z".to_string(),
            created_at: "2026-09-24T09:12:00Z".to_string(),
            updated_at: "2026-09-24T09:12:00Z".to_string(),
        };
        crate::db::operations::create_task_candidate(&conn, &cand).unwrap();

        // 09:20 - Important item identified
        let item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "file",
            "src/controllers/CheckoutController.ts",
            Some("CheckoutController.ts"),
            "{\"app_name\":\"VS Code\"}",
        ).unwrap();
        let _ = conn.execute(
            "UPDATE items SET created_at = '2026-09-24T09:20:00Z' WHERE id = ?1",
            params![item.id],
        );
        let _ = crate::db::operations::create_or_update_membership(
            &conn,
            &item.id,
            &ws.id,
            "confirmed",
            1.0,
            "user",
        );
        let _ = conn.execute(
            "UPDATE context_memberships SET created_at = '2026-09-24T09:20:00Z', updated_at = '2026-09-24T09:20:00Z' WHERE item_id = ?1",
            params![item.id],
        );

        // 09:45 - Context paused (transition out)
        let trans_pause = crate::db::models::ContextTransition {
            id: uuid::Uuid::new_v4().to_string(),
            from_context_id: Some(ws.id.clone()),
            to_context_id: None,
            transition_type: crate::db::models::ContextTransitionType::ContextEnd,
            confidence: 0.88,
            detected_at: "2026-09-24T09:45:00Z".to_string(),
            session_id: Some(session.id.clone()),
            trigger_item_id: None,
            trigger_signal_id: None,
            metadata: "{}".to_string(),
            created_at: "2026-09-24T09:45:00Z".to_string(),
        };
        crate::db::operations::create_context_transition(&conn, &trans_pause).unwrap();

        // 10:30 - Context resumed
        let trans_resume = crate::db::models::ContextTransition {
            id: uuid::Uuid::new_v4().to_string(),
            from_context_id: None,
            to_context_id: Some(ws.id.clone()),
            transition_type: crate::db::models::ContextTransitionType::ContextReturn,
            confidence: 0.94,
            detected_at: "2026-09-24T10:30:00Z".to_string(),
            session_id: Some(session.id.clone()),
            trigger_item_id: None,
            trigger_signal_id: None,
            metadata: "{}".to_string(),
            created_at: "2026-09-24T10:30:00Z".to_string(),
        };
        crate::db::operations::create_context_transition(&conn, &trans_resume).unwrap();

        // 10:45 - User rejected unrelated item
        let sig_reject = serde_json::json!({
            "action": "rejected",
            "reason": "Unrelated item from University OS"
        }).to_string();
        let _ = conn.execute(
            "INSERT INTO context_signals (id, workspace_id, item_id, session_id, signal_type, payload, created_at, updated_at)
             VALUES (?1, ?2, NULL, ?3, 'user_action', ?4, '2026-09-24T10:45:00Z', '2026-09-24T10:45:00Z')",
            params![uuid::Uuid::new_v4().to_string(), ws.id, session.id, sig_reject],
        ).unwrap();

        // 11:00 - Context snapshot updated
        let mut snap = crate::context_engine::snapshot::ContextSnapshotBuilder::new(&ws.id, &ws.name).build();
        snap.created_at = "2026-09-24T11:00:00Z".to_string();
        let snap_json = serde_json::to_string(&snap).unwrap();
        let _ = conn.execute(
            "INSERT INTO context_snapshots (id, context_id, context_name, created_at, state, snapshot_confidence, quality_rating, snapshot_json)
             VALUES (?1, ?2, ?3, '2026-09-24T11:00:00Z', 'Active', 0.95, 'complete', ?4)",
            params![snap.id, ws.id, ws.name, snap_json],
        ).unwrap();

        // Run inspection
        let report = ContextTimelineInspector::inspect_context(&conn, &ws.id, None).unwrap();
        assert_eq!(report.context_name, "PayX");
        assert_eq!(report.total_events, 8);

        let dev_view = report.format_developer_view();
        let expected_lines = vec![
            "09:00",
            "Context became active",
            "09:05",
            "Repository detected",
            "09:12",
            "Task candidate created",
            "09:20",
            "Important item identified",
            "09:45",
            "Context paused",
            "10:30",
            "Context resumed",
            "10:45",
            "User rejected unrelated item",
            "11:00",
            "Context snapshot updated",
        ];

        for expected in expected_lines {
            assert!(dev_view.contains(expected), "Dev view missing line: {}", expected);
        }

        // Test Filtering by category
        let lifecycle_filter = ContextTimelineFilter::only_lifecycle();
        let lifecycle_report = ContextTimelineInspector::inspect_context(&conn, &ws.id, Some(&lifecycle_filter)).unwrap();
        assert_eq!(lifecycle_report.total_events, 3); // 09:00 active, 09:45 paused, 10:30 resumed
        assert!(lifecycle_report.events.iter().all(|e| e.category == ContextTimelineCategory::Lifecycle));

        let task_filter = ContextTimelineFilter::only_task();
        let task_report = ContextTimelineInspector::inspect_context(&conn, &ws.id, Some(&task_filter)).unwrap();
        assert_eq!(task_report.total_events, 1);
        assert_eq!(task_report.events[0].headline, "Task candidate created");

        let memory_filter = ContextTimelineFilter::only_memory();
        let memory_report = ContextTimelineInspector::inspect_context(&conn, &ws.id, Some(&memory_filter)).unwrap();
        assert_eq!(memory_report.total_events, 1);
        assert_eq!(memory_report.events[0].headline, "Important item identified");

        let feedback_filter = ContextTimelineFilter::only_feedback();
        let feedback_report = ContextTimelineInspector::inspect_context(&conn, &ws.id, Some(&feedback_filter)).unwrap();
        assert_eq!(feedback_report.total_events, 1);
        assert_eq!(feedback_report.events[0].headline, "User rejected unrelated item");

        let snapshot_filter = ContextTimelineFilter::only_snapshot();
        let snapshot_report = ContextTimelineInspector::inspect_context(&conn, &ws.id, Some(&snapshot_filter)).unwrap();
        assert_eq!(snapshot_report.total_events, 1);
        assert_eq!(snapshot_report.events[0].headline, "Context snapshot updated");
    }
}


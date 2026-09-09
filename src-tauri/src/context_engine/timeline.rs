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
}

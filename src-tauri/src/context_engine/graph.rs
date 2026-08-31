use rusqlite::{Connection, Result, params};
use serde::{Serialize, Deserialize};
use serde_json::Value;

use crate::db::models::{Item, ItemRelationship};

#[derive(Debug, Serialize, Deserialize)]
pub struct ContextualEvidence {
    pub related_items: Vec<Item>,
    pub shared_sessions: Vec<String>,
    pub shared_domains: Vec<String>,
    pub shared_apps: Vec<String>,
}

/// Retrieves items that have a relationship with the given item ID exceeding `min_strength`.
pub fn get_related_items(conn: &Connection, item_id: &str, min_strength: f64) -> Result<Vec<Item>> {
    let sql = r#"
        SELECT i.id, i.workspace_id, i.session_id, i.item_type, i.content, i.title, i.metadata, i.created_at, i.updated_at
        FROM items i
        JOIN item_relationships r ON (i.id = r.item_a_id OR i.id = r.item_b_id)
        WHERE (r.item_a_id = ?1 OR r.item_b_id = ?1)
          AND i.id != ?1
          AND r.strength >= ?2
        ORDER BY r.strength DESC
    "#;
    
    let mut stmt = conn.prepare(sql)?;
    let items_iter = stmt.query_map(params![item_id, min_strength], |row| {
        Ok(Item {
            id: row.get(0)?,
            workspace_id: row.get(1)?,
            session_id: row.get(2)?,
            item_type: row.get(3)?,
            content: row.get(4)?,
            title: row.get(5)?,
            metadata: row.get(6)?,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
        })
    })?;
    
    let mut items = Vec::new();
    for item in items_iter {
        items.push(item?);
    }
    
    Ok(items)
}

/// Retrieves the raw relationship edges for a given item, ordered by strength descending.
pub fn get_strongest_relationships(conn: &Connection, item_id: &str, limit: usize) -> Result<Vec<ItemRelationship>> {
    let sql = r#"
        SELECT id, item_a_id, item_b_id, relationship_type, strength, created_at, metadata
        FROM item_relationships
        WHERE item_a_id = ?1 OR item_b_id = ?1
        ORDER BY strength DESC
        LIMIT ?2
    "#;
    
    let mut stmt = conn.prepare(sql)?;
    let rels_iter = stmt.query_map(params![item_id, limit], |row| {
        Ok(ItemRelationship {
            id: row.get(0)?,
            item_a_id: row.get(1)?,
            item_b_id: row.get(2)?,
            relationship_type: row.get(3)?,
            strength: row.get(4)?,
            created_at: row.get(5)?,
            metadata: row.get(6)?,
        })
    })?;
    
    let mut rels = Vec::new();
    for rel in rels_iter {
        rels.push(rel?);
    }
    
    Ok(rels)
}

/// Retrieves all items belonging to a specific session.
pub fn get_items_from_session(conn: &Connection, session_id: &str) -> Result<Vec<Item>> {
    crate::db::operations::list_items(conn, None).map(|items| {
        items.into_iter().filter(|i| i.session_id.as_deref() == Some(session_id)).collect()
    })
}

/// Retrieves items that are strongly related to ANY item within the specified workspace.
/// This excludes items that are already in the workspace.
pub fn get_items_related_to_workspace(conn: &Connection, workspace_id: &str, min_strength: f64) -> Result<Vec<Item>> {
    let sql = r#"
        SELECT DISTINCT i.id, i.workspace_id, i.session_id, i.item_type, i.content, i.title, i.metadata, i.created_at, i.updated_at
        FROM items i
        JOIN item_relationships r ON (i.id = r.item_a_id OR i.id = r.item_b_id)
        JOIN items ws_i ON (ws_i.id = r.item_a_id OR ws_i.id = r.item_b_id)
        WHERE ws_i.workspace_id = ?1
          AND ws_i.id != i.id
          AND (i.workspace_id IS NULL OR i.workspace_id != ?1)
          AND r.strength >= ?2
    "#;
    
    let mut stmt = conn.prepare(sql)?;
    let items_iter = stmt.query_map(params![workspace_id, min_strength], |row| {
        Ok(Item {
            id: row.get(0)?,
            workspace_id: row.get(1)?,
            session_id: row.get(2)?,
            item_type: row.get(3)?,
            content: row.get(4)?,
            title: row.get(5)?,
            metadata: row.get(6)?,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
        })
    })?;
    
    let mut items = Vec::new();
    for item in items_iter {
        items.push(item?);
    }
    
    Ok(items)
}

/// Compiles contextual evidence bridging a specific item and a target workspace.
/// This gathers shared sessions, domains, apps, and explicit item relationships.
pub fn get_contextual_evidence(conn: &Connection, item_id: &str, workspace_id: &str) -> Result<ContextualEvidence> {
    // 1. Get explicit related items in that workspace
    let mut related_items_in_ws = Vec::new();
    if let Ok(rels) = get_related_items(conn, item_id, 0.3) {
        for item in rels {
            if item.workspace_id.as_deref() == Some(workspace_id) {
                related_items_in_ws.push(item);
            }
        }
    }
    
    // 2. Fetch Target Item signals
    let target_signals = crate::context_engine::operations::list_context_signals(conn, None, Some(item_id)).unwrap_or_default();
    
    // 3. Fetch Workspace Items signals
    let ws_items = crate::db::operations::list_items(conn, Some(workspace_id))?;
    let ws_item_ids: Vec<String> = ws_items.iter().map(|i| i.id.clone()).collect();
    
    let mut shared_sessions = Vec::new();
    let mut shared_domains = Vec::new();
    let mut shared_apps = Vec::new();
    
    if !ws_item_ids.is_empty() {
        let placeholders: String = vec!["?"; ws_item_ids.len()].join(",");
        let sql = format!("SELECT signal_type, session_id, payload FROM context_signals WHERE item_id IN ({})", placeholders);
        
        let mut stmt = conn.prepare(&sql)?;
        let params = rusqlite::params_from_iter(ws_item_ids.iter());
        
        let mut ws_sessions = Vec::new();
        let mut ws_domains = Vec::new();
        let mut ws_apps = Vec::new();
        
        if let Ok(iter) = stmt.query_map(params, |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
            ))
        }) {
            for row_res in iter {
                if let Ok((sig_type, session_id, payload_str)) = row_res {
                    if let Some(s_id) = session_id {
                        ws_sessions.push(s_id);
                    }
                    if sig_type == "active_app" {
                        if let Ok(payload) = serde_json::from_str::<Value>(&payload_str) {
                            if let Some(app) = payload.get("app_name").and_then(|v| v.as_str()) {
                                ws_apps.push(app.to_string());
                            }
                        }
                    } else if sig_type == "url_metadata" {
                        if let Ok(payload) = serde_json::from_str::<Value>(&payload_str) {
                            if let Some(domain) = payload.get("domain").and_then(|v| v.as_str()) {
                                ws_domains.push(domain.to_string());
                            }
                        }
                    }
                }
            }
        }
        
        // Find overlaps
        for t_sig in &target_signals {
            if let Some(s_id) = &t_sig.session_id {
                if ws_sessions.contains(s_id) && !shared_sessions.contains(s_id) {
                    shared_sessions.push(s_id.clone());
                }
            }
            
            if t_sig.signal_type == "active_app" {
                if let Ok(payload) = serde_json::from_str::<Value>(&t_sig.payload) {
                    if let Some(app) = payload.get("app_name").and_then(|v| v.as_str()) {
                        if ws_apps.contains(&app.to_string()) && !shared_apps.contains(&app.to_string()) {
                            shared_apps.push(app.to_string());
                        }
                    }
                }
            } else if t_sig.signal_type == "url_metadata" {
                if let Ok(payload) = serde_json::from_str::<Value>(&t_sig.payload) {
                    if let Some(domain) = payload.get("domain").and_then(|v| v.as_str()) {
                        if ws_domains.contains(&domain.to_string()) && !shared_domains.contains(&domain.to_string()) {
                            shared_domains.push(domain.to_string());
                        }
                    }
                }
            }
        }
    }
    
    Ok(ContextualEvidence {
        related_items: related_items_in_ws,
        shared_sessions,
        shared_domains,
        shared_apps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use crate::db::operations::{create_workspace, create_item, create_item_relationship};
    use crate::context_engine::operations::create_context_signal;
    
    fn setup_in_memory_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }
    
    #[test]
    fn test_graph_get_related_items() {
        let conn = setup_in_memory_db();
        
        let target = create_item(&conn, None, None, "text", "t", None, "{}").unwrap();
        let r1 = create_item(&conn, None, None, "text", "r1", None, "{}").unwrap();
        let r2 = create_item(&conn, None, None, "text", "r2", None, "{}").unwrap();
        
        create_item_relationship(&conn, &target.id, &r1.id, "related", 0.8, None).unwrap();
        create_item_relationship(&conn, &target.id, &r2.id, "related", 0.4, None).unwrap();
        
        // Threshold 0.5 should only return r1
        let related = get_related_items(&conn, &target.id, 0.5).unwrap();
        assert_eq!(related.len(), 1);
        assert_eq!(related[0].id, r1.id);
        
        // Threshold 0.3 should return both
        let related = get_related_items(&conn, &target.id, 0.3).unwrap();
        assert_eq!(related.len(), 2);
    }
    
    #[test]
    fn test_graph_get_items_related_to_workspace() {
        let conn = setup_in_memory_db();
        
        let ws = create_workspace(&conn, "GraphWS").unwrap();
        let ws2 = create_workspace(&conn, "OtherWS").unwrap();
        
        let ws_item = create_item(&conn, Some(&ws.id), None, "text", "ws", None, "{}").unwrap();
        
        // Inbox item strongly related to ws_item
        let inbox_item = create_item(&conn, None, None, "text", "inbox", None, "{}").unwrap();
        create_item_relationship(&conn, &inbox_item.id, &ws_item.id, "related", 0.9, None).unwrap();
        
        // OtherWS item strongly related to ws_item
        let other_ws_item = create_item(&conn, Some(&ws2.id), None, "text", "other", None, "{}").unwrap();
        create_item_relationship(&conn, &other_ws_item.id, &ws_item.id, "related", 0.8, None).unwrap();
        
        let related = get_items_related_to_workspace(&conn, &ws.id, 0.5).unwrap();
        
        // Should find both the inbox item and the other_ws item
        assert_eq!(related.len(), 2);
        assert!(related.iter().any(|i| i.id == inbox_item.id));
        assert!(related.iter().any(|i| i.id == other_ws_item.id));
    }
    
    #[test]
    fn test_graph_get_contextual_evidence() {
        let conn = setup_in_memory_db();
        let ws = create_workspace(&conn, "WS").unwrap();
        
        let target = create_item(&conn, None, None, "url", "t", None, "{}").unwrap();
        let ws_item = create_item(&conn, Some(&ws.id), None, "url", "w", None, "{}").unwrap();
        
        let payload = serde_json::json!({ "domain": "example.com" }).to_string();
        
        create_context_signal(&conn, None, Some(&target.id), "url_metadata", None, &payload).unwrap();
        create_context_signal(&conn, Some(&ws.id), Some(&ws_item.id), "url_metadata", None, &payload).unwrap();
        
        let evidence = get_contextual_evidence(&conn, &target.id, &ws.id).unwrap();
        assert_eq!(evidence.shared_domains.len(), 1);
        assert_eq!(evidence.shared_domains[0], "example.com");
    }
}

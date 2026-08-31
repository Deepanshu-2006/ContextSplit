use rusqlite::Connection;
use chrono::{DateTime, Utc};
use serde_json::Value;

pub fn generate_semantic_relationships(ce: &crate::context_engine::engine::ContextEngine, conn: &Connection, new_item_id: &str) -> Result<(), String> {
    let min_threshold = 0.85;
    let time_window_secs = 86400; // 24 hours
    
    let now = Utc::now();
    let window_start = now - chrono::Duration::seconds(time_window_secs);
    let window_start_str = window_start.to_rfc3339();
    
    // Fetch recent items (excluding self)
    let mut stmt = conn.prepare(
        "SELECT id FROM items WHERE created_at >= ?1 AND id != ?2"
    ).map_err(|e| e.to_string())?;
    
    let iter = stmt.query_map(rusqlite::params![window_start_str, new_item_id], |row| {
        row.get::<_, String>(0)
    }).map_err(|e| e.to_string())?;
    
    for row in iter {
        if let Ok(recent_id) = row {
            if let Some(score) = ce.get_semantic_similarity(conn, new_item_id, &recent_id) {
                if score >= min_threshold {
                    let primary_reason = "semantic_similarity".to_string();
                    let meta = serde_json::json!({
                        "reasons": ["semantic_similarity"]
                    }).to_string();
                    
                    // Create relationship, ignoring error if it already exists
                    let _ = crate::db::operations::create_item_relationship(
                        conn,
                        new_item_id,
                        &recent_id,
                        &primary_reason,
                        score as f64,
                        Some(&meta),
                    );
                }
            }
        }
    }
    
    Ok(())
}


pub fn generate_relationships(conn: &Connection, new_item_id: &str) -> Result<(), String> {
    let min_threshold = 0.3;
    let time_window_secs = 86400; // 24 hours
    
    let now = Utc::now();
    let window_start = now - chrono::Duration::seconds(time_window_secs);
    let window_start_str = window_start.to_rfc3339();
    
    // Fetch new item
    let new_item = match crate::db::operations::get_item(conn, new_item_id) {
        Ok(item) => item,
        Err(_) => return Ok(()), // If item doesn't exist, ignore
    };
    
    // Fetch signals for new item
    let new_signals = crate::context_engine::operations::list_context_signals(conn, None, Some(new_item_id))
        .unwrap_or_default();
    
    let mut new_app = None;
    let mut new_domain = None;
    
    for sig in &new_signals {
        if sig.signal_type == "active_app" {
            if let Ok(payload) = serde_json::from_str::<Value>(&sig.payload) {
                if let Some(app) = payload.get("app_name").and_then(|v| v.as_str()) {
                    new_app = Some(app.to_string());
                }
            }
        } else if sig.signal_type == "url_metadata" {
            if let Ok(payload) = serde_json::from_str::<Value>(&sig.payload) {
                if let Some(domain) = payload.get("domain").and_then(|v| v.as_str()) {
                    new_domain = Some(domain.to_string());
                }
            }
        }
    }
    
    // Fetch recent items
    let mut stmt = conn.prepare(
        "SELECT id, workspace_id, session_id, created_at FROM items 
         WHERE created_at >= ?1 AND id != ?2"
    ).map_err(|e| e.to_string())?;
    
    let iter = stmt.query_map(rusqlite::params![window_start_str, new_item_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
        ))
    }).map_err(|e| e.to_string())?;
    
    for row in iter {
        if let Ok((recent_id, recent_ws_id, recent_session_id, recent_created_at)) = row {
            let mut score: f64 = 0.0;
            let mut reasons = Vec::new();
            
            // 1. Same Session (+0.4)
            if let (Some(n_session), Some(r_session)) = (&new_item.session_id, &recent_session_id) {
                if n_session == r_session {
                    score += 0.4;
                    reasons.push("same_session");
                }
            }
            
            // 2. Same Workspace (+0.2)
            if let (Some(n_ws), Some(r_ws)) = (&new_item.workspace_id, &recent_ws_id) {
                if n_ws == r_ws {
                    score += 0.2;
                    reasons.push("same_workspace");
                }
            }
            
            // 3. Time Proximity (+0.2)
            // Parse created_at
            if let (Ok(n_time), Ok(r_time)) = (
                new_item.created_at.parse::<DateTime<Utc>>(),
                recent_created_at.parse::<DateTime<Utc>>()
            ) {
                let diff = (n_time - r_time).num_seconds().abs();
                if diff <= 300 { // 5 minutes
                    score += 0.2;
                    reasons.push("temporal_proximity");
                }
            }
            
            // Check signals for recent item if needed
            if new_app.is_some() || new_domain.is_some() {
                let recent_signals = crate::context_engine::operations::list_context_signals(conn, None, Some(&recent_id))
                    .unwrap_or_default();
                    
                for sig in &recent_signals {
                    if sig.signal_type == "active_app" {
                        if let Some(n_app) = &new_app {
                            if let Ok(payload) = serde_json::from_str::<Value>(&sig.payload) {
                                if let Some(r_app) = payload.get("app_name").and_then(|v| v.as_str()) {
                                    if n_app == r_app {
                                        score += 0.3;
                                        reasons.push("same_source_app");
                                    }
                                }
                            }
                        }
                    } else if sig.signal_type == "url_metadata" {
                        if let Some(n_domain) = &new_domain {
                            if let Ok(payload) = serde_json::from_str::<Value>(&sig.payload) {
                                if let Some(r_domain) = payload.get("domain").and_then(|v| v.as_str()) {
                                    if n_domain == r_domain {
                                        score += 0.3;
                                        reasons.push("same_domain");
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            score = score.min(1.0);
            
            if score >= min_threshold {
                let primary_reason = if reasons.is_empty() { "related".to_string() } else { reasons[0].to_string() };
                let meta = serde_json::json!({
                    "reasons": reasons
                }).to_string();
                
                let _ = crate::db::operations::create_item_relationship(
                    conn,
                    new_item_id,
                    &recent_id,
                    &primary_reason,
                    score as f64,
                    Some(&meta),
                );
            }
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use crate::db::operations::{create_item, create_session, get_item_relationships};
    use crate::context_engine::operations::create_context_signal;

    fn setup_in_memory_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_relationships_same_session() {
        let conn = setup_in_memory_db();
        let session = create_session(&conn).unwrap();
        
        let _i1 = create_item(&conn, None, Some(&session.id), "text", "Content 1", None, "{}").unwrap();
        
        // Sleep a bit to ensure created_at is slightly different, though tests run fast enough
        std::thread::sleep(std::time::Duration::from_millis(10));
        
        let i2 = create_item(&conn, None, Some(&session.id), "text", "Content 2", None, "{}").unwrap();
        
        generate_relationships(&conn, &i2.id).unwrap();
        
        let rels = get_item_relationships(&conn, &i2.id).unwrap();
        assert_eq!(rels.len(), 1);
        
        let score = rels[0].strength;
        // Same Session (0.4) + Time Proximity (0.2) = 0.6
        assert!((score - 0.6).abs() < f64::EPSILON);
        assert_eq!(rels[0].relationship_type, "same_session");
    }

    #[test]
    fn test_relationships_same_domain() {
        let conn = setup_in_memory_db();
        
        let i1 = create_item(&conn, None, None, "url", "https://example.com/1", None, "{}").unwrap();
        let p1 = serde_json::json!({"domain": "example.com"}).to_string();
        create_context_signal(&conn, None, Some(&i1.id), "url_metadata", None, &p1).unwrap();
        
        let i2 = create_item(&conn, None, None, "url", "https://example.com/2", None, "{}").unwrap();
        let p2 = serde_json::json!({"domain": "example.com"}).to_string();
        create_context_signal(&conn, None, Some(&i2.id), "url_metadata", None, &p2).unwrap();
        
        generate_relationships(&conn, &i2.id).unwrap();
        
        let rels = get_item_relationships(&conn, &i2.id).unwrap();
        assert_eq!(rels.len(), 1);
        
        let score = rels[0].strength;
        // Same Domain (0.3) + Time Proximity (0.2) = 0.5
        assert!((score - 0.5).abs() < f64::EPSILON);
    }
    
    #[test]
    fn test_relationships_same_app() {
        let conn = setup_in_memory_db();
        
        let i1 = create_item(&conn, None, None, "text", "test", None, "{}").unwrap();
        let p1 = serde_json::json!({"app_name": "Google Chrome"}).to_string();
        create_context_signal(&conn, None, Some(&i1.id), "active_app", None, &p1).unwrap();
        
        let i2 = create_item(&conn, None, None, "text", "test2", None, "{}").unwrap();
        let p2 = serde_json::json!({"app_name": "Google Chrome"}).to_string();
        create_context_signal(&conn, None, Some(&i2.id), "active_app", None, &p2).unwrap();
        
        generate_relationships(&conn, &i2.id).unwrap();
        
        let rels = get_item_relationships(&conn, &i2.id).unwrap();
        assert_eq!(rels.len(), 1);
        
        let score = rels[0].strength;
        // Same App (0.3) + Time Proximity (0.2) = 0.5
        assert!((score - 0.5).abs() < f64::EPSILON);
    }
    
    #[test]
    fn test_relationships_unrelated_items() {
        let conn = setup_in_memory_db();
        
        // i1 created 10 minutes ago
        let i1 = create_item(&conn, None, None, "text", "test", None, "{}").unwrap();
        let past_time = (Utc::now() - chrono::Duration::minutes(10)).to_rfc3339();
        conn.execute("UPDATE items SET created_at = ?1 WHERE id = ?2", rusqlite::params![past_time, i1.id]).unwrap();
        
        let i2 = create_item(&conn, None, None, "text", "test2", None, "{}").unwrap();
        
        generate_relationships(&conn, &i2.id).unwrap();
        
        let rels = get_item_relationships(&conn, &i2.id).unwrap();
        // Since diff > 5 mins, no time proximity score. 
        // No session, no app, no domain. Score = 0.0, which is < min_threshold (0.3).
        assert_eq!(rels.len(), 0);
    }
    
    #[test]
    fn test_generate_semantic_relationships() {
        let conn = setup_in_memory_db();
        let ce = crate::context_engine::engine::ContextEngine::new();
        
        let i1 = create_item(&conn, None, None, "text", "Apple", None, "{}").unwrap();
        let i2 = create_item(&conn, None, None, "text", "Banana", None, "{}").unwrap();
        let i3 = create_item(&conn, None, None, "text", "Cherry", None, "{}").unwrap();
        
        // Save identical embeddings for i1 and i2 to force a 1.0 similarity score
        let emb_a = vec![1.0, 0.0];
        let emb_b = vec![1.0, 0.0];
        crate::db::operations::save_item_embedding(&conn, &i1.id, &emb_a, "test", "test").unwrap();
        crate::db::operations::save_item_embedding(&conn, &i2.id, &emb_b, "test", "test").unwrap();
        
        // No embedding for i3
        
        // Generate semantic relationships for i2
        generate_semantic_relationships(&ce, &conn, &i2.id).unwrap();
        
        // Verify relationship created between i1 and i2
        let rels = get_item_relationships(&conn, &i2.id).unwrap();
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].relationship_type, "semantic_similarity");
        assert_eq!(rels[0].strength, 1.0);
        
        // Generate for i3, should do nothing
        generate_semantic_relationships(&ce, &conn, &i3.id).unwrap();
        let rels3 = get_item_relationships(&conn, &i3.id).unwrap();
        assert_eq!(rels3.len(), 0);
    }
}

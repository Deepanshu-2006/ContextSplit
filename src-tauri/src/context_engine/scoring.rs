use rusqlite::Connection;
use serde::{Serialize, Deserialize};
use serde_json::Value;
use std::cmp::Ordering;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceScore {
    pub workspace_id: String,
    pub score: f32,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ScoringWeights {
    pub workspace_history: f32,
    pub same_source_app_per_match: f32,
    pub same_source_app_max: f32,
    pub same_domain_per_match: f32,
    pub same_domain_max: f32,
    pub same_session_per_match: f32,
    pub same_session_max: f32,
    pub same_content_type_per_match: f32,
    pub same_content_type_max: f32,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            workspace_history: 0.4,
            same_source_app_per_match: 0.05,
            same_source_app_max: 0.2,
            same_domain_per_match: 0.1,
            same_domain_max: 0.3,
            same_session_per_match: 0.1,
            same_session_max: 0.3,
            same_content_type_per_match: 0.02,
            same_content_type_max: 0.1,
        }
    }
}

pub fn score_item(conn: &Connection, item_id: &str) -> Result<Vec<WorkspaceScore>, String> {
    let weights = ScoringWeights::default();
    
    // Fetch Target Item
    let target_item = crate::db::operations::get_item(conn, item_id).map_err(|e| e.to_string())?;
    
    // Fetch Target Signals
    let target_signals = crate::context_engine::operations::list_context_signals(conn, None, Some(item_id)).unwrap_or_default();
    
    // Fetch All Workspaces
    let workspaces = crate::db::operations::list_workspaces(conn).map_err(|e| e.to_string())?;
    if workspaces.is_empty() {
        return Ok(vec![]);
    }
    
    let mut scores = Vec::new();
    
    for ws in workspaces {
        let mut score = 0.0;
        let mut reasons = Vec::new();
        
        // --- 1. Workspace History ---
        let mut has_history = false;
        for sig in &target_signals {
            if sig.signal_type == "user_action" {
                if let Ok(payload) = serde_json::from_str::<Value>(&sig.payload) {
                    if let Some(action) = payload.get("action").and_then(|v| v.as_str()) {
                        if action == "moved_to_workspace" {
                            if let Some(dest) = payload.get("destination_workspace").and_then(|v| v.as_str()) {
                                if dest == ws.id {
                                    has_history = true;
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
        
        if has_history {
            score += weights.workspace_history;
            reasons.push("Previously organized into this workspace".to_string());
        }
        
        // --- Prepare for signal comparisons ---
        let ws_items = crate::db::operations::list_items(conn, Some(&ws.id)).unwrap_or_default();
        let ws_item_ids: Vec<String> = ws_items.iter().map(|i| i.id.clone()).collect();
        
        if !ws_item_ids.is_empty() {
            // Check Content Type
            let mut content_type_matches = 0;
            for i in &ws_items {
                if i.item_type == target_item.item_type {
                    content_type_matches += 1;
                }
            }
            if content_type_matches > 0 {
                let s = (content_type_matches as f32 * weights.same_content_type_per_match).min(weights.same_content_type_max);
                score += s;
                reasons.push(format!("Shares content type with {} items", content_type_matches));
            }
            
            // Check Context Signals for the workspace's items
            let placeholders: String = vec!["?"; ws_item_ids.len()].join(",");
            let sql = format!("SELECT signal_type, session_id, payload FROM context_signals WHERE item_id IN ({})", placeholders);
            
            let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
            let params = rusqlite::params_from_iter(ws_item_ids.iter());
            
            let mut ws_app_names = Vec::new();
            let mut ws_domains = Vec::new();
            let mut ws_sessions = Vec::new();
            
            let iter = stmt.query_map(params, |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                ))
            });
            
            if let Ok(iter) = iter {
                for row_res in iter {
                    if let Ok((sig_type, session_id, payload_str)) = row_res {
                        if let Some(s_id) = session_id {
                            ws_sessions.push(s_id);
                        }
                        if sig_type == "active_app" {
                            if let Ok(payload) = serde_json::from_str::<Value>(&payload_str) {
                                if let Some(app_name) = payload.get("app_name").and_then(|v| v.as_str()) {
                                    ws_app_names.push(app_name.to_string());
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
            
            // Compare Target App
            for t_sig in &target_signals {
                if t_sig.signal_type == "active_app" {
                    if let Ok(payload) = serde_json::from_str::<Value>(&t_sig.payload) {
                        if let Some(t_app) = payload.get("app_name").and_then(|v| v.as_str()) {
                            let match_count = ws_app_names.iter().filter(|&a| a == t_app).count();
                            if match_count > 0 {
                                let s = (match_count as f32 * weights.same_source_app_per_match).min(weights.same_source_app_max);
                                score += s;
                                reasons.push(format!("Same source app as {} items", match_count));
                            }
                        }
                    }
                } else if t_sig.signal_type == "url_metadata" {
                    if let Ok(payload) = serde_json::from_str::<Value>(&t_sig.payload) {
                        if let Some(t_domain) = payload.get("domain").and_then(|v| v.as_str()) {
                            let match_count = ws_domains.iter().filter(|&d| d == t_domain).count();
                            if match_count > 0 {
                                let s = (match_count as f32 * weights.same_domain_per_match).min(weights.same_domain_max);
                                score += s;
                                reasons.push(format!("Same URL domain as {} items", match_count));
                            }
                        }
                    }
                }
            }
            
            // Check session ID match
            let mut session_match_count = 0;
            for t_sig in &target_signals {
                if let Some(t_session) = &t_sig.session_id {
                    session_match_count += ws_sessions.iter().filter(|&s| s == t_session).count();
                }
            }
            
            if session_match_count > 0 {
                let s = (session_match_count as f32 * weights.same_session_per_match).min(weights.same_session_max);
                score += s;
                reasons.push(format!("Captured in the same session as {} items", session_match_count));
            }
        }
        
        // Cap at 1.0
        score = score.min(1.0);
        
        // Add minimal threshold to not spam
        if score > 0.0 {
            scores.push(WorkspaceScore {
                workspace_id: ws.id.clone(),
                score,
                reasons,
            });
        }
    }
    
    // Sort descending
    scores.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
    
    Ok(scores)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use crate::db::operations::{create_workspace, create_item};
    use crate::context_engine::operations::create_context_signal;
    
    fn setup_in_memory_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_scoring_history_and_content_type() {
        let conn = setup_in_memory_db();
        
        let ws1 = create_workspace(&conn, "Work").unwrap();
        let ws2 = create_workspace(&conn, "Personal").unwrap();
        
        // Item in WS1 (Text)
        let _i1 = create_item(&conn, Some(&ws1.id), "text", "test content 1", None, "{}").unwrap();
        let _i2 = create_item(&conn, Some(&ws1.id), "text", "test content 2", None, "{}").unwrap();
        
        // Item in Inbox (Text)
        let target = create_item(&conn, None, "text", "target text", None, "{}").unwrap();
        
        // Mock a move signal indicating target WAS previously in WS2
        let payload = serde_json::json!({
            "action": "moved_to_workspace",
            "destination_workspace": ws2.id
        }).to_string();
        
        let _ = create_context_signal(
            &conn,
            None,
            Some(&target.id),
            "user_action",
            None,
            &payload
        ).unwrap();
        
        let scores = score_item(&conn, &target.id).unwrap();
        assert_eq!(scores.len(), 2);
        
        // WS2 should win due to history (0.4)
        // WS1 should get (2 * 0.02) = 0.04 for content type
        
        let w2_score = scores.iter().find(|s| s.workspace_id == ws2.id).unwrap();
        let w1_score = scores.iter().find(|s| s.workspace_id == ws1.id).unwrap();
        
        assert!(w2_score.score >= 0.4);
        assert_eq!(w1_score.score, 0.04);
        assert_eq!(w1_score.reasons[0], "Shares content type with 2 items");
    }
    
    #[test]
    fn test_scoring_app_and_domain() {
        let conn = setup_in_memory_db();
        
        let ws = create_workspace(&conn, "Research").unwrap();
        
        // Item in WS
        let i1 = create_item(&conn, Some(&ws.id), "url", "https://example.com", None, "{}").unwrap();
        
        // Signal for i1
        let payload1 = serde_json::json!({
            "domain": "example.com"
        }).to_string();
        let _ = create_context_signal(&conn, Some(&ws.id), Some(&i1.id), "url_metadata", None, &payload1).unwrap();
        
        // Target Item
        let target = create_item(&conn, None, "url", "https://example.com/2", None, "{}").unwrap();
        
        let payload2 = serde_json::json!({
            "domain": "example.com"
        }).to_string();
        let _ = create_context_signal(&conn, None, Some(&target.id), "url_metadata", None, &payload2).unwrap();
        
        let scores = score_item(&conn, &target.id).unwrap();
        assert_eq!(scores.len(), 1);
        
        let score = &scores[0];
        assert_eq!(score.workspace_id, ws.id);
        
        // content_type_matches = 1 (0.02)
        // domain_match = 1 (0.1)
        // Total = 0.12
        assert!((score.score - 0.12).abs() < f32::EPSILON);
        assert!(score.reasons.iter().any(|r| r == "Same URL domain as 1 items"));
    }
}

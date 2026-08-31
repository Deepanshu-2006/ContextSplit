use rusqlite::Connection;
use serde::{Serialize, Deserialize};
use serde_json::Value;
use std::cmp::Ordering;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceScore {
    pub workspace_id: String,
    pub total_score: f32,
    pub confidence: f32,
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
    pub related_item_per_match: f32,
    pub related_item_max: f32,
    pub semantic_similarity_max: f32,
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
            related_item_per_match: 0.2,
            related_item_max: 0.5,
            semantic_similarity_max: 0.4,
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
    
    // --- Pre-compute Relationships ---
    let relationships = crate::db::operations::get_item_relationships(conn, item_id).unwrap_or_default();
    let mut ws_relationship_scores: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
    let mut ws_relationship_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut ws_semantic_max: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
    
    for rel in &relationships {
        let other_id = if rel.item_a_id == item_id { &rel.item_b_id } else { &rel.item_a_id };
        if let Ok(other_item) = crate::db::operations::get_item(conn, other_id) {
            if let Some(ws_id) = other_item.workspace_id {
                if rel.relationship_type == "semantic_similarity" {
                    let current_max = ws_semantic_max.entry(ws_id.clone()).or_insert(0.0);
                    if rel.strength as f32 > *current_max {
                        *current_max = rel.strength as f32;
                    }
                } else {
                    *ws_relationship_scores.entry(ws_id.clone()).or_insert(0.0) += rel.strength as f32;
                    *ws_relationship_counts.entry(ws_id).or_insert(0) += 1;
                }
            }
        }
    }
    
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
            reasons.push(format!("User previously organized similar Items into {}", ws.name));
        }
        
        // --- 2. Item Relationships ---
        if let Some(rel_score) = ws_relationship_scores.get(&ws.id) {
            if let Some(count) = ws_relationship_counts.get(&ws.id) {
                let s = (*rel_score * weights.related_item_per_match).min(weights.related_item_max);
                score += s;
                reasons.push(format!("{} related Items already belong to {}", count, ws.name));
            }
        }
        
        // --- 3. Semantic Similarity ---
        if let Some(sem_max) = ws_semantic_max.get(&ws.id) {
            let s = (*sem_max).min(weights.semantic_similarity_max);
            score += s;
            reasons.push(format!("Semantic similarity with {} Items: {:.2}", ws.name, sem_max));
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
                // Avoid spamming reasons for content type if it's very minor, but include for accuracy
                reasons.push(format!("Shares content type with {} items in {}", content_type_matches, ws.name));
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
                                reasons.push(format!("Same {} source app", t_app));
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
                                let display_domain = t_domain.strip_prefix("www.").unwrap_or(t_domain);
                                let domain_name = display_domain.split('.').next().unwrap_or(t_domain);
                                // Capitalize the first letter for a nice reason (e.g. figma.com -> Figma)
                                let mut chars = domain_name.chars();
                                let mut pretty_domain = match chars.next() {
                                    None => String::new(),
                                    Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
                                };
                                if pretty_domain.is_empty() {
                                    pretty_domain = t_domain.to_string();
                                }
                                reasons.push(format!("Same {} domain", pretty_domain));
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
                reasons.push("Same Context Session".to_string());
            }
        }
        
        // Cap at 1.0
        score = score.min(1.0);
        
        // Add minimal threshold to not spam
        if score > 0.0 {
            scores.push(WorkspaceScore {
                workspace_id: ws.id.clone(),
                total_score: score,
                confidence: score, // UI handles percentage mapping
                reasons,
            });
        }
    }
    
    // Sort descending
    scores.sort_by(|a, b| b.total_score.partial_cmp(&a.total_score).unwrap_or(Ordering::Equal));
    
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
        let _i1 = create_item(&conn, Some(&ws1.id), None, "text", "test content 1", None, "{}").unwrap();
        let _i2 = create_item(&conn, Some(&ws1.id), None, "text", "test content 2", None, "{}").unwrap();
        
        // Item in Inbox (Text)
        let target = create_item(&conn, None, None, "text", "target text", None, "{}").unwrap();
        
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
        
        assert!(w2_score.total_score >= 0.4);
        assert_eq!(w1_score.total_score, 0.04);
        assert_eq!(w1_score.reasons[0], "Shares content type with 2 items in Work");
    }
    
    #[test]
    fn test_scoring_app_and_domain() {
        let conn = setup_in_memory_db();
        
        let ws = create_workspace(&conn, "Research").unwrap();
        
        // Item in WS
        let i1 = create_item(&conn, Some(&ws.id), None, "url", "https://example.com", None, "{}").unwrap();
        
        // Signal for i1
        let payload1 = serde_json::json!({
            "domain": "example.com"
        }).to_string();
        let _ = create_context_signal(&conn, Some(&ws.id), Some(&i1.id), "url_metadata", None, &payload1).unwrap();
        
        // Target Item
        let target = create_item(&conn, None, None, "url", "https://example.com/2", None, "{}").unwrap();
        
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
        assert!((score.total_score - 0.12).abs() < f32::EPSILON);
        assert!(score.reasons.iter().any(|r| r.contains("Same Example domain")));
    }
    
    #[test]
    fn test_scoring_with_relationships() {
        let conn = setup_in_memory_db();
        
        let ws = create_workspace(&conn, "Portfolio").unwrap();
        
        let target = create_item(&conn, None, None, "text", "target", None, "{}").unwrap();
        let related_item_1 = create_item(&conn, Some(&ws.id), None, "text", "r1", None, "{}").unwrap();
        let related_item_2 = create_item(&conn, Some(&ws.id), None, "text", "r2", None, "{}").unwrap();
        
        // Target -> related 1 (strength 0.6)
        let _ = crate::db::operations::create_item_relationship(
            &conn,
            &target.id,
            &related_item_1.id,
            "temporal_proximity",
            0.6,
            None
        ).unwrap();
        
        // Target -> related 2 (strength 0.4)
        let _ = crate::db::operations::create_item_relationship(
            &conn,
            &target.id,
            &related_item_2.id,
            "same_session",
            0.4,
            None
        ).unwrap();
        
        let scores = score_item(&conn, &target.id).unwrap();
        assert_eq!(scores.len(), 1);
        
        let score = &scores[0];
        assert_eq!(score.workspace_id, ws.id);
        
        // Content type match for 2 items: 2 * 0.02 = 0.04
        // Relationships: (0.6 + 0.4) = 1.0 * related_item_per_match (0.2) = 0.2
        // Total = 0.24
        assert!((score.total_score - 0.24).abs() < f32::EPSILON);
        assert!(score.reasons.iter().any(|r| r == "2 related Items already belong to Portfolio"));
    }
    
    #[test]
    fn test_scoring_with_semantic_similarity() {
        let conn = setup_in_memory_db();
        
        let ws_semantic = create_workspace(&conn, "AI Workspace").unwrap();
        let ws_history = create_workspace(&conn, "Manual Workspace").unwrap();
        
        let target = create_item(&conn, None, None, "text", "target", None, "{}").unwrap();
        let related_sem = create_item(&conn, Some(&ws_semantic.id), None, "text", "r_sem", None, "{}").unwrap();
        
        // Semantic relationship with high score
        let _ = crate::db::operations::create_item_relationship(
            &conn,
            &target.id,
            &related_sem.id,
            "semantic_similarity",
            0.95,
            None
        ).unwrap();
        
        // User explicitly moved an item to ws_history
        let payload = serde_json::json!({
            "action": "moved_to_workspace",
            "destination_workspace": ws_history.id
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
        
        let score_history = scores.iter().find(|s| s.workspace_id == ws_history.id).unwrap();
        let score_semantic = scores.iter().find(|s| s.workspace_id == ws_semantic.id).unwrap();
        
        // History should give 0.4.
        // Semantic should be capped at 0.4 (from 0.95). 
        // Content type match for Semantic Workspace adds 0.02.
        // Both end up very close. Let's verify reasons.
        assert!(score_semantic.reasons.iter().any(|r| r == "Semantic similarity with AI Workspace Items: 0.95"));
        assert!(score_history.reasons.iter().any(|r| r == "User previously organized similar Items into Manual Workspace"));
    }
}

use rusqlite::Connection;
use serde::{Serialize, Deserialize};
use serde_json::Value;
use std::cmp::Ordering;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceScore {
    pub workspace_id: String,
    pub total_score: f32,
    pub confidence: f32,
    pub confidence_band: String,
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
    pub workspace_profile_similarity_max: f32,
    pub threshold_high: f32,
    pub threshold_medium: f32,
    pub threshold_low: f32,
    pub temporal_decay: crate::context_engine::temporal::TemporalDecayConfig,
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
            workspace_profile_similarity_max: 0.45,
            threshold_high: 0.6,
            threshold_medium: 0.4,
            threshold_low: 0.2,
            temporal_decay: crate::context_engine::temporal::TemporalDecayConfig::default(),
        }
    }
}

pub fn score_item(conn: &Connection, item_id: &str) -> Result<Vec<WorkspaceScore>, String> {
    let mut weights = ScoringWeights::default();
    weights.temporal_decay = crate::context_engine::temporal::get_temporal_decay_config(conn);
    let now = chrono::Utc::now();
    
    let enable_semantic = if let Ok(settings) = crate::db::settings::get_settings(conn) {
        settings.enable_semantic_suggestions
    } else { true };
    
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
    
    // Fetch Target Embedding
    let target_embedding = crate::db::operations::get_item_embedding(conn, item_id).unwrap_or(None);
    
    // --- Pre-compute Relationships ---
    let relationships = crate::db::operations::get_item_relationships(conn, item_id).unwrap_or_default();
    let mut ws_relationship_scores: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
    let mut ws_relationship_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut ws_semantic_max: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
    
    for rel in &relationships {
        let other_id = if rel.item_a_id == item_id { &rel.item_b_id } else { &rel.item_a_id };
        if let Ok(other_item) = crate::db::operations::get_item(conn, other_id) {
            if let Some(ws_id) = other_item.workspace_id {
                let t_weight = weights.temporal_decay.compute_weight(&other_item.created_at, now) as f32;
                if rel.relationship_type == "semantic_similarity" {
                    let current_max = ws_semantic_max.entry(ws_id.clone()).or_insert(0.0);
                    let decayed = (rel.strength as f32) * t_weight;
                    if decayed > *current_max {
                        *current_max = decayed;
                    }
                } else {
                    let decayed_strength = (rel.strength as f32) * t_weight;
                    *ws_relationship_scores.entry(ws_id.clone()).or_insert(0.0) += decayed_strength;
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
        let mut history_t_weight = 1.0_f32;
        for sig in &target_signals {
            if sig.signal_type == "user_action" {
                if let Ok(payload) = serde_json::from_str::<Value>(&sig.payload) {
                    if let Some(action) = payload.get("action").and_then(|v| v.as_str()) {
                        if action == "moved_to_workspace" {
                            if let Some(dest) = payload.get("destination_workspace").and_then(|v| v.as_str()) {
                                if dest == ws.id {
                                    has_history = true;
                                    history_t_weight = weights.temporal_decay.compute_weight(&sig.created_at, now) as f32;
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
        
        if has_history {
            score += weights.workspace_history * history_t_weight;
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
        
        // --- 3. Item Relationships Semantic Similarity ---
        if enable_semantic {
            if let Some(sem_max) = ws_semantic_max.get(&ws.id) {
                let s = (*sem_max).min(weights.semantic_similarity_max);
                score += s;
                reasons.push(format!("Semantic similarity with {} Items: {:.2}", ws.name, sem_max));
            }
        }
        
        // --- 4. Workspace Semantic Profile ---
        if enable_semantic {
            if let Some(t_emb) = &target_embedding {
                if let Ok(Some(ws_profile)) = crate::db::operations::get_workspace_embedding(conn, &ws.id) {
                    if t_emb.provider == ws_profile.provider && t_emb.model == ws_profile.model {
                        let sim = crate::semantic::cosine_similarity(&t_emb.embedding, &ws_profile.embedding);
                        if sim > 0.0 {
                            let s = sim.min(weights.workspace_profile_similarity_max);
                            score += s;
                            reasons.push(format!("Matches {} semantic profile: {:.2}", ws.name, sim));
                        }
                    }
                }
            }
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
        
        // --- Context Lifecycle State Influence ---
        if ws.lifecycle_state == "archived" {
            score = 0.0;
            reasons.push(format!("{} is archived by user; suggestions suppressed", ws.name));
        } else if ws.lifecycle_state == "dormant" {
            score *= 0.50;
            reasons.push(format!("{} is dormant (no recent meaningful activity)", ws.name));
        }

        // Cap at 1.0
        score = score.min(1.0);
        
        let confidence_band = if score >= weights.threshold_high {
            "High"
        } else if score >= weights.threshold_medium {
            "Medium"
        } else {
            "Low"
        };
        
        // Add minimal threshold to not spam
        if score >= weights.threshold_low {
            scores.push(WorkspaceScore {
                workspace_id: ws.id.clone(),
                total_score: score,
                confidence: score, // UI handles percentage mapping
                confidence_band: confidence_band.to_string(),
                reasons,
            });
        }
    }
    
    // Sort descending
    scores.sort_by(|a, b| b.total_score.partial_cmp(&a.total_score).unwrap_or(Ordering::Equal));
    
    Ok(scores)
}

/// Evaluates a newly captured or unassigned Item against both:
/// 1. All existing Workspaces
/// 2. Potential new ContextCandidates
///
/// Returns a unified contextual interpretation with explainable reasons.
/// Does not move or create anything automatically.
pub fn evaluate_item_context(
    conn: &Connection,
    item_id: &str,
) -> Result<crate::db::models::UnifiedContextInterpretation, String> {
    let mut weights = ScoringWeights::default();
    weights.temporal_decay = crate::context_engine::temporal::get_temporal_decay_config(conn);
    let now = chrono::Utc::now();
    
    let enable_semantic = if let Ok(settings) = crate::db::settings::get_settings(conn) {
        settings.enable_semantic_suggestions
    } else { true };

    // 1. Fetch Target Item & Signals
    let target_item = crate::db::operations::get_item(conn, item_id).map_err(|e| e.to_string())?;
    let target_signals = crate::context_engine::operations::list_context_signals(conn, None, Some(item_id)).unwrap_or_default();
    let workspaces = crate::db::operations::list_workspaces(conn).map_err(|e| e.to_string())?;
    let target_embedding = crate::db::operations::get_item_embedding(conn, item_id).unwrap_or(None);
    let relationships = crate::db::operations::get_item_relationships(conn, item_id).unwrap_or_default();

    // --- Pre-compute Workspace Relationship Aggregates ---
    let mut ws_relationship_scores: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
    let mut ws_relationship_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut ws_semantic_max: std::collections::HashMap<String, f32> = std::collections::HashMap::new();

    for rel in &relationships {
        let other_id = if rel.item_a_id == item_id { &rel.item_b_id } else { &rel.item_a_id };
        if let Ok(other_item) = crate::db::operations::get_item(conn, other_id) {
            if let Some(ws_id) = other_item.workspace_id {
                let t_weight = weights.temporal_decay.compute_weight(&other_item.created_at, now) as f32;
                if rel.relationship_type == "semantic_similarity" {
                    let current_max = ws_semantic_max.entry(ws_id.clone()).or_insert(0.0);
                    let decayed = (rel.strength as f32) * t_weight;
                    if decayed > *current_max {
                        *current_max = decayed;
                    }
                } else {
                    let decayed_strength = (rel.strength as f32) * t_weight;
                    *ws_relationship_scores.entry(ws_id.clone()).or_insert(0.0) += decayed_strength;
                    *ws_relationship_counts.entry(ws_id).or_insert(0) += 1;
                }
            }
        }
    }

    // --- Evaluate Against Every Existing Workspace ---
    let mut existing_workspaces = Vec::new();

    for ws in &workspaces {
        let mut score = 0.0_f32;
        let mut reasons = Vec::new();

        // 1. Workspace History
        for sig in &target_signals {
            if sig.signal_type == "user_action" {
                if let Ok(payload) = serde_json::from_str::<Value>(&sig.payload) {
                    if let Some(action) = payload.get("action").and_then(|v| v.as_str()) {
                        if action == "moved_to_workspace" {
                            if let Some(dest) = payload.get("destination_workspace").and_then(|v| v.as_str()) {
                                if dest == ws.id {
                                    let t_weight = weights.temporal_decay.compute_weight(&sig.created_at, now) as f32;
                                    score += weights.workspace_history * t_weight;
                                    if t_weight >= 0.90 {
                                        reasons.push(format!("Recent user actions associated similar items with {}", ws.name));
                                    } else {
                                        reasons.push(format!("Prior user actions associated similar items with {}", ws.name));
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        // 2. Item Relationships
        if let Some(rel_score) = ws_relationship_scores.get(&ws.id) {
            if let Some(count) = ws_relationship_counts.get(&ws.id) {
                let s = (*rel_score * weights.related_item_per_match).min(weights.related_item_max);
                score += s;
                reasons.push(format!("{} related items already belong to {}", count, ws.name));
            }
        }

        // 3. Semantic Similarity with Workspace Items
        if enable_semantic {
            if let Some(sem_max) = ws_semantic_max.get(&ws.id) {
                let s = (*sem_max).min(weights.semantic_similarity_max);
                score += s;
                reasons.push(format!("Semantic similarity with {} items ({:.0}%)", ws.name, sem_max * 100.0));
            }
        }

        // 4. Workspace Semantic Profile (Centroid)
        if enable_semantic {
            if let Some(t_emb) = &target_embedding {
                if let Ok(Some(ws_profile)) = crate::db::operations::get_workspace_embedding(conn, &ws.id) {
                    if t_emb.provider == ws_profile.provider && t_emb.model == ws_profile.model {
                        let sim = crate::semantic::cosine_similarity(&t_emb.embedding, &ws_profile.embedding);
                        if sim > 0.0 {
                            let s = sim.min(weights.workspace_profile_similarity_max);
                            score += s;
                            reasons.push(format!("Matches {} workspace semantic profile ({:.0}%)", ws.name, sim * 100.0));
                        }
                    }
                }
            }
        }

        // 5. App, Domain, Session Signals with Workspace Items
        let ws_items = crate::db::operations::list_items(conn, Some(&ws.id)).unwrap_or_default();
        let ws_item_ids: Vec<String> = ws_items.iter().map(|i| i.id.clone()).collect();

        if !ws_item_ids.is_empty() {
            // Content Type Match
            let content_type_matches = ws_items.iter().filter(|i| i.item_type == target_item.item_type).count();
            if content_type_matches > 0 {
                let s = (content_type_matches as f32 * weights.same_content_type_per_match).min(weights.same_content_type_max);
                score += s;
            }

            let placeholders: String = vec!["?"; ws_item_ids.len()].join(",");
            let sql = format!("SELECT signal_type, session_id, payload FROM context_signals WHERE item_id IN ({})", placeholders);

            if let Ok(mut stmt) = conn.prepare(&sql) {
                let params = rusqlite::params_from_iter(ws_item_ids.iter());
                let mut ws_app_names = Vec::new();
                let mut ws_domains = Vec::new();
                let mut ws_sessions = Vec::new();

                if let Ok(iter) = stmt.query_map(params, |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                }) {
                    for row_res in iter.flatten() {
                        let (sig_type, session_id, payload_str) = row_res;
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

                // Check active app match
                for t_sig in &target_signals {
                    if t_sig.signal_type == "active_app" {
                        if let Ok(payload) = serde_json::from_str::<Value>(&t_sig.payload) {
                            if let Some(t_app) = payload.get("app_name").and_then(|v| v.as_str()) {
                                let match_count = ws_app_names.iter().filter(|&a| a == t_app).count();
                                if match_count > 0 {
                                    let s = (match_count as f32 * weights.same_source_app_per_match).min(weights.same_source_app_max);
                                    score += s;
                                    reasons.push(format!("Common application '{}'", t_app));
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
                                    reasons.push(format!("Common domain '{}'", t_domain));
                                }
                            }
                        }
                    }
                }

                // Check session match
                let mut session_matches = 0;
                for t_sig in &target_signals {
                    if let Some(t_session) = &t_sig.session_id {
                        session_matches += ws_sessions.iter().filter(|&s| s == t_session).count();
                    }
                }
                if session_matches > 0 {
                    let s = (session_matches as f32 * weights.same_session_per_match).min(weights.same_session_max);
                    score += s;
                    reasons.push("Captured in the same Context Session".to_string());
                }
            }
        }

        // --- Context Lifecycle State Influence ---
        if ws.lifecycle_state == "archived" {
            score = 0.0;
            reasons.push(format!("{} is archived by user; suggestions suppressed", ws.name));
        } else if ws.lifecycle_state == "dormant" {
            score *= 0.50;
            reasons.push(format!("{} is dormant (no recent meaningful activity)", ws.name));
        }

        let clamped_score = (score as f64).clamp(0.0, 1.0);
        let confidence_band = if clamped_score >= 0.60 {
            "High"
        } else if clamped_score >= 0.35 {
            "Medium"
        } else {
            "Low"
        };

        let contextual_reasons = crate::context_engine::explanation::get_concise_explanations(conn, item_id, &ws.id);
        let final_reasons = if !contextual_reasons.is_empty() && clamped_score >= 0.20 {
            contextual_reasons
        } else if !reasons.is_empty() {
            reasons
        } else {
            vec![format!("Baseline comparison with minimal contextual overlap with {}.", ws.name)]
        };

        existing_workspaces.push(crate::db::models::ExistingWorkspaceEvaluation {
            workspace_id: ws.id.clone(),
            workspace_name: ws.name.clone(),
            score: clamped_score,
            confidence: clamped_score,
            confidence_percentage: format!("{:.0}%", (clamped_score * 100.0).round()),
            confidence_band: confidence_band.to_string(),
            reasons: final_reasons,
        });
    }

    // Rank existing workspaces descending
    existing_workspaces.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));

    // --- Evaluate Against Potential New ContextCandidates ---
    let mut potential_new_contexts = Vec::new();

    // 1. Check if the item already belongs to an active detected/suggested candidate
    let mut candidate_stmt = conn.prepare(
        "SELECT c.id, c.label, c.confidence,
                (SELECT COUNT(*) FROM context_candidate_items WHERE candidate_id = c.id) as item_count
         FROM context_candidates c
         JOIN context_candidate_items ci ON c.id = ci.candidate_id
         WHERE ci.item_id = ?1 AND c.status IN ('detected', 'suggested')
         ORDER BY c.confidence DESC",
    ).map_err(|e| e.to_string())?;

    let candidate_rows = candidate_stmt.query_map(rusqlite::params![item_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, usize>(3)?,
        ))
    }).map_err(|e| e.to_string())?;

    let mut found_candidate = false;
    for c_res in candidate_rows.flatten() {
        found_candidate = true;
        let (cand_id, label, conf, count) = c_res;
        let band = if conf >= 0.70 { "High" } else if conf >= 0.40 { "Medium" } else { "Low" };
        let display_label = label.unwrap_or_else(|| "possible new context".to_string());
        potential_new_contexts.push(crate::db::models::PotentialNewContextEvaluation {
            candidate_id: Some(cand_id.clone()),
            label: display_label,
            confidence: conf,
            confidence_percentage: format!("{:.0}%", (conf * 100.0).round()),
            confidence_band: band.to_string(),
            item_count: count,
            reasons: vec![
                format!("Detected Context Candidate with {} related items", count),
                "Cohesive session and relationship cluster".to_string(),
            ],
        });
    }

    // 2. If not already in an active candidate, evaluate cluster around target item
    if !found_candidate {
        let all_unassigned = crate::db::operations::list_items(conn, None).unwrap_or_default();
        let mut peer_ids = Vec::new();

        for other in &all_unassigned {
            if other.id != item_id {
                let same_session = target_item.session_id.is_some() && other.session_id == target_item.session_id;
                let has_rel = relationships.iter().any(|r| (r.item_a_id == other.id || r.item_b_id == other.id) && r.strength >= 0.4);
                if same_session || has_rel {
                    peer_ids.push(other.id.clone());
                }
            }
        }

        if !peer_ids.is_empty() {
            let mut group = vec![item_id.to_string()];
            group.extend(peer_ids);
            if let Ok(group_score) = crate::context_engine::discovery::score_candidate_group(conn, &group, None) {
                let conf = group_score.confidence;
                let band = if conf >= 0.70 { "High" } else if conf >= 0.40 { "Medium" } else { "Low" };
                let group_label = crate::context_engine::discovery::generate_candidate_name(conn, &group)
                    .unwrap_or_else(|_| "possible new context".to_string());

                potential_new_contexts.push(crate::db::models::PotentialNewContextEvaluation {
                    candidate_id: None,
                    label: group_label,
                    confidence: conf,
                    confidence_percentage: format!("{:.0}%", (conf * 100.0).round()),
                    confidence_band: band.to_string(),
                    item_count: group.len(),
                    reasons: group_score.evidence,
                });
            }
        } else {
            // Single unassigned item: calculate novelty score against existing workspaces
            let max_existing = existing_workspaces.first().map(|w| w.score).unwrap_or(0.0);
            let (conf, reasons) = if max_existing >= 0.60 {
                // High existing match (e.g. 91% Paris Trip) -> novelty is low (e.g. 3%)
                let c = ((1.0 - max_existing) * 0.35).clamp(0.01, 0.05);
                let top_name = &existing_workspaces[0].workspace_name;
                (c, vec![format!("Strongly matches existing workspace '{}' ({:.0}%); novel context unlikely", top_name, max_existing * 100.0)])
            } else if max_existing >= 0.30 {
                let c = ((1.0 - max_existing) * 0.30).clamp(0.05, 0.20);
                (c, vec!["Moderate overlap with existing workspaces; weak evidence for a standalone new context".to_string()])
            } else {
                let c = (0.50 * (1.0 - max_existing)).clamp(0.20, 0.45);
                (c, vec!["Does not strongly match existing workspaces; potential starting point for a new context pending further activity".to_string()])
            };

            let band = if conf >= 0.70 { "High" } else if conf >= 0.40 { "Medium" } else { "Low" };
            potential_new_contexts.push(crate::db::models::PotentialNewContextEvaluation {
                candidate_id: None,
                label: "possible new context".to_string(),
                confidence: conf,
                confidence_percentage: format!("{:.0}%", (conf * 100.0).round()),
                confidence_band: band.to_string(),
                item_count: 1,
                reasons,
            });
        }
    }

    potential_new_contexts.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(Ordering::Equal));

    // --- Unified Interpretation & Summary ---
    let top_ws = existing_workspaces.first();
    let top_new = potential_new_contexts.first();

    let (primary_interpretation, summary) = if let Some(ws) = top_ws {
        // Check if top candidate is ambiguous with runner-up
        let is_ambiguous = existing_workspaces.len() > 1
            && (existing_workspaces[0].score - existing_workspaces[1].score).abs() <= 0.12
            && existing_workspaces[1].score >= 0.35;

        if is_ambiguous {
            let ws2 = &existing_workspaces[1];
            (
                "ambiguous".to_string(),
                format!(
                    "Ambiguous match: competing affinity between '{}' ({}) and '{}' ({}); item remains in Inbox without forced assignment",
                    ws.workspace_name, ws.confidence_percentage, ws2.workspace_name, ws2.confidence_percentage
                ),
            )
        } else if ws.confidence >= 0.60 && (top_new.is_none() || ws.confidence >= top_new.unwrap().confidence) {
            (
                "existing_workspace".to_string(),
                format!("Strong match to existing workspace '{}' ({})", ws.workspace_name, ws.confidence_percentage),
            )
        } else if ws.confidence < 0.25 {
            (
                "unassigned".to_string(),
                "Weak signals detected; item remains in Inbox".to_string(),
            )
        } else if let Some(new_ctx) = top_new {
            if new_ctx.confidence >= 0.60 {
                (
                    "new_context".to_string(),
                    format!("Cohesive new context detected: '{}' ({})", new_ctx.label, new_ctx.confidence_percentage),
                )
            } else {
                (
                    "unassigned".to_string(),
                    "Low confidence or ambiguous across workspaces; item remains in Inbox".to_string(),
                )
            }
        } else {
            (
                "unassigned".to_string(),
                "No decisive contextual match found".to_string(),
            )
        }
    } else {
        (
            "unassigned".to_string(),
            "No workspaces available for evaluation".to_string(),
        )
    };

    Ok(crate::db::models::UnifiedContextInterpretation {
        item_id: item_id.to_string(),
        existing_workspaces,
        potential_new_contexts,
        primary_interpretation,
        summary,
    })
}

/// Recalculates confidence for suggested ContextMemberships associated with a workspace.
/// Triggered when new items are added or moved into the workspace.
///
/// Rules:
/// - Keep calculation deterministic.
/// - Do NOT silently convert suggested into confirmed membership.
/// - User-confirmed membership remains authoritative.
/// - Store the latest confidence in context_memberships.
/// - Preserve meaningful history when confidence changes (via context_signals).
/// - Avoid recalculating every item unnecessarily (only targets suggested items and directly related unassigned items).
pub fn recalculate_workspace_suggested_memberships(
    conn: &Connection,
    workspace_id: &str,
    trigger_item_id: Option<&str>,
) -> Result<Vec<crate::db::models::ContextMembership>, String> {
    let ws = crate::db::operations::get_workspace(conn, workspace_id)
        .map_err(|e| format!("Workspace '{}' not found: {}", workspace_id, e))?;

    // Archived workspaces are suppressed from automatic suggestion recalculation
    if ws.lifecycle_state == "archived" {
        return Ok(Vec::new());
    }

    // 1. Identify candidate items to recalculate without scanning the entire database:
    let mut target_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    // a) Items currently having a 'suggested' membership for this workspace
    if let Ok(mut stmt) = conn.prepare(
        "SELECT item_id FROM context_memberships WHERE workspace_id = ?1 AND membership_type = 'suggested'"
    ) {
        if let Ok(rows) = stmt.query_map(rusqlite::params![workspace_id], |row| row.get::<_, String>(0)) {
            for r in rows.flatten() {
                target_ids.insert(r);
            }
        }
    }

    // b) If trigger_item_id is provided, also look for unassigned items related to this trigger item
    if let Some(t_id) = trigger_item_id {
        if let Ok(mut rel_stmt) = conn.prepare(
            "SELECT DISTINCT CASE WHEN r.item_a_id = ?1 THEN r.item_b_id ELSE r.item_a_id END as other_id
             FROM item_relationships r
             JOIN items other ON other.id = (CASE WHEN r.item_a_id = ?1 THEN r.item_b_id ELSE r.item_a_id END)
             WHERE (r.item_a_id = ?1 OR r.item_b_id = ?1)
               AND other.workspace_id IS NULL
               AND r.strength >= 0.40"
        ) {
            if let Ok(rows) = rel_stmt.query_map(rusqlite::params![t_id], |row| row.get::<_, String>(0)) {
                for r in rows.flatten() {
                    target_ids.insert(r);
                }
            }
        }
    }

    if target_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut updated_memberships = Vec::new();

    for target_item_id in target_ids {
        // Exclude trigger_item itself
        if Some(target_item_id.as_str()) == trigger_item_id {
            continue;
        }

        // Never touch confirmed or dismissed memberships (User-confirmed is authoritative)
        let existing_membership = crate::db::operations::get_item_workspace_membership(conn, &target_item_id, workspace_id)
            .ok()
            .flatten();

        if let Some(ref m) = existing_membership {
            if m.membership_type == "confirmed" || m.membership_type == "dismissed" {
                continue;
            }
        }

        // Also check if item physically belongs to this or another workspace
        if let Ok(item) = crate::db::operations::get_item(conn, &target_item_id) {
            if item.workspace_id.is_some() && item.workspace_id.as_deref() != Some(workspace_id) {
                // Item is physically in another workspace and user-assigned; skip
                continue;
            }
        }

        // Deterministic recalculation
        let scores = score_item(conn, &target_item_id).unwrap_or_default();
        let is_ambiguous = scores.len() > 1
            && (scores[0].confidence - scores[1].confidence).abs() <= 0.12
            && scores[1].confidence >= 0.35;

        let new_score = scores.into_iter().find(|s| s.workspace_id == workspace_id);
        let new_confidence = new_score.map(|s| s.confidence as f64).unwrap_or(0.0).clamp(0.0, 1.0);

        let old_confidence = existing_membership.as_ref().map(|m| m.confidence).unwrap_or(0.0);
        let delta = (new_confidence - old_confidence).abs();

        // If changed by >= 1% or new suggested membership qualified (only when unambiguous)
        let should_qualify = if existing_membership.is_some() {
            delta >= 0.01
        } else {
            new_confidence >= 0.35 && !is_ambiguous
        };

        if should_qualify {
            // NEVER silently convert suggested to confirmed: always keeps "suggested"
            let membership = crate::db::operations::create_or_update_membership(
                conn,
                &target_item_id,
                workspace_id,
                "suggested",
                new_confidence,
                "context_engine",
            ).map_err(|e| e.to_string())?;

            // Preserve meaningful history when confidence changes
            let payload = serde_json::json!({
                "action": "membership_confidence_recalculated",
                "workspace_id": workspace_id,
                "workspace_name": ws.name,
                "item_id": target_item_id,
                "previous_confidence": old_confidence,
                "new_confidence": new_confidence,
                "delta": new_confidence - old_confidence,
                "trigger_item_id": trigger_item_id,
                "reason": format!(
                    "Confidence dynamically updated from {:.0}% to {:.0}% as new items were added to {}",
                    old_confidence * 100.0,
                    new_confidence * 100.0,
                    ws.name
                )
            }).to_string();

            let _ = crate::context_engine::operations::create_context_signal(
                conn,
                Some(workspace_id),
                Some(&target_item_id),
                "membership_confidence_recalculated",
                None,
                &payload,
            );

            updated_memberships.push(membership);
        }
    }

    Ok(updated_memberships)
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
        assert_eq!(scores.len(), 1);
        
        let w2_score = scores.iter().find(|s| s.workspace_id == ws2.id).unwrap();
        
        assert!(w2_score.total_score >= 0.4);
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
        
        // score is 0.12, but threshold is 0.3. Wait, we should add history or lower threshold for test.
        // Let's add history to push it above threshold
        let payload3 = serde_json::json!({
            "action": "moved_to_workspace",
            "destination_workspace": ws.id
        }).to_string();
        let _ = create_context_signal(&conn, None, Some(&target.id), "user_action", None, &payload3).unwrap();
        
        let scores = score_item(&conn, &target.id).unwrap();
        assert_eq!(scores.len(), 1);
        
        let score = &scores[0];
        assert_eq!(score.workspace_id, ws.id);
        
        // history = 0.4
        // content_type_matches = 1 (0.02)
        // domain_match = 1 (0.1)
        // Total = 0.52
        assert!((score.total_score - 0.52).abs() < f32::EPSILON);
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
        
        // Target -> related 3 (strength 0.8) to push score above threshold
        let related_item_3 = create_item(&conn, Some(&ws.id), None, "text", "r3", None, "{}").unwrap();
        let _ = crate::db::operations::create_item_relationship(
            &conn,
            &target.id,
            &related_item_3.id,
            "same_app",
            0.8,
            None
        ).unwrap();
        
        let scores = score_item(&conn, &target.id).unwrap();
        assert_eq!(scores.len(), 1);
        
        let score = &scores[0];
        assert_eq!(score.workspace_id, ws.id);
        
        // Content type match for 3 items: 3 * 0.02 = 0.06
        // Relationships: (0.6 + 0.4 + 0.8) = 1.8 * related_item_per_match (0.2) = 0.36
        // Total = 0.42
        assert!((score.total_score - 0.42).abs() < f32::EPSILON);
        assert!(score.reasons.iter().any(|r| r == "3 related Items already belong to Portfolio"));
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
    
    #[test]
    fn test_scoring_with_workspace_profile() {
        let conn = setup_in_memory_db();
        
        let ws = create_workspace(&conn, "Profile WS").unwrap();
        
        let target = create_item(&conn, None, None, "text", "target", None, "{}").unwrap();
        crate::db::operations::save_item_embedding(&conn, &target.id, &[1.0, 0.0, 0.0], "test", "test").unwrap();
        
        // Mock a workspace profile that is identical to target
        let bytes: Vec<u8> = vec![1.0f32, 0.0, 0.0].iter().flat_map(|&f| f.to_ne_bytes()).collect();
        conn.execute(
            "INSERT INTO workspace_embeddings (workspace_id, embedding, provider, model, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![ws.id, bytes, "test", "test", "time"],
        ).unwrap();
        
        let scores = score_item(&conn, &target.id).unwrap();
        assert_eq!(scores.len(), 1);
        
        let score = &scores[0];
        // 0.45 from profile similarity max.
        assert!((score.total_score - 0.45).abs() < f32::EPSILON);
        assert!(score.reasons.iter().any(|r| r == "Matches Profile WS semantic profile: 1.00"));
    }

    #[test]
    fn test_scoring_confidence_bands() {
        let conn = setup_in_memory_db();
        
        let ws_high = create_workspace(&conn, "High WS").unwrap();
        let ws_med = create_workspace(&conn, "Medium WS").unwrap();
        let ws_low = create_workspace(&conn, "Low WS").unwrap();
        let ws_filtered = create_workspace(&conn, "Filtered WS").unwrap();
        
        let target = create_item(&conn, None, None, "text", "target", None, "{}").unwrap();
        
        // High WS: score should be >= 0.6
        // history (0.4) + relationships (0.2) = 0.6
        let r_high = create_item(&conn, Some(&ws_high.id), None, "text", "rh", None, "{}").unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &r_high.id, "same_app", 1.0, None).unwrap();
        let payload_high = serde_json::json!({"action": "moved_to_workspace", "destination_workspace": ws_high.id}).to_string();
        let _ = create_context_signal(&conn, None, Some(&target.id), "user_action", None, &payload_high).unwrap();
        
        // Medium WS: score should be >= 0.4 and < 0.6
        // history (0.4) + content_type (0.02) = 0.42
        let _ = create_item(&conn, Some(&ws_med.id), None, "text", "rm", None, "{}").unwrap();
        let payload_med = serde_json::json!({"action": "moved_to_workspace", "destination_workspace": ws_med.id}).to_string();
        let _ = create_context_signal(&conn, None, Some(&target.id), "user_action", None, &payload_med).unwrap();
        
        // Low WS: score should be >= 0.2 and < 0.4
        // relationship (0.2) + content_type (0.02) = 0.22
        let r_low = create_item(&conn, Some(&ws_low.id), None, "text", "rl", None, "{}").unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &r_low.id, "same_app", 1.0, None).unwrap();
        
        // Filtered WS: score < 0.2
        // Just content type = 0.02 -> should not be returned
        let _ = create_item(&conn, Some(&ws_filtered.id), None, "text", "rf", None, "{}").unwrap();
        
        let scores = score_item(&conn, &target.id).unwrap();
        
        // Should only return high, med, low (filtered is below 0.2)
        assert_eq!(scores.len(), 3);
        
        let high_score = scores.iter().find(|s| s.workspace_id == ws_high.id).unwrap();
        assert_eq!(high_score.confidence_band, "High");
        
        let med_score = scores.iter().find(|s| s.workspace_id == ws_med.id).unwrap();
        assert_eq!(med_score.confidence_band, "Medium");
        
        let low_score = scores.iter().find(|s| s.workspace_id == ws_low.id).unwrap();
        assert_eq!(low_score.confidence_band, "Low");
        
        let filtered_score = scores.iter().find(|s| s.workspace_id == ws_filtered.id);
        assert!(filtered_score.is_none());
    }

    #[test]
    fn test_evaluate_item_context_unified_interpretation() {
        let conn = setup_in_memory_db();

        // Workspaces
        let ws_paris = create_workspace(&conn, "Paris Trip").unwrap();
        let ws_portfolio = create_workspace(&conn, "Portfolio").unwrap();
        let ws_college = create_workspace(&conn, "College").unwrap();

        // Add items to Portfolio and College (different apps & content)
        let _port_item = create_item(&conn, Some(&ws_portfolio.id), None, "code", "const x = 42;", Some("Project code"), "{\"app_name\":\"VSCode\"}").unwrap();
        let _coll_item = create_item(&conn, Some(&ws_college.id), None, "pdf", "Calculus notes", Some("Math Lecture"), "{\"app_name\":\"Preview\"}").unwrap();

        // Add items to Paris Trip
        let paris_item1 = create_item(&conn, Some(&ws_paris.id), None, "url", "https://airbnb.com/paris", Some("Paris Hotel"), "{\"app_name\":\"Safari\"}").unwrap();
        let paris_item2 = create_item(&conn, Some(&ws_paris.id), None, "url", "https://booking.com/paris-flights", Some("Paris Flight"), "{\"app_name\":\"Safari\"}").unwrap();

        let _ = create_context_signal(&conn, Some(&ws_paris.id), Some(&paris_item1.id), "url_metadata", None, "{\"domain\":\"airbnb.com\"}").unwrap();
        let _ = create_context_signal(&conn, Some(&ws_paris.id), Some(&paris_item1.id), "active_app", None, "{\"app_name\":\"Safari\"}").unwrap();
        let _ = create_context_signal(&conn, Some(&ws_paris.id), Some(&paris_item2.id), "url_metadata", None, "{\"domain\":\"booking.com\"}").unwrap();
        let _ = create_context_signal(&conn, Some(&ws_paris.id), Some(&paris_item2.id), "active_app", None, "{\"app_name\":\"Safari\"}").unwrap();

        // Target Item: Paris restaurant (in Inbox)
        let target = create_item(&conn, None, None, "url", "https://tripadvisor.com/paris-bistro", Some("Paris restaurant"), "{\"app_name\":\"Safari\"}").unwrap();
        let _ = create_context_signal(&conn, None, Some(&target.id), "active_app", None, "{\"app_name\":\"Safari\"}").unwrap();
        let _ = create_context_signal(&conn, None, Some(&target.id), "url_metadata", None, "{\"domain\":\"tripadvisor.com\"}").unwrap();

        // Historical action: user previously moved similar item to Paris Trip
        let hist_payload = serde_json::json!({
            "action": "moved_to_workspace",
            "destination_workspace": ws_paris.id
        }).to_string();
        let _ = create_context_signal(&conn, None, Some(&target.id), "user_action", None, &hist_payload).unwrap();

        // Strong relationship to paris_item1 and paris_item2
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &paris_item1.id, "same_app", 1.0, None).unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &paris_item2.id, "same_app", 1.0, None).unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &paris_item1.id, "temporal_proximity", 0.9, None).unwrap();

        // Run unified contextual interpretation
        let interpretation = evaluate_item_context(&conn, &target.id).unwrap();

        // 1. Verify ranking across existing workspaces
        assert_eq!(interpretation.existing_workspaces.len(), 3);
        assert_eq!(interpretation.existing_workspaces[0].workspace_name, "Paris Trip");
        assert!(interpretation.existing_workspaces[0].score >= 0.70, "Paris Trip should have high score (actual: {})", interpretation.existing_workspaces[0].score);
        assert_eq!(interpretation.existing_workspaces[0].confidence_band, "High");

        // Portfolio and College should be ranked much lower
        let portfolio_eval = interpretation.existing_workspaces.iter().find(|w| w.workspace_name == "Portfolio").unwrap();
        let college_eval = interpretation.existing_workspaces.iter().find(|w| w.workspace_name == "College").unwrap();
        assert!(portfolio_eval.score < 0.20);
        assert!(college_eval.score < 0.20);

        // 2. Potential new context
        assert!(!interpretation.potential_new_contexts.is_empty());
        let new_ctx = &interpretation.potential_new_contexts[0];
        assert_eq!(new_ctx.label, "possible new context");
        // Because Paris Trip is a strong existing match, new context confidence is low (<= 5%)
        assert!(new_ctx.confidence <= 0.05, "Novelty should be low when existing workspace match is strong (actual: {})", new_ctx.confidence);

        // 3. Explainable reasons are present
        assert!(!interpretation.existing_workspaces[0].reasons.is_empty());
        assert!(!new_ctx.reasons.is_empty());

        // 4. Primary interpretation
        assert_eq!(interpretation.primary_interpretation, "existing_workspace");
        assert!(interpretation.summary.contains("Paris Trip"));

        // 5. Invariant: target item remains unassigned in Inbox, no workspace or candidate created
        let target_after = crate::db::operations::get_item(&conn, &target.id).unwrap();
        assert_eq!(target_after.workspace_id, None);
        assert_eq!(crate::db::operations::list_items(&conn, Some(&ws_paris.id)).unwrap().len(), 2);
    }

    #[test]
    fn test_dynamic_context_membership_confidence_recalculation() {
        let conn = setup_in_memory_db();

        // 1. Create Workspace
        let ws_paris = create_workspace(&conn, "Paris Trip").unwrap();

        // 2. Initial item in Paris Trip
        let initial_item = create_item(
            &conn,
            Some(&ws_paris.id),
            None,
            "url",
            "https://booking.com/paris-hotel",
            Some("Paris Hotel"),
            "{\"app_name\":\"Safari\",\"original_url\":\"https://booking.com/paris-hotel\"}",
        ).unwrap();

        // 3. Unassigned Inbox Item: Paris restaurant
        let restaurant = create_item(
            &conn,
            None,
            None,
            "url",
            "https://tripadvisor.com/paris-bistro",
            Some("Paris restaurant"),
            "{\"app_name\":\"Safari\",\"original_url\":\"https://tripadvisor.com/paris-bistro\"}",
        ).unwrap();

        // Establish initial connection with initial_item
        let _ = crate::db::operations::create_item_relationship(&conn, &restaurant.id, &initial_item.id, "same_app", 1.0, None).unwrap();

        // Initial suggested membership: ~61%
        let initial_membership = crate::db::operations::create_or_update_membership(
            &conn,
            &restaurant.id,
            &ws_paris.id,
            "suggested",
            0.61,
            "context_engine",
        ).unwrap();
        assert_eq!(initial_membership.confidence, 0.61);
        assert_eq!(initial_membership.membership_type, "suggested");

        // 4. Add additional related Items to Paris Trip
        let flight = create_item(
            &conn,
            Some(&ws_paris.id),
            None,
            "url",
            "https://airfrance.com/flight-paris",
            Some("Paris Flight"),
            "{\"app_name\":\"Safari\",\"original_url\":\"https://airfrance.com/flight-paris\"}",
        ).unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &restaurant.id, &flight.id, "same_app", 1.0, None).unwrap();

        let museum = create_item(
            &conn,
            Some(&ws_paris.id),
            None,
            "text",
            "Louvre tickets 10am, Orsay 3pm",
            Some("Paris Museum Guide"),
            "{\"app_name\":\"Safari\"}",
        ).unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &restaurant.id, &museum.id, "same_app", 1.0, None).unwrap();

        // History signal: user previously moved items to Paris Trip
        let hist_payload = serde_json::json!({
            "action": "moved_to_workspace",
            "destination_workspace": ws_paris.id
        }).to_string();
        let _ = create_context_signal(&conn, None, Some(&restaurant.id), "user_action", None, &hist_payload).unwrap();

        // 5. Trigger dynamic recalculation
        let updated = recalculate_workspace_suggested_memberships(&conn, &ws_paris.id, Some(&museum.id)).unwrap();
        assert!(!updated.is_empty(), "Should have updated the suggested membership");

        // 6. Verify updated membership in DB
        let stored_membership = crate::db::operations::get_item_workspace_membership(&conn, &restaurant.id, &ws_paris.id)
            .unwrap()
            .unwrap();

        // Requirements verification:
        // a) Confidence increased dynamically (from 61% to high confidence ~89%)
        assert!(
            stored_membership.confidence >= 0.80,
            "Confidence should increase after additional related items (actual: {})",
            stored_membership.confidence
        );

        // b) Deterministic: recalculating again produces same result
        let rerun = recalculate_workspace_suggested_memberships(&conn, &ws_paris.id, None).unwrap();
        assert!(rerun.is_empty(), "Rerunning without new changes should not produce delta");

        // c) Do not silently convert suggested into confirmed membership
        assert_eq!(stored_membership.membership_type, "suggested");

        // d) Physical location remains in Inbox
        let target_item_after = crate::db::operations::get_item(&conn, &restaurant.id).unwrap();
        assert_eq!(target_item_after.workspace_id, None);

        // e) Meaningful history preserved in context_signals
        let signals = crate::context_engine::operations::list_context_signals(&conn, Some(&ws_paris.id), Some(&restaurant.id)).unwrap();
        let recalc_signals: Vec<_> = signals
            .iter()
            .filter(|s| s.payload.contains("membership_confidence_recalculated"))
            .collect();
        assert!(!recalc_signals.is_empty(), "Should record confidence update signal in history");

        let latest_payload: serde_json::Value = serde_json::from_str(&recalc_signals.first().unwrap().payload).unwrap();
        assert!(latest_payload["new_confidence"].as_f64().unwrap() >= 0.80);
        assert!(latest_payload["delta"].as_f64().unwrap() != 0.0);
        assert_eq!(latest_payload["workspace_id"].as_str().unwrap(), ws_paris.id);
    }

    #[test]
    fn test_temporal_context_weighting_recent_vs_old() {
        let conn = setup_in_memory_db();
        let now = chrono::Utc::now();
        let time_1h_ago = (now - chrono::Duration::hours(1)).to_rfc3339();
        let time_90d_ago = (now - chrono::Duration::days(90)).to_rfc3339();

        // 1. Create two workspaces
        let ws_recent = create_workspace(&conn, "Recent Project").unwrap();
        let ws_old = create_workspace(&conn, "Archived Project").unwrap();

        // 2. Create item in Recent Project (1 hour ago)
        let recent_item = create_item(
            &conn,
            Some(&ws_recent.id),
            None,
            "text",
            "Project architecture notes",
            Some("Recent Architecture"),
            "{}",
        ).unwrap();
        // Update its timestamp to 1h ago
        conn.execute("UPDATE items SET created_at = ?1 WHERE id = ?2", rusqlite::params![time_1h_ago, recent_item.id]).unwrap();

        // 3. Create identical item in Archived Project (90 days ago)
        let old_item = create_item(
            &conn,
            Some(&ws_old.id),
            None,
            "text",
            "Project architecture notes",
            Some("Old Architecture"),
            "{}",
        ).unwrap();
        // Update its timestamp to 90 days ago
        conn.execute("UPDATE items SET created_at = ?1 WHERE id = ?2", rusqlite::params![time_90d_ago, old_item.id]).unwrap();

        // 4. Target Item in Inbox (current)
        let target = create_item(
            &conn,
            None,
            None,
            "text",
            "Target architecture query",
            Some("Target Query"),
            "{}",
        ).unwrap();

        // 5. Connect target with both items with equal relationship strength (0.90)
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &recent_item.id, "same_topic", 0.90, None).unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &old_item.id, "same_topic", 0.90, None).unwrap();

        // 6. Evaluate contextual interpretation
        let interpretation = evaluate_item_context(&conn, &target.id).unwrap();
        let eval_recent = interpretation.existing_workspaces.iter().find(|w| w.workspace_id == ws_recent.id).unwrap();
        let eval_old = interpretation.existing_workspaces.iter().find(|w| w.workspace_id == ws_old.id).unwrap();

        // Requirements verification:
        // a) Recent activity provides stronger evidence than old activity
        assert!(
            eval_recent.score > eval_old.score,
            "Recent activity ({}) should score higher than old activity ({})",
            eval_recent.score,
            eval_old.score
        );

        // b) Historical context remains accessible (decayed to floor, not zeroed out)
        assert!(
            eval_old.score > 0.0,
            "Old activity should not be completely zeroed out (actual: {})",
            eval_old.score
        );

        // c) Old Items are NOT deleted
        let old_item_check = crate::db::operations::get_item(&conn, &old_item.id);
        assert!(old_item_check.is_ok(), "Old item must not be deleted");

        // d) User-confirmed membership remains stable and authoritative
        let confirmed_old_membership = crate::db::operations::get_item_workspace_membership(&conn, &old_item.id, &ws_old.id).unwrap().unwrap();
        assert_eq!(confirmed_old_membership.membership_type, "confirmed");
        assert_eq!(confirmed_old_membership.confidence, 1.0);

        // e) Decay function is configurable: disable decay and verify scores become equal
        let mut no_decay_config = crate::context_engine::temporal::TemporalDecayConfig::default();
        no_decay_config.strategy = "none".to_string();
        crate::context_engine::temporal::update_temporal_decay_config(&conn, &no_decay_config).unwrap();

        let interp_no_decay = evaluate_item_context(&conn, &target.id).unwrap();
        let r_no_decay = interp_no_decay.existing_workspaces.iter().find(|w| w.workspace_id == ws_recent.id).unwrap();
        let o_no_decay = interp_no_decay.existing_workspaces.iter().find(|w| w.workspace_id == ws_old.id).unwrap();

        assert!(
            (r_no_decay.score - o_no_decay.score).abs() < 0.001,
            "With decay disabled, both workspaces should score identically"
        );
    }

    #[test]
    fn test_lifecycle_states_influence_contextual_suggestions() {
        let conn = setup_in_memory_db();

        // 1. Create three workspaces with identical items
        let ws_active = create_workspace(&conn, "Active WS").unwrap();
        let ws_dormant = create_workspace(&conn, "Dormant WS").unwrap();
        let ws_archived = create_workspace(&conn, "Archived WS").unwrap();

        // 2. Create identical items in each workspace
        let item_act = create_item(&conn, Some(&ws_active.id), None, "text", "React Hooks", Some("React 1"), "{}").unwrap();
        let item_dor = create_item(&conn, Some(&ws_dormant.id), None, "text", "React Hooks", Some("React 2"), "{}").unwrap();
        let item_arc = create_item(&conn, Some(&ws_archived.id), None, "text", "React Hooks", Some("React 3"), "{}").unwrap();

        // 3. Transition dormant and archived states
        let _ = crate::db::operations::update_workspace_lifecycle_state(&conn, &ws_dormant.id, "dormant").unwrap();
        let _ = crate::db::operations::archive_workspace(&conn, &ws_archived.id).unwrap();

        // 3. Target Item in Inbox
        let target = create_item(&conn, None, None, "text", "React Hooks Tutorial", Some("Target React"), "{}").unwrap();

        // 4. Equal relationship to all three items
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &item_act.id, "same_topic", 0.90, None).unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &item_dor.id, "same_topic", 0.90, None).unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &item_arc.id, "same_topic", 0.90, None).unwrap();

        // 5. Evaluate unified context interpretation
        let interp = evaluate_item_context(&conn, &target.id).unwrap();

        let eval_active = interp.existing_workspaces.iter().find(|w| w.workspace_id == ws_active.id).unwrap();
        let eval_dormant = interp.existing_workspaces.iter().find(|w| w.workspace_id == ws_dormant.id).unwrap();
        let eval_archived = interp.existing_workspaces.iter().find(|w| w.workspace_id == ws_archived.id).unwrap();

        // Rule verification:
        // a) Active context receives full suggestion score
        assert!(eval_active.score > 0.0);

        // b) Dormant context receives discounted suggestion score (~50%)
        assert!(
            eval_active.score > eval_dormant.score,
            "Active ({}) should score higher than dormant ({})",
            eval_active.score,
            eval_dormant.score
        );
        assert!(
            eval_dormant.score > 0.0,
            "Dormant context remains available with non-zero score"
        );

        // c) Archived context is suppressed from suggestions (score == 0.0)
        assert_eq!(eval_archived.score, 0.0, "Archived context must be suppressed from suggestions");
        assert!(eval_archived.reasons.iter().any(|r| r.contains("archived by user")));

        // d) Rule: Lifecycle must never override explicit user organization
        // User moves target item directly to archived workspace: succeeds authoritatively
        let moved = crate::db::operations::move_item(&conn, &target.id, Some(&ws_archived.id)).unwrap();
        assert_eq!(moved.workspace_id.as_deref(), Some(ws_archived.id.as_str()));
        let mem = crate::db::operations::get_item_workspace_membership(&conn, &target.id, &ws_archived.id).unwrap().unwrap();
        assert_eq!(mem.membership_type, "confirmed");
        assert_eq!(mem.confidence, 1.0);
    }
}

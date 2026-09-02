use rusqlite::Connection;
use serde_json::Value;
use std::collections::HashSet;
use crate::db::models::{ContextEvidenceItem, ContextExplanation};

/// Deterministic contextual explanation service.
/// Generates human-readable explanations using clean, template-based logic.
/// Never exposes internal vector embeddings or raw weights to the user.
pub fn explain_context_association(
    conn: &Connection,
    item_id: &str,
    workspace_id: &str,
) -> Result<ContextExplanation, String> {
    let ws = crate::db::operations::get_workspace(conn, workspace_id)
        .map_err(|e| format!("Workspace '{}' not found: {}", workspace_id, e))?;
    let item = crate::db::operations::get_item(conn, item_id)
        .map_err(|e| format!("Item '{}' not found: {}", item_id, e))?;

    let ws_name = &ws.name;
    let ws_items = crate::db::operations::list_items(conn, Some(workspace_id)).unwrap_or_default();
    let ws_item_ids: HashSet<String> = ws_items.iter().map(|i| i.id.clone()).collect();

    // Check membership status if existing
    let membership = crate::db::operations::get_item_workspace_membership(conn, item_id, workspace_id).ok().flatten();
    let (membership_type, membership_conf) = if let Some(m) = membership {
        (m.membership_type, m.confidence)
    } else if item.workspace_id.as_deref() == Some(workspace_id) {
        ("confirmed".to_string(), 1.0)
    } else {
        ("suggested".to_string(), 0.5)
    };

    let mut evidence_items: Vec<ContextEvidenceItem> = Vec::new();

    // -----------------------------------------------------------------------
    // 1. Semantic Similarity
    // -----------------------------------------------------------------------
    let relationships = crate::db::operations::get_item_relationships(conn, item_id).unwrap_or_default();
    
    let mut sem_matches = Vec::new();
    for rel in &relationships {
        if rel.relationship_type == "semantic_similarity" && rel.strength >= 0.55 {
            let other_id = if rel.item_a_id == item_id { &rel.item_b_id } else { &rel.item_a_id };
            if ws_item_ids.contains(other_id) {
                sem_matches.push((other_id.clone(), rel.strength));
            }
        }
    }

    if !sem_matches.is_empty() {
        let count = sem_matches.len();
        let expl = if count == 1 {
            let other_title = ws_items
                .iter()
                .find(|i| i.id == sem_matches[0].0)
                .and_then(|i| i.title.as_deref())
                .unwrap_or("another item");
            format!("Similar to '{}' in {}.", other_title, ws_name)
        } else {
            format!("Similar to {} items in {}.", count, ws_name)
        };

        let max_strength = sem_matches.iter().map(|(_, s)| *s).fold(0.0_f64, f64::max);
        evidence_items.push(ContextEvidenceItem {
            category: "semantic_similarity".to_string(),
            explanation: expl,
            weight: max_strength,
            related_item_ids: sem_matches.iter().map(|(id, _)| id.clone()).collect(),
        });
    }

    // -----------------------------------------------------------------------
    // 2. Session Relationships
    // -----------------------------------------------------------------------
    if let Some(target_session) = &item.session_id {
        let same_session_items: Vec<String> = ws_items
            .iter()
            .filter(|i| i.session_id.as_deref() == Some(target_session.as_str()) && i.id != item.id)
            .map(|i| i.id.clone())
            .collect();

        if !same_session_items.is_empty() {
            let count = same_session_items.len();
            let expl = if count == 1 {
                "Captured during the same activity session.".to_string()
            } else {
                format!("Captured during the same activity session as {} items in {}.", count, ws_name)
            };

            evidence_items.push(ContextEvidenceItem {
                category: "session_relationship".to_string(),
                explanation: expl,
                weight: 0.90,
                related_item_ids: same_session_items,
            });
        }
    }

    // -----------------------------------------------------------------------
    // 3. Domain Relationships & Application
    // -----------------------------------------------------------------------
    let mut target_domain = None;
    let mut target_app = None;

    if let Ok(meta) = serde_json::from_str::<Value>(&item.metadata) {
        if let Some(url_str) = meta.get("original_url").and_then(|v| v.as_str()) {
            if let Ok(parsed) = url::Url::parse(url_str) {
                target_domain = parsed.host_str().map(|h| h.to_string());
            }
        }
        if let Some(app) = meta.get("app_name").and_then(|v| v.as_str()) {
            target_app = Some(app.to_string());
        }
    }

    if let Some(dom) = &target_domain {
        let mut domain_matching_ids = Vec::new();
        for ws_i in &ws_items {
            if ws_i.id != item.id {
                if let Ok(meta) = serde_json::from_str::<Value>(&ws_i.metadata) {
                    if let Some(url_str) = meta.get("original_url").and_then(|v| v.as_str()) {
                        if let Ok(parsed) = url::Url::parse(url_str) {
                            if parsed.host_str() == Some(dom.as_str()) {
                                domain_matching_ids.push(ws_i.id.clone());
                            }
                        }
                    }
                }
            }
        }

        if !domain_matching_ids.is_empty() {
            let count = domain_matching_ids.len();
            let expl = format!(
                "Shares a domain with {} item{} already in {}.",
                count,
                if count == 1 { "" } else { "s" },
                ws_name
            );

            evidence_items.push(ContextEvidenceItem {
                category: "domain_relationship".to_string(),
                explanation: expl,
                weight: 0.75,
                related_item_ids: domain_matching_ids,
            });
        }
    }

    if let Some(app) = &target_app {
        let mut app_matching_ids = Vec::new();
        for ws_i in &ws_items {
            if ws_i.id != item.id {
                if let Ok(meta) = serde_json::from_str::<Value>(&ws_i.metadata) {
                    if let Some(a) = meta.get("app_name").and_then(|v| v.as_str()) {
                        if a == app {
                            app_matching_ids.push(ws_i.id.clone());
                        }
                    }
                }
            }
        }

        if !app_matching_ids.is_empty() {
            let count = app_matching_ids.len();
            let expl = format!(
                "Shared application '{}' with {} item{} in {}.",
                app,
                count,
                if count == 1 { "" } else { "s" },
                ws_name
            );

            evidence_items.push(ContextEvidenceItem {
                category: "domain_relationship".to_string(),
                explanation: expl,
                weight: 0.65,
                related_item_ids: app_matching_ids,
            });
        }
    }

    // -----------------------------------------------------------------------
    // 4. Strongest Relationships (General Graph Connections)
    // -----------------------------------------------------------------------
    let mut general_rel_ids = Vec::new();
    for rel in &relationships {
        if rel.relationship_type != "semantic_similarity" && rel.strength >= 0.50 {
            let other_id = if rel.item_a_id == item_id { &rel.item_b_id } else { &rel.item_a_id };
            if ws_item_ids.contains(other_id) {
                general_rel_ids.push(other_id.clone());
            }
        }
    }

    if !general_rel_ids.is_empty() {
        let count = general_rel_ids.len();
        let expl = if count == 1 {
            format!("Connected to 1 related item in {}.", ws_name)
        } else {
            format!("Connected to {} related items in {}.", count, ws_name)
        };

        evidence_items.push(ContextEvidenceItem {
            category: "strongest_relationships".to_string(),
            explanation: expl,
            weight: 0.80,
            related_item_ids: general_rel_ids,
        });
    }

    // -----------------------------------------------------------------------
    // 5. User History
    // -----------------------------------------------------------------------
    let signals = crate::context_engine::operations::list_context_signals(conn, None, Some(item_id)).unwrap_or_default();
    let mut has_user_history = false;

    for sig in &signals {
        if sig.signal_type == "user_action" {
            if let Ok(payload) = serde_json::from_str::<Value>(&sig.payload) {
                if let Some(dest) = payload.get("destination_workspace").or_else(|| payload.get("suggested_workspace")).and_then(|v| v.as_str()) {
                    if dest == workspace_id {
                        has_user_history = true;
                        break;
                    }
                }
            }
        }
    }

    if has_user_history {
        evidence_items.push(ContextEvidenceItem {
            category: "user_history".to_string(),
            explanation: format!("Matches your past organization pattern for {}.", ws_name),
            weight: 0.85,
            related_item_ids: Vec::new(),
        });
    }

    // -----------------------------------------------------------------------
    // 6. Workspace Profile Similarity
    // -----------------------------------------------------------------------
    if let Ok(Some(item_emb)) = crate::db::operations::get_item_embedding(conn, item_id) {
        if let Ok(Some(ws_profile)) = crate::db::operations::get_workspace_embedding(conn, workspace_id) {
            if item_emb.provider == ws_profile.provider && item_emb.model == ws_profile.model {
                let sim = crate::semantic::cosine_similarity(&item_emb.embedding, &ws_profile.embedding);
                if sim >= 0.60 {
                    evidence_items.push(ContextEvidenceItem {
                        category: "workspace_profile".to_string(),
                        explanation: format!("Aligns closely with the overall theme of {}.", ws_name),
                        weight: sim as f64,
                        related_item_ids: Vec::new(),
                    });
                } else if sim >= 0.35 {
                    evidence_items.push(ContextEvidenceItem {
                        category: "workspace_profile".to_string(),
                        explanation: format!("Consistent with the conceptual profile of {}.", ws_name),
                        weight: sim as f64,
                        related_item_ids: Vec::new(),
                    });
                }
            }
        }
    }

    // Sort evidence by weight descending
    evidence_items.sort_by(|a, b| b.weight.partial_cmp(&a.weight).unwrap_or(std::cmp::Ordering::Equal));

    // Extract concise bullet list (deduplicating strings)
    let mut explanations = Vec::new();
    let mut seen_expl = HashSet::new();
    for ev in &evidence_items {
        if seen_expl.insert(ev.explanation.clone()) {
            explanations.push(ev.explanation.clone());
        }
    }

    if explanations.is_empty() {
        explanations.push(format!("Baseline comparison with minimal contextual overlap with {}.", ws_name));
    }

    // Summary line
    let summary = explanations.first().cloned().unwrap_or_else(|| "Contextual association detected.".to_string());

    let final_confidence = if membership_conf > 0.0 {
        membership_conf
    } else {
        evidence_items.first().map(|e| e.weight).unwrap_or(0.50)
    };

    Ok(ContextExplanation {
        item_id: item_id.to_string(),
        workspace_id: workspace_id.to_string(),
        workspace_name: ws_name.to_string(),
        membership_type,
        confidence: final_confidence,
        confidence_percentage: format!("{:.0}%", (final_confidence * 100.0).round()),
        summary,
        explanations,
        evidence: evidence_items,
    })
}

/// Convenience helper to get just concise strings for a given item and workspace
pub fn get_concise_explanations(
    conn: &Connection,
    item_id: &str,
    workspace_id: &str,
) -> Vec<String> {
    explain_context_association(conn, item_id, workspace_id)
        .map(|e| e.explanations)
        .unwrap_or_default()
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
    fn test_context_explanation_service() {
        let conn = setup_in_memory_db();

        // 1. Create Workspace
        let ws = create_workspace(&conn, "Paris Trip").unwrap();

        // 2. Create Session
        let session = create_session(&conn).unwrap();

        // 3. Create items in Paris Trip
        let item1 = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://airbnb.com/paris-center",
            Some("Paris Center Apartment"),
            "{\"app_name\":\"Safari\",\"original_url\":\"https://airbnb.com/paris-center\"}",
        ).unwrap();

        let item2 = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://airbnb.com/paris-flat",
            Some("Paris Flat"),
            "{\"app_name\":\"Safari\",\"original_url\":\"https://airbnb.com/paris-flat\"}",
        ).unwrap();

        let item3 = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://airbnb.com/paris-villa",
            Some("Paris Villa"),
            "{\"app_name\":\"Safari\",\"original_url\":\"https://airbnb.com/paris-villa\"}",
        ).unwrap();

        // 4. Target item in Inbox (suggested membership)
        let target = create_item(
            &conn,
            None,
            Some(&session.id),
            "url",
            "https://airbnb.com/paris-bistro",
            Some("Paris Bistro Near Airbnb"),
            "{\"app_name\":\"Safari\",\"original_url\":\"https://airbnb.com/paris-bistro\"}",
        ).unwrap();

        // 5. Create semantic relationships & general relationship
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &item1.id, "semantic_similarity", 0.92, None).unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &item2.id, "semantic_similarity", 0.88, None).unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &item3.id, "semantic_similarity", 0.85, None).unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &item1.id, "same_app", 1.0, None).unwrap();

        // 6. Add user history signal
        let hist_payload = serde_json::json!({
            "action": "moved_to_workspace",
            "destination_workspace": ws.id
        }).to_string();
        let _ = create_context_signal(&conn, None, Some(&target.id), "user_action", None, &hist_payload).unwrap();

        // 7. Request explanation
        let explanation = explain_context_association(&conn, &target.id, &ws.id).unwrap();

        assert_eq!(explanation.workspace_name, "Paris Trip");
        assert!(!explanation.explanations.is_empty());

        // Verify key templates requested by prompt:
        // "Similar to 3 items in Paris Trip."
        let has_similar = explanation.explanations.iter().any(|e| e.contains("Similar to 3 items in Paris Trip"));
        assert!(has_similar, "Explanations should contain 'Similar to 3 items in Paris Trip', got: {:?}", explanation.explanations);

        // "Captured during the same activity session as 3 items in Paris Trip."
        let has_session = explanation.explanations.iter().any(|e| e.contains("Captured during the same activity session"));
        assert!(has_session, "Explanations should contain session relationship, got: {:?}", explanation.explanations);

        // "Shares a domain with 3 items already in Paris Trip."
        let has_domain = explanation.explanations.iter().any(|e| e.contains("Shares a domain with 3 items already in Paris Trip"));
        assert!(has_domain, "Explanations should contain 'Shares a domain with 3 items already in Paris Trip', got: {:?}", explanation.explanations);

        // "Matches your past organization pattern for Paris Trip."
        let has_history = explanation.explanations.iter().any(|e| e.contains("Matches your past organization pattern"));
        assert!(has_history, "Explanations should contain user history, got: {:?}", explanation.explanations);

        // Invariant: No internal raw vector numbers in any explanation string
        for expl in &explanation.explanations {
            assert!(!expl.contains("0."), "Explanation should not expose raw vector numbers: {}", expl);
            assert!(!expl.contains("["), "Explanation should not expose raw vector brackets: {}", expl);
        }
    }
}

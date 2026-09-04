use rusqlite::Connection;
use serde_json::Value;

use super::models::{
    CandidateContextEvidence, EvidenceMetadata, EvidencePackage, ItemRelationshipEvidence,
    SessionEvidence, TargetItemEvidence, TemporalEvidence, ContextReasoningResult,
};
use super::traits::AIReasoner;

/// Assembles a curated, read-only EvidencePackage from the deterministic engine.
/// Extracts verified facts from items, signals, relationships, sessions, and candidate evaluations.
///
/// CRITICAL ARCHITECTURAL RULE:
/// This builder ONLY reads from the database. It does NOT mutate any table,
/// change item state, or alter workspace memberships.
pub fn build_evidence_package(conn: &Connection, item_id: &str) -> Result<EvidencePackage, String> {
    let now = chrono::Utc::now();

    // 1. Fetch Target Item
    let item = crate::db::operations::get_item(conn, item_id)
        .map_err(|e| format!("Failed to fetch target item '{}': {}", item_id, e))?;

    // Truncate/sanitize snippet for privacy
    let content_snippet = if item.content.chars().count() > 120 {
        let truncated: String = item.content.chars().take(120).collect();
        format!("{}...", truncated)
    } else {
        item.content.clone()
    };

    // Extract App Name & Domain from signals
    let signals = crate::context_engine::operations::list_context_signals(conn, None, Some(item_id))
        .unwrap_or_default();

    let mut app_name: Option<String> = None;
    let mut domain: Option<String> = None;

    for sig in &signals {
        if sig.signal_type == "active_app" {
            if let Ok(v) = serde_json::from_str::<Value>(&sig.payload) {
                if let Some(app) = v.get("app_name").and_then(|a| a.as_str()) {
                    app_name = Some(app.to_string());
                }
            }
        } else if sig.signal_type == "url_metadata" {
            if let Ok(v) = serde_json::from_str::<Value>(&sig.payload) {
                if let Some(d) = v.get("domain").and_then(|d| d.as_str()) {
                    domain = Some(d.to_string());
                }
            }
        }
    }

    let target_item = TargetItemEvidence {
        id: item.id.clone(),
        item_type: item.item_type.clone(),
        title: item.title.clone(),
        content_snippet,
        app_name,
        domain,
        session_id: item.session_id.clone(),
        captured_at: item.created_at.clone(),
    };

    // 2. Deterministic Context Evaluation (Existing Workspaces & Potential Candidates)
    let evaluation = crate::context_engine::scoring::evaluate_item_context(conn, item_id)?;

    let mut candidate_contexts = Vec::new();

    // Convert existing workspaces
    for ws_eval in evaluation.existing_workspaces {
        let ws_obj = crate::db::operations::get_workspace(conn, &ws_eval.workspace_id).ok();
        let lifecycle_state = ws_obj
            .as_ref()
            .map(|w| w.lifecycle_state.clone())
            .unwrap_or_else(|| "active".to_string());

        let item_count = crate::db::operations::list_items(conn, Some(&ws_eval.workspace_id))
            .map(|items| items.len())
            .unwrap_or(0);

        let temporal_recency_score = ws_obj
            .as_ref()
            .and_then(|w| w.last_activity_at.as_deref())
            .map(|ts| {
                let decay = crate::context_engine::temporal::get_temporal_decay_config(conn);
                decay.compute_weight(ts, now)
            })
            .unwrap_or(1.0);

        candidate_contexts.push(CandidateContextEvidence {
            context_id: ws_eval.workspace_id,
            name: ws_eval.workspace_name,
            context_type: "workspace".to_string(),
            lifecycle_state,
            deterministic_score: ws_eval.score,
            deterministic_confidence: ws_eval.confidence,
            confidence_band: ws_eval.confidence_band,
            evidence_factors: ws_eval.reasons,
            item_count,
            temporal_recency_score,
        });
    }

    // Convert potential new context candidates
    for cand_eval in evaluation.potential_new_contexts {
        let cid = cand_eval.candidate_id.unwrap_or_else(|| format!("cand_{}", cand_eval.label));
        candidate_contexts.push(CandidateContextEvidence {
            context_id: cid,
            name: cand_eval.label,
            context_type: "candidate".to_string(),
            lifecycle_state: "active".to_string(),
            deterministic_score: cand_eval.confidence,
            deterministic_confidence: cand_eval.confidence,
            confidence_band: cand_eval.confidence_band,
            evidence_factors: cand_eval.reasons,
            item_count: cand_eval.item_count,
            temporal_recency_score: 1.0,
        });
    }

    // 3. Relationships to other items
    let relationships = crate::db::operations::get_item_relationships(conn, item_id)
        .unwrap_or_default();

    let mut relationship_evidence = Vec::new();
    for rel in relationships {
        let other_id = if rel.item_a_id == item.id {
            &rel.item_b_id
        } else {
            &rel.item_a_id
        };

        if let Ok(other_item) = crate::db::operations::get_item(conn, other_id) {
            relationship_evidence.push(ItemRelationshipEvidence {
                other_item_id: other_id.clone(),
                other_item_title: other_item.title.clone(),
                relationship_type: rel.relationship_type.clone(),
                strength: rel.strength,
                target_context_id: other_item.workspace_id,
                explanation: format!("Deterministic strength {:.2} via {}", rel.strength, rel.relationship_type),
            });
        }
    }

    // 4. Session Context
    let session_context = if let Some(session_id) = &item.session_id {
        let mut sess_items_stmt = conn.prepare(
            "SELECT id, workspace_id FROM items WHERE session_id = ?"
        ).map_err(|e| e.to_string())?;

        let sess_rows = sess_items_stmt.query_map([session_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
            ))
        }).map_err(|e| e.to_string())?;

        let mut item_count = 0;
        let mut associated_workspaces = std::collections::HashSet::new();

        for r in sess_rows.flatten() {
            item_count += 1;
            if let Some(ws_id) = r.1 {
                associated_workspaces.insert(ws_id);
            }
        }

        Some(SessionEvidence {
            session_id: session_id.clone(),
            item_count,
            associated_workspaces: associated_workspaces.into_iter().collect(),
        })
    } else {
        None
    };

    // 5. Temporal Evidence
    let decay_config = crate::context_engine::temporal::get_temporal_decay_config(conn);
    let recency_decay_weight = decay_config.compute_weight(&item.created_at, now);

    let age_days = if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&item.created_at) {
        let diff = now.signed_duration_since(dt.with_timezone(&chrono::Utc));
        (diff.num_seconds() as f64) / 86400.0
    } else {
        0.0
    };

    let temporal_context = TemporalEvidence {
        age_days: age_days.max(0.0),
        recency_decay_weight,
        is_recent: age_days <= (decay_config.grace_period_hours / 24.0),
    };

    // 6. Metadata
    let system_metadata = EvidenceMetadata {
        generated_at: now.to_rfc3339(),
        engine_version: "ContextSplit-v0.1.0".to_string(),
        privacy_boundary: "sanitized_evidence_package_only".to_string(),
    };

    Ok(EvidencePackage {
        target_item,
        candidate_contexts,
        relationships: relationship_evidence,
        session_context,
        temporal_context,
        system_metadata,
    })
}

/// Advisory reasoning invocation.
/// Builds the EvidencePackage and executes the provided AIReasoner.
/// Returns an advisory interpretation without mutating any database records or state.
pub fn analyze_item_context_ai(
    conn: &Connection,
    item_id: &str,
    reasoner: &dyn AIReasoner,
) -> Result<ContextReasoningResult, String> {
    let package = build_evidence_package(conn, item_id)?;
    reasoner.analyze_context(&package).map_err(|e| e.to_string())
}

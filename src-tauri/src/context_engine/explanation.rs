use rusqlite::Connection;

use crate::context_engine::evidence::{ContextEvidenceCollector, ContextEvidenceSet};
use crate::context_engine::scoring::{
    load_workspace_scoring_contexts, ContextEvidenceScorer, ContextScoreResult, ScoringWeights,
    WorkspaceScoringContext,
};
use crate::db::models::{ContextEvidenceItem, ContextExplanation, ContradictoryEvidenceItem};

/// Deterministic, evidence-based contextual explanation service.
/// Generates human-readable, structured explanations directly from `ContextEvidenceSet`
/// and scoring contributions produced by `ContextEvidenceScorer`.
///
/// Invariants:
/// - Never outputs generic non-explanations (e.g. "AI thinks this belongs here").
/// - Explanations strictly correspond to actual observed and inferred evidence.
/// - Clearly distinguishes supporting evidence, contradictory evidence, observed facts, and inferred signals.
/// - Never exposes internal vector embeddings, raw weights, or ungrounded claims.
pub fn explain_context_association(
    conn: &Connection,
    item_id: &str,
    workspace_id: &str,
) -> Result<ContextExplanation, String> {
    let ws = crate::db::operations::get_workspace(conn, workspace_id)
        .map_err(|e| format!("Workspace '{}' not found: {}", workspace_id, e))?;
    let item = crate::db::operations::get_item(conn, item_id)
        .map_err(|e| format!("Item '{}' not found: {}", item_id, e))?;

    // Determine membership status
    let membership = crate::db::operations::get_item_workspace_membership(conn, item_id, workspace_id)
        .ok()
        .flatten();
    let membership_type = if let Some(m) = membership {
        m.membership_type
    } else if item.workspace_id.as_deref() == Some(workspace_id) {
        "confirmed".to_string()
    } else {
        "suggested".to_string()
    };

    // 1. Gather ContextEvidence via the centralized Collector
    let evidence_set = ContextEvidenceCollector::new().collect(conn, item_id)?;

    // 2. Load Workspace Contexts
    let ws_contexts = load_workspace_scoring_contexts(conn)?;
    let target_ws = ws_contexts
        .into_iter()
        .find(|w| w.id == workspace_id)
        .unwrap_or_else(|| {
            WorkspaceScoringContext::new(&ws.id, &ws.name).with_lifecycle_state(&ws.lifecycle_state)
        });

    // 3. Score against target workspace with active temporal decay
    let mut weights = ScoringWeights::default();
    weights.temporal_decay = crate::context_engine::temporal::get_temporal_decay_config(conn);

    let enable_semantic = crate::db::settings::get_settings(conn)
        .map(|s| s.enable_semantic_suggestions)
        .unwrap_or(true);

    let scorer = ContextEvidenceScorer::with_weights(weights).with_semantic_enabled(enable_semantic);
    let score_result = scorer.score_evidence_against_workspace(&evidence_set, &target_ws);

    // 4. Build Structured ContextExplanation
    Ok(explain_from_score_result(
        &evidence_set,
        &score_result,
        &membership_type,
        item_id,
    ))
}

/// Constructs a structured, evidence-based `ContextExplanation` directly from a `ContextScoreResult`
/// and its source `ContextEvidenceSet`.
pub fn explain_from_score_result(
    evidence_set: &ContextEvidenceSet,
    score_result: &ContextScoreResult,
    membership_type: &str,
    item_id: &str,
) -> ContextExplanation {
    let ws_name = &score_result.context_name;
    let ws_id = &score_result.context_id;

    let confidence = if membership_type == "confirmed" {
        1.0
    } else {
        score_result.confidence as f64
    };
    let confidence_percentage = format!("{:.0}%", (confidence * 100.0).round());
    let confidence_band = score_result.confidence_band.clone();

    let mut why_reasons: Vec<String> = Vec::new();
    let mut uncertainty_reasons: Vec<String> = Vec::new();
    let mut supporting_evidence: Vec<ContextEvidenceItem> = Vec::new();
    let mut contradictory_evidence: Vec<ContradictoryEvidenceItem> = Vec::new();
    let mut observed_evidence: Vec<ContextEvidenceItem> = Vec::new();
    let mut inferred_evidence: Vec<ContextEvidenceItem> = Vec::new();

    // -------------------------------------------------------------------------
    // 1. Structured Supporting Reasons (Why)
    // -------------------------------------------------------------------------
    for reason in &score_result.strongest_reasons {
        let formatted = if reason.starts_with('✓') {
            reason.clone()
        } else {
            format!("✓ {}", reason)
        };
        why_reasons.push(formatted);
    }

    if why_reasons.is_empty() {
        why_reasons.push(format!("✓ Baseline comparison with minimal contextual overlap with {}.", ws_name));
    }

    // -------------------------------------------------------------------------
    // 2. Structured Uncertainty / Contradictory Indicators
    // -------------------------------------------------------------------------
    for unc in &score_result.uncertainty_indicators {
        let formatted = if unc.starts_with('•') {
            unc.clone()
        } else {
            format!("• {}", unc)
        };
        uncertainty_reasons.push(formatted);
    }

    // -------------------------------------------------------------------------
    // 3. Supporting Evidence Items & Provenance Segregation
    // -------------------------------------------------------------------------
    for contrib in &score_result.evidence_contributions {
        if !contrib.is_positive {
            continue;
        }

        // Match related item IDs from evidence_set for this category
        let mut related_item_ids = Vec::new();
        for ev in &evidence_set.items {
            if let Some(other_id) = ev.attributes.get("other_item_id").and_then(|v| v.as_str()) {
                if !related_item_ids.contains(&other_id.to_string()) {
                    related_item_ids.push(other_id.to_string());
                }
            }
        }

        let provenance = match contrib.category.as_str() {
            "confirmed_membership" | "same_session" | "source_app" | "browser_domain" | "content_type" => {
                "observed"
            }
            _ => "inferred",
        }
        .to_string();

        let item = ContextEvidenceItem {
            category: contrib.category.clone(),
            explanation: contrib.description.clone(),
            weight: contrib.weighted_score as f64,
            raw_strength: contrib.raw_strength as f64,
            provenance: provenance.clone(),
            is_positive: true,
            related_item_ids,
        };

        if provenance == "observed" {
            observed_evidence.push(item.clone());
        } else {
            inferred_evidence.push(item.clone());
        }
        supporting_evidence.push(item);
    }

    // -------------------------------------------------------------------------
    // 4. Contradictory Evidence Records
    // -------------------------------------------------------------------------
    for c in &score_result.contradictory_evidence {
        contradictory_evidence.push(ContradictoryEvidenceItem {
            category: c.category.clone(),
            description: c.description.clone(),
            penalty: c.penalty as f64,
        });
    }

    // -------------------------------------------------------------------------
    // 5. Backward-Compatible Explanations & Evidence
    // -------------------------------------------------------------------------
    let mut explanations: Vec<String> = score_result.strongest_reasons.clone();
    if explanations.is_empty() {
        explanations.push(format!("Baseline comparison with minimal contextual overlap with {}.", ws_name));
    }

    // 6. Summary line (e.g. "Portfolio — 91%")
    let summary = format!("{} — {}", ws_name, confidence_percentage);

    ContextExplanation {
        item_id: item_id.to_string(),
        workspace_id: ws_id.to_string(),
        workspace_name: ws_name.to_string(),
        membership_type: membership_type.to_string(),
        confidence,
        confidence_percentage,
        confidence_band,
        summary,
        explanations,
        evidence: supporting_evidence.clone(),
        why_reasons,
        uncertainty_reasons,
        supporting_evidence,
        contradictory_evidence,
        observed_evidence,
        inferred_evidence,
        ambiguity_state: score_result
            .confidence_factors
            .as_ref()
            .and_then(|f| f.ambiguity_state)
            .map(|s| s.as_str().to_string())
            .or_else(|| {
                score_result
                    .competition_analysis
                    .as_ref()
                    .map(|c| c.ambiguity_state.as_str().to_string())
            }),
    }
}

/// Convenience helper to get concise human-readable reasons for a given item and workspace.
pub fn get_concise_explanations(
    conn: &Connection,
    item_id: &str,
    workspace_id: &str,
) -> Vec<String> {
    explain_context_association(conn, item_id, workspace_id)
        .map(|e| e.why_reasons)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use crate::context_engine::operations::create_context_signal;
    use crate::db::operations::{create_item, create_session, create_workspace};

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
        )
        .unwrap();

        let item2 = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://airbnb.com/paris-flat",
            Some("Paris Flat"),
            "{\"app_name\":\"Safari\",\"original_url\":\"https://airbnb.com/paris-flat\"}",
        )
        .unwrap();

        let item3 = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://airbnb.com/paris-villa",
            Some("Paris Villa"),
            "{\"app_name\":\"Safari\",\"original_url\":\"https://airbnb.com/paris-villa\"}",
        )
        .unwrap();

        let _ = create_context_signal(&conn, Some(&ws.id), Some(&item1.id), "active_app", None, "{\"app_name\":\"Safari\"}").unwrap();
        let _ = create_context_signal(&conn, Some(&ws.id), Some(&item1.id), "url_metadata", None, "{\"domain\":\"airbnb.com\"}").unwrap();
        let _ = create_context_signal(&conn, Some(&ws.id), Some(&item2.id), "active_app", None, "{\"app_name\":\"Safari\"}").unwrap();
        let _ = create_context_signal(&conn, Some(&ws.id), Some(&item2.id), "url_metadata", None, "{\"domain\":\"airbnb.com\"}").unwrap();
        let _ = create_context_signal(&conn, Some(&ws.id), Some(&item3.id), "active_app", None, "{\"app_name\":\"Safari\"}").unwrap();
        let _ = create_context_signal(&conn, Some(&ws.id), Some(&item3.id), "url_metadata", None, "{\"domain\":\"airbnb.com\"}").unwrap();

        // 4. Target item in Inbox (suggested membership)
        let target = create_item(
            &conn,
            None,
            Some(&session.id),
            "url",
            "https://airbnb.com/paris-bistro",
            Some("Paris Bistro Near Airbnb"),
            "{\"app_name\":\"Safari\",\"original_url\":\"https://airbnb.com/paris-bistro\"}",
        )
        .unwrap();

        let _ = create_context_signal(&conn, None, Some(&target.id), "active_app", None, "{\"app_name\":\"Safari\"}").unwrap();
        let _ = create_context_signal(&conn, None, Some(&target.id), "url_metadata", None, "{\"domain\":\"airbnb.com\"}").unwrap();

        // 5. Create semantic relationships & general relationship
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &item1.id, "semantic_similarity", 0.92, None).unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &item2.id, "semantic_similarity", 0.88, None).unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &item3.id, "semantic_similarity", 0.85, None).unwrap();
        let _ = crate::db::operations::create_item_relationship(&conn, &target.id, &item1.id, "same_app", 1.0, None).unwrap();

        // 6. Add user history signal
        let hist_payload = serde_json::json!({
            "action": "moved_to_workspace",
            "destination_workspace": ws.id
        })
        .to_string();
        let _ = create_context_signal(&conn, None, Some(&target.id), "user_action", None, &hist_payload).unwrap();

        // 7. Request explanation
        let explanation = explain_context_association(&conn, &target.id, &ws.id).unwrap();

        assert_eq!(explanation.workspace_name, "Paris Trip");
        assert!(!explanation.why_reasons.is_empty());
        assert!(explanation.confidence_band == "Very High" || explanation.confidence_band == "High");
        assert!(explanation.confidence >= 0.70);
        assert!(explanation.summary.contains("Paris Trip —"));

        // Verify concise why reasons are properly formatted with ✓
        assert!(explanation.why_reasons.iter().all(|r| r.starts_with('✓')));

        // Verify key evidence categories are present in supporting reasons
        let has_semantic = explanation.why_reasons.iter().any(|e| e.contains("Semantic similarity with Paris Trip"));
        assert!(has_semantic, "Explanations should contain semantic similarity reason, got: {:?}", explanation.why_reasons);

        let has_history = explanation.why_reasons.iter().any(|e| e.contains("User previously organized similar Items into Paris Trip"));
        assert!(has_history, "Explanations should contain past user history reason, got: {:?}", explanation.why_reasons);

        let has_domain = explanation.why_reasons.iter().any(|e| e.contains("Same Airbnb domain"));
        assert!(has_domain, "Explanations should contain domain reason, got: {:?}", explanation.why_reasons);

        let has_app = explanation.why_reasons.iter().any(|e| e.contains("Same Safari source app"));
        assert!(has_app, "Explanations should contain source app reason, got: {:?}", explanation.why_reasons);

        // Verify Observed vs Inferred evidence segregation
        assert!(!explanation.observed_evidence.is_empty(), "Observed evidence must be populated");
        assert!(!explanation.inferred_evidence.is_empty(), "Inferred evidence must be populated");
        assert!(explanation.observed_evidence.iter().all(|e| e.provenance == "observed"));
        assert!(explanation.inferred_evidence.iter().all(|e| e.provenance == "inferred"));

        // Invariant: No generic placeholders or raw vector representations
        for reason in &explanation.why_reasons {
            assert!(!reason.contains("AI thinks this belongs here"));
            assert!(!reason.contains("0.923485"), "Raw vector floating numbers must not be dumped: {}", reason);
            assert!(!reason.contains('['), "Raw brackets must not be exposed: {}", reason);
        }
    }

    #[test]
    fn test_explanation_exposes_contradictory_feedback_and_uncertainty() {
        let conn = setup_in_memory_db();

        let ws = create_workspace(&conn, "Portfolio").unwrap();
        let target = create_item(&conn, None, None, "text", "Design spec", Some("Figma spec"), "{}").unwrap();

        // Add user dismissal signal
        let dismiss_payload = serde_json::json!({
            "action": "dismissed_suggestion",
            "destination_workspace": ws.id,
            "feedback_type": "user_dismissal"
        })
        .to_string();
        let _ = create_context_signal(&conn, None, Some(&target.id), "membership_dismissed", None, &dismiss_payload).unwrap();

        let explanation = explain_context_association(&conn, &target.id, &ws.id).unwrap();

        assert!(!explanation.contradictory_evidence.is_empty(), "Contradictory evidence must be exposed");
        let conflict_item = &explanation.contradictory_evidence[0];
        assert_eq!(conflict_item.category, "user_feedback");
        assert!(conflict_item.description.contains("rejected suggestion"));
        assert!(conflict_item.penalty > 0.0);

        // Verify uncertainty indicator is generated
        assert!(!explanation.uncertainty_reasons.is_empty());
        assert!(explanation.uncertainty_reasons.iter().any(|u| u.contains("negative user feedback")));
    }

    #[test]
    fn test_explanation_dormant_and_archived_lifecycle_reasons() {
        let conn = setup_in_memory_db();

        let ws_dormant = create_workspace(&conn, "Old Archives").unwrap();
        let _ = crate::db::operations::update_workspace_lifecycle_state(&conn, &ws_dormant.id, "dormant").unwrap();

        let target = create_item(&conn, None, None, "text", "Old notes", Some("Notes"), "{}").unwrap();

        let explanation = explain_context_association(&conn, &target.id, &ws_dormant.id).unwrap();

        assert!(explanation.uncertainty_reasons.iter().any(|u| u.contains("dormant")));
        assert!(explanation.why_reasons.iter().any(|r| r.contains("dormant")));
    }
}

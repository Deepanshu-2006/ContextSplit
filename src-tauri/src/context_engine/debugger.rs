use std::cmp::Ordering;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::context_engine::diversity::EvidenceDiversityCalculator;
use crate::context_engine::evidence::ContextEvidenceCollector;
use crate::context_engine::normalization::EvidenceNormalizer;
use crate::context_engine::scoring::{
    evaluate_item_context, load_workspace_scoring_contexts, ContextEvidenceScorer, ScoringWeights,
};

/// Raw contextual evidence item before weighting, decay, or scoring transformations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RawEvidenceDebugItem {
    pub id: String,
    pub category: String,
    pub provenance: String,
    pub raw_strength: f64,
    pub title: String,
    pub description: String,
    pub badge: Option<String>,
    pub attributes: std::collections::HashMap<String, serde_json::Value>,
    pub observed_at: String,
}

/// Normalized contextual evidence item reflecting applied decay, deduplication, and caps.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NormalizedEvidenceDebugItem {
    pub id: String,
    pub category: String,
    pub provenance: String,
    pub raw_strength: f64,
    pub normalized_strength: f64,
    pub effective_weight: f64,
    pub correlation_group: Option<String>,
    pub duplicate_count: usize,
    pub is_capped: bool,
    pub is_conflicting: bool,
    pub normalization_notes: Vec<String>,
}

/// Score contribution item for a single signal category toward a workspace candidate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScoreContributionDebugItem {
    pub category: String,
    pub description: String,
    pub raw_strength: f32,
    pub weighted_score: f32,
    pub percentage_of_score: f32,
    pub is_positive: bool,
}

/// Contradictory evidence item penalized against a workspace candidate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContradictionDebugItem {
    pub category: String,
    pub description: String,
    pub penalty: f32,
}

/// Structured diagnostic evaluation for a single candidate workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CandidateConfidenceDebugItem {
    pub context_id: String,
    pub context_name: String,
    pub raw_score: f32,
    pub score_formatted: u32,
    pub confidence: f32,
    pub confidence_percentage: String,
    pub confidence_band: String,
    pub evidence_bullets: Vec<String>,
    pub contradiction_bullets: Vec<String>,
    pub score_contributions: Vec<ScoreContributionDebugItem>,
    pub contradictions: Vec<ContradictionDebugItem>,
    pub uncertainty_indicators: Vec<String>,
}

/// Pairwise and margin competition diagnostic across candidate contexts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompetitionDebugInfo {
    pub top_context_id: Option<String>,
    pub top_context_name: Option<String>,
    pub top_score: u32,
    pub runner_up_context_id: Option<String>,
    pub runner_up_context_name: Option<String>,
    pub runner_up_score: u32,
    pub score_margin: u32,
    pub relative_confidence: f32,
    pub ambiguity_state: String,
    pub explanation: String,
}

/// Evidence diversity calculation breakdown.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceDiversityDebugInfo {
    pub evidence_quantity: usize,
    pub evidence_categories_count: usize,
    pub independent_groups_count: usize,
    pub independent_groups: Vec<String>,
    pub diversity_score: f32,
    pub diversity_multiplier: f32,
    pub is_single_group: bool,
    pub explanation: String,
}

/// Final high-level contextual interpretation and decision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FinalResultDebugInfo {
    pub decision: String,
    pub title: String,
    pub summary: String,
    pub primary_context_id: Option<String>,
    pub primary_context_name: Option<String>,
    pub confidence_band: String,
    pub confidence_percentage: String,
    pub no_context_reason: Option<String>,
    pub no_context_explanation: Option<String>,
}

/// Complete debug trace for an item's contextual confidence evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextConfidenceDebugTrace {
    pub item_id: String,
    pub item_title: Option<String>,
    pub item_type: String,
    pub candidates: Vec<CandidateConfidenceDebugItem>,
    pub competition: Option<CompetitionDebugInfo>,
    pub evidence_diversity: EvidenceDiversityDebugInfo,
    pub final_result: FinalResultDebugInfo,
    pub raw_evidence: Vec<RawEvidenceDebugItem>,
    pub normalized_evidence: Vec<NormalizedEvidenceDebugItem>,
}

/// Evaluates and generates a complete, fine-grained Context Confidence Debug Trace for an Item.
pub fn debug_item_context_confidence(
    conn: &Connection,
    item_id: &str,
) -> Result<ContextConfidenceDebugTrace, String> {
    let item = crate::db::operations::get_item(conn, item_id)
        .map_err(|e| format!("Failed to retrieve item '{}': {}", item_id, e))?;

    let mut weights = ScoringWeights::default();
    let decay_cfg = crate::context_engine::temporal::get_temporal_decay_config(conn);
    weights.temporal_decay = decay_cfg.clone();

    let enable_semantic = if let Ok(settings) = crate::db::settings::get_settings(conn) {
        settings.enable_semantic_suggestions
    } else {
        true
    };

    // 1. Gather Raw Context Evidence
    let raw_evidence_set = ContextEvidenceCollector::new().collect(conn, item_id)?;

    let raw_evidence: Vec<RawEvidenceDebugItem> = raw_evidence_set
        .items
        .iter()
        .map(|ev| RawEvidenceDebugItem {
            id: ev.id.clone(),
            category: ev.category.as_str().to_string(),
            provenance: ev.provenance.as_str().to_string(),
            raw_strength: ev.strength,
            title: ev.explanation.title.clone(),
            description: ev.explanation.description.clone(),
            badge: ev.explanation.badge.clone(),
            attributes: ev.attributes.clone(),
            observed_at: ev.observed_at.clone(),
        })
        .collect();

    // 2. Normalized Evidence
    let normalizer = EvidenceNormalizer::with_config(crate::context_engine::normalization::NormalizerConfig::default());
    let normalized_set = normalizer.normalize(&raw_evidence_set);

    let normalized_evidence: Vec<NormalizedEvidenceDebugItem> = normalized_set
        .items
        .iter()
        .map(|norm| NormalizedEvidenceDebugItem {
            id: norm.evidence.id.clone(),
            category: norm.evidence.category.as_str().to_string(),
            provenance: norm.evidence.provenance.as_str().to_string(),
            raw_strength: norm.raw_strength,
            normalized_strength: norm.normalized_strength,
            effective_weight: norm.effective_weight,
            correlation_group: norm.correlation_group.clone(),
            duplicate_count: norm.duplicate_count,
            is_capped: norm.is_capped,
            is_conflicting: norm.is_conflicting,
            normalization_notes: norm.normalization_notes.clone(),
        })
        .collect();

    // 3. Load Workspace Contexts and Score
    let ws_contexts = load_workspace_scoring_contexts(conn)?;
    let scorer = ContextEvidenceScorer::with_weights(weights).with_semantic_enabled(enable_semantic);
    let scored_results = scorer.score_evidence(&raw_evidence_set, &ws_contexts);

    // 4. Evidence Diversity
    let diversity_res = EvidenceDiversityCalculator::calculate_from_evidence_set(&raw_evidence_set);
    let evidence_diversity = EvidenceDiversityDebugInfo {
        evidence_quantity: diversity_res.evidence_quantity,
        evidence_categories_count: diversity_res.evidence_categories_count,
        independent_groups_count: diversity_res.independent_groups_count,
        independent_groups: diversity_res.independent_groups.clone(),
        diversity_score: diversity_res.diversity_score,
        diversity_multiplier: diversity_res.diversity_multiplier,
        is_single_group: diversity_res.is_single_group,
        explanation: diversity_res.explanation.clone(),
    };

    // 5. Candidate Scores & Bullet Formattings
    let mut candidates: Vec<CandidateConfidenceDebugItem> = Vec::new();

    for res in &scored_results {
        let total_score_u32 = (res.total_score * 100.0).round().clamp(0.0, 100.0) as u32;
        let conf_pct = format!("{:.0}%", (res.confidence * 100.0).round().clamp(0.0, 100.0));

        let total_pos_contrib: f32 = res
            .evidence_contributions
            .iter()
            .filter(|c| c.is_positive)
            .map(|c| c.weighted_score)
            .sum();

        let score_contributions: Vec<ScoreContributionDebugItem> = res
            .evidence_contributions
            .iter()
            .map(|c| {
                let pct = if total_pos_contrib > 0.0 && c.is_positive {
                    (c.weighted_score / total_pos_contrib) * 100.0
                } else {
                    0.0
                };
                ScoreContributionDebugItem {
                    category: c.category.clone(),
                    description: c.description.clone(),
                    raw_strength: c.raw_strength,
                    weighted_score: c.weighted_score,
                    percentage_of_score: pct,
                    is_positive: c.is_positive,
                }
            })
            .collect();

        let contradictions: Vec<ContradictionDebugItem> = res
            .contradictory_evidence
            .iter()
            .map(|c| ContradictionDebugItem {
                category: c.category.clone(),
                description: c.description.clone(),
                penalty: c.penalty,
            })
            .collect();

        // Format clean evidence bullets: "+ semantic similarity: 0.88", "+ same session", etc.
        let mut evidence_bullets: Vec<String> = Vec::new();
        for c in &res.evidence_contributions {
            if c.is_positive {
                if c.category.contains("semantic") {
                    evidence_bullets.push(format!("+ semantic similarity: {:.2}", c.raw_strength));
                } else if c.category == "same_session" {
                    evidence_bullets.push("+ same session".to_string());
                } else if c.category == "workspace_history" {
                    evidence_bullets.push("+ workspace history".to_string());
                } else if c.category == "confirmed_membership" {
                    evidence_bullets.push("+ confirmed membership".to_string());
                } else if c.category == "related_items" || c.category == "related_item" {
                    evidence_bullets.push(format!("+ {}", c.description.to_lowercase()));
                } else {
                    evidence_bullets.push(format!("+ {}", c.description));
                }
            }
        }

        // Format clean contradiction bullets: "- current domain also appears in College"
        let mut contradiction_bullets: Vec<String> = Vec::new();
        for c in &res.contradictory_evidence {
            contradiction_bullets.push(format!("- {}", c.description));
        }

        candidates.push(CandidateConfidenceDebugItem {
            context_id: res.context_id.clone(),
            context_name: res.context_name.clone(),
            raw_score: res.total_score,
            score_formatted: total_score_u32,
            confidence: res.confidence,
            confidence_percentage: conf_pct,
            confidence_band: res.confidence_band.clone(),
            evidence_bullets,
            contradiction_bullets,
            score_contributions,
            contradictions,
            uncertainty_indicators: res.uncertainty_indicators.clone(),
        });
    }

    // Sort candidates descending by score
    candidates.sort_by(|a, b| b.raw_score.partial_cmp(&a.raw_score).unwrap_or(Ordering::Equal));

    // 6. Competition Analysis
    let competition: Option<CompetitionDebugInfo> = if !scored_results.is_empty() {
        let top_res = &scored_results[0];
        top_res.competition_analysis.as_ref().map(|comp| {
            CompetitionDebugInfo {
                top_context_id: comp.top_context.as_ref().map(|c| c.context_id.clone()),
                top_context_name: comp.top_context.as_ref().map(|c| c.context_name.clone()),
                top_score: comp.top_context.as_ref().map_or(0, |c| (c.score * 100.0).round().clamp(0.0, 100.0) as u32),
                runner_up_context_id: comp.second_best_context.as_ref().map(|c| c.context_id.clone()),
                runner_up_context_name: comp.second_best_context.as_ref().map(|c| c.context_name.clone()),
                runner_up_score: comp.second_best_context.as_ref().map_or(0, |c| (c.score * 100.0).round().clamp(0.0, 100.0) as u32),
                score_margin: (comp.score_margin * 100.0).round().clamp(0.0, 100.0) as u32,
                relative_confidence: comp.relative_confidence,
                ambiguity_state: comp.ambiguity_state.as_str().to_string(),
                explanation: comp.explanation.clone(),
            }
        })
    } else {
        None
    };

    // 7. Unified Evaluation and Final Result
    let unified_interp = evaluate_item_context(conn, item_id)?;

    let decision_str = unified_interp
        .interpretation_kind
        .map(|k| k.as_str())
        .unwrap_or(&unified_interp.primary_interpretation);

    let (final_title, final_ctx_id, final_ctx_name, final_conf_band, final_conf_pct) =
        match decision_str {
            "existing_context" | "existing_workspace" => {
                if let Some(top) = unified_interp.existing_workspaces.first() {
                    let band_capitalized = match top.confidence_band.as_str() {
                        "very_high" => "Very High-confidence",
                        "high" => "High-confidence",
                        "medium" => "Moderate-confidence",
                        "low" => "Low-confidence",
                        _ => "Suggested",
                    };
                    (
                        format!("{} {} suggestion", band_capitalized, top.workspace_name),
                        Some(top.workspace_id.clone()),
                        Some(top.workspace_name.clone()),
                        top.confidence_band.clone(),
                        top.confidence_percentage.clone(),
                    )
                } else {
                    (
                        "Suggested workspace".to_string(),
                        None,
                        None,
                        "medium".to_string(),
                        "50%".to_string(),
                    )
                }
            }
            "new_context_candidate" | "new_context" => {
                if let Some(cand) = unified_interp.potential_new_contexts.first() {
                    (
                        format!("New Context Candidate ({})", cand.label),
                        cand.candidate_id.clone(),
                        Some(cand.label.clone()),
                        cand.confidence_band.clone(),
                        cand.confidence_percentage.clone(),
                    )
                } else {
                    (
                        "New context candidate".to_string(),
                        None,
                        None,
                        "medium".to_string(),
                        "50%".to_string(),
                    )
                }
            }
            "ambiguous" => {
                let comp_title = if let Some(ref comp) = competition {
                    if let (Some(top_n), Some(run_n)) = (&comp.top_context_name, &comp.runner_up_context_name) {
                        format!("Ambiguous Context ({} vs {})", top_n, run_n)
                    } else {
                        "Ambiguous Context (competing candidates)".to_string()
                    }
                } else {
                    "Ambiguous Context (competing candidates)".to_string()
                };
                (
                    comp_title,
                    None,
                    None,
                    "low".to_string(),
                    "35%".to_string(),
                )
            }
            _ => (
                "No Strong Context Yet".to_string(),
                None,
                None,
                "very_low".to_string(),
                "0%".to_string(),
            ),
        };

    let final_result = FinalResultDebugInfo {
        decision: decision_str.to_string(),
        title: final_title,
        summary: unified_interp.summary.clone(),
        primary_context_id: final_ctx_id,
        primary_context_name: final_ctx_name,
        confidence_band: final_conf_band,
        confidence_percentage: final_conf_pct,
        no_context_reason: unified_interp.no_context_reason.map(|r| r.as_str().to_string()),
        no_context_explanation: unified_interp.no_context_reason.map(|r| r.explanation().to_string()),
    };

    Ok(ContextConfidenceDebugTrace {
        item_id: item_id.to_string(),
        item_title: item.title,
        item_type: item.item_type,
        candidates,
        competition,
        evidence_diversity,
        final_result,
        raw_evidence,
        normalized_evidence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use crate::context_engine::operations::create_context_signal;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_context_confidence_debugger_single_strong_context() {
        let conn = setup_test_db();
        let ws_port = crate::db::operations::create_workspace(&conn, "Portfolio").unwrap();
        let _ws_col = crate::db::operations::create_workspace(&conn, "College").unwrap();

        let item = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://portfolio.dev/project",
            Some("Project Showcase"),
            "{\"domain\":\"portfolio.dev\",\"app_name\":\"Google Chrome\"}",
        ).unwrap();

        // 1. Session signal matching Portfolio
        let _ = create_context_signal(
            &conn,
            Some(&ws_port.id),
            Some(&item.id),
            "active_app",
            None,
            "{\"app_name\":\"Google Chrome\"}",
        ).unwrap();

        let _ = create_context_signal(
            &conn,
            Some(&ws_port.id),
            Some(&item.id),
            "url_metadata",
            None,
            "{\"domain\":\"portfolio.dev\"}",
        ).unwrap();

        // 2. Confirmed workspace items for Portfolio
        let ws_item1 = crate::db::operations::create_item(&conn, Some(&ws_port.id), None, "url", "https://portfolio.dev/home", Some("Home"), "{\"domain\":\"portfolio.dev\"}").unwrap();
        let _ = create_context_signal(&conn, Some(&ws_port.id), Some(&ws_item1.id), "url_metadata", None, "{\"domain\":\"portfolio.dev\"}").unwrap();

        // 3. User action in history
        let hist_payload = serde_json::json!({"action": "moved_to_workspace", "destination_workspace": ws_port.id}).to_string();
        let _ = create_context_signal(&conn, None, Some(&item.id), "user_action", None, &hist_payload).unwrap();

        // Run Debugger
        let trace = debug_item_context_confidence(&conn, &item.id).unwrap();

        assert_eq!(trace.item_id, item.id);
        assert!(!trace.candidates.is_empty());
        let top = &trace.candidates[0];
        assert_eq!(top.context_name, "Portfolio");
        assert!(top.score_formatted > 0);
        assert!(!top.evidence_bullets.is_empty());

        // Check diversity
        assert!(trace.evidence_diversity.evidence_quantity >= 2);
        assert!(trace.evidence_diversity.independent_groups_count >= 1);

        // Check final result
        assert_eq!(trace.final_result.decision, "existing_context");
        assert!(trace.final_result.title.contains("Portfolio"));
    }

    #[test]
    fn test_context_confidence_debugger_competition_and_margin() {
        let conn = setup_test_db();
        let ws_port = crate::db::operations::create_workspace(&conn, "Portfolio").unwrap();
        let ws_col = crate::db::operations::create_workspace(&conn, "College").unwrap();

        let item = crate::db::operations::create_item(
            &conn,
            None,
            None,
            "text",
            "shared campus portfolio draft",
            Some("Draft"),
            "{}",
        ).unwrap();

        // Add matching signals for Portfolio
        let port_payload = serde_json::json!({"action": "moved_to_workspace", "destination_workspace": ws_port.id}).to_string();
        let _ = create_context_signal(&conn, None, Some(&item.id), "user_action", None, &port_payload).unwrap();

        // Add matching signals for College
        let col_payload = serde_json::json!({"action": "moved_to_workspace", "destination_workspace": ws_col.id}).to_string();
        let _ = create_context_signal(&conn, None, Some(&item.id), "user_action", None, &col_payload).unwrap();

        let trace = debug_item_context_confidence(&conn, &item.id).unwrap();

        assert!(trace.candidates.len() >= 2);
        assert!(trace.competition.is_some());
        let comp = trace.competition.unwrap();
        assert!(comp.top_score > 0);
        assert!(comp.runner_up_score > 0);
        assert!(comp.explanation.len() > 0);
    }

    #[test]
    fn test_context_confidence_debugger_contradiction_detection() {
        let conn = setup_test_db();
        let ws = crate::db::operations::create_workspace(&conn, "Contradicted WS").unwrap();
        let item = crate::db::operations::create_item(&conn, None, None, "text", "test content", None, "{}").unwrap();

        // History signal points to WS, but suggestion was dismissed
        let hist_payload = serde_json::json!({"action": "moved_to_workspace", "destination_workspace": ws.id}).to_string();
        let _ = create_context_signal(&conn, None, Some(&item.id), "user_action", None, &hist_payload).unwrap();

        let dismiss_payload = serde_json::json!({"action": "suggestion_dismissed", "workspace_id": ws.id}).to_string();
        let _ = create_context_signal(&conn, None, Some(&item.id), "suggestion_dismissed", None, &dismiss_payload).unwrap();

        let trace = debug_item_context_confidence(&conn, &item.id).unwrap();

        let cand = trace.candidates.iter().find(|c| c.context_id == ws.id).unwrap();
        assert!(!cand.contradiction_bullets.is_empty());
        assert!(cand.contradiction_bullets[0].contains("rejected") || cand.contradiction_bullets[0].contains("dismissed"));
    }

    #[test]
    fn test_context_confidence_debugger_no_context_diagnostic() {
        let conn = setup_test_db();
        let _ws = crate::db::operations::create_workspace(&conn, "Portfolio").unwrap();
        let item = crate::db::operations::create_item(&conn, None, None, "", "isolated text", None, "{}").unwrap();

        let trace = debug_item_context_confidence(&conn, &item.id).unwrap();

        assert_eq!(trace.final_result.decision, "no_context");
        assert_eq!(trace.final_result.title, "No Strong Context Yet");
        assert_eq!(trace.final_result.no_context_reason, Some("zero_evidence".to_string()));
    }
}

use crate::context_engine::scoring::score_item;
use crate::db::models::{InboxState, Item};
use crate::db::operations::{get_inbox_item_state, get_item, get_workspace};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// Attention level required from the user for an Inbox item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionLevel {
    None,
    Low,
    Medium,
    High,
}

impl AttentionLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            AttentionLevel::None => "none",
            AttentionLevel::Low => "low",
            AttentionLevel::Medium => "medium",
            AttentionLevel::High => "high",
        }
    }
}

impl std::fmt::Display for AttentionLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Breakdown of factors contributing to the attention decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttentionFactors {
    pub is_organized: bool,
    pub is_dismissed: bool,
    pub contextual_confidence: f32,
    pub is_ambiguous: bool,
    pub ambiguity_margin: Option<f32>,
    pub has_new_candidate: bool,
    pub candidate_id: Option<String>,
    pub candidate_confidence: Option<f32>,
    pub has_suggested_membership: bool,
    pub suggested_workspace_id: Option<String>,
    pub suggested_workspace_name: Option<String>,
    pub dismissal_count: usize,
    pub similar_dismissals_count: usize,
    pub item_age_hours: f64,
    pub historical_rejection_penalty: f32,
}

/// Full Attention calculation result for an Inbox Item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemAttention {
    pub item_id: String,
    pub attention_required: bool,
    pub attention_level: AttentionLevel,
    pub reason: String,
    pub score: f32,
    pub factors: AttentionFactors,
}

/// Configurable thresholds and weights for the Attention Engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttentionConfig {
    pub high_attention_threshold: f32,
    pub medium_attention_threshold: f32,
    pub low_attention_threshold: f32,
    pub ambiguity_threshold: f32,
    pub candidate_bonus: f32,
    pub suggested_membership_bonus: f32,
    pub ambiguity_bonus: f32,
    pub dismissal_penalty_per_count: f32,
    pub similar_dismissal_penalty: f32,
    pub fresh_item_window_hours: f64,
    pub stale_item_threshold_days: f64,
}

impl Default for AttentionConfig {
    fn default() -> Self {
        Self {
            high_attention_threshold: 0.75,
            medium_attention_threshold: 0.45,
            low_attention_threshold: 0.20,
            ambiguity_threshold: 0.15,
            candidate_bonus: 0.35,
            suggested_membership_bonus: 0.30,
            ambiguity_bonus: 0.30,
            dismissal_penalty_per_count: 0.25,
            similar_dismissal_penalty: 0.10,
            fresh_item_window_hours: 24.0,
            stale_item_threshold_days: 7.0,
        }
    }
}

/// Evaluates whether an Inbox Item requires user attention.
/// Pure, deterministic, read-only calculation. Does NOT modify Items or Workspaces.
pub fn evaluate_inbox_item_attention(
    conn: &Connection,
    item_id: &str,
    config_opt: Option<&AttentionConfig>,
) -> Result<ItemAttention, String> {
    let default_config = AttentionConfig::default();
    let config = config_opt.unwrap_or(&default_config);

    // 1. Fetch Item
    let item = get_item(conn, item_id).map_err(|e| format!("Failed to fetch item: {}", e))?;

    // 2. Check if already organized in a workspace
    let is_physically_organized = item
        .workspace_id
        .as_deref()
        .map_or(false, |w| !w.is_empty() && w != "inbox");

    let inbox_state = get_inbox_item_state(conn, item_id).ok().flatten();
    let is_state_organized = inbox_state
        .as_ref()
        .map_or(false, |s| s.state == InboxState::Organized);
    let is_state_dismissed = inbox_state
        .as_ref()
        .map_or(false, |s| s.state == InboxState::Dismissed);

    if is_physically_organized || is_state_organized {
        let ws_name = if let Some(ws_id) = &item.workspace_id {
            get_workspace(conn, ws_id).map(|w| w.name).unwrap_or_else(|_| ws_id.clone())
        } else {
            "workspace".to_string()
        };

        return Ok(ItemAttention {
            item_id: item_id.to_string(),
            attention_required: false,
            attention_level: AttentionLevel::None,
            reason: format!("Item is already organized in '{}'", ws_name),
            score: 0.0,
            factors: AttentionFactors {
                is_organized: true,
                is_dismissed: false,
                contextual_confidence: 0.0,
                is_ambiguous: false,
                ambiguity_margin: None,
                has_new_candidate: false,
                candidate_id: None,
                candidate_confidence: None,
                has_suggested_membership: false,
                suggested_workspace_id: None,
                suggested_workspace_name: None,
                dismissal_count: 0,
                similar_dismissals_count: 0,
                item_age_hours: compute_item_age_hours(&item),
                historical_rejection_penalty: 0.0,
            },
        });
    }

    if is_state_dismissed {
        return Ok(ItemAttention {
            item_id: item_id.to_string(),
            attention_required: false,
            attention_level: AttentionLevel::None,
            reason: "Item was dismissed by the user".to_string(),
            score: 0.0,
            factors: AttentionFactors {
                is_organized: false,
                is_dismissed: true,
                contextual_confidence: 0.0,
                is_ambiguous: false,
                ambiguity_margin: None,
                has_new_candidate: false,
                candidate_id: None,
                candidate_confidence: None,
                has_suggested_membership: false,
                suggested_workspace_id: None,
                suggested_workspace_name: None,
                dismissal_count: 0,
                similar_dismissals_count: 0,
                item_age_hours: compute_item_age_hours(&item),
                historical_rejection_penalty: 0.0,
            },
        });
    }

    // 3. Compute Contextual Confidence & Ambiguity from workspace scoring
    let workspace_scores = score_item(conn, item_id).unwrap_or_default();
    let top_1 = workspace_scores.get(0);
    let top_2 = workspace_scores.get(1);

    let contextual_confidence = top_1.map_or(0.0, |s| s.confidence);
    let mut is_ambiguous = false;
    let mut ambiguity_margin = None;
    let mut top_1_ws_name = None;
    let mut top_2_ws_name = None;

    if let (Some(s1), Some(s2)) = (top_1, top_2) {
        if s1.confidence >= 0.10 && s2.confidence >= 0.10 {
            let margin = (s1.confidence - s2.confidence).abs();
            if margin <= config.ambiguity_threshold {
                is_ambiguous = true;
                ambiguity_margin = Some(margin);
                top_1_ws_name = get_workspace(conn, &s1.workspace_id).ok().map(|w| w.name);
                top_2_ws_name = get_workspace(conn, &s2.workspace_id).ok().map(|w| w.name);
            }
        }
    }

    // 4. Check for Suggested Context Memberships
    let mut has_suggested_membership = false;
    let mut suggested_workspace_id = None;
    let mut suggested_workspace_name = None;
    let mut suggested_mem_confidence = 0.0f32;

    if let Ok(mut stmt) = conn.prepare(
        "SELECT workspace_id, confidence FROM context_memberships 
         WHERE item_id = ?1 AND membership_type = 'suggested' 
         ORDER BY confidence DESC LIMIT 1",
    ) {
        if let Ok(mut rows) = stmt.query(params![item_id]) {
            if let Ok(Some(row)) = rows.next() {
                if let (Ok(ws_id), Ok(conf)) = (row.get::<_, String>(0), row.get::<_, f64>(1)) {
                    has_suggested_membership = true;
                    suggested_mem_confidence = conf as f32;
                    suggested_workspace_name = get_workspace(conn, &ws_id).ok().map(|w| w.name);
                    suggested_workspace_id = Some(ws_id);
                }
            }
        }
    }

    // 5. Check for New Context Candidates
    let mut has_new_candidate = false;
    let mut candidate_id = None;
    let mut candidate_confidence = None;
    let mut candidate_label = None;

    if let Ok(mut stmt) = conn.prepare(
        "SELECT c.id, c.confidence, c.label 
         FROM context_candidates c 
         JOIN context_candidate_items ci ON c.id = ci.candidate_id 
         WHERE ci.item_id = ?1 AND c.status IN ('detected', 'suggested') 
         ORDER BY c.confidence DESC LIMIT 1",
    ) {
        if let Ok(mut rows) = stmt.query(params![item_id]) {
            if let Ok(Some(row)) = rows.next() {
                if let (Ok(cid), Ok(conf)) = (row.get::<_, String>(0), row.get::<_, f64>(1)) {
                    has_new_candidate = true;
                    candidate_id = Some(cid);
                    candidate_confidence = Some(conf as f32);
                    candidate_label = row.get::<_, Option<String>>(2).ok().flatten();
                }
            }
        }
    }

    // 6. User Dismissal History
    let direct_dismissals: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM context_signals WHERE item_id = ?1 AND signal_type = 'membership_dismissed'",
            params![item_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // Similar dismissals across matching app / domain
    let mut similar_dismissals: usize = 0;
    if let Ok(meta_val) = serde_json::from_str::<serde_json::Value>(&item.metadata) {
        let app_name = meta_val.get("source_application").and_then(|v| v.as_str());
        let domain = meta_val.get("domain").and_then(|v| v.as_str());

        if let Some(app) = app_name {
            let app_pattern = format!("%\"source_application\":\"{}\"%", app);
            let count: usize = conn
                .query_row(
                    "SELECT COUNT(*) FROM context_signals WHERE signal_type = 'membership_dismissed' AND payload LIKE ?1",
                    params![app_pattern],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            similar_dismissals = similar_dismissals.max(count);
        }

        if let Some(dom) = domain {
            let dom_pattern = format!("%\"domain\":\"{}\"%", dom);
            let count: usize = conn
                .query_row(
                    "SELECT COUNT(*) FROM context_signals WHERE signal_type = 'membership_dismissed' AND payload LIKE ?1",
                    params![dom_pattern],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            similar_dismissals = similar_dismissals.max(count);
        }
    }

    let item_age_hours = compute_item_age_hours(&item);

    // 7. Calculate Deterministic Score
    let cand_conf_f32 = candidate_confidence.unwrap_or(0.0);
    let mut score = contextual_confidence
        .max(cand_conf_f32)
        .max(suggested_mem_confidence);

    if has_new_candidate {
        score += config.candidate_bonus;
    }
    if has_suggested_membership {
        score += config.suggested_membership_bonus;
    }
    if is_ambiguous {
        score += config.ambiguity_bonus;
    }

    // Dismissal penalty damping
    let direct_penalty = direct_dismissals as f32 * config.dismissal_penalty_per_count;
    let extra_similar = similar_dismissals.saturating_sub(direct_dismissals);
    let similar_penalty = extra_similar as f32 * config.similar_dismissal_penalty;
    let total_rejection_penalty = (direct_penalty + similar_penalty).min(0.60);
    score -= total_rejection_penalty;

    // Recency / Stale weighting
    if item_age_hours <= config.fresh_item_window_hours {
        score *= 1.10;
    } else if item_age_hours >= (config.stale_item_threshold_days * 24.0) {
        score *= 0.75;
    }

    let final_score = score.clamp(0.0, 1.0);

    // 8. Determine Attention Level & Rationale
    let (level, attention_required, reason) = if final_score >= config.high_attention_threshold {
        let r = if has_new_candidate {
            let label_str = candidate_label.as_deref().unwrap_or("New Group");
            format!(
                "New context candidate '{}' detected ({:.0}% confidence). Review candidate to organize items.",
                label_str,
                cand_conf_f32 * 100.0
            )
        } else if has_suggested_membership {
            let ws_name = suggested_workspace_name
                .as_deref()
                .unwrap_or("matched workspace");
            format!(
                "Strong suggestion for workspace '{}' ({:.0}% confidence). Ready to file.",
                ws_name,
                suggested_mem_confidence * 100.0
            )
        } else if is_ambiguous {
            let w1 = top_1_ws_name.as_deref().unwrap_or("Workspace A");
            let w2 = top_2_ws_name.as_deref().unwrap_or("Workspace B");
            format!(
                "High confidence ambiguity between '{}' and '{}'. User decision needed.",
                w1, w2
            )
        } else {
            format!(
                "High contextual affinity ({:.0}% confidence).",
                contextual_confidence * 100.0
            )
        };
        (AttentionLevel::High, true, r)
    } else if final_score >= config.medium_attention_threshold || (is_ambiguous && final_score >= config.low_attention_threshold) {
        let r = if is_ambiguous {
            let w1 = top_1_ws_name.as_deref().unwrap_or("Workspace A");
            let w2 = top_2_ws_name.as_deref().unwrap_or("Workspace B");
            format!(
                "Ambiguous match between '{}' and '{}' ({:.0}% vs {:.0}%). Review needed.",
                w1,
                w2,
                top_1.map_or(0.0, |s| s.confidence) * 100.0,
                top_2.map_or(0.0, |s| s.confidence) * 100.0
            )
        } else if has_new_candidate {
            "Context candidate cluster detected for review.".to_string()
        } else if has_suggested_membership {
            let ws_name = suggested_workspace_name
                .as_deref()
                .unwrap_or("target workspace");
            format!(
                "Suggested match with workspace '{}' ({:.0}% confidence).",
                ws_name,
                suggested_mem_confidence * 100.0
            )
        } else {
            format!(
                "Moderate contextual match ({:.0}% confidence).",
                contextual_confidence * 100.0
            )
        };
        (AttentionLevel::Medium, true, r)
    } else if final_score >= config.low_attention_threshold {
        let r = if total_rejection_penalty > 0.0 {
            "Low attention due to user dismissal history for similar items.".to_string()
        } else if item_age_hours >= (config.stale_item_threshold_days * 24.0) {
            "Low attention: Stale item without strong contextual affinities.".to_string()
        } else {
            format!(
                "Low contextual confidence ({:.0}%).",
                contextual_confidence * 100.0
            )
        };
        (AttentionLevel::Low, false, r)
    } else {
        let r = if total_rejection_penalty > 0.0 {
            "Attention suppressed due to prior dismissals.".to_string()
        } else {
            "No immediate contextual attention required.".to_string()
        };
        (AttentionLevel::None, false, r)
    };

    Ok(ItemAttention {
        item_id: item_id.to_string(),
        attention_required,
        attention_level: level,
        reason,
        score: final_score,
        factors: AttentionFactors {
            is_organized: false,
            is_dismissed: false,
            contextual_confidence,
            is_ambiguous,
            ambiguity_margin,
            has_new_candidate,
            candidate_id,
            candidate_confidence,
            has_suggested_membership,
            suggested_workspace_id,
            suggested_workspace_name,
            dismissal_count: direct_dismissals,
            similar_dismissals_count: similar_dismissals,
            item_age_hours,
            historical_rejection_penalty: total_rejection_penalty,
        },
    })
}

/// Evaluates attention for all active Inbox items.
pub fn evaluate_inbox_attention_batch(
    conn: &Connection,
    config: Option<&AttentionConfig>,
) -> Result<Vec<ItemAttention>, String> {
    let mut stmt = conn
        .prepare("SELECT id FROM items WHERE workspace_id IS NULL OR workspace_id = '' OR workspace_id = 'inbox' ORDER BY created_at DESC")
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;

    let mut results = Vec::new();
    for item_id_res in rows {
        if let Ok(item_id) = item_id_res {
            if let Ok(attention) = evaluate_inbox_item_attention(conn, &item_id, config) {
                results.push(attention);
            }
        }
    }

    Ok(results)
}

fn compute_item_age_hours(item: &Item) -> f64 {
    if let Ok(dt) = DateTime::parse_from_rfc3339(&item.created_at) {
        let age_secs = (Utc::now() - dt.with_timezone(&Utc)).num_seconds();
        (age_secs as f64 / 3600.0).max(0.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::operations::{
        create_item, create_or_update_membership, create_workspace, dismiss_membership,
    };
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_organized_item_requires_no_attention() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Design System").unwrap();
        let item = create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Button primary spec",
            Some("Button"),
            "{}",
        )
        .unwrap();

        let attention = evaluate_inbox_item_attention(&conn, &item.id, None).unwrap();
        assert!(!attention.attention_required);
        assert_eq!(attention.attention_level, AttentionLevel::None);
        assert!(attention.factors.is_organized);
        assert!(attention.reason.contains("already organized"));
    }

    #[test]
    fn test_dismissed_item_requires_no_attention() {
        let conn = setup_test_db();
        let item = create_item(&conn, None, None, "text", "Dismissed item", None, "{}").unwrap();

        // Dismiss item
        crate::context_engine::inbox_state::InboxStateMachine::dismiss_item(&conn, &item.id, None)
            .unwrap();

        let attention = evaluate_inbox_item_attention(&conn, &item.id, None).unwrap();
        assert!(!attention.attention_required);
        assert_eq!(attention.attention_level, AttentionLevel::None);
        assert!(attention.factors.is_dismissed);
        assert!(attention.reason.contains("dismissed"));
    }

    #[test]
    fn test_suggested_membership_elevates_to_high_attention() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Trip to Japan").unwrap();
        let item = create_item(
            &conn,
            None,
            None,
            "text",
            "Tokyo hotel booking",
            Some("Hotel"),
            "{}",
        )
        .unwrap();

        // Create high-confidence suggested membership
        create_or_update_membership(&conn, &item.id, &ws.id, "suggested", 0.90, "context_engine")
            .unwrap();

        let attention = evaluate_inbox_item_attention(&conn, &item.id, None).unwrap();
        assert!(attention.attention_required);
        assert_eq!(attention.attention_level, AttentionLevel::High);
        assert!(attention.factors.has_suggested_membership);
        assert_eq!(attention.factors.suggested_workspace_id, Some(ws.id));
        assert!(attention.reason.contains("Trip to Japan"));
    }

    #[test]
    fn test_new_context_candidate_elevates_attention() {
        let conn = setup_test_db();
        let item = create_item(
            &conn,
            None,
            None,
            "text",
            "Financial statements Q3",
            Some("Finances"),
            "{}",
        )
        .unwrap();

        // Insert context candidate
        let cand_id = "cand-finance-1";
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO context_candidates (id, status, confidence, label, metadata, created_at, updated_at)
             VALUES (?1, 'detected', 0.88, 'Q3 Finance Project', '{}', ?2, ?2)",
            params![cand_id, now],
        ).unwrap();

        conn.execute(
            "INSERT INTO context_candidate_items (candidate_id, item_id, relevance_score, added_at)
             VALUES (?1, ?2, 0.95, ?3)",
            params![cand_id, item.id, now],
        ).unwrap();

        let attention = evaluate_inbox_item_attention(&conn, &item.id, None).unwrap();
        assert!(attention.attention_required);
        assert!(
            attention.attention_level == AttentionLevel::High
                || attention.attention_level == AttentionLevel::Medium
        );
        assert!(attention.factors.has_new_candidate);
        assert_eq!(attention.factors.candidate_id, Some(cand_id.to_string()));
        assert!(attention.reason.contains("Q3 Finance Project"));
    }

    #[test]
    fn test_ambiguity_between_contexts_boosts_attention() {
        let conn = setup_test_db();
        let ws_a = create_workspace(&conn, "Alpha").unwrap();
        let ws_b = create_workspace(&conn, "Beta").unwrap();

        // Populate items in Alpha and Beta with matching application metadata
        let item_meta = r#"{"source_application":"Figma","domain":"figma.com"}"#;
        let item_a = create_item(&conn, Some(&ws_a.id), None, "url", "https://figma.com/file1", None, item_meta).unwrap();
        let item_b = create_item(&conn, Some(&ws_b.id), None, "url", "https://figma.com/file2", None, item_meta).unwrap();

        // Create new inbox item matching both with close relationship scores
        let target = create_item(&conn, None, None, "url", "https://figma.com/file3", None, item_meta).unwrap();
        crate::db::operations::create_item_relationship(&conn, &target.id, &item_a.id, "semantic_similarity", 0.85, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &target.id, &item_b.id, "semantic_similarity", 0.82, None).unwrap();

        let attention = evaluate_inbox_item_attention(&conn, &target.id, None).unwrap();
        assert!(attention.attention_required);
        assert!(attention.factors.is_ambiguous);
        assert!(attention.reason.contains("Ambiguous") || attention.reason.contains("ambiguity"));
    }

    #[test]
    fn test_user_dismissal_history_suppresses_attention() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Marketing").unwrap();
        let item = create_item(
            &conn,
            None,
            None,
            "text",
            "Ad creative draft",
            Some("Ad"),
            r#"{"source_application":"Canva","domain":"canva.com"}"#,
        )
        .unwrap();

        // Initially create a suggested membership
        create_or_update_membership(&conn, &item.id, &ws.id, "suggested", 0.60, "context_engine")
            .unwrap();

        // User dismisses membership -> records dismissal signal
        dismiss_membership(&conn, &item.id, &ws.id).unwrap();

        let attention = evaluate_inbox_item_attention(&conn, &item.id, None).unwrap();
        assert!(attention.factors.dismissal_count >= 1);
        assert!(attention.factors.historical_rejection_penalty > 0.0);
        // After dismissal, attention should be suppressed
        assert!(
            attention.attention_level == AttentionLevel::Low
                || attention.attention_level == AttentionLevel::None
        );
    }

    #[test]
    fn test_attention_calculation_is_deterministic() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Deterministic Test").unwrap();
        let item = create_item(
            &conn,
            None,
            None,
            "text",
            "Static reproducible payload",
            Some("Static"),
            "{}",
        )
        .unwrap();

        create_or_update_membership(&conn, &item.id, &ws.id, "suggested", 0.85, "context_engine")
            .unwrap();

        let res1 = evaluate_inbox_item_attention(&conn, &item.id, None).unwrap();
        let res2 = evaluate_inbox_item_attention(&conn, &item.id, None).unwrap();

        assert_eq!(res1.score, res2.score);
        assert_eq!(res1.attention_level, res2.attention_level);
        assert_eq!(res1.attention_required, res2.attention_required);
        assert_eq!(res1.reason, res2.reason);
    }

    #[test]
    fn test_attention_engine_does_not_modify_database() {
        let conn = setup_test_db();
        let item = create_item(&conn, None, None, "text", "Unmodified test", None, "{}").unwrap();

        let initial_item = get_item(&conn, &item.id).unwrap();
        let _ = evaluate_inbox_item_attention(&conn, &item.id, None).unwrap();
        let after_item = get_item(&conn, &item.id).unwrap();

        assert_eq!(initial_item.updated_at, after_item.updated_at);
        assert_eq!(initial_item.workspace_id, after_item.workspace_id);
    }

    #[test]
    fn test_inbox_attention_batch_evaluation() {
        let conn = setup_test_db();
        let _item1 = create_item(&conn, None, None, "text", "Item 1", None, "{}").unwrap();
        let _item2 = create_item(&conn, None, None, "text", "Item 2", None, "{}").unwrap();
        let ws = create_workspace(&conn, "Filed WS").unwrap();
        let item3 = create_item(&conn, Some(&ws.id), None, "text", "Item 3", None, "{}").unwrap();

        let batch = evaluate_inbox_attention_batch(&conn, None).unwrap();
        // Only inbox items (item1, item2) are returned by batch query
        assert_eq!(batch.len(), 2);
        assert!(batch.iter().all(|a| a.item_id != item3.id));
    }
}

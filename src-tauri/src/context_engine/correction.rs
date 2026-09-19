use chrono::Utc;
use rusqlite::{params, Connection, Result as SqlResult};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

use crate::context_engine::contamination::extract_topic_tokens;
use crate::context_engine::memory::invalidate_memory_cache;
use crate::context_engine::reconstruction::invalidate_reconstruction_cache;
use crate::context_engine::snapshot::invalidate_snapshot_cache;
use crate::context_engine::workset::invalidate_workset_cache;
use crate::db::models::Item;
use crate::db::operations::{create_or_update_membership, get_item};

// =========================================================================
// Correction Evidence Signature & Record (Step 9)
// =========================================================================

/// Evidence signature extracted from a corrected item, used to detect and prevent
/// the same weak pattern from immediately reattaching to the corrected workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CorrectionEvidenceSignature {
    pub domain: Option<String>,
    pub app_name: Option<String>,
    pub title_keywords: Vec<String>,
    pub item_type: String,
    pub url: Option<String>,
}

impl CorrectionEvidenceSignature {
    pub fn from_item(item: &Item) -> Self {
        let mut domain = None;
        let mut app_name = None;
        let mut url = None;

        // Extract metadata attributes
        if let Ok(meta_json) = serde_json::from_str::<serde_json::Value>(&item.metadata) {
            if let Some(d) = meta_json.get("domain").and_then(|v| v.as_str()) {
                domain = Some(d.to_lowercase());
            }
            if let Some(app) = meta_json.get("app_name").and_then(|v| v.as_str()) {
                app_name = Some(app.to_string());
            }
            if let Some(u) = meta_json.get("url").and_then(|v| v.as_str()) {
                url = Some(u.to_string());
                if domain.is_none() {
                    domain = extract_domain_from_url_str(u);
                }
            }
        }

        // Extract domain from content if url type
        if item.item_type == "url" && domain.is_none() {
            domain = extract_domain_from_url_str(&item.content);
            if url.is_none() {
                url = Some(item.content.clone());
            }
        }

        // Tokenize title keywords
        let title_keywords = if let Some(ref title) = item.title {
            extract_topic_tokens(title)
        } else {
            extract_topic_tokens(&item.content)
        };

        Self {
            domain,
            app_name,
            title_keywords,
            item_type: item.item_type.clone(),
            url,
        }
    }

    /// Computes pattern similarity score between this signature and another item [0.0, 1.0]
    pub fn pattern_similarity(&self, other: &CorrectionEvidenceSignature) -> f64 {
        let mut score = 0.0;
        let mut total_weight = 0.0;

        // 1. Domain match (weight 0.45)
        if self.domain.is_some() && self.domain == other.domain {
            score += 0.45;
        }
        if self.domain.is_some() || other.domain.is_some() {
            total_weight += 0.45;
        }

        // 2. App name match (weight 0.20)
        if self.app_name.is_some() && self.app_name == other.app_name {
            score += 0.20;
        }
        if self.app_name.is_some() || other.app_name.is_some() {
            total_weight += 0.20;
        }

        // 3. Title keyword overlap (weight 0.35)
        if !self.title_keywords.is_empty() && !other.title_keywords.is_empty() {
            let set_a: HashSet<&String> = self.title_keywords.iter().collect();
            let set_b: HashSet<&String> = other.title_keywords.iter().collect();
            let intersection = set_a.intersection(&set_b).count();
            let union = set_a.union(&set_b).count();
            if union > 0 {
                let jaccard = intersection as f64 / union as f64;
                score += jaccard * 0.35;
            }
            total_weight += 0.35;
        }

        if total_weight > 0.0 {
            score / total_weight
        } else {
            0.0
        }
    }
}

fn extract_domain_from_url_str(url: &str) -> Option<String> {
    let s = url.trim();
    let without_proto = if let Some(stripped) = s.strip_prefix("https://") {
        stripped
    } else if let Some(stripped) = s.strip_prefix("http://") {
        stripped
    } else {
        return None;
    };
    let host = without_proto.split('/').next()?.split(':').next()?;
    let clean = host.strip_prefix("www.").unwrap_or(host).trim();
    if clean.is_empty() || clean == "localhost" || clean == "127.0.0.1" {
        None
    } else {
        Some(clean.to_lowercase())
    }
}

/// Structured record for user correction feedback.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CorrectionFeedbackRecord {
    pub id: String,
    pub item_id: String,
    pub workspace_id: String,
    pub action: String,
    pub evidence_signature: CorrectionEvidenceSignature,
    pub reason: Option<String>,
    pub created_at: String,
    pub is_active: bool,
}

/// Verdict returned when checking an incoming activity against correction history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CorrectionDefenseVerdict {
    /// No matching correction feedback; standard evaluation proceeds.
    Allowed,
    /// Matching negative correction found for this workspace; reattachment is blocked.
    BlockedByCorrection {
        reason: String,
        pattern_similarity: f64,
        dismissed_item_id: String,
    },
    /// Negative correction existed, but future stronger evidence legitimately overrides it.
    OverridePermitted {
        reason: String,
        anchor_type: String,
        confidence: f64,
    },
}

impl CorrectionDefenseVerdict {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed | Self::OverridePermitted { .. })
    }

    pub fn is_blocked(&self) -> bool {
        matches!(self, Self::BlockedByCorrection { .. })
    }
}

// =========================================================================
// Context Correction Service
// =========================================================================

/// Core service managing user contextual corrections and negative feedback learning.
pub struct ContextCorrectionService;

impl ContextCorrectionService {
    /// Executes a full contextual correction when a user removes an item from a workspace.
    ///
    /// Invariants:
    /// 1. **Reduces/Removes association**: Updates `context_memberships` to `dismissed` (conf: 0.0).
    /// 2. **Physical unassign**: If `item.workspace_id == workspace_id`, sets it to `None`.
    /// 3. **Preserves item**: The item in `items` table is **NEVER** deleted.
    /// 4. **Records scoped signal**: Records `user_correction` signal with evidence signature in `context_signals`.
    /// 5. **Invalidates caches**: Refreshes workset, memory, snapshot, and reconstruction caches.
    /// 6. **Scoped feedback**: Scoped strictly to `(item_id, workspace_id)`; does NOT punish other workspaces.
    pub fn record_context_correction(
        conn: &Connection,
        item_id: &str,
        workspace_id: &str,
        reason: Option<&str>,
    ) -> Result<CorrectionFeedbackRecord, String> {
        let now = Utc::now().to_rfc3339();
        let signal_id = Uuid::new_v4().to_string();

        // 1. Fetch item to extract signature and verify existence
        let item = get_item(conn, item_id).map_err(|e| format!("Item '{}' not found: {}", item_id, e))?;
        let signature = CorrectionEvidenceSignature::from_item(&item);

        // 2. Demote membership to 'dismissed' (confidence 0.0, source 'user')
        create_or_update_membership(conn, item_id, workspace_id, "dismissed", 0.0, "user")
            .map_err(|e| format!("Failed to update membership to dismissed: {}", e))?;

        // 3. Clear physical workspace_id if filed to this workspace (preserving the item in DB)
        if item.workspace_id.as_deref() == Some(workspace_id) {
            conn.execute(
                "UPDATE items SET workspace_id = NULL, updated_at = ?1 WHERE id = ?2",
                params![now, item_id],
            )
            .map_err(|e| format!("Failed to unassign workspace_id on item: {}", e))?;
        }

        // 4. Record negative contextual feedback signal with evidence signature
        let payload = serde_json::json!({
            "action": "user_removed_from_context",
            "item_id": item_id,
            "workspace_id": workspace_id,
            "evidence_signature": signature,
            "reason": reason.unwrap_or("User explicitly removed item from workspace context"),
            "scope": "workspace_scoped",
            "recorded_at": now,
        })
        .to_string();

        conn.execute(
            "INSERT INTO context_signals (id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'user_correction', NULL, ?4, ?5, ?5)",
            params![signal_id, workspace_id, item_id, payload, now],
        )
        .map_err(|e| format!("Failed to insert user_correction signal: {}", e))?;

        // 5. Invalidate downstream caches
        invalidate_workset_cache(Some(workspace_id));
        invalidate_memory_cache(Some(workspace_id));
        invalidate_snapshot_cache(workspace_id);
        invalidate_reconstruction_cache(Some(workspace_id));

        Ok(CorrectionFeedbackRecord {
            id: signal_id,
            item_id: item_id.to_string(),
            workspace_id: workspace_id.to_string(),
            action: "user_removed_from_context".to_string(),
            evidence_signature: signature,
            reason: reason.map(|r| r.to_string()),
            created_at: now,
            is_active: true,
        })
    }

    /// Evaluates whether an incoming candidate activity/item is blocked by prior correction feedback
    /// or if future stronger evidence legitimately overrides it.
    ///
    /// Scoping Invariant: Only checks corrections recorded against `target_workspace_id`.
    /// Unrelated workspaces are completely unaffected.
    pub fn evaluate_correction_defense(
        conn: &Connection,
        item: &Item,
        target_workspace_id: &str,
        candidate_confidence: f64,
        has_strong_anchor: bool,
        is_explicit_user_action: bool,
    ) -> CorrectionDefenseVerdict {
        // 1. Authoritative explicit user action overrides prior dismissals immediately
        if is_explicit_user_action {
            return CorrectionDefenseVerdict::OverridePermitted {
                reason: "Explicit user action overrides prior correction feedback".to_string(),
                anchor_type: "explicit_user_action".to_string(),
                confidence: 1.0,
            };
        }

        // 2. Check if this exact item was directly dismissed from this workspace
        let is_directly_dismissed: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM context_memberships
                 WHERE item_id = ?1 AND workspace_id = ?2 AND membership_type = 'dismissed'",
                params![item.id, target_workspace_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|cnt| cnt > 0)
            .unwrap_or(false);

        if is_directly_dismissed {
            // Check if strong anchor + high confidence allows override
            if has_strong_anchor && candidate_confidence >= 0.85 {
                return CorrectionDefenseVerdict::OverridePermitted {
                    reason: format!(
                        "Strong corroborated anchor (confidence {:.1}%) overrides prior direct dismissal",
                        candidate_confidence * 100.0
                    ),
                    anchor_type: "strong_corroborated_anchor".to_string(),
                    confidence: candidate_confidence,
                };
            }

            return CorrectionDefenseVerdict::BlockedByCorrection {
                reason: "Item was previously dismissed by user from this workspace".to_string(),
                pattern_similarity: 1.0,
                dismissed_item_id: item.id.clone(),
            };
        }

        // 3. Check pattern dismissals for this workspace in context_signals
        let item_signature = CorrectionEvidenceSignature::from_item(item);

        let mut stmt = match conn.prepare(
            "SELECT id, item_id, payload FROM context_signals
             WHERE workspace_id = ?1 AND (signal_type = 'user_correction' OR signal_type = 'membership_dismissed' OR signal_type = 'user_dismiss')
             ORDER BY created_at DESC LIMIT 30",
        ) {
            Ok(s) => s,
            Err(_) => return CorrectionDefenseVerdict::Allowed,
        };

        let rows = stmt
            .query_map(params![target_workspace_id], |row| {
                let id: String = row.get(0)?;
                let dismissed_id: Option<String> = row.get(1)?;
                let payload: String = row.get(2)?;
                Ok((id, dismissed_id, payload))
            })
            .ok();

        if let Some(rows) = rows {
            for row in rows.flatten() {
                let (_, dismissed_id, payload_str) = row;
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload_str) {
                    if let Some(sig_val) = val.get("evidence_signature") {
                        if let Ok(dismissed_sig) =
                            serde_json::from_value::<CorrectionEvidenceSignature>(sig_val.clone())
                        {
                            let sim = item_signature.pattern_similarity(&dismissed_sig);
                            if sim >= 0.70 {
                                // Matching pattern found!
                                if has_strong_anchor && candidate_confidence >= 0.85 {
                                    return CorrectionDefenseVerdict::OverridePermitted {
                                        reason: format!(
                                            "Strong corroborated anchor (confidence {:.1}%) overrides matching dismissed pattern (sim {:.2})",
                                            candidate_confidence * 100.0,
                                            sim
                                        ),
                                        anchor_type: "strong_corroborated_anchor".to_string(),
                                        confidence: candidate_confidence,
                                    };
                                }

                                return CorrectionDefenseVerdict::BlockedByCorrection {
                                    reason: format!(
                                        "Matches previously corrected evidence pattern (similarity {:.1}%) in workspace '{}'",
                                        sim * 100.0,
                                        target_workspace_id
                                    ),
                                    pattern_similarity: sim,
                                    dismissed_item_id: dismissed_id.unwrap_or_default(),
                                };
                            }
                        }
                    }
                }
            }
        }

        CorrectionDefenseVerdict::Allowed
    }

    /// Verifies that an item is preserved in the database (invariant verification).
    pub fn is_item_preserved_in_db(conn: &Connection, item_id: &str) -> bool {
        conn.query_row(
            "SELECT COUNT(*) FROM items WHERE id = ?1",
            params![item_id],
            |r| r.get::<_, i64>(0),
        )
        .map(|cnt| cnt > 0)
        .unwrap_or(false)
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::operations::{create_item, create_or_update_membership, create_workspace};

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_correction_removes_association_and_preserves_item() {
        let conn = setup_test_db();
        let ws_payx = create_workspace(&conn, "PayX").unwrap();

        // Create random article initially associated with PayX
        let item = create_item(
            &conn,
            Some(&ws_payx.id),
            None,
            "url",
            "https://medium.com/random-tech/cool-gadgets",
            Some("10 Cool Gadgets of 2026"),
            "{\"domain\":\"medium.com\",\"app_name\":\"Google Chrome\"}",
        )
        .unwrap();

        create_or_update_membership(&conn, &item.id, &ws_payx.id, "confirmed", 0.9, "test").unwrap();

        // User removes item from PayX
        let record = ContextCorrectionService::record_context_correction(
            &conn,
            &item.id,
            &ws_payx.id,
            Some("Not related to PayX project"),
        )
        .unwrap();

        assert_eq!(record.action, "user_removed_from_context");
        assert_eq!(record.evidence_signature.domain.as_deref(), Some("medium.com"));

        // 1. Verify membership is demoted to 'dismissed' with 0.0 confidence
        let mem: ContextMembership = conn
            .query_row(
                "SELECT id, item_id, workspace_id, membership_type, confidence, source, created_at, updated_at
                 FROM context_memberships WHERE item_id = ?1 AND workspace_id = ?2",
                params![item.id, ws_payx.id],
                |r| {
                    Ok(ContextMembership {
                        id: r.get(0)?,
                        item_id: r.get(1)?,
                        workspace_id: r.get(2)?,
                        membership_type: r.get(3)?,
                        confidence: r.get(4)?,
                        source: r.get(5)?,
                        created_at: r.get(6)?,
                        updated_at: r.get(7)?,
                    })
                },
            )
            .unwrap();

        assert_eq!(mem.membership_type, "dismissed");
        assert_eq!(mem.confidence, 0.0);

        // 2. Verify physical workspace_id is cleared
        let fetched_item = get_item(&conn, &item.id).unwrap();
        assert!(fetched_item.workspace_id.is_none());

        // 3. Verify original item is PRESERVED in database
        assert!(ContextCorrectionService::is_item_preserved_in_db(&conn, &item.id));
    }

    #[test]
    fn test_negative_feedback_prevents_same_pattern_from_reattaching() {
        let conn = setup_test_db();
        let ws_payx = create_workspace(&conn, "PayX").unwrap();

        // User previously removed a Medium article from PayX
        let item1 = create_item(
            &conn,
            Some(&ws_payx.id),
            None,
            "url",
            "https://medium.com/tech/microservices-architecture",
            Some("Microservices Overview"),
            "{\"domain\":\"medium.com\",\"app_name\":\"Google Chrome\"}",
        )
        .unwrap();

        ContextCorrectionService::record_context_correction(
            &conn,
            &item1.id,
            &ws_payx.id,
            Some("Off-topic reading"),
        )
        .unwrap();

        // New similar Medium article arrives without strong repo/task anchors
        let item2 = create_item(
            &conn,
            None,
            None,
            "url",
            "https://medium.com/tech/monolith-vs-microservices",
            Some("Monolith vs Microservices"),
            "{\"domain\":\"medium.com\",\"app_name\":\"Google Chrome\"}",
        )
        .unwrap();

        // Evaluate defense against PayX
        let verdict = ContextCorrectionService::evaluate_correction_defense(
            &conn,
            &item2,
            &ws_payx.id,
            0.55,  // weak candidate confidence
            false, // no strong anchor
            false,
        );

        assert!(verdict.is_blocked(), "Should block reattachment due to matching dismissed pattern");
        if let CorrectionDefenseVerdict::BlockedByCorrection { pattern_similarity, .. } = verdict {
            assert!(pattern_similarity >= 0.70);
        }
    }

    #[test]
    fn test_unrelated_future_contexts_not_punished_globally() {
        let conn = setup_test_db();
        let ws_payx = create_workspace(&conn, "PayX").unwrap();
        let ws_reading = create_workspace(&conn, "Tech Reading List").unwrap();

        // User removed Medium article from PayX
        let item = create_item(
            &conn,
            Some(&ws_payx.id),
            None,
            "url",
            "https://medium.com/tech/distributed-systems",
            Some("Distributed Systems Guide"),
            "{\"domain\":\"medium.com\",\"app_name\":\"Google Chrome\"}",
        )
        .unwrap();

        ContextCorrectionService::record_context_correction(
            &conn,
            &item.id,
            &ws_payx.id,
            Some("Not related to PayX"),
        )
        .unwrap();

        // Evaluate defense for "Tech Reading List" workspace
        let verdict_reading = ContextCorrectionService::evaluate_correction_defense(
            &conn,
            &item,
            &ws_reading.id,
            0.75,
            false,
            false,
        );

        assert!(verdict_reading.is_allowed(), "Unrelated workspace must NOT be blocked or penalized");
        assert_eq!(verdict_reading, CorrectionDefenseVerdict::Allowed);
    }

    #[test]
    fn test_future_strong_evidence_overrides_negative_association() {
        let conn = setup_test_db();
        let ws_payx = create_workspace(&conn, "PayX").unwrap();

        // User dismissed an item from PayX in the past
        let item = create_item(
            &conn,
            None,
            None,
            "url",
            "https://github.com/PayX/api-spec",
            Some("PayX API Documentation"),
            "{\"domain\":\"github.com\",\"app_name\":\"Google Chrome\"}",
        )
        .unwrap();

        ContextCorrectionService::record_context_correction(
            &conn,
            &item.id,
            &ws_payx.id,
            Some("Temporarily removed"),
        )
        .unwrap();

        // Later, strong deterministic anchor arrives (e.g. project repo match with 0.95 confidence)
        let verdict_strong = ContextCorrectionService::evaluate_correction_defense(
            &conn,
            &item,
            &ws_payx.id,
            0.95,
            true, // has strong anchor
            false,
        );

        assert!(verdict_strong.is_allowed());
        if let CorrectionDefenseVerdict::OverridePermitted { anchor_type, confidence, .. } = verdict_strong {
            assert_eq!(anchor_type, "strong_corroborated_anchor");
            assert_eq!(confidence, 0.95);
        } else {
            panic!("Expected OverridePermitted");
        }

        // Explicit user action also overrides
        let verdict_user = ContextCorrectionService::evaluate_correction_defense(
            &conn,
            &item,
            &ws_payx.id,
            0.50,
            false,
            true, // is explicit user action
        );

        assert!(verdict_user.is_allowed());
        if let CorrectionDefenseVerdict::OverridePermitted { anchor_type, .. } = verdict_user {
            assert_eq!(anchor_type, "explicit_user_action");
        } else {
            panic!("Expected OverridePermitted");
        }
    }
}

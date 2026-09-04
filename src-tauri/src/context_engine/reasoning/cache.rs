use rusqlite::{Connection, params, Result};
use sha2::{Sha256, Digest};
use chrono::{Utc, Duration};
use crate::context_engine::reasoning::models::{EvidencePackage, ContextReasoningResult};

/// Default cache time-to-live: 7 days.
pub const DEFAULT_CACHE_TTL_SECS: i64 = 7 * 24 * 3600;

/// Default local user identifier for single-user desktop environment.
pub const DEFAULT_USER_ID: &str = "local_user";

/// Generates a deterministic SHA-256 fingerprint of an EvidencePackage and user context.
///
/// Invariants:
/// 1. Dynamic timestamps (`system_metadata.generated_at`) are excluded.
/// 2. Arrays are normalized and sorted to avoid ordering artifacts.
/// 3. User isolation is enforced by binding `user_id` to the fingerprint.
/// 4. Any change to item content, candidate contexts, related items, scores,
///    or workspace profiles produces a different fingerprint (automatic invalidation).
pub fn hash_evidence_package(package: &EvidencePackage, user_id: Option<&str>) -> String {
    let mut normalized = package.clone();
    
    // Clear dynamic timestamp
    normalized.system_metadata.generated_at = String::new();

    // Sort internal vectors deterministically
    normalized.candidate_contexts.sort_by(|a, b| a.context_id.cmp(&b.context_id));
    normalized.relevant_related_items.sort_by(|a, b| a.item_id.cmp(&b.item_id));
    normalized.relationships.sort_by(|a, b| {
        a.other_item_id.cmp(&b.other_item_id)
            .then_with(|| a.relationship_type.cmp(&b.relationship_type))
    });
    normalized.semantic_similarity_summaries.sort_by(|a, b| a.other_item_id.cmp(&b.other_item_id));
    normalized.relevant_sessions.sort_by(|a, b| a.session_id.cmp(&b.session_id));
    normalized.workspace_semantic_profiles.sort_by(|a, b| a.workspace_id.cmp(&b.workspace_id));

    let json_bytes = serde_json::to_vec(&normalized).unwrap_or_default();
    let effective_user_id = user_id.unwrap_or(DEFAULT_USER_ID);

    let mut hasher = Sha256::new();
    // Bind user_id to prevent cross-user cache leakage
    hasher.update(effective_user_id.as_bytes());
    hasher.update(b"::");
    hasher.update(&json_bytes);
    let result = hasher.finalize();

    // Convert to hex string
    result.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Retrieves a cached AI reasoning result if it exists, is valid, and has not expired.
pub fn get_cached_reasoning(
    conn: &Connection,
    hash: &str,
    user_id: Option<&str>,
) -> Result<Option<ContextReasoningResult>> {
    let effective_user_id = user_id.unwrap_or(DEFAULT_USER_ID);
    let mut stmt = conn.prepare(
        "SELECT result_json, expires_at FROM ai_reasoning_cache WHERE package_hash = ?1 AND user_id = ?2"
    )?;
    
    let mut rows = stmt.query(params![hash, effective_user_id])?;

    if let Some(row) = rows.next()? {
        let json: String = row.get(0)?;
        let expires_at_str: String = row.get(1)?;

        // Check expiration
        if !expires_at_str.is_empty() {
            if let Ok(expires_at) = chrono::DateTime::parse_from_rfc3339(&expires_at_str) {
                if Utc::now() > expires_at.with_timezone(&Utc) {
                    // Expired entry - clean up lazily
                    let _ = conn.execute(
                        "DELETE FROM ai_reasoning_cache WHERE package_hash = ?1 AND user_id = ?2",
                        params![hash, effective_user_id],
                    );
                    return Ok(None);
                }
            }
        }

        if let Ok(result) = serde_json::from_str::<ContextReasoningResult>(&json) {
            return Ok(Some(result));
        }
    }

    Ok(None)
}

/// Stores an AI reasoning result in the cache with an expiration policy.
pub fn cache_reasoning_result(
    conn: &Connection,
    hash: &str,
    item_id: &str,
    user_id: Option<&str>,
    result: &ContextReasoningResult,
    ttl_seconds: Option<i64>,
) -> Result<()> {
    let json = serde_json::to_string(result).unwrap_or_default();
    let effective_user_id = user_id.unwrap_or(DEFAULT_USER_ID);
    let now = Utc::now();
    let ttl = ttl_seconds.unwrap_or(DEFAULT_CACHE_TTL_SECS);
    let expires_at = (now + Duration::seconds(ttl)).to_rfc3339();
    let now_str = now.to_rfc3339();

    conn.execute(
        "INSERT OR REPLACE INTO ai_reasoning_cache (package_hash, item_id, user_id, result_json, created_at, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![hash, item_id, effective_user_id, json, now_str, expires_at],
    )?;

    Ok(())
}

/// Invalidates any cached reasoning result associated with a specific item.
pub fn invalidate_item_cache(conn: &Connection, item_id: &str) -> Result<usize> {
    conn.execute(
        "DELETE FROM ai_reasoning_cache WHERE item_id = ?1",
        params![item_id],
    )
}

/// Clears all AI reasoning cached results for a specific user (or all if user_id is None).
pub fn clear_reasoning_cache(conn: &Connection, user_id: Option<&str>) -> Result<usize> {
    if let Some(uid) = user_id {
        conn.execute(
            "DELETE FROM ai_reasoning_cache WHERE user_id = ?1",
            params![uid],
        )
    } else {
        conn.execute("DELETE FROM ai_reasoning_cache", [])
    }
}

/// Prunes all expired cache entries from the database.
pub fn prune_expired_cache(conn: &Connection) -> Result<usize> {
    let now_str = Utc::now().to_rfc3339();
    conn.execute(
        "DELETE FROM ai_reasoning_cache WHERE expires_at != '' AND expires_at < ?1",
        params![now_str],
    )
}

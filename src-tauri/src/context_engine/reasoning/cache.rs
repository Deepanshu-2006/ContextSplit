use rusqlite::{Connection, params, Result};
use sha2::{Sha256, Digest};
use chrono::Utc;
use crate::context_engine::reasoning::models::{EvidencePackage, ContextReasoningResult};

/// Generates a SHA-256 hash of the JSON representation of an EvidencePackage.
pub fn hash_evidence_package(package: &EvidencePackage) -> String {
    // Clone package and clear the non-deterministic generated_at timestamp before hashing
    let mut normalized = package.clone();
    normalized.system_metadata.generated_at = String::new();

    let json_bytes = serde_json::to_vec(&normalized).unwrap_or_default();
    
    let mut hasher = Sha256::new();
    hasher.update(&json_bytes);
    let result = hasher.finalize();
    
    // Convert to hex string
    result.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Retrieves a cached AI reasoning result if it exists.
pub fn get_cached_reasoning(
    conn: &Connection,
    hash: &str,
) -> Result<Option<ContextReasoningResult>> {
    let mut stmt = conn.prepare("SELECT result_json FROM ai_reasoning_cache WHERE package_hash = ?1")?;
    let mut rows = stmt.query(params![hash])?;
    
    if let Some(row) = rows.next()? {
        let json: String = row.get(0)?;
        if let Ok(result) = serde_json::from_str(&json) {
            return Ok(Some(result));
        }
    }
    
    Ok(None)
}

/// Stores an AI reasoning result in the cache.
pub fn cache_reasoning_result(
    conn: &Connection,
    hash: &str,
    item_id: &str,
    result: &ContextReasoningResult,
) -> Result<()> {
    let json = serde_json::to_string(result).unwrap_or_default();
    let now = Utc::now().to_rfc3339();
    
    conn.execute(
        "INSERT OR REPLACE INTO ai_reasoning_cache (package_hash, item_id, result_json, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![hash, item_id, json, now],
    )?;
    
    Ok(())
}

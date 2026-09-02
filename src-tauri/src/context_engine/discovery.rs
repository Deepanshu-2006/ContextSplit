use rusqlite::{params, Connection, Result};
use chrono::Utc;
use uuid::Uuid;
use std::collections::{HashMap, HashSet};
use crate::db::models::{ContextCandidate, ContextCandidateItem, CandidateWithItems, CandidateMember};

// ---------------------------------------------------------------------------
// CRUD
// ---------------------------------------------------------------------------

pub fn create_candidate(
    conn: &Connection,
    confidence: f64,
    label: Option<&str>,
    metadata: &str,
) -> Result<ContextCandidate> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO context_candidates (id, status, confidence, label, metadata, created_at, updated_at)
         VALUES (?1, 'detected', ?2, ?3, ?4, ?5, ?5)",
        params![id, confidence, label, metadata, now],
    )?;

    Ok(ContextCandidate {
        id,
        status: "detected".to_string(),
        confidence,
        label: label.map(|s| s.to_string()),
        metadata: metadata.to_string(),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn get_candidate(conn: &Connection, id: &str) -> Result<ContextCandidate> {
    conn.query_row(
        "SELECT id, status, confidence, label, metadata, created_at, updated_at
         FROM context_candidates WHERE id = ?1",
        params![id],
        row_to_candidate,
    )
}

pub fn list_candidates(conn: &Connection, status_filter: Option<&str>) -> Result<Vec<ContextCandidate>> {
    let mut candidates = Vec::new();

    if let Some(status) = status_filter {
        let mut stmt = conn.prepare(
            "SELECT id, status, confidence, label, metadata, created_at, updated_at
             FROM context_candidates WHERE status = ?1
             ORDER BY confidence DESC, created_at DESC",
        )?;
        let iter = stmt.query_map(params![status], row_to_candidate)?;
        for c in iter { candidates.push(c?); }
    } else {
        let mut stmt = conn.prepare(
            "SELECT id, status, confidence, label, metadata, created_at, updated_at
             FROM context_candidates
             ORDER BY confidence DESC, created_at DESC",
        )?;
        let iter = stmt.query_map([], row_to_candidate)?;
        for c in iter { candidates.push(c?); }
    }

    Ok(candidates)
}

pub fn update_candidate_status(conn: &Connection, id: &str, status: &str) -> Result<ContextCandidate> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE context_candidates SET status = ?1, updated_at = ?2 WHERE id = ?3",
        params![status, now, id],
    )?;
    get_candidate(conn, id)
}

pub fn delete_candidate(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM context_candidates WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn add_item_to_candidate(
    conn: &Connection,
    candidate_id: &str,
    item_id: &str,
    relevance_score: f64,
) -> Result<ContextCandidateItem> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT OR REPLACE INTO context_candidate_items (candidate_id, item_id, relevance_score, added_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![candidate_id, item_id, relevance_score, now],
    )?;
    Ok(ContextCandidateItem {
        candidate_id: candidate_id.to_string(),
        item_id: item_id.to_string(),
        relevance_score,
        added_at: now,
    })
}

pub fn get_candidate_items(conn: &Connection, candidate_id: &str) -> Result<CandidateWithItems> {
    let candidate = get_candidate(conn, candidate_id)?;

    let mut stmt = conn.prepare(
        "SELECT cci.candidate_id, cci.item_id, cci.relevance_score, cci.added_at,
                i.id, i.workspace_id, i.session_id, i.item_type, i.content,
                i.title, i.metadata, i.created_at, i.updated_at
         FROM context_candidate_items cci
         JOIN items i ON i.id = cci.item_id
         WHERE cci.candidate_id = ?1
         ORDER BY cci.relevance_score DESC",
    )?;

    let mut members = Vec::new();
    let iter = stmt.query_map(params![candidate_id], |row| {
        Ok(CandidateMember {
            membership: ContextCandidateItem {
                candidate_id: row.get(0)?,
                item_id: row.get(1)?,
                relevance_score: row.get(2)?,
                added_at: row.get(3)?,
            },
            item: crate::db::models::Item {
                id: row.get(4)?,
                workspace_id: row.get(5)?,
                session_id: row.get(6)?,
                item_type: row.get(7)?,
                content: row.get(8)?,
                title: row.get(9)?,
                metadata: row.get(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            },
        })
    })?;

    for m in iter { members.push(m?); }
    Ok(CandidateWithItems { candidate, items: members })
}

/// Mark all candidates older than `max_age_secs` that are still "detected" or "suggested" as "expired".
/// Returns the number of candidates expired.
pub fn expire_stale_candidates(conn: &Connection, max_age_secs: i64) -> Result<usize> {
    let cutoff = (Utc::now() - chrono::Duration::seconds(max_age_secs)).to_rfc3339();
    let now = Utc::now().to_rfc3339();
    let n = conn.execute(
        "UPDATE context_candidates
         SET status = 'expired', updated_at = ?1
         WHERE status IN ('detected', 'suggested') AND created_at < ?2",
        params![now, cutoff],
    )?;
    Ok(n)
}

// ---------------------------------------------------------------------------
// Core helper
// ---------------------------------------------------------------------------

fn row_to_candidate(row: &rusqlite::Row<'_>) -> rusqlite::Result<ContextCandidate> {
    Ok(ContextCandidate {
        id: row.get(0)?,
        status: row.get(1)?,
        confidence: row.get(2)?,
        label: row.get(3)?,
        metadata: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

// ---------------------------------------------------------------------------
// Discovery algorithm
// ---------------------------------------------------------------------------

/// Internal edge representation loaded from item_relationships.
#[derive(Debug)]
struct Edge {
    item_a: String,
    item_b: String,
    strength: f64,
    relationship_type: String,
}

/// Run the Context Discovery algorithm. Reads existing item_relationships data,
/// clusters strongly-connected items using greedy graph expansion, and persists
/// new ContextCandidate rows for clusters that are not already covered.
///
/// Returns the list of newly created candidates (does NOT return existing ones).
pub fn run_discovery(conn: &Connection) -> Result<Vec<ContextCandidate>> {
    const MIN_STRENGTH: f64 = 0.5;
    const MIN_CLUSTER_SIZE: usize = 3;
    const DEDUP_OVERLAP_THRESHOLD: f64 = 0.70;
    const WINDOW_DAYS: i64 = 7;
    const DEFAULT_EXPIRY_SECS: i64 = 72 * 3600;

    // ---- 0. Expire stale candidates first ----
    let _ = expire_stale_candidates(conn, DEFAULT_EXPIRY_SECS);

    // ---- 1. Load recent strong relationships ----
    let window_start = (Utc::now() - chrono::Duration::days(WINDOW_DAYS)).to_rfc3339();

    let mut stmt = conn.prepare(
        "SELECT r.item_a_id, r.item_b_id, r.strength, r.relationship_type
         FROM item_relationships r
         WHERE r.strength >= ?1
           AND r.created_at >= ?2
         ORDER BY r.strength DESC",
    )?;

    let edges: Vec<Edge> = stmt
        .query_map(params![MIN_STRENGTH, window_start], |row| {
            Ok(Edge {
                item_a: row.get(0)?,
                item_b: row.get(1)?,
                strength: row.get(2)?,
                relationship_type: row.get(3)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    if edges.is_empty() {
        return Ok(vec![]);
    }

    // ---- 2. Build adjacency map ----
    // adjacency[item_id] = Vec<(neighbour_id, strength)>
    let mut adjacency: HashMap<String, Vec<(String, f64, String)>> = HashMap::new();
    for e in &edges {
        adjacency
            .entry(e.item_a.clone())
            .or_default()
            .push((e.item_b.clone(), e.strength, e.relationship_type.clone()));
        adjacency
            .entry(e.item_b.clone())
            .or_default()
            .push((e.item_a.clone(), e.strength, e.relationship_type.clone()));
    }

    // ---- 3. Greedy cluster expansion (DFS-style, strength-gated) ----
    let all_items: Vec<String> = adjacency.keys().cloned().collect();
    let mut visited: HashSet<String> = HashSet::new();
    let mut raw_clusters: Vec<Vec<String>> = Vec::new();

    for seed in &all_items {
        if visited.contains(seed) {
            continue;
        }
        let mut cluster = Vec::new();
        let mut stack = vec![seed.clone()];
        while let Some(node) = stack.pop() {
            if visited.contains(&node) {
                continue;
            }
            visited.insert(node.clone());
            cluster.push(node.clone());
            if let Some(neighbours) = adjacency.get(&node) {
                for (nbr, strength, _) in neighbours {
                    if !visited.contains(nbr) && *strength >= MIN_STRENGTH {
                        stack.push(nbr.clone());
                    }
                }
            }
        }
        if cluster.len() >= MIN_CLUSTER_SIZE {
            raw_clusters.push(cluster);
        }
    }

    if raw_clusters.is_empty() {
        return Ok(vec![]);
    }

    // ---- 4. Compute per-cluster metrics ----
    struct ClusterMetrics {
        items: Vec<String>,
        mean_strength: f64,
        top_rel_types: Vec<String>,
        label: Option<String>,
    }

    let mut cluster_metrics: Vec<ClusterMetrics> = Vec::new();

    for cluster in &raw_clusters {
        let item_set: HashSet<&String> = cluster.iter().collect();

        // Intra-cluster edges only
        let intra_edges: Vec<&Edge> = edges
            .iter()
            .filter(|e| item_set.contains(&e.item_a) && item_set.contains(&e.item_b))
            .collect();

        let mean_strength = if intra_edges.is_empty() {
            MIN_STRENGTH
        } else {
            intra_edges.iter().map(|e| e.strength).sum::<f64>() / intra_edges.len() as f64
        };

        // Top relationship type (by frequency)
        let mut type_counts: HashMap<&str, usize> = HashMap::new();
        for e in &intra_edges {
            *type_counts.entry(&e.relationship_type).or_insert(0) += 1;
        }
        let mut sorted_types: Vec<(&str, usize)> = type_counts.into_iter().collect();
        sorted_types.sort_by(|a, b| b.1.cmp(&a.1));
        let top_rel_types: Vec<String> = sorted_types.into_iter().take(3).map(|(t, _)| t.to_string()).collect();

        // Infer a label from context_signals (most common source app for these items)
        let label = infer_label(conn, cluster);

        cluster_metrics.push(ClusterMetrics {
            items: cluster.clone(),
            mean_strength,
            top_rel_types,
            label,
        });
    }

    // ---- 5. Deduplicate against existing active candidates ----
    let existing = list_candidates(conn, None)?;
    let existing_active: Vec<&ContextCandidate> = existing
        .iter()
        .filter(|c| matches!(c.status.as_str(), "detected" | "suggested"))
        .collect();

    // For each existing candidate, load its item IDs
    let mut existing_item_sets: Vec<HashSet<String>> = Vec::new();
    for ec in &existing_active {
        let mut stmt = conn.prepare(
            "SELECT item_id FROM context_candidate_items WHERE candidate_id = ?1",
        )?;
        let ids: HashSet<String> = stmt
            .query_map(params![ec.id], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        existing_item_sets.push(ids);
    }

    // ---- 6. Persist new candidates ----
    let mut new_candidates: Vec<ContextCandidate> = Vec::new();

    for cm in &cluster_metrics {
        let cluster_set: HashSet<&String> = cm.items.iter().collect();

        // Check if this cluster is already covered by an existing candidate
        let already_covered = existing_item_sets.iter().any(|existing_set| {
            if existing_set.is_empty() {
                return false;
            }
            let intersection = existing_set.iter().filter(|id| cluster_set.contains(id)).count();
            let union = existing_set.len() + cluster_set.len() - intersection;
            (intersection as f64 / union as f64) >= DEDUP_OVERLAP_THRESHOLD
        });

        if already_covered {
            continue;
        }

        // Build metadata JSON
        let metadata = serde_json::json!({
            "detection_strategy": "graph_clustering",
            "item_count": cm.items.len(),
            "mean_strength": cm.mean_strength,
            "top_relationship_types": cm.top_rel_types,
        })
        .to_string();

        let confidence = (cm.mean_strength * 0.8
            + (cm.items.len().min(10) as f64 / 10.0) * 0.2)
            .min(1.0);

        let candidate = create_candidate(conn, confidence, cm.label.as_deref(), &metadata)?;

        // Compute per-item relevance: degree within cluster / max degree
        let degree_in_cluster: HashMap<&String, usize> = cm.items.iter().map(|id| {
            let d = adjacency.get(id).map_or(0, |nbrs| {
                nbrs.iter().filter(|(nbr, s, _)| cluster_set.contains(nbr) && *s >= MIN_STRENGTH).count()
            });
            (id, d)
        }).collect();
        let max_degree = degree_in_cluster.values().cloned().max().unwrap_or(1).max(1);

        for item_id in &cm.items {
            let relevance = degree_in_cluster.get(item_id).cloned().unwrap_or(0) as f64
                / max_degree as f64;
            let _ = add_item_to_candidate(conn, &candidate.id, item_id, relevance);
        }

        new_candidates.push(candidate);
    }

    Ok(new_candidates)
}

/// Infer a human-readable label from context_signals for the given item set.
/// Uses the most frequent `app_name` from `active_app` signals.
fn infer_label(conn: &Connection, item_ids: &[String]) -> Option<String> {
    if item_ids.is_empty() {
        return None;
    }

    // Build a parameterised IN clause
    let placeholders: String = item_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 1))
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!(
        "SELECT payload FROM context_signals
         WHERE item_id IN ({}) AND signal_type = 'active_app'",
        placeholders
    );

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return None,
    };

    // Build params dynamically via rusqlite's params_from_iter
    let payloads: Vec<String> = match stmt.query_map(
        rusqlite::params_from_iter(item_ids.iter()),
        |row| row.get::<_, String>(0),
    ) {
        Ok(iter) => iter.filter_map(|r| r.ok()).collect(),
        Err(_) => return None,
    };

    // Parse app_name from each payload JSON and find the most frequent
    let mut app_counts: HashMap<String, usize> = HashMap::new();
    for payload in &payloads {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(payload) {
            if let Some(app) = val.get("app_name").and_then(|v| v.as_str()) {
                if !app.is_empty() {
                    *app_counts.entry(app.to_string()).or_insert(0) += 1;
                }
            }
        }
    }

    app_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(app, _)| app)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    fn make_item(conn: &Connection) -> crate::db::models::Item {
        crate::db::operations::create_item(conn, None, None, "text", "test content", None, "{}").unwrap()
    }

    #[test]
    fn test_create_and_list_candidate() {
        let conn = setup_db();

        let c = create_candidate(&conn, 0.75, Some("Test Project"), "{\"item_count\":3}").unwrap();
        assert_eq!(c.status, "detected");
        assert!((c.confidence - 0.75).abs() < 1e-9);
        assert_eq!(c.label, Some("Test Project".to_string()));

        let all = list_candidates(&conn, None).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, c.id);
    }

    #[test]
    fn test_candidate_status_lifecycle() {
        let conn = setup_db();
        let c = create_candidate(&conn, 0.6, None, "{}").unwrap();

        let c2 = update_candidate_status(&conn, &c.id, "suggested").unwrap();
        assert_eq!(c2.status, "suggested");

        let c3 = update_candidate_status(&conn, &c.id, "accepted").unwrap();
        assert_eq!(c3.status, "accepted");

        // Only non-accepted ones show up in "detected" filter
        let detected = list_candidates(&conn, Some("detected")).unwrap();
        assert_eq!(detected.len(), 0);

        let accepted = list_candidates(&conn, Some("accepted")).unwrap();
        assert_eq!(accepted.len(), 1);
    }

    #[test]
    fn test_add_and_get_candidate_items() {
        let conn = setup_db();
        let item1 = make_item(&conn);
        let item2 = make_item(&conn);
        let c = create_candidate(&conn, 0.8, None, "{}").unwrap();

        add_item_to_candidate(&conn, &c.id, &item1.id, 1.0).unwrap();
        add_item_to_candidate(&conn, &c.id, &item2.id, 0.7).unwrap();

        let with_items = get_candidate_items(&conn, &c.id).unwrap();
        assert_eq!(with_items.candidate.id, c.id);
        assert_eq!(with_items.items.len(), 2);
        // Sorted by relevance_score DESC
        assert!((with_items.items[0].membership.relevance_score - 1.0).abs() < 1e-9);
        assert!((with_items.items[1].membership.relevance_score - 0.7).abs() < 1e-9);
    }

    #[test]
    fn test_expire_stale_candidates() {
        let conn = setup_db();
        let c1 = create_candidate(&conn, 0.5, None, "{}").unwrap();
        let _c2 = create_candidate(&conn, 0.5, None, "{}").unwrap();
        // Accept one — accepted should NOT be expired
        update_candidate_status(&conn, &c1.id, "accepted").unwrap();

        // Expire everything older than 0 seconds (catches everything)
        let expired_count = expire_stale_candidates(&conn, 0).unwrap();
        // Only the 1 "detected" candidate should be expired
        assert_eq!(expired_count, 1);

        let accepted = list_candidates(&conn, Some("accepted")).unwrap();
        assert_eq!(accepted.len(), 1);

        let expired = list_candidates(&conn, Some("expired")).unwrap();
        assert_eq!(expired.len(), 1);
    }

    #[test]
    fn test_run_discovery_clusters_three_items() {
        let conn = setup_db();

        // Create 3 items and wire them with strong relationships
        let i1 = make_item(&conn);
        let i2 = make_item(&conn);
        let i3 = make_item(&conn);

        crate::db::operations::create_item_relationship(&conn, &i1.id, &i2.id, "temporal_proximity", 0.9, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &i2.id, &i3.id, "temporal_proximity", 0.8, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &i1.id, &i3.id, "temporal_proximity", 0.7, None).unwrap();

        let new_candidates = run_discovery(&conn).unwrap();
        assert_eq!(new_candidates.len(), 1, "Expected exactly 1 discovered cluster");

        let c = &new_candidates[0];
        assert_eq!(c.status, "detected");
        assert!(c.confidence > 0.0);

        let with_items = get_candidate_items(&conn, &c.id).unwrap();
        assert_eq!(with_items.items.len(), 3);
    }

    #[test]
    fn test_discovery_deduplication() {
        let conn = setup_db();

        let i1 = make_item(&conn);
        let i2 = make_item(&conn);
        let i3 = make_item(&conn);

        crate::db::operations::create_item_relationship(&conn, &i1.id, &i2.id, "temporal_proximity", 0.9, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &i2.id, &i3.id, "temporal_proximity", 0.8, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &i1.id, &i3.id, "temporal_proximity", 0.7, None).unwrap();

        // First run — should create 1 candidate
        let first = run_discovery(&conn).unwrap();
        assert_eq!(first.len(), 1);

        // Second run — should NOT create a duplicate
        let second = run_discovery(&conn).unwrap();
        assert_eq!(second.len(), 0, "Second run should not create duplicates");

        let all = list_candidates(&conn, Some("detected")).unwrap();
        assert_eq!(all.len(), 1, "Still only 1 detected candidate in DB");
    }

    #[test]
    fn test_cluster_below_min_size_ignored() {
        let conn = setup_db();

        // Only 2 items — below MIN_CLUSTER_SIZE of 3
        let i1 = make_item(&conn);
        let i2 = make_item(&conn);
        crate::db::operations::create_item_relationship(&conn, &i1.id, &i2.id, "temporal_proximity", 0.95, None).unwrap();

        let result = run_discovery(&conn).unwrap();
        assert_eq!(result.len(), 0, "Clusters smaller than 3 should be ignored");
    }
}

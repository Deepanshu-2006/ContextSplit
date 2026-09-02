use rusqlite::{params, Connection, Result};
use chrono::Utc;
use uuid::Uuid;
use std::collections::{HashMap, HashSet};
use crate::db::models::{
    ContextCandidate, ContextCandidateItem, CandidateWithItems, CandidateMember,
    CandidateScore, StrongestRelationship, ContributingItem, CandidateScoringWeights,
    WorkspaceSuppressionMatch,
};

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
         JOIN items a ON a.id = r.item_a_id
         JOIN items b ON b.id = r.item_b_id
         WHERE r.strength >= ?1
           AND r.created_at >= ?2
           AND (a.workspace_id IS NULL OR a.workspace_id = 'inbox')
           AND (b.workspace_id IS NULL OR b.workspace_id = 'inbox')
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

        // Score the candidate group with deterministic, explainable scoring
        let score = score_candidate_group(conn, &cm.items, None)?;

        // ---- Existing-Context Suppression Check ----
        // If the candidate group strongly matches an existing workspace, do not create a new candidate
        if let Some(ref ws_match) = score.workspace_match {
            if ws_match.is_suppressed {
                continue;
            }
        }

        // Build metadata JSON with complete explainable score details
        let mut meta_val = serde_json::to_value(&score).unwrap_or(serde_json::json!({}));
        if let Some(obj) = meta_val.as_object_mut() {
            obj.insert("detection_strategy".to_string(), serde_json::json!("graph_clustering"));
            obj.insert("item_count".to_string(), serde_json::json!(cm.items.len()));
            obj.insert("top_relationship_types".to_string(), serde_json::json!(cm.top_rel_types));
        }
        let metadata = meta_val.to_string();

        let candidate = create_candidate(conn, score.confidence, cm.label.as_deref(), &metadata)?;

        // Attach contributing items with their normalized relevance scores
        for ci in &score.contributing_items {
            let _ = add_item_to_candidate(conn, &candidate.id, &ci.item_id, ci.relevance_score);
        }

        new_candidates.push(candidate);
    }

    Ok(new_candidates)
}

/// Score a candidate group of items deterministically, returning confidence, evidence,
/// strongest relationships, and contributing items.
pub fn score_candidate_group(
    conn: &Connection,
    item_ids: &[String],
    weights_opt: Option<&CandidateScoringWeights>,
) -> Result<CandidateScore> {
    let default_weights = CandidateScoringWeights::default();
    let weights = weights_opt.unwrap_or(&default_weights);

    if item_ids.is_empty() {
        return Ok(CandidateScore {
            confidence: 0.0,
            confidence_percentage: "0%".to_string(),
            evidence: vec![],
            strongest_relationships: vec![],
            contributing_items: vec![],
            component_scores: HashMap::new(),
            workspace_match: None,
        });
    }

    let total_items = item_ids.len();

    // 1. Fetch Item details
    let placeholders: String = item_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 1))
        .collect::<Vec<_>>()
        .join(", ");

    let items_sql = format!(
        "SELECT id, workspace_id, session_id, item_type, content, title, metadata, created_at, updated_at
         FROM items WHERE id IN ({})",
        placeholders
    );
    let mut stmt = conn.prepare(&items_sql)?;
    let items: Vec<crate::db::models::Item> = stmt
        .query_map(rusqlite::params_from_iter(item_ids.iter()), |row| {
            Ok(crate::db::models::Item {
                id: row.get(0)?,
                workspace_id: row.get(1)?,
                session_id: row.get(2)?,
                item_type: row.get(3)?,
                content: row.get(4)?,
                title: row.get(5)?,
                metadata: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    // 2. Fetch context signals for these items
    let signals_sql = format!(
        "SELECT id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at
         FROM context_signals WHERE item_id IN ({})",
        placeholders
    );
    let mut sig_stmt = conn.prepare(&signals_sql)?;
    let signals: Vec<crate::db::models::ContextSignal> = sig_stmt
        .query_map(rusqlite::params_from_iter(item_ids.iter()), |row| {
            Ok(crate::db::models::ContextSignal {
                id: row.get(0)?,
                workspace_id: row.get(1)?,
                item_id: row.get(2)?,
                signal_type: row.get(3)?,
                session_id: row.get(4)?,
                payload: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    // Map item_id -> app_names and domains
    let mut item_apps: HashMap<String, String> = HashMap::new();
    let mut item_domains: HashMap<String, String> = HashMap::new();

    for sig in &signals {
        if let Some(ref it_id) = sig.item_id {
            if sig.signal_type == "active_app" {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&sig.payload) {
                    if let Some(app) = val.get("app_name").and_then(|v| v.as_str()) {
                        if !app.is_empty() {
                            item_apps.insert(it_id.clone(), app.to_string());
                        }
                    }
                }
            } else if sig.signal_type == "url_metadata" || sig.signal_type == "domain" {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&sig.payload) {
                    if let Some(dom) = val.get("domain").and_then(|v| v.as_str()) {
                        if !dom.is_empty() {
                            item_domains.insert(it_id.clone(), dom.to_string());
                        }
                    }
                }
            }
        }
    }

    // Also check item.metadata JSON
    for it in &items {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&it.metadata) {
            if !item_apps.contains_key(&it.id) {
                if let Some(app) = val.get("app_name").and_then(|v| v.as_str()) {
                    if !app.is_empty() {
                        item_apps.insert(it.id.clone(), app.to_string());
                    }
                }
            }
            if !item_domains.contains_key(&it.id) {
                if let Some(dom) = val.get("domain").and_then(|v| v.as_str()) {
                    if !dom.is_empty() {
                        item_domains.insert(it.id.clone(), dom.to_string());
                    }
                }
            }
        }
    }

    // 3. Fetch Intra-cluster Relationships
    let rel_sql = format!(
        "SELECT item_a_id, item_b_id, relationship_type, strength, metadata
         FROM item_relationships
         WHERE item_a_id IN ({}) AND item_b_id IN ({})",
        placeholders, placeholders
    );

    let mut rel_stmt = conn.prepare(&rel_sql)?;
    let raw_rels: Vec<crate::db::models::ItemRelationship> = rel_stmt
        .query_map(rusqlite::params_from_iter(item_ids.iter()), |row| {
            Ok(crate::db::models::ItemRelationship {
                id: String::new(),
                item_a_id: row.get(0)?,
                item_b_id: row.get(1)?,
                relationship_type: row.get(2)?,
                strength: row.get(3)?,
                created_at: String::new(),
                metadata: row.get(4)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    let item_set: HashSet<&String> = item_ids.iter().collect();
    let intra_rels: Vec<&crate::db::models::ItemRelationship> = raw_rels
        .iter()
        .filter(|r| item_set.contains(&r.item_a_id) && item_set.contains(&r.item_b_id))
        .collect();

    let mut evidence = Vec::new();
    let mut component_scores = HashMap::new();

    // --- Metric 1: Number of related items (Item Count) ---
    let item_count_score = (total_items as f64 / weights.item_count_saturation as f64).min(1.0);
    component_scores.insert("item_count".to_string(), item_count_score);

    // --- Metric 2: Relationship Strength & Density ---
    let max_possible_pairs = (total_items * (total_items.saturating_sub(1))) / 2;
    let (mean_rel_strength, rel_strength_score) = if intra_rels.is_empty() {
        (0.0, 0.0)
    } else {
        let sum_strength: f64 = intra_rels.iter().map(|r| r.strength).sum();
        let avg = sum_strength / intra_rels.len() as f64;
        let density = (intra_rels.len() as f64 / max_possible_pairs.max(1) as f64).min(1.0);
        let score = (avg * (0.6 + 0.4 * density)).min(1.0);
        (avg, score)
    };
    component_scores.insert("relationship_strength".to_string(), rel_strength_score);
    if mean_rel_strength >= 0.7 {
        evidence.push("Items have strong mutual relationships".to_string());
    } else if mean_rel_strength >= 0.5 {
        evidence.push(format!("Items have mutual relationships (mean strength: {:.0}%)", mean_rel_strength * 100.0));
    }

    // --- Metric 3: Semantic Similarity ---
    let semantic_rels: Vec<&&crate::db::models::ItemRelationship> = intra_rels
        .iter()
        .filter(|r| r.relationship_type == "semantic_similarity")
        .collect();
    let mut semantic_item_ids = HashSet::new();
    for r in &semantic_rels {
        semantic_item_ids.insert(&r.item_a_id);
        semantic_item_ids.insert(&r.item_b_id);
    }
    let (_avg_semantic_sim, semantic_score) = if semantic_rels.is_empty() {
        (0.0, 0.0)
    } else {
        let sum: f64 = semantic_rels.iter().map(|r| r.strength).sum();
        let avg = sum / semantic_rels.len() as f64;
        let coverage = semantic_item_ids.len() as f64 / total_items as f64;
        (avg, (avg * coverage).min(1.0))
    };
    component_scores.insert("semantic_similarity".to_string(), semantic_score);
    if semantic_item_ids.len() >= 2 {
        evidence.push(format!("{} semantically related Items", semantic_item_ids.len()));
    }

    // --- Metric 4: Shared Sessions ---
    let mut session_counts: HashMap<String, usize> = HashMap::new();
    for it in &items {
        if let Some(ref sess) = it.session_id {
            if !sess.is_empty() {
                *session_counts.entry(sess.clone()).or_insert(0) += 1;
            }
        }
    }
    let max_session_count = session_counts.values().cloned().max().unwrap_or(0);
    let session_score = if max_session_count >= 2 {
        let s = (max_session_count as f64 / total_items as f64).min(1.0);
        evidence.push(format!("{} Items captured in the same session", max_session_count));
        s
    } else {
        0.0
    };
    component_scores.insert("shared_session".to_string(), session_score);

    // --- Metric 5: Temporal Proximity ---
    let temporal_rels = intra_rels.iter().filter(|r| r.relationship_type == "temporal_proximity").count();
    let mut close_time_items = HashSet::new();
    for i in 0..items.len() {
        for j in (i + 1)..items.len() {
            if let (Ok(t1), Ok(t2)) = (
                items[i].created_at.parse::<chrono::DateTime<chrono::Utc>>(),
                items[j].created_at.parse::<chrono::DateTime<chrono::Utc>>()
            ) {
                if (t1 - t2).num_seconds().abs() <= 900 {
                    close_time_items.insert(&items[i].id);
                    close_time_items.insert(&items[j].id);
                }
            }
        }
    }
    let temporal_count = close_time_items.len().max(if temporal_rels > 0 { 2 } else { 0 });
    let temporal_score = if temporal_count >= 2 {
        let s = (temporal_count as f64 / total_items as f64).min(1.0);
        evidence.push(format!("{} Items captured in close temporal proximity", temporal_count));
        s
    } else {
        0.0
    };
    component_scores.insert("temporal_proximity".to_string(), temporal_score);

    // --- Metric 6: Common Domains ---
    let mut domain_counts: HashMap<String, usize> = HashMap::new();
    for dom in item_domains.values() {
        *domain_counts.entry(dom.clone()).or_insert(0) += 1;
    }
    let top_domain_pair = domain_counts.into_iter().max_by_key(|(_, c)| *c);
    let domain_score = if let Some((dom, count)) = top_domain_pair {
        if count >= 2 {
            let s = (count as f64 / total_items as f64).min(1.0);
            evidence.push(format!("{} Items share {} domain", count, dom));
            s
        } else {
            0.0
        }
    } else {
        0.0
    };
    component_scores.insert("common_domain".to_string(), domain_score);

    // --- Metric 7: Common Applications ---
    let mut app_counts: HashMap<String, usize> = HashMap::new();
    for app in item_apps.values() {
        *app_counts.entry(app.clone()).or_insert(0) += 1;
    }
    let top_app_pair = app_counts.into_iter().max_by_key(|(_, c)| *c);
    let app_score = if let Some((app, count)) = top_app_pair {
        if count >= 2 {
            let s = (count as f64 / total_items as f64).min(1.0);
            evidence.push(format!("{} Items share {} application", count, app));
            s
        } else {
            0.0
        }
    } else {
        0.0
    };
    component_scores.insert("common_app".to_string(), app_score);

    // --- Metric 8: Unassigned Items ---
    let unassigned_count = items
        .iter()
        .filter(|i| i.workspace_id.is_none() || i.workspace_id.as_deref() == Some("inbox"))
        .count();
    let unassigned_score = (unassigned_count as f64 / total_items as f64).min(1.0);
    component_scores.insert("unassigned_items".to_string(), unassigned_score);
    if unassigned_count == total_items {
        evidence.push(format!("All {} Items are unassigned in Inbox", total_items));
    } else if unassigned_count >= 2 {
        evidence.push(format!("{} of {} Items are unassigned in Inbox", unassigned_count, total_items));
    }

    // --- Metric 9: User Organization History ---
    let user_history_score = 0.0;
    component_scores.insert("user_history".to_string(), user_history_score);

    // --- Weighted Total Confidence ---
    let mut weighted_sum = 0.0;
    let mut weight_sum = 0.0;

    weighted_sum += item_count_score * weights.item_count_weight;
    weight_sum += weights.item_count_weight;

    weighted_sum += rel_strength_score * weights.relationship_strength_weight;
    weight_sum += weights.relationship_strength_weight;

    weighted_sum += semantic_score * weights.semantic_similarity_weight;
    weight_sum += weights.semantic_similarity_weight;

    weighted_sum += session_score * weights.shared_session_weight;
    weight_sum += weights.shared_session_weight;

    weighted_sum += temporal_score * weights.temporal_proximity_weight;
    weight_sum += weights.temporal_proximity_weight;

    weighted_sum += domain_score * weights.common_domain_weight;
    weight_sum += weights.common_domain_weight;

    weighted_sum += app_score * weights.common_app_weight;
    weight_sum += weights.common_app_weight;

    weighted_sum += unassigned_score * weights.unassigned_items_weight;
    weight_sum += weights.unassigned_items_weight;

    if weights.user_history_weight > 0.0 {
        weighted_sum += user_history_score * weights.user_history_weight;
        weight_sum += weights.user_history_weight;
    }

    let confidence = if weight_sum > 0.0 {
        (weighted_sum / weight_sum).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let confidence_percentage = format!("{:.0}%", (confidence * 100.0).round());

    // --- Strongest Relationships ---
    let mut sorted_intra_rels = intra_rels.clone();
    sorted_intra_rels.sort_by(|a, b| b.strength.partial_cmp(&a.strength).unwrap_or(std::cmp::Ordering::Equal));

    let strongest_relationships: Vec<StrongestRelationship> = sorted_intra_rels
        .into_iter()
        .take(5)
        .map(|r| StrongestRelationship {
            item_a_id: r.item_a_id.clone(),
            item_b_id: r.item_b_id.clone(),
            relationship_type: r.relationship_type.clone(),
            strength: r.strength,
            description: Some(format!("{} (strength: {:.2})", r.relationship_type.replace('_', " "), r.strength)),
        })
        .collect();

    // --- Contributing Items ---
    let mut item_degrees: HashMap<&String, f64> = HashMap::new();
    for r in &intra_rels {
        *item_degrees.entry(&r.item_a_id).or_insert(0.0) += r.strength;
        *item_degrees.entry(&r.item_b_id).or_insert(0.0) += r.strength;
    }
    let max_deg = item_degrees.values().cloned().fold(0.0, f64::max).max(1.0);

    let mut contributing_items: Vec<ContributingItem> = items
        .into_iter()
        .map(|it| {
            let deg = item_degrees.get(&it.id).cloned().unwrap_or(0.0);
            let rel = (deg / max_deg).clamp(0.1, 1.0);
            let primary_sig = item_apps.get(&it.id).cloned()
                .or_else(|| item_domains.get(&it.id).cloned());

            ContributingItem {
                item_id: it.id,
                relevance_score: rel,
                title: it.title,
                item_type: it.item_type,
                primary_signal: primary_sig,
            }
        })
        .collect();

    contributing_items.sort_by(|a, b| b.relevance_score.partial_cmp(&a.relevance_score).unwrap_or(std::cmp::Ordering::Equal));

    // --- Metric 10: Existing Workspace Comparison & Suppression ---
    let workspace_match = evaluate_workspace_suppression(conn, item_ids, Some(weights.workspace_suppression_threshold))?;
    if let Some(ref ws_match) = workspace_match {
        if ws_match.is_suppressed {
            evidence.push(format!(
                "Strongly matches existing Workspace '{}' ({:.0}%) - handled by existing context",
                ws_match.workspace_name,
                ws_match.match_score * 100.0
            ));
        } else if ws_match.match_score >= 0.30 {
            evidence.push(format!(
                "Ambiguous match with Workspace '{}' ({:.0}%) - distinct context candidate retained",
                ws_match.workspace_name,
                ws_match.match_score * 100.0
            ));
        } else {
            evidence.push("Novel context: distinct from existing Workspaces".to_string());
        }
    } else {
        evidence.push("Novel context: no existing Workspaces match".to_string());
    }

    Ok(CandidateScore {
        confidence,
        confidence_percentage,
        evidence,
        strongest_relationships,
        contributing_items,
        component_scores,
        workspace_match,
    })
}

/// Compares a group of candidate item IDs against all existing workspaces using
/// semantic profiles, contextual relationships, sessions, apps, and domains.
/// Returns the highest-matching workspace details if any workspaces exist.
pub fn evaluate_workspace_suppression(
    conn: &Connection,
    item_ids: &[String],
    threshold_opt: Option<f64>,
) -> Result<Option<WorkspaceSuppressionMatch>> {
    let suppression_threshold = threshold_opt.unwrap_or(0.60);
    let ambiguous_threshold = 0.30;

    let workspaces = crate::db::operations::list_workspaces(conn)?;
    if workspaces.is_empty() || item_ids.is_empty() {
        return Ok(None);
    }

    let total_items = item_ids.len();

    // 1. Pre-fetch candidate item embeddings & context signals
    let mut candidate_embeddings: HashMap<String, Vec<f32>> = HashMap::new();
    let mut candidate_apps: HashSet<String> = HashSet::new();
    let mut candidate_domains: HashSet<String> = HashSet::new();
    let mut candidate_sessions: HashSet<String> = HashSet::new();

    for it_id in item_ids {
        if let Ok(Some(emb)) = crate::db::operations::get_item_embedding(conn, it_id) {
            candidate_embeddings.insert(it_id.clone(), emb.embedding);
        }
        if let Ok(it) = crate::db::operations::get_item(conn, it_id) {
            if let Some(ref sess) = it.session_id {
                candidate_sessions.insert(sess.clone());
            }
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&it.metadata) {
                if let Some(app) = val.get("app_name").and_then(|v| v.as_str()) {
                    candidate_apps.insert(app.to_string());
                }
                if let Some(dom) = val.get("domain").and_then(|v| v.as_str()) {
                    candidate_domains.insert(dom.to_string());
                }
            }
        }
        let signals = crate::context_engine::operations::list_context_signals(conn, None, Some(it_id)).unwrap_or_default();
        for sig in signals {
            if sig.signal_type == "active_app" {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&sig.payload) {
                    if let Some(app) = val.get("app_name").and_then(|v| v.as_str()) {
                        candidate_apps.insert(app.to_string());
                    }
                }
            } else if sig.signal_type == "url_metadata" || sig.signal_type == "domain" {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&sig.payload) {
                    if let Some(dom) = val.get("domain").and_then(|v| v.as_str()) {
                        candidate_domains.insert(dom.to_string());
                    }
                }
            }
        }
    }

    let mut best_match: Option<WorkspaceSuppressionMatch> = None;
    let mut highest_score: f64 = -1.0;

    for ws in &workspaces {
        let ws_items = crate::db::operations::list_items(conn, Some(&ws.id))?;
        if ws_items.is_empty() {
            continue;
        }

        let mut reasons = Vec::new();
        let mut score = 0.0;
        let mut total_weight = 0.0;

        // --- Factor A: Graph Relationships to Workspace Items ---
        let ws_item_ids: HashSet<String> = ws_items.iter().map(|i| i.id.clone()).collect();
        let placeholders: String = item_ids.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect::<Vec<_>>().join(", ");
        let ws_offset = item_ids.len();
        let ws_placeholders: String = ws_item_ids.iter().enumerate().map(|(i, _)| format!("?{}", ws_offset + i + 1)).collect::<Vec<_>>().join(", ");

        let cross_rel_sql = format!(
            "SELECT item_a_id, item_b_id, strength, relationship_type
             FROM item_relationships
             WHERE (item_a_id IN ({}) AND item_b_id IN ({}))
                OR (item_b_id IN ({}) AND item_a_id IN ({}))",
            placeholders, ws_placeholders, placeholders, ws_placeholders
        );

        let mut cross_params: Vec<&dyn rusqlite::ToSql> = Vec::new();
        for id in item_ids { cross_params.push(id); }
        for id in &ws_item_ids { cross_params.push(id); }

        let mut cross_stmt = conn.prepare(&cross_rel_sql)?;
        let cross_rels: Vec<(String, String, f64, String)> = cross_stmt
            .query_map(rusqlite::params_from_iter(cross_params.into_iter()), |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .filter_map(|r| r.ok())
            .collect();

        let mut connected_candidate_items = HashSet::new();
        let mut rel_strength_sum = 0.0;
        for (a, b, str_val, _) in &cross_rels {
            if item_ids.contains(a) { connected_candidate_items.insert(a); }
            if item_ids.contains(b) { connected_candidate_items.insert(b); }
            rel_strength_sum += *str_val;
        }

        let rel_weight = 0.45;
        total_weight += rel_weight;
        if !cross_rels.is_empty() {
            let avg_strength = rel_strength_sum / cross_rels.len() as f64;
            let coverage = connected_candidate_items.len() as f64 / total_items as f64;
            let rel_score = (avg_strength * (0.6 + 0.4 * coverage)).min(1.0);
            score += rel_score * rel_weight;
            if avg_strength >= 0.6 {
                reasons.push(format!("Strong relationships to {} Items in '{}' (avg: {:.2})", connected_candidate_items.len(), ws.name, avg_strength));
            }
        }

        // --- Factor B: Workspace Semantic Profile Match ---
        if let Ok(Some(ws_emb)) = crate::db::operations::get_workspace_embedding(conn, &ws.id) {
            let sem_weight = 0.35;
            total_weight += sem_weight;
            let mut sims = Vec::new();
            for cand_emb in candidate_embeddings.values() {
                let s = crate::semantic::cosine_similarity(cand_emb, &ws_emb.embedding);
                if s > 0.0 {
                    sims.push(s as f64);
                }
            }
            if !sims.is_empty() {
                let avg_sim: f64 = sims.iter().sum::<f64>() / sims.len() as f64;
                score += avg_sim * sem_weight;
                if avg_sim >= 0.70 {
                    reasons.push(format!("High semantic similarity with '{}' profile ({:.0}%)", ws.name, avg_sim * 100.0));
                } else if avg_sim >= 0.45 {
                    reasons.push(format!("Moderate semantic similarity with '{}' profile ({:.0}%)", ws.name, avg_sim * 100.0));
                }
            }
        }

        // --- Factor C: Shared Context Sessions ---
        let sess_weight = 0.10;
        total_weight += sess_weight;
        let mut shared_sess_count = 0;
        for wi in &ws_items {
            if let Some(ref ws_sess) = wi.session_id {
                if candidate_sessions.contains(ws_sess) {
                    shared_sess_count += 1;
                }
            }
        }
        if shared_sess_count > 0 {
            score += 1.0 * sess_weight;
            reasons.push(format!("Shares session history with '{}'", ws.name));
        }

        // --- Factor D: Shared Applications & Domains ---
        let app_dom_weight = 0.10;
        total_weight += app_dom_weight;
        let mut ws_apps = HashSet::new();
        let mut ws_domains = HashSet::new();
        for wi in &ws_items {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&wi.metadata) {
                if let Some(app) = val.get("app_name").and_then(|v| v.as_str()) { ws_apps.insert(app.to_string()); }
                if let Some(dom) = val.get("domain").and_then(|v| v.as_str()) { ws_domains.insert(dom.to_string()); }
            }
        }
        let common_app_count = candidate_apps.intersection(&ws_apps).count();
        let common_dom_count = candidate_domains.intersection(&ws_domains).count();
        if common_app_count > 0 || common_dom_count > 0 {
            let app_dom_score = ((common_app_count + common_dom_count) as f64 / 2.0).min(1.0);
            score += app_dom_score * app_dom_weight;
            if common_app_count > 0 {
                reasons.push(format!("Shares active application with '{}'", ws.name));
            }
        }

        let mut normalized_score = if total_weight > 0.0 {
            (score / total_weight).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // Boost score if cross relationships connect candidate items strongly to this workspace
        if !cross_rels.is_empty() {
            let avg_strength = rel_strength_sum / cross_rels.len() as f64;
            let coverage = connected_candidate_items.len() as f64 / total_items as f64;
            if avg_strength >= 0.75 && coverage >= 0.66 {
                normalized_score = normalized_score.max(avg_strength);
            }
        }

        if normalized_score > highest_score {
            highest_score = normalized_score;
            let (match_type, is_suppressed) = if normalized_score >= suppression_threshold {
                ("strong_match".to_string(), true)
            } else if normalized_score >= ambiguous_threshold {
                ("ambiguous_match".to_string(), false)
            } else {
                ("novel".to_string(), false)
            };

            best_match = Some(WorkspaceSuppressionMatch {
                workspace_id: ws.id.clone(),
                workspace_name: ws.name.clone(),
                match_score: normalized_score,
                match_type,
                is_suppressed,
                reasons,
            });
        }
    }

    Ok(best_match)
}

/// Score an existing candidate by its candidate ID
pub fn score_context_candidate(
    conn: &Connection,
    candidate_id: &str,
    weights_opt: Option<&CandidateScoringWeights>,
) -> Result<CandidateScore> {
    let candidate_with_items = get_candidate_items(conn, candidate_id)?;
    let item_ids: Vec<String> = candidate_with_items
        .items
        .into_iter()
        .map(|m| m.item.id)
        .collect();

    score_candidate_group(conn, &item_ids, weights_opt)
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
        assert_eq!(with_items.items[0].membership.item_id, item1.id);
        assert_eq!(with_items.items[1].membership.item_id, item2.id);
    }

    #[test]
    fn test_expire_stale_candidates() {
        let conn = setup_db();
        let c1 = create_candidate(&conn, 0.5, None, "{}").unwrap();
        let c2 = create_candidate(&conn, 0.6, None, "{}").unwrap();

        // Artificially age c1 by setting created_at to 5 days ago
        let old_date = (Utc::now() - chrono::Duration::days(5)).to_rfc3339();
        conn.execute(
            "UPDATE context_candidates SET created_at = ?1 WHERE id = ?2",
            params![old_date, c1.id],
        ).unwrap();

        // Expire candidates older than 3 days
        let expired = expire_stale_candidates(&conn, 3 * 86400).unwrap();
        assert_eq!(expired, 1);

        let updated_c1 = get_candidate(&conn, &c1.id).unwrap();
        assert_eq!(updated_c1.status, "expired");

        let updated_c2 = get_candidate(&conn, &c2.id).unwrap();
        assert_eq!(updated_c2.status, "detected");
    }

    #[test]
    fn test_run_discovery_clusters_three_items() {
        let conn = setup_db();

        let i1 = make_item(&conn);
        let i2 = make_item(&conn);
        let i3 = make_item(&conn);

        // Connect i1-i2 and i2-i3 with strong relationships (>= 0.5)
        crate::db::operations::create_item_relationship(&conn, &i1.id, &i2.id, "temporal_proximity", 0.9, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &i2.id, &i3.id, "temporal_proximity", 0.8, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &i1.id, &i3.id, "temporal_proximity", 0.7, None).unwrap();

        let candidates = run_discovery(&conn).unwrap();
        assert_eq!(candidates.len(), 1, "Should detect exactly 1 candidate");

        let candidate = &candidates[0];
        assert_eq!(candidate.status, "detected");
        assert!(candidate.confidence > 0.5, "Confidence should be > 0.5");

        let with_items = get_candidate_items(&conn, &candidate.id).unwrap();
        assert_eq!(with_items.items.len(), 3, "All 3 items should be members");
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

    #[test]
    fn test_unrelated_items_remain_separate() {
        let conn = setup_db();

        let i1 = make_item(&conn);
        let i2 = make_item(&conn);
        let i3 = make_item(&conn);
        let i4 = make_item(&conn);

        // Weak relationships (below MIN_STRENGTH = 0.5)
        crate::db::operations::create_item_relationship(&conn, &i1.id, &i2.id, "temporal_proximity", 0.4, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &i3.id, &i4.id, "temporal_proximity", 0.3, None).unwrap();

        let result = run_discovery(&conn).unwrap();
        assert_eq!(result.len(), 0, "Should not cluster weakly related items");
    }

    #[test]
    fn test_items_in_workspace_are_deprioritized() {
        let conn = setup_db();

        // 3 items, but one is already in a workspace
        let ws = crate::db::operations::create_workspace(&conn, "Existing WS").unwrap();
        let i1 = crate::db::operations::create_item(&conn, Some(&ws.id), None, "text", "c1", None, "{}").unwrap();
        let i2 = make_item(&conn); // Inbox
        let i3 = make_item(&conn); // Inbox

        crate::db::operations::create_item_relationship(&conn, &i1.id, &i2.id, "temporal_proximity", 0.9, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &i2.id, &i3.id, "temporal_proximity", 0.8, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &i1.id, &i3.id, "temporal_proximity", 0.7, None).unwrap();

        let result = run_discovery(&conn).unwrap();
        assert_eq!(result.len(), 0, "Should not cluster if key items are already in a workspace");
    }

    #[test]
    fn test_candidate_scoring_weights_and_confidence() {
        let conn = setup_db();

        let i1 = make_item(&conn);
        let i2 = make_item(&conn);
        let i3 = make_item(&conn);
        let i4 = make_item(&conn);

        // Strong relationships
        crate::db::operations::create_item_relationship(&conn, &i1.id, &i2.id, "semantic_similarity", 0.92, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &i2.id, &i3.id, "semantic_similarity", 0.88, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &i3.id, &i4.id, "semantic_similarity", 0.85, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &i1.id, &i4.id, "temporal_proximity", 0.90, None).unwrap();

        let item_ids = vec![i1.id.clone(), i2.id.clone(), i3.id.clone(), i4.id.clone()];
        let score = score_candidate_group(&conn, &item_ids, None).unwrap();

        assert!(score.confidence >= 0.5, "Confidence score should be strong: {}", score.confidence);
        assert!(!score.confidence_percentage.is_empty());
        assert!(!score.evidence.is_empty(), "Evidence list must not be empty");
        assert!(!score.strongest_relationships.is_empty(), "Must return strongest relationships");
        assert_eq!(score.contributing_items.len(), 4, "All 4 items should be listed as contributing items");
    }

    #[test]
    fn test_candidate_scoring_evidence_generation() {
        let conn = setup_db();

        // Create 4 items sharing a session and app
        let session = crate::db::operations::create_session(&conn).unwrap();
        let session_id = &session.id;
        let i1 = crate::db::operations::create_item(&conn, None, Some(session_id), "text", "Item 1", Some("Item 1"), "{\"app_name\":\"VS Code\"}").unwrap();
        let i2 = crate::db::operations::create_item(&conn, None, Some(session_id), "text", "Item 2", Some("Item 2"), "{\"app_name\":\"VS Code\"}").unwrap();
        let i3 = crate::db::operations::create_item(&conn, None, Some(session_id), "text", "Item 3", Some("Item 3"), "{\"app_name\":\"VS Code\"}").unwrap();
        let i4 = crate::db::operations::create_item(&conn, None, Some(session_id), "text", "Item 4", Some("Item 4"), "{\"domain\":\"github.com\"}").unwrap();

        // Relationships
        crate::db::operations::create_item_relationship(&conn, &i1.id, &i2.id, "semantic_similarity", 0.9, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &i2.id, &i3.id, "semantic_similarity", 0.85, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &i3.id, &i4.id, "temporal_proximity", 0.8, None).unwrap();

        let item_ids = vec![i1.id.clone(), i2.id.clone(), i3.id.clone(), i4.id.clone()];
        let score = score_candidate_group(&conn, &item_ids, None).unwrap();

        let evidence_text = score.evidence.join("\n");
        assert!(evidence_text.contains("semantically related Items"), "Evidence should note semantic relationships: {}", evidence_text);
        assert!(evidence_text.contains("captured in the same session"), "Evidence should note shared session: {}", evidence_text);
        assert!(evidence_text.contains("VS Code application"), "Evidence should note shared app: {}", evidence_text);
        assert!(evidence_text.contains("unassigned in Inbox"), "Evidence should note items in Inbox: {}", evidence_text);
    }

    #[test]
    fn test_candidate_scoring_strongest_relationships() {
        let conn = setup_db();

        let i1 = make_item(&conn);
        let i2 = make_item(&conn);
        let i3 = make_item(&conn);

        crate::db::operations::create_item_relationship(&conn, &i1.id, &i2.id, "semantic_similarity", 0.95, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &i2.id, &i3.id, "same_domain", 0.60, None).unwrap();

        let item_ids = vec![i1.id.clone(), i2.id.clone(), i3.id.clone()];
        let score = score_candidate_group(&conn, &item_ids, None).unwrap();

        assert_eq!(score.strongest_relationships.len(), 2);
        assert_eq!(score.strongest_relationships[0].strength, 0.95, "Strongest relationship should be first");
        assert_eq!(score.strongest_relationships[0].relationship_type, "semantic_similarity");
    }

    #[test]
    fn test_candidate_scoring_unassigned_items_preference() {
        let conn = setup_db();

        // 3 unassigned items
        let u1 = make_item(&conn);
        let u2 = make_item(&conn);
        let u3 = make_item(&conn);
        crate::db::operations::create_item_relationship(&conn, &u1.id, &u2.id, "temporal_proximity", 0.8, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &u2.id, &u3.id, "temporal_proximity", 0.8, None).unwrap();

        let unassigned_score = score_candidate_group(&conn, &[u1.id, u2.id, u3.id], None).unwrap();

        // 3 items with workspace assigned
        let ws = crate::db::operations::create_workspace(&conn, "Existing").unwrap();
        let w1 = crate::db::operations::create_item(&conn, Some(&ws.id), None, "text", "1", None, "{}").unwrap();
        let w2 = crate::db::operations::create_item(&conn, Some(&ws.id), None, "text", "2", None, "{}").unwrap();
        let w3 = crate::db::operations::create_item(&conn, Some(&ws.id), None, "text", "3", None, "{}").unwrap();
        crate::db::operations::create_item_relationship(&conn, &w1.id, &w2.id, "temporal_proximity", 0.8, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &w2.id, &w3.id, "temporal_proximity", 0.8, None).unwrap();

        let assigned_score = score_candidate_group(&conn, &[w1.id, w2.id, w3.id], None).unwrap();

        assert!(
            unassigned_score.confidence > assigned_score.confidence,
            "Unassigned candidate score ({}) should be higher than assigned score ({})",
            unassigned_score.confidence,
            assigned_score.confidence
        );
    }

    #[test]
    fn test_candidate_matching_existing_workspace_suppressed() {
        let conn = setup_db();

        // 1. Create an existing workspace with an item
        let ws = crate::db::operations::create_workspace(&conn, "Alpha Workspace").unwrap();
        let ws_item = crate::db::operations::create_item(&conn, Some(&ws.id), None, "text", "Workspace item", None, "{}").unwrap();

        // 2. Create 3 unassigned items
        let i1 = make_item(&conn);
        let i2 = make_item(&conn);
        let i3 = make_item(&conn);

        // Connect i1, i2, i3 mutually
        crate::db::operations::create_item_relationship(&conn, &i1.id, &i2.id, "temporal_proximity", 0.9, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &i2.id, &i3.id, "temporal_proximity", 0.8, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &i1.id, &i3.id, "temporal_proximity", 0.7, None).unwrap();

        // ALSO strongly connect i1, i2, i3 to ws_item (Existing Workspace)
        crate::db::operations::create_item_relationship(&conn, &i1.id, &ws_item.id, "semantic_similarity", 0.95, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &i2.id, &ws_item.id, "semantic_similarity", 0.90, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &i3.id, &ws_item.id, "semantic_similarity", 0.88, None).unwrap();

        // 3. Run discovery — candidate creation MUST be suppressed because it belongs to Alpha Workspace
        let discovered = run_discovery(&conn).unwrap();
        assert_eq!(
            discovered.len(),
            0,
            "Discovered cluster should be suppressed because it strongly matches Alpha Workspace"
        );

        // Verify that score_candidate_group reports suppression
        let score = score_candidate_group(&conn, &[i1.id, i2.id, i3.id], None).unwrap();
        assert!(score.workspace_match.is_some());
        let match_info = score.workspace_match.unwrap();
        assert!(match_info.is_suppressed);
        assert_eq!(match_info.match_type, "strong_match");
        assert_eq!(match_info.workspace_id, ws.id);
    }

    #[test]
    fn test_genuinely_new_candidate_created() {
        let conn = setup_db();

        // 1. Create an existing workspace with unrelated item
        let ws = crate::db::operations::create_workspace(&conn, "Design System").unwrap();
        let _ws_item = crate::db::operations::create_item(&conn, Some(&ws.id), None, "text", "Design item", None, "{\"app_name\":\"Figma\"}").unwrap();

        // 2. Create 3 unassigned items with their own strong relationships and NO link to Design System
        let i1 = crate::db::operations::create_item(&conn, None, None, "text", "Code 1", None, "{\"app_name\":\"VS Code\"}").unwrap();
        let i2 = crate::db::operations::create_item(&conn, None, None, "text", "Code 2", None, "{\"app_name\":\"VS Code\"}").unwrap();
        let i3 = crate::db::operations::create_item(&conn, None, None, "text", "Code 3", None, "{\"app_name\":\"VS Code\"}").unwrap();

        crate::db::operations::create_item_relationship(&conn, &i1.id, &i2.id, "same_source_app", 0.9, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &i2.id, &i3.id, "same_source_app", 0.85, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &i1.id, &i3.id, "same_source_app", 0.8, None).unwrap();

        // 3. Run discovery — genuinely new candidate MUST be created
        let discovered = run_discovery(&conn).unwrap();
        assert_eq!(discovered.len(), 1, "Genuinely new cluster should be created as candidate");
        assert_eq!(discovered[0].status, "detected");

        // Verify score reports novel / unsuppressed
        let score = score_candidate_group(&conn, &[i1.id, i2.id, i3.id], None).unwrap();
        assert!(score.workspace_match.is_some());
        let match_info = score.workspace_match.unwrap();
        assert!(!match_info.is_suppressed);
        assert_eq!(match_info.match_type, "novel");
    }

    #[test]
    fn test_ambiguous_candidate_handled() {
        let conn = setup_db();

        // 1. Create existing workspace
        let ws = crate::db::operations::create_workspace(&conn, "Backend API").unwrap();
        let ws_item = crate::db::operations::create_item(&conn, Some(&ws.id), None, "text", "API item", None, "{}").unwrap();

        // 2. Create 3 unassigned items with strong internal cohesion
        let i1 = make_item(&conn);
        let i2 = make_item(&conn);
        let i3 = make_item(&conn);

        crate::db::operations::create_item_relationship(&conn, &i1.id, &i2.id, "temporal_proximity", 0.9, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &i2.id, &i3.id, "temporal_proximity", 0.85, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &i1.id, &i3.id, "temporal_proximity", 0.8, None).unwrap();

        // Only i1 has a weak/moderate relationship (0.35) to Backend API, i2 and i3 have none
        crate::db::operations::create_item_relationship(&conn, &i1.id, &ws_item.id, "temporal_proximity", 0.35, None).unwrap();

        // 3. Run discovery — ambiguous candidate is NOT suppressed, representing a potential distinct context
        let discovered = run_discovery(&conn).unwrap();
        assert_eq!(discovered.len(), 1, "Ambiguous cluster should NOT be suppressed");
        assert_eq!(discovered[0].status, "detected");

        let score = score_candidate_group(&conn, &[i1.id, i2.id, i3.id], None).unwrap();
        assert!(score.workspace_match.is_some());
        let match_info = score.workspace_match.unwrap();
        assert!(!match_info.is_suppressed, "Ambiguous match must not be suppressed");
    }
}

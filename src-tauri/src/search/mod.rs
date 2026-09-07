use rusqlite::{Connection, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::db::models::{InboxItemState, InboxState, Item};
use crate::semantic::{cosine_similarity, SemanticProvider};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InboxSearchSort {
    Newest,
    Oldest,
    Relevance,
    Title,
}

impl Default for InboxSearchSort {
    fn default() -> Self {
        InboxSearchSort::Newest
    }
}

/// Parameters for querying, filtering, and semantically ranking Inbox items.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InboxSearchParams {
    /// Free-text query string to match title, content, or metadata
    pub query: Option<String>,
    /// Filter by associated workspace ID (or "inbox" for unassigned items)
    pub workspace_id: Option<String>,
    /// Filter by detected/suggested ContextCandidate cluster ID
    pub candidate_id: Option<String>,
    /// Filter by item capture type: "text", "image", "url", "file"
    pub item_type: Option<String>,
    /// Filter items created at or after this ISO-8601 timestamp
    pub date_from: Option<String>,
    /// Filter items created at or before this ISO-8601 timestamp
    pub date_to: Option<String>,
    /// Filter by Inbox lifecycle state: "new", "processing", "suggested", "needs_review", "understood", "dismissed", "organized"
    pub inbox_state: Option<String>,
    /// Whether to compute semantic vector similarity using existing embeddings
    pub enable_semantic: Option<bool>,
    /// Minimum semantic similarity threshold (0.0 to 1.0)
    pub min_similarity: Option<f32>,
    /// Maximum number of results to return (default: 100)
    pub limit: Option<usize>,
    /// Number of items to skip for pagination
    pub offset: Option<usize>,
    /// Result sorting strategy
    pub sort_by: Option<InboxSearchSort>,
}

/// A ranked search result item enriched with contextual metadata and score highlights.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxSearchResult {
    pub item: Item,
    pub inbox_state: Option<InboxItemState>,
    pub matched_candidate_ids: Vec<String>,
    pub matched_workspace_ids: Vec<String>,
    pub text_score: f32,
    pub semantic_score: Option<f32>,
    pub combined_score: f32,
    pub match_highlights: Vec<String>,
}

/// Core function to execute multi-faceted search and filtering across items in the database.
pub fn search_inbox_items(
    conn: &Connection,
    params: &InboxSearchParams,
    semantic_provider: Option<&dyn SemanticProvider>,
) -> Result<Vec<InboxSearchResult>> {
    let mut sql = String::from(
        "SELECT i.id, i.workspace_id, i.session_id, i.item_type, i.content, i.title, i.metadata, i.created_at, i.updated_at,
                s.state, s.previous_state, s.reason, s.created_at AS state_created_at, s.updated_at AS state_updated_at
         FROM items i
         LEFT JOIN inbox_item_states s ON i.id = s.item_id
         WHERE 1=1"
    );

    let mut query_params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    // 1. Workspace Filter
    if let Some(ref ws) = params.workspace_id {
        if ws == "inbox" || ws == "unassigned" {
            sql.push_str(" AND i.workspace_id IS NULL");
        } else {
            sql.push_str(" AND (i.workspace_id = ? OR i.id IN (SELECT item_id FROM context_memberships WHERE workspace_id = ?))");
            query_params.push(Box::new(ws.clone()));
            query_params.push(Box::new(ws.clone()));
        }
    }

    // 2. Candidate Filter
    if let Some(ref cand_id) = params.candidate_id {
        if !cand_id.is_empty() {
            sql.push_str(" AND i.id IN (SELECT item_id FROM context_candidate_items WHERE candidate_id = ?)");
            query_params.push(Box::new(cand_id.clone()));
        }
    }

    // 3. Capture Type Filter
    if let Some(ref itype) = params.item_type {
        if !itype.is_empty() && itype != "all" {
            sql.push_str(" AND i.item_type = ?");
            query_params.push(Box::new(itype.clone()));
        }
    }

    // 4. Date Range Filters
    if let Some(ref d_from) = params.date_from {
        if !d_from.is_empty() {
            sql.push_str(" AND i.created_at >= ?");
            query_params.push(Box::new(d_from.clone()));
        }
    }

    if let Some(ref d_to) = params.date_to {
        if !d_to.is_empty() {
            sql.push_str(" AND i.created_at <= ?");
            query_params.push(Box::new(d_to.clone()));
        }
    }

    // 5. Inbox State Filter
    if let Some(ref state_str) = params.inbox_state {
        if !state_str.is_empty() && state_str != "all" {
            if state_str == "new" {
                sql.push_str(" AND (s.state = 'new' OR s.state IS NULL)");
            } else {
                sql.push_str(" AND s.state = ?");
                query_params.push(Box::new(state_str.clone()));
            }
        }
    }

    // 6. Text Query Filter (Keyword & Substring Matching when semantic is not active)
    let is_semantic = params.enable_semantic.unwrap_or(false);
    let search_terms = if let Some(ref q) = params.query {
        let trimmed = q.trim();
        if !trimmed.is_empty() {
            if !is_semantic {
                let pattern = format!("%{}%", trimmed);
                sql.push_str(" AND (i.title LIKE ? OR i.content LIKE ? OR i.metadata LIKE ?)");
                query_params.push(Box::new(pattern.clone()));
                query_params.push(Box::new(pattern.clone()));
                query_params.push(Box::new(pattern));
            }
            trimmed.to_lowercase().split_whitespace().map(|s| s.to_string()).collect::<Vec<_>>()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    // Default sorting in SQL if not relevance
    let sort_order = params.sort_by.unwrap_or_default();
    match sort_order {
        InboxSearchSort::Newest => sql.push_str(" ORDER BY i.created_at DESC"),
        InboxSearchSort::Oldest => sql.push_str(" ORDER BY i.created_at ASC"),
        InboxSearchSort::Title => sql.push_str(" ORDER BY i.title ASC, i.created_at DESC"),
        InboxSearchSort::Relevance => sql.push_str(" ORDER BY i.created_at DESC"),
    }

    let param_refs: Vec<&dyn rusqlite::ToSql> = query_params.iter().map(|p| p.as_ref()).collect();

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        let item = Item {
            id: row.get(0)?,
            workspace_id: row.get(1)?,
            session_id: row.get(2)?,
            item_type: row.get(3)?,
            content: row.get(4)?,
            title: row.get(5)?,
            metadata: row.get(6)?,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
        };

        let state_str: Option<String> = row.get(9)?;
        let prev_state_str: Option<String> = row.get(10)?;
        let reason: Option<String> = row.get(11)?;
        let state_created_at: Option<String> = row.get(12)?;
        let state_updated_at: Option<String> = row.get(13)?;

        let inbox_state = state_str.and_then(|s| {
            InboxState::from_str(&s).ok().map(|st| InboxItemState {
                item_id: item.id.clone(),
                state: st,
                previous_state: prev_state_str.and_then(|ps| InboxState::from_str(&ps).ok()),
                reason,
                created_at: state_created_at.unwrap_or_default(),
                updated_at: state_updated_at.unwrap_or_default(),
            })
        });

        Ok((item, inbox_state))
    })?;

    let mut raw_results = Vec::new();
    for row_res in rows {
        raw_results.push(row_res?);
    }

    // Optional query embedding for semantic search
    let query_embedding: Option<Vec<f32>> = if params.enable_semantic.unwrap_or(false) {
        if let Some(ref q) = params.query {
            if let Some(provider) = semantic_provider {
                if provider.status().is_ready && !q.trim().is_empty() {
                    provider.generate_embedding(q.trim()).ok()
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    // Batch load embeddings for matched items if semantic search is active
    let item_embeddings_map: HashMap<String, Vec<f32>> = if query_embedding.is_some() && !raw_results.is_empty() {
        let mut map = HashMap::new();
        let mut emb_stmt = conn.prepare(
            "SELECT item_id, embedding FROM item_embeddings"
        )?;
        let emb_rows = emb_stmt.query_map([], |row| {
            let item_id: String = row.get(0)?;
            let emb_blob: Vec<u8> = row.get(1)?;
            let mut embedding = Vec::with_capacity(emb_blob.len() / 4);
            for chunk in emb_blob.chunks_exact(4) {
                embedding.push(f32::from_ne_bytes(chunk.try_into().unwrap()));
            }
            Ok((item_id, embedding))
        })?;
        for r in emb_rows {
            if let Ok((id, emb)) = r {
                map.insert(id, emb);
            }
        }
        map
    } else {
        HashMap::new()
    };

    // Batch load candidate and workspace memberships for matched items
    let mut candidate_memberships_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut workspace_memberships_map: HashMap<String, Vec<String>> = HashMap::new();

    if !raw_results.is_empty() {
        let mut cand_stmt = conn.prepare("SELECT item_id, candidate_id FROM context_candidate_items")?;
        let cand_rows = cand_stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
        for r in cand_rows {
            if let Ok((item_id, cand_id)) = r {
                candidate_memberships_map.entry(item_id).or_default().push(cand_id);
            }
        }

        let mut ws_stmt = conn.prepare("SELECT item_id, workspace_id FROM context_memberships")?;
        let ws_rows = ws_stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
        for r in ws_rows {
            if let Ok((item_id, ws_id)) = r {
                workspace_memberships_map.entry(item_id).or_default().push(ws_id);
            }
        }
    }

    let min_sim = params.min_similarity.unwrap_or(0.0);
    let mut search_results: Vec<InboxSearchResult> = Vec::new();

    for (item, state) in raw_results {
        let mut text_score = 0.0f32;
        let mut highlights = Vec::new();

        // Calculate text match score & highlights
        if !search_terms.is_empty() {
            let title_lower = item.title.as_deref().unwrap_or("").to_lowercase();
            let content_lower = item.content.to_lowercase();
            let meta_lower = item.metadata.to_lowercase();

            let mut match_count = 0;
            for term in &search_terms {
                if title_lower.contains(term) {
                    text_score += 1.0;
                    match_count += 1;
                    if !highlights.contains(&format!("Title: {}", term)) {
                        highlights.push(format!("Title match: \"{}\"", term));
                    }
                }
                if content_lower.contains(term) {
                    text_score += 0.8;
                    match_count += 1;
                    if !highlights.contains(&format!("Content: {}", term)) {
                        highlights.push(format!("Content match: \"{}\"", term));
                    }
                }
                if meta_lower.contains(term) {
                    text_score += 0.5;
                    match_count += 1;
                    if !highlights.contains(&format!("Metadata: {}", term)) {
                        highlights.push(format!("Metadata match: \"{}\"", term));
                    }
                }
            }
            if match_count > 0 {
                text_score = (text_score / (search_terms.len() as f32 * 2.3)).min(1.0);
            }
        } else {
            text_score = 1.0; // Default full score when no text query is specified
        }

        // Calculate semantic similarity score
        let semantic_score = if let Some(ref q_emb) = query_embedding {
            if let Some(item_emb) = item_embeddings_map.get(&item.id) {
                let sim = cosine_similarity(q_emb, item_emb);
                Some(sim.max(0.0).min(1.0))
            } else {
                None
            }
        } else {
            None
        };

        // If semantic filtering threshold is enabled and item fails threshold
        if let Some(sem_val) = semantic_score {
            if sem_val < min_sim {
                continue;
            }
        }

        let combined_score = match semantic_score {
            Some(sem_val) => {
                if !search_terms.is_empty() {
                    0.4 * text_score + 0.6 * sem_val
                } else {
                    sem_val
                }
            }
            None => text_score,
        };

        let matched_cands = candidate_memberships_map.remove(&item.id).unwrap_or_default();
        let mut matched_workspaces = workspace_memberships_map.remove(&item.id).unwrap_or_default();
        if let Some(ref direct_ws) = item.workspace_id {
            if !matched_workspaces.contains(direct_ws) {
                matched_workspaces.push(direct_ws.clone());
            }
        }

        search_results.push(InboxSearchResult {
            item,
            inbox_state: state,
            matched_candidate_ids: matched_cands,
            matched_workspace_ids: matched_workspaces,
            text_score,
            semantic_score,
            combined_score,
            match_highlights: highlights,
        });
    }

    // Sort by combined relevance score if requested
    if sort_order == InboxSearchSort::Relevance || (params.query.is_some() && query_embedding.is_some()) {
        search_results.sort_by(|a, b| {
            b.combined_score
                .partial_cmp(&a.combined_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    // Apply pagination (offset & limit)
    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(100);

    let paginated_results = search_results
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect();

    Ok(paginated_results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::semantic::MockSemanticProvider;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        db::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_text_search_title_and_content() {
        let conn = setup_test_db();

        let item1 = db::operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Flight booking details for Paris vacation in spring",
            Some("Paris Flight Booking"),
            "{\"app_name\": \"Chrome\", \"domain\": \"airbnb.com\"}",
        ).unwrap();

        let _item2 = db::operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Quantum algorithm paper discussion and implementation",
            Some("Quantum Physics"),
            "{\"app_name\": \"VSCode\"}",
        ).unwrap();

        // Search for "Paris"
        let params = InboxSearchParams {
            query: Some("Paris".to_string()),
            ..Default::default()
        };
        let results = search_inbox_items(&conn, &params, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].item.id, item1.id);
        assert!(results[0].text_score > 0.0);

        // Search for metadata domain "airbnb"
        let params_domain = InboxSearchParams {
            query: Some("airbnb".to_string()),
            ..Default::default()
        };
        let results_domain = search_inbox_items(&conn, &params_domain, None).unwrap();
        assert_eq!(results_domain.len(), 1);
        assert_eq!(results_domain[0].item.id, item1.id);
    }

    #[test]
    fn test_filter_by_workspace_and_candidate() {
        let conn = setup_test_db();
        let ws = db::operations::create_workspace(&conn, "Travel Workspace").unwrap();

        let item1 = db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Hotel reservation receipt in Rome",
            Some("Rome Hotel"),
            "{}",
        ).unwrap();

        let item2 = db::operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://louvre.fr/tickets",
            Some("Louvre Museum"),
            "{}",
        ).unwrap();

        // Create a ContextCandidate and attach item2
        let cand = crate::context_engine::discovery::create_candidate(
            &conn,
            0.92,
            Some("Museum Tour"),
            "{\"detection_reason\": \"common_domain\"}",
        ).unwrap();
        crate::context_engine::discovery::add_item_to_candidate(&conn, &cand.id, &item2.id, 0.95).unwrap();

        // Filter by workspace
        let ws_params = InboxSearchParams {
            workspace_id: Some(ws.id.clone()),
            ..Default::default()
        };
        let ws_results = search_inbox_items(&conn, &ws_params, None).unwrap();
        assert_eq!(ws_results.len(), 1);
        assert_eq!(ws_results[0].item.id, item1.id);

        // Filter by candidate
        let cand_params = InboxSearchParams {
            candidate_id: Some(cand.id.clone()),
            ..Default::default()
        };
        let cand_results = search_inbox_items(&conn, &cand_params, None).unwrap();
        assert_eq!(cand_results.len(), 1);
        assert_eq!(cand_results[0].item.id, item2.id);
        assert_eq!(cand_results[0].matched_candidate_ids, vec![cand.id]);
    }

    #[test]
    fn test_filter_by_item_type_and_inbox_state() {
        let conn = setup_test_db();

        let text_item = db::operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Meeting notes from sprint retrospective",
            Some("Sprint Notes"),
            "{}",
        ).unwrap();

        let url_item = db::operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://github.com/rust-lang/rust",
            Some("Rust Repository"),
            "{}",
        ).unwrap();

        // Set inbox state through valid transitions (New -> Processing -> Target)
        db::operations::transition_inbox_item_state(
            &conn,
            &text_item.id,
            InboxState::Processing,
            Some("Processing text item"),
        ).unwrap();
        db::operations::transition_inbox_item_state(
            &conn,
            &text_item.id,
            InboxState::NeedsReview,
            Some("Ambiguous context detected"),
        ).unwrap();

        db::operations::transition_inbox_item_state(
            &conn,
            &url_item.id,
            InboxState::Processing,
            Some("Processing url item"),
        ).unwrap();
        db::operations::transition_inbox_item_state(
            &conn,
            &url_item.id,
            InboxState::Understood,
            Some("Understood URL item"),
        ).unwrap();

        // Filter by capture type "url"
        let type_params = InboxSearchParams {
            item_type: Some("url".to_string()),
            ..Default::default()
        };
        let type_results = search_inbox_items(&conn, &type_params, None).unwrap();
        assert_eq!(type_results.len(), 1);
        assert_eq!(type_results[0].item.id, url_item.id);

        // Filter by inbox state "needs_review"
        let state_params = InboxSearchParams {
            inbox_state: Some("needs_review".to_string()),
            ..Default::default()
        };
        let state_results = search_inbox_items(&conn, &state_params, None).unwrap();
        assert_eq!(state_results.len(), 1);
        assert_eq!(state_results[0].item.id, text_item.id);
        assert_eq!(
            state_results[0].inbox_state.as_ref().map(|s| s.state),
            Some(InboxState::NeedsReview)
        );
    }

    #[test]
    fn test_filter_by_date_range() {
        let conn = setup_test_db();

        let item = db::operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Timestamped entry",
            None,
            "{}",
        ).unwrap();

        // Match with broad date range
        let match_params = InboxSearchParams {
            date_from: Some("2020-01-01T00:00:00Z".to_string()),
            date_to: Some("2030-01-01T00:00:00Z".to_string()),
            ..Default::default()
        };
        let results = search_inbox_items(&conn, &match_params, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].item.id, item.id);

        // No match with past date range
        let no_match_params = InboxSearchParams {
            date_from: Some("2010-01-01T00:00:00Z".to_string()),
            date_to: Some("2015-01-01T00:00:00Z".to_string()),
            ..Default::default()
        };
        let empty_results = search_inbox_items(&conn, &no_match_params, None).unwrap();
        assert_eq!(empty_results.len(), 0);
    }

    #[test]
    fn test_local_semantic_search_with_embeddings() {
        let conn = setup_test_db();
        let provider = MockSemanticProvider::new();

        let item1 = db::operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Rust async runtime and Tokio performance guide",
            Some("Rust Guide"),
            "{}",
        ).unwrap();

        let item2 = db::operations::create_item(
            &conn,
            None,
            None,
            "text",
            "French culinary recipes and baking techniques",
            Some("Baking Recipes"),
            "{}",
        ).unwrap();

        // Save embeddings
        let emb1 = provider.generate_embedding(&item1.content).unwrap();
        let emb2 = provider.generate_embedding(&item2.content).unwrap();
        db::operations::save_item_embedding(&conn, &item1.id, &emb1, "mock-provider", "mock-model").unwrap();
        db::operations::save_item_embedding(&conn, &item2.id, &emb2, "mock-provider", "mock-model").unwrap();

        // Perform semantic search
        let params = InboxSearchParams {
            query: Some("Rust concurrency runtime".to_string()),
            enable_semantic: Some(true),
            min_similarity: Some(0.0),
            sort_by: Some(InboxSearchSort::Relevance),
            ..Default::default()
        };

        let results = search_inbox_items(&conn, &params, Some(&provider)).unwrap();
        assert!(!results.is_empty());
        assert!(results[0].semantic_score.is_some());
        assert!(results[0].combined_score > 0.0);
    }
}

use rusqlite::Connection;
use serde_json::Value;
use std::collections::HashMap;
use crate::db::models::{AppUsageSummary, ContextOverview, DomainUsageSummary};

/// Helper to cleanly extract domain names from URLs or web addresses.
fn extract_domain_from_url(url: &str) -> Option<String> {
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

/// Helper to sanitize and normalize application names.
fn normalize_app_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("unknown") {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Builds a human-readable, useful Context Overview for a workspace.
/// Aggregates context metadata, lifecycle state, items, sessions, detected applications,
/// important domains, recent items, and chronological timeline.
/// Local-only, private, no raw technical signals or vector dumps.
pub fn get_context_overview(
    conn: &Connection,
    workspace_id: &str,
) -> Result<ContextOverview, String> {
    // 1. Fetch Workspace
    let ws = crate::db::operations::get_workspace(conn, workspace_id)
        .map_err(|e| format!("Workspace '{}' not found: {}", workspace_id, e))?;

    // 2. Fetch Context Timeline (provides items, sessions, actions, and chronological events)
    let timeline = crate::context_engine::timeline::get_context_timeline(conn, workspace_id)?;

    // 3. Aggregate Related Applications
    let mut app_counts: HashMap<String, usize> = HashMap::new();

    // From Timeline items
    for item in &timeline.items {
        if let Some(app) = &item.app_name {
            if let Some(normalized) = normalize_app_name(app) {
                *app_counts.entry(normalized).or_insert(0) += 1;
            }
        }
    }

    // From Context Signals associated with this workspace
    let mut sig_stmt = conn.prepare(
        "SELECT payload FROM context_signals WHERE workspace_id = ?1 LIMIT 200",
    ).map_err(|e| e.to_string())?;

    let sig_rows = sig_stmt.query_map(rusqlite::params![workspace_id], |row| {
        let payload_str: String = row.get(0)?;
        Ok(payload_str)
    }).map_err(|e| e.to_string())?;

    for payload_res in sig_rows.flatten() {
        if let Ok(payload) = serde_json::from_str::<Value>(&payload_res) {
            if let Some(app) = payload.get("app_name").and_then(|v| v.as_str()).and_then(normalize_app_name) {
                let entry = app_counts.entry(app).or_insert(0);
                if *entry == 0 {
                    *entry += 1;
                }
            }
        }
    }

    let mut related_applications: Vec<AppUsageSummary> = app_counts
        .into_iter()
        .map(|(name, item_count)| AppUsageSummary { name, item_count })
        .collect();
    related_applications.sort_by(|a, b| b.item_count.cmp(&a.item_count));
    related_applications.truncate(5);

    // 4. Aggregate Important Domains
    let mut domain_counts: HashMap<String, usize> = HashMap::new();

    for item in &timeline.items {
        if let Some(domain) = &item.domain {
            let clean = domain.strip_prefix("www.").unwrap_or(domain).to_lowercase();
            if !clean.is_empty() && clean != "localhost" {
                *domain_counts.entry(clean).or_insert(0) += 1;
            }
        } else if item.item_type == "url" {
            if let Some(domain) = extract_domain_from_url(&item.content_snippet) {
                *domain_counts.entry(domain).or_insert(0) += 1;
            }
        }
    }

    let mut important_domains: Vec<DomainUsageSummary> = domain_counts
        .into_iter()
        .map(|(domain, item_count)| DomainUsageSummary { domain, item_count })
        .collect();
    important_domains.sort_by(|a, b| b.item_count.cmp(&a.item_count));
    important_domains.truncate(5);

    // 5. Recent Items (first 6 most recent)
    let recent_items = timeline.items.iter().take(6).cloned().collect();

    // 6. Recent Activity Timestamp
    let recent_activity = ws.last_activity_at
        .clone()
        .or_else(|| timeline.latest_activity_at.clone())
        .or_else(|| Some(ws.updated_at.clone()));

    Ok(ContextOverview {
        workspace_id: ws.id,
        name: ws.name,
        lifecycle_state: ws.lifecycle_state,
        item_count: timeline.total_items,
        recent_activity,
        session_count: timeline.total_sessions,
        related_applications,
        important_domains,
        recent_items,
        timeline,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_in_memory_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_context_overview_service() {
        let conn = setup_in_memory_db();

        // 1. Create Workspace
        let ws = crate::db::operations::create_workspace(&conn, "Product Design").unwrap();
        assert_eq!(ws.lifecycle_state, "active");

        // 2. Create Session
        let session = crate::db::operations::create_session(&conn).unwrap();

        // 3. Create Items with application metadata and domains
        let _item1 = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://figma.com/design/xyz",
            Some("Figma Board"),
            r#"{"app_name":"Figma","domain":"figma.com"}"#,
        ).unwrap();

        let _item2 = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "Colors and typography palette",
            Some("Palette Specs"),
            r#"{"app_name":"Figma"}"#,
        ).unwrap();

        let _item3 = crate::db::operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://github.com/org/repo/pull/42",
            Some("PR Implementation"),
            r#"{"app_name":"VS Code","domain":"github.com"}"#,
        ).unwrap();

        // 4. Retrieve Context Overview
        let overview = get_context_overview(&conn, &ws.id).unwrap();

        // Verifications:
        // a) Context Name and Lifecycle State
        assert_eq!(overview.name, "Product Design");
        assert_eq!(overview.lifecycle_state, "active");

        // b) Item Count and Session Count
        assert_eq!(overview.item_count, 3);
        assert_eq!(overview.session_count, 1);

        // c) Recent Activity
        assert!(overview.recent_activity.is_some());

        // d) Related Applications
        assert!(overview.related_applications.iter().any(|a| a.name == "Figma" && a.item_count >= 2));
        assert!(overview.related_applications.iter().any(|a| a.name == "VS Code"));

        // e) Important Domains
        assert!(overview.important_domains.iter().any(|d| d.domain == "figma.com"));
        assert!(overview.important_domains.iter().any(|d| d.domain == "github.com"));

        // f) Recent Items populated
        assert_eq!(overview.recent_items.len(), 3);

        // g) Contextual Timeline is present and populated
        assert_eq!(overview.timeline.total_items, 3);
        assert_eq!(overview.timeline.sessions.len(), 1);
        assert!(!overview.timeline.chronological_activity.is_empty());
    }
}

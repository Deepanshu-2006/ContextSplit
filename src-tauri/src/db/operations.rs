use super::models::{Workspace, Session, InboxState, InboxItemState, TaskCandidate, TaskCandidateStatus};
use chrono::Utc;
use rusqlite::{params, Connection, Result};
use uuid::Uuid;

pub fn create_workspace(conn: &Connection, name: &str) -> Result<Workspace> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO workspaces (id, name, created_at, updated_at, lifecycle_state, last_activity_at, archived_at) 
         VALUES (?1, ?2, ?3, ?4, 'active', ?5, NULL)",
        params![id, name, now, now, now],
    )?;

    Ok(Workspace {
        id,
        name: name.to_string(),
        created_at: now.clone(),
        updated_at: now.clone(),
        lifecycle_state: "active".to_string(),
        last_activity_at: Some(now),
        archived_at: None,
    })
}

pub fn get_workspace(conn: &Connection, id: &str) -> Result<Workspace> {
    let mut stmt = conn.prepare(
        "SELECT id, name, created_at, updated_at, lifecycle_state, last_activity_at, archived_at 
         FROM workspaces WHERE id = ?1"
    )?;
    let workspace = stmt.query_row(params![id], |row| {
        Ok(Workspace {
            id: row.get(0)?,
            name: row.get(1)?,
            created_at: row.get(2)?,
            updated_at: row.get(3)?,
            lifecycle_state: row.get::<_, Option<String>>(4)?.unwrap_or_else(|| "active".to_string()),
            last_activity_at: row.get(5)?,
            archived_at: row.get(6)?,
        })
    })?;
    
    Ok(workspace)
}

pub fn list_workspaces(conn: &Connection) -> Result<Vec<Workspace>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, created_at, updated_at, lifecycle_state, last_activity_at, archived_at 
         FROM workspaces ORDER BY created_at DESC"
    )?;
    let workspace_iter = stmt.query_map([], |row| {
        Ok(Workspace {
            id: row.get(0)?,
            name: row.get(1)?,
            created_at: row.get(2)?,
            updated_at: row.get(3)?,
            lifecycle_state: row.get::<_, Option<String>>(4)?.unwrap_or_else(|| "active".to_string()),
            last_activity_at: row.get(5)?,
            archived_at: row.get(6)?,
        })
    })?;

    let mut workspaces = Vec::new();
    for workspace in workspace_iter {
        workspaces.push(workspace?);
    }
    
    Ok(workspaces)
}

pub fn update_workspace(conn: &Connection, id: &str, name: &str) -> Result<Workspace> {
    let now = Utc::now().to_rfc3339();
    
    conn.execute(
        "UPDATE workspaces SET name = ?1, updated_at = ?2 WHERE id = ?3",
        params![name, now, id],
    )?;
    
    get_workspace(conn, id)
}

pub fn delete_workspace(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM workspaces WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn update_workspace_lifecycle_state(
    conn: &Connection,
    id: &str,
    new_state: &str,
) -> Result<Workspace, String> {
    let state_str = match new_state {
        "active" | "dormant" | "archived" => new_state,
        _ => return Err(format!("Invalid lifecycle state '{}'. Must be 'active', 'dormant', or 'archived'", new_state)),
    };

    let now = Utc::now().to_rfc3339();
    let current = get_workspace(conn, id).map_err(|e| e.to_string())?;

    let archived_at = if state_str == "archived" {
        Some(now.clone())
    } else {
        None
    };

    conn.execute(
        "UPDATE workspaces SET lifecycle_state = ?1, archived_at = ?2, updated_at = ?3 WHERE id = ?4",
        params![state_str, archived_at, now, id],
    ).map_err(|e| e.to_string())?;

    // Record signal for lifecycle state change
    let payload = serde_json::json!({
        "action": "workspace_lifecycle_changed",
        "previous_state": current.lifecycle_state,
        "new_state": state_str,
        "workspace_id": id,
        "workspace_name": current.name,
    }).to_string();

    let _ = crate::context_engine::operations::create_context_signal(
        conn,
        Some(id),
        None,
        "user_action",
        None,
        &payload,
    );

    get_workspace(conn, id).map_err(|e| e.to_string())
}

pub fn archive_workspace(conn: &Connection, id: &str) -> Result<Workspace, String> {
    update_workspace_lifecycle_state(conn, id, "archived")
}

pub fn unarchive_workspace(conn: &Connection, id: &str) -> Result<Workspace, String> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE workspaces SET lifecycle_state = 'active', archived_at = NULL, last_activity_at = ?1, updated_at = ?1 WHERE id = ?2",
        params![now, id],
    ).map_err(|e| e.to_string())?;
    get_workspace(conn, id).map_err(|e| e.to_string())
}

pub fn touch_workspace_activity(conn: &Connection, id: &str) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    let ws = get_workspace(conn, id).map_err(|e| e.to_string())?;
    
    // If dormant, wake up to active upon meaningful activity!
    // Note: Never automatically unarchive an archived workspace.
    let new_state = if ws.lifecycle_state == "dormant" {
        "active"
    } else {
        &ws.lifecycle_state
    };

    conn.execute(
        "UPDATE workspaces SET last_activity_at = ?1, updated_at = ?1, lifecycle_state = ?2 WHERE id = ?3",
        params![now, new_state, id],
    ).map_err(|e| e.to_string())?;

    Ok(())
}

pub fn evaluate_workspace_lifecycle(
    conn: &Connection,
    id: &str,
    inactivity_days_threshold: Option<f64>,
) -> Result<String, String> {
    let ws = get_workspace(conn, id).map_err(|e| e.to_string())?;
    // Rule: "Do not automatically archive user contexts."
    if ws.lifecycle_state == "archived" {
        return Ok("archived".to_string());
    }

    let threshold_days = inactivity_days_threshold.unwrap_or(30.0);
    let now = Utc::now();
    let last_activity_str = ws.last_activity_at.as_deref().unwrap_or(&ws.updated_at);
    
    let is_dormant = if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(last_activity_str) {
        let elapsed = now.signed_duration_since(dt.with_timezone(&Utc));
        let days = elapsed.num_seconds().max(0) as f64 / 86400.0;
        days > threshold_days
    } else {
        false
    };

    let target_state = if is_dormant { "dormant" } else { "active" };

    if target_state != ws.lifecycle_state {
        let now_str = now.to_rfc3339();
        conn.execute(
            "UPDATE workspaces SET lifecycle_state = ?1, updated_at = ?2 WHERE id = ?3",
            params![target_state, now_str, id],
        ).map_err(|e| e.to_string())?;
    }

    Ok(target_state.to_string())
}


pub fn create_item(
    conn: &Connection,
    workspace_id: Option<&str>,
    session_id: Option<&str>,
    item_type: &str,
    content: &str,
    title: Option<&str>,
    metadata: &str,
) -> Result<super::models::Item> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO items (id, workspace_id, session_id, item_type, content, title, metadata, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![id, workspace_id, session_id, item_type, content, title, metadata, now, now],
    )?;

    let item = super::models::Item {
        id: id.to_string(),
        workspace_id: workspace_id.map(|s| s.to_string()),
        session_id: session_id.map(|s| s.to_string()),
        item_type: item_type.to_string(),
        content: content.to_string(),
        title: title.map(|s| s.to_string()),
        metadata: metadata.to_string(),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    
    // Create pending semantic status
    conn.execute(
        "INSERT INTO item_semantic_status (item_id, status, retries, updated_at) VALUES (?1, ?2, 0, ?3)",
        rusqlite::params![item.id, "pending", now],
    )?;

    // Day 12: Initialize InboxItemState
    let is_inbox = match workspace_id {
        None => true,
        Some(w) if w.is_empty() || w == "inbox" => true,
        _ => false,
    };

    if is_inbox {
        let _ = conn.execute(
            "INSERT OR IGNORE INTO inbox_item_states (item_id, state, previous_state, reason, created_at, updated_at) VALUES (?1, 'new', NULL, 'New capture to inbox', ?2, ?3)",
            params![item.id, now, now],
        );
    } else {
        let _ = conn.execute(
            "INSERT OR IGNORE INTO inbox_item_states (item_id, state, previous_state, reason, created_at, updated_at) VALUES (?1, 'organized', NULL, 'Created directly in workspace', ?2, ?3)",
            params![item.id, now, now],
        );
    }

    if let Some(ws_id) = workspace_id {
        if ws_id != "inbox" && !ws_id.is_empty() {
            let _ = touch_workspace_activity(conn, ws_id);
            let _ = create_or_update_membership(conn, &id, ws_id, "confirmed", 1.0, "user");
            let _ = crate::context_engine::scoring::recalculate_workspace_suggested_memberships(conn, ws_id, Some(&id));
        }
    }
    
    Ok(item)
}

pub fn get_item(conn: &Connection, id: &str) -> Result<super::models::Item> {
    let mut stmt = conn.prepare("SELECT id, workspace_id, session_id, item_type, content, title, metadata, created_at, updated_at FROM items WHERE id = ?1")?;
    let item = stmt.query_row(params![id], |row| {
        Ok(super::models::Item {
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
    })?;
    
    Ok(item)
}

pub fn list_items(conn: &Connection, workspace_id: Option<&str>) -> Result<Vec<super::models::Item>> {
    let mut items = Vec::new();

    if let Some(w_id) = workspace_id {
        let mut stmt = conn.prepare("SELECT id, workspace_id, session_id, item_type, content, title, metadata, created_at, updated_at FROM items WHERE workspace_id = ?1 ORDER BY created_at DESC")?;
        let item_iter = stmt.query_map(params![w_id], |row| {
            Ok(super::models::Item {
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
        })?;
        for item in item_iter {
            items.push(item?);
        }
    } else {
        let mut stmt = conn.prepare("SELECT id, workspace_id, session_id, item_type, content, title, metadata, created_at, updated_at FROM items WHERE workspace_id IS NULL ORDER BY created_at DESC")?;
        let item_iter = stmt.query_map([], |row| {
            Ok(super::models::Item {
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
        })?;
        for item in item_iter {
            items.push(item?);
        }
    }

    Ok(items)
}

pub fn list_items_for_session(conn: &Connection, session_id: &str) -> Result<Vec<super::models::Item>> {
    let mut stmt = conn.prepare("SELECT id, workspace_id, session_id, item_type, content, title, metadata, created_at, updated_at FROM items WHERE session_id = ?1 ORDER BY created_at ASC")?;
    let item_iter = stmt.query_map(params![session_id], |row| {
        Ok(super::models::Item {
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
    })?;

    let mut items = Vec::new();
    for item in item_iter {
        items.push(item?);
    }

    Ok(items)
}

pub fn update_item(
    conn: &Connection,
    id: &str,
    content: Option<&str>,
    title: Option<&str>,
    metadata: Option<&str>,
) -> Result<super::models::Item> {
    let now = Utc::now().to_rfc3339();
    
    // We update fields individually to avoid overwriting with None if not provided
    if let Some(c) = content {
        conn.execute("UPDATE items SET content = ?1, updated_at = ?2 WHERE id = ?3", params![c, now, id])?;
    }
    if let Some(t) = title {
        conn.execute("UPDATE items SET title = ?1, updated_at = ?2 WHERE id = ?3", params![t, now, id])?;
    }
    if let Some(m) = metadata {
        conn.execute("UPDATE items SET metadata = ?1, updated_at = ?2 WHERE id = ?3", params![m, now, id])?;
    }
    
    // If no specific fields were updated, we still bump updated_at
    if content.is_none() && title.is_none() && metadata.is_none() {
        conn.execute("UPDATE items SET updated_at = ?1 WHERE id = ?2", params![now, id])?;
    }
    
    get_item(conn, id)
}

pub fn delete_item(conn: &Connection, id: &str) -> Result<()> {
    let old_ws = get_item(conn, id).ok().and_then(|i| i.workspace_id);
    
    conn.execute("DELETE FROM items WHERE id = ?1", params![id])?;
    
    if let Some(ws_id) = old_ws {
        let _ = recalculate_workspace_profile(conn, &ws_id);
    }
    
    Ok(())
}

pub fn empty_inbox(conn: &Connection) -> Result<usize> {
    let count = conn.execute("DELETE FROM items WHERE workspace_id IS NULL", [])?;
    Ok(count)
}

pub fn move_item(
    conn: &Connection,
    id: &str,
    workspace_id: Option<&str>,
) -> Result<super::models::Item> {
    let now = Utc::now().to_rfc3339();
    
    // Get old workspace
    let old_item = get_item(conn, id)?;
    let old_workspace_id = old_item.workspace_id.clone();
    
    conn.execute(
        "UPDATE items SET workspace_id = ?1, updated_at = ?2 WHERE id = ?3",
        params![workspace_id, now, id],
    )?;
    
    // Maintain context memberships
    if let Some(new_ws) = workspace_id {
        if new_ws != "inbox" && !new_ws.is_empty() {
            let _ = touch_workspace_activity(conn, new_ws);
            let _ = create_or_update_membership(conn, id, new_ws, "confirmed", 1.0, "user");
        }
    }
    if let Some(old_ws) = &old_workspace_id {
        if Some(old_ws.as_str()) != workspace_id {
            let _ = delete_item_workspace_membership(conn, id, old_ws);
        }
    }
    
    // Recalculate old workspace profile if it existed and is different
    if let Some(old_ws) = &old_workspace_id {
        if Some(old_ws.as_str()) != workspace_id {
            let _ = recalculate_workspace_profile(conn, old_ws);
        }
    }
    
    // Recalculate new workspace profile and dynamically recalculate suggested memberships
    if let Some(new_ws) = workspace_id {
        if Some(new_ws) != old_workspace_id.as_deref() {
            let _ = recalculate_workspace_profile(conn, new_ws);
            let _ = crate::context_engine::scoring::recalculate_workspace_suggested_memberships(conn, new_ws, Some(id));
        }
    }

    // Day 12: Update InboxItemState on move
    if let Some(new_ws) = workspace_id {
        if new_ws != "inbox" && !new_ws.is_empty() {
            let _ = transition_inbox_item_state(
                conn,
                id,
                InboxState::Organized,
                Some(&format!("User moved item to workspace '{}'", new_ws)),
            );
        } else {
            let _ = transition_inbox_item_state(
                conn,
                id,
                InboxState::New,
                Some("Item returned to inbox"),
            );
        }
    } else {
        let _ = transition_inbox_item_state(
            conn,
            id,
            InboxState::New,
            Some("Item returned to inbox"),
        );
    }
    
    get_item(conn, id)
}

// Session Operations

pub fn create_session(conn: &Connection) -> Result<Session> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let status = "active".to_string();

    conn.execute(
        "INSERT INTO sessions (id, started_at, last_activity_at, ended_at, status) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, now, now, None::<String>, status],
    )?;

    Ok(Session {
        id,
        started_at: now.clone(),
        last_activity_at: now,
        ended_at: None,
        status,
    })
}

pub fn get_latest_session(conn: &Connection) -> Result<Option<Session>> {
    let mut stmt = conn.prepare("SELECT id, started_at, last_activity_at, ended_at, status FROM sessions ORDER BY started_at DESC LIMIT 1")?;
    let mut session_iter = stmt.query_map([], |row| {
        Ok(Session {
            id: row.get(0)?,
            started_at: row.get(1)?,
            last_activity_at: row.get(2)?,
            ended_at: row.get(3)?,
            status: row.get(4)?,
        })
    })?;

    if let Some(session_result) = session_iter.next() {
        Ok(Some(session_result?))
    } else {
        Ok(None)
    }
}

pub fn get_session(conn: &Connection, id: &str) -> Result<Session> {
    let mut stmt = conn.prepare("SELECT id, started_at, last_activity_at, ended_at, status FROM sessions WHERE id = ?1")?;
    let session = stmt.query_row(params![id], |row| {
        Ok(Session {
            id: row.get(0)?,
            started_at: row.get(1)?,
            last_activity_at: row.get(2)?,
            ended_at: row.get(3)?,
            status: row.get(4)?,
        })
    })?;
    
    Ok(session)
}

pub fn update_session_activity(conn: &Connection, id: &str) -> Result<Session> {
    let now = Utc::now().to_rfc3339();
    
    conn.execute(
        "UPDATE sessions SET last_activity_at = ?1 WHERE id = ?2",
        params![now, id],
    )?;
    
    get_session(conn, id)
}

pub fn end_session(conn: &Connection, id: &str) -> Result<Session> {
    let now = Utc::now().to_rfc3339();
    
    conn.execute(
        "UPDATE sessions SET ended_at = ?1, status = 'ended' WHERE id = ?2",
        params![now, id],
    )?;
    
    get_session(conn, id)
}

#[allow(dead_code)]
pub fn list_sessions(conn: &Connection) -> Result<Vec<Session>> {
    let mut stmt = conn.prepare("SELECT id, started_at, last_activity_at, ended_at, status FROM sessions ORDER BY started_at DESC")?;
    let session_iter = stmt.query_map([], |row| {
        Ok(Session {
            id: row.get(0)?,
            started_at: row.get(1)?,
            last_activity_at: row.get(2)?,
            ended_at: row.get(3)?,
            status: row.get(4)?,
        })
    })?;

    let mut sessions = Vec::new();
    for session in session_iter {
        sessions.push(session?);
    }
    
    Ok(sessions)
}

// Item Relationships

pub fn create_item_relationship(
    conn: &Connection,
    item_1_id: &str,
    item_2_id: &str,
    relationship_type: &str,
    strength: f64,
    metadata: Option<&str>,
) -> Result<super::models::ItemRelationship> {
    // Lexicographically sort IDs to ensure consistent undirected edges
    let (item_a_id, item_b_id) = if item_1_id < item_2_id {
        (item_1_id, item_2_id)
    } else {
        (item_2_id, item_1_id)
    };

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO item_relationships (id, item_a_id, item_b_id, relationship_type, strength, metadata, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, item_a_id, item_b_id, relationship_type, strength, metadata, now],
    )?;

    Ok(super::models::ItemRelationship {
        id,
        item_a_id: item_a_id.to_string(),
        item_b_id: item_b_id.to_string(),
        relationship_type: relationship_type.to_string(),
        strength,
        metadata: metadata.map(|s| s.to_string()),
        created_at: now,
    })
}

pub fn get_item_relationships(conn: &Connection, item_id: &str) -> Result<Vec<super::models::ItemRelationship>> {
    let mut stmt = conn.prepare(
        "SELECT id, item_a_id, item_b_id, relationship_type, strength, metadata, created_at 
         FROM item_relationships 
         WHERE item_a_id = ?1 OR item_b_id = ?1
         ORDER BY created_at DESC"
    )?;
    
    let rel_iter = stmt.query_map(params![item_id], |row| {
        Ok(super::models::ItemRelationship {
            id: row.get(0)?,
            item_a_id: row.get(1)?,
            item_b_id: row.get(2)?,
            relationship_type: row.get(3)?,
            strength: row.get(4)?,
            metadata: row.get(5)?,
            created_at: row.get(6)?,
        })
    })?;

    let mut relationships = Vec::new();
    for rel in rel_iter {
        relationships.push(rel?);
    }
    
    Ok(relationships)
}

pub fn save_item_embedding(
    conn: &Connection,
    item_id: &str,
    embedding: &[f32],
    provider: &str,
    model: &str,
) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    
    let bytes: Vec<u8> = embedding.iter().flat_map(|&f| f.to_ne_bytes()).collect();
    
    conn.execute(
        "INSERT OR REPLACE INTO item_embeddings (item_id, embedding, provider, model, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![item_id, bytes, provider, model, now],
    )?;
    
    // If this item belongs to a workspace, recalculate its profile
    if let Ok(item) = get_item(conn, item_id) {
        if let Some(ws_id) = item.workspace_id {
            let _ = recalculate_workspace_profile(conn, &ws_id);
        }
    }
    
    Ok(())
}

pub fn get_item_embedding(conn: &Connection, item_id: &str) -> Result<Option<crate::db::models::ItemEmbedding>> {
    let mut stmt = conn.prepare("SELECT item_id, embedding, provider, model, created_at FROM item_embeddings WHERE item_id = ?1")?;
    
    let mut rows = stmt.query(rusqlite::params![item_id])?;
    if let Some(row) = rows.next()? {
        let item_id: String = row.get(0)?;
        let bytes: Vec<u8> = row.get(1)?;
        let provider: String = row.get(2)?;
        let model: String = row.get(3)?;
        let created_at: String = row.get(4)?;
        
        let mut embedding = Vec::with_capacity(bytes.len() / 4);
        for chunk in bytes.chunks_exact(4) {
            embedding.push(f32::from_ne_bytes(chunk.try_into().unwrap()));
        }
        
        Ok(Some(crate::db::models::ItemEmbedding {
            item_id,
            embedding,
            provider,
            model,
            created_at,
        }))
    } else {
        Ok(None)
    }
}

pub fn get_item_embedding_metadata(conn: &Connection, item_id: &str) -> Result<Option<crate::db::models::ItemEmbeddingMetadata>> {
    let mut stmt = conn.prepare("SELECT item_id, length(embedding), provider, model, created_at FROM item_embeddings WHERE item_id = ?1")?;
    
    let mut rows = stmt.query(rusqlite::params![item_id])?;
    if let Some(row) = rows.next()? {
        let item_id: String = row.get(0)?;
        let byte_len: usize = row.get(1)?;
        let provider: String = row.get(2)?;
        let model: String = row.get(3)?;
        let created_at: String = row.get(4)?;
        
        Ok(Some(crate::db::models::ItemEmbeddingMetadata {
            item_id,
            provider,
            model,
            created_at,
            dimensions: byte_len / 4,
        }))
    } else {
        Ok(None)
    }
}

#[allow(dead_code)]
pub fn delete_item_relationship(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM item_relationships WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn get_workspace_embedding(conn: &Connection, workspace_id: &str) -> Result<Option<crate::db::models::WorkspaceEmbedding>> {
    let mut stmt = conn.prepare("SELECT workspace_id, embedding, provider, model, updated_at FROM workspace_embeddings WHERE workspace_id = ?1")?;
    
    let mut rows = stmt.query(rusqlite::params![workspace_id])?;
    if let Some(row) = rows.next()? {
        let workspace_id: String = row.get(0)?;
        let bytes: Vec<u8> = row.get(1)?;
        let provider: String = row.get(2)?;
        let model: String = row.get(3)?;
        let updated_at: String = row.get(4)?;
        
        let mut embedding = Vec::with_capacity(bytes.len() / 4);
        for chunk in bytes.chunks_exact(4) {
            embedding.push(f32::from_ne_bytes(chunk.try_into().unwrap()));
        }
        
        Ok(Some(crate::db::models::WorkspaceEmbedding {
            workspace_id,
            embedding,
            provider,
            model,
            updated_at,
        }))
    } else {
        Ok(None)
    }
}

pub fn recalculate_workspace_profile(conn: &Connection, workspace_id: &str) -> Result<Option<crate::db::models::WorkspaceEmbedding>> {
    let items = list_items(conn, Some(workspace_id))?;
    
    if items.is_empty() {
        conn.execute("DELETE FROM workspace_embeddings WHERE workspace_id = ?1", rusqlite::params![workspace_id])?;
        return Ok(None);
    }
    
    let item_ids: Vec<String> = items.iter().map(|i| i.id.clone()).collect();
    let placeholders = vec!["?"; item_ids.len()].join(",");
    
    // Fetch all embeddings for these items
    let sql = format!("SELECT item_id, embedding, provider, model FROM item_embeddings WHERE item_id IN ({})", placeholders);
    let mut stmt = conn.prepare(&sql)?;
    let params = rusqlite::params_from_iter(item_ids.iter());
    
    struct TempEmb {
        item_id: String,
        emb: Vec<f32>,
        provider: String,
        model: String,
    }
    
    let mut item_embs = Vec::new();
    let mut model_counts: std::collections::HashMap<(String, String), usize> = std::collections::HashMap::new();
    
    let iter = stmt.query_map(params, |row| {
        let id: String = row.get(0)?;
        let bytes: Vec<u8> = row.get(1)?;
        let provider: String = row.get(2)?;
        let model: String = row.get(3)?;
        
        let mut emb = Vec::with_capacity(bytes.len() / 4);
        for chunk in bytes.chunks_exact(4) {
            emb.push(f32::from_ne_bytes(chunk.try_into().unwrap()));
        }
        
        Ok(TempEmb { item_id: id, emb, provider, model })
    })?;
    
    for row_res in iter {
        if let Ok(temp) = row_res {
            *model_counts.entry((temp.provider.clone(), temp.model.clone())).or_insert(0) += 1;
            item_embs.push(temp);
        }
    }
    
    if item_embs.is_empty() {
        conn.execute("DELETE FROM workspace_embeddings WHERE workspace_id = ?1", rusqlite::params![workspace_id])?;
        return Ok(None);
    }
    
    // Find the dominant model
    let (dominant_provider, dominant_model) = model_counts
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(k, _)| k)
        .unwrap();
    
    // Fetch user actions for weighting
    let sig_sql = format!("SELECT item_id, payload FROM context_signals WHERE item_id IN ({}) AND signal_type = 'user_action'", placeholders);
    let mut sig_stmt = conn.prepare(&sig_sql)?;
    let sig_params = rusqlite::params_from_iter(item_ids.iter());
    
    let mut explicit_items = std::collections::HashSet::new();
    let sig_iter = sig_stmt.query_map(sig_params, |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    
    for row_res in sig_iter {
        if let Ok((id, payload_str)) = row_res {
            if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&payload_str) {
                if payload.get("action").and_then(|v| v.as_str()) == Some("moved_to_workspace") {
                    if payload.get("destination_workspace").and_then(|v| v.as_str()) == Some(workspace_id) {
                        explicit_items.insert(id);
                    }
                }
            }
        }
    }
    
    // Compute centroid
    let mut centroid = Vec::new();
    let mut total_weight = 0.0_f32;
    
    for item_emb in item_embs {
        if item_emb.provider == dominant_provider && item_emb.model == dominant_model {
            let weight = if explicit_items.contains(&item_emb.item_id) { 2.0 } else { 1.0 };
            
            if centroid.is_empty() {
                centroid = vec![0.0; item_emb.emb.len()];
            }
            
            for (i, val) in item_emb.emb.iter().enumerate() {
                if i < centroid.len() {
                    centroid[i] += val * weight;
                }
            }
            total_weight += weight;
        }
    }
    
    if total_weight > 0.0 {
        for val in centroid.iter_mut() {
            *val /= total_weight;
        }
    }
    
    // Store it
    let now = chrono::Utc::now().to_rfc3339();
    let bytes: Vec<u8> = centroid.iter().flat_map(|&f| f.to_ne_bytes()).collect();
    
    conn.execute(
        "INSERT OR REPLACE INTO workspace_embeddings (workspace_id, embedding, provider, model, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![workspace_id, bytes, dominant_provider, dominant_model, now],
    )?;
    
    Ok(Some(crate::db::models::WorkspaceEmbedding {
        workspace_id: workspace_id.to_string(),
        embedding: centroid,
        provider: dominant_provider,
        model: dominant_model,
        updated_at: now,
    }))
}

pub fn get_item_semantic_status(conn: &Connection, item_id: &str) -> Result<Option<crate::db::models::ItemSemanticStatus>> {
    let mut stmt = conn.prepare("SELECT status, retries, model_version, updated_at FROM item_semantic_status WHERE item_id = ?1")?;
    let mut rows = stmt.query(rusqlite::params![item_id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(crate::db::models::ItemSemanticStatus {
            item_id: item_id.to_string(),
            status: row.get(0)?,
            retries: row.get(1)?,
            model_version: row.get(2)?,
            updated_at: row.get(3)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn update_item_semantic_status(
    conn: &Connection,
    item_id: &str,
    status: &str,
    model_version: Option<&str>,
) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE item_semantic_status SET status = ?1, model_version = ?2, updated_at = ?3 WHERE item_id = ?4",
        rusqlite::params![status, model_version, now, item_id],
    )?;
    Ok(())
}

pub fn increment_semantic_retries(conn: &Connection, item_id: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE item_semantic_status SET retries = retries + 1, updated_at = ?1 WHERE item_id = ?2",
        rusqlite::params![now, item_id],
    )?;
    Ok(())
}

pub fn get_pending_or_failed_items(conn: &Connection, max_retries: i32) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT item_id FROM item_semantic_status WHERE status IN ('pending', 'failed') AND retries < ?1")?;
    let rows = stmt.query_map(rusqlite::params![max_retries], |row| row.get(0))?;
    let mut items = Vec::new();
    for row in rows {
        if let Ok(id) = row {
            items.push(id);
        }
    }
    Ok(items)
}

pub fn delete_semantic_data(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM item_embeddings", [])?;
    conn.execute("DELETE FROM workspace_embeddings", [])?;
    conn.execute("DELETE FROM item_relationships WHERE relationship_type = 'semantic_similarity'", [])?;
    conn.execute("UPDATE item_semantic_status SET status = 'deleted', retries = 0", [])?;
    Ok(())
}

pub fn rebuild_semantic_data(conn: &Connection) -> Result<()> {
    delete_semantic_data(conn)?;
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute("INSERT OR REPLACE INTO item_semantic_status (item_id, status, retries, updated_at) SELECT id, 'pending', 0, ?1 FROM items", rusqlite::params![now])?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Context Membership Operations
// ---------------------------------------------------------------------------

pub fn create_or_update_membership(
    conn: &Connection,
    item_id: &str,
    workspace_id: &str,
    membership_type: &str,
    confidence: f64,
    source: &str,
) -> Result<super::models::ContextMembership> {
    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO context_memberships (id, item_id, workspace_id, membership_type, confidence, source, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
         ON CONFLICT(item_id, workspace_id) DO UPDATE SET
            membership_type = excluded.membership_type,
            confidence = excluded.confidence,
            source = excluded.source,
            updated_at = excluded.updated_at",
        params![id, item_id, workspace_id, membership_type, confidence, source, now],
    )?;

    get_item_workspace_membership(conn, item_id, workspace_id)?
        .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows)
}

pub fn get_membership(conn: &Connection, id: &str) -> Result<super::models::ContextMembership> {
    conn.query_row(
        "SELECT id, item_id, workspace_id, membership_type, confidence, source, created_at, updated_at
         FROM context_memberships WHERE id = ?1",
        params![id],
        |row| {
            Ok(super::models::ContextMembership {
                id: row.get(0)?,
                item_id: row.get(1)?,
                workspace_id: row.get(2)?,
                membership_type: row.get(3)?,
                confidence: row.get(4)?,
                source: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        },
    )
}

pub fn get_item_workspace_membership(
    conn: &Connection,
    item_id: &str,
    workspace_id: &str,
) -> Result<Option<super::models::ContextMembership>> {
    let mut stmt = conn.prepare(
        "SELECT id, item_id, workspace_id, membership_type, confidence, source, created_at, updated_at
         FROM context_memberships WHERE item_id = ?1 AND workspace_id = ?2",
    )?;

    let mut rows = stmt.query(params![item_id, workspace_id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(super::models::ContextMembership {
            id: row.get(0)?,
            item_id: row.get(1)?,
            workspace_id: row.get(2)?,
            membership_type: row.get(3)?,
            confidence: row.get(4)?,
            source: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn list_item_memberships(
    conn: &Connection,
    item_id: &str,
) -> Result<Vec<super::models::ContextMembership>> {
    let mut stmt = conn.prepare(
        "SELECT id, item_id, workspace_id, membership_type, confidence, source, created_at, updated_at
         FROM context_memberships WHERE item_id = ?1 ORDER BY created_at DESC",
    )?;

    let iter = stmt.query_map(params![item_id], |row| {
        Ok(super::models::ContextMembership {
            id: row.get(0)?,
            item_id: row.get(1)?,
            workspace_id: row.get(2)?,
            membership_type: row.get(3)?,
            confidence: row.get(4)?,
            source: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    })?;

    let mut memberships = Vec::new();
    for m in iter {
        memberships.push(m?);
    }
    Ok(memberships)
}

pub fn list_workspace_memberships(
    conn: &Connection,
    workspace_id: &str,
    membership_type: Option<&str>,
) -> Result<Vec<super::models::ContextMembership>> {
    let mut memberships = Vec::new();

    if let Some(m_type) = membership_type {
        let mut stmt = conn.prepare(
            "SELECT id, item_id, workspace_id, membership_type, confidence, source, created_at, updated_at
             FROM context_memberships WHERE workspace_id = ?1 AND membership_type = ?2 ORDER BY created_at DESC",
        )?;
        let iter = stmt.query_map(params![workspace_id, m_type], |row| {
            Ok(super::models::ContextMembership {
                id: row.get(0)?,
                item_id: row.get(1)?,
                workspace_id: row.get(2)?,
                membership_type: row.get(3)?,
                confidence: row.get(4)?,
                source: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })?;
        for m in iter {
            memberships.push(m?);
        }
    } else {
        let mut stmt = conn.prepare(
            "SELECT id, item_id, workspace_id, membership_type, confidence, source, created_at, updated_at
             FROM context_memberships WHERE workspace_id = ?1 ORDER BY created_at DESC",
        )?;
        let iter = stmt.query_map(params![workspace_id], |row| {
            Ok(super::models::ContextMembership {
                id: row.get(0)?,
                item_id: row.get(1)?,
                workspace_id: row.get(2)?,
                membership_type: row.get(3)?,
                confidence: row.get(4)?,
                source: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })?;
        for m in iter {
            memberships.push(m?);
        }
    }

    Ok(memberships)
}

pub fn confirm_membership(
    conn: &Connection,
    item_id: &str,
    workspace_id: &str,
) -> Result<super::models::ContextMembership> {
    let now = Utc::now().to_rfc3339();

    // 1. Create or update confirmed ContextMembership
    let mem = create_or_update_membership(conn, item_id, workspace_id, "confirmed", 1.0, "user")?;

    // 2. Assign item to workspace while preserving all capture metadata, content, and timestamps
    let _ = conn.execute(
        "UPDATE items SET workspace_id = ?1, updated_at = ?2 WHERE id = ?3",
        params![workspace_id, now, item_id],
    );

    // 3. Mark item organized in inbox_item_states
    let _ = transition_inbox_item_state(
        conn,
        item_id,
        InboxState::Organized,
        Some(&format!("Confirmed contextual membership in workspace '{}'", workspace_id)),
    );

    // 4. Record user action context signal for feedback learning
    let signal_id = Uuid::new_v4().to_string();
    let payload = serde_json::json!({
        "item_id": item_id,
        "workspace_id": workspace_id,
        "action": "membership_confirmed",
        "created_at": now
    })
    .to_string();

    let _ = conn.execute(
        "INSERT INTO context_signals (id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at)
         VALUES (?1, ?2, ?3, 'user_action', NULL, ?4, ?5, ?5)",
        params![signal_id, workspace_id, item_id, payload, now],
    );

    // 5. Touch workspace activity timestamp
    let _ = touch_workspace_activity(conn, workspace_id);

    // 6. Recalculate workspace profile and suggested memberships
    let _ = recalculate_workspace_profile(conn, workspace_id);
    let _ = crate::context_engine::scoring::recalculate_workspace_suggested_memberships(
        conn,
        workspace_id,
        Some(item_id),
    );

    Ok(mem)
}

pub fn dismiss_membership(
    conn: &Connection,
    item_id: &str,
    workspace_id: &str,
) -> Result<super::models::ContextMembership> {
    let mem = create_or_update_membership(conn, item_id, workspace_id, "dismissed", 0.0, "user")?;
    
    // Also record user feedback signal
    let now = Utc::now().to_rfc3339();
    let signal_id = Uuid::new_v4().to_string();
    let payload = serde_json::json!({
        "item_id": item_id,
        "workspace_id": workspace_id,
        "action": "membership_dismissed",
        "created_at": now
    }).to_string();

    let _ = conn.execute(
        "INSERT INTO context_signals (id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at)
         VALUES (?1, ?2, ?3, 'membership_dismissed', NULL, ?4, ?5, ?5)",
        params![signal_id, workspace_id, item_id, payload, now],
    );

    Ok(mem)
}

pub fn delete_membership(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM context_memberships WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn delete_item_workspace_membership(
    conn: &Connection,
    item_id: &str,
    workspace_id: &str,
) -> Result<()> {
    conn.execute(
        "DELETE FROM context_memberships WHERE item_id = ?1 AND workspace_id = ?2",
        params![item_id, workspace_id],
    )?;
    Ok(())
}

// ==========================================
// Day 12: Context-Aware Inbox State Machine
// ==========================================

pub fn create_inbox_item_state(
    conn: &Connection,
    item_id: &str,
    initial_state: InboxState,
    reason: Option<&str>,
) -> Result<InboxItemState> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO inbox_item_states (item_id, state, previous_state, reason, created_at, updated_at)
         VALUES (?1, ?2, NULL, ?3, ?4, ?5)
         ON CONFLICT(item_id) DO UPDATE SET
            state = excluded.state,
            reason = excluded.reason,
            updated_at = excluded.updated_at",
        params![item_id, initial_state.as_str(), reason, now, now],
    )?;

    Ok(InboxItemState {
        item_id: item_id.to_string(),
        state: initial_state,
        previous_state: None,
        reason: reason.map(|r| r.to_string()),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn get_inbox_item_state(conn: &Connection, item_id: &str) -> Result<Option<InboxItemState>> {
    let mut stmt = conn.prepare(
        "SELECT item_id, state, previous_state, reason, created_at, updated_at
         FROM inbox_item_states WHERE item_id = ?1"
    )?;

    let mut rows = stmt.query(params![item_id])?;
    if let Some(row) = rows.next()? {
        let state_str: String = row.get(1)?;
        let prev_state_str: Option<String> = row.get(2)?;
        
        let state = InboxState::from_str(&state_str).unwrap_or(InboxState::New);
        let previous_state = prev_state_str.and_then(|s| InboxState::from_str(&s).ok());

        Ok(Some(InboxItemState {
            item_id: row.get(0)?,
            state,
            previous_state,
            reason: row.get(3)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn transition_inbox_item_state(
    conn: &Connection,
    item_id: &str,
    target_state: InboxState,
    reason: Option<&str>,
) -> Result<InboxItemState, String> {
    let current_state_opt = get_inbox_item_state(conn, item_id)
        .map_err(|e| format!("Database error fetching inbox item state: {}", e))?;

    let (current_state, created_at) = match current_state_opt {
        Some(s) => (s.state, s.created_at),
        None => {
            let now = Utc::now().to_rfc3339();
            (InboxState::New, now)
        }
    };

    // Validate transition using explicit state machine rules
    if !current_state.can_transition_to(target_state) {
        return Err(format!(
            "Invalid state transition: Cannot transition item '{}' from '{}' to '{}'",
            item_id,
            current_state.as_str(),
            target_state.as_str()
        ));
    }

    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO inbox_item_states (item_id, state, previous_state, reason, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(item_id) DO UPDATE SET
            previous_state = ?3,
            state = ?2,
            reason = ?4,
            updated_at = ?6",
        params![
            item_id,
            target_state.as_str(),
            current_state.as_str(),
            reason,
            created_at,
            now
        ],
    ).map_err(|e| format!("Database error updating inbox item state: {}", e))?;

    Ok(InboxItemState {
        item_id: item_id.to_string(),
        state: target_state,
        previous_state: Some(current_state),
        reason: reason.map(|r| r.to_string()),
        created_at,
        updated_at: now,
    })
}

pub fn list_inbox_item_states(
    conn: &Connection,
    state_filter: Option<InboxState>,
) -> Result<Vec<InboxItemState>> {
    let mut states = Vec::new();
    if let Some(filter) = state_filter {
        let mut stmt = conn.prepare(
            "SELECT item_id, state, previous_state, reason, created_at, updated_at
             FROM inbox_item_states WHERE state = ?1 ORDER BY updated_at DESC"
        )?;
        let rows = stmt.query_map(params![filter.as_str()], |row| {
            let state_str: String = row.get(1)?;
            let prev_state_str: Option<String> = row.get(2)?;
            Ok(InboxItemState {
                item_id: row.get(0)?,
                state: InboxState::from_str(&state_str).unwrap_or(InboxState::New),
                previous_state: prev_state_str.and_then(|s| InboxState::from_str(&s).ok()),
                reason: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?;
        for s in rows {
            states.push(s?);
        }
    } else {
        let mut stmt = conn.prepare(
            "SELECT item_id, state, previous_state, reason, created_at, updated_at
             FROM inbox_item_states ORDER BY updated_at DESC"
        )?;
        let rows = stmt.query_map([], |row| {
            let state_str: String = row.get(1)?;
            let prev_state_str: Option<String> = row.get(2)?;
            Ok(InboxItemState {
                item_id: row.get(0)?,
                state: InboxState::from_str(&state_str).unwrap_or(InboxState::New),
                previous_state: prev_state_str.and_then(|s| InboxState::from_str(&s).ok()),
                reason: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?;
        for s in rows {
            states.push(s?);
        }
    }
    Ok(states)
}

#[allow(dead_code)]
pub fn delete_inbox_item_state(conn: &Connection, item_id: &str) -> Result<()> {
    conn.execute("DELETE FROM inbox_item_states WHERE item_id = ?1", params![item_id])?;
    Ok(())
}

// ===========================================================================
// Multi-Select Bulk Inbox Operations with Atomic Transaction Support
// ===========================================================================

/// Atomically assigns multiple inbox items to a destination workspace.
/// Uses a SQLite transaction ensuring all-or-nothing execution.
/// Creates confirmed ContextMemberships, transitions InboxItemState to Organized,
/// records user action ContextSignals, preserves capture metadata, and keeps ItemRelationships intact.
pub fn bulk_assign_inbox_items_to_workspace(
    conn: &mut Connection,
    item_ids: &[String],
    workspace_id: &str,
) -> Result<Vec<super::models::Item>, String> {
    if item_ids.is_empty() {
        return Ok(Vec::new());
    }

    let tx = conn
        .transaction()
        .map_err(|e| format!("Failed to begin bulk assignment transaction: {}", e))?;

    // 1. Verify destination workspace exists
    {
        let mut check_ws = tx
            .prepare("SELECT id FROM workspaces WHERE id = ?1")
            .map_err(|e| format!("Failed to prepare workspace check: {}", e))?;
        let exists = check_ws
            .exists(params![workspace_id])
            .map_err(|e| format!("Failed to verify workspace '{}': {}", workspace_id, e))?;
        if !exists {
            return Err(format!("Destination workspace '{}' does not exist", workspace_id));
        }
    }

    let now = Utc::now().to_rfc3339();
    let mut old_workspaces: Vec<String> = Vec::new();

    // 2. Validate all items exist before applying mutations
    for item_id in item_ids {
        let mut check_item = tx
            .prepare("SELECT id, workspace_id FROM items WHERE id = ?1")
            .map_err(|e| format!("Failed to prepare item check: {}", e))?;
        let mut rows = check_item
            .query(params![item_id])
            .map_err(|e| format!("Failed to query item '{}': {}", item_id, e))?;

        if let Some(row) = rows
            .next()
            .map_err(|e| format!("Error inspecting item '{}': {}", item_id, e))?
        {
            let old_ws: Option<String> = row.get(1).map_err(|e| e.to_string())?;
            if let Some(ows) = old_ws {
                if ows != workspace_id && !old_workspaces.contains(&ows) {
                    old_workspaces.push(ows);
                }
            }
        } else {
            return Err(format!("Item '{}' does not exist", item_id));
        }
    }

    // 3. Update items, memberships, and states atomically
    for item_id in item_ids {
        // Update item workspace_id while preserving all metadata, content, and original timestamps
        let rows_affected = tx
            .execute(
                "UPDATE items SET workspace_id = ?1, updated_at = ?2 WHERE id = ?3",
                params![workspace_id, now, item_id],
            )
            .map_err(|e| format!("Failed to update item '{}': {}", item_id, e))?;

        if rows_affected == 0 {
            return Err(format!("Item '{}' not found during update", item_id));
        }

        // Create or update confirmed ContextMembership
        let mem_id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO context_memberships (id, item_id, workspace_id, membership_type, confidence, source, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'confirmed', 1.0, 'user', ?4, ?4)
             ON CONFLICT(item_id, workspace_id) DO UPDATE SET
                membership_type = 'confirmed',
                confidence = 1.0,
                source = 'user',
                updated_at = excluded.updated_at",
            params![mem_id, item_id, workspace_id, now],
        )
        .map_err(|e| format!("Failed to create confirmed membership for item '{}': {}", item_id, e))?;

        // Update InboxItemState to Organized
        tx.execute(
            "INSERT INTO inbox_item_states (item_id, state, previous_state, reason, created_at, updated_at)
             VALUES (?1, 'organized', NULL, ?2, ?3, ?3)
             ON CONFLICT(item_id) DO UPDATE SET
                previous_state = state,
                state = 'organized',
                reason = ?2,
                updated_at = ?3",
            params![item_id, format!("Bulk assigned to workspace '{}'", workspace_id), now],
        )
        .map_err(|e| format!("Failed to update inbox state for item '{}': {}", item_id, e))?;

        // Clean up old workspace memberships if moved from another workspace
        for old_ws in &old_workspaces {
            tx.execute(
                "DELETE FROM context_memberships WHERE item_id = ?1 AND workspace_id = ?2",
                params![item_id, old_ws],
            )
            .map_err(|e| format!("Failed to clean up old membership for item '{}': {}", item_id, e))?;
        }
    }

    // 4. Record user action context signals for feedback learning
    let bulk_sig_id = Uuid::new_v4().to_string();
    let bulk_payload = serde_json::json!({
        "action": "bulk_assigned_to_workspace",
        "workspace_id": workspace_id,
        "item_ids": item_ids,
        "item_count": item_ids.len(),
        "created_at": now
    })
    .to_string();

    tx.execute(
        "INSERT INTO context_signals (id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at)
         VALUES (?1, ?2, NULL, 'user_action', NULL, ?3, ?4, ?4)",
        params![bulk_sig_id, workspace_id, bulk_payload, now],
    )
    .map_err(|e| format!("Failed to record bulk signal: {}", e))?;

    for item_id in item_ids {
        let item_sig_id = Uuid::new_v4().to_string();
        let item_payload = serde_json::json!({
            "action": "context_assigned",
            "workspace_id": workspace_id,
            "item_id": item_id,
            "created_at": now
        })
        .to_string();

        tx.execute(
            "INSERT INTO context_signals (id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'user_action', NULL, ?4, ?5, ?5)",
            params![item_sig_id, workspace_id, item_id, item_payload, now],
        )
        .map_err(|e| format!("Failed to record item signal for '{}': {}", item_id, e))?;
    }

    // 5. Touch workspace activity timestamp
    tx.execute(
        "UPDATE workspaces SET last_activity_at = ?1, updated_at = ?1 WHERE id = ?2",
        params![now, workspace_id],
    )
    .map_err(|e| format!("Failed to update workspace activity: {}", e))?;

    // 6. Commit transaction atomically
    tx.commit()
        .map_err(|e| format!("Failed to commit bulk assignment transaction: {}", e))?;

    // 7. Recalculate profiles and suggestions after successful transaction commit
    let _ = recalculate_workspace_profile(conn, workspace_id);
    for old_ws in &old_workspaces {
        let _ = recalculate_workspace_profile(conn, old_ws);
    }
    let _ = crate::context_engine::scoring::recalculate_workspace_suggested_memberships(
        conn,
        workspace_id,
        None,
    );

    // 8. Return updated items
    let mut updated_items = Vec::new();
    for id in item_ids {
        if let Ok(item) = get_item(conn, id) {
            updated_items.push(item);
        }
    }

    Ok(updated_items)
}

/// Atomically dismisses suggestions for multiple inbox items in a transaction.
pub fn bulk_dismiss_inbox_suggestions(
    conn: &mut Connection,
    item_ids: &[String],
    workspace_id: Option<&str>,
) -> Result<(), String> {
    if item_ids.is_empty() {
        return Ok(());
    }

    let tx = conn
        .transaction()
        .map_err(|e| format!("Failed to begin bulk dismiss transaction: {}", e))?;
    let now = Utc::now().to_rfc3339();

    for item_id in item_ids {
        if let Some(ws_id) = workspace_id {
            let mem_id = Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO context_memberships (id, item_id, workspace_id, membership_type, confidence, source, created_at, updated_at)
                 VALUES (?1, ?2, ?3, 'dismissed', 0.0, 'user', ?4, ?4)
                 ON CONFLICT(item_id, workspace_id) DO UPDATE SET
                    membership_type = 'dismissed',
                    confidence = 0.0,
                    source = 'user',
                    updated_at = excluded.updated_at",
                params![mem_id, item_id, ws_id, now],
            )
            .map_err(|e| format!("Failed to dismiss membership for '{}': {}", item_id, e))?;
        }

        // Transition inbox state to 'understood'
        tx.execute(
            "INSERT INTO inbox_item_states (item_id, state, previous_state, reason, created_at, updated_at)
             VALUES (?1, 'understood', NULL, ?2, ?3, ?3)
             ON CONFLICT(item_id) DO UPDATE SET
                previous_state = state,
                state = 'understood',
                reason = ?2,
                updated_at = ?3",
            params![item_id, "Bulk dismissed suggestion", now],
        )
        .map_err(|e| format!("Failed to update inbox state for '{}': {}", item_id, e))?;

        // Record dismissal feedback signal
        let sig_id = Uuid::new_v4().to_string();
        let payload = serde_json::json!({
            "item_id": item_id,
            "workspace_id": workspace_id,
            "action": "bulk_suggestion_dismissed",
            "created_at": now
        })
        .to_string();

        tx.execute(
            "INSERT INTO context_signals (id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'membership_dismissed', NULL, ?4, ?5, ?5)",
            params![sig_id, workspace_id, item_id, payload, now],
        )
        .map_err(|e| format!("Failed to record dismissal signal for '{}': {}", item_id, e))?;
    }

    tx.commit()
        .map_err(|e| format!("Failed to commit bulk dismiss transaction: {}", e))?;
    Ok(())
}

/// Atomically archives/clears multiple selected items from the inbox in a transaction.
pub fn bulk_archive_or_clear_inbox_items(
    conn: &mut Connection,
    item_ids: &[String],
) -> Result<usize, String> {
    if item_ids.is_empty() {
        return Ok(0);
    }

    let tx = conn
        .transaction()
        .map_err(|e| format!("Failed to begin bulk clear transaction: {}", e))?;
    let mut deleted_count = 0;

    for item_id in item_ids {
        // Delete inbox item (only if workspace_id IS NULL / in inbox)
        let count = tx
            .execute(
                "DELETE FROM items WHERE id = ?1 AND workspace_id IS NULL",
                params![item_id],
            )
            .map_err(|e| format!("Failed to delete inbox item '{}': {}", item_id, e))?;

        if count > 0 {
            deleted_count += count;
            let _ = tx.execute(
                "DELETE FROM inbox_item_states WHERE item_id = ?1",
                params![item_id],
            );
        }
    }

    tx.commit()
        .map_err(|e| format!("Failed to commit bulk clear transaction: {}", e))?;
    Ok(deleted_count)
}

// =========================================================================
// Context Transitions CRUD
// =========================================================================

pub fn create_context_transition(
    conn: &Connection,
    transition: &super::models::ContextTransition,
) -> Result<super::models::ContextTransition> {
    let trans_type_str = transition.transition_type.as_str();
    conn.execute(
        "INSERT INTO context_transitions (
            id, from_context_id, to_context_id, transition_type, confidence,
            detected_at, session_id, trigger_item_id, trigger_signal_id, metadata, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            transition.id,
            transition.from_context_id,
            transition.to_context_id,
            trans_type_str,
            transition.confidence,
            transition.detected_at,
            transition.session_id,
            transition.trigger_item_id,
            transition.trigger_signal_id,
            transition.metadata,
            transition.created_at,
        ],
    )?;

    get_context_transition(conn, &transition.id)
}

pub fn get_context_transition(
    conn: &Connection,
    id: &str,
) -> Result<super::models::ContextTransition> {
    let mut stmt = conn.prepare(
        "SELECT id, from_context_id, to_context_id, transition_type, confidence,
                detected_at, session_id, trigger_item_id, trigger_signal_id, metadata, created_at
         FROM context_transitions WHERE id = ?1",
    )?;

    stmt.query_row(params![id], |row| {
        let raw_type: String = row.get(3)?;
        let transition_type = super::models::ContextTransitionType::from_str_lenient(&raw_type)
            .unwrap_or(super::models::ContextTransitionType::ContextSwitch);

        Ok(super::models::ContextTransition {
            id: row.get(0)?,
            from_context_id: row.get(1)?,
            to_context_id: row.get(2)?,
            transition_type,
            confidence: row.get(4)?,
            detected_at: row.get(5)?,
            session_id: row.get(6)?,
            trigger_item_id: row.get(7)?,
            trigger_signal_id: row.get(8)?,
            metadata: row.get(9)?,
            created_at: row.get(10)?,
        })
    })
}

pub fn list_context_transitions(
    conn: &Connection,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<Vec<super::models::ContextTransition>> {
    let limit_val = limit.unwrap_or(100) as i64;
    let offset_val = offset.unwrap_or(0) as i64;

    let mut stmt = conn.prepare(
        "SELECT id, from_context_id, to_context_id, transition_type, confidence,
                detected_at, session_id, trigger_item_id, trigger_signal_id, metadata, created_at
         FROM context_transitions
         ORDER BY detected_at DESC
         LIMIT ?1 OFFSET ?2",
    )?;

    let rows = stmt.query_map(params![limit_val, offset_val], |row| {
        let raw_type: String = row.get(3)?;
        let transition_type = super::models::ContextTransitionType::from_str_lenient(&raw_type)
            .unwrap_or(super::models::ContextTransitionType::ContextSwitch);

        Ok(super::models::ContextTransition {
            id: row.get(0)?,
            from_context_id: row.get(1)?,
            to_context_id: row.get(2)?,
            transition_type,
            confidence: row.get(4)?,
            detected_at: row.get(5)?,
            session_id: row.get(6)?,
            trigger_item_id: row.get(7)?,
            trigger_signal_id: row.get(8)?,
            metadata: row.get(9)?,
            created_at: row.get(10)?,
        })
    })?;

    let mut transitions = Vec::new();
    for r in rows {
        transitions.push(r?);
    }
    Ok(transitions)
}

pub fn list_context_transitions_for_context(
    conn: &Connection,
    context_id: &str,
) -> Result<Vec<super::models::ContextTransition>> {
    let mut stmt = conn.prepare(
        "SELECT id, from_context_id, to_context_id, transition_type, confidence,
                detected_at, session_id, trigger_item_id, trigger_signal_id, metadata, created_at
         FROM context_transitions
         WHERE from_context_id = ?1 OR to_context_id = ?1
         ORDER BY detected_at DESC",
    )?;

    let rows = stmt.query_map(params![context_id], |row| {
        let raw_type: String = row.get(3)?;
        let transition_type = super::models::ContextTransitionType::from_str_lenient(&raw_type)
            .unwrap_or(super::models::ContextTransitionType::ContextSwitch);

        Ok(super::models::ContextTransition {
            id: row.get(0)?,
            from_context_id: row.get(1)?,
            to_context_id: row.get(2)?,
            transition_type,
            confidence: row.get(4)?,
            detected_at: row.get(5)?,
            session_id: row.get(6)?,
            trigger_item_id: row.get(7)?,
            trigger_signal_id: row.get(8)?,
            metadata: row.get(9)?,
            created_at: row.get(10)?,
        })
    })?;

    let mut transitions = Vec::new();
    for r in rows {
        transitions.push(r?);
    }
    Ok(transitions)
}

pub fn list_context_transitions_for_session(
    conn: &Connection,
    session_id: &str,
) -> Result<Vec<super::models::ContextTransition>> {
    let mut stmt = conn.prepare(
        "SELECT id, from_context_id, to_context_id, transition_type, confidence,
                detected_at, session_id, trigger_item_id, trigger_signal_id, metadata, created_at
         FROM context_transitions
         WHERE session_id = ?1
         ORDER BY detected_at ASC",
    )?;

    let rows = stmt.query_map(params![session_id], |row| {
        let raw_type: String = row.get(3)?;
        let transition_type = super::models::ContextTransitionType::from_str_lenient(&raw_type)
            .unwrap_or(super::models::ContextTransitionType::ContextSwitch);

        Ok(super::models::ContextTransition {
            id: row.get(0)?,
            from_context_id: row.get(1)?,
            to_context_id: row.get(2)?,
            transition_type,
            confidence: row.get(4)?,
            detected_at: row.get(5)?,
            session_id: row.get(6)?,
            trigger_item_id: row.get(7)?,
            trigger_signal_id: row.get(8)?,
            metadata: row.get(9)?,
            created_at: row.get(10)?,
        })
    })?;

    let mut transitions = Vec::new();
    for r in rows {
        transitions.push(r?);
    }
    Ok(transitions)
}

pub fn delete_context_transition(
    conn: &Connection,
    id: &str,
) -> Result<()> {
    conn.execute("DELETE FROM context_transitions WHERE id = ?1", params![id])?;
    Ok(())
}

// =========================================================================
// Task Candidates CRUD (Inferred Contextual Tasks)
// =========================================================================

pub fn create_task_candidate(
    conn: &Connection,
    candidate: &super::models::TaskCandidate,
) -> Result<super::models::TaskCandidate> {
    let status_str = candidate.status.as_str();
    conn.execute(
        "INSERT INTO task_candidates (
            id, context_id, title, confidence, status,
            first_detected_at, last_active_at, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            candidate.id,
            candidate.context_id,
            candidate.title,
            candidate.confidence,
            status_str,
            candidate.first_detected_at,
            candidate.last_active_at,
            candidate.created_at,
            candidate.updated_at,
        ],
    )?;

    get_task_candidate(conn, &candidate.id)
}

pub fn get_task_candidate(
    conn: &Connection,
    id: &str,
) -> Result<super::models::TaskCandidate> {
    let mut stmt = conn.prepare(
        "SELECT id, context_id, title, confidence, status,
                first_detected_at, last_active_at, created_at, updated_at
         FROM task_candidates WHERE id = ?1",
    )?;

    stmt.query_row(params![id], |row| {
        let raw_status: String = row.get(4)?;
        let status = super::models::TaskCandidateStatus::from_str_lenient(&raw_status)
            .unwrap_or(super::models::TaskCandidateStatus::Detected);

        Ok(super::models::TaskCandidate {
            id: row.get(0)?,
            context_id: row.get(1)?,
            title: row.get(2)?,
            confidence: row.get(3)?,
            status,
            first_detected_at: row.get(5)?,
            last_active_at: row.get(6)?,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
        })
    })
}

pub fn list_task_candidates(
    conn: &Connection,
    context_id_filter: Option<&str>,
    status_filter: Option<&str>,
) -> Result<Vec<super::models::TaskCandidate>> {
    let mut sql = "SELECT id, context_id, title, confidence, status,
                          first_detected_at, last_active_at, created_at, updated_at
                   FROM task_candidates WHERE 1=1".to_string();

    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(cid) = context_id_filter {
        sql.push_str(" AND context_id = ?");
        params_vec.push(Box::new(cid.to_string()));
    }

    if let Some(st) = status_filter {
        sql.push_str(" AND status = ?");
        params_vec.push(Box::new(st.to_string()));
    }

    sql.push_str(" ORDER BY last_active_at DESC, created_at DESC");

    let mut stmt = conn.prepare(&sql)?;
    let params_slice: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

    let rows = stmt.query_map(rusqlite::params_from_iter(params_slice), |row| {
        let raw_status: String = row.get(4)?;
        let status = super::models::TaskCandidateStatus::from_str_lenient(&raw_status)
            .unwrap_or(super::models::TaskCandidateStatus::Detected);

        Ok(super::models::TaskCandidate {
            id: row.get(0)?,
            context_id: row.get(1)?,
            title: row.get(2)?,
            confidence: row.get(3)?,
            status,
            first_detected_at: row.get(5)?,
            last_active_at: row.get(6)?,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
        })
    })?;

    let mut candidates = Vec::new();
    for r in rows {
        candidates.push(r?);
    }
    Ok(candidates)
}

pub fn list_task_candidates_for_context(
    conn: &Connection,
    context_id: &str,
) -> Result<Vec<super::models::TaskCandidate>> {
    list_task_candidates(conn, Some(context_id), None)
}

pub fn update_task_candidate_status(
    conn: &Connection,
    id: &str,
    status: super::models::TaskCandidateStatus,
) -> Result<super::models::TaskCandidate> {
    let now = chrono::Utc::now().to_rfc3339();
    let status_str = status.as_str();

    conn.execute(
        "UPDATE task_candidates SET status = ?1, updated_at = ?2 WHERE id = ?3",
        params![status_str, now, id],
    )?;

    get_task_candidate(conn, id)
}

pub fn update_task_candidate_activity(
    conn: &Connection,
    id: &str,
    last_active_at: &str,
    confidence: Option<f64>,
) -> Result<super::models::TaskCandidate> {
    let now = chrono::Utc::now().to_rfc3339();

    if let Some(conf) = confidence {
        let clamped = conf.clamp(0.0, 1.0);
        conn.execute(
            "UPDATE task_candidates 
             SET last_active_at = ?1, confidence = ?2, status = CASE WHEN status IN ('detected', 'inactive') THEN 'active' ELSE status END, updated_at = ?3 
             WHERE id = ?4",
            params![last_active_at, clamped, now, id],
        )?;
    } else {
        conn.execute(
            "UPDATE task_candidates 
             SET last_active_at = ?1, status = CASE WHEN status IN ('detected', 'inactive') THEN 'active' ELSE status END, updated_at = ?2 
             WHERE id = ?3",
            params![last_active_at, now, id],
        )?;
    }

    get_task_candidate(conn, id)
}

pub fn update_task_candidate_title(
    conn: &Connection,
    id: &str,
    title: &str,
) -> Result<super::models::TaskCandidate> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE task_candidates SET title = ?1, updated_at = ?2 WHERE id = ?3",
        params![title, now, id],
    )?;

    get_task_candidate(conn, id)
}

pub fn delete_task_candidate(
    conn: &Connection,
    id: &str,
) -> Result<()> {
    conn.execute("DELETE FROM task_candidates WHERE id = ?1", params![id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }
    
    #[test]
    fn test_workspace_profile_recalculation() {
        let conn = setup_db();
        
        let ws = create_workspace(&conn, "Test WS").unwrap();
        
        // Add item 1
        let i1 = create_item(&conn, Some(&ws.id), None, "text", "c1", None, "{}").unwrap();
        save_item_embedding(&conn, &i1.id, &[1.0, 0.0, 0.0], "test_prov", "test_model").unwrap();
        
        // Add item 2
        let i2 = create_item(&conn, Some(&ws.id), None, "text", "c2", None, "{}").unwrap();
        save_item_embedding(&conn, &i2.id, &[0.0, 1.0, 0.0], "test_prov", "test_model").unwrap();
        
        // Add user_action for i2
        let payload = serde_json::json!({
            "action": "moved_to_workspace",
            "destination_workspace": ws.id
        }).to_string();
        crate::context_engine::operations::create_context_signal(
            &conn,
            None,
            Some(&i2.id),
            "user_action",
            None,
            &payload
        ).unwrap();
        
        // Recalculate manually (normally done by save_item_embedding hook or move_item hook)
        let profile = recalculate_workspace_profile(&conn, &ws.id).unwrap().unwrap();
        
        // i1 weight = 1.0, i2 weight = 2.0. Total = 3.0.
        // i1 vector = [1, 0, 0]
        // i2 vector = [0, 1, 0] * 2 = [0, 2, 0]
        // sum = [1, 2, 0] / 3 = [0.333, 0.666, 0]
        
        assert!((profile.embedding[0] - 0.3333333).abs() < 0.0001);
        assert!((profile.embedding[1] - 0.6666667).abs() < 0.0001);
        assert_eq!(profile.embedding[2], 0.0);
    }
    
    #[test]
    fn test_semantic_status_lifecycle() {
        let conn = setup_db();
        
        let item = create_item(&conn, None, None, "text", "c1", None, "{}").unwrap();
        
        let status = get_item_semantic_status(&conn, &item.id).unwrap().unwrap();
        assert_eq!(status.status, "pending");
        assert_eq!(status.retries, 0);
        
        update_item_semantic_status(&conn, &item.id, "processing", None).unwrap();
        let status2 = get_item_semantic_status(&conn, &item.id).unwrap().unwrap();
        assert_eq!(status2.status, "processing");
        
        increment_semantic_retries(&conn, &item.id).unwrap();
        increment_semantic_retries(&conn, &item.id).unwrap();
        update_item_semantic_status(&conn, &item.id, "failed", None).unwrap();
        
        let status3 = get_item_semantic_status(&conn, &item.id).unwrap().unwrap();
        assert_eq!(status3.status, "failed");
        assert_eq!(status3.retries, 2);
        
        let failed_items = get_pending_or_failed_items(&conn, 3).unwrap();
        assert_eq!(failed_items.len(), 1);
        assert_eq!(failed_items[0], item.id);
        
        // At max retries
        let failed_items_max = get_pending_or_failed_items(&conn, 2).unwrap();
        assert_eq!(failed_items_max.len(), 0);
    }

    #[test]
    fn test_session_lifecycle() {
        let conn = setup_db();
        
        // Create session
        let session = create_session(&conn).unwrap();
        assert_eq!(session.status, "active");
        assert!(session.ended_at.is_none());
        
        // Update activity
        let updated = update_session_activity(&conn, &session.id).unwrap();
        assert_ne!(session.last_activity_at, updated.last_activity_at);
        
        // End session
        let ended = end_session(&conn, &session.id).unwrap();
        assert_eq!(ended.status, "ended");
        assert!(ended.ended_at.is_some());
        
        // List sessions
        let sessions = list_sessions(&conn).unwrap();
        assert_eq!(sessions.len(), 1);
    }

    #[test]
    fn test_item_with_session() {
        let conn = setup_db();
        let session = create_session(&conn).unwrap();
        
        let item = create_item(
            &conn,
            None,
            Some(&session.id),
            "text",
            "Hello World",
            None,
            "{}",
        ).unwrap();
        
        assert_eq!(item.session_id, Some(session.id.clone()));
        
        let retrieved = get_item(&conn, &item.id).unwrap();
        assert_eq!(retrieved.session_id, Some(session.id));
    }

    #[test]
    fn test_item_relationships() {
        let conn = setup_db();
        
        let item1 = create_item(&conn, None, None, "text", "Content 1", None, "{}").unwrap();
        let item2 = create_item(&conn, None, None, "text", "Content 2", None, "{}").unwrap();
        
        // 1. Create a relationship (item2 first, then item1 to test sorting)
        let rel = create_item_relationship(
            &conn,
            &item2.id,
            &item1.id,
            "temporal_proximity",
            0.8,
            Some("{\"time_diff_secs\": 10}"),
        ).unwrap();
        
        // Verify lexicographical sort
        let expected_a = if item1.id < item2.id { &item1.id } else { &item2.id };
        let expected_b = if item1.id < item2.id { &item2.id } else { &item1.id };
        
        assert_eq!(rel.item_a_id, *expected_a);
        assert_eq!(rel.item_b_id, *expected_b);
        assert_eq!(rel.relationship_type, "temporal_proximity");
        assert_eq!(rel.strength, 0.8);
        
        // 2. Fetch for item1
        let rels_for_1 = get_item_relationships(&conn, &item1.id).unwrap();
        assert_eq!(rels_for_1.len(), 1);
        assert_eq!(rels_for_1[0].id, rel.id);
        
        // 3. Fetch for item2
        let rels_for_2 = get_item_relationships(&conn, &item2.id).unwrap();
        assert_eq!(rels_for_2.len(), 1);
        assert_eq!(rels_for_2[0].id, rel.id);
        
        // 4. Duplicate should fail (UNIQUE constraint)
        let dup_result = create_item_relationship(
            &conn,
            &item1.id,
            &item2.id,
            "temporal_proximity",
            0.9,
            None,
        );
        assert!(dup_result.is_err(), "Duplicate relationship types between the same items should fail");
        
        // 5. Delete relationship
        delete_item_relationship(&conn, &rel.id).unwrap();
        let rels_after_delete = get_item_relationships(&conn, &item1.id).unwrap();
        assert_eq!(rels_after_delete.len(), 0);
    }

    #[test]
    fn test_context_membership_crud() {
        let conn = setup_db();
        let ws = create_workspace(&conn, "Design System").unwrap();
        let item = create_item(&conn, None, None, "text", "Button Component", None, "{}").unwrap();

        // 1. Create a suggested membership from context_engine
        let mem = create_or_update_membership(
            &conn,
            &item.id,
            &ws.id,
            "suggested",
            0.85,
            "context_engine",
        ).unwrap();

        assert_eq!(mem.item_id, item.id);
        assert_eq!(mem.workspace_id, ws.id);
        assert_eq!(mem.membership_type, "suggested");
        assert_eq!(mem.confidence, 0.85);
        assert_eq!(mem.source, "context_engine");

        // 2. Fetch by ID
        let fetched = get_membership(&conn, &mem.id).unwrap();
        assert_eq!(fetched, mem);

        // 3. Fetch by item & workspace
        let item_ws_mem = get_item_workspace_membership(&conn, &item.id, &ws.id).unwrap();
        assert_eq!(item_ws_mem.as_ref(), Some(&mem));

        // 4. List by item
        let item_mems = list_item_memberships(&conn, &item.id).unwrap();
        assert_eq!(item_mems.len(), 1);
        assert_eq!(item_mems[0].id, mem.id);

        // 5. List by workspace with filter
        let ws_suggested = list_workspace_memberships(&conn, &ws.id, Some("suggested")).unwrap();
        assert_eq!(ws_suggested.len(), 1);
        let ws_confirmed = list_workspace_memberships(&conn, &ws.id, Some("confirmed")).unwrap();
        assert_eq!(ws_confirmed.len(), 0);

        // 6. Delete membership
        delete_membership(&conn, &mem.id).unwrap();
        let after_delete = get_item_workspace_membership(&conn, &item.id, &ws.id).unwrap();
        assert!(after_delete.is_none());
    }

    #[test]
    fn test_suggested_vs_confirmed_membership_isolation() {
        let conn = setup_db();
        let ws = create_workspace(&conn, "Research").unwrap();
        let item = create_item(&conn, None, None, "text", "Paper draft", None, "{}").unwrap();

        // Adding a suggested membership must NOT move or alter the item's workspace_id
        create_or_update_membership(
            &conn,
            &item.id,
            &ws.id,
            "suggested",
            0.78,
            "context_discovery",
        ).unwrap();

        let fetched_item = get_item(&conn, &item.id).unwrap();
        assert_eq!(fetched_item.workspace_id, None, "Suggested membership must NEVER change item workspace_id");

        // Standard workspace list_items query must still return empty
        let ws_items = list_items(&conn, Some(&ws.id)).unwrap();
        assert_eq!(ws_items.len(), 0, "Suggested items must not appear in standard workspace item list");
    }

    #[test]
    fn test_membership_confirm_promotion() {
        let conn = setup_db();
        let ws = create_workspace(&conn, "Engineering").unwrap();
        let item = create_item(&conn, None, None, "text", "API Spec", None, "{}").unwrap();

        // 1. Initially suggested by context_discovery
        create_or_update_membership(
            &conn,
            &item.id,
            &ws.id,
            "suggested",
            0.82,
            "context_discovery",
        ).unwrap();

        // 2. User confirms the membership
        let confirmed = confirm_membership(&conn, &item.id, &ws.id).unwrap();
        assert_eq!(confirmed.membership_type, "confirmed");
        assert_eq!(confirmed.confidence, 1.0);
        assert_eq!(confirmed.source, "user");

        let ws_confirmed = list_workspace_memberships(&conn, &ws.id, Some("confirmed")).unwrap();
        assert_eq!(ws_confirmed.len(), 1);
        let ws_suggested = list_workspace_memberships(&conn, &ws.id, Some("suggested")).unwrap();
        assert_eq!(ws_suggested.len(), 0);
    }

    #[test]
    fn test_membership_cascade_deletions() {
        let conn = setup_db();
        let ws = create_workspace(&conn, "Temp WS").unwrap();
        let item = create_item(&conn, None, None, "text", "Temp note", None, "{}").unwrap();

        create_or_update_membership(&conn, &item.id, &ws.id, "suggested", 0.9, "context_engine").unwrap();
        assert_eq!(list_item_memberships(&conn, &item.id).unwrap().len(), 1);

        // Delete item -> cascades to context_memberships
        delete_item(&conn, &item.id).unwrap();
        assert_eq!(list_workspace_memberships(&conn, &ws.id, None).unwrap().len(), 0);

        // Recreate item and workspace to test workspace delete cascade
        let item2 = create_item(&conn, None, None, "text", "Temp note 2", None, "{}").unwrap();
        create_or_update_membership(&conn, &item2.id, &ws.id, "confirmed", 1.0, "user").unwrap();

        delete_workspace(&conn, &ws.id).unwrap();
        assert_eq!(list_item_memberships(&conn, &item2.id).unwrap().len(), 0);
    }

    #[test]
    fn test_physical_organization_vs_contextual_association() {
        let conn = setup_db();

        // 1. Inbox Item with suggested membership
        // Item: Paris restaurant
        // Physical location: Inbox
        // Context membership: Paris Trip (suggested, 87%)
        let ws_paris = create_workspace(&conn, "Paris Trip").unwrap();
        let item_restaurant = create_item(&conn, None, None, "url", "https://le-gabriel.paris", Some("Paris restaurant"), "{}").unwrap();
        
        // Physical location must remain in Inbox
        assert_eq!(item_restaurant.workspace_id, None);
        assert_eq!(list_items(&conn, None).unwrap().iter().filter(|i| i.id == item_restaurant.id).count(), 1);
        assert_eq!(list_items(&conn, Some(&ws_paris.id)).unwrap().len(), 0);

        // Add suggested membership from Context Engine with 87% confidence
        let suggested_mem = create_or_update_membership(
            &conn,
            &item_restaurant.id,
            &ws_paris.id,
            "suggested",
            0.87,
            "context_engine",
        ).unwrap();
        assert_eq!(suggested_mem.membership_type, "suggested");
        assert_eq!(suggested_mem.confidence, 0.87);
        assert_eq!(suggested_mem.source, "context_engine");

        // Verify physical location is STILL in Inbox (user is not forced to move item)
        let item_still_inbox = get_item(&conn, &item_restaurant.id).unwrap();
        assert_eq!(item_still_inbox.workspace_id, None);
        assert_eq!(list_items(&conn, Some(&ws_paris.id)).unwrap().len(), 0);

        // Contextual membership is accessible via membership queries
        let ws_suggested = list_workspace_memberships(&conn, &ws_paris.id, Some("suggested")).unwrap();
        assert_eq!(ws_suggested.len(), 1);
        assert_eq!(ws_suggested[0].item_id, item_restaurant.id);
        assert_eq!(ws_suggested[0].confidence, 0.87);

        // 2. Confirmed Workspace Item
        // Item: Flight to Paris
        // Physical location: Paris Trip
        // Context membership: Paris Trip (confirmed, 100%, user)
        let item_flight = create_item(&conn, Some(&ws_paris.id), None, "url", "https://airfrance.com/flight", Some("Flight to Paris"), "{}").unwrap();
        assert_eq!(item_flight.workspace_id.as_deref(), Some(ws_paris.id.as_str()));
        
        // Appears in Paris Trip workspace items list
        let ws_paris_items = list_items(&conn, Some(&ws_paris.id)).unwrap();
        assert_eq!(ws_paris_items.iter().filter(|i| i.id == item_flight.id).count(), 1);

        // Confirmed membership exists
        let flight_mem = get_item_workspace_membership(&conn, &item_flight.id, &ws_paris.id).unwrap().unwrap();
        assert_eq!(flight_mem.membership_type, "confirmed");
        assert_eq!(flight_mem.confidence, 1.0);
        assert_eq!(flight_mem.source, "user");

        // 3. Changing a confirmed Workspace
        let ws_europe = create_workspace(&conn, "Europe Vacation").unwrap();
        let moved_flight = move_item(&conn, &item_flight.id, Some(&ws_europe.id)).unwrap();
        assert_eq!(moved_flight.workspace_id.as_deref(), Some(ws_europe.id.as_str()));

        // Old workspace no longer contains the item
        assert_eq!(list_items(&conn, Some(&ws_paris.id)).unwrap().iter().filter(|i| i.id == item_flight.id).count(), 0);
        assert!(get_item_workspace_membership(&conn, &item_flight.id, &ws_paris.id).unwrap().is_none());

        // New workspace contains the item and has confirmed membership
        assert_eq!(list_items(&conn, Some(&ws_europe.id)).unwrap().iter().filter(|i| i.id == item_flight.id).count(), 1);
        let europe_mem = get_item_workspace_membership(&conn, &item_flight.id, &ws_europe.id).unwrap().unwrap();
        assert_eq!(europe_mem.membership_type, "confirmed");
        assert_eq!(europe_mem.source, "user");

        // 4. Dismissing a suggestion
        let dismissed_mem = dismiss_membership(&conn, &item_restaurant.id, &ws_paris.id).unwrap();
        assert_eq!(dismissed_mem.membership_type, "dismissed");
        assert_eq!(dismissed_mem.confidence, 0.0);
        assert_eq!(dismissed_mem.source, "user");

        // Item still physically in Inbox
        let item_after_dismiss = get_item(&conn, &item_restaurant.id).unwrap();
        assert_eq!(item_after_dismiss.workspace_id, None);

        // No longer in suggested memberships list
        let active_suggestions = list_workspace_memberships(&conn, &ws_paris.id, Some("suggested")).unwrap();
        assert_eq!(active_suggestions.len(), 0);

        // Feedback signal recorded for future engine learning
        let signals = crate::context_engine::operations::list_context_signals(&conn, Some(&ws_paris.id), Some(&item_restaurant.id)).unwrap();
        assert_eq!(signals.iter().filter(|s| s.signal_type == "membership_dismissed").count(), 1);
    }

    #[test]
    fn test_context_workspace_lifecycle_states() {
        let conn = setup_db();
        
        // 1. Initial creation: workspace is Active
        let ws = create_workspace(&conn, "Paris Research").unwrap();
        assert_eq!(ws.lifecycle_state, "active");
        assert!(ws.last_activity_at.is_some());
        assert!(ws.archived_at.is_none());

        // 2. Add an item: confirmed in workspace
        let item = create_item(&conn, Some(&ws.id), None, "text", "Eiffel Tower tickets", Some("Tickets"), "{}").unwrap();
        assert_eq!(item.workspace_id.as_deref(), Some(ws.id.as_str()));

        // 3. Simulating inactivity: update last_activity_at to 45 days ago
        let old_time = (chrono::Utc::now() - chrono::Duration::days(45)).to_rfc3339();
        conn.execute("UPDATE workspaces SET last_activity_at = ?1 WHERE id = ?2", rusqlite::params![old_time, ws.id]).unwrap();

        // 4. Lifecycle evaluation identifies workspace as Dormant (no recent activity, but remains available)
        let state = evaluate_workspace_lifecycle(&conn, &ws.id, Some(30.0)).unwrap();
        assert_eq!(state, "dormant");

        let ws_dormant = get_workspace(&conn, &ws.id).unwrap();
        assert_eq!(ws_dormant.lifecycle_state, "dormant");

        // Rule: Do not delete data
        let item_still_exists = get_item(&conn, &item.id).unwrap();
        assert_eq!(item_still_exists.workspace_id.as_deref(), Some(ws.id.as_str()));

        // 5. Meaningful user activity wakes dormant workspace back to Active
        let new_item = create_item(&conn, Some(&ws.id), None, "text", "Louvre itinerary", Some("Louvre"), "{}").unwrap();
        let ws_active_again = get_workspace(&conn, &ws.id).unwrap();
        assert_eq!(ws_active_again.lifecycle_state, "active");
        assert_eq!(new_item.workspace_id.as_deref(), Some(ws.id.as_str()));

        // 6. User explicitly archives workspace
        let ws_archived = archive_workspace(&conn, &ws.id).unwrap();
        assert_eq!(ws_archived.lifecycle_state, "archived");
        assert!(ws_archived.archived_at.is_some());

        // Rule: Do not automatically archive or unarchive user contexts
        // Even if evaluation runs, archived remains archived
        let eval_archived = evaluate_workspace_lifecycle(&conn, &ws.id, Some(30.0)).unwrap();
        assert_eq!(eval_archived, "archived");

        // Rule: User-confirmed workspace assignment continues working authoritatively
        let item_in_archived = create_item(&conn, Some(&ws.id), None, "text", "Post-archive note", Some("Note"), "{}").unwrap();
        assert_eq!(item_in_archived.workspace_id.as_deref(), Some(ws.id.as_str()));

        // 7. Explicit unarchive restores active state
        let ws_unarchived = unarchive_workspace(&conn, &ws.id).unwrap();
        assert_eq!(ws_unarchived.lifecycle_state, "active");
        assert!(ws_unarchived.archived_at.is_none());
    }

    #[test]
    fn test_inbox_item_state_crud_and_cascade() {
        let conn = setup_db();
        let item = create_item(&conn, None, None, "text", "Inbox Test Item", None, "{}").unwrap();

        // 1. Verify automatic creation of New state on capture to inbox
        let state = get_inbox_item_state(&conn, &item.id).unwrap().unwrap();
        assert_eq!(state.item_id, item.id);
        assert_eq!(state.state, InboxState::New);
        assert_eq!(state.previous_state, None);

        // 2. Fetch list of states
        let all_states = list_inbox_item_states(&conn, None).unwrap();
        assert_eq!(all_states.len(), 1);

        let new_states = list_inbox_item_states(&conn, Some(InboxState::New)).unwrap();
        assert_eq!(new_states.len(), 1);

        let organized_states = list_inbox_item_states(&conn, Some(InboxState::Organized)).unwrap();
        assert_eq!(organized_states.len(), 0);

        // 3. Delete item and verify cascade deletion in inbox_item_states
        delete_item(&conn, &item.id).unwrap();
        let state_after_delete = get_inbox_item_state(&conn, &item.id).unwrap();
        assert!(state_after_delete.is_none());
    }

    #[test]
    fn test_inbox_item_state_transitions_and_history() {
        let conn = setup_db();
        let item = create_item(&conn, None, None, "text", "Design tokens", None, "{}").unwrap();

        // 1. New -> Processing
        let s_proc = transition_inbox_item_state(
            &conn,
            &item.id,
            InboxState::Processing,
            Some("Analyzing signals"),
        ).unwrap();
        assert_eq!(s_proc.state, InboxState::Processing);
        assert_eq!(s_proc.previous_state, Some(InboxState::New));
        assert_eq!(s_proc.reason.as_deref(), Some("Analyzing signals"));

        // 2. Processing -> Understood
        let s_under = transition_inbox_item_state(
            &conn,
            &item.id,
            InboxState::Understood,
            Some("Features extracted"),
        ).unwrap();
        assert_eq!(s_under.state, InboxState::Understood);
        assert_eq!(s_under.previous_state, Some(InboxState::Processing));

        // 3. Understood -> Suggested
        let s_sugg = transition_inbox_item_state(
            &conn,
            &item.id,
            InboxState::Suggested,
            Some("Workspace match: Design System"),
        ).unwrap();
        assert_eq!(s_sugg.state, InboxState::Suggested);
        assert_eq!(s_sugg.previous_state, Some(InboxState::Understood));

        // 4. Suggested -> Organized (User filing)
        let ws = create_workspace(&conn, "Design System").unwrap();
        let s_org = transition_inbox_item_state(
            &conn,
            &item.id,
            InboxState::Organized,
            Some(&format!("Filed to {}", ws.id)),
        ).unwrap();
        assert_eq!(s_org.state, InboxState::Organized);
        assert_eq!(s_org.previous_state, Some(InboxState::Suggested));
    }

    #[test]
    fn test_inbox_item_state_invalid_transitions_rejected() {
        let conn = setup_db();
        let item = create_item(&conn, None, None, "text", "Raw item", None, "{}").unwrap();

        // Invalid: New -> Suggested (cannot skip Processing & Understood)
        let invalid1 = transition_inbox_item_state(&conn, &item.id, InboxState::Suggested, None);
        assert!(invalid1.is_err());
        assert!(invalid1.unwrap_err().contains("Invalid state transition"));

        // Invalid: New -> NeedsReview (cannot skip Processing & Understood)
        let invalid2 = transition_inbox_item_state(&conn, &item.id, InboxState::NeedsReview, None);
        assert!(invalid2.is_err());
    }

    #[test]
    fn test_inbox_move_item_transitions_state() {
        let conn = setup_db();
        let item = create_item(&conn, None, None, "text", "Component code", None, "{}").unwrap();
        let ws = create_workspace(&conn, "Frontend").unwrap();

        let initial_state = get_inbox_item_state(&conn, &item.id).unwrap().unwrap();
        assert_eq!(initial_state.state, InboxState::New);

        // Move to workspace -> state becomes Organized
        move_item(&conn, &item.id, Some(&ws.id)).unwrap();
        let organized_state = get_inbox_item_state(&conn, &item.id).unwrap().unwrap();
        assert_eq!(organized_state.state, InboxState::Organized);
        assert_eq!(organized_state.previous_state, Some(InboxState::New));

        // Move back to inbox -> state becomes New
        move_item(&conn, &item.id, None).unwrap();
        let returned_state = get_inbox_item_state(&conn, &item.id).unwrap().unwrap();
        assert_eq!(returned_state.state, InboxState::New);
        assert_eq!(returned_state.previous_state, Some(InboxState::Organized));
    }

    #[test]
    fn test_bulk_assign_inbox_items_success() {
        let mut conn = setup_db();
        let ws = create_workspace(&conn, "Product Roadmap").unwrap();

        // 1. Create 3 inbox items with specific capture metadata
        let meta1 = serde_json::json!({"app_name": "Slack", "source": "chat", "custom_tag": "v1"}).to_string();
        let item1 = create_item(&conn, None, None, "text", "Feature spec 1", Some("Spec 1"), &meta1).unwrap();
        
        let meta2 = serde_json::json!({"app_name": "Chrome", "source": "browser", "custom_tag": "v2"}).to_string();
        let item2 = create_item(&conn, None, None, "url", "https://example.com/spec", Some("Web Spec"), &meta2).unwrap();
        
        let meta3 = serde_json::json!({"app_name": "Terminal", "source": "cli"}).to_string();
        let item3 = create_item(&conn, None, None, "text", "Deploy script", Some("Script"), &meta3).unwrap();

        // 2. Establish an item relationship between item 1 and item 2
        create_item_relationship(&conn, &item1.id, &item2.id, "same_session", 0.95, Some("{}")).unwrap();
        
        // 3. Establish a pre-existing signal
        let pre_sig_payload = serde_json::json!({"event": "captured"}).to_string();
        crate::context_engine::operations::create_context_signal(
            &conn,
            None,
            Some(&item1.id),
            "capture_event",
            None,
            &pre_sig_payload
        ).unwrap();

        let item_ids = vec![item1.id.clone(), item2.id.clone(), item3.id.clone()];

        // 4. Perform bulk assignment to workspace
        let assigned = bulk_assign_inbox_items_to_workspace(&mut conn, &item_ids, &ws.id).unwrap();
        assert_eq!(assigned.len(), 3);

        // 5. Verify all items are now assigned to the workspace and metadata is fully preserved
        for item in &assigned {
            assert_eq!(item.workspace_id.as_deref(), Some(ws.id.as_str()));
        }

        let refetched1 = get_item(&conn, &item1.id).unwrap();
        assert_eq!(refetched1.workspace_id.as_deref(), Some(ws.id.as_str()));
        assert_eq!(refetched1.content, "Feature spec 1");
        assert_eq!(refetched1.title.as_deref(), Some("Spec 1"));
        assert_eq!(refetched1.metadata, meta1);
        assert_eq!(refetched1.created_at, item1.created_at);

        let refetched2 = get_item(&conn, &item2.id).unwrap();
        assert_eq!(refetched2.workspace_id.as_deref(), Some(ws.id.as_str()));
        assert_eq!(refetched2.metadata, meta2);

        // 6. Verify confirmed ContextMemberships were created
        let memberships = list_workspace_memberships(&conn, &ws.id, Some("confirmed")).unwrap();
        assert_eq!(memberships.len(), 3);
        for m in &memberships {
            assert_eq!(m.membership_type, "confirmed");
            assert_eq!(m.confidence, 1.0);
            assert_eq!(m.source, "user");
        }

        // 7. Verify InboxItemStates are transitioned to Organized
        for id in &item_ids {
            let state = get_inbox_item_state(&conn, id).unwrap().unwrap();
            assert_eq!(state.state, InboxState::Organized);
        }

        // 8. Verify ItemRelationships were preserved
        let rels = get_item_relationships(&conn, &item1.id).unwrap();
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].relationship_type, "same_session");

        // 9. Verify ContextSignals were recorded
        let signals = crate::context_engine::operations::list_context_signals(&conn, Some(&ws.id), None).unwrap();
        assert!(signals.iter().any(|s| s.signal_type == "user_action"));
    }

    #[test]
    fn test_bulk_assign_inbox_items_partial_failure_rollback() {
        let mut conn = setup_db();
        let ws = create_workspace(&conn, "Design System").unwrap();

        // 1. Create two valid inbox items
        let item1 = create_item(&conn, None, None, "text", "Valid Item 1", None, "{}").unwrap();
        let item2 = create_item(&conn, None, None, "text", "Valid Item 2", None, "{}").unwrap();

        // Transition item1 to Understood
        transition_inbox_item_state(&conn, &item1.id, InboxState::Processing, None).unwrap();
        transition_inbox_item_state(&conn, &item1.id, InboxState::Understood, Some("Parsed")).unwrap();

        // 2. Attempt bulk assignment with an invalid/non-existent item ID in the middle
        let mixed_ids = vec![
            item1.id.clone(),
            "completely_non_existent_item_id_9999".to_string(),
            item2.id.clone(),
        ];

        let result = bulk_assign_inbox_items_to_workspace(&mut conn, &mixed_ids, &ws.id);
        assert!(result.is_err(), "Bulk assignment must fail if any item is invalid");
        let err_msg = result.unwrap_err();
        assert!(err_msg.contains("completely_non_existent_item_id_9999"));

        // 3. Verify complete transaction rollback:
        // Item 1 must still be in inbox (workspace_id = None)
        let refetched1 = get_item(&conn, &item1.id).unwrap();
        assert_eq!(refetched1.workspace_id, None);

        // Item 2 must still be in inbox (workspace_id = None)
        let refetched2 = get_item(&conn, &item2.id).unwrap();
        assert_eq!(refetched2.workspace_id, None);

        // Item 1 state must NOT have become Organized, must still be Understood
        let state1 = get_inbox_item_state(&conn, &item1.id).unwrap().unwrap();
        assert_eq!(state1.state, InboxState::Understood);

        // Zero confirmed memberships must exist for the workspace
        let memberships = list_workspace_memberships(&conn, &ws.id, None).unwrap();
        assert_eq!(memberships.len(), 0);

        // Zero context signals must exist for the workspace
        let signals = crate::context_engine::operations::list_context_signals(&conn, Some(&ws.id), None).unwrap();
        assert_eq!(signals.len(), 0);
    }

    #[test]
    fn test_bulk_assign_inbox_items_invalid_workspace_rollback() {
        let mut conn = setup_db();
        let item1 = create_item(&conn, None, None, "text", "Item A", None, "{}").unwrap();
        let item2 = create_item(&conn, None, None, "text", "Item B", None, "{}").unwrap();

        let item_ids = vec![item1.id.clone(), item2.id.clone()];
        let result = bulk_assign_inbox_items_to_workspace(&mut conn, &item_ids, "non_existent_ws_id");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Destination workspace 'non_existent_ws_id' does not exist"));

        // Items remain unassigned
        assert_eq!(get_item(&conn, &item1.id).unwrap().workspace_id, None);
        assert_eq!(get_item(&conn, &item2.id).unwrap().workspace_id, None);
    }

    #[test]
    fn test_bulk_dismiss_inbox_suggestions() {
        let mut conn = setup_db();
        let ws = create_workspace(&conn, "Marketing").unwrap();

        let item1 = create_item(&conn, None, None, "text", "Ad copy 1", None, "{}").unwrap();
        let item2 = create_item(&conn, None, None, "text", "Ad copy 2", None, "{}").unwrap();

        let item_ids = vec![item1.id.clone(), item2.id.clone()];
        bulk_dismiss_inbox_suggestions(&mut conn, &item_ids, Some(&ws.id)).unwrap();

        // States are Understood
        for id in &item_ids {
            let state = get_inbox_item_state(&conn, id).unwrap().unwrap();
            assert_eq!(state.state, InboxState::Understood);
        }

        // Dismissed memberships exist
        let memberships = list_workspace_memberships(&conn, &ws.id, Some("dismissed")).unwrap();
        assert_eq!(memberships.len(), 2);
    }

    #[test]
    fn test_bulk_archive_or_clear_inbox_items() {
        let mut conn = setup_db();
        let ws = create_workspace(&conn, "Important").unwrap();

        let inbox1 = create_item(&conn, None, None, "text", "Temporary 1", None, "{}").unwrap();
        let inbox2 = create_item(&conn, None, None, "text", "Temporary 2", None, "{}").unwrap();
        let ws_item = create_item(&conn, Some(&ws.id), None, "text", "Permanent", None, "{}").unwrap();

        let ids = vec![inbox1.id.clone(), inbox2.id.clone(), ws_item.id.clone()];
        let cleared_count = bulk_archive_or_clear_inbox_items(&mut conn, &ids).unwrap();
        assert_eq!(cleared_count, 2);

        // Inbox items are deleted
        assert!(get_item(&conn, &inbox1.id).is_err());
        assert!(get_item(&conn, &inbox2.id).is_err());

        // Workspace item is kept safe
        assert!(get_item(&conn, &ws_item.id).is_ok());
    }

    #[test]
    fn test_inbox_item_organization_invariants() {
        let conn = setup_db();
        let ws = create_workspace(&conn, "Backend Architecture").unwrap();

        let initial_metadata = serde_json::json!({
            "source_app": "VS Code",
            "window_title": "architecture.md - ContextSplit",
            "url": "file:///path/to/architecture.md",
            "tags": ["rust", "database", "sqlite"]
        })
        .to_string();

        // 1. Create item in Inbox
        let item = create_item(
            &conn,
            None,
            None,
            "text",
            "System architecture notes and memory management guarantees",
            Some("Database Architecture Notes"),
            &initial_metadata,
        )
        .unwrap();

        let companion_item = create_item(
            &conn,
            None,
            None,
            "text",
            "Related SQLite schema documentation",
            Some("Schema Docs"),
            "{}",
        )
        .unwrap();

        // 2. Establish relationship and initial context signals
        let _ = create_item_relationship(
            &conn,
            &item.id,
            &companion_item.id,
            "semantic_similarity",
            0.88,
            Some("sqlite_database"),
        );

        let initial_signal_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let _ = conn.execute(
            "INSERT INTO context_signals (id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at)
             VALUES (?1, NULL, ?2, 'capture_signal', NULL, '{\"source\":\"cli\"}', ?3, ?3)",
            params![initial_signal_id, item.id, now],
        );

        // 3. Initially item is unassigned and in Inbox
        assert_eq!(item.workspace_id, None);
        assert_eq!(list_items(&conn, None).unwrap().iter().filter(|i| i.id == item.id).count(), 1);
        assert_eq!(list_items(&conn, Some(&ws.id)).unwrap().len(), 0);

        // 4. Confirm contextual membership (user accepts organization)
        let confirmed_mem = confirm_membership(&conn, &item.id, &ws.id).unwrap();
        assert_eq!(confirmed_mem.membership_type, "confirmed");
        assert_eq!(confirmed_mem.confidence, 1.0);
        assert_eq!(confirmed_mem.source, "user");

        // -------------------------------------------------------------
        // Invariant 1: Do not delete the Item
        // -------------------------------------------------------------
        let organized_item = get_item(&conn, &item.id).expect("Item must not be deleted");

        // -------------------------------------------------------------
        // Invariant 2: Do not lose capture metadata
        // -------------------------------------------------------------
        assert_eq!(organized_item.metadata, initial_metadata, "Capture metadata must be 100% preserved");
        assert_eq!(organized_item.title.as_deref(), Some("Database Architecture Notes"));
        assert_eq!(organized_item.item_type, "text");

        // -------------------------------------------------------------
        // Invariant 3: Mark it organized in state machine
        // -------------------------------------------------------------
        let state_record = get_inbox_item_state(&conn, &item.id).unwrap().expect("State record exists");
        assert_eq!(state_record.state, InboxState::Organized);

        // -------------------------------------------------------------
        // Invariant 4: Remove it from Needs Attention
        // -------------------------------------------------------------
        let attention = crate::context_engine::attention::evaluate_inbox_item_attention(&conn, &item.id, None).unwrap();
        assert!(!attention.attention_required, "Organized item must NOT require attention");
        assert_eq!(attention.attention_level, crate::context_engine::attention::AttentionLevel::None);
        assert!(attention.factors.is_organized, "Factors must reflect organized status");

        // -------------------------------------------------------------
        // Invariant 5: Preserve relationships and signals
        // -------------------------------------------------------------
        let rels = get_item_relationships(&conn, &item.id).unwrap();
        assert_eq!(rels.len(), 1, "Relationships must be preserved intact");
        assert!(rels[0].item_a_id == companion_item.id || rels[0].item_b_id == companion_item.id);

        let signals = crate::context_engine::operations::list_context_signals(&conn, None, Some(&item.id)).unwrap();
        assert!(signals.iter().any(|s| s.signal_type == "capture_signal"), "Initial capture signal preserved");
        assert!(signals.iter().any(|s| s.signal_type == "user_action"), "Confirmation feedback signal recorded");

        // -------------------------------------------------------------
        // Invariant 6: Keep it accessible through the Workspace
        // -------------------------------------------------------------
        assert_eq!(organized_item.workspace_id.as_deref(), Some(ws.id.as_str()));
        let ws_items = list_items(&conn, Some(&ws.id)).unwrap();
        assert_eq!(ws_items.iter().filter(|i| i.id == item.id).count(), 1, "Item must be accessible in workspace item list");

        // -------------------------------------------------------------
        // Invariant 7: Retain it in searchable history
        // -------------------------------------------------------------
        let search_text_params = crate::search::InboxSearchParams {
            query: Some("Database Architecture".to_string()),
            ..Default::default()
        };
        let search_results = crate::search::search_inbox_items(&conn, &search_text_params, None).unwrap();
        assert!(search_results.iter().any(|r| r.item.id == item.id), "Organized item must be found in text search");

        let search_ws_params = crate::search::InboxSearchParams {
            workspace_id: Some(ws.id.clone()),
            ..Default::default()
        };
        let ws_results = crate::search::search_inbox_items(&conn, &search_ws_params, None).unwrap();
        assert!(ws_results.iter().any(|r| r.item.id == item.id), "Organized item must be found in workspace search filter");

        let search_state_params = crate::search::InboxSearchParams {
            inbox_state: Some("organized".to_string()),
            ..Default::default()
        };
        let state_results = crate::search::search_inbox_items(&conn, &search_state_params, None).unwrap();
        assert!(state_results.iter().any(|r| r.item.id == item.id), "Organized item must be found in organized state filter");
    }

    #[test]
    fn test_context_transition_crud_and_invariants() {
        let conn = setup_db();

        let ws_a = create_workspace(&conn, "Backend Rust").unwrap();
        let ws_b = create_workspace(&conn, "Frontend React").unwrap();
        let session = create_session(&conn).unwrap();

        let item = create_item(&conn, Some(&ws_a.id), Some(&session.id), "code", "fn main() {}", Some("main.rs"), "{}").unwrap();
        let signal = crate::context_engine::operations::create_context_signal(&conn, Some(&ws_a.id), Some(&item.id), "active_app", Some(&session.id), "{\"app_name\":\"VS Code\"}").unwrap();

        // 1. Create ContextSwitch transition
        let switch_trans = super::super::models::ContextTransition::new(
            Some(ws_a.id.clone()),
            Some(ws_b.id.clone()),
            super::super::models::ContextTransitionType::ContextSwitch,
            0.92,
        )
        .with_session(Some(session.id.clone()))
        .with_trigger_item(Some(item.id.clone()))
        .with_trigger_signal(Some(signal.id.clone()))
        .with_metadata("{\"reason\":\"User switched active editor to browser\"}");

        let created_switch = create_context_transition(&conn, &switch_trans).unwrap();
        assert_eq!(created_switch.id, switch_trans.id);
        assert_eq!(created_switch.transition_type, super::super::models::ContextTransitionType::ContextSwitch);
        assert_eq!(created_switch.from_context_id.as_deref(), Some(ws_a.id.as_str()));
        assert_eq!(created_switch.to_context_id.as_deref(), Some(ws_b.id.as_str()));
        assert_eq!(created_switch.confidence, 0.92);
        assert_eq!(created_switch.session_id.as_deref(), Some(session.id.as_str()));
        assert_eq!(created_switch.trigger_item_id.as_deref(), Some(item.id.as_str()));
        assert_eq!(created_switch.trigger_signal_id.as_deref(), Some(signal.id.as_str()));

        // 2. Create remaining 4 transition types
        let return_trans = super::super::models::ContextTransition::new(
            Some(ws_b.id.clone()),
            Some(ws_a.id.clone()),
            super::super::models::ContextTransitionType::ContextReturn,
            0.88,
        )
        .with_session(Some(session.id.clone()));
        let _ = create_context_transition(&conn, &return_trans).unwrap();

        let start_trans = super::super::models::ContextTransition::new(
            None,
            Some(ws_a.id.clone()),
            super::super::models::ContextTransitionType::ContextStart,
            0.95,
        );
        let _ = create_context_transition(&conn, &start_trans).unwrap();

        let end_trans = super::super::models::ContextTransition::new(
            Some(ws_a.id.clone()),
            None,
            super::super::models::ContextTransitionType::ContextEnd,
            1.0,
        );
        let _ = create_context_transition(&conn, &end_trans).unwrap();

        let amb_trans = super::super::models::ContextTransition::new(
            Some(ws_a.id.clone()),
            Some(ws_b.id.clone()),
            super::super::models::ContextTransitionType::AmbiguousTransition,
            0.45,
        );
        let _ = create_context_transition(&conn, &amb_trans).unwrap();

        // 3. List and filter transitions
        let all_transitions = list_context_transitions(&conn, None, None).unwrap();
        assert_eq!(all_transitions.len(), 5);

        let ws_a_transitions = list_context_transitions_for_context(&conn, &ws_a.id).unwrap();
        assert_eq!(ws_a_transitions.len(), 5); // Involved in all 5 (switch, return, start, end, amb)

        let ws_b_transitions = list_context_transitions_for_context(&conn, &ws_b.id).unwrap();
        assert_eq!(ws_b_transitions.len(), 3); // Involved in switch, return, amb

        let session_transitions = list_context_transitions_for_session(&conn, &session.id).unwrap();
        assert_eq!(session_transitions.len(), 2); // switch and return

        // 4. Invariant: Creating transitions must NOT modify Item or Workspace memberships
        let fresh_item = get_item(&conn, &item.id).unwrap();
        assert_eq!(fresh_item.workspace_id.as_deref(), Some(ws_a.id.as_str()), "Item workspace must remain unchanged");
        let memberships = list_item_memberships(&conn, &item.id).unwrap();
        assert_eq!(memberships.len(), 1);
        assert_eq!(memberships[0].workspace_id, ws_a.id);

        // 5. Delete transition
        delete_context_transition(&conn, &switch_trans.id).unwrap();
        assert!(get_context_transition(&conn, &switch_trans.id).is_err());
        let remaining = list_context_transitions(&conn, None, None).unwrap();
        assert_eq!(remaining.len(), 4);
    }

    #[test]
    fn test_task_candidate_model_and_status_invariants() {
        use super::super::models::{TaskCandidate, TaskCandidateStatus};

        // 1. Test status parsing and string roundtrip
        for status in [
            TaskCandidateStatus::Detected,
            TaskCandidateStatus::Active,
            TaskCandidateStatus::Inactive,
            TaskCandidateStatus::Uncertain,
            TaskCandidateStatus::Dismissed,
        ] {
            let s_str = status.as_str();
            assert_eq!(TaskCandidateStatus::from_str(s_str).unwrap(), status);
            assert_eq!(TaskCandidateStatus::from_str_lenient(s_str).unwrap(), status);
        }

        assert_eq!(TaskCandidateStatus::from_str_lenient("new"), Some(TaskCandidateStatus::Detected));
        assert_eq!(TaskCandidateStatus::from_str_lenient("ongoing"), Some(TaskCandidateStatus::Active));
        assert_eq!(TaskCandidateStatus::from_str_lenient("idle"), Some(TaskCandidateStatus::Inactive));
        assert_eq!(TaskCandidateStatus::from_str_lenient("ambiguous"), Some(TaskCandidateStatus::Uncertain));
        assert_eq!(TaskCandidateStatus::from_str_lenient("rejected"), Some(TaskCandidateStatus::Dismissed));

        // 2. Test status helpers
        assert!(TaskCandidateStatus::Active.is_active());
        assert!(!TaskCandidateStatus::Inactive.is_active());
        assert!(TaskCandidateStatus::Dismissed.is_dismissed());
        assert!(TaskCandidateStatus::Uncertain.is_uncertain());

        // 3. Test TaskCandidate builder and mutation invariants
        let mut candidate = TaskCandidate::new(
            Some("ws-123".to_string()),
            "Debugging Transition Hysteresis",
            0.85,
        );

        assert_eq!(candidate.context_id.as_deref(), Some("ws-123"));
        assert_eq!(candidate.title, "Debugging Transition Hysteresis");
        assert_eq!(candidate.confidence, 0.85);
        assert_eq!(candidate.status, TaskCandidateStatus::Detected);
        assert_eq!(candidate.first_detected_at, candidate.last_active_at);

        // Touch activity promotes Detected -> Active
        candidate.touch_activity(Some("2026-09-13T10:00:00Z"), Some(0.92));
        assert_eq!(candidate.status, TaskCandidateStatus::Active);
        assert_eq!(candidate.last_active_at, "2026-09-13T10:00:00Z");
        assert_eq!(candidate.confidence, 0.92);

        // Mark inactive
        candidate.mark_inactive(Some("2026-09-13T10:30:00Z"));
        assert_eq!(candidate.status, TaskCandidateStatus::Inactive);

        // Resumed activity promotes Inactive -> Active
        candidate.touch_activity(Some("2026-09-13T11:00:00Z"), None);
        assert_eq!(candidate.status, TaskCandidateStatus::Active);

        // Mark uncertain
        candidate.mark_uncertain(Some("2026-09-13T11:15:00Z"));
        assert_eq!(candidate.status, TaskCandidateStatus::Uncertain);

        // Dismiss
        candidate.dismiss(Some("2026-09-13T11:30:00Z"));
        assert_eq!(candidate.status, TaskCandidateStatus::Dismissed);
    }

    #[test]
    fn test_task_candidate_crud_and_context_multiplicity() {
        use super::super::models::{TaskCandidate, TaskCandidateStatus};
        let conn = setup_db();

        let ws_college = create_workspace(&conn, "College").unwrap();
        let ws_work = create_workspace(&conn, "Work").unwrap();

        // 1. Create multiple task candidates for the same context (College) over time
        let task_1 = TaskCandidate::new(
            Some(ws_college.id.clone()),
            "CS106B Homework 2 - Dynamic Programming",
            0.90,
        )
        .with_status(TaskCandidateStatus::Active);

        let task_2 = TaskCandidate::new(
            Some(ws_college.id.clone()),
            "Bio 101 Lab 3 Report",
            0.75,
        )
        .with_status(TaskCandidateStatus::Inactive);

        let task_3 = TaskCandidate::new(
            Some(ws_work.id.clone()),
            "Q3 Financial Planning Model",
            0.88,
        )
        .with_status(TaskCandidateStatus::Detected);

        // Unassigned task candidate (no context yet)
        let task_4 = TaskCandidate::new(
            None,
            "Researching Japanese Sourdough Recipes",
            0.65,
        )
        .with_status(TaskCandidateStatus::Uncertain);

        let created_1 = create_task_candidate(&conn, &task_1).unwrap();
        let created_2 = create_task_candidate(&conn, &task_2).unwrap();
        let created_3 = create_task_candidate(&conn, &task_3).unwrap();
        let created_4 = create_task_candidate(&conn, &task_4).unwrap();

        assert_eq!(created_1.id, task_1.id);
        assert_eq!(created_1.title, "CS106B Homework 2 - Dynamic Programming");
        assert_eq!(created_1.status, TaskCandidateStatus::Active);

        assert_eq!(created_2.id, task_2.id);
        assert_eq!(created_2.status, TaskCandidateStatus::Inactive);

        assert_eq!(created_3.id, task_3.id);
        assert_eq!(created_3.status, TaskCandidateStatus::Detected);

        assert_eq!(created_4.id, task_4.id);
        assert_eq!(created_4.status, TaskCandidateStatus::Uncertain);

        // 2. Fetch by ID
        let fetched_1 = get_task_candidate(&conn, &task_1.id).unwrap();
        assert_eq!(fetched_1.id, task_1.id);
        assert_eq!(fetched_1.context_id.as_deref(), Some(ws_college.id.as_str()));

        // 3. List candidates for specific context (College has 2 candidates)
        let college_candidates = list_task_candidates_for_context(&conn, &ws_college.id).unwrap();
        assert_eq!(college_candidates.len(), 2, "A context must support multiple task candidates over time");
        let titles: Vec<&str> = college_candidates.iter().map(|c| c.title.as_str()).collect();
        assert!(titles.contains(&"CS106B Homework 2 - Dynamic Programming"));
        assert!(titles.contains(&"Bio 101 Lab 3 Report"));

        // 4. List with filters
        let all_candidates = list_task_candidates(&conn, None, None).unwrap();
        assert_eq!(all_candidates.len(), 4);

        let active_candidates = list_task_candidates(&conn, None, Some("active")).unwrap();
        assert_eq!(active_candidates.len(), 1);
        assert_eq!(active_candidates[0].id, task_1.id);

        let unassigned_candidates = list_task_candidates(&conn, None, Some("uncertain")).unwrap();
        assert_eq!(unassigned_candidates.len(), 1);
        assert_eq!(unassigned_candidates[0].id, task_4.id);

        // 5. Update status
        let updated_2 = update_task_candidate_status(&conn, &task_2.id, TaskCandidateStatus::Dismissed).unwrap();
        assert_eq!(updated_2.status, TaskCandidateStatus::Dismissed);

        // 6. Update activity
        let updated_3 = update_task_candidate_activity(&conn, &task_3.id, "2026-09-13T12:00:00Z", Some(0.95)).unwrap();
        assert_eq!(updated_3.status, TaskCandidateStatus::Active, "Updating activity on Detected task should promote to Active");
        assert_eq!(updated_3.last_active_at, "2026-09-13T12:00:00Z");
        assert_eq!(updated_3.confidence, 0.95);

        // 7. Update title
        let updated_title = update_task_candidate_title(&conn, &task_1.id, "CS106B HW2 - Graph Algorithms").unwrap();
        assert_eq!(updated_title.title, "CS106B HW2 - Graph Algorithms");

        // 8. Delete candidate
        delete_task_candidate(&conn, &task_4.id).unwrap();
        assert!(get_task_candidate(&conn, &task_4.id).is_err());
        assert_eq!(list_task_candidates(&conn, None, None).unwrap().len(), 3);
    }

    #[test]
    fn test_task_candidate_context_deletion_nullifies_foreign_key() {
        use super::super::models::TaskCandidate;
        let conn = setup_db();

        let ws = create_workspace(&conn, "Temporary Project").unwrap();
        let candidate = TaskCandidate::new(
            Some(ws.id.clone()),
            "Initial Setup and Scaffolding",
            0.80,
        );
        create_task_candidate(&conn, &candidate).unwrap();

        // Delete workspace
        delete_workspace(&conn, &ws.id).unwrap();

        // Task candidate still exists, but context_id is set to NULL (ON DELETE SET NULL)
        let refetched = get_task_candidate(&conn, &candidate.id).unwrap();
        assert_eq!(refetched.context_id, None, "Workspace deletion must set context_id to NULL rather than cascade delete inferred tasks");
    }
}

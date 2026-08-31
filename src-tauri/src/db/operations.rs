use super::models::{Workspace, Session};
use chrono::Utc;
use rusqlite::{params, Connection, Result};
use uuid::Uuid;

pub fn create_workspace(conn: &Connection, name: &str) -> Result<Workspace> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO workspaces (id, name, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
        params![id, name, now, now],
    )?;

    Ok(Workspace {
        id,
        name: name.to_string(),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn get_workspace(conn: &Connection, id: &str) -> Result<Workspace> {
    let mut stmt = conn.prepare("SELECT id, name, created_at, updated_at FROM workspaces WHERE id = ?1")?;
    let workspace = stmt.query_row(params![id], |row| {
        Ok(Workspace {
            id: row.get(0)?,
            name: row.get(1)?,
            created_at: row.get(2)?,
            updated_at: row.get(3)?,
        })
    })?;
    
    Ok(workspace)
}

pub fn list_workspaces(conn: &Connection) -> Result<Vec<Workspace>> {
    let mut stmt = conn.prepare("SELECT id, name, created_at, updated_at FROM workspaces ORDER BY created_at DESC")?;
    let workspace_iter = stmt.query_map([], |row| {
        Ok(Workspace {
            id: row.get(0)?,
            name: row.get(1)?,
            created_at: row.get(2)?,
            updated_at: row.get(3)?,
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

    Ok(super::models::Item {
        id,
        workspace_id: workspace_id.map(|s| s.to_string()),
        session_id: session_id.map(|s| s.to_string()),
        item_type: item_type.to_string(),
        content: content.to_string(),
        title: title.map(|s| s.to_string()),
        metadata: metadata.to_string(),
        created_at: now.clone(),
        updated_at: now,
    })
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
    conn.execute("DELETE FROM items WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn move_item(
    conn: &Connection,
    id: &str,
    workspace_id: Option<&str>,
) -> Result<super::models::Item> {
    let now = Utc::now().to_rfc3339();
    
    conn.execute(
        "UPDATE items SET workspace_id = ?1, updated_at = ?2 WHERE id = ?3",
        params![workspace_id, now, id],
    )?;
    
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

pub fn delete_item_relationship(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM item_relationships WHERE id = ?1", params![id])?;
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
}

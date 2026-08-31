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
}

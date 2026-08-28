use super::models::Workspace;
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
    item_type: &str,
    content: &str,
    title: Option<&str>,
    metadata: &str,
) -> Result<super::models::Item> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO items (id, workspace_id, item_type, content, title, metadata, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, workspace_id, item_type, content, title, metadata, now, now],
    )?;

    Ok(super::models::Item {
        id,
        workspace_id: workspace_id.map(|s| s.to_string()),
        item_type: item_type.to_string(),
        content: content.to_string(),
        title: title.map(|s| s.to_string()),
        metadata: metadata.to_string(),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn get_item(conn: &Connection, id: &str) -> Result<super::models::Item> {
    let mut stmt = conn.prepare("SELECT id, workspace_id, item_type, content, title, metadata, created_at, updated_at FROM items WHERE id = ?1")?;
    let item = stmt.query_row(params![id], |row| {
        Ok(super::models::Item {
            id: row.get(0)?,
            workspace_id: row.get(1)?,
            item_type: row.get(2)?,
            content: row.get(3)?,
            title: row.get(4)?,
            metadata: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    })?;
    
    Ok(item)
}

pub fn list_items(conn: &Connection, workspace_id: Option<&str>) -> Result<Vec<super::models::Item>> {
    let mut items = Vec::new();

    if let Some(w_id) = workspace_id {
        let mut stmt = conn.prepare("SELECT id, workspace_id, item_type, content, title, metadata, created_at, updated_at FROM items WHERE workspace_id = ?1 ORDER BY created_at DESC")?;
        let item_iter = stmt.query_map(params![w_id], |row| {
            Ok(super::models::Item {
                id: row.get(0)?,
                workspace_id: row.get(1)?,
                item_type: row.get(2)?,
                content: row.get(3)?,
                title: row.get(4)?,
                metadata: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })?;
        for item in item_iter {
            items.push(item?);
        }
    } else {
        let mut stmt = conn.prepare("SELECT id, workspace_id, item_type, content, title, metadata, created_at, updated_at FROM items WHERE workspace_id IS NULL ORDER BY created_at DESC")?;
        let item_iter = stmt.query_map([], |row| {
            Ok(super::models::Item {
                id: row.get(0)?,
                workspace_id: row.get(1)?,
                item_type: row.get(2)?,
                content: row.get(3)?,
                title: row.get(4)?,
                metadata: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
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

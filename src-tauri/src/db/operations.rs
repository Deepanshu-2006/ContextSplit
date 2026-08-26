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

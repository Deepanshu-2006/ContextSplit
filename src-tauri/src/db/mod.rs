pub mod models;
pub mod operations;

use rusqlite::{Connection, Result};
use std::path::PathBuf;

pub fn init(app_data_dir: PathBuf) -> std::result::Result<Connection, Box<dyn std::error::Error>> {
    // Ensure the app data directory exists
    std::fs::create_dir_all(&app_data_dir)?;

    let db_path = app_data_dir.join("contextsplit.db");
    
    // Open a connection (creates the file if it doesn't exist)
    let conn = Connection::open(db_path)?;

    // Run migrations (create tables)
    run_migrations(&conn)?;

    Ok(conn)
}

fn run_migrations(conn: &Connection) -> Result<()> {
    let mut user_version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;

    if user_version < 1 {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS workspaces (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
            [],
        )?;
        user_version = 1;
    }

    if user_version < 2 {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS items (
                id TEXT PRIMARY KEY,
                workspace_id TEXT,
                item_type TEXT NOT NULL,
                content TEXT NOT NULL,
                title TEXT,
                metadata TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY(workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
            )",
            [],
        )?;
        
        user_version = 2;
    }

    if user_version < 3 {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS context_signals (
                id TEXT PRIMARY KEY,
                workspace_id TEXT,
                item_id TEXT,
                signal_type TEXT NOT NULL,
                session_id TEXT,
                payload TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY(workspace_id) REFERENCES workspaces(id) ON DELETE SET NULL,
                FOREIGN KEY(item_id) REFERENCES items(id) ON DELETE SET NULL
            )",
            [],
        )?;
        
        user_version = 3;
    }

    conn.pragma_update(None, "user_version", user_version)?;
    
    // Enable foreign key support
    conn.execute("PRAGMA foreign_keys = ON", [])?;

    Ok(())
}

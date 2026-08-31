pub mod models;
pub mod operations;
pub mod settings;

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

pub fn run_migrations(conn: &Connection) -> Result<()> {
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

    if user_version < 4 {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                started_at TEXT NOT NULL,
                last_activity_at TEXT NOT NULL,
                ended_at TEXT,
                status TEXT NOT NULL
            )",
            [],
        )?;
        
        // Add session_id to items
        conn.execute(
            "ALTER TABLE items ADD COLUMN session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL",
            [],
        )?;
        
        user_version = 4;
    }

    if user_version < 5 {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS item_relationships (
                id TEXT PRIMARY KEY,
                item_a_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
                item_b_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
                relationship_type TEXT NOT NULL,
                strength REAL NOT NULL,
                metadata TEXT,
                created_at TEXT NOT NULL,
                UNIQUE(item_a_id, item_b_id, relationship_type)
            )",
            [],
        )?;
        
        user_version = 5;
    }

    if user_version < 6 {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS settings (
                id TEXT PRIMARY KEY,
                enable_active_app INTEGER NOT NULL DEFAULT 0,
                enable_window_title INTEGER NOT NULL DEFAULT 0,
                enable_browser_domain INTEGER NOT NULL DEFAULT 0,
                enable_session_tracking INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )?;
        
        // Insert default row
        conn.execute(
            "INSERT OR IGNORE INTO settings (id, enable_active_app, enable_window_title, enable_browser_domain, enable_session_tracking) 
             VALUES ('default', 0, 0, 0, 0)",
            [],
        )?;
        
        user_version = 6;
    }

    if user_version < 7 {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS item_embeddings (
                item_id TEXT PRIMARY KEY REFERENCES items(id) ON DELETE CASCADE,
                embedding BLOB NOT NULL,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                created_at TEXT NOT NULL
            )",
            [],
        )?;
        
        user_version = 7;
    }

    if user_version < 8 {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS workspace_embeddings (
                workspace_id TEXT PRIMARY KEY REFERENCES workspaces(id) ON DELETE CASCADE,
                embedding BLOB NOT NULL,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
            [],
        )?;
        
        user_version = 8;
    }

    if user_version < 9 {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS item_semantic_status (
                item_id TEXT PRIMARY KEY REFERENCES items(id) ON DELETE CASCADE,
                status TEXT NOT NULL DEFAULT 'pending',
                retries INTEGER NOT NULL DEFAULT 0,
                model_version TEXT,
                updated_at TEXT NOT NULL
            )",
            [],
        )?;
        
        user_version = 9;
    }

    conn.pragma_update(None, "user_version", user_version)?;
    
    // Enable foreign key support
    conn.execute("PRAGMA foreign_keys = ON", [])?;

    Ok(())
}

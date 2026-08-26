pub mod models;
pub mod operations;

use rusqlite::{Connection, Result};
use std::path::PathBuf;

pub fn init(app_data_dir: PathBuf) -> Result<Connection> {
    // Ensure the app data directory exists
    std::fs::create_dir_all(&app_data_dir)
        .expect("Failed to create application data directory");

    let db_path = app_data_dir.join("contextsplit.db");
    
    // Open a connection (creates the file if it doesn't exist)
    let conn = Connection::open(db_path)?;

    // Run migrations (create tables)
    create_schema(&conn)?;

    Ok(conn)
}

fn create_schema(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS workspaces (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
        [],
    )?;
    
    Ok(())
}

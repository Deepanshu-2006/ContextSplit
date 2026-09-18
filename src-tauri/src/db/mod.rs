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

    if user_version < 10 {
        conn.execute("ALTER TABLE settings ADD COLUMN enable_semantic_processing INTEGER DEFAULT 1", [])?;
        conn.execute("ALTER TABLE settings ADD COLUMN semantic_provider TEXT DEFAULT 'local'", [])?;
        conn.execute("ALTER TABLE settings ADD COLUMN enable_semantic_suggestions INTEGER DEFAULT 1", [])?;
        user_version = 10;
    }

    if user_version < 11 {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS context_candidates (
                id TEXT PRIMARY KEY,
                status TEXT NOT NULL DEFAULT 'detected',
                confidence REAL NOT NULL DEFAULT 0.0,
                label TEXT,
                metadata TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS context_candidate_items (
                candidate_id TEXT NOT NULL REFERENCES context_candidates(id) ON DELETE CASCADE,
                item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
                relevance_score REAL NOT NULL DEFAULT 1.0,
                added_at TEXT NOT NULL,
                PRIMARY KEY (candidate_id, item_id)
            )",
            [],
        )?;

        // Index for fast lookup by status
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_context_candidates_status ON context_candidates(status)",
            [],
        )?;

        // Index for fast lookup of candidates containing a given item
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_candidate_items_item_id ON context_candidate_items(item_id)",
            [],
        )?;

        user_version = 11;
    }

    if user_version < 12 {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS context_memberships (
                id TEXT PRIMARY KEY,
                item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
                workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
                membership_type TEXT NOT NULL,
                confidence REAL NOT NULL DEFAULT 1.0,
                source TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE(item_id, workspace_id)
            )",
            [],
        )?;

        // Indices for fast lookup
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memberships_item_id ON context_memberships(item_id)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memberships_workspace_type ON context_memberships(workspace_id, membership_type)",
            [],
        )?;

        // Backfill existing confirmed item workspace assignments into context_memberships
        conn.execute(
            "INSERT OR IGNORE INTO context_memberships (id, item_id, workspace_id, membership_type, confidence, source, created_at, updated_at)
             SELECT 
                lower(hex(randomblob(4)) || '-' || hex(randomblob(2)) || '-4' || substr(hex(randomblob(2)),2) || '-' || substr('89ab', 1 + (abs(random()) % 4), 1) || substr(hex(randomblob(2)),2) || '-' || hex(randomblob(6))),
                id,
                workspace_id,
                'confirmed',
                1.0,
                'user',
                created_at,
                updated_at
             FROM items
             WHERE workspace_id IS NOT NULL AND workspace_id != 'inbox' AND workspace_id != ''",
            [],
        )?;

        user_version = 12;
    }

    if user_version < 13 {
        conn.execute("ALTER TABLE settings ADD COLUMN temporal_decay_config TEXT DEFAULT '{\"strategy\":\"exponential\",\"half_life_days\":14.0,\"min_weight_floor\":0.20,\"grace_period_hours\":24.0}'", [])?;
        user_version = 13;
    }

    if user_version < 14 {
        conn.execute("ALTER TABLE workspaces ADD COLUMN lifecycle_state TEXT NOT NULL DEFAULT 'active'", [])?;
        conn.execute("ALTER TABLE workspaces ADD COLUMN last_activity_at TEXT", [])?;
        conn.execute("ALTER TABLE workspaces ADD COLUMN archived_at TEXT", [])?;

        // Initialize last_activity_at with updated_at for existing workspaces
        conn.execute("UPDATE workspaces SET last_activity_at = updated_at WHERE last_activity_at IS NULL", [])?;

        // Create index on lifecycle_state
        conn.execute("CREATE INDEX IF NOT EXISTS idx_workspaces_lifecycle ON workspaces(lifecycle_state)", [])?;

        user_version = 14;
    }

    if user_version < 15 {
        conn.execute("ALTER TABLE settings ADD COLUMN enable_ai_reasoning INTEGER DEFAULT 1", [])?;
        user_version = 15;
    }

    if user_version < 16 {
        conn.execute("ALTER TABLE settings ADD COLUMN ai_gating_enabled INTEGER DEFAULT 1", [])?;
        conn.execute("ALTER TABLE settings ADD COLUMN ai_gating_confidence_threshold REAL DEFAULT 0.85", [])?;
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS ai_reasoning_cache (
                package_hash TEXT PRIMARY KEY,
                item_id TEXT NOT NULL,
                user_id TEXT NOT NULL DEFAULT 'local_user',
                result_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                expires_at TEXT NOT NULL
            )",
            [],
        )?;
        
        user_version = 16;
    }

    if user_version < 17 {
        // Handle migration for tables created in v16 without user_id / expires_at
        let _ = conn.execute("ALTER TABLE ai_reasoning_cache ADD COLUMN user_id TEXT NOT NULL DEFAULT 'local_user'", []);
        let _ = conn.execute("ALTER TABLE ai_reasoning_cache ADD COLUMN expires_at TEXT NOT NULL DEFAULT ''", []);
        
        // Update any empty expires_at with 7 days from now
        let _ = conn.execute("UPDATE ai_reasoning_cache SET expires_at = datetime('now', '+7 days') WHERE expires_at = ''", []);

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_ai_cache_item ON ai_reasoning_cache(item_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_ai_cache_user ON ai_reasoning_cache(user_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_ai_cache_expires ON ai_reasoning_cache(expires_at)",
            [],
        )?;

        user_version = 17;
    }

    if user_version < 18 {
        let _ = conn.execute("ALTER TABLE settings ADD COLUMN capture_paused INTEGER DEFAULT 0", []);
        user_version = 18;
    }

    if user_version < 19 {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS inbox_item_states (
                item_id TEXT PRIMARY KEY REFERENCES items(id) ON DELETE CASCADE,
                state TEXT NOT NULL,
                previous_state TEXT,
                reason TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_inbox_item_states_state ON inbox_item_states(state)",
            [],
        )?;

        // Backfill existing inbox items (items with workspace_id IS NULL or empty or 'inbox')
        conn.execute(
            "INSERT OR IGNORE INTO inbox_item_states (item_id, state, previous_state, reason, created_at, updated_at)
             SELECT 
                id,
                'new',
                NULL,
                'Migration backfill for existing inbox item',
                created_at,
                updated_at
             FROM items
             WHERE workspace_id IS NULL OR workspace_id = '' OR workspace_id = 'inbox'",
            [],
        )?;

        user_version = 19;
    }

    if user_version < 20 {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS context_transitions (
                id TEXT PRIMARY KEY,
                from_context_id TEXT,
                to_context_id TEXT,
                transition_type TEXT NOT NULL,
                confidence REAL NOT NULL DEFAULT 1.0,
                detected_at TEXT NOT NULL,
                session_id TEXT,
                trigger_item_id TEXT,
                trigger_signal_id TEXT,
                metadata TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE SET NULL,
                FOREIGN KEY(trigger_item_id) REFERENCES items(id) ON DELETE SET NULL,
                FOREIGN KEY(trigger_signal_id) REFERENCES context_signals(id) ON DELETE SET NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_context_transitions_detected ON context_transitions(detected_at)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_context_transitions_type ON context_transitions(transition_type)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_context_transitions_to_context ON context_transitions(to_context_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_context_transitions_from_context ON context_transitions(from_context_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_context_transitions_session ON context_transitions(session_id)",
            [],
        )?;

        user_version = 20;
    }

    if user_version < 21 {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS task_candidates (
                id TEXT PRIMARY KEY,
                context_id TEXT,
                title TEXT NOT NULL,
                confidence REAL NOT NULL DEFAULT 0.0,
                status TEXT NOT NULL DEFAULT 'detected',
                first_detected_at TEXT NOT NULL,
                last_active_at TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY(context_id) REFERENCES workspaces(id) ON DELETE SET NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_task_candidates_context_id ON task_candidates(context_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_task_candidates_status ON task_candidates(status)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_task_candidates_last_active ON task_candidates(last_active_at)",
            [],
        )?;

        user_version = 21;
    }

    if user_version < 22 {
        let _ = conn.execute("ALTER TABLE settings ADD COLUMN enable_context_snapshots INTEGER DEFAULT 1", []);
        user_version = 22;
    }

    if user_version < 23 {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS context_snapshots (
                id TEXT PRIMARY KEY,
                context_id TEXT NOT NULL,
                context_name TEXT NOT NULL,
                task_id TEXT,
                task_title TEXT,
                created_at TEXT NOT NULL,
                last_activity_at TEXT,
                state TEXT NOT NULL,
                snapshot_confidence REAL NOT NULL DEFAULT 0.0,
                quality_rating TEXT NOT NULL,
                snapshot_json TEXT NOT NULL,
                FOREIGN KEY(context_id) REFERENCES workspaces(id) ON DELETE CASCADE
            )",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_context_snapshots_context_created ON context_snapshots(context_id, created_at DESC)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_context_snapshots_task ON context_snapshots(context_id, task_title)",
            [],
        )?;

        user_version = 23;
    }

    if user_version < 24 {
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_items_workspace_created ON items(workspace_id, created_at DESC)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_items_updated_at ON items(updated_at DESC)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_context_memberships_ws_type_conf ON context_memberships(workspace_id, membership_type, confidence DESC)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_context_memberships_item_ws ON context_memberships(item_id, workspace_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_item_relationships_item_a ON item_relationships(item_a_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_item_relationships_item_b ON item_relationships(item_b_id)",
            [],
        )?;

        user_version = 24;
    }

    conn.pragma_update(None, "user_version", user_version)?;
    
    // Enable foreign key support
    conn.execute("PRAGMA foreign_keys = ON", [])?;

    Ok(())
}


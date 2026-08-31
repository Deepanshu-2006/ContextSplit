use rusqlite::{Connection, Result, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PrivacySettings {
    pub enable_active_app: bool,
    pub enable_window_title: bool,
    pub enable_browser_domain: bool,
    pub enable_session_tracking: bool,
    pub enable_semantic_processing: bool,
    pub semantic_provider: String,
    pub enable_semantic_suggestions: bool,
}

pub fn get_settings(conn: &Connection) -> Result<PrivacySettings> {
    let mut stmt = conn.prepare("SELECT enable_active_app, enable_window_title, enable_browser_domain, enable_session_tracking, enable_semantic_processing, semantic_provider, enable_semantic_suggestions FROM settings WHERE id = 'default'")?;
    
    let settings = stmt.query_row([], |row| {
        Ok(PrivacySettings {
            enable_active_app: row.get::<_, i32>(0)? != 0,
            enable_window_title: row.get::<_, i32>(1)? != 0,
            enable_browser_domain: row.get::<_, i32>(2)? != 0,
            enable_session_tracking: row.get::<_, i32>(3)? != 0,
            enable_semantic_processing: row.get::<_, i32>(4).unwrap_or(1) != 0,
            semantic_provider: row.get::<_, String>(5).unwrap_or_else(|_| "local".to_string()),
            enable_semantic_suggestions: row.get::<_, i32>(6).unwrap_or(1) != 0,
        })
    })?;
    
    Ok(settings)
}

pub fn update_settings(conn: &Connection, settings: PrivacySettings) -> Result<()> {
    conn.execute(
        "UPDATE settings SET 
            enable_active_app = ?1,
            enable_window_title = ?2,
            enable_browser_domain = ?3,
            enable_session_tracking = ?4,
            enable_semantic_processing = ?5,
            semantic_provider = ?6,
            enable_semantic_suggestions = ?7
         WHERE id = 'default'",
        params![
            if settings.enable_active_app { 1 } else { 0 },
            if settings.enable_window_title { 1 } else { 0 },
            if settings.enable_browser_domain { 1 } else { 0 },
            if settings.enable_session_tracking { 1 } else { 0 },
            if settings.enable_semantic_processing { 1 } else { 0 },
            settings.semantic_provider,
            if settings.enable_semantic_suggestions { 1 } else { 0 },
        ],
    )?;
    
    Ok(())
}

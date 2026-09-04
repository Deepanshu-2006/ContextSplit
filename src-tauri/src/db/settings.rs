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
    pub enable_ai_reasoning: bool,
    pub ai_gating_enabled: bool,
    pub ai_gating_confidence_threshold: f64,
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            enable_active_app: false,
            enable_window_title: false,
            enable_browser_domain: false,
            enable_session_tracking: false,
            enable_semantic_processing: true,
            semantic_provider: "local".to_string(),
            enable_semantic_suggestions: true,
            enable_ai_reasoning: true,
            ai_gating_enabled: true,
            ai_gating_confidence_threshold: 0.85,
        }
    }
}

pub fn get_settings(conn: &Connection) -> Result<PrivacySettings> {
    let mut stmt = conn.prepare("SELECT enable_active_app, enable_window_title, enable_browser_domain, enable_session_tracking, enable_semantic_processing, semantic_provider, enable_semantic_suggestions, enable_ai_reasoning, ai_gating_enabled, ai_gating_confidence_threshold FROM settings WHERE id = 'default'")?;
    
    let settings = stmt.query_row([], |row| {
        Ok(PrivacySettings {
            enable_active_app: row.get::<_, i32>(0)? != 0,
            enable_window_title: row.get::<_, i32>(1)? != 0,
            enable_browser_domain: row.get::<_, i32>(2)? != 0,
            enable_session_tracking: row.get::<_, i32>(3)? != 0,
            enable_semantic_processing: row.get::<_, i32>(4).unwrap_or(1) != 0,
            semantic_provider: row.get::<_, String>(5).unwrap_or_else(|_| "local".to_string()),
            enable_semantic_suggestions: row.get::<_, i32>(6).unwrap_or(1) != 0,
            enable_ai_reasoning: row.get::<_, i32>(7).unwrap_or(1) != 0,
            ai_gating_enabled: row.get::<_, i32>(8).unwrap_or(1) != 0,
            ai_gating_confidence_threshold: row.get::<_, f64>(9).unwrap_or(0.85),
        })
    })?;
    
    Ok(settings)
}

pub fn update_settings(conn: &Connection, settings: PrivacySettings) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO settings 
         (id, enable_active_app, enable_window_title, enable_browser_domain, enable_session_tracking, enable_semantic_processing, semantic_provider, enable_semantic_suggestions, enable_ai_reasoning, ai_gating_enabled, ai_gating_confidence_threshold)
         VALUES ('default', ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            settings.enable_active_app,
            settings.enable_window_title,
            settings.enable_browser_domain,
            settings.enable_session_tracking,
            settings.enable_semantic_processing,
            settings.semantic_provider,
            settings.enable_semantic_suggestions,
            settings.enable_ai_reasoning,
            settings.ai_gating_enabled,
            settings.ai_gating_confidence_threshold
        ],
    )?;
    
    Ok(())
}

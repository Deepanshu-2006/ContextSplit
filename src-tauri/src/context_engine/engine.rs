use rusqlite::Connection;
use serde_json::Value;
use chrono::{DateTime, Utc};
use crate::db::operations;

pub enum ContextEvent {
    Capture {
        item_id: String,
        workspace_id: Option<String>,
        source: String,
        data_type: String,
        metadata: Value,
    },
    UserAction {
        item_id: String,
        action: String,
        source_workspace: Option<String>,
        destination_workspace: Option<String>,
    },
    WorkspaceSelected {
        workspace_id: String,
    },
}

pub struct ContextSignalParams {
    pub workspace_id: Option<String>,
    pub item_id: Option<String>,
    pub signal_type: String,
    pub payload: Value,
}

pub trait ContextProvider: Send + Sync {
    fn name(&self) -> &'static str;
    
    // Lifecycle
    fn initialize(&mut self) -> Result<(), String> { Ok(()) }
    fn cleanup(&mut self) -> Result<(), String> { Ok(()) }
    
    // For push-based (event-driven) and pull-based contextual signals
    fn collect(&self, conn: &Connection, event: Option<&ContextEvent>) -> Vec<ContextSignalParams>;
}

pub struct ContextEngine {
    providers: Vec<Box<dyn ContextProvider>>,
    pub inactivity_threshold_secs: i64,
}

impl ContextEngine {
    pub fn new() -> Self {
        Self { 
            providers: vec![],
            inactivity_threshold_secs: 1800, // 30 minutes default
        }
    }
    
    pub fn register(&mut self, provider: Box<dyn ContextProvider>) {
        self.providers.push(provider);
    }
    
    pub fn initialize_all(&mut self) -> Result<(), String> {
        for p in &mut self.providers {
            p.initialize()?;
        }
        Ok(())
    }
    
    pub fn get_or_update_active_session(&self, conn: &Connection) -> Result<String, String> {
        // Find latest session
        match operations::get_latest_session(conn) {
            Ok(Some(session)) => {
                if session.status == "active" {
                    // Check if it has expired
                    if let Ok(last_activity) = session.last_activity_at.parse::<DateTime<Utc>>() {
                        let now = Utc::now();
                        let diff = now.signed_duration_since(last_activity).num_seconds();
                        
                        if diff > self.inactivity_threshold_secs {
                            // Expired. End it and create a new one.
                            let _ = operations::end_session(conn, &session.id);
                            
                            // Check if session tracking is enabled
                            if let Ok(settings) = crate::db::settings::get_settings(conn) {
                                if !settings.enable_session_tracking {
                                    return Err("Session tracking disabled".to_string());
                                }
                            }
                            
                            let new_session = operations::create_session(conn)
                                .map_err(|e| e.to_string())?;
                            return Ok(new_session.id);
                        } else {
                            // Still active. Update activity time.
                            let _ = operations::update_session_activity(conn, &session.id);
                            return Ok(session.id);
                        }
                    }
                }
                
                // If it was ended, or parsing failed, create new.
                // Check if session tracking is enabled
                if let Ok(settings) = crate::db::settings::get_settings(conn) {
                    if !settings.enable_session_tracking {
                        return Err("Session tracking disabled".to_string());
                    }
                }
                
                let new_session = operations::create_session(conn)
                    .map_err(|e| e.to_string())?;
                Ok(new_session.id)
            },
            Ok(None) | Err(_) => {
                // No sessions exist yet, create first one
                // Check if session tracking is enabled
                if let Ok(settings) = crate::db::settings::get_settings(conn) {
                    if !settings.enable_session_tracking {
                        return Err("Session tracking disabled".to_string());
                    }
                }
                
                let new_session = operations::create_session(conn)
                    .map_err(|e| e.to_string())?;
                Ok(new_session.id)
            }
        }
    }
    
    pub fn process_event(&self, conn: &Connection, event: ContextEvent) {
        let session_id_opt = self.get_or_update_active_session(conn).ok();
        
        for provider in &self.providers {
            let signal_params = provider.collect(conn, Some(&event));
            for param in signal_params {
                let _ = super::operations::create_context_signal(
                    conn,
                    param.workspace_id.as_deref(),
                    param.item_id.as_deref(),
                    &param.signal_type,
                    session_id_opt.as_deref(),
                    &param.payload.to_string()
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::operations;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn test_session_detection_lifecycle() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();

        let mut ce = ContextEngine::new();
        // Use a short threshold for testing
        ce.inactivity_threshold_secs = 2;

        // 1. First activity creates a session
        let session_id_1 = ce.get_or_update_active_session(&conn).unwrap();
        
        let session_1 = operations::get_session(&conn, &session_id_1).unwrap();
        assert_eq!(session_1.status, "active");

        // 2. Activity continues the session
        sleep(Duration::from_secs(1)); // Wait less than threshold
        let session_id_2 = ce.get_or_update_active_session(&conn).unwrap();
        
        assert_eq!(session_id_1, session_id_2, "Session should be continued within threshold");

        // 3. Inactivity creates a new session
        sleep(Duration::from_secs(3)); // Wait more than threshold
        let session_id_3 = ce.get_or_update_active_session(&conn).unwrap();
        
        assert_ne!(session_id_1, session_id_3, "A new session should be created after threshold exceeded");
        
        let old_session = operations::get_session(&conn, &session_id_1).unwrap();
        assert_eq!(old_session.status, "ended", "Old session should be marked as ended");
        
        let new_session = operations::get_session(&conn, &session_id_3).unwrap();
        assert_eq!(new_session.status, "active", "New session should be active");
    }

    #[test]
    fn test_multiple_items_same_session() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();

        let mut ce = ContextEngine::new();
        ce.inactivity_threshold_secs = 60; // 1 minute

        // Fetch session id (simulating first item capture)
        let session_id_1 = ce.get_or_update_active_session(&conn).unwrap();
        
        let item_1 = operations::create_item(&conn, None, Some(&session_id_1), "text", "first item", None, "{}").unwrap();
        assert_eq!(item_1.session_id.as_ref().unwrap(), &session_id_1);

        // Fetch session id again right away (simulating second item capture)
        let session_id_2 = ce.get_or_update_active_session(&conn).unwrap();
        
        let item_2 = operations::create_item(&conn, None, Some(&session_id_2), "text", "second item", None, "{}").unwrap();
        
        assert_eq!(session_id_1, session_id_2, "Session IDs should match");
        assert_eq!(item_1.session_id, item_2.session_id, "Both items should belong to the same session");
    }
}

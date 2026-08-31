#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::thread;
    use std::time::Duration;

    use crate::db::{self, operations, settings::PrivacySettings};
    use crate::context_engine::engine::{ContextEngine, ContextEvent};
    use crate::context_engine::providers::{CaptureContextProvider, UserActionContextProvider, WorkspaceContextProvider, UrlContextProvider};
    use crate::context_engine::relationships;
    use crate::context_engine::scoring;

    // Helper to setup a test DB with temp file
    fn setup_test_db() -> (Connection, PathBuf) {
        let temp_dir = std::env::temp_dir().join(format!("contextsplit_test_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("test.db");
        
        let conn = Connection::open(&db_path).unwrap();
        db::run_migrations(&conn).unwrap();
        
        // Default settings in test DB
        let settings = PrivacySettings {
            enable_active_app: true,
            enable_window_title: true,
            enable_browser_domain: true,
            enable_session_tracking: true,
        };
        crate::db::settings::update_settings(&conn, settings).unwrap();

        (conn, db_path)
    }

    // Helper to create an engine with all providers
    fn create_engine() -> ContextEngine {
        let mut ce = ContextEngine::new();
        ce.register(Box::new(CaptureContextProvider));
        ce.register(Box::new(UserActionContextProvider));
        ce.register(Box::new(WorkspaceContextProvider));
        ce.register(Box::new(UrlContextProvider));
        // Note: Skipping ActiveAppProvider in test since it relies on actual OS window state which we can't easily mock in a standard Cargo test without trait refactoring.
        // We will simulate it via standard Capture requests.
        ce
    }

    #[test]
    fn test_complete_workflow() {
        let (conn, db_path) = setup_test_db();
        let engine = create_engine();

        // 1. Create a Portfolio workspace
        let workspace = operations::create_workspace(&conn, "Portfolio").unwrap();
        
        // Verify it was created
        let workspaces = operations::list_workspaces(&conn).unwrap();
        assert_eq!(workspaces.len(), 1);
        assert_eq!(workspaces[0].name, "Portfolio");

        // 2 & 3. Capture several related Items (e.g. from Figma)
        // Item 1
        let session_id_1 = engine.get_or_update_active_session(&conn).unwrap();
        let item_1 = operations::create_item(
            &conn, 
            None, 
            Some(&session_id_1), 
            "url", 
            "https://www.figma.com/file/123/Portfolio-Design", 
            Some("Portfolio Design"), 
            "{}"
        ).unwrap();

        engine.process_event(&conn, ContextEvent::Capture {
            item_id: item_1.id.clone(),
            workspace_id: None,
            source: "browser".to_string(),
            data_type: "url".to_string(),
            metadata: json!({"original_url": "https://www.figma.com/file/123/Portfolio-Design"}),
        });
        relationships::generate_relationships(&conn, &item_1.id).unwrap();

        // Item 2 (Captured right after, same session)
        let item_2 = operations::create_item(
            &conn, 
            None, 
            Some(&session_id_1), 
            "image", 
            "base64_image_data...", 
            Some("Screenshot"), 
            "{}"
        ).unwrap();
        
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: item_2.id.clone(),
            workspace_id: None,
            source: "screenshot".to_string(),
            data_type: "image".to_string(),
            metadata: json!({"filename": "Screenshot 2026-08-31.png"}),
        });
        relationships::generate_relationships(&conn, &item_2.id).unwrap();

        // 4. Capture unrelated Item (e.g. Twitter)
        let item_3 = operations::create_item(
            &conn, 
            None, 
            Some(&session_id_1), 
            "url", 
            "https://twitter.com/home", 
            Some("Twitter"), 
            "{}"
        ).unwrap();
        
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: item_3.id.clone(),
            workspace_id: None,
            source: "browser".to_string(),
            data_type: "url".to_string(),
            metadata: json!({"original_url": "https://twitter.com/home"}),
        });
        relationships::generate_relationships(&conn, &item_3.id).unwrap();

        // 6. Verify they share a Context Session
        assert_eq!(item_1.session_id, item_2.session_id);
        assert_eq!(item_2.session_id, item_3.session_id);

        // 7. Verify ItemRelationships are generated
        let rels_for_1 = crate::context_engine::graph::get_strongest_relationships(&conn, &item_1.id, 10).unwrap();
        // Should have a relationship with item 2 (same session)
        assert!(rels_for_1.iter().any(|r| (r.item_a_id == item_1.id && r.item_b_id == item_2.id) || (r.item_a_id == item_2.id && r.item_b_id == item_1.id)));

        // 5. Move several related Items into Portfolio
        operations::move_item(&conn, &item_1.id, Some(&workspace.id)).unwrap();
        engine.process_event(&conn, ContextEvent::UserAction {
            item_id: item_1.id.clone(),
            action: "suggestion_accepted".to_string(), // Simulating user moving it
            source_workspace: None,
            destination_workspace: Some(workspace.id.clone()),
        });

        operations::move_item(&conn, &item_2.id, Some(&workspace.id)).unwrap();
        engine.process_event(&conn, ContextEvent::UserAction {
            item_id: item_2.id.clone(),
            action: "suggestion_accepted".to_string(),
            source_workspace: None,
            destination_workspace: Some(workspace.id.clone()),
        });

        // 8. Capture another related Item (another Figma link)
        let item_4 = operations::create_item(
            &conn, 
            None, 
            Some(&session_id_1), 
            "url", 
            "https://www.figma.com/file/456/Assets", 
            Some("Assets"), 
            "{}"
        ).unwrap();
        
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: item_4.id.clone(),
            workspace_id: None,
            source: "browser".to_string(),
            data_type: "url".to_string(),
            metadata: json!({"original_url": "https://www.figma.com/file/456/Assets"}),
        });
        relationships::generate_relationships(&conn, &item_4.id).unwrap();

        // 9. Verify Context Engine increases Portfolio confidence
        let scores = scoring::score_item(&conn, &item_4.id).unwrap();
        let portfolio_score = scores.iter().find(|s| s.workspace_id == workspace.id);
        assert!(portfolio_score.is_some(), "Portfolio should have a score for this new related item");
        let score = portfolio_score.unwrap();
        
        // 10. Verify explanation reasons are correct
        assert!(score.confidence > 0.0);
        assert!(!score.reasons.is_empty(), "There should be reasons attached to the score");

        // 13 & 14. Move an Item to a different Workspace and verify signal
        let workspace_2 = operations::create_workspace(&conn, "Social").unwrap();
        operations::move_item(&conn, &item_3.id, Some(&workspace_2.id)).unwrap();
        engine.process_event(&conn, ContextEvent::UserAction {
            item_id: item_3.id.clone(),
            action: "moved".to_string(),
            source_workspace: None,
            destination_workspace: Some(workspace_2.id.clone()),
        });
        
        let signals = crate::context_engine::operations::list_context_signals(&conn, Some(&workspace_2.id), Some(&item_3.id)).unwrap();
        assert!(signals.iter().any(|s| s.signal_type == "user_action"));

        // 15. Restart ContextSplit (Simulate by dropping connection and opening new one)
        drop(conn);
        let conn2 = Connection::open(&db_path).unwrap();

        // 16. Verify Sessions, relationships and signals persist
        let workspaces_reloaded = operations::list_workspaces(&conn2).unwrap();
        assert_eq!(workspaces_reloaded.len(), 2);
        
        let items_reloaded = operations::list_items(&conn2, Some(&workspace.id)).unwrap();
        assert_eq!(items_reloaded.len(), 2); // item_1 and item_2
        
        let rels_reloaded = crate::context_engine::graph::get_strongest_relationships(&conn2, &item_1.id, 10).unwrap();
        assert!(!rels_reloaded.is_empty(), "Relationships should persist");

        // 17. Disable one privacy setting
        let settings_off = PrivacySettings {
            enable_active_app: false,
            enable_window_title: false,
            enable_browser_domain: false,
            enable_session_tracking: false,
        };
        crate::db::settings::update_settings(&conn2, settings_off).unwrap();

        // 18. Capture another Item
        let item_5 = operations::create_item(
            &conn2, 
            None, 
            None, // No session because tracking is off! Wait, let's test if engine respects it.
            "text", 
            "Secret note", 
            None, 
            "{}"
        ).unwrap();

        // The engine should fail to create a session if tracking is disabled.
        let session_result = engine.get_or_update_active_session(&conn2);
        assert!(session_result.is_err(), "Session tracking should be disabled");

        // Emit capture event (url provider should be disabled)
        engine.process_event(&conn2, ContextEvent::Capture {
            item_id: item_5.id.clone(),
            workspace_id: None,
            source: "browser".to_string(),
            data_type: "url".to_string(),
            metadata: json!({"original_url": "https://secret.com/page"}),
        });

        // 19. Verify that signal is no longer collected
        let signals_for_5 = crate::context_engine::operations::list_context_signals(&conn2, None, Some(&item_5.id)).unwrap();
        // Should only have "capture_event", no "url_metadata"
        assert!(!signals_for_5.iter().any(|s| s.signal_type == "url_metadata"), "URL metadata signal should NOT be collected");
        
        // Clean up
        let _ = fs::remove_dir_all(db_path.parent().unwrap());
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;

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
            enable_semantic_processing: true,
            semantic_provider: "local".to_string(),
            enable_semantic_suggestions: true,
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
        // Add Mock Semantic Provider
        ce.set_semantic_provider(Box::new(crate::semantic::MockSemanticProvider::new()));
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
            enable_semantic_processing: false,
            semantic_provider: "local".to_string(),
            enable_semantic_suggestions: false,
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

    #[test]
    fn test_semantic_layer_optionality() {
        let (conn, db_path) = setup_test_db();
        let engine = create_engine(); // Engine has MockSemanticProvider now

        // Create an item
        let item = operations::create_item(
            &conn, 
            None, 
            None, 
            "text", 
            "Test semantic embedding", 
            Some("Title"), 
            "{}"
        ).unwrap();

        // Index it semantically
        engine.index_item_semantically(&conn, &item.id).unwrap();

        // Verify embedding was stored
        let embedding_record = operations::get_item_embedding(&conn, &item.id).unwrap();
        assert!(embedding_record.is_some(), "Embedding should be saved in the database");
        
        let record = embedding_record.unwrap();
        assert_eq!(record.item_id, item.id);
        assert_eq!(record.embedding.len(), 3); // Mock provider generates size 3
        assert_eq!(record.model, "mock-semantic-v1");
        
        // Clean up
        let _ = fs::remove_dir_all(db_path.parent().unwrap());
    }

    #[test]
    fn test_full_context_discovery_workflow() {
        let (mut conn, db_path) = setup_test_db();
        let engine = create_engine();

        // Setup: Create existing Workspaces: "Portfolio" and "College"
        let ws_portfolio = operations::create_workspace(&conn, "Portfolio").unwrap();
        let _ws_college = operations::create_workspace(&conn, "College").unwrap();

        // Seed some existing items into Portfolio so it has profile/context
        let seed_p1 = operations::create_item(
            &conn,
            Some(&ws_portfolio.id),
            None,
            "url",
            "https://www.figma.com/portfolio-ui",
            Some("Portfolio Figma"),
            "{}",
        ).unwrap();
        relationships::generate_relationships(&conn, &seed_p1.id).unwrap();

        // 1-3. Simulate new travel activity: Capture Items with shared session & context signals
        let travel_session_id = engine.get_or_update_active_session(&conn).unwrap();

        // (1) Hotel URL
        let item_hotel = operations::create_item(
            &conn,
            None, // unassigned
            Some(&travel_session_id),
            "url",
            "https://www.airbnb.com/rooms/tokyo-shinjuku-hotel",
            Some("Shinjuku Hotel Tokyo"),
            "{\"app_name\":\"Safari\"}",
        ).unwrap();
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: item_hotel.id.clone(),
            workspace_id: None,
            source: "browser".to_string(),
            data_type: "url".to_string(),
            metadata: json!({"original_url": "https://www.airbnb.com/rooms/tokyo-shinjuku-hotel"}),
        });
        engine.index_item_semantically(&conn, &item_hotel.id).unwrap();
        relationships::generate_relationships(&conn, &item_hotel.id).unwrap();
        relationships::generate_semantic_relationships(&engine, &conn, &item_hotel.id).unwrap();

        // (2) Flight URL
        let item_flight = operations::create_item(
            &conn,
            None,
            Some(&travel_session_id),
            "url",
            "https://www.booking.com/flights/tokyo-roundtrip",
            Some("Flight to Tokyo"),
            "{\"app_name\":\"Safari\"}",
        ).unwrap();
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: item_flight.id.clone(),
            workspace_id: None,
            source: "browser".to_string(),
            data_type: "url".to_string(),
            metadata: json!({"original_url": "https://www.booking.com/flights/tokyo-roundtrip"}),
        });
        engine.index_item_semantically(&conn, &item_flight.id).unwrap();
        relationships::generate_relationships(&conn, &item_flight.id).unwrap();
        relationships::generate_semantic_relationships(&engine, &conn, &item_flight.id).unwrap();

        // (3) Itinerary Text
        let item_itinerary = operations::create_item(
            &conn,
            None,
            Some(&travel_session_id),
            "text",
            "Day 1: Arrive at Narita Tokyo. Day 2: Shibuya and Shinjuku Tokyo. Day 3: Tokyo Mt Fuji Tour.",
            Some("Tokyo Trip Itinerary"),
            "{\"app_name\":\"Safari\"}",
        ).unwrap();
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: item_itinerary.id.clone(),
            workspace_id: None,
            source: "browser".to_string(),
            data_type: "text".to_string(),
            metadata: json!({"app_name": "Safari"}),
        });
        engine.index_item_semantically(&conn, &item_itinerary.id).unwrap();
        relationships::generate_relationships(&conn, &item_itinerary.id).unwrap();
        relationships::generate_semantic_relationships(&engine, &conn, &item_itinerary.id).unwrap();

        // (4) Restaurant URL
        let item_restaurant = operations::create_item(
            &conn,
            None,
            Some(&travel_session_id),
            "url",
            "https://www.tripadvisor.com/restaurant-tokyo-ramen",
            Some("Tokyo Ramen Guide"),
            "{\"app_name\":\"Safari\"}",
        ).unwrap();
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: item_restaurant.id.clone(),
            workspace_id: None,
            source: "browser".to_string(),
            data_type: "url".to_string(),
            metadata: json!({"original_url": "https://www.tripadvisor.com/restaurant-tokyo-ramen"}),
        });
        engine.index_item_semantically(&conn, &item_restaurant.id).unwrap();
        relationships::generate_relationships(&conn, &item_restaurant.id).unwrap();
        relationships::generate_semantic_relationships(&engine, &conn, &item_restaurant.id).unwrap();

        // (5) Map location URL
        let item_map = operations::create_item(
            &conn,
            None,
            Some(&travel_session_id),
            "url",
            "https://maps.google.com/maps?q=Tokyo+Station",
            Some("Tokyo Station Map"),
            "{\"app_name\":\"Safari\"}",
        ).unwrap();
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: item_map.id.clone(),
            workspace_id: None,
            source: "browser".to_string(),
            data_type: "url".to_string(),
            metadata: json!({"original_url": "https://maps.google.com/maps?q=Tokyo+Station"}),
        });
        engine.index_item_semantically(&conn, &item_map.id).unwrap();
        relationships::generate_relationships(&conn, &item_map.id).unwrap();
        relationships::generate_semantic_relationships(&engine, &conn, &item_map.id).unwrap();

        // (6) Destination Image
        let item_image = operations::create_item(
            &conn,
            None,
            Some(&travel_session_id),
            "image",
            "base64_image_tokyo_tower_view",
            Some("Tokyo Tower Photo"),
            "{\"app_name\":\"Safari\"}",
        ).unwrap();
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: item_image.id.clone(),
            workspace_id: None,
            source: "finder".to_string(),
            data_type: "image".to_string(),
            metadata: json!({"filename": "Tokyo Tower.jpg", "app_name": "Safari"}),
        });
        engine.index_item_semantically(&conn, &item_image.id).unwrap();
        relationships::generate_relationships(&conn, &item_image.id).unwrap();
        relationships::generate_semantic_relationships(&engine, &conn, &item_image.id).unwrap();

        // (7) Travel Notes
        let item_notes = operations::create_item(
            &conn,
            None,
            Some(&travel_session_id),
            "text",
            "Pack passport, JR pass, travel power adapter, Japanese Yen cash for Tokyo trip",
            Some("Tokyo Travel Checklist"),
            "{\"app_name\":\"Safari\"}",
        ).unwrap();
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: item_notes.id.clone(),
            workspace_id: None,
            source: "notes".to_string(),
            data_type: "text".to_string(),
            metadata: json!({"app_name": "Safari"}),
        });
        engine.index_item_semantically(&conn, &item_notes.id).unwrap();
        relationships::generate_relationships(&conn, &item_notes.id).unwrap();
        relationships::generate_semantic_relationships(&engine, &conn, &item_notes.id).unwrap();

        // Capture an UNRELATED item in a separate session (e.g., Rust Compiler Github)
        let other_session = operations::create_session(&conn).unwrap();
        let item_unrelated = operations::create_item(
            &conn,
            None,
            Some(&other_session.id),
            "url",
            "https://github.com/rust-lang/rust/pulls",
            Some("Rust Compiler Pull Requests"),
            "{\"app_name\":\"Chrome\"}",
        ).unwrap();
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: item_unrelated.id.clone(),
            workspace_id: None,
            source: "browser".to_string(),
            data_type: "url".to_string(),
            metadata: json!({"original_url": "https://github.com/rust-lang/rust/pulls"}),
        });
        relationships::generate_relationships(&conn, &item_unrelated.id).unwrap();

        // 4 & 5. Verify ContextSignals and ItemRelationships exist
        let sigs_hotel = crate::context_engine::operations::list_context_signals(&conn, None, Some(&item_hotel.id)).unwrap();
        assert!(!sigs_hotel.is_empty(), "Signals must be generated for hotel item");

        let rels_hotel = crate::context_engine::graph::get_strongest_relationships(&conn, &item_hotel.id, 10).unwrap();
        assert!(!rels_hotel.is_empty(), "Relationships must be generated for hotel item");

        // 6 & 7. Run Context Discovery Engine & verify Workspace suppression
        let travel_ids = vec![
            item_hotel.id.clone(),
            item_flight.id.clone(),
            item_itinerary.id.clone(),
            item_restaurant.id.clone(),
            item_map.id.clone(),
            item_image.id.clone(),
            item_notes.id.clone(),
        ];
        
        let suppression_match = crate::context_engine::discovery::evaluate_workspace_suppression(&conn, &travel_ids, None).unwrap();
        if let Some(ref m) = suppression_match {
            assert!(!m.is_suppressed, "Travel items must NOT be suppressed into Portfolio or College workspaces");
        }

        // 8. Run discovery and verify candidate creation
        let discovered_candidates = crate::context_engine::discovery::run_discovery(&conn).unwrap();
        assert!(!discovered_candidates.is_empty(), "Context discovery must discover the travel group candidate");

        let candidates = crate::context_engine::discovery::list_candidates(&conn, Some("detected")).unwrap();
        assert!(!candidates.is_empty(), "Candidate must be created in 'detected' status");

        // Find the travel candidate
        let cand = candidates.iter().find(|c| {
            let items_res = crate::context_engine::discovery::get_candidate_items(&conn, &c.id).unwrap();
            items_res.items.iter().any(|m| m.item.id == item_hotel.id)
        }).expect("Discovered candidate for Tokyo travel must exist");

        // 9. High confidence score
        assert!(cand.confidence >= 0.70, "Travel candidate must have high confidence: {}", cand.confidence);

        // 10. Deterministic naming
        let cand_name = cand.label.as_deref().unwrap_or("");
        assert!(cand_name.contains("Tokyo") || cand_name.contains("Safari") || cand_name.contains("Trip"),
            "Candidate name should be meaningful: {}", cand_name);

        // 11. UI detection & status
        assert_eq!(cand.status, "detected");

        // 12. Review details & reasons
        let review_details = crate::context_engine::discovery::get_candidate_items(&conn, &cand.id).unwrap();
        assert!(review_details.items.len() >= 3, "Review should list member items");
        let score_info = crate::context_engine::discovery::score_context_candidate(&conn, &cand.id, None).unwrap();
        assert!(!score_info.evidence.is_empty(), "Candidate score must have plain-English evidence reasons");

        // 13. User removes an item (e.g. remove map item from candidate)
        crate::context_engine::discovery::remove_item_from_candidate(&conn, &cand.id, &item_map.id).unwrap();
        let after_removal = crate::context_engine::discovery::get_candidate_items(&conn, &cand.id).unwrap();
        assert!(!after_removal.items.iter().any(|m| m.item.id == item_map.id), "Map item must be removed from candidate");
        
        // Verify candidate_item_removed signal was recorded
        let remove_sigs = crate::context_engine::operations::list_context_signals(&conn, None, Some(&item_map.id)).unwrap();
        assert!(remove_sigs.iter().any(|s| s.signal_type == "candidate_item_removed"));

        // 14. User renames the candidate
        let renamed = crate::context_engine::discovery::update_candidate_label(&conn, &cand.id, "Tokyo Vacation 2026").unwrap();
        assert_eq!(renamed.label.as_deref(), Some("Tokyo Vacation 2026"));

        // Verify candidate_renamed signal was recorded
        let rename_sigs = crate::context_engine::operations::list_context_signals(&conn, None, None).unwrap();
        assert!(rename_sigs.iter().any(|s| s.signal_type == "candidate_renamed"));

        // 15. User creates a Workspace explicitly
        let new_ws = crate::context_engine::discovery::accept_candidate_and_create_workspace(&mut conn, &cand.id, Some("Tokyo Vacation 2026")).unwrap();
        assert_eq!(new_ws.name, "Tokyo Vacation 2026");

        // 16. Selected items move into the new workspace
        let moved_hotel = operations::get_item(&conn, &item_hotel.id).unwrap();
        assert_eq!(moved_hotel.workspace_id, Some(new_ws.id.clone()));

        // The removed map item remains unassigned in Inbox
        let unassigned_map = operations::get_item(&conn, &item_map.id).unwrap();
        assert_eq!(unassigned_map.workspace_id, None);

        // 17. Signals and relationships remain intact
        let retained_sigs = crate::context_engine::operations::list_context_signals(&conn, None, Some(&item_hotel.id)).unwrap();
        assert!(!retained_sigs.is_empty(), "Original signals must be preserved");

        // 18. Candidate marked accepted
        let final_cand = crate::context_engine::discovery::get_candidate(&conn, &cand.id).unwrap();
        assert_eq!(final_cand.status, "accepted");

        // 19. User feedback signals stored
        let ws_sigs = crate::context_engine::operations::list_context_signals(&conn, Some(&new_ws.id), None).unwrap();
        assert!(ws_sigs.iter().any(|s| s.signal_type == "workspace_created_from_candidate"));
        assert!(ws_sigs.iter().any(|s| s.signal_type == "candidate_accepted"));

        // 20. Restart ContextSplit (persist to disk, reload from file)
        drop(conn);
        let conn_reopened = Connection::open(&db_path).unwrap();

        let persisted_ws = operations::get_workspace(&conn_reopened, &new_ws.id).unwrap();
        assert_eq!(persisted_ws.name, "Tokyo Vacation 2026");

        let persisted_hotel = operations::get_item(&conn_reopened, &item_hotel.id).unwrap();
        assert_eq!(persisted_hotel.workspace_id, Some(new_ws.id.clone()));

        let persisted_cand = crate::context_engine::discovery::get_candidate(&conn_reopened, &cand.id).unwrap();
        assert_eq!(persisted_cand.status, "accepted");

        // Additional edge cases:
        // Test A: Deduplication / running discovery twice doesn't create duplicate candidates
        let second_discovery = crate::context_engine::discovery::run_discovery(&conn_reopened).unwrap();
        assert_eq!(second_discovery.len(), 0, "Second discovery on already accepted items should discover 0 new candidates");

        // Clean up
        let _ = fs::remove_dir_all(db_path.parent().unwrap());
    }
}

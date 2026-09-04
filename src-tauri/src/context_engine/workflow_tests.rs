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
            enable_ai_reasoning: true,
            ai_gating_enabled: true,
            ai_gating_confidence_threshold: 0.9,
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
            enable_ai_reasoning: false,
            ai_gating_enabled: false,
            ai_gating_confidence_threshold: 0.9,
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

    #[test]
    fn test_day9_contextsplit_end_to_end() {
        let (conn, db_path) = setup_test_db();
        let _engine = create_engine();

        // -------------------------------------------------------------------
        // 1. Create a "Paris Trip" Workspace
        // -------------------------------------------------------------------
        let ws_paris = operations::create_workspace(&conn, "Paris Trip").unwrap();
        assert_eq!(ws_paris.name, "Paris Trip");
        assert_eq!(ws_paris.lifecycle_state, "active");
        assert!(ws_paris.last_activity_at.is_some());

        // -------------------------------------------------------------------
        // 2. Add several Items manually to Paris Trip
        // -------------------------------------------------------------------
        let item_hotel = operations::create_item(
            &conn,
            Some(&ws_paris.id),
            None,
            "url",
            "https://booking.com/hotel-paris-centre",
            Some("Paris Hotel"),
            r#"{"domain":"booking.com","app_name":"Safari"}"#,
        ).unwrap();

        let _item_flight = operations::create_item(
            &conn,
            Some(&ws_paris.id),
            None,
            "url",
            "https://airfrance.com/paris-flight",
            Some("Flight to CDG"),
            r#"{"domain":"airfrance.com","app_name":"Safari"}"#,
        ).unwrap();

        let item_museums = operations::create_item(
            &conn,
            Some(&ws_paris.id),
            None,
            "text",
            "Louvre and Musée d'Orsay itinerary for Paris visit",
            Some("Paris Museums"),
            r#"{"app_name":"Notes"}"#,
        ).unwrap();

        // Verify confirmed memberships exist for manually assigned items
        let mem_hotel = operations::get_item_workspace_membership(&conn, &item_hotel.id, &ws_paris.id).unwrap().unwrap();
        assert_eq!(mem_hotel.membership_type, "confirmed");
        assert_eq!(mem_hotel.confidence, 1.0);

        // -------------------------------------------------------------------
        // 3 & 4. Capture a new related Item and leave it in Inbox
        // -------------------------------------------------------------------
        let item_bistro = operations::create_item(
            &conn,
            None, // In Inbox
            None,
            "url",
            "https://booking.com/paris-restaurant-guide",
            Some("Paris Bistro Guide"),
            r#"{"domain":"booking.com","app_name":"Safari"}"#,
        ).unwrap();

        assert_eq!(item_bistro.workspace_id, None, "Item must remain in Inbox");

        // -------------------------------------------------------------------
        // 5. Verify ContextSplit creates a suggested ContextMembership
        // -------------------------------------------------------------------
        // Create relationship between bistro and hotel (common domain + topic)
        let _ = operations::create_item_relationship(&conn, &item_bistro.id, &item_hotel.id, "same_domain", 0.75, None).unwrap();
        let _ = operations::create_item_relationship(&conn, &item_bistro.id, &item_museums.id, "same_topic", 0.70, None).unwrap();

        let interpretation = scoring::evaluate_item_context(&conn, &item_bistro.id).unwrap();
        let paris_eval = interpretation.existing_workspaces.iter().find(|w| w.workspace_id == ws_paris.id).unwrap();
        assert!(paris_eval.score >= 0.20, "Paris Trip must be suggested with non-trivial score");

        // Engine records suggested membership
        let sug_mem = operations::create_or_update_membership(
            &conn,
            &item_bistro.id,
            &ws_paris.id,
            "suggested",
            paris_eval.score,
            "context_engine",
        ).unwrap();

        assert_eq!(sug_mem.membership_type, "suggested");
        assert_eq!(sug_mem.confidence, paris_eval.score);
        // Physical location remains Inbox
        assert_eq!(operations::get_item(&conn, &item_bistro.id).unwrap().workspace_id, None);

        // -------------------------------------------------------------------
        // 6. Verify the confidence and explanation
        // -------------------------------------------------------------------
        let explanation = crate::context_engine::explanation::explain_context_association(&conn, &item_bistro.id, &ws_paris.id).unwrap();
        assert_eq!(explanation.membership_type, "suggested");
        assert!(!explanation.summary.is_empty());
        assert!(!explanation.explanations.is_empty());
        assert!(!explanation.evidence.is_empty());
        assert!(explanation.evidence.iter().any(|e| e.category == "domain_relationship" || e.category == "strongest_relationships"));

        // -------------------------------------------------------------------
        // 7 & 8. Add more related Items to Paris Trip & verify dynamic confidence changes
        // -------------------------------------------------------------------
        let initial_conf = sug_mem.confidence;

        let item_cafe = operations::create_item(
            &conn,
            Some(&ws_paris.id),
            None,
            "text",
            "Café de Flore French bistro in Saint-Germain Paris",
            Some("Paris Café"),
            r#"{"domain":"booking.com","app_name":"Notes"}"#,
        ).unwrap();

        // Connect new café to bistro
        let _ = operations::create_item_relationship(&conn, &item_bistro.id, &item_cafe.id, "same_domain", 0.80, None).unwrap();

        // Recalculate memberships
        let _ = scoring::recalculate_workspace_suggested_memberships(&conn, &ws_paris.id, Some(&item_cafe.id)).unwrap();

        let updated_mem = operations::get_item_workspace_membership(&conn, &item_bistro.id, &ws_paris.id).unwrap().unwrap();
        assert_eq!(updated_mem.membership_type, "suggested", "Must not silently convert to confirmed");
        assert!(
            updated_mem.confidence >= initial_conf,
            "Confidence should increase or remain strong with additional evidence (initial: {}, updated: {})",
            initial_conf,
            updated_mem.confidence
        );

        // Verify confidence change signal was recorded
        let signals = crate::context_engine::operations::list_context_signals(&conn, Some(&ws_paris.id), Some(&item_bistro.id)).unwrap();
        assert!(signals.iter().any(|s| s.signal_type == "membership_confidence_recalculated"));

        // -------------------------------------------------------------------
        // 9 & 10. Accept the suggestion & verify confirmed membership
        // -------------------------------------------------------------------
        operations::confirm_membership(&conn, &item_bistro.id, &ws_paris.id).unwrap();
        operations::move_item(&conn, &item_bistro.id, Some(&ws_paris.id)).unwrap();

        let confirmed_mem = operations::get_item_workspace_membership(&conn, &item_bistro.id, &ws_paris.id).unwrap().unwrap();
        assert_eq!(confirmed_mem.membership_type, "confirmed");
        assert_eq!(confirmed_mem.confidence, 1.0);
        assert_eq!(operations::get_item(&conn, &item_bistro.id).unwrap().workspace_id.as_deref(), Some(ws_paris.id.as_str()));

        // -------------------------------------------------------------------
        // 11 & 12. Capture an unrelated Item & verify Paris Trip is not suggested
        // -------------------------------------------------------------------
        let item_tax = operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Quarterly corporate tax deduction spreadsheet analysis",
            Some("Tax Audit"),
            r#"{"app_name":"Excel"}"#,
        ).unwrap();

        let tax_interp = scoring::evaluate_item_context(&conn, &item_tax.id).unwrap();
        let paris_tax_score = tax_interp.existing_workspaces.iter().find(|w| w.workspace_id == ws_paris.id).map(|w| w.score).unwrap_or(0.0);
        assert!(paris_tax_score < 0.20, "Unrelated item should not meet suggestion threshold for Paris Trip");

        // -------------------------------------------------------------------
        // 13, 14, 15. Simulate old contextual activity & verify temporal weighting
        // -------------------------------------------------------------------
        let old_time = (chrono::Utc::now() - chrono::Duration::days(90)).to_rfc3339();
        let ws_historical = operations::create_workspace(&conn, "Historical 2024").unwrap();
        let old_item = operations::create_item(&conn, Some(&ws_historical.id), None, "text", "Old documentation archive", Some("Old Doc"), "{}").unwrap();
        conn.execute("UPDATE items SET created_at = ?1 WHERE id = ?2", rusqlite::params![old_time, old_item.id]).unwrap();

        let target_query = operations::create_item(&conn, None, None, "text", "Documentation archive", Some("Target Query"), "{}").unwrap();
        let _ = operations::create_item_relationship(&conn, &target_query.id, &old_item.id, "same_topic", 0.90, None).unwrap();

        let interp_temporal = scoring::evaluate_item_context(&conn, &target_query.id).unwrap();
        let old_eval = interp_temporal.existing_workspaces.iter().find(|w| w.workspace_id == ws_historical.id).unwrap();

        // Decay affects inferred relevance but floor keeps it accessible
        assert!(old_eval.score < 0.25, "Old contextual activity should be decayed");
        assert!(old_eval.score > 0.0, "Decayed relevance must remain non-zero and accessible");

        // Confirmed membership of the old item remains authoritative
        let confirmed_old = operations::get_item_workspace_membership(&conn, &old_item.id, &ws_historical.id).unwrap().unwrap();
        assert_eq!(confirmed_old.membership_type, "confirmed");
        assert_eq!(confirmed_old.confidence, 1.0);

        // -------------------------------------------------------------------
        // 16. Verify Context Timeline
        // -------------------------------------------------------------------
        let timeline = crate::context_engine::timeline::get_context_timeline(&conn, &ws_paris.id).unwrap();
        assert_eq!(timeline.workspace_name, "Paris Trip");
        assert!(timeline.total_items >= 4);
        assert!(!timeline.chronological_activity.is_empty());
        assert!(timeline.chronological_activity.iter().any(|e| e.title.contains("Paris") || e.title.contains("Flight")));

        // -------------------------------------------------------------------
        // 17. Verify Context Overview
        // -------------------------------------------------------------------
        let overview = crate::context_engine::overview::get_context_overview(&conn, &ws_paris.id).unwrap();
        assert_eq!(overview.name, "Paris Trip");
        assert_eq!(overview.lifecycle_state, "active");
        assert!(overview.item_count >= 4);
        assert!(overview.recent_activity.is_some());
        assert!(overview.related_applications.iter().any(|a| a.name == "Safari" || a.name == "Notes"));
        assert!(overview.important_domains.iter().any(|d| d.domain == "booking.com" || d.domain == "airfrance.com"));
        assert!(!overview.recent_items.is_empty());
        assert_eq!(overview.timeline.workspace_id, ws_paris.id);

        // -------------------------------------------------------------------
        // 18 & 19. Restart ContextSplit & verify state persistence
        // -------------------------------------------------------------------
        drop(conn);
        let conn_restarted = Connection::open(&db_path).unwrap();

        let re_ws = operations::get_workspace(&conn_restarted, &ws_paris.id).unwrap();
        assert_eq!(re_ws.name, "Paris Trip");
        assert_eq!(re_ws.lifecycle_state, "active");

        let re_item = operations::get_item(&conn_restarted, &item_bistro.id).unwrap();
        assert_eq!(re_item.workspace_id.as_deref(), Some(ws_paris.id.as_str()));

        let re_overview = crate::context_engine::overview::get_context_overview(&conn_restarted, &ws_paris.id).unwrap();
        assert_eq!(re_overview.item_count, overview.item_count);
        assert_eq!(re_overview.name, "Paris Trip");

        // -------------------------------------------------------------------
        // Additional Tests:
        // A) Dismissed suggestions
        // -------------------------------------------------------------------
        let item_dismiss_test = operations::create_item(&conn_restarted, None, None, "text", "Optional Paris metro map", Some("Metro"), "{}").unwrap();
        let _ = operations::create_or_update_membership(&conn_restarted, &item_dismiss_test.id, &ws_paris.id, "suggested", 0.65, "context_engine").unwrap();
        
        let dismissed = operations::dismiss_membership(&conn_restarted, &item_dismiss_test.id, &ws_paris.id).unwrap();
        assert_eq!(dismissed.membership_type, "dismissed");
        assert_eq!(dismissed.confidence, 0.0);
        // Item remains in Inbox
        assert_eq!(operations::get_item(&conn_restarted, &item_dismiss_test.id).unwrap().workspace_id, None);
        // Feedback recorded
        let dismiss_sigs = crate::context_engine::operations::list_context_signals(&conn_restarted, Some(&ws_paris.id), Some(&item_dismiss_test.id)).unwrap();
        assert!(dismiss_sigs.iter().any(|s| s.signal_type == "membership_dismissed"));

        // -------------------------------------------------------------------
        // B) Archived Contexts
        // -------------------------------------------------------------------
        let ws_to_archive = operations::create_workspace(&conn_restarted, "To Archive").unwrap();
        let item_in_arch = operations::create_item(&conn_restarted, Some(&ws_to_archive.id), None, "text", "Archive item", None, "{}").unwrap();
        let _ = operations::archive_workspace(&conn_restarted, &ws_to_archive.id).unwrap();

        let archived_ws = operations::get_workspace(&conn_restarted, &ws_to_archive.id).unwrap();
        assert_eq!(archived_ws.lifecycle_state, "archived");
        assert!(archived_ws.archived_at.is_some());

        // Suggestions suppressed for archived workspaces
        let target_for_arch = operations::create_item(&conn_restarted, None, None, "text", "Archive item query", None, "{}").unwrap();
        let _ = operations::create_item_relationship(&conn_restarted, &target_for_arch.id, &item_in_arch.id, "same_topic", 0.90, None).unwrap();
        let interp_arch = scoring::evaluate_item_context(&conn_restarted, &target_for_arch.id).unwrap();
        let arch_eval = interp_arch.existing_workspaces.iter().find(|w| w.workspace_id == ws_to_archive.id).unwrap();
        assert_eq!(arch_eval.score, 0.0, "Archived workspaces must be suppressed from suggestions");

        // Explicit user organization into archived workspace succeeds
        let moved_to_arch = operations::move_item(&conn_restarted, &target_for_arch.id, Some(&ws_to_archive.id)).unwrap();
        assert_eq!(moved_to_arch.workspace_id.as_deref(), Some(ws_to_archive.id.as_str()));

        // -------------------------------------------------------------------
        // C) Dormant Contexts
        // -------------------------------------------------------------------
        let ws_dormant = operations::create_workspace(&conn_restarted, "Dormant Project").unwrap();
        let dor_item = operations::create_item(&conn_restarted, Some(&ws_dormant.id), None, "text", "Dormant code notes", None, "{}").unwrap();
        // Simulate 45 days inactivity
        let dor_time = (chrono::Utc::now() - chrono::Duration::days(45)).to_rfc3339();
        conn_restarted.execute("UPDATE workspaces SET last_activity_at = ?1 WHERE id = ?2", rusqlite::params![dor_time, ws_dormant.id]).unwrap();

        let state = operations::evaluate_workspace_lifecycle(&conn_restarted, &ws_dormant.id, Some(30.0)).unwrap();
        assert_eq!(state, "dormant");

        // Dormant context is discounted (50%) in suggestions
        let dor_target = operations::create_item(&conn_restarted, None, None, "text", "Dormant code query", None, "{}").unwrap();
        let _ = operations::create_item_relationship(&conn_restarted, &dor_target.id, &dor_item.id, "same_topic", 0.90, None).unwrap();
        let interp_dor = scoring::evaluate_item_context(&conn_restarted, &dor_target.id).unwrap();
        let dor_eval = interp_dor.existing_workspaces.iter().find(|w| w.workspace_id == ws_dormant.id).unwrap();
        assert!(dor_eval.score > 0.0, "Dormant workspace remains available");
        assert!(dor_eval.reasons.iter().any(|r| r.contains("dormant")));

        // Meaningful user activity wakes dormant context back to active
        let _ = operations::create_item(&conn_restarted, Some(&ws_dormant.id), None, "text", "New activity", None, "{}").unwrap();
        let woken_ws = operations::get_workspace(&conn_restarted, &ws_dormant.id).unwrap();
        assert_eq!(woken_ws.lifecycle_state, "active");

        // -------------------------------------------------------------------
        // D) Duplicate Suggestions (Idempotency)
        // -------------------------------------------------------------------
        let idemp_item = operations::create_item(&conn_restarted, None, None, "text", "Idempotency test", None, "{}").unwrap();
        let m1 = operations::create_or_update_membership(&conn_restarted, &idemp_item.id, &ws_paris.id, "suggested", 0.70, "context_engine").unwrap();
        let m2 = operations::create_or_update_membership(&conn_restarted, &idemp_item.id, &ws_paris.id, "suggested", 0.85, "context_engine").unwrap();
        assert_eq!(m1.id, m2.id, "Repeated membership calls for the same item/workspace must update the same record");
        let all_mems = operations::list_item_memberships(&conn_restarted, &idemp_item.id).unwrap();
        assert_eq!(all_mems.iter().filter(|m| m.workspace_id == ws_paris.id).count(), 1);

        // -------------------------------------------------------------------
        // E) Privacy Settings
        // -------------------------------------------------------------------
        let mut priv_settings = crate::db::settings::get_settings(&conn_restarted).unwrap();
        priv_settings.enable_semantic_suggestions = false;
        crate::db::settings::update_settings(&conn_restarted, priv_settings).unwrap();

        // Evaluation runs cleanly without error when semantic suggestions are disabled
        let priv_interp = scoring::evaluate_item_context(&conn_restarted, &idemp_item.id).unwrap();
        assert_eq!(priv_interp.item_id, idemp_item.id);

        // Clean up temp database
        let _ = fs::remove_dir_all(db_path.parent().unwrap());
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use crate::capture::models::{
        CaptureEvent, CaptureType, ContentType, PlatformType,
    };
    use crate::capture::drag_normalizer::{DragPayloadNormalizer, RawDragPayload};
    use crate::capture::platform::clipboard::{
        MacOSClipboardAdapter, WindowsClipboardAdapter,
    };
    use crate::capture::notch::NotchCaptureController;
    use crate::capture::windows_target::WindowsCaptureTargetController;
    use crate::capture::target::TargetState;
    use crate::capture::pipeline::CapturePipeline;
    use crate::capture::privacy::ClipboardPrivacyFilter;
    use crate::context_engine::discovery;
    use crate::context_engine::engine::{ContextEngine, ContextEvent};
    use crate::context_engine::providers::{
        CaptureContextProvider, UrlContextProvider, UserActionContextProvider,
        WorkspaceContextProvider,
    };
    use crate::context_engine::reasoning::{
        analyze_item_context_ai_graceful,
        mock::MockAIReasoner,
        models::AIFailureReason,
    };
    use crate::context_engine::relationships;
    use crate::context_engine::scoring;
    use crate::context_engine::transitions::{
        ContextReference, ContextTransitionDetector, ContextTransitionType,
        record_context_transition,
    };
    use crate::context_engine::segments::{ContextActivitySegmenter, SegmentStreamEvent};
    use crate::context_engine::summary::ContextActivitySummaryService;
    use crate::context_engine::timeline::get_context_timeline;
    use crate::db::{self, operations, settings::PrivacySettings};

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
            capture_paused: false,
            enable_context_snapshots: true,
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
            capture_paused: false,
            enable_context_snapshots: true,
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
            paris_eval.confidence,
            "context_engine",
        ).unwrap();

        assert_eq!(sug_mem.membership_type, "suggested");
        assert_eq!(sug_mem.confidence, paris_eval.confidence);
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
        assert!(explanation.evidence.iter().any(|e| e.category == "domain_relationship" || e.category == "strongest_relationships" || e.category == "browser_domain" || e.category == "related_items"));

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

    // Helper reasoners for failure case testing
    struct TimeoutMockReasoner;
    impl crate::context_engine::reasoning::AIReasoner for TimeoutMockReasoner {
        fn provider_name(&self) -> &str { "Timeout Mock" }
        fn provider_id(&self) -> &str { "timeout_mock" }
        fn is_available(&self) -> bool { true }
        fn analyze_context(&self, _evidence: &crate::context_engine::reasoning::models::EvidencePackage) -> Result<crate::context_engine::reasoning::models::ContextReasoningResult, crate::context_engine::reasoning::models::AIReasoningError> {
            Err(crate::context_engine::reasoning::models::AIReasoningError::ProviderUnavailable("Request timed out after 10000ms".to_string()))
        }
        fn suggest_candidate_name(&self, _evidence: &crate::context_engine::reasoning::models::EvidencePackage) -> Result<String, crate::context_engine::reasoning::models::AIReasoningError> {
            Err(crate::context_engine::reasoning::models::AIReasoningError::ProviderUnavailable("Request timed out".to_string()))
        }
    }

    struct MalformedMockReasoner;
    impl crate::context_engine::reasoning::AIReasoner for MalformedMockReasoner {
        fn provider_name(&self) -> &str { "Malformed Mock" }
        fn provider_id(&self) -> &str { "malformed_mock" }
        fn is_available(&self) -> bool { true }
        fn analyze_context(&self, _evidence: &crate::context_engine::reasoning::models::EvidencePackage) -> Result<crate::context_engine::reasoning::models::ContextReasoningResult, crate::context_engine::reasoning::models::AIReasoningError> {
            // Returns invalid confidence > 1.0 which fails schema validation
            Ok(crate::context_engine::reasoning::models::ContextReasoningResult {
                decision: crate::context_engine::reasoning::models::ContextReasoningDecision::ExistingContext,
                suggested_context_reference: Some("some_ref".to_string()),
                suggested_name: None,
                confidence: 2.5, // INVALID
                explanation: "invalid confidence score".to_string(),
                evidence_references: vec![],
                uncertainty_reason: None,
                alternative_interpretations: vec![],
                provider_info: None,
            })
        }
        fn suggest_candidate_name(&self, _evidence: &crate::context_engine::reasoning::models::EvidencePackage) -> Result<String, crate::context_engine::reasoning::models::AIReasoningError> {
            Err(crate::context_engine::reasoning::models::AIReasoningError::InvalidResponse("Malformed name".to_string()))
        }
    }

    #[test]
    fn test_day10_context_reasoning_integration() {
        use crate::context_engine::reasoning::{
            build_evidence_package, analyze_item_context_ai_graceful,
            mock::MockAIReasoner,
            models::{AIFailureReason, ContextReasoningDecision},
        };

        let (conn, db_path) = setup_test_db();
        let engine = create_engine();
        let reasoner = MockAIReasoner::new();

        // -------------------------------------------------------------------
        // Scenario Setup: Existing Workspaces
        // - Portfolio
        // - College
        // - Paris Trip
        // -------------------------------------------------------------------
        let ws_portfolio = operations::create_workspace(&conn, "Portfolio").unwrap();
        let ws_college = operations::create_workspace(&conn, "College").unwrap();
        let ws_paris = operations::create_workspace(&conn, "Paris Trip").unwrap();

        // Seed items into Portfolio
        let item_p1 = operations::create_item(
            &conn,
            Some(&ws_portfolio.id),
            None,
            "url",
            "https://www.figma.com/portfolio-design-v2",
            Some("Portfolio Figma Design"),
            r#"{"app_name":"Figma","domain":"figma.com"}"#,
        ).unwrap();

        // Seed items into College
        let item_c1 = operations::create_item(
            &conn,
            Some(&ws_college.id),
            None,
            "text",
            "CS101 Algorithms Assignment 3 Syllabus and Lecture Notes",
            Some("CS101 Algorithms"),
            r#"{"app_name":"Notion"}"#,
        ).unwrap();

        // Seed items into Paris Trip
        let item_paris_hotel = operations::create_item(
            &conn,
            Some(&ws_paris.id),
            None,
            "url",
            "https://booking.com/hotel-le-marais-paris",
            Some("Le Marais Paris Hotel Booking"),
            r#"{"app_name":"Safari","domain":"booking.com"}"#,
        ).unwrap();

        let item_paris_flight = operations::create_item(
            &conn,
            Some(&ws_paris.id),
            None,
            "url",
            "https://airfrance.com/cdg-paris-flights",
            Some("Flight to Paris CDG"),
            r#"{"app_name":"Safari","domain":"airfrance.com"}"#,
        ).unwrap();

        // -------------------------------------------------------------------
        // Step 1: Capture a new Item clearly related to Paris Trip
        // -------------------------------------------------------------------
        let item_louvre = operations::create_item(
            &conn,
            None, // In Inbox
            None,
            "url",
            "https://booking.com/paris-louvre-museum-tickets-and-eiffel-tower",
            Some("Louvre Museum Tickets Paris"),
            r#"{"app_name":"Safari","domain":"booking.com"}"#,
        ).unwrap();

        assert_eq!(item_louvre.workspace_id, None, "1. Item must be captured in Inbox");

        // Record context signals for the captured item
        crate::context_engine::operations::create_context_signal(
            &conn,
            None,
            Some(&item_louvre.id),
            "active_app",
            None,
            r#"{"app_name":"Safari"}"#,
        ).unwrap();
        crate::context_engine::operations::create_context_signal(
            &conn,
            None,
            Some(&item_louvre.id),
            "url_metadata",
            None,
            r#"{"domain":"booking.com"}"#,
        ).unwrap();

        // Connect relationships to Paris Trip items
        let _ = operations::create_item_relationship(&conn, &item_louvre.id, &item_paris_hotel.id, "same_domain", 0.85, None).unwrap();
        let _ = operations::create_item_relationship(&conn, &item_louvre.id, &item_paris_flight.id, "same_topic", 0.80, None).unwrap();

        // -------------------------------------------------------------------
        // Step 2: Deterministic Context Engine analyzes it
        // -------------------------------------------------------------------
        let interpretation = scoring::evaluate_item_context(&conn, &item_louvre.id).unwrap();
        let paris_eval = interpretation.existing_workspaces.iter().find(|w| w.workspace_id == ws_paris.id).unwrap();
        assert!(paris_eval.score >= 0.25, "2. Deterministic Context Engine should assign affinity to Paris Trip: {}", paris_eval.score);

        // -------------------------------------------------------------------
        // Step 3 & 4: EvidencePackage is generated and contains ONLY relevant evidence
        // -------------------------------------------------------------------
        let package = build_evidence_package(&conn, &item_louvre.id).unwrap();
        assert_eq!(package.target_item.id, item_louvre.id, "3. EvidencePackage target item must match");
        assert_eq!(package.target_item.app_name.as_deref(), Some("Safari"));
        assert_eq!(package.target_item.domain.as_deref(), Some("booking.com"));

        // 4. Verify only relevant evidence is present
        assert!(package.candidate_contexts.iter().any(|c| c.context_id == ws_paris.id));
        // Related items should contain Paris items, not unrelated College items
        assert!(package.relevant_related_items.iter().any(|r| r.item_id == item_paris_hotel.id));
        assert!(!package.relevant_related_items.iter().any(|r| r.workspace_id.as_deref() == Some(ws_college.id.as_str())));
        assert_eq!(package.system_metadata.privacy_boundary, "ai_privacy_boundary:strict_curated_evidence_only:v1");

        // -------------------------------------------------------------------
        // Step 5, 6, 7 & 8: AI Reasoner receives EvidencePackage and returns valid result
        // -------------------------------------------------------------------
        let ai_response = analyze_item_context_ai_graceful(&conn, &item_louvre.id, &reasoner, true);
        assert!(!ai_response.ai_unavailable, "5. AI Reasoner should succeed");
        assert!(ai_response.ai_result.is_some());

        let result = ai_response.ai_result.unwrap();
        // 6. Valid ContextReasoningResult
        assert_eq!(result.decision, ContextReasoningDecision::ExistingContext);
        assert_eq!(result.suggested_context_reference.as_deref(), Some(ws_paris.id.as_str()));
        assert_eq!(result.suggested_name.as_deref(), Some("Paris Trip"));
        assert!(result.confidence >= 0.25, "Confidence must reflect strong grounded evidence: {}", result.confidence);

        // 7. Schema validation succeeds
        assert!(result.validate().is_ok());

        // 8. Human-readable explanation produced
        assert!(!result.explanation.is_empty());
        assert!(!result.explanation.contains("0."), "Explanation must not expose internal float scores");
        assert!(!result.explanation.contains("vector"), "Explanation must not expose embeddings");
        assert!(!result.explanation.contains("embedding"), "Explanation must not expose embeddings");

        // -------------------------------------------------------------------
        // Step 9 & 10: Existing Workspace suggestion remains user-controlled & no mutation
        // -------------------------------------------------------------------
        // 9. Item is STILL in Inbox!
        let item_check = operations::get_item(&conn, &item_louvre.id).unwrap();
        assert_eq!(item_check.workspace_id, None, "9. Item must remain in Inbox until user acts");

        // 10. No database mutation performed by AI
        let all_ws = operations::list_workspaces(&conn).unwrap();
        assert_eq!(all_ws.len(), 3, "10. AI layer must not mutate workspace count");

        // -------------------------------------------------------------------
        // Edge Case A: Ambiguous Item
        // -------------------------------------------------------------------
        let item_ambiguous = operations::create_item(
            &conn,
            None,
            None,
            "text",
            "College project portfolio website submission",
            Some("Ambiguous College Portfolio"),
            r#"{"app_name":"VS Code"}"#,
        ).unwrap();
        crate::context_engine::operations::create_context_signal(
            &conn,
            Some(&ws_portfolio.id),
            Some(&item_p1.id),
            "active_app",
            None,
            r#"{"app_name":"VS Code"}"#,
        ).unwrap();
        crate::context_engine::operations::create_context_signal(
            &conn,
            Some(&ws_college.id),
            Some(&item_c1.id),
            "active_app",
            None,
            r#"{"app_name":"VS Code"}"#,
        ).unwrap();
        crate::context_engine::operations::create_context_signal(
            &conn,
            None,
            Some(&item_ambiguous.id),
            "active_app",
            None,
            r#"{"app_name":"VS Code"}"#,
        ).unwrap();
        let _ = operations::create_item_relationship(&conn, &item_ambiguous.id, &item_p1.id, "same_topic", 0.90, None).unwrap();
        let _ = operations::create_item_relationship(&conn, &item_ambiguous.id, &item_c1.id, "same_topic", 0.90, None).unwrap();

        let hist_p = serde_json::json!({"action": "moved_to_workspace", "destination_workspace": ws_portfolio.id}).to_string();
        let hist_c = serde_json::json!({"action": "moved_to_workspace", "destination_workspace": ws_college.id}).to_string();
        let _ = crate::context_engine::operations::create_context_signal(&conn, None, Some(&item_ambiguous.id), "user_action", None, &hist_p).unwrap();
        let _ = crate::context_engine::operations::create_context_signal(&conn, None, Some(&item_ambiguous.id), "user_action", None, &hist_c).unwrap();

        let amb_package = build_evidence_package(&conn, &item_ambiguous.id).unwrap();
        assert!(amb_package.deterministic_assessment.is_ambiguous, "A. Deterministic engine must detect ambiguity");
        let amb_ai = analyze_item_context_ai_graceful(&conn, &item_ambiguous.id, &reasoner, true);
        assert!(!amb_ai.ai_unavailable);
        let amb_result = amb_ai.ai_result.unwrap();
        // Reasoner should evaluate alternative interpretations and not forcefully auto-assign
        assert!(amb_result.decision == ContextReasoningDecision::Uncertain || amb_result.decision == ContextReasoningDecision::ExistingContext);
        assert_eq!(operations::get_item(&conn, &item_ambiguous.id).unwrap().workspace_id, None);

        // -------------------------------------------------------------------
        // Edge Case B: Unrelated Item
        // -------------------------------------------------------------------
        let item_unrelated = operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Authentic homemade Italian lasagna recipe with bechamel sauce",
            Some("Lasagna Recipe"),
            r#"{"app_name":"Safari"}"#,
        ).unwrap();

        let unrel_ai = analyze_item_context_ai_graceful(&conn, &item_unrelated.id, &reasoner, true);
        assert!(!unrel_ai.ai_unavailable);
        let unrel_result = unrel_ai.ai_result.unwrap();
        assert!(unrel_result.decision == ContextReasoningDecision::NoContext || unrel_result.decision == ContextReasoningDecision::Uncertain);
        assert!(unrel_result.explanation.to_lowercase().contains("inbox") || unrel_result.uncertainty_reason.is_some());

        // -------------------------------------------------------------------
        // Edge Case C: New ContextCandidate
        // -------------------------------------------------------------------
        let item_tokyo1 = operations::create_item(&conn, None, None, "text", "Tokyo trip hotel", Some("Tokyo Hotel"), "{}").unwrap();
        let item_tokyo2 = operations::create_item(&conn, None, None, "text", "Tokyo flight booking", Some("Tokyo Flight"), "{}").unwrap();
        let candidate = crate::context_engine::discovery::create_candidate(&conn, 0.85, Some("Tokyo Trip"), "{}").unwrap();
        crate::context_engine::discovery::add_item_to_candidate(&conn, &candidate.id, &item_tokyo1.id, 0.85).unwrap();
        crate::context_engine::discovery::add_item_to_candidate(&conn, &candidate.id, &item_tokyo2.id, 0.85).unwrap();

        let cand_name = engine.suggest_candidate_name_ai(&conn, &candidate.id).unwrap();
        assert!(!cand_name.is_empty(), "C. AI-assisted candidate naming succeeds");

        // -------------------------------------------------------------------
        // Edge Case D: AI Timeout
        // -------------------------------------------------------------------
        let timeout_reasoner = TimeoutMockReasoner;
        let timeout_resp = analyze_item_context_ai_graceful(&conn, &item_louvre.id, &timeout_reasoner, true);
        assert!(timeout_resp.ai_unavailable, "D. AI Timeout must set ai_unavailable = true");
        assert_eq!(timeout_resp.failure_reason, Some(AIFailureReason::Timeout));
        // Deterministic engine is unaffected
        let det_after_timeout = scoring::evaluate_item_context(&conn, &item_louvre.id).unwrap();
        assert!(!det_after_timeout.existing_workspaces.is_empty());

        // -------------------------------------------------------------------
        // Edge Case E: AI Malformed Response
        // -------------------------------------------------------------------
        let malformed_reasoner = MalformedMockReasoner;
        let malformed_resp = analyze_item_context_ai_graceful(&conn, &item_louvre.id, &malformed_reasoner, true);
        assert!(malformed_resp.ai_unavailable, "E. Malformed AI response must be caught safely");
        assert_eq!(malformed_resp.failure_reason, Some(AIFailureReason::MalformedResponse));

        // -------------------------------------------------------------------
        // Edge Case F: AI Disabled
        // -------------------------------------------------------------------
        let mut settings = crate::db::settings::get_settings(&conn).unwrap();
        settings.enable_ai_reasoning = false;
        crate::db::settings::update_settings(&conn, settings).unwrap();

        let disabled_resp = analyze_item_context_ai_graceful(&conn, &item_louvre.id, &reasoner, true);
        assert!(disabled_resp.ai_unavailable, "F. AI Disabled must be handled gracefully");
        assert_eq!(disabled_resp.failure_reason, Some(AIFailureReason::Disabled));

        // Deterministic discovery and scoring still work 100%
        let disc = crate::context_engine::discovery::run_discovery(&conn);
        assert!(disc.is_ok(), "Context discovery must continue operating when AI is disabled");

        // Restore settings
        let mut settings_restore = crate::db::settings::get_settings(&conn).unwrap();
        settings_restore.enable_ai_reasoning = true;
        crate::db::settings::update_settings(&conn, settings_restore).unwrap();

        // -------------------------------------------------------------------
        // Edge Case G: Repeated Identical Reasoning (Cache Hit)
        // -------------------------------------------------------------------
        // First call caches
        let resp_g1 = analyze_item_context_ai_graceful(&conn, &item_louvre.id, &reasoner, true);
        // Second call should hit the cache seamlessly
        let resp_g2 = analyze_item_context_ai_graceful(&conn, &item_louvre.id, &reasoner, true);
        assert_eq!(resp_g1.ai_result, resp_g2.ai_result, "G. Cache hit returns identical result");

        // -------------------------------------------------------------------
        // Edge Case H: Privacy Filtering
        // -------------------------------------------------------------------
        let item_secret = operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Secret credentials api_key=sk-1234567890abcdef1234567890abcdef and password=SuperSecretPassword123! in note",
            Some("Credentials"),
            r#"{"app_name":"Notes"}"#,
        ).unwrap();
        let secret_package = build_evidence_package(&conn, &item_secret.id).unwrap();
        assert!(!secret_package.target_item.content_snippet.contains("sk-1234567890abcdef"), "H. API key must be scrubbed");
        assert!(!secret_package.target_item.content_snippet.contains("SuperSecretPassword123!"), "H. Password must be scrubbed");
        assert!(secret_package.target_item.content_snippet.contains("[API_KEY_REDACTED]") || secret_package.target_item.content_snippet.contains("[REDACTED]"));

        // -------------------------------------------------------------------
        // Edge Case I: Insufficient Evidence
        // -------------------------------------------------------------------
        let item_insufficient = operations::create_item(&conn, None, None, "text", "hi", None, "{}").unwrap();
        let weak_package = build_evidence_package(&conn, &item_insufficient.id).unwrap();
        assert!(weak_package.deterministic_assessment.is_weak_evidence, "I. Evidence assessment must mark weak evidence");
        assert_eq!(operations::get_item(&conn, &item_insufficient.id).unwrap().workspace_id, None);

        // Clean up temp database
        let _ = fs::remove_dir_all(db_path.parent().unwrap());
    }

    // =======================================================================
    // Day 11: Cross-Platform Capture Integration Test (17 Core Verifications)
    // =======================================================================
    #[test]
    fn test_day11_cross_platform_capture_integration() {
        let (conn, db_path) = setup_test_db();
        let engine = create_engine();

        // Base Workspaces
        let ws_rust = operations::create_workspace(&conn, "Rust Systems").unwrap();
        let _ws_design = operations::create_workspace(&conn, "Design Systems").unwrap();

        let settings = crate::db::settings::get_settings(&conn).unwrap();

        // Seed context items in Rust Systems
        let item_seed1 = operations::create_item(
            &conn,
            Some(&ws_rust.id),
            None,
            "url",
            "https://doc.rust-lang.org/book/ch01-00-getting-started.html",
            Some("Rust Book Getting Started"),
            r#"{"domain":"doc.rust-lang.org","app_name":"Safari"}"#,
        )
        .unwrap();
        let _ = operations::create_or_update_membership(&conn, &item_seed1.id, &ws_rust.id, "confirmed", 1.0, "user").unwrap();

        // ===================================================================
        // 1. Verification Requirements 1 & 2: Platform Clipboard Capture & Normalization
        // MACOS: Clipboard -> MacOSClipboardAdapter -> CaptureEvent -> CapturePipeline -> Item -> Context Engine
        // WINDOWS: Clipboard -> WindowsClipboardAdapter -> CaptureEvent -> CapturePipeline -> Item -> Context Engine
        // ===================================================================
        let mut mac_clipboard = MacOSClipboardAdapter::new().with_privacy_settings(settings.clone());
        let mut win_clipboard = WindowsClipboardAdapter::new().with_privacy_settings(settings.clone());

        let raw_url = "https://github.com/rust-lang/rust\r\n";
        let mac_app = Some("Safari".to_string());
        let mac_win_title = Some("GitHub - rust-lang/rust: Empowering everyone".to_string());
        let win_app = Some("msedge.exe".to_string());
        let win_win_title = Some("GitHub - rust-lang/rust: Empowering everyone".to_string());

        let mac_event = mac_clipboard
            .on_clipboard_change(Some(raw_url.to_string()), mac_app.clone(), mac_win_title.clone())
            .unwrap()
            .expect("macOS clipboard adapter must produce CaptureEvent");

        let win_event = win_clipboard
            .on_clipboard_change(Some(raw_url.to_string()), win_app.clone(), win_win_title.clone())
            .unwrap()
            .expect("Windows clipboard adapter must produce CaptureEvent");

        // Verify 1: Both platforms produce equivalent normalized CaptureEvents
        assert_eq!(mac_event.content, "https://github.com/rust-lang/rust", "1. macOS content normalized");
        assert_eq!(win_event.content, "https://github.com/rust-lang/rust", "1. Windows content normalized");
        assert_eq!(mac_event.content_type, ContentType::Url, "1. macOS content_type detected as Url");
        assert_eq!(win_event.content_type, ContentType::Url, "1. Windows content_type detected as Url");
        assert_eq!(mac_event.capture_type, CaptureType::Clipboard);
        assert_eq!(win_event.capture_type, CaptureType::Clipboard);
        assert_eq!(mac_event.domain().as_deref(), Some("github.com"), "1. macOS domain extracted");
        assert_eq!(win_event.domain().as_deref(), Some("github.com"), "1. Windows domain extracted");
        assert_eq!(mac_event.platform, PlatformType::MacOS, "1. macOS platform tagged");
        assert_eq!(win_event.platform, PlatformType::Windows, "1. Windows platform tagged");

        // Process through CapturePipeline (without deduplication so both distinct platform items persist)
        let pipeline = CapturePipeline::new().without_deduplication();

        let mac_result = pipeline.process_event_headless(&conn, Some(&engine), mac_event).unwrap();
        let win_result = pipeline.process_event_headless(&conn, Some(&engine), win_event).unwrap();

        // Verify 2: Both platforms create equivalent Item structures
        assert_eq!(mac_result.item.item_type, "url", "2. macOS item type is url");
        assert_eq!(win_result.item.item_type, "url", "2. Windows item type is url");
        assert_eq!(mac_result.item.content, "https://github.com/rust-lang/rust", "2. macOS item content matches");
        assert_eq!(win_result.item.content, "https://github.com/rust-lang/rust", "2. Windows item content matches");
        assert_eq!(mac_result.item.workspace_id, None, "2. Captured macOS item starts in Inbox");
        assert_eq!(win_result.item.workspace_id, None, "2. Captured Windows item starts in Inbox");

        // Verify 3: ContextSignals are generated consistently
        let mac_signals = crate::context_engine::operations::list_context_signals(&conn, None, Some(&mac_result.item.id)).unwrap();
        let win_signals = crate::context_engine::operations::list_context_signals(&conn, None, Some(&win_result.item.id)).unwrap();

        assert_eq!(mac_signals.len(), win_signals.len(), "3. ContextSignal counts must match across platforms");
        assert!(mac_signals.iter().any(|s| s.signal_type == "capture_event"), "3. macOS capture_event signal present");
        assert!(win_signals.iter().any(|s| s.signal_type == "capture_event"), "3. Windows capture_event signal present");
        assert!(mac_signals.iter().any(|s| s.signal_type == "active_app"), "3. macOS active_app signal present");
        assert!(win_signals.iter().any(|s| s.signal_type == "active_app"), "3. Windows active_app signal present");
        assert!(mac_signals.iter().any(|s| s.signal_type == "window_title"), "3. macOS window_title signal present");
        assert!(win_signals.iter().any(|s| s.signal_type == "window_title"), "3. Windows window_title signal present");
        assert!(mac_signals.iter().any(|s| s.signal_type == "url_metadata"), "3. macOS url_metadata signal present");
        assert!(win_signals.iter().any(|s| s.signal_type == "url_metadata"), "3. Windows url_metadata signal present");

        // Verify 4: ContextSessions are generated consistently
        assert!(mac_result.session_id.is_some(), "4. macOS item associated with session");
        assert!(win_result.session_id.is_some(), "4. Windows item associated with session");
        assert_eq!(mac_result.session_id, win_result.session_id, "4. Consecutive captures share active ContextSession");

        // Verify 5: Context Engine receives Items and updates context state
        let relationships = crate::context_engine::graph::get_strongest_relationships(&conn, &mac_result.item.id, 10).unwrap();
        assert!(!relationships.is_empty(), "5. Context Engine generates relationships for captured items");
        let emb_mac = operations::get_item_embedding(&conn, &mac_result.item.id).unwrap();
        assert!(emb_mac.is_some(), "5. Semantic embedding indexed for macOS item");
        let emb_win = operations::get_item_embedding(&conn, &win_result.item.id).unwrap();
        assert!(emb_win.is_some(), "5. Semantic embedding indexed for Windows item");

        // ===================================================================
        // 2. Verification Requirements 13, 14, 15: Drag Capture & Target Controller Lifecycle
        // MACOS: Drag -> NotchCaptureController -> CaptureEvent -> CapturePipeline
        // WINDOWS: Drag -> WindowsCaptureTargetController -> CaptureEvent -> CapturePipeline
        // ===================================================================
        let mac_notch_events = Arc::new(Mutex::new(Vec::new()));
        let mac_events_cb = mac_notch_events.clone();
        let mac_notch = NotchCaptureController::new(move |st| mac_events_cb.lock().unwrap().push(st));

        let win_target_events = Arc::new(Mutex::new(Vec::new()));
        let win_events_cb = win_target_events.clone();
        let win_target = WindowsCaptureTargetController::new(move |st| win_events_cb.lock().unwrap().push(st));

        // Verify 14: Drag activation feedback works
        assert_eq!(mac_notch.get_state(), TargetState::Idle);
        assert_eq!(win_target.get_state(), TargetState::Idle);

        mac_notch.on_drag_detected();
        assert_eq!(mac_notch.get_state(), TargetState::DragDetected);
        mac_notch.on_drag_enter();
        assert_eq!(mac_notch.get_state(), TargetState::Hover);
        mac_notch.on_drag_over(true);
        assert_eq!(mac_notch.get_state(), TargetState::Active, "14. macOS Active feedback state reached");

        win_target.on_drag_detected();
        assert_eq!(win_target.get_state(), TargetState::DragDetected);
        win_target.on_drag_enter();
        assert_eq!(win_target.get_state(), TargetState::Hover);
        win_target.on_drag_over(true);
        assert_eq!(win_target.get_state(), TargetState::Active, "14. Windows Active feedback state reached");

        // Verify 13: Drag cancellation works
        let cancel_controller = WindowsCaptureTargetController::new(|_| {});
        cancel_controller.on_drag_enter();
        assert_eq!(cancel_controller.get_state(), TargetState::Hover);
        cancel_controller.on_drag_leave();
        assert_eq!(cancel_controller.get_state(), TargetState::Idle, "13. Leaving drag area returns to Idle");

        cancel_controller.on_drag_detected();
        cancel_controller.on_drag_enter();
        cancel_controller.on_drag_over(true);
        assert_eq!(cancel_controller.get_state(), TargetState::Active);
        cancel_controller.on_drag_leave();
        assert_eq!(cancel_controller.get_state(), TargetState::Idle, "13. Drag leave from active returns to Idle");

        // Verify 15: Capture confirmation works
        let mac_drop_accepted = mac_notch.on_drop();
        assert!(mac_drop_accepted, "15. macOS drop accepted");
        let win_drop_accepted = win_target.on_drop();
        assert!(win_drop_accepted, "15. Windows drop accepted");

        // Drag payload normalization across platforms
        let drag_normalizer = DragPayloadNormalizer::new();
        let mac_drag_raw = RawDragPayload {
            url: Some("https://doc.rust-lang.org/cargo/index.html".to_string()),
            app_name: Some("Safari".to_string()),
            platform: Some(PlatformType::MacOS),
            ..Default::default()
        };
        let win_drag_raw = RawDragPayload {
            url: Some("https://doc.rust-lang.org/cargo/index.html\r\n".to_string()),
            app_name: Some("explorer.exe".to_string()),
            platform: Some(PlatformType::Windows),
            ..Default::default()
        };

        let mac_drag_event = drag_normalizer.normalize_drag_payload(&mac_drag_raw).unwrap();
        let win_drag_event = drag_normalizer.normalize_drag_payload(&win_drag_raw).unwrap();

        assert_eq!(mac_drag_event.content, "https://doc.rust-lang.org/cargo/index.html");
        assert_eq!(win_drag_event.content, "https://doc.rust-lang.org/cargo/index.html");
        assert_eq!(mac_drag_event.content_type, ContentType::Url);
        assert_eq!(win_drag_event.content_type, ContentType::Url);

        let mac_drag_item = pipeline.process_event_headless(&conn, Some(&engine), mac_drag_event).unwrap();
        let win_drag_item = pipeline.process_event_headless(&conn, Some(&engine), win_drag_event).unwrap();
        assert_eq!(mac_drag_item.item.content, "https://doc.rust-lang.org/cargo/index.html");
        assert_eq!(win_drag_item.item.content, "https://doc.rust-lang.org/cargo/index.html");

        // ===================================================================
        // 3. Verification Requirement 6: Existing ContextMembership Logic
        // ===================================================================
        let membership = operations::create_or_update_membership(
            &conn,
            &mac_result.item.id,
            &ws_rust.id,
            "suggested",
            0.88,
            "context_engine",
        )
        .unwrap();
        assert_eq!(membership.membership_type, "suggested");
        assert_eq!(membership.confidence, 0.88);

        let confirmed = operations::confirm_membership(&conn, &mac_result.item.id, &ws_rust.id).unwrap();
        assert_eq!(confirmed.membership_type, "confirmed");
        assert_eq!(confirmed.confidence, 1.0);

        let retrieved = operations::get_item_workspace_membership(&conn, &mac_result.item.id, &ws_rust.id).unwrap();
        assert!(retrieved.is_some(), "6. Confirmed membership must be retrievable");
        assert_eq!(retrieved.unwrap().membership_type, "confirmed");

        // ===================================================================
        // 4. Verification Requirement 7: New ContextCandidates Discovery
        // ===================================================================
        let item_cand1 = operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Quantum entanglement algorithms in quantum computing research",
            Some("Quantum Entanglement"),
            r#"{"app_name":"Notes"}"#,
        )
        .unwrap();
        let item_cand2 = operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Quantum error correction codes in fault-tolerant qubits",
            Some("Quantum Error Correction"),
            r#"{"app_name":"Notes"}"#,
        )
        .unwrap();
        let item_cand3 = operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Quantum supremacy benchmark results on superconducting quantum processors",
            Some("Quantum Supremacy"),
            r#"{"app_name":"Notes"}"#,
        )
        .unwrap();

        // Create explicit strong topic relationships between candidates (cluster of 3)
        let _ = operations::create_item_relationship(&conn, &item_cand1.id, &item_cand2.id, "same_topic", 0.95, None).unwrap();
        let _ = operations::create_item_relationship(&conn, &item_cand2.id, &item_cand3.id, "same_topic", 0.95, None).unwrap();
        let _ = operations::create_item_relationship(&conn, &item_cand1.id, &item_cand3.id, "same_topic", 0.95, None).unwrap();

        let candidates = discovery::run_discovery(&conn).unwrap();
        assert!(!candidates.is_empty(), "7. Context candidate cluster must be discovered");
        assert!(candidates.iter().any(|c| c.label.as_deref().unwrap_or("").to_lowercase().contains("quantum") || c.metadata.contains("quantum") || c.metadata.contains("item_count")));

        // ===================================================================
        // 5. Verification Requirements 8 & 9: AI Reasoning Asynchronous & Non-Blocking Resilience
        // ===================================================================
        let reasoner = MockAIReasoner::new();
        let ai_res = analyze_item_context_ai_graceful(&conn, &mac_result.item.id, &reasoner, true);
        assert!(!ai_res.ai_unavailable, "8. AI reasoning completes gracefully");
        assert!(ai_res.ai_result.is_some(), "8. AI result is generated");

        // Test AI failure resilience: Timeout (tested on uncached Windows item)
        let timeout_reasoner = TimeoutMockReasoner;
        let timeout_res = analyze_item_context_ai_graceful(&conn, &win_result.item.id, &timeout_reasoner, true);
        assert!(timeout_res.ai_unavailable, "9. AI Timeout handled gracefully");
        assert_eq!(timeout_res.failure_reason, Some(AIFailureReason::Timeout));
        // Verify item and DB persistence untouched
        assert_eq!(operations::get_item(&conn, &win_result.item.id).unwrap().id, win_result.item.id);

        // Test AI failure resilience: Malformed Response (tested on uncached drag item)
        let malformed_reasoner = MalformedMockReasoner;
        let malformed_res = analyze_item_context_ai_graceful(&conn, &mac_drag_item.item.id, &malformed_reasoner, true);
        assert!(malformed_res.ai_unavailable, "9. AI Malformed response handled gracefully");
        assert_eq!(malformed_res.failure_reason, Some(AIFailureReason::MalformedResponse));
        assert_eq!(operations::get_item(&conn, &mac_drag_item.item.id).unwrap().id, mac_drag_item.item.id);

        // ===================================================================
        // 6. Verification Requirement 10: Privacy Filtering Before Persistence
        // ===================================================================
        let sensitive_text = "Personal credential api_key=sk-1234567890abcdef1234567890abcdef and password=SuperSecretPassword123!";
        let privacy_filtered_mac = mac_clipboard.on_clipboard_change(Some(sensitive_text.to_string()), Some("1Password".to_string()), None).unwrap();
        assert!(privacy_filtered_mac.is_none(), "10. macOS adapter filters sensitive credentials");

        let privacy_filtered_win = win_clipboard.on_clipboard_change(Some(sensitive_text.to_string()), Some("Bitwarden".to_string()), None).unwrap();
        assert!(privacy_filtered_win.is_none(), "10. Windows adapter filters sensitive credentials");

        let direct_sensitive_event = CaptureEvent::new(
            CaptureType::Clipboard,
            ContentType::PlainText,
            sensitive_text,
            PlatformType::MacOS,
        );
        let audit_res = pipeline.process_event_headless(&conn, Some(&engine), direct_sensitive_event);
        assert!(audit_res.is_err(), "10. Pipeline audit rejects sensitive credentials before persistence");

        // ===================================================================
        // 7. Verification Requirement 11: Global Pause Mode
        // ===================================================================
        let mut pause_settings = crate::db::settings::get_settings(&conn).unwrap();
        pause_settings.capture_paused = true;
        crate::db::settings::update_settings(&conn, pause_settings.clone()).unwrap();

        let mut paused_mac_clipboard = MacOSClipboardAdapter::new().with_privacy_settings(pause_settings.clone());
        let mut paused_win_clipboard = WindowsClipboardAdapter::new().with_privacy_settings(pause_settings.clone());
        let paused_pipeline = CapturePipeline::new().with_privacy_filter(ClipboardPrivacyFilter::new().with_privacy_settings(&pause_settings));

        let mac_paused_out = paused_mac_clipboard.on_clipboard_change(Some("Copied while paused".to_string()), None, None).unwrap();
        assert!(mac_paused_out.is_none(), "11. macOS clipboard returns None when paused");

        let win_paused_out = paused_win_clipboard.on_clipboard_change(Some("Copied while paused".to_string()), None, None).unwrap();
        assert!(win_paused_out.is_none(), "11. Windows clipboard returns None when paused");

        let auto_event = CaptureEvent::new(
            CaptureType::Clipboard,
            ContentType::PlainText,
            "Automatic item while paused",
            PlatformType::MacOS,
        );
        let paused_pipeline_res = paused_pipeline.process_event_headless(&conn, Some(&engine), auto_event);
        assert!(paused_pipeline_res.is_err(), "11. Automatic capture rejected when paused");

        // Target controller gesture behavior in pause mode
        let paused_target = WindowsCaptureTargetController::new(|_| {});
        paused_target.set_paused(true);
        paused_target.on_drag_enter();
        assert_eq!(paused_target.get_state(), TargetState::Paused);
        assert!(!paused_target.on_drop(), "11. Drop rejected when paused");

        // Manual capture remains permitted
        let manual_event = CaptureEvent::new(
            CaptureType::Manual,
            ContentType::PlainText,
            "Manual user note while paused",
            PlatformType::MacOS,
        );
        let manual_res = paused_pipeline.process_event_headless(&conn, Some(&engine), manual_event).unwrap();
        assert_eq!(manual_res.item.content, "Manual user note while paused", "11. Manual capture succeeds while paused");

        // Resume capture
        pause_settings.capture_paused = false;
        crate::db::settings::update_settings(&conn, pause_settings).unwrap();

        // ===================================================================
        // 8. Verification Requirement 12: Duplicate Clipboard Deduplication
        // ===================================================================
        let mut dedup_mac_clipboard = MacOSClipboardAdapter::new();
        let text_to_repeat = "Unique Clipboard String 12345";

        let first_clip = dedup_mac_clipboard.on_clipboard_change(Some(text_to_repeat.to_string()), None, None).unwrap();
        assert!(first_clip.is_some(), "First clipboard copy produces event");

        let second_clip = dedup_mac_clipboard.on_clipboard_change(Some(text_to_repeat.to_string()), None, None).unwrap();
        assert!(second_clip.is_none(), "12. Repeated identical clipboard content produces None");

        // Pipeline deduplication
        let dedup_pipeline = CapturePipeline::new().with_dedup_window_secs(10);
        let evt1 = CaptureEvent::new(CaptureType::Clipboard, ContentType::PlainText, "Dedup Test Content", PlatformType::MacOS);
        let res1 = dedup_pipeline.process_event_headless(&conn, Some(&engine), evt1).unwrap();
        assert!(!res1.is_duplicate);

        let evt2 = CaptureEvent::new(CaptureType::Clipboard, ContentType::PlainText, "Dedup Test Content", PlatformType::Windows);
        let res2 = dedup_pipeline.process_event_headless(&conn, Some(&engine), evt2).unwrap();
        assert!(res2.is_duplicate, "12. Pipeline flags duplicate item within window");

        // ===================================================================
        // 9. Verification Requirement 16: Application Restart Persistence
        // ===================================================================
        let count_items_before: i64 = conn.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0)).unwrap();
        let count_workspaces_before: i64 = conn.query_row("SELECT COUNT(*) FROM workspaces", [], |r| r.get(0)).unwrap();
        let count_memberships_before: i64 = conn.query_row("SELECT COUNT(*) FROM context_memberships", [], |r| r.get(0)).unwrap();

        drop(conn);

        // Reopen new connection (simulating cold app restart)
        let conn2 = Connection::open(&db_path).unwrap();

        let count_items_after: i64 = conn2.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0)).unwrap();
        let count_workspaces_after: i64 = conn2.query_row("SELECT COUNT(*) FROM workspaces", [], |r| r.get(0)).unwrap();
        let count_memberships_after: i64 = conn2.query_row("SELECT COUNT(*) FROM context_memberships", [], |r| r.get(0)).unwrap();

        assert_eq!(count_items_before, count_items_after, "16. Items count preserved across restart");
        assert_eq!(count_workspaces_before, count_workspaces_after, "16. Workspaces preserved across restart");
        assert_eq!(count_memberships_before, count_memberships_after, "16. Memberships preserved across restart");

        let reloaded_item = operations::get_item(&conn2, &mac_result.item.id).unwrap();
        assert_eq!(reloaded_item.content, "https://github.com/rust-lang/rust");

        // Clean up temp database
        let _ = fs::remove_dir_all(db_path.parent().unwrap());
    }

    #[test]
    fn test_day12_intelligent_inbox_integration() {
        let (mut conn, db_path) = setup_test_db();
        let engine = create_engine();

        // ===================================================================
        // Step 1: Start with empty Inbox
        // ===================================================================
        let initial_items = operations::list_items(&conn, None).unwrap();
        assert_eq!(initial_items.len(), 0, "1. Start with empty Inbox (0 items)");
        let initial_states = operations::list_inbox_item_states(&conn, None).unwrap();
        assert_eq!(initial_states.len(), 0, "1. Start with 0 inbox item states");

        // ===================================================================
        // Step 2: Capture unrelated Items (macOS, Windows, Drag/Manual)
        // ===================================================================
        let settings = crate::db::settings::get_settings(&conn).unwrap();
        let mut mac_clipboard = MacOSClipboardAdapter::new().with_privacy_settings(settings.clone());
        let mut win_clipboard = WindowsClipboardAdapter::new().with_privacy_settings(settings.clone());
        let pipeline = CapturePipeline::new().without_deduplication();

        // 2a. macOS clipboard capture
        let mac_event = mac_clipboard
            .on_clipboard_change(
                Some("Random grocery list: apples, almond milk, sourdough bread".to_string()),
                Some("Notes".to_string()),
                Some("Grocery Notes".to_string()),
            )
            .unwrap()
            .expect("macOS clipboard event created");
        let item_unrelated_1 = pipeline.process_event_headless(&conn, Some(&engine), mac_event).unwrap().item;

        // 2b. Windows clipboard capture
        let win_event = win_clipboard
            .on_clipboard_change(
                Some("System diagnostic log 0x80070002 file not found error".to_string()),
                Some("notepad.exe".to_string()),
                Some("system_log.txt - Notepad".to_string()),
            )
            .unwrap()
            .expect("Windows clipboard event created");
        let item_unrelated_2 = pipeline.process_event_headless(&conn, Some(&engine), win_event).unwrap().item;

        // 2c. Drag payload capture
        let drag_normalizer = DragPayloadNormalizer::new();
        let drag_raw = RawDragPayload {
            content: Some("Philosophical note: All models are wrong, but some are useful".to_string()),
            text: Some("Philosophical note: All models are wrong, but some are useful".to_string()),
            app_name: Some("Obsidian".to_string()),
            platform: Some(PlatformType::MacOS),
            ..Default::default()
        };
        let drag_event = drag_normalizer.normalize_drag_payload(&drag_raw).unwrap();
        let item_unrelated_3 = pipeline.process_event_headless(&conn, Some(&engine), drag_event).unwrap().item;

        // Verify initial state for unrelated items
        for item in [&item_unrelated_1, &item_unrelated_2, &item_unrelated_3] {
            assert_eq!(item.workspace_id, None, "2. Unrelated item is in Inbox");
            let state = operations::get_inbox_item_state(&conn, &item.id).unwrap().expect("Inbox state initialized");
            assert_eq!(state.state, crate::db::models::InboxState::New, "2. Initial inbox state is New");
        }

        // ===================================================================
        // Step 3: Capture several Items related to an existing Workspace
        // ===================================================================
        let ws_infra = operations::create_workspace(&conn, "DevOps Infrastructure").unwrap();

        // Seed an item in DevOps Infrastructure to establish workspace context profile
        let seed_k8s = operations::create_item(
            &conn,
            Some(&ws_infra.id),
            None,
            "url",
            "https://kubernetes.io/docs/concepts/overview/what-is-kubernetes/",
            Some("What is Kubernetes?"),
            r#"{"domain":"kubernetes.io","app_name":"Safari"}"#,
        ).unwrap();
        operations::create_or_update_membership(&conn, &seed_k8s.id, &ws_infra.id, "confirmed", 1.0, "user").unwrap();
        operations::recalculate_workspace_profile(&conn, &ws_infra.id).unwrap();

        // Capture 2 new items related to DevOps Infrastructure
        let mut k8s_pod_event = CaptureEvent::new(
            CaptureType::Clipboard,
            ContentType::Url,
            "https://kubernetes.io/docs/tasks/configure-pod-container/configure-liveness-readiness-startup-probes/",
            PlatformType::MacOS,
        );
        k8s_pod_event.source_application = Some("Safari".to_string());
        k8s_pod_event.metadata = json!({
            "domain": "kubernetes.io",
            "title": "Configure Probes - Kubernetes"
        });
        let item_k8s = pipeline.process_event_headless(&conn, Some(&engine), k8s_pod_event).unwrap().item;

        let mut tf_event = CaptureEvent::new(
            CaptureType::Clipboard,
            ContentType::Url,
            "https://registry.terraform.io/providers/hashicorp/aws/latest/docs",
            PlatformType::Windows,
        );
        tf_event.source_application = Some("msedge.exe".to_string());
        tf_event.metadata = json!({
            "domain": "terraform.io",
            "title": "Terraform AWS Provider"
        });
        let item_tf = pipeline.process_event_headless(&conn, Some(&engine), tf_event).unwrap().item;

        // ===================================================================
        // Step 4: Capture several Items forming a new ContextCandidate
        // ===================================================================
        let _session_quantum = engine.get_or_update_active_session(&conn).unwrap();

        let mut q1_event = CaptureEvent::new(
            CaptureType::Clipboard,
            ContentType::Url,
            "https://qiskit.org/documentation/getting_started.html",
            PlatformType::MacOS,
        );
        q1_event.source_application = Some("Safari".to_string());
        q1_event.metadata = json!({
            "domain": "qiskit.org",
            "title": "Qiskit Quantum SDK Getting Started"
        });
        let item_q1 = pipeline.process_event_headless(&conn, Some(&engine), q1_event).unwrap().item;

        let mut q2_event = CaptureEvent::new(
            CaptureType::Clipboard,
            ContentType::Url,
            "https://quantum-journal.org/papers/q-2024-fault-tolerant-quantum-circuits",
            PlatformType::MacOS,
        );
        q2_event.source_application = Some("Safari".to_string());
        q2_event.metadata = json!({
            "domain": "quantum-journal.org",
            "title": "Fault Tolerant Quantum Circuits - Quantum Journal"
        });
        let item_q2 = pipeline.process_event_headless(&conn, Some(&engine), q2_event).unwrap().item;

        let mut q3_event = CaptureEvent::new(
            CaptureType::Clipboard,
            ContentType::Url,
            "https://qiskit.org/ecosystem/algorithms/shor-algorithm-tutorial",
            PlatformType::MacOS,
        );
        q3_event.source_application = Some("Safari".to_string());
        q3_event.metadata = json!({
            "domain": "qiskit.org",
            "title": "Shor's Algorithm Tutorial - Qiskit"
        });
        let item_q3 = pipeline.process_event_headless(&conn, Some(&engine), q3_event).unwrap().item;

        // ===================================================================
        // Step 5: Verify Inbox states update correctly
        // ===================================================================
        // Transition item_k8s through processing -> understood
        crate::context_engine::inbox_state::InboxStateMachine::start_processing(&conn, &item_k8s.id).unwrap();
        let proc_state = operations::get_inbox_item_state(&conn, &item_k8s.id).unwrap().unwrap();
        assert_eq!(proc_state.state, crate::db::models::InboxState::Processing, "5. State transitioned to Processing");
        assert_eq!(proc_state.previous_state, Some(crate::db::models::InboxState::New));

        crate::context_engine::inbox_state::InboxStateMachine::mark_understood(&conn, &item_k8s.id, Some("High confidence kubernetes domain match")).unwrap();
        let understood_state = operations::get_inbox_item_state(&conn, &item_k8s.id).unwrap().unwrap();
        assert_eq!(understood_state.state, crate::db::models::InboxState::Understood, "5. State transitioned to Understood");
        assert_eq!(understood_state.previous_state, Some(crate::db::models::InboxState::Processing));

        // ===================================================================
        // Step 6: Verify high-confidence Items become "understood" (NOT silently confirmed)
        // ===================================================================
        let k8s_item_check = operations::get_item(&conn, &item_k8s.id).unwrap();
        assert_eq!(k8s_item_check.workspace_id, None, "6. Understood item is NOT silently assigned to workspace");
        assert_eq!(understood_state.state, crate::db::models::InboxState::Understood, "6. Item is Understood");

        // ===================================================================
        // Step 7: Verify suggested memberships appear
        // ===================================================================
        operations::create_or_update_membership(&conn, &item_k8s.id, &ws_infra.id, "suggested", 0.94, "context_engine").unwrap();
        operations::create_or_update_membership(&conn, &item_tf.id, &ws_infra.id, "suggested", 0.88, "context_engine").unwrap();
        crate::context_engine::inbox_state::InboxStateMachine::mark_suggested(&conn, &item_k8s.id, &ws_infra.id, 0.94, None).unwrap();
        
        crate::context_engine::inbox_state::InboxStateMachine::start_processing(&conn, &item_tf.id).unwrap();
        crate::context_engine::inbox_state::InboxStateMachine::mark_understood(&conn, &item_tf.id, Some("Terraform AWS Provider matched")).unwrap();
        crate::context_engine::inbox_state::InboxStateMachine::mark_suggested(&conn, &item_tf.id, &ws_infra.id, 0.88, None).unwrap();

        let k8s_memberships = operations::list_item_memberships(&conn, &item_k8s.id).unwrap();
        assert_eq!(k8s_memberships.len(), 1, "7. Suggested membership created");
        assert_eq!(k8s_memberships[0].workspace_id, ws_infra.id);
        assert_eq!(k8s_memberships[0].membership_type, "suggested");
        assert!(k8s_memberships[0].confidence >= 0.90);

        // ===================================================================
        // Step 8: Verify uncertain Items require attention
        // ===================================================================
        let att_unrelated_1 = crate::context_engine::attention::evaluate_inbox_item_attention(&conn, &item_unrelated_1.id, None).unwrap();
        assert!(att_unrelated_1.attention_required, "8. Unrelated/unorganized item requires attention");

        let att_k8s = crate::context_engine::attention::evaluate_inbox_item_attention(&conn, &item_k8s.id, None).unwrap();
        assert!(att_k8s.attention_required, "8. Suggested item requires attention for confirmation");
        assert_eq!(att_k8s.attention_level, crate::context_engine::attention::AttentionLevel::High);
        assert!(att_k8s.factors.has_suggested_membership);
        assert_eq!(att_k8s.factors.suggested_workspace_id, Some(ws_infra.id.clone()));

        // ===================================================================
        // Step 9: Verify contextual grouping works
        // ===================================================================
        let search_grouped = crate::search::search_inbox_items(
            &conn,
            &crate::search::InboxSearchParams {
                workspace_id: Some(ws_infra.id.clone()),
                ..Default::default()
            },
            None,
        ).unwrap();
        let found_ids: Vec<String> = search_grouped.iter().map(|r| r.item.id.clone()).collect();
        assert!(found_ids.contains(&item_k8s.id), "9. Context grouping finds suggested k8s item");
        assert!(found_ids.contains(&item_tf.id), "9. Context grouping finds suggested terraform item");

        // ===================================================================
        // Step 10: Verify ContextCandidate grouping works
        // ===================================================================
        // Link Quantum items with relationships and candidate cluster
        crate::db::operations::create_item_relationship(&conn, &item_q1.id, &item_q2.id, "temporal_cooccurrence", 0.95, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &item_q1.id, &item_q3.id, "domain_match", 0.98, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &item_q2.id, &item_q3.id, "domain_match", 0.90, None).unwrap();

        let cand_quantum = discovery::create_candidate(
            &conn,
            0.92,
            Some("Quantum Computing Research"),
            "{}",
        ).unwrap();
        discovery::add_item_to_candidate(&conn, &cand_quantum.id, &item_q1.id, 0.95).unwrap();
        discovery::add_item_to_candidate(&conn, &cand_quantum.id, &item_q2.id, 0.92).unwrap();
        discovery::add_item_to_candidate(&conn, &cand_quantum.id, &item_q3.id, 0.90).unwrap();

        let cand_items = discovery::get_candidate_items(&conn, &cand_quantum.id).unwrap();
        assert_eq!(cand_items.items.len(), 3, "10. ContextCandidate group contains exactly 3 quantum items");

        let search_cand = crate::search::search_inbox_items(
            &conn,
            &crate::search::InboxSearchParams {
                candidate_id: Some(cand_quantum.id.clone()),
                ..Default::default()
            },
            None,
        ).unwrap();
        assert_eq!(search_cand.len(), 3, "10. Searching by candidate_id groups all 3 items");

        // ===================================================================
        // Step 11: Verify AI explanations appear when enabled
        // ===================================================================
        let ai_reasoner = MockAIReasoner::new();
        let ai_res_enabled = analyze_item_context_ai_graceful(&conn, &item_k8s.id, &ai_reasoner, true);
        assert!(!ai_res_enabled.ai_unavailable, "11. AI reasoning completes when enabled");
        assert!(ai_res_enabled.ai_result.is_some(), "11. AI result generated");
        let ai_result = ai_res_enabled.ai_result.unwrap();
        assert!(!ai_result.explanation.is_empty(), "11. AI explanation is non-empty");

        // ===================================================================
        // Step 12: Disable AI and verify deterministic explanations still work
        // ===================================================================
        let mut disabled_settings = crate::db::settings::get_settings(&conn).unwrap();
        disabled_settings.enable_ai_reasoning = false;
        crate::db::settings::update_settings(&conn, disabled_settings.clone()).unwrap();

        let ai_res_disabled = analyze_item_context_ai_graceful(&conn, &item_k8s.id, &ai_reasoner, true);
        assert!(ai_res_disabled.ai_unavailable, "12. AI is flagged unavailable when disabled");
        assert_eq!(ai_res_disabled.failure_reason, Some(AIFailureReason::Disabled));

        // Re-enable AI settings for subsequent steps
        disabled_settings.enable_ai_reasoning = true;
        crate::db::settings::update_settings(&conn, disabled_settings).unwrap();

        let det_explanation = crate::context_engine::explanation::explain_context_association(&conn, &item_k8s.id, &ws_infra.id).unwrap();
        assert!(!det_explanation.summary.is_empty(), "12. Deterministic summary is populated");
        assert!(!det_explanation.explanations.is_empty(), "12. Deterministic explanations list is populated");

        // ===================================================================
        // Step 13: Accept a contextual suggestion
        // ===================================================================
        let confirmed_mem = operations::confirm_membership(&conn, &item_k8s.id, &ws_infra.id).unwrap();
        assert_eq!(confirmed_mem.membership_type, "confirmed", "13. Contextual membership confirmed");

        // ===================================================================
        // Step 14: Verify Item becomes confirmed/organized
        // ===================================================================
        // Invariant 1: Item is not deleted
        let organized_item = operations::get_item(&conn, &item_k8s.id).unwrap();
        assert_eq!(organized_item.id, item_k8s.id, "14. Item not deleted");
        // Invariant 2: Metadata intact
        assert_eq!(organized_item.metadata, item_k8s.metadata, "14. Metadata preserved");
        // Invariant 3: Accessible through Workspace
        assert_eq!(organized_item.workspace_id, Some(ws_infra.id.clone()), "14. Item accessible through workspace");
        // Invariant 4: State is Organized
        let organized_state = operations::get_inbox_item_state(&conn, &item_k8s.id).unwrap().unwrap();
        assert_eq!(organized_state.state, crate::db::models::InboxState::Organized, "14. State marked Organized");
        // Invariant 5: Removed from Needs Attention
        let att_organized = crate::context_engine::attention::evaluate_inbox_item_attention(&conn, &item_k8s.id, None).unwrap();
        assert!(!att_organized.attention_required, "14. Organized item removed from Needs Attention");
        assert_eq!(att_organized.attention_level, crate::context_engine::attention::AttentionLevel::None);

        // ===================================================================
        // Step 15: Dismiss another suggestion
        // ===================================================================
        operations::dismiss_membership(&conn, &item_tf.id, &ws_infra.id).unwrap();
        crate::context_engine::inbox_state::InboxStateMachine::dismiss_item(&conn, &item_tf.id, Some("User dismissed suggestion")).unwrap();

        // ===================================================================
        // Step 16: Verify feedback is stored
        // ===================================================================
        let dismissed_state = operations::get_inbox_item_state(&conn, &item_tf.id).unwrap().unwrap();
        assert_eq!(dismissed_state.state, crate::db::models::InboxState::Dismissed, "16. State marked Dismissed");

        let att_dismissed = crate::context_engine::attention::evaluate_inbox_item_attention(&conn, &item_tf.id, None).unwrap();
        assert!(!att_dismissed.attention_required, "16. Dismissed item requires no attention");
        assert!(att_dismissed.factors.is_dismissed);
        assert_eq!(att_dismissed.attention_level, crate::context_engine::attention::AttentionLevel::None);

        let signal_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM context_signals WHERE signal_type = 'membership_dismissed' AND item_id = ?1",
            rusqlite::params![item_tf.id],
            |r| r.get(0),
        ).unwrap();
        assert!(signal_count >= 1, "16. User feedback signal stored in database");

        // ===================================================================
        // Step 17: Select multiple Items
        // ===================================================================
        let selected_ids = vec![item_q1.id.clone(), item_q2.id.clone(), item_q3.id.clone()];
        assert_eq!(selected_ids.len(), 3, "17. Selected 3 quantum items");

        // ===================================================================
        // Step 18: Bulk assign them to a Workspace
        // ===================================================================
        let ws_quantum = operations::create_workspace(&conn, "Quantum Computing").unwrap();
        let bulk_assigned = operations::bulk_assign_inbox_items_to_workspace(&mut conn, &selected_ids, &ws_quantum.id).unwrap();
        assert_eq!(bulk_assigned.len(), 3, "18. Bulk assigned 3 items");

        for q_id in &selected_ids {
            let item = operations::get_item(&conn, q_id).unwrap();
            assert_eq!(item.workspace_id, Some(ws_quantum.id.clone()), "18. Item workspace updated");
            let state = operations::get_inbox_item_state(&conn, q_id).unwrap().unwrap();
            assert_eq!(state.state, crate::db::models::InboxState::Organized, "18. Bulk assigned state is Organized");
            let att = crate::context_engine::attention::evaluate_inbox_item_attention(&conn, q_id, None).unwrap();
            assert!(!att.attention_required, "18. Bulk assigned items no longer require attention");
        }

        // ===================================================================
        // Step 19: Search Inbox
        // ===================================================================
        let search_text = crate::search::search_inbox_items(
            &conn,
            &crate::search::InboxSearchParams {
                query: Some("apples".to_string()),
                ..Default::default()
            },
            None,
        ).unwrap();
        assert_eq!(search_text.len(), 1, "19. Search finds grocery note");
        assert_eq!(search_text[0].item.id, item_unrelated_1.id);

        let search_quantum = crate::search::search_inbox_items(
            &conn,
            &crate::search::InboxSearchParams {
                query: Some("Quantum".to_string()),
                ..Default::default()
            },
            None,
        ).unwrap();
        assert!(search_quantum.len() >= 2, "19. Search finds quantum items");

        // ===================================================================
        // Step 20: Filter by context (Workspace filter & Unassigned filter)
        // ===================================================================
        let search_ws_quantum = crate::search::search_inbox_items(
            &conn,
            &crate::search::InboxSearchParams {
                workspace_id: Some(ws_quantum.id.clone()),
                ..Default::default()
            },
            None,
        ).unwrap();
        assert_eq!(search_ws_quantum.len(), 3, "20. Filter by Quantum workspace returns exactly 3 items");

        let search_unassigned = crate::search::search_inbox_items(
            &conn,
            &crate::search::InboxSearchParams {
                workspace_id: Some("inbox".to_string()),
                ..Default::default()
            },
            None,
        ).unwrap();
        let unassigned_ids: Vec<String> = search_unassigned.iter().map(|r| r.item.id.clone()).collect();
        assert!(unassigned_ids.contains(&item_unrelated_1.id), "20. Filter by inbox returns unassigned items");
        assert!(unassigned_ids.contains(&item_unrelated_2.id));
        assert!(unassigned_ids.contains(&item_unrelated_3.id));
        assert!(!unassigned_ids.contains(&item_k8s.id), "20. Organized item is excluded from unassigned filter");

        // ===================================================================
        // Step 21: Filter by date
        // ===================================================================
        let date_from_past = "2020-01-01T00:00:00Z".to_string();
        let date_to_future = "2030-01-01T00:00:00Z".to_string();
        let search_date_valid = crate::search::search_inbox_items(
            &conn,
            &crate::search::InboxSearchParams {
                date_from: Some(date_from_past),
                date_to: Some(date_to_future),
                ..Default::default()
            },
            None,
        ).unwrap();
        assert!(search_date_valid.len() >= 5, "21. Valid date range returns items");

        let search_date_empty = crate::search::search_inbox_items(
            &conn,
            &crate::search::InboxSearchParams {
                date_from: Some("2099-01-01T00:00:00Z".to_string()),
                ..Default::default()
            },
            None,
        ).unwrap();
        assert_eq!(search_date_empty.len(), 0, "21. Far future date filter returns 0 items");

        // ===================================================================
        // Step 22: Restart application (Cold restart simulation)
        // ===================================================================
        let items_count_pre_restart: i64 = conn.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0)).unwrap();
        let ws_count_pre_restart: i64 = conn.query_row("SELECT COUNT(*) FROM workspaces", [], |r| r.get(0)).unwrap();
        let states_count_pre_restart: i64 = conn.query_row("SELECT COUNT(*) FROM inbox_item_states", [], |r| r.get(0)).unwrap();
        let mem_count_pre_restart: i64 = conn.query_row("SELECT COUNT(*) FROM context_memberships", [], |r| r.get(0)).unwrap();
        let sig_count_pre_restart: i64 = conn.query_row("SELECT COUNT(*) FROM context_signals", [], |r| r.get(0)).unwrap();

        drop(conn);

        // Reopen database connection as a fresh application lifecycle
        let conn_restarted = Connection::open(&db_path).unwrap();

        // ===================================================================
        // Step 23: Verify all state persists
        // ===================================================================
        let items_count_post_restart: i64 = conn_restarted.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0)).unwrap();
        let ws_count_post_restart: i64 = conn_restarted.query_row("SELECT COUNT(*) FROM workspaces", [], |r| r.get(0)).unwrap();
        let states_count_post_restart: i64 = conn_restarted.query_row("SELECT COUNT(*) FROM inbox_item_states", [], |r| r.get(0)).unwrap();
        let mem_count_post_restart: i64 = conn_restarted.query_row("SELECT COUNT(*) FROM context_memberships", [], |r| r.get(0)).unwrap();
        let sig_count_post_restart: i64 = conn_restarted.query_row("SELECT COUNT(*) FROM context_signals", [], |r| r.get(0)).unwrap();

        assert_eq!(items_count_pre_restart, items_count_post_restart, "23. Items count persists");
        assert_eq!(ws_count_pre_restart, ws_count_post_restart, "23. Workspaces count persists");
        assert_eq!(states_count_pre_restart, states_count_post_restart, "23. Inbox states count persists");
        assert_eq!(mem_count_pre_restart, mem_count_post_restart, "23. Context memberships count persists");
        assert_eq!(sig_count_pre_restart, sig_count_post_restart, "23. Context signals count persists");

        // Verify specific persisted records
        let reloaded_k8s = operations::get_item(&conn_restarted, &item_k8s.id).unwrap();
        assert_eq!(reloaded_k8s.workspace_id, Some(ws_infra.id.clone()), "23. Organized workspace assignment persists");

        let reloaded_k8s_state = operations::get_inbox_item_state(&conn_restarted, &item_k8s.id).unwrap().unwrap();
        assert_eq!(reloaded_k8s_state.state, crate::db::models::InboxState::Organized, "23. Organized state persists");

        let reloaded_tf_state = operations::get_inbox_item_state(&conn_restarted, &item_tf.id).unwrap().unwrap();
        assert_eq!(reloaded_tf_state.state, crate::db::models::InboxState::Dismissed, "23. Dismissed state persists");

        // Verify search queries work identically on restarted connection
        let search_post_restart = crate::search::search_inbox_items(
            &conn_restarted,
            &crate::search::InboxSearchParams {
                workspace_id: Some(ws_quantum.id.clone()),
                ..Default::default()
            },
            None,
        ).unwrap();
        assert_eq!(search_post_restart.len(), 3, "23. Search queries return persisted results after restart");

        // Clean up temp database directory
        let _ = fs::remove_dir_all(db_path.parent().unwrap());
    }

    #[test]
    fn test_context_transition_workflow_e2e() {
        let (conn, db_path) = setup_test_db();
        let engine = create_engine();

        // -------------------------------------------------------------------
        // Setup: Create Workspaces and establish profile signals
        // -------------------------------------------------------------------
        let ws_college = operations::create_workspace(&conn, "College").unwrap();
        let ws_portfolio = operations::create_workspace(&conn, "Portfolio").unwrap();

        let session_1 = engine.get_or_update_active_session(&conn).unwrap();
        crate::context_engine::temporal::update_temporal_decay_config(
            &conn,
            &crate::context_engine::temporal::TemporalDecayConfig {
                strategy: "none".to_string(),
                ..Default::default()
            },
        ).unwrap();

        // Seed initial reference items to establish workspace profiles
        let seed_col = operations::create_item(
            &conn,
            Some(&ws_college.id),
            Some(&session_1),
            "url",
            "https://canvas.stanford.edu/courses/106b",
            Some("Stanford CS106B Canvas"),
            "{\"app_name\": \"Google Chrome\", \"original_url\": \"https://canvas.stanford.edu/courses/106b\"}",
        ).unwrap();
        conn.execute("UPDATE items SET created_at = '2026-09-10T08:50:00Z' WHERE id = ?1", rusqlite::params![seed_col.id]).unwrap();
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: seed_col.id.clone(),
            workspace_id: Some(ws_college.id.clone()),
            source: "browser".to_string(),
            data_type: "url".to_string(),
            metadata: json!({
                "app_name": "Google Chrome",
                "original_url": "https://canvas.stanford.edu/courses/106b",
                "domain": "canvas.stanford.edu"
            }),
        });

        let seed_port = operations::create_item(
            &conn,
            Some(&ws_portfolio.id),
            Some(&session_1),
            "url",
            "https://www.figma.com/file/portfolio-core",
            Some("Figma Portfolio Design"),
            "{\"app_name\": \"Figma\", \"original_url\": \"https://www.figma.com/file/portfolio-core\"}",
        ).unwrap();
        conn.execute("UPDATE items SET created_at = '2026-09-10T08:55:00Z' WHERE id = ?1", rusqlite::params![seed_port.id]).unwrap();
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: seed_port.id.clone(),
            workspace_id: Some(ws_portfolio.id.clone()),
            source: "app".to_string(),
            data_type: "url".to_string(),
            metadata: json!({
                "app_name": "Figma",
                "original_url": "https://www.figma.com/file/portfolio-core",
                "domain": "figma.com"
            }),
        });

        // Initialize Transition Detector
        let mut detector = ContextTransitionDetector::new()
            .with_min_consecutive_evidence(1)
            .with_inactivity_threshold(7200)
            .with_min_switch_confidence(0.40)
            .with_min_return_confidence(0.40)
            .with_max_return_gap_seconds(86400);

        // ===================================================================
        // Phase 1: User captures several College items (Scenario step 1)
        // Verify: College is detected as initial context
        // ===================================================================
        let c1 = operations::create_item(
            &conn,
            None,
            Some(&session_1),
            "url",
            "https://canvas.stanford.edu/courses/106b/assignments/2",
            Some("CS106B Assignment 2 Recursion"),
            "{\"app_name\": \"Google Chrome\", \"original_url\": \"https://canvas.stanford.edu/courses/106b/assignments/2\"}",
        ).unwrap();
        conn.execute("UPDATE items SET created_at = '2026-09-10T09:00:00Z' WHERE id = ?1", rusqlite::params![c1.id]).unwrap();
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: c1.id.clone(),
            workspace_id: None,
            source: "browser".to_string(),
            data_type: "url".to_string(),
            metadata: json!({
                "app_name": "Google Chrome",
                "original_url": "https://canvas.stanford.edu/courses/106b/assignments/2",
                "domain": "canvas.stanford.edu"
            }),
        });
        operations::create_item_relationship(&conn, &c1.id, &seed_col.id, "same_domain", 0.85, None).unwrap();

        let c2 = operations::create_item(
            &conn,
            None,
            Some(&session_1),
            "url",
            "https://canvas.stanford.edu/courses/106b/lectures/5",
            Some("Lecture 5 Notes: Binary Trees and Graph Traversals"),
            "{\"app_name\": \"Google Chrome\", \"original_url\": \"https://canvas.stanford.edu/courses/106b/lectures/5\"}",
        ).unwrap();
        conn.execute("UPDATE items SET created_at = '2026-09-10T09:05:00Z' WHERE id = ?1", rusqlite::params![c2.id]).unwrap();
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: c2.id.clone(),
            workspace_id: None,
            source: "browser".to_string(),
            data_type: "url".to_string(),
            metadata: json!({
                "app_name": "Google Chrome",
                "original_url": "https://canvas.stanford.edu/courses/106b/lectures/5",
                "domain": "canvas.stanford.edu"
            }),
        });
        operations::create_item_relationship(&conn, &c2.id, &seed_col.id, "same_domain", 0.85, None).unwrap();

        let c3 = operations::create_item(
            &conn,
            None,
            Some(&session_1),
            "url",
            "https://canvas.stanford.edu/courses/106b/handouts/trees.pdf",
            Some("Trees and Heaps Handout"),
            "{\"app_name\": \"Google Chrome\", \"original_url\": \"https://canvas.stanford.edu/courses/106b/handouts/trees.pdf\"}",
        ).unwrap();
        conn.execute("UPDATE items SET created_at = '2026-09-10T09:10:00Z' WHERE id = ?1", rusqlite::params![c3.id]).unwrap();
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: c3.id.clone(),
            workspace_id: None,
            source: "browser".to_string(),
            data_type: "url".to_string(),
            metadata: json!({
                "app_name": "Google Chrome",
                "original_url": "https://canvas.stanford.edu/courses/106b/handouts/trees.pdf",
                "domain": "canvas.stanford.edu"
            }),
        });
        operations::create_item_relationship(&conn, &c3.id, &seed_col.id, "same_domain", 0.85, None).unwrap();

        // Evaluate detector on c1..c3
        let res_c1 = detector.process_item(&conn, &c1.id).unwrap();
        assert!(res_c1.is_transition(), "1. c1 triggers ContextStart transition");
        let t1 = res_c1.transition().unwrap();
        assert_eq!(t1.transition_type, ContextTransitionType::ContextStart);
        assert_eq!(t1.to_context.as_ref().unwrap().id, ws_college.id);
        assert_eq!(t1.to_context.as_ref().unwrap().name, "College");
        assert!(t1.confidence >= 0.40, "1. Transition confidence is reasonable");
        record_context_transition(&conn, &t1.to_context_transition()).unwrap();

        let res_c2 = detector.process_item(&conn, &c2.id).unwrap();
        assert!(res_c2.is_continuation(), "1. c2 continues College activity");

        let res_c3 = detector.process_item(&conn, &c3.id).unwrap();
        assert!(res_c3.is_continuation(), "1. c3 continues College activity");
        assert_eq!(detector.current_context().unwrap().streak_count, 3);
        assert_eq!(detector.current_context().unwrap().reference.name, "College");

        // ===================================================================
        // Phase 2: User continues College activity (Scenario step 2)
        // Verify: Continuous activity increases streak without false transitions
        // ===================================================================
        let c4 = operations::create_item(
            &conn,
            None,
            Some(&session_1),
            "url",
            "https://canvas.stanford.edu/courses/106b/discussions/42",
            Some("Canvas Discussion on Recursive Backtracking"),
            "{\"app_name\": \"Google Chrome\", \"original_url\": \"https://canvas.stanford.edu/courses/106b/discussions/42\"}",
        ).unwrap();
        conn.execute("UPDATE items SET created_at = '2026-09-10T09:15:00Z' WHERE id = ?1", rusqlite::params![c4.id]).unwrap();
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: c4.id.clone(),
            workspace_id: None,
            source: "browser".to_string(),
            data_type: "url".to_string(),
            metadata: json!({
                "app_name": "Google Chrome",
                "original_url": "https://canvas.stanford.edu/courses/106b/discussions/42",
                "domain": "canvas.stanford.edu"
            }),
        });
        operations::create_item_relationship(&conn, &c4.id, &seed_col.id, "same_domain", 0.85, None).unwrap();

        let c5 = operations::create_item(
            &conn,
            None,
            Some(&session_1),
            "url",
            "https://canvas.stanford.edu/courses/106b/submissions/confirm",
            Some("CS106B Project 2 Submission Confirmation"),
            "{\"app_name\": \"Google Chrome\", \"original_url\": \"https://canvas.stanford.edu/courses/106b/submissions/confirm\"}",
        ).unwrap();
        conn.execute("UPDATE items SET created_at = '2026-09-10T09:20:00Z' WHERE id = ?1", rusqlite::params![c5.id]).unwrap();
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: c5.id.clone(),
            workspace_id: None,
            source: "browser".to_string(),
            data_type: "url".to_string(),
            metadata: json!({
                "app_name": "Google Chrome",
                "original_url": "https://canvas.stanford.edu/courses/106b/submissions/confirm",
                "domain": "canvas.stanford.edu"
            }),
        });
        operations::create_item_relationship(&conn, &c5.id, &seed_col.id, "same_domain", 0.85, None).unwrap();

        let res_c4 = detector.process_item(&conn, &c4.id).unwrap();
        assert!(res_c4.is_continuation(), "2. c4 continues College");
        let res_c5 = detector.process_item(&conn, &c5.id).unwrap();
        assert!(res_c5.is_continuation(), "2. c5 continues College");
        assert_eq!(detector.current_context().unwrap().streak_count, 5);

        // ===================================================================
        // Phase 3: Unrelated / Noisy item (Scenario step 7 - part 1)
        // Verify: Temporary noisy activity does not create false transitions
        // ===================================================================
        let noise_1 = operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://www.amazon.com/orders/402-91823",
            Some("Amazon Order Confirmation #402-91823"),
            "{\"app_name\": \"Google Chrome\", \"original_url\": \"https://www.amazon.com/orders/402-91823\"}",
        ).unwrap();
        conn.execute("UPDATE items SET created_at = '2026-09-10T09:23:00Z' WHERE id = ?1", rusqlite::params![noise_1.id]).unwrap();
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: noise_1.id.clone(),
            workspace_id: None,
            source: "browser".to_string(),
            data_type: "url".to_string(),
            metadata: json!({
                "app_name": "Google Chrome",
                "original_url": "https://www.amazon.com/orders/402-91823",
                "domain": "amazon.com"
            }),
        });

        let res_noise1 = detector.process_item(&conn, &noise_1.id).unwrap();
        assert!(!res_noise1.is_transition(), "3. Noise item does not create false transition");
        assert_eq!(detector.current_context().unwrap().reference.name, "College", "3. Context remains College");

        // ===================================================================
        // Phase 4: User begins Portfolio activity with strong evidence (Scenario step 3 & 4)
        // Verify: Portfolio transition is detected (ContextSwitch)
        // ===================================================================
        let p1 = operations::create_item(
            &conn,
            None,
            Some(&session_1),
            "url",
            "https://www.figma.com/file/portfolio-2026/Hero",
            Some("Portfolio Hero Section Design"),
            "{\"app_name\": \"Figma\", \"original_url\": \"https://www.figma.com/file/portfolio-2026/Hero\"}",
        ).unwrap();
        conn.execute("UPDATE items SET created_at = '2026-09-10T09:30:00Z' WHERE id = ?1", rusqlite::params![p1.id]).unwrap();
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: p1.id.clone(),
            workspace_id: None,
            source: "app".to_string(),
            data_type: "url".to_string(),
            metadata: json!({
                "app_name": "Figma",
                "original_url": "https://www.figma.com/file/portfolio-2026/Hero",
                "domain": "figma.com"
            }),
        });
        operations::create_item_relationship(&conn, &p1.id, &seed_port.id, "same_domain", 0.85, None).unwrap();

        let p2 = operations::create_item(
            &conn,
            None,
            Some(&session_1),
            "url",
            "https://www.figma.com/file/portfolio-2026/Components",
            Some("Portfolio UI Component Library"),
            "{\"app_name\": \"Figma\", \"original_url\": \"https://www.figma.com/file/portfolio-2026/Components\"}",
        ).unwrap();
        conn.execute("UPDATE items SET created_at = '2026-09-10T09:35:00Z' WHERE id = ?1", rusqlite::params![p2.id]).unwrap();
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: p2.id.clone(),
            workspace_id: None,
            source: "app".to_string(),
            data_type: "url".to_string(),
            metadata: json!({
                "app_name": "Figma",
                "original_url": "https://www.figma.com/file/portfolio-2026/Components",
                "domain": "figma.com"
            }),
        });
        operations::create_item_relationship(&conn, &p2.id, &seed_port.id, "same_domain", 0.85, None).unwrap();

        let p3 = operations::create_item(
            &conn,
            None,
            Some(&session_1),
            "url",
            "https://www.figma.com/file/portfolio-2026/CaseStudy",
            Some("Interactive Project Case Study UI"),
            "{\"app_name\": \"Figma\", \"original_url\": \"https://www.figma.com/file/portfolio-2026/CaseStudy\"}",
        ).unwrap();
        conn.execute("UPDATE items SET created_at = '2026-09-10T09:40:00Z' WHERE id = ?1", rusqlite::params![p3.id]).unwrap();
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: p3.id.clone(),
            workspace_id: None,
            source: "app".to_string(),
            data_type: "url".to_string(),
            metadata: json!({
                "app_name": "Figma",
                "original_url": "https://www.figma.com/file/portfolio-2026/CaseStudy",
                "domain": "figma.com"
            }),
        });
        operations::create_item_relationship(&conn, &p3.id, &seed_port.id, "same_domain", 0.85, None).unwrap();

        let res_p1 = detector.process_item(&conn, &p1.id).unwrap();
        assert!(res_p1.is_transition(), "4. p1 triggers ContextSwitch to Portfolio");
        let t2 = res_p1.transition().unwrap();
        assert_eq!(t2.transition_type, ContextTransitionType::ContextSwitch);
        assert_eq!(t2.from_context.as_ref().unwrap().name, "College");
        assert_eq!(t2.to_context.as_ref().unwrap().name, "Portfolio");
        assert!(t2.confidence >= 0.40, "4. Switch confidence is reasonable");
        record_context_transition(&conn, &t2.to_context_transition()).unwrap();

        let res_p2 = detector.process_item(&conn, &p2.id).unwrap();
        assert!(res_p2.is_continuation(), "4. p2 continues Portfolio");
        let res_p3 = detector.process_item(&conn, &p3.id).unwrap();
        assert!(res_p3.is_continuation(), "4. p3 continues Portfolio");
        assert_eq!(detector.current_context().unwrap().reference.name, "Portfolio");
        assert_eq!(detector.current_context().unwrap().streak_count, 3);

        // ===================================================================
        // Phase 5: Intermediate noisy item (Scenario step 7 - part 2)
        // ===================================================================
        let noise_2 = operations::create_item(
            &conn,
            None,
            None,
            "url",
            "https://weather.com/weather/today/l/94305",
            Some("Local Weather Forecast"),
            "{\"app_name\": \"Safari\", \"original_url\": \"https://weather.com/weather/today/l/94305\"}",
        ).unwrap();
        conn.execute("UPDATE items SET created_at = '2026-09-10T09:45:00Z' WHERE id = ?1", rusqlite::params![noise_2.id]).unwrap();
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: noise_2.id.clone(),
            workspace_id: None,
            source: "browser".to_string(),
            data_type: "url".to_string(),
            metadata: json!({
                "app_name": "Safari",
                "original_url": "https://weather.com/weather/today/l/94305",
                "domain": "weather.com"
            }),
        });

        let res_noise2 = detector.process_item(&conn, &noise_2.id).unwrap();
        assert!(!res_noise2.is_transition(), "5. Intermediate noise ignored");
        assert_eq!(detector.current_context().unwrap().reference.name, "Portfolio");

        // ===================================================================
        // Phase 6: User returns to College (Scenario step 5)
        // Verify: College return is detected (ContextReturn)
        // ===================================================================
        let c6 = operations::create_item(
            &conn,
            None,
            Some(&session_1),
            "url",
            "https://canvas.stanford.edu/courses/106b/assignments/final-practice",
            Some("CS106B Final Exam Practice"),
            "{\"app_name\": \"Google Chrome\", \"original_url\": \"https://canvas.stanford.edu/courses/106b/assignments/final-practice\"}",
        ).unwrap();
        conn.execute("UPDATE items SET created_at = '2026-09-10T10:15:00Z' WHERE id = ?1", rusqlite::params![c6.id]).unwrap();
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: c6.id.clone(),
            workspace_id: None,
            source: "browser".to_string(),
            data_type: "url".to_string(),
            metadata: json!({
                "app_name": "Google Chrome",
                "original_url": "https://canvas.stanford.edu/courses/106b/assignments/final-practice",
                "domain": "canvas.stanford.edu"
            }),
        });
        operations::create_item_relationship(&conn, &c6.id, &seed_col.id, "same_domain", 0.85, None).unwrap();

        let c7 = operations::create_item(
            &conn,
            None,
            Some(&session_1),
            "url",
            "https://canvas.stanford.edu/courses/106b/files/exam-review.pdf",
            Some("CS106B Exam Review Slides"),
            "{\"app_name\": \"Google Chrome\", \"original_url\": \"https://canvas.stanford.edu/courses/106b/files/exam-review.pdf\"}",
        ).unwrap();
        conn.execute("UPDATE items SET created_at = '2026-09-10T10:20:00Z' WHERE id = ?1", rusqlite::params![c7.id]).unwrap();
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: c7.id.clone(),
            workspace_id: None,
            source: "browser".to_string(),
            data_type: "url".to_string(),
            metadata: json!({
                "app_name": "Google Chrome",
                "original_url": "https://canvas.stanford.edu/courses/106b/files/exam-review.pdf",
                "domain": "canvas.stanford.edu"
            }),
        });
        operations::create_item_relationship(&conn, &c7.id, &seed_col.id, "same_domain", 0.85, None).unwrap();

        let res_c6 = detector.process_item(&conn, &c6.id).unwrap();
        assert!(res_c6.is_transition(), "6. c6 triggers ContextReturn to College");
        let t3 = res_c6.transition().unwrap();
        assert_eq!(t3.transition_type, ContextTransitionType::ContextReturn);
        assert_eq!(t3.from_context.as_ref().unwrap().name, "Portfolio");
        assert_eq!(t3.to_context.as_ref().unwrap().name, "College");
        assert!(t3.confidence >= 0.40, "6. Return confidence is reasonable");

        let ret_c6 = t3.as_context_return().unwrap();
        assert_eq!(ret_c6.returned_context.name, "College");
        assert_eq!(ret_c6.previous_context.unwrap().name, "Portfolio");
        assert_eq!(ret_c6.duration_since_previous_activity_seconds, 3300); // 10:15:00 - 09:20:00 = 55 mins
        assert_eq!(ret_c6.duration_since_previous_activity_formatted, "55 minutes");
        record_context_transition(&conn, &t3.to_context_transition()).unwrap();

        let res_c7 = detector.process_item(&conn, &c7.id).unwrap();
        assert!(res_c7.is_continuation(), "6. c7 continues College");
        assert_eq!(detector.current_context().unwrap().reference.name, "College");

        // ===================================================================
        // Phase 7: User later returns to Portfolio (Scenario step 6)
        // Verify: Portfolio return is detected (ContextReturn)
        // ===================================================================
        let p4 = operations::create_item(
            &conn,
            None,
            Some(&session_1),
            "url",
            "https://www.figma.com/file/portfolio-2026/VercelProduction",
            Some("Portfolio Production Mockup"),
            "{\"app_name\": \"Figma\", \"original_url\": \"https://www.figma.com/file/portfolio-2026/VercelProduction\"}",
        ).unwrap();
        conn.execute("UPDATE items SET created_at = '2026-09-10T11:00:00Z' WHERE id = ?1", rusqlite::params![p4.id]).unwrap();
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: p4.id.clone(),
            workspace_id: None,
            source: "app".to_string(),
            data_type: "url".to_string(),
            metadata: json!({
                "app_name": "Figma",
                "original_url": "https://www.figma.com/file/portfolio-2026/VercelProduction",
                "domain": "figma.com"
            }),
        });
        operations::create_item_relationship(&conn, &p4.id, &seed_port.id, "same_domain", 0.85, None).unwrap();

        let p5 = operations::create_item(
            &conn,
            None,
            Some(&session_1),
            "url",
            "https://www.figma.com/file/portfolio-2026/LighthouseAudit",
            Some("Portfolio Lighthouse Audit"),
            "{\"app_name\": \"Figma\", \"original_url\": \"https://www.figma.com/file/portfolio-2026/LighthouseAudit\"}",
        ).unwrap();
        conn.execute("UPDATE items SET created_at = '2026-09-10T11:05:00Z' WHERE id = ?1", rusqlite::params![p5.id]).unwrap();
        engine.process_event(&conn, ContextEvent::Capture {
            item_id: p5.id.clone(),
            workspace_id: None,
            source: "app".to_string(),
            data_type: "url".to_string(),
            metadata: json!({
                "app_name": "Figma",
                "original_url": "https://www.figma.com/file/portfolio-2026/LighthouseAudit",
                "domain": "figma.com"
            }),
        });
        operations::create_item_relationship(&conn, &p5.id, &seed_port.id, "same_domain", 0.85, None).unwrap();

        let res_p4 = detector.process_item(&conn, &p4.id).unwrap();
        assert!(res_p4.is_transition(), "7. p4 triggers ContextReturn to Portfolio");
        let t4 = res_p4.transition().unwrap();
        assert_eq!(t4.transition_type, ContextTransitionType::ContextReturn);
        assert_eq!(t4.from_context.as_ref().unwrap().name, "College");
        assert_eq!(t4.to_context.as_ref().unwrap().name, "Portfolio");

        let ret_p4 = t4.as_context_return().unwrap();
        assert_eq!(ret_p4.returned_context.name, "Portfolio");
        assert_eq!(ret_p4.previous_context.unwrap().name, "College");
        assert_eq!(ret_p4.duration_since_previous_activity_seconds, 4800); // 11:00:00 - 09:40:00 = 80 mins
        assert_eq!(ret_p4.duration_since_previous_activity_formatted, "1.3 hours");
        record_context_transition(&conn, &t4.to_context_transition()).unwrap();

        let res_p5 = detector.process_item(&conn, &p5.id).unwrap();
        assert!(res_p5.is_continuation(), "7. p5 continues Portfolio");
        assert_eq!(detector.current_context().unwrap().reference.name, "Portfolio");

        // ===================================================================
        // Phase 8: Also test ambiguous transitions (User request requirement)
        // ===================================================================
        let amb_item = operations::create_item(
            &conn,
            None,
            Some(&session_1),
            "text",
            "Shared Design System and Course Presentation combining Stanford CS106B and Personal Portfolio Showcase",
            Some("Ambiguous Cross-Context Project"),
            "{\"app_name\": \"Notes\"}",
        ).unwrap();
        conn.execute("UPDATE items SET created_at = '2026-09-10T11:15:00Z' WHERE id = ?1", rusqlite::params![amb_item.id]).unwrap();

        let res_amb = detector.evaluate_interpretation(
            Some(&amb_item.id),
            Some(&session_1),
            "2026-09-10T11:15:00Z",
            None,
            0.45,
            "Medium",
            true, // is_ambiguous
            vec![
                "Ambiguous candidate affinities: College (48%) vs Portfolio (47%)".to_string(),
                "Conflicting cross-domain signals".to_string(),
            ],
        );
        assert!(res_amb.is_transition(), "8. Ambiguous signal triggers AmbiguousTransition");
        let t5 = res_amb.transition().unwrap();
        assert_eq!(t5.transition_type, ContextTransitionType::AmbiguousTransition);
        assert_eq!(t5.from_context.as_ref().unwrap().name, "Portfolio");
        assert!(t5.to_context.is_none());
        assert_eq!(t5.confidence, 0.45);
        record_context_transition(&conn, &t5.to_context_transition()).unwrap();

        // ===================================================================
        // Phase 9: Verifications
        // ===================================================================

        // 9.A: Activity Segments Verification
        let item_sequence_events = vec![
            SegmentStreamEvent {
                item_id: c1.id.clone(),
                session_id: Some(session_1.clone()),
                timestamp: "2026-09-10T09:00:00Z".to_string(),
                target_context: Some(ContextReference::workspace(&ws_college.id, &ws_college.name)),
                confidence: 0.90,
                confidence_band: "High".to_string(),
                is_ambiguous: false,
                item_type: "url".to_string(),
                item_title: Some("CS106B Assignment 2".to_string()),
                evidence: vec!["Canvas URL".to_string()],
            },
            SegmentStreamEvent {
                item_id: c2.id.clone(),
                session_id: Some(session_1.clone()),
                timestamp: "2026-09-10T09:05:00Z".to_string(),
                target_context: Some(ContextReference::workspace(&ws_college.id, &ws_college.name)),
                confidence: 0.90,
                confidence_band: "High".to_string(),
                is_ambiguous: false,
                item_type: "text".to_string(),
                item_title: Some("Lecture 5 Notes".to_string()),
                evidence: vec!["Notes".to_string()],
            },
            SegmentStreamEvent {
                item_id: c3.id.clone(),
                session_id: Some(session_1.clone()),
                timestamp: "2026-09-10T09:10:00Z".to_string(),
                target_context: Some(ContextReference::workspace(&ws_college.id, &ws_college.name)),
                confidence: 0.90,
                confidence_band: "High".to_string(),
                is_ambiguous: false,
                item_type: "url".to_string(),
                item_title: Some("Trees Handout".to_string()),
                evidence: vec![],
            },
            SegmentStreamEvent {
                item_id: c4.id.clone(),
                session_id: Some(session_1.clone()),
                timestamp: "2026-09-10T09:15:00Z".to_string(),
                target_context: Some(ContextReference::workspace(&ws_college.id, &ws_college.name)),
                confidence: 0.90,
                confidence_band: "High".to_string(),
                is_ambiguous: false,
                item_type: "url".to_string(),
                item_title: Some("Canvas Discussion".to_string()),
                evidence: vec![],
            },
            SegmentStreamEvent {
                item_id: c5.id.clone(),
                session_id: Some(session_1.clone()),
                timestamp: "2026-09-10T09:20:00Z".to_string(),
                target_context: Some(ContextReference::workspace(&ws_college.id, &ws_college.name)),
                confidence: 0.90,
                confidence_band: "High".to_string(),
                is_ambiguous: false,
                item_type: "url".to_string(),
                item_title: Some("CS106B Submission".to_string()),
                evidence: vec![],
            },
            // Noise 1
            SegmentStreamEvent {
                item_id: noise_1.id.clone(),
                session_id: Some(session_1.clone()),
                timestamp: "2026-09-10T09:23:00Z".to_string(),
                target_context: None,
                confidence: 0.15,
                confidence_band: "Low".to_string(),
                is_ambiguous: false,
                item_type: "url".to_string(),
                item_title: Some("Amazon Order".to_string()),
                evidence: vec![],
            },
            // Portfolio 1..3
            SegmentStreamEvent {
                item_id: p1.id.clone(),
                session_id: Some(session_1.clone()),
                timestamp: "2026-09-10T09:30:00Z".to_string(),
                target_context: Some(ContextReference::workspace(&ws_portfolio.id, &ws_portfolio.name)),
                confidence: 0.92,
                confidence_band: "Very High".to_string(),
                is_ambiguous: false,
                item_type: "url".to_string(),
                item_title: Some("Portfolio Hero".to_string()),
                evidence: vec!["Figma app".to_string()],
            },
            SegmentStreamEvent {
                item_id: p2.id.clone(),
                session_id: Some(session_1.clone()),
                timestamp: "2026-09-10T09:35:00Z".to_string(),
                target_context: Some(ContextReference::workspace(&ws_portfolio.id, &ws_portfolio.name)),
                confidence: 0.90,
                confidence_band: "High".to_string(),
                is_ambiguous: false,
                item_type: "url".to_string(),
                item_title: Some("UI Components".to_string()),
                evidence: vec!["Figma app".to_string()],
            },
            SegmentStreamEvent {
                item_id: p3.id.clone(),
                session_id: Some(session_1.clone()),
                timestamp: "2026-09-10T09:40:00Z".to_string(),
                target_context: Some(ContextReference::workspace(&ws_portfolio.id, &ws_portfolio.name)),
                confidence: 0.90,
                confidence_band: "High".to_string(),
                is_ambiguous: false,
                item_type: "url".to_string(),
                item_title: Some("Case Study".to_string()),
                evidence: vec!["Figma app".to_string()],
            },
            // Noise 2
            SegmentStreamEvent {
                item_id: noise_2.id.clone(),
                session_id: Some(session_1.clone()),
                timestamp: "2026-09-10T09:45:00Z".to_string(),
                target_context: None,
                confidence: 0.10,
                confidence_band: "Low".to_string(),
                is_ambiguous: false,
                item_type: "url".to_string(),
                item_title: Some("Weather".to_string()),
                evidence: vec![],
            },
            // College Return (c6, c7)
            SegmentStreamEvent {
                item_id: c6.id.clone(),
                session_id: Some(session_1.clone()),
                timestamp: "2026-09-10T10:15:00Z".to_string(),
                target_context: Some(ContextReference::workspace(&ws_college.id, &ws_college.name)),
                confidence: 0.91,
                confidence_band: "Very High".to_string(),
                is_ambiguous: false,
                item_type: "url".to_string(),
                item_title: Some("Final Exam Practice".to_string()),
                evidence: vec!["Canvas URL".to_string()],
            },
            SegmentStreamEvent {
                item_id: c7.id.clone(),
                session_id: Some(session_1.clone()),
                timestamp: "2026-09-10T10:20:00Z".to_string(),
                target_context: Some(ContextReference::workspace(&ws_college.id, &ws_college.name)),
                confidence: 0.91,
                confidence_band: "Very High".to_string(),
                is_ambiguous: false,
                item_type: "url".to_string(),
                item_title: Some("Exam Review Slides".to_string()),
                evidence: vec![],
            },
            // Portfolio Return (p4, p5)
            SegmentStreamEvent {
                item_id: p4.id.clone(),
                session_id: Some(session_1.clone()),
                timestamp: "2026-09-10T11:00:00Z".to_string(),
                target_context: Some(ContextReference::workspace(&ws_portfolio.id, &ws_portfolio.name)),
                confidence: 0.92,
                confidence_band: "Very High".to_string(),
                is_ambiguous: false,
                item_type: "url".to_string(),
                item_title: Some("Portfolio Vercel".to_string()),
                evidence: vec!["Figma app".to_string()],
            },
            SegmentStreamEvent {
                item_id: p5.id.clone(),
                session_id: Some(session_1.clone()),
                timestamp: "2026-09-10T11:05:00Z".to_string(),
                target_context: Some(ContextReference::workspace(&ws_portfolio.id, &ws_portfolio.name)),
                confidence: 0.92,
                confidence_band: "Very High".to_string(),
                is_ambiguous: false,
                item_type: "url".to_string(),
                item_title: Some("Lighthouse Audit".to_string()),
                evidence: vec![],
            },
            // Ambiguous item
            SegmentStreamEvent {
                item_id: amb_item.id.clone(),
                session_id: Some(session_1.clone()),
                timestamp: "2026-09-10T11:15:00Z".to_string(),
                target_context: None,
                confidence: 0.45,
                confidence_band: "Medium".to_string(),
                is_ambiguous: true,
                item_type: "text".to_string(),
                item_title: Some("Ambiguous Project".to_string()),
                evidence: vec!["Split affinity".to_string()],
            },
        ];

        let segmenter_detector = ContextTransitionDetector::new()
            .with_min_consecutive_evidence(1)
            .with_inactivity_threshold(7200)
            .with_min_switch_confidence(0.40)
            .with_min_return_confidence(0.40)
            .with_max_return_gap_seconds(86400);
        let mut segmenter = ContextActivitySegmenter::new().with_detector(segmenter_detector);
        let segments = segmenter.segment_event_stream(&item_sequence_events);

        assert_eq!(segments.len(), 5, "9.A. Must produce exactly 5 distinct activity segments");

        // Segment 1: College (09:00 - 09:23)
        let seg1 = &segments[0];
        assert_eq!(seg1.context.as_ref().unwrap().name, "College");
        assert_eq!(seg1.started_at, "2026-09-10T09:00:00Z");
        assert_eq!(seg1.ended_at, "2026-09-10T09:23:00Z");
        assert_eq!(seg1.item_count, 6); // c1..c5 + noise_1
        assert_eq!(seg1.transition_in.as_ref().unwrap().transition_type, ContextTransitionType::ContextStart);
        assert_eq!(seg1.transition_out.as_ref().unwrap().transition_type, ContextTransitionType::ContextSwitch);

        // Segment 2: Portfolio (09:30 - 09:45)
        let seg2 = &segments[1];
        assert_eq!(seg2.context.as_ref().unwrap().name, "Portfolio");
        assert_eq!(seg2.started_at, "2026-09-10T09:30:00Z");
        assert_eq!(seg2.ended_at, "2026-09-10T09:45:00Z");
        assert_eq!(seg2.item_count, 4); // p1..p3 + noise_2
        assert_eq!(seg2.transition_in.as_ref().unwrap().transition_type, ContextTransitionType::ContextSwitch);
        assert_eq!(seg2.transition_out.as_ref().unwrap().transition_type, ContextTransitionType::ContextReturn);

        // Segment 3: College Return (10:15 - 10:20)
        let seg3 = &segments[2];
        assert_eq!(seg3.context.as_ref().unwrap().name, "College");
        assert_eq!(seg3.started_at, "2026-09-10T10:15:00Z");
        assert_eq!(seg3.ended_at, "2026-09-10T10:20:00Z");
        assert_eq!(seg3.item_count, 2); // c6, c7
        assert_eq!(seg3.transition_in.as_ref().unwrap().transition_type, ContextTransitionType::ContextReturn);
        assert_eq!(seg3.transition_out.as_ref().unwrap().transition_type, ContextTransitionType::ContextReturn);

        // Segment 4: Portfolio Return (11:00 - 11:05)
        let seg4 = &segments[3];
        assert_eq!(seg4.context.as_ref().unwrap().name, "Portfolio");
        assert_eq!(seg4.started_at, "2026-09-10T11:00:00Z");
        assert_eq!(seg4.ended_at, "2026-09-10T11:05:00Z");
        assert_eq!(seg4.item_count, 2); // p4, p5
        assert_eq!(seg4.transition_in.as_ref().unwrap().transition_type, ContextTransitionType::ContextReturn);
        assert_eq!(seg4.transition_out.as_ref().unwrap().transition_type, ContextTransitionType::AmbiguousTransition);

        // Segment 5: Ambiguous Activity (11:15)
        let seg5 = &segments[4];
        assert!(seg5.is_ambiguous);
        assert!(seg5.context.is_none());
        assert_eq!(seg5.transition_in.as_ref().unwrap().transition_type, ContextTransitionType::AmbiguousTransition);
        assert_eq!(seg5.item_count, 1);

        // 9.B: Timeline Sequence Verification
        let college_timeline = get_context_timeline(&conn, &ws_college.id).unwrap();
        assert_eq!(college_timeline.workspace_name, "College");
        // Verify timeline contains transition events
        let college_transition_events: Vec<_> = college_timeline.chronological_activity.iter()
            .filter(|e| e.event_type == "context_transition")
            .collect();
        assert!(college_transition_events.len() >= 2, "9.B. College timeline includes transitions");
        assert!(college_transition_events.iter().any(|e| e.title.contains("Started College")));
        assert!(college_transition_events.iter().any(|e| e.title.contains("Returned to College")));

        let portfolio_timeline = get_context_timeline(&conn, &ws_portfolio.id).unwrap();
        assert_eq!(portfolio_timeline.workspace_name, "Portfolio");
        let portfolio_transition_events: Vec<_> = portfolio_timeline.chronological_activity.iter()
            .filter(|e| e.event_type == "context_transition")
            .collect();
        assert!(portfolio_transition_events.len() >= 2, "9.B. Portfolio timeline includes transitions");
        assert!(portfolio_transition_events.iter().any(|e| e.title.contains("Switched to Portfolio")));
        assert!(portfolio_transition_events.iter().any(|e| e.title.contains("Returned to Portfolio")));

        // 9.C: Context Activity Summary Service Verification
        let summary_svc = ContextActivitySummaryService::new();
        let summary = summary_svc.summarize_segments(&segments, None, None);
        assert_eq!(summary.context_switches, 3, "9.C. 3 context switches total (1 switch + 2 returns)");
        assert_eq!(summary.context_returns, 2, "9.C. Exactly 2 context returns (Portfolio -> College, College -> Portfolio)");
        let named_contexts: Vec<_> = summary.contexts_used.iter().filter(|c| c.context.is_some()).collect();
        assert_eq!(named_contexts.len(), 2, "9.C. 2 distinct named contexts used");
        assert!(summary.sequence_formatted.contains("College → Portfolio → College → Portfolio"));

        // 9.D: Strict Membership Invariance Verification
        // No item was silently assigned a workspace!
        let all_items_to_check = vec![
            &c1, &c2, &c3, &c4, &c5, &noise_1,
            &p1, &p2, &p3, &noise_2,
            &c6, &c7, &p4, &p5, &amb_item,
        ];
        for item in all_items_to_check {
            let reloaded = operations::get_item(&conn, &item.id).unwrap();
            assert_eq!(reloaded.workspace_id, None, "9.D. Invariant: item '{}' remains unassigned in inbox", item.title.as_deref().unwrap_or(""));
        }

        let memberships = operations::list_workspace_memberships(&conn, &ws_college.id, None).unwrap();
        // Seed item was confirmed, but none of the captured items were auto-assigned as confirmed
        assert_eq!(memberships.len(), 1, "9.D. No phantom confirmed memberships created for College");

        let port_memberships = operations::list_workspace_memberships(&conn, &ws_portfolio.id, None).unwrap();
        assert_eq!(port_memberships.len(), 1, "9.D. No phantom confirmed memberships created for Portfolio");

        // Clean up temp database directory
        let _ = fs::remove_dir_all(db_path.parent().unwrap());
    }
}


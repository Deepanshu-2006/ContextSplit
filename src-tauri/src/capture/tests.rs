#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use std::fs;
    use std::path::PathBuf;

    use crate::capture::core::CaptureCore;
    use crate::capture::models::*;
    use crate::capture::normalizer::CaptureNormalizer;
    use crate::capture::platform::{
        MacOSCaptureAdapter, MockCaptureAdapter, PlatformCaptureAdapter, WindowsCaptureAdapter,
    };
    use crate::capture::validator::CaptureValidator;
    use crate::context_engine::engine::ContextEngine;
    use crate::context_engine::providers::{
        CaptureContextProvider, UrlContextProvider, UserActionContextProvider,
        WorkspaceContextProvider,
    };
    use crate::db::{self, operations, settings::PrivacySettings};

    fn setup_test_db() -> (Connection, PathBuf) {
        let temp_dir =
            std::env::temp_dir().join(format!("contextsplit_capture_test_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("test.db");

        let conn = Connection::open(&db_path).unwrap();
        db::run_migrations(&conn).unwrap();

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

    fn create_test_engine() -> ContextEngine {
        let mut ce = ContextEngine::new();
        ce.register(Box::new(CaptureContextProvider));
        ce.register(Box::new(UserActionContextProvider));
        ce.register(Box::new(WorkspaceContextProvider));
        ce.register(Box::new(UrlContextProvider));
        ce.set_semantic_provider(Box::new(crate::semantic::MockSemanticProvider::new()));
        ce
    }

    // =======================================================================
    // 1. Validation Tests (CaptureValidator)
    // =======================================================================

    #[test]
    fn test_validator_rejects_empty_content() {
        let validator = CaptureValidator::new();

        let empty_payload = RawCapturePayload::new("", CaptureMethod::Manual);
        assert_eq!(validator.validate(&empty_payload), Err(CaptureError::EmptyContent));

        let whitespace_payload = RawCapturePayload::new("   \n\t  ", CaptureMethod::Clipboard);
        assert_eq!(validator.validate(&whitespace_payload), Err(CaptureError::EmptyContent));
    }

    #[test]
    fn test_validator_enforces_payload_size_limits() {
        let validator = CaptureValidator::with_limits(50, 100);

        let valid_payload = RawCapturePayload::new("Hello World", CaptureMethod::Manual);
        assert!(validator.validate(&valid_payload).is_ok());

        let oversized_text = "A".repeat(100);
        let invalid_payload = RawCapturePayload::new(oversized_text, CaptureMethod::Manual);
        assert!(matches!(
            validator.validate(&invalid_payload),
            Err(CaptureError::PayloadTooLarge { .. })
        ));
    }

    #[test]
    fn test_validator_rejects_null_bytes() {
        let validator = CaptureValidator::new();

        let null_content = RawCapturePayload::new("hello\0world", CaptureMethod::Manual);
        assert!(matches!(
            validator.validate(&null_content),
            Err(CaptureError::ProcessingError(_))
        ));

        let mut null_title = RawCapturePayload::new("valid content", CaptureMethod::Manual);
        null_title.title = Some("invalid\0title".to_string());
        assert!(matches!(
            validator.validate(&null_title),
            Err(CaptureError::ProcessingError(_))
        ));
    }

    #[test]
    fn test_validator_url_validation() {
        let validator = CaptureValidator::new();

        assert!(validator.validate_url("https://booking.com/paris-hotel").is_ok());
        assert!(validator.validate_url("http://localhost:3000/app").is_ok());
        assert!(validator.validate_url("file:///Users/john/doc.pdf").is_ok());

        assert!(matches!(
            validator.validate_url("ftp://server.com"),
            Err(CaptureError::InvalidUrl(_))
        ));
        assert!(matches!(
            validator.validate_url("not a url"),
            Err(CaptureError::InvalidUrl(_))
        ));
        assert!(matches!(
            validator.validate_url("https://"),
            Err(CaptureError::InvalidUrl(_))
        ));
    }

    // =======================================================================
    // 2. Normalization Tests (CaptureNormalizer)
    // =======================================================================

    #[test]
    fn test_normalizer_text_normalization() {
        let normalizer = CaptureNormalizer::new();
        let payload = RawCapturePayload::new(
            "   First Line Title\r\nSecond line with content.   \r\n\r\n",
            CaptureMethod::Manual,
        );

        let event = normalizer.normalize(&payload).unwrap();
        assert_eq!(event.item_type, "text");
        assert_eq!(event.title.as_deref(), Some("First Line Title"));
        assert!(!event.content.contains("\r\n"));
        assert!(event.content.contains('\n'));
        assert_eq!(event.mime_type.as_deref(), Some("text/plain"));
    }

    #[test]
    fn test_normalizer_url_normalization_and_domain_extraction() {
        let normalizer = CaptureNormalizer::new();
        let payload = RawCapturePayload::new(
            "https://www.booking.com/hotel/fr/le-marais.html?ref=123",
            CaptureMethod::Clipboard,
        );

        let event = normalizer.normalize(&payload).unwrap();
        assert_eq!(event.item_type, "url");
        assert_eq!(event.domain.as_deref(), Some("booking.com"));
        assert_eq!(event.title.as_deref(), Some("Link from Booking.com"));
        assert_eq!(event.mime_type.as_deref(), Some("text/uri-list"));
        assert!(event.original_url.is_some());
    }

    #[test]
    fn test_normalizer_file_normalization_and_mime_detection() {
        let normalizer = CaptureNormalizer::new();
        let mut payload = RawCapturePayload::new(
            "/Users/deepanshukhatri/Documents/Trip_Itinerary.pdf",
            CaptureMethod::DragAndDrop,
        );
        payload.file_paths = vec!["/Users/deepanshukhatri/Documents/Trip_Itinerary.pdf".to_string()];

        let event = normalizer.normalize(&payload).unwrap();
        assert_eq!(event.item_type, "file");
        assert_eq!(event.filename.as_deref(), Some("Trip_Itinerary.pdf"));
        assert_eq!(event.title.as_deref(), Some("Trip_Itinerary.pdf"));
        assert_eq!(event.mime_type.as_deref(), Some("application/pdf"));
    }

    #[test]
    fn test_normalizer_image_normalization() {
        let normalizer = CaptureNormalizer::new();
        let payload = RawCapturePayload::new(
            "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
            CaptureMethod::Clipboard,
        );

        let event = normalizer.normalize(&payload).unwrap();
        assert_eq!(event.item_type, "image");
        assert_eq!(event.mime_type.as_deref(), Some("image/png"));
        assert_eq!(event.title.as_deref(), Some("Captured Image"));
    }

    // =======================================================================
    // 3. Platform Adapter Tests (macOS & Windows)
    // =======================================================================

    #[test]
    fn test_macos_adapter_clipboard_file_url_conversion() {
        let adapter = MacOSCaptureAdapter::new();
        assert_eq!(adapter.platform_type(), PlatformType::MacOS);

        let input = PlatformClipboardInput {
            text: None,
            html: None,
            rtf: None,
            image_bytes: None,
            image_dimensions: None,
            file_urls: vec!["file:///Users/john/Documents/Project%20Report.pdf".to_string()],
            active_app: Some("Finder".to_string()),
            window_title: Some("Documents".to_string()),
        };

        let raw = adapter.convert_clipboard_input(input).unwrap();
        assert_eq!(raw.platform, PlatformType::MacOS);
        assert_eq!(raw.source_method, CaptureMethod::Clipboard);
        assert_eq!(raw.data_type.as_deref(), Some("file"));
        assert_eq!(raw.content, "/Users/john/Documents/Project Report.pdf");
        assert_eq!(raw.app_name.as_deref(), Some("Finder"));
    }

    #[test]
    fn test_windows_adapter_drag_drop_path_normalization() {
        let adapter = WindowsCaptureAdapter::new();
        assert_eq!(adapter.platform_type(), PlatformType::Windows);

        let input = PlatformDragDropInput {
            paths: vec!["\"C:/Users/John/Documents/Spreadsheet.xlsx\"".to_string()],
            text: None,
            url: None,
            active_app: Some("explorer.exe".to_string()),
            window_title: Some("Finance 2026".to_string()),
            workspace_id: Some("ws_finance_123".to_string()),
        };

        let raw = adapter.convert_drag_drop_input(input).unwrap();
        assert_eq!(raw.platform, PlatformType::Windows);
        assert_eq!(raw.source_method, CaptureMethod::DragAndDrop);
        assert_eq!(raw.data_type.as_deref(), Some("file"));
        assert_eq!(raw.content, "C:\\Users\\John\\Documents\\Spreadsheet.xlsx");
        assert_eq!(raw.workspace_id.as_deref(), Some("ws_finance_123"));
        assert_eq!(raw.app_name.as_deref(), Some("explorer.exe"));
    }

    #[test]
    fn test_mock_adapter_cross_platform_simulation() {
        let context = PlatformWindowContext {
            app_name: "Safari".to_string(),
            window_title: Some("Paris Flights - AirFrance".to_string()),
            process_id: Some(1234),
            bundle_identifier: Some("com.apple.Safari".to_string()),
        };

        let mock_adapter = MockCaptureAdapter::new(PlatformType::MacOS).with_context(context);
        assert_eq!(mock_adapter.platform_type(), PlatformType::MacOS);

        let manual_input = PlatformManualInput {
            content: "https://airfrance.com/flights-paris".to_string(),
            data_type: Some("url".to_string()),
            title: Some("AirFrance Paris Flight".to_string()),
            workspace_id: None,
            active_app: None,
            window_title: None,
        };

        let raw = mock_adapter.convert_manual_input(manual_input).unwrap();
        assert_eq!(raw.app_name.as_deref(), Some("Safari"));
        assert_eq!(raw.window_title.as_deref(), Some("Paris Flights - AirFrance"));
        assert_eq!(raw.source_method, CaptureMethod::Manual);
    }

    // =======================================================================
    // 4. CaptureCore 5-Step Pipeline Integration Tests
    // =======================================================================

    #[test]
    fn test_capture_core_complete_5_step_pipeline() {
        let (conn, db_path) = setup_test_db();
        let engine = create_test_engine();
        let core = CaptureCore::new();

        // Step 0: Create a Paris Trip workspace to verify relationship mapping
        let ws_paris = operations::create_workspace(&conn, "Paris Trip").unwrap();
        let paris_hotel = operations::create_item(
            &conn,
            Some(&ws_paris.id),
            None,
            "url",
            "https://booking.com/hotel-paris-louvre",
            Some("Paris Louvre Hotel"),
            r#"{"app_name":"Safari","domain":"booking.com"}"#,
        )
        .unwrap();

        crate::context_engine::operations::create_context_signal(
            &conn,
            Some(&ws_paris.id),
            Some(&paris_hotel.id),
            "url_metadata",
            None,
            r#"{"domain":"booking.com"}"#,
        )
        .unwrap();
        crate::context_engine::operations::create_context_signal(
            &conn,
            Some(&ws_paris.id),
            Some(&paris_hotel.id),
            "active_app",
            None,
            r#"{"app_name":"Safari"}"#,
        )
        .unwrap();

        // -------------------------------------------------------------------
        // Prepare Capture Payload (Simulating macOS Safari browser URL capture)
        // -------------------------------------------------------------------
        let raw_payload = RawCapturePayload::new(
            "https://booking.com/eiffel-tower-tour-tickets",
            CaptureMethod::Clipboard,
        )
        .with_platform(PlatformType::MacOS)
        .with_app_info(
            Some("Safari".to_string()),
            Some("Eiffel Tower Tickets Booking".to_string()),
        );

        // -------------------------------------------------------------------
        // Run CaptureCore Pipeline
        // -------------------------------------------------------------------
        let result = core.process_headless(&conn, Some(&engine), raw_payload).unwrap();

        // 1. Verify Item was persisted in DB
        assert_eq!(result.item.workspace_id, None, "Captured item must initially reside in Inbox");
        assert_eq!(result.item.item_type, "url");
        assert_eq!(result.item.content, "https://booking.com/eiffel-tower-tour-tickets");
        assert_eq!(result.item.title.as_deref(), Some("Eiffel Tower Tickets Booking"));

        // 2. Verify Session was created and associated
        assert!(result.session_id.is_some());
        assert_eq!(result.item.session_id, result.session_id);

        // 3. Verify ContextSignals were emitted
        assert!(result.signals_emitted >= 3, "Expected at least capture_event, active_app, url_metadata signals: {}", result.signals_emitted);
        let signals = crate::context_engine::operations::list_context_signals(&conn, None, Some(&result.item.id)).unwrap();
        assert!(signals.iter().any(|s| s.signal_type == "capture_event"));
        assert!(signals.iter().any(|s| s.signal_type == "active_app"));
        assert!(signals.iter().any(|s| s.signal_type == "url_metadata"));

        // 4. Verify Context Engine generated relationships
        let rels = operations::get_item_relationships(&conn, &result.item.id).unwrap();
        assert!(
            rels.iter().any(|r| r.item_b_id == paris_hotel.id || r.item_a_id == paris_hotel.id),
            "Captured item should automatically form domain relationship with Paris Hotel item"
        );

        // 5. Verify Context Scoring evaluates high affinity for Paris Trip
        let eval = crate::context_engine::scoring::evaluate_item_context(&conn, &result.item.id).unwrap();
        let paris_score = eval.existing_workspaces.iter().find(|w| w.workspace_id == ws_paris.id).unwrap();
        assert!(paris_score.score >= 0.25, "Paris Trip score should be elevated: {}", paris_score.score);

        // Clean up temp DB
        let _ = fs::remove_dir_all(db_path.parent().unwrap());
    }

    #[test]
    fn test_capture_core_respects_privacy_settings() {
        let (conn, db_path) = setup_test_db();
        let engine = create_test_engine();
        let core = CaptureCore::new();

        // Disable active app tracking in privacy settings
        let mut settings = crate::db::settings::get_settings(&conn).unwrap();
        settings.enable_active_app = false;
        settings.enable_browser_domain = false;
        crate::db::settings::update_settings(&conn, settings).unwrap();

        let raw = RawCapturePayload::new(
            "https://booking.com/paris-restaurant",
            CaptureMethod::Manual,
        )
        .with_app_info(
            Some("Safari".to_string()),
            Some("Paris Restaurant".to_string()),
        );

        let result = core.process_headless(&conn, Some(&engine), raw).unwrap();

        // Verify that active_app and url_metadata signals are SUPPRESSED per privacy settings
        let signals = crate::context_engine::operations::list_context_signals(&conn, None, Some(&result.item.id)).unwrap();
        assert!(signals.iter().any(|s| s.signal_type == "capture_event"), "capture_event signal must always exist");
        assert!(!signals.iter().any(|s| s.signal_type == "active_app"), "active_app must be suppressed by privacy settings");
        assert!(!signals.iter().any(|s| s.signal_type == "url_metadata"), "url_metadata must be suppressed by privacy settings");

        let _ = fs::remove_dir_all(db_path.parent().unwrap());
    }

    #[test]
    fn test_capture_core_error_resilience_on_invalid_input() {
        let (conn, db_path) = setup_test_db();
        let engine = create_test_engine();
        let core = CaptureCore::new();

        // Invalid empty input
        let empty_payload = RawCapturePayload::new("   ", CaptureMethod::Clipboard);
        let err = core.process_headless(&conn, Some(&engine), empty_payload).unwrap_err();
        assert_eq!(err, CaptureError::EmptyContent);

        // Verify no items or corrupted rows were written to DB
        let items = operations::list_items(&conn, None).unwrap();
        assert_eq!(items.len(), 0);

        let _ = fs::remove_dir_all(db_path.parent().unwrap());
    }
}

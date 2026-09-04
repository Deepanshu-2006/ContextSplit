#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;

    use crate::capture::core::CaptureCore;
    use crate::capture::models::*;
    use crate::capture::normalizer::CaptureNormalizer;
    use crate::capture::platform::{
        MacOSCaptureAdapter, MacOSClipboardAdapter, MockCaptureAdapter, PlatformCaptureAdapter,
        WindowsCaptureAdapter, WindowsClipboardAdapter,
    };
    use crate::capture::privacy::{
        ClipboardPrivacyFilter, PrivacyFilterResult, PrivacyRejectionReason, SensitiveContentType,
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
    // 1. Normalized CaptureEvent Model & Enums Tests
    // =======================================================================

    #[test]
    fn test_capture_event_model_fields_and_serialization() {
        let event_id = "evt_test_123456".to_string();
        let timestamp = "2026-09-04T12:00:00Z".to_string();

        let source_metadata = SourceMetadata {
            window_title: Some("Paris Hotel Booking - Safari".to_string()),
            process_name: Some("Safari".to_string()),
            bundle_identifier: Some("com.apple.Safari".to_string()),
            url_origin: Some("https://booking.com/paris-hotel".to_string()),
            extra: json!({ "tab_index": 2 }),
        };

        let event = CaptureEvent {
            event_id: event_id.clone(),
            capture_type: CaptureType::Clipboard,
            timestamp: timestamp.clone(),
            content_type: ContentType::Url,
            content: "https://booking.com/paris-hotel".to_string(),
            source_application: Some("Safari".to_string()),
            source_metadata: source_metadata.clone(),
            platform: PlatformType::MacOS,
            metadata: json!({
                "domain": "booking.com",
                "title": "Paris Hotel Booking",
                "size": 31
            }),
        };

        // Check fields
        assert_eq!(event.event_id, event_id);
        assert_eq!(event.capture_type, CaptureType::Clipboard);
        assert_eq!(event.timestamp, timestamp);
        assert_eq!(event.content_type, ContentType::Url);
        assert_eq!(event.content, "https://booking.com/paris-hotel");
        assert_eq!(event.source_application.as_deref(), Some("Safari"));
        assert_eq!(event.source_metadata.window_title.as_deref(), Some("Paris Hotel Booking - Safari"));
        assert_eq!(event.platform, PlatformType::MacOS);
        assert_eq!(event.item_type_str(), "url");
        assert_eq!(event.domain().as_deref(), Some("booking.com"));

        // Serialization roundtrip
        let serialized = serde_json::to_string(&event).unwrap();
        assert!(serialized.contains("event_id"));
        assert!(serialized.contains("clipboard"));
        assert!(serialized.contains("url"));
        assert!(serialized.contains("source_metadata"));

        let deserialized: CaptureEvent = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, event);
    }

    #[test]
    fn test_capture_types_and_extensibility() {
        // Core standard types
        assert_eq!(CaptureType::from("clipboard"), CaptureType::Clipboard);
        assert_eq!(CaptureType::from("drag_drop"), CaptureType::DragDrop);
        assert_eq!(CaptureType::from("drag_and_drop"), CaptureType::DragDrop);
        assert_eq!(CaptureType::from("manual"), CaptureType::Manual);

        // Future extensibility
        let future_ext = CaptureType::from("future_browser_extension_v2");
        assert_eq!(future_ext, CaptureType::Custom("future_browser_extension_v2".to_string()));
        assert_eq!(future_ext.to_string(), "future_browser_extension_v2");

        // Serde serialization of custom capture types
        let json_val = serde_json::to_string(&future_ext).unwrap();
        assert_eq!(json_val, "\"future_browser_extension_v2\"");
    }

    #[test]
    fn test_content_types_and_extensibility() {
        // Standard content types
        assert_eq!(ContentType::from("plain_text"), ContentType::PlainText);
        assert_eq!(ContentType::from("text"), ContentType::PlainText);
        assert_eq!(ContentType::from("url"), ContentType::Url);
        assert_eq!(ContentType::from("uri"), ContentType::Url);
        assert_eq!(ContentType::from("image"), ContentType::Image);
        assert_eq!(ContentType::from("file"), ContentType::File);
        assert_eq!(ContentType::from("rich_text"), ContentType::RichText);
        assert_eq!(ContentType::from("rtf"), ContentType::RichText);
        assert_eq!(ContentType::from("html"), ContentType::RichText);
        assert_eq!(ContentType::from("unknown"), ContentType::Unknown);

        // Custom future content types
        let custom_type = ContentType::from("code_snippet_rust");
        assert_eq!(custom_type, ContentType::Custom("code_snippet_rust".to_string()));
        assert_eq!(custom_type.to_string(), "code_snippet_rust");
    }

    #[test]
    fn test_capture_event_validation() {
        // Valid Event
        let valid_event = CaptureEvent::new(
            CaptureType::Manual,
            ContentType::PlainText,
            "Valid note content for ContextSplit",
            PlatformType::MacOS,
        );
        assert!(valid_event.validate().is_ok());

        // Empty content fails validation
        let mut empty_event = valid_event.clone();
        empty_event.content = "   ".to_string();
        assert_eq!(empty_event.validate(), Err(CaptureError::EmptyContent));

        // Invalid timestamp fails validation
        let mut bad_timestamp_event = valid_event.clone();
        bad_timestamp_event.timestamp = "invalid-date-format".to_string();
        assert!(matches!(
            bad_timestamp_event.validate(),
            Err(CaptureError::ProcessingError(_))
        ));

        // Null bytes in event_id fail validation
        let mut null_id_event = valid_event.clone();
        null_id_event.event_id = "evt_\0bad".to_string();
        assert!(matches!(
            null_id_event.validate(),
            Err(CaptureError::ProcessingError(_))
        ));

        // Invalid URL content type validation
        let mut bad_url_event = valid_event.clone();
        bad_url_event.content_type = ContentType::Url;
        bad_url_event.content = "ftp://invalid-url.com".to_string();
        assert!(matches!(
            bad_url_event.validate(),
            Err(CaptureError::InvalidUrl(_))
        ));
    }

    #[test]
    fn test_platform_decoupling() {
        let mac_event = CaptureEvent::new(
            CaptureType::Clipboard,
            ContentType::PlainText,
            "macOS text",
            PlatformType::MacOS,
        );
        let win_event = CaptureEvent::new(
            CaptureType::Clipboard,
            ContentType::PlainText,
            "Windows text",
            PlatformType::Windows,
        );
        let generic_event = CaptureEvent::new(
            CaptureType::Manual,
            ContentType::PlainText,
            "Generic text",
            PlatformType::Generic,
        );

        assert_eq!(mac_event.platform, PlatformType::MacOS);
        assert_eq!(win_event.platform, PlatformType::Windows);
        assert_eq!(generic_event.platform, PlatformType::Generic);

        // All platforms share identical validation and processing schema
        assert!(mac_event.validate().is_ok());
        assert!(win_event.validate().is_ok());
        assert!(generic_event.validate().is_ok());
    }

    // =======================================================================
    // 2. Validation Tests (CaptureValidator)
    // =======================================================================

    #[test]
    fn test_validator_rejects_empty_content() {
        let validator = CaptureValidator::new();

        let empty_payload = RawCapturePayload::new("", CaptureType::Manual);
        assert_eq!(validator.validate(&empty_payload), Err(CaptureError::EmptyContent));

        let whitespace_payload = RawCapturePayload::new("   \n\t  ", CaptureType::Clipboard);
        assert_eq!(validator.validate(&whitespace_payload), Err(CaptureError::EmptyContent));
    }

    #[test]
    fn test_validator_enforces_payload_size_limits() {
        let validator = CaptureValidator::with_limits(50, 100);

        let valid_payload = RawCapturePayload::new("Hello World", CaptureType::Manual);
        assert!(validator.validate(&valid_payload).is_ok());

        let oversized_text = "A".repeat(100);
        let invalid_payload = RawCapturePayload::new(oversized_text, CaptureType::Manual);
        assert!(matches!(
            validator.validate(&invalid_payload),
            Err(CaptureError::PayloadTooLarge { .. })
        ));
    }

    #[test]
    fn test_validator_rejects_null_bytes() {
        let validator = CaptureValidator::new();

        let null_content = RawCapturePayload::new("hello\0world", CaptureType::Manual);
        assert!(matches!(
            validator.validate(&null_content),
            Err(CaptureError::ProcessingError(_))
        ));

        let mut null_title = RawCapturePayload::new("valid content", CaptureType::Manual);
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
    // 3. Normalization Tests (CaptureNormalizer)
    // =======================================================================

    #[test]
    fn test_normalizer_text_normalization() {
        let normalizer = CaptureNormalizer::new();
        let payload = RawCapturePayload::new(
            "   First Line Title\r\nSecond line with content.   \r\n\r\n",
            CaptureType::Manual,
        );

        let event = normalizer.normalize(&payload).unwrap();
        assert_eq!(event.content_type, ContentType::PlainText);
        assert_eq!(event.title().as_deref(), Some("First Line Title"));
        assert!(!event.content.contains("\r\n"));
        assert!(event.content.contains('\n'));
        assert_eq!(event.mime_type().as_deref(), Some("text/plain"));
    }

    #[test]
    fn test_normalizer_url_normalization_and_domain_extraction() {
        let normalizer = CaptureNormalizer::new();
        let payload = RawCapturePayload::new(
            "https://www.booking.com/hotel/fr/le-marais.html?ref=123",
            CaptureType::Clipboard,
        );

        let event = normalizer.normalize(&payload).unwrap();
        assert_eq!(event.content_type, ContentType::Url);
        assert_eq!(event.domain().as_deref(), Some("booking.com"));
        assert_eq!(event.title().as_deref(), Some("Link from Booking.com"));
        assert_eq!(event.mime_type().as_deref(), Some("text/uri-list"));
        assert!(event.source_metadata.url_origin.is_some());
    }

    #[test]
    fn test_normalizer_file_normalization_and_mime_detection() {
        let normalizer = CaptureNormalizer::new();
        let mut payload = RawCapturePayload::new(
            "/Users/deepanshukhatri/Documents/Trip_Itinerary.pdf",
            CaptureType::DragDrop,
        );
        payload.file_paths = vec!["/Users/deepanshukhatri/Documents/Trip_Itinerary.pdf".to_string()];

        let event = normalizer.normalize(&payload).unwrap();
        assert_eq!(event.content_type, ContentType::File);
        assert_eq!(event.filename().as_deref(), Some("Trip_Itinerary.pdf"));
        assert_eq!(event.title().as_deref(), Some("Trip_Itinerary.pdf"));
        assert_eq!(event.mime_type().as_deref(), Some("application/pdf"));
    }

    #[test]
    fn test_normalizer_image_normalization() {
        let normalizer = CaptureNormalizer::new();
        let payload = RawCapturePayload::new(
            "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
            CaptureType::Clipboard,
        );

        let event = normalizer.normalize(&payload).unwrap();
        assert_eq!(event.content_type, ContentType::Image);
        assert_eq!(event.mime_type().as_deref(), Some("image/png"));
        assert_eq!(event.title().as_deref(), Some("Captured Image"));
    }

    // =======================================================================
    // 4. Platform Adapter Tests (macOS & Windows)
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
        assert_eq!(raw.source_method, CaptureType::Clipboard);
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
        assert_eq!(raw.source_method, CaptureType::DragDrop);
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
        assert_eq!(raw.source_method, CaptureType::Manual);
    }

    // =======================================================================
    // 5. CaptureCore 5-Step Pipeline Integration Tests
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
            CaptureType::Clipboard,
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
            CaptureType::Manual,
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
        let empty_payload = RawCapturePayload::new("   ", CaptureType::Clipboard);
        let err = core.process_headless(&conn, Some(&engine), empty_payload).unwrap_err();
        assert_eq!(err, CaptureError::EmptyContent);

        // Verify no items or corrupted rows were written to DB
        let items = operations::list_items(&conn, None).unwrap();
        assert_eq!(items.len(), 0);

        let _ = fs::remove_dir_all(db_path.parent().unwrap());
    }

    // =======================================================================
    // 6. CapturePipeline 9-Step & Capture Types Tests
    // =======================================================================

    #[test]
    fn test_capture_pipeline_clipboard_type() {
        let (conn, db_path) = setup_test_db();
        let engine = create_test_engine();
        let pipeline = crate::capture::CapturePipeline::new();

        let event = CaptureEvent::new(
            CaptureType::Clipboard,
            ContentType::PlainText,
            "Meeting notes from client synchronization call",
            PlatformType::MacOS,
        );

        let result = pipeline.process_event_headless(&conn, Some(&engine), event).unwrap();
        assert_eq!(result.item.item_type, "text");
        assert_eq!(result.item.content, "Meeting notes from client synchronization call");
        assert_eq!(result.item.workspace_id, None, "Must be placed in Inbox");
        assert!(result.session_id.is_some());
        assert!(!result.is_duplicate);
        assert!(result.signals_emitted >= 1);

        let _ = fs::remove_dir_all(db_path.parent().unwrap());
    }

    #[test]
    fn test_capture_pipeline_drag_drop_type() {
        let (conn, db_path) = setup_test_db();
        let engine = create_test_engine();
        let pipeline = crate::capture::CapturePipeline::new();

        let mut event = CaptureEvent::new(
            CaptureType::DragDrop,
            ContentType::File,
            "/Users/deepanshukhatri/Documents/Quarterly_Report_2026.pdf",
            PlatformType::MacOS,
        );
        event.source_application = Some("Finder".to_string());

        let result = pipeline.process_event_headless(&conn, Some(&engine), event).unwrap();
        assert_eq!(result.item.item_type, "file");
        assert_eq!(result.item.content, "/Users/deepanshukhatri/Documents/Quarterly_Report_2026.pdf");
        assert_eq!(result.item.title.as_deref(), Some("Quarterly_Report_2026.pdf"));
        assert!(!result.is_duplicate);

        let signals = crate::context_engine::operations::list_context_signals(&conn, None, Some(&result.item.id)).unwrap();
        assert!(signals.iter().any(|s| s.signal_type == "file_metadata"));

        let _ = fs::remove_dir_all(db_path.parent().unwrap());
    }

    #[test]
    fn test_capture_pipeline_manual_type() {
        let (conn, db_path) = setup_test_db();
        let engine = create_test_engine();
        let pipeline = crate::capture::CapturePipeline::new();

        let ws = operations::create_workspace(&conn, "College Project").unwrap();

        let mut event = CaptureEvent::new(
            CaptureType::Manual,
            ContentType::PlainText,
            "Research topic: Distributed Consensus in Context Engines",
            PlatformType::Generic,
        );
        event.metadata = json!({
            "workspace_id": ws.id,
            "title": "Consensus Research"
        });

        let result = pipeline.process_event_headless(&conn, Some(&engine), event).unwrap();
        assert_eq!(result.item.workspace_id.as_deref(), Some(ws.id.as_str()));
        assert_eq!(result.item.title.as_deref(), Some("Consensus Research"));
        assert!(!result.is_duplicate);

        let _ = fs::remove_dir_all(db_path.parent().unwrap());
    }

    #[test]
    fn test_capture_pipeline_custom_extensibility_type() {
        let (conn, db_path) = setup_test_db();
        let engine = create_test_engine();
        let pipeline = crate::capture::CapturePipeline::new();

        let custom_type = CaptureType::from("browser_extension_v3");
        let event = CaptureEvent::new(
            custom_type.clone(),
            ContentType::Url,
            "https://developer.mozilla.org/en-US/docs/Web/API",
            PlatformType::MacOS,
        );

        assert_eq!(event.capture_type, CaptureType::Custom("browser_extension_v3".to_string()));

        let result = pipeline.process_event_headless(&conn, Some(&engine), event).unwrap();
        assert_eq!(result.item.item_type, "url");
        assert_eq!(result.item.workspace_id, None);
        assert!(!result.is_duplicate);

        let signals = crate::context_engine::operations::list_context_signals(&conn, None, Some(&result.item.id)).unwrap();
        let cap_sig = signals.iter().find(|s| s.signal_type == "capture_event").unwrap();
        assert!(cap_sig.payload.contains("browser_extension_v3"));

        let _ = fs::remove_dir_all(db_path.parent().unwrap());
    }

    #[test]
    fn test_capture_pipeline_deduplication() {
        let (conn, db_path) = setup_test_db();
        let engine = create_test_engine();
        let pipeline = crate::capture::CapturePipeline::new().with_dedup_window_secs(10);

        let event1 = CaptureEvent::new(
            CaptureType::Clipboard,
            ContentType::PlainText,
            "Identical captured string",
            PlatformType::MacOS,
        );

        let res1 = pipeline.process_event_headless(&conn, Some(&engine), event1.clone()).unwrap();
        assert!(!res1.is_duplicate, "First capture must not be flagged duplicate");

        // Consecutive capture with identical content within window
        let res2 = pipeline.process_event_headless(&conn, Some(&engine), event1).unwrap();
        assert!(res2.is_duplicate, "Second identical capture must be flagged duplicate");
        assert_eq!(res2.item.id, res1.item.id, "Duplicate capture should reference the existing item");

        // Verify only 1 item was created in DB
        let items = operations::list_items(&conn, None).unwrap();
        assert_eq!(items.len(), 1, "Database should contain only 1 item");

        let _ = fs::remove_dir_all(db_path.parent().unwrap());
    }

    #[test]
    fn test_capture_pipeline_inbox_and_session_association() {
        let (conn, db_path) = setup_test_db();
        let engine = create_test_engine();
        let pipeline = crate::capture::CapturePipeline::new();

        let event = CaptureEvent::new(
            CaptureType::Clipboard,
            ContentType::PlainText,
            "Random inbox thought for later organization",
            PlatformType::MacOS,
        );

        let res = pipeline.process_event_headless(&conn, Some(&engine), event).unwrap();

        // 1. Verify item is in Inbox (workspace_id = None)
        assert!(res.item.workspace_id.is_none());

        // 2. Verify item is in the list of Inbox items
        let inbox_items = operations::list_items(&conn, None).unwrap();
        assert!(inbox_items.iter().any(|i| i.id == res.item.id));

        // 3. Verify session was associated
        assert!(res.session_id.is_some());
        assert_eq!(res.item.session_id, res.session_id);

        let _ = fs::remove_dir_all(db_path.parent().unwrap());
    }

    // =======================================================================
    // 7. Background Processing Queue & Latency Optimization Tests
    // =======================================================================

    #[test]
    fn test_capture_succeeds_when_ai_is_unavailable() {
        let (conn, db_path) = setup_test_db();
        // Create engine with NO AI reasoner
        let mut engine = ContextEngine::new();
        engine.register(Box::new(CaptureContextProvider));
        engine.register(Box::new(UserActionContextProvider));
        engine.register(Box::new(WorkspaceContextProvider));
        engine.register(Box::new(UrlContextProvider));
        engine.set_semantic_provider(Box::new(crate::semantic::MockSemanticProvider::new()));
        // Note: engine.set_ai_reasoner is NOT called, so AI is completely unavailable

        let pipeline = crate::capture::CapturePipeline::new();

        let event = CaptureEvent::new(
            CaptureType::Clipboard,
            ContentType::PlainText,
            "Capture note while AI provider service is offline",
            PlatformType::MacOS,
        );

        // Capture MUST succeed even if AI is unavailable
        let result = pipeline.process_event_headless(&conn, Some(&engine), event);
        assert!(result.is_ok(), "Capture must succeed when AI is unavailable");

        let capture_res = result.unwrap();
        assert_eq!(capture_res.item.content, "Capture note while AI provider service is offline");
        assert!(capture_res.item.workspace_id.is_none());

        // Verify item is stored in DB
        let item = operations::get_item(&conn, &capture_res.item.id).unwrap();
        assert_eq!(item.id, capture_res.item.id);

        let _ = fs::remove_dir_all(db_path.parent().unwrap());
    }

    #[test]
    fn test_capture_succeeds_when_embedding_generation_fails() {
        let (conn, db_path) = setup_test_db();
        // Create engine with NO semantic provider (simulating embedding generation failure/absence)
        let mut engine = ContextEngine::new();
        engine.register(Box::new(CaptureContextProvider));
        engine.register(Box::new(UserActionContextProvider));
        engine.register(Box::new(WorkspaceContextProvider));

        let pipeline = crate::capture::CapturePipeline::new();

        let event = CaptureEvent::new(
            CaptureType::Clipboard,
            ContentType::PlainText,
            "Text captured when embedding model fails or is downloading",
            PlatformType::MacOS,
        );

        // Capture MUST succeed immediately
        let result = pipeline.process_event_headless(&conn, Some(&engine), event);
        assert!(result.is_ok(), "Capture must succeed when embedding generation fails");

        let capture_res = result.unwrap();
        assert_eq!(capture_res.item.content, "Text captured when embedding model fails or is downloading");

        // Verify status is recorded as failed and retry count is incremented safely
        let status = operations::get_item_semantic_status(&conn, &capture_res.item.id).unwrap().unwrap();
        assert_eq!(status.status, "failed");
        assert!(status.retries >= 1);

        let _ = fs::remove_dir_all(db_path.parent().unwrap());
    }

    #[test]
    fn test_background_processing_retries_safely() {
        let (conn, db_path) = setup_test_db();
        let queue = crate::capture::BackgroundProcessingQueue::new();

        // 1. Create an item in the DB with failed semantic status and 1 retry
        let item = operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Paris travel guide for background retry test",
            Some("Paris Guide"),
            "{}",
        )
        .unwrap();

        operations::update_item_semantic_status(&conn, &item.id, "failed", None).unwrap();
        operations::increment_semantic_retries(&conn, &item.id).unwrap();

        let initial_status = operations::get_item_semantic_status(&conn, &item.id).unwrap().unwrap();
        assert_eq!(initial_status.status, "failed");
        assert_eq!(initial_status.retries, 1);

        // 2. Configure engine with a working provider now available
        let mut engine = ContextEngine::new();
        engine.set_semantic_provider(Box::new(crate::semantic::MockSemanticProvider::new()));

        // 3. Process retry job through the queue
        let job = crate::capture::BackgroundJob::new(&item.id, crate::capture::BackgroundJobType::FullContextualPipeline)
            .with_max_retries(3);

        let job_res = queue.process_job_headless(&conn, &engine, &job);
        assert!(job_res.success, "Background retry must succeed when provider is restored");

        // 4. Verify item semantic status transitioned to "ready"
        let updated_status = operations::get_item_semantic_status(&conn, &item.id).unwrap().unwrap();
        assert_eq!(updated_status.status, "ready");

        // 5. Verify embedding was persisted
        let emb_meta = operations::get_item_embedding_metadata(&conn, &item.id).unwrap();
        assert!(emb_meta.is_some());

        let _ = fs::remove_dir_all(db_path.parent().unwrap());
    }

    #[test]
    fn test_duplicate_processing_is_avoided() {
        let queue = crate::capture::BackgroundProcessingQueue::new();
        let item_id = "item_test_duplicate_job_999";

        let job1 = crate::capture::BackgroundJob::new(item_id, crate::capture::BackgroundJobType::FullContextualPipeline);
        let job2 = crate::capture::BackgroundJob::new(item_id, crate::capture::BackgroundJobType::FullContextualPipeline);

        // First enqueue must succeed
        assert!(queue.enqueue(job1.clone()), "First job must be enqueued");
        assert!(queue.is_in_flight(item_id, &crate::capture::BackgroundJobType::FullContextualPipeline));

        // Concurrent duplicate enqueue for identical item and job type MUST be rejected
        assert!(!queue.enqueue(job2), "Duplicate in-flight job must be rejected to prevent duplicate processing");
        assert_eq!(queue.len(), 1, "Queue length should remain 1");

        // Dequeue and complete
        let dequeued = queue.dequeue().unwrap();
        assert_eq!(dequeued.item_id, item_id);

        queue.mark_completed(item_id, &crate::capture::BackgroundJobType::FullContextualPipeline);
        assert!(!queue.is_in_flight(item_id, &crate::capture::BackgroundJobType::FullContextualPipeline));

        // Now a subsequent job CAN be enqueued
        let job3 = crate::capture::BackgroundJob::new(item_id, crate::capture::BackgroundJobType::FullContextualPipeline);
        assert!(queue.enqueue(job3), "New job after completion should be accepted");
    }

    // =======================================================================
    // 8. MacOSClipboardAdapter & WindowsClipboardAdapter Tests
    // =======================================================================

    #[test]
    fn test_macos_clipboard_adapter_emits_normalized_capture_event() {
        let mut settings = PrivacySettings::default();
        settings.enable_active_app = true;
        settings.enable_window_title = true;
        settings.enable_browser_domain = true;

        let mut adapter = MacOSClipboardAdapter::new().with_privacy_settings(settings);

        // 1. Capture text
        let res = adapter.on_clipboard_change(
            Some("Meeting discussion regarding Paris architecture".to_string()),
            Some("Notes".to_string()),
            Some("Sprint Notes - Notes".to_string()),
        ).unwrap();

        assert!(res.is_some());
        let event = res.unwrap();
        assert_eq!(event.platform, PlatformType::MacOS);
        assert_eq!(event.capture_type, CaptureType::Clipboard);
        assert_eq!(event.content_type, ContentType::PlainText);
        assert_eq!(event.content, "Meeting discussion regarding Paris architecture");
        assert_eq!(event.source_application.as_deref(), Some("Notes"));
        assert_eq!(event.source_metadata.window_title.as_deref(), Some("Sprint Notes - Notes"));
        assert!(event.validate().is_ok());

        // 2. Immediate duplicate must be skipped (avoid repeatedly capturing same content)
        let dup_res = adapter.on_clipboard_change(
            Some("Meeting discussion regarding Paris architecture".to_string()),
            Some("Notes".to_string()),
            Some("Sprint Notes - Notes".to_string()),
        ).unwrap();
        assert!(dup_res.is_none(), "Duplicate content must return None");

        // 3. Capture URL
        let url_res = adapter.on_clipboard_change(
            Some("https://developer.apple.com/documentation/appkit".to_string()),
            Some("Safari".to_string()),
            Some("AppKit Documentation".to_string()),
        ).unwrap();

        assert!(url_res.is_some());
        let url_event = url_res.unwrap();
        assert_eq!(url_event.content_type, ContentType::Url);
        assert_eq!(url_event.content, "https://developer.apple.com/documentation/appkit");
        assert_eq!(url_event.domain().as_deref(), Some("developer.apple.com"));
        assert!(url_event.validate().is_ok());
    }

    #[test]
    fn test_windows_clipboard_adapter_emits_normalized_capture_event() {
        let mut settings = PrivacySettings::default();
        settings.enable_active_app = true;
        settings.enable_window_title = true;
        settings.enable_browser_domain = true;

        let mut adapter = WindowsClipboardAdapter::new().with_privacy_settings(settings);

        // 1. Capture Windows text with CRLF
        let res = adapter.on_clipboard_change(
            Some("Windows line 1\r\nWindows line 2".to_string()),
            Some("notepad.exe".to_string()),
            Some("Document.txt - Notepad".to_string()),
        ).unwrap();

        assert!(res.is_some());
        let event = res.unwrap();
        assert_eq!(event.platform, PlatformType::Windows);
        assert_eq!(event.capture_type, CaptureType::Clipboard);
        assert_eq!(event.content_type, ContentType::PlainText);
        assert_eq!(event.content, "Windows line 1\nWindows line 2", "CRLF must be normalized to LF");
        assert_eq!(event.source_application.as_deref(), Some("notepad.exe"));
        assert_eq!(event.source_metadata.window_title.as_deref(), Some("Document.txt - Notepad"));
        assert!(event.validate().is_ok());

        // 2. Duplicate must return None
        let dup_res = adapter.on_clipboard_change(
            Some("Windows line 1\r\nWindows line 2".to_string()),
            Some("notepad.exe".to_string()),
            Some("Document.txt - Notepad".to_string()),
        ).unwrap();
        assert!(dup_res.is_none());

        // 3. Capture URL
        let url_res = adapter.on_clipboard_change(
            Some("https://learn.microsoft.com/en-us/windows/win32/".to_string()),
            Some("msedge.exe".to_string()),
            Some("Win32 API - Microsoft Edge".to_string()),
        ).unwrap();

        assert!(url_res.is_some());
        let url_event = url_res.unwrap();
        assert_eq!(url_event.content_type, ContentType::Url);
        assert_eq!(url_event.platform, PlatformType::Windows);
        assert_eq!(url_event.domain().as_deref(), Some("learn.microsoft.com"));
        assert!(url_event.validate().is_ok());
    }

    #[test]
    fn test_clipboard_adapters_respect_disabled_state() {
        let mut mac_adapter = MacOSClipboardAdapter::new().with_enabled(false);
        let mut win_adapter = WindowsClipboardAdapter::new().with_enabled(false);

        // When disabled, on_clipboard_change must immediately return Ok(None)
        assert!(mac_adapter.on_clipboard_change(Some("Text".to_string()), None, None).unwrap().is_none());
        assert!(win_adapter.on_clipboard_change(Some("Text".to_string()), None, None).unwrap().is_none());

        // Re-enabling allows capture
        mac_adapter.set_enabled(true);
        win_adapter.set_enabled(true);

        assert!(mac_adapter.on_clipboard_change(Some("Text 1".to_string()), None, None).unwrap().is_some());
        assert!(win_adapter.on_clipboard_change(Some("Text 2".to_string()), None, None).unwrap().is_some());
    }

    #[test]
    fn test_clipboard_adapters_respect_privacy_settings() {
        let mut privacy = PrivacySettings::default();
        privacy.enable_active_app = false; // Scrub app
        privacy.enable_window_title = false; // Scrub window
        privacy.enable_browser_domain = false; // Scrub domain

        let mut mac_adapter = MacOSClipboardAdapter::new().with_privacy_settings(privacy.clone());
        let mut win_adapter = WindowsClipboardAdapter::new().with_privacy_settings(privacy);

        let mac_event = mac_adapter.on_clipboard_change(
            Some("https://github.com/org/repo".to_string()),
            Some("Safari".to_string()),
            Some("Secret Project - Safari".to_string()),
        ).unwrap().unwrap();

        let win_event = win_adapter.on_clipboard_change(
            Some("https://github.com/org/repo".to_string()),
            Some("chrome.exe".to_string()),
            Some("Secret Project - Chrome".to_string()),
        ).unwrap().unwrap();

        // App and Window title MUST be stripped per privacy settings
        assert!(mac_event.source_application.is_none());
        assert!(mac_event.source_metadata.window_title.is_none());
        assert!(mac_event.domain().is_none());

        assert!(win_event.source_application.is_none());
        assert!(win_event.source_metadata.window_title.is_none());
        assert!(win_event.domain().is_none());
    }

    #[test]
    fn test_privacy_filter_normal_text_allowed() {
        let filter = ClipboardPrivacyFilter::new();

        let res = filter.filter_clipboard(
            "Here is the project proposal for the Paris trip itinerary and flight details.",
            Some("Notes"),
            Some("Paris Trip - Notes"),
        );

        assert!(res.is_allowed());
        assert_eq!(res, PrivacyFilterResult::Allowed);
    }

    #[test]
    fn test_privacy_filter_password_rejection() {
        let filter = ClipboardPrivacyFilter::new();

        // 1. Key-value style password assignment
        let res1 = filter.filter_clipboard("password = SuperSecretPass123!", None, None);
        assert!(res1.is_rejected());
        if let PrivacyFilterResult::Rejected(rej) = res1 {
            assert_eq!(
                rej.reason,
                PrivacyRejectionReason::SensitiveContent {
                    content_type: SensitiveContentType::Password,
                }
            );
            assert!(rej.explanation.contains("Password-like credential detected"));
            assert!(!rej.explanation.contains("SuperSecretPass123!"), "Secret must never appear in explanation");
        }

        // 2. High complexity token without spaces
        let res2 = filter.filter_clipboard("K9#vL2$pQ8@zM1!", None, None);
        assert!(res2.is_rejected());
        if let PrivacyFilterResult::Rejected(rej) = res2 {
            assert_eq!(
                rej.reason,
                PrivacyRejectionReason::SensitiveContent {
                    content_type: SensitiveContentType::Password,
                }
            );
            assert!(!rej.explanation.contains("K9#vL2$pQ8@zM1!"), "Secret must never appear in explanation");
        }
    }

    #[test]
    fn test_privacy_filter_otp_rejection() {
        let filter = ClipboardPrivacyFilter::new();

        // 1. Exact 6-digit numeric code
        let res1 = filter.filter_clipboard("482910", None, None);
        assert!(res1.is_rejected());
        if let PrivacyFilterResult::Rejected(rej) = res1 {
            assert_eq!(
                rej.reason,
                PrivacyRejectionReason::SensitiveContent {
                    content_type: SensitiveContentType::OneTimePassword,
                }
            );
            assert!(rej.explanation.contains("One-Time Password (OTP)"));
            assert!(!rej.explanation.contains("482910"), "OTP code must never appear in explanation");
        }

        // 2. Formatted 6-digit code with dash (e.g. 123-456)
        let res2 = filter.filter_clipboard("123-456", None, None);
        assert!(res2.is_rejected());

        // 3. SMS / Email verification message
        let res3 = filter.filter_clipboard("Your verification code is: 839201", None, None);
        assert!(res3.is_rejected());
        if let PrivacyFilterResult::Rejected(rej) = res3 {
            assert_eq!(
                rej.reason,
                PrivacyRejectionReason::SensitiveContent {
                    content_type: SensitiveContentType::OneTimePassword,
                }
            );
            assert!(!rej.explanation.contains("839201"));
        }
    }

    #[test]
    fn test_privacy_filter_excluded_applications() {
        let mut filter = ClipboardPrivacyFilter::new();

        // 1. Default password managers (1Password, Bitwarden, KeePassXC)
        let res_1p = filter.filter_clipboard("My non-sensitive text", Some("1Password 8"), None);
        assert!(res_1p.is_rejected());
        if let PrivacyFilterResult::Rejected(rej) = res_1p {
            assert_eq!(
                rej.reason,
                PrivacyRejectionReason::ExcludedApplication {
                    application_name: "1Password 8".to_string(),
                }
            );
            assert!(rej.explanation.contains("1Password 8"));
        }

        let res_bw = filter.filter_clipboard("My other text", Some("bitwarden.exe"), None);
        assert!(res_bw.is_rejected());

        // 2. Custom excluded application
        filter.add_excluded_application("Slack");
        let res_slack = filter.filter_clipboard("Confidential chat", Some("Slack"), None);
        assert!(res_slack.is_rejected());
        if let PrivacyFilterResult::Rejected(rej) = res_slack {
            assert_eq!(
                rej.reason,
                PrivacyRejectionReason::ExcludedApplication {
                    application_name: "Slack".to_string(),
                }
            );
        }
    }

    #[test]
    fn test_privacy_filter_monitoring_disabled() {
        let mut filter = ClipboardPrivacyFilter::new();
        filter.set_enabled(false);

        let res = filter.filter_clipboard("Ordinary document note", Some("Notes"), None);
        assert!(res.is_rejected());
        if let PrivacyFilterResult::Rejected(rej) = res {
            assert_eq!(rej.reason, PrivacyRejectionReason::MonitoringDisabled);
            assert!(rej.explanation.contains("disabled"));
        }
    }

    #[test]
    fn test_privacy_filter_temporary_pause_and_resume() {
        let mut filter = ClipboardPrivacyFilter::new();

        // 1. Pause for 10 minutes
        filter.pause_for(std::time::Duration::from_secs(600));
        assert!(filter.is_paused());

        let res_paused = filter.filter_clipboard("Important architecture note", Some("Notes"), None);
        assert!(res_paused.is_rejected());
        if let PrivacyFilterResult::Rejected(rej) = res_paused {
            assert!(matches!(rej.reason, PrivacyRejectionReason::MonitoringPaused { .. }));
            assert!(rej.explanation.contains("paused"));
        }

        // 2. Resume
        filter.resume();
        assert!(!filter.is_paused());

        let res_resumed = filter.filter_clipboard("Important architecture note", Some("Notes"), None);
        assert!(res_resumed.is_allowed());

        // 3. Pause indefinitely
        filter.pause_indefinitely();
        assert!(filter.is_paused());
        assert!(filter.filter_clipboard("Some text", None, None).is_rejected());

        filter.resume();
        assert!(!filter.is_paused());
    }

    #[test]
    fn test_privacy_filter_api_keys_and_private_keys() {
        let filter = ClipboardPrivacyFilter::new();

        // 1. AWS Key
        let res_aws = filter.filter_clipboard("AKIAIOSFODNN7EXAMPLE", None, None);
        assert!(res_aws.is_rejected());

        // 2. OpenAI Key
        let res_openai = filter.filter_clipboard("sk-live1234567890abcdef1234567890", None, None);
        assert!(res_openai.is_rejected());

        // 3. Cryptographic Private Key
        let res_pk = filter.filter_clipboard(
            "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA0...\n-----END RSA PRIVATE KEY-----",
            None,
            None,
        );
        assert!(res_pk.is_rejected());

        // 4. Database connection string with password
        let res_db = filter.filter_clipboard("postgres://admin:SecretPass@db.internal.net:5432/prod", None, None);
        assert!(res_db.is_rejected());
    }

    #[test]
    fn test_privacy_filter_pipeline_blocks_persistence_and_ai() {
        let (conn, _temp_dir) = setup_test_db();
        let pipeline = crate::capture::CapturePipeline::new();

        // Attempt capturing a password through the pipeline
        let password_event = CaptureEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            capture_type: CaptureType::Clipboard,
            timestamp: chrono::Utc::now().to_rfc3339(),
            content_type: ContentType::PlainText,
            content: "SuperSecretTokenPassword123!".to_string(),
            source_application: Some("Terminal".to_string()),
            source_metadata: SourceMetadata::default(),
            platform: PlatformType::current(),
            metadata: json!({}),
        };

        let result = pipeline.process_event_headless(&conn, None, password_event);
        assert!(result.is_err(), "Sensitive clipboard content must be rejected");

        if let Err(CaptureError::PrivacyRejected(explanation)) = result {
            assert!(explanation.contains("Password-like credential detected"));
            assert!(!explanation.contains("SuperSecretTokenPassword123!"), "Secret must not leak into error");
        } else {
            panic!("Expected PrivacyRejected error, got {:?}", result);
        }

        // Verify that NO item was written to database
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 0, "Zero items must be written to DB on privacy rejection");

        // Verify that NO context signals were recorded
        let sig_count: i64 = conn.query_row("SELECT COUNT(*) FROM context_signals", [], |row| row.get(0)).unwrap();
        assert_eq!(sig_count, 0, "Zero context signals must be recorded on privacy rejection");
    }
}

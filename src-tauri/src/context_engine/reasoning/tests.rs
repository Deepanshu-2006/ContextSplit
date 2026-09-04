#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;

    use crate::context_engine::reasoning::builder::{
        analyze_item_context_ai, build_evidence_package, scrub_private_data, EvidencePackageBuilder,
    };
    use crate::context_engine::reasoning::mock::MockAIReasoner;
    use crate::context_engine::reasoning::models::*;
    use crate::context_engine::reasoning::traits::AIReasoner;
    use crate::context_engine::reasoning::validator::{
        parse_and_validate_response, validate_reasoning_result, ReasoningValidationError,
    };
    use crate::db::{self, operations, settings::PrivacySettings};

    fn setup_test_db() -> (Connection, PathBuf) {
        let temp_dir = std::env::temp_dir().join(format!("contextsplit_ai_test_{}", uuid::Uuid::new_v4()));
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
        };
        crate::db::settings::update_settings(&conn, settings).unwrap();

        (conn, db_path)
    }

    fn dummy_target_item() -> TargetItemEvidence {
        TargetItemEvidence {
            id: "item_target_1".to_string(),
            item_type: "text".to_string(),
            title: Some("Eiffel Tower Tickets".to_string()),
            content_snippet: "Booked Eiffel Tower summit tickets for 8pm".to_string(),
            app_name: Some("Google Chrome".to_string()),
            domain: Some("toureiffel.paris".to_string()),
            session_id: Some("sess_100".to_string()),
            captured_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    fn dummy_temporal_evidence() -> TemporalEvidence {
        TemporalEvidence {
            age_days: 0.1,
            recency_decay_weight: 1.0,
            is_recent: true,
        }
    }

    fn dummy_metadata() -> EvidenceMetadata {
        EvidenceMetadata {
            generated_at: chrono::Utc::now().to_rfc3339(),
            engine_version: "ContextSplit-v0.1.0".to_string(),
            total_db_items_excluded: 0,
            privacy_boundary: "test_package".to_string(),
        }
    }

    fn base_dummy_package(candidates: Vec<CandidateContextEvidence>) -> EvidencePackage {
        EvidencePackage {
            target_item: dummy_target_item(),
            candidate_contexts: candidates,
            relevant_related_items: Vec::new(),
            relationships: Vec::new(),
            semantic_similarity_summaries: Vec::new(),
            relevant_sessions: Vec::new(),
            session_context: None,
            workspace_semantic_profiles: Vec::new(),
            contextual_signals: Vec::new(),
            temporal_context: dummy_temporal_evidence(),
            system_metadata: dummy_metadata(),
        }
    }

    #[test]
    fn test_mock_reasoner_clear_winner() {
        let mut package = base_dummy_package(vec![
            CandidateContextEvidence {
                context_id: "ws_paris".to_string(),
                name: "Paris Trip".to_string(),
                context_type: "workspace".to_string(),
                lifecycle_state: "active".to_string(),
                deterministic_score: 0.88,
                deterministic_confidence: 0.88,
                confidence_band: "High".to_string(),
                evidence_factors: vec![
                    "Similar to 3 items in Paris Trip.".to_string(),
                    "Captured during the same activity session.".to_string(),
                ],
                item_count: 5,
                temporal_recency_score: 1.0,
            },
            CandidateContextEvidence {
                context_id: "ws_work".to_string(),
                name: "Work Projects".to_string(),
                context_type: "workspace".to_string(),
                lifecycle_state: "active".to_string(),
                deterministic_score: 0.20,
                deterministic_confidence: 0.20,
                confidence_band: "Low".to_string(),
                evidence_factors: vec!["Baseline comparison with minimal contextual overlap with Work Projects.".to_string()],
                item_count: 12,
                temporal_recency_score: 0.8,
            },
        ]);
        package.session_context = Some(SessionEvidence {
            session_id: "sess_100".to_string(),
            item_count: 3,
            is_target_session: true,
            associated_workspaces: vec!["ws_paris".to_string()],
        });

        let reasoner = MockAIReasoner::new();
        let result = reasoner.analyze_context(&package).unwrap();

        assert_eq!(result.decision, ContextReasoningDecision::ExistingContext);
        assert_eq!(result.suggested_context_reference, Some("ws_paris".to_string()));
        assert_eq!(result.suggested_name, Some("Paris Trip".to_string()));
        assert!(result.confidence >= 0.85);
        assert!(result.explanation.contains("Paris Trip"));
        assert_eq!(result.uncertainty_reason, None);
        assert_eq!(result.alternative_interpretations.len(), 1);
        assert_eq!(result.alternative_interpretations[0].context_name, "Work Projects");
    }

    #[test]
    fn test_mock_reasoner_ambiguous_contexts() {
        let package = base_dummy_package(vec![
            CandidateContextEvidence {
                context_id: "ws_paris".to_string(),
                name: "Paris Trip".to_string(),
                context_type: "workspace".to_string(),
                lifecycle_state: "active".to_string(),
                deterministic_score: 0.62,
                deterministic_confidence: 0.62,
                confidence_band: "High".to_string(),
                evidence_factors: vec!["Common domain 'toureiffel.paris'".to_string()],
                item_count: 4,
                temporal_recency_score: 1.0,
            },
            CandidateContextEvidence {
                context_id: "ws_france".to_string(),
                name: "France Research".to_string(),
                context_type: "workspace".to_string(),
                lifecycle_state: "active".to_string(),
                deterministic_score: 0.58,
                deterministic_confidence: 0.58,
                confidence_band: "Medium".to_string(),
                evidence_factors: vec!["Similar to 2 items in France Research.".to_string()],
                item_count: 6,
                temporal_recency_score: 1.0,
            },
        ]);

        let reasoner = MockAIReasoner::new();
        let result = reasoner.analyze_context(&package).unwrap();

        assert_eq!(result.decision, ContextReasoningDecision::Uncertain);
        assert!(result.suggested_context_reference.is_some());
        assert!(result.uncertainty_reason.is_some());
        assert!(result.explanation.contains("Contextual ambiguity detected"));
        assert_eq!(result.alternative_interpretations.len(), 1);
        assert_eq!(result.alternative_interpretations[0].context_name, "France Research");
    }

    #[test]
    fn test_mock_reasoner_weak_evidence_recommends_inbox() {
        let package = base_dummy_package(vec![CandidateContextEvidence {
            context_id: "ws_misc".to_string(),
            name: "Misc".to_string(),
            context_type: "workspace".to_string(),
            lifecycle_state: "active".to_string(),
            deterministic_score: 0.15,
            deterministic_confidence: 0.15,
            confidence_band: "Low".to_string(),
            evidence_factors: vec!["Weak single match".to_string()],
            item_count: 1,
            temporal_recency_score: 0.5,
        }]);

        let reasoner = MockAIReasoner::new();
        let result = reasoner.analyze_context(&package).unwrap();

        assert_eq!(result.decision, ContextReasoningDecision::NoContext);
        assert_eq!(result.suggested_context_reference, None);
        assert!(result.uncertainty_reason.is_some());
        assert!(result.explanation.contains("Weak contextual evidence"));
    }

    #[test]
    fn test_mock_reasoner_empty_candidates() {
        let package = base_dummy_package(vec![]);

        let reasoner = MockAIReasoner::new();
        let result = reasoner.analyze_context(&package).unwrap();

        assert_eq!(result.decision, ContextReasoningDecision::NoContext);
        assert_eq!(result.suggested_context_reference, None);
        assert_eq!(result.confidence, 0.0);
        assert!(result.uncertainty_reason.is_some());
    }

    #[test]
    fn test_mock_reasoner_archived_workspace_suppressed() {
        let package = base_dummy_package(vec![CandidateContextEvidence {
            context_id: "ws_old_trip".to_string(),
            name: "Old Trip 2022".to_string(),
            context_type: "workspace".to_string(),
            lifecycle_state: "archived".to_string(),
            deterministic_score: 0.85,
            deterministic_confidence: 0.85,
            confidence_band: "High".to_string(),
            evidence_factors: vec!["High semantic overlap".to_string()],
            item_count: 10,
            temporal_recency_score: 0.2,
        }]);

        let reasoner = MockAIReasoner::new();
        let result = reasoner.analyze_context(&package).unwrap();

        assert_eq!(result.decision, ContextReasoningDecision::NoContext);
        assert_eq!(result.suggested_context_reference, None);
        assert!(result.explanation.contains("archived"));
        assert!(result.uncertainty_reason.is_some());
    }

    #[test]
    fn test_provider_abstraction_pluggability() {
        struct CustomRuleReasoner;
        impl AIReasoner for CustomRuleReasoner {
            fn provider_name(&self) -> &str {
                "Custom Rule-Based Engine"
            }
            fn provider_id(&self) -> &str {
                "custom-rule"
            }
            fn analyze_context(
                &self,
                package: &EvidencePackage,
            ) -> Result<ContextReasoningResult, AIReasoningError> {
                let res = ContextReasoningResult {
                    decision: ContextReasoningDecision::ExistingContext,
                    suggested_context_reference: package.candidate_contexts.first().map(|c| c.context_id.clone()),
                    suggested_name: package.candidate_contexts.first().map(|c| c.name.clone()),
                    confidence: 0.99,
                    explanation: "Custom engine verified rules".to_string(),
                    evidence_references: vec!["Custom factor 1".to_string()],
                    uncertainty_reason: None,
                    alternative_interpretations: vec![],
                    provider_info: Some(ReasoningProviderInfo {
                        provider_id: self.provider_id().to_string(),
                        provider_type: "custom".to_string(),
                        model_version: "v1".to_string(),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    }),
                };
                res.validate().map_err(|e| AIReasoningError::InvalidResponse(e.to_string()))?;
                Ok(res)
            }
        }

        let package = base_dummy_package(vec![CandidateContextEvidence {
            context_id: "c1".to_string(),
            name: "Custom Context".to_string(),
            context_type: "workspace".to_string(),
            lifecycle_state: "active".to_string(),
            deterministic_score: 0.95,
            deterministic_confidence: 0.95,
            confidence_band: "High".to_string(),
            evidence_factors: vec![],
            item_count: 2,
            temporal_recency_score: 1.0,
        }]);

        let custom_reasoner: Box<dyn AIReasoner> = Box::new(CustomRuleReasoner);
        let result = custom_reasoner.analyze_context(&package).unwrap();

        assert_eq!(result.decision, ContextReasoningDecision::ExistingContext);
        assert_eq!(result.suggested_context_reference, Some("c1".to_string()));
        assert_eq!(result.provider_info.as_ref().unwrap().provider_id, "custom-rule");
    }

    #[test]
    fn test_build_evidence_package_and_analyze_with_db() {
        let (conn, _db_path) = setup_test_db();
        let reasoner = MockAIReasoner::new();

        let session = operations::create_session(&conn).unwrap();
        // 1. Create a workspace "Paris Trip"
        let ws = operations::create_workspace(&conn, "Paris Trip").unwrap();

        // 2. Add an item in Paris Trip
        let item1 = operations::create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "Flight to Paris CDG on Air France",
            Some("Flight Booking"),
            "{}",
        ).unwrap();

        // 3. Add context signal for item1 (active_app: Chrome)
        crate::context_engine::operations::create_context_signal(
            &conn,
            Some(&ws.id),
            Some(&item1.id),
            "active_app",
            Some(&session.id),
            &json!({ "app_name": "Google Chrome" }).to_string(),
        ).unwrap();

        // 4. Capture a new item left in Inbox (workspace_id = None)
        let inbox_item = operations::create_item(
            &conn,
            None,
            Some(&session.id),
            "text",
            "Hotel Louvre Paris reservation confirmation",
            Some("Hotel Booking"),
            "{}",
        ).unwrap();

        // 5. Add signal for inbox_item (active_app: Chrome)
        crate::context_engine::operations::create_context_signal(
            &conn,
            None,
            Some(&inbox_item.id),
            "active_app",
            Some(&session.id),
            &json!({ "app_name": "Google Chrome" }).to_string(),
        ).unwrap();

        // 6. Create semantic relationship between items
        operations::create_item_relationship(
            &conn,
            &inbox_item.id,
            &item1.id,
            "semantic_similarity",
            0.85,
            None,
        ).unwrap();

        // 7. Build EvidencePackage using EvidencePackageBuilder
        let builder = EvidencePackageBuilder::default();
        let package = builder.build(&conn, &inbox_item.id).unwrap();

        assert_eq!(package.target_item.id, inbox_item.id);
        assert_eq!(package.target_item.app_name, Some("Google Chrome".to_string()));
        assert!(!package.candidate_contexts.is_empty());
        assert_eq!(package.candidate_contexts[0].name, "Paris Trip");
        assert!(!package.relationships.is_empty());
        assert!(!package.relevant_related_items.is_empty());
        assert_eq!(package.relevant_related_items[0].item_id, item1.id);
        assert!(!package.semantic_similarity_summaries.is_empty());

        // 8. Analyze with AI reasoner
        let reasoning_result = analyze_item_context_ai(&conn, &inbox_item.id, &reasoner).unwrap();
        assert_eq!(reasoning_result.decision, ContextReasoningDecision::ExistingContext);
        assert_eq!(reasoning_result.suggested_context_reference, Some(ws.id.clone()));
        assert_eq!(reasoning_result.suggested_name, Some("Paris Trip".to_string()));
        assert!(!reasoning_result.explanation.is_empty());
    }

    #[test]
    fn test_ai_layer_strict_non_mutation_invariant() {
        let (conn, _db_path) = setup_test_db();
        let reasoner = MockAIReasoner::new();

        // Setup environment: 2 workspaces, 3 items
        let ws1 = operations::create_workspace(&conn, "Paris Trip").unwrap();
        let _ws2 = operations::create_workspace(&conn, "Work Project").unwrap();

        let _item1 = operations::create_item(&conn, Some(&ws1.id), None, "text", "Paris hotel", Some("Hotel"), "{}").unwrap();
        let _item2 = operations::create_item(&conn, Some(&ws1.id), None, "text", "Paris flight", Some("Flight"), "{}").unwrap();
        let inbox_item = operations::create_item(&conn, None, None, "text", "Eiffel tower ticket", Some("Ticket"), "{}").unwrap();

        // Add a signal
        crate::context_engine::operations::create_context_signal(
            &conn,
            None,
            Some(&inbox_item.id),
            "active_app",
            None,
            &json!({ "app_name": "Google Chrome" }).to_string(),
        ).unwrap();

        // Query state counts BEFORE AI reasoning
        let items_count_before: i64 = conn.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0)).unwrap();
        let workspaces_count_before: i64 = conn.query_row("SELECT COUNT(*) FROM workspaces", [], |r| r.get(0)).unwrap();
        let memberships_count_before: i64 = conn.query_row("SELECT COUNT(*) FROM context_memberships", [], |r| r.get(0)).unwrap();
        let signals_count_before: i64 = conn.query_row("SELECT COUNT(*) FROM context_signals", [], |r| r.get(0)).unwrap();
        let candidates_count_before: i64 = conn.query_row("SELECT COUNT(*) FROM context_candidates", [], |r| r.get(0)).unwrap();

        let inbox_item_before = operations::get_item(&conn, &inbox_item.id).unwrap();
        assert_eq!(inbox_item_before.workspace_id, None, "Item must start in Inbox (workspace_id = None)");

        // Execute AI Context Reasoning
        let result = analyze_item_context_ai(&conn, &inbox_item.id, &reasoner).unwrap();
        assert!(result.suggested_context_reference.is_some());

        // Query state counts AFTER AI reasoning
        let items_count_after: i64 = conn.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0)).unwrap();
        let workspaces_count_after: i64 = conn.query_row("SELECT COUNT(*) FROM workspaces", [], |r| r.get(0)).unwrap();
        let memberships_count_after: i64 = conn.query_row("SELECT COUNT(*) FROM context_memberships", [], |r| r.get(0)).unwrap();
        let signals_count_after: i64 = conn.query_row("SELECT COUNT(*) FROM context_signals", [], |r| r.get(0)).unwrap();
        let candidates_count_after: i64 = conn.query_row("SELECT COUNT(*) FROM context_candidates", [], |r| r.get(0)).unwrap();

        let inbox_item_after = operations::get_item(&conn, &inbox_item.id).unwrap();

        // INVARIANT ASSERTIONS:
        assert_eq!(items_count_before, items_count_after, "AI reasoning must NOT create or delete items");
        assert_eq!(workspaces_count_before, workspaces_count_after, "AI reasoning must NOT create or delete workspaces");
        assert_eq!(memberships_count_before, memberships_count_after, "AI reasoning must NOT create or modify memberships");
        assert_eq!(signals_count_before, signals_count_after, "AI reasoning must NOT insert or alter signals");
        assert_eq!(candidates_count_before, candidates_count_after, "AI reasoning must NOT insert or alter candidate records");

        // The item must STILL be in Inbox (workspace_id = None)
        assert_eq!(inbox_item_after.workspace_id, None, "AI reasoning must NOT move the item or assign a workspace");
        assert_eq!(inbox_item_before.content, inbox_item_after.content);
        assert_eq!(inbox_item_before.updated_at, inbox_item_after.updated_at);
    }

    #[test]
    fn test_builder_excludes_raw_embeddings() {
        let (conn, _db_path) = setup_test_db();
        let ws = operations::create_workspace(&conn, "ML Research").unwrap();

        // Create an item with a raw embedding
        let item = operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Deep learning transformers paper notes",
            Some("Paper Notes"),
            "{}",
        ).unwrap();

        // Store 384-dimensional raw embedding in database for item
        let raw_vector = vec![0.1234_f32; 384];
        operations::save_item_embedding(&conn, &item.id, &raw_vector, "local", "all-MiniLM-L6-v2").unwrap();

        // Calculate workspace profile embedding in database from member items
        operations::recalculate_workspace_profile(&conn, &ws.id).unwrap();

        // Build EvidencePackage using convenience builder function
        let package = build_evidence_package(&conn, &item.id).unwrap();

        // Serialize to JSON string and JSON value
        let json_str = serde_json::to_string(&package).unwrap();
        let json_val: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        // Recursively verify NO raw vector embedding or float array exists
        fn assert_no_embeddings(v: &serde_json::Value) {
            match v {
                serde_json::Value::Object(map) => {
                    for (k, child) in map {
                        assert_ne!(k, "embedding", "Raw 'embedding' field must NOT exist in EvidencePackage JSON");
                        assert_ne!(k, "embeddings", "Raw 'embeddings' field must NOT exist in EvidencePackage JSON");
                        assert_ne!(k, "vector", "Raw 'vector' field must NOT exist in EvidencePackage JSON");
                        assert_no_embeddings(child);
                    }
                }
                serde_json::Value::Array(arr) => {
                    if arr.len() > 20 && arr.iter().all(|x| x.is_number()) {
                        panic!("Found unexpected raw vector numeric array in EvidencePackage JSON!");
                    }
                    for child in arr {
                        assert_no_embeddings(child);
                    }
                }
                _ => {}
            }
        }

        assert_no_embeddings(&json_val);

        // Verify that workspace semantic profile is a human summary with scalar score only
        assert!(!package.workspace_semantic_profiles.is_empty());
        let prof = &package.workspace_semantic_profiles[0];
        assert!(prof.has_profile);
        assert!(prof.alignment_score.is_some());
        assert_eq!(prof.alignment_band, Some("Strong".to_string()));
        assert!(prof.theme_summary.contains("Aligns strongly with the overall semantic profile of ML Research."));
    }

    #[test]
    fn test_builder_excludes_unnecessary_private_information() {
        let (conn, _db_path) = setup_test_db();

        let sensitive_content = "Login credentials: password: MySuperSecretPass! and Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9 and AWS AKIAIOSFODNN7EXAMPLE and sk-live1234567890abcdef123456 and CC 4532-1234-5678-9012.";

        let item = operations::create_item(
            &conn,
            None,
            None,
            "text",
            sensitive_content,
            Some("Sensitive Credentials Note"),
            "{}",
        ).unwrap();

        let package = EvidencePackageBuilder::default().build(&conn, &item.id).unwrap();

        let snippet = &package.target_item.content_snippet;
        assert!(!snippet.contains("MySuperSecretPass!"), "Passwords must be scrubbed");
        assert!(!snippet.contains("AKIAIOSFODNN7EXAMPLE"), "AWS keys must be scrubbed");
        assert!(!snippet.contains("sk-live1234567890abcdef123456"), "API keys must be scrubbed");
        assert!(!snippet.contains("4532-1234-5678-9012"), "Credit card numbers must be scrubbed");
        assert!(snippet.contains("[REDACTED]"), "Must contain redaction placeholder");
    }

    #[test]
    fn test_builder_excludes_irrelevant_database_items() {
        let (conn, _db_path) = setup_test_db();

        let ws_relevant = operations::create_workspace(&conn, "Relevant WS").unwrap();
        let ws_unrelated = operations::create_workspace(&conn, "Unrelated WS").unwrap();

        let item_rel = operations::create_item(
            &conn,
            Some(&ws_relevant.id),
            None,
            "text",
            "Related item content",
            Some("Related Doc"),
            "{}",
        ).unwrap();

        for i in 1..=15 {
            let ws_target = if i % 2 == 0 { Some(ws_unrelated.id.as_str()) } else { None };
            operations::create_item(
                &conn,
                ws_target,
                None,
                "text",
                &format!("Unrelated item content {}", i),
                Some(&format!("Unrelated {}", i)),
                "{}",
            ).unwrap();
        }

        let target_item = operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Target item needing context",
            Some("Target"),
            "{}",
        ).unwrap();

        operations::create_item_relationship(
            &conn,
            &target_item.id,
            &item_rel.id,
            "semantic_similarity",
            0.92,
            None,
        ).unwrap();

        let builder = EvidencePackageBuilder::default().with_max_related_items(5);
        let package = builder.build(&conn, &target_item.id).unwrap();

        assert_eq!(package.relevant_related_items.len(), 1);
        assert_eq!(package.relevant_related_items[0].item_id, item_rel.id);
        assert_eq!(package.system_metadata.total_db_items_excluded, 15);
    }

    #[test]
    fn test_builder_excludes_internal_database_ids() {
        let (conn, _db_path) = setup_test_db();
        let ws = operations::create_workspace(&conn, "App WS").unwrap();

        let item = operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Testing signal stripping",
            Some("Title"),
            "{}",
        ).unwrap();

        crate::context_engine::operations::create_context_signal(
            &conn,
            Some(&ws.id),
            Some(&item.id),
            "active_app",
            None,
            &json!({
                "app_name": "Terminal",
                "process_id": 99999,
                "window_title": "zsh (top secret path /home/user/passwords)",
                "raw_window_id": 12345678
            }).to_string(),
        ).unwrap();

        crate::context_engine::operations::create_context_signal(
            &conn,
            Some(&ws.id),
            Some(&item.id),
            "url_metadata",
            None,
            &json!({
                "domain": "github.com",
                "full_url": "https://github.com/secret/repo?token=secret12345",
                "internal_tracker": "track_98765"
            }).to_string(),
        ).unwrap();

        let package = EvidencePackageBuilder::default().build(&conn, &item.id).unwrap();
        let signals_json = serde_json::to_string(&package.contextual_signals).unwrap();

        assert!(!signals_json.contains("process_id"), "Internal process_id must be stripped");
        assert!(!signals_json.contains("99999"), "Process ID value must be stripped");
        assert!(!signals_json.contains("raw_window_id"), "Raw window handle must be stripped");
        assert!(!signals_json.contains("token=secret12345"), "Query parameter tokens in URLs must be stripped");
        assert!(!signals_json.contains("internal_tracker"), "Internal trackers must be stripped");

        assert!(signals_json.contains("Application: Terminal"));
        assert!(signals_json.contains("Domain: github.com"));
    }

    #[test]
    fn test_builder_deterministic_and_reproducible() {
        let (conn, _db_path) = setup_test_db();
        let ws = operations::create_workspace(&conn, "Deterministic WS").unwrap();

        let item = operations::create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Deterministic context evaluation item",
            Some("Item"),
            "{}",
        ).unwrap();

        let builder = EvidencePackageBuilder::default();
        let pkg1 = builder.build(&conn, &item.id).unwrap();
        let pkg2 = builder.build(&conn, &item.id).unwrap();

        assert_eq!(pkg1.target_item, pkg2.target_item);
        assert_eq!(pkg1.candidate_contexts, pkg2.candidate_contexts);
        assert_eq!(pkg1.relevant_related_items, pkg2.relevant_related_items);
        assert_eq!(pkg1.relationships, pkg2.relationships);
        assert_eq!(pkg1.semantic_similarity_summaries, pkg2.semantic_similarity_summaries);
        assert_eq!(pkg1.relevant_sessions, pkg2.relevant_sessions);
        assert_eq!(pkg1.workspace_semantic_profiles, pkg2.workspace_semantic_profiles);
        assert_eq!(pkg1.contextual_signals, pkg2.contextual_signals);
        assert_eq!(pkg1.temporal_context, pkg2.temporal_context);
        assert_eq!(pkg1.system_metadata.total_db_items_excluded, pkg2.system_metadata.total_db_items_excluded);
    }

    #[test]
    fn test_scrub_private_data_patterns() {
        assert_eq!(
            scrub_private_data("Authorization: Bearer mySecretToken12345!"),
            "Authorization: Bearer [REDACTED]!"
        );
        assert_eq!(
            scrub_private_data("JWT eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.doNotLeakThis"),
            "JWT [JWT_REDACTED]"
        );
        assert_eq!(
            scrub_private_data("AWS access: AKIAIOSFODNN7EXAMPLE in config"),
            "AWS access: [AWS_KEY_REDACTED] in config"
        );
        assert_eq!(
            scrub_private_data("password: myPassword123"),
            "password: [REDACTED]"
        );
    }

    // =======================================================================
    // STRICT SCHEMA VALIDATION TESTS
    // =======================================================================

    #[test]
    fn test_validation_valid_decisions() {
        // 1. Valid existing_context
        let valid_existing = ContextReasoningResult {
            decision: ContextReasoningDecision::ExistingContext,
            suggested_context_reference: Some("ws_paris".to_string()),
            suggested_name: Some("Paris Trip".to_string()),
            confidence: 0.85,
            explanation: "Item has high semantic overlap with Paris Trip.".to_string(),
            evidence_references: vec!["3 semantic matches".to_string()],
            uncertainty_reason: None,
            provider_info: None,
            alternative_interpretations: vec![],
        };
        assert!(validate_reasoning_result(&valid_existing).is_ok());

        // 2. Valid new_context
        let valid_new = ContextReasoningResult {
            decision: ContextReasoningDecision::NewContext,
            suggested_context_reference: Some("cand_fitness".to_string()),
            suggested_name: Some("Fitness Training".to_string()),
            confidence: 0.75,
            explanation: "Items form an independent workout activity cluster.".to_string(),
            evidence_references: vec!["Shared workout session".to_string()],
            uncertainty_reason: None,
            provider_info: None,
            alternative_interpretations: vec![],
        };
        assert!(validate_reasoning_result(&valid_new).is_ok());

        // 3. Valid uncertain
        let valid_uncertain = ContextReasoningResult {
            decision: ContextReasoningDecision::Uncertain,
            suggested_context_reference: Some("ws_work".to_string()),
            suggested_name: Some("Work".to_string()),
            confidence: 0.50,
            explanation: "Competing signals between Work and Personal.".to_string(),
            evidence_references: vec!["Work email domain".to_string(), "Personal note content".to_string()],
            uncertainty_reason: Some("Signals conflict between two distinct workspaces.".to_string()),
            provider_info: None,
            alternative_interpretations: vec![],
        };
        assert!(validate_reasoning_result(&valid_uncertain).is_ok());

        // 4. Valid no_context
        let valid_no_context = ContextReasoningResult {
            decision: ContextReasoningDecision::NoContext,
            suggested_context_reference: None,
            suggested_name: None,
            confidence: 0.10,
            explanation: "Zero contextual signals detected; item remains in Inbox.".to_string(),
            evidence_references: vec!["No candidate contexts".to_string()],
            uncertainty_reason: Some("Insufficient evidence for association.".to_string()),
            provider_info: None,
            alternative_interpretations: vec![],
        };
        assert!(validate_reasoning_result(&valid_no_context).is_ok());
    }

    #[test]
    fn test_validation_rejects_invalid_confidence() {
        let mut res = ContextReasoningResult {
            decision: ContextReasoningDecision::NoContext,
            suggested_context_reference: None,
            suggested_name: None,
            confidence: 1.5, // > 1.0 invalid!
            explanation: "Explanation".to_string(),
            evidence_references: vec![],
            uncertainty_reason: None,
            provider_info: None,
            alternative_interpretations: vec![],
        };
        assert!(matches!(
            validate_reasoning_result(&res),
            Err(ReasoningValidationError::InvalidConfidence(_))
        ));

        res.confidence = -0.1; // < 0.0 invalid!
        assert!(matches!(
            validate_reasoning_result(&res),
            Err(ReasoningValidationError::InvalidConfidence(_))
        ));

        res.confidence = f64::NAN; // NaN invalid!
        assert!(matches!(
            validate_reasoning_result(&res),
            Err(ReasoningValidationError::InvalidConfidence(_))
        ));
    }

    #[test]
    fn test_validation_rejects_missing_context_reference_for_existing_context() {
        let res = ContextReasoningResult {
            decision: ContextReasoningDecision::ExistingContext,
            suggested_context_reference: None, // Missing!
            suggested_name: Some("Workspace".to_string()),
            confidence: 0.8,
            explanation: "Valid explanation".to_string(),
            evidence_references: vec![],
            uncertainty_reason: None,
            provider_info: None,
            alternative_interpretations: vec![],
        };
        assert!(matches!(
            validate_reasoning_result(&res),
            Err(ReasoningValidationError::MissingContextReference(_))
        ));
    }

    #[test]
    fn test_validation_rejects_missing_suggested_name_for_new_context() {
        let res = ContextReasoningResult {
            decision: ContextReasoningDecision::NewContext,
            suggested_context_reference: Some("cand_1".to_string()),
            suggested_name: None, // Missing!
            confidence: 0.8,
            explanation: "Valid explanation".to_string(),
            evidence_references: vec![],
            uncertainty_reason: None,
            provider_info: None,
            alternative_interpretations: vec![],
        };
        assert!(matches!(
            validate_reasoning_result(&res),
            Err(ReasoningValidationError::MissingSuggestedName(_))
        ));
    }

    #[test]
    fn test_validation_rejects_missing_uncertainty_reason_for_uncertain() {
        let res = ContextReasoningResult {
            decision: ContextReasoningDecision::Uncertain,
            suggested_context_reference: Some("ws_1".to_string()),
            suggested_name: Some("Workspace".to_string()),
            confidence: 0.5,
            explanation: "Valid explanation".to_string(),
            evidence_references: vec![],
            uncertainty_reason: None, // Missing for uncertain!
            provider_info: None,
            alternative_interpretations: vec![],
        };
        assert!(matches!(
            validate_reasoning_result(&res),
            Err(ReasoningValidationError::MissingUncertaintyReason(_))
        ));
    }

    #[test]
    fn test_validation_rejects_empty_explanation() {
        let res = ContextReasoningResult {
            decision: ContextReasoningDecision::NoContext,
            suggested_context_reference: None,
            suggested_name: None,
            confidence: 0.1,
            explanation: "   ".to_string(), // Empty whitespace!
            evidence_references: vec![],
            uncertainty_reason: None,
            provider_info: None,
            alternative_interpretations: vec![],
        };
        assert!(matches!(
            validate_reasoning_result(&res),
            Err(ReasoningValidationError::EmptyExplanation(_))
        ));
    }

    #[test]
    fn test_validation_rejects_executable_instructions() {
        // 1. SQL Injection in explanation
        let res_sql = ContextReasoningResult {
            decision: ContextReasoningDecision::ExistingContext,
            suggested_context_reference: Some("ws_1".to_string()),
            suggested_name: Some("Workspace".to_string()),
            confidence: 0.9,
            explanation: "I recommend DROP TABLE items; to clean up state.".to_string(),
            evidence_references: vec![],
            uncertainty_reason: None,
            provider_info: None,
            alternative_interpretations: vec![],
        };
        assert!(matches!(
            validate_reasoning_result(&res_sql),
            Err(ReasoningValidationError::ExecutableInstructionsDetected(_))
        ));

        // 2. Destructive shell command in suggested_name
        let res_shell = ContextReasoningResult {
            decision: ContextReasoningDecision::NewContext,
            suggested_context_reference: Some("cand_1".to_string()),
            suggested_name: Some("Project rm -rf /".to_string()),
            confidence: 0.8,
            explanation: "New project discovered.".to_string(),
            evidence_references: vec![],
            uncertainty_reason: None,
            provider_info: None,
            alternative_interpretations: vec![],
        };
        assert!(matches!(
            validate_reasoning_result(&res_shell),
            Err(ReasoningValidationError::ExecutableInstructionsDetected(_))
        ));

        // 3. HTML/Script tag in evidence reference
        let res_script = ContextReasoningResult {
            decision: ContextReasoningDecision::ExistingContext,
            suggested_context_reference: Some("ws_1".to_string()),
            suggested_name: Some("Workspace".to_string()),
            confidence: 0.8,
            explanation: "Valid explanation".to_string(),
            evidence_references: vec!["<script>alert('pwn')</script>".to_string()],
            uncertainty_reason: None,
            provider_info: None,
            alternative_interpretations: vec![],
        };
        assert!(matches!(
            validate_reasoning_result(&res_script),
            Err(ReasoningValidationError::ExecutableInstructionsDetected(_))
        ));

        // 4. Programmatic execution call (eval)
        let res_eval = ContextReasoningResult {
            decision: ContextReasoningDecision::Uncertain,
            suggested_context_reference: Some("ws_1".to_string()),
            suggested_name: Some("Workspace".to_string()),
            confidence: 0.5,
            explanation: "Run eval(payload) to decide".to_string(),
            evidence_references: vec![],
            uncertainty_reason: Some("Conflicting".to_string()),
            provider_info: None,
            alternative_interpretations: vec![],
        };
        assert!(matches!(
            validate_reasoning_result(&res_eval),
            Err(ReasoningValidationError::ExecutableInstructionsDetected(_))
        ));
    }

    #[test]
    fn test_parse_and_validate_response_json() {
        // Valid JSON string
        let valid_json = r#"{
            "decision": "existing_context",
            "suggested_context_reference": "ws_123",
            "suggested_name": "Project Apollo",
            "confidence": 0.88,
            "explanation": "High correlation with Apollo project documents.",
            "evidence_references": ["Shared session", "Common domain"],
            "uncertainty_reason": null
        }"#;
        let parsed = parse_and_validate_response(valid_json).unwrap();
        assert_eq!(parsed.decision, ContextReasoningDecision::ExistingContext);
        assert_eq!(parsed.suggested_context_reference, Some("ws_123".to_string()));

        // Malformed JSON string
        let malformed_json = r#"{ "decision": "existing_context", "#;
        assert!(matches!(
            parse_and_validate_response(malformed_json),
            Err(ReasoningValidationError::MalformedJson(_))
        ));

        // Unknown decision variant
        let unknown_decision = r#"{
            "decision": "destroy_all",
            "suggested_context_reference": "ws_1",
            "confidence": 0.5,
            "explanation": "Unknown variant",
            "evidence_references": []
        }"#;
        assert!(matches!(
            parse_and_validate_response(unknown_decision),
            Err(ReasoningValidationError::MalformedJson(_))
        ));
    }
}

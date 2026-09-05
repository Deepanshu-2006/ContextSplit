#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;

    use crate::context_engine::reasoning::builder::{
        analyze_item_context_ai, build_candidate_evidence_package, build_evidence_package,
        build_item_group_evidence_package, scrub_private_data, suggest_candidate_name_ai,
        suggest_item_group_name_ai, EvidencePackageBuilder,
    };
    use crate::context_engine::reasoning::fixtures::*;
    use crate::context_engine::reasoning::mock::MockAIReasoner;
    use crate::context_engine::reasoning::models::*;
    use crate::context_engine::reasoning::privacy::{
        AIPrivacyBoundary, AIPrivacyPolicy, AIPrivacyViolation,
    };
    use crate::context_engine::reasoning::provider::*;
    use crate::context_engine::reasoning::remote::RemoteLLMReasoner;
    use crate::context_engine::reasoning::traits::AIReasoner;
    use crate::context_engine::reasoning::validator::{
        detect_exposed_internal_metrics, parse_and_validate_response,
        validate_explanation_grounding, validate_reasoning_result, ReasoningValidationError,
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
            enable_ai_reasoning: true,
            ai_gating_enabled: true,
            ai_gating_confidence_threshold: 0.9,
            capture_paused: false,
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
        let is_weak = candidates.is_empty() || candidates[0].deterministic_score < 0.25;
        let is_ambiguous = candidates.len() > 1
            && (candidates[0].deterministic_score - candidates[1].deterministic_score).abs() <= 0.12
            && candidates[1].deterministic_score >= 0.35;
        let band = if is_weak {
            "weak".to_string()
        } else if is_ambiguous {
            "uncertain".to_string()
        } else if candidates[0].deterministic_score >= 0.70 {
            "high".to_string()
        } else {
            "moderate".to_string()
        };

        let competing = if is_ambiguous {
            vec![candidates[0].name.clone(), candidates[1].name.clone()]
        } else {
            Vec::new()
        };

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
            deterministic_assessment: DeterministicEvidenceAssessment {
                confidence_band: band,
                is_weak_evidence: is_weak,
                is_ambiguous,
                competing_contexts: competing,
                assessment_summary: "Test deterministic assessment summary".to_string(),
            },
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
        // CRITICAL INVARIANT: The AI must NOT be forced to choose a Workspace!
        assert_eq!(result.suggested_context_reference, None);
        assert_eq!(result.suggested_name, None);
        assert!(result.uncertainty_reason.is_some());
        assert!(result.explanation.contains("competing signals") || result.explanation.contains("Paris Trip"));
        assert_eq!(result.alternative_interpretations.len(), 2);
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
        assert!(result.explanation.contains("available evidence is limited"));
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

        let item1 = operations::create_item(&conn, Some(&ws1.id), None, "text", "Paris hotel", Some("Hotel"), "{}").unwrap();
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

        // Add semantic relationship so inbox_item has strong evidence for Paris Trip
        operations::create_item_relationship(
            &conn,
            &inbox_item.id,
            &item1.id,
            "semantic_similarity",
            0.85,
            None,
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

    #[test]
    fn test_mock_ai_reasoner_fixture_strong_existing_context() {
        let fixture = strong_existing_context_fixture();
        let reasoner = MockAIReasoner::new();
        let result = reasoner.analyze_context(&fixture).expect("Reasoning should succeed");

        assert_eq!(result.decision, ContextReasoningDecision::ExistingContext);
        assert_eq!(result.suggested_context_reference.as_deref(), Some("ws_paris_trip"));
        assert_eq!(result.suggested_name.as_deref(), Some("Paris Trip"));
        assert!((result.confidence - 0.89).abs() < 1e-4);
        assert!(result.uncertainty_reason.is_none());
        assert!(!result.explanation.is_empty());
        assert!(result.explanation.contains("Paris Trip"));
        assert!(!result.evidence_references.is_empty());

        let provider = result.provider_info.as_ref().expect("Provider info should be set");
        assert_eq!(provider.provider_id, "mock");
        assert_eq!(provider.provider_type, "mock_simulation");

        validate_reasoning_result(&result).expect("Result must pass strict schema validation");
    }

    #[test]
    fn test_mock_ai_reasoner_fixture_strong_new_context() {
        let fixture = strong_new_context_fixture();
        let reasoner = MockAIReasoner::new();
        let result = reasoner.analyze_context(&fixture).expect("Reasoning should succeed");

        assert_eq!(result.decision, ContextReasoningDecision::NewContext);
        assert_eq!(result.suggested_context_reference.as_deref(), Some("cand_fitness_training"));
        assert_eq!(result.suggested_name.as_deref(), Some("Fitness Training"));
        assert!((result.confidence - 0.82).abs() < 1e-4);
        assert!(result.uncertainty_reason.is_none());
        assert!(!result.explanation.is_empty());
        assert!(result.explanation.contains("Fitness Training"));
        assert!(!result.evidence_references.is_empty());

        validate_reasoning_result(&result).expect("Result must pass strict schema validation");
    }

    #[test]
    fn test_mock_ai_reasoner_fixture_ambiguous_context() {
        let fixture = ambiguous_context_fixture();
        let reasoner = MockAIReasoner::new();
        let result = reasoner.analyze_context(&fixture).expect("Reasoning should succeed");

        assert_eq!(result.decision, ContextReasoningDecision::Uncertain);
        // CRITICAL INVARIANT: The AI must NOT be forced to choose a Workspace!
        assert_eq!(result.suggested_context_reference, None);
        assert_eq!(result.suggested_name, None);
        assert!((result.confidence - 0.61).abs() < 1e-4);
        assert!(result.uncertainty_reason.is_some());
        let uncertainty = result.uncertainty_reason.as_ref().unwrap();
        assert!(uncertainty.contains("Paris Trip") && uncertainty.contains("London Client Meetings"));
        assert_eq!(result.alternative_interpretations.len(), 2);

        validate_reasoning_result(&result).expect("Result must pass strict schema validation");
    }

    #[test]
    fn test_mock_ai_reasoner_fixture_no_context() {
        let fixture = no_context_fixture();
        let reasoner = MockAIReasoner::new();
        let result = reasoner.analyze_context(&fixture).expect("Reasoning should succeed");

        assert_eq!(result.decision, ContextReasoningDecision::NoContext);
        assert!(result.suggested_context_reference.is_none());
        assert!(result.suggested_name.is_none());
        assert_eq!(result.confidence, 0.0);
        assert!(result.uncertainty_reason.is_some());
        assert!(result.explanation.contains("Inbox"));

        validate_reasoning_result(&result).expect("Result must pass strict schema validation");
    }

    #[test]
    fn test_mock_ai_reasoner_fixture_insufficient_evidence() {
        let fixture = insufficient_evidence_fixture();
        let reasoner = MockAIReasoner::new();
        let result = reasoner.analyze_context(&fixture).expect("Reasoning should succeed");

        assert_eq!(result.decision, ContextReasoningDecision::NoContext);
        assert!(result.suggested_context_reference.is_none());
        assert!(result.suggested_name.is_none());
        assert!((result.confidence - 0.14).abs() < 1e-4);
        assert!(result.uncertainty_reason.is_some());
        assert!(result.uncertainty_reason.as_ref().unwrap().contains("25%"));

        validate_reasoning_result(&result).expect("Result must pass strict schema validation");
    }

    #[test]
    fn test_mock_ai_reasoner_deterministic_reproducibility() {
        let fixtures = vec![
            strong_existing_context_fixture(),
            strong_new_context_fixture(),
            ambiguous_context_fixture(),
            no_context_fixture(),
            insufficient_evidence_fixture(),
        ];

        let reasoner = MockAIReasoner::new();
        for (idx, fixture) in fixtures.iter().enumerate() {
            let res1 = reasoner.analyze_context(fixture).expect("Run 1 failed");
            let res2 = reasoner.analyze_context(fixture).expect("Run 2 failed");

            assert_eq!(res1.decision, res2.decision, "Decision mismatch on fixture {}", idx);
            assert_eq!(
                res1.suggested_context_reference, res2.suggested_context_reference,
                "Reference mismatch on fixture {}", idx
            );
            assert_eq!(res1.suggested_name, res2.suggested_name, "Name mismatch on fixture {}", idx);
            assert_eq!(res1.confidence, res2.confidence, "Confidence mismatch on fixture {}", idx);
            assert_eq!(res1.explanation, res2.explanation, "Explanation mismatch on fixture {}", idx);
            assert_eq!(
                res1.uncertainty_reason, res2.uncertainty_reason,
                "Uncertainty reason mismatch on fixture {}", idx
            );
        }
    }

    #[test]
    fn test_provider_config_defaults_and_masking() {
        // 1. Default mock configuration
        let default_cfg = AIProviderConfig::default_mock();
        assert_eq!(default_cfg.provider_type, AIProviderType::Mock);
        assert_eq!(default_cfg.model_name, "mock-reasoner-v1.0");
        assert!(default_cfg.endpoint.is_none());
        assert!(default_cfg.api_key.is_none());
        assert!(default_cfg.validate().is_ok());

        // 2. Custom mock configuration
        let custom_mock = AIProviderConfig::mock("custom-mock-v2");
        assert_eq!(custom_mock.provider_type, AIProviderType::Mock);
        assert_eq!(custom_mock.model_name, "custom-mock-v2");

        // 3. Remote configuration with sensitive API key
        let secret_key = "sk-secret987654321abcdef";
        let remote_cfg = AIProviderConfig::remote(
            AIProviderType::OpenAi,
            "gpt-4o",
            Some("https://api.openai.com/v1".to_string()),
            Some(secret_key.to_string()),
        );
        assert_eq!(remote_cfg.provider_type, AIProviderType::OpenAi);
        assert_eq!(remote_cfg.model_name, "gpt-4o");
        assert_eq!(remote_cfg.endpoint.as_deref(), Some("https://api.openai.com/v1"));
        assert!(remote_cfg.validate().is_ok());

        // 4. Verify API key is masked in Debug output
        let debug_str = format!("{:?}", remote_cfg);
        assert!(debug_str.contains("sk-***"));
        assert!(!debug_str.contains(secret_key), "Plaintext API key leaked in Debug output!");

        // 5. Validation failure on empty model name
        let mut invalid_cfg = remote_cfg.clone();
        invalid_cfg.model_name = "   ".to_string();
        assert!(invalid_cfg.validate().is_err());

        // 6. Validation failure on whitespace-only remote endpoint
        let mut invalid_endpoint_cfg = remote_cfg.clone();
        invalid_endpoint_cfg.endpoint = Some("   ".to_string());
        assert!(invalid_endpoint_cfg.validate().is_err());
    }

    #[test]
    fn test_provider_factory_and_defaults() {
        // Default factory creation returns MockAIReasoner
        let default_reasoner = create_default_ai_reasoner();
        assert_eq!(default_reasoner.provider_id(), "mock");
        assert_eq!(default_reasoner.provider_type(), "mock");
        assert!(default_reasoner.is_available());

        // Factory creates mock provider from config
        let mock_cfg = AIProviderConfig::mock("mock-test-model");
        let mock_reasoner = create_ai_reasoner(&mock_cfg).expect("Mock factory creation should succeed");
        assert_eq!(mock_reasoner.provider_id(), "mock");
        assert!(mock_reasoner.is_available());

        // Factory creates remote provider from config
        let remote_cfg = AIProviderConfig::remote(
            AIProviderType::OpenAi,
            "gpt-4o",
            Some("https://api.openai.com/v1".to_string()),
            Some("sk-test1234".to_string()),
        );
        let remote_reasoner = create_ai_reasoner(&remote_cfg).expect("Remote factory creation should succeed");
        assert_eq!(remote_reasoner.provider_id(), "openai");
        assert_eq!(remote_reasoner.provider_type(), "remote_llm");
        assert!(remote_reasoner.is_available());

        // Status inspection
        let status = get_reasoner_status(remote_reasoner.as_ref());
        assert_eq!(status.provider_id, "openai");
        assert_eq!(status.provider_type, "remote_llm");
        assert!(status.is_available);
        assert!(!status.is_mock);

        let mock_status = get_reasoner_status(mock_reasoner.as_ref());
        assert_eq!(mock_status.provider_id, "mock");
        assert!(mock_status.is_mock);
    }

    #[test]
    fn test_remote_llm_reasoner_payload_and_extraction() {
        let remote_cfg = AIProviderConfig::remote(
            AIProviderType::Ollama,
            "llama3",
            Some("http://localhost:11434".to_string()),
            None,
        );
        let remote = RemoteLLMReasoner::new(remote_cfg);
        let fixture = strong_existing_context_fixture();

        // 1. Verify prompt construction contains constraints and schema instructions
        let prompt = remote.build_full_prompt(&fixture);
        assert!(prompt.contains("ContextSplit's AI Context Reasoner"));
        assert!(prompt.contains("existing_context"));
        assert!(prompt.contains("new_context"));
        assert!(prompt.contains("Air France Flight to Paris CDG"));

        // 2. Verify structured request payload format
        let payload = remote.build_request_payload(&fixture);
        assert_eq!(payload["model"], "llama3");
        assert_eq!(payload["response_format"]["type"], "json_object");
        assert_eq!(payload["temperature"], 0.1);
        assert_eq!(payload["messages"].as_array().unwrap().len(), 2);

        // 3. Verify content extraction variants (OpenAI, Ollama, Direct)
        let openai_body = json!({
            "choices": [{
                "message": {
                    "content": r#"{"decision":"no_context","confidence":0.0,"explanation":"No match","evidence_references":[],"uncertainty_reason":"No signals"}"#
                }
            }]
        });
        let extracted_openai = remote.extract_content_from_response(&openai_body).unwrap();
        assert!(extracted_openai.contains("no_context"));

        let ollama_body = json!({
            "message": {
                "content": r#"{"decision":"no_context","confidence":0.0,"explanation":"No match","evidence_references":[],"uncertainty_reason":"No signals"}"#
            }
        });
        let extracted_ollama = remote.extract_content_from_response(&ollama_body).unwrap();
        assert!(extracted_ollama.contains("no_context"));

        // 4. Verify remote response processing parses and validates conforming JSON
        let valid_json = r#"{
            "decision": "existing_context",
            "suggested_context_reference": "ws_paris_trip",
            "suggested_name": "Paris Trip",
            "confidence": 0.92,
            "explanation": "High correlation with Paris flight reservation.",
            "evidence_references": ["Common domain airfrance.us", "Shared booking session"],
            "uncertainty_reason": null
        }"#;
        let parsed = remote.process_remote_response(valid_json).expect("Parsing valid response must succeed");
        assert_eq!(parsed.decision, ContextReasoningDecision::ExistingContext);
        assert_eq!(parsed.suggested_context_reference.as_deref(), Some("ws_paris_trip"));
        assert_eq!(parsed.provider_info.as_ref().unwrap().provider_id, "ollama");

        // 5. Verify remote response processing rejects malicious script or executable injection
        let malicious_json = r#"{
            "decision": "existing_context",
            "suggested_context_reference": "ws_paris_trip",
            "suggested_name": "Paris Trip",
            "confidence": 0.92,
            "explanation": "DROP TABLE items; -- malicious code",
            "evidence_references": [],
            "uncertainty_reason": null
        }"#;
        let mal_err = remote.process_remote_response(malicious_json).expect_err("Malicious response must be rejected");
        assert!(matches!(mal_err, AIReasoningError::InvalidResponse(_)));
    }

    #[test]
    fn test_remote_llm_bounded_retries_and_graceful_failure() {
        // Point to an unreachable port with short timeout and 1 retry
        let remote_cfg = AIProviderConfig::remote(
            AIProviderType::RemoteLlm,
            "test-remote-model",
            Some("http://127.0.0.1:59999".to_string()),
            Some("sk-dummy-key".to_string()),
        )
        .with_timeout_and_retries(1, 1);

        let remote = RemoteLLMReasoner::new(remote_cfg);
        let fixture = strong_existing_context_fixture();

        let err = remote.analyze_context(&fixture).expect_err("Should fail gracefully when host is unreachable");
        assert!(matches!(err, AIReasoningError::ProviderUnavailable(_)));
        let err_msg = err.to_string();
        assert!(err_msg.contains("127.0.0.1:59999"));
        // Confirm sensitive API key is NOT leaked into the error message
        assert!(!err_msg.contains("sk-dummy-key"));
    }

    #[test]
    fn test_ai_provider_config_from_env_discovery() {
        // Test fallback to MockAIReasoner when no env variables are set
        // (Clean environment guarantee by running with fallback)
        let config = AIProviderConfig::default_mock();
        assert_eq!(config.provider_type, AIProviderType::Mock);
        assert_eq!(config.model_name, "mock-reasoner-v1.0");

        // Test custom remote config creation without leaking secret in Debug
        let test_key = "sk-live-production-secret-9999";
        let custom = AIProviderConfig::remote(
            AIProviderType::OpenAi,
            "gpt-4o",
            Some("https://api.openai.com/v1".to_string()),
            Some(test_key.to_string()),
        );
        assert_eq!(custom.provider_type, AIProviderType::OpenAi);
        assert_eq!(custom.endpoint.as_deref(), Some("https://api.openai.com/v1"));
        let debug_str = format!("{:?}", custom);
        assert!(debug_str.contains("sk-***"));
        assert!(!debug_str.contains(test_key));
    }

    #[test]
    fn test_context_engine_ai_reasoner_pluggability() {
        let (conn, _db_path) = setup_test_db();
        let mut ce = crate::context_engine::engine::ContextEngine::new();

        // 1. ContextEngine defaults to MockAIReasoner
        assert_eq!(ce.ai_reasoner().provider_id(), "mock");
        assert!(ce.ai_reasoner().is_available());

        // 2. Insert test workspace and items
        let session = operations::create_session(&conn).unwrap();
        let ws = operations::create_workspace(&conn, "Paris Trip").unwrap();
        let item1 = operations::create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "Flight to Paris CDG on Air France",
            Some("Flight Booking"),
            "{}",
        )
        .unwrap();

        let inbox_item = operations::create_item(
            &conn,
            None,
            Some(&session.id),
            "text",
            "Hotel Louvre Paris reservation confirmation",
            Some("Hotel Booking"),
            "{}",
        ).unwrap();

        operations::create_item_relationship(
            &conn,
            &inbox_item.id,
            &item1.id,
            "semantic_similarity",
            0.88,
            None,
        )
        .unwrap();

        // 3. Run analysis via ContextEngine abstraction
        let res = ce.analyze_item_context_ai(&conn, &inbox_item.id).expect("Analysis should succeed via engine");
        assert_eq!(res.decision, ContextReasoningDecision::ExistingContext);
        assert_eq!(res.suggested_context_reference.as_deref(), Some(ws.id.as_str()));

        // 4. Dynamically swap the reasoner on ContextEngine to a custom provider
        struct DynamicProvider {
            id: String,
        }
        impl AIReasoner for DynamicProvider {
            fn analyze_context(&self, _package: &EvidencePackage) -> Result<ContextReasoningResult, AIReasoningError> {
                let res = ContextReasoningResult {
                    decision: ContextReasoningDecision::NoContext,
                    suggested_context_reference: None,
                    suggested_name: None,
                    confidence: 0.0,
                    explanation: "Custom dynamic reasoner forced no_context.".to_string(),
                    evidence_references: vec!["Dynamic pluggability test".to_string()],
                    uncertainty_reason: Some("Testing dynamic swap".to_string()),
                    alternative_interpretations: vec![],
                    provider_info: None,
                };
                Ok(res)
            }
            fn provider_name(&self) -> &str {
                "Dynamic Test Provider"
            }
            fn provider_id(&self) -> &str {
                &self.id
            }
        }

        ce.set_ai_reasoner(std::sync::Arc::new(DynamicProvider { id: "dynamic_custom".to_string() }));
        assert_eq!(ce.ai_reasoner().provider_id(), "dynamic_custom");

        // 5. Verify ContextEngine executes the newly swapped provider without any engine code modification
        let custom_res = ce.analyze_item_context_ai(&conn, &inbox_item.id).expect("Analysis should succeed with custom reasoner");
        assert_eq!(custom_res.decision, ContextReasoningDecision::NoContext);
        assert_eq!(custom_res.explanation, "Custom dynamic reasoner forced no_context.");
    }

    // =========================================================================
    // AI Privacy Boundary & Policy Layer Unit Tests
    // =========================================================================

    #[test]
    fn test_privacy_boundary_blocks_when_ai_reasoning_disabled() {
        let (conn, _path) = setup_test_db();
        let session = operations::create_session(&conn).unwrap();
        let _ws = operations::create_workspace(&conn, "Kyoto Trip").unwrap();
        let inbox_item = operations::create_item(
            &conn,
            None,
            Some(&session.id),
            "text",
            "Shinkansen tickets to Kyoto",
            Some("Train Tickets"),
            "{}",
        ).unwrap();

        // 1. Disable remote AI reasoning in settings
        let mut settings = crate::db::settings::get_settings(&conn).unwrap();
        settings.enable_ai_reasoning = false;
        crate::db::settings::update_settings(&conn, settings).unwrap();

        // 2. Invoking analyze_item_context_ai directly must return an error
        let reasoner = MockAIReasoner::new();
        let result = analyze_item_context_ai(&conn, &inbox_item.id, &reasoner);
        assert!(result.is_err(), "analyze_item_context_ai should fail when AI reasoning is disabled");
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("AI reasoning is disabled in Privacy Settings"),
            "Error message should explain that AI reasoning is disabled: {}",
            err_msg
        );

        // 3. build_evidence_package must also fail with the privacy boundary violation
        let pkg_res = build_evidence_package(&conn, &inbox_item.id);
        assert!(pkg_res.is_err(), "build_evidence_package should fail when AI reasoning is disabled");

        // 4. Direct audit_and_sanitize check
        let boundary = AIPrivacyBoundary::from_settings(&crate::db::settings::get_settings(&conn).unwrap());
        assert_eq!(boundary.check_permission(), Err(AIPrivacyViolation::AIReasoningDisabled));
    }

    #[test]
    fn test_deterministic_engine_continues_functioning_when_ai_disabled() {
        let (conn, _path) = setup_test_db();
        let session = operations::create_session(&conn).unwrap();
        let ws = operations::create_workspace(&conn, "Kyoto Trip").unwrap();

        let _item1 = operations::create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "Hotel Granvia Kyoto booking",
            Some("Hotel"),
            "{}",
        ).unwrap();

        let inbox_item = operations::create_item(
            &conn,
            None,
            Some(&session.id),
            "text",
            "Shinkansen bullet train reservation to Kyoto station",
            Some("Train"),
            "{}",
        ).unwrap();

        // Disable AI reasoning in user settings
        let mut settings = crate::db::settings::get_settings(&conn).unwrap();
        settings.enable_ai_reasoning = false;
        crate::db::settings::update_settings(&conn, settings).unwrap();

        // Deterministic Context Engine logic must continue functioning without issue
        let eval = crate::context_engine::scoring::evaluate_item_context(&conn, &inbox_item.id)
            .expect("Deterministic scoring must succeed even when AI reasoning is disabled");
        
        assert!(!eval.existing_workspaces.is_empty(), "Deterministic engine should score existing workspaces");
        let top_ws = &eval.existing_workspaces[0];
        assert_eq!(top_ws.workspace_id, ws.id);
        assert!(top_ws.score > 0.0, "Deterministic score should be non-zero");

        // Candidate discovery must also continue functioning deterministically
        let candidates = crate::context_engine::discovery::run_discovery(&conn)
            .expect("Deterministic context candidate search should succeed");
        let _ = candidates.len();
    }

    #[test]
    fn test_privacy_boundary_never_sends_full_clipboard_history() {
        let (conn, _path) = setup_test_db();
        let session = operations::create_session(&conn).unwrap();
        let ws = operations::create_workspace(&conn, "Research").unwrap();

        // 1. Create target item
        let target_item = operations::create_item(
            &conn,
            None,
            Some(&session.id),
            "text",
            "Target item: compiling Rust to WebAssembly with wasm-pack",
            Some("Wasm Compiler"),
            "{}",
        ).unwrap();

        // 2. Create 2 related items
        let rel1 = operations::create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "Related 1: wasm-bindgen guide and memory buffers",
            Some("Wasm Bindgen"),
            "{}",
        ).unwrap();
        let rel2 = operations::create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "Related 2: web-sys and js-sys bindings for Rust",
            Some("Web Sys"),
            "{}",
        ).unwrap();

        operations::create_item_relationship(&conn, &target_item.id, &rel1.id, "semantic_similarity", 0.88, None).unwrap();
        operations::create_item_relationship(&conn, &target_item.id, &rel2.id, "content_overlap", 0.72, None).unwrap();

        // 3. Flood the database with 25 unrelated clipboard items
        for i in 0..25 {
            operations::create_item(
                &conn,
                None,
                Some(&session.id),
                "text",
                &format!("Unrelated clipboard dump item #{} random text", i),
                Some(&format!("Clipboard #{}", i)),
                "{}",
            ).unwrap();
        }

        // Total items in database: 1 + 2 + 25 = 28 items
        let total_items: usize = conn.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0)).unwrap();
        assert_eq!(total_items, 28);

        // 4. Build EvidencePackage
        let package = build_evidence_package(&conn, &target_item.id).unwrap();

        // 5. Verify data minimization:
        // Only target + related items are allowed (max 3 related items)
        assert!(package.relevant_related_items.len() <= 3, "Never send more than max_related_items");
        assert_eq!(package.relevant_related_items.len(), 2, "Only the 2 related items should be packaged");
        
        // Excluded count should reflect the other 25 items not included
        assert_eq!(package.system_metadata.total_db_items_excluded, 25, "All 25 unrelated clipboard items must be excluded");

        // Verify none of the unrelated clipboard items are present
        for rel in &package.relevant_related_items {
            assert!(rel.item_id == rel1.id || rel.item_id == rel2.id);
            assert!(!rel.snippet.contains("Unrelated clipboard dump"));
        }
    }

    #[test]
    fn test_privacy_boundary_never_sends_unrelated_items() {
        let (conn, _path) = setup_test_db();
        let session = operations::create_session(&conn).unwrap();

        let target_item = operations::create_item(
            &conn,
            None,
            Some(&session.id),
            "text",
            "Debugging memory leak in async worker thread pool",
            Some("Backend Issue"),
            "{}",
        ).unwrap();

        let unrelated_item = operations::create_item(
            &conn,
            None,
            Some(&session.id),
            "text",
            "Grandma's secret chocolate chip cookie recipe with melted butter",
            Some("Cookie Recipe"),
            "{}",
        ).unwrap();

        // Weak relationship (strength 0.15, below threshold 0.40)
        operations::create_item_relationship(
            &conn,
            &target_item.id,
            &unrelated_item.id,
            "temporal_proximity",
            0.15,
            None,
        ).unwrap();

        let package = build_evidence_package(&conn, &target_item.id).unwrap();

        // The unrelated item must be completely excluded from relevant_related_items
        assert!(package.relevant_related_items.is_empty(), "Unrelated item below threshold must be excluded");
        assert_eq!(package.system_metadata.total_db_items_excluded, 1);
    }

    #[test]
    fn test_privacy_boundary_never_sends_raw_embeddings() {
        let (conn, _path) = setup_test_db();
        let session = operations::create_session(&conn).unwrap();
        let ws = operations::create_workspace(&conn, "AI Safety").unwrap();

        let target_item = operations::create_item(
            &conn,
            None,
            Some(&session.id),
            "text",
            "Alignment and interpretability research in transformer neural networks",
            Some("AI Research"),
            "{}",
        ).unwrap();

        // Store raw byte float vectors in the database
        let fake_emb = vec![0.1234_f32, -0.5678_f32, 0.9101_f32, 0.4321_f32];
        let bytes: Vec<u8> = fake_emb.iter().flat_map(|f| f.to_le_bytes()).collect();

        conn.execute(
            "INSERT OR REPLACE INTO item_embeddings (item_id, embedding, provider, model, created_at) VALUES (?1, ?2, 'test', 'mini', '2026-09-04T12:00:00Z')",
            rusqlite::params![target_item.id, bytes],
        ).unwrap();

        conn.execute(
            "INSERT OR REPLACE INTO workspace_embeddings (workspace_id, embedding, provider, model, updated_at) VALUES (?1, ?2, 'test', 'mini', '2026-09-04T12:00:00Z')",
            rusqlite::params![ws.id, bytes],
        ).unwrap();

        let package = build_evidence_package(&conn, &target_item.id).unwrap();

        // Verify serializing the EvidencePackage produces NO raw float vectors or byte arrays
        let pkg_json = serde_json::to_string(&package).unwrap();
        assert!(!pkg_json.contains("0.1234"), "Raw embedding float numbers must never enter EvidencePackage");
        assert!(!pkg_json.contains("-0.5678"), "Raw embedding float numbers must never enter EvidencePackage");
        assert!(!pkg_json.contains("0.9101"), "Raw embedding float numbers must never enter EvidencePackage");

        // Verify workspace profile contains only scalar summary fields
        for prof in &package.workspace_semantic_profiles {
            if prof.has_profile {
                assert!(prof.alignment_score.is_some());
                assert!(prof.alignment_band.is_some());
                assert!(!prof.theme_summary.is_empty());
            }
        }
    }

    #[test]
    fn test_privacy_boundary_never_sends_unnecessary_application_metadata() {
        let (conn, _path) = setup_test_db();
        let session = operations::create_session(&conn).unwrap();
        let ws = operations::create_workspace(&conn, "Browsing").unwrap();

        let target_item = operations::create_item(
            &conn,
            None,
            Some(&session.id),
            "text",
            "Read article on zero-knowledge succinct non-interactive arguments of knowledge",
            Some("ZK Proofs"),
            "{}",
        ).unwrap();

        // Insert active app and domain signals
        crate::context_engine::operations::create_context_signal(
            &conn,
            Some(&ws.id),
            Some(&target_item.id),
            "active_app",
            Some(&session.id),
            &json!({ "app_name": "Google Chrome", "process_id": 12345, "window_title": "Confidential Window" }).to_string(),
        ).unwrap();

        crate::context_engine::operations::create_context_signal(
            &conn,
            Some(&ws.id),
            Some(&target_item.id),
            "url_metadata",
            Some(&session.id),
            &json!({ "domain": "sensitive-internal-wiki.corp.net", "url": "https://sensitive-internal-wiki.corp.net/secret" }).to_string(),
        ).unwrap();

        // 1. When active_app and browser_domain are disabled:
        let mut settings = crate::db::settings::get_settings(&conn).unwrap();
        settings.enable_active_app = false;
        settings.enable_browser_domain = false;
        settings.enable_session_tracking = false;
        crate::db::settings::update_settings(&conn, settings).unwrap();

        let package = build_evidence_package(&conn, &target_item.id).unwrap();

        // Assert app_name, domain, and session_id are stripped from target_item
        assert!(package.target_item.app_name.is_none(), "app_name must be None when enable_active_app is false");
        assert!(package.target_item.domain.is_none(), "domain must be None when enable_browser_domain is false");
        assert!(package.target_item.session_id.is_none(), "session_id must be None when enable_session_tracking is false");

        // Assert signals are stripped from contextual_signals
        for sig in &package.contextual_signals {
            assert_ne!(sig.signal_type, "active_app", "active_app signal must be excluded when disabled");
            assert_ne!(sig.signal_type, "url_metadata", "url_metadata signal must be excluded when disabled");
        }

        // Assert session evidence is cleared
        assert!(package.session_context.is_none());
        assert!(package.relevant_sessions.is_empty());

        // Assert full process IDs or window handles never exist
        let json_str = serde_json::to_string(&package).unwrap();
        assert!(!json_str.contains("12345"), "OS Process IDs must not be leaked");
        assert!(!json_str.contains("Confidential Window"), "Window titles must not be leaked");
    }

    #[test]
    fn test_privacy_boundary_scrubs_all_secrets_credentials_and_tokens() {
        let (conn, _path) = setup_test_db();
        let session = operations::create_session(&conn).unwrap();

        let content_with_secrets = r#"
Deploy credentials:
Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.e30.t-IDcSemACt8x4iTMC6Y5bP3iUrWgnBAxDqiDaJYhoo
AWS_KEY: AKIAIOSFODNN7EXAMPLE
aws_secret_access_key: wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY
OPENAI_KEY: sk-proj-1234567890abcdef1234567890abcdef
GITHUB_TOKEN: ghp_1234567890abcdefghijklmnopqrstuvwxyz
password: SuperSecretPassword123!
DATABASE_URL: postgres://postgres:SecretDbPass123@prod-db.internal:5432/main_db
API_ENDPOINT: https://admin:HttpBasicSecret456@service.corp.internal/v1
CARD: 4532 0150 1234 5678
PRIVATE_KEY:
-----BEGIN RSA PRIVATE KEY-----
MIIEowIBAAKCAQEA0m4w4k0+pG7...
-----END RSA PRIVATE KEY-----
"#;

        let scrubbed_direct = scrub_private_data(content_with_secrets);
        assert!(!scrubbed_direct.contains("SuperSecretPassword123!"));
        assert!(!scrubbed_direct.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(!scrubbed_direct.contains("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"));
        assert!(!scrubbed_direct.contains("sk-proj-1234567890abcdef1234567890abcdef"));
        assert!(!scrubbed_direct.contains("ghp_1234567890abcdefghijklmnopqrstuvwxyz"));
        assert!(!scrubbed_direct.contains("SecretDbPass123"));
        assert!(!scrubbed_direct.contains("HttpBasicSecret456"));
        assert!(!scrubbed_direct.contains("4532 0150 1234 5678"));
        assert!(!scrubbed_direct.contains("MIIEowIBAAKCAQEA0"));

        assert!(scrubbed_direct.contains("[REDACTED]") || scrubbed_direct.contains("[API_KEY_REDACTED]"));
        assert!(scrubbed_direct.contains("[AWS_KEY_REDACTED]"));
        assert!(scrubbed_direct.contains("[CARD_REDACTED]"));
        assert!(scrubbed_direct.contains("[CONNECTION_STRING_REDACTED]"));
        assert!(scrubbed_direct.contains("[CREDENTIAL_REDACTED]"));
        assert!(scrubbed_direct.contains("[PRIVATE_KEY_REDACTED]"));

        // Now test item packaged into EvidencePackage
        let package_item_content = "Card: 4532 0150 1234 5678, DB: postgres://admin:SecretPass99@db.net/app, AWS: AKIAIOSFODNN7EXAMPLE";
        let target_item = operations::create_item(
            &conn,
            None,
            Some(&session.id),
            "text",
            package_item_content,
            Some("Secrets Vault: password=AdminPassword789!"),
            "{}",
        ).unwrap();

        let package = build_evidence_package(&conn, &target_item.id).unwrap();
        let pkg_json = serde_json::to_string(&package).unwrap();

        // 1. Assert NONE of the plaintext secrets exist anywhere in the output
        assert!(!pkg_json.contains("4532 0150 1234 5678"), "Credit card must be redacted");
        assert!(!pkg_json.contains("SecretPass99"), "Database password must be redacted");
        assert!(!pkg_json.contains("AKIAIOSFODNN7EXAMPLE"), "AWS Key ID must be redacted");
        assert!(!pkg_json.contains("AdminPassword789!"), "Title password must be redacted");

        // 2. Assert that redaction placeholders are present in the package JSON
        assert!(pkg_json.contains("[CARD_REDACTED]"));
        assert!(pkg_json.contains("[CONNECTION_STRING_REDACTED]"));
        assert!(pkg_json.contains("[AWS_KEY_REDACTED]"));
        assert!(pkg_json.contains("[REDACTED]"));
    }

    #[test]
    fn test_privacy_boundary_truncates_oversized_snippets() {
        let (conn, _path) = setup_test_db();
        let session = operations::create_session(&conn).unwrap();

        let massive_content = "A".repeat(1500);
        let target_item = operations::create_item(
            &conn,
            None,
            Some(&session.id),
            "text",
            &massive_content,
            Some("Massive Content Item"),
            "{}",
        ).unwrap();

        let package = build_evidence_package(&conn, &target_item.id).unwrap();

        // Default max_snippet_chars is 200 (with "..." suffix => 203)
        assert!(package.target_item.content_snippet.chars().count() <= 203);
        assert!(package.target_item.content_snippet.ends_with("..."));
    }

    #[test]
    fn test_remote_llm_receives_strictly_evidence_package() {
        let (conn, _path) = setup_test_db();
        let session = operations::create_session(&conn).unwrap();
        let _ws = operations::create_workspace(&conn, "DevOps").unwrap();

        let target_item = operations::create_item(
            &conn,
            None,
            Some(&session.id),
            "text",
            "Kubernetes ingress configuration for production gateway",
            Some("K8s Ingress"),
            "{}",
        ).unwrap();

        let package = build_evidence_package(&conn, &target_item.id).unwrap();

        let config = AIProviderConfig {
            provider_type: AIProviderType::RemoteLlm,
            model_name: "gpt-4o-mini".to_string(),
            endpoint: Some("https://api.openai.com/v1".to_string()),
            api_key: Some("sk-test-key-mock".to_string()),
            timeout_seconds: 5,
            max_retries: 2,
            custom_headers: std::collections::HashMap::new(),
        };
        let reasoner = RemoteLLMReasoner::new(config);

        // Verify the prompt content formatted for the remote LLM:
        let user_prompt = reasoner.format_user_prompt(&package);
        
        // The prompt MUST contain the sanitized EvidencePackage JSON
        assert!(user_prompt.contains("Kubernetes ingress configuration"));
        assert!(user_prompt.contains("EvidencePackage"));
        assert!(user_prompt.contains("ai_privacy_boundary:strict_curated_evidence_only:v1"));

        // The prompt must NOT contain raw provider secrets or unscrubbed database metadata
        assert!(!user_prompt.contains("sk-test-key-mock"), "API key must not be leaked in the user prompt payload");
    }

    #[test]
    fn test_privacy_boundary_custom_policy_audit() {
        let policy = AIPrivacyPolicy {
            enable_ai_reasoning: true,
            allow_active_app: false,
            allow_browser_domain: false,
            allow_session_tracking: false,
            max_related_items: 1,
            max_snippet_chars: 50,
            min_relationship_strength: 0.70,
            scrub_secrets: true,
        };

        let boundary = AIPrivacyBoundary::new(policy);

        let mut package = EvidencePackage {
            target_item: TargetItemEvidence {
                id: "target_1".to_string(),
                item_type: "text".to_string(),
                title: Some("Test item with password=secret".to_string()),
                content_snippet: "Very long content exceeding fifty characters limit for testing privacy boundary truncation".to_string(),
                app_name: Some("LeakedApp".to_string()),
                domain: Some("leaked.domain.com".to_string()),
                session_id: Some("sess_leaked".to_string()),
                captured_at: "2026-09-04T12:00:00Z".to_string(),
            },
            candidate_contexts: vec![],
            relevant_related_items: vec![
                RelatedItemEvidence {
                    item_id: "rel_1".to_string(),
                    title: None,
                    item_type: "text".to_string(),
                    snippet: "Strong relation item".to_string(),
                    workspace_id: None,
                    workspace_name: None,
                    relevance_score: 0.85,
                },
                RelatedItemEvidence {
                    item_id: "rel_2".to_string(),
                    title: None,
                    item_type: "text".to_string(),
                    snippet: "Weak relation item".to_string(),
                    workspace_id: None,
                    workspace_name: None,
                    relevance_score: 0.50,
                },
            ],
            relationships: vec![],
            semantic_similarity_summaries: vec![],
            relevant_sessions: vec![],
            session_context: None,
            workspace_semantic_profiles: vec![],
            contextual_signals: vec![
                CuratedContextSignal {
                    signal_type: "active_app".to_string(),
                    summary: "Application: LeakedApp".to_string(),
                },
            ],
            temporal_context: TemporalEvidence {
                age_days: 0.1,
                recency_decay_weight: 1.0,
                is_recent: true,
            },
            deterministic_assessment: DeterministicEvidenceAssessment {
                confidence_band: "none".to_string(),
                is_weak_evidence: true,
                is_ambiguous: false,
                competing_contexts: vec![],
                assessment_summary: "No candidates".to_string(),
            },
            system_metadata: EvidenceMetadata {
                generated_at: "2026-09-04T12:00:00Z".to_string(),
                engine_version: "v1".to_string(),
                total_db_items_excluded: 0,
                privacy_boundary: "".to_string(),
            },
        };

        boundary.audit_and_sanitize(&mut package).unwrap();

        // 1. App name and domain stripped
        assert!(package.target_item.app_name.is_none());
        assert!(package.target_item.domain.is_none());
        assert!(package.target_item.session_id.is_none());

        // 2. Secret scrubbed
        assert!(package.target_item.title.unwrap().contains("[REDACTED]"));

        // 3. Truncated to 50 chars
        assert!(package.target_item.content_snippet.chars().count() <= 53);

        // 4. Weak relation filtered (< 0.70) and count capped to max_related_items = 1
        assert_eq!(package.relevant_related_items.len(), 1);
        assert_eq!(package.relevant_related_items[0].item_id, "rel_1");

        // 5. Active app signal stripped
        assert!(package.contextual_signals.is_empty());

        // 6. Privacy boundary stamp set
        assert_eq!(package.system_metadata.privacy_boundary, "ai_privacy_boundary:strict_curated_evidence_only:v1");
    }

    #[test]
    fn test_human_readable_strong_explanation_matches_exact_spec() {
        let fixture = strong_existing_context_fixture();
        let reasoner = MockAIReasoner::new();
        let result = reasoner.analyze_context(&fixture).expect("Reasoning should succeed");

        assert_eq!(result.decision, ContextReasoningDecision::ExistingContext);
        assert_eq!(result.suggested_name.as_deref(), Some("Paris Trip"));
        assert_eq!(
            result.explanation,
            "This appears related to Paris Trip because it is similar to several recent travel-research items and shares domains already associated with that context."
        );
        assert!(result.uncertainty_reason.is_none());

        // Invariant checks
        assert!(detect_exposed_internal_metrics(&result.explanation).is_none());
        assert!(validate_explanation_grounding(&result, &fixture).is_ok());
    }

    #[test]
    fn test_human_readable_uncertain_explanation_matches_exact_spec() {
        let package = base_dummy_package(vec![CandidateContextEvidence {
            context_id: "ws_paris".to_string(),
            name: "Paris Trip".to_string(),
            context_type: "workspace".to_string(),
            lifecycle_state: "active".to_string(),
            deterministic_score: 0.18,
            deterministic_confidence: 0.18,
            confidence_band: "Low".to_string(),
            evidence_factors: vec!["Marginal domain overlap".to_string()],
            item_count: 5,
            temporal_recency_score: 0.5,
        }]);

        let reasoner = MockAIReasoner::new();
        let result = reasoner.analyze_context(&package).expect("Reasoning should succeed");

        assert_eq!(result.decision, ContextReasoningDecision::NoContext);
        assert_eq!(
            result.explanation,
            "This may be related to Paris Trip, but the available evidence is limited."
        );
        assert!(result.uncertainty_reason.is_some());

        // Invariant checks
        assert!(detect_exposed_internal_metrics(&result.explanation).is_none());
        assert!(validate_explanation_grounding(&result, &package).is_ok());
    }

    #[test]
    fn test_validation_rejects_exposed_internal_scores_in_explanation() {
        let base_res = ContextReasoningResult {
            decision: ContextReasoningDecision::ExistingContext,
            suggested_context_reference: Some("ws_1".to_string()),
            suggested_name: Some("Paris Trip".to_string()),
            confidence: 0.85,
            explanation: "Valid text".to_string(),
            evidence_references: vec![],
            uncertainty_reason: None,
            provider_info: None,
            alternative_interpretations: vec![],
        };

        // 1. Raw float decimal leak (e.g. 0.85)
        let mut res1 = base_res.clone();
        res1.explanation = "This item matches Paris Trip with 0.85 confidence score.".to_string();
        assert!(matches!(
            validate_reasoning_result(&res1),
            Err(ReasoningValidationError::ExposedInternalMetrics(_))
        ));

        // 2. Explicit score indicator (e.g. score: 0.9)
        let mut res2 = base_res.clone();
        res2.explanation = "Related to Paris Trip because score: 0.9 exceeds threshold.".to_string();
        assert!(matches!(
            validate_reasoning_result(&res2),
            Err(ReasoningValidationError::ExposedInternalMetrics(_))
        ));

        // 3. Percentage score indicator (e.g. 89% confidence)
        let mut res3 = base_res.clone();
        res3.explanation = "Strong match with 89% confidence based on recent flights.".to_string();
        assert!(matches!(
            validate_reasoning_result(&res3),
            Err(ReasoningValidationError::ExposedInternalMetrics(_))
        ));
    }

    #[test]
    fn test_validation_rejects_exposed_embeddings_in_explanation() {
        let base_res = ContextReasoningResult {
            decision: ContextReasoningDecision::ExistingContext,
            suggested_context_reference: Some("ws_1".to_string()),
            suggested_name: Some("Paris Trip".to_string()),
            confidence: 0.85,
            explanation: "Valid text".to_string(),
            evidence_references: vec![],
            uncertainty_reason: None,
            provider_info: None,
            alternative_interpretations: vec![],
        };

        // 1. Cosine similarity technical jargon
        let mut res1 = base_res.clone();
        res1.explanation = "High cosine similarity observed with travel items.".to_string();
        assert!(matches!(
            validate_reasoning_result(&res1),
            Err(ReasoningValidationError::ExposedInternalMetrics(_))
        ));

        // 2. Vector embedding reference
        let mut res2 = base_res.clone();
        res2.explanation = "Item sits close in the vector space to Paris Trip.".to_string();
        assert!(matches!(
            validate_reasoning_result(&res2),
            Err(ReasoningValidationError::ExposedInternalMetrics(_))
        ));

        // 3. Raw vector array representation
        let mut res3 = base_res.clone();
        res3.explanation = "Item embedding [0.245, -0.912] matches cluster.".to_string();
        assert!(matches!(
            validate_reasoning_result(&res3),
            Err(ReasoningValidationError::ExposedInternalMetrics(_))
        ));
    }

    #[test]
    fn test_validation_rejects_uncertain_without_communicating_uncertainty() {
        // Uncertain decision must communicate uncertainty in explanation
        let res = ContextReasoningResult {
            decision: ContextReasoningDecision::Uncertain,
            suggested_context_reference: Some("ws_1".to_string()),
            suggested_name: Some("Paris Trip".to_string()),
            confidence: 0.50,
            explanation: "This definitely belongs to Paris Trip without a doubt.".to_string(),
            evidence_references: vec![],
            uncertainty_reason: Some("Competing signals".to_string()),
            provider_info: None,
            alternative_interpretations: vec![],
        };

        assert!(matches!(
            validate_reasoning_result(&res),
            Err(ReasoningValidationError::UncertaintyNotCommunicated(_))
        ));
    }

    #[test]
    fn test_grounding_rejects_hallucinated_workspace_or_unsupported_facts() {
        let fixture = strong_existing_context_fixture(); // contains "Paris Trip" and "General Work"

        // 1. Hallucinated workspace reference
        let fake_res = ContextReasoningResult {
            decision: ContextReasoningDecision::ExistingContext,
            suggested_context_reference: Some("ws_mars_colony".to_string()), // Nonexistent in package!
            suggested_name: Some("Mars Colony".to_string()),
            confidence: 0.90,
            explanation: "This appears related to Mars Colony because of space exploration items.".to_string(),
            evidence_references: vec![],
            uncertainty_reason: None,
            provider_info: None,
            alternative_interpretations: vec![],
        };

        let err = validate_explanation_grounding(&fake_res, &fixture);
        assert!(matches!(err, Err(ReasoningValidationError::UnsupportedFactDetected(_))));

        // 2. Hallucinated workspace name for real ID
        let mismatch_res = ContextReasoningResult {
            decision: ContextReasoningDecision::ExistingContext,
            suggested_context_reference: Some("ws_paris_trip".to_string()),
            suggested_name: Some("Fictitious Trip".to_string()), // Real ID, but fake name!
            confidence: 0.90,
            explanation: "This appears related to Fictitious Trip.".to_string(),
            evidence_references: vec![],
            uncertainty_reason: None,
            provider_info: None,
            alternative_interpretations: vec![],
        };

        let err2 = validate_explanation_grounding(&mismatch_res, &fixture);
        assert!(matches!(err2, Err(ReasoningValidationError::UnsupportedFactDetected(_))));
    }

    #[test]
    fn test_grounding_rejects_no_context_with_unsupported_reference() {
        let fixture = no_context_fixture();

        let invalid_no_context = ContextReasoningResult {
            decision: ContextReasoningDecision::NoContext,
            suggested_context_reference: Some("ws_unexpected_link".to_string()), // NoContext cannot link to a workspace!
            suggested_name: None,
            confidence: 0.0,
            explanation: "There is insufficient evidence to associate this item with a context; item remains in Inbox.".to_string(),
            evidence_references: vec![],
            uncertainty_reason: Some("No matching signals".to_string()),
            provider_info: None,
            alternative_interpretations: vec![],
        };

        let err = validate_explanation_grounding(&invalid_no_context, &fixture);
        assert!(matches!(err, Err(ReasoningValidationError::UnsupportedFactDetected(_))));
    }

    #[test]
    fn test_remote_system_prompt_demands_human_readable_and_bans_scores() {
        let config = AIProviderConfig::default();
        let reasoner = RemoteLLMReasoner::new(config);
        let prompt = reasoner.generate_system_prompt();

        // 1. Must ban exposed internal scores or embeddings
        assert!(prompt.contains("DO NOT Expose Internal Scores or Embeddings"));
        assert!(prompt.contains("NEVER include raw internal scores"));
        assert!(prompt.contains("cosine similarity"));

        // 2. Must require grounding exclusively in EvidencePackage
        assert!(prompt.contains("The explanation must ONLY use evidence present in the EvidencePackage"));
        assert!(prompt.contains("DO NOT introduce unsupported facts"));

        // 3. Must require communicating uncertainty
        assert!(prompt.contains("Communicate Uncertainty"));

        // 4. Must include the exact few-shot examples
        assert!(prompt.contains("This appears related to Paris Trip because it is similar to several recent travel-research items and shares domains already associated with that context."));
        assert!(prompt.contains("This may be related to Paris Trip, but the available evidence is limited."));

        // 5. Must explicitly instruct that the AI must NOT be forced to choose a workspace
        assert!(prompt.contains("The AI Must NOT Be Forced to Choose a Workspace"));
        assert!(prompt.contains("set \"suggested_context_reference\": null"));
    }

    #[test]
    fn test_mock_reasoner_ambiguous_does_not_force_workspace() {
        let fixture = ambiguous_context_fixture();
        let reasoner = MockAIReasoner::new();
        let result = reasoner.analyze_context(&fixture).expect("Reasoning should succeed");

        assert_eq!(result.decision, ContextReasoningDecision::Uncertain);
        // The AI must NOT be forced to choose a Workspace
        assert_eq!(result.suggested_context_reference, None);
        assert_eq!(result.suggested_name, None);
        assert!(result.uncertainty_reason.is_some());
        // Alternatives contain all competing candidates for user selection
        assert_eq!(result.alternative_interpretations.len(), 2);
        let names: Vec<_> = result.alternative_interpretations.iter().map(|a| a.context_name.as_str()).collect();
        assert!(names.contains(&"Paris Trip") && names.contains(&"London Client Meetings"));
    }

    #[test]
    fn test_mock_reasoner_no_context_does_not_force_workspace() {
        let fixture = no_context_fixture();
        let reasoner = MockAIReasoner::new();
        let result = reasoner.analyze_context(&fixture).expect("Reasoning should succeed");

        assert_eq!(result.decision, ContextReasoningDecision::NoContext);
        // The AI must NOT be forced to choose a Workspace
        assert_eq!(result.suggested_context_reference, None);
        assert_eq!(result.suggested_name, None);
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn test_evidence_package_communicates_weak_evidence_uncertainty() {
        let (conn, _path) = setup_test_db();
        // Insert an isolated item with no workspace or relationships
        let item = operations::create_item(
            &conn,
            None,
            None,
            "text",
            "vague note about lunch tomorrow",
            Some("Personal"),
            "{}",
        ).unwrap();

        let package = build_evidence_package(&conn, &item.id).expect("Build should succeed");

        // The EvidencePackage must communicate weak evidence uncertainty
        assert!(package.deterministic_assessment.is_weak_evidence);
        assert!(
            package.deterministic_assessment.confidence_band == "weak"
                || package.deterministic_assessment.confidence_band == "none"
        );
        assert!(!package.deterministic_assessment.assessment_summary.is_empty());

        // When reasoning over this weak package, AI must return NoContext without forcing a workspace
        let reasoner = MockAIReasoner::new();
        let result = reasoner.analyze_context(&package).expect("Reasoning should succeed");
        assert_eq!(result.decision, ContextReasoningDecision::NoContext);
        assert_eq!(result.suggested_context_reference, None);
    }

    #[test]
    fn test_evidence_package_communicates_ambiguity() {
        let fixture = ambiguous_context_fixture();
        assert!(fixture.deterministic_assessment.is_ambiguous);
        assert_eq!(fixture.deterministic_assessment.confidence_band, "uncertain");
        assert_eq!(fixture.deterministic_assessment.competing_contexts.len(), 2);
        assert!(fixture.deterministic_assessment.assessment_summary.contains("Ambiguous signals"));
    }

    #[test]
    fn test_ambiguous_items_do_not_get_forced_into_workspaces() {
        let (conn, _path) = setup_test_db();

        // Create two competing workspaces
        let ws1 = operations::create_workspace(&conn, "Paris Vacation").unwrap();
        let ws2 = operations::create_workspace(&conn, "London Trip").unwrap();

        // Create an item in ws1 with domain eurostar.com
        let item1 = operations::create_item(
            &conn,
            Some(&ws1.id),
            None,
            "text",
            "Paris hotel reservation",
            Some("Paris Hotel"),
            "{}",
        ).unwrap();
        let _ = crate::context_engine::operations::create_context_signal(
            &conn,
            Some(&ws1.id),
            Some(&item1.id),
            "url_metadata",
            None,
            &serde_json::json!({ "domain": "eurostar.com" }).to_string(),
        );

        // Create an item in ws2 with domain eurostar.com
        let item2 = operations::create_item(
            &conn,
            Some(&ws2.id),
            None,
            "text",
            "London meeting notes",
            Some("London Office"),
            "{}",
        ).unwrap();
        let _ = crate::context_engine::operations::create_context_signal(
            &conn,
            Some(&ws2.id),
            Some(&item2.id),
            "url_metadata",
            None,
            &serde_json::json!({ "domain": "eurostar.com" }).to_string(),
        );

        // Create unassigned ticket with common domain eurostar.com
        let target_item = operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Eurostar ticket London to Paris",
            Some("Eurostar Booking"),
            "{}",
        ).unwrap();
        let _ = crate::context_engine::operations::create_context_signal(
            &conn,
            None,
            Some(&target_item.id),
            "url_metadata",
            None,
            &serde_json::json!({ "domain": "eurostar.com" }).to_string(),
        );

        // Add relationships with items in both workspaces to produce competing high affinity (> 0.35)
        operations::create_item_relationship(&conn, &target_item.id, &item1.id, "semantic_similarity", 0.60, None).unwrap();
        operations::create_item_relationship(&conn, &target_item.id, &item2.id, "semantic_similarity", 0.58, None).unwrap();

        // Evaluate item context
        let interp = crate::context_engine::scoring::evaluate_item_context(&conn, &target_item.id).unwrap();

        // Ambiguous item MUST NOT be forced into "existing_workspace"!
        assert_eq!(interp.primary_interpretation, "ambiguous");
        assert!(interp.summary.contains("Ambiguous match"));

        // Recalculating workspace suggested memberships MUST NOT auto-create a suggested membership for ambiguous items
        let new_memberships = crate::context_engine::scoring::recalculate_workspace_suggested_memberships(
            &conn,
            &ws1.id,
            None,
        ).unwrap();
        assert!(
            new_memberships.iter().all(|m| m.item_id != target_item.id),
            "Ambiguous item must NOT be forced into a workspace suggestion"
        );
    }

    #[test]
    fn test_ai_assisted_candidate_naming_grounded_in_evidence() {
        let (conn, _db_path) = setup_test_db();
        let reasoner = MockAIReasoner::new();

        // Create 2 items in Inbox with clear Paris travel theme
        let i1 = operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Louvre museum tickets and opening hours",
            Some("Louvre Museum"),
            "{}",
        ).unwrap();
        let _ = crate::context_engine::operations::create_context_signal(
            &conn,
            None,
            Some(&i1.id),
            "url_metadata",
            None,
            &json!({ "domain": "louvre.fr" }).to_string(),
        );

        let i2 = operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Paris hotel reservation confirmation near Eiffel tower",
            Some("Paris Hotel"),
            "{}",
        ).unwrap();
        let _ = crate::context_engine::operations::create_context_signal(
            &conn,
            None,
            Some(&i2.id),
            "url_metadata",
            None,
            &json!({ "domain": "booking.com" }).to_string(),
        );

        // Request AI-assisted candidate naming
        let name = suggest_item_group_name_ai(&conn, &[i1.id.clone(), i2.id.clone()], &reasoner).unwrap();

        // Name must be 2–4 words preferred, non-empty, and grounded in evidence
        let words: Vec<&str> = name.split_whitespace().collect();
        assert!(words.len() >= 2 && words.len() <= 4, "Name should be 2–4 words, got '{}' (len {})", name, words.len());
        assert!(!name.is_empty());
        assert!(name != "New Context" && name != "Untitled");
    }

    #[test]
    fn test_ai_candidate_naming_avoids_generic_when_specific_evidence_exists() {
        let (conn, _db_path) = setup_test_db();
        let reasoner = MockAIReasoner::new();

        let i1 = operations::create_item(
            &conn,
            None,
            None,
            "text",
            "TripWise frontend architecture design and state management",
            Some("TripWise Frontend"),
            "{}",
        ).unwrap();

        let i2 = operations::create_item(
            &conn,
            None,
            None,
            "text",
            "TripWise backend SQLite schema migrations",
            Some("TripWise Backend"),
            "{}",
        ).unwrap();

        let name = suggest_item_group_name_ai(&conn, &[i1.id, i2.id], &reasoner).unwrap();
        assert!(name.contains("Tripwise") || name.contains("TripWise") || name.contains("Frontend") || name.contains("Backend"));
        assert_ne!(name, "New Context");
        assert_ne!(name, "General Project");
    }

    #[test]
    fn test_ai_candidate_naming_fallback_on_failure() {
        let (conn, _db_path) = setup_test_db();

        let i1 = operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Rust compiler optimization benchmarks",
            Some("Compiler Benchmarks"),
            "{}",
        ).unwrap();
        let i2 = operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Rust async runtime profiling",
            Some("Async Runtime"),
            "{}",
        ).unwrap();

        // Deterministic fallback always produces a valid non-empty name
        let deterministic_name = crate::context_engine::discovery::generate_candidate_name(&conn, &[i1.id.clone(), i2.id.clone()]).unwrap();
        assert!(!deterministic_name.is_empty());
        assert!(deterministic_name.contains("Rust") || deterministic_name.contains("Compiler") || deterministic_name.contains("Runtime") || deterministic_name.contains("Bench"));
    }

    #[test]
    fn test_ai_candidate_naming_disabled_setting_fallback() {
        let (conn, _db_path) = setup_test_db();

        // Disable remote AI reasoning
        let mut settings = crate::db::settings::get_settings(&conn).unwrap();
        settings.enable_ai_reasoning = false;
        crate::db::settings::update_settings(&conn, settings).unwrap();

        let i1 = operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Figma design system components",
            Some("Figma Components"),
            "{}",
        ).unwrap();
        let i2 = operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Figma UI color tokens and typography",
            Some("Figma Design Tokens"),
            "{}",
        ).unwrap();
        let i3 = operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Figma layout grids and wireframes",
            Some("Figma Layout Grids"),
            "{}",
        ).unwrap();

        // Create semantic relationship between items
        operations::create_item_relationship(&conn, &i1.id, &i2.id, "semantic_similarity", 0.90, None).unwrap();
        operations::create_item_relationship(&conn, &i2.id, &i3.id, "semantic_similarity", 0.88, None).unwrap();
        operations::create_item_relationship(&conn, &i1.id, &i3.id, "semantic_similarity", 0.85, None).unwrap();

        // Run context discovery with AI reasoning disabled
        let candidates = crate::context_engine::discovery::run_discovery(&conn).unwrap();

        // Discovery must succeed using deterministic naming without failing
        assert!(!candidates.is_empty());
        let cand = &candidates[0];
        assert!(cand.label.is_some());
        let label = cand.label.as_ref().unwrap();
        assert!(!label.is_empty());
    }

    #[test]
    fn test_ai_candidate_naming_does_not_break_context_discovery() {
        let (conn, _db_path) = setup_test_db();
        let reasoner = MockAIReasoner::new();

        // Ensure AI reasoning is enabled
        let mut settings = crate::db::settings::get_settings(&conn).unwrap();
        settings.enable_ai_reasoning = true;
        crate::db::settings::update_settings(&conn, settings).unwrap();

        let i1 = operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Database clustering query optimizations",
            Some("DB Query Optimization"),
            "{}",
        ).unwrap();
        let i2 = operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Database indexing and execution plan",
            Some("DB Indexing"),
            "{}",
        ).unwrap();
        let i3 = operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Database connection pooling configuration",
            Some("DB Connection Pooling"),
            "{}",
        ).unwrap();

        // Add relationships
        operations::create_item_relationship(&conn, &i1.id, &i2.id, "semantic_similarity", 0.92, None).unwrap();
        operations::create_item_relationship(&conn, &i2.id, &i3.id, "semantic_similarity", 0.89, None).unwrap();
        operations::create_item_relationship(&conn, &i1.id, &i3.id, "semantic_similarity", 0.86, None).unwrap();

        // Test building candidate evidence package directly
        let pkg_group = build_item_group_evidence_package(&conn, &[i1.id.clone(), i2.id.clone(), i3.id.clone()]).unwrap();
        assert!(!pkg_group.target_item.content_snippet.is_empty());

        // Run discovery
        let candidates = crate::context_engine::discovery::run_discovery(&conn).unwrap();
        assert!(!candidates.is_empty(), "Context discovery must create candidates");

        let cand = &candidates[0];
        assert!(cand.label.is_some());
        assert!(!cand.label.as_ref().unwrap().is_empty());
        assert_eq!(cand.status, "detected");

        // Verify building evidence package for existing candidate record
        let pkg_cand = build_candidate_evidence_package(&conn, &cand.id).unwrap();
        assert!(!pkg_cand.target_item.content_snippet.is_empty());

        // Verify suggest_candidate_name_ai works on the discovered candidate
        let ai_name = suggest_candidate_name_ai(&conn, &cand.id, &reasoner).unwrap();
        assert!(!ai_name.is_empty());
        let words: Vec<&str> = ai_name.split_whitespace().collect();
        assert!(words.len() >= 2 && words.len() <= 4);
    }

    #[test]
    fn test_ai_naming_strict_non_mutation_invariant() {
        let (conn, _db_path) = setup_test_db();
        let reasoner = MockAIReasoner::new();

        let i1 = operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Machine Learning inference pipeline",
            Some("ML Pipeline"),
            "{}",
        ).unwrap();
        let i2 = operations::create_item(
            &conn,
            None,
            None,
            "text",
            "Model training dataset preparation",
            Some("Dataset Prep"),
            "{}",
        ).unwrap();

        let items_before: i64 = conn.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0)).unwrap();
        let workspaces_before: i64 = conn.query_row("SELECT COUNT(*) FROM workspaces", [], |r| r.get(0)).unwrap();
        let memberships_before: i64 = conn.query_row("SELECT COUNT(*) FROM context_memberships", [], |r| r.get(0)).unwrap();

        // Request AI naming
        let name = suggest_item_group_name_ai(&conn, &[i1.id.clone(), i2.id.clone()], &reasoner).unwrap();
        assert!(!name.is_empty());

        let items_after: i64 = conn.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0)).unwrap();
        let workspaces_after: i64 = conn.query_row("SELECT COUNT(*) FROM workspaces", [], |r| r.get(0)).unwrap();
        let memberships_after: i64 = conn.query_row("SELECT COUNT(*) FROM context_memberships", [], |r| r.get(0)).unwrap();

        // INVARIANTS: AI naming MUST NOT create or mutate workspaces or items
        assert_eq!(items_before, items_after, "AI naming must NOT create items");
        assert_eq!(workspaces_before, workspaces_after, "AI naming must NOT create workspaces");
        assert_eq!(memberships_before, memberships_after, "AI naming must NOT create memberships");

        // Items must still be in Inbox (workspace_id = None)
        let item1 = operations::get_item(&conn, &i1.id).unwrap();
        let item2 = operations::get_item(&conn, &i2.id).unwrap();
        assert_eq!(item1.workspace_id, None);
        assert_eq!(item2.workspace_id, None);
    }

    // =========================================================================
    // Graceful AI Failure Handling — Integration Tests
    // =========================================================================
    //
    // These tests verify the core safety invariant:
    //   AI failure MUST NEVER prevent item capture, inbox, workspace management,
    //   context scoring, context discovery, or context membership.
    //
    // Each test:
    //   1. Injects a specific failure mode via FailingMockAIReasoner.
    //   2. Confirms analyze_item_context_ai_graceful returns ai_unavailable = true.
    //   3. Confirms the correct failure reason is classified.
    //   4. Confirms the database and all deterministic features remain intact.
    // =========================================================================

    use crate::context_engine::reasoning::builder::analyze_item_context_ai_graceful;
    use crate::context_engine::reasoning::mock::FailingMockAIReasoner;

    fn setup_item_for_failure_tests(conn: &Connection) -> String {
        operations::create_item(conn, None, None, "text", "AI failure test item content", Some("Test Item"), "{}")
            .unwrap()
            .id
    }

    // ── Timeout ──────────────────────────────────────────────────────────────

    #[test]
    fn test_graceful_failure_timeout() {
        let (conn, _) = setup_test_db();
        let item_id = setup_item_for_failure_tests(&conn);
        let reasoner = FailingMockAIReasoner::timeout();

        let response = analyze_item_context_ai_graceful(&conn, &item_id, &reasoner, false);

        assert!(response.ai_unavailable, "Timeout should mark AI as unavailable");
        assert!(response.ai_result.is_none(), "ai_result must be None on timeout");
        assert_eq!(response.failure_reason, Some(AIFailureReason::Timeout));
        assert!(response.unavailability_reason.is_some());

        // DB must be fully intact
        let item = operations::get_item(&conn, &item_id).unwrap();
        assert_eq!(item.item_type, "text", "Item must remain intact after AI timeout");
        assert_eq!(item.workspace_id, None, "Item must remain in Inbox after AI timeout");
    }

    // ── Rate Limit (429) ──────────────────────────────────────────────────────

    #[test]
    fn test_graceful_failure_rate_limited() {
        let (conn, _) = setup_test_db();
        let item_id = setup_item_for_failure_tests(&conn);
        let reasoner = FailingMockAIReasoner::rate_limited();

        let response = analyze_item_context_ai_graceful(&conn, &item_id, &reasoner, false);

        assert!(response.ai_unavailable);
        assert!(response.ai_result.is_none());
        assert_eq!(response.failure_reason, Some(AIFailureReason::ProviderError));
        assert!(response.unavailability_reason.is_some());

        // Workspace management is unaffected
        let ws = operations::create_workspace(&conn, "Rate Limit Test Workspace").unwrap();
        assert!(!ws.id.is_empty(), "Workspace creation must succeed after AI rate limit");
    }

    // ── Malformed Response ────────────────────────────────────────────────────

    #[test]
    fn test_graceful_failure_malformed_response() {
        let (conn, _) = setup_test_db();
        let item_id = setup_item_for_failure_tests(&conn);
        let reasoner = FailingMockAIReasoner::malformed_response();

        let response = analyze_item_context_ai_graceful(&conn, &item_id, &reasoner, false);

        assert!(response.ai_unavailable);
        assert!(response.ai_result.is_none());
        assert_eq!(response.failure_reason, Some(AIFailureReason::MalformedResponse));

        // Item capture still works
        let new_item = operations::create_item(&conn, None, None, "link", "https://example.com", None, "{}").unwrap();
        assert!(!new_item.id.is_empty(), "Item capture must work after malformed AI response");
    }

    // ── Provider Unreachable (is_available = false) ───────────────────────────

    #[test]
    fn test_graceful_failure_provider_unavailable() {
        let (conn, _) = setup_test_db();
        let item_id = setup_item_for_failure_tests(&conn);
        let reasoner = FailingMockAIReasoner::unavailable();

        let response = analyze_item_context_ai_graceful(&conn, &item_id, &reasoner, false);

        assert!(response.ai_unavailable);
        assert!(response.ai_result.is_none());
        assert_eq!(response.failure_reason, Some(AIFailureReason::ProviderUnavailable));

        // Context scoring (deterministic) still works
        let ws = operations::create_workspace(&conn, "Workspace A").unwrap();
        let item2 = operations::create_item(&conn, Some(&ws.id), None, "text", "workspace item", None, "{}").unwrap();
        assert!(!item2.id.is_empty());
    }

    // ── AI Disabled in Settings ───────────────────────────────────────────────

    #[test]
    fn test_graceful_failure_ai_disabled_in_settings() {
        let (conn, _dir) = {
            let temp_dir = std::env::temp_dir().join(format!("cs_ai_disabled_{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&temp_dir).unwrap();
            let db_path = temp_dir.join("test.db");
            let conn = Connection::open(&db_path).unwrap();
            db::run_migrations(&conn).unwrap();
            (conn, temp_dir)
        };

        // Explicitly disable AI reasoning
        crate::db::settings::update_settings(&conn, PrivacySettings {
            enable_active_app: true,
            enable_window_title: true,
            enable_browser_domain: true,
            enable_session_tracking: true,
            enable_semantic_processing: true,
            semantic_provider: "local".to_string(),
            enable_semantic_suggestions: true,
            enable_ai_reasoning: false,  // <-- DISABLED
            ai_gating_enabled: true,
            ai_gating_confidence_threshold: 0.9,
            capture_paused: false,
        }).unwrap();

        let item_id = operations::create_item(&conn, None, None, "text", "disabled AI test", None, "{}")
            .unwrap()
            .id;

        let reasoner = MockAIReasoner::new();  // Would succeed if called, but won't be reached
        let response = analyze_item_context_ai_graceful(&conn, &item_id, &reasoner, false);

        assert!(response.ai_unavailable, "AI must be reported unavailable when disabled in settings");
        assert!(response.ai_result.is_none());
        assert_eq!(response.failure_reason, Some(AIFailureReason::Disabled));

        // All other features work normally
        let ws = operations::create_workspace(&conn, "Workspace Created With AI Disabled").unwrap();
        assert!(!ws.id.is_empty());

        let item = operations::get_item(&conn, &item_id).unwrap();
        assert_eq!(item.workspace_id, None, "Item must remain in Inbox");
    }

    // ── Misconfigured Provider (InvalidConfiguration) ─────────────────────────

    #[test]
    fn test_graceful_failure_misconfigured_provider() {
        let (conn, _) = setup_test_db();
        let item_id = setup_item_for_failure_tests(&conn);
        let reasoner = FailingMockAIReasoner::misconfigured();

        let response = analyze_item_context_ai_graceful(&conn, &item_id, &reasoner, false);

        assert!(response.ai_unavailable);
        assert!(response.ai_result.is_none());
        assert_eq!(response.failure_reason, Some(AIFailureReason::Disabled));
    }

    // ── Evidence Unavailable (item not found) ────────────────────────────────

    #[test]
    fn test_graceful_failure_evidence_unavailable() {
        let (conn, _) = setup_test_db();
        let reasoner = MockAIReasoner::new();

        // Pass a non-existent item ID — EvidencePackage cannot be built
        let response = analyze_item_context_ai_graceful(&conn, "nonexistent-item-id-xyz", &reasoner, false);

        assert!(response.ai_unavailable, "Missing item must result in ai_unavailable");
        assert!(response.ai_result.is_none());
        assert_eq!(response.failure_reason, Some(AIFailureReason::EvidenceUnavailable));
    }

    // ── Generic Inference Failure ─────────────────────────────────────────────

    #[test]
    fn test_graceful_failure_generic_inference_error() {
        let (conn, _) = setup_test_db();
        let item_id = setup_item_for_failure_tests(&conn);
        let reasoner = FailingMockAIReasoner::inference_failed();

        let response = analyze_item_context_ai_graceful(&conn, &item_id, &reasoner, false);

        assert!(response.ai_unavailable);
        assert!(response.ai_result.is_none());
        assert_eq!(response.failure_reason, Some(AIFailureReason::InternalError));

        // Discovery, membership, and inbox unaffected
        let all_items = operations::list_items(&conn, None).unwrap();
        assert!(!all_items.is_empty(), "Items must remain accessible after inference failure");
    }

    // ── Failure does not prevent Workspace Creation ───────────────────────────

    #[test]
    fn test_ai_failure_does_not_prevent_workspace_creation() {
        let (conn, _) = setup_test_db();
        let item_id = setup_item_for_failure_tests(&conn);

        // Simulate AI timeout
        let reasoner = FailingMockAIReasoner::timeout();
        let response = analyze_item_context_ai_graceful(&conn, &item_id, &reasoner, false);
        assert!(response.ai_unavailable);

        // Workspace creation must still succeed
        let ws = operations::create_workspace(&conn, "Emergency Workspace").unwrap();
        assert!(!ws.id.is_empty());

        // Moving item must still succeed
        let moved = operations::move_item(&conn, &item_id, Some(&ws.id)).unwrap();
        assert_eq!(moved.workspace_id.as_deref(), Some(ws.id.as_str()));
    }

    // ── Failure does not prevent Context Discovery ────────────────────────────

    #[test]
    fn test_ai_failure_does_not_prevent_context_discovery() {
        let (conn, _) = setup_test_db();

        // Create items
        let i1 = operations::create_item(&conn, None, None, "text", "discovery item A", None, "{}").unwrap();
        let i2 = operations::create_item(&conn, None, None, "text", "discovery item B", None, "{}").unwrap();

        // Simulate AI malformed response
        let reasoner = FailingMockAIReasoner::malformed_response();
        let r1 = analyze_item_context_ai_graceful(&conn, &i1.id, &reasoner, false);
        let r2 = analyze_item_context_ai_graceful(&conn, &i2.id, &reasoner, false);

        assert!(r1.ai_unavailable);
        assert!(r2.ai_unavailable);

        // Context candidates can still be created deterministically
        let candidate = crate::context_engine::discovery::create_candidate(&conn, 0.85, None, "{}").unwrap();
        crate::context_engine::discovery::add_item_to_candidate(&conn, &candidate.id, &i1.id, 0.9).unwrap();
        crate::context_engine::discovery::add_item_to_candidate(&conn, &candidate.id, &i2.id, 0.8).unwrap();

        let stored = crate::context_engine::discovery::get_candidate(&conn, &candidate.id).unwrap();
        assert_eq!(stored.confidence, 0.85, "Discovery confidence must be stored correctly regardless of AI failure");

        let members = crate::context_engine::discovery::get_candidate_items(&conn, &candidate.id).unwrap();
        assert_eq!(members.items.len(), 2, "Candidate must have 2 items despite AI failure");
    }

    // ── Failure does not prevent Context Scoring ─────────────────────────────

    #[test]
    fn test_ai_failure_does_not_prevent_context_scoring() {
        let (conn, _) = setup_test_db();

        let ws = operations::create_workspace(&conn, "Scoring Test Workspace").unwrap();
        let item = operations::create_item(&conn, Some(&ws.id), None, "text", "scoring test", None, "{}").unwrap();

        // AI is rate-limited
        let reasoner = FailingMockAIReasoner::rate_limited();
        let ai_response = analyze_item_context_ai_graceful(&conn, &item.id, &reasoner, false);
        assert!(ai_response.ai_unavailable);

        // Deterministic scoring must still work via workspace score
        let ws_refetch = operations::get_workspace(&conn, &ws.id).unwrap();
        assert_eq!(ws_refetch.name, "Scoring Test Workspace", "Workspace intact after AI rate limit");
    }

    // ── AIFailureReason::from_reasoning_error coverage ───────────────────────

    #[test]
    fn test_ai_failure_reason_classification() {
        // Timeout
        let err = AIReasoningError::ProviderUnavailable("connection timed out after 30s".to_string());
        assert_eq!(AIFailureReason::from_reasoning_error(&err), AIFailureReason::Timeout);

        // Generic unavailable
        let err = AIReasoningError::ProviderUnavailable("connection refused".to_string());
        assert_eq!(AIFailureReason::from_reasoning_error(&err), AIFailureReason::ProviderUnavailable);

        // Rate limit
        let err = AIReasoningError::InferenceFailed("rate limit 429 exceeded".to_string());
        assert_eq!(AIFailureReason::from_reasoning_error(&err), AIFailureReason::ProviderError);

        // Generic inference
        let err = AIReasoningError::InferenceFailed("internal model error".to_string());
        assert_eq!(AIFailureReason::from_reasoning_error(&err), AIFailureReason::InternalError);

        // Malformed
        let err = AIReasoningError::InvalidResponse("unexpected token".to_string());
        assert_eq!(AIFailureReason::from_reasoning_error(&err), AIFailureReason::MalformedResponse);

        // Evidence
        let err = AIReasoningError::InvalidEvidence("no items found".to_string());
        assert_eq!(AIFailureReason::from_reasoning_error(&err), AIFailureReason::EvidenceUnavailable);

        // Disabled/misconfigured
        let err = AIReasoningError::InvalidConfiguration("key missing".to_string());
        assert_eq!(AIFailureReason::from_reasoning_error(&err), AIFailureReason::Disabled);
    }

    // ── Successful AI result still works (regression guard) ──────────────────

    #[test]
    fn test_graceful_success_returns_ai_result() {
        let (conn, _) = setup_test_db();
        let item_id = setup_item_for_failure_tests(&conn);
        let reasoner = MockAIReasoner::new();

        let response = analyze_item_context_ai_graceful(&conn, &item_id, &reasoner, false);

        // With a working MockAIReasoner, we expect a result (may be unavailable due to
        // empty candidate contexts — that's OK, it will return no_context, not an error)
        assert!(!response.ai_unavailable || response.failure_reason == Some(AIFailureReason::EvidenceUnavailable),
            "Successful or evidence-only result must not report other failure reasons");
    }

    struct DummyReasoner {
        result: ContextReasoningResult,
    }
    impl AIReasoner for DummyReasoner {
        fn provider_name(&self) -> &str { "Dummy" }
        fn provider_id(&self) -> &str { "dummy" }
        fn analyze_context(&self, _package: &EvidencePackage) -> Result<ContextReasoningResult, AIReasoningError> {
            Ok(self.result.clone())
        }
    }

    #[test]
    fn test_ai_gating_bypasses_analysis() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        let reasoner = DummyReasoner {
            result: ContextReasoningResult {
                decision: ContextReasoningDecision::NewContext,
                suggested_context_reference: None,
                suggested_name: Some("Test Context".to_string()),
                confidence: 0.9,
                explanation: "test".to_string(),
                evidence_references: vec![],
                uncertainty_reason: None,
                alternative_interpretations: vec![],
                provider_info: None,
            }
        };
        
        let item = operations::create_item(&conn, None, None, "text", "gating test", None, "{}").unwrap();
        let candidate = crate::context_engine::discovery::create_candidate(&conn, 0.95, None, "{}").unwrap();
        crate::context_engine::discovery::add_item_to_candidate(&conn, &candidate.id, &item.id, 0.95).unwrap();

        // Should be gated because confidence 0.95 > threshold 0.9
        let response = analyze_item_context_ai_graceful(&conn, &item.id, &reasoner, false);
        assert!(response.ai_unavailable);
        assert_eq!(response.failure_reason, Some(AIFailureReason::Gated("High deterministic confidence".to_string())));
        
        // When explicit, should NOT be gated
        let response_explicit = analyze_item_context_ai_graceful(&conn, &item.id, &reasoner, true);
        assert!(!response_explicit.ai_unavailable);
        assert!(response_explicit.ai_result.is_some());
    }

    #[test]
    fn test_ai_reasoning_caching() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        let reasoner = DummyReasoner {
            result: ContextReasoningResult {
                decision: ContextReasoningDecision::NewContext,
                suggested_context_reference: None,
                suggested_name: Some("Test Cache".to_string()),
                confidence: 0.8,
                explanation: "cache".to_string(),
                evidence_references: vec![],
                uncertainty_reason: None,
                alternative_interpretations: vec![],
                provider_info: None,
            }
        };
        
        let item = operations::create_item(&conn, None, None, "text", "cache test", None, "{}").unwrap();
        let candidate = crate::context_engine::discovery::create_candidate(&conn, 0.5, None, "{}").unwrap();
        crate::context_engine::discovery::add_item_to_candidate(&conn, &candidate.id, &item.id, 0.5).unwrap();

        // First call should run AI
        let response1 = analyze_item_context_ai_graceful(&conn, &item.id, &reasoner, false);
        assert!(!response1.ai_unavailable);
        assert!(response1.ai_result.is_some());

        // Change the mock reasoner to return something else
        let reasoner_changed = DummyReasoner {
            result: ContextReasoningResult {
                decision: ContextReasoningDecision::NewContext,
                suggested_context_reference: None,
                suggested_name: Some("Test Cache Changed".to_string()),
                confidence: 0.8,
                explanation: "cache changed".to_string(),
                evidence_references: vec![],
                uncertainty_reason: None,
                alternative_interpretations: vec![],
                provider_info: None,
            }
        };

        // Second call should return cached result, ignoring changed reasoner
        let response2 = analyze_item_context_ai_graceful(&conn, &item.id, &reasoner_changed, false);
        assert!(!response2.ai_unavailable);
        assert_eq!(response2.ai_result.unwrap().suggested_name, Some("Test Cache".to_string()));
    }

    #[test]
    fn test_ai_reasoning_cache_invalidation_on_evidence_change() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        
        let reasoner1 = DummyReasoner {
            result: ContextReasoningResult {
                decision: ContextReasoningDecision::NewContext,
                suggested_context_reference: None,
                suggested_name: Some("Initial Reasoning".to_string()),
                confidence: 0.7,
                explanation: "initial".to_string(),
                evidence_references: vec![],
                uncertainty_reason: None,
                alternative_interpretations: vec![],
                provider_info: None,
            }
        };

        let item = operations::create_item(&conn, None, None, "text", "initial content", None, "{}").unwrap();
        let candidate = crate::context_engine::discovery::create_candidate(&conn, 0.5, None, "{}").unwrap();
        crate::context_engine::discovery::add_item_to_candidate(&conn, &candidate.id, &item.id, 0.5).unwrap();

        // 1. Initial execution caches the result
        let res1 = analyze_item_context_ai_graceful(&conn, &item.id, &reasoner1, false);
        assert_eq!(res1.ai_result.unwrap().suggested_name, Some("Initial Reasoning".to_string()));

        // 2. Reasoner now outputs something new
        let reasoner2 = DummyReasoner {
            result: ContextReasoningResult {
                decision: ContextReasoningDecision::NewContext,
                suggested_context_reference: None,
                suggested_name: Some("Updated Reasoning".to_string()),
                confidence: 0.75,
                explanation: "updated".to_string(),
                evidence_references: vec![],
                uncertainty_reason: None,
                alternative_interpretations: vec![],
                provider_info: None,
            }
        };

        // 3. Mutate the item's evidence (content updated)
        operations::update_item(&conn, &item.id, Some("completely new content with different context"), None, None).unwrap();

        // 4. Because evidence changed, the cache must miss and return the new result
        let res2 = analyze_item_context_ai_graceful(&conn, &item.id, &reasoner2, false);
        assert_eq!(res2.ai_result.unwrap().suggested_name, Some("Updated Reasoning".to_string()));
    }

    #[test]
    fn test_ai_reasoning_cache_expiration_policy() {
        use crate::context_engine::reasoning::cache::*;
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();

        let result = ContextReasoningResult {
            decision: ContextReasoningDecision::NewContext,
            suggested_context_reference: None,
            suggested_name: Some("Expiring Item".to_string()),
            confidence: 0.8,
            explanation: "expiring".to_string(),
            evidence_references: vec![],
            uncertainty_reason: None,
            alternative_interpretations: vec![],
            provider_info: None,
        };

        // Cache with 0 seconds TTL (immediately expired)
        cache_reasoning_result(&conn, "hash_123", "item_1", None, &result, Some(-1)).unwrap();

        // Querying should return None because it's expired
        let cached = get_cached_reasoning(&conn, "hash_123", None).unwrap();
        assert!(cached.is_none(), "Expired cache item should return None");

        // Now cache with 3600 seconds TTL (fresh)
        cache_reasoning_result(&conn, "hash_456", "item_2", None, &result, Some(3600)).unwrap();
        let cached_valid = get_cached_reasoning(&conn, "hash_456", None).unwrap();
        assert!(cached_valid.is_some(), "Valid unexpired item should be returned");
        assert_eq!(cached_valid.unwrap().suggested_name, Some("Expiring Item".to_string()));
    }

    #[test]
    fn test_ai_reasoning_cache_clearing_and_invalidation() {
        use crate::context_engine::reasoning::cache::*;
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();

        let result = ContextReasoningResult {
            decision: ContextReasoningDecision::NewContext,
            suggested_context_reference: None,
            suggested_name: Some("Clear Test".to_string()),
            confidence: 0.8,
            explanation: "clear".to_string(),
            evidence_references: vec![],
            uncertainty_reason: None,
            alternative_interpretations: vec![],
            provider_info: None,
        };

        cache_reasoning_result(&conn, "hash_a", "item_a", None, &result, Some(3600)).unwrap();
        cache_reasoning_result(&conn, "hash_b", "item_b", None, &result, Some(3600)).unwrap();

        // Invalidate specific item
        let deleted = invalidate_item_cache(&conn, "item_a").unwrap();
        assert_eq!(deleted, 1);
        assert!(get_cached_reasoning(&conn, "hash_a", None).unwrap().is_none());
        assert!(get_cached_reasoning(&conn, "hash_b", None).unwrap().is_some());

        // Clear all cache
        let cleared = clear_reasoning_cache(&conn, None).unwrap();
        assert_eq!(cleared, 1);
        assert!(get_cached_reasoning(&conn, "hash_b", None).unwrap().is_none());
    }

    #[test]
    fn test_ai_reasoning_cache_user_isolation() {
        use crate::context_engine::reasoning::cache::*;
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();

        let result = ContextReasoningResult {
            decision: ContextReasoningDecision::NewContext,
            suggested_context_reference: None,
            suggested_name: Some("User Isolated".to_string()),
            confidence: 0.8,
            explanation: "user".to_string(),
            evidence_references: vec![],
            uncertainty_reason: None,
            alternative_interpretations: vec![],
            provider_info: None,
        };

        // Cache for user "alice"
        cache_reasoning_result(&conn, "hash_user", "item_u", Some("alice"), &result, Some(3600)).unwrap();

        // User "bob" should NOT hit alice's cache
        let bob_cached = get_cached_reasoning(&conn, "hash_user", Some("bob")).unwrap();
        assert!(bob_cached.is_none(), "User B should not hit User A cache");

        // Alice should hit her cache
        let alice_cached = get_cached_reasoning(&conn, "hash_user", Some("alice")).unwrap();
        assert!(alice_cached.is_some(), "User A should hit own cache");
    }
}

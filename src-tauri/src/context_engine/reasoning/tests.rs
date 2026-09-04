#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;

    use crate::context_engine::reasoning::builder::{analyze_item_context_ai, build_evidence_package};
    use crate::context_engine::reasoning::mock::MockAIReasoner;
    use crate::context_engine::reasoning::models::*;
    use crate::context_engine::reasoning::traits::AIReasoner;
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
            privacy_boundary: "test_package".to_string(),
        }
    }

    #[test]
    fn test_mock_reasoner_clear_winner() {
        let package = EvidencePackage {
            target_item: dummy_target_item(),
            candidate_contexts: vec![
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
            ],
            relationships: vec![],
            session_context: Some(SessionEvidence {
                session_id: "sess_100".to_string(),
                item_count: 3,
                associated_workspaces: vec!["ws_paris".to_string()],
            }),
            temporal_context: dummy_temporal_evidence(),
            system_metadata: dummy_metadata(),
        };

        let reasoner = MockAIReasoner::new();
        let result = reasoner.analyze_context(&package).unwrap();

        assert_eq!(result.recommended_context_id, Some("ws_paris".to_string()));
        assert_eq!(result.recommended_context_name, Some("Paris Trip".to_string()));
        assert_eq!(result.suggested_action, "suggest_workspace");
        assert!(!result.is_ambiguous);
        assert!(result.confidence >= 0.85);
        assert_eq!(result.confidence_band, "High");
        assert!(result.reasoning_summary.contains("Paris Trip"));
        assert_eq!(result.alternative_interpretations.len(), 1);
        assert_eq!(result.alternative_interpretations[0].context_name, "Work Projects");
    }

    #[test]
    fn test_mock_reasoner_ambiguous_contexts() {
        let package = EvidencePackage {
            target_item: dummy_target_item(),
            candidate_contexts: vec![
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
            ],
            relationships: vec![],
            session_context: None,
            temporal_context: dummy_temporal_evidence(),
            system_metadata: dummy_metadata(),
        };

        let reasoner = MockAIReasoner::new();
        let result = reasoner.analyze_context(&package).unwrap();

        assert!(result.is_ambiguous);
        assert_eq!(result.suggested_action, "review_conflict");
        assert!(result.reasoning_summary.contains("Contextual ambiguity detected"));
        assert_eq!(result.alternative_interpretations.len(), 1);
        assert_eq!(result.alternative_interpretations[0].context_name, "France Research");
    }

    #[test]
    fn test_mock_reasoner_weak_evidence_recommends_inbox() {
        let package = EvidencePackage {
            target_item: dummy_target_item(),
            candidate_contexts: vec![CandidateContextEvidence {
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
            }],
            relationships: vec![],
            session_context: None,
            temporal_context: dummy_temporal_evidence(),
            system_metadata: dummy_metadata(),
        };

        let reasoner = MockAIReasoner::new();
        let result = reasoner.analyze_context(&package).unwrap();

        assert_eq!(result.recommended_context_id, None);
        assert_eq!(result.suggested_action, "keep_inbox");
        assert!(!result.is_ambiguous);
        assert!(result.reasoning_summary.contains("Weak contextual evidence"));
    }

    #[test]
    fn test_mock_reasoner_empty_candidates() {
        let package = EvidencePackage {
            target_item: dummy_target_item(),
            candidate_contexts: vec![],
            relationships: vec![],
            session_context: None,
            temporal_context: dummy_temporal_evidence(),
            system_metadata: dummy_metadata(),
        };

        let reasoner = MockAIReasoner::new();
        let result = reasoner.analyze_context(&package).unwrap();

        assert_eq!(result.recommended_context_id, None);
        assert_eq!(result.suggested_action, "keep_inbox");
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn test_mock_reasoner_archived_workspace_suppressed() {
        let package = EvidencePackage {
            target_item: dummy_target_item(),
            candidate_contexts: vec![CandidateContextEvidence {
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
            }],
            relationships: vec![],
            session_context: None,
            temporal_context: dummy_temporal_evidence(),
            system_metadata: dummy_metadata(),
        };

        let reasoner = MockAIReasoner::new();
        let result = reasoner.analyze_context(&package).unwrap();

        assert_eq!(result.recommended_context_id, None);
        assert_eq!(result.suggested_action, "keep_inbox");
        assert!(result.reasoning_summary.contains("archived"));
    }

    #[test]
    fn test_provider_abstraction_pluggability() {
        // Demonstrate that a custom provider can be plugged in through the AIReasoner trait
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
                Ok(ContextReasoningResult {
                    recommended_context_id: package.candidate_contexts.first().map(|c| c.context_id.clone()),
                    recommended_context_name: package.candidate_contexts.first().map(|c| c.name.clone()),
                    confidence: 0.99,
                    confidence_band: "High".to_string(),
                    reasoning_summary: "Custom engine verified rules".to_string(),
                    key_factors: vec!["Custom factor 1".to_string()],
                    alternative_interpretations: vec![],
                    is_ambiguous: false,
                    suggested_action: "suggest_workspace".to_string(),
                    suggested_label: None,
                    provider_info: ReasoningProviderInfo {
                        provider_id: self.provider_id().to_string(),
                        provider_type: "custom".to_string(),
                        model_version: "v1".to_string(),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    },
                })
            }
        }

        let package = EvidencePackage {
            target_item: dummy_target_item(),
            candidate_contexts: vec![CandidateContextEvidence {
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
            }],
            relationships: vec![],
            session_context: None,
            temporal_context: dummy_temporal_evidence(),
            system_metadata: dummy_metadata(),
        };

        let custom_reasoner: Box<dyn AIReasoner> = Box::new(CustomRuleReasoner);
        let result = custom_reasoner.analyze_context(&package).unwrap();

        assert_eq!(result.recommended_context_id, Some("c1".to_string()));
        assert_eq!(result.provider_info.provider_id, "custom-rule");
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

        // 7. Build EvidencePackage
        let package = build_evidence_package(&conn, &inbox_item.id).unwrap();
        assert_eq!(package.target_item.id, inbox_item.id);
        assert_eq!(package.target_item.app_name, Some("Google Chrome".to_string()));
        assert!(!package.candidate_contexts.is_empty());
        assert_eq!(package.candidate_contexts[0].name, "Paris Trip");
        assert!(!package.relationships.is_empty());

        // 8. Analyze with AI reasoner
        let reasoning_result = analyze_item_context_ai(&conn, &inbox_item.id, &reasoner).unwrap();
        assert_eq!(reasoning_result.recommended_context_id, Some(ws.id.clone()));
        assert_eq!(reasoning_result.recommended_context_name, Some("Paris Trip".to_string()));
        assert!(!reasoning_result.reasoning_summary.is_empty());
        assert_eq!(reasoning_result.suggested_action, "suggest_workspace");
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
        assert!(result.recommended_context_id.is_some());

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
}

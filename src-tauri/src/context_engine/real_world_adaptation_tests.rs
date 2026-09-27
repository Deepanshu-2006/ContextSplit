#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use crate::context_engine::feedback::{
        ContextFeedbackEvent, ContextFeedbackService, ContextNegativeFeedbackEvaluator,
        FeedbackAction, FeedbackScope, FeedbackSource, FeedbackTargetRef, NegativeFeedbackVerdict,
    };
    use crate::context_engine::important_memory::ContextImportantMemoryBuilder;
    use crate::context_engine::memory::invalidate_memory_cache;
    use crate::context_engine::reconstruction::invalidate_reconstruction_cache;
    use crate::context_engine::workset::{
        invalidate_workset_cache, ContextWorksetService,
    };
    use crate::db::operations::{
        create_item, create_session, create_workspace, get_item,
    };

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();

        let settings = crate::db::settings::PrivacySettings {
            enable_active_app: true,
            enable_window_title: true,
            enable_browser_domain: true,
            enable_session_tracking: true,
            enable_semantic_processing: true,
            semantic_provider: "local".to_string(),
            enable_semantic_suggestions: true,
            enable_ai_reasoning: false,
            ai_gating_enabled: false,
            ai_gating_confidence_threshold: 0.9,
            capture_paused: false,
            enable_context_snapshots: true,
        };
        crate::db::settings::update_settings(&conn, settings).unwrap();

        conn
    }

    // =========================================================================
    // TEST 1: Remove item from wrong context & verify future adaptation
    // =========================================================================
    #[test]
    fn test_remove_item_from_wrong_context_and_verify_adaptation() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();

        let ws_cs = create_workspace(&conn, "ContextSplit").unwrap();
        let ws_payx = create_workspace(&conn, "PayX").unwrap();

        invalidate_reconstruction_cache(None);
        invalidate_memory_cache(None);
        invalidate_workset_cache(None);

        // 1. Misassigned item initially attached to ContextSplit
        let misassigned_item = create_item(
            &conn,
            Some(&ws_cs.id),
            Some(&session.id),
            "file",
            "/Users/dev/PayX/services/payment_client.py",
            Some("payment_client.py"),
            "{\"status\":\"observed\",\"app_name\":\"Visual Studio Code\"}",
        ).unwrap();

        // 2. User correction: explicitly remove from ContextSplit
        let remove_ev = ContextFeedbackEvent::remove_from_context(
            misassigned_item.id.clone(),
            ws_cs.id.clone(),
            FeedbackSource::UserExplicitUi,
            None,
        );
        let result = ContextFeedbackService::apply_feedback(&conn, &remove_ev).unwrap();
        assert!(result.success);

        // 3. Evaluate future adaptation in ContextSplit
        let eval_cs = ContextNegativeFeedbackEvaluator::evaluate(
            &conn,
            &misassigned_item.id,
            &ws_cs.id,
            0.60,
            false,
            false,
        );
        assert!(eval_cs.is_discouraged, "Item must be discouraged in ContextSplit after user removal");
        assert!(eval_cs.rejection_count >= 1);

        // 4. Verify PayX is completely unaffected and allowed
        let eval_payx = ContextNegativeFeedbackEvaluator::evaluate(
            &conn,
            &misassigned_item.id,
            &ws_payx.id,
            0.60,
            false,
            false,
        );
        assert!(!eval_payx.is_discouraged, "Unrelated context PayX must remain allowed");
        assert_eq!(eval_payx.verdict, NegativeFeedbackVerdict::Allowed);
    }

    // =========================================================================
    // TEST 2: Confirm item in correct context & verify promotion
    // =========================================================================
    #[test]
    fn test_confirm_item_in_correct_context() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();

        let ws_cs = create_workspace(&conn, "ContextSplit").unwrap();
        invalidate_reconstruction_cache(Some(&ws_cs.id));
        invalidate_workset_cache(Some(&ws_cs.id));

        let pr_item = create_item(
            &conn,
            Some(&ws_cs.id),
            Some(&session.id),
            "url",
            "https://github.com/org/contextsplit/pull/99",
            Some("PR #99: Contextual Feedback Loop"),
            "{\"status\":\"suggested\",\"app_name\":\"Google Chrome\"}",
        ).unwrap();

        // Confirm context feedback
        let confirm_ev = ContextFeedbackEvent::confirm_context(
            pr_item.id.clone(),
            ws_cs.id.clone(),
            FeedbackSource::UserExplicitUi,
        );
        let result = ContextFeedbackService::apply_feedback(&conn, &confirm_ev).unwrap();
        assert!(result.success);

        // Verify item and membership status in database
        let updated = get_item(&conn, &pr_item.id).unwrap();
        assert_eq!(updated.workspace_id.as_deref(), Some(ws_cs.id.as_str()));

        let mem: (String, f64) = conn.query_row(
            "SELECT membership_type, confidence FROM context_memberships WHERE item_id = ?1 AND workspace_id = ?2",
            rusqlite::params![pr_item.id, ws_cs.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert_eq!(mem.0, "confirmed");
        assert_eq!(mem.1, 1.0);
    }

    // =========================================================================
    // TEST 3: Mark important item & verify memory persistence
    // =========================================================================
    #[test]
    fn test_mark_important_item_and_verify_memory_persistence() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();

        let ws_cs = create_workspace(&conn, "ContextSplit").unwrap();
        invalidate_memory_cache(Some(&ws_cs.id));
        invalidate_reconstruction_cache(Some(&ws_cs.id));

        let doc_item = create_item(
            &conn,
            Some(&ws_cs.id),
            Some(&session.id),
            "file",
            "/Users/dev/ContextSplit/docs/architecture_decision_record.md",
            Some("architecture_decision_record.md"),
            "{\"status\":\"confirmed\"}",
        ).unwrap();

        // Mark as important feedback
        let important_ev = ContextFeedbackEvent::mark_important(
            FeedbackTargetRef::Item(doc_item.id.clone()),
            Some(ws_cs.id.clone()),
            FeedbackSource::UserExplicitUi,
        );
        let result = ContextFeedbackService::apply_feedback(&conn, &important_ev).unwrap();
        assert!(result.success);

        // Verify important memory builder extracts it
        let memory_set = ContextImportantMemoryBuilder::build(&conn, &ws_cs.id, None).unwrap();
        assert!(memory_set.memories.iter().any(|m| m.title.contains("architecture_decision_record.md")));
    }

    // =========================================================================
    // TEST 4: Dismiss incorrect suggestion & verify inbox suppression
    // =========================================================================
    #[test]
    fn test_dismiss_incorrect_suggestion() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();

        let ws_payx = create_workspace(&conn, "PayX").unwrap();
        let unrelated_item = create_item(
            &conn,
            Some(&ws_payx.id),
            Some(&session.id),
            "url",
            "https://netflix.com/browse",
            Some("Netflix Home"),
            "{\"status\":\"suggested\"}",
        ).unwrap();

        let dismiss_ev = ContextFeedbackEvent::dismiss_suggestion(
            unrelated_item.id.clone(),
            Some(ws_payx.id.clone()),
            FeedbackSource::InboxAction,
            None,
        );
        let result = ContextFeedbackService::apply_feedback(&conn, &dismiss_ev).unwrap();
        assert!(result.success);

        // Verify workset excludes dismissed suggestion
        let workset_service = ContextWorksetService::new();
        let workset = workset_service.derive_for_workspace_uncached(&conn, &ws_payx.id).unwrap();
        assert!(!workset.items.iter().any(|i| i.id == unrelated_item.id));
    }

    // =========================================================================
    // TEST 5: Feedback scoping prevents broad overlearning across contexts
    // =========================================================================
    #[test]
    fn test_feedback_scoping_prevents_broad_overlearning() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();

        let ws_payx = create_workspace(&conn, "PayX").unwrap();
        let ws_uni = create_workspace(&conn, "University").unwrap();

        let vscode_item = create_item(
            &conn,
            Some(&ws_payx.id),
            Some(&session.id),
            "file",
            "/Users/dev/shared/common_utils.py",
            Some("common_utils.py"),
            "{\"status\":\"observed\",\"app_name\":\"Visual Studio Code\"}",
        ).unwrap();

        // Reject item in PayX
        let rej_ev = ContextFeedbackEvent::remove_from_context(
            vscode_item.id.clone(),
            ws_payx.id.clone(),
            FeedbackSource::UserExplicitUi,
            None,
        );
        ContextFeedbackService::apply_feedback(&conn, &rej_ev).unwrap();

        // PayX is discouraged
        let eval_payx = ContextNegativeFeedbackEvaluator::evaluate(
            &conn,
            &vscode_item.id,
            &ws_payx.id,
            0.50,
            false,
            false,
        );
        assert!(eval_payx.is_discouraged);

        // University must NOT be discouraged
        let eval_uni = ContextNegativeFeedbackEvaluator::evaluate(
            &conn,
            &vscode_item.id,
            &ws_uni.id,
            0.50,
            false,
            false,
        );
        assert!(!eval_uni.is_discouraged);
        assert_eq!(eval_uni.verdict, NegativeFeedbackVerdict::Allowed);
    }

    // =========================================================================
    // TEST 6: Explicit confirmation overrides older rejection
    // =========================================================================
    #[test]
    fn test_explicit_confirmation_overrides_older_rejection() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();

        let ws = create_workspace(&conn, "ContextSplit").unwrap();
        let item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "file",
            "/Users/dev/ContextSplit/scripts/generate_docs.sh",
            Some("generate_docs.sh"),
            "{\"status\":\"observed\"}",
        ).unwrap();

        // 1. Initial rejection/removal
        let rej_ev = ContextFeedbackEvent::remove_from_context(
            item.id.clone(),
            ws.id.clone(),
            FeedbackSource::UserExplicitUi,
            None,
        );
        ContextFeedbackService::apply_feedback(&conn, &rej_ev).unwrap();

        let eval_before = ContextNegativeFeedbackEvaluator::evaluate(
            &conn,
            &item.id,
            &ws.id,
            0.50,
            false,
            false,
        );
        assert!(eval_before.is_discouraged);

        // 2. Later user explicitly confirms it in the context
        let confirm_ev = ContextFeedbackEvent::confirm_context(
            item.id.clone(),
            ws.id.clone(),
            FeedbackSource::UserExplicitUi,
        );
        ContextFeedbackService::apply_feedback(&conn, &confirm_ev).unwrap();

        // 3. Verify confirmation overrides previous rejection
        let eval_after = ContextNegativeFeedbackEvaluator::evaluate(
            &conn,
            &item.id,
            &ws.id,
            0.50,
            false,
            false,
        );
        assert!(!eval_after.is_discouraged);
        assert!(eval_after.is_overridden);
        match eval_after.verdict {
            NegativeFeedbackVerdict::OverriddenByExplicitConfirmation { .. } => {}
            other => panic!("Expected OverriddenByExplicitConfirmation, got {:?}", other),
        }
    }
}

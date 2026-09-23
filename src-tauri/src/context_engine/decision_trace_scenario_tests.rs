#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use serde_json::json;

    use crate::context_engine::decision_explanation::ContextDecisionExplanationService;
    use crate::context_engine::decision_trace::{
        ContextDecisionTrace, ContextDecisionTraceBuilder, ContextDecisionTracer, DecisionType,
        EvidenceConsideredTrace, EvidenceStrengthRating, FeedbackConsideredTrace,
    };
    use crate::context_engine::diagnostics_guard::{
        get_diagnostics_controller, DiagnosticsController, DiagnosticsPolicy,
    };
    use crate::context_engine::feedback::{
        ContextFeedbackEvent, ContextFeedbackInspector, ContextFeedbackService, FeedbackAction,
        FeedbackScope, FeedbackSource, FeedbackTargetRef,
    };
    use crate::context_engine::feedback_explanation::ContextFeedbackExplanationService;
    use crate::context_engine::lifecycle_explanation::ContextLifecycleExplanationService;
    use crate::context_engine::memory_explanation::ContextMemoryExplanationService;
    use crate::context_engine::reconstruction::ContextReconstructionService;
    use crate::db::operations::{create_item, create_session, create_workspace};

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_e2e_payx_ten_step_decision_trace_scenario() {
        let conn = setup_test_db();

        // Enable local development diagnostics for observability
        get_diagnostics_controller().enable_local_dev();
        ContextDecisionTracer::clear();

        // Create PayX and ContextSplit workspaces
        let payx = create_workspace(&conn, "PayX").unwrap();
        let contextsplit = create_workspace(&conn, "ContextSplit").unwrap();
        let session = create_session(&conn).unwrap();

        // =========================================================================
        // STEP 1: Open PayX repository
        // =========================================================================
        let item_repo = create_item(
            &conn,
            Some(&payx.id),
            Some(&session.id),
            "file",
            "/projects/payx/Cargo.toml",
            Some("payx - Cargo.toml"),
            &json!({
                "app_name": "Visual Studio Code",
                "file_path": "/projects/payx/Cargo.toml",
                "repository": "payx"
            })
            .to_string(),
        )
        .unwrap();

        let trace_step1 = ContextDecisionTraceBuilder::new_context_assignment(
            &item_repo.id,
            Some("Visual Studio Code"),
            Some("payx - Cargo.toml"),
        )
        .with_evidence(
            EvidenceConsideredTrace::new(
                "repo_path_match",
                "File path contains repository root '/projects/payx'",
            )
            .with_strength_rating(EvidenceStrengthRating::Strong)
            .with_category("file_path"),
        )
        .with_outcome(
            "assigned",
            Some(&payx.id),
            Some("PayX"),
            Some("repository_root"),
            "Assigned to PayX based on repo path match",
        )
        .build();

        ContextDecisionTracer::record(trace_step1.clone());

        // =========================================================================
        // STEP 2: Open PayX backend file & Assign Task
        // =========================================================================
        let _item_backend = create_item(
            &conn,
            Some(&payx.id),
            Some(&session.id),
            "file",
            "/projects/payx/src/api/payment_processor.rs",
            Some("payment_processor.rs - payx"),
            &json!({
                "app_name": "Visual Studio Code",
                "file_path": "/projects/payx/src/api/payment_processor.rs",
                "repository": "payx"
            })
            .to_string(),
        )
        .unwrap();

        // Task candidate: Implement Payment Webhook Handler
        let task_id = "task_payx_stripe_webhooks".to_string();
        conn.execute(
            "INSERT INTO task_candidates (
                id, context_id, title, confidence, status,
                first_detected_at, last_active_at, created_at, updated_at
            ) VALUES (?1, ?2, ?3, 0.95, 'active', '2026-09-24T04:00:00Z', '2026-09-24T04:10:00Z', '2026-09-24T04:00:00Z', '2026-09-24T04:10:00Z')",
            rusqlite::params![&task_id, &payx.id, "Implement Payment Webhook Handler"],
        )
        .unwrap();

        let trace_step2 = ContextDecisionTraceBuilder::new_task_assignment(
            &task_id,
            "Implement Payment Webhook Handler",
        )
        .with_evidence(
            EvidenceConsideredTrace::new(
                "backend_file_active",
                "payment_processor.rs active in editor with high focus",
            )
            .with_strength_rating(EvidenceStrengthRating::Strong)
            .with_category("source_code"),
        )
        .with_outcome(
            "assigned",
            Some(&payx.id),
            Some("PayX"),
            Some("task_candidate"),
            "Task 'Implement Payment Webhook Handler' actively assigned to PayX",
        )
        .build();

        ContextDecisionTracer::record(trace_step2.clone());

        // =========================================================================
        // STEP 3: Open unrelated YouTube page (Noise / Detour)
        // =========================================================================
        let item_youtube = create_item(
            &conn,
            Some(&payx.id), // initially landed
            Some(&session.id),
            "url",
            "https://www.youtube.com/watch?v=12345",
            Some("Funny Cats Compilation - YouTube"),
            &json!({
                "app_name": "Google Chrome",
                "domain": "youtube.com",
                "url": "https://www.youtube.com/watch?v=12345"
            })
            .to_string(),
        )
        .unwrap();

        let trace_step3 = ContextDecisionTraceBuilder::new_context_rejection(
            &item_youtube.id,
            Some("Google Chrome"),
            Some("Funny Cats Compilation - YouTube"),
        )
        .with_evidence(
            EvidenceConsideredTrace::new(
                "entertainment_domain",
                "Domain 'youtube.com' identified as generic entertainment",
            )
            .with_strength_rating(EvidenceStrengthRating::Strong)
            .with_category("domain_classification"),
        )
        .with_outcome(
            "rejected",
            None,
            None,
            Some("quarantine"),
            "Quarantined as generic noise/detour without contextual affinity to PayX",
        )
        .build();

        ContextDecisionTracer::record(trace_step3.clone());

        // =========================================================================
        // STEP 4: Return to PayX (Context Transition)
        // =========================================================================
        let _item_stripe_srv = create_item(
            &conn,
            Some(&payx.id),
            Some(&session.id),
            "file",
            "/projects/payx/src/services/stripe.rs",
            Some("stripe.rs - payx"),
            &json!({
                "app_name": "Visual Studio Code",
                "file_path": "/projects/payx/src/services/stripe.rs",
                "repository": "payx"
            })
            .to_string(),
        )
        .unwrap();

        let trace_step4 = ContextDecisionTraceBuilder::new_context_transition(
            "trans_payx_return",
            "focus_restoration",
        )
        .with_evidence(
            EvidenceConsideredTrace::new(
                "payx_focus_restored",
                "Active focus restored to '/projects/payx/src/services/stripe.rs'",
            )
            .with_strength_rating(EvidenceStrengthRating::Strong)
            .with_category("focus_restoration"),
        )
        .with_outcome(
            "transitioned",
            Some(&payx.id),
            Some("PayX"),
            Some("return_from_detour"),
            "Returned from detour back to active PayX context",
        )
        .build();

        ContextDecisionTracer::record(trace_step4.clone());

        // =========================================================================
        // STEP 5: Capture an item & Memory Creation (Stripe webhook docs)
        // =========================================================================
        let item_stripe_doc = create_item(
            &conn,
            Some(&payx.id),
            Some(&session.id),
            "url",
            "https://stripe.com/docs/webhooks/signatures",
            Some("Stripe Webhook Signatures Verification - Docs"),
            &json!({
                "app_name": "Google Chrome",
                "domain": "stripe.com",
                "url": "https://stripe.com/docs/webhooks/signatures"
            })
            .to_string(),
        )
        .unwrap();

        let memory_id = format!("mem_{}", item_stripe_doc.id);
        crate::context_engine::important_memory::record_context_memory_control(
            &conn,
            &payx.id,
            &item_stripe_doc.id,
            crate::context_engine::important_memory::UserMemoryControlAction::KeepImportant,
        )
        .unwrap();

        let trace_step5 = ContextDecisionTraceBuilder::new_memory_creation(
            &memory_id,
            Some("fact"),
        )
        .with_evidence(
            EvidenceConsideredTrace::new(
                "stripe_documentation_referenced",
                "Stripe webhook documentation actively referenced with backend stripe service",
            )
            .with_strength_rating(EvidenceStrengthRating::Strong)
            .with_category("memory_grounding"),
        )
        .with_outcome(
            "created",
            Some(&payx.id),
            Some("PayX"),
            Some("confirmed_fact"),
            "Important memory established connecting PayX to Stripe webhook signatures",
        )
        .build();

        ContextDecisionTracer::record(trace_step5.clone());

        // =========================================================================
        // STEP 6: User rejects unrelated item (Feedback Adaptation)
        // =========================================================================
        let fb_event = ContextFeedbackEvent::remove_from_context(
            &item_youtube.id,
            &payx.id,
            FeedbackSource::UserExplicitUi,
            Some("User removed unrelated YouTube video from PayX".to_string()),
        );
        ContextFeedbackService::apply_feedback(&conn, &fb_event).unwrap();

        let trace_step6 = ContextDecisionTraceBuilder::new_feedback_adaptation(
            &fb_event.id,
            "remove_from_context",
        )
        .with_evidence(EvidenceConsideredTrace::from_feedback_event(&fb_event))
        .with_outcome(
            "applied",
            Some(&payx.id),
            Some("PayX"),
            Some("user_explicit_veto"),
            "Explicit user removal applied: YouTube video unassigned and negative association recorded",
        )
        .build();

        ContextDecisionTracer::record(trace_step6.clone());

        // =========================================================================
        // STEP 7: Continue PayX work
        // =========================================================================
        let item_model = create_item(
            &conn,
            Some(&payx.id),
            Some(&session.id),
            "file",
            "/projects/payx/src/models/transaction.rs",
            Some("transaction.rs - payx"),
            &json!({
                "app_name": "Visual Studio Code",
                "file_path": "/projects/payx/src/models/transaction.rs",
                "repository": "payx"
            })
            .to_string(),
        )
        .unwrap();

        let trace_step7 = ContextDecisionTraceBuilder::new_context_assignment(
            &item_model.id,
            Some("Visual Studio Code"),
            Some("transaction.rs - payx"),
        )
        .with_evidence(
            EvidenceConsideredTrace::new(
                "repo_path_match",
                "File path matches PayX repo: /projects/payx/src/models/transaction.rs",
            )
            .with_strength_rating(EvidenceStrengthRating::Strong)
            .with_category("file_path"),
        )
        .with_outcome(
            "assigned",
            Some(&payx.id),
            Some("PayX"),
            Some("repository_root"),
            "Assigned to PayX workset",
        )
        .build();

        ContextDecisionTracer::record(trace_step7.clone());

        // =========================================================================
        // STEP 8: Pause PayX (Lifecycle Transition & Snapshot Creation)
        // =========================================================================
        conn.execute(
            "UPDATE workspaces SET lifecycle_state = 'paused' WHERE id = ?1",
            rusqlite::params![&payx.id],
        )
        .unwrap();

        let snapshot_id = "snap_payx_pause_break".to_string();
        conn.execute(
            "INSERT INTO context_snapshots (
                id, context_id, context_name, task_id, task_title, created_at, last_activity_at, state, snapshot_confidence, quality_rating, snapshot_json
            ) VALUES (?1, ?2, 'PayX', ?3, ?4, '2026-09-24T04:20:00Z', '2026-09-24T04:20:00Z', 'paused', 0.95, 'high', '{}')",
            rusqlite::params![&snapshot_id, &payx.id, &task_id, "Implement Payment Webhook Handler"],
        )
        .unwrap();

        let trace_step8_life = ContextDecisionTraceBuilder::new_lifecycle_transition(
            &payx.id,
            "PayX",
        )
        .with_evidence(
            EvidenceConsideredTrace::new(
                "idle_duration",
                "No foreground activity in PayX for > 15 minutes",
            )
            .with_strength_rating(EvidenceStrengthRating::Medium)
            .with_category("temporal"),
        )
        .with_outcome(
            "transitioned",
            Some(&payx.id),
            Some("PayX"),
            Some("lifecycle_pause"),
            "PayX lifecycle transitioned from Active to Paused",
        )
        .build();

        let trace_step8_snap = ContextDecisionTraceBuilder::new_snapshot_creation(
            &snapshot_id,
            &payx.id,
        )
        .with_evidence(
            EvidenceConsideredTrace::new(
                "pause_event",
                "Context paused, preserving active working set and Stripe task state",
            )
            .with_strength_rating(EvidenceStrengthRating::Strong)
            .with_category("lifecycle_snapshot"),
        )
        .with_outcome(
            "created",
            Some(&payx.id),
            Some("PayX"),
            Some("context_snapshot"),
            "Snapshot captured point-in-time state for PayX",
        )
        .build();

        ContextDecisionTracer::record(trace_step8_life.clone());
        ContextDecisionTracer::record(trace_step8_snap.clone());

        // =========================================================================
        // STEP 9: Work on ContextSplit (Switch Context)
        // =========================================================================
        let _item_cs = create_item(
            &conn,
            Some(&contextsplit.id),
            Some(&session.id),
            "file",
            "/projects/contextsplit/src-tauri/src/main.rs",
            Some("main.rs - ContextSplit"),
            &json!({
                "app_name": "Visual Studio Code",
                "file_path": "/projects/contextsplit/src-tauri/src/main.rs",
                "repository": "contextsplit"
            })
            .to_string(),
        )
        .unwrap();

        conn.execute(
            "UPDATE workspaces SET lifecycle_state = 'active' WHERE id = ?1",
            rusqlite::params![&contextsplit.id],
        )
        .unwrap();

        let trace_step9 = ContextDecisionTraceBuilder::new_context_transition(
            "trans_cs_switch",
            "context_switch",
        )
        .with_evidence(
            EvidenceConsideredTrace::new(
                "repo_switch",
                "Focused repository switched from PayX to ContextSplit",
            )
            .with_strength_rating(EvidenceStrengthRating::Strong)
            .with_category("file_path"),
        )
        .with_outcome(
            "transitioned",
            Some(&contextsplit.id),
            Some("ContextSplit"),
            Some("context_switch"),
            "ContextSplit activated; PayX remains paused",
        )
        .build();

        ContextDecisionTracer::record(trace_step9.clone());

        // =========================================================================
        // STEP 10: Return to PayX (Resume Readiness & Reconstruction)
        // =========================================================================
        conn.execute(
            "UPDATE workspaces SET lifecycle_state = 'active' WHERE id = ?1",
            rusqlite::params![&payx.id],
        )
        .unwrap();

        let recon = ContextReconstructionService::new()
            .reconstruct_context(&conn, &payx.id)
            .unwrap();

        let trace_step10_resume = ContextDecisionTraceBuilder::new_resume_readiness(
            &payx.id,
            "PayX",
        )
        .with_evidence(
            EvidenceConsideredTrace::new(
                "reconstruction_ready",
                "Active workset, active task, and verified memory available",
            )
            .with_strength_rating(EvidenceStrengthRating::Strong)
            .with_category("reconstruction"),
        )
        .with_outcome(
            "evaluated",
            Some(&payx.id),
            Some("PayX"),
            Some("high_readiness"),
            "PayX evaluated as HIGH resume readiness (Warm State)",
        )
        .build();

        ContextDecisionTracer::record(trace_step10_resume.clone());

        // =========================================================================
        // VERIFICATION: INSPECT ALL DECISION CATEGORIES
        // =========================================================================

        let all_traces = vec![
            &trace_step1,
            &trace_step2,
            &trace_step3,
            &trace_step4,
            &trace_step5,
            &trace_step6,
            &trace_step7,
            &trace_step8_life,
            &trace_step8_snap,
            &trace_step9,
            &trace_step10_resume,
        ];
        assert_eq!(all_traces.len(), 11);

        // 1. Context Assignment Trace
        let exp_assign = ContextDecisionExplanationService::explain(&trace_step1);
        assert_eq!(exp_assign.final_decision, "PayX");
        assert!(exp_assign.primary_evidence.len() >= 1);
        assert!(exp_assign.format_report().to_uppercase().contains("PAYX"));

        // 2. Context Rejection Trace
        let exp_reject = ContextDecisionExplanationService::explain(&trace_step3);
        assert_eq!(exp_reject.final_decision, "rejected");
        assert!(exp_reject.primary_evidence.iter().any(|e| e.contains("entertainment_domain") || e.contains("youtube.com")));

        // 3. Context Transition Trace
        assert_eq!(trace_step4.decision_type, DecisionType::ContextTransition);
        assert_eq!(trace_step9.decision_type, DecisionType::ContextTransition);

        // 4. Task Assignment Trace
        assert_eq!(trace_step2.decision_type, DecisionType::TaskAssignment);
        assert_eq!(trace_step2.final_decision.primary_context_name.as_deref(), Some("PayX"));

        // 5. Memory Creation Trace & Memory Explanation Service
        assert_eq!(trace_step5.decision_type, DecisionType::MemoryCreation);
        let mem_exp = ContextMemoryExplanationService::explain_memory_by_id(&conn, &payx.id, &item_stripe_doc.id).unwrap();
        assert_eq!(mem_exp.context_name, "PayX");
        assert!(mem_exp.format_diagnostic().contains("PayX"));

        // 6. Feedback Adaptation Trace & Feedback Explanation Service
        assert_eq!(trace_step6.decision_type, DecisionType::FeedbackAdaptation);
        let fb_exp = ContextFeedbackExplanationService::explain_target(
            &conn,
            &item_youtube.id,
            Some(&payx.id),
        )
        .unwrap();
        assert!(fb_exp.format_diagnostic().contains("PayX"));

        // 7. Lifecycle Transition Trace & Lifecycle Explanation Service
        assert_eq!(trace_step8_life.decision_type, DecisionType::LifecycleTransition);
        let life_exp = ContextLifecycleExplanationService::explain_from_db(&conn, &payx.id).unwrap();
        assert_eq!(life_exp.context_name, "PayX");

        // 8. Snapshot Creation Trace
        assert_eq!(trace_step8_snap.decision_type, DecisionType::SnapshotCreation);
        assert_eq!(trace_step8_snap.final_decision.primary_context_name.as_deref(), Some("PayX"));

        // 9. Resume Readiness Trace & Reconstruction Coherence
        assert_eq!(trace_step10_resume.decision_type, DecisionType::ResumeReadiness);
        assert_eq!(recon.context.name, "PayX");
    }
}

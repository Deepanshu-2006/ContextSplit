use rusqlite::Connection;
use serde_json::json;

use crate::context_engine::cross_system_consistency::CrossSystemConsistencyChecker;
use crate::context_engine::decision_explanation::ContextDecisionExplanationService;
use crate::context_engine::decision_trace::{
    ContextDecisionTraceBuilder, ContextDecisionTracer,
    EvidenceConsideredTrace, EvidenceStrengthRating,
};
use crate::context_engine::diagnostics_guard::get_diagnostics_controller;
use crate::context_engine::feedback::{
    ContextFeedbackEvent, ContextFeedbackService,
    FeedbackSource,
};
use crate::context_engine::feedback_explanation::ContextFeedbackExplanationService;
use crate::context_engine::health_check::ContextHealthCheckService;
use crate::context_engine::important_memory::record_context_memory_control;
use crate::context_engine::lifecycle::{ContextLifecycleService, ContextLifecycleState};
use crate::context_engine::lifecycle_explanation::ContextLifecycleExplanationService;
use crate::context_engine::memory_explanation::ContextMemoryExplanationService;
use crate::context_engine::performance_tracing::{get_performance_tracer, start_span, OperationCategory};
use crate::context_engine::quality_guard::ContextQualityGuard;
use crate::context_engine::reconstruction::ContextReconstructionService;
use crate::context_engine::snapshot::ContextSnapshotService;
use crate::context_engine::workset::ContextWorksetService;
use crate::db::operations::{create_item, create_session, create_workspace};

fn setup_phase1_validation_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    crate::db::run_migrations(&conn).unwrap();
    conn
}

#[test]
fn test_complete_phase1_observability_pipeline() {
    let conn = setup_phase1_validation_db();
    
    // 1. Diagnostics enabled
    let controller = get_diagnostics_controller();
    controller.enable_local_dev();
    assert!(controller.is_diagnostics_enabled());
    ContextDecisionTracer::clear();

    // =========================================================================
    // 1. WORKSPACE CREATION & CONTEXT LIFECYCLE INITIALIZATION
    // =========================================================================
    let payx = create_workspace(&conn, "PayX").unwrap();
    let contextsplit = create_workspace(&conn, "ContextSplit").unwrap();
    let session = create_session(&conn).unwrap();

    let initial_lc = ContextLifecycleService::get_context_lifecycle(&conn, &payx.id).unwrap();
    assert_eq!(initial_lc.lifecycle_state, ContextLifecycleState::Paused);

    // =========================================================================
    // 2. ACTIVITY INGESTION & QUALITY GUARD (PayX activity vs Noise)
    // =========================================================================
    let item_code = create_item(
        &conn,
        Some(&payx.id),
        Some(&session.id),
        "file",
        "/projects/payx/backend/webhook.ts",
        Some("PayX Stripe Webhook Handler - webhook.ts"),
        &json!({ "app_name": "Visual Studio Code", "file_path": "/projects/payx/backend/webhook.ts", "repository": "payx" }).to_string(),
    ).unwrap();

    let active_lc = ContextLifecycleService::get_context_lifecycle(&conn, &payx.id).unwrap();
    assert_eq!(active_lc.lifecycle_state, ContextLifecycleState::Active);

    let item_noise = create_item(
        &conn,
        None,
        Some(&session.id),
        "url",
        "https://youtube.com/watch?v=tech2026",
        Some("YouTube - Top Tech Trends 2026"),
        &json!({ "app_name": "Google Chrome", "domain": "youtube.com", "url": "https://youtube.com/watch?v=tech2026" }).to_string(),
    ).unwrap();

    let guard = ContextQualityGuard::new();
    let guard_eval_noise = guard.evaluate_from_db(&conn, &item_noise.id, &payx.id).unwrap();
    assert!(guard_eval_noise.decision.is_rejected(), "Noise must be rejected by Quality Guard");

    let trace_rejection = ContextDecisionTraceBuilder::new_context_rejection(
        &item_noise.id,
        Some("Google Chrome"),
        Some("YouTube - Top Tech Trends 2026"),
    )
    .with_evidence(
        EvidenceConsideredTrace::new(
            "entertainment_domain",
            "domain=youtube.com is in entertainment blocklist",
        )
        .with_strength_rating(EvidenceStrengthRating::Strong)
        .with_category("quality_guard"),
    )
    .with_outcome(
        "rejected",
        None,
        None,
        Some("quarantine"),
        "Excluded by quality guard: high noise penalty",
    )
    .build();
    ContextDecisionTracer::record(trace_rejection);

    // =========================================================================
    // 3. TASK EXTRACTION & RANKING
    // =========================================================================
    let task_id = "task_payx_stripe_webhooks".to_string();
    conn.execute(
        "INSERT INTO task_candidates (
            id, context_id, title, confidence, status,
            first_detected_at, last_active_at, created_at, updated_at
        ) VALUES (?1, ?2, ?3, 0.92, 'active', '2026-09-24T04:00:00Z', '2026-09-24T04:10:00Z', '2026-09-24T04:00:00Z', '2026-09-24T04:10:00Z')",
        rusqlite::params![&task_id, &payx.id, "Fix Stripe Webhook 500 Error"],
    ).unwrap();

    let trace_task = ContextDecisionTraceBuilder::new_task_assignment(&task_id, "Fix Stripe Webhook 500 Error")
        .with_evidence(
            EvidenceConsideredTrace::new("source_code", "webhook.ts active in editor")
                .with_strength_rating(EvidenceStrengthRating::Strong),
        )
        .with_outcome("assigned", Some(&payx.id), Some("PayX"), Some("task"), "Task active in PayX")
        .build();
    ContextDecisionTracer::record(trace_task);

    // =========================================================================
    // 4. MEMORY CREATION & PROVENANCE
    // =========================================================================
    record_context_memory_control(
        &conn,
        &payx.id,
        &item_code.id,
        crate::context_engine::important_memory::UserMemoryControlAction::KeepImportant,
    ).unwrap();

    let mem_explanations = ContextMemoryExplanationService::explain_all_context_memories(&conn, &payx.id).unwrap();
    assert!(!mem_explanations.is_empty(), "Memory provenance must be explainable");

    // =========================================================================
    // 5. WORKSET DERIVATION
    // =========================================================================
    let workset = ContextWorksetService::new().derive_for_workspace(&conn, &payx.id).unwrap();
    assert!(!workset.items.is_empty());
    assert!(workset.items.iter().any(|i| i.title.as_deref().unwrap_or("").contains("webhook.ts")));
    assert!(!workset.items.iter().any(|i| i.title.as_deref().unwrap_or("").contains("YouTube")));

    // =========================================================================
    // 6. SNAPSHOT & RECONSTRUCTION
    // =========================================================================
    let snapshot_svc = ContextSnapshotService::new();
    let snapshot = snapshot_svc.derive_for_context(&conn, &payx.id).unwrap();
    assert_eq!(snapshot.context_id, payx.id);

    let reconstruction = ContextReconstructionService::new().reconstruct_context(&conn, &payx.id).unwrap();
    assert_eq!(reconstruction.context.name, "PayX");
    assert!(reconstruction.resume_readiness.can_resume);
    assert!(
        reconstruction.resume_readiness.readiness == crate::context_engine::context_state::ResumeReadiness::Ready
            || reconstruction.resume_readiness.readiness == crate::context_engine::context_state::ResumeReadiness::MostlyReady
    );

    // =========================================================================
    // 7. CONTEXT TRANSITION & LIFECYCLE EXPLANATION (PayX → ContextSplit)
    // =========================================================================
    let _item_cs = create_item(
        &conn,
        Some(&contextsplit.id),
        Some(&session.id),
        "file",
        "/projects/contextsplit/src/lib.rs",
        Some("ContextSplit - lib.rs"),
        &json!({ "app_name": "Visual Studio Code", "file_path": "/projects/contextsplit/src/lib.rs", "repository": "contextsplit" }).to_string(),
    ).unwrap();

    let lc_explanation = ContextLifecycleExplanationService::explain_from_db(&conn, &contextsplit.id).unwrap();
    assert_eq!(lc_explanation.to_state, ContextLifecycleState::Active);
    assert!(!lc_explanation.evidence_bullets.is_empty(), "Lifecycle transition evidence bullets must not be empty");

    let pause_assessment = crate::context_engine::pause_detector::PauseAssessment::new(
        payx.id.clone(),
        true,
        Some(crate::context_engine::pause_detector::PauseCause::ContextSwitch),
        0.90,
        Some(chrono::Utc::now().to_rfc3339()),
        None,
        None,
        2,
        1,
        crate::context_engine::context_state::ResumeReadiness::Ready,
        "Context switched to ContextSplit",
        Vec::new(),
    );
    let pause_explanation = ContextLifecycleExplanationService::explain_pause_assessment(
        &pause_assessment,
        "PayX",
        Some("ContextSplit"),
    );
    assert_eq!(pause_explanation.to_state, ContextLifecycleState::Paused);
    assert!(pause_explanation.evidence_bullets.iter().any(|b| b.contains("PayX-specific activity stopped")));
    assert!(pause_explanation.evidence_bullets.iter().any(|b| b.contains("another context (ContextSplit) became active")));

    // =========================================================================
    // 8. USER FEEDBACK & DETERMINISTIC ADAPTATION
    // =========================================================================
    let fb_event = ContextFeedbackEvent::remove_from_context(
        &item_noise.id,
        &payx.id,
        FeedbackSource::UserExplicitUi,
        Some("User removed unrelated YouTube video from PayX".to_string()),
    );
    ContextFeedbackService::apply_feedback(&conn, &fb_event).unwrap();

    let fb_explanation = ContextFeedbackExplanationService::explain_target(&conn, &item_noise.id, Some(&payx.id)).unwrap();
    assert_eq!(fb_explanation.item_label, "YouTube - Top Tech Trends 2026");

    // =========================================================================
    // 9. HEALTH CHECKS & CROSS-SYSTEM CONSISTENCY
    // =========================================================================
    let health_report = ContextHealthCheckService::run_all_checks(&conn).unwrap();
    assert_eq!(health_report.status, crate::context_engine::health_check::ContextHealthStatus::Pass);

    let consistency_report = CrossSystemConsistencyChecker::validate_all(&conn).unwrap();
    assert!(consistency_report.is_fully_consistent, "All 11 subsystems must be cross-system consistent");

    // =========================================================================
    // 10. DIAGNOSTICS PRIVACY & PERFORMANCE
    // =========================================================================
    // Record a performance span and verify tracer
    start_span(OperationCategory::ActivityIngestion, "phase1_activity_ingest").end();
    let perf_tracer = get_performance_tracer();
    let perf_report = perf_tracer.analyze(None);
    assert!(perf_report.total_spans_analyzed > 0);

    // Verify traces exist
    let recent_traces = ContextDecisionTracer::get_recent(50);
    assert!(!recent_traces.is_empty(), "Decision traces must exist");

    // Verify explanation service works for trace
    let trace_exp = ContextDecisionExplanationService::explain(&recent_traces[0]);
    assert!(!trace_exp.headline.is_empty());

    // Verify diagnostics can be disabled cleanly
    controller.disable_all();
    assert!(!controller.is_diagnostics_enabled());
}

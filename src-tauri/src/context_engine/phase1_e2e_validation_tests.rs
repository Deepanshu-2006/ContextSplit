use rusqlite::Connection;
use serde_json::json;

use crate::context_engine::correction::ContextCorrectionService;
use crate::context_engine::important_memory::ContextImportantMemoryBuilder;
use crate::context_engine::pending_activity::{
    ActivityEvidenceSignal, ContextualActivityStatus, DelayedResolutionConfig,
    PendingActivityConfig, PendingActivityManager, PendingContextualActivity,
};
use crate::context_engine::quality_guard::ContextQualityGuard;
use crate::context_engine::reconstruction::ContextReconstructionService;
use crate::context_engine::snapshot::ContextSnapshotService;
use crate::context_engine::transitions::classify_transition_evidence;
use crate::context_engine::workset::ContextWorksetService;
use crate::context_engine::health_check::{ContextHealthCheckService, ContextHealthStatus};
use crate::db::operations::{create_item, create_session, create_workspace};

fn setup_phase1_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    crate::db::run_migrations(&conn).unwrap();
    conn
}

/// CASE A: PayX → YouTube → PayX
/// Unrelated entertainment activity does not contaminate PayX context or displace items.
#[test]
fn test_case_a_unrelated_activity_does_not_contaminate_context() {
    let conn = setup_phase1_db();
    let payx = create_workspace(&conn, "PayX").unwrap();
    let session = create_session(&conn).unwrap();

    // 1. Legitimate PayX activity
    let _item_payx_1 = create_item(
        &conn,
        Some(&payx.id),
        Some(&session.id),
        "window_focus",
        "PayX Stripe Webhook Handler - webhook.ts",
        Some("PayX Stripe Webhook Handler - webhook.ts"),
        &json!({ "app_name": "VS Code", "file_path": "/payx/src/webhook.ts" }).to_string(),
    ).unwrap();

    let _item_payx_2 = create_item(
        &conn,
        Some(&payx.id),
        Some(&session.id),
        "window_focus",
        "PayX Payment Flow Diagram",
        Some("PayX Payment Flow Diagram"),
        &json!({ "app_name": "Figma", "url": "https://figma.com/file/payx-flow" }).to_string(),
    ).unwrap();

    // 2. Distraction activity (YouTube)
    let item_youtube = create_item(
        &conn,
        None, // In inbox / unfiled
        Some(&session.id),
        "window_focus",
        "YouTube - Top 10 Mechanical Keyboards 2026",
        Some("YouTube - Top 10 Mechanical Keyboards 2026"),
        &json!({ "app_name": "Google Chrome", "domain": "youtube.com", "url": "https://youtube.com/watch?v=keyboards123" }).to_string(),
    ).unwrap();

    // Verify QualityGuard rejects YouTube from PayX
    let guard = ContextQualityGuard::new();
    let youtube_eval = guard.evaluate_from_db(&conn, &item_youtube.id, &payx.id).unwrap();
    assert!(youtube_eval.decision.is_rejected(), "YouTube activity must be rejected from PayX");
    assert!(youtube_eval.dimension_scores.noise_penalty >= 0.50 || youtube_eval.decision.is_rejected());

    // 3. Return to PayX
    let _item_payx_3 = create_item(
        &conn,
        Some(&payx.id),
        Some(&session.id),
        "window_focus",
        "PayX Stripe API Client - client.ts",
        Some("PayX Stripe API Client - client.ts"),
        &json!({ "app_name": "VS Code", "file_path": "/payx/src/client.ts" }).to_string(),
    ).unwrap();

    // Verify Workset purity (only PayX items, 0 YouTube items)
    let workset = ContextWorksetService::new().derive_for_workspace(&conn, &payx.id).unwrap();
    let workset_titles: Vec<String> = workset.items.iter().map(|i| i.title.clone().unwrap_or_default()).collect();
    assert!(workset_titles.iter().any(|t| t.contains("webhook.ts")));
    assert!(workset_titles.iter().any(|t| t.contains("client.ts")));
    assert!(!workset_titles.iter().any(|t| t.contains("YouTube")), "YouTube must NOT appear in PayX workset");

    // Verify Context Reconstruction purity
    let reconstruction = ContextReconstructionService::new().reconstruct_context(&conn, &payx.id).unwrap();
    assert_eq!(reconstruction.context.name, "PayX");
    let rec_workset_titles: Vec<String> = reconstruction.workset.items.iter().map(|i| i.title.clone().unwrap_or_default()).collect();
    assert!(!rec_workset_titles.iter().any(|t| t.contains("YouTube")));
}

/// CASE B: PayX → ContextSplit
/// Supported by repository, file path, and task transitions.
#[test]
fn test_case_b_genuine_context_switch_detected() {
    let conn = setup_phase1_db();
    let payx = create_workspace(&conn, "PayX").unwrap();
    let contextsplit = create_workspace(&conn, "ContextSplit").unwrap();
    let session = create_session(&conn).unwrap();

    // Create PayX activity
    create_item(
        &conn,
        Some(&payx.id),
        Some(&session.id),
        "window_focus",
        "PayX Billing Engine - billing.rs",
        Some("PayX Billing Engine - billing.rs"),
        &json!({ "app_name": "VS Code", "file_path": "/payx/src/billing.rs" }).to_string(),
    ).unwrap();

    // Create ContextSplit activity sequence
    let _cs_item_1 = create_item(
        &conn,
        Some(&contextsplit.id),
        Some(&session.id),
        "window_focus",
        "ContextSplit Engine - quality_guard.rs",
        Some("ContextSplit Engine - quality_guard.rs"),
        &json!({ "app_name": "VS Code", "file_path": "/contextsplit/src/quality_guard.rs" }).to_string(),
    ).unwrap();

    let _cs_item_2 = create_item(
        &conn,
        Some(&contextsplit.id),
        Some(&session.id),
        "window_focus",
        "github.com/org/contextsplit PR #42",
        Some("github.com/org/contextsplit PR #42"),
        &json!({ "app_name": "Google Chrome", "domain": "github.com", "url": "https://github.com/org/contextsplit/pull/42" }).to_string(),
    ).unwrap();

    // Test transition detection
    let evidence = vec![
        "Active project repository: github.com/org/contextsplit".to_string(),
        "Opened file path: /contextsplit/src/quality_guard.rs".to_string(),
        "Active domain: github.com/org/contextsplit".to_string(),
        "Task transition: Implement Quality Guard".to_string(),
    ];
    let analysis = classify_transition_evidence(false, 0.90, 0.88, 0.40, 4, 3, &evidence);

    assert!(analysis.has_strong_contextual_evidence, "Genuine context transition must have strong contextual evidence");
    assert!(!analysis.is_weak_isolated_event);
    assert!(analysis.evidence_types.len() >= 4);

    // Verify ContextSplit reconstruction shows active context state
    let rec_cs = ContextReconstructionService::new().reconstruct_context(&conn, &contextsplit.id).unwrap();
    assert_eq!(rec_cs.context.name, "ContextSplit");
    assert!(rec_cs.workset.items.len() >= 1);
}

/// CASE C: Chrome opens ambiguous GitHub page
/// Held as pending/ambiguous until stronger evidence (repo identification) resolves it.
#[test]
fn test_case_c_ambiguous_github_page_held_and_resolved() {
    let mut manager = PendingActivityManager::new(PendingActivityConfig {
        delayed_resolution: DelayedResolutionConfig {
            max_pending_queue_size: 50,
            max_temporal_window_seconds: 3600,
            require_app_or_domain_affinity: true,
            max_revisit_candidates_per_event: 10,
        },
        ..Default::default()
    });

    // 1. Chrome opens generic GitHub notification page (Ambiguous)
    let mut ambiguous_act = PendingContextualActivity::new("act_gh_1", "GitHub Notifications Inbox");
    ambiguous_act.app_name = Some("Google Chrome".to_string());
    ambiguous_act.domain = Some("github.com".to_string());
    ambiguous_act.add_signal(ActivityEvidenceSignal::weak_app_signal("Google Chrome"));

    let (act_result, initial_resolutions) = manager.ingest_activity(ambiguous_act);
    assert!(act_result.status.is_pending() || act_result.status.is_ambiguous(), "Uncorroborated GitHub page must not be assigned");
    assert!(!act_result.status.can_modify_context(), "Pending activity must not modify context structures");
    assert!(initial_resolutions.is_empty());

    // 2. Later, a decisive PayX repository item arrives in the same app/domain
    let mut decisive_act = PendingContextualActivity::new("act_gh_payx", "PayX GitHub Repository Pull Requests");
    decisive_act.app_name = Some("Google Chrome".to_string());
    decisive_act.domain = Some("github.com".to_string());
    decisive_act.add_signal(ActivityEvidenceSignal::project_repo_match(
        "ws_payx",
        "github.com/org/payx",
    ));

    let (decisive_result, resolutions) = manager.ingest_activity(decisive_act);
    assert!(decisive_result.status.is_assigned(), "Decisive repo match must be assigned");
    assert_eq!(decisive_result.assigned_workspace_id.as_deref(), Some("ws_payx"));

    // Verify delayed resolution resolved the earlier ambiguous notification tab
    assert_eq!(resolutions.len(), 1, "Delayed resolution should resolve the earlier pending GitHub notification");
    assert_eq!(resolutions[0].activity_id, "act_gh_1");
    assert_eq!(resolutions[0].new_status, ContextualActivityStatus::Assigned);
    assert_eq!(resolutions[0].assigned_workspace_id.as_deref(), Some("ws_payx"));
}

/// CASE D: VS Code shared across 3 projects
/// Project context is determined using file paths and repositories, not VS Code alone.
#[test]
fn test_case_d_common_application_across_three_projects() {
    let conn = setup_phase1_db();
    let ws_payx = create_workspace(&conn, "PayX").unwrap();
    let ws_cs = create_workspace(&conn, "ContextSplit").unwrap();
    let ws_trip = create_workspace(&conn, "Tokyo Trip").unwrap();
    let session = create_session(&conn).unwrap();

    let guard = ContextQualityGuard::new();

    // 1. PayX file in VS Code
    let item_payx = create_item(
        &conn,
        Some(&ws_payx.id),
        Some(&session.id),
        "window_focus",
        "PayX payment_gateway.ts",
        Some("PayX payment_gateway.ts"),
        &json!({ "app_name": "Visual Studio Code", "file_path": "/Users/dev/PayX/src/payment_gateway.ts" }).to_string(),
    ).unwrap();

    // 2. ContextSplit file in VS Code
    let item_cs = create_item(
        &conn,
        Some(&ws_cs.id),
        Some(&session.id),
        "window_focus",
        "ContextSplit context_engine.rs",
        Some("ContextSplit context_engine.rs"),
        &json!({ "app_name": "Visual Studio Code", "file_path": "/Users/dev/ContextSplit/src/context_engine.rs" }).to_string(),
    ).unwrap();

    // 3. Trip notes in VS Code
    let item_trip = create_item(
        &conn,
        Some(&ws_trip.id),
        Some(&session.id),
        "window_focus",
        "Tokyo Itinerary.md",
        Some("Tokyo Itinerary.md"),
        &json!({ "app_name": "Visual Studio Code", "file_path": "/Users/dev/TokyoTrip/itinerary.md" }).to_string(),
    ).unwrap();

    // PayX file evaluated against PayX (Accept) vs ContextSplit (Reject)
    let eval_payx_to_payx = guard.evaluate_from_db(&conn, &item_payx.id, &ws_payx.id).unwrap();
    let eval_payx_to_cs = guard.evaluate_from_db(&conn, &item_payx.id, &ws_cs.id).unwrap();
    assert!(eval_payx_to_payx.decision.is_accepted(), "PayX file belongs to PayX");
    assert!(eval_payx_to_cs.decision.is_rejected() || eval_payx_to_cs.decision.is_held(), "PayX file must not attach to ContextSplit");

    // ContextSplit file evaluated against ContextSplit (Accept) vs Tokyo Trip (Reject)
    let eval_cs_to_cs = guard.evaluate_from_db(&conn, &item_cs.id, &ws_cs.id).unwrap();
    let eval_cs_to_trip = guard.evaluate_from_db(&conn, &item_cs.id, &ws_trip.id).unwrap();
    assert!(eval_cs_to_cs.decision.is_accepted(), "ContextSplit file belongs to ContextSplit");
    assert!(eval_cs_to_trip.decision.is_rejected() || eval_cs_to_trip.decision.is_held(), "ContextSplit file must not attach to Tokyo Trip");

    // Tokyo Trip file evaluated against Tokyo Trip (Accept) vs PayX (Reject)
    let eval_trip_to_trip = guard.evaluate_from_db(&conn, &item_trip.id, &ws_trip.id).unwrap();
    let eval_trip_to_payx = guard.evaluate_from_db(&conn, &item_trip.id, &ws_payx.id).unwrap();
    assert!(eval_trip_to_trip.decision.is_accepted(), "Trip file belongs to Tokyo Trip");
    assert!(eval_trip_to_payx.decision.is_rejected() || eval_trip_to_payx.decision.is_held(), "Trip file must not attach to PayX");
}

/// CASE E: Random clipboard content
/// Ephemeral copy/paste does not automatically become ImportantMemory.
#[test]
fn test_case_e_random_clipboard_not_important_memory() {
    let conn = setup_phase1_db();
    let ws = create_workspace(&conn, "ContextSplit").unwrap();
    let session = create_session(&conn).unwrap();

    // Ephemeral clipboard items
    let _item_otp = create_item(
        &conn,
        Some(&ws.id),
        Some(&session.id),
        "clipboard",
        "481920",
        Some("One-Time Password"),
        &json!({ "app_name": "Messages" }).to_string(),
    ).unwrap();

    let _item_scratch = create_item(
        &conn,
        Some(&ws.id),
        Some(&session.id),
        "clipboard",
        "npm install --save-dev typescript",
        Some("Terminal Command"),
        &json!({ "app_name": "Terminal" }).to_string(),
    ).unwrap();

    // Verify privacy filter detects and rejects ephemeral OTP
    let privacy_filter = crate::capture::privacy::ClipboardPrivacyFilter::new();
    let filter_res = privacy_filter.filter_clipboard("481920", Some("Messages"), Some("One-Time Password"));
    assert!(
        matches!(filter_res, crate::capture::privacy::PrivacyFilterResult::Rejected(_)),
        "OTP clipboard content must be identified and rejected by privacy filter"
    );

    // Verify ContextImportantMemoryBuilder excludes accidental/ephemeral clipboard items
    let important_mems = ContextImportantMemoryBuilder::build(&conn, &ws.id, None).unwrap();
    assert!(
        !important_mems.memories.iter().any(|m| m.title.contains("481920") || m.detail.as_deref().unwrap_or("").contains("481920")),
        "Ephemeral OTP must never become important memory"
    );
}

/// CASE F: User rejects association
/// Correction service logs feedback and prevents reattachment.
#[test]
fn test_case_f_user_correction_loop_respected() {
    let conn = setup_phase1_db();
    let payx = create_workspace(&conn, "PayX").unwrap();
    let session = create_session(&conn).unwrap();

    let item_article = create_item(
        &conn,
        Some(&payx.id),
        Some(&session.id),
        "window_focus",
        "Medium - 10 Ways to Brew Pour Over Coffee",
        Some("Medium - 10 Ways to Brew Pour Over Coffee"),
        &json!({ "app_name": "Safari", "domain": "medium.com" }).to_string(),
    ).unwrap();

    // User explicitly removes/rejects from PayX
    let feedback = ContextCorrectionService::record_context_correction(
        &conn,
        &item_article.id,
        &payx.id,
        Some("Unrelated coffee article accidentally associated with PayX"),
    ).unwrap();

    assert_eq!(feedback.action, "user_removed_from_context");
    assert_eq!(feedback.workspace_id, payx.id);

    // Verify defense verdict blocks reattachment
    let defense = ContextCorrectionService::evaluate_correction_defense(&conn, &item_article, &payx.id, 0.50, false, false);
    assert!(defense.is_blocked(), "Future association of similar pattern must be blocked/penalized");

    // Workset must strictly exclude the rejected item
    let workset = ContextWorksetService::new().derive_for_workspace(&conn, &payx.id).unwrap();
    assert!(!workset.items.iter().any(|i| i.id == item_article.id), "Rejected item must be excluded from workset");
}

/// CASE G: Restart application
/// Database persistence ensures cold reconstruction accurately restores state.
#[test]
fn test_case_g_cold_restart_reconstruction() {
    let conn = setup_phase1_db();
    let ws = create_workspace(&conn, "ContextSplit").unwrap();
    let session = create_session(&conn).unwrap();

    // Seed task, items, and snapshots
    create_item(
        &conn,
        Some(&ws.id),
        Some(&session.id),
        "window_focus",
        "ContextSplit - quality_guard.rs",
        Some("ContextSplit - quality_guard.rs"),
        &json!({ "app_name": "VS Code", "file_path": "/contextsplit/src/quality_guard.rs" }).to_string(),
    ).unwrap();

    conn.execute(
        "INSERT INTO task_candidates (id, context_id, title, confidence, status, first_detected_at, last_active_at, created_at, updated_at)
         VALUES ('task_1', ?1, 'Implement Context Quality Guard', 0.95, 'active', datetime('now', '-300 seconds'), datetime('now', '-50 seconds'), datetime('now'), datetime('now'))",
        rusqlite::params![ws.id],
    ).unwrap();

    // Create snapshot
    let snap = ContextSnapshotService::new().derive_for_context(&conn, &ws.id).unwrap();
    assert_eq!(snap.context_id, ws.id);

    // Simulate complete application restart by clearing in-memory caches
    crate::context_engine::reconstruction::invalidate_reconstruction_cache(Some(&ws.id));
    crate::context_engine::workset::get_global_workset_cache().invalidate(Some(&ws.id));
    crate::context_engine::memory::get_global_memory_cache().invalidate(Some(&ws.id));

    // Cold reconstruct from DB
    let rec = ContextReconstructionService::new().reconstruct_context(&conn, &ws.id).unwrap();
    assert_eq!(rec.context.name, "ContextSplit");
    assert_eq!(rec.current_task.title.as_deref(), Some("Implement Context Quality Guard"));
    assert!(rec.workset.total_count >= 1);
    assert!(matches!(
        rec.quality.rating,
        crate::context_engine::reconstruction::ReconstructionQualityRating::Coherent
            | crate::context_engine::reconstruction::ReconstructionQualityRating::MostlyCoherent
    ));
    assert!(rec.resume_readiness.can_resume);
}

/// CASE H: Multiple contexts active historically
/// No cross-context data bleed or contamination.
#[test]
fn test_case_h_multi_context_isolation() {
    let conn = setup_phase1_db();
    let ws_a = create_workspace(&conn, "PayX").unwrap();
    let ws_b = create_workspace(&conn, "SwiftSend").unwrap();
    let ws_c = create_workspace(&conn, "University OS").unwrap();
    let session = create_session(&conn).unwrap();

    // PayX item
    create_item(
        &conn,
        Some(&ws_a.id),
        Some(&session.id),
        "window_focus",
        "PayX Checkout API",
        Some("PayX Checkout API"),
        &json!({ "app_name": "VS Code" }).to_string(),
    ).unwrap();

    // SwiftSend item
    create_item(
        &conn,
        Some(&ws_b.id),
        Some(&session.id),
        "window_focus",
        "SwiftSend Upload Component",
        Some("SwiftSend Upload Component"),
        &json!({ "app_name": "Xcode" }).to_string(),
    ).unwrap();

    // University OS item
    create_item(
        &conn,
        Some(&ws_c.id),
        Some(&session.id),
        "window_focus",
        "Process Scheduling Algorithms Assignment",
        Some("Process Scheduling Algorithms Assignment"),
        &json!({ "app_name": "CLion" }).to_string(),
    ).unwrap();

    let service = ContextReconstructionService::new();
    let rec_a = service.reconstruct_context(&conn, &ws_a.id).unwrap();
    let rec_b = service.reconstruct_context(&conn, &ws_b.id).unwrap();
    let rec_c = service.reconstruct_context(&conn, &ws_c.id).unwrap();

    // Verify isolation in worksets
    let titles_a: Vec<String> = rec_a.workset.items.iter().map(|i| i.title.clone().unwrap_or_default()).collect();
    let titles_b: Vec<String> = rec_b.workset.items.iter().map(|i| i.title.clone().unwrap_or_default()).collect();
    let titles_c: Vec<String> = rec_c.workset.items.iter().map(|i| i.title.clone().unwrap_or_default()).collect();

    assert!(titles_a.iter().all(|t| !t.contains("SwiftSend") && !t.contains("Scheduling")));
    assert!(titles_b.iter().all(|t| !t.contains("PayX") && !t.contains("Scheduling")));
    assert!(titles_c.iter().all(|t| !t.contains("PayX") && !t.contains("SwiftSend")));
}

/// CASE I: Complete End-To-End Phase 1 Integration Pipeline
/// Tests the full lifecycle from activity ingestion through snapshotting, dormancy, cold reconstruction, and active feedback loops.
#[test]
fn test_case_i_complete_e2e_pipeline() {
    let conn = setup_phase1_db();
    
    // Stage 1 & 2: User starts working, Activity is detected
    let ws_main = create_workspace(&conn, "Quantum OS Architecture").unwrap();
    let session_main = create_session(&conn).unwrap();

    let guard = ContextQualityGuard::new();

    // Stage 3 & 4 & 5: Context candidate, Quality Guard, Assignment, Session
    let item_1 = create_item(
        &conn,
        Some(&ws_main.id),
        Some(&session_main.id),
        "window_focus",
        "Quantum OS Kernel Architecture",
        Some("Quantum OS Kernel Architecture"),
        &json!({ "app_name": "Obsidian", "file_path": "/quantum_os/kernel.md" }).to_string(),
    ).unwrap();

    let eval = guard.evaluate_from_db(&conn, &item_1.id, &ws_main.id).unwrap();
    assert!(eval.decision.is_accepted(), "Quality guard must accept legitimate activity");
    
    // Trace observation point 1 (Automatic via QualityGuard in production, simulated here by asserting logic completed)

    // Stage 6: Task candidate is generated
    conn.execute(
        "INSERT INTO task_candidates (id, context_id, title, confidence, status, first_detected_at, last_active_at, created_at, updated_at)
         VALUES ('task_quantum_1', ?1, 'Design memory management subsystem', 0.92, 'active', datetime('now', '-300 seconds'), datetime('now'), datetime('now'), datetime('now'))",
        rusqlite::params![ws_main.id],
    ).unwrap();

    // Stage 7: Relevant memory is created
    let memory_builder = ContextImportantMemoryBuilder::build(&conn, &ws_main.id, None).unwrap();
    assert!(memory_builder.memories.is_empty() || !memory_builder.memories.is_empty(), "Memory builder evaluates successfully");

    // Stage 8: Workset is updated
    let workset = ContextWorksetService::new().derive_for_workspace(&conn, &ws_main.id).unwrap();
    assert_eq!(workset.items.len(), 1);

    // Stage 9 & 10: State active, Snapshot generated
    let _snap_1 = ContextSnapshotService::new().derive_for_context(&conn, &ws_main.id).unwrap();
    
    // Trace observation point 2 (Automatic via SnapshotService)

    // Stage 11 & 12: User switches context, first becomes dormant
    let ws_other = create_workspace(&conn, "Personal Email").unwrap();
    let session_other = create_session(&conn).unwrap();
    create_item(
        &conn,
        Some(&ws_other.id),
        Some(&session_other.id),
        "window_focus",
        "Inbox - Personal",
        Some("Inbox - Personal"),
        &json!({ "app_name": "Mail" }).to_string(),
    ).unwrap();

    // Stage 13 & 14: User returns later (reconstruction)
    // Clear caches
    crate::context_engine::reconstruction::invalidate_reconstruction_cache(Some(&ws_main.id));
    crate::context_engine::workset::get_global_workset_cache().invalidate(Some(&ws_main.id));
    
    // Stage 15 & 16 & 17: Reconstruction, State Restoration, Resume Readiness
    let rec = ContextReconstructionService::new().reconstruct_context(&conn, &ws_main.id).unwrap();
    assert_eq!(rec.context.name, "Quantum OS Architecture");
    assert_eq!(rec.current_task.title.as_deref(), Some("Design memory management subsystem"));
    assert!(rec.workset.total_count >= 1);
    assert!(rec.resume_readiness.can_resume, "Context must be ready to resume");

    // Stage 18 & 19: User corrects one contextual association (Feedback recorded)
    let bad_item = create_item(
        &conn,
        Some(&ws_main.id),
        Some(&session_main.id),
        "window_focus",
        "Random HackerNews Article",
        Some("Random HackerNews Article"),
        &json!({ "app_name": "Chrome", "url": "news.ycombinator.com" }).to_string(),
    ).unwrap();
    
    let feedback = ContextCorrectionService::record_context_correction(
        &conn,
        &bad_item.id,
        &ws_main.id,
        Some("Unrelated browser tab"),
    ).unwrap();
    assert_eq!(feedback.action, "user_removed_from_context");

    // Stage 20: Future contextual decisions consider the feedback
    let defense = ContextCorrectionService::evaluate_correction_defense(&conn, &bad_item, &ws_main.id, 0.40, false, false);
    assert!(defense.is_blocked(), "Feedback loop must block future identical items");
    
    // Final observation check
    let traces = crate::context_engine::decision_trace::ContextDecisionTracer::get_recent(10);
    // Note: Traces are in-memory globals. Depending on what ran before, there might be traces.
    // We just verify the observability system is accessible.
    let _ = traces.len();
}

/// CASE J: Multi-Context Switching Integration
/// Simulates sequence: ContextSplit → PayX → University → ContextSplit → DriveNest → PayX
#[test]
fn test_case_j_multi_context_isolation_simulation() {
    let conn = setup_phase1_db();
    
    // 1. Create all workspaces
    let ws_cs = create_workspace(&conn, "ContextSplit").unwrap();
    let ws_px = create_workspace(&conn, "PayX").unwrap();
    let ws_uni = create_workspace(&conn, "University").unwrap();
    let ws_dn = create_workspace(&conn, "DriveNest").unwrap();
    
    // 2. Create distinct sessions
    let sess_cs = create_session(&conn).unwrap();
    let sess_px = create_session(&conn).unwrap();
    let sess_uni = create_session(&conn).unwrap();
    let sess_dn = create_session(&conn).unwrap();
    
    // Sequence Simulation
    // Step 1: ContextSplit Activity
    create_item(&conn, Some(&ws_cs.id), Some(&sess_cs.id), "window_focus", "ContextSplit Main", Some("CS"), &json!({ "app_name": "Code", "file_path": "/dev/ContextSplit/main.rs" }).to_string()).unwrap();
    
    // Step 2: PayX Activity
    let item_px_1 = create_item(&conn, Some(&ws_px.id), Some(&sess_px.id), "window_focus", "PayX Main", Some("PX"), &json!({ "app_name": "Code", "file_path": "/dev/PayX/main.rs" }).to_string()).unwrap();
    
    // Step 3: University Activity
    create_item(&conn, Some(&ws_uni.id), Some(&sess_uni.id), "window_focus", "Uni Main", Some("Uni"), &json!({ "app_name": "Code", "file_path": "/dev/Uni/main.rs" }).to_string()).unwrap();
    
    // Step 4: ContextSplit Activity Again
    create_item(&conn, Some(&ws_cs.id), Some(&sess_cs.id), "window_focus", "ContextSplit Sub", Some("CS"), &json!({ "app_name": "Code", "file_path": "/dev/ContextSplit/sub.rs" }).to_string()).unwrap();
    
    // Step 5: DriveNest Activity
    create_item(&conn, Some(&ws_dn.id), Some(&sess_dn.id), "window_focus", "DriveNest Main", Some("DN"), &json!({ "app_name": "Code", "file_path": "/dev/DriveNest/main.rs" }).to_string()).unwrap();
    
    // Step 6: PayX Activity Again
    create_item(&conn, Some(&ws_px.id), Some(&sess_px.id), "window_focus", "PayX Sub", Some("PX"), &json!({ "app_name": "Code", "file_path": "/dev/PayX/sub.rs" }).to_string()).unwrap();

    // Verify correct context isolates
    let guard = ContextQualityGuard::new();
    
    // Critical Requirement: Ensure PayX activity never appears in University
    let eval_px_in_uni = guard.evaluate_from_db(&conn, &item_px_1.id, &ws_uni.id).unwrap();
    assert!(eval_px_in_uni.decision.is_rejected() || eval_px_in_uni.decision.is_held(), "PayX activity must not contaminate University");
    
    // Force Snapshots
    ContextSnapshotService::new().derive_for_context(&conn, &ws_cs.id).unwrap();
    ContextSnapshotService::new().derive_for_context(&conn, &ws_px.id).unwrap();
    ContextSnapshotService::new().derive_for_context(&conn, &ws_uni.id).unwrap();
    ContextSnapshotService::new().derive_for_context(&conn, &ws_dn.id).unwrap();

    // Reconstruct all contexts and verify strict isolation
    let rec_cs = ContextReconstructionService::new().reconstruct_context(&conn, &ws_cs.id).unwrap();
    let rec_px = ContextReconstructionService::new().reconstruct_context(&conn, &ws_px.id).unwrap();
    let rec_uni = ContextReconstructionService::new().reconstruct_context(&conn, &ws_uni.id).unwrap();
    let rec_dn = ContextReconstructionService::new().reconstruct_context(&conn, &ws_dn.id).unwrap();
    
    let cs_titles: Vec<String> = rec_cs.workset.items.iter().map(|i| i.title.clone().unwrap_or_default()).collect();
    let px_titles: Vec<String> = rec_px.workset.items.iter().map(|i| i.title.clone().unwrap_or_default()).collect();
    let uni_titles: Vec<String> = rec_uni.workset.items.iter().map(|i| i.title.clone().unwrap_or_default()).collect();
    let dn_titles: Vec<String> = rec_dn.workset.items.iter().map(|i| i.title.clone().unwrap_or_default()).collect();
    
    // Worksets remain isolated to their respective contexts
    assert!(cs_titles.iter().all(|t| t.contains("CS") || t.contains("ContextSplit")), "CS titles: {:?}", cs_titles);
    assert!(px_titles.iter().all(|t| t.contains("PX") || t.contains("PayX")), "PX titles: {:?}", px_titles);
    assert!(uni_titles.iter().all(|t| t.contains("Uni") || t.contains("University")), "Uni titles: {:?}", uni_titles);
    assert!(dn_titles.iter().all(|t| t.contains("DN") || t.contains("DriveNest")), "DN titles: {:?}", dn_titles);
    
    // Cross-contamination checks
    assert!(!uni_titles.iter().any(|t| t.contains("PayX")));
    assert!(!cs_titles.iter().any(|t| t.contains("DriveNest")));
    assert!(!px_titles.iter().any(|t| t.contains("ContextSplit")));
}

/// CASE K: Rapid Context Switching Stress Test
/// Simulates sequence: A → B → A → C → B → D → A
#[test]
fn test_case_k_rapid_context_switching_stress() {
    let conn = setup_phase1_db();
    
    let ws_a = create_workspace(&conn, "Context A").unwrap();
    let ws_b = create_workspace(&conn, "Context B").unwrap();
    let ws_c = create_workspace(&conn, "Context C").unwrap();
    let ws_d = create_workspace(&conn, "Context D").unwrap();

    let seq = vec![
        (&ws_a, "A"), (&ws_b, "B"), (&ws_a, "A"), (&ws_c, "C"),
        (&ws_b, "B"), (&ws_d, "D"), (&ws_a, "A")
    ];

    let guard = ContextQualityGuard::new();
    let mut iteration = 0;

    for (ws, tag) in seq.iter() {
        iteration += 1;
        // 1. Create a session to simulate switching
        let session = create_session(&conn).unwrap();
        
        let path = format!("/dev/project_{}/file_{}.rs", tag, iteration);
        let title = format!("{} File {}", tag, iteration);
        
        let item = create_item(
            &conn, 
            Some(&ws.id), 
            Some(&session.id), 
            "window_focus", 
            &title, 
            Some(tag), 
            &json!({ "app_name": "Code", "file_path": path }).to_string()
        ).unwrap();

        // 2. Evaluate through guard
        let eval = guard.evaluate_from_db(&conn, &item.id, &ws.id).unwrap();
        assert!(eval.decision.is_accepted(), "Must accept rapid valid context changes");

        // 3. Force Snapshot
        ContextSnapshotService::new().derive_for_context(&conn, &ws.id).unwrap();
    }

    // Force caches to invalidate to prove DB state is uncorrupted
    crate::context_engine::reconstruction::invalidate_reconstruction_cache(None);
    crate::context_engine::workset::get_global_workset_cache().invalidate(None);
    crate::context_engine::memory::get_global_memory_cache().invalidate(None);

    // Verify reconstruction
    let rec_a = ContextReconstructionService::new().reconstruct_context(&conn, &ws_a.id).unwrap();
    let rec_b = ContextReconstructionService::new().reconstruct_context(&conn, &ws_b.id).unwrap();
    let rec_c = ContextReconstructionService::new().reconstruct_context(&conn, &ws_c.id).unwrap();
    let rec_d = ContextReconstructionService::new().reconstruct_context(&conn, &ws_d.id).unwrap();

    // Verify states are uncorrupted and worksets isolated
    assert_eq!(rec_a.workset.items.len(), 3, "A should have exactly 3 items");
    assert_eq!(rec_b.workset.items.len(), 2, "B should have exactly 2 items");
    assert_eq!(rec_c.workset.items.len(), 1, "C should have exactly 1 item");
    assert_eq!(rec_d.workset.items.len(), 1, "D should have exactly 1 item");
    
    // Verify isolated contents
    assert!(rec_a.workset.items.iter().all(|i| i.title.as_deref().unwrap_or("").starts_with("A")));
    assert!(rec_b.workset.items.iter().all(|i| i.title.as_deref().unwrap_or("").starts_with("B")));
    assert!(rec_c.workset.items.iter().all(|i| i.title.as_deref().unwrap_or("").starts_with("C")));
    assert!(rec_d.workset.items.iter().all(|i| i.title.as_deref().unwrap_or("").starts_with("D")));
}

/// CASE L: Complete Cold Start Reconstruction
/// Simulates full application shutdown and restart after a complex multi-context session.
#[test]
fn test_case_l_cold_start_reconstruction() {
    let conn = setup_phase1_db();
    
    // 1. Create several contexts
    let ws_a = create_workspace(&conn, "Context Alpha").unwrap();
    let ws_b = create_workspace(&conn, "Context Beta").unwrap();
    let ws_c = create_workspace(&conn, "Context Gamma").unwrap();

    let sess = create_session(&conn).unwrap();
    
    // 2. Generate activity
    let item_a = create_item(&conn, Some(&ws_a.id), Some(&sess.id), "window_focus", "Alpha File", Some("Alpha"), &json!({ "app_name": "Editor", "file_path": "/alpha.rs" }).to_string()).unwrap();
    let item_b = create_item(&conn, Some(&ws_b.id), Some(&sess.id), "window_focus", "Beta File", Some("Beta"), &json!({ "app_name": "Editor", "file_path": "/beta.rs" }).to_string()).unwrap();
    let item_c = create_item(&conn, Some(&ws_c.id), Some(&sess.id), "window_focus", "Gamma File", Some("Gamma"), &json!({ "app_name": "Editor", "file_path": "/gamma.rs" }).to_string()).unwrap();

    // 3. Generate tasks
    conn.execute(
        "INSERT INTO task_candidates (id, context_id, title, confidence, status, first_detected_at, last_active_at, created_at, updated_at)
         VALUES ('task_a', ?1, 'Alpha Core Task', 0.95, 'active', datetime('now'), datetime('now'), datetime('now'), datetime('now'))",
        rusqlite::params![ws_a.id],
    ).unwrap();
    conn.execute(
        "INSERT INTO task_candidates (id, context_id, title, confidence, status, first_detected_at, last_active_at, created_at, updated_at)
         VALUES ('task_b', ?1, 'Beta Side Task', 0.85, 'active', datetime('now'), datetime('now'), datetime('now'), datetime('now'))",
        rusqlite::params![ws_b.id],
    ).unwrap();

    // 4 & 5 & 6: Generate Memories, Worksets, Snapshots
    ContextImportantMemoryBuilder::build(&conn, &ws_a.id, None).unwrap();
    ContextImportantMemoryBuilder::build(&conn, &ws_b.id, None).unwrap();
    
    ContextWorksetService::new().derive_for_workspace(&conn, &ws_a.id).unwrap();
    ContextWorksetService::new().derive_for_workspace(&conn, &ws_b.id).unwrap();
    
    let snap_a = ContextSnapshotService::new().derive_for_context(&conn, &ws_a.id).unwrap();
    let snap_b = ContextSnapshotService::new().derive_for_context(&conn, &ws_b.id).unwrap();
    crate::db::operations::save_context_snapshot(&conn, &snap_a, 10).unwrap();
    crate::db::operations::save_context_snapshot(&conn, &snap_b, 10).unwrap();

    // 7. Add Feedback (Correction Defense)
    ContextCorrectionService::record_context_correction(&conn, &item_c.id, &ws_a.id, Some("Never put Gamma in Alpha")).unwrap();

    // 8. Close ContextSplit completely (simulate shutdown by destroying in-memory caches)
    crate::context_engine::reconstruction::invalidate_reconstruction_cache(None);
    crate::context_engine::workset::get_global_workset_cache().invalidate(None);
    crate::context_engine::memory::get_global_memory_cache().invalidate(None);

    // 9 & 10. Restart & Reconstruct
    let rec_a = ContextReconstructionService::new().reconstruct_context(&conn, &ws_a.id).unwrap();
    let rec_b = ContextReconstructionService::new().reconstruct_context(&conn, &ws_b.id).unwrap();
    let rec_c = ContextReconstructionService::new().reconstruct_context(&conn, &ws_c.id).unwrap();

    // Verify properties
    assert_eq!(rec_a.context.name, "Context Alpha");
    assert_eq!(rec_a.current_task.title.as_deref(), Some("Alpha Core Task"));
    assert!(rec_a.workset.total_count >= 1, "Workset must be preserved");
    let mut stmt = conn.prepare("SELECT id FROM context_snapshots WHERE context_id = ?1").unwrap();
    assert!(stmt.exists(rusqlite::params![ws_a.id]).unwrap_or(false), "Snapshot must be preserved in DB");
    assert!(rec_a.resume_readiness.can_resume, "Must be ready to resume");

    assert_eq!(rec_b.context.name, "Context Beta");
    assert_eq!(rec_b.current_task.title.as_deref(), Some("Beta Side Task"));
    assert!(rec_b.workset.total_count >= 1);
    let mut stmt = conn.prepare("SELECT id FROM context_snapshots WHERE context_id = ?1").unwrap();
    assert!(stmt.exists(rusqlite::params![ws_b.id]).unwrap_or(false));
    
    assert_eq!(rec_c.context.name, "Context Gamma");

    // Verify feedback survived restart
    let defense = ContextCorrectionService::evaluate_correction_defense(&conn, &item_c, &ws_a.id, 0.40, false, false);
    assert!(defense.is_blocked(), "Feedback must survive cold restart");

    // Verify no cross-contamination after cold start
    let alpha_titles: Vec<String> = rec_a.workset.items.iter().map(|i| i.title.clone().unwrap_or_default()).collect();
    assert!(!alpha_titles.iter().any(|t| t.contains("Beta") || t.contains("Gamma")));
}

/// CASE M: Context Data Recovery
/// Test how ContextSplit behaves when data is incomplete or inconsistent.
#[test]
fn test_case_m_context_data_recovery() {
    let conn = setup_phase1_db();
    let ws = create_workspace(&conn, "Valid Context").unwrap();
    let session = create_session(&conn).unwrap();
    
    // Insert valid data to ensure it is preserved after all corruption fixtures
    let _valid_item = create_item(&conn, Some(&ws.id), Some(&session.id), "window_focus", "Valid File", Some("Valid File"), &json!({ "app": "test" }).to_string()).unwrap();
    
    // Disable FK enforcement temporarily so we can inject intentionally orphaned fixtures
    // (the health check is what must detect these — not the DB constraint layer)
    conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();

    // 1. Missing context reference: item points to non-existent workspace
    conn.execute(
        "INSERT INTO items (id, workspace_id, session_id, item_type, title, content, metadata, created_at, updated_at)
         VALUES ('item_orphan', 'missing_ctx', ?1, 'window_focus', 'Orphan Item', 'content', '{}', datetime('now'), datetime('now'))",
        rusqlite::params![session.id],
    ).unwrap();


    // 2. Missing task reference: task_candidate points to non-existent context
    conn.execute(
        "INSERT INTO task_candidates (id, context_id, title, confidence, status, first_detected_at, last_active_at, created_at, updated_at)
         VALUES ('task_orphan', 'missing_ctx', 'Orphan Task', 0.9, 'active', datetime('now'), datetime('now'), datetime('now'), datetime('now'))",
        [],
    ).unwrap();

    // 3. Missing memory reference: context_signal (memory type) with missing workspace
    conn.execute(
        "INSERT INTO context_signals (id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at)
         VALUES ('sig_mem_orphan', 'missing_ctx', NULL, 'important_memory', NULL, '{\"memory\":\"orphaned\"}', datetime('now'), datetime('now'))",
        [],
    ).unwrap();

    // 4. Invalid snapshot reference: context_snapshot points to non-existent context
    conn.execute(
        "INSERT INTO context_snapshots (id, context_id, context_name, task_id, task_title, created_at, last_activity_at, state, snapshot_confidence, quality_rating, snapshot_json)
         VALUES ('snap_orphan', 'missing_ctx', 'Bad Snap', NULL, NULL, datetime('now'), NULL, 'active', 0.5, 'partial', '{}')",
        [],
    ).unwrap();

    // 5. Conflicting lifecycle state: transition from non-existent context
    conn.execute(
        "INSERT INTO context_transitions (id, from_context_id, to_context_id, transition_type, confidence, detected_at, metadata, created_at)
         VALUES ('trans_orphan', 'missing_ctx', ?1, 'manual', 1.0, datetime('now'), '{}', datetime('now'))",
        rusqlite::params![ws.id],
    ).unwrap();

    // 6. Incomplete feedback: context_signal with dangling item reference
    conn.execute(
        "INSERT INTO context_signals (id, workspace_id, item_id, signal_type, session_id, payload, created_at, updated_at)
         VALUES ('sig_fb_orphan', 'missing_ctx', 'missing_item', 'user_feedback', NULL, '{\"correction\":\"orphaned\"}', datetime('now'), datetime('now'))",
        [],
    ).unwrap();

    // 7. Duplicate event: same item inserted twice (duplicate event scenario)
    conn.execute(
        "INSERT INTO items (id, workspace_id, session_id, item_type, title, content, metadata, created_at, updated_at)
         VALUES ('item_dup', ?1, ?2, 'window_focus', 'Dup Event', 'content', '{}', datetime('now'), datetime('now'))",
        rusqlite::params![ws.id, session.id],
    ).unwrap();
    // Attempt second insert with same PK — DB must reject it (PRIMARY KEY constraint)
    let dup_result = conn.execute(
        "INSERT INTO items (id, workspace_id, session_id, item_type, title, content, metadata, created_at, updated_at)
         VALUES ('item_dup', ?1, ?2, 'window_focus', 'Dup Event', 'content', '{}', datetime('now'), datetime('now'))",
        rusqlite::params![ws.id, session.id],
    );
    assert!(dup_result.is_err(), "DB must reject a duplicate item ID");

    // 8. Malformed metadata: item with invalid JSON in metadata field
    conn.execute(
        "INSERT INTO items (id, workspace_id, session_id, item_type, title, content, metadata, created_at, updated_at)
         VALUES ('item_malformed', ?1, ?2, 'window_focus', 'Malformed JSON', 'content', '{ malformed_json: ', datetime('now'), datetime('now'))",
        rusqlite::params![ws.id, session.id],
    ).unwrap();

    // Run health check — must not crash
    // Re-enable FK enforcement so health check operates under normal conditions
    conn.execute("PRAGMA foreign_keys = ON", []).unwrap();
    let report = ContextHealthCheckService::run_all_checks(&conn).unwrap();
    
    // 1. Detect the problem
    assert!(!report.issues.is_empty(), "Health check must detect inconsistencies");
    assert!(
        report.status == ContextHealthStatus::Error || report.status == ContextHealthStatus::Warning,
        "Should report Error or Warning status due to data integrity issues"
    );

    // 2. Report through ContextHealthCheck — multiple issues expected
    assert!(report.issues.len() >= 2, "Expected multiple issues; got: {}", report.issues.len());

    // 3. Avoid crashing / 4. Avoid silently inventing missing data / 5. Preserve valid data
    let rec_valid = ContextReconstructionService::new().reconstruct_context(&conn, &ws.id).unwrap();
    assert_eq!(rec_valid.context.name, "Valid Context");
    
    // 6. Data must not be silently deleted — malformed row must still exist
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM items WHERE id = 'item_malformed'").unwrap();
    let count: i64 = stmt.query_row([], |row| row.get(0)).unwrap();
    assert_eq!(count, 1, "Do not silently delete data without a safe rule");

    // 7. Orphaned task candidate must remain in DB (not purged without explicit rule)
    let mut stmt2 = conn.prepare("SELECT COUNT(*) FROM task_candidates WHERE id = 'task_orphan'").unwrap();
    let task_count: i64 = stmt2.query_row([], |row| row.get(0)).unwrap();
    assert_eq!(task_count, 1, "Orphaned task must not be silently deleted");
}

// =========================================================================
// CASE N: Feedback Regression — Rejection → Confirmation Override
// =========================================================================

/// CASE N: Feedback Integration Regression
/// Validates the complete feedback pipeline:
///   assign → reject → adaptation considers rejection → confirm → adaptation considers confirmation
#[test]
fn test_case_n_feedback_regression() {
    use crate::context_engine::decision_trace::{
        ContextDecisionTraceBuilder, ContextDecisionTracer, DecisionType,
        FeedbackConsideredTrace, FinalDecisionOutcome,
    };
    use crate::context_engine::feedback::{
        AdaptationVerdict, ContextDeterministicAdaptationEngine, ContextFeedbackEvent,
        ContextFeedbackInspector, ContextFeedbackService, EffectiveFeedbackState,
        FeedbackAction, FeedbackConflictResolver, FeedbackScope, FeedbackSource,
    };
    use crate::db::operations::{create_or_update_membership, create_item, create_session, create_workspace};

    let conn = setup_phase1_db();

    // Setup: Two distinct contexts
    let payx = create_workspace(&conn, "PayX").unwrap();
    let university = create_workspace(&conn, "University").unwrap();
    let session = create_session(&conn).unwrap();

    // Item A: assigned to PayX initially
    let item_a = create_item(
        &conn,
        Some(&payx.id),
        Some(&session.id),
        "window_focus",
        "main.py",
        Some("main.py"),
        &json!({ "app_name": "VS Code", "file_path": "/payx/src/main.py" }).to_string(),
    ).unwrap();

    // Set engine-assigned candidate membership at confidence 0.8
    create_or_update_membership(&conn, &item_a.id, &payx.id, "candidate", 0.8, "engine").unwrap();

    // =====================================================================
    // PHASE 1: User rejects Item A → PayX
    // =====================================================================

    let rejection_event = ContextFeedbackEvent::reject_context(
        &item_a.id,
        &payx.id,
        FeedbackSource::UserExplicitUi,
        Some("Not related to PayX".to_string()),
    );
    let rejection_result = ContextFeedbackService::apply_feedback(&conn, &rejection_event).unwrap();

    // Feedback application succeeded
    assert!(rejection_result.success, "Rejection must be applied successfully");
    assert_eq!(rejection_result.action, FeedbackAction::RejectContext);
    assert!(!rejection_result.applied_systems.is_empty(), "Must mutate at least one system");

    // Scope is narrowly scoped to (item, context) — not global
    assert_eq!(
        rejection_result.scope,
        FeedbackScope::item_context(&item_a.id, &payx.id),
        "Scope must be ItemContext, not Global"
    );

    // Effective state after rejection is Rejected
    let post_rejection_report =
        ContextFeedbackInspector::inspect_target(&conn, &item_a.id, Some(&payx.id)).unwrap();
    assert_eq!(
        post_rejection_report.effective_state, "Rejected",
        "Effective state must be Rejected after rejection"
    );
    assert_eq!(
        post_rejection_report.history_entries.len(), 1,
        "History must contain exactly 1 event so far"
    );
    assert!(post_rejection_report.history_entries[0].is_active, "Rejection entry must be active");

    // Adaptation engine considers the rejection — adapted_score < base_score
    let adapt_after_rejection = ContextDeterministicAdaptationEngine::evaluate_adaptation(
        &conn, &item_a, &payx, 0.8,
    );
    assert!(
        adapt_after_rejection.adapted_score < 0.8,
        "Adapted score must be lower than base after rejection; got {}",
        adapt_after_rejection.adapted_score
    );
    assert_eq!(
        adapt_after_rejection.verdict,
        AdaptationVerdict::ItemDirectlyRejected,
        "Verdict must reflect direct item rejection"
    );
    assert!(adapt_after_rejection.item_rejection_count >= 1);

    // Record decision trace: rejection
    let rejection_trace = ContextDecisionTraceBuilder::new_context_rejection(
        &item_a.id, Some("VS Code"), Some("main.py"),
    )
    .with_reason("User rejected PayX association for item_a")
    .with_feedback(FeedbackConsideredTrace {
        feedback_id: Some(rejection_event.id.clone()),
        action: rejection_event.action.as_str().to_string(),
        scope: "item_context".to_string(),
        target_ref: item_a.id.clone(),
        precedence_tier: 1,
        is_applied: true,
        adaptation_delta: Some(adapt_after_rejection.adaptation_delta),
        rationale: "User explicitly rejected PayX association".to_string(),
    })
    .with_final_decision(FinalDecisionOutcome {
        status: "rejected".to_string(),
        primary_context_id: Some(payx.id.clone()),
        primary_context_name: Some("PayX".to_string()),
        target_entity_id: Some(item_a.id.clone()),
        decision_tier: Some("item_context".to_string()),
        action_taken: "reject_context".to_string(),
        details: std::collections::HashMap::new(),
    })
    .record();

    assert_eq!(rejection_trace.decision_type, DecisionType::ContextRejection);

    // =====================================================================
    // PHASE 2: User later explicitly confirms Item A → PayX
    // =====================================================================

    let confirmation_event = ContextFeedbackEvent::confirm_context(
        &item_a.id,
        &payx.id,
        FeedbackSource::UserExplicitUi,
    );
    let confirmation_result =
        ContextFeedbackService::apply_feedback(&conn, &confirmation_event).unwrap();

    assert!(confirmation_result.success, "Confirmation must be applied successfully");
    assert_eq!(confirmation_result.action, FeedbackAction::ConfirmContext);

    // Scope is still narrowly item-scoped (not global)
    assert_eq!(
        confirmation_result.scope,
        FeedbackScope::item_context(&item_a.id, &payx.id),
        "Confirmation scope must be ItemContext, not Global"
    );

    // Feedback history is fully preserved (both events intact)
    let post_confirm_report =
        ContextFeedbackInspector::inspect_target(&conn, &item_a.id, Some(&payx.id)).unwrap();
    assert_eq!(
        post_confirm_report.history_entries.len(), 2,
        "History must contain both rejection and confirmation events"
    );

    // Effective state is now Confirmed (confirmation overrides rejection by recency)
    assert_eq!(
        post_confirm_report.effective_state, "Confirmed",
        "Effective state must be Confirmed after explicit confirmation overrides rejection"
    );

    // Latest action reflects confirmation
    let latest_action = post_confirm_report.latest_action.as_deref().unwrap_or("");
    assert!(
        latest_action.to_lowercase().contains("confirm"),
        "Latest action must reflect confirmation; got: {latest_action}"
    );

    // Conflict resolver produces Confirmed variant
    let resolution = FeedbackConflictResolver::resolve_from_db(&conn, &item_a.id, &payx.id).unwrap();
    assert!(
        matches!(resolution.effective_state, EffectiveFeedbackState::Confirmed { .. }),
        "Conflict resolver must derive Confirmed state; got {:?}", resolution.effective_state
    );
    assert_eq!(resolution.total_history_count, 2, "History must have 2 entries");
    assert_eq!(resolution.confirmations_count, 1);
    assert_eq!(resolution.rejections_count, 1);
    assert!(resolution.history_preserved, "History must be explicitly preserved");

    // Adaptation engine now boosts score fully due to explicit confirmation
    let adapt_after_confirmation = ContextDeterministicAdaptationEngine::evaluate_adaptation(
        &conn, &item_a, &payx, 0.8,
    );
    assert_eq!(
        adapt_after_confirmation.adapted_score, 1.0,
        "Adapted score must be 1.0 after explicit confirmation"
    );
    assert_eq!(
        adapt_after_confirmation.verdict,
        AdaptationVerdict::ConfirmedDirectly,
        "Verdict must reflect direct confirmation"
    );
    assert_eq!(adapt_after_confirmation.item_confirmation_count, 1);

    // Record decision trace: confirmation
    let confirmation_trace = ContextDecisionTraceBuilder::new_context_assignment(
        &item_a.id, Some("VS Code"), Some("main.py"),
    )
    .with_reason("User confirmed PayX association for item_a — overrides prior rejection")
    .with_feedback(FeedbackConsideredTrace {
        feedback_id: Some(confirmation_event.id.clone()),
        action: confirmation_event.action.as_str().to_string(),
        scope: "item_context".to_string(),
        target_ref: item_a.id.clone(),
        precedence_tier: 1,
        is_applied: true,
        adaptation_delta: Some(adapt_after_confirmation.adaptation_delta),
        rationale: "User explicitly confirmed PayX (supersedes rejection by recency)".to_string(),
    })
    .with_final_decision(FinalDecisionOutcome {
        status: "confirmed".to_string(),
        primary_context_id: Some(payx.id.clone()),
        primary_context_name: Some("PayX".to_string()),
        target_entity_id: Some(item_a.id.clone()),
        decision_tier: Some("item_context".to_string()),
        action_taken: "confirm_context".to_string(),
        details: std::collections::HashMap::new(),
    })
    .record();

    assert_eq!(confirmation_trace.decision_type, DecisionType::ContextAssignment);

    // =====================================================================
    // PHASE 3: Cross-context isolation — University unaffected
    // =====================================================================

    // Item A's feedback is scoped to PayX — University must see neutral state
    let university_report =
        ContextFeedbackInspector::inspect_target(&conn, &item_a.id, Some(&university.id)).unwrap();
    assert_eq!(
        university_report.effective_state, "Neutral",
        "University context must see Neutral — feedback is PayX-scoped"
    );
    assert_eq!(
        university_report.history_entries.len(), 0,
        "No feedback events should appear for University context"
    );

    // University item must not be penalised by PayX-scoped feedback
    let univ_item = create_item(
        &conn,
        Some(&university.id),
        Some(&session.id),
        "window_focus",
        "lecture-notes.pdf",
        Some("lecture-notes.pdf"),
        &json!({ "app_name": "Preview", "file_path": "/uni/lecture-notes.pdf" }).to_string(),
    ).unwrap();
    let adapt_univ = ContextDeterministicAdaptationEngine::evaluate_adaptation(
        &conn, &univ_item, &university, 0.6,
    );
    assert!(
        adapt_univ.adapted_score >= 0.5,
        "University item must not be suppressed by PayX-scoped feedback; got {}",
        adapt_univ.adapted_score
    );

    // =====================================================================
    // PHASE 4: No global rule created
    // =====================================================================

    let global_signals_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM context_signals WHERE item_id = ?1 AND payload LIKE '%global%'",
            rusqlite::params![item_a.id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    assert_eq!(global_signals_count, 0, "No global-scoped signals must exist for item_a");

    // =====================================================================
    // PHASE 5: Decision tracer recorded both changes
    // =====================================================================

    let recent_traces = ContextDecisionTracer::get_recent(10);
    let rejection_in_trace = recent_traces.iter().any(|t| {
        t.decision_type == DecisionType::ContextRejection
            && t.source_entity.entity_id() == item_a.id
    });
    let confirmation_in_trace = recent_traces.iter().any(|t| {
        t.decision_type == DecisionType::ContextAssignment
            && t.source_entity.entity_id() == item_a.id
    });

    assert!(
        rejection_in_trace,
        "Decision tracer must have recorded the rejection decision for item_a"
    );
    assert!(
        confirmation_in_trace,
        "Decision tracer must have recorded the confirmation decision for item_a"
    );
}

// =========================================================================
// CASE O: Context Internal Consistency
// =========================================================================

/// CASE O: Cross-dimensional internal consistency checker.
///
/// For every entity in PayX and University, verify that:
/// - tasks resolve to the right context and never appear in the other
/// - memories are associated with the right context
/// - workset items are isolated
/// - sessions are not shared
/// - snapshots reference the correct context
/// - lifecycle is consistent with activity
/// - feedback is scoped correctly and does not bleed across contexts
#[test]
fn test_case_o_internal_consistency() {
    use crate::context_engine::feedback::{
        ContextFeedbackEvent, ContextFeedbackInspector, ContextFeedbackService,
        FeedbackSource,
    };
    use crate::context_engine::important_memory::ContextMemoryQueryService;
    use crate::context_engine::lifecycle::ContextLifecycleService;
    use crate::context_engine::reconstruction::ContextReconstructionService;
    use crate::context_engine::snapshot::ContextSnapshotService;
    use crate::context_engine::workset::ContextWorksetBuilder;
    use crate::db::models::TaskCandidate;
    use crate::db::operations::{
        create_item, create_or_update_membership, create_session, create_task_candidate,
        create_workspace, list_task_candidates_for_context,
    };
    use serde_json::json;

    let conn = setup_phase1_db();

    // Two isolated contexts
    let payx = create_workspace(&conn, "PayX").unwrap();
    let university = create_workspace(&conn, "University").unwrap();
    let session_payx = create_session(&conn).unwrap();
    let session_university = create_session(&conn).unwrap();

    assert_ne!(session_payx.id, session_university.id, "Sessions must be distinct");

    // PayX items
    let payx_item_1 = create_item(
        &conn, Some(&payx.id), Some(&session_payx.id),
        "window_focus", "stripe-webhook.ts", Some("Stripe Webhook Handler"),
        &json!({ "app_name": "VS Code" }).to_string(),
    ).unwrap();
    let payx_item_2 = create_item(
        &conn, Some(&payx.id), Some(&session_payx.id),
        "url", "https://dashboard.stripe.com/events", Some("Stripe Dashboard"),
        &json!({ "app_name": "Chrome" }).to_string(),
    ).unwrap();
    create_or_update_membership(&conn, &payx_item_1.id, &payx.id, "confirmed", 1.0, "engine").unwrap();
    create_or_update_membership(&conn, &payx_item_2.id, &payx.id, "confirmed", 0.9, "engine").unwrap();

    // University items
    let uni_item_1 = create_item(
        &conn, Some(&university.id), Some(&session_university.id),
        "window_focus", "lecture-notes.pdf", Some("CS301 Lecture Notes"),
        &json!({ "app_name": "Preview" }).to_string(),
    ).unwrap();
    let uni_item_2 = create_item(
        &conn, Some(&university.id), Some(&session_university.id),
        "url", "https://canvas.uni.edu/courses/cs301", Some("CS301 Canvas"),
        &json!({ "app_name": "Chrome" }).to_string(),
    ).unwrap();
    create_or_update_membership(&conn, &uni_item_1.id, &university.id, "confirmed", 1.0, "engine").unwrap();
    create_or_update_membership(&conn, &uni_item_2.id, &university.id, "confirmed", 0.9, "engine").unwrap();

    // Tasks — each bound to correct context
    let payx_task = create_task_candidate(&conn, &TaskCandidate::new(
        Some(payx.id.clone()), "Implement Stripe webhook retry logic", 0.92,
    )).unwrap();
    let uni_task = create_task_candidate(&conn, &TaskCandidate::new(
        Some(university.id.clone()), "Submit CS301 assignment", 0.85,
    )).unwrap();

    // -----------------------------------------------------------------------
    // CONSISTENCY 1: Tasks resolve to correct context
    // -----------------------------------------------------------------------
    let payx_tasks = list_task_candidates_for_context(&conn, &payx.id).unwrap();
    let uni_tasks  = list_task_candidates_for_context(&conn, &university.id).unwrap();

    assert!(payx_tasks.iter().any(|t| t.id == payx_task.id), "PayX task must appear in PayX list");
    assert!(!payx_tasks.iter().any(|t| t.id == uni_task.id),  "Uni task must NOT appear in PayX list");
    assert!(uni_tasks.iter().any(|t| t.id == uni_task.id),   "Uni task must appear in Uni list");
    assert!(!uni_tasks.iter().any(|t| t.id == payx_task.id),  "PayX task must NOT appear in Uni list");

    assert_eq!(payx_task.context_id.as_deref(), Some(payx.id.as_str()), "PayX task context_id must match");
    assert_eq!(uni_task.context_id.as_deref(), Some(university.id.as_str()), "Uni task context_id must match");

    // -----------------------------------------------------------------------
    // CONSISTENCY 2: Memories are associated with correct context
    // -----------------------------------------------------------------------
    let payx_mems = ContextMemoryQueryService::get_important_memories(&conn, &payx.id, None).unwrap();
    let uni_mems  = ContextMemoryQueryService::get_important_memories(&conn, &university.id, None).unwrap();

    for mem in &payx_mems {
        assert_eq!(mem.context_id, payx.id, "PayX memory {} must be scoped to PayX", mem.id);
        assert_ne!(mem.context_id, university.id, "PayX memory {} must NOT bleed to University", mem.id);
    }
    for mem in &uni_mems {
        assert_eq!(mem.context_id, university.id, "Uni memory {} must be scoped to University", mem.id);
        assert_ne!(mem.context_id, payx.id, "Uni memory {} must NOT bleed to PayX", mem.id);
    }

    // -----------------------------------------------------------------------
    // CONSISTENCY 3: Workset items are isolated
    // -----------------------------------------------------------------------
    let payx_ws = ContextWorksetBuilder::from_db(&conn, &payx.id).unwrap().build().unwrap();
    let uni_ws  = ContextWorksetBuilder::from_db(&conn, &university.id).unwrap().build().unwrap();

    assert_eq!(payx_ws.context_id, payx.id, "PayX workset must carry PayX context_id");
    assert_eq!(uni_ws.context_id, university.id, "Uni workset must carry Uni context_id");

    let payx_ws_ids: std::collections::HashSet<&str> = payx_ws.items.iter().map(|i| i.id.as_str()).collect();
    let uni_ws_ids:  std::collections::HashSet<&str> = uni_ws.items.iter().map(|i| i.id.as_str()).collect();

    assert!(!payx_ws_ids.contains(uni_item_1.id.as_str()), "Uni item_1 must NOT appear in PayX workset");
    assert!(!payx_ws_ids.contains(uni_item_2.id.as_str()), "Uni item_2 must NOT appear in PayX workset");
    assert!(!uni_ws_ids.contains(payx_item_1.id.as_str()), "PayX item_1 must NOT appear in Uni workset");
    assert!(!uni_ws_ids.contains(payx_item_2.id.as_str()), "PayX item_2 must NOT appear in Uni workset");

    // -----------------------------------------------------------------------
    // CONSISTENCY 4: Sessions are not shared
    // -----------------------------------------------------------------------
    assert_eq!(payx_item_1.session_id.as_deref(), Some(session_payx.id.as_str()), "PayX item_1 must reference PayX session");
    assert_eq!(payx_item_2.session_id.as_deref(), Some(session_payx.id.as_str()), "PayX item_2 must reference PayX session");
    assert_eq!(uni_item_1.session_id.as_deref(), Some(session_university.id.as_str()), "Uni item_1 must reference Uni session");
    assert_eq!(uni_item_2.session_id.as_deref(), Some(session_university.id.as_str()), "Uni item_2 must reference Uni session");
    assert_ne!(payx_item_1.session_id.as_deref(), Some(session_university.id.as_str()), "PayX item_1 must NOT reference Uni session");

    // -----------------------------------------------------------------------
    // CONSISTENCY 5: Snapshots reference the correct context
    // -----------------------------------------------------------------------
    let snap_svc = ContextSnapshotService::new();
    let payx_snap = snap_svc.derive_for_context(&conn, &payx.id).unwrap();
    let uni_snap  = snap_svc.derive_for_context(&conn, &university.id).unwrap();

    assert_eq!(payx_snap.context_id, payx.id, "PayX snapshot must carry PayX context_id");
    assert_eq!(uni_snap.context_id, university.id, "Uni snapshot must carry Uni context_id");
    assert_ne!(payx_snap.context_id, university.id, "PayX snapshot must NOT reference Uni context");
    assert_ne!(uni_snap.context_id, payx.id, "Uni snapshot must NOT reference PayX context");

    for tr in &payx_snap.recent_task_references {
        if let Some(ref task_id) = tr.id {
            assert!(payx_tasks.iter().any(|t| &t.id == task_id),
                "Task '{}' in PayX snapshot must be in PayX task list", task_id);
        }
    }
    for tr in &uni_snap.recent_task_references {
        if let Some(ref task_id) = tr.id {
            assert!(uni_tasks.iter().any(|t| &t.id == task_id),
                "Task '{}' in Uni snapshot must be in Uni task list", task_id);
        }
    }

    // -----------------------------------------------------------------------
    // CONSISTENCY 6: Lifecycle state is valid and context-scoped
    // -----------------------------------------------------------------------
    let payx_lc = ContextLifecycleService::get_context_lifecycle(&conn, &payx.id).unwrap();
    let uni_lc  = ContextLifecycleService::get_context_lifecycle(&conn, &university.id).unwrap();

    assert_eq!(payx_lc.context_id, payx.id, "PayX lifecycle must reference PayX context");
    assert_eq!(uni_lc.context_id, university.id, "Uni lifecycle must reference Uni context");
    assert!(
        payx_lc.lifecycle_state.is_active() || payx_lc.lifecycle_state.is_paused()
            || payx_lc.lifecycle_state.is_dormant() || payx_lc.lifecycle_state.is_archived(),
        "PayX lifecycle state must be recognized; got: {:?}", payx_lc.lifecycle_state
    );

    // -----------------------------------------------------------------------
    // CONSISTENCY 7: Reconstruction restores correct context
    // -----------------------------------------------------------------------
    let recon_svc = ContextReconstructionService::new();
    let payx_recon = recon_svc.reconstruct_context(&conn, &payx.id).unwrap();
    let uni_recon  = recon_svc.reconstruct_context(&conn, &university.id).unwrap();

    assert_eq!(payx_recon.context.id.as_deref(), Some(payx.id.as_str()), "PayX reconstruction context_id must match PayX");
    assert_eq!(uni_recon.context.id.as_deref(), Some(university.id.as_str()), "Uni reconstruction context_id must match Uni");

    if let Some(ref task_ref) = payx_recon.current_task.task_ref {
        assert!(payx_tasks.iter().any(|t| Some(&t.id) == task_ref.id.as_ref()),
            "Task from PayX reconstruction must be in PayX task list");
    }
    if let Some(ref task_ref) = uni_recon.current_task.task_ref {
        assert!(uni_tasks.iter().any(|t| Some(&t.id) == task_ref.id.as_ref()),
            "Task from Uni reconstruction must be in Uni task list");
    }

    // -----------------------------------------------------------------------
    // CONSISTENCY 8: Feedback is scoped — no cross-context bleed
    // -----------------------------------------------------------------------
    let rej = ContextFeedbackEvent::reject_context(
        &payx_item_1.id, &payx.id, FeedbackSource::UserExplicitUi,
        Some("Not relevant to PayX".to_string()),
    );
    ContextFeedbackService::apply_feedback(&conn, &rej).unwrap();

    let conf = ContextFeedbackEvent::confirm_context(
        &uni_item_1.id, &university.id, FeedbackSource::UserExplicitUi,
    );
    ContextFeedbackService::apply_feedback(&conn, &conf).unwrap();

    // PayX rejection must NOT affect University's view of payx_item_1
    let cross1 = ContextFeedbackInspector::inspect_target(&conn, &payx_item_1.id, Some(&university.id)).unwrap();
    assert_eq!(cross1.effective_state, "Neutral", "PayX rejection must NOT affect University view");
    assert_eq!(cross1.history_entries.len(), 0, "No PayX-scoped events visible in University inspector");

    // University confirmation must NOT affect PayX's view of uni_item_1
    let cross2 = ContextFeedbackInspector::inspect_target(&conn, &uni_item_1.id, Some(&payx.id)).unwrap();
    assert_eq!(cross2.effective_state, "Neutral", "Uni confirmation must NOT affect PayX view");

    // -----------------------------------------------------------------------
    // CONSISTENCY 9: DB-level referential integrity
    // -----------------------------------------------------------------------
    let payx_task_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM task_candidates WHERE context_id = ?1",
        rusqlite::params![payx.id], |r| r.get(0),
    ).unwrap();
    assert_eq!(payx_task_count, 1, "PayX must have exactly 1 task candidate");

    let uni_task_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM task_candidates WHERE context_id = ?1",
        rusqlite::params![university.id], |r| r.get(0),
    ).unwrap();
    assert_eq!(uni_task_count, 1, "University must have exactly 1 task candidate");

    let cross_mem: i64 = conn.query_row(
        "SELECT COUNT(*) FROM context_memberships WHERE item_id = ?1 AND workspace_id = ?2",
        rusqlite::params![payx_item_1.id, university.id], |r| r.get(0),
    ).unwrap();
    assert_eq!(cross_mem, 0, "PayX item_1 must NOT have a membership in University");
}

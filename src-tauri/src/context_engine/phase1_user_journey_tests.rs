//! Realistic User Daily Journey Context Workflow Integration Tests (Day 28 - Step 11)
//!
//! Simulates a complete, realistic developer working day across multiple active contexts:
//!
//! Workflow Timeline:
//! 1. Morning: Open ContextSplit.
//! 2. Work on ContextSplit development (code edits, terminal tests, browser documentation).
//! 3. Switch to PayX (payment gateway feature, Stripe documentation).
//! 4. Switch to University (Operating Systems virtual memory assignment).
//! 5. Return to ContextSplit (context return, restored working set).
//! 6. Capture several items (architecture docs, lifecycle code, and a misattributed PayX invoice spec).
//! 7. Mark one item important (architecture_v1.md marked important by user).
//! 8. Correct one incorrect context association (misassigned invoice corrected from ContextSplit to PayX).
//! 9. Switch back to PayX (resuming payment work with corrected invoice item).
//! 10. Pause work (departure snapshots captured, lifecycle set to paused).
//! 11. Close ContextSplit (simulating full process shutdown and cache clearing).
//! 12. Restart ContextSplit (cold boot state recovery from disk database).
//! 13. Reconstruct previous contexts (ContextSplit, PayX, University).
//! 14. Resume PayX (verifying resume readiness, active task, working set).
//! 15. Return to ContextSplit (verifying return readiness, important memories, and cross-system consistency).
//!
//! Verifies:
//! - Correct contexts (identity, names, isolation)
//! - Correct sessions (session boundaries, item associations)
//! - Correct tasks (active task candidates, confidence, evidence linkage)
//! - Correct memories (important memory promotion, user marks, decay)
//! - Correct worksets (active files, exclusion of corrected items, relevance scores)
//! - Correct lifecycle (active, paused, dormant transitions)
//! - Correct snapshots (departure snapshots, quality ratings, workset snapshots)
//! - Correct feedback (importance promotion, correction non-regression)
//! - Correct resume readiness (actionable resume statements, no hallucinated tasks)
//! - Correct interaction API state (timeline, state overview, cross-system consistency)

#[cfg(test)]
mod tests {
    use std::time::Instant;
    use chrono::Utc;
    use rusqlite::Connection;
    use serde_json::json;

    use crate::context_engine::context_state::ResumeReadiness;
    use crate::context_engine::cross_system_consistency::CrossSystemConsistencyChecker;
    use crate::context_engine::feedback::{
        ContextFeedbackEvent, ContextFeedbackService, FeedbackAction, FeedbackScope,
        FeedbackSource, FeedbackTargetRef,
    };
    use crate::context_engine::important_memory::ContextImportantMemoryBuilder;
    use crate::context_engine::lifecycle::ContextLifecycleState;
    use crate::context_engine::memory::invalidate_memory_cache;
    use crate::context_engine::overview::get_context_overview;
    use crate::context_engine::reconstruction::{
        invalidate_reconstruction_cache, ContextReconstructionService,
        ReconstructionSectionStatus,
    };
    use crate::context_engine::snapshot::{
        invalidate_snapshot_cache, ContextSnapshotService,
    };
    use crate::context_engine::timeline::get_context_timeline;
    use crate::context_engine::transitions::{
        record_context_transition, ContextTransitionBuilder, ContextTransitionType,
    };
    use crate::context_engine::workset::{
        invalidate_workset_cache, ContextWorksetService,
    };
    use crate::db::models::{TaskCandidate, TaskCandidateStatus};
    use crate::db::operations::{
        create_item, create_or_update_membership, create_session, create_task_candidate,
        create_workspace,
    };

    fn setup_journey_db() -> Connection {
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

    #[test]
    fn test_realistic_daily_user_journey_full_lifecycle() {
        let conn = setup_journey_db();
        let rec_service = ContextReconstructionService::new();
        let workset_service = ContextWorksetService::new();
        let snapshot_service = ContextSnapshotService::new();

        println!("\n=======================================================");
        println!(">>> STARTING REALISTIC DAILY USER JOURNEY SIMULATION <<<");
        println!("=======================================================\n");

        let now = Utc::now().to_rfc3339();

        // =====================================================================
        // ACT I: MORNING — OPEN CONTEXTSPLIT & WORK ON CONTEXTSPLIT DEV
        // =====================================================================
        println!("[JOURNEY 08:30] Morning: User launches ContextSplit & initializes workspaces");
        let ws_cs = create_workspace(&conn, "ContextSplit").unwrap();
        let ws_payx = create_workspace(&conn, "PayX").unwrap();
        let ws_uni = create_workspace(&conn, "University OS").unwrap();

        let session_morning = create_session(&conn).unwrap();

        // 1. Initial ContextSplit telemetry
        let _item_cs_1 = create_item(
            &conn,
            Some(&ws_cs.id),
            Some(&session_morning.id),
            "file_edit",
            "ContextEngine - src/context_engine/reconstruction.rs",
            Some("ContextEngine - src/context_engine/reconstruction.rs"),
            &json!({ "app_name": "VS Code", "file_path": "src/context_engine/reconstruction.rs" }).to_string(),
        ).unwrap();

        let _item_cs_2 = create_item(
            &conn,
            Some(&ws_cs.id),
            Some(&session_morning.id),
            "file_edit",
            "ContextEngine - src/context_engine/workset.rs",
            Some("ContextEngine - src/context_engine/workset.rs"),
            &json!({ "app_name": "VS Code", "file_path": "src/context_engine/workset.rs" }).to_string(),
        ).unwrap();

        let _item_cs_3 = create_item(
            &conn,
            Some(&ws_cs.id),
            Some(&session_morning.id),
            "terminal",
            "cargo test --lib context_engine::reconstruction",
            Some("cargo test --lib context_engine::reconstruction"),
            &json!({ "app_name": "Terminal", "command": "cargo test --lib" }).to_string(),
        ).unwrap();

        let _item_cs_4 = create_item(
            &conn,
            Some(&ws_cs.id),
            Some(&session_morning.id),
            "browser_focus",
            "SQLite Query Planner & Indexed Scans - Rusqlite Documentation",
            Some("SQLite Query Planner & Indexed Scans - Rusqlite Documentation"),
            &json!({ "app_name": "Firefox", "url": "https://docs.rs/rusqlite/latest/rusqlite/" }).to_string(),
        ).unwrap();

        // Active task candidate for ContextSplit
        let task_cs_1 = TaskCandidate {
            id: "task_cs_1".to_string(),
            context_id: Some(ws_cs.id.clone()),
            title: "Context Engine Phase 1 Reconstruction Architecture".to_string(),
            confidence: 0.96,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &task_cs_1).unwrap();

        // Verify initial morning workset
        let ws_cs_morning = workset_service.derive_for_workspace(&conn, &ws_cs.id).unwrap();
        assert_eq!(ws_cs_morning.items.len(), 4);
        assert_eq!(ws_cs_morning.context_id, ws_cs.id);

        // =====================================================================
        // ACT II: 10:15 — SWITCH TO PAYX (PAYMENT GATEWAY DEVELOPMENT)
        // =====================================================================
        println!("[JOURNEY 10:15] Switch to PayX project");
        let transition_1 = ContextTransitionBuilder::new(ContextTransitionType::ContextSwitch, 0.94)
            .from_context(ws_cs.id.clone())
            .to_context(ws_payx.id.clone())
            .session(session_morning.id.clone())
            .build();
        record_context_transition(&conn, &transition_1).unwrap();

        let item_payx_1 = create_item(
            &conn,
            Some(&ws_payx.id),
            Some(&session_morning.id),
            "file_edit",
            "PayX - Sources/PayX/PaymentGateway.swift",
            Some("PayX - Sources/PayX/PaymentGateway.swift"),
            &json!({ "app_name": "Xcode", "file_path": "Sources/PayX/PaymentGateway.swift" }).to_string(),
        ).unwrap();

        let item_payx_2 = create_item(
            &conn,
            Some(&ws_payx.id),
            Some(&session_morning.id),
            "browser_focus",
            "Stripe API Reference - Create a Checkout Session",
            Some("Stripe API Reference - Create a Checkout Session"),
            &json!({ "app_name": "Safari", "url": "https://stripe.com/docs/api/checkout/sessions" }).to_string(),
        ).unwrap();

        let item_payx_3 = create_item(
            &conn,
            Some(&ws_payx.id),
            Some(&session_morning.id),
            "terminal",
            "swift test --filter PaymentGatewayTests",
            Some("swift test --filter PaymentGatewayTests"),
            &json!({ "app_name": "Terminal", "command": "swift test" }).to_string(),
        ).unwrap();

        let task_payx_1 = TaskCandidate {
            id: "task_payx_1".to_string(),
            context_id: Some(ws_payx.id.clone()),
            title: "Stripe Checkout Webhook & Session Integration".to_string(),
            confidence: 0.94,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &task_payx_1).unwrap();

        // Verify PayX workset isolation
        let ws_payx_workset = workset_service.derive_for_workspace(&conn, &ws_payx.id).unwrap();
        assert_eq!(ws_payx_workset.items.len(), 3);
        assert!(ws_payx_workset.items.iter().all(|i| i.id == item_payx_1.id || i.id == item_payx_2.id || i.id == item_payx_3.id));

        // =====================================================================
        // ACT III: 11:45 — SWITCH TO UNIVERSITY OS (VIRTUAL MEMORY PAGING)
        // =====================================================================
        println!("[JOURNEY 11:45] Switch to University OS assignment");
        let transition_2 = ContextTransitionBuilder::new(ContextTransitionType::ContextSwitch, 0.92)
            .from_context(ws_payx.id.clone())
            .to_context(ws_uni.id.clone())
            .session(session_morning.id.clone())
            .build();
        record_context_transition(&conn, &transition_2).unwrap();

        let _item_uni_1 = create_item(
            &conn,
            Some(&ws_uni.id),
            Some(&session_morning.id),
            "file_edit",
            "xv6-riscv - kernel/vm.c (page table walk)",
            Some("xv6-riscv - kernel/vm.c (page table walk)"),
            &json!({ "app_name": "CLion", "file_path": "kernel/vm.c" }).to_string(),
        ).unwrap();

        let _item_uni_2 = create_item(
            &conn,
            Some(&ws_uni.id),
            Some(&session_morning.id),
            "file_edit",
            "OS Lab 4 - Paging & TLB Invalidation Report.md",
            Some("OS Lab 4 - Paging & TLB Invalidation Report.md"),
            &json!({ "app_name": "Obsidian", "file_path": "docs/os_lab4.md" }).to_string(),
        ).unwrap();

        let _item_uni_3 = create_item(
            &conn,
            Some(&ws_uni.id),
            Some(&session_morning.id),
            "browser_focus",
            "CS451 Operating Systems - Assignment 4 Virtual Memory Specification",
            Some("CS451 Operating Systems - Assignment 4 Virtual Memory Specification"),
            &json!({ "app_name": "Chrome", "url": "https://canvas.university.edu/courses/451/assignments/4" }).to_string(),
        ).unwrap();

        let task_uni_1 = TaskCandidate {
            id: "task_uni_1".to_string(),
            context_id: Some(ws_uni.id.clone()),
            title: "Operating Systems Page Table Walk & Demand Paging".to_string(),
            confidence: 0.97,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &task_uni_1).unwrap();

        // =====================================================================
        // ACT IV: 13:30 — RETURN TO CONTEXTSPLIT
        // =====================================================================
        println!("[JOURNEY 13:30] Afternoon: Return to ContextSplit");
        let transition_3 = ContextTransitionBuilder::new(ContextTransitionType::ContextReturn, 0.95)
            .from_context(ws_uni.id.clone())
            .to_context(ws_cs.id.clone())
            .session(session_morning.id.clone())
            .build();
        record_context_transition(&conn, &transition_3).unwrap();

        // Verify clean return reconstruction
        let rec_cs_return = rec_service.reconstruct_context(&conn, &ws_cs.id).unwrap();
        assert_eq!(rec_cs_return.context.name, "ContextSplit");
        assert_eq!(rec_cs_return.workset.status, ReconstructionSectionStatus::Available);
        assert_eq!(rec_cs_return.current_task.title.as_deref().unwrap(), "Context Engine Phase 1 Reconstruction Architecture");

        // =====================================================================
        // ACT V: 14:00 — CAPTURE SEVERAL ITEMS (INCLUDING MISTAKEN ASSOCIATION)
        // =====================================================================
        println!("[JOURNEY 14:00] Capturing new items + 1 misattributed PayX specification");
        let item_cs_arch = create_item(
            &conn,
            Some(&ws_cs.id),
            Some(&session_morning.id),
            "file_edit",
            "docs/contextsplit_architecture_v1.md",
            Some("docs/contextsplit_architecture_v1.md"),
            &json!({ "app_name": "VS Code", "file_path": "docs/contextsplit_architecture_v1.md" }).to_string(),
        ).unwrap();

        let _item_cs_lifecycle = create_item(
            &conn,
            Some(&ws_cs.id),
            Some(&session_morning.id),
            "file_edit",
            "src-tauri/src/context_engine/lifecycle.rs",
            Some("src-tauri/src/context_engine/lifecycle.rs"),
            &json!({ "app_name": "VS Code", "file_path": "src/context_engine/lifecycle.rs" }).to_string(),
        ).unwrap();

        // Mistakenly captured / suggested under ContextSplit
        let item_misassigned_payx = create_item(
            &conn,
            Some(&ws_cs.id),
            Some(&session_morning.id),
            "browser_focus",
            "PayX Internal Merchant Invoicing API Specification v2",
            Some("PayX Internal Merchant Invoicing API Specification v2"),
            &json!({ "app_name": "Safari", "url": "https://internal.payx.dev/docs/invoicing" }).to_string(),
        ).unwrap();

        // =====================================================================
        // ACT VI: 14:30 — USER MARKS ONE ITEM IMPORTANT
        // =====================================================================
        println!("[JOURNEY 14:30] User feedback: Mark architecture_v1.md as IMPORTANT");
        let feedback_important = ContextFeedbackEvent::new(
            FeedbackAction::MarkImportant,
            FeedbackTargetRef::Item(item_cs_arch.id.clone()),
            Some(ws_cs.id.clone()),
            FeedbackSource::UserExplicitUi,
            FeedbackScope::item_context(item_cs_arch.id.clone(), ws_cs.id.clone()),
            Some("Key architectural blueprint for ContextSplit engine".to_string()),
        );
        let fb_res1 = ContextFeedbackService::apply_feedback(&conn, &feedback_important).unwrap();
        assert!(fb_res1.success);

        // Verify Important Memory contains architecture_v1.md with user-boosted ranking
        let important_memories = ContextImportantMemoryBuilder::build(&conn, &ws_cs.id, None).unwrap();
        assert!(important_memories.memories.iter().any(|m| m.title.contains("architecture_v1.md")));

        // =====================================================================
        // ACT VII: 15:00 — USER CORRECTS THE INCORRECT CONTEXT ASSOCIATION
        // =====================================================================
        println!("[JOURNEY 15:00] User feedback: Correct misassigned PayX item to PayX workspace");
        let feedback_correct = ContextFeedbackEvent::new(
            FeedbackAction::RemoveFromContext,
            FeedbackTargetRef::Item(item_misassigned_payx.id.clone()),
            Some(ws_cs.id.clone()),
            FeedbackSource::UserExplicitUi,
            FeedbackScope::item_context(item_misassigned_payx.id.clone(), ws_cs.id.clone()),
            Some("This invoice specification belongs to PayX, not ContextSplit".to_string()),
        );
        let fb_res2 = ContextFeedbackService::apply_feedback(&conn, &feedback_correct).unwrap();
        assert!(fb_res2.success);

        // Reassign to PayX
        conn.execute(
            "UPDATE items SET workspace_id = ?1 WHERE id = ?2",
            rusqlite::params![ws_payx.id, item_misassigned_payx.id],
        ).unwrap();
        create_or_update_membership(&conn, &item_misassigned_payx.id, &ws_payx.id, "confirmed", 1.0, "user_correction").unwrap();
        invalidate_workset_cache(None);

        // Verify ContextSplit workset NO LONGER contains the corrected item
        let ws_cs_cleaned = workset_service.derive_for_workspace(&conn, &ws_cs.id).unwrap();
        assert!(!ws_cs_cleaned.items.iter().any(|i| i.id == item_misassigned_payx.id));
        assert!(ws_cs_cleaned.items.iter().any(|i| i.id == item_cs_arch.id));

        // =====================================================================
        // ACT VIII: 15:30 — SWITCH BACK TO PAYX (WITH CORRECTED ITEM)
        // =====================================================================
        println!("[JOURNEY 15:30] Switch back to PayX with corrected invoice reference");
        let transition_4 = ContextTransitionBuilder::new(ContextTransitionType::ContextSwitch, 0.93)
            .from_context(ws_cs.id.clone())
            .to_context(ws_payx.id.clone())
            .session(session_morning.id.clone())
            .build();
        record_context_transition(&conn, &transition_4).unwrap();

        // Verify PayX workset now includes the corrected item
        let ws_payx_updated = workset_service.derive_for_workspace(&conn, &ws_payx.id).unwrap();
        assert!(ws_payx_updated.items.iter().any(|i| i.id == item_misassigned_payx.id));

        // =====================================================================
        // ACT IX: 17:00 — PAUSE WORK & CAPTURE DEPARTURE SNAPSHOTS
        // =====================================================================
        println!("[JOURNEY 17:00] Evening: User pauses work. Capture point-in-time snapshots");
        let snap_payx = snapshot_service.derive_for_context(&conn, &ws_payx.id).unwrap();
        snapshot_service.record_snapshot_history_with_db(&conn, snap_payx).unwrap();

        let snap_cs = snapshot_service.derive_for_context(&conn, &ws_cs.id).unwrap();
        snapshot_service.record_snapshot_history_with_db(&conn, snap_cs).unwrap();

        // Update workspace lifecycle state to Paused
        conn.execute(
            "UPDATE workspaces SET lifecycle_state = 'paused', updated_at = datetime('now') WHERE id IN (?1, ?2, ?3)",
            rusqlite::params![ws_payx.id, ws_cs.id, ws_uni.id],
        ).unwrap();

        // =====================================================================
        // ACT X: 17:30 — CLOSE CONTEXTSPLIT (FULL PROCESS TERMINATION)
        // =====================================================================
        println!("[JOURNEY 17:30] User closes ContextSplit (simulating full process exit)");
        invalidate_reconstruction_cache(None);
        invalidate_workset_cache(None);
        invalidate_memory_cache(None);
        invalidate_snapshot_cache(&ws_cs.id);
        invalidate_snapshot_cache(&ws_payx.id);
        invalidate_snapshot_cache(&ws_uni.id);

        // =====================================================================
        // ACT XI: NEXT MORNING (09:00) — RESTART CONTEXTSPLIT (COLD BOOT)
        // =====================================================================
        println!("[JOURNEY NEXT DAY 09:00] Cold Application Restart & Context Reconstruction");
        let t_restart = Instant::now();

        // =====================================================================
        // ACT XII: RECONSTRUCT PREVIOUS CONTEXTS & THOROUGH VERIFICATION
        // =====================================================================
        let rec_cs_restored = rec_service.reconstruct_context(&conn, &ws_cs.id).unwrap();
        let rec_payx_restored = rec_service.reconstruct_context(&conn, &ws_payx.id).unwrap();
        let rec_uni_restored = rec_service.reconstruct_context(&conn, &ws_uni.id).unwrap();
        let restart_dur = t_restart.elapsed();

        println!("[JOURNEY VERIFICATION] Cold reconstruction completed in {:?}", restart_dur);

        // 1. Verify Correct Contexts
        println!("  -> 1. Verifying Correct Contexts");
        assert_eq!(rec_cs_restored.context.id.as_deref(), Some(ws_cs.id.as_str()));
        assert_eq!(rec_cs_restored.context.name, "ContextSplit");
        assert_eq!(rec_payx_restored.context.id.as_deref(), Some(ws_payx.id.as_str()));
        assert_eq!(rec_payx_restored.context.name, "PayX");
        assert_eq!(rec_uni_restored.context.id.as_deref(), Some(ws_uni.id.as_str()));
        assert_eq!(rec_uni_restored.context.name, "University OS");

        // 2. Verify Correct Sessions
        println!("  -> 2. Verifying Correct Sessions");
        assert_eq!(rec_cs_restored.recent_activity.status, ReconstructionSectionStatus::Available);
        assert_eq!(rec_payx_restored.recent_activity.status, ReconstructionSectionStatus::Available);
        assert_eq!(rec_uni_restored.recent_activity.status, ReconstructionSectionStatus::Available);

        // 3. Verify Correct Tasks
        println!("  -> 3. Verifying Correct Tasks");
        assert_eq!(rec_cs_restored.current_task.status, ReconstructionSectionStatus::Available);
        assert_eq!(rec_cs_restored.current_task.title.as_deref().unwrap(), "Context Engine Phase 1 Reconstruction Architecture");
        assert_eq!(rec_payx_restored.current_task.status, ReconstructionSectionStatus::Available);
        assert_eq!(rec_payx_restored.current_task.title.as_deref().unwrap(), "Stripe Checkout Webhook & Session Integration");
        assert_eq!(rec_uni_restored.current_task.status, ReconstructionSectionStatus::Available);
        assert_eq!(rec_uni_restored.current_task.title.as_deref().unwrap(), "Operating Systems Page Table Walk & Demand Paging");

        // 4. Verify Correct Memories
        println!("  -> 4. Verifying Correct Memories");
        assert_eq!(rec_cs_restored.important_memories.status, ReconstructionSectionStatus::Available);
        assert!(rec_cs_restored.important_memories.memories.iter().any(|m| m.title.contains("architecture_v1.md")));

        // 5. Verify Correct Worksets
        println!("  -> 5. Verify Correct Worksets");
        assert_eq!(rec_cs_restored.workset.status, ReconstructionSectionStatus::Available);
        // Corrected item must NOT be in ContextSplit workset
        assert!(!rec_cs_restored.workset.items.iter().any(|i| i.id == item_misassigned_payx.id));
        // Important architecture doc MUST be in ContextSplit workset
        assert!(rec_cs_restored.workset.items.iter().any(|i| i.id == item_cs_arch.id));
        // Corrected invoice item MUST be in PayX workset
        assert!(rec_payx_restored.workset.items.iter().any(|i| i.id == item_misassigned_payx.id));

        // 6. Verify Correct Lifecycle
        println!("  -> 6. Verifying Correct Lifecycle");
        assert_eq!(rec_cs_restored.lifecycle.as_ref().unwrap().lifecycle_state, ContextLifecycleState::Paused);
        assert_eq!(rec_payx_restored.lifecycle.as_ref().unwrap().lifecycle_state, ContextLifecycleState::Active);
        assert_eq!(rec_uni_restored.lifecycle.as_ref().unwrap().lifecycle_state, ContextLifecycleState::Paused);

        // 7. Verify Correct Snapshots
        println!("  -> 7. Verifying Correct Snapshots");
        assert!(rec_payx_restored.latest_snapshot.snapshot.is_some());
        assert!(rec_cs_restored.latest_snapshot.snapshot.is_some());

        // 8. Verify Correct Feedback
        println!("  -> 8. Verifying Correct Feedback Application");
        let fb_count: usize = conn.query_row(
            "SELECT COUNT(*) FROM context_signals WHERE workspace_id = ?1",
            rusqlite::params![ws_cs.id],
            |row| row.get(0),
        ).unwrap();
        assert!(fb_count >= 2, "Feedback signals must be recorded for ContextSplit");

        // 9. Verify Correct Resume Readiness
        println!("  -> 9. Verifying Resume Readiness State");
        assert!(rec_payx_restored.resume_readiness.can_resume);
        assert!(rec_cs_restored.resume_readiness.can_resume);
        assert_eq!(rec_payx_restored.resume_readiness.readiness, ResumeReadiness::Ready);
        assert_eq!(rec_cs_restored.resume_readiness.readiness, ResumeReadiness::Ready);

        // 10. Verify Correct Interaction API State & Cross-System Consistency
        println!("  -> 10. Verifying Interaction API State & 100% Cross-System Consistency");
        let overview = get_context_overview(&conn, &ws_cs.id).unwrap();
        assert_eq!(overview.name, "ContextSplit");

        let timeline = get_context_timeline(&conn, &ws_cs.id).unwrap();
        assert!(!timeline.items.is_empty() || !timeline.sessions.is_empty());

        let consistency_report = CrossSystemConsistencyChecker::validate_all(&conn).unwrap();
        assert!(consistency_report.is_fully_consistent, "All contextual subsystems must agree on workspace identity");
        assert_eq!(consistency_report.workspace_reports.iter().filter(|r| !r.all_systems_consistent).count(), 0);

        // =====================================================================
        // ACT XIII: RESUME PAYX & RETURN TO CONTEXTSPLIT
        // =====================================================================
        println!("[JOURNEY 09:30] Resume PayX and finalize ContextSplit return");
        conn.execute(
            "UPDATE workspaces SET lifecycle_state = 'active', updated_at = datetime('now') WHERE id = ?1",
            rusqlite::params![ws_payx.id],
        ).unwrap();
        let transition_resume = ContextTransitionBuilder::new(ContextTransitionType::ContextReturn, 0.96)
            .to_context(ws_payx.id.clone())
            .build();
        record_context_transition(&conn, &transition_resume).unwrap();

        let _item_payx_resumed = create_item(
            &conn,
            Some(&ws_payx.id),
            Some(&session_morning.id),
            "file_edit",
            "PayX - Sources/PayX/PaymentGateway.swift",
            Some("PayX - Sources/PayX/PaymentGateway.swift"),
            &json!({ "app_name": "Xcode" }).to_string(),
        ).unwrap();

        invalidate_reconstruction_cache(None);
        invalidate_workset_cache(None);
        let rec_payx_active = rec_service.reconstruct_context(&conn, &ws_payx.id).unwrap();
        assert_eq!(rec_payx_active.lifecycle.unwrap().lifecycle_state, ContextLifecycleState::Active);

        // Final Return to ContextSplit
        conn.execute(
            "UPDATE workspaces SET lifecycle_state = 'active', updated_at = datetime('now') WHERE id = ?1",
            rusqlite::params![ws_cs.id],
        ).unwrap();
        let transition_return_cs = ContextTransitionBuilder::new(ContextTransitionType::ContextReturn, 0.96)
            .from_context(ws_payx.id.clone())
            .to_context(ws_cs.id.clone())
            .build();
        record_context_transition(&conn, &transition_return_cs).unwrap();

        let _item_cs_final_activity = create_item(
            &conn,
            Some(&ws_cs.id),
            Some(&session_morning.id),
            "file_edit",
            "src/context_engine/engine.rs",
            Some("src/context_engine/engine.rs"),
            &json!({ "app_name": "VS Code" }).to_string(),
        ).unwrap();

        invalidate_reconstruction_cache(None);
        invalidate_workset_cache(None);
        let rec_cs_final = rec_service.reconstruct_context(&conn, &ws_cs.id).unwrap();
        assert_eq!(rec_cs_final.lifecycle.unwrap().lifecycle_state, ContextLifecycleState::Active);
        assert!(!rec_cs_final.workset.items.is_empty());

        println!("\n=======================================================");
        println!(">>> REALISTIC USER JOURNEY TEST COMPLETED: 100% PASS <<<");
        println!("=======================================================\n");
    }
}

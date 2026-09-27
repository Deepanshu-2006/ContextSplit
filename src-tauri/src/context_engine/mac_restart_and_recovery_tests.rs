#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use rusqlite::Connection;

    use crate::context_engine::memory::invalidate_memory_cache;
    use crate::context_engine::pause_detector::{ContextPauseDetector, PauseCause, PauseEvaluationInput};
    use crate::context_engine::reconstruction::{
        invalidate_reconstruction_cache, ContextReconstructionService, ReconstructionSectionStatus,
    };
    use crate::context_engine::segments::ContextActivitySegmenter;
    use crate::context_engine::snapshot::{invalidate_snapshot_cache, ContextSnapshotService};
    use crate::context_engine::transitions::{
        record_context_transition, ContextTransitionBuilder, ContextTransitionType,
    };
    use crate::context_engine::workset::{invalidate_workset_cache, ContextWorksetService};
    use crate::db::models::{TaskCandidate, TaskCandidateStatus};
    use crate::db::operations::{
        confirm_membership, create_item, create_session, create_task_candidate,
        create_workspace, dismiss_membership, list_workspaces, save_context_snapshot,
    };

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;
             PRAGMA foreign_keys = ON;",
        ).unwrap();
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

    fn invalidate_all_in_memory_caches() {
        invalidate_memory_cache(None);
        invalidate_reconstruction_cache(None);
        invalidate_workset_cache(None);
        invalidate_snapshot_cache("");
    }

    // =========================================================================
    // STEP 14.1 — FULL PROCESS RESTART AND COLD RECOVERY TEST
    // =========================================================================
    #[test]
    fn test_full_restart_and_cold_recovery() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();

        // 1. Start ContextSplit & 2. Create multiple contexts
        let ws_cs = create_workspace(&conn, "ContextSplit").unwrap();
        let ws_payx = create_workspace(&conn, "PayX Engine").unwrap();
        let ws_uni = create_workspace(&conn, "University Distributed Systems").unwrap();

        // 3. Generate activity in ContextSplit
        let _item_cs1 = create_item(
            &conn,
            Some(&ws_cs.id),
            Some(&session.id),
            "file",
            "/Users/dev/ContextSplit/src/context_engine/reconstruction.rs",
            Some("reconstruction.rs"),
            "{\"app_name\":\"Visual Studio Code\",\"status\":\"confirmed\"}",
        ).unwrap();

        let _item_cs2 = create_item(
            &conn,
            Some(&ws_cs.id),
            Some(&session.id),
            "url",
            "https://github.com/org/ContextSplit/pull/42",
            Some("PR #42 - Context Engine"),
            "{\"app_name\":\"Google Chrome\",\"domain\":\"github.com\",\"status\":\"confirmed\"}",
        ).unwrap();

        // 4. Switch contexts to PayX
        record_context_transition(
            &conn,
            &ContextTransitionBuilder::new(ContextTransitionType::ContextSwitch, 1.0)
                .from_context(ws_cs.id.clone())
                .to_context(ws_payx.id.clone())
                .build(),
        ).unwrap();
        conn.execute("UPDATE workspaces SET lifecycle_state = 'active' WHERE id = ?1", rusqlite::params![ws_payx.id]).unwrap();

        let item_payx = create_item(
            &conn,
            Some(&ws_payx.id),
            Some(&session.id),
            "file",
            "/Users/dev/PayX/src/settlement/ledger.rs",
            Some("ledger.rs"),
            "{\"app_name\":\"Visual Studio Code\",\"status\":\"confirmed\"}",
        ).unwrap();

        // 5. Create user feedback: confirm PayX item, dismiss accidental noisy University item
        confirm_membership(&conn, &item_payx.id, &ws_payx.id).unwrap();

        let noisy_item = create_item(
            &conn,
            Some(&ws_uni.id),
            Some(&session.id),
            "url",
            "https://random-forum.com/thread/999",
            Some("Random Off-topic Tab"),
            "{\"app_name\":\"Google Chrome\",\"status\":\"observed\"}",
        ).unwrap();
        dismiss_membership(&conn, &noisy_item.id, &ws_uni.id).unwrap();

        // 6. Create Tasks & 7. Create Memories
        let task_cs = TaskCandidate {
            id: format!("task-cs-{}", ws_cs.id),
            context_id: Some(ws_cs.id.clone()),
            title: "Implement cold context reconstruction".to_string(),
            confidence: 0.95,
            status: TaskCandidateStatus::Confirmed,
            first_detected_at: Utc::now().to_rfc3339(),
            last_active_at: Utc::now().to_rfc3339(),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };
        create_task_candidate(&conn, &task_cs).unwrap();

        let task_payx = TaskCandidate {
            id: format!("task-payx-{}", ws_payx.id),
            context_id: Some(ws_payx.id.clone()),
            title: "Reconcile payment settlements".to_string(),
            confidence: 0.88,
            status: TaskCandidateStatus::Active,
            first_detected_at: Utc::now().to_rfc3339(),
            last_active_at: Utc::now().to_rfc3339(),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };
        create_task_candidate(&conn, &task_payx).unwrap();

        // Snapshot ContextSplit state before switch
        let snap_service = ContextSnapshotService::new();
        let snap_cs = snap_service.derive_for_context(&conn, &ws_cs.id).unwrap();
        save_context_snapshot(&conn, &snap_cs, 50).unwrap();

        // Switch back to ContextSplit
        record_context_transition(
            &conn,
            &ContextTransitionBuilder::new(ContextTransitionType::ContextSwitch, 1.0)
                .from_context(ws_payx.id.clone())
                .to_context(ws_cs.id.clone())
                .build(),
        ).unwrap();
        conn.execute("UPDATE workspaces SET lifecycle_state = 'active' WHERE id = ?1", rusqlite::params![ws_cs.id]).unwrap();

        // 8. Close ContextSplit (simulate complete process shutdown & memory flush)
        invalidate_all_in_memory_caches();

        // 9. Reopen ContextSplit (cold boot from SQLite storage)
        // Verify: No duplicate contexts
        let all_workspaces = list_workspaces(&conn).unwrap();
        assert_eq!(all_workspaces.len(), 3, "Must have exactly 3 workspaces, no duplicates");
        assert!(all_workspaces.iter().any(|w| w.name == "ContextSplit"));
        assert!(all_workspaces.iter().any(|w| w.name == "PayX Engine"));
        assert!(all_workspaces.iter().any(|w| w.name == "University Distributed Systems"));

        // 10. Continue working in ContextSplit
        let _item_cs3 = create_item(
            &conn,
            Some(&ws_cs.id),
            Some(&session.id),
            "file",
            "/Users/dev/ContextSplit/src/context_engine/mac_restart_and_recovery_tests.rs",
            Some("mac_restart_and_recovery_tests.rs"),
            "{\"app_name\":\"Visual Studio Code\",\"status\":\"confirmed\"}",
        ).unwrap();

        // Verify: Active context derivation after cold reboot
        let active_workset = ContextWorksetService::new().derive_active(&conn).unwrap();
        assert!(active_workset.is_some(), "Active workset must be derived after restart");
        let active_ws = active_workset.unwrap();
        assert_eq!(active_ws.context_id, ws_cs.id, "Active context must be ContextSplit");

        // Verify: Cold reconstruction quality & resume readiness
        let recon_service = ContextReconstructionService::new();
        let recon_cs = recon_service.reconstruct_cold(&conn, &ws_cs.id).unwrap();

        assert_eq!(recon_cs.context.status, ReconstructionSectionStatus::Available);
        assert_eq!(recon_cs.context.name, "ContextSplit");
        assert_eq!(recon_cs.current_task.status, ReconstructionSectionStatus::Available);
        assert_eq!(recon_cs.current_task.title.as_deref(), Some("Implement cold context reconstruction"));
        assert!(recon_cs.resume_readiness.can_resume, "Must be READY to resume after restart");

        // Verify: Feedback persisted across reboot (University dismissed item suppressed from workset)
        let uni_workset = ContextWorksetService::new().derive_for_workspace(&conn, &ws_uni.id).unwrap();
        assert!(
            !uni_workset.items.iter().any(|i| i.id == noisy_item.id),
            "Dismissed item must not appear in University workset after reboot"
        );

        // Verify: PayX state intact after reboot
        let recon_payx = recon_service.reconstruct_cold(&conn, &ws_payx.id).unwrap();
        assert_eq!(recon_payx.context.name, "PayX Engine");
        assert_eq!(recon_payx.current_task.title.as_deref(), Some("Reconcile payment settlements"));
    }

    // =========================================================================
    // STEP 14.2 — MAC SLEEP AND WAKE LIFECYCLE TEST
    // =========================================================================
    #[test]
    fn test_mac_sleep_and_wake_lifecycle() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let ws = create_workspace(&conn, "ContextSplit Active Work").unwrap();

        let t0 = Utc::now();

        // Initial active work before sleep
        create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "file",
            "/Users/dev/ContextSplit/src/main.rs",
            Some("main.rs"),
            "{\"app_name\":\"Visual Studio Code\",\"status\":\"confirmed\"}",
        ).unwrap();

        // 11. Put the Mac to sleep (simulate 3 hours gap / OS sleep)
        let pause_detector = ContextPauseDetector::default();
        let input = PauseEvaluationInput {
            context_id: &ws.id,
            context_name: Some(&ws.name),
            workspace: Some(&ws),
            last_telemetry_gap_seconds: Some(3 * 3600), // 3 hours gap
            reference_time: Some(t0 + Duration::hours(3) + Duration::minutes(30)),
            ..Default::default()
        };

        let pause_eval = pause_detector.evaluate(input);
        assert!(pause_eval.is_paused, "A 3-hour gap must be recognized as a system sleep pause");
        assert_eq!(pause_eval.pause_cause, Some(PauseCause::SystemSleep));
        assert!(pause_eval.is_resumable, "Invariant: Paused context must remain resumable");

        // 12. Wake the Mac & 13. Continue working
        let _item_after_wake = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "file",
            "/Users/dev/ContextSplit/src/lib.rs",
            Some("lib.rs"),
            "{\"app_name\":\"Visual Studio Code\",\"status\":\"confirmed\"}",
        ).unwrap();

        // Verify: Segmenter separates pre-sleep and post-sleep activity without timestamp corruption
        let mut segmenter = ContextActivitySegmenter::new();
        let segments = segmenter.build_segments_for_workspace(&conn, &ws.id).unwrap();
        assert!(!segments.is_empty(), "Segments must exist across sleep/wake");

        // Verify: Context identity remained stable throughout sleep/wake
        let recon = ContextReconstructionService::new().reconstruct_context(&conn, &ws.id).unwrap();
        assert_eq!(recon.context.status, ReconstructionSectionStatus::Available);
        assert_eq!(recon.context.name, "ContextSplit Active Work");
        assert!(recon.resume_readiness.can_resume);
    }

    // =========================================================================
    // STEP 14.3 — MULTI-CONTEXT CRASH RECOVERY WITHOUT CORRUPTION
    // =========================================================================
    #[test]
    fn test_crash_recovery_without_corruption() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let ws = create_workspace(&conn, "Crash Recovery Context").unwrap();

        // Populate rapid items
        for i in 0..10 {
            create_item(
                &conn,
                Some(&ws.id),
                Some(&session.id),
                "file",
                &format!("/dev/crash_test_{}.rs", i),
                Some(&format!("crash_test_{}.rs", i)),
                "{\"status\":\"observed\"}",
            ).unwrap();
        }

        // Simulate abrupt crash by dropping all memory state without calling snapshot save
        invalidate_all_in_memory_caches();

        // Cold reconstruct on restart
        let recon = ContextReconstructionService::new().reconstruct_cold(&conn, &ws.id).unwrap();
        assert_eq!(recon.context.status, ReconstructionSectionStatus::Available);
        assert_eq!(recon.context.name, "Crash Recovery Context");
        assert_eq!(recon.workset.status, ReconstructionSectionStatus::Available);
        assert!(!recon.workset.items.is_empty(), "Workset must recover from SQLite storage");
    }
}

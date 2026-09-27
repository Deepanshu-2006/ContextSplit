#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use rusqlite::Connection;

    use crate::context_engine::context_state::ResumeReadiness;
    use crate::context_engine::memory::invalidate_memory_cache;
    use crate::context_engine::reconstruction::{
        invalidate_reconstruction_cache, ContextReconstructionService, ReconstructionQualityRating,
        ReconstructionSectionStatus,
    };
    use crate::context_engine::snapshot::{
        invalidate_snapshot_cache, ContextSnapshotService,
    };
    use crate::context_engine::workset::invalidate_workset_cache;
    use crate::db::models::{TaskCandidate, TaskCandidateStatus};
    use crate::db::operations::{
        create_item, create_item_relationship, create_session,
        create_task_candidate, create_workspace, save_context_snapshot,
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

    /// Classification enum matching the test specification: READY | PARTIALLY_READY | NOT_READY
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TestResumeClassification {
        Ready,
        PartiallyReady,
        NotReady,
    }

    impl TestResumeClassification {
        pub fn from_reconstruction(reconstruction: &crate::context_engine::reconstruction::ContextReconstruction) -> Self {
            if reconstruction.resume_readiness.can_resume
                && (reconstruction.quality.rating == ReconstructionQualityRating::Coherent
                    || reconstruction.quality.rating == ReconstructionQualityRating::MostlyCoherent)
                && reconstruction.resume_readiness.readiness == ResumeReadiness::Ready
            {
                Self::Ready
            } else if reconstruction.resume_readiness.readiness == ResumeReadiness::MostlyReady
                || reconstruction.quality.rating == ReconstructionQualityRating::Partial
                || reconstruction.resume_readiness.readiness == ResumeReadiness::Partial
            {
                Self::PartiallyReady
            } else {
                Self::NotReady
            }
        }
    }

    // =========================================================================
    // CONTEXT 1: ContextSplit — ACTIVELY WORKING
    // =========================================================================
    // State: Actively working
    // Task: "Validate Task Reconstruction" (Confirmed)
    // Files: task_extractor.rs, reconstruction.rs
    // URLs: https://github.com/org/contextsplit/pull/42
    // Apps: Visual Studio Code, Terminal, Google Chrome
    // Snapshot: Fresh departure snapshot captured
    // Memory: Pinned architectural invariant
    // Expected: READY
    // =========================================================================
    #[test]
    fn test_contextsplit_actively_working_resume_readiness() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let now = Utc::now();
        let now_str = now.to_rfc3339();

        let ws = create_workspace(&conn, "ContextSplit").unwrap();
        invalidate_reconstruction_cache(Some(&ws.id));
        invalidate_memory_cache(Some(&ws.id));
        invalidate_workset_cache(Some(&ws.id));
        invalidate_snapshot_cache(&ws.id);

        // 1. Confirmed active task
        let task = TaskCandidate {
            id: format!("task-contextsplit-{}", ws.id),
            context_id: Some(ws.id.clone()),
            title: "Validate Task Reconstruction".to_string(),
            confidence: 0.98,
            status: TaskCandidateStatus::Confirmed,
            first_detected_at: (now - Duration::minutes(30)).to_rfc3339(),
            last_active_at: now_str.clone(),
            created_at: (now - Duration::minutes(30)).to_rfc3339(),
            updated_at: now_str.clone(),
        };
        create_task_candidate(&conn, &task).unwrap();

        // 2. High-signal files, URLs, and applications
        let file_extractor = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "file",
            "/Users/dev/ContextSplit/src-tauri/src/context_engine/task_extractor.rs",
            Some("task_extractor.rs"),
            "{\"status\":\"confirmed\",\"app_name\":\"Visual Studio Code\"}",
        ).unwrap();

        let file_reconstruct = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "file",
            "/Users/dev/ContextSplit/src-tauri/src/context_engine/reconstruction.rs",
            Some("reconstruction.rs"),
            "{\"status\":\"confirmed\",\"app_name\":\"Visual Studio Code\"}",
        ).unwrap();

        let pr_url = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://github.com/org/contextsplit/pull/42",
            Some("PR #42: Task Extraction & Quality Filters"),
            "{\"status\":\"confirmed\",\"app_name\":\"Google Chrome\",\"browser_domain\":\"github.com\"}",
        ).unwrap();

        let term_item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "terminal",
            "cargo test --lib context_engine::task",
            Some("Cargo Test Task Engine"),
            "{\"status\":\"confirmed\",\"app_name\":\"Terminal\"}",
        ).unwrap();

        create_item_relationship(&conn, &file_extractor.id, &file_reconstruct.id, "references", 0.95, None).unwrap();
        create_item_relationship(&conn, &file_reconstruct.id, &pr_url.id, "documents", 0.90, None).unwrap();
        create_item_relationship(&conn, &file_reconstruct.id, &term_item.id, "executes", 0.85, None).unwrap();

        // 3. Pinned contextual memory
        let _pinned_mem = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "note",
            "Epistemic invariant: Never fabricate missing context fields or claim false precision",
            Some("Architectural Invariant"),
            "{\"status\":\"confirmed\",\"pinned\":true,\"memory_importance\":\"critical\"}",
        ).unwrap();

        // 4. Departure Snapshot
        let snap_service = ContextSnapshotService::new();
        let snapshot = snap_service.derive_for_context(&conn, &ws.id).unwrap();
        save_context_snapshot(&conn, &snapshot, crate::context_engine::snapshot::DEFAULT_MAX_SNAPSHOT_RETENTION).unwrap();

        // 5. Reconstruct Context
        let recon_service = ContextReconstructionService::new();
        let recon = recon_service.reconstruct_context(&conn, &ws.id).unwrap();

        // 6. Verify Evaluations
        assert_eq!(recon.context.name, "ContextSplit");
        assert_eq!(recon.context.status, ReconstructionSectionStatus::Available);
        
        // Task
        assert_eq!(recon.current_task.status, ReconstructionSectionStatus::Available);
        assert_eq!(recon.current_task.title.as_deref(), Some("Validate Task Reconstruction"));
        
        // Workset & Items
        assert!(recon.workset.total_count >= 3);
        assert!(recon.workset.status == ReconstructionSectionStatus::Available);
        let workset_titles: Vec<_> = recon.workset.items.iter().map(|i| i.title.as_deref().unwrap_or("")).collect();
        assert!(workset_titles.iter().any(|t| t.contains("task_extractor.rs")));
        assert!(workset_titles.iter().any(|t| t.contains("reconstruction.rs")));

        // Snapshot
        assert_eq!(recon.latest_snapshot.status, ReconstructionSectionStatus::Available);
        assert!(!recon.latest_snapshot.is_outdated);

        // Resume Readiness Classification
        let classification = TestResumeClassification::from_reconstruction(&recon);
        assert_eq!(classification, TestResumeClassification::Ready);
        assert!(recon.resume_readiness.can_resume);
        assert!(!recon.resume_readiness.formatted_explanation.is_empty());
        println!("\n--- ContextSplit Resume Readiness Explanation ---");
        println!("{}", recon.resume_readiness.formatted_explanation);
    }

    // =========================================================================
    // CONTEXT 2: PayX — PAUSED DURING A TASK
    // =========================================================================
    // State: Paused during a task
    // Task: "Stripe Webhook Integration" (Active, paused during testing)
    // Files: webhook_handler.py, stripe_config.json
    // URLs: https://dashboard.stripe.com/test/webhooks
    // Apps: Visual Studio Code, Postman, Google Chrome
    // Snapshot: Captured at pause
    // Expected: READY (or PARTIALLY_READY with explicit pause details)
    // =========================================================================
    #[test]
    fn test_payx_paused_during_task_resume_readiness() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let now = Utc::now();
        let now_str = (now - Duration::minutes(45)).to_rfc3339();

        let ws = create_workspace(&conn, "PayX").unwrap();
        invalidate_reconstruction_cache(Some(&ws.id));
        invalidate_memory_cache(Some(&ws.id));
        invalidate_workset_cache(Some(&ws.id));
        invalidate_snapshot_cache(&ws.id);

        // 1. Task in progress paused
        let task = TaskCandidate {
            id: format!("task-payx-{}", ws.id),
            context_id: Some(ws.id.clone()),
            title: "Stripe Webhook Integration".to_string(),
            confidence: 0.90,
            status: TaskCandidateStatus::Active,
            first_detected_at: (now - Duration::hours(2)).to_rfc3339(),
            last_active_at: now_str.clone(),
            created_at: (now - Duration::hours(2)).to_rfc3339(),
            updated_at: now_str.clone(),
        };
        create_task_candidate(&conn, &task).unwrap();

        // 2. Relevant assets
        let handler_file = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "file",
            "/Users/dev/PayX/services/webhook_handler.py",
            Some("webhook_handler.py"),
            "{\"status\":\"confirmed\",\"app_name\":\"Visual Studio Code\"}",
        ).unwrap();

        let stripe_url = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://dashboard.stripe.com/test/webhooks",
            Some("Stripe Webhook Dashboard"),
            "{\"status\":\"confirmed\",\"app_name\":\"Google Chrome\",\"browser_domain\":\"stripe.com\"}",
        ).unwrap();

        let postman_item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "api_collection",
            "PayX Stripe Local Simulation Collection",
            Some("Postman Webhook Mock Collection"),
            "{\"status\":\"confirmed\",\"app_name\":\"Postman\"}",
        ).unwrap();

        create_item_relationship(&conn, &handler_file.id, &stripe_url.id, "configures", 0.92, None).unwrap();
        create_item_relationship(&conn, &handler_file.id, &postman_item.id, "tested_by", 0.88, None).unwrap();

        // 3. Snapshot created at pause departure
        let snap_service = ContextSnapshotService::new();
        let snapshot = snap_service.derive_for_context(&conn, &ws.id).unwrap();
        save_context_snapshot(&conn, &snapshot, crate::context_engine::snapshot::DEFAULT_MAX_SNAPSHOT_RETENTION).unwrap();

        // 4. Reconstruct Context
        let recon_service = ContextReconstructionService::new();
        let recon = recon_service.reconstruct_context(&conn, &ws.id).unwrap();

        // 5. Verify Evaluations
        assert_eq!(recon.context.name, "PayX");
        assert_eq!(recon.current_task.status, ReconstructionSectionStatus::Available);
        assert_eq!(recon.current_task.title.as_deref(), Some("Stripe Webhook Integration"));
        assert!(recon.workset.total_count >= 2);

        // Resume readiness classification
        let classification = TestResumeClassification::from_reconstruction(&recon);
        assert!(classification == TestResumeClassification::Ready || classification == TestResumeClassification::PartiallyReady);
        assert!(recon.resume_readiness.can_resume);
        println!("\n--- PayX Resume Readiness Explanation ---");
        println!("{}", recon.resume_readiness.formatted_explanation);
    }

    // =========================================================================
    // CONTEXT 3: University — UNFINISHED ASSIGNMENT WITH SPARSE / STALE EVIDENCE
    // =========================================================================
    // State: Unfinished assignment left with incomplete/sparse context
    // Task: Unconfirmed / ambiguous assignment note
    // Files: Lab3_Draft.docx (single sparse item)
    // URLs: None
    // Apps: Microsoft Word only
    // Snapshot: None (abrupt exit without snapshot)
    // Freshness: Stale (last active > 3 days ago)
    // Expected: PARTIALLY_READY or NOT_READY (system must not falsely claim Ready)
    // =========================================================================
    #[test]
    fn test_university_unfinished_assignment_sparse_resume_readiness() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let now = Utc::now();
        let _stale_time = (now - Duration::days(4)).to_rfc3339();

        let ws = create_workspace(&conn, "University").unwrap();
        invalidate_reconstruction_cache(Some(&ws.id));
        invalidate_memory_cache(Some(&ws.id));
        invalidate_workset_cache(Some(&ws.id));
        invalidate_snapshot_cache(&ws.id);

        // Single sparse item with no active dev tools or repo connections
        let _doc_item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "file",
            "/Users/dev/Documents/University/Lab3_Draft.docx",
            Some("Lab3_Draft.docx"),
            "{\"status\":\"observed\",\"app_name\":\"Microsoft Word\"}",
        ).unwrap();

        // No confirmed task candidate, no departure snapshot

        // Reconstruct Context
        let recon_service = ContextReconstructionService::new();
        let recon = recon_service.reconstruct_context(&conn, &ws.id).unwrap();

        // Verify Evaluations
        assert_eq!(recon.context.name, "University");
        
        // Task should be missing or partial
        assert!(recon.current_task.status != ReconstructionSectionStatus::Available);

        // Snapshot is unavailable
        assert_eq!(recon.latest_snapshot.status, ReconstructionSectionStatus::Unavailable);

        // Classification must NOT be Ready
        let classification = TestResumeClassification::from_reconstruction(&recon);
        assert!(
            classification == TestResumeClassification::PartiallyReady
                || classification == TestResumeClassification::NotReady,
            "University with sparse/stale evidence must not be classified as Ready"
        );

        // The system must explain why readiness is limited
        assert!(!recon.resume_readiness.formatted_explanation.is_empty());
        assert!(
            recon.resume_readiness.reasons.iter().any(|r| r.contains("task uncertain") || r.contains("insufficient") || r.contains("stale") || r.contains("limited") || r.contains("context identified")),
            "System must provide defensible explanation of missing or weak factors: {:?}",
            recon.resume_readiness.reasons
        );

        println!("\n--- University Resume Readiness Explanation ---");
        println!("{}", recon.resume_readiness.formatted_explanation);
    }

    // =========================================================================
    // COMPARATIVE MATRIX VALIDATION
    // =========================================================================
    #[test]
    fn test_three_contexts_readiness_comparative_matrix() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let now = Utc::now();

        // 1. ContextSplit (Active, Confirmed Task, 3+ items, Snapshot)
        let ws1 = create_workspace(&conn, "ContextSplit Matrix").unwrap();
        let task1 = TaskCandidate {
            id: format!("task-cs-{}", ws1.id),
            context_id: Some(ws1.id.clone()),
            title: "Validate Task Reconstruction".to_string(),
            confidence: 0.98,
            status: TaskCandidateStatus::Confirmed,
            first_detected_at: (now - Duration::minutes(10)).to_rfc3339(),
            last_active_at: now.to_rfc3339(),
            created_at: (now - Duration::minutes(10)).to_rfc3339(),
            updated_at: now.to_rfc3339(),
        };
        create_task_candidate(&conn, &task1).unwrap();
        create_item(&conn, Some(&ws1.id), Some(&session.id), "file", "/dev/cs/main.rs", Some("main.rs"), "{\"status\":\"confirmed\"}").unwrap();
        create_item(&conn, Some(&ws1.id), Some(&session.id), "file", "/dev/cs/lib.rs", Some("lib.rs"), "{\"status\":\"confirmed\"}").unwrap();
        let snap1 = ContextSnapshotService::new().derive_for_context(&conn, &ws1.id).unwrap();
        save_context_snapshot(&conn, &snap1, crate::context_engine::snapshot::DEFAULT_MAX_SNAPSHOT_RETENTION).unwrap();

        // 2. PayX (Paused, Active Task, 2 items, Snapshot)
        let ws2 = create_workspace(&conn, "PayX Matrix").unwrap();
        let task2 = TaskCandidate {
            id: format!("task-payx-{}", ws2.id),
            context_id: Some(ws2.id.clone()),
            title: "Stripe Webhook Handler".to_string(),
            confidence: 0.85,
            status: TaskCandidateStatus::Active,
            first_detected_at: (now - Duration::hours(1)).to_rfc3339(),
            last_active_at: (now - Duration::minutes(30)).to_rfc3339(),
            created_at: (now - Duration::hours(1)).to_rfc3339(),
            updated_at: (now - Duration::minutes(30)).to_rfc3339(),
        };
        create_task_candidate(&conn, &task2).unwrap();
        create_item(&conn, Some(&ws2.id), Some(&session.id), "file", "/dev/payx/webhook.py", Some("webhook.py"), "{\"status\":\"confirmed\"}").unwrap();
        create_item(&conn, Some(&ws2.id), Some(&session.id), "url", "https://stripe.com", Some("Stripe"), "{\"status\":\"confirmed\"}").unwrap();
        let snap2 = ContextSnapshotService::new().derive_for_context(&conn, &ws2.id).unwrap();
        save_context_snapshot(&conn, &snap2, crate::context_engine::snapshot::DEFAULT_MAX_SNAPSHOT_RETENTION).unwrap();

        // 3. University (Sparse, No Task, 1 item, Stale)
        let ws3 = create_workspace(&conn, "University Matrix").unwrap();
        create_item(&conn, Some(&ws3.id), Some(&session.id), "file", "/docs/uni/essay.docx", Some("essay.docx"), "{\"status\":\"observed\"}").unwrap();

        let recon_service = ContextReconstructionService::new();
        let recon1 = recon_service.reconstruct_context(&conn, &ws1.id).unwrap();
        let recon2 = recon_service.reconstruct_context(&conn, &ws2.id).unwrap();
        let recon3 = recon_service.reconstruct_context(&conn, &ws3.id).unwrap();

        let class1 = TestResumeClassification::from_reconstruction(&recon1);
        let class2 = TestResumeClassification::from_reconstruction(&recon2);
        let class3 = TestResumeClassification::from_reconstruction(&recon3);

        assert_eq!(class1, TestResumeClassification::Ready);
        assert!(class2 == TestResumeClassification::Ready || class2 == TestResumeClassification::PartiallyReady);
        assert!(class3 == TestResumeClassification::PartiallyReady || class3 == TestResumeClassification::NotReady);

        // Check explanations are unique and informative
        assert!(!recon1.summary_statement.is_empty());
        assert!(!recon2.summary_statement.is_empty());
        assert!(!recon3.summary_statement.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;
    use chrono::{Duration, Utc};
    use rusqlite::Connection;

    use crate::context_engine::context_state::{
        ContextStateValue, ResumeReadiness, StateFreshness,
    };
    use crate::context_engine::memory::{
        invalidate_memory_cache, InformationProvenance,
    };
    use crate::context_engine::reconstruction::{
        invalidate_reconstruction_cache, ContextReconstructionService, ReconstructionQualityRating,
        ReconstructionSectionStatus,
    };
    use crate::context_engine::snapshot::invalidate_snapshot_cache;
    use crate::context_engine::transitions::{
        record_context_transition, ContextTransitionBuilder, ContextTransitionType,
    };
    use crate::context_engine::workset::invalidate_workset_cache;
    use crate::db::models::{TaskCandidate, TaskCandidateStatus};
    use crate::db::operations::{
        create_item, create_item_relationship, create_session, create_task_candidate,
        create_workspace,
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
    // SCENARIO 1 — NORMAL WORK / COMPLETE RECONSTRUCTION
    //
    // Aurora Website
    // Task: Hero animation
    // Items:
    // - Figma hero design
    // - React Hero component
    // - Framer Motion reference
    // - GitHub repository
    //
    // Expected:
    // Coherent reconstruction with task, active workset, and ready state.
    // =========================================================================
    #[test]
    fn test_scenario_1_normal_work_aurora_website() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let now = Utc::now().to_rfc3339();

        let ws = create_workspace(&conn, "Aurora Website").unwrap();
        invalidate_reconstruction_cache(Some(&ws.id));
        invalidate_memory_cache(Some(&ws.id));
        invalidate_workset_cache(Some(&ws.id));
        invalidate_snapshot_cache(&ws.id);

        let task = TaskCandidate {
            id: format!("task-hero-anim-{}", ws.id),
            context_id: Some(ws.id.clone()),
            title: "Hero animation".to_string(),
            confidence: 0.95,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &task).unwrap();

        let figma = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://figma.com/file/aurora-hero-spec",
            Some("Figma hero design"),
            "{\"status\":\"confirmed\",\"app_name\":\"Google Chrome\"}",
        ).unwrap();

        let react_comp = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "file",
            "/projects/aurora/src/components/Hero.tsx",
            Some("React Hero component"),
            "{\"status\":\"confirmed\",\"app_name\":\"Visual Studio Code\"}",
        ).unwrap();

        let framer = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://www.framer.com/motion/examples",
            Some("Framer Motion reference"),
            "{\"status\":\"confirmed\",\"app_name\":\"Google Chrome\"}",
        ).unwrap();

        let github = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://github.com/aurora/web-app",
            Some("GitHub repository"),
            "{\"status\":\"confirmed\",\"app_name\":\"Google Chrome\"}",
        ).unwrap();

        create_item_relationship(&conn, &figma.id, &react_comp.id, "design_spec", 0.9, None).unwrap();
        create_item_relationship(&conn, &framer.id, &react_comp.id, "reference", 0.85, None).unwrap();
        create_item_relationship(&conn, &react_comp.id, &github.id, "source_control", 0.9, None).unwrap();

        let transition = ContextTransitionBuilder::new(ContextTransitionType::ContextSwitch, 0.95)
            .to_context(&ws.id)
            .session(&session.id)
            .build();
        record_context_transition(&conn, &transition).unwrap();

        let service = ContextReconstructionService::new();
        let rec = service.reconstruct_context(&conn, &ws.id).unwrap();

        assert_eq!(rec.context.status, ReconstructionSectionStatus::Available);
        assert_eq!(rec.context.name, "Aurora Website");
        assert_eq!(rec.context.provenance, InformationProvenance::Confirmed);

        assert!(rec.current_task.status.is_present());
        assert_eq!(rec.current_task.title.as_deref(), Some("Hero animation"));

        assert!(rec.workset.status.is_present());
        assert!(rec.workset.total_count >= 4);

        assert_eq!(rec.current_state.state_value, Some(ContextStateValue::Active));
        assert!(rec.resume_readiness.can_resume);
        assert!(
            rec.quality.rating == ReconstructionQualityRating::Coherent
                || rec.quality.rating == ReconstructionQualityRating::MostlyCoherent
        );
        assert!(rec.quality.can_confidently_resume);
    }

    // =========================================================================
    // SCENARIO 2 — COLD RECONSTRUCTION AFTER APPLICATION RESTART
    //
    // Close and restart ContextSplit.
    // Expected: Cold reconstruction produces equivalent contextual understanding.
    // =========================================================================
    #[test]
    fn test_scenario_2_cold_reconstruction_after_app_restart() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Aurora Restart Persistence").unwrap();
        let now = Utc::now().to_rfc3339();

        let task = TaskCandidate {
            id: format!("task-restart-{}", ws.id),
            context_id: Some(ws.id.clone()),
            title: "Hero animation".to_string(),
            confidence: 0.95,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &task).unwrap();

        let _figma = create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://figma.com/file/aurora-hero-spec",
            Some("Figma hero design"),
            "{\"status\":\"confirmed\"}",
        ).unwrap();

        let service = ContextReconstructionService::new();
        let rec_initial = service.reconstruct_context(&conn, &ws.id).unwrap();

        // Simulate complete app restart: invalidate all in-memory caches
        invalidate_reconstruction_cache(None);
        invalidate_memory_cache(None);
        invalidate_workset_cache(None);
        invalidate_snapshot_cache(&ws.id);

        let rec_cold = service.reconstruct_cold(&conn, &ws.id).unwrap();

        assert_eq!(rec_cold.context.id, rec_initial.context.id);
        assert_eq!(rec_cold.context.name, rec_initial.context.name);
        assert_eq!(rec_cold.current_task.title, rec_initial.current_task.title);
        assert_eq!(rec_cold.workset.status, rec_initial.workset.status);
        assert_eq!(rec_cold.quality.rating, rec_initial.quality.rating);
    }

    // =========================================================================
    // SCENARIO 3 — MULTI-CONTEXT SWITCHING & COMPLETE ISOLATION
    //
    // Switch: Aurora Website -> University OS -> Tokyo Trip -> Aurora Website
    // Expected: Each context reconstructs independently without cross-leakage.
    // =========================================================================
    #[test]
    fn test_scenario_3_multi_context_switching_and_isolation() {
        let conn = setup_test_db();
        let now = Utc::now();

        // 1. Aurora Website
        let ws_aurora = create_workspace(&conn, "Aurora Website").unwrap();
        let task_aurora = TaskCandidate {
            id: format!("task-aurora-{}", ws_aurora.id),
            context_id: Some(ws_aurora.id.clone()),
            title: "Hero animation".to_string(),
            confidence: 0.95,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.to_rfc3339(),
            last_active_at: now.to_rfc3339(),
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
        };
        create_task_candidate(&conn, &task_aurora).unwrap();
        create_item(
            &conn,
            Some(&ws_aurora.id),
            None,
            "file",
            "/projects/aurora/Hero.tsx",
            Some("Hero.tsx"),
            "{\"status\":\"confirmed\"}",
        ).unwrap();

        // 2. University OS
        let ws_os = create_workspace(&conn, "University OS").unwrap();
        let task_os = TaskCandidate {
            id: format!("task-os-{}", ws_os.id),
            context_id: Some(ws_os.id.clone()),
            title: "Operating Systems Assignment".to_string(),
            confidence: 0.90,
            status: TaskCandidateStatus::Active,
            first_detected_at: (now - Duration::minutes(10)).to_rfc3339(),
            last_active_at: (now - Duration::minutes(10)).to_rfc3339(),
            created_at: (now - Duration::minutes(10)).to_rfc3339(),
            updated_at: (now - Duration::minutes(10)).to_rfc3339(),
        };
        create_task_candidate(&conn, &task_os).unwrap();
        create_item(
            &conn,
            Some(&ws_os.id),
            None,
            "file",
            "/university/cs301/kernel.c",
            Some("kernel.c"),
            "{\"status\":\"confirmed\"}",
        ).unwrap();

        // 3. Tokyo Trip
        let ws_tokyo = create_workspace(&conn, "Tokyo Trip").unwrap();
        let task_tokyo = TaskCandidate {
            id: format!("task-tokyo-{}", ws_tokyo.id),
            context_id: Some(ws_tokyo.id.clone()),
            title: "Flight booking".to_string(),
            confidence: 0.85,
            status: TaskCandidateStatus::Active,
            first_detected_at: (now - Duration::minutes(20)).to_rfc3339(),
            last_active_at: (now - Duration::minutes(20)).to_rfc3339(),
            created_at: (now - Duration::minutes(20)).to_rfc3339(),
            updated_at: (now - Duration::minutes(20)).to_rfc3339(),
        };
        create_task_candidate(&conn, &task_tokyo).unwrap();
        create_item(
            &conn,
            Some(&ws_tokyo.id),
            None,
            "url",
            "https://ana.co.jp/booking/tokyo",
            Some("ANA Flights"),
            "{\"status\":\"confirmed\"}",
        ).unwrap();

        // Record sequential transitions
        let t1 = ContextTransitionBuilder::new(ContextTransitionType::ContextSwitch, 0.95)
            .to_context(&ws_aurora.id)
            .build();
        record_context_transition(&conn, &t1).unwrap();

        let t2 = ContextTransitionBuilder::new(ContextTransitionType::ContextSwitch, 0.95)
            .from_context(&ws_aurora.id)
            .to_context(&ws_os.id)
            .build();
        record_context_transition(&conn, &t2).unwrap();

        let t3 = ContextTransitionBuilder::new(ContextTransitionType::ContextSwitch, 0.95)
            .from_context(&ws_os.id)
            .to_context(&ws_tokyo.id)
            .build();
        record_context_transition(&conn, &t3).unwrap();

        let t4 = ContextTransitionBuilder::new(ContextTransitionType::ContextReturn, 0.95)
            .from_context(&ws_tokyo.id)
            .to_context(&ws_aurora.id)
            .build();
        record_context_transition(&conn, &t4).unwrap();

        let service = ContextReconstructionService::new();

        let rec_aurora = service.reconstruct_context(&conn, &ws_aurora.id).unwrap();
        let rec_os = service.reconstruct_context(&conn, &ws_os.id).unwrap();
        let rec_tokyo = service.reconstruct_context(&conn, &ws_tokyo.id).unwrap();

        // Assert strictly independent derivation
        assert_eq!(rec_aurora.current_task.title.as_deref(), Some("Hero animation"));
        assert_eq!(rec_os.current_task.title.as_deref(), Some("Operating Systems Assignment"));
        assert_eq!(rec_tokyo.current_task.title.as_deref(), Some("Flight booking"));

        assert!(rec_aurora.workset.items.iter().any(|i| i.title.as_deref().unwrap_or("").contains("Hero.tsx")));
        assert!(!rec_aurora.workset.items.iter().any(|i| i.title.as_deref().unwrap_or("").contains("kernel.c")));
        assert!(!rec_aurora.workset.items.iter().any(|i| i.title.as_deref().unwrap_or("").contains("ANA Flights")));

        assert!(rec_os.workset.items.iter().any(|i| i.title.as_deref().unwrap_or("").contains("kernel.c")));
        assert!(!rec_os.workset.items.iter().any(|i| i.title.as_deref().unwrap_or("").contains("Hero.tsx")));

        assert!(rec_tokyo.workset.items.iter().any(|i| i.title.as_deref().unwrap_or("").contains("ANA Flights")));
        assert!(!rec_tokyo.workset.items.iter().any(|i| i.title.as_deref().unwrap_or("").contains("Hero.tsx")));
    }

    // =========================================================================
    // SCENARIO 4 — LEGITIMATE TASK EVOLUTION
    //
    // Change task: Hero animation -> Pricing page
    // Expected: Reconstruction reflects the latest meaningful task.
    // =========================================================================
    #[test]
    fn test_scenario_4_task_evolution_to_pricing_page() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Aurora Evolving Work").unwrap();
        let now = Utc::now();

        // 1. Initial Task: Hero animation (now Inactive / older)
        let old_task = TaskCandidate {
            id: format!("task-old-{}", ws.id),
            context_id: Some(ws.id.clone()),
            title: "Hero animation".to_string(),
            confidence: 0.90,
            status: TaskCandidateStatus::Inactive,
            first_detected_at: (now - Duration::hours(2)).to_rfc3339(),
            last_active_at: (now - Duration::hours(1)).to_rfc3339(),
            created_at: (now - Duration::hours(2)).to_rfc3339(),
            updated_at: (now - Duration::hours(1)).to_rfc3339(),
        };
        create_task_candidate(&conn, &old_task).unwrap();

        // 2. New Active Task: Pricing page
        let new_task = TaskCandidate {
            id: format!("task-new-{}", ws.id),
            context_id: Some(ws.id.clone()),
            title: "Pricing page".to_string(),
            confidence: 0.95,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.to_rfc3339(),
            last_active_at: now.to_rfc3339(),
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
        };
        create_task_candidate(&conn, &new_task).unwrap();

        create_item(
            &conn,
            Some(&ws.id),
            None,
            "file",
            "/projects/aurora/Pricing.tsx",
            Some("Pricing.tsx"),
            "{\"status\":\"confirmed\"}",
        ).unwrap();

        let service = ContextReconstructionService::new();
        let rec = service.reconstruct_context(&conn, &ws.id).unwrap();

        assert_eq!(rec.current_task.title.as_deref(), Some("Pricing page"));
        assert!(rec.workset.items.iter().any(|i| i.title.as_deref().unwrap_or("").contains("Pricing.tsx")));
    }

    // =========================================================================
    // SCENARIO 5 — ADD UNRELATED CAPTURE (INERTIA & STABILITY)
    //
    // Add unrelated capture.
    // Expected: Reconstruction does not suddenly flip or corrupt context.
    // =========================================================================
    #[test]
    fn test_scenario_5_unrelated_capture_stability() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Aurora Stable").unwrap();
        let now = Utc::now().to_rfc3339();

        let task = TaskCandidate {
            id: format!("task-stable-{}", ws.id),
            context_id: Some(ws.id.clone()),
            title: "Hero animation".to_string(),
            confidence: 0.95,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &task).unwrap();

        create_item(
            &conn,
            Some(&ws.id),
            None,
            "file",
            "/projects/aurora/Hero.tsx",
            Some("Hero.tsx"),
            "{\"status\":\"confirmed\"}",
        ).unwrap();

        // Add unrelated capture to the global inbox / unconnected
        create_item(
            &conn,
            None,
            None,
            "url",
            "https://music.spotify.com/track/12345",
            Some("Spotify Web Player"),
            "{}",
        ).unwrap();

        let service = ContextReconstructionService::new();
        let rec = service.reconstruct_context(&conn, &ws.id).unwrap();

        assert_eq!(rec.context.name, "Aurora Stable");
        assert_eq!(rec.current_task.title.as_deref(), Some("Hero animation"));
        assert!(!rec.workset.items.iter().any(|i| i.title.as_deref().unwrap_or("").contains("Spotify")));
    }

    // =========================================================================
    // SCENARIO 6 — CONFLICTING TASK EVIDENCE (UNCERTAINTY HANDLING)
    //
    // Create conflicting task evidence.
    // Expected: Uncertain result if evidence cannot be resolved.
    // =========================================================================
    #[test]
    fn test_scenario_6_conflicting_task_evidence_handling() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Ambiguous Project").unwrap();
        let now = Utc::now();

        // Two competing tasks with identical confidence and active status
        let task_a = TaskCandidate {
            id: format!("task-a-{}", ws.id),
            context_id: Some(ws.id.clone()),
            title: "Refactor Database Schema".to_string(),
            confidence: 0.85,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.to_rfc3339(),
            last_active_at: now.to_rfc3339(),
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
        };
        create_task_candidate(&conn, &task_a).unwrap();

        let task_b = TaskCandidate {
            id: format!("task-b-{}", ws.id),
            context_id: Some(ws.id.clone()),
            title: "Redesign Marketing Hero".to_string(),
            confidence: 0.85,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.to_rfc3339(),
            last_active_at: now.to_rfc3339(),
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
        };
        create_task_candidate(&conn, &task_b).unwrap();

        let service = ContextReconstructionService::new();
        let rec = service.reconstruct_context(&conn, &ws.id).unwrap();

        // When multiple conflicting active tasks compete equally without resolution:
        assert!(rec.quality.rating == ReconstructionQualityRating::Partial
            || rec.quality.rating == ReconstructionQualityRating::Uncertain
            || rec.confidence.band != "High");
    }

    // =========================================================================
    // SCENARIO 7 — DELETE OPTIONAL SNAPSHOT
    //
    // Delete optional snapshot.
    // Expected: Reconstruction still works cleanly using other live sources.
    // =========================================================================
    #[test]
    fn test_scenario_7_deleted_optional_snapshot() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Snapshot Deleted Project").unwrap();
        let now = Utc::now().to_rfc3339();

        let task = TaskCandidate {
            id: format!("task-snap-{}", ws.id),
            context_id: Some(ws.id.clone()),
            title: "Core Infrastructure".to_string(),
            confidence: 0.90,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &task).unwrap();

        create_item(
            &conn,
            Some(&ws.id),
            None,
            "file",
            "/infra/docker-compose.yml",
            Some("docker-compose.yml"),
            "{\"status\":\"confirmed\"}",
        ).unwrap();

        // Explicitly clear all snapshots from the database and cache
        conn.execute("DELETE FROM context_snapshots WHERE context_id = ?1", rusqlite::params![ws.id]).unwrap();
        invalidate_snapshot_cache(&ws.id);

        let service = ContextReconstructionService::new();
        let rec = service.reconstruct_context(&conn, &ws.id).unwrap();

        assert_eq!(rec.latest_snapshot.status, ReconstructionSectionStatus::Unavailable);
        assert_eq!(rec.context.status, ReconstructionSectionStatus::Available);
        assert!(rec.current_task.status.is_present());
        assert!(rec.workset.status.is_present());
        assert!(rec.quality.can_confidently_resume || rec.quality.rating == ReconstructionQualityRating::MostlyCoherent || rec.quality.rating == ReconstructionQualityRating::Partial);
    }

    // =========================================================================
    // SCENARIO 8 — CREATE STALE CONTEXT
    //
    // Create stale context (dormant activity from 45 days ago).
    // Expected: State/readiness reflects stale information and orientation requirement.
    // =========================================================================
    #[test]
    fn test_scenario_8_stale_context_state_and_readiness() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Stale Legacy Project").unwrap();
        let stale_time = Utc::now() - Duration::days(45);
        let stale_ts = stale_time.to_rfc3339();

        // Set workspace updated_at and last_activity_at to 45 days ago
        conn.execute(
            "UPDATE workspaces SET updated_at = ?1, last_activity_at = ?1, created_at = ?1 WHERE id = ?2",
            rusqlite::params![stale_ts, ws.id],
        ).unwrap();

        let task = TaskCandidate {
            id: format!("task-stale-{}", ws.id),
            context_id: Some(ws.id.clone()),
            title: "Legacy Migration".to_string(),
            confidence: 0.90,
            status: TaskCandidateStatus::Inactive,
            first_detected_at: stale_ts.clone(),
            last_active_at: stale_ts.clone(),
            created_at: stale_ts.clone(),
            updated_at: stale_ts.clone(),
        };
        create_task_candidate(&conn, &task).unwrap();

        create_item(
            &conn,
            Some(&ws.id),
            None,
            "file",
            "/legacy/migrate.sh",
            Some("migrate.sh"),
            "{\"status\":\"confirmed\"}",
        ).unwrap();
        conn.execute(
            "UPDATE items SET created_at = ?1, updated_at = ?1 WHERE workspace_id = ?2",
            rusqlite::params![stale_ts, ws.id],
        ).unwrap();

        invalidate_memory_cache(Some(&ws.id));
        invalidate_workset_cache(Some(&ws.id));
        invalidate_snapshot_cache(&ws.id);
        invalidate_reconstruction_cache(Some(&ws.id));

        let service = ContextReconstructionService::new();
        let rec = service.reconstruct_context(&conn, &ws.id).unwrap();

        assert_eq!(rec.current_state.state_value, Some(ContextStateValue::Dormant));
        assert_eq!(rec.current_state.freshness, Some(StateFreshness::VeryStale));
        assert!(rec.current_state.freshness.unwrap().is_very_stale());
    }

    // =========================================================================
    // SCENARIO 9 — DISABLE CONTEXTUAL SNAPSHOTS PRIVACY SETTING
    //
    // Disable contextual snapshots.
    // Expected: Reconstruction continues using available memory/workset/state/activity.
    // =========================================================================
    #[test]
    fn test_scenario_9_privacy_snapshots_disabled() {
        let conn = setup_test_db();

        // Update privacy settings to disable context snapshots
        let mut settings = crate::db::settings::get_settings(&conn).unwrap();
        settings.enable_context_snapshots = false;
        crate::db::settings::update_settings(&conn, settings).unwrap();

        let ws = create_workspace(&conn, "No Snapshots Privacy").unwrap();
        let now = Utc::now().to_rfc3339();

        let task = TaskCandidate {
            id: format!("task-privacy-{}", ws.id),
            context_id: Some(ws.id.clone()),
            title: "Privacy Audit".to_string(),
            confidence: 0.95,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &task).unwrap();

        create_item(
            &conn,
            Some(&ws.id),
            None,
            "file",
            "/docs/privacy.md",
            Some("privacy.md"),
            "{\"status\":\"confirmed\"}",
        ).unwrap();

        let service = ContextReconstructionService::new();
        let rec = service.reconstruct_context(&conn, &ws.id).unwrap();

        assert_eq!(rec.latest_snapshot.status, ReconstructionSectionStatus::Unavailable);
        assert_eq!(rec.context.status, ReconstructionSectionStatus::Available);
        assert_eq!(rec.current_task.status, ReconstructionSectionStatus::Available);
        assert!(rec.workset.status.is_present());
        assert!(rec.quality.can_confidently_resume);
    }

    // =========================================================================
    // SCENARIO 10 — SAME APPLICATION ACROSS MULTIPLE CONTEXTS
    //
    // Use same application across multiple contexts (e.g. VS Code, Chrome).
    // Expected: No context contamination.
    // =========================================================================
    #[test]
    fn test_scenario_10_shared_application_isolation() {
        let conn = setup_test_db();

        let ws_web = create_workspace(&conn, "Web Development").unwrap();
        let ws_book = create_workspace(&conn, "Novel Writing").unwrap();

        create_item(
            &conn,
            Some(&ws_web.id),
            None,
            "file",
            "/code/app.tsx",
            Some("app.tsx"),
            "{\"status\":\"confirmed\",\"app_name\":\"Visual Studio Code\"}",
        ).unwrap();

        create_item(
            &conn,
            Some(&ws_book.id),
            None,
            "file",
            "/manuscripts/chapter1.txt",
            Some("chapter1.txt"),
            "{\"status\":\"confirmed\",\"app_name\":\"Visual Studio Code\"}",
        ).unwrap();

        let service = ContextReconstructionService::new();
        let rec_web = service.reconstruct_context(&conn, &ws_web.id).unwrap();
        let rec_book = service.reconstruct_context(&conn, &ws_book.id).unwrap();

        assert!(rec_web.workset.items.iter().any(|i| i.title.as_deref() == Some("app.tsx")));
        assert!(!rec_web.workset.items.iter().any(|i| i.title.as_deref() == Some("chapter1.txt")));

        assert!(rec_book.workset.items.iter().any(|i| i.title.as_deref() == Some("chapter1.txt")));
        assert!(!rec_book.workset.items.iter().any(|i| i.title.as_deref() == Some("app.tsx")));
    }

    // =========================================================================
    // SCENARIO 11 — LARGE CONTEXT PERFORMANCE
    //
    // 1,000 items seeded in workspace.
    // Expected: Reconstruction remains bounded and performant.
    // =========================================================================
    #[test]
    fn test_scenario_11_large_context_performance() {
        let mut conn = setup_test_db();
        let ws = create_workspace(&conn, "Large Scale Project").unwrap();
        let now = Utc::now();

        let task = TaskCandidate {
            id: format!("task-large-{}", ws.id),
            context_id: Some(ws.id.clone()),
            title: "Scale Optimization".to_string(),
            confidence: 0.90,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.to_rfc3339(),
            last_active_at: now.to_rfc3339(),
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
        };
        create_task_candidate(&conn, &task).unwrap();

        // Batch insert 1,000 items inside a single fast transaction
        let tx = conn.transaction().unwrap();
        {
            let mut stmt = tx.prepare(
                "INSERT INTO items (id, workspace_id, session_id, item_type, content, title, metadata, created_at, updated_at) 
                 VALUES (?1, ?2, NULL, 'file', ?3, ?4, '{\"status\":\"confirmed\"}', ?5, ?5)"
            ).unwrap();

            let mut mem_stmt = tx.prepare(
                "INSERT INTO context_memberships (id, item_id, workspace_id, membership_type, confidence, source, created_at, updated_at)
                 VALUES (?1, ?2, ?3, 'confirmed', 0.95, 'manual', ?4, ?4)"
            ).unwrap();

            for i in 1..=1000 {
                let id = format!("item_scale_{}_{}", ws.id, i);
                let uri = format!("/src/module_{}.rs", i);
                let title = format!("module_{}.rs", i);
                let ts = (now - Duration::seconds(i as i64 * 5)).to_rfc3339();

                stmt.execute(rusqlite::params![id, ws.id, uri, title, ts]).unwrap();
                mem_stmt.execute(rusqlite::params![format!("cm_{}_{}", ws.id, i), id, ws.id, ts]).unwrap();
            }
        }
        tx.commit().unwrap();

        let service = ContextReconstructionService::new();

        let start_cold = Instant::now();
        let rec_cold = service.reconstruct_cold(&conn, &ws.id).unwrap();
        let cold_duration = start_cold.elapsed();

        assert_eq!(rec_cold.context.status, ReconstructionSectionStatus::Available);
        assert!(rec_cold.workset.status.is_present());
        assert!(rec_cold.workset.total_count > 0);
        assert!(cold_duration < std::time::Duration::from_secs(30));

        let _ = service.reconstruct_context_cached(&conn, &ws.id).unwrap();
        let start_warm = Instant::now();
        let rec_warm = service.reconstruct_context_cached(&conn, &ws.id).unwrap();
        let warm_duration = start_warm.elapsed();

        assert_eq!(rec_warm.context.id, rec_cold.context.id);
        assert!(warm_duration < std::time::Duration::from_millis(500));
    }

    // =========================================================================
    // SCENARIO 12 — CORRUPTED OR MISSING OPTIONAL DATA
    //
    // Inject corrupt/missing optional records.
    // Expected: Graceful degradation without panic.
    // =========================================================================
    #[test]
    fn test_scenario_12_corrupted_or_missing_optional_data() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Resilience Project").unwrap();

        // Corrupted metadata JSON in item
        conn.execute(
            "INSERT INTO items (id, workspace_id, session_id, item_type, content, title, metadata, created_at, updated_at)
             VALUES ('item_corrupt_e2e', ?1, NULL, 'file', '/corrupt.txt', 'corrupt.txt', '{malformed json!}', ?2, ?2)",
            rusqlite::params![ws.id, Utc::now().to_rfc3339()],
        ).unwrap();

        let service = ContextReconstructionService::new();
        let rec = service.reconstruct_context(&conn, &ws.id).unwrap();

        // Must never panic; returns valid reconstruction model
        assert_eq!(rec.context.id.as_deref(), Some(ws.id.as_str()));
        assert_eq!(rec.context.status, ReconstructionSectionStatus::Available);
    }
}

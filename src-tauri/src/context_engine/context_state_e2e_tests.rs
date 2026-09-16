#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use rusqlite::Connection;

    use crate::context_engine::context_state::{
        ContextStateService, ResumeReadiness, StateFreshness,
    };
    use crate::context_engine::memory::{
        invalidate_memory_cache, record_memory_feedback, ContextMemoryActivityState,
        ContextMemoryService,
    };
    use crate::context_engine::overview::get_context_overview;
    use crate::context_engine::timeline::get_context_timeline;
    use crate::context_engine::transitions::{
        record_context_transition, ContextTransitionBuilder, ContextTransitionType,
    };
    use crate::context_engine::workset::{
        invalidate_workset_cache, record_workset_feedback, ContextWorksetService,
    };
    use crate::db::models::{TaskCandidate, TaskCandidateStatus};
    use crate::db::operations::{
        create_item, create_item_relationship, create_session,
        create_task_candidate, create_workspace, get_inbox_item_state, get_task_candidate,
        list_task_candidates_for_context,
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

        // Invalidate global caches before each test
        invalidate_memory_cache(None);
        invalidate_workset_cache(None);
        conn
    }

    // =========================================================================
    // SCENARIO 1 — NORMAL WORK
    //
    // Context: Aurora Website
    // Task: Hero animation
    // Items:
    // - Figma hero design
    // - React Hero component
    // - Framer Motion reference
    // - GitHub repository
    //
    // Expected:
    // Context: Aurora Website
    // Task: Hero animation
    // Last known state: recent Hero animation activity
    // Resume readiness: ready
    // =========================================================================
    #[test]
    fn test_scenario_1_normal_work_hero_animation() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let now = Utc::now().to_rfc3339();

        // 1. Setup workspace & task
        let ws = create_workspace(&conn, "Aurora Website").unwrap();
        let task = TaskCandidate {
            id: "task-hero-anim".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Hero animation".to_string(),
            confidence: 0.94,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &task).unwrap();

        // 2. Setup items
        let figma = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://figma.com/file/aurora-hero/Hero-Design",
            Some("Figma hero design"),
            r#"{"app_name":"Figma","domain":"figma.com"}"#,
        )
        .unwrap();

        let react_comp = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "code",
            "export const Hero = () => <motion.div className='hero' />;",
            Some("React Hero component"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();

        let framer_ref = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://framer.com/motion/use-scroll",
            Some("Framer Motion reference"),
            r#"{"app_name":"Arc","domain":"framer.com"}"#,
        )
        .unwrap();

        let github_repo = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://github.com/aurora-team/aurora-website",
            Some("GitHub repository"),
            r#"{"app_name":"Arc","domain":"github.com"}"#,
        )
        .unwrap();

        // Semantic relationships to establish coherent workset
        create_item_relationship(&conn, &react_comp.id, &figma.id, "similarity", 0.90, None).unwrap();
        create_item_relationship(&conn, &react_comp.id, &framer_ref.id, "similarity", 0.88, None).unwrap();
        create_item_relationship(&conn, &react_comp.id, &github_repo.id, "similarity", 0.80, None).unwrap();

        // 3. Derive ContextState
        let service = ContextStateService::new();
        let state = service.derive_for_context(&conn, &ws.id).unwrap();

        // Verification:
        // Context: Aurora Website
        assert_eq!(state.context_name.as_deref(), Some("Aurora Website"));
        // Task: Hero animation
        assert_eq!(state.task_id.as_deref(), Some(task.id.as_str()));
        assert_eq!(state.task_title.as_deref(), Some("Hero animation"));

        // Last known state: recent Hero animation activity
        assert_eq!(state.resume_state.left_off_on.as_deref(), Some("Hero animation"));
        assert!(state.resume_state.last_meaningful_activity.is_some());
        let last_activity = state.resume_state.last_meaningful_activity.as_ref().unwrap();
        assert!(last_activity.contains("Hero") || last_activity.contains("React") || last_activity.contains("Figma"));

        // Resume readiness: ready
        assert_eq!(state.readiness.readiness, ResumeReadiness::Ready);
        assert_eq!(state.resume_state.readiness.readiness, ResumeReadiness::Ready);
        assert_eq!(state.freshness, StateFreshness::Fresh);

        // Subsystem Consistency checks:
        // Context Memory
        let mem = ContextMemoryService::new().reconstruct_memory(&conn, &ws.id).unwrap();
        assert_eq!(mem.current_task_title.as_deref(), Some("Hero animation"));
        assert_eq!(mem.activity_state, ContextMemoryActivityState::Active);

        // Context Workset
        let ws_service = ContextWorksetService::new();
        let ws_set = ws_service.derive_for_workspace(&conn, &ws.id).unwrap();
        assert!(!ws_set.items.is_empty());
        assert_eq!(ws_set.items.len(), 4);

        // Context Overview
        let overview = get_context_overview(&conn, &ws.id).unwrap();
        assert_eq!(overview.workspace_id, ws.id);
        assert_eq!(overview.item_count, 4);

        // Timeline
        let timeline = get_context_timeline(&conn, &ws.id).unwrap();
        assert_eq!(timeline.items.len(), 4);

        // Inbox
        let inbox_state = get_inbox_item_state(&conn, &figma.id).unwrap();
        assert!(inbox_state.is_some());
    }

    // =========================================================================
    // SCENARIO 2 — MULTIPLE TASKS
    //
    // Aurora Website:
    // Hero animation → Pricing page → Hero animation
    //
    // Expected:
    // Current task/state reflects the latest meaningful task.
    // Previous task remains historical.
    // =========================================================================
    #[test]
    fn test_scenario_2_multiple_tasks_hero_to_pricing_to_hero() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Aurora Website").unwrap();

        let t0 = (Utc::now() - Duration::hours(3)).to_rfc3339();
        let t1 = (Utc::now() - Duration::hours(2)).to_rfc3339();
        let t2 = (Utc::now() - Duration::minutes(15)).to_rfc3339();

        // 1. First task: Hero animation (created at t0)
        let task_hero = TaskCandidate {
            id: "task-hero".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Hero animation".to_string(),
            confidence: 0.90,
            status: TaskCandidateStatus::Active,
            first_detected_at: t0.clone(),
            last_active_at: t0.clone(),
            created_at: t0.clone(),
            updated_at: t0.clone(),
        };
        create_task_candidate(&conn, &task_hero).unwrap();

        let item_hero = create_item(
            &conn,
            Some(&ws.id),
            None,
            "code",
            "const heroAnimation = true;",
            Some("Hero Animation"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();
        conn.execute("UPDATE items SET created_at = ?1, updated_at = ?1 WHERE id = ?2", rusqlite::params![t0, item_hero.id]).unwrap();

        // 2. Second task: Pricing page (created at t1)
        let task_pricing = TaskCandidate {
            id: "task-pricing".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Pricing page".to_string(),
            confidence: 0.88,
            status: TaskCandidateStatus::Active,
            first_detected_at: t1.clone(),
            last_active_at: t1.clone(),
            created_at: t1.clone(),
            updated_at: t1.clone(),
        };
        create_task_candidate(&conn, &task_pricing).unwrap();

        let item_pricing = create_item(
            &conn,
            Some(&ws.id),
            None,
            "code",
            "const pricingTable = true;",
            Some("Pricing Table"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();
        conn.execute("UPDATE items SET created_at = ?1, updated_at = ?1 WHERE id = ?2", rusqlite::params![t1, item_pricing.id]).unwrap();

        // 3. User returns to Hero animation at t2 (reactivating it with newest activity)
        let item_hero2 = create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://framer.com/motion",
            Some("Framer Motion Docs"),
            r#"{"app_name":"Arc"}"#,
        )
        .unwrap();
        conn.execute("UPDATE items SET created_at = ?1, updated_at = ?1 WHERE id = ?2", rusqlite::params![t2, item_hero2.id]).unwrap();

        conn.execute(
            "UPDATE task_candidates SET last_active_at = ?1, updated_at = ?1, confidence = 0.95 WHERE id = ?2",
            rusqlite::params![t2, task_hero.id],
        )
        .unwrap();

        invalidate_memory_cache(Some(&ws.id));
        invalidate_workset_cache(Some(&ws.id));

        let service = ContextStateService::new();
        let state = service.derive_for_context(&conn, &ws.id).unwrap();

        // Expected: Current task reflects the latest active task (Hero animation)
        assert_eq!(state.task_id.as_deref(), Some(task_hero.id.as_str()));
        assert_eq!(state.task_title.as_deref(), Some("Hero animation"));
        assert_eq!(state.resume_state.left_off_on.as_deref(), Some("Hero animation"));

        // Expected: Previous task (Pricing page) remains preserved historically
        let all_tasks = list_task_candidates_for_context(&conn, &ws.id).unwrap();
        assert_eq!(all_tasks.len(), 2);
        assert!(all_tasks.iter().any(|t| t.id == task_pricing.id && t.title == "Pricing page"));
        assert!(all_tasks.iter().any(|t| t.id == task_hero.id && t.title == "Hero animation"));

        let mem = ContextMemoryService::new().reconstruct_memory(&conn, &ws.id).unwrap();
        assert_eq!(mem.current_task_title.as_deref(), Some("Hero animation"));
        assert!(mem.recent_tasks.iter().any(|t| t.title == "Pricing page"));
    }

    // =========================================================================
    // SCENARIO 3 — CONTEXT SWITCH
    //
    // Aurora Website → University OS
    //
    // Expected:
    // University OS state becomes current.
    // Aurora state remains preserved.
    // =========================================================================
    #[test]
    fn test_scenario_3_context_switch_aurora_to_university() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let now = Utc::now().to_rfc3339();

        // 1. Create Aurora Website
        let ws_aurora = create_workspace(&conn, "Aurora Website").unwrap();
        let task_aurora = TaskCandidate {
            id: "task-aurora".to_string(),
            context_id: Some(ws_aurora.id.clone()),
            title: "Hero animation".to_string(),
            confidence: 0.92,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &task_aurora).unwrap();
        let _item_aurora = create_item(
            &conn,
            Some(&ws_aurora.id),
            Some(&session.id),
            "code",
            "const hero = true;",
            Some("Aurora Hero"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();

        // 2. Create University OS
        let ws_uni = create_workspace(&conn, "University OS").unwrap();
        let task_uni = TaskCandidate {
            id: "task-uni".to_string(),
            context_id: Some(ws_uni.id.clone()),
            title: "Process scheduling assignment".to_string(),
            confidence: 0.90,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &task_uni).unwrap();
        let _item_uni = create_item(
            &conn,
            Some(&ws_uni.id),
            Some(&session.id),
            "code",
            "void round_robin_schedule() {}",
            Some("Round Robin Scheduler"),
            r#"{"app_name":"CLion"}"#,
        )
        .unwrap();

        // 3. Record Context Transition: Aurora -> University OS
        let transition = ContextTransitionBuilder::new(ContextTransitionType::ContextSwitch, 0.95)
            .from_context(ws_aurora.id.clone())
            .to_context(ws_uni.id.clone())
            .session(session.id.clone())
            .build();
        record_context_transition(&conn, &transition).unwrap();

        // 4. Evaluate states
        let service = ContextStateService::new();
        let state_uni = service.derive_for_context(&conn, &ws_uni.id).unwrap();
        let state_aurora = service.derive_for_context(&conn, &ws_aurora.id).unwrap();

        // University OS state is active with its own task
        assert_eq!(state_uni.context_name.as_deref(), Some("University OS"));
        assert_eq!(state_uni.task_title.as_deref(), Some("Process scheduling assignment"));
        assert_eq!(state_uni.resume_state.left_off_on.as_deref(), Some("Process scheduling assignment"));

        // Aurora state is preserved independently with its own task
        assert_eq!(state_aurora.context_name.as_deref(), Some("Aurora Website"));
        assert_eq!(state_aurora.task_title.as_deref(), Some("Hero animation"));
        assert_eq!(state_aurora.resume_state.left_off_on.as_deref(), Some("Hero animation"));
    }

    // =========================================================================
    // SCENARIO 4 — CONTEXT RETURN
    //
    // Aurora Website → University OS → Aurora Website
    //
    // Expected:
    // Aurora's previous state is reconstructed.
    // =========================================================================
    #[test]
    fn test_scenario_4_context_return_aurora_uni_aurora() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let now = Utc::now().to_rfc3339();

        let ws_aurora = create_workspace(&conn, "Aurora Website").unwrap();
        let task_aurora = TaskCandidate {
            id: "task-aurora-ret".to_string(),
            context_id: Some(ws_aurora.id.clone()),
            title: "Hero animation".to_string(),
            confidence: 0.95,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &task_aurora).unwrap();
        let item_aurora = create_item(
            &conn,
            Some(&ws_aurora.id),
            Some(&session.id),
            "code",
            "export const HeroAnim = () => null;",
            Some("Hero Component"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();

        let ws_uni = create_workspace(&conn, "University OS").unwrap();
        let task_uni = TaskCandidate {
            id: "task-uni-ret".to_string(),
            context_id: Some(ws_uni.id.clone()),
            title: "Process scheduling".to_string(),
            confidence: 0.90,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &task_uni).unwrap();

        // Transition 1: Aurora -> Uni
        let tr1 = ContextTransitionBuilder::new(ContextTransitionType::ContextSwitch, 0.90)
            .from_context(ws_aurora.id.clone())
            .to_context(ws_uni.id.clone())
            .session(session.id.clone())
            .build();
        record_context_transition(&conn, &tr1).unwrap();

        // Transition 2: Uni -> Aurora (Return)
        let tr2 = ContextTransitionBuilder::new(ContextTransitionType::ContextReturn, 0.92)
            .from_context(ws_uni.id.clone())
            .to_context(ws_aurora.id.clone())
            .session(session.id.clone())
            .build();
        record_context_transition(&conn, &tr2).unwrap();

        // Reconstruct Aurora State
        let service = ContextStateService::new();
        let state_aurora = service.derive_for_context(&conn, &ws_aurora.id).unwrap();

        assert_eq!(state_aurora.context_name.as_deref(), Some("Aurora Website"));
        assert_eq!(state_aurora.task_title.as_deref(), Some("Hero animation"));
        assert_eq!(state_aurora.resume_state.left_off_on.as_deref(), Some("Hero animation"));
        assert!(state_aurora.recent_item_ids.contains(&item_aurora.id));
        assert!(matches!(
            state_aurora.readiness.readiness,
            ResumeReadiness::Ready | ResumeReadiness::MostlyReady
        ));
    }

    // =========================================================================
    // SCENARIO 5 — RANDOM CAPTURE
    //
    // Capture unrelated content.
    //
    // Expected:
    // It does not become the last meaningful state.
    // =========================================================================
    #[test]
    fn test_scenario_5_random_capture_does_not_pollute_meaningful_state() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let now = Utc::now().to_rfc3339();

        let ws = create_workspace(&conn, "Aurora Website").unwrap();
        let task = TaskCandidate {
            id: "task-hero-rand".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Hero animation".to_string(),
            confidence: 0.94,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &task).unwrap();

        let hero_item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "code",
            "export const HeroAnim = () => null;",
            Some("React Hero component"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();

        // Random capture in inbox (unassigned item)
        let _random_item = create_item(
            &conn,
            None, // unassigned
            Some(&session.id),
            "text",
            "random unformatted text snippet",
            Some("Random snippet"),
            r#"{"app_name":"Notes"}"#,
        )
        .unwrap();

        let service = ContextStateService::new();
        let state = service.derive_for_context(&conn, &ws.id).unwrap();

        // Verification: Aurora's task and meaningful state is strictly Hero animation
        assert_eq!(state.task_id.as_deref(), Some(task.id.as_str()));
        assert_eq!(state.task_title.as_deref(), Some("Hero animation"));
        assert_eq!(state.resume_state.left_off_on.as_deref(), Some("Hero animation"));
        assert_eq!(state.recent_item_ids, vec![hero_item.id]);
    }

    // =========================================================================
    // SCENARIO 6 — WEAK EVIDENCE
    //
    // Only one weak item exists.
    //
    // Expected:
    // State is uncertain or partial.
    // Do not fabricate a task.
    // =========================================================================
    #[test]
    fn test_scenario_6_weak_evidence_no_fabricated_task() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();

        let ws = create_workspace(&conn, "Minimal Space").unwrap();

        // Only one generic/weak item exists, no tasks identified
        let item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://google.com/search?q=weather",
            Some("Search"),
            r#"{"app_name":"Chrome","domain":"google.com"}"#,
        )
        .unwrap();

        let service = ContextStateService::new();
        let state = service.derive_for_context(&conn, &ws.id).unwrap();

        // Verification:
        // Do not fabricate a task
        assert_eq!(state.task_id, None);
        assert_eq!(state.task_title, None);
        assert_eq!(state.resume_state.left_off_on, None);

        // State is uncertain or partial/insufficient
        assert!(
            state.readiness.readiness == ResumeReadiness::Partial
                || state.readiness.readiness == ResumeReadiness::Insufficient
        );

        assert!(state.recent_item_ids.contains(&item.id));
    }

    // =========================================================================
    // SCENARIO 7 — LONG INACTIVITY
    //
    // No activity for a long period.
    //
    // Expected:
    // Context becomes stale/dormant according to existing lifecycle rules.
    // =========================================================================
    #[test]
    fn test_scenario_7_long_inactivity_dormant_freshness() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Old Project").unwrap();

        let time_long_ago = (Utc::now() - Duration::days(12)).to_rfc3339();

        let item = create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://archive.org",
            Some("Old Archive"),
            r#"{"app_name":"Safari"}"#,
        )
        .unwrap();

        conn.execute("UPDATE workspaces SET last_activity_at = ?1, updated_at = ?1 WHERE id = ?2", rusqlite::params![time_long_ago, ws.id]).unwrap();
        conn.execute("UPDATE items SET created_at = ?1, updated_at = ?1 WHERE id = ?2", rusqlite::params![time_long_ago, item.id]).unwrap();
        conn.execute("UPDATE context_memberships SET created_at = ?1, updated_at = ?1 WHERE item_id = ?2", rusqlite::params![time_long_ago, item.id]).unwrap();

        invalidate_memory_cache(Some(&ws.id));
        invalidate_workset_cache(Some(&ws.id));

        let service = ContextStateService::new();
        let state = service.derive_for_context(&conn, &ws.id).unwrap();

        // Verification:
        // Freshness classified as VeryStale (12 days ago)
        assert_eq!(state.freshness, StateFreshness::VeryStale);
        assert_eq!(state.freshness_display, "Last active 12 days ago");
        assert_eq!(state.resume_state.freshness, StateFreshness::VeryStale);

        // Memory lifecycle activity state reflects Dormant
        let mem = ContextMemoryService::new().reconstruct_memory(&conn, &ws.id).unwrap();
        assert_eq!(mem.activity_state, ContextMemoryActivityState::Dormant);

        // Readiness reflects age
        assert!(
            state.readiness.readiness == ResumeReadiness::Partial
                || state.readiness.readiness == ResumeReadiness::Insufficient
        );

        // State is preserved (never deleted)
        let items_in_db = crate::db::operations::list_items(&conn, Some(&ws.id)).unwrap();
        assert_eq!(items_in_db.len(), 1);
        assert_eq!(items_in_db[0].id, item.id);
    }

    // =========================================================================
    // SCENARIO 8 — APPLICATION NOISE
    //
    // Switch: Chrome → VS Code → Figma → Chrome without meaningful context evidence.
    //
    // Expected:
    // No false contextual state transitions.
    // =========================================================================
    #[test]
    fn test_scenario_8_application_noise_no_false_transitions() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let ws = create_workspace(&conn, "Aurora Website").unwrap();
        let now = Utc::now().to_rfc3339();

        let task = TaskCandidate {
            id: "task-hero-noise".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Hero animation".to_string(),
            confidence: 0.90,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &task).unwrap();

        let _hero_item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "code",
            "export const HeroAnim = () => null;",
            Some("Hero Component"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();

        // Simulate noise: Unassigned ephemeral snippets from random apps
        let _noise1 = create_item(&conn, None, Some(&session.id), "text", "tmp1", Some("untitled"), r#"{"app_name":"Chrome"}"#).unwrap();
        let _noise2 = create_item(&conn, None, Some(&session.id), "text", "tmp2", Some("untitled"), r#"{"app_name":"VS Code"}"#).unwrap();
        let _noise3 = create_item(&conn, None, Some(&session.id), "text", "tmp3", Some("untitled"), r#"{"app_name":"Figma"}"#).unwrap();

        // Context state of Aurora Website remains completely stable and intact
        let service = ContextStateService::new();
        let state = service.derive_for_context(&conn, &ws.id).unwrap();
        assert_eq!(state.task_id.as_deref(), Some(task.id.as_str()));
        assert_eq!(state.task_title.as_deref(), Some("Hero animation"));
        assert_eq!(state.resume_state.left_off_on.as_deref(), Some("Hero animation"));
    }

    // =========================================================================
    // SCENARIO 9 — RESTART
    //
    // Close ContextSplit. Restart.
    //
    // Expected:
    // Context state reconstructs correctly from persistent DB storage.
    // =========================================================================
    #[test]
    fn test_scenario_9_restart_context_state_reconstruction() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let now = Utc::now().to_rfc3339();

        let ws = create_workspace(&conn, "Aurora Website").unwrap();
        let task = TaskCandidate {
            id: "task-hero-restrt".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Hero animation".to_string(),
            confidence: 0.92,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &task).unwrap();

        let item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "code",
            "export const Hero = () => <div />;",
            Some("React Hero"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();

        // Initial state before shutdown
        let service1 = ContextStateService::new();
        let state1 = service1.derive_for_context(&conn, &ws.id).unwrap();
        assert_eq!(state1.task_title.as_deref(), Some("Hero animation"));

        // SIMULATE APP RESTART: Invalidate all memory and workset caches, drop service1
        drop(service1);
        invalidate_memory_cache(None);
        invalidate_workset_cache(None);

        // New service instance on fresh app startup
        let service2 = ContextStateService::new();
        let state2 = service2.derive_for_context(&conn, &ws.id).unwrap();

        // Verify state is accurately and identically reconstructed
        assert_eq!(state2.context_id, ws.id);
        assert_eq!(state2.context_name.as_deref(), Some("Aurora Website"));
        assert_eq!(state2.task_id.as_deref(), Some(task.id.as_str()));
        assert_eq!(state2.task_title.as_deref(), Some("Hero animation"));
        assert_eq!(state2.resume_state.left_off_on.as_deref(), Some("Hero animation"));
        assert_eq!(state2.recent_item_ids, vec![item.id]);
        assert_eq!(state2.readiness.readiness, state1.readiness.readiness);
        assert_eq!(state2.freshness, state1.freshness);
    }

    // =========================================================================
    // SCENARIO 10 — USER FEEDBACK
    //
    // User rejects an inferred state.
    //
    // Expected:
    // Feedback is recorded and future reconstruction respects it.
    // =========================================================================
    #[test]
    fn test_scenario_10_user_feedback_rejection_respected() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let now = Utc::now().to_rfc3339();

        let ws = create_workspace(&conn, "Aurora Website").unwrap();
        let task = TaskCandidate {
            id: "task-wrong-infer".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Wrong Inferred Task".to_string(),
            confidence: 0.85,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &task).unwrap();

        let item1 = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "code",
            "export const Component = () => null;",
            Some("Component"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();

        let service = ContextStateService::new();
        let state_before = service.derive_for_context(&conn, &ws.id).unwrap();
        assert_eq!(state_before.task_title.as_deref(), Some("Wrong Inferred Task"));

        // 1. User rejects the inferred task
        record_memory_feedback(&conn, &ws.id, None, Some(&task.id), false).unwrap();

        // 2. User removes item1 from workset
        record_workset_feedback(&conn, &ws.id, &item1.id, false).unwrap();

        // Invalidate caches
        invalidate_memory_cache(Some(&ws.id));
        invalidate_workset_cache(Some(&ws.id));

        // 3. Reconstruct context state after user feedback
        let state_after = service.derive_for_context(&conn, &ws.id).unwrap();

        // Task was dismissed: No longer presented as active task
        assert_ne!(state_after.task_id.as_deref(), Some(task.id.as_str()));
        assert_ne!(state_after.task_title.as_deref(), Some("Wrong Inferred Task"));
        assert_ne!(state_after.resume_state.left_off_on.as_deref(), Some("Wrong Inferred Task"));

        // Item was removed from workset: No longer in workset items
        let ws_after = ContextWorksetService::new().derive_for_workspace(&conn, &ws.id).unwrap();
        assert!(!ws_after.items.iter().any(|it| it.id == item1.id));

        // Invariant: Historical DB data remains preserved (feedback signal recorded)
        let task_db = get_task_candidate(&conn, &task.id).unwrap();
        assert_eq!(task_db.status, TaskCandidateStatus::Dismissed);
    }
}

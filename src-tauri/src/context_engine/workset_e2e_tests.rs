#[cfg(test)]
mod tests {
    use chrono::Utc;
    use rusqlite::Connection;

    use crate::context_engine::memory_query::ContextMemoryQueryService;
    use crate::context_engine::overview::get_context_overview;
    use crate::context_engine::timeline::get_context_timeline;
    use crate::context_engine::transitions::{
        record_context_transition, ContextTransitionBuilder, ContextTransitionType,
    };
    use crate::context_engine::workset::{
        invalidate_workset_cache, record_workset_feedback, ContextWorksetService,
        WorksetItemRelevance, WorksetState,
    };
    use crate::db::models::{InboxState, TaskCandidate, TaskCandidateStatus};
    use crate::db::operations::{
        confirm_membership, create_item, create_item_relationship, create_session,
        create_task_candidate, create_workspace, get_inbox_item_state,
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
        };
        crate::db::settings::update_settings(&conn, settings).unwrap();

        // Invalidate global caches before each test
        crate::context_engine::memory::invalidate_memory_cache(None);
        invalidate_workset_cache(None);
        conn
    }

    // =========================================================================
    // SCENARIO 1 — SINGLE TASK
    //
    // Context: Aurora Website
    // Task: Hero animation
    // Items:
    // - Figma hero design
    // - React Hero component
    // - Framer Motion reference
    // - GitHub repository
    // - unrelated image
    //
    // Expected workset:
    // Primary: React Hero component
    // Supporting: Figma hero design, Framer Motion reference
    // Reference: GitHub repository
    // Unrelated image: excluded
    // =========================================================================
    #[test]
    fn test_scenario_1_single_task_hero_animation() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();

        // 1. Create Aurora Website Workspace
        let ws = create_workspace(&conn, "Aurora Website").unwrap();

        // 2. Capture items
        let react_item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "code",
            "export const HeroAnimation = () => <motion.div animate={{ opacity: 1, y: 0 }} />;",
            Some("React Hero Animation component"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();

        let figma_item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://figma.com/file/aurora-hero/Hero-Design",
            Some("Figma hero design"),
            r#"{"app_name":"Figma","domain":"figma.com"}"#,
        )
        .unwrap();

        let framer_item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://framer.com/motion/use-scroll/animation-reference",
            Some("Framer Motion reference"),
            r#"{"app_name":"Arc","domain":"framer.com"}"#,
        )
        .unwrap();

        let github_item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://github.com/aurora-team/aurora-website",
            Some("GitHub repository"),
            r#"{"app_name":"GitHub","domain":"github.com"}"#,
        )
        .unwrap();

        let unrelated_image = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "image",
            "data:image/png;base64,unrelated_meme_payload",
            Some("Untitled"),
            r#"{"app_name":"Finder"}"#,
        )
        .unwrap();

        // Connect supporting items to the primary active component
        create_item_relationship(
            &conn,
            &react_item.id,
            &framer_item.id,
            "semantic_similarity",
            0.85,
            None,
        )
        .unwrap();

        create_item_relationship(
            &conn,
            &react_item.id,
            &figma_item.id,
            "temporal_proximity",
            0.75,
            None,
        )
        .unwrap();

        // 3. Establish task candidate: Hero animation
        let now = Utc::now().to_rfc3339();
        let cand = TaskCandidate {
            id: "task-hero-anim".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Hero animation".to_string(),
            confidence: 0.88,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &cand).unwrap();

        // 4. Derive Context Workset
        let service = ContextWorksetService::new();
        let workset = service.derive_for_workspace(&conn, &ws.id).unwrap();

        // Validate Task and Context
        assert_eq!(workset.context_id, ws.id);
        assert_eq!(workset.task_title.as_deref(), Some("Hero animation"));

        // Validate Primary tier
        let primary = workset.primary_items();
        assert!(!primary.is_empty(), "Must contain a primary item");
        assert_eq!(
            primary[0].id, react_item.id,
            "React Hero component should be primary"
        );

        // Validate Supporting tier
        let supporting_ids: Vec<String> = workset
            .supporting_items()
            .into_iter()
            .map(|i| i.id.clone())
            .collect();
        assert!(
            supporting_ids.contains(&figma_item.id),
            "Figma hero design should be in supporting items"
        );
        assert!(
            supporting_ids.contains(&framer_item.id),
            "Framer Motion reference should be in supporting items"
        );

        // Validate Reference tier
        let reference_items = workset
            .items
            .iter()
            .filter(|i| i.relevance == WorksetItemRelevance::Reference)
            .collect::<Vec<_>>();
        assert!(
            reference_items.iter().any(|i| i.id == github_item.id),
            "GitHub repo should be in Reference tier"
        );

        // Validate Unrelated item is excluded from workset
        assert!(
            !workset.items.iter().any(|i| i.id == unrelated_image.id),
            "Unrelated generic noise image must be excluded from workset"
        );

        // Verify ContextOverview and Timeline consistency
        let overview = get_context_overview(&conn, &ws.id).unwrap();
        assert_eq!(overview.workspace_id, ws.id);
        assert!(overview.current_task.is_some());

        let timeline = get_context_timeline(&conn, &ws.id).unwrap();
        assert_eq!(timeline.workspace_id, ws.id);
    }

    // =========================================================================
    // SCENARIO 2 — MULTIPLE TASKS
    //
    // Aurora Website:
    // Task A: Hero animation
    // Task B: Pricing page
    //
    // Expected: Switching current task changes the workset appropriately.
    // =========================================================================
    #[test]
    fn test_scenario_2_multiple_tasks_switching() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let ws = create_workspace(&conn, "Aurora Website").unwrap();

        // Hero items
        let hero_comp = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "code",
            "export const Hero = () => <h1>Aurora</h1>;",
            Some("Hero Component"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();

        let _hero_figma = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://figma.com/file/hero/Hero",
            Some("Figma Hero"),
            r#"{"app_name":"Figma"}"#,
        )
        .unwrap();

        // Pricing items
        let pricing_calc = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "code",
            "export const PricingCalculator = () => calculateTiers();",
            Some("Pricing Calculator"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();

        let stripe_doc = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://stripe.com/docs/billing/subscriptions/pricing-table",
            Some("Stripe Pricing Docs"),
            r#"{"app_name":"Arc"}"#,
        )
        .unwrap();

        // Task A: Hero animation (earlier)
        let t1 = Utc::now().to_rfc3339();
        let cand_a = TaskCandidate {
            id: "task-hero".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Hero animation".to_string(),
            confidence: 0.80,
            status: TaskCandidateStatus::Active,
            first_detected_at: t1.clone(),
            last_active_at: t1.clone(),
            created_at: t1.clone(),
            updated_at: t1.clone(),
        };
        create_task_candidate(&conn, &cand_a).unwrap();

        let service = ContextWorksetService::new();
        let workset_a = service.derive_for_workspace_uncached(&conn, &ws.id).unwrap();
        assert_eq!(workset_a.task_title.as_deref(), Some("Hero animation"));
        assert!(workset_a.items.iter().any(|i| i.id == hero_comp.id));

        // Now user switches to Task B: Pricing page (more recent, higher recency)
        let t2 = Utc::now().to_rfc3339();
        let cand_b = TaskCandidate {
            id: "task-pricing".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Pricing page implementation".to_string(),
            confidence: 0.95,
            status: TaskCandidateStatus::Active,
            first_detected_at: t2.clone(),
            last_active_at: t2.clone(),
            created_at: t2.clone(),
            updated_at: t2.clone(),
        };
        create_task_candidate(&conn, &cand_b).unwrap();

        // Invalidate cache on task switch
        invalidate_workset_cache(Some(&ws.id));
        crate::context_engine::memory::invalidate_memory_cache(Some(&ws.id));

        let workset_b = service.derive_for_workspace_uncached(&conn, &ws.id).unwrap();
        assert_eq!(workset_b.task_title.as_deref(), Some("Pricing page implementation"));

        // Pricing items should now lead the working set
        let top_item = &workset_b.items[0];
        assert!(
            top_item.id == pricing_calc.id || top_item.id == stripe_doc.id,
            "Pricing items must lead the workset after task switch"
        );
    }

    // =========================================================================
    // SCENARIO 3 — CONTEXT SWITCH
    //
    // Aurora Website → University OS
    // Expected: Aurora workset disappears from active. University OS workset becomes active.
    // =========================================================================
    #[test]
    fn test_scenario_3_context_switch() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();

        let ws_aurora = create_workspace(&conn, "Aurora Website").unwrap();
        let ws_univ = create_workspace(&conn, "University OS").unwrap();

        let aurora_item = create_item(
            &conn,
            Some(&ws_aurora.id),
            Some(&session.id),
            "code",
            "const Aurora = () => <Header />;",
            Some("Aurora App"),
            "{}",
        )
        .unwrap();

        let univ_item = create_item(
            &conn,
            Some(&ws_univ.id),
            Some(&session.id),
            "code",
            "void* virtual_memory_alloc(size_t size) { return kmalloc(size); }",
            Some("Virtual Memory Allocator"),
            r#"{"app_name":"CLion"}"#,
        )
        .unwrap();

        let service = ContextWorksetService::new();

        // Initially, Aurora is active
        let _ = crate::db::operations::touch_workspace_activity(&conn, &ws_aurora.id);
        let active_1 = service.derive_active(&conn).unwrap().unwrap();
        assert_eq!(active_1.context_id, ws_aurora.id);
        assert!(active_1.items.iter().any(|i| i.id == aurora_item.id));

        // User switches context to University OS
        let _ = crate::db::operations::touch_workspace_activity(&conn, &ws_univ.id);
        invalidate_workset_cache(None);

        let active_2 = service.derive_active(&conn).unwrap().unwrap();
        assert_eq!(active_2.context_id, ws_univ.id);
        assert!(active_2.items.iter().any(|i| i.id == univ_item.id));
        assert!(!active_2.items.iter().any(|i| i.id == aurora_item.id));
    }

    // =========================================================================
    // SCENARIO 4 — CONTEXT RETURN
    //
    // Aurora Website → University OS → Aurora Website
    // Expected: Aurora's relevant workset is reconstructed with previous state.
    // =========================================================================
    #[test]
    fn test_scenario_4_context_return() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();

        let ws_aurora = create_workspace(&conn, "Aurora Website").unwrap();
        let ws_univ = create_workspace(&conn, "University OS").unwrap();

        let hero_comp = create_item(
            &conn,
            Some(&ws_aurora.id),
            Some(&session.id),
            "code",
            "export const HeroAnimation = () => <div />;",
            Some("Hero Animation Component"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();

        let figma_hero = create_item(
            &conn,
            Some(&ws_aurora.id),
            Some(&session.id),
            "url",
            "https://figma.com/file/hero",
            Some("Figma Hero Mock"),
            r#"{"app_name":"Figma"}"#,
        )
        .unwrap();

        let now = Utc::now().to_rfc3339();
        let cand = TaskCandidate {
            id: "task-hero-anim".to_string(),
            context_id: Some(ws_aurora.id.clone()),
            title: "Hero animation".to_string(),
            confidence: 0.88,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &cand).unwrap();

        // 1. Initial work on Aurora
        let service = ContextWorksetService::new();
        let initial_aurora = service.derive_for_workspace(&conn, &ws_aurora.id).unwrap();
        assert_eq!(initial_aurora.task_title.as_deref(), Some("Hero animation"));

        // 2. Detour to University OS
        let _ = create_item(
            &conn,
            Some(&ws_univ.id),
            Some(&session.id),
            "text",
            "Page table implementation notes",
            Some("Page Tables"),
            "{}",
        )
        .unwrap();

        // 3. User returns to Aurora Website
        let trans = ContextTransitionBuilder::new(ContextTransitionType::ContextReturn, 0.92)
            .with_id("trans-return-aurora")
            .session(&session.id)
            .trigger_item(&hero_comp.id)
            .from_context(&ws_univ.id)
            .to_context(&ws_aurora.id)
            .detected_at(&now)
            .build();
        record_context_transition(&conn, &trans).unwrap();

        invalidate_workset_cache(Some(&ws_aurora.id));
        crate::context_engine::memory::invalidate_memory_cache(Some(&ws_aurora.id));

        // 4. Reconstruct workset
        let restored_aurora = service.derive_for_workspace(&conn, &ws_aurora.id).unwrap();
        assert_eq!(restored_aurora.context_id, ws_aurora.id);
        assert_eq!(restored_aurora.task_title.as_deref(), Some("Hero animation"));
        assert!(restored_aurora.items.iter().any(|i| i.id == hero_comp.id));
        assert!(restored_aurora.items.iter().any(|i| i.id == figma_hero.id));
        assert_eq!(restored_aurora.workset_state, WorksetState::DetourReturn);
    }

    // =========================================================================
    // SCENARIO 5 — RANDOM CAPTURE
    //
    // Capture an unrelated item.
    // Expected: It does not destabilize the workset.
    // =========================================================================
    #[test]
    fn test_scenario_5_random_capture_stability() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let ws = create_workspace(&conn, "Aurora Website").unwrap();

        let hero_item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "code",
            "export const HeroSection = () => <section />;",
            Some("Hero Section"),
            "{}",
        )
        .unwrap();

        let now = Utc::now().to_rfc3339();
        let cand = TaskCandidate {
            id: "task-hero-anim".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Hero animation".to_string(),
            confidence: 0.85,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &cand).unwrap();

        let service = ContextWorksetService::new();
        let initial_workset = service.derive_for_workspace(&conn, &ws.id).unwrap();
        assert_eq!(initial_workset.items[0].id, hero_item.id);

        // Random capture: unrelated search / recipe
        let random_capture = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://cooking.com/recipes/best-sourdough-pizza-crust",
            Some("Sourdough Pizza Recipe"),
            r#"{"app_name":"Chrome"}"#,
        )
        .unwrap();

        invalidate_workset_cache(Some(&ws.id));
        let updated_workset = service.derive_for_workspace(&conn, &ws.id).unwrap();

        // Hero item remains the stable primary item
        assert_eq!(updated_workset.items[0].id, hero_item.id);
        assert_eq!(updated_workset.items[0].relevance, WorksetItemRelevance::Primary);

        // Random item is not displacing hero item
        assert!(
            !updated_workset.primary_items().iter().any(|i| i.id == random_capture.id),
            "Random capture must not enter primary tier"
        );
    }

    // =========================================================================
    // SCENARIO 6 — STRONG NEW ITEM
    //
    // Capture a highly relevant item.
    // Expected: It can enter the workset quickly.
    // =========================================================================
    #[test]
    fn test_scenario_6_strong_new_item_fast_track() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let ws = create_workspace(&conn, "Aurora Website").unwrap();

        let _hero_comp = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "code",
            "export const Hero = () => <div />;",
            Some("Hero Component"),
            "{}",
        )
        .unwrap();

        let now = Utc::now().to_rfc3339();
        let cand = TaskCandidate {
            id: "task-hero-anim".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Hero animation".to_string(),
            confidence: 0.85,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &cand).unwrap();

        let service = ContextWorksetService::new();
        let _ = service.derive_for_workspace(&conn, &ws.id).unwrap();

        // Capture a strong new item matching the active task title and subject
        let strong_new_item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "code",
            "export const HeroAnimationSpringConfig = { stiffness: 300, damping: 20 };",
            Some("Hero Animation Spring Config"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();

        // User confirmed membership
        let _ = confirm_membership(&conn, &strong_new_item.id, &ws.id).unwrap();

        invalidate_workset_cache(Some(&ws.id));
        let updated_workset = service.derive_for_workspace(&conn, &ws.id).unwrap();

        assert!(
            updated_workset.items.iter().any(|i| i.id == strong_new_item.id),
            "Strong relevant new item must enter the working set quickly"
        );
    }

    // =========================================================================
    // SCENARIO 7 — USER REMOVAL
    //
    // User removes an item.
    // Expected: Negative feedback is recorded. Future ranking respects the feedback.
    // Item is NOT deleted from the database.
    // =========================================================================
    #[test]
    fn test_scenario_7_user_removal_feedback() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Aurora Website").unwrap();

        let item1 = create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Framer Motion curves reference documentation",
            Some("Framer Curves Spec"),
            "{}",
        )
        .unwrap();

        let item2 = create_item(
            &conn,
            Some(&ws.id),
            None,
            "code",
            "export const Hero = () => <div />;",
            Some("Hero Component"),
            "{}",
        )
        .unwrap();

        let service = ContextWorksetService::new();
        let initial_workset = service.derive_for_workspace(&conn, &ws.id).unwrap();
        assert!(initial_workset.items.iter().any(|i| i.id == item1.id));
        assert!(initial_workset.items.iter().any(|i| i.id == item2.id));

        // User removes item1
        record_workset_feedback(&conn, &ws.id, &item1.id, false).unwrap();

        let updated_workset = service.derive_for_workspace(&conn, &ws.id).unwrap();

        // item1 is now excluded from the workset
        assert!(
            !updated_workset.items.iter().any(|i| i.id == item1.id),
            "Removed item must be excluded from workset"
        );
        assert!(
            updated_workset.items.iter().any(|i| i.id == item2.id),
            "Remaining items must be preserved"
        );

        // Non-destructive verification: item1 is still in SQLite
        let db_item1 = crate::db::operations::get_item(&conn, &item1.id).unwrap();
        assert_eq!(db_item1.id, item1.id);
        assert_eq!(db_item1.title.as_deref(), Some("Framer Curves Spec"));
    }

    // =========================================================================
    // SCENARIO 8 — LOW CONFIDENCE TASK
    //
    // Strong context but weak task.
    // Expected: Context workset still exists. Task-specific ranking remains conservative.
    // =========================================================================
    #[test]
    fn test_scenario_8_low_confidence_task_conservative() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Aurora Website").unwrap();

        let _doc1 = create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://aurora.design/brand-guidelines",
            Some("Brand Guidelines"),
            "{}",
        )
        .unwrap();

        let _doc2 = create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "General project notes and requirements",
            Some("Project Notes"),
            "{}",
        )
        .unwrap();

        // Create low-confidence, generic task
        let now = Utc::now().to_rfc3339();
        let cand = TaskCandidate {
            id: "task-browsing".to_string(),
            context_id: Some(ws.id.clone()),
            title: "General browsing".to_string(),
            confidence: 0.25,
            status: TaskCandidateStatus::Detected,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &cand).unwrap();

        let service = ContextWorksetService::new();
        let workset = service.derive_for_workspace(&conn, &ws.id).unwrap();

        // Workset exists
        assert_eq!(workset.context_id, ws.id);
        assert!(!workset.items.is_empty(), "Context workset must still contain context items");

        // Conservative task: has_active_task is false because confidence < 0.45
        assert!(!workset.has_active_task(), "Low confidence task must not be flagged as active");
    }

    // =========================================================================
    // SCENARIO 9 — EMPTY CONTEXT
    //
    // Expected: "No recent work to continue." Do not fabricate a workset.
    // =========================================================================
    #[test]
    fn test_scenario_9_empty_context() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Brand New Empty Workspace").unwrap();

        let service = ContextWorksetService::new();
        let workset = service.derive_for_workspace(&conn, &ws.id).unwrap();

        assert_eq!(workset.context_id, ws.id);
        assert_eq!(workset.context_name, "Brand New Empty Workspace");
        assert!(workset.items.is_empty(), "Empty context must have 0 workset items");
        assert!(workset.relevant_items.is_empty());
        assert!(workset.important_references.is_empty());
        assert_eq!(workset.workset_state, WorksetState::InsufficientEvidence);
    }

    // =========================================================================
    // SYSTEM CONSISTENCY VERIFICATION
    //
    // Verify:
    // - Context Engine
    // - Task Engine
    // - Context Memory
    // - Context Transitions
    // - Workset Builder
    // - Inbox
    // - Context Overview
    // - Timeline
    // remain consistent.
    // =========================================================================
    #[test]
    fn test_full_system_consistency() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();

        // 1. Inbox capture (unfiled item)
        let inbox_item = create_item(
            &conn,
            None,
            Some(&session.id),
            "url",
            "https://aurora.design/components/hero",
            Some("Hero Design Specs"),
            r#"{"app_name":"Chrome","domain":"aurora.design"}"#,
        )
        .unwrap();

        let inbox_state = get_inbox_item_state(&conn, &inbox_item.id).unwrap().unwrap();
        assert_eq!(inbox_state.state, InboxState::New);

        // 2. Create Context & Confirm item membership
        let ws = create_workspace(&conn, "Aurora Design System").unwrap();
        let _ = confirm_membership(&conn, &inbox_item.id, &ws.id).unwrap();

        let organized_state = get_inbox_item_state(&conn, &inbox_item.id).unwrap().unwrap();
        assert_eq!(organized_state.state, InboxState::Organized);

        // 3. Task Engine creates Task Candidate
        let now = Utc::now().to_rfc3339();
        let cand = TaskCandidate {
            id: "task-ds-hero".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Design system hero components".to_string(),
            confidence: 0.85,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &cand).unwrap();

        // 4. Memory Query returns consistent memory snapshot
        let memory_response = ContextMemoryQueryService::new()
            .get_context_memory(&conn, &ws.id)
            .unwrap()
            .unwrap();
        assert_eq!(memory_response.context_id, ws.id);
        assert_eq!(memory_response.task.as_deref(), Some("Design system hero components"));

        // 5. Workset derives consistent items
        let workset = ContextWorksetService::new().derive_for_workspace(&conn, &ws.id).unwrap();
        assert_eq!(workset.context_id, ws.id);
        assert!(workset.items.iter().any(|i| i.id == inbox_item.id));

        // 6. Overview and Timeline return unified state
        let overview = get_context_overview(&conn, &ws.id).unwrap();
        assert_eq!(overview.workspace_id, ws.id);
        assert_eq!(overview.timeline.total_items, 1);

        let timeline = get_context_timeline(&conn, &ws.id).unwrap();
        assert_eq!(timeline.workspace_id, ws.id);
        assert!(!timeline.chronological_activity.is_empty());
    }
}

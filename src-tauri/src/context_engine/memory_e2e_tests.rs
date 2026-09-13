#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use rusqlite::Connection;

    use crate::context_engine::memory::{
        record_memory_feedback,
        InformationProvenance,
    };
    use crate::context_engine::memory_query::ContextMemoryQueryService;
    use crate::context_engine::overview::get_context_overview;
    use crate::context_engine::segments::ContextActivitySegmenter;
    use crate::context_engine::summary::ContextActivitySummaryService;
    use crate::context_engine::timeline::get_context_timeline;
    use crate::context_engine::transitions::{
        ContextTransitionBuilder, ContextTransitionType,
    };
    use crate::context_engine::work_sequence::RecentWorkSequenceService;
    use crate::db::models::{InboxState, TaskCandidateStatus};
    use crate::db::operations::{
        confirm_membership, create_item, create_session, create_task_candidate,
        create_workspace, get_inbox_item_state, get_task_candidate,
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
        };
        crate::db::settings::update_settings(&conn, settings).unwrap();

        // Invalidate global memory cache before each test
        crate::context_engine::memory::invalidate_memory_cache(None);
        conn
    }

    // =========================================================================
    // SCENARIO 1: Workflow Progression
    // User works on:
    // Aurora Website -> Figma hero -> React Hero component -> animation -> GitHub
    // Expected:
    // Context: Aurora Website
    // Task: Hero animation
    // =========================================================================
    #[test]
    fn test_scenario_1_workflow_progression_hero_animation() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();

        // 1. Create Aurora Website Workspace
        let ws = create_workspace(&conn, "Aurora Website").unwrap();

        // 2. User captures workflow sequence: Figma -> React component -> animation -> GitHub
        let item1 = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://figma.com/file/123/Aurora-Hero-Designs",
            Some("Hero layout design"),
            r#"{"app_name":"Figma","domain":"figma.com"}"#,
        )
        .unwrap();
        conn.execute(
            "UPDATE items SET created_at = '2026-09-14 10:00:00' WHERE id = ?1",
            [&item1.id],
        )
        .unwrap();

        let item2 = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "code",
            "export const HeroAnimation = () => <motion.div animate={{ opacity: 1 }} />;",
            Some("React Hero component"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();
        conn.execute(
            "UPDATE items SET created_at = '2026-09-14 10:15:00' WHERE id = ?1",
            [&item2.id],
        )
        .unwrap();

        let item3 = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "Keyframes for hero entrance animation and easing curves",
            Some("Hero animation keyframes"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();
        conn.execute(
            "UPDATE items SET created_at = '2026-09-14 10:30:00' WHERE id = ?1",
            [&item3.id],
        )
        .unwrap();

        let item4 = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://github.com/aurora/website/pull/42",
            Some("PR #42: Add hero animation and responsive styles"),
            r#"{"app_name":"Safari","domain":"github.com"}"#,
        )
        .unwrap();
        conn.execute(
            "UPDATE items SET created_at = '2026-09-14 10:45:00' WHERE id = ?1",
            [&item4.id],
        )
        .unwrap();

        // Register active task candidate
        let cand = crate::db::models::TaskCandidate {
            id: "task-hero-anim".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Hero animation".to_string(),
            confidence: 0.88,
            status: TaskCandidateStatus::Active,
            first_detected_at: "2026-09-14T10:00:00Z".to_string(),
            last_active_at: "2026-09-14T10:45:00Z".to_string(),
            created_at: "2026-09-14T10:00:00Z".to_string(),
            updated_at: "2026-09-14T10:45:00Z".to_string(),
        };
        create_task_candidate(&conn, &cand).unwrap();

        let ref_now = DateTime::parse_from_rfc3339("2026-09-14T10:50:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let service = ContextMemoryQueryService::new().with_now(ref_now);

        // 3. Verify ContextMemory
        let mem = service.get_context_memory(&conn, &ws.id).unwrap().unwrap();
        assert_eq!(mem.context, "Aurora Website");
        assert_eq!(mem.context_id, ws.id);
        assert_eq!(mem.task.as_deref(), Some("Hero animation"));
        assert!(mem.task_confidence.is_some());
        assert_eq!(mem.state, "active");
        assert_eq!(mem.recent_items.len(), 4);

        // Verify Explainability
        assert!(!mem.explanation.why_reasons.is_empty());
        assert!(!mem.explanation.supporting_evidence.is_empty());

        // 4. Verify Context Overview consistency
        let overview = get_context_overview(&conn, &ws.id).unwrap();
        assert_eq!(overview.name, "Aurora Website");
        assert_eq!(overview.item_count, 4);
        assert!(overview.current_task.is_some());
        let cur = overview.current_task.unwrap();
        assert!(!cur.title.is_empty());
        assert!(!cur.recent_activity.is_empty());

        // 5. Verify Timeline consistency
        let timeline = get_context_timeline(&conn, &ws.id).unwrap();
        assert_eq!(timeline.total_items, 4);
    }

    // =========================================================================
    // SCENARIO 2: Context Switch & Return
    // User switches: Aurora Website -> University OS -> Aurora Website
    // Expected: Memory correctly reflects the returned context.
    // =========================================================================
    #[test]
    fn test_scenario_2_context_switch_and_return() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();

        let ws_aurora = create_workspace(&conn, "Aurora Website").unwrap();
        let ws_os = create_workspace(&conn, "University OS").unwrap();

        // 1. Aurora Website (09:00 - 10:00)
        let a1 = create_item(
            &conn,
            Some(&ws_aurora.id),
            Some(&session.id),
            "text",
            "Hero animation styles",
            Some("Hero styles"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();
        conn.execute("UPDATE items SET created_at = '2026-09-14 09:30:00' WHERE id = ?1", [&a1.id]).unwrap();

        // 2. Switch to University OS (10:30 - 11:30)
        let os1 = create_item(
            &conn,
            Some(&ws_os.id),
            Some(&session.id),
            "code",
            "int schedule_processes(Process* queue) { ... }",
            Some("Process scheduling assignment"),
            r#"{"app_name":"CLion"}"#,
        )
        .unwrap();
        conn.execute("UPDATE items SET created_at = '2026-09-14 10:30:00' WHERE id = ?1", [&os1.id]).unwrap();

        // 3. Return to Aurora Website (12:00 - 12:30)
        let a2 = create_item(
            &conn,
            Some(&ws_aurora.id),
            Some(&session.id),
            "text",
            "Polishing hero animation transitions",
            Some("Hero transition polish"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();
        conn.execute("UPDATE items SET created_at = '2026-09-14 12:00:00' WHERE id = ?1", [&a2.id]).unwrap();

        // Record ContextReturn transition
        let trans = ContextTransitionBuilder::new(ContextTransitionType::ContextReturn, 0.92)
            .with_id("trans-return-aurora")
            .session(&session.id)
            .trigger_item(&a2.id)
            .from_context(&ws_os.id)
            .to_context(&ws_aurora.id)
            .detected_at("2026-09-14T12:00:00Z")
            .build();
        crate::context_engine::transitions::record_context_transition(&conn, &trans).unwrap();

        let ref_now = DateTime::parse_from_rfc3339("2026-09-14T12:15:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let service = ContextMemoryQueryService::new().with_now(ref_now);

        let mem = service.get_context_memory(&conn, &ws_aurora.id).unwrap().unwrap();
        assert_eq!(mem.context, "Aurora Website");
        assert!(mem.is_context_return);
        assert!(mem.evidence.iter().any(|e| e.contains("Returned to Aurora Website")));

        // Verify Work Sequence Service
        let mut segmenter = ContextActivitySegmenter::new();
        let segments = segmenter.build_all_segments(&conn).unwrap();
        let seq_service = RecentWorkSequenceService::new();
        let seq = seq_service.reconstruct_from_segments(&conn, &segments, Some(60), Some(ref_now));

        assert_eq!(
            seq.last_active_context.as_ref().map(|c| c.context_name.as_str()),
            Some("Aurora Website")
        );
        assert!(seq.context_returns >= 1);
    }

    // =========================================================================
    // SCENARIO 3: Strong Context Evidence, Weak Task Evidence
    // User has strong context evidence but weak task evidence.
    // Expected: Context identified. Task remains uncertain.
    // =========================================================================
    #[test]
    fn test_scenario_3_strong_context_weak_task_evidence() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Aurora Website").unwrap();

        // Add 3 generic browsing items in Aurora Website context without a cohesive task pattern
        let item1 = create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://aurora.design/settings",
            Some("Settings"),
            r#"{"app_name":"Chrome","domain":"aurora.design"}"#,
        )
        .unwrap();
        conn.execute(
            "UPDATE items SET created_at = '2026-09-14 10:00:00' WHERE id = ?1",
            [&item1.id],
        )
        .unwrap();

        let item2 = create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://google.com/search?q=aurora",
            Some("Google Search"),
            r#"{"app_name":"Chrome"}"#,
        )
        .unwrap();
        conn.execute(
            "UPDATE items SET created_at = '2026-09-14 10:01:00' WHERE id = ?1",
            [&item2.id],
        )
        .unwrap();

        let item3 = create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Quick reminder note",
            Some("Note"),
            r#"{"app_name":"Notes"}"#,
        )
        .unwrap();
        conn.execute(
            "UPDATE items SET created_at = '2026-09-14 10:02:00' WHERE id = ?1",
            [&item3.id],
        )
        .unwrap();

        let ref_now = DateTime::parse_from_rfc3339("2026-09-14T10:10:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let service = ContextMemoryQueryService::new().with_now(ref_now);

        let mem = service.get_context_memory(&conn, &ws.id).unwrap().unwrap();
        assert_eq!(mem.context, "Aurora Website");
        assert_eq!(mem.context_confidence, "high");
        // Task remains uncertain
        assert!(mem.task.is_none());
        assert_eq!(mem.task_confidence.as_deref(), None);

        // Memory statement acknowledges context without inventing a fake task
        assert!(mem.memory_statement.contains("Aurora Website"));
        assert!(!mem.memory_statement.contains("working on '"));
    }

    // =========================================================================
    // SCENARIO 4: Single Unrelated Item Capture
    // User captures one unrelated item.
    // Expected: Do not invent a task. Single item ceiling enforced.
    // =========================================================================
    #[test]
    fn test_scenario_4_single_unrelated_item_no_task_invention() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Tokyo Trip").unwrap();

        let item = create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Flight itinerary confirmation snippet",
            Some("Flight confirmation"),
            r#"{"app_name":"Mail"}"#,
        )
        .unwrap();
        conn.execute(
            "UPDATE items SET created_at = '2026-09-14 10:00:00' WHERE id = ?1",
            [&item.id],
        )
        .unwrap();

        let ref_now = DateTime::parse_from_rfc3339("2026-09-14T10:05:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let service = ContextMemoryQueryService::new().with_now(ref_now);

        let mem = service.get_context_memory(&conn, &ws.id).unwrap().unwrap();
        assert_eq!(mem.context, "Tokyo Trip");
        // Single item should not invent a confident task
        assert!(mem.task.is_none());
        assert_eq!(mem.recent_items.len(), 1);
        assert_eq!(mem.recent_items[0].provenance, InformationProvenance::Confirmed);
    }

    // =========================================================================
    // SCENARIO 5: Inactivity / Long Gap -> Recently Active & Dormancy
    // User leaves for several hours.
    // Expected: Context becomes recently_active/dormant according to lifecycle rules.
    // =========================================================================
    #[test]
    fn test_scenario_5_inactivity_transition_to_recently_active_and_dormant() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Aurora Website").unwrap();

        let item = create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Component layout",
            Some("Component layout"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();
        conn.execute(
            "UPDATE items SET created_at = '2026-09-14 08:00:00' WHERE id = ?1",
            [&item.id],
        )
        .unwrap();

        // 1. Query 2 hours later -> recently_active
        let now_2h = DateTime::parse_from_rfc3339("2026-09-14T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let service_2h = ContextMemoryQueryService::new().with_now(now_2h);
        let mem_2h = service_2h.get_context_memory(&conn, &ws.id).unwrap().unwrap();
        assert_eq!(mem_2h.state, "recently_active");

        // 2. Query 48 hours later -> dormant
        let now_48h = DateTime::parse_from_rfc3339("2026-09-16T08:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let service_48h = ContextMemoryQueryService::new().with_now(now_48h);
        let mem_48h = service_48h.get_context_memory(&conn, &ws.id).unwrap().unwrap();
        assert_eq!(mem_48h.state, "dormant");

        // 3. Verify activity segmenter excludes the 48h gap from active duration
        let mut segmenter = ContextActivitySegmenter::new();
        let segments = segmenter.build_all_segments(&conn).unwrap();
        assert_eq!(segments.len(), 1);
        assert!(segments[0].duration_seconds < 1800);
    }

    // =========================================================================
    // SCENARIO 6: Return to Old / Dormant Context
    // User returns to an old context.
    // Expected: Memory updates to the returned context.
    // =========================================================================
    #[test]
    fn test_scenario_6_return_to_old_context_wakes_up() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Tokyo Trip").unwrap();

        // Old item 7 days ago
        let old_item = create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://tokyo-guide.com",
            Some("Tokyo Guide"),
            r#"{"app_name":"Safari"}"#,
        )
        .unwrap();
        conn.execute(
            "UPDATE items SET created_at = '2026-09-07 10:00:00' WHERE id = ?1",
            [&old_item.id],
        )
        .unwrap();

        // Fresh item today returning to Tokyo Trip
        let new_item = create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://shinjuku-hotels.com",
            Some("Shinjuku Hotels"),
            r#"{"app_name":"Safari"}"#,
        )
        .unwrap();
        conn.execute(
            "UPDATE items SET created_at = '2026-09-14 14:00:00' WHERE id = ?1",
            [&new_item.id],
        )
        .unwrap();

        let ref_now = DateTime::parse_from_rfc3339("2026-09-14T14:05:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let service = ContextMemoryQueryService::new().with_now(ref_now);

        let current = service.get_current_memory(&conn).unwrap().unwrap();
        assert_eq!(current.context, "Tokyo Trip");
        assert_eq!(current.state, "active");
        assert_eq!(current.recent_items.len(), 2);
    }

    // =========================================================================
    // SCENARIO 7: User Corrects an Inferred Task (Memory Feedback)
    // User corrects an inferred task:
    // Expected: Feedback is recorded, candidate dismissed, future inference respects correction.
    // =========================================================================
    #[test]
    fn test_scenario_7_user_corrects_inferred_task_and_future_inference() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Aurora Website").unwrap();

        let cand = crate::db::models::TaskCandidate {
            id: "task-wrong-hero".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Old Hero draft".to_string(),
            confidence: 0.80,
            status: TaskCandidateStatus::Active,
            first_detected_at: "2026-09-14T10:00:00Z".to_string(),
            last_active_at: "2026-09-14T10:30:00Z".to_string(),
            created_at: "2026-09-14T10:00:00Z".to_string(),
            updated_at: "2026-09-14T10:30:00Z".to_string(),
        };
        create_task_candidate(&conn, &cand).unwrap();

        // 1. Initial memory derives "Old Hero draft"
        let service = ContextMemoryQueryService::new();
        let mem1 = service.get_context_memory(&conn, &ws.id).unwrap().unwrap();
        assert_eq!(mem1.task.as_deref(), Some("Old Hero draft"));

        // 2. User provides negative feedback ("Not what I was doing")
        let dismissed = record_memory_feedback(&conn, &ws.id, Some("Old Hero draft"), Some(&cand.id), false).unwrap();
        assert!(dismissed);

        // Verify task candidate is Dismissed
        let updated_cand = get_task_candidate(&conn, &cand.id).unwrap();
        assert_eq!(updated_cand.status, TaskCandidateStatus::Dismissed);

        // 3. Subsequent memory derivation suppresses the dismissed candidate
        let mem2 = service.get_context_memory(&conn, &ws.id).unwrap().unwrap();
        assert_ne!(mem2.task.as_deref(), Some("Old Hero draft"));

        // 4. User adds positive correction for "Hero animation"
        let confirmed = record_memory_feedback(&conn, &ws.id, Some("Hero animation"), None, true).unwrap();
        assert!(confirmed);

        let mem3 = service.get_context_memory(&conn, &ws.id).unwrap().unwrap();
        assert_eq!(mem3.task.as_deref(), Some("Hero animation"));
        assert_eq!(mem3.provenance, InformationProvenance::Confirmed);
    }

    // =========================================================================
    // SCENARIO 8: Zero / Empty Activity
    // No meaningful activity exists.
    // Expected: Truthful indication without inventing fake memories.
    // =========================================================================
    #[test]
    fn test_scenario_8_empty_database_no_recent_work() {
        let conn = setup_test_db();
        let service = ContextMemoryQueryService::new();

        // 1. get_current_memory on empty DB returns None
        let cur = service.get_current_memory(&conn).unwrap();
        assert!(cur.is_none());

        // 2. get_today_memory returns truthful empty summary
        let today_resp = service.get_today_memory(&conn).unwrap();
        assert!(!today_resp.has_sufficient_evidence);
        assert!(today_resp.contexts_used.is_empty());
        assert!(today_resp.summary_statement.contains("No meaningful activity recorded"));

        // 3. Unregistered workspace returns None
        let nonexistent = service.get_context_memory(&conn, "nonexistent-ws").unwrap();
        assert!(nonexistent.is_none());
    }

    // =========================================================================
    // SCENARIO 9: Full Cross-System End-to-End Consistency
    // Verify: ContextMemory, TaskCandidate, ContextTransition,
    // ContextActivitySegment, ContextActivitySummary, Inbox, Context Overview,
    // Timeline all remain mutually consistent.
    // =========================================================================
    #[test]
    fn test_scenario_9_full_cross_system_consistency() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();

        // 1. Create Workspace & Items
        let ws = create_workspace(&conn, "TripWise").unwrap();

        let item1 = create_item(
            &conn,
            None, // Captured into Inbox first
            Some(&session.id),
            "url",
            "https://tripadvisor.com/tokyo-itinerary",
            Some("Tokyo 5-day itinerary"),
            r#"{"app_name":"Safari","domain":"tripadvisor.com"}"#,
        )
        .unwrap();

        // Confirm item into workspace
        confirm_membership(&conn, &item1.id, &ws.id).unwrap();

        // Verify Inbox State is Organized
        let inbox_state = get_inbox_item_state(&conn, &item1.id).unwrap();
        assert_eq!(inbox_state.map(|s| s.state), Some(InboxState::Organized));

        // Add 2 more items
        let _item2 = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "Day 1: Shinjuku and Shibuya crossing",
            Some("Tokyo itinerary draft"),
            r#"{"app_name":"Notes"}"#,
        )
        .unwrap();

        let _item3 = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://booking.com/hotels/tokyo",
            Some("Tokyo hotels booking"),
            r#"{"app_name":"Safari","domain":"booking.com"}"#,
        )
        .unwrap();

        // Add TaskCandidate
        let cand = crate::db::models::TaskCandidate {
            id: "task-tokyo-plan".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Tokyo trip planning".to_string(),
            confidence: 0.85,
            status: TaskCandidateStatus::Active,
            first_detected_at: "2026-09-14T10:00:00Z".to_string(),
            last_active_at: "2026-09-14T10:30:00Z".to_string(),
            created_at: "2026-09-14T10:00:00Z".to_string(),
            updated_at: "2026-09-14T10:30:00Z".to_string(),
        };
        create_task_candidate(&conn, &cand).unwrap();

        // 2. Query Context Memory
        let service = ContextMemoryQueryService::new();
        let mem = service.get_context_memory(&conn, &ws.id).unwrap().unwrap();
        assert_eq!(mem.context, "TripWise");
        assert_eq!(mem.task.as_deref(), Some("Tokyo trip planning"));
        assert_eq!(mem.recent_items.len(), 3);

        // 3. Query Activity Summary
        let mut summary_service = ContextActivitySummaryService::new();
        let summary = summary_service.summarize_all(&conn).unwrap();
        assert!(!summary.contexts_used.is_empty());
        assert_eq!(summary.contexts_used[0].context.as_ref().map(|c| c.name.as_str()), Some("TripWise"));

        // 4. Query Context Overview
        let overview = get_context_overview(&conn, &ws.id).unwrap();
        assert_eq!(overview.name, "TripWise");
        assert_eq!(overview.item_count, 3);
        assert!(overview.current_task.is_some());

        // 5. Query Timeline
        let timeline = get_context_timeline(&conn, &ws.id).unwrap();
        assert_eq!(timeline.total_items, 3);
        assert_eq!(timeline.items.len(), 3);

        // 6. Query Task Candidates
        let candidates = list_task_candidates_for_context(&conn, &ws.id).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].title, "Tokyo trip planning");
    }
}

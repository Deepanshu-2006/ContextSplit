#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use rusqlite::Connection;

    use crate::context_engine::context_state::{
        ContextStateValue, ResumeReadiness, StateFreshness,
    };
    use crate::context_engine::memory::{
        invalidate_memory_cache, InformationProvenance,
    };
    use crate::context_engine::snapshot::{
        invalidate_snapshot_cache, ContextSnapshotBuilder,
        ContextSnapshotComparator, ContextSnapshotService, SnapshotQualityRating,
        SnapshotTrigger, SnapshotTriggerClassifier, SnapshotTriggerCoalescer,
        SnapshotWorksetItemReference, DEFAULT_MAX_SNAPSHOT_RETENTION,
    };
    use crate::context_engine::transitions::{
        record_context_transition, ContextTransitionBuilder, ContextTransitionType,
    };
    use crate::context_engine::workset::{
        invalidate_workset_cache, WorksetItemRelevance,
    };
    use crate::db::models::{TaskCandidate, TaskCandidateStatus};
    use crate::db::operations::{
        create_item, create_item_relationship, create_session, create_task_candidate,
        create_workspace, get_inbox_item_state, get_item, list_context_snapshots_bounded,
        list_items_for_session, list_task_candidates_for_context, save_context_snapshot,
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
    // SCENARIO 1 — NORMAL WORK
    //
    // Aurora Website
    // Task: Hero animation
    // Items:
    // - Figma hero design
    // - React Hero component
    // - Framer Motion reference
    // - GitHub repository
    //
    // Expected snapshot:
    // Context: Aurora Website
    // Task: Hero animation
    // State: recently_active (or active)
    // Workset: relevant Aurora items
    // Resume readiness: useful/ready
    // =========================================================================
    #[test]
    fn test_scenario_1_normal_work() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let now = Utc::now().to_rfc3339();

        let ws = create_workspace(&conn, "Aurora Normal Work").unwrap();
        invalidate_snapshot_cache(&ws.id);
        invalidate_memory_cache(Some(&ws.id));
        invalidate_workset_cache(Some(&ws.id));

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

        create_item_relationship(&conn, &react_comp.id, &figma.id, "similarity", 0.90, None).unwrap();
        create_item_relationship(&conn, &react_comp.id, &framer_ref.id, "similarity", 0.88, None).unwrap();
        create_item_relationship(&conn, &react_comp.id, &github_repo.id, "similarity", 0.82, None).unwrap();

        let service = ContextSnapshotService::new();
        let snapshot = service.derive_for_context(&conn, &ws.id).unwrap();

        assert_eq!(snapshot.context_id, ws.id);
        assert_eq!(snapshot.context_name, "Aurora Normal Work");
        assert!(snapshot.task_title.is_some());
        assert_eq!(snapshot.task_title.as_deref(), Some("Hero animation"));
        assert!(snapshot.state == ContextStateValue::RecentlyActive || snapshot.state == ContextStateValue::Active);
        assert!(!snapshot.workset_item_references.is_empty());
        assert!(snapshot.workset_item_references.iter().any(|i| i.id == react_comp.id));
        assert!(snapshot.resume_readiness.readiness == ResumeReadiness::Ready || snapshot.resume_readiness.readiness == ResumeReadiness::MostlyReady);
        assert!(snapshot.quality.rating == SnapshotQualityRating::Complete || snapshot.quality.rating == SnapshotQualityRating::Useful);
    }

    // =========================================================================
    // SCENARIO 2 — TASK CHANGE
    //
    // Hero design → Hero animation
    //
    // Expected:
    // New snapshot reflects the task transition.
    // Previous snapshot remains available in history.
    // =========================================================================
    #[test]
    fn test_scenario_2_task_change() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let t0 = (Utc::now() - Duration::minutes(30)).to_rfc3339();
        let t1 = Utc::now().to_rfc3339();

        let ws = create_workspace(&conn, "Aurora Task Change").unwrap();
        invalidate_snapshot_cache(&ws.id);
        invalidate_memory_cache(Some(&ws.id));
        invalidate_workset_cache(Some(&ws.id));

        // 1. Initial Task: Hero design
        let task_old = TaskCandidate {
            id: format!("task-hero-design-{}", ws.id),
            context_id: Some(ws.id.clone()),
            title: "Hero design".to_string(),
            confidence: 0.90,
            status: TaskCandidateStatus::Active,
            first_detected_at: t0.clone(),
            last_active_at: t0.clone(),
            created_at: t0.clone(),
            updated_at: t0.clone(),
        };
        create_task_candidate(&conn, &task_old).unwrap();

        create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://figma.com/file/aurora-hero/design",
            Some("Figma hero mockups"),
            r#"{"app_name":"Figma"}"#,
        )
        .unwrap();

        let service = ContextSnapshotService::new();
        let snap1 = service.derive_for_context(&conn, &ws.id).unwrap();
        assert_eq!(snap1.task_title.as_deref(), Some("Hero design"));
        service.record_snapshot_history(snap1.clone());

        // 2. Task Transition to: Hero animation
        invalidate_snapshot_cache(&ws.id);
        invalidate_memory_cache(Some(&ws.id));
        invalidate_workset_cache(Some(&ws.id));

        let task_new = TaskCandidate {
            id: format!("task-hero-anim-{}", ws.id),
            context_id: Some(ws.id.clone()),
            title: "Hero animation".to_string(),
            confidence: 0.96,
            status: TaskCandidateStatus::Active,
            first_detected_at: t1.clone(),
            last_active_at: t1.clone(),
            created_at: t1.clone(),
            updated_at: t1.clone(),
        };
        create_task_candidate(&conn, &task_new).unwrap();

        create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "code",
            "const animation = useAnimation();",
            Some("Hero animation hook"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();

        let snap2 = service.derive_for_context(&conn, &ws.id).unwrap();
        assert_eq!(snap2.task_title.as_deref(), Some("Hero animation"));
        service.record_snapshot_history(snap2.clone());

        // History check: both snapshots available
        let history = service.get_snapshot_history(&ws.id).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history.latest().unwrap().task_title.as_deref(), Some("Hero animation"));
        assert_eq!(history.previous().unwrap().task_title.as_deref(), Some("Hero design"));
    }

    // =========================================================================
    // SCENARIO 3 — CONTEXT SWITCH
    //
    // Aurora Website → University OS
    //
    // Expected:
    // University OS gets its own state.
    // Aurora snapshot remains intact.
    // =========================================================================
    #[test]
    fn test_scenario_3_context_switch() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let now = Utc::now().to_rfc3339();

        let ws_aurora = create_workspace(&conn, "Aurora Context Switch").unwrap();
        let ws_uni = create_workspace(&conn, "University OS Switch").unwrap();

        invalidate_snapshot_cache(&ws_aurora.id);
        invalidate_snapshot_cache(&ws_uni.id);

        // Setup Aurora
        create_item(&conn, Some(&ws_aurora.id), Some(&session.id), "url", "https://aurora.design", Some("Aurora Home"), "{}").unwrap();
        let service = ContextSnapshotService::new();
        let snap_aurora = service.derive_for_context(&conn, &ws_aurora.id).unwrap();
        service.record_snapshot_history(snap_aurora);

        // Switch to University OS
        create_item(&conn, Some(&ws_uni.id), Some(&session.id), "url", "https://canvas.edu/courses/os101", Some("OS 101 Lecture Notes"), "{}").unwrap();
        let task_uni = TaskCandidate {
            id: format!("task-os-lecture-{}", ws_uni.id),
            context_id: Some(ws_uni.id.clone()),
            title: "Virtual Memory Assignment".to_string(),
            confidence: 0.91,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &task_uni).unwrap();

        let snap_uni = service.derive_for_context(&conn, &ws_uni.id).unwrap();
        service.record_snapshot_history(snap_uni);

        // University OS has its own state
        let hist_uni = service.get_snapshot_history(&ws_uni.id).unwrap();
        assert_eq!(hist_uni.context_name, "University OS Switch");
        assert_eq!(hist_uni.latest().unwrap().task_title.as_deref(), Some("Virtual Memory Assignment"));

        // Aurora remains intact
        let hist_aurora = service.get_snapshot_history(&ws_aurora.id).unwrap();
        assert_eq!(hist_aurora.context_name, "Aurora Context Switch");
        assert_eq!(hist_aurora.len(), 1);
    }

    // =========================================================================
    // SCENARIO 4 — CONTEXT RETURN
    //
    // Aurora → University OS → Aurora
    //
    // Expected:
    // Aurora's latest meaningful snapshot is reconstructed.
    // =========================================================================
    #[test]
    fn test_scenario_4_context_return() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let now = Utc::now().to_rfc3339();

        let ws_aurora = create_workspace(&conn, "Aurora Context Return").unwrap();
        let ws_uni = create_workspace(&conn, "University OS Return").unwrap();

        invalidate_snapshot_cache(&ws_aurora.id);
        invalidate_snapshot_cache(&ws_uni.id);

        let task_aurora = TaskCandidate {
            id: format!("task-aurora-hero-{}", ws_aurora.id),
            context_id: Some(ws_aurora.id.clone()),
            title: "Hero component refinement".to_string(),
            confidence: 0.93,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &task_aurora).unwrap();
        create_item(&conn, Some(&ws_aurora.id), Some(&session.id), "code", "export function Hero() {}", Some("Hero.tsx"), "{}").unwrap();

        let service = ContextSnapshotService::new();
        let snap_aurora_initial = service.derive_for_context(&conn, &ws_aurora.id).unwrap();
        service.record_snapshot_history(snap_aurora_initial);

        // Switch to University OS
        create_item(&conn, Some(&ws_uni.id), Some(&session.id), "url", "https://canvas.edu", Some("Canvas"), "{}").unwrap();
        let _ = service.derive_for_context(&conn, &ws_uni.id).unwrap();

        // Return to Aurora
        let return_trans = ContextTransitionBuilder::new(
            ContextTransitionType::ContextReturn,
            0.92,
        )
        .from_context(&ws_uni.id)
        .to_context(&ws_aurora.id)
        .build();
        record_context_transition(&conn, &return_trans).unwrap();

        // Reconstruct Aurora snapshot
        let snap_aurora_return = service.derive_for_context(&conn, &ws_aurora.id).unwrap();
        assert_eq!(snap_aurora_return.context_id, ws_aurora.id);
        assert_eq!(snap_aurora_return.task_title.as_deref(), Some("Hero component refinement"));
    }

    // =========================================================================
    // SCENARIO 5 — RANDOM CAPTURE
    //
    // Capture unrelated item.
    //
    // Expected:
    // No unnecessary snapshot creation via classifier & coalescer.
    // =========================================================================
    #[test]
    fn test_scenario_5_random_capture() {
        let trigger = SnapshotTriggerClassifier::classify_event("clipboard_capture", false);
        assert!(trigger.is_suppressed());
        assert!(!trigger.is_meaningful());

        let coalescer = SnapshotTriggerCoalescer::new();
        let scheduled = coalescer.schedule("ctx-random-capture", trigger);
        assert!(!scheduled, "Trivial item capture should not schedule snapshot refresh");
    }

    // =========================================================================
    // SCENARIO 6 — NOISY APP SWITCHING
    //
    // Chrome → VS Code → Figma → Chrome without meaningful context changes.
    //
    // Expected:
    // No meaningless snapshots created.
    // =========================================================================
    #[test]
    fn test_scenario_6_noisy_app_switching() {
        let trigger1 = SnapshotTriggerClassifier::classify_event("app_switch", false);
        let trigger2 = SnapshotTriggerClassifier::classify_event("window_focus", false);
        let trigger3 = SnapshotTriggerClassifier::classify_event("mouse_move", false);

        assert!(trigger1.is_suppressed());
        assert!(trigger2.is_suppressed());
        assert!(trigger3.is_suppressed());

        let coalescer = SnapshotTriggerCoalescer::new();
        assert!(!coalescer.schedule("ctx-noisy-app-switch", trigger1));
        assert!(!coalescer.schedule("ctx-noisy-app-switch", trigger2));
        assert!(!coalescer.schedule("ctx-noisy-app-switch", trigger3));
        assert_eq!(coalescer.pending_contexts_count(), 0);
    }

    // =========================================================================
    // SCENARIO 7 — LOW EVIDENCE
    //
    // One weak item.
    //
    // Expected:
    // Snapshot quality is partial/insufficient.
    // Do not fabricate a task.
    // =========================================================================
    #[test]
    fn test_scenario_7_low_evidence() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();

        let ws = create_workspace(&conn, "Sparse Workspace Low Evidence").unwrap();
        // 1 single weak item with no task candidates
        create_item(&conn, Some(&ws.id), Some(&session.id), "url", "https://example.com", Some("Example"), "{}").unwrap();

        let service = ContextSnapshotService::new();
        let snapshot = service.derive_for_context(&conn, &ws.id).unwrap();

        assert_eq!(snapshot.task_title, None, "Must not fabricate a task on low evidence");
        assert!(
            snapshot.quality.rating == SnapshotQualityRating::Partial
                || snapshot.quality.rating == SnapshotQualityRating::Insufficient
        );
        assert!(snapshot.quality.evidence_diversity_score < 0.60);
    }

    // =========================================================================
    // SCENARIO 8 — WHAT CHANGED
    //
    // Previous snapshot: Hero design
    // Current snapshot: Hero animation
    //
    // Expected:
    // Comparator correctly identifies the meaningful changes.
    // =========================================================================
    #[test]
    fn test_scenario_8_what_changed() {
        let prev_snap = ContextSnapshotBuilder::new("ws-diff-test", "Aurora What Changed")
            .with_task(Some("task-1".to_string()), "Hero design", 0.90, InformationProvenance::Confirmed)
            .with_state(ContextStateValue::RecentlyActive)
            .with_freshness(StateFreshness::Recent, 7200)
            .add_workset_item(SnapshotWorksetItemReference {
                id: "item-figma".to_string(),
                title: Some("Figma design".to_string()),
                item_type: "url".to_string(),
                relevance: WorksetItemRelevance::Primary,
                relevance_score: 0.95,
                provenance: InformationProvenance::Confirmed,
                app_name: Some("Figma".to_string()),
                domain: Some("figma.com".to_string()),
                content_snippet: "https://figma.com/hero".to_string(),
                reasons: vec!["Core design mockup".to_string()],
                created_at: Utc::now().to_rfc3339(),
                last_active_at: Some(Utc::now().to_rfc3339()),
            })
            .add_workset_item(SnapshotWorksetItemReference {
                id: "item-old-ref".to_string(),
                title: Some("Hero reference".to_string()),
                item_type: "url".to_string(),
                relevance: WorksetItemRelevance::Reference,
                relevance_score: 0.70,
                provenance: InformationProvenance::Inferred,
                app_name: None,
                domain: None,
                content_snippet: "Old inspiration".to_string(),
                reasons: vec!["Old reference".to_string()],
                created_at: Utc::now().to_rfc3339(),
                last_active_at: Some(Utc::now().to_rfc3339()),
            })
            .build();

        let curr_snap = ContextSnapshotBuilder::new("ws-diff-test", "Aurora What Changed")
            .with_task(Some("task-2".to_string()), "Hero animation", 0.95, InformationProvenance::Confirmed)
            .with_state(ContextStateValue::Active)
            .with_freshness(StateFreshness::Fresh, 30)
            .add_workset_item(SnapshotWorksetItemReference {
                id: "item-react".to_string(),
                title: Some("React Hero component".to_string()),
                item_type: "code".to_string(),
                relevance: WorksetItemRelevance::Primary,
                relevance_score: 0.98,
                provenance: InformationProvenance::Confirmed,
                app_name: Some("VS Code".to_string()),
                domain: None,
                content_snippet: "export const Hero = () => {};".to_string(),
                reasons: vec!["Main implementation".to_string()],
                created_at: Utc::now().to_rfc3339(),
                last_active_at: Some(Utc::now().to_rfc3339()),
            })
            .add_workset_item(SnapshotWorksetItemReference {
                id: "item-framer".to_string(),
                title: Some("Framer Motion reference".to_string()),
                item_type: "url".to_string(),
                relevance: WorksetItemRelevance::Supporting,
                relevance_score: 0.88,
                provenance: InformationProvenance::Inferred,
                app_name: Some("Arc".to_string()),
                domain: Some("framer.com".to_string()),
                content_snippet: "https://framer.com/motion".to_string(),
                reasons: vec!["Animation docs".to_string()],
                created_at: Utc::now().to_rfc3339(),
                last_active_at: Some(Utc::now().to_rfc3339()),
            })
            .add_workset_item(SnapshotWorksetItemReference {
                id: "item-github".to_string(),
                title: Some("GitHub repository".to_string()),
                item_type: "url".to_string(),
                relevance: WorksetItemRelevance::Reference,
                relevance_score: 0.80,
                provenance: InformationProvenance::Inferred,
                app_name: Some("Arc".to_string()),
                domain: Some("github.com".to_string()),
                content_snippet: "https://github.com/aurora/site".to_string(),
                reasons: vec!["Source control".to_string()],
                created_at: Utc::now().to_rfc3339(),
                last_active_at: Some(Utc::now().to_rfc3339()),
            })
            .build();

        let diff = ContextSnapshotComparator::compare(Some(&prev_snap), &curr_snap);

        assert!(diff.has_meaningful_changes);
        assert!(diff.task_changed);
        assert_eq!(diff.previous_task.as_deref(), Some("Hero design"));
        assert_eq!(diff.current_task.as_deref(), Some("Hero animation"));
        assert_eq!(diff.added_items.len(), 3);
        assert!(diff.added_items.iter().any(|i| i.title.as_deref() == Some("React Hero component")));
        assert!(diff.added_items.iter().any(|i| i.title.as_deref() == Some("Framer Motion reference")));
        assert!(diff.added_items.iter().any(|i| i.title.as_deref() == Some("GitHub repository")));
        assert_eq!(diff.removed_items.len(), 2);
        assert!(diff.removed_items.iter().any(|i| i.title.as_deref() == Some("Hero reference")));
    }

    // =========================================================================
    // SCENARIO 9 — APPLICATION RESTART
    //
    // Close ContextSplit. Restart.
    //
    // Expected:
    // Latest useful snapshot can be reconstructed from database.
    // =========================================================================
    #[test]
    fn test_scenario_9_application_restart() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Aurora Application Restart").unwrap();

        let snapshot = ContextSnapshotBuilder::new(&ws.id, "Aurora Application Restart")
            .with_task(Some("task-persisted".to_string()), "Hero animation", 0.95, InformationProvenance::Confirmed)
            .with_state(ContextStateValue::RecentlyActive)
            .with_freshness(StateFreshness::Stale, 86400)
            .add_workset_item(SnapshotWorksetItemReference {
                id: "item-hero".to_string(),
                title: Some("React Hero component".to_string()),
                item_type: "code".to_string(),
                relevance: WorksetItemRelevance::Primary,
                relevance_score: 0.95,
                provenance: InformationProvenance::Confirmed,
                app_name: Some("VS Code".to_string()),
                domain: None,
                content_snippet: "Hero code".to_string(),
                reasons: vec!["Active component".to_string()],
                created_at: Utc::now().to_rfc3339(),
                last_active_at: Some(Utc::now().to_rfc3339()),
            })
            .build();

        // 1. Save to SQLite database
        save_context_snapshot(&conn, &snapshot, 20).unwrap();

        // 2. Simulate complete application restart: flush all in-memory caches
        invalidate_snapshot_cache(&ws.id);

        // 3. New service instance reading from persistent DB storage
        let service = ContextSnapshotService::new();
        let restored_history = service.get_snapshot_history_from_db(&conn, &ws.id).unwrap().unwrap();

        assert_eq!(restored_history.context_id, ws.id);
        assert_eq!(restored_history.len(), 1);
        let latest = restored_history.latest().unwrap();
        assert_eq!(latest.task_title.as_deref(), Some("Hero animation"));
        assert_eq!(latest.workset_item_references.len(), 1);
        assert_eq!(latest.workset_item_references[0].title.as_deref(), Some("React Hero component"));
    }

    // =========================================================================
    // SCENARIO 10 — PRIVACY DISABLED
    //
    // Disable snapshots.
    //
    // Expected:
    // New snapshots stop being generated.
    // Capture and Inbox continue working.
    // =========================================================================
    #[test]
    fn test_scenario_10_privacy_disabled() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let ws = create_workspace(&conn, "Aurora Privacy Disabled").unwrap();

        // 1. Disable context snapshots in privacy settings
        let mut settings = crate::db::settings::get_settings(&conn).unwrap();
        settings.enable_context_snapshots = false;
        crate::db::settings::update_settings(&conn, settings).unwrap();

        // 2. Derive snapshot should return None when checked or handled via trigger
        assert!(!ContextSnapshotService::is_snapshots_enabled(&conn));
        let service = ContextSnapshotService::new();
        let trigger_res = service.handle_trigger(
            &conn,
            &ws.id,
            SnapshotTrigger::TaskTransition {
                task_id: None,
                task_title: "Hero animation".to_string(),
                transition_type: "inferred".to_string(),
            },
        ).unwrap();
        assert!(trigger_res.is_none(), "When snapshots are disabled, handle_trigger must return None");

        // 3. Basic capture and inbox MUST continue working seamlessly
        let captured_item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "Privacy test captured snippet",
            Some("Captured Item"),
            "{}",
        )
        .unwrap();

        let inbox_state = get_inbox_item_state(&conn, &captured_item.id).unwrap().unwrap();
        assert_eq!(inbox_state.item_id, captured_item.id);

        let session_items = list_items_for_session(&conn, &session.id).unwrap();
        assert_eq!(session_items.len(), 1);
        assert_eq!(session_items[0].id, captured_item.id);
    }

    // =========================================================================
    // SCENARIO 11 — SNAPSHOT RETENTION
    //
    // Generate many snapshots.
    //
    // Expected:
    // Retention remains bounded.
    // Underlying Items and contextual evidence remain intact.
    // =========================================================================
    #[test]
    fn test_scenario_11_snapshot_retention_and_evidence_integrity() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let ws = create_workspace(&conn, "Aurora Snapshot Retention").unwrap();

        // Create underlying contextual items
        let item1 = create_item(&conn, Some(&ws.id), Some(&session.id), "code", "fn hero() {}", Some("Hero component"), "{}").unwrap();
        let item2 = create_item(&conn, Some(&ws.id), Some(&session.id), "url", "https://figma.com", Some("Figma file"), "{}").unwrap();

        // Create underlying task candidate
        let task = TaskCandidate {
            id: format!("task-hero-anim-{}", ws.id),
            context_id: Some(ws.id.clone()),
            title: "Hero animation".to_string(),
            confidence: 0.95,
            status: TaskCandidateStatus::Active,
            first_detected_at: Utc::now().to_rfc3339(),
            last_active_at: Utc::now().to_rfc3339(),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };
        create_task_candidate(&conn, &task).unwrap();

        let service = ContextSnapshotService::new();

        // Generate 35 snapshots over time
        for i in 0..35 {
            let timestamp = (Utc::now() + Duration::seconds(i * 10)).to_rfc3339();
            let snap = ContextSnapshotBuilder::new(&ws.id, "Aurora Snapshot Retention")
                .with_task(Some(task.id.clone()), format!("Hero animation step {}", i), 0.90, InformationProvenance::Confirmed)
                .add_workset_item(SnapshotWorksetItemReference {
                    id: item1.id.clone(),
                    title: Some("Hero component".to_string()),
                    item_type: "code".to_string(),
                    relevance: WorksetItemRelevance::Primary,
                    relevance_score: 0.90,
                    provenance: InformationProvenance::Confirmed,
                    app_name: None,
                    domain: None,
                    content_snippet: "fn hero() {}".to_string(),
                    reasons: vec!["Active item".to_string()],
                    created_at: timestamp.clone(),
                    last_active_at: Some(timestamp.clone()),
                })
                .build();

            // Override id & created_at for uniqueness
            let mut snap_custom = snap;
            snap_custom.id = format!("snap-gen-{}-{}", ws.id, i);
            snap_custom.created_at = timestamp;

            service.record_snapshot_history_with_db(&conn, snap_custom).unwrap();
        }

        // 1. Verify in-memory history is bounded to DEFAULT_MAX_SNAPSHOT_RETENTION (20)
        let in_memory_history = service.get_snapshot_history(&ws.id).unwrap();
        assert_eq!(in_memory_history.len(), DEFAULT_MAX_SNAPSHOT_RETENTION);

        // 2. Verify database records are pruned to DEFAULT_MAX_SNAPSHOT_RETENTION (20)
        let db_snapshots = list_context_snapshots_bounded(&conn, &ws.id, 100).unwrap();
        assert_eq!(db_snapshots.len(), DEFAULT_MAX_SNAPSHOT_RETENTION);

        // 3. CRITICAL INVARIANT: Underlying items and task candidates MUST remain 100% intact!
        assert_eq!(get_item(&conn, &item1.id).unwrap().id, item1.id, "Item 1 must not be deleted during snapshot pruning");
        assert_eq!(get_item(&conn, &item2.id).unwrap().id, item2.id, "Item 2 must not be deleted during snapshot pruning");

        let session_items = list_items_for_session(&conn, &session.id).unwrap();
        assert_eq!(session_items.len(), 2, "Session items count must remain intact");

        let task_candidates = list_task_candidates_for_context(&conn, &ws.id).unwrap();
        assert_eq!(task_candidates.len(), 1, "Task candidates must remain intact");
    }
}

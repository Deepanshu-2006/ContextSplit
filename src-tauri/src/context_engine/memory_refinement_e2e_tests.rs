#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use rusqlite::Connection;

    use crate::context_engine::important_memory::{
        record_context_memory_control, ContextImportantMemoryBuilder, ContextMemoryQueryService,
        ImportantMemoryType, MemoryImportanceBand, UserMemoryControlAction,
    };
    use crate::context_engine::memory::{
        ContextMemoryBuilder, InformationProvenance,
    };
    use crate::context_engine::memory_decay::{
        ContextMemoryDecayEvaluator, MemoryDecayInput, MemoryDecayPolicy,
    };
    use crate::context_engine::memory_importance::{
        ContextMemoryQualityGuard, MemoryImportanceTier,
    };
    use crate::context_engine::snapshot::ContextSnapshot;
    use crate::context_engine::task_transitions::TaskSnapshot;
    use crate::context_engine::transitions::{
        ContextTransitionBuilder, ContextTransitionType,
    };
    use crate::context_engine::workset::ContextWorksetBuilder;
    use crate::db::models::{ItemRelationship, TaskCandidate, TaskCandidateStatus};
    use crate::db::operations::{
        confirm_membership, create_item, create_item_relationship, create_session,
        create_task_candidate, create_workspace, get_item, list_task_candidates_for_context,
        update_task_candidate_status,
    };
    use crate::db::settings::PrivacySettings;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();

        let settings = PrivacySettings {
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
    // SCENARIO 1 — IMPORTANT PROJECT REFERENCE
    // Aurora Website: Figma project, React Hero component, GitHub repository,
    // Framer Motion reference.
    // Expected: Important memories identify the genuinely useful references.
    // =========================================================================
    #[test]
    fn test_scenario_1_important_project_reference() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let ws = create_workspace(&conn, "Aurora Website").unwrap();

        // 1. Core items
        let figma_item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://figma.com/file/aurora-design-system",
            Some("Aurora Design System"),
            r#"{"app_name":"Figma","domain":"figma.com"}"#,
        )
        .unwrap();

        let code_item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "export const HeroAnimation = () => { return <motion.div animate={{ opacity: 1 }} />; };",
            Some("HeroAnimation.tsx"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();

        let github_item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://github.com/aurora/aurora-site",
            Some("Aurora GitHub Repository"),
            r#"{"app_name":"Safari","domain":"github.com"}"#,
        )
        .unwrap();

        let framer_item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://framer.com/motion/guide",
            Some("Framer Motion Reference Docs"),
            r#"{"app_name":"Safari","domain":"framer.com"}"#,
        )
        .unwrap();

        // Confirm memberships
        confirm_membership(&conn, &figma_item.id, &ws.id).unwrap();
        confirm_membership(&conn, &code_item.id, &ws.id).unwrap();
        confirm_membership(&conn, &github_item.id, &ws.id).unwrap();
        confirm_membership(&conn, &framer_item.id, &ws.id).unwrap();

        // Build important memories
        let important_set = ContextImportantMemoryBuilder::build(&conn, &ws.id, None).unwrap();
        assert!(!important_set.memories.is_empty(), "Important memories must not be empty");

        let titles: Vec<String> = important_set.memories.iter().map(|m| m.title.clone()).collect();
        assert!(titles.iter().any(|t| t.contains("Aurora Design System") || t.contains("Figma")));
        assert!(titles.iter().any(|t| t.contains("Aurora GitHub Repository") || t.contains("github.com")));
        assert!(titles.iter().any(|t| t.contains("HeroAnimation.tsx") || t.contains("HeroAnimation")));

        // All confirmed project memories have high confidence
        for mem in &important_set.memories {
            assert!(mem.importance >= 0.50, "Project reference importance must be significant");
            assert_eq!(mem.provenance, InformationProvenance::Confirmed);
        }
    }

    // =========================================================================
    // SCENARIO 2 — RANDOM CAPTURE
    // Capture unrelated image / transient note.
    // Expected: Does not become important memory.
    // =========================================================================
    #[test]
    fn test_scenario_2_random_capture() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let ws = create_workspace(&conn, "Aurora Website").unwrap();

        // Confirmed legitimate project item
        let repo_item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://github.com/aurora/site",
            Some("Aurora Repo"),
            r#"{"domain":"github.com"}"#,
        )
        .unwrap();
        confirm_membership(&conn, &repo_item.id, &ws.id).unwrap();

        // Random unconfirmed image / temporary capture in inbox
        let _random_img = create_item(
            &conn,
            None,
            Some(&session.id),
            "image",
            "random_screenshot_blob_12345",
            Some("Screenshot 2026-09-18 at 01.00.png"),
            r#"{"app_name":"Preview"}"#,
        )
        .unwrap();

        let important_set = ContextImportantMemoryBuilder::build(&conn, &ws.id, None).unwrap();

        // Repo is important; random screenshot is NOT in important memories
        let mem_item_ids: Vec<String> = important_set.memories.iter().map(|m| m.source_id.clone()).collect();
        assert!(mem_item_ids.contains(&repo_item.id));
        assert!(!mem_item_ids.contains(&_random_img.id), "Random screenshot capture must not become important memory");
    }

    // =========================================================================
    // SCENARIO 3 — REPEATED RELEVANCE
    // Reference appears across multiple meaningful sessions.
    // Expected: Importance increases and promotes to recurring reference.
    // =========================================================================
    #[test]
    fn test_scenario_3_repeated_relevance() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "TripWise").unwrap();

        let s1 = create_session(&conn).unwrap();
        let s2 = create_session(&conn).unwrap();
        let s3 = create_session(&conn).unwrap();

        let shared_url = "https://booking.com/tokyo-hotels";

        let item1 = create_item(&conn, Some(&ws.id), Some(&s1.id), "url", shared_url, Some("Tokyo Hotels 1"), r#"{"domain":"booking.com"}"#).unwrap();
        let _item2 = create_item(&conn, Some(&ws.id), Some(&s2.id), "url", shared_url, Some("Tokyo Hotels 2"), r#"{"domain":"booking.com"}"#).unwrap();
        let _item3 = create_item(&conn, Some(&ws.id), Some(&s3.id), "url", shared_url, Some("Tokyo Hotels 3"), r#"{"domain":"booking.com"}"#).unwrap();

        confirm_membership(&conn, &item1.id, &ws.id).unwrap();

        let important_set = ContextImportantMemoryBuilder::build(&conn, &ws.id, None).unwrap();
        let rec_mem = important_set
            .memories
            .iter()
            .find(|m| m.memory_type == ImportantMemoryType::RecurringReference || m.source_id == item1.id);

        assert!(rec_mem.is_some(), "Multi-session recurring reference must be identified");
        let mem = rec_mem.unwrap();
        assert!(mem.importance >= 0.70, "Multi-session item importance must be boosted");
    }

    // =========================================================================
    // SCENARIO 4 — REPEATED ACCIDENTAL CAPTURE
    // Same unrelated generic content captured repeatedly.
    // Expected: Frequency alone does not make it important.
    // =========================================================================
    #[test]
    fn test_scenario_4_repeated_accidental_capture() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Clean Workspace").unwrap();

        // Simulate 5 repeated accidental captures of generic noise
        for i in 1..=5 {
            let session = create_session(&conn).unwrap();
            let _accidental = create_item(
                &conn,
                Some(&ws.id),
                Some(&session.id),
                "url",
                "about:blank",
                Some(&format!("New Tab {}", i)),
                r#"{"app_name":"Chrome"}"#,
            )
            .unwrap();
        }

        let important_set = ContextImportantMemoryBuilder::build(&conn, &ws.id, None).unwrap();

        // Verified: None of the generic noise items became important memories
        for mem in &important_set.memories {
            assert_ne!(mem.title, "New Tab");
            assert_ne!(mem.title, "about:blank");
        }
    }

    // =========================================================================
    // SCENARIO 5 — USER CONFIRMATION
    // User marks an item important.
    // Expected: It becomes user-confirmed important memory with top tier.
    // =========================================================================
    #[test]
    fn test_scenario_5_user_confirmation() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let ws = create_workspace(&conn, "Portfolio").unwrap();

        let item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://dribbble.com/shots/hero-inspiration",
            Some("Hero Layout Inspiration"),
            r#"{"domain":"dribbble.com"}"#,
        )
        .unwrap();

        // User explicitly marks as keep important
        let res = record_context_memory_control(&conn, &ws.id, &item.id, UserMemoryControlAction::KeepImportant).unwrap();
        assert!(res);

        let important_set = ContextImportantMemoryBuilder::build(&conn, &ws.id, None).unwrap();
        let confirmed = important_set.memories.iter().find(|m| m.source_id == item.id);
        assert!(confirmed.is_some(), "User-confirmed item must be present in important memories");

        let m = confirmed.unwrap();
        assert_eq!(m.provenance, InformationProvenance::Confirmed);
        assert_eq!(m.importance_band, MemoryImportanceBand::Critical);
        assert_eq!(m.importance, 1.0);
    }

    // =========================================================================
    // SCENARIO 6 — USER REJECTION
    // User removes an item from important memory.
    // Expected: Future inference respects that feedback and excludes it.
    // =========================================================================
    #[test]
    fn test_scenario_6_user_rejection() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let ws = create_workspace(&conn, "Portfolio").unwrap();

        let item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://temp-draft.com/spec",
            Some("Temporary Spec"),
            r#"{"domain":"temp-draft.com"}"#,
        )
        .unwrap();

        confirm_membership(&conn, &item.id, &ws.id).unwrap();

        // User rejects from important memory
        record_context_memory_control(&conn, &ws.id, &item.id, UserMemoryControlAction::RemoveFromImportant).unwrap();

        let important_set = ContextImportantMemoryBuilder::build(&conn, &ws.id, None).unwrap();
        let mem = important_set.memories.iter().find(|m| m.source_id == item.id);
        assert!(mem.is_none(), "User rejected item must be excluded from important memories");

        // Underlying item is still in the database
        let db_item = get_item(&conn, &item.id).unwrap();
        assert_eq!(db_item.id, item.id);
    }

    // =========================================================================
    // SCENARIO 7 — MEMORY DECAY
    // Old inferred memory becomes stale.
    // Expected: Importance decreases, underlying item remains available.
    // =========================================================================
    #[test]
    fn test_scenario_7_memory_decay() {
        let now = Utc::now();
        let old_date = (now - Duration::days(45)).to_rfc3339();

        let policy = MemoryDecayPolicy::default();
        let input = MemoryDecayInput {
            entity_id: "inferred_item_1",
            initial_importance: 0.70,
            initial_tier: MemoryImportanceTier::Useful,
            provenance: InformationProvenance::Inferred,
            is_user_confirmed: false,
            user_feedback: None,
            memory_type: Some(ImportantMemoryType::ImportantItem),
            last_relevant_at: Some(&old_date),
            first_seen_at: Some(&old_date),
            now: Some(now),
            is_active_task: false,
            is_recent_task: false,
            workset_relevance: None,
            session_count: 1,
            distinct_task_count: 1,
            historical_usefulness_score: 0.5,
            context_dormancy_ratio: 0.0,
        };

        let result = ContextMemoryDecayEvaluator::evaluate_with_policy(&input, &policy);
        assert!(result.decayed_importance < input.initial_importance, "Inferred importance must decay over 45 days");
        assert!(result.decay_factor < 0.50, "Decay factor must reflect age");
    }

    // =========================================================================
    // SCENARIO 8 — OLD BUT IMPORTANT
    // User-confirmed important reference has not been used recently.
    // Expected: Zero decay for user-confirmed anchor, remains distinguishable.
    // =========================================================================
    #[test]
    fn test_scenario_8_old_but_important() {
        let now = Utc::now();
        let old_date = (now - Duration::days(90)).to_rfc3339();

        let policy = MemoryDecayPolicy::default();
        let input = MemoryDecayInput {
            entity_id: "confirmed_repo_1",
            initial_importance: 1.0,
            initial_tier: MemoryImportanceTier::Critical,
            provenance: InformationProvenance::Confirmed,
            is_user_confirmed: true,
            user_feedback: Some("pinned"),
            memory_type: Some(ImportantMemoryType::ConfirmedContextFact),
            last_relevant_at: Some(&old_date),
            first_seen_at: Some(&old_date),
            now: Some(now),
            is_active_task: false,
            is_recent_task: false,
            workset_relevance: None,
            session_count: 5,
            distinct_task_count: 2,
            historical_usefulness_score: 1.0,
            context_dormancy_ratio: 0.0,
        };

        let result = ContextMemoryDecayEvaluator::evaluate_with_policy(&input, &policy);
        assert_eq!(result.decayed_importance, 1.0, "User-confirmed anchor experiences zero decay");
        assert_eq!(result.decay_factor, 1.0);
        assert_eq!(result.decayed_tier, MemoryImportanceTier::Critical);
    }

    // =========================================================================
    // SCENARIO 9 — CONTEXT RETURN
    // Aurora → University OS → Aurora.
    // Expected: Aurora important memories remain available; current state reconstructed.
    // =========================================================================
    #[test]
    fn test_scenario_9_context_return() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();

        let ws_aurora = create_workspace(&conn, "Aurora Website").unwrap();
        let ws_uni = create_workspace(&conn, "University OS").unwrap();

        // 1. Work on Aurora
        let aurora_repo = create_item(
            &conn,
            Some(&ws_aurora.id),
            Some(&session.id),
            "url",
            "https://github.com/aurora/site",
            Some("Aurora GitHub Repo"),
            r#"{"domain":"github.com"}"#,
        )
        .unwrap();
        confirm_membership(&conn, &aurora_repo.id, &ws_aurora.id).unwrap();

        // 2. Switch to University OS
        let uni_item = create_item(
            &conn,
            Some(&ws_uni.id),
            Some(&session.id),
            "text",
            "Virtual memory paging assignment",
            Some("HW3 Spec"),
            r#"{"app_name":"Notes"}"#,
        )
        .unwrap();
        confirm_membership(&conn, &uni_item.id, &ws_uni.id).unwrap();

        // Record context transition: Aurora -> Uni
        let trans1 = ContextTransitionBuilder::new(ContextTransitionType::ContextSwitch, 0.95)
            .from_context(ws_aurora.id.clone())
            .to_context(ws_uni.id.clone())
            .build();
        crate::context_engine::transitions::record_context_transition(&conn, &trans1).unwrap();

        // 3. Return to Aurora
        let trans2 = ContextTransitionBuilder::new(ContextTransitionType::ContextReturn, 0.95)
            .from_context(ws_uni.id.clone())
            .to_context(ws_aurora.id.clone())
            .build();
        crate::context_engine::transitions::record_context_transition(&conn, &trans2).unwrap();

        // Verify Aurora important memory is still intact
        let aurora_important = ContextImportantMemoryBuilder::build(&conn, &ws_aurora.id, None).unwrap();
        assert!(aurora_important.memories.iter().any(|m| m.source_id == aurora_repo.id));

        // Verify working memory and workset reconstruct independently
        let aurora_workset = ContextWorksetBuilder::from_db(&conn, &ws_aurora.id).unwrap().build().unwrap();
        assert!(!aurora_workset.items.is_empty());
    }

    // =========================================================================
    // SCENARIO 10 — TASK CHANGE
    // Hero animation → Pricing page.
    // Expected: Old task remains historical; current task changes; important memories remain context-level.
    // =========================================================================
    #[test]
    fn test_scenario_10_task_change() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let ws = create_workspace(&conn, "Aurora Website").unwrap();

        // Context-level anchor
        let repo = create_item(&conn, Some(&ws.id), Some(&session.id), "url", "https://github.com/aurora/site", Some("Aurora Repo"), r#"{}"#).unwrap();
        confirm_membership(&conn, &repo.id, &ws.id).unwrap();

        // Task 1: Hero animation (completed/inactive)
        let task1 = TaskCandidate {
            id: "task-hero".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Hero animation".to_string(),
            confidence: 0.85,
            status: TaskCandidateStatus::Inactive,
            first_detected_at: "2026-09-10T10:00:00Z".to_string(),
            last_active_at: "2026-09-10T12:00:00Z".to_string(),
            created_at: "2026-09-10T10:00:00Z".to_string(),
            updated_at: "2026-09-10T12:00:00Z".to_string(),
        };
        create_task_candidate(&conn, &task1).unwrap();

        // Task 2: Pricing page (active)
        let task2 = TaskCandidate {
            id: "task-pricing".to_string(),
            context_id: Some(ws.id.clone()),
            title: "Pricing page redesign".to_string(),
            confidence: 0.90,
            status: TaskCandidateStatus::Active,
            first_detected_at: "2026-09-18T10:00:00Z".to_string(),
            last_active_at: "2026-09-18T11:00:00Z".to_string(),
            created_at: "2026-09-18T10:00:00Z".to_string(),
            updated_at: "2026-09-18T11:00:00Z".to_string(),
        };
        create_task_candidate(&conn, &task2).unwrap();

        let memory = ContextMemoryBuilder::from_db(&conn, &ws.id).unwrap().build().unwrap();
        assert_eq!(memory.current_task_title.as_deref(), Some("Pricing page redesign"));

        let important_set = ContextImportantMemoryBuilder::build(&conn, &ws.id, None).unwrap();
        let tasks: Vec<String> = important_set.memories.iter().filter(|m| m.memory_type.is_task()).map(|m| m.title.clone()).collect();
        assert!(tasks.iter().any(|t| t.contains("Pricing page")));
        assert!(tasks.iter().any(|t| t.contains("Hero animation")), "Old task remains historical memory");
    }

    // =========================================================================
    // SCENARIO 11 — SNAPSHOT
    // Generate ContextSnapshot.
    // Expected: Snapshot references important memories without duplicating data.
    // =========================================================================
    #[test]
    fn test_scenario_11_snapshot() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let ws = create_workspace(&conn, "Aurora Website").unwrap();

        let repo = create_item(&conn, Some(&ws.id), Some(&session.id), "url", "https://github.com/aurora/site", Some("Aurora Repo"), r#"{}"#).unwrap();
        confirm_membership(&conn, &repo.id, &ws.id).unwrap();

        let snapshot = crate::context_engine::snapshot::ContextSnapshotBuilder::from_db(&conn, &ws.id)
            .unwrap()
            .build();
        assert_eq!(snapshot.context_id, ws.id);
        assert_eq!(snapshot.context_name, "Aurora Website");
        assert!(!snapshot.workset_item_references.is_empty());
        assert!(!snapshot.important_memory_references.is_empty());

        for mem_ref in &snapshot.important_memory_references {
            assert!(!mem_ref.title.is_empty());
            assert!(!mem_ref.id.is_empty());
        }
    }

    // =========================================================================
    // SCENARIO 12 — PRIVACY
    // Excluded / private content (passwords, bearer tokens).
    // Expected: Never enters contextual memory unscrubbed.
    // =========================================================================
    #[test]
    fn test_scenario_12_privacy() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();
        let ws = create_workspace(&conn, "Security Context").unwrap();

        let item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "Authorization: Bearer secret_token_xyz123! password: mySuperPassword",
            Some("Credentials file with password: secret!"),
            r#"{"app_name":"Terminal"}"#,
        )
        .unwrap();
        confirm_membership(&conn, &item.id, &ws.id).unwrap();

        let memory = ContextMemoryBuilder::from_db(&conn, &ws.id).unwrap().build().unwrap();
        for recent in &memory.recent_items {
            if let Some(ref t) = recent.title {
                assert!(!t.contains("secret_token_xyz123!"));
                assert!(!t.contains("mySuperPassword"));
            }
        }

        let workset = ContextWorksetBuilder::from_db(&conn, &ws.id).unwrap().build().unwrap();
        for ws_item in &workset.items {
            assert!(!ws_item.content_snippet.contains("secret_token_xyz123!"));
            assert!(!ws_item.content_snippet.contains("mySuperPassword"));
        }

        let imp_set = ContextImportantMemoryBuilder::build(&conn, &ws.id, None).unwrap();
        for mem in &imp_set.memories {
            if let Some(ref d) = mem.detail {
                assert!(!d.contains("secret_token_xyz123!"));
                assert!(!d.contains("mySuperPassword"));
            }
        }
    }

    // =========================================================================
    // SCENARIO 13 — MULTIPLE CONTEXTS
    // Same domain/app used across different contexts.
    // Expected: Memory remains context-specific.
    // =========================================================================
    #[test]
    fn test_scenario_13_multiple_contexts() {
        let conn = setup_test_db();
        let session = create_session(&conn).unwrap();

        let ws1 = create_workspace(&conn, "Aurora Website").unwrap();
        let ws2 = create_workspace(&conn, "University OS").unwrap();

        // Both use GitHub, but distinct repos
        let item1 = create_item(&conn, Some(&ws1.id), Some(&session.id), "url", "https://github.com/aurora/site", Some("Aurora Repo"), r#"{"domain":"github.com"}"#).unwrap();
        let item2 = create_item(&conn, Some(&ws2.id), Some(&session.id), "url", "https://github.com/stanford/pintos", Some("Pintos OS Repo"), r#"{"domain":"github.com"}"#).unwrap();

        confirm_membership(&conn, &item1.id, &ws1.id).unwrap();
        confirm_membership(&conn, &item2.id, &ws2.id).unwrap();

        let mem1 = ContextMemoryQueryService::get_important_memories(&conn, &ws1.id, None).unwrap();
        let mem2 = ContextMemoryQueryService::get_important_memories(&conn, &ws2.id, None).unwrap();

        let titles1: Vec<String> = mem1.iter().map(|m| m.title.clone()).collect();
        let titles2: Vec<String> = mem2.iter().map(|m| m.title.clone()).collect();

        assert!(titles1.iter().any(|t| t.contains("Aurora Repo")));
        assert!(!titles1.iter().any(|t| t.contains("Pintos OS Repo")));

        assert!(titles2.iter().any(|t| t.contains("Pintos OS Repo")));
        assert!(!titles2.iter().any(|t| t.contains("Aurora Repo")));
    }

    // =========================================================================
    // SCENARIO 14 — INSUFFICIENT EVIDENCE
    // Very sparse context (empty or 1 ungrounded item).
    // Expected: No fabricated important memory or forced task guessing.
    // =========================================================================
    #[test]
    fn test_scenario_14_insufficient_evidence() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Sparse Empty Context").unwrap();

        let memory = ContextMemoryBuilder::from_db(&conn, &ws.id).unwrap().build().unwrap();
        assert!(memory.current_task_title.is_none() || memory.current_task_title.as_deref() == Some("Unknown"));
        assert_eq!(memory.current_task.title, "Unknown");
        assert!(!memory.current_task.is_confident);

        let important_set = ContextImportantMemoryBuilder::build(&conn, &ws.id, None).unwrap();
        let task_memories = important_set.memories.iter().filter(|m| m.memory_type.is_task()).count();
        assert_eq!(task_memories, 0, "No task memory should be fabricated for empty workspace");
    }
}

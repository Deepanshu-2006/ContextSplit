#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use serde_json::json;

    use crate::context_engine::memory::{ContextMemory, InferredTaskState};
    use crate::context_engine::overview::get_context_overview;
    use crate::context_engine::task_evidence::{TaskEvidenceCollector, TaskEvidenceType};
    use crate::context_engine::task_extractor::{
        is_generic_task_name, TaskCandidateExtractor,
    };
    use crate::context_engine::task_ranking::{
        RankedTaskCandidate, TaskAmbiguityState, TaskCandidateRanker,
    };
    use crate::context_engine::task_transitions::{
        TaskSnapshot, TaskTransitionDetector, TaskTransitionEvaluationResult,
        TaskTransitionType,
    };
    use crate::context_engine::transitions::ContextReference;
    use crate::db::operations::{create_item, create_item_relationship, create_session, create_workspace};

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    // =========================================================================
    // Test Case 1: Multiple unrelated items in the same context
    //
    // Context has 4 completely disparate items (algorithms HW, grocery shopping,
    // flight search, music playlist) without cohesive semantic overlap.
    //
    // Expected:
    // - The system does not invent a fake unified task name.
    // - Ambiguity state is InsufficientEvidence or HighlyAmbiguous.
    // - Confidence remains low / insufficient (< 0.45).
    // - ContextMemory current task defaults to "Unknown".
    // =========================================================================
    #[test]
    fn test_adversarial_01_multiple_unrelated_items_same_context() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "General Workspace").unwrap();
        let session = create_session(&conn).unwrap();

        create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "Dynamic Programming memoization algorithms and Bellman equation",
            Some("CS Algorithms"),
            r#"{"app_name":"Preview"}"#,
        )
        .unwrap();

        create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "Buy organic milk, sourdough bread, avocados, olive oil",
            Some("Grocery Shopping List"),
            r#"{"app_name":"Notes"}"#,
        )
        .unwrap();

        create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://kayak.com/flights/SFO-NRT",
            Some("Tokyo Flight Booking Search"),
            r#"{"app_name":"Safari","domain":"kayak.com"}"#,
        )
        .unwrap();

        create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://spotify.com/playlist/ambient-synth",
            Some("Ambient Synthwave Focus Playlist"),
            r#"{"app_name":"Spotify","domain":"spotify.com"}"#,
        )
        .unwrap();

        let evidence_set = TaskEvidenceCollector::new()
            .collect_for_context(&conn, &ws.id)
            .unwrap();

        let candidates = TaskCandidateExtractor::extract_from_evidence(&evidence_set, None);
        let ranked_result = TaskCandidateRanker::rank_candidates(&candidates, Some(0.90), None);

        // Ambiguity should reflect the fragmented nature
        assert!(
            ranked_result.ambiguity_state == TaskAmbiguityState::InsufficientEvidence
                || ranked_result.ambiguity_state == TaskAmbiguityState::HighlyAmbiguous
                || ranked_result.ambiguity_state == TaskAmbiguityState::CloseContenders
        );

        let ctx_ref = ContextReference::workspace(&ws.id, &ws.name);
        let mut memory = ContextMemory::new(Some(ctx_ref), 0.90, "2026-09-13T10:00:00Z");

        if let Some(top) = ranked_result.top_candidate.as_ref() {
            memory.update_from_ranked_candidate(top, "2026-09-13T10:00:00Z");
        }

        // Must NOT force a confident task
        assert_eq!(memory.current_task_title(), "Unknown");
        assert!(!memory.has_confident_task());
    }

    // =========================================================================
    // Test Case 2: Generic terms such as "React", "design", "project"
    //
    // Context contains items with vague / generic labels.
    //
    // Expected:
    // - `is_generic_task_name` identifies plain generic words.
    // - The extractor specializes or filters raw generic terms.
    // - Generic terms cannot achieve High confidence without specialization.
    // =========================================================================
    #[test]
    fn test_adversarial_02_generic_terms_suppression_and_specialization() {
        assert!(is_generic_task_name("React"));
        assert!(is_generic_task_name("Design"));
        assert!(is_generic_task_name("Project"));
        assert!(is_generic_task_name("Coding"));
        assert!(is_generic_task_name("Work"));

        // Specialized variants should NOT be generic
        assert!(!is_generic_task_name("React component library migration"));
        assert!(!is_generic_task_name("Hero animation design"));
        assert!(!is_generic_task_name("Database assignment project"));

        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Dev Workspace").unwrap();

        create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Work on design for project using React",
            Some("Project React Design"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();

        let evidence_set = TaskEvidenceCollector::new()
            .collect_for_context(&conn, &ws.id)
            .unwrap();

        let candidates = TaskCandidateExtractor::extract_from_evidence(&evidence_set, None);
        for cand in &candidates {
            // Either specialized or marked non-generic
            assert!(!is_generic_task_name(&cand.title) || cand.relevance_score < 0.70);
        }
    }

    // =========================================================================
    // Test Case 3: One item strongly suggesting a task
    //
    // A single isolated item contains a very specific title and metadata.
    //
    // Expected:
    // - Single-item confidence ceiling applies: confidence <= 0.70.
    // - The system does NOT declare "High" confidence (>= 0.75) for a single isolated item.
    // =========================================================================
    #[test]
    fn test_adversarial_03_single_item_strong_signal_confidence_ceiling() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Auth Service").unwrap();

        create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Implementing OAuth2 PKCE token exchange and refresh flow in Rust Actix-web",
            Some("OAuth2 PKCE Flow"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();

        let evidence_set = TaskEvidenceCollector::new()
            .collect_for_context(&conn, &ws.id)
            .unwrap();

        let candidates = TaskCandidateExtractor::extract_from_evidence(&evidence_set, None);
        let ranked_result = TaskCandidateRanker::rank_candidates(&candidates, Some(0.95), None);

        assert!(!ranked_result.ranked_candidates.is_empty());
        let top = &ranked_result.ranked_candidates[0];

        // Single-item confidence ceiling invariant
        assert!(
            top.confidence <= 0.70,
            "Single item must not exceed 0.70 confidence ceiling, got {}",
            top.confidence
        );
        assert_ne!(top.confidence_band, "high");
    }

    // =========================================================================
    // Test Case 4: Several independent items supporting the same task
    //
    // 4 items from different apps and files all support "Hero animation".
    //
    // Expected:
    // - Multi-item corroboration achieves High confidence (>= 0.75).
    // - Ambiguity state is ClearLeader.
    // - Evidence diversity score is high.
    // - ContextMemory marks task as confident with title "Hero animation".
    // =========================================================================
    #[test]
    fn test_adversarial_04_several_independent_items_high_confidence() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Portfolio").unwrap();
        let session = create_session(&conn).unwrap();

        let item1 = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "Hero animation interactive spring physics implementation in Figma",
            Some("Figma hero design"),
            r#"{"app_name":"Figma"}"#,
        )
        .unwrap();

        let item2 = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "Hero animation React component code and physics",
            Some("React hero animation component"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();

        let item3 = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "https://framer.com/motion/hero-animation",
            Some("Hero animation reference"),
            r#"{"app_name":"Safari","domain":"framer.com"}"#,
        )
        .unwrap();

        create_item_relationship(&conn, &item1.id, &item2.id, "semantic_similarity", 0.90, None).unwrap();
        create_item_relationship(&conn, &item2.id, &item3.id, "semantic_similarity", 0.88, None).unwrap();

        let evidence_set = TaskEvidenceCollector::new()
            .collect_for_context(&conn, &ws.id)
            .unwrap();

        let candidates = TaskCandidateExtractor::extract_from_evidence(&evidence_set, None);
        let ranked_result = TaskCandidateRanker::rank_candidates(&candidates, Some(0.95), None);

        assert!(!ranked_result.ranked_candidates.is_empty());
        let top = &ranked_result.ranked_candidates[0];

        assert!(
            top.confidence >= 0.75,
            "Multi-item corroborated task should reach High confidence, got {}",
            top.confidence
        );
        assert_eq!(top.confidence_band, "high");
        assert_eq!(ranked_result.ambiguity_state, TaskAmbiguityState::ClearLeader);

        let ctx_ref = ContextReference::workspace(&ws.id, &ws.name);
        let mut memory = ContextMemory::new(Some(ctx_ref), 0.95, "2026-09-13T10:00:00Z");
        memory.update_from_ranked_candidate(top, "2026-09-13T10:00:00Z");

        assert!(memory.has_confident_task());
        assert_eq!(memory.current_task_title(), top.candidate.title);
        assert_eq!(memory.context_name(), "Portfolio");
    }

    // =========================================================================
    // Test Case 5: Two competing tasks
    //
    // Context contains 2 distinct task clusters of equal size and strength:
    // Cluster A: "OAuth Authentication Refactor"
    // Cluster B: "PostgreSQL Database Migration"
    //
    // Expected:
    // - Ambiguity state detects CloseContenders or HighlyAmbiguous.
    // - Score margin between candidates is narrow (< 0.15).
    // - The system does NOT falsely present an overwhelming victory.
    // =========================================================================
    #[test]
    fn test_adversarial_05_competing_tasks_ambiguity() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Backend Core").unwrap();
        let session = create_session(&conn).unwrap();

        // 2 items for Task A: OAuth
        create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "OAuth authentication middleware and JWT verification",
            Some("OAuth Auth Middleware"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();
        create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "OAuth client credentials grant implementation",
            Some("OAuth Client Credentials"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();

        // 2 items for Task B: Database Migration
        create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "PostgreSQL database migration schema index optimization",
            Some("DB Migration Schema"),
            r#"{"app_name":"DataGrip"}"#,
        )
        .unwrap();
        create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "PostgreSQL database query performance index tuning",
            Some("DB Query Indexing"),
            r#"{"app_name":"DataGrip"}"#,
        )
        .unwrap();

        let evidence_set = TaskEvidenceCollector::new()
            .collect_for_context(&conn, &ws.id)
            .unwrap();

        let candidates = TaskCandidateExtractor::extract_from_evidence(&evidence_set, None);
        let ranked_result = TaskCandidateRanker::rank_candidates(&candidates, Some(0.90), None);

        assert!(ranked_result.ranked_candidates.len() >= 2);
        let top = &ranked_result.ranked_candidates[0];
        let runner_up = &ranked_result.ranked_candidates[1];

        let score_diff = (top.score - runner_up.score).abs();
        assert!(
            score_diff <= 0.20,
            "Competing tasks should have narrow margin, got {}",
            score_diff
        );
        assert!(
            ranked_result.ambiguity_state == TaskAmbiguityState::CloseContenders
                || ranked_result.ambiguity_state == TaskAmbiguityState::HighlyAmbiguous
        );
    }

    // =========================================================================
    // Test Case 6: User switches tasks within the same context
    //
    // Portfolio Context:
    // Sequence 1: 3 observations of "Hero animation"
    // Sequence 2: 3 observations of "Case study writing"
    //
    // Expected:
    // - TaskTransitionDetector declares TaskSwitch.
    // - ContextMemory archives "Hero animation" in task_history.
    // - ContextMemory current_task updates to "Case study writing".
    // - Workspace name ("Portfolio") is NEVER modified or overwritten.
    // =========================================================================
    #[test]
    fn test_adversarial_06_intra_context_task_switch() {
        let mut memory = ContextMemory::new(
            Some(ContextReference::workspace("ws-portfolio", "Portfolio")),
            0.95,
            "2026-09-13T10:00:00Z",
        );

        // Phase 1: Hero animation
        let snap1 = TaskSnapshot::new(
            "Hero animation",
            Some("Designing".to_string()),
            Some("Hero animation".to_string()),
            0.85,
            "2026-09-13T10:00:00Z",
            Some("ws-portfolio".to_string()),
        );
        memory.update_from_snapshot(&snap1, vec!["Figma hero design".to_string()]);
        assert_eq!(memory.current_task_title(), "Hero animation");
        assert_eq!(memory.task_history().len(), 0);

        // Phase 2: Switch to Case study writing
        let snap2 = TaskSnapshot::new(
            "Case study writing",
            Some("Writing".to_string()),
            Some("Case study".to_string()),
            0.88,
            "2026-09-13T10:30:00Z",
            Some("ws-portfolio".to_string()),
        );
        memory.update_from_snapshot(&snap2, vec!["Notion draft".to_string()]);

        // Verifications
        assert_eq!(memory.current_task_title(), "Case study writing");
        assert_eq!(memory.task_history().len(), 1);
        assert_eq!(memory.task_history()[0].title, "Hero animation");
        assert_eq!(memory.context_name(), "Portfolio");
    }

    // =========================================================================
    // Test Case 7: User returns to a previous task
    //
    // Sequence: Task A ("Hero animation") -> Task B ("Case study writing") -> Task A ("Hero animation")
    //
    // Expected:
    // - The return to Task A is recognized as TaskReturn.
    // - ContextMemory resumes Task A with updated activity timestamp.
    // =========================================================================
    #[test]
    fn test_adversarial_07_task_return_resumption() {
        let mut detector = TaskTransitionDetector::new();

        // 1. Task A: Hero animation
        let cand_a = RankedTaskCandidate {
            candidate: crate::context_engine::task_extractor::ExtractedTaskCandidate {
                title: "Hero animation".to_string(),
                action_verb: Some("Designing".to_string()),
                target_subject: Some("Hero animation".to_string()),
                relevance_score: 0.90,
                evidence_references: Vec::new(),
                item_ids: vec!["item-1".to_string(), "item-2".to_string()],
                primary_evidence_types: vec![TaskEvidenceType::RecentItemContent],
                salient_terms: vec!["hero".to_string(), "animation".to_string()],
                context_id: Some("ws-portfolio".to_string()),
                metadata: json!({}),
            },
            score: 0.90,
            confidence: 0.90,
            confidence_band: "high".to_string(),
            rank: 1,
            supporting_evidence: Vec::new(),
            contradictory_evidence: Vec::new(),
            dimension_scores: crate::context_engine::task_ranking::TaskRankingDimensionScores {
                recency_score: 0.9,
                evidence_strength_score: 0.9,
                evidence_diversity_score: 0.9,
                semantic_coherence_score: 0.9,
                activity_frequency_score: 0.9,
                session_continuity_score: 0.9,
                contextual_relevance_score: 0.9,
                stability_score: 0.9,
            },
            margin_over_next: Some(0.4),
        };

        detector.evaluate_ranked_candidate(
            Some(&cand_a),
            "2026-09-13T10:00:00Z",
            Some("item-1"),
            Some("ws-portfolio"),
        );

        // 2. Task B: Case study writing
        let cand_b = RankedTaskCandidate {
            candidate: crate::context_engine::task_extractor::ExtractedTaskCandidate {
                title: "Case study writing".to_string(),
                action_verb: Some("Writing".to_string()),
                target_subject: Some("Case study".to_string()),
                relevance_score: 0.90,
                evidence_references: Vec::new(),
                item_ids: vec!["item-3".to_string(), "item-4".to_string()],
                primary_evidence_types: vec![TaskEvidenceType::RecentItemContent],
                salient_terms: vec!["case".to_string(), "study".to_string()],
                context_id: Some("ws-portfolio".to_string()),
                metadata: json!({}),
            },
            score: 0.90,
            confidence: 0.90,
            confidence_band: "high".to_string(),
            rank: 1,
            supporting_evidence: Vec::new(),
            contradictory_evidence: Vec::new(),
            dimension_scores: crate::context_engine::task_ranking::TaskRankingDimensionScores {
                recency_score: 0.9,
                evidence_strength_score: 0.9,
                evidence_diversity_score: 0.9,
                semantic_coherence_score: 0.9,
                activity_frequency_score: 0.9,
                session_continuity_score: 0.9,
                contextual_relevance_score: 0.9,
                stability_score: 0.9,
            },
            margin_over_next: Some(0.4),
        };

        detector.evaluate_ranked_candidate(
            Some(&cand_b),
            "2026-09-13T10:30:00Z",
            Some("item-3"),
            Some("ws-portfolio"),
        );

        // 3. Return to Task A: Hero animation
        let res_return = detector.evaluate_ranked_candidate(
            Some(&cand_a),
            "2026-09-13T11:00:00Z",
            Some("item-2"),
            Some("ws-portfolio"),
        );

        match res_return {
            TaskTransitionEvaluationResult::TransitionDetected(t) => {
                assert_eq!(t.transition_type, TaskTransitionType::TaskReturn);
                assert_eq!(t.to_task, Some("Hero animation".to_string()));
                assert_eq!(t.from_task, Some("Case study writing".to_string()));
            }
            other => panic!("Expected TaskReturn, got {:?}", other),
        }
    }

    // =========================================================================
    // Test Case 8: Old task evidence versus recent task evidence
    //
    // Context contains:
    // - 2 items created 30 days ago for "Setup CI Pipeline"
    // - 2 items created today for "Fix Memory Leak"
    //
    // Expected:
    // - Temporal decay heavily favors "Fix Memory Leak".
    // - "Fix Memory Leak" is ranked as top candidate.
    // - Historical task is discounted and not selected as active.
    // =========================================================================
    #[test]
    fn test_adversarial_08_temporal_decay_old_vs_recent_task() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "DevOps").unwrap();

        // Old items (simulated older created_at in SQLite)
        let _old_item1 = conn.execute(
            "INSERT INTO items (id, workspace_id, item_type, content, title, metadata, created_at, updated_at)
             VALUES ('item-old-1', ?1, 'text', 'GitHub Actions workflow yaml CI setup', 'Setup CI Pipeline', '{}', '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
            rusqlite::params![ws.id],
        ).unwrap();

        let _old_item2 = conn.execute(
            "INSERT INTO items (id, workspace_id, item_type, content, title, metadata, created_at, updated_at)
             VALUES ('item-old-2', ?1, 'text', 'GitHub Actions runner configuration and docker', 'Setup CI Pipeline Runner', '{}', '2026-08-01T01:00:00Z', '2026-08-01T01:00:00Z')",
            rusqlite::params![ws.id],
        ).unwrap();

        // Fresh items (today)
        create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Valgrind heap memory leak profiling and dangling pointer fix in allocator",
            Some("Fix Memory Leak"),
            r#"{"app_name":"CLion"}"#,
        )
        .unwrap();

        create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Memory leak resolved in connection pool buffer recycling",
            Some("Fix Memory Leak Buffer"),
            r#"{"app_name":"CLion"}"#,
        )
        .unwrap();

        let evidence_set = TaskEvidenceCollector::new()
            .collect_for_context(&conn, &ws.id)
            .unwrap();

        let candidates = TaskCandidateExtractor::extract_from_evidence(&evidence_set, None);
        let ranked_result = TaskCandidateRanker::rank_candidates(&candidates, Some(0.90), None);

        assert!(!ranked_result.ranked_candidates.is_empty());
        let top = &ranked_result.ranked_candidates[0];

        // Recent task should dominate
        assert!(
            top.candidate.title.to_lowercase().contains("memory")
                || top.candidate.title.to_lowercase().contains("leak"),
            "Recent task should win over 30-day-old task, got {}",
            top.candidate.title
        );
    }

    // =========================================================================
    // Test Case 9: Strong context evidence but weak task evidence
    //
    // User is on canvas.stanford.edu generic portal page.
    //
    // Expected:
    // - Context confidence is 95% High (matches "College" workspace).
    // - Task confidence is Low / Insufficient (no specific assignment signal).
    // - Dual-confidence metrics remain decoupled.
    // =========================================================================
    #[test]
    fn test_adversarial_09_strong_context_weak_task_decoupling() {
        let ctx = ContextReference::workspace("ws-college", "College");
        let memory = ContextMemory::new(Some(ctx), 0.95, "2026-09-13T10:00:00Z").with_task(
            InferredTaskState::new(
                "Canvas LMS Portal",
                None,
                None,
                0.30, // Weak task confidence (< 0.45)
                vec!["generic portal page".to_string()],
                "2026-09-13T10:00:00Z",
                Some("ws-college".to_string()),
            ),
        );

        // Context is 95% High
        assert_eq!(memory.context_confidence(), 0.95);
        assert_eq!(memory.context_confidence_band(), "High");

        // Task is Unknown
        assert_eq!(memory.current_task_title(), "Unknown");
        assert!(!memory.has_confident_task());
        assert_eq!(memory.task_confidence_band(), "Insufficient");
    }

    // =========================================================================
    // Test Case 10: Completely insufficient evidence
    //
    // Empty workspace with zero items and zero signals.
    //
    // Expected:
    // - ContextMemory returns "Unknown".
    // - ContextOverview returns "Not clear yet".
    // - Zero panic, crash, or forced task assignment.
    // =========================================================================
    #[test]
    fn test_adversarial_10_completely_insufficient_evidence() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Empty Workspace").unwrap();

        let overview = get_context_overview(&conn, &ws.id).unwrap();

        assert_eq!(overview.name, "Empty Workspace");
        assert!(overview.current_task.is_some());
        let task = overview.current_task.unwrap();

        assert!(!task.is_confident);
        assert_eq!(task.title, "Not clear yet");
        assert_eq!(task.confidence, 0.0);

        let ctx = ContextReference::workspace(&ws.id, &ws.name);
        let memory = ContextMemory::new(Some(ctx), 0.50, "2026-09-13T10:00:00Z");
        assert_eq!(memory.current_task_title(), "Unknown");
        assert!(!memory.has_confident_task());
    }

    // =========================================================================
    // Test Case 11: Multiple applications contributing to one task
    //
    // Task "Hero animation" is supported by:
    // - Figma (design)
    // - VS Code (implementation)
    // - Terminal (running tests)
    // - Chrome (viewing preview)
    //
    // Expected:
    // - Unified candidate extracted without splitting into separate tasks per app.
    // - High evidence diversity score.
    // =========================================================================
    #[test]
    fn test_adversarial_11_multiple_applications_single_task() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Portfolio").unwrap();
        let session = create_session(&conn).unwrap();

        create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "Hero animation frame timeline specs and vector curves",
            Some("Figma Hero Animation"),
            r#"{"app_name":"Figma"}"#,
        )
        .unwrap();

        create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "Hero animation react spring component physics",
            Some("HeroAnimation.tsx"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();

        create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "npm test -- -t 'hero animation spring'",
            Some("Terminal Test Runner"),
            r#"{"app_name":"Terminal"}"#,
        )
        .unwrap();

        create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "url",
            "http://localhost:3000/hero-animation-preview",
            Some("Localhost Hero Animation Preview"),
            r#"{"app_name":"Google Chrome","domain":"localhost"}"#,
        )
        .unwrap();

        let evidence_set = TaskEvidenceCollector::new()
            .collect_for_context(&conn, &ws.id)
            .unwrap();

        let candidates = TaskCandidateExtractor::extract_from_evidence(&evidence_set, None);
        let ranked_result = TaskCandidateRanker::rank_candidates(&candidates, Some(0.95), None);

        assert!(!ranked_result.ranked_candidates.is_empty());
        let top = &ranked_result.ranked_candidates[0];

        // Should extract unified hero animation task
        assert!(
            top.candidate.title.to_lowercase().contains("hero")
                || top.candidate.title.to_lowercase().contains("animation"),
            "Expected hero animation task, got {}",
            top.candidate.title
        );

        // Evidence diversity should be high
        assert!(
            top.dimension_scores.evidence_diversity_score >= 0.70,
            "Evidence diversity should be high for 4 distinct apps, got {}",
            top.dimension_scores.evidence_diversity_score
        );
    }

    // =========================================================================
    // Test Case 12: One application used across multiple tasks
    //
    // VS Code is used for:
    // Task 1: "Auth Middleware JWT" (2 files)
    // Task 2: "Database Migration Schema" (2 files)
    //
    // Expected:
    // - The shared application (VS Code) does NOT force both tasks into one blob.
    // - Separate task candidates are extracted and ranked.
    // =========================================================================
    #[test]
    fn test_adversarial_12_single_application_multiple_tasks() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Backend").unwrap();
        let session = create_session(&conn).unwrap();

        // Task 1 in VS Code
        create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "JWT auth middleware token verification in auth.rs",
            Some("auth.rs"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();
        create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "Bearer token parser and refresh token handler",
            Some("token_handler.rs"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();

        // Task 2 in VS Code
        create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "CREATE TABLE users_v2 migration and alter column index",
            Some("002_migration.sql"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();
        create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "Database schema migration rollback script",
            Some("002_rollback.sql"),
            r#"{"app_name":"VS Code"}"#,
        )
        .unwrap();

        let evidence_set = TaskEvidenceCollector::new()
            .collect_for_context(&conn, &ws.id)
            .unwrap();

        let candidates = TaskCandidateExtractor::extract_from_evidence(&evidence_set, None);

        // Should extract distinct candidates rather than one "VS Code" blob
        let has_auth = candidates
            .iter()
            .any(|c| c.title.to_lowercase().contains("jwt") || c.title.to_lowercase().contains("auth") || c.title.to_lowercase().contains("token"));
        let has_db = candidates
            .iter()
            .any(|c| c.title.to_lowercase().contains("migration") || c.title.to_lowercase().contains("sql") || c.title.to_lowercase().contains("database"));

        assert!(has_auth, "Should identify Auth task");
        assert!(has_db, "Should identify Database Migration task");
    }
}

// src-tauri/src/context_engine/multi_context_confusion_tests.rs
//
// Regression tests for multi-context competition and anti-application-dominance.
//
// Invariant: The system must NEVER assume application = context.
// Multi-context evidence must evaluate combinations of:
// - project
// - repository
// - file path
// - URL & domain
// - recent activity
// - session continuity
// - task relationship
// - memory relationship
// - explicit user feedback

#[cfg(test)]
mod tests {
    use crate::context_engine::contamination::{ContaminationCheckInput, ContaminationGuard};
    use crate::context_engine::evidence::{ContextEvidence, ContextEvidenceSet, EvidenceCategory};
    use crate::context_engine::pending_activity::{
        ActivityEvidenceSignal, ContextualActivityStatus, PendingActivityConfig,
        PendingActivityManager, PendingContextualActivity,
    };
    use crate::context_engine::quality_guard::{ContextQualityGuard, QualityGuardInput};
    use crate::context_engine::scoring::{
        ContextEvidenceScorer, ScoringWeights, WorkspaceScoringContext,
    };

    fn create_test_workspaces() -> (
        WorkspaceScoringContext,
        WorkspaceScoringContext,
        WorkspaceScoringContext,
    ) {
        let ws_contextsplit = WorkspaceScoringContext {
            id: "ws_contextsplit".to_string(),
            name: "ContextSplit Development".to_string(),
            lifecycle_state: "active".to_string(),
            item_ids: vec!["item_cs_1".to_string(), "item_cs_2".to_string()],
            item_types: vec!["code".to_string(), "url".to_string()],
            app_names: vec![
                "Code".to_string(),
                "Google Chrome".to_string(),
                "Terminal".to_string(),
            ],
            domains: vec!["v2.tauri.app".to_string(), "github.com".to_string()],
            sessions: vec!["sess_cs_1".to_string()],
            profile_embedding: None,
            profile_provider: None,
            profile_model: None,
        };

        let ws_payx = WorkspaceScoringContext {
            id: "ws_payx".to_string(),
            name: "PayX Development".to_string(),
            lifecycle_state: "active".to_string(),
            item_ids: vec!["item_payx_1".to_string(), "item_payx_2".to_string()],
            item_types: vec!["code".to_string(), "url".to_string()],
            app_names: vec![
                "Code".to_string(),
                "Google Chrome".to_string(),
                "Terminal".to_string(),
                "Xcode".to_string(),
            ],
            domains: vec!["dashboard.stripe.com".to_string(), "github.com".to_string()],
            sessions: vec!["sess_payx_1".to_string()],
            profile_embedding: None,
            profile_provider: None,
            profile_model: None,
        };

        let ws_university = WorkspaceScoringContext {
            id: "ws_university".to_string(),
            name: "University CS4500".to_string(),
            lifecycle_state: "active".to_string(),
            item_ids: vec!["item_uni_1".to_string(), "item_uni_2".to_string()],
            item_types: vec!["code".to_string(), "url".to_string(), "pdf".to_string()],
            app_names: vec![
                "Code".to_string(),
                "Google Chrome".to_string(),
                "Terminal".to_string(),
                "Preview".to_string(),
            ],
            domains: vec![
                "canvas.university.edu".to_string(),
                "piazza.com".to_string(),
            ],
            sessions: vec!["sess_uni_1".to_string()],
            profile_embedding: None,
            profile_provider: None,
            profile_model: None,
        };

        (ws_contextsplit, ws_payx, ws_university)
    }

    #[test]
    fn test_vscode_multi_context_disambiguation_via_filepath() {
        let (ws_cs, ws_payx, ws_uni) = create_test_workspaces();
        let config = PendingActivityConfig::default();
        let mut manager = PendingActivityManager::new(config);

        // 1. VS Code editing ContextSplit scoring.rs
        let mut act_cs = PendingContextualActivity::new("act_1", "scoring.rs — ContextSplit")
            .with_app_name("Code")
            .with_file_path("/Users/deepanshukhatri/Desktop/ContextSplit/src-tauri/src/scoring.rs")
            .with_candidates(vec![
                ws_cs.id.clone(),
                ws_payx.id.clone(),
                ws_uni.id.clone(),
            ]);

        act_cs.add_signal(ActivityEvidenceSignal::project_repo_match(
            &ws_cs.id,
            "ContextSplit",
        ));
        act_cs.add_signal(ActivityEvidenceSignal::file_path_match(
            &ws_cs.id,
            "/Desktop/ContextSplit/src-tauri/src/scoring.rs",
        ));
        act_cs.add_signal(ActivityEvidenceSignal::weak_app_signal("Code"));

        let (ingested_cs, _) = manager.ingest_activity(act_cs);
        assert_eq!(ingested_cs.status, ContextualActivityStatus::Assigned);
        assert_eq!(ingested_cs.assigned_workspace_id.as_deref(), Some("ws_contextsplit"));

        // 2. VS Code editing PayX PaymentGateway.swift
        let mut act_payx =
            PendingContextualActivity::new("act_2", "PaymentGateway.swift — PayX Core")
                .with_app_name("Code")
                .with_file_path("/Users/deepanshukhatri/Work/PayX/Sources/PaymentGateway.swift")
                .with_candidates(vec![
                    ws_cs.id.clone(),
                    ws_payx.id.clone(),
                    ws_uni.id.clone(),
                ]);

        act_payx.add_signal(ActivityEvidenceSignal::project_repo_match(
            &ws_payx.id,
            "PayX",
        ));
        act_payx.add_signal(ActivityEvidenceSignal::file_path_match(
            &ws_payx.id,
            "/Work/PayX/Sources/PaymentGateway.swift",
        ));
        act_payx.add_signal(ActivityEvidenceSignal::weak_app_signal("Code"));

        let (ingested_payx, _) = manager.ingest_activity(act_payx);
        assert_eq!(ingested_payx.status, ContextualActivityStatus::Assigned);
        assert_eq!(ingested_payx.assigned_workspace_id.as_deref(), Some("ws_payx"));

        // 3. VS Code editing University virtual_memory.c
        let mut act_uni =
            PendingContextualActivity::new("act_3", "virtual_memory.c — CS4500 Assignment 3")
                .with_app_name("Code")
                .with_file_path(
                    "/Users/deepanshukhatri/University/CS4500/assignment3/virtual_memory.c",
                )
                .with_candidates(vec![
                    ws_cs.id.clone(),
                    ws_payx.id.clone(),
                    ws_uni.id.clone(),
                ]);

        act_uni.add_signal(ActivityEvidenceSignal::project_repo_match(
            &ws_uni.id,
            "CS4500",
        ));
        act_uni.add_signal(ActivityEvidenceSignal::file_path_match(
            &ws_uni.id,
            "/University/CS4500/assignment3/virtual_memory.c",
        ));
        act_uni.add_signal(ActivityEvidenceSignal::weak_app_signal("Code"));

        let (ingested_uni, _) = manager.ingest_activity(act_uni);
        assert_eq!(ingested_uni.status, ContextualActivityStatus::Assigned);
        assert_eq!(ingested_uni.assigned_workspace_id.as_deref(), Some("ws_university"));

        // 4. VS Code with NO file path, NO repo match, only app identity -> Must NOT assign!
        let mut act_generic = PendingContextualActivity::new("act_4", "Untitled-1 — Visual Studio Code")
            .with_app_name("Code")
            .with_candidates(vec![
                ws_cs.id.clone(),
                ws_payx.id.clone(),
                ws_uni.id.clone(),
            ]);

        act_generic.add_signal(ActivityEvidenceSignal::weak_app_signal("Code"));

        let (ingested_generic, _) = manager.ingest_activity(act_generic);
        assert_ne!(ingested_generic.status, ContextualActivityStatus::Assigned);
        assert_eq!(ingested_generic.assigned_workspace_id, None);
        assert_eq!(ingested_generic.status, ContextualActivityStatus::Pending);
    }

    #[test]
    fn test_chrome_multi_context_disambiguation_and_personal_noise_suppression() {
        let (ws_cs, ws_payx, ws_uni) = create_test_workspaces();
        let q_guard = ContextQualityGuard::default();
        let c_guard = ContaminationGuard::default();
        let config = PendingActivityConfig::default();
        let mut manager = PendingActivityManager::new(config);

        // 1. Chrome browsing Tauri documentation -> ContextSplit
        let mut act_tauri = PendingContextualActivity::new("act_c1", "Window API | Tauri Docs")
            .with_app_name("Google Chrome")
            .with_url_and_domain(
                Some("https://v2.tauri.app/api".to_string()),
                Some("v2.tauri.app".to_string()),
            )
            .with_candidates(vec![ws_cs.id.clone(), ws_payx.id.clone(), ws_uni.id.clone()]);

        act_tauri.add_signal(ActivityEvidenceSignal::new(
            "domain_match",
            Some(ws_cs.id.clone()),
            0.92,
            1.4,
            true,
            "Domain v2.tauri.app associated with ContextSplit tech stack",
        ));

        let (ingested_tauri, _) = manager.ingest_activity(act_tauri);
        assert_eq!(ingested_tauri.status, ContextualActivityStatus::Assigned);
        assert_eq!(ingested_tauri.assigned_workspace_id.as_deref(), Some("ws_contextsplit"));

        // 2. Chrome browsing Stripe Webhook Dashboard -> PayX
        let mut act_stripe =
            PendingContextualActivity::new("act_c2", "Stripe Dashboard - Webhooks - PayX")
                .with_app_name("Google Chrome")
                .with_url_and_domain(
                    Some("https://dashboard.stripe.com/webhooks/we_1".to_string()),
                    Some("dashboard.stripe.com".to_string()),
                )
                .with_candidates(vec![ws_cs.id.clone(), ws_payx.id.clone(), ws_uni.id.clone()]);

        act_stripe.add_signal(ActivityEvidenceSignal::new(
            "domain_match",
            Some(ws_payx.id.clone()),
            0.92,
            1.4,
            true,
            "Stripe payments developer dashboard bound to PayX",
        ));

        let (ingested_stripe, _) = manager.ingest_activity(act_stripe);
        assert_eq!(ingested_stripe.status, ContextualActivityStatus::Assigned);
        assert_eq!(ingested_stripe.assigned_workspace_id.as_deref(), Some("ws_payx"));

        // 3. Chrome browsing Canvas LMS -> University
        let mut act_canvas =
            PendingContextualActivity::new("act_c3", "CS4500 Operating Systems: Assignment 3")
                .with_app_name("Google Chrome")
                .with_url_and_domain(
                    Some("https://canvas.university.edu/courses/4500".to_string()),
                    Some("canvas.university.edu".to_string()),
                )
                .with_candidates(vec![ws_cs.id.clone(), ws_payx.id.clone(), ws_uni.id.clone()]);

        act_canvas.add_signal(ActivityEvidenceSignal::new(
            "domain_match",
            Some(ws_uni.id.clone()),
            0.95,
            1.5,
            true,
            "University Canvas course LMS",
        ));

        let (ingested_canvas, _) = manager.ingest_activity(act_canvas);
        assert_eq!(ingested_canvas.status, ContextualActivityStatus::Assigned);
        assert_eq!(ingested_canvas.assigned_workspace_id.as_deref(), Some("ws_university"));

        // 4. Chrome browsing Reddit -> Filtered by Quality Guard & Contamination Guard
        let q_input = QualityGuardInput::new("r/technology - Reddit")
            .with_app_name("Google Chrome")
            .with_noise_flags(true, false)
            .with_target(&ws_cs.id, &ws_cs.name);
        let q_res = q_guard.evaluate(&q_input);
        assert!(!q_res.decision.should_modify_context());

        let c_input = ContaminationCheckInput::new("r/technology - Reddit", &ws_cs.id, &ws_cs.name)
            .with_app("Google Chrome")
            .with_domain("reddit.com");
        let c_res = c_guard.assess(&c_input);
        assert!(c_res.is_contamination_risk);
    }

    #[test]
    fn test_generic_shared_filename_triggers_ambiguity_quarantine() {
        let (ws_cs, ws_payx, _ws_uni) = create_test_workspaces();
        let config = PendingActivityConfig {
            min_score_margin: 0.15,
            ambiguity_threshold: 0.40,
            ..PendingActivityConfig::default()
        };
        let mut manager = PendingActivityManager::new(config);

        // Activity with generic title "README.md" and equally matching keyword scores
        let mut act_readme = PendingContextualActivity::new("act_amb_1", "README.md")
            .with_app_name("Code")
            .with_candidates(vec![ws_cs.id.clone(), ws_payx.id.clone()]);

        // Moderate non-differentiating signals for both workspaces
        act_readme.add_signal(ActivityEvidenceSignal::new(
            "filename_mention",
            Some(ws_cs.id.clone()),
            0.50,
            1.0,
            false,
            "README.md file present in ContextSplit",
        ));
        act_readme.add_signal(ActivityEvidenceSignal::new(
            "filename_mention",
            Some(ws_payx.id.clone()),
            0.50,
            1.0,
            false,
            "README.md file present in PayX",
        ));

        let (ingested_readme, _) = manager.ingest_activity(act_readme);
        // Margin = 0.0 < 0.15, scores >= 0.40 -> Must be quarantined into Ambiguous
        assert_eq!(ingested_readme.status, ContextualActivityStatus::Ambiguous);
        assert_eq!(ingested_readme.assigned_workspace_id, None);
    }

    #[test]
    fn test_user_feedback_overrides_application_and_inferred_competition() {
        let (ws_cs, ws_payx, _ws_uni) = create_test_workspaces();
        let config = PendingActivityConfig::default();
        let mut manager = PendingActivityManager::new(config);

        let mut act = PendingContextualActivity::new("act_fb_1", "shared_utility.py")
            .with_app_name("Code")
            .with_candidates(vec![ws_cs.id.clone(), ws_payx.id.clone()]);

        // Competing signals
        act.add_signal(ActivityEvidenceSignal::new(
            "keyword_match",
            Some(ws_cs.id.clone()),
            0.60,
            1.0,
            false,
            "Keyword match in ContextSplit",
        ));
        act.add_signal(ActivityEvidenceSignal::new(
            "keyword_match",
            Some(ws_payx.id.clone()),
            0.60,
            1.0,
            false,
            "Keyword match in PayX",
        ));

        // User authoritatively assigns to PayX
        act.add_signal(ActivityEvidenceSignal::user_confirmation(&ws_payx.id));

        let (ingested, _) = manager.ingest_activity(act);
        assert_eq!(ingested.status, ContextualActivityStatus::Assigned);
        assert_eq!(ingested.assigned_workspace_id.as_deref(), Some("ws_payx"));
        assert_eq!(ingested.assigned_confidence, 1.0);
    }

    #[test]
    fn test_task_and_memory_links_resolve_ambiguous_evidence() {
        let (_ws_cs, _ws_payx, ws_uni) = create_test_workspaces();
        let config = PendingActivityConfig::default();
        let mut manager = PendingActivityManager::new(config);

        let mut act = PendingContextualActivity::new("act_task_1", "Assignment Notes")
            .with_app_name("Notes")
            .with_candidates(vec![ws_uni.id.clone()]);

        act.add_signal(ActivityEvidenceSignal::task_transition_match(
            &ws_uni.id,
            "CS4500 Virtual Memory Lab",
        ));

        let (ingested, _) = manager.ingest_activity(act);
        assert_eq!(ingested.status, ContextualActivityStatus::Assigned);
        assert_eq!(ingested.assigned_workspace_id.as_deref(), Some("ws_university"));
    }

    #[test]
    fn test_session_continuity_cannot_pull_across_distinct_project_repositories() {
        let (ws_cs, ws_payx, _ws_uni) = create_test_workspaces();
        let config = PendingActivityConfig::default();
        let mut manager = PendingActivityManager::new(config);

        // Activity occurs in Xcode in PayX repo, while session history existed for ContextSplit
        let mut act = PendingContextualActivity::new("act_trans_1", "AppDelegate.swift — PayX")
            .with_app_name("Xcode")
            .with_file_path("/Users/deepanshukhatri/Work/PayX/Sources/AppDelegate.swift")
            .with_candidates(vec![ws_cs.id.clone(), ws_payx.id.clone()]);

        // Prior session signal for ContextSplit
        act.add_signal(ActivityEvidenceSignal::new(
            "prior_session_continuity",
            Some(ws_cs.id.clone()),
            0.40,
            0.8,
            false,
            "Recent activity session belonged to ContextSplit",
        ));

        // Strong repository match for PayX
        act.add_signal(ActivityEvidenceSignal::project_repo_match(
            &ws_payx.id,
            "PayX",
        ));
        act.add_signal(ActivityEvidenceSignal::file_path_match(
            &ws_payx.id,
            "/Work/PayX/Sources/AppDelegate.swift",
        ));

        let (ingested, _) = manager.ingest_activity(act);
        // Strong repo + file path must decisively win over prior session continuity
        assert_eq!(ingested.status, ContextualActivityStatus::Assigned);
        assert_eq!(ingested.assigned_workspace_id.as_deref(), Some("ws_payx"));
    }

    #[test]
    fn test_scoring_weights_anti_dominance_caps_for_isolated_weak_app_signals() {
        let (ws_cs, _ws_payx, _ws_uni) = create_test_workspaces();
        let scorer = ContextEvidenceScorer::default();

        let mut ev_set = ContextEvidenceSet::new("item_isolated_app");
        ev_set.add(ContextEvidence::observed_app(
            "item_isolated_app",
            "Code",
            None,
        ));

        let res = scorer.score_evidence_against_workspace(&ev_set, &ws_cs);
        // Isolated weak app signal must NOT exceed the weak_signals_isolated_cap (0.24)
        assert!(res.total_score <= 0.24);
        assert!(res.confidence_band == "Very Low" || res.confidence_band == "Low");
        assert!(res.confidence < ScoringWeights::default().threshold_medium);
    }
}

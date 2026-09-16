#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use serde_json::json;

    use crate::context_engine::debugger::debug_item_context_confidence;
    use crate::context_engine::evidence::{
        ContextEvidence, ContextEvidenceSet, EvidenceCategory, EvidenceProvenance,
    };
    use crate::context_engine::operations::create_context_signal;
    use crate::context_engine::scoring::{
        evaluate_item_context, ContextEvidenceScorer, WorkspaceScoringContext,
    };
    use crate::db::models::{ContextInterpretationKind, NoContextReason};
    use crate::db::operations::{
        create_item, create_session, create_workspace,
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
    // Scenario 1: Same browser domain used for two unrelated projects
    //
    // Two workspaces ("Backend Rust Service" and "Client React App") both contain
    // items from github.com. A newly captured item has only `domain: github.com`
    // without session, graph, or semantic link to either.
    //
    // Expected:
    // - Anti-dominance prevents domain from dominating.
    // - Capped score <= 0.25.
    // - Low confidence.
    // - Ambiguity state is ambiguous or unknown; no misleading high-confidence winner.
    // =========================================================================
    #[test]
    fn test_adversarial_01_same_browser_domain_two_unrelated_projects() {
        let conn = setup_test_db();
        let ws_backend = create_workspace(&conn, "Backend Rust Service").unwrap();
        let ws_frontend = create_workspace(&conn, "Client React App").unwrap();

        // Populate both workspaces with items from github.com
        let item_b = create_item(&conn, Some(&ws_backend.id), None, "url", "https://github.com/org/rust-api", Some("Rust API"), "{\"domain\":\"github.com\"}").unwrap();
        let _ = create_context_signal(&conn, Some(&ws_backend.id), Some(&item_b.id), "url_metadata", None, "{\"domain\":\"github.com\"}").unwrap();

        let item_f = create_item(&conn, Some(&ws_frontend.id), None, "url", "https://github.com/org/react-web", Some("React Web"), "{\"domain\":\"github.com\"}").unwrap();
        let _ = create_context_signal(&conn, Some(&ws_frontend.id), Some(&item_f.id), "url_metadata", None, "{\"domain\":\"github.com\"}").unwrap();

        // Target item with only github.com domain
        let target = create_item(&conn, None, None, "url", "https://github.com/trending", Some("GitHub Trending"), "{\"domain\":\"github.com\"}").unwrap();
        let _ = create_context_signal(&conn, None, Some(&target.id), "url_metadata", None, "{\"domain\":\"github.com\"}").unwrap();

        let trace = debug_item_context_confidence(&conn, &target.id).unwrap();

        // 1. Verify score is capped by anti-dominance (domain alone cannot inflate score)
        for cand in &trace.candidates {
            assert!(cand.raw_score <= 0.25, "Score {} exceeds anti-dominance cap for domain only", cand.raw_score);
            assert!(cand.confidence <= 0.40, "Confidence {} should be low for domain only", cand.confidence);
            let band = cand.confidence_band.to_lowercase();
            assert!(band.contains("low"), "Expected low confidence band, got {}", cand.confidence_band);
        }

        // 2. Verify competition shows minimal margin between the two workspaces
        if let Some(comp) = &trace.competition {
            assert!(comp.score_margin <= 5, "Margin should be negligible between two projects sharing domain");
        }

        // 3. Verify final decision is not a confident assignment
        let final_band = trace.final_result.confidence_band.to_lowercase();
        assert_ne!(final_band, "high");
        assert_ne!(final_band, "very_high");
    }

    // =========================================================================
    // Scenario 2: Same application used for multiple contexts
    //
    // "Google Chrome" is used across Work, Personal, and Research.
    // A new item captured with only `app_name: "Google Chrome"`.
    //
    // Expected:
    // - Low scores (< 0.25), low confidence.
    // - Cannot blindly pick a single workspace with high confidence.
    // =========================================================================
    #[test]
    fn test_adversarial_02_same_app_used_for_multiple_contexts() {
        let conn = setup_test_db();
        let ws1 = create_workspace(&conn, "Work").unwrap();
        let ws2 = create_workspace(&conn, "Personal").unwrap();
        let ws3 = create_workspace(&conn, "Research").unwrap();

        for ws in &[&ws1, &ws2, &ws3] {
            let item = create_item(&conn, Some(&ws.id), None, "text", "doc", None, "{\"app_name\":\"Google Chrome\"}").unwrap();
            let _ = create_context_signal(&conn, Some(&ws.id), Some(&item.id), "active_app", None, "{\"app_name\":\"Google Chrome\"}").unwrap();
        }

        let target = create_item(&conn, None, None, "text", "new tab note", None, "{\"app_name\":\"Google Chrome\"}").unwrap();
        let _ = create_context_signal(&conn, None, Some(&target.id), "active_app", None, "{\"app_name\":\"Google Chrome\"}").unwrap();

        let interp = evaluate_item_context(&conn, &target.id).unwrap();

        assert_eq!(interp.interpretation_kind, Some(ContextInterpretationKind::NoContext));
        assert!(interp.existing_workspaces.iter().all(|ws| ws.score <= 0.25));
        assert!(interp.existing_workspaces.iter().all(|ws| ws.confidence <= 0.40));
        assert!(interp.summary.contains("No strong context yet") || interp.summary.contains("weak peripheral"));
    }

    // =========================================================================
    // Scenario 3: Generic words such as "React", "design", "assignment", "project"
    //
    // An item contains generic terms overlapping slightly with several workspaces
    // but with 0 specific session, graph, or confirmed history.
    //
    // Expected:
    // - Low semantic score without independent corroboration.
    // - Prevents false high-confidence categorization.
    // =========================================================================
    #[test]
    fn test_adversarial_03_generic_keywords_overlap() {
        let conn = setup_test_db();
        let ws_dev = create_workspace(&conn, "Web Development").unwrap();
        let ws_school = create_workspace(&conn, "School Assignments").unwrap();

        let _item_dev = create_item(&conn, Some(&ws_dev.id), None, "text", "React web project implementation", Some("React Project"), "{}").unwrap();
        let _item_school = create_item(&conn, Some(&ws_school.id), None, "text", "Design assignment project submission", Some("Design Assignment"), "{}").unwrap();

        let target = create_item(&conn, None, None, "text", "generic react design assignment project notes", Some("Generic Notes"), "{}").unwrap();

        let trace = debug_item_context_confidence(&conn, &target.id).unwrap();

        // Verify no false high-confidence winner
        assert_ne!(trace.final_result.confidence_band, "high");
        assert_ne!(trace.final_result.confidence_band, "very_high");
        assert!(trace.candidates.iter().all(|c| c.confidence < 0.65));
    }

    // =========================================================================
    // Scenario 4: Many repeated captures from one source
    //
    // 10 repeated URLs from the exact same domain (e.g. medium.com).
    //
    // Expected:
    // - Evidence diversity calculator collapses them into 1 correlation group.
    // - Diversity penalty caps confidence.
    // - Confidence remains bounded despite high raw evidence count.
    // =========================================================================
    #[test]
    fn test_adversarial_04_repeated_captures_from_single_source() {
        let mut evidence_set = ContextEvidenceSet::new("target_repeated_source");
        for i in 1..=10 {
            evidence_set.add(ContextEvidence::observed_url(
                "target_repeated_source",
                &format!("https://medium.com/@author/post-{}", i),
                None,
            ));
            evidence_set.add(ContextEvidence::observed_domain(
                "target_repeated_source",
                "medium.com",
                Some(&format!("https://medium.com/@author/post-{}", i)),
                None,
            ));
        }

        let mut ws = WorkspaceScoringContext::new("ws_reading", "Tech Reading");
        ws.domains.push("medium.com".to_string());

        let scorer = ContextEvidenceScorer::default();
        let results = scorer.score_evidence(&evidence_set, &[ws]);

        assert_eq!(results.len(), 1);
        let res = &results[0];

        // 1. Evidence count is 20 raw items, but independent groups count is only 1 (web_navigation)
        if let Some(cf) = &res.confidence_factors {
            assert_eq!(cf.independent_groups_count, 1);
            assert!(cf.diversity_score <= 0.40);
        }

        // 2. Confidence must not inflate to high/very_high simply because 10 URLs were captured
        assert!(res.confidence < 0.60, "Confidence {} inflated from repeated single source", res.confidence);
        assert_ne!(res.confidence_band, "very_high");
    }

    // =========================================================================
    // Scenario 5: Strong semantic similarity but explicit contradictory user feedback
    //
    // Item has high semantic similarity (0.88) with Workspace A,
    // but the user previously dismissed Workspace A.
    //
    // Expected:
    // - Contradiction report detects UserCorrection (penalty >= 0.70).
    // - Confidence is substantially reduced.
    // - Engine does not blindly pick Workspace A with high confidence.
    // - Explanation explicitly reports user dismissal.
    // =========================================================================
    #[test]
    fn test_adversarial_05_strong_semantic_with_explicit_contradictory_feedback() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Design System").unwrap();

        let item = create_item(&conn, None, None, "text", "Button component styles and color tokens", Some("Button Docs"), "{}").unwrap();

        // 1. User previously dismissed suggestion for this workspace
        let dismiss_payload = json!({"action": "suggestion_dismissed", "workspace_id": ws.id}).to_string();
        let _ = create_context_signal(&conn, None, Some(&item.id), "suggestion_dismissed", None, &dismiss_payload).unwrap();

        // 2. Target has semantic signal
        let sem_payload = json!({"similarity": 0.88, "workspace_id": ws.id}).to_string();
        let _ = create_context_signal(&conn, None, Some(&item.id), "semantic_similarity", None, &sem_payload).unwrap();

        let trace = debug_item_context_confidence(&conn, &item.id).unwrap();

        let cand = trace.candidates.iter().find(|c| c.context_id == ws.id).unwrap();

        // Contradiction detected
        assert!(!cand.contradictions.is_empty(), "Contradiction should be detected");
        assert!(cand.contradiction_bullets.iter().any(|b| b.contains("rejected") || b.contains("dismissed")));

        // Confidence penalized
        assert!(cand.confidence < 0.50, "Confidence should be discounted due to explicit dismissal");
        assert_ne!(trace.final_result.confidence_band, "high");
        assert_ne!(trace.final_result.confidence_band, "very_high");
    }

    // =========================================================================
    // Scenario 6: Two contexts with nearly identical evidence
    //
    // Workspace 1 ("Frontend App") and Workspace 2 ("Client Portal") have close scores (0.72 vs 0.70).
    //
    // Expected:
    // - Ambiguity classification is `Ambiguous`.
    // - Relative confidence is downgraded due to narrow margin (< 0.10).
    // - Engine flags Ambiguous instead of picking one arbitrarily.
    // =========================================================================
    #[test]
    fn test_adversarial_06_competing_contexts_nearly_identical_evidence() {
        let conn = setup_test_db();
        let ws1 = create_workspace(&conn, "Frontend App").unwrap();
        let ws2 = create_workspace(&conn, "Client Portal").unwrap();

        let session = create_session(&conn).unwrap();

        // Populate both workspaces with matching session and items
        let item1 = create_item(&conn, Some(&ws1.id), Some(&session.id), "url", "https://app.dev/dashboard", Some("Dash"), "{\"domain\":\"app.dev\"}").unwrap();
        let _ = create_context_signal(&conn, Some(&ws1.id), Some(&item1.id), "url_metadata", None, "{\"domain\":\"app.dev\"}").unwrap();

        let item2 = create_item(&conn, Some(&ws2.id), Some(&session.id), "url", "https://app.dev/portal", Some("Portal"), "{\"domain\":\"app.dev\"}").unwrap();
        let _ = create_context_signal(&conn, Some(&ws2.id), Some(&item2.id), "url_metadata", None, "{\"domain\":\"app.dev\"}").unwrap();

        let target = create_item(&conn, None, Some(&session.id), "url", "https://app.dev/settings", Some("Settings"), "{\"domain\":\"app.dev\"}").unwrap();
        let _ = create_context_signal(&conn, None, Some(&target.id), "url_metadata", None, "{\"domain\":\"app.dev\"}").unwrap();

        // Both workspaces have strong, nearly identical graph/semantic evidence
        crate::db::operations::create_item_relationship(&conn, &target.id, &item1.id, "semantic_similarity", 0.88, None).unwrap();
        crate::db::operations::create_item_relationship(&conn, &target.id, &item2.id, "semantic_similarity", 0.86, None).unwrap();

        let trace = debug_item_context_confidence(&conn, &target.id).unwrap();

        assert!(trace.candidates.len() >= 2);
        assert!(trace.competition.is_some());
        let comp = trace.competition.unwrap();

        // Margin is narrow
        assert!(comp.score_margin <= 10, "Margin {} should be small between identical candidates", comp.score_margin);
        assert_eq!(comp.ambiguity_state, "ambiguous");
        assert!(trace.final_result.decision == "ambiguous" || trace.final_result.confidence_band != "high");
    }

    // =========================================================================
    // Scenario 7: Old historical evidence versus strong recent evidence
    //
    // Item has an old historical move signal (60 days ago) for Workspace A,
    // but a strong active session + confirmed items for Workspace B (5 mins ago).
    //
    // Expected:
    // - Temporal decay discounts the 60-day old historical signal.
    // - Workspace B wins clearly.
    // =========================================================================
    #[test]
    fn test_adversarial_07_old_historical_evidence_vs_strong_recent_evidence() {
        let conn = setup_test_db();
        let ws_old = create_workspace(&conn, "Old Project").unwrap();
        let ws_active = create_workspace(&conn, "Active Sprint").unwrap();

        let session_now = create_session(&conn).unwrap();

        // Active workspace items in current session
        let ws_active_item = create_item(&conn, Some(&ws_active.id), Some(&session_now.id), "url", "https://jira.corp/sprint-42", Some("Sprint 42"), "{\"domain\":\"jira.corp\"}").unwrap();
        let _ = create_context_signal(&conn, Some(&ws_active.id), Some(&ws_active_item.id), "url_metadata", None, "{\"domain\":\"jira.corp\"}").unwrap();

        let target = create_item(&conn, None, Some(&session_now.id), "url", "https://jira.corp/ticket-101", Some("Ticket 101"), "{\"domain\":\"jira.corp\"}").unwrap();
        let _ = create_context_signal(&conn, None, Some(&target.id), "url_metadata", None, "{\"domain\":\"jira.corp\"}").unwrap();

        // Strong active semantic alignment with active sprint
        let sem_active = json!({"similarity": 0.88, "workspace_id": ws_active.id}).to_string();
        let _ = create_context_signal(&conn, None, Some(&target.id), "semantic_similarity", None, &sem_active).unwrap();

        // Old historical move to ws_old (60 days ago)
        let old_time = (chrono::Utc::now() - chrono::Duration::days(60)).to_rfc3339();
        let hist_payload = json!({"action": "moved_to_workspace", "destination_workspace": ws_old.id}).to_string();
        let sig = create_context_signal(&conn, None, Some(&target.id), "user_action", None, &hist_payload).unwrap();
        conn.execute("UPDATE context_signals SET created_at = ?1 WHERE id = ?2", rusqlite::params![&old_time, &sig.id]).unwrap();

        let trace = debug_item_context_confidence(&conn, &target.id).unwrap();

        let cand_active = trace.candidates.iter().find(|c| c.context_id == ws_active.id).unwrap();
        let cand_old = trace.candidates.iter().find(|c| c.context_id == ws_old.id).unwrap();

        // Active workspace score must strictly beat decayed old project
        assert!(cand_active.raw_score > cand_old.raw_score);
        assert_eq!(trace.candidates[0].context_id, ws_active.id);
    }

    // =========================================================================
    // Scenario 8: A single highly unusual item inside an otherwise coherent context
    //
    // A tax return PDF captured while user has gaming workspace open.
    //
    // Expected:
    // - Low semantic relevance and absence of graph relationships.
    // - Engine diagnoses `no_context` or `weak_evidence`.
    // - Does NOT falsely assign Tax PDF to Gaming Workspace.
    // =========================================================================
    #[test]
    fn test_adversarial_08_unusual_isolated_item_in_coherent_workspace() {
        let conn = setup_test_db();
        let ws_gaming = create_workspace(&conn, "Gaming & Esports").unwrap();

        let game_item = create_item(&conn, Some(&ws_gaming.id), None, "url", "https://twitch.tv/streamer", Some("Twitch"), "{\"domain\":\"twitch.tv\"}").unwrap();
        let _ = create_context_signal(&conn, Some(&ws_gaming.id), Some(&game_item.id), "url_metadata", None, "{\"domain\":\"twitch.tv\"}").unwrap();

        // Tax return item with only file_metadata / pdf
        let tax_item = create_item(&conn, None, None, "file", "/docs/Tax_Return_2025.pdf", Some("Tax Return 2025"), "{\"filename\":\"Tax_Return_2025.pdf\",\"mime_type\":\"application/pdf\"}").unwrap();

        let interp = evaluate_item_context(&conn, &tax_item.id).unwrap();

        assert_eq!(interp.interpretation_kind, Some(ContextInterpretationKind::NoContext));
        assert!(interp.existing_workspaces.iter().all(|ws| ws.score < 0.20));
        assert!(interp.summary.contains("No strong context yet"));
    }

    // =========================================================================
    // Scenario 9: Noisy/random captures
    //
    // Random UUID or gibberish string captured without app/domain/session metadata.
    //
    // Expected:
    // - Evaluates to `no_context` (`zero_evidence` or `weak_evidence`).
    // - Confidence is `very_low` (0%).
    // =========================================================================
    #[test]
    fn test_adversarial_09_noisy_random_captures() {
        let conn = setup_test_db();
        let _ws = create_workspace(&conn, "Portfolio").unwrap();

        let noise_item = create_item(&conn, None, None, "", "d82f3a91-4c12-4e89-a991-8fa92301c23a", None, "{}").unwrap();

        let interp = evaluate_item_context(&conn, &noise_item.id).unwrap();

        assert_eq!(interp.interpretation_kind, Some(ContextInterpretationKind::NoContext));
        assert_eq!(interp.no_context_reason, Some(NoContextReason::ZeroEvidence));
        assert!(interp.summary.contains("No strong context yet"));
    }

    // =========================================================================
    // Scenario 10: A completely new topic with no existing context
    //
    // Item about Quantum Computing Simulation when existing workspaces are
    // "E-Commerce Store" and "Fitness Tracker".
    //
    // Expected:
    // - All workspace scores < 0.20.
    // - Returns `no_context` (low semantic relevance / no meaningful relationships).
    // - Does not force item into any workspace.
    // =========================================================================
    #[test]
    fn test_adversarial_10_completely_new_topic_no_existing_context() {
        let conn = setup_test_db();
        let _ws_ecom = create_workspace(&conn, "E-Commerce Store").unwrap();
        let _ws_fit = create_workspace(&conn, "Fitness Tracker").unwrap();

        let item = create_item(&conn, None, None, "text", "quantum entanglement state simulation hamiltonian operator", Some("Quantum Sim"), "{}").unwrap();

        let interp = evaluate_item_context(&conn, &item.id).unwrap();

        assert_eq!(interp.interpretation_kind, Some(ContextInterpretationKind::NoContext));
        assert!(interp.existing_workspaces.iter().all(|ws| ws.score < 0.20));
        assert!(interp.summary.contains("No strong context yet"));
    }

    // =========================================================================
    // Scenario 11: Multiple contexts active within a short period
    //
    // Rapid switching between Work and Personal within 2 minutes.
    //
    // Expected:
    // - Signals point to both Work and Personal.
    // - Score margin is narrow, resulting in `ambiguous` or low confidence.
    // - Engine does not make a reckless high-confidence guess.
    // =========================================================================
    #[test]
    fn test_adversarial_11_rapid_context_switching_temporal_overlap() {
        let conn = setup_test_db();
        let ws_work = create_workspace(&conn, "Work").unwrap();
        let ws_personal = create_workspace(&conn, "Personal").unwrap();

        let session = create_session(&conn).unwrap();

        // Recent items in both workspaces in same session
        let w_item = create_item(&conn, Some(&ws_work.id), Some(&session.id), "url", "https://corp.internal/task", Some("Task"), "{\"domain\":\"corp.internal\"}").unwrap();
        let _ = create_context_signal(&conn, Some(&ws_work.id), Some(&w_item.id), "url_metadata", None, "{\"domain\":\"corp.internal\"}").unwrap();

        let p_item = create_item(&conn, Some(&ws_personal.id), Some(&session.id), "url", "https://reddit.com/r/hobby", Some("Hobby"), "{\"domain\":\"reddit.com\"}").unwrap();
        let _ = create_context_signal(&conn, Some(&ws_personal.id), Some(&p_item.id), "url_metadata", None, "{\"domain\":\"reddit.com\"}").unwrap();

        // Target captured during rapid switch
        let target = create_item(&conn, None, Some(&session.id), "text", "quick note during switch", None, "{}").unwrap();

        let trace = debug_item_context_confidence(&conn, &target.id).unwrap();

        assert!(trace.candidates.len() >= 2);
        if let Some(comp) = trace.competition {
            assert!(comp.score_margin <= 15);
            assert!(comp.ambiguity_state == "ambiguous" || comp.ambiguity_state == "unknown");
        }
        assert_ne!(trace.final_result.confidence_band, "high");
    }

    // =========================================================================
    // Scenario 12: Very sparse evidence
    //
    // Item with 0 metadata and empty body.
    //
    // Expected:
    // - Score = 0.0, Confidence = 0.0 ("very_low").
    // - Reason = `zero_evidence`.
    // =========================================================================
    #[test]
    fn test_adversarial_12_very_sparse_evidence() {
        let conn = setup_test_db();
        let _ws = create_workspace(&conn, "Main Workspace").unwrap();

        let empty_item = create_item(&conn, None, None, "", "", None, "{}").unwrap();

        let interp = evaluate_item_context(&conn, &empty_item.id).unwrap();

        assert_eq!(interp.interpretation_kind, Some(ContextInterpretationKind::NoContext));
        assert_eq!(interp.no_context_reason, Some(NoContextReason::ZeroEvidence));
        assert_eq!(interp.existing_workspaces[0].score, 0.0);
        assert_eq!(interp.existing_workspaces[0].confidence, 0.0);
    }

    // =========================================================================
    // Scenario 13: Strong evidence from only one category
    //
    // Item has high semantic similarity (0.95), but 0 other categories
    // (no session, no domain, no app, no relationship, no history).
    //
    // Expected:
    // - Diversity calculator flags single group.
    // - Calibrated confidence cannot reach `very_high`.
    // =========================================================================
    #[test]
    fn test_adversarial_13_strong_evidence_single_category_bounded_confidence() {
        let mut evidence_set = ContextEvidenceSet::new("target_single_strong");
        let mut ev = ContextEvidence::new(
            EvidenceCategory::SemanticSimilarity,
            EvidenceProvenance::Inferred,
            0.95,
            "target_single_strong",
            crate::context_engine::evidence::EvidenceExplanation::new("Semantic Similarity", "High cosine similarity"),
        );
        ev = ev.with_attribute("workspace_id", "ws_ai");
        evidence_set.add(ev);

        let ws = WorkspaceScoringContext::new("ws_ai", "AI Research");
        let scorer = ContextEvidenceScorer::default();
        let results = scorer.score_evidence(&evidence_set, &[ws]);

        assert_eq!(results.len(), 1);
        let res = &results[0];

        // Raw score reflects semantic contribution (capped by semantic_similarity_max 0.40),
        // and calibrated confidence is strictly bounded without diversity
        assert!(res.total_score >= 0.35);
        assert!(res.confidence < 0.85, "Confidence should not reach very_high without diversity");
        assert_ne!(res.confidence_band.to_lowercase(), "very_high");
    }

    // =========================================================================
    // Scenario 14: Several weak signals incorrectly combining into high confidence
    //
    // Item has App ("Safari"), ContentType ("url"), CaptureMethod ("hotkey"),
    // and Domain ("generic-cdn.net"). All 4 are peripheral weak signals.
    //
    // Expected:
    // - Anti-dominance invariant caps score at `weak_signals_isolated_cap` (<= 0.25).
    // - Confidence remains `low` / `very_low`.
    // - Diagnosed as `weak_evidence`.
    // =========================================================================
    #[test]
    fn test_adversarial_14_multiple_weak_signals_anti_dominance() {
        let mut evidence_set = ContextEvidenceSet::new("target_weak_combo");
        evidence_set.add(ContextEvidence::observed_app("target_weak_combo", "Safari", None));
        evidence_set.add(ContextEvidence::observed_content_type("target_weak_combo", "url", None));
        evidence_set.add(ContextEvidence::observed_capture_method("target_weak_combo", "hotkey", None));
        evidence_set.add(ContextEvidence::observed_domain("target_weak_combo", "generic-cdn.net", None, None));

        let mut ws = WorkspaceScoringContext::new("ws_general", "General Workspace");
        ws.app_names.push("Safari".to_string());
        ws.item_types.push("url".to_string());
        ws.domains.push("generic-cdn.net".to_string());

        let scorer = ContextEvidenceScorer::default();
        let results = scorer.score_evidence(&evidence_set, &[ws]);

        assert_eq!(results.len(), 1);
        let res = &results[0];

        // Total score capped by weak_signals_isolated_cap
        assert!(res.total_score <= 0.25, "Score {} exceeds weak signals cap", res.total_score);
        assert!(res.confidence <= 0.40, "Confidence {} inflated from weak signals", res.confidence);
        let band = res.confidence_band.to_lowercase();
        assert!(band.contains("low"), "Expected low confidence band, got {}", res.confidence_band);
    }

    // =========================================================================
    // Scenario 15: New context candidate competing against an existing Workspace
    //
    // An emerging context candidate cluster ("Project Horizon") strongly groups
    // the item, while existing Workspace has weak/moderate link.
    //
    // Expected:
    // - `evaluate_item_context` returns both `existing_workspaces` and `potential_new_contexts`.
    // - Does NOT automatically force item into existing workspace.
    // =========================================================================
    #[test]
    fn test_adversarial_15_candidate_competing_against_existing_workspace() {
        let conn = setup_test_db();
        let _ws_general = create_workspace(&conn, "General Workspace").unwrap();

        // Create a new context candidate
        let cand = crate::context_engine::discovery::create_candidate(&conn, 0.85, Some("Project Horizon"), "{}").unwrap();

        let item = create_item(&conn, None, None, "text", "Horizon architecture specification document", Some("Horizon Spec"), "{}").unwrap();

        // Add item to candidate cluster
        let _ = crate::context_engine::discovery::add_item_to_candidate(&conn, &cand.id, &item.id, 0.90).unwrap();

        let interp = evaluate_item_context(&conn, &item.id).unwrap();

        // Verify candidate is evaluated alongside workspace
        assert!(!interp.potential_new_contexts.is_empty());
        let top_cand = &interp.potential_new_contexts[0];
        assert_eq!(top_cand.label, "Project Horizon");
        assert!(top_cand.confidence >= 0.70);

        // Verify item is NOT automatically forced into General Workspace
        assert_ne!(interp.primary_interpretation, "existing_workspace");
    }
}

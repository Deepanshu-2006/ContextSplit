#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use serde_json::json;

    use crate::context_engine::discovery::run_discovery;
    use crate::context_engine::evidence::{ContextEvidence, ContextEvidenceCollector};
    use crate::context_engine::explanation::explain_context_association;
    use crate::context_engine::normalization::EvidenceNormalizer;
    use crate::context_engine::operations::create_context_signal;
    use crate::context_engine::scoring::{
        evaluate_item_context, score_item, ContextEvidenceScorer, WorkspaceScoringContext,
    };
    use crate::db::operations::{
        create_item, create_item_relationship, create_session, create_workspace, get_item,
        list_items,
    };

    fn setup_in_memory_db() -> Connection {
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
    // Scenario 1: Portfolio contains confirmed Figma, React and design items.
    // Capture another Figma-related item.
    // Expected: Portfolio receives strong contextual evidence & high-confidence suggestion.
    // =========================================================================
    #[test]
    fn test_scenario_1_strong_contextual_evidence_yields_high_confidence() {
        let conn = setup_in_memory_db();

        // 1. Create Portfolio workspace
        let ws_portfolio = create_workspace(&conn, "Portfolio").unwrap();
        let session = create_session(&conn).unwrap();

        // 2. Confirmed items in Portfolio
        let figma_ui = create_item(
            &conn,
            Some(&ws_portfolio.id),
            Some(&session.id),
            "url",
            "https://figma.com/file/101/Portfolio-UI-Kit",
            Some("Portfolio UI Kit"),
            "{\"app_name\":\"Figma\",\"original_url\":\"https://figma.com/file/101/Portfolio-UI-Kit\"}",
        )
        .unwrap();

        let react_hero = create_item(
            &conn,
            Some(&ws_portfolio.id),
            Some(&session.id),
            "code",
            "export const Hero = () => <h1>Portfolio Showcase</h1>;",
            Some("Portfolio Hero Component"),
            "{\"app_name\":\"VSCode\"}",
        )
        .unwrap();

        let design_tokens = create_item(
            &conn,
            Some(&ws_portfolio.id),
            Some(&session.id),
            "text",
            "Primary color: #6366F1, Typography: Inter",
            Some("Design Tokens"),
            "{\"app_name\":\"Figma\"}",
        )
        .unwrap();

        let _ = create_context_signal(&conn, Some(&ws_portfolio.id), Some(&figma_ui.id), "active_app", None, "{\"app_name\":\"Figma\"}").unwrap();
        let _ = create_context_signal(&conn, Some(&ws_portfolio.id), Some(&figma_ui.id), "url_metadata", None, "{\"domain\":\"figma.com\"}").unwrap();
        let _ = create_context_signal(&conn, Some(&ws_portfolio.id), Some(&react_hero.id), "active_app", None, "{\"app_name\":\"VSCode\"}").unwrap();
        let _ = create_context_signal(&conn, Some(&ws_portfolio.id), Some(&design_tokens.id), "active_app", None, "{\"app_name\":\"Figma\"}").unwrap();

        // 3. Target Item: newly captured Figma case study
        let target = create_item(
            &conn,
            None,
            Some(&session.id),
            "url",
            "https://figma.com/file/202/Portfolio-Case-Study",
            Some("Portfolio Case Study Presentation"),
            "{\"app_name\":\"Figma\",\"original_url\":\"https://figma.com/file/202/Portfolio-Case-Study\"}",
        )
        .unwrap();

        let _ = create_context_signal(&conn, None, Some(&target.id), "active_app", None, "{\"app_name\":\"Figma\"}").unwrap();
        let _ = create_context_signal(&conn, None, Some(&target.id), "url_metadata", None, "{\"domain\":\"figma.com\"}").unwrap();

        // Add history and strong semantic relationship
        let hist_payload = json!({
            "action": "moved_to_workspace",
            "destination_workspace": ws_portfolio.id
        })
        .to_string();
        let _ = create_context_signal(&conn, None, Some(&target.id), "user_action", None, &hist_payload).unwrap();

        let _ = create_item_relationship(&conn, &target.id, &figma_ui.id, "semantic_similarity", 0.92, None).unwrap();
        let _ = create_item_relationship(&conn, &target.id, &design_tokens.id, "same_app", 1.0, None).unwrap();

        // 4. Evaluate contextual score
        let scores = score_item(&conn, &target.id).unwrap();
        assert!(!scores.is_empty(), "Scores must contain Portfolio");
        let port_score = scores.iter().find(|s| s.workspace_id == ws_portfolio.id).unwrap();

        // Assert: High confidence suggestion (score >= 0.70)
        assert!(
            port_score.total_score >= 0.70,
            "Portfolio score must be >= 0.70 (actual: {})",
            port_score.total_score
        );
        assert!(port_score.confidence_band == "Very High" || port_score.confidence_band == "High");

        // 5. Verify ContextExplanation
        let explanation = explain_context_association(&conn, &target.id, &ws_portfolio.id).unwrap();
        assert_eq!(explanation.workspace_name, "Portfolio");
        assert!(explanation.confidence_band == "Very High" || explanation.confidence_band == "High");
        assert!(explanation.confidence >= 0.70);
        assert!(explanation.summary.contains("Portfolio —"));

        // Verify concise why reasons with checkmarks
        assert!(explanation.why_reasons.iter().any(|r| r.contains("Same Figma source app")));
        assert!(explanation.why_reasons.iter().any(|r| r.contains("Same Figma domain")));
        assert!(explanation.why_reasons.iter().any(|r| r.contains("Semantic similarity with Portfolio")));
        assert!(explanation.why_reasons.iter().any(|r| r.contains("User previously organized similar Items into Portfolio")));

        // Verify segregated observed and inferred evidence
        assert!(!explanation.observed_evidence.is_empty());
        assert!(!explanation.inferred_evidence.is_empty());
        assert!(explanation.contradictory_evidence.is_empty());

        // Zero-Mutation Invariant: Target item is not moved or assigned
        let target_after = get_item(&conn, &target.id).unwrap();
        assert_eq!(target_after.workspace_id, None, "Target item must remain in Inbox");
    }

    // =========================================================================
    // Scenario 2: College context contains OS assignment materials.
    // Capture an unrelated travel item.
    // Expected: College receives weak/no contextual suggestion.
    // =========================================================================
    #[test]
    fn test_scenario_2_unrelated_item_yields_weak_or_no_suggestion() {
        let conn = setup_in_memory_db();

        let ws_college = create_workspace(&conn, "College").unwrap();

        let _os_pdf = create_item(
            &conn,
            Some(&ws_college.id),
            None,
            "pdf",
            "Virtual memory paging algorithm notes",
            Some("OS Assignment 3 Notes"),
            "{\"app_name\":\"Preview\"}",
        )
        .unwrap();

        let _kernel_code = create_item(
            &conn,
            Some(&ws_college.id),
            None,
            "code",
            "struct page *alloc_page(void);",
            Some("Kernel Page Allocator"),
            "{\"app_name\":\"VSCode\"}",
        )
        .unwrap();

        // Capture an unrelated travel item
        let travel_item = create_item(
            &conn,
            None,
            None,
            "url",
            "https://booking.com/hotels/tokyo-shinjuku",
            Some("Tokyo Hotel Reservation"),
            "{\"app_name\":\"Safari\",\"original_url\":\"https://booking.com/hotels/tokyo-shinjuku\"}",
        )
        .unwrap();

        let _ = create_context_signal(&conn, None, Some(&travel_item.id), "active_app", None, "{\"app_name\":\"Safari\"}").unwrap();
        let _ = create_context_signal(&conn, None, Some(&travel_item.id), "url_metadata", None, "{\"domain\":\"booking.com\"}").unwrap();

        let scores = score_item(&conn, &travel_item.id).unwrap();
        let college_score = scores.iter().find(|s| s.workspace_id == ws_college.id);

        // College score must either be filtered out (< 0.20) or have Low/Very Low confidence band
        if let Some(s) = college_score {
            assert!(s.total_score < 0.20, "College score should be < 0.20, got {}", s.total_score);
            assert!(s.confidence_band == "Low" || s.confidence_band == "Very Low");
        }

        // Full context evaluation
        let interp = evaluate_item_context(&conn, &travel_item.id).unwrap();
        let college_eval = interp.existing_workspaces.iter().find(|w| w.workspace_id == ws_college.id);
        if let Some(eval) = college_eval {
            assert!(eval.score < 0.20, "College suggestion score must be < 0.20, got {}", eval.score);
            assert!(eval.confidence_band == "Low" || eval.confidence_band == "Very Low");
        }

        // Zero-Mutation Invariant
        let item_check = get_item(&conn, &travel_item.id).unwrap();
        assert_eq!(item_check.workspace_id, None);
    }

    // =========================================================================
    // Scenario 3: Several travel items appear within the same session:
    // hotel, flight, restaurant, map.
    // Expected: Existing contexts ranked, but if none fit, evidence supports new context candidate.
    // =========================================================================
    #[test]
    fn test_scenario_3_co_session_cluster_supports_new_context_candidate() {
        let conn = setup_in_memory_db();

        // Existing unrelated workspace
        let ws_work = create_workspace(&conn, "Work Projects").unwrap();
        let _work_item = create_item(
            &conn,
            Some(&ws_work.id),
            None,
            "code",
            "git push origin main",
            Some("Deploy Script"),
            "{\"app_name\":\"Terminal\"}",
        )
        .unwrap();

        // Several travel items appear in the same session
        let session = create_session(&conn).unwrap();

        let hotel = create_item(
            &conn,
            None,
            Some(&session.id),
            "url",
            "https://booking.com/kyoto-hotel",
            Some("Kyoto Ryokan Hotel Booking"),
            "{\"app_name\":\"Safari\",\"original_url\":\"https://booking.com/kyoto-hotel\"}",
        )
        .unwrap();

        let flight = create_item(
            &conn,
            None,
            Some(&session.id),
            "url",
            "https://jal.co.jp/flight-kyoto",
            Some("Flight to Kansai Airport"),
            "{\"app_name\":\"Safari\",\"original_url\":\"https://jal.co.jp/flight-kyoto\"}",
        )
        .unwrap();

        let restaurant = create_item(
            &conn,
            None,
            Some(&session.id),
            "url",
            "https://tabelog.com/kyoto-ramen",
            Some("Kyoto Gion Ramen Recommendation"),
            "{\"app_name\":\"Safari\",\"original_url\":\"https://tabelog.com/kyoto-ramen\"}",
        )
        .unwrap();

        let map = create_item(
            &conn,
            None,
            Some(&session.id),
            "url",
            "https://maps.google.com/kyoto-bamboo",
            Some("Arashiyama Bamboo Grove Map"),
            "{\"app_name\":\"Safari\",\"original_url\":\"https://maps.google.com/kyoto-bamboo\"}",
        )
        .unwrap();

        for item in &[&hotel, &flight, &restaurant, &map] {
            let _ = create_context_signal(&conn, None, Some(&item.id), "active_app", None, "{\"app_name\":\"Safari\"}").unwrap();
        }

        // Establish session-based relationships among the travel items
        let _ = create_item_relationship(&conn, &hotel.id, &flight.id, "same_session", 0.85, None).unwrap();
        let _ = create_item_relationship(&conn, &flight.id, &restaurant.id, "same_session", 0.85, None).unwrap();
        let _ = create_item_relationship(&conn, &restaurant.id, &map.id, "same_session", 0.85, None).unwrap();

        // Run discovery / candidate evaluation on hotel
        let interpretation = evaluate_item_context(&conn, &hotel.id).unwrap();

        // Existing workspace score is very low
        let work_eval = interpretation.existing_workspaces.iter().find(|w| w.workspace_id == ws_work.id);
        if let Some(eval) = work_eval {
            assert!(eval.score < 0.20);
        }

        // Evidence supports a potential new context candidate
        assert!(!interpretation.potential_new_contexts.is_empty(), "Potential new contexts must be detected");
        let candidate = &interpretation.potential_new_contexts[0];
        assert!(!candidate.label.is_empty(), "Candidate must have a valid label");
        assert!(!candidate.reasons.is_empty());

        // Context candidate discovery across workspace
        let candidates = run_discovery(&conn).unwrap();
        assert!(!candidates.is_empty(), "Session-based context candidate must be detected");
        assert!(candidates.iter().any(|c| c.status == "detected" && c.confidence > 0.0));

        // Zero-Mutation Invariant
        assert_eq!(get_item(&conn, &hotel.id).unwrap().workspace_id, None);
        assert_eq!(get_item(&conn, &flight.id).unwrap().workspace_id, None);
        assert_eq!(get_item(&conn, &restaurant.id).unwrap().workspace_id, None);
        assert_eq!(get_item(&conn, &map.id).unwrap().workspace_id, None);
        assert_eq!(list_items(&conn, Some(&ws_work.id)).unwrap().len(), 1);
    }

    // =========================================================================
    // Scenario 4: An item has strong semantic similarity but contradictory user feedback.
    // Expected: User feedback reduces the contextual score.
    // =========================================================================
    #[test]
    fn test_scenario_4_contradictory_user_feedback_reduces_context_score() {
        let conn = setup_in_memory_db();

        let ws_mobile = create_workspace(&conn, "Mobile App").unwrap();

        let mobile_spec = create_item(
            &conn,
            Some(&ws_mobile.id),
            None,
            "text",
            "iOS Swift SwiftUI navigation architecture",
            Some("SwiftUI Architecture"),
            "{\"app_name\":\"Xcode\"}",
        )
        .unwrap();

        let target = create_item(
            &conn,
            None,
            None,
            "text",
            "iOS SwiftUI state management with Combine",
            Some("SwiftUI State Guide"),
            "{\"app_name\":\"Xcode\"}",
        )
        .unwrap();

        // 1. Semantic similarity edge with high score (0.95)
        let _ = create_item_relationship(&conn, &target.id, &mobile_spec.id, "semantic_similarity", 0.95, None).unwrap();

        // Score before rejection
        let score_before = score_item(&conn, &target.id).unwrap();
        let mobile_score_before = score_before.iter().find(|s| s.workspace_id == ws_mobile.id).unwrap();
        let initial_score = mobile_score_before.total_score;
        assert!(initial_score >= 0.40, "Initial score with 0.95 semantic similarity should be >= 0.40 (actual: {})", initial_score);

        // 2. Add contradictory user feedback (user dismissed/rejected suggestion for Mobile App)
        let dismiss_payload = json!({
            "action": "dismissed_suggestion",
            "destination_workspace": ws_mobile.id,
            "feedback_type": "user_dismissal"
        })
        .to_string();
        let _ = create_context_signal(&conn, None, Some(&target.id), "membership_dismissed", None, &dismiss_payload).unwrap();

        // 3. Score after rejection
        let score_after = score_item(&conn, &target.id).unwrap();
        let mobile_score_after = score_after.iter().find(|s| s.workspace_id == ws_mobile.id);

        let final_score = mobile_score_after.map(|s| s.total_score).unwrap_or(0.0);
        assert!(
            final_score < initial_score,
            "User feedback must reduce score: initial {}, final {}",
            initial_score,
            final_score
        );

        // 4. Verify explanation exposes contradictory evidence and uncertainty
        let explanation = explain_context_association(&conn, &target.id, &ws_mobile.id).unwrap();
        assert!(!explanation.contradictory_evidence.is_empty(), "Contradictory evidence must be present");
        assert!(explanation.contradictory_evidence.iter().any(|c| c.category == "user_feedback"));
        assert!(!explanation.uncertainty_reasons.is_empty());
        assert!(explanation.uncertainty_reasons.iter().any(|u| u.contains("negative user feedback")));

        // Zero-Mutation Invariant
        assert_eq!(get_item(&conn, &target.id).unwrap().workspace_id, None);
    }

    // =========================================================================
    // Scenario 5: Only weak evidence exists.
    // Expected: Engine returns uncertain/no strong suggestion rather than forcing a context.
    // =========================================================================
    #[test]
    fn test_scenario_5_weak_signals_capped_and_return_uncertain_without_forcing() {
        let conn = setup_in_memory_db();

        let ws_general = create_workspace(&conn, "General Workspace").unwrap();

        // Workspace contains 10 items from Google Chrome
        for i in 1..=10 {
            let item = create_item(
                &conn,
                Some(&ws_general.id),
                None,
                "url",
                &format!("https://random-site-{}.com", i),
                Some(&format!("Random Site {}", i)),
                "{\"app_name\":\"Google Chrome\"}",
            )
            .unwrap();
            let _ = create_context_signal(&conn, Some(&ws_general.id), Some(&item.id), "active_app", None, "{\"app_name\":\"Google Chrome\"}").unwrap();
        }

        // Target item ONLY shares the app name "Google Chrome"
        let target = create_item(
            &conn,
            None,
            None,
            "url",
            "https://completely-unrelated-domain.org",
            Some("Unrelated Web Page"),
            "{\"app_name\":\"Google Chrome\"}",
        )
        .unwrap();
        let _ = create_context_signal(&conn, None, Some(&target.id), "active_app", None, "{\"app_name\":\"Google Chrome\"}").unwrap();

        // Evaluate target item
        let interp = evaluate_item_context(&conn, &target.id).unwrap();
        let eval = interp.existing_workspaces.iter().find(|w| w.workspace_id == ws_general.id);

        if let Some(e) = eval {
            // Anti-dominance cap ensures isolated app signal is <= 0.24 (Low/Very Low band)
            assert!(
                e.score <= 0.24,
                "Isolated source app signal must not exceed cap of 0.24 (actual: {})",
                e.score
            );
            assert!(e.confidence_band == "Low" || e.confidence_band == "Very Low", "Must never force Medium or High confidence on weak signal");
        }

        // Verify ContextScoreResult details
        let scorer = ContextEvidenceScorer::new();
        let evidence_set = ContextEvidenceCollector::new().collect(&conn, &target.id).unwrap();
        let ws_contexts = crate::context_engine::scoring::load_workspace_scoring_contexts(&conn).unwrap();
        let ws_ctx = ws_contexts.into_iter().find(|w| w.id == ws_general.id).unwrap();
        let result = scorer.score_evidence_against_workspace(&evidence_set, &ws_ctx);

        assert!(result.total_score <= 0.24);
        assert!(result.confidence_band == "Low" || result.confidence_band == "Very Low");
        assert!(result.uncertainty_indicators.iter().any(|u| u.contains("weak peripheral signals")));

        // Zero-Mutation Invariant
        assert_eq!(get_item(&conn, &target.id).unwrap().workspace_id, None);
    }

    // =========================================================================
    // Scenario 6: The same underlying signal appears multiple times.
    // Expected: Evidence caps prevent artificial confidence inflation.
    // =========================================================================
    #[test]
    fn test_scenario_6_repeated_signals_saturate_and_prevent_inflation() {
        let normalizer = EvidenceNormalizer::default();
        let item_id = "test_item_scenario_6";

        // 10 repeated URLs all pointing to figma.com
        let mut evidences = Vec::new();
        for i in 1..=10 {
            let url = format!("https://figma.com/file/comp_{}/design", i);
            evidences.push(ContextEvidence::observed_domain(item_id, "figma.com", Some(&url), None));
        }

        // Normalize
        let normalized_set = normalizer.normalize_slice(item_id, &evidences);

        // Group total effective weight must saturate at the domain cap (0.35)
        let domain_total: f64 = normalized_set
            .by_correlation_group("domain:figma.com")
            .iter()
            .map(|e| e.effective_weight)
            .sum();

        assert!(
            domain_total <= 0.35001,
            "Domain total must saturate at 0.35 cap, actual: {}",
            domain_total
        );

        // Diminishing returns: later items contribute less than the first item
        assert!(normalized_set.items[0].effective_weight > normalized_set.items[1].effective_weight);
        assert!(normalized_set.items[1].effective_weight > normalized_set.items[2].effective_weight);
        assert!(normalized_set.items.last().unwrap().is_capped || normalized_set.items.last().unwrap().effective_weight < 0.01);

        // Scorer evaluation: multiple weak signals cannot inflate score above weak signals isolated cap
        let mut ws = WorkspaceScoringContext::new("ws_design", "Design Workspace");
        for _ in 0..10 {
            ws.domains.push("figma.com".to_string());
        }

        let mut evidence_set = crate::context_engine::evidence::ContextEvidenceSet::new(item_id);
        evidence_set.extend(evidences);

        let scorer = ContextEvidenceScorer::default();
        let score_result = scorer.score_evidence_against_workspace(&evidence_set, &ws);

        assert!(
            score_result.total_score <= scorer.weights.weak_signals_isolated_cap,
            "Score must not exceed weak signals isolated cap of {} (actual: {})",
            scorer.weights.weak_signals_isolated_cap,
            score_result.total_score
        );
        assert!(score_result.confidence_band == "Low" || score_result.confidence_band == "Very Low");
        assert!(score_result.uncertainty_indicators.iter().any(|u| u.contains("weak peripheral signals")));
    }
}

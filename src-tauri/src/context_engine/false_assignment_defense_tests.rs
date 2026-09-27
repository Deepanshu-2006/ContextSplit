// src-tauri/src/context_engine/false_assignment_defense_tests.rs
//
// Regression tests for reducing false context assignments across:
// 1. Weak evidence treated as strong (isolated weak signals capped)
// 2. Stale context (temporal decay reducing stale session/history affinity)
// 3. Application over-weighting (uncorroborated common app identity blocked)
// 4. Recent activity over-weighting (continuity cannot override conflicting evidence)
// 5. Memory leakage (cross-workspace memory isolation)
// 6. Incorrect session association (contamination guard on foreign domain/file)
// 7. Feedback application (respecting user dismissals and negative feedback)
// 8. Insufficient ambiguity handling (quarantine when margin < 0.15)

#[cfg(test)]
mod tests {
    use crate::context_engine::contamination::{ContaminationCheckInput, ContaminationGuard};
    use crate::context_engine::evidence::{ContextEvidence, ContextEvidenceSet};
    use crate::context_engine::pending_activity::{
        ActivityEvidenceSignal, ContextualActivityStatus, PendingActivityConfig,
        PendingActivityManager, PendingContextualActivity,
    };
    use crate::context_engine::quality_guard::{
        ContextQualityGuard, QualityGuardDecision, QualityGuardInput,
    };
    use crate::context_engine::scoring::{
        ContextEvidenceScorer, ScoringWeights, WorkspaceScoringContext,
    };

    // -------------------------------------------------------------------------
    // 1. Weak Evidence Treated As Strong Defense
    // -------------------------------------------------------------------------
    #[test]
    fn test_weak_evidence_cannot_produce_high_confidence_assignment() {
        let ws = WorkspaceScoringContext::new("ws_main", "Main Project");
        let scorer = ContextEvidenceScorer::default();

        // Single isolated weak app evidence
        let mut ev_set = ContextEvidenceSet::new("item_weak");
        ev_set.add(ContextEvidence::observed_app("item_weak", "Code", None));

        let score_res = scorer.score_evidence_against_workspace(&ev_set, &ws);
        assert!(score_res.total_score <= 0.24);
        assert!(score_res.confidence < ScoringWeights::default().threshold_medium);
        assert_ne!(score_res.confidence_band, "High");
    }

    // -------------------------------------------------------------------------
    // 2. Stale Context Defense (Temporal Decay)
    // -------------------------------------------------------------------------
    #[test]
    fn test_stale_context_history_decays_over_time() {
        let ws = WorkspaceScoringContext::new("ws_old", "Old Project");
        let scorer = ContextEvidenceScorer::default();

        // Fresh history evidence (decay weight = 1.0)
        let mut fresh_set = ContextEvidenceSet::new("item_fresh");
        fresh_set.add(ContextEvidence::inferred_workspace_history(
            "item_fresh",
            "ws_old",
            "Old Project",
            1.0,
            None,
        ));
        let fresh_res = scorer.score_evidence_against_workspace(&fresh_set, &ws);

        // Stale history evidence after 30 days (decay weight = 0.15)
        let mut stale_set = ContextEvidenceSet::new("item_stale");
        stale_set.add(ContextEvidence::inferred_workspace_history(
            "item_stale",
            "ws_old",
            "Old Project",
            0.15,
            None,
        ));
        let stale_res = scorer.score_evidence_against_workspace(&stale_set, &ws);

        assert!(fresh_res.total_score > stale_res.total_score);
        assert!(stale_res.total_score <= 0.10);
    }

    // -------------------------------------------------------------------------
    // 3. Application Over-Weighting Defense (Uncorroborated App)
    // -------------------------------------------------------------------------
    #[test]
    fn test_uncorroborated_common_app_held_in_inbox() {
        let guard = ContextQualityGuard::default();
        let mut input = QualityGuardInput::new("Chrome Browser Window")
            .with_app_name("Google Chrome")
            .with_target("ws_1", "ContextSplit");
        input.is_uncorroborated_app_only = true;

        let result = guard.evaluate(&input);
        assert_eq!(result.decision, QualityGuardDecision::HoldForMoreEvidence);
        assert!(!result.decision.should_modify_context());
        assert!(result.composite_quality_score <= 0.20);
    }

    // -------------------------------------------------------------------------
    // 4. Recent Activity Over-Weighting Defense (Conflicting Domain)
    // -------------------------------------------------------------------------
    #[test]
    fn test_recent_continuity_cannot_override_severe_context_conflict() {
        let guard = ContextQualityGuard::default();
        let input = QualityGuardInput::new("Stripe Payment Webhook")
            .with_app_name("Google Chrome")
            .with_domain("dashboard.stripe.com")
            .with_target("ws_uni", "University CS4500")
            .with_continuity(0.80, 0.70) // High recent continuity in University
            .with_conflict(0.90); // But domain clearly belongs to PayX/financial

        let result = guard.evaluate(&input);
        assert_eq!(result.decision, QualityGuardDecision::Reject);
        assert!(!result.decision.should_modify_context());
    }

    // -------------------------------------------------------------------------
    // 5. Memory Leakage Defense (Cross-Workspace Isolation)
    // -------------------------------------------------------------------------
    #[test]
    fn test_foreign_workspace_item_relationships_do_not_score_target_workspace() {
        let ws_target = WorkspaceScoringContext {
            id: "ws_target".to_string(),
            name: "Target Context".to_string(),
            item_ids: vec!["item_target_1".to_string()],
            ..WorkspaceScoringContext::default()
        };
        let scorer = ContextEvidenceScorer::default();

        // Evidence connecting to an item in a completely different workspace
        let mut ev_set = ContextEvidenceSet::new("item_test");
        ev_set.add(ContextEvidence::inferred_related_item(
            "item_test",
            "item_foreign_99",
            "references",
            0.95,
            Some("Foreign Context Document"),
            None,
        ));

        let res = scorer.score_evidence_against_workspace(&ev_set, &ws_target);
        // Foreign relationship must NOT contribute any score to ws_target
        assert_eq!(res.total_score, 0.0);
        assert_eq!(res.confidence, 0.0);
    }

    // -------------------------------------------------------------------------
    // 6. Incorrect Session Association Defense (Contamination Guard)
    // -------------------------------------------------------------------------
    #[test]
    fn test_contamination_guard_rejects_foreign_domain_in_active_session() {
        let c_guard = ContaminationGuard::default();

        let input = ContaminationCheckInput::new(
            "YouTube - Lofi Hip Hop Radio",
            "ws_contextsplit",
            "ContextSplit Development",
        )
        .with_app("Google Chrome")
        .with_domain("youtube.com");

        let res = c_guard.assess(&input);
        assert!(res.is_contamination_risk);
        assert_eq!(res.category.map(|c| c.as_str()), Some("entertainment_media"));
    }

    // -------------------------------------------------------------------------
    // 7. Feedback Application Defense (User Dismissal Respected)
    // -------------------------------------------------------------------------
    #[test]
    fn test_user_dismissal_suppresses_context_assignment() {
        let guard = ContextQualityGuard::default();

        let mut input = QualityGuardInput::new("Generic Tool Script")
            .with_target("ws_cs", "ContextSplit")
            .with_semantic_relevance(0.50);
        input.user_feedback = -0.80; // User explicitly dismissed this association

        let result = guard.evaluate(&input);
        assert_eq!(result.decision, QualityGuardDecision::Reject);
        assert!(!result.decision.should_modify_context());
    }

    // -------------------------------------------------------------------------
    // 8. Insufficient Ambiguity Handling Defense (Quarantine on Narrow Margin)
    // -------------------------------------------------------------------------
    #[test]
    fn test_narrow_score_margin_quarantines_into_ambiguous_status() {
        let config = PendingActivityConfig {
            min_score_margin: 0.15,
            ambiguity_threshold: 0.35,
            ..PendingActivityConfig::default()
        };
        let mut manager = PendingActivityManager::new(config);

        let mut act = PendingContextualActivity::new("act_amb", "Makefile")
            .with_candidates(vec!["ws_a".to_string(), "ws_b".to_string()]);

        // Close competing scores (0.60 vs 0.55 -> margin 0.05 < 0.15)
        act.add_signal(ActivityEvidenceSignal::new(
            "keyword",
            Some("ws_a".to_string()),
            0.60,
            1.0,
            false,
            "Keyword match in Workspace A",
        ));
        act.add_signal(ActivityEvidenceSignal::new(
            "keyword",
            Some("ws_b".to_string()),
            0.55,
            1.0,
            false,
            "Keyword match in Workspace B",
        ));

        let (ingested, _) = manager.ingest_activity(act);
        assert_eq!(ingested.status, ContextualActivityStatus::Ambiguous);
        assert_eq!(ingested.assigned_workspace_id, None);
    }
}

// src-tauri/src/context_engine/ambiguous_context_validation_tests.rs
//
// Regression tests for ambiguous context handling:
// - Ambiguity detection when multiple contexts are genuinely plausible
// - Candidate presentation and affinity score preservation
// - No destructive automatic assignment
// - Preservation of the item
// - Later reassessment when stronger evidence appears
// - Invariant: Never artificially increase ambiguity when decisive evidence exists

#[cfg(test)]
mod tests {
    use crate::context_engine::ambiguity::{
        AmbiguityConfig, AmbiguityDecision, AmbiguityEvaluationInput, AmbiguityManager,
        AmbiguityStatus, CandidateContextAffinity, NewEvidenceInput,
    };

    #[test]
    fn test_google_docs_competing_between_university_and_contextsplit_held_ambiguous() {
        let manager = AmbiguityManager::new();

        // Scenario: User opens Google Docs "Project Proposal" while both University and ContextSplit are active
        let input = AmbiguityEvaluationInput::new("act_gdocs_1", "Project Proposal Draft - Google Docs")
            .with_app("Google Chrome")
            .with_domain("docs.google.com")
            .with_url("https://docs.google.com/document/d/12345/edit")
            .with_candidates(vec![
                CandidateContextAffinity::new("ws_uni", "University CS4500", 0.65, 0.60)
                    .with_signals(
                        vec!["Recent university session".to_string(), "Document topic overlap".to_string()],
                        vec!["No course syllabus reference".to_string()],
                    ),
                CandidateContextAffinity::new("ws_cs", "ContextSplit Development", 0.62, 0.58)
                    .with_signals(
                        vec!["Active developer window".to_string(), "Product proposal keyword".to_string()],
                        vec!["No GitHub repo link".to_string()],
                    ),
            ]);

        let result = manager.evaluate(&input);

        // 1. Ambiguity detection verified
        assert!(result.is_ambiguous);
        assert_eq!(result.competing_candidates_count, 2);
        assert!((result.score_margin - 0.03).abs() < 1e-4);

        // 2. Candidate presentation preserved
        match &result.decision {
            AmbiguityDecision::HoldAmbiguous {
                candidate_workspace_ids,
                score_margin,
                explanation,
            } => {
                assert_eq!(candidate_workspace_ids.len(), 2);
                assert!(candidate_workspace_ids.contains(&"ws_uni".to_string()));
                assert!(candidate_workspace_ids.contains(&"ws_cs".to_string()));
                assert!(*score_margin < 0.15);
                assert!(explanation.contains("University CS4500"));
                assert!(explanation.contains("ContextSplit Development"));
            }
            _ => panic!("Expected HoldAmbiguous decision, got {:?}", result.decision),
        }

        // 3. No destructive automatic assignment verified
        assert_eq!(result.record.status, AmbiguityStatus::PendingMoreEvidence);
        assert_eq!(result.record.resolved_workspace_id, None);

        // 4. Preservation of the item record verified
        assert_eq!(result.record.activity_id, "act_gdocs_1");
        assert_eq!(result.record.title, "Project Proposal Draft - Google Docs");
        assert_eq!(result.record.domain.as_deref(), Some("docs.google.com"));
        assert_eq!(result.record.candidates.len(), 2);
    }

    #[test]
    fn test_ambiguous_activity_resolved_when_stronger_evidence_arrives() {
        let manager = AmbiguityManager::new();

        // 1. Initial ambiguous state
        let input = AmbiguityEvaluationInput::new("act_doc_2", "Requirements Document")
            .with_app("Google Chrome")
            .with_domain("docs.google.com")
            .with_candidates(vec![
                CandidateContextAffinity::new("ws_uni", "University CS4500", 0.60, 0.55),
                CandidateContextAffinity::new("ws_cs", "ContextSplit Development", 0.58, 0.55),
            ]);

        let initial_result = manager.evaluate(&input);
        assert!(initial_result.is_ambiguous);
        assert_eq!(initial_result.record.status, AmbiguityStatus::PendingMoreEvidence);

        // 2. Later reassessment: User explicitly confirms University workspace
        let mut record = initial_result.record;
        let new_evidence = NewEvidenceInput::empty().with_user_workspace("ws_uni");

        let res_result = manager.re_evaluate_with_new_evidence(&mut record, &new_evidence);
        assert_eq!(res_result.record.status, AmbiguityStatus::ExplicitlyAssigned);
        assert_eq!(res_result.record.resolved_workspace_id.as_deref(), Some("ws_uni"));
    }

    #[test]
    fn test_ambiguous_activity_resolved_when_course_url_arrives() {
        let manager = AmbiguityManager::new();

        // 1. Initial ambiguous state
        let input = AmbiguityEvaluationInput::new("act_doc_3", "Lab Assignment")
            .with_app("Google Chrome")
            .with_candidates(vec![
                CandidateContextAffinity::new("ws_uni", "University CS4500", 0.55, 0.50),
                CandidateContextAffinity::new("ws_cs", "ContextSplit Development", 0.50, 0.50),
            ]);

        let initial_result = manager.evaluate(&input);
        assert!(initial_result.is_ambiguous);

        // 2. Later reassessment: Submitting to Canvas LMS URL arrives
        let mut record = initial_result.record;
        let new_evidence = NewEvidenceInput::empty()
            .with_url("https://canvas.university.edu/courses/4500/assignment3");

        let res_result = manager.re_evaluate_with_new_evidence(&mut record, &new_evidence);
        assert_eq!(res_result.record.status, AmbiguityStatus::Resolved);
        assert_eq!(res_result.record.resolved_workspace_id.as_deref(), Some("ws_uni"));
    }

    #[test]
    fn test_decisive_evidence_does_not_artificially_trigger_ambiguity() {
        let manager = AmbiguityManager::new();

        // Scenario: Activity has a decisive project repository match for PayX
        let input = AmbiguityEvaluationInput::new("act_clear_1", "PaymentGateway.swift — PayX Core")
            .with_app("Xcode")
            .with_file_path("/Users/deepanshukhatri/Work/PayX/Sources/PaymentGateway.swift")
            .with_candidates(vec![
                CandidateContextAffinity::new("ws_payx", "PayX Development", 0.95, 0.90)
                    .with_signals(
                        vec!["Project directory match".to_string(), "Swift module structure".to_string()],
                        vec![],
                    ),
                CandidateContextAffinity::new("ws_cs", "ContextSplit Development", 0.10, 0.10)
                    .with_signals(vec![], vec!["Mismatched workspace root".to_string()]),
            ]);

        let result = manager.evaluate(&input);

        // Decisive margin (0.95 - 0.10 = 0.85 >> 0.15) must NOT be marked ambiguous
        assert!(!result.is_ambiguous);
        assert_eq!(result.record.status, AmbiguityStatus::Resolved);
        assert_eq!(result.record.resolved_workspace_id.as_deref(), Some("ws_payx"));
        match result.decision {
            AmbiguityDecision::DecisiveAssign { workspace_id, confidence, .. } => {
                assert_eq!(workspace_id, "ws_payx");
                assert!(confidence >= 0.85);
            }
            _ => panic!("Expected DecisiveAssign decision"),
        }
    }

    #[test]
    fn test_uniformly_weak_evidence_held_as_weak_without_false_assignment() {
        let manager = AmbiguityManager::new();

        // Scenario: Activity has only weak ungrounded keywords (score 0.10)
        let input = AmbiguityEvaluationInput::new("act_weak_1", "Random Notes")
            .with_candidates(vec![
                CandidateContextAffinity::new("ws_uni", "University CS4500", 0.12, 0.10),
                CandidateContextAffinity::new("ws_cs", "ContextSplit Development", 0.10, 0.10),
            ]);

        let result = manager.evaluate(&input);

        // Scores below min_score_for_competition (0.20) must be held as weak evidence
        assert!(!result.is_ambiguous);
        assert_eq!(result.record.resolved_workspace_id, None);
        match result.decision {
            AmbiguityDecision::HoldWeakEvidence { .. } => (),
            _ => panic!("Expected HoldWeakEvidence decision"),
        }
    }
}

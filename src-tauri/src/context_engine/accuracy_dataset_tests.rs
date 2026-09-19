use crate::context_engine::contamination::{ContaminationCheckInput, ContaminationGuard};
use crate::context_engine::pending_activity::{
    ActivityEvidenceSignal, ContextualActivityStatus, PendingActivityConfig,
    PendingActivityManager, PendingContextualActivity,
};
use crate::context_engine::quality_guard::{ContextQualityGuard, QualityGuardInput};
use crate::context_engine::transitions::{
    classify_transition_evidence, TransitionEvidenceType,
};
use serde_json::Value;

const DATASET_JSON: &str = include_str!("../../../tests/context-accuracy/dataset.json");

#[test]
fn test_dataset_json_validity_and_coverage() {
    let root: Value = serde_json::from_str(DATASET_JSON)
        .expect("dataset.json must be valid, well-formed JSON");

    let contexts = root.get("contexts").and_then(|v| v.as_array()).expect("contexts array");
    assert_eq!(contexts.len(), 5, "Dataset must contain exactly 5 standard contexts");

    let context_ids: Vec<&str> = contexts
        .iter()
        .filter_map(|c| c.get("id").and_then(|id| id.as_str()))
        .collect();

    assert!(context_ids.contains(&"ws_payx"));
    assert!(context_ids.contains(&"ws_contextsplit"));
    assert!(context_ids.contains(&"ws_swiftsend"));
    assert!(context_ids.contains(&"ws_university_os"));
    assert!(context_ids.contains(&"ws_tokyo_trip"));

    let cases = root.get("cases").and_then(|v| v.as_array()).expect("cases array");
    assert!(cases.len() >= 15, "Dataset must contain comprehensive test cases");

    // Check category presence
    let categories: Vec<&str> = cases
        .iter()
        .filter_map(|c| c.get("category").and_then(|cat| cat.as_str()))
        .collect();

    assert!(categories.contains(&"clean"), "Must contain clean cases");
    assert!(categories.contains(&"noisy"), "Must contain noisy cases");
    assert!(categories.contains(&"ambiguous"), "Must contain ambiguous cases");
    assert!(categories.contains(&"misleading_application"), "Must contain misleading application cases");
    assert!(categories.contains(&"duplicate"), "Must contain duplicate cases");
    assert!(categories.contains(&"context_switch"), "Must contain context switch cases");
    assert!(categories.contains(&"delayed_evidence"), "Must contain delayed evidence cases");
}

#[test]
fn test_dataset_clean_and_misleading_cases_evaluation() {
    let root: Value = serde_json::from_str(DATASET_JSON).unwrap();
    let cases = root.get("cases").and_then(|v| v.as_array()).unwrap();

    let guard = ContextQualityGuard::default();
    let mut pending_mgr = PendingActivityManager::new(PendingActivityConfig::default());

    for case_val in cases {
        let category = case_val.get("category").and_then(|v| v.as_str()).unwrap_or_default();
        if category != "clean" && category != "misleading_application" {
            continue;
        }

        let id = case_val.get("id").and_then(|v| v.as_str()).unwrap();
        let act = case_val.get("activity").unwrap();
        let title = act.get("title").and_then(|v| v.as_str()).unwrap();
        let app_name = act.get("app_name").and_then(|v| v.as_str());
        let file_path = act.get("file_path").and_then(|v| v.as_str());
        let url = act.get("url").and_then(|v| v.as_str());
        let domain = act.get("domain").and_then(|v| v.as_str());

        let expected = case_val.get("expected").unwrap();
        let exp_ws = expected.get("workspace_id").and_then(|v| v.as_str()).unwrap();

        // 1. Evaluate via PendingActivityManager
        let mut p_act = PendingContextualActivity::new(id, title)
            .with_url_and_domain(url.map(|s| s.to_string()), domain.map(|s| s.to_string()));
        if let Some(app) = app_name {
            p_act = p_act.with_app_name(app);
        }
        if let Some(fp) = file_path {
            p_act = p_act.with_file_path(fp);
            p_act.add_signal(ActivityEvidenceSignal::file_path_match(exp_ws, fp));
        } else if let Some(u) = url {
            p_act.add_signal(ActivityEvidenceSignal::new(
                "url_domain_match",
                Some(exp_ws.to_string()),
                0.90,
                1.4,
                true,
                format!("URL match for {}", u),
            ));
        }

        let (ingested, _) = pending_mgr.ingest_activity(p_act);
        assert_eq!(
            ingested.status,
            ContextualActivityStatus::Assigned,
            "Case '{}' should be decisively assigned",
            id
        );
        assert_eq!(
            ingested.assigned_workspace_id.as_deref(),
            Some(exp_ws),
            "Case '{}' target workspace mismatch",
            id
        );

        // 2. Evaluate via QualityGuardInput
        let mut q_input = QualityGuardInput::new(title)
            .with_target(exp_ws, exp_ws)
            .with_context_confidence(0.90)
            .with_semantic_relevance(0.85)
            .with_task_relevance(0.80)
            .with_continuity(0.75, 0.85)
            .with_item_relationships(0.70);
        if let Some(app) = app_name {
            q_input = q_input.with_app_name(app);
        }
        if let Some(d) = domain {
            q_input = q_input.with_domain(d).with_domain_relevance(0.80);
        }
        let q_res = guard.evaluate(&q_input);
        assert!(
            q_res.decision.should_modify_context(),
            "Case '{}' should permit modifying context in quality guard (got decision {:?})",
            id,
            q_res.decision
        );
    }
}

#[test]
fn test_dataset_noisy_cases_suppression() {
    let root: Value = serde_json::from_str(DATASET_JSON).unwrap();
    let cases = root.get("cases").and_then(|v| v.as_array()).unwrap();

    let guard = ContextQualityGuard::default();
    let c_guard = ContaminationGuard::new();

    for case_val in cases {
        let category = case_val.get("category").and_then(|v| v.as_str()).unwrap_or_default();
        if category != "noisy" {
            continue;
        }

        let id = case_val.get("id").and_then(|v| v.as_str()).unwrap();
        let act = case_val.get("activity").unwrap();
        let title = act.get("title").and_then(|v| v.as_str()).unwrap();
        let app_name = act.get("app_name").and_then(|v| v.as_str());
        let domain = act.get("domain").and_then(|v| v.as_str());
        let url = act.get("url").and_then(|v| v.as_str());

        // Contamination check against PayX
        let mut c_input = ContaminationCheckInput::new(title, "ws_payx", "PayX");
        if let Some(app) = app_name {
            c_input = c_input.with_app(app);
        }
        if let Some(d) = domain {
            c_input = c_input.with_domain(d);
        }
        if let Some(u) = url {
            c_input = c_input.with_url(u);
        }
        let c_result = c_guard.assess(&c_input);

        assert!(
            c_result.is_contamination_risk,
            "Noisy case '{}' ({}) must be detected as contaminated",
            id, title
        );

        // Quality guard check against PayX
        let mut q_input = QualityGuardInput::new(title)
            .with_target("ws_payx", "PayX");
        if let Some(d) = domain {
            q_input = q_input.with_domain(d);
        }
        if let Some(app) = app_name {
            q_input = q_input.with_app_name(app);
        }
        let q_res = guard.evaluate(&q_input);
        assert!(
            !q_res.decision.should_modify_context(),
            "Noisy case '{}' must NOT modify context in quality guard",
            id
        );
    }
}

#[test]
fn test_dataset_ambiguous_cases_held() {
    let root: Value = serde_json::from_str(DATASET_JSON).unwrap();
    let cases = root.get("cases").and_then(|v| v.as_array()).unwrap();

    let mut pending_mgr = PendingActivityManager::new(PendingActivityConfig::default());

    for case_val in cases {
        let category = case_val.get("category").and_then(|v| v.as_str()).unwrap_or_default();
        if category != "ambiguous" {
            continue;
        }

        let id = case_val.get("id").and_then(|v| v.as_str()).unwrap();
        let act = case_val.get("activity").unwrap();
        let title = act.get("title").and_then(|v| v.as_str()).unwrap();
        let app_name = act.get("app_name").and_then(|v| v.as_str());
        let domain = act.get("domain").and_then(|v| v.as_str());

        let mut p_act = PendingContextualActivity::new(id, title)
            .with_url_and_domain(None, domain.map(|s| s.to_string()));
        if let Some(app) = app_name {
            p_act = p_act.with_app_name(app);
            p_act.add_signal(ActivityEvidenceSignal::weak_app_signal(app));
        }

        if domain == Some("github.com") {
            // Split domain evidence between PayX and ContextSplit
            p_act.add_signal(ActivityEvidenceSignal::new("domain", Some("ws_payx".to_string()), 0.5, 0.8, false, "GitHub"));
            p_act.add_signal(ActivityEvidenceSignal::new("domain", Some("ws_contextsplit".to_string()), 0.5, 0.8, false, "GitHub"));
        }

        let (ingested, _) = pending_mgr.ingest_activity(p_act);
        assert!(
            ingested.status.is_pending() || ingested.status.is_ambiguous(),
            "Ambiguous case '{}' must be pending or ambiguous, got '{}'",
            id,
            ingested.status
        );
        assert!(
            !ingested.status.can_modify_context(),
            "Ambiguous case '{}' must NOT modify context",
            id
        );
    }
}

#[test]
fn test_dataset_delayed_evidence_resolution() {
    let root: Value = serde_json::from_str(DATASET_JSON).unwrap();
    let cases = root.get("cases").and_then(|v| v.as_array()).unwrap();

    let mut pending_mgr = PendingActivityManager::new(PendingActivityConfig::default());

    for case_val in cases {
        let category = case_val.get("category").and_then(|v| v.as_str()).unwrap_or_default();
        if category != "delayed_evidence" {
            continue;
        }

        let e_init = case_val.get("event_initial").unwrap();
        let init_id = e_init.get("id").and_then(|v| v.as_str()).unwrap();
        let init_title = e_init.get("title").and_then(|v| v.as_str()).unwrap();
        let app = e_init.get("app_name").and_then(|v| v.as_str()).unwrap();
        let domain = e_init.get("domain").and_then(|v| v.as_str()).unwrap();

        let mut event1 = PendingContextualActivity::new(init_id, init_title)
            .with_app_name(app)
            .with_url_and_domain(Some(format!("https://{}", domain)), Some(domain.to_string()));
        event1.add_signal(ActivityEvidenceSignal::new("domain", Some("ws_payx".to_string()), 0.5, 0.8, false, "GitHub"));
        event1.add_signal(ActivityEvidenceSignal::new("domain", Some("ws_swiftsend".to_string()), 0.5, 0.8, false, "GitHub"));

        let (e1_ingested, _) = pending_mgr.ingest_activity(event1);
        assert_eq!(e1_ingested.status, ContextualActivityStatus::Ambiguous);

        // Decisive event arrives
        let e_dec = case_val.get("event_decisive").unwrap();
        let dec_id = e_dec.get("id").and_then(|v| v.as_str()).unwrap();
        let dec_title = e_dec.get("title").and_then(|v| v.as_str()).unwrap();
        let dec_ws = e_dec.get("workspace_id").and_then(|v| v.as_str()).unwrap();
        let dec_url = e_dec.get("url").and_then(|v| v.as_str()).unwrap();

        let mut event2 = PendingContextualActivity::new(dec_id, dec_title)
            .with_app_name(app)
            .with_url_and_domain(Some(dec_url.to_string()), Some(domain.to_string()));
        event2.add_signal(ActivityEvidenceSignal::project_repo_match(dec_ws, "SwiftSend/swiftsend-core"));

        let (e2_ingested, resolutions) = pending_mgr.ingest_activity(event2);
        assert_eq!(e2_ingested.status, ContextualActivityStatus::Assigned);
        assert_eq!(resolutions.len(), 1);
        assert_eq!(resolutions[0].activity_id, init_id);
        assert_eq!(resolutions[0].assigned_workspace_id.as_deref(), Some(dec_ws));

        let e1_after = pending_mgr.get_activity(init_id).unwrap();
        assert_eq!(e1_after.status, ContextualActivityStatus::Assigned);
        assert_eq!(e1_after.assigned_workspace_id.as_deref(), Some(dec_ws));
    }
}

#[test]
fn test_dataset_context_switch_sequence() {
    let ev_analysis = classify_transition_evidence(
        false,
        0.90,
        0.85,
        0.35,
        3,
        2,
        &[
            "repository: ContextSplit/contextsplit-desktop".to_string(),
            "project path: /Projects/ContextSplit/src-tauri".to_string(),
            "associated task: Enhance contextual transition detection".to_string(),
        ],
    );

    assert!(
        ev_analysis.evidence_types.contains(&TransitionEvidenceType::ProjectOrRepoChange),
        "Must classify repository switch"
    );
    assert!(
        ev_analysis.evidence_types.contains(&TransitionEvidenceType::FilePathChange),
        "Must classify file path switch"
    );
    assert!(
        ev_analysis.evidence_types.contains(&TransitionEvidenceType::TaskTransition),
        "Must classify task transition"
    );
}

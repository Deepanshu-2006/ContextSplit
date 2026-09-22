use chrono::{Duration, Utc};
use rusqlite::Connection;
use serde_json::json;

use crate::context_engine::feedback::{
    AdaptationVerdict, ContextDeterministicAdaptationEngine, ContextFeedbackEvent,
    ContextFeedbackInspector, ContextFeedbackService, FeedbackAction, FeedbackScope,
    FeedbackSource, FeedbackTargetRef,
};
use crate::context_engine::lifecycle::{ContextLifecycleService, ContextLifecycleState};
use crate::db::operations::{create_item, create_session, create_workspace};

fn setup_simulation_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    crate::db::run_migrations(&conn).unwrap();
    conn
}

// =========================================================================
// SCENARIO 1
// PayX context.
// Random article incorrectly assigned.
// User removes it.
// Expected: Future similar association is discouraged.
// =========================================================================
#[test]
fn test_scenario_1_random_article_removal_discourages_future_similar_associations() {
    let conn = setup_simulation_db();
    let payx = create_workspace(&conn, "PayX").unwrap();
    let session = create_session(&conn).unwrap();

    // 1. Random generic article incorrectly filed in PayX
    let item_article = create_item(
        &conn,
        Some(&payx.id),
        Some(&session.id),
        "url",
        "https://medium.com/@dev/10-productivity-tips-for-developers",
        Some("10 Productivity Tips for Developers - Medium"),
        &json!({
            "app_name": "Google Chrome",
            "domain": "medium.com",
            "url": "https://medium.com/@dev/10-productivity-tips-for-developers"
        })
        .to_string(),
    )
    .unwrap();

    // 2. User explicitly removes the random article from PayX
    let ev = ContextFeedbackEvent::remove_from_context(
        &item_article.id,
        &payx.id,
        FeedbackSource::UserExplicitUi,
        Some("Incorrectly assigned generic blog post".to_string()),
    );
    ContextFeedbackService::apply_feedback(&conn, &ev).unwrap();

    // 3. Evaluate adaptation for a future similar generic article on the same domain
    let future_article = create_item(
        &conn,
        None,
        Some(&session.id),
        "url",
        "https://medium.com/@writer/another-unrelated-post",
        Some("Another Unrelated Post - Medium"),
        &json!({
            "app_name": "Google Chrome",
            "domain": "medium.com",
            "url": "https://medium.com/@writer/another-unrelated-post"
        })
        .to_string(),
    )
    .unwrap();

    let assessment = ContextDeterministicAdaptationEngine::evaluate_adaptation(
        &conn,
        &future_article,
        &payx,
        0.70,
    );

    // Future similar association must be discouraged/dampened
    assert!(
        assessment.verdict == AdaptationVerdict::GenericAppDampened
            || assessment.verdict == AdaptationVerdict::ItemDirectlyRejected
            || assessment.is_generic_suppressed
            || assessment.adapted_score < assessment.base_score,
        "Future generic medium.com article must be discouraged for PayX after user rejection"
    );
    assert!(assessment.app_rejection_count >= 1 || assessment.item_rejection_count >= 1);
}

// =========================================================================
// SCENARIO 2
// Same article later explicitly confirmed by user.
// Expected: Explicit confirmation overrides older rejection.
// =========================================================================
#[test]
fn test_scenario_2_explicit_confirmation_overrides_older_rejection() {
    let conn = setup_simulation_db();
    let payx = create_workspace(&conn, "PayX").unwrap();
    let session = create_session(&conn).unwrap();

    let item = create_item(
        &conn,
        None,
        Some(&session.id),
        "url",
        "https://payx-payments.com/docs/api-reference",
        Some("PayX API Reference Documentation"),
        &json!({
            "app_name": "Google Chrome",
            "domain": "payx-payments.com",
            "url": "https://payx-payments.com/docs/api-reference"
        })
        .to_string(),
    )
    .unwrap();

    // 1. Initial rejection 2 hours ago (e.g. accidental dismissal)
    let rejection_ev = ContextFeedbackEvent::remove_from_context(
        &item.id,
        &payx.id,
        FeedbackSource::UserExplicitUi,
        None,
    )
    .with_timestamp((Utc::now() - Duration::hours(2)).to_rfc3339());
    ContextFeedbackService::apply_feedback(&conn, &rejection_ev).unwrap();

    // 2. Later explicit confirmation 5 minutes ago
    let confirmation_ev = ContextFeedbackEvent::confirm_context(
        &item.id,
        &payx.id,
        FeedbackSource::UserExplicitUi,
    )
    .with_timestamp((Utc::now() - Duration::minutes(5)).to_rfc3339());
    ContextFeedbackService::apply_feedback(&conn, &confirmation_ev).unwrap();

    // 3. Inspect target state
    let report = ContextFeedbackInspector::inspect_target(&conn, &item.id, Some(&payx.id)).unwrap();

    // Explicit confirmation must override older rejection
    assert_eq!(report.effective_state, "Confirmed");
    assert!(report.history_entries.len() >= 2);
    assert_eq!(report.competing_conflicts.len(), 1);
    assert_eq!(report.competing_conflicts[0].action, "remove_from_context");

    // Adaptation engine reflects confirmed directly
    let assessment = ContextDeterministicAdaptationEngine::evaluate_adaptation(&conn, &item, &payx, 0.50);
    assert_eq!(assessment.verdict, AdaptationVerdict::ConfirmedDirectly);
    assert!(assessment.adapted_score >= 0.95);
}

// =========================================================================
// SCENARIO 3
// VS Code incorrectly associated with PayX.
// User corrects it.
// Expected: VS Code itself is NOT globally rejected.
// =========================================================================
#[test]
fn test_scenario_3_vscode_correction_does_not_globally_reject_vscode() {
    let conn = setup_simulation_db();
    let payx = create_workspace(&conn, "PayX").unwrap();
    let personal_project = create_workspace(&conn, "Personal Blog").unwrap();
    let session = create_session(&conn).unwrap();

    // VS Code item editing a blog post erroneously assigned to PayX
    let item_blog = create_item(
        &conn,
        Some(&payx.id),
        Some(&session.id),
        "window_focus",
        "VS Code - personal_blog/post.md",
        Some("VS Code - personal_blog/post.md"),
        &json!({
            "app_name": "Visual Studio Code",
            "file_path": "/Users/deepanshu/projects/personal_blog/post.md"
        })
        .to_string(),
    )
    .unwrap();

    // User corrects it by removing it from PayX
    let ev = ContextFeedbackEvent::remove_from_context(
        &item_blog.id,
        &payx.id,
        FeedbackSource::UserExplicitUi,
        Some("Not related to PayX".to_string()),
    );
    ContextFeedbackService::apply_feedback(&conn, &ev).unwrap();

    // Verify scope is narrow (item-context or workspace-specific)
    assert!(matches!(ev.scope, FeedbackScope::ItemContext { .. }));

    // Evaluate a new VS Code item in the context of Personal Blog
    let personal_item = create_item(
        &conn,
        Some(&personal_project.id),
        Some(&session.id),
        "window_focus",
        "VS Code - personal_blog/main.rs",
        Some("VS Code - personal_blog/main.rs"),
        &json!({
            "app_name": "Visual Studio Code",
            "file_path": "/Users/deepanshu/projects/personal_blog/main.rs"
        })
        .to_string(),
    )
    .unwrap();

    let personal_eval = ContextDeterministicAdaptationEngine::evaluate_adaptation(
        &conn,
        &personal_item,
        &personal_project,
        0.70,
    );

    // VS Code must NOT be globally rejected or dampened in other workspaces
    assert_ne!(
        personal_eval.verdict,
        AdaptationVerdict::ItemDirectlyRejected,
        "VS Code must not be rejected globally"
    );
    assert_ne!(
        personal_eval.verdict,
        AdaptationVerdict::GenericAppDampened,
        "VS Code must not be dampened in unrelated workspaces"
    );
    assert_eq!(personal_eval.app_rejection_count, 0);
}

// =========================================================================
// SCENARIO 4
// GitHub repository confirmed as PayX.
// Another unrelated GitHub repository appears.
// Expected: Only the relevant repository relationship is strengthened.
// =========================================================================
#[test]
fn test_scenario_4_github_repo_confirmation_only_strengthens_relevant_repo() {
    let conn = setup_simulation_db();
    let payx = create_workspace(&conn, "PayX").unwrap();
    let session = create_session(&conn).unwrap();

    // 1. PayX repository item confirmed
    let item_payx_repo = create_item(
        &conn,
        Some(&payx.id),
        Some(&session.id),
        "url",
        "https://github.com/payx-org/payx-backend",
        Some("payx-org/payx-backend - GitHub"),
        &json!({
            "app_name": "Google Chrome",
            "domain": "github.com",
            "url": "https://github.com/payx-org/payx-backend"
        })
        .to_string(),
    )
    .unwrap();

    let confirm_ev = ContextFeedbackEvent::confirm_context(
        &item_payx_repo.id,
        &payx.id,
        FeedbackSource::UserExplicitUi,
    );
    ContextFeedbackService::apply_feedback(&conn, &confirm_ev).unwrap();

    // 2. Future visit to the confirmed PayX repo
    let payx_repo_item = create_item(
        &conn,
        None,
        Some(&session.id),
        "url",
        "https://github.com/payx-org/payx-backend/pull/42",
        Some("PR 42 - payx-org/payx-backend - GitHub"),
        &json!({
            "app_name": "Google Chrome",
            "domain": "github.com",
            "url": "https://github.com/payx-org/payx-backend/pull/42"
        })
        .to_string(),
    )
    .unwrap();

    let payx_repo_eval = ContextDeterministicAdaptationEngine::evaluate_adaptation(
        &conn,
        &payx_repo_item,
        &payx,
        0.60,
    );

    // 3. Future visit to an UNRELATED GitHub repository
    let unrelated_repo_item = create_item(
        &conn,
        None,
        Some(&session.id),
        "url",
        "https://github.com/unrelated-org/analytics-engine",
        Some("unrelated-org/analytics-engine - GitHub"),
        &json!({
            "app_name": "Google Chrome",
            "domain": "github.com",
            "url": "https://github.com/unrelated-org/analytics-engine"
        })
        .to_string(),
    )
    .unwrap();

    let unrelated_repo_eval = ContextDeterministicAdaptationEngine::evaluate_adaptation(
        &conn,
        &unrelated_repo_item,
        &payx,
        0.60,
    );

    // PayX repo should be recognized / strengthened
    assert!(
        payx_repo_eval.verdict == AdaptationVerdict::ConfirmedDirectly
            || payx_repo_eval.verdict == AdaptationVerdict::SpecificRelationshipRecognized
            || payx_repo_eval.has_specific_entity_match
            || payx_repo_eval.adapted_score >= payx_repo_eval.base_score,
        "PayX repo relationship must be recognized and strengthened"
    );

    // Unrelated repo should NOT inherit specific repo confirmation boost
    assert!(
        payx_repo_eval.adapted_score >= unrelated_repo_eval.adapted_score,
        "Confirmed repo must have higher or equal score than an unrelated repo"
    );
}

// =========================================================================
// SCENARIO 5
// User marks Figma project important.
// Expected: Important memory becomes confirmed.
// =========================================================================
#[test]
fn test_scenario_5_user_marks_figma_project_important_confirms_memory() {
    let conn = setup_simulation_db();
    let payx = create_workspace(&conn, "PayX").unwrap();
    let session = create_session(&conn).unwrap();

    let figma_item = create_item(
        &conn,
        Some(&payx.id),
        Some(&session.id),
        "url",
        "https://figma.com/file/payx-checkout-v2",
        Some("PayX Checkout Flow V2 - Figma"),
        &json!({
            "app_name": "Figma",
            "domain": "figma.com",
            "url": "https://figma.com/file/payx-checkout-v2"
        })
        .to_string(),
    )
    .unwrap();

    // User explicitly marks it as important in PayX
    let ev = ContextFeedbackEvent::mark_important(
        FeedbackTargetRef::Item(figma_item.id.clone()),
        Some(payx.id.clone()),
        FeedbackSource::UserExplicitUi,
    );
    let result = ContextFeedbackService::apply_feedback(&conn, &ev).unwrap();

    assert!(result.success);
    assert_eq!(result.action, FeedbackAction::MarkImportant);

    // Inspector verification
    let report = ContextFeedbackInspector::inspect_target(&conn, &figma_item.id, Some(&payx.id)).unwrap();
    assert_eq!(report.effective_state, "Confirmed");
    assert!(report.history_entries.iter().any(|e| e.action == "mark_important"));
}

// =========================================================================
// SCENARIO 6
// User removes it from important memory.
// Expected: It does not immediately return because of weak inference.
// =========================================================================
#[test]
fn test_scenario_6_removal_from_important_memory_prevents_immediate_return() {
    let conn = setup_simulation_db();
    let payx = create_workspace(&conn, "PayX").unwrap();
    let session = create_session(&conn).unwrap();

    let item = create_item(
        &conn,
        Some(&payx.id),
        Some(&session.id),
        "url",
        "https://figma.com/file/payx-old-mockups",
        Some("PayX Old Mockups - Figma"),
        &json!({
            "app_name": "Figma",
            "domain": "figma.com",
            "url": "https://figma.com/file/payx-old-mockups"
        })
        .to_string(),
    )
    .unwrap();

    // 1. Initially marked important
    let ev_mark = ContextFeedbackEvent::mark_important(
        FeedbackTargetRef::Item(item.id.clone()),
        Some(payx.id.clone()),
        FeedbackSource::UserExplicitUi,
    )
    .with_timestamp((Utc::now() - Duration::hours(1)).to_rfc3339());
    ContextFeedbackService::apply_feedback(&conn, &ev_mark).unwrap();

    // 2. User removes importance status from the item
    let ev_demote = ContextFeedbackEvent::remove_importance(
        FeedbackTargetRef::Item(item.id.clone()),
        Some(payx.id.clone()),
        FeedbackSource::UserExplicitUi,
    )
    .with_timestamp(Utc::now().to_rfc3339());
    ContextFeedbackService::apply_feedback(&conn, &ev_demote).unwrap();

    // 3. Inspector verification
    let report = ContextFeedbackInspector::inspect_target(&conn, &item.id, Some(&payx.id)).unwrap();
    assert_eq!(report.effective_state, "Rejected");

    // Precedence ensures explicit demotion prevents immediate weak heuristic return
    assert!(report.history_entries.iter().any(|e| e.action == "remove_importance"));
}

// =========================================================================
// SCENARIO 7
// User corrects current task.
// Expected: Task reconstruction reflects the correction.
// =========================================================================
#[test]
fn test_scenario_7_user_corrects_current_task_reconstruction_reflects_correction() {
    let conn = setup_simulation_db();
    let payx = create_workspace(&conn, "PayX").unwrap();
    let session = create_session(&conn).unwrap();

    // Insert task candidate into DB
    let initial_task_id = "task_payx_stripe_initial";
    conn.execute(
        "INSERT INTO task_candidates (id, context_id, title, confidence, status, first_detected_at, last_active_at, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            initial_task_id,
            payx.id,
            "Generic Documentation Browsing",
            0.65,
            "detected",
            (Utc::now() - Duration::minutes(30)).to_rfc3339(),
            (Utc::now() - Duration::minutes(30)).to_rfc3339(),
            (Utc::now() - Duration::minutes(30)).to_rfc3339(),
            (Utc::now() - Duration::minutes(30)).to_rfc3339(),
        ],
    )
    .unwrap();

    // User rejects the inaccurate generic task
    let reject_ev = ContextFeedbackEvent::reject_task(
        initial_task_id,
        Some(payx.id.clone()),
        FeedbackSource::UserExplicitUi,
        Some("Inaccurate task name".to_string()),
    );
    ContextFeedbackService::apply_feedback(&conn, &reject_ev).unwrap();

    // User confirms the corrected specific task candidate
    let corrected_task_id = "task_payx_stripe_corrected";
    conn.execute(
        "INSERT INTO task_candidates (id, context_id, title, confidence, status, first_detected_at, last_active_at, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            corrected_task_id,
            payx.id,
            "PayX Stripe Webhook Migration & Testing",
            0.95,
            "detected",
            Utc::now().to_rfc3339(),
            Utc::now().to_rfc3339(),
            Utc::now().to_rfc3339(),
            Utc::now().to_rfc3339(),
        ],
    )
    .unwrap();

    let confirm_ev = ContextFeedbackEvent::confirm_task(
        corrected_task_id,
        &payx.id,
        FeedbackSource::UserExplicitUi,
    );
    ContextFeedbackService::apply_feedback(&conn, &confirm_ev).unwrap();

    // Verify task candidate in DB is confirmed
    let (status, title): (String, String) = conn
        .query_row(
            "SELECT status, title FROM task_candidates WHERE id = ?1",
            rusqlite::params![corrected_task_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();

    assert_eq!(status, "confirmed");
    assert_eq!(title, "PayX Stripe Webhook Migration & Testing");

    // Verify inspector reports confirmed state for the corrected task
    let report = ContextFeedbackInspector::inspect_target(&conn, corrected_task_id, Some(&payx.id)).unwrap();
    assert_eq!(report.effective_state, "Confirmed");
}

// =========================================================================
// SCENARIO 8
// User resumes dormant context.
// Expected: Lifecycle changes to active.
// =========================================================================
#[test]
fn test_scenario_8_user_resumes_dormant_context_changes_lifecycle_to_active() {
    let conn = setup_simulation_db();
    let payx = create_workspace(&conn, "PayX").unwrap();

    // User explicitly resumes the context
    let ev = ContextFeedbackEvent::resume_context(&payx.id, FeedbackSource::UserExplicitUi);
    ContextFeedbackService::apply_feedback(&conn, &ev).unwrap();

    // Context lifecycle evaluation must reflect Active
    let lifecycle = ContextLifecycleService::get_context_lifecycle(&conn, &payx.id).unwrap();
    assert_eq!(lifecycle.lifecycle_state, ContextLifecycleState::Active);
}

// =========================================================================
// SCENARIO 9
// User archives context.
// Expected: Context becomes archived without deleting history.
// =========================================================================
#[test]
fn test_scenario_9_user_archives_context_without_deleting_history() {
    let conn = setup_simulation_db();
    let payx = create_workspace(&conn, "PayX").unwrap();
    let session = create_session(&conn).unwrap();

    // Create several items in PayX
    for i in 1..=5 {
        create_item(
            &conn,
            Some(&payx.id),
            Some(&session.id),
            "window_focus",
            &format!("PayX Module {}", i),
            Some(&format!("PayX Module {}", i)),
            "{}",
        )
        .unwrap();
    }

    // User explicitly archives the context
    let ev = ContextFeedbackEvent::archive_context(
        &payx.id,
        FeedbackSource::UserExplicitUi,
        Some("Project milestone completed".to_string()),
    );
    ContextFeedbackService::apply_feedback(&conn, &ev).unwrap();

    // Context lifecycle must be Archived
    let lifecycle = ContextLifecycleService::get_context_lifecycle(&conn, &payx.id).unwrap();
    assert_eq!(lifecycle.lifecycle_state, ContextLifecycleState::Archived);

    // Historical items must NOT be deleted
    let item_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM items WHERE workspace_id = ?1",
            rusqlite::params![payx.id],
            |r| r.get(0),
        )
        .unwrap();

    assert_eq!(item_count, 5, "Archiving must preserve all historical context items intact");
}

// =========================================================================
// SCENARIO 10
// Feedback from ContextSplit.
// Expected: Does not affect PayX.
// =========================================================================
#[test]
fn test_scenario_10_contextsplit_feedback_does_not_affect_payx() {
    let conn = setup_simulation_db();
    let payx = create_workspace(&conn, "PayX").unwrap();
    let contextsplit = create_workspace(&conn, "ContextSplit").unwrap();
    let session = create_session(&conn).unwrap();

    // Generate feedback events exclusively within ContextSplit
    let cs_item_1 = create_item(
        &conn,
        Some(&contextsplit.id),
        Some(&session.id),
        "url",
        "https://tauri.app/docs",
        Some("Tauri Documentation"),
        "{}",
    )
    .unwrap();

    let cs_item_2 = create_item(
        &conn,
        Some(&contextsplit.id),
        Some(&session.id),
        "url",
        "https://rust-lang.org",
        Some("Rust Programming Language"),
        "{}",
    )
    .unwrap();

    // Reject in ContextSplit
    let ev1 = ContextFeedbackEvent::remove_from_context(
        &cs_item_1.id,
        &contextsplit.id,
        FeedbackSource::UserExplicitUi,
        None,
    );
    ContextFeedbackService::apply_feedback(&conn, &ev1).unwrap();

    // Confirm in ContextSplit
    let ev2 = ContextFeedbackEvent::confirm_context(
        &cs_item_2.id,
        &contextsplit.id,
        FeedbackSource::UserExplicitUi,
    );
    ContextFeedbackService::apply_feedback(&conn, &ev2).unwrap();

    // Inspect PayX context
    let payx_report = ContextFeedbackInspector::inspect_context(&conn, &payx.id).unwrap();

    // PayX must have 0 total feedback events, 0 confirmations, 0 rejections
    assert_eq!(payx_report.total_feedback_events, 0);
    assert_eq!(payx_report.confirmations_count, 0);
    assert_eq!(payx_report.rejections_count, 0);
    assert_eq!(payx_report.conflicted_targets_count, 0);
}

use chrono::{Duration, Utc};
use rusqlite::Connection;
use serde_json::json;

use crate::context_engine::feedback::{
    AdaptationVerdict, ContextDeterministicAdaptationEngine, ContextFeedbackEvent,
    ContextFeedbackRelevanceEngine, ContextFeedbackService, FeedbackAction, FeedbackScope,
    FeedbackSource,
};
use crate::db::operations::{create_item, create_session, create_workspace};

fn setup_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    crate::db::run_migrations(&conn).unwrap();
    conn
}

// =========================================================================
// ADVERSARIAL TEST 1: YouTube Rejection != Global YouTube Ban
//
// User rejects: YouTube video → PayX
// Must NOT create: YouTube → never PayX
// A legitimate PayX technical demo / webinar on YouTube must still be recognized.
// =========================================================================
#[test]
fn test_adversarial_youtube_rejection_does_not_create_global_youtube_ban_for_payx() {
    let conn = setup_test_db();
    let payx = create_workspace(&conn, "PayX").unwrap();
    let session = create_session(&conn).unwrap();

    // 1. User rejects an unrelated entertainment YouTube video
    let item_unrelated_video = create_item(
        &conn,
        Some(&payx.id),
        Some(&session.id),
        "url",
        "https://www.youtube.com/watch?v=gaming-review-12345",
        Some("Top 10 Mechanical Keyboards 2026 - YouTube"),
        &json!({
            "app_name": "Google Chrome",
            "domain": "youtube.com",
            "url": "https://www.youtube.com/watch?v=gaming-review-12345"
        })
        .to_string(),
    )
    .unwrap();

    let rejection_ev = ContextFeedbackEvent::remove_from_context(
        &item_unrelated_video.id,
        &payx.id,
        FeedbackSource::UserExplicitUi,
        Some("Gaming video unrelated to PayX".to_string()),
    );
    ContextFeedbackService::apply_feedback(&conn, &rejection_ev).unwrap();

    // 2. Another generic entertainment video is indeed dampened
    let item_generic_entertainment = create_item(
        &conn,
        None,
        Some(&session.id),
        "url",
        "https://www.youtube.com/watch?v=funny-cats-999",
        Some("Funny Cats Compilation - YouTube"),
        &json!({
            "app_name": "Google Chrome",
            "domain": "youtube.com",
            "url": "https://www.youtube.com/watch?v=funny-cats-999"
        })
        .to_string(),
    )
    .unwrap();

    let generic_eval = ContextDeterministicAdaptationEngine::evaluate_adaptation(
        &conn,
        &item_generic_entertainment,
        &payx,
        0.50,
    );
    assert_eq!(
        generic_eval.verdict,
        AdaptationVerdict::GenericAppDampened,
        "Generic YouTube entertainment video without PayX relation must be dampened"
    );

    // 3. BUT a specific PayX developer webinar on YouTube appears!
    let item_payx_webinar = create_item(
        &conn,
        None,
        Some(&session.id),
        "url",
        "https://www.youtube.com/watch?v=payx-architecture-deep-dive",
        Some("PayX Stripe Migration & Architecture Deep Dive - YouTube"),
        &json!({
            "app_name": "Google Chrome",
            "domain": "youtube.com",
            "url": "https://www.youtube.com/watch?v=payx-architecture-deep-dive"
        })
        .to_string(),
    )
    .unwrap();

    let webinar_eval = ContextDeterministicAdaptationEngine::evaluate_adaptation(
        &conn,
        &item_payx_webinar,
        &payx,
        0.50,
    );

    // Specific relationship must bypass app/domain-level dampening
    assert_eq!(
        webinar_eval.verdict,
        AdaptationVerdict::SpecificRelationshipRecognized,
        "Specific PayX technical webinar on YouTube must NOT be banned; specific relationship takes precedence"
    );
    assert!(
        webinar_eval.adapted_score > webinar_eval.base_score,
        "PayX webinar receives affinity boost instead of being blocked"
    );
    assert!(webinar_eval.has_specific_entity_match);
    assert!(!webinar_eval.is_generic_suppressed);
}

// =========================================================================
// ADVERSARIAL TEST 2: VS Code Rejection != Global VS Code Ban
//
// User rejects: VS Code → University OS
// Must NOT create: VS Code → never University OS
// An actual OS assignment file in VS Code must still be strongly recognized.
// =========================================================================
#[test]
fn test_adversarial_vscode_rejection_does_not_create_global_vscode_ban_for_university_os() {
    let conn = setup_test_db();
    let uni_os = create_workspace(&conn, "University OS").unwrap();
    let session = create_session(&conn).unwrap();

    // 1. User rejects an unrelated personal React project opened in VS Code
    let item_personal_react = create_item(
        &conn,
        Some(&uni_os.id),
        Some(&session.id),
        "window_focus",
        "VS Code - personal_react_portfolio/App.tsx",
        Some("VS Code - personal_react_portfolio/App.tsx"),
        &json!({
            "app_name": "Visual Studio Code",
            "file_path": "/Users/student/projects/personal_react_portfolio/App.tsx"
        })
        .to_string(),
    )
    .unwrap();

    let rejection_ev = ContextFeedbackEvent::remove_from_context(
        &item_personal_react.id,
        &uni_os.id,
        FeedbackSource::UserExplicitUi,
        Some("Personal portfolio not part of University OS course".to_string()),
    );
    ContextFeedbackService::apply_feedback(&conn, &rejection_ev).unwrap();

    // 2. An actual Operating Systems assignment in VS Code is opened
    let item_os_kernel = create_item(
        &conn,
        None,
        Some(&session.id),
        "window_focus",
        "VS Code - university_os/assignment2/kernel_scheduler.c",
        Some("VS Code - university_os/assignment2/kernel_scheduler.c"),
        &json!({
            "app_name": "Visual Studio Code",
            "file_path": "/Users/student/courses/university_os/assignment2/kernel_scheduler.c"
        })
        .to_string(),
    )
    .unwrap();

    let kernel_eval = ContextDeterministicAdaptationEngine::evaluate_adaptation(
        &conn,
        &item_os_kernel,
        &uni_os,
        0.70,
    );

    // Specific relationship must be recognized and bypass generic dampening
    assert_eq!(
        kernel_eval.verdict,
        AdaptationVerdict::SpecificRelationshipRecognized,
        "University OS kernel file in VS Code must be recognized; VS Code is not globally banned"
    );
    assert!(kernel_eval.has_specific_entity_match);
    assert!(kernel_eval.adapted_score >= 0.90);
}

// =========================================================================
// ADVERSARIAL TEST 3: Repository X Confirmation != GitHub-wide Rule
//
// User confirms: Repository X → PayX
// Must NOT create: GitHub → PayX
// Unrelated GitHub repositories must NOT inherit confirmation affinity.
// =========================================================================
#[test]
fn test_adversarial_github_repo_confirmation_does_not_create_github_wide_rule_for_payx() {
    let conn = setup_test_db();
    let payx = create_workspace(&conn, "PayX").unwrap();
    let session = create_session(&conn).unwrap();

    // 1. User confirms specific repository: payx-org/payx-core
    let item_payx_core = create_item(
        &conn,
        Some(&payx.id),
        Some(&session.id),
        "url",
        "https://github.com/payx-org/payx-core",
        Some("payx-org/payx-core: Core Payment Gateway - GitHub"),
        &json!({
            "app_name": "Google Chrome",
            "domain": "github.com",
            "repo_slug": "payx-org/payx-core",
            "url": "https://github.com/payx-org/payx-core"
        })
        .to_string(),
    )
    .unwrap();

    let confirm_ev = ContextFeedbackEvent::confirm_context(
        &item_payx_core.id,
        &payx.id,
        FeedbackSource::UserExplicitUi,
    );
    ContextFeedbackService::apply_feedback(&conn, &confirm_ev).unwrap();

    // 2. User browses a completely unrelated open-source project on github.com
    let item_unrelated_repo = create_item(
        &conn,
        None,
        Some(&session.id),
        "url",
        "https://github.com/torvalds/linux",
        Some("torvalds/linux: Linux kernel source tree - GitHub"),
        &json!({
            "app_name": "Google Chrome",
            "domain": "github.com",
            "repo_slug": "torvalds/linux",
            "url": "https://github.com/torvalds/linux"
        })
        .to_string(),
    )
    .unwrap();

    let unrelated_eval = ContextDeterministicAdaptationEngine::evaluate_adaptation(
        &conn,
        &item_unrelated_repo,
        &payx,
        0.40,
    );

    // Multi-tenant domain safeguard: github.com is NOT treated as a private host
    assert_ne!(
        unrelated_eval.verdict,
        AdaptationVerdict::ConfirmedDirectly,
        "Unrelated GitHub repo must not be marked ConfirmedDirectly"
    );
    assert_ne!(
        unrelated_eval.verdict,
        AdaptationVerdict::SpecificRelationshipRecognized,
        "Unrelated GitHub repo must not receive specific relationship recognition for PayX"
    );
    assert!(
        !unrelated_eval.has_specific_entity_match,
        "Unrelated repo has zero entity matches with PayX"
    );
    assert_eq!(unrelated_eval.adapted_score, 0.40);
}

// =========================================================================
// ADVERSARIAL TEST 4: Multi-Tenant Cloud Services (Figma, Notion, Google Docs)
//
// Confirming one specific project file on Figma or Notion must NOT associate
// all other workspace documents from other clients to that context.
// =========================================================================
#[test]
fn test_adversarial_multitenant_cloud_services_preserve_isolation() {
    let conn = setup_test_db();
    let client_a = create_workspace(&conn, "Client Alpha").unwrap();
    let _client_b = create_workspace(&conn, "Client Beta").unwrap();
    let session = create_session(&conn).unwrap();

    // 1. Confirm Client Alpha Figma file
    let figma_alpha = create_item(
        &conn,
        Some(&client_a.id),
        Some(&session.id),
        "url",
        "https://figma.com/file/alpha-design-system",
        Some("Alpha Design System - Figma"),
        &json!({
            "app_name": "Figma",
            "domain": "figma.com",
            "url": "https://figma.com/file/alpha-design-system"
        })
        .to_string(),
    )
    .unwrap();

    let ev_alpha = ContextFeedbackEvent::confirm_context(
        &figma_alpha.id,
        &client_a.id,
        FeedbackSource::UserExplicitUi,
    );
    ContextFeedbackService::apply_feedback(&conn, &ev_alpha).unwrap();

    // 2. Client Beta visits a totally different Figma file
    let figma_beta = create_item(
        &conn,
        None,
        Some(&session.id),
        "url",
        "https://figma.com/file/beta-dashboard-v1",
        Some("Beta Dashboard V1 - Figma"),
        &json!({
            "app_name": "Figma",
            "domain": "figma.com",
            "url": "https://figma.com/file/beta-dashboard-v1"
        })
        .to_string(),
    )
    .unwrap();

    // Evaluate Beta file against Client Alpha context
    let alpha_eval_for_beta = ContextDeterministicAdaptationEngine::evaluate_adaptation(
        &conn,
        &figma_beta,
        &client_a,
        0.30,
    );

    // Multi-tenant protection: figma.com does not bind all files to Client Alpha
    assert!(
        !alpha_eval_for_beta.has_specific_entity_match,
        "Beta Figma file must not match Client Alpha"
    );
    assert_eq!(alpha_eval_for_beta.adapted_score, 0.30);
}

// =========================================================================
// ADVERSARIAL TEST 5: Narrowest Scope Precedence Over Broad Fallback
//
// Specific ItemContext feedback takes precedence over general application noise.
// =========================================================================
#[test]
fn test_adversarial_narrowest_scope_precedence_over_broad_fallback() {
    let conn = setup_test_db();
    let payx = create_workspace(&conn, "PayX").unwrap();
    let session = create_session(&conn).unwrap();

    // Generate 3 rejections for generic Chrome pages in PayX
    for i in 1..=3 {
        let item_noise = create_item(
            &conn,
            Some(&payx.id),
            Some(&session.id),
            "url",
            &format!("https://news.ycombinator.com/item?id={}", i),
            Some(&format!("Hacker News Item {}", i)),
            &json!({
                "app_name": "Google Chrome",
                "domain": "news.ycombinator.com"
            })
            .to_string(),
        )
        .unwrap();

        let rej = ContextFeedbackEvent::remove_from_context(
            &item_noise.id,
            &payx.id,
            FeedbackSource::UserExplicitUi,
            None,
        );
        ContextFeedbackService::apply_feedback(&conn, &rej).unwrap();
    }

    // Now an explicit PayX dashboard item in Chrome appears
    let item_payx_dash = create_item(
        &conn,
        None,
        Some(&session.id),
        "url",
        "https://app.payx.internal/dashboard",
        Some("PayX Merchant Operations Dashboard"),
        &json!({
            "app_name": "Google Chrome",
            "domain": "app.payx.internal",
            "url": "https://app.payx.internal/dashboard"
        })
        .to_string(),
    )
    .unwrap();

    let eval = ContextDeterministicAdaptationEngine::evaluate_adaptation(
        &conn,
        &item_payx_dash,
        &payx,
        0.60,
    );

    // Specific relationship MUST bypass 3 previous Chrome rejections
    assert_eq!(
        eval.verdict,
        AdaptationVerdict::SpecificRelationshipRecognized,
        "Specific PayX internal dashboard must override 3 previous generic Chrome rejections"
    );
    assert!(eval.has_specific_entity_match);
    assert!(eval.adapted_score >= 0.80);
}

// =========================================================================
// ADVERSARIAL TEST 6: Temporary Dismissal Decay Boundedness
//
// Temporary dismissal of an inbox suggestion must not permanently poison
// future explicit user associations.
// =========================================================================
#[test]
fn test_adversarial_temporary_dismissal_decay_boundedness() {
    let now = Utc::now();
    let config = crate::context_engine::feedback::FeedbackRelevanceConfig::default();

    // Create a temporary suggestion dismissal event
    let dismissal_ev = ContextFeedbackEvent::new(
        FeedbackAction::DismissSuggestion,
        crate::context_engine::feedback::FeedbackTargetRef::Suggestion("sugg_payx_1".to_string()),
        Some("ws_payx".to_string()),
        FeedbackSource::InboxAction,
        FeedbackScope::item_context("sugg_payx_1", "ws_payx"),
        Some("Dismissed recommendation for now".to_string()),
    )
    .with_timestamp((now - Duration::days(20)).to_rfc3339()); // 20 days ago (past 14d cooldown)

    let relevance = ContextFeedbackRelevanceEngine::evaluate_event_relevance(&dismissal_ev, now, &config);

    // After 20 days (past 14d cooldown), temporary dismissal is expired
    assert!(
        relevance.is_expired,
        "Temporary dismissal must expire after cooldown period"
    );
    assert_eq!(relevance.effective_weight, 0.0);
}

use std::time::Instant;
use rusqlite::Connection;
use serde_json::json;

use crate::context_engine::ambiguity::{AmbiguityConfig, AmbiguityManager, AmbiguityEvaluationInput, CandidateContextAffinity};
use crate::context_engine::contamination::{ContaminationCheckInput, ContaminationGuard};
use crate::context_engine::pending_activity::{
    ActivityEvidenceSignal, DelayedResolutionConfig,
    PendingActivityConfig, PendingActivityManager, PendingContextualActivity,
};
use crate::context_engine::quality_guard::ContextQualityGuard;
use crate::context_engine::reconstruction::ContextReconstructionService;
use crate::context_engine::scoring::evaluate_item_context;
use crate::db::operations::{create_item, create_session, create_workspace};

fn setup_perf_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    crate::db::run_migrations(&conn).unwrap();
    conn
}

#[test]
fn test_perf_low_activity_pipeline() {
    let conn = setup_perf_db();
    let ws = create_workspace(&conn, "ContextSplit").unwrap();
    let session = create_session(&conn).unwrap();

    let start = Instant::now();
    for i in 0..10 {
        let title = format!("ContextSplit - file_{}.rs", i);
        let item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "window_focus",
            &format!("ContextSplit Editor tab {}", i),
            Some(&title),
            &json!({ "app_name": "Code", "file_path": format!("/src/file_{}.rs", i) }).to_string(),
        ).unwrap();

        let guard = ContextQualityGuard::new();
        let res = guard.evaluate_from_db(&conn, &item.id, &ws.id).unwrap();
        assert!(res.decision.is_accepted());
    }
    let elapsed = start.elapsed();
    println!("Low activity (10 items) latency: {:?}", elapsed);
    assert!(elapsed.as_millis() < 500, "Low activity pipeline should execute under 500ms");
}

#[test]
fn test_perf_normal_activity_pipeline() {
    let conn = setup_perf_db();
    let ws1 = create_workspace(&conn, "ContextSplit").unwrap();
    let ws2 = create_workspace(&conn, "PayX").unwrap();
    let session = create_session(&conn).unwrap();

    let start = Instant::now();
    for i in 0..50 {
        let (target_ws, title, app) = if i % 2 == 0 {
            (&ws1, format!("ContextSplit State Management - slice_{}.rs", i), "VS Code")
        } else {
            (&ws2, format!("PayX Payment Gateway - checkout_{}.ts", i), "VS Code")
        };

        let item = create_item(
            &conn,
            Some(&target_ws.id),
            Some(&session.id),
            "window_focus",
            &title,
            Some(&title),
            &json!({ "app_name": app }).to_string(),
        ).unwrap();

        let guard = ContextQualityGuard::new();
        let res = guard.evaluate_from_db(&conn, &item.id, &target_ws.id).unwrap();
        assert!(res.decision.is_accepted());
    }
    let elapsed = start.elapsed();
    println!("Normal activity (50 items) latency: {:?}", elapsed);
    assert!(elapsed.as_millis() < 1500, "Normal activity pipeline should execute under 1500ms");
}

#[test]
fn test_perf_high_activity_stress() {
    let conn = setup_perf_db();
    let ws = create_workspace(&conn, "HighLoadContext").unwrap();
    let session = create_session(&conn).unwrap();

    let start = Instant::now();
    for i in 0..200 {
        let title = format!("HighLoad file_{}.cpp", i % 20);
        let item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "file_edit",
            &format!("Code snippet line {}", i),
            Some(&title),
            &json!({ "app_name": "CLion" }).to_string(),
        ).unwrap();

        let guard = ContextQualityGuard::new();
        let res = guard.evaluate_from_db(&conn, &item.id, &ws.id).unwrap();
        assert!(res.decision.is_accepted());
    }
    let elapsed = start.elapsed();
    println!("High activity (200 items) latency: {:?}", elapsed);
    assert!(elapsed.as_millis() < 4000, "High activity pipeline should complete in under 4 seconds");
}

#[test]
fn test_perf_many_contexts_scale() {
    let conn = setup_perf_db();
    let mut ws_ids = Vec::new();
    for i in 0..50 {
        let ws = create_workspace(&conn, &format!("Workspace_{:02}", i)).unwrap();
        ws_ids.push(ws.id);
    }
    let session = create_session(&conn).unwrap();

    // Create target item in Workspace_25
    let target_ws_id = &ws_ids[25];
    let item = create_item(
        &conn,
        Some(target_ws_id),
        Some(&session.id),
        "window_focus",
        "Workspace_25 Deep Engine Module",
        Some("Workspace_25 Deep Engine Module"),
        &json!({ "app_name": "RustRover", "file_path": "/src/workspace_25/core.rs" }).to_string(),
    ).unwrap();

    let start = Instant::now();
    // evaluate_item_context scores across all 50 workspaces
    let interp = evaluate_item_context(&conn, &item.id).unwrap();
    let elapsed = start.elapsed();

    println!("Scoring across 50 contexts latency: {:?}", elapsed);
    assert_eq!(interp.existing_workspaces.len(), 50);
    assert_eq!(interp.existing_workspaces[0].workspace_id, *target_ws_id);
    assert!(elapsed.as_millis() < 100, "Scoring against 50 workspaces should be sub-100ms (no N+1 DB overhead)");
}

#[test]
fn test_perf_many_pending_events_bounded_queue() {
    let mut manager = PendingActivityManager::new(PendingActivityConfig {
        delayed_resolution: DelayedResolutionConfig {
            max_pending_queue_size: 100,
            max_temporal_window_seconds: 3600,
            require_app_or_domain_affinity: false,
            max_revisit_candidates_per_event: 20,
        },
        ..Default::default()
    });

    let start = Instant::now();
    // Ingest 2,000 ambiguous pending items
    for i in 0..2000 {
        let mut act = PendingContextualActivity::new(
            format!("act_{}", i),
            format!("Generic Chrome Tab {}", i),
        );
        act.app_name = Some("Google Chrome".to_string());
        act.domain = Some("github.com".to_string());
        act.add_signal(ActivityEvidenceSignal::weak_app_signal("Google Chrome"));
        manager.ingest_activity(act);
    }
    let elapsed = start.elapsed();
    println!("Ingesting 2,000 pending items with bounded pruning: {:?}", elapsed);

    // Bounded queue must strictly retain at most max_pending_queue_size unassigned items
    let unassigned = manager.get_pending_activities().len() + manager.get_ambiguous_activities().len();
    assert_eq!(unassigned, 100, "Pending queue must be strictly bounded to 100 items");
    assert!(elapsed.as_millis() < 500, "2,000 item ingestion and bound pruning should be under 500ms");

    // Ingest decisive event triggering delayed resolution
    let mut decisive = PendingContextualActivity::new("act_decisive", "ContextSplit Main Repo");
    decisive.app_name = Some("Google Chrome".to_string());
    decisive.domain = Some("github.com".to_string());
    decisive.add_signal(ActivityEvidenceSignal::project_repo_match(
        "ws_cs",
        "github.com/org/contextsplit",
    ));

    let revisit_start = Instant::now();
    let (_, resolutions) = manager.ingest_activity(decisive);
    let revisit_elapsed = revisit_start.elapsed();

    println!("Revisiting bounded pending items latency: {:?}, resolved count: {}", revisit_elapsed, resolutions.len());
    assert!(!resolutions.is_empty(), "Decisive event should resolve recent pending items");
    assert!(resolutions.len() <= 20, "Revisit candidate count must respect max_revisit_candidates_per_event");
    assert!(revisit_elapsed.as_millis() < 50, "Delayed resolution revisit must be sub-50ms");
}

#[test]
fn test_perf_many_historical_items_index_efficiency() {
    let conn = setup_perf_db();
    let ws1 = create_workspace(&conn, "ContextSplit").unwrap();
    let ws2 = create_workspace(&conn, "PayX").unwrap();
    let session = create_session(&conn).unwrap();

    // Insert 2,000 historical items into database inside a single transaction for speed
    let start_seed = Instant::now();
    conn.execute_batch("BEGIN TRANSACTION;").unwrap();
    for i in 0..2000 {
        let (ws, app, dom) = if i % 2 == 0 {
            (&ws1.id, "VS Code", "github.com/org/contextsplit")
        } else {
            (&ws2.id, "Xcode", "developer.apple.com")
        };
        conn.execute(
            "INSERT INTO items (id, workspace_id, item_type, content, title, metadata, session_id, created_at, updated_at)
             VALUES (?1, ?2, 'window_focus', ?3, ?4, ?5, ?6, datetime('now', '-' || ?7 || ' seconds'), datetime('now', '-' || ?7 || ' seconds'))",
            rusqlite::params![
                format!("item_{}", i),
                ws,
                format!("Activity content {}", i),
                format!("Activity title {}", i),
                json!({ "app_name": app, "domain": dom }).to_string(),
                session.id,
                i,
            ],
        ).unwrap();
    }
    conn.execute_batch("COMMIT;").unwrap();
    println!("Database 2,000 items seeded in {:?}", start_seed.elapsed());

    // Measure index-accelerated Context Reconstruction on workspace with 1,000 items
    let reconstruction_service = ContextReconstructionService::new();
    let start_rec = Instant::now();
    let rec = reconstruction_service.reconstruct_context_cached(&conn, &ws1.id).unwrap();
    let elapsed_rec = start_rec.elapsed();

    println!("Reconstructing context with 1,000 items latency: {:?}", elapsed_rec);
    assert_eq!(rec.context.id.as_deref(), Some(ws1.id.as_str()));
    assert!(elapsed_rec.as_millis() < 4000, "Cold reconstruction over 1,000 indexed items must complete smoothly in debug mode");

    // Second reconstruction should hit the cache (<1ms)
    let start_cached = Instant::now();
    let rec_cached = reconstruction_service.reconstruct_context_cached(&conn, &ws1.id).unwrap();
    let elapsed_cached = start_cached.elapsed();
    println!("Cached reconstruction latency: {:?}", elapsed_cached);
    assert_eq!(rec_cached.context.id.as_deref(), Some(ws1.id.as_str()));
    assert!(elapsed_cached.as_millis() < 10, "Cached reconstruction must be sub-10ms");
}

#[test]
fn test_perf_ambiguity_manager_stress() {
    let manager = AmbiguityManager::with_config(AmbiguityConfig::default());

    let start = Instant::now();
    for i in 0..1000 {
        let input = AmbiguityEvaluationInput::new(
            format!("item_{}", i),
            "Ambiguous Multi-Project GitHub PR",
        )
        .with_candidate(CandidateContextAffinity::new("ws_a", "PayX", 0.60, 0.60))
        .with_candidate(CandidateContextAffinity::new("ws_b", "ContextSplit", 0.58, 0.58));

        let res = manager.evaluate(&input);
        assert!(res.is_ambiguous);
    }
    let elapsed = start.elapsed();
    println!("Ambiguity manager 1,000 evaluations latency: {:?}", elapsed);
    assert!(elapsed.as_millis() < 200, "1,000 ambiguity evaluations should complete under 200ms");
}

#[test]
fn test_perf_contamination_guard_stress() {
    let guard = ContaminationGuard::new();
    let start = Instant::now();
    for i in 0..2000 {
        let input = ContaminationCheckInput::new(
            format!("YouTube - Funny Cat Video Episode {}", i),
            "ws_payx",
            "PayX",
        )
        .with_app("Google Chrome")
        .with_domain("youtube.com")
        .with_url("https://youtube.com/watch?v=12345")
        .with_semantic_score(0.02);

        let res = guard.assess(&input);
        assert!(res.is_contamination_risk);
    }
    let elapsed = start.elapsed();
    println!("Contamination guard 2,000 evaluations latency: {:?}", elapsed);
    assert!(elapsed.as_millis() < 100, "2,000 pure contamination guard checks must complete in sub-100ms");
}

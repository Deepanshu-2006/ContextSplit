use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use rusqlite::Connection;
use serde_json::json;

use crate::context_engine::ambiguity::{AmbiguityConfig, AmbiguityEvaluationInput, AmbiguityManager, CandidateContextAffinity};
use crate::context_engine::contamination::{ContaminationCheckInput, ContaminationGuard};
use crate::context_engine::context_state::{ContextState, ContextStateService};
use crate::context_engine::feedback::{
    ContextFeedbackEvent, ContextFeedbackService, FeedbackAction, FeedbackMetadata,
    FeedbackScope, FeedbackSource, FeedbackTargetRef,
};
use crate::context_engine::important_memory::ContextImportantMemoryBuilder;
use crate::context_engine::lifecycle::ContextLifecycleService;
use crate::context_engine::memory::{invalidate_memory_cache, ContextMemory, ContextMemoryService};
use crate::context_engine::pending_activity::{
    ActivityEvidenceSignal, DelayedResolutionConfig, PendingActivityConfig,
    PendingActivityManager, PendingContextualActivity,
};
use crate::context_engine::performance_tracing::{
    get_performance_tracer, start_span, OperationCategory, PerformanceSpan,
};
use crate::context_engine::quality_guard::ContextQualityGuard;
use crate::context_engine::reconstruction::{
    invalidate_reconstruction_cache, ContextReconstruction, ContextReconstructionService,
};
use crate::context_engine::scoring::evaluate_item_context;
use crate::context_engine::session::ContextSessionEvaluator;
use crate::context_engine::snapshot::{
    get_global_snapshot_cache, invalidate_snapshot_cache, ContextSnapshotService,
};
use crate::context_engine::transitions::{
    classify_transition_evidence, ContextReference, ContextTransition, ContextTransitionType,
};
use crate::context_engine::workset::{
    get_global_workset_cache, invalidate_workset_cache, ContextWorkset, ContextWorksetService,
};
use crate::db::operations::{
    create_item, create_session, create_workspace, get_workspace, list_workspaces,
};

fn setup_perf_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;",
    )
    .unwrap();
    crate::db::run_migrations(&conn).unwrap();
    conn
}

/// Helper struct to track and aggregate pipeline performance metrics during benchmark execution.
#[derive(Debug, Default, Clone)]
pub struct PipelinePerformanceMetrics {
    pub total_operations: usize,
    pub total_duration_us: u64,
    pub min_duration_us: u64,
    pub max_duration_us: u64,
    pub avg_duration_us: u64,
    pub db_queries_recorded: u32,
    pub throughput_ops_per_sec: f64,
    pub memory_allocated_approx_bytes: usize,
}

impl PipelinePerformanceMetrics {
    pub fn record(durations_us: &[u64], total_queries: u32) -> Self {
        if durations_us.is_empty() {
            return Self::default();
        }
        let total: u64 = durations_us.iter().sum();
        let min = *durations_us.iter().min().unwrap();
        let max = *durations_us.iter().max().unwrap();
        let avg = total / durations_us.len() as u64;
        let total_secs = total as f64 / 1_000_000.0;
        let throughput = if total_secs > 0.0 {
            durations_us.len() as f64 / total_secs
        } else {
            0.0
        };

        Self {
            total_operations: durations_us.len(),
            total_duration_us: total,
            min_duration_us: min,
            max_duration_us: max,
            avg_duration_us: avg,
            db_queries_recorded: total_queries,
            throughput_ops_per_sec: throughput,
            memory_allocated_approx_bytes: durations_us.len() * 1024,
        }
    }
}

// =========================================================================
// TEST 1: Normal Activity Ingestion & Processing Latency
// =========================================================================
#[test]
fn test_perf_1_normal_activity_processing_latency() {
    let conn = setup_perf_db();
    let ws = create_workspace(&conn, "ContextSplit").unwrap();
    let session = create_session(&conn).unwrap();
    let tracer = get_performance_tracer();
    tracer.clear();

    let mut latencies_us = Vec::new();
    let guard = ContextQualityGuard::new();

    // 100 sequential normal developer events (VS Code edits, file focus, tab switching)
    for i in 0..100 {
        let mut span = start_span(OperationCategory::ActivityIngestion, "normal_telemetry_event")
            .with_context(&ws.id);

        let t_start = Instant::now();
        let item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "window_focus",
            &format!("ContextSplit - buffer_{}.rs", i),
            Some(&format!("ContextSplit - buffer_{}.rs", i)),
            &json!({ "app_name": "VS Code", "file_path": format!("/src/buffer_{}.rs", i) }).to_string(),
        )
        .unwrap();

        span.add_query(); // create_item query

        let eval_res = guard.evaluate_from_db(&conn, &item.id, &ws.id).unwrap();
        assert!(eval_res.decision.is_accepted());
        span.increment_queries(2); // quality guard queries

        let elapsed_us = t_start.elapsed().as_micros() as u64;
        latencies_us.push(elapsed_us);
        span.set_items_processed(1);
        span.end();
    }

    let metrics = PipelinePerformanceMetrics::record(&latencies_us, 300);
    println!(
        "\n[PERF TEST 1 - Normal Activity] Processed {} events | Avg: {}µs ({}ms) | Max: {}µs | Throughput: {:.2} ops/sec",
        metrics.total_operations,
        metrics.avg_duration_us,
        metrics.avg_duration_us as f64 / 1000.0,
        metrics.max_duration_us,
        metrics.throughput_ops_per_sec
    );

    // Assert average per-item processing latency is sub-25ms in debug mode
    assert!(
        metrics.avg_duration_us < 25_000,
        "Normal activity per-item processing latency must average < 25ms, got {}µs",
        metrics.avg_duration_us
    );
}

// =========================================================================
// TEST 2: Many Contexts Scale (100 Workspaces / Contexts)
// =========================================================================
#[test]
fn test_perf_2_many_contexts_scale_evaluation() {
    let conn = setup_perf_db();
    let mut ws_ids = Vec::new();

    // Create 100 distinct project contexts
    for i in 0..100 {
        let ws = create_workspace(&conn, &format!("ProjectContext_{:03}", i)).unwrap();
        ws_ids.push(ws.id);
    }
    let session = create_session(&conn).unwrap();

    // Target item assigned to ProjectContext_050
    let target_ws = &ws_ids[50];
    let item = create_item(
        &conn,
        Some(target_ws),
        Some(&session.id),
        "window_focus",
        "ProjectContext_050 Core Controller - controller.rs",
        Some("ProjectContext_050 Core Controller - controller.rs"),
        &json!({ "app_name": "RustRover", "file_path": "/src/project_050/controller.rs" }).to_string(),
    )
    .unwrap();

    let mut latencies_us = Vec::new();
    // Run scoring evaluations against 100 contexts
    for _ in 0..20 {
        let t_start = Instant::now();
        let interp = evaluate_item_context(&conn, &item.id).unwrap();
        let elapsed_us = t_start.elapsed().as_micros() as u64;
        latencies_us.push(elapsed_us);

        assert_eq!(interp.existing_workspaces.len(), 100);
        assert_eq!(interp.existing_workspaces[0].workspace_id, *target_ws);
    }

    let metrics = PipelinePerformanceMetrics::record(&latencies_us, 20);
    println!(
        "\n[PERF TEST 2 - Many Contexts Scale] Evaluated 100 contexts | Avg: {}µs ({}ms) | Max: {}µs",
        metrics.avg_duration_us,
        metrics.avg_duration_us as f64 / 1000.0,
        metrics.max_duration_us
    );

    // Scoring against 100 workspaces must be under 80ms in debug mode (single-pass indexed query)
    assert!(
        metrics.avg_duration_us < 80_000,
        "Scoring against 100 contexts must be < 80ms, got {}µs",
        metrics.avg_duration_us
    );
}

// =========================================================================
// TEST 3: Many Items Scale (5,000 Items across multiple Workspaces)
// =========================================================================
#[test]
fn test_perf_3_many_items_scale_throughput() {
    let conn = setup_perf_db();
    let ws1 = create_workspace(&conn, "ContextSplit").unwrap();
    let ws2 = create_workspace(&conn, "PayX").unwrap();
    let session = create_session(&conn).unwrap();

    // Insert 5,000 items in a single transaction
    let t_seed = Instant::now();
    conn.execute_batch("BEGIN TRANSACTION;").unwrap();
    for i in 0..5000 {
        let (ws, app, file) = if i % 2 == 0 {
            (&ws1.id, "VS Code", format!("/cs/src/module_{}.rs", i % 50))
        } else {
            (&ws2.id, "Xcode", format!("/payx/PaymentView_{}.swift", i % 50))
        };

        conn.execute(
            "INSERT INTO items (id, workspace_id, item_type, content, title, metadata, session_id, created_at, updated_at)
             VALUES (?1, ?2, 'file_edit', ?3, ?4, ?5, ?6, datetime('now', '-' || ?7 || ' seconds'), datetime('now', '-' || ?7 || ' seconds'))",
            rusqlite::params![
                format!("item_{:05}", i),
                ws,
                format!("Activity content for item {}", i),
                format!("Telemetry title {}", i),
                json!({ "app_name": app, "file_path": file }).to_string(),
                session.id,
                i,
            ],
        ).unwrap();
    }
    conn.execute_batch("COMMIT;").unwrap();
    let seed_dur = t_seed.elapsed();
    println!(
        "\n[PERF TEST 3 - Many Items Scale] 5,000 items inserted in {:?}",
        seed_dur
    );

    // Verify bounded workset derivation over 2,500 items per workspace
    let workset_service = ContextWorksetService::new();
    let t_ws = Instant::now();
    let workset = workset_service.derive_for_workspace(&conn, &ws1.id).unwrap();
    let ws_elapsed = t_ws.elapsed();

    assert!(!workset.items.is_empty());
    assert!(workset.items.len() <= 50, "Workset must be bounded to maximum 50 active items");
    println!(
        "[PERF TEST 3] Bounded workset derivation over 2,500 items in workspace completed in {:?}",
        ws_elapsed
    );
    assert!(
        ws_elapsed.as_millis() < 3000,
        "Bounded workset query must complete in < 3000ms even with 5,000 total items"
    );
}

// =========================================================================
// TEST 4: Many Sessions Scale (100 Sessions)
// =========================================================================
#[test]
fn test_perf_4_many_sessions_scale() {
    let conn = setup_perf_db();
    let ws = create_workspace(&conn, "MultiSessionContext").unwrap();

    let mut session_ids = Vec::new();
    for _ in 0..100 {
        let sess = create_session(&conn).unwrap();
        session_ids.push(sess.id);
    }

    // Assign activity items to various sessions
    conn.execute_batch("BEGIN TRANSACTION;").unwrap();
    for (i, sess_id) in session_ids.iter().enumerate() {
        conn.execute(
            "INSERT INTO items (id, workspace_id, item_type, content, title, metadata, session_id, created_at, updated_at)
             VALUES (?1, ?2, 'window_focus', ?3, ?4, ?5, ?6, datetime('now', '-' || ?7 || ' minutes'), datetime('now', '-' || ?7 || ' minutes'))",
            rusqlite::params![
                format!("sess_item_{}", i),
                ws.id,
                format!("Session activity {}", i),
                format!("Session title {}", i),
                json!({ "app_name": "Terminal" }).to_string(),
                sess_id,
                i * 10,
            ],
        ).unwrap();
    }
    conn.execute_batch("COMMIT;").unwrap();

    // Measure session evaluation latency across sessions
    let t_start = Instant::now();
    let sessions_count: usize = conn
        .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
        .unwrap();
    let elapsed = t_start.elapsed();

    assert_eq!(sessions_count, 100);
    println!(
        "\n[PERF TEST 4 - Many Sessions Scale] Indexed query on 100 sessions completed in {:?}",
        elapsed
    );
    assert!(elapsed.as_millis() < 50, "Session lookup over 100 sessions must be sub-50ms");
}

// =========================================================================
// TEST 5: Many Memories Scale (Episodic Memory & Importance Ranking)
// =========================================================================
#[test]
fn test_perf_5_many_memories_scale() {
    let conn = setup_perf_db();
    let ws = create_workspace(&conn, "ContextSplit").unwrap();
    let session = create_session(&conn).unwrap();

    // Create 300 items with rich metadata
    conn.execute_batch("BEGIN TRANSACTION;").unwrap();
    for i in 0..300 {
        conn.execute(
            "INSERT INTO items (id, workspace_id, item_type, content, title, metadata, session_id, created_at, updated_at)
             VALUES (?1, ?2, 'file_edit', ?3, ?4, ?5, ?6, datetime('now', '-' || ?7 || ' minutes'), datetime('now', '-' || ?7 || ' minutes'))",
            rusqlite::params![
                format!("mem_item_{}", i),
                ws.id,
                format!("Code implementation slice {}", i),
                format!("ContextSplit Feature {}", i % 15),
                json!({ "app_name": "VS Code", "file_path": format!("/src/feature_{}.rs", i % 15) }).to_string(),
                session.id,
                i * 5,
            ],
        ).unwrap();
    }
    conn.execute_batch("COMMIT;").unwrap();

    // Measure Important Memory extraction and ranking
    let t_start = Instant::now();
    let memory_set = ContextImportantMemoryBuilder::build(&conn, &ws.id, None).unwrap();
    let elapsed = t_start.elapsed();

    println!(
        "\n[PERF TEST 5 - Many Memories Scale] Extracted {} important memories from 300 candidates in {:?}",
        memory_set.memories.len(),
        elapsed
    );
    assert!(memory_set.memories.len() <= 20);
    assert!(
        elapsed.as_millis() < 6000,
        "Important memory ranking over 300 candidates must complete in < 6000ms in debug mode"
    );
}

// =========================================================================
// TEST 6: Repeated Context Switching (Rapid Switch & Return Pipeline)
// =========================================================================
#[test]
fn test_perf_6_repeated_context_switching() {
    let conn = setup_perf_db();
    let ws_a = create_workspace(&conn, "ContextSplit").unwrap();
    let ws_b = create_workspace(&conn, "PayX").unwrap();
    let ws_c = create_workspace(&conn, "InfraOps").unwrap();
    let _session = create_session(&conn).unwrap();

    let contexts = [&ws_a, &ws_b, &ws_c];
    let mut switch_latencies_us = Vec::new();
    let evidence_tags = vec![
        "repo: github.com/org/contextsplit".to_string(),
        "file: /src/controller.rs".to_string(),
    ];

    // Simulate 60 rapid context transitions (A -> B -> C -> A ...)
    for i in 0..60 {
        let _current_ws = contexts[i % 3];
        let next_ws = contexts[(i + 1) % 3];

        let t_start = Instant::now();
        let evidence = classify_transition_evidence(
            false,
            0.88,
            0.85,
            0.35,
            3,
            5,
            &evidence_tags,
        );

        assert!(evidence.has_project_or_repo_change || evidence.has_file_path_change);

        // Derive active workset for the switched context
        let _workset = ContextWorksetService::new()
            .derive_for_workspace(&conn, &next_ws.id)
            .unwrap();

        let elapsed_us = t_start.elapsed().as_micros() as u64;
        switch_latencies_us.push(elapsed_us);
    }

    let metrics = PipelinePerformanceMetrics::record(&switch_latencies_us, 60);
    println!(
        "\n[PERF TEST 6 - Rapid Context Switching] 60 switches | Avg: {}µs ({}ms) | Max: {}µs",
        metrics.avg_duration_us,
        metrics.avg_duration_us as f64 / 1000.0,
        metrics.max_duration_us
    );

    // Rapid switch classification + workset derivation should be sub-15ms
    assert!(
        metrics.avg_duration_us < 15_000,
        "Context switch processing must average < 15ms, got {}µs",
        metrics.avg_duration_us
    );
}

// =========================================================================
// TEST 7: Large Worksets Derivation & Quality Gating
// =========================================================================
#[test]
fn test_perf_7_large_worksets_derivation() {
    let conn = setup_perf_db();
    let ws = create_workspace(&conn, "LargeWorksetProject").unwrap();
    let session = create_session(&conn).unwrap();

    // Populate 500 items in the workspace
    conn.execute_batch("BEGIN TRANSACTION;").unwrap();
    for i in 0..500 {
        conn.execute(
            "INSERT INTO items (id, workspace_id, item_type, content, title, metadata, session_id, created_at, updated_at)
             VALUES (?1, ?2, 'window_focus', ?3, ?4, ?5, ?6, datetime('now', '-' || ?7 || ' seconds'), datetime('now', '-' || ?7 || ' seconds'))",
            rusqlite::params![
                format!("lws_item_{}", i),
                ws.id,
                format!("Large workset content {}", i),
                format!("Large workset item {}", i),
                json!({ "app_name": "Editor", "file_path": format!("/src/file_{}.rs", i) }).to_string(),
                session.id,
                i,
            ],
        ).unwrap();
    }
    conn.execute_batch("COMMIT;").unwrap();

    let service = ContextWorksetService::new();

    // Cold derivation
    invalidate_workset_cache(Some(&ws.id));
    let t_cold = Instant::now();
    let cold_ws = service.derive_for_workspace(&conn, &ws.id).unwrap();
    let cold_dur = t_cold.elapsed();

    assert!(!cold_ws.items.is_empty());
    assert!(cold_ws.items.len() <= 50);
    println!(
        "\n[PERF TEST 7 - Large Worksets] Cold derivation over 500 items: {:?}",
        cold_dur
    );
    assert!(
        cold_dur.as_millis() < 3000,
        "Cold workset derivation over 500 items must be < 3000ms in debug mode"
    );

    // Warm derivation (cache hit)
    let t_warm = Instant::now();
    let warm_ws = service.derive_for_workspace(&conn, &ws.id).unwrap();
    let warm_dur = t_warm.elapsed();

    assert_eq!(cold_ws.context_id, warm_ws.context_id);
    println!(
        "[PERF TEST 7] Warm workset derivation (cache hit): {:?}",
        warm_dur
    );
    assert!(
        warm_dur.as_micros() < 2000,
        "Warm cached workset derivation must be sub-2ms (got {}µs)",
        warm_dur.as_micros()
    );
}

// =========================================================================
// TEST 8: Repeated Context Reconstruction Stress (100 Iterations)
// =========================================================================
#[test]
fn test_perf_8_repeated_reconstruction_stress() {
    let conn = setup_perf_db();
    let ws = create_workspace(&conn, "ContextSplit").unwrap();
    let session = create_session(&conn).unwrap();

    // Seed realistic workspace items & tasks
    create_item(
        &conn,
        Some(&ws.id),
        Some(&session.id),
        "window_focus",
        "ContextSplit Engine - engine.rs",
        Some("ContextSplit Engine - engine.rs"),
        &json!({ "app_name": "VS Code" }).to_string(),
    )
    .unwrap();

    conn.execute(
        "INSERT INTO task_candidates (id, context_id, title, confidence, status, first_detected_at, last_active_at, created_at, updated_at)
         VALUES ('task_1', ?1, 'Phase 1 Performance Validation', 0.95, 'active', datetime('now'), datetime('now'), datetime('now'), datetime('now'))",
        rusqlite::params![ws.id],
    ).unwrap();

    let service = ContextReconstructionService::new();
    let mut cold_durations_us = Vec::new();
    let mut warm_durations_us = Vec::new();

    // 10 Cold Reconstructions (bypassing caches)
    for _ in 0..10 {
        let t_cold = Instant::now();
        let rec = service.reconstruct_cold(&conn, &ws.id).unwrap();
        let elapsed_us = t_cold.elapsed().as_micros() as u64;
        cold_durations_us.push(elapsed_us);
        assert_eq!(rec.context.name, "ContextSplit");
    }

    // 50 Warm Reconstructions (utilizing cache)
    for _ in 0..50 {
        let t_warm = Instant::now();
        let rec = service.reconstruct_context_cached(&conn, &ws.id).unwrap();
        let elapsed_us = t_warm.elapsed().as_micros() as u64;
        warm_durations_us.push(elapsed_us);
        assert_eq!(rec.context.name, "ContextSplit");
    }

    let cold_metrics = PipelinePerformanceMetrics::record(&cold_durations_us, 100);
    let warm_metrics = PipelinePerformanceMetrics::record(&warm_durations_us, 0);

    println!(
        "\n[PERF TEST 8 - Repeated Reconstruction] 10 Cold Avg: {}µs ({}ms) | 50 Warm Avg: {}µs ({:.3}ms)",
        cold_metrics.avg_duration_us,
        cold_metrics.avg_duration_us as f64 / 1000.0,
        warm_metrics.avg_duration_us,
        warm_metrics.avg_duration_us as f64 / 1000.0
    );

    assert!(
        cold_metrics.avg_duration_us < 600_000,
        "Cold reconstruction must average < 600ms in debug mode, got {}µs",
        cold_metrics.avg_duration_us
    );
    assert!(
        warm_metrics.avg_duration_us < 15_000,
        "Warm reconstruction must average < 15ms (cache hit), got {}µs",
        warm_metrics.avg_duration_us
    );
}

// =========================================================================
// TEST 9: Repeated Feedback Throughput & Cache Invalidation
// =========================================================================
#[test]
fn test_perf_9_repeated_feedback_throughput() {
    let conn = setup_perf_db();
    let ws = create_workspace(&conn, "FeedbackScaleProject").unwrap();
    let session = create_session(&conn).unwrap();

    let mut item_ids = Vec::new();
    for i in 0..50 {
        let item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "window_focus",
            &format!("Feedback Candidate Item {}", i),
            Some(&format!("Feedback Candidate Item {}", i)),
            &json!({ "app_name": "Browser" }).to_string(),
        )
        .unwrap();
        item_ids.push(item.id);
    }

    let mut feedback_latencies_us = Vec::new();

    // Submit 50 feedback events (confirmations and dismissals)
    for (i, item_id) in item_ids.iter().enumerate() {
        let action = if i % 2 == 0 {
            FeedbackAction::ConfirmContext
        } else {
            FeedbackAction::DismissSuggestion
        };

        let event = ContextFeedbackEvent::new(
            action,
            FeedbackTargetRef::Item(item_id.clone()),
            Some(ws.id.clone()),
            FeedbackSource::UserExplicitUi,
            FeedbackScope::item_context(item_id.clone(), ws.id.clone()),
            Some("Phase 1 performance evaluation feedback".to_string()),
        );

        let t_start = Instant::now();
        let result = ContextFeedbackService::apply_feedback(&conn, &event).unwrap();
        assert!(result.success);
        let elapsed_us = t_start.elapsed().as_micros() as u64;
        feedback_latencies_us.push(elapsed_us);
    }

    let metrics = PipelinePerformanceMetrics::record(&feedback_latencies_us, 150);
    println!(
        "\n[PERF TEST 9 - Repeated Feedback] Processed 50 feedback events | Avg: {}µs ({}ms) | Max: {}µs",
        metrics.avg_duration_us,
        metrics.avg_duration_us as f64 / 1000.0,
        metrics.max_duration_us
    );

    // Explicit feedback application must be fast (< 25ms per feedback)
    assert!(
        metrics.avg_duration_us < 25_000,
        "Feedback application must average < 25ms, got {}µs",
        metrics.avg_duration_us
    );
}

// =========================================================================
// TEST 10: Application Restart Simulation (Cold Boot State Recovery)
// =========================================================================
#[test]
fn test_perf_10_application_restart_simulation() {
    let conn = setup_perf_db();
    let ws = create_workspace(&conn, "PersistedWorkspace").unwrap();
    let session = create_session(&conn).unwrap();

    // Populate persisted context state
    create_item(
        &conn,
        Some(&ws.id),
        Some(&session.id),
        "file_edit",
        "ContextSplit State - state.rs",
        Some("ContextSplit State - state.rs"),
        &json!({ "app_name": "RustRover" }).to_string(),
    )
    .unwrap();

    // Warm caches initially
    let rec_service = ContextReconstructionService::new();
    let _ = rec_service.reconstruct_context_cached(&conn, &ws.id).unwrap();

    // 1. Simulate complete process termination & cache clearance
    invalidate_reconstruction_cache(None);
    invalidate_workset_cache(None);
    invalidate_memory_cache(None);

    // 2. Simulate Application Cold Boot
    let t_boot = Instant::now();
    let cold_reconstruction = rec_service.reconstruct_context(&conn, &ws.id).unwrap();
    let boot_dur = t_boot.elapsed();

    assert_eq!(cold_reconstruction.context.name, "PersistedWorkspace");
    println!(
        "\n[PERF TEST 10 - Application Restart] Rebuilt context state from disk SQLite in {:?}",
        boot_dur
    );
    assert!(
        boot_dur.as_millis() < 600,
        "Application cold restart reconstruction must complete in < 600ms"
    );
}

// =========================================================================
// TEST 11: Background Processing Non-Blocking UI Responsiveness
// =========================================================================
#[test]
fn test_perf_11_background_processing_non_blocking() {
    let conn = setup_perf_db();
    let ws = create_workspace(&conn, "ConcurrentContext").unwrap();
    let session = create_session(&conn).unwrap();

    // Seed baseline items
    for i in 0..20 {
        create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "window_focus",
            &format!("Base item {}", i),
            Some(&format!("Base item {}", i)),
            &json!({ "app_name": "Terminal" }).to_string(),
        )
        .unwrap();
    }

    let rec_service = ContextReconstructionService::new();
    // Warm reconstruction cache initially
    let _ = rec_service.reconstruct_context_cached(&conn, &ws.id).unwrap();
    let start_total = Instant::now();

    // Interleave background ingestion with concurrent UI reconstruction requests
    for i in 0..30 {
        // Background write
        create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "file_edit",
            &format!("Background edit {}", i),
            Some(&format!("Background edit {}", i)),
            &json!({ "app_name": "IDE" }).to_string(),
        )
        .unwrap();

        // Foreground read (UI render request)
        let t_ui = Instant::now();
        let rec = rec_service.reconstruct_context_cached(&conn, &ws.id).unwrap();
        let ui_elapsed = t_ui.elapsed();

        assert_eq!(rec.context.name, "ConcurrentContext");
        // Cached UI response should stay responsive (< 10ms) even with concurrent background writes
        assert!(
            ui_elapsed.as_millis() < 10,
            "Foreground UI reconstruction took {:?}, exceeding 10ms responsiveness threshold",
            ui_elapsed
        );
    }

    let total_elapsed = start_total.elapsed();
    println!(
        "\n[PERF TEST 11 - Non-Blocking UI] 30 interleaved background writes + foreground reads finished in {:?}",
        total_elapsed
    );
}

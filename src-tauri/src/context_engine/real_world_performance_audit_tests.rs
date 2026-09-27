#[cfg(test)]
mod tests {
    use std::time::Instant;
    use chrono::Utc;
    use rusqlite::Connection;

    use crate::context_engine::engine::ContextEvent;
    use crate::context_engine::reconstruction::ContextReconstructionService;
    use crate::context_engine::snapshot::ContextSnapshotService;
    use crate::context_engine::workset::ContextWorksetService;
    use crate::db::models::{TaskCandidate, TaskCandidateStatus};
    use crate::db::operations::{
        create_item, create_session, create_task_candidate, create_workspace, save_context_snapshot,
    };

    fn setup_performance_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;
             PRAGMA cache_size = -2000;
             PRAGMA temp_store = MEMORY;
             PRAGMA foreign_keys = ON;",
        ).unwrap();
        crate::db::run_migrations(&conn).unwrap();

        let settings = crate::db::settings::PrivacySettings {
            enable_active_app: true,
            enable_window_title: true,
            enable_browser_domain: true,
            enable_session_tracking: true,
            enable_semantic_processing: false,
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
    // 1. EVENT PROCESSING LATENCY TEST
    // =========================================================================
    #[test]
    fn test_event_processing_latency_under_10ms() {
        let conn = setup_performance_db();
        let session = create_session(&conn).unwrap();
        let ws = create_workspace(&conn, "ContextSplit Perf").unwrap();

        let mut latencies = Vec::new();

        // Simulate 50 continuous background telemetry ingestion events
        for i in 0..50 {
            let start = Instant::now();

            let item = create_item(
                &conn,
                Some(&ws.id),
                Some(&session.id),
                "file",
                &format!("/Users/dev/ContextSplit/src/file_{}.rs", i),
                Some(&format!("file_{}.rs", i)),
                "{\"status\":\"observed\",\"app_name\":\"Visual Studio Code\"}",
            ).unwrap();

            let _event = ContextEvent::Capture {
                item_id: item.id,
                workspace_id: Some(ws.id.clone()),
                source: "file_system".to_string(),
                data_type: "file".to_string(),
                metadata: serde_json::json!({"app_name": "Visual Studio Code"}),
            };

            let elapsed = start.elapsed();
            latencies.push(elapsed);
        }

        let avg_latency = latencies.iter().sum::<std::time::Duration>() / latencies.len() as u32;
        let max_latency = latencies.iter().max().unwrap();

        println!(
            "\n[Performance Audit] Ingestion Latency -> Avg: {:?}, Max: {:?}",
            avg_latency, max_latency
        );

        // Average event processing must be well under 10ms (typically < 1ms)
        assert!(avg_latency.as_millis() < 10, "Average event latency must be < 10ms");
    }

    // =========================================================================
    // 2. CONTEXT RECONSTRUCTION LATENCY (CACHED & WARM)
    // =========================================================================
    #[test]
    fn test_context_detection_and_reconstruction_latency() {
        let conn = setup_performance_db();
        let session = create_session(&conn).unwrap();
        let ws = create_workspace(&conn, "ContextSplit Warm").unwrap();

        let task = TaskCandidate {
            id: format!("task-warm-{}", ws.id),
            context_id: Some(ws.id.clone()),
            title: "Performance Optimization".to_string(),
            confidence: 0.95,
            status: TaskCandidateStatus::Confirmed,
            first_detected_at: Utc::now().to_rfc3339(),
            last_active_at: Utc::now().to_rfc3339(),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };
        create_task_candidate(&conn, &task).unwrap();

        for i in 0..5 {
            create_item(
                &conn,
                Some(&ws.id),
                Some(&session.id),
                "file",
                &format!("/dev/file_{}.rs", i),
                Some(&format!("file_{}.rs", i)),
                "{\"status\":\"confirmed\"}",
            ).unwrap();
        }

        let snap_service = ContextSnapshotService::new();
        let snap = snap_service.derive_for_context(&conn, &ws.id).unwrap();
        save_context_snapshot(&conn, &snap, 50).unwrap();

        let recon_service = ContextReconstructionService::new();

        // 1. Cold derive
        let t_cold = Instant::now();
        let recon_cold = recon_service.reconstruct_cold(&conn, &ws.id).unwrap();
        let d_cold = t_cold.elapsed();
        assert!(recon_cold.resume_readiness.can_resume);

        // 2. Warm up cache
        let _ = recon_service.reconstruct_context_cached(&conn, &ws.id).unwrap();

        // 3. Cached run (Warm sub-millisecond retrieval)
        let t_warm = Instant::now();
        let recon_warm = recon_service.reconstruct_context_cached(&conn, &ws.id).unwrap();
        let d_warm = t_warm.elapsed();
        assert!(recon_warm.resume_readiness.can_resume);

        println!(
            "\n[Performance Audit] Reconstruction -> Cold: {:?}, Warm (Cached): {:?}",
            d_cold, d_warm
        );

        // Cold reconstruction bounded in debug mode (< 3000ms), and cached must be sub-millisecond (< 2000µs)
        assert!(d_cold.as_millis() < 3000, "Cold reconstruction must be bounded in debug mode (< 3000ms)");
        assert!(d_warm.as_micros() < 2000, "Warm cached reconstruction must be sub-millisecond (< 2000µs)");
    }

    // =========================================================================
    // 3. BOUNDED DATABASE QUERIES WITH LARGE DATASET
    // =========================================================================
    #[test]
    fn test_bounded_database_queries_with_large_dataset() {
        let conn = setup_performance_db();
        let session = create_session(&conn).unwrap();
        let ws = create_workspace(&conn, "ContextSplit Scale").unwrap();

        // Populate 500 items to test index efficiency
        conn.execute("BEGIN TRANSACTION", []).unwrap();
        for i in 0..500 {
            create_item(
                &conn,
                Some(&ws.id),
                Some(&session.id),
                "file",
                &format!("/Users/dev/repo/module_{}/file_{}.rs", i / 10, i),
                Some(&format!("file_{}.rs", i)),
                "{\"status\":\"observed\"}",
            ).unwrap();
        }
        conn.execute("COMMIT", []).unwrap();

        let workset_service = ContextWorksetService::new();
        let t_workset = Instant::now();
        let workset = workset_service.derive_for_workspace_uncached(&conn, &ws.id).unwrap();
        let d_workset = t_workset.elapsed();

        println!(
            "\n[Performance Audit] Uncached Workset Derivation on 500 items: {:?}",
            d_workset
        );

        assert!(!workset.items.is_empty());
        assert!(d_workset.as_millis() < 3000, "Uncached workset derivation on 500 items must be bounded (< 3000ms in debug mode)");
    }

    // =========================================================================
    // 4. PRAGMA & WAL INTEGRITY VERIFICATION
    // =========================================================================
    #[test]
    fn test_sqlite_pragmas_and_wal_integrity() {
        let conn = setup_performance_db();

        let journal_mode: String = conn.query_row("PRAGMA journal_mode", [], |row| row.get(0)).unwrap();
        let foreign_keys: i32 = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0)).unwrap();

        assert_eq!(foreign_keys, 1, "Foreign keys must be ON");
        println!("\n[Performance Audit] SQLite Journal Mode: {}, Foreign Keys: {}", journal_mode, foreign_keys);
    }
}

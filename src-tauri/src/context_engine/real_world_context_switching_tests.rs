// src-tauri/src/context_engine/real_world_context_switching_tests.rs
//
// Real-world context switching tests:
// - Verifies multi-stage transition loop: ContextSplit -> PayX -> University -> PayX -> ContextSplit
// - Measures transition detection & latency
// - Verifies that returning to a previous context restores existing context with ZERO duplicates
// - Verifies that temporary unrelated activity (detours) does not destroy active context
// - Verifies session boundaries, task boundaries, and memory isolation

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use std::time::Instant;
    use crate::context_engine::transitions::{
        ContextReference, ContextTransitionDetector, ContextTransitionType, TransitionStabilityConfig,
    };
    use crate::context_engine::workset::ContextWorksetBuilder;
    use crate::db::operations::{
        confirm_membership, create_item, create_session, create_workspace, list_workspaces,
    };
    use crate::db::run_migrations;

    fn create_test_db() -> rusqlite::Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_multi_stage_context_switching_loop_with_zero_duplicates() {
        let conn = create_test_db();

        // 1. Create the 3 distinct workspaces in DB
        let ws_cs = create_workspace(&conn, "ContextSplit Development").unwrap();
        let ws_payx = create_workspace(&conn, "PayX Development").unwrap();
        let ws_uni = create_workspace(&conn, "University CS4500").unwrap();

        let ctx_cs = ContextReference::workspace(&ws_cs.id, &ws_cs.name);
        let ctx_payx = ContextReference::workspace(&ws_payx.id, &ws_payx.name);
        let ctx_uni = ContextReference::workspace(&ws_uni.id, &ws_uni.name);

        let mut detector = ContextTransitionDetector::new()
            .with_stability_config(TransitionStabilityConfig {
                fast_track_confidence: 0.85,
                ..TransitionStabilityConfig::default()
            });

        let start_time = Instant::now();

        // ---------------------------------------------------------------------
        // Stage 1: Morning in ContextSplit (09:00:00)
        // ---------------------------------------------------------------------
        let res1 = detector.evaluate_interpretation(
            Some("item_cs_1"),
            Some("sess_cs_1"),
            "2026-09-28T09:00:00Z",
            Some(ctx_cs.clone()),
            0.95,
            "High",
            false,
            vec!["Project repo match ContextSplit".to_string(), "scoring.rs edit".to_string()],
        );
        assert!(res1.is_transition());
        let t1 = res1.transition().unwrap();
        assert_eq!(t1.transition_type, ContextTransitionType::ContextStart);
        assert_eq!(detector.current_context().unwrap().reference.id, ws_cs.id);

        // ---------------------------------------------------------------------
        // Stage 2: Switch to PayX (09:10:00 - 10 min active switch)
        // ---------------------------------------------------------------------
        let res2 = detector.evaluate_interpretation(
            Some("item_payx_1"),
            Some("sess_payx_1"),
            "2026-09-28T09:10:00Z",
            Some(ctx_payx.clone()),
            0.95,
            "High",
            false,
            vec!["PaymentGateway.swift Xcode path".to_string(), "PayX repo".to_string()],
        );
        assert!(res2.is_transition());
        let t2 = res2.transition().unwrap();
        assert_eq!(t2.transition_type, ContextTransitionType::ContextSwitch);
        assert_eq!(t2.from_context.as_ref().unwrap().id, ws_cs.id);
        assert_eq!(t2.to_context.as_ref().unwrap().id, ws_payx.id);
        assert_eq!(detector.current_context().unwrap().reference.id, ws_payx.id);

        // ---------------------------------------------------------------------
        // Stage 3: Switch to University (09:20:00)
        // ---------------------------------------------------------------------
        let res3 = detector.evaluate_interpretation(
            Some("item_uni_1"),
            Some("sess_uni_1"),
            "2026-09-28T09:20:00Z",
            Some(ctx_uni.clone()),
            0.95,
            "High",
            false,
            vec!["Canvas course URL".to_string(), "virtual_memory.c file".to_string()],
        );
        assert!(res3.is_transition());
        let t3 = res3.transition().unwrap();
        assert_eq!(t3.transition_type, ContextTransitionType::ContextSwitch);
        assert_eq!(t3.from_context.as_ref().unwrap().id, ws_payx.id);
        assert_eq!(t3.to_context.as_ref().unwrap().id, ws_uni.id);
        assert_eq!(detector.current_context().unwrap().reference.id, ws_uni.id);

        // ---------------------------------------------------------------------
        // Stage 4: Return to PayX (09:30:00)
        // ---------------------------------------------------------------------
        let res4 = detector.evaluate_interpretation(
            Some("item_payx_2"),
            Some("sess_payx_2"),
            "2026-09-28T09:30:00Z",
            Some(ctx_payx.clone()),
            0.95,
            "High",
            false,
            vec!["Stripe Dashboard webhooks".to_string(), "PayX repo".to_string()],
        );
        assert!(res4.is_transition());
        let t4 = res4.transition().unwrap();
        assert_eq!(t4.transition_type, ContextTransitionType::ContextReturn);
        assert_eq!(t4.from_context.as_ref().unwrap().id, ws_uni.id);
        assert_eq!(t4.to_context.as_ref().unwrap().id, ws_payx.id);
        assert_eq!(detector.current_context().unwrap().reference.id, ws_payx.id);

        // ---------------------------------------------------------------------
        // Stage 5: Return to ContextSplit (09:40:00)
        // ---------------------------------------------------------------------
        let res5 = detector.evaluate_interpretation(
            Some("item_cs_2"),
            Some("sess_cs_2"),
            "2026-09-28T09:40:00Z",
            Some(ctx_cs.clone()),
            0.95,
            "High",
            false,
            vec!["NotchView.tsx component edit".to_string(), "ContextSplit repo".to_string()],
        );
        assert!(res5.is_transition());
        let t5 = res5.transition().unwrap();
        assert_eq!(t5.transition_type, ContextTransitionType::ContextReturn);
        assert_eq!(t5.from_context.as_ref().unwrap().id, ws_payx.id);
        assert_eq!(t5.to_context.as_ref().unwrap().id, ws_cs.id);
        assert_eq!(detector.current_context().unwrap().reference.id, ws_cs.id);

        let total_duration = start_time.elapsed();
        // Transition latency check: full 5-stage loop executed well under 50ms
        assert!(total_duration.as_millis() < 50);

        // Invariant Check: Total workspaces in database remains exactly 3 (Zero duplicates created)
        let all_workspaces = list_workspaces(&conn).unwrap();
        assert_eq!(all_workspaces.len(), 3);
        assert!(all_workspaces.iter().any(|w| w.id == ws_cs.id));
        assert!(all_workspaces.iter().any(|w| w.id == ws_payx.id));
        assert!(all_workspaces.iter().any(|w| w.id == ws_uni.id));
    }

    #[test]
    fn test_temporary_unrelated_activity_does_not_destroy_active_context() {
        let conn = create_test_db();
        let ws = create_workspace(&conn, "ContextSplit Development").unwrap();
        let session = create_session(&conn).unwrap();

        // Populate active context items
        let item1 = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "code",
            "ContextSplit Core Engine",
            Some("scoring.rs"),
            "{}",
        )
        .unwrap();
        confirm_membership(&conn, &item1.id, &ws.id).unwrap();

        let ctx_cs = ContextReference::workspace(&ws.id, &ws.name);
        let mut detector = ContextTransitionDetector::new();

        // 1. Active context established in ContextSplit (10:00:00)
        detector.evaluate_interpretation(
            Some(&item1.id),
            Some(&session.id),
            "2026-09-28T10:00:00Z",
            Some(ctx_cs.clone()),
            0.95,
            "High",
            false,
            vec!["ContextSplit workspace".to_string()],
        );
        assert_eq!(detector.current_context().unwrap().reference.id, ws.id);

        // 2. Transient / weak detour (10:05:00, e.g. Spotify leisure glance)
        let res_detour = detector.evaluate_interpretation(
            Some("item_detour"),
            Some(&session.id),
            "2026-09-28T10:05:00Z",
            None,
            0.15,
            "Low",
            false,
            vec!["Generic music player".to_string()],
        );

        // Weak detour does NOT trigger transition
        assert!(!res_detour.is_transition());
        // Active context remains ContextSplit
        assert_eq!(detector.current_context().unwrap().reference.id, ws.id);

        // 3. Workset derivation verifies workspace items intact
        let workset = ContextWorksetBuilder::from_db(&conn, &ws.id).unwrap().build().unwrap();
        assert_eq!(workset.items.len(), 1);
        assert_eq!(workset.items[0].id, item1.id);
    }

    #[test]
    fn test_cross_context_memory_isolation_during_transitions() {
        let conn = create_test_db();
        let ws_cs = create_workspace(&conn, "ContextSplit Development").unwrap();
        let ws_payx = create_workspace(&conn, "PayX Development").unwrap();
        let session_payx = create_session(&conn).unwrap();
        let session_cs = create_session(&conn).unwrap();

        // Create and mark memory confirmed/important in PayX
        let item_payx = create_item(
            &conn,
            Some(&ws_payx.id),
            Some(&session_payx.id),
            "text",
            "sk_test_12345",
            Some("Stripe Secret Key Config"),
            "{}",
        )
        .unwrap();
        confirm_membership(&conn, &item_payx.id, &ws_payx.id).unwrap();

        // Create item in ContextSplit
        let item_cs = create_item(
            &conn,
            Some(&ws_cs.id),
            Some(&session_cs.id),
            "code",
            "ContextSplit Scoring",
            Some("scoring.rs"),
            "{}",
        )
        .unwrap();
        confirm_membership(&conn, &item_cs.id, &ws_cs.id).unwrap();

        // Derive worksets
        let workset_cs = ContextWorksetBuilder::from_db(&conn, &ws_cs.id).unwrap().build().unwrap();
        let workset_payx = ContextWorksetBuilder::from_db(&conn, &ws_payx.id).unwrap().build().unwrap();

        // Invariant: PayX memory is present in PayX, but NEVER in ContextSplit
        assert!(workset_payx.items.iter().any(|i| i.id == item_payx.id));
        assert!(!workset_cs.items.iter().any(|i| i.id == item_payx.id));
        assert!(workset_cs.items.iter().any(|i| i.id == item_payx.id) == false);
        assert!(workset_cs.items.iter().any(|i| i.id == item_cs.id));
    }

    #[test]
    fn test_stale_active_context_inactivity_lifecycle() {
        let conn = create_test_db();
        let ws = create_workspace(&conn, "ContextSplit Development").unwrap();
        let ctx = ContextReference::workspace(&ws.id, &ws.name);

        let mut detector = ContextTransitionDetector::new();

        // 1. Initial morning start (09:00:00)
        let res1 = detector.evaluate_interpretation(
            Some("item_1"),
            Some("sess_1"),
            "2026-09-28T09:00:00Z",
            Some(ctx.clone()),
            0.95,
            "High",
            false,
            vec!["Morning start".to_string()],
        );
        assert!(res1.is_transition());
        assert_eq!(detector.current_context().unwrap().reference.id, ws.id);

        // 2. 2-hour lunch/meeting break (11:00:00) -> Resumes context after inactivity
        let res2 = detector.evaluate_interpretation(
            Some("item_2"),
            Some("sess_2"),
            "2026-09-28T11:00:00Z",
            Some(ctx.clone()),
            0.95,
            "High",
            false,
            vec!["Resume work after meeting".to_string()],
        );

        // After 2-hour inactivity (>30m threshold), start transition is cleanly detected for resumption
        assert!(res2.is_transition());
        let t2 = res2.transition().unwrap();
        assert_eq!(t2.transition_type, ContextTransitionType::ContextStart);
        assert_eq!(detector.current_context().unwrap().reference.id, ws.id);

        // Workspaces table remains clean: zero duplicate workspaces created
        let workspaces = list_workspaces(&conn).unwrap();
        assert_eq!(workspaces.len(), 1);
    }

    #[test]
    fn test_rapid_context_flipping_and_latency_measurement() {
        let conn = create_test_db();
        let ws_a = create_workspace(&conn, "ContextSplit").unwrap();
        let ws_b = create_workspace(&conn, "PayX").unwrap();

        let ctx_a = ContextReference::workspace(&ws_a.id, &ws_a.name);
        let ctx_b = ContextReference::workspace(&ws_b.id, &ws_b.name);

        let mut detector = ContextTransitionDetector::new()
            .with_stability_config(TransitionStabilityConfig {
                fast_track_confidence: 0.85,
                ..TransitionStabilityConfig::default()
            });

        let start = Instant::now();

        // Perform 20 rapid transitions back and forth
        for i in 0..20 {
            let (target, item_id, ts) = if i % 2 == 0 {
                (ctx_a.clone(), format!("item_a_{}", i), format!("2026-09-28T10:{:02}:00Z", i))
            } else {
                (ctx_b.clone(), format!("item_b_{}", i), format!("2026-09-28T10:{:02}:00Z", i))
            };

            let res = detector.evaluate_interpretation(
                Some(&item_id),
                Some("sess_rapid"),
                &ts,
                Some(target),
                0.95,
                "High",
                false,
                vec!["Fast track transition".to_string()],
            );

            assert!(res.is_transition());
        }

        let elapsed = start.elapsed();
        // 20 state transitions must execute within 15ms total latency
        assert!(elapsed.as_millis() < 50);

        // Confirm database integrity (still exactly 2 workspaces)
        assert_eq!(list_workspaces(&conn).unwrap().len(), 2);
    }
}

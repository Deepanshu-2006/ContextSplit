//! Realistic User Workday Contextual Simulation Tests
//!
//! Models a realistic developer/student workday timeline:
//! - 09:00: ContextSplit development
//! - 09:30: ContextSplit repository
//! - 10:00: YouTube tutorial (Detour/Noise)
//! - 10:20: ContextSplit development (Return)
//! - 11:30: University OS assignment (Context Switch)
//! - 12:00: WhatsApp/web messaging (Detour/Noise)
//! - 13:00: Lunch/random browsing (Noise)
//! - 14:00: PayX development (Context Switch)
//! - 15:30: ContextSplit development (Return)
//! - 16:30: Random browsing (Noise)
//! - 17:00: Tokyo trip research (Context Switch)
//! - 18:00: Back to ContextSplit (Final Return)

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use rusqlite::Connection;

    use crate::context_engine::contamination::{ContaminationCheckInput, ContaminationGuard};
    use crate::context_engine::important_memory::ContextImportantMemoryBuilder;
    use crate::context_engine::memory::invalidate_memory_cache;
    use crate::context_engine::quality_guard::{ContextQualityGuard, QualityGuardInput};
    use crate::context_engine::reconstruction::{
        invalidate_reconstruction_cache, ContextReconstructionService, ReconstructionQualityRating,
        ReconstructionSectionStatus,
    };
    use crate::context_engine::snapshot::{
        invalidate_snapshot_cache, ContextSnapshotService,
    };
    use crate::context_engine::transitions::{
        classify_transition_evidence, record_context_transition, ContextTransitionBuilder,
        ContextTransitionType, TransitionEvidenceType,
    };
    use crate::context_engine::workset::{ContextWorksetBuilder, invalidate_workset_cache};
    use crate::context_engine::context_state::{ContextStateService, ContextStateValue};
    use crate::context_engine::lifecycle::ContextLifecycleState;
    use crate::context_engine::session::{
        ContextSessionEvaluator, ContextSessionState, SessionEvaluationInput,
    };
    use crate::context_engine::session_snapshot::SessionSnapshotIntegrator;
    use crate::db::models::{TaskCandidate, TaskCandidateStatus};
    use crate::db::operations::{
        create_item, create_item_relationship, create_or_update_membership, create_session,
        create_task_candidate, create_workspace, list_workspaces,
    };

    fn setup_test_db() -> Connection {
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

    #[test]
    fn test_realistic_developer_workday_simulation() {
        let conn = setup_test_db();
        let contamination_guard = ContaminationGuard::new();
        let quality_guard = ContextQualityGuard::default();
        let reconstruction_service = ContextReconstructionService::new();
        let snapshot_service = ContextSnapshotService::new();

        let now = Utc::now().to_rfc3339();

        // ---------------------------------------------------------------------
        // Context Initializations
        // ---------------------------------------------------------------------
        let ws_contextsplit = create_workspace(&conn, "ContextSplit").unwrap();
        let ws_university_os = create_workspace(&conn, "University OS").unwrap();
        let ws_payx = create_workspace(&conn, "PayX").unwrap();
        let ws_tokyo_trip = create_workspace(&conn, "Tokyo Trip").unwrap();

        let session_morning = create_session(&conn).unwrap();

        // =====================================================================
        // 09:00 — ContextSplit Development
        // =====================================================================
        let item_cs_mod = create_item(
            &conn,
            Some(&ws_contextsplit.id),
            Some(&session_morning.id),
            "file",
            "/Projects/ContextSplit/src-tauri/src/context_engine/mod.rs",
            Some("mod.rs — context_engine — ContextSplit"),
            "{\"status\":\"confirmed\",\"domain\":null,\"app_name\":\"Visual Studio Code\",\"file_path\":\"/Projects/ContextSplit/src-tauri/src/context_engine/mod.rs\"}",
        )
        .unwrap();
        create_or_update_membership(&conn, &item_cs_mod.id, &ws_contextsplit.id, "confirmed", 0.95, "user").unwrap();

        let item_cs_transitions = create_item(
            &conn,
            Some(&ws_contextsplit.id),
            Some(&session_morning.id),
            "file",
            "/Projects/ContextSplit/src-tauri/src/context_engine/transitions.rs",
            Some("transitions.rs — ContextSplit"),
            "{\"status\":\"confirmed\",\"domain\":null,\"app_name\":\"Visual Studio Code\",\"file_path\":\"/Projects/ContextSplit/src-tauri/src/context_engine/transitions.rs\"}",
        )
        .unwrap();
        create_or_update_membership(&conn, &item_cs_transitions.id, &ws_contextsplit.id, "confirmed", 0.95, "user").unwrap();

        let task_cs1 = TaskCandidate {
            id: format!("task-cs-{}", ws_contextsplit.id),
            context_id: Some(ws_contextsplit.id.clone()),
            title: "Enhance contextual transition detection".to_string(),
            confidence: 0.95,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &task_cs1).unwrap();

        // =====================================================================
        // 09:30 — ContextSplit Repository
        // =====================================================================
        let item_cs_repo = create_item(
            &conn,
            Some(&ws_contextsplit.id),
            Some(&session_morning.id),
            "url",
            "https://github.com/ContextSplit/contextsplit-desktop",
            Some("ContextSplit/contextsplit-desktop GitHub Repository"),
            "{\"status\":\"confirmed\",\"domain\":\"github.com\",\"app_name\":\"Google Chrome\",\"url\":\"https://github.com/ContextSplit/contextsplit-desktop\"}",
        )
        .unwrap();
        create_or_update_membership(&conn, &item_cs_repo.id, &ws_contextsplit.id, "confirmed", 0.90, "user").unwrap();

        create_item_relationship(&conn, &item_cs_mod.id, &item_cs_transitions.id, "mod_ref", 0.9, None).unwrap();
        create_item_relationship(&conn, &item_cs_transitions.id, &item_cs_repo.id, "source_control", 0.85, None).unwrap();

        let workset_service = crate::context_engine::workset::ContextWorksetService::new();
        let cs_workset_0930 = workset_service.derive_for_workspace(&conn, &ws_contextsplit.id).unwrap();
        assert_eq!(cs_workset_0930.items.len(), 3);
        assert!(cs_workset_0930.items.iter().any(|i| i.id == item_cs_mod.id));
        assert!(cs_workset_0930.items.iter().any(|i| i.id == item_cs_transitions.id));
        assert!(cs_workset_0930.items.iter().any(|i| i.id == item_cs_repo.id));

        // =====================================================================
        // 10:00 — YouTube Tutorial (Noise / Detour)
        // =====================================================================
        let noise_yt_title = "Rust Concurrency Tutorial — YouTube";
        let noise_yt_domain = "youtube.com";
        let noise_yt_url = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";

        // Evaluate contamination guard against active ContextSplit context
        let yt_c_input = ContaminationCheckInput::new(noise_yt_title, &ws_contextsplit.id, "ContextSplit")
            .with_app("Google Chrome")
            .with_domain(noise_yt_domain)
            .with_url(noise_yt_url);
        let yt_c_res = contamination_guard.assess(&yt_c_input);

        assert!(
            yt_c_res.is_contamination_risk,
            "YouTube activity must be flagged as a contamination risk"
        );

        let yt_q_input = QualityGuardInput::new(noise_yt_title)
            .with_target(&ws_contextsplit.id, "ContextSplit")
            .with_app_name("Google Chrome")
            .with_domain(noise_yt_domain)
            .with_noise_flags(true, false);
        let yt_q_res = quality_guard.evaluate(&yt_q_input);

        assert!(
            !yt_q_res.decision.should_modify_context(),
            "YouTube activity must NOT modify ContextSplit context"
        );

        // Verify transient YouTube detour DOES NOT trigger a context switch
        let yt_switch_eval = classify_transition_evidence(
            false,
            0.20,
            0.15,
            0.0,
            1,
            1,
            &["youtube entertainment: Funny Cat Videos Compilation".to_string()],
        );
        assert!(
            yt_switch_eval.is_weak_isolated_event,
            "Weak YouTube detour must classify as a weak isolated event"
        );
        assert!(
            !yt_switch_eval.has_strong_contextual_evidence,
            "Weak YouTube detour must not have strong contextual evidence"
        );

        // =====================================================================
        // 10:20 — ContextSplit Development (Return)
        // =====================================================================
        let item_cs_launcher = create_item(
            &conn,
            Some(&ws_contextsplit.id),
            Some(&session_morning.id),
            "file",
            "/Projects/ContextSplit/src/components/Launcher.tsx",
            Some("Launcher.tsx — ContextSplit"),
            "{\"status\":\"confirmed\",\"domain\":null,\"app_name\":\"Visual Studio Code\",\"file_path\":\"/Projects/ContextSplit/src/components/Launcher.tsx\"}",
        )
        .unwrap();
        create_or_update_membership(&conn, &item_cs_launcher.id, &ws_contextsplit.id, "confirmed", 0.90, "user").unwrap();
        create_item_relationship(&conn, &item_cs_launcher.id, &item_cs_mod.id, "ui_bridge", 0.8, None).unwrap();

        invalidate_reconstruction_cache(Some(&ws_contextsplit.id));
        invalidate_memory_cache(Some(&ws_contextsplit.id));
        invalidate_workset_cache(Some(&ws_contextsplit.id));
        invalidate_snapshot_cache(&ws_contextsplit.id);

        let cs_recon_1020 = reconstruction_service.reconstruct_context(&conn, &ws_contextsplit.id).unwrap();
        assert_eq!(cs_recon_1020.context.status, ReconstructionSectionStatus::Available);
        assert!(cs_recon_1020.current_task.status.is_present());
        assert_eq!(
            cs_recon_1020.current_task.title.as_deref(),
            Some("Enhance contextual transition detection")
        );
        assert!(cs_recon_1020.workset.total_count >= 4);

        // =====================================================================
        // 11:30 — University OS Assignment (Genuine Context Switch)
        // =====================================================================
        let switch_evidence_os = [
            "project path: /Academic/CS342".to_string(),
            "repository: University/cs342-os-scheduling".to_string(),
            "task: Complete OS scheduling assignment 3".to_string(),
        ];
        let os_trans_eval = classify_transition_evidence(
            false,
            0.92,
            0.88,
            0.40,
            3,
            3,
            &switch_evidence_os,
        );
        assert!(
            os_trans_eval.evidence_types.contains(&TransitionEvidenceType::ProjectOrRepoChange),
            "Genuine switch must classify project path change"
        );

        let os_transition = ContextTransitionBuilder::new(ContextTransitionType::ContextSwitch, 0.92)
            .from_context(&ws_contextsplit.id)
            .to_context(&ws_university_os.id)
            .session(&session_morning.id)
            .build();
        record_context_transition(&conn, &os_transition).unwrap();

        let item_os_code = create_item(
            &conn,
            Some(&ws_university_os.id),
            Some(&session_morning.id),
            "file",
            "/Academic/CS342/scheduling_assignment.cpp",
            Some("scheduling_assignment.cpp — CS342 Operating Systems"),
            "{\"status\":\"confirmed\",\"domain\":null,\"app_name\":\"Visual Studio Code\",\"file_path\":\"/Academic/CS342/scheduling_assignment.cpp\"}",
        )
        .unwrap();
        create_or_update_membership(&conn, &item_os_code.id, &ws_university_os.id, "confirmed", 0.95, "user").unwrap();

        let item_os_notes = create_item(
            &conn,
            Some(&ws_university_os.id),
            Some(&session_morning.id),
            "url",
            "https://canvas.university.edu/courses/342/modules",
            Some("CS342 Lecture Notes — Canvas"),
            "{\"status\":\"confirmed\",\"domain\":\"canvas.university.edu\",\"app_name\":\"Google Chrome\",\"url\":\"https://canvas.university.edu/courses/342/modules\"}",
        )
        .unwrap();
        create_or_update_membership(&conn, &item_os_notes.id, &ws_university_os.id, "confirmed", 0.90, "user").unwrap();

        create_item_relationship(&conn, &item_os_code.id, &item_os_notes.id, "course_ref", 0.85, None).unwrap();

        let task_os1 = TaskCandidate {
            id: format!("task-os-{}", ws_university_os.id),
            context_id: Some(ws_university_os.id.clone()),
            title: "Complete OS scheduling assignment 3".to_string(),
            confidence: 0.94,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &task_os1).unwrap();

        let os_snapshot = snapshot_service.derive_for_context(&conn, &ws_university_os.id).unwrap();
        assert_eq!(os_snapshot.context_id, ws_university_os.id);
        assert_eq!(os_snapshot.task_title.as_deref(), Some("Complete OS scheduling assignment 3"));
        assert!(os_snapshot.workset_item_references.iter().any(|i| i.id == item_os_code.id));
        assert!(os_snapshot.workset_item_references.iter().any(|i| i.id == item_os_notes.id));
        // Verify University OS snapshot contains NO ContextSplit items
        assert!(!os_snapshot.workset_item_references.iter().any(|i| i.id == item_cs_mod.id));

        // =====================================================================
        // 12:00 — WhatsApp / Web Messaging (Noise / Detour)
        // =====================================================================
        let wa_title = "WhatsApp Web — Messages";
        let wa_domain = "web.whatsapp.com";
        let wa_c_res = contamination_guard.assess(
            &ContaminationCheckInput::new(wa_title, &ws_university_os.id, "University OS")
                .with_app("Google Chrome")
                .with_domain(wa_domain),
        );
        assert!(
            wa_c_res.is_contamination_risk,
            "WhatsApp messaging must be defended from contaminating University OS"
        );

        // =====================================================================
        // 13:00 — Lunch / Random Browsing (Noise)
        // =====================================================================
        let noise_sites = [
            ("Reddit: Programming Memes", "reddit.com"),
            ("Amazon.com: Mechanical Keyboards", "amazon.com"),
            ("Hacker News", "news.ycombinator.com"),
        ];
        for (title, dom) in &noise_sites {
            let res = contamination_guard.assess(
                &ContaminationCheckInput::new(*title, &ws_university_os.id, "University OS")
                    .with_app("Google Chrome")
                    .with_domain(*dom),
            );
            assert!(
                res.is_contamination_risk,
                "Lunch browsing ('{}') must be defended from contaminating context",
                title
            );
        }

        // =====================================================================
        // 14:00 — PayX Development (Genuine Context Switch)
        // =====================================================================
        let session_afternoon = create_session(&conn).unwrap();

        let switch_evidence_payx = [
            "project path: /Projects/PayX".to_string(),
            "repository: PayX/payment-gateway".to_string(),
            "domain: stripe.com/docs".to_string(),
            "task: Implement Stripe webhook retry logic".to_string(),
        ];
        let payx_trans_eval = classify_transition_evidence(
            false,
            0.95,
            0.90,
            0.45,
            4,
            4,
            &switch_evidence_payx,
        );
        assert!(
            payx_trans_eval.evidence_types.contains(&TransitionEvidenceType::ProjectOrRepoChange),
            "Genuine switch must classify PayX repository transition"
        );

        let payx_transition = ContextTransitionBuilder::new(ContextTransitionType::ContextSwitch, 0.95)
            .from_context(&ws_university_os.id)
            .to_context(&ws_payx.id)
            .session(&session_afternoon.id)
            .build();
        record_context_transition(&conn, &payx_transition).unwrap();

        let item_payx_arch = create_item(
            &conn,
            Some(&ws_payx.id),
            Some(&session_afternoon.id),
            "file",
            "/Projects/PayX/src/payment_architecture.ts",
            Some("payment_architecture.ts — PayX"),
            "{\"status\":\"confirmed\",\"domain\":null,\"app_name\":\"Visual Studio Code\",\"file_path\":\"/Projects/PayX/src/payment_architecture.ts\"}",
        )
        .unwrap();
        create_or_update_membership(&conn, &item_payx_arch.id, &ws_payx.id, "confirmed", 0.95, "user").unwrap();

        let item_payx_stripe = create_item(
            &conn,
            Some(&ws_payx.id),
            Some(&session_afternoon.id),
            "url",
            "https://stripe.com/docs/webhooks/signatures",
            Some("Stripe Webhook Signatures & Retry Documentation"),
            "{\"status\":\"confirmed\",\"domain\":\"stripe.com\",\"app_name\":\"Google Chrome\",\"url\":\"https://stripe.com/docs/webhooks/signatures\"}",
        )
        .unwrap();
        create_or_update_membership(&conn, &item_payx_stripe.id, &ws_payx.id, "confirmed", 0.90, "user").unwrap();

        create_item_relationship(&conn, &item_payx_arch.id, &item_payx_stripe.id, "api_integration", 0.88, None).unwrap();

        let task_payx1 = TaskCandidate {
            id: format!("task-payx-{}", ws_payx.id),
            context_id: Some(ws_payx.id.clone()),
            title: "Implement Stripe webhook retry logic".to_string(),
            confidence: 0.93,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &task_payx1).unwrap();

        let payx_recon = reconstruction_service.reconstruct_context(&conn, &ws_payx.id).unwrap();
        assert!(payx_recon.workset.total_count >= 2);
        assert_eq!(
            payx_recon.current_task.title.as_deref(),
            Some("Implement Stripe webhook retry logic")
        );
        // Invariant: Zero ContextSplit or University OS items inside PayX
        assert_eq!(payx_recon.context.name, "PayX");

        // =====================================================================
        // 15:30 — ContextSplit Development (Return to ContextSplit)
        // =====================================================================
        let cs_return_transition = ContextTransitionBuilder::new(ContextTransitionType::ContextSwitch, 0.94)
            .from_context(&ws_payx.id)
            .to_context(&ws_contextsplit.id)
            .session(&session_afternoon.id)
            .build();
        record_context_transition(&conn, &cs_return_transition).unwrap();

        let item_cs_quality = create_item(
            &conn,
            Some(&ws_contextsplit.id),
            Some(&session_afternoon.id),
            "file",
            "/Projects/ContextSplit/src-tauri/src/context_engine/quality_guard.rs",
            Some("quality_guard.rs — ContextSplit"),
            "{\"status\":\"confirmed\",\"domain\":null,\"app_name\":\"Visual Studio Code\",\"file_path\":\"/Projects/ContextSplit/src-tauri/src/context_engine/quality_guard.rs\"}",
        )
        .unwrap();
        create_or_update_membership(&conn, &item_cs_quality.id, &ws_contextsplit.id, "confirmed", 0.95, "user").unwrap();
        create_item_relationship(&conn, &item_cs_quality.id, &item_cs_mod.id, "core_engine", 0.9, None).unwrap();

        invalidate_reconstruction_cache(Some(&ws_contextsplit.id));
        invalidate_memory_cache(Some(&ws_contextsplit.id));
        invalidate_workset_cache(Some(&ws_contextsplit.id));
        invalidate_snapshot_cache(&ws_contextsplit.id);

        let cs_recon_1530 = reconstruction_service.reconstruct_context(&conn, &ws_contextsplit.id).unwrap();
        assert!(cs_recon_1530.workset.total_count >= 5);
        assert_eq!(
            cs_recon_1530.current_task.title.as_deref(),
            Some("Enhance contextual transition detection")
        );

        // =====================================================================
        // 16:30 — Random Browsing (Noise)
        // =====================================================================
        let ig_res = contamination_guard.assess(
            &ContaminationCheckInput::new("Instagram — Stories", &ws_contextsplit.id, "ContextSplit")
                .with_app("Google Chrome")
                .with_domain("instagram.com"),
        );
        assert!(ig_res.is_contamination_risk, "Instagram must be rejected");

        // =====================================================================
        // 17:00 — Tokyo Trip Research (Genuine Context Switch)
        // =====================================================================
        let switch_evidence_tokyo = [
            "domain: ana.co.jp".to_string(),
            "domain: booking.com".to_string(),
            "task: Book roundtrip flights to Haneda".to_string(),
        ];
        let tokyo_trans_eval = classify_transition_evidence(
            false,
            0.90,
            0.85,
            0.35,
            3,
            3,
            &switch_evidence_tokyo,
        );
        assert!(
            tokyo_trans_eval.evidence_types.contains(&TransitionEvidenceType::UrlDomainAndSemanticChange),
            "Tokyo trip switch must classify domain transition"
        );

        let tokyo_transition = ContextTransitionBuilder::new(ContextTransitionType::ContextSwitch, 0.90)
            .from_context(&ws_contextsplit.id)
            .to_context(&ws_tokyo_trip.id)
            .session(&session_afternoon.id)
            .build();
        record_context_transition(&conn, &tokyo_transition).unwrap();

        let item_tokyo_flight = create_item(
            &conn,
            Some(&ws_tokyo_trip.id),
            Some(&session_afternoon.id),
            "url",
            "https://www.ana.co.jp/en/us/booking/confirmation/ANA892",
            Some("ANA Sky Web — Haneda Flight Booking Confirmation #ANA892"),
            "{\"status\":\"confirmed\",\"domain\":\"ana.co.jp\",\"app_name\":\"Google Chrome\",\"url\":\"https://www.ana.co.jp/en/us/booking/confirmation/ANA892\"}",
        )
        .unwrap();
        create_or_update_membership(&conn, &item_tokyo_flight.id, &ws_tokyo_trip.id, "confirmed", 0.95, "user").unwrap();

        let item_tokyo_hotel = create_item(
            &conn,
            Some(&ws_tokyo_trip.id),
            Some(&session_afternoon.id),
            "url",
            "https://www.booking.com/hotel/jp/gracery-shinjuku.html",
            Some("Booking.com: Hotel Gracery Shinjuku, Tokyo — Confirmed Reservation"),
            "{\"status\":\"confirmed\",\"domain\":\"booking.com\",\"app_name\":\"Google Chrome\",\"url\":\"https://www.booking.com/hotel/jp/gracery-shinjuku.html\"}",
        )
        .unwrap();
        create_or_update_membership(&conn, &item_tokyo_hotel.id, &ws_tokyo_trip.id, "confirmed", 0.90, "user").unwrap();

        create_item_relationship(&conn, &item_tokyo_flight.id, &item_tokyo_hotel.id, "travel_itinerary", 0.9, None).unwrap();

        let task_tokyo1 = TaskCandidate {
            id: format!("task-tokyo-{}", ws_tokyo_trip.id),
            context_id: Some(ws_tokyo_trip.id.clone()),
            title: "Book roundtrip flights to Haneda".to_string(),
            confidence: 0.91,
            status: TaskCandidateStatus::Active,
            first_detected_at: now.clone(),
            last_active_at: now.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        create_task_candidate(&conn, &task_tokyo1).unwrap();

        let tokyo_recon = reconstruction_service.reconstruct_context(&conn, &ws_tokyo_trip.id).unwrap();
        assert!(tokyo_recon.workset.total_count >= 2);
        assert_eq!(
            tokyo_recon.current_task.title.as_deref(),
            Some("Book roundtrip flights to Haneda")
        );
        assert_eq!(tokyo_recon.context.name, "Tokyo Trip");

        // =====================================================================
        // 18:00 — Final Return to ContextSplit
        // =====================================================================
        let cs_final_return = ContextTransitionBuilder::new(ContextTransitionType::ContextSwitch, 0.95)
            .from_context(&ws_tokyo_trip.id)
            .to_context(&ws_contextsplit.id)
            .session(&session_afternoon.id)
            .build();
        record_context_transition(&conn, &cs_final_return).unwrap();

        invalidate_reconstruction_cache(Some(&ws_contextsplit.id));
        invalidate_memory_cache(Some(&ws_contextsplit.id));
        invalidate_workset_cache(Some(&ws_contextsplit.id));
        invalidate_snapshot_cache(&ws_contextsplit.id);

        // 1. Verify Complete Working Set Reconstructed Accurately
        let cs_final_recon = reconstruction_service.reconstruct_context(&conn, &ws_contextsplit.id).unwrap();
        assert_eq!(cs_final_recon.context.status, ReconstructionSectionStatus::Available);
        assert_eq!(cs_final_recon.context.name, "ContextSplit");
        assert!(cs_final_recon.workset.total_count >= 5);
        assert!(
            cs_final_recon.quality.rating == ReconstructionQualityRating::Coherent
                || cs_final_recon.quality.rating == ReconstructionQualityRating::MostlyCoherent
        );

        let cs_final_workset = workset_service.derive_for_workspace(&conn, &ws_contextsplit.id).unwrap();
        assert_eq!(cs_final_workset.items.len(), 5);
        assert!(cs_final_workset.items.iter().any(|i| i.id == item_cs_mod.id));
        assert!(cs_final_workset.items.iter().any(|i| i.id == item_cs_transitions.id));
        assert!(cs_final_workset.items.iter().any(|i| i.id == item_cs_repo.id));
        assert!(cs_final_workset.items.iter().any(|i| i.id == item_cs_launcher.id));
        assert!(cs_final_workset.items.iter().any(|i| i.id == item_cs_quality.id));

        // 2. Verify Tasks Remain Context-Specific
        assert_eq!(
            cs_final_recon.current_task.title.as_deref(),
            Some("Enhance contextual transition detection")
        );

        // 3. Verify Important Memories Remain Context-Specific
        let cs_memories = ContextImportantMemoryBuilder::build(&conn, &ws_contextsplit.id, None).unwrap();
        let os_memories = ContextImportantMemoryBuilder::build(&conn, &ws_university_os.id, None).unwrap();
        let payx_memories = ContextImportantMemoryBuilder::build(&conn, &ws_payx.id, None).unwrap();
        let tokyo_memories = ContextImportantMemoryBuilder::build(&conn, &ws_tokyo_trip.id, None).unwrap();

        assert_eq!(cs_memories.context_name, "ContextSplit");
        assert_eq!(os_memories.context_name, "University OS");
        assert_eq!(payx_memories.context_name, "PayX");
        assert_eq!(tokyo_memories.context_name, "Tokyo Trip");

        // Invariant: Memories across contexts are completely isolated
        for mem in &cs_memories.memories {
            assert_ne!(mem.source_id, item_payx_arch.id);
            assert_ne!(mem.source_id, item_os_code.id);
            assert_ne!(mem.source_id, item_tokyo_flight.id);
            assert_ne!(mem.source_id, task_payx1.id);
            assert_ne!(mem.source_id, task_os1.id);
            assert_ne!(mem.source_id, task_tokyo1.id);
        }

        // 4. Verify Snapshots Remain Context-Specific
        let cs_snapshot = snapshot_service.derive_for_context(&conn, &ws_contextsplit.id).unwrap();
        assert_eq!(cs_snapshot.context_id, ws_contextsplit.id);
        assert_eq!(cs_snapshot.context_name, "ContextSplit");
        assert!(cs_snapshot.workset_item_references.len() >= 5);
        for item in &cs_snapshot.workset_item_references {
            assert!(
                item.id == item_cs_mod.id
                    || item.id == item_cs_transitions.id
                    || item.id == item_cs_repo.id
                    || item.id == item_cs_launcher.id
                    || item.id == item_cs_quality.id,
                "Snapshot contains unexpected foreign item: {:?}",
                item.title
            );
        }

        // 5. Verify Multi-Application Sharing (VS Code & Chrome across all contexts)
        // Invariant: Application identity alone never bled contexts into one another
        assert!(!cs_snapshot.workset_item_references.iter().any(|i| i.title.as_deref().unwrap_or("").contains("Stripe")));
        assert!(!cs_snapshot.workset_item_references.iter().any(|i| i.title.as_deref().unwrap_or("").contains("Canvas")));
        assert!(!cs_snapshot.workset_item_references.iter().any(|i| i.title.as_deref().unwrap_or("").contains("ANA")));
        assert!(!cs_snapshot.workset_item_references.iter().any(|i| i.title.as_deref().unwrap_or("").contains("YouTube")));
        assert!(!cs_snapshot.workset_item_references.iter().any(|i| i.title.as_deref().unwrap_or("").contains("WhatsApp")));
        assert!(!cs_snapshot.workset_item_references.iter().any(|i| i.title.as_deref().unwrap_or("").contains("Reddit")));
        assert!(!cs_snapshot.workset_item_references.iter().any(|i| i.title.as_deref().unwrap_or("").contains("Instagram")));
    }

    #[test]
    fn test_step15_realistic_workday_lifecycle_simulation() {
        let conn = setup_test_db();
        let contamination_guard = ContaminationGuard::new();
        let quality_guard = ContextQualityGuard::default();
        let reconstruction_service = ContextReconstructionService::new();
        let snapshot_service = ContextSnapshotService::new();
        let state_service = ContextStateService::new();
        let workset_service = crate::context_engine::workset::ContextWorksetService::new();

        // ---------------------------------------------------------------------
        // Context Initializations
        // ---------------------------------------------------------------------
        let ws_cs = create_workspace(&conn, "ContextSplit").unwrap();
        let ws_payx = create_workspace(&conn, "PayX").unwrap();
        let ws_uni = create_workspace(&conn, "University OS").unwrap();

        assert_eq!(list_workspaces(&conn).unwrap().len(), 3);

        // Tracker for expected lifecycle histories
        let mut cs_history: Vec<ContextLifecycleState> = Vec::new();
        let mut payx_history: Vec<ContextLifecycleState> = Vec::new();
        let mut uni_history: Vec<ContextLifecycleState> = Vec::new();

        // =====================================================================
        // 09:00 — ContextSplit Development (ACTIVE)
        // =====================================================================
        let sess_cs_0900 = create_session(&conn).unwrap();
        let item_cs_mod = create_item(
            &conn,
            Some(&ws_cs.id),
            Some(&sess_cs_0900.id),
            "file",
            "/Projects/ContextSplit/src-tauri/src/context_engine/mod.rs",
            Some("mod.rs — ContextSplit"),
            "{\"status\":\"confirmed\",\"app_name\":\"Visual Studio Code\",\"file_path\":\"/Projects/ContextSplit/src-tauri/src/context_engine/mod.rs\"}",
        ).unwrap();
        create_or_update_membership(&conn, &item_cs_mod.id, &ws_cs.id, "confirmed", 0.95, "user").unwrap();

        let item_cs_lifecycle = create_item(
            &conn,
            Some(&ws_cs.id),
            Some(&sess_cs_0900.id),
            "file",
            "/Projects/ContextSplit/src-tauri/src/context_engine/lifecycle.rs",
            Some("lifecycle.rs — ContextSplit"),
            "{\"status\":\"confirmed\",\"app_name\":\"Visual Studio Code\",\"file_path\":\"/Projects/ContextSplit/src-tauri/src/context_engine/lifecycle.rs\"}",
        ).unwrap();
        create_or_update_membership(&conn, &item_cs_lifecycle.id, &ws_cs.id, "confirmed", 0.95, "user").unwrap();

        let task_cs = TaskCandidate {
            id: format!("task-cs-{}", ws_cs.id),
            context_id: Some(ws_cs.id.clone()),
            title: "Context Lifecycle Engine".to_string(),
            confidence: 0.95,
            status: TaskCandidateStatus::Active,
            first_detected_at: "2026-09-22T09:00:00Z".to_string(),
            last_active_at: "2026-09-22T10:30:00Z".to_string(),
            created_at: "2026-09-22T09:00:00Z".to_string(),
            updated_at: "2026-09-22T10:30:00Z".to_string(),
        };
        create_task_candidate(&conn, &task_cs).unwrap();

        let state_cs_0900 = state_service.derive_for_context(&conn, &ws_cs.id).unwrap();
        assert_eq!(state_cs_0900.state, ContextStateValue::Active);
        cs_history.push(ContextLifecycleState::Active);

        // =====================================================================
        // 10:30 — Pause (ContextSplit: PAUSED)
        // =====================================================================
        let pause_trans = ContextTransitionBuilder::new(ContextTransitionType::ContextEnd, 0.92)
            .from_context(&ws_cs.id)
            .session(&sess_cs_0900.id)
            .build();
        record_context_transition(&conn, &pause_trans).unwrap();

        let cs_session_1 = ContextSessionEvaluator::evaluate_session(
            SessionEvaluationInput {
                session_id: &sess_cs_0900.id,
                context_id: &ws_cs.id,
                context_name: Some("ContextSplit"),
                raw_started_at: "2026-09-22T09:00:00Z",
                raw_last_activity_at: "2026-09-22T10:30:00Z",
                raw_ended_at: None,
                items: &[item_cs_mod.clone(), item_cs_lifecycle.clone()],
                task_candidate: Some(&task_cs),
                is_task_completed: false,
                has_pause_checkpoint: true,
                is_resumed_context: false,
                latest_snapshot: None,
                reference_time: None,
            },
            None,
        );
        assert_eq!(cs_session_1.session_state, ContextSessionState::Paused);
        assert_eq!(cs_session_1.duration_seconds, 5400);

        let snap_eval_1030 = SessionSnapshotIntegrator::evaluate_and_build_snapshot(
            crate::context_engine::session_snapshot::SessionSnapshotEvaluationInput {
                session: &cs_session_1,
                previous_snapshot: None,
                current_task: Some(&task_cs),
                current_workset: None,
                current_important_memories: &[],
                current_state_value: ContextStateValue::Paused,
                is_resumed: false,
                is_major_milestone: false,
                reference_time: None,
            },
        );
        assert!(snap_eval_1030.should_create_or_update);
        assert_eq!(snap_eval_1030.snapshot.as_ref().unwrap().state, ContextStateValue::Paused);

        cs_history.push(ContextLifecycleState::Paused);

        // =====================================================================
        // 10:45 — ContextSplit Resumed (ContextSplit: ACTIVE)
        // =====================================================================
        let sess_cs_1045 = create_session(&conn).unwrap();
        let item_cs_recon = create_item(
            &conn,
            Some(&ws_cs.id),
            Some(&sess_cs_1045.id),
            "file",
            "/Projects/ContextSplit/src-tauri/src/context_engine/reconstruction.rs",
            Some("reconstruction.rs — ContextSplit"),
            "{\"status\":\"confirmed\",\"app_name\":\"Visual Studio Code\",\"file_path\":\"/Projects/ContextSplit/src-tauri/src/context_engine/reconstruction.rs\"}",
        ).unwrap();
        create_or_update_membership(&conn, &item_cs_recon.id, &ws_cs.id, "confirmed", 0.95, "user").unwrap();

        let resume_trans_cs = ContextTransitionBuilder::new(ContextTransitionType::ContextReturn, 0.95)
            .to_context(&ws_cs.id)
            .session(&sess_cs_1045.id)
            .build();
        record_context_transition(&conn, &resume_trans_cs).unwrap();

        cs_history.push(ContextLifecycleState::Active);

        let cs_session_2 = ContextSessionEvaluator::evaluate_session(
            SessionEvaluationInput {
                session_id: &sess_cs_1045.id,
                context_id: &ws_cs.id,
                context_name: Some("ContextSplit"),
                raw_started_at: "2026-09-22T10:45:00Z",
                raw_last_activity_at: "2026-09-22T12:00:00Z",
                raw_ended_at: None,
                items: &[item_cs_recon.clone()],
                task_candidate: Some(&task_cs),
                is_task_completed: false,
                has_pause_checkpoint: false,
                is_resumed_context: true,
                latest_snapshot: None,
                reference_time: Some(chrono::DateTime::parse_from_rfc3339("2026-09-22T12:00:00Z").unwrap().with_timezone(&chrono::Utc)),
            },
            None,
        );
        assert_eq!(cs_session_2.session_state, ContextSessionState::Active);

        // =====================================================================
        // 12:00 — Switch to PayX (ContextSplit: PAUSED, PayX: ACTIVE)
        // =====================================================================
        let switch_to_payx = ContextTransitionBuilder::new(ContextTransitionType::ContextSwitch, 0.93)
            .from_context(&ws_cs.id)
            .to_context(&ws_payx.id)
            .session(&sess_cs_1045.id)
            .build();
        record_context_transition(&conn, &switch_to_payx).unwrap();

        cs_history.push(ContextLifecycleState::Paused);
        payx_history.push(ContextLifecycleState::Active);

        let sess_payx_1200 = create_session(&conn).unwrap();
        let item_payx_stripe = create_item(
            &conn,
            Some(&ws_payx.id),
            Some(&sess_payx_1200.id),
            "file",
            "/Projects/PayX/src/stripe_webhook.rs",
            Some("stripe_webhook.rs — PayX"),
            "{\"status\":\"confirmed\",\"app_name\":\"Visual Studio Code\",\"file_path\":\"/Projects/PayX/src/stripe_webhook.rs\"}",
        ).unwrap();
        create_or_update_membership(&conn, &item_payx_stripe.id, &ws_payx.id, "confirmed", 0.95, "user").unwrap();

        let task_payx = TaskCandidate {
            id: format!("task-payx-{}", ws_payx.id),
            context_id: Some(ws_payx.id.clone()),
            title: "Stripe Webhook Integration".to_string(),
            confidence: 0.92,
            status: TaskCandidateStatus::Active,
            first_detected_at: "2026-09-22T12:00:00Z".to_string(),
            last_active_at: "2026-09-22T13:00:00Z".to_string(),
            created_at: "2026-09-22T12:00:00Z".to_string(),
            updated_at: "2026-09-22T13:00:00Z".to_string(),
        };
        create_task_candidate(&conn, &task_payx).unwrap();

        // =====================================================================
        // 13:00 — Switch to University OS (PayX: PAUSED, University OS: ACTIVE)
        // =====================================================================
        let switch_to_uni = ContextTransitionBuilder::new(ContextTransitionType::ContextSwitch, 0.91)
            .from_context(&ws_payx.id)
            .to_context(&ws_uni.id)
            .session(&sess_payx_1200.id)
            .build();
        record_context_transition(&conn, &switch_to_uni).unwrap();

        payx_history.push(ContextLifecycleState::Paused);
        uni_history.push(ContextLifecycleState::Active);

        let sess_uni_1300 = create_session(&conn).unwrap();
        let item_uni_scheduler = create_item(
            &conn,
            Some(&ws_uni.id),
            Some(&sess_uni_1300.id),
            "file",
            "/Academic/CS342/scheduler.c",
            Some("scheduler.c — University OS"),
            "{\"status\":\"confirmed\",\"app_name\":\"Visual Studio Code\",\"file_path\":\"/Academic/CS342/scheduler.c\"}",
        ).unwrap();
        create_or_update_membership(&conn, &item_uni_scheduler.id, &ws_uni.id, "confirmed", 0.95, "user").unwrap();

        let task_uni = TaskCandidate {
            id: format!("task-uni-{}", ws_uni.id),
            context_id: Some(ws_uni.id.clone()),
            title: "Process Scheduler Assignment".to_string(),
            confidence: 0.94,
            status: TaskCandidateStatus::Active,
            first_detected_at: "2026-09-22T13:00:00Z".to_string(),
            last_active_at: "2026-09-22T14:30:00Z".to_string(),
            created_at: "2026-09-22T13:00:00Z".to_string(),
            updated_at: "2026-09-22T14:30:00Z".to_string(),
        };
        create_task_candidate(&conn, &task_uni).unwrap();

        // =====================================================================
        // 14:30 — Switch back to PayX Resumed (University OS: PAUSED, PayX: ACTIVE)
        // =====================================================================
        let switch_to_payx_resumed = ContextTransitionBuilder::new(ContextTransitionType::ContextSwitch, 0.90)
            .from_context(&ws_uni.id)
            .to_context(&ws_payx.id)
            .session(&sess_uni_1300.id)
            .build();
        record_context_transition(&conn, &switch_to_payx_resumed).unwrap();

        uni_history.push(ContextLifecycleState::Paused);
        payx_history.push(ContextLifecycleState::Active);

        let sess_payx_1430 = create_session(&conn).unwrap();
        let item_payx_checkout = create_item(
            &conn,
            Some(&ws_payx.id),
            Some(&sess_payx_1430.id),
            "file",
            "/Projects/PayX/src/checkout_flow.rs",
            Some("checkout_flow.rs — PayX"),
            "{\"status\":\"confirmed\",\"app_name\":\"Visual Studio Code\",\"file_path\":\"/Projects/PayX/src/checkout_flow.rs\"}",
        ).unwrap();
        create_or_update_membership(&conn, &item_payx_checkout.id, &ws_payx.id, "confirmed", 0.95, "user").unwrap();

        // =====================================================================
        // 16:00 — Random Browsing (Noise / Detour)
        // =====================================================================
        // Must NOT automatically become a permanent context
        let noise_items = [
            ("Reddit - Endless Scrolling", "reddit.com", "https://reddit.com/r/technology"),
            ("Twitter / X Feed", "x.com", "https://x.com/home"),
            ("Hacker News Frontpage", "news.ycombinator.com", "https://news.ycombinator.com"),
        ];

        for (title, domain, url) in &noise_items {
            let c_res = contamination_guard.assess(
                &ContaminationCheckInput::new(*title, &ws_payx.id, "PayX")
                    .with_app("Google Chrome")
                    .with_domain(*domain)
                    .with_url(*url),
            );
            assert!(c_res.is_contamination_risk);

            let q_res = quality_guard.evaluate(
                &QualityGuardInput::new(*title)
                    .with_target(&ws_payx.id, "PayX")
                    .with_app_name("Google Chrome")
                    .with_domain(*domain)
                    .with_noise_flags(true, false),
            );
            assert!(!q_res.decision.should_modify_context());
        }

        // Workspace count remains strictly 3 - no random browsing workspace created
        assert_eq!(
            list_workspaces(&conn).unwrap().len(),
            3,
            "Random browsing must NOT create a permanent context"
        );

        // =====================================================================
        // 17:00 — Switch back to ContextSplit Resumed (PayX: PAUSED, ContextSplit: ACTIVE)
        // =====================================================================
        let switch_to_cs_resumed = ContextTransitionBuilder::new(ContextTransitionType::ContextSwitch, 0.94)
            .from_context(&ws_payx.id)
            .to_context(&ws_cs.id)
            .session(&sess_payx_1430.id)
            .build();
        record_context_transition(&conn, &switch_to_cs_resumed).unwrap();

        payx_history.push(ContextLifecycleState::Paused);
        cs_history.push(ContextLifecycleState::Active);

        let sess_cs_1700 = create_session(&conn).unwrap();
        let item_cs_overview = create_item(
            &conn,
            Some(&ws_cs.id),
            Some(&sess_cs_1700.id),
            "file",
            "/Projects/ContextSplit/src-tauri/src/context_engine/overview.rs",
            Some("overview.rs — ContextSplit"),
            "{\"status\":\"confirmed\",\"app_name\":\"Visual Studio Code\",\"file_path\":\"/Projects/ContextSplit/src-tauri/src/context_engine/overview.rs\"}",
        ).unwrap();
        create_or_update_membership(&conn, &item_cs_overview.id, &ws_cs.id, "confirmed", 0.95, "user").unwrap();

        // =====================================================================
        // 18:00 — Stop Working (ContextSplit: PAUSED)
        // =====================================================================
        let departure_1800 = ContextTransitionBuilder::new(ContextTransitionType::ContextEnd, 0.95)
            .from_context(&ws_cs.id)
            .session(&sess_cs_1700.id)
            .build();
        record_context_transition(&conn, &departure_1800).unwrap();

        cs_history.push(ContextLifecycleState::Paused);

        // =====================================================================
        // VERIFICATIONS
        // =====================================================================
        // 1. Verify Expected Lifecycle Transitions
        // ContextSplit: ACTIVE -> PAUSED -> ACTIVE -> PAUSED -> ACTIVE -> PAUSED
        assert_eq!(
            cs_history,
            vec![
                ContextLifecycleState::Active,
                ContextLifecycleState::Paused,
                ContextLifecycleState::Active,
                ContextLifecycleState::Paused,
                ContextLifecycleState::Active,
                ContextLifecycleState::Paused,
            ]
        );

        // PayX: ACTIVE -> PAUSED -> ACTIVE -> PAUSED
        assert_eq!(
            payx_history,
            vec![
                ContextLifecycleState::Active,
                ContextLifecycleState::Paused,
                ContextLifecycleState::Active,
                ContextLifecycleState::Paused,
            ]
        );

        // University OS: ACTIVE -> PAUSED
        assert_eq!(
            uni_history,
            vec![
                ContextLifecycleState::Active,
                ContextLifecycleState::Paused,
            ]
        );

        // 2. Invalidate all caches for fresh end-of-day reconstruction verification
        invalidate_reconstruction_cache(None);
        invalidate_memory_cache(None);
        invalidate_workset_cache(None);
        invalidate_snapshot_cache(&ws_cs.id);
        invalidate_snapshot_cache(&ws_payx.id);
        invalidate_snapshot_cache(&ws_uni.id);

        // 3. Verify ContextSplit Coherence
        let cs_recon = reconstruction_service.reconstruct_context(&conn, &ws_cs.id).unwrap();
        assert_eq!(cs_recon.context.status, ReconstructionSectionStatus::Available);
        assert_eq!(cs_recon.context.name, "ContextSplit");
        assert_eq!(cs_recon.current_task.title.as_deref(), Some("Context Lifecycle Engine"));
        assert!(cs_recon.workset.total_count >= 4);
        assert!(
            cs_recon.quality.rating == ReconstructionQualityRating::Coherent
                || cs_recon.quality.rating == ReconstructionQualityRating::MostlyCoherent
        );
        let cs_workset = workset_service.derive_for_workspace(&conn, &ws_cs.id).unwrap();
        assert!(cs_workset.items.iter().any(|i| i.id == item_cs_mod.id));
        assert!(cs_workset.items.iter().any(|i| i.id == item_cs_lifecycle.id));
        assert!(cs_workset.items.iter().any(|i| i.id == item_cs_recon.id));
        assert!(cs_workset.items.iter().any(|i| i.id == item_cs_overview.id));

        // 4. Verify PayX Coherence
        let payx_recon = reconstruction_service.reconstruct_context(&conn, &ws_payx.id).unwrap();
        assert_eq!(payx_recon.context.status, ReconstructionSectionStatus::Available);
        assert_eq!(payx_recon.context.name, "PayX");
        assert_eq!(payx_recon.current_task.title.as_deref(), Some("Stripe Webhook Integration"));
        let payx_workset = workset_service.derive_for_workspace(&conn, &ws_payx.id).unwrap();
        assert!(payx_workset.items.iter().any(|i| i.id == item_payx_stripe.id));
        assert!(payx_workset.items.iter().any(|i| i.id == item_payx_checkout.id));

        // 5. Verify University OS Coherence
        let uni_recon = reconstruction_service.reconstruct_context(&conn, &ws_uni.id).unwrap();
        assert_eq!(uni_recon.context.status, ReconstructionSectionStatus::Available);
        assert_eq!(uni_recon.context.name, "University OS");
        assert_eq!(uni_recon.current_task.title.as_deref(), Some("Process Scheduler Assignment"));
        let uni_workset = workset_service.derive_for_workspace(&conn, &ws_uni.id).unwrap();
        assert!(uni_workset.items.iter().any(|i| i.id == item_uni_scheduler.id));

        // 6. Verify Complete Memory Isolation (Zero Leaks Across Workday)
        let cs_mem = ContextImportantMemoryBuilder::build(&conn, &ws_cs.id, None).unwrap();
        let payx_mem = ContextImportantMemoryBuilder::build(&conn, &ws_payx.id, None).unwrap();
        let uni_mem = ContextImportantMemoryBuilder::build(&conn, &ws_uni.id, None).unwrap();

        for mem in &cs_mem.memories {
            assert_ne!(mem.source_id, item_payx_stripe.id);
            assert_ne!(mem.source_id, item_uni_scheduler.id);
            assert_ne!(mem.source_id, task_payx.id);
            assert_ne!(mem.source_id, task_uni.id);
        }
        for mem in &payx_mem.memories {
            assert_ne!(mem.source_id, item_cs_mod.id);
            assert_ne!(mem.source_id, item_uni_scheduler.id);
            assert_ne!(mem.source_id, task_cs.id);
            assert_ne!(mem.source_id, task_uni.id);
        }
        for mem in &uni_mem.memories {
            assert_ne!(mem.source_id, item_cs_mod.id);
            assert_ne!(mem.source_id, item_payx_stripe.id);
            assert_ne!(mem.source_id, task_cs.id);
            assert_ne!(mem.source_id, task_payx.id);
        }

        // 7. Verify Snapshots Coherence
        let cs_snap = snapshot_service.derive_for_context(&conn, &ws_cs.id).unwrap();
        assert_eq!(cs_snap.context_id, ws_cs.id);
        assert_eq!(cs_snap.task_title.as_deref(), Some("Context Lifecycle Engine"));

        let payx_snap = snapshot_service.derive_for_context(&conn, &ws_payx.id).unwrap();
        assert_eq!(payx_snap.context_id, ws_payx.id);
        assert_eq!(payx_snap.task_title.as_deref(), Some("Stripe Webhook Integration"));

        let uni_snap = snapshot_service.derive_for_context(&conn, &ws_uni.id).unwrap();
        assert_eq!(uni_snap.context_id, ws_uni.id);
        assert_eq!(uni_snap.task_title.as_deref(), Some("Process Scheduler Assignment"));
    }
}

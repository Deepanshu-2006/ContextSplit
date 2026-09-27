// src-tauri/src/context_engine/task_reconstruction_quality_tests.rs
//
// Regression tests for task reconstruction quality (Step 10):
// - Verifies correct context assignment for tasks.
// - Rejects overly generic task labels (e.g. "Work", "Coding", "Google Search").
// - Verifies actionable deliverable and action verb extraction.
// - Ensures task continuity prevents duplicate task candidates.
// - Verifies relationship between active tasks, worksets, and important memories.

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use crate::context_engine::important_memory::{
        ContextImportantMemoryBuilder, ImportantMemoryType,
    };
    use crate::context_engine::task_extractor::{
        humanize_identifier, is_generic_task_name,
    };
    use crate::context_engine::task_transitions::TaskSnapshot;
    use crate::context_engine::workset::ContextWorksetBuilder;
    use crate::db::operations::{
        confirm_membership, create_item, create_session, create_task_candidate, create_workspace,
    };
    use crate::db::run_migrations;

    fn create_test_db() -> rusqlite::Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_generic_task_rejection_and_specialization() {
        // Overly generic task names must be flagged as generic
        assert!(is_generic_task_name("Work"));
        assert!(is_generic_task_name("Coding"));
        assert!(is_generic_task_name("Google Search"));
        assert!(is_generic_task_name("Spotify"));
        assert!(is_generic_task_name("General Development"));
        assert!(is_generic_task_name("Untitled Document"));
        assert!(is_generic_task_name("New Tab"));
        assert!(is_generic_task_name("Web Browsing"));

        // Specific, actionable task names must NOT be flagged as generic
        assert!(!is_generic_task_name("Implementing JWT Auth Service"));
        assert!(!is_generic_task_name("Fixing Stripe Webhook Signatures"));
        assert!(!is_generic_task_name("Operating Systems Virtual Memory Lab"));
        assert!(!is_generic_task_name("Refactoring Context Transition Pipeline"));
    }

    #[test]
    fn test_task_identifier_humanization() {
        assert_eq!(humanize_identifier("payment_gateway_controller"), "Payment Gateway Controller");
        assert_eq!(humanize_identifier("virtual_memory_paging"), "Virtual Memory Paging");
        assert_eq!(humanize_identifier("dragTracker"), "Drag Tracker");
        assert_eq!(humanize_identifier("ContextTransitionDetector"), "Context Transition Detector");
    }

    #[test]
    fn test_task_continuity_prevents_duplicate_candidates() {
        let mut snapshot = TaskSnapshot::new(
            "Fixing Stripe Webhook Handler",
            Some("Fixing".to_string()),
            Some("Stripe Webhook Handler".to_string()),
            0.90,
            "2026-09-28T10:00:00Z",
            Some("ws_payx".to_string()),
        );

        // Same task continuing
        assert!(snapshot.is_same_task("Fixing Stripe Webhook Handler", Some("Stripe Webhook Handler")));
        assert!(snapshot.is_same_task("fixing stripe webhook handler", Some("stripe webhook handler")));

        snapshot.touch("2026-09-28T10:05:00Z", Some("item_1"), 0.95);
        assert_eq!(snapshot.streak_count, 2);
        assert_eq!(snapshot.last_seen_at, "2026-09-28T10:05:00Z");

        // Distinct task must NOT match
        assert!(!snapshot.is_same_task("Implementing Apple Pay Integration", Some("Apple Pay")));
    }

    #[test]
    fn test_task_alignment_with_workset_and_memory() {
        let conn = create_test_db();
        let ws = create_workspace(&conn, "PayX Development").unwrap();
        let session = create_session(&conn).unwrap();

        // 1. Create a confirmed active task candidate
        let task = crate::db::models::TaskCandidate::new(
            Some(ws.id.clone()),
            "Fixing Stripe Webhook Signatures",
            0.92,
        );
        let created_task = create_task_candidate(&conn, &task).unwrap();

        // Confirm task candidate status
        crate::db::operations::update_task_candidate_status(
            &conn,
            &created_task.id,
            crate::db::models::TaskCandidateStatus::Confirmed,
        )
        .unwrap();

        // 2. Create and associate an item
        let item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "code",
            "pub fn verify_stripe_webhook_signature() -> bool { true }",
            Some("signature_verification.rs"),
            "{}",
        )
        .unwrap();
        confirm_membership(&conn, &item.id, &ws.id).unwrap();

        // 3. Derive important memories
        let memory_set = ContextImportantMemoryBuilder::build(&conn, &ws.id, None).unwrap();

        // Active task must be present in important memory set
        let active_task_mem = memory_set
            .memories
            .iter()
            .find(|m| m.memory_type == ImportantMemoryType::ActiveTask);

        assert!(active_task_mem.is_some(), "Active task must be reflected in context memory");
        let task_mem = active_task_mem.unwrap();
        assert_eq!(task_mem.title, "Fixing Stripe Webhook Signatures");
        assert_eq!(task_mem.source_id, created_task.id);

        // 4. Derive active workset
        let workset = ContextWorksetBuilder::from_db(&conn, &ws.id).unwrap().build().unwrap();
        assert!(!workset.items.is_empty());
        assert!(workset.items.iter().any(|i| i.id == item.id));
    }
}

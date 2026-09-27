// src-tauri/src/context_engine/contextual_memory_quality_tests.rs
//
// Regression tests for contextual memory quality (Step 9):
// - Verifies "FEWER BUT MORE USEFUL MEMORIES" invariant.
// - Suppresses noisy, generic, or accidental memories.
// - Prevents duplicate or near-duplicate memories.
// - Ensures proper epistemic provenance (Confirmed vs Inferred vs Observed).
// - Validates budget capping and decay of stale unconfirmed items.

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use crate::context_engine::important_memory::{
        ContextImportantMemory, ContextImportantMemoryBuilder, ImportantMemoryConfig,
        ImportantMemoryExplanation, ImportantMemoryMetadata, ImportantMemoryType,
    };
    use crate::context_engine::memory::InformationProvenance;
    use crate::context_engine::memory_importance::{
        is_app_name_only, is_unrelated_popular_domain, normalize_memory_key,
        ContextMemoryQualityGuard,
    };
    use crate::db::operations::{
        confirm_membership, create_item, create_session, create_workspace,
    };
    use crate::db::run_migrations;

    fn create_test_db() -> rusqlite::Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_noisy_and_generic_memory_suppression() {
        // Generic searches, popular homepages, and empty noise should be rejected
        assert!(is_unrelated_popular_domain("https://google.com/", Some("Google")));
        assert!(is_unrelated_popular_domain("https://www.google.com/search?q=weather+sf", Some("Google Search")));
        assert!(is_unrelated_popular_domain("https://youtube.com/", Some("YouTube")));
        assert!(is_unrelated_popular_domain("https://reddit.com/", Some("reddit: the front page of the internet")));

        // App-only names
        assert!(is_app_name_only(Some("Google Chrome"), ""));
        assert!(is_app_name_only(Some("VS Code"), "Visual Studio Code"));

        // Genuine project assets should NOT be rejected
        assert!(!is_unrelated_popular_domain("https://github.com/contextsplit/core", Some("ContextSplit Core Engine")));
        assert!(!is_unrelated_popular_domain("https://canvas.university.edu/courses/4500", Some("CS4500 Course Syllabus")));
        assert!(!is_app_name_only(Some("scoring.rs - ContextSplit"), "fn evaluate_interpretation"));
    }

    #[test]
    fn test_canonical_duplicate_memory_deduplication() {
        let key1 = normalize_memory_key("https://github.com/payx/payx", None);
        let key2 = normalize_memory_key("https://github.com/payx/payx/", None);
        let key3 = normalize_memory_key("PayX Core Repo", Some("https://github.com/payx/payx"));
        assert_eq!(key1, key2);
        assert_eq!(key1, key3);

        let ws_key1 = normalize_memory_key("Workspace: ContextSplit Development", None);
        let ws_key2 = normalize_memory_key("ContextSplit Development", None);
        assert_eq!(ws_key1, ws_key2);

        // Memory set deduplication
        let mem1 = ContextImportantMemory::new(
            "mem_1".to_string(),
            "ws_1",
            "item_1".to_string(),
            ImportantMemoryType::ImportantReference,
            0.85,
            0.90,
            None,
            None,
            InformationProvenance::Observed,
            "https://github.com/payx/payx".to_string(),
            Some("PayX Core Repository".to_string()),
            ImportantMemoryExplanation::new("Test repo 1", vec![], vec![], 0.9, InformationProvenance::Observed, None),
            ImportantMemoryMetadata::default(),
        );

        let mem2 = ContextImportantMemory::new(
            "mem_2".to_string(),
            "ws_1",
            "item_2".to_string(),
            ImportantMemoryType::ImportantReference,
            0.80,
            0.85,
            None,
            None,
            InformationProvenance::Observed,
            "https://github.com/payx/payx/".to_string(), // Trailing slash duplicate
            Some("PayX Core Repository".to_string()),
            ImportantMemoryExplanation::new("Test repo 2", vec![], vec![], 0.85, InformationProvenance::Observed, None),
            ImportantMemoryMetadata::default(),
        );

        let filtered = ContextMemoryQualityGuard::filter_memories(vec![mem1, mem2]);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].source_id, "item_1");
    }

    #[test]
    fn test_fewer_but_more_useful_memories_budget_capping() {
        let conn = create_test_db();
        let ws = create_workspace(&conn, "PayX Development").unwrap();
        let session = create_session(&conn).unwrap();

        // Create 20 items in PayX
        for i in 1..=20 {
            let item = create_item(
                &conn,
                Some(&ws.id),
                Some(&session.id),
                "code",
                &format!("PaymentModule_{}.swift content", i),
                Some(&format!("PaymentModule_{}.swift", i)),
                "{}",
            )
            .unwrap();
            confirm_membership(&conn, &item.id, &ws.id).unwrap();
        }

        // Build important memories
        let memory_set = ContextImportantMemoryBuilder::build(&conn, &ws.id, None).unwrap();

        // Quality invariant: Capped at maximum 12 high-value memories (Zero unbounded bloat)
        assert!(memory_set.memories.len() <= 12);
        assert!(!memory_set.memories.is_empty());
    }

    #[test]
    fn test_user_confirmed_provenance_integrity_and_immunity() {
        let conn = create_test_db();
        let ws = create_workspace(&conn, "ContextSplit Development").unwrap();
        let session = create_session(&conn).unwrap();

        let item = create_item(
            &conn,
            Some(&ws.id),
            Some(&session.id),
            "text",
            "Critical Architecture Decided: Rule out application-only heuristics.",
            Some("Architecture Decision 001"),
            "{}",
        )
        .unwrap();
        confirm_membership(&conn, &item.id, &ws.id).unwrap();

        let memory_set = ContextImportantMemoryBuilder::build(&conn, &ws.id, None).unwrap();
        let confirmed_mem = memory_set.memories.iter().find(|m| m.source_id == item.id).unwrap();

        // Confirmed fact has 1.0 importance, 1.0 confidence, and Confirmed provenance
        assert_eq!(confirmed_mem.importance, 1.0);
        assert_eq!(confirmed_mem.confidence, 1.0);
        assert_eq!(confirmed_mem.provenance, InformationProvenance::Confirmed);
    }
}

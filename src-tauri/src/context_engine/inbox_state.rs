use crate::db::models::{InboxItemState, InboxState};
use crate::db::operations::{
    create_inbox_item_state, get_inbox_item_state, transition_inbox_item_state,
    list_inbox_item_states,
};
use rusqlite::Connection;

/// High-level coordinator for Context-Aware Inbox State Machine.
/// Guarantees that inbox states are strictly separated from item data and workspace memberships.
pub struct InboxStateMachine;

impl InboxStateMachine {
    /// Initializes a newly captured item in the inbox with state 'New'.
    pub fn init_item(conn: &Connection, item_id: &str) -> Result<InboxItemState, String> {
        create_inbox_item_state(conn, item_id, InboxState::New, Some("Newly captured inbox item"))
            .map_err(|e| e.to_string())
    }

    /// Gets current inbox item state.
    pub fn get_state(conn: &Connection, item_id: &str) -> Result<Option<InboxItemState>, String> {
        get_inbox_item_state(conn, item_id).map_err(|e| e.to_string())
    }

    /// Transitions item into 'Processing' when context engine / background queue starts analysis.
    pub fn start_processing(conn: &Connection, item_id: &str) -> Result<InboxItemState, String> {
        transition_inbox_item_state(
            conn,
            item_id,
            InboxState::Processing,
            Some("Context Engine background processing started"),
        )
    }

    /// Transitions item into 'Understood' when features, signals, and embeddings are extracted.
    pub fn mark_understood(
        conn: &Connection,
        item_id: &str,
        reason: Option<&str>,
    ) -> Result<InboxItemState, String> {
        transition_inbox_item_state(
            conn,
            item_id,
            InboxState::Understood,
            reason.or(Some("Context features and relationships extracted")),
        )
    }

    /// Transitions item into 'Suggested' when a confident workspace candidate is identified.
    pub fn mark_suggested(
        conn: &Connection,
        item_id: &str,
        workspace_id: &str,
        confidence: f64,
        reason: Option<&str>,
    ) -> Result<InboxItemState, String> {
        let default_reason = format!(
            "Suggested workspace '{}' with {:.1}% confidence",
            workspace_id,
            confidence * 100.0
        );
        transition_inbox_item_state(
            conn,
            item_id,
            InboxState::Suggested,
            Some(reason.unwrap_or(&default_reason)),
        )
    }

    /// Transitions item into 'NeedsReview' when confidence is low or ambiguity exists.
    pub fn mark_needs_review(
        conn: &Connection,
        item_id: &str,
        reason: Option<&str>,
    ) -> Result<InboxItemState, String> {
        transition_inbox_item_state(
            conn,
            item_id,
            InboxState::NeedsReview,
            reason.or(Some("Low confidence or ambiguous context requires user review")),
        )
    }

    /// User action: Confirms/moves item to workspace (transitions to 'Organized').
    /// Invariant: NEVER called automatically by background processing.
    pub fn organize_item(
        conn: &Connection,
        item_id: &str,
        workspace_id: &str,
    ) -> Result<InboxItemState, String> {
        let reason = format!("User filed item to workspace '{}'", workspace_id);
        transition_inbox_item_state(
            conn,
            item_id,
            InboxState::Organized,
            Some(&reason),
        )
    }

    /// User action: Dismisses item from inbox.
    pub fn dismiss_item(
        conn: &Connection,
        item_id: &str,
        reason: Option<&str>,
    ) -> Result<InboxItemState, String> {
        transition_inbox_item_state(
            conn,
            item_id,
            InboxState::Dismissed,
            reason.or(Some("User dismissed item from inbox")),
        )
    }

    /// Reopens an item back to 'New' or 'NeedsReview'.
    pub fn reopen_item(conn: &Connection, item_id: &str) -> Result<InboxItemState, String> {
        transition_inbox_item_state(
            conn,
            item_id,
            InboxState::New,
            Some("Item reopened in inbox"),
        )
    }

    /// Lists items by optional state filter.
    pub fn list_by_state(
        conn: &Connection,
        state_filter: Option<InboxState>,
    ) -> Result<Vec<InboxItemState>, String> {
        list_inbox_item_states(conn, state_filter).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::db::operations::{create_item, create_workspace, get_item};
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_valid_path_new_processing_understood_suggested_organized() {
        let conn = setup_test_db();
        let item = create_item(&conn, None, None, "text", "Design tokens draft", Some("Tokens"), "{}").unwrap();

        // 1. Initial State: New
        let s1 = InboxStateMachine::get_state(&conn, &item.id).unwrap().unwrap();
        assert_eq!(s1.state, InboxState::New);
        assert_eq!(s1.previous_state, None);

        // 2. Transition -> Processing
        let s2 = InboxStateMachine::start_processing(&conn, &item.id).unwrap();
        assert_eq!(s2.state, InboxState::Processing);
        assert_eq!(s2.previous_state, Some(InboxState::New));

        // 3. Transition -> Understood
        let s3 = InboxStateMachine::mark_understood(&conn, &item.id, Some("Extracted design keyword tokens")).unwrap();
        assert_eq!(s3.state, InboxState::Understood);
        assert_eq!(s3.previous_state, Some(InboxState::Processing));

        // 4. Transition -> Suggested
        let ws = create_workspace(&conn, "Design System").unwrap();
        let s4 = InboxStateMachine::mark_suggested(&conn, &item.id, &ws.id, 0.94, None).unwrap();
        assert_eq!(s4.state, InboxState::Suggested);
        assert_eq!(s4.previous_state, Some(InboxState::Understood));
        assert!(s4.reason.unwrap().contains("94.0%"));

        // 5. User accepts suggestion -> Organized
        let s5 = InboxStateMachine::organize_item(&conn, &item.id, &ws.id).unwrap();
        assert_eq!(s5.state, InboxState::Organized);
        assert_eq!(s5.previous_state, Some(InboxState::Suggested));
    }

    #[test]
    fn test_valid_path_new_processing_understood_needs_review() {
        let conn = setup_test_db();
        let item = create_item(&conn, None, None, "text", "Ambiguous snippets", None, "{}").unwrap();

        // New -> Processing -> Understood -> NeedsReview
        InboxStateMachine::start_processing(&conn, &item.id).unwrap();
        InboxStateMachine::mark_understood(&conn, &item.id, None).unwrap();
        
        let review_state = InboxStateMachine::mark_needs_review(
            &conn,
            &item.id,
            Some("No strong workspace affinity found (highest confidence: 24%)"),
        ).unwrap();

        assert_eq!(review_state.state, InboxState::NeedsReview);
        assert_eq!(review_state.previous_state, Some(InboxState::Understood));

        // From NeedsReview, user can manually file item to a workspace
        let ws = create_workspace(&conn, "Random Notes").unwrap();
        let organized_state = InboxStateMachine::organize_item(&conn, &item.id, &ws.id).unwrap();
        assert_eq!(organized_state.state, InboxState::Organized);
        assert_eq!(organized_state.previous_state, Some(InboxState::NeedsReview));
    }

    #[test]
    fn test_valid_path_suggested_dismissed_and_needs_review_dismissed() {
        let conn = setup_test_db();
        let item1 = create_item(&conn, None, None, "text", "Item 1", None, "{}").unwrap();
        let ws = create_workspace(&conn, "Project Alpha").unwrap();

        // New -> Processing -> Understood -> Suggested -> Dismissed
        InboxStateMachine::start_processing(&conn, &item1.id).unwrap();
        InboxStateMachine::mark_understood(&conn, &item1.id, None).unwrap();
        InboxStateMachine::mark_suggested(&conn, &item1.id, &ws.id, 0.88, None).unwrap();
        
        let dismissed1 = InboxStateMachine::dismiss_item(&conn, &item1.id, Some("User declined recommendation")).unwrap();
        assert_eq!(dismissed1.state, InboxState::Dismissed);
        assert_eq!(dismissed1.previous_state, Some(InboxState::Suggested));

        // NeedsReview -> Dismissed
        let item2 = create_item(&conn, None, None, "text", "Item 2", None, "{}").unwrap();
        InboxStateMachine::start_processing(&conn, &item2.id).unwrap();
        InboxStateMachine::mark_needs_review(&conn, &item2.id, None).unwrap();
        
        let dismissed2 = InboxStateMachine::dismiss_item(&conn, &item2.id, None).unwrap();
        assert_eq!(dismissed2.state, InboxState::Dismissed);
        assert_eq!(dismissed2.previous_state, Some(InboxState::NeedsReview));
    }

    #[test]
    fn test_valid_direct_manual_filing_from_new() {
        let conn = setup_test_db();
        let item = create_item(&conn, None, None, "text", "Quick drop", None, "{}").unwrap();
        let ws = create_workspace(&conn, "Target WS").unwrap();

        // User drags/moves item directly before background processing finishes
        let s = InboxStateMachine::organize_item(&conn, &item.id, &ws.id).unwrap();
        assert_eq!(s.state, InboxState::Organized);
        assert_eq!(s.previous_state, Some(InboxState::New));
    }

    #[test]
    fn test_invalid_state_transitions_are_rejected() {
        let conn = setup_test_db();
        let item = create_item(&conn, None, None, "text", "Test item", None, "{}").unwrap();

        // Invalid: New -> Suggested (skipping Processing and Understood)
        let invalid1 = transition_inbox_item_state(&conn, &item.id, InboxState::Suggested, None);
        assert!(invalid1.is_err());
        assert!(invalid1.unwrap_err().contains("Invalid state transition"));

        // Invalid: New -> NeedsReview (skipping Processing and Understood)
        let invalid2 = transition_inbox_item_state(&conn, &item.id, InboxState::NeedsReview, None);
        assert!(invalid2.is_err());

        // Advance to Processing
        InboxStateMachine::start_processing(&conn, &item.id).unwrap();

        // Invalid: Processing -> Suggested (cannot suggest without understanding)
        let invalid3 = transition_inbox_item_state(&conn, &item.id, InboxState::Suggested, None);
        assert!(invalid3.is_err());

        // Advance to Dismissed
        InboxStateMachine::dismiss_item(&conn, &item.id, None).unwrap();

        // Invalid: Dismissed -> Suggested
        let invalid4 = transition_inbox_item_state(&conn, &item.id, InboxState::Suggested, None);
        assert!(invalid4.is_err());
    }

    #[test]
    fn test_state_machine_does_not_duplicate_item_data() {
        let conn = setup_test_db();
        let content = "Sensitive proprietary architecture spec";
        let title = "Architecture Spec";
        let metadata = r#"{"author":"alice","tags":["security","arch"]}"#;

        let item = create_item(&conn, None, None, "text", content, Some(title), metadata).unwrap();
        
        let state = InboxStateMachine::get_state(&conn, &item.id).unwrap().unwrap();
        assert_eq!(state.item_id, item.id);
        assert_eq!(state.state, InboxState::New);

        // Verify items table holds full content
        let fetched_item = get_item(&conn, &item.id).unwrap();
        assert_eq!(fetched_item.content, content);
        assert_eq!(fetched_item.title.as_deref(), Some(title));
        assert_eq!(fetched_item.metadata, metadata);

        // Verify foreign key cascade: deleting item deletes inbox state record
        crate::db::operations::delete_item(&conn, &item.id).unwrap();
        let state_after_delete = InboxStateMachine::get_state(&conn, &item.id).unwrap();
        assert!(state_after_delete.is_none(), "InboxItemState must cascade delete when item is deleted");
    }

    #[test]
    fn test_invariant_never_automatically_organizes_items() {
        let conn = setup_test_db();
        let item = create_item(&conn, None, None, "text", "High match snippet", None, "{}").unwrap();
        let ws = create_workspace(&conn, "Auto Match WS").unwrap();

        // Simulate complete automated background processing pipeline
        InboxStateMachine::start_processing(&conn, &item.id).unwrap();
        InboxStateMachine::mark_understood(&conn, &item.id, Some("Confidence: 99.9%")).unwrap();
        
        // Even with 99.9% confidence, background pipeline MUST only transition to Suggested!
        let suggested = InboxStateMachine::mark_suggested(&conn, &item.id, &ws.id, 0.999, None).unwrap();
        assert_eq!(suggested.state, InboxState::Suggested);
        assert_ne!(suggested.state, InboxState::Organized, "Automated processing MUST NEVER automatically organize items");

        // The item remains in Inbox until explicit user action
        let current = InboxStateMachine::get_state(&conn, &item.id).unwrap().unwrap();
        assert_eq!(current.state, InboxState::Suggested);
    }

    #[test]
    fn test_list_inbox_item_states_by_filter() {
        let conn = setup_test_db();
        let item1 = create_item(&conn, None, None, "text", "Item 1", None, "{}").unwrap();
        let item2 = create_item(&conn, None, None, "text", "Item 2", None, "{}").unwrap();
        let item3 = create_item(&conn, None, None, "text", "Item 3", None, "{}").unwrap();

        InboxStateMachine::start_processing(&conn, &item2.id).unwrap();
        InboxStateMachine::mark_understood(&conn, &item2.id, None).unwrap();
        InboxStateMachine::mark_needs_review(&conn, &item2.id, None).unwrap();

        InboxStateMachine::dismiss_item(&conn, &item3.id, None).unwrap();

        let new_items = InboxStateMachine::list_by_state(&conn, Some(InboxState::New)).unwrap();
        assert_eq!(new_items.len(), 1);
        assert_eq!(new_items[0].item_id, item1.id);

        let review_items = InboxStateMachine::list_by_state(&conn, Some(InboxState::NeedsReview)).unwrap();
        assert_eq!(review_items.len(), 1);
        assert_eq!(review_items[0].item_id, item2.id);

        let dismissed_items = InboxStateMachine::list_by_state(&conn, Some(InboxState::Dismissed)).unwrap();
        assert_eq!(dismissed_items.len(), 1);
        assert_eq!(dismissed_items[0].item_id, item3.id);

        let all_items = InboxStateMachine::list_by_state(&conn, None).unwrap();
        assert_eq!(all_items.len(), 3);
    }

    #[test]
    fn test_understood_state_preserves_inbox_and_does_not_confirm_membership() {
        let conn = setup_test_db();
        let item = create_item(
            &conn,
            None,
            None,
            "text",
            "Understood high confidence documentation draft",
            Some("Doc Draft"),
            "{}",
        )
        .unwrap();

        // 1. Advance to Processing -> Understood
        InboxStateMachine::start_processing(&conn, &item.id).unwrap();
        let understood = InboxStateMachine::mark_understood(
            &conn,
            &item.id,
            Some("ContextSplit reached 96% confidence interpretation"),
        )
        .unwrap();

        assert_eq!(understood.state, InboxState::Understood);

        // 2. Invariant Check: Item MUST remain in Inbox (workspace_id is None / inbox)
        let fresh_item = get_item(&conn, &item.id).unwrap();
        assert_eq!(
            fresh_item.workspace_id, None,
            "Understood item MUST NOT be silently assigned to any workspace"
        );

        // 3. Invariant Check: No confirmed memberships exist for this item
        let memberships = crate::db::operations::list_item_memberships(&conn, &item.id).unwrap();
        assert!(
            memberships.is_empty() || memberships.iter().all(|m| m.membership_type != "confirmed"),
            "Understood item MUST NOT have confirmed memberships without explicit user confirmation"
        );

        // 4. Only explicit user action promotes Understood -> Organized / Confirmed
        let ws = create_workspace(&conn, "Product Docs").unwrap();
        let organized = InboxStateMachine::organize_item(&conn, &item.id, &ws.id).unwrap();
        assert_eq!(organized.state, InboxState::Organized);
        assert_eq!(organized.previous_state, Some(InboxState::Understood));
    }
}

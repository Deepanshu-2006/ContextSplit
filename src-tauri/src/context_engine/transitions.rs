pub use crate::db::models::{ContextTransition, ContextTransitionType};
use rusqlite::Connection;
use serde_json::Value;

/// Builder helper for constructing validated `ContextTransition` instances.
#[derive(Debug, Clone)]
pub struct ContextTransitionBuilder {
    id: Option<String>,
    from_context_id: Option<String>,
    to_context_id: Option<String>,
    transition_type: ContextTransitionType,
    confidence: f64,
    detected_at: Option<String>,
    session_id: Option<String>,
    trigger_item_id: Option<String>,
    trigger_signal_id: Option<String>,
    metadata: Value,
}

impl ContextTransitionBuilder {
    pub fn new(transition_type: ContextTransitionType, confidence: f64) -> Self {
        Self {
            id: None,
            from_context_id: None,
            to_context_id: None,
            transition_type,
            confidence: confidence.clamp(0.0, 1.0),
            detected_at: None,
            session_id: None,
            trigger_item_id: None,
            trigger_signal_id: None,
            metadata: serde_json::json!({}),
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn from_context(mut self, context_id: impl Into<String>) -> Self {
        self.from_context_id = Some(context_id.into());
        self
    }

    pub fn from_context_opt(mut self, context_id: Option<String>) -> Self {
        self.from_context_id = context_id;
        self
    }

    pub fn to_context(mut self, context_id: impl Into<String>) -> Self {
        self.to_context_id = Some(context_id.into());
        self
    }

    pub fn to_context_opt(mut self, context_id: Option<String>) -> Self {
        self.to_context_id = context_id;
        self
    }

    pub fn detected_at(mut self, timestamp: impl Into<String>) -> Self {
        self.detected_at = Some(timestamp.into());
        self
    }

    pub fn session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    pub fn session_opt(mut self, session_id: Option<String>) -> Self {
        self.session_id = session_id;
        self
    }

    pub fn trigger_item(mut self, item_id: impl Into<String>) -> Self {
        self.trigger_item_id = Some(item_id.into());
        self
    }

    pub fn trigger_item_opt(mut self, item_id: Option<String>) -> Self {
        self.trigger_item_id = item_id;
        self
    }

    pub fn trigger_signal(mut self, signal_id: impl Into<String>) -> Self {
        self.trigger_signal_id = Some(signal_id.into());
        self
    }

    pub fn trigger_signal_opt(mut self, signal_id: Option<String>) -> Self {
        self.trigger_signal_id = signal_id;
        self
    }

    pub fn metadata(mut self, metadata: Value) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn build(self) -> ContextTransition {
        let now = chrono::Utc::now().to_rfc3339();
        ContextTransition {
            id: self.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            from_context_id: self.from_context_id,
            to_context_id: self.to_context_id,
            transition_type: self.transition_type,
            confidence: self.confidence,
            detected_at: self.detected_at.unwrap_or_else(|| now.clone()),
            session_id: self.session_id,
            trigger_item_id: self.trigger_item_id,
            trigger_signal_id: self.trigger_signal_id,
            metadata: self.metadata.to_string(),
            created_at: now,
        }
    }
}

/// Formats a human-readable summary of a contextual transition.
pub fn format_transition_summary(
    transition: &ContextTransition,
    from_name: Option<&str>,
    to_name: Option<&str>,
) -> String {
    let from_label = from_name.unwrap_or("Previous Context");
    let to_label = to_name.unwrap_or("New Context");

    match transition.transition_type {
        ContextTransitionType::ContextSwitch => {
            if let (Some(_), Some(_)) = (&transition.from_context_id, &transition.to_context_id) {
                format!("Context switched from '{}' to '{}' ({:.0}% confidence)", from_label, to_label, transition.confidence * 100.0)
            } else {
                format!("Context switched to '{}' ({:.0}% confidence)", to_label, transition.confidence * 100.0)
            }
        }
        ContextTransitionType::ContextReturn => {
            format!("Returned to '{}' after detour ({:.0}% confidence)", to_label, transition.confidence * 100.0)
        }
        ContextTransitionType::ContextStart => {
            format!("Started activity in '{}' ({:.0}% confidence)", to_label, transition.confidence * 100.0)
        }
        ContextTransitionType::ContextEnd => {
            format!("Concluded activity in '{}' ({:.0}% confidence)", from_label, transition.confidence * 100.0)
        }
        ContextTransitionType::AmbiguousTransition => {
            format!("Ambiguous activity shift ({:.0}% confidence)", transition.confidence * 100.0)
        }
    }
}

/// Persists a `ContextTransition` into the SQLite database.
///
/// Invariant: Recording a transition is purely an activity inference and
/// NEVER alters Item or Workspace memberships.
pub fn record_context_transition(
    conn: &Connection,
    transition: &ContextTransition,
) -> Result<ContextTransition, String> {
    crate::db::operations::create_context_transition(conn, transition)
        .map_err(|e| format!("Failed to record context transition: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_transition_types_and_metadata() {
        let types = [
            (ContextTransitionType::ContextSwitch, "context_switch", "Context Switch", true),
            (ContextTransitionType::ContextReturn, "context_return", "Return to Context", true),
            (ContextTransitionType::ContextStart, "context_start", "Context Start", true),
            (ContextTransitionType::ContextEnd, "context_end", "Context End", true),
            (ContextTransitionType::AmbiguousTransition, "ambiguous_transition", "Ambiguous Transition", false),
        ];

        for (t_enum, t_str, display, is_conf) in types {
            assert_eq!(t_enum.as_str(), t_str);
            assert_eq!(t_enum.display_name(), display);
            assert_eq!(t_enum.is_confident(), is_conf);
            assert_eq!(ContextTransitionType::from_str_lenient(t_str), Some(t_enum));
        }
    }

    #[test]
    fn test_transition_builder_and_persistence() {
        let conn = setup_test_db();

        let ws1 = crate::db::operations::create_workspace(&conn, "Design System").unwrap();
        let ws2 = crate::db::operations::create_workspace(&conn, "Backend API").unwrap();
        let session = crate::db::operations::create_session(&conn).unwrap();

        let item = crate::db::operations::create_item(&conn, Some(&ws1.id), Some(&session.id), "url", "https://figma.com", None, "{}").unwrap();

        let transition = ContextTransitionBuilder::new(ContextTransitionType::ContextSwitch, 0.89)
            .from_context(&ws1.id)
            .to_context(&ws2.id)
            .session(&session.id)
            .trigger_item(&item.id)
            .metadata(serde_json::json!({
                "reason": "Switched from Figma to VS Code",
                "prior_streak_count": 4
            }))
            .build();

        assert_eq!(transition.transition_type, ContextTransitionType::ContextSwitch);
        assert_eq!(transition.confidence, 0.89);
        assert_eq!(transition.from_context_id.as_deref(), Some(ws1.id.as_str()));
        assert_eq!(transition.to_context_id.as_deref(), Some(ws2.id.as_str()));
        assert_eq!(transition.session_id.as_deref(), Some(session.id.as_str()));
        assert_eq!(transition.trigger_item_id.as_deref(), Some(item.id.as_str()));

        // Format summary
        let summary = format_transition_summary(&transition, Some(&ws1.name), Some(&ws2.name));
        assert!(summary.contains("Context switched from 'Design System' to 'Backend API'"));

        // Persist
        let recorded = record_context_transition(&conn, &transition).unwrap();
        assert_eq!(recorded.id, transition.id);

        // Verify Invariant: Item workspace membership was NOT changed
        let fresh_item = crate::db::operations::get_item(&conn, &item.id).unwrap();
        assert_eq!(fresh_item.workspace_id.as_deref(), Some(ws1.id.as_str()));
    }

    #[test]
    fn test_format_transition_summaries_all_variants() {
        let t_switch = ContextTransitionBuilder::new(ContextTransitionType::ContextSwitch, 0.90)
            .from_context("ws1")
            .to_context("ws2")
            .build();
        assert_eq!(format_transition_summary(&t_switch, Some("Project A"), Some("Project B")), "Context switched from 'Project A' to 'Project B' (90% confidence)");

        let t_return = ContextTransitionBuilder::new(ContextTransitionType::ContextReturn, 0.85)
            .to_context("ws1")
            .build();
        assert_eq!(format_transition_summary(&t_return, None, Some("Project A")), "Returned to 'Project A' after detour (85% confidence)");

        let t_start = ContextTransitionBuilder::new(ContextTransitionType::ContextStart, 0.95)
            .to_context("ws1")
            .build();
        assert_eq!(format_transition_summary(&t_start, None, Some("Project A")), "Started activity in 'Project A' (95% confidence)");

        let t_end = ContextTransitionBuilder::new(ContextTransitionType::ContextEnd, 1.0)
            .from_context("ws1")
            .build();
        assert_eq!(format_transition_summary(&t_end, Some("Project A"), None), "Concluded activity in 'Project A' (100% confidence)");

        let t_amb = ContextTransitionBuilder::new(ContextTransitionType::AmbiguousTransition, 0.40)
            .build();
        assert_eq!(format_transition_summary(&t_amb, None, None), "Ambiguous activity shift (40% confidence)");
    }
}

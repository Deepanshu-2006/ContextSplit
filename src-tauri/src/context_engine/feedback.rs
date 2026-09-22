use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// =========================================================================
// Feedback Disclaimer & Core Philosophy
// =========================================================================

pub const CONTEXT_FEEDBACK_EVENT_DISCLAIMER: &str =
    "ContextSplit feedback events represent immutable, auditable, and deterministic \
     user actions. They reference existing entity IDs rather than duplicating raw text \
     or private user content, preserving strict privacy and preventing data drift.";

// =========================================================================
// Feedback Actions (Supported Types)
// =========================================================================

/// Supported user feedback actions in ContextSplit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackAction {
    /// Explicit user affirmation attaching an item/activity to a context/workspace.
    ConfirmContext,
    /// Explicit user rejection of a proposed context assignment for an item/activity.
    RejectContext,
    /// User explicitly removing an existing item from a context.
    RemoveFromContext,
    /// User marking an item or memory as high-importance in context.
    MarkImportant,
    /// User removing high-importance status from an item or memory.
    RemoveImportance,
    /// User confirming a task candidate or active task in context.
    ConfirmTask,
    /// User rejecting or dismissing a task candidate.
    RejectTask,
    /// User dismissing an inbox/workspace suggestion or recommendation.
    DismissSuggestion,
    /// User explicitly resuming a paused or dormant context.
    ResumeContext,
    /// User explicitly archiving a context.
    ArchiveContext,
}

impl FeedbackAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ConfirmContext => "confirm_context",
            Self::RejectContext => "reject_context",
            Self::RemoveFromContext => "remove_from_context",
            Self::MarkImportant => "mark_important",
            Self::RemoveImportance => "remove_importance",
            Self::ConfirmTask => "confirm_task",
            Self::RejectTask => "reject_task",
            Self::DismissSuggestion => "dismiss_suggestion",
            Self::ResumeContext => "resume_context",
            Self::ArchiveContext => "archive_context",
        }
    }

    /// User-facing display title for the feedback action.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::ConfirmContext => "Confirm Context",
            Self::RejectContext => "Reject Context",
            Self::RemoveFromContext => "Remove from Context",
            Self::MarkImportant => "Mark as Important",
            Self::RemoveImportance => "Remove Importance",
            Self::ConfirmTask => "Confirm Task",
            Self::RejectTask => "Reject Task",
            Self::DismissSuggestion => "Dismiss Suggestion",
            Self::ResumeContext => "Resume Context",
            Self::ArchiveContext => "Archive Context",
        }
    }

    /// Whether this feedback action is affirmative (positive feedback)
    /// or negative/suppressive (negative feedback).
    pub fn is_positive(&self) -> bool {
        matches!(
            self,
            Self::ConfirmContext
                | Self::MarkImportant
                | Self::ConfirmTask
                | Self::ResumeContext
        )
    }

    /// Whether this action explicitly indicates negative association or dismissal.
    pub fn is_negative(&self) -> bool {
        matches!(
            self,
            Self::RejectContext
                | Self::RemoveFromContext
                | Self::RemoveImportance
                | Self::RejectTask
                | Self::DismissSuggestion
                | Self::ArchiveContext
        )
    }

    /// Parse from snake_case or kebab-case string.
    pub fn from_str_lenient(s: &str) -> Result<Self, String> {
        let norm = s.trim().to_lowercase().replace('-', "_");
        match norm.as_str() {
            "confirm_context" | "confirm" | "confirm_membership" => Ok(Self::ConfirmContext),
            "reject_context" | "reject" | "reject_membership" => Ok(Self::RejectContext),
            "remove_from_context" | "remove" | "unassign" => Ok(Self::RemoveFromContext),
            "mark_important" | "pin_memory" | "set_important" => Ok(Self::MarkImportant),
            "remove_importance" | "unpin_memory" | "unset_important" => Ok(Self::RemoveImportance),
            "confirm_task" | "accept_task" => Ok(Self::ConfirmTask),
            "reject_task" | "dismiss_task" => Ok(Self::RejectTask),
            "dismiss_suggestion" | "dismiss" | "dismiss_inbox" => Ok(Self::DismissSuggestion),
            "resume_context" | "resume" => Ok(Self::ResumeContext),
            "archive_context" | "archive" => Ok(Self::ArchiveContext),
            _ => Err(format!("Unknown feedback action: '{}'", s)),
        }
    }
}

impl std::fmt::Display for FeedbackAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// =========================================================================
// Target References (Auditable entity references)
// =========================================================================

/// Target entity referenced by the feedback event.
/// References existing entity IDs without duplicating private/raw content.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "target_type", content = "target_id", rename_all = "snake_case")]
pub enum FeedbackTargetRef {
    /// Telemetry / Captured item reference (UUID)
    Item(String),
    /// Extracted or candidate task reference
    Task(String),
    /// Workspace / Context reference
    Context(String),
    /// Session reference
    Session(String),
    /// Contextual memory record reference
    Memory(String),
    /// Inbox or recommendation suggestion reference
    Suggestion(String),
    /// Candidate context discovery reference
    Candidate(String),
    /// Custom entity type reference
    Other {
        entity_type: String,
        entity_id: String,
    },
}

impl FeedbackTargetRef {
    pub fn id(&self) -> &str {
        match self {
            Self::Item(id)
            | Self::Task(id)
            | Self::Context(id)
            | Self::Session(id)
            | Self::Memory(id)
            | Self::Suggestion(id)
            | Self::Candidate(id) => id,
            Self::Other { entity_id, .. } => entity_id,
        }
    }

    pub fn entity_type_str(&self) -> &str {
        match self {
            Self::Item(_) => "item",
            Self::Task(_) => "task",
            Self::Context(_) => "context",
            Self::Session(_) => "session",
            Self::Memory(_) => "memory",
            Self::Suggestion(_) => "suggestion",
            Self::Candidate(_) => "candidate",
            Self::Other { entity_type, .. } => entity_type,
        }
    }
}

// =========================================================================
// Feedback Source & Scope
// =========================================================================

/// Origin / UI surface where the feedback originated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackSource {
    /// Direct click in the desktop UI / ContextSplit window.
    UserExplicitUi,
    /// Keyboard shortcut execution.
    ShortcutKey,
    /// Action taken within the Inbox / Suggestion panel.
    InboxAction,
    /// Action taken within the Active Workset tray.
    WorksetTray,
    /// Context menu on an item or tab.
    ContextMenu,
    /// Memory drawer / inspection panel.
    MemoryInspectionPanel,
    /// Interactive system notification action.
    SystemNotification,
    /// Automated test or diagnostic evaluation harness.
    DiagnosticHarness,
}

impl FeedbackSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UserExplicitUi => "user_explicit_ui",
            Self::ShortcutKey => "shortcut_key",
            Self::InboxAction => "inbox_action",
            Self::WorksetTray => "workset_tray",
            Self::ContextMenu => "context_menu",
            Self::MemoryInspectionPanel => "memory_inspection_panel",
            Self::SystemNotification => "system_notification",
            Self::DiagnosticHarness => "diagnostic_harness",
        }
    }
}

impl Default for FeedbackSource {
    fn default() -> Self {
        Self::UserExplicitUi
    }
}

/// Scope of the feedback's effect.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "scope_type", content = "scope_id", rename_all = "snake_case")]
pub enum FeedbackScope {
    /// Feedback applies strictly to the specified workspace / context.
    WorkspaceScoped(String),
    /// Feedback applies to a specific work session.
    SessionScoped(String),
    /// Global system-wide feedback (e.g. system noise filter).
    Global,
}

impl FeedbackScope {
    pub fn is_workspace_scoped(&self) -> bool {
        matches!(self, Self::WorkspaceScoped(_))
    }

    pub fn workspace_id(&self) -> Option<&str> {
        match self {
            Self::WorkspaceScoped(id) => Some(id),
            _ => None,
        }
    }

    pub fn scope_str(&self) -> &'static str {
        match self {
            Self::WorkspaceScoped(_) => "workspace_scoped",
            Self::SessionScoped(_) => "session_scoped",
            Self::Global => "global",
        }
    }
}

pub type FeedbackMetadata = HashMap<String, serde_json::Value>;

// =========================================================================
// Structured Context Feedback Event Model
// =========================================================================

/// Immutable, auditable contextual feedback event.
/// Captures user ground-truth intent deterministically and securely.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextFeedbackEvent {
    /// Unique event identifier (UUID v4)
    pub id: String,
    /// Exact UTC timestamp (RFC 3339 format) when the feedback was registered
    pub timestamp: String,
    /// Feedback action performed
    pub action: FeedbackAction,
    /// Referenced target entity (Item, Task, Context, etc.)
    pub target_ref: FeedbackTargetRef,
    /// Referenced context / workspace ID where applicable
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_ref: Option<String>,
    /// UI surface or interaction channel where feedback originated
    pub source: FeedbackSource,
    /// Scope of impact for this feedback event
    pub scope: FeedbackScope,
    /// Optional human or structured reason (e.g. "Not related to project")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Structured metadata required for deterministic downstream interpretation
    /// (e.g. previous confidence, pattern signature, client context, prior state)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: FeedbackMetadata,
}

impl ContextFeedbackEvent {
    /// Creates a new ContextFeedbackEvent with auto-generated UUID and current UTC timestamp.
    pub fn new(
        action: FeedbackAction,
        target_ref: FeedbackTargetRef,
        context_ref: Option<String>,
        source: FeedbackSource,
        scope: FeedbackScope,
        reason: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now().to_rfc3339(),
            action,
            target_ref,
            context_ref,
            source,
            scope,
            reason,
            metadata: HashMap::new(),
        }
    }

    /// Sets an explicit RFC 3339 timestamp.
    pub fn with_timestamp(mut self, ts: impl Into<String>) -> Self {
        self.timestamp = ts.into();
        self
    }

    /// Parses the timestamp into DateTime<Utc> if valid RFC 3339.
    pub fn timestamp_dt(&self) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(&self.timestamp)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    }

    /// Builder method to attach interpretation metadata.
    pub fn with_metadata<V: Into<serde_json::Value>>(mut self, key: impl Into<String>, value: V) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Builder method to attach multiple metadata key-value pairs.
    pub fn with_metadata_map(mut self, map: HashMap<String, serde_json::Value>) -> Self {
        self.metadata.extend(map);
        self
    }

    // -------------------------------------------------------------------------
    // Factory Helpers for Supported Feedback Types
    // -------------------------------------------------------------------------

    /// Factory: User confirms an item into a context workspace.
    pub fn confirm_context(
        item_id: impl Into<String>,
        workspace_id: impl Into<String>,
        source: FeedbackSource,
    ) -> Self {
        let ws_id = workspace_id.into();
        Self::new(
            FeedbackAction::ConfirmContext,
            FeedbackTargetRef::Item(item_id.into()),
            Some(ws_id.clone()),
            source,
            FeedbackScope::WorkspaceScoped(ws_id),
            Some("User confirmed item context association".to_string()),
        )
    }

    /// Factory: User rejects a suggested context association for an item.
    pub fn reject_context(
        item_id: impl Into<String>,
        workspace_id: impl Into<String>,
        source: FeedbackSource,
        reason: Option<String>,
    ) -> Self {
        let ws_id = workspace_id.into();
        Self::new(
            FeedbackAction::RejectContext,
            FeedbackTargetRef::Item(item_id.into()),
            Some(ws_id.clone()),
            source,
            FeedbackScope::WorkspaceScoped(ws_id),
            reason.or(Some("User rejected context suggestion".to_string())),
        )
    }

    /// Factory: User explicitly removes an existing item from a context workspace.
    pub fn remove_from_context(
        item_id: impl Into<String>,
        workspace_id: impl Into<String>,
        source: FeedbackSource,
        reason: Option<String>,
    ) -> Self {
        let ws_id = workspace_id.into();
        Self::new(
            FeedbackAction::RemoveFromContext,
            FeedbackTargetRef::Item(item_id.into()),
            Some(ws_id.clone()),
            source,
            FeedbackScope::WorkspaceScoped(ws_id),
            reason.or(Some("User removed item from context".to_string())),
        )
    }

    /// Factory: User marks an item or memory as high-importance.
    pub fn mark_important(
        target_ref: FeedbackTargetRef,
        workspace_id: Option<String>,
        source: FeedbackSource,
    ) -> Self {
        let scope = match workspace_id.as_ref() {
            Some(ws) => FeedbackScope::WorkspaceScoped(ws.clone()),
            None => FeedbackScope::Global,
        };
        Self::new(
            FeedbackAction::MarkImportant,
            target_ref,
            workspace_id,
            source,
            scope,
            Some("User marked as important".to_string()),
        )
    }

    /// Factory: User removes importance status from an item or memory.
    pub fn remove_importance(
        target_ref: FeedbackTargetRef,
        workspace_id: Option<String>,
        source: FeedbackSource,
    ) -> Self {
        let scope = match workspace_id.as_ref() {
            Some(ws) => FeedbackScope::WorkspaceScoped(ws.clone()),
            None => FeedbackScope::Global,
        };
        Self::new(
            FeedbackAction::RemoveImportance,
            target_ref,
            workspace_id,
            source,
            scope,
            Some("User removed importance status".to_string()),
        )
    }

    /// Factory: User confirms a task candidate in a context.
    pub fn confirm_task(
        task_id: impl Into<String>,
        workspace_id: impl Into<String>,
        source: FeedbackSource,
    ) -> Self {
        let ws_id = workspace_id.into();
        Self::new(
            FeedbackAction::ConfirmTask,
            FeedbackTargetRef::Task(task_id.into()),
            Some(ws_id.clone()),
            source,
            FeedbackScope::WorkspaceScoped(ws_id),
            Some("User confirmed task in context".to_string()),
        )
    }

    /// Factory: User rejects a task candidate.
    pub fn reject_task(
        task_id: impl Into<String>,
        workspace_id: Option<String>,
        source: FeedbackSource,
        reason: Option<String>,
    ) -> Self {
        let scope = match workspace_id.as_ref() {
            Some(ws) => FeedbackScope::WorkspaceScoped(ws.clone()),
            None => FeedbackScope::Global,
        };
        Self::new(
            FeedbackAction::RejectTask,
            FeedbackTargetRef::Task(task_id.into()),
            workspace_id,
            source,
            scope,
            reason.or(Some("User rejected task candidate".to_string())),
        )
    }

    /// Factory: User dismisses a suggestion or inbox recommendation.
    pub fn dismiss_suggestion(
        suggestion_id: impl Into<String>,
        workspace_id: Option<String>,
        source: FeedbackSource,
        reason: Option<String>,
    ) -> Self {
        let scope = match workspace_id.as_ref() {
            Some(ws) => FeedbackScope::WorkspaceScoped(ws.clone()),
            None => FeedbackScope::Global,
        };
        Self::new(
            FeedbackAction::DismissSuggestion,
            FeedbackTargetRef::Suggestion(suggestion_id.into()),
            workspace_id,
            source,
            scope,
            reason.or(Some("User dismissed suggestion".to_string())),
        )
    }

    /// Factory: User explicitly resumes a context.
    pub fn resume_context(
        workspace_id: impl Into<String>,
        source: FeedbackSource,
    ) -> Self {
        let ws_id = workspace_id.into();
        Self::new(
            FeedbackAction::ResumeContext,
            FeedbackTargetRef::Context(ws_id.clone()),
            Some(ws_id.clone()),
            source,
            FeedbackScope::WorkspaceScoped(ws_id),
            Some("User explicitly resumed context".to_string()),
        )
    }

    /// Factory: User explicitly archives a context.
    pub fn archive_context(
        workspace_id: impl Into<String>,
        source: FeedbackSource,
        reason: Option<String>,
    ) -> Self {
        let ws_id = workspace_id.into();
        Self::new(
            FeedbackAction::ArchiveContext,
            FeedbackTargetRef::Context(ws_id.clone()),
            Some(ws_id.clone()),
            source,
            FeedbackScope::WorkspaceScoped(ws_id),
            reason.or(Some("User archived context".to_string())),
        )
    }

    // -------------------------------------------------------------------------
    // Validation & Audit Formatting
    // -------------------------------------------------------------------------

    /// Validates structural integrity and coherence of the feedback event.
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("Feedback event ID cannot be empty".to_string());
        }
        if self.target_ref.id().trim().is_empty() {
            return Err("Target reference ID cannot be empty".to_string());
        }
        if let FeedbackScope::WorkspaceScoped(ref ws) = self.scope {
            if ws.trim().is_empty() {
                return Err("WorkspaceScoped feedback requires a non-empty workspace ID".to_string());
            }
        }
        Ok(())
    }

    /// Generates a human-readable, deterministic audit summary string for logging and transparency.
    pub fn audit_summary(&self) -> String {
        let target_str = format!("{}:{}", self.target_ref.entity_type_str(), self.target_ref.id());
        let ws_str = self.context_ref.as_deref().unwrap_or("none");
        let reason_str = self.reason.as_deref().unwrap_or("unspecified");
        format!(
            "[{}] Action='{}' Target='{}' Context='{}' Source='{}' Scope='{}' Reason='{}'",
            self.timestamp,
            self.action.as_str(),
            target_str,
            ws_str,
            self.source.as_str(),
            self.scope.scope_str(),
            reason_str
        )
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feedback_action_properties() {
        assert!(FeedbackAction::ConfirmContext.is_positive());
        assert!(!FeedbackAction::ConfirmContext.is_negative());
        assert_eq!(FeedbackAction::ConfirmContext.as_str(), "confirm_context");

        assert!(FeedbackAction::RejectContext.is_negative());
        assert!(!FeedbackAction::RejectContext.is_positive());
        assert_eq!(FeedbackAction::RejectContext.as_str(), "reject_context");

        assert!(FeedbackAction::RemoveFromContext.is_negative());
        assert!(FeedbackAction::MarkImportant.is_positive());
        assert!(FeedbackAction::RemoveImportance.is_negative());
        assert!(FeedbackAction::ConfirmTask.is_positive());
        assert!(FeedbackAction::RejectTask.is_negative());
        assert!(FeedbackAction::DismissSuggestion.is_negative());
        assert!(FeedbackAction::ResumeContext.is_positive());
        assert!(FeedbackAction::ArchiveContext.is_negative());
    }

    #[test]
    fn test_feedback_action_lenient_parsing() {
        assert_eq!(
            FeedbackAction::from_str_lenient("confirm-context").unwrap(),
            FeedbackAction::ConfirmContext
        );
        assert_eq!(
            FeedbackAction::from_str_lenient("remove_from_context").unwrap(),
            FeedbackAction::RemoveFromContext
        );
        assert_eq!(
            FeedbackAction::from_str_lenient("mark_important").unwrap(),
            FeedbackAction::MarkImportant
        );
        assert_eq!(
            FeedbackAction::from_str_lenient("reject_task").unwrap(),
            FeedbackAction::RejectTask
        );
        assert_eq!(
            FeedbackAction::from_str_lenient("dismiss_suggestion").unwrap(),
            FeedbackAction::DismissSuggestion
        );
        assert_eq!(
            FeedbackAction::from_str_lenient("resume_context").unwrap(),
            FeedbackAction::ResumeContext
        );
        assert_eq!(
            FeedbackAction::from_str_lenient("archive_context").unwrap(),
            FeedbackAction::ArchiveContext
        );
        assert!(FeedbackAction::from_str_lenient("invalid_action").is_err());
    }

    #[test]
    fn test_factory_constructors_and_validation() {
        let ev1 = ContextFeedbackEvent::confirm_context("item-123", "ws-payx", FeedbackSource::UserExplicitUi);
        assert_eq!(ev1.action, FeedbackAction::ConfirmContext);
        assert_eq!(ev1.target_ref, FeedbackTargetRef::Item("item-123".to_string()));
        assert_eq!(ev1.context_ref.as_deref(), Some("ws-payx"));
        assert!(ev1.validate().is_ok());

        let ev2 = ContextFeedbackEvent::remove_from_context(
            "item-456",
            "ws-payx",
            FeedbackSource::WorksetTray,
            Some("Accidental capture".to_string()),
        );
        assert_eq!(ev2.action, FeedbackAction::RemoveFromContext);
        assert_eq!(ev2.reason.as_deref(), Some("Accidental capture"));
        assert!(ev2.validate().is_ok());

        let ev3 = ContextFeedbackEvent::mark_important(
            FeedbackTargetRef::Memory("mem-789".to_string()),
            Some("ws-payx".to_string()),
            FeedbackSource::MemoryInspectionPanel,
        );
        assert_eq!(ev3.action, FeedbackAction::MarkImportant);
        assert_eq!(ev3.target_ref, FeedbackTargetRef::Memory("mem-789".to_string()));
        assert!(ev3.validate().is_ok());

        let ev4 = ContextFeedbackEvent::confirm_task("task-001", "ws-payx", FeedbackSource::UserExplicitUi);
        assert_eq!(ev4.action, FeedbackAction::ConfirmTask);
        assert_eq!(ev4.target_ref, FeedbackTargetRef::Task("task-001".to_string()));
        assert!(ev4.validate().is_ok());

        let ev5 = ContextFeedbackEvent::reject_task(
            "task-002",
            Some("ws-payx".to_string()),
            FeedbackSource::InboxAction,
            Some("Not my task".to_string()),
        );
        assert_eq!(ev5.action, FeedbackAction::RejectTask);
        assert!(ev5.validate().is_ok());

        let ev6 = ContextFeedbackEvent::dismiss_suggestion(
            "sugg-001",
            Some("ws-payx".to_string()),
            FeedbackSource::InboxAction,
            None,
        );
        assert_eq!(ev6.action, FeedbackAction::DismissSuggestion);
        assert!(ev6.validate().is_ok());

        let ev7 = ContextFeedbackEvent::resume_context("ws-payx", FeedbackSource::UserExplicitUi);
        assert_eq!(ev7.action, FeedbackAction::ResumeContext);
        assert_eq!(ev7.target_ref, FeedbackTargetRef::Context("ws-payx".to_string()));
        assert!(ev7.validate().is_ok());

        let ev8 = ContextFeedbackEvent::archive_context("ws-old", FeedbackSource::ContextMenu, None);
        assert_eq!(ev8.action, FeedbackAction::ArchiveContext);
        assert!(ev8.validate().is_ok());
    }

    #[test]
    fn test_metadata_attachment_and_serialization() {
        let ev = ContextFeedbackEvent::remove_from_context(
            "item-999",
            "ws-payx",
            FeedbackSource::UserExplicitUi,
            Some("Not related to project".to_string()),
        )
        .with_metadata("candidate_confidence", 0.62)
        .with_metadata("domain", "youtube.com")
        .with_metadata("is_entertainment", true);

        assert_eq!(ev.metadata.get("domain").unwrap(), "youtube.com");
        assert_eq!(ev.metadata.get("candidate_confidence").unwrap(), 0.62);
        assert_eq!(ev.metadata.get("is_entertainment").unwrap(), true);

        // Test serialization and deserialization
        let json_str = serde_json::to_string(&ev).unwrap();
        let deserialized: ContextFeedbackEvent = serde_json::from_str(&json_str).unwrap();

        assert_eq!(deserialized.id, ev.id);
        assert_eq!(deserialized.action, FeedbackAction::RemoveFromContext);
        assert_eq!(deserialized.target_ref, ev.target_ref);
        assert_eq!(deserialized.context_ref, ev.context_ref);
        assert_eq!(deserialized.metadata.get("domain").unwrap(), "youtube.com");
    }

    #[test]
    fn test_audit_summary_format() {
        let ev = ContextFeedbackEvent::confirm_context("item-abc", "ws-123", FeedbackSource::UserExplicitUi);
        let summary = ev.audit_summary();
        assert!(summary.contains("Action='confirm_context'"));
        assert!(summary.contains("Target='item:item-abc'"));
        assert!(summary.contains("Context='ws-123'"));
        assert!(summary.contains("Source='user_explicit_ui'"));
        assert!(summary.contains("Scope='workspace_scoped'"));
    }

    #[test]
    fn test_invalid_event_validation() {
        let mut invalid_ev = ContextFeedbackEvent::confirm_context("item-abc", "ws-123", FeedbackSource::UserExplicitUi);
        invalid_ev.id = "   ".to_string();
        assert!(invalid_ev.validate().is_err());

        let mut invalid_ws = ContextFeedbackEvent::confirm_context("item-abc", "ws-123", FeedbackSource::UserExplicitUi);
        invalid_ws.scope = FeedbackScope::WorkspaceScoped("".to_string());
        assert!(invalid_ws.validate().is_err());
    }
}
